//! Reconstruct a block from independently authenticated, durable isolated output.
//! Explicit graph registration is separate and never grants signing permission
//! or publishes authoritative state.
use super::*;
use crate::native_block_ledger::{build_durable_block_v1, build_prepared_block_v1};
use crate::native_block_ledger::{
    NovNativeBlockCandidateRecordV1, NovNativeIsolatedExecutionBindingV1,
};

/// An in-memory artifact, not a ledger membership or current-state capability.
/// The legacy block codec's canonical_local field describes local continuity;
/// it does not attest that this artifact has been selected or published.
#[derive(Debug, Clone, PartialEq)]
pub struct IsolatedBlockArtifactV1 {
    pub workspace_id: [u8; 32],
    pub plan_commitment: [u8; 32],
    pub output_digest: [u8; 32],
    block: NovNativeDurableBlockV1,
}

impl IsolatedBlockArtifactV1 {
    pub fn block(&self) -> &NovNativeDurableBlockV1 {
        &self.block
    }
}

/// Reads and revalidates the complete input/output, including transaction auth,
/// state/receipt commitments and completion marker. Never executes or repairs.
/// Absence/incomplete output returns None; corrupt/aborted evidence is an error.
pub fn load_block_artifact_v1(
    chain_id: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<Option<IsolatedBlockArtifactV1>> {
    let workspace = WorkspaceStore::open(chain_id, params)?;
    load_block_artifact_inner_v1(&workspace, id, params)
}

fn load_block_artifact_inner_v1(
    workspace: &WorkspaceStore,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<Option<IsolatedBlockArtifactV1>> {
    if !workspace.catalog()?.iter().any(|(_, input)| input.id == id) {
        return Ok(None);
    }
    let input = ready_input(workspace, id)?;
    let outputs = catalog(workspace)?;
    let Some((_, descriptor)) = outputs.iter().find(|(known, _)| *known == id) else {
        return Ok(None);
    };
    if !is_complete(workspace, &input, descriptor)? {
        return Ok(None);
    }
    let payload = workspace.read_payload(&input)?;
    let output = read_output(workspace, &input, descriptor, &payload, params)?
        .context("completed isolated block output missing")?;
    let plan = &payload.plan;
    let mut prepared = build_prepared_block_v1(NovNativeBlockCandidateInputV1 {
        context: plan.context,
        tx_hashes: plan.tx_hashes.clone(),
        raw_txs: plan.raw_txs.clone(),
        pre_state_root: plan.pre_state_root,
        aoem_parent: plan.aoem_parent.clone(),
    })?;
    prepared.expected_aoem_batch_id = Some(output.batch_result.batch_id.clone());
    prepared.expected_aoem_output_commitment = Some(output.expected_output_commitment);
    let receipts = plan
        .tx_hashes
        .iter()
        .map(|hash| {
            let receipt = output
                .store
                .receipts
                .get(&to_hex(hash))
                .context("isolated block receipt missing")?;
            full_native_receipt_commitment_v1(receipt)
        })
        .collect::<Result<Vec<_>>>()?;
    let result = &output.batch_result;
    let block = build_durable_block_v1(
        &prepared,
        NovNativeBlockCommitInputV1 {
            post_state_root: parse_fixed_hex_32_v1(
                &result.state_delta_root,
                "isolated block state root",
            )?,
            cumulative_receipt_root: parse_fixed_hex_32_v1(
                &result.receipt_root,
                "isolated block receipt root",
            )?,
            per_block_receipt_commitments: receipts,
            aoem_batch_id: result.batch_id.clone(),
            aoem_batch_result_id: result.batch_result_id.clone(),
            aoem_evidence_commitment: parse_fixed_hex_32_v1(
                &native_aoem_execution_evidence_commitment_v1(result)?,
                "isolated block evidence",
            )?,
            state_version: result.snapshot_metadata.state_version,
        },
    )?;
    plan.validate_against_block(&block)?;
    Ok(Some(IsolatedBlockArtifactV1 {
        workspace_id: id,
        plan_commitment: input.plan,
        output_digest: descriptor.digest,
        block,
    }))
}

/// Explicit local registration, not a network admission or signing API. Holds
/// the workspace lock through ledger readback so abort cannot race the read.
pub fn register_block_candidate_v1(
    chain_id: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<NovNativeBlockCandidateRecordV1> {
    let workspace = WorkspaceStore::open(chain_id, params)?;
    let artifact = load_block_artifact_inner_v1(&workspace, id, params)?
        .context("isolated candidate has no complete verified output")?;
    let store_path = resolve_native_execution_store_path_from_params_v1(params)
        .unwrap_or_else(nov_native_execution_store_path_v1);
    let _authority_lock = acquire_nov_native_execution_store_write_lock_v1(&store_path)?;
    let ledger_path = nov_native_block_ledger_rocksdb_path_v1(&store_path);
    let probe = NovNativeBlockLedgerV1::open_existing_read_only(&ledger_path)?
        .context("isolated registration requires an existing ledger")?;
    drop(probe);
    let ledger = NovNativeBlockLedgerV1::open(&ledger_path)?;
    ledger.register_isolated_candidate_v1(
        artifact.block,
        NovNativeIsolatedExecutionBindingV1 {
            workspace_id: id,
            plan_commitment: artifact.plan_commitment,
            output_digest: artifact.output_digest,
        },
        &workspace.namespace,
        &to_hex(&workspace.protocol),
    )
}

/// Revalidate isolated execution and its current authoritative parent, then
/// permit seal operations only within this synchronous callback. Holds workspace,
/// authority and ledger locks in that order. The callback must not re-enter
/// execution/workspace APIs or mutate this ledger through another handle.
/// It may persist seal votes using the existing seal safety rules. No promotion.
pub fn with_verified_block_candidate_v1<T>(
    chain_id: u64,
    id: [u8; 32],
    params: &serde_json::Value,
    action: impl FnOnce(&NovNativeBlockLedgerV1) -> Result<T>,
) -> Result<T> {
    let workspace = WorkspaceStore::open(chain_id, params)?;
    let artifact = load_block_artifact_inner_v1(&workspace, id, params)?
        .context("isolated signing requires complete live execution output")?;
    let input = ready_input(&workspace, id)?;
    let payload = workspace.read_payload(&input)?;
    let store_path = resolve_native_execution_store_path_from_params_v1(params)
        .unwrap_or_else(nov_native_execution_store_path_v1);
    let _authority_lock = acquire_nov_native_execution_store_write_lock_v1(&store_path)?;
    let current = capture_parent_locked(&payload.plan, &store_path, &workspace)?;
    if current.parent_block != payload.parent_block
        || serde_json::to_value(&current.parent_snapshot)?
            != serde_json::to_value(&payload.parent_snapshot)?
    {
        bail!("isolated signing authoritative parent no longer matches captured state");
    }
    let ledger =
        NovNativeBlockLedgerV1::open(&nov_native_block_ledger_rocksdb_path_v1(&store_path))?;
    ledger.with_isolated_seal_scope_v1(
        &artifact.block,
        &NovNativeIsolatedExecutionBindingV1 {
            workspace_id: id,
            plan_commitment: artifact.plan_commitment,
            output_digest: artifact.output_digest,
        },
        action,
    )
}
