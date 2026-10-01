//! Bounded record queries and durable pool reconciliation, inside the existing
//! real-AOEM finalized-first-block fixture. Cold data is only a test oracle.

use super::*;
use crate::native_state_records::{RecordChange, RecordOverlayV1};
use crate::native_state_storage::AoemStateReaderV1;
use crate::tx_ingress::fresh_pool::{FreshTransactionPool, PendingTransaction};
use native_transfer_record_execution::RootedAccess;

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
    for change_receipt in [false, true] {
        let mut physical = RecordOverlayV1::new(&reader, physical_root);
        let (path, raw) = if change_receipt {
            let mut bad = receipt.clone();
            bad.status = !bad.status;
            (
                vec!["receipts".into(), bad.tx_hash.clone()],
                serde_json::to_vec(&bad)?,
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
            if change_receipt {
                records
                    .receipt(&parse_fixed_hex_32_v1(&receipt.tx_hash, "receipt")?)
                    .map(|_| ())
            } else {
                records.next_nonce(identity).map(|_| ())
            }
        });
        let error = result
            .err()
            .context("hash-valid physical/consensus disagreement was accepted")?;
        let expected = if change_receipt {
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
                    admission.insert_live_batch(admission_entries.clone(), Some(&view), params),
                    "faulted batch admission",
                )
            })?;
            if pool_image(&pool) != all {
                bail!("failed record query partially deleted in-memory pool entries");
            }
            if !admission.is_empty() || admission.write_sequence_for_test() != admission_sequence {
                bail!("faulted batch admission wrote an earlier live prefix");
            }
            Ok(())
        })?;
        drop(pool);
        pool = open_pool()?;
        drop(admission);
        admission = open_admission()?;
        if !admission.is_empty() || admission.write_sequence_for_test() != admission_sequence {
            bail!("faulted batch admission changed durable pool after restart");
        }
        if pool_image(&pool) != all {
            bail!("failed record query partially deleted durable pool entries");
        }
        check_queries()?;
    }
    state_records::without_materialization_for_test(|| {
        reject_mismatched_projection(
            &view,
            &alternative.identity,
            &cold.state().receipts[&hash],
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
        if admission.insert_live_batch(admission_entries.clone(), Some(&view), params)? != 0
            || pool_image(&admission) != remaining
            || admission.write_sequence_for_test() != admission_sequence + pending.len() as u64
        {
            bail!("admission did not write exactly the live transactions");
        }
        let sequence = admission.write_sequence_for_test();
        for _ in 0..3 {
            if admission.insert_live_batch(admission_entries.clone(), Some(&view), params)? != 0 {
                bail!("exact pending replay was rejected");
            }
            admission.reconcile_rooted(&view, params)?;
        }
        // Already-authenticated entry is deliberately forged only in this test
        // to exercise the second-layer same-hash/full-raw consistency check.
        let mut mismatch = pending[0].clone();
        mismatch.raw.push(0);
        if admission.insert_live_batch(vec![mismatch], Some(&view), params)? != 1
            || admission.write_sequence_for_test() != sequence
            || pool_image(&admission) != remaining
        {
            bail!("replays wrote to the pool or bypassed raw-byte equality");
        }
        Ok(())
    })?;
    drop(admission);
    let admission = open_admission()?;
    if pool_image(&admission) != remaining
        || admission.write_sequence_for_test() != admission_sequence + pending.len() as u64
    {
        bail!("filtered batch admission changed across restart");
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
