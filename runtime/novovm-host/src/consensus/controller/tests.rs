//! Real journal/compute tests; the four-process network acceptance is separate.
use super::*;
use crate::consensus::channel::{ChannelConfig, HostChannel, LaneBudget, QueueBudget};
use crate::consensus::collector::CollectorLimits;
use crate::consensus::controller::{Controller, ControllerConfig, ControllerLimits};
use crate::consensus::pacemaker::TimeoutPolicy;
use crate::consensus::transport::{DecodeLimits, Message};
use novovm_network::fragments::ReassemblyLimits;
use novovm_network::peer_id_from_ed25519_public_key_v1;
use novovm_network::product_relay_client::{ProductRelayClientConfigV1, ProductRelayTlsTrustV1};
use novovm_network::worker::{NetworkWorker, NetworkWorkerConfig, WorkerLimits};

fn fixture(root: Hash) -> Result<(ControllerConfig, usize, ConsensusContext, ParentPoint)> {
    let set = Arc::new(ValidatorSet::new(
        CHAIN,
        1,
        1,
        (0..4)
            .map(|i| Validator::new(validator_key(i).verifying_key().to_bytes(), 1))
            .collect::<Result<_>>()?,
    )?);
    let local = set.leader(1, 0)?;
    let index = (0..4)
        .find(|i| {
            Validator::new(validator_key(*i).verifying_key().to_bytes(), 1)
                .unwrap()
                .id()
                == local
        })
        .unwrap();
    let context = ConsensusContext {
        chain_id: CHAIN,
        genesis_config_commitment: GENESIS,
        protocol_commitment: PROTOCOL,
        epoch: 1,
        validator_set_hash: set.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    };
    let parent = ParentPoint {
        height: 0,
        block_hash: [0; 32],
        state_root: root,
        receipt_batch_commitment: empty_root(),
        state_version: 0,
        decision_hash: [0; 32],
    };
    Ok((
        ControllerConfig {
            peers: set
                .members()
                .iter()
                .filter(|member| member.id() != local)
                .map(|member| {
                    (
                        member.id(),
                        peer_id_from_ed25519_public_key_v1(member.public_key()),
                    )
                })
                .collect(),
            validators: set,
            local_validator: local,
            execution: batch_context(root),
            collector: CollectorLimits {
                max_retained_rounds: 2,
                max_future_round_span: 64,
                max_votes: 28,
            },
            timeouts: TimeoutPolicy {
                propose: Duration::from_secs(30),
                prevote: Duration::from_secs(30),
                precommit: Duration::from_secs(30),
                round_increment: Duration::from_secs(1),
            },
            limits: ControllerLimits::default(),
            retransmit: Duration::from_millis(100),
        },
        index,
        context,
        parent,
    ))
}

fn disconnected_channel(config: &ControllerConfig, index: usize) -> Result<HostChannel> {
    let peers: Vec<_> = config.peers.values().cloned().collect();
    // A closed loopback port exercises asynchronous connection failure; it is
    // not a fake transport or a claimed real-WSS finality test.
    let relay = SigningKey::from_bytes(&[91; 32]);
    let network = NetworkWorker::start(
        NetworkWorkerConfig {
            chain_id: CHAIN,
            peers: peers.clone(),
            limits: WorkerLimits::default(),
            relay: ProductRelayClientConfigV1 {
                endpoint: "wss://127.0.0.1:1/novovm".into(),
                expected_relay_peer_id: peer_id_from_ed25519_public_key_v1(
                    &relay.verifying_key().to_bytes(),
                ),
                connect_timeout_ms: 50,
                read_timeout_ms: 10,
                tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
            },
            handshake_timeout_ms: 1_000,
            reconnect_delay_ms: 100,
            heartbeat_interval_ms: 1_000,
            queue_ttl_ms: 5_000,
        },
        validator_key(index),
    )?;
    let codec = DecodeLimits {
        transactions: 8,
        transaction_bytes: 1024,
        body_bytes: 8192,
        message_bytes: 512 * 1024,
    };
    let lanes = LaneBudget {
        control: QueueBudget {
            messages: 16,
            bytes: 64 * 1024 * 1024,
        },
        body: QueueBudget {
            messages: 8,
            bytes: 32 * 1024 * 1024,
        },
    };
    HostChannel::start(
        network,
        ChannelConfig {
            chain_id: CHAIN,
            genesis: GENESIS,
            protocol: PROTOCOL,
            peers,
            validators: config.validators.clone(),
            policy: policy(),
            codec,
            reassembly: ReassemblyLimits {
                max_message_bytes: codec.message_bytes,
                messages: 16,
                bytes: 4 * 1024 * 1024,
                peer_messages: 4,
                peer_bytes: 2 * 1024 * 1024,
                ttl: Duration::from_secs(10),
            },
            prepare: lanes,
            send: lanes,
            receive: lanes,
            ttl: Duration::from_secs(10),
        },
    )
}

#[test]
#[ignore = "requires explicit real AOEM library; autonomous controller with disconnected transport"]
fn real_controller_executes_and_signs_without_external_protocol_driver_but_cannot_self_finalize(
) -> Result<()> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/controller-tests")
        .join(format!(
            "autonomous-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    let root = initialize(&directory)?;
    let pipeline = CandidatePipeline::start(pipeline_config(&directory)?, OpenMode::Existing)?;
    let (config, index, context, parent) = fixture(root)?;
    let journal = open_journal(
        ValidatorJournal::open(
            &pipeline,
            context,
            parent,
            config.validators.clone(),
            validator_key(index),
        )?,
        &pipeline,
    )?;
    let channel = disconnected_channel(&config, index)?;
    let mut controller = Controller::new(config, journal, channel)?;
    let result = (|| -> Result<()> {
        controller.regression_peer_requests_are_not_head_authority()?;
        let mut wrong = batch_context(root);
        wrong.parent_state_root = [0x93; 32];
        ensure!(
            controller
                .try_submit_body(&Arc::new(Message::Body {
                    context: wrong,
                    raw_transactions: raw()?
                }))
                .is_err(),
            "wrong parent body accepted"
        );
        let body = Arc::new(Message::Body {
            context: batch_context(root),
            raw_transactions: raw()?,
        });
        let deadline = Instant::now() + DEADLINE;
        while !controller.try_submit_body(&body)? {
            ensure!(
                Instant::now() < deadline,
                "controller local admission timeout"
            );
            controller.poll(&pipeline, Instant::now())?;
        }
        ensure!(
            !controller.try_submit_body(&body)?,
            "duplicate local body admitted while owner pending"
        );
        let mut checked_pending_pin = false;
        loop {
            ensure!(
                Instant::now() < deadline,
                "controller did not autonomously execute/propose/prevote: {:?}",
                controller.stats()
            );
            controller.poll(&pipeline, Instant::now())?;
            if !checked_pending_pin
                && controller.is_pending()
                && controller.stats().executed_batches == 1
            {
                controller.regression_pending_candidate_is_not_evicted()?;
                checked_pending_pin = true;
            }
            if controller.stats().prevote_weight == 1 {
                break;
            }
            std::thread::yield_now();
        }
        ensure!(
            checked_pending_pin,
            "test missed real journal stage/ACK window"
        );
        ensure!(
            controller.stats().executed_batches == 1 && controller.stats().durable_votes == 1,
            "proposal/vote was not driven by one real execution: {:?}",
            controller.stats()
        );
        ensure!(
            controller.head().is_none()
                && controller.context() == context
                && controller.parent() == parent,
            "one validator manufactured finality or changed visible parent"
        );
        ensure!(
            !controller.try_submit_body(&body)?,
            "already proposed local round accepted replacement body"
        );
        Ok(())
    })();
    controller.shutdown()?;
    drop(controller);
    pipeline.shutdown()?;
    result
}

#[test]
#[ignore = "requires explicit real AOEM library; durable journal configuration rejection"]
fn real_controller_rejects_wrong_signer_and_unreserved_body_budget() -> Result<()> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/controller-tests")
        .join(format!(
            "config-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    let root = initialize(&directory)?;
    let pipeline = CandidatePipeline::start(pipeline_config(&directory)?, OpenMode::Existing)?;
    let result = (|| -> Result<()> {
        for wrong_signer in [true, false] {
            let (mut config, index, context, parent) = fixture(root)?;
            let journal = open_journal(
                ValidatorJournal::open(
                    &pipeline,
                    context,
                    parent,
                    config.validators.clone(),
                    validator_key(index),
                )?,
                &pipeline,
            )?;
            let channel = disconnected_channel(&config, index)?;
            if wrong_signer {
                config.local_validator = config
                    .validators
                    .members()
                    .iter()
                    .find(|member| member.id() != config.local_validator)
                    .unwrap()
                    .id();
            } else {
                config.limits.max_body_bytes = 1;
            }
            let error = Controller::new(config, journal, channel)
                .err()
                .context("unsafe controller configuration admitted")?;
            let expected = if wrong_signer {
                "durable journal signer"
            } else {
                "independent peer/current/archive body slots"
            };
            ensure!(
                error.to_string().contains(expected),
                "wrong config rejection: {error:#}"
            );
        }
        Ok(())
    })();
    pipeline.shutdown()?;
    result
}
