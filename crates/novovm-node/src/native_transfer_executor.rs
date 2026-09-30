//! Compute one independent native-transfer wave on AOEM's generic scheduler.
//!
//! This is a pure candidate-computation boundary, not transaction admission or
//! an authority writer. Callers must authenticate the complete signed batch,
//! bind snapshots to a verified parent, and handle failed execution according
//! to the agreed protocol before building/publishing a candidate. No balance,
//! nonce, receipt, or treasury state is published by this module.

use crate::native_transfer_delta::{
    compute_delta, conflict_waves, TransferDelta, TransferError, TransferIntent, TransferSnapshot,
};
use anyhow::{bail, Context, Result};
use novovm_exec::{execute_aoem_compute_tasks_v1, AoemComputeTaskV1, AoemRuntimeConfig};
use std::collections::HashSet;
use std::time::Duration;

const MAX_WAVE_TASKS: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferWaveResultV1 {
    /// Corresponds exactly to input order, independent of callback order.
    pub outcomes: Vec<Result<TransferDelta, TransferError>>,
    /// Actual overlapping AOEM host callbacks, not queue depth or worker count.
    pub peak_inflight: usize,
}

pub fn execute_transfer_wave_v1(
    runtime: &AoemRuntimeConfig,
    work: Vec<(TransferIntent, TransferSnapshot)>,
    timeout: Duration,
) -> Result<TransferWaveResultV1> {
    validate_wave(&work)?;
    let count = work.len();
    let tasks: Vec<AoemComputeTaskV1> = work
        .into_iter()
        .map(|(intent, snapshot)| {
            Box::new(move || {
                // The business calculation happens here, on the AOEM callback.
                // The submitting Host has only captured the small input view.
                serde_json::to_vec(&compute_delta(&intent, &snapshot))
                    .context("encode native transfer task outcome")
            }) as AoemComputeTaskV1
        })
        .collect();
    let report = execute_aoem_compute_tasks_v1(runtime, tasks, timeout)?;
    if report.processed != count as u64
        || report.succeeded != count as u64
        || report.failed != 0
        || report.outputs.len() != count
    {
        bail!("native transfer compute wave did not complete all callbacks");
    }
    let outcomes = report
        .outputs
        .iter()
        .map(|bytes| serde_json::from_slice(bytes).context("decode native transfer task outcome"))
        .collect::<Result<_>>()?;
    Ok(TransferWaveResultV1 {
        outcomes,
        peak_inflight: report.peak_inflight,
    })
}

fn validate_wave(work: &[(TransferIntent, TransferSnapshot)]) -> Result<()> {
    if work.is_empty() || work.len() > MAX_WAVE_TASKS {
        bail!("native transfer wave requires 1..=1024 tasks");
    }
    let mut hashes = HashSet::with_capacity(work.len());
    if work
        .iter()
        .any(|(intent, _)| !hashes.insert(intent.tx_hash))
    {
        bail!("native transfer wave contains duplicate transaction hashes");
    }
    let intents: Vec<_> = work.iter().map(|(intent, _)| intent.clone()).collect();
    if conflict_waves(&intents).len() != 1 {
        bail!("native transfer wave contains conflicting state accesses");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(id: u8) -> (TransferIntent, TransferSnapshot) {
        (
            TransferIntent {
                tx_hash: [id; 32],
                from: [id; 20],
                to: [id + 1; 20],
                nonce_identity: format!("signer-{id}"),
                nonce: 0,
                amount: 10,
                approved_fee: 2,
                fee_cap: 2,
            },
            TransferSnapshot {
                payer_balance: 100,
                recipient_balance: 5,
                next_nonce: 0,
            },
        )
    }

    #[test]
    fn admission_rejects_conflicts_duplicate_hashes_and_empty_work() {
        assert!(validate_wave(&[]).is_err());
        let a = work(1);
        let b = work(3);
        assert!(validate_wave(&[a.clone(), b.clone()]).is_ok());
        assert!(validate_wave(&[a.clone(), work(2)]).is_err());
        let mut duplicate = b.clone();
        duplicate.0.tx_hash = a.0.tx_hash;
        assert!(validate_wave(&[a.clone(), duplicate]).is_err());
        let mut nonce_conflict = b;
        nonce_conflict.0.nonce_identity = a.0.nonce_identity.clone();
        assert!(validate_wave(&[a, nonce_conflict]).is_err());
    }

    #[test]
    fn outcome_codec_preserves_success_and_rejection() {
        let (mut intent, mut snapshot) = work(1);
        let success = compute_delta(&intent, &snapshot);
        assert!(success.is_ok());
        snapshot.payer_balance = 1;
        let failure = compute_delta(&intent, &snapshot);
        assert!(failure.is_err());
        intent.amount = u128::MAX - 2;
        snapshot.payer_balance = u128::MAX;
        snapshot.recipient_balance = 0;
        let maximum = compute_delta(&intent, &snapshot);
        assert!(maximum.is_ok());
        for outcome in [success, failure, maximum] {
            let bytes = serde_json::to_vec(&outcome).unwrap();
            let decoded: Result<TransferDelta, TransferError> =
                serde_json::from_slice(&bytes).unwrap();
            assert_eq!(decoded, outcome);
        }
    }

    #[test]
    #[ignore = "requires the packaged AOEM runtime; run explicitly for integration evidence"]
    fn real_aoem_transfer_wave_matches_serial_reference() {
        let mut runtime = AoemRuntimeConfig::from_env().unwrap();
        runtime.ingress_workers = Some(4);
        let inputs: Vec<_> = (1..=120).step_by(2).map(work).collect();
        let expected: Vec<_> = inputs
            .iter()
            .map(|(intent, snapshot)| compute_delta(intent, snapshot))
            .collect();
        let observed = execute_transfer_wave_v1(&runtime, inputs, Duration::from_secs(30)).unwrap();
        assert_eq!(observed.outcomes, expected);
        // Tiny transfers may complete before a second callback starts. Record
        // the actual observation; don't add sleeps to inflate business overlap.
        assert!(observed.peak_inflight >= 1);
        eprintln!(
            "native transfer callbacks peak_inflight={}",
            observed.peak_inflight
        );
    }
}
