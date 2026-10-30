//! Regression tests for bounded transport fragmentation, not consensus finality.

use super::*;

#[test]
fn bounded_feedback_matches_original_full_snapshot_after_reordering_and_duplicates() -> Result<()> {
    use crate::novorudp::{missing_count, select_first_missing_window, NovoRudpWindowConfig};
    let now = Instant::now();
    let wire = vec![7; 137 * UDP_CHUNK_BYTES - 17];
    let body = OutgoingMessage::with_profile(DOMAIN, wire, 200_000, FragmentProfile::NativeUdpV2)?;
    let mut receiver = Reassembler::with_profile(
        DOMAIN,
        vec![PEERS[0].into()],
        ReassemblyLimits {
            max_message_bytes: 200_000,
            messages: 1,
            bytes: 200_000,
            peer_messages: 1,
            peer_bytes: 200_000,
            ttl: Duration::from_secs(5),
        },
        FragmentProfile::NativeUdpV2,
    )?;
    // 53 is coprime to 137: reorder every fragment exactly once.
    for step in 0..137 {
        let index = (step * 53) % 137;
        let frame = body.frame(index)?;
        assert_eq!(
            receiver.push(PEERS[0], &frame, now)?,
            FragmentAdmission::Accepted
        );
        assert_eq!(
            receiver.push(PEERS[0], &frame, now)?,
            FragmentAdmission::Duplicate
        );
        let (total, ranges) = receiver.missing_ranges(PEERS[0], body.id()).unwrap();
        for width in [1, 7, 64] {
            let config = NovoRudpWindowConfig {
                window_size: width,
                ..Default::default()
            };
            assert_eq!(
                receiver.window_snapshot(PEERS[0], body.id(), &config)?,
                Some((
                    total,
                    missing_count(&ranges),
                    select_first_missing_window(&ranges, total, &config)
                ))
            );
        }
    }
    assert_eq!(receiver.expire(now + Duration::from_secs(5)), 1);
    assert!(receiver
        .window_snapshot(PEERS[0], body.id(), &NovoRudpWindowConfig::default())?
        .is_none());
    assert_eq!(receiver.reserved_bytes(), 0);
    Ok(())
}

const DOMAIN: Hash = [0x31; 32];
const PEERS: [&str; 3] = ["validator-a", "validator-b", "validator-c"];

#[test]
fn expiry_skips_repeated_polls_until_the_first_exact_deadline() -> Result<()> {
    let now = Instant::now();
    let ttl = limits().ttl;
    let done = message(17, 41);
    let pending = message(CHUNK_BYTES + 1, 42);
    let mut receiver = receiver().with_completion_history();
    push_all(&mut receiver, PEERS[0], &done, now);
    assert!(receiver.poll_complete(now, 64)?.is_some());
    receiver.push(PEERS[1], &pending.frame(0)?, now + Duration::from_secs(1))?;
    let before_deadline = now + ttl - Duration::from_nanos(1);
    for _ in 0..64 {
        assert_eq!(receiver.expire(before_deadline), 0);
        assert!(receiver.poll_complete(before_deadline, 64)?.is_none());
        assert_eq!(
            receiver.push(PEERS[0], &done.frame(0)?, before_deadline)?,
            FragmentAdmission::Duplicate
        );
    }
    assert_eq!(receiver.expiry_scans, 0);
    assert_eq!(receiver.completed.len(), 1);
    assert_eq!(receiver.reserved_bytes(), pending.byte_len());
    assert_eq!(receiver.expire(now + ttl), 0);
    assert_eq!(receiver.expiry_scans, 1);
    assert!(receiver.completed.is_empty());
    assert_eq!(receiver.oldest_created, Some(now + Duration::from_secs(1)));
    assert_eq!(receiver.expire(now + ttl + Duration::from_secs(1)), 1);
    assert_eq!(receiver.expiry_scans, 2);
    assert_eq!(receiver.reserved_bytes(), 0);
    assert_eq!(receiver.oldest_created, None);
    Ok(())
}

#[test]
fn completion_moves_to_history_ttl_and_repairs_an_early_expiry_hint() -> Result<()> {
    let now = Instant::now();
    let ttl = limits().ttl;
    let completed_at = now + Duration::from_secs(1);
    let body = message(19, 43);
    let mut receiver = receiver().with_completion_history();
    push_all(&mut receiver, PEERS[0], &body, now);
    assert!(receiver.poll_complete(completed_at, 64)?.is_some());
    assert_eq!(receiver.reserved_bytes(), 0);
    // Removal leaves an early hint, never a late one. The full scan repairs it.
    assert_eq!(receiver.oldest_created, Some(now));
    assert_eq!(receiver.expire(now + ttl), 0);
    assert_eq!(receiver.expiry_scans, 1);
    assert_eq!(receiver.oldest_created, Some(completed_at));
    let before_deadline = completed_at + ttl - Duration::from_nanos(1);
    assert_eq!(receiver.expire(before_deadline), 0);
    assert_eq!(receiver.expiry_scans, 1);
    assert_eq!(
        receiver.push(PEERS[0], &body.frame(0)?, before_deadline)?,
        FragmentAdmission::Duplicate
    );
    assert_eq!(receiver.expire(completed_at + ttl), 0);
    assert!(receiver.completed.is_empty());
    assert_eq!(receiver.oldest_created, None);
    assert_eq!(
        receiver.push(PEERS[0], &body.frame(0)?, completed_at + ttl)?,
        FragmentAdmission::Accepted
    );
    Ok(())
}

#[test]
fn history_eviction_and_session_retirement_leave_only_conservative_expiry_hints() -> Result<()> {
    let now = Instant::now();
    let ttl = limits().ttl;
    let mut settings = limits();
    settings.messages = 2;
    settings.peer_messages = 1;
    let mut receiver = receiver_with(settings).with_completion_history();
    let first = message(20, 44);
    let second = message(21, 45);
    let pending = message(CHUNK_BYTES + 1, 46);
    push_all(&mut receiver, PEERS[0], &first, now);
    assert!(receiver.poll_complete(now, 64)?.is_some());
    let second_at = now + Duration::from_secs(1);
    push_all(&mut receiver, PEERS[0], &second, second_at);
    assert!(receiver.poll_complete(second_at, 64)?.is_some());
    assert!(receiver
        .completed_total(PEERS[0], first.id(), second_at)
        .is_none());
    assert_eq!(
        receiver.completed_total(PEERS[0], second.id(), second_at),
        Some(1)
    );
    let pending_at = now + Duration::from_secs(2);
    // The evicted object can be received again. Session retirement must release
    // its new reservation as well as the newer completion history.
    receiver.push(PEERS[0], &first.frame(0)?, pending_at)?;
    receiver.push(PEERS[1], &pending.frame(0)?, pending_at)?;
    assert_eq!(receiver.retire_peer_session(PEERS[0]), 1);
    assert!(receiver.completed.is_empty());
    assert_eq!(receiver.oldest_created, Some(now));
    assert_eq!(receiver.expire(now + ttl), 0);
    assert_eq!(receiver.oldest_created, Some(pending_at));
    assert_eq!(receiver.reserved_bytes(), pending.byte_len());
    assert_eq!(receiver.expire(pending_at + ttl), 1);
    assert_eq!(receiver.reserved_bytes(), 0);
    assert_eq!(receiver.oldest_created, None);
    Ok(())
}

#[test]
fn earlier_clock_input_updates_hint_and_does_not_hide_expired_objects() -> Result<()> {
    let now = Instant::now();
    let ttl = limits().ttl;
    let future = now + Duration::from_secs(3);
    let mut receiver = receiver().with_completion_history();
    let later = message(CHUNK_BYTES + 1, 47);
    let earlier = message(22, 48);
    let middle = message(CHUNK_BYTES + 2, 49);
    receiver.push(PEERS[0], &later.frame(0)?, future)?;
    push_all(&mut receiver, PEERS[1], &earlier, now);
    assert!(receiver.poll_complete(now, 64)?.is_some());
    assert_eq!(receiver.oldest_created, Some(now));
    assert_eq!(receiver.expire(now + ttl), 0);
    assert!(receiver.completed.is_empty());
    assert_eq!(receiver.oldest_created, Some(future));
    let scans = receiver.expiry_scans;
    assert_eq!(receiver.expire(now), 0);
    assert_eq!(receiver.expiry_scans, scans);
    let middle_at = now + Duration::from_secs(1);
    receiver.push(PEERS[2], &middle.frame(0)?, middle_at)?;
    assert_eq!(receiver.oldest_created, Some(middle_at));
    assert_eq!(receiver.expire(middle_at + ttl), 1);
    assert_eq!(receiver.reserved_bytes(), later.byte_len());
    assert_eq!(receiver.oldest_created, Some(future));
    assert_eq!(receiver.expire(future + ttl), 1);
    assert_eq!(receiver.reserved_bytes(), 0);
    assert_eq!(receiver.oldest_created, None);
    Ok(())
}

#[test]
fn completion_history_is_hash_checked_bounded_peer_isolated_and_does_not_renew() -> Result<()> {
    let now = Instant::now();
    let mut settings = limits();
    settings.messages = 2;
    settings.peer_messages = 1;
    let mut receiver = receiver_with(settings).with_completion_history();
    let first = message(99, 1);
    let second = message(100, 2);
    let third = message(101, 3);
    for (peer, body) in [(PEERS[0], &first), (PEERS[1], &second)] {
        push_all(&mut receiver, peer, body, now);
        assert!(receiver.completed_total(peer, body.id(), now).is_none());
        assert!(receiver.poll_complete(now, 64)?.is_some());
        assert_eq!(receiver.completed_total(peer, body.id(), now), Some(1));
    }
    assert_eq!(
        receiver.push(PEERS[0], &first.frame(0)?, now + Duration::from_secs(4))?,
        FragmentAdmission::Duplicate
    );
    assert!(receiver.poll_complete(now, 64)?.is_none());
    assert_eq!(receiver.reserved_bytes(), 0);
    push_all(
        &mut receiver,
        PEERS[0],
        &third,
        now + Duration::from_secs(4),
    );
    assert!(receiver
        .poll_complete(now + Duration::from_secs(4), 64)?
        .is_some());
    assert!(receiver
        .completed_total(PEERS[0], first.id(), now)
        .is_none());
    assert_eq!(
        receiver.completed_total(PEERS[1], second.id(), now),
        Some(1)
    );
    assert_eq!(receiver.completed.len(), 2);
    receiver.expire(now + Duration::from_secs(5));
    assert_eq!(receiver.completed.len(), 1);
    // Corrupt complete bytes never acquire a DONE identity.
    let mut broken = first.frame(0)?;
    *broken.last_mut().unwrap() ^= 1;
    receiver.push(PEERS[1], &broken, now + Duration::from_secs(5))?;
    assert!(receiver
        .poll_complete(now + Duration::from_secs(5), 64)
        .is_err());
    assert!(receiver
        .completed_total(PEERS[1], first.id(), now)
        .is_none());
    Ok(())
}

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
fn observed_reassembly_does_not_combine_expired_generations_or_count_duplicates() {
    use super::super::diagnostics::NativeTransferDiagnosticsV1;
    let now = Instant::now();
    let outgoing = message(CHUNK_BYTES * 2 + 17, 1);
    let mut receiver = receiver().with_completion_history();
    let observer = NativeTransferDiagnosticsV1::new();
    let tag = outgoing.id();
    let mut accept = |index, at, repair| {
        receiver
            .push_observed(PEERS[0], &outgoing.frame(index).unwrap(), at, |fact| {
                observer.fragment(tag, fact, index as u64, repair, at)
            })
            .unwrap()
    };
    assert_eq!(accept(0, now, false), FragmentAdmission::Accepted);
    assert_eq!(accept(0, now, true), FragmentAdmission::Duplicate);
    let later = now + limits().ttl;
    assert_eq!(accept(1, later, false), FragmentAdmission::Accepted);
    assert_eq!(accept(2, later, false), FragmentAdmission::Accepted);
    let partial = observer.snapshot_transfer(tag).unwrap();
    assert_eq!(partial.assemblies_started, 2);
    assert_eq!(partial.fragments, 2);
    assert_eq!(partial.duplicate_repair, 1);
    assert!(partial.fragments_complete_us.is_none());
    assert_eq!(accept(0, later, true), FragmentAdmission::Accepted);
    assert_eq!(accept(0, later, false), FragmentAdmission::Duplicate);
    let complete = observer.snapshot_transfer(tag).unwrap();
    assert_eq!(complete.fragments, 3);
    assert_eq!(complete.repair_fragments, 1);
    assert_eq!(complete.duplicate_data, 1);
    assert!(complete.fragments_complete_us.is_some());
    assert!(receiver.poll_complete(later, 64).unwrap().is_some());
    assert_eq!(
        receiver
            .push_observed(PEERS[0], &outgoing.frame(1).unwrap(), later, |fact| {
                observer.fragment(tag, fact, 1, true, later)
            })
            .unwrap(),
        FragmentAdmission::Duplicate
    );
    assert_eq!(
        observer
            .snapshot_transfer(tag)
            .unwrap()
            .completed_history_hits,
        1
    );
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
fn hash_progress_stops_at_a_hole_and_does_not_spin_or_renew_custody() -> Result<()> {
    let now = Instant::now();
    let body = message(3 * CHUNK_BYTES, 7);
    let mut receiver = receiver();
    receiver.push(PEERS[0], &body.frame(0)?, now)?;
    // A later fragment cannot make the missing middle fragment hashable.
    receiver.push(PEERS[0], &body.frame(2)?, now)?;
    let (item, progress) = receiver.poll_complete_with_progress(now, 1)?;
    assert!(item.is_none() && progress);
    assert_eq!(receiver.entries.values().next().unwrap().next_hash, 1);
    for _ in 0..3 {
        let (item, progress) = receiver.poll_complete_with_progress(now, 64)?;
        assert!(item.is_none() && !progress);
        assert_eq!(receiver.reserved_bytes(), 3 * CHUNK_BYTES);
    }
    let ttl = receiver.limits.ttl;
    let (item, progress) = receiver.poll_complete_with_progress(now + ttl, 64)?;
    assert!(item.is_none() && !progress);
    assert_eq!(receiver.reserved_bytes(), 0);
    assert_eq!(receiver.pending_messages(), 0);
    Ok(())
}

#[test]
fn hash_progress_preserves_quantum_and_verifies_before_completion() -> Result<()> {
    let now = Instant::now();
    let body = message(3 * CHUNK_BYTES, 8);
    let mut receiver = receiver();
    push_all(&mut receiver, PEERS[0], &body, now);
    let (item, progress) = receiver.poll_complete_with_progress(now, 2)?;
    assert!(item.is_none() && progress);
    assert_eq!(receiver.entries.values().next().unwrap().next_hash, 2);
    let (item, progress) = receiver.poll_complete_with_progress(now, 1)?;
    assert!(progress);
    assert_payload(item.unwrap(), PEERS[0], &body, &bytes(3 * CHUNK_BYTES, 8));
    let (item, progress) = receiver.poll_complete_with_progress(now, 1)?;
    assert!(item.is_none() && !progress);

    // A locally advanced hash never becomes an integrity bypass.
    let mut frame = body.frame(2)?;
    frame[HEADER] ^= 1;
    for frame in [body.frame(0)?, body.frame(1)?, frame] {
        receiver.push(PEERS[0], &frame, now)?;
    }
    assert!(receiver.poll_complete_with_progress(now, 2)?.0.is_none());
    assert!(receiver.poll_complete_with_progress(now, 1).is_err());
    assert_eq!(receiver.reserved_bytes(), 0);
    Ok(())
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
#[test]
fn native_profile_keeps_budget_ttl_peer_isolation_and_missing_snapshot() {
    let now = Instant::now();
    let limits = ReassemblyLimits {
        max_message_bytes: 4096,
        messages: 2,
        bytes: 8192,
        peer_messages: 1,
        peer_bytes: 4096,
        ttl: Duration::from_secs(5),
    };
    let mut receiver = Reassembler::with_profile(
        DOMAIN,
        vec!["a".into(), "b".into()],
        limits,
        FragmentProfile::NativeUdpV2,
    )
    .unwrap();
    let outgoing =
        OutgoingMessage::with_profile(DOMAIN, bytes(3000, 7), 4096, FragmentProfile::NativeUdpV2)
            .unwrap();
    let legacy = OutgoingMessage::new(DOMAIN, bytes(3000, 7), 4096).unwrap();
    assert_eq!(outgoing.id(), legacy.id()); // Body identity is not carrier-specific.
    assert!(receiver.push("a", &legacy.frame(0).unwrap(), now).is_err());
    assert_eq!(receiver.reserved_bytes(), 0);
    let last = outgoing.frame(3).unwrap();
    assert_eq!(
        receiver.push("a", &last, now).unwrap(),
        FragmentAdmission::Accepted
    );
    assert_eq!(
        receiver.missing_ranges("a", outgoing.id()).unwrap(),
        (4, vec![super::super::NovoRudpRange::new(0, 2)])
    );
    assert_eq!(receiver.reserved_bytes(), 3000);
    let other =
        OutgoingMessage::with_profile(DOMAIN, bytes(3000, 8), 4096, FragmentProfile::NativeUdpV2)
            .unwrap();
    assert_eq!(
        receiver.push("a", &other.frame(0).unwrap(), now).unwrap(),
        FragmentAdmission::Backpressure
    );
    assert_eq!(
        receiver.push("b", &other.frame(0).unwrap(), now).unwrap(),
        FragmentAdmission::Accepted
    );
    assert_eq!(receiver.reserved_bytes(), 6000);
    let mut bad = outgoing.frame(0).unwrap();
    bad[76..80].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(receiver.push("a", &bad, now).is_err());
    assert_eq!(receiver.reserved_bytes(), 6000);
    assert_eq!(
        receiver
            .push("a", &last, now + Duration::from_secs(4))
            .unwrap(),
        FragmentAdmission::Duplicate
    );
    assert_eq!(receiver.expire(now + Duration::from_secs(5)), 2);
    assert_eq!(receiver.reserved_bytes(), 0);
    assert!(receiver.missing_ranges("a", outgoing.id()).is_none());
}
