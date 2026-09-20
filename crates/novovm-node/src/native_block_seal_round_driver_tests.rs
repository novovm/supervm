// Controlled local message delivery with synthetic executed-candidate fixtures.
// Every validator owns a different durable store and only its own signing key.
// This does not claim independent AOEM execution, real networking or finality.
use super::*;
use crate::native_block_seal::round_driver::NovNativeSealRoundDriverV1;
use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as RoundMessage;
use crate::native_block_seal_overlay::{
    NovNativeSealEpochAuthorityV1, NOV_NATIVE_SEAL_OVERLAY_MAX_ROUND_V1,
};
use std::time::{Duration, Instant};

const DRIVER_INTERVAL: Duration = Duration::from_secs(10);

struct DriverPeer {
    node: TestNodeV1,
    key: SigningKey,
    driver: NovNativeSealRoundDriverV1,
}

struct DriverCluster {
    peers: Vec<DriverPeer>,
    authority: NovNativeSealEpochAuthorityV1,
    block_hash: [u8; 32],
    started: Instant,
}

impl DriverCluster {
    fn new(chain_id: u64) -> Self {
        let started = Instant::now();
        let mut peers = Vec::new();
        let mut expected_authority = None;
        let mut block_hash = [0; 32];
        for index in 0..4 {
            let (node, block, keys, set) =
                genesis_fixture_v1(&format!("round-driver-{index}"), chain_id);
            let authority = native_block_seal_newview::authority_v1(&node, &set);
            if let Some(expected) = &expected_authority {
                assert_eq!(expected, &authority);
                assert_eq!(block_hash, block.header.block_hash);
            } else {
                expected_authority = Some(authority.clone());
                block_hash = block.header.block_hash;
            }
            let key = keys.into_iter().nth(index).unwrap();
            let driver = NovNativeSealRoundDriverV1::open(
                node.ledger(),
                node.store(),
                authority,
                block_hash,
                None,
                validator_id_v1(key.verifying_key().as_bytes()),
                started,
                DRIVER_INTERVAL,
            )
            .expect("open independent single-validator driver");
            peers.push(DriverPeer { node, key, driver });
        }
        assert!(peers.iter().enumerate().all(|(i, peer)| peers[..i]
            .iter()
            .all(|other| other.node.root != peer.node.root)));
        Self {
            peers,
            authority: expected_authority.unwrap(),
            block_hash,
            started,
        }
    }

    fn id(&self, index: usize) -> [u8; 32] {
        validator_id_v1(self.peers[index].key.verifying_key().as_bytes())
    }

    fn source(&self, index: usize) -> String {
        self.authority
            .transport_peer_id(self.id(index))
            .unwrap()
            .to_owned()
    }

    fn without_initial_leader(&self) -> Vec<usize> {
        let leader = self.peers[0].driver.status().leader_id;
        (0..4).filter(|index| self.id(*index) != leader).collect()
    }

    fn poll(&mut self, active: &[usize], now: Instant) -> Vec<(usize, RoundMessage)> {
        let mut messages = Vec::new();
        for &index in active {
            let peer = &mut self.peers[index];
            for message in peer
                .driver
                .poll(peer.node.ledger(), peer.node.store(), &peer.key, now)
                .expect("single-validator poll")
            {
                messages.push((index, message));
            }
        }
        messages
    }

    fn deliver(&mut self, active: &[usize], messages: &[(usize, RoundMessage)], reverse: bool) {
        let mut order = (0..messages.len()).collect::<Vec<_>>();
        if reverse {
            order.reverse();
        }
        for message_index in order {
            let (sender, message) = &messages[message_index];
            let source = self.source(*sender);
            for &recipient in active {
                if recipient == *sender {
                    continue;
                }
                let peer = &mut self.peers[recipient];
                peer.driver
                    .ingest_authenticated(
                        peer.node.ledger(),
                        peer.node.store(),
                        &source,
                        message.clone(),
                    )
                    .expect("valid authenticated peer message");
                if reverse {
                    peer.driver
                        .ingest_authenticated(
                            peer.node.ledger(),
                            peer.node.store(),
                            &source,
                            message.clone(),
                        )
                        .expect("duplicate peer message is harmless");
                }
            }
        }
    }

    fn pump(&mut self, active: &[usize], now: Instant, reverse: bool) {
        for _ in 0..16 {
            let messages = self.poll(active, now);
            self.deliver(active, &messages, reverse);
            if active
                .iter()
                .all(|index| self.peers[*index].driver.status().prepared)
            {
                return;
            }
        }
        panic!("controlled active quorum did not produce a prepare QC within 16 deliveries");
    }

    fn restart(&mut self, index: usize, now: Instant) {
        let local_id = self.id(index);
        let peer = &mut self.peers[index];
        peer.node.reopen_store();
        peer.driver = NovNativeSealRoundDriverV1::open(
            peer.node.ledger(),
            peer.node.store(),
            self.authority.clone(),
            self.block_hash,
            None,
            local_id,
            now,
            DRIVER_INTERVAL,
        )
        .expect("recover driver from its own durable state");
    }

    fn assert_unfinalized(&self) {
        assert_eq!(NOV_NATIVE_SEAL_OVERLAY_MAX_ROUND_V1, 0);
        for peer in &self.peers {
            assert!(!peer.driver.status().finalized);
            let head = peer
                .node
                .ledger()
                .load_head(self.authority.chain_id)
                .unwrap()
                .unwrap();
            assert!(!head.proof_sealed && !head.safe && !head.finalized);
            let candidate = peer
                .node
                .ledger()
                .load_candidate_record(self.authority.chain_id, self.block_hash)
                .unwrap()
                .unwrap();
            assert!(!candidate.chain_canonical && !candidate.fork_choice_selected);
            assert!(!candidate.proof_sealed && !candidate.safe && !candidate.finalized);
        }
    }

    fn assert_prepared(&self, active: &[usize], round: u64) {
        let mut subject = None;
        for &index in active {
            let peer = &self.peers[index];
            let status = peer.driver.status();
            assert_eq!(status.height, 1);
            assert_eq!(status.round, round);
            assert!(status.prepared);
            let qc = peer
                .driver
                .prepared_qc()
                .expect("prepared means an actual QC");
            qc.verify(&self.authority.validator_set).unwrap();
            assert_eq!(qc.subject.block_hash, self.block_hash);
            assert_eq!(qc.subject.round, round);
            assert!(qc.signed_weight >= 3);
            assert_eq!(status.qc_hash, Some(qc.qc_hash));
            assert_eq!(
                peer.node.store().load_qc(qc.qc_hash).unwrap().as_ref(),
                Some(qc)
            );
            if let Some(expected) = &subject {
                assert_eq!(expected, &qc.subject);
            } else {
                subject = Some(qc.subject.clone());
            }
            // No driver has been given another validator's signing key.
            for validator in &self.authority.validator_set.validators {
                if validator.validator_id != self.id(index) {
                    assert!(peer
                        .node
                        .store()
                        .load_pending_outbox(self.authority.chain_id, validator.validator_id, 16,)
                        .unwrap()
                        .is_empty());
                }
            }
        }
        self.assert_unfinalized();
    }
}

fn driver_store_snapshot(peer: &DriverPeer) -> Vec<(Vec<u8>, Vec<u8>)> {
    peer.node
        .store()
        .db
        .iterator(IteratorMode::Start)
        .map(|item| {
            let (key, value) = item.unwrap();
            (key.to_vec(), value.to_vec())
        })
        .collect()
}

#[test]
fn native_seal_round_driver_three_survivors_replace_missing_leader_with_same_candidate_qc() {
    let mut cluster = DriverCluster::new(85_001);
    let active = cluster.without_initial_leader();
    let paused = (0..4).find(|index| !active.contains(index)).unwrap();
    let paused_before = driver_store_snapshot(&cluster.peers[paused]);
    let initial_leader = cluster.peers[paused].driver.status().leader_id;
    assert!(cluster.poll(&active, cluster.started).is_empty());
    cluster.pump(&active, cluster.started + DRIVER_INTERVAL, false);
    cluster.assert_prepared(&active, 1);
    assert_ne!(
        cluster.peers[active[0]].driver.status().leader_id,
        initial_leader
    );
    assert_eq!(driver_store_snapshot(&cluster.peers[paused]), paused_before);
    assert!(!cluster.peers[paused].driver.status().prepared);
}

#[test]
fn native_seal_round_driver_two_survivors_cannot_advance_or_prepare() {
    let mut cluster = DriverCluster::new(85_002);
    let active = cluster.without_initial_leader()[..2].to_vec();
    for _ in 0..5 {
        let messages = cluster.poll(&active, cluster.started + DRIVER_INTERVAL);
        cluster.deliver(&active, &messages, false);
    }
    for index in active {
        let peer = &cluster.peers[index];
        assert_eq!(peer.driver.status().round, 0);
        assert!(!peer.driver.status().prepared);
        assert!(peer.driver.prepared_qc().is_none());
        assert!(peer
            .node
            .store()
            .load_local_new_view_admission(cluster.authority.chain_id, 1, 1, 1,)
            .unwrap()
            .is_none());
    }
    cluster.assert_unfinalized();
}

#[test]
fn native_seal_round_driver_authentication_and_duplicates_do_not_sign_or_advance() {
    let mut cluster = DriverCluster::new(85_003);
    let active = cluster.without_initial_leader();
    let sender = active[0];
    let target = active[1];
    let source = cluster.source(sender);
    let wrong_source = cluster.source(active[2]);
    let messages = cluster.poll(&[sender], cluster.started + DRIVER_INTERVAL);
    let timeout = messages
        .into_iter()
        .map(|(_, message)| message)
        .find(|message| matches!(message, RoundMessage::Timeout(_)))
        .unwrap();
    let before = driver_store_snapshot(&cluster.peers[target]);
    let peer = &mut cluster.peers[target];
    assert!(peer
        .driver
        .ingest_authenticated(
            peer.node.ledger(),
            peer.node.store(),
            &wrong_source,
            timeout.clone(),
        )
        .is_err());
    let mut wrong_domain = timeout.clone();
    if let RoundMessage::Timeout(vote) = &mut wrong_domain {
        vote.context.chain_id += 1;
    }
    assert!(peer
        .driver
        .ingest_authenticated(peer.node.ledger(), peer.node.store(), &source, wrong_domain,)
        .is_err());
    let mut wrong_signature = timeout.clone();
    if let RoundMessage::Timeout(vote) = &mut wrong_signature {
        vote.signature[0] ^= 1;
    }
    assert!(peer
        .driver
        .ingest_authenticated(
            peer.node.ledger(),
            peer.node.store(),
            &source,
            wrong_signature,
        )
        .is_err());
    assert!(peer
        .driver
        .ingest_authenticated(
            peer.node.ledger(),
            peer.node.store(),
            &source,
            timeout.clone(),
        )
        .unwrap());
    assert!(!peer
        .driver
        .ingest_authenticated(peer.node.ledger(), peer.node.store(), &source, timeout,)
        .unwrap());
    assert_eq!(peer.driver.status().round, 0);
    assert_eq!(driver_store_snapshot(peer), before);
    cluster.assert_unfinalized();
}

#[test]
fn native_seal_round_driver_reordered_retransmissions_recover_and_stale_messages_are_ignored() {
    let mut cluster = DriverCluster::new(85_004);
    let active = cluster.without_initial_leader();
    let timeouts = cluster.poll(&active, cluster.started + DRIVER_INTERVAL);
    assert!(timeouts
        .iter()
        .any(|(_, message)| matches!(message, RoundMessage::Timeout(_))));
    cluster.deliver(&active, &timeouts, true);
    cluster.pump(&active, cluster.started + DRIVER_INTERVAL, true);
    cluster.assert_prepared(&active, 1);
    for (sender, message) in timeouts {
        if !matches!(message, RoundMessage::Timeout(_)) {
            continue;
        }
        let source = cluster.source(sender);
        for &recipient in &active {
            if recipient == sender {
                continue;
            }
            let peer = &mut cluster.peers[recipient];
            let before = driver_store_snapshot(peer);
            assert!(!peer
                .driver
                .ingest_authenticated(
                    peer.node.ledger(),
                    peer.node.store(),
                    &source,
                    message.clone(),
                )
                .unwrap());
            assert_eq!(driver_store_snapshot(peer), before);
        }
    }
    cluster.assert_prepared(&active, 1);
    // A valid later-round certificate is not authority to skip this paused node's round.
    let paused = (0..4).find(|index| !active.contains(index)).unwrap();
    let sender = active[0];
    let source = cluster.source(sender);
    let outputs = cluster.poll(&[sender], cluster.started + DRIVER_INTERVAL);
    let future = outputs
        .into_iter()
        .map(|(_, message)| message)
        .find(|message| matches!(message, RoundMessage::QuorumCertificate { .. }))
        .unwrap();
    let peer = &mut cluster.peers[paused];
    let before = driver_store_snapshot(peer);
    assert!(!peer
        .driver
        .ingest_authenticated(peer.node.ledger(), peer.node.store(), &source, future,)
        .unwrap());
    assert_eq!(peer.driver.status().round, 0);
    assert_eq!(driver_store_snapshot(peer), before);
}

#[test]
fn native_seal_round_driver_timeout_restart_replays_original_without_resigning() {
    let mut cluster = DriverCluster::new(85_005);
    let active = cluster.without_initial_leader();
    let index = active[0];
    let first = cluster.poll(&[index], cluster.started + DRIVER_INTERVAL);
    let signed_timeout = first
        .into_iter()
        .map(|(_, message)| message)
        .find(|message| matches!(message, RoundMessage::Timeout(_)))
        .unwrap();
    let restarted = cluster.started + DRIVER_INTERVAL + Duration::from_secs(1);
    cluster.restart(index, restarted);
    let before = driver_store_snapshot(&cluster.peers[index]);
    let early = cluster.poll(&[index], restarted);
    assert!(early.iter().any(|(_, message)| message == &signed_timeout));
    assert!(early.iter().all(|(_, message)| !matches!(
        message,
        RoundMessage::Proposal { .. } | RoundMessage::Vote { .. }
    )));
    assert_eq!(driver_store_snapshot(&cluster.peers[index]), before);
    let replay = cluster.poll(&[index], restarted + DRIVER_INTERVAL);
    let replayed = replay
        .into_iter()
        .map(|(_, message)| message)
        .find(|message| matches!(message, RoundMessage::Timeout(_)))
        .unwrap();
    match (signed_timeout, replayed) {
        (RoundMessage::Timeout(first), RoundMessage::Timeout(replay)) => assert_eq!(first, replay),
        _ => unreachable!("both messages were filtered to timeout votes"),
    }
    cluster.pump(&active, restarted + DRIVER_INTERVAL, false);
    cluster.assert_prepared(&active, 1);
}

#[test]
fn native_seal_round_driver_restart_after_round_advance_and_admission_recovers_without_resigning() {
    let mut cluster = DriverCluster::new(85_006);
    let active = cluster.without_initial_leader();
    let now = cluster.started + DRIVER_INTERVAL;
    let timeouts = cluster.poll(&active, now);
    cluster.deliver(&active, &timeouts, false);
    let _lost_on_restart = cluster.poll(&active, now);
    for &index in &active {
        assert_eq!(cluster.peers[index].driver.status().round, 1);
        cluster.restart(index, now);
        assert_eq!(cluster.peers[index].driver.status().round, 1);
    }
    // Recollect volatile peer observations solely from each driver's retransmissions.
    for _ in 0..12 {
        let messages = cluster.poll(&active, now);
        let admitted = active.iter().all(|index| {
            cluster.peers[*index]
                .node
                .store()
                .load_local_new_view_admission(cluster.authority.chain_id, 1, 1, 1)
                .unwrap()
                .is_some()
        });
        if admitted {
            break;
        }
        cluster.deliver(&active, &messages, false);
    }
    for &index in &active {
        let peer = &cluster.peers[index];
        assert!(peer
            .node
            .store()
            .load_local_new_view_admission(cluster.authority.chain_id, 1, 1, 1,)
            .unwrap()
            .is_some());
        let before = peer
            .node
            .store()
            .load_pending_outbox(cluster.authority.chain_id, cluster.id(index), 16)
            .unwrap();
        cluster.restart(index, now);
        assert_eq!(
            cluster.peers[index]
                .node
                .store()
                .load_pending_outbox(cluster.authority.chain_id, cluster.id(index), 16,)
                .unwrap(),
            before
        );
    }
    cluster.pump(&active, now, true);
    cluster.assert_prepared(&active, 1);
    for &index in &active {
        let qc = cluster.peers[index].driver.prepared_qc().unwrap().clone();
        cluster.restart(index, now + Duration::from_secs(100));
        assert_eq!(cluster.peers[index].driver.prepared_qc(), Some(&qc));
    }
    cluster.assert_prepared(&active, 1);
}

#[test]
fn native_seal_round_driver_rejects_wrong_local_key_and_unexecuted_candidate() {
    let mut cluster = DriverCluster::new(85_007);
    let before = driver_store_snapshot(&cluster.peers[0]);
    let wrong_key = SigningKey::from_bytes(&[0xed; 32]);
    let peer = &mut cluster.peers[0];
    assert!(peer
        .driver
        .poll(
            peer.node.ledger(),
            peer.node.store(),
            &wrong_key,
            cluster.started,
        )
        .is_err());
    assert_eq!(driver_store_snapshot(peer), before);
    assert!(NovNativeSealRoundDriverV1::open(
        peer.node.ledger(),
        peer.node.store(),
        cluster.authority.clone(),
        [0xff; 32],
        None,
        validator_id_v1(peer.key.verifying_key().as_bytes()),
        cluster.started,
        DRIVER_INTERVAL,
    )
    .is_err());
    assert_eq!(driver_store_snapshot(peer), before);
    cluster.assert_unfinalized();
}

#[test]
fn native_seal_round_driver_four_online_prepare_round_zero_without_timeout() {
    let mut cluster = DriverCluster::new(85_008);
    let all = [0, 1, 2, 3];
    cluster.pump(&all, cluster.started, true);
    cluster.assert_prepared(&all, 0);
    for index in all {
        let peer = &cluster.peers[index];
        assert!(peer
            .node
            .store()
            .load_local_timeout(
                peer.node.ledger(),
                &cluster.authority.validator_set,
                1,
                0,
                cluster.id(index),
            )
            .unwrap()
            .is_none());
        assert!(peer
            .node
            .store()
            .load_local_new_view_admission(cluster.authority.chain_id, 1, 1, 1,)
            .unwrap()
            .is_none());
    }
}

#[test]
fn native_seal_round_driver_failover_retains_candidate_after_proposal_and_partial_votes() {
    let mut cluster = DriverCluster::new(85_009);
    let active = cluster.without_initial_leader();
    let leader = (0..4).find(|index| !active.contains(index)).unwrap();
    let leader_output = cluster.poll(&[leader], cluster.started);
    let proposal_only = leader_output
        .into_iter()
        .filter(|(_, message)| matches!(message, RoundMessage::Proposal { .. }))
        .collect::<Vec<_>>();
    assert_eq!(proposal_only.len(), 1);
    cluster.deliver(&active, &proposal_only, false);
    let withheld_votes = cluster.poll(&active, cluster.started);
    assert_eq!(
        withheld_votes
            .iter()
            .filter(|(_, message)| matches!(message, RoundMessage::Vote { .. }))
            .count(),
        3
    );
    // Each survivor has a durable height lock, but no one has seen a quorum of votes.
    for &index in &active {
        let peer = &cluster.peers[index];
        assert!(!peer.driver.status().prepared);
        let outbox = peer
            .node
            .store()
            .load_pending_outbox(cluster.authority.chain_id, cluster.id(index), 16)
            .unwrap();
        assert!(outbox
            .iter()
            .any(|entry| entry.round == 0 && entry.object_kind == "vote"));
    }
    let paused_before = driver_store_snapshot(&cluster.peers[leader]);
    cluster.pump(&active, cluster.started + DRIVER_INTERVAL, true);
    cluster.assert_prepared(&active, 1);
    assert_eq!(driver_store_snapshot(&cluster.peers[leader]), paused_before);
    for index in active {
        let outbox = cluster.peers[index]
            .node
            .store()
            .load_pending_outbox(cluster.authority.chain_id, cluster.id(index), 16)
            .unwrap();
        for round in [0, 1] {
            assert!(outbox
                .iter()
                .any(|entry| entry.round == round && entry.object_kind == "vote"));
        }
    }
}

#[test]
fn native_seal_round_driver_typed_sources_and_new_view_certificate_shape_fail_closed() {
    let mut cluster = DriverCluster::new(85_010);
    let active = cluster.without_initial_leader();
    let paused = (0..4).find(|index| !active.contains(index)).unwrap();
    let round_zero = cluster
        .poll(&[paused], cluster.started)
        .into_iter()
        .find(|(_, message)| matches!(message, RoundMessage::Proposal { .. }))
        .unwrap()
        .1;
    let now = cluster.started + DRIVER_INTERVAL;
    let mut trace = Vec::new();
    for _ in 0..16 {
        let messages = cluster.poll(&active, now);
        trace.extend(messages.clone());
        cluster.deliver(&active, &messages, false);
        if active
            .iter()
            .all(|index| cluster.peers[*index].driver.status().prepared)
        {
            break;
        }
    }
    cluster.assert_prepared(&active, 1);
    let before = driver_store_snapshot(&cluster.peers[paused]);
    let status_before = cluster.peers[paused].driver.status();
    for kind in 0..3 {
        let (sender, message) = trace
            .iter()
            .find(|(_, message)| {
                matches!(
                    (kind, message),
                    (0, RoundMessage::NewView { .. })
                        | (1, RoundMessage::Proposal { .. })
                        | (2, RoundMessage::Vote { .. })
                )
            })
            .unwrap();
        let wrong_source = cluster.source((*sender + 1) % 4);
        let peer = &mut cluster.peers[paused];
        assert!(peer
            .driver
            .ingest_authenticated(
                peer.node.ledger(),
                peer.node.store(),
                &wrong_source,
                message.clone(),
            )
            .is_err());
    }
    let (sender, round_one) = trace
        .iter()
        .find(|(_, message)| matches!(message, RoundMessage::Proposal { .. }))
        .unwrap();
    let source = cluster.source(*sender);
    let mut without_certificate = round_one.clone();
    let certificate = match &mut without_certificate {
        RoundMessage::Proposal { certificate, .. } => certificate.take().unwrap(),
        _ => unreachable!(),
    };
    let round_zero_source = cluster.source(paused);
    let mut wrong_round_zero = round_zero;
    if let RoundMessage::Proposal {
        certificate: carried,
        ..
    } = &mut wrong_round_zero
    {
        *carried = Some(certificate);
    }
    let peer = &mut cluster.peers[paused];
    for (source, message) in [
        (source, without_certificate),
        (round_zero_source, wrong_round_zero),
    ] {
        assert!(peer
            .driver
            .ingest_authenticated(peer.node.ledger(), peer.node.store(), &source, message,)
            .is_err());
    }
    assert_eq!(peer.driver.status(), status_before);
    assert_eq!(driver_store_snapshot(peer), before);
    cluster.assert_unfinalized();
}

#[test]
fn native_seal_round_driver_oversized_typed_input_rejects_before_cache_or_store_change() {
    let mut cluster = DriverCluster::new(85_011);
    let active = cluster.without_initial_leader();
    let sender = active[0];
    let target = active[1];
    let source = cluster.source(sender);
    let timeout = cluster
        .poll(&[sender], cluster.started + DRIVER_INTERVAL)
        .into_iter()
        .find(|(_, message)| matches!(message, RoundMessage::Timeout(_)))
        .unwrap()
        .1;
    let mut oversized = timeout.clone();
    if let RoundMessage::Timeout(vote) = &mut oversized {
        vote.signature.resize(
            crate::native_block_seal_overlay::NOV_NATIVE_SEAL_OVERLAY_MAX_WIRE_BYTES_V1 + 1,
            0,
        );
    }
    let peer = &mut cluster.peers[target];
    let before = driver_store_snapshot(peer);
    let status_before = peer.driver.status();
    let error = peer
        .driver
        .ingest_authenticated(peer.node.ledger(), peer.node.store(), &source, oversized)
        .expect_err("oversized nested signature must fail at the typed-input bound");
    assert!(error.to_string().contains("bounded typed-input size"));
    assert_eq!(peer.driver.status(), status_before);
    assert_eq!(driver_store_snapshot(peer), before);
    assert!(
        peer.driver
            .ingest_authenticated(peer.node.ledger(), peer.node.store(), &source, timeout,)
            .unwrap(),
        "rejected oversize must not occupy the signer's cache slot"
    );
    assert_eq!(driver_store_snapshot(peer), before);
    cluster.assert_unfinalized();
}

#[test]
fn native_seal_round_driver_owner_handles_and_durable_configuration_cannot_change() {
    let mut cluster = DriverCluster::new(85_012);
    let source = cluster.source(2);
    let timeout = cluster
        .poll(&[2], cluster.started + DRIVER_INTERVAL)
        .into_iter()
        .find(|(_, message)| matches!(message, RoundMessage::Timeout(_)))
        .unwrap()
        .1;
    let local_id = cluster.id(0);
    let first_before = driver_store_snapshot(&cluster.peers[0]);
    let other_before = driver_store_snapshot(&cluster.peers[1]);
    {
        let (first, rest) = cluster.peers.split_at_mut(1);
        let peer = &mut first[0];
        let other = &rest[0];
        assert!(peer
            .driver
            .poll(
                peer.node.ledger(),
                other.node.store(),
                &peer.key,
                cluster.started,
            )
            .is_err());
        assert!(peer
            .driver
            .ingest_authenticated(
                other.node.ledger(),
                peer.node.store(),
                &source,
                timeout.clone(),
            )
            .is_err());
    }
    assert_eq!(driver_store_snapshot(&cluster.peers[0]), first_before);
    assert_eq!(driver_store_snapshot(&cluster.peers[1]), other_before);
    let peer = &mut cluster.peers[0];
    let mut transport = cluster.authority.transport_bindings.clone();
    transport[0].transport_peer_id = "ab".repeat(32);
    let changed = NovNativeSealEpochAuthorityV1::derive_operator_pinned_genesis_epoch(
        peer.node.ledger(),
        cluster.authority.validator_set.clone(),
        transport,
    )
    .unwrap();
    assert!(NovNativeSealRoundDriverV1::open(
        peer.node.ledger(),
        peer.node.store(),
        changed,
        cluster.block_hash,
        None,
        local_id,
        cluster.started,
        DRIVER_INTERVAL,
    )
    .is_err());
    assert_eq!(driver_store_snapshot(peer), first_before);
    // Deliberate disk corruption fixture, not a supported configuration update.
    let binding_key = format!(
        "native_block_seal/v1/round-driver/{}/1/1/{}",
        cluster.authority.chain_id,
        hex_v1(&local_id)
    );
    let original = peer
        .node
        .store()
        .db
        .get(binding_key.as_bytes())
        .unwrap()
        .unwrap();
    let mut corrupt: serde_json::Value = serde_json::from_slice(&original).unwrap();
    corrupt["schema"] = serde_json::json!("tampered-round-driver-owner");
    peer.node
        .store()
        .db
        .put(
            binding_key.as_bytes(),
            serde_json::to_vec(&corrupt).unwrap(),
        )
        .unwrap();
    let corrupted_before = driver_store_snapshot(peer);
    assert!(peer
        .driver
        .poll(
            peer.node.ledger(),
            peer.node.store(),
            &peer.key,
            cluster.started,
        )
        .is_err());
    assert!(peer
        .driver
        .ingest_authenticated(peer.node.ledger(), peer.node.store(), &source, timeout,)
        .is_err());
    assert!(NovNativeSealRoundDriverV1::open(
        peer.node.ledger(),
        peer.node.store(),
        cluster.authority.clone(),
        cluster.block_hash,
        None,
        local_id,
        cluster.started,
        DRIVER_INTERVAL,
    )
    .is_err());
    assert_eq!(driver_store_snapshot(peer), corrupted_before);
    cluster.assert_unfinalized();
}

#[test]
fn native_seal_round_driver_missing_or_corrupt_timeout_watermark_blocks_restart() {
    let mut cluster = DriverCluster::new(85_013);
    let active = cluster.without_initial_leader()[..2].to_vec();
    let _withheld = cluster.poll(&active, cluster.started + DRIVER_INTERVAL);
    for (case, index) in active.into_iter().enumerate() {
        let local_id = cluster.id(index);
        let peer = &mut cluster.peers[index];
        let watermark = format!(
            "native_block_seal/v1/timeout/{}/1/{}/watermark",
            cluster.authority.chain_id,
            hex_v1(&local_id)
        );
        assert!(peer
            .node
            .store()
            .db
            .get(watermark.as_bytes())
            .unwrap()
            .is_some());
        // Deliberately corrupt only this disposable node's signing safety metadata.
        if case == 0 {
            peer.node.store().db.delete(watermark.as_bytes()).unwrap();
        } else {
            peer.node
                .store()
                .db
                .put(watermark.as_bytes(), b"corrupt watermark")
                .unwrap();
        }
        peer.node.reopen_store();
        let before = driver_store_snapshot(peer);
        assert!(NovNativeSealRoundDriverV1::open(
            peer.node.ledger(),
            peer.node.store(),
            cluster.authority.clone(),
            cluster.block_hash,
            None,
            local_id,
            cluster.started + DRIVER_INTERVAL,
            DRIVER_INTERVAL,
        )
        .is_err());
        assert_eq!(driver_store_snapshot(peer), before);
    }
    cluster.assert_unfinalized();
}

#[test]
fn native_seal_round_driver_resumed_initial_leader_catches_up_from_prepared_peer_retransmissions() {
    let mut cluster = DriverCluster::new(85_014);
    let active = cluster.without_initial_leader();
    let paused = (0..4).find(|index| !active.contains(index)).unwrap();
    let now = cluster.started + DRIVER_INTERVAL;
    cluster.pump(&active, now, true);
    cluster.assert_prepared(&active, 1);
    assert_eq!(cluster.peers[paused].driver.status().round, 0);
    assert!(!cluster.peers[paused].driver.status().prepared);
    // Prepared peers must retain the preceding TC: a QC alone cannot skip round 0.
    cluster.pump(&[0, 1, 2, 3], now, true);
    cluster.assert_prepared(&[0, 1, 2, 3], 1);
}

#[test]
fn native_seal_round_driver_prepared_qc_and_all_indexes_cannot_disappear_silently() {
    let mut cluster = DriverCluster::new(85_015);
    cluster.pump(&[0, 1, 2, 3], cluster.started, false);
    cluster.assert_prepared(&[0, 1, 2, 3], 0);
    let local_id = cluster.id(0);
    let peer = &mut cluster.peers[0];
    let qc = peer.driver.prepared_qc().unwrap().clone();
    // Explicit corrupt-disk fixture: deleting the object together with every index
    // must not turn an already prepared owner into an empty inventory on restart.
    for key in [
        qc_object_key_v1(&qc.qc_hash),
        qc_subject_index_key_v1(&qc.subject_hash),
        qc_block_index_key_v1(cluster.authority.chain_id, &cluster.block_hash),
        qc_height_index_key_v1(cluster.authority.chain_id, 1, 1),
    ] {
        peer.node.store().db.delete(key.as_bytes()).unwrap();
    }
    let before = driver_store_snapshot(peer);
    assert!(peer
        .driver
        .poll(
            peer.node.ledger(),
            peer.node.store(),
            &peer.key,
            cluster.started,
        )
        .is_err());
    assert_eq!(driver_store_snapshot(peer), before);
    assert!(!peer.driver.status().prepared);
    assert!(peer.driver.prepared_qc().is_none());
    assert_eq!(
        peer.driver.status().phase,
        crate::native_block_seal::round_driver::NovNativeSealRoundDriverPhaseV1::Halted
    );
    peer.node.reopen_store();
    assert!(NovNativeSealRoundDriverV1::open(
        peer.node.ledger(),
        peer.node.store(),
        cluster.authority.clone(),
        cluster.block_hash,
        None,
        local_id,
        cluster.started,
        DRIVER_INTERVAL,
    )
    .is_err());
    assert_eq!(driver_store_snapshot(peer), before);
    cluster.assert_unfinalized();
}

#[test]
fn native_seal_round_driver_recovers_qc_persisted_before_completion_pin() {
    let mut cluster = DriverCluster::new(85_016);
    let active = cluster.without_initial_leader();
    let now = cluster.started + DRIVER_INTERVAL;
    cluster.pump(&active, now, false);
    let index = active[0];
    let hash = cluster.peers[index].driver.prepared_qc().unwrap().qc_hash;
    let pin = format!(
        "native_block_seal/v1/round-driver/{}/1/1/{}/prepared",
        cluster.authority.chain_id,
        hex_v1(&cluster.id(index))
    );
    let peer = &cluster.peers[index];
    assert!(peer.node.store().db.get(pin.as_bytes()).unwrap().is_some());
    // Model the crash prefix after the core QC batch but before the driver's
    // completion batch. The QC, proposal and all indexes remain intact.
    peer.node.store().db.delete(pin.as_bytes()).unwrap();
    cluster.restart(index, now);
    assert_eq!(cluster.peers[index].driver.status().qc_hash, Some(hash));
    assert_eq!(
        read_json_v1::<[u8; 32]>(
            &cluster.peers[index].node.store().db,
            pin.as_bytes(),
            "recovered test prepared QC"
        )
        .unwrap(),
        Some(hash)
    );
    cluster.assert_prepared(&active, 1);
}
