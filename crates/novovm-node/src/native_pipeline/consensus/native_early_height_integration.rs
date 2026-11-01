//! Exact early-height custody regression. Honest proposals, votes and decisions
//! come from the original durable drivers. Only message delivery/persistence
//! polling order is controlled; no fabricated QC, ACK or alternate state owner.
use super::*;

#[test]
#[ignore = "real AOEM; early next-height proposal survives original parent publication"]
fn native_progress_early_height_proposal_survives_parent_ack() -> Result<()> {
    early_height(false, false)
}

#[test]
#[ignore = "real AOEM; early next-height decision survives original parent publication"]
fn native_progress_early_height_decision_survives_parent_ack() -> Result<()> {
    early_height(true, false)
}

#[test]
#[ignore = "real AOEM; authenticated wrong-parent early proposal grants no body demand"]
fn native_progress_early_height_wrong_parent_is_not_promoted() -> Result<()> {
    early_height(false, true)
}

fn early_height(decision: bool, wrong_parent: bool) -> Result<()> {
    let Fixture {
        keys,
        ids,
        set,
        mut mesh,
        mut services,
        anchor,
        mut drivers,
        ..
    } = fixture(ExecutionBackend::CpuCallbacks)?;
    let laggard = (0..4)
        .find(|&i| ids[i] != set.leader(1, 0).unwrap() && ids[i] != set.leader(2, 0).unwrap())
        .context("no non-leader laggard")?;
    let healthy: Vec<_> = (0..4).filter(|&i| i != laggard).collect();
    let mut parent_decision = None;
    let mut child_proposal = None;
    let mut child_decision = None;
    let mut child_body = None;

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
        let body_id = transport::body_id(&execution, &raw, codec())?;
        if height == 2 {
            child_body = Some(body_id);
        }
        // Each owner independently executes its own exact-parent input. The
        // laggard has only height-one work; no speculative child is registered.
        for (i, driver) in drivers.iter_mut().enumerate() {
            if height == 2 && i == laggard {
                continue;
            }
            let Submission::Accepted(mut ticket) =
                services.0[i].try_submit(BatchRequest::new(raw.clone(), execution, policy())?)?
            else {
                bail!("early-height fixture submission full");
            };
            let deadline = Instant::now() + DEADLINE;
            let candidate = loop {
                ensure!(Instant::now() < deadline, "early-height execution timeout");
                if let Some(done) = ticket.try_take()? {
                    break done.candidate().clone();
                }
                std::thread::yield_now();
            };
            ensure!(
                driver.register_candidate(body_id, &candidate, true)?,
                "early-height candidate budget"
            );
        }
        let deadline = Instant::now() + DEADLINE;
        while healthy
            .iter()
            .any(|&i| drivers[i].context().height == height)
        {
            ensure!(
                Instant::now() < deadline,
                "healthy early-height progression timeout"
            );
            for &i in &healthy {
                drivers[i].poll(&services.0[i], Instant::now())?;
                if let Some(message) = drivers[i].take_message() {
                    match &message {
                        Message::Proposal { .. } if height == 2 => {
                            child_proposal.get_or_insert((i, message.clone()));
                        }
                        Message::Decision { certificate, .. } => {
                            ensure!(
                                certificate.verify(&set)?.value().is_some(),
                                "driver emitted invalid decision"
                            );
                            if height == 1 {
                                parent_decision.get_or_insert((i, message.clone()));
                            } else {
                                child_decision.get_or_insert((i, message.clone()));
                            }
                        }
                        _ => {}
                    }
                    // This portion intentionally isolates original driver
                    // ordering. The early child below uses real native UDP.
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
    let (source, mut message) = if decision {
        child_decision.context("no actual child decision")?
    } else {
        child_proposal.context("no actual child proposal")?
    };
    if wrong_parent {
        let Message::Proposal { proposal, .. } = &mut message else {
            bail!("wrong-parent counterexample requires proposal");
        };
        let mut context = proposal.context;
        context.parent_block_hash[0] ^= 1;
        // Only this explicit adversarial counterexample uses fixture signing.
        // All positive controls above were emitted by real durable journals.
        *proposal = novovm_consensus::round_bft::test_vectors::sign_proposal(
            context,
            proposal.round,
            proposal.value,
            None,
            &set,
            &keys[source],
        )?;
    }
    let delivered = mesh.broadcast(source, &message)?;
    drivers[laggard].receive(ids[source], &delivered[laggard])?;
    ensure!(
        drivers[laggard].context().height == 1
            && drivers[laggard].parent() == anchor
            && drivers[laggard].head().is_none()
            && drivers[laggard].pending_message().is_none()
            && drivers[laggard].missing_bodies().next().is_none(),
        "early evidence changed parent, signed, published or demanded child before ACK"
    );
    let (source, parent_message) = parent_decision.context("no real parent decision")?;
    drivers[laggard].receive(ids[source], &parent_message)?;
    let deadline = Instant::now() + DEADLINE;
    while drivers[laggard].context().height == 1 {
        ensure!(
            Instant::now() < deadline,
            "laggard parent publication timeout"
        );
        drivers[laggard].poll(&services.0[laggard], Instant::now())?;
        // Consume its real parent outbox only; no signed child is permitted.
        if let Some(message) = drivers[laggard].take_message() {
            let height = match message {
                Message::Proposal { proposal, .. } | Message::Decision { proposal, .. } => {
                    proposal.context.height
                }
                Message::Vote(vote) => vote.context.height,
                _ => bail!("unexpected driver outbox"),
            };
            ensure!(height == 1, "early child acquired signing authority");
        }
        std::thread::yield_now();
    }
    ensure!(
        drivers[laggard].context().height == 2
            && drivers[laggard].head().is_some_and(|head| head.height == 1),
        "child published without independently executing its body"
    );
    let demands: Vec<_> = drivers[laggard].missing_bodies().collect();
    if wrong_parent {
        ensure!(
            demands.is_empty(),
            "wrong-parent early evidence was promoted"
        );
    } else {
        ensure!(
            demands.iter().any(|(_, body)| Some(*body) == child_body),
            "early next-height evidence lost after original parent ACK; no retransmission supplied"
        );
    }
    drop(drivers);
    services.shutdown()?;
    Ok(())
}
