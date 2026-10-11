//! One bounded, non-reconnecting byte stream over the existing duplex relay.
//!
//! The caller selects/authorizes exactly one relay and pins the remote transport
//! identity. This owner reuses the relay pipeline, signed peer handshake and
//! encrypted NovoRUDP Data/Done frames. It does not select routes, qualify a
//! privacy path, or acknowledge application delivery. A TLS owner may consume
//! these bytes and independently authenticate its own endpoint and chat pins.
//!
//! Stream-local queues are bounded below. The reused pipeline/socket also keep
//! their existing separate bounded buffers; these constants are not a bound on
//! all memory used by TLS, DNS, or the relay implementation.

use super::{
    peer_id_from_ed25519_public_key_v1,
    product_relay_client::{
        pipeline::{PipelineProgress, ProductRelayPipelineV1},
        ProductRelayClientConfigV1, ProductRelayClientControlV1, ProductRelayClientEventV1,
        ProductRelayClientV1, ProductRelayTlsTrustV1,
    },
    E2eSecureChannelV1, HandshakeReplayCacheV1, NodeHandshakeInitiatorV1, NodeHandshakeResponderV1,
    NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0, RelayForwardDispositionV1,
    RelayPeerHandshakeV1, SecureNovoRudpEnvelopeV1,
};
use anyhow::{ensure, Result};
use ed25519_dalek::{SigningKey, VerifyingKey};
use std::{
    collections::VecDeque,
    io,
    panic::{catch_unwind, AssertUnwindSafe},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll, Waker},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::oneshot,
};

pub const RELAY_BYTE_STREAM_MAX_OWNERS_V1: usize = 4;
pub const RELAY_BYTE_STREAM_CHUNK_BYTES_V1: usize = 16 * 1024;
pub const RELAY_BYTE_STREAM_QUEUE_BYTES_V1: usize = 64 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(20);
const MAX_LIFETIME: Duration = Duration::from_secs(120);
const HANDSHAKE_LIMIT: Duration = Duration::from_secs(10);
// Existing frame fields carry the byte-stream domain and cumulative byte
// offset. No new relay wire variant or serialization codec is introduced.
const STREAM_DOMAIN: u64 = u64::from_le_bytes(*b"NVBSTR01");
static OWNERS: AtomicUsize = AtomicUsize::new(0);

type Authority = Arc<dyn Fn() -> io::Result<()> + Send + Sync>;

#[derive(Clone)]
pub struct RelayByteStreamControlV1 {
    deadline: Instant,
    authority: Authority,
}
impl RelayByteStreamControlV1 {
    pub fn new(
        deadline: Instant,
        check: impl Fn() -> io::Result<()> + Send + Sync + 'static,
    ) -> Result<Self> {
        let now = Instant::now();
        ensure!(
            deadline > now && deadline.duration_since(now) <= MAX_LIFETIME,
            "invalid relay byte-stream deadline"
        );
        let value = Self {
            deadline,
            authority: Arc::new(check),
        };
        value.check()?;
        Ok(value)
    }
    fn check(&self) -> io::Result<()> {
        // Authority failures are terminal protocol/local failures, not a
        // recoverable socket timeout in a caller's transport retry classifier.
        (self.authority)().map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if Instant::now() >= self.deadline {
            return Err(Failure::deadline().error());
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Failure {
    kind: io::ErrorKind,
    reason: &'static str,
}
impl Failure {
    fn protocol(reason: &'static str) -> Self {
        Self {
            kind: io::ErrorKind::InvalidData,
            reason,
        }
    }
    fn cancelled() -> Self {
        Self {
            kind: io::ErrorKind::ConnectionAborted,
            reason: "relay byte stream cancelled",
        }
    }
    fn deadline() -> Self {
        Self {
            kind: io::ErrorKind::TimedOut,
            reason: "relay byte-stream original deadline expired",
        }
    }
    fn error(self) -> io::Error {
        io::Error::new(self.kind, self.reason)
    }
}
type WorkResult<T> = std::result::Result<T, Failure>;

struct Permit;
impl Permit {
    fn acquire() -> Result<Self> {
        OWNERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < RELAY_BYTE_STREAM_MAX_OWNERS_V1).then_some(count + 1)
            })
            .map_err(|_| anyhow::anyhow!("relay byte-stream owner capacity exhausted"))?;
        Ok(Self)
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        OWNERS.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Default)]
struct State {
    outbound: VecDeque<Vec<u8>>,
    // Includes a submitted chunk until its correlated relay result is accepted.
    outbound_bytes: usize,
    written: u64,
    confirmed: u64,
    inbound: VecDeque<Vec<u8>>,
    inbound_bytes: usize,
    head_offset: usize,
    shutdown_requested: bool,
    shutdown_confirmed: bool,
    remote_done: bool,
    consumer_dropped: bool,
    successful: bool,
    failure: Option<Failure>,
    read_waker: Option<Waker>,
    write_waker: Option<Waker>,
    relay_waker: Option<Waker>,
}
struct Shared {
    control: RelayByteStreamControlV1,
    state: Mutex<State>,
    worker: Mutex<Option<JoinHandle<()>>>,
    joined: AtomicBool,
}
impl Shared {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| {
            let mut state = error.into_inner();
            state
                .failure
                .get_or_insert(Failure::protocol("relay byte-stream state poisoned"));
            state
        })
    }
    fn wake(&self) {
        let wakes = {
            let state = self.state();
            [
                state.read_waker.clone(),
                state.write_waker.clone(),
                state.relay_waker.clone(),
            ]
        };
        for waker in wakes.into_iter().flatten() {
            waker.wake();
        }
    }
    fn wake_relay(&self) {
        let waker = self.state().relay_waker.clone();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
    fn fail(&self, failure: Failure) {
        {
            let mut state = self.state();
            state.failure.get_or_insert(failure);
            state.outbound.clear();
            state.outbound_bytes = 0;
            state.inbound.clear();
            state.inbound_bytes = 0;
        }
        self.wake();
    }
    fn check(&self) -> io::Result<()> {
        // Never invoke caller code while holding queue/worker locks.
        if let Err(error) = self.control.check() {
            self.fail(Failure {
                kind: error.kind(),
                reason: "relay byte-stream authority or deadline rejected",
            });
            return Err(error);
        }
        if let Some(error) = self.state().failure {
            return Err(error.error());
        }
        Ok(())
    }
    fn check_worker(&self) -> WorkResult<()> {
        self.check()
            .map_err(|_| self.state().failure.unwrap_or(Failure::cancelled()))
    }
    fn joined(&self) -> bool {
        if self.joined.load(Ordering::Acquire) {
            return true;
        }
        let ready = {
            let mut worker = self.worker.lock().unwrap_or_else(|e| e.into_inner());
            if worker.as_ref().is_some_and(JoinHandle::is_finished) {
                worker.take()
            } else {
                None
            }
        };
        if let Some(worker) = ready {
            if worker.join().is_err() {
                self.fail(Failure::protocol("relay byte-stream worker panicked"));
            }
            self.joined.store(true, Ordering::Release);
            return true;
        }
        false
    }
}

/// Observes/joins the actual worker. Cancellation is permanent; this handle
/// cannot create a connection, replace authority, or restart a session.
#[derive(Clone)]
pub struct RelayByteStreamCompletionV1 {
    shared: Arc<Shared>,
}
impl RelayByteStreamCompletionV1 {
    pub fn cancel(&self) {
        self.shared.fail(Failure::cancelled());
    }
    pub fn is_finished(&self) -> bool {
        self.shared.joined()
    }
    /// Only a bounded resource-join wait. `deadline` does not extend the stream's
    /// original authority or network deadline. An aborted worker returns Err
    /// even after it has exited; `is_finished` separately proves actual exit.
    pub async fn wait_finished(&self, deadline: Instant) -> io::Result<()> {
        loop {
            if self.shared.joined() {
                let state = self.shared.state();
                if let Some(failure) = state.failure {
                    return Err(failure.error());
                }
                return if state.successful {
                    Ok(())
                } else {
                    Err(Failure::protocol("relay byte-stream worker did not complete").error())
                };
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(Failure::deadline().error());
            }
            tokio::time::sleep(POLL_INTERVAL.min(deadline.duration_since(now))).await;
        }
    }
}

/// Non-clonable owned IO. Async poll methods only inspect bounded memory; the
/// single worker owns all blocking DNS/TLS/pipeline work. Successful writes mean
/// local bounded admission. Flush additionally waits for precise relay forward
/// results, which do NOT prove peer consumption or application durable receipt.
pub struct RelayByteStreamV1 {
    shared: Arc<Shared>,
}
impl RelayByteStreamV1 {
    pub async fn connect(
        identity: SigningKey,
        mut config: ProductRelayClientConfigV1,
        remote_peer_key: [u8; 32],
        initiator: bool,
        control: RelayByteStreamControlV1,
    ) -> Result<Self> {
        control.check()?;
        ensure!(
            !matches!(
                config.tls_trust,
                ProductRelayTlsTrustV1::NodeKeyBoundEncrypted
            ),
            "relay byte stream requires certificate validation"
        );
        validate_remote(&remote_peer_key, &identity.verifying_key().to_bytes())?;
        let permit = Permit::acquire()?;
        // The pipeline's existing wait uses this timeout. Clamping the private
        // config copy keeps idle revocation checks bounded without altering the
        // shared pipeline or resetting its existing absolute frame deadlines.
        config.read_timeout_ms = 20;
        let shared = Arc::new(Shared {
            control,
            state: Mutex::new(State::default()),
            worker: Mutex::new(None),
            joined: AtomicBool::new(false),
        });
        let mut connecting = Connecting {
            shared: shared.clone(),
            armed: true,
        };
        let (ready, received) = oneshot::channel();
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("novovm-relay-bytes".into())
            .spawn(move || {
                let _permit = permit;
                // Keep setup notification alive until the final error is stored,
                // so a waiting constructor cannot replace its cause with cancel.
                let mut ready = Some(ready);
                let result = catch_unwind(AssertUnwindSafe(|| {
                    run_worker(
                        &worker_shared,
                        identity,
                        config,
                        remote_peer_key,
                        initiator,
                        &mut ready,
                    )
                }));
                match result {
                    Ok(Ok(())) => worker_shared.state().successful = true,
                    Ok(Err(error)) => worker_shared.fail(error),
                    Err(_) => {
                        worker_shared.fail(Failure::protocol("relay byte-stream worker panicked"))
                    }
                }
                // run_worker has dropped its socket/pipeline and handshake keys.
                worker_shared.state().relay_waker = None;
                worker_shared.wake();
            })?;
        *shared
            .worker
            .lock()
            .map_err(|_| anyhow::anyhow!("relay worker lock poisoned"))? = Some(worker);
        if received.await.is_err() {
            return Err(shared
                .state()
                .failure
                .unwrap_or(Failure::protocol("relay byte-stream setup failed"))
                .error()
                .into());
        }
        shared.check()?;
        connecting.armed = false;
        Ok(Self { shared })
    }
    pub fn completion(&self) -> RelayByteStreamCompletionV1 {
        RelayByteStreamCompletionV1 {
            shared: self.shared.clone(),
        }
    }
}
struct Connecting {
    shared: Arc<Shared>,
    armed: bool,
}
impl Drop for Connecting {
    fn drop(&mut self) {
        if self.armed {
            self.shared.fail(Failure::cancelled());
        }
    }
}
impl Drop for RelayByteStreamV1 {
    fn drop(&mut self) {
        let graceful = {
            let mut state = self.shared.state();
            state.consumer_dropped = true;
            // TLS finish shuts down, then immediately drops its underlying IO.
            // Keep the worker to drain the remote authenticated Done; never
            // silently discard unread trailing data or an unconfirmed write.
            state.shutdown_confirmed && state.inbound_bytes == 0 && state.failure.is_none()
        };
        if graceful {
            self.shared.wake();
        } else {
            self.shared.fail(Failure::cancelled());
        }
    }
}

impl AsyncRead for RelayByteStreamV1 {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if let Err(error) = self.shared.check() {
            return Poll::Ready(Err(error));
        }
        let result = {
            let mut state = self.shared.state();
            state.read_waker = Some(cx.waker().clone());
            if output.remaining() == 0 {
                Poll::Ready(Ok(()))
            } else if let Some(front) = state.inbound.front() {
                let count = output.remaining().min(front.len() - state.head_offset);
                output.put_slice(&front[state.head_offset..state.head_offset + count]);
                state.head_offset += count;
                state.inbound_bytes -= count;
                if state.head_offset == state.inbound.front().expect("front remains").len() {
                    state.inbound.pop_front();
                    state.head_offset = 0;
                }
                Poll::Ready(Ok(()))
            } else if state.remote_done {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        };
        if matches!(result, Poll::Ready(_)) {
            self.shared.wake_relay();
        }
        match self.shared.check() {
            Ok(()) => result,
            Err(error) => Poll::Ready(Err(error)),
        }
    }
}
impl AsyncWrite for RelayByteStreamV1 {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        input: &[u8],
    ) -> Poll<io::Result<usize>> {
        if let Err(error) = self.shared.check() {
            return Poll::Ready(Err(error));
        }
        let result = {
            let mut state = self.shared.state();
            state.write_waker = Some(cx.waker().clone());
            if state.shutdown_requested {
                Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "relay byte-stream write half closed",
                )))
            } else if input.is_empty() {
                Poll::Ready(Ok(0))
            } else {
                let count = input
                    .len()
                    .min(RELAY_BYTE_STREAM_CHUNK_BYTES_V1)
                    .min(RELAY_BYTE_STREAM_QUEUE_BYTES_V1 - state.outbound_bytes);
                if count == 0 {
                    Poll::Pending
                } else if let Some(next) = state.written.checked_add(count as u64) {
                    state.outbound.push_back(input[..count].to_vec());
                    state.outbound_bytes += count;
                    state.written = next;
                    Poll::Ready(Ok(count))
                } else {
                    Poll::Ready(Err(
                        Failure::protocol("relay byte-stream offset exhausted").error()
                    ))
                }
            }
        };
        if matches!(result, Poll::Ready(Ok(_))) {
            self.shared.wake_relay();
        }
        if matches!(result, Poll::Ready(Err(_))) {
            self.shared
                .fail(Failure::protocol("relay byte-stream write rejected"));
        }
        match self.shared.check() {
            Ok(()) => result,
            Err(error) => Poll::Ready(Err(error)),
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let Err(error) = self.shared.check() {
            return Poll::Ready(Err(error));
        }
        let result = {
            let mut state = self.shared.state();
            state.write_waker = Some(cx.waker().clone());
            if state.confirmed == state.written {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        };
        match self.shared.check() {
            Ok(()) => result,
            Err(error) => Poll::Ready(Err(error)),
        }
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let Err(error) = self.shared.check() {
            return Poll::Ready(Err(error));
        }
        let result = {
            let mut state = self.shared.state();
            state.write_waker = Some(cx.waker().clone());
            state.shutdown_requested = true;
            if state.shutdown_confirmed {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        };
        // Wake only the worker: waking our own registered writer on every
        // Pending poll would spin instead of waiting for the correlated ACK.
        self.shared.wake_relay();
        match self.shared.check() {
            Ok(()) => result,
            Err(error) => Poll::Ready(Err(error)),
        }
    }
}

fn validate_remote(remote: &[u8; 32], local: &[u8; 32]) -> Result<()> {
    let key = VerifyingKey::from_bytes(remote)?;
    let point = key.to_edwards();
    ensure!(
        remote != local
            && !key.is_weak()
            && point.is_torsion_free()
            && point.compress().to_bytes() == *remote,
        "invalid relay byte-stream peer key"
    );
    Ok(())
}
fn now_ms() -> WorkResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Failure::protocol("relay byte-stream clock unavailable"))?
        .as_millis()
        .try_into()
        .map_err(|_| Failure::protocol("relay byte-stream clock overflow"))
}
fn admitted(value: RelayForwardDispositionV1) -> bool {
    matches!(
        value,
        RelayForwardDispositionV1::Forwarded
            | RelayForwardDispositionV1::QueuedTargetOffline
            | RelayForwardDispositionV1::QueuedBackpressure
    )
}
enum Flight {
    Handshake,
    Data { end: u64, bytes: usize },
    Done,
}

struct ReceiveOrder {
    sequence: u64,
    offset: u64,
    done: bool,
}
impl ReceiveOrder {
    fn open(
        &mut self,
        channel: &mut E2eSecureChannelV1,
        envelope: &SecureNovoRudpEnvelopeV1,
    ) -> WorkResult<Option<Vec<u8>>> {
        if self.done || envelope.sequence != self.sequence {
            return Err(Failure::protocol(
                "relay byte-stream envelope sequence rejected",
            ));
        }
        let frame = channel
            .open_novorudp_frame(envelope)
            .map_err(|_| Failure::protocol("relay byte-stream frame authentication failed"))?;
        if frame.session_id != channel.session_id()
            || frame.stream_id != 0
            || frame.object_id != STREAM_DOMAIN
            || frame.sequence != self.sequence
            || frame.ack_epoch != self.offset
        {
            return Err(Failure::protocol(
                "relay byte-stream frame binding or offset rejected",
            ));
        }
        let result = match frame.kind {
            NovoRudpTransportFrameKindV0::Data
                if !frame.payload.is_empty()
                    && frame.payload.len() <= RELAY_BYTE_STREAM_CHUNK_BYTES_V1 =>
            {
                self.offset = self.offset.checked_add(frame.payload.len() as u64).ok_or(
                    Failure::protocol("relay byte-stream receive offset exhausted"),
                )?;
                Some(frame.payload)
            }
            NovoRudpTransportFrameKindV0::Done if frame.payload.is_empty() => {
                self.done = true;
                None
            }
            _ => return Err(Failure::protocol("relay byte-stream frame kind rejected")),
        };
        self.sequence = self.sequence.checked_add(1).ok_or(Failure::protocol(
            "relay byte-stream receive sequence exhausted",
        ))?;
        Ok(result)
    }
}

fn run_worker(
    shared: &Arc<Shared>,
    identity: SigningKey,
    config: ProductRelayClientConfigV1,
    remote_key: [u8; 32],
    initiator: bool,
    ready: &mut Option<oneshot::Sender<()>>,
) -> WorkResult<()> {
    shared.check_worker()?;
    let weak = Arc::downgrade(shared);
    let control = ProductRelayClientControlV1::new(move || {
        weak.upgrade()
            .ok_or_else(|| Failure::cancelled().error())?
            .check()
    })
    .with_connect_deadline(shared.control.deadline);
    let client = ProductRelayClientV1::connect_with_control(&identity, &config, control)
        .map_err(|_| Failure::protocol("relay byte-stream connection failed"))?;
    shared.check_worker()?;
    let mut relay = client
        .into_pipeline()
        .map_err(|_| Failure::protocol("relay byte-stream pipeline unavailable"))?;
    shared.state().relay_waker = Some(relay.read_waker());
    let local = peer_id_from_ed25519_public_key_v1(&identity.verifying_key().to_bytes());
    let remote = peer_id_from_ed25519_public_key_v1(&remote_key);
    let handshake_deadline = (Instant::now() + HANDSHAKE_LIMIT).min(shared.control.deadline);
    let ttl = handshake_deadline
        .saturating_duration_since(Instant::now())
        .as_millis() as u64;
    let mut handshake = if initiator {
        Some(
            NodeHandshakeInitiatorV1::start(&identity, remote.clone(), now_ms()?, ttl)
                .map_err(|_| Failure::protocol("relay byte-stream offer failed"))?,
        )
    } else {
        None
    };
    let mut pending_handshake = handshake
        .as_ref()
        .map(|value| RelayPeerHandshakeV1::Offer(value.offer().clone()));
    let mut channel: Option<E2eSecureChannelV1> = None;
    let mut replay = HandshakeReplayCacheV1::new(8);
    let mut flight: Option<(u64, Flight)> = None;
    let mut handshake_admitted = false;
    let mut order = ReceiveOrder {
        sequence: 0,
        offset: 0,
        done: false,
    };
    let mut send_sequence = 0u64;
    let mut send_offset = 0u64;
    let mut local_done = false;
    let mut pending_receive = None;
    let mut heartbeat = Instant::now();
    let mut previous_wall = now_ms()?;
    loop {
        shared.check_worker()?;
        let current_wall = now_ms()?;
        if current_wall < previous_wall {
            return Err(Failure::protocol("relay byte-stream clock moved backwards"));
        }
        previous_wall = current_wall;
        if ready.is_some() && Instant::now() >= handshake_deadline {
            return Err(Failure::protocol(
                "relay byte-stream peer handshake expired",
            ));
        }
        if let Some(bytes) = pending_receive.take() {
            if !queue_received(shared, bytes, &mut pending_receive)? {
                // Preserve exactly one bounded decrypted chunk. Do not poll
                // another delivery until the consumer makes room; no TTL/drop.
                thread::sleep(
                    POLL_INTERVAL.min(
                        shared
                            .control
                            .deadline
                            .saturating_duration_since(Instant::now()),
                    ),
                );
                continue;
            }
        }
        let mut progressed =
            match relay
                .poll()
                .map_err(|_| Failure::protocol("relay byte-stream transport failed"))?
            {
                PipelineProgress::Event(event) => {
                    match *event {
                        ProductRelayClientEventV1::PeerHandshake(delivery) => {
                            if delivery.source_peer_id != remote
                                || delivery.target_peer_id != local
                                || channel.is_some()
                            {
                                return Err(Failure::protocol(
                                    "relay byte-stream unexpected peer handshake",
                                ));
                            }
                            match delivery.handshake {
                                RelayPeerHandshakeV1::Offer(offer) if !initiator => {
                                    if offer.initiator_identity_public_key != remote_key {
                                        return Err(Failure::protocol(
                                            "relay byte-stream wrong peer pin",
                                        ));
                                    }
                                    let responder = NodeHandshakeResponderV1::respond(
                                        &offer,
                                        &identity,
                                        current_wall,
                                        ttl,
                                        &mut replay,
                                    )
                                    .map_err(|_| {
                                        Failure::protocol(
                                            "relay byte-stream offer authentication failed",
                                        )
                                    })?;
                                    pending_handshake = Some(RelayPeerHandshakeV1::Response(
                                        responder.response().clone(),
                                    ));
                                    channel = Some(responder.into_channel());
                                }
                                RelayPeerHandshakeV1::Response(response) if initiator => {
                                    channel = Some(
                                        handshake
                                            .take()
                                            .ok_or(Failure::protocol(
                                                "relay byte-stream duplicate response",
                                            ))?
                                            .complete(&response, current_wall, &mut replay)
                                            .map_err(|_| {
                                                Failure::protocol(
                                                "relay byte-stream response authentication failed",
                                            )
                                            })?,
                                    );
                                }
                                _ => {
                                    return Err(Failure::protocol(
                                        "relay byte-stream handshake role mismatch",
                                    ))
                                }
                            }
                        }
                        ProductRelayClientEventV1::Delivery(delivery) => {
                            if delivery.source_peer_id != remote || delivery.target_peer_id != local
                            {
                                return Err(Failure::protocol(
                                    "relay byte-stream wrong delivery route",
                                ));
                            }
                            let channel = channel.as_mut().ok_or(Failure::protocol(
                                "relay byte-stream unauthenticated data",
                            ))?;
                            match order.open(channel, &delivery.envelope)? {
                                Some(bytes) => {
                                    queue_received(shared, bytes, &mut pending_receive)?;
                                }
                                None => {
                                    shared.state().remote_done = true;
                                    shared.wake();
                                }
                            }
                        }
                        ProductRelayClientEventV1::HeartbeatAck => {}
                        ProductRelayClientEventV1::Closed => {
                            return Err(Failure::protocol(
                                "relay byte-stream unauthenticated relay EOF",
                            ))
                        }
                    }
                    true
                }
                PipelineProgress::Forward { ticket, outcome } => {
                    let (expected, submitted) = flight.take().ok_or(Failure::protocol(
                        "relay byte-stream unexpected forward result",
                    ))?;
                    if ticket != expected || !admitted(outcome.disposition) {
                        return Err(Failure::protocol("relay byte-stream forward rejected"));
                    }
                    // Pipeline already verifies source/target/session/envelope-seq.
                    match submitted {
                        Flight::Handshake => {
                            handshake_admitted = true;
                        }
                        Flight::Data { end, bytes } => {
                            let mut state = shared.state();
                            state.confirmed = end;
                            state.outbound_bytes = state.outbound_bytes.checked_sub(bytes).ok_or(
                                Failure::protocol("relay byte-stream queue accounting failed"),
                            )?;
                        }
                        Flight::Done => {
                            shared.state().shutdown_confirmed = true;
                            local_done = true;
                        }
                    }
                    shared.wake();
                    true
                }
                PipelineProgress::Progress => true,
                PipelineProgress::Idle => false,
            };
        shared.check_worker()?;
        if channel.is_some() && handshake_admitted {
            if let Some(ready) = ready.take() {
                if ready.send(()).is_err() {
                    return Err(Failure::cancelled());
                }
            }
        }
        if let Some(item) = pending_handshake.take() {
            if flight.is_none() && relay.can_submit_handshake() {
                let ticket = relay
                    .try_submit_handshake(remote.clone(), item)
                    .map_err(|_| {
                        Failure::protocol("relay byte-stream handshake submission failed")
                    })?
                    .ok_or(Failure::protocol(
                        "relay byte-stream handshake admission changed",
                    ))?;
                flight = Some((ticket, Flight::Handshake));
                progressed = true;
            } else {
                pending_handshake = Some(item);
            }
        }
        if ready.is_none() && flight.is_none() && relay.can_submit() && !local_done {
            let (bytes, closing) = {
                let mut state = shared.state();
                (state.outbound.pop_front(), state.shutdown_requested)
            };
            if let Some(bytes) = bytes {
                let size = bytes.len();
                let end = send_offset
                    .checked_add(size as u64)
                    .ok_or(Failure::protocol("relay byte-stream send offset exhausted"))?;
                let ticket = submit(
                    &mut relay,
                    channel.as_mut().expect("ready channel"),
                    NovoRudpTransportFrameKindV0::Data,
                    send_sequence,
                    send_offset,
                    bytes,
                )?;
                send_sequence = send_sequence.checked_add(1).ok_or(Failure::protocol(
                    "relay byte-stream send sequence exhausted",
                ))?;
                send_offset = end;
                flight = Some((ticket, Flight::Data { end, bytes: size }));
                progressed = true;
            } else if closing {
                let ticket = submit(
                    &mut relay,
                    channel.as_mut().expect("ready channel"),
                    NovoRudpTransportFrameKindV0::Done,
                    send_sequence,
                    send_offset,
                    Vec::new(),
                )?;
                flight = Some((ticket, Flight::Done));
                progressed = true;
            }
        }
        if local_done && order.done && flight.is_none() {
            return Ok(());
        }
        if heartbeat.elapsed() >= Duration::from_secs(5)
            && relay
                .try_heartbeat()
                .map_err(|_| Failure::protocol("relay byte-stream heartbeat failed"))?
        {
            heartbeat = Instant::now();
            progressed = true;
        }
        if !progressed {
            relay
                .wait()
                .map_err(|_| Failure::protocol("relay byte-stream wait failed"))?;
        }
    }
}

fn queue_received(
    shared: &Shared,
    bytes: Vec<u8>,
    pending: &mut Option<Vec<u8>>,
) -> WorkResult<bool> {
    let inserted = {
        let mut state = shared.state();
        if state.consumer_dropped {
            return Err(Failure::protocol(
                "relay byte-stream trailing bytes after IO drop",
            ));
        }
        if state.inbound_bytes + bytes.len() > RELAY_BYTE_STREAM_QUEUE_BYTES_V1 {
            *pending = Some(bytes);
            false
        } else {
            state.inbound_bytes += bytes.len();
            state.inbound.push_back(bytes);
            true
        }
    };
    if inserted {
        shared.wake();
    }
    Ok(inserted)
}

fn submit(
    relay: &mut ProductRelayPipelineV1,
    channel: &mut E2eSecureChannelV1,
    kind: NovoRudpTransportFrameKindV0,
    sequence: u64,
    offset: u64,
    bytes: Vec<u8>,
) -> WorkResult<u64> {
    let frame = NovoRudpTransportFrameV0::new(
        kind,
        channel.session_id(),
        0,
        STREAM_DOMAIN,
        sequence,
        offset,
        bytes,
    );
    let envelope = channel
        .seal_novorudp_frame(&frame)
        .map_err(|_| Failure::protocol("relay byte-stream encryption failed"))?;
    relay
        .try_submit_envelope(envelope)
        .map_err(|_| Failure::protocol("relay byte-stream submission failed"))?
        .ok_or(Failure::protocol("relay byte-stream admission changed"))
}

#[cfg(test)]
#[path = "byte_stream/tests.rs"]
mod tests;
