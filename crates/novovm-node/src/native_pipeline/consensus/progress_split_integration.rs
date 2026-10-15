use super::*;
use std::collections::BTreeSet;

#[test]
#[ignore = "real AOEM; three autonomous honest drivers and one equivocating native peer, not process acceptance"]
fn native_progress_aab_split_recovers_without_external_qc() -> Result<()> {
    let Fixture {
        paths,
        keys,
        ids,
        set,
        genesis,
        peer_ids,
        mut mesh,
        mut services,
        anchor,
        mut drivers,
    } = fixture(ExecutionBackend::CpuCallbacks)?;
    let fault = ids
        .iter()
        .position(|id| *id == set.leader(1, 0).unwrap())
        .unwrap();
    let honest: Vec<_> = (0..4).filter(|i| *i != fault).collect();
    let context = batch_context(anchor.state_root);
    let raw_a = super::super::chain_integration::raw_at(1)?;
    let mut raw_b = raw_a.clone();
    raw_b.reverse();
    let mut candidates = vec![Vec::new(); 4];
    let mut body_ids = Vec::new();
    for raw in [&raw_a, &raw_b] {
        let batch = Arc::new(ApflTransferBatch::from_raw(
            raw,
            ApflLimits {
                transactions: 8,
                transaction_bytes: 1024,
                body_bytes: 8192,
            },
        )?);
        let bodies = mesh.broadcast(fault, &Message::ApflBody { context, batch })?;
        body_ids.push(transport::body_id(&context, raw, codec())?);
        for &i in &honest {
            let Message::ApflBody { context, batch } = &bodies[i] else {
                bail!("wrong body");
            };
            let Submission::Accepted(mut ticket) = services.0[i]
                .try_submit(BatchRequest::from_apfl(batch.clone(), *context, policy())?)?
            else {
                bail!("fixture full")
            };
            let deadline = Instant::now() + DEADLINE;
            let done = loop {
                ensure!(Instant::now() < deadline, "candidate timeout");
                if let Some(done) = ticket.try_take()? {
                    break done;
                }
                std::thread::yield_now();
            };
            candidates[i].push(done.candidate().clone());
        }
    }
    let values: Vec<_> = candidates[honest[0]]
        .iter()
        .map(|c| {
            BlockStatement::from_executed(c.packet(), genesis, &set, &anchor).map(|s| s.hash())
        })
        .collect::<Result<_>>()?;
    ensure!(values[0] != values[1], "split fixture identical values");
    let far_round = (33..100)
        .find(|&round| set.leader(1, round).unwrap() == ids[fault])
        .unwrap();
    let far = Message::Proposal {
        proposal: novovm_consensus::round_bft::test_vectors::sign_proposal(
            genesis,
            far_round,
            values[0],
            None,
            &set,
            &keys[fault],
        )?,
        valid_quorum: None,
        body_id: body_ids[0],
    };
    let rejected = drivers[honest[0]].receive(ids[fault], &far).unwrap_err();
    ensure!(
        rejected.to_string().contains("future proposal exceeds") && !drivers[honest[0]].is_frozen(),
        "future admission not bounded/peer error froze owner"
    );
    for &i in &honest {
        let other = *honest.iter().find(|&&j| j != i).unwrap();
        ensure!(
            drivers[i]
                .register_candidate(body_ids[0], &candidates[other][0], false)
                .is_err()
                && !drivers[i].is_frozen(),
            "foreign execution owner accepted/froze node"
        );
        for (j, candidate) in candidates[i].iter().enumerate() {
            ensure!(
                drivers[i].register_candidate(body_ids[j], candidate, false)?,
                "candidate budget"
            );
        }
    }
    // Only the Byzantine key is used by the fixture. Honest votes, QCs,
    // proposals in later rounds and decisions can only come from the driver.
    for (n, &i) in honest.iter().enumerate() {
        let value = if n == 2 { 1 } else { 0 };
        let proposal = novovm_consensus::round_bft::test_vectors::sign_proposal(
            genesis,
            0,
            values[value],
            None,
            &set,
            &keys[fault],
        )?;
        let message = Message::Proposal {
            proposal,
            valid_quorum: None,
            body_id: body_ids[value],
        };
        // An unsigned body hint cannot be assigned by some other session.
        ensure!(
            drivers[i].receive(ids[i], &message).is_err() && !drivers[i].is_frozen(),
            "non-proposer session gained body-hint authority"
        );
        let prepared = mesh.channels[fault].prepare(&message)?;
        let peer = (0..3)
            .find(|&p| mesh.channels[fault].peer_id(p).unwrap() == peer_ids[i])
            .unwrap();
        ensure!(
            mesh.channels[fault].try_send(peer, &prepared, Instant::now())?,
            "fault send budget"
        );
    }
    let mut outbound: Vec<VecDeque<(PreparedMessage, u8)>> =
        (0..4).map(|_| VecDeque::new()).collect();
    let mut first_prevotes = BTreeMap::new();
    let mut nil_precommits = BTreeSet::new();
    let mut decided = BTreeSet::new();
    let laggard = *honest
        .iter()
        .find(|&&i| ids[i] != set.leader(1, 1).unwrap())
        .unwrap();
    let mut future_proposal_seen = false;
    let deadline = Instant::now() + DEADLINE;
    while honest.iter().any(|&i| drivers[i].context().height == 1) {
        ensure!(
            Instant::now() < deadline,
            "A/A/B autonomous progress timeout: {:?}, future={future_proposal_seen}, nil={nil_precommits:?}",
            honest.iter().map(|&i| (i, drivers[i].round(), drivers[i].step(), drivers[i].head())).collect::<Vec<_>>()
        );
        mesh.pump()?;
        // Drain the fault peer too, but it never runs a journal or signs votes.
        let _ = mesh.channels[fault].poll_receive(Instant::now(), 64)?;
        for &i in &honest {
            if let Some(event) = mesh.channels[i].poll_receive(Instant::now(), 64)? {
                match event {
                    ReceiveEvent::Message(received) => {
                        let peer = peer_ids.iter().position(|p| p == &received.peer).unwrap();
                        if i == laggard
                            && drivers[i].round() == 0
                            && matches!(&received.message, Message::Proposal {proposal,..} if proposal.round==1)
                        {
                            future_proposal_seen = true;
                        }
                        drivers[i].receive(ids[peer], &received.message)?;
                    }
                    ReceiveEvent::Rejected { error, .. } => {
                        bail!("split message rejection: {error}")
                    }
                }
            }
            // After round change the application's existing candidate policy
            // can offer A again. No lock reset or state-machine method is used.
            if drivers[i].context().height == 1 && drivers[i].round() > 0 {
                ensure!(
                    drivers[i].register_candidate(body_ids[0], &candidates[i][0], true)?,
                    "successor round candidate budget"
                );
            }
            // Delay only one honest local persistence poll after its nil
            // precommit, until the next proposal actually arrives on UDP.
            // No forged clock, ACK, vote or round transition is supplied.
            if !(i == laggard
                && drivers[i].round() == 0
                && nil_precommits.contains(&ids[i])
                && !future_proposal_seen)
            {
                drivers[i].poll(&services.0[i], Instant::now())?;
            }
            if outbound[i].len() < 8 {
                if let Some(message) = drivers[i].pending_message().cloned() {
                    match &message {
                        Message::Vote(v) if v.round == 0 && v.phase == Phase::Prevote => {
                            ensure!(v.validator_id == ids[i], "external honest signer");
                            first_prevotes.insert(ids[i], v.value);
                        }
                        Message::Vote(v) if v.round == 0 && v.phase == Phase::Precommit => {
                            ensure!(v.value.is_none(), "split votes formed non-nil precommit");
                            nil_precommits.insert(v.validator_id);
                        }
                        Message::Decision {
                            proposal,
                            certificate,
                            ..
                        } => {
                            ensure!(
                                proposal.round == 1 && proposal.value == values[0],
                                "wrong recovered decision"
                            );
                            let qc = certificate.verify(&set)?;
                            ensure!(
                                qc.phase() == Phase::Precommit
                                    && qc.value() == Some(values[0])
                                    && certificate.votes.len() == 3
                                    && certificate
                                        .votes
                                        .iter()
                                        .all(|v| v.validator_id != ids[fault]),
                                "recovery QC lacks three honest signatures"
                            );
                            decided.insert(ids[i]);
                        }
                        _ => {}
                    }
                    outbound[i].push_back((mesh.channels[i].prepare(&message)?, 0));
                    drivers[i].take_message();
                }
            }
            if let Some((prepared, sent)) = outbound[i].front_mut() {
                for peer in 0..3 {
                    if *sent & (1 << peer) == 0
                        && mesh.channels[i].try_send(peer, prepared, Instant::now())?
                    {
                        *sent |= 1 << peer;
                    }
                }
                if *sent == 7 {
                    outbound[i].pop_front();
                }
            }
        }
        std::thread::yield_now();
    }
    ensure!(
        first_prevotes.get(&ids[honest[0]]) == Some(&Some(values[0]))
            && first_prevotes.get(&ids[honest[1]]) == Some(&Some(values[0]))
            && first_prevotes.get(&ids[honest[2]]) == Some(&Some(values[1]))
            && nil_precommits.len() == 3
            && future_proposal_seen
            && decided.len() == 3,
        "A/A/B phase evidence missing"
    );
    let head = drivers[honest[0]].head().unwrap();
    ensure!(
        honest
            .iter()
            .all(|&i| drivers[i].head() == Some(head) && drivers[i].parent() == head),
        "split recovery conflicting heads"
    );
    drop(drivers);
    services.shutdown()?;
    for &i in &honest {
        let p = CandidatePipeline::start(pipeline_config(&paths[i])?, OpenMode::Existing)?;
        let journal = open_journal(
            ValidatorJournal::open(&p, genesis, anchor, set.clone(), keys[i].clone())?,
            &p,
        )?;
        ensure!(
            journal.head() == Some(head) && journal.context().height == 2,
            "split cold recovery"
        );
        p.shutdown()?;
    }
    Ok(())
}
