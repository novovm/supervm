//! Private scheduling regressions plus a real-candidate ACK-window probe used
//! by the explicit AOEM integration test. No production controller test hooks.
use super::*;

#[test]
fn body_fanout_does_not_resend_admitted_peers_but_keeps_backpressured_peer() -> Result<()> {
    let now = Instant::now();
    let interval = Duration::from_millis(100);
    let mut fanout = BodyFanout::new(3);
    let mut schedule = RetrySchedule::new();
    // Offline first peer must not delay the two healthy peers.
    schedule.attempted(false, false, 3, now, interval)?;
    for peer in [1, 2] {
        assert_eq!(schedule.next_peer, peer);
        assert!(fanout.needs(peer));
        fanout.accepted(peer);
        schedule.attempted(false, true, 3, now, interval)?;
    }
    assert_eq!(fanout.remaining, 1);
    assert!(fanout.needs(0));
    assert!(!fanout.needs(1) && !fanout.needs(2));
    assert_eq!(schedule.next_peer, 0);
    fanout.accepted(0);
    assert_eq!(fanout.remaining, 0);
    fanout.accepted(0);
    assert_eq!(
        fanout.remaining, 0,
        "duplicate admission underflowed fanout"
    );
    // No receipt/finality state exists here: a lost initial send is recovered
    // through a separate exact RequestBody, whose direct schedule can re-arm.
    let mut requested = RetrySchedule::new();
    requested.attempted(true, true, 3, now, interval)?;
    assert!(!requested.active);
    requested.wake();
    assert!(requested.active);
    assert!(!fanout.needs(0), "direct request restarted blind broadcast");
    Ok(())
}

#[test]
fn execution_observations_accumulate_scalars_and_maximum_without_claiming_workers() {
    let mut stats = ControllerStats::default();
    assert!(stats.last_execution_observation.is_none());
    let first = ExecutionObservation {
        components: 6,
        credit_only_accounts: 2,
        recomputed_transactions: 3,
        peak_callbacks: 4,
    };
    let last = ExecutionObservation {
        components: 3,
        credit_only_accounts: 1,
        recomputed_transactions: 2,
        peak_callbacks: 1,
    };
    stats.observe_execution(first);
    stats.observe_execution(last);
    assert_eq!(stats.execution_components_total, 9);
    assert_eq!(stats.execution_credit_only_accounts_total, 3);
    assert_eq!(stats.execution_recomputed_transactions_total, 5);
    assert_eq!(stats.execution_peak_callbacks, 4);
    assert_eq!(stats.last_execution_observation, Some(last));
    assert!(!stats.execution_observation_saturated);
    assert_eq!(
        stats.executed_batches, 0,
        "observation aggregation invented a completed batch"
    );
    assert_eq!(
        stats.durable_decisions, 0,
        "observation aggregation invented finality"
    );
}

#[test]
fn execution_observation_overflow_is_explicit_and_does_not_wrap() {
    let mut stats = ControllerStats {
        execution_components_total: u64::MAX - 1,
        execution_credit_only_accounts_total: u64::MAX,
        execution_recomputed_transactions_total: u64::MAX - 2,
        ..ControllerStats::default()
    };
    let observation = ExecutionObservation {
        components: 2,
        credit_only_accounts: 1,
        recomputed_transactions: 3,
        peak_callbacks: 1,
    };
    stats.observe_execution(observation);
    assert_eq!(stats.execution_components_total, u64::MAX);
    assert_eq!(stats.execution_credit_only_accounts_total, u64::MAX);
    assert_eq!(stats.execution_recomputed_transactions_total, u64::MAX);
    assert!(stats.execution_observation_saturated);
    assert_eq!(stats.last_execution_observation, Some(observation));
    stats.observe_execution(ExecutionObservation {
        components: 0,
        credit_only_accounts: 0,
        recomputed_transactions: 0,
        peak_callbacks: 0,
    });
    assert!(
        stats.execution_observation_saturated,
        "later observation hid saturation"
    );
    assert_eq!(stats.execution_peak_callbacks, 1);
}

#[test]
fn direct_history_is_demand_driven_not_a_permanent_background_retransmission() -> Result<()> {
    let now = Instant::now();
    let interval = Duration::from_millis(100);
    let mut schedule = RetrySchedule::new();
    assert!(schedule.active);
    schedule.attempted(true, true, 3, now, interval)?;
    assert!(
        !schedule.active,
        "accepted history response kept flooding indefinitely"
    );
    // Advancing time does not re-arm a direct response. A new exact request does.
    assert!(now + interval * 10 >= schedule.due.unwrap());
    assert!(!schedule.active);
    schedule.wake();
    assert!(schedule.active && schedule.due.is_none());
    schedule.attempted(true, false, 3, now, interval)?;
    assert!(
        schedule.active,
        "backpressure was misreported as accepted response"
    );
    assert_eq!(schedule.next_peer, 0);
    Ok(())
}

#[test]
fn offline_first_peer_does_not_block_healthy_broadcast_recipients() -> Result<()> {
    let now = Instant::now();
    let interval = Duration::from_millis(100);
    let mut schedule = RetrySchedule::new();
    schedule.attempted(false, false, 3, now, interval)?;
    assert_eq!(
        schedule.next_peer, 1,
        "offline first peer caused head-of-line blocking"
    );
    assert!(schedule.active && schedule.due.is_none());
    schedule.attempted(false, true, 3, now, interval)?;
    assert_eq!(schedule.next_peer, 2);
    schedule.attempted(false, true, 3, now, interval)?;
    assert_eq!(
        schedule.next_peer, 0,
        "failed first peer lost its retry opportunity"
    );
    assert!(schedule.active);
    assert_eq!(schedule.due, Some(now + interval));
    Ok(())
}

impl Controller {
    /// Keep one genuine owner-verified remote control ready for the ordinary
    /// warm receive path. This changes ingress timing only, not verification,
    /// execution, retirement admission, or the controller's event budget.
    pub(in crate::consensus) fn regression_warm_control(
        &mut self,
        peer: &str,
        ready: &Ready,
    ) -> Result<bool> {
        ensure!(
            !self.is_recovering(),
            "warm ingress fixture used during recovery"
        );
        let Message::Vote(vote) = ready.message.as_ref() else {
            anyhow::bail!("warm ingress fixture requires a vote");
        };
        let VerifiedEvidence::Vote(verified) = ready.evidence.as_ref() else {
            anyhow::bail!("warm ingress fixture requires owner verification");
        };
        ensure!(
            verified.vote() == vote
                && vote.context == self.context()
                && vote.round == self.round()
                && vote.phase == Phase::Prevote
                && vote.value.is_none()
                && ready.body.is_none()
                && self
                    .config
                    .peers
                    .get(&vote.validator_id)
                    .map(String::as_str)
                    == Some(peer),
            "warm ingress fixture requires the exact remote nil prevote and route"
        );
        if self.recovery_test_ingress.is_some() {
            // Owner backpressure must preserve the previous slot, not replace
            // or synchronously destroy its original Ready on the control path.
            return Ok(false);
        }
        self.recovery_test_ingress = Some(crate::consensus::channel::Received {
            peer: peer.to_owned(),
            ready: Ready {
                message: ready.message.clone(),
                prepared: ready.prepared.clone(),
                evidence: ready.evidence.clone(),
                body: None,
            },
        });
        Ok(true)
    }

    /// Called only from the real AOEM test after an ACTUAL candidate has staged
    /// a journal write, but before that write is polled/ACKed. The second body
    /// is genuinely prepared by HostChannel, not a fabricated private packet.
    pub(in crate::consensus) fn regression_pending_candidate_is_not_evicted(
        &mut self,
    ) -> Result<()> {
        ensure!(
            self.journal.is_pending(),
            "ACK-window test requires real pending journal"
        );
        let value = self
            .pending_value
            .context("ACK-window test needs non-nil executed value")?;
        let (id, candidate) = self
            .candidate(value)
            .context("real pending candidate missing")?;
        let original = self.bodies.get(&id).context("pending body missing")?;
        let source = original.source.clone();
        let context = *candidate.packet().context();
        let original_packet = candidate.packet().candidate_id();
        let token = self.allocate_token()?;
        let mut request = PrepareRequest {
            token,
            input: PrepareInput::New(Arc::new(Message::Body {
                context,
                raw_transactions: vec![vec![0xf1, 0x02, 0x03]],
            })),
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            ensure!(
                Instant::now() < deadline,
                "owner admission timed out in ACK-window regression"
            );
            match self.channel.try_prepare(request)? {
                PrepareAdmission::Accepted => break,
                PrepareAdmission::Backpressure(returned) => request = returned,
                PrepareAdmission::Rejected { reason, .. } => anyhow::bail!(reason),
            }
            self.flush_retired()?;
            std::thread::yield_now();
        }
        let ready = loop {
            ensure!(
                Instant::now() < deadline,
                "owner preparation timed out in ACK-window regression"
            );
            match self.channel.try_recv()? {
                Some(ChannelEvent::Prepared {
                    token: found,
                    result,
                }) if found == token => break result.map_err(anyhow::Error::msg)?,
                Some(ChannelEvent::Prepared { token, result }) => self.prepared(token, result)?,
                Some(ChannelEvent::Received(received)) => {
                    self.receive(received.peer, received.ready)?
                }
                None => std::thread::yield_now(),
            }
            self.flush_retired()?;
        };
        let replacement_id = ready
            .prepared
            .body_id()
            .context("owner failed to prepare replacement body")?;
        ensure!(
            replacement_id != id,
            "regression replacement was not distinct"
        );
        self.keep_body(source, ready, None)?;
        ensure!(
            self.bodies.contains_key(&id) && self.bodies.contains_key(&replacement_id),
            "same-peer replacement evicted candidate during pending durable ACK"
        );
        ensure!(
            self.candidate(value)
                .context("pending executed value lost")?
                .1
                .packet()
                .candidate_id()
                == original_packet,
            "pending journal statement changed to a new body"
        );
        ensure!(
            self.journal.is_pending() && self.head().is_none(),
            "test accidentally acknowledged or published pending journal"
        );
        Ok(())
    }

    pub(in crate::consensus) fn regression_peer_requests_are_not_head_authority(
        &mut self,
    ) -> Result<()> {
        let peer = self
            .config
            .peers
            .values()
            .next()
            .context("regression needs remote peer")?
            .clone();
        let initial = self.context();
        let mut future = initial;
        future.height += 1;
        future.parent_block_hash = [0x81; 32];
        future.parent_decision_hash = [0x82; 32];
        self.request_archive(peer.clone(), future)?;
        ensure!(
            self.peer_requests.get(&peer) == Some(&future),
            "new requested height was not recorded"
        );
        self.request_archive(peer.clone(), initial)?;
        ensure!(
            self.peer_requests.get(&peer) == Some(&initial),
            "cold peer lower-height request was incorrectly monotonic-pinned"
        );
        ensure!(
            self.context() == initial && self.head().is_none() && self.archives.is_empty(),
            "untrusted replay request was adopted as canonical context or head"
        );
        Ok(())
    }
}
