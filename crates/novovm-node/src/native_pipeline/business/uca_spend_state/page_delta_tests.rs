//! Exact original-codec/tree comparisons, not transaction TPS measurements.
use super::*;

#[test]
fn grant_page_delta_budget_and_varint_changes_match_complete_record_updates() -> Result<()> {
    let intent = intent();
    for (before, after) in [
        (100, 99),
        (128, 127),
        (16_384, 16_383),
        (u128::MAX, 127),
        (100, 100),
        (1, 0),
    ] {
        let mut old = grant(&intent);
        old.remaining_debit_budget = before;
        let mut new = old.clone();
        new.remaining_debit_budget = after;
        let old_changes = grant_record_changes(&intent, &old)?;
        let full = grant_record_changes(&intent, &new)?;
        let delta = grant_record_changes_from_parent(&intent, &new, Some((&intent, &old)))?;
        assert_eq!(full.len(), 3);
        assert!(delta
            .iter()
            .all(|change| matches!(change, StateChange::Put { .. })));
        let expected_changes = if old == new {
            0
        } else if postcard::to_allocvec(&old)?.len() == postcard::to_allocvec(&new)?.len() {
            1
        } else {
            2
        };
        assert_eq!(delta.len(), expected_changes, "budget {before} -> {after}");
        let (mut memory, root) = tree(&old_changes);
        let expected = stage_state_update(&memory, root, &full)?;
        let actual = stage_state_update(&memory, root, &delta)?;
        assert_eq!(actual.parent_root(), expected.parent_root());
        assert_eq!(actual.root(), expected.root());
        assert_eq!(actual.nodes(), expected.nodes());
        memory.0.extend(actual.nodes().clone());
        let bytes = record_pages::read_with(
            |key| {
                crate::native_pipeline::state::tree::read_state_value(&memory, actual.root(), key)
            },
            &grant_prefix(&intent)?,
            GRANT_BYTES,
        )?;
        assert_eq!(decode::<SuppliedAssetSpendGrantV2>(&bytes)?, new);
    }
    Ok(())
}

#[test]
fn grant_page_delta_uses_captured_epoch_and_keeps_unknown_parent_full_range() -> Result<()> {
    let old_intent = intent();
    let old = grant(&old_intent);
    let mut new_intent = old_intent.clone();
    new_intent.authorization_epoch += 1;
    let mut new = old.clone();
    new.authorization_epoch = new_intent.authorization_epoch;
    new.remaining_debit_budget = 200;
    assert!(grant_record_changes(&new_intent, &old).is_err());
    let delta = grant_record_changes_from_parent(&new_intent, &new, Some((&old_intent, &old)))?;
    let full = grant_record_changes(&new_intent, &new)?;
    let (memory, root) = tree(&grant_record_changes(&old_intent, &old)?);
    let expected = stage_state_update(&memory, root, &full)?;
    let actual = stage_state_update(&memory, root, &delta)?;
    assert_eq!(actual.root(), expected.root());
    assert_eq!(actual.nodes(), expected.nodes());

    let unknown = grant_record_changes_from_parent(&new_intent, &new, None)?;
    assert_eq!(unknown.len(), full.len());
    assert!(matches!(unknown.last(), Some(StateChange::Delete { .. })));
    let unknown_staged = stage_state_update(&memory, root, &unknown)?;
    assert_eq!(unknown_staged.root(), expected.root());
    assert_eq!(unknown_staged.nodes(), expected.nodes());
    let mut wrong_intent = old_intent;
    wrong_intent.payer = vec![13; 20];
    let mut wrong_old = old;
    wrong_old.account = wrong_intent.payer.clone();
    assert!(
        grant_record_changes_from_parent(&new_intent, &new, Some((&wrong_intent, &wrong_old)))
            .is_err()
    );
    Ok(())
}

#[test]
fn record_page_delta_preserves_real_tail_shrink_growth_and_original_profile() -> Result<()> {
    // Valid grant records currently fit in one page. Exercise the shared page
    // comparison with real two-page records, rather than inventing a large grant.
    let prefix = b"test/native-record-page-delta";
    for (before, after) in [(vec![7; 512], vec![8; 100]), (vec![7; 100], vec![8; 512])] {
        let old = record_pages::record_changes(prefix, &before, 512)?;
        let full = record_pages::record_changes(prefix, &after, 512)?;
        let delta = record_pages::changed_record_pages(full.clone(), old.clone())?;
        if after.len() < before.len() {
            assert!(
                matches!(delta.last(), Some(StateChange::Delete { .. })),
                "real obsolete tail must be removed"
            );
        } else {
            assert!(
                matches!(delta.last(), Some(StateChange::Put { .. })),
                "new tail must be installed"
            );
        }
        let (mut memory, root) = tree(&old);
        let expected = stage_state_update(&memory, root, &full)?;
        let actual = stage_state_update(&memory, root, &delta)?;
        assert_eq!(actual.root(), expected.root());
        assert_eq!(actual.nodes(), expected.nodes());
        memory.0.extend(actual.nodes().clone());
        assert_eq!(
            record_pages::read_with(
                |key| crate::native_pipeline::state::tree::read_state_value(
                    &memory,
                    actual.root(),
                    key
                ),
                prefix,
                512,
            )?,
            after
        );
    }
    assert!(
        record_pages::changed_record_pages(
            record_pages::record_changes(prefix, &[8; 100], 256)?,
            record_pages::record_changes(prefix, &[7; 512], 512)?,
        )
        .is_err(),
        "a smaller page profile cannot hide old tails"
    );
    Ok(())
}

#[test]
fn grant_page_delta_1024_independent_records_share_one_original_tree_put_run() -> Result<()> {
    const ROWS: usize = 1024;
    let records = (0..ROWS)
        .map(|row| {
            let mut intent = intent();
            intent.payer = vec![0; 32];
            intent.payer[..8].copy_from_slice(&(row as u64).to_be_bytes());
            intent.payer[31] = 1;
            let mut old = grant(&intent);
            old.remaining_debit_budget = 100_000;
            (intent, old)
        })
        .collect::<Vec<_>>();
    let mut memory = Memory::default();
    let mut root = empty_root();
    for group in records.chunks(128) {
        let changes = group
            .iter()
            .map(|(intent, old)| grant_record_changes(intent, old))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let staged = stage_state_update(&memory, root, &changes)?;
        root = staged.root();
        memory.0.extend(staged.nodes().clone());
    }
    let mut full = Vec::new();
    let mut delta = Vec::new();
    for (intent, old) in &records {
        let mut new = old.clone();
        new.remaining_debit_budget -= 1;
        full.extend(grant_record_changes(intent, &new)?);
        delta.extend(grant_record_changes_from_parent(
            intent,
            &new,
            Some((intent, old)),
        )?);
    }
    assert_eq!(full.len(), 3 * ROWS);
    assert_eq!(
        full.iter()
            .filter(|change| matches!(change, StateChange::Delete { .. }))
            .count(),
        ROWS
    );
    assert_eq!(delta.len(), ROWS);
    assert!(delta
        .iter()
        .all(|change| matches!(change, StateChange::Put { .. })));
    let expected = stage_state_update(&memory, root, &full)?;
    let actual = stage_state_update(&memory, root, &delta)?;
    assert_eq!(actual.parent_root(), expected.parent_root());
    assert_eq!(actual.root(), expected.root());
    assert_eq!(actual.nodes(), expected.nodes());
    assert_eq!(expected.max_batched_puts(), 2);
    assert_eq!(actual.max_batched_puts(), ROWS);
    assert!(actual.staged_calls() < expected.staged_calls());
    eprintln!("grant-page-delta tree-only: records={ROWS} full_changes={} delta_changes={} full_stages={} delta_stages={} full_loaded={} delta_loaded={} full_max_put_run={} delta_max_put_run={}",
        full.len(), delta.len(), expected.staged_calls(), actual.staged_calls(),
        expected.loaded_nodes(), actual.loaded_nodes(), expected.max_batched_puts(), actual.max_batched_puts());
    Ok(())
}
