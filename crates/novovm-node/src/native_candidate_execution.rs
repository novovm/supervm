#![forbid(unsafe_code)]

//! Host business computation against a copied parent, AOEM generic precommit
//! and durable result storage. No authority publication or remote scheduling.

use super::auth::{authenticate_plan, AuthenticatedItem};
use super::*;

const OUTPUT_SCHEMA: &str = "novovm-native-candidate-execution/v1";
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_TOTAL_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

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

fn output_digest(bytes: &[u8]) -> [u8; 32] {
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
    for (_, input) in workspace.catalog()? {
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
    payload: &Payload,
    items: &[AuthenticatedItem],
) -> Result<novovm_exec::NovovmAoemNativeTxBatchV1> {
    let mut batch_items = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let parameter_payload = native_tx_parameter_payload_v1(
            &item.native_tx,
            &item.ir,
            Some(&item.execution_request),
            Some(&item.execution_subject),
            &payload.plan.raw_txs[index],
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
    let context_commitment = payload.plan.context.commitment()?;
    let batch_id = deterministic_native_aoem_batch_id_v1(
        payload.plan.context.chain_id,
        &payload.parent_snapshot.store,
        &batch_items,
        Some(&context_commitment),
    );
    novovm_exec::build_native_tx_batch_v1(
        batch_id,
        payload.plan.context.chain_id,
        Some(payload.plan.context.block_height),
        batch_items,
    )
}

fn build_result(
    batch: &novovm_exec::NovovmAoemNativeTxBatchV1,
    store: &NovNativeExecutionStoreV1,
    backend: String,
) -> Result<novovm_exec::NovovmAoemNativeTxBatchResultV1> {
    let receipts = batch
        .tx_items
        .iter()
        .map(|item| {
            let hash = parse_fixed_hex_32_v1(&item.tx_hash, "candidate receipt hash")?;
            let receipt = store
                .receipts
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
        native_semantic_ledger_state_digest_v1(&store.module_state),
        native_execution_receipt_root_v2(store)?,
        novovm_exec::NovovmAoemSnapshotMetadataV1 {
            snapshot_version: 1,
            state_version: store.module_state.aoem_semantic_ledger_sequence,
            backend,
            persistence_owner: "aoem_runtime".to_string(),
        },
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
    let items = authenticate_plan(&payload.plan, &payload.parent_snapshot.store, params)?;
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
    let mut store = payload.parent_snapshot.store.clone();
    let mut mirror_records = Vec::new();
    for (index, item) in items.iter().enumerate() {
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
    }
    verify_native_business_protocol_config_v1(&store)?;
    if verify_required_native_business_protocol_config_pin_v1()? != to_hex(&workspace.protocol) {
        bail!("candidate protocol configuration changed during execution");
    }
    let batch_result = build_result(
        &batch,
        &store,
        native_aoem_owned_runtime_config_v1()?.persist_backend,
    )?;
    Ok(Output {
        schema: OUTPUT_SCHEMA.to_string(),
        workspace_id: input.id,
        input_digest: input.payload,
        expected_output_commitment: batch.expected_output_commitment,
        batch_result,
        store,
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
    let items = authenticate_plan(&payload.plan, &payload.parent_snapshot.store, params)?;
    let batch = build_batch(payload, &items)?;
    if output.expected_output_commitment != batch.expected_output_commitment
        || output.batch_result
            != build_result(
                &batch,
                &output.store,
                output.batch_result.snapshot_metadata.backend.clone(),
            )?
    {
        bail!("candidate execution result/root/receipt binding mismatch");
    }
    let chunk_size = NOV_NATIVE_AOEM_CONSENSUS_BATCH_CHUNK_SIZE_V1;
    for (chunk_index, raws) in payload.plan.raw_txs.chunks(chunk_size).enumerate() {
        let (wire, plan_id) = build_native_aoem_raw_tx_batch_ops_wire_v1(raws, chunk_size)?;
        let wire_digest = to_hex(&sha256_bytes_v1(&[
            b"novovm-native-aoem-semantic-wire-digest-v1",
            &wire.bytes,
        ]));
        for offset in 0..raws.len() {
            let index = chunk_index * chunk_size + offset;
            let hash = to_hex(&items[index].tx_hash);
            let receipt = output
                .store
                .receipts
                .get(&hash)
                .context("candidate receipt missing")?;
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
    verify_production_native_execution_store_authority_domain_v2(
        &output.store,
        workspace.chain_id,
        &workspace.namespace,
    )?;
    verify_native_business_protocol_config_v1(&output.store)?;
    let mut expected_nonces = payload
        .parent_snapshot
        .store
        .module_state
        .native_auth_next_nonces
        .clone();
    let mut expected_reservations = payload
        .parent_snapshot
        .store
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
        || output.store.receipts.len() != payload.parent_snapshot.store.receipts.len() + items.len()
        || output.store.module_state.aoem_semantic_ledger_sequence
            != payload
                .parent_snapshot
                .store
                .module_state
                .aoem_semantic_ledger_sequence
                .checked_add(items.len() as u64)
                .context("candidate state version overflow")?
    {
        bail!("candidate output nonce/receipt/version continuity mismatch");
    }
    for (hash, receipt) in &payload.parent_snapshot.store.receipts {
        if output.store.receipts.get(hash) != Some(receipt) {
            bail!("candidate output changed an ancestor receipt");
        }
    }
    Ok(())
}

fn read_output(
    workspace: &WorkspaceStore,
    input: &Descriptor,
    descriptor: &OutputDescriptor,
    payload: &Payload,
    params: &serde_json::Value,
) -> Result<Option<Output>> {
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
    let output: Output = serde_json::from_slice(&bytes)?;
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
    output: Output,
) -> Result<ExecutionInfoV1> {
    Ok(ExecutionInfoV1 {
        schema: OUTPUT_SCHEMA,
        workspace_id: input.id,
        plan_commitment: input.plan,
        output_digest: descriptor.digest,
        post_state_root: output.batch_result.state_delta_root.clone(),
        receipt_root: output.batch_result.receipt_root.clone(),
        execution_evidence_commitment: native_aoem_execution_evidence_commitment_v1(
            &output.batch_result,
        )?,
        batch_result: output.batch_result,
        transactions_authenticated: true,
        aoem_called: true,
        execution_completed: true,
        candidate_state_persisted: true,
        business_transition_computation_owner: "SUPERVM_host",
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
    let payload = workspace.read_payload(&input)?;
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
        if let Some(output) = read_output(&workspace, &input, descriptor, &payload, params)? {
            if !complete {
                publish(&mut workspace, &input, descriptor)?;
                checkpoint(ExecutionCheckpointV1::Completed)?;
            }
            return info(&input, descriptor, output);
        }
        if complete {
            bail!("completed candidate output has missing chunks");
        }
    }
    let output = compute(&payload, &input, &workspace, params)?;
    validate_output(&output, &payload, &input, &workspace, params)?;
    let bytes = serde_json::to_vec(&output)?;
    check_output_capacity(0, bytes.len())?;
    let descriptor = OutputDescriptor {
        len: bytes.len(),
        digest: output_digest(&bytes),
        input_digest: input.payload,
    };
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
    let readback = read_output(&workspace, &input, &descriptor, &payload, params)?
        .context("candidate output readback incomplete")?;
    publish(&mut workspace, &input, &descriptor)?;
    checkpoint(ExecutionCheckpointV1::Completed)?;
    info(&input, &descriptor, readback)
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
    let payload = workspace.read_payload(&input)?;
    let output = read_output(&workspace, &input, descriptor, &payload, params)?
        .context("completed candidate output missing")?;
    Ok(Some(info(&input, descriptor, output)?))
}

#[cfg(test)]
pub(crate) fn load_execution_snapshot_for_test_v1(
    chain_id: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
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
    Ok(serde_json::to_value(output.store)?)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_workspace_execution_descriptor_bounds_and_output_capacity_are_closed() {
        let input = Descriptor {
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
