//! Bounded binary value inside NVMBOX07 APFL; preserves the signed directory
//! record byte-for-byte at the field level. Not a new directory/signature format.
use super::{fixed, u64_value, Reader};
use crate::product_directory::{PeerSignedRelayRecordV1, RelayEndpointV1, RelayTransportV1};
use anyhow::{ensure, Result};

const MAX_RECORD_BYTES: usize = 8192;
const MAX_ID_BYTES: usize = 256;
const MAX_PEER_BYTES: usize = 128;
const MAX_URI_BYTES: usize = 2048;
const MAX_ENDPOINTS: usize = 3;

fn text_bounds(value: &str, max: usize) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= max,
        "mailbox locator text bound"
    );
    Ok(())
}
pub(crate) fn validate(record: &PeerSignedRelayRecordV1) -> Result<()> {
    ensure!(record.version == 1, "mailbox locator record version");
    text_bounds(&record.record_id, MAX_ID_BYTES)?;
    text_bounds(&record.relay_peer_id, MAX_PEER_BYTES)?;
    ensure!(
        (1..=MAX_ENDPOINTS).contains(&record.endpoints.len()),
        "mailbox locator endpoint count"
    );
    ensure!(
        record.signature.len() == 64 && record.issued_at_ms < record.expires_at_ms,
        "mailbox locator signature/lifetime shape"
    );
    for endpoint in &record.endpoints {
        text_bounds(&endpoint.uri, MAX_URI_BYTES)?;
        ensure!(
            endpoint.max_sessions > 0 && endpoint.max_bytes_per_minute > 0,
            "mailbox locator resource shape"
        );
    }
    Ok(())
}
fn put_text(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u16).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}
fn text(r: &mut Reader<'_>, max: usize) -> Result<String> {
    let length = u16::from_le_bytes(fixed(r.take(2)?)?) as usize;
    ensure!((1..=max).contains(&length), "mailbox locator text bound");
    Ok(std::str::from_utf8(r.take(length)?)?.to_owned())
}
pub(super) fn encode(record: &PeerSignedRelayRecordV1) -> Result<Vec<u8>> {
    validate(record)?;
    let mut out = Vec::new();
    out.extend_from_slice(&record.version.to_le_bytes());
    put_text(&mut out, &record.record_id);
    put_text(&mut out, &record.relay_peer_id);
    out.extend_from_slice(&record.relay_public_key);
    out.push(record.endpoints.len() as u8);
    for endpoint in &record.endpoints {
        out.push(match endpoint.transport {
            RelayTransportV1::Wss443 => 1,
            RelayTransportV1::Quic443 => 2,
            RelayTransportV1::IrohRendezvousV1 => 3,
            RelayTransportV1::TlsTcpV1 => 4,
            RelayTransportV1::Udp => 5,
        });
        put_text(&mut out, &endpoint.uri);
        out.extend_from_slice(&endpoint.priority.to_le_bytes());
        out.extend_from_slice(&endpoint.max_sessions.to_le_bytes());
        out.extend_from_slice(&endpoint.max_bytes_per_minute.to_le_bytes());
    }
    out.extend_from_slice(&record.issued_at_ms.to_le_bytes());
    out.extend_from_slice(&record.expires_at_ms.to_le_bytes());
    out.extend_from_slice(&record.sequence.to_le_bytes());
    out.extend_from_slice(&record.signature);
    ensure!(out.len() <= MAX_RECORD_BYTES, "mailbox locator byte bound");
    Ok(out)
}
pub(super) fn decode(bytes: &[u8]) -> Result<PeerSignedRelayRecordV1> {
    ensure!(
        bytes.len() <= MAX_RECORD_BYTES,
        "mailbox locator byte bound"
    );
    let mut r = Reader(bytes);
    let version = u16::from_le_bytes(fixed(r.take(2)?)?);
    ensure!(version == 1, "mailbox locator record version");
    let record_id = text(&mut r, MAX_ID_BYTES)?;
    let relay_peer_id = text(&mut r, MAX_PEER_BYTES)?;
    let relay_public_key = fixed(r.take(32)?)?;
    let count = r.byte()? as usize;
    ensure!(
        (1..=MAX_ENDPOINTS).contains(&count),
        "mailbox locator endpoint count"
    );
    let mut endpoints = Vec::with_capacity(count);
    for _ in 0..count {
        let transport = match r.byte()? {
            1 => RelayTransportV1::Wss443,
            2 => RelayTransportV1::Quic443,
            3 => RelayTransportV1::IrohRendezvousV1,
            4 => RelayTransportV1::TlsTcpV1,
            5 => RelayTransportV1::Udp,
            _ => anyhow::bail!("mailbox locator transport tag"),
        };
        endpoints.push(RelayEndpointV1 {
            transport,
            uri: text(&mut r, MAX_URI_BYTES)?,
            priority: u16::from_le_bytes(fixed(r.take(2)?)?),
            max_sessions: u32::from_le_bytes(fixed(r.take(4)?)?),
            max_bytes_per_minute: u64_value(r.take(8)?)?,
        });
    }
    let record = PeerSignedRelayRecordV1 {
        version,
        record_id,
        relay_peer_id,
        relay_public_key,
        endpoints,
        issued_at_ms: u64_value(r.take(8)?)?,
        expires_at_ms: u64_value(r.take(8)?)?,
        sequence: u64_value(r.take(8)?)?,
        signature: r.take(64)?.to_vec(),
    };
    r.end()?;
    validate(&record)?;
    Ok(record)
}
