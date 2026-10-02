use super::tests::{authenticate_to_relay, opaque_envelope};
use super::*;
use crate::product_overlay::{
    HandshakeReplayCacheV1, NodeHandshakeInitiatorV1, NodeHandshakeResponderV1,
};
use ed25519_dalek::SigningKey;
use std::{sync::Mutex, time::Duration};

#[tokio::test]
async fn delivery_encoding_does_not_hold_the_shared_session_lock() {
    let relay = SigningKey::from_bytes(&[230; 32]);
    let source = SigningKey::from_bytes(&[231; 32]);
    let target = SigningKey::from_bytes(&[232; 32]);
    let mut manager = ProductRelaySessionManagerV1::new(Default::default()).unwrap();
    let (source, _source_inbox) = manager
        .register_authenticated_session(authenticate_to_relay(&source, &relay, 1000), 1000)
        .await
        .unwrap();
    let (target, mut inbox) = manager
        .register_authenticated_session(authenticate_to_relay(&target, &relay, 1000), 1000)
        .await
        .unwrap();
    let state = Arc::clone(&manager.state);
    let observed = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&observed);
    manager.encode_hook = Some(Arc::new(move || {
        assert!(
            state.try_write().is_ok(),
            "delivery serialization holds the shared session lock"
        );
        count.fetch_add(1, Ordering::Relaxed);
    }));
    let envelope = opaque_envelope(&source.peer_id, &target.peer_id, 7, 192 * 1024);
    let outcome = manager
        .forward_opaque(&source.peer_id, source.session_id, envelope.clone(), 1010)
        .await;
    assert_eq!(outcome.disposition, RelayForwardDispositionV1::Forwarded);
    assert_eq!(observed.load(Ordering::Relaxed), 1);
    assert_eq!(inbox.try_recv().unwrap().envelope, envelope);
}

#[derive(Clone, Copy, Debug)]
enum Lane {
    Data,
    Offer,
    Response,
}

const LANES: [Lane; 3] = [Lane::Data, Lane::Offer, Lane::Response];

struct Fixture {
    manager: ProductRelaySessionManagerV1,
    relay_key: SigningKey,
    source_key: SigningKey,
    target_key: SigningKey,
    source: RelaySessionRegistrationV1,
    target: RelaySessionRegistrationV1,
    _source_inbox: RelaySessionInboxV1,
    inbox: RelaySessionInboxV1,
}

impl Fixture {
    async fn new(config: ProductRelayRuntimeConfigV1) -> Self {
        let manager = ProductRelaySessionManagerV1::new(config).unwrap();
        let relay_key = SigningKey::from_bytes(&[230; 32]);
        let source_key = SigningKey::from_bytes(&[231; 32]);
        let target_key = SigningKey::from_bytes(&[232; 32]);
        let (source, source_inbox) = manager
            .register_authenticated_session(
                authenticate_to_relay(&source_key, &relay_key, 1000),
                1000,
            )
            .await
            .unwrap();
        let (target, inbox) = manager
            .register_authenticated_session(
                authenticate_to_relay(&target_key, &relay_key, 1000),
                1000,
            )
            .await
            .unwrap();
        Self {
            manager,
            relay_key,
            source_key,
            target_key,
            source,
            target,
            _source_inbox: source_inbox,
            inbox,
        }
    }

    fn input(&self, lane: Lane) -> ProductRelayWireMessageV1 {
        let handshake = match lane {
            Lane::Data => {
                let mut envelope =
                    opaque_envelope(&self.source.peer_id, &self.target.peer_id, 7, 4096);
                // Exercise every JSON byte width, not only the cheapest zero encoding.
                for (index, byte) in envelope.ciphertext.iter_mut().enumerate() {
                    *byte = index as u8;
                }
                return ProductRelayWireMessageV1::Data(envelope);
            }
            Lane::Offer => RelayPeerHandshakeV1::Offer(
                NodeHandshakeInitiatorV1::start(
                    &self.source_key,
                    self.target.peer_id.clone(),
                    1000,
                    5000,
                )
                .unwrap()
                .offer()
                .clone(),
            ),
            Lane::Response => {
                let initiator = NodeHandshakeInitiatorV1::start(
                    &self.target_key,
                    self.source.peer_id.clone(),
                    1000,
                    5000,
                )
                .unwrap();
                RelayPeerHandshakeV1::Response(
                    NodeHandshakeResponderV1::respond(
                        initiator.offer(),
                        &self.source_key,
                        1001,
                        5000,
                        &mut HandshakeReplayCacheV1::default(),
                    )
                    .unwrap()
                    .response()
                    .clone(),
                )
            }
        };
        ProductRelayWireMessageV1::PeerHandshake {
            target_peer_id: self.target.peer_id.clone(),
            handshake,
        }
    }

    async fn admit(
        &self,
        input: &ProductRelayWireMessageV1,
        now_ms: u64,
    ) -> RelayIngressAdmissionV1 {
        self.manager
            .admit_authenticated_wire_v1(
                &self.source.peer_id,
                self.source.session_id,
                serde_json::to_vec(input).unwrap().len(),
                now_ms,
            )
            .await
            .unwrap()
    }
}

fn expected_delivery(
    input: &ProductRelayWireMessageV1,
    source: &str,
    received_at_ms: u64,
) -> Vec<u8> {
    let message = match input.clone() {
        ProductRelayWireMessageV1::Data(envelope) => {
            ProductRelayWireMessageV1::Delivery(OpaqueRelayDeliveryV1 {
                source_peer_id: source.to_string(),
                target_peer_id: envelope.recipient_peer_id.clone(),
                received_at_ms,
                envelope,
            })
        }
        ProductRelayWireMessageV1::PeerHandshake {
            target_peer_id,
            handshake,
        } => ProductRelayWireMessageV1::PeerHandshakeDelivery(RelayPeerHandshakeDeliveryV1 {
            source_peer_id: source.to_string(),
            target_peer_id,
            received_at_ms,
            handshake,
        }),
        _ => unreachable!(),
    };
    serde_json::to_vec(&message).unwrap()
}

async fn dispatch(
    manager: &ProductRelaySessionManagerV1,
    admission: RelayIngressAdmissionV1,
    input: ProductRelayWireMessageV1,
    now_ms: u64,
) -> RelayForwardOutcomeV1 {
    match input {
        ProductRelayWireMessageV1::Data(envelope) => {
            manager
                .forward_opaque_admitted_v1(admission, envelope, now_ms)
                .await
        }
        ProductRelayWireMessageV1::PeerHandshake {
            target_peer_id,
            handshake,
        } => {
            manager
                .forward_peer_handshake_admitted_v1(admission, &target_peer_id, handshake, now_ms)
                .await
        }
        _ => unreachable!(),
    }
}

fn take_encoded(
    inbox: &mut RelaySessionInboxV1,
    lane: Lane,
) -> Result<RelayEncodedDeliveryV1, mpsc::error::TryRecvError> {
    match lane {
        Lane::Data => inbox.try_recv_encoded(),
        Lane::Offer | Lane::Response => inbox.try_recv_peer_handshake_encoded(),
    }
}

#[tokio::test]
async fn both_encoded_lanes_revalidate_source_and_shutdown_without_recharging() {
    #[derive(Clone, Copy, Debug)]
    enum Change {
        Missing,
        Replaced,
        Expired,
        Shutdown,
    }
    for lane in LANES {
        for change in [
            Change::Missing,
            Change::Replaced,
            Change::Expired,
            Change::Shutdown,
        ] {
            let mut fixture = Fixture::new(ProductRelayRuntimeConfigV1 {
                session_ttl_ms: 1000,
                rate_limit_frames: 1,
                max_frames_per_window: 1,
                ..Default::default()
            })
            .await;
            let input = fixture.input(lane);
            let input_bytes = serde_json::to_vec(&input).unwrap().len();
            let admission = fixture.admit(&input, 1010).await;
            let state = Arc::clone(&fixture.manager.state);
            let accepting = Arc::clone(&fixture.manager.accepting);
            let source_id = fixture.source.peer_id.clone();
            let encoded_count = Arc::new(AtomicUsize::new(0));
            let count = Arc::clone(&encoded_count);
            fixture.manager.encode_hook = Some(Arc::new(move || {
                let mut state = state
                    .try_write()
                    .expect("encoding must not own the session lock");
                count.fetch_add(1, Ordering::Relaxed);
                match change {
                    Change::Missing => {
                        state.sessions.remove(&source_id);
                    }
                    Change::Replaced => {
                        state.sessions.get_mut(&source_id).unwrap().session_id[0] ^= 1;
                    }
                    Change::Expired => {
                        state.sessions.get_mut(&source_id).unwrap().last_seen_ms = 0;
                    }
                    Change::Shutdown => accepting.store(false, Ordering::Release),
                }
            }));
            let outcome = dispatch(&fixture.manager, admission, input, 1010).await;
            let expected = match change {
                Change::Missing => RelayForwardDispositionV1::RejectedSourceSessionMissing,
                Change::Replaced => RelayForwardDispositionV1::RejectedStaleSourceSession,
                Change::Expired => RelayForwardDispositionV1::RejectedSourceSessionExpired,
                Change::Shutdown => RelayForwardDispositionV1::RejectedShuttingDown,
            };
            assert_eq!(outcome.disposition, expected, "{lane:?} / {change:?}");
            assert_eq!(outcome.admitted_wire_bytes, input_bytes);
            assert_eq!(encoded_count.load(Ordering::Relaxed), 1);
            assert!(take_encoded(&mut fixture.inbox, lane).is_err());
            let snapshot = fixture.manager.snapshot().await;
            assert_eq!(snapshot.queued_frame_count, 0);
            assert_eq!(snapshot.queued_bytes, 0);
            assert_eq!(snapshot.admitted_wire_bytes_total, input_bytes as u64);
            assert_eq!(snapshot.rejected_wire_bytes_total, input_bytes as u64);
            assert_eq!(snapshot.rejected_frame_total, 1);
            assert_eq!(snapshot.protocol_rejected_frame_total, 0);
            assert_eq!(snapshot.rate_limited_frame_total, 0);
            let state = fixture.manager.state.read().await;
            let source_budget = &state.source_budgets[&fixture.source.peer_id];
            assert_eq!(source_budget.frame_count, 1);
            assert_eq!(source_budget.byte_count, input_bytes as u64);
            assert_eq!(state.aggregate_frame_budget.frame_count, 1);
            assert_eq!(state.aggregate_byte_budget.byte_count, input_bytes as u64);
        }
    }
}

#[tokio::test]
async fn encoded_dispatch_allows_cross_thread_heartbeat_snapshot_and_target_replacement() {
    for lane in LANES {
        let mut fixture = Fixture::new(Default::default()).await;
        let input = fixture.input(lane);
        let expected = expected_delivery(&input, &fixture.source.peer_id, 1010);
        let input_bytes = serde_json::to_vec(&input).unwrap().len();
        let admission = fixture.admit(&input, 1010).await;
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
        let release_rx = Mutex::new(release_rx);
        let mut forwarding_manager = fixture.manager.clone();
        forwarding_manager.encode_hook = Some(Arc::new(move || {
            entered_tx.send(()).unwrap();
            release_rx
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .expect("test must release the encoder even if coordination fails");
        }));
        let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            let result = runtime.block_on(dispatch(&forwarding_manager, admission, input, 1010));
            done_tx.send(result).unwrap();
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let heartbeat_bytes = serde_json::to_vec(&ProductRelayWireMessageV1::Heartbeat)
            .unwrap()
            .len();
        let coordinated = tokio::time::timeout(Duration::from_secs(2), async {
            let snapshot = fixture.manager.snapshot().await;
            let heartbeat = fixture
                .manager
                .heartbeat(&fixture.source.peer_id, fixture.source.session_id, 1011)
                .await;
            let replacement = fixture
                .manager
                .register_authenticated_session(
                    authenticate_to_relay(&fixture.target_key, &fixture.relay_key, 1011),
                    1011,
                )
                .await;
            (snapshot, heartbeat, replacement)
        })
        .await;
        // Release before any assertion: a lock regression must fail, not strand a thread.
        release_tx.send(()).unwrap();
        let outcome = done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        thread.join().unwrap();
        let (during, heartbeat, replacement) =
            coordinated.expect("unrelated authenticated sessions must progress during encoding");
        assert_eq!(during.active_queued_frame_count, 0);
        assert!(heartbeat);
        let (replacement, mut replacement_inbox) = replacement.unwrap();
        assert!(replacement.replaced_existing_session);
        assert_ne!(replacement.session_id, fixture.target.session_id);
        assert_eq!(outcome.disposition, RelayForwardDispositionV1::Forwarded);
        assert!(take_encoded(&mut fixture.inbox, lane).is_err());
        let delivered = take_encoded(&mut replacement_inbox, lane).unwrap();
        assert_eq!(delivered.wire_bytes(), expected);
        let snapshot = fixture.manager.snapshot().await;
        assert_eq!(
            snapshot.admitted_wire_bytes_total,
            (input_bytes + heartbeat_bytes) as u64
        );
        assert_eq!(snapshot.active_queued_frame_count, 1);
        assert_eq!(snapshot.active_queued_bytes, expected.len());
        assert_eq!(snapshot.rejected_frame_total, 0);
        drop(delivered);
        assert_eq!(fixture.manager.snapshot().await.active_queued_bytes, 0);
    }
}

#[tokio::test]
async fn encoded_bytes_match_v1_and_decoded_compatibility_while_charge_matches_output() {
    for lane in LANES {
        let mut fixture = Fixture::new(Default::default()).await;
        let input = fixture.input(lane);
        let input_bytes = serde_json::to_vec(&input).unwrap().len();
        let expected = expected_delivery(&input, &fixture.source.peer_id, 1010);
        assert_ne!(input_bytes, expected.len());
        let admission = fixture.admit(&input, 1010).await;
        let outcome = dispatch(&fixture.manager, admission, input.clone(), 1010).await;
        assert_eq!(outcome.disposition, RelayForwardDispositionV1::Forwarded);
        assert_eq!(outcome.admitted_wire_bytes, input_bytes);
        let guard = take_encoded(&mut fixture.inbox, lane).unwrap();
        assert_eq!(guard.wire_bytes(), expected);
        assert_eq!(guard.0.accounted_bytes, guard.wire_bytes().len());
        let snapshot = fixture.manager.snapshot().await;
        assert_eq!(snapshot.admitted_wire_bytes_total, input_bytes as u64);
        assert_eq!(snapshot.active_queued_frame_count, 1);
        assert_eq!(snapshot.active_queued_bytes, expected.len());
        drop(guard);
        assert_eq!(fixture.manager.snapshot().await.active_queued_bytes, 0);

        let admission = fixture.admit(&input, 1010).await;
        assert_eq!(
            dispatch(&fixture.manager, admission, input, 1010)
                .await
                .disposition,
            RelayForwardDispositionV1::Forwarded
        );
        let decoded = match lane {
            Lane::Data => ProductRelayWireMessageV1::Delivery(fixture.inbox.try_recv().unwrap()),
            Lane::Offer | Lane::Response => ProductRelayWireMessageV1::PeerHandshakeDelivery(
                fixture.inbox.try_recv_peer_handshake().unwrap(),
            ),
        };
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), expected);
        let snapshot = fixture.manager.snapshot().await;
        assert_eq!(snapshot.active_queued_frame_count, 0);
        assert_eq!(snapshot.active_queued_bytes, 0);
        assert_eq!(snapshot.admitted_wire_bytes_total, 2 * input_bytes as u64);
    }
}

#[tokio::test]
async fn encoded_offline_routes_keep_original_timestamp_and_exact_ttl_boundary() {
    for lane in LANES {
        for reconnect_at in [1060, 1061] {
            let mut fixture = Fixture::new(ProductRelayRuntimeConfigV1 {
                offline_queue_ttl_ms: 50,
                ..Default::default()
            })
            .await;
            assert!(
                fixture
                    .manager
                    .disconnect(&fixture.target.peer_id, fixture.target.session_id)
                    .await
            );
            let input = fixture.input(lane);
            let expected = expected_delivery(&input, &fixture.source.peer_id, 1010);
            let admission = fixture.admit(&input, 1010).await;
            // Make dispatch time strictly later; neither wire time nor offline TTL may renew.
            fixture.manager.encode_hook = Some(Arc::new(|| {
                std::thread::sleep(Duration::from_millis(2));
            }));
            let outcome = dispatch(&fixture.manager, admission, input, 1010).await;
            assert_eq!(
                outcome.disposition,
                RelayForwardDispositionV1::QueuedTargetOffline
            );
            let offline_bytes = offline_memory_accounted_bytes_v1(
                expected.len(),
                fixture.source.peer_id.len(),
                fixture.target.peer_id.len(),
            );
            {
                let state = fixture.manager.state.read().await;
                let item = &state.offline_queues[&fixture.target.peer_id][0];
                assert_eq!(item.source_peer_id, fixture.source.peer_id);
                assert_eq!(item.target_peer_id, fixture.target.peer_id);
                assert_eq!(item.received_at_ms, 1010);
                assert_eq!(item.active_accounted_bytes, expected.len());
                assert_eq!(item.offline_accounted_bytes, offline_bytes);
                let bytes = match &item.message {
                    RelayOfflineMessageV1::Data(encoded)
                    | RelayOfflineMessageV1::Control(encoded) => encoded.wire_bytes(),
                };
                assert_eq!(bytes, expected);
            }
            let queued = fixture.manager.snapshot().await;
            assert_eq!(queued.offline_queued_frame_count, 1);
            assert_eq!(queued.offline_queued_bytes, offline_bytes);
            let (replacement, mut inbox) = fixture
                .manager
                .register_authenticated_session(
                    authenticate_to_relay(&fixture.target_key, &fixture.relay_key, reconnect_at),
                    reconnect_at,
                )
                .await
                .unwrap();
            if reconnect_at == 1060 {
                assert_eq!(replacement.queued_frames_drained, 1);
                let guard = take_encoded(&mut inbox, lane).unwrap();
                assert_eq!(guard.wire_bytes(), expected);
                assert_eq!(
                    fixture.manager.snapshot().await.active_queued_bytes,
                    expected.len()
                );
                drop(guard);
            } else {
                assert_eq!(replacement.queued_frames_drained, 0);
                assert!(take_encoded(&mut inbox, lane).is_err());
                let expired = fixture.manager.snapshot().await;
                assert_eq!(expired.expired_queued_frame_total, 1);
                assert_eq!(expired.expired_queued_bytes_total, offline_bytes as u64);
            }
            let snapshot = fixture.manager.snapshot().await;
            assert_eq!(snapshot.queued_frame_count, 0);
            assert_eq!(snapshot.queued_bytes, 0);
        }
    }
}

#[tokio::test]
async fn encoded_dispatch_delay_cannot_freeze_source_expiry_at_ingress() {
    for lane in LANES {
        let mut fixture = Fixture::new(ProductRelayRuntimeConfigV1 {
            session_ttl_ms: 10,
            ..Default::default()
        })
        .await;
        let input = fixture.input(lane);
        let admission = fixture.admit(&input, 1000).await;
        fixture.manager.encode_hook = Some(Arc::new(|| {
            std::thread::sleep(Duration::from_millis(30));
        }));
        let outcome = dispatch(&fixture.manager, admission, input, 1000).await;
        assert_eq!(
            outcome.disposition,
            RelayForwardDispositionV1::RejectedSourceSessionExpired
        );
        let snapshot = fixture.manager.snapshot().await;
        assert_eq!(snapshot.expired_session_total, 1);
        assert_eq!(snapshot.rejected_frame_total, 1);
        assert_eq!(snapshot.queued_frame_count, 0);
        assert!(take_encoded(&mut fixture.inbox, lane).is_err());
    }
}

#[tokio::test]
async fn lookahead_and_encoded_send_guards_hold_shared_quota_across_replacement_and_shutdown() {
    struct NoopWake;
    impl std::task::Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
    let mut fixture = Fixture::new(ProductRelayRuntimeConfigV1 {
        session_queue_capacity: 2,
        active_queue_total: 2,
        ..Default::default()
    })
    .await;
    let mut expected_bytes = 0;
    for lane in [Lane::Data, Lane::Offer] {
        let input = fixture.input(lane);
        expected_bytes += expected_delivery(&input, &fixture.source.peer_id, 1010).len();
        let admission = fixture.admit(&input, 1010).await;
        assert_eq!(
            dispatch(&fixture.manager, admission, input, 1010)
                .await
                .disposition,
            RelayForwardDispositionV1::Forwarded
        );
    }
    fixture
        .inbox
        .arm_delivery_wake(&Waker::from(Arc::new(NoopWake)));
    assert!(fixture.inbox.ready_data.is_some());
    assert!(fixture.inbox.ready_control.is_some());
    assert_eq!(
        fixture.manager.snapshot().await.active_queued_bytes,
        expected_bytes
    );
    let data_guard = take_encoded(&mut fixture.inbox, Lane::Data).unwrap();
    let control_guard = take_encoded(&mut fixture.inbox, Lane::Offer).unwrap();
    assert_eq!(
        fixture.manager.snapshot().await.active_queued_frame_count,
        2
    );
    let (replacement, mut inbox) = fixture
        .manager
        .register_authenticated_session(
            authenticate_to_relay(&fixture.target_key, &fixture.relay_key, 1011),
            1011,
        )
        .await
        .unwrap();
    assert!(replacement.replaced_existing_session);
    let input = fixture.input(Lane::Response);
    let expected_response = expected_delivery(&input, &fixture.source.peer_id, 1012);
    let admission = fixture.admit(&input, 1012).await;
    assert_eq!(
        dispatch(&fixture.manager, admission, input, 1012)
            .await
            .disposition,
        RelayForwardDispositionV1::QueuedBackpressure
    );
    let full = fixture.manager.snapshot().await;
    assert_eq!(full.active_queued_frame_count, 2);
    assert_eq!(full.active_queued_bytes, expected_bytes);
    assert_eq!(full.offline_queued_frame_count, 1);
    assert!(take_encoded(&mut inbox, Lane::Response).is_err());

    drop(data_guard);
    assert_eq!(
        fixture.manager.snapshot().await.active_queued_frame_count,
        1
    );
    assert_eq!(
        fixture
            .manager
            .drain_queued_for_session(&replacement.peer_id, replacement.session_id, 1013)
            .await,
        1
    );
    let response_guard = take_encoded(&mut inbox, Lane::Response).unwrap();
    assert_eq!(response_guard.wire_bytes(), expected_response);
    let held_bytes = control_guard.wire_bytes().len() + response_guard.wire_bytes().len();
    let stopped = fixture.manager.finish_graceful_shutdown().await;
    assert_eq!(stopped.active_queued_frame_count, 2);
    assert_eq!(stopped.active_queued_bytes, held_bytes);
    drop(fixture.inbox);
    drop(inbox);
    assert_eq!(
        fixture.manager.snapshot().await.active_queued_bytes,
        held_bytes
    );
    drop(control_guard);
    assert_eq!(
        fixture.manager.snapshot().await.active_queued_frame_count,
        1
    );
    drop(response_guard);
    let empty = fixture.manager.snapshot().await;
    assert_eq!(empty.queued_frame_count, 0);
    assert_eq!(empty.queued_bytes, 0);
}
