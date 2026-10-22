//! NVMBOX07 session adapter on the existing NativeObjectLink. The embedding
//! socket owner keeps polling the SAME link while mailbox work awaits storage
//! or Watch. No socket, worker, store or identity is created here.
//!
//! Four business-owned lanes and an authenticated resource capacity exchange
//! keep request progress bounded without making four the device ceiling. Every slot has its own monotonic sequence, bound to the
//! authenticated transport session. Errors/cancellation latch the slot; never
//! reset counters or automatically retry an unknown business outcome.
use super::*;
use crate::{
    novorudp::{
        diagnostics::{transfer_tag_v1, NativeProgressStageV1},
        fragments::{CompletedMessage, FragmentProfile, OutgoingMessage},
        object_link::{
            NativeApplicationRole, NativeObjectLink, NativeSendCancellation, NativeSendCompletion,
            NativeSessionValidity,
        },
        object_sender::ObjectSendState,
    },
    product_overlay::peer_id_from_ed25519_public_key_v1,
};
use sha2::{Digest, Sha256};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Dedicated control/watch/receipt/send lanes; additional work uses negotiated slots.
pub const SLOTS: usize = 4;
mod capacity;
use crate::service_concurrency::ServiceConcurrencyV1;
pub use capacity::request_retained_bytes_v1;
use capacity::{ClientCapacity, ServerCapacity, Window};
pub const REQUEST_DEADLINE: Duration = Duration::from_secs(30);
#[derive(Debug)]
pub struct CapacityNegotiationExpired;
impl std::fmt::Display for CapacityNegotiationExpired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native resource negotiation deadline")
    }
}
impl std::error::Error for CapacityNegotiationExpired {}
/// Resource-derived protocol allocation ceiling shared by the existing link owner.
pub fn adaptive_request_slot_ceiling_v1(controller: &ServiceConcurrencyV1) -> usize {
    Window::from_controller(controller).slots
}
pub fn adaptive_retained_bytes_v1(controller: &ServiceConcurrencyV1) -> usize {
    Window::from_controller(controller).bytes
}

pub fn domain() -> [u8; 32] {
    Sha256::digest(b"novonet.mailbox.apfl.v7\0").into()
}
/// Routing hint only, not authentication or validation. Dispatch a completed
/// object from this lane to the indicated session adapter, which validates the
/// entire schema/context. This permits the same L4 peer to be client AND server.
pub fn recipient_role(body: &CompletedMessage) -> Result<NativeApplicationRole> {
    let prefix: Vec<u8> = body.chunks.iter().flatten().take(24).copied().collect();
    ensure!(prefix.len() == 24, "short mailbox routing header");
    match u64::from_le_bytes(fixed(&prefix[16..24])?) {
        REQUEST_SCHEMA | BULK_REQUEST_SCHEMA => Ok(NativeApplicationRole::Service),
        REPLY_SCHEMA | BULK_REPLY_SCHEMA => Ok(NativeApplicationRole::Client),
        _ => anyhow::bail!("unknown mailbox routing schema"),
    }
}

struct Binding {
    peer: usize,
    id: String,
    session: [u8; 16],
    local: [u8; 32],
    role: NativeApplicationRole,
    validity: NativeSessionValidity,
}
impl Binding {
    fn claim(
        link: &mut NativeObjectLink,
        peer: usize,
        role: NativeApplicationRole,
    ) -> Result<Self> {
        let id = link.peer_id(peer)?.to_owned();
        let session = link.claim_application_session(peer, STREAM, domain(), role)?;
        Ok(Self {
            peer,
            id,
            session,
            local: link.local_identity(),
            role,
            validity: link.session_validity(peer)?,
        })
    }
    fn check(&self, link: &NativeObjectLink) -> Result<()> {
        ensure!(
            self.validity.is_current()
                && link.local_identity() == self.local
                && link.session_id(self.peer) == Some(self.session)
                && link.peer_id(self.peer)? == self.id,
            "native mailbox session changed"
        );
        Ok(())
    }
    fn context(&self, slot: usize, sequence: u64) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"novonet.mailbox.request.v7\0");
        hash.update(domain());
        hash.update(self.session);
        let local = peer_id_from_ed25519_public_key_v1(&self.local);
        let parties = match self.role {
            NativeApplicationRole::Client => [&local, &self.id],
            NativeApplicationRole::Service => [&self.id, &local],
        };
        for party in parties {
            hash.update((party.len() as u32).to_le_bytes());
            hash.update(party.as_bytes());
        }
        hash.update((slot as u32).to_le_bytes());
        hash.update(sequence.to_le_bytes());
        hash.finalize().into()
    }
}
fn flatten(body: &CompletedMessage) -> Result<Zeroizing<Vec<u8>>> {
    // Inspect the fixed header before coalescing a potentially large object.
    let prefix: Vec<u8> = body.chunks.iter().flatten().take(24).copied().collect();
    let schema = u64::from_le_bytes(fixed(prefix.get(16..24).context("mailbox schema header")?)?);
    ensure!(
        matches!(
            schema,
            REQUEST_SCHEMA | REPLY_SCHEMA | BULK_REQUEST_SCHEMA | BULK_REPLY_SCHEMA
        ),
        "mailbox schema/version"
    );
    let budget = wire_limits(schema).wire_bytes;
    let mut length = 0usize;
    for chunk in &body.chunks {
        length = length
            .checked_add(chunk.len())
            .filter(|n| *n <= budget)
            .context("native mailbox wire bound")?;
    }
    let mut out = Zeroizing::new(Vec::with_capacity(length));
    for chunk in &body.chunks {
        out.extend_from_slice(chunk);
    }
    Ok(out)
}
fn outgoing(bytes: Zeroizing<Vec<u8>>) -> Result<Arc<OutgoingMessage>> {
    // The existing transport retains ordinary byte buffers. Their secure erase
    // lifecycle remains a separate integration requirement; do not claim it here.
    Ok(Arc::new(OutgoingMessage::with_profile(
        domain(),
        bytes.to_vec(),
        BULK_LIMITS.wire_bytes,
        FragmentProfile::NativeUdpV2,
    )?))
}
fn context(bytes: &[u8]) -> Result<[u8; 32]> {
    fixed(bytes.get(24..56).context("short mailbox batch")?)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestId {
    pub slot: usize,
    pub sequence: u64,
}
struct Pending {
    cancellation: NativeSendCancellation,
    cancel_on_drop: bool,
    id: RequestId,
    context: [u8; 32],
    requests: Vec<Request>,
    body: Arc<OutgoingMessage>,
    sent: bool,
    deadline: Instant,
}
impl Drop for Pending {
    fn drop(&mut self) {
        if self.cancel_on_drop {
            self.cancellation.cancel();
        }
    }
}
struct RetiredRequest {
    id: RequestId,
    context: [u8; 32],
    rows: usize,
}
/// A late reply for a terminated slot is distinct from an unsolicited frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyRequestV7 {
    Pending(RequestId),
    Retired(RequestId),
}
struct ClientSlot {
    retired: Option<RetiredRequest>,
    sequence: u64,
    failed: bool,
    pending: Option<Pending>,
}
impl ClientSlot {
    fn fail_pending(&mut self) -> Option<Pending> {
        self.failed = true;
        let pending = self.pending.take()?;
        self.retired = Some(RetiredRequest {
            id: pending.id,
            context: pending.context,
            rows: pending.requests.len(),
        });
        Some(pending)
    }
}
pub struct NativeMailboxClient {
    binding: Binding,
    node: [u8; 32],
    slots: Vec<ClientSlot>,
    bulk_ready: bool,
    capacity: Option<ClientCapacity>,
}
impl NativeMailboxClient {
    pub fn enable_verified_bulk_v1(&mut self) {
        self.bulk_ready = true;
    }

    pub fn invalidate(&mut self) {
        if let Some(c) = &self.capacity {
            c.cancellation.cancel();
        }
        for slot in &mut self.slots {
            slot.fail_pending();
        }
    }
    pub fn check_session(&self, link: &NativeObjectLink) -> Result<()> {
        self.binding.check(link)
    }
    /// Cancel exactly this request; no sequence reset or remote rollback.
    pub fn cancel(&mut self, id: RequestId) -> bool {
        let Some(slot) = self.slots.get_mut(id.slot) else {
            return false;
        };
        if !slot.pending.as_ref().is_some_and(|p| p.id == id) {
            return false;
        }
        slot.fail_pending();
        true
    }
    /// Inspect only the authenticated peer/session and a fixed-size context
    /// prefix on the hot path. Terminated replies additionally undergo full
    /// schema validation before being classified as safe to discard.
    pub fn reply_request(
        &self,
        link: &NativeObjectLink,
        body: &CompletedMessage,
    ) -> Result<ReplyRequestV7> {
        self.binding.check(link)?;
        ensure!(body.peer == self.binding.id, "mailbox reply peer");
        let mut prefix = [0u8; 56];
        let mut source = body.chunks.iter().flatten();
        for byte in &mut prefix {
            *byte = *source.next().context("short mailbox reply")?;
        }
        let context = context(&prefix)?;
        for slot in &self.slots {
            if let Some(p) = slot.pending.as_ref().filter(|p| p.context == context) {
                return Ok(ReplyRequestV7::Pending(p.id));
            }
            if let Some(p) = slot.retired.as_ref().filter(|p| p.context == context) {
                let bytes = flatten(body)?;
                decode_replies(&bytes, p.context, p.rows)?;
                return Ok(ReplyRequestV7::Retired(p.id));
            }
        }
        anyhow::bail!("unsolicited/old mailbox reply")
    }
    /// Call only once after the existing pinned peer handshake is established.
    /// No handshake per batch; node must be that same peer, not a receipt key
    /// supplied by an unauthenticated response.
    pub fn attach(link: &mut NativeObjectLink, peer: usize, node: [u8; 32]) -> Result<Self> {
        Self::attach_adaptive(link, peer, node, ServiceConcurrencyV1::from_host())
    }
    /// Exchange resource capacity on the existing authenticated lane.
    pub fn attach_adaptive(
        link: &mut NativeObjectLink,
        peer: usize,
        node: [u8; 32],
        controller: Arc<ServiceConcurrencyV1>,
    ) -> Result<Self> {
        ensure!(
            link.peer_id(peer)? == peer_id_from_ed25519_public_key_v1(&node),
            "mailbox node pin mismatch"
        );
        let mut value = Self {
            binding: Binding::claim(link, peer, NativeApplicationRole::Client)?,
            node,
            bulk_ready: false,
            capacity: None,
            slots: (0..SLOTS)
                .map(|_| ClientSlot {
                    retired: None,
                    sequence: 1,
                    failed: false,
                    pending: None,
                })
                .collect(),
        };
        let offer = Window::from_controller(&controller);
        value.capacity = Some(ClientCapacity {
            offer,
            body: outgoing(capacity::request(capacity::binding(&value.binding), offer)?)?,
            deadline: Instant::now() + REQUEST_DEADLINE,
            cancellation: NativeSendCancellation::default(),
            sent: false,
            grant: None,
        });
        Ok(value)
    }
    pub fn slot_capacity(&self) -> usize {
        self.slots.len()
    }
    pub fn negotiated_retained_bytes(&self) -> Option<usize> {
        self.capacity
            .as_ref()
            .and_then(|c| c.grant.map(|w| w.bytes))
    }
    pub fn capacity_deadline(&self) -> Option<Instant> {
        self.capacity
            .as_ref()
            .filter(|c| c.grant.is_none())
            .map(|c| c.deadline)
    }
    /// Required control data never occupies or retires a business slot.
    pub fn progress_capacity(&mut self, link: &mut NativeObjectLink, now: Instant) -> Result<()> {
        self.binding.check(link)?;
        if let Some(c) = &mut self.capacity {
            if c.grant.is_some() {
                return Ok(());
            }
            if now >= c.deadline {
                c.cancellation.cancel();
                return Err(CapacityNegotiationExpired.into());
            }
            if !c.sent {
                c.sent = link.try_send_cancellable_until(
                    self.binding.peer,
                    STREAM,
                    &c.body,
                    now,
                    c.deadline,
                    &c.cancellation,
                )?;
            }
        }
        Ok(())
    }
    /// True only for the current session's authenticated resource context.
    pub fn receive_capacity(
        &mut self,
        link: &NativeObjectLink,
        body: &CompletedMessage,
        now: Instant,
    ) -> Result<bool> {
        self.binding.check(link)?;
        let expected = capacity::binding(&self.binding);
        if !capacity::is_context(body, expected) {
            return Ok(false);
        }
        ensure!(body.peer == self.binding.id, "capacity reply peer");
        let c = self
            .capacity
            .as_mut()
            .context("unsolicited capacity reply")?;
        let (offer, grant) = capacity::decode(&capacity::flatten_control(body)?, expected, true)?;
        ensure!(offer == c.offer, "capacity reply offer mismatch");
        let grant = grant.context("capacity reply grant")?;
        if let Some(previous) = c.grant {
            ensure!(previous == grant, "capacity grant changed");
        }
        if c.grant.is_none() {
            if now >= c.deadline {
                return Err(CapacityNegotiationExpired.into());
            }
            self.slots.resize_with(grant.slots, || ClientSlot {
                retired: None,
                sequence: 1,
                failed: false,
                pending: None,
            });
            c.grant = Some(grant);
        }
        Ok(true)
    }
    /// Bounded local preparation, not a service receipt or automatic replay.
    pub fn begin(
        &mut self,
        slot: usize,
        requests: Vec<Request>,
        now: Instant,
    ) -> Result<RequestId> {
        self.begin_until(slot, requests, now, now + REQUEST_DEADLINE)
    }
    /// Caller deadline starts before enqueueing; preparation never restarts it.
    pub fn begin_until(
        &mut self,
        slot: usize,
        requests: Vec<Request>,
        now: Instant,
        deadline: Instant,
    ) -> Result<RequestId> {
        self.begin_until_cancellable(
            slot,
            requests,
            now,
            deadline,
            &NativeSendCancellation::default(),
        )
    }
    /// Shared owner cancellation reaches retained link sends even if the future
    /// is dropped between driver progress and the next link polling quantum.
    pub(crate) fn begin_until_cancellable(
        &mut self,
        slot: usize,
        requests: Vec<Request>,
        now: Instant,
        deadline: Instant,
        cancellation: &NativeSendCancellation,
    ) -> Result<RequestId> {
        ensure!(
            self.bulk_ready
                || !requests.iter().any(|r| matches!(
                    r,
                    Request::PutBulkV1 { .. }
                        | Request::FetchBulkV1 { .. }
                        | Request::WatchFetchBulkV1 { .. }
                )),
            "bulk capability not negotiated"
        );
        let deadline = deadline.min(now + REQUEST_DEADLINE);
        ensure!(now < deadline, "mailbox deadline before preparation");
        let s = self.slots.get_mut(slot).context("mailbox slot")?;
        ensure!(
            !s.failed && s.pending.is_none(),
            "mailbox slot busy/unusable"
        );
        let id = RequestId {
            slot,
            sequence: s.sequence,
        };
        let context = self.binding.context(slot, id.sequence);
        let body = outgoing(encode_requests(context, &requests)?)?;
        s.pending = Some(Pending {
            cancellation: cancellation.clone(),
            cancel_on_drop: true,
            id,
            context,
            requests,
            body,
            sent: false,
            deadline,
        });
        Ok(id)
    }
    pub fn request_transfer_tag_v1(&self, id: RequestId) -> Option<[u8; 32]> {
        let pending = self
            .slots
            .get(id.slot)?
            .pending
            .as_ref()
            .filter(|p| p.id == id)?;
        Some(transfer_tag_v1(
            self.binding.session,
            STREAM,
            pending.body.id(),
        ))
    }
    pub fn try_send(
        &mut self,
        link: &mut NativeObjectLink,
        id: RequestId,
        now: Instant,
        authority: impl Fn() -> Result<()>,
    ) -> Result<bool> {
        if let Err(error) = self.binding.check(link).and_then(|_| authority()) {
            self.invalidate();
            return Err(error);
        }
        let s = self.slots.get_mut(id.slot).context("mailbox slot")?;
        let p = s
            .pending
            .as_mut()
            .filter(|p| p.id == id)
            .context("mailbox pending identity")?;
        if now >= p.deadline {
            s.fail_pending();
            anyhow::bail!("mailbox deadline; outcome unknown");
        }
        if !p.sent {
            p.sent = link.try_send_cancellable_until(
                self.binding.peer,
                STREAM,
                &p.body,
                now,
                p.deadline,
                &p.cancellation,
            )?;
        }
        Ok(p.sent)
    }
    /// Consume only a completion from this link's NVMBOX07 lane. A transport
    /// DONE never calls this method and cannot turn into business success.
    pub fn receive(
        &mut self,
        link: &NativeObjectLink,
        body: CompletedMessage,
        now: Instant,
        authority: impl Fn() -> Result<()>,
    ) -> Result<(RequestId, Vec<Reply>)> {
        if let Err(error) = self.binding.check(link).and_then(|_| authority()) {
            self.invalidate();
            return Err(error);
        }
        ensure!(body.peer == self.binding.id, "mailbox reply peer");
        let bytes = flatten(&body)?;
        let context = context(&bytes)?;
        let index = self
            .slots
            .iter()
            .position(|s| s.pending.as_ref().is_some_and(|p| p.context == context))
            .context("unsolicited/old mailbox reply")?;
        let s = &mut self.slots[index];
        let mut p = s.fail_pending().unwrap(); // all decode, semantic, expiry and authority errors latch
        ensure!(p.sent && now < p.deadline, "mailbox late/unsent reply");
        let replies = decode_replies(&bytes, p.context, p.requests.len())?;
        for (request, reply) in p.requests.iter().zip(&replies) {
            verify_reply(self.node, request, reply)?;
        }
        authority()?;
        if p.requests
            .iter()
            .zip(&replies)
            .any(|(r, reply)| matches!((r, reply), (Request::ProbeBulkV1, Reply::BulkReady { .. })))
        {
            self.bulk_ready = true;
        }
        s.sequence = s
            .sequence
            .checked_add(1)
            .context("mailbox sequence exhausted")?;
        s.failed = false;
        s.retired = None;
        // A verified business reply is success, not cancellation. Its request
        // may still be draining a redundant transport repair or awaiting DONE.
        // Let the original bounded link send finish; errors above still cancel.
        p.cancel_on_drop = false;
        if let Some(tag) = link.diagnostic_transfer_tag_v1(self.binding.session, STREAM, body.id) {
            link.note_response_validated_v1(tag, now);
        }
        Ok((p.id, replies))
    }
    /// Feed the existing link's completion events. Returns an unknown-outcome
    /// request on expiry/replacement; Delivered releases no business request slot.
    pub fn transport_complete(&mut self, event: &NativeSendCompletion) -> Option<RequestId> {
        if event.peer != self.binding.peer || event.stream != STREAM {
            return None;
        }
        for s in &mut self.slots {
            if s.pending
                .as_ref()
                .is_some_and(|p| Arc::ptr_eq(&p.body, &event.body))
                && matches!(
                    event.state,
                    ObjectSendState::Expired
                        | ObjectSendState::SessionReplaced
                        | ObjectSendState::Cancelled
                )
            {
                return s.fail_pending().map(|p| p.id);
            }
        }
        None
    }
    pub fn expire(&mut self, now: Instant) -> Vec<RequestId> {
        let mut expired = Vec::new();
        for s in &mut self.slots {
            if s.pending
                .as_ref()
                .is_some_and(|p| now >= p.deadline || !self.binding.validity.is_current())
            {
                expired.push(s.fail_pending().unwrap().id);
            }
        }
        expired
    }
}
fn verify_reply(node: [u8; 32], request: &Request, reply: &Reply) -> Result<()> {
    if matches!(reply, Reply::Rejected(_)) {
        return Ok(());
    }
    match (request, reply) {
        (
            Request::EndpointDeliverV1 { object, .. },
            Reply::EndpointDeliveredV1 {
                object: echoed,
                receipt_object,
                receipt_expires,
                receipt,
            },
        ) => {
            ensure!(
                echoed == object
                    && *receipt_object != [0; 32]
                    && *receipt_expires > 0
                    && !receipt.is_empty()
                    && receipt.len() <= crate::endpoint_delivery::MAX_ENDPOINT_DELIVERY_BYTES,
                "endpoint device receipt binding/bounds"
            );
            Ok(())
        }
        (Request::ProbeBulkV1, Reply::BulkReady { max_ciphertext }) => {
            ensure!(
                *max_ciphertext as usize == crate::mailbox_store::MAX_BULK_CIPHERTEXT,
                "bulk capability"
            );
            Ok(())
        }
        (Request::ServiceRecordV7, Reply::ServiceRecordV7 { record }) => {
            ensure!(
                record.relay_public_key == node,
                "mailbox locator node binding"
            );
            // This completes a metadata lookup, NOT permission to use its
            // route. Signature/shape/version remain mandatory here. The original
            // trust book checks pin, revocation, sequence and time before use,
            // allowing an authentic future declaration to wait without latching
            // every business slot on this otherwise healthy session.
            crate::product_service_lease::verify_native_record_metadata_v1(record)?;
            Ok(())
        }
        (Request::Register { .. }, Reply::Registered)
        | (Request::Consume { .. }, Reply::Consumed)
        | (Request::Renew { .. }, Reply::Renewed)
        | (Request::Revoke { .. }, Reply::Revoked)
        | (Request::Fetch { .. }, Reply::Empty)
        | (Request::FetchBulkV1 { .. }, Reply::Empty) => Ok(()),
        (
            Request::Put {
                mailbox,
                object,
                ciphertext,
                expires,
                ..
            }
            | Request::PutBulkV1 {
                mailbox,
                object,
                ciphertext,
                expires,
                ..
            },
            Reply::Stored(r),
        ) => r.verify(node, *mailbox, *object, ciphertext, *expires),
        (
            Request::Fetch { .. } | Request::FetchBulkV1 { .. },
            Reply::Object {
                ciphertext, digest, ..
            },
        ) => {
            ensure!(
                ciphertext.len()
                    <= if matches!(request, Request::FetchBulkV1 { .. }) {
                        crate::mailbox_store::MAX_BULK_CIPHERTEXT
                    } else {
                        65_536
                    },
                "mailbox fetch capability size"
            );
            ensure!(
                <[u8; 32]>::from(Sha256::digest(ciphertext)) == *digest,
                "mailbox object digest"
            );
            Ok(())
        }
        (Request::Watch { boxes, .. }, Reply::Notice(notices)) => {
            ensure!(boxes.len() == notices.len(), "mailbox notice shape");
            Ok(())
        }
        (
            Request::WatchFetchV6 { boxes, .. } | Request::WatchFetchBulkV1 { boxes, .. },
            Reply::NoticeObjectV6 { notices, object },
        ) => {
            ensure!(boxes.len() == notices.len(), "mailbox notice shape");
            if let Some(o) = object {
                ensure!(
                    o.ciphertext.len()
                        <= if matches!(request, Request::WatchFetchBulkV1 { .. }) {
                            crate::mailbox_store::MAX_BULK_CIPHERTEXT
                        } else {
                            65_536
                        },
                    "mailbox watch capability size"
                );
                ensure!(
                    o.box_index < boxes.len()
                        && <[u8; 32]>::from(Sha256::digest(&o.ciphertext)) == o.digest,
                    "mailbox prefetch binding"
                );
            }
            Ok(())
        }
        _ => anyhow::bail!("mailbox response operation mismatch"),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Ready,
    Working,
    Failed,
}
struct ServerSlot {
    sequence: u64,
    phase: Phase,
}
type Slots = Arc<Mutex<Vec<ServerSlot>>>;
pub struct NativeMailboxServer {
    binding: Binding,
    cancellation: NativeSendCancellation,
    slots: Slots,
    controller: Arc<ServiceConcurrencyV1>,
    capacity: Option<ServerCapacity>,
}
impl NativeMailboxServer {
    pub(crate) fn slot_capacity(&self) -> usize {
        self.slots.lock().map_or(0, |slots| slots.len())
    }
    pub fn invalidate(&mut self) {
        self.cancellation.cancel();
        if let Ok(mut slots) = self.slots.lock() {
            for slot in slots.iter_mut() {
                slot.phase = Phase::Failed;
            }
        }
    }
    pub fn attach(link: &mut NativeObjectLink, peer: usize) -> Result<Self> {
        Self::attach_adaptive(link, peer, ServiceConcurrencyV1::from_host())
    }
    pub fn attach_adaptive(
        link: &mut NativeObjectLink,
        peer: usize,
        controller: Arc<ServiceConcurrencyV1>,
    ) -> Result<Self> {
        Ok(Self {
            binding: Binding::claim(link, peer, NativeApplicationRole::Service)?,
            cancellation: NativeSendCancellation::default(),
            slots: Arc::new(Mutex::new(
                (0..SLOTS)
                    .map(|_| ServerSlot {
                        sequence: 1,
                        phase: Phase::Ready,
                    })
                    .collect(),
            )),
            controller,
            capacity: None,
        })
    }
    /// Owner must hold at most one adapter per authenticated peer and retain its
    /// node-wide link/output budgets. Returned work is Send and does not borrow
    /// the link: poll network and other slots while it awaits the original owner.
    pub fn admit(
        &mut self,
        link: &NativeObjectLink,
        body: CompletedMessage,
        now: Instant,
        authority: impl Fn() -> Result<()>,
    ) -> Result<MailboxWork> {
        if let Err(error) = self.binding.check(link).and_then(|_| authority()) {
            self.invalidate();
            return Err(error);
        }
        ensure!(
            !self.cancellation.is_cancelled(),
            "mailbox service owner cancelled"
        );
        ensure!(body.peer == self.binding.id, "mailbox request peer");
        let control_context = capacity::binding(&self.binding);
        if capacity::is_context(&body, control_context) {
            let bytes = capacity::flatten_control(&body)?;
            let (offer, _) = capacity::decode(&bytes, control_context, false)?;
            if let Some(previous) = &self.capacity {
                ensure!(
                    previous.offer == offer,
                    "capacity offer changed within session"
                );
            } else {
                let grant = offer.intersect(Window::from_controller(&self.controller));
                let reply = outgoing(capacity::reply(control_context, offer, grant)?)?;
                self.slots
                    .lock()
                    .map_err(|_| anyhow::anyhow!("mailbox owner poisoned"))?
                    .resize_with(grant.slots, || ServerSlot {
                        sequence: 1,
                        phase: Phase::Ready,
                    });
                self.capacity = Some(ServerCapacity { offer, body: reply });
            }
            let response = self.capacity.as_ref().unwrap().body.clone();
            // A control-only response guard does not reset or borrow any normal
            // slot. Replays reproduce exactly the frozen grant, never allocate more.
            return Ok(MailboxWork {
                request_tag: link.diagnostic_transfer_tag_v1(self.binding.session, STREAM, body.id),
                waiting_watch: false,
                rows: 1,
                bytes,
                context: control_context,
                guard: WorkGuard {
                    slots: Arc::new(Mutex::new(vec![ServerSlot {
                        sequence: 1,
                        phase: Phase::Working,
                    }])),
                    slot: 0,
                    sequence: 1,
                    armed: true,
                    validity: self.binding.validity.clone(),
                    cancellation: self.cancellation.clone(),
                },
                peer: self.binding.peer,
                session: self.binding.session,
                signer: self.binding.local,
                deadline: now + REQUEST_DEADLINE,
                capacity_response: Some(response),
                retained_bytes: 4096,
            });
        }
        let bytes = flatten(&body)?;
        let context = context(&bytes)?;
        let mut slots = self
            .slots
            .lock()
            .map_err(|_| anyhow::anyhow!("mailbox owner poisoned"))?;
        let index = slots
            .iter()
            .enumerate()
            .position(|(i, s)| {
                s.phase == Phase::Ready && self.binding.context(i, s.sequence) == context
            })
            .context("mailbox replay/busy/unknown request")?;
        let batch = RequestBatch::decode(&bytes, context)?; // ALL rows before any durable effects
        let waiting_watch = batch.has_waiting_watch()?;
        let rows = batch.len();
        let retained_bytes = batch.retained_bytes_estimate()?;
        slots[index].phase = Phase::Working;
        Ok(MailboxWork {
            request_tag: link.diagnostic_transfer_tag_v1(self.binding.session, STREAM, body.id),
            waiting_watch,
            rows,
            bytes,
            context,
            guard: WorkGuard {
                slots: self.slots.clone(),
                slot: index,
                sequence: slots[index].sequence,
                armed: true,
                validity: self.binding.validity.clone(),
                cancellation: self.cancellation.clone(),
            },
            peer: self.binding.peer,
            session: self.binding.session,
            signer: self.binding.local,
            deadline: now + REQUEST_DEADLINE,
            capacity_response: None,
            retained_bytes,
        })
    }
}
impl Drop for NativeMailboxServer {
    fn drop(&mut self) {
        self.invalidate();
    }
}
struct WorkGuard {
    slots: Slots,
    slot: usize,
    sequence: u64,
    armed: bool,
    validity: NativeSessionValidity,
    cancellation: NativeSendCancellation,
}
impl WorkGuard {
    fn check(&self) -> Result<()> {
        ensure!(
            self.validity.is_current() && !self.cancellation.is_cancelled(),
            "mailbox transport session retired/cancelled"
        );
        let slots = self
            .slots
            .lock()
            .map_err(|_| anyhow::anyhow!("mailbox owner poisoned"))?;
        ensure!(
            slots[self.slot].sequence == self.sequence && slots[self.slot].phase == Phase::Working,
            "mailbox work no longer authorized"
        );
        Ok(())
    }
    fn fail(&mut self) {
        if let Ok(mut slots) = self.slots.lock() {
            let s = &mut slots[self.slot];
            if s.sequence == self.sequence {
                s.phase = Phase::Failed;
            }
        }
    }
}
impl Drop for WorkGuard {
    fn drop(&mut self) {
        if self.armed {
            if let Ok(mut slots) = self.slots.lock() {
                let s = &mut slots[self.slot];
                if s.sequence == self.sequence {
                    s.phase = Phase::Failed;
                }
            }
        }
    }
}
pub struct MailboxWork {
    request_tag: Option<[u8; 32]>,
    capacity_response: Option<Arc<OutgoingMessage>>,
    retained_bytes: usize,
    waiting_watch: bool,
    rows: usize,
    bytes: Zeroizing<Vec<u8>>,
    context: [u8; 32],
    guard: WorkGuard,
    peer: usize,
    session: [u8; 16],
    signer: [u8; 32],
    deadline: Instant,
}
impl MailboxWork {
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn retained_bytes_estimate(&self) -> usize {
        self.retained_bytes
    }
    pub fn is_capacity_negotiation(&self) -> bool {
        self.capacity_response.is_some()
    }
    pub fn has_waiting_watch(&self) -> bool {
        self.waiting_watch
    }

    /// Bounded overload reply on the SAME authenticated request slot. No row is
    /// executed, and the slot advances only when the response is enqueued by
    /// try_send. This is never a storage or delivery acknowledgement.
    pub fn reject_quota(
        self,
        service: &MailboxServiceV1,
        authority: impl Fn() -> Result<()>,
    ) -> Result<MailboxResponse> {
        ensure!(Instant::now() < self.deadline, "mailbox rejection expired");
        ensure!(
            service.identity_public_key() == self.signer,
            "mailbox signer differs from authenticated node"
        );
        self.guard.check()?;
        authority()?;
        if let Some(body) = self.capacity_response {
            return Ok(MailboxResponse {
                request_tag: self.request_tag,
                body,
                guard: self.guard,
                peer: self.peer,
                session: self.session,
                deadline: self.deadline,
                sent: false,
                failed: false,
            });
        }
        let replies = (0..self.rows)
            .map(|_| Reply::Rejected(MailboxRejectionV1::Quota))
            .collect::<Vec<_>>();
        let bytes = encode_replies(self.context, &replies)?;
        Ok(MailboxResponse {
            request_tag: self.request_tag,
            body: outgoing(bytes)?,
            guard: self.guard,
            peer: self.peer,
            session: self.session,
            deadline: self.deadline,
            sent: false,
            failed: false,
        })
    }

    pub async fn execute(
        self,
        service: &MailboxServiceV1,
        authority: impl Fn() -> Result<()>,
    ) -> Result<MailboxResponse> {
        self.execute_timed(service, authority, None).await
    }
    pub(crate) async fn execute_timed(
        self,
        service: &MailboxServiceV1,
        authority: impl Fn() -> Result<()>,
        timing: Option<&crate::service_concurrency::ServiceExecutionTimingV1>,
    ) -> Result<MailboxResponse> {
        let tag = self.request_tag;
        let diagnostics = service.native_diagnostics_v1();
        if let (Some(d), Some(tag)) = (&diagnostics, tag) {
            d.progress(tag, NativeProgressStageV1::WorkStarted, 0, Instant::now());
        }
        let work = self.execute_inner_timed(service, authority, timing);
        let result = if let Some(timing) = timing {
            timing.measure(work).await
        } else {
            work.await
        };
        if result.is_ok() {
            if let (Some(d), Some(tag)) = (&diagnostics, tag) {
                d.progress(tag, NativeProgressStageV1::WorkFinished, 0, Instant::now());
            }
        }
        result
    }
    async fn execute_inner_timed(
        self,
        service: &MailboxServiceV1,
        authority: impl Fn() -> Result<()>,
        timing: Option<&crate::service_concurrency::ServiceExecutionTimingV1>,
    ) -> Result<MailboxResponse> {
        ensure!(
            Instant::now() < self.deadline,
            "mailbox work expired before execution"
        );
        ensure!(
            service.identity_public_key() == self.signer,
            "mailbox signer differs from authenticated node"
        );
        if let Some(body) = self.capacity_response {
            self.guard.check()?;
            authority()?;
            return Ok(MailboxResponse {
                request_tag: self.request_tag,
                body,
                guard: self.guard,
                peer: self.peer,
                session: self.session,
                deadline: self.deadline,
                sent: false,
                failed: false,
            });
        }
        let bytes = tokio::time::timeout_at(
            self.deadline.into(),
            execute_local_batch_timed(
                service,
                &self.bytes,
                self.context,
                || {
                    self.guard.check()?;
                    authority()
                },
                timing,
            ),
        )
        .await
        .context("mailbox deadline; outcome unknown")??;
        // Opt-in, bounded inspection of the existing small encoded reply.
        // Observation never changes the business result or re-executes a row.
        // Large Fetch bodies are not copied/decoded again for diagnostics.
        if let (Some(d), Some(tag)) = (service.native_diagnostics_v1(), self.request_tag) {
            if bytes.len() <= 4096
                && decode_replies(&bytes, self.context, self.rows).is_ok_and(|replies| {
                    replies
                        .iter()
                        .any(|reply| matches!(reply, Reply::Stored(_)))
                })
            {
                d.progress(
                    tag,
                    NativeProgressStageV1::StoredReceiptCreated,
                    0,
                    Instant::now(),
                );
            }
        }
        Ok(MailboxResponse {
            request_tag: self.request_tag,
            body: outgoing(bytes)?,
            guard: self.guard,
            peer: self.peer,
            session: self.session,
            deadline: self.deadline,
            sent: false,
            failed: false,
        })
    }
}

pub struct MailboxResponse {
    request_tag: Option<[u8; 32]>,
    failed: bool,
    body: Arc<OutgoingMessage>,
    guard: WorkGuard,
    peer: usize,
    session: [u8; 16],
    deadline: Instant,
    sent: bool,
}

impl MailboxResponse {
    /// The retained body is released only after this exact send completes.
    pub fn transport_complete(&self, event: &NativeSendCompletion) -> bool {
        event.peer == self.peer && event.stream == STREAM && Arc::ptr_eq(&event.body, &self.body)
    }
    /// false retains THIS handle for retry; do not drop/rebuild it. Successful
    /// enqueue advances this slot only after owner work completed. Later sender
    /// expiry remains an unknown client outcome, not storage rollback.
    pub fn try_send(
        &mut self,
        link: &mut NativeObjectLink,
        now: Instant,
        authority: impl Fn() -> Result<()>,
    ) -> Result<bool> {
        ensure!(!self.failed, "mailbox response cancelled");
        if let Err(error) = authority().and_then(|_| {
            ensure!(
                self.guard.validity.is_current() && !self.guard.cancellation.is_cancelled(),
                "mailbox transport session retired"
            );
            ensure!(
                link.session_id(self.peer) == Some(self.session),
                "mailbox response session changed"
            );
            ensure!(
                now < self.deadline,
                "mailbox response deadline; outcome unknown"
            );
            Ok(())
        }) {
            self.failed = true;
            link.cancel_send(self.peer, STREAM, &self.body);
            self.guard.fail();
            return Err(error);
        }
        if !self.sent {
            let mut slots = self
                .guard
                .slots
                .lock()
                .map_err(|_| anyhow::anyhow!("mailbox owner poisoned"))?;
            let s = &mut slots[self.guard.slot];
            ensure!(
                s.phase == Phase::Working && s.sequence == self.guard.sequence,
                "mailbox response owner mismatch"
            );
            let next = s
                .sequence
                .checked_add(1)
                .context("mailbox sequence exhausted")?;
            if !link.try_send_cancellable_until(
                self.peer,
                STREAM,
                &self.body,
                now,
                self.deadline,
                &self.guard.cancellation,
            )? {
                return Ok(false);
            }
            s.sequence = next;
            s.phase = Phase::Ready;
            self.guard.armed = false;
            link.note_response_diagnostic_v1(
                self.request_tag,
                self.session,
                STREAM,
                self.body.id(),
                now,
            );
            self.sent = true;
        }
        Ok(self.sent)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod endpoint_tests {
    use super::*;
    #[test]
    fn endpoint_reply_requires_original_object_and_bounded_receipt() {
        let request = Request::EndpointDeliverV1 {
            mailbox: [1; 32],
            write: [2; 32],
            object: [3; 32],
            ciphertext: vec![4],
            expires: 100,
        };
        let good = || Reply::EndpointDeliveredV1 {
            object: [3; 32],
            receipt_object: [5; 32],
            receipt_expires: 101,
            receipt: vec![6],
        };
        assert!(verify_reply([9; 32], &request, &good()).is_ok());
        let mut wrong = good();
        if let Reply::EndpointDeliveredV1 { object, .. } = &mut wrong {
            *object = [7; 32];
        }
        assert!(verify_reply([9; 32], &request, &wrong).is_err());
        let mut large = good();
        if let Reply::EndpointDeliveredV1 { receipt, .. } = &mut large {
            *receipt = vec![0; 65_537];
        }
        assert!(verify_reply([9; 32], &request, &large).is_err());
        assert!(verify_reply([9; 32], &request, &Reply::Consumed).is_err());
        assert!(verify_reply(
            [9; 32],
            &Request::Fetch {
                mailbox: [1; 32],
                read: [2; 32]
            },
            &good()
        )
        .is_err());
    }
}
