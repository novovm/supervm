//! Same-call readback equivalence in the existing real third-block fixture.
//! The altered archives/snapshots below are in-memory negative inputs, not
//! claims of new disk-corruption coverage or permission to cache an archive.

use super::*;
use crate::native_block_ledger::FinalizedRecordArchiveV1;
use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1;

#[derive(Debug, PartialEq, Eq)]
struct ReadbackEvidence {
    input: Vec<u8>,
    input_chunks: Vec<Vec<u8>>,
    output: Vec<u8>,
    authority: Option<Vec<u8>>,
    parent_publication: Option<Vec<u8>>,
    publication: Option<Vec<u8>>,
    ready: Option<Vec<u8>>,
    output_descriptor: Option<Vec<u8>>,
}

fn evidence(
    workspace: &WorkspaceStore,
    input: &Descriptor,
    parent: [u8; 32],
) -> Result<ReadbackEvidence> {
    let input = ready_input(workspace, input.id)?;
    Ok(ReadbackEvidence {
        input: input.encode(),
        input_chunks: (0..input.len.div_ceil(CHUNK_BYTES))
            .map(|index| {
                workspace
                    .graph
                    .get(&workspace.chunk_key(&input.id, index))?
                    .context("readback fixture input chunk missing")
            })
            .collect::<Result<_>>()?,
        output: read_completed_output_bytes(workspace, input.id)?,
        authority: workspace.graph.get(&native_aoem_owned_state_head_key_v1(
            workspace.chain_id,
            &workspace.namespace,
        ))?,
        parent_publication: workspace.graph.get(&workspace.key(b'h', &parent))?,
        publication: workspace.graph.get(&workspace.key(b'h', &input.id))?,
        ready: workspace.graph.get(&workspace.key(b'r', &input.id))?,
        output_descriptor: workspace.graph.get(&workspace.key(b'v', &input.id))?,
    })
}

fn copied_archive(source: &FinalizedRecordArchiveV1) -> FinalizedRecordArchiveV1 {
    FinalizedRecordArchiveV1 {
        config: source.config.clone(),
        block: source.block.clone(),
        proof: source.proof.clone(),
        execution: source.execution.clone(),
        commitment: source.commitment,
    }
}

fn require_rejection<T>(result: Result<T>, label: &str) -> Result<()> {
    let error = result
        .err()
        .with_context(|| format!("{label} was accepted"))?;
    if format!("{error:#}").contains("unexpected full candidate store materialization") {
        bail!("{label} used forbidden cold fallback instead of rejecting: {error:#}");
    }
    Ok(())
}

pub(crate) fn exercise_publication_readback_for_test_v1(
    chain: u64,
    id: [u8; 32],
    params: &serde_json::Value,
    parent_archive: &FinalizedRecordArchiveV1,
) -> Result<()> {
    state_records::without_materialization_for_test(|| {
        // The exact same lock order and lifetime as publication: workspace,
        // authority, then ledger/source checks. No guard survives this call.
        let workspace = WorkspaceStore::open(chain, params)?;
        let path = resolve_native_execution_store_path_from_params_v1(params)
            .context("readback fixture requires explicit native path")?;
        let _authority = acquire_nov_native_execution_store_write_lock_v1(&path)?;
        let input = ready_input(&workspace, id)?;
        if input.version != DescriptorVersion::Ncw2 {
            bail!("readback fixture requires the actual NCW2 input");
        }
        let before = evidence(&workspace, &input, parent_archive.execution.workspace_id)?;
        if !state_records::is_delta_document(&before.output)? {
            bail!("readback fixture requires the actual V3 output");
        }
        let (original, original_count) =
            NovNativeBlockLedgerV1::count_fresh_ledger_verifications_for_test_v1(|| {
                block_artifact::load_block_artifact_inner_v1(&workspace, id, params)
            });
        let original = original?.context("original readback artifact missing")?;
        assert_eq!(
            original_count, 1,
            "original NCW2 readback validates parent history"
        );
        let (reused, reused_count) =
            NovNativeBlockLedgerV1::count_fresh_ledger_verifications_for_test_v1(|| {
                block_artifact::load_block_artifact_with_parent_archive_v1(
                    &workspace,
                    id,
                    params,
                    Some(parent_archive),
                )
            });
        assert_eq!(reused?.as_ref(), Some(&original));
        assert_eq!(
            reused_count, 0,
            "same-call archive replaces only the history lookup"
        );

        // A genuine archived block with valid QC is still not this child's
        // direct parent. In this fixture height one is the actual grandparent.
        if original.block().header.height != 3 || parent_archive.block.header.height != 2 {
            bail!("readback fixture requires the existing third-block lineage");
        }
        let grandparent = NovNativeBlockLedgerV1::load_fresh_finalized_archive_v1(
            &nov_native_block_ledger_rocksdb_path_v1(&path),
            parent_archive.config.compile()?.config_commitment(),
            parse_fixed_hex_32_v1(&workspace.namespace, "readback fixture namespace")?,
            1,
        )?;
        let (wrong_ancestor, count) =
            NovNativeBlockLedgerV1::count_fresh_ledger_verifications_for_test_v1(|| {
                block_artifact::load_block_artifact_with_parent_archive_v1(
                    &workspace,
                    id,
                    params,
                    Some(&grandparent),
                )
            });
        require_rejection(wrong_ancestor, "valid QC for the wrong ancestor")?;
        assert_eq!(
            count, 0,
            "wrong ancestor must not trigger a fallback lookup"
        );

        // Exercise the actual new artifact-readback entry point with mismatched
        // supplied archives, not a mock getter or metadata-only substitute.
        for field in 0..6 {
            let mut changed = copied_archive(parent_archive);
            match field {
                0 => changed.execution.workspace_id[0] ^= 1,
                1 => changed.execution.output_digest[0] ^= 1,
                2 => changed.commitment[0] ^= 1,
                3 => changed.block.header.post_state_root[0] ^= 1,
                4 => changed.block.header.cumulative_receipt_root[0] ^= 1,
                _ => {
                    let NovNativeSealRoundMessageV1::DecisionCertificateV3 { decision, .. } =
                        &mut changed.proof.witness
                    else {
                        bail!("readback fixture requires the real decision certificate");
                    };
                    decision.votes.clear();
                }
            }
            let (result, count) =
                NovNativeBlockLedgerV1::count_fresh_ledger_verifications_for_test_v1(|| {
                    block_artifact::load_block_artifact_with_parent_archive_v1(
                        &workspace,
                        id,
                        params,
                        Some(&changed),
                    )
                });
            require_rejection(result, "mismatched supplied parent archive")?;
            assert_eq!(
                count, 0,
                "bad supplied archive must not fall back to a ledger reload"
            );
        }

        let VerifiedInput::Light(payload) =
            workspace.read_input_with_parent_archive(&input, Some(parent_archive))?
        else {
            bail!("readback fixture unexpectedly decoded a cold input");
        };
        let reference = payload
            .record_state
            .as_ref()
            .context("NCW2 reference missing")?;
        let validate = |parent: &rooted_parent::RootedParentSnapshot,
                        reference: &state_records::StoreRef| {
            parent.validate_with_archive(
                &workspace,
                &payload.plan,
                reference,
                params,
                Some(parent_archive),
            )
        };
        // In-memory source/QC/root negatives use the exact parent validator
        // called by the new readback; the durable fixture is never rewritten.
        let mut changed = payload.finalized_parent.clone();
        let mut source: serde_json::Value = serde_json::from_str(changed.source_output.get())?;
        source["inline"]["expected_output_commitment"] = serde_json::json!("changed-source");
        changed.source_output =
            serde_json::value::RawValue::from_string(serde_json::to_string(&source)?)?;
        require_rejection(validate(&changed, reference), "changed parent source bytes")?;
        let mut changed = payload.finalized_parent.clone();
        let NovNativeSealRoundMessageV1::DecisionCertificateV3 { decision, .. } =
            &mut changed.proof.witness
        else {
            bail!("readback fixture parent lacks a decision certificate");
        };
        decision.votes.clear();
        require_rejection(validate(&changed, reference), "missing parent QC votes")?;
        for role in ["physical", "state", "receipts"] {
            let mut changed = serde_json::to_value(reference)?;
            let byte = changed["bundle"][role]["root"][0]
                .as_u64()
                .context("record root byte missing")?;
            changed["bundle"][role]["root"][0] = serde_json::json!(byte ^ 1);
            if role == "physical" {
                changed["root"][0] = serde_json::json!(byte ^ 1);
            }
            let changed: state_records::StoreRef = serde_json::from_value(changed)?;
            require_rejection(
                validate(&payload.finalized_parent, &changed),
                "changed parent root",
            )?;
        }
        validate(&payload.finalized_parent, reference)?;
        assert_eq!(
            evidence(&workspace, &input, parent_archive.execution.workspace_id)?,
            before,
            "readback rejection must not change input/output/publication evidence"
        );
        Ok(())
    })
}
