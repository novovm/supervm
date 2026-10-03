//! Same-process integration of four independent AOEM stores with real WSS,
//! authenticated peer E2E, bounded fragments, and the development Host codec.
//! This deterministic fixture driver is NOT a production node/pacemaker, four
//! independent main processes, public-network evidence, or a TPS measurement.

use super::*;
use crate::native_pipeline::consensus::collector::{CollectorLimits, VoteCollector};
use crate::native_pipeline::consensus::transport::{self, DecodeLimits, Message};
use crate::native_pipeline::consensus::wire::VerifiedProposal;
use novovm_network::duplex::fragments::{FragmentAdmission, Reassembler, ReassemblyLimits};
use novovm_network::duplex::peer_id_from_ed25519_public_key_v1;
use novovm_network::duplex::product_relay_client::{
    ProductRelayClientConfigV1, ProductRelayTlsTrustV1,
};
use novovm_network::duplex::product_relay_daemon::{
    run_product_relay_daemon_with_shutdown_v1, ProductRelayDaemonConfigV1,
    ProductRelayDaemonReportV1,
};
use novovm_network::duplex::worker::{
    NetworkWorker, NetworkWorkerConfig, SendAdmission, WorkerLimits,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

fn codec_limits() -> DecodeLimits {
    DecodeLimits {
        transactions: 8,
        transaction_bytes: 1024,
        body_bytes: 8192,
        message_bytes: 512 * 1024,
    }
}

fn peer_id(index: usize) -> String {
    peer_id_from_ed25519_public_key_v1(&validator_key(index).verifying_key().to_bytes())
}

pub(super) struct Relay {
    pub(super) endpoint: String,
    pub(super) certificate: PathBuf,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<()>>>,
}

impl Relay {
    pub(super) fn start(directory: &Path) -> Result<Self> {
        fs::create_dir(directory)?;
        let certificate =
            rcgen::generate_simple_self_signed(vec!["localhost".into(), "127.0.0.1".into()])?;
        let cert_path = directory.join("cert.pem");
        let key_path = directory.join("tls-key.pem");
        let identity_path = directory.join("identity.hex");
        let report_path = directory.join("report.json");
        fs::write(&cert_path, certificate.serialize_pem()?)?;
        fs::write(&key_path, certificate.serialize_private_key_pem())?;
        fs::write(&identity_path, "5b".repeat(32))?;
        let config: ProductRelayDaemonConfigV1 = serde_json::from_value(serde_json::json!({
            "bind_addr": "127.0.0.1:0",
            "tls_cert_path": cert_path,
            "tls_key_path": key_path,
            "relay_identity_key_path": identity_path,
            "report_path": report_path,
            "report_interval_ms": 10,
            "max_connections": 16,
            "max_sessions": 8,
        }))?;
        let stop = Arc::new(AtomicBool::new(false));
        let daemon_stop = stop.clone();
        let worker =
            thread::spawn(move || run_product_relay_daemon_with_shutdown_v1(config, daemon_stop));
        let mut relay = Self {
            endpoint: String::new(),
            certificate: cert_path,
            stop,
            thread: Some(worker),
        };
        let deadline = Instant::now() + DEADLINE;
        loop {
            ensure!(Instant::now() < deadline, "real relay startup timed out");
            if let Some(report) = fs::read(&report_path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<ProductRelayDaemonReportV1>(&bytes).ok())
            {
                let address: std::net::SocketAddr = report.listen_addr.parse()?;
                relay.endpoint = format!("wss://127.0.0.1:{}/novovm", address.port());
                return Ok(relay);
            }
            thread::yield_now();
        }
    }

    pub(super) fn shutdown(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.thread.take() {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("relay thread panicked"))??;
        }
        Ok(())
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

// Only this test's assembly/ingress owner flattens/decodes complete messages.
// Nothing here adds blocking network or whole-body work to the real pipeline.
struct Mesh {
    workers: Vec<Option<NetworkWorker>>,
    assemblers: Vec<Reassembler>,
    inboxes: Vec<VecDeque<(String, Message)>>,
    received: [usize; 4],
    domain: Hash,
}

impl Mesh {
    fn new() -> Result<Self> {
        let domain = transport::fragment_domain(CHAIN, GENESIS, PROTOCOL);
        let assemblers = (0..4)
            .map(|local| {
                Reassembler::new(
                    domain,
                    (0..4).filter(|peer| *peer != local).map(peer_id).collect(),
                    ReassemblyLimits {
                        max_message_bytes: codec_limits().message_bytes,
                        messages: 32,
                        bytes: 4 * 1024 * 1024,
                        peer_messages: 16,
                        peer_bytes: 2 * 1024 * 1024,
                        ttl: DEADLINE,
                    },
                )
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            workers: (0..4).map(|_| None).collect(),
            assemblers,
            inboxes: (0..4).map(|_| VecDeque::new()).collect(),
            received: [0; 4],
            domain,
        })
    }

    fn start(&mut self, local: usize, relay: &Relay) -> Result<()> {
        ensure!(self.workers[local].is_none(), "worker already started");
        let relay_key = SigningKey::from_bytes(&[91; 32]);
        self.workers[local] = Some(NetworkWorker::start(
            NetworkWorkerConfig {
                chain_id: CHAIN,
                relay: ProductRelayClientConfigV1 {
                    endpoint: relay.endpoint.clone(),
                    expected_relay_peer_id: peer_id_from_ed25519_public_key_v1(
                        &relay_key.verifying_key().to_bytes(),
                    ),
                    connect_timeout_ms: 2_000,
                    read_timeout_ms: 10,
                    tls_trust: ProductRelayTlsTrustV1::ExplicitCa {
                        certificate_path: relay.certificate.clone(),
                    },
                },
                peers: (0..4).filter(|peer| *peer != local).map(peer_id).collect(),
                limits: WorkerLimits::default(),
                handshake_timeout_ms: 5_000,
                reconnect_delay_ms: 50,
                heartbeat_interval_ms: 1_000,
                queue_ttl_ms: 60_000,
            },
            validator_key(local),
        )?);
        Ok(())
    }

    fn ready(&mut self, active: &[usize]) -> Result<()> {
        let deadline = Instant::now() + DEADLINE;
        loop {
            ensure!(
                Instant::now() < deadline,
                "peer E2E readiness timed out: {:?}",
                self.workers
                    .iter()
                    .map(|w| w.as_ref().map(NetworkWorker::status))
                    .collect::<Vec<_>>()
            );
            if active.iter().all(|local| {
                self.workers[*local].as_ref().is_some_and(|worker| {
                    worker.status().is_ok_and(|status| {
                        active
                            .iter()
                            .filter(|peer| *peer != local)
                            .all(|peer| status.active_peers.contains(&peer_id(*peer)))
                    })
                })
            }) {
                return Ok(());
            }
            self.pump()?;
            thread::yield_now();
        }
    }

    fn pump(&mut self) -> Result<()> {
        let now = Instant::now();
        for local in 0..4 {
            let Some(worker) = &self.workers[local] else {
                continue;
            };
            for _ in 0..8 {
                let Some(inbound) = worker.try_recv()? else {
                    break;
                };
                ensure!(
                    self.assemblers[local].push(&inbound.peer_id, &inbound.bytes, now)?
                        != FragmentAdmission::Backpressure,
                    "fixture exceeded bounded fragment admission"
                );
            }
            if let Some(message) = self.assemblers[local].poll_complete(now, 1)? {
                ensure!(
                    self.inboxes[local].len() < 64,
                    "fixture decoded inbox exceeded budget"
                );
                let decoded = transport::decode(&message.chunks.concat(), codec_limits())?;
                self.inboxes[local].push_back((message.peer, decoded));
                self.received[local] += 1;
            }
        }
        Ok(())
    }

    fn send(&mut self, source: usize, target: usize, message: &Message) -> Result<()> {
        ensure!(
            source != target,
            "fixture cannot bypass network with local send"
        );
        let outgoing = transport::prepare_message(self.domain, message, codec_limits())?;
        let deadline = Instant::now() + DEADLINE;
        for index in 0..outgoing.frame_count() {
            let mut bytes = outgoing.frame(index)?;
            loop {
                ensure!(
                    Instant::now() < deadline,
                    "network send admission timed out"
                );
                match self.workers[source]
                    .as_ref()
                    .context("source worker absent")?
                    .try_send(peer_id(target), bytes)?
                {
                    SendAdmission::Accepted => break,
                    SendAdmission::Backpressure(returned) => bytes = returned.bytes,
                    SendAdmission::Rejected { reason, .. } => {
                        bail!("fixture network send rejected: {reason:?}")
                    }
                }
                self.pump()?;
                thread::yield_now();
            }
        }
        // This result proves ONLY local queue admission. Callers separately
        // wait for decoded receipt, local AOEM execution, votes and durable ACK.
        Ok(())
    }

    fn next(&mut self, local: usize) -> Result<(String, Message)> {
        let deadline = Instant::now() + DEADLINE;
        loop {
            if let Some(message) = self.inboxes[local].pop_front() {
                return Ok(message);
            }
            ensure!(
                Instant::now() < deadline,
                "node {local} network receive timed out"
            );
            self.pump()?;
            thread::yield_now();
        }
    }

    fn shutdown(&mut self) -> Result<()> {
        for worker in self.workers.iter_mut().flatten() {
            worker.shutdown()?;
        }
        Ok(())
    }
}

struct Local {
    journal: ValidatorJournal,
    collector: VoteCollector,
    candidate: Option<DurableCandidate>,
    proposal: Option<VerifiedProposal>,
    body: Option<Message>,
    body_id: Option<Hash>,
    votes: Vec<wire::Vote>,
}

fn execute_received_body(
    mesh: &mut Mesh,
    local: usize,
    source: usize,
    node: &mut Local,
    pipeline: &CandidatePipeline,
    expected_context: BatchContext,
) -> Result<()> {
    let (peer, message) = mesh.next(local)?;
    ensure!(peer == peer_id(source), "body came from unexpected peer");
    let Message::Body {
        context,
        raw_transactions,
    } = &message
    else {
        bail!("expected network body")
    };
    ensure!(
        *context == expected_context,
        "network body changed locally pinned execution context"
    );
    ensure!(
        *raw_transactions == raw()?,
        "network changed original signed transaction bytes"
    );
    let id = transport::body_id(context, raw_transactions, codec_limits())?;
    let mut request = BatchRequest::new(raw_transactions.clone(), *context, policy())?;
    let deadline = Instant::now() + DEADLINE;
    let mut ticket = loop {
        ensure!(
            Instant::now() < deadline,
            "received body admission timed out"
        );
        match pipeline.try_submit(request)? {
            Submission::Accepted(ticket) => break ticket,
            Submission::Backpressured(returned) => request = returned,
        }
        mesh.pump()?;
        thread::yield_now();
    };
    loop {
        ensure!(
            Instant::now() < deadline,
            "received body real AOEM execution timed out"
        );
        if let Some(done) = ticket.try_take()? {
            ensure!(
                done.observation.peak_callbacks > 0 && !done.persisted.already_present,
                "node {local} did not perform fresh real AOEM work"
            );
            ensure!(
                done.candidate().packet().transaction_count() == 2,
                "wrong executed transaction count"
            );
            node.candidate = Some(done.candidate().clone());
            node.body = Some(message);
            node.body_id = Some(id);
            return Ok(());
        }
        mesh.pump()?;
        thread::yield_now();
    }
}

fn receive_proposal(
    mesh: &mut Mesh,
    local: usize,
    leader: usize,
    node: &mut Local,
    set: &ValidatorSet,
) -> Result<()> {
    let (peer, message) = mesh.next(local)?;
    ensure!(
        peer == peer_id(leader),
        "proposal carrier differs from configured leader"
    );
    let Message::Proposal {
        proposal,
        valid_quorum,
        body_id,
    } = message
    else {
        bail!("expected network proposal")
    };
    ensure!(
        valid_quorum.is_none() && Some(body_id) == node.body_id,
        "wrong proposal body reference/valid-round evidence"
    );
    ensure!(
        proposal.context == node.journal.context(),
        "proposal changed local consensus context"
    );
    node.proposal = Some(proposal.verify(set)?);
    Ok(())
}

fn sign_vote(
    local: usize,
    node: &mut Local,
    pipeline: &CandidatePipeline,
    set: &ValidatorSet,
    phase: Phase,
) -> Result<()> {
    let proposal = node.proposal.as_ref().context("local proposal missing")?;
    let candidate = node.candidate.as_ref().context("local execution missing")?;
    match phase {
        Phase::Prevote => node.journal.accept_proposal(proposal, candidate, None)?,
        Phase::Precommit => {
            let qc = node
                .collector
                .quorum(0, Phase::Prevote, Some(proposal.proposal().value))?
                .context("own collector lacks real prevote quorum")?;
            node.journal
                .observe_prevotes(&qc, Some((proposal, candidate)))?;
        }
    }
    let vote = wire_vote(
        complete(&mut node.journal, pipeline)?,
        set,
        phase,
        proposal.proposal().value,
    )?;
    ensure!(
        vote.validator_id
            == Validator::new(validator_key(local).verifying_key().to_bytes(), 1)?.id(),
        "node emitted another validator's vote"
    );
    node.collector.insert(&vote)?;
    node.votes.push(vote);
    Ok(())
}

fn send_own_votes(
    mesh: &mut Mesh,
    nodes: &[Local],
    sources: &[usize],
    targets: &[usize],
    phase: Phase,
) -> Result<()> {
    for source in sources {
        let vote = nodes[*source]
            .votes
            .iter()
            .find(|vote| vote.phase == phase)
            .context("source has not durably emitted this phase")?;
        for target in targets.iter().filter(|target| *target != source) {
            mesh.send(*source, *target, &Message::Vote(vote.clone()))?;
        }
    }
    Ok(())
}

fn collect_votes(
    mesh: &mut Mesh,
    nodes: &mut [Local],
    targets: &[usize],
    phase: Phase,
    weight: u64,
) -> Result<()> {
    let deadline = Instant::now() + DEADLINE;
    loop {
        mesh.pump()?;
        for local in targets {
            while let Some((peer, message)) = mesh.inboxes[*local].pop_front() {
                let Message::Vote(vote) = message else {
                    bail!("expected actual peer vote")
                };
                let source = (0..4)
                    .find(|index| peer_id(*index) == peer)
                    .context("unconfigured vote carrier")?;
                ensure!(
                    vote.validator_id
                        == Validator::new(validator_key(source).verifying_key().to_bytes(), 1)?
                            .id(),
                    "carrier attempted to impersonate a validator"
                );
                nodes[*local].collector.insert(&vote)?;
            }
        }
        if targets
            .iter()
            .all(|local| nodes[*local].collector.phase_weight(0, phase) == weight)
        {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "actual peer vote receipt timed out for {phase:?}"
        );
        thread::yield_now();
    }
}

fn publish(node: &mut Local, pipeline: &CandidatePipeline) -> Result<Message> {
    let proposal = node.proposal.as_ref().context("local proposal missing")?;
    let candidate = node.candidate.as_ref().context("local execution missing")?;
    ensure!(
        node.votes.iter().any(|vote| vote.phase == Phase::Precommit),
        "publication preceded durable local precommit"
    );
    let qc = node
        .collector
        .quorum(0, Phase::Precommit, Some(proposal.proposal().value))?
        .context("own collector lacks actual precommit quorum")?;
    node.journal.observe_decision(proposal, candidate, &qc)?;
    ensure!(
        node.journal.head().is_none(),
        "head escaped before durable decision ACK"
    );
    let DurableMessage::Decision {
        proposal,
        certificate,
    } = complete(&mut node.journal, pipeline)?
    else {
        bail!("expected locally durable decision");
    };
    Ok(Message::Decision {
        proposal,
        certificate,
        body_id: node.body_id.unwrap(),
    })
}

fn receive_decision(
    mesh: &mut Mesh,
    local: usize,
    source: usize,
    node: &Local,
    set: &ValidatorSet,
) -> Result<()> {
    let before = message_bytes(node.journal.last_durable_message())?;
    let (peer, message) = mesh.next(local)?;
    ensure!(peer == peer_id(source), "unexpected decision carrier");
    let Message::Decision {
        proposal,
        certificate,
        body_id,
    } = message
    else {
        bail!("expected network decision")
    };
    let proposal = proposal.verify(set)?;
    let qc = certificate.verify(set)?;
    ensure!(
        Some(body_id) == node.body_id
            && qc.phase() == Phase::Precommit
            && qc.value() == node.journal.decided()
            && qc.context() == &node.journal.context()
            && proposal.proposal().value == node.journal.decided().unwrap(),
        "network decision disagrees with local publication"
    );
    ensure!(
        before == message_bytes(node.journal.last_durable_message())?,
        "relay decision replaced local durable outbox"
    );
    Ok(())
}

fn recipient_balance(pipeline: &CandidatePipeline, root: Hash) -> Result<u128> {
    let deadline = Instant::now() + DEADLINE;
    let mut ticket = loop {
        ensure!(
            Instant::now() < deadline,
            "balance read admission timed out"
        );
        if let Some(ticket) = pipeline.try_read_value(root, balance_key(&account(2)))? {
            break ticket;
        }
        thread::yield_now();
    };
    loop {
        ensure!(
            Instant::now() < deadline,
            "balance read completion timed out"
        );
        if let Some(value) = ticket.try_take()? {
            return Ok(u128::from_le_bytes(
                value
                    .context("recipient balance absent")?
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("bad balance encoding"))?,
            ));
        }
        thread::yield_now();
    }
}

#[test]
#[ignore = "requires explicit real AOEM library; four same-process stores over real WSS, not four main processes or TPS"]
fn real_wss_four_local_validators_execute_received_body_collect_own_votes_and_reopen() -> Result<()>
{
    let _ = library()?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    // Validate the canonical location, but pass the ordinary manifest-derived
    // pathname to the C++ RocksDB provider: Windows verbatim \\?\ paths are
    // not accepted by that provider when it appends its internal filenames.
    let artifacts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/runtime-rebuild");
    ensure!(
        artifacts.canonicalize()?.starts_with(&repository),
        "test artifact directory escaped repository"
    );
    let directory = artifacts.join(format!(
        "network-consensus-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir(&directory)?;
    let paths: Vec<_> = (0..4)
        .map(|index| directory.join(format!("validator-{index}")))
        .collect();
    let set = Arc::new(ValidatorSet::new(
        CHAIN,
        1,
        1,
        (0..4)
            .map(|i| Validator::new(validator_key(i).verifying_key().to_bytes(), 1))
            .collect::<Result<_>>()?,
    )?);
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
    let mut services = Services::default();
    let mut roots = Vec::new();
    for path in &paths {
        roots.push(initialize(path)?);
        services.0.push(CandidatePipeline::start(
            pipeline_config(path)?,
            OpenMode::Existing,
        )?);
    }
    ensure!(
        roots.iter().all(|root| *root == roots[0]),
        "genesis state roots disagree"
    );
    let parent = ParentPoint {
        height: 0,
        block_hash: [0; 32],
        state_root: roots[0],
        receipt_batch_commitment: empty_root(),
        state_version: 0,
        decision_hash: [0; 32],
    };
    let execution = batch_context(roots[0]);
    let mut nodes = Vec::new();
    for (index, pipeline) in services.0.iter().enumerate() {
        nodes.push(Local {
            journal: open_journal(
                ValidatorJournal::open(
                    pipeline,
                    context,
                    parent,
                    set.clone(),
                    validator_key(index),
                )?,
                pipeline,
            )?,
            collector: VoteCollector::new(
                context,
                set.clone(),
                0,
                CollectorLimits {
                    max_retained_rounds: 8,
                    max_future_round_span: 0,
                    max_votes: 64,
                },
            )?,
            candidate: None,
            proposal: None,
            body: None,
            body_id: None,
            votes: Vec::new(),
        });
    }
    let leader = (0..4)
        .find(|index| {
            Validator::new(validator_key(*index).verifying_key().to_bytes(), 1)
                .unwrap()
                .id()
                == set.leader(1, 0).unwrap()
        })
        .unwrap();
    let order: Vec<_> = std::iter::once(leader)
        .chain((0..4).filter(|index| *index != leader))
        .collect();
    let (second, third, fourth) = (order[1], order[2], order[3]);
    let mut relay = Relay::start(&directory.join("relay"))?;
    let mut mesh = Mesh::new()?;
    for index in &order[..2] {
        mesh.start(*index, &relay)?;
    }
    mesh.ready(&order[..2])?;

    // The ingress origin does not execute its local assembly copy. Even the
    // leader executes only bytes received through another actual worker.
    mesh.send(
        second,
        leader,
        &Message::Body {
            context: execution,
            raw_transactions: raw()?,
        },
    )?;
    execute_received_body(
        &mut mesh,
        leader,
        second,
        &mut nodes[leader],
        &services.0[leader],
        execution,
    )?;
    {
        let local = &mut nodes[leader];
        local
            .journal
            .propose(local.candidate.as_ref().unwrap(), None)?;
        let DurableMessage::Proposal(proposal) = complete(&mut local.journal, &services.0[leader])?
        else {
            bail!("leader proposal missing")
        };
        local.proposal = Some(proposal.verify(&set)?);
    }
    let proposal_message = Message::Proposal {
        proposal: nodes[leader].proposal.as_ref().unwrap().proposal().clone(),
        valid_quorum: None,
        body_id: nodes[leader].body_id.unwrap(),
    };
    mesh.send(leader, second, nodes[leader].body.as_ref().unwrap())?;
    mesh.send(leader, second, &proposal_message)?;
    execute_received_body(
        &mut mesh,
        second,
        leader,
        &mut nodes[second],
        &services.0[second],
        execution,
    )?;
    receive_proposal(&mut mesh, second, leader, &mut nodes[second], &set)?;
    for index in &order[..2] {
        sign_vote(
            *index,
            &mut nodes[*index],
            &services.0[*index],
            &set,
            Phase::Prevote,
        )?;
    }
    send_own_votes(&mut mesh, &nodes, &order[..2], &order[..2], Phase::Prevote)?;
    collect_votes(&mut mesh, &mut nodes, &order[..2], Phase::Prevote, 2)?;
    let block = nodes[leader].proposal.as_ref().unwrap().proposal().value;
    for index in &order[..2] {
        let node = &nodes[*index];
        ensure!(
            node.collector.vote_count() == 2
                && node
                    .collector
                    .quorum(0, Phase::Prevote, Some(block))?
                    .is_none()
                && node
                    .collector
                    .quorum(0, Phase::Precommit, Some(block))?
                    .is_none()
                && node.journal.head().is_none()
                && node.journal.decided().is_none(),
            "two real received votes incorrectly confirmed"
        );
        ensure!(
            read_metadata(
                &services.0[*index],
                vec![MetaKey::ChainHead, MetaKey::ChainBlock { height: 1 }]
            )?
            .values
            .iter()
            .all(Option::is_none),
            "2/4 published durable head/archive"
        );
    }

    mesh.start(third, &relay)?;
    mesh.ready(&order[..3])?;
    mesh.send(leader, third, nodes[leader].body.as_ref().unwrap())?;
    mesh.send(leader, third, &proposal_message)?;
    execute_received_body(
        &mut mesh,
        third,
        leader,
        &mut nodes[third],
        &services.0[third],
        execution,
    )?;
    receive_proposal(&mut mesh, third, leader, &mut nodes[third], &set)?;
    sign_vote(
        third,
        &mut nodes[third],
        &services.0[third],
        &set,
        Phase::Prevote,
    )?;
    send_own_votes(&mut mesh, &nodes, &order[..2], &[third], Phase::Prevote)?;
    send_own_votes(&mut mesh, &nodes, &[third], &order[..2], Phase::Prevote)?;
    collect_votes(&mut mesh, &mut nodes, &order[..3], Phase::Prevote, 3)?;
    for index in &order[..3] {
        sign_vote(
            *index,
            &mut nodes[*index],
            &services.0[*index],
            &set,
            Phase::Precommit,
        )?;
    }
    send_own_votes(
        &mut mesh,
        &nodes,
        &order[..3],
        &order[..3],
        Phase::Precommit,
    )?;
    collect_votes(&mut mesh, &mut nodes, &order[..3], Phase::Precommit, 3)?;
    let mut decisions = BTreeMap::new();
    for index in &order[..3] {
        decisions.insert(*index, publish(&mut nodes[*index], &services.0[*index])?);
    }
    let head = nodes[leader]
        .journal
        .head()
        .context("3/4 decision head missing")?;
    ensure!(
        nodes[fourth].journal.head().is_none() && nodes[fourth].candidate.is_none(),
        "offline fourth executed or decided without body"
    );

    mesh.start(fourth, &relay)?;
    mesh.ready(&order)?;
    mesh.send(leader, fourth, nodes[leader].body.as_ref().unwrap())?;
    mesh.send(leader, fourth, &proposal_message)?;
    execute_received_body(
        &mut mesh,
        fourth,
        leader,
        &mut nodes[fourth],
        &services.0[fourth],
        execution,
    )?;
    receive_proposal(&mut mesh, fourth, leader, &mut nodes[fourth], &set)?;
    sign_vote(
        fourth,
        &mut nodes[fourth],
        &services.0[fourth],
        &set,
        Phase::Prevote,
    )?;
    send_own_votes(&mut mesh, &nodes, &order[..3], &[fourth], Phase::Prevote)?;
    send_own_votes(&mut mesh, &nodes, &[fourth], &order[..3], Phase::Prevote)?;
    collect_votes(&mut mesh, &mut nodes, &order, Phase::Prevote, 4)?;
    sign_vote(
        fourth,
        &mut nodes[fourth],
        &services.0[fourth],
        &set,
        Phase::Precommit,
    )?;
    send_own_votes(&mut mesh, &nodes, &order[..3], &[fourth], Phase::Precommit)?;
    send_own_votes(&mut mesh, &nodes, &[fourth], &order[..3], Phase::Precommit)?;
    collect_votes(&mut mesh, &mut nodes, &order, Phase::Precommit, 4)?;
    decisions.insert(fourth, publish(&mut nodes[fourth], &services.0[fourth])?);
    // Also carry actual durable certificates through the codec/network. They
    // do not replace each receiver's own collector, execution or publication.
    mesh.send(leader, fourth, &decisions[&leader])?;
    receive_decision(&mut mesh, fourth, leader, &nodes[fourth], &set)?;
    for target in &order[..3] {
        mesh.send(fourth, *target, &decisions[&fourth])?;
        receive_decision(&mut mesh, *target, fourth, &nodes[*target], &set)?;
    }
    let mut exact_outboxes = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        ensure!(
            node.journal.head() == Some(head) && node.journal.decided() == Some(block),
            "independent local heads disagree"
        );
        ensure!(
            node.candidate.as_ref().unwrap().packet().records()
                == nodes[leader].candidate.as_ref().unwrap().packet().records(),
            "network body executions disagree"
        );
        ensure!(
            mesh.received[index] > 0
                && recipient_balance(&services.0[index], head.state_root)? == 150,
            "missing real receipt/execution evidence"
        );
        exact_outboxes.push(message_bytes(node.journal.last_durable_message())?);
    }
    mesh.shutdown()?;
    relay.shutdown()?;
    drop(nodes);
    services.shutdown()?;
    for (index, path) in paths.iter().enumerate() {
        services.0.push(CandidatePipeline::start(
            pipeline_config(path)?,
            OpenMode::Existing,
        )?);
        let pipeline = &services.0[index];
        let journal = open_journal(
            ValidatorJournal::open(pipeline, context, parent, set.clone(), validator_key(index))?,
            pipeline,
        )?;
        ensure!(
            journal.head() == Some(head)
                && journal.decided() == Some(block)
                && message_bytes(journal.last_durable_message())? == exact_outboxes[index]
                && recipient_balance(pipeline, head.state_root)? == 150,
            "cold independent recovery lost head/outbox/state"
        );
    }
    services.shutdown()?;
    eprintln!("real WSS same-process four-store consensus passed: 2/4 no head, 3/4 decided, fourth caught up; artifacts={}", directory.display());
    Ok(())
}
