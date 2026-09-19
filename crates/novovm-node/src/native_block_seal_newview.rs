//! Signed new-view observations over the current height's prepare QC.
//!
//! This is evidence, not an unlock certificate, finality proof, or network
//! admission rule. A missing highest QC does not imply that no candidate lock
//! exists. Existing height locks and the Overlay round-zero limit stay intact.
//! The current-height highest QC is distinct from a proposal's parent-height
//! `justify_qc_hash`, including at genesis where the latter must remain zero.

use super::timeout::{NovNativeSealTimeoutCertificateV1, NovNativeSealTimeoutContextV1};
use super::*;
use crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1;
use std::collections::{BTreeMap, BTreeSet};

#[path = "native_block_seal_newview_admission.rs"]
mod admission;
#[path = "native_block_seal_newview_store.rs"]
mod store;
pub use admission::NovNativeSealNewViewAdmissionV1;

pub(super) const OBSERVATION_SCHEMA_V1: &str = "novovm-native-seal-new-view-observation/v1";
pub(super) const CERTIFICATE_SCHEMA_V1: &str = "novovm-native-seal-new-view-certificate/v1";
const OBSERVATION_DOMAIN_V1: &[u8] = b"novovm-native-seal-new-view-observation-v1\0";

pub(super) fn validate_context_v1(
    expected: &NovNativeSealTimeoutContextV1,
    authority: &NovNativeSealEpochAuthorityV1,
) -> Result<()> {
    authority.validate()?;
    if expected.chain_id != authority.chain_id
        || expected.genesis_block_hash != authority.genesis_block_hash
        || expected.protocol_config_commitment != authority.protocol_config_commitment
        || expected.epoch != authority.epoch
        || expected.validator_set_hash != authority.validator_set.validator_set_hash
        || expected.height < authority.activation_height
        || expected.round == 0
        || expected.round == u64::MAX
    {
        bail!("native new-view context is outside the pinned authority domain");
    }
    // Reject a target for which the authority schedule would overflow, even
    // though this contract does not authorize that leader to emit a proposal.
    authority.scheduled_leader_v1(expected.height, expected.round)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeSealNewViewQcV1 {
    pub proposal: NovNativeSealProposalV1,
    pub qc: NovNativeSealQuorumCertificateV1,
}

impl NovNativeSealNewViewQcV1 {
    pub fn verify(
        &self,
        expected: &NovNativeSealTimeoutContextV1,
        authority: &NovNativeSealEpochAuthorityV1,
    ) -> Result<()> {
        validate_context_v1(expected, authority)?;
        let set = &authority.validator_set;
        self.proposal.verify(set)?;
        self.qc.verify(set)?;
        let subject = &self.qc.subject;
        if self.proposal.subject != *subject
            || self.proposal.proposal_hash != self.qc.proposal_hash
            || subject.chain_id != expected.chain_id
            || subject.genesis_block_hash != expected.genesis_block_hash
            || subject.protocol_config_commitment != expected.protocol_config_commitment
            || subject.epoch != expected.epoch
            || subject.validator_set_hash != expected.validator_set_hash
            || subject.height != expected.height
            || subject.round >= expected.round
        {
            bail!("native new-view QC does not bind the current height and an earlier round");
        }
        if self.proposal.proposer_id
            != authority.scheduled_leader_v1(subject.height, subject.round)?
        {
            bail!("native new-view QC proposal signer is not the scheduled leader");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeSealNewViewObservationV1 {
    pub schema: String,
    pub authority_commitment: [u8; 32],
    pub context: NovNativeSealTimeoutContextV1,
    pub highest_qc: Option<NovNativeSealNewViewQcV1>,
    pub validator_id: [u8; 32],
    pub signature: Vec<u8>,
}

impl NovNativeSealNewViewObservationV1 {
    pub fn verify(
        &self,
        expected: &NovNativeSealTimeoutContextV1,
        authority: &NovNativeSealEpochAuthorityV1,
    ) -> Result<()> {
        validate_context_v1(expected, authority)?;
        if self.schema != OBSERVATION_SCHEMA_V1
            || self.authority_commitment != authority.authority_commitment
            || &self.context != expected
        {
            bail!("native new-view observation domain mismatch");
        }
        if let Some(evidence) = &self.highest_qc {
            evidence.verify(expected, authority)?;
        }
        let validator = authority
            .validator_set
            .validator(self.validator_id)
            .context("native new-view signer is not a validator")?;
        VerifyingKey::from_bytes(&validator.public_key)?
            .verify_strict(&self.message(), &Signature::from_slice(&self.signature)?)
            .context("invalid native new-view signature")
    }

    /// Binds the target context, not a particular TC vote subset. The certificate
    /// verifier separately checks a preceding TC for this exact target context.
    pub(super) fn message(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(OBSERVATION_DOMAIN_V1);
        hasher.update(self.authority_commitment);
        hasher.update(self.context.chain_id.to_be_bytes());
        hasher.update(self.context.genesis_block_hash);
        hasher.update(self.context.protocol_config_commitment);
        hasher.update(self.context.epoch.to_be_bytes());
        hasher.update(self.context.validator_set_hash);
        hasher.update(self.context.height.to_be_bytes());
        hasher.update(self.context.round.to_be_bytes());
        hasher.update(self.validator_id);
        match &self.highest_qc {
            None => hasher.update([0u8]),
            Some(evidence) => {
                hasher.update([1u8]);
                hasher.update(evidence.qc.qc_hash);
                hasher.update(evidence.proposal.proposal_hash);
            }
        }
        hasher.finalize().into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeSealNewViewCertificateV1 {
    pub schema: String,
    pub authority_commitment: [u8; 32],
    pub context: NovNativeSealTimeoutContextV1,
    pub previous_timeout: NovNativeSealTimeoutCertificateV1,
    pub observations: Vec<NovNativeSealNewViewObservationV1>,
}

impl NovNativeSealNewViewCertificateV1 {
    /// Returns the highest prepare QC reported by this quorum only. This is not
    /// a claim that no higher QC exists elsewhere, and does not release a lock.
    pub fn verify(
        &self,
        expected: &NovNativeSealTimeoutContextV1,
        authority: &NovNativeSealEpochAuthorityV1,
    ) -> Result<Option<NovNativeSealNewViewQcV1>> {
        validate_context_v1(expected, authority)?;
        let set = &authority.validator_set;
        if self.schema != CERTIFICATE_SCHEMA_V1
            || self.authority_commitment != authority.authority_commitment
            || &self.context != expected
            || self.observations.is_empty()
            || self.observations.len() > set.validators.len()
        {
            bail!("native new-view certificate domain or size mismatch");
        }
        let mut preceding = expected.clone();
        preceding.round -= 1;
        self.previous_timeout.verify(&preceding, set)?;
        let mut signers = BTreeSet::new();
        let mut weight = 0u64;
        let mut evidence = Vec::new();
        for observation in &self.observations {
            if !signers.insert(observation.validator_id) {
                bail!("native new-view certificate contains a duplicate validator");
            }
            observation.verify(expected, authority)?;
            let validator = set
                .validator(observation.validator_id)
                .context("native new-view signer is not a validator")?;
            weight = weight
                .checked_add(validator.weight)
                .context("native new-view weight overflow")?;
            if let Some(qc) = &observation.highest_qc {
                evidence.push(qc.clone());
            }
        }
        if weight < set.quorum_weight {
            bail!("native new-view certificate has insufficient signed weight");
        }
        select_highest_v1(&evidence)
    }
}

/// Select only among already cryptographically verified, context-bound evidence.
/// This version conservatively refuses cross-block QCs even across rounds: it
/// has no lock-migration proof and must not choose a competing block by rank.
pub(super) fn select_highest_v1(
    evidence: &[NovNativeSealNewViewQcV1],
) -> Result<Option<NovNativeSealNewViewQcV1>> {
    let Some(first) = evidence.first() else {
        return Ok(None);
    };
    let domain = &first.qc.subject;
    let mut immutable_subject = domain.clone();
    immutable_subject.round = 0;
    immutable_subject.subject_hash = [0; 32];
    let mut subjects_by_round = BTreeMap::new();
    let mut selected = first;
    for item in evidence {
        let subject = &item.qc.subject;
        if subject.chain_id != domain.chain_id
            || subject.genesis_block_hash != domain.genesis_block_hash
            || subject.protocol_config_commitment != domain.protocol_config_commitment
            || subject.epoch != domain.epoch
            || subject.validator_set_hash != domain.validator_set_hash
            || subject.height != domain.height
        {
            bail!("native new-view highest QC inputs mix height or authority domains");
        }
        if subject.block_hash != domain.block_hash {
            bail!("native new-view contains competing block QCs; lock migration is unsupported");
        }
        // A block hash in remote QC evidence is a signed claim, not a local
        // reconstruction of the block body. Do not let a later round hide
        // contradictory execution, receipt, DA, or parent commitments under
        // the same claimed block hash. This version also conservatively pins
        // the parent justify QC; it has no proof for changing that binding.
        let mut candidate_subject = subject.clone();
        candidate_subject.round = 0;
        candidate_subject.subject_hash = [0; 32];
        if candidate_subject != immutable_subject {
            bail!("native new-view same-block QCs contain inconsistent immutable commitments");
        }
        if subjects_by_round
            .insert(subject.round, subject.subject_hash)
            .is_some_and(|previous| previous != subject.subject_hash)
        {
            bail!("native new-view contains competing subjects in the same round");
        }
        if subject.round > selected.qc.subject.round
            || (subject.round == selected.qc.subject.round && item.qc.qc_hash < selected.qc.qc_hash)
        {
            selected = item;
        }
    }
    Ok(Some(selected.clone()))
}
