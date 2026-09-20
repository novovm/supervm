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
    /// Recovers the original durable timeout without signing or authorizing the
    /// active round. A scheduler may retransmit it after restart immediately;
    /// absence of a record does not manufacture elapsed local timer time.
    pub fn load_local_timeout(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        set: &NovNativeSealValidatorSetV1,
        height: u64,
        round: u64,
        validator_id: [u8; 32],
    ) -> Result<Option<NovNativeSealTimeoutVoteV1>> {
        let mut expected = local_context(ledger, set, height)?;
        expected.round = round;
        expected.validate(set)?;
        if set.validator(validator_id).is_none() {
            bail!("timeout recovery signer is not a validator");
        }
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&store_binding_v1(ledger, set.chain_id)?)?;
        self.ensure_registered_validator_set_v1(set)?;
        let watermark = load_watermark(self, set.chain_id, set.epoch, validator_id, set)?;
        if watermark.as_ref().is_some_and(|record| {
            record.context.genesis_block_hash != expected.genesis_block_hash
                || record.context.protocol_config_commitment != expected.protocol_config_commitment
        }) {
            bail!("timeout recovery watermark network domain mismatch");
        }
        let record = read_json_v1::<NovNativeSealTimeoutVoteV1>(
            &self.db,
            format!(
                "{}vote/{height}/{round}",
                prefix(set.chain_id, set.epoch, validator_id)
            )
            .as_bytes(),
            "recovered timeout vote",
        )?;
        if let Some(record) = &record {
            record.verify(set)?;
            if record.context != expected
                || record.validator_id != validator_id
                || watermark.as_ref().is_none_or(|watermark| {
                    (watermark.context.height, watermark.context.round) < (height, round)
                })
            {
                bail!("timeout recovery does not match durable safety state");
            }
        }
        Ok(record)
    }

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
        self.ensure_tracked_round_v1(&context, set)?;
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
        let context = NovNativeSealTimeoutContextV1 {
            chain_id: subject.chain_id,
            genesis_block_hash: subject.genesis_block_hash,
            protocol_config_commitment: subject.protocol_config_commitment,
            epoch: subject.epoch,
            validator_set_hash: subject.validator_set_hash,
            height: subject.height,
            round: subject.round,
        };
        self.ensure_timeout_signer_active_v1(&context, signer, set)
    }

    pub(super) fn ensure_timeout_signer_active_v1(
        &self,
        context: &NovNativeSealTimeoutContextV1,
        signer: [u8; 32],
        set: &NovNativeSealValidatorSetV1,
    ) -> Result<()> {
        context.validate(set)?;
        self.ensure_tracked_round_v1(context, set)?;
        if let Some(previous) = load_watermark(self, context.chain_id, context.epoch, signer, set)?
        {
            if previous.context.genesis_block_hash != context.genesis_block_hash
                || previous.context.protocol_config_commitment != context.protocol_config_commitment
                || (context.height, context.round)
                    <= (previous.context.height, previous.context.round)
            {
                bail!("native seal signer has already timed out this height/round");
            }
        }
        Ok(())
    }
}

const ROUND_STATE_SCHEMA: &str = "novovm-native-seal-round-state/v1";

/// Durable local round, not a canonical chain head or an unlock certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeSealRoundStateV1 {
    pub schema: String,
    pub current: NovNativeSealTimeoutContextV1,
    pub previous_timeout: Option<NovNativeSealTimeoutCertificateV1>,
}

fn round_key(chain: u64, epoch: u64, height: u64) -> String {
    format!("native_block_seal/v1/round-state/{chain}/{epoch}/{height}")
}

fn local_context(
    ledger: &NovNativeBlockLedgerV1,
    set: &NovNativeSealValidatorSetV1,
    height: u64,
) -> Result<NovNativeSealTimeoutContextV1> {
    let binding = store_binding_v1(ledger, set.chain_id)?;
    let context = NovNativeSealTimeoutContextV1 {
        chain_id: set.chain_id,
        genesis_block_hash: binding.genesis_block_hash,
        protocol_config_commitment: binding.protocol_config_commitment,
        epoch: set.epoch,
        validator_set_hash: set.validator_set_hash,
        height,
        round: 0,
    };
    context.validate(set)?;
    let head = ledger
        .load_head(set.chain_id)?
        .context("round state requires local head")?;
    if height > head.height.saturating_add(1) {
        bail!("round height too far ahead");
    }
    Ok(context)
}

impl NovNativeBlockSealStoreV1 {
    fn read_round_state_v1(
        &self,
        expected: &NovNativeSealTimeoutContextV1,
        set: &NovNativeSealValidatorSetV1,
    ) -> Result<Option<NovNativeSealRoundStateV1>> {
        expected.validate(set)?;
        let state = read_json_v1::<NovNativeSealRoundStateV1>(
            &self.db,
            round_key(expected.chain_id, expected.epoch, expected.height).as_bytes(),
            "round state",
        )?;
        if let Some(state) = &state {
            state.current.validate(set)?;
            let mut pinned = expected.clone();
            pinned.round = state.current.round;
            if state.schema != ROUND_STATE_SCHEMA || state.current != pinned {
                bail!("round state domain mismatch");
            }
            match &state.previous_timeout {
                None if state.current.round == 0 => (),
                Some(tc) if state.current.round > 0 => {
                    pinned.round -= 1;
                    tc.verify(&pinned, set)?;
                }
                _ => bail!("round state lacks the preceding timeout certificate"),
            }
        }
        Ok(state)
    }

    /// Explicit opt-in for a local scheduler. Never resets existing round state.
    pub fn start_round_tracking(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        set: &NovNativeSealValidatorSetV1,
        height: u64,
    ) -> Result<NovNativeSealRoundStateV1> {
        let context = local_context(ledger, set, height)?;
        let binding = store_binding_v1(ledger, set.chain_id)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&binding)?;
        if let Some(state) = self.read_round_state_v1(&context, set)? {
            return Ok(state);
        }
        let state = NovNativeSealRoundStateV1 {
            schema: ROUND_STATE_SCHEMA.into(),
            current: context,
            previous_timeout: None,
        };
        let mut batch = RocksDbWriteBatch::default();
        self.stage_binding_and_validator_set_v1(&mut batch, &binding, set)?;
        put_json_v1(
            &mut batch,
            round_key(set.chain_id, set.epoch, height).as_bytes(),
            &state,
            "round state",
        )?;
        write_sync_v1(&self.db, batch)?;
        let stored = self.read_round_state_v1(&state.current, set)?;
        if stored.as_ref() != Some(&state) {
            bail!("round initialization readback mismatch");
        }
        Ok(state)
    }

    pub fn load_round_tracking(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        set: &NovNativeSealValidatorSetV1,
        height: u64,
    ) -> Result<Option<NovNativeSealRoundStateV1>> {
        let context = local_context(ledger, set, height)?;
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&store_binding_v1(ledger, set.chain_id)?)?;
        self.read_round_state_v1(&context, set)
    }

    /// Only sequential timeout certificates advance this local tracker. Keeps
    /// every existing candidate height lock; it never chooses a safe proposal.
    pub fn advance_round_tracking(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        set: &NovNativeSealValidatorSetV1,
        tc: &NovNativeSealTimeoutCertificateV1,
    ) -> Result<NovNativeSealRoundStateV1> {
        let mut expected = local_context(ledger, set, tc.context.height)?;
        expected.round = tc.context.round;
        tc.verify(&expected, set)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        self.ensure_store_binding_v1(&store_binding_v1(ledger, set.chain_id)?)?;
        self.ensure_registered_validator_set_v1(set)?;
        let mut state = self
            .read_round_state_v1(&expected, set)?
            .context("round tracking not initialized")?;
        if tc.context.round < state.current.round {
            return Ok(state);
        }
        if tc.context.round != state.current.round {
            bail!("timeout certificate skips a local round");
        }
        let next = state
            .current
            .round
            .checked_add(1)
            .context("round overflow")?;
        if next == u64::MAX {
            bail!("round limit reached");
        }
        state.current.round = next;
        state.previous_timeout = Some(tc.clone());
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            round_key(set.chain_id, set.epoch, state.current.height).as_bytes(),
            &state,
            "round state",
        )?;
        write_sync_v1(&self.db, batch)?;
        if self.read_round_state_v1(&expected, set)?.as_ref() != Some(&state) {
            bail!("round advance readback mismatch");
        }
        Ok(state)
    }

    fn ensure_tracked_round_v1(
        &self,
        context: &NovNativeSealTimeoutContextV1,
        set: &NovNativeSealValidatorSetV1,
    ) -> Result<()> {
        if let Some(state) = self.read_round_state_v1(context, set)? {
            if state.current.round != context.round {
                bail!("signature does not match durable active round");
            }
        }
        Ok(())
    }
}

/// Process-local monotonic timer. On restart, construct from the loaded durable
/// state and wait a fresh interval; wall-clock changes never expire it early.
/// An owner polls this timer and sends the returned persisted vote. No network
/// callback or remote timeout is allowed to manufacture local elapsed time.
pub struct NovNativeSealRoundTimerV1 {
    context: NovNativeSealTimeoutContextV1,
    started: std::time::Instant,
    interval: std::time::Duration,
}
impl NovNativeSealRoundTimerV1 {
    pub fn new(
        state: &NovNativeSealRoundStateV1,
        now: std::time::Instant,
        interval: std::time::Duration,
    ) -> Result<Self> {
        if interval.is_zero() || interval > std::time::Duration::from_secs(300) {
            bail!("round timer interval must be positive and at most 300 seconds");
        }
        Ok(Self {
            context: state.current.clone(),
            started: now,
            interval,
        })
    }
    pub fn poll(
        &self,
        now: std::time::Instant,
        store: &NovNativeBlockSealStoreV1,
        ledger: &NovNativeBlockLedgerV1,
        set: &NovNativeSealValidatorSetV1,
        key: &SigningKey,
    ) -> Result<Option<NovNativeSealTimeoutVoteV1>> {
        let state = store
            .load_round_tracking(ledger, set, self.context.height)?
            .context("round timer has no durable state")?;
        if state.current != self.context {
            return Ok(None);
        }
        if now
            .checked_duration_since(self.started)
            .is_none_or(|d| d < self.interval)
        {
            return Ok(None);
        }
        Ok(Some(store.sign_local_timeout(
            ledger,
            set,
            self.context.height,
            self.context.round,
            key,
        )?))
    }
}
