//! Durable faults applied before independent public publication verification.
//! Reuses the real height-three NCW2/V3 fixture; never touches a live chain.

use super::*;
use crate::native_state_storage::AoemStateReaderV1;

fn output_chunk(workspace: &WorkspaceStore, id: &[u8; 32], index: usize) -> Vec<u8> {
    workspace.key(
        b'o',
        &[id.as_slice(), &(index as u32).to_be_bytes()].concat(),
    )
}

fn snapshot(
    chain: u64,
    params: &serde_json::Value,
    keys: &[Vec<u8>],
) -> Result<Vec<Option<Vec<u8>>>> {
    let workspace = WorkspaceStore::open(chain, params)?;
    keys.iter().map(|key| workspace.graph.get(key)).collect()
}

fn write_fixture_key(
    chain: u64,
    params: &serde_json::Value,
    key: &[u8],
    value: Option<Vec<u8>>,
) -> Result<()> {
    let workspace = WorkspaceStore::open(chain, params)?;
    let digest = sha256_bytes_v1(&[
        b"publication-verify-fixture-fault-v1\0",
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

// Restore even if the assertion unwinds: the enclosing fixture continues to
// finalize/retire subsequent blocks in the same real database.
fn with_fault(
    chain: u64,
    params: &serde_json::Value,
    key: &[u8],
    changed_bytes: bool,
    check: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let saved = snapshot(chain, params, &[key.to_vec()])?
        .pop()
        .flatten()
        .context("publication fault target must exist")?;
    let fault = if changed_bytes {
        let mut changed = saved.clone();
        *changed
            .first_mut()
            .context("publication fault target empty")? ^= 1;
        Some(changed)
    } else {
        None
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        write_fixture_key(chain, params, key, fault.clone())?;
        // The injection's provider and OS lock have been dropped before check.
        check()?;
        if snapshot(chain, params, &[key.to_vec()])? != vec![fault] {
            bail!("publication verification repaired its fault target");
        }
        Ok(())
    }));
    write_fixture_key(chain, params, key, Some(saved.clone()))?;
    if snapshot(chain, params, &[key.to_vec()])? != vec![Some(saved)] {
        bail!("publication fixture failed exact key restoration");
    }
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

pub(crate) fn exercise_publication_verify_corruption_for_test_v1(
    chain: u64,
    parent: [u8; 32],
    child: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<()> {
    let verify = || {
        state_records::without_materialization_for_test(|| {
            verify_successor_authority_v1(chain, parent, child, genesis, params)
        })
    };
    let (faults, observed) = {
        let workspace = WorkspaceStore::open(chain, params)?;
        let catalog = workspace.catalog()?;
        for id in [parent, child] {
            let (slot, input) = catalog
                .iter()
                .find(|(_, input)| input.id == id)
                .context("publication fixture workspace missing")?;
            if workspace.status(*slot, input)? != WorkspaceStatusV1::Ready {
                bail!("publication faults require real Ready parent and child");
            }
            // The real height-two Execute parent retains its NCW1 input;
            // its completed record output is the NCW2 child's rooted source.
            if id == child && input.version != DescriptorVersion::Ncw2 {
                bail!("publication faults require a real NCW2 child");
            }
        }
        let child_bytes = execution::read_completed_output_bytes(&workspace, child)?;
        if !state_records::is_delta_document(&child_bytes)? {
            bail!("publication faults require real V3 child output");
        }
        let document = state_records::decode_published_output_metadata::<
            Box<serde_json::value::RawValue>,
        >(&workspace, &child_bytes, &["store"])?;
        let (physical, _, _, _, _) = document
            .state
            .rooted_parts()?
            .context("publication fixture physical root missing")?;
        // The full artifact verifier reads this actual output record. Find its
        // immutable blob through one authenticated point read, not a tree scan.
        let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
        let record = native_store_records::key(&[
            "module_state".into(),
            "aoem_semantic_ledger_sequence".into(),
        ])?;
        crate::native_state_records::read_record(&reader, physical, &record)?
            .context("publication fixture sequence record missing")?;
        let leaf = crate::native_state_tree::read_state_value(&reader, physical, &record)?
            .context("publication fixture sequence leaf missing")?;
        if leaf.len() != 40 || &leaf[..4] != b"NRL1" {
            bail!("publication fixture sequence leaf codec changed");
        }
        let head = native_aoem_owned_state_head_key_v1(chain, &workspace.namespace);
        let faults = vec![
            (
                "missing child completion",
                workspace.key(b'e', &child),
                false,
            ),
            (
                "changed child output chunk",
                output_chunk(&workspace, &child, 0),
                true,
            ),
            (
                "changed child input chunk",
                workspace.chunk_key(&child, 0),
                true,
            ),
            (
                "changed parent source chunk",
                output_chunk(&workspace, &parent, 0),
                true,
            ),
            ("missing parent Ready", workspace.key(b'r', &parent), false),
            (
                "missing parent publication",
                workspace.key(b'h', &parent),
                false,
            ),
            ("changed authority head", head.clone(), true),
            (
                "missing child publication",
                workspace.key(b'h', &child),
                false,
            ),
            (
                "changed output record node",
                [b"NST1".as_slice(), &workspace.scope, b"n", &physical].concat(),
                true,
            ),
            (
                "changed output record blob",
                [
                    b"NST1".as_slice(),
                    &workspace.scope,
                    b"b",
                    &leaf[4..36],
                    &0u32.to_be_bytes(),
                ]
                .concat(),
                true,
            ),
        ];
        let mut observed = vec![head];
        observed.extend(faults.iter().map(|(_, key, _)| key.clone()));
        // Include absent lifecycle markers and every catalog slot, so a failed
        // read cannot silently create a reservation, abort, or retire evidence.
        observed.extend((0..MAX_WORKSPACES_V1).map(|slot| workspace.slot_key(slot)));
        for (_, input) in &catalog {
            observed.extend(
                [b'r', b'a', b'g', b'v', b'e', b'h'].map(|kind| workspace.key(kind, &input.id)),
            );
            observed.extend(
                (0..input.len.div_ceil(CHUNK_BYTES)).map(|i| workspace.chunk_key(&input.id, i)),
            );
            if [parent, child].contains(&input.id) {
                let bytes = execution::read_completed_output_bytes(&workspace, input.id)?;
                observed.extend(
                    (0..bytes.len().div_ceil(CHUNK_BYTES))
                        .map(|i| output_chunk(&workspace, &input.id, i)),
                );
            }
        }
        observed.sort();
        observed.dedup();
        (faults, observed)
    };
    let before = snapshot(chain, params, &observed)?;
    // A known-good independent call prevents vacuous rejection caused by an
    // unregistered/unpublished fixture or the wrong ledger phase.
    let expected = verify().context("intact public publication verification")?;
    if snapshot(chain, params, &observed)? != before {
        bail!("successful public verification changed durable evidence");
    }
    for (label, key, changed_bytes) in faults {
        with_fault(chain, params, &key, changed_bytes, || {
            let faulted = snapshot(chain, params, &observed)?;
            let result = verify();
            if snapshot(chain, params, &observed)? != faulted {
                bail!("{label}: rejected public verification changed durable evidence");
            }
            let error = result
                .err()
                .with_context(|| format!("{label} was accepted"))?;
            if format!("{error:#}").contains("unexpected full candidate store materialization") {
                bail!("{label} reached forbidden cold fallback: {error:#}");
            }
            Ok(())
        })
        .with_context(|| label.to_owned())?;
        if snapshot(chain, params, &observed)? != before || verify()? != expected {
            bail!("{label}: restored public verification differs from its original result");
        }
        if snapshot(chain, params, &observed)? != before {
            bail!("{label}: restored public verification changed durable evidence");
        }
    }
    Ok(())
}
