//! Experimental decision attestations and explicit local durable signing.
//! No network activation, unlock or finality API.
use super::*;
#[path = "native_block_seal_commit_v3_collector.rs"]
pub mod collector;
#[path = "native_block_seal_commit_v3_sender.rs"]
pub mod sender;
#[path = "native_block_seal_commit_v3_store.rs"]
mod store;

const VOTE_SCHEMA: &str = "novovm-native-seal-decision-vote/v3";
const CERT_SCHEMA: &str = "novovm-native-seal-decision-certificate/v3";
const SIGN_DOMAIN: &[u8] = b"novovm-native-seal-decision-signing-v3\0";

/// Bind every V1 execution subject field except its voting round. In particular,
/// justify_qc_hash is a parent dependency, NOT expendable round-change evidence.
/// A valid QC is a cryptographic witness, not proof of local execution, DA,
/// scheduled leadership, new-view admission or a finalized ancestor.
pub fn decision_target_v3(
    prepare: &NovNativeSealQuorumCertificateV1,
    set: &NovNativeSealValidatorSetV1,
) -> Result<[u8; 32]> {
    prepare.verify(set)?;
    let mut decision = prepare.subject.clone();
    decision.round = 0;
    decision.subject_hash = subject_hash_v1(&decision);
    decision.validate(set)?;
    let mut hash = Sha256::new();
    hash.update(b"novovm-native-seal-decision-target-v3\0");
    hash.update(decision.subject_hash);
    Ok(hash.finalize().into())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeSealDecisionVoteV3 {
    pub schema: String,
    pub target_hash: [u8; 32],
    pub validator_id: [u8; 32],
    pub signature: Vec<u8>,
    pub vote_hash: [u8; 32],
}

impl NovNativeSealDecisionVoteV3 {
    pub fn verify(
        &self,
        prepare: &NovNativeSealQuorumCertificateV1,
        set: &NovNativeSealValidatorSetV1,
    ) -> Result<()> {
        self.verify_target(decision_target_v3(prepare, set)?, set)
    }

    fn verify_target(&self, target: [u8; 32], set: &NovNativeSealValidatorSetV1) -> Result<()> {
        if self.schema != VOTE_SCHEMA || self.target_hash != target {
            bail!("decision v3 schema or target mismatch");
        }
        let member = set
            .validator(self.validator_id)
            .context("unknown decision v3 signer")?;
        VerifyingKey::from_bytes(&member.public_key)?
            .verify_strict(&self.message(), &Signature::from_slice(&self.signature)?)?;
        if self.vote_hash != self.hash() {
            bail!("decision v3 vote hash mismatch");
        }
        Ok(())
    }

    fn message(&self) -> Vec<u8> {
        [SIGN_DOMAIN, &self.target_hash, &self.validator_id].concat()
    }

    fn hash(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"novovm-native-seal-decision-vote-v3\0");
        hash.update(self.target_hash);
        hash.update(self.validator_id);
        hash.update(&self.signature);
        hash.finalize().into()
    }
}

/// A quorum of V3 decision signatures; never reinterpret V1/V2 confirmations.
/// The witness round is not a claim that every decision signer voted in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeSealDecisionCertificateV3 {
    pub schema: String,
    pub prepare: NovNativeSealQuorumCertificateV1,
    pub votes: Vec<NovNativeSealDecisionVoteV3>,
    pub signed_weight: u64,
    pub certificate_hash: [u8; 32],
}

impl NovNativeSealDecisionCertificateV3 {
    pub fn from_votes(
        prepare: NovNativeSealQuorumCertificateV1,
        set: &NovNativeSealValidatorSetV1,
        mut votes: Vec<NovNativeSealDecisionVoteV3>,
    ) -> Result<Self> {
        votes.sort_by_key(|vote| vote.validator_id);
        let mut result = Self {
            schema: CERT_SCHEMA.into(),
            prepare,
            votes,
            signed_weight: 0,
            certificate_hash: [0; 32],
        };
        result.signed_weight = result.checked_weight(set)?;
        result.certificate_hash = result.hash();
        Ok(result)
    }

    pub fn verify(&self, set: &NovNativeSealValidatorSetV1) -> Result<()> {
        if self.schema != CERT_SCHEMA
            || self.signed_weight != self.checked_weight(set)?
            || self.certificate_hash != self.hash()
        {
            bail!("decision v3 certificate metadata or hash mismatch");
        }
        Ok(())
    }

    fn checked_weight(&self, set: &NovNativeSealValidatorSetV1) -> Result<u64> {
        let target = decision_target_v3(&self.prepare, set)?;
        if self.votes.is_empty() || self.votes.len() > set.validators.len() {
            bail!("invalid decision v3 vote count");
        }
        let mut previous = None;
        let mut weight = 0u64;
        for vote in &self.votes {
            vote.verify_target(target, set)?;
            if previous.is_some_and(|id| id >= vote.validator_id) {
                bail!("decision v3 signers must be sorted and unique");
            }
            previous = Some(vote.validator_id);
            weight = weight
                .checked_add(
                    set.validator(vote.validator_id)
                        .context("unknown decision v3 signer")?
                        .weight,
                )
                .context("decision v3 weight overflow")?;
        }
        if weight < set.quorum_weight {
            bail!("insufficient decision v3 quorum weight");
        }
        Ok(weight)
    }

    fn hash(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"novovm-native-seal-decision-certificate-v3\0");
        hash.update(self.prepare.qc_hash);
        hash.update(self.signed_weight.to_le_bytes());
        hash.update((self.votes.len() as u64).to_le_bytes());
        for vote in &self.votes {
            hash.update(vote.vote_hash);
        }
        hash.finalize().into()
    }
}
