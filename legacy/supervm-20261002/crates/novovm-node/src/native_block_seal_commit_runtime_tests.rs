// Uses the independent key/store DriverCluster; ingress is exercised through
// the bounded authenticated wire codec, not an unverified typed shortcut.
fn commit_delivery(cluster: &mut DriverCluster, active: &[usize], now: Instant) {
    use crate::native_block_seal::round_wire::{
        decode_nov_native_seal_round_wire_v1 as decode,
        encode_nov_native_seal_round_wire_v1 as encode,
    };
    let messages = cluster.poll(active, now);
    let mut decoded = Vec::new();
    for (sender, message) in messages {
        let source = cluster.source(sender);
        let wire = encode(&message, &cluster.authority, 1, &source).unwrap();
        let message = decode(&wire, &cluster.authority, 1, &source).unwrap();
        if matches!(message, RoundMessage::CommitVoteV2 { .. }) {
            assert!(decode(
                &wire,
                &cluster.authority,
                1,
                &cluster.source((sender + 1) % 4)
            )
            .is_err());
        }
        decoded.push((sender, message));
    }
    cluster.deliver(active, &decoded, true);
}

#[test]
fn native_commit_runtime_quorum_restart_and_certificate_loss() {
    let mut cluster = DriverCluster::new_with_commit(85_201, true);
    let now = cluster.started;
    for _ in 0..12 {
        commit_delivery(&mut cluster, &[0, 1, 2, 3], now);
        if cluster
            .peers
            .iter()
            .all(|p| p.driver.status().commit_confirmed)
        {
            break;
        }
    }
    for index in 0..4 {
        assert!(cluster.peers[index].driver.status().commit_confirmed);
        let before = cluster.peers[index].driver.status().commit_certificate_hash;
        cluster.restart(index, now);
        assert_eq!(
            cluster.peers[index].driver.status().commit_certificate_hash,
            before
        );
        assert!(cluster.peers[index].driver.status().commit_confirmed);
    }
    cluster.assert_unfinalized();
    let peer = &mut cluster.peers[0];
    peer.node
        .store()
        .db
        .delete(crate::native_block_seal::commit::certificate_height_key(
            85_201, 1, 1,
        ))
        .unwrap();
    assert!(peer
        .driver
        .poll(peer.node.ledger(), peer.node.store(), &peer.key, now)
        .is_err());
    assert!(!peer.driver.status().commit_confirmed);
    assert!(peer.driver.status().commit_certificate_hash.is_none());
    assert!(NovNativeSealRoundDriverV1::open_with_commit_v2(
        peer.node.ledger(),
        peer.node.store(),
        cluster.authority.clone(),
        cluster.block_hash,
        None,
        validator_id_v1(peer.key.verifying_key().as_bytes()),
        now,
        DRIVER_INTERVAL,
        true
    )
    .is_err());
}

#[test]
fn native_commit_runtime_two_votes_cannot_confirm_and_mode_is_pinned() {
    let mut cluster = DriverCluster::new_with_commit(85_202, true);
    let now = cluster.started;
    for _ in 0..12 {
        let messages = cluster
            .poll(&[0, 1, 2, 3], now)
            .into_iter()
            .filter(|(_, message)| !message.is_commit_v2())
            .collect::<Vec<_>>();
        cluster.deliver(&[0, 1, 2, 3], &messages, false);
        if cluster.peers.iter().all(|p| p.driver.status().prepared) {
            break;
        }
    }
    assert!(cluster.peers.iter().all(|p| p.driver.status().prepared));
    for _ in 0..4 {
        commit_delivery(&mut cluster, &[0, 1], now);
    }
    assert!(cluster
        .peers
        .iter()
        .all(|p| !p.driver.status().commit_confirmed));
    let peer = &cluster.peers[0];
    assert!(NovNativeSealRoundDriverV1::open(
        peer.node.ledger(),
        peer.node.store(),
        cluster.authority.clone(),
        cluster.block_hash,
        None,
        cluster.id(0),
        now,
        DRIVER_INTERVAL
    )
    .is_err());
    cluster.restart(0, now);
    for _ in 0..8 {
        commit_delivery(&mut cluster, &[0, 1, 2], now);
        if cluster.peers[..3]
            .iter()
            .all(|p| p.driver.status().commit_confirmed)
        {
            break;
        }
    }
    assert!(cluster.peers[..3]
        .iter()
        .all(|p| p.driver.status().commit_confirmed));
    cluster.assert_unfinalized();
}

#[test]
fn native_commit_runtime_late_qc_after_timeout_is_observe_only() {
    let mut cluster = DriverCluster::new_with_commit(85_203, true);
    let now = cluster.started;
    let follower = cluster.without_initial_leader()[0];
    let peer = &cluster.peers[follower];
    peer.node
        .store()
        .sign_local_timeout(
            peer.node.ledger(),
            &cluster.authority.validator_set,
            1,
            0,
            &peer.key,
        )
        .unwrap();
    for _ in 0..14 {
        commit_delivery(&mut cluster, &[0, 1, 2, 3], now);
        if cluster
            .peers
            .iter()
            .all(|p| p.driver.status().commit_confirmed)
        {
            break;
        }
    }
    assert!(cluster
        .peers
        .iter()
        .all(|p| p.driver.status().commit_confirmed));
    let peer = &cluster.peers[follower];
    let qc = peer.driver.prepared_qc().unwrap();
    assert!(peer
        .node
        .store()
        .db
        .get(crate::native_block_seal::commit::lock_key(
            &qc.subject,
            cluster.id(follower)
        ))
        .unwrap()
        .is_none());
    cluster.restart(follower, now);
    assert!(cluster.peers[follower].driver.status().commit_confirmed);
    cluster.assert_unfinalized();
}

#[test]
fn native_commit_runtime_nonzero_round_after_prepare_failover() {
    let mut cluster = DriverCluster::new_with_commit(85_204, true);
    let active = cluster.without_initial_leader();
    let now = cluster.started + DRIVER_INTERVAL;
    for _ in 0..18 {
        commit_delivery(&mut cluster, &active, now);
        if active
            .iter()
            .all(|i| cluster.peers[*i].driver.status().commit_confirmed)
        {
            break;
        }
    }
    for i in active {
        assert_eq!(cluster.peers[i].driver.status().round, 1);
        assert!(cluster.peers[i].driver.status().commit_confirmed);
        cluster.restart(i, now);
        assert!(cluster.peers[i].driver.status().commit_confirmed);
    }
    cluster.assert_unfinalized();
}
