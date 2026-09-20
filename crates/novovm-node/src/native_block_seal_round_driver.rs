//! Opt-in, single-height prepare coordinator. No sockets, finality or lock migration.
//! Only `poll` may emit local signatures; ingress only verifies and collects evidence.
use super::newview::{NovNativeSealNewViewCertificateV1, NovNativeSealNewViewObservationV1};
use super::round_message::NovNativeSealRoundMessageV1 as Message;
use super::timeout::{
    NovNativeSealRoundStateV1, NovNativeSealRoundTimerV1, NovNativeSealTimeoutCertificateV1,
    NovNativeSealTimeoutVoteV1,
};
use super::*;
use crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NovNativeSealRoundDriverPhaseV1 {
    WaitingProposal,
    CollectingTimeouts,
    CollectingNewViews,
    CollectingVotes,
    Prepared,
    Halted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NovNativeSealRoundDriverStatusV1 {
    pub height: u64,
    pub round: u64,
    pub leader_id: [u8; 32],
    pub phase: NovNativeSealRoundDriverPhaseV1,
    pub prepared: bool,
    pub qc_hash: Option<[u8; 32]>,
    pub finalized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DriverBinding {
    schema: String,
    authority: NovNativeSealEpochAuthorityV1,
    height: u64,
    block_hash: [u8; 32],
    justify_qc_hash: Option<[u8; 32]>,
    local_validator_id: [u8; 32],
}

/// One owner, one signer identity, one already locally executed candidate.
/// Remote caches are bounded by the pinned validator set and may be recollected.
/// Keys are never stored here; the local scheduler supplies its key to `poll`.
pub struct NovNativeSealRoundDriverV1 {
    binding: DriverBinding,
    store_path: PathBuf,
    ledger_path: PathBuf,
    state: NovNativeSealRoundStateV1,
    leader_id: [u8; 32],
    interval: Duration,
    timer: NovNativeSealRoundTimerV1,
    last_poll: Instant,
    timeouts: BTreeMap<[u8; 32], NovNativeSealTimeoutVoteV1>,
    pending_timeout: Option<NovNativeSealTimeoutCertificateV1>,
    observations: BTreeMap<[u8; 32], NovNativeSealNewViewObservationV1>,
    certificate: Option<NovNativeSealNewViewCertificateV1>,
    proposal: Option<NovNativeSealProposalV1>,
    votes: BTreeMap<[u8; 32], NovNativeSealVoteV1>,
    pending_qc: Option<NovNativeSealQuorumCertificateV1>,
    prepared: Option<NovNativeSealQuorumCertificateV1>,
    halted: bool,
}

impl NovNativeSealRoundDriverV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        authority: NovNativeSealEpochAuthorityV1,
        block_hash: [u8; 32],
        justify_qc_hash: Option<[u8; 32]>,
        local_validator_id: [u8; 32],
        now: Instant,
        interval: Duration,
    ) -> Result<Self> {
        authority.validate_against_ledger(ledger)?;
        if interval.is_zero() || interval > Duration::from_secs(300) {
            bail!("round driver interval must be positive and at most 300 seconds");
        }
        if authority
            .validator_set
            .validator(local_validator_id)
            .is_none()
        {
            bail!("round driver local identity is not a pinned validator");
        }
        let subject = store.prepare_local_subject(
            ledger,
            authority.chain_id,
            block_hash,
            &authority.validator_set,
            0,
            justify_qc_hash,
        )?;
        ensure_subject_budget(&subject)?;
        let binding = DriverBinding {
            schema: "novovm-native-seal-round-driver-binding/v1".into(),
            authority,
            height: subject.height,
            block_hash,
            justify_qc_hash,
            local_validator_id,
        };
        let key = Self::binding_key(&binding);
        {
            let _guard = store.lock_writes_v1()?;
            let expected = store_binding_v1(ledger, binding.authority.chain_id)?;
            store.ensure_schema_v1()?;
            store.ensure_store_binding_v1(&expected)?;
            match read_json_v1::<DriverBinding>(&store.db, key.as_bytes(), "round driver binding")?
            {
                Some(old) if old != binding => bail!("round driver durable configuration changed"),
                Some(_) => (),
                None => {
                    let mut batch = RocksDbWriteBatch::default();
                    store.stage_binding_and_validator_set_v1(
                        &mut batch,
                        &expected,
                        &binding.authority.validator_set,
                    )?;
                    put_json_v1(&mut batch, key.as_bytes(), &binding, "round driver binding")?;
                    write_sync_v1(&store.db, batch)?;
                }
            }
        }
        let state =
            store.start_round_tracking(ledger, &binding.authority.validator_set, binding.height)?;
        let leader_id = binding
            .authority
            .scheduled_leader_v1(binding.height, state.current.round)?;
        let timer = NovNativeSealRoundTimerV1::new(&state, now, interval)?;
        let mut driver = Self {
            binding,
            store_path: fs::canonicalize(store.path())?,
            ledger_path: fs::canonicalize(ledger.path())?,
            state,
            leader_id,
            interval,
            timer,
            last_poll: now,
            timeouts: BTreeMap::new(),
            pending_timeout: None,
            observations: BTreeMap::new(),
            certificate: None,
            proposal: None,
            votes: BTreeMap::new(),
            pending_qc: None,
            prepared: None,
            halted: false,
        };
        driver.check_owner(ledger, store)?;
        driver.recover_local(ledger, store)?;
        Ok(driver)
    }

    fn binding_key(binding: &DriverBinding) -> String {
        format!(
            "native_block_seal/v1/round-driver/{}/{}/{}/{}",
            binding.authority.chain_id,
            binding.authority.epoch,
            binding.height,
            hex_v1(&binding.local_validator_id)
        )
    }

    fn set(&self) -> &NovNativeSealValidatorSetV1 {
        &self.binding.authority.validator_set
    }

    fn request(&self) -> NovNativeSealLocalProposalRequestV1 {
        NovNativeSealLocalProposalRequestV1 {
            chain_id: self.binding.authority.chain_id,
            block_hash: self.binding.block_hash,
            round: self.state.current.round,
            justify_qc_hash: self.binding.justify_qc_hash,
        }
    }

    fn check_owner(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
    ) -> Result<()> {
        if fs::canonicalize(store.path())? != self.store_path
            || fs::canonicalize(ledger.path())? != self.ledger_path
        {
            bail!("round driver cannot switch ledger or signer store handles");
        }
        self.binding.authority.validate_against_ledger(ledger)?;
        let binding = read_json_v1::<DriverBinding>(
            &store.db,
            Self::binding_key(&self.binding).as_bytes(),
            "round driver binding",
        )?
        .context("round driver durable configuration is missing")?;
        if binding != self.binding {
            bail!("round driver durable configuration differs from its owner");
        }
        let state = store
            .load_round_tracking(ledger, self.set(), self.binding.height)?
            .context("round driver durable tracking is missing")?;
        if state != self.state {
            bail!("round driver tracking changed outside its owner; reopen the driver");
        }
        Ok(())
    }

    fn match_subject(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        subject: &NovNativeSealSubjectV1,
    ) -> Result<()> {
        let local = store.prepare_local_subject(
            ledger,
            self.binding.authority.chain_id,
            self.binding.block_hash,
            self.set(),
            subject.round,
            self.binding.justify_qc_hash,
        )?;
        if local != *subject {
            bail!("round driver evidence conflicts with its fixed local candidate");
        }
        ensure_subject_budget(subject)?;
        Ok(())
    }

    fn recover_local(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
    ) -> Result<()> {
        let prepared_key = format!("{}/prepared", Self::binding_key(&self.binding));
        let pinned = read_json_v1::<[u8; 32]>(
            &store.db,
            prepared_key.as_bytes(),
            "round driver prepared QC",
        )?;
        if let Some(qc) = &self.prepared {
            if pinned != Some(qc.qc_hash)
                || store.load_qc(qc.qc_hash)?.as_ref() != Some(qc)
                || store.load_proposal(qc.proposal_hash)?.as_ref() != self.proposal.as_ref()
            {
                bail!("round driver prepared evidence disappeared or changed on disk");
            }
            store.ensure_qc_indexes_contain_v1(qc)?;
            store.ensure_new_view_admission_v1(
                &qc.subject,
                self.proposal
                    .as_ref()
                    .context("round driver prepared proposal is missing")?
                    .proposer_id,
                self.set(),
            )?;
        }
        if let Some(hash) = pinned {
            let qc = store
                .load_qc(hash)?
                .context("round driver prepared QC pin points to missing evidence")?;
            store.ensure_qc_indexes_contain_v1(&qc)?;
            self.match_subject(ledger, store, &qc.subject)?;
            if qc.subject.round != self.state.current.round {
                bail!("round driver prepared QC pin differs from its durable round");
            }
        }
        if let Some(vote) = store.load_local_timeout(
            ledger,
            self.set(),
            self.binding.height,
            self.state.current.round,
            self.binding.local_validator_id,
        )? {
            self.timeouts.insert(vote.validator_id, vote);
        }
        if self.state.current.round > 0 {
            if let Some(admission) = store.load_local_new_view_admission(
                self.set().chain_id,
                self.set().epoch,
                self.binding.height,
                self.state.current.round,
            )? {
                self.match_subject(ledger, store, &admission.subject)?;
                if admission.authority != self.binding.authority {
                    bail!("round driver recovered admission authority changed");
                }
                self.certificate = Some(admission.certificate);
            }
        }
        for qc in
            store.load_qcs_by_height(self.set().chain_id, self.set().epoch, self.binding.height)?
        {
            self.match_subject(ledger, store, &qc.subject)?;
            if qc.subject.round > self.state.current.round {
                bail!("round driver found a future QC");
            }
            if qc.subject.round != self.state.current.round {
                continue;
            }
            let proposal = store
                .load_proposal(qc.proposal_hash)?
                .context("round driver QC proposal is missing")?;
            let message = Message::QuorumCertificate {
                proposal: Box::new(proposal.clone()),
                qc: Box::new(qc.clone()),
                certificate: self.certificate.clone().map(Box::new),
            };
            message.validate_authenticated(
                &self.binding.authority,
                self.binding.height,
                self.binding
                    .authority
                    .transport_peer_id(self.binding.local_validator_id)?,
            )?;
            store.ensure_new_view_admission_v1(&qc.subject, proposal.proposer_id, self.set())?;
            if pinned.is_some_and(|hash| hash != qc.qc_hash) {
                continue;
            }
            if self
                .prepared
                .as_ref()
                .is_none_or(|old| qc.qc_hash < old.qc_hash)
            {
                self.proposal = Some(proposal);
                self.prepared = Some(qc);
            }
        }
        if pinned.is_some() && self.prepared.is_none() {
            bail!("round driver prepared QC pin is absent from verified inventory");
        }
        if self.prepared.is_some() {
            self.pin_prepared(store)?;
        }
        Ok(())
    }

    fn pin_prepared(&self, store: &NovNativeBlockSealStoreV1) -> Result<()> {
        // Validate the complete retransmission bundle before publishing a
        // recoverable completion marker. QC persistence may precede this write.
        self.completed_output()?;
        let qc = self
            .prepared
            .as_ref()
            .context("round driver has no QC to pin")?;
        let key = format!("{}/prepared", Self::binding_key(&self.binding));
        let _guard = store.lock_writes_v1()?;
        if store.load_qc(qc.qc_hash)?.as_ref() != Some(qc)
            || store.load_proposal(qc.proposal_hash)?.as_ref() != self.proposal.as_ref()
        {
            bail!("round driver cannot pin missing prepared evidence");
        }
        store.ensure_qc_indexes_contain_v1(qc)?;
        match read_json_v1::<[u8; 32]>(&store.db, key.as_bytes(), "round driver prepared QC")? {
            Some(old) if old != qc.qc_hash => {
                bail!("round driver cannot replace its prepared QC pin")
            }
            Some(_) => return Ok(()),
            None => (),
        }
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            key.as_bytes(),
            &qc.qc_hash,
            "round driver prepared QC",
        )?;
        write_sync_v1(&store.db, batch)?;
        if read_json_v1::<[u8; 32]>(&store.db, key.as_bytes(), "round driver prepared QC")?
            != Some(qc.qc_hash)
        {
            bail!("round driver prepared QC pin readback mismatch");
        }
        Ok(())
    }

    /// Caller must supply the transport-authenticated source, never a peer ID
    /// claimed by message content. This method emits no local signature or TC.
    pub fn ingest_authenticated(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        source_peer_id: &str,
        message: Message,
    ) -> Result<bool> {
        if self.halted {
            bail!("round driver is halted; inspect the error and reopen");
        }
        self.check_owner(ledger, store)?;
        message.validate_authenticated(
            &self.binding.authority,
            self.binding.height,
            source_peer_id,
        )?;
        if message.round() != self.state.current.round || self.prepared.is_some() {
            return Ok(false);
        }
        if let Some(proposal) = message.proposal() {
            self.match_subject(ledger, store, &proposal.subject)?;
            if self.proposal.as_ref().is_some_and(|old| old != proposal) {
                bail!("round driver received competing proposals");
            }
        }
        if let Message::NewView { observation, .. } = &message {
            if let Some(evidence) = &observation.highest_qc {
                self.match_subject(ledger, store, &evidence.qc.subject)?;
            }
        }
        match &message {
            Message::Timeout(vote) => {
                return insert_unique(&mut self.timeouts, vote.validator_id, vote.as_ref().clone())
            }
            Message::TimeoutCertificate(tc) => {
                if self.pending_timeout.is_some() {
                    return Ok(false);
                }
                self.pending_timeout = Some(tc.as_ref().clone());
            }
            Message::NewView { observation, .. } => {
                return insert_unique(
                    &mut self.observations,
                    observation.validator_id,
                    observation.as_ref().clone(),
                )
            }
            Message::Vote { vote, .. } => {
                // Verify duplicate consistency before staging the embedded proposal.
                insert_unique(&mut self.votes, vote.validator_id, vote.as_ref().clone())?;
            }
            Message::QuorumCertificate { qc, .. } => {
                if self
                    .pending_qc
                    .as_ref()
                    .is_some_and(|old| old == qc.as_ref())
                {
                    return Ok(false);
                }
                self.pending_qc = Some(qc.as_ref().clone());
            }
            Message::Proposal { .. } => (),
        }
        if self.certificate.is_none() {
            self.certificate = message.certificate().cloned();
        }
        if self.proposal.is_none() {
            self.proposal = message.proposal().cloned();
        }
        Ok(true)
    }

    /// Local event loop only. Re-emits durable observations/decisions to allow
    /// recovery after dropped/reordered messages; transport retries are external.
    /// A returned error is fail-closed: the caller must not manufacture fallback votes.
    pub fn poll(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        key: &SigningKey,
        now: Instant,
    ) -> Result<Vec<Message>> {
        if self.halted {
            bail!("round driver is halted; inspect the error and reopen");
        }
        let result = self.poll_inner(ledger, store, key, now);
        if result.is_err() {
            self.halted = true;
        }
        result
    }

    fn poll_inner(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        key: &SigningKey,
        now: Instant,
    ) -> Result<Vec<Message>> {
        self.check_owner(ledger, store)?;
        if validator_id_v1(key.verifying_key().as_bytes()) != self.binding.local_validator_id
            || now < self.last_poll
        {
            bail!("round driver signer or monotonic clock changed");
        }
        self.last_poll = now;
        self.recover_local(ledger, store)?;
        if self.prepared.is_some() {
            return self.completed_output();
        }
        if let Some(qc) = self.pending_qc.clone() {
            self.admit(ledger, store)?;
            let proposal = self
                .proposal
                .as_ref()
                .context("round driver QC lacks proposal")?;
            store.persist_locally_matched_remote_proposal(ledger, proposal, self.set())?;
            store.persist_local_verified_qc(ledger, &qc, self.set())?;
            self.prepared = Some(qc);
            self.pin_prepared(store)?;
            return self.completed_output();
        }
        let mut output = Vec::new();
        if !self.timeouts.contains_key(&self.binding.local_validator_id) {
            if let Some(vote) = self.timer.poll(now, store, ledger, self.set(), key)? {
                self.timeouts.insert(vote.validator_id, vote);
            }
        }
        if let Some(vote) = self.timeouts.get(&self.binding.local_validator_id) {
            output.push(Message::Timeout(Box::new(vote.clone())));
        }
        if self.pending_timeout.is_none() && self.has_quorum(self.timeouts.keys())? {
            self.pending_timeout = Some(NovNativeSealTimeoutCertificateV1 {
                context: self.state.current.clone(),
                votes: self.timeouts.values().cloned().collect(),
            });
        }
        if let Some(tc) = self.pending_timeout.clone() {
            let next = store.advance_round_tracking(ledger, self.set(), &tc)?;
            self.state = next;
            self.leader_id = self
                .binding
                .authority
                .scheduled_leader_v1(self.binding.height, self.state.current.round)?;
            self.timer = NovNativeSealRoundTimerV1::new(&self.state, now, self.interval)?;
            self.timeouts.clear();
            self.pending_timeout = None;
            self.observations.clear();
            self.certificate = None;
            self.proposal = None;
            self.votes.clear();
            self.pending_qc = None;
        }
        if let Some(tc) = &self.state.previous_timeout {
            output.push(Message::TimeoutCertificate(Box::new(tc.clone())));
        }
        // A durable local timeout fences all new proposal/vote signatures in this round.
        if self.timeouts.contains_key(&self.binding.local_validator_id) {
            return self.validate_output(output);
        }
        if self.state.current.round > 0 {
            let observation = store.sign_local_new_view(
                ledger,
                &self.binding.authority,
                self.binding.height,
                self.state.current.round,
                key,
            )?;
            insert_unique(
                &mut self.observations,
                observation.validator_id,
                observation.clone(),
            )?;
            output.push(Message::NewView {
                observation: Box::new(observation),
                previous_timeout: Box::new(
                    self.state
                        .previous_timeout
                        .clone()
                        .context("round driver lacks preceding TC")?,
                ),
            });
            if self.certificate.is_none() && self.has_quorum(self.observations.keys())? {
                self.certificate = Some(NovNativeSealNewViewCertificateV1 {
                    schema: "novovm-native-seal-new-view-certificate/v1".into(),
                    authority_commitment: self.binding.authority.authority_commitment,
                    context: self.state.current.clone(),
                    previous_timeout: self
                        .state
                        .previous_timeout
                        .clone()
                        .context("round driver lacks preceding TC")?,
                    observations: self.observations.values().cloned().collect(),
                });
            }
            if self.certificate.is_none() {
                return self.validate_output(output);
            }
            self.admit(ledger, store)?;
        }
        if self.proposal.is_none() && self.leader_id == self.binding.local_validator_id {
            self.proposal =
                Some(store.sign_local_proposal(ledger, &self.request(), self.set(), key)?);
        }
        let Some(proposal) = self.proposal.clone() else {
            return self.validate_output(output);
        };
        store.persist_locally_matched_remote_proposal(ledger, &proposal, self.set())?;
        if self.leader_id == self.binding.local_validator_id {
            output.push(Message::Proposal {
                proposal: Box::new(proposal.clone()),
                certificate: self.certificate.clone().map(Box::new),
            });
        }
        let vote = store.sign_local_vote(ledger, &proposal, self.set(), key)?;
        insert_unique(&mut self.votes, vote.validator_id, vote.clone())?;
        output.push(Message::Vote {
            proposal: Box::new(proposal.clone()),
            vote: Box::new(vote),
            certificate: self.certificate.clone().map(Box::new),
        });
        if self.has_quorum(self.votes.keys())? {
            let qc = NovNativeSealQuorumCertificateV1::from_votes(
                proposal.subject,
                self.set(),
                self.votes.values().cloned().collect(),
            )?;
            store.persist_local_verified_qc(ledger, &qc, self.set())?;
            self.prepared = Some(qc);
            self.pin_prepared(store)?;
            output.extend(self.completed_output()?);
        }
        self.validate_output(output)
    }

    fn validate_output(&self, output: Vec<Message>) -> Result<Vec<Message>> {
        // Also applies to early returns while collecting evidence.
        for message in &output {
            message.validate_authenticated(
                &self.binding.authority,
                self.binding.height,
                self.binding
                    .authority
                    .transport_peer_id(self.binding.local_validator_id)?,
            )?;
        }
        Ok(output)
    }

    fn admit(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
    ) -> Result<()> {
        if self.state.current.round == 0 {
            return Ok(());
        }
        let certificate = self
            .certificate
            .as_ref()
            .context("round driver requires candidate admission evidence")?;
        // Reserve 64 KiB of the 192 KiB typed-message budget for proposal/QC
        // wrappers: at most 64 validators, two bounded subjects and vote fields.
        // Reject before saving admission or creating local proposal/vote signatures.
        let mut certificate_budget = vec![0u8; 128 * 1024];
        postcard::to_slice(certificate, &mut certificate_budget)
            .context("round driver new-view certificate exceeds its admission budget")?;
        store.admit_local_new_view_candidate(
            ledger,
            &self.binding.authority,
            certificate,
            &self.request(),
        )?;
        // Preserve first-write evidence, even when peers carry equivalent subsets.
        self.certificate = Some(
            store
                .load_local_new_view_admission(
                    self.set().chain_id,
                    self.set().epoch,
                    self.binding.height,
                    self.state.current.round,
                )?
                .context("round driver admission readback disappeared")?
                .certificate,
        );
        Ok(())
    }

    fn has_quorum<'a>(&self, signers: impl Iterator<Item = &'a [u8; 32]>) -> Result<bool> {
        let mut weight = 0u64;
        for signer in signers {
            weight = weight
                .checked_add(
                    self.set()
                        .validator(*signer)
                        .context("round driver contains an unpinned signer")?
                        .weight,
                )
                .context("round driver weight overflow")?;
        }
        Ok(weight >= self.set().quorum_weight)
    }

    fn completed_output(&self) -> Result<Vec<Message>> {
        let qc = self
            .prepared
            .as_ref()
            .context("round driver has no prepared QC")?;
        let message = Message::QuorumCertificate {
            proposal: Box::new(
                self.proposal
                    .clone()
                    .context("round driver prepared proposal is missing")?,
            ),
            qc: Box::new(qc.clone()),
            certificate: self.certificate.clone().map(Box::new),
        };
        message.validate_authenticated(
            &self.binding.authority,
            self.binding.height,
            self.binding
                .authority
                .transport_peer_id(self.binding.local_validator_id)?,
        )?;
        // A peer one round behind must still be able to obtain the preceding
        // TC after this node has stopped its timer on a prepared candidate.
        let mut output = Vec::new();
        if let Some(tc) = &self.state.previous_timeout {
            output.push(Message::TimeoutCertificate(Box::new(tc.clone())));
        }
        output.push(message);
        self.validate_output(output)
    }

    pub fn prepared_qc(&self) -> Option<&NovNativeSealQuorumCertificateV1> {
        if self.halted {
            None
        } else {
            self.prepared.as_ref()
        }
    }

    pub fn status(&self) -> NovNativeSealRoundDriverStatusV1 {
        use NovNativeSealRoundDriverPhaseV1 as Phase;
        let phase = if self.halted {
            Phase::Halted
        } else if self.prepared.is_some() {
            Phase::Prepared
        } else if self.timeouts.contains_key(&self.binding.local_validator_id) {
            Phase::CollectingTimeouts
        } else if self.state.current.round > 0 && self.certificate.is_none() {
            Phase::CollectingNewViews
        } else if self.proposal.is_some() {
            Phase::CollectingVotes
        } else {
            Phase::WaitingProposal
        };
        NovNativeSealRoundDriverStatusV1 {
            height: self.binding.height,
            round: self.state.current.round,
            leader_id: self.leader_id,
            phase,
            prepared: !self.halted && self.prepared.is_some(),
            qc_hash: self.prepared_qc().map(|qc| qc.qc_hash),
            finalized: false,
        }
    }
}

fn ensure_subject_budget(subject: &NovNativeSealSubjectV1) -> Result<()> {
    let mut bounded = [0u8; 4096];
    postcard::to_slice(subject, &mut bounded)
        .context("round driver local subject exceeds its bounded encoding budget")?;
    Ok(())
}

fn insert_unique<T: PartialEq>(
    map: &mut BTreeMap<[u8; 32], T>,
    signer: [u8; 32],
    value: T,
) -> Result<bool> {
    if let Some(old) = map.get(&signer) {
        if old != &value {
            bail!("round driver received contradictory signed evidence from one validator");
        }
        return Ok(false);
    }
    map.insert(signer, value);
    Ok(true)
}
