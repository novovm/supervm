//! Runs only inside the existing isolated real-AOEM finalized-parent fixture.
//! No production ledger, directory or validator material is changed here.

use super::*;

#[derive(Debug, PartialEq, Eq)]
struct AdmissionFingerprint {
    authority: Option<Vec<u8>>,
    parent_publication: Option<Vec<u8>>,
    parent_output_descriptor: Option<Vec<u8>>,
    candidate_output_descriptor: Option<Vec<u8>>,
    parent_output: Vec<u8>,
    inputs: Vec<Vec<u8>>,
}

fn fingerprint(
    chain: u64,
    parent: [u8; 32],
    plan: &NovNativeCandidateExecutionPlanV1,
    params: &serde_json::Value,
) -> Result<AdmissionFingerprint> {
    let workspace = WorkspaceStore::open(chain, params)?;
    let id = workspace_id(&workspace.scope, &plan.plan_commitment);
    Ok(AdmissionFingerprint {
        authority: workspace.graph.get(&native_aoem_owned_state_head_key_v1(
            chain,
            &workspace.namespace,
        ))?,
        parent_publication: workspace.graph.get(&workspace.key(b'h', &parent))?,
        parent_output_descriptor: workspace.graph.get(&workspace.key(b'v', &parent))?,
        candidate_output_descriptor: workspace.graph.get(&workspace.key(b'v', &id))?,
        parent_output: execution::read_completed_output_bytes(&workspace, parent)?,
        inputs: workspace
            .catalog()?
            .into_iter()
            .map(|(_, descriptor)| descriptor.encode())
            .collect(),
    })
}

fn require_rejection<T>(result: Result<T>, label: &str) -> Result<()> {
    let error = result.err().with_context(|| format!("{label} accepted"))?;
    // A cold fallback hitting the test guard is not the intended rejection.
    if format!("{error:#}").contains("unexpected full candidate store materialization") {
        bail!("{label} reached a forbidden cold fallback: {error:#}");
    }
    Ok(())
}

fn assert_signing_scopes_reject(
    chain: u64,
    parent: [u8; 32],
    candidate: Option<[u8; 32]>,
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<()> {
    let called = std::cell::Cell::new(false);
    let round = with_verified_finalized_parent_round_v1(chain, parent, genesis, params, |_| {
        called.set(true);
        Ok(())
    });
    if called.get() {
        bail!("invalid finalized parent reached the round signing callback");
    }
    require_rejection(round, "invalid finalized parent round scope")?;
    if let Some(candidate) = candidate {
        assert_successor_scope_reject(chain, parent, candidate, genesis, params)?;
    }
    Ok(())
}

fn assert_successor_scope_reject(
    chain: u64,
    parent: [u8; 32],
    candidate: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<()> {
    let called = std::cell::Cell::new(false);
    let successor =
        with_verified_finalized_successor_v1(chain, parent, candidate, genesis, params, |_| {
            called.set(true);
            Ok(())
        });
    if called.get() {
        bail!("invalid finalized evidence reached the successor signing callback");
    }
    require_rejection(successor, "invalid finalized successor signing scope")
}

fn assert_authority_lock(path: &std::path::Path, held: bool) -> Result<()> {
    let contender = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(nov_native_execution_store_lock_path_v1(path))?;
    match (held, contender.try_lock()) {
        (true, Err(std::fs::TryLockError::WouldBlock)) => Ok(()),
        (false, Ok(())) => {
            contender.unlock()?;
            Ok(())
        }
        (expected, observed) => {
            bail!("authority lock held={expected}, actual try_lock={observed:?}")
        }
    }
}

fn assert_round_scope_and_lock(
    chain: u64,
    parent: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<()> {
    let path = resolve_native_execution_store_path_from_params_v1(params)
        .context("round lock fixture requires explicit native path")?;
    let calls = std::cell::Cell::new(0);
    let value = with_verified_finalized_parent_round_v1(chain, parent, genesis, params, |_| {
        calls.set(calls.get() + 1);
        assert_authority_lock(&path, true)?;
        Ok(17)
    })?;
    if value != 17 || calls.get() != 1 {
        bail!("valid parent round scope did not run its callback exactly once");
    }
    assert_authority_lock(&path, false)?;
    let failure: Result<()> =
        with_verified_finalized_parent_round_v1(chain, parent, genesis, params, |_| {
            calls.set(calls.get() + 1);
            assert_authority_lock(&path, true)?;
            bail!("intentional read-only parent round callback failure")
        });
    let error = failure
        .err()
        .context("round callback error was swallowed")?;
    if calls.get() != 2
        || !format!("{error:#}").contains("intentional read-only parent round callback failure")
    {
        bail!("parent round callback failure was not preserved: {error:#}");
    }
    assert_authority_lock(&path, false)
}

fn write_fixture_key(
    chain: u64,
    params: &serde_json::Value,
    key: &[u8],
    value: Option<Vec<u8>>,
) -> Result<()> {
    let workspace = WorkspaceStore::open(chain, params)?;
    let digest = sha256_bytes_v1(&[
        b"live-parent-admission-test-write-v1\0",
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

/// Only mutate an existing, backed-up key in the fixture database. Restore on
/// ordinary errors and unwinding alike, and verify exact bytes before returning.
fn with_fixture_key_fault(
    chain: u64,
    params: &serde_json::Value,
    key: &[u8],
    fault: Option<Vec<u8>>,
    check: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let original = {
        let workspace = WorkspaceStore::open(chain, params)?;
        workspace
            .graph
            .get(key)?
            .context("fixture fault key must already exist")?
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        write_fixture_key(chain, params, key, fault.clone())?;
        check()?;
        let workspace = WorkspaceStore::open(chain, params)?;
        if workspace.graph.get(key)? != fault {
            bail!("live parent reader repaired or changed the injected fault");
        }
        Ok(())
    }));
    // Do not use `?` on the checked operation until the original is restored.
    write_fixture_key(chain, params, key, Some(original.clone()))?;
    {
        let workspace = WorkspaceStore::open(chain, params)?;
        if workspace.graph.get(key)?.as_deref() != Some(original.as_slice()) {
            bail!("live parent fixture failed exact key restoration");
        }
    }
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

/// This must run against an actually finalized NCW2 parent, not the first
/// NCW1 block. Rebinding Ready alone cannot authorize a changed parent digest.
pub(crate) fn assert_live_parent_descriptor_binding_for_test_v1(
    plan: &NovNativeCandidateExecutionPlanV1,
    parent_id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<()> {
    let chain = plan.context.chain_id;
    let before = fingerprint(chain, parent_id, plan, params)?;
    let (slot, original, slot_key, ready_key, forged_ready, input_chunks, namespace) = {
        let workspace = WorkspaceStore::open(chain, params)?;
        let (slot, original) = workspace
            .catalog()?
            .into_iter()
            .find(|(_, descriptor)| descriptor.id == parent_id)
            .context("descriptor binding fixture parent missing")?;
        if original.version != DescriptorVersion::Ncw2
            || workspace.status(slot, &original)? != WorkspaceStatusV1::Ready
        {
            bail!("descriptor binding fixture requires a real Ready NCW2 parent");
        }
        let mut forged = original.clone();
        forged.parent_snapshot[0] ^= 1;
        let chunks = (0..original.len.div_ceil(CHUNK_BYTES))
            .map(|index| {
                let key = workspace.chunk_key(&parent_id, index);
                Ok((key.clone(), workspace.graph.get(&key)?))
            })
            .collect::<Result<Vec<_>>>()?;
        (
            slot,
            original,
            workspace.slot_key(slot),
            workspace.key(b'r', &parent_id),
            workspace.marker(b'r', slot, &forged),
            chunks,
            parse_fixed_hex_32_v1(&workspace.namespace, "fixture namespace")?,
        )
    };
    let current = state_records::without_materialization_for_test(|| {
        let current =
            live_parent::load_finalized_parent_view_v1(chain, parent_id, genesis, params)?;
        assert_round_scope_and_lock(chain, parent_id, genesis, params)?;
        Ok(current)
    })?;
    let native_path = resolve_native_execution_store_path_from_params_v1(params)
        .context("descriptor binding fixture native path missing")?;
    let archive_bytes = || -> Result<Vec<u8>> {
        let archive = NovNativeBlockLedgerV1::load_fresh_finalized_archive_v1(
            &nov_native_block_ledger_rocksdb_path_v1(&native_path),
            genesis,
            namespace,
            current.block().header.height,
        )?;
        Ok(serde_json::to_vec(&(
            archive.config,
            archive.block,
            archive.proof,
            archive.execution,
            archive.commitment,
        ))?)
    };
    let archived = archive_bytes()?;
    let mut forged = original;
    forged.parent_snapshot[0] ^= 1;
    with_fixture_key_fault(chain, params, &slot_key, Some(forged.encode()), || {
        with_fixture_key_fault(chain, params, &ready_key, Some(forged_ready), || {
            {
                let workspace = WorkspaceStore::open(chain, params)?;
                if workspace.status(slot, &forged)? != WorkspaceStatusV1::Ready {
                    bail!("forged descriptor fixture did not rebind its Ready marker");
                }
            }
            let faulted = fingerprint(chain, parent_id, plan, params)?;
            state_records::without_materialization_for_test(|| {
                require_rejection(
                    live_parent::load_finalized_parent_view_v1(chain, parent_id, genesis, params),
                    "NCW2 parent-snapshot descriptor with a matching forged Ready marker",
                )?;
                require_rejection(
                    create_from_finalized_genesis_v1(plan, parent_id, genesis, params),
                    "candidate admission from a changed NCW2 parent descriptor",
                )?;
                assert_signing_scopes_reject(chain, parent_id, None, genesis, params)
            })?;
            if fingerprint(chain, parent_id, plan, params)? != faulted
                || archive_bytes()? != archived
            {
                bail!(
                    "descriptor rejection changed source, outputs, authority or finality archive"
                );
            }
            let workspace = WorkspaceStore::open(chain, params)?;
            for (key, expected) in &input_chunks {
                if workspace.graph.get(key)? != *expected {
                    bail!("descriptor rejection changed original input bytes");
                }
            }
            Ok(())
        })
    })?;
    if fingerprint(chain, parent_id, plan, params)? != before || archive_bytes()? != archived {
        bail!("descriptor fault fixture failed to restore its exact original state");
    }
    state_records::without_materialization_for_test(|| {
        live_parent::load_finalized_parent_view_v1(chain, parent_id, genesis, params)
    })?;
    Ok(())
}

pub(crate) fn exercise_live_parent_admission_for_test_v1(
    plan: &NovNativeCandidateExecutionPlanV1,
    parent_id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    previous_head: &[u8],
) -> Result<WorkspaceInfoV1> {
    let chain = plan.context.chain_id;
    let before = fingerprint(chain, parent_id, plan, params)?;
    let view = state_records::without_materialization_for_test(|| {
        let view = live_parent::load_finalized_parent_view_v1(chain, parent_id, genesis, params)?;
        if view.workspace_id() != parent_id
            || view.block().header.block_hash != plan.context.parent_block_hash
            || view.successor_plan(plan.context, plan.raw_txs.clone(), params)? != *plan
        {
            bail!("light live parent planning differs from the complete cold reference");
        }
        assert_round_scope_and_lock(chain, parent_id, genesis, params)?;
        Ok(view)
    })?;
    if fingerprint(chain, parent_id, plan, params)? != before {
        bail!("loading/planning a live parent changed candidate or authority data");
    }
    let ncw2_parent = {
        let workspace = WorkspaceStore::open(chain, params)?;
        workspace.catalog()?.iter().any(|(_, descriptor)| {
            descriptor.id == parent_id && descriptor.version == DescriptorVersion::Ncw2
        })
    };
    if ncw2_parent {
        assert_live_parent_descriptor_binding_for_test_v1(plan, parent_id, genesis, params)?;
    }
    state_records::without_materialization_for_test(|| {
        let mut wrong_id = parent_id;
        wrong_id[0] ^= 1;
        require_rejection(
            live_parent::load_finalized_parent_view_v1(chain, wrong_id, genesis, params),
            "wrong finalized workspace",
        )?;
        assert_signing_scopes_reject(chain, wrong_id, None, genesis, params)?;
        let mut wrong_genesis = genesis;
        wrong_genesis[0] ^= 1;
        require_rejection(
            live_parent::load_finalized_parent_view_v1(chain, parent_id, wrong_genesis, params),
            "wrong finalized genesis",
        )?;
        assert_signing_scopes_reject(chain, parent_id, None, wrong_genesis, params)?;
        let spent = view
            .block()
            .body
            .raw_txs
            .first()
            .context("fixture parent has no transaction")?;
        require_rejection(
            view.successor_plan(plan.context, vec![spent.clone()], params),
            "already consumed finalized nonce",
        )?;
        let mut bad_tx = decode_nov_native_tx_wire_v1(&plan.raw_txs[0])?;
        *bad_tx
            .signature
            .last_mut()
            .context("fixture signature missing")? ^= 1;
        let bad_raw = novovm_protocol::encode_nov_native_tx_wire_v1(&bad_tx)?;
        require_rejection(
            view.successor_plan(plan.context, vec![bad_raw.clone()], params),
            "invalid successor signature",
        )?;
        let invalid_plan = NovNativeCandidateExecutionPlanV1::new(
            plan.context,
            plan.protocol_config_commitment,
            plan.pre_state_root,
            plan.aoem_parent.clone(),
            vec![canonical_nov_native_tx_hash_from_payload_v1(&bad_raw)?],
            vec![bad_raw],
        )?;
        require_rejection(
            create_from_finalized_genesis_v1(&invalid_plan, parent_id, genesis, params),
            "invalid signature candidate admission",
        )?;
        let mut wrong_root = plan.pre_state_root;
        wrong_root[0] ^= 1;
        let mut wrong_parent = plan.aoem_parent.clone();
        wrong_parent
            .as_mut()
            .context("live parent fixture plan lacks an AOEM parent")?
            .state_root = wrong_root;
        let wrong_root_plan = NovNativeCandidateExecutionPlanV1::new(
            plan.context,
            plan.protocol_config_commitment,
            wrong_root,
            wrong_parent,
            plan.tx_hashes.clone(),
            plan.raw_txs.clone(),
        )?;
        require_rejection(
            create_from_finalized_genesis_v1(&wrong_root_plan, parent_id, genesis, params),
            "unbound parent state root candidate admission",
        )?;
        // Verify the real parent's independent proof boundary without changing
        // immutable ledger archives or fabricating a new persisted certificate.
        let mut proof = view.finality_proof().clone();
        let crate::native_block_seal::round_message::NovNativeSealRoundMessageV1::DecisionCertificateV3 { decision, .. } = &mut proof.witness else {
            bail!("live parent fixture requires its original decision certificate");
        };
        decision.votes.truncate(2);
        require_rejection(
            proof.validate_archived_certificate(view.genesis_config(), view.block()),
            "two-vote parent finality proof",
        )?;
        Ok(())
    })?;
    if fingerprint(chain, parent_id, plan, params)? != before {
        bail!("rejected authentication changed source, reservation or authority");
    }
    let (head_key, evidence_key, root_keys) = {
        let workspace = WorkspaceStore::open(chain, params)?;
        let document = state_records::decode_published_output_metadata::<
            Box<serde_json::value::RawValue>,
        >(&workspace, &before.parent_output, &["store"])?;
        let (physical, state, receipts, _, _) = document
            .state
            .rooted_parts()?
            .context("live parent fixture requires three committed roots")?;
        (
            native_aoem_owned_state_head_key_v1(chain, &workspace.namespace),
            workspace.key(b'h', &parent_id),
            [physical, state, receipts]
                .map(|root| [b"NST1".as_slice(), &workspace.scope, b"n", &root].concat()),
        )
    };
    let check_bad_live_authority = || {
        let faulted = fingerprint(chain, parent_id, plan, params)?;
        state_records::without_materialization_for_test(|| {
            require_rejection(
                live_parent::load_finalized_parent_view_v1(chain, parent_id, genesis, params),
                "unavailable or stale live parent",
            )?;
            require_rejection(
                create_from_finalized_genesis_v1(plan, parent_id, genesis, params),
                "candidate admission with unavailable or stale parent",
            )?;
            assert_signing_scopes_reject(chain, parent_id, None, genesis, params)
        })?;
        if fingerprint(chain, parent_id, plan, params)? != faulted {
            bail!("rejected live admission mutated source, reservation or authority");
        }
        Ok(())
    };
    if previous_head.is_empty() || before.authority.as_deref() == Some(previous_head) {
        bail!("stale-head fixture needs a distinct earlier genuine authority head");
    }
    with_fixture_key_fault(
        chain,
        params,
        &head_key,
        Some(previous_head.to_vec()),
        check_bad_live_authority,
    )?;
    with_fixture_key_fault(chain, params, &head_key, None, check_bad_live_authority)?;
    with_fixture_key_fault(chain, params, &evidence_key, None, check_bad_live_authority)?;
    for root in &root_keys {
        with_fixture_key_fault(chain, params, root, None, check_bad_live_authority)?;
    }
    if fingerprint(chain, parent_id, plan, params)? != before {
        bail!("live parent fault fixture did not restore its original facts");
    }
    let info = state_records::without_materialization_for_test(|| {
        let current =
            live_parent::load_finalized_parent_view_v1(chain, parent_id, genesis, params)?;
        if current.successor_plan(plan.context, plan.raw_txs.clone(), params)? != *plan {
            bail!("restored light parent produced a different plan");
        }
        let info = create_from_finalized_genesis_v1(plan, parent_id, genesis, params)?;
        if info.schema != LIGHT_SCHEMA {
            bail!("guarded fresh successor creation did not select NCW2");
        }
        if create_from_finalized_genesis_v1(plan, parent_id, genesis, params)? != info {
            bail!("light candidate creation replay changed its exact reservation");
        }
        Ok(info)
    })?;
    let after = fingerprint(chain, parent_id, plan, params)?;
    if after.authority != before.authority
        || after.parent_publication != before.parent_publication
        || after.parent_output_descriptor != before.parent_output_descriptor
        || after.parent_output != before.parent_output
        || after.candidate_output_descriptor != before.candidate_output_descriptor
        || after.inputs.len() != before.inputs.len() + 1
        || before
            .inputs
            .iter()
            .any(|input| !after.inputs.contains(input))
    {
        bail!("light input creation changed authority or execution outputs");
    }
    // A saved Ready reservation is not continuing permission to bypass live
    // authority. The same rejected calls must remain read-only on re-entry.
    with_fixture_key_fault(
        chain,
        params,
        &head_key,
        Some(previous_head.to_vec()),
        check_bad_live_authority,
    )?;
    with_fixture_key_fault(chain, params, &evidence_key, None, check_bad_live_authority)?;
    if fingerprint(chain, parent_id, plan, params)? != after {
        bail!("rejected Ready candidate re-entry changed its source or reservation");
    }
    Ok(info)
}

/// Call only after real execution and ledger registration of an unfinalized
/// child. The existing four-block record fixture supplies actual certificates
/// for both the live parent and its retained immediate predecessor.
pub(crate) fn exercise_live_successor_signing_scope_for_test_v1(
    chain: u64,
    parent_id: [u8; 32],
    candidate_id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<()> {
    let path = resolve_native_execution_store_path_from_params_v1(params)
        .context("successor signing fixture requires native path")?;
    let (parent_fault_keys, candidate_fault_keys, observed_keys) = {
        let workspace = WorkspaceStore::open(chain, params)?;
        let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "signing fixture namespace")?;
        let (_, previous) = NovNativeBlockLedgerV1::load_fresh_finalized_tip_archive_v1(
            &nov_native_block_ledger_rocksdb_path_v1(&path),
            genesis,
            namespace,
            parent_id,
        )?;
        let previous = previous.context("signing fixture requires a real finalized predecessor")?;
        let output_chunk =
            |id: &[u8; 32]| workspace.key(b'o', &[id.as_slice(), &0u32.to_be_bytes()].concat());
        let mut parent_keys = vec![native_aoem_owned_state_head_key_v1(
            chain,
            &workspace.namespace,
        )];
        for source in [parent_id, previous.execution.workspace_id] {
            let bytes = execution::read_completed_output_bytes(&workspace, source)?;
            let document = state_records::decode_published_output_metadata::<
                Box<serde_json::value::RawValue>,
            >(&workspace, &bytes, &["store"])?;
            let (physical, state, receipts, _, _) = document
                .state
                .rooted_parts()?
                .context("signing fixture requires retained record-profile source roots")?;
            parent_keys.extend([
                workspace.key(b'h', &source),
                workspace.key(b'v', &source),
                workspace.key(b'e', &source),
                output_chunk(&source),
            ]);
            parent_keys.extend(
                [physical, state, receipts]
                    .map(|root| [b"NST1".as_slice(), &workspace.scope, b"n", &root].concat()),
            );
        }
        parent_keys.sort();
        parent_keys.dedup();
        let candidate_keys = [
            (
                "candidate output reservation",
                workspace.key(b'v', &candidate_id),
                true,
            ),
            (
                "candidate completion marker",
                workspace.key(b'e', &candidate_id),
                false,
            ),
            (
                "candidate first output chunk",
                output_chunk(&candidate_id),
                false,
            ),
        ];
        let mut observed = parent_keys.clone();
        observed.extend(candidate_keys.iter().map(|(_, key, _)| key.clone()));
        for (slot, descriptor) in workspace.catalog()? {
            observed.push(workspace.slot_key(slot));
            observed.push(workspace.key(b'r', &descriptor.id));
        }
        (parent_keys, candidate_keys, observed)
    };
    let snapshot = || -> Result<Vec<Option<Vec<u8>>>> {
        let workspace = WorkspaceStore::open(chain, params)?;
        observed_keys
            .iter()
            .map(|key| workspace.graph.get(key))
            .collect()
    };
    let before = snapshot()?;
    let positive = || -> Result<()> {
        state_records::without_materialization_for_test(|| {
            assert_round_scope_and_lock(chain, parent_id, genesis, params)?;
            let called = std::cell::Cell::new(false);
            let result = with_verified_finalized_successor_v1(
                chain,
                parent_id,
                candidate_id,
                genesis,
                params,
                |_| {
                    called.set(true);
                    assert_authority_lock(&path, true)?;
                    Ok(29)
                },
            )?;
            if !called.get() || result != 29 {
                bail!("real registered successor did not reach its read-only callback");
            }
            assert_authority_lock(&path, false)
        })
    };
    // Successful scopes must precede fault injection: a missing/unregistered
    // child must not make these parent-evidence negatives pass vacuously.
    positive().context("intact registered successor signing scopes")?;
    if snapshot()? != before {
        bail!("read-only signing callbacks mutated candidate or publication data");
    }
    for key in &parent_fault_keys {
        with_fixture_key_fault(chain, params, key, None, || {
            let faulted = snapshot()?;
            state_records::without_materialization_for_test(|| {
                assert_signing_scopes_reject(chain, parent_id, Some(candidate_id), genesis, params)
            })?;
            if snapshot()? != faulted {
                bail!("rejected source-evidence signing changed persisted fixture data");
            }
            Ok(())
        })
        .with_context(|| format!("source-evidence signing fault key={}", to_hex(key)))?;
    }
    // A missing reservation with an existing completion marker corrupts the
    // shared execution catalog, which every parent output read validates.
    // Missing child completion/chunk data does not corrupt that catalog: only
    // those local child faults must leave the valid parent round available.
    for (label, key, invalid_catalog) in &candidate_fault_keys {
        with_fixture_key_fault(chain, params, key, None, || {
            let faulted = snapshot()?;
            state_records::without_materialization_for_test(|| {
                if *invalid_catalog {
                    assert_signing_scopes_reject(
                        chain,
                        parent_id,
                        Some(candidate_id),
                        genesis,
                        params,
                    )
                } else {
                    assert_successor_scope_reject(chain, parent_id, candidate_id, genesis, params)?;
                    assert_round_scope_and_lock(chain, parent_id, genesis, params)
                }
            })?;
            if snapshot()? != faulted {
                bail!("rejected incomplete successor changed persisted fixture data");
            }
            Ok(())
        })
        .with_context(|| format!("{label} signing fault key={}", to_hex(key)))?;
    }
    positive().context("restored registered successor signing scopes")?;
    if snapshot()? != before {
        bail!("signing fixture did not restore its exact original evidence");
    }
    Ok(())
}
