//! Fixed-size ML-DSA-65 carrier for the original signed handshake owner.
//! The explicit version and suite are checked before any owned field is built;
//! raw JSON and legacy key/signature encodings are not accepted on UDP.

use super::{
    NodeHandshakeOfferV1, NodeHandshakeResponseV1, MLDSA65_PUBLIC_KEY_BYTES,
    MLDSA65_SIGNATURE_BYTES, PRODUCT_OVERLAY_PROTOCOL_VERSION_V1,
};
use anyhow::{ensure, Context, Result};

const MAGIC: &[u8; 8] = b"NVHSHK02";
const SUITE: u16 = 65;
const PEER_PREFIX: &[u8] = b"novovm-mldsa65:";
const PEER_BYTES: usize = PEER_PREFIX.len() + 64;
const PEERS_OFFSET: usize = 8 + 1 + 2 + 2 + 16;
pub(crate) const OFFER_BYTES: usize = PEERS_OFFSET
    + 2 * PEER_BYTES
    + MLDSA65_PUBLIC_KEY_BYTES
    + 33
    + 32
    + 8
    + 8
    + MLDSA65_SIGNATURE_BYTES;
pub(crate) const RESPONSE_BYTES: usize = OFFER_BYTES + 32 + 32;

#[derive(Debug, PartialEq, Eq)]
pub enum HandshakeMessage {
    Offer(NodeHandshakeOfferV1),
    Response(NodeHandshakeResponseV1),
}

fn peer(bytes: &[u8]) -> Result<&str> {
    ensure!(
        bytes.len() == PEER_BYTES
            && bytes.starts_with(PEER_PREFIX)
            && bytes[PEER_PREFIX.len()..]
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b)),
        "noncanonical native peer id"
    );
    Ok(std::str::from_utf8(bytes)?)
}

fn common(
    tag: u8,
    version: u16,
    session: &[u8; 16],
    initiator: &str,
    responder: &str,
) -> Result<Vec<u8>> {
    ensure!(
        version == PRODUCT_OVERLAY_PROTOCOL_VERSION_V1,
        "unsupported native handshake version"
    );
    peer(initiator.as_bytes())?;
    peer(responder.as_bytes())?;
    let mut out = Vec::with_capacity(RESPONSE_BYTES);
    out.extend_from_slice(MAGIC);
    out.push(tag);
    out.extend_from_slice(&version.to_be_bytes());
    out.extend_from_slice(&SUITE.to_be_bytes());
    out.extend_from_slice(session);
    out.extend_from_slice(initiator.as_bytes());
    out.extend_from_slice(responder.as_bytes());
    Ok(out)
}

pub fn encode_offer(value: &NodeHandshakeOfferV1) -> Result<Vec<u8>> {
    ensure!(
        value.initiator_identity_public_key.len() == MLDSA65_PUBLIC_KEY_BYTES
            && value.initiator_ephemeral_public_key.len() == 33
            && value.signature.len() == MLDSA65_SIGNATURE_BYTES,
        "invalid handshake field size"
    );
    let mut out = common(
        1,
        value.version,
        &value.session_id,
        &value.initiator_peer_id,
        &value.responder_peer_id,
    )?;
    out.extend_from_slice(&value.initiator_identity_public_key);
    out.extend_from_slice(&value.initiator_ephemeral_public_key);
    out.extend_from_slice(&value.challenge);
    out.extend_from_slice(&value.issued_at_ms.to_be_bytes());
    out.extend_from_slice(&value.expires_at_ms.to_be_bytes());
    out.extend_from_slice(&value.signature);
    debug_assert_eq!(out.len(), OFFER_BYTES);
    Ok(out)
}

pub fn encode_response(value: &NodeHandshakeResponseV1) -> Result<Vec<u8>> {
    ensure!(
        value.responder_identity_public_key.len() == MLDSA65_PUBLIC_KEY_BYTES
            && value.responder_ephemeral_public_key.len() == 33
            && value.signature.len() == MLDSA65_SIGNATURE_BYTES,
        "invalid handshake field size"
    );
    let mut out = common(
        2,
        value.version,
        &value.session_id,
        &value.initiator_peer_id,
        &value.responder_peer_id,
    )?;
    out.extend_from_slice(&value.responder_identity_public_key);
    out.extend_from_slice(&value.responder_ephemeral_public_key);
    out.extend_from_slice(&value.challenge);
    out.extend_from_slice(&value.response_nonce);
    out.extend_from_slice(&value.offer_hash);
    out.extend_from_slice(&value.issued_at_ms.to_be_bytes());
    out.extend_from_slice(&value.expires_at_ms.to_be_bytes());
    out.extend_from_slice(&value.signature);
    debug_assert_eq!(out.len(), RESPONSE_BYTES);
    Ok(out)
}

struct Cursor<'a>(&'a [u8]);
impl Cursor<'_> {
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let value = self.0.get(..N).context("truncated handshake")?.try_into()?;
        self.0 = &self.0[N..];
        Ok(value)
    }
}

pub fn decode(wire: &[u8]) -> Result<HandshakeMessage> {
    // No allocations before the entire fixed layout and both IDs are checked.
    ensure!(wire.starts_with(MAGIC), "native handshake carrier mismatch");
    let tag = *wire.get(8).context("missing native handshake tag")?;
    ensure!(
        (tag == 1 && wire.len() == OFFER_BYTES) || (tag == 2 && wire.len() == RESPONSE_BYTES),
        "native handshake size/tag mismatch"
    );
    let version = u16::from_be_bytes(wire[9..11].try_into()?);
    ensure!(
        version == PRODUCT_OVERLAY_PROTOCOL_VERSION_V1,
        "unsupported native handshake version"
    );
    ensure!(
        u16::from_be_bytes(wire[11..13].try_into()?) == SUITE,
        "unsupported native handshake signature suite"
    );
    peer(&wire[PEERS_OFFSET..PEERS_OFFSET + PEER_BYTES])?;
    peer(&wire[PEERS_OFFSET + PEER_BYTES..PEERS_OFFSET + 2 * PEER_BYTES])?;
    let mut input = Cursor(&wire[9..]);
    let version = u16::from_be_bytes(input.array()?);
    let _suite = input.array::<2>()?;
    let session_id = input.array()?;
    let initiator_peer_id = peer(&input.array::<PEER_BYTES>()?)?.to_owned();
    let responder_peer_id = peer(&input.array::<PEER_BYTES>()?)?.to_owned();
    let identity = input.array::<MLDSA65_PUBLIC_KEY_BYTES>()?.to_vec();
    let ephemeral = input.array::<33>()?.to_vec();
    let challenge = input.array()?;
    let message = if tag == 1 {
        HandshakeMessage::Offer(NodeHandshakeOfferV1 {
            version,
            session_id,
            initiator_peer_id,
            responder_peer_id,
            initiator_identity_public_key: identity,
            initiator_ephemeral_public_key: ephemeral,
            challenge,
            issued_at_ms: u64::from_be_bytes(input.array()?),
            expires_at_ms: u64::from_be_bytes(input.array()?),
            signature: input.array::<MLDSA65_SIGNATURE_BYTES>()?.to_vec(),
        })
    } else {
        HandshakeMessage::Response(NodeHandshakeResponseV1 {
            version,
            session_id,
            initiator_peer_id,
            responder_peer_id,
            responder_identity_public_key: identity,
            responder_ephemeral_public_key: ephemeral,
            challenge,
            response_nonce: input.array()?,
            offer_hash: input.array()?,
            issued_at_ms: u64::from_be_bytes(input.array()?),
            expires_at_ms: u64::from_be_bytes(input.array()?),
            signature: input.array::<MLDSA65_SIGNATURE_BYTES>()?.to_vec(),
        })
    };
    ensure!(input.0.is_empty(), "trailing native handshake bytes");
    Ok(message)
}

#[cfg(test)]
mod tests;
