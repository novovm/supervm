use crate::novorudp::{NovoRudpTransportFrameDecodeErrorV0, NovoRudpTransportFrameV0};
use chacha20poly1305::{
    aead::{Aead, Payload},
    ChaCha20Poly1305, KeyInit, Nonce,
};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use k256::{ecdh::diffie_hellman, elliptic_curve::sec1::ToEncodedPoint, PublicKey, SecretKey};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashSet, VecDeque};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const PRODUCT_OVERLAY_PROTOCOL_VERSION_V1: u16 = 1;
pub const PRODUCT_OVERLAY_HANDSHAKE_DOMAIN_V1: &[u8] = b"novovm-product-overlay-handshake-v1";
pub const PRODUCT_OVERLAY_SECURE_FRAME_DOMAIN_V1: &[u8] = b"novovm-product-overlay-secure-frame-v1";

#[derive(Debug, Error)]
pub enum ProductOverlayErrorV1 {
    #[error("unsupported product overlay version: {0}")]
    UnsupportedVersion(u16),
    #[error("handshake message is outside its validity window")]
    HandshakeExpired,
    #[error("handshake identity does not match the declared peer id")]
    IdentityMismatch,
    #[error("handshake target does not match the expected peer")]
    HandshakeTargetMismatch,
    #[error("handshake transcript does not match the offer")]
    HandshakeTranscriptMismatch,
    #[error("handshake signature is invalid")]
    InvalidHandshakeSignature,
    #[error("handshake key material is invalid")]
    InvalidHandshakeKey,
    #[error("handshake message was replayed")]
    HandshakeReplay,
    #[error("secure frame route does not match the authenticated channel")]
    SecureFrameRouteMismatch,
    #[error("secure frame nonce does not match its sequence")]
    SecureFrameNonceMismatch,
    #[error("secure frame sequence was replayed or is outside the replay window")]
    SecureFrameReplay,
    #[error("secure frame authentication failed")]
    SecureFrameAuthenticationFailed,
    #[error("secure frame sequence exhausted")]
    SecureFrameSequenceExhausted,
    #[error("key derivation failed")]
    KeyDerivationFailed,
    #[error("NOVORUDP frame decode failed: {0}")]
    NovoRudpDecode(#[from] NovoRudpTransportFrameDecodeErrorV0),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeHandshakeOfferV1 {
    pub version: u16,
    pub session_id: [u8; 16],
    pub initiator_peer_id: String,
    pub responder_peer_id: String,
    pub initiator_identity_public_key: [u8; 32],
    pub initiator_ephemeral_public_key: Vec<u8>,
    pub challenge: [u8; 32],
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeHandshakeResponseV1 {
    pub version: u16,
    pub session_id: [u8; 16],
    pub initiator_peer_id: String,
    pub responder_peer_id: String,
    pub responder_identity_public_key: [u8; 32],
    pub responder_ephemeral_public_key: Vec<u8>,
    pub challenge: [u8; 32],
    pub response_nonce: [u8; 32],
    pub offer_hash: [u8; 32],
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub signature: Vec<u8>,
}

pub struct NodeHandshakeInitiatorV1 {
    offer: NodeHandshakeOfferV1,
    ephemeral_secret: SecretKey,
}

pub struct NodeHandshakeResponderV1 {
    response: NodeHandshakeResponseV1,
    channel: E2eSecureChannelV1,
    authenticated_remote: AuthenticatedPeerV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedPeerV1 {
    peer_id: String,
    identity_public_key: [u8; 32],
    session_id: [u8; 16],
    authenticated_at_ms: u64,
}

#[derive(Debug)]
pub struct HandshakeReplayCacheV1 {
    capacity: usize,
    order: VecDeque<[u8; 32]>,
    seen: HashSet<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecureNovoRudpEnvelopeV1 {
    pub version: u16,
    pub session_id: [u8; 16],
    pub sender_peer_id: String,
    pub recipient_peer_id: String,
    pub sequence: u64,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

#[derive(Zeroize, ZeroizeOnDrop)]
struct E2eSessionSecretsV1 {
    initiator_to_responder_key: [u8; 32],
    responder_to_initiator_key: [u8; 32],
    initiator_nonce_prefix: [u8; 4],
    responder_nonce_prefix: [u8; 4],
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct E2eSecureChannelV1 {
    session_id: [u8; 16],
    local_peer_id: String,
    remote_peer_id: String,
    outbound_key: [u8; 32],
    inbound_key: [u8; 32],
    outbound_nonce_prefix: [u8; 4],
    inbound_nonce_prefix: [u8; 4],
    outbound_sequence: u64,
    #[zeroize(skip)]
    inbound_replay: SlidingReplayWindowV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlidingReplayWindowV1 {
    width: u32,
    highest_sequence: Option<u64>,
    bitmap: u128,
}

impl HandshakeReplayCacheV1 {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            order: VecDeque::new(),
            seen: HashSet::new(),
        }
    }

    fn insert_once(&mut self, fingerprint: [u8; 32]) -> bool {
        if self.seen.contains(&fingerprint) {
            return false;
        }
        self.seen.insert(fingerprint);
        self.order.push_back(fingerprint);
        while self.order.len() > self.capacity {
            if let Some(expired) = self.order.pop_front() {
                self.seen.remove(&expired);
            }
        }
        true
    }
}

impl Default for HandshakeReplayCacheV1 {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl SlidingReplayWindowV1 {
    #[must_use]
    pub fn new(width: u32) -> Self {
        Self {
            width: width.clamp(1, 128),
            highest_sequence: None,
            bitmap: 0,
        }
    }

    #[must_use]
    pub fn would_accept(&self, sequence: u64) -> bool {
        let Some(highest) = self.highest_sequence else {
            return true;
        };
        if sequence > highest {
            return true;
        }
        let distance = highest - sequence;
        distance < u64::from(self.width) && (self.bitmap & (1u128 << distance)) == 0
    }

    fn mark_authenticated(&mut self, sequence: u64) -> bool {
        if !self.would_accept(sequence) {
            return false;
        }
        match self.highest_sequence {
            None => {
                self.highest_sequence = Some(sequence);
                self.bitmap = 1;
            }
            Some(highest) if sequence > highest => {
                let advance = sequence - highest;
                self.bitmap = if advance >= u64::from(self.width) {
                    1
                } else {
                    (self.bitmap << advance) | 1
                };
                self.highest_sequence = Some(sequence);
            }
            Some(highest) => {
                self.bitmap |= 1u128 << (highest - sequence);
            }
        }
        true
    }
}

impl NodeHandshakeInitiatorV1 {
    pub fn start(
        identity: &SigningKey,
        responder_peer_id: impl Into<String>,
        now_ms: u64,
        ttl_ms: u64,
    ) -> Result<Self, ProductOverlayErrorV1> {
        let responder_peer_id = responder_peer_id.into();
        let ephemeral_secret = SecretKey::random(&mut OsRng);
        let mut session_id = [0u8; 16];
        let mut challenge = [0u8; 32];
        OsRng.fill_bytes(&mut session_id);
        OsRng.fill_bytes(&mut challenge);
        Self::start_with_material(
            identity,
            responder_peer_id,
            now_ms,
            ttl_ms,
            session_id,
            challenge,
            ephemeral_secret,
        )
    }

    fn start_with_material(
        identity: &SigningKey,
        responder_peer_id: String,
        now_ms: u64,
        ttl_ms: u64,
        session_id: [u8; 16],
        challenge: [u8; 32],
        ephemeral_secret: SecretKey,
    ) -> Result<Self, ProductOverlayErrorV1> {
        let identity_public_key = identity.verifying_key().to_bytes();
        let initiator_peer_id = peer_id_from_ed25519_public_key_v1(&identity_public_key);
        let ephemeral_public_key = ephemeral_secret
            .public_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec();
        let expires_at_ms = now_ms
            .checked_add(ttl_ms.max(1))
            .ok_or(ProductOverlayErrorV1::HandshakeExpired)?;
        let mut offer = NodeHandshakeOfferV1 {
            version: PRODUCT_OVERLAY_PROTOCOL_VERSION_V1,
            session_id,
            initiator_peer_id,
            responder_peer_id,
            initiator_identity_public_key: identity_public_key,
            initiator_ephemeral_public_key: ephemeral_public_key,
            challenge,
            issued_at_ms: now_ms,
            expires_at_ms,
            signature: Vec::new(),
        };
        offer.signature = identity
            .sign(&handshake_offer_signing_bytes_v1(&offer))
            .to_bytes()
            .to_vec();
        Ok(Self {
            offer,
            ephemeral_secret,
        })
    }

    #[must_use]
    pub fn offer(&self) -> &NodeHandshakeOfferV1 {
        &self.offer
    }

    pub fn complete(
        self,
        response: &NodeHandshakeResponseV1,
        now_ms: u64,
        replay_cache: &mut HandshakeReplayCacheV1,
    ) -> Result<E2eSecureChannelV1, ProductOverlayErrorV1> {
        validate_handshake_response_v1(&self.offer, response, now_ms, replay_cache)?;
        let remote_public = PublicKey::from_sec1_bytes(&response.responder_ephemeral_public_key)
            .map_err(|_| ProductOverlayErrorV1::InvalidHandshakeKey)?;
        let secrets = derive_session_secrets_v1(
            &self.ephemeral_secret,
            &remote_public,
            &self.offer,
            response,
        )?;
        Ok(E2eSecureChannelV1::for_initiator(&self.offer, secrets))
    }
}

impl NodeHandshakeResponderV1 {
    pub fn respond(
        offer: &NodeHandshakeOfferV1,
        identity: &SigningKey,
        now_ms: u64,
        ttl_ms: u64,
        replay_cache: &mut HandshakeReplayCacheV1,
    ) -> Result<Self, ProductOverlayErrorV1> {
        validate_handshake_offer_v1(offer, now_ms, replay_cache)?;
        let responder_identity_public_key = identity.verifying_key().to_bytes();
        let responder_peer_id = peer_id_from_ed25519_public_key_v1(&responder_identity_public_key);
        if responder_peer_id != offer.responder_peer_id {
            return Err(ProductOverlayErrorV1::HandshakeTargetMismatch);
        }
        let initiator_public = PublicKey::from_sec1_bytes(&offer.initiator_ephemeral_public_key)
            .map_err(|_| ProductOverlayErrorV1::InvalidHandshakeKey)?;
        let ephemeral_secret = SecretKey::random(&mut OsRng);
        let ephemeral_public_key = ephemeral_secret
            .public_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec();
        let mut response_nonce = [0u8; 32];
        OsRng.fill_bytes(&mut response_nonce);
        let expires_at_ms = now_ms
            .checked_add(ttl_ms.max(1))
            .ok_or(ProductOverlayErrorV1::HandshakeExpired)?;
        let mut response = NodeHandshakeResponseV1 {
            version: PRODUCT_OVERLAY_PROTOCOL_VERSION_V1,
            session_id: offer.session_id,
            initiator_peer_id: offer.initiator_peer_id.clone(),
            responder_peer_id,
            responder_identity_public_key,
            responder_ephemeral_public_key: ephemeral_public_key,
            challenge: offer.challenge,
            response_nonce,
            offer_hash: handshake_offer_hash_v1(offer),
            issued_at_ms: now_ms,
            expires_at_ms,
            signature: Vec::new(),
        };
        response.signature = identity
            .sign(&handshake_response_signing_bytes_v1(&response))
            .to_bytes()
            .to_vec();
        let secrets =
            derive_session_secrets_v1(&ephemeral_secret, &initiator_public, offer, &response)?;
        let channel = E2eSecureChannelV1::for_responder(offer, secrets);
        let authenticated_remote = AuthenticatedPeerV1 {
            peer_id: offer.initiator_peer_id.clone(),
            identity_public_key: offer.initiator_identity_public_key,
            session_id: offer.session_id,
            authenticated_at_ms: now_ms,
        };
        Ok(Self {
            response,
            channel,
            authenticated_remote,
        })
    }

    #[must_use]
    pub fn response(&self) -> &NodeHandshakeResponseV1 {
        &self.response
    }

    #[must_use]
    pub fn into_channel(self) -> E2eSecureChannelV1 {
        self.channel
    }

    #[must_use]
    pub fn authenticated_remote(&self) -> &AuthenticatedPeerV1 {
        &self.authenticated_remote
    }

    #[must_use]
    pub fn into_parts(self) -> (AuthenticatedPeerV1, E2eSecureChannelV1) {
        (self.authenticated_remote, self.channel)
    }
}

impl AuthenticatedPeerV1 {
    #[must_use]
    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    #[must_use]
    pub fn identity_public_key(&self) -> [u8; 32] {
        self.identity_public_key
    }

    #[must_use]
    pub fn session_id(&self) -> [u8; 16] {
        self.session_id
    }

    #[must_use]
    pub fn authenticated_at_ms(&self) -> u64 {
        self.authenticated_at_ms
    }
}

impl E2eSecureChannelV1 {
    fn for_initiator(offer: &NodeHandshakeOfferV1, mut secrets: E2eSessionSecretsV1) -> Self {
        Self {
            session_id: offer.session_id,
            local_peer_id: offer.initiator_peer_id.clone(),
            remote_peer_id: offer.responder_peer_id.clone(),
            outbound_key: std::mem::take(&mut secrets.initiator_to_responder_key),
            inbound_key: std::mem::take(&mut secrets.responder_to_initiator_key),
            outbound_nonce_prefix: secrets.initiator_nonce_prefix,
            inbound_nonce_prefix: secrets.responder_nonce_prefix,
            outbound_sequence: 0,
            inbound_replay: SlidingReplayWindowV1::new(128),
        }
    }

    fn for_responder(offer: &NodeHandshakeOfferV1, mut secrets: E2eSessionSecretsV1) -> Self {
        Self {
            session_id: offer.session_id,
            local_peer_id: offer.responder_peer_id.clone(),
            remote_peer_id: offer.initiator_peer_id.clone(),
            outbound_key: std::mem::take(&mut secrets.responder_to_initiator_key),
            inbound_key: std::mem::take(&mut secrets.initiator_to_responder_key),
            outbound_nonce_prefix: secrets.responder_nonce_prefix,
            inbound_nonce_prefix: secrets.initiator_nonce_prefix,
            outbound_sequence: 0,
            inbound_replay: SlidingReplayWindowV1::new(128),
        }
    }

    #[must_use]
    pub fn local_peer_id(&self) -> &str {
        &self.local_peer_id
    }

    #[must_use]
    pub fn remote_peer_id(&self) -> &str {
        &self.remote_peer_id
    }

    #[must_use]
    pub fn session_id(&self) -> [u8; 16] {
        self.session_id
    }

    pub fn seal_novorudp_frame(
        &mut self,
        frame: &NovoRudpTransportFrameV0,
    ) -> Result<SecureNovoRudpEnvelopeV1, ProductOverlayErrorV1> {
        let sequence = self.outbound_sequence;
        self.outbound_sequence = self
            .outbound_sequence
            .checked_add(1)
            .ok_or(ProductOverlayErrorV1::SecureFrameSequenceExhausted)?;
        let nonce = secure_frame_nonce_v1(self.outbound_nonce_prefix, sequence);
        let mut envelope = SecureNovoRudpEnvelopeV1 {
            version: PRODUCT_OVERLAY_PROTOCOL_VERSION_V1,
            session_id: self.session_id,
            sender_peer_id: self.local_peer_id.clone(),
            recipient_peer_id: self.remote_peer_id.clone(),
            sequence,
            nonce,
            ciphertext: Vec::new(),
        };
        let aad = secure_frame_aad_v1(&envelope);
        let cipher = ChaCha20Poly1305::new((&self.outbound_key).into());
        envelope.ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &frame.encode(),
                    aad: &aad,
                },
            )
            .map_err(|_| ProductOverlayErrorV1::SecureFrameAuthenticationFailed)?;
        Ok(envelope)
    }

    pub fn open_novorudp_frame(
        &mut self,
        envelope: &SecureNovoRudpEnvelopeV1,
    ) -> Result<NovoRudpTransportFrameV0, ProductOverlayErrorV1> {
        if envelope.version != PRODUCT_OVERLAY_PROTOCOL_VERSION_V1 {
            return Err(ProductOverlayErrorV1::UnsupportedVersion(envelope.version));
        }
        if envelope.session_id != self.session_id
            || envelope.sender_peer_id != self.remote_peer_id
            || envelope.recipient_peer_id != self.local_peer_id
        {
            return Err(ProductOverlayErrorV1::SecureFrameRouteMismatch);
        }
        let expected_nonce = secure_frame_nonce_v1(self.inbound_nonce_prefix, envelope.sequence);
        if envelope.nonce != expected_nonce {
            return Err(ProductOverlayErrorV1::SecureFrameNonceMismatch);
        }
        if !self.inbound_replay.would_accept(envelope.sequence) {
            return Err(ProductOverlayErrorV1::SecureFrameReplay);
        }
        let aad = secure_frame_aad_v1(envelope);
        let cipher = ChaCha20Poly1305::new((&self.inbound_key).into());
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&envelope.nonce),
                Payload {
                    msg: &envelope.ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| ProductOverlayErrorV1::SecureFrameAuthenticationFailed)?;
        if !self.inbound_replay.mark_authenticated(envelope.sequence) {
            return Err(ProductOverlayErrorV1::SecureFrameReplay);
        }
        Ok(NovoRudpTransportFrameV0::decode(&plaintext)?)
    }
}

#[must_use]
pub fn peer_id_from_ed25519_public_key_v1(public_key: &[u8; 32]) -> String {
    format!("novovm-ed25519:{}", hex_lower_v1(public_key))
}

pub fn validate_handshake_offer_v1(
    offer: &NodeHandshakeOfferV1,
    now_ms: u64,
    replay_cache: &mut HandshakeReplayCacheV1,
) -> Result<(), ProductOverlayErrorV1> {
    if offer.version != PRODUCT_OVERLAY_PROTOCOL_VERSION_V1 {
        return Err(ProductOverlayErrorV1::UnsupportedVersion(offer.version));
    }
    if offer.issued_at_ms > now_ms || now_ms > offer.expires_at_ms {
        return Err(ProductOverlayErrorV1::HandshakeExpired);
    }
    let expected_peer_id = peer_id_from_ed25519_public_key_v1(&offer.initiator_identity_public_key);
    if offer.initiator_peer_id != expected_peer_id {
        return Err(ProductOverlayErrorV1::IdentityMismatch);
    }
    PublicKey::from_sec1_bytes(&offer.initiator_ephemeral_public_key)
        .map_err(|_| ProductOverlayErrorV1::InvalidHandshakeKey)?;
    verify_ed25519_signature_v1(
        &offer.initiator_identity_public_key,
        &handshake_offer_signing_bytes_v1(offer),
        &offer.signature,
    )?;
    if !replay_cache.insert_once(handshake_offer_hash_v1(offer)) {
        return Err(ProductOverlayErrorV1::HandshakeReplay);
    }
    Ok(())
}

pub fn validate_handshake_response_v1(
    offer: &NodeHandshakeOfferV1,
    response: &NodeHandshakeResponseV1,
    now_ms: u64,
    replay_cache: &mut HandshakeReplayCacheV1,
) -> Result<(), ProductOverlayErrorV1> {
    if response.version != PRODUCT_OVERLAY_PROTOCOL_VERSION_V1 {
        return Err(ProductOverlayErrorV1::UnsupportedVersion(response.version));
    }
    if offer.issued_at_ms > now_ms
        || now_ms > offer.expires_at_ms
        || response.issued_at_ms < offer.issued_at_ms
        || response.issued_at_ms > now_ms
        || now_ms > response.expires_at_ms
    {
        return Err(ProductOverlayErrorV1::HandshakeExpired);
    }
    if response.session_id != offer.session_id
        || response.initiator_peer_id != offer.initiator_peer_id
        || response.responder_peer_id != offer.responder_peer_id
        || response.challenge != offer.challenge
        || response.offer_hash != handshake_offer_hash_v1(offer)
    {
        return Err(ProductOverlayErrorV1::HandshakeTranscriptMismatch);
    }
    let expected_peer_id =
        peer_id_from_ed25519_public_key_v1(&response.responder_identity_public_key);
    if response.responder_peer_id != expected_peer_id {
        return Err(ProductOverlayErrorV1::IdentityMismatch);
    }
    PublicKey::from_sec1_bytes(&response.responder_ephemeral_public_key)
        .map_err(|_| ProductOverlayErrorV1::InvalidHandshakeKey)?;
    verify_ed25519_signature_v1(
        &response.responder_identity_public_key,
        &handshake_response_signing_bytes_v1(response),
        &response.signature,
    )?;
    if !replay_cache.insert_once(handshake_response_hash_v1(response)) {
        return Err(ProductOverlayErrorV1::HandshakeReplay);
    }
    Ok(())
}

fn derive_session_secrets_v1(
    local_secret: &SecretKey,
    remote_public: &PublicKey,
    offer: &NodeHandshakeOfferV1,
    response: &NodeHandshakeResponseV1,
) -> Result<E2eSessionSecretsV1, ProductOverlayErrorV1> {
    let shared = diffie_hellman(local_secret.to_nonzero_scalar(), remote_public.as_affine());
    let transcript_hash = handshake_transcript_hash_v1(offer, response);
    let hkdf = Hkdf::<Sha256>::new(Some(&transcript_hash), shared.raw_secret_bytes().as_slice());
    let mut output = [0u8; 72];
    hkdf.expand(b"novovm-product-overlay-session-keys-v1", &mut output)
        .map_err(|_| ProductOverlayErrorV1::KeyDerivationFailed)?;
    let mut initiator_to_responder_key = [0u8; 32];
    let mut responder_to_initiator_key = [0u8; 32];
    let mut initiator_nonce_prefix = [0u8; 4];
    let mut responder_nonce_prefix = [0u8; 4];
    initiator_to_responder_key.copy_from_slice(&output[0..32]);
    responder_to_initiator_key.copy_from_slice(&output[32..64]);
    initiator_nonce_prefix.copy_from_slice(&output[64..68]);
    responder_nonce_prefix.copy_from_slice(&output[68..72]);
    output.zeroize();
    Ok(E2eSessionSecretsV1 {
        initiator_to_responder_key,
        responder_to_initiator_key,
        initiator_nonce_prefix,
        responder_nonce_prefix,
    })
}

fn verify_ed25519_signature_v1(
    public_key: &[u8; 32],
    message: &[u8],
    signature: &[u8],
) -> Result<(), ProductOverlayErrorV1> {
    let verifying_key = VerifyingKey::from_bytes(public_key)
        .map_err(|_| ProductOverlayErrorV1::InvalidHandshakeKey)?;
    let signature_bytes: [u8; 64] = signature
        .try_into()
        .map_err(|_| ProductOverlayErrorV1::InvalidHandshakeSignature)?;
    verifying_key
        .verify(message, &Signature::from_bytes(&signature_bytes))
        .map_err(|_| ProductOverlayErrorV1::InvalidHandshakeSignature)
}

fn handshake_offer_signing_bytes_v1(offer: &NodeHandshakeOfferV1) -> Vec<u8> {
    let mut out = Vec::new();
    append_field_v1(&mut out, PRODUCT_OVERLAY_HANDSHAKE_DOMAIN_V1);
    out.extend_from_slice(&offer.version.to_be_bytes());
    out.extend_from_slice(&offer.session_id);
    append_field_v1(&mut out, offer.initiator_peer_id.as_bytes());
    append_field_v1(&mut out, offer.responder_peer_id.as_bytes());
    out.extend_from_slice(&offer.initiator_identity_public_key);
    append_field_v1(&mut out, &offer.initiator_ephemeral_public_key);
    out.extend_from_slice(&offer.challenge);
    out.extend_from_slice(&offer.issued_at_ms.to_be_bytes());
    out.extend_from_slice(&offer.expires_at_ms.to_be_bytes());
    out
}

fn handshake_response_signing_bytes_v1(response: &NodeHandshakeResponseV1) -> Vec<u8> {
    let mut out = Vec::new();
    append_field_v1(&mut out, PRODUCT_OVERLAY_HANDSHAKE_DOMAIN_V1);
    out.extend_from_slice(&response.version.to_be_bytes());
    out.extend_from_slice(&response.session_id);
    append_field_v1(&mut out, response.initiator_peer_id.as_bytes());
    append_field_v1(&mut out, response.responder_peer_id.as_bytes());
    out.extend_from_slice(&response.responder_identity_public_key);
    append_field_v1(&mut out, &response.responder_ephemeral_public_key);
    out.extend_from_slice(&response.challenge);
    out.extend_from_slice(&response.response_nonce);
    out.extend_from_slice(&response.offer_hash);
    out.extend_from_slice(&response.issued_at_ms.to_be_bytes());
    out.extend_from_slice(&response.expires_at_ms.to_be_bytes());
    out
}

fn handshake_offer_hash_v1(offer: &NodeHandshakeOfferV1) -> [u8; 32] {
    hash_parts_v1(&[
        b"novovm-product-overlay-offer-hash-v1",
        &handshake_offer_signing_bytes_v1(offer),
        &offer.signature,
    ])
}

fn handshake_response_hash_v1(response: &NodeHandshakeResponseV1) -> [u8; 32] {
    hash_parts_v1(&[
        b"novovm-product-overlay-response-hash-v1",
        &handshake_response_signing_bytes_v1(response),
        &response.signature,
    ])
}

fn handshake_transcript_hash_v1(
    offer: &NodeHandshakeOfferV1,
    response: &NodeHandshakeResponseV1,
) -> [u8; 32] {
    hash_parts_v1(&[
        b"novovm-product-overlay-transcript-v1",
        &handshake_offer_hash_v1(offer),
        &handshake_response_hash_v1(response),
    ])
}

fn secure_frame_nonce_v1(prefix: [u8; 4], sequence: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(&prefix);
    nonce[4..].copy_from_slice(&sequence.to_be_bytes());
    nonce
}

fn secure_frame_aad_v1(envelope: &SecureNovoRudpEnvelopeV1) -> Vec<u8> {
    let mut out = Vec::new();
    append_field_v1(&mut out, PRODUCT_OVERLAY_SECURE_FRAME_DOMAIN_V1);
    out.extend_from_slice(&envelope.version.to_be_bytes());
    out.extend_from_slice(&envelope.session_id);
    append_field_v1(&mut out, envelope.sender_peer_id.as_bytes());
    append_field_v1(&mut out, envelope.recipient_peer_id.as_bytes());
    out.extend_from_slice(&envelope.sequence.to_be_bytes());
    out.extend_from_slice(&envelope.nonce);
    out
}

fn append_field_v1(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn hash_parts_v1(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        append_field_v1_to_hasher(&mut hasher, part);
    }
    hasher.finalize().into()
}

fn append_field_v1_to_hasher(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn hex_lower_v1(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[(byte >> 4) as usize]));
        out.push(char::from(HEX[(byte & 0x0f) as usize]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::novorudp::NovoRudpTransportFrameKindV0;

    fn identities() -> (SigningKey, SigningKey) {
        (
            SigningKey::from_bytes(&[31u8; 32]),
            SigningKey::from_bytes(&[47u8; 32]),
        )
    }

    fn channels() -> (E2eSecureChannelV1, E2eSecureChannelV1) {
        let (initiator_identity, responder_identity) = identities();
        let responder_peer_id =
            peer_id_from_ed25519_public_key_v1(&responder_identity.verifying_key().to_bytes());
        let initiator =
            NodeHandshakeInitiatorV1::start(&initiator_identity, responder_peer_id, 1_000, 5_000)
                .expect("start handshake");
        let mut responder_replay = HandshakeReplayCacheV1::default();
        let responder = NodeHandshakeResponderV1::respond(
            initiator.offer(),
            &responder_identity,
            1_100,
            5_000,
            &mut responder_replay,
        )
        .expect("respond handshake");
        let response = responder.response().clone();
        let responder_channel = responder.into_channel();
        let mut initiator_replay = HandshakeReplayCacheV1::default();
        let initiator_channel = initiator
            .complete(&response, 1_200, &mut initiator_replay)
            .expect("complete handshake");
        (initiator_channel, responder_channel)
    }

    fn frame(sequence: u64) -> NovoRudpTransportFrameV0 {
        NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            [0x55; 16],
            7,
            8,
            sequence,
            9,
            format!("sealed-payload-{sequence}").into_bytes(),
        )
    }

    #[test]
    fn challenge_response_derives_matching_secure_channels() {
        let (mut initiator, mut responder) = channels();
        assert_eq!(initiator.remote_peer_id(), responder.local_peer_id());
        assert_eq!(responder.remote_peer_id(), initiator.local_peer_id());
        assert_eq!(initiator.session_id(), responder.session_id());

        let envelope = initiator
            .seal_novorudp_frame(&frame(3))
            .expect("seal frame");
        let opened = responder
            .open_novorudp_frame(&envelope)
            .expect("open frame");
        assert_eq!(opened, frame(3));

        let reply = responder
            .seal_novorudp_frame(&frame(4))
            .expect("seal reply");
        assert_eq!(
            initiator.open_novorudp_frame(&reply).expect("open reply"),
            frame(4)
        );
    }

    #[test]
    fn handshake_rejects_identity_and_signature_tamper() {
        let (initiator_identity, responder_identity) = identities();
        let responder_peer_id =
            peer_id_from_ed25519_public_key_v1(&responder_identity.verifying_key().to_bytes());
        let initiator =
            NodeHandshakeInitiatorV1::start(&initiator_identity, responder_peer_id, 1_000, 5_000)
                .expect("start handshake");

        let mut identity_tamper = initiator.offer().clone();
        identity_tamper.initiator_peer_id = "novovm-ed25519:attacker".into();
        let mut cache = HandshakeReplayCacheV1::default();
        assert!(matches!(
            validate_handshake_offer_v1(&identity_tamper, 1_100, &mut cache),
            Err(ProductOverlayErrorV1::IdentityMismatch)
        ));

        let mut signature_tamper = initiator.offer().clone();
        signature_tamper.challenge[0] ^= 0x80;
        let mut cache = HandshakeReplayCacheV1::default();
        assert!(matches!(
            validate_handshake_offer_v1(&signature_tamper, 1_100, &mut cache),
            Err(ProductOverlayErrorV1::InvalidHandshakeSignature)
        ));
    }

    #[test]
    fn handshake_replay_and_expiry_are_rejected() {
        let (initiator_identity, responder_identity) = identities();
        let responder_peer_id =
            peer_id_from_ed25519_public_key_v1(&responder_identity.verifying_key().to_bytes());
        let initiator =
            NodeHandshakeInitiatorV1::start(&initiator_identity, responder_peer_id, 1_000, 100)
                .expect("start handshake");
        let mut replay = HandshakeReplayCacheV1::default();
        validate_handshake_offer_v1(initiator.offer(), 1_050, &mut replay)
            .expect("first offer accepted");
        assert!(matches!(
            validate_handshake_offer_v1(initiator.offer(), 1_050, &mut replay),
            Err(ProductOverlayErrorV1::HandshakeReplay)
        ));

        let mut fresh_cache = HandshakeReplayCacheV1::default();
        assert!(matches!(
            validate_handshake_offer_v1(initiator.offer(), 1_101, &mut fresh_cache),
            Err(ProductOverlayErrorV1::HandshakeExpired)
        ));
    }

    #[test]
    fn secure_frame_rejects_ciphertext_route_nonce_and_replay_tamper() {
        let (mut initiator, mut responder) = channels();
        let envelope = initiator
            .seal_novorudp_frame(&frame(5))
            .expect("seal frame");

        let mut ciphertext_tamper = envelope.clone();
        ciphertext_tamper.ciphertext[0] ^= 0x01;
        assert!(matches!(
            responder.open_novorudp_frame(&ciphertext_tamper),
            Err(ProductOverlayErrorV1::SecureFrameAuthenticationFailed)
        ));

        let mut route_tamper = envelope.clone();
        route_tamper.recipient_peer_id = "novovm-ed25519:wrong".into();
        assert!(matches!(
            responder.open_novorudp_frame(&route_tamper),
            Err(ProductOverlayErrorV1::SecureFrameRouteMismatch)
        ));

        let mut nonce_tamper = envelope.clone();
        nonce_tamper.nonce[0] ^= 0x01;
        assert!(matches!(
            responder.open_novorudp_frame(&nonce_tamper),
            Err(ProductOverlayErrorV1::SecureFrameNonceMismatch)
        ));

        responder
            .open_novorudp_frame(&envelope)
            .expect("first authenticated delivery");
        assert!(matches!(
            responder.open_novorudp_frame(&envelope),
            Err(ProductOverlayErrorV1::SecureFrameReplay)
        ));
    }

    #[test]
    fn replay_window_accepts_reordering_once_within_window() {
        let mut window = SlidingReplayWindowV1::new(8);
        assert!(window.mark_authenticated(10));
        assert!(window.mark_authenticated(12));
        assert!(window.mark_authenticated(11));
        assert!(!window.mark_authenticated(11));
        assert!(window.mark_authenticated(20));
        assert!(!window.mark_authenticated(10));
    }
}
