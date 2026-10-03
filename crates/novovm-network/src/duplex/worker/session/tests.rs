use super::*;
use crate::duplex::worker::tests::{channels, config, id, key, unstarted};

// These unit results enter below the transport's correlation check. Actual
// malformed/duplicate wire outcomes are tested by the real client pump.
fn outcome(accepted: bool) -> RelayForwardOutcomeV1 {
    RelayForwardOutcomeV1 {
        disposition: if accepted {
            RelayForwardDispositionV1::Forwarded
        } else {
            RelayForwardDispositionV1::RejectedQueueFull
        },
        source_peer_id: id(1),
        target_peer_id: id(2),
        forwarded: accepted,
        queued: false,
        payload_treated_opaque: true,
        envelope_session_id: None,
        envelope_sequence: None,
        admitted_wire_bytes: 0,
    }
}

fn reserve(worker: &NetworkWorker, flights: &mut Flights, session: [u8; 16], tag: u64, byte: u8) {
    let mut shared = worker.shared.lock().unwrap();
    shared
        .outbound
        .push(&id(2), vec![byte], 1, Instant::now())
        .unwrap();
    let (peer, entry, bytes) = shared.outbound.reserve_next(|_| true, tag).unwrap();
    assert_eq!(bytes, vec![byte]);
    assert!(flights
        .insert(
            tag + 100,
            Flight::Data {
                peer,
                entry,
                reservation: tag,
                session
            }
        )
        .is_none());
}

#[test]
fn rejected_first_outcome_does_not_undo_already_accepted_successors() {
    let config = config(vec![id(2)]);
    let worker = unstarted(&config);
    let (channel, _) = channels(1, 2);
    let session = channel.session_id();
    let mut peers = BTreeMap::from([(
        id(2),
        Peer {
            phase: Phase::Active(channel),
            frame_sequence: 3,
            ..Peer::new()
        },
    )]);
    let mut flights = Flights::new();
    for tag in 1..=3 {
        reserve(&worker, &mut flights, session, tag, tag as u8);
    }
    settle_forward(
        103,
        outcome(true),
        &mut flights,
        &config,
        &mut peers,
        &worker.shared,
    )
    .unwrap();
    settle_forward(
        101,
        outcome(false),
        &mut flights,
        &config,
        &mut peers,
        &worker.shared,
    )
    .unwrap();
    assert!(!peers[&id(2)].can_send());
    assert_eq!(
        peers[&id(2)].frame_sequence,
        3,
        "no nonce/sequence rollback"
    );
    assert_eq!(worker.status().unwrap().outbound_messages, 2);
    settle_forward(
        102,
        outcome(true),
        &mut flights,
        &config,
        &mut peers,
        &worker.shared,
    )
    .unwrap();
    assert!(flights.is_empty());
    assert_eq!(worker.status().unwrap().relay_admissions, 2);
    let mut shared = worker.shared.lock().unwrap();
    let (_, _, bytes) = shared.outbound.reserve_next(|_| true, 4).unwrap();
    assert_eq!(
        bytes,
        vec![1],
        "only rejected original remains for fresh encryption"
    );
}

#[test]
fn inflight_ttl_keeps_original_charged_until_late_exact_outcome() {
    let mut config = config(vec![id(2)]);
    config.limits.outbound.max_messages = 1;
    config.limits.outbound.peer_max_messages = 1;
    let worker = unstarted(&config);
    let (channel, _) = channels(1, 2);
    let session = channel.session_id();
    let mut peers = BTreeMap::from([(
        id(2),
        Peer {
            phase: Phase::Active(channel),
            ..Peer::new()
        },
    )]);
    let mut flights = Flights::new();
    reserve(&worker, &mut flights, session, 1, 7);
    assert_eq!(
        worker.shared.lock().unwrap().outbound.expire(
            Instant::now() + Duration::from_secs(60),
            Duration::from_secs(1)
        ),
        0
    );
    assert!(matches!(
        worker.try_send(id(2), vec![8]).unwrap(),
        SendAdmission::Backpressure(_)
    ));
    settle_forward(
        101,
        outcome(true),
        &mut flights,
        &config,
        &mut peers,
        &worker.shared,
    )
    .unwrap();
    assert_eq!(worker.status().unwrap().outbound_messages, 0);
    assert_eq!(
        worker.try_send(id(2), vec![8]).unwrap(),
        SendAdmission::Accepted
    );
}

#[test]
fn old_generation_outcomes_settle_only_old_originals_without_isolating_replacement() {
    let config = config(vec![id(2)]);
    let worker = unstarted(&config);
    let (old, _) = channels(1, 2);
    let (new, _) = channels(1, 2);
    let new_session = new.session_id();
    assert_ne!(old.session_id(), new_session);
    let mut flights = Flights::new();
    reserve(&worker, &mut flights, old.session_id(), 1, 1);
    reserve(&worker, &mut flights, old.session_id(), 2, 2);
    let mut peers = BTreeMap::from([(
        id(2),
        Peer {
            phase: Phase::Active(new),
            frame_sequence: 99,
            ..Peer::new()
        },
    )]);
    assert_eq!(
        worker.try_send(id(2), vec![3]).unwrap(),
        SendAdmission::Accepted
    );
    settle_forward(
        101,
        outcome(false),
        &mut flights,
        &config,
        &mut peers,
        &worker.shared,
    )
    .unwrap();
    settle_forward(
        102,
        outcome(true),
        &mut flights,
        &config,
        &mut peers,
        &worker.shared,
    )
    .unwrap();
    assert!(peers[&id(2)].can_send());
    assert!(peers[&id(2)].expected_session(new_session));
    assert_eq!(peers[&id(2)].frame_sequence, 99);
    let mut shared = worker.shared.lock().unwrap();
    assert_eq!(shared.outbound.count, 2);
    let (_, _, first) = shared.outbound.reserve_next(|_| true, 3).unwrap();
    let (_, _, second) = shared.outbound.reserve_next(|_| true, 4).unwrap();
    assert_eq!((first, second), (vec![1], vec![3]));
}

#[test]
fn unknown_or_duplicate_ticket_never_removes_an_unrelated_original() {
    let config = config(vec![id(2)]);
    let worker = unstarted(&config);
    let (channel, _) = channels(1, 2);
    let session = channel.session_id();
    let mut peers = BTreeMap::from([(
        id(2),
        Peer {
            phase: Phase::Active(channel),
            ..Peer::new()
        },
    )]);
    let mut flights = Flights::new();
    reserve(&worker, &mut flights, session, 1, 1);
    reserve(&worker, &mut flights, session, 2, 2);
    assert!(settle_forward(
        100,
        outcome(true),
        &mut flights,
        &config,
        &mut peers,
        &worker.shared
    )
    .is_err());
    assert_eq!(worker.status().unwrap().outbound_messages, 2);
    settle_forward(
        101,
        outcome(true),
        &mut flights,
        &config,
        &mut peers,
        &worker.shared,
    )
    .unwrap();
    assert!(settle_forward(
        101,
        outcome(true),
        &mut flights,
        &config,
        &mut peers,
        &worker.shared
    )
    .is_err());
    assert_eq!(worker.status().unwrap().outbound_messages, 1);
    assert_eq!(flights.len(), 1);
}

#[test]
fn authenticated_response_before_offer_outcome_does_not_reset_activated_sequence() {
    let config = config(vec![id(2)]);
    let worker = unstarted(&config);
    let initiator = NodeHandshakeInitiatorV1::start(&key(1), id(2), now_ms(), 5000).unwrap();
    let session = initiator.offer().session_id;
    let responder = NodeHandshakeResponderV1::respond(
        initiator.offer(),
        &key(2),
        now_ms(),
        5000,
        &mut HandshakeReplayCacheV1::new(256),
    )
    .unwrap();
    let response = responder.response().clone();
    let mut peers = BTreeMap::from([(
        id(2),
        Peer {
            phase: Phase::Handshaking {
                initiator,
                deadline: Instant::now() + Duration::from_secs(5),
            },
            ..Peer::new()
        },
    )]);
    let mut flights = BTreeMap::from([(
        10,
        Flight::Handshake {
            peer: id(2),
            session,
            response: false,
        },
    )]);
    let mut preauth = Queues::new(&config.peers, config.limits.preauth.clone());
    handle_event(
        ProductRelayClientEventV1::PeerHandshake(crate::duplex::RelayPeerHandshakeDeliveryV1 {
            source_peer_id: id(2),
            target_peer_id: id(1),
            received_at_ms: now_ms(),
            handshake: RelayPeerHandshakeV1::Response(response),
        }),
        &id(1),
        &config,
        &key(1),
        &mut peers,
        &worker.shared,
        &mut preauth,
    )
    .unwrap();
    assert!(peers[&id(2)].can_send());
    peers.get_mut(&id(2)).unwrap().frame_sequence = 3;
    reserve(&worker, &mut flights, session, 1, 7);
    settle_forward(
        10,
        outcome(true),
        &mut flights,
        &config,
        &mut peers,
        &worker.shared,
    )
    .unwrap();
    assert!(peers[&id(2)].can_send());
    assert_eq!(peers[&id(2)].frame_sequence, 3);
    assert_eq!(
        flights.len(),
        1,
        "early data retains its independent outcome"
    );
    // A later rejection of that exact data must freeze this generation, not
    // undo the already verified handshake or roll its used sequence back.
    settle_forward(
        101,
        outcome(false),
        &mut flights,
        &config,
        &mut peers,
        &worker.shared,
    )
    .unwrap();
    assert!(!peers[&id(2)].can_send());
    assert_eq!(peers[&id(2)].frame_sequence, 3);
    assert_eq!(worker.status().unwrap().outbound_messages, 1);
}

#[test]
fn late_handshake_rejection_drains_data_without_undoing_accepted_original() {
    let config = config(vec![id(2)]);
    let worker = unstarted(&config);
    let (channel, _) = channels(1, 2);
    let session = channel.session_id();
    let mut peers = BTreeMap::from([(
        id(2),
        Peer {
            phase: Phase::Active(channel),
            frame_sequence: 1,
            ..Peer::new()
        },
    )]);
    let mut flights = BTreeMap::from([(
        10,
        Flight::Handshake {
            peer: id(2),
            session,
            response: false,
        },
    )]);
    reserve(&worker, &mut flights, session, 1, 7);
    settle_forward(
        10,
        outcome(false),
        &mut flights,
        &config,
        &mut peers,
        &worker.shared,
    )
    .unwrap();
    assert!(!peers[&id(2)].can_send());
    assert!(peers[&id(2)].expected_session(session));
    assert_eq!(flights.len(), 1);
    settle_forward(
        101,
        outcome(true),
        &mut flights,
        &config,
        &mut peers,
        &worker.shared,
    )
    .unwrap();
    assert!(flights.is_empty());
    assert_eq!(worker.status().unwrap().outbound_messages, 0);
    assert_eq!(worker.status().unwrap().relay_admissions, 1);
    assert_eq!(peers[&id(2)].frame_sequence, 1);
}
