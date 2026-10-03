//! One value for voting, derived from an actual execution packet. The input-plan
//! ID alone is NOT a block hash. This value also commits to the exact output,
//! ordered receipts, content document and parent decision, independent of the
//! voting round or which validator subset later forms a certificate.

#[cfg(test)]
mod tests;

use super::wire::{Context as ConsensusContext, Hash, ValidatorSet};
use crate::native_pipeline::execution::plan::BatchContext;
use crate::native_pipeline::persistence::{PreparedCandidate, StoredCandidate};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};

// The durable journal and execution archive share one exact parent layout.
pub use novovm_consensus::round_bft::journal::ParentPoint;

/// An immutable local execution statement. Construction requires the private
/// PreparedCandidate created by real execution; arbitrary peer roots/raw bytes
/// cannot be deserialized into this type. It is NOT a durability capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockStatement {
    consensus: ConsensusContext,
    execution: BatchContext,
    candidate_id: Hash,
    state_root: Hash,
    receipt_batch_commitment: Hash,
    execution_statement: Hash,
    document_digest: Hash,
    transaction_count: u64,
    state_version: u64,
    hash: Hash,
}

impl BlockStatement {
    /// Cold archive reconstruction only. This does not confer execution or
    /// durability capability; the journal requires a real DurableCandidate.
    pub(crate) fn from_stored(
        stored: &StoredCandidate,
        consensus: ConsensusContext,
        set: &ValidatorSet,
        parent: &ParentPoint,
    ) -> Result<Self> {
        let execution = *stored.context();
        let transaction_count = u64::try_from(stored.raw_transactions().len())?;
        let statement = Self {
            consensus,
            execution,
            candidate_id: stored.candidate_id(),
            state_root: stored.state_root(),
            receipt_batch_commitment: stored.receipt_batch_commitment(),
            execution_statement: stored.statement_commitment(),
            document_digest: stored.document_digest(),
            transaction_count,
            state_version: execution
                .parent_state_version
                .checked_add(transaction_count)
                .context("executed statement state version overflow")?,
            hash: [0; 32],
        };
        statement.checked(set, parent)
    }

    pub fn from_executed(
        packet: &PreparedCandidate,
        consensus: ConsensusContext,
        set: &ValidatorSet,
        parent: &ParentPoint,
    ) -> Result<Self> {
        let execution = *packet.context();
        let transaction_count = u64::try_from(packet.transaction_count())?;
        let state_version = execution
            .parent_state_version
            .checked_add(transaction_count)
            .context("executed statement state version overflow")?;
        let statement = Self {
            consensus,
            execution,
            candidate_id: packet.candidate_id(),
            state_root: packet.state_root(),
            receipt_batch_commitment: packet.receipt_batch_commitment(),
            execution_statement: packet.statement_commitment(),
            document_digest: packet.document_digest(),
            transaction_count,
            state_version,
            hash: [0; 32],
        };
        statement.checked(set, parent)
    }

    fn checked(mut self, set: &ValidatorSet, parent: &ParentPoint) -> Result<Self> {
        self.consensus.validate(set)?;
        let (consensus, execution) = (&self.consensus, &self.execution);
        ensure!(
            consensus.chain_id == execution.chain_id
                && consensus.genesis_config_commitment == execution.genesis_config_commitment
                && consensus.protocol_commitment == execution.protocol_commitment
                && consensus.height == execution.height
                && consensus.parent_block_hash == execution.parent_block_hash,
            "consensus and executed candidate domain/parent mismatch"
        );
        self.validate_parent(parent)?;
        self.hash = self.compute_hash();
        Ok(self)
    }

    pub fn hash(&self) -> Hash {
        self.hash
    }

    pub fn context(&self) -> &ConsensusContext {
        &self.consensus
    }

    pub fn execution_context(&self) -> &BatchContext {
        &self.execution
    }

    pub fn state_version(&self) -> u64 {
        self.state_version
    }

    pub fn matches_packet(&self, packet: &PreparedCandidate) -> bool {
        self.execution == *packet.context()
            && self.candidate_id == packet.candidate_id()
            && self.state_root == packet.state_root()
            && self.receipt_batch_commitment == packet.receipt_batch_commitment()
            && self.execution_statement == packet.statement_commitment()
            && self.document_digest == packet.document_digest()
            && self.transaction_count == packet.transaction_count() as u64
    }

    pub fn validate_parent(&self, parent: &ParentPoint) -> Result<()> {
        ensure!(
            self.execution.parent_height == parent.height
                && self.execution.parent_block_hash == parent.block_hash
                && self.execution.parent_state_root == parent.state_root
                && self.execution.parent_receipt_root == parent.receipt_batch_commitment
                && self.execution.parent_state_version == parent.state_version
                && self.consensus.parent_decision_hash == parent.decision_hash,
            "executed statement does not extend exact current parent"
        );
        Ok(())
    }

    fn compute_hash(&self) -> Hash {
        let c = &self.execution;
        let mut hash = Sha256::new();
        hash.update(b"novovm/replacement/executed-block/round-bft-v1\0");
        for value in [
            c.chain_id,
            self.consensus.epoch,
            c.parent_height,
            c.parent_state_version,
            c.height,
            c.slot,
            c.timestamp_unix_ms,
            self.transaction_count,
            self.state_version,
        ] {
            hash.update(value.to_be_bytes());
        }
        hash.update(c.semantic_version.to_be_bytes());
        for value in [
            c.genesis_config_commitment,
            c.protocol_commitment,
            self.consensus.validator_set_hash,
            c.business_program,
            c.effect_contract,
            c.parent_block_hash,
            c.parent_state_root,
            c.parent_receipt_root,
            self.consensus.parent_decision_hash,
            c.receipt_codec,
            self.candidate_id,
            self.state_root,
            self.receipt_batch_commitment,
            self.execution_statement,
            self.document_digest,
        ] {
            hash.update(value);
        }
        hash.finalize().into()
    }
}
