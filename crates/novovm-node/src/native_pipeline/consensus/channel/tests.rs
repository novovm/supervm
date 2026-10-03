//! Channel-owner tests. Transport and preparation are not consensus finality.

use super::*;
use crate::native_pipeline::business::nov_transfer_batch::{
    effect_contract, program_id, receipt_codec, SEMANTIC_VERSION,
};
use crate::native_pipeline::consensus::wire::{self, Context as WireContext, Quorum, Validator};
use crate::native_pipeline::state::tree::empty_root;
use ed25519_dalek::SigningKey;
use novovm_network::duplex::fragments::CompletedMessage;
use novovm_network::duplex::peer_id_from_ed25519_public_key_v1;
use novovm_network::duplex::product_relay_client::{
    ProductRelayClientConfigV1, ProductRelayTlsTrustV1,
};
use novovm_network::duplex::product_relay_daemon::{
    run_product_relay_daemon_with_shutdown_v1, ProductRelayDaemonConfigV1,
    ProductRelayDaemonReportV1,
};
use novovm_network::duplex::worker::{NetworkWorkerConfig, WorkerLimits};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn key(index: u8) -> SigningKey {
    SigningKey::from_bytes(&[101 + index; 32])
}
fn peer(index: u8) -> String {
    peer_id_from_ed25519_public_key_v1(&key(index).verifying_key().to_bytes())
}

fn policy() -> DirectNovFeePolicy {
    DirectNovFeePolicy {
        quote_ttl_ms: 15_000,
        policy_version: 1,
        policy_source: "default".into(),
        resolution_source: "runtime_state".into(),
        reserve_share_bps: 7000,
        fee_share_bps: 2000,
        risk_buffer_share_bps: 1000,
        min_reserve_bucket_nov: 0,
        min_fee_bucket_nov: 0,
        min_risk_buffer_nov: 1,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: 1_000_000,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 500,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    }
}

fn config(local: u8) -> ChannelConfig {
    let validators = Arc::new(
        ValidatorSet::new(
            292,
            1,
            1,
            (0..4)
                .map(|index| Validator::new(key(index).verifying_key().to_bytes(), 1))
                .collect::<Result<_>>()
                .unwrap(),
        )
        .unwrap(),
    );
    let codec = DecodeLimits {
        transactions: 8,
        transaction_bytes: 256 * 1024,
        body_bytes: 512 * 1024,
        message_bytes: 768 * 1024,
    };
    let charge = codec.message_bytes * 4 + 4096;
    let lanes = LaneBudget {
        control: QueueBudget {
            messages: 8,
            bytes: charge * 8,
        },
        body: QueueBudget {
            messages: 8,
            bytes: charge * 8,
        },
    };
    ChannelConfig {
        chain_id: 292,
        genesis: [0x71; 32],
        protocol: [0x72; 32],
        peers: (0..3).filter(|index| *index != local).map(peer).collect(),
        validators,
        policy: policy(),
        codec,
        reassembly: ReassemblyLimits {
            max_message_bytes: codec.message_bytes,
            messages: 16,
            bytes: 4 * 1024 * 1024,
            peer_messages: 8,
            peer_bytes: 2 * 1024 * 1024,
            ttl: Duration::from_secs(20),
        },
        prepare: lanes,
        send: lanes,
        receive: lanes,
        ttl: Duration::from_secs(20),
    }
}

fn body(config: &ChannelConfig, big: bool) -> Arc<Message> {
    Arc::new(Message::Body {
        context: BatchContext {
            chain_id: config.chain_id,
            genesis_config_commitment: config.genesis,
            protocol_commitment: config.protocol,
            business_program: program_id(),
            semantic_version: SEMANTIC_VERSION,
            effect_contract: effect_contract(&config.policy).unwrap(),
            parent_block_hash: [0; 32],
            parent_height: 0,
            parent_state_root: empty_root(),
            parent_receipt_root: empty_root(),
            parent_state_version: 0,
            receipt_codec: receipt_codec(),
            height: 1,
            slot: 0,
            timestamp_unix_ms: 1,
        },
        // These deliberately unverified bytes test preparation only. The AOEM
        // authentication/business pipeline, not the channel, must approve them.
        raw_transactions: if big {
            vec![vec![0x51; 200 * 1024], vec![0x52; 100 * 1024]]
        } else {
            vec![vec![1, 2, 3]]
        },
    })
}

fn request(token: u64, message: Arc<Message>) -> PrepareRequest {
    PrepareRequest {
        token,
        input: PrepareInput::New(message),
    }
}
fn control() -> Arc<Message> {
    Arc::new(Message::RequestBody {
        body_id: [0x41; 32],
    })
}

fn unstarted(config: &ChannelConfig) -> HostChannel {
    let (_, charge) = validate(config).unwrap();
    HostChannel {
        shared: Arc::new(Mutex::new(Shared::default())),
        stop: Arc::new(AtomicBool::new(false)),
        scope: Arc::new(()),
        peers: config.peers.iter().cloned().collect(),
        prepare_budget: config.prepare,
        send_budget: config.send,
        charge,
        ttl: config.ttl,
        worker: None,
    }
}

fn prepared(channel: &HostChannel, config: &ChannelConfig, message: Arc<Message>) -> Ready {
    let encoded = transport::encode(&message, config.codec).unwrap();
    prepare(message, encoded, config, &channel.scope, channel.charge).unwrap()
}

fn decoded(config: &ChannelConfig, from: u8, message: &Message) -> CompletedMessage {
    CompletedMessage {
        peer: peer(from),
        id: [1; 32],
        chunks: vec![transport::encode(message, config.codec).unwrap()],
    }
}

fn wire_context(config: &ChannelConfig) -> WireContext {
    WireContext {
        chain_id: config.chain_id,
        genesis_config_commitment: config.genesis,
        protocol_commitment: config.protocol,
        epoch: 1,
        validator_set_hash: config.validators.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    }
}

fn signed_proposal(config: &ChannelConfig) -> wire::Proposal {
    let leader = config.validators.leader(1, 0).unwrap();
    let index = (0..4)
        .find(|index| {
            Validator::new(key(*index).verifying_key().to_bytes(), 1)
                .unwrap()
                .id()
                == leader
        })
        .unwrap();
    novovm_consensus::round_bft::test_vectors::sign_proposal(
        wire_context(config),
        0,
        [0x21; 32],
        None,
        &config.validators,
        &key(index),
    )
    .unwrap()
}

fn signed_qc(config: &ChannelConfig, phase: Phase) -> Quorum {
    Quorum::from_votes(
        &config.validators,
        (0..3)
            .map(|index| {
                novovm_consensus::round_bft::test_vectors::sign_vote(
                    wire_context(config),
                    0,
                    phase,
                    Some([0x21; 32]),
                    &config.validators,
                    &key(index),
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap()
}

fn network_config(local: u8) -> NetworkWorkerConfig {
    NetworkWorkerConfig {
        chain_id: 292,
        relay: ProductRelayClientConfigV1 {
            endpoint: "wss://127.0.0.1:9/novovm".into(),
            expected_relay_peer_id: peer(8),
            connect_timeout_ms: 100,
            read_timeout_ms: 10,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
        },
        peers: config(local).peers,
        limits: WorkerLimits::default(),
        handshake_timeout_ms: 5000,
        reconnect_delay_ms: 50,
        heartbeat_interval_ms: 1000,
        queue_ttl_ms: 20_000,
    }
}

#[test]
fn new_input_admission_is_constant_work_and_returns_original_on_contention() {
    let config = config(0);
    let channel = unstarted(&config);
    let input = body(&config, true);
    let lock = channel.shared.lock().unwrap();
    let PrepareAdmission::Backpressure(returned) =
        channel.try_prepare(request(1, input.clone())).unwrap()
    else {
        panic!("lock contention did not return input")
    };
    let PrepareInput::New(returned) = returned.input else {
        panic!("changed input type")
    };
    assert!(Arc::ptr_eq(&returned, &input));
    assert_eq!(lock.status.encoded_messages, 0);
    drop(lock);
    assert!(matches!(
        channel.try_prepare(request(1, input.clone())).unwrap(),
        PrepareAdmission::Accepted
    ));
    let shared = channel.shared.lock().unwrap();
    assert_eq!(shared.status.encoded_messages, 0);
    assert_eq!(shared.prepare_usage[1].bytes, channel.charge);
    assert_eq!(shared.prepare[1].len(), 1);
    let PrepareInput::New(retained) = &shared.prepare[1][0].value.input else {
        panic!("changed input type")
    };
    assert!(Arc::ptr_eq(retained, &input));
}

#[test]
fn local_count_and_byte_reservations_cover_queued_processing_and_unclaimed_reply() {
    let mut config = config(0);
    let charge = validate(&config).unwrap().1;
    config.prepare.control = QueueBudget {
        messages: 2,
        bytes: charge,
    };
    let channel = unstarted(&config);
    assert!(matches!(
        channel.try_prepare(request(1, control())).unwrap(),
        PrepareAdmission::Accepted
    ));
    let PrepareAdmission::Backpressure(returned) =
        channel.try_prepare(request(2, control())).unwrap()
    else {
        panic!("byte quota bypassed")
    };
    assert_eq!(returned.token, 2);
    prepare_one(&config, &channel.scope, charge, &channel.shared, 0).unwrap();
    assert!(matches!(
        channel.try_prepare(returned).unwrap(),
        PrepareAdmission::Backpressure(_)
    ));
    assert_eq!(channel.status().unwrap().pending_prepare, 1);
    assert!(matches!(
        channel.try_recv().unwrap(),
        Some(ChannelEvent::Prepared {
            token: 1,
            result: Ok(_)
        })
    ));
    assert!(matches!(
        channel.try_prepare(request(3, control())).unwrap(),
        PrepareAdmission::Accepted
    ));
    let mut config = config.clone();
    config.prepare.control.messages = 1;
    config.prepare.control.bytes = charge * 2;
    let channel = unstarted(&config);
    assert!(matches!(
        channel.try_prepare(request(1, control())).unwrap(),
        PrepareAdmission::Accepted
    ));
    assert!(matches!(
        channel.try_prepare(request(2, control())).unwrap(),
        PrepareAdmission::Backpressure(_)
    ));
}

#[test]
fn control_body_local_remote_and_outbound_use_independent_quotas() {
    let mut config = config(0);
    let charge = validate(&config).unwrap().1;
    let one = QueueBudget {
        messages: 1,
        bytes: charge,
    };
    config.prepare = LaneBudget {
        control: one,
        body: one,
    };
    config.receive = config.prepare;
    config.send = config.prepare;
    config.peers = vec![peer(1)];
    let channel = unstarted(&config);
    assert!(matches!(
        channel
            .try_prepare(request(1, body(&config, true)))
            .unwrap(),
        PrepareAdmission::Accepted
    ));
    assert!(matches!(
        channel
            .try_prepare(request(2, body(&config, false)))
            .unwrap(),
        PrepareAdmission::Backpressure(_)
    ));
    assert!(matches!(
        channel.try_prepare(request(3, control())).unwrap(),
        PrepareAdmission::Accepted
    ));
    receive_one(
        decoded(&config, 1, &control()),
        &config,
        &channel.scope,
        charge,
        &channel.shared,
    )
    .unwrap();
    receive_one(
        decoded(&config, 1, &control()),
        &config,
        &channel.scope,
        charge,
        &channel.shared,
    )
    .unwrap();
    assert_eq!(channel.status().unwrap().dropped_received, 1);
    prepare_one(&config, &channel.scope, charge, &channel.shared, 0).unwrap();
    prepare_one(&config, &channel.scope, charge, &channel.shared, 1).unwrap();
    let mut local = 0;
    let mut remote = 0;
    for _ in 0..3 {
        match channel.try_recv().unwrap().unwrap() {
            ChannelEvent::Prepared { result: Ok(_), .. } => local += 1,
            ChannelEvent::Received(_) => remote += 1,
            _ => panic!("preparation failed"),
        }
    }
    assert_eq!((local, remote), (2, 1));
    let prepared = prepared(&channel, &config, control()).prepared;
    assert!(matches!(
        channel
            .try_send(Outbound {
                peer: peer(1),
                message: prepared
            })
            .unwrap(),
        SendAdmission::Accepted
    ));
    assert_eq!(channel.status().unwrap().pending_send, 1);
    assert_eq!(channel.status().unwrap().pending_prepare, 0);
    assert_eq!(channel.status().unwrap().pending_receive, 0);
}

#[test]
fn cached_preparation_builds_new_owned_request_without_rehash_or_shared_request() {
    let config = config(0);
    let channel = unstarted(&config);
    assert!(matches!(
        channel
            .try_prepare(request(1, body(&config, true)))
            .unwrap(),
        PrepareAdmission::Accepted
    ));
    prepare_one(&config, &channel.scope, channel.charge, &channel.shared, 1).unwrap();
    let Some(ChannelEvent::Prepared {
        result: Ok(mut first),
        ..
    }) = channel.try_recv().unwrap()
    else {
        panic!("no body reply")
    };
    let mut body = first.body.take().unwrap();
    assert_eq!(body.id(), first.prepared.body_id().unwrap());
    assert!(Arc::ptr_eq(body.message(), &first.message));
    assert!(body.take_request().is_some());
    assert!(body.take_request().is_none());
    let encoded = first.prepared.0.clone();
    assert!(matches!(
        channel
            .try_prepare(PrepareRequest {
                token: 2,
                input: PrepareInput::Cached(first.prepared)
            })
            .unwrap(),
        PrepareAdmission::Accepted
    ));
    prepare_one(&config, &channel.scope, channel.charge, &channel.shared, 1).unwrap();
    let Some(ChannelEvent::Prepared {
        result: Ok(mut second),
        ..
    }) = channel.try_recv().unwrap()
    else {
        panic!("no retry body")
    };
    assert!(Arc::ptr_eq(&encoded, &second.prepared.0));
    assert!(second.body.as_mut().unwrap().take_request().is_some());
    assert_eq!(channel.status().unwrap().encoded_messages, 1);
    assert_eq!(channel.status().unwrap().prepared_bodies, 2);
}

#[test]
fn owner_verifies_typed_evidence_and_rejects_wrong_domain_or_certificate() {
    let config = config(0);
    let channel = unstarted(&config);
    let proposal = signed_proposal(&config);
    let valid = Message::Decision {
        proposal: proposal.clone(),
        certificate: signed_qc(&config, Phase::Precommit),
        body_id: [0x42; 32],
    };
    let ready = prepared(&channel, &config, Arc::new(valid.clone()));
    assert!(
        matches!(ready.evidence.as_ref(), VerifiedEvidence::Decision { certificate, .. } if certificate.signed_weight() == 3)
    );
    let mut wrong = valid.clone();
    if let Message::Decision { certificate, .. } = &mut wrong {
        certificate.votes[0].signature[0] ^= 1;
    }
    let encoded = transport::encode(&wrong, config.codec).unwrap();
    assert!(prepare(
        Arc::new(wrong),
        encoded,
        &config,
        &channel.scope,
        channel.charge
    )
    .is_err());
    let wrong = Message::Decision {
        proposal,
        certificate: signed_qc(&config, Phase::Prevote),
        body_id: [0x42; 32],
    };
    assert!(evidence(&wrong, &config).is_err());
    let mut context = wire_context(&config);
    context.genesis_config_commitment[0] ^= 1;
    assert!(evidence(&Message::RequestDecision { context }, &config).is_err());
    assert!(matches!(
        evidence(
            &Message::RequestDecision {
                context: wire_context(&config)
            },
            &config
        )
        .unwrap(),
        VerifiedEvidence::None
    ));
    let vote = novovm_consensus::round_bft::test_vectors::sign_vote(
        wire_context(&config),
        0,
        Phase::Prevote,
        Some([0x21; 32]),
        &config.validators,
        &key(1),
    )
    .unwrap();
    assert!(matches!(
        evidence(&Message::Vote(vote), &config).unwrap(),
        VerifiedEvidence::Vote(_)
    ));
}

#[test]
fn malformed_peer_input_releases_reservation_without_killing_local_work() {
    let config = config(0);
    let channel = unstarted(&config);
    let bad = CompletedMessage {
        peer: peer(1),
        id: [2; 32],
        chunks: vec![b"not a host message".to_vec()],
    };
    receive_one(
        bad,
        &config,
        &channel.scope,
        channel.charge,
        &channel.shared,
    )
    .unwrap();
    let mut proposal = signed_proposal(&config);
    proposal.signature[0] ^= 1;
    receive_one(
        decoded(
            &config,
            1,
            &Message::Proposal {
                proposal,
                valid_quorum: None,
                body_id: [3; 32],
            },
        ),
        &config,
        &channel.scope,
        channel.charge,
        &channel.shared,
    )
    .unwrap();
    assert_eq!(channel.status().unwrap().invalid_received, 2);
    assert_eq!(channel.status().unwrap().pending_receive, 0);
    assert!(matches!(
        channel.try_prepare(request(1, control())).unwrap(),
        PrepareAdmission::Accepted
    ));
    prepare_one(&config, &channel.scope, channel.charge, &channel.shared, 0).unwrap();
    assert!(matches!(
        channel.try_recv().unwrap(),
        Some(ChannelEvent::Prepared { result: Ok(_), .. })
    ));
}

#[test]
fn foreign_handles_unknown_peers_stop_and_contention_preserve_owned_payload() {
    let config = config(0);
    let channel = unstarted(&config);
    let other = unstarted(&config);
    let handle = prepared(&channel, &config, control()).prepared;
    let SendAdmission::Rejected { outbound, .. } = other
        .try_send(Outbound {
            peer: peer(1),
            message: handle.clone(),
        })
        .unwrap()
    else {
        panic!("foreign handle accepted")
    };
    assert!(Arc::ptr_eq(&handle.0, &outbound.message.0));
    assert!(matches!(
        other
            .try_prepare(PrepareRequest {
                token: 1,
                input: PrepareInput::Cached(handle.clone())
            })
            .unwrap(),
        PrepareAdmission::Rejected { .. }
    ));
    assert!(matches!(
        channel
            .try_send(Outbound {
                peer: "unknown".into(),
                message: handle.clone()
            })
            .unwrap(),
        SendAdmission::Rejected { .. }
    ));
    let lock = channel.shared.lock().unwrap();
    let SendAdmission::Backpressure(outbound) = channel
        .try_send(Outbound {
            peer: peer(1),
            message: handle.clone(),
        })
        .unwrap()
    else {
        panic!("contention lost payload")
    };
    assert!(Arc::ptr_eq(&outbound.message.0, &handle.0));
    drop(lock);
    channel.stop.store(true, Ordering::Release);
    assert!(matches!(
        channel.try_send(outbound).unwrap(),
        SendAdmission::Rejected { .. }
    ));
    assert!(matches!(
        channel.try_prepare(request(1, control())).unwrap(),
        PrepareAdmission::Rejected { .. }
    ));
}

#[test]
fn blocked_large_body_does_not_prevent_small_control_send_and_ttl_releases_bytes() {
    let config = config(0);
    let channel = unstarted(&config);
    let mut net_config = network_config(0);
    net_config.limits.outbound.max_bytes = 256;
    net_config.limits.outbound.peer_max_bytes = 256;
    let network = NetworkWorker::start(net_config, key(0)).unwrap();
    let large = prepared(&channel, &config, body(&config, true)).prepared;
    let small = prepared(&channel, &config, control()).prepared;
    assert!(large.0.encoded.frame_count() > 1);
    assert!(matches!(
        channel
            .try_send(Outbound {
                peer: peer(1),
                message: large.clone()
            })
            .unwrap(),
        SendAdmission::Accepted
    ));
    assert!(matches!(
        channel
            .try_send(Outbound {
                peer: peer(1),
                message: small.clone()
            })
            .unwrap(),
        SendAdmission::Accepted
    ));
    assert!(!send_one(&network, &config, &channel.shared, 1).unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    while channel.status().unwrap().sent_messages == 0 {
        assert!(Instant::now() < deadline);
        send_one(&network, &config, &channel.shared, 0).unwrap();
        thread::yield_now();
    }
    assert_eq!(channel.status().unwrap().pending_send, 1);
    {
        let mut shared = channel.shared.lock().unwrap();
        assert!(Arc::ptr_eq(
            &shared.send[1][0].value.outbound.message.0,
            &large.0
        ));
        shared.send[1][0].created = Instant::now() - config.ttl;
    }
    send_one(&network, &config, &channel.shared, 1).unwrap();
    assert_eq!(channel.status().unwrap().expired_sends, 1);
    assert_eq!(channel.status().unwrap().pending_send, 0);
    assert_eq!(channel.shared.lock().unwrap().send_usage[1].bytes, 0);
    let shared = channel.shared.lock().unwrap();
    assert_eq!(shared.send_peer_usage[1][&peer(1)].messages, 0);
    assert_eq!(shared.send_peer_usage[1][&peer(1)].bytes, 0);
}

#[test]
fn resident_owner_prepares_and_retries_body_while_relay_is_offline() {
    let config = config(0);
    let network = NetworkWorker::start(network_config(0), key(0)).unwrap();
    let mut channel = HostChannel::start(network, config.clone()).unwrap();
    let first = await_prepare(&channel, request(1, body(&config, true)));
    let id = first.body.as_ref().unwrap().id();
    let second = await_prepare(
        &channel,
        PrepareRequest {
            token: 2,
            input: PrepareInput::Cached(first.prepared.clone()),
        },
    );
    assert_eq!(second.body.as_ref().unwrap().id(), id);
    assert!(Arc::ptr_eq(&first.prepared.0, &second.prepared.0));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(status) = channel.status() {
            assert_eq!(status.encoded_messages, 1);
            assert_eq!(status.prepared_bodies, 2);
            break;
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    channel.shutdown().unwrap();
}

fn await_prepare(channel: &HostChannel, mut request: PrepareRequest) -> Ready {
    let deadline = Instant::now() + Duration::from_secs(15);
    let token = request.token;
    loop {
        match channel.try_prepare(request).unwrap() {
            PrepareAdmission::Accepted => break,
            PrepareAdmission::Backpressure(returned) => request = returned,
            PrepareAdmission::Rejected { reason, .. } => panic!("prepare rejected: {reason}"),
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    loop {
        if let Some(ChannelEvent::Prepared {
            token: actual,
            result,
        }) = channel.try_recv().unwrap()
        {
            assert_eq!(actual, token);
            return result.unwrap();
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
}

struct Relay {
    endpoint: String,
    cert: PathBuf,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<()>>>,
}

impl Relay {
    fn start() -> Self {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        // A fresh checkout with an external CARGO_TARGET_DIR has no local
        // artifact directory yet. Do not depend on another test creating it.
        let base = repository.join("target/runtime-rebuild");
        fs::create_dir_all(&base).unwrap();
        let base = base.canonicalize().unwrap();
        assert!(base.starts_with(repository));
        let root = base.join(format!(
            "channel-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into(), "127.0.0.1".into()])
            .unwrap();
        let cert_path = root.join("cert.pem");
        let key_path = root.join("key.pem");
        let identity = root.join("identity.hex");
        let report = root.join("report.json");
        fs::write(&cert_path, cert.serialize_pem().unwrap()).unwrap();
        fs::write(&key_path, cert.serialize_private_key_pem()).unwrap();
        fs::write(&identity, format!("{:02x}", 109).repeat(32)).unwrap();
        let config: ProductRelayDaemonConfigV1 = serde_json::from_value(serde_json::json!({
            "bind_addr": "127.0.0.1:0", "tls_cert_path": cert_path, "tls_key_path": key_path,
            "relay_identity_key_path": identity, "report_path": report, "report_interval_ms": 10,
            "max_connections": 8, "max_sessions": 4,
        }))
        .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker =
            thread::spawn(move || run_product_relay_daemon_with_shutdown_v1(config, worker_stop));
        let mut relay = Self {
            endpoint: String::new(),
            cert: cert_path,
            stop,
            worker: Some(worker),
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(report) = fs::read(&report)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<ProductRelayDaemonReportV1>(&bytes).ok())
            {
                let address: std::net::SocketAddr = report.listen_addr.parse().unwrap();
                relay.endpoint = format!("wss://127.0.0.1:{}/novovm", address.port());
                return relay;
            }
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
    }
    fn network(&self, local: u8) -> NetworkWorker {
        let mut config = network_config(local);
        config.relay.endpoint = self.endpoint.clone();
        config.relay.connect_timeout_ms = 2000;
        config.relay.tls_trust = ProductRelayTlsTrustV1::ExplicitCa {
            certificate_path: self.cert.clone(),
        };
        NetworkWorker::start(config, key(local)).unwrap()
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn admit_send(channel: &HostChannel, mut outbound: Outbound) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match channel.try_send(outbound).unwrap() {
            SendAdmission::Accepted => return,
            SendAdmission::Backpressure(returned) => outbound = returned,
            SendAdmission::Rejected { reason, .. } => panic!("send rejected: {reason}"),
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
}

#[test]
fn real_wss_owner_fragments_large_body_receives_typed_vote_and_survives_bad_peer() {
    let relay = Relay::start();
    let mut first = HostChannel::start(relay.network(0), config(0)).unwrap();
    let mut second = HostChannel::start(relay.network(1), config(1)).unwrap();
    let mut raw_peer = relay.network(2);
    let body = await_prepare(&first, request(1, body(&config(0), true)));
    assert!(body.prepared.0.encoded.frame_count() > 1);
    let vote = novovm_consensus::round_bft::test_vectors::sign_vote(
        wire_context(&config(0)),
        0,
        Phase::Prevote,
        Some([0x21; 32]),
        &config(0).validators,
        &key(0),
    )
    .unwrap();
    let vote = await_prepare(&first, request(2, Arc::new(Message::Vote(vote))));
    admit_send(
        &first,
        Outbound {
            peer: peer(1),
            message: body.prepared.clone(),
        },
    );
    admit_send(
        &first,
        Outbound {
            peer: peer(1),
            message: vote.prepared,
        },
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut got_body = false;
    let mut got_vote = false;
    while !got_body || !got_vote {
        if let Some(ChannelEvent::Received(mut received)) = second.try_recv().unwrap() {
            assert_eq!(received.peer, peer(0));
            match received.ready.message.as_ref() {
                Message::Body { .. } => {
                    assert_eq!(
                        received.ready.body.as_ref().unwrap().id(),
                        body.body.as_ref().unwrap().id()
                    );
                    assert!(received
                        .ready
                        .body
                        .as_mut()
                        .unwrap()
                        .take_request()
                        .is_some());
                    got_body = true;
                }
                Message::Vote(_) => {
                    assert!(matches!(
                        received.ready.evidence.as_ref(),
                        VerifiedEvidence::Vote(_)
                    ));
                    got_vote = true;
                }
                _ => panic!("unexpected host message"),
            }
        }
        assert!(Instant::now() < deadline, "real owner receive timed out");
        thread::yield_now();
    }
    // A configured authenticated peer can still send invalid Host payloads.
    let malformed = OutgoingMessage::new(
        transport::fragment_domain(292, [0x71; 32], [0x72; 32]),
        b"invalid host bytes".to_vec(),
        1024,
    )
    .unwrap();
    let mut bytes = malformed.frame(0).unwrap();
    loop {
        match raw_peer.try_send(peer(1), bytes).unwrap() {
            NetworkAdmission::Accepted => break,
            NetworkAdmission::Backpressure(returned) => bytes = returned.bytes,
            NetworkAdmission::Rejected { .. } => panic!("raw peer send rejected"),
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    loop {
        if second
            .status()
            .is_ok_and(|status| status.invalid_received > 0)
        {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    let encoded_before = current_status(&first).encoded_messages;
    admit_send(
        &first,
        Outbound {
            peer: peer(1),
            message: body.prepared.clone(),
        },
    );
    loop {
        if let Some(ChannelEvent::Received(received)) = second.try_recv().unwrap() {
            assert_eq!(received.ready.prepared.body_id(), body.prepared.body_id());
            break;
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    assert_eq!(current_status(&first).encoded_messages, encoded_before);
    first.shutdown().unwrap();
    second.shutdown().unwrap();
    raw_peer.shutdown().unwrap();
}

#[test]
fn invalid_ceiling_domain_and_lane_budget_fail_before_owner_start() {
    let mut config = config(0);
    config.reassembly.max_message_bytes -= 1;
    assert!(validate(&config).is_err());
    config.reassembly.max_message_bytes += 1;
    config.prepare.control.bytes = 1;
    assert!(validate(&config).is_err());
    config.prepare.control.bytes = usize::MAX;
    config.genesis = [0; 32];
    assert!(validate(&config).is_err());
}

#[test]
fn expired_remote_body_is_reclaimed_on_owner_not_coordinator() {
    let config = config(0);
    let channel = unstarted(&config);
    receive_one(
        decoded(&config, 1, &body(&config, true)),
        &config,
        &channel.scope,
        channel.charge,
        &channel.shared,
    )
    .unwrap();
    let weak = {
        let mut shared = channel.shared.lock().unwrap();
        shared.receive[1][0].created = Instant::now() - config.ttl;
        Arc::downgrade(&shared.receive[1][0].value.ready.message)
    };
    assert!(channel.try_recv().unwrap().is_none());
    assert!(weak.upgrade().is_some());
    assert_eq!(channel.status().unwrap().pending_receive, 1);
    assert!(expire_receive_one(&channel.shared, 1, config.ttl).unwrap());
    assert!(weak.upgrade().is_none());
    assert_eq!(channel.status().unwrap().pending_receive, 0);
    assert_eq!(channel.status().unwrap().dropped_received, 1);
}

fn current_status(channel: &HostChannel) -> ChannelStatus {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(status) = channel.status() {
            return status;
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
}

#[test]
fn ingress_prefix_rejects_bad_magic_version_kind_and_every_truncation() {
    let config = config(0);
    let mut bytes = transport::encode(&body(&config, false), config.codec).unwrap();
    for length in 0..11 {
        assert!(transport::body_prefix(&bytes[..length]).is_err());
    }
    assert!(transport::body_prefix(&bytes[..11]).unwrap());
    for tag in 2..=6 {
        bytes[10] = tag;
        assert!(!transport::body_prefix(&bytes[..11]).unwrap());
    }
    bytes[10] = 7;
    assert!(transport::body_prefix(&bytes[..11]).unwrap());
    assert!(transport::decode(&bytes[..11], config.codec).is_err());
    bytes[10] = 8;
    assert!(!transport::body_prefix(&bytes[..11]).unwrap());
    assert!(transport::decode(&bytes[..11], config.codec).is_err());
    for tag in [0, 9, 255] {
        bytes[10] = tag;
        assert!(transport::body_prefix(&bytes).is_err());
    }
    bytes[10] = 1;
    for offset in 0..10 {
        bytes[offset] ^= 1;
        assert!(transport::body_prefix(&bytes).is_err());
        bytes[offset] ^= 1;
    }
    // Classification is deliberately not full-body validation.
    assert!(transport::body_prefix(&bytes[..11]).unwrap());
    assert!(transport::decode(&bytes[..11], config.codec).is_err());
}

#[test]
fn saturated_receive_lane_is_rejected_before_body_decode_and_errors_release_charge() {
    let mut config = config(0);
    config.receive.body.messages = 1;
    let channel = unstarted(&config);
    let input = body(&config, true);
    receive_one(
        decoded(&config, 1, &input),
        &config,
        &channel.scope,
        channel.charge,
        &channel.shared,
    )
    .unwrap();
    let mut truncated = decoded(&config, 1, &input);
    truncated.chunks[0].truncate(11);
    // Valid lane prefix, invalid body: saturation must win over decode.
    receive_one(
        truncated,
        &config,
        &channel.scope,
        channel.charge,
        &channel.shared,
    )
    .unwrap();
    assert_eq!(channel.status().unwrap().dropped_received, 1);
    assert_eq!(channel.status().unwrap().invalid_received, 0);
    assert!(matches!(
        channel.try_recv().unwrap(),
        Some(ChannelEvent::Received(_))
    ));
    let mut truncated = decoded(&config, 1, &input);
    truncated.chunks[0].truncate(11);
    receive_one(
        truncated,
        &config,
        &channel.scope,
        channel.charge,
        &channel.shared,
    )
    .unwrap();
    assert_eq!(channel.status().unwrap().invalid_received, 1);
    let shared = channel.shared.lock().unwrap();
    assert_eq!(shared.receive_usage[1].messages, 0);
    assert_eq!(shared.receive_usage[1].bytes, 0);
}

#[test]
fn retirement_last_body_reference_is_destroyed_only_by_owner_and_returns_quota() {
    let config = config(0);
    let channel = unstarted(&config);
    let message = body(&config, true);
    let weak = Arc::downgrade(&message);
    let ready = prepared(&channel, &config, message);
    assert!(matches!(
        channel.try_retire(Retirement::Ready(ready)).unwrap(),
        RetireAdmission::Accepted
    ));
    assert!(weak.upgrade().is_some());
    let shared = channel.shared.lock().unwrap();
    assert_eq!(shared.retire_usage[1].messages, 1);
    assert_eq!(shared.retire_usage[1].bytes, channel.preparation_charge());
    drop(shared);
    assert!(retire_one(&channel.shared, 1).unwrap());
    assert!(weak.upgrade().is_none());
    assert_eq!(channel.status().unwrap().pending_retirement, 0);
    assert_eq!(channel.status().unwrap().retired_messages, 1);
    assert_eq!(channel.shared.lock().unwrap().retire_usage[1].bytes, 0);
}

#[test]
fn retirement_backpressure_contention_and_stop_return_original_and_keep_lanes_independent() {
    let mut config = config(0);
    config.prepare.body.messages = 1;
    let channel = unstarted(&config);
    let first = body(&config, true);
    assert!(matches!(
        channel.try_retire(Retirement::Message(first)).unwrap(),
        RetireAdmission::Accepted
    ));
    let second = body(&config, true);
    let RetireAdmission::Backpressure(Retirement::Message(returned)) = channel
        .try_retire(Retirement::Message(second.clone()))
        .unwrap()
    else {
        panic!("full retirement lane lost original message")
    };
    assert!(Arc::ptr_eq(&second, &returned));
    // Retained garbage cannot consume preparation or the control lane.
    assert!(matches!(
        channel.try_prepare(request(1, second.clone())).unwrap(),
        PrepareAdmission::Accepted
    ));
    assert!(matches!(
        channel.try_retire(Retirement::Message(control())).unwrap(),
        RetireAdmission::Accepted
    ));
    let lock = channel.shared.lock().unwrap();
    let RetireAdmission::Backpressure(Retirement::Message(returned)) = channel
        .try_retire(Retirement::Message(second.clone()))
        .unwrap()
    else {
        panic!("contention lost original message")
    };
    assert!(Arc::ptr_eq(&second, &returned));
    drop(lock);
    channel.stop.store(true, Ordering::Release);
    let RetireAdmission::Rejected {
        value: Retirement::Message(returned),
        ..
    } = channel
        .try_retire(Retirement::Message(second.clone()))
        .unwrap()
    else {
        panic!("stopped owner lost original message")
    };
    assert!(Arc::ptr_eq(&second, &returned));
}

#[test]
fn resident_owner_reclaims_retirement_while_transport_is_offline() {
    let config = config(0);
    let network = NetworkWorker::start(network_config(0), key(0)).unwrap();
    let message = body(&config, true);
    let weak = Arc::downgrade(&message);
    let mut channel = HostChannel::start(network, config).unwrap();
    let mut value = Retirement::Message(message);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match channel.try_retire(value).unwrap() {
            RetireAdmission::Accepted => break,
            RetireAdmission::Backpressure(returned) => value = returned,
            RetireAdmission::Rejected { reason, .. } => panic!("retirement rejected: {reason}"),
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    loop {
        if current_status(&channel).retired_messages == 1 {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    assert!(weak.upgrade().is_none());
    channel.shutdown().unwrap();
}

#[test]
fn retirement_bytes_stop_admission_before_item_limit_and_reuse_after_owner_drop() {
    let mut config = config(0);
    config.prepare.body.bytes = config.codec.message_bytes * 4 + 4096;
    let channel = unstarted(&config);
    assert!(config.prepare.body.messages > 1);
    let mut ready = prepared(&channel, &config, body(&config, false));
    let body = Retirement::Body {
        prepared: ready.prepared,
        request: ready.body.as_mut().unwrap().take_request(),
        candidate: None,
    };
    assert!(matches!(
        channel.try_retire(body).unwrap(),
        RetireAdmission::Accepted
    ));
    let message = control();
    assert!(matches!(
        channel
            .try_retire(Retirement::Input(PrepareInput::New(message)))
            .unwrap(),
        RetireAdmission::Accepted
    ));
    let request = BatchRequest::new(
        vec![vec![0x11; 64]],
        *ready.body.as_ref().unwrap().context(),
        config.policy.clone(),
    )
    .unwrap();
    let RetireAdmission::Backpressure(value @ Retirement::Request(_)) =
        channel.try_retire(Retirement::Request(request)).unwrap()
    else {
        panic!("byte ceiling did not preserve request")
    };
    assert!(retire_one(&channel.shared, 1).unwrap());
    assert!(matches!(
        channel.try_retire(value).unwrap(),
        RetireAdmission::Accepted
    ));
    assert_eq!(channel.shared.lock().unwrap().retire_usage[1].messages, 1);
}

#[test]
fn send_peer_count_and_byte_shares_cannot_consume_other_peer_or_other_lane() {
    for byte_limited in [false, true] {
        let mut config = config(0);
        let charge = validate(&config).unwrap().1;
        let global = QueueBudget {
            messages: 4,
            bytes: charge * if byte_limited { 2 } else { 4 },
        };
        config.send = LaneBudget {
            control: global,
            body: global,
        };
        let channel = unstarted(&config);
        let share = if byte_limited { 1 } else { 2 };
        for message in [control(), body(&config, false)] {
            let handle = prepared(&channel, &config, message).prepared;
            for _ in 0..share {
                assert!(matches!(
                    channel
                        .try_send(Outbound {
                            peer: peer(1),
                            message: handle.clone()
                        })
                        .unwrap(),
                    SendAdmission::Accepted
                ));
            }
            let SendAdmission::Backpressure(returned) = channel
                .try_send(Outbound {
                    peer: peer(1),
                    message: handle.clone(),
                })
                .unwrap()
            else {
                panic!("one peer consumed another peer's reservation")
            };
            assert_eq!(returned.peer, peer(1));
            assert!(Arc::ptr_eq(&returned.message.0, &handle.0));
            assert!(matches!(
                channel
                    .try_send(Outbound {
                        peer: peer(2),
                        message: handle.clone()
                    })
                    .unwrap(),
                SendAdmission::Accepted
            ));
            let shared = channel.shared.lock().unwrap();
            assert_eq!(shared.send_usage[handle.0.lane].messages, share + 1);
            assert_eq!(
                shared.send_peer_usage[handle.0.lane][&peer(1)].messages,
                share
            );
            assert!(shared.send_usage[handle.0.lane].bytes <= global.bytes);
        }
    }
}

#[test]
fn send_peer_budget_configuration_rejects_impossible_reservations() {
    let mut config = config(0);
    let charge = validate(&config).unwrap().1;
    config.send.control.messages = 1;
    assert!(validate(&config).is_err());
    config.send.control.messages = 8;
    config.send.body.bytes = charge;
    assert!(validate(&config).is_err());
    config.send.body.bytes = charge * config.peers.len();
    assert!(validate(&config).is_ok());
}

#[test]
fn real_offline_peer_saturation_leaves_healthy_vote_delivery_before_ttl() {
    let relay = Relay::start();
    let mut network_config = network_config(0);
    network_config.relay.endpoint = relay.endpoint.clone();
    network_config.relay.connect_timeout_ms = 2000;
    network_config.relay.tls_trust = ProductRelayTlsTrustV1::ExplicitCa {
        certificate_path: relay.cert.clone(),
    };
    // Fill the offline peer's actual network queue with two messages first;
    // subsequent jobs must remain backpressured on the Host owner.
    network_config.limits.outbound.peer_max_messages = 2;
    network_config.limits.outbound.max_messages = 8;
    let network = NetworkWorker::start(network_config, key(0)).unwrap();
    let mut source_config = config(0);
    let charge = validate(&source_config).unwrap().1;
    source_config.send.control = QueueBudget {
        messages: 4,
        bytes: charge * 4,
    };
    let mut source = HostChannel::start(network, source_config.clone()).unwrap();
    let mut sink = HostChannel::start(relay.network(1), config(1)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    for token in 1..=4 {
        let handle = await_prepare(
            &source,
            request(
                token,
                Arc::new(Message::RequestBody {
                    body_id: [token as u8; 32],
                }),
            ),
        )
        .prepared;
        admit_send(
            &source,
            Outbound {
                peer: peer(2),
                message: handle,
            },
        );
    }
    loop {
        let status = current_status(&source);
        if status.sent_messages == 2 && status.pending_send == 2 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "offline queues did not reach explicit saturation"
        );
        thread::yield_now();
    }
    let vote = novovm_consensus::round_bft::test_vectors::sign_vote(
        wire_context(&source_config),
        0,
        Phase::Prevote,
        Some([0x21; 32]),
        &source_config.validators,
        &key(0),
    )
    .unwrap();
    let handle = await_prepare(&source, request(5, Arc::new(Message::Vote(vote)))).prepared;
    // The two unsent offline jobs remain live; they cannot prevent this other
    // peer's control admission or actual authenticated network delivery.
    admit_send(
        &source,
        Outbound {
            peer: peer(1),
            message: handle,
        },
    );
    loop {
        if let Some(ChannelEvent::Received(received)) = sink.try_recv().unwrap() {
            assert_eq!(received.peer, peer(0));
            assert!(matches!(
                received.ready.evidence.as_ref(),
                VerifiedEvidence::Vote(_)
            ));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "offline peer starved healthy vote delivery"
        );
        thread::yield_now();
    }
    let status = current_status(&source);
    assert_eq!(status.expired_sends, 0);
    assert_eq!(status.pending_send, 2);
    assert_eq!(status.sent_messages, 3);
    source.shutdown().unwrap();
    sink.shutdown().unwrap();
}
