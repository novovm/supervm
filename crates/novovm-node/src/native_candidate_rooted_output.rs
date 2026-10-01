//! A checked current-block view, never a partially materialized full store.
//! Inputs remain cold-verified NCW1 parents. V3 output deltas are replayed only
//! as Merkle updates (not business execution), then checked against the exact
//! authenticated Transfer footprint. Legacy output/promotion keeps cold reads.

use super::super::auth::authenticate_record_plan;
use super::*;
use crate::native_state_records::RecordOverlayV1;
use crate::native_state_storage::AoemStateReaderV1;
use native_store_records::{NativeRecordAccessV1, RawPathChangeV1};
use native_transfer_record_execution::{ExecutionReader, RootedAccess};
use std::collections::BTreeSet;

pub(super) struct OutputView {
    pub(super) expected_output_commitment: String,
    pub(super) batch_result: novovm_exec::NovovmAoemNativeTxBatchResultV1,
    pub(super) receipts: BTreeMap<String, NovNativeExecutionReceiptV1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputMetadata {
    schema: String,
    workspace_id: [u8; 32],
    input_digest: [u8; 32],
    expected_output_commitment: String,
    batch_result: novovm_exec::NovovmAoemNativeTxBatchResultV1,
    // A null placeholder, not a Store with fabricated/default historical fields.
    store: (),
}

pub(super) fn authenticate_payload(
    payload: &Payload,
    workspace: &WorkspaceStore,
    params: &serde_json::Value,
) -> Result<Vec<AuthenticatedItem>> {
    if payload.root_codec_profile()? == NativeRootCodecProfileV1::RecordTreeV1 {
        if let Some((physical, state, receipts, _, _)) = payload
            .record_state
            .as_ref()
            .map(state_records::StoreRef::rooted_parts)
            .transpose()?
            .flatten()
        {
            if state != payload.plan.pre_state_root {
                bail!("record authentication parent root differs from verified input");
            }
            let base = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
            let reader = ExecutionReader::new(&base);
            return authenticate_record_plan(
                &payload.plan,
                &RootedAccess {
                    physical: &RecordOverlayV1::new(&reader, physical),
                    state: &RecordOverlayV1::new(&reader, state),
                    receipts: &RecordOverlayV1::new(&reader, receipts),
                },
                params,
            );
        }
    }
    authenticate_plan(&payload.plan, payload.parent_store()?, params)
}

pub(super) fn read_output_view(
    workspace: &WorkspaceStore,
    input: &Descriptor,
    descriptor: &OutputDescriptor,
    payload: &Payload,
    params: &serde_json::Value,
) -> Result<Option<OutputView>> {
    let Some(bytes) = read_output_bytes(workspace, input, descriptor)? else {
        return Ok(None);
    };
    if let Some(view) = try_delta_output_view(workspace, input, &bytes, payload, params)? {
        return Ok(Some(view));
    }
    // A legacy format deliberately takes the complete validation path. A bad
    // V3 witness never falls back here; decode_delta returns an error for it.
    let (output, _): (Output, _) = state_records::decode(workspace, &bytes, &["store"])?;
    validate_output(&output, payload, input, workspace, params)?;
    let mut all = output.store.receipts;
    let receipts = payload
        .plan
        .tx_hashes
        .iter()
        .map(|hash| {
            let hash = to_hex(hash);
            let receipt = all.remove(&hash).context("candidate receipt missing")?;
            Ok((hash, receipt))
        })
        .collect::<Result<_>>()?;
    Ok(Some(OutputView {
        expected_output_commitment: output.expected_output_commitment,
        batch_result: output.batch_result,
        receipts,
    }))
}

pub(super) fn try_delta_output_view(
    workspace: &WorkspaceStore,
    input: &Descriptor,
    bytes: &[u8],
    payload: &Payload,
    params: &serde_json::Value,
) -> Result<Option<OutputView>> {
    if !state_records::is_delta_document(bytes)? {
        return Ok(None);
    }
    let parent = payload
        .record_state
        .as_ref()
        .context("delta output requires a verified three-root parent document")?;
    let delta =
        state_records::decode_delta::<OutputMetadata>(workspace, bytes, &["store"], parent)?
            .context("delta output schema changed during decode")?;
    validate_delta_output(workspace, input, payload, params, delta).map(Some)
}

#[cfg(test)]
pub(crate) fn assert_delta_output_point_read_for_test_v1(
    chain: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<()> {
    let workspace = WorkspaceStore::open(chain, params)?;
    let input = ready_input(&workspace, id)?;
    let mut payload = workspace.read_payload(&input)?;
    let outputs = catalog(&workspace)?;
    let descriptor = &outputs
        .iter()
        .find(|(known, _)| *known == id)
        .context("delta point-read fixture output missing")?
        .1;
    let bytes = read_output_bytes(&workspace, &input, descriptor)?.context("output incomplete")?;
    if !state_records::is_delta_document(&bytes)? {
        bail!("point-read fixture requires a real delta output document");
    }
    // A valid-looking V3 output is never downgraded to cold legacy validation
    // merely because its caller omitted the three-root parent reference.
    let parent_reference = payload.record_state.take();
    if try_delta_output_view(&workspace, &input, &bytes, &payload, params).is_ok() {
        bail!("delta output without its parent reference was accepted");
    }
    payload.record_state = parent_reference;
    let view = state_records::without_materialization_for_test(|| {
        read_output_view(&workspace, &input, descriptor, &payload, params)?
            .context("delta point-read fixture incomplete")
    })?;
    let cold = read_output(&workspace, &input, descriptor, &payload, params)?
        .context("delta cold comparison output missing")?;
    if state_records::without_materialization_for_test(|| {
        read_output(&workspace, &input, descriptor, &payload, params)
    })
    .is_ok()
    {
        bail!("point-read test guard did not reject the explicit cold reader");
    }
    if view.batch_result != cold.batch_result
        || view.expected_output_commitment != cold.expected_output_commitment
    {
        bail!("delta point-read result differs from full cold output");
    }
    for (hash, receipt) in view.receipts {
        if cold.store.receipts.get(&hash) != Some(&receipt) {
            bail!("delta point-read receipt differs from full cold output");
        }
    }
    Ok(())
}

fn required<T: serde::de::DeserializeOwned>(
    access: &dyn NativeRecordAccessV1,
    path: &[&str],
) -> Result<T> {
    let raw = access
        .read_path(path)?
        .with_context(|| format!("rooted candidate output required record missing: {path:?}"))?;
    Ok(serde_json::from_slice(&raw)?)
}

fn validate_delta_output(
    workspace: &WorkspaceStore,
    input: &Descriptor,
    payload: &Payload,
    params: &serde_json::Value,
    delta: state_records::VerifiedDeltaDocument<OutputMetadata>,
) -> Result<OutputView> {
    let output = delta.inline;
    let () = output.store;
    if payload.root_codec_profile()? != NativeRootCodecProfileV1::RecordTreeV1
        || output.schema != OUTPUT_SCHEMA
        || output.workspace_id != input.id
        || output.input_digest != input.payload
    {
        bail!("rooted candidate output input/profile binding mismatch");
    }
    let backend = &output.batch_result.snapshot_metadata.backend;
    if backend.trim().is_empty() || backend.trim().eq_ignore_ascii_case("none") {
        bail!("candidate output requires a persistent backend");
    }
    let items = authenticate_payload(payload, workspace, params)?;
    if !items
        .iter()
        .all(|item| matches!(item.native_tx.kind, NovTxKindV1::Transfer(_)))
    {
        bail!("rooted candidate output witness requires an all-Transfer batch");
    }
    let (parent_physical, parent_state, parent_receipts, _, _) = payload
        .record_state
        .as_ref()
        .context("rooted output has no verified parent reference")?
        .rooted_parts()?
        .context("rooted output has no parent root bundle")?;
    let (physical, state, receipts, _, _) = delta
        .state
        .rooted_parts()?
        .context("rooted output has no output root bundle")?;
    let base = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
    let reader = ExecutionReader::new(&base);
    let parent_physical = RecordOverlayV1::new(&reader, parent_physical);
    let parent_state = RecordOverlayV1::new(&reader, parent_state);
    let parent_receipts = RecordOverlayV1::new(&reader, parent_receipts);
    let before = RootedAccess {
        physical: &parent_physical,
        state: &parent_state,
        receipts: &parent_receipts,
    };
    let batch_items: Vec<_> = items
        .iter()
        .map(|item| (&item.native_tx, &item.durable_auth_reservation))
        .collect();
    let footprint =
        native_transfer_state_access::TransferAccessV1::for_batch(&batch_items)?.load(&before)?;
    // This includes dynamically declared bounded trace eviction paths. It is
    // not enough that arbitrary changes happen to hash to the stated roots.
    footprint.validate_changes_v1(&delta.changes)?;

    let physical_tree = RecordOverlayV1::new(&reader, physical);
    let state_tree = RecordOverlayV1::new(&reader, state);
    let receipt_tree = RecordOverlayV1::new(&reader, receipts);
    let after = RootedAccess {
        physical: &physical_tree,
        state: &state_tree,
        receipts: &receipt_tree,
    };
    let expected_hashes: BTreeSet<_> = items.iter().map(|item| to_hex(&item.tx_hash)).collect();
    let changed_receipts: BTreeSet<_> = delta
        .changes
        .iter()
        .filter_map(|change| {
            let path = match change {
                RawPathChangeV1::Put { path, .. } | RawPathChangeV1::Delete { path } => path,
            };
            (path.len() == 2 && path[0] == "receipts").then(|| path[1].clone())
        })
        .collect();
    if changed_receipts != expected_hashes {
        bail!("rooted output must append exactly the authenticated batch receipts");
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
                .context("candidate nonce overflow")?,
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
        current_receipts.insert(hash, receipt);
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
    let sequence: u64 = required(&after, &["module_state", "aoem_semantic_ledger_sequence"])?;
    if sequence
        != payload
            .parent_store()?
            .module_state
            .aoem_semantic_ledger_sequence
            .checked_add(items.len() as u64)
            .context("candidate state version overflow")?
    {
        bail!("rooted candidate output version continuity mismatch");
    }
    // The footprint forbids mutation of these fields. Recheck both the current
    // runtime pin and their persisted values, keeping the old domain boundary.
    verify_native_business_protocol_config_v1(payload.parent_store()?)?;
    if required::<Option<u64>>(&after, &["authority_chain_id"])? != Some(workspace.chain_id)
        || required::<String>(&after, &["authority_namespace_digest"])? != workspace.namespace
        || required::<String>(&after, &["module_state", "protocol_config_commitment"])?
            != to_hex(&workspace.protocol)
        || required::<String>(
            &after,
            &["module_state", "native_auth_nonce_identity_scheme"],
        )? != NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2
    {
        bail!("rooted candidate output authority/protocol domain mismatch");
    }
    let batch = build_batch(payload, &items)?;
    let expected = build_result_from_commitments(
        &batch,
        &current_receipts,
        to_hex(&state),
        to_hex(&receipts),
        novovm_exec::NovovmAoemSnapshotMetadataV1 {
            snapshot_version: 2,
            state_version: sequence,
            backend: backend.clone(),
            persistence_owner: "aoem_runtime".into(),
        },
    )?;
    if output.expected_output_commitment != batch.expected_output_commitment
        || output.batch_result != expected
    {
        bail!("candidate execution result/root/receipt binding mismatch");
    }
    validate_precommit_receipts(payload, &items, &current_receipts)?;
    Ok(OutputView {
        expected_output_commitment: output.expected_output_commitment,
        batch_result: output.batch_result,
        receipts: current_receipts,
    })
}
