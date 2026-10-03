//! Product storage/execution adapter for the single consensus-owned journal.
//! Migrated from a7db795 without changing its packet, archive or metadata bytes.
//! The private durable candidate must belong to this resident pipeline. The
//! backend never grants a synthetic ACK, splits a conditional write, or publishes
//! a head separately from the consensus journal's checked metadata transition.

use super::chain::{ChainRecord, ChainRecovery};
use super::statement::{BlockStatement, ParentPoint};
use super::wire::{Context as ConsensusContext, Hash, ValidatorSet};
use crate::native_pipeline::persistence::io::IoTicket;
use crate::native_pipeline::persistence::metadata::{
    MetaKey, MetaOutcome, MetaTransition, MetadataSnapshot,
};
use crate::native_pipeline::pipeline::{CandidatePipeline, DurableCandidate};
use anyhow::Result;
use novovm_consensus::round_bft::journal::backend::{
    JournalBackend, JournalDomain, JournalRecord, JournalStatement, JournalTicket, RecoveredChain,
};
use std::sync::Arc;

pub use novovm_consensus::round_bft::journal::{
    codec, CandidateLocator, DurableMessage, ReplayEvidence, ReplayRecord, TimeoutStep,
    MAX_REPLAY_RECORDS,
};

pub type ValidatorJournal =
    novovm_consensus::round_bft::journal::ValidatorJournal<CandidatePipeline>;
pub type JournalOpening = novovm_consensus::round_bft::journal::JournalOpening<CandidatePipeline>;

impl<T> JournalTicket<T> for IoTicket<T> {
    fn try_take(&mut self) -> Result<Option<T>> {
        IoTicket::try_take(self)
    }
}

impl JournalStatement for BlockStatement {
    fn hash(&self) -> Hash {
        BlockStatement::hash(self)
    }
}

impl JournalRecord for ChainRecord {
    fn parent(&self) -> ParentPoint {
        ChainRecord::parent(self)
    }
    fn context(&self) -> ConsensusContext {
        ChainRecord::context(self)
    }
    fn point(&self) -> ParentPoint {
        ChainRecord::point(self)
    }
    fn encode(&self) -> Result<Vec<u8>> {
        ChainRecord::encode(self)
    }
    fn head_bytes(&self) -> Result<Vec<u8>> {
        ChainRecord::head_bytes(self)
    }
}

impl JournalBackend for CandidatePipeline {
    type Candidate = DurableCandidate;
    type Statement = BlockStatement;
    type Record = ChainRecord;
    type Recovery = ChainRecovery;
    type ReadTicket = IoTicket<MetadataSnapshot>;
    type WriteTicket = IoTicket<MetaOutcome>;

    fn owner_identity(&self) -> Arc<()> {
        CandidatePipeline::owner_identity(self)
    }

    fn storage_domain(&self) -> JournalDomain {
        let domain = CandidatePipeline::storage_domain(self);
        JournalDomain {
            chain_id: domain.chain_id,
            genesis_config_commitment: domain.genesis_config_commitment,
            protocol_commitment: domain.protocol_commitment,
        }
    }

    fn checked_statement(
        candidate: &Self::Candidate,
        owner: &Arc<()>,
        context: ConsensusContext,
        set: &ValidatorSet,
        parent: &ParentPoint,
    ) -> Result<Self::Statement> {
        BlockStatement::from_executed(candidate.bind_to(owner)?, context, set, parent)
    }

    fn candidate_locator(candidate: &Self::Candidate, value: Hash) -> CandidateLocator {
        CandidateLocator {
            value,
            candidate_id: candidate.packet().candidate_id(),
            document_digest: candidate.packet().document_digest(),
        }
    }

    fn new_record(
        statement: &Self::Statement,
        candidate: &Self::Candidate,
        parent: ParentPoint,
        validator: Hash,
        revision: u64,
        outbox: &[u8],
    ) -> Result<Self::Record> {
        ChainRecord::new(
            statement,
            candidate.packet(),
            parent,
            validator,
            revision,
            outbox,
        )
    }

    fn begin_recovery(
        context: ConsensusContext,
        parent: ParentPoint,
        set: Arc<ValidatorSet>,
        head: Option<Vec<u8>>,
    ) -> Result<Self::Recovery> {
        ChainRecovery::new(context, parent, set, head)
    }

    fn poll_recovery(
        &self,
        recovery: &mut Self::Recovery,
    ) -> Result<Option<RecoveredChain<Self::Record>>> {
        Ok(recovery.poll(self)?.map(|recovered| RecoveredChain {
            record: recovered.record,
            head_bytes: recovered.head_bytes,
        }))
    }

    fn try_read_consensus_metadata(&self, keys: Vec<MetaKey>) -> Result<Option<Self::ReadTicket>> {
        CandidatePipeline::try_read_consensus_metadata(self, keys)
    }

    fn try_apply_consensus_metadata(
        &self,
        transition: MetaTransition,
    ) -> Result<Option<Self::WriteTicket>> {
        CandidatePipeline::try_apply_consensus_metadata(self, transition)
    }
}
