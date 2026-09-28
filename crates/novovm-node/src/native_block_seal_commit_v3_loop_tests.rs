// Real loopback WSS/Overlay; synthetic AOEM-owned candidate facts.
#[test]
fn native_decision_v3_lifecycle_real_wss_quorum_restart_and_loss() {
    use crate::native_block_seal::commit_v3::lifecycle::NovNativeSealDecisionLoopV3 as DecisionLoop;
    use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
    let mut cluster = NetworkCluster::new(9_782_300);
    for i in 0..4 {
        cluster.start_peer(i, cluster.started);
    }
    let prepared_at = cluster.prepare_over_network(&[0, 1, 2, 3], cluster.started);
    cluster.assert_prepared(&[0, 1, 2, 3], 0);
    let mut loops = Vec::new();
    let mut local_votes = Vec::new();
    for peer in &cluster.peers {
        let qc = peer
            .adapter
            .as_ref()
            .unwrap()
            .prepared_qc()
            .unwrap()
            .clone();
        let proposal = peer
            .node
            .store()
            .load_proposal(qc.proposal_hash)
            .unwrap()
            .unwrap();
        let vote = peer
            .node
            .store()
            .sign_local_decision_vote_v3(
                peer.node.ledger(),
                &qc,
                &cluster.authority.validator_set,
                &peer.key,
            )
            .unwrap();
        let id = vote.validator_id;
        let message = Message::DecisionVoteV3 {
            proposal: Box::new(proposal),
            qc: Box::new(qc),
            vote: Box::new(vote),
            certificate: None,
        };
        loops.push(
            DecisionLoop::attach(
                peer.node.ledger(),
                peer.node.store(),
                cluster.authority.clone(),
                id,
                message.clone(),
                peer.runtime.as_ref().unwrap(),
                prepared_at,
            )
            .unwrap(),
        );
        local_votes.push(message);
    }
    let began = Instant::now();
    let mut certificate_received = false;
    let mut inbox_bound_checked = false;
    loop {
        let now = prepared_at + began.elapsed();
        // Only two senders for the initial interval: no accidental 2/4 quorum.
        let count = if began.elapsed() < Duration::from_millis(500) {
            2
        } else {
            4
        };
        for (i, service) in loops.iter_mut().enumerate().take(count) {
            let peer = &cluster.peers[i];
            let runtime = peer.runtime.as_ref().unwrap();
            for event in runtime.drain_events(128) {
                match event {
                    ProductMainlineOverlayEventV1::Inbound(inbound) => {
                        if inbound
                            .frame
                            .payload
                            .get(10)
                            .is_some_and(|kind| *kind == 9 || *kind == 10)
                        {
                            certificate_received |= inbound.frame.payload[10] == 10;
                            let before = durable_seal_facts(peer.node.store());
                            if !inbox_bound_checked {
                                let mut invalid = inbound.clone();
                                invalid.payload_sha256[0] ^= 1;
                                for _ in 0..4 {
                                    assert!(service.enqueue(invalid.clone()));
                                }
                                assert!(!service.enqueue(invalid));
                                inbox_bound_checked = true;
                            } else {
                                service.enqueue(inbound);
                            }
                            assert_eq!(
                                durable_seal_facts(peer.node.store()),
                                before,
                                "ingress wrote durable state"
                            );
                        }
                    }
                    ProductMainlineOverlayEventV1::WorkerFailed(error) => {
                        panic!("V3 WSS worker: {error}")
                    }
                    _ => (),
                }
            }
            service
                .poll(peer.node.ledger(), peer.node.store(), runtime, now)
                .unwrap();
            assert!(!service.halted());
        }
        if count == 2 {
            assert!(loops.iter().all(|service| !service.confirmed()));
        }
        if loops.iter().all(|service| service.confirmed()) && certificate_received {
            break;
        }
        assert!(
            began.elapsed() < NETWORK_DEADLINE,
            "V3 WSS confirmation deadline"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(inbox_bound_checked);
    for peer in &cluster.peers {
        let cert = peer
            .node
            .store()
            .load_decision_certificate_by_height_v3(
                cluster.authority.chain_id,
                cluster.authority.epoch,
                1,
            )
            .unwrap()
            .unwrap();
        cert.verify(&cluster.authority.validator_set).unwrap();
    }
    cluster.assert_unfinalized();
    // Binding/clock faults are local faults, not remote packet rejection.
    let peer = &cluster.peers[1];
    assert!(loops[1]
        .poll(
            peer.node.ledger(),
            peer.node.store(),
            cluster.peers[0].runtime.as_ref().unwrap(),
            prepared_at + began.elapsed()
        )
        .is_err());
    assert!(loops[1].halted() && !loops[1].confirmed());
    let peer = &cluster.peers[2];
    assert!(loops[2]
        .poll(
            peer.node.ledger(),
            peer.node.store(),
            peer.runtime.as_ref().unwrap(),
            prepared_at
        )
        .is_err());
    assert!(loops[2].halted() && !loops[2].confirmed());
    cluster.peers[0].runtime.take().unwrap().shutdown();
    cluster.peers[0].adapter.take();
    cluster.peers[0].node.reopen_store();
    let now = prepared_at + began.elapsed();
    cluster.start_peer(0, now);
    let peer = &cluster.peers[0];
    let runtime = peer.runtime.as_ref().unwrap();
    let before = durable_seal_facts(peer.node.store());
    loops[0] = DecisionLoop::attach(
        peer.node.ledger(),
        peer.node.store(),
        cluster.authority.clone(),
        validator_id_v1(peer.key.verifying_key().as_bytes()),
        local_votes[0].clone(),
        runtime,
        now,
    )
    .unwrap();
    assert!(loops[0].confirmed());
    loops[0]
        .poll(peer.node.ledger(), peer.node.store(), runtime, now)
        .unwrap();
    assert_eq!(
        durable_seal_facts(peer.node.store()),
        before,
        "recovery signed or mutated archive"
    );
    let slot = crate::native_block_seal::commit::certificate_height_key(
        cluster.authority.chain_id,
        cluster.authority.epoch,
        1,
    );
    peer.node
        .store()
        .db
        .delete(format!("{slot}/decision-v3-certificate-hash"))
        .unwrap();
    assert!(loops[0]
        .poll(
            peer.node.ledger(),
            peer.node.store(),
            runtime,
            now + Duration::from_secs(1)
        )
        .is_err());
    assert!(loops[0].halted() && !loops[0].confirmed());
    assert!(DecisionLoop::attach(
        peer.node.ledger(),
        peer.node.store(),
        cluster.authority.clone(),
        validator_id_v1(peer.key.verifying_key().as_bytes()),
        local_votes[0].clone(),
        runtime,
        now
    )
    .is_err());
    cluster.assert_unfinalized();
}
