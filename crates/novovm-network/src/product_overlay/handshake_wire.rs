//! Fixed-size native carrier for the EXISTING signed handshake V1 structs.
//! No new cryptography or signing preimage. The carrier version is explicit;
//! raw JSON and variable-sized keys/signatures are not accepted on UDP.

use super::{NodeHandshakeOfferV1, NodeHandshakeResponseV1};
use anyhow::{ensure, Context, Result};

const MAGIC: &[u8; 8] = b"NVHSHK01";
const PEER_BYTES: usize = 79;
const OFFER_BYTES: usize = 8 + 1 + 2 + 16 + 2 * PEER_BYTES + 32 + 33 + 32 + 8 + 8 + 64;
const RESPONSE_BYTES: usize = OFFER_BYTES + 32 + 32;

#[derive(Debug, PartialEq, Eq)]
pub enum HandshakeMessage {
    Offer(NodeHandshakeOfferV1),
    Response(NodeHandshakeResponseV1),
}

fn peer(bytes: &[u8]) -> Result<&str> {
    ensure!(
        bytes.len() == PEER_BYTES
            && bytes.starts_with(b"novovm-ed25519:")
            && bytes[15..]
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
    peer(initiator.as_bytes())?;
    peer(responder.as_bytes())?;
    let mut out = Vec::with_capacity(RESPONSE_BYTES);
    out.extend_from_slice(MAGIC);
    out.push(tag);
    out.extend_from_slice(&version.to_be_bytes());
    out.extend_from_slice(session);
    out.extend_from_slice(initiator.as_bytes());
    out.extend_from_slice(responder.as_bytes());
    Ok(out)
}

pub fn encode_offer(value: &NodeHandshakeOfferV1) -> Result<Vec<u8>> {
    ensure!(
        value.initiator_ephemeral_public_key.len() == 33 && value.signature.len() == 64,
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
        value.responder_ephemeral_public_key.len() == 33 && value.signature.len() == 64,
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
    peer(&wire[27..106])?;
    peer(&wire[106..185])?;
    let mut input = Cursor(&wire[9..]);
    let version = u16::from_be_bytes(input.array()?);
    let session_id = input.array()?;
    let initiator_peer_id = peer(&input.array::<PEER_BYTES>()?)?.to_owned();
    let responder_peer_id = peer(&input.array::<PEER_BYTES>()?)?.to_owned();
    let identity = input.array()?;
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
            signature: input.array::<64>()?.to_vec(),
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
            signature: input.array::<64>()?.to_vec(),
        })
    };
    ensure!(input.0.is_empty(), "trailing native handshake bytes");
    Ok(message)
}

#[cfg(test)]
mod tests;
