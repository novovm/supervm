//! Trusted product adapter for the a7db795 durable journal. This boundary moves
//! physical I/O and execution-format knowledge out of consensus, not safety
//! rules into the caller. Implementations must use one resident storage owner,
//! keep conditional writes atomic, and acknowledge only after exact readback.
//! Implementing this trait is part of the trusted node, not a peer capability.

use super::metadata::{MetaKey, MetaOutcome, MetaTransition, MetadataSnapshot};
use super::{CandidateLocator, ConsensusContext, Hash, ValidatorSet};
use anyhow::Result;
use std::sync::Arc;

/// Exact original parent layout. Per-batch receipts are not a cumulative tree.
/// Values alone do not authenticate a parent or confer signing permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ParentPoint {
    pub height: u64,
    pub block_hash: Hash,
    pub state_root: Hash,
    pub receipt_batch_commitment: Hash,
    pub state_version: u64,
    pub decision_hash: Hash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JournalDomain {
    pub chain_id: u64,
    pub genesis_config_commitment: Hash,
    pub protocol_commitment: Hash,
}

/// A ticket comes only from this backend's submitted operation. No journal API
/// accepts a caller-created boolean ACK or an unrelated completion response.
pub trait JournalTicket<T> {
    fn try_take(&mut self) -> Result<Option<T>>;
}

pub trait JournalStatement {
    fn hash(&self) -> Hash;
}

/// Product-owned archive layout, verified against the executed statement and
/// exact decision outbox by `new_record`, or by complete cold chain recovery.
pub trait JournalRecord {
    fn parent(&self) -> ParentPoint;
    fn context(&self) -> ConsensusContext;
    fn point(&self) -> ParentPoint;
    fn encode(&self) -> Result<Vec<u8>>;
    fn head_bytes(&self) -> Result<Vec<u8>>;
}

pub struct RecoveredChain<R> {
    pub record: Option<R>,
    pub head_bytes: Option<Vec<u8>>,
}

pub trait JournalBackend: Sized {
    type Candidate;
    type Statement: JournalStatement;
    type Record: JournalRecord;
    type Recovery;
    type ReadTicket: JournalTicket<MetadataSnapshot>;
    type WriteTicket: JournalTicket<MetaOutcome>;

    fn owner_identity(&self) -> Arc<()>;
    fn storage_domain(&self) -> JournalDomain;

    /// Bind the real, private durable candidate to `owner`, then construct the
    /// existing statement with exact context/set/parent validation. Neither a
    /// raw peer hash nor a decoded archive is an acceptable durable candidate.
    fn checked_statement(
        candidate: &Self::Candidate,
        owner: &Arc<()>,
        context: ConsensusContext,
        set: &ValidatorSet,
        parent: &ParentPoint,
    ) -> Result<Self::Statement>;

    fn candidate_locator(candidate: &Self::Candidate, value: Hash) -> CandidateLocator;

    /// Preserve the original statement/packet, parent and exact outbox binding.
    /// This only constructs the record; it must not publish a separate head.
    fn new_record(
        statement: &Self::Statement,
        candidate: &Self::Candidate,
        parent: ParentPoint,
        validator: Hash,
        revision: u64,
        outbox: &[u8],
    ) -> Result<Self::Record>;

    fn begin_recovery(
        context: ConsensusContext,
        parent: ParentPoint,
        set: Arc<ValidatorSet>,
        head: Option<Vec<u8>>,
    ) -> Result<Self::Recovery>;

    /// Verify the configured genesis-to-head prefix, archived certificates and
    /// candidate contents through the SAME owner before returning a head.
    fn poll_recovery(
        &self,
        recovery: &mut Self::Recovery,
    ) -> Result<Option<RecoveredChain<Self::Record>>>;

    fn try_read_consensus_metadata(&self, keys: Vec<MetaKey>) -> Result<Option<Self::ReadTicket>>;

    /// Preserve expected bytes, append-only outbox/block keys and head guards.
    /// Apply/read back the WHOLE transition in one non-yielding owner operation;
    /// unknown writes poison that owner, not merely this journal instance.
    fn try_apply_consensus_metadata(
        &self,
        transition: MetaTransition,
    ) -> Result<Option<Self::WriteTicket>>;
}
