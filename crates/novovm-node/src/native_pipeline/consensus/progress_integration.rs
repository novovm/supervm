//! Four real UDP sockets/AOEM stores. The fixture supplies business input and
//! pumps I/O; it never calls propose/vote/observe_prevotes/observe_decision,
//! constructs a QC, clears a lock, or advances a journal height.
use super::*;
use crate::native_pipeline::consensus::{
    collector::CollectorLimits,
    native::{
        tests::{codec, component_transport_key, Mesh},
        MessageDomain, PreparedMessage, ReceiveEvent,
    },
    pacemaker::TimeoutPolicy,
    progress::{ProgressLimits, RoundProgress},
    transport::{self, Message},
    ArchiveRead,
};
use crate::native_pipeline::ingress::apfl::{ApflLimits, ApflTransferBatch};
use crate::native_pipeline::pipeline::ExecutionBackend;
use std::collections::VecDeque;

#[path = "native_deadline_integration.rs"]
mod deadline;
#[path = "native_lifecycle_integration.rs"]
mod lifecycle;
#[path = "published_projection_integration.rs"]
mod published_projection;
#[path = "progress_split_integration.rs"]
mod split;
#[path = "native_successor_integration.rs"]
mod successor;

fn progress(
    journal: ValidatorJournal,
    pipeline: &CandidatePipeline,
    set: Arc<ValidatorSet>,
) -> Result<RoundProgress> {
    RoundProgress::new(
        journal,
        pipeline,
        set,
        ProgressLimits {
            candidates: 5,
            candidate_bytes: 32 * 1024 * 1024,
            votes: CollectorLimits {
                max_retained_rounds: 4,
                max_future_round_span: 32,
                max_votes: 44,
            },
        },
        TimeoutPolicy {
            propose: Duration::from_secs(5),
            prevote: Duration::from_secs(5),
            precommit: Duration::from_secs(5),
            round_increment: Duration::from_secs(1),
        },
    )
}

#[test]
#[ignore = "real AOEM; autonomous rounds over four UDP sockets/stores, not independent processes/TPS"]
fn native_progress_three_heights_with_unresolved_restart() -> Result<()> {
    run(ExecutionBackend::CpuCallbacks)
}

#[test]
#[ignore = "real AOEM integer backend; autonomous native rounds, not performance acceptance"]
fn native_progress_algebraic_three_heights_with_unresolved_restart() -> Result<()> {
    run(ExecutionBackend::AoemIntegerV7)
}

struct Fixture {
    paths: Vec<PathBuf>,
    keys: Vec<novovm_consensus::round_bft::test_vectors::FixtureSigningKey>,
    ids: Vec<Hash>,
    set: Arc<ValidatorSet>,
    genesis: ConsensusContext,
    peer_ids: Vec<String>,
    mesh: Mesh,
    services: Services,
    anchor: ParentPoint,
    drivers: Vec<RoundProgress>,
}
fn fixture(backend: ExecutionBackend) -> Result<Fixture> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/consensus-tests")
        .join(format!(
            "progress-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    let paths: Vec<_> = (0..4)
        .map(|i| directory.join(format!("node-{i}")))
        .collect();
    let keys: Vec<_> = (0..4).map(component_validator_key).collect();
    let members: Vec<_> = keys
        .iter()
        .map(|key| Validator::new(key.verifying_key().to_bytes(), 1))
        .collect::<Result<_>>()?;
    let ids: Vec<_> = members.iter().map(Validator::id).collect();
    let set = Arc::new(ValidatorSet::new_for_test(CHAIN, 1, 1, members)?);
    let genesis = ConsensusContext {
        chain_id: CHAIN,
        genesis_config_commitment: GENESIS,
        protocol_commitment: PROTOCOL,
        epoch: 1,
        validator_set_hash: set.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    };
    let peer_ids: Vec<_> = (0..keys.len())
        .map(|index| {
            novovm_network::product_overlay::peer_id_from_ed25519_public_key_v1(
                &component_transport_key(index).verifying_key().to_bytes(),
            )
        })
        .collect();
    let mesh = Mesh::new(
        &keys,
        MessageDomain {
            chain_id: CHAIN,
            genesis: GENESIS,
            protocol: PROTOCOL,
            validators: set.clone(),
        },
    )?;
    let mut services = Services::default();
    let mut anchor = None;
    for path in &paths {
        let root = initialize(path)?;
        anchor.get_or_insert(ParentPoint {
            height: 0,
            block_hash: [0; 32],
            state_root: root,
            receipt_batch_commitment: empty_root(),
            state_version: 0,
            decision_hash: [0; 32],
        });
        ensure!(
            anchor.unwrap().state_root == root,
            "different fixture genesis"
        );
        let mut config = pipeline_config(path)?;
        config.execution_backend = backend;
        services
            .0
            .push(CandidatePipeline::start(config, OpenMode::Existing)?);
    }
    let anchor = anchor.unwrap();
    let mut drivers = Vec::new();
    for (i, p) in services.0.iter().enumerate() {
        drivers.push(progress(
            open_journal(
                ValidatorJournal::open(p, genesis, anchor, set.clone(), keys[i].clone())?,
                p,
            )?,
            p,
            set.clone(),
        )?);
    }
    Ok(Fixture {
        paths,
        keys,
        ids,
        set,
        genesis,
        peer_ids,
        mesh,
        services,
        anchor,
        drivers,
    })
}

fn run(backend: ExecutionBackend) -> Result<()> {
    let Fixture {
        paths,
        keys,
        ids,
        set,
        genesis,
        peer_ids,
        mut mesh,
        mut services,
        anchor,
        mut drivers,
    } = fixture(backend)?;
    let leader = ids
        .iter()
        .position(|id| *id == set.leader(1, 0).unwrap())
        .unwrap();
    let delayed = (leader + 1) % 4;
    let mut outbound: Vec<VecDeque<(PreparedMessage, u8)>> =
        (0..4).map(|_| VecDeque::new()).collect();
    let mut proposal_restarted = false;
    let mut saw_missing_body = false;
    let mut votes = 0;
    let mut phase_signers = BTreeMap::<(u64, u8), std::collections::BTreeSet<Hash>>::new();
    let mut decisions = 0;
    let mut last_head = None;
    let mut delayed_candidate = None;
    for height in 1..=3 {
        let parent = drivers[0].parent();
        let mut execution = batch_context(parent.state_root);
        execution.height = height;
        execution.parent_height = parent.height;
        execution.parent_block_hash = parent.block_hash;
        execution.parent_state_root = parent.state_root;
        execution.parent_receipt_root = parent.receipt_batch_commitment;
        execution.parent_state_version = parent.state_version;
        execution.slot = height - 1;
        execution.timestamp_unix_ms += height;
        let raw = super::chain_integration::raw_at(height)?;
        let batch = Arc::new(ApflTransferBatch::from_raw(
            &raw,
            ApflLimits {
                transactions: 8,
                transaction_bytes: 1024,
                body_bytes: 8192,
            },
        )?);
        let body = Message::ApflBody {
            context: execution,
            batch,
        };
        // Only input delivery is fixture-scheduled. All control below comes
        // from the real driver, after journal ACK, over independent sessions.
        let bodies = mesh.broadcast(0, &body)?;
        let body_id = transport::body_id(&execution, &raw, codec())?;
        let mut tickets = Vec::new();
        for (i, body) in bodies.into_iter().enumerate() {
            let Message::ApflBody { context, batch } = body else {
                bail!("wrong body")
            };
            let Submission::Accepted(ticket) =
                services.0[i].try_submit(BatchRequest::from_apfl(batch, context, policy())?)?
            else {
                bail!("fixture full")
            };
            tickets.push(Some(ticket));
        }
        let deadline = Instant::now() + DEADLINE;
        loop {
            ensure!(
                Instant::now() < deadline,
                "autonomous height {height} timed out: {:?}",
                drivers
                    .iter()
                    .map(|d| (d.context().height, d.round(), d.step(), d.head()))
                    .collect::<Vec<_>>()
            );
            mesh.pump()?;
            for i in 0..4 {
                if let Some(ticket) = &mut tickets[i] {
                    if let Some(done) = ticket.try_take()? {
                        if backend == ExecutionBackend::AoemIntegerV7 {
                            ensure!(
                                done.observation.peak_callbacks == 0
                                    && done.observation.recomputed_transactions == 0
                                    && done.observation.gpu_waves == 1,
                                "algebraic fallback"
                            );
                        }
                        if height == 1 && i == delayed {
                            delayed_candidate = Some(done.candidate().clone());
                        } else {
                            ensure!(
                                drivers[i].register_candidate(body_id, done.candidate(), true)?,
                                "candidate budget"
                            );
                        }
                        tickets[i] = None;
                    }
                }
                if let Some(event) = mesh.channels[i].poll_receive(Instant::now(), 64)? {
                    match event {
                        ReceiveEvent::Message(received) => {
                            let peer = peer_ids
                                .iter()
                                .position(|p| p == &received.peer)
                                .context("unknown peer")?;
                            drivers[i].receive(ids[peer], &received.message)?;
                        }
                        ReceiveEvent::Rejected { error, .. } => {
                            bail!("native control rejected: {error}")
                        }
                    }
                }
                if height == 1 && i == delayed && drivers[i].missing_bodies().next().is_some() {
                    saw_missing_body = true;
                    ensure!(drivers[i].head().is_none(), "body hint granted publication");
                }
                if height == 1 && i == delayed && drivers.iter().any(|d| d.head().is_some()) {
                    if let Some(candidate) = delayed_candidate.take() {
                        ensure!(
                            drivers[i].register_candidate(body_id, &candidate, true)?,
                            "late candidate budget"
                        );
                    }
                }
                drivers[i].poll(&services.0[i], Instant::now())?;
                // Crash after durable local proposal, before its first send.
                // Reopen the same store and reexecute its actual saved body.
                if height == 1 && i == leader && !proposal_restarted {
                    if let Some(message @ Message::Proposal { .. }) =
                        drivers[i].pending_message().cloned()
                    {
                        let bytes = transport::encode(&message, codec())?;
                        let old = services.0.remove(i);
                        old.shutdown()?;
                        let mut config = pipeline_config(&paths[i])?;
                        config.execution_backend = backend;
                        services
                            .0
                            .insert(i, CandidatePipeline::start(config, OpenMode::Existing)?);
                        let p = &services.0[i];
                        drivers[i] = progress(
                            open_journal(
                                ValidatorJournal::open(
                                    p,
                                    genesis,
                                    anchor,
                                    set.clone(),
                                    keys[i].clone(),
                                )?,
                                p,
                            )?,
                            p,
                            set.clone(),
                        )?;
                        ensure!(
                            drivers[i].is_recovering(),
                            "unresolved proposal bypassed replay"
                        );
                        for _ in 0..5 {
                            drivers[i].poll(p, Instant::now() + Duration::from_secs(3600))?;
                            ensure!(
                                drivers[i].pending_message().is_none()
                                    && drivers[i].round() == 0
                                    && drivers[i].head().is_none(),
                                "unreexecuted content authorized action"
                            );
                        }
                        let locators = drivers[i]
                            .recovery_candidates()
                            .copied()
                            .collect::<Vec<_>>();
                        ensure!(locators.len() == 1, "unexpected replay closure");
                        while drivers[i].pending_message().is_none() {
                            ensure!(Instant::now() < deadline, "cold replay deadline");
                            drivers[i].poll_recovery(p, &policy(), codec())?;
                            drivers[i].poll(p, Instant::now())?;
                            mesh.pump()?;
                            std::thread::yield_now();
                        }
                        ensure!(
                            transport::encode(
                                drivers[i]
                                    .pending_message()
                                    .context("missing exact replay")?,
                                codec()
                            )? == bytes,
                            "cold proposal re-signed/relabelled"
                        );
                        proposal_restarted = true;
                        let restored = drivers[i]
                            .candidate_body(body_id)
                            .context("replayed body missing")?;
                        ensure!(
                            restored.as_ref() == &body
                                && transport::candidate_body_id(&restored, codec())? == body_id,
                            "recovered body changed original APFL fields or exact identity"
                        );
                    }
                }
                // Bound the consumer queue. Holding the driver's one outbox
                // must prevent further transitions, not discard signed work.
                if outbound[i].len() < 8 {
                    if let Some(message) = drivers[i].pending_message().cloned() {
                        let head = drivers[i].head();
                        drivers[i].poll(&services.0[i], Instant::now())?;
                        ensure!(
                            drivers[i].pending_message() == Some(&message)
                                && drivers[i].head() == head,
                            "outbox backpressure lost signed work"
                        );
                        match &message {
                            Message::Vote(vote) => {
                                votes += 1;
                                if vote.value.is_some() {
                                    let phase = if vote.phase == Phase::Prevote { 0 } else { 1 };
                                    phase_signers
                                        .entry((vote.context.height, phase))
                                        .or_default()
                                        .insert(vote.validator_id);
                                }
                            }
                            Message::Decision { certificate, .. } => {
                                ensure!(
                                    certificate.verify(&set)?.value().is_some(),
                                    "bad generated QC"
                                );
                                decisions += 1;
                            }
                            _ => {}
                        }
                        let prepared = mesh.channels[i].prepare(&message)?;
                        drivers[i].take_message();
                        outbound[i].push_back((prepared, 0));
                    }
                }
                if let Some((prepared, sent)) = outbound[i].front_mut() {
                    for peer in 0..3 {
                        if *sent & (1 << peer) == 0
                            && mesh.channels[i].try_send(peer, prepared, Instant::now())?
                        {
                            *sent |= 1 << peer;
                        }
                    }
                    if *sent == 7 {
                        outbound[i].pop_front();
                    }
                }
            }
            if drivers.iter().all(|d| d.context().height == height + 1)
                && outbound.iter().all(VecDeque::is_empty)
            {
                break;
            }
            std::thread::yield_now();
        }
        let head = drivers[0].head().context("no published head")?;
        ensure!(
            head.height == height
                && head.state_version == height * 2
                && drivers
                    .iter()
                    .all(|d| d.head() == Some(head) && d.parent() == head),
            "head disagreement"
        );
        last_head = Some(head);
        // Drain residual delivered control before the next body-only fixture.
        for _ in 0..200 {
            mesh.pump()?;
            for (i, channel) in mesh.channels.iter_mut().enumerate() {
                if let Some(ReceiveEvent::Message(received)) =
                    channel.poll_receive(Instant::now(), 64)?
                {
                    let peer = peer_ids.iter().position(|p| p == &received.peer).unwrap();
                    drivers[i].receive(ids[peer], &received.message)?;
                }
            }
        }
    }
    // A node receiving an already-qualified decision after local execution
    // need not manufacture extra prevote/precommit votes. Require actual
    // three-distinct-signer phase quorums, not four votes from every node.
    ensure!(
        proposal_restarted
            && saw_missing_body
            && decisions == 12
            && (1..=3).all(|h| (0..=1).all(|phase| phase_signers
                .get(&(h, phase))
                .is_some_and(|signers| signers.len() >= 3))),
        "autonomous fault/phase coverage missing: votes {votes}, decisions {decisions}"
    );
    let head = last_head.unwrap();
    drop(drivers);
    services.shutdown()?;
    for (i, path) in paths.iter().enumerate() {
        let mut config = pipeline_config(path)?;
        config.execution_backend = backend;
        services
            .0
            .push(CandidatePipeline::start(config, OpenMode::Existing)?);
        let p = &services.0[i];
        let journal = open_journal(
            ValidatorJournal::open(p, genesis, anchor, set.clone(), keys[i].clone())?,
            p,
        )?;
        ensure!(
            journal.head() == Some(head) && journal.context().height == 4,
            "cold head recovery"
        );
        let mut read = ArchiveRead::new(3, head, journal.context(), set.clone())?;
        let deadline = Instant::now() + DEADLINE;
        let block = loop {
            ensure!(Instant::now() < deadline, "archive timeout");
            if let Some(done) = read.poll(p)? {
                break done.context("archive missing")?;
            }
            std::thread::yield_now();
        };
        ensure!(
            block.stored().state_root() == head.state_root
                && block.stored().raw_transactions() == super::chain_integration::raw_at(3)?,
            "cold body/roots changed"
        );
        let recipient =
            super::chain_integration::value(p, head.state_root, balance_key(&account(2)))?;
        ensure!(
            u128::from_le_bytes(recipient.try_into().unwrap()) == 350,
            "autonomous failed transfer credited recipient"
        );
        for raw in super::chain_integration::raw_at(3)? {
            let signed = crate::native_pipeline::ingress::authentication::authenticate_transfer_v3(
                &raw, CHAIN, 1024,
            )?;
            let key = crate::native_pipeline::business::nov_transfer_batch::nonce_key(
                &signed.nonce_identity(),
            );
            let nonce = super::chain_integration::value(p, head.state_root, key)?;
            ensure!(
                u64::from_le_bytes(nonce.try_into().unwrap()) == 3,
                "autonomous business failure nonce changed after restart"
            );
        }
    }
    services.shutdown()?;
    Ok(())
}
