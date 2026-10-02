//! Authenticated relay protocol scheduling over one incremental TLS owner.
//! Replies are bounded control obligations, never a second delivery queue.
use super::{pump::RelayServerPumpV1, *};
use std::collections::VecDeque;

const MAX_REPLY_COUNT: usize = PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_EVENTS_V1;
const MAX_REPLY_BYTES: usize = PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1;

struct Reply {
    opcode: u8,
    payload: Vec<u8>,
    deadline: Instant,
}

#[derive(Default)]
struct Replies {
    queued: VecDeque<Reply>,
    // The pump owns these bytes until its TLS output is fully flushed. Moving
    // a reply out of the FIFO must not turn its capacity into a free credit.
    active: Option<(usize, Instant)>,
    bytes: usize,
}

impl Replies {
    fn push(&mut self, opcode: u8, payload: Vec<u8>, deadline: Instant) -> Result<()> {
        let bytes = self
            .bytes
            .checked_add(payload.len())
            .context("relay reply size overflow")?;
        if self.queued.len() + usize::from(self.active.is_some()) >= MAX_REPLY_COUNT
            || bytes > MAX_REPLY_BYTES
        {
            bail!("relay pending response capacity exceeded");
        }
        if Instant::now() >= deadline {
            bail!("relay response original request deadline exceeded");
        }
        // Charge the retained allocation, not the serializer's spare capacity.
        let payload = payload.into_boxed_slice().into_vec();
        self.queued.push_back(Reply {
            opcode,
            payload,
            deadline,
        });
        self.bytes = bytes;
        Ok(())
    }

    fn wire(&mut self, message: ProductRelayWireMessageV1, deadline: Instant) -> Result<()> {
        self.push(0x2, serde_json::to_vec(&message)?, deadline)
    }

    fn earliest_deadline(&self) -> Option<Instant> {
        self.queued
            .iter()
            .map(|reply| reply.deadline)
            .chain(self.active.map(|(_, deadline)| deadline))
            .min()
    }

    fn check(&self) -> Result<()> {
        if self
            .earliest_deadline()
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            bail!("relay queued response original request deadline exceeded");
        }
        Ok(())
    }

    fn take_next(&mut self) -> Result<Option<Reply>> {
        if self.active.is_some() {
            bail!("relay reply write is still charged");
        }
        let Some(reply) = self.queued.pop_front() else {
            return Ok(None);
        };
        self.active = Some((reply.payload.len(), reply.deadline));
        Ok(Some(reply))
    }

    fn flushed(&mut self) {
        if let Some((bytes, _)) = self.active.take() {
            self.bytes -= bytes;
        }
    }

    fn start(&mut self, pump: &mut RelayServerPumpV1) -> Result<bool> {
        let Some(reply) = self.take_next()? else {
            return Ok(false);
        };
        pump.start_control(reply.opcode, reply.payload, reply.deadline)?;
        Ok(true)
    }
}

enum Incoming {
    Continue,
    Close,
    Rejected(crate::RelayForwardDispositionV1),
}

pub(super) fn run(
    websocket: rustls::StreamOwned<rustls::ServerConnection, ProductRelayDaemonDeadlineTcpStreamV1>,
    context: ProductRelayConnectionLoopV1<'_>,
) -> Result<()> {
    let ProductRelayConnectionLoopV1 {
        manager,
        runtime,
        peer_id,
        session_id,
        inbox,
        stopping,
        ..
    } = context;
    let mut pump = RelayServerPumpV1::new(websocket)?;
    let waker = pump.read_waker();
    let mut replies = Replies::default();
    let mut window = RelayDeliveryWindowV1::default();
    let mut prefer_control = false;
    let mut prefer_reply = true;
    let mut closing = None;
    while !stopping.load(Ordering::Acquire) {
        replies.check()?;
        if closing.is_none()
            && !runtime.block_on(manager.is_current_session(peer_id, session_id, now_ms_v1()))
        {
            bail!("product relay session was replaced, expired, or revoked");
        }

        // No preemption inside a WS frame. Every pump turn still services read
        // and write independently, including while a large delivery is stalled.
        let progress = pump.poll()?;
        if pump.can_write() {
            replies.flushed();
        }
        if progress.delivery_finished {
            // Account this completion before a credit arriving in the same turn.
            window.sent()?;
        }
        if closing.is_none() {
            if let Some((frame, deadline)) = progress.frame {
                match dispatch(
                    frame,
                    deadline,
                    manager,
                    runtime,
                    (peer_id, session_id),
                    &mut replies,
                    &mut window,
                )? {
                    Incoming::Continue => {}
                    Incoming::Close => return Ok(()),
                    Incoming::Rejected(reason) => closing = Some(reason),
                }
            }
        }
        replies.check()?;

        let mut started = false;
        if pump.can_write() {
            // Alternate responses and existing inbox work under continuous
            // traffic. Within each lane FIFO is preserved; no extra delivery
            // reservation is taken until the preceding TLS frame is flushed.
            if prefer_reply || closing.is_some() {
                started = replies.start(&mut pump)?;
                if started {
                    prefer_reply = false;
                }
            }
            if !started && closing.is_none() && window.available() > 0 {
                runtime.block_on(manager.drain_queued_for_session(
                    peer_id,
                    session_id,
                    now_ms_v1(),
                ));
                if let Some(delivery) = next_delivery(inbox, &mut prefer_control) {
                    if !runtime.block_on(manager.is_current_session(
                        peer_id,
                        session_id,
                        now_ms_v1(),
                    )) {
                        bail!("product relay session was replaced before incremental delivery");
                    }
                    pump.start_delivery(delivery)?;
                    prefer_reply = true;
                    started = true;
                }
            }
            if !started {
                started = replies.start(&mut pump)?;
                if started {
                    prefer_reply = false;
                }
            }
        }
        if let Some(reason) = closing {
            if pump.can_write() && replies.queued.is_empty() {
                bail!("product relay rejected authenticated forward: {reason:?}");
            }
        }

        if !progress.progressed && !started {
            if closing.is_none() && pump.can_write() && window.available() > 0 {
                // Do not re-arm a ready inbox while its preceding frame is
                // still writing: that would self-wake a backpressured socket.
                inbox.arm_delivery_wake(&waker);
            }
            pump.wait(replies.earliest_deadline())?;
        }
    }
    Ok(())
}

fn next_delivery(
    inbox: &mut crate::RelaySessionInboxV1,
    prefer_control: &mut bool,
) -> Option<crate::product_relay::RelayEncodedDeliveryV1> {
    let item = if *prefer_control {
        inbox
            .try_recv_peer_handshake_encoded()
            .map(|value| (value, false))
            .or_else(|_| inbox.try_recv_encoded().map(|value| (value, true)))
    } else {
        inbox
            .try_recv_encoded()
            .map(|value| (value, true))
            .or_else(|_| {
                inbox
                    .try_recv_peer_handshake_encoded()
                    .map(|value| (value, false))
            })
    };
    let (delivery, next) = item.ok()?;
    *prefer_control = next;
    Some(delivery)
}

fn dispatch(
    frame: WebSocketFrameV1,
    deadline: Instant,
    manager: &ProductRelaySessionManagerV1,
    runtime: &Runtime,
    session: (&str, [u8; 16]),
    replies: &mut Replies,
    window: &mut RelayDeliveryWindowV1,
) -> Result<Incoming> {
    let (peer_id, session_id) = session;
    match frame {
        WebSocketFrameV1::Binary(bytes) => {
            let admission = runtime
                .block_on(manager.admit_authenticated_wire_v1(
                    peer_id,
                    session_id,
                    bytes.len(),
                    now_ms_v1(),
                ))
                .map_err(|reason| {
                    anyhow::anyhow!(
                        "product relay rejected raw authenticated wire admission: {reason:?}"
                    )
                })?;
            let message: ProductRelayWireMessageV1 = match serde_json::from_slice(&bytes) {
                Ok(message) => message,
                Err(error) => {
                    runtime.block_on(manager.reject_admitted_wire_v1(admission));
                    return Err(error).context("decode incremental relay message");
                }
            };
            drop(bytes);
            match message {
                ProductRelayWireMessageV1::Data(envelope) => {
                    let outcome = runtime.block_on(manager.forward_opaque_admitted_v1(
                        admission,
                        envelope,
                        now_ms_v1(),
                    ));
                    let disposition = outcome.disposition;
                    replies.wire(ProductRelayWireMessageV1::ForwardOutcome(outcome), deadline)?;
                    if relay_forward_disposition_requires_close_v1(disposition) {
                        return Ok(Incoming::Rejected(disposition));
                    }
                }
                ProductRelayWireMessageV1::PeerHandshake {
                    target_peer_id,
                    handshake,
                } => {
                    let outcome = runtime.block_on(manager.forward_peer_handshake_admitted_v1(
                        admission,
                        &target_peer_id,
                        handshake,
                        now_ms_v1(),
                    ));
                    let disposition = outcome.disposition;
                    replies.wire(ProductRelayWireMessageV1::ForwardOutcome(outcome), deadline)?;
                    if relay_forward_disposition_requires_close_v1(disposition) {
                        return Ok(Incoming::Rejected(disposition));
                    }
                }
                ProductRelayWireMessageV1::Heartbeat => {
                    if !runtime.block_on(manager.heartbeat_admitted_v1(admission, now_ms_v1())) {
                        bail!("product relay rejected heartbeat budget or stale session");
                    }
                    replies.wire(ProductRelayWireMessageV1::HeartbeatAck, deadline)?;
                }
                ProductRelayWireMessageV1::DeliveryConsumedV1 { through } => {
                    if let Err(error) = window.acknowledge(through) {
                        runtime.block_on(manager.reject_admitted_wire_v1(admission));
                        return Err(error);
                    }
                    if !runtime.block_on(manager.heartbeat_admitted_v1(admission, now_ms_v1())) {
                        bail!("relay rejected delivery consumption session");
                    }
                }
                ProductRelayWireMessageV1::Close => return Ok(Incoming::Close),
                ProductRelayWireMessageV1::HandshakeOffer(_)
                | ProductRelayWireMessageV1::HandshakeResponse(_)
                | ProductRelayWireMessageV1::DeliveryWindowV1 { .. }
                | ProductRelayWireMessageV1::Delivery(_)
                | ProductRelayWireMessageV1::PeerHandshakeDelivery(_)
                | ProductRelayWireMessageV1::HeartbeatAck
                | ProductRelayWireMessageV1::ForwardOutcome(_) => {
                    runtime.block_on(manager.reject_admitted_wire_v1(admission));
                    bail!("invalid relay wire message after authentication");
                }
            }
        }
        WebSocketFrameV1::Ping(payload) => {
            if !runtime.block_on(manager.ping_with_wire_bytes(
                peer_id,
                session_id,
                payload.len(),
                now_ms_v1(),
            )) {
                bail!("product relay rejected ping budget or stale session");
            }
            replies.push(0xA, payload, deadline)?;
        }
        WebSocketFrameV1::Pong(payload) => {
            if !runtime.block_on(manager.ping_with_wire_bytes(
                peer_id,
                session_id,
                payload.len(),
                now_ms_v1(),
            )) {
                bail!("product relay rejected pong budget or stale session");
            }
        }
        WebSocketFrameV1::Close => return Ok(Incoming::Close),
    }
    Ok(Incoming::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_count_and_actual_owned_bytes_are_bounded_without_partial_admission() {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut replies = Replies::default();
        for _ in 0..MAX_REPLY_COUNT {
            replies.push(0xA, vec![1], deadline).unwrap();
        }
        assert!(replies.push(0xA, vec![2], deadline).is_err());
        assert_eq!(replies.queued.len(), MAX_REPLY_COUNT);
        assert_eq!(replies.bytes, MAX_REPLY_COUNT);
        assert!(replies.queued.iter().all(|reply| reply.payload == [1]));
        let active = replies.take_next().unwrap().unwrap();
        assert_eq!(replies.bytes, MAX_REPLY_COUNT);
        assert!(replies.push(0xA, vec![2], deadline).is_err());
        assert!(replies.take_next().is_err());
        assert_eq!(active.payload, [1]);
        replies.flushed();
        assert_eq!(replies.bytes, MAX_REPLY_COUNT - 1);
        replies.push(0xA, vec![2], deadline).unwrap();

        let mut replies = Replies::default();
        let mut oversized_capacity = Vec::with_capacity(MAX_REPLY_BYTES * 2);
        oversized_capacity.resize(MAX_REPLY_BYTES, 7);
        replies.push(0x2, oversized_capacity, deadline).unwrap();
        assert_eq!(replies.queued[0].payload.capacity(), MAX_REPLY_BYTES);
        let active = replies.take_next().unwrap().unwrap();
        assert!(replies.push(0xA, vec![1], deadline).is_err());
        assert_eq!(replies.queued.len(), 0);
        assert_eq!(replies.bytes, MAX_REPLY_BYTES);
        assert_eq!(active.payload.len(), MAX_REPLY_BYTES);
        replies.flushed();
        assert_eq!(replies.bytes, 0);
    }

    #[test]
    fn pending_reply_deadlines_are_original_absolute_and_not_fifo_assumptions() {
        let now = Instant::now();
        let mut replies = Replies::default();
        replies
            .push(0x2, vec![1], now + Duration::from_secs(10))
            .unwrap();
        replies
            .push(0x2, vec![2], now + Duration::from_secs(5))
            .unwrap();
        assert_eq!(
            replies.earliest_deadline(),
            Some(now + Duration::from_secs(5))
        );
        replies.check().unwrap();
        assert!(replies.push(0x2, vec![3], now).is_err());
        assert_eq!(replies.bytes, 2);
        assert_eq!(replies.queued.len(), 2);
        replies.queued[1].deadline = now;
        assert!(replies.check().is_err());
        assert_eq!(replies.queued[0].payload, [1]);
        assert_eq!(replies.queued[1].payload, [2]);
    }
}
