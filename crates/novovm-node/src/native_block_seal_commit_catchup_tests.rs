fn catchup_snapshot(store: &NovNativeBlockSealStoreV1) -> Vec<(Vec<u8>, Vec<u8>)> {
    store
        .db
        .iterator(IteratorMode::Start)
        .map(|row| {
            let (key, value) = row.unwrap();
            (key.to_vec(), value.to_vec())
        })
        .collect()
}

#[test]
fn native_commit_catchup_old_prepared_round_preserves_its_existing_signature() {
    let mut cluster = DriverCluster::new_with_commit(85_301, true);
    let active = cluster.without_initial_leader();
    let delayed = (0..4).find(|index| !active.contains(index)).unwrap();
    // Only the initial leader sees the round-zero prepare quorum and creates
    // its own commit vote. Its QC/commit packets are withheld from the others.
    let initial = cluster.started;
    let proposals = cluster.poll(&[0, 1, 2, 3], initial);
    cluster.deliver(&[0, 1, 2, 3], &proposals, false);
    let votes = cluster.poll(&[0, 1, 2, 3], initial);
    cluster.deliver(&[delayed], &votes, false);
    cluster.poll(&[delayed], initial);
    let old_output = cluster.poll(&[delayed], initial);
    assert!(old_output
        .iter()
        .any(|(_, m)| matches!(m, RoundMessage::CommitVoteV2 { .. })));
    assert!(cluster.peers[delayed].driver.status().prepared);
    assert!(!cluster.peers[delayed].driver.status().commit_confirmed);
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
    assert!(active
        .iter()
        .all(|i| cluster.peers[*i].driver.status().commit_confirmed));
    let messages = cluster.poll(&[active[0]], now);
    let (_, certificate) = messages
        .into_iter()
        .find(|(_, m)| matches!(m, RoundMessage::CommitCertificateV2 { .. }))
        .unwrap();
    assert_eq!(certificate.round(), 1);
    let source = cluster.source(active[0]);
    let id = cluster.id(delayed);
    let peer = &mut cluster.peers[delayed];
    assert_eq!(peer.driver.status().round, 0);
    assert!(peer.driver.status().prepared);
    let before = catchup_snapshot(peer.node.store());
    // An aggregate with insufficient/tampered confirmation evidence is rejected
    // before any cache/durable-state change, even though its prepare QC is valid.
    let mut bad = certificate.clone();
    if let RoundMessage::CommitCertificateV2 { commit, .. } = &mut bad {
        commit.votes.truncate(2);
    }
    assert!(peer
        .driver
        .ingest_authenticated(peer.node.ledger(), peer.node.store(), &source, bad)
        .is_err());
    assert!(peer
        .driver
        .ingest_authenticated(
            peer.node.ledger(),
            peer.node.store(),
            "unknown-source",
            certificate.clone()
        )
        .is_err());
    assert!(!peer.driver.status().commit_confirmed);
    assert_eq!(catchup_snapshot(peer.node.store()), before);
    assert!(peer
        .driver
        .ingest_authenticated(
            peer.node.ledger(),
            peer.node.store(),
            &source,
            certificate.clone()
        )
        .unwrap());
    assert_eq!(
        catchup_snapshot(peer.node.store()),
        before,
        "ingress must not persist or sign"
    );
    assert!(!peer
        .driver
        .ingest_authenticated(
            peer.node.ledger(),
            peer.node.store(),
            &source,
            certificate.clone()
        )
        .unwrap());
    let output = peer
        .driver
        .poll(peer.node.ledger(), peer.node.store(), &peer.key, now)
        .unwrap();
    assert_eq!(output, vec![certificate.clone()]);
    let status = peer.driver.status();
    assert_eq!(status.round, 0);
    assert_eq!(status.commit_round, Some(1));
    assert!(status.commit_confirmed && status.prepared && !status.finalized);
    assert!(status.commit_observed);
    let after = catchup_snapshot(peer.node.store());
    assert_eq!(after.len(), before.len() + 2);
    let untouched = after
        .into_iter()
        .filter(|(k, _)| !String::from_utf8_lossy(k).contains("/commit-v2/observed"))
        .collect::<Vec<_>>();
    assert_eq!(
        untouched, before,
        "catchup may not import active QC/admission or alter signer locks"
    );
    cluster.restart(delayed, now);
    let peer = &mut cluster.peers[delayed];
    assert_eq!(peer.driver.status(), status);
    assert_eq!(
        peer.driver
            .poll(
                peer.node.ledger(),
                peer.node.store(),
                &peer.key,
                now + DRIVER_INTERVAL
            )
            .unwrap(),
        vec![certificate]
    );
    let key = format!(
        "native_block_seal/v1/round-driver/{}/{}/1/{}/commit-v2/observed/hash",
        cluster.authority.chain_id,
        cluster.authority.epoch,
        hex_v1(&id)
    );
    peer.node.store().db.delete(key.as_bytes()).unwrap();
    assert!(peer
        .driver
        .poll(
            peer.node.ledger(),
            peer.node.store(),
            &peer.key,
            now + DRIVER_INTERVAL
        )
        .is_err());
    assert!(!peer.driver.status().commit_confirmed);
    assert!(peer.driver.status().commit_round.is_none());
    peer.node.reopen_store();
    assert!(NovNativeSealRoundDriverV1::open_with_commit_v2(
        peer.node.ledger(),
        peer.node.store(),
        cluster.authority.clone(),
        cluster.block_hash,
        None,
        id,
        now,
        DRIVER_INTERVAL,
        true
    )
    .is_err());
    cluster.assert_unfinalized();
}
