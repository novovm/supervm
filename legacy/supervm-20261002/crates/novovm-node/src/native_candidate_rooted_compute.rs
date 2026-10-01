//! First execution of a verified record-profile Transfer candidate. Only the
//! declared parent records and this batch's staged results are read. Metadata
//! plus immutable updates remain isolated; no authority head is published here.

use super::*;
use crate::native_state_records::RecordOverlayV1;
use crate::native_state_storage::AoemStateReaderV1;
use native_store_records::NativeRecordAccessV1;
use native_transfer_record_execution::{ExecutionReader, RootedAccess, UpdatedReaderV1};
use serde::de::DeserializeOwned;

#[cfg(test)]
std::thread_local! {
    static COMPUTE_OBSERVATION: std::cell::Cell<Option<([u8; 32], usize, usize)>> = const {
        std::cell::Cell::new(None)
    };
}

/// Candidate identity, transaction count and actual callback overlap observed
/// by first-compute. Transaction count is NOT the number of AOEM component tasks.
/// Thread-local isolation keeps unrelated concurrent tests out of evidence.
#[cfg(test)]
pub(super) fn take_observation_for_test() -> Option<([u8; 32], usize, usize)> {
    COMPUTE_OBSERVATION.with(std::cell::Cell::take)
}

pub(super) struct ComputedDeltaOutput {
    pub(super) metadata: rooted_output::OutputMetadata,
    pub(super) updates: state_records::RecordTreeUpdatesV1,
}

impl ComputedDeltaOutput {
    /// Explicit old-format/capacity compatibility only. Reconstruct from the
    /// SAME computed physical update; never submit AOEM or execute business a
    /// second time. The caller still performs the complete old output checks.
    pub(super) fn into_cold(self, workspace: &WorkspaceStore) -> Result<Output> {
        let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
        let store = native_transfer_record_execution::materialize_update_v1(
            &reader,
            &self.updates.physical,
            self.updates.records,
            self.updates.blob_bytes,
        )?;
        Ok(Output {
            schema: self.metadata.schema,
            workspace_id: self.metadata.workspace_id,
            input_digest: self.metadata.input_digest,
            expected_output_commitment: self.metadata.expected_output_commitment,
            batch_result: self.metadata.batch_result,
            store,
            record_state: None,
            record_updates: Some(self.updates),
        })
    }
}

fn required<T: DeserializeOwned>(access: &dyn NativeRecordAccessV1, path: &[&str]) -> Result<T> {
    let raw = access.read_path(path)?.with_context(|| {
        format!("rooted candidate computation required record missing: {path:?}")
    })?;
    serde_json::from_slice(&raw)
        .with_context(|| format!("rooted candidate computation record type mismatch: {path:?}"))
}

fn validate_domain(
    access: &dyn NativeRecordAccessV1,
    workspace: &WorkspaceStore,
    plan: &NovNativeCandidateExecutionPlanV1,
) -> Result<()> {
    if plan.context.chain_id != workspace.chain_id
        || plan.protocol_config_commitment != workspace.protocol
        || verify_required_native_business_protocol_config_pin_v1()? != to_hex(&workspace.protocol)
        || required::<Option<u64>>(access, &["authority_chain_id"])? != Some(workspace.chain_id)
        || required::<String>(access, &["authority_namespace_digest"])? != workspace.namespace
        || required::<String>(access, &["module_state", "protocol_config_commitment"])?
            != to_hex(&workspace.protocol)
        || required::<String>(
            access,
            &["module_state", "native_auth_nonce_identity_scheme"],
        )? != NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2
    {
        bail!("rooted candidate computation authority/protocol/nonce domain mismatch");
    }
    Ok(())
}

/// Eligibility is checked again here: a bad record input or execution error is
/// never a signal to try another format. The caller authenticates provenance
/// before handing out ExecutionInput; this function authenticates all signers
/// and nonce transitions before any AOEM submission.
pub(super) fn compute(
    payload: &impl ExecutionInput,
    input: &Descriptor,
    workspace: &WorkspaceStore,
    params: &serde_json::Value,
) -> Result<ComputedDeltaOutput> {
    if payload.root_codec_profile()? != NativeRootCodecProfileV1::RecordTreeV1 {
        bail!("rooted candidate computation requires the record-tree profile");
    }
    let plan = payload.plan();
    plan.validate()?;
    let (physical_root, state_root, receipt_root, records, blob_bytes) = payload
        .record_state()
        .context("rooted candidate computation parent reference missing")?
        .rooted_parts()?
        .context("rooted candidate computation requires all three parent roots")?;
    if state_root != plan.pre_state_root
        || input.plan != plan.plan_commitment
        || input.parent_state != plan.pre_state_root
        || input.parent_block != plan.context.parent_block_hash
        || input.id != workspace_id(&workspace.scope, &plan.plan_commitment)
    {
        bail!("rooted candidate computation plan/input/parent binding mismatch");
    }
    let items = authenticate_payload(payload, workspace, params)?;
    if items.is_empty()
        || !items
            .iter()
            .all(|item| matches!(item.native_tx.kind, NovTxKindV1::Transfer(_)))
    {
        bail!("rooted candidate computation requires a nonempty all-Transfer batch");
    }

    let base = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
    let reader = ExecutionReader::new(&base);
    let parent_sequence = payload.parent_sequence()?;
    {
        let physical = RecordOverlayV1::new(&reader, physical_root);
        let state = RecordOverlayV1::new(&reader, state_root);
        let receipts = RecordOverlayV1::new(&reader, receipt_root);
        let before = RootedAccess {
            physical: &physical,
            state: &state,
            receipts: &receipts,
        };
        validate_domain(&before, workspace, plan)?;
        if required::<u64>(&before, &["module_state", "aoem_semantic_ledger_sequence"])?
            != parent_sequence
        {
            bail!("rooted candidate parent sequence differs from its authenticated metadata");
        }
    }
    let expected_sequence = parent_sequence
        .checked_add(u64::try_from(items.len())?)
        .context("rooted candidate state version overflow")?;
    let batch = build_batch(payload, &items)?;
    if !probe_semantic_graph_v3_capability_v1()?.ready {
        bail!("candidate execution requires AOEM semantic graph V3");
    }
    // Exactly the old canonical raw-wire precommit and item/chunk coordinates.
    // These auxiliary puts do not publish NOV authority or consume its nonce.
    let chunk_size = NOV_NATIVE_AOEM_CONSENSUS_BATCH_CHUNK_SIZE_V1;
    let (aggregate, chunks) =
        execute_native_raw_tx_batch_chunks_via_aoem_semantic_ingress_v1(&plan.raw_txs, chunk_size)?;
    if !aggregate.submitted
        || aggregate.processed_ops as usize != items.len()
        || aggregate.success_ops as usize != items.len()
    {
        bail!("candidate AOEM precommit did not complete the full authenticated batch");
    }
    let transfer_items = items
        .iter()
        .enumerate()
        .map(|(index, item)| -> Result<_> {
            let chunk = chunks
                .get(index / chunk_size)
                .context("rooted candidate AOEM precommit chunk missing")?;
            Ok(native_transfer_dispatch::Item {
                transaction: &item.native_tx,
                request: &item.execution_request,
                subject: &item.execution_subject,
                reservation: &item.durable_auth_reservation,
                ingress: native_aoem_batch_item_ingress_meta_v1(chunk, index, items.len()),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let update = native_transfer_record_execution::execute_rooted_segment_v1(
        &reader,
        physical_root,
        state_root,
        receipt_root,
        &transfer_items,
        u128::from(plan.context.timestamp_unix_ms),
    )?;
    let (records, blob_bytes) = update.stats.checked_apply(records, blob_bytes)?;

    // Read each role through its own staged+inherited immutable reader. A
    // physical receipt alone is insufficient: RootedAccess also checks its
    // cumulative receipt commitment and every consensus-bearing scalar.
    let physical_reader = UpdatedReaderV1 {
        reader: &reader,
        update: &update.physical,
    };
    let state_reader = UpdatedReaderV1 {
        reader: &reader,
        update: &update.state,
    };
    let receipt_reader = UpdatedReaderV1 {
        reader: &reader,
        update: &update.receipts,
    };
    let physical = RecordOverlayV1::new(&physical_reader, update.physical.root());
    let state = RecordOverlayV1::new(&state_reader, update.state.root());
    let receipts = RecordOverlayV1::new(&receipt_reader, update.receipts.root());
    let after = RootedAccess {
        physical: &physical,
        state: &state,
        receipts: &receipts,
    };
    validate_domain(&after, workspace, plan)?;
    let sequence = required::<u64>(&after, &["module_state", "aoem_semantic_ledger_sequence"])?;
    if sequence != expected_sequence {
        bail!("rooted candidate output state version continuity mismatch");
    }
    let mut expected_nonces = BTreeMap::new();
    let mut current_receipts = BTreeMap::new();
    for item in &items {
        let reservation = &item.durable_auth_reservation;
        expected_nonces.insert(
            &reservation.identity_key,
            reservation
                .nonce
                .checked_add(1)
                .context("rooted candidate nonce overflow")?,
        );
        if required::<String>(
            &after,
            &[
                "module_state",
                "native_auth_nonce_reservations",
                &reservation.ledger_key,
            ],
        )? != reservation.reservation_id
        {
            bail!("rooted candidate output reservation continuity mismatch");
        }
        let hash = to_hex(&item.tx_hash);
        let receipt: NovNativeExecutionReceiptV1 = required(&after, &["receipts", &hash])?;
        if receipt.tx_hash != hash || current_receipts.insert(hash, receipt).is_some() {
            bail!("rooted candidate current receipt identity mismatch");
        }
    }
    for (identity, nonce) in expected_nonces {
        if required::<u64>(
            &after,
            &["module_state", "native_auth_next_nonces", identity],
        )? != nonce
        {
            bail!("rooted candidate output nonce continuity mismatch");
        }
    }
    validate_precommit_receipts(payload, &items, &current_receipts)?;
    let batch_result = build_result_from_commitments(
        &batch,
        &current_receipts,
        to_hex(&update.state.root()),
        to_hex(&update.receipts.root()),
        novovm_exec::NovovmAoemSnapshotMetadataV1 {
            snapshot_version: 2,
            state_version: sequence,
            backend: native_aoem_owned_runtime_config_v1()?.persist_backend,
            persistence_owner: "aoem_runtime".into(),
        },
    )?;
    let backend = &batch_result.snapshot_metadata.backend;
    if backend.trim().is_empty() || backend.trim().eq_ignore_ascii_case("none") {
        bail!("candidate output requires a persistent backend");
    }
    #[cfg(test)]
    COMPUTE_OBSERVATION.with(|observation| {
        observation.set(Some((input.id, items.len(), update.peak_inflight)));
    });
    Ok(ComputedDeltaOutput {
        metadata: rooted_output::OutputMetadata {
            schema: OUTPUT_SCHEMA.into(),
            workspace_id: input.id,
            input_digest: input.payload,
            expected_output_commitment: batch.expected_output_commitment,
            batch_result,
            store: (),
        },
        updates: state_records::RecordTreeUpdatesV1 {
            physical: update.physical,
            state: update.state,
            receipts: update.receipts,
            records,
            blob_bytes,
            changes: Some(update.changes),
        },
    })
}
