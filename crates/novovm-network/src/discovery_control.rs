//! Bounded discovery RPC over a Finished-verified NOVOVM carrier.
//! The node stores opaque endpoint leases, never a chat-message queue. This
//! adapter grants neither UCA nor privacy: callers must authorize the path
//! before creating a scope and keep network/identity generations supervised.
use crate::bound_session::BoundSessionStage;
use crate::carrier_stream::{is_carrier_io_failure, CarrierStreamV1};
use crate::{
    duplex::{NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0},
    iroh_transport::{decode_iroh_envelope_v1, encode_iroh_envelope_v1},
    opaque_rendezvous::{RendezvousRecordV1, RendezvousStoreV1},
    transport_binding::BoundSecureChannelV1,
};
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

// Historical deployed wire constants remain exact; ownership moved to NOVOVM.
// Changing these requires an explicit version, never a silent legacy fallback.
const DOMAIN: &[u8] = b"novochat.endpoint-rpc.v1\0";
const STREAM: u64 = 0x4e434453;
const HEADER: usize = 8 + 16 + 32 + 1 + 32 + 8 + 4 + 2;
const MAX_BODY: usize = 4096;
const MAX_REQUESTS: u64 = 32;
const REQUEST: &[u8; 8] = b"NCDISC01";
const RESPONSE: &[u8; 8] = b"NCDISR01";
const PUT: u8 = 1;
const GET: u8 = 2;
const ACK: u8 = 3;
const CLOSE: u8 = 4;
const RECEIPT: u8 = 11;
const RECORD: u8 = 12;
const MISSING: u8 = 13;
const ACKED: u8 = 14;
const NOT_ACKED: u8 = 15;
const CLOSED: u8 = 16;
const REJECTED: u8 = 17;

/// Only the discovery client's own authentication budget can construct this
/// marker. It does not classify carrier, identity, observer or RPC failures as
/// retryable; a caller must still check its overall deadline and authority.
#[derive(Debug, thiserror::Error)]
#[error("discovery authentication deadline expired")]
pub struct DiscoveryAuthenticationTimeout(());

/// Diagnostic observations only: neither stage proves lookup success, peer
/// reachability or delivery. Released means the fixed local pause elapsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryLookupDelayStage {
    Started,
    Released { elapsed_ms: u64 },
}

const DIAGNOSTIC_LOOKUP_DELAY: Duration = Duration::from_secs(10);

/// A typed transport IO error from a carrier operation. The private constructor
/// and upstream IO marker keep framing validation, RPC decoding, AEAD checks,
/// authority gates and local observations out of this category. This is
/// incomplete IO, never proof of a peer's cancellation or permission to retry:
/// the owner must recheck its authority and corroborate diagnostic cancellation
/// with the peer's independently observed closure.
#[derive(Debug, thiserror::Error)]
#[error("discovery service carrier IO failed: {0:#}")]
pub struct DiscoveryServiceTransportFailure(#[source] anyhow::Error);

fn service_io_failure(error: anyhow::Error) -> anyhow::Error {
    if is_carrier_io_failure(&error) {
        DiscoveryServiceTransportFailure(error).into()
    } else {
        error
    }
}

/// Narrow service-side accounting category. In particular an elapsed timer,
/// rejected pin, bad cryptographic/RPC frame, observer failure or unknown error
/// is not a cancellation. Clients must continue treating this marker as terminal.
pub fn is_service_transport_failure(error: &anyhow::Error) -> bool {
    if let Some(error) = error.downcast_ref::<crate::bound_session::BoundSessionError>() {
        return match error {
            crate::bound_session::BoundSessionError::Transport(source) => {
                is_carrier_io_failure(source)
            }
            _ => false,
        };
    }
    error.is::<DiscoveryServiceTransportFailure>() || is_carrier_io_failure(error)
}

// One fixed, canonical header. `slot` carries a response digest; unused fields
// must be zero. Bounds are checked before copying any variable-length body.
struct Message {
    request: [u8; 16],
    instance: [u8; 32],
    kind: u8,
    slot: [u8; 32],
    revision: u64,
    ttl_ms: u32,
    body: Vec<u8>,
}
impl Message {
    fn encode(&self, magic: &[u8; 8]) -> Result<Vec<u8>> {
        ensure!(self.body.len() <= MAX_BODY, "discovery body exceeds limit");
        let mut wire = Vec::with_capacity(HEADER + self.body.len());
        wire.extend_from_slice(magic);
        wire.extend_from_slice(&self.request);
        wire.extend_from_slice(&self.instance);
        wire.push(self.kind);
        wire.extend_from_slice(&self.slot);
        wire.extend_from_slice(&self.revision.to_be_bytes());
        wire.extend_from_slice(&self.ttl_ms.to_be_bytes());
        wire.extend_from_slice(&(self.body.len() as u16).to_be_bytes());
        wire.extend_from_slice(&self.body);
        Ok(wire)
    }
    fn decode(wire: &[u8], magic: &[u8; 8]) -> Result<Self> {
        ensure!(
            (HEADER..=HEADER + MAX_BODY).contains(&wire.len()),
            "discovery wire size"
        );
        ensure!(&wire[..8] == magic, "discovery purpose/version differs");
        let length = u16::from_be_bytes(wire[HEADER - 2..HEADER].try_into()?) as usize;
        ensure!(
            wire.len() == HEADER + length,
            "discovery noncanonical length"
        );
        Ok(Self {
            request: wire[8..24].try_into()?,
            instance: wire[24..56].try_into()?,
            kind: wire[56],
            slot: wire[57..89].try_into()?,
            revision: u64::from_be_bytes(wire[89..97].try_into()?),
            ttl_ms: u32::from_be_bytes(wire[97..101].try_into()?),
            body: wire[HEADER..].to_vec(),
        })
    }
    fn empty(&self, kind: u8) -> Self {
        Self {
            request: self.request,
            instance: self.instance,
            kind,
            slot: [0; 32],
            revision: 0,
            ttl_ms: 0,
            body: Vec::new(),
        }
    }
    fn is_empty(&self) -> bool {
        self.slot == [0; 32] && self.revision == 0 && self.ttl_ms == 0 && self.body.is_empty()
    }
}

async fn send(
    stream: &mut CarrierStreamV1<'_>,
    channel: &mut BoundSecureChannelV1,
    sequence: u64,
    message: &Message,
    magic: &[u8; 8],
) -> Result<()> {
    let mut payload = DOMAIN.to_vec();
    payload.extend_from_slice(&message.encode(magic)?);
    let frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        channel.session_id(),
        STREAM,
        sequence,
        sequence,
        0,
        payload,
    );
    let envelope = channel.seal_novorudp_frame(&frame)?;
    ensure!(
        envelope.sequence == sequence,
        "discovery send sequence differs"
    );
    stream
        .write_frame(&encode_iroh_envelope_v1(&envelope)?)
        .await
        .map_err(service_io_failure)
}
async fn receive(
    stream: &mut CarrierStreamV1<'_>,
    channel: &mut BoundSecureChannelV1,
    sequence: u64,
    magic: &[u8; 8],
) -> Result<Message> {
    let wire = stream.read_frame().await.map_err(service_io_failure)?;
    let envelope = decode_iroh_envelope_v1(&wire)?;
    ensure!(
        envelope.sequence == sequence,
        "discovery receive sequence differs"
    );
    let frame = channel.open_novorudp_frame(&envelope)?;
    ensure!(
        frame
            == NovoRudpTransportFrameV0::new(
                NovoRudpTransportFrameKindV0::Data,
                channel.session_id(),
                STREAM,
                sequence,
                sequence,
                0,
                frame.payload.clone()
            ),
        "discovery route differs"
    );
    Message::decode(
        frame
            .payload
            .strip_prefix(DOMAIN)
            .context("discovery domain differs")?,
        magic,
    )
}

/// A single bounded authenticated session. Dropping a partially completed RPC
/// requires dropping its carrier: requests are not retried on this stream.
pub struct DiscoveryClient<'a, 'scope> {
    stream: &'a mut CarrierStreamV1<'scope>,
    channel: BoundSecureChannelV1,
    instance: [u8; 32],
    sequence: u64,
    failed: bool,
}
impl<'a, 'scope> DiscoveryClient<'a, 'scope> {
    pub async fn connect(
        stream: &'a mut CarrierStreamV1<'scope>,
        identity: &SigningKey,
        pinned_node: [u8; 32],
        instance: [u8; 32],
    ) -> Result<Self> {
        Self::connect_observed(stream, identity, pinned_node, instance, &mut |_, _| Ok(())).await
    }

    /// The observer receives completed authentication stages and a current path
    /// class only. It must be fast and nonblocking; an error prevents exposing
    /// an RPC client. The same stream is authenticated and used for all RPCs.
    pub async fn connect_observed<O>(
        stream: &'a mut CarrierStreamV1<'scope>,
        identity: &SigningKey,
        pinned_node: [u8; 32],
        instance: [u8; 32],
        observer: &mut O,
    ) -> Result<Self>
    where
        O: FnMut(BoundSessionStage, &'static str) -> Result<()> + Send + ?Sized,
    {
        Self::connect_observed_with_timeout(
            stream,
            identity,
            pinned_node,
            instance,
            observer,
            Duration::from_secs(8),
        )
        .await
    }

    // Keep the public budget fixed. A shorter private budget lets local socket
    // tests exercise the real incomplete handshake without eight-second sleeps.
    async fn connect_observed_with_timeout<O>(
        stream: &'a mut CarrierStreamV1<'scope>,
        identity: &SigningKey,
        pinned_node: [u8; 32],
        instance: [u8; 32],
        observer: &mut O,
        timeout: Duration,
    ) -> Result<Self>
    where
        O: FnMut(BoundSessionStage, &'static str) -> Result<()> + Send + ?Sized,
    {
        ensure!(instance != [0; 32], "empty discovery instance");
        let channel = tokio::time::timeout(
            timeout,
            crate::bound_session::authenticate_bound_session_observed(
                stream,
                identity,
                pinned_node,
                true,
                &mut crate::duplex::HandshakeReplayCacheV1::new(8),
                observer,
            ),
        )
        .await
        // Map only this wrapper's Elapsed, never an error from authentication
        // or its observer. Such errors remain terminal in their original type.
        .map_err(|_| DiscoveryAuthenticationTimeout(()))??;
        Ok(Self {
            stream,
            channel,
            instance,
            sequence: 0,
            failed: false,
        })
    }
    async fn request(
        &mut self,
        kind: u8,
        slot: [u8; 32],
        revision: u64,
        ttl_ms: u32,
        body: Vec<u8>,
    ) -> Result<Message> {
        ensure!(
            !self.failed && self.sequence < MAX_REQUESTS,
            "discovery session exhausted"
        );
        // Latch before any cancellable await. Only a whole authenticated reply
        // clears it, so cancellation cannot leave a reusable half-frame stream.
        self.failed = true;
        self.sequence += 1;
        let mut request = [0u8; 16];
        rand::RngCore::try_fill_bytes(&mut rand::rngs::OsRng, &mut request)
            .map_err(|_| anyhow::anyhow!("OS entropy unavailable"))?;
        let message = Message {
            request,
            instance: self.instance,
            kind,
            slot,
            revision,
            ttl_ms,
            body,
        };
        send(
            self.stream,
            &mut self.channel,
            self.sequence,
            &message,
            REQUEST,
        )
        .await?;
        let reply = receive(self.stream, &mut self.channel, self.sequence, RESPONSE).await?;
        ensure!(
            reply.instance == self.instance && reply.request == request,
            "discovery reply binding differs"
        );
        ensure!(reply.kind != REJECTED, "discovery node rejected request");
        Ok(reply)
    }
    pub async fn put(
        &mut self,
        slot: [u8; 32],
        revision: u64,
        ttl_ms: u32,
        ciphertext: Vec<u8>,
    ) -> Result<()> {
        ensure!(
            revision > 0 && (1..=30_000).contains(&ttl_ms),
            "invalid discovery lease"
        );
        ensure!(
            !ciphertext.is_empty() && ciphertext.len() <= MAX_BODY,
            "discovery payload limit"
        );
        let digest: [u8; 32] = Sha256::digest(&ciphertext).into();
        let reply = self
            .request(PUT, slot, revision, ttl_ms, ciphertext)
            .await?;
        ensure!(
            reply.kind == RECEIPT
                && reply.revision == revision
                && reply.slot == digest
                && reply.ttl_ms > 0
                && reply.ttl_ms <= ttl_ms
                && reply.body.is_empty(),
            "discovery receipt differs"
        );
        self.failed = false;
        Ok(())
    }
    pub async fn get(&mut self, slot: [u8; 32]) -> Result<Option<RendezvousRecordV1>> {
        let reply = self.request(GET, slot, 0, 0, Vec::new()).await?;
        let record = if reply.kind == MISSING {
            ensure!(reply.is_empty(), "nonempty missing reply");
            None
        } else {
            ensure!(
                reply.kind == RECORD
                    && reply.revision > 0
                    && (1..=30_000).contains(&reply.ttl_ms)
                    && !reply.body.is_empty()
                    && reply.slot == <[u8; 32]>::from(Sha256::digest(&reply.body)),
                "invalid discovery record"
            );
            Some(RendezvousRecordV1 {
                revision: reply.revision,
                digest: reply.slot,
                remaining_ttl_ms: u64::from(reply.ttl_ms),
                ciphertext: reply.body,
            })
        };
        self.failed = false;
        Ok(record)
    }
    pub async fn ack(&mut self, slot: [u8; 32], revision: u64, digest: [u8; 32]) -> Result<bool> {
        let reply = self
            .request(ACK, slot, revision, 0, digest.to_vec())
            .await?;
        ensure!(
            matches!(reply.kind, ACKED | NOT_ACKED) && reply.is_empty(),
            "invalid discovery ack"
        );
        self.failed = false;
        Ok(reply.kind == ACKED)
    }
    pub async fn close(mut self) -> Result<()> {
        let reply = self.request(CLOSE, [0; 32], 0, 0, Vec::new()).await?;
        ensure!(
            reply.kind == CLOSED && reply.is_empty(),
            "invalid discovery close"
        );
        self.stream.finish().await.map_err(service_io_failure)
    }
}

/// Authenticates this stream against the allowlist, then serves its bound owner.
/// Returns that owner only after the full session closes. The node owner must
/// maintain store expiry even while clients idle. Global store limits and the
/// enclosing carrier authority remain effective across all client sessions.
pub async fn serve_rendezvous(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    allowed: &[[u8; 32]],
    store: Arc<Mutex<RendezvousStoreV1>>,
) -> Result<[u8; 32]> {
    serve_rendezvous_observed(stream, identity, allowed, store, &mut |_, _| Ok(())).await
}

/// Observe completed authentication stages before allowing any RPC or storage
/// operation. Observer failures terminate the session; they do not authorize a
/// retry or a weaker handshake. Observation does not change the eight-second
/// authentication budget or the same-stream identity binding.
pub async fn serve_rendezvous_observed<O>(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    allowed: &[[u8; 32]],
    store: Arc<Mutex<RendezvousStoreV1>>,
    observer: &mut O,
) -> Result<[u8; 32]>
where
    O: FnMut(BoundSessionStage, &'static str) -> Result<()> + Send + ?Sized,
{
    serve_rendezvous_inner(
        stream,
        identity,
        allowed,
        store,
        observer,
        None,
        &mut |_| Ok(()),
    )
    .await
}

/// Explicit fault fixture for the native diagnostic, never the normal service.
/// After authentication and any immediate PUTs, pause exactly once for ten
/// seconds before the first well-formed GET looks up its record. No lease,
/// authentication, request or enclosing scope deadline is extended. The caller
/// must continue supervising its carrier and maintaining the store's TTLs.
/// Both observers must be fast and nonblocking; any observer error is terminal.
pub async fn serve_rendezvous_delayed_lookup_for_diagnostic<O, D>(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    allowed: &[[u8; 32]],
    store: Arc<Mutex<RendezvousStoreV1>>,
    observer: &mut O,
    delay_observer: &mut D,
) -> Result<[u8; 32]>
where
    O: FnMut(BoundSessionStage, &'static str) -> Result<()> + Send + ?Sized,
    D: FnMut(DiscoveryLookupDelayStage) -> Result<()> + Send + ?Sized,
{
    serve_rendezvous_inner(
        stream,
        identity,
        allowed,
        store,
        observer,
        Some(DIAGNOSTIC_LOOKUP_DELAY),
        delay_observer,
    )
    .await
}

// Only the explicit public diagnostic chooses a delay. Keeping this private
// permits short real-socket tests without an unbounded runtime delay option.
async fn serve_rendezvous_inner<O, D>(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    allowed: &[[u8; 32]],
    store: Arc<Mutex<RendezvousStoreV1>>,
    observer: &mut O,
    mut lookup_delay: Option<Duration>,
    delay_observer: &mut D,
) -> Result<[u8; 32]>
where
    O: FnMut(BoundSessionStage, &'static str) -> Result<()> + Send + ?Sized,
    D: FnMut(DiscoveryLookupDelayStage) -> Result<()> + Send + ?Sized,
{
    let (mut channel, owner) = tokio::time::timeout(
        Duration::from_secs(8),
        crate::bound_session::authenticate_allowed_service_client_observed(
            stream,
            identity,
            allowed,
            &mut crate::duplex::HandshakeReplayCacheV1::new(8),
            observer,
        ),
    )
    .await??;
    for sequence in 1..=MAX_REQUESTS {
        let request = receive(stream, &mut channel, sequence, REQUEST).await?;
        if request.kind == GET
            && request.revision == 0
            && request.ttl_ms == 0
            && request.body.is_empty()
        {
            if let Some(delay) = lookup_delay.take() {
                // Reject a wrong/expired node instance before reporting a valid
                // delayed lookup. Never retain this lock across the pause.
                {
                    let mut store = store
                        .lock()
                        .map_err(|_| anyhow::anyhow!("discovery store lock poisoned"))?;
                    ensure!(
                        request.instance == store.instance(),
                        "discovery instance differs"
                    );
                    store.stats(Instant::now())?;
                }
                stream.selected_path()?;
                let started = Instant::now();
                delay_observer(DiscoveryLookupDelayStage::Started).map_err(|error| {
                    crate::bound_session::BoundSessionError::Local(
                        error.context("discovery diagnostic delay observer failed"),
                    )
                })?;
                // Observers may revoke local authority without returning Err.
                stream.selected_path()?;
                tokio::time::sleep(delay).await;
                stream.selected_path()?;
                let elapsed = started.elapsed();
                ensure!(
                    elapsed >= delay,
                    "discovery diagnostic delay completed early"
                );
                delay_observer(DiscoveryLookupDelayStage::Released {
                    elapsed_ms: elapsed.as_millis().try_into()?,
                })
                .map_err(|error| {
                    crate::bound_session::BoundSessionError::Local(
                        error.context("discovery diagnostic delay observer failed"),
                    )
                })?;
                stream.selected_path()?;
            }
        }
        let response = {
            let mut store = store
                .lock()
                .map_err(|_| anyhow::anyhow!("discovery store lock poisoned"))?;
            ensure!(
                request.instance == store.instance(),
                "discovery instance differs"
            );
            let now = Instant::now();
            // Even CLOSE and malformed operations do not bypass instance expiry.
            store.stats(now)?;
            let mut reply = request.empty(REJECTED);
            match request.kind {
                PUT if request.revision > 0 && (1..=30_000).contains(&request.ttl_ms) => {
                    if let Ok(receipt) = store.put(
                        owner,
                        request.slot,
                        request.revision,
                        Duration::from_millis(u64::from(request.ttl_ms)),
                        request.body,
                        now,
                    ) {
                        reply.kind = RECEIPT;
                        reply.revision = receipt.revision;
                        reply.slot = receipt.digest;
                        reply.ttl_ms = receipt.remaining_ttl_ms.try_into()?;
                    }
                }
                GET if request.revision == 0 && request.ttl_ms == 0 && request.body.is_empty() => {
                    if let Some(record) = store.get(request.slot, now)? {
                        reply.kind = RECORD;
                        reply.revision = record.revision;
                        reply.slot = record.digest;
                        reply.ttl_ms = record.remaining_ttl_ms.try_into()?;
                        reply.body = record.ciphertext;
                    } else {
                        reply.kind = MISSING;
                    }
                }
                ACK if request.revision > 0 && request.ttl_ms == 0 && request.body.len() == 32 => {
                    reply.kind = if store.ack(
                        request.slot,
                        request.revision,
                        request.body.as_slice().try_into()?,
                        now,
                    )? {
                        ACKED
                    } else {
                        NOT_ACKED
                    };
                }
                CLOSE if request.is_empty() => {
                    reply.kind = CLOSED;
                }
                _ => {}
            }
            reply
        };
        // Preserve an already established protocol rejection even if the peer
        // disconnects before receiving it. An IO failure sending REJECTED must
        // not turn a malformed request into a diagnostic aborted session.
        let sent = send(stream, &mut channel, sequence, &response, RESPONSE).await;
        ensure!(response.kind != REJECTED, "invalid discovery request");
        sent?;
        if response.kind == CLOSED {
            stream.finish().await.map_err(service_io_failure)?;
            return Ok(owner);
        }
    }
    anyhow::bail!("discovery request budget exhausted")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bound_session::BoundSessionError;
    use crate::iroh_transport::{with_iroh_scope_v1, IrohAddressV1, IrohConfigV1, IrohControlV1};
    use crate::transport_binding::BoundNodeHandshakeOfferV1;
    use tokio::sync::oneshot;

    use crate::SERVICE_TEST_SERIAL as SERIAL;

    fn local_config() -> IrohConfigV1 {
        IrohConfigV1::new(&[], "127.0.0.1".parse().unwrap())
            .unwrap()
            .with_bind_address("127.0.0.1:0".parse().unwrap())
            .unwrap()
    }

    #[tokio::test]
    async fn real_quic_diagnostic_delay_precedes_lookup_once_without_holding_store() -> Result<()> {
        let _permit = SERIAL.acquire().await?;
        assert_eq!(DIAGNOSTIC_LOOKUP_DELAY, Duration::from_secs(10));
        let client_identity = SigningKey::from_bytes(&[101; 32]);
        let node_identity = SigningKey::from_bytes(&[102; 32]);
        let client_pin = client_identity.verifying_key().to_bytes();
        let node_pin = node_identity.verifying_key().to_bytes();
        let instance = [103; 32];
        let first_slot = [104; 32];
        let second_slot = [105; 32];
        let body = vec![106; 64];
        let first_digest: [u8; 32] = Sha256::digest(&body).into();
        let deadline = Instant::now() + Duration::from_secs(10);
        let delay = Duration::from_millis(250);
        let store = Arc::new(Mutex::new(RendezvousStoreV1::new(instance, deadline)?));
        let events = Arc::new(Mutex::new(Vec::new()));
        let node_store = store.clone();
        let janitor_store = store.clone();
        let node_events = events.clone();
        let client_events = events.clone();
        let (node_tx, node_rx) = oneshot::channel();
        let (client_tx, client_rx) = oneshot::channel();
        let (started_tx, started_rx) = oneshot::channel();
        let node = with_iroh_scope_v1(
            local_config(),
            IrohControlV1::new(deadline, || Ok(()))?,
            move |scope| {
                Box::pin(async move {
                    node_tx.send(scope.address()?).unwrap();
                    let peer: IrohAddressV1 = client_rx.await?;
                    let mut stream: CarrierStreamV1<'_> =
                        (scope.accept(peer.endpoint_key).await?).into();
                    let mut started_tx = Some(started_tx);
                    let owner = serve_rendezvous_inner(
                        &mut stream,
                        &node_identity,
                        &[client_pin],
                        node_store,
                        &mut |_, _| Ok(()),
                        Some(delay),
                        &mut |stage| {
                            node_events.lock().unwrap().push(stage);
                            if stage == DiscoveryLookupDelayStage::Started {
                                started_tx.take().expect("delay repeated").send(()).unwrap();
                            }
                            Ok(())
                        },
                    )
                    .await?;
                    ensure!(owner == client_pin);
                    Ok(())
                })
            },
        );
        let client = with_iroh_scope_v1(
            local_config(),
            IrohControlV1::new(deadline, || Ok(()))?,
            move |scope| {
                Box::pin(async move {
                    client_tx.send(scope.address()?).unwrap();
                    let mut stream: CarrierStreamV1<'_> =
                        (scope.connect(&node_rx.await?).await?).into();
                    let mut client =
                        DiscoveryClient::connect(&mut stream, &client_identity, node_pin, instance)
                            .await?;
                    ensure!(client_events.lock().unwrap().is_empty());
                    client.put(first_slot, 1, 30_000, body.clone()).await?;
                    // Authentication and publication must not enter the pause.
                    ensure!(client_events.lock().unwrap().is_empty());
                    ensure!(client.get(first_slot).await?.is_none());
                    client.put(second_slot, 1, 30_000, body.clone()).await?;
                    let second = client
                        .get(second_slot)
                        .await?
                        .context("second record missing")?;
                    ensure!(second.ciphertext == body);
                    ensure!(
                        client
                            .ack(second_slot, second.revision, second.digest)
                            .await?
                    );
                    client.close().await
                })
            },
        );
        let janitor = async move {
            started_rx.await?;
            // Independent maintenance must acquire the store during the actual
            // pause. Removing a real published record here makes an early
            // store.get observably wrong: the client must receive MISSING.
            let mut store = janitor_store
                .try_lock()
                .map_err(|_| anyhow::anyhow!("diagnostic pause held the store lock"))?;
            ensure!(store.stats(Instant::now())?.live_records == 1);
            ensure!(store.ack(first_slot, 1, first_digest, Instant::now())?);
            Ok::<(), anyhow::Error>(())
        };
        let (node, client, janitor) = tokio::join!(node, client, janitor);
        node?;
        client?;
        janitor?;
        assert!(matches!(events.lock().unwrap().as_slice(), [
            DiscoveryLookupDelayStage::Started,
            DiscoveryLookupDelayStage::Released { elapsed_ms }
        ] if *elapsed_ms >= delay.as_millis() as u64));
        assert_eq!(store.lock().unwrap().stats(Instant::now())?.live_records, 0);
        Ok(())
    }

    #[derive(Clone, Copy)]
    enum LookupDelayFailureFixture {
        MalformedRevision,
        MalformedTtl,
        MalformedBody,
        WrongInstance,
        ObserverStarted,
        ObserverReleased,
        CancelDuringPause,
        RevokeAtReleased,
    }

    async fn lookup_delay_failure_fixture(
        fixture: LookupDelayFailureFixture,
    ) -> Result<(anyhow::Error, Vec<DiscoveryLookupDelayStage>)> {
        use std::sync::atomic::{AtomicBool, Ordering};

        let _permit = SERIAL.acquire().await?;
        let client_identity = SigningKey::from_bytes(&[111; 32]);
        let node_identity = SigningKey::from_bytes(&[112; 32]);
        let client_pin = client_identity.verifying_key().to_bytes();
        let node_pin = node_identity.verifying_key().to_bytes();
        let instance = [113; 32];
        let deadline = Instant::now() + Duration::from_secs(10);
        let store = Arc::new(Mutex::new(RendezvousStoreV1::new(instance, deadline)?));
        let events = Arc::new(Mutex::new(Vec::new()));
        let node_events = events.clone();
        let revoked = Arc::new(AtomicBool::new(false));
        let gate_revoked = revoked.clone();
        let observer_revoked = revoked.clone();
        let (node_tx, node_rx) = oneshot::channel();
        let (client_tx, client_rx) = oneshot::channel();
        let (started_tx, started_rx) = oneshot::channel();
        let (node_done_tx, node_done_rx) = oneshot::channel();
        let node = with_iroh_scope_v1(
            local_config(),
            IrohControlV1::new(deadline, move || {
                ensure!(
                    !gate_revoked.load(Ordering::Acquire),
                    "lookup fixture revoked"
                );
                Ok(())
            })?,
            move |scope| {
                Box::pin(async move {
                    node_tx.send(scope.address()?).unwrap();
                    let peer: IrohAddressV1 = client_rx.await?;
                    let mut stream: CarrierStreamV1<'_> =
                        (scope.accept(peer.endpoint_key).await?).into();
                    let mut started_tx = Some(started_tx);
                    serve_rendezvous_inner(
                        &mut stream,
                        &node_identity,
                        &[client_pin],
                        store,
                        &mut |_, _| Ok(()),
                        Some(Duration::from_millis(150)),
                        &mut |stage| {
                            node_events.lock().unwrap().push(stage);
                            if stage == DiscoveryLookupDelayStage::Started {
                                started_tx.take().unwrap().send(()).unwrap();
                            }
                            if matches!(
                                (fixture, stage),
                                (
                                    LookupDelayFailureFixture::ObserverStarted,
                                    DiscoveryLookupDelayStage::Started
                                ) | (
                                    LookupDelayFailureFixture::ObserverReleased,
                                    DiscoveryLookupDelayStage::Released { .. }
                                )
                            ) {
                                return Err(ObserverRefused.into());
                            }
                            if matches!(
                                (fixture, stage),
                                (
                                    LookupDelayFailureFixture::RevokeAtReleased,
                                    DiscoveryLookupDelayStage::Released { .. }
                                )
                            ) {
                                observer_revoked.store(true, Ordering::Release);
                            }
                            Ok(())
                        },
                    )
                    .await?;
                    Ok(())
                })
            },
        );
        let node = async move {
            let result = node.await;
            // Aborting a revoked/error scope does not promise timely delivery
            // of a QUIC close packet. Test local termination independently of
            // whether the peer has already observed its transport's EOF.
            let _ = node_done_tx.send(());
            result
        };
        let client = with_iroh_scope_v1(
            local_config(),
            IrohControlV1::new(deadline, || Ok(()))?,
            move |scope| {
                Box::pin(async move {
                    client_tx.send(scope.address()?).unwrap();
                    let mut stream: CarrierStreamV1<'_> =
                        (scope.connect(&node_rx.await?).await?).into();
                    let mut client =
                        DiscoveryClient::connect(&mut stream, &client_identity, node_pin, instance)
                            .await?;
                    let revision = u64::from(matches!(
                        fixture,
                        LookupDelayFailureFixture::MalformedRevision
                    ));
                    let ttl_ms =
                        u32::from(matches!(fixture, LookupDelayFailureFixture::MalformedTtl));
                    let body = if matches!(fixture, LookupDelayFailureFixture::MalformedBody) {
                        vec![1]
                    } else {
                        Vec::new()
                    };
                    if matches!(fixture, LookupDelayFailureFixture::WrongInstance) {
                        client.instance = [114; 32];
                    }
                    let request = client.request(GET, [115; 32], revision, ttl_ms, body);
                    tokio::pin!(request);
                    tokio::select! {
                        biased;
                        reply = &mut request => ensure!(reply.is_err(),
                            "rejected diagnostic lookup returned a successful reply"),
                        ended = node_done_rx => ended?,
                    }
                    // If local node termination won, dropping this pending
                    // request also drops its entire client stream below. Never
                    // restart a partially received response on the same stream.
                    Ok(())
                })
            },
        );
        let cancellation = async move {
            let started = started_rx.await;
            if matches!(fixture, LookupDelayFailureFixture::CancelDuringPause) {
                started?;
                tokio::time::sleep(Duration::from_millis(20)).await;
                revoked.store(true, Ordering::Release);
            }
            Ok::<(), anyhow::Error>(())
        };
        let (node, client, cancellation) = tokio::join!(node, client, cancellation);
        client?;
        cancellation?;
        let error = node.expect_err("fault fixture returned a successful service");
        let events = events.lock().unwrap().clone();
        Ok((error, events))
    }

    #[tokio::test]
    async fn real_quic_malformed_get_never_enters_diagnostic_delay() -> Result<()> {
        for fixture in [
            LookupDelayFailureFixture::MalformedRevision,
            LookupDelayFailureFixture::MalformedTtl,
            LookupDelayFailureFixture::MalformedBody,
            LookupDelayFailureFixture::WrongInstance,
        ] {
            let (error, events) = lookup_delay_failure_fixture(fixture).await?;
            assert!(events.is_empty());
            assert!(!is_service_transport_failure(&error), "{error:#}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn real_quic_diagnostic_delay_observer_errors_remain_terminal() -> Result<()> {
        for fixture in [
            LookupDelayFailureFixture::ObserverStarted,
            LookupDelayFailureFixture::ObserverReleased,
        ] {
            let (error, events) = lookup_delay_failure_fixture(fixture).await?;
            assert!(matches!(error.downcast_ref::<BoundSessionError>(),
                Some(BoundSessionError::Local(source)) if source.is::<ObserverRefused>()));
            assert!(!is_service_transport_failure(&error), "{error:#}");
            assert_eq!(events.first(), Some(&DiscoveryLookupDelayStage::Started));
            assert_eq!(
                events.len(),
                if matches!(fixture, LookupDelayFailureFixture::ObserverStarted) {
                    1
                } else {
                    2
                }
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn real_quic_scope_revocation_during_diagnostic_delay_never_releases_lookup() -> Result<()>
    {
        for fixture in [
            LookupDelayFailureFixture::CancelDuringPause,
            LookupDelayFailureFixture::RevokeAtReleased,
        ] {
            let (error, events) = lookup_delay_failure_fixture(fixture).await?;
            assert!(error.to_string().contains("lookup fixture revoked"));
            assert!(!is_service_transport_failure(&error));
            assert_eq!(events.first(), Some(&DiscoveryLookupDelayStage::Started));
            assert_eq!(
                events.len(),
                if matches!(fixture, LookupDelayFailureFixture::CancelDuringPause) {
                    1
                } else {
                    2
                }
            );
        }
        Ok(())
    }

    #[derive(Clone, Copy)]
    enum ClientRejectionFixture {
        IncompleteHandshake,
        ObserverElapsed,
        ObserverRefusal,
        MalformedResponse,
    }

    #[derive(Debug, thiserror::Error)]
    #[error("local test observer refused")]
    struct ObserverRefused;

    async fn client_rejection_fixture(
        fixture: ClientRejectionFixture,
    ) -> Result<(anyhow::Error, Vec<BoundSessionStage>)> {
        let _permit = SERIAL.acquire().await?;
        // Real local QUIC streams with explicit fixture keys, no relay or DNS.
        // The node receives an actual bound offer before the test may complete.
        let client_identity = SigningKey::from_bytes(&[81; 32]);
        let node_pin = SigningKey::from_bytes(&[82; 32]).verifying_key().to_bytes();
        let deadline = Instant::now() + Duration::from_secs(10);
        let (node_tx, node_rx) = oneshot::channel();
        let (client_tx, client_rx) = oneshot::channel();
        let (offer_tx, offer_rx) = oneshot::channel();
        let (done_tx, done_rx) = oneshot::channel();
        let node = with_iroh_scope_v1(
            local_config(),
            IrohControlV1::new(deadline, || Ok(()))?,
            move |scope| {
                Box::pin(async move {
                    node_tx.send(scope.address()?).unwrap();
                    let peer: IrohAddressV1 = client_rx.await?;
                    let mut stream: CarrierStreamV1<'_> =
                        (scope.accept(peer.endpoint_key).await?).into();
                    BoundNodeHandshakeOfferV1::decode_json(&stream.read_frame().await?)?;
                    offer_tx.send(()).unwrap();
                    if matches!(fixture, ClientRejectionFixture::MalformedResponse) {
                        stream.write_frame(b"{}").await?;
                    }
                    // Keep the carrier alive until the client has classified
                    // its error. A remote drop must not cause the timeout case.
                    done_rx.await?;
                    Ok(())
                })
            },
        );
        let client = with_iroh_scope_v1(
            local_config(),
            IrohControlV1::new(deadline, || Ok(()))?,
            move |scope| {
                Box::pin(async move {
                    client_tx.send(scope.address()?).unwrap();
                    let mut stream: CarrierStreamV1<'_> =
                        (scope.connect(&node_rx.await?).await?).into();
                    // This Elapsed came from an unrelated operation. Returning
                    // it through an observer must not grant a discovery retry.
                    let mut observer_elapsed = Some(
                        tokio::time::timeout(Duration::ZERO, std::future::pending::<()>())
                            .await
                            .expect_err("pending operation completed"),
                    );
                    let mut stages = Vec::new();
                    let result = DiscoveryClient::connect_observed_with_timeout(
                        &mut stream,
                        &client_identity,
                        node_pin,
                        [83; 32],
                        &mut |stage, _| {
                            stages.push(stage);
                            match fixture {
                                ClientRejectionFixture::ObserverElapsed => {
                                    Err(observer_elapsed.take().unwrap().into())
                                }
                                ClientRejectionFixture::ObserverRefusal => {
                                    Err(ObserverRefused.into())
                                }
                                _ => Ok(()),
                            }
                        },
                        Duration::from_secs(1),
                    )
                    .await;
                    let error = match result {
                        Err(error) => error,
                        Ok(_) => {
                            anyhow::bail!("incomplete or rejected handshake exposed an RPC client")
                        }
                    };
                    offer_rx.await?;
                    done_tx.send(()).unwrap();
                    Ok((error, stages))
                })
            },
        );
        let (node, client) = tokio::join!(node, client);
        node?;
        client
    }

    #[tokio::test]
    async fn real_quic_incomplete_authentication_has_only_local_timeout_marker() -> Result<()> {
        let (error, stages) =
            client_rejection_fixture(ClientRejectionFixture::IncompleteHandshake).await?;
        assert!(
            error.is::<DiscoveryAuthenticationTimeout>(),
            "local authentication deadline was not typed: {error:#}"
        );
        assert!(!error.is::<tokio::time::error::Elapsed>());
        assert_eq!(stages, [BoundSessionStage::OfferWritten]);
        Ok(())
    }

    #[tokio::test]
    async fn real_quic_observer_elapsed_cannot_claim_authentication_timeout() -> Result<()> {
        let (error, stages) =
            client_rejection_fixture(ClientRejectionFixture::ObserverElapsed).await?;
        assert!(!error.is::<DiscoveryAuthenticationTimeout>());
        assert!(matches!(error.downcast_ref::<BoundSessionError>(),
            Some(BoundSessionError::Local(source)) if source.is::<tokio::time::error::Elapsed>()));
        assert_eq!(stages, [BoundSessionStage::OfferWritten]);
        Ok(())
    }

    #[tokio::test]
    async fn real_quic_observer_refusal_remains_terminal() -> Result<()> {
        let (error, stages) =
            client_rejection_fixture(ClientRejectionFixture::ObserverRefusal).await?;
        assert!(!error.is::<DiscoveryAuthenticationTimeout>());
        assert!(matches!(error.downcast_ref::<BoundSessionError>(),
            Some(BoundSessionError::Local(source)) if source.is::<ObserverRefused>()));
        assert_eq!(stages, [BoundSessionStage::OfferWritten]);
        Ok(())
    }

    #[tokio::test]
    async fn real_quic_malformed_response_remains_authentication_failure() -> Result<()> {
        let (error, stages) =
            client_rejection_fixture(ClientRejectionFixture::MalformedResponse).await?;
        assert!(!error.is::<DiscoveryAuthenticationTimeout>());
        assert!(!is_service_transport_failure(&error));
        assert!(matches!(
            error.downcast_ref::<BoundSessionError>(),
            Some(BoundSessionError::Authentication(_))
        ));
        assert_eq!(stages, [BoundSessionStage::OfferWritten]);
        Ok(())
    }

    #[derive(Clone, Copy)]
    enum ServiceFailureFixture {
        Disconnect,
        MalformedEnvelope,
        TamperedCiphertext,
        MalformedRpc,
        TrailingAfterClose,
    }

    async fn service_failure_fixture(fixture: ServiceFailureFixture) -> Result<anyhow::Error> {
        let _permit = SERIAL.acquire().await?;
        // These explicit identities and local QUIC sockets belong only to this
        // test. The initial successful GET proves both Finished messages were
        // accepted before exercising the service's RPC/transport distinction.
        let client_identity = SigningKey::from_bytes(&[91; 32]);
        let node_identity = SigningKey::from_bytes(&[92; 32]);
        let client_pin = client_identity.verifying_key().to_bytes();
        let node_pin = node_identity.verifying_key().to_bytes();
        let instance = [93; 32];
        let deadline = Instant::now() + Duration::from_secs(10);
        let store = Arc::new(Mutex::new(RendezvousStoreV1::new(instance, deadline)?));
        let (node_tx, node_rx) = oneshot::channel();
        let (client_tx, client_rx) = oneshot::channel();
        let (done_tx, done_rx) = oneshot::channel();
        let node = with_iroh_scope_v1(
            local_config(),
            IrohControlV1::new(deadline, || Ok(()))?,
            move |scope| {
                Box::pin(async move {
                    node_tx.send(scope.address()?).unwrap();
                    let peer: IrohAddressV1 = client_rx.await?;
                    let mut stream: CarrierStreamV1<'_> =
                        (scope.accept(peer.endpoint_key).await?).into();
                    let error = serve_rendezvous(&mut stream, &node_identity, &[client_pin], store)
                        .await
                        .expect_err("broken or malformed service session completed");
                    done_tx.send(()).unwrap();
                    Ok(error)
                })
            },
        );
        let client = with_iroh_scope_v1(
            local_config(),
            IrohControlV1::new(deadline, || Ok(()))?,
            move |scope| {
                Box::pin(async move {
                    client_tx.send(scope.address()?).unwrap();
                    let mut stream: CarrierStreamV1<'_> =
                        (scope.connect(&node_rx.await?).await?).into();
                    let mut client =
                        DiscoveryClient::connect(&mut stream, &client_identity, node_pin, instance)
                            .await?;
                    ensure!(client.get([94; 32]).await?.is_none());
                    match fixture {
                        ServiceFailureFixture::Disconnect => {
                            drop(client);
                            drop(stream);
                        }
                        ServiceFailureFixture::MalformedEnvelope => {
                            client.stream.write_frame(b"invalid envelope").await?;
                        }
                        ServiceFailureFixture::TamperedCiphertext => {
                            let message = Message {
                                request: [95; 16],
                                instance,
                                kind: GET,
                                slot: [94; 32],
                                revision: 0,
                                ttl_ms: 0,
                                body: Vec::new(),
                            };
                            let mut payload = DOMAIN.to_vec();
                            payload.extend_from_slice(&message.encode(REQUEST)?);
                            let sequence = client.sequence + 1;
                            let frame = NovoRudpTransportFrameV0::new(
                                NovoRudpTransportFrameKindV0::Data,
                                client.channel.session_id(),
                                STREAM,
                                sequence,
                                sequence,
                                0,
                                payload,
                            );
                            let mut envelope = client.channel.seal_novorudp_frame(&frame)?;
                            envelope.ciphertext[0] ^= 1;
                            client
                                .stream
                                .write_frame(&encode_iroh_envelope_v1(&envelope)?)
                                .await?;
                        }
                        ServiceFailureFixture::MalformedRpc => {
                            let request = Message {
                                request: [95; 16],
                                instance,
                                kind: 255,
                                slot: [0; 32],
                                revision: 0,
                                ttl_ms: 0,
                                body: Vec::new(),
                            };
                            send(
                                client.stream,
                                &mut client.channel,
                                client.sequence + 1,
                                &request,
                                REQUEST,
                            )
                            .await?;
                        }
                        ServiceFailureFixture::TrailingAfterClose => {
                            let reply = client.request(CLOSE, [0; 32], 0, 0, Vec::new()).await?;
                            ensure!(reply.kind == CLOSED && reply.is_empty());
                            // The server's finish must reject real trailing data,
                            // not classify its validation error as disconnect.
                            // It may close the carrier as soon as the first byte
                            // arrives, so the writer itself may fail as well.
                            let _ = client.stream.write_frame(b"unexpected trailing data").await;
                        }
                    }
                    done_rx.await?;
                    Ok(())
                })
            },
        );
        let (node, client) = tokio::join!(node, client);
        client?;
        node
    }

    #[tokio::test]
    async fn real_quic_disconnected_service_has_only_typed_io_failure() -> Result<()> {
        let error = service_failure_fixture(ServiceFailureFixture::Disconnect).await?;
        assert!(is_service_transport_failure(&error), "{error:#}");
        assert!(error.is::<DiscoveryServiceTransportFailure>());
        assert!(!error.is::<DiscoveryAuthenticationTimeout>());
        let io = error
            .downcast::<DiscoveryServiceTransportFailure>()
            .unwrap()
            .0;
        assert!(io.is::<crate::iroh_transport::IrohStreamIoFailureV1>());
        let bound: anyhow::Error = BoundSessionError::Transport(io).into();
        assert!(is_service_transport_failure(&bound));
        // An observer returning a saved transport error is still a local
        // failure, not a current stream cancellation. Do not search through
        // arbitrary error causes past the BoundSessionError category.
        let observer: anyhow::Error = BoundSessionError::Local(bound).into();
        assert!(!is_service_transport_failure(&observer));
        Ok(())
    }

    #[tokio::test]
    async fn real_quic_bad_envelope_and_aead_are_not_service_transport_failures() -> Result<()> {
        for fixture in [
            ServiceFailureFixture::MalformedEnvelope,
            ServiceFailureFixture::TamperedCiphertext,
        ] {
            let error = service_failure_fixture(fixture).await?;
            assert!(!is_service_transport_failure(&error), "{error:#}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn real_quic_bad_rpc_and_trailing_data_are_not_service_transport_failures() -> Result<()>
    {
        for fixture in [
            ServiceFailureFixture::MalformedRpc,
            ServiceFailureFixture::TrailingAfterClose,
        ] {
            let error = service_failure_fixture(fixture).await?;
            assert!(!is_service_transport_failure(&error), "{error:#}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn service_classifier_rejects_untyped_transport_timer_auth_and_observer_errors() {
        let elapsed = tokio::time::timeout(Duration::ZERO, std::future::pending::<()>())
            .await
            .unwrap_err();
        for error in [
            anyhow::anyhow!("unknown carrier failure"),
            BoundSessionError::Transport(anyhow::anyhow!("unknown transport failure")).into(),
            BoundSessionError::Authentication(anyhow::anyhow!("pin differs")).into(),
            BoundSessionError::Local(ObserverRefused.into()).into(),
            DiscoveryAuthenticationTimeout(()).into(),
            elapsed.into(),
        ] {
            assert!(!is_service_transport_failure(&error), "{error:#}");
        }
    }

    #[tokio::test]
    async fn real_quic_discovery_wrappers_observe_auth_before_rpc() -> Result<()> {
        let _permit = SERIAL.acquire().await?;
        // Independent, explicit localhost fixture identities; never UCA or
        // production discovery credentials. No relay or DNS is contacted.
        let client_identity = SigningKey::from_bytes(&[71; 32]);
        let node_identity = SigningKey::from_bytes(&[72; 32]);
        let client_pin = client_identity.verifying_key().to_bytes();
        let node_pin = node_identity.verifying_key().to_bytes();
        let instance = [73; 32];
        let deadline = Instant::now() + Duration::from_secs(10);
        let store = Arc::new(Mutex::new(RendezvousStoreV1::new(instance, deadline)?));
        let node_store = store.clone();
        let (node_tx, node_rx) = oneshot::channel();
        let (client_tx, client_rx) = oneshot::channel();
        let node = with_iroh_scope_v1(
            local_config(),
            IrohControlV1::new(deadline, || Ok(()))?,
            move |scope| {
                Box::pin(async move {
                    node_tx.send(scope.address()?).unwrap();
                    let peer: IrohAddressV1 = client_rx.await?;
                    let mut stream: CarrierStreamV1<'_> =
                        (scope.accept(peer.endpoint_key).await?).into();
                    let mut stages = Vec::new();
                    let owner = serve_rendezvous_observed(
                        &mut stream,
                        &node_identity,
                        &[client_pin],
                        node_store,
                        &mut |stage, path| {
                            ensure!(matches!(path, "direct" | "relay" | "other" | "unknown"));
                            stages.push(stage);
                            Ok(())
                        },
                    )
                    .await?;
                    ensure!(owner == client_pin);
                    ensure!(
                        stages
                            == [
                                BoundSessionStage::OfferVerified,
                                BoundSessionStage::ResponseWritten,
                                BoundSessionStage::FinishedWritten,
                                BoundSessionStage::FinishedVerified,
                            ]
                    );
                    Ok(())
                })
            },
        );
        let client = with_iroh_scope_v1(
            local_config(),
            IrohControlV1::new(deadline, || Ok(()))?,
            move |scope| {
                Box::pin(async move {
                    client_tx.send(scope.address()?).unwrap();
                    let mut stream: CarrierStreamV1<'_> =
                        (scope.connect(&node_rx.await?).await?).into();
                    let mut stages = Vec::new();
                    let mut client = DiscoveryClient::connect_observed(
                        &mut stream,
                        &client_identity,
                        node_pin,
                        instance,
                        &mut |stage, path| {
                            ensure!(matches!(path, "direct" | "relay" | "other" | "unknown"));
                            stages.push(stage);
                            Ok(())
                        },
                    )
                    .await?;
                    ensure!(
                        stages
                            == [
                                BoundSessionStage::OfferWritten,
                                BoundSessionStage::ResponseVerified,
                                BoundSessionStage::FinishedWritten,
                                BoundSessionStage::FinishedVerified,
                            ]
                    );
                    ensure!(client.get([74; 32]).await?.is_none());
                    client.close().await
                })
            },
        );
        let (node, client) = tokio::join!(node, client);
        node?;
        client?;
        let stats = store.lock().unwrap().stats(Instant::now())?;
        assert_eq!(stats.live_records, 0);
        assert_eq!(stats.retained_slots, 0);
        Ok(())
    }

    #[test]
    fn codec_bounds_purpose_lengths_and_trailing_bytes() {
        let message = Message {
            request: [1; 16],
            instance: [2; 32],
            kind: PUT,
            slot: [3; 32],
            revision: 9,
            ttl_ms: 30_000,
            body: vec![4; 4096],
        };
        let wire = message.encode(REQUEST).unwrap();
        let decoded = Message::decode(&wire, REQUEST).unwrap();
        assert_eq!(decoded.body, message.body);
        assert_eq!(decoded.revision, 9);
        assert!(Message::decode(&wire, RESPONSE).is_err());
        for size in 0..HEADER {
            assert!(Message::decode(&wire[..size], REQUEST).is_err());
        }
        let mut trailing = wire.clone();
        trailing.push(0);
        assert!(Message::decode(&trailing, REQUEST).is_err());
        let mut bad = wire;
        bad[HEADER - 1] ^= 1;
        assert!(Message::decode(&bad, REQUEST).is_err());
    }
}
