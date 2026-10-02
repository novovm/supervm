//! Regression tests for bounded transport fragmentation, not consensus finality.

use super::*;

const DOMAIN: Hash = [0x31; 32];
const PEERS: [&str; 3] = ["validator-a", "validator-b", "validator-c"];

fn limits() -> ReassemblyLimits {
    ReassemblyLimits {
        max_message_bytes: 4 * CHUNK_BYTES,
        messages: 8,
        bytes: 12 * CHUNK_BYTES,
        peer_messages: 4,
        peer_bytes: 6 * CHUNK_BYTES,
        ttl: Duration::from_secs(5),
    }
}

fn receiver_with(limits: ReassemblyLimits) -> Reassembler {
    Reassembler::new(
        DOMAIN,
        PEERS.iter().map(|peer| (*peer).into()).collect(),
        limits,
    )
    .unwrap()
}

fn receiver() -> Reassembler {
    receiver_with(limits())
}

fn bytes(length: usize, seed: u8) -> Vec<u8> {
    (0..length)
        .map(|index| (index as u8).wrapping_add(seed))
        .collect()
}

fn message(length: usize, seed: u8) -> OutgoingMessage {
    OutgoingMessage::new(DOMAIN, bytes(length, seed), limits().max_message_bytes).unwrap()
}

fn push_all(receiver: &mut Reassembler, peer: &str, message: &OutgoingMessage, now: Instant) {
    for index in 0..message.frame_count() {
        assert_eq!(
            receiver
                .push(peer, &message.frame(index).unwrap(), now)
                .unwrap(),
            FragmentAdmission::Accepted
        );
    }
}

fn assert_payload(
    completed: CompletedMessage,
    peer: &str,
    message: &OutgoingMessage,
    expected: &[u8],
) {
    assert_eq!(completed.peer, peer);
    assert_eq!(completed.id, message.id());
    assert_eq!(completed.chunks.concat(), expected);
    assert!(completed
        .chunks
        .iter()
        .all(|chunk| !chunk.is_empty() && chunk.len() <= CHUNK_BYTES));
}

#[test]
fn qc_sized_and_larger_body_round_trip_out_of_order_without_raising_carrier_limit() {
    // Opaque test bytes of a maximum-size QC and a larger body: this does not
    // establish QC validity, business execution, or consensus finality.
    for length in [329_743, 3 * CHUNK_BYTES + 321] {
        assert!(length > 192 * 1024);
        let now = Instant::now();
        let expected = bytes(length, 0x67);
        let outgoing =
            OutgoingMessage::new(DOMAIN, expected.clone(), limits().max_message_bytes).unwrap();
        let mut receiver = receiver();
        for index in (0..outgoing.frame_count()).rev() {
            let frame = outgoing.frame(index).unwrap();
            assert!(frame.len() <= NETWORK_WORKER_MAX_PAYLOAD_BYTES);
            assert_eq!(
                receiver.push(PEERS[0], &frame, now).unwrap(),
                FragmentAdmission::Accepted
            );
            assert_eq!(
                receiver.push(PEERS[0], &frame, now).unwrap(),
                FragmentAdmission::Duplicate
            );
            assert_eq!(receiver.pending_messages(), 1);
            assert_eq!(receiver.reserved_bytes(), length);
        }
        let completed = receiver
            .poll_complete(now, outgoing.frame_count())
            .unwrap()
            .unwrap();
        assert_payload(completed, PEERS[0], &outgoing, &expected);
        assert_eq!(receiver.reserved_bytes(), 0);
        assert_eq!(receiver.pending_messages(), 0);
    }
}

#[test]
fn outgoing_validation_and_chunk_boundaries_are_checked() {
    assert!(OutgoingMessage::new([0; 32], vec![1], 1).is_err());
    assert!(OutgoingMessage::new(DOMAIN, Vec::new(), 1).is_err());
    assert!(OutgoingMessage::new(DOMAIN, vec![1], 0).is_err());
    assert!(OutgoingMessage::new(DOMAIN, vec![1], MAX_MESSAGE_BYTES + 1).is_err());
    assert!(OutgoingMessage::new(DOMAIN, vec![1, 2], 1).is_err());
    for length in [1, CHUNK_BYTES, CHUNK_BYTES + 1, 2 * CHUNK_BYTES] {
        let outgoing = message(length, 1);
        assert_eq!(outgoing.frame_count(), length.div_ceil(CHUNK_BYTES));
        assert!(outgoing.frame(outgoing.frame_count()).is_err());
        assert!(outgoing.frame(usize::MAX).is_err());
        for index in 0..outgoing.frame_count() {
            let frame = outgoing.frame(index).unwrap();
            assert_eq!(
                frame.len(),
                HEADER + (length - index * CHUNK_BYTES).min(CHUNK_BYTES)
            );
            assert!(frame.len() <= NETWORK_WORKER_MAX_PAYLOAD_BYTES);
        }
    }
}

#[test]
fn invalid_peers_domains_limits_and_quantums_are_rejected() {
    let configured = vec![PEERS[0].to_owned()];
    assert!(Reassembler::new([0; 32], configured.clone(), limits()).is_err());
    for peers in [
        vec![],
        vec![String::new()],
        vec!["x".repeat(257)],
        vec![PEERS[0].into(), PEERS[0].into()],
        vec!["a".into(); 1025],
    ] {
        assert!(Reassembler::new(DOMAIN, peers, limits()).is_err());
    }
    let mut invalid = Vec::new();
    let mut current = limits();
    current.max_message_bytes = 0;
    invalid.push(current);
    let mut current = limits();
    current.max_message_bytes = MAX_MESSAGE_BYTES + 1;
    invalid.push(current);
    let mut current = limits();
    current.messages = 0;
    invalid.push(current);
    let mut current = limits();
    current.messages = 4097;
    invalid.push(current);
    let mut current = limits();
    current.peer_messages = 0;
    invalid.push(current);
    let mut current = limits();
    current.peer_messages = current.messages + 1;
    invalid.push(current);
    let mut current = limits();
    current.peer_bytes = 0;
    invalid.push(current);
    let mut current = limits();
    current.peer_bytes = current.bytes + 1;
    invalid.push(current);
    let mut current = limits();
    current.ttl = Duration::ZERO;
    invalid.push(current);
    for config in invalid {
        assert!(Reassembler::new(DOMAIN, configured.clone(), config).is_err());
    }
    let mut receiver = receiver();
    let now = Instant::now();
    let outgoing = message(10, 1);
    push_all(&mut receiver, PEERS[0], &outgoing, now);
    for quantum in [0, 65, usize::MAX] {
        assert!(receiver.poll_complete(now, quantum).is_err());
        assert_eq!(receiver.reserved_bytes(), 10);
        assert_eq!(receiver.entries.values().next().unwrap().next_hash, 0);
    }
    assert!(receiver.poll_complete(now, 64).unwrap().is_some());
}

#[test]
fn malformed_frames_wrong_domain_unknown_peer_and_trailing_bytes_spend_no_quota() {
    let now = Instant::now();
    let outgoing = message(31, 1);
    let valid = outgoing.frame(0).unwrap();
    let mut malformed = vec![
        Vec::new(),
        valid[..HEADER].to_vec(),
        valid[..valid.len() - 1].to_vec(),
        vec![0; NETWORK_WORKER_MAX_PAYLOAD_BYTES + 1],
    ];
    let mut frame = valid.clone();
    frame.push(0);
    malformed.push(frame);
    let mut frame = valid.clone();
    frame[0] ^= 1;
    malformed.push(frame);
    let mut frame = valid.clone();
    frame[8] ^= 1;
    malformed.push(frame);
    for length in [0u32, limits().max_message_bytes as u32 + 1, u32::MAX] {
        let mut frame = valid.clone();
        frame[72..76].copy_from_slice(&length.to_be_bytes());
        malformed.push(frame);
    }
    for index in [1u32, u32::MAX] {
        let mut frame = valid.clone();
        frame[76..80].copy_from_slice(&index.to_be_bytes());
        malformed.push(frame);
    }
    let mut receiver = receiver();
    assert!(receiver.push("unknown-peer", &valid, now).is_err());
    for frame in malformed {
        assert!(receiver.push(PEERS[0], &frame, now).is_err());
        assert_eq!(receiver.reserved_bytes(), 0);
        assert_eq!(receiver.pending_messages(), 0);
    }
    assert_eq!(
        receiver.push(PEERS[0], &valid, now).unwrap(),
        FragmentAdmission::Accepted
    );
}

#[test]
fn conflicting_duplicates_and_mixed_lengths_preserve_original_message() {
    let now = Instant::now();
    let length = CHUNK_BYTES + 23;
    let outgoing = message(length, 1);
    let first = outgoing.frame(0).unwrap();
    let mut receiver = receiver();
    assert_eq!(
        receiver.push(PEERS[0], &first, now).unwrap(),
        FragmentAdmission::Accepted
    );
    let mut conflicting = first.clone();
    conflicting[HEADER + 7] ^= 1;
    assert!(receiver
        .push(PEERS[0], &conflicting, now)
        .unwrap_err()
        .to_string()
        .contains("conflicting"));
    let mut mixed = first.clone();
    mixed[72..76].copy_from_slice(&((length + 1) as u32).to_be_bytes());
    assert!(receiver
        .push(PEERS[0], &mixed, now)
        .unwrap_err()
        .to_string()
        .contains("mixed message length"));
    assert_eq!(receiver.reserved_bytes(), length);
    assert_eq!(receiver.pending_messages(), 1);
    assert_eq!(
        receiver.push(PEERS[0], &first, now).unwrap(),
        FragmentAdmission::Duplicate
    );
    assert_eq!(
        receiver
            .push(PEERS[0], &outgoing.frame(1).unwrap(), now)
            .unwrap(),
        FragmentAdmission::Accepted
    );
    assert_payload(
        receiver.poll_complete(now, 2).unwrap().unwrap(),
        PEERS[0],
        &outgoing,
        &bytes(length, 1),
    );
    assert_eq!(receiver.reserved_bytes(), 0);
}

#[test]
fn first_small_tail_reserves_the_whole_declared_message() {
    let now = Instant::now();
    let outgoing = message(CHUNK_BYTES + 1, 1);
    let mut receiver = receiver();
    let tail = outgoing.frame(1).unwrap();
    assert_eq!(tail.len(), HEADER + 1);
    assert_eq!(
        receiver.push(PEERS[0], &tail, now).unwrap(),
        FragmentAdmission::Accepted
    );
    assert_eq!(receiver.reserved_bytes(), CHUNK_BYTES + 1);
    assert!(receiver.poll_complete(now, 64).unwrap().is_none());
    assert_eq!(receiver.entries.values().next().unwrap().next_hash, 0);
}

#[test]
fn global_message_reservations_release_on_completion() {
    let now = Instant::now();
    let mut config = limits();
    config.messages = 1;
    config.peer_messages = 1;
    let mut receiver = receiver_with(config);
    let first = message(10, 1);
    let second = message(11, 2);
    push_all(&mut receiver, PEERS[0], &first, now);
    assert_eq!(
        receiver
            .push(PEERS[1], &second.frame(0).unwrap(), now)
            .unwrap(),
        FragmentAdmission::Backpressure
    );
    assert_eq!(receiver.reserved_bytes(), 10);
    assert!(receiver.poll_complete(now, 1).unwrap().is_some());
    assert_eq!(
        receiver
            .push(PEERS[1], &second.frame(0).unwrap(), now)
            .unwrap(),
        FragmentAdmission::Accepted
    );
}

#[test]
fn global_byte_reservations_are_independent_of_peer_counts() {
    let now = Instant::now();
    let mut config = limits();
    config.bytes = 19;
    config.peer_bytes = 19;
    let mut receiver = receiver_with(config);
    let first = message(10, 1);
    let second = message(10, 2);
    push_all(&mut receiver, PEERS[0], &first, now);
    assert_eq!(
        receiver
            .push(PEERS[1], &second.frame(0).unwrap(), now)
            .unwrap(),
        FragmentAdmission::Backpressure
    );
    assert_eq!(receiver.pending_messages(), 1);
    assert_eq!(receiver.reserved_bytes(), 10);
    assert_eq!(receiver.expire(now + limits().ttl), 1);
    assert_eq!(receiver.reserved_bytes(), 0);
    assert_eq!(
        receiver
            .push(PEERS[1], &second.frame(0).unwrap(), now + limits().ttl)
            .unwrap(),
        FragmentAdmission::Accepted
    );
}

#[test]
fn per_peer_message_limit_leaves_capacity_for_other_peers() {
    let now = Instant::now();
    let mut config = limits();
    config.peer_messages = 1;
    let mut receiver = receiver_with(config);
    let first = message(10, 1);
    let second = message(11, 2);
    push_all(&mut receiver, PEERS[0], &first, now);
    assert_eq!(
        receiver
            .push(PEERS[0], &second.frame(0).unwrap(), now)
            .unwrap(),
        FragmentAdmission::Backpressure
    );
    assert_eq!(
        receiver
            .push(PEERS[1], &second.frame(0).unwrap(), now)
            .unwrap(),
        FragmentAdmission::Accepted
    );
    assert_eq!(receiver.pending_messages(), 2);
    assert_eq!(receiver.reserved_bytes(), 21);
}

#[test]
fn per_peer_byte_limit_leaves_capacity_for_other_peers() {
    let now = Instant::now();
    let mut config = limits();
    config.peer_bytes = 19;
    let mut receiver = receiver_with(config);
    let first = message(10, 1);
    let second = message(10, 2);
    push_all(&mut receiver, PEERS[0], &first, now);
    assert_eq!(
        receiver
            .push(PEERS[0], &second.frame(0).unwrap(), now)
            .unwrap(),
        FragmentAdmission::Backpressure
    );
    assert_eq!(
        receiver
            .push(PEERS[1], &second.frame(0).unwrap(), now)
            .unwrap(),
        FragmentAdmission::Accepted
    );
    assert_eq!(receiver.pending_messages(), 2);
    assert_eq!(receiver.reserved_bytes(), 20);
}

#[test]
fn identical_message_ids_from_distinct_peers_do_not_share_reservations() {
    let now = Instant::now();
    let outgoing = message(17, 1);
    let mut receiver = receiver();
    push_all(&mut receiver, PEERS[0], &outgoing, now);
    push_all(&mut receiver, PEERS[1], &outgoing, now);
    assert_eq!(receiver.pending_messages(), 2);
    assert_eq!(receiver.reserved_bytes(), 34);
    let first = receiver.poll_complete(now, 1).unwrap().unwrap();
    let second = receiver.poll_complete(now, 1).unwrap().unwrap();
    assert_ne!(first.peer, second.peer);
    assert_eq!(first.id, second.id);
    assert_eq!(receiver.reserved_bytes(), 0);
}

#[test]
fn duplicate_and_new_chunks_do_not_extend_original_ttl() {
    let now = Instant::now();
    let ttl = limits().ttl;
    let outgoing = message(CHUNK_BYTES + 17, 1);
    let frame = outgoing.frame(0).unwrap();
    let mut receiver = receiver();
    assert_eq!(
        receiver.push(PEERS[0], &frame, now).unwrap(),
        FragmentAdmission::Accepted
    );
    let late = now + ttl - Duration::from_nanos(1);
    assert_eq!(
        receiver.push(PEERS[0], &frame, late).unwrap(),
        FragmentAdmission::Duplicate
    );
    assert_eq!(
        receiver
            .push(PEERS[0], &outgoing.frame(1).unwrap(), late)
            .unwrap(),
        FragmentAdmission::Accepted
    );
    assert_eq!(receiver.expire(late), 0);
    assert_eq!(receiver.expire(now + ttl), 1);
    assert_eq!(receiver.pending_messages(), 0);
    assert_eq!(receiver.reserved_bytes(), 0);
    assert!(receiver.poll_complete(now + ttl, 64).unwrap().is_none());
}

#[test]
fn expiration_releases_only_expired_messages_and_push_reclaims_capacity() {
    let now = Instant::now();
    let ttl = limits().ttl;
    let mut config = limits();
    config.messages = 2;
    config.peer_messages = 1;
    let mut receiver = receiver_with(config);
    let first = message(10, 1);
    let second = message(11, 2);
    let third = message(12, 3);
    push_all(&mut receiver, PEERS[0], &first, now);
    push_all(
        &mut receiver,
        PEERS[1],
        &second,
        now + Duration::from_secs(1),
    );
    assert_eq!(
        receiver
            .push(PEERS[2], &third.frame(0).unwrap(), now + ttl)
            .unwrap(),
        FragmentAdmission::Accepted
    );
    assert_eq!(receiver.pending_messages(), 2);
    assert_eq!(receiver.reserved_bytes(), 23);
    assert!(!receiver
        .entries
        .contains_key(&(PEERS[0].into(), first.id())));
    assert_eq!(receiver.expire(now + ttl + Duration::from_secs(1)), 1);
    assert_eq!(receiver.reserved_bytes(), 12);
}

#[test]
fn hashing_is_incremental_and_duplicates_do_not_rehash_progress() {
    let now = Instant::now();
    let outgoing = message(3 * CHUNK_BYTES, 1);
    let mut receiver = receiver();
    push_all(&mut receiver, PEERS[0], &outgoing, now);
    for expected_next in 1..3 {
        assert!(receiver.poll_complete(now, 1).unwrap().is_none());
        assert_eq!(
            receiver.entries.values().next().unwrap().next_hash,
            expected_next
        );
        assert_eq!(
            receiver
                .push(PEERS[0], &outgoing.frame(0).unwrap(), now)
                .unwrap(),
            FragmentAdmission::Duplicate
        );
        assert_eq!(
            receiver.entries.values().next().unwrap().next_hash,
            expected_next
        );
    }
    assert_payload(
        receiver.poll_complete(now, 1).unwrap().unwrap(),
        PEERS[0],
        &outgoing,
        &bytes(3 * CHUNK_BYTES, 1),
    );
    assert_eq!(receiver.reserved_bytes(), 0);
}

#[test]
fn missing_chunk_does_not_block_another_peer_and_quantum_is_shared() {
    let now = Instant::now();
    let blocked = message(2 * CHUNK_BYTES, 1);
    let ready = message(2 * CHUNK_BYTES, 2);
    let mut receiver = receiver();
    assert_eq!(
        receiver
            .push(PEERS[0], &blocked.frame(0).unwrap(), now)
            .unwrap(),
        FragmentAdmission::Accepted
    );
    push_all(&mut receiver, PEERS[1], &ready, now);
    assert!(receiver.poll_complete(now, 2).unwrap().is_none());
    assert_eq!(
        receiver
            .entries
            .values()
            .map(|entry| entry.next_hash)
            .sum::<usize>(),
        2
    );
    assert_payload(
        receiver.poll_complete(now, 1).unwrap().unwrap(),
        PEERS[1],
        &ready,
        &bytes(2 * CHUNK_BYTES, 2),
    );
    assert_eq!(receiver.pending_messages(), 1);
    assert_eq!(receiver.reserved_bytes(), 2 * CHUNK_BYTES);
    assert!(receiver.poll_complete(now, 64).unwrap().is_none());
    assert_eq!(
        receiver
            .push(PEERS[0], &blocked.frame(1).unwrap(), now)
            .unwrap(),
        FragmentAdmission::Accepted
    );
    assert_payload(
        receiver.poll_complete(now, 1).unwrap().unwrap(),
        PEERS[0],
        &blocked,
        &bytes(2 * CHUNK_BYTES, 1),
    );
}

#[test]
fn completion_hash_failure_returns_quota_and_other_peers_keep_progress() {
    let now = Instant::now();
    let bad = message(CHUNK_BYTES + 17, 1);
    let good = message(CHUNK_BYTES + 17, 2);
    let mut receiver = receiver();
    for index in 0..bad.frame_count() {
        let mut frame = bad.frame(index).unwrap();
        if index == 1 {
            frame[HEADER] ^= 1;
        }
        assert_eq!(
            receiver.push(PEERS[0], &frame, now).unwrap(),
            FragmentAdmission::Accepted
        );
    }
    push_all(&mut receiver, PEERS[1], &good, now);
    assert!(receiver
        .poll_complete(now, 64)
        .err()
        .unwrap()
        .to_string()
        .contains("hash mismatch"));
    assert_eq!(receiver.pending_messages(), 1);
    assert_eq!(receiver.reserved_bytes(), CHUNK_BYTES + 17);
    assert_payload(
        receiver.poll_complete(now, 64).unwrap().unwrap(),
        PEERS[1],
        &good,
        &bytes(CHUNK_BYTES + 17, 2),
    );
    assert_eq!(receiver.reserved_bytes(), 0);
    push_all(&mut receiver, PEERS[0], &bad, now);
    assert_payload(
        receiver.poll_complete(now, 64).unwrap().unwrap(),
        PEERS[0],
        &bad,
        &bytes(CHUNK_BYTES + 17, 1),
    );
}
