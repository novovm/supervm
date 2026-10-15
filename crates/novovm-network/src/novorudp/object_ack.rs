//! Bounded binary object binding for the existing NovoRudpAckFrame/window rules.
//! This replaces neither the repair selector nor its pacing policy. A transport
//! completion is not an AOEM receipt or durable-chain acknowledgment. This native
//! profile uses the original 64-fragment window (at most 32 missing runs).

use super::{
    fragments::{MAX_MESSAGE_BYTES, UDP_CHUNK_BYTES},
    *,
};
use anyhow::{ensure, Result};

const MAGIC: &[u8; 8] = b"NVACK001";
const HEADER: usize = 59;
pub const OBJECT_WINDOW: u64 = 64;
const MAX_RUNS: usize = 32;

fn validate(ack: &NovoRudpAckFrame) -> Result<()> {
    ensure!(
        ack.header.version == 1
            && ack.header.kind == NovoRudpFrameKind::Ack
            && ack.header.sequence.is_none()
            && ack.header.epoch > 0,
        "invalid object ACK header"
    );
    ensure!(
        ack.expected_total > 0
            && ack.expected_total <= MAX_MESSAGE_BYTES.div_ceil(UDP_CHUNK_BYTES) as u64
            && ack.missing_count <= ack.expected_total,
        "invalid object ACK counts"
    );
    let ranges = &ack.current_window_missing_ranges;
    ensure!(ranges.len() <= MAX_RUNS, "too many object ACK runs");
    if let Some(window) = ack.current_window {
        ensure!(
            !ack.receiver_done && ack.missing_count > 0 && !ranges.is_empty(),
            "invalid incomplete ACK"
        );
        // Original selector starts at the FIRST MISSING sequence, not a grid.
        ensure!(
            window.start < ack.expected_total
                && window.end_inclusive
                    == (window.start + OBJECT_WINDOW - 1).min(ack.expected_total - 1)
                && ack.header.window_id == Some(window.start / OBJECT_WINDOW),
            "invalid object ACK window"
        );
        let mut previous = None;
        for range in ranges {
            ensure!(
                range.start <= range.end_inclusive
                    && range.start >= window.start
                    && range.end_inclusive <= window.end_inclusive,
                "object ACK run outside window"
            );
            if let Some(last) = previous {
                ensure!(range.start > last + 1, "noncanonical object ACK runs");
            }
            previous = Some(range.end_inclusive);
        }
        ensure!(
            missing_count(ranges) <= ack.missing_count,
            "object ACK missing count below window"
        );
    } else {
        ensure!(
            ack.missing_count == 0 && ranges.is_empty() && ack.header.window_id.is_none(),
            "missing ACK window"
        );
    }
    Ok(())
}

pub fn encode_object_ack(
    id: [u8; 32],
    stream: u64,
    ack: &NovoRudpAckFrame,
) -> Result<NovoRudpTransportFrameV0> {
    validate(ack)?;
    let mut bytes = Vec::with_capacity(HEADER + 8 * ack.current_window_missing_ranges.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&id);
    bytes.extend_from_slice(&(ack.expected_total as u32).to_be_bytes());
    bytes.extend_from_slice(&(ack.missing_count as u32).to_be_bytes());
    bytes.push(u8::from(ack.receiver_done));
    bytes.push(u8::from(ack.current_window.is_some()));
    let window = ack.current_window.unwrap_or(NovoRudpRange::new(0, 0));
    bytes.extend_from_slice(&(window.start as u32).to_be_bytes());
    bytes.extend_from_slice(&(window.end_inclusive as u32).to_be_bytes());
    bytes.push(ack.current_window_missing_ranges.len() as u8);
    for range in &ack.current_window_missing_ranges {
        bytes.extend_from_slice(&(range.start as u32).to_be_bytes());
        bytes.extend_from_slice(&(range.end_inclusive as u32).to_be_bytes());
    }
    Ok(NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Ack,
        ack.header.session_id,
        stream,
        u64::from_be_bytes(id[..8].try_into()?),
        0,
        ack.header.epoch,
        bytes,
    ))
}

/// Call only after authenticating/decrypting the carrier. Bind the FULL object
/// identity, session, stream and shape to retained outbound work before letting
/// this feedback change its repair state. No state changes happen on rejection.
pub fn decode_object_ack(
    frame: &NovoRudpTransportFrameV0,
    session: [u8; 16],
    stream: u64,
    id: [u8; 32],
    total: u64,
) -> Result<NovoRudpAckFrame> {
    ensure!(
        frame.kind == NovoRudpTransportFrameKindV0::Ack
            && frame.session_id == session
            && frame.stream_id == stream
            && frame.object_id == u64::from_be_bytes(id[..8].try_into()?)
            && frame.sequence == 0,
        "object ACK route mismatch"
    );
    let b = &frame.payload;
    ensure!(
        b.len() >= HEADER && b.starts_with(MAGIC) && b[8..40] == id,
        "object ACK identity mismatch"
    );
    let read = |start: usize| -> Result<u64> {
        Ok(u64::from(u32::from_be_bytes(
            b[start..start + 4].try_into()?,
        )))
    };
    let expected_total = read(40)?;
    ensure!(expected_total == total, "object ACK shape mismatch");
    let count = usize::from(b[58]);
    ensure!(
        count <= MAX_RUNS && b.len() == HEADER + 8 * count && b[48] <= 1 && b[49] <= 1,
        "object ACK length/flags mismatch"
    );
    let start = read(50)?;
    let end = read(54)?;
    ensure!(
        b[49] != 0 || (start == 0 && end == 0),
        "noncanonical empty ACK window"
    );
    let current_window = (b[49] == 1).then_some(NovoRudpRange::new(start, end));
    let mut ranges = Vec::with_capacity(count);
    for i in 0..count {
        ranges.push(NovoRudpRange::new(
            read(HEADER + 8 * i)?,
            read(HEADER + 8 * i + 4)?,
        ));
    }
    let ack = NovoRudpAckFrame {
        header: NovoRudpFrameHeader {
            version: 1,
            kind: NovoRudpFrameKind::Ack,
            session_id: session,
            epoch: frame.ack_epoch,
            sequence: None,
            window_id: current_window.map(|w| w.start / OBJECT_WINDOW),
        },
        expected_total,
        receiver_done: b[48] == 1,
        missing_count: read(44)?,
        current_window,
        current_window_missing_ranges: ranges,
    };
    validate(&ack)?;
    Ok(ack)
}

#[cfg(test)]
mod tests;
