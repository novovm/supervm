//! Local-only durable new-view observations; never a candidate unlock path.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableObservation {
    observation: NovNativeSealNewViewObservationV1,
    previous_timeout: NovNativeSealTimeoutCertificateV1,
}

fn prefix(context: &NovNativeSealTimeoutContextV1, signer: [u8; 32]) -> String {
    format!(
        "native_block_seal/v1/new-view/{}/{}/{}/",
        context.chain_id,
        context.epoch,
        hex_v1(&signer)
    )
}

fn observation_key(context: &NovNativeSealTimeoutContextV1, signer: [u8; 32]) -> String {
    format!(
        "{}observation/{}/{}",
        prefix(context, signer),
        context.height,
        context.round
    )
}

pub(super) fn authority_key(authority: &NovNativeSealEpochAuthorityV1) -> String {
    format!(
        "native_block_seal/v1/new-view-authority/{}/{}",
        authority.chain_id, authority.epoch
    )
}

pub(super) fn context_v1(
    authority: &NovNativeSealEpochAuthorityV1,
    height: u64,
    round: u64,
) -> Result<NovNativeSealTimeoutContextV1> {
    let context = NovNativeSealTimeoutContextV1 {
        chain_id: authority.chain_id,
        genesis_block_hash: authority.genesis_block_hash,
        protocol_config_commitment: authority.protocol_config_commitment,
        epoch: authority.epoch,
        validator_set_hash: authority.validator_set.validator_set_hash,
        height,
        round,
    };
    validate_context_v1(&context, authority)?;
    Ok(context)
}

impl NovNativeBlockSealStoreV1 {
    /// Validate retained admission references and return their bounded count.
    /// Call under the shared write lock for signature and capacity decisions.
    pub(super) fn new_view_admission_inventory_count_v1(
        &self,
        context: &NovNativeSealTimeoutContextV1,
    ) -> Result<usize> {
        // A retained admission is also durable knowledge of its highest QC.
        // Recheck those references even if both a QC object and all secondary
        // indexes disappeared; absence must not become a signed "no QC".
        let admission_prefix = format!(
            "native_block_seal/v1/new-view-admission/{}/{}/{}/",
            context.chain_id, context.epoch, context.height
        );
        let mut admission_count = 0usize;
        for item in self.db.iterator(IteratorMode::From(
            admission_prefix.as_bytes(),
            Direction::Forward,
        )) {
            let (key, value) = item.context("scan new-view admission QC references")?;
            if !key.starts_with(admission_prefix.as_bytes()) {
                break;
            }
            admission_count += 1;
            if admission_count > NOV_NATIVE_BLOCK_SEAL_MAX_QCS_PER_INDEX_V1 {
                bail!("new-view admission inventory exceeds bounded per-height recovery scan");
            }
            let record: NovNativeSealNewViewAdmissionV1 =
                serde_json::from_slice(&value).context("decode new-view admission reference")?;
            let subject = &record.subject;
            if (subject.chain_id, subject.epoch, subject.height)
                != (context.chain_id, context.epoch, context.height)
                || key.as_ref() != format!("{admission_prefix}{}", subject.round).as_bytes()
            {
                bail!("new-view admission inventory object/key mismatch");
            }
            if self
                .load_local_new_view_admission(
                    context.chain_id,
                    context.epoch,
                    context.height,
                    subject.round,
                )?
                .as_ref()
                != Some(&record)
            {
                bail!("new-view admission inventory failed durable reference verification");
            }
        }
        Ok(admission_count)
    }

    /// Until a separately authenticated per-height inventory is available,
    /// cross-check the secondary index against a bounded scan of durable QC
    /// objects. A missing/truncated index must not become a signed "no QC".
    /// Called under the shared seal write lock, before any new signature.
    pub(super) fn new_view_qc_inventory_v1(
        &self,
        context: &NovNativeSealTimeoutContextV1,
    ) -> Result<Vec<NovNativeSealQuorumCertificateV1>> {
        self.new_view_admission_inventory_count_v1(context)?;
        let indexed = self.load_qcs_by_height(context.chain_id, context.epoch, context.height)?;
        let indexed_hashes = indexed.iter().map(|qc| qc.qc_hash).collect::<BTreeSet<_>>();
        let prefix = format!("{KEY_PREFIX_V1}qc/object/");
        let mut scanned = 0usize;
        let mut stored_hashes = BTreeSet::new();
        for item in self
            .db
            .iterator(IteratorMode::From(prefix.as_bytes(), Direction::Forward))
        {
            let (key, value) = item.context("scan new-view durable QC inventory")?;
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }
            scanned += 1;
            if scanned > NOV_NATIVE_BLOCK_SEAL_MAX_QCS_PER_INDEX_V1 {
                bail!("new-view QC inventory exceeds bounded recovery scan; signature refused");
            }
            let qc: NovNativeSealQuorumCertificateV1 =
                serde_json::from_slice(&value).context("decode new-view inventory QC")?;
            if key.as_ref() != qc_object_key_v1(&qc.qc_hash).as_bytes() {
                bail!("new-view QC inventory object/key mismatch");
            }
            // Verify before filtering: corrupted slot metadata must not hide a
            // target-height QC by making it appear to belong to another slot.
            if self.load_qc(qc.qc_hash)?.as_ref() != Some(&qc) {
                bail!("new-view QC inventory object failed durable verification");
            }
            if qc.subject.chain_id == context.chain_id
                && qc.subject.epoch == context.epoch
                && qc.subject.height == context.height
            {
                stored_hashes.insert(qc.qc_hash);
            }
        }
        if stored_hashes != indexed_hashes {
            bail!("new-view QC height index differs from durable object inventory");
        }
        Ok(indexed)
    }

    pub(super) fn ensure_new_view_authority_v1(
        &self,
        authority: &NovNativeSealEpochAuthorityV1,
        required: bool,
    ) -> Result<()> {
        let pinned = read_json_v1::<[u8; 32]>(
            &self.db,
            authority_key(authority).as_bytes(),
            "new-view pinned authority",
        )?;
        if pinned.is_some_and(|pin| pin != authority.authority_commitment)
            || (required && pinned.is_none())
        {
            bail!("new-view durable authority missing or mismatched");
        }
        Ok(())
    }

    fn validate_new_view_record_v1(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        authority: &NovNativeSealEpochAuthorityV1,
        expected: &NovNativeSealTimeoutContextV1,
        signer: [u8; 32],
        record: &DurableObservation,
    ) -> Result<()> {
        self.ensure_new_view_authority_v1(authority, true)?;
        self.ensure_registered_validator_set_v1(&authority.validator_set)?;
        record.observation.verify(expected, authority)?;
        if record.observation.validator_id != signer {
            bail!("new-view durable signer mismatch");
        }
        let mut previous = expected.clone();
        previous.round -= 1;
        record
            .previous_timeout
            .verify(&previous, &authority.validator_set)?;
        if let Some(evidence) = &record.observation.highest_qc {
            let stored = self
                .load_qc(evidence.qc.qc_hash)?
                .context("new-view references a missing durable QC")?;
            if stored != evidence.qc {
                bail!("new-view durable QC differs from signed snapshot");
            }
            self.ensure_qc_indexes_contain_v1(&stored)?;
            self.verify_new_view_local_qc_v1(ledger, authority, expected, evidence)?;
        }
        Ok(())
    }

    pub(super) fn verify_new_view_local_qc_v1(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        authority: &NovNativeSealEpochAuthorityV1,
        context: &NovNativeSealTimeoutContextV1,
        evidence: &NovNativeSealNewViewQcV1,
    ) -> Result<()> {
        evidence.verify(context, authority)?;
        let subject = &evidence.qc.subject;
        let expected = self.prepare_local_subject(
            ledger,
            subject.chain_id,
            subject.block_hash,
            &authority.validator_set,
            subject.round,
            (subject.justify_qc_hash != [0; 32]).then_some(subject.justify_qc_hash),
        )?;
        if expected != *subject {
            bail!("new-view QC does not match the local AOEM-owned candidate");
        }
        Ok(())
    }

    fn load_new_view_watermark_v1(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        authority: &NovNativeSealEpochAuthorityV1,
        context: &NovNativeSealTimeoutContextV1,
        signer: [u8; 32],
    ) -> Result<Option<DurableObservation>> {
        let watermark = read_json_v1::<DurableObservation>(
            &self.db,
            format!("{}watermark", prefix(context, signer)).as_bytes(),
            "new-view signer watermark",
        )?;
        if let Some(record) = &watermark {
            let old_context = context_v1(
                authority,
                record.observation.context.height,
                record.observation.context.round,
            )?;
            self.validate_new_view_record_v1(ledger, authority, &old_context, signer, record)?;
            let stored = read_json_v1::<DurableObservation>(
                &self.db,
                observation_key(&old_context, signer).as_bytes(),
                "watermarked new-view observation",
            )?;
            if stored.as_ref() != Some(record) {
                bail!("new-view watermark points to a missing or different observation");
            }
        }
        Ok(watermark)
    }

    /// Read-only recovery of the original signed snapshot, not a claim that its
    /// highest QC is still the latest. Does not re-sign after new QCs arrive.
    pub fn load_local_new_view(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        authority: &NovNativeSealEpochAuthorityV1,
        height: u64,
        target_round: u64,
        validator_id: [u8; 32],
    ) -> Result<Option<NovNativeSealNewViewObservationV1>> {
        authority.validate_against_ledger(ledger)?;
        let context = context_v1(authority, height, target_round)?;
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&store_binding_v1(ledger, authority.chain_id)?)?;
        self.ensure_new_view_authority_v1(authority, false)?;
        let watermark =
            self.load_new_view_watermark_v1(ledger, authority, &context, validator_id)?;
        let record = read_json_v1::<DurableObservation>(
            &self.db,
            observation_key(&context, validator_id).as_bytes(),
            "new-view observation",
        )?;
        if let Some(record) = record {
            self.validate_new_view_record_v1(ledger, authority, &context, validator_id, &record)?;
            if watermark.as_ref().is_none_or(|w| {
                (w.observation.context.height, w.observation.context.round) < (height, target_round)
            }) {
                bail!("new-view observation has no covering signer watermark");
            }
            return Ok(Some(record.observation));
        }
        Ok(None)
    }

    /// Opt-in local scheduling only. The preceding TC must already have advanced
    /// the durable tracker. This records highest *locally observed* prepare QC;
    /// it neither selects a safe proposal nor changes any candidate signing lock.
    pub fn sign_local_new_view(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        authority: &NovNativeSealEpochAuthorityV1,
        height: u64,
        target_round: u64,
        key: &SigningKey,
    ) -> Result<NovNativeSealNewViewObservationV1> {
        authority.validate_against_ledger(ledger)?;
        let context = context_v1(authority, height, target_round)?;
        let set = &authority.validator_set;
        let signer = validator_id_v1(key.verifying_key().as_bytes());
        if set.validator(signer).is_none() {
            bail!("new-view signer is not a validator");
        }
        let binding = store_binding_v1(ledger, authority.chain_id)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&binding)?;
        self.ensure_new_view_authority_v1(authority, false)?;
        let state = self
            .load_round_tracking(ledger, set, height)?
            .context("new-view requires durable round tracking")?;
        if state.current != context {
            bail!("new-view target differs from the durable active round");
        }
        let previous_timeout = state
            .previous_timeout
            .context("new-view requires preceding timeout certificate")?;
        let watermark = self.load_new_view_watermark_v1(ledger, authority, &context, signer)?;
        if let Some(existing) =
            self.load_local_new_view(ledger, authority, height, target_round, signer)?
        {
            // Returning an already durable signature does not authorize a new
            // proposal, even if this signer has since timed out the same slot.
            return Ok(existing);
        }
        self.ensure_timeout_signer_active_v1(&context, signer, set)?;
        if watermark.as_ref().is_some_and(|w| {
            (w.observation.context.height, w.observation.context.round) >= (height, target_round)
        }) {
            bail!("new-view cannot regress or replace a missing signed observation");
        }
        let mut evidence = Vec::new();
        for qc in self.new_view_qc_inventory_v1(&context)? {
            self.ensure_qc_indexes_contain_v1(&qc)?;
            let proposal = self
                .load_proposal(qc.proposal_hash)?
                .context("new-view QC proposal is missing")?;
            let item = NovNativeSealNewViewQcV1 { proposal, qc };
            // A QC at/after the requested round is inconsistent with entering
            // that round: do not omit it and sign a lower or empty observation.
            self.verify_new_view_local_qc_v1(ledger, authority, &context, &item)?;
            evidence.push(item);
        }
        let mut observation = NovNativeSealNewViewObservationV1 {
            schema: OBSERVATION_SCHEMA_V1.into(),
            authority_commitment: authority.authority_commitment,
            context,
            highest_qc: select_highest_v1(&evidence)?,
            validator_id: signer,
            signature: Vec::new(),
        };
        observation.signature = key.sign(&observation.message()).to_bytes().to_vec();
        observation.verify(&observation.context, authority)?;
        let record = DurableObservation {
            observation,
            previous_timeout,
        };
        let mut batch = RocksDbWriteBatch::default();
        self.stage_binding_and_validator_set_v1(&mut batch, &binding, set)?;
        put_json_v1(
            &mut batch,
            authority_key(authority).as_bytes(),
            &authority.authority_commitment,
            "new-view authority",
        )?;
        put_json_v1(
            &mut batch,
            observation_key(&record.observation.context, signer).as_bytes(),
            &record,
            "new-view observation",
        )?;
        put_json_v1(
            &mut batch,
            format!("{}watermark", prefix(&record.observation.context, signer)).as_bytes(),
            &record,
            "new-view watermark",
        )?;
        write_sync_v1(&self.db, batch)?;
        let stored = self.load_local_new_view(ledger, authority, height, target_round, signer)?;
        if stored.as_ref() != Some(&record.observation) {
            bail!("new-view durable signature readback mismatch");
        }
        Ok(record.observation)
    }
}
