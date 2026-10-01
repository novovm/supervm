//! Real NCW2 fixtures called from the existing finalized-parent scenario.
//! Cold references are explicit test oracles, never a production fallback.

use super::*;

pub(crate) fn exercise_light_first_compute_for_test_v1(
    chain: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<ExecutionInfoV1> {
    let expected = {
        let workspace = WorkspaceStore::open(chain, params)?;
        let input = ready_input(&workspace, id)?;
        if input.version != DescriptorVersion::Ncw2
            || catalog(&workspace)?.iter().any(|(known, _)| *known == id)
        {
            bail!("first-compute fixture needs NCW2 with no output reservation");
        }
        let (computed, prepared) = state_records::without_materialization_for_test(|| {
            let verified = workspace.read_input(&input)?;
            let computed = rooted_compute::compute(&verified, &input, &workspace, params)?;
            let prepared = state_records::prepare_delta(
                &workspace,
                &computed.metadata,
                &["store"],
                verified
                    .record_state()
                    .context("first-compute fixture parent reference missing")?,
                &computed.updates,
            )?;
            Ok((computed, prepared))
        })?;
        let reader =
            crate::native_state_storage::AoemStateReaderV1::new(&workspace.graph, workspace.scope);
        let forbidden = state_records::without_materialization_for_test(|| {
            native_transfer_record_execution::materialize_update_v1(
                &reader,
                &computed.updates.physical,
                computed.updates.records,
                computed.updates.blob_bytes,
            )
        })
        .err()
        .context("first-compute guard did not reject the explicit cold bridge")?;
        if !format!("{forbidden:#}").contains("unexpected full candidate store materialization") {
            bail!("cold bridge failed for a reason other than the guard: {forbidden:#}");
        }
        // Compare two serializers of this exact computation, not two runs with
        // possibly different local ingress observations. The full image is a
        // deliberate cold test oracle and remains outside the guarded region.
        let mut cold = computed.into_cold(&workspace)?;
        let cold_input = workspace.read_payload(&input)?;
        validate_output(&cold, &cold_input, &input, &workspace, params)?;
        let updates = cold.record_updates.take();
        let parent_store = cold_input.parent_store()?;
        let old = state_records::prepare_record_profile(
            &workspace,
            &cold,
            &["store"],
            &cold.store,
            cold_input
                .record_state
                .as_ref()
                .map(|reference| (reference, parent_store)),
            updates,
        )?;
        if prepared.bytes != old.bytes {
            bail!("rooted first-compute V3 bytes differ from the old cold writer");
        }
        cold.batch_result
    }; // Release the workspace lock before exercising the public first call.

    // This public call has no existing output; it must authenticate, compute,
    // prepare, persist and read back without materializing any full Store.
    // The earlier extra computation is only a serialization test oracle.
    let _ = rooted_compute::take_observation_for_test();
    let actual = state_records::without_materialization_for_test(|| execute_v1(chain, id, params))?;
    let (observed_id, task_count, peak_inflight) = rooted_compute::take_observation_for_test()
        .context("public first-compute did not record its real AOEM task observation")?;
    if observed_id != id
        || task_count != 5
        || task_count != actual.batch_result.per_tx_receipts.len()
        || peak_inflight == 0
        || peak_inflight > task_count
    {
        bail!("public first-compute AOEM task observation does not match its candidate");
    }
    eprintln!(
        "guarded NCW2 public first-compute tasks={task_count} peak_inflight={peak_inflight} full_materialization=false"
    );
    if actual.batch_result != expected {
        bail!("guarded public first-compute result differs from the cold writer reference");
    }
    Ok(actual)
}

pub(crate) fn exercise_light_output_recovery_for_test_v1(
    chain: u64,
    source_id: [u8; 32],
    params: &serde_json::Value,
) -> Result<()> {
    use ExecutionCheckpointV1 as Stage;
    let (template, reference, authority, source_descriptor) = {
        let workspace = WorkspaceStore::open(chain, params)?;
        let descriptor = ready_input(&workspace, source_id)?;
        let VerifiedInput::Light(light) = workspace.read_input(&descriptor)? else {
            bail!("light output recovery fixture requires NCW2");
        };
        if workspace.catalog()?.len().saturating_add(16) > MAX_WORKSPACES_V1 {
            bail!("light output recovery fixture would exhaust workspace slots");
        }
        (
            serde_json::to_vec(&light)?,
            light.record_state.clone(),
            workspace.graph.get(&native_aoem_owned_state_head_key_v1(
                chain,
                &workspace.namespace,
            ))?,
            descriptor,
        )
    };
    // 0 = inline, 1 = physical-only records, 2 = old three-root records,
    // 3 = current delta records. Only V3 claims a guarded light recovery.
    for version in [3u8, 2, 1, 0] {
        for (index, stage) in [
            Stage::OutputReserved,
            Stage::PartialOutput,
            Stage::OutputWritten,
            Stage::Completed,
        ]
        .into_iter()
        .enumerate()
        {
            let id = state_records::without_materialization_for_test(|| {
                let mut light: LightPayload = serde_json::from_slice(&template)?;
                light.record_state = reference.clone();
                let old = &light.plan;
                let mut context = old.context;
                context.slot = context
                    .slot
                    .checked_add(200 + u64::from(version) * 4 + index as u64)
                    .context("light recovery fixture slot overflow")?;
                light.plan = NovNativeCandidateExecutionPlanV1::new(
                    context,
                    old.protocol_config_commitment,
                    old.pre_state_root,
                    old.aoem_parent.clone(),
                    old.tx_hashes.clone(),
                    old.raw_txs.clone(),
                )?;
                let mut workspace = WorkspaceStore::open(chain, params)?;
                let info = super::super::stage_light_payload(&mut workspace, &light, |_| Ok(()))?
                    .context("small recovery fixture unexpectedly requested cold input")?;
                if catalog(&workspace)?
                    .iter()
                    .any(|(known, _)| *known == info.workspace_id)
                {
                    bail!("output recovery fixture context was already used");
                }
                Ok(info.workspace_id)
            })?;
            let reserved_digest = if version == 3 {
                let reached = std::cell::Cell::new(false);
                let error = state_records::without_materialization_for_test(|| {
                    execute_with_checkpoint_v1(chain, id, params, |point| {
                        if point == stage {
                            reached.set(true);
                            bail!("light output fixture deliberate crash");
                        }
                        Ok(())
                    })
                })
                .err()
                .context("light output crash fixture unexpectedly completed")?;
                if !reached.get()
                    || !format!("{error:#}").contains("light output fixture deliberate crash")
                {
                    bail!("light output failed before its intended checkpoint: {error:#}");
                }
                None
            } else {
                Some(seed_legacy_output_for_test_v1(
                    chain, id, params, stage, false, version,
                )?)
            };
            let original_descriptor = {
                let workspace = WorkspaceStore::open(chain, params)?;
                let descriptor = catalog(&workspace)?
                    .into_iter()
                    .find(|(known, _)| *known == id)
                    .context("interrupted fixture output reservation missing")?
                    .1;
                if reserved_digest.is_some_and(|digest| digest != descriptor.digest) {
                    bail!("legacy fixture changed its initial reservation digest");
                }
                descriptor
            };
            let lookup = || load_execution_v1(chain, id, params);
            let before = if version == 3 {
                state_records::without_materialization_for_test(lookup)?
            } else {
                lookup()?
            };
            if before.is_some() != (stage == Stage::Completed) {
                bail!("interrupted output completion state changed");
            }
            reset_native_aoem_semantic_ingress_session_v1();
            let checkpoints = std::cell::RefCell::new(Vec::new());
            let recover = || {
                execute_with_checkpoint_v1(chain, id, params, |point| {
                    if matches!(stage, Stage::OutputWritten | Stage::Completed)
                        && point != Stage::Completed
                    {
                        bail!("fully written fixture must not recompute business");
                    }
                    checkpoints.borrow_mut().push(point);
                    Ok(())
                })
            };
            let recovered = if version == 3 {
                state_records::without_materialization_for_test(recover)?
            } else {
                recover()?
            };
            let expected_checkpoints = match stage {
                Stage::OutputWritten => vec![Stage::Completed],
                Stage::Completed => vec![],
                _ => vec![
                    Stage::OutputReserved,
                    Stage::PartialOutput,
                    Stage::OutputWritten,
                    Stage::Completed,
                ],
            };
            if *checkpoints.borrow() != expected_checkpoints
                || recovered.output_digest != original_descriptor.digest
            {
                bail!("light/legacy output recovery changed its checkpoints or reserved bytes");
            }
            let replay = || {
                execute_with_checkpoint_v1(chain, id, params, |_| {
                    bail!("completed output must not execute or publish twice")
                })
            };
            let replayed = if version == 3 {
                state_records::without_materialization_for_test(replay)?
            } else {
                replay()?
            };
            if replayed != recovered {
                bail!("completed output replay differs from recovered execution");
            }
            // Full export is explicitly outside the light boundary. It must
            // recover the same roots from every version's actual durable data.
            let cold = load_typed_execution_snapshot_for_test_v1(chain, id, params)?;
            if recovered.post_state_root
                != to_hex(&native_record_commitment::consensus_state_root_v1(
                    &cold.module_state,
                )?)
                || recovered.receipt_root
                    != to_hex(&native_record_commitment::cumulative_receipt_root_v1(
                        &cold,
                    )?)
            {
                bail!("recovered roots differ from the complete cold store");
            }
            let workspace = WorkspaceStore::open(chain, params)?;
            let current = catalog(&workspace)?
                .into_iter()
                .find(|(known, _)| *known == id)
                .context("recovered output reservation missing")?
                .1;
            if current.encode() != original_descriptor.encode()
                || ready_input(&workspace, source_id)? != source_descriptor
                || workspace.graph.get(&native_aoem_owned_state_head_key_v1(
                    chain,
                    &workspace.namespace,
                ))? != authority
            {
                bail!("output recovery changed reserved bytes, source input or authority");
            }
        }
    }
    Ok(())
}
