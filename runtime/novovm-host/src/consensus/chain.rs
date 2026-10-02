//! Bounded local chain metadata and cold, forward recovery on the existing I/O
//! owner. A certificate locator is local storage data, NEVER a block identity.
//! Recovery verifies content and quorum evidence; it does not re-execute the
//! business program or manufacture a zero-knowledge execution validity proof.

use super::journal::codec::decode_archived_decision;
use super::statement::{BlockStatement, ParentPoint};
use super::wire::{Context as ConsensusContext, Hash, Phase, ValidatorSet};
use crate::persistence::io::IoTicket;
use crate::persistence::metadata::{MetaKey, MetadataSnapshot};
use crate::persistence::{PreparedCandidate, StoredCandidate};
use crate::pipeline::CandidatePipeline;
use anyhow::{ensure, Context, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

const RECORD_MAGIC: &[u8; 8] = b"NVCHAIN1";
const HEAD_MAGIC: &[u8; 8] = b"NVHEAD01";
const MAX_RECORD_BYTES: usize = 4096;

/// The decided VALUE identity is independent of round, signatures, certificate
/// subset, proposer, and local outbox location. Callers separately verify the
/// context and non-nil value; hashing alone is not a decision certificate.
pub(crate) fn decision_id(context: &ConsensusContext, value: Hash) -> Hash {
    let mut hash = Sha256::new();
    hash.update(b"novovm/replacement/decided-value/round-bft-v1\0");
    hash.update(context.chain_id.to_be_bytes());
    hash.update(context.genesis_config_commitment);
    hash.update(context.protocol_commitment);
    hash.update(context.epoch.to_be_bytes());
    hash.update(context.validator_set_hash);
    hash.update(context.height.to_be_bytes());
    hash.update(context.parent_block_hash);
    hash.update(context.parent_decision_hash);
    hash.update(value);
    hash.finalize().into()
}

pub(crate) fn archived_outbox_digest(bytes: &[u8]) -> Hash {
    let mut hash = Sha256::new();
    hash.update(b"novovm/replacement/chain-outbox/v1\0");
    hash.update(bytes);
    hash.finalize().into()
}

/// An archive entry's proof is the exact immutable decision outbox, avoiding a
/// second copy of the maximum-sized QC in the same atomic metadata transaction.
/// Decoding this local record confers no execution or signing capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChainRecord {
    parent: ParentPoint,
    context: ConsensusContext,
    point: ParentPoint,
    candidate_id: Hash,
    document_digest: Hash,
    outbox_validator: Hash,
    outbox_revision: u64,
    outbox_digest: Hash,
}

impl ChainRecord {
    pub(crate) fn new(
        statement: &BlockStatement,
        packet: &PreparedCandidate,
        parent: ParentPoint,
        validator: Hash,
        revision: u64,
        outbox: &[u8],
    ) -> Result<Self> {
        ensure!(
            statement.matches_packet(packet),
            "chain statement/packet mismatch"
        );
        statement.validate_parent(&parent)?;
        let context = *statement.context();
        let value = statement.hash();
        let record = Self {
            parent,
            context,
            point: ParentPoint {
                height: context.height,
                block_hash: value,
                state_root: packet.state_root(),
                receipt_batch_commitment: packet.receipt_batch_commitment(),
                state_version: statement.state_version(),
                decision_hash: decision_id(&context, value),
            },
            candidate_id: packet.candidate_id(),
            document_digest: packet.document_digest(),
            outbox_validator: validator,
            outbox_revision: revision,
            outbox_digest: archived_outbox_digest(outbox),
        };
        record.validate_shape()?;
        // The journal has already verified the signatures. Still bind the
        // selected outbox's exact decision to this value before packaging it.
        let (proposal, quorum) = decode_archived_decision(outbox, revision)?;
        ensure!(
            proposal.context == context && proposal.value == value,
            "chain outbox proposal differs from executed statement"
        );
        ensure!(
            quorum.votes.iter().all(|vote| {
                vote.context == context
                    && vote.round == proposal.round
                    && vote.phase == Phase::Precommit
                    && vote.value == Some(value)
            }) && !quorum.votes.is_empty(),
            "chain outbox certificate differs from executed statement"
        );
        Ok(record)
    }

    pub(crate) fn parent(&self) -> ParentPoint {
        self.parent
    }
    pub(crate) fn context(&self) -> ConsensusContext {
        self.context
    }
    pub(crate) fn point(&self) -> ParentPoint {
        self.point
    }
    pub(crate) fn candidate_id(&self) -> Hash {
        self.candidate_id
    }
    pub(crate) fn outbox_key(&self) -> MetaKey {
        MetaKey::ConsensusOutbox {
            validator: self.outbox_validator,
            sequence: self.outbox_revision,
        }
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>> {
        self.validate_shape()?;
        encode_local(RECORD_MAGIC, self)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self> {
        let record: Self = decode_local(RECORD_MAGIC, bytes)?;
        record.validate_shape()?;
        Ok(record)
    }

    pub(crate) fn head_bytes(&self) -> Result<Vec<u8>> {
        let record = self.encode()?;
        encode_local(
            HEAD_MAGIC,
            &Head {
                height: self.point.height,
                block_hash: self.point.block_hash,
                record_digest: record_digest(&record),
            },
        )
    }

    fn validate_shape(&self) -> Result<()> {
        self.context.validate_shape()?;
        ensure!(
            self.parent.height.checked_add(1) == Some(self.point.height)
                && self.point.height == self.context.height
                && self.parent.block_hash == self.context.parent_block_hash
                && self.parent.decision_hash == self.context.parent_decision_hash,
            "chain record parent/context/height mismatch"
        );
        ensure!(
            self.parent.state_root != [0; 32]
                && self.parent.receipt_batch_commitment != [0; 32]
                && self.point.block_hash != [0; 32]
                && self.point.state_root != [0; 32]
                && self.point.receipt_batch_commitment != [0; 32]
                && self.point.state_version > self.parent.state_version
                && self.candidate_id != [0; 32]
                && self.document_digest != [0; 32]
                && self.outbox_revision != 0,
            "chain record contains invalid root/count/locator"
        );
        ensure!(
            self.point.decision_hash == decision_id(&self.context, self.point.block_hash),
            "chain decision identity mismatch"
        );
        Ok(())
    }

    fn verify_prefix(
        &self,
        genesis: &ConsensusContext,
        parent: &ParentPoint,
        set: &ValidatorSet,
    ) -> Result<()> {
        self.validate_shape()?;
        let expected = ConsensusContext {
            height: parent
                .height
                .checked_add(1)
                .context("chain height exhausted")?,
            parent_block_hash: parent.block_hash,
            parent_decision_hash: parent.decision_hash,
            ..*genesis
        };
        ensure!(
            self.parent == *parent && self.context == expected,
            "chain record does not extend exact verified prefix"
        );
        self.context.validate(set)
    }

    fn verify_outbox(&self, bytes: &[u8], set: &ValidatorSet) -> Result<()> {
        ensure!(
            archived_outbox_digest(bytes) == self.outbox_digest,
            "chain decision outbox digest mismatch"
        );
        ensure!(
            set.member(&self.outbox_validator).is_some(),
            "chain outbox belongs to unknown validator"
        );
        let (proposal, quorum) = decode_archived_decision(bytes, self.outbox_revision)?;
        proposal.verify(set)?;
        let quorum = quorum.verify(set)?;
        ensure!(
            proposal.context == self.context
                && proposal.value == self.point.block_hash
                && quorum.context() == &self.context
                && quorum.round() == proposal.round
                && quorum.phase() == Phase::Precommit
                && quorum.value() == Some(self.point.block_hash),
            "archived proposal/precommit certificate does not bind chain value"
        );
        Ok(())
    }

    fn verify_candidate(&self, stored: &StoredCandidate, set: &ValidatorSet) -> Result<()> {
        ensure!(
            stored.candidate_id() == self.candidate_id
                && stored.document_digest() == self.document_digest,
            "archived candidate identity/document mismatch"
        );
        let statement = BlockStatement::from_stored(stored, self.context, set, &self.parent)?;
        ensure!(
            statement.hash() == self.point.block_hash
                && statement.state_version() == self.point.state_version
                && stored.state_root() == self.point.state_root
                && stored.receipt_batch_commitment() == self.point.receipt_batch_commitment,
            "archived candidate statement/root mismatch"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Head {
    height: u64,
    block_hash: Hash,
    record_digest: Hash,
}

impl Head {
    fn decode(bytes: &[u8]) -> Result<Self> {
        let head: Self = decode_local(HEAD_MAGIC, bytes)?;
        ensure!(
            head.height != 0 && head.block_hash != [0; 32] && head.record_digest != [0; 32],
            "invalid chain head fields"
        );
        Ok(head)
    }

    fn verify_record(&self, record: &ChainRecord) -> Result<()> {
        ensure!(
            self.height == record.point.height
                && self.block_hash == record.point.block_hash
                && self.record_digest == record_digest(&record.encode()?),
            "chain head/archive binding mismatch"
        );
        Ok(())
    }
}

fn record_digest(bytes: &[u8]) -> Hash {
    let mut hash = Sha256::new();
    hash.update(b"novovm/replacement/chain-record/v1\0");
    hash.update(bytes);
    hash.finalize().into()
}

fn codec_digest(bytes: &[u8]) -> Hash {
    let mut hash = Sha256::new();
    hash.update(b"novovm/replacement/chain-codec/v1\0");
    hash.update(bytes);
    hash.finalize().into()
}

fn encode_local<T: Serialize>(magic: &[u8; 8], value: &T) -> Result<Vec<u8>> {
    let payload = postcard::to_allocvec(value)?;
    ensure!(
        payload.len() <= MAX_RECORD_BYTES - 40,
        "chain metadata exceeds bound"
    );
    let mut bytes = Vec::with_capacity(40 + payload.len());
    bytes.extend_from_slice(magic);
    bytes.extend_from_slice(&payload);
    bytes.extend_from_slice(&codec_digest(&bytes));
    Ok(bytes)
}

fn decode_local<T: Serialize + DeserializeOwned>(magic: &[u8; 8], bytes: &[u8]) -> Result<T> {
    ensure!(
        (40..=MAX_RECORD_BYTES).contains(&bytes.len()) && bytes.starts_with(magic),
        "invalid chain metadata version/length"
    );
    let (body, expected) = bytes.split_at(bytes.len() - 32);
    ensure!(
        codec_digest(body).as_slice() == expected,
        "chain metadata digest mismatch"
    );
    let (value, tail) = postcard::take_from_bytes::<T>(&body[8..])?;
    ensure!(tail.is_empty(), "trailing chain metadata bytes");
    ensure!(
        encode_local(magic, &value)? == bytes,
        "noncanonical chain metadata"
    );
    Ok(value)
}

pub(crate) struct RecoveredChain {
    pub(crate) record: Option<ChainRecord>,
    pub(crate) head_bytes: Option<Vec<u8>>,
}

enum RecoveryStage {
    Empty(Option<IoTicket<MetadataSnapshot>>),
    Block(Option<IoTicket<MetadataSnapshot>>),
    Outbox {
        record: ChainRecord,
        ticket: Option<IoTicket<MetadataSnapshot>>,
    },
    Candidate {
        record: ChainRecord,
        ticket: Option<IoTicket<Option<StoredCandidate>>>,
    },
    Head {
        record: ChainRecord,
        ticket: Option<IoTicket<MetadataSnapshot>>,
    },
    Finished,
}

/// Cold-only, O(history) recovery. Each poll advances at most one bounded stage;
/// no complete-history vector, repair, or authority cache is created. The final
/// exact head recheck is still not a substitute for each signing CAS's guard.
pub(crate) struct ChainRecovery {
    genesis: ConsensusContext,
    current_parent: ParentPoint,
    set: Arc<ValidatorSet>,
    expected_head: Option<Head>,
    head_bytes: Option<Vec<u8>>,
    owner: Option<Arc<()>>,
    stage: RecoveryStage,
}

impl ChainRecovery {
    pub(crate) fn new(
        genesis_context: ConsensusContext,
        genesis_parent: ParentPoint,
        set: Arc<ValidatorSet>,
        head_bytes: Option<Vec<u8>>,
    ) -> Result<Self> {
        genesis_context.validate(&set)?;
        ensure!(
            genesis_context.height == 1
                && genesis_parent.height == 0
                && genesis_parent.block_hash == [0; 32]
                && genesis_parent.decision_hash == [0; 32]
                && genesis_parent.state_root != [0; 32]
                && genesis_parent.receipt_batch_commitment != [0; 32],
            "chain recovery requires configured first-height genesis anchor"
        );
        let expected_head = head_bytes.as_deref().map(Head::decode).transpose()?;
        let stage = if expected_head.is_some() {
            RecoveryStage::Block(None)
        } else {
            RecoveryStage::Empty(None)
        };
        Ok(Self {
            genesis: genesis_context,
            current_parent: genesis_parent,
            set,
            expected_head,
            head_bytes,
            owner: None,
            stage,
        })
    }

    pub(crate) fn poll(&mut self, pipeline: &CandidatePipeline) -> Result<Option<RecoveredChain>> {
        ensure!(
            !matches!(self.stage, RecoveryStage::Finished),
            "chain recovery finished or failed; explicit reopen required"
        );
        // Replace first so any error is sticky, including an owner/domain error.
        let stage = std::mem::replace(&mut self.stage, RecoveryStage::Finished);
        let domain = pipeline.storage_domain();
        ensure!(
            domain.chain_id == self.genesis.chain_id
                && domain.genesis_config_commitment == self.genesis.genesis_config_commitment
                && domain.protocol_commitment == self.genesis.protocol_commitment,
            "chain recovery storage domain mismatch"
        );
        let owner = pipeline.owner_identity();
        if let Some(expected) = &self.owner {
            ensure!(
                Arc::ptr_eq(expected, &owner),
                "chain recovery pipeline changed"
            );
        } else {
            self.owner = Some(owner);
        }
        match stage {
            RecoveryStage::Empty(mut ticket) => {
                if ticket.is_none() {
                    ticket = pipeline.try_read_consensus_metadata(vec![
                        MetaKey::ChainHead,
                        MetaKey::ChainBlock { height: 1 },
                    ])?;
                }
                let Some(reply) = take_reply(&mut ticket)? else {
                    self.stage = RecoveryStage::Empty(ticket);
                    return Ok(None);
                };
                ensure!(
                    reply.values.len() == 2 && reply.values.iter().all(Option::is_none),
                    "missing chain head has archived content or changed during recovery"
                );
                Ok(Some(RecoveredChain {
                    record: None,
                    head_bytes: None,
                }))
            }
            RecoveryStage::Block(mut ticket) => {
                let height = self
                    .current_parent
                    .height
                    .checked_add(1)
                    .context("chain height exhausted")?;
                if ticket.is_none() {
                    ticket = pipeline
                        .try_read_consensus_metadata(vec![MetaKey::ChainBlock { height }])?;
                }
                let Some(reply) = take_reply(&mut ticket)? else {
                    self.stage = RecoveryStage::Block(ticket);
                    return Ok(None);
                };
                let record =
                    ChainRecord::decode(&required_value(reply, "decided chain block missing")?)?;
                record.verify_prefix(&self.genesis, &self.current_parent, &self.set)?;
                self.stage = RecoveryStage::Outbox {
                    record,
                    ticket: None,
                };
                Ok(None)
            }
            RecoveryStage::Outbox { record, mut ticket } => {
                if ticket.is_none() {
                    ticket = pipeline.try_read_consensus_metadata(vec![record.outbox_key()])?;
                }
                let Some(reply) = take_reply(&mut ticket)? else {
                    self.stage = RecoveryStage::Outbox { record, ticket };
                    return Ok(None);
                };
                record.verify_outbox(
                    &required_value(reply, "decided chain outbox missing")?,
                    &self.set,
                )?;
                self.stage = RecoveryStage::Candidate {
                    record,
                    ticket: None,
                };
                Ok(None)
            }
            RecoveryStage::Candidate { record, mut ticket } => {
                if ticket.is_none() {
                    ticket = pipeline.try_recover_consensus_candidate(record.candidate_id())?;
                }
                let Some(reply) = take_reply(&mut ticket)? else {
                    self.stage = RecoveryStage::Candidate { record, ticket };
                    return Ok(None);
                };
                let stored = reply.context("decided chain candidate missing")?;
                record.verify_candidate(&stored, &self.set)?;
                let head = self.expected_head.as_ref().context("chain head missing")?;
                ensure!(
                    record.point.height <= head.height,
                    "chain recovery passed declared head"
                );
                if record.point.height == head.height {
                    head.verify_record(&record)?;
                    self.stage = RecoveryStage::Head {
                        record,
                        ticket: None,
                    };
                } else {
                    self.current_parent = record.point;
                    self.stage = RecoveryStage::Block(None);
                }
                Ok(None)
            }
            RecoveryStage::Head { record, mut ticket } => {
                let successor = record.point.height.checked_add(1);
                if ticket.is_none() {
                    let mut keys = vec![MetaKey::ChainHead];
                    if let Some(height) = successor {
                        keys.push(MetaKey::ChainBlock { height });
                    }
                    ticket = pipeline.try_read_consensus_metadata(keys)?;
                }
                let Some(reply) = take_reply(&mut ticket)? else {
                    self.stage = RecoveryStage::Head { record, ticket };
                    return Ok(None);
                };
                ensure!(
                    reply.values.len() == 1 + usize::from(successor.is_some())
                        && reply.values[0] == self.head_bytes,
                    "chain head changed during cold recovery"
                );
                // A mutable head and signer snapshot can be rolled back while
                // the immutable decided successor remains. Do not reopen that
                // stale signer merely because its old outbox still verifies.
                // This is not protection against rollback of the entire DB.
                ensure!(
                    successor.is_none() || reply.values[1].is_none(),
                    "decided successor exists beyond declared head; refuse partial rollback"
                );
                Ok(Some(RecoveredChain {
                    record: Some(record),
                    head_bytes: self.head_bytes.clone(),
                }))
            }
            RecoveryStage::Finished => unreachable!(),
        }
    }
}

fn take_reply<T>(ticket: &mut Option<IoTicket<T>>) -> Result<Option<T>> {
    ticket
        .as_mut()
        .map(IoTicket::try_take)
        .transpose()
        .map(Option::flatten)
}

fn required_value(snapshot: MetadataSnapshot, message: &'static str) -> Result<Vec<u8>> {
    ensure!(
        snapshot.values.len() == 1,
        "chain metadata reply count mismatch"
    );
    snapshot
        .values
        .into_iter()
        .next()
        .flatten()
        .context(message)
}

#[cfg(test)]
mod tests;
