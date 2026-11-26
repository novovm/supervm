//! Wire evidence for the original signed block statement and its state root.
use super::{BatchContext, BlockStatement, ConsensusContext, Hash, ParentPoint, ValidatorSet};
use crate::native_pipeline::consensus::{wire, ArchiveBlock};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

const MAGIC: &[u8; 8] = b"NVFHDR01";
pub(crate) const MAX_BYTES: usize = 1024 + 2 * wire::MAX_WIRE_BYTES;

#[derive(Deserialize, Serialize)]
struct Header {
    execution: BatchContext,
    candidate_id: Hash,
    state_root: Hash,
    receipt_batch_commitment: Hash,
    execution_statement: Hash,
    document_digest: Hash,
    transaction_count: u64,
}

pub(crate) fn encode(block: &ArchiveBlock) -> Result<Vec<u8>> {
    let stored = block.stored();
    let header = Header {
        execution: *stored.context(),
        candidate_id: stored.candidate_id(),
        state_root: stored.state_root(),
        receipt_batch_commitment: stored.receipt_batch_commitment(),
        execution_statement: stored.statement_commitment(),
        document_digest: stored.document_digest(),
        transaction_count: u64::try_from(stored.raw_transactions().len())?,
    };
    let proposal = wire::encode_proposal(block.proposal())?;
    let precommits = wire::encode_quorum(block.certificate())?;
    let mut bytes = postcard::to_extend(&header, MAGIC.to_vec())?;
    bytes.extend_from_slice(&u32::try_from(proposal.len())?.to_be_bytes());
    bytes.extend(proposal);
    bytes.extend(precommits);
    ensure!(bytes.len() <= MAX_BYTES, "finalized header byte limit");
    Ok(bytes)
}

/// Neither the sender nor the wire can select the expected chain, set or parent.
pub(crate) fn verify(
    bytes: &[u8],
    set: &ValidatorSet,
    context: ConsensusContext,
    parent: &ParentPoint,
) -> Result<ParentPoint> {
    ensure!(bytes.len() <= MAX_BYTES, "finalized header byte limit");
    ensure!(
        parent.height.checked_add(1) == Some(context.height),
        "header height skips verified parent"
    );
    let contents = bytes
        .strip_prefix(MAGIC)
        .context("unrecognized certified header")?;
    let (header, evidence): (Header, _) = postcard::take_from_bytes(contents)?;
    let (proposal_len, evidence) = evidence
        .split_first_chunk::<4>()
        .context("short header evidence")?;
    let (proposal, precommits) = evidence
        .split_at_checked(u32::from_be_bytes(*proposal_len) as usize)
        .context("short header proposal")?;
    ensure!(
        (1..=wire::MAX_WIRE_BYTES).contains(&proposal.len())
            && (1..=wire::MAX_WIRE_BYTES).contains(&precommits.len()),
        "header consensus evidence limit"
    );
    ensure!(
        header.transaction_count > 0
            && header.state_root != [0; 32]
            && header.receipt_batch_commitment != [0; 32]
            && header.document_digest != [0; 32],
        "invalid certified header fields"
    );
    BlockStatement {
        consensus: context,
        execution: header.execution,
        candidate_id: header.candidate_id,
        state_root: header.state_root,
        receipt_batch_commitment: header.receipt_batch_commitment,
        execution_statement: header.execution_statement,
        document_digest: header.document_digest,
        transaction_count: header.transaction_count,
        state_version: parent
            .state_version
            .checked_add(header.transaction_count)
            .context("header state version exhausted")?,
        hash: [0; 32],
    }
    .checked(set, parent)?
    .verified_point(set, proposal, precommits)
}
