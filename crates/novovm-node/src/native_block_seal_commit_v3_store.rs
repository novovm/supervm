//! A V3 decision occupies the SAME signer/height slot as V1/V2.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionLock {
    schema: String,
    prepare_qc_hash: [u8; 32],
    vote: NovNativeSealDecisionVoteV3,
}

impl NovNativeBlockSealStoreV1 {
    /// Explicit local call only. Persist before releasing a signature. A replay
    /// returns the original bytes, even under another valid round witness.
    pub fn sign_local_decision_vote_v3(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        prepare: &NovNativeSealQuorumCertificateV1,
        set: &NovNativeSealValidatorSetV1,
        key: &SigningKey,
    ) -> Result<NovNativeSealDecisionVoteV3> {
        let target = decision_target_v3(prepare, set)?;
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
            bail!("decision v3 does not match local execution");
        }
        let id = validator_id_v1(key.verifying_key().as_bytes());
        if set.validator(id).is_none() {
            bail!("unknown local decision v3 signer");
        }
        let binding = store_binding_v1(ledger, subject.chain_id)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&binding)?;
        self.ensure_registered_validator_set_v1(set)?;
        let proposal = self.verify_decision_witness_v3(prepare, set)?;
        for other in self.load_qcs_by_height(subject.chain_id, subject.epoch, subject.height)? {
            if decision_target_v3(&other, set)? != target {
                bail!("competing decision QC prevents v3 signing or replay");
            }
        }
        // No automatic protocol migration of an already confirmed height.
        if self
            .db
            .get(
                commit::certificate_height_key(subject.chain_id, subject.epoch, subject.height)
                    .as_bytes(),
            )?
            .is_some()
        {
            bail!("existing commit certificate prevents v3 signing");
        }
        let slot = commit::lock_key(subject, id);
        let marker = format!("{slot}/decision-v3-vote-hash");
        let pin = read_json_v1::<[u8; 32]>(&self.db, marker.as_bytes(), "decision v3 vote marker")?;
        if let Some(lock) =
            read_json_v1::<DecisionLock>(&self.db, slot.as_bytes(), "decision v3 lock")?
        {
            if lock.schema != "novovm-native-seal-decision-lock/v3" || lock.vote.validator_id != id
            {
                bail!("decision v3 lock version or identity mismatch");
            }
            let original = self
                .load_qc(lock.prepare_qc_hash)?
                .context("decision v3 original QC missing")?;
            self.verify_decision_witness_v3(&original, set)?;
            lock.vote.verify(&original, set)?;
            lock.vote.verify_target(target, set)?;
            if pin != Some(lock.vote.vote_hash) {
                bail!("decision v3 marker missing or changed");
            }
            // Replaying a decision is not a signature in the requesting round.
            self.validate_existing_safety_locks_v1(&original.subject, id)?;
            return Ok(lock.vote);
        }
        if pin.is_some() {
            bail!("decision v3 durable lock disappeared");
        }
        self.ensure_active_new_view_admission_v1(ledger, subject, proposal.proposer_id, set)?;
        self.ensure_not_timed_out_v1(subject, id, set)?;
        let (round_lock, height_lock) = self.prepare_safety_locks_v1(subject, id)?;
        let mut vote = NovNativeSealDecisionVoteV3 {
            schema: VOTE_SCHEMA.into(),
            target_hash: target,
            validator_id: id,
            signature: Vec::new(),
            vote_hash: [0; 32],
        };
        vote.signature = key.sign(&vote.message()).to_bytes().to_vec();
        vote.vote_hash = vote.hash();
        vote.verify_target(target, set)?;
        let lock = DecisionLock {
            schema: "novovm-native-seal-decision-lock/v3".into(),
            prepare_qc_hash: prepare.qc_hash,
            vote,
        };
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(&mut batch, slot.as_bytes(), &lock, "decision v3 lock")?;
        put_json_v1(
            &mut batch,
            marker.as_bytes(),
            &lock.vote.vote_hash,
            "decision v3 marker",
        )?;
        put_json_v1(
            &mut batch,
            round_lock_key_v1(subject, id).as_bytes(),
            &round_lock,
            "round safety lock",
        )?;
        put_json_v1(
            &mut batch,
            height_lock_key_v1(subject, id).as_bytes(),
            &height_lock,
            "height safety lock",
        )?;
        write_sync_v1(&self.db, batch)?;
        if read_json_v1::<DecisionLock>(&self.db, slot.as_bytes(), "decision v3 readback")?.as_ref()
            != Some(&lock)
            || read_json_v1::<[u8; 32]>(&self.db, marker.as_bytes(), "decision v3 marker readback")?
                != Some(lock.vote.vote_hash)
        {
            bail!("decision v3 durable readback mismatch");
        }
        self.validate_existing_safety_locks_v1(subject, id)?;
        Ok(lock.vote)
    }

    fn verify_decision_witness_v3(
        &self,
        qc: &NovNativeSealQuorumCertificateV1,
        set: &NovNativeSealValidatorSetV1,
    ) -> Result<NovNativeSealProposalV1> {
        qc.verify(set)?;
        if self.load_qc(qc.qc_hash)?.as_ref() != Some(qc) {
            bail!("decision v3 requires exact durable QC");
        }
        self.ensure_qc_indexes_contain_v1(qc)?;
        let proposal = self
            .load_proposal(qc.proposal_hash)?
            .context("decision v3 proposal missing")?;
        if proposal.subject != qc.subject {
            bail!("decision v3 proposal mismatch");
        }
        self.ensure_new_view_admission_v1(&qc.subject, proposal.proposer_id, set)?;
        Ok(proposal)
    }
}
