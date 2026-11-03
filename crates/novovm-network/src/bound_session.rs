//! Authenticate a pinned NOVOVM identity on an already authorized reliable stream.
//!
//! The caller owns discovery, privacy policy, cancellation, network/identity
//! generations and the stream's bounded lifetime. This adapter never creates or
//! changes a contact pin, grants UCA authorization, retries authentication, or
//! interprets a transport connection as message delivery.

use crate::product_overlay::TransportIdentity;
use crate::{
    carrier_stream::CarrierStreamV1,
    duplex::{HandshakeReplayCacheV1, SecureNovoRudpEnvelopeV1},
    transport_binding::{
        BoundHandshakeInitiatorV1, BoundHandshakeResponderV1, BoundSecureChannelV1,
        BOUND_HANDSHAKE_MAX_TTL_MS_V1,
    },
};
use thiserror::Error;

pub mod peer_time;
pub mod session_clock;

/// A completed local handshake step. This contains no peer or session data.
/// `FinishedVerified` does not prove that the remote side has received our
/// Finished, nor that a business message has been delivered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundSessionStage {
    OfferWritten,
    ResponseVerified,
    OfferVerified,
    ResponseWritten,
    FinishedWritten,
    FinishedVerified,
}

impl BoundSessionStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OfferWritten => "offer_written",
            Self::ResponseVerified => "response_verified",
            Self::OfferVerified => "offer_verified",
            Self::ResponseWritten => "response_written",
            Self::FinishedWritten => "finished_written",
            Self::FinishedVerified => "finished_verified",
        }
    }
}

#[derive(Debug, Error)]
pub enum BoundSessionError {
    /// The enclosing executor must still distinguish cancellation/revocation
    /// from a retryable network failure. This variant alone does not grant retry.
    #[error("bound session transport failed: {0:#}")]
    Transport(#[source] anyhow::Error),
    /// Terminal for this attempt/round. Never switch identities, remove the pin,
    /// accept a legacy handshake, or turn this into a transport-quality sample.
    #[error("bound session authentication failed: {0:#}")]
    Authentication(#[source] anyhow::Error),
    #[error("bound session local operation failed: {0:#}")]
    Local(#[source] anyhow::Error),
}

fn authentication(error: impl Into<anyhow::Error>) -> BoundSessionError {
    BoundSessionError::Authentication(error.into())
}

fn local(error: impl Into<anyhow::Error>) -> BoundSessionError {
    BoundSessionError::Local(error.into())
}

fn now_ms() -> Result<u64, BoundSessionError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(local)?
        .as_millis()
        .try_into()
        .map_err(local)
}

fn observe<O>(
    stream: &CarrierStreamV1<'_>,
    observer: &mut O,
    stage: BoundSessionStage,
) -> Result<(), BoundSessionError>
where
    O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
{
    let path = stream
        .selected_path()
        .map_err(BoundSessionError::Transport)?;
    observer(stage, path).map_err(|error| local(error.context("bound session observer failed")))?;
    // An observer can revoke its local authority. Do not expose a channel or
    // begin the next frame after a callback invalidated the carrier.
    stream
        .selected_path()
        .map_err(BoundSessionError::Transport)?;
    Ok(())
}

use crate::transport_binding::structured as wire;

/// Native entry alias for the one current APFL bound handshake and Finished.
/// Default and explicitly named structured consumers use the same schemas;
/// there is no JSON decoding or algorithm fallback. This authenticates the
/// carrier stream, not a mailbox resource grant or a delivered chat message.
pub async fn authenticate_structured_bound_session_observed<O>(
    stream: &mut CarrierStreamV1<'_>,
    identity: &TransportIdentity,
    expected_peer: Vec<u8>,
    initiator: bool,
    replay: &mut HandshakeReplayCacheV1,
    observer: &mut O,
) -> Result<BoundSecureChannelV1, BoundSessionError>
where
    O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
{
    authenticate_bound_session_inner(stream, identity, expected_peer, initiator, replay, observer)
        .await
}

/// Complete both identity proofs and the encrypted, role-separated Finished
/// exchange before exposing an E2E channel. The local Finished is actually
/// written to the carrier before the remote Finished is accepted.
/// The current UTC contract uses APFL schemas 03 and binary Finished inside
/// the unchanged carrier frame cap; JSON is not a negotiated alternative.
///
/// `identity` must be the separately authorized network/device signing key, not
/// a spending key. `expected_peer` is the caller's independently verified pin.
/// The ephemeral-key diagnostic can use this same protocol but is not UCA or
/// production chat acceptance. On error, drop the stream/attempt; do not reuse
/// a partly completed handshake or its replay cache as a downgrade mechanism.
pub async fn authenticate_bound_session(
    stream: &mut CarrierStreamV1<'_>,
    identity: &TransportIdentity,
    expected_peer: Vec<u8>,
    initiator: bool,
    replay: &mut HandshakeReplayCacheV1,
) -> Result<BoundSecureChannelV1, BoundSessionError> {
    authenticate_bound_session_observed(
        stream,
        identity,
        expected_peer,
        initiator,
        replay,
        &mut |_, _| Ok(()),
    )
    .await
}

/// Observe completed steps without inspecting plaintext, pins, addresses,
/// digests or keys. The synchronous observer must be fast and nonblocking; use
/// a bounded try-send if forwarding events. Its error is terminal, including at
/// the last step: no channel is returned. Events occur between whole frames,
/// never by cancelling/restarting a pending read or write. The path string is
/// only the current carrier snapshot (including `tls-unclassified`),
/// not a promise about the entire session or a privacy classification.
pub async fn authenticate_bound_session_observed<O>(
    stream: &mut CarrierStreamV1<'_>,
    identity: &TransportIdentity,
    expected_peer: Vec<u8>,
    initiator: bool,
    replay: &mut HandshakeReplayCacheV1,
    observer: &mut O,
) -> Result<BoundSecureChannelV1, BoundSessionError>
where
    O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
{
    authenticate_bound_session_inner(stream, identity, expected_peer, initiator, replay, observer)
        .await
}

async fn authenticate_bound_session_inner<O>(
    stream: &mut CarrierStreamV1<'_>,
    identity: &TransportIdentity,
    expected_peer: Vec<u8>,
    initiator: bool,
    replay: &mut HandshakeReplayCacheV1,
    observer: &mut O,
) -> Result<BoundSecureChannelV1, BoundSessionError>
where
    O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
{
    // This evidence is derived from completed TLS by the trusted carrier. A
    // missing or mismatched binder is never a reason to try an unbound session.
    let binding = stream.binding().map_err(authentication)?;
    let pending = if initiator {
        let start = BoundHandshakeInitiatorV1::start(
            identity,
            expected_peer,
            binding,
            now_ms()?,
            BOUND_HANDSHAKE_MAX_TTL_MS_V1,
        )
        .map_err(authentication)?;
        let offer = wire::encode_offer(start.offer()).map_err(local)?;
        stream
            .write_frame(&offer)
            .await
            .map_err(BoundSessionError::Transport)?;
        observe(stream, observer, BoundSessionStage::OfferWritten)?;
        let response = stream
            .read_frame()
            .await
            .map_err(BoundSessionError::Transport)?;
        let response = wire::decode_response(&response).map_err(authentication)?;
        let pending = start
            .complete(&response, now_ms()?, replay)
            .map_err(authentication)?;
        observe(stream, observer, BoundSessionStage::ResponseVerified)?;
        pending
    } else {
        let offer = stream
            .read_frame()
            .await
            .map_err(BoundSessionError::Transport)?;
        let offer = wire::decode_offer(&offer).map_err(authentication)?;
        let response = BoundHandshakeResponderV1::respond(
            &offer,
            identity,
            expected_peer,
            binding,
            now_ms()?,
            BOUND_HANDSHAKE_MAX_TTL_MS_V1,
            replay,
        )
        .map_err(authentication)?;
        observe(stream, observer, BoundSessionStage::OfferVerified)?;
        let encoded = wire::encode_response(response.response()).map_err(local)?;
        stream
            .write_frame(&encoded)
            .await
            .map_err(BoundSessionError::Transport)?;
        observe(stream, observer, BoundSessionStage::ResponseWritten)?;
        response.into_pending()
    };

    finish_bound_session(stream, pending, observer).await
}

/// Service-side admission for a temporary transport endpoint. The offer's key
/// only selects an independently configured allowlist entry; no channel or
/// storage authority exists until its signature, TLS binding and both Finished
/// records have been verified. An empty or oversized allowlist fails closed.
pub async fn authenticate_allowed_service_client(
    stream: &mut CarrierStreamV1<'_>,
    identity: &TransportIdentity,
    allowed: &[Vec<u8>],
    replay: &mut HandshakeReplayCacheV1,
) -> Result<(BoundSecureChannelV1, Vec<u8>), BoundSessionError> {
    authenticate_allowed_service_client_observed(stream, identity, allowed, replay, &mut |_, _| {
        Ok(())
    })
    .await
}

/// Allowlist authentication with the same bounded, nonblocking observation
/// contract as [`authenticate_bound_session_observed`]. A claimed identity is
/// not observed or authorized before its actual signature and binding verify.
pub async fn authenticate_allowed_service_client_observed<O>(
    stream: &mut CarrierStreamV1<'_>,
    identity: &TransportIdentity,
    allowed: &[Vec<u8>],
    replay: &mut HandshakeReplayCacheV1,
    observer: &mut O,
) -> Result<(BoundSecureChannelV1, Vec<u8>), BoundSessionError>
where
    O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
{
    authenticate_service_client_observed(
        stream,
        identity,
        ServicePeerPolicyV1::Pinned(allowed),
        replay,
        observer,
    )
    .await
}

/// Local provider choice, not a network license. Open admission proves key
/// possession; it grants neither mailbox capabilities nor extra resources.
/// An empty Pinned policy remains invalid and never means open access.
#[derive(Clone, Copy)]
pub enum ServicePeerPolicyV1<'a> {
    Pinned(&'a [Vec<u8>]),
    AnyAuthenticated,
}

pub async fn authenticate_service_client_observed<O>(
    stream: &mut CarrierStreamV1<'_>,
    identity: &TransportIdentity,
    policy: ServicePeerPolicyV1<'_>,
    replay: &mut HandshakeReplayCacheV1,
    observer: &mut O,
) -> Result<(BoundSecureChannelV1, Vec<u8>), BoundSessionError>
where
    O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
{
    authenticate_service_client_inner(stream, identity, policy, replay, observer).await
}

/// Open or pinned provider policy on the explicit native structured entry.
/// Key possession is verified once for this stream; mailbox capabilities and
/// resource limits remain the provider's separate per-operation checks.
pub async fn authenticate_structured_service_client_observed<O>(
    stream: &mut CarrierStreamV1<'_>,
    identity: &TransportIdentity,
    policy: ServicePeerPolicyV1<'_>,
    replay: &mut HandshakeReplayCacheV1,
    observer: &mut O,
) -> Result<(BoundSecureChannelV1, Vec<u8>), BoundSessionError>
where
    O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
{
    authenticate_service_client_inner(stream, identity, policy, replay, observer).await
}

async fn authenticate_service_client_inner<O>(
    stream: &mut CarrierStreamV1<'_>,
    identity: &TransportIdentity,
    policy: ServicePeerPolicyV1<'_>,
    replay: &mut HandshakeReplayCacheV1,
    observer: &mut O,
) -> Result<(BoundSecureChannelV1, Vec<u8>), BoundSessionError>
where
    O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
{
    if let ServicePeerPolicyV1::Pinned(allowed) = policy {
        if allowed.is_empty() || allowed.len() > 16 {
            return Err(local(anyhow::anyhow!("invalid service allowlist size")));
        }
    }
    let binding = stream.binding().map_err(authentication)?;
    let received = stream
        .read_frame()
        .await
        .map_err(BoundSessionError::Transport)?;
    let offer = wire::decode_offer(&received).map_err(authentication)?;
    let claimed = offer.context.initiator_identity_public_key.clone();
    if claimed == identity.public_key().to_vec()
        || matches!(policy, ServicePeerPolicyV1::Pinned(allowed) if !allowed.contains(&claimed))
    {
        return Err(authentication(anyhow::anyhow!(
            "service client not permitted"
        )));
    }
    let response = BoundHandshakeResponderV1::respond(
        &offer,
        identity,
        claimed.clone(),
        binding,
        now_ms()?,
        BOUND_HANDSHAKE_MAX_TTL_MS_V1,
        replay,
    )
    .map_err(authentication)?;
    observe(stream, observer, BoundSessionStage::OfferVerified)?;
    stream
        .write_frame(&wire::encode_response(response.response()).map_err(local)?)
        .await
        .map_err(BoundSessionError::Transport)?;
    observe(stream, observer, BoundSessionStage::ResponseWritten)?;
    let channel = finish_bound_session(stream, response.into_pending(), observer).await?;
    Ok((channel, claimed))
}

async fn finish_bound_session<O>(
    stream: &mut CarrierStreamV1<'_>,
    mut pending: crate::transport_binding::PendingBoundChannelV1,
    observer: &mut O,
) -> Result<BoundSecureChannelV1, BoundSessionError>
where
    O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
{
    let finished = pending.make_finished(now_ms()?).map_err(local)?;
    let encoded = wire::encode_finished(&finished).map_err(local)?;
    stream
        .write_frame(&encoded)
        .await
        .map_err(BoundSessionError::Transport)?;
    observe(stream, observer, BoundSessionStage::FinishedWritten)?;
    let remote_finished = stream
        .read_frame()
        .await
        .map_err(BoundSessionError::Transport)?;
    let remote_finished: SecureNovoRudpEnvelopeV1 =
        wire::decode_finished(&remote_finished).map_err(authentication)?;
    pending
        .verify_finished(&remote_finished, now_ms()?)
        .map_err(authentication)?;
    let channel = pending.into_channel(now_ms()?).map_err(authentication)?;
    observe(stream, observer, BoundSessionStage::FinishedVerified)?;
    Ok(channel)
}

#[cfg(all(test, feature = "iroh-transport"))]
mod tests {
    mod native_carrier;
    mod session_clock;
    use super::*;
    #[derive(Clone, Copy)]
    enum Entry {
        Default,
        StructuredApfl,
    }

    use crate::{
        duplex::{NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0},
        iroh_transport::{with_iroh_scope_v1, IrohAddressV1, IrohConfigV1, IrohControlV1},
    };
    use anyhow::{bail, ensure, Result};
    use std::time::{Duration, Instant};
    use tokio::sync::oneshot;

    use crate::SERVICE_TEST_SERIAL as SERIAL;

    // Keys and addressing below exist only in localhost tests. No DNS lookup,
    // public discovery, public relay, handset or user data is involved.
    fn key(seed: u8) -> TransportIdentity {
        TransportIdentity::from_bytes(&[seed; 32])
    }

    fn public(seed: u8) -> Vec<u8> {
        key(seed).public_key().to_vec()
    }

    fn config() -> IrohConfigV1 {
        IrohConfigV1::new(&[], "127.0.0.1".parse().unwrap())
            .unwrap()
            .with_bind_address("127.0.0.1:0".parse().unwrap())
            .unwrap()
    }

    fn control() -> IrohControlV1 {
        IrohControlV1::new(Instant::now() + Duration::from_secs(10), || Ok(())).unwrap()
    }

    fn record_stage(
        stages: &mut Vec<BoundSessionStage>,
        stage: BoundSessionStage,
        path: &'static str,
    ) -> Result<()> {
        ensure!(
            matches!(path, "direct" | "relay" | "other" | "unknown"),
            "observer received data beyond a path class"
        );
        stages.push(stage);
        Ok(())
    }

    async fn test_bound_entry<O>(
        stream: &mut CarrierStreamV1<'_>,
        identity: &TransportIdentity,
        peer: Vec<u8>,
        initiator: bool,
        replay: &mut HandshakeReplayCacheV1,
        observer: &mut O,
        wire: Entry,
    ) -> Result<BoundSecureChannelV1, BoundSessionError>
    where
        O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
    {
        match wire {
            Entry::Default => {
                authenticate_bound_session_observed(
                    stream, identity, peer, initiator, replay, observer,
                )
                .await
            }
            Entry::StructuredApfl => {
                authenticate_structured_bound_session_observed(
                    stream, identity, peer, initiator, replay, observer,
                )
                .await
            }
        }
    }
    async fn test_service_entry<O>(
        stream: &mut CarrierStreamV1<'_>,
        identity: &TransportIdentity,
        policy: ServicePeerPolicyV1<'_>,
        replay: &mut HandshakeReplayCacheV1,
        observer: &mut O,
        wire: Entry,
    ) -> Result<(BoundSecureChannelV1, Vec<u8>), BoundSessionError>
    where
        O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
    {
        match wire {
            Entry::Default => {
                authenticate_service_client_observed(stream, identity, policy, replay, observer)
                    .await
            }
            Entry::StructuredApfl => {
                authenticate_structured_service_client_observed(
                    stream, identity, policy, replay, observer,
                )
                .await
            }
        }
    }

    async fn handshake_business_roundtrip(wire: Entry, open: bool) {
        let _permit = SERIAL.acquire().await.unwrap();
        let (server_tx, server_rx) = oneshot::channel();
        let (client_tx, client_rx) = oneshot::channel();
        let server = with_iroh_scope_v1(config(), control(), move |scope| {
            Box::pin(async move {
                server_tx.send(scope.address()?).unwrap();
                let peer: IrohAddressV1 = client_rx.await?;
                let mut stream: CarrierStreamV1<'_> =
                    (scope.accept(peer.endpoint_key).await?).into();
                let mut replay = HandshakeReplayCacheV1::new(8);
                let mut stages = Vec::new();
                let mut channel = if open {
                    test_service_entry(
                        &mut stream,
                        &key(2),
                        ServicePeerPolicyV1::AnyAuthenticated,
                        &mut replay,
                        &mut |stage, path| record_stage(&mut stages, stage, path),
                        wire,
                    )
                    .await?
                    .0
                } else {
                    test_bound_entry(
                        &mut stream,
                        &key(2),
                        public(1),
                        false,
                        &mut replay,
                        &mut |stage, path| record_stage(&mut stages, stage, path),
                        wire,
                    )
                    .await?
                };
                ensure!(
                    stages
                        == [
                            BoundSessionStage::OfferVerified,
                            BoundSessionStage::ResponseWritten,
                            BoundSessionStage::FinishedWritten,
                            BoundSessionStage::FinishedVerified,
                        ],
                    "responder observation order differs"
                );
                let encrypted: SecureNovoRudpEnvelopeV1 =
                    crate::duplex::decode_iroh_envelope_v1(&stream.read_frame().await?)?;
                ensure!(
                    encrypted.sequence == 1,
                    "first business frame must follow Finished"
                );
                let received = channel.open_novorudp_frame(&encrypted)?;
                ensure!(
                    received.payload == b"localhost bound session",
                    "unexpected payload"
                );
                ensure!(
                    channel.open_novorudp_frame(&encrypted).is_err(),
                    "replay accepted"
                );
                let echo = channel.seal_novorudp_frame(&received)?;
                stream
                    .write_frame(&crate::duplex::encode_iroh_envelope_v1(&echo)?)
                    .await?;
                stream.finish().await?;
                Ok(channel.session_id())
            })
        });
        let client = with_iroh_scope_v1(config(), control(), move |scope| {
            Box::pin(async move {
                client_tx.send(scope.address()?).unwrap();
                let mut stream: CarrierStreamV1<'_> =
                    (scope.connect(&server_rx.await?).await?).into();
                let mut replay = HandshakeReplayCacheV1::new(8);
                let mut stages = Vec::new();
                let mut channel = test_bound_entry(
                    &mut stream,
                    &key(1),
                    public(2),
                    true,
                    &mut replay,
                    &mut |stage, path| record_stage(&mut stages, stage, path),
                    wire,
                )
                .await?;
                ensure!(
                    stages
                        == [
                            BoundSessionStage::OfferWritten,
                            BoundSessionStage::ResponseVerified,
                            BoundSessionStage::FinishedWritten,
                            BoundSessionStage::FinishedVerified,
                        ],
                    "initiator observation order differs"
                );
                let frame = NovoRudpTransportFrameV0::new(
                    NovoRudpTransportFrameKindV0::Data,
                    channel.session_id(),
                    1,
                    1,
                    0,
                    0,
                    b"localhost bound session".to_vec(),
                );
                let envelope = channel.seal_novorudp_frame(&frame)?;
                ensure!(
                    envelope.sequence == 1,
                    "first business frame must follow Finished"
                );
                stream
                    .write_frame(&crate::duplex::encode_iroh_envelope_v1(&envelope)?)
                    .await?;
                let response: SecureNovoRudpEnvelopeV1 =
                    crate::duplex::decode_iroh_envelope_v1(&stream.read_frame().await?)?;
                ensure!(
                    channel.open_novorudp_frame(&response)? == frame,
                    "encrypted reply differs"
                );
                stream.finish().await?;
                Ok(channel.session_id())
            })
        });
        let (server, client) = tokio::join!(server, client);
        assert_eq!(server.unwrap(), client.unwrap());
    }

    #[tokio::test]
    async fn real_quic_finished_authenticates_both_directions_before_business() {
        handshake_business_roundtrip(Entry::Default, false).await;
    }

    #[tokio::test]
    async fn structured_quic_finished_authenticates_both_directions_before_business() {
        handshake_business_roundtrip(Entry::StructuredApfl, false).await;
    }

    #[tokio::test]
    async fn structured_quic_open_service_authenticates_then_exchanges_encrypted_data() {
        handshake_business_roundtrip(Entry::StructuredApfl, true).await;
    }

    async fn wrong_identity_pin(wire: Entry) {
        let _permit = SERIAL.acquire().await.unwrap();
        let (server_tx, server_rx) = oneshot::channel();
        let (client_tx, client_rx) = oneshot::channel();
        let server = with_iroh_scope_v1(config(), control(), move |scope| {
            Box::pin(async move {
                server_tx.send(scope.address()?).unwrap();
                let peer: IrohAddressV1 = client_rx.await?;
                let mut stream: CarrierStreamV1<'_> =
                    (scope.accept(peer.endpoint_key).await?).into();
                let mut stages = Vec::new();
                let result = test_bound_entry(
                    &mut stream,
                    &key(2),
                    public(3),
                    false,
                    &mut HandshakeReplayCacheV1::new(8),
                    &mut |stage, path| record_stage(&mut stages, stage, path),
                    wire,
                )
                .await;
                ensure!(
                    matches!(result, Err(BoundSessionError::Authentication(_))),
                    "wrong pin was not a terminal authentication failure"
                );
                ensure!(
                    stages.is_empty(),
                    "unverified offer emitted a verified stage"
                );
                drop(stream);
                Ok(())
            })
        });
        let client = with_iroh_scope_v1(config(), control(), move |scope| {
            Box::pin(async move {
                client_tx.send(scope.address()?).unwrap();
                let mut stream: CarrierStreamV1<'_> =
                    (scope.connect(&server_rx.await?).await?).into();
                ensure!(
                    test_bound_entry(
                        &mut stream,
                        &key(1),
                        public(2),
                        true,
                        &mut HandshakeReplayCacheV1::new(8),
                        &mut |_, _| Ok(()),
                        wire
                    )
                    .await
                    .is_err(),
                    "unverified counterpart received a channel"
                );
                Ok(())
            })
        });
        let (server, client) = tokio::join!(server, client);
        server.unwrap();
        client.unwrap();
    }

    #[tokio::test]
    async fn real_quic_wrong_identity_pin_is_authentication_failure() {
        wrong_identity_pin(Entry::Default).await;
    }

    #[tokio::test]
    async fn structured_quic_wrong_identity_pin_is_authentication_failure() {
        wrong_identity_pin(Entry::StructuredApfl).await;
    }

    #[derive(Clone, Copy)]
    enum InterruptedPeer {
        Close,
        InvalidHandshake,
        InvalidFinished,
    }

    async fn interrupted_peer(mode: InterruptedPeer, wire: Entry) -> Result<BoundSessionError> {
        let (server_tx, server_rx) = oneshot::channel();
        let (client_tx, client_rx) = oneshot::channel();
        let server = with_iroh_scope_v1(config(), control(), move |scope| {
            Box::pin(async move {
                server_tx.send(scope.address()?).unwrap();
                let peer: IrohAddressV1 = client_rx.await?;
                let mut stream: CarrierStreamV1<'_> =
                    (scope.accept(peer.endpoint_key).await?).into();
                // Wait for the actual offer so failure occurs during the bound
                // session, rather than racing initial QUIC stream establishment.
                let offer = stream.read_frame().await?;
                let offer = wire::decode_offer(&offer)?;
                if matches!(mode, InterruptedPeer::InvalidHandshake) {
                    stream.write_frame(b"not a bound handshake").await?;
                    // Keep the connection alive until the client has read and
                    // rejected the frame; a write alone does not prove delivery.
                    let _ = stream.read_frame().await;
                } else if matches!(mode, InterruptedPeer::InvalidFinished) {
                    let response = BoundHandshakeResponderV1::respond(
                        &offer,
                        &key(2),
                        public(1),
                        stream.binding()?,
                        now_ms()?,
                        BOUND_HANDSHAKE_MAX_TTL_MS_V1,
                        &mut HandshakeReplayCacheV1::new(8),
                    )?;
                    stream
                        .write_frame(&wire::encode_response(response.response())?)
                        .await?;
                    let mut pending = response.into_pending();
                    let mut finished = pending.make_finished(now_ms()?)?;
                    let remote: SecureNovoRudpEnvelopeV1 =
                        wire::decode_finished(&stream.read_frame().await?)?;
                    pending.verify_finished(&remote, now_ms()?)?;
                    ensure!(!finished.ciphertext.is_empty());
                    finished.ciphertext[0] ^= 1;
                    stream
                        .write_frame(&wire::encode_finished(&finished)?)
                        .await?;
                    let _ = stream.read_frame().await;
                }
                drop(stream);
                Ok(())
            })
        });
        let client = with_iroh_scope_v1(config(), control(), move |scope| {
            Box::pin(async move {
                client_tx.send(scope.address()?).unwrap();
                let mut stream: CarrierStreamV1<'_> =
                    (scope.connect(&server_rx.await?).await?).into();
                let mut stages = Vec::new();
                let result = test_bound_entry(
                    &mut stream,
                    &key(1),
                    public(2),
                    true,
                    &mut HandshakeReplayCacheV1::new(8),
                    &mut |stage, path| record_stage(&mut stages, stage, path),
                    wire,
                )
                .await;
                let Err(error) = result else {
                    bail!("interrupted peer produced an authenticated channel");
                };
                let expected = if matches!(mode, InterruptedPeer::InvalidFinished) {
                    vec![
                        BoundSessionStage::OfferWritten,
                        BoundSessionStage::ResponseVerified,
                        BoundSessionStage::FinishedWritten,
                    ]
                } else {
                    vec![BoundSessionStage::OfferWritten]
                };
                ensure!(
                    stages == expected,
                    "interrupted peer emitted an unverified stage"
                );
                drop(stream);
                Ok(error)
            })
        });
        let (server, client) = tokio::join!(server, client);
        server?;
        client
    }

    #[tokio::test]
    async fn real_quic_peer_closing_before_response_is_transport_failure() {
        let _permit = SERIAL.acquire().await.unwrap();
        assert!(matches!(
            interrupted_peer(InterruptedPeer::Close, Entry::Default)
                .await
                .unwrap(),
            BoundSessionError::Transport(_)
        ));
    }

    #[tokio::test]
    async fn real_quic_unknown_handshake_format_is_authentication_failure() {
        let _permit = SERIAL.acquire().await.unwrap();
        assert!(matches!(
            interrupted_peer(InterruptedPeer::InvalidHandshake, Entry::Default)
                .await
                .unwrap(),
            BoundSessionError::Authentication(_)
        ));
    }

    #[tokio::test]
    async fn real_quic_tampered_finished_never_emits_finished_verified() {
        let _permit = SERIAL.acquire().await.unwrap();
        assert!(matches!(
            interrupted_peer(InterruptedPeer::InvalidFinished, Entry::Default)
                .await
                .unwrap(),
            BoundSessionError::Authentication(_)
        ));
    }

    #[tokio::test]
    async fn structured_quic_interruption_and_tampered_finished_never_expose_channel() {
        let _permit = SERIAL.acquire().await.unwrap();
        for mode in [
            InterruptedPeer::Close,
            InterruptedPeer::InvalidHandshake,
            InterruptedPeer::InvalidFinished,
        ] {
            let error = interrupted_peer(mode, Entry::StructuredApfl).await.unwrap();
            if matches!(mode, InterruptedPeer::Close) {
                assert!(matches!(error, BoundSessionError::Transport(_)));
            } else {
                assert!(matches!(error, BoundSessionError::Authentication(_)));
            }
        }
    }

    // Real QUIC admission matrix: no fixed client list in open mode; the
    // original signature/TLS binding/Finished checks still precede any channel.
    #[tokio::test]
    async fn open_service_authenticates_unknown_keys_without_relaxing_crypto() -> Result<()> {
        let _permit = SERIAL.acquire().await.unwrap();
        for case in 0..7u8 {
            let (server_tx, server_rx) = oneshot::channel();
            let (client_tx, client_rx) = oneshot::channel();
            let server = with_iroh_scope_v1(config(), control(), move |scope| {
                Box::pin(async move {
                    server_tx.send(scope.address()?).unwrap();
                    let peer: IrohAddressV1 = client_rx.await?;
                    let mut stream: CarrierStreamV1<'_> =
                        scope.accept(peer.endpoint_key).await?.into();
                    let pins = [public(9)];
                    let policy = match case {
                        1 => ServicePeerPolicyV1::Pinned(&pins),
                        2 => ServicePeerPolicyV1::Pinned(&[]),
                        _ => ServicePeerPolicyV1::AnyAuthenticated,
                    };
                    let mut stages = Vec::new();
                    let result = authenticate_service_client_observed(
                        &mut stream,
                        &key(2),
                        policy,
                        &mut HandshakeReplayCacheV1::new(8),
                        &mut |stage, path| record_stage(&mut stages, stage, path),
                    )
                    .await;
                    if case == 0 {
                        ensure!(result?.1 == public(1));
                        ensure!(stages.last() == Some(&BoundSessionStage::FinishedVerified));
                        stream.finish().await?;
                    } else {
                        ensure!(result.is_err(), "invalid admission case {case} accepted");
                        ensure!(!stages.contains(&BoundSessionStage::FinishedVerified));
                        if case == 2 {
                            ensure!(matches!(result, Err(BoundSessionError::Local(_))));
                        } else {
                            ensure!(matches!(result, Err(BoundSessionError::Authentication(_))));
                        }
                    }
                    Ok(())
                })
            });
            let client = with_iroh_scope_v1(config(), control(), move |scope| {
                Box::pin(async move {
                    client_tx.send(scope.address()?).unwrap();
                    let mut stream: CarrierStreamV1<'_> =
                        scope.connect(&server_rx.await?).await?.into();
                    if case <= 2 {
                        let result = authenticate_bound_session(
                            &mut stream,
                            &key(1),
                            public(2),
                            true,
                            &mut HandshakeReplayCacheV1::new(8),
                        )
                        .await;
                        if case == 0 {
                            result?;
                            stream.finish().await?;
                        } else {
                            ensure!(result.is_err());
                        }
                    } else {
                        let initiator = BoundHandshakeInitiatorV1::start(
                            &key(1),
                            public(2),
                            stream.binding()?,
                            now_ms()?,
                            BOUND_HANDSHAKE_MAX_TTL_MS_V1,
                        )?;
                        let mut offer = initiator.offer().clone();
                        match case {
                            3 => offer.signature[0] ^= 1,
                            5 => offer.context.initiator_identity_public_key = vec![0; 32],
                            6 => offer.context.initiator_identity_public_key = public(2),
                            _ => {}
                        }
                        // Encode untrusted field mutations without the typed
                        // encoder's local shape checks; the real receiver must
                        // reject them before granting a channel.
                        use novovm_protocol::{apfl_batch, apfl_column::Column};
                        let limits = apfl_batch::Limits {
                            rows: 1,
                            columns: 18,
                            field_bytes: crate::product_overlay::MLDSA65_SIGNATURE_BYTES,
                            wire_bytes: crate::transport_binding::BOUND_HANDSHAKE_MAX_WIRE_BYTES_V1,
                            expanded_bytes: 16 * 1024,
                        };
                        let canonical = wire::encode_offer(initiator.offer())?;
                        let batch = apfl_batch::decode(&canonical, wire::OFFER_SCHEMA, limits)
                            .map_err(anyhow::Error::msg)?;
                        let mut fields = (0..batch.columns())
                            .map(|i| batch.field(0, i).map_err(anyhow::Error::msg))
                            .collect::<Result<Vec<_>>>()?;
                        fields[2] = &offer.context.initiator_identity_public_key;
                        fields[13] = &offer.signature;
                        let columns = fields
                            .into_iter()
                            .map(|field| Column::collect(std::iter::once(field)))
                            .collect::<Vec<_>>();
                        let untrusted = apfl_batch::encode(
                            wire::OFFER_SCHEMA,
                            batch.context,
                            1,
                            &columns,
                            limits,
                        )
                        .map_err(anyhow::Error::msg)?;
                        stream.write_frame(&untrusted).await?;
                        if case == 4 {
                            let response = wire::decode_response(&stream.read_frame().await?)?;
                            let mut pending = initiator.complete(
                                &response,
                                now_ms()?,
                                &mut HandshakeReplayCacheV1::new(8),
                            )?;
                            let mut finished = pending.make_finished(now_ms()?)?;
                            finished.ciphertext[0] ^= 1;
                            stream
                                .write_frame(&wire::encode_finished(&finished)?)
                                .await?;
                            // Consume the server's Finished, then observe refusal.
                            let _ = stream.read_frame().await;
                        }
                        ensure!(stream.read_frame().await.is_err());
                    }
                    Ok(())
                })
            });
            let (server, client) = tokio::join!(server, client);
            client.map_err(|e| e.context(format!("client admission case {case}")))?;
            server.map_err(|e| e.context(format!("server admission case {case}")))?;
        }
        Ok(())
    }

    async fn observer_rejection(
        reject_on_server: bool,
        rejected_stage: BoundSessionStage,
        wire: Entry,
    ) -> Result<()> {
        let (server_tx, server_rx) = oneshot::channel();
        let (client_tx, client_rx) = oneshot::channel();
        let (server_done_tx, server_done_rx) = oneshot::channel();
        let (client_done_tx, client_done_rx) = oneshot::channel();
        let server = with_iroh_scope_v1(config(), control(), move |scope| {
            Box::pin(async move {
                server_tx.send(scope.address()?).unwrap();
                let peer: IrohAddressV1 = client_rx.await?;
                let mut stream: CarrierStreamV1<'_> =
                    (scope.accept(peer.endpoint_key).await?).into();
                let mut stages = Vec::new();
                let result = test_service_entry(
                    &mut stream,
                    &key(2),
                    ServicePeerPolicyV1::Pinned(&[public(1)]),
                    &mut HandshakeReplayCacheV1::new(8),
                    &mut |stage, path| {
                        record_stage(&mut stages, stage, path)?;
                        ensure!(
                            !(reject_on_server && stage == rejected_stage),
                            "test observer refusal"
                        );
                        Ok(())
                    },
                    wire,
                )
                .await;
                if reject_on_server {
                    ensure!(
                        matches!(result, Err(BoundSessionError::Local(ref error))
                    if error.to_string() == "bound session observer failed"),
                        "observer failure exposed a channel or changed error class"
                    );
                    ensure!(
                        stages.last() == Some(&rejected_stage),
                        "wrong observer rejection stage"
                    );
                    drop(stream);
                    let _ = server_done_tx.send(());
                } else {
                    // The other side can reject its final observation after this
                    // side has verified Finished. Keep the connection alive until
                    // that local decision is observed, rather than racing a close.
                    client_done_rx.await?;
                    drop(stream);
                }
                Ok(())
            })
        });
        let client = with_iroh_scope_v1(config(), control(), move |scope| {
            Box::pin(async move {
                client_tx.send(scope.address()?).unwrap();
                let mut stream: CarrierStreamV1<'_> =
                    (scope.connect(&server_rx.await?).await?).into();
                let mut stages = Vec::new();
                let result = test_bound_entry(
                    &mut stream,
                    &key(1),
                    public(2),
                    true,
                    &mut HandshakeReplayCacheV1::new(8),
                    &mut |stage, path| {
                        record_stage(&mut stages, stage, path)?;
                        ensure!(
                            reject_on_server || stage != rejected_stage,
                            "test observer refusal"
                        );
                        Ok(())
                    },
                    wire,
                )
                .await;
                if !reject_on_server {
                    ensure!(
                        matches!(result, Err(BoundSessionError::Local(ref error))
                    if error.to_string() == "bound session observer failed"),
                        "observer failure exposed a channel or changed error class"
                    );
                    ensure!(
                        stages.last() == Some(&rejected_stage),
                        "wrong observer rejection stage"
                    );
                    drop(stream);
                    let _ = client_done_tx.send(());
                } else {
                    server_done_rx.await?;
                    drop(stream);
                }
                Ok(())
            })
        });
        let (server, client) = tokio::join!(server, client);
        server?;
        client
    }

    #[tokio::test]
    async fn real_quic_observer_failure_never_exposes_channel_even_after_finished() {
        let _permit = SERIAL.acquire().await.unwrap();
        for (server, stage) in [
            (false, BoundSessionStage::ResponseVerified),
            (false, BoundSessionStage::FinishedVerified),
            (true, BoundSessionStage::OfferVerified),
            (true, BoundSessionStage::FinishedVerified),
        ] {
            observer_rejection(server, stage, Entry::Default)
                .await
                .unwrap();
        }
    }
    #[tokio::test]
    async fn structured_quic_observer_refusal_never_exposes_channel() {
        let _permit = SERIAL.acquire().await.unwrap();
        for server in [false, true] {
            for stage in [
                BoundSessionStage::FinishedWritten,
                BoundSessionStage::FinishedVerified,
            ] {
                observer_rejection(server, stage, Entry::StructuredApfl)
                    .await
                    .unwrap();
            }
        }
    }

    #[tokio::test]
    async fn structured_quic_rejects_legacy_offer_without_fallback() {
        let _permit = SERIAL.acquire().await.unwrap();
        let (server_tx, server_rx) = oneshot::channel();
        let (client_tx, client_rx) = oneshot::channel();
        let server = with_iroh_scope_v1(config(), control(), move |scope| {
            Box::pin(async move {
                server_tx.send(scope.address()?).unwrap();
                let peer: IrohAddressV1 = client_rx.await?;
                let mut stream: CarrierStreamV1<'_> = scope.accept(peer.endpoint_key).await?.into();
                let mut stages = Vec::new();
                let result = authenticate_structured_service_client_observed(
                    &mut stream,
                    &key(2),
                    ServicePeerPolicyV1::AnyAuthenticated,
                    &mut HandshakeReplayCacheV1::new(8),
                    &mut |stage, path| record_stage(&mut stages, stage, path),
                )
                .await;
                ensure!(matches!(result, Err(BoundSessionError::Authentication(_))));
                ensure!(stages.is_empty());
                Ok(())
            })
        });
        let client = with_iroh_scope_v1(config(), control(), move |scope| {
            Box::pin(async move {
                client_tx.send(scope.address()?).unwrap();
                let mut stream: CarrierStreamV1<'_> =
                    scope.connect(&server_rx.await?).await?.into();
                let start = BoundHandshakeInitiatorV1::start(
                    &key(1),
                    public(2),
                    stream.binding()?,
                    now_ms()?,
                    BOUND_HANDSHAKE_MAX_TTL_MS_V1,
                )?;
                // The default API is APFL too; send the superseded JSON
                // representation explicitly rather than calling that API.
                let mut legacy = start.offer().clone();
                legacy.context.version = 1;
                legacy.context.initiator_identity_public_key.truncate(32);
                legacy.context.responder_identity_public_key.truncate(32);
                legacy.e2e.version = 1;
                legacy.e2e.initiator_identity_public_key.truncate(32);
                legacy.e2e.signature.truncate(64);
                legacy.signature.truncate(64);
                let mut json = serde_json::to_value(legacy)?;
                json["context"]
                    .as_object_mut()
                    .unwrap()
                    .remove("signature_suite");
                let bytes = serde_json::to_vec(&json)?;
                ensure!(bytes.len() <= crate::iroh_transport::IROH_MAX_FRAME_V1);
                stream.write_frame(&bytes).await?;
                ensure!(stream.read_frame().await.is_err());
                Ok(())
            })
        });
        let (server, client) = tokio::join!(server, client);
        server.unwrap();
        client.unwrap();
    }
}
