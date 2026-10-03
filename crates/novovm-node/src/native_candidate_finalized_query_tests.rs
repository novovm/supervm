//! Bounded record queries and durable pool reconciliation, inside the existing
//! real-AOEM finalized-first-block fixture. Cold data is only a test oracle.

use super::*;
use crate::native_state_records::{RecordChange, RecordOverlayV1};
use crate::native_state_storage::AoemStateReaderV1;
use crate::tx_ingress::fresh_pool::{FreshTransactionPool, PendingTransaction};
use native_transfer_record_execution::RootedAccess;

#[test]
fn finalized_nov_balance_typed_read_preserves_u128_and_rejects_corruption() {
    struct Records {
        account: Option<Vec<u8>>,
        balance: Option<Vec<u8>>,
        missing_map: bool,
        fail: bool,
    }
    impl native_store_records::NativeRecordAccessV1 for Records {
        fn read_path(&self, path: &[&str]) -> Result<Option<Vec<u8>>> {
            if self.fail {
                bail!("original read failure");
            }
            match path {
                ["module_state", "account_asset_balances"] if self.missing_map => Ok(None),
                ["module_state", "account_asset_balances", "test"] => Ok(self.account.clone()),
                ["module_state", "account_asset_balances", "test", "NOV"] => {
                    Ok(self.balance.clone())
                }
                _ => Ok(Some(b"{}".to_vec())),
            }
        }
    }
    let mut source = Records {
        account: Some(b"{}".to_vec()),
        balance: None,
        missing_map: false,
        fail: false,
    };
    let read = |source: &Records| {
        live_parent::with_rooted_records_for_test_v1(source, |records| records.nov_balance("test"))
    };
    for balance in [0, (1u128 << 53) + 1, u128::MAX] {
        source.balance = Some(balance.to_string().into_bytes());
        assert_eq!(read(&source).unwrap(), Some(balance));
    }
    for malformed in [
        "-1",
        "1.5",
        "null",
        "{}",
        "\"9\"",
        "340282366920938463463374607431768211456",
    ] {
        source.balance = Some(malformed.as_bytes().to_vec());
        assert!(read(&source).is_err(), "accepted {malformed}");
    }
    source.balance = None;
    assert_eq!(read(&source).unwrap(), None);
    source.account = None;
    assert_eq!(read(&source).unwrap(), None);
    source.balance = Some(b"0".to_vec());
    assert!(read(&source).is_err(), "orphan balance accepted");
    source.account = Some(b"[]".to_vec());
    assert!(read(&source).is_err(), "invalid account object accepted");
    source.account = Some(b"{}".to_vec());
    source.missing_map = true;
    assert!(read(&source).is_err(), "missing map was reported empty");
    source.missing_map = false;
    source.fail = true;
    assert!(read(&source)
        .unwrap_err()
        .to_string()
        .contains("original read failure"));
}

fn pool_image(pool: &FreshTransactionPool) -> Vec<([u8; 32], Vec<u8>, String, u64)> {
    pool.ordered()
        .into_iter()
        .map(|item| (item.hash, item.raw, item.identity, item.nonce))
        .collect()
}

fn rejection<T>(result: Result<T>, label: &str) -> Result<()> {
    let error = result
        .err()
        .with_context(|| format!("{label} unexpectedly succeeded"))?;
    if format!("{error:#}").contains("unexpected full candidate store materialization") {
        bail!("{label} reached an unintended cold fallback: {error:#}");
    }
    Ok(())
}

fn write_fixture_chunk(
    chain: u64,
    params: &serde_json::Value,
    key: &[u8],
    value: Option<Vec<u8>>,
) -> Result<()> {
    let workspace = WorkspaceStore::open(chain, params)?;
    let digest = sha256_bytes_v1(&[
        b"finalized-query-test-chunk-v1\0",
        key,
        &serde_json::to_vec(&value)?,
    ]);
    let write = match value {
        Some(value) => AoemAtomicGraphWriteV1::Put {
            key: key.to_vec(),
            value,
        },
        None => AoemAtomicGraphWriteV1::Delete { key: key.to_vec() },
    };
    workspace.graph.commit(AoemAtomicGraphRequestV1 {
        graph_id: u64::from_be_bytes(digest[..8].try_into()?).max(1),
        steps: vec![AoemAtomicGraphStepV1 {
            task_kind: 0,
            task_payload: vec![],
            writes: vec![write.clone()],
            event: None,
        }],
        completion_write: write,
    })?;
    Ok(())
}

fn without_fixture_chunk(
    chain: u64,
    params: &serde_json::Value,
    key: &[u8],
    test: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let saved = {
        let workspace = WorkspaceStore::open(chain, params)?;
        workspace
            .graph
            .get(key)?
            .context("query fault chunk must already exist")?
    };
    let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        write_fixture_chunk(chain, params, key, None)?;
        test()?;
        let workspace = WorkspaceStore::open(chain, params)?;
        if workspace.graph.get(key)?.is_some() {
            bail!("finalized query repaired a missing fixture chunk");
        }
        Ok(())
    }));
    write_fixture_chunk(chain, params, key, Some(saved.clone()))?;
    {
        let workspace = WorkspaceStore::open(chain, params)?;
        if workspace.graph.get(key)?.as_deref() != Some(saved.as_slice()) {
            bail!("query fixture chunk was not restored exactly");
        }
    }
    match checked {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

/// Follow a single already authenticated leaf to its first immutable chunk.
/// No tree traversal/whole-store scan is used to discover a fault target.
fn physical_chunk_key(
    workspace: &WorkspaceStore,
    root: [u8; 32],
    path: &[String],
) -> Result<Vec<u8>> {
    let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
    let key = native_store_records::key(path)?;
    crate::native_state_records::read_record(&reader, root, &key)?
        .context("query fixture physical record missing")?;
    let leaf = crate::native_state_tree::read_state_value(&reader, root, &key)?
        .context("query fixture physical leaf missing")?;
    if leaf.len() != 40 || &leaf[..4] != b"NRL1" {
        bail!("query fixture record leaf has an unexpected codec");
    }
    Ok([
        b"NST1".as_slice(),
        &workspace.scope,
        b"b",
        &leaf[4..36],
        &0u32.to_be_bytes(),
    ]
    .concat())
}

/// Pure projection-reader checks use staged, hash-valid physical changes. They
/// never construct a verified parent or claim that the forged roots finalized.
fn reject_mismatched_projection(
    view: &live_parent::FinalizedParentViewV1,
    identity: &str,
    receipt: &NovNativeExecutionReceiptV1,
    account: &str,
    params: &serde_json::Value,
) -> Result<()> {
    let workspace = WorkspaceStore::open(view.block().header.chain_id, params)?;
    let (physical_root, state_root, receipt_root, _, _) = view
        .record_state()
        .context("query projection fixture lacks record reference")?
        .rooted_parts()?
        .context("query projection fixture requires three roots")?;
    let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
    let state = RecordOverlayV1::new(&reader, state_root);
    let receipts = RecordOverlayV1::new(&reader, receipt_root);
    for query in 0..3 {
        let mut physical = RecordOverlayV1::new(&reader, physical_root);
        let (path, raw) = if query == 1 {
            let mut bad = receipt.clone();
            bad.status = !bad.status;
            (
                vec!["receipts".into(), bad.tx_hash.clone()],
                serde_json::to_vec(&bad)?,
            )
        } else if query == 2 {
            (
                vec![
                    "module_state".into(),
                    "account_asset_balances".into(),
                    account.into(),
                    "NOV".into(),
                ],
                u128::MAX.to_string().into_bytes(),
            )
        } else {
            (
                vec![
                    "module_state".into(),
                    "native_auth_next_nonces".into(),
                    identity.into(),
                ],
                b"18446744073709551615".to_vec(),
            )
        };
        physical.stage(&[RecordChange::Put {
            key: native_store_records::key(&path)?,
            value: native_store_records::value(&path, &raw)?,
        }])?;
        let access = RootedAccess {
            physical: &physical,
            state: &state,
            receipts: &receipts,
        };
        let result = live_parent::with_rooted_records_for_test_v1(&access, |records| {
            if query == 1 {
                records
                    .receipt(&parse_fixed_hex_32_v1(&receipt.tx_hash, "receipt")?)
                    .map(|_| ())
            } else if query == 2 {
                records.nov_balance(account).map(|_| ())
            } else {
                records.next_nonce(identity).map(|_| ())
            }
        });
        let error = result
            .err()
            .context("hash-valid physical/consensus disagreement was accepted")?;
        let expected = if query == 1 {
            "physical/receipt read mismatch"
        } else {
            "physical/state read mismatch"
        };
        if !format!("{error:#}").contains(expected) {
            bail!("projection fixture failed before the intended cross-check: {error:#}");
        }
    }
    Ok(())
}

pub(crate) fn exercise_finalized_record_queries_for_test_v1(
    chain: u64,
    parent_id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    alternative_spent_raw: &[u8],
    pending_raws: &[Vec<u8>],
) -> Result<()> {
    // Independent complete-image oracle is deliberately outside every guard.
    let cold = load_finalized_genesis_parent_v1(chain, parent_id, genesis, params)?;
    let balance_account = cold
        .state()
        .module_state
        .account_asset_balances
        .iter()
        .find(|(_, assets)| {
            assets
                .get("NOV")
                .is_some_and(|balance| *balance != u128::MAX)
        })
        .context("finalized query fixture lacks a NOV account")?
        .0;
    let original_entries = cold
        .block()
        .body
        .raw_txs
        .iter()
        .map(|raw| PendingTransaction::authenticate(raw.clone(), chain, params))
        .collect::<Result<Vec<_>>>()?;
    if original_entries.len() != 5
        || pending_raws.len() != 2
        || !cold.state().receipts.values().any(|receipt| receipt.status)
        || !cold
            .state()
            .receipts
            .values()
            .any(|receipt| !receipt.status)
    {
        bail!("finalized query fixture requires the existing five-transaction mixed-outcome block");
    }
    let alternative =
        PendingTransaction::authenticate(alternative_spent_raw.to_vec(), chain, params)?;
    let pending = pending_raws
        .iter()
        .map(|raw| PendingTransaction::authenticate(raw.clone(), chain, params))
        .collect::<Result<Vec<_>>>()?;
    let mut entries = original_entries
        .iter()
        .filter(|entry| {
            (entry.identity.as_str(), entry.nonce)
                != (alternative.identity.as_str(), alternative.nonce)
        })
        .cloned()
        .collect::<Vec<_>>();
    if entries.len() != 4
        || cold
            .state()
            .receipts
            .contains_key(&to_hex(&alternative.hash))
    {
        bail!("nonce-only retirement fixture must replace one spent transaction with another hash");
    }
    entries.push(alternative.clone());
    entries.extend(pending.iter().cloned());
    let new_signer = pending
        .iter()
        .find(|entry| {
            !cold
                .state()
                .module_state
                .native_auth_next_nonces
                .contains_key(&entry.identity)
        })
        .context("query fixture lacks an unspent new signer")?;
    if new_signer.nonce != 0 {
        bail!("new signer fixture must begin at nonce zero");
    }
    let view = state_records::without_materialization_for_test(|| {
        live_parent::load_finalized_parent_view_v1(chain, parent_id, genesis, params)
    })?;
    let check_queries = || {
        state_records::without_materialization_for_test(|| {
            view.with_records(params, |records| {
                for (account, assets) in &cold.state().module_state.account_asset_balances {
                    if records.nov_balance(account)? != assets.get("NOV").copied() {
                        bail!("finalized NOV balance differs from complete cold reference");
                    }
                }
                if records
                    .nov_balance("0xabsent-finalized-query-account")?
                    .is_some()
                {
                    bail!("absent account balance was not absent");
                }
                for entry in &original_entries {
                    let expected = cold
                        .state()
                        .receipts
                        .get(&to_hex(&entry.hash))
                        .context("fixture finalized receipt missing")?;
                    if records.receipt(&entry.hash)?.as_ref() != Some(expected)
                        || !records.contains_receipt(&entry.hash)?
                    {
                        bail!("typed finalized receipt differs from the complete cold reference");
                    }
                }
                for entry in &entries {
                    let expected = cold
                        .state()
                        .module_state
                        .native_auth_next_nonces
                        .get(&entry.identity)
                        .copied()
                        .unwrap_or(0);
                    if records.next_nonce(&entry.identity)? != expected {
                        bail!("point nonce differs from the complete cold reference");
                    }
                }
                if records.next_nonce(&new_signer.identity)? != 0
                    || records.receipt(&pending[0].hash)?.is_some()
                    || records.contains_receipt(&pending[0].hash)?
                {
                    bail!("genuinely absent nonce/receipt was not reported as absent");
                }
                Ok(())
            })
        })
    };
    check_queries()?;
    let balance_chunk = {
        let workspace = WorkspaceStore::open(chain, params)?;
        let (physical, _, _, _, _) = view.record_state().unwrap().rooted_parts()?.unwrap();
        physical_chunk_key(
            &workspace,
            physical,
            &[
                "module_state".into(),
                "account_asset_balances".into(),
                balance_account.clone(),
                "NOV".into(),
            ],
        )?
    };
    without_fixture_chunk(chain, params, &balance_chunk, || {
        state_records::without_materialization_for_test(|| {
            rejection(
                view.with_records(params, |records| records.nov_balance(balance_account)),
                "missing finalized balance must not become zero",
            )
        })
    })?;
    check_queries()?;
    let native_path = resolve_native_execution_store_path_from_params_v1(params)
        .context("query fixture requires an explicit native path")?;
    let pool_path = native_path.with_extension("finalized-record-query-pool");
    if pool_path.exists() {
        bail!("query fixture refuses to reuse an existing pool path");
    }
    let open_pool = || FreshTransactionPool::open(&pool_path, chain, genesis, params);
    let admission_path = native_path.with_extension("finalized-record-admission-pool");
    if admission_path.exists() {
        bail!("admission fixture refuses to reuse an existing pool path");
    }
    let open_admission = || FreshTransactionPool::open(&admission_path, chain, genesis, params);
    let mut admission = open_admission()?;
    let admission_sequence = admission.write_sequence_for_test();
    let retained_path = native_path.with_extension("finalized-record-retention-read-pool");
    if retained_path.exists() {
        bail!("retention fixture refuses to reuse an existing pool path");
    }
    let open_retained = || FreshTransactionPool::open(&retained_path, chain, genesis, params);
    let mut retained_probe = open_retained()?;
    if !retained_probe.insert(pending[0].clone())? {
        bail!("retention read-failure fixture requires one already durable entry");
    }
    let retained_image = pool_image(&retained_probe);
    let retained_sequence = retained_probe.write_sequence_for_test();
    // Live entries first ensure a later corrupted stale record cannot leave
    // an admitted prefix. Include both successful and failed finalized raws.
    let admission_entries: Vec<_> = pending
        .iter()
        .chain(original_entries.iter())
        .chain(std::iter::once(&alternative))
        .cloned()
        .collect();
    let mut pool = open_pool()?;
    for entry in &entries {
        if !pool.insert(entry.clone())? {
            bail!("query fixture pending entry was not accepted");
        }
    }
    let all = pool_image(&pool);
    if pool.len() != 7 {
        bail!("query fixture requires exactly seven distinct pool entries");
    }
    // The maximum finalized hash has at least one earlier eligible receipt in
    // the pool's hash iteration order. Its failure must not partially retire it.
    let target_receipt = entries
        .iter()
        .filter(|entry| cold.state().receipts.contains_key(&to_hex(&entry.hash)))
        .max_by_key(|entry| entry.hash)
        .context("query fixture receipt target missing")?;
    let hash = to_hex(&target_receipt.hash);
    let faults = [
        vec!["module_state".into(), "native_auth_next_nonces".into()],
        vec!["receipts".into()],
        vec![
            "module_state".into(),
            "native_auth_next_nonces".into(),
            alternative.identity.clone(),
        ],
        vec!["receipts".into(), hash.clone()],
    ];
    let (keys, authority, evidence, source) = {
        let workspace = WorkspaceStore::open(chain, params)?;
        let (physical, _, _, _, _) = view
            .record_state()
            .context("query parent reference missing")?
            .rooted_parts()?
            .context("query parent requires three roots")?;
        (
            faults
                .iter()
                .map(|path| physical_chunk_key(&workspace, physical, path))
                .collect::<Result<Vec<_>>>()?,
            workspace.graph.get(&native_aoem_owned_state_head_key_v1(
                chain,
                &workspace.namespace,
            ))?,
            workspace.graph.get(&workspace.key(b'h', &parent_id))?,
            execution::read_completed_output_bytes(&workspace, parent_id)?,
        )
    };
    for (index, key) in keys.iter().enumerate() {
        without_fixture_chunk(chain, params, key, || {
            state_records::without_materialization_for_test(|| {
                let query = view.with_records(params, |records| match index {
                    0 => records.next_nonce(&new_signer.identity).map(|_| ()),
                    1 => records.receipt(&pending[0].hash).map(|_| ()),
                    2 => records.next_nonce(&alternative.identity).map(|_| ()),
                    _ => records.receipt(&target_receipt.hash).map(|_| ()),
                });
                rejection(query, "missing finalized query object or record chunk")?;
                rejection(
                    pool.reconcile_rooted(&view, params),
                    "faulted pool reconciliation",
                )?;
                rejection(
                    admission.insert_live_batch_with_retention(
                        admission_entries.clone(),
                        Some(&view),
                        params,
                    ),
                    "faulted batch admission",
                )?;
                // The first input already has a positive durable match, but a
                // later required read failure must return Err, not partial ACKs.
                rejection(
                    retained_probe.insert_live_batch_with_retention(
                        admission_entries.clone(),
                        Some(&view),
                        params,
                    ),
                    "faulted admission after an existing retained match",
                )
            })?;
            if pool_image(&pool) != all {
                bail!("failed record query partially deleted in-memory pool entries");
            }
            if !admission.is_empty() || admission.write_sequence_for_test() != admission_sequence {
                bail!("faulted batch admission wrote an earlier live prefix");
            }
            if pool_image(&retained_probe) != retained_image
                || retained_probe.write_sequence_for_test() != retained_sequence
            {
                bail!("faulted retention result changed an already durable pool");
            }
            Ok(())
        })?;
        drop(pool);
        pool = open_pool()?;
        drop(admission);
        admission = open_admission()?;
        drop(retained_probe);
        retained_probe = open_retained()?;
        if !admission.is_empty() || admission.write_sequence_for_test() != admission_sequence {
            bail!("faulted batch admission changed durable pool after restart");
        }
        if pool_image(&pool) != all {
            bail!("failed record query partially deleted durable pool entries");
        }
        if pool_image(&retained_probe) != retained_image
            || retained_probe.write_sequence_for_test() != retained_sequence
        {
            bail!("failed retained-prefix admission changed after restart");
        }
        check_queries()?;
    }
    state_records::without_materialization_for_test(|| {
        reject_mismatched_projection(
            &view,
            &alternative.identity,
            &cold.state().receipts[&hash],
            balance_account,
            params,
        )
    })?;
    state_records::without_materialization_for_test(|| pool.reconcile_rooted(&view, params))?;
    let remaining = pool_image(&pool);
    if pool.len() != 2
        || pending.iter().any(|entry| !pool.contains(&entry.hash))
        || pool.contains(&alternative.hash)
        || original_entries
            .iter()
            .any(|entry| pool.contains(&entry.hash))
    {
        bail!("finalized receipt/consumed nonce cleanup did not keep only the two pending entries");
    }
    drop(pool);
    let mut reopened = open_pool()?;
    if pool_image(&reopened) != remaining {
        bail!("successful record-backed pool cleanup changed after restart");
    }
    state_records::without_materialization_for_test(|| {
        let reloaded =
            live_parent::load_finalized_parent_view_v1(chain, parent_id, genesis, params)?;
        reopened.reconcile_rooted(&reloaded, params)
    })?;
    if pool_image(&reopened) != remaining {
        bail!("record-backed reconciliation is not idempotent after restart");
    }
    state_records::without_materialization_for_test(|| {
        let mut expected_retained = vec![false; admission_entries.len()];
        expected_retained[..pending.len()].fill(true);
        // Keep result positions distinct even for repeated hashes. These two
        // fabricated authenticated entries exercise the second-layer raw and
        // nonce checks; neither may be persisted or acknowledged.
        let mut mismatch = pending[0].clone();
        mismatch.raw.push(0);
        let mut conflict = pending[0].clone();
        conflict.hash[0] ^= 1;
        conflict.raw.push(1);
        let mut batch = admission_entries.clone();
        batch.extend([
            pending[0].clone(),
            mismatch.clone(),
            original_entries[0].clone(),
            alternative.clone(),
            conflict,
        ]);
        let mut first_expected = expected_retained.clone();
        first_expected.extend([true, false, false, false, false]);
        let accepted = admission.insert_live_batch_with_retention(batch, Some(&view), params)?;
        if accepted.rejected != 2
            || accepted.retained != first_expected
            || pool_image(&admission) != remaining
            || admission.write_sequence_for_test() != admission_sequence + pending.len() as u64
        {
            bail!("admission did not retain exactly the live inputs in their original positions");
        }
        for entry in &pending {
            if admission
                .get(&entry.hash)
                .map(|retained| retained.raw.as_slice())
                != Some(entry.raw.as_slice())
            {
                bail!("retained acknowledgement lookup did not bind exact pending raw bytes");
            }
        }
        if admission.get(&alternative.hash).is_some()
            || original_entries
                .iter()
                .any(|entry| admission.get(&entry.hash).is_some())
        {
            bail!("filtered finalized or consumed input became acknowledgeable");
        }
        let sequence = admission.write_sequence_for_test();
        for _ in 0..3 {
            let accepted = admission.insert_live_batch_with_retention(
                admission_entries.clone(),
                Some(&view),
                params,
            )?;
            if accepted.rejected != 0 || accepted.retained != expected_retained {
                bail!("existing pending replay retention differs from filtered stale inputs");
            }
            // Preserve coverage of the old rejected-count-only API as well.
            if admission.insert_live_batch(admission_entries.clone(), Some(&view), params)? != 0 {
                bail!("exact pending replay was rejected");
            }
            admission.reconcile_rooted(&view, params)?;
        }
        let accepted = admission.insert_live_batch_with_retention(
            vec![
                mismatch.clone(),
                pending[0].clone(),
                mismatch,
                pending[1].clone(),
            ],
            None,
            params,
        )?;
        if accepted.rejected != 2
            || accepted.retained != [false, true, false, true]
            || admission.write_sequence_for_test() != sequence
            || pool_image(&admission) != remaining
        {
            bail!("replays wrote to the pool or bypassed raw-byte equality");
        }
        Ok(())
    })?;
    drop(admission);
    let mut admission = open_admission()?;
    if pool_image(&admission) != remaining
        || admission.write_sequence_for_test() != admission_sequence + pending.len() as u64
    {
        bail!("filtered batch admission changed across restart");
    }
    let sequence = admission.write_sequence_for_test();
    let accepted = state_records::without_materialization_for_test(|| {
        admission.insert_live_batch_with_retention(admission_entries.clone(), Some(&view), params)
    })?;
    let mut expected_retained = vec![false; admission_entries.len()];
    expected_retained[..pending.len()].fill(true);
    if accepted.rejected != 0
        || accepted.retained != expected_retained
        || admission.write_sequence_for_test() != sequence
        || pool_image(&admission) != remaining
    {
        bail!("durable retention acknowledgements changed after pool recovery");
    }
    let workspace = WorkspaceStore::open(chain, params)?;
    if workspace.graph.get(&native_aoem_owned_state_head_key_v1(
        chain,
        &workspace.namespace,
    ))? != authority
        || workspace.graph.get(&workspace.key(b'h', &parent_id))? != evidence
        || execution::read_completed_output_bytes(&workspace, parent_id)? != source
    {
        bail!("finalized queries or pool cleanup changed parent authority/source data");
    }
    Ok(())
}
