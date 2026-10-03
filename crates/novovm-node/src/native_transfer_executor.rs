//! Compute disjoint native-transfer components on AOEM's generic scheduler.
//!
//! This is a pure candidate-computation boundary, not transaction admission or
//! an authority writer. Callers must authenticate the complete signed batch,
//! bind snapshots to a verified parent, and handle failed execution according
//! to the agreed protocol before building/publishing a candidate. No balance,
//! nonce, receipt, or treasury state is published by this module.

use crate::native_transfer_delta::{
    compute_outcome_v1, effects::TransferEffectPlanV1, Account, TransferExecutionOutcomeV1,
    TransferIntent, TransferSnapshot,
};
#[cfg(test)]
use crate::native_transfer_delta::{conflict_segments, TransferError};
use anyhow::{bail, Context, Result};
#[cfg(test)]
use novovm_exec::{execute_aoem_compute_tasks_v1, AoemRuntimeConfig};
use novovm_exec::{AoemComputeSessionV1, AoemComputeTaskV1};
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAX_WAVE_TASKS: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
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

#[cfg(test)]
pub(crate) struct TransferComponentResultV1 {
    /// One result per original transaction, not per component or callback.
    pub outcomes: Vec<TransferExecutionOutcomeV1>,
    pub component_by_index: Vec<usize>,
    pub component_count: usize,
    pub peak_inflight: usize,
}

/// All snapshots refer to the SAME parent prefix. Arithmetic and subsequent
/// component snapshots are evaluated inside the AOEM callback, not on Host.
#[cfg(test)]
pub(crate) fn execute_transfer_components_v1(
    session: &mut AoemComputeSessionV1,
    work: Vec<TransferWorkV1>,
    timeout: Duration,
) -> Result<TransferComponentResultV1> {
    let (mut result, peak_inflight) = execute_transfer_components_with_reducer_v1(
        session,
        work,
        timeout,
        |outcomes, component_by_index, component_count| {
            let commitment = Sha256::digest(
                serde_json::to_vec(&outcomes).context("encode AOEM transfer component result")?,
            )
            .into();
            Ok((
                TransferComponentResultV1 {
                    outcomes,
                    component_by_index,
                    component_count,
                    peak_inflight: 0,
                },
                commitment,
            ))
        },
    )?;
    result.peak_inflight = peak_inflight;
    Ok(result)
}

/// Execute disjoint components and a single deterministic continuation in the
/// SAME AOEM graph. The continuation receives only complete, original-order
/// outcomes, after the existing checked shared-credit reduction. It runs on
/// the last arriving AOEM callback, outside the short-lived join lock.
///
/// The caller supplies domain rules and a bounded owned input capture through
/// `reducer`; neither this module nor AOEM introduces a NOV-specific ABI. The
/// typed result stays private until every callback has completed successfully
/// and its commitment matches the sole non-empty AOEM completion output.
pub(crate) fn execute_transfer_components_with_reducer_v1<R: Send + 'static>(
    session: &mut AoemComputeSessionV1,
    work: Vec<TransferWorkV1>,
    timeout: Duration,
    reducer: impl FnOnce(Vec<TransferExecutionOutcomeV1>, Vec<usize>, usize) -> Result<(R, [u8; 32])>
        + Send
        + 'static,
) -> Result<(R, usize)> {
    let _timing = crate::native_fresh_timing::Span::start("candidate.transfer_components");
    validate_work(&work)?;
    let count = work.len();
    let intents: Vec<_> = work.iter().map(|item| item.intent.clone()).collect();
    let snapshots: Vec<_> = work.iter().map(|item| item.snapshot).collect();
    let plan = Arc::new(TransferEffectPlanV1::build(&intents, &snapshots)?);
    let components = &plan.components;
    let component_count = components.len();
    let mut component_by_index = vec![0; count];
    for (component, indices) in components.iter().enumerate() {
        for &index in indices {
            component_by_index[index] = component;
        }
    }
    let join = Arc::new(Mutex::new(ComponentReductionJoinV1 {
        remaining: component_count,
        outcomes: vec![None; count],
        component_by_index: Some(component_by_index),
        reducer: Some(reducer),
    }));
    let completed = Arc::new(Mutex::new(None::<(R, [u8; 32])>));
    let mut owned: Vec<_> = work.into_iter().map(Some).collect();
    let mut tasks: Vec<AoemComputeTaskV1> = Vec::with_capacity(components.len());
    for indices in components {
        let mut inputs = Vec::with_capacity(indices.len());
        for &index in indices {
            inputs.push(
                owned[index]
                    .take()
                    .context("duplicate transfer component index")?,
            );
        }
        let join = Arc::clone(&join);
        let completed = Arc::clone(&completed);
        let plan = Arc::clone(&plan);
        let indices = indices.clone();
        tasks.push(Box::new(move || {
            let outcomes = compute_component_v1(inputs)?;
            let ready = {
                // No callback waits for another callback. The last arrival
                // takes ownership of the complete set, then releases the
                // lock before credit reduction and the business continuation.
                let mut joined = join
                    .lock()
                    .map_err(|_| anyhow::anyhow!("transfer component join poisoned"))?;
                joined.arrive(&indices, outcomes)?
            };
            let Some(mut ready) = ready else {
                return Ok(Vec::new());
            };
            if plan.has_credit_reduction() {
                plan.reduce_ordered(&mut ready.outcomes)?;
            }
            let (result, commitment) =
                (ready.reducer)(ready.outcomes, ready.component_by_index, component_count)?;
            let mut output = completed
                .lock()
                .map_err(|_| anyhow::anyhow!("transfer reduction result poisoned"))?;
            if output.replace((result, commitment)).is_some() {
                bail!("transfer reduction published more than one typed result");
            }
            Ok(commitment.to_vec())
        }));
    }
    let report = session.execute(tasks, timeout)?;
    if report.processed != components.len() as u64
        || report.succeeded != components.len() as u64
        || report.failed != 0
        || report.outputs.len() != components.len()
    {
        bail!("native transfer component graph did not complete all callbacks");
    }
    let (result, commitment) = completed
        .lock()
        .map_err(|_| anyhow::anyhow!("transfer reduction result poisoned"))?
        .take()
        .context("missing AOEM transfer reduction typed result")?;
    validate_reduction_completion_v1(&report.outputs, commitment)?;
    Ok((result, report.peak_inflight))
}

fn validate_reduction_completion_v1(outputs: &[Vec<u8>], commitment: [u8; 32]) -> Result<()> {
    let mut nonempty = outputs.iter().filter(|bytes| !bytes.is_empty());
    let output = nonempty
        .next()
        .context("missing AOEM transfer reduction commitment")?;
    if nonempty.next().is_some() || output.as_slice() != commitment {
        bail!("AOEM transfer reduction must publish exactly one matching commitment");
    }
    Ok(())
}

struct ComponentReductionReadyV1<F> {
    outcomes: Vec<TransferExecutionOutcomeV1>,
    component_by_index: Vec<usize>,
    reducer: F,
}

struct ComponentReductionJoinV1<F> {
    remaining: usize,
    outcomes: Vec<Option<TransferExecutionOutcomeV1>>,
    component_by_index: Option<Vec<usize>>,
    reducer: Option<F>,
}

impl<F> ComponentReductionJoinV1<F> {
    fn arrive(
        &mut self,
        indices: &[usize],
        outcomes: Vec<TransferExecutionOutcomeV1>,
    ) -> Result<Option<ComponentReductionReadyV1<F>>> {
        if indices.len() != outcomes.len() || self.remaining == 0 {
            bail!("invalid credit reduction component completion");
        }
        for (&index, outcome) in indices.iter().zip(outcomes) {
            let slot = self
                .outcomes
                .get_mut(index)
                .context("invalid credit reduction index")?;
            if slot.replace(outcome).is_some() {
                bail!("duplicate credit reduction outcome");
            }
        }
        self.remaining -= 1;
        if self.remaining != 0 {
            return Ok(None);
        }
        let outcomes = self
            .outcomes
            .iter_mut()
            .map(|slot| {
                slot.take()
                    .context("missing credit reduction component outcome")
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(ComponentReductionReadyV1 {
            outcomes,
            component_by_index: self
                .component_by_index
                .take()
                .context("missing transfer component index mapping")?,
            reducer: self.reducer.take().context("missing transfer reducer")?,
        }))
    }
}

/// Callback-only component evaluator. The private maps contain only touched
/// balances/nonces, never a whole Store or an authoritative ledger. Global fee
/// settlement is intentionally not predicted here; the graph's final reducer
/// validates these results against its original-order prefix and handles stale
/// component suffixes before handing a complete result back to Host.
fn compute_component_v1(work: Vec<TransferWorkV1>) -> Result<Vec<TransferExecutionOutcomeV1>> {
    let mut balances = BTreeMap::<Account, u128>::new();
    let mut nonces = BTreeMap::<String, u64>::new();
    for item in &work {
        for (account, value) in [
            (&item.intent.from, item.snapshot.payer_balance),
            (&item.intent.to, item.snapshot.recipient_balance),
        ] {
            if balances
                .insert(account.clone(), value)
                .is_some_and(|old| old != value)
            {
                bail!("transfer component has inconsistent parent balance snapshots");
            }
        }
        if nonces
            .insert(item.intent.nonce_identity.clone(), item.snapshot.next_nonce)
            .is_some_and(|old| old != item.snapshot.next_nonce)
        {
            bail!("transfer component has inconsistent parent nonce snapshots");
        }
    }
    let mut outcomes = Vec::with_capacity(work.len());
    for item in work {
        let snapshot = TransferSnapshot {
            payer_balance: balances[&item.intent.from],
            recipient_balance: balances[&item.intent.to],
            next_nonce: nonces[&item.intent.nonce_identity],
        };
        let outcome = compute_outcome_v1(&item.intent, &snapshot, item.fee_rejection.as_deref())
            .context("AOEM transfer component input invariant failed")?;
        let delta = outcome.delta();
        balances.insert(delta.payer.account.clone(), delta.payer.after);
        balances.insert(delta.recipient.account.clone(), delta.recipient.after);
        nonces.insert(delta.nonce_identity.clone(), delta.nonce_after);
        outcomes.push(outcome);
    }
    Ok(outcomes)
}

fn validate_work(work: &[TransferWorkV1]) -> Result<()> {
    if work.is_empty() || work.len() > MAX_WAVE_TASKS {
        bail!("native transfer computation requires 1..=1024 transactions");
    }
    let mut hashes = HashSet::with_capacity(work.len());
    if work.iter().any(|work| !hashes.insert(work.intent.tx_hash)) {
        bail!("native transfer computation contains duplicate transaction hashes");
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn execute_transfer_wave_v1(
    runtime: &AoemRuntimeConfig,
    work: Vec<TransferWorkV1>,
    timeout: Duration,
) -> Result<TransferWaveResultV1> {
    let _timing = crate::native_fresh_timing::Span::start("candidate.transfer_wave");
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

#[cfg(test)]
fn validate_wave(work: &[TransferWorkV1]) -> Result<()> {
    validate_work(work)?;
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
    fn component_callback_chains_actual_outcomes_and_preserves_business_failures() {
        let first = work(1);
        let mut second = first.clone();
        second.intent.tx_hash = [91; 32];
        second.intent.nonce = 1;
        second.intent.amount = 1000;
        let mut third = first.clone();
        third.intent.tx_hash = [92; 32];
        third.intent.nonce = 2;
        let observed =
            compute_component_v1(vec![first.clone(), second.clone(), third.clone()]).unwrap();
        let mut snapshot = first.snapshot;
        for (input, result) in [first, second, third].into_iter().zip(&observed) {
            let expected =
                compute_outcome_v1(&input.intent, &snapshot, input.fee_rejection.as_deref())
                    .unwrap();
            assert_eq!(*result, expected);
            snapshot.payer_balance = expected.delta().payer.after;
            snapshot.recipient_balance = expected.delta().recipient.after;
            snapshot.next_nonce = expected.delta().nonce_after;
        }
        assert!(observed[0].is_success());
        assert!(!observed[1].is_success());
        assert_eq!(observed[1].delta().fee_funding_delta, 2);
        assert!(observed[2].is_success());
        assert_eq!(observed[2].delta().nonce_after, 3);
    }

    #[test]
    fn component_callback_rejects_inconsistent_parent_and_nonce_inputs() {
        let first = work(1);
        let mut second = first.clone();
        second.intent.tx_hash = [91; 32];
        second.intent.nonce = 1;
        second.snapshot.payer_balance += 1;
        assert!(compute_component_v1(vec![first.clone(), second.clone()])
            .unwrap_err()
            .to_string()
            .contains("parent balance"));
        second.snapshot = first.snapshot;
        second.snapshot.next_nonce = 1;
        assert!(compute_component_v1(vec![first.clone(), second.clone()])
            .unwrap_err()
            .to_string()
            .contains("parent nonce"));
        second.snapshot = first.snapshot;
        second.intent.nonce = 2;
        assert!(compute_component_v1(vec![first, second]).is_err());
        assert!(validate_work(&vec![work(1); 1025]).is_err());
    }

    #[test]
    fn component_join_releases_only_complete_ordered_inputs_and_reducer_once() {
        let first = compute_component_v1(vec![work(1)]).unwrap().remove(0);
        let second = compute_component_v1(vec![work(3)]).unwrap().remove(0);
        let mut join = ComponentReductionJoinV1 {
            remaining: 2,
            outcomes: vec![None; 2],
            component_by_index: Some(vec![0, 1]),
            reducer: Some(()),
        };
        assert!(join.arrive(&[1], vec![second.clone()]).unwrap().is_none());
        assert!(join.reducer.is_some());
        let ready = join
            .arrive(&[0], vec![first.clone()])
            .unwrap()
            .expect("all components arrived");
        assert_eq!(ready.outcomes, vec![first.clone(), second]);
        assert_eq!(ready.component_by_index, vec![0, 1]);
        assert!(join.reducer.is_none());
        assert!(join.arrive(&[0], vec![first.clone()]).is_err());

        let mut duplicate = ComponentReductionJoinV1 {
            remaining: 2,
            outcomes: vec![None; 2],
            component_by_index: Some(vec![0, 1]),
            reducer: Some(()),
        };
        assert!(duplicate
            .arrive(&[0], vec![first.clone()])
            .unwrap()
            .is_none());
        assert!(duplicate.arrive(&[0], vec![first]).is_err());
        assert!(duplicate.reducer.is_some());
    }

    #[test]
    fn reducer_completion_requires_one_exact_commitment() {
        let commitment = [7; 32];
        assert!(validate_reduction_completion_v1(
            &[vec![], commitment.to_vec(), vec![]],
            commitment
        )
        .is_ok());
        for invalid in [
            vec![],
            vec![vec![]],
            vec![vec![7; 31]],
            vec![vec![8; 32]],
            vec![commitment.to_vec(), commitment.to_vec()],
        ] {
            assert!(validate_reduction_completion_v1(&invalid, commitment).is_err());
        }
    }

    #[test]
    #[ignore = "requires the packaged AOEM runtime; run explicitly for integration evidence"]
    fn real_aoem_transfer_reducer_stays_in_graph_and_fails_closed() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let mut runtime = AoemRuntimeConfig::from_env().unwrap();
        runtime.ingress_workers = Some(4);
        let mut session = AoemComputeSessionV1::open(&runtime).unwrap();
        let submitting_thread = std::thread::current().id();
        let calls = Arc::new(AtomicUsize::new(0));
        let callback_calls = Arc::clone(&calls);
        let inputs = vec![work(1), work(3), work(5)];
        let expected = compute_component_v1(inputs.clone()).unwrap();
        let (observed, peak) = execute_transfer_components_with_reducer_v1(
            &mut session,
            inputs,
            Duration::from_secs(30),
            move |outcomes, mapping, count| {
                assert_eq!(callback_calls.fetch_add(1, Ordering::SeqCst), 0);
                assert_ne!(std::thread::current().id(), submitting_thread);
                assert_eq!(count, 3);
                assert_eq!(mapping, vec![0, 1, 2]);
                assert_eq!(outcomes, expected);
                Ok((outcomes, [73; 32]))
            },
        )
        .unwrap();
        assert_eq!(observed.len(), 3);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(peak >= 1);
        // A successful graph leaves the same resident session usable.
        execute_transfer_components_v1(&mut session, vec![work(9)], Duration::from_secs(30))
            .unwrap();
        let error = execute_transfer_components_with_reducer_v1(
            &mut session,
            vec![work(11)],
            Duration::from_secs(30),
            |_, _, _| -> Result<((), [u8; 32])> {
                bail!("test deterministic business reducer rejected")
            },
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("test deterministic business reducer rejected"));

        // A component failure must not expose partial results to the reducer.
        let mut failed_session = AoemComputeSessionV1::open(&runtime).unwrap();
        let failed_calls = Arc::new(AtomicUsize::new(0));
        let callback_calls = Arc::clone(&failed_calls);
        let mut invalid = work(1);
        invalid.intent.nonce = 9;
        let error = execute_transfer_components_with_reducer_v1(
            &mut failed_session,
            vec![invalid, work(3)],
            Duration::from_secs(30),
            move |_, _, _| {
                callback_calls.fetch_add(1, Ordering::SeqCst);
                Ok(((), [74; 32]))
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("input invariant failed"));
        assert_eq!(failed_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    #[ignore = "requires the packaged AOEM runtime; run explicitly for integration evidence"]
    fn real_aoem_checked_shared_credits_match_serial_callbacks() {
        let mut runtime = AoemRuntimeConfig::from_env().unwrap();
        runtime.ingress_workers = Some(4);
        let mut session = AoemComputeSessionV1::open(&runtime).unwrap();
        let mut inputs = Vec::new();
        // 128 payer/nonce chains previously became ONE component through a
        // common recipient. No sleeps/barriers inflate the overlap observation.
        for nonce in 0..8u64 {
            for id in 1..=128u8 {
                let mut item = work(id);
                item.intent.to = [254; 32].into();
                item.intent.tx_hash[31] = nonce as u8;
                item.intent.nonce = nonce;
                item.snapshot.payer_balance = 1_000;
                item.snapshot.recipient_balance = u64::MAX as u128 + 1;
                if id == 1 && nonce == 1 {
                    item.intent.amount = 10_000; // fee-paid business failure
                }
                if id == 2 && nonce == 2 {
                    item.fee_rejection = Some("fee.quote.test_rejection".into());
                }
                inputs.push(item);
            }
        }
        assert_eq!(
            crate::native_transfer_delta::conflict_components_v1(
                &inputs
                    .iter()
                    .map(|item| item.intent.clone())
                    .collect::<Vec<_>>()
            )
            .len(),
            1
        );
        let expected = compute_component_v1(inputs.clone()).unwrap();
        let actual =
            execute_transfer_components_v1(&mut session, inputs, Duration::from_secs(30)).unwrap();
        assert_eq!(actual.component_count, 128);
        assert_eq!(actual.outcomes, expected);
        assert!(
            actual.peak_inflight > 1,
            "must observe real AOEM callback overlap"
        );
        eprintln!("checked credit AOEM parity: transactions=1024 old_components=1 components={} actual_callback_peak={} ordered_outcomes_equal=true", actual.component_count, actual.peak_inflight);
    }

    #[test]
    #[ignore = "requires the packaged AOEM runtime; run explicitly for integration evidence"]
    fn real_aoem_transfer_components_share_session_and_match_serial_callbacks() {
        let mut runtime = AoemRuntimeConfig::from_env().unwrap();
        runtime.ingress_workers = Some(4);
        let mut session = AoemComputeSessionV1::open(&runtime).unwrap();
        let mut work_items = Vec::new();
        for id in (1..=31).step_by(2) {
            for nonce in 0..8u64 {
                let mut item = work(id);
                item.intent.tx_hash[31] = nonce as u8;
                item.intent.nonce = nonce;
                work_items.push(item);
            }
        }
        let mut expected = Vec::new();
        for chain in work_items.chunks(8) {
            expected.extend(compute_component_v1(chain.to_vec()).unwrap());
        }
        let report =
            execute_transfer_components_v1(&mut session, work_items, Duration::from_secs(30))
                .unwrap();
        assert_eq!(report.component_count, 16);
        assert_eq!(report.outcomes, expected);
        assert_eq!(
            report.component_by_index,
            (0..16).flat_map(|id| [id; 8]).collect::<Vec<_>>()
        );
        assert!(report.peak_inflight >= 1);
        let repair =
            execute_transfer_components_v1(&mut session, vec![work(99)], Duration::from_secs(30))
                .unwrap();
        assert_eq!(
            repair.outcomes,
            compute_component_v1(vec![work(99)]).unwrap()
        );
        eprintln!("native transfer components transactions=128 components=16 callback_peak={} same_session_second_graph=true", report.peak_inflight);
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
