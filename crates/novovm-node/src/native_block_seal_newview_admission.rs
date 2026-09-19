//! Durable, candidate-bound local new-view admission. Never releases a signer lock.
use super::store::{authority_key, context_v1};
use super::*;

const ADMISSION_SCHEMA_V1: &str = "novovm-native-seal-new-view-admission/v1";
const ADMISSION_DOMAIN_V1: &[u8] = b"novovm-native-seal-new-view-admission-v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeSealNewViewAdmissionV1 {
    pub schema: String,
    pub authority: NovNativeSealEpochAuthorityV1,
    pub certificate: NovNativeSealNewViewCertificateV1,
    pub subject: NovNativeSealSubjectV1,
    pub admission_hash: [u8; 32],
}

fn admission_key(chain_id: u64, epoch: u64, height: u64, round: u64) -> String {
    format!("native_block_seal/v1/new-view-admission/{chain_id}/{epoch}/{height}/{round}")
}

fn admission_hash(record: &NovNativeSealNewViewAdmissionV1) -> Result<[u8; 32]> {
    let mut canonical = record.clone();
    canonical.admission_hash = [0; 32];
    canonical
        .certificate
        .previous_timeout
        .votes
        .sort_by_key(|vote| vote.validator_id);
    canonical
        .certificate
        .observations
        .sort_by_key(|observation| observation.validator_id);
    let bytes = serde_json::to_vec(&canonical).context("encode canonical new-view admission")?;
    let mut hasher = Sha256::new();
    hasher.update(ADMISSION_DOMAIN_V1);
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    Ok(hasher.finalize().into())
}

fn ensure_same_candidate(
    expected: &NovNativeSealSubjectV1,
    candidate: &NovNativeSealSubjectV1,
) -> Result<()> {
    let mut expected = expected.clone();
    let mut candidate = candidate.clone();
    expected.round = 0;
    expected.subject_hash = [0; 32];
    candidate.round = 0;
    candidate.subject_hash = [0; 32];
    if expected != candidate {
        bail!("native new-view admission contradicts immutable candidate commitments");
    }
    Ok(())
}

impl NovNativeBlockSealStoreV1 {
    fn ensure_admission_highest_qc_durable_v1(
        &self,
        evidence: &NovNativeSealNewViewQcV1,
    ) -> Result<()> {
        let stored = self
            .load_qc(evidence.qc.qc_hash)?
            .context("native new-view admission highest QC is not durably stored")?;
        let proposal = self
            .load_proposal(evidence.proposal.proposal_hash)?
            .context("native new-view admission highest QC proposal is not durably stored")?;
        if stored != evidence.qc || proposal != evidence.proposal {
            bail!("native new-view admission highest QC differs from durable evidence");
        }
        self.ensure_qc_indexes_contain_v1(&stored)
    }

    fn validate_admission_record_v1(&self, record: &NovNativeSealNewViewAdmissionV1) -> Result<()> {
        let authority = &record.authority;
        let subject = &record.subject;
        authority.validate()?;
        subject.validate(&authority.validator_set)?;
        let expected = context_v1(authority, subject.height, subject.round)?;
        if record.schema != ADMISSION_SCHEMA_V1
            || subject.chain_id != expected.chain_id
            || subject.epoch != expected.epoch
            || subject.genesis_block_hash != expected.genesis_block_hash
            || subject.protocol_config_commitment != expected.protocol_config_commitment
            || subject.validator_set_hash != expected.validator_set_hash
            || record.admission_hash != admission_hash(record)?
        {
            bail!("native new-view admission record domain or digest mismatch");
        }
        let highest = record.certificate.verify(&expected, authority)?;
        if let Some(highest) = highest {
            ensure_same_candidate(subject, &highest.qc.subject)?;
            self.ensure_admission_highest_qc_durable_v1(&highest)?;
        }
        self.ensure_new_view_authority_v1(authority, true)?;
        self.ensure_registered_validator_set_v1(&authority.validator_set)?;
        let binding = read_json_v1::<NovNativeSealStoreBindingV1>(
            &self.db,
            store_binding_key_v1(subject.chain_id).as_bytes(),
            "new-view admission store binding",
        )?
        .context("native new-view admission has no durable store binding")?;
        validate_store_binding_v1(&binding)?;
        if binding.chain_id != expected.chain_id
            || binding.genesis_block_hash != expected.genesis_block_hash
            || binding.protocol_config_commitment != expected.protocol_config_commitment
        {
            bail!("native new-view admission does not bind its durable ledger identity");
        }
        Ok(())
    }

    /// Historical evidence lookup. Does not advance a round, emit signatures,
    /// release a candidate lock, or imply that this is still the active round.
    pub fn load_local_new_view_admission(
        &self,
        chain_id: u64,
        epoch: u64,
        height: u64,
        round: u64,
    ) -> Result<Option<NovNativeSealNewViewAdmissionV1>> {
        self.ensure_schema_v1()?;
        let record = read_json_v1::<NovNativeSealNewViewAdmissionV1>(
            &self.db,
            admission_key(chain_id, epoch, height, round).as_bytes(),
            "new-view candidate admission",
        )?;
        if let Some(record) = &record {
            self.validate_admission_record_v1(record)?;
            let subject = &record.subject;
            if (
                subject.chain_id,
                subject.epoch,
                subject.height,
                subject.round,
            ) != (chain_id, epoch, height, round)
            {
                bail!("native new-view admission key/subject mismatch");
            }
        }
        Ok(record)
    }

    /// Local opt-in only: admits one exact candidate per nonzero round after
    /// quorum new-view evidence, durable round advancement and AOEM readback.
    /// Later certificates cannot replace the first admitted candidate/evidence.
    pub fn admit_local_new_view_candidate(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        authority: &NovNativeSealEpochAuthorityV1,
        certificate: &NovNativeSealNewViewCertificateV1,
        request: &NovNativeSealLocalProposalRequestV1,
    ) -> Result<bool> {
        authority.validate_against_ledger(ledger)?;
        if request.chain_id != authority.chain_id || request.round != certificate.context.round {
            bail!("native new-view admission request differs from certificate domain");
        }
        let context = context_v1(authority, certificate.context.height, request.round)?;
        let highest = certificate.verify(&context, authority)?;
        let binding = store_binding_v1(ledger, authority.chain_id)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&binding)?;
        self.ensure_new_view_authority_v1(authority, false)?;
        self.ensure_registered_validator_set_v1(&authority.validator_set)?;
        self.ensure_admission_active_context_v1(ledger, authority, &context)?;
        let subject = self.prepare_local_subject(
            ledger,
            request.chain_id,
            request.block_hash,
            &authority.validator_set,
            request.round,
            request.justify_qc_hash,
        )?;
        if subject.height != context.height {
            bail!("native new-view admission candidate has a different height");
        }
        if let Some(highest) = &highest {
            ensure_same_candidate(&subject, &highest.qc.subject)?;
            // Persist/import through the existing local-candidate verification
            // path first. An inline-only QC would otherwise disappear from the
            // next round's highest-locally-observed-QC inventory.
            self.ensure_admission_highest_qc_durable_v1(highest)?;
        }
        // Check every carried QC against the locally reconstructed candidate,
        // including lower QCs that the maximum-selection result would hide.
        for observation in &certificate.observations {
            if let Some(evidence) = &observation.highest_qc {
                self.verify_new_view_local_qc_v1(ledger, authority, &context, evidence)?;
                ensure_same_candidate(&subject, &evidence.qc.subject)?;
            }
        }
        if let Some(existing) = self.load_local_new_view_admission(
            subject.chain_id,
            subject.epoch,
            subject.height,
            subject.round,
        )? {
            if existing.subject != subject || existing.authority != *authority {
                bail!("native new-view slot is already admitted to a different candidate");
            }
            self.ensure_active_new_view_admission_v1(
                ledger,
                &subject,
                authority.scheduled_leader_v1(subject.height, subject.round)?,
                &authority.validator_set,
            )?;
            return Ok(false);
        }
        if self.new_view_admission_inventory_count_v1(&context)?
            >= NOV_NATIVE_BLOCK_SEAL_MAX_QCS_PER_INDEX_V1
        {
            bail!("native new-view admission would exceed bounded per-height recovery capacity");
        }
        self.verify_admission_inventory_v1(
            ledger,
            authority,
            &context,
            &subject,
            highest.as_ref(),
            false,
        )?;
        let mut record = NovNativeSealNewViewAdmissionV1 {
            schema: ADMISSION_SCHEMA_V1.into(),
            authority: authority.clone(),
            certificate: certificate.clone(),
            subject,
            admission_hash: [0; 32],
        };
        record.admission_hash = admission_hash(&record)?;
        let mut batch = RocksDbWriteBatch::default();
        self.stage_binding_and_validator_set_v1(&mut batch, &binding, &authority.validator_set)?;
        put_json_v1(
            &mut batch,
            authority_key(authority).as_bytes(),
            &authority.authority_commitment,
            "new-view authority",
        )?;
        put_json_v1(
            &mut batch,
            admission_key(
                record.subject.chain_id,
                record.subject.epoch,
                record.subject.height,
                record.subject.round,
            )
            .as_bytes(),
            &record,
            "new-view candidate admission",
        )?;
        write_sync_v1(&self.db, batch)?;
        if self
            .load_local_new_view_admission(
                record.subject.chain_id,
                record.subject.epoch,
                record.subject.height,
                record.subject.round,
            )?
            .as_ref()
            != Some(&record)
        {
            bail!("native new-view admission durable readback mismatch");
        }
        Ok(true)
    }

    fn ensure_admission_active_context_v1(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        authority: &NovNativeSealEpochAuthorityV1,
        context: &NovNativeSealTimeoutContextV1,
    ) -> Result<()> {
        let state = self
            .load_round_tracking(ledger, &authority.validator_set, context.height)?
            .context("native new-view admission requires durable round tracking")?;
        if state.current != *context {
            bail!("native new-view admission differs from durable active round");
        }
        let mut previous = context.clone();
        previous.round -= 1;
        state
            .previous_timeout
            .context("native new-view admission requires a preceding durable TC")?
            .verify(&previous, &authority.validator_set)?;
        Ok(())
    }

    // Must be called while holding the shared seal write lock. A same-round QC
    // can arrive after admission, but a late prior QC cannot be silently omitted.
    fn verify_admission_inventory_v1(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        authority: &NovNativeSealEpochAuthorityV1,
        context: &NovNativeSealTimeoutContextV1,
        subject: &NovNativeSealSubjectV1,
        highest: Option<&NovNativeSealNewViewQcV1>,
        allow_current: bool,
    ) -> Result<()> {
        for qc in self.new_view_qc_inventory_v1(context)? {
            self.ensure_qc_indexes_contain_v1(&qc)?;
            if qc.subject.round > context.round
                || (!allow_current && qc.subject.round == context.round)
            {
                bail!("native new-view admission inventory contains a current or future QC");
            }
            let proposal = self
                .load_proposal(qc.proposal_hash)?
                .context("native new-view admission inventory proposal is missing")?;
            if proposal.proposer_id
                != authority.scheduled_leader_v1(qc.subject.height, qc.subject.round)?
            {
                bail!("native new-view admission inventory proposal has the wrong leader");
            }
            let local = self.prepare_local_subject(
                ledger,
                qc.subject.chain_id,
                qc.subject.block_hash,
                &authority.validator_set,
                qc.subject.round,
                (qc.subject.justify_qc_hash != [0; 32]).then_some(qc.subject.justify_qc_hash),
            )?;
            if local != qc.subject {
                bail!("native new-view admission inventory QC differs from local candidate");
            }
            ensure_same_candidate(subject, &qc.subject)?;
            if qc.subject.round < context.round
                && highest.is_none_or(|reported| reported.qc.subject.round < qc.subject.round)
            {
                bail!("native new-view admission certificate omits a higher locally known QC");
            }
        }
        Ok(())
    }

    /// Historical evidence guard shared by all nonzero-round mutation and
    /// outbox-recovery paths. Does not turn historical replay into a new vote.
    pub(in crate::native_block_seal) fn ensure_new_view_admission_v1(
        &self,
        subject: &NovNativeSealSubjectV1,
        proposer_id: [u8; 32],
        set: &NovNativeSealValidatorSetV1,
    ) -> Result<()> {
        if subject.round == 0 {
            return Ok(());
        }
        let record = self
            .load_local_new_view_admission(
                subject.chain_id,
                subject.epoch,
                subject.height,
                subject.round,
            )?
            .context("nonzero native seal round requires durable new-view candidate admission")?;
        if record.subject != *subject || record.authority.validator_set != *set {
            bail!("native seal subject differs from durable new-view admission");
        }
        if proposer_id
            != record
                .authority
                .scheduled_leader_v1(subject.height, subject.round)?
        {
            bail!("native seal nonzero-round proposal signer is not the scheduled leader");
        }
        Ok(())
    }

    /// Before any new local signature (including retries), require the durable
    /// active round and recheck local evidence that may have arrived meanwhile.
    pub(in crate::native_block_seal) fn ensure_active_new_view_admission_v1(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        subject: &NovNativeSealSubjectV1,
        proposer_id: [u8; 32],
        set: &NovNativeSealValidatorSetV1,
    ) -> Result<()> {
        if subject.round == 0 {
            return Ok(());
        }
        self.ensure_new_view_admission_v1(subject, proposer_id, set)?;
        let record = self
            .load_local_new_view_admission(
                subject.chain_id,
                subject.epoch,
                subject.height,
                subject.round,
            )?
            .context("native new-view admission disappeared")?;
        record.authority.validate_against_ledger(ledger)?;
        self.ensure_store_binding_v1(&store_binding_v1(ledger, subject.chain_id)?)?;
        let context = &record.certificate.context;
        self.ensure_admission_active_context_v1(ledger, &record.authority, context)?;
        let highest = record.certificate.verify(context, &record.authority)?;
        self.verify_admission_inventory_v1(
            ledger,
            &record.authority,
            context,
            subject,
            highest.as_ref(),
            true,
        )
    }
}
