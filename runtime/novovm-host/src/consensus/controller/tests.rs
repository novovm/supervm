//! Real journal/compute tests; the four-process network acceptance is separate.
use super::*;
use crate::consensus::channel::{
    ChannelConfig, ChannelEvent, HostChannel, LaneBudget, Outbound, PrepareAdmission, PrepareInput,
    PrepareRequest, QueueBudget, RetireAdmission, Retirement, SendAdmission, VerifiedEvidence,
};
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
    // A closed loopback port exercises asynchronous connection failure; it is
    // not a fake transport or a claimed real-WSS finality test.
    test_channel(
        config,
        index,
        "wss://127.0.0.1:1/novovm".into(),
        ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
        50,
    )
}

fn test_channel(
    config: &ControllerConfig,
    index: usize,
    endpoint: String,
    tls_trust: ProductRelayTlsTrustV1,
    connect_timeout_ms: u64,
) -> Result<HostChannel> {
    let peers: Vec<_> = config.peers.values().cloned().collect();
    let relay = SigningKey::from_bytes(&[91; 32]);
    let network = NetworkWorker::start(
        NetworkWorkerConfig {
            chain_id: CHAIN,
            peers: peers.clone(),
            limits: WorkerLimits::default(),
            relay: ProductRelayClientConfigV1 {
                endpoint,
                expected_relay_peer_id: peer_id_from_ed25519_public_key_v1(
                    &relay.verifying_key().to_bytes(),
                ),
                connect_timeout_ms,
                read_timeout_ms: 10,
                tls_trust,
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
        let stats = controller.stats();
        let observed = stats
            .last_execution_observation
            .context("real executed batch observation missing")?;
        ensure!(observed.components > 0 && observed.peak_callbacks >= 1
            && observed.peak_callbacks <= observed.components,
            "real AOEM callback observation has inconsistent component/overlap counts: {observed:?}");
        // One real completed batch: every aggregate must equal its executor
        // observation, without assuming a small batch naturally overlaps >1.
        ensure!(
            stats.execution_components_total == observed.components as u64
                && stats.execution_credit_only_accounts_total
                    == observed.credit_only_accounts as u64
                && stats.execution_recomputed_transactions_total
                    == observed.recomputed_transactions as u64
                && stats.execution_peak_callbacks == observed.peak_callbacks
                && !stats.execution_observation_saturated,
            "controller did not preserve actual execution evidence: {stats:?}"
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

#[test]
#[ignore = "requires explicit real AOEM; real WSS body loss and repeated signed-proposal recovery"]
fn real_controller_recovers_initial_and_direct_body_loss_before_remote_execution_and_vote(
) -> Result<()> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    // Validate with a canonical path, but do not pass Windows' verbatim \\?\
    // spelling to the native AOEM/RocksDB path boundary.
    let artifacts =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/runtime-rebuild/controller-tests");
    fs::create_dir_all(&artifacts)?;
    ensure!(
        artifacts.canonicalize()?.starts_with(&repository),
        "controller body-loss artifacts outside repository"
    );
    let directory = artifacts.join(format!(
        "body-loss-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir(&directory)?;
    let leader_directory = directory.join("leader");
    let receiver_directory = directory.join("receiver");
    let root = initialize(&leader_directory)?;
    ensure!(
        initialize(&receiver_directory)? == root,
        "body-loss fixture parents differ"
    );
    let mut relay = super::network_integration::Relay::start(&directory.join("relay"))?;
    let mut services = Services::default();
    for path in [&leader_directory, &receiver_directory] {
        services.0.push(CandidatePipeline::start(
            pipeline_config(path)?,
            OpenMode::Existing,
        )?);
    }
    let (leader_config, leader_index, context, parent) = fixture(root)?;
    let set = leader_config.validators.clone();
    let receiver_index = (leader_index + 1) % 4;
    let receiver_id =
        Validator::new(validator_key(receiver_index).verifying_key().to_bytes(), 1)?.id();
    let leader_peer =
        peer_id_from_ed25519_public_key_v1(&validator_key(leader_index).verifying_key().to_bytes());
    let (mut receiver_config, _, _, _) = fixture(root)?;
    receiver_config.local_validator = receiver_id;
    receiver_config.peers = set
        .members()
        .iter()
        .filter(|member| member.id() != receiver_id)
        .map(|member| {
            (
                member.id(),
                peer_id_from_ed25519_public_key_v1(member.public_key()),
            )
        })
        .collect();
    let leader_journal = open_journal(
        ValidatorJournal::open(
            &services.0[0],
            context,
            parent,
            set.clone(),
            validator_key(leader_index),
        )?,
        &services.0[0],
    )?;
    let mut receiver_journal = open_journal(
        ValidatorJournal::open(
            &services.0[1],
            context,
            parent,
            set.clone(),
            validator_key(receiver_index),
        )?,
        &services.0[1],
    )?;
    let connected = |config: &ControllerConfig, index| {
        test_channel(
            config,
            index,
            relay.endpoint.clone(),
            ProductRelayTlsTrustV1::ExplicitCa {
                certificate_path: relay.certificate.clone(),
            },
            2_000,
        )
    };
    let leader_channel = connected(&leader_config, leader_index)?;
    let mut receiver = connected(&receiver_config, receiver_index)?;
    let mut leader = Controller::new(leader_config, leader_journal, leader_channel)?;
    let result = (|| -> Result<()> {
        // This receiving loss probe is NOT a second autonomous controller. It
        // uses only the public channel/pipeline/journal boundaries and never
        // prepares a candidate, signature or business result on another node's
        // behalf. Its sole network request is triggered by a repeated, verified
        // leader proposal after intentionally discarding a completed body.
        let originals = raw()?;
        let local_body = Arc::new(Message::Body {
            context: batch_context(root),
            raw_transactions: originals.clone(),
        });
        let deadline = Instant::now() + DEADLINE;
        let mut offered = false;
        let mut proposal = None;
        let mut proposal_bytes = None;
        let mut expected_body = None;
        let mut proposals_received = 0;
        let mut bodies_received = 0;
        let mut requests_sent = 0;
        let mut pending_prepare = None;
        let mut preparing = false;
        let mut pending_send: Option<(u64, Outbound)> = None;
        let mut pending_request = None;
        let mut execution: Option<PipelineTicket> = None;
        let mut executed = false;
        let mut vote_sent = false;
        let mut retired = std::collections::VecDeque::new();
        loop {
            ensure!(
                Instant::now() < deadline,
                "body-loss recovery timed out: proposals={proposals_received}, bodies={bodies_received}, requests={requests_sent}, executed={executed}, leader={:?}",
                leader.stats()
            );
            leader.poll(&services.0[0], Instant::now())?;
            if !offered {
                offered = leader.try_submit_body(&local_body)?;
            }
            if let Some(value) = retired.pop_front() {
                match receiver.try_retire(value)? {
                    RetireAdmission::Accepted => {}
                    RetireAdmission::Backpressure(value) => retired.push_front(value),
                    RetireAdmission::Rejected { reason, .. } => bail!(reason),
                }
            }
            if let Some(request) = pending_prepare.take() {
                match receiver.try_prepare(request)? {
                    PrepareAdmission::Accepted => preparing = true,
                    PrepareAdmission::Backpressure(request) => pending_prepare = Some(request),
                    PrepareAdmission::Rejected { reason, .. } => bail!(reason),
                }
            }
            if let Some((token, outbound)) = pending_send.take() {
                match receiver.try_send(outbound)? {
                    SendAdmission::Accepted => {
                        if token <= 2 {
                            requests_sent += 1;
                        } else {
                            vote_sent = true;
                        }
                    }
                    SendAdmission::Backpressure(outbound) => {
                        pending_send = Some((token, outbound));
                    }
                    SendAdmission::Rejected { reason, .. } => bail!(reason),
                }
            }
            if retired.is_empty() {
                if let Some(event) = receiver.try_recv()? {
                    match event {
                        ChannelEvent::Prepared { token, result } => {
                            let ready = result.map_err(anyhow::Error::msg)?;
                            ensure!(
                                preparing && pending_send.is_none(),
                                "unexpected prepared reply"
                            );
                            preparing = false;
                            pending_send = Some((
                                token,
                                Outbound {
                                    peer: leader_peer.clone(),
                                    message: ready.prepared.clone(),
                                },
                            ));
                            retired.push_back(Retirement::Ready(ready));
                        }
                        ChannelEvent::Received(mut received) => {
                            ensure!(received.peer == leader_peer, "unexpected active peer");
                            match received.ready.message.as_ref() {
                                Message::Proposal { body_id, .. } => {
                                    let VerifiedEvidence::Proposal {
                                        proposal: verified,
                                        valid_quorum,
                                    } = received.ready.evidence.as_ref()
                                    else {
                                        bail!("proposal lacks Host owner signature verification");
                                    };
                                    ensure!(
                                        verified.proposal().context == context
                                            && verified.proposal().round == 0,
                                        "unexpected proposal subject"
                                    );
                                    let bytes = wire::encode_proposal(verified.proposal())?;
                                    if let Some(previous) = &proposal_bytes {
                                        ensure!(
                                            *previous == bytes,
                                            "retransmission changed signed proposal"
                                        );
                                    }
                                    proposal_bytes = Some(bytes);
                                    proposal = Some((verified.clone(), valid_quorum.clone()));
                                    if let Some(previous) = expected_body {
                                        ensure!(previous == *body_id, "proposal changed body hint");
                                    }
                                    expected_body = Some(*body_id);
                                    proposals_received += 1;
                                    // No timer or admission is treated as delivery. A NEW
                                    // duplicate proposal after each observed body loss
                                    // re-requests the same body through the real network.
                                    if proposals_received >= 2
                                        && (1..=2).contains(&bodies_received)
                                        && requests_sent < bodies_received
                                        && pending_prepare.is_none()
                                        && !preparing
                                        && pending_send.is_none()
                                    {
                                        pending_prepare = Some(PrepareRequest {
                                            token: bodies_received,
                                            input: PrepareInput::New(Arc::new(
                                                Message::RequestBody { body_id: *body_id },
                                            )),
                                        });
                                    }
                                }
                                Message::Body {
                                    context: body_context,
                                    raw_transactions,
                                } => {
                                    ensure!(
                                        *body_context == batch_context(root)
                                            && *raw_transactions == originals,
                                        "recovered body differs from signed originals or parent"
                                    );
                                    let mut body = received
                                        .ready
                                        .body
                                        .take()
                                        .context("owner body request absent")?;
                                    if let Some(previous) = expected_body {
                                        ensure!(previous == body.id(), "recovered body id differs");
                                    }
                                    expected_body = Some(body.id());
                                    bodies_received += 1;
                                    match bodies_received {
                                        1 => ensure!(requests_sent == 0, "first body was not initial fanout"),
                                        2 => ensure!(requests_sent == 1, "first requested body was not re-armed once"),
                                        3 => {
                                            ensure!(requests_sent == 2, "body arrived without the second request");
                                            pending_request = body.take_request();
                                            ensure!(pending_request.is_some(), "owner did not prepare received raw execution");
                                        }
                                        _ => bail!("unsolicited repeated full body after two explicit requests"),
                                    }
                                    // The first two intact network bodies are deliberately
                                    // discarded without giving their requests to AOEM.
                                    received.ready.body = Some(body);
                                }
                                _ => {}
                            }
                            retired.push_back(Retirement::Ready(received.ready));
                        }
                    }
                }
            }
            if let Some(request) = pending_request.take() {
                ensure!(
                    !executed && execution.is_none(),
                    "body executed more than once"
                );
                match services.0[1].try_submit_owned(request) {
                    Ok(Submission::Accepted(ticket)) => execution = Some(ticket),
                    Ok(Submission::Backpressured(request)) => pending_request = Some(request),
                    Err(rejected) => {
                        retired.push_back(Retirement::Request(rejected.request));
                        return Err(rejected.error);
                    }
                }
            }
            if let Some(ticket) = execution.as_mut() {
                if let Some(batch) = ticket.try_take()? {
                    let (verified, valid) =
                        proposal.as_ref().context("body executed before proposal")?;
                    let statement =
                        BlockStatement::from_executed(&batch.packet, context, &set, &parent)?;
                    ensure!(
                        statement.hash() == verified.proposal().value
                            && batch.observation.peak_callbacks >= 1,
                        "retrieved body did not really execute to the proposed value"
                    );
                    receiver_journal.accept_proposal(
                        verified,
                        batch.candidate(),
                        valid.as_ref(),
                    )?;
                    retired.push_back(Retirement::Batch(batch));
                    executed = true;
                    execution = None;
                }
            }
            if receiver_journal.is_pending() {
                if let Some(message) = receiver_journal.poll(&services.0[1])? {
                    let message = message.context("remote signing ACK lacked a message")?;
                    let value = proposal
                        .as_ref()
                        .context("signed vote has no proposal")?
                        .0
                        .proposal()
                        .value;
                    let vote = wire_vote(message, &set, Phase::Prevote, value)?;
                    ensure!(
                        vote.validator_id == receiver_id,
                        "vote belongs to another validator"
                    );
                    ensure!(
                        pending_prepare.is_none() && !preparing && pending_send.is_none(),
                        "vote overtook body request"
                    );
                    pending_prepare = Some(PrepareRequest {
                        token: 3,
                        input: PrepareInput::New(Arc::new(Message::Vote(vote))),
                    });
                }
            }
            if vote_sent && leader.stats().prevote_weight == 2 && retired.is_empty() {
                ensure!(
                    executed && bodies_received == 3 && requests_sent == 2
                        && proposals_received >= 3 && leader.stats().executed_batches == 1,
                    "lost-body regression bypassed repeated proposal, local execution or exact retries"
                );
                ensure!(
                    leader.head().is_none()
                        && receiver_journal.head().is_none()
                        && matches!(
                            receiver_journal.last_durable_message(),
                            Some(DurableMessage::Vote(_))
                        ),
                    "two validators manufactured finality or vote escaped the durable outbox"
                );
                break;
            }
            std::thread::yield_now();
        }
        Ok(())
    })();
    let leader_shutdown = leader.shutdown();
    let receiver_shutdown = receiver.shutdown();
    drop(leader);
    drop(receiver);
    let pipelines_shutdown = services.shutdown();
    let relay_shutdown = relay.shutdown();
    result?;
    leader_shutdown?;
    receiver_shutdown?;
    pipelines_shutdown?;
    relay_shutdown
}

#[test]
#[ignore = "requires explicit real AOEM; deterministic verified-control ingress during cold recovery"]
fn real_cold_recovery_progresses_with_control_retirement_on_every_poll() -> Result<()> {
    // This fixture fixes ingress TIMING only. It uses the real AOEM candidate,
    // persisted journal, reopened pipeline, and real channel preparation owner;
    // it is not a live-WSS/independent-process or performance measurement.
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/controller-tests")
        .join(format!(
            "recovery-ingress-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    let root = initialize(&directory)?;
    let mut services = Services::default();
    services.0.push(CandidatePipeline::start(
        pipeline_config(&directory)?,
        OpenMode::Existing,
    )?);
    let (config, index, context, parent) = fixture(root)?;
    let local = config.local_validator;
    let journal = open_journal(
        ValidatorJournal::open(
            &services.0[0],
            context,
            parent,
            config.validators.clone(),
            validator_key(index),
        )?,
        &services.0[0],
    )?;
    let channel = disconnected_channel(&config, index)?;
    let mut controller = Controller::new(config, journal, channel)?;
    let original = (|| -> Result<MetadataSnapshot> {
        let body = Arc::new(Message::Body {
            context: batch_context(root),
            raw_transactions: raw()?,
        });
        let deadline = Instant::now() + DEADLINE;
        while !controller.try_submit_body(&body)? {
            ensure!(Instant::now() < deadline, "initial body admission stalled");
            controller.poll(&services.0[0], Instant::now())?;
            std::thread::yield_now();
        }
        while controller.stats().durable_votes != 1 || controller.is_pending() {
            ensure!(Instant::now() < deadline, "initial durable prevote stalled");
            controller.poll(&services.0[0], Instant::now())?;
            std::thread::yield_now();
        }
        ensure!(
            controller.head().is_none(),
            "single signer manufactured finality"
        );
        read_metadata(&services.0[0], vec![MetaKey::ConsensusState(local)])
    })();
    controller.shutdown()?;
    drop(controller);
    services.shutdown()?;
    let original = original?;

    // Nothing from the first execution is submitted to this new owner. Only
    // the journal's immutable candidate locator can recover the actual body.
    services.0.push(CandidatePipeline::start(
        pipeline_config(&directory)?,
        OpenMode::Existing,
    )?);
    let (config, index, context, parent) = fixture(root)?;
    let remote_index = (0..4).find(|candidate| *candidate != index).unwrap();
    let remote = Validator::new(validator_key(remote_index).verifying_key().to_bytes(), 1)?.id();
    let peer = config
        .peers
        .get(&remote)
        .context("remote route missing")?
        .clone();
    let vote = wire::Vote::sign(
        context,
        0,
        Phase::Prevote,
        None,
        &config.validators,
        &validator_key(remote_index),
    )?;
    let journal = open_journal(
        ValidatorJournal::open(
            &services.0[0],
            context,
            parent,
            config.validators.clone(),
            validator_key(index),
        )?,
        &services.0[0],
    )?;
    let channel = disconnected_channel(&config, index)?;
    let ready = (|| -> Result<_> {
        let deadline = Instant::now() + DEADLINE;
        let mut request = PrepareRequest {
            token: u64::MAX,
            input: PrepareInput::New(Arc::new(Message::Vote(vote))),
        };
        loop {
            ensure!(
                Instant::now() < deadline,
                "control preparation admission stalled"
            );
            match channel.try_prepare(request)? {
                PrepareAdmission::Accepted => break,
                PrepareAdmission::Backpressure(returned) => request = returned,
                PrepareAdmission::Rejected { reason, .. } => bail!(reason),
            }
            std::thread::yield_now();
        }
        loop {
            ensure!(Instant::now() < deadline, "control preparation stalled");
            match channel.try_recv()? {
                Some(ChannelEvent::Prepared {
                    token: u64::MAX,
                    result,
                }) => {
                    return result.map_err(anyhow::Error::msg);
                }
                None => std::thread::yield_now(),
                Some(_) => bail!("unexpected event on disconnected preparation owner"),
            }
        }
    })()?;
    let mut controller = Controller::new(config, journal, channel)?;
    let result = (|| -> Result<()> {
        ensure!(
            controller.is_recovering(),
            "cold journal bypassed recovery gate"
        );
        let deadline = Instant::now() + DEADLINE;
        let mut supplied = 0usize;
        while controller.is_recovering() {
            ensure!(
                Instant::now() < deadline,
                "control retirement starved actual candidate recovery: {:?}",
                controller.stats()
            );
            // Exactly one bounded slot: if prior owner backpressure left the
            // previous control queued, keep it rather than inventing capacity.
            supplied += usize::from(controller.regression_recovery_control(&peer, &ready)?);
            controller.poll(&services.0[0], Instant::now())?;
            std::thread::yield_now();
        }
        ensure!(
            supplied >= 2,
            "fixture did not maintain repeated control ingress"
        );
        ensure!(
            controller.stats().executed_batches == 1
                && controller.stats().durable_votes == 0
                && controller.stats().received_votes == 2
                && !controller.is_pending()
                && controller.head().is_none()
                && controller.context() == context
                && controller.parent() == parent,
            "cold gate skipped real execution or signed/advanced during replay: {:?}",
            controller.stats()
        );
        ensure!(
            read_metadata(&services.0[0], vec![MetaKey::ConsensusState(local)])? == original,
            "control ingress/replay changed durable signer revision or safety snapshot"
        );
        Ok(())
    })();
    controller.shutdown()?;
    drop(controller);
    drop(ready);
    services.shutdown()?;
    result
}

#[test]
#[ignore = "requires explicit real AOEM; deterministic verified-control ingress during warm execution"]
fn real_warm_execution_progresses_with_control_retirement_on_every_poll() -> Result<()> {
    warm_control_retirement(ControllerLimits::default().events_per_poll)
}

#[test]
#[ignore = "requires explicit real AOEM; one-event warm controller scheduling budget"]
fn real_warm_execution_progresses_with_one_event_per_poll() -> Result<()> {
    warm_control_retirement(1)
}

fn warm_control_retirement(events_per_poll: usize) -> Result<()> {
    // Only ingress timing is deterministic here. Body preparation, signature
    // checking, AOEM execution, persistence and signing use the real owners.
    // This is not a live-WSS, independent-process or throughput acceptance.
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/controller-tests")
        .join(format!(
            "warm-ingress-{events_per_poll}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    let root = initialize(&directory)?;
    let mut services = Services(vec![CandidatePipeline::start(
        pipeline_config(&directory)?,
        OpenMode::Existing,
    )?]);
    let pipeline = &services.0[0];
    let (mut config, index, context, parent) = fixture(root)?;
    config.limits.events_per_poll = events_per_poll;
    let set = config.validators.clone();
    let local = config.local_validator;
    let remote_index = (0..4).find(|candidate| *candidate != index).unwrap();
    let remote = Validator::new(validator_key(remote_index).verifying_key().to_bytes(), 1)?.id();
    let peer = config
        .peers
        .get(&remote)
        .context("remote route missing")?
        .clone();
    let vote = wire::Vote::sign(
        context,
        0,
        Phase::Prevote,
        None,
        &set,
        &validator_key(remote_index),
    )?;
    let journal = open_journal(
        ValidatorJournal::open(pipeline, context, parent, set.clone(), validator_key(index))?,
        pipeline,
    )?;
    let channel = disconnected_channel(&config, index)?;
    let ready = (|| -> Result<_> {
        let deadline = Instant::now() + DEADLINE;
        let mut request = PrepareRequest {
            token: u64::MAX,
            input: PrepareInput::New(Arc::new(Message::Vote(vote))),
        };
        loop {
            ensure!(
                Instant::now() < deadline,
                "warm control preparation admission stalled"
            );
            match channel.try_prepare(request)? {
                PrepareAdmission::Accepted => break,
                PrepareAdmission::Backpressure(returned) => request = returned,
                PrepareAdmission::Rejected { reason, .. } => bail!(reason),
            }
            std::thread::yield_now();
        }
        loop {
            ensure!(
                Instant::now() < deadline,
                "warm control preparation stalled"
            );
            match channel.try_recv()? {
                Some(ChannelEvent::Prepared {
                    token: u64::MAX,
                    result,
                }) => return result.map_err(anyhow::Error::msg),
                None => std::thread::yield_now(),
                Some(_) => bail!("unexpected event on disconnected preparation owner"),
            }
        }
    })()?;
    let mut controller = Controller::new(config, journal, channel)?;
    let result = (|| -> Result<()> {
        ensure!(
            !controller.is_recovering(),
            "fresh journal entered recovery"
        );
        let body = Arc::new(Message::Body {
            context: batch_context(root),
            raw_transactions: raw()?,
        });
        // Keep the protocol clock fixed. A wall-clock failure must not turn
        // into a timeout/nil vote and make the execution assertion pass.
        let now = Instant::now();
        let deadline = now + DEADLINE;
        let mut supplied = 0usize;
        while !controller.try_submit_body(&body)? {
            ensure!(Instant::now() < deadline, "warm body admission stalled");
            supplied += usize::from(controller.regression_warm_control(&peer, &ready)?);
            controller.poll(pipeline, now)?;
            std::thread::yield_now();
        }
        while controller.stats().durable_votes != 1
            || controller.is_pending()
            || controller.stats().prevote_weight != 2
        {
            ensure!(
                Instant::now() < deadline,
                "control retirement starved warm execution/durable prevote: {:?}",
                controller.stats()
            );
            supplied += usize::from(controller.regression_warm_control(&peer, &ready)?);
            controller.poll(pipeline, now)?;
            std::thread::yield_now();
        }
        ensure!(supplied >= 2, "fixture did not sustain control ingress");
        let stats = controller.stats();
        let observed = stats
            .last_execution_observation
            .context("warm execution lacks actual AOEM callback observation")?;
        ensure!(
            stats.executed_batches == 1
                && stats.execution_failures == 0
                && observed.components > 0
                && observed.peak_callbacks > 0
                && stats.durable_votes == 1
                && stats.received_votes == 2
                && stats.prevote_weight == 2
                && stats.precommit_weight == 0
                && stats.durable_decisions == 0
                && controller.head().is_none()
                && controller.context() == context
                && controller.parent() == parent
                && controller.round() == 0,
            "warm control flow skipped real execution, invented votes or finalized: {stats:?}"
        );
        // Re-read the actual AOEM journal. Its latest ACK must be our non-nil
        // prevote, not the injected remote vote or a timer-created nil vote.
        let reopened = open_journal(
            ValidatorJournal::open(pipeline, context, parent, set.clone(), validator_key(index))?,
            pipeline,
        )?;
        let Some(DurableMessage::Vote(vote)) = reopened.last_durable_message() else {
            bail!("warm execution did not persist a local prevote");
        };
        vote.verify(&set)?;
        ensure!(
            vote.validator_id == local
                && vote.context == context
                && vote.round == 0
                && vote.phase == Phase::Prevote
                && vote.value.is_some()
                && reopened.decided().is_none(),
            "warm execution persisted the wrong signer/phase/value or a decision"
        );
        Ok(())
    })();
    let channel_shutdown = controller.shutdown();
    drop(controller);
    drop(ready);
    let pipeline_shutdown = services.shutdown();
    result?;
    channel_shutdown?;
    pipeline_shutdown
}
