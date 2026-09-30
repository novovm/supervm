//! Compute one independent native-transfer wave on AOEM's generic scheduler.
//!
//! This is a pure candidate-computation boundary, not transaction admission or
//! an authority writer. Callers must authenticate the complete signed batch,
//! bind snapshots to a verified parent, and handle failed execution according
//! to the agreed protocol before building/publishing a candidate. No balance,
//! nonce, receipt, or treasury state is published by this module.

use crate::native_transfer_delta::{
    compute_outcome_v1, conflict_segments, TransferError, TransferExecutionOutcomeV1,
    TransferIntent, TransferSnapshot,
};
use anyhow::{bail, Context, Result};
use novovm_exec::{execute_aoem_compute_tasks_v1, AoemComputeTaskV1, AoemRuntimeConfig};
use std::collections::HashSet;
use std::time::Duration;

const MAX_WAVE_TASKS: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransferWaveResultV1 {
    /// Corresponds exactly to input order, independent of callback order.
    pub outcomes: Vec<Result<TransferExecutionOutcomeV1, TransferError>>,
    /// Actual overlapping AOEM host callbacks, not queue depth or worker count.
    pub peak_inflight: usize,
}

/// A small, authenticated input view. Quotation errors are supplied by the
/// caller, whose unified fee policy checks the slippage-inclusive requirement.
#[derive(Debug, Clone)]
pub(crate) struct TransferWorkV1 {
    pub intent: TransferIntent,
    pub snapshot: TransferSnapshot,
    pub fee_rejection: Option<String>,
}

pub(crate) fn execute_transfer_wave_v1(
    runtime: &AoemRuntimeConfig,
    work: Vec<TransferWorkV1>,
    timeout: Duration,
) -> Result<TransferWaveResultV1> {
    validate_wave(&work)?;
    let count = work.len();
    let tasks: Vec<AoemComputeTaskV1> = work
        .into_iter()
        .map(|work| {
            Box::new(move || {
                // The business calculation happens here, on the AOEM callback.
                // The submitting Host has only captured the small input view.
                serde_json::to_vec(&compute_outcome_v1(
                    &work.intent,
                    &work.snapshot,
                    work.fee_rejection.as_deref(),
                ))
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

fn validate_wave(work: &[TransferWorkV1]) -> Result<()> {
    if work.is_empty() || work.len() > MAX_WAVE_TASKS {
        bail!("native transfer wave requires 1..=1024 tasks");
    }
    let mut hashes = HashSet::with_capacity(work.len());
    if work.iter().any(|work| !hashes.insert(work.intent.tx_hash)) {
        bail!("native transfer wave contains duplicate transaction hashes");
    }
    let intents: Vec<_> = work.iter().map(|work| work.intent.clone()).collect();
    if conflict_segments(&intents).len() != 1 {
        bail!("native transfer wave contains conflicting state accesses");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(id: u8) -> TransferWorkV1 {
        TransferWorkV1 {
            intent: TransferIntent {
                tx_hash: [id; 32],
                from: [id; 20].into(),
                to: [id + 1; 20].into(),
                nonce_identity: format!("signer-{id}"),
                nonce: 0,
                amount: 10,
                approved_fee: 2,
                fee_cap: 2,
            },
            snapshot: TransferSnapshot {
                payer_balance: 100,
                recipient_balance: 5,
                next_nonce: 0,
            },
            fee_rejection: None,
        }
    }

    #[test]
    fn admission_rejects_conflicts_duplicate_hashes_and_empty_work() {
        assert!(validate_wave(&[]).is_err());
        let a = work(1);
        let b = work(3);
        assert!(validate_wave(&[a.clone(), b.clone()]).is_ok());
        assert!(validate_wave(&[a.clone(), work(2)]).is_err());
        let mut duplicate = b.clone();
        duplicate.intent.tx_hash = a.intent.tx_hash;
        assert!(validate_wave(&[a.clone(), duplicate]).is_err());
        let mut nonce_conflict = b;
        nonce_conflict.intent.nonce_identity = a.intent.nonce_identity.clone();
        assert!(validate_wave(&[a, nonce_conflict]).is_err());
    }

    #[test]
    fn outcome_codec_preserves_success_and_rejection() {
        let TransferWorkV1 {
            mut intent,
            mut snapshot,
            ..
        } = work(1);
        let success = compute_outcome_v1(&intent, &snapshot, None);
        assert!(success.as_ref().unwrap().is_success());
        snapshot.payer_balance = 1;
        let failure = compute_outcome_v1(&intent, &snapshot, None);
        assert!(!failure.as_ref().unwrap().is_success());
        snapshot.payer_balance = 2;
        let business_failure = compute_outcome_v1(&intent, &snapshot, None);
        assert!(!business_failure.as_ref().unwrap().is_success());
        intent.amount = u128::MAX - 2;
        snapshot.payer_balance = u128::MAX;
        snapshot.recipient_balance = 0;
        let maximum = compute_outcome_v1(&intent, &snapshot, None);
        assert!(maximum.is_ok());
        intent.nonce = 1;
        let invalid_nonce = compute_outcome_v1(&intent, &snapshot, None);
        assert!(invalid_nonce.is_err());
        for outcome in [success, failure, business_failure, maximum, invalid_nonce] {
            let bytes = serde_json::to_vec(&outcome).unwrap();
            let decoded: Result<TransferExecutionOutcomeV1, TransferError> =
                serde_json::from_slice(&bytes).unwrap();
            assert_eq!(decoded, outcome);
        }
    }

    #[test]
    #[ignore = "requires the packaged AOEM runtime; run explicitly for integration evidence"]
    fn real_aoem_transfer_wave_matches_serial_reference() {
        let mut runtime = AoemRuntimeConfig::from_env().unwrap();
        runtime.ingress_workers = Some(4);
        let mut inputs: Vec<_> = (1..=120).step_by(2).map(work).collect();
        inputs[0].snapshot.payer_balance = 1;
        inputs[1].snapshot.payer_balance = 2;
        inputs[2].fee_rejection = Some("fee.quote.max_pay_exceeded: unit quote".into());
        inputs[3].intent.from = [7; 32].into();
        inputs[3].intent.to = [8; 32].into();
        let expected: Vec<_> = inputs
            .iter()
            .map(|work| {
                compute_outcome_v1(&work.intent, &work.snapshot, work.fee_rejection.as_deref())
            })
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
