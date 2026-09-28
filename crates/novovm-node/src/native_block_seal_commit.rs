//! Second-phase attestation primitives, not a finality or promotion API.
//!
//! This conservative local contract pins one exact prepare QC per signer/height.
//! No network driver uses it yet. Replacing that QC (including another signer
//! subset for the same block) requires a future versioned protocol, not an unlock.
use super::*;

const VOTE_SCHEMA: &str = "novovm-native-seal-commit-vote/v1";
const CERTIFICATE_SCHEMA: &str = "novovm-native-seal-commit-certificate/v1";
const SIGNING_DOMAIN: &[u8] = b"novovm-native-seal-commit-signing-v1\0";
const VOTE_DOMAIN: &[u8] = b"novovm-native-seal-commit-vote-v1\0";
const CERTIFICATE_DOMAIN: &[u8] = b"novovm-native-seal-commit-certificate-v1\0";

/// A distinct signature over the full prepare QC hash, which transitively binds
/// chain/genesis, epoch, round, candidate roots, proposal and prepare signers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeSealCommitVoteV1 {
    pub schema: String,
    pub prepare_qc_hash: [u8; 32],
    pub validator_id: [u8; 32],
    pub signature: Vec<u8>,
    pub vote_hash: [u8; 32],
}

impl NovNativeSealCommitVoteV1 {
    pub fn verify(
        &self,
        prepare: &NovNativeSealQuorumCertificateV1,
        set: &NovNativeSealValidatorSetV1,
    ) -> Result<()> {
        prepare.verify(set)?;
        self.verify_with_checked_prepare(prepare, set)
    }

    fn verify_with_checked_prepare(
        &self,
        prepare: &NovNativeSealQuorumCertificateV1,
        set: &NovNativeSealValidatorSetV1,
    ) -> Result<()> {
        if self.schema != VOTE_SCHEMA || self.prepare_qc_hash != prepare.qc_hash {
            bail!("native commit vote metadata or prepare QC binding is invalid");
        }
        let validator = set
            .validator(self.validator_id)
            .context("unknown commit signer")?;
        let key = VerifyingKey::from_bytes(&validator.public_key)?;
        let signature = Signature::from_slice(&self.signature)?;
        key.verify_strict(&self.signing_message(), &signature)
            .context("native commit signature verification failed")?;
        if self.vote_hash != self.compute_hash() {
            bail!("native commit vote hash is invalid");
        }
        Ok(())
    }

    fn signing_message(&self) -> Vec<u8> {
        [SIGNING_DOMAIN, &self.prepare_qc_hash, &self.validator_id].concat()
    }

    fn compute_hash(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(VOTE_DOMAIN);
        hash.update(self.prepare_qc_hash);
        hash.update(self.validator_id);
        hash.update(&self.signature);
        hash.finalize().into()
    }
}

/// Verified second-phase quorum evidence only. It does not establish a
/// finalized parent, fork choice, DA availability now, or state promotion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeSealCommitCertificateV1 {
    pub schema: String,
    pub prepare: NovNativeSealQuorumCertificateV1,
    pub votes: Vec<NovNativeSealCommitVoteV1>,
    pub signed_weight: u64,
    pub certificate_hash: [u8; 32],
}

impl NovNativeSealCommitCertificateV1 {
    pub fn from_votes(
        prepare: NovNativeSealQuorumCertificateV1,
        set: &NovNativeSealValidatorSetV1,
        mut votes: Vec<NovNativeSealCommitVoteV1>,
    ) -> Result<Self> {
        prepare.verify(set)?;
        votes.sort_by_key(|vote| vote.validator_id);
        let signed_weight = checked_weight(&prepare, set, &votes)?;
        let mut certificate = Self {
            schema: CERTIFICATE_SCHEMA.into(),
            prepare,
            votes,
            signed_weight,
            certificate_hash: [0; 32],
        };
        certificate.certificate_hash = certificate.compute_hash();
        Ok(certificate)
    }

    pub fn verify(&self, set: &NovNativeSealValidatorSetV1) -> Result<()> {
        self.prepare.verify(set)?;
        if self.schema != CERTIFICATE_SCHEMA
            || self.signed_weight != checked_weight(&self.prepare, set, &self.votes)?
            || self.certificate_hash != self.compute_hash()
        {
            bail!("native commit certificate metadata or hash is invalid");
        }
        Ok(())
    }

    fn compute_hash(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(CERTIFICATE_DOMAIN);
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
    votes: &[NovNativeSealCommitVoteV1],
) -> Result<u64> {
    if votes.is_empty() || votes.len() > set.validators.len() {
        bail!("native commit certificate vote count is invalid");
    }
    let mut previous = None;
    let mut weight = 0u64;
    for vote in votes {
        vote.verify_with_checked_prepare(prepare, set)?;
        if previous.is_some_and(|id| id >= vote.validator_id) {
            bail!("native commit votes must be sorted and unique");
        }
        previous = Some(vote.validator_id);
        weight = weight
            .checked_add(
                set.validator(vote.validator_id)
                    .context("unknown commit signer")?
                    .weight,
            )
            .context("native commit weight overflow")?;
    }
    if weight < set.quorum_weight {
        bail!("native commit quorum has insufficient weight");
    }
    Ok(weight)
}

fn lock_key(subject: &NovNativeSealSubjectV1, validator_id: [u8; 32]) -> String {
    format!(
        "{KEY_PREFIX_V1}commit/local/{:020}/{:020}/{:020}/{}",
        subject.chain_id,
        subject.epoch,
        subject.height,
        hex_v1(&validator_id)
    )
}

impl NovNativeBlockSealStoreV1 {
    /// Archive one exact second-phase certificate per height. This is an
    /// immutable evidence slot, NOT a canonical pointer or a promotion decision.
    /// Alternative certificates are rejected, never silently substituted.
    pub fn persist_local_verified_commit_certificate(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        certificate: &NovNativeSealCommitCertificateV1,
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
            bail!("commit certificate does not match local candidate");
        }
        let binding = store_binding_v1(ledger, subject.chain_id)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&binding)?;
        self.ensure_registered_validator_set_v1(set)?;
        self.validate_commit_certificate_dependencies(certificate)?;
        for other in self.load_qcs_by_height(subject.chain_id, subject.epoch, subject.height)? {
            if other.subject.block_hash != subject.block_hash {
                bail!("competing prepare QC prevents commit certificate persistence");
            }
        }
        if let Some(existing) =
            self.load_commit_certificate_by_height(subject.chain_id, subject.epoch, subject.height)?
        {
            if existing != *certificate {
                bail!("commit certificate height slot already contains different evidence");
            }
            return Ok(false);
        }
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            certificate_height_key(subject.chain_id, subject.epoch, subject.height).as_bytes(),
            certificate,
            "commit certificate height slot",
        )?;
        write_sync_v1(&self.db, batch).context("persist commit certificate evidence")?;
        if self
            .load_commit_certificate_by_height(subject.chain_id, subject.epoch, subject.height)?
            .as_ref()
            != Some(certificate)
        {
            bail!("commit certificate persistence readback mismatch");
        }
        Ok(true)
    }

    /// Revalidate archived evidence against its durable validator set and QC.
    /// No current execution/DA or finality claim is made by this read API.
    pub fn load_commit_certificate_by_height(
        &self,
        chain_id: u64,
        epoch: u64,
        height: u64,
    ) -> Result<Option<NovNativeSealCommitCertificateV1>> {
        self.ensure_schema_v1()?;
        let certificate = read_json_v1::<NovNativeSealCommitCertificateV1>(
            &self.db,
            certificate_height_key(chain_id, epoch, height).as_bytes(),
            "commit certificate height slot",
        )?;
        if let Some(certificate) = &certificate {
            let subject = &certificate.prepare.subject;
            if (subject.chain_id, subject.epoch, subject.height) != (chain_id, epoch, height) {
                bail!("commit certificate height slot binding mismatch");
            }
            self.validate_commit_certificate_dependencies(certificate)?;
        }
        Ok(certificate)
    }

    fn validate_commit_certificate_dependencies(
        &self,
        certificate: &NovNativeSealCommitCertificateV1,
    ) -> Result<()> {
        let prepare = &certificate.prepare;
        let subject = &prepare.subject;
        let set = self
            .load_validator_set(subject.chain_id, subject.epoch)?
            .context("commit certificate missing durable validator set")?;
        certificate.verify(&set)?;
        if self.load_qc(prepare.qc_hash)?.as_ref() != Some(prepare) {
            bail!("commit certificate requires exact durable prepare QC");
        }
        self.ensure_qc_indexes_contain_v1(prepare)?;
        let proposal = self
            .load_proposal(prepare.proposal_hash)?
            .context("commit certificate missing durable proposal")?;
        if proposal.subject != *subject {
            bail!("commit certificate proposal subject mismatch");
        }
        self.ensure_new_view_admission_v1(subject, proposal.proposer_id, &set)?;
        Ok(())
    }

    /// Trusted local Host API, deliberately not called by the round driver.
    /// Persist signature and shared safety locks in one sync batch before return.
    /// A stored prepare QC alone never causes signing, networking or finality.
    pub fn sign_local_commit_vote(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        prepare: &NovNativeSealQuorumCertificateV1,
        set: &NovNativeSealValidatorSetV1,
        signing_key: &SigningKey,
    ) -> Result<NovNativeSealCommitVoteV1> {
        prepare.verify(set)?;
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
            bail!("commit QC does not match local candidate");
        }
        let validator_id = validator_id_v1(signing_key.verifying_key().as_bytes());
        if set.validator(validator_id).is_none() {
            bail!("unknown local commit signer");
        }
        let binding = store_binding_v1(ledger, subject.chain_id)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&binding)?;
        self.ensure_registered_validator_set_v1(set)?;
        if self.load_qc(prepare.qc_hash)?.as_ref() != Some(prepare) {
            bail!("commit requires exact durably stored prepare QC");
        }
        self.ensure_qc_indexes_contain_v1(prepare)?;
        let proposal = self
            .load_proposal(prepare.proposal_hash)?
            .context("commit proposal missing")?;
        if proposal.subject != *subject {
            bail!("commit proposal subject mismatch");
        }
        self.ensure_new_view_admission_v1(subject, proposal.proposer_id, set)?;
        for other in self.load_qcs_by_height(subject.chain_id, subject.epoch, subject.height)? {
            if other.subject.block_hash != subject.block_hash {
                bail!("competing prepare QC prevents local commit signing");
            }
        }
        let key = lock_key(subject, validator_id);
        if let Some(vote) =
            read_json_v1::<NovNativeSealCommitVoteV1>(&self.db, key.as_bytes(), "commit lock")?
        {
            vote.verify_with_checked_prepare(prepare, set)?;
            if vote.validator_id != validator_id {
                bail!("commit lock signer mismatch");
            }
            self.validate_existing_safety_locks_v1(subject, validator_id)?;
            return Ok(vote);
        }
        self.ensure_active_new_view_admission_v1(ledger, subject, proposal.proposer_id, set)?;
        self.ensure_not_timed_out_v1(subject, validator_id, set)?;
        let (round_lock, height_lock) = self.prepare_safety_locks_v1(subject, validator_id)?;
        let mut vote = NovNativeSealCommitVoteV1 {
            schema: VOTE_SCHEMA.into(),
            prepare_qc_hash: prepare.qc_hash,
            validator_id,
            signature: Vec::new(),
            vote_hash: [0; 32],
        };
        vote.signature = signing_key
            .sign(&vote.signing_message())
            .to_bytes()
            .to_vec();
        vote.vote_hash = vote.compute_hash();
        vote.verify_with_checked_prepare(prepare, set)?;
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(&mut batch, key.as_bytes(), &vote, "commit vote and lock")?;
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
        if read_json_v1::<NovNativeSealCommitVoteV1>(&self.db, key.as_bytes(), "commit readback")?
            .as_ref()
            != Some(&vote)
        {
            bail!("commit vote persistence readback mismatch");
        }
        self.validate_existing_safety_locks_v1(subject, validator_id)?;
        Ok(vote)
    }
}

fn certificate_height_key(chain_id: u64, epoch: u64, height: u64) -> String {
    format!("{KEY_PREFIX_V1}commit/certificate/{chain_id:020}/{epoch:020}/{height:020}")
}
