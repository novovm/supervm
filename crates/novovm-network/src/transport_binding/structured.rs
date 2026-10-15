//! Explicit structured representation of the existing bound handshake.
//! Decoding checks shape, not authority: callers must still run respond/complete
//! and the bidirectional Finished exchange on the actual authenticated carrier.
//! No JSON fallback or changes to the existing signed transcript are permitted.
use super::*;
use anyhow::{ensure, Result};
use novovm_protocol::{
    apfl_batch::{self, BatchView, Limits},
    apfl_column::Column,
};

pub const OFFER_SCHEMA: u64 = u64::from_be_bytes(*b"NVBOFR01");
pub const RESPONSE_SCHEMA: u64 = u64::from_be_bytes(*b"NVBRES01");
const OFFER_WIDTHS: [usize; 14] = [2, 32, 32, 32, 32, 32, 2, 16, 33, 32, 8, 8, 64, 64];
const RESPONSE_WIDTHS: [usize; 17] = [
    2, 32, 32, 32, 32, 32, 2, 16, 33, 32, 8, 8, 64, 64, 32, 32, 32,
];
const LIMITS: Limits = Limits {
    rows: 1,
    columns: 17,
    field_bytes: 64,
    wire_bytes: BOUND_HANDSHAKE_MAX_WIRE_BYTES_V1,
    expanded_bytes: 1024,
};
const FINISHED_WIRE_LIMIT: usize = 1024;

fn encode(schema: u64, binding: [u8; 32], fields: &[&[u8]]) -> Result<Vec<u8>> {
    ensure!(
        binding != [0; 32],
        "bound structured empty connection binding"
    );
    let columns: Vec<_> = fields
        .iter()
        .map(|f| Column::collect(std::iter::once(*f)))
        .collect();
    apfl_batch::encode(schema, binding, 1, &columns, LIMITS).map_err(anyhow::Error::msg)
}
fn decode<'a>(wire: &'a [u8], schema: u64, widths: &[usize]) -> Result<BatchView<'a>> {
    let batch = apfl_batch::decode(wire, schema, LIMITS).map_err(anyhow::Error::msg)?;
    ensure!(
        batch.rows() == 1 && batch.columns() == widths.len() && batch.context != [0; 32],
        "bound structured shape/context"
    );
    for (column, width) in widths.iter().enumerate() {
        ensure!(
            field(&batch, column)?.len() == *width,
            "bound structured field width"
        );
    }
    Ok(batch)
}
fn field<'a>(batch: &BatchView<'a>, column: usize) -> Result<&'a [u8]> {
    batch.field(0, column).map_err(anyhow::Error::msg)
}
fn fixed<const N: usize>(batch: &BatchView<'_>, column: usize) -> Result<[u8; N]> {
    field(batch, column)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("bound structured field width"))
}
fn context(batch: &BatchView<'_>) -> Result<BoundHandshakeContextV1> {
    let result = BoundHandshakeContextV1 {
        version: u16::from_le_bytes(fixed(batch, 0)?),
        purpose: fixed(batch, 1)?,
        initiator_identity_public_key: fixed(batch, 2)?,
        responder_identity_public_key: fixed(batch, 3)?,
        initiator_transport_public_key: fixed(batch, 4)?,
        responder_transport_public_key: fixed(batch, 5)?,
        connection_binding: batch.context,
    };
    check_context(&result)?;
    Ok(result)
}

pub fn encode_offer(value: &BoundNodeHandshakeOfferV1) -> Result<Vec<u8>> {
    check_offer_shape(value)?;
    let c = &value.context;
    let e = &value.e2e;
    encode(
        OFFER_SCHEMA,
        c.connection_binding,
        &[
            &c.version.to_le_bytes(),
            &c.purpose,
            &c.initiator_identity_public_key,
            &c.responder_identity_public_key,
            &c.initiator_transport_public_key,
            &c.responder_transport_public_key,
            &e.version.to_le_bytes(),
            &e.session_id,
            &e.initiator_ephemeral_public_key,
            &e.challenge,
            &e.issued_at_ms.to_le_bytes(),
            &e.expires_at_ms.to_le_bytes(),
            &e.signature,
            &value.signature,
        ],
    )
}
pub fn decode_offer(wire: &[u8]) -> Result<BoundNodeHandshakeOfferV1> {
    let batch = decode(wire, OFFER_SCHEMA, &OFFER_WIDTHS)?;
    let c = context(&batch)?;
    // These redundant signed fields are determined exactly by context; shape
    // validation on encoding ensures this representation preserves the object.
    let e2e = NodeHandshakeOfferV1 {
        version: u16::from_le_bytes(fixed(&batch, 6)?),
        session_id: fixed(&batch, 7)?,
        initiator_peer_id: peer_id_from_ed25519_public_key_v1(&c.initiator_identity_public_key),
        responder_peer_id: peer_id_from_ed25519_public_key_v1(&c.responder_identity_public_key),
        initiator_identity_public_key: c.initiator_identity_public_key,
        initiator_ephemeral_public_key: field(&batch, 8)?.to_vec(),
        challenge: fixed(&batch, 9)?,
        issued_at_ms: u64::from_le_bytes(fixed(&batch, 10)?),
        expires_at_ms: u64::from_le_bytes(fixed(&batch, 11)?),
        signature: field(&batch, 12)?.to_vec(),
    };
    let result = BoundNodeHandshakeOfferV1 {
        context: c,
        e2e,
        signature: field(&batch, 13)?.to_vec(),
    };
    check_offer_shape(&result)?;
    Ok(result)
}
pub fn encode_response(value: &BoundNodeHandshakeResponseV1) -> Result<Vec<u8>> {
    check_response_shape(value)?;
    let c = &value.context;
    let e = &value.e2e;
    encode(
        RESPONSE_SCHEMA,
        c.connection_binding,
        &[
            &c.version.to_le_bytes(),
            &c.purpose,
            &c.initiator_identity_public_key,
            &c.responder_identity_public_key,
            &c.initiator_transport_public_key,
            &c.responder_transport_public_key,
            &e.version.to_le_bytes(),
            &e.session_id,
            &e.responder_ephemeral_public_key,
            &e.challenge,
            &e.issued_at_ms.to_le_bytes(),
            &e.expires_at_ms.to_le_bytes(),
            &e.signature,
            &value.signature,
            &e.response_nonce,
            &e.offer_hash,
            &value.offer_hash,
        ],
    )
}
pub fn decode_response(wire: &[u8]) -> Result<BoundNodeHandshakeResponseV1> {
    let batch = decode(wire, RESPONSE_SCHEMA, &RESPONSE_WIDTHS)?;
    let c = context(&batch)?;
    let e2e = NodeHandshakeResponseV1 {
        version: u16::from_le_bytes(fixed(&batch, 6)?),
        session_id: fixed(&batch, 7)?,
        initiator_peer_id: peer_id_from_ed25519_public_key_v1(&c.initiator_identity_public_key),
        responder_peer_id: peer_id_from_ed25519_public_key_v1(&c.responder_identity_public_key),
        responder_identity_public_key: c.responder_identity_public_key,
        responder_ephemeral_public_key: field(&batch, 8)?.to_vec(),
        challenge: fixed(&batch, 9)?,
        issued_at_ms: u64::from_le_bytes(fixed(&batch, 10)?),
        expires_at_ms: u64::from_le_bytes(fixed(&batch, 11)?),
        signature: field(&batch, 12)?.to_vec(),
        response_nonce: fixed(&batch, 14)?,
        offer_hash: fixed(&batch, 15)?,
    };
    let result = BoundNodeHandshakeResponseV1 {
        context: c,
        offer_hash: fixed(&batch, 16)?,
        e2e,
        signature: field(&batch, 13)?.to_vec(),
    };
    check_response_shape(&result)?;
    Ok(result)
}
fn check_finished(value: &SecureNovoRudpEnvelopeV1) -> Result<()> {
    check_envelope_shape(value)?;
    ensure!(
        value.version == PRODUCT_OVERLAY_PROTOCOL_VERSION_V1
            && value.sequence == 0
            && (16..=512).contains(&value.ciphertext.len()),
        "bound structured Finished fields"
    );
    Ok(())
}
/// Reuses the existing binary Data envelope, never its JSON control decoder.
pub fn encode_finished(value: &SecureNovoRudpEnvelopeV1) -> Result<Vec<u8>> {
    check_finished(value)?;
    let wire = crate::duplex::encode_iroh_envelope_v1(value)?;
    ensure!(
        wire.len() <= FINISHED_WIRE_LIMIT,
        "bound structured Finished wire limit"
    );
    Ok(wire)
}
pub fn decode_finished(wire: &[u8]) -> Result<SecureNovoRudpEnvelopeV1> {
    ensure!(
        wire.len() <= FINISHED_WIRE_LIMIT,
        "bound structured Finished wire limit"
    );
    let result = crate::duplex::decode_iroh_envelope_v1(wire)?;
    check_finished(&result)?;
    Ok(result)
}

#[cfg(test)]
mod tests;
