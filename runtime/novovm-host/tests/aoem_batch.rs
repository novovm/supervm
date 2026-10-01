//! Real packaged AOEM component integration, NOT an authenticated transaction,
//! settled receipt, persistent candidate, parallel block or mainchain TPS test.
//! Fixture bytes are decoded inside AOEM; no precomputed transition is sent in.

use anyhow::{ensure, Context, Result};
use novovm_aoem::{ComputeSession, ComputeTask};
use novovm_host::business::quoted_transfer::{compute_outcome, TransferIntent, TransferSnapshot};
use novovm_host::execution::plan::{
    BatchContext, BatchPlan, OwnedBatchInput, PlanBudget, UnpublishedBatchEffects,
};
use novovm_host::state::frontier::{CaptureBudget, DeclaredAccess};
use novovm_host::state::tree::{
    empty_root, read_state_value, stage_state_update, NodeHash, StateChange, StateNodeReader,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const TAG: &[u8] = b"UNSIGNED-OPERATOR-FIXTURE-v1\0";
const PAYER: &[u8] = b"fixture/payer";
const RECIPIENT: &[u8] = b"fixture/recipient";
const NONCE: &[u8] = b"fixture/nonce";
const FUNDING: &[u8] = b"fixture/pending-fee-funding";

#[derive(Clone, Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);

impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

struct Source {
    memory: Memory,
    reads: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
}

impl StateNodeReader for Source {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.memory.read_node(hash)
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

fn library_path() -> PathBuf {
    if let Some(path) = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY") {
        return PathBuf::from(path);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    if cfg!(target_os = "windows") {
        root.join("aoem/windows/core/bin/aoem_ffi.dll")
    } else {
        root.join("aoem/linux/core/bin/libaoem_ffi.so")
    }
}

fn put(key: &[u8], value: impl Into<Vec<u8>>) -> StateChange {
    StateChange::Put {
        key: key.to_vec(),
        value: value.into(),
    }
}

fn number(input: &OwnedBatchInput, key: &[u8]) -> Result<u128> {
    let value = input.read(key)?.context("fixture number missing")?;
    Ok(u128::from_be_bytes(
        value
            .try_into()
            .map_err(|_| anyhow::anyhow!("bad fixture number"))?,
    ))
}

fn fixture_bytes(case: u8, nonce: u64, amount: u128, fee: u128, cap: u128) -> Vec<u8> {
    let mut raw = TAG.to_vec();
    raw.push(case);
    raw.extend_from_slice(&nonce.to_be_bytes());
    raw.extend_from_slice(&amount.to_be_bytes());
    raw.extend_from_slice(&fee.to_be_bytes());
    raw.extend_from_slice(&cap.to_be_bytes());
    raw
}

// Test-only format. This is deliberately not an implementation of NOV ingress.
fn fixture_intent(raw: &[u8]) -> Result<TransferIntent> {
    ensure!(
        raw.len() == TAG.len() + 57 && raw.starts_with(TAG),
        "bad unsigned fixture"
    );
    let body = &raw[TAG.len()..];
    let self_transfer = matches!(body[0], 5 | 6);
    Ok(TransferIntent {
        tx_hash: [body[0] + 1; 32],
        from: [1; 20].into(),
        to: if self_transfer { [1; 20] } else { [2; 20] }.into(),
        nonce_identity: "unsigned-fixture-signer".to_owned(),
        nonce: u64::from_be_bytes(body[1..9].try_into()?),
        amount: u128::from_be_bytes(body[9..25].try_into()?),
        approved_fee: u128::from_be_bytes(body[25..41].try_into()?),
        fee_cap: u128::from_be_bytes(body[41..57].try_into()?),
    })
}

#[test]
#[ignore = "requires the explicit packaged AOEM library; component test, not mainchain acceptance"]
fn detached_plans_compute_transfers_and_tree_effects_in_real_aoem() -> Result<()> {
    let mut session = ComputeSession::open(&library_path(), 4)?;
    let caller = std::thread::current().id();
    let results: Arc<Mutex<Vec<Option<UnpublishedBatchEffects>>>> =
        Arc::new(Mutex::new((0..64).map(|_| None).collect()));
    let mut tasks: Vec<ComputeTask> = Vec::new();
    let mut expected = Vec::new();
    let mut sources = Vec::new();
    for index in 0..64 {
        let case = (index % 8) as u8;
        // payer, recipient, amount, fee, cap, expected payer/recipient/funding/success.
        let (payer, recipient, amount, fee, cap, after_payer, after_recipient, funding, success) =
            match case {
                0 => (1000, 5, 13, 3, 3, 984, 18, 3, true),
                1 => (10, 5, 11, 3, 3, 7, 5, 3, false),
                2 => (1000, u128::MAX, 1, 3, 3, 997, u128::MAX, 3, false),
                3 => (1000, 5, 13, 3, 2, 1000, 5, 0, false),
                4 => (2, 5, 1, 3, 3, 2, 5, 0, false),
                5 => (1000, 1000, 13, 3, 3, 997, 997, 3, true),
                6 => (10, 10, 11, 3, 3, 7, 7, 3, false),
                7 => (u128::MAX, 5, u128::MAX, 1, 1, u128::MAX - 1, 5, 1, false),
                _ => unreachable!(),
            };
        let initial = stage_state_update(
            &Memory::default(),
            empty_root(),
            &[
                put(PAYER, payer.to_be_bytes()),
                put(RECIPIENT, recipient.to_be_bytes()),
                put(NONCE, 7u128.to_be_bytes()),
                put(FUNDING, 0u128.to_be_bytes()),
            ],
        )?;
        let reference = Memory(initial.nodes().clone());
        let source = Source {
            memory: reference.clone(),
            reads: Arc::new(AtomicUsize::new(0)),
            dropped: Arc::new(AtomicBool::new(false)),
        };
        let context = BatchContext {
            chain_id: 1,
            genesis_config_commitment: [1; 32],
            protocol_commitment: [2; 32],
            business_program: [3; 32],
            semantic_version: 1,
            effect_contract: [4; 32],
            parent_block_hash: [0; 32],
            parent_height: 0,
            parent_state_root: initial.root(),
            parent_receipt_root: empty_root(),
            parent_state_version: 0,
            receipt_codec: [5; 32],
            height: 1,
            slot: index as u64,
            timestamp_unix_ms: 1,
        };
        let plan = BatchPlan::new(
            context,
            vec![fixture_bytes(case, 7, amount, fee, cap)],
            [PAYER, RECIPIENT, NONCE, FUNDING]
                .into_iter()
                .map(|key| DeclaredAccess {
                    key: key.to_vec(),
                    may_put: true,
                    may_delete: false,
                })
                .collect(),
            PlanBudget {
                transactions: 1,
                transaction_bytes: 128,
                body_bytes: 128,
                access_keys: 4,
            },
        )?;
        let commitment = plan.commitment();
        let input = plan.capture(
            &source,
            CaptureBudget {
                keys: 4,
                nodes: 64,
                bytes: 8192,
            },
        )?;
        let captured_reads = source.reads.load(Ordering::SeqCst);
        ensure!(captured_reads > 0);
        sources.push((source.reads.clone(), source.dropped.clone(), captured_reads));
        drop(source);
        let outputs = results.clone();
        tasks.push(Box::new(move || {
            ensure!(
                std::thread::current().id() != caller,
                "operator ran on submitting thread"
            );
            let intent = fixture_intent(&input.plan().raw_transactions()[0])?;
            let payer_balance = number(&input, PAYER)?;
            let self_transfer = intent.from == intent.to;
            let state = TransferSnapshot {
                payer_balance,
                recipient_balance: if self_transfer {
                    payer_balance
                } else {
                    number(&input, RECIPIENT)?
                },
                next_nonce: u64::try_from(number(&input, NONCE)?)?,
            };
            let outcome = compute_outcome(&intent, &state, None)?;
            let delta = outcome.delta();
            let funding_after = number(&input, FUNDING)?
                .checked_add(delta.fee_funding_delta)
                .context("pending funding overflow")?;
            let mut changes = vec![
                put(PAYER, delta.payer.after.to_be_bytes()),
                put(NONCE, u128::from(delta.nonce_after).to_be_bytes()),
                put(FUNDING, funding_after.to_be_bytes()),
            ];
            if !self_transfer {
                changes.push(put(RECIPIENT, delta.recipient.after.to_be_bytes()));
            }
            let effects = input.stage(&changes)?;
            let mut output = effects.plan_commitment().to_vec();
            output.extend_from_slice(&effects.update().root());
            output.push(u8::from(outcome.is_success()));
            outputs
                .lock()
                .map_err(|_| anyhow::anyhow!("fixture result lock poisoned"))?[index] =
                Some(effects);
            Ok(output)
        }));
        expected.push((
            reference,
            context,
            commitment,
            after_payer,
            if matches!(case, 5 | 6) {
                recipient
            } else {
                after_recipient
            },
            funding,
            success,
        ));
    }
    for (_, dropped, _) in &sources {
        ensure!(dropped.load(Ordering::SeqCst));
    }
    let report = session.execute(tasks, Duration::from_secs(30))?;
    assert_eq!(
        (report.processed, report.succeeded, report.failed),
        (64, 64, 0)
    );
    assert_eq!(report.outputs.len(), 64);
    let mut results = results.lock().unwrap();
    for (index, (mut reference, context, commitment, payer, recipient, funding, success)) in
        expected.into_iter().enumerate()
    {
        let effects = results[index]
            .take()
            .context("AOEM operator produced no effects")?;
        assert_eq!(effects.plan_commitment(), commitment);
        assert_eq!(*effects.context(), context);
        assert_eq!(effects.update().parent_root(), context.parent_state_root);
        assert_eq!(report.outputs[index].len(), 65);
        assert_eq!(&report.outputs[index][..32], &commitment);
        assert_eq!(&report.outputs[index][32..64], &effects.update().root());
        assert_eq!(report.outputs[index][64], u8::from(success));
        reference.0.extend(effects.update().nodes().clone());
        for (key, value) in [
            (PAYER, payer),
            (RECIPIENT, recipient),
            (NONCE, 8),
            (FUNDING, funding),
        ] {
            assert_eq!(
                read_state_value(&reference, effects.update().root(), key)?,
                Some(value.to_be_bytes().to_vec())
            );
        }
        assert_eq!(sources[index].0.load(Ordering::SeqCst), sources[index].2);
    }
    eprintln!(
        "64 AOEM component jobs completed; observed peak={} (not a TPS or block-parallelism claim)",
        report.peak_inflight
    );
    Ok(())
}
