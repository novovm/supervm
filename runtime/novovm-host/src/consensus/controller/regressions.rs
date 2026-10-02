//! Private scheduling regressions plus a real-candidate ACK-window probe used
//! by the explicit AOEM integration test. No production controller test hooks.
use super::*;

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
