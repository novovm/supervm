//! Versioned, prepare-signer-subset-independent commit attestations.
//! Local primitives only: no online activation, round unlock or finality.
use super::*;

const VOTE_SCHEMA: &str = "novovm-native-seal-commit-vote/v2";
const CERT_SCHEMA: &str = "novovm-native-seal-commit-certificate/v2";
const LOCK_SCHEMA: &str = "novovm-native-seal-commit-lock/v2";
const SIGN_DOMAIN: &[u8] = b"novovm-native-seal-commit-signing-v2\0";

/// Only the prepare witness's signer subset/weight/hash is excluded. The full
/// validated subject (including round and parent QC) and proposal remain bound.
pub fn commit_target_v2(
    prepare: &NovNativeSealQuorumCertificateV1,
    set: &NovNativeSealValidatorSetV1,
) -> Result<[u8; 32]> {
    prepare.verify(set)?;
    Ok(target_hash(prepare))
}

fn target_hash(prepare: &NovNativeSealQuorumCertificateV1) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"novovm-native-seal-commit-target-v2\0");
    hash.update(prepare.subject_hash);
    hash.update(prepare.proposal_hash);
    hash.finalize().into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeSealCommitVoteV2 {
    pub schema: String,
    pub target_hash: [u8; 32],
    pub validator_id: [u8; 32],
    pub signature: Vec<u8>,
    pub vote_hash: [u8; 32],
}

impl NovNativeSealCommitVoteV2 {
    pub fn verify(
        &self,
        prepare: &NovNativeSealQuorumCertificateV1,
        set: &NovNativeSealValidatorSetV1,
    ) -> Result<()> {
        self.verify_target(commit_target_v2(prepare, set)?, set)
    }

    fn verify_target(&self, target: [u8; 32], set: &NovNativeSealValidatorSetV1) -> Result<()> {
        if self.schema != VOTE_SCHEMA || self.target_hash != target {
            bail!("commit v2 schema or target mismatch");
        }
        let member = set
            .validator(self.validator_id)
            .context("unknown commit v2 signer")?;
        VerifyingKey::from_bytes(&member.public_key)?
            .verify_strict(&self.message(), &Signature::from_slice(&self.signature)?)?;
        if self.vote_hash != self.hash() {
            bail!("commit v2 vote hash mismatch");
        }
        Ok(())
    }

    fn message(&self) -> Vec<u8> {
        [SIGN_DOMAIN, &self.target_hash, &self.validator_id].concat()
    }

    fn hash(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"novovm-native-seal-commit-vote-v2\0");
        hash.update(self.target_hash);
        hash.update(self.validator_id);
        hash.update(&self.signature);
        hash.finalize().into()
    }
}

/// Votes may originate from different valid prepare subsets for the same target.
/// The certificate hash identifies exact evidence, NOT the consensus target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeSealCommitCertificateV2 {
    pub schema: String,
    pub prepare: NovNativeSealQuorumCertificateV1,
    pub votes: Vec<NovNativeSealCommitVoteV2>,
    pub signed_weight: u64,
    pub certificate_hash: [u8; 32],
}

impl NovNativeSealCommitCertificateV2 {
    pub fn from_votes(
        prepare: NovNativeSealQuorumCertificateV1,
        set: &NovNativeSealValidatorSetV1,
        mut votes: Vec<NovNativeSealCommitVoteV2>,
    ) -> Result<Self> {
        votes.sort_by_key(|vote| vote.validator_id);
        let signed_weight = checked_weight(&prepare, set, &votes)?;
        let mut result = Self {
            schema: CERT_SCHEMA.into(),
            prepare,
            votes,
            signed_weight,
            certificate_hash: [0; 32],
        };
        result.certificate_hash = result.hash();
        Ok(result)
    }

    pub fn verify(&self, set: &NovNativeSealValidatorSetV1) -> Result<()> {
        if self.schema != CERT_SCHEMA
            || self.signed_weight != checked_weight(&self.prepare, set, &self.votes)?
            || self.certificate_hash != self.hash()
        {
            bail!("commit v2 certificate metadata or hash mismatch");
        }
        Ok(())
    }

    fn hash(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"novovm-native-seal-commit-certificate-v2\0");
        hash.update(self.prepare.qc_hash);
        hash.update(self.signed_weight.to_le_bytes());
        hash.update((self.votes.len() as u64).to_le_bytes());
        for vote in &self.votes {
            hash.update(vote.vote_hash);
        }
        hash.finalize().into()
    }
}

fn checked_weight(
    prepare: &NovNativeSealQuorumCertificateV1,
    set: &NovNativeSealValidatorSetV1,
    votes: &[NovNativeSealCommitVoteV2],
) -> Result<u64> {
    let target = commit_target_v2(prepare, set)?;
    if votes.is_empty() || votes.len() > set.validators.len() {
        bail!("invalid commit v2 vote count");
    }
    let mut previous = None;
    let mut weight = 0u64;
    for vote in votes {
        vote.verify_target(target, set)?;
        if previous.is_some_and(|id| id >= vote.validator_id) {
            bail!("commit v2 signers must be sorted and unique");
        }
        previous = Some(vote.validator_id);
        weight = weight
            .checked_add(
                set.validator(vote.validator_id)
                    .context("unknown commit v2 signer")?
                    .weight,
            )
            .context("commit v2 weight overflow")?;
    }
    if weight < set.quorum_weight {
        bail!("insufficient commit v2 quorum weight");
    }
    Ok(weight)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommitLockV2 {
    schema: String,
    /// Retain the original durable witness even when replay uses another subset.
    prepare_qc_hash: [u8; 32],
    vote: NovNativeSealCommitVoteV2,
}

impl NovNativeBlockSealStoreV1 {
    /// Archive the first verified certificate for a target. Equivalent witness
    /// subsets return false without replacing the original exact evidence.
    /// A different target/version at this height is an error, not an unlock.
    pub fn persist_local_verified_commit_certificate_v2(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        certificate: &NovNativeSealCommitCertificateV2,
        set: &NovNativeSealValidatorSetV1,
    ) -> Result<bool> {
        certificate.verify(set)?;
        let subject = &certificate.prepare.subject;
        let local = self.prepare_local_subject(
            ledger,
            subject.chain_id,
            subject.block_hash,
            set,
            subject.round,
            (subject.justify_qc_hash != [0; 32]).then_some(subject.justify_qc_hash),
        )?;
        if local != *subject {
            bail!("commit v2 certificate does not match local candidate");
        }
        let binding = store_binding_v1(ledger, subject.chain_id)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&binding)?;
        self.ensure_registered_validator_set_v1(set)?;
        self.verify_commit_v2_witness(&certificate.prepare, set)?;
        for other in self.load_qcs_by_height(subject.chain_id, subject.epoch, subject.height)? {
            if other.subject.block_hash != subject.block_hash {
                bail!("competing prepare QC prevents commit v2 certificate persistence");
            }
        }
        if let Some(existing) = self.load_commit_certificate_by_height_v2(
            subject.chain_id,
            subject.epoch,
            subject.height,
        )? {
            if target_hash(&existing.prepare) != target_hash(&certificate.prepare) {
                bail!("commit v2 certificate height slot contains a different target");
            }
            return Ok(false);
        }
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            commit::certificate_height_key(subject.chain_id, subject.epoch, subject.height)
                .as_bytes(),
            certificate,
            "commit v2 certificate",
        )?;
        write_sync_v1(&self.db, batch)?;
        if self
            .load_commit_certificate_by_height_v2(subject.chain_id, subject.epoch, subject.height)?
            .as_ref()
            != Some(certificate)
        {
            bail!("commit v2 certificate readback mismatch");
        }
        Ok(true)
    }

    /// Historical verified evidence only; does not assert current DA or finality.
    pub fn load_commit_certificate_by_height_v2(
        &self,
        chain_id: u64,
        epoch: u64,
        height: u64,
    ) -> Result<Option<NovNativeSealCommitCertificateV2>> {
        self.ensure_schema_v1()?;
        let certificate = read_json_v1::<NovNativeSealCommitCertificateV2>(
            &self.db,
            commit::certificate_height_key(chain_id, epoch, height).as_bytes(),
            "commit v2 certificate",
        )?;
        if let Some(certificate) = &certificate {
            let subject = &certificate.prepare.subject;
            if (subject.chain_id, subject.epoch, subject.height) != (chain_id, epoch, height) {
                bail!("commit v2 certificate height slot mismatch");
            }
            let set = self
                .load_validator_set(chain_id, epoch)?
                .context("commit v2 certificate missing validator set")?;
            certificate.verify(&set)?;
            self.verify_commit_v2_witness(&certificate.prepare, &set)?;
        }
        Ok(certificate)
    }

    /// Explicit local API. V1 and V2 deliberately share the same immutable
    /// signer/height slot: either version rejects the other's stored schema.
    pub fn sign_local_commit_vote_v2(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        prepare: &NovNativeSealQuorumCertificateV1,
        set: &NovNativeSealValidatorSetV1,
        key: &SigningKey,
    ) -> Result<NovNativeSealCommitVoteV2> {
        let target = commit_target_v2(prepare, set)?;
        let subject = &prepare.subject;
        let local = self.prepare_local_subject(
            ledger,
            subject.chain_id,
            subject.block_hash,
            set,
            subject.round,
            (subject.justify_qc_hash != [0; 32]).then_some(subject.justify_qc_hash),
        )?;
        if local != *subject {
            bail!("commit v2 subject does not match local candidate");
        }
        let validator_id = validator_id_v1(key.verifying_key().as_bytes());
        if set.validator(validator_id).is_none() {
            bail!("unknown local commit v2 signer");
        }
        let binding = store_binding_v1(ledger, subject.chain_id)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&binding)?;
        self.ensure_registered_validator_set_v1(set)?;
        let proposal = self.verify_commit_v2_witness(prepare, set)?;
        for other in self.load_qcs_by_height(subject.chain_id, subject.epoch, subject.height)? {
            if other.subject.block_hash != subject.block_hash {
                bail!("competing prepare QC prevents commit v2 signing");
            }
        }
        let slot = commit::lock_key(subject, validator_id);
        if let Some(lock) =
            read_json_v1::<CommitLockV2>(&self.db, slot.as_bytes(), "commit v2 lock")?
        {
            if lock.schema != LOCK_SCHEMA || lock.vote.validator_id != validator_id {
                bail!("commit v2 lock identity mismatch");
            }
            let original = self
                .load_qc(lock.prepare_qc_hash)?
                .context("commit v2 original QC missing")?;
            self.verify_commit_v2_witness(&original, set)?;
            lock.vote.verify(&original, set)?;
            lock.vote.verify_target(target, set)?;
            self.validate_existing_safety_locks_v1(subject, validator_id)?;
            return Ok(lock.vote);
        }
        self.ensure_active_new_view_admission_v1(ledger, subject, proposal.proposer_id, set)?;
        self.ensure_not_timed_out_v1(subject, validator_id, set)?;
        let (round_lock, height_lock) = self.prepare_safety_locks_v1(subject, validator_id)?;
        let mut vote = NovNativeSealCommitVoteV2 {
            schema: VOTE_SCHEMA.into(),
            target_hash: target,
            validator_id,
            signature: Vec::new(),
            vote_hash: [0; 32],
        };
        vote.signature = key.sign(&vote.message()).to_bytes().to_vec();
        vote.vote_hash = vote.hash();
        vote.verify_target(target, set)?;
        let lock = CommitLockV2 {
            schema: LOCK_SCHEMA.into(),
            prepare_qc_hash: prepare.qc_hash,
            vote,
        };
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(&mut batch, slot.as_bytes(), &lock, "commit v2 lock")?;
        put_json_v1(
            &mut batch,
            round_lock_key_v1(subject, validator_id).as_bytes(),
            &round_lock,
            "round safety lock",
        )?;
        put_json_v1(
            &mut batch,
            height_lock_key_v1(subject, validator_id).as_bytes(),
            &height_lock,
            "height safety lock",
        )?;
        write_sync_v1(&self.db, batch)?;
        if read_json_v1::<CommitLockV2>(&self.db, slot.as_bytes(), "commit v2 readback")?.as_ref()
            != Some(&lock)
        {
            bail!("commit v2 lock readback mismatch");
        }
        self.validate_existing_safety_locks_v1(subject, validator_id)?;
        Ok(lock.vote)
    }

    fn verify_commit_v2_witness(
        &self,
        prepare: &NovNativeSealQuorumCertificateV1,
        set: &NovNativeSealValidatorSetV1,
    ) -> Result<NovNativeSealProposalV1> {
        prepare.verify(set)?;
        if self.load_qc(prepare.qc_hash)?.as_ref() != Some(prepare) {
            bail!("commit v2 requires exact durable prepare witness");
        }
        self.ensure_qc_indexes_contain_v1(prepare)?;
        let proposal = self
            .load_proposal(prepare.proposal_hash)?
            .context("commit v2 proposal missing")?;
        if proposal.subject != prepare.subject {
            bail!("commit v2 proposal subject mismatch");
        }
        self.ensure_new_view_admission_v1(&prepare.subject, proposal.proposer_id, set)?;
        Ok(proposal)
    }
}
