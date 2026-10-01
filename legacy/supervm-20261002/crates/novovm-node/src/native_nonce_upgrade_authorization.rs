#![forbid(unsafe_code)]

//! Quorum attestation for an isolated nonce-upgrade transition. This is not a
//! block seal, source-checkpoint finality, an AOEM import, or runtime activation.
//! The caller supplies an independently pinned old epoch authority. Production
//! signing must additionally use the durable sibling signer; the unfenced
//! primitive below is deliberately not public outside the ingress module.

use super::native_nonce_bundle::verified_nonce_checkpoint_inputs_v1;
use super::native_nonce_checkpoint::NonceMigrationCheckpointV1;
use super::native_nonce_upgrade::plan_nonce_upgrade_v1;
use crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1;
use anyhow::{bail, Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_NONCE_UPGRADE_AUTHORIZATION_BYTES_V1: usize = 256 * 1024;
const SUBJECT_SCHEMA: &str = "novovm-native-nonce-upgrade-authorization-subject/v1";
const VOTE_SCHEMA: &str = "novovm-native-nonce-upgrade-authorization-vote/v1";
const CERTIFICATE_SCHEMA: &str = "novovm-native-nonce-upgrade-authorization-certificate/v1";
const SUBJECT_DOMAIN: &[u8] = b"novovm-native-nonce-upgrade-authorization-subject-v1\0";
const SIGNING_DOMAIN: &[u8] = b"novovm-native-nonce-upgrade-authorization-vote-signing-v1\0";
const VOTE_DOMAIN: &[u8] = b"novovm-native-nonce-upgrade-authorization-vote-hash-v1\0";
const CERTIFICATE_DOMAIN: &[u8] = b"novovm-native-nonce-upgrade-authorization-certificate-v1\0";

pub struct NonceUpgradeAuthorizationInputsV1<'a> {
    pub bundle: &'a [u8],
    pub bundle_digest: &'a str,
    pub checkpoint: &'a NonceMigrationCheckpointV1,
    pub target_protocol: &'a str,
    pub authority: &'a NovNativeSealEpochAuthorityV1,
    pub expected_authority_commitment: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NonceUpgradeAuthorizationSubjectV1 {
    pub schema: String,
    pub chain_id: u64,
    pub genesis_block_hash: [u8; 32],
    pub namespace_digest: [u8; 32],
    pub epoch: u64,
    pub checkpoint_height: u64,
    pub activation_height: u64,
    pub checkpoint_block_hash: [u8; 32],
    pub authority_commitment: [u8; 32],
    pub validator_set_hash: [u8; 32],
    pub old_protocol_config_commitment: [u8; 32],
    pub target_protocol_config_commitment: [u8; 32],
    pub transition_id: [u8; 32],
    pub source_bundle_digest: [u8; 32],
    pub snapshot_digest: [u8; 32],
    pub ordered_history_commitment: [u8; 32],
    pub old_state_root: [u8; 32],
    pub proposed_state_root: [u8; 32],
    pub receipt_root: [u8; 32],
    pub state_version: u64,
    pub subject_hash: [u8; 32],
}

/// Only the complete source/transition/authority verifier constructs this token.
/// In particular, deserializing an arbitrary subject never grants signing rights.
#[derive(Debug, Clone)]
pub struct VerifiedNonceUpgradeAuthorizationV1 {
    subject: NonceUpgradeAuthorizationSubjectV1,
    authority: NovNativeSealEpochAuthorityV1,
}

impl VerifiedNonceUpgradeAuthorizationV1 {
    pub fn subject(&self) -> &NonceUpgradeAuthorizationSubjectV1 {
        &self.subject
    }

    pub fn authority(&self) -> &NovNativeSealEpochAuthorityV1 {
        &self.authority
    }
}

fn hash_parts(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(domain);
    for part in parts {
        hash.update(part);
    }
    hash.finalize().into()
}

fn parse_commitment(value: &str, label: &str) -> Result<[u8; 32]> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("nonce upgrade authorization {label} must be canonical lowercase 32-byte hex");
    }
    let mut parsed = [0u8; 32];
    for (index, byte) in parsed.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .context("decode nonce upgrade authorization commitment")?;
    }
    if parsed == [0; 32] {
        bail!("nonce upgrade authorization {label} must be nonzero");
    }
    Ok(parsed)
}

fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn subject_hash(subject: &NonceUpgradeAuthorizationSubjectV1) -> Result<[u8; 32]> {
    let mut unsigned = subject.clone();
    unsigned.subject_hash = [0; 32];
    let encoded = serde_json::to_vec(&unsigned)?;
    Ok(hash_parts(SUBJECT_DOMAIN, &[&encoded]))
}

pub fn prepare_nonce_upgrade_authorization_v1(
    inputs: &NonceUpgradeAuthorizationInputsV1<'_>,
) -> Result<VerifiedNonceUpgradeAuthorizationV1> {
    let expected_authority =
        parse_commitment(inputs.expected_authority_commitment, "authority pin")?;
    inputs.authority.validate()?;
    if inputs.authority.authority_commitment != expected_authority {
        bail!("nonce upgrade authorization authority differs from independent pin");
    }
    let transition = plan_nonce_upgrade_v1(
        inputs.bundle,
        inputs.bundle_digest,
        inputs.checkpoint,
        inputs.target_protocol,
    )?;
    let (_, head, _, genesis_block_hash) =
        verified_nonce_checkpoint_inputs_v1(inputs.bundle, inputs.checkpoint)?;
    let old_protocol = parse_commitment(
        &inputs.checkpoint.legacy_protocol_config_commitment,
        "old protocol commitment",
    )?;
    if inputs.authority.chain_id != inputs.checkpoint.chain_id
        || inputs.authority.genesis_block_hash != genesis_block_hash
        || inputs.authority.protocol_config_commitment != old_protocol
        || head.height < inputs.authority.activation_height
    {
        bail!("nonce upgrade authorization old authority does not bind verified checkpoint chain, genesis, protocol, or height");
    }
    let activation_height = head
        .height
        .checked_add(1)
        .context("nonce upgrade authorization activation height overflow")?;
    let mut subject = NonceUpgradeAuthorizationSubjectV1 {
        schema: SUBJECT_SCHEMA.to_string(),
        chain_id: inputs.checkpoint.chain_id,
        genesis_block_hash,
        namespace_digest: parse_commitment(&inputs.checkpoint.namespace_digest, "namespace")?,
        epoch: inputs.authority.epoch,
        checkpoint_height: head.height,
        activation_height,
        checkpoint_block_hash: parse_commitment(
            &inputs.checkpoint.tip_block_hash,
            "checkpoint block",
        )?,
        authority_commitment: expected_authority,
        validator_set_hash: inputs.authority.validator_set.validator_set_hash,
        old_protocol_config_commitment: old_protocol,
        target_protocol_config_commitment: parse_commitment(
            inputs.target_protocol,
            "target protocol",
        )?,
        transition_id: parse_commitment(&transition.transition_id, "transition id")?,
        source_bundle_digest: parse_commitment(inputs.bundle_digest, "bundle digest")?,
        snapshot_digest: parse_commitment(&inputs.checkpoint.snapshot_digest, "snapshot digest")?,
        ordered_history_commitment: parse_commitment(
            &transition.ordered_history_commitment,
            "ordered history",
        )?,
        old_state_root: parse_commitment(&transition.legacy_state_root, "old state root")?,
        proposed_state_root: parse_commitment(
            &transition.proposed_state_root,
            "proposed state root",
        )?,
        receipt_root: parse_commitment(&transition.receipt_root, "receipt root")?,
        state_version: transition.state_version,
        subject_hash: [0; 32],
    };
    subject.subject_hash = subject_hash(&subject)?;
    Ok(VerifiedNonceUpgradeAuthorizationV1 {
        subject,
        authority: inputs.authority.clone(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NonceUpgradeAuthorizationVoteV1 {
    pub schema: String,
    pub subject_hash: [u8; 32],
    pub validator_id: [u8; 32],
    pub signature: Vec<u8>,
    pub vote_hash: [u8; 32],
}

fn signing_message(vote: &NonceUpgradeAuthorizationVoteV1) -> [u8; 32] {
    hash_parts(SIGNING_DOMAIN, &[&vote.subject_hash, &vote.validator_id])
}

fn vote_hash(vote: &NonceUpgradeAuthorizationVoteV1) -> [u8; 32] {
    hash_parts(VOTE_DOMAIN, &[&signing_message(vote), &vote.signature])
}

impl NonceUpgradeAuthorizationVoteV1 {
    pub fn verify(&self, verified: &VerifiedNonceUpgradeAuthorizationV1) -> Result<()> {
        if self.schema != VOTE_SCHEMA
            || self.subject_hash != verified.subject.subject_hash
            || self.signature.len() != 64
            || self.vote_hash != vote_hash(self)
        {
            bail!("nonce upgrade authorization vote metadata or commitment mismatch");
        }
        let validator = verified
            .authority
            .validator_set
            .validator(self.validator_id)
            .context("nonce upgrade authorization voter is not an old-authority validator")?;
        let key = VerifyingKey::from_bytes(&validator.public_key)?;
        let signature = Signature::from_slice(&self.signature)?;
        key.verify_strict(&signing_message(self), &signature)
            .context("nonce upgrade authorization vote signature failed")?;
        Ok(())
    }
}

/// The durable sibling signer must lock this upgrade boundary before releasing
/// this signature. It is intentionally inaccessible to library consumers.
pub(super) fn sign_nonce_upgrade_vote_unfenced_v1(
    verified: &VerifiedNonceUpgradeAuthorizationV1,
    key: &SigningKey,
) -> Result<NonceUpgradeAuthorizationVoteV1> {
    let public_key = key.verifying_key().to_bytes();
    let validator = verified
        .authority
        .validator_set
        .validators
        .iter()
        .find(|validator| validator.public_key == public_key)
        .context("nonce upgrade authorization signing key is not in the old authority")?;
    let mut vote = NonceUpgradeAuthorizationVoteV1 {
        schema: VOTE_SCHEMA.to_string(),
        subject_hash: verified.subject.subject_hash,
        validator_id: validator.validator_id,
        signature: Vec::new(),
        vote_hash: [0; 32],
    };
    vote.signature = key.sign(&signing_message(&vote)).to_bytes().to_vec();
    vote.vote_hash = vote_hash(&vote);
    vote.verify(verified)?;
    Ok(vote)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NonceUpgradeAuthorizationCertificateV1 {
    pub schema: String,
    pub subject: NonceUpgradeAuthorizationSubjectV1,
    pub votes: Vec<NonceUpgradeAuthorizationVoteV1>,
    pub signature_count: u32,
    pub signed_weight: u64,
    pub quorum_weight: u64,
    pub certificate_hash: [u8; 32],
}

fn certificate_hash(certificate: &NonceUpgradeAuthorizationCertificateV1) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(CERTIFICATE_DOMAIN);
    hash.update(certificate.subject.subject_hash);
    hash.update(certificate.signature_count.to_be_bytes());
    hash.update(certificate.signed_weight.to_be_bytes());
    hash.update(certificate.quorum_weight.to_be_bytes());
    for vote in &certificate.votes {
        hash.update(vote.vote_hash);
        hash.update(vote.validator_id);
    }
    hash.finalize().into()
}

fn verified_weight(
    votes: &[NonceUpgradeAuthorizationVoteV1],
    verified: &VerifiedNonceUpgradeAuthorizationV1,
) -> Result<u64> {
    if votes.is_empty() || votes.len() > verified.authority.validator_set.validators.len() {
        bail!("nonce upgrade authorization vote count is outside authority bounds");
    }
    let mut previous = None;
    let mut weight = 0u64;
    for vote in votes {
        if previous.is_some_and(|id| id >= vote.validator_id) {
            bail!("nonce upgrade authorization votes must be strictly sorted and unique");
        }
        vote.verify(verified)?;
        previous = Some(vote.validator_id);
        weight = weight
            .checked_add(
                verified
                    .authority
                    .validator_set
                    .validator(vote.validator_id)
                    .context("nonce upgrade authorization voter missing")?
                    .weight,
            )
            .context("nonce upgrade authorization signed weight overflow")?;
    }
    if weight < verified.authority.validator_set.quorum_weight {
        bail!("nonce upgrade authorization quorum has insufficient signed weight");
    }
    Ok(weight)
}

impl NonceUpgradeAuthorizationCertificateV1 {
    pub fn from_votes(
        verified: &VerifiedNonceUpgradeAuthorizationV1,
        mut votes: Vec<NonceUpgradeAuthorizationVoteV1>,
    ) -> Result<Self> {
        if votes.len() > verified.authority.validator_set.validators.len() {
            bail!("nonce upgrade authorization vote count exceeds authority bounds");
        }
        votes.sort_by_key(|vote| vote.validator_id);
        let signed_weight = verified_weight(&votes, verified)?;
        let mut certificate = Self {
            schema: CERTIFICATE_SCHEMA.to_string(),
            subject: verified.subject.clone(),
            signature_count: u32::try_from(votes.len())?,
            votes,
            signed_weight,
            quorum_weight: verified.authority.validator_set.quorum_weight,
            certificate_hash: [0; 32],
        };
        certificate.certificate_hash = certificate_hash(&certificate);
        certificate.verify(verified)?;
        Ok(certificate)
    }

    pub fn verify(&self, verified: &VerifiedNonceUpgradeAuthorizationV1) -> Result<()> {
        if self.schema != CERTIFICATE_SCHEMA
            || self.subject != verified.subject
            || self.signature_count as usize != self.votes.len()
            || self.quorum_weight != verified.authority.validator_set.quorum_weight
        {
            bail!("nonce upgrade authorization certificate does not bind verified transition and authority");
        }
        let observed_weight = verified_weight(&self.votes, verified)?;
        if self.signed_weight != observed_weight || self.certificate_hash != certificate_hash(self)
        {
            bail!("nonce upgrade authorization certificate weight or hash mismatch");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NonceUpgradeAuthorizationReportV1 {
    pub schema: &'static str,
    pub subject_hash: String,
    pub certificate_hash: String,
    pub signature_count: u32,
    pub signed_weight: u64,
    pub quorum_weight: u64,
    pub quorum_verified: bool,
    pub activation_ready: bool,
    pub authority_state_published: bool,
    pub import_performed: bool,
    pub chain_canonical: bool,
    pub proof_sealed: bool,
    pub safe: bool,
    pub finalized: bool,
    pub aoem_evidence_verified: bool,
    pub independent_provenance_verified: bool,
    pub target_runtime_compatibility_verified: bool,
    pub execution_replayed: bool,
}

pub fn verify_nonce_upgrade_authorization_json_v1(
    bytes: &[u8],
    inputs: &NonceUpgradeAuthorizationInputsV1<'_>,
) -> Result<NonceUpgradeAuthorizationReportV1> {
    if bytes.is_empty() || bytes.len() > MAX_NONCE_UPGRADE_AUTHORIZATION_BYTES_V1 {
        bail!("nonce upgrade authorization certificate exceeds nonempty 256 KiB bound");
    }
    let certificate: NonceUpgradeAuthorizationCertificateV1 =
        serde_json::from_slice(bytes).context("decode nonce upgrade authorization certificate")?;
    let verified = prepare_nonce_upgrade_authorization_v1(inputs)?;
    certificate.verify(&verified)?;
    Ok(NonceUpgradeAuthorizationReportV1 {
        schema: "novovm-native-nonce-upgrade-authorization-report/v1",
        subject_hash: hex(&certificate.subject.subject_hash),
        certificate_hash: hex(&certificate.certificate_hash),
        signature_count: certificate.signature_count,
        signed_weight: certificate.signed_weight,
        quorum_weight: certificate.quorum_weight,
        quorum_verified: true,
        activation_ready: false,
        authority_state_published: false,
        import_performed: false,
        chain_canonical: false,
        proof_sealed: false,
        safe: false,
        finalized: false,
        aoem_evidence_verified: false,
        independent_provenance_verified: false,
        target_runtime_compatibility_verified: false,
        execution_replayed: false,
    })
}

#[cfg(test)]
pub(crate) use tests::native_nonce_upgrade_authorization_fixture_v1;

#[cfg(test)]
mod tests {
    use super::super::native_nonce_bundle::{checkpoint_bundle_digest_v1, encode_bundle};
    use super::super::native_nonce_checkpoint::test_fixture_v1;
    use super::*;
    use crate::native_block_ledger::NovNativeBlockLedgerV1;
    use crate::native_block_seal::{NovNativeSealValidatorSetV1, NovNativeSealValidatorV1};
    use crate::native_block_seal_overlay::NovNativeSealValidatorTransportBindingV1;

    const TARGET: &str = "efefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefef";

    pub(crate) struct AuthorizationFixtureV1 {
        pub(crate) bundle: Vec<u8>,
        pub(crate) checkpoint: NonceMigrationCheckpointV1,
        pub(crate) authority: NovNativeSealEpochAuthorityV1,
        pub(crate) keys: Vec<SigningKey>,
    }

    impl AuthorizationFixtureV1 {
        pub(crate) fn prepare(&self, target: &str) -> VerifiedNonceUpgradeAuthorizationV1 {
            prepare_nonce_upgrade_authorization_v1(&NonceUpgradeAuthorizationInputsV1 {
                bundle: &self.bundle,
                bundle_digest: &checkpoint_bundle_digest_v1(&self.bundle),
                checkpoint: &self.checkpoint,
                target_protocol: target,
                authority: &self.authority,
                expected_authority_commitment: &hex(&self.authority.authority_commitment),
            })
            .unwrap()
        }

        fn certificate(
            &self,
            verified: &VerifiedNonceUpgradeAuthorizationV1,
        ) -> NonceUpgradeAuthorizationCertificateV1 {
            NonceUpgradeAuthorizationCertificateV1::from_votes(
                verified,
                self.keys
                    .iter()
                    .take(3)
                    .map(|key| sign_nonce_upgrade_vote_unfenced_v1(verified, key).unwrap())
                    .collect(),
            )
            .unwrap()
        }

        fn verify_json(&self, bytes: &[u8]) -> Result<NonceUpgradeAuthorizationReportV1> {
            verify_nonce_upgrade_authorization_json_v1(
                bytes,
                &NonceUpgradeAuthorizationInputsV1 {
                    bundle: &self.bundle,
                    bundle_digest: &checkpoint_bundle_digest_v1(&self.bundle),
                    checkpoint: &self.checkpoint,
                    target_protocol: TARGET,
                    authority: &self.authority,
                    expected_authority_commitment: &hex(&self.authority.authority_commitment),
                },
            )
        }
    }

    pub(crate) fn native_nonce_upgrade_authorization_fixture_v1() -> AuthorizationFixtureV1 {
        fixture_with_weights(&[1, 1, 1, 1])
    }

    fn fixture_with_weights(weights: &[u64; 4]) -> AuthorizationFixtureV1 {
        let (snapshot, head, blocks, checkpoint, path) = test_fixture_v1();
        let bundle = encode_bundle(&snapshot, &head, &blocks).unwrap();
        let keys = (1..=4)
            .map(|seed| SigningKey::from_bytes(&[seed; 32]))
            .collect::<Vec<_>>();
        let validators = keys
            .iter()
            .zip(weights)
            .map(|(key, weight)| {
                NovNativeSealValidatorV1::new(key.verifying_key().to_bytes(), *weight).unwrap()
            })
            .collect::<Vec<_>>();
        let bindings = validators
            .iter()
            .enumerate()
            .map(
                |(index, validator)| NovNativeSealValidatorTransportBindingV1 {
                    validator_id: validator.validator_id,
                    transport_peer_id: format!("{:02x}", index + 17).repeat(32),
                },
            )
            .collect();
        let set = NovNativeSealValidatorSetV1::new(checkpoint.chain_id, 1, 1, validators).unwrap();
        let ledger = NovNativeBlockLedgerV1::open_existing_read_only(&path)
            .unwrap()
            .unwrap();
        let authority = NovNativeSealEpochAuthorityV1::derive_operator_pinned_genesis_epoch(
            &ledger, set, bindings,
        )
        .unwrap();
        AuthorizationFixtureV1 {
            bundle,
            checkpoint,
            authority,
            keys,
        }
    }

    #[test]
    fn native_nonce_upgrade_authorization_three_of_four_is_attestation_only() {
        let fixture = native_nonce_upgrade_authorization_fixture_v1();
        let verified = fixture.prepare(TARGET);
        assert_eq!(
            verified.subject().activation_height,
            verified.subject().checkpoint_height + 1
        );
        let certificate = fixture.certificate(&verified);
        assert_eq!(certificate.signature_count, 3);
        assert_eq!(certificate.quorum_weight, 3);
        certificate.verify(&verified).unwrap();
        let bytes = serde_json::to_vec(&certificate).unwrap();
        let report = fixture.verify_json(&bytes).unwrap();
        assert!(report.quorum_verified);
        assert!(!report.activation_ready);
        assert!(!report.authority_state_published);
        assert!(!report.import_performed);
        assert!(!report.chain_canonical);
        assert!(!report.proof_sealed);
        assert!(!report.safe);
        assert!(!report.finalized);
        assert!(!report.aoem_evidence_verified);
        assert!(!report.independent_provenance_verified);
        assert!(!report.target_runtime_compatibility_verified);
        assert!(!report.execution_replayed);
        let mut reversed = certificate.votes.clone();
        reversed.reverse();
        assert_eq!(
            certificate,
            NonceUpgradeAuthorizationCertificateV1::from_votes(&verified, reversed).unwrap()
        );
    }

    #[test]
    fn native_nonce_upgrade_authorization_two_of_four_and_duplicates_reject() {
        let fixture = native_nonce_upgrade_authorization_fixture_v1();
        let verified = fixture.prepare(TARGET);
        let votes = fixture
            .keys
            .iter()
            .take(2)
            .map(|key| sign_nonce_upgrade_vote_unfenced_v1(&verified, key).unwrap())
            .collect::<Vec<_>>();
        assert!(
            NonceUpgradeAuthorizationCertificateV1::from_votes(&verified, votes.clone()).is_err()
        );
        let mut duplicates = votes.clone();
        duplicates.push(votes[0].clone());
        assert!(NonceUpgradeAuthorizationCertificateV1::from_votes(&verified, duplicates).is_err());
        assert!(
            sign_nonce_upgrade_vote_unfenced_v1(&verified, &SigningKey::from_bytes(&[99; 32]))
                .is_err()
        );
        assert!(NonceUpgradeAuthorizationCertificateV1::from_votes(&verified, Vec::new()).is_err());
    }

    #[test]
    fn native_nonce_upgrade_authorization_weighted_quorum_recomputes_weight() {
        let fixture = fixture_with_weights(&[3, 1, 1, 1]);
        let verified = fixture.prepare(TARGET);
        let mut certificate = fixture.certificate(&verified);
        assert_eq!(certificate.signed_weight, 5);
        assert_eq!(certificate.quorum_weight, 5);
        certificate.signed_weight = 6;
        certificate.certificate_hash = certificate_hash(&certificate);
        assert!(certificate.verify(&verified).is_err());
        let low_weight = fixture
            .keys
            .iter()
            .skip(1)
            .map(|key| sign_nonce_upgrade_vote_unfenced_v1(&verified, key).unwrap())
            .collect();
        assert!(NonceUpgradeAuthorizationCertificateV1::from_votes(&verified, low_weight).is_err());
    }

    #[test]
    fn native_nonce_upgrade_authorization_rejects_tamper_target_order_and_domain_replay() {
        let fixture = native_nonce_upgrade_authorization_fixture_v1();
        let verified = fixture.prepare(TARGET);
        let certificate = fixture.certificate(&verified);
        let other = fixture.prepare(&"ee".repeat(32));
        assert!(certificate.verify(&other).is_err());
        let mut bad = certificate.clone();
        bad.subject.activation_height += 1;
        bad.subject.subject_hash = subject_hash(&bad.subject).unwrap();
        bad.certificate_hash = certificate_hash(&bad);
        assert!(bad.verify(&verified).is_err());
        bad = certificate.clone();
        bad.votes.reverse();
        bad.certificate_hash = certificate_hash(&bad);
        assert!(bad.verify(&verified).is_err());
        let mut vote = sign_nonce_upgrade_vote_unfenced_v1(&verified, &fixture.keys[0]).unwrap();
        vote.signature[0] ^= 1;
        vote.vote_hash = vote_hash(&vote);
        assert!(vote.verify(&verified).is_err());
        // Even the same key and subject cannot turn a legacy consensus or
        // block-seal signature into an upgrade authorization signature.
        for domain in [
            b"VOTE:".as_slice(),
            b"GOV_VOTE_V1:".as_slice(),
            b"novovm-native-seal-vote-signing-v1\0".as_slice(),
        ] {
            let message = hash_parts(domain, &[&vote.subject_hash, &vote.validator_id]);
            vote.signature = fixture.keys[0].sign(&message).to_bytes().to_vec();
            vote.vote_hash = vote_hash(&vote);
            assert!(vote.verify(&verified).is_err());
        }
    }

    // Build another internally valid operator manifest to test that consistency
    // of the manifest itself is not enough to establish source-chain identity.
    fn rehash_authority(authority: &mut NovNativeSealEpochAuthorityV1) {
        let mut hash = Sha256::new();
        hash.update(b"novovm-native-seal-epoch-authority-v1\0");
        fn text(hash: &mut Sha256, value: &str) {
            hash.update((value.len() as u64).to_be_bytes());
            hash.update(value.as_bytes());
        }
        text(&mut hash, &authority.schema);
        text(&mut hash, &authority.authority_kind);
        hash.update(authority.chain_id.to_be_bytes());
        hash.update(authority.genesis_block_hash);
        hash.update(authority.protocol_config_commitment);
        hash.update(authority.epoch.to_be_bytes());
        hash.update(authority.activation_height.to_be_bytes());
        hash.update(authority.validator_set.validator_set_hash);
        hash.update((authority.transport_bindings.len() as u64).to_be_bytes());
        for binding in &authority.transport_bindings {
            hash.update(binding.validator_id);
            text(&mut hash, &binding.transport_peer_id);
        }
        text(&mut hash, &authority.leader_schedule);
        authority.authority_commitment = hash.finalize().into();
    }

    #[test]
    fn native_nonce_upgrade_authorization_rejects_wrong_pin_authority_genesis_and_protocol() {
        let fixture = native_nonce_upgrade_authorization_fixture_v1();
        let bundle_digest = checkpoint_bundle_digest_v1(&fixture.bundle);
        let good_pin = hex(&fixture.authority.authority_commitment);
        let inputs = NonceUpgradeAuthorizationInputsV1 {
            bundle: &fixture.bundle,
            bundle_digest: &bundle_digest,
            checkpoint: &fixture.checkpoint,
            target_protocol: TARGET,
            authority: &fixture.authority,
            expected_authority_commitment: &good_pin,
        };
        for bad_pin in [
            "00".repeat(32),
            "ff".repeat(32),
            good_pin.to_ascii_uppercase(),
        ] {
            assert!(
                prepare_nonce_upgrade_authorization_v1(&NonceUpgradeAuthorizationInputsV1 {
                    expected_authority_commitment: &bad_pin,
                    ..inputs
                })
                .is_err()
            );
        }
        for change_genesis in [true, false] {
            let mut bad_authority = fixture.authority.clone();
            if change_genesis {
                bad_authority.genesis_block_hash[0] ^= 1;
            } else {
                bad_authority.protocol_config_commitment[0] ^= 1;
            }
            rehash_authority(&mut bad_authority);
            bad_authority.validate().unwrap();
            let pin = hex(&bad_authority.authority_commitment);
            let error =
                prepare_nonce_upgrade_authorization_v1(&NonceUpgradeAuthorizationInputsV1 {
                    authority: &bad_authority,
                    expected_authority_commitment: &pin,
                    ..inputs
                })
                .unwrap_err();
            assert!(error
                .to_string()
                .contains("does not bind verified checkpoint"));
        }
        let mut bad_set = fixture.authority.clone();
        bad_set.validator_set.total_weight += 1;
        assert!(
            prepare_nonce_upgrade_authorization_v1(&NonceUpgradeAuthorizationInputsV1 {
                authority: &bad_set,
                ..inputs
            })
            .is_err()
        );
    }

    #[test]
    fn native_nonce_upgrade_authorization_json_rejects_unknown_duplicate_trailing_and_oversized() {
        let fixture = native_nonce_upgrade_authorization_fixture_v1();
        let verified = fixture.prepare(TARGET);
        let certificate = fixture.certificate(&verified);
        let mut value = serde_json::to_value(&certificate).unwrap();
        value["activation_ready"] = serde_json::json!(true);
        assert!(fixture
            .verify_json(&serde_json::to_vec(&value).unwrap())
            .is_err());
        value = serde_json::to_value(&certificate).unwrap();
        value["subject"]["extra"] = serde_json::json!(true);
        assert!(fixture
            .verify_json(&serde_json::to_vec(&value).unwrap())
            .is_err());
        value = serde_json::to_value(&certificate).unwrap();
        value["votes"][0]["extra"] = serde_json::json!(true);
        assert!(fixture
            .verify_json(&serde_json::to_vec(&value).unwrap())
            .is_err());
        let text = serde_json::to_string(&certificate).unwrap();
        let duplicate = text.replacen("{", &format!("{{\"schema\":\"{CERTIFICATE_SCHEMA}\","), 1);
        assert!(fixture.verify_json(duplicate.as_bytes()).is_err());
        assert!(fixture
            .verify_json(format!("{text}{{}}").as_bytes())
            .is_err());
        assert!(fixture.verify_json(&[]).is_err());
        assert!(fixture
            .verify_json(&vec![b' '; MAX_NONCE_UPGRADE_AUTHORIZATION_BYTES_V1 + 1])
            .is_err());
    }

    #[test]
    fn native_nonce_upgrade_authorization_rejects_changed_source_member_and_set() {
        let fixture = native_nonce_upgrade_authorization_fixture_v1();
        let verified = fixture.prepare(TARGET);
        let bundle_digest = checkpoint_bundle_digest_v1(&fixture.bundle);
        let pin = hex(&fixture.authority.authority_commitment);
        let inputs = NonceUpgradeAuthorizationInputsV1 {
            bundle: &fixture.bundle,
            bundle_digest: &bundle_digest,
            checkpoint: &fixture.checkpoint,
            target_protocol: TARGET,
            authority: &fixture.authority,
            expected_authority_commitment: &pin,
        };
        assert!(
            prepare_nonce_upgrade_authorization_v1(&NonceUpgradeAuthorizationInputsV1 {
                bundle_digest: &"aa".repeat(32),
                ..inputs
            })
            .is_err()
        );
        let mut checkpoint = fixture.checkpoint.clone();
        checkpoint.tip_block_hash = "aa".repeat(32);
        assert!(
            prepare_nonce_upgrade_authorization_v1(&NonceUpgradeAuthorizationInputsV1 {
                checkpoint: &checkpoint,
                ..inputs
            })
            .is_err()
        );
        let mut altered_set = fixture.authority.clone();
        altered_set.validator_set.validators[1] = altered_set.validator_set.validators[0].clone();
        assert!(
            prepare_nonce_upgrade_authorization_v1(&NonceUpgradeAuthorizationInputsV1 {
                authority: &altered_set,
                ..inputs
            })
            .is_err()
        );
        let mut vote = sign_nonce_upgrade_vote_unfenced_v1(&verified, &fixture.keys[0]).unwrap();
        vote.validator_id = [0xff; 32];
        vote.vote_hash = vote_hash(&vote);
        assert!(vote.verify(&verified).is_err());
        vote = sign_nonce_upgrade_vote_unfenced_v1(&verified, &fixture.keys[0]).unwrap();
        vote.subject_hash[0] ^= 1;
        vote.signature = fixture.keys[0]
            .sign(&signing_message(&vote))
            .to_bytes()
            .to_vec();
        vote.vote_hash = vote_hash(&vote);
        assert!(vote.verify(&verified).is_err());
        let certificate = fixture.certificate(&verified);
        for change_epoch in [true, false] {
            let mut changed = certificate.clone();
            if change_epoch {
                changed.subject.epoch += 1;
            } else {
                changed.subject.chain_id += 1;
            }
            changed.subject.subject_hash = subject_hash(&changed.subject).unwrap();
            changed.certificate_hash = certificate_hash(&changed);
            assert!(changed.verify(&verified).is_err());
        }
    }
}
