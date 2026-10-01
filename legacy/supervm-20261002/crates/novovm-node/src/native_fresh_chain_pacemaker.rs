use super::*;
use crate::native_block_seal::{
    newview::{NovNativeSealNewViewCertificateV1, NovNativeSealNewViewObservationV1},
    round_message::NovNativeSealRoundMessageV1 as Message,
    round_overlay::{submit_frames, validate_round_inbound},
    round_wire::{encode_nov_native_seal_round_wire_v1, round_wire_object_hash_v1},
    timeout::{
        NovNativeSealRoundStateV1, NovNativeSealRoundTimerV1, NovNativeSealTimeoutCertificateV1,
        NovNativeSealTimeoutVoteV1,
    },
};
use crate::tx_ingress::candidate_workspace as workspace;

pub(super) struct ParentPacemaker {
    store: NovNativeBlockSealStoreV1,
    state: NovNativeSealRoundStateV1,
    timer: NovNativeSealRoundTimerV1,
    timeouts: BTreeMap<[u8; 32], NovNativeSealTimeoutVoteV1>,
    observations: BTreeMap<(u64, [u8; 32]), NovNativeSealNewViewObservationV1>,
    pending_timeout: Option<NovNativeSealTimeoutCertificateV1>,
    certificate: Option<NovNativeSealNewViewCertificateV1>,
    attempted: BTreeMap<([u8; 32], String), Instant>,
    next_send: usize,
    last_poll: Instant,
    local_timed_out: bool,
    ingress_budgets: BTreeMap<String, (Instant, usize)>,
}

#[cfg(test)]
pub(crate) fn exercise_parent_pacemaker(
    configs: &[NovNativeSealServiceConfigV1],
    params: &serde_json::Value,
    publication_store: &Path,
    mut toggle_corrupt_parent: impl FnMut(),
) {
    use crate::product_mainline_overlay::ProductMainlineOverlayEventV1 as Event;
    crate::native_block_seal::tests::native_seal_round_network::with_service_test_transports(
        configs[0].chain_id,
        |peers| {
            let started = Instant::now();
            let initial = configs[0].authority.scheduled_leader_v1(2, 0).unwrap();
            let active = configs
                .iter()
                .filter(|config| config.local_validator_id != initial)
                .collect::<Vec<_>>();
            let runtimes = active
                .iter()
                .map(|config| {
                    peers
                        .iter()
                        .find(|(runtime, _)| {
                            runtime.startup().local_peer_id
                                == config
                                    .authority
                                    .transport_peer_id(config.local_validator_id)
                                    .unwrap()
                        })
                        .unwrap()
                        .0
                })
                .collect::<Vec<_>>();
            let mut drivers = active
                .iter()
                .map(|config| ParentPacemaker::open(config, params, started).unwrap())
                .collect::<Vec<_>>();
            for (index, driver) in drivers.iter_mut().enumerate() {
                driver
                    .poll(
                        active[index],
                        params,
                        runtimes[index],
                        started + Duration::from_millis(1999),
                    )
                    .unwrap();
                assert_eq!(driver.status_json()["timeout_count"], 0);
            }
            let due = started + Duration::from_secs(2);
            let mut captured = None;
            let mut step = |drivers: &mut [ParentPacemaker], count: usize| {
                for index in 0..count {
                    for event in runtimes[index].drain_events(128) {
                        if let Event::Inbound(event) = event {
                            if drivers[index]
                                .ingest(active[index], &event, due)
                                .unwrap_or(false)
                                && captured.is_none()
                            {
                                captured = Some((index, event));
                            }
                        }
                    }
                    drivers[index]
                        .poll(active[index], params, runtimes[index], due)
                        .unwrap();
                }
                std::thread::sleep(Duration::from_millis(20));
            };
            for _ in 0..8 {
                step(&mut drivers, 2);
            }
            for driver in &drivers[..2] {
                assert_eq!(driver.state.current.round, 0);
                assert_eq!(driver.proposal_round(), None);
            }
            let deadline = Instant::now();
            while drivers
                .iter()
                .any(|driver| driver.proposal_round() != Some(1))
            {
                assert!(
                    deadline.elapsed() < Duration::from_secs(45),
                    "candidate-less pacemaker convergence deadline"
                );
                step(&mut drivers, 3);
            }
            for driver in &drivers {
                assert_eq!(driver.certificate.as_ref().unwrap().observations.len(), 3);
                assert_eq!(
                    driver.state.previous_timeout.as_ref().unwrap().votes.len(),
                    3
                );
            }
            let (index, event) = captured.unwrap();
            let mut lifecycle_config = active[index].clone();
            lifecycle_config.seal_store_path = publication_store.into();
            let ledger_path = crate::tx_ingress::native_persistence_write_paths_v1(params)
                .into_iter()
                .find(|(label, _)| *label == "native block ledger")
                .unwrap()
                .1;
            let mut lifecycle = FreshChainLifecycleV1::open(
                lifecycle_config,
                &ledger_path,
                params,
                runtimes[index],
                due,
            )
            .unwrap();
            let mut unverified_body = event.clone();
            unverified_body.frame.payload = b"body-queue-capacity-fixture".to_vec();
            for _ in 0..PER_PEER_QUEUE {
                assert!(lifecycle.enqueue(event.clone()));
            }
            assert!(!lifecycle.enqueue(event.clone()));
            for _ in 0..PER_PEER_QUEUE {
                assert!(lifecycle.enqueue(unverified_body.clone()));
            }
            assert!(!lifecycle.enqueue(unverified_body));
            assert_eq!(lifecycle.status_json()["proposed_successors"], 0);
            assert_eq!(lifecycle.status_json()["received_successors"], 0);
            drop(lifecycle);
            let mut tampered = event.clone();
            tampered.delivery_id = [0; 32];
            assert!(drivers[index]
                .ingest(active[index], &tampered, due)
                .is_err());
            tampered = event;
            tampered.source_peer_id = "unauthenticated-peer".into();
            assert!(drivers[index]
                .ingest(active[index], &tampered, due)
                .is_err());
            let before = drivers[index].state.clone();
            let restarted = due + Duration::from_secs(60);
            drivers[index] = ParentPacemaker::open(active[index], params, restarted).unwrap();
            assert_eq!(drivers[index].state, before);
            drivers[index]
                .poll(active[index], params, runtimes[index], restarted)
                .unwrap();
            assert_eq!(drivers[index].state.current.round, 1);
            assert!(!drivers[index].local_timed_out);
            assert_eq!(drivers[index].proposal_round(), None);
            assert!(drivers[index]
                .poll(active[index], params, runtimes[index], due)
                .is_err());
            toggle_corrupt_parent();
            assert!(drivers[index]
                .poll(active[index], params, runtimes[index], restarted)
                .is_err());
            toggle_corrupt_parent();
        },
    );
}

fn with_parent<T>(
    config: &NovNativeSealServiceConfigV1,
    params: &serde_json::Value,
    action: impl FnOnce(&NovNativeBlockLedgerV1) -> Result<T>,
) -> Result<T> {
    workspace::with_verified_finalized_parent_round_v1(
        config.chain_id,
        config
            .isolated_workspace_id
            .context("pacemaker parent missing")?,
        config
            .fresh_genesis_config_commitment
            .context("pacemaker genesis missing")?,
        params,
        |view| {
            config.authority.validate_against_ledger(view)?;
            if view.fresh_round_height_v1()? != config.height.checked_add(1) {
                bail!("pacemaker height differs from live finalized parent");
            }
            action(view)
        },
    )
}

impl ParentPacemaker {
    pub(super) fn open(
        config: &NovNativeSealServiceConfigV1,
        params: &serde_json::Value,
        now: Instant,
    ) -> Result<Self> {
        with_parent(config, params, |view| {
            let store = NovNativeBlockSealStoreV1::open(&config.seal_store_path)?;
            let height = config
                .height
                .checked_add(1)
                .context("pacemaker height overflow")?;
            if NovNativeSealRoundDriverV1::startup_candidate(
                &store,
                &config.authority,
                height,
                config.local_validator_id,
            )?
            .is_some()
            {
                bail!("candidate owner must recover through the candidate service");
            }
            let state =
                store.start_round_tracking(view, &config.authority.validator_set, height)?;
            let timer = NovNativeSealRoundTimerV1::new(&state, now, config.round_timeout)?;
            Ok(Self {
                store,
                state,
                timer,
                timeouts: BTreeMap::new(),
                observations: BTreeMap::new(),
                pending_timeout: None,
                certificate: None,
                attempted: BTreeMap::new(),
                next_send: 0,
                last_poll: now,
                local_timed_out: false,
                ingress_budgets: BTreeMap::new(),
            })
        })
    }

    pub(super) fn ingest(
        &mut self,
        config: &NovNativeSealServiceConfigV1,
        event: &ProductMainlineOverlayInboundV1,
        now: Instant,
    ) -> Result<bool> {
        if now < self.last_poll
            || !config.authority.transport_bindings.iter().any(|binding| {
                binding.transport_peer_id == event.source_peer_id
                    && binding.validator_id != config.local_validator_id
            })
        {
            bail!("pacemaker ingress clock or source mismatch");
        }
        let message = validate_round_inbound(
            &config.authority,
            self.state.current.height,
            config
                .authority
                .transport_peer_id(config.local_validator_id)?,
            event,
        )?;
        if !matches!(
            message,
            Message::Timeout(_) | Message::TimeoutCertificate(_) | Message::NewView { .. }
        ) {
            return Ok(false);
        }
        let budget = self
            .ingress_budgets
            .entry(event.source_peer_id.clone())
            .or_insert((now, 0));
        if now.duration_since(budget.0) >= Duration::from_secs(1) {
            *budget = (now, 0);
        }
        if budget.1 >= config.ingress_per_source_per_second {
            bail!("pacemaker source rate exceeded");
        }
        budget.1 += 1;
        let round = self.state.current.round;
        match message {
            Message::Timeout(vote) if vote.context == self.state.current => {
                self.timeouts.entry(vote.validator_id).or_insert(*vote);
            }
            Message::TimeoutCertificate(certificate)
                if certificate.context == self.state.current =>
            {
                self.pending_timeout.get_or_insert(*certificate);
            }
            Message::NewView {
                observation,
                previous_timeout,
            } if observation.context.round == round
                || previous_timeout.context == self.state.current =>
            {
                if previous_timeout.context == self.state.current {
                    self.pending_timeout.get_or_insert(*previous_timeout);
                }
                self.observations
                    .entry((observation.context.round, observation.validator_id))
                    .or_insert(*observation);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn advance(
        &mut self,
        view: &NovNativeBlockLedgerV1,
        config: &NovNativeSealServiceConfigV1,
        certificate: &NovNativeSealTimeoutCertificateV1,
        now: Instant,
    ) -> Result<()> {
        self.state = self.store.advance_round_tracking(
            view,
            &config.authority.validator_set,
            certificate,
        )?;
        self.timer = NovNativeSealRoundTimerV1::new(&self.state, now, config.round_timeout)?;
        self.timeouts.clear();
        self.observations
            .retain(|(round, _), _| *round == self.state.current.round);
        self.pending_timeout = None;
        self.certificate = None;
        self.local_timed_out = false;
        Ok(())
    }

    fn check_owner(
        &self,
        view: &NovNativeBlockLedgerV1,
        config: &NovNativeSealServiceConfigV1,
    ) -> Result<()> {
        if self
            .store
            .load_round_tracking(
                view,
                &config.authority.validator_set,
                self.state.current.height,
            )?
            .as_ref()
            != Some(&self.state)
        {
            bail!("pacemaker durable state changed or disappeared");
        }
        Ok(())
    }

    pub(super) fn poll(
        &mut self,
        config: &NovNativeSealServiceConfigV1,
        params: &serde_json::Value,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<()> {
        check_runtime(config, runtime)?;
        if now < self.last_poll {
            bail!("pacemaker clock moved backwards");
        }
        self.last_poll = now;
        let messages = with_parent(config, params, |view| {
            self.check_owner(view, config)?;
            let set = &config.authority.validator_set;
            let recovered = self.store.load_local_timeout(
                view,
                set,
                self.state.current.height,
                self.state.current.round,
                config.local_validator_id,
            )?;
            let timeout = match recovered {
                Some(vote) => Some(vote),
                None => self
                    .timer
                    .poll(now, &self.store, view, set, &config.signer)?,
            };
            if let Some(vote) = timeout {
                self.timeouts.insert(vote.validator_id, vote);
            }
            let mut output = Vec::new();
            if let Some(vote) = self.timeouts.get(&config.local_validator_id) {
                output.push(Message::Timeout(Box::new(vote.clone())));
            }
            let weight: u64 = self
                .timeouts
                .keys()
                .map(|signer| set.validator(*signer).unwrap().weight)
                .sum();
            if self.pending_timeout.is_none() && weight >= set.quorum_weight {
                self.pending_timeout = Some(NovNativeSealTimeoutCertificateV1 {
                    context: self.state.current.clone(),
                    votes: self.timeouts.values().cloned().collect(),
                });
            }
            if let Some(certificate) = self.pending_timeout.clone() {
                self.advance(view, config, &certificate, now)?;
            }
            self.local_timed_out = self.timeouts.contains_key(&config.local_validator_id);
            if let Some(previous) = &self.state.previous_timeout {
                output.push(Message::TimeoutCertificate(Box::new(previous.clone())));
                if !self.local_timed_out {
                    let observation = self.store.sign_local_new_view(
                        view,
                        &config.authority,
                        self.state.current.height,
                        self.state.current.round,
                        &config.signer,
                    )?;
                    self.observations.insert(
                        (observation.context.round, observation.validator_id),
                        observation.clone(),
                    );
                    output.push(Message::NewView {
                        observation: Box::new(observation),
                        previous_timeout: Box::new(previous.clone()),
                    });
                    let observations = self
                        .observations
                        .values()
                        .filter(|observation| observation.context == self.state.current)
                        .cloned()
                        .collect::<Vec<_>>();
                    let weight: u64 = observations
                        .iter()
                        .map(|observation| set.validator(observation.validator_id).unwrap().weight)
                        .sum();
                    if weight >= set.quorum_weight {
                        let certificate = NovNativeSealNewViewCertificateV1 {
                            schema: "novovm-native-seal-new-view-certificate/v1".into(),
                            authority_commitment: config.authority.authority_commitment,
                            context: self.state.current.clone(),
                            previous_timeout: previous.clone(),
                            observations,
                        };
                        if certificate
                            .verify(&self.state.current, &config.authority)?
                            .is_none()
                        {
                            self.certificate = Some(certificate);
                        } else {
                            self.certificate = None;
                        }
                    }
                }
            }
            Ok(output)
        })?;
        let peer = config
            .authority
            .transport_peer_id(config.local_validator_id)?;
        let frames = messages
            .iter()
            .map(|message| {
                let wire = encode_nov_native_seal_round_wire_v1(
                    message,
                    &config.authority,
                    self.state.current.height,
                    peer,
                )?;
                Ok((round_wire_object_hash_v1(&wire), wire))
            })
            .collect::<Result<Vec<_>>>()?;
        self.attempted
            .retain(|(hash, _), _| frames.iter().any(|(current, _)| current == hash));
        let peers = runtime.remote_peer_ids().iter().cloned().collect();
        submit_frames(
            &frames,
            &peers,
            &mut self.attempted,
            &mut self.next_send,
            now,
            |peer, hash, wire| {
                runtime.try_submit_to_peer(
                    peer,
                    ProductMainlineOverlayPayloadClassV1::NativeSeal,
                    hash,
                    wire,
                )
            },
        )?;
        Ok(())
    }

    pub(super) fn proposal_round(&self) -> Option<u64> {
        (!self.local_timed_out && (self.state.current.round == 0 || self.certificate.is_some()))
            .then_some(self.state.current.round)
    }

    pub(super) fn certificate(&self) -> Option<&NovNativeSealNewViewCertificateV1> {
        self.certificate.as_ref()
    }

    pub(super) fn admit_body_round(
        &mut self,
        config: &NovNativeSealServiceConfigV1,
        params: &serde_json::Value,
        message: &Message,
        now: Instant,
    ) -> Result<bool> {
        with_parent(config, params, |view| {
            self.check_owner(view, config)?;
            if message.round() == self.state.current.round.saturating_add(1) {
                if let Some(certificate) = message.certificate() {
                    self.advance(view, config, &certificate.previous_timeout, now)?;
                }
            }
            Ok(message.round() == self.state.current.round
                && self
                    .store
                    .load_local_timeout(
                        view,
                        &config.authority.validator_set,
                        self.state.current.height,
                        self.state.current.round,
                        config.local_validator_id,
                    )?
                    .is_none())
        })
    }

    pub(super) fn status_json(&self) -> serde_json::Value {
        serde_json::json!({"height":self.state.current.height,"round":self.state.current.round,
            "local_timed_out":self.local_timed_out,"new_view_ready":self.certificate.is_some(),
            "timeout_count":self.timeouts.len(),"observation_count":self.observations.len()})
    }
}
