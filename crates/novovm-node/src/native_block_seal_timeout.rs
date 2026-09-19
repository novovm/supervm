//! Durable timeout observations. These certificates do not unlock a candidate,
//! select a leader, advance a round, or grant finality. A future pacemaker must
//! supply independently verified safe-proposal rules before using them.
use super::*;
use std::collections::BTreeSet;

const TIMEOUT_DOMAIN: &[u8] = b"novovm-native-seal-timeout-observation-v1\0";
const TIMEOUT_SCHEMA: &str = "novovm-native-seal-timeout-observation/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeSealTimeoutContextV1 {
    pub chain_id: u64,
    pub genesis_block_hash: [u8; 32],
    pub protocol_config_commitment: [u8; 32],
    pub epoch: u64,
    pub validator_set_hash: [u8; 32],
    pub height: u64,
    pub round: u64,
}

impl NovNativeSealTimeoutContextV1 {
    fn validate(&self, set: &NovNativeSealValidatorSetV1) -> Result<()> {
        set.validate()?;
        if self.chain_id != set.chain_id
            || self.epoch != set.epoch
            || self.validator_set_hash != set.validator_set_hash
            || self.height < set.activation_height
            || self.round == u64::MAX
            || self.genesis_block_hash == [0; 32]
            || self.protocol_config_commitment == [0; 32]
        {
            bail!("invalid native timeout context");
        }
        Ok(())
    }
    fn message(&self, signer: [u8; 32]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(TIMEOUT_DOMAIN);
        h.update(self.chain_id.to_be_bytes());
        h.update(self.genesis_block_hash);
        h.update(self.protocol_config_commitment);
        h.update(self.epoch.to_be_bytes());
        h.update(self.validator_set_hash);
        h.update(self.height.to_be_bytes());
        h.update(self.round.to_be_bytes());
        h.update(signer);
        h.finalize().into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeSealTimeoutVoteV1 {
    pub schema: String,
    pub context: NovNativeSealTimeoutContextV1,
    pub validator_id: [u8; 32],
    pub signature: Vec<u8>,
}
impl NovNativeSealTimeoutVoteV1 {
    pub fn verify(&self, set: &NovNativeSealValidatorSetV1) -> Result<()> {
        self.context.validate(set)?;
        if self.schema != TIMEOUT_SCHEMA {
            bail!("invalid timeout schema");
        }
        let member = set
            .validator(self.validator_id)
            .context("timeout signer is not a validator")?;
        VerifyingKey::from_bytes(&member.public_key)?
            .verify_strict(
                &self.context.message(self.validator_id),
                &Signature::from_slice(&self.signature)?,
            )
            .context("invalid timeout signature")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeSealTimeoutCertificateV1 {
    pub context: NovNativeSealTimeoutContextV1,
    pub votes: Vec<NovNativeSealTimeoutVoteV1>,
}
impl NovNativeSealTimeoutCertificateV1 {
    /// Caller supplies the pinned expected domain/slot, never trusts the wire
    /// object's own genesis/config/height as authority.
    pub fn verify(
        &self,
        expected: &NovNativeSealTimeoutContextV1,
        set: &NovNativeSealValidatorSetV1,
    ) -> Result<u64> {
        expected.validate(set)?;
        if &self.context != expected
            || self.votes.is_empty()
            || self.votes.len() > set.validators.len()
        {
            bail!("timeout certificate domain or size mismatch");
        }
        let mut signers = BTreeSet::new();
        let mut weight = 0u64;
        for vote in &self.votes {
            if vote.context != self.context || !signers.insert(vote.validator_id) {
                bail!("mixed timeout context or duplicate signer");
            }
            vote.verify(set)?;
            weight = weight
                .checked_add(set.validator(vote.validator_id).unwrap().weight)
                .context("timeout weight overflow")?;
        }
        if weight < set.quorum_weight {
            bail!("insufficient timeout weight");
        }
        Ok(weight)
    }
}

fn prefix(chain: u64, epoch: u64, signer: [u8; 32]) -> String {
    format!(
        "native_block_seal/v1/timeout/{chain}/{epoch}/{}/",
        hex_v1(&signer)
    )
}
fn load_watermark(
    store: &NovNativeBlockSealStoreV1,
    chain: u64,
    epoch: u64,
    signer: [u8; 32],
    set: &NovNativeSealValidatorSetV1,
) -> Result<Option<NovNativeSealTimeoutVoteV1>> {
    let vote = read_json_v1::<NovNativeSealTimeoutVoteV1>(
        &store.db,
        format!("{}watermark", prefix(chain, epoch, signer)).as_bytes(),
        "timeout watermark",
    )?;
    if let Some(v) = &vote {
        v.verify(set)?;
        if v.validator_id != signer {
            bail!("timeout watermark signer mismatch");
        }
        let vote_key = format!(
            "{}vote/{}/{}",
            prefix(chain, epoch, signer),
            v.context.height,
            v.context.round
        );
        if read_json_v1::<NovNativeSealTimeoutVoteV1>(
            &store.db,
            vote_key.as_bytes(),
            "watermarked timeout vote",
        )?
        .as_ref()
            != Some(v)
        {
            bail!("timeout watermark points to a missing or different vote");
        }
    }
    Ok(vote)
}

impl NovNativeBlockSealStoreV1 {
    /// Persists the signed observation and monotonic signer watermark in one
    /// synchronous batch. The vote key is also the durable retransmission item;
    /// calling with the same slot after restart returns the identical vote.
    /// Only a locally supplied scheduler may call this, not an RPC/remote timer.
    pub fn sign_local_timeout(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        set: &NovNativeSealValidatorSetV1,
        height: u64,
        round: u64,
        key: &SigningKey,
    ) -> Result<NovNativeSealTimeoutVoteV1> {
        set.validate()?;
        let binding = store_binding_v1(ledger, set.chain_id)?;
        let context = NovNativeSealTimeoutContextV1 {
            chain_id: set.chain_id,
            genesis_block_hash: binding.genesis_block_hash,
            protocol_config_commitment: binding.protocol_config_commitment,
            epoch: set.epoch,
            validator_set_hash: set.validator_set_hash,
            height,
            round,
        };
        context.validate(set)?;
        let head = ledger
            .load_head(set.chain_id)?
            .context("timeout requires a local head")?;
        if height > head.height.saturating_add(1) {
            bail!("timeout height too far ahead");
        }
        let signer = validator_id_v1(key.verifying_key().as_bytes());
        if set.validator(signer).is_none() {
            bail!("timeout signer is not a validator");
        }
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&binding)?;
        let base = prefix(set.chain_id, set.epoch, signer);
        let vote_key = format!("{base}vote/{height}/{round}");
        let previous = load_watermark(self, set.chain_id, set.epoch, signer, set)?;
        if previous.as_ref().is_some_and(|p| {
            p.context.genesis_block_hash != context.genesis_block_hash
                || p.context.protocol_config_commitment != context.protocol_config_commitment
        }) {
            bail!("timeout watermark network domain mismatch");
        }
        if let Some(existing) = read_json_v1::<NovNativeSealTimeoutVoteV1>(
            &self.db,
            vote_key.as_bytes(),
            "timeout vote",
        )? {
            existing.verify(set)?;
            self.ensure_registered_validator_set_v1(set)?;
            if existing.context != context
                || existing.validator_id != signer
                || previous
                    .as_ref()
                    .is_none_or(|p| (p.context.height, p.context.round) < (height, round))
            {
                bail!("timeout replay does not match durable safety state");
            }
            return Ok(existing);
        }
        if previous
            .as_ref()
            .is_some_and(|p| (p.context.height, p.context.round) >= (height, round))
        {
            bail!("timeout cannot regress or replace a missing signed vote");
        }
        let vote = NovNativeSealTimeoutVoteV1 {
            schema: TIMEOUT_SCHEMA.into(),
            signature: key.sign(&context.message(signer)).to_bytes().to_vec(),
            context,
            validator_id: signer,
        };
        vote.verify(set)?;
        let mut batch = RocksDbWriteBatch::default();
        self.stage_binding_and_validator_set_v1(&mut batch, &binding, set)?;
        put_json_v1(&mut batch, vote_key.as_bytes(), &vote, "timeout vote")?;
        put_json_v1(
            &mut batch,
            format!("{base}watermark").as_bytes(),
            &vote,
            "timeout watermark",
        )?;
        write_sync_v1(&self.db, batch)?;
        let stored = read_json_v1::<NovNativeSealTimeoutVoteV1>(
            &self.db,
            vote_key.as_bytes(),
            "timeout vote",
        )?;
        if stored.as_ref() != Some(&vote)
            || load_watermark(self, set.chain_id, set.epoch, signer, set)?.as_ref() != Some(&vote)
        {
            bail!("timeout durable readback mismatch");
        }
        Ok(vote)
    }

    pub(super) fn ensure_not_timed_out_v1(
        &self,
        subject: &NovNativeSealSubjectV1,
        signer: [u8; 32],
        set: &NovNativeSealValidatorSetV1,
    ) -> Result<()> {
        if let Some(previous) = load_watermark(self, subject.chain_id, subject.epoch, signer, set)?
        {
            if (subject.height, subject.round) <= (previous.context.height, previous.context.round)
            {
                bail!("native seal signer has already timed out this height/round");
            }
        }
        Ok(())
    }
}
