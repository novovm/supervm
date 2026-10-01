//! Strict, replacement-local `round-bft/v1` signed evidence.
//!
//! This is a NEW development format, not activation of a production consensus
//! protocol or compatibility with the archived prepare/decision formats. A
//! verified signature/quorum grants no local execution, data availability,
//! current-round signing, canonical-parent or finality authority. In particular,
//! the signing primitives below are crate-private arithmetic operations, NOT a
//! durable signer: the signing journal enforces persist-before-emit.
//!
//! Fixed-width, big-endian encoding bounds allocations before parsing. A nil
//! value is a distinct tag with a mandatory zero payload; zero is not a block.
//! Certificate votes have one exact context/round/phase/value and strictly
//! increasing signer IDs. No cross-round decision aggregation is supported.

use anyhow::{ensure, Context as _, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub type Hash = [u8; 32];
pub const PROTOCOL: &str = "round-bft/v1";
/// Local format bound, not a selected production validator count.
pub const MAX_VALIDATORS: usize = 1024;
const MAGIC: &[u8; 8] = b"NOVRBFT1";
const VERSION: u16 = 1;
const PREFIX_BYTES: usize = 11;
const CONTEXT_BYTES: usize = 184;
const VOTE_BYTES: usize = CONTEXT_BYTES + 8 + 1 + 1 + 32 + 32 + 64;
const PROPOSAL_BYTES: usize = CONTEXT_BYTES + 8 + 32 + 1 + 8 + 32 + 64;
pub const MAX_WIRE_BYTES: usize = PREFIX_BYTES + 4 + MAX_VALIDATORS * VOTE_BYTES;
const VOTE_DOMAIN: &[u8] = b"novovm-round-bft/v1/vote\0";
const PROPOSAL_DOMAIN: &[u8] = b"novovm-round-bft/v1/proposal\0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Validator {
    id: Hash,
    public_key: Hash,
    weight: u64,
}

impl Validator {
    pub fn new(public_key: Hash, weight: u64) -> Result<Self> {
        ensure!(weight != 0, "validator weight must be nonzero");
        let key = VerifyingKey::from_bytes(&public_key).context("invalid validator public key")?;
        ensure!(!key.is_weak(), "weak validator public key");
        let id = digest(b"novovm-round-bft/v1/validator-id\0", &public_key);
        Ok(Self {
            id,
            public_key,
            weight,
        })
    }

    pub fn id(&self) -> Hash {
        self.id
    }
    pub fn public_key(&self) -> &Hash {
        &self.public_key
    }
    pub fn weight(&self) -> u64 {
        self.weight
    }
}

/// Immutable, constructor-validated set. There is deliberately no public
/// deserializer capable of attaching a claimed hash or quorum weight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatorSet {
    chain_id: u64,
    epoch: u64,
    activation_height: u64,
    members: Vec<Validator>,
    total_weight: u64,
    quorum_weight: u64,
    hash: Hash,
}

impl ValidatorSet {
    pub fn new(
        chain_id: u64,
        epoch: u64,
        activation_height: u64,
        mut members: Vec<Validator>,
    ) -> Result<Self> {
        ensure!(
            chain_id != 0 && epoch != 0 && activation_height != 0,
            "validator-set domain must be nonzero"
        );
        ensure!(
            !members.is_empty() && members.len() <= MAX_VALIDATORS,
            "invalid validator-set size"
        );
        members.sort_by_key(Validator::id);
        ensure!(
            members.windows(2).all(|pair| pair[0].id < pair[1].id),
            "duplicate validator"
        );
        let total_weight = members.iter().try_fold(0u64, |sum, member| {
            sum.checked_add(member.weight)
                .context("validator weight overflow")
        })?;
        let quorum_weight = ((u128::from(total_weight) * 2) / 3 + 1) as u64;
        let mut bytes = Vec::with_capacity(48 + members.len() * 72);
        for value in [
            chain_id,
            epoch,
            activation_height,
            total_weight,
            quorum_weight,
            members.len() as u64,
        ] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        for member in &members {
            bytes.extend_from_slice(&member.id);
            bytes.extend_from_slice(&member.public_key);
            bytes.extend_from_slice(&member.weight.to_be_bytes());
        }
        let hash = digest(b"novovm-round-bft/v1/validator-set\0", &bytes);
        Ok(Self {
            chain_id,
            epoch,
            activation_height,
            members,
            total_weight,
            quorum_weight,
            hash,
        })
    }

    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn activation_height(&self) -> u64 {
        self.activation_height
    }
    pub fn hash(&self) -> Hash {
        self.hash
    }
    pub fn total_weight(&self) -> u64 {
        self.total_weight
    }
    pub fn quorum_weight(&self) -> u64 {
        self.quorum_weight
    }
    pub fn members(&self) -> &[Validator] {
        &self.members
    }
    pub fn member(&self, id: &Hash) -> Option<&Validator> {
        self.members
            .binary_search_by_key(id, Validator::id)
            .ok()
            .map(|index| &self.members[index])
    }

    /// Sorted-ID round robin, deliberately not weighted proposer selection.
    /// Caller still has to establish the CURRENT height/round independently.
    pub fn leader(&self, height: u64, round: u64) -> Result<Hash> {
        let offset = height
            .checked_sub(self.activation_height)
            .and_then(|offset| offset.checked_add(round))
            .context("leader height/round overflow or inactive set")?;
        Ok(self.members[(offset % self.members.len() as u64) as usize].id)
    }
}

/// Public, untrusted context. Matching the set does not approve the genesis,
/// protocol or parent supplied here; the local authority must pin those fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub chain_id: u64,
    pub genesis_config_commitment: Hash,
    pub protocol_commitment: Hash,
    pub epoch: u64,
    pub validator_set_hash: Hash,
    pub height: u64,
    pub parent_block_hash: Hash,
    pub parent_decision_hash: Hash,
}

impl Context {
    pub(crate) fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.chain_id != 0 && self.epoch != 0 && self.height != 0,
            "consensus context scalar domain is zero"
        );
        ensure!(
            self.genesis_config_commitment != [0; 32]
                && self.protocol_commitment != [0; 32]
                && self.validator_set_hash != [0; 32],
            "consensus context commitment is zero"
        );
        if self.height == 1 {
            ensure!(
                self.parent_block_hash == [0; 32] && self.parent_decision_hash == [0; 32],
                "first block must have zero parent block and decision"
            );
        } else {
            ensure!(
                self.parent_block_hash != [0; 32] && self.parent_decision_hash != [0; 32],
                "successor requires parent block and decision"
            );
        }
        Ok(())
    }

    pub fn validate(&self, set: &ValidatorSet) -> Result<()> {
        self.validate_shape()?;
        ensure!(
            self.chain_id == set.chain_id
                && self.epoch == set.epoch
                && self.validator_set_hash == set.hash
                && self.height >= set.activation_height,
            "consensus context differs from validator set"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Prevote,
    Precommit,
}

/// Untrusted signed object. Deserialize/decode is not signature verification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vote {
    pub context: Context,
    pub round: u64,
    pub phase: Phase,
    pub value: Option<Hash>,
    pub validator_id: Hash,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct VerifiedVote {
    vote: Vote,
}
impl VerifiedVote {
    pub fn vote(&self) -> &Vote {
        &self.vote
    }
}

impl Vote {
    fn validate_shape(&self) -> Result<()> {
        self.context.validate_shape()?;
        ensure!(
            self.validator_id != [0; 32] && self.value != Some([0; 32]),
            "invalid vote signer/value"
        );
        ensure!(
            self.signature.len() == 64,
            "vote signature must be exactly 64 bytes"
        );
        Ok(())
    }

    pub fn verify(&self, set: &ValidatorSet) -> Result<VerifiedVote> {
        self.verify_signature(set)?;
        Ok(VerifiedVote { vote: self.clone() })
    }

    fn verify_signature(&self, set: &ValidatorSet) -> Result<()> {
        self.validate_shape()?;
        self.context.validate(set)?;
        let member = set
            .member(&self.validator_id)
            .context("vote signer is not a validator")?;
        VerifyingKey::from_bytes(member.public_key())?
            .verify_strict(&self.message(), &Signature::from_slice(&self.signature)?)
            .context("invalid round-bft vote signature")
    }

    /// Cryptographic operation only. No local lock, nonce, execution or durable
    /// write is performed. The caller MUST persist its safety state before emit.
    pub(crate) fn sign(
        context: Context,
        round: u64,
        phase: Phase,
        value: Option<Hash>,
        set: &ValidatorSet,
        key: &SigningKey,
    ) -> Result<Self> {
        let member = Validator::new(key.verifying_key().to_bytes(), 1)?;
        let mut vote = Self {
            context,
            round,
            phase,
            value,
            validator_id: member.id,
            signature: vec![0; 64],
        };
        vote.validate_shape()?;
        vote.context.validate(set)?;
        ensure!(
            set.member(&vote.validator_id).is_some(),
            "vote signer is not a validator"
        );
        vote.signature = key.sign(&vote.message()).to_bytes().to_vec();
        vote.verify_signature(set)?;
        Ok(vote)
    }

    fn message(&self) -> Hash {
        let mut bytes = Vec::with_capacity(VOTE_BYTES - 64);
        append_vote_unsigned(&mut bytes, self);
        digest(VOTE_DOMAIN, &bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub context: Context,
    pub round: u64,
    pub value: Hash,
    pub valid_round: Option<u64>,
    pub proposer_id: Hash,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct VerifiedProposal {
    proposal: Proposal,
}
impl VerifiedProposal {
    pub fn proposal(&self) -> &Proposal {
        &self.proposal
    }
}

impl Proposal {
    fn validate_shape(&self) -> Result<()> {
        self.context.validate_shape()?;
        ensure!(
            self.value != [0; 32] && self.proposer_id != [0; 32],
            "invalid proposal value/signer"
        );
        ensure!(
            self.valid_round.is_none_or(|round| round < self.round),
            "proposal valid_round must precede its round"
        );
        ensure!(
            self.signature.len() == 64,
            "proposal signature must be exactly 64 bytes"
        );
        Ok(())
    }

    pub fn verify(&self, set: &ValidatorSet) -> Result<VerifiedProposal> {
        self.verify_signature(set)?;
        Ok(VerifiedProposal {
            proposal: self.clone(),
        })
    }

    fn verify_signature(&self, set: &ValidatorSet) -> Result<()> {
        self.validate_shape()?;
        self.context.validate(set)?;
        ensure!(
            self.proposer_id == set.leader(self.context.height, self.round)?,
            "proposal signer is not the scheduled leader"
        );
        let member = set
            .member(&self.proposer_id)
            .context("unknown proposal signer")?;
        VerifyingKey::from_bytes(member.public_key())?
            .verify_strict(&self.message(), &Signature::from_slice(&self.signature)?)
            .context("invalid round-bft proposal signature")
    }

    /// A claimed `valid_round` is signed metadata, NOT evidence of its prevote
    /// quorum. The state machine must separately demand that exact certificate.
    pub(crate) fn sign(
        context: Context,
        round: u64,
        value: Hash,
        valid_round: Option<u64>,
        set: &ValidatorSet,
        key: &SigningKey,
    ) -> Result<Self> {
        let member = Validator::new(key.verifying_key().to_bytes(), 1)?;
        let mut proposal = Self {
            context,
            round,
            value,
            valid_round,
            proposer_id: member.id,
            signature: vec![0; 64],
        };
        proposal.validate_shape()?;
        proposal.context.validate(set)?;
        ensure!(
            proposal.proposer_id == set.leader(context.height, round)?,
            "proposal signer is not the scheduled leader"
        );
        proposal.signature = key.sign(&proposal.message()).to_bytes().to_vec();
        proposal.verify_signature(set)?;
        Ok(proposal)
    }

    fn message(&self) -> Hash {
        let mut bytes = Vec::with_capacity(PROPOSAL_BYTES - 64);
        append_proposal_unsigned(&mut bytes, self);
        digest(PROPOSAL_DOMAIN, &bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quorum {
    pub votes: Vec<Vote>,
}

#[derive(Clone, Debug)]
pub struct VerifiedQuorum {
    quorum: Quorum,
    signed_weight: u64,
}
impl VerifiedQuorum {
    pub fn context(&self) -> &Context {
        &self.quorum.votes[0].context
    }
    pub fn round(&self) -> u64 {
        self.quorum.votes[0].round
    }
    pub fn phase(&self) -> Phase {
        self.quorum.votes[0].phase
    }
    pub fn value(&self) -> Option<Hash> {
        self.quorum.votes[0].value
    }
    pub fn quorum(&self) -> &Quorum {
        &self.quorum
    }
    pub fn signed_weight(&self) -> u64 {
        self.signed_weight
    }
}

impl Quorum {
    pub fn from_votes(set: &ValidatorSet, mut votes: Vec<Vote>) -> Result<Self> {
        ensure!(
            !votes.is_empty() && votes.len() <= set.members.len(),
            "invalid quorum vote count"
        );
        votes.sort_by_key(|vote| vote.validator_id);
        let quorum = Self { votes };
        quorum.checked_weight(set)?;
        Ok(quorum)
    }

    pub fn verify(&self, set: &ValidatorSet) -> Result<VerifiedQuorum> {
        let signed_weight = self.checked_weight(set)?;
        Ok(VerifiedQuorum {
            quorum: self.clone(),
            signed_weight,
        })
    }

    fn validate_shape(&self) -> Result<()> {
        ensure!(
            !self.votes.is_empty() && self.votes.len() <= MAX_VALIDATORS,
            "invalid quorum vote count"
        );
        let first = &self.votes[0];
        for (index, vote) in self.votes.iter().enumerate() {
            vote.validate_shape()?;
            ensure!(
                vote.context == first.context
                    && vote.round == first.round
                    && vote.phase == first.phase
                    && vote.value == first.value,
                "quorum mixes context/round/phase/value"
            );
            ensure!(
                index == 0 || self.votes[index - 1].validator_id < vote.validator_id,
                "quorum signers must be sorted and unique"
            );
        }
        Ok(())
    }

    fn checked_weight(&self, set: &ValidatorSet) -> Result<u64> {
        self.validate_shape()?;
        ensure!(
            self.votes.len() <= set.members.len(),
            "quorum exceeds validator set"
        );
        let mut weight = 0u64;
        for vote in &self.votes {
            vote.verify_signature(set)?;
            weight = weight
                .checked_add(
                    set.member(&vote.validator_id)
                        .context("unknown quorum signer")?
                        .weight,
                )
                .context("quorum signed weight overflow")?;
        }
        ensure!(weight >= set.quorum_weight, "insufficient quorum weight");
        Ok(weight)
    }
}

fn digest(domain: &[u8], bytes: &[u8]) -> Hash {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(bytes);
    hash.finalize().into()
}

fn append_context(bytes: &mut Vec<u8>, context: &Context) {
    bytes.extend_from_slice(&context.chain_id.to_be_bytes());
    bytes.extend_from_slice(&context.genesis_config_commitment);
    bytes.extend_from_slice(&context.protocol_commitment);
    bytes.extend_from_slice(&context.epoch.to_be_bytes());
    bytes.extend_from_slice(&context.validator_set_hash);
    bytes.extend_from_slice(&context.height.to_be_bytes());
    bytes.extend_from_slice(&context.parent_block_hash);
    bytes.extend_from_slice(&context.parent_decision_hash);
}
fn append_vote_unsigned(bytes: &mut Vec<u8>, vote: &Vote) {
    append_context(bytes, &vote.context);
    bytes.extend_from_slice(&vote.round.to_be_bytes());
    bytes.push(match vote.phase {
        Phase::Prevote => 0,
        Phase::Precommit => 1,
    });
    bytes.push(u8::from(vote.value.is_some()));
    bytes.extend_from_slice(&vote.value.unwrap_or([0; 32]));
    bytes.extend_from_slice(&vote.validator_id);
}
fn append_proposal_unsigned(bytes: &mut Vec<u8>, proposal: &Proposal) {
    append_context(bytes, &proposal.context);
    bytes.extend_from_slice(&proposal.round.to_be_bytes());
    bytes.extend_from_slice(&proposal.value);
    bytes.push(u8::from(proposal.valid_round.is_some()));
    bytes.extend_from_slice(&proposal.valid_round.unwrap_or(0).to_be_bytes());
    bytes.extend_from_slice(&proposal.proposer_id);
}
fn prefix(kind: u8, capacity: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&VERSION.to_be_bytes());
    bytes.push(kind);
    bytes
}

pub fn encode_vote(vote: &Vote) -> Result<Vec<u8>> {
    vote.validate_shape()?;
    let mut bytes = prefix(1, PREFIX_BYTES + VOTE_BYTES);
    append_vote_unsigned(&mut bytes, vote);
    bytes.extend_from_slice(&vote.signature);
    Ok(bytes)
}
pub fn encode_proposal(proposal: &Proposal) -> Result<Vec<u8>> {
    proposal.validate_shape()?;
    let mut bytes = prefix(2, PREFIX_BYTES + PROPOSAL_BYTES);
    append_proposal_unsigned(&mut bytes, proposal);
    bytes.extend_from_slice(&proposal.signature);
    Ok(bytes)
}
pub fn encode_quorum(quorum: &Quorum) -> Result<Vec<u8>> {
    quorum.validate_shape()?;
    let mut bytes = prefix(3, PREFIX_BYTES + 4 + quorum.votes.len() * VOTE_BYTES);
    bytes.extend_from_slice(&(quorum.votes.len() as u32).to_be_bytes());
    for vote in &quorum.votes {
        append_vote_unsigned(&mut bytes, vote);
        bytes.extend_from_slice(&vote.signature);
    }
    Ok(bytes)
}

struct Cursor<'a> {
    bytes: &'a [u8],
}
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8], kind: u8) -> Result<Self> {
        ensure!(
            bytes.len() >= PREFIX_BYTES && bytes.len() <= MAX_WIRE_BYTES,
            "invalid consensus wire length"
        );
        ensure!(
            &bytes[..8] == MAGIC && bytes[8..10] == VERSION.to_be_bytes() && bytes[10] == kind,
            "unknown consensus wire format/version/kind"
        );
        Ok(Self {
            bytes: &bytes[PREFIX_BYTES..],
        })
    }
    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        let prefix = self.bytes.get(..N).context("truncated consensus wire")?;
        let result = prefix
            .try_into()
            .context("consensus field width mismatch")?;
        self.bytes = &self.bytes[N..];
        Ok(result)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take::<1>()?[0])
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take()?))
    }
    fn context(&mut self) -> Result<Context> {
        let context = Context {
            chain_id: self.u64()?,
            genesis_config_commitment: self.take()?,
            protocol_commitment: self.take()?,
            epoch: self.u64()?,
            validator_set_hash: self.take()?,
            height: self.u64()?,
            parent_block_hash: self.take()?,
            parent_decision_hash: self.take()?,
        };
        context.validate_shape()?;
        Ok(context)
    }
    fn vote(&mut self) -> Result<Vote> {
        let context = self.context()?;
        let round = self.u64()?;
        let phase = match self.byte()? {
            0 => Phase::Prevote,
            1 => Phase::Precommit,
            _ => anyhow::bail!("unknown consensus vote phase"),
        };
        let tag = self.byte()?;
        let value = self.take::<32>()?;
        let value = match tag {
            0 if value == [0; 32] => None,
            1 if value != [0; 32] => Some(value),
            _ => anyhow::bail!("noncanonical consensus vote value"),
        };
        let validator_id = self.take()?;
        let signature = self.take::<64>()?.to_vec();
        let vote = Vote {
            context,
            round,
            phase,
            value,
            validator_id,
            signature,
        };
        vote.validate_shape()?;
        Ok(vote)
    }
}

pub fn decode_vote(bytes: &[u8]) -> Result<Vote> {
    ensure!(
        bytes.len() == PREFIX_BYTES + VOTE_BYTES,
        "invalid vote wire length"
    );
    let mut cursor = Cursor::new(bytes, 1)?;
    let vote = cursor.vote()?;
    ensure!(cursor.bytes.is_empty(), "trailing vote bytes");
    Ok(vote)
}
pub fn decode_proposal(bytes: &[u8]) -> Result<Proposal> {
    ensure!(
        bytes.len() == PREFIX_BYTES + PROPOSAL_BYTES,
        "invalid proposal wire length"
    );
    let mut cursor = Cursor::new(bytes, 2)?;
    let context = cursor.context()?;
    let round = cursor.u64()?;
    let value = cursor.take()?;
    let tag = cursor.byte()?;
    let encoded_round = cursor.u64()?;
    let valid_round = match tag {
        0 if encoded_round == 0 => None,
        1 => Some(encoded_round),
        _ => anyhow::bail!("noncanonical proposal valid_round"),
    };
    let proposer_id = cursor.take()?;
    let signature = cursor.take::<64>()?.to_vec();
    ensure!(cursor.bytes.is_empty(), "trailing proposal bytes");
    let proposal = Proposal {
        context,
        round,
        value,
        valid_round,
        proposer_id,
        signature,
    };
    proposal.validate_shape()?;
    Ok(proposal)
}
pub fn decode_quorum(bytes: &[u8]) -> Result<Quorum> {
    let mut cursor = Cursor::new(bytes, 3)?;
    let count = u32::from_be_bytes(cursor.take()?) as usize;
    ensure!(
        (1..=MAX_VALIDATORS).contains(&count),
        "invalid quorum wire count"
    );
    ensure!(
        cursor.bytes.len() == count * VOTE_BYTES,
        "quorum count/length mismatch"
    );
    // Only after the hard count and exact backing byte length have been checked.
    let mut votes = Vec::with_capacity(count);
    for _ in 0..count {
        votes.push(cursor.vote()?);
    }
    ensure!(cursor.bytes.is_empty(), "trailing quorum bytes");
    let quorum = Quorum { votes };
    quorum.validate_shape()?;
    Ok(quorum)
}

#[cfg(test)]
mod tests;
