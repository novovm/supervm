//! Reconstruct a block from independently authenticated, durable isolated output.
//! This does not register a signing candidate or publish any authoritative state.
use super::*;
use crate::native_block_ledger::{build_durable_block_v1, build_prepared_block_v1};

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
