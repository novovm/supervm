//! Hold ONLY the existing AdvanceHeight ACK. All positive votes below come
//! from the original durable journals, never a fabricated QC or timer change.
use super::*;

#[test]
#[ignore = "real AOEM; next-height votes survive the original AdvanceHeight ACK window"]
fn native_progress_early_height_votes_survive_advance_height_ack() -> Result<()> {
    let Fixture {
        ids,
        set,
        mut services,
        mut drivers,
        ..
    } = fixture(ExecutionBackend::CpuCallbacks)?;
    let laggard = (0..4)
        .find(|&i| ids[i] != set.leader(1, 0).unwrap() && ids[i] != set.leader(2, 0).unwrap())
        .context("missing non-leader laggard")?;
    let healthy: Vec<_> = (0..4).filter(|&i| i != laggard).collect();
    let mut parent_decision = None;
    let mut child_proposal = None;
    let mut child_prevotes = BTreeMap::new();
    let mut child_input = None;
    for height in 1..=2 {
        let parent = drivers[healthy[0]].parent();
        let mut execution = batch_context(parent.state_root);
        execution.height = height;
        execution.parent_height = parent.height;
        execution.parent_block_hash = parent.block_hash;
        execution.parent_state_root = parent.state_root;
        execution.parent_receipt_root = parent.receipt_batch_commitment;
        execution.parent_state_version = parent.state_version;
        execution.slot = height - 1;
        execution.timestamp_unix_ms += height;
        let raw = super::super::chain_integration::raw_at(height)?;
        let body = transport::body_id(&execution, &raw, codec())?;
        if height == 2 {
            child_input = Some((execution, raw.clone(), body));
        }
        for (i, driver) in drivers.iter_mut().enumerate() {
            if height == 2 && i == laggard {
                continue;
            }
            let Submission::Accepted(mut ticket) =
                services.0[i].try_submit(BatchRequest::new(raw.clone(), execution, policy())?)?
            else {
                bail!("early vote candidate backpressure");
            };
            let deadline = Instant::now() + DEADLINE;
            let candidate = loop {
                ensure!(Instant::now() < deadline, "early vote candidate timeout");
                if let Some(done) = ticket.try_take()? {
                    break done.candidate().clone();
                }
                std::thread::yield_now();
            };
            ensure!(driver.register_candidate(body, &candidate, true)?);
        }
        let deadline = Instant::now() + DEADLINE;
        while healthy
            .iter()
            .any(|&i| drivers[i].context().height == height)
        {
            ensure!(
                Instant::now() < deadline,
                "healthy early vote progression timeout"
            );
            for &i in &healthy {
                drivers[i].poll(&services.0[i], Instant::now())?;
                if let Some(message) = drivers[i].take_message() {
                    match &message {
                        Message::Decision { .. } if height == 1 => {
                            parent_decision.get_or_insert((i, message.clone()));
                        }
                        Message::Proposal { .. } if height == 2 => {
                            child_proposal.get_or_insert((i, message.clone()));
                        }
                        Message::Vote(vote) if height == 2 && vote.phase == Phase::Prevote => {
                            child_prevotes.insert(i, message.clone());
                        }
                        _ => {}
                    }
                    for &receiver in &healthy {
                        if receiver != i {
                            drivers[receiver].receive(ids[i], &message)?;
                        }
                    }
                }
            }
            std::thread::yield_now();
        }
    }
    ensure!(
        child_prevotes.len() == 3,
        "need three actual durable child prevotes"
    );
    let (source, parent) = parent_decision.context("no actual parent decision")?;
    drivers[laggard].receive(ids[source], &parent)?;
    let deadline = Instant::now() + DEADLINE;
    while drivers[laggard].head().is_none() {
        ensure!(Instant::now() < deadline, "parent publication timeout");
        drivers[laggard].poll(&services.0[laggard], Instant::now())?;
        if let Some(message) = drivers[laggard].take_message() {
            ensure!(
                matches!(message, Message::Decision { .. }),
                "unexpected parent outbox"
            );
        }
    }
    ensure!(drivers[laggard].context().height == 1);
    // Decision publication and AdvanceHeight are distinct durable transitions.
    // Stage the latter, but deliberately do not poll its ACK until all early
    // child messages have passed through the SAME authenticated receiver once.
    drivers[laggard].poll_after_receive(Instant::now())?;
    let before_parent = drivers[laggard].parent();
    let before_head = drivers[laggard].head();
    let (source, proposal) = child_proposal.context("no actual child proposal")?;
    drivers[laggard].receive(ids[source], &proposal)?;
    for vote in child_prevotes.values() {
        // Relay identity is not vote ownership. Re-delivery by another valid
        // peer must neither multiply weight nor consume another phase slot.
        drivers[laggard].receive(ids[healthy[0]], vote)?;
        drivers[laggard].receive(ids[healthy[1]], vote)?;
    }
    ensure!(
        drivers[laggard].context().height == 1
            && drivers[laggard].parent() == before_parent
            && drivers[laggard].head() == before_head
            && drivers[laggard].pending_message().is_none()
            && drivers[laggard].missing_bodies().next().is_none(),
        "future votes changed authority before AdvanceHeight ACK"
    );
    while drivers[laggard].context().height == 1 {
        ensure!(Instant::now() < deadline, "AdvanceHeight ACK timeout");
        drivers[laggard].poll_acknowledgements(&services.0[laggard])?;
        std::thread::yield_now();
    }
    ensure!(drivers[laggard].head() == before_head);
    ensure!(drivers[laggard].pending_message().is_none());
    let (execution, raw, body) = child_input.context("missing child input")?;
    let Submission::Accepted(mut ticket) =
        services.0[laggard].try_submit(BatchRequest::new(raw, execution, policy())?)?
    else {
        bail!("laggard child candidate backpressure");
    };
    let candidate = loop {
        ensure!(Instant::now() < deadline, "laggard child execution timeout");
        if let Some(done) = ticket.try_take()? {
            break done.candidate().clone();
        }
        std::thread::yield_now();
    };
    ensure!(drivers[laggard].register_candidate(body, &candidate, false)?);
    // No vote is retransmitted after the ACK. Original QC processing must
    // recover the three already delivered prevotes and emit a durable
    // precommit, without waiting for or changing the five-second timers.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        ensure!(
            Instant::now() < deadline,
            "early next-height votes were lost across AdvanceHeight ACK"
        );
        drivers[laggard].poll(&services.0[laggard], Instant::now())?;
        if let Some(Message::Vote(vote)) = drivers[laggard].take_message() {
            ensure!(vote.context.height == 2 && vote.round == 0 && vote.value.is_some());
            if vote.phase == Phase::Precommit {
                break;
            }
        }
        std::thread::yield_now();
    }
    ensure!(
        drivers[laggard].head() == before_head,
        "prevotes published a child without precommit QC"
    );
    drop(drivers);
    services.shutdown()?;
    Ok(())
}
