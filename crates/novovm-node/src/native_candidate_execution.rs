#![forbid(unsafe_code)]

//! Isolated computation against a copied parent: AOEM schedules NOV transfers;
//! the Host orders fee settlement and serial Execute barriers. AOEM also owns
//! generic precommit and durable output. This module does not publish authority.

use super::auth::{authenticate_plan, AuthenticatedItem};
use super::*;
use crate::native_root_codecs::NativeRootCodecProfileV1;

#[path = "native_candidate_block_artifact.rs"]
mod block_artifact;
#[path = "native_candidate_promotion.rs"]
mod promotion;
#[path = "native_candidate_rooted_output.rs"]
mod rooted_output;
pub use block_artifact::{
    load_block_artifact_v1, prepare_genesis_promotion_v1, register_block_candidate_v1,
    register_genesis_block_candidate_v1, with_verified_block_candidate_v1,
    with_verified_genesis_block_candidate_v1, IsolatedBlockArtifactV1,
};
pub(super) use promotion::capture_finalized_parent_locked;
pub(super) use promotion::publication_target_fields;
pub use promotion::{
    complete_genesis_promotion_v1, complete_successor_ledger_v1, finalize_genesis_promotion_v1,
    finalize_successor_v1, load_finalized_genesis_parent_v1, load_latest_finalized_parent_v1,
    prepare_successor_promotion_v1, publish_genesis_promotion_v1, publish_successor_authority_v1,
    register_finalized_successor_v1, resume_genesis_promotion_v1, resume_successor_promotion_v1,
    retire_old_workspaces_v1, verify_genesis_promotion_v1, verify_successor_authority_v1,
    with_verified_finalized_parent_round_v1, with_verified_finalized_successor_v1,
    FinalizedGenesisParentV1, FreshSuccessorPublicationV1, GenesisPromotionPublicationV1,
    WorkspaceRetirementV1,
};
#[cfg(test)]
pub(crate) use promotion::{
    complete_successor_with_checkpoint_v1, complete_with_checkpoint_v1,
    finalize_successor_with_checkpoint_v1, publish_successor_with_checkpoint_v1,
    publish_with_checkpoint_v1, retire_with_checkpoint_v1, PromotionCheckpointV1,
    RetirementCheckpointV1,
};
pub(crate) use promotion::{load_startup_artifact_v1, load_startup_successor_v1};
#[cfg(test)]
pub(crate) use rooted_output::{
    assert_delta_output_point_read_for_test_v1, assert_light_input_output_point_read_for_test_v1,
};
use rooted_output::{authenticate_payload, read_output_view, OutputView};

pub(super) const OUTPUT_SCHEMA: &str = "novovm-native-candidate-execution/v1";
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_TOTAL_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

// Only verified input variants implement this internal view. A rooted parent
// exposes metadata and point reads, never a fabricated partial Store.
trait ExecutionInput {
    fn plan(&self) -> &NovNativeCandidateExecutionPlanV1;
    fn root_codec_profile(&self) -> Result<NativeRootCodecProfileV1>;
    fn record_state(&self) -> Option<&state_records::StoreRef>;
    fn parent_sequence(&self) -> Result<u64>;
    fn cold_payload(&self) -> Option<&Payload>;
}

impl ExecutionInput for Payload {
    fn plan(&self) -> &NovNativeCandidateExecutionPlanV1 {
        &self.plan
    }
    fn root_codec_profile(&self) -> Result<NativeRootCodecProfileV1> {
        Payload::root_codec_profile(self)
    }
    fn record_state(&self) -> Option<&state_records::StoreRef> {
        self.record_state.as_ref()
    }
    fn parent_sequence(&self) -> Result<u64> {
        Ok(self
            .parent_store()?
            .module_state
            .aoem_semantic_ledger_sequence)
    }
    fn cold_payload(&self) -> Option<&Payload> {
        Some(self)
    }
}

impl ExecutionInput for VerifiedInput {
    fn plan(&self) -> &NovNativeCandidateExecutionPlanV1 {
        match self {
            Self::Cold(p) => &p.plan,
            Self::Light(p) => &p.plan,
        }
    }
    fn root_codec_profile(&self) -> Result<NativeRootCodecProfileV1> {
        match self {
            Self::Cold(p) => p.root_codec_profile(),
            Self::Light(p) => p.finalized_parent.config.root_codec_profile(),
        }
    }
    fn record_state(&self) -> Option<&state_records::StoreRef> {
        match self {
            Self::Cold(p) => p.record_state.as_ref(),
            Self::Light(p) => p.record_state.as_ref(),
        }
    }
    fn parent_sequence(&self) -> Result<u64> {
        match self {
            Self::Cold(p) => p.parent_sequence(),
            Self::Light(p) => Ok(p.finalized_parent.block.header.state_version),
        }
    }
    fn cold_payload(&self) -> Option<&Payload> {
        match self {
            Self::Cold(p) => Some(p),
            Self::Light(_) => None,
        }
    }
}

fn check_output_capacity(reserved: usize, len: usize) -> Result<()> {
    if len == 0 || len > MAX_OUTPUT_BYTES {
        bail!("candidate output exceeds 8 MiB");
    }
    if reserved
        .checked_add(len)
        .context("candidate output capacity overflow")?
        > MAX_TOTAL_OUTPUT_BYTES
    {
        bail!("candidate output aggregate capacity exhausted");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExecutionInfoV1 {
    pub schema: &'static str,
    pub workspace_id: [u8; 32],
    pub plan_commitment: [u8; 32],
    pub output_digest: [u8; 32],
    pub post_state_root: String,
    pub receipt_root: String,
    pub execution_evidence_commitment: String,
    pub batch_result: novovm_exec::NovovmAoemNativeTxBatchResultV1,
    pub transactions_authenticated: bool,
    pub aoem_called: bool,
    pub execution_completed: bool,
    pub candidate_state_persisted: bool,
    pub business_transition_computation_owner: &'static str,
    pub persistence_owner: &'static str,
    pub authority_state_published: bool,
    pub chain_canonical: bool,
    pub proof_sealed: bool,
    pub safe: bool,
    pub finalized: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Output {
    schema: String,
    workspace_id: [u8; 32],
    input_digest: [u8; 32],
    expected_output_commitment: String,
    batch_result: novovm_exec::NovovmAoemNativeTxBatchResultV1,
    store: NovNativeExecutionStoreV1,
    #[serde(skip)]
    record_state: Option<state_records::StoreRef>,
    #[serde(skip)]
    record_updates: Option<state_records::RecordTreeUpdatesV1>,
}

#[derive(Clone, PartialEq, Eq)]
struct OutputDescriptor {
    len: usize,
    digest: [u8; 32],
    input_digest: [u8; 32],
}

impl OutputDescriptor {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = b"NCE1".to_vec();
        bytes.extend_from_slice(&(self.len as u64).to_be_bytes());
        bytes.extend_from_slice(&self.digest);
        bytes.extend_from_slice(&self.input_digest);
        bytes
    }

    fn decode(bytes: &[u8], input: &Descriptor) -> Result<Self> {
        if bytes.len() != 76 || &bytes[..4] != b"NCE1" {
            bail!("invalid candidate output descriptor codec");
        }
        let descriptor = Self {
            len: usize::try_from(u64::from_be_bytes(bytes[4..12].try_into()?))?,
            digest: bytes[12..44].try_into()?,
            input_digest: bytes[44..76].try_into()?,
        };
        if descriptor.len == 0
            || descriptor.len > MAX_OUTPUT_BYTES
            || descriptor.input_digest != input.payload
        {
            bail!("candidate output descriptor bounds or input mismatch");
        }
        Ok(descriptor)
    }
}

pub(super) fn output_digest(bytes: &[u8]) -> [u8; 32] {
    sha256_bytes_v1(&[b"novovm-candidate-output-v1\0", bytes])
}

fn output_chunk_key(workspace: &WorkspaceStore, id: &[u8; 32], index: usize) -> Vec<u8> {
    let mut suffix = id.to_vec();
    suffix.extend_from_slice(&(index as u32).to_be_bytes());
    workspace.key(b'o', &suffix)
}

fn completion(
    workspace: &WorkspaceStore,
    input: &Descriptor,
    output: &OutputDescriptor,
) -> Vec<u8> {
    sha256_bytes_v1(&[
        b"novovm-candidate-execution-complete-v1\0",
        &workspace.scope,
        &input.id,
        &output.encode(),
    ])
    .to_vec()
}

fn catalog(workspace: &WorkspaceStore) -> Result<Vec<([u8; 32], OutputDescriptor)>> {
    let mut outputs = Vec::new();
    let mut total = 0usize;
    for (slot, input) in workspace.catalog()? {
        if workspace.status(slot, &input)? == WorkspaceStatusV1::Retiring {
            continue;
        }
        if let Some(raw) = workspace.graph.get(&workspace.key(b'v', &input.id))? {
            let descriptor = OutputDescriptor::decode(&raw, &input)?;
            total = total
                .checked_add(descriptor.len)
                .context("candidate output size overflow")?;
            if total > MAX_TOTAL_OUTPUT_BYTES {
                bail!("candidate output catalog exceeds aggregate capacity");
            }
            outputs.push((input.id, descriptor));
        } else if workspace
            .graph
            .get(&workspace.key(b'e', &input.id))?
            .is_some()
        {
            bail!("candidate execution completion has no output reservation");
        }
    }
    Ok(outputs)
}

fn ready_input(workspace: &WorkspaceStore, id: [u8; 32]) -> Result<Descriptor> {
    let (slot, input) = workspace
        .catalog()?
        .into_iter()
        .find(|(_, input)| input.id == id)
        .context("candidate workspace not found")?;
    if workspace.status(slot, &input)? != WorkspaceStatusV1::Ready {
        bail!("candidate execution requires a ready, non-aborted input workspace");
    }
    Ok(input)
}

fn build_batch(
    payload: &impl ExecutionInput,
    items: &[AuthenticatedItem],
) -> Result<novovm_exec::NovovmAoemNativeTxBatchV1> {
    let plan = payload.plan();
    let mut batch_items = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let parameter_payload = native_tx_parameter_payload_v1(
            &item.native_tx,
            &item.ir,
            Some(&item.execution_request),
            Some(&item.execution_subject),
            &plan.raw_txs[index],
        );
        let sequence = index as u64;
        let tx_hash = to_hex_prefixed_v1(&item.tx_hash);
        let sender_identity = native_tx_sender_identity_v1(&item.native_tx);
        let signer_identity = Some(format!(
            "signature:{}",
            to_hex_prefixed_v1(&item.native_tx.signature)
        ));
        let nonce = native_tx_nonce_v1(&item.native_tx);
        let intent_type = native_tx_kind_label_v1(&item.native_tx).to_string();
        let semantic_operator = native_tx_semantic_operator_v1(&item.native_tx).to_string();
        let canonical_rebuild_commitment = novovm_exec::native_tx_batch_v1_item_commitment(
            novovm_exec::NativeTxBatchV1ItemCommitmentInputV1 {
                sequence,
                tx_hash: &tx_hash,
                sender_identity: &sender_identity,
                signer_identity: signer_identity.as_deref(),
                nonce,
                intent_type: &intent_type,
                semantic_operator: &semantic_operator,
                parameter_payload: &parameter_payload,
            },
        );
        batch_items.push(novovm_exec::NovovmAoemNativeTxBatchItemV1 {
            sequence,
            tx_hash,
            sender_identity,
            signer_identity,
            nonce,
            intent_type,
            semantic_operator,
            parameter_payload,
            canonical_rebuild_commitment,
        });
    }
    let context_commitment = plan.context.commitment()?;
    let batch_id = match payload.root_codec_profile()? {
        NativeRootCodecProfileV1::LegacyWireV1 => deterministic_native_aoem_batch_id_v1(
            plan.context.chain_id,
            payload
                .cold_payload()
                .context("legacy batch requires a complete parent")?
                .parent_store()?,
            &batch_items,
            Some(&context_commitment),
        ),
        NativeRootCodecProfileV1::RecordTreeV1 => {
            use sha2::{Digest, Sha256};
            let mut hash = Sha256::new();
            hash.update(b"novovm-native-record-aoem-batch-id-v1\0");
            hash.update(plan.context.chain_id.to_be_bytes());
            hash.update(plan.pre_state_root);
            hash.update(payload.parent_sequence()?.to_be_bytes());
            hash.update(context_commitment);
            hash.update((batch_items.len() as u64).to_be_bytes());
            for item in &batch_items {
                hash.update(item.sequence.to_be_bytes());
                hash.update((item.canonical_rebuild_commitment.len() as u64).to_be_bytes());
                hash.update(item.canonical_rebuild_commitment.as_bytes());
            }
            format!("novovm-record-aoem-owned-v1-{}", to_hex(&hash.finalize()))
        }
    };
    novovm_exec::build_native_tx_batch_v1(
        batch_id,
        plan.context.chain_id,
        Some(plan.context.block_height),
        batch_items,
    )
}

fn build_result(
    batch: &novovm_exec::NovovmAoemNativeTxBatchV1,
    store: &NovNativeExecutionStoreV1,
    backend: String,
    profile: NativeRootCodecProfileV1,
) -> Result<novovm_exec::NovovmAoemNativeTxBatchResultV1> {
    let (state_root, receipt_root, snapshot_version) = match profile {
        NativeRootCodecProfileV1::LegacyWireV1 => (
            native_semantic_ledger_state_digest_v1(&store.module_state),
            native_execution_receipt_root_v2(store)?,
            1,
        ),
        NativeRootCodecProfileV1::RecordTreeV1 => (
            to_hex(&native_record_commitment::consensus_state_root_v1(
                &store.module_state,
            )?),
            to_hex(&native_record_commitment::cumulative_receipt_root_v1(
                store,
            )?),
            2,
        ),
    };
    build_result_from_commitments(
        batch,
        &store.receipts,
        state_root,
        receipt_root,
        novovm_exec::NovovmAoemSnapshotMetadataV1 {
            snapshot_version,
            state_version: store.module_state.aoem_semantic_ledger_sequence,
            backend,
            persistence_owner: "aoem_runtime".to_string(),
        },
    )
}

fn build_result_from_commitments(
    batch: &novovm_exec::NovovmAoemNativeTxBatchV1,
    current_receipts: &BTreeMap<String, NovNativeExecutionReceiptV1>,
    state_root: String,
    receipt_root: String,
    snapshot: novovm_exec::NovovmAoemSnapshotMetadataV1,
) -> Result<novovm_exec::NovovmAoemNativeTxBatchResultV1> {
    let receipts = batch
        .tx_items
        .iter()
        .map(|item| {
            let hash = parse_fixed_hex_32_v1(&item.tx_hash, "candidate receipt hash")?;
            let receipt = current_receipts
                .get(&to_hex(&hash))
                .context("candidate receipt missing")?;
            let meta = receipt
                .aoem_semantic_ingress
                .as_ref()
                .context("candidate AOEM precommit missing")?;
            if !meta.submitted || meta.processed_ops == 0 || meta.processed_ops != meta.success_ops
            {
                bail!("candidate receipt has no successful AOEM precommit");
            }
            Ok(novovm_exec::NovovmAoemNativeTxReceiptV1 {
                sequence: item.sequence,
                tx_hash: item.tx_hash.clone(),
                status_ok: receipt.status,
                receipt_commitment: novovm_exec::native_tx_batch_v1_receipt_commitment(
                    item.sequence,
                    &item.tx_hash,
                    receipt.status,
                ),
                error_class: receipt.failure_reason.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    novovm_exec::build_native_tx_batch_result_from_execution_v1(
        batch,
        receipts,
        state_root,
        receipt_root,
        snapshot,
    )
}

fn compute(
    payload: &Payload,
    input: &Descriptor,
    workspace: &WorkspaceStore,
    params: &serde_json::Value,
) -> Result<Output> {
    // Authenticate the ENTIRE ordered batch before any AOEM submission. Never
    // call ingress: even its no-admission variant observes rejected pending.
    let items = authenticate_payload(payload, workspace, params)?;
    let profile = payload.root_codec_profile()?;
    if profile == NativeRootCodecProfileV1::RecordTreeV1
        && items
            .iter()
            .any(|item| matches!(item.native_tx.kind, NovTxKindV1::Execute(_)))
    {
        // Transfer's typed record codec supports every u128. Execute still
        // uses the legacy JSON evidence domain: reject an unsupported parent
        // before auxiliary AOEM submission, rather than panic in json! later.
        validate_legacy_execution_json_domain_v1(&payload.parent_store()?.module_state)?;
    }
    let batch = build_batch(payload, &items)?;
    if !probe_semantic_graph_v3_capability_v1()?.ready {
        bail!("candidate execution requires AOEM semantic graph V3");
    }
    let chunk_size = NOV_NATIVE_AOEM_CONSENSUS_BATCH_CHUNK_SIZE_V1;
    // Keep the canonical raw wire for root parity. These idempotent auxiliary
    // digest puts may touch AOEM core state/cache, never NOV authority keys.
    let (aggregate, chunks) = execute_native_raw_tx_batch_chunks_via_aoem_semantic_ingress_v1(
        &payload.plan.raw_txs,
        chunk_size,
    )?;
    if !aggregate.submitted
        || aggregate.processed_ops as usize != items.len()
        || aggregate.success_ops as usize != items.len()
    {
        bail!("candidate AOEM precommit did not complete the full authenticated batch");
    }
    let rooted = if profile == NativeRootCodecProfileV1::RecordTreeV1
        && items
            .iter()
            .all(|item| matches!(item.native_tx.kind, NovTxKindV1::Transfer(_)))
    {
        payload
            .record_state
            .as_ref()
            .map(|reference| reference.rooted_parts())
            .transpose()?
            .flatten()
    } else {
        None
    };
    let mut record_updates = None;
    let mut store;
    if let Some((physical_root, state_root, receipt_root, records, blob_bytes)) = rooted {
        if state_root != payload.plan.pre_state_root {
            bail!("rooted candidate state differs from authenticated parent");
        }
        let reader =
            crate::native_state_storage::AoemStateReaderV1::new(&workspace.graph, workspace.scope);
        let transfer_items: Vec<_> = items
            .iter()
            .enumerate()
            .map(|(index, item)| native_transfer_dispatch::Item {
                transaction: &item.native_tx,
                request: &item.execution_request,
                subject: &item.execution_subject,
                reservation: &item.durable_auth_reservation,
                ingress: native_aoem_batch_item_ingress_meta_v1(
                    &chunks[index / chunk_size],
                    index,
                    items.len(),
                ),
            })
            .collect();
        let update = native_transfer_record_execution::execute_rooted_segment_v1(
            &reader,
            physical_root,
            state_root,
            receipt_root,
            &transfer_items,
            u128::from(payload.plan.context.timestamp_unix_ms),
        )?;
        let (records, blob_bytes) = update.stats.checked_apply(records, blob_bytes)?;
        let _peak = update.peak_inflight;
        #[cfg(test)]
        eprintln!("fresh candidate rooted AOEM transfer tasks={} peak_inflight={} parent_tree_import=false", items.len(), _peak);
        // The executor above never scans or imports historical state. Existing
        // output/finality validation still consumes an explicit cold full image;
        // do not confuse removal of the execution import with a fully lazy node.
        store = native_transfer_record_execution::materialize_update_v1(
            &reader,
            &update.physical,
            records,
            blob_bytes,
        )?;
        record_updates = Some(state_records::RecordTreeUpdatesV1 {
            physical: update.physical,
            state: update.state,
            receipts: update.receipts,
            records,
            blob_bytes,
            changes: Some(update.changes),
        });
    } else {
        store = payload.parent_store()?.clone();
        let mut mirror_records = Vec::new();
        let mut index = 0;
        while index < items.len() {
            let item = &items[index];
            if matches!(item.native_tx.kind, NovTxKindV1::Transfer(_)) {
                // Execute is a barrier: it may change any balance, policy or fee
                // state. Only contiguous Transfer runs are submitted together.
                let end = items[index..]
                    .iter()
                    .position(|item| !matches!(item.native_tx.kind, NovTxKindV1::Transfer(_)))
                    .map_or(items.len(), |offset| index + offset);
                let transfer_items: Vec<_> = items[index..end]
                    .iter()
                    .enumerate()
                    .map(|(offset, item)| {
                        let position = index + offset;
                        native_transfer_dispatch::Item {
                            transaction: &item.native_tx,
                            request: &item.execution_request,
                            subject: &item.execution_subject,
                            reservation: &item.durable_auth_reservation,
                            ingress: native_aoem_batch_item_ingress_meta_v1(
                                &chunks[position / chunk_size],
                                position,
                                items.len(),
                            ),
                        }
                    })
                    .collect();
                let now_ms = u128::from(payload.plan.context.timestamp_unix_ms);
                let _peak = match payload.root_codec_profile()? {
                    NativeRootCodecProfileV1::LegacyWireV1 => native_transfer_dispatch::execute_v1(
                        &mut store,
                        &transfer_items,
                        now_ms,
                        &mut mirror_records,
                    )?,
                    NativeRootCodecProfileV1::RecordTreeV1 => {
                        native_transfer_record_execution::execute_segment_v1(
                            &mut store,
                            &transfer_items,
                            now_ms,
                        )?
                    }
                };
                #[cfg(test)]
                eprintln!(
                    "fresh candidate AOEM transfer tasks={} peak_inflight={_peak}",
                    end - index
                );
                index = end;
                continue;
            }
            if profile == NativeRootCodecProfileV1::RecordTreeV1 {
                // A preceding Transfer run may have created a >u64 balance even
                // when every parent balance passed the initial domain check.
                validate_legacy_execution_json_domain_v1(&store.module_state)?;
            }
            dispatch_nov_execution_request_into_loaded_store_v1(
                &mut store,
                &item.execution_request,
                NovExecutionRequestDispatchContextV1 {
                    // This path is never used: mirror records are collected only.
                    mirror_base_path: Path::new(""),
                    subject_meta: Some(&item.execution_subject),
                    requested_behavior: Some(&item.requested_execution_behavior),
                    authenticated_key_algo: Some(UcaKeyAlgo::Ed25519),
                    unified_account_store_path: None,
                    durable_auth_reservation: Some(&item.durable_auth_reservation),
                    aoem_semantic_ingress_override: Some(native_aoem_batch_item_ingress_meta_v1(
                        &chunks[index / chunk_size],
                        index,
                        items.len(),
                    )),
                    mirror_records: Some(&mut mirror_records),
                    emit_policy_observability: false,
                    now_ms: u128::from(payload.plan.context.timestamp_unix_ms),
                },
            )?;
            index += 1;
        }
    }
    verify_native_business_protocol_config_v1(&store)?;
    if verify_required_native_business_protocol_config_pin_v1()? != to_hex(&workspace.protocol) {
        bail!("candidate protocol configuration changed during execution");
    }
    let batch_result = build_result(
        &batch,
        &store,
        native_aoem_owned_runtime_config_v1()?.persist_backend,
        payload.root_codec_profile()?,
    )?;
    Ok(Output {
        schema: OUTPUT_SCHEMA.to_string(),
        workspace_id: input.id,
        input_digest: input.payload,
        expected_output_commitment: batch.expected_output_commitment,
        batch_result,
        store,
        record_state: None,
        record_updates,
    })
}

fn validate_output(
    output: &Output,
    payload: &Payload,
    input: &Descriptor,
    workspace: &WorkspaceStore,
    params: &serde_json::Value,
) -> Result<()> {
    if output.schema != OUTPUT_SCHEMA
        || output.workspace_id != input.id
        || output.input_digest != input.payload
    {
        bail!("candidate output input binding mismatch");
    }
    if output
        .batch_result
        .snapshot_metadata
        .backend
        .trim()
        .is_empty()
        || output
            .batch_result
            .snapshot_metadata
            .backend
            .trim()
            .eq_ignore_ascii_case("none")
    {
        bail!("candidate output requires a persistent backend");
    }
    let items = authenticate_plan(&payload.plan, payload.parent_store()?, params)?;
    let batch = build_batch(payload, &items)?;
    if output.expected_output_commitment != batch.expected_output_commitment
        || output.batch_result
            != build_result(
                &batch,
                &output.store,
                output.batch_result.snapshot_metadata.backend.clone(),
                payload.root_codec_profile()?,
            )?
    {
        bail!("candidate execution result/root/receipt binding mismatch");
    }
    validate_precommit_receipts(payload, &items, &output.store.receipts)?;
    verify_production_native_execution_store_authority_domain_v2(
        &output.store,
        workspace.chain_id,
        &workspace.namespace,
    )?;
    verify_native_business_protocol_config_v1(&output.store)?;
    let mut expected_nonces = payload
        .parent_store()?
        .module_state
        .native_auth_next_nonces
        .clone();
    let mut expected_reservations = payload
        .parent_store()?
        .module_state
        .native_auth_nonce_reservations
        .clone();
    for item in &items {
        let reservation = &item.durable_auth_reservation;
        expected_nonces.insert(
            reservation.identity_key.clone(),
            reservation
                .nonce
                .checked_add(1)
                .context("candidate nonce overflow")?,
        );
        expected_reservations.insert(
            reservation.ledger_key.clone(),
            reservation.reservation_id.clone(),
        );
    }
    if output.store.module_state.native_auth_next_nonces != expected_nonces
        || output.store.module_state.native_auth_nonce_reservations != expected_reservations
        || output.store.receipts.len() != payload.parent_store()?.receipts.len() + items.len()
        || output.store.module_state.aoem_semantic_ledger_sequence
            != payload
                .parent_store()?
                .module_state
                .aoem_semantic_ledger_sequence
                .checked_add(items.len() as u64)
                .context("candidate state version overflow")?
    {
        bail!("candidate output nonce/receipt/version continuity mismatch");
    }
    for (hash, receipt) in &payload.parent_store()?.receipts {
        if output.store.receipts.get(hash) != Some(receipt) {
            bail!("candidate output changed an ancestor receipt");
        }
    }
    Ok(())
}

fn validate_precommit_receipts(
    payload: &impl ExecutionInput,
    items: &[AuthenticatedItem],
    receipts: &BTreeMap<String, NovNativeExecutionReceiptV1>,
) -> Result<()> {
    let chunk_size = NOV_NATIVE_AOEM_CONSENSUS_BATCH_CHUNK_SIZE_V1;
    for (chunk_index, raws) in payload.plan().raw_txs.chunks(chunk_size).enumerate() {
        let (wire, plan_id) = build_native_aoem_raw_tx_batch_ops_wire_v1(raws, chunk_size)?;
        let wire_digest = to_hex(&sha256_bytes_v1(&[
            b"novovm-native-aoem-semantic-wire-digest-v1",
            &wire.bytes,
        ]));
        for offset in 0..raws.len() {
            let index = chunk_index * chunk_size + offset;
            let hash = to_hex(&items[index].tx_hash);
            let receipt = receipts.get(&hash).context("candidate receipt missing")?;
            let meta = receipt
                .aoem_semantic_ingress
                .as_ref()
                .context("candidate precommit metadata missing")?;
            if receipt.tx_hash != hash
                || meta.execution_kernel != "AOEM"
                || meta.semantic_entry != native_aoem_raw_tx_batch_precommit_entry_v1()
                || !meta.enabled
                || !meta.submitted
                || !meta.algebraic_semantic_entry
                || !meta.batch_mode
                || meta.ingress_scope != "raw_tx_batch_precommit_item"
                || meta.plan_id != plan_id
                || meta.batch_plan_id != Some(plan_id)
                || meta.wire_digest != wire_digest
                || meta.op_count != raws.len()
                || meta.batch_size != raws.len()
                || meta.batch_item_index != Some(index)
                || meta.batch_item_count != Some(items.len())
                || meta.processed_ops as usize != raws.len()
                || meta.success_ops as usize != raws.len()
                || meta.fallback_reason.is_some()
                || receipt.aoem_semantic_commit
                    != build_native_receipt_aoem_semantic_commit_v1(receipt)
            {
                bail!("candidate receipt is not bound to its ordered AOEM precommit chunk");
            }
        }
    }
    Ok(())
}

fn read_output_bytes(
    workspace: &WorkspaceStore,
    input: &Descriptor,
    descriptor: &OutputDescriptor,
) -> Result<Option<Vec<u8>>> {
    let mut bytes = Vec::with_capacity(descriptor.len);
    for index in 0..descriptor.len.div_ceil(CHUNK_BYTES) {
        let Some(chunk) = workspace
            .graph
            .get(&output_chunk_key(workspace, &input.id, index))?
        else {
            return Ok(None);
        };
        if chunk.len() != CHUNK_BYTES.min(descriptor.len - bytes.len()) {
            bail!("candidate output chunk length mismatch");
        }
        bytes.extend_from_slice(&chunk);
    }
    if output_digest(&bytes) != descriptor.digest {
        bail!("candidate output digest mismatch");
    }
    Ok(Some(bytes))
}

// Read the exact completed immutable source document. Its publication and
// finalized-ledger provenance must be checked separately by the caller.
pub(super) fn read_completed_output_bytes(
    workspace: &WorkspaceStore,
    id: [u8; 32],
) -> Result<Vec<u8>> {
    let input = ready_input(workspace, id)?;
    let descriptor = catalog(workspace)?
        .into_iter()
        .find(|(known, _)| *known == id)
        .context("published parent output reservation missing")?
        .1;
    if !is_complete(workspace, &input, &descriptor)? {
        bail!("published parent output completion missing");
    }
    read_output_bytes(workspace, &input, &descriptor)?
        .context("published parent output chunks missing")
}

fn read_output(
    workspace: &WorkspaceStore,
    input: &Descriptor,
    descriptor: &OutputDescriptor,
    payload: &Payload,
    params: &serde_json::Value,
) -> Result<Option<Output>> {
    let Some(bytes) = read_output_bytes(workspace, input, descriptor)? else {
        return Ok(None);
    };
    // Even explicit cold export/promotion must reject an invalid V3 witness;
    // full typed root checks do not authorize ignoring its declared transition.
    rooted_output::try_delta_output_view(workspace, input, &bytes, payload, params)?;
    let (mut output, reference): (Output, _) =
        state_records::decode(workspace, &bytes, &["store"])?;
    output.record_state = reference;
    validate_output(&output, payload, input, workspace, params)?;
    Ok(Some(output))
}

fn is_complete(
    workspace: &WorkspaceStore,
    input: &Descriptor,
    output: &OutputDescriptor,
) -> Result<bool> {
    match workspace.graph.get(&workspace.key(b'e', &input.id))? {
        None => Ok(false),
        Some(raw) if raw == completion(workspace, input, output) => Ok(true),
        Some(_) => bail!("candidate execution completion binding mismatch"),
    }
}

fn info(
    input: &Descriptor,
    descriptor: &OutputDescriptor,
    output: OutputView,
    profile: NativeRootCodecProfileV1,
) -> Result<ExecutionInfoV1> {
    let business_owner = if output.batch_result.per_tx_receipts.iter().any(|item| {
        output
            .receipts
            .get(item.tx_hash.trim_start_matches("0x"))
            .is_some_and(|receipt| {
                receipt
                    .logs
                    .iter()
                    .any(|log| log.event == "aoem.native_transfer.computed")
            })
    }) {
        "AOEM_transfer_compute_and_SUPERVM_host_ordered_settlement_or_execute"
    } else {
        "SUPERVM_host"
    };
    Ok(ExecutionInfoV1 {
        schema: OUTPUT_SCHEMA,
        workspace_id: input.id,
        plan_commitment: input.plan,
        output_digest: descriptor.digest,
        post_state_root: output.batch_result.state_delta_root.clone(),
        receipt_root: output.batch_result.receipt_root.clone(),
        execution_evidence_commitment: native_aoem_execution_evidence_with_profile_v1(
            &output.batch_result,
            profile,
        )?,
        batch_result: output.batch_result,
        transactions_authenticated: true,
        aoem_called: true,
        execution_completed: true,
        candidate_state_persisted: true,
        business_transition_computation_owner: business_owner,
        persistence_owner: "aoem_runtime",
        authority_state_published: false,
        chain_canonical: false,
        proof_sealed: false,
        safe: false,
        finalized: false,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecutionCheckpointV1 {
    OutputReserved,
    PartialOutput,
    OutputWritten,
    Completed,
}

pub fn execute_v1(
    chain_id: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<ExecutionInfoV1> {
    execute_with_checkpoint_v1(chain_id, id, params, |_| Ok(()))
}

pub(crate) fn execute_with_checkpoint_v1(
    chain_id: u64,
    id: [u8; 32],
    params: &serde_json::Value,
    checkpoint: impl Fn(ExecutionCheckpointV1) -> Result<()>,
) -> Result<ExecutionInfoV1> {
    let mut workspace = WorkspaceStore::open(chain_id, params)?;
    let input = ready_input(&workspace, id)?;
    let verified = workspace.read_input(&input)?;
    let outputs = catalog(&workspace)?;
    let existing = outputs
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(_, output)| output);
    let complete = existing
        .map(|output| is_complete(&workspace, &input, output))
        .transpose()?
        .unwrap_or(false);
    // A completed or fully written result recovers without resubmitting AOEM
    // or recomputing business transitions, including after authority GC.
    if let Some(descriptor) = existing {
        if let Some(output) = read_output_view(&workspace, &input, descriptor, &verified, params)? {
            if !complete {
                publish(&mut workspace, &input, descriptor)?;
                checkpoint(ExecutionCheckpointV1::Completed)?;
            }
            return info(&input, descriptor, output, verified.root_codec_profile()?);
        }
        if complete {
            bail!("completed candidate output has missing chunks");
        }
    }
    // First computation/output preparation still requires an explicit full
    // image. Completed NCW2/V3 recovery above never takes this cold boundary.
    let materialized;
    let payload = match verified.cold_payload() {
        Some(payload) => payload,
        None => {
            materialized = workspace.read_payload(&input)?;
            &materialized
        }
    };
    let mut output = compute(payload, &input, &workspace, params)?;
    validate_output(&output, payload, &input, &workspace, params)?;
    let parent_store = payload.parent_store()?;
    let mut prepared = if payload.root_codec_profile()? == NativeRootCodecProfileV1::RecordTreeV1 {
        let updates = output.record_updates.take();
        state_records::prepare_record_profile(
            &workspace,
            &output,
            &["store"],
            &output.store,
            payload
                .record_state
                .as_ref()
                .map(|reference| (reference, parent_store)),
            updates,
        )?
    } else {
        state_records::prepare(
            &workspace,
            &output,
            &["store"],
            &output.store,
            payload
                .record_state
                .as_ref()
                .map(|reference| (reference, parent_store)),
        )?
    };
    let mut descriptor = OutputDescriptor {
        len: prepared.bytes.len(),
        digest: output_digest(&prepared.bytes),
        input_digest: input.payload,
    };
    // A partial old record document is pinned as strictly as an old inline
    // image. Reproduce its original bytes; never upgrade the reservation.
    if existing.is_some_and(|previous| *previous != descriptor) {
        prepared = state_records::without_delta(prepared)?;
        descriptor = OutputDescriptor {
            len: prepared.bytes.len(),
            digest: output_digest(&prepared.bytes),
            input_digest: input.payload,
        };
    }
    if existing.is_some_and(|previous| *previous != descriptor) {
        let old = state_records::prepare(
            &workspace,
            &output,
            &["store"],
            &output.store,
            payload
                .record_state
                .as_ref()
                .map(|reference| (reference, parent_store)),
        )?;
        let old_descriptor = OutputDescriptor {
            len: old.bytes.len(),
            digest: output_digest(&old.bytes),
            input_digest: input.payload,
        };
        if existing == Some(&old_descriptor) {
            prepared = old;
            descriptor = old_descriptor;
        }
    }
    // Recover old partial inline output only when the recomputed typed image
    // reproduces EVERY byte bound by its original reservation. New candidates
    // still always use record documents; this is not a fallback on corruption.
    let inline = if let Some(previous) = existing.filter(|previous| **previous != descriptor) {
        let bytes = serde_json::to_vec(&output)?;
        let legacy = OutputDescriptor {
            len: bytes.len(),
            digest: output_digest(&bytes),
            input_digest: input.payload,
        };
        if *previous != legacy {
            bail!("incomplete candidate output recomputation differs from reserved bytes");
        }
        descriptor = legacy;
        Some(bytes)
    } else {
        None
    };
    let bytes = inline.as_deref().unwrap_or(&prepared.bytes);
    check_output_capacity(0, bytes.len())?;
    if let Some(previous) = existing {
        if previous != &descriptor {
            bail!("incomplete candidate output recomputation differs from reserved bytes");
        }
    } else {
        let total = outputs
            .iter()
            .try_fold(0usize, |total, (_, output)| total.checked_add(output.len))
            .context("candidate output capacity overflow")?;
        check_output_capacity(total, bytes.len())?;
        let reservation = AoemAtomicGraphWriteV1::Put {
            key: workspace.key(b'v', &id),
            value: descriptor.encode(),
        };
        workspace.commit(b'V', &input, vec![reservation.clone()], reservation)?;
    }
    checkpoint(ExecutionCheckpointV1::OutputReserved)?;
    if inline.is_none() {
        state_records::persist(&workspace, &prepared)?;
    }
    let writes: Vec<_> = bytes
        .chunks(CHUNK_BYTES)
        .enumerate()
        .map(|(index, chunk)| AoemAtomicGraphWriteV1::Put {
            key: output_chunk_key(&workspace, &id, index),
            value: chunk.to_vec(),
        })
        .collect();
    let reservation = AoemAtomicGraphWriteV1::Put {
        key: workspace.key(b'v', &id),
        value: descriptor.encode(),
    };
    workspace.commit(b'P', &input, writes[..1].to_vec(), reservation.clone())?;
    checkpoint(ExecutionCheckpointV1::PartialOutput)?;
    workspace.commit(b'O', &input, writes, reservation)?;
    checkpoint(ExecutionCheckpointV1::OutputWritten)?;
    let readback = read_output_view(&workspace, &input, &descriptor, &verified, params)?
        .context("candidate output readback incomplete")?;
    publish(&mut workspace, &input, &descriptor)?;
    checkpoint(ExecutionCheckpointV1::Completed)?;
    info(&input, &descriptor, readback, payload.root_codec_profile()?)
}

fn publish(
    workspace: &mut WorkspaceStore,
    input: &Descriptor,
    output: &OutputDescriptor,
) -> Result<()> {
    let reservation = AoemAtomicGraphWriteV1::Put {
        key: workspace.key(b'v', &input.id),
        value: output.encode(),
    };
    let marker = AoemAtomicGraphWriteV1::Put {
        key: workspace.key(b'e', &input.id),
        value: completion(workspace, input, output),
    };
    workspace.commit(b'E', input, vec![reservation], marker)?;
    if !is_complete(workspace, input, output)? {
        bail!("candidate execution completion readback missing");
    }
    Ok(())
}

pub fn load_execution_v1(
    chain_id: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<Option<ExecutionInfoV1>> {
    let workspace = WorkspaceStore::open(chain_id, params)?;
    if !workspace.catalog()?.iter().any(|(_, input)| input.id == id) {
        return Ok(None);
    }
    let input = ready_input(&workspace, id)?;
    let outputs = catalog(&workspace)?;
    let Some((_, descriptor)) = outputs.iter().find(|(known, _)| *known == id) else {
        return Ok(None);
    };
    if !is_complete(&workspace, &input, descriptor)? {
        return Ok(None);
    }
    let payload = workspace.read_input(&input)?;
    let output = read_output_view(&workspace, &input, descriptor, &payload, params)?
        .context("completed candidate output missing")?;
    Ok(Some(info(
        &input,
        descriptor,
        output,
        payload.root_codec_profile()?,
    )?))
}

#[cfg(test)]
pub(crate) fn load_execution_snapshot_for_test_v1(
    chain_id: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(
        load_typed_execution_snapshot_for_test_v1(chain_id, id, params)?,
    )?)
}

#[cfg(test)]
pub(crate) fn load_typed_execution_snapshot_for_test_v1(
    chain_id: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<NovNativeExecutionStoreV1> {
    let workspace = WorkspaceStore::open(chain_id, params)?;
    let input = ready_input(&workspace, id)?;
    let outputs = catalog(&workspace)?;
    let (_, descriptor) = outputs
        .iter()
        .find(|(known, _)| *known == id)
        .context("output not found")?;
    if !is_complete(&workspace, &input, descriptor)? {
        bail!("output not completed");
    }
    let payload = workspace.read_payload(&input)?;
    let output = read_output(&workspace, &input, descriptor, &payload, params)?
        .context("output incomplete")?;
    Ok(output.store)
}

#[cfg(test)]
pub(crate) fn corrupt_execution_output_for_test_v1(
    chain_id: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<()> {
    let mut workspace = WorkspaceStore::open(chain_id, params)?;
    let input = ready_input(&workspace, id)?;
    let outputs = catalog(&workspace)?;
    let (_, descriptor) = outputs
        .iter()
        .find(|(known, _)| *known == id)
        .context("output not found")?;
    let key = output_chunk_key(&workspace, &id, 0);
    let mut bytes = workspace
        .graph
        .get(&key)?
        .context("first output chunk missing")?;
    bytes[0] ^= 1;
    let marker = AoemAtomicGraphWriteV1::Put {
        key: workspace.key(b'e', &id),
        value: completion(&workspace, &input, descriptor),
    };
    workspace.commit(
        b'X',
        &input,
        vec![AoemAtomicGraphWriteV1::Put { key, value: bytes }],
        marker,
    )
}

/// Preserve the old output reservation/chunk encoding for upgrade tests only.
#[cfg(test)]
pub(crate) fn seed_legacy_inline_output_for_test_v1(
    chain: u64,
    id: [u8; 32],
    params: &serde_json::Value,
    stage: ExecutionCheckpointV1,
    corrupt_descriptor: bool,
) -> Result<[u8; 32]> {
    seed_legacy_output_for_test_v1(chain, id, params, stage, corrupt_descriptor, 0)
}

/// Seed the physical-only record document used before three-root bundles.
/// This helper does not alter the production serializer or recovery policy.
#[cfg(test)]
pub(crate) fn seed_legacy_record_output_for_test_v1(
    chain: u64,
    id: [u8; 32],
    params: &serde_json::Value,
    stage: ExecutionCheckpointV1,
    corrupt_descriptor: bool,
) -> Result<[u8; 32]> {
    seed_legacy_output_for_test_v1(chain, id, params, stage, corrupt_descriptor, 1)
}

/// Seed the three-root output format that predates delta witnesses.
#[cfg(test)]
pub(crate) fn seed_previous_record_output_for_test_v1(
    chain: u64,
    id: [u8; 32],
    params: &serde_json::Value,
    stage: ExecutionCheckpointV1,
    corrupt_descriptor: bool,
) -> Result<[u8; 32]> {
    seed_legacy_output_for_test_v1(chain, id, params, stage, corrupt_descriptor, 2)
}

#[cfg(test)]
fn seed_legacy_output_for_test_v1(
    chain: u64,
    id: [u8; 32],
    params: &serde_json::Value,
    stage: ExecutionCheckpointV1,
    corrupt_descriptor: bool,
    document_version: u8,
) -> Result<[u8; 32]> {
    let mut workspace = WorkspaceStore::open(chain, params)?;
    let input = ready_input(&workspace, id)?;
    if catalog(&workspace)?.iter().any(|(known, _)| *known == id) {
        bail!("legacy output fixture requires a new output");
    }
    let payload = workspace.read_payload(&input)?;
    let mut output = compute(&payload, &input, &workspace, params)?;
    validate_output(&output, &payload, &input, &workspace, params)?;
    let prepared = if document_version == 2 {
        let parent_store = payload.parent_store()?;
        let mut updates = output.record_updates.take();
        if let Some(updates) = &mut updates {
            updates.changes = None;
        }
        Some(state_records::prepare_record_profile(
            &workspace,
            &output,
            &["store"],
            &output.store,
            payload
                .record_state
                .as_ref()
                .map(|reference| (reference, parent_store)),
            updates,
        )?)
    } else if document_version == 1 {
        let parent_store = payload.parent_store()?;
        Some(state_records::prepare(
            &workspace,
            &output,
            &["store"],
            &output.store,
            payload
                .record_state
                .as_ref()
                .map(|reference| (reference, parent_store)),
        )?)
    } else {
        None
    };
    let bytes = match &prepared {
        Some(prepared) => prepared.bytes.clone(),
        None => serde_json::to_vec(&output)?,
    };
    if stage == ExecutionCheckpointV1::PartialOutput && bytes.len() <= CHUNK_BYTES {
        bail!("legacy partial-output fixture requires more than one chunk");
    }
    check_output_capacity(0, bytes.len())?;
    let mut descriptor = OutputDescriptor {
        len: bytes.len(),
        digest: output_digest(&bytes),
        input_digest: input.payload,
    };
    if corrupt_descriptor {
        descriptor.digest[0] ^= 1;
    }
    let reservation = AoemAtomicGraphWriteV1::Put {
        key: workspace.key(b'v', &id),
        value: descriptor.encode(),
    };
    workspace.commit(b'V', &input, vec![reservation.clone()], reservation.clone())?;
    // The old producer persisted physical records after OutputReserved and
    // before writing output chunks. Recreate each crash boundary exactly.
    if stage != ExecutionCheckpointV1::OutputReserved {
        if let Some(prepared) = &prepared {
            state_records::persist(&workspace, prepared)?;
        }
    }
    let writes: Vec<_> = bytes
        .chunks(CHUNK_BYTES)
        .enumerate()
        .map(|(index, chunk)| AoemAtomicGraphWriteV1::Put {
            key: output_chunk_key(&workspace, &id, index),
            value: chunk.to_vec(),
        })
        .collect();
    match stage {
        ExecutionCheckpointV1::OutputReserved => {}
        ExecutionCheckpointV1::PartialOutput => {
            workspace.commit(b'P', &input, writes[..1].to_vec(), reservation)?;
        }
        ExecutionCheckpointV1::OutputWritten | ExecutionCheckpointV1::Completed => {
            workspace.commit(b'O', &input, writes, reservation)?;
            if stage == ExecutionCheckpointV1::Completed {
                publish(&mut workspace, &input, &descriptor)?;
            }
        }
    }
    Ok(descriptor.digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_workspace_execution_descriptor_bounds_and_output_capacity_are_closed() {
        let input = Descriptor {
            version: DescriptorVersion::Ncw1,
            id: [1; 32],
            plan: [2; 32],
            payload: [3; 32],
            parent_block: [4; 32],
            parent_state: [5; 32],
            parent_snapshot: [6; 32],
            len: 1,
        };
        let descriptor = OutputDescriptor {
            len: MAX_OUTPUT_BYTES,
            digest: [7; 32],
            input_digest: input.payload,
        };
        assert!(OutputDescriptor::decode(&descriptor.encode(), &input).unwrap() == descriptor);
        let mut wrong = descriptor.clone();
        wrong.input_digest[0] ^= 1;
        assert!(OutputDescriptor::decode(&wrong.encode(), &input).is_err());
        for len in [0, MAX_OUTPUT_BYTES + 1, usize::MAX] {
            wrong = descriptor.clone();
            wrong.len = len;
            assert!(OutputDescriptor::decode(&wrong.encode(), &input).is_err());
        }
        let mut trailing = descriptor.encode();
        trailing.push(0);
        assert!(OutputDescriptor::decode(&trailing, &input).is_err());
        assert!(OutputDescriptor::decode(&[], &input).is_err());
        check_output_capacity(MAX_TOTAL_OUTPUT_BYTES - MAX_OUTPUT_BYTES, MAX_OUTPUT_BYTES).unwrap();
        for (reserved, len) in [
            (MAX_TOTAL_OUTPUT_BYTES, 1),
            (usize::MAX, 1),
            (0, 0),
            (0, MAX_OUTPUT_BYTES + 1),
        ] {
            assert!(check_output_capacity(reserved, len).is_err());
        }
    }
}
