//! Real owner-thread/worker-thread handoff inside the finalized-chain fixture.
//! The pause proves scheduler responsiveness, not execution throughput. The
//! stale-round token is injected in memory. A separate signed timeout travels
//! through the real WSS ingress; one vote is not a quorum or finality claim.

use super::*;
use crate::native_fresh_rpc::handle_fresh_rpc;
use crate::product_mainline_overlay::ProductMainlineOverlayEventV1 as Event;
use crate::tx_ingress::fresh_pool::PendingTransaction;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::SyncSender;

struct ReleaseWorker(Option<SyncSender<()>>);

impl Drop for ReleaseWorker {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.try_send(());
        }
    }
}

fn rpc(
    lifecycle: &mut FreshChainLifecycleV1,
    method: &str,
    argument: &[u8],
) -> Result<serde_json::Value> {
    rpc_params(
        lifecycle,
        method,
        serde_json::json!([crate::native_block_seal::hex_v1(argument)]),
    )
}

fn rpc_params(
    lifecycle: &mut FreshChainLifecycleV1,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    let result = handle_fresh_rpc(
        serde_json::json!({
            "jsonrpc":"2.0", "id":1, "method":method, "params":params,
        }),
        lifecycle,
    );
    if let Some(error) = result.get("error") {
        bail!("pipeline fixture RPC failed: {error}");
    }
    result
        .get("result")
        .cloned()
        .context("pipeline fixture RPC result missing")
}

fn http_request(
    address: std::net::SocketAddr,
    method: &str,
    params: serde_json::Value,
) -> Result<TcpStream> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "jsonrpc":"2.0", "id":7, "method":method, "params":params,
    }))?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    write!(
        stream,
        "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
        bytes.len()
    )?;
    stream.write_all(&bytes)?;
    stream.set_nonblocking(true)?;
    Ok(stream)
}

fn read_available(stream: &mut TcpStream, bytes: &mut Vec<u8>) -> Result<()> {
    let mut buffer = [0; 4096];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(count) => {
                bytes.extend_from_slice(&buffer[..count]);
                if bytes.len() > 64 * 1024 {
                    bail!("pipeline fixture HTTP response exceeded bounded buffer");
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
}

impl FreshChainLifecycleV1 {
    /// Receives three already signed test transactions, not unverified expected
    /// roots. Only the first enters the worker's immutable candidate input.
    pub(crate) fn exercise_candidate_pipeline_for_test_v1(
        path: &Path,
        params: &serde_json::Value,
        raws: [Vec<u8>; 3],
        timestamp: u64,
    ) -> Result<()> {
        let config_path = path.with_extension("fresh-service-3").join("service.json");
        let ledger = crate::tx_ingress::native_persistence_write_paths_v1(params)
            .into_iter()
            .find(|(label, _)| *label == "native block ledger")
            .context("pipeline fixture ledger path missing")?
            .1;
        let chain = novovm_protocol::decode_nov_native_tx_wire_v1(&raws[0])?.chain_id;
        let mut config = NovNativeSealServiceConfigV1::load(&config_path, chain)?;
        let pin = config
            .fresh_genesis_config_commitment
            .context("pipeline genesis missing")?;
        let leader_id = config.authority.scheduled_leader_v1(4, 0)?;
        config.local_validator_id = leader_id;
        config.seal_store_path = path.with_extension("genesis-seal-0");
        config.follow_finalized_tip = true;
        config.receive_successors = true;
        config.propose_successors = false;
        config.transaction_ingress_enabled = false;
        config.proposal_collect = Duration::ZERO;
        let pool_path = path.with_extension("pipeline-liveness.txpool");
        let entries = raws
            .iter()
            .map(|raw| PendingTransaction::authenticate(raw.clone(), chain, params))
            .collect::<Result<Vec<_>>>()?;
        let owner = crate::tx_ingress::start_native_candidate_storage_owner_v1(params)
            .context("pipeline fixture start resident storage owner")?;
        let client = owner.client();
        let _remote_scope = client
            .enter()
            .context("pipeline fixture enter storage client")?;
        crate::native_block_seal::tests::native_seal_round_network::with_service_test_transports(
            chain,
            |peers| -> Result<()> {
                let peer_id = config.authority.transport_peer_id(leader_id)?;
                let (runtime, key) = peers
                    .iter()
                    .find(|(runtime, _)| runtime.startup().local_peer_id == peer_id)
                    .context("pipeline fixture leader transport missing")?;
                config.signer = (*key).clone();
                let (sender, sender_key) = peers
                    .iter()
                    .find(|(r, _)| r.startup().local_peer_id != peer_id)
                    .context("pipeline fixture remote transport missing")?;
                let now = Instant::now();
                let mut lifecycle = Self::open(config.clone(), &ledger, params, runtime, now)
                    .context("pipeline fixture open validated fresh lifecycle")?;
                // Use a separate real durable *pending pool* only in this test.
                // The original seal store/QC/AOEM authority DB are unchanged,
                // and subsequent sequence assertions keep their original pool.
                lifecycle.pool = Some(FreshTransactionPool::open(&pool_path, chain, pin, params)?);
                lifecycle
                    .config
                    .as_mut()
                    .context("pipeline config missing")?
                    .propose_successors = true;
                lifecycle
                    .enable_candidate_pipeline(client.clone())
                    .context("pipeline fixture enable resident candidate worker")?;
                // Match the real node after validated startup. This retains
                // the connection/revision only, never an ordinary-ledger or
                // signing permission for the reserved fresh ledger.
                let _ledger_session = crate::native_block_ledger::NovNativeBlockLedgerV1::retain_existing_fresh_session_v1(&ledger)
                    .context("pipeline fixture retain validated fresh ledger session")?;
                let parent = lifecycle
                    .finalized_parent
                    .as_ref()
                    .context("pipeline parent missing")?;
                if parent.block().header.height != 3 {
                    bail!("pipeline fixture requires the finalized third block");
                }
                let parent_id = parent.workspace_id();
                let parent_digest = parent.output_digest();
                let committed_hash = parent.block().body.tx_hashes[0];
                let parent_hash = parent.block().header.block_hash;
                let parent_state_root = parent.block().header.post_state_root;
                let context = novovm_protocol::NovBlockExecutionContextV1 {
                    chain_id: chain,
                    block_height: 4,
                    parent_block_hash: parent.block().header.block_hash,
                    slot: parent.block().header.slot + 1,
                    timestamp_unix_ms: timestamp,
                };
                let original_catalog = workspace::list_v1(chain, params)?;
                // More than the entire durable slot budget may be captured and
                // discarded: unaccepted work must never consume those slots.
                for offset in 0..workspace::MAX_WORKSPACES_V1 + 2 {
                    let mut dropped_context = context;
                    dropped_context.timestamp_unix_ms += 100 + offset as u64;
                    let plan =
                        parent.successor_plan(dropped_context, vec![raws[0].clone()], params)?;
                    let captured = workspace::capture_execution_from_finalized_v1(
                        &plan, parent_id, pin, params,
                    )?;
                    if workspace::load_v1(chain, captured.workspace_id(), params)?.is_some() {
                        bail!("unaccepted capture allocated a durable candidate slot");
                    }
                    drop(captured);
                }
                if workspace::list_v1(chain, params)? != original_catalog {
                    bail!("dropping captured candidates changed the workspace catalog");
                }
                let novovm_protocol::NovTxKindV1::Transfer(transfer) =
                    novovm_protocol::decode_nov_native_tx_wire_v1(&raws[0])?.kind
                else {
                    bail!("pipeline balance fixture requires a signed Transfer");
                };
                let balance_params = serde_json::json!({
                    "account":format!("0x{}", crate::native_block_seal::hex_v1(&transfer.from)),
                    "asset":"NOV",
                });
                let balance_before = rpc_params(
                    &mut lifecycle,
                    "nov_getAssetBalance",
                    balance_params.clone(),
                )?;
                if balance_before["finalized"] != true
                    || balance_before["found"] != true
                    || balance_before["finalized_tip_height"] != 3
                    || balance_before["block_hash"]
                        != crate::native_block_seal::hex_v1(&parent_hash)
                    || balance_before["state_root"]
                        != crate::native_block_seal::hex_v1(&parent_state_root)
                {
                    bail!("balance fixture is not bound to its finalized parent");
                }
                let (entered, release) = lifecycle
                    .candidate_worker
                    .as_mut()
                    .context("pipeline worker missing")?
                    .pause_next_for_test()?;
                let mut release = ReleaseWorker(Some(release));
                if rpc(&mut lifecycle, "nov_sendRawTransaction", &raws[0])?["status"] != "queued" {
                    bail!("pipeline candidate transaction was not durably queued");
                }
                lifecycle.poll_with_wall_time(runtime, now, timestamp)?;
                entered
                    .recv_timeout(Duration::from_secs(3))
                    .context("pipeline never dispatched its real job to the worker")?;
                let pending = lifecycle
                    .preparing
                    .as_ref()
                    .context("pipeline continuation missing")?;
                if pending.execution_ready
                    || !lifecycle
                        .candidate_worker
                        .as_ref()
                        .is_some_and(|w| w.is_busy())
                {
                    bail!("pipeline fixture bypassed worker through an existing complete output");
                }
                let candidate = pending.preparation.workspace_id();
                if workspace::load_execution_v1(chain, candidate, params)?.is_some() {
                    bail!("paused pipeline published an output before computation");
                }
                // These public RPC handlers point-read finalized state through
                // the same remote storage owner while a real job is outstanding.
                if rpc(&mut lifecycle, "nov_getTransactionStatus", &committed_hash)?["status"]
                    != "finalized"
                    || rpc(&mut lifecycle, "nov_sendRawTransaction", &raws[1])?["status"]
                        != "queued"
                    || rpc(&mut lifecycle, "nov_getTransactionStatus", &entries[1].hash)?["status"]
                        != "queued"
                {
                    bail!("RPC query/admission did not progress while candidate was in flight");
                }
                if rpc_params(
                    &mut lifecycle,
                    "nov_getAssetBalance",
                    balance_params.clone(),
                )? != balance_before
                {
                    bail!("in-flight execution changed the finalized RPC balance");
                }
                if !sender.try_submit_to_peer(
                    peer_id,
                    ProductMainlineOverlayPayloadClassV1::NativeTransaction,
                    entries[2].hash,
                    raws[2].clone(),
                )? {
                    bail!("pipeline fixture transaction transport refused initial submission");
                }
                let wait = Instant::now();
                let mut accepted_network = false;
                while !accepted_network {
                    for event in runtime.drain_events(128) {
                        if let Event::Inbound(inbound) = event {
                            if inbound.payload_class
                                == ProductMainlineOverlayPayloadClassV1::NativeTransaction
                                && inbound.object_hash == entries[2].hash
                            {
                                if !lifecycle.enqueue(inbound) {
                                    bail!(
                                        "pipeline fixture authenticated transport was not enqueued"
                                    );
                                }
                                accepted_network = true;
                            }
                        }
                    }
                    if wait.elapsed() >= Duration::from_secs(4) {
                        bail!("pipeline fixture WSS delivery deadline");
                    }
                    if !accepted_network {
                        std::thread::park_timeout(Duration::from_millis(5));
                    }
                }
                // Fixed monotonic test instant remains before the real round
                // timer: this tests a live pacemaker poll without signing a
                // timeout that would contaminate the following round-0 fixture.
                lifecycle.poll_with_wall_time(
                    runtime,
                    now + Duration::from_millis(1),
                    timestamp,
                )?;
                if lifecycle.status_json()["parent_pacemaker"]["timeout_count"] != 0
                    || lifecycle.status_json()["durable_pending_transactions"] != 3
                    || lifecycle.status_json()["candidate_pipeline"]["completed"] != 0
                    || lifecycle.status_json()["candidate_preparation_inflight"] != true
                {
                    bail!("pipeline did not preserve pending work during its live main poll");
                }
                // Reuse the existing durable timeout signer and authenticated
                // overlay, with a separate peer store and controlled timer.
                // This is an actual signed round message, not a queue counter
                // or an invented receipt. One remote vote cannot change round.
                let mut peer_config = config.clone();
                peer_config.local_validator_id = config
                    .authority
                    .transport_bindings
                    .iter()
                    .find(|binding| binding.transport_peer_id == sender.startup().local_peer_id)
                    .context("pipeline timeout peer is not a validator")?
                    .validator_id;
                peer_config.signer = (*sender_key).clone();
                peer_config.seal_store_path = path.with_extension("pipeline-timeout-peer-seal");
                peer_config.validate(chain)?;
                // Keep the production service's legal timeout interval. Only
                // the peer's monotonic start instant is earlier; do not lower
                // the service timing bounds merely to speed up this fixture.
                let due = now + Duration::from_millis(1);
                let peer_started = due
                    .checked_sub(peer_config.round_timeout)
                    .context("pipeline timeout fixture instant underflow")?;
                let mut timeout_peer = super::super::pacemaker::ParentPacemaker::open(
                    &peer_config,
                    params,
                    peer_started,
                )?;
                timeout_peer.poll(&peer_config, params, sender, due)?;
                let mut received_vote = None;
                let vote_wait = Instant::now();
                while lifecycle.status_json()["parent_pacemaker"]["timeout_count"] != 1 {
                    for event in runtime.drain_events(128) {
                        if let Event::Inbound(inbound) = event {
                            if inbound.payload_class
                                != ProductMainlineOverlayPayloadClassV1::NativeSeal
                            {
                                continue;
                            }
                            use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1;
                            let message = crate::native_block_seal::round_wire::decode_nov_native_seal_round_wire_v1(
                                &inbound.frame.payload, &config.authority, 4, &inbound.source_peer_id,
                            )?;
                            if let NovNativeSealRoundMessageV1::Timeout(vote) = message {
                                vote.verify(&config.authority.validator_set)?;
                                if vote.validator_id != peer_config.local_validator_id {
                                    bail!("pipeline received a different timeout signer");
                                }
                                received_vote = Some(*vote);
                                if !lifecycle.enqueue(inbound) {
                                    bail!("pipeline rejected authenticated in-flight timeout transport");
                                }
                            }
                        }
                    }
                    lifecycle.poll_with_wall_time(runtime, due, timestamp)?;
                    if vote_wait.elapsed() >= Duration::from_secs(3) {
                        bail!("real signed timeout did not reach the live in-flight pacemaker");
                    }
                    std::thread::yield_now();
                }
                let live = lifecycle.status_json();
                if received_vote.is_none()
                    || live["parent_pacemaker"]["round"] != 0
                    || live["parent_pacemaker"]["new_view_ready"] != false
                    || live["candidate_pipeline"]["busy"] != true
                    || live["candidate_pipeline"]["completed"] != 0
                    || live["candidate_preparation_inflight"] != true
                {
                    bail!("signed timeout did not coexist with an unfinished candidate: {live}");
                }
                drop(timeout_peer);
                let recovered_vote = workspace::with_verified_finalized_parent_round_v1(
                    chain,
                    parent_id,
                    pin,
                    params,
                    |view| {
                        crate::native_block_seal::NovNativeBlockSealStoreV1::open(
                            &peer_config.seal_store_path,
                        )?
                        .load_local_timeout(
                            view,
                            &config.authority.validator_set,
                            4,
                            0,
                            peer_config.local_validator_id,
                        )
                    },
                )?;
                if recovered_vote != received_vote {
                    bail!("in-flight timeout differs from its reopened durable signer record");
                }
                // Explicit completion-fence injection, not a claimed real BFT
                // round change. The existing pacemaker tests cover real votes.
                lifecycle
                    .preparing
                    .as_mut()
                    .context("pipeline pending missing")?
                    .round = 1;
                release
                    .0
                    .take()
                    .context("pipeline release missing")?
                    .send(())
                    .context("pipeline worker stopped before explicit release")?;
                let compute_wait = Instant::now();
                while !lifecycle.candidate_completion_ready() {
                    if compute_wait.elapsed() >= Duration::from_secs(30) {
                        bail!("pipeline real AOEM worker completion deadline");
                    }
                    std::thread::park_timeout(Duration::from_millis(10));
                }
                // A ready result remains queued during clock waiting, without
                // repeatedly bypassing the RPC idle budget. This injects only
                // the wait flag; clock rollback itself has separate tests.
                lifecycle.clock_waiting = true;
                if lifecycle.candidate_completion_ready()
                    || lifecycle.status_json()["candidate_pipeline"]["completion_ready"] != true
                {
                    bail!("clock wait lost its queued completion or would busy-spin");
                }
                lifecycle.clock_waiting = false;
                lifecycle.poll_with_wall_time(
                    runtime,
                    now + Duration::from_millis(2),
                    timestamp,
                )?;
                let status = lifecycle.status_json();
                if status["candidate_stale_completions"] != 1
                    || status["candidate_preparation_inflight"] != false
                    || status["candidate_pipeline"]["completed"] != 1
                    || status["candidate_pipeline"]["failed"] != 0
                    || status["candidate_pipeline"]["busy"] != false
                    || status["height"] != 3
                    || status["proposed_successors"] != 0
                    || status["received_successors"] != 0
                    || status["durable_pending_transactions"] != 3
                    || lifecycle.service.is_some()
                {
                    bail!(
                        "stale real worker completion was published or lost pending work: {status}"
                    );
                }
                if workspace::load_v1(chain, candidate, params)?.is_some()
                    || workspace::list_v1(chain, params)? != original_catalog
                {
                    bail!("stale owned worker completion left a durable candidate slot");
                }
                workspace::assert_execution_unpublished_for_test_v1(chain, candidate, params)?;
                let current =
                    workspace::load_finalized_parent_view_v1(chain, parent_id, pin, params)?;
                if current.output_digest() != parent_digest || current.block().header.height != 3 {
                    bail!("stale worker completion changed finalized authority");
                }
                // The same dropped plan is not permanently tombstoned: capture
                // it again, compute with AOEM, check the in-memory subject, and
                // pass its original bytes through the owner-local durable stage.
                let plan = current.successor_plan(context, vec![raws[0].clone()], params)?;
                let workspace::ExecutionStartV1::Job(job) =
                    workspace::capture_execution_from_finalized_v1(&plan, parent_id, pin, params)?
                else {
                    bail!("dropped plan unexpectedly already has durable execution");
                };
                let computed = job.run()?;
                if computed.workspace_id() != candidate {
                    bail!("recaptured dropped plan changed its candidate identity");
                }
                let preview = computed.preview_successor_subject_v1(&current, 0)?;
                let mut false_subject = preview.clone();
                false_subject.post_state_root[0] ^= 1;
                let bad = config.clone().begin_fresh_successor(
                    context.slot,
                    timestamp,
                    vec![raws[0].clone()],
                    params,
                    Some(false_subject),
                )?;
                let error = bad
                    .check_computed(&computed, params)
                    .err()
                    .context("false output subject was accepted before staging")?;
                if !error.is::<SuccessorOutputMismatch>()
                    || workspace::list_v1(chain, params)? != original_catalog
                {
                    bail!("false subject rejection changed catalog or used the wrong error: {error:#}");
                }
                drop(bad);
                let mut preparation = config.clone().begin_fresh_successor(
                    context.slot,
                    timestamp,
                    vec![raws[0].clone()],
                    params,
                    Some(preview.clone()),
                )?;
                // Use the result already computed above. The newly captured
                // duplicate input is owned-only and consumes no durable slot.
                drop(preparation.take_execution()?);
                let seal = crate::native_block_seal::NovNativeBlockSealStoreV1::open(
                    &config.seal_store_path,
                )?;
                let outbox_before = seal.load_pending_outbox(chain, leader_id, 128)?;
                let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
                let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
                let mut release = ReleaseWorker(Some(release_tx));
                lifecycle.preparing = Some(PreparingCandidate {
                    preparation,
                    round: 0,
                    origin: CandidateOrigin::Local { certificate: None },
                    execution_ready: false,
                    durability: Some(DurabilityStage::Waiting(Box::new(move || {
                        entered_tx
                            .send(())
                            .context("durability entry observer disappeared")?;
                        release_rx
                            .recv_timeout(Duration::from_secs(10))
                            .context("durability fixture release deadline")?;
                        workspace::finish_execution_v1(computed)
                    }))),
                });
                lifecycle.poll_candidate_durability()?;
                entered_rx
                    .recv_timeout(Duration::from_secs(3))
                    .context("real storage owner did not enter durable stage")?;
                if !lifecycle.candidate_storage_busy()
                    || lifecycle
                        .preparing
                        .as_ref()
                        .is_none_or(|p| p.execution_ready)
                    || lifecycle.candidate_completion_ready()
                {
                    bail!("durability stage was not independently in flight");
                }
                // The owner is actually occupied. A hidden main-thread storage
                // access would block this poll until the latch times out; the
                // pure chain status response must still be served. State RPC is
                // buffered, never falsely acknowledged as durably admitted.
                let mut server =
                    crate::native_fresh_rpc::FreshRpcServer::bind("127.0.0.1:0".parse()?)?;
                let mut state_query = http_request(
                    server.local_addr()?,
                    "nov_getTransactionStatus",
                    serde_json::json!([crate::native_block_seal::hex_v1(&committed_hash)]),
                )?;
                let mut balance_query = http_request(
                    server.local_addr()?,
                    "nov_getAssetBalance",
                    balance_params.clone(),
                )?;
                let mut status_query = http_request(
                    server.local_addr()?,
                    "nov_chainStatus",
                    serde_json::json!([]),
                )?;
                let mut response = Vec::new();
                let responsive_since = Instant::now();
                let chain_status = loop {
                    server.poll(&mut lifecycle)?;
                    lifecycle.poll_with_wall_time(
                        runtime,
                        now + Duration::from_millis(3),
                        timestamp,
                    )?;
                    read_available(&mut status_query, &mut response)?;
                    if let Some(end) = response.windows(4).position(|v| v == b"\r\n\r\n") {
                        if let Ok(value) =
                            serde_json::from_slice::<serde_json::Value>(&response[end + 4..])
                        {
                            break value;
                        }
                    }
                    if responsive_since.elapsed() >= Duration::from_secs(1) {
                        bail!("chain status stalled behind the real durable stage");
                    }
                    std::thread::yield_now();
                };
                if chain_status.get("error").is_some()
                    || chain_status["result"]["height"] != 3
                    || chain_status["result"]["candidate_preparation_inflight"] != true
                    || lifecycle
                        .preparing
                        .as_ref()
                        .is_none_or(|p| p.execution_ready)
                    || lifecycle.service.is_some()
                {
                    bail!("durability stage reported an early ready/signing state: {chain_status}");
                }
                let mut state_response = Vec::new();
                read_available(&mut state_query, &mut state_response)?;
                let mut balance_response = Vec::new();
                read_available(&mut balance_query, &mut balance_response)?;
                let balance_error = rpc_params(
                    &mut lifecycle,
                    "nov_getAssetBalance",
                    balance_params.clone(),
                )
                .err()
                .context("busy storage owner accepted a direct balance query")?;
                if !state_response.is_empty()
                    || !balance_response.is_empty()
                    || !format!("{balance_error:#}").contains("candidate durable stage busy")
                    || rpc(&mut lifecycle, "nov_getTransactionStatus", &committed_hash).is_ok()
                    || rpc(&mut lifecycle, "nov_sendRawTransaction", &raws[0]).is_ok()
                {
                    bail!("storage-dependent RPC dispatched while its owner was occupied");
                }
                // This query deliberately disconnects without retry; it was
                // never an admitted transaction. RPC's separate tests cover
                // resuming buffered requests within their original deadlines.
                drop(state_query);
                drop(balance_query);
                drop(status_query);
                drop(server);
                release
                    .0
                    .take()
                    .context("durability release missing")?
                    .send(())?;
                let durable_wait = Instant::now();
                while !lifecycle.candidate_completion_ready() {
                    if durable_wait.elapsed() >= Duration::from_secs(30) {
                        bail!("owner-local durable completion deadline");
                    }
                    std::thread::park_timeout(Duration::from_millis(10));
                }
                lifecycle.poll_with_wall_time(
                    runtime,
                    now + Duration::from_millis(4),
                    timestamp,
                )?;
                if lifecycle.candidate_storage_busy()
                    || lifecycle
                        .preparing
                        .as_ref()
                        .is_none_or(|p| !p.execution_ready)
                    || lifecycle.service.is_some()
                    || lifecycle.status_json()["height"] != 3
                {
                    bail!("durable completion registered before the next live fence");
                }
                if seal.load_pending_outbox(chain, leader_id, 128)? != outbox_before {
                    bail!("durable-only stage produced a new vote");
                }
                // Inject a stale scheduler token only after the true durable
                // result is ready. The next normal poll must reject it after
                // its real pacemaker/live-parent work, without registration.
                lifecycle
                    .preparing
                    .as_mut()
                    .context("durable continuation missing")?
                    .round = 1;
                lifecycle.poll_with_wall_time(
                    runtime,
                    now + Duration::from_millis(5),
                    timestamp,
                )?;
                if lifecycle.preparing.is_some()
                    || lifecycle.service.is_some()
                    || lifecycle.status_json()["candidate_stale_completions"] != 2
                    || lifecycle.status_json()["proposed_successors"] != 0
                    || lifecycle.status_json()["durable_pending_transactions"] != 3
                    || seal.load_pending_outbox(chain, leader_id, 128)? != outbox_before
                {
                    bail!("stale durable output registered, voted or removed pending transactions");
                }
                drop(lifecycle); // joins the compute worker before its storage owner.
                let durable = workspace::load_block_artifact_v1(chain, candidate, params)?
                    .context("recaptured accepted output missing")?;
                if current.successor_seal_subject(&durable, 0)? != preview {
                    bail!("unstaged computed preview differs from the durable artifact subject");
                }
                // Reserved fresh ledgers intentionally reject ordinary open,
                // even while a retained session exists. Inspect registration
                // only through the existing verified live-parent read scope.
                let stale_record = workspace::with_verified_finalized_parent_round_v1(
                    chain,
                    parent_id,
                    pin,
                    params,
                    |view| view.load_candidate_record(chain, preview.block_hash),
                )
                .context(
                    "pipeline fixture inspect stale registration through verified fresh parent",
                )?;
                if stale_record.is_some() {
                    bail!("stale durable output was registered in the block ledger");
                }
                // This isolated complete output intentionally remains in the
                // fixture. The test does NOT claim unregistered stale outputs
                // are reclaimed by the production retirement implementation.
                let recovered = FreshTransactionPool::open(&pool_path, chain, pin, params)?;
                if recovered.len() != 3
                    || entries.iter().any(|entry| !recovered.contains(&entry.hash))
                {
                    bail!("worker completion fence lost durable pending transactions after reopen");
                }
                eprintln!("real candidate worker: in-flight RPC dispatch/query, WSS ingress and verified remote timeout vote progressed; timeout signer record reopened exactly, no quorum claimed; AOEM completed; stale-round computation dropped with no slot; 34 captures dropped without catalog growth; false subject rejected before stage; same plan owner-local durability kept HTTP chainStatus/main poll responsive and state RPC backpressured; ready durable output crossed no signing boundary; injected stale round rejected registration; exact preview and 3 durable pool entries recovered");
                Ok(())
            },
        )
    }
}
