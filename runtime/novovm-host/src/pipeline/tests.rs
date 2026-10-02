use super::*;
use crate::persistence::{CandidateStore, PacketBudget, StorageDomain};
use crate::state::tree::{empty_root, stage_state_update, StateChange, StateNodeReader};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub(super) fn request(root: NodeHash) -> BatchRequest {
    let input = compute::tests::control_test_request(root);
    BatchRequest::new(input.raw_transactions, input.context, input.policy).unwrap()
}

pub(super) fn config(database: PathBuf) -> PipelineConfig {
    let context = request(empty_root()).request.context;
    PipelineConfig {
        store: StoreConfig {
            library: PathBuf::from("unused-in-channel-test"),
            database,
            domain: StorageDomain {
                chain_id: context.chain_id,
                genesis_config_commitment: context.genesis_config_commitment,
                protocol_commitment: context.protocol_commitment,
            },
            storage: novovm_aoem::StorageConfig::default(),
            packet_budget: PacketBudget::default(),
        },
        workers: 2,
        max_batches: 1,
        max_retained_bytes: 128 * 1024 * 1024,
        authentication: AuthenticationBudget {
            transactions: 16,
            transaction_bytes: 4096,
            body_bytes: 65536,
        },
        plan: PlanBudget {
            transactions: 16,
            transaction_bytes: 4096,
            body_bytes: 65536,
            access_keys: 4096,
        },
        capture: CaptureBudget {
            keys: 4096,
            nodes: 4096,
            bytes: 1024 * 1024,
        },
        compute_timeout: Duration::from_secs(30),
        io: IoBudget::default(),
        capture_edge_quantum: 64,
    }
}

pub(super) fn inert_pipeline(
    config: PipelineConfig,
) -> (CandidatePipeline, mpsc::Receiver<DriverMessage>) {
    let (sender, receiver) = mpsc::sync_channel(1);
    (
        CandidatePipeline {
            config,
            sender: Some(sender),
            worker: Some(thread::spawn(|| Ok(()))),
            io: None,
            queries: None,
            metadata: None,
            identity: Arc::new(()),
            usage: Arc::new(Mutex::new(Usage::default())),
        },
        receiver,
    )
}

fn admitted(pipeline: &CandidatePipeline, request: BatchRequest) -> PipelineTicket {
    match pipeline.try_submit(request).unwrap() {
        Submission::Accepted(ticket) => ticket,
        Submission::Backpressured(_) => panic!("unexpected test backpressure"),
    }
}

fn recv_batch(receiver: &mpsc::Receiver<DriverMessage>) -> Command {
    match receiver.recv().unwrap() {
        DriverMessage::Batch(command) => command,
        _ => panic!("expected ordinary batch command"),
    }
}

#[test]
fn background_admission_reserves_an_ordinary_slot_and_worst_case_content() {
    let cfg = config("unused-background".into());
    let (single, _) = inert_pipeline(cfg.clone());
    assert!(matches!(
        single
            .try_submit_background_owned(request(empty_root()))
            .unwrap(),
        Submission::Backpressured(_)
    ));
    let bytes = request(empty_root()).reservation(&cfg).unwrap();
    let reserve = BatchRequest::retained_reservation(
        cfg.authentication.body_bytes.min(cfg.plan.body_bytes),
        &cfg,
    )
    .unwrap();
    let mut cfg = cfg;
    cfg.max_batches = 2;
    cfg.max_retained_bytes = bytes + reserve - 1;
    let (short, _) = inert_pipeline(cfg.clone());
    assert!(matches!(
        short
            .try_submit_background_owned(request(empty_root()))
            .unwrap(),
        Submission::Backpressured(_)
    ));
    cfg.max_retained_bytes += 1;
    let (pipeline, receiver) = inert_pipeline(cfg);
    let Submission::Accepted(background) = pipeline
        .try_submit_background_owned(request(empty_root()))
        .unwrap()
    else {
        panic!("exact reservation should fit")
    };
    let command = recv_batch(&receiver);
    assert!(command.background);
    assert!(matches!(
        pipeline
            .try_submit_background_owned(request(empty_root()))
            .unwrap(),
        Submission::Backpressured(_)
    ));
    let ordinary = admitted(&pipeline, request(empty_root()));
    let normal_command = recv_batch(&receiver);
    assert!(!normal_command.background);
    assert_eq!(pipeline.usage.lock().unwrap().batches, 2);
    drop(background);
    drop(command);
    assert_eq!(pipeline.usage.lock().unwrap().background, 0);
    drop(ordinary);
    drop(normal_command);
    assert_eq!(pipeline.usage.lock().unwrap().bytes, 0);
}

#[test]
fn background_reply_and_abandoned_ticket_never_release_live_work_early() {
    let mut cfg = config("unused-background-lifetime".into());
    cfg.max_batches = 4;
    cfg.max_retained_bytes = 512 * 1024 * 1024;
    let (pipeline, receiver) = inert_pipeline(cfg);
    let Submission::Accepted(mut ticket) = pipeline
        .try_submit_background_owned(request(empty_root()))
        .unwrap()
    else {
        panic!("expected background admission")
    };
    let command = recv_batch(&receiver);
    command
        .reply
        .send(Err(anyhow::anyhow!("fixture rejection")))
        .ok()
        .unwrap();
    drop(command);
    assert_eq!(pipeline.usage.lock().unwrap().background, 1);
    assert!(matches!(
        pipeline
            .try_submit_background_owned(request(empty_root()))
            .unwrap(),
        Submission::Backpressured(_)
    ));
    assert!(ticket.try_take().is_err());
    assert_eq!(pipeline.usage.lock().unwrap().background, 0);
    let Submission::Accepted(ticket) = pipeline
        .try_submit_background_owned(request(empty_root()))
        .unwrap()
    else {
        panic!("background permit should recover")
    };
    drop(ticket);
    assert_eq!(pipeline.usage.lock().unwrap().background, 1);
    let command = recv_batch(&receiver);
    drop(command);
    assert_eq!(pipeline.usage.lock().unwrap().background, 0);
    assert_eq!(pipeline.usage.lock().unwrap().bytes, 0);
}

#[test]
fn unconsumed_error_reply_and_lost_ticket_keep_whole_job_reservations() {
    let (pipeline, receiver) = inert_pipeline(config("unused".into()));
    let input = request(empty_root());
    let bytes = input.reservation(&pipeline.config).unwrap();
    let mut ticket = admitted(&pipeline, input);
    let command = recv_batch(&receiver);
    command
        .reply
        .send(Err(anyhow::anyhow!("explicit rejection")))
        .ok()
        .unwrap();
    drop(command);
    assert_eq!(pipeline.usage.lock().unwrap().bytes, bytes);
    assert!(matches!(
        pipeline.try_submit(request(empty_root())).unwrap(),
        Submission::Backpressured(_)
    ));
    assert!(ticket.try_take().is_err());
    assert_eq!(pipeline.usage.lock().unwrap().bytes, 0);
    assert!(ticket.try_take().is_err());
    let ticket = admitted(&pipeline, request(empty_root()));
    drop(ticket);
    assert_eq!(pipeline.usage.lock().unwrap().batches, 1);
    let command = recv_batch(&receiver);
    assert_eq!(command.request.transaction_count, 1);
    drop(command);
    assert_eq!(pipeline.usage.lock().unwrap().batches, 0);
}

#[test]
fn full_queue_returns_exact_input_and_domain_byte_limits_precede_admission() {
    let mut cfg = config("unused".into());
    cfg.max_batches = 4;
    cfg.max_retained_bytes = 512 * 1024 * 1024;
    let (pipeline, receiver) = inert_pipeline(cfg);
    let first = admitted(&pipeline, request(empty_root()));
    let next = request(empty_root());
    let allocation = next.request.raw_transactions[0].as_ptr();
    match pipeline.try_submit(next).unwrap() {
        Submission::Backpressured(request) => {
            assert_eq!(allocation, request.request.raw_transactions[0].as_ptr())
        }
        Submission::Accepted(_) => panic!("queue was full"),
    }
    assert_eq!(pipeline.usage.lock().unwrap().batches, 1);
    let mut wrong = request(empty_root());
    wrong.request.context.genesis_config_commitment[0] ^= 1;
    assert!(pipeline.try_submit(wrong).is_err());
    let mut oversized = request(empty_root());
    oversized.body_bytes = usize::MAX;
    assert!(pipeline.try_submit(oversized).is_err());
    drop(receiver);
    assert!(pipeline.try_submit(request(empty_root())).is_err());
    drop(first);
    assert_eq!(pipeline.usage.lock().unwrap().batches, 0);
}

#[derive(Debug, PartialEq, Eq)]
struct RequestAllocation {
    owner: *const compute::PrepareRequest,
    transactions: *const Vec<u8>,
    bodies: Vec<(*const u8, usize, usize)>,
}

fn allocation(input: &BatchRequest) -> RequestAllocation {
    RequestAllocation {
        owner: input.request.as_ref(),
        transactions: input.request.raw_transactions.as_ptr(),
        bodies: input
            .request
            .raw_transactions
            .iter()
            .map(|raw| (raw.as_ptr(), raw.len(), raw.capacity()))
            .collect(),
    }
}

fn accounting(pipeline: &CandidatePipeline) -> (usize, usize) {
    let usage = pipeline
        .usage
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    (usage.batches, usage.bytes)
}

fn expect_owned_rejection(
    pipeline: &CandidatePipeline,
    input: BatchRequest,
    expected_error: &str,
) -> BatchRequest {
    let expected = allocation(&input);
    let usage = accounting(pipeline);
    let rejected = match pipeline.try_submit_owned(input) {
        Err(rejected) => rejected,
        Ok(_) => panic!("invalid input/service was not rejected"),
    };
    assert!(
        rejected.error.to_string().contains(expected_error),
        "wrong rejection: {}",
        rejected.error
    );
    assert_eq!(allocation(&rejected.request), expected);
    assert_eq!(accounting(pipeline), usage);
    rejected.request
}

#[test]
fn owned_admission_returns_allocations_on_domain_size_and_reservation_errors() {
    let (pipeline, receiver) = inert_pipeline(config("unused".into()));
    let mut wrong = request(empty_root());
    wrong.request.context.genesis_config_commitment[0] ^= 1;
    let _returned = expect_owned_rejection(&pipeline, wrong, "domain mismatch");
    let mut oversized = request(empty_root());
    oversized.body_bytes = usize::MAX;
    let _returned = expect_owned_rejection(&pipeline, oversized, "input budget");
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));

    let mut cfg = config("unused".into());
    cfg.max_retained_bytes = 1;
    let (pipeline, receiver) = inert_pipeline(cfg);
    let _returned = expect_owned_rejection(
        &pipeline,
        request(empty_root()),
        "one batch exceeds pipeline retained-content budget",
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));

    let mut cfg = config("unused".into());
    cfg.capture.bytes = usize::MAX;
    let (pipeline, receiver) = inert_pipeline(cfg);
    let _returned = expect_owned_rejection(&pipeline, request(empty_root()), "budget overflow");
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
}

#[test]
fn owned_admission_checks_sender_and_worker_before_enqueue() {
    let (mut pipeline, receiver) = inert_pipeline(config("unused".into()));
    pipeline.sender.take();
    let _returned = expect_owned_rejection(&pipeline, request(empty_root()), "pipeline closed");
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));

    let (mut pipeline, receiver) = inert_pipeline(config("unused".into()));
    pipeline.worker.take().unwrap().join().unwrap().unwrap();
    let _returned = expect_owned_rejection(&pipeline, request(empty_root()), "worker unavailable");
    // A missing worker must not enqueue and then falsely report non-acceptance.
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert_eq!(accounting(&pipeline), (0, 0));
}

#[test]
fn owned_disconnected_and_poisoned_admission_return_request_without_leaking_quota() {
    let (pipeline, receiver) = inert_pipeline(config("unused".into()));
    drop(receiver);
    let _returned = expect_owned_rejection(
        &pipeline,
        request(empty_root()),
        "pipeline unavailable; request not accepted",
    );
    assert_eq!(accounting(&pipeline), (0, 0));

    let (pipeline, receiver) = inert_pipeline(config("unused".into()));
    let usage = pipeline.usage.clone();
    let poisoned = std::panic::catch_unwind(move || {
        let _guard = usage.lock().unwrap();
        panic!("deliberate admission accounting poison");
    });
    assert!(poisoned.is_err());
    let _returned = expect_owned_rejection(
        &pipeline,
        request(empty_root()),
        "admission accounting poisoned",
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert_eq!(accounting(&pipeline), (0, 0));
}

#[test]
fn owned_full_and_busy_admission_preserve_allocation_and_release_unused_permits() {
    let mut cfg = config("unused".into());
    cfg.max_batches = 4;
    cfg.max_retained_bytes = 512 * 1024 * 1024;
    let (pipeline, receiver) = inert_pipeline(cfg);
    let first = admitted(&pipeline, request(empty_root()));
    let before = accounting(&pipeline);
    let input = request(empty_root());
    let expected = allocation(&input);
    let Submission::Backpressured(returned) = pipeline.try_submit_owned(input).unwrap() else {
        panic!("full queue accepted input");
    };
    assert_eq!(allocation(&returned), expected);
    assert_eq!(accounting(&pipeline), before);
    let command = recv_batch(&receiver);
    drop(command);
    drop(first);
    assert_eq!(accounting(&pipeline), (0, 0));

    let usage = pipeline.usage.lock().unwrap();
    let input = request(empty_root());
    let expected = allocation(&input);
    let Submission::Backpressured(returned) = pipeline.try_submit_owned(input).unwrap() else {
        panic!("busy admission accounting accepted input");
    };
    assert_eq!(allocation(&returned), expected);
    drop(usage);
    assert_eq!(accounting(&pipeline), (0, 0));
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
}

#[test]
fn owned_success_is_accepted_even_when_the_reply_is_later_lost() {
    let (pipeline, receiver) = inert_pipeline(config("unused".into()));
    let input = request(empty_root());
    let expected = allocation(&input);
    let bytes = input.reservation(&pipeline.config).unwrap();
    let Submission::Accepted(mut ticket) = pipeline.try_submit_owned(input).unwrap() else {
        panic!("empty queue rejected input");
    };
    let command = recv_batch(&receiver);
    assert_eq!(allocation(&command.request), expected);
    assert_eq!(accounting(&pipeline), (1, bytes));
    drop(command);
    assert!(ticket
        .try_take()
        .err()
        .unwrap()
        .to_string()
        .contains("accepted request outcome unknown"));
    assert_eq!(accounting(&pipeline), (0, 0));
}

struct Empty;
impl StateNodeReader for Empty {
    fn read_node(&self, _: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

#[test]
#[ignore = "requires real AOEM; explicit worker latch proves control isolation, NOT natural concurrency or TPS"]
fn real_paused_compute_does_not_block_control_or_aoem_storage_queries() -> Result<()> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/pipeline-control-tests")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    std::fs::create_dir_all(&directory)?;
    let mut cfg = config(directory.join("state.rocksdb"));
    cfg.store.library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .context("explicit AOEM test library required")?
        .into();
    let key = b"control-query".to_vec();
    let update = stage_state_update(
        &Empty,
        empty_root(),
        &[StateChange::Put {
            key: key.clone(),
            value: vec![19],
        }],
    )?;
    let store = CandidateStore::open(cfg.store.clone(), OpenMode::CreateNew)?;
    store.install_unpublished_state(&update)?;
    drop(store);
    let io = Arc::new(IoService::start(
        cfg.store.clone(),
        OpenMode::Existing,
        cfg.io,
    )?);
    let compute = compute::ComputeOwner::start(
        compute::ComputeConfig {
            capture: CaptureBudget {
                keys: 4096,
                nodes: 4096,
                bytes: 1024 * 1024,
            },
            library: cfg.store.library.clone(),
            workers: cfg.workers,
            queue_capacity: cfg.max_batches,
            authentication: cfg.authentication,
            plan: cfg.plan,
            packet: cfg.store.packet_budget,
            timeout: cfg.compute_timeout,
            domain: cfg.store.domain,
        },
        None,
    )?;
    let (entered, release) = compute.hold_for_test()?;
    entered.recv_timeout(Duration::from_secs(5))?;
    let (sender, receiver) = mpsc::sync_channel(cfg.max_batches);
    let worker_config = cfg.clone();
    let worker_io = io.clone();
    let identity = Arc::new(());
    let worker_identity = identity.clone();
    let worker = thread::spawn(move || {
        driver::run(
            &worker_config,
            &worker_io,
            &compute,
            receiver,
            &worker_identity,
        );
        compute.shutdown()
    });
    let queries = io.read_client()?;
    let metadata = io.metadata_client()?;
    let pipeline = CandidatePipeline {
        config: cfg,
        sender: Some(sender),
        worker: Some(worker),
        io: Some(io),
        queries: Some(queries),
        metadata: Some(metadata),
        identity,
        usage: Arc::new(Mutex::new(Usage::default())),
    };
    let mut ticket = admitted(&pipeline, request(update.root()));
    assert!(ticket.try_take()?.is_none());
    let mut query = pipeline
        .try_read_value(update.root(), key)?
        .context("query not admitted")?;
    let started = Instant::now();
    loop {
        ensure!(
            started.elapsed() < Duration::from_secs(5),
            "real storage query blocked behind compute latch"
        );
        // Known latch is still held. No time threshold is used to infer that.
        assert!(ticket.try_take()?.is_none());
        if let Some(value) = query.try_take()? {
            assert_eq!(value, Some(vec![19]));
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    release.send(())?;
    let failure = ticket
        .wait()
        .err()
        .context("invalid fixture unexpectedly accepted")?;
    let typed = failure
        .downcast_ref::<PipelineFailure>()
        .context("lost pipeline failure stage")?;
    assert_eq!(typed.stage, FailureStage::Prepare);
    assert!(typed.candidate_id.is_none());
    pipeline.shutdown()?;
    Ok(())
}
