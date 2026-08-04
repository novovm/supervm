#![forbid(unsafe_code)]

//! NOV-native proof-seal contract and local validator safety storage.
//!
//! This module is deliberately separate from the legacy consensus wire and
//! from the candidate ledger. A valid QC stored here is still only an
//! independently durable quorum attestation: it does not mutate candidate
//! lifecycle flags and does not confer canonicality, safety, or finality.

use crate::native_block_ledger::{
    NovNativeBlockCandidateRecordV1, NovNativeBlockLedgerV1, NovNativeDurableBlockV1,
};
use anyhow::{bail, Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rocksdb::{
    Direction, IteratorMode, Options as RocksDbOptions, WriteBatch as RocksDbWriteBatch,
    WriteOptions, DB,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};

pub const NOV_NATIVE_BLOCK_SEAL_STORE_SCHEMA_V1: &str = "novovm-native-block-seal-store/v1";
pub const NOV_NATIVE_BLOCK_SEAL_VALIDATOR_SET_SCHEMA_V1: &str =
    "novovm-native-block-seal-validator-set/v1";
pub const NOV_NATIVE_BLOCK_SEAL_SUBJECT_SCHEMA_V1: &str = "novovm-native-block-seal-subject/v1";
pub const NOV_NATIVE_BLOCK_SEAL_PROPOSAL_SCHEMA_V1: &str = "novovm-native-block-seal-proposal/v1";
pub const NOV_NATIVE_BLOCK_SEAL_VOTE_SCHEMA_V1: &str = "novovm-native-block-seal-vote/v1";
pub const NOV_NATIVE_BLOCK_SEAL_QC_SCHEMA_V1: &str = "novovm-native-block-seal-qc/v1";
pub const NOV_NATIVE_BLOCK_SEAL_PROTOCOL_VERSION_V1: &str = "novovm-proof-seal-bft/v1";
pub const NOV_NATIVE_BLOCK_SEAL_PROOF_VERSION_V1: &str = "novovm-native-proof-seal/v1";
pub const NOV_NATIVE_BLOCK_SEAL_VERIFICATION_PROFILE_V1: &str = "local-aoem-readback-and-body/v1";
pub const NOV_NATIVE_BLOCK_SEAL_PHASE_V1: &str = "prepare";
pub const NOV_NATIVE_BLOCK_SEAL_SIGNATURE_SCHEME_V1: &str = "ed25519";
pub const NOV_NATIVE_BLOCK_SEAL_MAX_VALIDATORS_V1: usize = 1_024;
pub const NOV_NATIVE_BLOCK_SEAL_MAX_QCS_PER_INDEX_V1: usize = 4_096;
pub const NOV_NATIVE_BLOCK_SEAL_MAX_OUTBOX_SCAN_V1: usize = 4_096;

const KEY_PREFIX_V1: &str = "native_block_seal/v1/";
const KEY_SCHEMA_V1: &[u8] = b"native_block_seal/v1/schema";
const STORE_BINDING_SCHEMA_V1: &str = "novovm-native-block-seal-store-binding/v1";
const ROUND_LOCK_SCHEMA_V1: &str = "novovm-native-block-seal-round-lock/v1";
const HEIGHT_LOCK_SCHEMA_V1: &str = "novovm-native-block-seal-height-lock/v1";
const PROPOSAL_LOCK_SCHEMA_V1: &str = "novovm-native-block-seal-proposal-lock/v1";
const VOTE_LOCK_SCHEMA_V1: &str = "novovm-native-block-seal-vote-lock/v1";
const OUTBOX_SCHEMA_V1: &str = "novovm-native-block-seal-outbox/v1";
const QC_INDEX_SCHEMA_V1: &str = "novovm-native-block-seal-qc-index/v1";
const COMPETING_QC_EVIDENCE_SCHEMA_V1: &str = "novovm-native-block-seal-competing-qc-evidence/v1";

const VALIDATOR_ID_DOMAIN_V1: &[u8] = b"novovm-native-seal-validator-id-v1\0";
const VALIDATOR_SET_HASH_DOMAIN_V1: &[u8] = b"novovm-native-seal-validator-set-v1\0";
const NETWORK_DOMAIN_COMMITMENT_DOMAIN_V1: &[u8] = b"novovm-native-seal-network-domain-v1\0";
const INLINE_BODY_COMMITMENT_DOMAIN_V1: &[u8] = b"novovm-native-seal-inline-body-commitment-v1\0";
const AOEM_PARENT_COMMITMENT_DOMAIN_V1: &[u8] = b"novovm-native-seal-aoem-parent-commitment-v1\0";
const SUBJECT_HASH_DOMAIN_V1: &[u8] = b"novovm-native-seal-subject-v1\0";
const PROPOSAL_SIGNING_DOMAIN_V1: &[u8] = b"novovm-native-seal-proposal-signing-v1\0";
const PROPOSAL_HASH_DOMAIN_V1: &[u8] = b"novovm-native-seal-proposal-hash-v1\0";
const VOTE_SIGNING_DOMAIN_V1: &[u8] = b"novovm-native-seal-vote-signing-v1\0";
const VOTE_HASH_DOMAIN_V1: &[u8] = b"novovm-native-seal-vote-hash-v1\0";
const QC_HASH_DOMAIN_V1: &[u8] = b"novovm-native-seal-qc-hash-v1\0";
const LEDGER_IDENTITY_COMMITMENT_DOMAIN_V1: &[u8] = b"novovm-native-seal-ledger-identity-v1\0";
const COMPETING_QC_EVIDENCE_DOMAIN_V1: &[u8] = b"novovm-native-seal-competing-qc-evidence-v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealValidatorV1 {
    pub validator_id: [u8; 32],
    pub public_key: [u8; 32],
    pub weight: u64,
}

impl NovNativeSealValidatorV1 {
    pub fn new(public_key: [u8; 32], weight: u64) -> Result<Self> {
        if weight == 0 {
            bail!("NOV native seal validator weight must be non-zero");
        }
        let verifying_key = VerifyingKey::from_bytes(&public_key)
            .context("NOV native seal validator public key is invalid")?;
        if verifying_key.is_weak() {
            bail!("NOV native seal validator public key is weak");
        }
        Ok(Self {
            validator_id: validator_id_v1(&public_key),
            public_key,
            weight,
        })
    }

    fn validate(&self) -> Result<()> {
        if self.weight == 0 || self.validator_id != validator_id_v1(&self.public_key) {
            bail!("NOV native seal validator identity or weight is invalid");
        }
        let verifying_key = VerifyingKey::from_bytes(&self.public_key)
            .context("NOV native seal validator public key is invalid")?;
        if verifying_key.is_weak() {
            bail!("NOV native seal validator public key is weak");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealValidatorSetV1 {
    pub schema: String,
    pub chain_id: u64,
    pub epoch: u64,
    pub activation_height: u64,
    pub validators: Vec<NovNativeSealValidatorV1>,
    pub total_weight: u64,
    pub quorum_weight: u64,
    pub validator_set_hash: [u8; 32],
}

impl NovNativeSealValidatorSetV1 {
    pub fn new(
        chain_id: u64,
        epoch: u64,
        activation_height: u64,
        mut validators: Vec<NovNativeSealValidatorV1>,
    ) -> Result<Self> {
        if chain_id == 0 || epoch == 0 || activation_height == 0 {
            bail!("NOV native seal validator set chain, epoch, and activation height must be non-zero");
        }
        if validators.is_empty() || validators.len() > NOV_NATIVE_BLOCK_SEAL_MAX_VALIDATORS_V1 {
            bail!("NOV native seal validator set size is invalid");
        }
        for validator in &validators {
            validator.validate()?;
        }
        validators.sort_by_key(|validator| validator.validator_id);
        if validators
            .windows(2)
            .any(|pair| pair[0].validator_id == pair[1].validator_id)
        {
            bail!("NOV native seal validator set contains a duplicate validator");
        }
        let total_weight = validators.iter().try_fold(0u64, |total, validator| {
            total
                .checked_add(validator.weight)
                .context("NOV native seal validator weight overflow")
        })?;
        let quorum_weight = (((total_weight as u128) * 2) / 3 + 1) as u64;
        let validator_set_hash = validator_set_hash_v1(
            chain_id,
            epoch,
            activation_height,
            validators.as_slice(),
            total_weight,
            quorum_weight,
        );
        let set = Self {
            schema: NOV_NATIVE_BLOCK_SEAL_VALIDATOR_SET_SCHEMA_V1.to_string(),
            chain_id,
            epoch,
            activation_height,
            validators,
            total_weight,
            quorum_weight,
            validator_set_hash,
        };
        set.validate()?;
        Ok(set)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema != NOV_NATIVE_BLOCK_SEAL_VALIDATOR_SET_SCHEMA_V1
            || self.chain_id == 0
            || self.epoch == 0
            || self.activation_height == 0
            || self.validators.is_empty()
            || self.validators.len() > NOV_NATIVE_BLOCK_SEAL_MAX_VALIDATORS_V1
        {
            bail!("NOV native seal validator set metadata is invalid");
        }
        let mut total_weight = 0u64;
        let mut previous = None;
        for validator in &self.validators {
            validator.validate()?;
            if previous.is_some_and(|id| id >= validator.validator_id) {
                bail!("NOV native seal validators are not strictly sorted and unique");
            }
            previous = Some(validator.validator_id);
            total_weight = total_weight
                .checked_add(validator.weight)
                .context("NOV native seal validator weight overflow")?;
        }
        let quorum_weight = (((total_weight as u128) * 2) / 3 + 1) as u64;
        let expected_hash = validator_set_hash_v1(
            self.chain_id,
            self.epoch,
            self.activation_height,
            self.validators.as_slice(),
            total_weight,
            quorum_weight,
        );
        if self.total_weight != total_weight
            || self.quorum_weight != quorum_weight
            || self.validator_set_hash != expected_hash
        {
            bail!("NOV native seal validator set commitment is invalid");
        }
        Ok(())
    }

    pub fn validator(&self, validator_id: [u8; 32]) -> Option<&NovNativeSealValidatorV1> {
        self.validators
            .binary_search_by_key(&validator_id, |validator| validator.validator_id)
            .ok()
            .map(|index| &self.validators[index])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealSubjectV1 {
    pub schema: String,
    pub protocol_version: String,
    pub proof_version: String,
    pub verification_profile: String,
    pub phase: String,
    pub chain_id: u64,
    pub epoch: u64,
    pub height: u64,
    pub slot: u64,
    pub round: u64,
    pub timestamp_unix_ms: u64,
    pub validator_set_hash: [u8; 32],
    pub justify_qc_hash: [u8; 32],
    pub genesis_block_hash: [u8; 32],
    pub network_domain_commitment: [u8; 32],
    pub block_hash: [u8; 32],
    pub parent_block_hash: [u8; 32],
    pub candidate_id: [u8; 32],
    pub execution_context_commitment: [u8; 32],
    pub protocol_config_commitment: [u8; 32],
    pub pre_state_root: [u8; 32],
    pub post_state_root: [u8; 32],
    pub post_state_root_codec: String,
    pub ordered_tx_root: [u8; 32],
    pub body_digest: [u8; 32],
    pub data_availability_scheme: String,
    pub inline_body_commitment: [u8; 32],
    pub body_bytes: u64,
    pub tx_count: u32,
    pub receipt_count: u32,
    pub block_receipt_root: [u8; 32],
    pub cumulative_receipt_root: [u8; 32],
    pub cumulative_receipt_root_codec: String,
    pub state_version: u64,
    pub aoem_parent_commitment: [u8; 32],
    pub aoem_batch_id: String,
    pub aoem_batch_result_id: String,
    pub aoem_expected_output_commitment: String,
    pub aoem_evidence_commitment: [u8; 32],
    pub execution_evidence_kind: String,
    pub subject_hash: [u8; 32],
}

impl NovNativeSealSubjectV1 {
    pub fn validate(&self, validator_set: &NovNativeSealValidatorSetV1) -> Result<()> {
        validator_set.validate()?;
        if self.schema != NOV_NATIVE_BLOCK_SEAL_SUBJECT_SCHEMA_V1
            || self.protocol_version != NOV_NATIVE_BLOCK_SEAL_PROTOCOL_VERSION_V1
            || self.proof_version != NOV_NATIVE_BLOCK_SEAL_PROOF_VERSION_V1
            || self.verification_profile != NOV_NATIVE_BLOCK_SEAL_VERIFICATION_PROFILE_V1
            || self.phase != NOV_NATIVE_BLOCK_SEAL_PHASE_V1
            || self.chain_id == 0
            || self.epoch == 0
            || self.height == 0
            || self.chain_id != validator_set.chain_id
            || self.epoch != validator_set.epoch
            || self.height < validator_set.activation_height
            || self.validator_set_hash != validator_set.validator_set_hash
        {
            bail!("NOV native seal subject protocol or validator-set binding is invalid");
        }
        if self.height == 1 {
            if self.parent_block_hash != [0u8; 32]
                || self.justify_qc_hash != [0u8; 32]
                || self.aoem_parent_commitment != [0u8; 32]
                || self.genesis_block_hash != self.block_hash
            {
                bail!("NOV native genesis seal subject has an invalid parent or justify QC");
            }
        } else if self.parent_block_hash == [0u8; 32]
            || self.justify_qc_hash == [0u8; 32]
            || self.aoem_parent_commitment == [0u8; 32]
        {
            bail!("NOV native non-genesis seal subject requires a parent and justify QC");
        }
        validate_ascii_id_v1("AOEM batch id", self.aoem_batch_id.as_str(), 512)?;
        validate_hex_commitment_v1("AOEM batch result id", self.aoem_batch_result_id.as_str())?;
        validate_hex_commitment_v1(
            "AOEM expected output commitment",
            self.aoem_expected_output_commitment.as_str(),
        )?;
        if self.post_state_root_codec != "novovm-consensus-native-state-wire/v1"
            || self.cumulative_receipt_root_codec != "novovm-consensus-receipt-wire/v1"
            || self.data_availability_scheme != "inline-full-body-digest/v1"
            || self.execution_evidence_kind != "aoem_execution_commitment_not_consensus_seal"
            || self.receipt_count != self.tx_count
        {
            bail!("NOV native seal subject codec, DA, receipt, or evidence profile is invalid");
        }
        let expected_network_domain = network_domain_commitment_v1(
            self.chain_id,
            &self.genesis_block_hash,
            &self.protocol_config_commitment,
        );
        if self.block_hash == [0u8; 32]
            || self.candidate_id == [0u8; 32]
            || self.execution_context_commitment == [0u8; 32]
            || self.protocol_config_commitment == [0u8; 32]
            || self.genesis_block_hash == [0u8; 32]
            || self.network_domain_commitment != expected_network_domain
            || self.post_state_root == [0u8; 32]
            || self.ordered_tx_root == [0u8; 32]
            || self.body_digest == [0u8; 32]
            || self.body_bytes == 0
            || self.tx_count == 0
            || self.block_receipt_root == [0u8; 32]
            || self.cumulative_receipt_root == [0u8; 32]
            || self.state_version == 0
            || self.aoem_evidence_commitment == [0u8; 32]
        {
            bail!("NOV native seal subject contains an empty required commitment");
        }
        let expected_inline_body_commitment = inline_body_commitment_v1(
            self.chain_id,
            self.height,
            &self.block_hash,
            &self.ordered_tx_root,
            &self.body_digest,
            self.body_bytes,
            self.tx_count,
        );
        if self.inline_body_commitment != expected_inline_body_commitment
            || self.subject_hash != subject_hash_v1(self)
        {
            bail!("NOV native seal subject commitment is invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealProposalV1 {
    pub schema: String,
    pub subject: NovNativeSealSubjectV1,
    pub subject_hash: [u8; 32],
    pub proposer_id: [u8; 32],
    pub signature_scheme: String,
    pub signature: Vec<u8>,
    pub proposal_hash: [u8; 32],
}

impl NovNativeSealProposalV1 {
    pub fn verify(&self, validator_set: &NovNativeSealValidatorSetV1) -> Result<()> {
        self.subject.validate(validator_set)?;
        if self.schema != NOV_NATIVE_BLOCK_SEAL_PROPOSAL_SCHEMA_V1
            || self.subject_hash != self.subject.subject_hash
            || self.signature_scheme != NOV_NATIVE_BLOCK_SEAL_SIGNATURE_SCHEME_V1
            || self.signature.len() != 64
        {
            bail!("NOV native seal proposal metadata is invalid");
        }
        let validator = validator_set
            .validator(self.proposer_id)
            .context("NOV native seal proposer is not in the validator set")?;
        let verifying_key = VerifyingKey::from_bytes(&validator.public_key)
            .context("NOV native seal proposer public key is invalid")?;
        let signature = Signature::from_slice(self.signature.as_slice())
            .context("NOV native seal proposal signature encoding is invalid")?;
        verifying_key
            .verify_strict(
                &proposal_signing_message_v1(&self.subject_hash, &self.proposer_id),
                &signature,
            )
            .context("NOV native seal proposal signature verification failed")?;
        if self.proposal_hash != proposal_hash_v1(self) {
            bail!("NOV native seal proposal hash is invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealVoteV1 {
    pub schema: String,
    pub chain_id: u64,
    pub epoch: u64,
    pub height: u64,
    pub round: u64,
    pub phase: String,
    pub validator_set_hash: [u8; 32],
    pub subject_hash: [u8; 32],
    pub proposal_hash: [u8; 32],
    pub validator_id: [u8; 32],
    pub signature_scheme: String,
    pub signature: Vec<u8>,
    pub vote_hash: [u8; 32],
}

impl NovNativeSealVoteV1 {
    pub fn verify(
        &self,
        subject: &NovNativeSealSubjectV1,
        proposal_hash: [u8; 32],
        validator_set: &NovNativeSealValidatorSetV1,
    ) -> Result<()> {
        subject.validate(validator_set)?;
        if self.schema != NOV_NATIVE_BLOCK_SEAL_VOTE_SCHEMA_V1
            || self.chain_id != subject.chain_id
            || self.epoch != subject.epoch
            || self.height != subject.height
            || self.round != subject.round
            || self.phase != subject.phase
            || self.validator_set_hash != subject.validator_set_hash
            || self.subject_hash != subject.subject_hash
            || self.proposal_hash != proposal_hash
            || self.proposal_hash == [0u8; 32]
            || self.signature_scheme != NOV_NATIVE_BLOCK_SEAL_SIGNATURE_SCHEME_V1
            || self.signature.len() != 64
        {
            bail!("NOV native seal vote metadata is invalid");
        }
        let validator = validator_set
            .validator(self.validator_id)
            .context("NOV native seal voter is not in the validator set")?;
        let verifying_key = VerifyingKey::from_bytes(&validator.public_key)
            .context("NOV native seal voter public key is invalid")?;
        let signature = Signature::from_slice(self.signature.as_slice())
            .context("NOV native seal vote signature encoding is invalid")?;
        verifying_key
            .verify_strict(&vote_signing_message_v1(self), &signature)
            .context("NOV native seal vote signature verification failed")?;
        if self.vote_hash != vote_hash_v1(self) {
            bail!("NOV native seal vote hash is invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealQuorumCertificateV1 {
    pub schema: String,
    pub subject: NovNativeSealSubjectV1,
    pub subject_hash: [u8; 32],
    pub proposal_hash: [u8; 32],
    pub validator_set_hash: [u8; 32],
    pub votes: Vec<NovNativeSealVoteV1>,
    pub signature_count: u32,
    pub signed_weight: u64,
    pub quorum_weight: u64,
    pub threshold_satisfied: bool,
    pub qc_hash: [u8; 32],
}

impl NovNativeSealQuorumCertificateV1 {
    pub fn from_votes(
        subject: NovNativeSealSubjectV1,
        validator_set: &NovNativeSealValidatorSetV1,
        mut votes: Vec<NovNativeSealVoteV1>,
    ) -> Result<Self> {
        subject.validate(validator_set)?;
        if votes.is_empty() || votes.len() > validator_set.validators.len() {
            bail!("NOV native seal QC vote count is invalid");
        }
        votes.sort_by_key(|vote| vote.validator_id);
        let proposal_hash = votes
            .first()
            .map(|vote| vote.proposal_hash)
            .context("NOV native seal QC has no proposal binding")?;
        let mut signed_weight = 0u64;
        let mut previous = None;
        for vote in &votes {
            vote.verify(&subject, proposal_hash, validator_set)?;
            if previous.is_some_and(|validator_id| validator_id == vote.validator_id) {
                bail!("NOV native seal QC contains a duplicate validator vote");
            }
            previous = Some(vote.validator_id);
            signed_weight = signed_weight
                .checked_add(
                    validator_set
                        .validator(vote.validator_id)
                        .context("NOV native seal QC voter is missing")?
                        .weight,
                )
                .context("NOV native seal QC signed weight overflow")?;
        }
        if signed_weight < validator_set.quorum_weight {
            bail!(
                "NOV native seal QC has insufficient weight: signed={} required={}",
                signed_weight,
                validator_set.quorum_weight
            );
        }
        let signature_count =
            u32::try_from(votes.len()).context("NOV native seal QC signature count overflow")?;
        let mut qc = Self {
            schema: NOV_NATIVE_BLOCK_SEAL_QC_SCHEMA_V1.to_string(),
            subject_hash: subject.subject_hash,
            proposal_hash,
            validator_set_hash: validator_set.validator_set_hash,
            subject,
            votes,
            signature_count,
            signed_weight,
            quorum_weight: validator_set.quorum_weight,
            threshold_satisfied: true,
            qc_hash: [0u8; 32],
        };
        qc.qc_hash = qc_hash_v1(&qc);
        qc.verify(validator_set)?;
        Ok(qc)
    }

    pub fn verify(&self, validator_set: &NovNativeSealValidatorSetV1) -> Result<()> {
        self.subject.validate(validator_set)?;
        if self.schema != NOV_NATIVE_BLOCK_SEAL_QC_SCHEMA_V1
            || self.subject_hash != self.subject.subject_hash
            || self.proposal_hash == [0u8; 32]
            || self.validator_set_hash != validator_set.validator_set_hash
            || self.quorum_weight != validator_set.quorum_weight
            || !self.threshold_satisfied
            || self.signature_count as usize != self.votes.len()
            || self.votes.is_empty()
            || self.votes.len() > validator_set.validators.len()
        {
            bail!("NOV native seal QC metadata is invalid");
        }
        let mut signed_weight = 0u64;
        let mut previous = None;
        for vote in &self.votes {
            vote.verify(&self.subject, self.proposal_hash, validator_set)?;
            if previous.is_some_and(|validator_id| validator_id >= vote.validator_id) {
                bail!("NOV native seal QC votes are not strictly sorted and unique");
            }
            previous = Some(vote.validator_id);
            signed_weight = signed_weight
                .checked_add(
                    validator_set
                        .validator(vote.validator_id)
                        .context("NOV native seal QC voter is missing")?
                        .weight,
                )
                .context("NOV native seal QC signed weight overflow")?;
        }
        if signed_weight != self.signed_weight
            || signed_weight < validator_set.quorum_weight
            || self.qc_hash != qc_hash_v1(self)
        {
            bail!("NOV native seal QC weight or commitment is invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealOutboxEntryV1 {
    pub schema: String,
    pub object_kind: String,
    pub object_hash: [u8; 32],
    pub subject_hash: [u8; 32],
    pub chain_id: u64,
    pub epoch: u64,
    pub height: u64,
    pub round: u64,
    pub validator_id: [u8; 32],
    pub emit_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealLocalProposalRequestV1 {
    pub chain_id: u64,
    pub block_hash: [u8; 32],
    pub round: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub justify_qc_hash: Option<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealCompetingQcEvidenceV1 {
    pub schema: String,
    pub chain_id: u64,
    pub epoch: u64,
    pub height: u64,
    pub round: u64,
    pub left_qc_hash: [u8; 32],
    pub right_qc_hash: [u8; 32],
    pub evidence_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NovNativeSealStoreBindingV1 {
    schema: String,
    chain_id: u64,
    genesis_block_hash: [u8; 32],
    namespace_digest: [u8; 32],
    protocol_config_commitment: [u8; 32],
    ledger_identity_commitment: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NovNativeSealRoundLockV1 {
    schema: String,
    chain_id: u64,
    epoch: u64,
    height: u64,
    round: u64,
    validator_id: [u8; 32],
    subject_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NovNativeSealHeightLockV1 {
    schema: String,
    chain_id: u64,
    epoch: u64,
    height: u64,
    validator_id: [u8; 32],
    block_hash: [u8; 32],
    first_round: u64,
    highest_round: u64,
    revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NovNativeSealProposalLockV1 {
    schema: String,
    chain_id: u64,
    epoch: u64,
    height: u64,
    round: u64,
    proposer_id: [u8; 32],
    subject_hash: [u8; 32],
    proposal_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NovNativeSealVoteLockV1 {
    schema: String,
    chain_id: u64,
    epoch: u64,
    height: u64,
    round: u64,
    validator_id: [u8; 32],
    subject_hash: [u8; 32],
    vote_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NovNativeSealQcIndexV1 {
    schema: String,
    index_kind: String,
    chain_id: u64,
    epoch: u64,
    height: u64,
    binding_hash: [u8; 32],
    qc_hashes: Vec<[u8; 32]>,
}

struct NovNativeBlockSealProcessEntryV1 {
    db: DB,
    write_lock: Arc<Mutex<()>>,
}

impl Deref for NovNativeBlockSealProcessEntryV1 {
    type Target = DB;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}

pub struct NovNativeBlockSealStoreV1 {
    path: PathBuf,
    db: Arc<NovNativeBlockSealProcessEntryV1>,
    write_lock: Arc<Mutex<()>>,
    read_only: bool,
}

impl NovNativeBlockSealStoreV1 {
    pub fn open(path: &Path) -> Result<Self> {
        let process_key = seal_store_process_key_v1(path)?;
        let mut registry = seal_store_process_registry_v1()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "create NOV native seal store parent failed: {}",
                        parent.display()
                    )
                })?;
            }
        }
        registry.retain(|_, entry| entry.strong_count() > 0);
        let db = if let Some(entry) = registry.get(process_key.as_str()).and_then(Weak::upgrade) {
            entry
        } else {
            let mut options = RocksDbOptions::default();
            options.create_if_missing(true);
            let db = DB::open(&options, path).with_context(|| {
                format!("open NOV native seal store failed: {}", path.display())
            })?;
            let entry = Arc::new(NovNativeBlockSealProcessEntryV1 {
                db,
                write_lock: Arc::new(Mutex::new(())),
            });
            registry.insert(process_key, Arc::downgrade(&entry));
            entry
        };
        drop(registry);
        match db
            .get(KEY_SCHEMA_V1)
            .context("read NOV native seal schema failed")?
        {
            Some(raw) if raw.as_slice() != NOV_NATIVE_BLOCK_SEAL_STORE_SCHEMA_V1.as_bytes() => {
                bail!(
                    "unsupported NOV native seal store schema: {}",
                    String::from_utf8_lossy(raw.as_slice())
                );
            }
            Some(_) => {}
            None => {
                let mut batch = RocksDbWriteBatch::default();
                batch.put(
                    KEY_SCHEMA_V1,
                    NOV_NATIVE_BLOCK_SEAL_STORE_SCHEMA_V1.as_bytes(),
                );
                write_sync_v1(&db, batch).context("initialize NOV native seal store schema")?;
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
            write_lock: Arc::clone(&db.write_lock),
            db,
            read_only: false,
        })
    }

    pub fn open_existing_read_only(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let process_key = seal_store_process_key_v1(path)?;
        let mut registry = seal_store_process_registry_v1()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registry.retain(|_, entry| entry.strong_count() > 0);
        let db = if let Some(entry) = registry.get(process_key.as_str()).and_then(Weak::upgrade) {
            entry
        } else {
            let options = RocksDbOptions::default();
            let db = DB::open_for_read_only(&options, path, false).with_context(|| {
                format!(
                    "open existing NOV native seal store read-only failed: {}",
                    path.display()
                )
            })?;
            Arc::new(NovNativeBlockSealProcessEntryV1 {
                db,
                write_lock: Arc::new(Mutex::new(())),
            })
        };
        drop(registry);
        let store = Self {
            path: path.to_path_buf(),
            write_lock: Arc::clone(&db.write_lock),
            db,
            read_only: true,
        };
        store.ensure_schema_v1()?;
        Ok(Some(store))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn register_validator_set(
        &self,
        validator_set: &NovNativeSealValidatorSetV1,
    ) -> Result<bool> {
        validator_set.validate()?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        let key = validator_set_epoch_key_v1(validator_set.chain_id, validator_set.epoch);
        if let Some(existing) =
            read_json_v1::<NovNativeSealValidatorSetV1>(&self.db, key.as_bytes(), "validator set")?
        {
            existing.validate()?;
            if existing != *validator_set {
                bail!("NOV native seal validator epoch is already bound to a different set");
            }
            return Ok(false);
        }
        let mut batch = RocksDbWriteBatch::default();
        stage_validator_set_v1(&self.db, &mut batch, validator_set)?;
        write_sync_v1(&self.db, batch).context("persist NOV native seal validator set")?;
        if self
            .load_validator_set(validator_set.chain_id, validator_set.epoch)?
            .as_ref()
            != Some(validator_set)
        {
            bail!("NOV native seal validator set readback mismatch");
        }
        Ok(true)
    }

    pub fn load_validator_set(
        &self,
        chain_id: u64,
        epoch: u64,
    ) -> Result<Option<NovNativeSealValidatorSetV1>> {
        self.ensure_schema_v1()?;
        let set = read_json_v1::<NovNativeSealValidatorSetV1>(
            &self.db,
            validator_set_epoch_key_v1(chain_id, epoch).as_bytes(),
            "validator set",
        )?;
        if let Some(set) = set.as_ref() {
            set.validate()?;
            if set.chain_id != chain_id || set.epoch != epoch {
                bail!("NOV native seal validator set key binding mismatch");
            }
        }
        Ok(set)
    }

    pub fn prepare_local_subject(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        chain_id: u64,
        block_hash: [u8; 32],
        validator_set: &NovNativeSealValidatorSetV1,
        round: u64,
        justify_qc_hash: Option<[u8; 32]>,
    ) -> Result<NovNativeSealSubjectV1> {
        self.ensure_schema_v1()?;
        validator_set.validate()?;
        let (record, block) = ledger.load_seal_eligible_local_candidate_v1(chain_id, block_hash)?;
        let ownership = ledger
            .load_aoem_ownership()?
            .context("NOV native seal requires a durable AOEM ownership binding")?;
        if ownership.chain_id != chain_id {
            bail!("NOV native seal AOEM ownership chain binding mismatch");
        }
        let protocol_config_commitment = decode_hex_commitment_v1(
            "AOEM protocol config commitment",
            ownership.protocol_config_commitment.as_str(),
        )?;
        let genesis_block_hash = ledger
            .load_by_height(chain_id, 1)?
            .context("NOV native seal requires a durable genesis block")?
            .header
            .block_hash;
        let justify_qc_hash = justify_qc_hash.unwrap_or([0u8; 32]);
        self.validate_justify_qc_v1(&record, validator_set, justify_qc_hash)?;
        subject_from_candidate_v1(
            &record,
            &block,
            validator_set,
            round,
            justify_qc_hash,
            genesis_block_hash,
            protocol_config_commitment,
        )
    }

    /// Persist-before-emit proposal signing. The returned proposal is safe to
    /// hand to the network only because its immutable object, local safety
    /// locks, and outbox record were synchronously committed and read back.
    pub fn sign_local_proposal(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        request: &NovNativeSealLocalProposalRequestV1,
        validator_set: &NovNativeSealValidatorSetV1,
        signing_key: &SigningKey,
    ) -> Result<NovNativeSealProposalV1> {
        let subject = self.prepare_local_subject(
            ledger,
            request.chain_id,
            request.block_hash,
            validator_set,
            request.round,
            request.justify_qc_hash,
        )?;
        let proposer_id = validator_id_v1(signing_key.verifying_key().as_bytes());
        if validator_set.validator(proposer_id).is_none() {
            bail!("NOV native seal proposal signer is not in the validator set");
        }
        let expected_binding = store_binding_v1(ledger, request.chain_id)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&expected_binding)?;

        let proposal_lock_key = proposal_lock_key_v1(&subject, proposer_id);
        if let Some(lock) = read_json_v1::<NovNativeSealProposalLockV1>(
            &self.db,
            proposal_lock_key.as_bytes(),
            "proposal lock",
        )? {
            self.ensure_registered_validator_set_v1(validator_set)?;
            validate_proposal_lock_v1(&lock, &subject, proposer_id)?;
            self.validate_existing_safety_locks_v1(&subject, proposer_id)?;
            let proposal = self
                .load_proposal(lock.proposal_hash)?
                .context("NOV native seal proposal lock points to a missing object")?;
            proposal.verify(validator_set)?;
            if proposal.subject != subject {
                bail!("NOV native seal proposal lock conflicts with the requested subject");
            }
            self.validate_outbox_for_object_v1(
                "proposal",
                proposal.proposal_hash,
                &subject,
                proposer_id,
            )?;
            return Ok(proposal);
        }

        let (round_lock, height_lock) = self.prepare_safety_locks_v1(&subject, proposer_id)?;
        let proposal = sign_proposal_v1(subject.clone(), validator_set, signing_key)?;
        let proposal_lock = NovNativeSealProposalLockV1 {
            schema: PROPOSAL_LOCK_SCHEMA_V1.to_string(),
            chain_id: subject.chain_id,
            epoch: subject.epoch,
            height: subject.height,
            round: subject.round,
            proposer_id,
            subject_hash: subject.subject_hash,
            proposal_hash: proposal.proposal_hash,
        };
        let outbox = outbox_entry_v1("proposal", proposal.proposal_hash, &subject, proposer_id);
        let mut batch = RocksDbWriteBatch::default();
        self.stage_binding_and_validator_set_v1(&mut batch, &expected_binding, validator_set)?;
        stage_object_if_available_v1(
            &self.db,
            &mut batch,
            proposal_object_key_v1(&proposal.proposal_hash).as_bytes(),
            &proposal,
            "proposal object",
        )?;
        put_json_v1(
            &mut batch,
            proposal_lock_key.as_bytes(),
            &proposal_lock,
            "proposal lock",
        )?;
        put_json_v1(
            &mut batch,
            round_lock_key_v1(&subject, proposer_id).as_bytes(),
            &round_lock,
            "round safety lock",
        )?;
        put_json_v1(
            &mut batch,
            height_lock_key_v1(&subject, proposer_id).as_bytes(),
            &height_lock,
            "height safety lock",
        )?;
        put_json_v1(
            &mut batch,
            outbox_key_v1("proposal", &proposal.proposal_hash).as_bytes(),
            &outbox,
            "proposal outbox",
        )?;
        write_sync_v1(&self.db, batch).context("persist NOV native seal proposal safety batch")?;
        self.verify_proposal_persistence_v1(
            &proposal,
            &proposal_lock,
            &round_lock,
            &height_lock,
            &outbox,
            validator_set,
        )?;
        Ok(proposal)
    }

    /// Verify a proposal against a local AOEM-owned candidate and persist a
    /// vote before returning it. A conservative height lock prevents this v1
    /// signer from switching candidates in a later round without a future,
    /// explicitly versioned unlock/justify rule.
    pub fn sign_local_vote(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        proposal: &NovNativeSealProposalV1,
        validator_set: &NovNativeSealValidatorSetV1,
        signing_key: &SigningKey,
    ) -> Result<NovNativeSealVoteV1> {
        proposal.verify(validator_set)?;
        let justify = (proposal.subject.justify_qc_hash != [0u8; 32])
            .then_some(proposal.subject.justify_qc_hash);
        let expected_subject = self.prepare_local_subject(
            ledger,
            proposal.subject.chain_id,
            proposal.subject.block_hash,
            validator_set,
            proposal.subject.round,
            justify,
        )?;
        if expected_subject != proposal.subject {
            bail!("NOV native seal proposal does not match the locally verified candidate");
        }
        let validator_id = validator_id_v1(signing_key.verifying_key().as_bytes());
        if validator_set.validator(validator_id).is_none() {
            bail!("NOV native seal vote signer is not in the validator set");
        }
        let expected_binding = store_binding_v1(ledger, proposal.subject.chain_id)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&expected_binding)?;

        let vote_lock_key = vote_lock_key_v1(&proposal.subject, validator_id);
        if let Some(lock) = read_json_v1::<NovNativeSealVoteLockV1>(
            &self.db,
            vote_lock_key.as_bytes(),
            "vote lock",
        )? {
            self.ensure_registered_validator_set_v1(validator_set)?;
            validate_vote_lock_v1(&lock, &proposal.subject, validator_id)?;
            self.validate_existing_safety_locks_v1(&proposal.subject, validator_id)?;
            let vote = self
                .load_vote(lock.vote_hash)?
                .context("NOV native seal vote lock points to a missing object")?;
            vote.verify(&proposal.subject, proposal.proposal_hash, validator_set)?;
            if vote.proposal_hash != proposal.proposal_hash {
                bail!("NOV native seal vote lock is bound to a different proposal");
            }
            self.validate_outbox_for_object_v1(
                "vote",
                vote.vote_hash,
                &proposal.subject,
                validator_id,
            )?;
            return Ok(vote);
        }

        let (round_lock, height_lock) =
            self.prepare_safety_locks_v1(&proposal.subject, validator_id)?;
        let vote = sign_vote_v1(proposal, validator_set, signing_key)?;
        let vote_lock = NovNativeSealVoteLockV1 {
            schema: VOTE_LOCK_SCHEMA_V1.to_string(),
            chain_id: proposal.subject.chain_id,
            epoch: proposal.subject.epoch,
            height: proposal.subject.height,
            round: proposal.subject.round,
            validator_id,
            subject_hash: proposal.subject.subject_hash,
            vote_hash: vote.vote_hash,
        };
        let outbox = outbox_entry_v1("vote", vote.vote_hash, &proposal.subject, validator_id);
        let mut batch = RocksDbWriteBatch::default();
        self.stage_binding_and_validator_set_v1(&mut batch, &expected_binding, validator_set)?;
        stage_object_if_available_v1(
            &self.db,
            &mut batch,
            proposal_object_key_v1(&proposal.proposal_hash).as_bytes(),
            proposal,
            "proposal object",
        )?;
        stage_object_if_available_v1(
            &self.db,
            &mut batch,
            vote_object_key_v1(&vote.vote_hash).as_bytes(),
            &vote,
            "vote object",
        )?;
        put_json_v1(
            &mut batch,
            vote_lock_key.as_bytes(),
            &vote_lock,
            "vote lock",
        )?;
        put_json_v1(
            &mut batch,
            round_lock_key_v1(&proposal.subject, validator_id).as_bytes(),
            &round_lock,
            "round safety lock",
        )?;
        put_json_v1(
            &mut batch,
            height_lock_key_v1(&proposal.subject, validator_id).as_bytes(),
            &height_lock,
            "height safety lock",
        )?;
        put_json_v1(
            &mut batch,
            outbox_key_v1("vote", &vote.vote_hash).as_bytes(),
            &outbox,
            "vote outbox",
        )?;
        write_sync_v1(&self.db, batch).context("persist NOV native seal vote safety batch")?;
        self.verify_vote_persistence_v1(
            &proposal.subject,
            &vote,
            &vote_lock,
            &round_lock,
            &height_lock,
            &outbox,
            validator_set,
        )?;
        Ok(vote)
    }

    pub fn persist_local_verified_qc(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        qc: &NovNativeSealQuorumCertificateV1,
        validator_set: &NovNativeSealValidatorSetV1,
    ) -> Result<bool> {
        qc.verify(validator_set)?;
        let justify =
            (qc.subject.justify_qc_hash != [0u8; 32]).then_some(qc.subject.justify_qc_hash);
        let expected_subject = self.prepare_local_subject(
            ledger,
            qc.subject.chain_id,
            qc.subject.block_hash,
            validator_set,
            qc.subject.round,
            justify,
        )?;
        if expected_subject != qc.subject {
            bail!("NOV native seal QC does not match the locally verified candidate");
        }
        let proposal = self
            .load_proposal(qc.proposal_hash)?
            .context("NOV native seal QC proposal object is not durably stored")?;
        proposal.verify(validator_set)?;
        if proposal.subject != qc.subject || proposal.proposal_hash != qc.proposal_hash {
            bail!("NOV native seal QC proposal/subject binding mismatch");
        }
        let expected_binding = store_binding_v1(ledger, qc.subject.chain_id)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&expected_binding)?;
        if let Some(existing) = self.load_qc(qc.qc_hash)? {
            if existing != *qc {
                bail!("NOV native seal QC hash collision or conflicting object");
            }
            self.ensure_qc_indexes_contain_v1(qc)?;
            return Ok(false);
        }

        let mut batch = RocksDbWriteBatch::default();
        self.stage_binding_and_validator_set_v1(&mut batch, &expected_binding, validator_set)?;
        stage_object_if_available_v1(
            &self.db,
            &mut batch,
            qc_object_key_v1(&qc.qc_hash).as_bytes(),
            qc,
            "QC object",
        )?;
        self.stage_qc_index_v1(
            &mut batch,
            qc_subject_index_key_v1(&qc.subject_hash),
            "subject",
            qc.subject.chain_id,
            qc.subject.epoch,
            qc.subject.height,
            qc.subject_hash,
            qc.qc_hash,
        )?;
        self.stage_qc_index_v1(
            &mut batch,
            qc_block_index_key_v1(qc.subject.chain_id, &qc.subject.block_hash),
            "block",
            qc.subject.chain_id,
            qc.subject.epoch,
            qc.subject.height,
            qc.subject.block_hash,
            qc.qc_hash,
        )?;
        let height_index_key =
            qc_height_index_key_v1(qc.subject.chain_id, qc.subject.epoch, qc.subject.height);
        let existing_height_index = self.load_qc_index_v1(
            height_index_key.as_str(),
            "height",
            qc.subject.chain_id,
            qc.subject.epoch,
            qc.subject.height,
            [0u8; 32],
        )?;
        if let Some(index) = existing_height_index.as_ref() {
            for existing_hash in &index.qc_hashes {
                let existing_qc = self
                    .load_qc(*existing_hash)?
                    .context("NOV native seal height index points to a missing QC")?;
                if existing_qc.subject.round == qc.subject.round
                    && existing_qc.subject.block_hash != qc.subject.block_hash
                {
                    let evidence = competing_qc_evidence_v1(&existing_qc, qc);
                    stage_object_if_available_v1(
                        &self.db,
                        &mut batch,
                        competing_qc_evidence_key_v1(
                            evidence.chain_id,
                            evidence.epoch,
                            evidence.height,
                            evidence.round,
                            &evidence.evidence_hash,
                        )
                        .as_bytes(),
                        &evidence,
                        "competing QC evidence",
                    )?;
                }
            }
        }
        self.stage_qc_index_v1(
            &mut batch,
            height_index_key,
            "height",
            qc.subject.chain_id,
            qc.subject.epoch,
            qc.subject.height,
            [0u8; 32],
            qc.qc_hash,
        )?;
        write_sync_v1(&self.db, batch).context("persist NOV native seal QC and indexes")?;
        let readback = self
            .load_qc(qc.qc_hash)?
            .context("NOV native seal QC readback is missing")?;
        if readback != *qc {
            bail!("NOV native seal QC readback mismatch");
        }
        self.ensure_qc_indexes_contain_v1(qc)?;
        Ok(true)
    }

    pub fn load_proposal(
        &self,
        proposal_hash: [u8; 32],
    ) -> Result<Option<NovNativeSealProposalV1>> {
        self.ensure_schema_v1()?;
        let proposal = read_json_v1::<NovNativeSealProposalV1>(
            &self.db,
            proposal_object_key_v1(&proposal_hash).as_bytes(),
            "proposal object",
        )?;
        if proposal
            .as_ref()
            .is_some_and(|proposal| proposal.proposal_hash != proposal_hash)
        {
            bail!("NOV native seal proposal object key binding mismatch");
        }
        if let Some(proposal) = proposal.as_ref() {
            let validator_set = self
                .load_validator_set(proposal.subject.chain_id, proposal.subject.epoch)?
                .context("NOV native seal proposal is missing its durable validator set")?;
            proposal.verify(&validator_set)?;
        }
        Ok(proposal)
    }

    pub fn load_vote(&self, vote_hash: [u8; 32]) -> Result<Option<NovNativeSealVoteV1>> {
        self.ensure_schema_v1()?;
        let vote = read_json_v1::<NovNativeSealVoteV1>(
            &self.db,
            vote_object_key_v1(&vote_hash).as_bytes(),
            "vote object",
        )?;
        if vote
            .as_ref()
            .is_some_and(|vote| vote.vote_hash != vote_hash)
        {
            bail!("NOV native seal vote object key binding mismatch");
        }
        if let Some(vote) = vote.as_ref() {
            let proposal = self
                .load_proposal(vote.proposal_hash)?
                .context("NOV native seal vote is missing its durable proposal")?;
            let validator_set = self
                .load_validator_set(vote.chain_id, vote.epoch)?
                .context("NOV native seal vote is missing its durable validator set")?;
            vote.verify(&proposal.subject, proposal.proposal_hash, &validator_set)?;
        }
        Ok(vote)
    }

    pub fn load_qc(&self, qc_hash: [u8; 32]) -> Result<Option<NovNativeSealQuorumCertificateV1>> {
        self.ensure_schema_v1()?;
        let qc = read_json_v1::<NovNativeSealQuorumCertificateV1>(
            &self.db,
            qc_object_key_v1(&qc_hash).as_bytes(),
            "QC object",
        )?;
        if qc.as_ref().is_some_and(|qc| qc.qc_hash != qc_hash) {
            bail!("NOV native seal QC object key binding mismatch");
        }
        if let Some(qc) = qc.as_ref() {
            let validator_set = self
                .load_validator_set(qc.subject.chain_id, qc.subject.epoch)?
                .context("NOV native seal QC is missing its durable validator set")?;
            qc.verify(&validator_set)?;
            let proposal = self
                .load_proposal(qc.proposal_hash)?
                .context("NOV native seal QC is missing its durable proposal")?;
            if proposal.subject != qc.subject || proposal.proposal_hash != qc.proposal_hash {
                bail!("NOV native seal QC proposal/subject reverse binding mismatch");
            }
        }
        Ok(qc)
    }

    pub fn load_qcs_by_subject_hash(
        &self,
        subject_hash: [u8; 32],
    ) -> Result<Vec<NovNativeSealQuorumCertificateV1>> {
        self.load_qcs_from_index_v1(
            qc_subject_index_key_v1(&subject_hash).as_str(),
            "subject",
            None,
            None,
            None,
            subject_hash,
        )
    }

    pub fn load_qcs_by_block_hash(
        &self,
        chain_id: u64,
        block_hash: [u8; 32],
    ) -> Result<Vec<NovNativeSealQuorumCertificateV1>> {
        self.load_qcs_from_index_v1(
            qc_block_index_key_v1(chain_id, &block_hash).as_str(),
            "block",
            Some(chain_id),
            None,
            None,
            block_hash,
        )
    }

    pub fn load_qcs_by_height(
        &self,
        chain_id: u64,
        epoch: u64,
        height: u64,
    ) -> Result<Vec<NovNativeSealQuorumCertificateV1>> {
        self.load_qcs_from_index_v1(
            qc_height_index_key_v1(chain_id, epoch, height).as_str(),
            "height",
            Some(chain_id),
            Some(epoch),
            Some(height),
            [0u8; 32],
        )
    }

    pub fn load_pending_outbox(
        &self,
        chain_id: u64,
        validator_id: [u8; 32],
        limit: usize,
    ) -> Result<Vec<NovNativeSealOutboxEntryV1>> {
        self.ensure_schema_v1()?;
        if limit == 0 || limit > NOV_NATIVE_BLOCK_SEAL_MAX_OUTBOX_SCAN_V1 {
            bail!("NOV native seal outbox scan limit is invalid");
        }
        let prefix = format!("{KEY_PREFIX_V1}outbox/");
        let mut entries = Vec::new();
        let mut scanned = 0usize;
        for item in self
            .db
            .iterator(IteratorMode::From(prefix.as_bytes(), Direction::Forward))
        {
            let (key, value) = item.context("iterate NOV native seal outbox")?;
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }
            scanned = scanned
                .checked_add(1)
                .context("NOV native seal outbox scan count overflow")?;
            if scanned > NOV_NATIVE_BLOCK_SEAL_MAX_OUTBOX_SCAN_V1 {
                bail!("NOV native seal outbox recovery scan exceeds its fail-closed bound");
            }
            let entry: NovNativeSealOutboxEntryV1 = serde_json::from_slice(value.as_ref())
                .context("decode NOV native seal outbox entry")?;
            validate_outbox_entry_v1(&entry)?;
            self.validate_outbox_recovery_entry_v1(&entry)?;
            if entry.chain_id == chain_id
                && entry.validator_id == validator_id
                && entry.emit_state == "ready_to_emit"
            {
                entries.push(entry);
                if entries.len() == limit {
                    break;
                }
            }
        }
        entries.sort_by_key(|entry| {
            (
                entry.height,
                entry.round,
                entry.object_kind.clone(),
                entry.object_hash,
            )
        });
        Ok(entries)
    }

    pub fn load_competing_qc_evidence(
        &self,
        chain_id: u64,
        epoch: u64,
        height: u64,
        round: u64,
        limit: usize,
    ) -> Result<Vec<NovNativeSealCompetingQcEvidenceV1>> {
        self.ensure_schema_v1()?;
        if limit == 0 || limit > NOV_NATIVE_BLOCK_SEAL_MAX_QCS_PER_INDEX_V1 {
            bail!("NOV native seal competing-QC evidence scan limit is invalid");
        }
        let prefix = competing_qc_evidence_prefix_v1(chain_id, epoch, height, round);
        let mut evidence = Vec::new();
        for item in self
            .db
            .iterator(IteratorMode::From(prefix.as_bytes(), Direction::Forward))
        {
            let (key, value) = item.context("iterate NOV native competing-QC evidence")?;
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }
            let item: NovNativeSealCompetingQcEvidenceV1 =
                serde_json::from_slice(value.as_ref())
                    .context("decode NOV native competing-QC evidence")?;
            validate_competing_qc_evidence_v1(&item)?;
            evidence.push(item);
            if evidence.len() == limit {
                break;
            }
        }
        Ok(evidence)
    }

    fn validate_justify_qc_v1(
        &self,
        candidate: &NovNativeBlockCandidateRecordV1,
        validator_set: &NovNativeSealValidatorSetV1,
        justify_qc_hash: [u8; 32],
    ) -> Result<()> {
        if candidate.height == 1 {
            if candidate.parent_block_hash != [0u8; 32] || justify_qc_hash != [0u8; 32] {
                bail!("NOV native genesis seal candidate cannot carry a justify QC");
            }
            return Ok(());
        }
        if justify_qc_hash == [0u8; 32] {
            bail!("NOV native non-genesis seal candidate requires a durable justify QC");
        }
        let qc = self
            .load_qc(justify_qc_hash)?
            .context("NOV native seal justify QC is not durably stored")?;
        qc.verify(validator_set)?;
        let expected_height = qc
            .subject
            .height
            .checked_add(1)
            .context("NOV native seal justify QC height overflow")?;
        if qc.subject.chain_id != candidate.chain_id
            || qc.subject.epoch != validator_set.epoch
            || expected_height != candidate.height
            || qc.subject.block_hash != candidate.parent_block_hash
        {
            bail!("NOV native seal justify QC does not bind the candidate parent");
        }
        Ok(())
    }

    fn ensure_qc_indexes_contain_v1(&self, qc: &NovNativeSealQuorumCertificateV1) -> Result<()> {
        for qcs in [
            self.load_qcs_by_subject_hash(qc.subject_hash)?,
            self.load_qcs_by_block_hash(qc.subject.chain_id, qc.subject.block_hash)?,
            self.load_qcs_by_height(qc.subject.chain_id, qc.subject.epoch, qc.subject.height)?,
        ] {
            if !qcs.iter().any(|stored| stored.qc_hash == qc.qc_hash) {
                bail!("NOV native seal QC index readback is missing the persisted QC");
            }
        }
        Ok(())
    }

    fn prepare_safety_locks_v1(
        &self,
        subject: &NovNativeSealSubjectV1,
        validator_id: [u8; 32],
    ) -> Result<(NovNativeSealRoundLockV1, NovNativeSealHeightLockV1)> {
        let round_key = round_lock_key_v1(subject, validator_id);
        let round_lock = match read_json_v1::<NovNativeSealRoundLockV1>(
            &self.db,
            round_key.as_bytes(),
            "round safety lock",
        )? {
            Some(lock) => {
                validate_round_lock_v1(&lock, subject, validator_id)?;
                lock
            }
            None => NovNativeSealRoundLockV1 {
                schema: ROUND_LOCK_SCHEMA_V1.to_string(),
                chain_id: subject.chain_id,
                epoch: subject.epoch,
                height: subject.height,
                round: subject.round,
                validator_id,
                subject_hash: subject.subject_hash,
            },
        };
        let height_key = height_lock_key_v1(subject, validator_id);
        let height_lock = match read_json_v1::<NovNativeSealHeightLockV1>(
            &self.db,
            height_key.as_bytes(),
            "height safety lock",
        )? {
            Some(mut lock) => {
                validate_height_lock_identity_v1(&lock, subject, validator_id)?;
                if lock.block_hash != subject.block_hash {
                    bail!("NOV native seal validator is already locked to a competing candidate at this height");
                }
                if subject.round < lock.highest_round {
                    bail!("NOV native seal validator cannot create a new signature for an older round");
                }
                if subject.round > lock.highest_round {
                    lock.highest_round = subject.round;
                    lock.revision = lock
                        .revision
                        .checked_add(1)
                        .context("NOV native seal height lock revision overflow")?;
                }
                lock
            }
            None => NovNativeSealHeightLockV1 {
                schema: HEIGHT_LOCK_SCHEMA_V1.to_string(),
                chain_id: subject.chain_id,
                epoch: subject.epoch,
                height: subject.height,
                validator_id,
                block_hash: subject.block_hash,
                first_round: subject.round,
                highest_round: subject.round,
                revision: 1,
            },
        };
        Ok((round_lock, height_lock))
    }

    fn validate_existing_safety_locks_v1(
        &self,
        subject: &NovNativeSealSubjectV1,
        validator_id: [u8; 32],
    ) -> Result<()> {
        let round_lock = read_json_v1::<NovNativeSealRoundLockV1>(
            &self.db,
            round_lock_key_v1(subject, validator_id).as_bytes(),
            "round safety lock",
        )?
        .context("NOV native signed object is missing its durable round safety lock")?;
        validate_round_lock_v1(&round_lock, subject, validator_id)?;
        let height_lock = read_json_v1::<NovNativeSealHeightLockV1>(
            &self.db,
            height_lock_key_v1(subject, validator_id).as_bytes(),
            "height safety lock",
        )?
        .context("NOV native signed object is missing its durable height safety lock")?;
        validate_height_lock_identity_v1(&height_lock, subject, validator_id)?;
        if height_lock.block_hash != subject.block_hash
            || height_lock.first_round > subject.round
            || height_lock.highest_round < subject.round
        {
            bail!("NOV native signed object conflicts with its durable height safety lock");
        }
        Ok(())
    }

    fn ensure_registered_validator_set_v1(
        &self,
        validator_set: &NovNativeSealValidatorSetV1,
    ) -> Result<()> {
        let stored = self
            .load_validator_set(validator_set.chain_id, validator_set.epoch)?
            .context("NOV native signed object is missing its durable validator set")?;
        if stored != *validator_set {
            bail!("NOV native signed object validator-set binding mismatch");
        }
        Ok(())
    }

    fn stage_binding_and_validator_set_v1(
        &self,
        batch: &mut RocksDbWriteBatch,
        binding: &NovNativeSealStoreBindingV1,
        validator_set: &NovNativeSealValidatorSetV1,
    ) -> Result<()> {
        if read_json_v1::<NovNativeSealStoreBindingV1>(
            &self.db,
            store_binding_key_v1(binding.chain_id).as_bytes(),
            "store binding",
        )?
        .is_none()
        {
            put_json_v1(
                batch,
                store_binding_key_v1(binding.chain_id).as_bytes(),
                binding,
                "store binding",
            )?;
        }
        stage_validator_set_v1(&self.db, batch, validator_set)
    }

    fn ensure_store_binding_v1(&self, expected: &NovNativeSealStoreBindingV1) -> Result<()> {
        if let Some(existing) = read_json_v1::<NovNativeSealStoreBindingV1>(
            &self.db,
            store_binding_key_v1(expected.chain_id).as_bytes(),
            "store binding",
        )? {
            validate_store_binding_v1(&existing)?;
            if existing != *expected {
                bail!("NOV native seal store is bound to a different candidate ledger");
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn stage_qc_index_v1(
        &self,
        batch: &mut RocksDbWriteBatch,
        key: String,
        index_kind: &str,
        chain_id: u64,
        epoch: u64,
        height: u64,
        binding_hash: [u8; 32],
        qc_hash: [u8; 32],
    ) -> Result<()> {
        let mut index = self
            .load_qc_index_v1(
                key.as_str(),
                index_kind,
                chain_id,
                epoch,
                height,
                binding_hash,
            )?
            .unwrap_or_else(|| NovNativeSealQcIndexV1 {
                schema: QC_INDEX_SCHEMA_V1.to_string(),
                index_kind: index_kind.to_string(),
                chain_id,
                epoch,
                height,
                binding_hash,
                qc_hashes: Vec::new(),
            });
        index.qc_hashes.push(qc_hash);
        index.qc_hashes.sort_unstable();
        index.qc_hashes.dedup();
        validate_qc_index_v1(&index, index_kind, chain_id, epoch, height, binding_hash)?;
        put_json_v1(batch, key.as_bytes(), &index, "QC index")
    }

    fn load_qc_index_v1(
        &self,
        key: &str,
        index_kind: &str,
        chain_id: u64,
        epoch: u64,
        height: u64,
        binding_hash: [u8; 32],
    ) -> Result<Option<NovNativeSealQcIndexV1>> {
        let index = read_json_v1::<NovNativeSealQcIndexV1>(&self.db, key.as_bytes(), "QC index")?;
        if let Some(index) = index.as_ref() {
            validate_qc_index_v1(index, index_kind, chain_id, epoch, height, binding_hash)?;
        }
        Ok(index)
    }

    fn load_qcs_from_index_v1(
        &self,
        key: &str,
        index_kind: &str,
        chain_id: Option<u64>,
        epoch: Option<u64>,
        height: Option<u64>,
        binding_hash: [u8; 32],
    ) -> Result<Vec<NovNativeSealQuorumCertificateV1>> {
        self.ensure_schema_v1()?;
        let raw = read_json_v1::<NovNativeSealQcIndexV1>(&self.db, key.as_bytes(), "QC index")?;
        let Some(index) = raw else {
            return Ok(Vec::new());
        };
        if index.schema != QC_INDEX_SCHEMA_V1
            || index.index_kind != index_kind
            || chain_id.is_some_and(|value| index.chain_id != value)
            || epoch.is_some_and(|value| index.epoch != value)
            || height.is_some_and(|value| index.height != value)
            || index.binding_hash != binding_hash
        {
            bail!("NOV native seal QC index key binding mismatch");
        }
        validate_qc_hashes_v1(index.qc_hashes.as_slice())?;
        let mut qcs = Vec::with_capacity(index.qc_hashes.len());
        for hash in index.qc_hashes {
            let qc = self
                .load_qc(hash)?
                .context("NOV native seal QC index points to a missing object")?;
            let binding_matches = match index_kind {
                "subject" => qc.subject_hash == binding_hash,
                "block" => qc.subject.block_hash == binding_hash,
                "height" => true,
                _ => false,
            };
            if !binding_matches
                || qc.subject.chain_id != index.chain_id
                || qc.subject.epoch != index.epoch
                || qc.subject.height != index.height
            {
                bail!("NOV native seal QC index/object reverse binding mismatch");
            }
            qcs.push(qc);
        }
        Ok(qcs)
    }

    fn validate_outbox_for_object_v1(
        &self,
        object_kind: &str,
        object_hash: [u8; 32],
        subject: &NovNativeSealSubjectV1,
        validator_id: [u8; 32],
    ) -> Result<()> {
        let outbox = read_json_v1::<NovNativeSealOutboxEntryV1>(
            &self.db,
            outbox_key_v1(object_kind, &object_hash).as_bytes(),
            "outbox entry",
        )?
        .context("NOV native seal signed object is missing its durable outbox entry")?;
        validate_outbox_entry_v1(&outbox)?;
        if outbox.object_kind != object_kind
            || outbox.object_hash != object_hash
            || outbox.subject_hash != subject.subject_hash
            || outbox.chain_id != subject.chain_id
            || outbox.epoch != subject.epoch
            || outbox.height != subject.height
            || outbox.round != subject.round
            || outbox.validator_id != validator_id
        {
            bail!("NOV native seal outbox object binding mismatch");
        }
        Ok(())
    }

    fn validate_outbox_recovery_entry_v1(&self, entry: &NovNativeSealOutboxEntryV1) -> Result<()> {
        match entry.object_kind.as_str() {
            "proposal" => {
                let proposal = self
                    .load_proposal(entry.object_hash)?
                    .context("NOV native seal outbox points to a missing proposal")?;
                if proposal.subject_hash != entry.subject_hash
                    || proposal.subject.chain_id != entry.chain_id
                    || proposal.subject.epoch != entry.epoch
                    || proposal.subject.height != entry.height
                    || proposal.subject.round != entry.round
                    || proposal.proposer_id != entry.validator_id
                {
                    bail!("NOV native seal proposal outbox reverse binding mismatch");
                }
                let lock = read_json_v1::<NovNativeSealProposalLockV1>(
                    &self.db,
                    proposal_lock_key_v1(&proposal.subject, proposal.proposer_id).as_bytes(),
                    "proposal lock",
                )?
                .context("NOV native seal proposal outbox is missing its role lock")?;
                validate_proposal_lock_v1(&lock, &proposal.subject, proposal.proposer_id)?;
                if lock.proposal_hash != proposal.proposal_hash {
                    bail!("NOV native seal proposal outbox role-lock mismatch");
                }
                self.validate_existing_safety_locks_v1(&proposal.subject, proposal.proposer_id)?;
            }
            "vote" => {
                let vote = self
                    .load_vote(entry.object_hash)?
                    .context("NOV native seal outbox points to a missing vote")?;
                let proposal = self
                    .load_proposal(vote.proposal_hash)?
                    .context("NOV native seal vote outbox is missing its proposal")?;
                if vote.subject_hash != entry.subject_hash
                    || vote.chain_id != entry.chain_id
                    || vote.epoch != entry.epoch
                    || vote.height != entry.height
                    || vote.round != entry.round
                    || vote.validator_id != entry.validator_id
                    || proposal.subject.subject_hash != vote.subject_hash
                {
                    bail!("NOV native seal vote outbox reverse binding mismatch");
                }
                let lock = read_json_v1::<NovNativeSealVoteLockV1>(
                    &self.db,
                    vote_lock_key_v1(&proposal.subject, vote.validator_id).as_bytes(),
                    "vote lock",
                )?
                .context("NOV native seal vote outbox is missing its role lock")?;
                validate_vote_lock_v1(&lock, &proposal.subject, vote.validator_id)?;
                if lock.vote_hash != vote.vote_hash {
                    bail!("NOV native seal vote outbox role-lock mismatch");
                }
                self.validate_existing_safety_locks_v1(&proposal.subject, vote.validator_id)?;
            }
            _ => bail!("NOV native seal outbox object kind is unsupported"),
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_proposal_persistence_v1(
        &self,
        proposal: &NovNativeSealProposalV1,
        proposal_lock: &NovNativeSealProposalLockV1,
        round_lock: &NovNativeSealRoundLockV1,
        height_lock: &NovNativeSealHeightLockV1,
        outbox: &NovNativeSealOutboxEntryV1,
        validator_set: &NovNativeSealValidatorSetV1,
    ) -> Result<()> {
        let stored = self
            .load_proposal(proposal.proposal_hash)?
            .context("NOV native seal proposal readback is missing")?;
        stored.verify(validator_set)?;
        if stored != *proposal
            || read_json_v1::<NovNativeSealProposalLockV1>(
                &self.db,
                proposal_lock_key_v1(&proposal.subject, proposal.proposer_id).as_bytes(),
                "proposal lock",
            )?
            .as_ref()
                != Some(proposal_lock)
            || read_json_v1::<NovNativeSealRoundLockV1>(
                &self.db,
                round_lock_key_v1(&proposal.subject, proposal.proposer_id).as_bytes(),
                "round safety lock",
            )?
            .as_ref()
                != Some(round_lock)
            || read_json_v1::<NovNativeSealHeightLockV1>(
                &self.db,
                height_lock_key_v1(&proposal.subject, proposal.proposer_id).as_bytes(),
                "height safety lock",
            )?
            .as_ref()
                != Some(height_lock)
            || read_json_v1::<NovNativeSealOutboxEntryV1>(
                &self.db,
                outbox_key_v1("proposal", &proposal.proposal_hash).as_bytes(),
                "proposal outbox",
            )?
            .as_ref()
                != Some(outbox)
        {
            bail!("NOV native seal proposal safety batch readback mismatch");
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_vote_persistence_v1(
        &self,
        subject: &NovNativeSealSubjectV1,
        vote: &NovNativeSealVoteV1,
        vote_lock: &NovNativeSealVoteLockV1,
        round_lock: &NovNativeSealRoundLockV1,
        height_lock: &NovNativeSealHeightLockV1,
        outbox: &NovNativeSealOutboxEntryV1,
        validator_set: &NovNativeSealValidatorSetV1,
    ) -> Result<()> {
        let stored = self
            .load_vote(vote.vote_hash)?
            .context("NOV native seal vote readback is missing")?;
        stored.verify(subject, stored.proposal_hash, validator_set)?;
        if stored != *vote
            || read_json_v1::<NovNativeSealVoteLockV1>(
                &self.db,
                vote_lock_key_v1(subject, vote.validator_id).as_bytes(),
                "vote lock",
            )?
            .as_ref()
                != Some(vote_lock)
            || read_json_v1::<NovNativeSealRoundLockV1>(
                &self.db,
                round_lock_key_v1(subject, vote.validator_id).as_bytes(),
                "round safety lock",
            )?
            .as_ref()
                != Some(round_lock)
            || read_json_v1::<NovNativeSealHeightLockV1>(
                &self.db,
                height_lock_key_v1(subject, vote.validator_id).as_bytes(),
                "height safety lock",
            )?
            .as_ref()
                != Some(height_lock)
            || read_json_v1::<NovNativeSealOutboxEntryV1>(
                &self.db,
                outbox_key_v1("vote", &vote.vote_hash).as_bytes(),
                "vote outbox",
            )?
            .as_ref()
                != Some(outbox)
        {
            bail!("NOV native seal vote safety batch readback mismatch");
        }
        Ok(())
    }

    fn ensure_schema_v1(&self) -> Result<()> {
        match self
            .db
            .get(KEY_SCHEMA_V1)
            .context("read NOV native seal schema failed")?
        {
            Some(raw) if raw.as_slice() == NOV_NATIVE_BLOCK_SEAL_STORE_SCHEMA_V1.as_bytes() => {
                Ok(())
            }
            Some(raw) => bail!(
                "unsupported NOV native seal store schema: {}",
                String::from_utf8_lossy(raw.as_slice())
            ),
            None => bail!("NOV native seal store schema is missing"),
        }
    }

    fn lock_writes_v1(&self) -> Result<MutexGuard<'_, ()>> {
        if self.read_only {
            bail!("NOV native seal store is read-only");
        }
        Ok(self
            .write_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))
    }
}

fn subject_from_candidate_v1(
    record: &NovNativeBlockCandidateRecordV1,
    block: &NovNativeDurableBlockV1,
    validator_set: &NovNativeSealValidatorSetV1,
    round: u64,
    justify_qc_hash: [u8; 32],
    genesis_block_hash: [u8; 32],
    protocol_config_commitment: [u8; 32],
) -> Result<NovNativeSealSubjectV1> {
    validator_set.validate()?;
    if record.chain_id != validator_set.chain_id || record.height < validator_set.activation_height
    {
        bail!("NOV native seal candidate is outside the validator-set activation domain");
    }
    let inline_body_commitment = inline_body_commitment_v1(
        record.chain_id,
        record.height,
        &record.block_hash,
        &record.ordered_tx_root,
        &record.body_digest,
        record.body_bytes,
        record.tx_count,
    );
    let network_domain_commitment = network_domain_commitment_v1(
        record.chain_id,
        &genesis_block_hash,
        &protocol_config_commitment,
    );
    let aoem_parent_commitment = aoem_parent_commitment_v1(block);
    let mut subject = NovNativeSealSubjectV1 {
        schema: NOV_NATIVE_BLOCK_SEAL_SUBJECT_SCHEMA_V1.to_string(),
        protocol_version: NOV_NATIVE_BLOCK_SEAL_PROTOCOL_VERSION_V1.to_string(),
        proof_version: NOV_NATIVE_BLOCK_SEAL_PROOF_VERSION_V1.to_string(),
        verification_profile: NOV_NATIVE_BLOCK_SEAL_VERIFICATION_PROFILE_V1.to_string(),
        phase: NOV_NATIVE_BLOCK_SEAL_PHASE_V1.to_string(),
        chain_id: record.chain_id,
        epoch: validator_set.epoch,
        height: record.height,
        slot: record.slot,
        round,
        timestamp_unix_ms: record.timestamp_unix_ms,
        validator_set_hash: validator_set.validator_set_hash,
        justify_qc_hash,
        genesis_block_hash,
        network_domain_commitment,
        block_hash: record.block_hash,
        parent_block_hash: record.parent_block_hash,
        candidate_id: record.candidate_id,
        execution_context_commitment: record.execution_context_commitment,
        protocol_config_commitment,
        pre_state_root: record.pre_state_root,
        post_state_root: record.post_state_root,
        post_state_root_codec: block.header.post_state_root_codec.clone(),
        ordered_tx_root: record.ordered_tx_root,
        body_digest: record.body_digest,
        data_availability_scheme: "inline-full-body-digest/v1".to_string(),
        inline_body_commitment,
        body_bytes: record.body_bytes,
        tx_count: record.tx_count,
        receipt_count: block.header.receipt_count,
        block_receipt_root: record.block_receipt_root,
        cumulative_receipt_root: record.cumulative_receipt_root,
        cumulative_receipt_root_codec: block.header.cumulative_receipt_root_codec.clone(),
        state_version: record.state_version,
        aoem_parent_commitment,
        aoem_batch_id: record.aoem_batch_id.clone(),
        aoem_batch_result_id: record.aoem_batch_result_id.clone(),
        aoem_expected_output_commitment: block.header.aoem_expected_output_commitment.clone(),
        aoem_evidence_commitment: record.aoem_evidence_commitment,
        execution_evidence_kind: block.execution_evidence.evidence_kind.clone(),
        subject_hash: [0u8; 32],
    };
    subject.subject_hash = subject_hash_v1(&subject);
    subject.validate(validator_set)?;
    Ok(subject)
}

fn sign_proposal_v1(
    subject: NovNativeSealSubjectV1,
    validator_set: &NovNativeSealValidatorSetV1,
    signing_key: &SigningKey,
) -> Result<NovNativeSealProposalV1> {
    subject.validate(validator_set)?;
    let proposer_id = validator_id_v1(signing_key.verifying_key().as_bytes());
    let validator = validator_set
        .validator(proposer_id)
        .context("NOV native seal proposal signer is not in the validator set")?;
    if validator.public_key != *signing_key.verifying_key().as_bytes() {
        bail!("NOV native seal proposal signer public key binding mismatch");
    }
    let signature = signing_key
        .sign(&proposal_signing_message_v1(
            &subject.subject_hash,
            &proposer_id,
        ))
        .to_bytes()
        .to_vec();
    let mut proposal = NovNativeSealProposalV1 {
        schema: NOV_NATIVE_BLOCK_SEAL_PROPOSAL_SCHEMA_V1.to_string(),
        subject_hash: subject.subject_hash,
        subject,
        proposer_id,
        signature_scheme: NOV_NATIVE_BLOCK_SEAL_SIGNATURE_SCHEME_V1.to_string(),
        signature,
        proposal_hash: [0u8; 32],
    };
    proposal.proposal_hash = proposal_hash_v1(&proposal);
    proposal.verify(validator_set)?;
    Ok(proposal)
}

fn sign_vote_v1(
    proposal: &NovNativeSealProposalV1,
    validator_set: &NovNativeSealValidatorSetV1,
    signing_key: &SigningKey,
) -> Result<NovNativeSealVoteV1> {
    proposal.verify(validator_set)?;
    let subject = &proposal.subject;
    subject.validate(validator_set)?;
    let validator_id = validator_id_v1(signing_key.verifying_key().as_bytes());
    let validator = validator_set
        .validator(validator_id)
        .context("NOV native seal vote signer is not in the validator set")?;
    if validator.public_key != *signing_key.verifying_key().as_bytes() {
        bail!("NOV native seal vote signer public key binding mismatch");
    }
    let mut vote = NovNativeSealVoteV1 {
        schema: NOV_NATIVE_BLOCK_SEAL_VOTE_SCHEMA_V1.to_string(),
        chain_id: subject.chain_id,
        epoch: subject.epoch,
        height: subject.height,
        round: subject.round,
        phase: subject.phase.clone(),
        validator_set_hash: subject.validator_set_hash,
        subject_hash: subject.subject_hash,
        proposal_hash: proposal.proposal_hash,
        validator_id,
        signature_scheme: NOV_NATIVE_BLOCK_SEAL_SIGNATURE_SCHEME_V1.to_string(),
        signature: Vec::new(),
        vote_hash: [0u8; 32],
    };
    vote.signature = signing_key
        .sign(&vote_signing_message_v1(&vote))
        .to_bytes()
        .to_vec();
    vote.vote_hash = vote_hash_v1(&vote);
    vote.verify(subject, proposal.proposal_hash, validator_set)?;
    Ok(vote)
}

fn validator_id_v1(public_key: &[u8; 32]) -> [u8; 32] {
    hash_parts_v1(VALIDATOR_ID_DOMAIN_V1, &[public_key.as_slice()])
}

fn validator_set_hash_v1(
    chain_id: u64,
    epoch: u64,
    activation_height: u64,
    validators: &[NovNativeSealValidatorV1],
    total_weight: u64,
    quorum_weight: u64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(VALIDATOR_SET_HASH_DOMAIN_V1);
    hasher.update(chain_id.to_be_bytes());
    hasher.update(epoch.to_be_bytes());
    hasher.update(activation_height.to_be_bytes());
    hasher.update((validators.len() as u64).to_be_bytes());
    for validator in validators {
        hasher.update(validator.validator_id);
        hasher.update(validator.public_key);
        hasher.update(validator.weight.to_be_bytes());
    }
    hasher.update(total_weight.to_be_bytes());
    hasher.update(quorum_weight.to_be_bytes());
    hasher.finalize().into()
}

fn network_domain_commitment_v1(
    chain_id: u64,
    genesis_block_hash: &[u8; 32],
    protocol_config_commitment: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(NETWORK_DOMAIN_COMMITMENT_DOMAIN_V1);
    hasher.update(chain_id.to_be_bytes());
    hasher.update(genesis_block_hash);
    hasher.update(protocol_config_commitment);
    hasher.finalize().into()
}

fn inline_body_commitment_v1(
    chain_id: u64,
    height: u64,
    block_hash: &[u8; 32],
    ordered_tx_root: &[u8; 32],
    body_digest: &[u8; 32],
    body_bytes: u64,
    tx_count: u32,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(INLINE_BODY_COMMITMENT_DOMAIN_V1);
    hasher.update(chain_id.to_be_bytes());
    hasher.update(height.to_be_bytes());
    hasher.update(block_hash);
    hasher.update(ordered_tx_root);
    hasher.update(body_digest);
    hasher.update(body_bytes.to_be_bytes());
    hasher.update(tx_count.to_be_bytes());
    hasher.finalize().into()
}

fn aoem_parent_commitment_v1(block: &NovNativeDurableBlockV1) -> [u8; 32] {
    let Some(parent) = block.header.aoem_parent.as_ref() else {
        return [0u8; 32];
    };
    let mut hasher = Sha256::new();
    hasher.update(AOEM_PARENT_COMMITMENT_DOMAIN_V1);
    update_len_prefixed_v1(&mut hasher, parent.batch_id.as_bytes());
    update_len_prefixed_v1(&mut hasher, parent.batch_result_id.as_bytes());
    hasher.update(parent.state_root);
    update_len_prefixed_v1(&mut hasher, parent.state_root_codec.as_bytes());
    hasher.update(parent.cumulative_receipt_root);
    update_len_prefixed_v1(&mut hasher, parent.receipt_root_codec.as_bytes());
    hasher.update(parent.state_version.to_be_bytes());
    hasher.finalize().into()
}

fn subject_hash_v1(subject: &NovNativeSealSubjectV1) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(SUBJECT_HASH_DOMAIN_V1);
    update_len_prefixed_v1(&mut hasher, subject.protocol_version.as_bytes());
    update_len_prefixed_v1(&mut hasher, subject.proof_version.as_bytes());
    update_len_prefixed_v1(&mut hasher, subject.verification_profile.as_bytes());
    update_len_prefixed_v1(&mut hasher, subject.phase.as_bytes());
    hasher.update(subject.chain_id.to_be_bytes());
    hasher.update(subject.epoch.to_be_bytes());
    hasher.update(subject.height.to_be_bytes());
    hasher.update(subject.slot.to_be_bytes());
    hasher.update(subject.round.to_be_bytes());
    hasher.update(subject.timestamp_unix_ms.to_be_bytes());
    hasher.update(subject.validator_set_hash);
    hasher.update(subject.justify_qc_hash);
    hasher.update(subject.genesis_block_hash);
    hasher.update(subject.network_domain_commitment);
    hasher.update(subject.block_hash);
    hasher.update(subject.parent_block_hash);
    hasher.update(subject.candidate_id);
    hasher.update(subject.execution_context_commitment);
    hasher.update(subject.protocol_config_commitment);
    hasher.update(subject.pre_state_root);
    hasher.update(subject.post_state_root);
    update_len_prefixed_v1(&mut hasher, subject.post_state_root_codec.as_bytes());
    hasher.update(subject.ordered_tx_root);
    hasher.update(subject.body_digest);
    update_len_prefixed_v1(&mut hasher, subject.data_availability_scheme.as_bytes());
    hasher.update(subject.inline_body_commitment);
    hasher.update(subject.body_bytes.to_be_bytes());
    hasher.update(subject.tx_count.to_be_bytes());
    hasher.update(subject.receipt_count.to_be_bytes());
    hasher.update(subject.block_receipt_root);
    hasher.update(subject.cumulative_receipt_root);
    update_len_prefixed_v1(
        &mut hasher,
        subject.cumulative_receipt_root_codec.as_bytes(),
    );
    hasher.update(subject.state_version.to_be_bytes());
    hasher.update(subject.aoem_parent_commitment);
    update_len_prefixed_v1(&mut hasher, subject.aoem_batch_id.as_bytes());
    update_len_prefixed_v1(&mut hasher, subject.aoem_batch_result_id.as_bytes());
    update_len_prefixed_v1(
        &mut hasher,
        subject.aoem_expected_output_commitment.as_bytes(),
    );
    hasher.update(subject.aoem_evidence_commitment);
    update_len_prefixed_v1(&mut hasher, subject.execution_evidence_kind.as_bytes());
    hasher.finalize().into()
}

fn proposal_signing_message_v1(subject_hash: &[u8; 32], proposer_id: &[u8; 32]) -> [u8; 32] {
    hash_parts_v1(
        PROPOSAL_SIGNING_DOMAIN_V1,
        &[subject_hash.as_slice(), proposer_id.as_slice()],
    )
}

fn proposal_hash_v1(proposal: &NovNativeSealProposalV1) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(PROPOSAL_HASH_DOMAIN_V1);
    hasher.update(proposal.subject_hash);
    hasher.update(proposal.proposer_id);
    update_len_prefixed_v1(&mut hasher, proposal.signature_scheme.as_bytes());
    update_len_prefixed_v1(&mut hasher, proposal.signature.as_slice());
    hasher.finalize().into()
}

fn vote_signing_message_v1(vote: &NovNativeSealVoteV1) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(VOTE_SIGNING_DOMAIN_V1);
    hasher.update(vote.chain_id.to_be_bytes());
    hasher.update(vote.epoch.to_be_bytes());
    hasher.update(vote.height.to_be_bytes());
    hasher.update(vote.round.to_be_bytes());
    update_len_prefixed_v1(&mut hasher, vote.phase.as_bytes());
    hasher.update(vote.validator_set_hash);
    hasher.update(vote.subject_hash);
    hasher.update(vote.proposal_hash);
    hasher.update(vote.validator_id);
    hasher.finalize().into()
}

fn vote_hash_v1(vote: &NovNativeSealVoteV1) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(VOTE_HASH_DOMAIN_V1);
    hasher.update(vote_signing_message_v1(vote));
    update_len_prefixed_v1(&mut hasher, vote.signature_scheme.as_bytes());
    update_len_prefixed_v1(&mut hasher, vote.signature.as_slice());
    hasher.finalize().into()
}

fn qc_hash_v1(qc: &NovNativeSealQuorumCertificateV1) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(QC_HASH_DOMAIN_V1);
    hasher.update(qc.subject_hash);
    hasher.update(qc.proposal_hash);
    hasher.update(qc.validator_set_hash);
    hasher.update(qc.signature_count.to_be_bytes());
    hasher.update(qc.signed_weight.to_be_bytes());
    hasher.update(qc.quorum_weight.to_be_bytes());
    for vote in &qc.votes {
        hasher.update(vote.vote_hash);
        hasher.update(vote.validator_id);
        update_len_prefixed_v1(&mut hasher, vote.signature.as_slice());
    }
    hasher.finalize().into()
}

fn competing_qc_evidence_v1(
    left: &NovNativeSealQuorumCertificateV1,
    right: &NovNativeSealQuorumCertificateV1,
) -> NovNativeSealCompetingQcEvidenceV1 {
    let (left_qc_hash, right_qc_hash) = if left.qc_hash < right.qc_hash {
        (left.qc_hash, right.qc_hash)
    } else {
        (right.qc_hash, left.qc_hash)
    };
    let mut hasher = Sha256::new();
    hasher.update(COMPETING_QC_EVIDENCE_DOMAIN_V1);
    hasher.update(left.subject.chain_id.to_be_bytes());
    hasher.update(left.subject.epoch.to_be_bytes());
    hasher.update(left.subject.height.to_be_bytes());
    hasher.update(left.subject.round.to_be_bytes());
    hasher.update(left_qc_hash);
    hasher.update(right_qc_hash);
    NovNativeSealCompetingQcEvidenceV1 {
        schema: COMPETING_QC_EVIDENCE_SCHEMA_V1.to_string(),
        chain_id: left.subject.chain_id,
        epoch: left.subject.epoch,
        height: left.subject.height,
        round: left.subject.round,
        left_qc_hash,
        right_qc_hash,
        evidence_hash: hasher.finalize().into(),
    }
}

fn validate_competing_qc_evidence_v1(evidence: &NovNativeSealCompetingQcEvidenceV1) -> Result<()> {
    if evidence.schema != COMPETING_QC_EVIDENCE_SCHEMA_V1
        || evidence.chain_id == 0
        || evidence.epoch == 0
        || evidence.height == 0
        || evidence.left_qc_hash >= evidence.right_qc_hash
    {
        bail!("NOV native competing-QC evidence metadata is invalid");
    }
    let mut hasher = Sha256::new();
    hasher.update(COMPETING_QC_EVIDENCE_DOMAIN_V1);
    hasher.update(evidence.chain_id.to_be_bytes());
    hasher.update(evidence.epoch.to_be_bytes());
    hasher.update(evidence.height.to_be_bytes());
    hasher.update(evidence.round.to_be_bytes());
    hasher.update(evidence.left_qc_hash);
    hasher.update(evidence.right_qc_hash);
    if evidence.evidence_hash != <[u8; 32]>::from(hasher.finalize()) {
        bail!("NOV native competing-QC evidence commitment is invalid");
    }
    Ok(())
}

fn store_binding_v1(
    ledger: &NovNativeBlockLedgerV1,
    chain_id: u64,
) -> Result<NovNativeSealStoreBindingV1> {
    let ownership = ledger
        .load_aoem_ownership()?
        .context("NOV native seal store binding requires AOEM ownership metadata")?;
    if ownership.chain_id != chain_id {
        bail!("NOV native seal store AOEM ownership chain binding mismatch");
    }
    let genesis_block_hash = ledger
        .load_by_height(chain_id, 1)?
        .context("NOV native seal store binding requires a durable genesis block")?
        .header
        .block_hash;
    let namespace_digest =
        decode_hex_commitment_v1("AOEM namespace digest", ownership.namespace_digest.as_str())?;
    let protocol_config_commitment = decode_hex_commitment_v1(
        "AOEM protocol config commitment",
        ownership.protocol_config_commitment.as_str(),
    )?;
    let ledger_identity_commitment = hash_parts_v1(
        LEDGER_IDENTITY_COMMITMENT_DOMAIN_V1,
        &[
            chain_id.to_be_bytes().as_slice(),
            genesis_block_hash.as_slice(),
            namespace_digest.as_slice(),
            protocol_config_commitment.as_slice(),
        ],
    );
    let binding = NovNativeSealStoreBindingV1 {
        schema: STORE_BINDING_SCHEMA_V1.to_string(),
        chain_id,
        genesis_block_hash,
        namespace_digest,
        protocol_config_commitment,
        ledger_identity_commitment,
    };
    validate_store_binding_v1(&binding)?;
    Ok(binding)
}

fn validate_store_binding_v1(binding: &NovNativeSealStoreBindingV1) -> Result<()> {
    if binding.schema != STORE_BINDING_SCHEMA_V1
        || binding.chain_id == 0
        || binding.genesis_block_hash == [0u8; 32]
        || binding.namespace_digest == [0u8; 32]
        || binding.protocol_config_commitment == [0u8; 32]
    {
        bail!("NOV native seal store binding is invalid");
    }
    let expected = hash_parts_v1(
        LEDGER_IDENTITY_COMMITMENT_DOMAIN_V1,
        &[
            binding.chain_id.to_be_bytes().as_slice(),
            binding.genesis_block_hash.as_slice(),
            binding.namespace_digest.as_slice(),
            binding.protocol_config_commitment.as_slice(),
        ],
    );
    if binding.ledger_identity_commitment != expected {
        bail!("NOV native seal store identity commitment is invalid");
    }
    Ok(())
}

fn outbox_entry_v1(
    object_kind: &str,
    object_hash: [u8; 32],
    subject: &NovNativeSealSubjectV1,
    validator_id: [u8; 32],
) -> NovNativeSealOutboxEntryV1 {
    NovNativeSealOutboxEntryV1 {
        schema: OUTBOX_SCHEMA_V1.to_string(),
        object_kind: object_kind.to_string(),
        object_hash,
        subject_hash: subject.subject_hash,
        chain_id: subject.chain_id,
        epoch: subject.epoch,
        height: subject.height,
        round: subject.round,
        validator_id,
        emit_state: "ready_to_emit".to_string(),
    }
}

fn validate_outbox_entry_v1(entry: &NovNativeSealOutboxEntryV1) -> Result<()> {
    if entry.schema != OUTBOX_SCHEMA_V1
        || !matches!(entry.object_kind.as_str(), "proposal" | "vote")
        || entry.object_hash == [0u8; 32]
        || entry.subject_hash == [0u8; 32]
        || entry.chain_id == 0
        || entry.epoch == 0
        || entry.height == 0
        || entry.validator_id == [0u8; 32]
        || entry.emit_state != "ready_to_emit"
    {
        bail!("NOV native seal outbox entry is invalid");
    }
    Ok(())
}

fn validate_round_lock_v1(
    lock: &NovNativeSealRoundLockV1,
    subject: &NovNativeSealSubjectV1,
    validator_id: [u8; 32],
) -> Result<()> {
    if lock.schema != ROUND_LOCK_SCHEMA_V1
        || lock.chain_id != subject.chain_id
        || lock.epoch != subject.epoch
        || lock.height != subject.height
        || lock.round != subject.round
        || lock.validator_id != validator_id
    {
        bail!("NOV native seal round safety lock key binding mismatch");
    }
    if lock.subject_hash != subject.subject_hash {
        bail!("NOV native seal validator already signed a competing subject in this round");
    }
    Ok(())
}

fn validate_height_lock_identity_v1(
    lock: &NovNativeSealHeightLockV1,
    subject: &NovNativeSealSubjectV1,
    validator_id: [u8; 32],
) -> Result<()> {
    if lock.schema != HEIGHT_LOCK_SCHEMA_V1
        || lock.chain_id != subject.chain_id
        || lock.epoch != subject.epoch
        || lock.height != subject.height
        || lock.validator_id != validator_id
        || lock.block_hash == [0u8; 32]
        || lock.revision == 0
        || lock.first_round > lock.highest_round
    {
        bail!("NOV native seal height safety lock key binding mismatch");
    }
    Ok(())
}

fn validate_proposal_lock_v1(
    lock: &NovNativeSealProposalLockV1,
    subject: &NovNativeSealSubjectV1,
    proposer_id: [u8; 32],
) -> Result<()> {
    if lock.schema != PROPOSAL_LOCK_SCHEMA_V1
        || lock.chain_id != subject.chain_id
        || lock.epoch != subject.epoch
        || lock.height != subject.height
        || lock.round != subject.round
        || lock.proposer_id != proposer_id
        || lock.proposal_hash == [0u8; 32]
    {
        bail!("NOV native seal proposal lock key binding mismatch");
    }
    if lock.subject_hash != subject.subject_hash {
        bail!("NOV native seal proposer already signed a competing proposal");
    }
    Ok(())
}

fn validate_vote_lock_v1(
    lock: &NovNativeSealVoteLockV1,
    subject: &NovNativeSealSubjectV1,
    validator_id: [u8; 32],
) -> Result<()> {
    if lock.schema != VOTE_LOCK_SCHEMA_V1
        || lock.chain_id != subject.chain_id
        || lock.epoch != subject.epoch
        || lock.height != subject.height
        || lock.round != subject.round
        || lock.validator_id != validator_id
        || lock.vote_hash == [0u8; 32]
    {
        bail!("NOV native seal vote lock key binding mismatch");
    }
    if lock.subject_hash != subject.subject_hash {
        bail!("NOV native seal validator already signed a competing vote");
    }
    Ok(())
}

fn validate_qc_index_v1(
    index: &NovNativeSealQcIndexV1,
    index_kind: &str,
    chain_id: u64,
    epoch: u64,
    height: u64,
    binding_hash: [u8; 32],
) -> Result<()> {
    if index.schema != QC_INDEX_SCHEMA_V1
        || index.index_kind != index_kind
        || index.chain_id != chain_id
        || index.epoch != epoch
        || index.height != height
        || index.binding_hash != binding_hash
    {
        bail!("NOV native seal QC index metadata is invalid");
    }
    validate_qc_hashes_v1(index.qc_hashes.as_slice())
}

fn validate_qc_hashes_v1(qc_hashes: &[[u8; 32]]) -> Result<()> {
    if qc_hashes.is_empty()
        || qc_hashes.len() > NOV_NATIVE_BLOCK_SEAL_MAX_QCS_PER_INDEX_V1
        || qc_hashes.windows(2).any(|pair| pair[0] >= pair[1])
    {
        bail!("NOV native seal QC index is empty, unsorted, duplicated, or oversized");
    }
    Ok(())
}

fn stage_validator_set_v1(
    db: &DB,
    batch: &mut RocksDbWriteBatch,
    validator_set: &NovNativeSealValidatorSetV1,
) -> Result<()> {
    validator_set.validate()?;
    let epoch_key = validator_set_epoch_key_v1(validator_set.chain_id, validator_set.epoch);
    if let Some(existing) =
        read_json_v1::<NovNativeSealValidatorSetV1>(db, epoch_key.as_bytes(), "validator set")?
    {
        existing.validate()?;
        if existing != *validator_set {
            bail!("NOV native seal validator epoch conflicts with a durable set");
        }
    } else {
        put_json_v1(batch, epoch_key.as_bytes(), validator_set, "validator set")?;
    }
    let hash_key = validator_set_hash_key_v1(&validator_set.validator_set_hash);
    if let Some(existing) = read_json_v1::<NovNativeSealValidatorSetV1>(
        db,
        hash_key.as_bytes(),
        "validator set hash object",
    )? {
        if existing != *validator_set {
            bail!("NOV native seal validator-set hash collision");
        }
    } else {
        put_json_v1(
            batch,
            hash_key.as_bytes(),
            validator_set,
            "validator set hash object",
        )?;
    }
    Ok(())
}

fn stage_object_if_available_v1<T>(
    db: &DB,
    batch: &mut RocksDbWriteBatch,
    key: &[u8],
    value: &T,
    label: &str,
) -> Result<()>
where
    T: Serialize + DeserializeOwned + PartialEq,
{
    if let Some(existing) = read_json_v1::<T>(db, key, label)? {
        if existing != *value {
            bail!("NOV native seal {label} conflicts with an existing hash");
        }
    } else {
        put_json_v1(batch, key, value, label)?;
    }
    Ok(())
}

fn decode_hex_commitment_v1(label: &str, value: &str) -> Result<[u8; 32]> {
    validate_hex_commitment_v1(label, value)?;
    let mut out = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = decode_hex_nibble_v1(pair[0]).context("invalid commitment hex")?;
        let low = decode_hex_nibble_v1(pair[1]).context("invalid commitment hex")?;
        out[index] = (high << 4) | low;
    }
    Ok(out)
}

fn decode_hex_nibble_v1(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn validate_hex_commitment_v1(label: &str, value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("{label} must be a canonical lowercase 32-byte hex commitment");
    }
    Ok(())
}

fn validate_ascii_id_v1(label: &str, value: &str, max_bytes: usize) -> Result<()> {
    if value.is_empty() || value.trim() != value || value.len() > max_bytes || !value.is_ascii() {
        bail!("{label} must be non-empty canonical ASCII and at most {max_bytes} bytes");
    }
    Ok(())
}

fn hash_parts_v1(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        update_len_prefixed_v1(&mut hasher, part);
    }
    hasher.finalize().into()
}

fn update_len_prefixed_v1(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn write_sync_v1(db: &DB, batch: RocksDbWriteBatch) -> Result<()> {
    let mut options = WriteOptions::default();
    options.set_sync(true);
    db.write_opt(batch, &options)
        .context("write synchronized NOV native seal batch")
}

fn read_json_v1<T: DeserializeOwned>(db: &DB, key: &[u8], label: &str) -> Result<Option<T>> {
    db.get(key)
        .with_context(|| format!("read NOV native seal {label} failed"))?
        .map(|raw| {
            serde_json::from_slice(raw.as_slice())
                .with_context(|| format!("decode NOV native seal {label} failed"))
        })
        .transpose()
}

fn put_json_v1<T: Serialize>(
    batch: &mut RocksDbWriteBatch,
    key: &[u8],
    value: &T,
    label: &str,
) -> Result<()> {
    let encoded = serde_json::to_vec(value)
        .with_context(|| format!("encode NOV native seal {label} failed"))?;
    batch.put(key, encoded);
    Ok(())
}

fn seal_store_process_registry_v1(
) -> &'static Mutex<HashMap<String, Weak<NovNativeBlockSealProcessEntryV1>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Weak<NovNativeBlockSealProcessEntryV1>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn seal_store_process_key_v1(path: &Path) -> Result<String> {
    let absolute = std::path::absolute(path)
        .with_context(|| format!("resolve NOV native seal store path: {}", path.display()))?;
    let mut key = absolute.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        key.make_ascii_lowercase();
    }
    Ok(key)
}

fn hex_v1(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn store_binding_key_v1(chain_id: u64) -> String {
    format!("{KEY_PREFIX_V1}binding/{chain_id:020}")
}

fn validator_set_epoch_key_v1(chain_id: u64, epoch: u64) -> String {
    format!("{KEY_PREFIX_V1}validator_set/by_epoch/{chain_id:020}/{epoch:020}")
}

fn validator_set_hash_key_v1(hash: &[u8; 32]) -> String {
    format!("{KEY_PREFIX_V1}validator_set/object/{}", hex_v1(hash))
}

fn proposal_object_key_v1(hash: &[u8; 32]) -> String {
    format!("{KEY_PREFIX_V1}proposal/object/{}", hex_v1(hash))
}

fn vote_object_key_v1(hash: &[u8; 32]) -> String {
    format!("{KEY_PREFIX_V1}vote/object/{}", hex_v1(hash))
}

fn qc_object_key_v1(hash: &[u8; 32]) -> String {
    format!("{KEY_PREFIX_V1}qc/object/{}", hex_v1(hash))
}

fn proposal_lock_key_v1(subject: &NovNativeSealSubjectV1, validator_id: [u8; 32]) -> String {
    format!(
        "{KEY_PREFIX_V1}proposal/local_lock/{:020}/{:020}/{:020}/{:020}/{}",
        subject.chain_id,
        subject.epoch,
        subject.height,
        subject.round,
        hex_v1(&validator_id)
    )
}

fn vote_lock_key_v1(subject: &NovNativeSealSubjectV1, validator_id: [u8; 32]) -> String {
    format!(
        "{KEY_PREFIX_V1}vote/local_lock/{:020}/{:020}/{:020}/{:020}/{}",
        subject.chain_id,
        subject.epoch,
        subject.height,
        subject.round,
        hex_v1(&validator_id)
    )
}

fn round_lock_key_v1(subject: &NovNativeSealSubjectV1, validator_id: [u8; 32]) -> String {
    format!(
        "{KEY_PREFIX_V1}safety/round/{:020}/{:020}/{:020}/{:020}/{}",
        subject.chain_id,
        subject.epoch,
        subject.height,
        subject.round,
        hex_v1(&validator_id)
    )
}

fn height_lock_key_v1(subject: &NovNativeSealSubjectV1, validator_id: [u8; 32]) -> String {
    format!(
        "{KEY_PREFIX_V1}safety/height/{:020}/{:020}/{:020}/{}",
        subject.chain_id,
        subject.epoch,
        subject.height,
        hex_v1(&validator_id)
    )
}

fn outbox_key_v1(kind: &str, hash: &[u8; 32]) -> String {
    format!("{KEY_PREFIX_V1}outbox/{kind}/{}", hex_v1(hash))
}

fn qc_subject_index_key_v1(subject_hash: &[u8; 32]) -> String {
    format!("{KEY_PREFIX_V1}qc/index/subject/{}", hex_v1(subject_hash))
}

fn qc_block_index_key_v1(chain_id: u64, block_hash: &[u8; 32]) -> String {
    format!(
        "{KEY_PREFIX_V1}qc/index/block/{chain_id:020}/{}",
        hex_v1(block_hash)
    )
}

fn qc_height_index_key_v1(chain_id: u64, epoch: u64, height: u64) -> String {
    format!("{KEY_PREFIX_V1}qc/index/height/{chain_id:020}/{epoch:020}/{height:020}")
}

fn competing_qc_evidence_prefix_v1(chain_id: u64, epoch: u64, height: u64, round: u64) -> String {
    format!("{KEY_PREFIX_V1}qc/conflict/{chain_id:020}/{epoch:020}/{height:020}/{round:020}/")
}

fn competing_qc_evidence_key_v1(
    chain_id: u64,
    epoch: u64,
    height: u64,
    round: u64,
    evidence_hash: &[u8; 32],
) -> String {
    format!(
        "{}{}",
        competing_qc_evidence_prefix_v1(chain_id, epoch, height, round),
        hex_v1(evidence_hash)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_block_ledger::{
        NovNativeBlockCandidateInputV1, NovNativeBlockCommitInputV1, NovNativePreparedAoemParentV1,
    };
    use novovm_protocol::NovBlockExecutionContextV1;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_SERIAL_V1: AtomicU64 = AtomicU64::new(1);

    struct TestNodeV1 {
        root: PathBuf,
        ledger: Option<NovNativeBlockLedgerV1>,
        store: Option<NovNativeBlockSealStoreV1>,
    }

    impl TestNodeV1 {
        fn new(label: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test clock")
                .as_nanos();
            let serial = TEST_SERIAL_V1.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "novovm-native-seal-{label}-{}-{serial}-{nanos}",
                std::process::id()
            ));
            let ledger = NovNativeBlockLedgerV1::open(root.join("ledger").as_path())
                .expect("open test ledger");
            let store = NovNativeBlockSealStoreV1::open(root.join("seal").as_path())
                .expect("open test seal store");
            Self {
                root,
                ledger: Some(ledger),
                store: Some(store),
            }
        }

        fn ledger(&self) -> &NovNativeBlockLedgerV1 {
            self.ledger.as_ref().expect("test ledger is open")
        }

        fn store(&self) -> &NovNativeBlockSealStoreV1 {
            self.store.as_ref().expect("test seal store is open")
        }

        fn reopen_store(&mut self) {
            self.store.take();
            self.store = Some(
                NovNativeBlockSealStoreV1::open(self.root.join("seal").as_path())
                    .expect("reopen test seal store"),
            );
        }
    }

    impl Drop for TestNodeV1 {
        fn drop(&mut self) {
            self.store.take();
            self.ledger.take();
            let _ = fs::remove_dir_all(self.root.as_path());
        }
    }

    fn validator_fixture_v1(chain_id: u64) -> (Vec<SigningKey>, NovNativeSealValidatorSetV1) {
        let keys = (1u8..=4)
            .map(|seed| SigningKey::from_bytes(&[seed; 32]))
            .collect::<Vec<_>>();
        let validators = [2usize, 0, 3, 1]
            .into_iter()
            .map(|index| {
                NovNativeSealValidatorV1::new(*keys[index].verifying_key().as_bytes(), 1)
                    .expect("build validator")
            })
            .collect();
        let set = NovNativeSealValidatorSetV1::new(chain_id, 1, 1, validators)
            .expect("build validator set");
        (keys, set)
    }

    fn bind_ownership_v1(ledger: &NovNativeBlockLedgerV1, chain_id: u64) {
        ledger
            .bind_aoem_ownership(chain_id, &"a1".repeat(32), &"b2".repeat(32))
            .expect("bind AOEM ownership");
    }

    fn commit_block_v1(
        ledger: &NovNativeBlockLedgerV1,
        chain_id: u64,
        height: u64,
        parent: Option<&NovNativeDurableBlockV1>,
        seed: u8,
    ) -> NovNativeDurableBlockV1 {
        let parent_hash = parent
            .map(|block| block.header.block_hash)
            .unwrap_or([0u8; 32]);
        let pre_state_root = parent
            .map(|block| block.header.post_state_root)
            .unwrap_or([0x11; 32]);
        let aoem_parent = parent.map(|block| NovNativePreparedAoemParentV1 {
            batch_id: block.header.aoem_batch_id.clone(),
            batch_result_id: block.header.aoem_batch_result_id.clone(),
            state_root: block.header.post_state_root,
            state_root_codec: block.header.post_state_root_codec.clone(),
            cumulative_receipt_root: block.header.cumulative_receipt_root,
            receipt_root_codec: block.header.cumulative_receipt_root_codec.clone(),
            state_version: block.header.state_version,
        });
        let context = NovBlockExecutionContextV1 {
            chain_id,
            block_height: height,
            parent_block_hash: parent_hash,
            slot: height.checked_mul(2).expect("slot"),
            timestamp_unix_ms: 1_900_000_000_000u64
                .checked_add(height.checked_mul(2_000).expect("timestamp step"))
                .expect("timestamp"),
        };
        let prepared = ledger
            .prepare(NovNativeBlockCandidateInputV1 {
                context,
                tx_hashes: vec![[seed; 32]],
                raw_txs: vec![vec![seed, height as u8, 0x7f]],
                pre_state_root,
                aoem_parent,
            })
            .expect("prepare block");
        let state_version = parent
            .map(|block| {
                block
                    .header
                    .state_version
                    .checked_add(1)
                    .expect("state version")
            })
            .unwrap_or(1);
        let mut batch_result = [seed.wrapping_add(1); 32];
        batch_result[24..].copy_from_slice(&state_version.to_be_bytes());
        let input = NovNativeBlockCommitInputV1 {
            post_state_root: [seed.wrapping_add(2); 32],
            cumulative_receipt_root: [seed.wrapping_add(3); 32],
            per_block_receipt_commitments: vec![[seed.wrapping_add(4); 32]],
            aoem_batch_id: format!("seal-batch-{height}-{seed}"),
            aoem_batch_result_id: hex_v1(&batch_result),
            aoem_evidence_commitment: [seed.wrapping_add(5); 32],
            state_version,
        };
        let bound = ledger
            .bind_expected_aoem_batch_id(
                &prepared,
                input.aoem_batch_id.as_str(),
                format!("{:064x}", state_version).as_str(),
            )
            .expect("bind expected AOEM result");
        ledger.commit(&bound, input).expect("commit block")
    }

    fn genesis_fixture_v1(
        label: &str,
        chain_id: u64,
    ) -> (
        TestNodeV1,
        NovNativeDurableBlockV1,
        Vec<SigningKey>,
        NovNativeSealValidatorSetV1,
    ) {
        let node = TestNodeV1::new(label);
        bind_ownership_v1(node.ledger(), chain_id);
        let block = commit_block_v1(node.ledger(), chain_id, 1, None, 0x31);
        let (keys, set) = validator_fixture_v1(chain_id);
        (node, block, keys, set)
    }

    fn proposal_and_votes_v1(
        node: &TestNodeV1,
        block: &NovNativeDurableBlockV1,
        keys: &[SigningKey],
        set: &NovNativeSealValidatorSetV1,
        round: u64,
        vote_count: usize,
    ) -> (NovNativeSealProposalV1, Vec<NovNativeSealVoteV1>) {
        let proposal = node
            .store()
            .sign_local_proposal(
                node.ledger(),
                &NovNativeSealLocalProposalRequestV1 {
                    chain_id: block.header.chain_id,
                    block_hash: block.header.block_hash,
                    round,
                    justify_qc_hash: None,
                },
                set,
                &keys[0],
            )
            .expect("sign proposal");
        let votes = keys
            .iter()
            .take(vote_count)
            .map(|key| {
                node.store()
                    .sign_local_vote(node.ledger(), &proposal, set, key)
                    .expect("sign vote")
            })
            .collect();
        (proposal, votes)
    }

    #[test]
    fn validator_set_is_canonical_and_uses_strict_two_thirds_quorum() {
        let chain_id = 81_001;
        let (keys, set) = validator_fixture_v1(chain_id);
        assert_eq!(set.validators.len(), 4);
        assert_eq!(set.total_weight, 4);
        assert_eq!(set.quorum_weight, 3);
        assert!(set
            .validators
            .windows(2)
            .all(|pair| pair[0].validator_id < pair[1].validator_id));

        let ordered = keys
            .iter()
            .map(|key| {
                NovNativeSealValidatorV1::new(*key.verifying_key().as_bytes(), 1)
                    .expect("validator")
            })
            .collect();
        let same =
            NovNativeSealValidatorSetV1::new(chain_id, 1, 1, ordered).expect("same validator set");
        assert_eq!(same, set);

        let duplicate = NovNativeSealValidatorV1::new(*keys[0].verifying_key().as_bytes(), 1)
            .expect("duplicate validator");
        assert!(NovNativeSealValidatorSetV1::new(
            chain_id,
            1,
            1,
            vec![duplicate.clone(), duplicate]
        )
        .is_err());
        assert!(NovNativeSealValidatorV1::new(*keys[0].verifying_key().as_bytes(), 0).is_err());
        assert!(NovNativeSealValidatorV1::new([0u8; 32], 1).is_err());
        let overflow = vec![
            NovNativeSealValidatorV1::new(*keys[0].verifying_key().as_bytes(), u64::MAX)
                .expect("max-weight validator"),
            NovNativeSealValidatorV1::new(*keys[1].verifying_key().as_bytes(), 1)
                .expect("overflow validator"),
        ];
        assert!(NovNativeSealValidatorSetV1::new(chain_id, 2, 2, overflow).is_err());
    }

    #[test]
    fn weighted_qc_counts_weight_instead_of_signature_count() {
        let chain_id = 81_009;
        let node = TestNodeV1::new("weighted-qc");
        bind_ownership_v1(node.ledger(), chain_id);
        let block = commit_block_v1(node.ledger(), chain_id, 1, None, 0x21);
        let keys = (1u8..=4)
            .map(|seed| SigningKey::from_bytes(&[seed.wrapping_add(10); 32]))
            .collect::<Vec<_>>();
        let validators = keys
            .iter()
            .zip([4u64, 3, 2, 1])
            .map(|(key, weight)| {
                NovNativeSealValidatorV1::new(*key.verifying_key().as_bytes(), weight)
                    .expect("weighted validator")
            })
            .collect();
        let set = NovNativeSealValidatorSetV1::new(chain_id, 1, 1, validators)
            .expect("weighted validator set");
        assert_eq!(set.total_weight, 10);
        assert_eq!(set.quorum_weight, 7);
        let proposal = node
            .store()
            .sign_local_proposal(
                node.ledger(),
                &NovNativeSealLocalProposalRequestV1 {
                    chain_id,
                    block_hash: block.header.block_hash,
                    round: 0,
                    justify_qc_hash: None,
                },
                &set,
                &keys[0],
            )
            .expect("weighted proposal");
        let votes = keys
            .iter()
            .map(|key| {
                node.store()
                    .sign_local_vote(node.ledger(), &proposal, &set, key)
                    .expect("weighted vote")
            })
            .collect::<Vec<_>>();
        let high_weight = NovNativeSealQuorumCertificateV1::from_votes(
            proposal.subject.clone(),
            &set,
            vec![votes[0].clone(), votes[1].clone()],
        )
        .expect("two high-weight votes reach quorum");
        assert_eq!(high_weight.signature_count, 2);
        assert_eq!(high_weight.signed_weight, 7);
        assert!(NovNativeSealQuorumCertificateV1::from_votes(
            proposal.subject,
            &set,
            vec![votes[1].clone(), votes[2].clone(), votes[3].clone()]
        )
        .is_err());
    }

    #[test]
    fn local_subject_is_deterministic_complete_and_does_not_mutate_candidate() {
        let chain_id = 81_002;
        let (node, block, _keys, set) = genesis_fixture_v1("subject", chain_id);
        let before_record = node
            .ledger()
            .load_candidate_record(chain_id, block.header.block_hash)
            .expect("load candidate")
            .expect("candidate");
        let before_head = node.ledger().load_head(chain_id).expect("load head");
        let before_block = node
            .ledger()
            .load_candidate_block(chain_id, block.header.block_hash)
            .expect("load candidate block");
        let tx_hash = block.body.tx_hashes[0];
        let before_tx = node
            .ledger()
            .load_tx_location(chain_id, tx_hash)
            .expect("load tx location");
        let before_receipt = node
            .ledger()
            .load_receipt_location(chain_id, tx_hash)
            .expect("load receipt location");
        let before_ownership = node
            .ledger()
            .load_aoem_ownership()
            .expect("load AOEM ownership");
        let first = node
            .store()
            .prepare_local_subject(
                node.ledger(),
                chain_id,
                block.header.block_hash,
                &set,
                0,
                None,
            )
            .expect("prepare subject");
        let repeat = node
            .store()
            .prepare_local_subject(
                node.ledger(),
                chain_id,
                block.header.block_hash,
                &set,
                0,
                None,
            )
            .expect("repeat subject");
        assert_eq!(first, repeat);
        assert_eq!(first.block_hash, block.header.block_hash);
        assert_eq!(first.protocol_config_commitment, [0xb2; 32]);
        assert_eq!(first.body_digest, block.header.body_digest);
        assert_eq!(first.block_receipt_root, block.header.block_receipt_root);
        assert_eq!(
            first.aoem_evidence_commitment,
            block.header.aoem_evidence_commitment
        );
        assert_eq!(
            hex_v1(&first.subject_hash),
            "8240e69c510bf3826d06e9597126879b6cb451481d65a69636d8424b367b8dcb"
        );

        let next_round = node
            .store()
            .prepare_local_subject(
                node.ledger(),
                chain_id,
                block.header.block_hash,
                &set,
                1,
                None,
            )
            .expect("prepare next-round subject");
        assert_ne!(next_round.subject_hash, first.subject_hash);

        let mut tampered = first.clone();
        tampered.post_state_root[0] ^= 1;
        assert!(tampered.validate(&set).is_err());
        let mut wrong_chain = first.clone();
        wrong_chain.chain_id = chain_id + 1;
        wrong_chain.subject_hash = subject_hash_v1(&wrong_chain);
        assert!(wrong_chain.validate(&set).is_err());
        let mut wrong_genesis = first.clone();
        wrong_genesis.genesis_block_hash[0] ^= 1;
        wrong_genesis.network_domain_commitment = network_domain_commitment_v1(
            wrong_genesis.chain_id,
            &wrong_genesis.genesis_block_hash,
            &wrong_genesis.protocol_config_commitment,
        );
        wrong_genesis.subject_hash = subject_hash_v1(&wrong_genesis);
        assert!(wrong_genesis.validate(&set).is_err());

        assert_eq!(
            node.ledger()
                .load_candidate_record(chain_id, block.header.block_hash)
                .expect("reload candidate")
                .expect("candidate"),
            before_record
        );
        assert_eq!(
            node.ledger().load_head(chain_id).expect("reload head"),
            before_head
        );
        assert_eq!(
            node.ledger()
                .load_candidate_block(chain_id, block.header.block_hash)
                .expect("reload candidate block"),
            before_block
        );
        assert_eq!(
            node.ledger()
                .load_tx_location(chain_id, tx_hash)
                .expect("reload tx location"),
            before_tx
        );
        assert_eq!(
            node.ledger()
                .load_receipt_location(chain_id, tx_hash)
                .expect("reload receipt location"),
            before_receipt
        );
        assert_eq!(
            node.ledger()
                .load_aoem_ownership()
                .expect("reload AOEM ownership"),
            before_ownership
        );
        assert!(!before_record.proof_sealed);
        assert!(!before_record.chain_canonical);
        assert!(!before_record.safe);
        assert!(!before_record.finalized);
    }

    #[test]
    fn proposal_vote_and_three_of_four_qc_are_durable_but_not_finality() {
        let chain_id = 81_003;
        let (mut node, block, keys, set) = genesis_fixture_v1("qc", chain_id);
        let (proposal, votes) = proposal_and_votes_v1(&node, &block, &keys, &set, 0, 3);
        assert!(NovNativeSealQuorumCertificateV1::from_votes(
            proposal.subject.clone(),
            &set,
            votes[..2].to_vec()
        )
        .is_err());
        let qc = NovNativeSealQuorumCertificateV1::from_votes(
            proposal.subject.clone(),
            &set,
            votes.clone(),
        )
        .expect("build 3-of-4 QC");
        assert_eq!(qc.signature_count, 3);
        assert_eq!(qc.signed_weight, 3);
        assert!(qc.threshold_satisfied);
        assert!(node
            .store()
            .persist_local_verified_qc(node.ledger(), &qc, &set)
            .expect("persist QC"));
        assert!(!node
            .store()
            .persist_local_verified_qc(node.ledger(), &qc, &set)
            .expect("repeat QC"));

        let signer_id = validator_id_v1(keys[0].verifying_key().as_bytes());
        let outbox = node
            .store()
            .load_pending_outbox(chain_id, signer_id, 16)
            .expect("load outbox");
        assert_eq!(outbox.len(), 2);
        assert!(outbox.iter().any(|entry| entry.object_kind == "proposal"));
        assert!(outbox.iter().any(|entry| entry.object_kind == "vote"));
        assert_eq!(
            node.store()
                .load_qcs_by_height(chain_id, 1, 1)
                .expect("height qcs"),
            vec![qc.clone()]
        );

        node.reopen_store();
        assert_eq!(
            node.store().load_qc(qc.qc_hash).expect("reload QC"),
            Some(qc.clone())
        );
        assert_eq!(
            node.store()
                .load_pending_outbox(chain_id, signer_id, 16)
                .expect("recover verified outbox after restart"),
            outbox
        );
        let proposal_retry = node
            .store()
            .sign_local_proposal(
                node.ledger(),
                &NovNativeSealLocalProposalRequestV1 {
                    chain_id,
                    block_hash: block.header.block_hash,
                    round: 0,
                    justify_qc_hash: None,
                },
                &set,
                &keys[0],
            )
            .expect("retry proposal after restart");
        assert_eq!(proposal_retry, proposal);
        let vote_retry = node
            .store()
            .sign_local_vote(node.ledger(), &proposal_retry, &set, &keys[0])
            .expect("retry vote after restart");
        assert_eq!(vote_retry, votes[0]);

        let record = node
            .ledger()
            .load_candidate_record(chain_id, block.header.block_hash)
            .expect("load candidate")
            .expect("candidate");
        assert!(!record.proof_sealed);
        assert!(!record.chain_canonical);
        assert!(!record.safe);
        assert!(!record.finalized);
        assert!(!block.header.proof_sealed);
    }

    #[test]
    fn signatures_reject_tampering_duplicate_voters_and_nonmembers() {
        let chain_id = 81_004;
        let (node, block, keys, set) = genesis_fixture_v1("negative-crypto", chain_id);
        let (proposal, votes) = proposal_and_votes_v1(&node, &block, &keys, &set, 0, 3);
        let mut bad_signature = votes[0].clone();
        bad_signature.signature[0] ^= 1;
        assert!(bad_signature
            .verify(&proposal.subject, proposal.proposal_hash, &set)
            .is_err());
        assert!(NovNativeSealQuorumCertificateV1::from_votes(
            proposal.subject.clone(),
            &set,
            vec![votes[0].clone(), votes[0].clone(), votes[1].clone()]
        )
        .is_err());

        let outsider = SigningKey::from_bytes(&[0x55; 32]);
        assert!(node
            .store()
            .sign_local_vote(node.ledger(), &proposal, &set, &outsider)
            .is_err());
        let mut wrong_proposal = proposal.clone();
        wrong_proposal.subject_hash[0] ^= 1;
        assert!(wrong_proposal.verify(&set).is_err());

        let legacy = serde_json::json!({
            "proposal_hash": vec![7u8; 32],
            "height": 1,
            "votes": [],
            "total_weight": 3
        });
        assert!(serde_json::from_value::<NovNativeSealQuorumCertificateV1>(legacy).is_err());
    }

    #[test]
    fn safety_lock_allows_same_candidate_new_round_and_rejects_competing_candidate() {
        let chain_id = 81_005;
        let (mut node, block, keys, set) = genesis_fixture_v1("safety-lock", chain_id);
        let first = node
            .store()
            .sign_local_proposal(
                node.ledger(),
                &NovNativeSealLocalProposalRequestV1 {
                    chain_id,
                    block_hash: block.header.block_hash,
                    round: 0,
                    justify_qc_hash: None,
                },
                &set,
                &keys[0],
            )
            .expect("sign first round");
        let second = node
            .store()
            .sign_local_proposal(
                node.ledger(),
                &NovNativeSealLocalProposalRequestV1 {
                    chain_id,
                    block_hash: block.header.block_hash,
                    round: 2,
                    justify_qc_hash: None,
                },
                &set,
                &keys[0],
            )
            .expect("sign next round for same candidate");
        assert_ne!(first.subject_hash, second.subject_hash);
        assert_eq!(first.subject.block_hash, second.subject.block_hash);

        node.reopen_store();
        assert!(node
            .store()
            .sign_local_proposal(
                node.ledger(),
                &NovNativeSealLocalProposalRequestV1 {
                    chain_id,
                    block_hash: block.header.block_hash,
                    round: 1,
                    justify_qc_hash: None,
                },
                &set,
                &keys[0],
            )
            .is_err());
        let mut competing = second.subject.clone();
        competing.block_hash = [0xee; 32];
        competing.inline_body_commitment = inline_body_commitment_v1(
            competing.chain_id,
            competing.height,
            &competing.block_hash,
            &competing.ordered_tx_root,
            &competing.body_digest,
            competing.body_bytes,
            competing.tx_count,
        );
        competing.subject_hash = subject_hash_v1(&competing);
        let signer_id = validator_id_v1(keys[0].verifying_key().as_bytes());
        assert!(node
            .store()
            .prepare_safety_locks_v1(&competing, signer_id)
            .is_err());
    }

    #[test]
    fn non_genesis_subject_requires_a_durable_parent_qc() {
        let chain_id = 81_006;
        let (node, first_block, keys, set) = genesis_fixture_v1("justify", chain_id);
        let (proposal, votes) = proposal_and_votes_v1(&node, &first_block, &keys, &set, 0, 3);
        let first_qc =
            NovNativeSealQuorumCertificateV1::from_votes(proposal.subject.clone(), &set, votes)
                .expect("build parent QC");
        node.store()
            .persist_local_verified_qc(node.ledger(), &first_qc, &set)
            .expect("persist parent QC");
        let second_block = commit_block_v1(node.ledger(), chain_id, 2, Some(&first_block), 0x41);
        assert!(node
            .store()
            .prepare_local_subject(
                node.ledger(),
                chain_id,
                second_block.header.block_hash,
                &set,
                0,
                None,
            )
            .is_err());
        let second = node
            .store()
            .prepare_local_subject(
                node.ledger(),
                chain_id,
                second_block.header.block_hash,
                &set,
                0,
                Some(first_qc.qc_hash),
            )
            .expect("prepare justified child subject");
        assert_eq!(second.parent_block_hash, first_block.header.block_hash);
        assert_eq!(second.justify_qc_hash, first_qc.qc_hash);
    }

    #[test]
    fn observed_candidate_cannot_become_a_local_signing_subject() {
        let chain_id = 81_007;
        let producer = TestNodeV1::new("observed-producer");
        bind_ownership_v1(producer.ledger(), chain_id);
        let block = commit_block_v1(producer.ledger(), chain_id, 1, None, 0x61);

        let observer = TestNodeV1::new("observed-receiver");
        bind_ownership_v1(observer.ledger(), chain_id);
        let observed = observer
            .ledger()
            .register_observed_unsealed_candidate(block.clone())
            .expect("register observed candidate");
        assert!(!observed.local_aoem_readback_verified);
        let (_keys, set) = validator_fixture_v1(chain_id);
        assert!(observer
            .store()
            .prepare_local_subject(
                observer.ledger(),
                chain_id,
                block.header.block_hash,
                &set,
                0,
                None,
            )
            .is_err());
    }

    #[test]
    fn subject_requires_aoem_ownership_and_store_rejects_another_ledger() {
        let chain_id = 81_008;
        let first = TestNodeV1::new("ledger-binding-first");
        let first_block = commit_block_v1(first.ledger(), chain_id, 1, None, 0x71);
        let (keys, set) = validator_fixture_v1(chain_id);
        assert!(first
            .store()
            .prepare_local_subject(
                first.ledger(),
                chain_id,
                first_block.header.block_hash,
                &set,
                0,
                None,
            )
            .is_err());
        bind_ownership_v1(first.ledger(), chain_id);
        let request = NovNativeSealLocalProposalRequestV1 {
            chain_id,
            block_hash: first_block.header.block_hash,
            round: 0,
            justify_qc_hash: None,
        };
        let first_proposal = first
            .store()
            .sign_local_proposal(first.ledger(), &request, &set, &keys[0])
            .expect("bind seal store to first ledger");

        let restored = TestNodeV1::new("ledger-binding-restored-copy");
        bind_ownership_v1(restored.ledger(), chain_id);
        let restored_block = commit_block_v1(restored.ledger(), chain_id, 1, None, 0x71);
        assert_eq!(
            restored_block.header.block_hash,
            first_block.header.block_hash
        );
        let restored_proposal = first
            .store()
            .sign_local_proposal(restored.ledger(), &request, &set, &keys[0])
            .expect("accept the same logical ledger restored at another path");
        assert_eq!(restored_proposal, first_proposal);

        let second = TestNodeV1::new("ledger-binding-second");
        bind_ownership_v1(second.ledger(), chain_id);
        let second_block = commit_block_v1(second.ledger(), chain_id, 1, None, 0x72);
        assert_ne!(
            second_block.header.block_hash,
            first_block.header.block_hash
        );
        let second_request = NovNativeSealLocalProposalRequestV1 {
            chain_id,
            block_hash: second_block.header.block_hash,
            round: 0,
            justify_qc_hash: None,
        };
        assert!(first
            .store()
            .sign_local_proposal(second.ledger(), &second_request, &set, &keys[0])
            .is_err());
    }

    #[test]
    fn read_only_open_does_not_create_a_missing_store() {
        let root = std::env::temp_dir().join(format!(
            "novovm-native-seal-missing-{}-{}",
            std::process::id(),
            TEST_SERIAL_V1.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(
            NovNativeBlockSealStoreV1::open_existing_read_only(root.as_path())
                .expect("read-only missing store")
                .is_none()
        );
        assert!(!root.exists());
    }

    #[test]
    fn validator_epoch_is_first_write_wins_and_read_only_store_cannot_mutate() {
        let chain_id = 81_010;
        let mut node = TestNodeV1::new("validator-epoch");
        let (keys, set) = validator_fixture_v1(chain_id);
        assert!(node
            .store()
            .register_validator_set(&set)
            .expect("register validator epoch"));
        assert!(!node
            .store()
            .register_validator_set(&set)
            .expect("repeat validator epoch"));
        let conflicting = NovNativeSealValidatorSetV1::new(
            chain_id,
            set.epoch,
            set.activation_height,
            keys.iter()
                .enumerate()
                .map(|(index, key)| {
                    NovNativeSealValidatorV1::new(
                        *key.verifying_key().as_bytes(),
                        if index == 0 { 2 } else { 1 },
                    )
                    .expect("conflicting validator")
                })
                .collect(),
        )
        .expect("conflicting validator set");
        assert!(node.store().register_validator_set(&conflicting).is_err());

        let seal_path = node.root.join("seal");
        node.store.take();
        let read_only = NovNativeBlockSealStoreV1::open_existing_read_only(seal_path.as_path())
            .expect("open existing seal store read-only")
            .expect("seal store exists");
        assert_eq!(
            read_only
                .load_validator_set(chain_id, set.epoch)
                .expect("read validator set"),
            Some(set.clone())
        );
        assert!(read_only.register_validator_set(&set).is_err());

        let activation_set = NovNativeSealValidatorSetV1::new(
            chain_id,
            2,
            2,
            keys.iter()
                .map(|key| {
                    NovNativeSealValidatorV1::new(*key.verifying_key().as_bytes(), 1)
                        .expect("activation validator")
                })
                .collect(),
        )
        .expect("future activation set");
        bind_ownership_v1(node.ledger(), chain_id);
        let block = commit_block_v1(node.ledger(), chain_id, 1, None, 0x51);
        assert!(read_only
            .prepare_local_subject(
                node.ledger(),
                chain_id,
                block.header.block_hash,
                &activation_set,
                0,
                None,
            )
            .is_err());
    }
}
