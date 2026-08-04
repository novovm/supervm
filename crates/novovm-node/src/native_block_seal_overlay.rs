//! Authenticated, bounded Product Overlay transport contract for NOV native
//! proof-seal artifacts.
//!
//! Remote artifacts are kept in an isolated quarantine database. They do not
//! acquire local signing locks, enter the local outbox, or mutate candidate
//! finality. A proposal or QC may cross into the local seal store only after
//! the local AOEM-owned ledger independently reconstructs the exact subject.

use anyhow::{bail, Context, Result};
use rocksdb::{Options as RocksDbOptions, WriteBatch as RocksDbWriteBatch, WriteOptions, DB};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{
    native_block_ledger::NovNativeBlockLedgerV1,
    native_block_seal::{
        NovNativeBlockSealStoreV1, NovNativeSealProposalV1, NovNativeSealQuorumCertificateV1,
        NovNativeSealSubjectV1, NovNativeSealValidatorSetV1, NovNativeSealVoteV1,
        NOV_NATIVE_BLOCK_SEAL_SIGNATURE_SCHEME_V1, NOV_NATIVE_BLOCK_SEAL_VOTE_SCHEMA_V1,
    },
};

pub const NOV_NATIVE_SEAL_OVERLAY_STORE_SCHEMA_V1: &str =
    "novovm-native-seal-overlay-quarantine/v1";
pub const NOV_NATIVE_SEAL_EPOCH_AUTHORITY_SCHEMA_V1: &str = "novovm-native-seal-epoch-authority/v1";
pub const NOV_NATIVE_SEAL_OVERLAY_ADMISSION_SCHEMA_V1: &str =
    "novovm-native-seal-overlay-admission/v1";
pub const NOV_NATIVE_SEAL_OVERLAY_EQUIVOCATION_SCHEMA_V1: &str =
    "novovm-native-seal-overlay-equivocation/v1";
pub const NOV_NATIVE_SEAL_OVERLAY_IDENTITY_GUARD_SCHEMA_V1: &str =
    "novovm-native-seal-overlay-identity-guard/v1";
pub const NOV_NATIVE_SEAL_OVERLAY_AUTHORITY_KIND_V1: &str = "operator-pinned-genesis-epoch/v1";
pub const NOV_NATIVE_SEAL_OVERLAY_LEADER_SCHEDULE_V1: &str = "round-robin-validator-id/v1";
pub const NOV_NATIVE_SEAL_OVERLAY_MAX_WIRE_BYTES_V1: usize = 192 * 1024;
pub const NOV_NATIVE_SEAL_OVERLAY_MAX_DIRECT_VALIDATORS_V1: usize = 64;
pub const NOV_NATIVE_SEAL_OVERLAY_MAX_SLOT_OBJECTS_V1: usize = 4;
pub const NOV_NATIVE_SEAL_OVERLAY_MAX_HEIGHT_BEHIND_V1: u64 = 128;
pub const NOV_NATIVE_SEAL_OVERLAY_MAX_HEIGHT_AHEAD_V1: u64 = 2;
/// Slice 2B1 has no durable pacemaker or timeout certificate. Accepting a
/// future round would let an otherwise valid leader pre-lock a height, so the
/// network ingress is deliberately restricted to round zero.
pub const NOV_NATIVE_SEAL_OVERLAY_MAX_ROUND_V1: u64 = 0;

const WIRE_MAGIC_V1: &[u8; 8] = b"NOVSLW01";
const WIRE_VERSION_V1: u16 = 1;
const WIRE_HEADER_BYTES_V1: usize = 8 + 2 + 1 + 1 + 8 + 8 + 32 + 32 + 4;
const WIRE_CHECKSUM_BYTES_V1: usize = 32;
const KEY_SCHEMA_V1: &[u8] = b"native_seal_overlay/v1/schema";
const KEY_PREFIX_V1: &str = "native_seal_overlay/v1/";
const AUTHORITY_COMMITMENT_DOMAIN_V1: &[u8] = b"novovm-native-seal-epoch-authority-v1\0";
const WIRE_CHECKSUM_DOMAIN_V1: &[u8] = b"novovm-native-seal-overlay-wire-v1\0";
const SLOT_BINDING_DOMAIN_V1: &[u8] = b"novovm-native-seal-overlay-slot-v1\0";
const EQUIVOCATION_DOMAIN_V1: &[u8] = b"novovm-native-seal-overlay-equivocation-v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealValidatorTransportBindingV1 {
    pub validator_id: [u8; 32],
    pub transport_peer_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealEpochAuthorityV1 {
    pub schema: String,
    pub authority_kind: String,
    pub chain_id: u64,
    pub genesis_block_hash: [u8; 32],
    pub protocol_config_commitment: [u8; 32],
    pub epoch: u64,
    pub activation_height: u64,
    pub validator_set: NovNativeSealValidatorSetV1,
    pub transport_bindings: Vec<NovNativeSealValidatorTransportBindingV1>,
    pub leader_schedule: String,
    pub authority_commitment: [u8; 32],
}

impl NovNativeSealEpochAuthorityV1 {
    pub fn derive_operator_pinned_genesis_epoch(
        ledger: &NovNativeBlockLedgerV1,
        validator_set: NovNativeSealValidatorSetV1,
        mut transport_bindings: Vec<NovNativeSealValidatorTransportBindingV1>,
    ) -> Result<Self> {
        validator_set.validate()?;
        if validator_set.epoch != 1 || validator_set.activation_height != 1 {
            bail!("NOV native seal overlay v1 accepts only epoch 1 activated at genesis");
        }
        let ownership = ledger
            .load_aoem_ownership()?
            .context("NOV native seal overlay authority requires AOEM ownership metadata")?;
        if ownership.chain_id != validator_set.chain_id {
            bail!("NOV native seal overlay authority chain does not match AOEM ownership");
        }
        let genesis_block_hash = ledger
            .load_by_height(validator_set.chain_id, 1)?
            .context("NOV native seal overlay authority requires a durable genesis block")?
            .header
            .block_hash;
        let protocol_config_commitment = decode_hex_32_v1(
            "AOEM protocol config commitment",
            ownership.protocol_config_commitment.as_str(),
        )?;
        transport_bindings.sort_by_key(|binding| binding.validator_id);
        let mut authority = Self {
            schema: NOV_NATIVE_SEAL_EPOCH_AUTHORITY_SCHEMA_V1.to_string(),
            authority_kind: NOV_NATIVE_SEAL_OVERLAY_AUTHORITY_KIND_V1.to_string(),
            chain_id: validator_set.chain_id,
            genesis_block_hash,
            protocol_config_commitment,
            epoch: validator_set.epoch,
            activation_height: validator_set.activation_height,
            validator_set,
            transport_bindings,
            leader_schedule: NOV_NATIVE_SEAL_OVERLAY_LEADER_SCHEDULE_V1.to_string(),
            authority_commitment: [0u8; 32],
        };
        authority.authority_commitment = authority_commitment_v1(&authority);
        authority.validate_against_ledger(ledger)?;
        Ok(authority)
    }

    pub fn validate(&self) -> Result<()> {
        self.validator_set.validate()?;
        if self.schema != NOV_NATIVE_SEAL_EPOCH_AUTHORITY_SCHEMA_V1
            || self.authority_kind != NOV_NATIVE_SEAL_OVERLAY_AUTHORITY_KIND_V1
            || self.chain_id == 0
            || self.genesis_block_hash == [0u8; 32]
            || self.protocol_config_commitment == [0u8; 32]
            || self.epoch != 1
            || self.activation_height != 1
            || self.validator_set.chain_id != self.chain_id
            || self.validator_set.epoch != self.epoch
            || self.validator_set.activation_height != self.activation_height
            || self.validator_set.validators.len()
                > NOV_NATIVE_SEAL_OVERLAY_MAX_DIRECT_VALIDATORS_V1
            || self.transport_bindings.len() != self.validator_set.validators.len()
            || self.leader_schedule != NOV_NATIVE_SEAL_OVERLAY_LEADER_SCHEDULE_V1
        {
            bail!("NOV native seal epoch authority metadata is invalid");
        }
        let mut previous_validator = None;
        let mut peer_ids = BTreeSet::new();
        for binding in &self.transport_bindings {
            if previous_validator.is_some_and(|previous| previous >= binding.validator_id)
                || self.validator_set.validator(binding.validator_id).is_none()
                || !valid_peer_id_v1(binding.transport_peer_id.as_str())
                || !peer_ids.insert(binding.transport_peer_id.as_str())
            {
                bail!("NOV native seal authority transport bindings are invalid");
            }
            previous_validator = Some(binding.validator_id);
        }
        if self.authority_commitment != authority_commitment_v1(self) {
            bail!("NOV native seal epoch authority commitment is invalid");
        }
        Ok(())
    }

    pub fn validate_against_ledger(&self, ledger: &NovNativeBlockLedgerV1) -> Result<()> {
        self.validate()?;
        let ownership = ledger
            .load_aoem_ownership()?
            .context("NOV native seal authority ledger is missing AOEM ownership")?;
        let protocol_config_commitment = decode_hex_32_v1(
            "AOEM protocol config commitment",
            ownership.protocol_config_commitment.as_str(),
        )?;
        let genesis = ledger
            .load_by_height(self.chain_id, 1)?
            .context("NOV native seal authority ledger is missing genesis")?;
        if ownership.chain_id != self.chain_id
            || genesis.header.block_hash != self.genesis_block_hash
            || protocol_config_commitment != self.protocol_config_commitment
        {
            bail!("NOV native seal authority does not bind the local ledger identity");
        }
        Ok(())
    }

    pub fn expected_leader(&self, height: u64, round: u64) -> Result<[u8; 32]> {
        self.validate()?;
        if height < self.activation_height || round > NOV_NATIVE_SEAL_OVERLAY_MAX_ROUND_V1 {
            bail!("NOV native seal leader request is outside the authority domain");
        }
        let offset = height
            .checked_sub(self.activation_height)
            .and_then(|value| value.checked_add(round))
            .context("NOV native seal leader schedule overflow")?;
        let index = usize::try_from(offset % self.validator_set.validators.len() as u64)
            .context("NOV native seal leader index conversion failed")?;
        Ok(self.validator_set.validators[index].validator_id)
    }

    pub fn transport_peer_id(&self, validator_id: [u8; 32]) -> Result<&str> {
        self.transport_bindings
            .binary_search_by_key(&validator_id, |binding| binding.validator_id)
            .ok()
            .and_then(|index| self.transport_bindings.get(index))
            .map(|binding| binding.transport_peer_id.as_str())
            .context("NOV native seal validator has no authority-bound transport identity")
    }

    pub fn validator_for_transport_peer(&self, peer_id: &str) -> Option<[u8; 32]> {
        self.transport_bindings
            .iter()
            .find(|binding| binding.transport_peer_id == peer_id)
            .map(|binding| binding.validator_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NovNativeSealOverlayArtifactKindV1 {
    Proposal,
    Vote,
    QuorumCertificate,
}

impl NovNativeSealOverlayArtifactKindV1 {
    const fn code(self) -> u8 {
        match self {
            Self::Proposal => 1,
            Self::Vote => 2,
            Self::QuorumCertificate => 3,
        }
    }

    fn from_code(code: u8) -> Result<Self> {
        match code {
            1 => Ok(Self::Proposal),
            2 => Ok(Self::Vote),
            3 => Ok(Self::QuorumCertificate),
            _ => bail!("NOV native seal overlay wire has an unknown artifact kind"),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Proposal => "proposal",
            Self::Vote => "vote",
            Self::QuorumCertificate => "qc",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NovNativeSealOverlayArtifactV1 {
    Proposal(Box<NovNativeSealProposalV1>),
    Vote {
        proposal: Box<NovNativeSealProposalV1>,
        vote: Box<NovNativeSealVoteV1>,
    },
    QuorumCertificate {
        proposal: Box<NovNativeSealProposalV1>,
        qc: Box<NovNativeSealQuorumCertificateV1>,
    },
}

impl NovNativeSealOverlayArtifactV1 {
    #[must_use]
    pub const fn kind(&self) -> NovNativeSealOverlayArtifactKindV1 {
        match self {
            Self::Proposal(_) => NovNativeSealOverlayArtifactKindV1::Proposal,
            Self::Vote { .. } => NovNativeSealOverlayArtifactKindV1::Vote,
            Self::QuorumCertificate { .. } => NovNativeSealOverlayArtifactKindV1::QuorumCertificate,
        }
    }

    #[must_use]
    pub const fn object_hash(&self) -> [u8; 32] {
        match self {
            Self::Proposal(proposal) => proposal.proposal_hash,
            Self::Vote { vote, .. } => vote.vote_hash,
            Self::QuorumCertificate { qc, .. } => qc.qc_hash,
        }
    }

    #[must_use]
    pub fn proposal(&self) -> &NovNativeSealProposalV1 {
        match self {
            Self::Proposal(proposal)
            | Self::Vote { proposal, .. }
            | Self::QuorumCertificate { proposal, .. } => proposal.as_ref(),
        }
    }

    fn subject(&self) -> &NovNativeSealSubjectV1 {
        &self.proposal().subject
    }

    fn signer_id(&self) -> Option<[u8; 32]> {
        match self {
            Self::Proposal(proposal) => Some(proposal.proposer_id),
            Self::Vote { vote, .. } => Some(vote.validator_id),
            Self::QuorumCertificate { .. } => None,
        }
    }

    pub fn validate(&self, authority: &NovNativeSealEpochAuthorityV1) -> Result<()> {
        authority.validate()?;
        let proposal = self.proposal();
        proposal.verify(&authority.validator_set)?;
        validate_subject_authority_v1(&proposal.subject, authority)?;
        if proposal.proposer_id
            != authority.expected_leader(proposal.subject.height, proposal.subject.round)?
        {
            bail!("NOV native seal proposal signer is not the authority-selected leader");
        }
        match self {
            Self::Proposal(_) => {}
            Self::Vote { vote, .. } => vote.verify(
                &proposal.subject,
                proposal.proposal_hash,
                &authority.validator_set,
            )?,
            Self::QuorumCertificate { qc, .. } => {
                qc.verify(&authority.validator_set)?;
                if qc.subject != proposal.subject || qc.proposal_hash != proposal.proposal_hash {
                    bail!("NOV native seal QC wire does not bind its embedded proposal");
                }
            }
        }
        Ok(())
    }

    pub fn validate_authenticated_source(
        &self,
        authority: &NovNativeSealEpochAuthorityV1,
        source_peer_id: &str,
    ) -> Result<()> {
        self.validate(authority)?;
        let source_validator = authority
            .validator_for_transport_peer(source_peer_id)
            .context("NOV native seal artifact came from an unbound transport peer")?;
        if self
            .signer_id()
            .is_some_and(|signer_id| signer_id != source_validator)
        {
            bail!(
                "NOV native seal artifact signer does not match the authenticated transport peer"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NovNativeSealVoteBundleWireV1 {
    proposal: NovNativeSealProposalV1,
    vote: NovNativeSealVoteV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NovNativeSealCompactVoteWireV1 {
    validator_id: [u8; 32],
    signature: Vec<u8>,
    vote_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NovNativeSealCompactQcWireV1 {
    schema: String,
    subject: NovNativeSealSubjectV1,
    subject_hash: [u8; 32],
    proposal_hash: [u8; 32],
    validator_set_hash: [u8; 32],
    votes: Vec<NovNativeSealCompactVoteWireV1>,
    signature_count: u32,
    signed_weight: u64,
    quorum_weight: u64,
    threshold_satisfied: bool,
    qc_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NovNativeSealQcBundleWireV1 {
    proposal: NovNativeSealProposalV1,
    qc: NovNativeSealCompactQcWireV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NovNativeSealOverlayDecodedWireV1 {
    pub artifact: NovNativeSealOverlayArtifactV1,
    pub authority_commitment: [u8; 32],
    pub wire_hash: [u8; 32],
}

pub fn is_nov_native_seal_overlay_wire_v1(bytes: &[u8]) -> bool {
    bytes.starts_with(WIRE_MAGIC_V1)
}

pub fn encode_nov_native_seal_overlay_wire_v1(
    artifact: &NovNativeSealOverlayArtifactV1,
    authority: &NovNativeSealEpochAuthorityV1,
) -> Result<Vec<u8>> {
    artifact.validate(authority)?;
    let payload = match artifact {
        NovNativeSealOverlayArtifactV1::Proposal(proposal) => postcard::to_allocvec(proposal),
        NovNativeSealOverlayArtifactV1::Vote { proposal, vote } => {
            postcard::to_allocvec(&NovNativeSealVoteBundleWireV1 {
                proposal: proposal.as_ref().clone(),
                vote: vote.as_ref().clone(),
            })
        }
        NovNativeSealOverlayArtifactV1::QuorumCertificate { proposal, qc } => {
            postcard::to_allocvec(&NovNativeSealQcBundleWireV1 {
                proposal: proposal.as_ref().clone(),
                qc: compact_qc_v1(qc),
            })
        }
    }
    .context("encode canonical NOV native seal overlay artifact")?;
    if payload.is_empty()
        || payload.len() > NOV_NATIVE_SEAL_OVERLAY_MAX_WIRE_BYTES_V1
        || payload.len() > u32::MAX as usize
    {
        bail!("NOV native seal overlay artifact exceeds its canonical wire bound");
    }
    let subject = artifact.subject();
    let mut wire =
        Vec::with_capacity(WIRE_HEADER_BYTES_V1 + payload.len() + WIRE_CHECKSUM_BYTES_V1);
    wire.extend_from_slice(WIRE_MAGIC_V1);
    wire.extend_from_slice(&WIRE_VERSION_V1.to_be_bytes());
    wire.push(artifact.kind().code());
    wire.push(0);
    wire.extend_from_slice(&subject.chain_id.to_be_bytes());
    wire.extend_from_slice(&subject.epoch.to_be_bytes());
    wire.extend_from_slice(&authority.authority_commitment);
    wire.extend_from_slice(&artifact.object_hash());
    wire.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    wire.extend_from_slice(payload.as_slice());
    let checksum = wire_checksum_v1(wire.as_slice());
    wire.extend_from_slice(&checksum);
    if wire.len() > NOV_NATIVE_SEAL_OVERLAY_MAX_WIRE_BYTES_V1 {
        bail!("NOV native seal overlay wire exceeds its transport bound");
    }
    Ok(wire)
}

pub fn decode_nov_native_seal_overlay_wire_v1(
    bytes: &[u8],
    authority: &NovNativeSealEpochAuthorityV1,
) -> Result<NovNativeSealOverlayDecodedWireV1> {
    authority.validate()?;
    if bytes.len() < WIRE_HEADER_BYTES_V1 + WIRE_CHECKSUM_BYTES_V1
        || bytes.len() > NOV_NATIVE_SEAL_OVERLAY_MAX_WIRE_BYTES_V1
    {
        bail!("NOV native seal overlay wire length is invalid");
    }
    if &bytes[..8] != WIRE_MAGIC_V1 {
        bail!("NOV native seal overlay wire magic is invalid");
    }
    let version = u16::from_be_bytes([bytes[8], bytes[9]]);
    if version != WIRE_VERSION_V1 || bytes[11] != 0 {
        bail!("NOV native seal overlay wire version or reserved flags are invalid");
    }
    let kind = NovNativeSealOverlayArtifactKindV1::from_code(bytes[10])?;
    let chain_id = read_u64_be_v1(bytes, 12)?;
    let epoch = read_u64_be_v1(bytes, 20)?;
    let mut authority_commitment = [0u8; 32];
    authority_commitment.copy_from_slice(&bytes[28..60]);
    let mut object_hash = [0u8; 32];
    object_hash.copy_from_slice(&bytes[60..92]);
    let payload_len = u32::from_be_bytes(bytes[92..96].try_into().unwrap_or_default()) as usize;
    let payload_end = WIRE_HEADER_BYTES_V1
        .checked_add(payload_len)
        .context("NOV native seal overlay payload length overflow")?;
    let expected_len = payload_end
        .checked_add(WIRE_CHECKSUM_BYTES_V1)
        .context("NOV native seal overlay wire length overflow")?;
    if payload_len == 0 || expected_len != bytes.len() {
        bail!("NOV native seal overlay wire payload length mismatch");
    }
    let mut checksum = [0u8; 32];
    checksum.copy_from_slice(&bytes[payload_end..]);
    let computed = wire_checksum_v1(&bytes[..payload_end]);
    if checksum != computed {
        bail!("NOV native seal overlay wire checksum mismatch");
    }
    if chain_id != authority.chain_id
        || epoch != authority.epoch
        || authority_commitment != authority.authority_commitment
        || object_hash == [0u8; 32]
    {
        bail!("NOV native seal overlay wire is outside the pinned authority domain");
    }
    let payload = &bytes[WIRE_HEADER_BYTES_V1..payload_end];
    let artifact = match kind {
        NovNativeSealOverlayArtifactKindV1::Proposal => {
            let proposal: NovNativeSealProposalV1 = canonical_postcard_decode_v1(payload)?;
            NovNativeSealOverlayArtifactV1::Proposal(Box::new(proposal))
        }
        NovNativeSealOverlayArtifactKindV1::Vote => {
            let bundle: NovNativeSealVoteBundleWireV1 = canonical_postcard_decode_v1(payload)?;
            NovNativeSealOverlayArtifactV1::Vote {
                proposal: Box::new(bundle.proposal),
                vote: Box::new(bundle.vote),
            }
        }
        NovNativeSealOverlayArtifactKindV1::QuorumCertificate => {
            let bundle: NovNativeSealQcBundleWireV1 = canonical_postcard_decode_v1(payload)?;
            let qc = expand_qc_v1(bundle.qc)?;
            NovNativeSealOverlayArtifactV1::QuorumCertificate {
                proposal: Box::new(bundle.proposal),
                qc: Box::new(qc),
            }
        }
    };
    artifact.validate(authority)?;
    if artifact.kind() != kind
        || artifact.object_hash() != object_hash
        || artifact.subject().chain_id != chain_id
        || artifact.subject().epoch != epoch
    {
        bail!("NOV native seal overlay wire header/object reverse binding mismatch");
    }
    Ok(NovNativeSealOverlayDecodedWireV1 {
        artifact,
        authority_commitment,
        wire_hash: checksum,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NovNativeSealOverlayAdmissionStateV1 {
    CryptoVerifiedAwaitingLocalExecution,
    QcCryptoVerifiedQuarantined,
    EquivocationQuarantined,
    LocallyMatchedVoteEligible,
    QcLocallyVerifiedDurable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealOverlayAdmissionV1 {
    pub schema: String,
    pub artifact_kind: NovNativeSealOverlayArtifactKindV1,
    pub chain_id: u64,
    pub epoch: u64,
    pub height: u64,
    pub round: u64,
    pub object_hash: [u8; 32],
    pub proposal_hash: [u8; 32],
    pub first_source_peer_id: String,
    pub last_source_peer_id: String,
    pub wire_hash: [u8; 32],
    pub state: NovNativeSealOverlayAdmissionStateV1,
    pub first_received_at_unix_ms: u64,
    pub last_received_at_unix_ms: u64,
    pub receive_count: u64,
    pub evidence_hashes: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealOverlayEquivocationEvidenceV1 {
    pub schema: String,
    pub artifact_kind: NovNativeSealOverlayArtifactKindV1,
    pub chain_id: u64,
    pub epoch: u64,
    pub height: u64,
    pub round: u64,
    pub slot_binding: [u8; 32],
    pub left_object_hash: [u8; 32],
    pub right_object_hash: [u8; 32],
    pub evidence_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealOverlayIdentityGuardV1 {
    pub schema: String,
    pub chain_id: u64,
    pub epoch: u64,
    pub validator_id: [u8; 32],
    pub signing_blocked: bool,
    pub evidence_hashes: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NovNativeSealOverlaySlotIndexV1 {
    schema: String,
    artifact_kind: NovNativeSealOverlayArtifactKindV1,
    slot_binding: [u8; 32],
    object_hashes: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NovNativeSealOverlayIngressContextV1 {
    pub local_execution_height: u64,
    pub local_validator_id: Option<[u8; 32]>,
    pub received_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NovNativeSealOverlayIngressResultV1 {
    pub artifact: NovNativeSealOverlayArtifactV1,
    pub admission: NovNativeSealOverlayAdmissionV1,
    pub duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NovNativeSealOverlayReconcileResultV1 {
    pub proposal_hash: [u8; 32],
    pub qc_hash: Option<[u8; 32]>,
    pub newly_persisted: bool,
    pub state: NovNativeSealOverlayAdmissionStateV1,
}

pub struct NovNativeSealOverlayQuarantineV1 {
    path: PathBuf,
    db: Arc<DB>,
    write_lock: Mutex<()>,
}

impl NovNativeSealOverlayQuarantineV1 {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "create NOV native seal overlay quarantine parent: {}",
                        parent.display()
                    )
                })?;
            }
        }
        let mut options = RocksDbOptions::default();
        options.create_if_missing(true);
        let db = Arc::new(DB::open(&options, path).with_context(|| {
            format!(
                "open NOV native seal overlay quarantine: {}",
                path.display()
            )
        })?);
        match db
            .get(KEY_SCHEMA_V1)
            .context("read NOV native seal overlay quarantine schema")?
        {
            Some(raw) if raw.as_slice() != NOV_NATIVE_SEAL_OVERLAY_STORE_SCHEMA_V1.as_bytes() => {
                bail!(
                    "unsupported NOV native seal overlay quarantine schema: {}",
                    String::from_utf8_lossy(raw.as_slice())
                );
            }
            Some(_) => {}
            None => {
                let mut batch = RocksDbWriteBatch::default();
                batch.put(
                    KEY_SCHEMA_V1,
                    NOV_NATIVE_SEAL_OVERLAY_STORE_SCHEMA_V1.as_bytes(),
                );
                write_sync_v1(&db, batch)
                    .context("initialize NOV native seal overlay quarantine schema")?;
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
            db,
            write_lock: Mutex::new(()),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn bind_epoch_authority(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        authority: &NovNativeSealEpochAuthorityV1,
        expected_authority_commitment: [u8; 32],
    ) -> Result<bool> {
        authority.validate_against_ledger(ledger)?;
        if authority.authority_commitment != expected_authority_commitment {
            bail!("NOV native seal authority does not match the operator-pinned commitment");
        }
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let key = authority_key_v1(authority.chain_id, authority.epoch);
        if let Some(existing) = read_json_v1::<NovNativeSealEpochAuthorityV1>(
            &self.db,
            key.as_bytes(),
            "epoch authority",
        )? {
            existing.validate_against_ledger(ledger)?;
            if existing != *authority {
                bail!("NOV native seal epoch authority is already pinned to a different manifest");
            }
            return Ok(false);
        }
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(&mut batch, key.as_bytes(), authority, "epoch authority")?;
        write_sync_v1(&self.db, batch).context("persist NOV native seal epoch authority")?;
        if self
            .load_epoch_authority(authority.chain_id, authority.epoch)?
            .as_ref()
            != Some(authority)
        {
            bail!("NOV native seal epoch authority readback mismatch");
        }
        Ok(true)
    }

    pub fn load_epoch_authority(
        &self,
        chain_id: u64,
        epoch: u64,
    ) -> Result<Option<NovNativeSealEpochAuthorityV1>> {
        let authority = read_json_v1::<NovNativeSealEpochAuthorityV1>(
            &self.db,
            authority_key_v1(chain_id, epoch).as_bytes(),
            "epoch authority",
        )?;
        if let Some(authority) = authority.as_ref() {
            authority.validate()?;
            if authority.chain_id != chain_id || authority.epoch != epoch {
                bail!("NOV native seal epoch authority key binding mismatch");
            }
        }
        Ok(authority)
    }

    pub fn ingest_authenticated_wire(
        &self,
        authority: &NovNativeSealEpochAuthorityV1,
        source_peer_id: &str,
        wire: &[u8],
        context: &NovNativeSealOverlayIngressContextV1,
    ) -> Result<NovNativeSealOverlayIngressResultV1> {
        self.ensure_bound_authority_v1(authority)?;
        validate_ingress_context_v1(context, authority)?;
        let decoded = decode_nov_native_seal_overlay_wire_v1(wire, authority)?;
        decoded
            .artifact
            .validate_authenticated_source(authority, source_peer_id)?;
        validate_ingress_window_v1(decoded.artifact.subject(), authority, context)?;
        let kind = decoded.artifact.kind();
        let object_hash = decoded.artifact.object_hash();
        let admission_key = admission_key_v1(kind, &object_hash);

        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.ensure_bound_authority_v1(authority)?;
        if let Some(mut admission) = read_json_v1::<NovNativeSealOverlayAdmissionV1>(
            &self.db,
            admission_key.as_bytes(),
            "overlay admission",
        )? {
            validate_admission_v1(&admission)?;
            let stored = self
                .load_artifact(kind, object_hash, authority)?
                .context("NOV native seal replay admission points to a missing artifact")?;
            if stored != decoded.artifact
                || admission.wire_hash != decoded.wire_hash
                || admission.object_hash != object_hash
            {
                bail!("NOV native seal replay conflicts with a durable admission");
            }
            admission.receive_count = admission
                .receive_count
                .checked_add(1)
                .context("NOV native seal replay count overflow")?;
            admission.last_received_at_unix_ms = admission
                .last_received_at_unix_ms
                .max(context.received_at_unix_ms);
            admission.last_source_peer_id = source_peer_id.to_string();
            validate_admission_v1(&admission)?;
            let mut batch = RocksDbWriteBatch::default();
            put_json_v1(
                &mut batch,
                admission_key.as_bytes(),
                &admission,
                "overlay replay admission",
            )?;
            write_sync_v1(&self.db, batch)
                .context("persist NOV native seal overlay replay admission")?;
            return Ok(NovNativeSealOverlayIngressResultV1 {
                artifact: decoded.artifact,
                admission,
                duplicate: true,
            });
        }

        let mut batch = RocksDbWriteBatch::default();
        let mut evidence = self.stage_artifact_v1(
            &mut batch,
            authority,
            &decoded.artifact,
            context.local_validator_id,
        )?;
        evidence.sort();
        evidence.dedup();
        if evidence.len() > NOV_NATIVE_SEAL_OVERLAY_MAX_SLOT_OBJECTS_V1 {
            bail!("NOV native seal admission produced too many equivocation records");
        }
        let subject = decoded.artifact.subject();
        let state = if evidence.is_empty() {
            if kind == NovNativeSealOverlayArtifactKindV1::QuorumCertificate {
                NovNativeSealOverlayAdmissionStateV1::QcCryptoVerifiedQuarantined
            } else {
                NovNativeSealOverlayAdmissionStateV1::CryptoVerifiedAwaitingLocalExecution
            }
        } else {
            NovNativeSealOverlayAdmissionStateV1::EquivocationQuarantined
        };
        let admission = NovNativeSealOverlayAdmissionV1 {
            schema: NOV_NATIVE_SEAL_OVERLAY_ADMISSION_SCHEMA_V1.to_string(),
            artifact_kind: kind,
            chain_id: subject.chain_id,
            epoch: subject.epoch,
            height: subject.height,
            round: subject.round,
            object_hash,
            proposal_hash: decoded.artifact.proposal().proposal_hash,
            first_source_peer_id: source_peer_id.to_string(),
            last_source_peer_id: source_peer_id.to_string(),
            wire_hash: decoded.wire_hash,
            state,
            first_received_at_unix_ms: context.received_at_unix_ms,
            last_received_at_unix_ms: context.received_at_unix_ms,
            receive_count: 1,
            evidence_hashes: evidence,
        };
        validate_admission_v1(&admission)?;
        put_json_v1(
            &mut batch,
            admission_key.as_bytes(),
            &admission,
            "overlay admission",
        )?;
        write_sync_v1(&self.db, batch)
            .context("persist NOV native seal overlay quarantine admission")?;
        let readback = self
            .load_admission(kind, object_hash)?
            .context("NOV native seal overlay admission readback is missing")?;
        if readback != admission {
            bail!("NOV native seal overlay admission readback mismatch");
        }
        let stored = self
            .load_artifact(kind, object_hash, authority)?
            .context("NOV native seal overlay artifact readback is missing")?;
        if stored != decoded.artifact {
            bail!("NOV native seal overlay artifact readback mismatch");
        }
        Ok(NovNativeSealOverlayIngressResultV1 {
            artifact: stored,
            admission: readback,
            duplicate: false,
        })
    }

    pub fn load_admission(
        &self,
        kind: NovNativeSealOverlayArtifactKindV1,
        object_hash: [u8; 32],
    ) -> Result<Option<NovNativeSealOverlayAdmissionV1>> {
        let admission = read_json_v1::<NovNativeSealOverlayAdmissionV1>(
            &self.db,
            admission_key_v1(kind, &object_hash).as_bytes(),
            "overlay admission",
        )?;
        if let Some(admission) = admission.as_ref() {
            validate_admission_v1(admission)?;
            if admission.artifact_kind != kind || admission.object_hash != object_hash {
                bail!("NOV native seal overlay admission key binding mismatch");
            }
        }
        Ok(admission)
    }

    pub fn load_artifact(
        &self,
        kind: NovNativeSealOverlayArtifactKindV1,
        object_hash: [u8; 32],
        authority: &NovNativeSealEpochAuthorityV1,
    ) -> Result<Option<NovNativeSealOverlayArtifactV1>> {
        self.ensure_bound_authority_v1(authority)?;
        let artifact = match kind {
            NovNativeSealOverlayArtifactKindV1::Proposal => self
                .load_proposal_object_v1(object_hash, authority)?
                .map(|proposal| NovNativeSealOverlayArtifactV1::Proposal(Box::new(proposal))),
            NovNativeSealOverlayArtifactKindV1::Vote => {
                let Some(vote) = read_json_v1::<NovNativeSealVoteV1>(
                    &self.db,
                    vote_object_key_v1(&object_hash).as_bytes(),
                    "remote vote object",
                )?
                else {
                    return Ok(None);
                };
                let proposal = self
                    .load_proposal_object_v1(vote.proposal_hash, authority)?
                    .context("remote vote is missing its verified proposal")?;
                vote.verify(
                    &proposal.subject,
                    proposal.proposal_hash,
                    &authority.validator_set,
                )?;
                Some(NovNativeSealOverlayArtifactV1::Vote {
                    proposal: Box::new(proposal),
                    vote: Box::new(vote),
                })
            }
            NovNativeSealOverlayArtifactKindV1::QuorumCertificate => {
                let Some(qc) = read_json_v1::<NovNativeSealQuorumCertificateV1>(
                    &self.db,
                    qc_object_key_v1(&object_hash).as_bytes(),
                    "remote QC object",
                )?
                else {
                    return Ok(None);
                };
                let proposal = self
                    .load_proposal_object_v1(qc.proposal_hash, authority)?
                    .context("remote QC is missing its verified proposal")?;
                qc.verify(&authority.validator_set)?;
                if qc.subject != proposal.subject || qc.proposal_hash != proposal.proposal_hash {
                    bail!("remote QC/proposal reverse binding mismatch");
                }
                Some(NovNativeSealOverlayArtifactV1::QuorumCertificate {
                    proposal: Box::new(proposal),
                    qc: Box::new(qc),
                })
            }
        };
        if let Some(artifact) = artifact.as_ref() {
            artifact.validate(authority)?;
            if artifact.kind() != kind || artifact.object_hash() != object_hash {
                bail!("NOV native seal overlay artifact key binding mismatch");
            }
        }
        Ok(artifact)
    }

    pub fn load_identity_guard(
        &self,
        chain_id: u64,
        epoch: u64,
        validator_id: [u8; 32],
    ) -> Result<Option<NovNativeSealOverlayIdentityGuardV1>> {
        let guard = read_json_v1::<NovNativeSealOverlayIdentityGuardV1>(
            &self.db,
            identity_guard_key_v1(chain_id, epoch, &validator_id).as_bytes(),
            "local identity guard",
        )?;
        if let Some(guard) = guard.as_ref() {
            validate_identity_guard_v1(guard)?;
            if guard.chain_id != chain_id
                || guard.epoch != epoch
                || guard.validator_id != validator_id
            {
                bail!("NOV native seal local identity guard key binding mismatch");
            }
        }
        Ok(guard)
    }

    pub fn load_equivocation_evidence(
        &self,
        evidence_hash: [u8; 32],
    ) -> Result<Option<NovNativeSealOverlayEquivocationEvidenceV1>> {
        let evidence = read_json_v1::<NovNativeSealOverlayEquivocationEvidenceV1>(
            &self.db,
            equivocation_object_key_v1(&evidence_hash).as_bytes(),
            "equivocation evidence",
        )?;
        if let Some(evidence) = evidence.as_ref() {
            validate_equivocation_v1(evidence)?;
            if evidence.evidence_hash != evidence_hash {
                bail!("NOV native seal equivocation evidence key binding mismatch");
            }
        }
        Ok(evidence)
    }

    pub fn reconcile_proposal_with_local_execution(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        seal_store: &NovNativeBlockSealStoreV1,
        authority: &NovNativeSealEpochAuthorityV1,
        proposal_hash: [u8; 32],
    ) -> Result<NovNativeSealOverlayReconcileResultV1> {
        self.ensure_bound_authority_v1(authority)?;
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let proposal = self
            .load_proposal_object_v1(proposal_hash, authority)?
            .context("NOV native seal quarantine proposal is missing")?;
        self.ensure_proposal_uncontested_v1(&proposal)?;
        let newly_persisted = seal_store.persist_locally_matched_remote_proposal(
            ledger,
            &proposal,
            &authority.validator_set,
        )?;
        self.update_admission_state_if_present_locked_v1(
            NovNativeSealOverlayArtifactKindV1::Proposal,
            proposal_hash,
            NovNativeSealOverlayAdmissionStateV1::LocallyMatchedVoteEligible,
        )?;
        Ok(NovNativeSealOverlayReconcileResultV1 {
            proposal_hash,
            qc_hash: None,
            newly_persisted,
            state: NovNativeSealOverlayAdmissionStateV1::LocallyMatchedVoteEligible,
        })
    }

    pub fn reconcile_qc_with_local_execution(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        seal_store: &NovNativeBlockSealStoreV1,
        authority: &NovNativeSealEpochAuthorityV1,
        qc_hash: [u8; 32],
    ) -> Result<NovNativeSealOverlayReconcileResultV1> {
        self.ensure_bound_authority_v1(authority)?;
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let artifact = self
            .load_artifact(
                NovNativeSealOverlayArtifactKindV1::QuorumCertificate,
                qc_hash,
                authority,
            )?
            .context("NOV native seal quarantine QC is missing")?;
        let NovNativeSealOverlayArtifactV1::QuorumCertificate { proposal, qc } = artifact else {
            unreachable!("QC key returned a non-QC artifact")
        };
        self.ensure_proposal_uncontested_v1(&proposal)?;
        self.ensure_qc_uncontested_v1(&qc)?;
        seal_store.persist_locally_matched_remote_proposal(
            ledger,
            &proposal,
            &authority.validator_set,
        )?;
        let newly_persisted =
            seal_store.persist_local_verified_qc(ledger, &qc, &authority.validator_set)?;
        self.update_admission_state_if_present_locked_v1(
            NovNativeSealOverlayArtifactKindV1::QuorumCertificate,
            qc_hash,
            NovNativeSealOverlayAdmissionStateV1::QcLocallyVerifiedDurable,
        )?;
        Ok(NovNativeSealOverlayReconcileResultV1 {
            proposal_hash: proposal.proposal_hash,
            qc_hash: Some(qc_hash),
            newly_persisted,
            state: NovNativeSealOverlayAdmissionStateV1::QcLocallyVerifiedDurable,
        })
    }

    fn ensure_bound_authority_v1(&self, authority: &NovNativeSealEpochAuthorityV1) -> Result<()> {
        authority.validate()?;
        let stored = self
            .load_epoch_authority(authority.chain_id, authority.epoch)?
            .context("NOV native seal overlay quarantine has no pinned epoch authority")?;
        if stored != *authority {
            bail!("NOV native seal overlay authority differs from the durable pinned manifest");
        }
        Ok(())
    }

    fn stage_artifact_v1(
        &self,
        batch: &mut RocksDbWriteBatch,
        authority: &NovNativeSealEpochAuthorityV1,
        artifact: &NovNativeSealOverlayArtifactV1,
        local_validator_id: Option<[u8; 32]>,
    ) -> Result<Vec<[u8; 32]>> {
        artifact.validate(authority)?;
        let proposal = artifact.proposal();
        let proposal_evidence = self.stage_proposal_v1(batch, proposal)?;
        let mut local_identity_evidence = if local_validator_id == Some(proposal.proposer_id) {
            proposal_evidence.clone()
        } else {
            Vec::new()
        };
        let mut evidence = proposal_evidence;
        match artifact {
            NovNativeSealOverlayArtifactV1::Proposal(_) => {}
            NovNativeSealOverlayArtifactV1::Vote { vote, .. } => {
                let vote_evidence = self.stage_vote_v1(batch, vote)?;
                if local_validator_id == Some(vote.validator_id) && !vote_evidence.is_empty() {
                    local_identity_evidence.extend(vote_evidence.iter().copied());
                }
                evidence.extend(vote_evidence);
            }
            NovNativeSealOverlayArtifactV1::QuorumCertificate { qc, .. } => {
                evidence.extend(self.stage_qc_v1(batch, qc)?);
            }
        }
        if let Some(local_validator_id) = local_validator_id {
            local_identity_evidence.sort();
            local_identity_evidence.dedup();
            if !local_identity_evidence.is_empty() {
                self.stage_identity_guard_v1(
                    batch,
                    authority.chain_id,
                    authority.epoch,
                    local_validator_id,
                    local_identity_evidence.as_slice(),
                )?;
            }
        }
        Ok(evidence)
    }

    fn stage_proposal_v1(
        &self,
        batch: &mut RocksDbWriteBatch,
        proposal: &NovNativeSealProposalV1,
    ) -> Result<Vec<[u8; 32]>> {
        stage_json_object_if_available_v1(
            &self.db,
            batch,
            proposal_object_key_v1(&proposal.proposal_hash).as_bytes(),
            proposal,
            "remote proposal object",
        )?;
        self.stage_strict_slot_v1(
            batch,
            NovNativeSealOverlayArtifactKindV1::Proposal,
            proposal_slot_key_v1(proposal),
            proposal.subject.chain_id,
            proposal.subject.epoch,
            proposal.subject.height,
            proposal.subject.round,
            proposal.proposal_hash,
        )
    }

    fn stage_vote_v1(
        &self,
        batch: &mut RocksDbWriteBatch,
        vote: &NovNativeSealVoteV1,
    ) -> Result<Vec<[u8; 32]>> {
        stage_json_object_if_available_v1(
            &self.db,
            batch,
            vote_object_key_v1(&vote.vote_hash).as_bytes(),
            vote,
            "remote vote object",
        )?;
        self.stage_strict_slot_v1(
            batch,
            NovNativeSealOverlayArtifactKindV1::Vote,
            vote_slot_key_v1(vote),
            vote.chain_id,
            vote.epoch,
            vote.height,
            vote.round,
            vote.vote_hash,
        )
    }

    fn stage_qc_v1(
        &self,
        batch: &mut RocksDbWriteBatch,
        qc: &NovNativeSealQuorumCertificateV1,
    ) -> Result<Vec<[u8; 32]>> {
        stage_json_object_if_available_v1(
            &self.db,
            batch,
            qc_object_key_v1(&qc.qc_hash).as_bytes(),
            qc,
            "remote QC object",
        )?;
        let slot_key = qc_slot_key_v1(qc);
        let slot_binding = slot_binding_v1(slot_key.as_str());
        let mut index = self.load_slot_index_v1(
            NovNativeSealOverlayArtifactKindV1::QuorumCertificate,
            slot_key.as_str(),
            slot_binding,
        )?;
        let mut evidence = Vec::new();
        if !index.object_hashes.contains(&qc.qc_hash) {
            if index.object_hashes.len() >= NOV_NATIVE_SEAL_OVERLAY_MAX_SLOT_OBJECTS_V1 {
                bail!("NOV native seal QC slot exceeds its fail-closed object bound");
            }
            for existing_hash in &index.object_hashes {
                let existing = read_json_v1::<NovNativeSealQuorumCertificateV1>(
                    &self.db,
                    qc_object_key_v1(existing_hash).as_bytes(),
                    "existing remote QC",
                )?
                .context("NOV native seal QC slot points to a missing object")?;
                if existing.subject_hash != qc.subject_hash
                    || existing.proposal_hash != qc.proposal_hash
                {
                    evidence.push(self.stage_equivocation_v1(
                        batch,
                        NovNativeSealOverlayArtifactKindV1::QuorumCertificate,
                        qc.subject.chain_id,
                        qc.subject.epoch,
                        qc.subject.height,
                        qc.subject.round,
                        slot_binding,
                        *existing_hash,
                        qc.qc_hash,
                    )?);
                }
            }
            index.object_hashes.push(qc.qc_hash);
            index.object_hashes.sort();
            put_json_v1(batch, slot_key.as_bytes(), &index, "remote QC slot index")?;
        }
        Ok(evidence)
    }

    #[allow(clippy::too_many_arguments)]
    fn stage_strict_slot_v1(
        &self,
        batch: &mut RocksDbWriteBatch,
        kind: NovNativeSealOverlayArtifactKindV1,
        slot_key: String,
        chain_id: u64,
        epoch: u64,
        height: u64,
        round: u64,
        object_hash: [u8; 32],
    ) -> Result<Vec<[u8; 32]>> {
        let slot_binding = slot_binding_v1(slot_key.as_str());
        let mut index = self.load_slot_index_v1(kind, slot_key.as_str(), slot_binding)?;
        let mut evidence = Vec::new();
        if !index.object_hashes.contains(&object_hash) {
            if index.object_hashes.len() >= NOV_NATIVE_SEAL_OVERLAY_MAX_SLOT_OBJECTS_V1 {
                bail!("NOV native seal signed slot exceeds its fail-closed object bound");
            }
            for existing_hash in &index.object_hashes {
                evidence.push(self.stage_equivocation_v1(
                    batch,
                    kind,
                    chain_id,
                    epoch,
                    height,
                    round,
                    slot_binding,
                    *existing_hash,
                    object_hash,
                )?);
            }
            index.object_hashes.push(object_hash);
            index.object_hashes.sort();
            put_json_v1(
                batch,
                slot_key.as_bytes(),
                &index,
                "remote signed slot index",
            )?;
        }
        Ok(evidence)
    }

    fn load_slot_index_v1(
        &self,
        kind: NovNativeSealOverlayArtifactKindV1,
        key: &str,
        slot_binding: [u8; 32],
    ) -> Result<NovNativeSealOverlaySlotIndexV1> {
        let index = read_json_v1::<NovNativeSealOverlaySlotIndexV1>(
            &self.db,
            key.as_bytes(),
            "remote signed slot index",
        )?
        .unwrap_or(NovNativeSealOverlaySlotIndexV1 {
            schema: "novovm-native-seal-overlay-slot-index/v1".to_string(),
            artifact_kind: kind,
            slot_binding,
            object_hashes: Vec::new(),
        });
        validate_slot_index_v1(&index, kind, slot_binding)?;
        Ok(index)
    }

    #[allow(clippy::too_many_arguments)]
    fn stage_equivocation_v1(
        &self,
        batch: &mut RocksDbWriteBatch,
        kind: NovNativeSealOverlayArtifactKindV1,
        chain_id: u64,
        epoch: u64,
        height: u64,
        round: u64,
        slot_binding: [u8; 32],
        left: [u8; 32],
        right: [u8; 32],
    ) -> Result<[u8; 32]> {
        let (left_object_hash, right_object_hash) = if left < right {
            (left, right)
        } else {
            (right, left)
        };
        if left_object_hash == right_object_hash {
            bail!("NOV native seal equivocation requires two distinct objects");
        }
        let evidence_hash = equivocation_hash_v1(
            kind,
            chain_id,
            epoch,
            height,
            round,
            slot_binding,
            left_object_hash,
            right_object_hash,
        );
        let evidence = NovNativeSealOverlayEquivocationEvidenceV1 {
            schema: NOV_NATIVE_SEAL_OVERLAY_EQUIVOCATION_SCHEMA_V1.to_string(),
            artifact_kind: kind,
            chain_id,
            epoch,
            height,
            round,
            slot_binding,
            left_object_hash,
            right_object_hash,
            evidence_hash,
        };
        validate_equivocation_v1(&evidence)?;
        stage_json_object_if_available_v1(
            &self.db,
            batch,
            equivocation_object_key_v1(&evidence_hash).as_bytes(),
            &evidence,
            "equivocation evidence",
        )?;
        Ok(evidence_hash)
    }

    fn stage_identity_guard_v1(
        &self,
        batch: &mut RocksDbWriteBatch,
        chain_id: u64,
        epoch: u64,
        validator_id: [u8; 32],
        evidence_hashes: &[[u8; 32]],
    ) -> Result<()> {
        let key = identity_guard_key_v1(chain_id, epoch, &validator_id);
        let mut guard = read_json_v1::<NovNativeSealOverlayIdentityGuardV1>(
            &self.db,
            key.as_bytes(),
            "local identity guard",
        )?
        .unwrap_or(NovNativeSealOverlayIdentityGuardV1 {
            schema: NOV_NATIVE_SEAL_OVERLAY_IDENTITY_GUARD_SCHEMA_V1.to_string(),
            chain_id,
            epoch,
            validator_id,
            signing_blocked: true,
            evidence_hashes: Vec::new(),
        });
        for evidence_hash in evidence_hashes {
            if !guard.evidence_hashes.contains(evidence_hash) {
                if guard.evidence_hashes.len() >= NOV_NATIVE_SEAL_OVERLAY_MAX_SLOT_OBJECTS_V1 * 4 {
                    bail!("NOV native seal local identity guard evidence exceeds its bound");
                }
                guard.evidence_hashes.push(*evidence_hash);
            }
        }
        guard.evidence_hashes.sort();
        validate_identity_guard_v1(&guard)?;
        put_json_v1(batch, key.as_bytes(), &guard, "local identity guard")
    }

    fn load_proposal_object_v1(
        &self,
        proposal_hash: [u8; 32],
        authority: &NovNativeSealEpochAuthorityV1,
    ) -> Result<Option<NovNativeSealProposalV1>> {
        let proposal = read_json_v1::<NovNativeSealProposalV1>(
            &self.db,
            proposal_object_key_v1(&proposal_hash).as_bytes(),
            "remote proposal object",
        )?;
        if let Some(proposal) = proposal.as_ref() {
            proposal.verify(&authority.validator_set)?;
            validate_subject_authority_v1(&proposal.subject, authority)?;
            if proposal.proposal_hash != proposal_hash
                || proposal.proposer_id
                    != authority.expected_leader(proposal.subject.height, proposal.subject.round)?
            {
                bail!("remote proposal object key or leader binding mismatch");
            }
        }
        Ok(proposal)
    }

    fn update_admission_state_if_present_locked_v1(
        &self,
        kind: NovNativeSealOverlayArtifactKindV1,
        object_hash: [u8; 32],
        state: NovNativeSealOverlayAdmissionStateV1,
    ) -> Result<()> {
        let key = admission_key_v1(kind, &object_hash);
        let Some(mut admission) = read_json_v1::<NovNativeSealOverlayAdmissionV1>(
            &self.db,
            key.as_bytes(),
            "overlay admission",
        )?
        else {
            return Ok(());
        };
        validate_admission_v1(&admission)?;
        if admission.state == NovNativeSealOverlayAdmissionStateV1::EquivocationQuarantined {
            bail!("equivocating NOV native seal artifact cannot leave quarantine");
        }
        admission.state = state;
        validate_admission_v1(&admission)?;
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            key.as_bytes(),
            &admission,
            "overlay admission state",
        )?;
        write_sync_v1(&self.db, batch).context("persist NOV native seal admission state")
    }

    fn ensure_proposal_uncontested_v1(&self, proposal: &NovNativeSealProposalV1) -> Result<()> {
        if self
            .load_admission(
                NovNativeSealOverlayArtifactKindV1::Proposal,
                proposal.proposal_hash,
            )?
            .is_some_and(|admission| {
                admission.state == NovNativeSealOverlayAdmissionStateV1::EquivocationQuarantined
                    || !admission.evidence_hashes.is_empty()
            })
        {
            bail!("equivocating NOV native seal proposal cannot leave quarantine");
        }
        let slot_key = proposal_slot_key_v1(proposal);
        let slot_binding = slot_binding_v1(slot_key.as_str());
        let index = self.load_slot_index_v1(
            NovNativeSealOverlayArtifactKindV1::Proposal,
            slot_key.as_str(),
            slot_binding,
        )?;
        if index.object_hashes.as_slice() != [proposal.proposal_hash] {
            bail!("contested NOV native seal proposal slot cannot leave quarantine");
        }
        Ok(())
    }

    fn ensure_qc_uncontested_v1(&self, qc: &NovNativeSealQuorumCertificateV1) -> Result<()> {
        if self
            .load_admission(
                NovNativeSealOverlayArtifactKindV1::QuorumCertificate,
                qc.qc_hash,
            )?
            .is_some_and(|admission| {
                admission.state == NovNativeSealOverlayAdmissionStateV1::EquivocationQuarantined
                    || !admission.evidence_hashes.is_empty()
            })
        {
            bail!("competing NOV native seal QC cannot leave quarantine");
        }
        let slot_key = qc_slot_key_v1(qc);
        let slot_binding = slot_binding_v1(slot_key.as_str());
        let index = self.load_slot_index_v1(
            NovNativeSealOverlayArtifactKindV1::QuorumCertificate,
            slot_key.as_str(),
            slot_binding,
        )?;
        for hash in index.object_hashes {
            let existing = read_json_v1::<NovNativeSealQuorumCertificateV1>(
                &self.db,
                qc_object_key_v1(&hash).as_bytes(),
                "contested remote QC",
            )?
            .context("NOV native seal QC slot points to a missing object")?;
            if existing.subject_hash != qc.subject_hash
                || existing.proposal_hash != qc.proposal_hash
            {
                bail!("contested NOV native seal QC slot cannot leave quarantine");
            }
        }
        Ok(())
    }
}

fn validate_subject_authority_v1(
    subject: &NovNativeSealSubjectV1,
    authority: &NovNativeSealEpochAuthorityV1,
) -> Result<()> {
    if subject.chain_id != authority.chain_id
        || subject.epoch != authority.epoch
        || subject.height < authority.activation_height
        || subject.validator_set_hash != authority.validator_set.validator_set_hash
        || subject.genesis_block_hash != authority.genesis_block_hash
        || subject.protocol_config_commitment != authority.protocol_config_commitment
    {
        bail!("NOV native seal subject is outside the pinned epoch authority");
    }
    Ok(())
}

fn compact_qc_v1(qc: &NovNativeSealQuorumCertificateV1) -> NovNativeSealCompactQcWireV1 {
    NovNativeSealCompactQcWireV1 {
        schema: qc.schema.clone(),
        subject: qc.subject.clone(),
        subject_hash: qc.subject_hash,
        proposal_hash: qc.proposal_hash,
        validator_set_hash: qc.validator_set_hash,
        votes: qc
            .votes
            .iter()
            .map(|vote| NovNativeSealCompactVoteWireV1 {
                validator_id: vote.validator_id,
                signature: vote.signature.clone(),
                vote_hash: vote.vote_hash,
            })
            .collect(),
        signature_count: qc.signature_count,
        signed_weight: qc.signed_weight,
        quorum_weight: qc.quorum_weight,
        threshold_satisfied: qc.threshold_satisfied,
        qc_hash: qc.qc_hash,
    }
}

fn expand_qc_v1(compact: NovNativeSealCompactQcWireV1) -> Result<NovNativeSealQuorumCertificateV1> {
    let votes = compact
        .votes
        .into_iter()
        .map(|vote| NovNativeSealVoteV1 {
            schema: NOV_NATIVE_BLOCK_SEAL_VOTE_SCHEMA_V1.to_string(),
            chain_id: compact.subject.chain_id,
            epoch: compact.subject.epoch,
            height: compact.subject.height,
            round: compact.subject.round,
            phase: compact.subject.phase.clone(),
            validator_set_hash: compact.subject.validator_set_hash,
            subject_hash: compact.subject.subject_hash,
            proposal_hash: compact.proposal_hash,
            validator_id: vote.validator_id,
            signature_scheme: NOV_NATIVE_BLOCK_SEAL_SIGNATURE_SCHEME_V1.to_string(),
            signature: vote.signature,
            vote_hash: vote.vote_hash,
        })
        .collect();
    Ok(NovNativeSealQuorumCertificateV1 {
        schema: compact.schema,
        subject: compact.subject,
        subject_hash: compact.subject_hash,
        proposal_hash: compact.proposal_hash,
        validator_set_hash: compact.validator_set_hash,
        votes,
        signature_count: compact.signature_count,
        signed_weight: compact.signed_weight,
        quorum_weight: compact.quorum_weight,
        threshold_satisfied: compact.threshold_satisfied,
        qc_hash: compact.qc_hash,
    })
}

fn canonical_postcard_decode_v1<T>(bytes: &[u8]) -> Result<T>
where
    T: DeserializeOwned + Serialize,
{
    let decoded: T =
        postcard::from_bytes(bytes).context("decode NOV native seal canonical wire")?;
    let canonical =
        postcard::to_allocvec(&decoded).context("re-encode NOV native seal canonical wire")?;
    if canonical.as_slice() != bytes {
        bail!("NOV native seal overlay wire payload is not canonical");
    }
    Ok(decoded)
}

fn validate_ingress_context_v1(
    context: &NovNativeSealOverlayIngressContextV1,
    authority: &NovNativeSealEpochAuthorityV1,
) -> Result<()> {
    if context.local_execution_height < authority.activation_height
        || context.received_at_unix_ms == 0
        || context
            .local_validator_id
            .is_some_and(|validator_id| authority.validator_set.validator(validator_id).is_none())
    {
        bail!("NOV native seal overlay ingress context is invalid");
    }
    Ok(())
}

fn validate_ingress_window_v1(
    subject: &NovNativeSealSubjectV1,
    authority: &NovNativeSealEpochAuthorityV1,
    context: &NovNativeSealOverlayIngressContextV1,
) -> Result<()> {
    let minimum = context
        .local_execution_height
        .saturating_sub(NOV_NATIVE_SEAL_OVERLAY_MAX_HEIGHT_BEHIND_V1)
        .max(authority.activation_height);
    let maximum = context
        .local_execution_height
        .checked_add(NOV_NATIVE_SEAL_OVERLAY_MAX_HEIGHT_AHEAD_V1)
        .context("NOV native seal overlay height window overflow")?;
    if subject.height < minimum
        || subject.height > maximum
        || subject.round > NOV_NATIVE_SEAL_OVERLAY_MAX_ROUND_V1
    {
        bail!("NOV native seal artifact is outside the bounded ingress window");
    }
    Ok(())
}

fn validate_admission_v1(admission: &NovNativeSealOverlayAdmissionV1) -> Result<()> {
    if admission.schema != NOV_NATIVE_SEAL_OVERLAY_ADMISSION_SCHEMA_V1
        || admission.chain_id == 0
        || admission.epoch != 1
        || admission.height == 0
        || admission.object_hash == [0u8; 32]
        || admission.proposal_hash == [0u8; 32]
        || !valid_peer_id_v1(admission.first_source_peer_id.as_str())
        || !valid_peer_id_v1(admission.last_source_peer_id.as_str())
        || admission.wire_hash == [0u8; 32]
        || admission.first_received_at_unix_ms == 0
        || admission.last_received_at_unix_ms < admission.first_received_at_unix_ms
        || admission.receive_count == 0
        || admission.evidence_hashes.len() > NOV_NATIVE_SEAL_OVERLAY_MAX_SLOT_OBJECTS_V1
        || admission
            .evidence_hashes
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || (admission.state == NovNativeSealOverlayAdmissionStateV1::EquivocationQuarantined
            && admission.evidence_hashes.is_empty())
        || (admission.state != NovNativeSealOverlayAdmissionStateV1::EquivocationQuarantined
            && !admission.evidence_hashes.is_empty())
    {
        bail!("NOV native seal overlay admission is invalid");
    }
    Ok(())
}

fn validate_slot_index_v1(
    index: &NovNativeSealOverlaySlotIndexV1,
    kind: NovNativeSealOverlayArtifactKindV1,
    slot_binding: [u8; 32],
) -> Result<()> {
    if index.schema != "novovm-native-seal-overlay-slot-index/v1"
        || index.artifact_kind != kind
        || index.slot_binding != slot_binding
        || index.object_hashes.len() > NOV_NATIVE_SEAL_OVERLAY_MAX_SLOT_OBJECTS_V1
        || index
            .object_hashes
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        bail!("NOV native seal overlay slot index is invalid");
    }
    Ok(())
}

fn validate_equivocation_v1(evidence: &NovNativeSealOverlayEquivocationEvidenceV1) -> Result<()> {
    if evidence.schema != NOV_NATIVE_SEAL_OVERLAY_EQUIVOCATION_SCHEMA_V1
        || evidence.chain_id == 0
        || evidence.epoch != 1
        || evidence.height == 0
        || evidence.left_object_hash >= evidence.right_object_hash
        || evidence.evidence_hash
            != equivocation_hash_v1(
                evidence.artifact_kind,
                evidence.chain_id,
                evidence.epoch,
                evidence.height,
                evidence.round,
                evidence.slot_binding,
                evidence.left_object_hash,
                evidence.right_object_hash,
            )
    {
        bail!("NOV native seal overlay equivocation evidence is invalid");
    }
    Ok(())
}

fn validate_identity_guard_v1(guard: &NovNativeSealOverlayIdentityGuardV1) -> Result<()> {
    if guard.schema != NOV_NATIVE_SEAL_OVERLAY_IDENTITY_GUARD_SCHEMA_V1
        || guard.chain_id == 0
        || guard.epoch != 1
        || guard.validator_id == [0u8; 32]
        || !guard.signing_blocked
        || guard.evidence_hashes.is_empty()
        || guard.evidence_hashes.len() > NOV_NATIVE_SEAL_OVERLAY_MAX_SLOT_OBJECTS_V1 * 4
        || guard
            .evidence_hashes
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        bail!("NOV native seal overlay local identity guard is invalid");
    }
    Ok(())
}

fn authority_commitment_v1(authority: &NovNativeSealEpochAuthorityV1) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(AUTHORITY_COMMITMENT_DOMAIN_V1);
    update_len_prefixed_v1(&mut hasher, authority.schema.as_bytes());
    update_len_prefixed_v1(&mut hasher, authority.authority_kind.as_bytes());
    hasher.update(authority.chain_id.to_be_bytes());
    hasher.update(authority.genesis_block_hash);
    hasher.update(authority.protocol_config_commitment);
    hasher.update(authority.epoch.to_be_bytes());
    hasher.update(authority.activation_height.to_be_bytes());
    hasher.update(authority.validator_set.validator_set_hash);
    hasher.update((authority.transport_bindings.len() as u64).to_be_bytes());
    for binding in &authority.transport_bindings {
        hasher.update(binding.validator_id);
        update_len_prefixed_v1(&mut hasher, binding.transport_peer_id.as_bytes());
    }
    update_len_prefixed_v1(&mut hasher, authority.leader_schedule.as_bytes());
    hasher.finalize().into()
}

fn wire_checksum_v1(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(WIRE_CHECKSUM_DOMAIN_V1);
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    hasher.finalize().into()
}

fn slot_binding_v1(slot_key: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(SLOT_BINDING_DOMAIN_V1);
    update_len_prefixed_v1(&mut hasher, slot_key.as_bytes());
    hasher.finalize().into()
}

#[allow(clippy::too_many_arguments)]
fn equivocation_hash_v1(
    kind: NovNativeSealOverlayArtifactKindV1,
    chain_id: u64,
    epoch: u64,
    height: u64,
    round: u64,
    slot_binding: [u8; 32],
    left: [u8; 32],
    right: [u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(EQUIVOCATION_DOMAIN_V1);
    hasher.update([kind.code()]);
    hasher.update(chain_id.to_be_bytes());
    hasher.update(epoch.to_be_bytes());
    hasher.update(height.to_be_bytes());
    hasher.update(round.to_be_bytes());
    hasher.update(slot_binding);
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

fn update_len_prefixed_v1(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn read_u64_be_v1(bytes: &[u8], offset: usize) -> Result<u64> {
    let end = offset
        .checked_add(8)
        .context("NOV native seal wire integer offset overflow")?;
    let raw: [u8; 8] = bytes
        .get(offset..end)
        .context("NOV native seal wire integer is truncated")?
        .try_into()
        .context("NOV native seal wire integer width mismatch")?;
    Ok(u64::from_be_bytes(raw))
}

fn decode_hex_32_v1(label: &str, value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 {
        bail!("NOV native seal {label} is not 32-byte canonical hex");
    }
    let bytes = value.as_bytes();
    let mut out = [0u8; 32];
    for (index, pair) in bytes.chunks_exact(2).enumerate() {
        let high = decode_hex_nibble_v1(pair[0]).context("invalid commitment hex")?;
        let low = decode_hex_nibble_v1(pair[1]).context("invalid commitment hex")?;
        out[index] = (high << 4) | low;
    }
    Ok(out)
}

const fn decode_hex_nibble_v1(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn valid_peer_id_v1(peer_id: &str) -> bool {
    !peer_id.is_empty()
        && peer_id.len() <= 256
        && peer_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.'))
}

fn write_sync_v1(db: &DB, batch: RocksDbWriteBatch) -> Result<()> {
    let mut options = WriteOptions::default();
    options.set_sync(true);
    db.write_opt(batch, &options)
        .context("write synchronized NOV native seal overlay batch")
}

fn read_json_v1<T: DeserializeOwned>(db: &DB, key: &[u8], label: &str) -> Result<Option<T>> {
    db.get(key)
        .with_context(|| format!("read NOV native seal overlay {label}"))?
        .map(|raw| {
            serde_json::from_slice(raw.as_slice())
                .with_context(|| format!("decode NOV native seal overlay {label}"))
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
        .with_context(|| format!("encode NOV native seal overlay {label}"))?;
    batch.put(key, encoded);
    Ok(())
}

fn stage_json_object_if_available_v1<T>(
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
            bail!("NOV native seal overlay {label} hash collision or conflicting object");
        }
        return Ok(());
    }
    put_json_v1(batch, key, value, label)
}

fn hex_v1(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn authority_key_v1(chain_id: u64, epoch: u64) -> String {
    format!("{KEY_PREFIX_V1}authority/{chain_id:020}/{epoch:020}")
}

fn proposal_object_key_v1(hash: &[u8; 32]) -> String {
    format!("{KEY_PREFIX_V1}remote/proposal/object/{}", hex_v1(hash))
}

fn vote_object_key_v1(hash: &[u8; 32]) -> String {
    format!("{KEY_PREFIX_V1}remote/vote/object/{}", hex_v1(hash))
}

fn qc_object_key_v1(hash: &[u8; 32]) -> String {
    format!("{KEY_PREFIX_V1}remote/qc/object/{}", hex_v1(hash))
}

fn admission_key_v1(kind: NovNativeSealOverlayArtifactKindV1, hash: &[u8; 32]) -> String {
    format!(
        "{KEY_PREFIX_V1}remote/admission/{}/{}",
        kind.label(),
        hex_v1(hash)
    )
}

fn proposal_slot_key_v1(proposal: &NovNativeSealProposalV1) -> String {
    format!(
        "{KEY_PREFIX_V1}remote/proposal/slot/{:020}/{:020}/{:020}/{:020}/{}",
        proposal.subject.chain_id,
        proposal.subject.epoch,
        proposal.subject.height,
        proposal.subject.round,
        hex_v1(&proposal.proposer_id)
    )
}

fn vote_slot_key_v1(vote: &NovNativeSealVoteV1) -> String {
    format!(
        "{KEY_PREFIX_V1}remote/vote/slot/{:020}/{:020}/{:020}/{:020}/{}/{}",
        vote.chain_id,
        vote.epoch,
        vote.height,
        vote.round,
        vote.phase,
        hex_v1(&vote.validator_id)
    )
}

fn qc_slot_key_v1(qc: &NovNativeSealQuorumCertificateV1) -> String {
    format!(
        "{KEY_PREFIX_V1}remote/qc/slot/{:020}/{:020}/{:020}/{:020}",
        qc.subject.chain_id, qc.subject.epoch, qc.subject.height, qc.subject.round
    )
}

fn equivocation_object_key_v1(hash: &[u8; 32]) -> String {
    format!("{KEY_PREFIX_V1}remote/equivocation/object/{}", hex_v1(hash))
}

fn identity_guard_key_v1(chain_id: u64, epoch: u64, validator_id: &[u8; 32]) -> String {
    format!(
        "{KEY_PREFIX_V1}local/identity_guard/{chain_id:020}/{epoch:020}/{}",
        hex_v1(validator_id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        native_block_ledger::{
            NovNativeBlockCandidateInputV1, NovNativeBlockCommitInputV1, NovNativeDurableBlockV1,
            NovNativePreparedAoemParentV1,
        },
        native_block_seal::{NovNativeSealLocalProposalRequestV1, NovNativeSealValidatorV1},
    };
    use ed25519_dalek::SigningKey;
    use novovm_network::peer_id_from_ed25519_public_key_v1;
    use novovm_protocol::NovBlockExecutionContextV1;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

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
                "novovm-native-seal-overlay-{label}-{}-{serial}-{nanos}",
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

        fn quarantine_path(&self) -> PathBuf {
            self.root.join("seal-overlay-quarantine")
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
        let validators = keys
            .iter()
            .map(|key| {
                NovNativeSealValidatorV1::new(*key.verifying_key().as_bytes(), 1)
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
        let prepared = ledger
            .prepare(NovNativeBlockCandidateInputV1 {
                context: NovBlockExecutionContextV1 {
                    chain_id,
                    block_height: height,
                    parent_block_hash: parent_hash,
                    slot: height.checked_mul(2).expect("slot"),
                    timestamp_unix_ms: 1_900_000_000_000u64
                        .checked_add(height.checked_mul(2_000).expect("timestamp step"))
                        .expect("timestamp"),
                },
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
            aoem_batch_id: format!("seal-overlay-batch-{height}-{seed}"),
            aoem_batch_result_id: hex_v1(&batch_result),
            aoem_evidence_commitment: [seed.wrapping_add(5); 32],
            state_version,
        };
        let bound = ledger
            .bind_expected_aoem_batch_id(
                &prepared,
                input.aoem_batch_id.as_str(),
                format!("{state_version:064x}").as_str(),
            )
            .expect("bind expected AOEM result");
        ledger.commit(&bound, input).expect("commit block")
    }

    fn authority_v1(
        ledger: &NovNativeBlockLedgerV1,
        set: &NovNativeSealValidatorSetV1,
    ) -> NovNativeSealEpochAuthorityV1 {
        let bindings = set
            .validators
            .iter()
            .map(|validator| NovNativeSealValidatorTransportBindingV1 {
                validator_id: validator.validator_id,
                transport_peer_id: peer_id_from_ed25519_public_key_v1(&validator.public_key),
            })
            .collect();
        NovNativeSealEpochAuthorityV1::derive_operator_pinned_genesis_epoch(
            ledger,
            set.clone(),
            bindings,
        )
        .expect("derive authority")
    }

    fn genesis_fixture_v1(
        label: &str,
        chain_id: u64,
    ) -> (
        TestNodeV1,
        NovNativeDurableBlockV1,
        Vec<SigningKey>,
        NovNativeSealValidatorSetV1,
        NovNativeSealEpochAuthorityV1,
    ) {
        let node = TestNodeV1::new(label);
        bind_ownership_v1(node.ledger(), chain_id);
        let block = commit_block_v1(node.ledger(), chain_id, 1, None, 0x31);
        let (keys, set) = validator_fixture_v1(chain_id);
        let authority = authority_v1(node.ledger(), &set);
        (node, block, keys, set, authority)
    }

    fn key_for_validator_v1<'a>(
        keys: &'a [SigningKey],
        set: &NovNativeSealValidatorSetV1,
        validator_id: [u8; 32],
    ) -> &'a SigningKey {
        let public_key = set
            .validator(validator_id)
            .expect("validator in set")
            .public_key;
        keys.iter()
            .find(|key| key.verifying_key().to_bytes() == public_key)
            .expect("validator signing key")
    }

    fn sign_proposal_v1(
        node: &TestNodeV1,
        block: &NovNativeDurableBlockV1,
        keys: &[SigningKey],
        set: &NovNativeSealValidatorSetV1,
        authority: &NovNativeSealEpochAuthorityV1,
        justify_qc_hash: Option<[u8; 32]>,
        round: u64,
    ) -> NovNativeSealProposalV1 {
        let leader_id = authority
            .expected_leader(block.header.height, 0)
            .expect("expected leader");
        node.store()
            .sign_local_proposal(
                node.ledger(),
                &NovNativeSealLocalProposalRequestV1 {
                    chain_id: block.header.chain_id,
                    block_hash: block.header.block_hash,
                    round,
                    justify_qc_hash,
                },
                set,
                key_for_validator_v1(keys, set, leader_id),
            )
            .expect("sign proposal")
    }

    fn votes_v1(
        node: &TestNodeV1,
        proposal: &NovNativeSealProposalV1,
        keys: &[SigningKey],
        set: &NovNativeSealValidatorSetV1,
        count: usize,
    ) -> Vec<NovNativeSealVoteV1> {
        keys.iter()
            .take(count)
            .map(|key| {
                node.store()
                    .sign_local_vote(node.ledger(), proposal, set, key)
                    .expect("sign vote")
            })
            .collect()
    }

    fn refresh_wire_checksum_v1(wire: &mut [u8]) {
        let payload_len =
            u32::from_be_bytes(wire[92..96].try_into().expect("payload length")) as usize;
        let payload_end = WIRE_HEADER_BYTES_V1 + payload_len;
        let checksum = wire_checksum_v1(&wire[..payload_end]);
        wire[payload_end..].copy_from_slice(&checksum);
    }

    fn assert_candidate_remains_unsealed_v1(
        ledger: &NovNativeBlockLedgerV1,
        chain_id: u64,
        block_hash: [u8; 32],
    ) {
        let record = ledger
            .load_candidate_record(chain_id, block_hash)
            .expect("load candidate record")
            .expect("candidate record");
        assert!(record.local_aoem_readback_verified);
        assert!(!record.fork_choice_selected);
        assert!(!record.chain_canonical);
        assert!(!record.proof_sealed);
        assert!(!record.safe);
        assert!(!record.finalized);
        let block = ledger
            .load_candidate_block(chain_id, block_hash)
            .expect("load candidate block")
            .expect("candidate block");
        assert!(!block.header.proof_sealed);
        assert!(!block.header.safe);
        assert!(!block.header.finalized);
        assert!(!block.execution_evidence.proof_sealed);
        let head = ledger
            .load_head(chain_id)
            .expect("load ledger head")
            .expect("ledger head");
        assert!(!head.proof_sealed);
        assert!(!head.safe);
        assert!(!head.finalized);
    }

    #[test]
    fn authority_canonical_wire_and_source_binding_fail_closed() {
        let chain_id = 82_101;
        let (node, block, keys, set, authority) = genesis_fixture_v1("wire", chain_id);
        authority.validate().expect("valid authority");
        assert!(authority.expected_leader(1, 1).is_err());

        let quarantine = NovNativeSealOverlayQuarantineV1::open(node.quarantine_path().as_path())
            .expect("open quarantine");
        let mut wrong_commitment = authority.authority_commitment;
        wrong_commitment[0] ^= 1;
        assert!(quarantine
            .bind_epoch_authority(node.ledger(), &authority, wrong_commitment)
            .is_err());
        assert!(quarantine
            .bind_epoch_authority(node.ledger(), &authority, authority.authority_commitment)
            .expect("pin authority"));
        assert!(!quarantine
            .bind_epoch_authority(node.ledger(), &authority, authority.authority_commitment)
            .expect("repeat authority"));

        let proposal = sign_proposal_v1(&node, &block, &keys, &set, &authority, None, 0);
        let votes = votes_v1(&node, &proposal, &keys, &set, 3);
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
        .expect("3-of-4 QC");
        assert_eq!(qc.signature_count, 3);
        assert!(qc.threshold_satisfied);

        let vote_artifact = NovNativeSealOverlayArtifactV1::Vote {
            proposal: Box::new(proposal.clone()),
            vote: Box::new(votes[0].clone()),
        };
        let vote_peer = authority
            .transport_peer_id(votes[0].validator_id)
            .expect("vote peer");
        vote_artifact
            .validate_authenticated_source(&authority, vote_peer)
            .expect("vote source");
        let wrong_vote_peer = authority
            .transport_bindings
            .iter()
            .find(|binding| binding.validator_id != votes[0].validator_id)
            .expect("different vote peer")
            .transport_peer_id
            .as_str();
        assert!(vote_artifact
            .validate_authenticated_source(&authority, wrong_vote_peer)
            .is_err());
        let qc_artifact = NovNativeSealOverlayArtifactV1::QuorumCertificate {
            proposal: Box::new(proposal.clone()),
            qc: Box::new(qc.clone()),
        };
        assert!(qc_artifact
            .validate_authenticated_source(&authority, "unbound-peer")
            .is_err());

        let artifacts = [
            NovNativeSealOverlayArtifactV1::Proposal(Box::new(proposal.clone())),
            vote_artifact,
            qc_artifact,
        ];
        for artifact in artifacts {
            let wire = encode_nov_native_seal_overlay_wire_v1(&artifact, &authority)
                .expect("encode artifact");
            assert!(is_nov_native_seal_overlay_wire_v1(wire.as_slice()));
            assert!(wire.len() <= NOV_NATIVE_SEAL_OVERLAY_MAX_WIRE_BYTES_V1);
            let decoded = decode_nov_native_seal_overlay_wire_v1(wire.as_slice(), &authority)
                .expect("decode artifact");
            assert_eq!(decoded.artifact, artifact);
            assert_eq!(decoded.authority_commitment, authority.authority_commitment);
        }

        let proposal_artifact =
            NovNativeSealOverlayArtifactV1::Proposal(Box::new(proposal.clone()));
        let leader_peer = authority
            .transport_peer_id(proposal.proposer_id)
            .expect("leader peer");
        proposal_artifact
            .validate_authenticated_source(&authority, leader_peer)
            .expect("leader source");
        let wrong_peer = authority
            .transport_bindings
            .iter()
            .find(|binding| binding.validator_id != proposal.proposer_id)
            .expect("different peer")
            .transport_peer_id
            .as_str();
        assert!(proposal_artifact
            .validate_authenticated_source(&authority, wrong_peer)
            .is_err());

        let wire = encode_nov_native_seal_overlay_wire_v1(&proposal_artifact, &authority)
            .expect("proposal wire");
        for malformed in [
            wire[..wire.len() - 1].to_vec(),
            {
                let mut value = wire.clone();
                value.push(0);
                value
            },
            {
                let mut value = wire.clone();
                value[wire.len() - 1] ^= 1;
                value
            },
            {
                let mut value = wire.clone();
                value[9] ^= 1;
                value
            },
            {
                let mut value = wire.clone();
                value[10] = 0xff;
                value
            },
            {
                let mut value = wire.clone();
                value[11] = 1;
                value
            },
            {
                let mut value = wire.clone();
                value[28] ^= 1;
                refresh_wire_checksum_v1(value.as_mut_slice());
                value
            },
            {
                let mut value = wire.clone();
                value[60] ^= 1;
                refresh_wire_checksum_v1(value.as_mut_slice());
                value
            },
        ] {
            assert!(decode_nov_native_seal_overlay_wire_v1(&malformed, &authority).is_err());
        }
        assert!(decode_nov_native_seal_overlay_wire_v1(
            br#"{\"legacy\":\"consensus-json\"}"#,
            &authority
        )
        .is_err());
        assert!(decode_nov_native_seal_overlay_wire_v1(
            &vec![0u8; NOV_NATIVE_SEAL_OVERLAY_MAX_WIRE_BYTES_V1 + 1],
            &authority
        )
        .is_err());

        let non_leader_key = keys
            .iter()
            .find(|key| key.verifying_key().to_bytes() != set.validators[0].public_key)
            .expect("non-leader key");
        let non_leader = node
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
                non_leader_key,
            )
            .expect("sign non-leader proposal for rejection test");
        assert!(encode_nov_native_seal_overlay_wire_v1(
            &NovNativeSealOverlayArtifactV1::Proposal(Box::new(non_leader)),
            &authority
        )
        .is_err());

        let round_one = sign_proposal_v1(&node, &block, &keys, &set, &authority, None, 1);
        assert!(encode_nov_native_seal_overlay_wire_v1(
            &NovNativeSealOverlayArtifactV1::Proposal(Box::new(round_one)),
            &authority
        )
        .is_err());
    }

    #[test]
    fn quarantine_replay_restart_and_local_bridge_preserve_unsealed_ledger() {
        let chain_id = 82_102;
        let (producer, producer_block, keys, set, authority) =
            genesis_fixture_v1("bridge-producer", chain_id);
        let (receiver, receiver_block, _receiver_keys, receiver_set, receiver_authority) =
            genesis_fixture_v1("bridge-receiver", chain_id);
        assert_eq!(receiver_block, producer_block);
        assert_eq!(receiver_set, set);
        assert_eq!(receiver_authority, authority);

        let proposal =
            sign_proposal_v1(&producer, &producer_block, &keys, &set, &authority, None, 0);
        let votes = votes_v1(&producer, &proposal, &keys, &set, 3);
        let qc =
            NovNativeSealQuorumCertificateV1::from_votes(proposal.subject.clone(), &set, votes)
                .expect("build QC");
        let proposal_artifact =
            NovNativeSealOverlayArtifactV1::Proposal(Box::new(proposal.clone()));
        let proposal_wire = encode_nov_native_seal_overlay_wire_v1(&proposal_artifact, &authority)
            .expect("proposal wire");
        assert_eq!(
            proposal_wire,
            encode_nov_native_seal_overlay_wire_v1(&proposal_artifact, &receiver_authority)
                .expect("cross-root deterministic wire")
        );

        let quarantine_path = receiver.quarantine_path();
        let quarantine = NovNativeSealOverlayQuarantineV1::open(quarantine_path.as_path())
            .expect("open receiver quarantine");
        assert!(quarantine
            .bind_epoch_authority(
                receiver.ledger(),
                &receiver_authority,
                receiver_authority.authority_commitment,
            )
            .expect("pin authority"));
        let leader_peer = authority
            .transport_peer_id(proposal.proposer_id)
            .expect("leader peer");
        let wrong_peer = authority
            .transport_bindings
            .iter()
            .find(|binding| binding.validator_id != proposal.proposer_id)
            .expect("wrong peer")
            .transport_peer_id
            .as_str();
        let context = NovNativeSealOverlayIngressContextV1 {
            local_execution_height: 1,
            local_validator_id: None,
            received_at_unix_ms: 2_000,
        };
        assert!(quarantine
            .ingest_authenticated_wire(&authority, wrong_peer, &proposal_wire, &context)
            .is_err());
        assert!(quarantine
            .load_admission(
                NovNativeSealOverlayArtifactKindV1::Proposal,
                proposal.proposal_hash,
            )
            .expect("load absent admission")
            .is_none());

        let admitted = quarantine
            .ingest_authenticated_wire(&authority, leader_peer, &proposal_wire, &context)
            .expect("admit proposal");
        assert!(!admitted.duplicate);
        assert_eq!(
            admitted.admission.state,
            NovNativeSealOverlayAdmissionStateV1::CryptoVerifiedAwaitingLocalExecution
        );
        assert!(receiver
            .store()
            .load_proposal(proposal.proposal_hash)
            .expect("load local proposal")
            .is_none());

        let replay = quarantine
            .ingest_authenticated_wire(
                &authority,
                leader_peer,
                &proposal_wire,
                &NovNativeSealOverlayIngressContextV1 {
                    received_at_unix_ms: 1_000,
                    ..context.clone()
                },
            )
            .expect("replay proposal");
        assert!(replay.duplicate);
        assert_eq!(replay.admission.receive_count, 2);
        assert_eq!(replay.admission.first_received_at_unix_ms, 2_000);
        assert_eq!(replay.admission.last_received_at_unix_ms, 2_000);
        drop(quarantine);

        let quarantine = NovNativeSealOverlayQuarantineV1::open(quarantine_path.as_path())
            .expect("reopen quarantine");
        assert!(!quarantine
            .bind_epoch_authority(
                receiver.ledger(),
                &receiver_authority,
                receiver_authority.authority_commitment,
            )
            .expect("recover pinned authority"));
        let proposal_result = quarantine
            .reconcile_proposal_with_local_execution(
                receiver.ledger(),
                receiver.store(),
                &receiver_authority,
                proposal.proposal_hash,
            )
            .expect("reconcile proposal");
        assert!(proposal_result.newly_persisted);
        assert_eq!(
            proposal_result.state,
            NovNativeSealOverlayAdmissionStateV1::LocallyMatchedVoteEligible
        );
        assert_eq!(
            receiver
                .store()
                .load_proposal(proposal.proposal_hash)
                .expect("load reconciled proposal"),
            Some(proposal.clone())
        );
        assert!(receiver
            .store()
            .load_pending_outbox(chain_id, proposal.proposer_id, 16)
            .expect("load remote proposal outbox")
            .is_empty());

        let qc_artifact = NovNativeSealOverlayArtifactV1::QuorumCertificate {
            proposal: Box::new(proposal.clone()),
            qc: Box::new(qc.clone()),
        };
        let qc_wire =
            encode_nov_native_seal_overlay_wire_v1(&qc_artifact, &authority).expect("QC wire");
        let qc_source = authority.transport_bindings[0].transport_peer_id.as_str();
        let qc_admitted = quarantine
            .ingest_authenticated_wire(
                &authority,
                qc_source,
                &qc_wire,
                &NovNativeSealOverlayIngressContextV1 {
                    received_at_unix_ms: 3_000,
                    ..context
                },
            )
            .expect("admit QC");
        assert_eq!(
            qc_admitted.admission.state,
            NovNativeSealOverlayAdmissionStateV1::QcCryptoVerifiedQuarantined
        );
        assert!(receiver
            .store()
            .load_qc(qc.qc_hash)
            .expect("load pre-reconcile QC")
            .is_none());
        let qc_result = quarantine
            .reconcile_qc_with_local_execution(
                receiver.ledger(),
                receiver.store(),
                &receiver_authority,
                qc.qc_hash,
            )
            .expect("reconcile QC");
        assert!(qc_result.newly_persisted);
        assert_eq!(
            receiver
                .store()
                .load_qc(qc.qc_hash)
                .expect("load reconciled QC"),
            Some(qc)
        );
        assert_candidate_remains_unsealed_v1(
            receiver.ledger(),
            chain_id,
            receiver_block.header.block_hash,
        );
    }

    #[test]
    fn competing_same_slot_proposals_block_both_sides_across_restart() {
        let chain_id = 82_103;
        let (first, first_genesis, keys, set, authority) =
            genesis_fixture_v1("conflict-first", chain_id);
        let (second, second_genesis, _second_keys, second_set, second_authority) =
            genesis_fixture_v1("conflict-second", chain_id);
        let (receiver, receiver_genesis, _receiver_keys, receiver_set, receiver_authority) =
            genesis_fixture_v1("conflict-receiver", chain_id);
        assert_eq!(first_genesis, second_genesis);
        assert_eq!(first_genesis, receiver_genesis);
        assert_eq!(set, second_set);
        assert_eq!(set, receiver_set);
        assert_eq!(authority, second_authority);
        assert_eq!(authority, receiver_authority);

        let parent_proposal =
            sign_proposal_v1(&first, &first_genesis, &keys, &set, &authority, None, 0);
        let parent_qc = NovNativeSealQuorumCertificateV1::from_votes(
            parent_proposal.subject.clone(),
            &set,
            votes_v1(&first, &parent_proposal, &keys, &set, 3),
        )
        .expect("parent QC");
        first
            .store()
            .persist_local_verified_qc(first.ledger(), &parent_qc, &set)
            .expect("persist first parent QC");
        for node in [&second, &receiver] {
            node.store()
                .persist_locally_matched_remote_proposal(node.ledger(), &parent_proposal, &set)
                .expect("persist common parent proposal");
            node.store()
                .persist_local_verified_qc(node.ledger(), &parent_qc, &set)
                .expect("persist common parent QC");
        }

        let first_child = commit_block_v1(first.ledger(), chain_id, 2, Some(&first_genesis), 0x41);
        let second_child =
            commit_block_v1(second.ledger(), chain_id, 2, Some(&second_genesis), 0x42);
        let receiver_child = commit_block_v1(
            receiver.ledger(),
            chain_id,
            2,
            Some(&receiver_genesis),
            0x41,
        );
        assert_eq!(first_child, receiver_child);
        assert_ne!(
            first_child.header.block_hash,
            second_child.header.block_hash
        );
        let first_proposal = sign_proposal_v1(
            &first,
            &first_child,
            &keys,
            &set,
            &authority,
            Some(parent_qc.qc_hash),
            0,
        );
        let second_proposal = sign_proposal_v1(
            &second,
            &second_child,
            &keys,
            &set,
            &second_authority,
            Some(parent_qc.qc_hash),
            0,
        );
        assert_eq!(first_proposal.proposer_id, second_proposal.proposer_id);
        assert_ne!(first_proposal.proposal_hash, second_proposal.proposal_hash);

        let quarantine_path = receiver.quarantine_path();
        let quarantine = NovNativeSealOverlayQuarantineV1::open(quarantine_path.as_path())
            .expect("open quarantine");
        quarantine
            .bind_epoch_authority(
                receiver.ledger(),
                &receiver_authority,
                receiver_authority.authority_commitment,
            )
            .expect("pin authority");
        let source_peer = authority
            .transport_peer_id(first_proposal.proposer_id)
            .expect("proposal source");
        let context = NovNativeSealOverlayIngressContextV1 {
            local_execution_height: 2,
            local_validator_id: Some(first_proposal.proposer_id),
            received_at_unix_ms: 4_000,
        };
        let first_wire = encode_nov_native_seal_overlay_wire_v1(
            &NovNativeSealOverlayArtifactV1::Proposal(Box::new(first_proposal.clone())),
            &authority,
        )
        .expect("first proposal wire");
        let second_wire = encode_nov_native_seal_overlay_wire_v1(
            &NovNativeSealOverlayArtifactV1::Proposal(Box::new(second_proposal.clone())),
            &authority,
        )
        .expect("second proposal wire");
        quarantine
            .ingest_authenticated_wire(&authority, source_peer, &first_wire, &context)
            .expect("admit first proposal");
        let conflict = quarantine
            .ingest_authenticated_wire(
                &authority,
                source_peer,
                &second_wire,
                &NovNativeSealOverlayIngressContextV1 {
                    received_at_unix_ms: 4_001,
                    ..context
                },
            )
            .expect("quarantine competing proposal");
        assert_eq!(
            conflict.admission.state,
            NovNativeSealOverlayAdmissionStateV1::EquivocationQuarantined
        );
        assert_eq!(conflict.admission.evidence_hashes.len(), 1);
        let evidence_hash = conflict.admission.evidence_hashes[0];
        let evidence = quarantine
            .load_equivocation_evidence(evidence_hash)
            .expect("load evidence")
            .expect("equivocation evidence");
        assert_eq!(evidence.evidence_hash, evidence_hash);
        let guard = quarantine
            .load_identity_guard(chain_id, 1, first_proposal.proposer_id)
            .expect("load identity guard")
            .expect("identity guard");
        assert!(guard.signing_blocked);
        assert_eq!(guard.evidence_hashes, vec![evidence_hash]);
        for proposal_hash in [first_proposal.proposal_hash, second_proposal.proposal_hash] {
            assert!(quarantine
                .reconcile_proposal_with_local_execution(
                    receiver.ledger(),
                    receiver.store(),
                    &receiver_authority,
                    proposal_hash,
                )
                .is_err());
            assert!(receiver
                .store()
                .load_proposal(proposal_hash)
                .expect("load blocked proposal")
                .is_none());
        }
        drop(quarantine);

        let quarantine = NovNativeSealOverlayQuarantineV1::open(quarantine_path.as_path())
            .expect("reopen quarantine");
        assert!(quarantine
            .load_identity_guard(chain_id, 1, first_proposal.proposer_id)
            .expect("reload identity guard")
            .is_some());
        for proposal_hash in [first_proposal.proposal_hash, second_proposal.proposal_hash] {
            assert!(quarantine
                .reconcile_proposal_with_local_execution(
                    receiver.ledger(),
                    receiver.store(),
                    &receiver_authority,
                    proposal_hash,
                )
                .is_err());
        }
        assert_candidate_remains_unsealed_v1(
            receiver.ledger(),
            chain_id,
            receiver_child.header.block_hash,
        );
    }

    #[test]
    fn authority_and_ingress_bounds_reject_oversized_or_stale_domains() {
        let chain_id = 82_104;
        let (node, block, keys, set, authority) = genesis_fixture_v1("bounds", chain_id);
        let proposal = sign_proposal_v1(&node, &block, &keys, &set, &authority, None, 0);
        let wire = encode_nov_native_seal_overlay_wire_v1(
            &NovNativeSealOverlayArtifactV1::Proposal(Box::new(proposal.clone())),
            &authority,
        )
        .expect("proposal wire");
        let quarantine = NovNativeSealOverlayQuarantineV1::open(node.quarantine_path().as_path())
            .expect("open quarantine");
        quarantine
            .bind_epoch_authority(node.ledger(), &authority, authority.authority_commitment)
            .expect("pin authority");
        assert!(quarantine
            .ingest_authenticated_wire(
                &authority,
                authority
                    .transport_peer_id(proposal.proposer_id)
                    .expect("proposal peer"),
                &wire,
                &NovNativeSealOverlayIngressContextV1 {
                    local_execution_height: 200,
                    local_validator_id: None,
                    received_at_unix_ms: 1,
                },
            )
            .is_err());

        let large_keys = (10u8..75)
            .map(|seed| SigningKey::from_bytes(&[seed; 32]))
            .collect::<Vec<_>>();
        let large_set = NovNativeSealValidatorSetV1::new(
            chain_id,
            1,
            1,
            large_keys
                .iter()
                .map(|key| {
                    NovNativeSealValidatorV1::new(*key.verifying_key().as_bytes(), 1)
                        .expect("large-set validator")
                })
                .collect(),
        )
        .expect("65-validator seal set");
        assert_eq!(large_set.validators.len(), 65);
        let bindings = large_set
            .validators
            .iter()
            .map(|validator| NovNativeSealValidatorTransportBindingV1 {
                validator_id: validator.validator_id,
                transport_peer_id: peer_id_from_ed25519_public_key_v1(&validator.public_key),
            })
            .collect();
        assert!(
            NovNativeSealEpochAuthorityV1::derive_operator_pinned_genesis_epoch(
                node.ledger(),
                large_set,
                bindings,
            )
            .is_err()
        );
    }
}
