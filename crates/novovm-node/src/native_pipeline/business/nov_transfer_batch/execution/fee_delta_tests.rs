//! Compare original full codec updates with parent-bound deltas through the
//! original captured tree, including its existing node-work counters.
use super::*;
use crate::native_pipeline::business::direct_nov_fee::{quote_and_settle, FeeFailureCode};
use crate::native_pipeline::state::frontier::{CaptureBudget, DeclaredAccess, OwnedStateInput};
use crate::native_pipeline::state::tree::{
    empty_root, read_state_value, stage_state_update, take_reachability_decodes_for_test,
    StateChange, StateNodeReader,
};

#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);
impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

fn settled_fees(before: &FeeState) -> Result<FeeState> {
    let policy = super::super::body_tests::policy();
    let request = TransferFeeRequest {
        tx_hash: [71; 32],
        payer: [1; 20].into(),
        recipient: [2; 20].into(),
        asset: "NOV".into(),
        amount: 100,
        pay_asset: "NOV".into(),
        max_pay_amount: 50,
        slippage_bps: 0,
    };
    let effects = quote_and_settle(&request, &policy, before, 10_000, 172_800_500)?;
    ensure!(effects.failure.is_none() && effects.journal.is_some());
    Ok(effects.after_fee_state)
}

fn compare_fee_updates(before: &FeeState, after: &FeeState) -> Result<(usize, usize)> {
    before.validate()?;
    after.validate()?;
    let old = postcard::to_allocvec(before)?;
    let new = postcard::to_allocvec(after)?;
    let full = record_pages::record_changes(FEE_PREFIX, &new, FEE_BYTES)?;
    let delta = record_pages::record_changes_from_parent(FEE_PREFIX, &new, FEE_BYTES, Some(&old))?;
    let mut memory = Memory::default();
    let initial = stage_state_update(
        &memory,
        empty_root(),
        &record_pages::record_changes(FEE_PREFIX, &old, FEE_BYTES)?,
    )?;
    let root = initial.root();
    memory.0.extend(initial.nodes().clone());
    let captured = OwnedStateInput::capture(
        &memory,
        root,
        &record_pages::declarations(FEE_PREFIX, FEE_BYTES, true)?,
        CaptureBudget {
            keys: 33,
            nodes: 512,
            bytes: 128 * 1024,
        },
    )?;
    let captured_old = record_pages::read_with(|key| captured.read(key), FEE_PREFIX, FEE_BYTES)?;
    assert_eq!(captured_old, old);
    let expected = captured.stage(&full)?;
    let actual = captured.stage(&delta)?;
    assert_eq!(actual.parent_root(), expected.parent_root());
    assert_eq!(actual.root(), expected.root());
    assert_eq!(actual.nodes(), expected.nodes());
    memory.0.extend(actual.nodes().clone());
    assert_eq!(
        record_pages::read_with(
            |key| read_state_value(&memory, actual.root(), key),
            FEE_PREFIX,
            FEE_BYTES
        )?,
        new
    );
    Ok((
        delta
            .iter()
            .filter(|change| matches!(change, StateChange::Delete { .. }))
            .count(),
        delta.len(),
    ))
}

#[test]
fn fee_record_delta_preserves_real_settlement_varints_and_tail_shrink() -> Result<()> {
    let ordinary = FeeState::default();
    let settled = settled_fees(&ordinary)?;
    let (deletes, _) = compare_fee_updates(&ordinary, &settled)?;
    assert_eq!(deletes, 0);
    let next = settled_fees(&settled)?;
    assert_eq!(compare_fee_updates(&settled, &next)?.0, 0);
    for (before, after) in [(127, 128), (16_383, 16_384), (128, 127)] {
        let mut old = settled.clone();
        old.diagnostics.quote_max_pay_exceeded = before;
        let mut new = old.clone();
        new.diagnostics.quote_max_pay_exceeded = after;
        assert_ne!(
            postcard::to_allocvec(&old)?.len(),
            postcard::to_allocvec(&new)?.len()
        );
        assert_eq!(compare_fee_updates(&old, &new)?.0, 0);
    }
    let mut large = settled.clone();
    large.diagnostics.last_quote_failure = Some(FeeFailure {
        code: FeeFailureCode::MaxPayExceeded,
        reason: format!(
            "{}: {}",
            FeeFailureCode::MaxPayExceeded.as_str(),
            "x".repeat(400)
        ),
    });
    let mut small = large.clone();
    small.diagnostics.last_quote_failure = None;
    assert!(
        compare_fee_updates(&large, &small)?.0 > 0,
        "genuinely used old fee pages must still be deleted"
    );
    Ok(())
}

#[test]
fn fee_page_delta_removes_reachability_rewalk_for_1024_account_put_run() -> Result<()> {
    const ROWS: usize = 3 * 1024 + 1;
    let before = settled_fees(&FeeState::default())?;
    let after = settled_fees(&before)?;
    let old = postcard::to_allocvec(&before)?;
    let new = postcard::to_allocvec(&after)?;
    let mut initial = record_pages::record_changes(FEE_PREFIX, &old, FEE_BYTES)?;
    let mut declarations = record_pages::declarations(FEE_PREFIX, FEE_BYTES, true)?;
    let mut writes = Vec::with_capacity(ROWS);
    for row in 0..ROWS {
        let key = format!("fee-page-delta/ledger/{row:08}").into_bytes();
        initial.push(StateChange::Put {
            key: key.clone(),
            value: 100_000u64.to_le_bytes().to_vec(),
        });
        writes.push(StateChange::Put {
            key: key.clone(),
            value: 99_999u64.to_le_bytes().to_vec(),
        });
        declarations.push(DeclaredAccess {
            key,
            may_put: true,
            may_delete: false,
        });
    }
    let mut memory = Memory::default();
    let staged = stage_state_update(&memory, empty_root(), &initial)?;
    let root = staged.root();
    memory.0.extend(staged.nodes().clone());
    let captured = OwnedStateInput::capture(
        &memory,
        root,
        &declarations,
        CaptureBudget {
            keys: declarations.len(),
            nodes: 65_536,
            bytes: 16 * 1024 * 1024,
        },
    )?;
    let mut full = record_pages::record_changes(FEE_PREFIX, &new, FEE_BYTES)?;
    assert!(full
        .iter()
        .any(|change| matches!(change, StateChange::Delete { .. })));
    let mut delta =
        record_pages::record_changes_from_parent(FEE_PREFIX, &new, FEE_BYTES, Some(&old))?;
    assert!(delta
        .iter()
        .all(|change| matches!(change, StateChange::Put { .. })));
    full.extend(writes.clone());
    delta.extend(writes);
    take_reachability_decodes_for_test();
    let expected = captured.stage(&full)?;
    let old_rewalk = take_reachability_decodes_for_test();
    let actual = captured.stage(&delta)?;
    let new_rewalk = take_reachability_decodes_for_test();
    assert_eq!(actual.root(), expected.root());
    assert_eq!(actual.nodes(), expected.nodes());
    assert!(old_rewalk >= ROWS);
    assert_eq!(new_rewalk, 0);
    assert!(actual.staged_calls() < expected.staged_calls());
    assert!(actual.max_batched_puts() > expected.max_batched_puts());
    memory.0.extend(actual.nodes().clone());
    assert_eq!(
        record_pages::read_with(
            |key| read_state_value(&memory, actual.root(), key),
            FEE_PREFIX,
            FEE_BYTES
        )?,
        new
    );
    eprintln!("fee-page-delta tree-only: ledger_puts={ROWS} full_changes={} delta_changes={} full_stages={} delta_stages={} full_loaded={} delta_loaded={} full_max_put_run={} delta_max_put_run={} full_reachability={} delta_reachability={}",
        full.len(), delta.len(), expected.staged_calls(), actual.staged_calls(), expected.loaded_nodes(), actual.loaded_nodes(),
        expected.max_batched_puts(), actual.max_batched_puts(), old_rewalk, new_rewalk);
    Ok(())
}
