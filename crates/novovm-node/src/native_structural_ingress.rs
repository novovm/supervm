//! Bounded structural-batch consumer extracted from the July NovoRUDP path.
//!
//! No socket, controller, execution owner or publication authority lives here.
//! A session id/checksum is NOT peer authentication. Frames remain untrusted;
//! transport receipt/ACK never grants business execution or voting permission.
//! The historical V0 fixture is deliberately absent from business admission.

use anyhow::{ensure, Result};
use novovm_network::{NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::native_pipeline::ingress::apfl::{ApflLimits, ApflTransferBatch};
use crate::native_pipeline::ingress::authentication::{
    authenticate_apfl_row, SignatureCheckedTransfer,
};

pub const DEFAULT_STRUCTURAL_BULK_SIZE: usize = 128;
/// Bounds the entire session's replay fingerprint table, including drained data.
pub const MAX_STRUCTURAL_SESSION_FRAMES: usize = 65_536;

#[derive(Clone, Copy, Debug)]
pub struct StructuralReceiveLimits {
    pub max_total_frames: usize,
    pub max_buffered_frames: usize,
    pub max_buffered_bytes: usize,
    pub max_payload_bytes: usize,
    pub bulk_size: usize,
}

impl Default for StructuralReceiveLimits {
    fn default() -> Self {
        Self {
            max_total_frames: 4096,
            max_buffered_frames: 256,
            max_buffered_bytes: 16 * 1024 * 1024,
            max_payload_bytes: 64 * 1024,
            bulk_size: DEFAULT_STRUCTURAL_BULK_SIZE,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReceiveDisposition {
    Accepted,
    Duplicate,
    IgnoredSession,
    IgnoredControl,
    /// No mutation or receipt occurred. The caller retains the exact input.
    Backpressured(NovoRudpTransportFrameV0),
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    object_id: u64,
    bytes: usize,
    digest: [u8; 32],
}

pub struct NativeStructuralReceiver {
    session: [u8; 16],
    stream_id: u64,
    expected_total: u64,
    limits: StructuralReceiveLimits,
    accepted: BTreeMap<u64, Fingerprint>,
    buffered: BTreeMap<u64, Vec<u8>>,
    buffered_bytes: usize,
    next_sequence: u64,
    // Missing members of the next natural bulk, not all missing session frames.
    // Updated only after acceptance or once when advancing that bulk boundary.
    frontier_missing: usize,
}

impl NativeStructuralReceiver {
    pub fn new(
        session: [u8; 16],
        stream_id: u64,
        expected_total: u64,
        limits: StructuralReceiveLimits,
    ) -> Result<Self> {
        ensure!(
            limits.max_total_frames > 0
                && limits.max_total_frames <= MAX_STRUCTURAL_SESSION_FRAMES
                && expected_total > 0
                && expected_total <= limits.max_total_frames as u64,
            "structural session frame budget exceeded"
        );
        ensure!(
            limits.bulk_size > 0
                && limits.bulk_size <= limits.max_total_frames
                && limits.max_buffered_frames >= limits.bulk_size.min(expected_total as usize)
                && limits.max_buffered_frames <= limits.max_total_frames
                && limits.max_payload_bytes > 0
                // Every admitted slot must support a maximum-size payload.
                // Reserving bytes only for the current bulk is insufficient:
                // after a small bulk drains, buffered future payloads could
                // otherwise prevent the next bulk's large repairs from fitting.
                && limits
                    .max_buffered_frames
                    .checked_mul(limits.max_payload_bytes)
                    .is_some_and(|bytes| bytes <= limits.max_buffered_bytes),
            "invalid structural receive budgets"
        );
        Ok(Self {
            session,
            stream_id,
            expected_total,
            limits,
            accepted: BTreeMap::new(),
            buffered: BTreeMap::new(),
            buffered_bytes: 0,
            next_sequence: 0,
            frontier_missing: limits.bulk_size.min(expected_total as usize),
        })
    }

    /// Caller decodes the original frame codec before handing it here.
    /// Data and Repair share identity; ack_epoch is not part of payload identity.
    pub fn accept(&mut self, frame: NovoRudpTransportFrameV0) -> Result<ReceiveDisposition> {
        if frame.session_id != self.session || frame.stream_id != self.stream_id {
            return Ok(ReceiveDisposition::IgnoredSession);
        }
        if !matches!(
            frame.kind,
            NovoRudpTransportFrameKindV0::Data | NovoRudpTransportFrameKindV0::Repair
        ) {
            return Ok(ReceiveDisposition::IgnoredControl);
        }
        ensure!(
            frame.sequence < self.expected_total,
            "structural sequence out of range"
        );
        ensure!(
            frame.payload.len() <= self.limits.max_payload_bytes,
            "structural payload too large"
        );
        let fingerprint = Fingerprint {
            object_id: frame.object_id,
            bytes: frame.payload.len(),
            digest: Sha256::digest(&frame.payload).into(),
        };
        if let Some(previous) = self.accepted.get(&frame.sequence) {
            ensure!(
                *previous == fingerprint,
                "conflicting structural retransmission"
            );
            return Ok(ReceiveDisposition::Duplicate);
        }
        let bytes = self.buffered_bytes.checked_add(frame.payload.len());
        // Reserve space for every missing member of the next deliverable bulk.
        // Otherwise out-of-order future frames can occupy every slot/byte and
        // permanently block the repair needed to release those same resources.
        let end = (self.next_sequence + self.limits.bulk_size as u64).min(self.expected_total);
        // Duplicate/conflicting frames returned above. A newly accepted member
        // consumes exactly one reserved slot; future frames consume none. Do
        // not rescan the entire bulk for every packet (especially once full).
        let consumes_frontier = (self.next_sequence..end).contains(&frame.sequence);
        let missing_after = self.frontier_missing - usize::from(consumes_frontier);
        let reserved_bytes = missing_after * self.limits.max_payload_bytes;
        if self.buffered.len() + 1 + missing_after > self.limits.max_buffered_frames
            || bytes
                .and_then(|n| n.checked_add(reserved_bytes))
                .is_none_or(|bytes| bytes > self.limits.max_buffered_bytes)
        {
            return Ok(ReceiveDisposition::Backpressured(frame));
        }
        self.accepted.insert(frame.sequence, fingerprint);
        self.buffered_bytes = bytes.expect("checked above");
        self.buffered.insert(frame.sequence, frame.payload);
        self.frontier_missing = missing_after;
        Ok(ReceiveDisposition::Accepted)
    }

    pub fn received_count(&self) -> usize {
        self.accepted.len()
    }
    pub fn is_complete(&self) -> bool {
        self.accepted.len() as u64 == self.expected_total
    }
    pub fn buffered_bytes(&self) -> usize {
        self.buffered_bytes
    }

    /// Observation only. Historical gate ACKs use this before draining;
    /// streaming callers must use received_count/has_received after draining.
    pub fn buffered_payloads(&self) -> &BTreeMap<u64, Vec<u8>> {
        &self.buffered
    }
    pub fn has_received(&self, sequence: u64) -> bool {
        self.accepted.contains_key(&sequence)
    }

    /// Returns one complete, contiguous original-order bulk. The final natural
    /// tail may be smaller. Never crosses a hole or releases an arbitrary prefix.
    /// Payload allocations move unchanged; replay identities remain bounded here.
    pub fn take_ready_bulk(&mut self) -> Option<Vec<(u64, Vec<u8>)>> {
        if self.next_sequence == self.expected_total || self.frontier_missing != 0 {
            return None;
        }
        let end = (self.next_sequence + self.limits.bulk_size as u64).min(self.expected_total);
        let mut result = Vec::with_capacity((end - self.next_sequence) as usize);
        for sequence in self.next_sequence..end {
            let payload = self
                .buffered
                .remove(&sequence)
                .expect("complete frontier tracked");
            self.buffered_bytes -= payload.len();
            result.push((sequence, payload));
        }
        self.next_sequence = end;
        let next_end = (end + self.limits.bulk_size as u64).min(self.expected_total);
        self.frontier_missing = (end..next_end)
            .filter(|sequence| !self.buffered.contains_key(sequence))
            .count();
        Some(result)
    }

    /// Historical finite-run diagnostics may need the incomplete set too.
    /// No business acceptance is implied by retaining those unprocessed bytes.
    pub fn into_buffered_payloads(self) -> BTreeMap<u64, Vec<u8>> {
        self.buffered
    }
}

/// The real signed V3 consumer retains shared APFL columns. It never rebuilds
/// raw transactions or regenerates signatures. Success is authentication only:
/// current nonce, funds, policy, candidate parent and publication remain later
/// boundaries. In particular NOVRUDP-APFL-NTX-V0 is not accepted here.
pub fn authenticate_structured_v3(
    bytes: &[u8],
    expected_count: usize,
    chain_id: u64,
    limits: ApflLimits,
) -> Result<Vec<SignatureCheckedTransfer>> {
    ensure!(
        expected_count > 0 && expected_count <= limits.transactions,
        "structural count out of budget"
    );
    let batch = Arc::new(ApflTransferBatch::decode(bytes, limits)?);
    ensure!(
        batch.len() == expected_count,
        "structural transaction count mismatch"
    );
    (0..batch.len())
        .map(|index| authenticate_apfl_row(Arc::clone(&batch), index, chain_id))
        .collect()
}

#[cfg(test)]
mod tests;
