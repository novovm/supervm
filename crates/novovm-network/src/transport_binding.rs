//! Bind the existing NOVOVM E2E handshake to an authenticated reliable carrier.
//!
//! The carrier supplies the actual TLS exporter and authenticated endpoint IDs;
//! peer supplied fields never create a `VerifiedTransportBinding`. This proves
//! possession of independent transport and NOVOVM identity keys, not UCA grants,
//! wallet authorization, anonymity, application delivery, or post-quantum safety.
//! Network authorization, cancellation and connection lifetime remain the
//! carrier owner's responsibility, including before it opens any connection.

use crate::duplex::{
    peer_id_from_ed25519_public_key_v1, E2eSecureChannelV1, HandshakeReplayCacheV1,
    NodeHandshakeInitiatorV1, NodeHandshakeOfferV1, NodeHandshakeResponderV1,
    NodeHandshakeResponseV1, NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0,
    ProductOverlayErrorV1, SecureNovoRudpEnvelopeV1, PRODUCT_OVERLAY_CLOCK_SKEW_MS_V1,
    PRODUCT_OVERLAY_PROTOCOL_VERSION_V1,
};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::Zeroize;

pub mod session_clock;
pub mod structured;

pub const BOUND_TRANSPORT_ALPN_V1: &[u8] = b"novovm/bound-carrier/1";
pub const BOUND_TRANSPORT_EXPORTER_LABEL_V1: &[u8] = b"EXPORTER-NOVOVM-bound-carrier-v1";
/// Legacy carriers use this exact context. Explicit NVMBOX05 Iroh streams append
/// a versioned domain and their actual QUIC stream ID on both ends before export.
pub const BOUND_TRANSPORT_EXPORTER_CONTEXT_V1: &[u8] = BOUND_TRANSPORT_ALPN_V1;
pub const BOUND_HANDSHAKE_VERSION_V1: u16 = 1;
pub const BOUND_HANDSHAKE_MAX_WIRE_BYTES_V1: usize = 8 * 1024;
pub const BOUND_HANDSHAKE_MAX_TTL_MS_V1: u64 = 30_000;
pub const BOUND_CHANNEL_MAX_PAYLOAD_BYTES_V1: usize = 64 * 1024;
const PEER_ID_BYTES: usize = b"novovm-ed25519:".len() + 32 * 2;
const FINISHED_STREAM: u64 = u64::MAX;
const PURPOSE: &[u8] = b"novovm.network.bound-reliable-carrier.v1";
const SESSION_CLOCK_PURPOSE_V2: &[u8] = b"novovm.network.bound-reliable-carrier.session-clock.v2";
const FINISHED_DOMAIN: &[u8] = b"novovm.bound-carrier.finished.v1";

/// Selected by an explicit local entry, never inferred as a fallback from a
/// peer's version. Both signatures and Finished commit to version and purpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoundClockMode {
    UtcV1,
    SessionV2,
}

impl BoundClockMode {
    fn version(self) -> u16 {
        match self {
            Self::UtcV1 => 1,
            Self::SessionV2 => 2,
        }
    }
    fn purpose(self) -> &'static [u8] {
        match self {
            Self::UtcV1 => PURPOSE,
            Self::SessionV2 => SESSION_CLOCK_PURPOSE_V2,
        }
    }
    fn from_validated_version(version: u16) -> Result<Self> {
        match version {
            1 => Ok(Self::UtcV1),
            2 => Ok(Self::SessionV2),
            _ => Err(TransportBindingErrorV1::Invalid("bound channel mode")),
        }
    }
    fn check_times(self, issued: u64, expires: u64) -> Result<()> {
        if self == Self::SessionV2 && (issued != 0 || expires != BOUND_HANDSHAKE_MAX_TTL_MS_V1) {
            return Err(TransportBindingErrorV1::Invalid("session clock fields"));
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum TransportBindingErrorV1 {
    #[error("invalid bound transport handshake: {0}")]
    Invalid(&'static str),
    #[error("bound transport handshake has expired")]
    Expired,
    /// Local liveness budget, distinct from an expired UTC credential or a
    /// failed signature. Never infer identity rejection from this condition.
    #[error("bound session monotonic handshake deadline elapsed")]
    HandshakeDeadline,
    #[error(
        "bound transport context does not match the authenticated carrier and pinned identities"
    )]
    BindingMismatch,
    #[error("bound transport signature is invalid")]
    InvalidSignature,
    #[error("bound transport handshake exceeds its resource limit")]
    ResourceLimit,
    #[error("bound transport Finished exchange is incomplete or invalid")]
    Finished,
    #[error("bound transport JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("NOVOVM E2E handshake failed: {0}")]
    Overlay(#[from] ProductOverlayErrorV1),
}

type Result<T> = std::result::Result<T, TransportBindingErrorV1>;

/// Evidence from the local carrier implementation. Intentionally not serde,
/// publicly constructible, or interchangeable with the wire commitment below.
/// The constructor's trust boundary is the crate's authenticated carrier code.
pub struct VerifiedTransportBinding {
    initiator_endpoint: [u8; 32],
    responder_endpoint: [u8; 32],
    connection_binding: [u8; 32],
    local_is_initiator: bool,
}

impl VerifiedTransportBinding {
    /// Local evidence for the native carrier source key. This is not a node
    /// identity or a signed native handshake binding; the carrier owner must
    /// still bind the expected node and its actual authenticated session.
    /// Never serialize this material or accept a remote supplied replacement.
    pub(crate) fn native_route_material(&self) -> ([u8; 32], [u8; 32]) {
        let remote = if self.local_is_initiator {
            self.responder_endpoint
        } else {
            self.initiator_endpoint
        };
        (remote, self.connection_binding)
    }

    /// Call only after the carrier's TLS handshake completes; no 0-RTT or
    /// unauthenticated endpoint hints. `exporter` must come from that connection
    /// using the exported label/context constants. Never use a peer's binder.
    pub(crate) fn from_authenticated_transport(
        local_endpoint: [u8; 32],
        remote_endpoint: [u8; 32],
        mut exporter: [u8; 32],
        alpn: &[u8],
        initiator: bool,
    ) -> Result<Self> {
        let result = (|| {
            if alpn != BOUND_TRANSPORT_ALPN_V1 || exporter == [0; 32] {
                return Err(TransportBindingErrorV1::BindingMismatch);
            }
            check_public_key(&local_endpoint)?;
            check_public_key(&remote_endpoint)?;
            if local_endpoint == remote_endpoint {
                return Err(TransportBindingErrorV1::Invalid("self transport"));
            }
            let (initiator_endpoint, responder_endpoint) = if initiator {
                (local_endpoint, remote_endpoint)
            } else {
                (remote_endpoint, local_endpoint)
            };
            let mut hash = domain_hash(b"novovm.bound-carrier.connection.v1");
            hash.update(BOUND_HANDSHAKE_VERSION_V1.to_be_bytes());
            hash.update(Sha256::digest(PURPOSE));
            hash.update(Sha256::digest(BOUND_TRANSPORT_ALPN_V1));
            hash.update(initiator_endpoint);
            hash.update(responder_endpoint);
            hash.update(exporter);
            Ok(Self {
                initiator_endpoint,
                responder_endpoint,
                connection_binding: hash.finalize().into(),
                local_is_initiator: initiator,
            })
        })();
        exporter.zeroize();
        result
    }

    #[cfg(test)]
    fn context(&self, initiator: [u8; 32], responder: [u8; 32]) -> Result<BoundHandshakeContextV1> {
        self.context_for_mode(initiator, responder, BoundClockMode::UtcV1)
    }

    fn context_for_mode(
        &self,
        initiator: [u8; 32],
        responder: [u8; 32],
        mode: BoundClockMode,
    ) -> Result<BoundHandshakeContextV1> {
        let context = BoundHandshakeContextV1 {
            version: mode.version(),
            purpose: Sha256::digest(mode.purpose()).into(),
            initiator_identity_public_key: initiator,
            responder_identity_public_key: responder,
            initiator_transport_public_key: self.initiator_endpoint,
            responder_transport_public_key: self.responder_endpoint,
            connection_binding: self.connection_binding,
        };
        check_context_for_mode(&context, mode)?;
        Ok(context)
    }
}

/// An untrusted wire commitment. Equality with the locally derived context is
/// mandatory; this type by itself grants no transport or identity authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundHandshakeContextV1 {
    pub version: u16,
    pub purpose: [u8; 32],
    pub initiator_identity_public_key: [u8; 32],
    pub responder_identity_public_key: [u8; 32],
    pub initiator_transport_public_key: [u8; 32],
    pub responder_transport_public_key: [u8; 32],
    pub connection_binding: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundNodeHandshakeOfferV1 {
    pub context: BoundHandshakeContextV1,
    pub e2e: NodeHandshakeOfferV1,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundNodeHandshakeResponseV1 {
    pub context: BoundHandshakeContextV1,
    pub offer_hash: [u8; 32],
    pub e2e: NodeHandshakeResponseV1,
    pub signature: Vec<u8>,
}

impl BoundNodeHandshakeOfferV1 {
    pub fn encode_json(&self) -> Result<Vec<u8>> {
        check_offer_shape(self)?;
        encode_bounded(self)
    }

    /// Bound the input before serde allocates peer-controlled vectors/strings.
    /// This only decodes structure; `respond` performs authentication.
    pub fn decode_json(bytes: &[u8]) -> Result<Self> {
        check_wire_length(bytes)?;
        let value = serde_json::from_slice(bytes)?;
        check_offer_shape(&value)?;
        Ok(value)
    }
}

impl BoundNodeHandshakeResponseV1 {
    pub fn encode_json(&self) -> Result<Vec<u8>> {
        check_response_shape(self)?;
        encode_bounded(self)
    }

    pub fn decode_json(bytes: &[u8]) -> Result<Self> {
        check_wire_length(bytes)?;
        let value = serde_json::from_slice(bytes)?;
        check_response_shape(&value)?;
        Ok(value)
    }
}

pub struct BoundHandshakeInitiatorV1 {
    inner: NodeHandshakeInitiatorV1,
    offer: BoundNodeHandshakeOfferV1,
    mode: BoundClockMode,
}

impl BoundHandshakeInitiatorV1 {
    /// `identity` is a separate NOVOVM device/network key, never a wallet key.
    /// `expected_remote_key` must come from an independently authorized pin.
    pub fn start(
        identity: &SigningKey,
        expected_remote_key: [u8; 32],
        binding: VerifiedTransportBinding,
        now_ms: u64,
        ttl_ms: u64,
    ) -> Result<Self> {
        Self::start_with_mode(
            identity,
            expected_remote_key,
            binding,
            now_ms,
            ttl_ms,
            BoundClockMode::UtcV1,
        )
    }

    fn start_with_mode(
        identity: &SigningKey,
        expected_remote_key: [u8; 32],
        binding: VerifiedTransportBinding,
        now_ms: u64,
        ttl_ms: u64,
        mode: BoundClockMode,
    ) -> Result<Self> {
        if !binding.local_is_initiator {
            return Err(TransportBindingErrorV1::BindingMismatch);
        }
        check_ttl(ttl_ms)?;
        if mode == BoundClockMode::SessionV2 {
            mode.check_times(
                now_ms,
                now_ms
                    .checked_add(ttl_ms)
                    .ok_or(TransportBindingErrorV1::Expired)?,
            )?;
        }
        let context = binding.context_for_mode(
            identity.verifying_key().to_bytes(),
            expected_remote_key,
            mode,
        )?;
        let inner = NodeHandshakeInitiatorV1::start(
            identity,
            peer_id_from_ed25519_public_key_v1(&expected_remote_key),
            now_ms,
            ttl_ms,
        )?;
        let mut offer = BoundNodeHandshakeOfferV1 {
            context,
            e2e: inner.offer().clone(),
            signature: Vec::new(),
        };
        offer.signature = identity
            .sign(&offer_signing_digest(&offer))
            .to_bytes()
            .to_vec();
        check_offer_shape_for_mode(&offer, mode)?;
        Ok(Self { inner, offer, mode })
    }

    #[must_use]
    pub fn offer(&self) -> &BoundNodeHandshakeOfferV1 {
        &self.offer
    }

    pub fn complete(
        self,
        response: &BoundNodeHandshakeResponseV1,
        now_ms: u64,
        replay_cache: &mut HandshakeReplayCacheV1,
    ) -> Result<PendingBoundChannelV1> {
        check_response_shape_for_mode(response, self.mode)?;
        check_window(
            self.offer.e2e.issued_at_ms,
            self.offer.e2e.expires_at_ms,
            now_ms,
        )?;
        check_window(
            response.e2e.issued_at_ms,
            response.e2e.expires_at_ms,
            now_ms,
        )?;
        if response.context != self.offer.context || response.offer_hash != offer_hash(&self.offer)
        {
            return Err(TransportBindingErrorV1::BindingMismatch);
        }
        verify_signature(
            &self.offer.context.responder_identity_public_key,
            &response_signing_digest(response),
            &response.signature,
        )?;
        let transcript = transcript_hash(&self.offer, response);
        let expires_at_ms = self.offer.e2e.expires_at_ms.min(response.e2e.expires_at_ms);
        let channel = self.inner.complete(&response.e2e, now_ms, replay_cache)?;
        Ok(PendingBoundChannelV1::new(
            channel,
            self.offer.context,
            transcript,
            true,
            expires_at_ms,
        ))
    }
}

pub struct BoundHandshakeResponderV1 {
    response: BoundNodeHandshakeResponseV1,
    pending: PendingBoundChannelV1,
}

impl BoundHandshakeResponderV1 {
    pub fn respond(
        offer: &BoundNodeHandshakeOfferV1,
        identity: &SigningKey,
        expected_remote_key: [u8; 32],
        binding: VerifiedTransportBinding,
        now_ms: u64,
        ttl_ms: u64,
        replay_cache: &mut HandshakeReplayCacheV1,
    ) -> Result<Self> {
        Self::respond_with_mode(
            offer,
            identity,
            expected_remote_key,
            binding,
            now_ms,
            ttl_ms,
            replay_cache,
            BoundClockMode::UtcV1,
        )
    }

    // Keep the existing V1 proof inputs intact while selecting an explicit
    // local mode. Both entries must share this verifier, not duplicate it.
    #[allow(clippy::too_many_arguments)]
    fn respond_with_mode(
        offer: &BoundNodeHandshakeOfferV1,
        identity: &SigningKey,
        expected_remote_key: [u8; 32],
        binding: VerifiedTransportBinding,
        now_ms: u64,
        ttl_ms: u64,
        replay_cache: &mut HandshakeReplayCacheV1,
        mode: BoundClockMode,
    ) -> Result<Self> {
        if binding.local_is_initiator {
            return Err(TransportBindingErrorV1::BindingMismatch);
        }
        check_ttl(ttl_ms)?;
        if mode == BoundClockMode::SessionV2 {
            mode.check_times(
                now_ms,
                now_ms
                    .checked_add(ttl_ms)
                    .ok_or(TransportBindingErrorV1::Expired)?,
            )?;
        }
        check_offer_shape_for_mode(offer, mode)?;
        check_window(offer.e2e.issued_at_ms, offer.e2e.expires_at_ms, now_ms)?;
        let context = binding.context_for_mode(
            expected_remote_key,
            identity.verifying_key().to_bytes(),
            mode,
        )?;
        if offer.context != context {
            return Err(TransportBindingErrorV1::BindingMismatch);
        }
        verify_signature(
            &expected_remote_key,
            &offer_signing_digest(offer),
            &offer.signature,
        )?;
        // Never consume the replay cache or derive E2E secrets before the outer
        // connection/identity proof has passed.
        let inner = NodeHandshakeResponderV1::respond(
            &offer.e2e,
            identity,
            now_ms,
            ttl_ms.min(offer.e2e.expires_at_ms - now_ms),
            replay_cache,
        )?;
        let mut response = BoundNodeHandshakeResponseV1 {
            context,
            offer_hash: offer_hash(offer),
            e2e: inner.response().clone(),
            signature: Vec::new(),
        };
        response.signature = identity
            .sign(&response_signing_digest(&response))
            .to_bytes()
            .to_vec();
        check_response_shape_for_mode(&response, mode)?;
        let transcript = transcript_hash(offer, &response);
        let expires_at_ms = offer.e2e.expires_at_ms.min(response.e2e.expires_at_ms);
        let context = response.context.clone();
        Ok(Self {
            response,
            pending: PendingBoundChannelV1::new(
                inner.into_channel(),
                context,
                transcript,
                false,
                expires_at_ms,
            ),
        })
    }

    #[must_use]
    pub fn response(&self) -> &BoundNodeHandshakeResponseV1 {
        &self.response
    }

    #[must_use]
    pub fn into_pending(self) -> PendingBoundChannelV1 {
        self.pending
    }
}

/// The channel remains inaccessible until a role-separated encrypted Finished
/// carrying the full bound transcript has been authenticated in each direction.
/// The caller must send its Finished successfully before `into_channel`; creating
/// an envelope is not a transport send or application delivery acknowledgement.
pub struct PendingBoundChannelV1 {
    channel: E2eSecureChannelV1,
    context: BoundHandshakeContextV1,
    transcript: [u8; 32],
    initiator: bool,
    expires_at_ms: u64,
    local_finished: bool,
    remote_finished: bool,
    failed: bool,
}

impl PendingBoundChannelV1 {
    fn new(
        channel: E2eSecureChannelV1,
        context: BoundHandshakeContextV1,
        transcript: [u8; 32],
        initiator: bool,
        expires_at_ms: u64,
    ) -> Self {
        Self {
            channel,
            context,
            transcript,
            initiator,
            expires_at_ms,
            local_finished: false,
            remote_finished: false,
            failed: false,
        }
    }

    /// Canonical digest already covered by both identity proofs and carried
    /// inside Finished. It is not a stand-alone authorization token.
    #[must_use]
    pub fn transcript_digest(&self) -> [u8; 32] {
        self.transcript
    }

    pub fn make_finished(&mut self, now_ms: u64) -> Result<SecureNovoRudpEnvelopeV1> {
        self.check_pending(now_ms)?;
        if self.local_finished {
            return Err(TransportBindingErrorV1::Finished);
        }
        let frame = self.finished_frame(self.initiator);
        self.failed = true;
        let envelope = self.channel.seal_novorudp_frame(&frame)?;
        self.local_finished = true;
        self.failed = false;
        Ok(envelope)
    }

    pub fn verify_finished(
        &mut self,
        envelope: &SecureNovoRudpEnvelopeV1,
        now_ms: u64,
    ) -> Result<()> {
        self.check_pending(now_ms)?;
        // Any malformed first frame is terminal. Never retry a downgraded or
        // partly authenticated handshake on this E2E state.
        self.failed = true;
        if self.remote_finished || envelope.sequence != 0 || envelope.ciphertext.len() > 512 {
            return Err(TransportBindingErrorV1::Finished);
        }
        check_envelope_shape(envelope)?;
        let frame = self.channel.open_novorudp_frame(envelope)?;
        if frame != self.finished_frame(!self.initiator) {
            return Err(TransportBindingErrorV1::Finished);
        }
        self.remote_finished = true;
        self.failed = false;
        Ok(())
    }

    pub fn into_channel(self, now_ms: u64) -> Result<BoundSecureChannelV1> {
        self.check_pending(now_ms)?;
        if !self.local_finished || !self.remote_finished {
            return Err(TransportBindingErrorV1::Finished);
        }
        Ok(BoundSecureChannelV1 {
            channel: self.channel,
            context: self.context,
            initiator: self.initiator,
        })
    }

    fn check_pending(&self, now_ms: u64) -> Result<()> {
        if self.failed {
            return Err(TransportBindingErrorV1::Finished);
        }
        if now_ms >= self.expires_at_ms {
            return Err(TransportBindingErrorV1::Expired);
        }
        Ok(())
    }

    fn finished_frame(&self, sender_is_initiator: bool) -> NovoRudpTransportFrameV0 {
        let mut payload = Vec::with_capacity(FINISHED_DOMAIN.len() + 33);
        payload.extend_from_slice(FINISHED_DOMAIN);
        payload.push(if sender_is_initiator { 1 } else { 2 });
        payload.extend_from_slice(&self.transcript);
        NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Endpoint,
            self.channel.session_id(),
            FINISHED_STREAM,
            0,
            0,
            0,
            payload,
        )
    }
}

/// Only returned after Finished validation. This does not relax the carrier's
/// privacy/cancellation policy and does not establish an application receipt.
pub struct BoundSecureChannelV1 {
    channel: E2eSecureChannelV1,
    context: BoundHandshakeContextV1,
    initiator: bool,
}

impl BoundSecureChannelV1 {
    /// Locally verified handshake mode, not a version supplied by a new frame.
    /// Explicit time-aware services must reject a completed legacy channel.
    pub fn uses_session_clock_v2(&self) -> bool {
        self.context.version == BoundClockMode::SessionV2.version()
    }

    /// Role fixed by the completed, carrier-bound handshake.
    pub fn local_is_initiator(&self) -> bool {
        self.initiator
    }

    /// Recheck this completed channel against the actual local carrier before
    /// transferring ownership. Neither a node key nor a route flag substitutes
    /// for the exact connection/stream exporter and authenticated direction.
    pub fn verify_transport_binding(&self, binding: &VerifiedTransportBinding) -> Result<()> {
        let mode = BoundClockMode::from_validated_version(self.context.version)?;
        if self.initiator != binding.local_is_initiator
            || self.context
                != binding.context_for_mode(
                    self.context.initiator_identity_public_key,
                    self.context.responder_identity_public_key,
                    mode,
                )?
        {
            return Err(TransportBindingErrorV1::BindingMismatch);
        }
        Ok(())
    }

    #[must_use]
    pub fn local_identity_public_key(&self) -> [u8; 32] {
        if self.initiator {
            self.context.initiator_identity_public_key
        } else {
            self.context.responder_identity_public_key
        }
    }

    #[must_use]
    pub fn local_peer_id(&self) -> &str {
        self.channel.local_peer_id()
    }
    #[must_use]
    pub fn remote_peer_id(&self) -> &str {
        self.channel.remote_peer_id()
    }
    #[must_use]
    pub fn session_id(&self) -> [u8; 16] {
        self.channel.session_id()
    }

    pub fn seal_novorudp_frame(
        &mut self,
        frame: &NovoRudpTransportFrameV0,
    ) -> Result<SecureNovoRudpEnvelopeV1> {
        self.check_business_frame(frame)?;
        Ok(self.channel.seal_novorudp_frame(frame)?)
    }

    pub fn open_novorudp_frame(
        &mut self,
        envelope: &SecureNovoRudpEnvelopeV1,
    ) -> Result<NovoRudpTransportFrameV0> {
        check_envelope_shape(envelope)?;
        let frame = self.channel.open_novorudp_frame(envelope)?;
        self.check_business_frame(&frame)?;
        Ok(frame)
    }

    fn check_business_frame(&self, frame: &NovoRudpTransportFrameV0) -> Result<()> {
        if frame.payload.len() > BOUND_CHANNEL_MAX_PAYLOAD_BYTES_V1 {
            return Err(TransportBindingErrorV1::ResourceLimit);
        }
        if frame.stream_id == FINISHED_STREAM || frame.session_id != self.channel.session_id() {
            return Err(TransportBindingErrorV1::Invalid("business frame route"));
        }
        Ok(())
    }
}

fn check_public_key(bytes: &[u8; 32]) -> Result<()> {
    let key = VerifyingKey::from_bytes(bytes)
        .map_err(|_| TransportBindingErrorV1::Invalid("public key"))?;
    if key.is_weak() {
        return Err(TransportBindingErrorV1::Invalid("weak public key"));
    }
    Ok(())
}

fn check_context_for_mode(context: &BoundHandshakeContextV1, mode: BoundClockMode) -> Result<()> {
    if context.version != mode.version()
        || context.purpose != <[u8; 32]>::from(Sha256::digest(mode.purpose()))
    {
        return Err(TransportBindingErrorV1::Invalid("version or purpose"));
    }
    let keys = [
        context.initiator_identity_public_key,
        context.responder_identity_public_key,
        context.initiator_transport_public_key,
        context.responder_transport_public_key,
    ];
    for (index, key) in keys.iter().enumerate() {
        check_public_key(key)?;
        if keys[..index].contains(key) {
            return Err(TransportBindingErrorV1::Invalid(
                "identity/transport key reuse",
            ));
        }
    }
    Ok(())
}

fn check_ttl(ttl_ms: u64) -> Result<()> {
    if ttl_ms == 0 || ttl_ms > BOUND_HANDSHAKE_MAX_TTL_MS_V1 {
        return Err(TransportBindingErrorV1::Invalid("handshake TTL"));
    }
    Ok(())
}

fn check_lifetime(issued_at_ms: u64, expires_at_ms: u64) -> Result<()> {
    check_ttl(
        expires_at_ms
            .checked_sub(issued_at_ms)
            .ok_or(TransportBindingErrorV1::Expired)?,
    )
}

fn check_window(issued_at_ms: u64, expires_at_ms: u64, now_ms: u64) -> Result<()> {
    check_lifetime(issued_at_ms, expires_at_ms)?;
    if now_ms >= expires_at_ms
        || issued_at_ms > now_ms.saturating_add(PRODUCT_OVERLAY_CLOCK_SKEW_MS_V1)
    {
        return Err(TransportBindingErrorV1::Expired);
    }
    Ok(())
}

fn check_offer_shape(offer: &BoundNodeHandshakeOfferV1) -> Result<()> {
    check_offer_shape_for_mode(offer, BoundClockMode::UtcV1)
}

fn check_offer_shape_for_mode(
    offer: &BoundNodeHandshakeOfferV1,
    mode: BoundClockMode,
) -> Result<()> {
    let e = &offer.e2e;
    check_context_for_mode(&offer.context, mode)?;
    check_lifetime(e.issued_at_ms, e.expires_at_ms)?;
    mode.check_times(e.issued_at_ms, e.expires_at_ms)?;
    if e.version != PRODUCT_OVERLAY_PROTOCOL_VERSION_V1
        || e.signature.len() != 64
        || offer.signature.len() != 64
        || e.initiator_ephemeral_public_key.len() != 33
        || e.initiator_identity_public_key != offer.context.initiator_identity_public_key
        || e.initiator_peer_id
            != peer_id_from_ed25519_public_key_v1(&offer.context.initiator_identity_public_key)
        || e.responder_peer_id
            != peer_id_from_ed25519_public_key_v1(&offer.context.responder_identity_public_key)
    {
        return Err(TransportBindingErrorV1::Invalid("offer fields"));
    }
    Ok(())
}

fn check_response_shape(response: &BoundNodeHandshakeResponseV1) -> Result<()> {
    check_response_shape_for_mode(response, BoundClockMode::UtcV1)
}

fn check_response_shape_for_mode(
    response: &BoundNodeHandshakeResponseV1,
    mode: BoundClockMode,
) -> Result<()> {
    let e = &response.e2e;
    check_context_for_mode(&response.context, mode)?;
    check_lifetime(e.issued_at_ms, e.expires_at_ms)?;
    mode.check_times(e.issued_at_ms, e.expires_at_ms)?;
    if e.version != PRODUCT_OVERLAY_PROTOCOL_VERSION_V1
        || e.signature.len() != 64
        || response.signature.len() != 64
        || e.responder_ephemeral_public_key.len() != 33
        || e.responder_identity_public_key != response.context.responder_identity_public_key
        || e.initiator_peer_id
            != peer_id_from_ed25519_public_key_v1(&response.context.initiator_identity_public_key)
        || e.responder_peer_id
            != peer_id_from_ed25519_public_key_v1(&response.context.responder_identity_public_key)
    {
        return Err(TransportBindingErrorV1::Invalid("response fields"));
    }
    Ok(())
}

fn check_envelope_shape(envelope: &SecureNovoRudpEnvelopeV1) -> Result<()> {
    if envelope.sender_peer_id.len() != PEER_ID_BYTES
        || envelope.recipient_peer_id.len() != PEER_ID_BYTES
        || envelope.ciphertext.len() > BOUND_CHANNEL_MAX_PAYLOAD_BYTES_V1 + 256
    {
        return Err(TransportBindingErrorV1::ResourceLimit);
    }
    Ok(())
}

fn check_wire_length(bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() || bytes.len() > BOUND_HANDSHAKE_MAX_WIRE_BYTES_V1 {
        return Err(TransportBindingErrorV1::ResourceLimit);
    }
    Ok(())
}

fn encode_bounded(value: &impl Serialize) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value)?;
    check_wire_length(&bytes)?;
    Ok(bytes)
}

fn verify_signature(public_key: &[u8; 32], digest: &[u8; 32], signature: &[u8]) -> Result<()> {
    let bytes: [u8; 64] = signature
        .try_into()
        .map_err(|_| TransportBindingErrorV1::InvalidSignature)?;
    let key = VerifyingKey::from_bytes(public_key)
        .map_err(|_| TransportBindingErrorV1::InvalidSignature)?;
    if key.is_weak() {
        return Err(TransportBindingErrorV1::InvalidSignature);
    }
    key.verify_strict(digest, &Signature::from_bytes(&bytes))
        .map_err(|_| TransportBindingErrorV1::InvalidSignature)
}

// Canonical fixed-width hashing: domains are hashed to 32 bytes, every remaining
// component is fixed-width after shape validation. No JSON ordering, native
// endian representation, ignored suffix, or variable-length concatenation.
fn domain_hash(domain: &[u8]) -> Sha256 {
    let mut hash = Sha256::new();
    hash.update(Sha256::digest(domain));
    hash
}

fn hash_context(hash: &mut Sha256, context: &BoundHandshakeContextV1) {
    hash.update(context.version.to_be_bytes());
    hash.update(context.purpose);
    hash.update(context.initiator_identity_public_key);
    hash.update(context.responder_identity_public_key);
    hash.update(context.initiator_transport_public_key);
    hash.update(context.responder_transport_public_key);
    hash.update(context.connection_binding);
}

fn inner_offer_hash(e: &NodeHandshakeOfferV1) -> [u8; 32] {
    let mut hash = domain_hash(b"novovm.bound-carrier.inner-offer.v1");
    hash.update(e.version.to_be_bytes());
    hash.update(e.session_id);
    hash.update(Sha256::digest(e.initiator_peer_id.as_bytes()));
    hash.update(Sha256::digest(e.responder_peer_id.as_bytes()));
    hash.update(e.initiator_identity_public_key);
    hash.update(&e.initiator_ephemeral_public_key);
    hash.update(e.challenge);
    hash.update(e.issued_at_ms.to_be_bytes());
    hash.update(e.expires_at_ms.to_be_bytes());
    hash.update(&e.signature);
    hash.finalize().into()
}

fn inner_response_hash(e: &NodeHandshakeResponseV1) -> [u8; 32] {
    let mut hash = domain_hash(b"novovm.bound-carrier.inner-response.v1");
    hash.update(e.version.to_be_bytes());
    hash.update(e.session_id);
    hash.update(Sha256::digest(e.initiator_peer_id.as_bytes()));
    hash.update(Sha256::digest(e.responder_peer_id.as_bytes()));
    hash.update(e.responder_identity_public_key);
    hash.update(&e.responder_ephemeral_public_key);
    hash.update(e.challenge);
    hash.update(e.response_nonce);
    hash.update(e.offer_hash);
    hash.update(e.issued_at_ms.to_be_bytes());
    hash.update(e.expires_at_ms.to_be_bytes());
    hash.update(&e.signature);
    hash.finalize().into()
}

fn offer_signing_digest(offer: &BoundNodeHandshakeOfferV1) -> [u8; 32] {
    let mut hash = domain_hash(b"novovm.bound-carrier.offer-proof.v1");
    hash.update([1u8]);
    hash_context(&mut hash, &offer.context);
    hash.update(inner_offer_hash(&offer.e2e));
    hash.finalize().into()
}

fn offer_hash(offer: &BoundNodeHandshakeOfferV1) -> [u8; 32] {
    let mut hash = domain_hash(b"novovm.bound-carrier.signed-offer.v1");
    hash.update(offer_signing_digest(offer));
    hash.update(&offer.signature);
    hash.finalize().into()
}

fn response_signing_digest(response: &BoundNodeHandshakeResponseV1) -> [u8; 32] {
    let mut hash = domain_hash(b"novovm.bound-carrier.response-proof.v1");
    hash.update([2u8]);
    hash_context(&mut hash, &response.context);
    hash.update(response.offer_hash);
    hash.update(inner_response_hash(&response.e2e));
    hash.finalize().into()
}

fn transcript_hash(
    offer: &BoundNodeHandshakeOfferV1,
    response: &BoundNodeHandshakeResponseV1,
) -> [u8; 32] {
    let mut hash = domain_hash(b"novovm.bound-carrier.transcript.v1");
    hash.update(offer_hash(offer));
    hash.update(response_signing_digest(response));
    hash.update(&response.signature);
    hash.finalize().into()
}

#[cfg(test)]
mod tests {
    mod carrier_feedback_tests {
        include!("transport_binding/carrier_feedback_tests.rs");
    }

    use super::*;

    const NOW: u64 = 100_000;
    const TTL: u64 = 10_000;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }
    fn public(seed: u8) -> [u8; 32] {
        key(seed).verifying_key().to_bytes()
    }

    // Test-only connection evidence. Production can construct the private
    // evidence type only inside the authenticated carrier's crate boundary.
    fn binding(initiator: bool, exporter: u8) -> VerifiedTransportBinding {
        let (local, remote) = if initiator {
            (public(3), public(4))
        } else {
            (public(4), public(3))
        };
        VerifiedTransportBinding::from_authenticated_transport(
            local,
            remote,
            [exporter; 32],
            BOUND_TRANSPORT_ALPN_V1,
            initiator,
        )
        .unwrap()
    }

    fn start() -> BoundHandshakeInitiatorV1 {
        BoundHandshakeInitiatorV1::start(&key(1), public(2), binding(true, 7), NOW, TTL).unwrap()
    }

    fn respond(offer: &BoundNodeHandshakeOfferV1) -> BoundHandshakeResponderV1 {
        BoundHandshakeResponderV1::respond(
            offer,
            &key(2),
            public(1),
            binding(false, 7),
            NOW,
            TTL,
            &mut HandshakeReplayCacheV1::new(8),
        )
        .unwrap()
    }

    fn pending_pair() -> (PendingBoundChannelV1, PendingBoundChannelV1) {
        let initiator = start();
        let responder = respond(initiator.offer());
        let left = initiator
            .complete(
                responder.response(),
                NOW,
                &mut HandshakeReplayCacheV1::new(8),
            )
            .unwrap();
        (left, responder.into_pending())
    }

    fn finished_pair() -> (BoundSecureChannelV1, BoundSecureChannelV1) {
        let (mut left, mut right) = pending_pair();
        let a = left.make_finished(NOW).unwrap();
        let b = right.make_finished(NOW).unwrap();
        left.verify_finished(&b, NOW).unwrap();
        right.verify_finished(&a, NOW).unwrap();
        (
            left.into_channel(NOW).unwrap(),
            right.into_channel(NOW).unwrap(),
        )
    }

    #[test]
    fn authenticated_bound_handshake_finished_and_bidirectional_business() {
        let initiator = start();
        let wire = initiator.offer().encode_json().unwrap();
        assert!(wire.len() < BOUND_HANDSHAKE_MAX_WIRE_BYTES_V1);
        let offer = BoundNodeHandshakeOfferV1::decode_json(&wire).unwrap();
        assert_eq!(&offer, initiator.offer());
        let responder = respond(&offer);
        let response =
            BoundNodeHandshakeResponseV1::decode_json(&responder.response().encode_json().unwrap())
                .unwrap();
        let mut left = initiator
            .complete(&response, NOW, &mut HandshakeReplayCacheV1::new(8))
            .unwrap();
        let mut right = responder.into_pending();
        assert_eq!(left.transcript_digest(), right.transcript_digest());
        let a = left.make_finished(NOW).unwrap();
        let b = right.make_finished(NOW).unwrap();
        assert_eq!(a.sequence, 0);
        assert_eq!(b.sequence, 0);
        left.verify_finished(&b, NOW).unwrap();
        right.verify_finished(&a, NOW).unwrap();
        let mut left = left.into_channel(NOW).unwrap();
        let mut right = right.into_channel(NOW).unwrap();
        assert_eq!(left.local_peer_id(), right.remote_peer_id());
        assert_eq!(left.remote_peer_id(), right.local_peer_id());
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            left.session_id(),
            1,
            7,
            0,
            0,
            vec![0, 0xff, 7, 0],
        );
        let envelope = left.seal_novorudp_frame(&frame).unwrap();
        assert_eq!(envelope.sequence, 1);
        assert_eq!(right.open_novorudp_frame(&envelope).unwrap(), frame);
        assert!(right.open_novorudp_frame(&envelope).is_err());
        let return_envelope = right.seal_novorudp_frame(&frame).unwrap();
        assert_eq!(left.open_novorudp_frame(&return_envelope).unwrap(), frame);
    }

    #[test]
    fn completed_bound_channel_rejects_other_carrier_generation_and_direction() {
        let (left, right) = finished_pair();
        left.verify_transport_binding(&binding(true, 7)).unwrap();
        right.verify_transport_binding(&binding(false, 7)).unwrap();
        assert_eq!(left.local_identity_public_key(), public(1));
        assert_eq!(right.local_identity_public_key(), public(2));
        assert!(left.verify_transport_binding(&binding(true, 8)).is_err());
        assert!(left.verify_transport_binding(&binding(false, 7)).is_err());
        let different_peer = VerifiedTransportBinding::from_authenticated_transport(
            public(3),
            public(5),
            [7; 32],
            BOUND_TRANSPORT_ALPN_V1,
            true,
        )
        .unwrap();
        assert!(left.verify_transport_binding(&different_peer).is_err());
    }

    #[test]
    fn completed_bound_channel_native_frames_preserve_replay_and_source_checks() {
        use crate::novorudp::{
            carrier::{NativeRoute, VerifiedCarrierRoute},
            datagram::NativeUdpSession,
        };
        let (left, right) = finished_pair();
        let mut left = NativeUdpSession::from_bound_channel(left, &binding(true, 7)).unwrap();
        let mut right = NativeUdpSession::from_bound_channel(right, &binding(false, 7)).unwrap();
        let incoming = NativeRoute::Carrier(VerifiedCarrierRoute::from_binding(&binding(false, 7)));
        let wrong = NativeRoute::Carrier(VerifiedCarrierRoute::from_binding(&binding(false, 8)));
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            left.session_id(),
            11,
            0,
            0,
            0,
            vec![42; 848],
        );
        let wire = left.encode(&frame).unwrap();
        assert!(wire.starts_with(b"NVRLY002"));
        assert!(right.open_route(wrong, &wire).is_err());
        let mut tampered = wire.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(right.open_route(incoming, &tampered).is_err());
        assert_eq!(right.open_route(incoming, &wire).unwrap(), frame);
        assert!(right.open_route(incoming, &wire).is_err());
        let repair = left.encode(&frame).unwrap();
        assert_ne!(repair, wire);
        assert_eq!(right.open_route(incoming, &repair).unwrap(), frame);
        let response = right.encode(&frame).unwrap();
        let left_incoming =
            NativeRoute::Carrier(VerifiedCarrierRoute::from_binding(&binding(true, 7)));
        assert_eq!(left.open_route(left_incoming, &response).unwrap(), frame);
    }

    #[test]
    fn completed_bound_channel_adopts_native_endpoint_without_udp_handshake() {
        use crate::novorudp::{carrier::NativeDatagramIo, endpoint::NativeEndpoint};
        use std::{
            io,
            sync::Arc,
            time::{Duration, Instant},
        };
        struct IdleIo;
        impl NativeDatagramIo for IdleIo {
            fn try_receive(
                &self,
                _: &mut [u8],
            ) -> io::Result<Option<(usize, crate::novorudp::carrier::NativeRoute)>> {
                Err(io::ErrorKind::WouldBlock.into())
            }
            fn try_send(
                &self,
                _: &[u8],
                _: crate::novorudp::carrier::NativeRoute,
            ) -> io::Result<usize> {
                panic!("adoption must not emit a second handshake")
            }
        }
        let (left, _) = finished_pair();
        let session = left.session_id();
        let mut endpoint =
            NativeEndpoint::from_bound_channel(Arc::new(IdleIo), left, &binding(true, 7), vec![11])
                .unwrap();
        assert_eq!(endpoint.session_id(0), Some(session));
        assert_eq!(endpoint.local_identity(), public(1));
        assert_eq!(
            endpoint.peer_id(0).unwrap(),
            peer_id_from_ed25519_public_key_v1(&public(2))
        );
        assert!(!endpoint.accepts_clients());
        assert!(endpoint.local_addr().is_err());
        let later = Instant::now() + Duration::from_secs(120);
        assert_eq!(endpoint.expired_handshakes(later), 0);
        assert_eq!(
            endpoint
                .poll(NOW + 120_000, later, 1)
                .unwrap()
                .handshakes_sent,
            0
        );
        let (left, _) = finished_pair();
        assert!(NativeEndpoint::from_bound_channel(
            Arc::new(IdleIo),
            left,
            &binding(true, 8),
            vec![11]
        )
        .is_err());
        let (left, _) = finished_pair();
        assert!(NativeEndpoint::from_bound_channel(
            Arc::new(IdleIo),
            left,
            &binding(true, 7),
            vec![u64::MAX]
        )
        .is_err());
    }

    #[test]
    fn completed_bound_endpoint_rejects_nested_handshake_and_propagates_carrier_reset() {
        use crate::novorudp::{
            carrier::{NativeDatagramIo, NativeRoute, VerifiedCarrierRoute},
            datagram::NativeUdpSession,
            endpoint::NativeEndpoint,
        };
        use std::{
            collections::VecDeque,
            io,
            sync::{Arc, Mutex},
            time::Instant,
        };
        struct Input {
            // Test fixture: exactly two bounded frames followed by one error.
            events: Mutex<VecDeque<io::Result<Vec<u8>>>>,
            route: NativeRoute,
        }
        impl NativeDatagramIo for Input {
            fn try_receive(&self, target: &mut [u8]) -> io::Result<Option<(usize, NativeRoute)>> {
                let event = self
                    .events
                    .lock()
                    .unwrap()
                    .pop_front()
                    .ok_or(io::ErrorKind::WouldBlock)??;
                if event.len() > target.len() {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                target[..event.len()].copy_from_slice(&event);
                Ok(Some((event.len(), self.route)))
            }
            fn try_send(&self, _: &[u8], _: NativeRoute) -> io::Result<usize> {
                panic!("nested handshake must not send any response")
            }
        }
        for failure in [
            io::ErrorKind::ConnectionReset,
            io::ErrorKind::ConnectionRefused,
            io::ErrorKind::Interrupted,
        ] {
            let (left, right) = finished_pair();
            let session = left.session_id();
            let mut right =
                NativeUdpSession::from_bound_channel(right, &binding(false, 7)).unwrap();
            let frame = NovoRudpTransportFrameV0::new(
                NovoRudpTransportFrameKindV0::Data,
                session,
                11,
                0,
                0,
                0,
                vec![9; 32],
            );
            let old_offer = crate::product_overlay::NodeHandshakeInitiatorV1::start(
                &key(2),
                peer_id_from_ed25519_public_key_v1(&public(1)),
                NOW,
                TTL,
            )
            .unwrap();
            let nested =
                crate::product_overlay::handshake_wire::encode_offer(old_offer.offer()).unwrap();
            let input = Arc::new(Input {
                events: Mutex::new(VecDeque::from([
                    Ok(nested),
                    Ok(right.encode(&frame).unwrap()),
                    Err(failure.into()),
                ])),
                route: NativeRoute::Carrier(VerifiedCarrierRoute::from_binding(&binding(true, 7))),
            });
            let mut endpoint =
                NativeEndpoint::from_bound_channel(input, left, &binding(true, 7), vec![11])
                    .unwrap();
            let report = endpoint.poll(NOW, Instant::now(), 2).unwrap();
            assert_eq!(report.rejected_datagrams, 1);
            assert_eq!(report.handshakes_sent, 0);
            assert_eq!(report.frames.len(), 1);
            assert_eq!(report.frames[0].frame(), &frame);
            assert_eq!(endpoint.session_id(0), Some(session));
            let error = endpoint
                .poll(NOW, Instant::now(), 1)
                .err()
                .expect("carrier failure must reach owner");
            assert_eq!(error.downcast_ref::<io::Error>().unwrap().kind(), failure);
        }
    }

    #[test]
    fn peer_id_wire_bound_matches_existing_identity_encoder() {
        for seed in [1, 2, 255] {
            assert_eq!(
                peer_id_from_ed25519_public_key_v1(&public(seed)).len(),
                PEER_ID_BYTES
            );
        }
        let (mut left, _) = pending_pair();
        let envelope = left.make_finished(NOW).unwrap();
        check_envelope_shape(&envelope).unwrap();
        for delta in [-1isize, 1] {
            let mut malformed = envelope.clone();
            malformed.sender_peer_id = "x".repeat((PEER_ID_BYTES as isize + delta) as usize);
            assert!(matches!(
                check_envelope_shape(&malformed),
                Err(TransportBindingErrorV1::ResourceLimit)
            ));
            let mut malformed = envelope.clone();
            malformed.recipient_peer_id = "x".repeat((PEER_ID_BYTES as isize + delta) as usize);
            assert!(matches!(
                check_envelope_shape(&malformed),
                Err(TransportBindingErrorV1::ResourceLimit)
            ));
        }
    }

    #[test]
    fn raw_exporter_endpoint_order_and_alpn_are_local_evidence() {
        let first = binding(true, 7);
        let second = binding(false, 7);
        assert_eq!(
            first.context(public(1), public(2)).unwrap(),
            second.context(public(1), public(2)).unwrap()
        );
        assert_ne!(
            first.connection_binding,
            binding(true, 8).connection_binding
        );
        let swapped = VerifiedTransportBinding::from_authenticated_transport(
            public(4),
            public(3),
            [7; 32],
            BOUND_TRANSPORT_ALPN_V1,
            true,
        )
        .unwrap();
        assert_ne!(first.connection_binding, swapped.connection_binding);
        assert!(VerifiedTransportBinding::from_authenticated_transport(
            public(3),
            public(4),
            [7; 32],
            b"legacy",
            true
        )
        .is_err());
        assert!(VerifiedTransportBinding::from_authenticated_transport(
            public(3),
            public(4),
            [0; 32],
            BOUND_TRANSPORT_ALPN_V1,
            true
        )
        .is_err());
        assert!(VerifiedTransportBinding::from_authenticated_transport(
            public(3),
            public(3),
            [7; 32],
            BOUND_TRANSPORT_ALPN_V1,
            true
        )
        .is_err());
        assert!(VerifiedTransportBinding::from_authenticated_transport(
            [0; 32],
            public(3),
            [7; 32],
            BOUND_TRANSPORT_ALPN_V1,
            true
        )
        .is_err());
    }

    #[test]
    fn wrong_connection_and_peer_pin_rejected_without_poisoning_replay_cache() {
        let initiator = start();
        let mut cache = HandshakeReplayCacheV1::new(8);
        assert!(matches!(
            BoundHandshakeResponderV1::respond(
                initiator.offer(),
                &key(2),
                public(1),
                binding(false, 8),
                NOW,
                TTL,
                &mut cache
            ),
            Err(TransportBindingErrorV1::BindingMismatch)
        ));
        assert!(BoundHandshakeResponderV1::respond(
            initiator.offer(),
            &key(2),
            public(5),
            binding(false, 7),
            NOW,
            TTL,
            &mut cache
        )
        .is_err());
        assert!(BoundHandshakeResponderV1::respond(
            initiator.offer(),
            &key(2),
            public(1),
            binding(true, 7),
            NOW,
            TTL,
            &mut cache
        )
        .is_err());
        assert!(BoundHandshakeResponderV1::respond(
            initiator.offer(),
            &key(2),
            public(1),
            binding(false, 7),
            NOW,
            TTL,
            &mut cache
        )
        .is_ok());
        assert!(matches!(
            BoundHandshakeResponderV1::respond(
                initiator.offer(),
                &key(2),
                public(1),
                binding(false, 7),
                NOW,
                TTL,
                &mut cache
            ),
            Err(TransportBindingErrorV1::Overlay(
                ProductOverlayErrorV1::HandshakeReplay
            ))
        ));
    }

    #[test]
    fn transport_identity_reuse_and_reflection_are_forbidden() {
        assert!(
            BoundHandshakeInitiatorV1::start(&key(3), public(2), binding(true, 7), NOW, TTL)
                .is_err()
        );
        assert!(
            BoundHandshakeInitiatorV1::start(&key(1), public(4), binding(true, 7), NOW, TTL)
                .is_err()
        );
        assert!(
            BoundHandshakeInitiatorV1::start(&key(1), public(1), binding(true, 7), NOW, TTL)
                .is_err()
        );
        assert!(
            BoundHandshakeInitiatorV1::start(&key(1), [0; 32], binding(true, 7), NOW, TTL).is_err()
        );
        assert!(
            BoundHandshakeInitiatorV1::start(&key(1), public(2), binding(false, 7), NOW, TTL)
                .is_err()
        );
    }

    #[test]
    fn offer_signature_covers_purpose_version_session_and_all_inner_fields() {
        let initiator = start();
        for field in 0..14 {
            let mut offer = initiator.offer().clone();
            match field {
                0 => offer.context.purpose[0] ^= 1,
                1 => offer.context.version += 1,
                2 => offer.context.connection_binding[0] ^= 1,
                3 => offer.context.initiator_transport_public_key = public(5),
                4 => offer.e2e.session_id[0] ^= 1,
                5 => offer.e2e.challenge[0] ^= 1,
                6 => offer.e2e.issued_at_ms += 1,
                7 => offer.e2e.expires_at_ms -= 1,
                8 => offer.e2e.initiator_ephemeral_public_key[5] ^= 1,
                9 => offer.e2e.signature[0] ^= 1,
                10 => offer.signature[0] ^= 1,
                11 => offer.context.responder_identity_public_key = public(5),
                12 => offer.e2e.responder_peer_id.push('0'),
                13 => offer.e2e.initiator_identity_public_key = public(5),
                _ => unreachable!(),
            }
            assert!(
                BoundHandshakeResponderV1::respond(
                    &offer,
                    &key(2),
                    public(1),
                    binding(false, 7),
                    NOW,
                    TTL,
                    &mut HandshakeReplayCacheV1::new(8)
                )
                .is_err(),
                "offer field {field}"
            );
        }
    }

    #[test]
    fn signed_outer_offer_does_not_bypass_existing_inner_signature() {
        let initiator = start();
        let mut offer = initiator.offer().clone();
        offer.e2e.signature[0] ^= 1;
        offer.signature = key(1)
            .sign(&offer_signing_digest(&offer))
            .to_bytes()
            .to_vec();
        assert!(matches!(
            BoundHandshakeResponderV1::respond(
                &offer,
                &key(2),
                public(1),
                binding(false, 7),
                NOW,
                TTL,
                &mut HandshakeReplayCacheV1::new(8)
            ),
            Err(TransportBindingErrorV1::Overlay(
                ProductOverlayErrorV1::InvalidHandshakeSignature
            ))
        ));
    }

    #[test]
    fn response_signature_and_link_reject_tamper_and_cross_role_signature() {
        for field in 0..13 {
            let initiator = start();
            let mut response = respond(initiator.offer()).response().clone();
            match field {
                0 => response.context.connection_binding[0] ^= 1,
                1 => response.offer_hash[0] ^= 1,
                2 => response.e2e.session_id[0] ^= 1,
                3 => response.e2e.response_nonce[0] ^= 1,
                4 => response.e2e.offer_hash[0] ^= 1,
                5 => response.e2e.challenge[0] ^= 1,
                6 => response.e2e.issued_at_ms += 1,
                7 => response.e2e.expires_at_ms -= 1,
                8 => response.e2e.responder_ephemeral_public_key[5] ^= 1,
                9 => response.e2e.signature[0] ^= 1,
                10 => response.signature = initiator.offer().signature.clone(),
                11 => response.e2e.version += 1,
                12 => response.context.responder_identity_public_key = public(5),
                _ => unreachable!(),
            }
            assert!(
                initiator
                    .complete(&response, NOW, &mut HandshakeReplayCacheV1::new(8))
                    .is_err(),
                "response field {field}"
            );
        }
    }

    #[test]
    fn signed_outer_response_does_not_bypass_existing_inner_signature() {
        let initiator = start();
        let mut response = respond(initiator.offer()).response().clone();
        response.e2e.signature[0] ^= 1;
        response.signature = key(2)
            .sign(&response_signing_digest(&response))
            .to_bytes()
            .to_vec();
        assert!(matches!(
            initiator.complete(&response, NOW, &mut HandshakeReplayCacheV1::new(8)),
            Err(TransportBindingErrorV1::Overlay(
                ProductOverlayErrorV1::InvalidHandshakeSignature
            ))
        ));
    }

    #[test]
    fn legacy_inner_handshake_cannot_be_used_without_bound_wrapper() {
        let initiator = start();
        let legacy = serde_json::to_vec(&initiator.offer().e2e).unwrap();
        assert!(BoundNodeHandshakeOfferV1::decode_json(&legacy).is_err());
        let mut offer = initiator.offer().clone();
        offer.signature = offer.e2e.signature.clone();
        assert!(BoundHandshakeResponderV1::respond(
            &offer,
            &key(2),
            public(1),
            binding(false, 7),
            NOW,
            TTL,
            &mut HandshakeReplayCacheV1::new(8)
        )
        .is_err());
    }

    #[test]
    fn wire_signature_key_and_lifetime_bounds_are_strict() {
        let initiator = start();
        let original = initiator.offer();
        for length in [0, 63, 65, 4096] {
            let mut offer = original.clone();
            offer.signature.resize(length, 0);
            assert!(offer.encode_json().is_err());
            assert!(
                BoundNodeHandshakeOfferV1::decode_json(&serde_json::to_vec(&offer).unwrap())
                    .is_err()
            );
            let mut offer = original.clone();
            offer.e2e.signature.resize(length, 0);
            assert!(offer.encode_json().is_err());
        }
        for length in [0, 32, 34, 65, 1024] {
            let mut offer = original.clone();
            offer.e2e.initiator_ephemeral_public_key.resize(length, 0);
            assert!(offer.encode_json().is_err());
        }
        let mut offer = original.clone();
        offer.e2e.expires_at_ms = NOW + BOUND_HANDSHAKE_MAX_TTL_MS_V1 + 1;
        assert!(offer.encode_json().is_err());
        let mut offer = original.clone();
        offer.e2e.expires_at_ms = NOW;
        assert!(offer.encode_json().is_err());
        let mut offer = original.clone();
        offer.e2e.initiator_peer_id = "x".repeat(9000);
        assert!(offer.encode_json().is_err());
        assert!(BoundNodeHandshakeOfferV1::decode_json(&vec![
            b' ';
            BOUND_HANDSHAKE_MAX_WIRE_BYTES_V1 + 1
        ])
        .is_err());
        assert!(BoundNodeHandshakeOfferV1::decode_json(b"").is_err());
        let mut trailing = original.encode_json().unwrap();
        trailing.extend_from_slice(b"{}");
        assert!(BoundNodeHandshakeOfferV1::decode_json(&trailing).is_err());
        let mut unknown = serde_json::to_value(original).unwrap();
        unknown
            .as_object_mut()
            .unwrap()
            .insert("privacy_qualified".into(), serde_json::Value::Bool(true));
        assert!(
            BoundNodeHandshakeOfferV1::decode_json(&serde_json::to_vec(&unknown).unwrap()).is_err()
        );
    }

    #[test]
    fn exact_expiry_future_time_and_overflow_fail_closed() {
        for ttl in [0, BOUND_HANDSHAKE_MAX_TTL_MS_V1 + 1] {
            assert!(BoundHandshakeInitiatorV1::start(
                &key(1),
                public(2),
                binding(true, 7),
                NOW,
                ttl
            )
            .is_err());
        }
        assert!(BoundHandshakeInitiatorV1::start(
            &key(1),
            public(2),
            binding(true, 7),
            u64::MAX,
            TTL
        )
        .is_err());
        let initiator = start();
        for now in [NOW + TTL, NOW - PRODUCT_OVERLAY_CLOCK_SKEW_MS_V1 - 1] {
            assert!(BoundHandshakeResponderV1::respond(
                initiator.offer(),
                &key(2),
                public(1),
                binding(false, 7),
                now,
                TTL,
                &mut HandshakeReplayCacheV1::new(8)
            )
            .is_err());
        }
        let responder = respond(initiator.offer());
        assert!(initiator
            .complete(
                responder.response(),
                NOW + TTL,
                &mut HandshakeReplayCacheV1::new(8)
            )
            .is_err());
        let (mut left, mut right) = pending_pair();
        let a = left.make_finished(NOW).unwrap();
        let b = right.make_finished(NOW).unwrap();
        left.verify_finished(&b, NOW).unwrap();
        right.verify_finished(&a, NOW).unwrap();
        assert!(left.into_channel(NOW + TTL).is_err());
        assert!(right.into_channel(NOW + TTL).is_err());
    }

    #[test]
    fn incomplete_finished_cannot_expose_channel() {
        let (left, right) = pending_pair();
        assert!(left.into_channel(NOW).is_err());
        assert!(right.into_channel(NOW).is_err());
        let (mut left, mut right) = pending_pair();
        let a = left.make_finished(NOW).unwrap();
        right.verify_finished(&a, NOW).unwrap();
        assert!(left.into_channel(NOW).is_err());
        assert!(right.into_channel(NOW).is_err());
    }

    #[test]
    fn reflected_cross_session_wrong_digest_and_repeated_finished_are_rejected() {
        let (mut left, mut right) = pending_pair();
        let own = left.make_finished(NOW).unwrap();
        assert!(left.verify_finished(&own, NOW).is_err());
        let valid = right.make_finished(NOW).unwrap();
        assert!(left.verify_finished(&valid, NOW).is_err()); // failure is terminal
        assert!(left.into_channel(NOW).is_err());

        let (mut left, _) = pending_pair();
        let (_, mut other) = pending_pair();
        assert!(left
            .verify_finished(&other.make_finished(NOW).unwrap(), NOW)
            .is_err());

        let (mut left, mut right) = pending_pair();
        right.transcript[0] ^= 1; // valid E2E envelope, wrong bound transcript
        assert!(matches!(
            left.verify_finished(&right.make_finished(NOW).unwrap(), NOW),
            Err(TransportBindingErrorV1::Finished)
        ));

        let (mut left, mut right) = pending_pair();
        let a = left.make_finished(NOW).unwrap();
        let b = right.make_finished(NOW).unwrap();
        left.verify_finished(&b, NOW).unwrap();
        right.verify_finished(&a, NOW).unwrap();
        assert!(left.verify_finished(&b, NOW).is_err());
        assert!(left.into_channel(NOW).is_err());
        assert!(right.make_finished(NOW).is_err());
    }

    #[test]
    fn first_encrypted_business_frame_cannot_replace_finished() {
        let (mut left, mut right) = pending_pair();
        let business = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            right.channel.session_id(),
            1,
            2,
            0,
            0,
            b"early business".to_vec(),
        );
        let envelope = right.channel.seal_novorudp_frame(&business).unwrap();
        assert!(matches!(
            left.verify_finished(&envelope, NOW),
            Err(TransportBindingErrorV1::Finished)
        ));
        assert!(left.into_channel(NOW).is_err());
    }

    #[test]
    fn finished_and_business_resource_bounds_and_session_checks() {
        let (mut left, mut right) = pending_pair();
        let mut envelope = right.make_finished(NOW).unwrap();
        envelope.ciphertext.resize(513, 0);
        assert!(left.verify_finished(&envelope, NOW).is_err());
        let (mut left, mut right) = finished_pair();
        let mut frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            left.session_id(),
            1,
            2,
            0,
            0,
            vec![0; BOUND_CHANNEL_MAX_PAYLOAD_BYTES_V1],
        );
        let envelope = left.seal_novorudp_frame(&frame).unwrap();
        assert_eq!(
            right.open_novorudp_frame(&envelope).unwrap().payload.len(),
            BOUND_CHANNEL_MAX_PAYLOAD_BYTES_V1
        );
        frame.payload.push(0);
        assert!(left.seal_novorudp_frame(&frame).is_err());
        frame.payload.clear();
        frame.session_id[0] ^= 1;
        assert!(left.seal_novorudp_frame(&frame).is_err());
        // Inner channel permits arbitrary frame session IDs; wrapper checks it
        // on receive too, so an authenticated malicious peer cannot bypass it.
        let envelope = left.channel.seal_novorudp_frame(&frame).unwrap();
        assert!(right.open_novorudp_frame(&envelope).is_err());
        frame.session_id = left.session_id();
        frame.stream_id = FINISHED_STREAM;
        assert!(left.seal_novorudp_frame(&frame).is_err());
    }
}
