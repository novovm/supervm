use super::super::scope_tests::{scope_request, with_scope_runtime};
use super::*;

fn inspect(client: &AoemSemanticGraphClientV1) -> (u64, thread::ThreadId) {
    let (reply, response) = mpsc::sync_channel(1);
    client.state.commands.send(Command::Inspect(reply)).unwrap();
    response.recv_timeout(Duration::from_secs(5)).unwrap()
}

fn accepted<T, F>(
    admission: AoemSemanticGraphStageAdmissionV1<T, F>,
) -> AoemSemanticGraphStageHandleV1<T> {
    match admission {
        AoemSemanticGraphStageAdmissionV1::Accepted(handle) => handle,
        AoemSemanticGraphStageAdmissionV1::Backpressured(_) => {
            panic!("unexpected test backpressure")
        }
    }
}

fn complete<T>(handle: &mut AoemSemanticGraphStageHandleV1<T>) -> Result<T> {
    let started = Instant::now();
    loop {
        if handle.is_ready() {
            if let Some(value) = handle.try_complete()? {
                return Ok(value);
            }
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "stage completion timed out"
        );
        thread::park_timeout(Duration::from_millis(5));
    }
}

#[test]
fn graph_owner_stage_handle_reports_pending_error_and_disconnection() {
    let (reply, response) = mpsc::sync_channel(1);
    let ready = Arc::new(AtomicBool::new(false));
    let completion = StageReplyV1::<()> {
        reply: Some(reply),
        ready: ready.clone(),
        caller: thread::current(),
    };
    let mut handle = AoemSemanticGraphStageHandleV1 {
        response,
        ready,
        consumed: false,
    };
    assert!(!handle.is_ready());
    assert!(handle.try_complete().unwrap().is_none());
    completion.complete(Err(anyhow::anyhow!("test stage failure")));
    assert!(handle.is_ready());
    assert!(handle
        .try_complete()
        .unwrap_err()
        .to_string()
        .contains("test stage failure"));
    assert!(handle.try_complete().is_err());

    let (reply, response) = mpsc::sync_channel(1);
    let ready = Arc::new(AtomicBool::new(false));
    let completion = StageReplyV1::<()> {
        reply: Some(reply),
        ready: ready.clone(),
        caller: thread::current(),
    };
    let mut handle = AoemSemanticGraphStageHandleV1 {
        response,
        ready,
        consumed: false,
    };
    drop(completion);
    assert!(handle.is_ready());
    assert!(handle
        .try_complete()
        .unwrap_err()
        .to_string()
        .contains("disconnected"));
}

#[test]
#[ignore = "requires bundled AOEM and isolated RocksDB; run runtime tests serially"]
fn graph_owner_real_aoem_stage_reuses_provider_and_rejects_self_send() {
    with_scope_runtime("owner-stage-reuse", |runtime, path| {
        let config = AoemStorageProviderConfigV1::default();
        let owner = AoemSemanticGraphOwnerV1::start(runtime, path, &config).unwrap();
        let client = owner.client();
        let identity = inspect(&client);
        let stage_runtime = runtime.clone();
        let stage_path = path.to_owned();
        let stage_config = config.clone();
        let self_client = client.clone();
        let mut stage = accepted(
            client
                .try_stage(move || {
                    let first =
                        AoemSemanticGraphStoreV1::open(&stage_runtime, &stage_path, &stage_config)?;
                    let alias = stage_path
                        .parent()
                        .unwrap()
                        .join(".")
                        .join("provider.rocksdb");
                    let second =
                        AoemSemanticGraphStoreV1::open(&stage_runtime, &alias, &stage_config)?;
                    assert!(Rc::ptr_eq(first.local_inner(), second.local_inner()));
                    assert_eq!(first.local_inner().database_id, identity.0);
                    assert_eq!(thread::current().id(), identity.1);
                    assert!(AoemSemanticGraphSessionScopeV1::enter().is_err());
                    assert!(self_client.enter().is_err());
                    assert!(self_client.get(b"head").is_err());
                    assert!(self_client
                        .commit(scope_request(920, b"must-not-send"))
                        .is_err());
                    assert!(self_client.try_stage(|| Ok(())).is_err());
                    let mut different = stage_config.clone();
                    different.sync_every += 1;
                    assert!(AoemSemanticGraphStoreV1::open(
                        &stage_runtime,
                        &stage_path,
                        &different
                    )
                    .is_err());
                    assert!(AoemSemanticGraphStoreV1::open(
                        &stage_runtime,
                        &stage_path.with_file_name("wrong-stage-db"),
                        &stage_config
                    )
                    .is_err());
                    let mut malformed = scope_request(921, b"invalid");
                    malformed.steps.clear();
                    assert!(first.commit(malformed).is_err());
                    first.commit(scope_request(922, b"stage-written"))?;
                    second.get(b"head")
                })
                .unwrap(),
        );
        assert_eq!(
            complete(&mut stage).unwrap(),
            Some(b"stage-written".to_vec())
        );
        // A normal closure error is not a provider failure and cannot erase a
        // previous completed write or force a new provider to be opened.
        let mut failed = accepted(
            client
                .try_stage(|| -> Result<()> { bail!("ordinary stage validation rejection") })
                .unwrap(),
        );
        assert!(complete(&mut failed)
            .unwrap_err()
            .to_string()
            .contains("validation rejection"));
        assert_eq!(
            client.get(b"head").unwrap(),
            Some(b"stage-written".to_vec())
        );
        assert_eq!(inspect(&client), identity);
        owner.shutdown().unwrap();
        assert!(client.try_stage(|| Ok(())).is_err());
        let reopened = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        assert_eq!(
            reopened.get(b"head").unwrap(),
            Some(b"stage-written".to_vec())
        );
    });
}

#[test]
#[ignore = "requires bundled AOEM and isolated RocksDB; run runtime tests serially"]
fn graph_owner_real_aoem_stage_backpressure_returns_work_and_drop_does_not_cancel() {
    with_scope_runtime("owner-stage-pressure", |runtime, path| {
        let config = AoemStorageProviderConfigV1::default();
        let owner = AoemSemanticGraphOwnerV1::start(runtime, path, &config).unwrap();
        let client = owner.client();
        let (entered, parked) = mpsc::sync_channel(1);
        let (release, waiting) = mpsc::sync_channel(1);
        let mut running = accepted(
            client
                .try_stage(move || {
                    entered.send(()).unwrap();
                    waiting.recv_timeout(Duration::from_secs(5))?;
                    Ok(())
                })
                .unwrap(),
        );
        parked.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!running.is_ready());
        assert!(running.try_complete().unwrap().is_none());
        let mut queued = Vec::new();
        for value in 0..QUEUE_CAPACITY {
            queued.push(accepted(client.try_stage(move || Ok(value)).unwrap()));
        }
        let called = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let stage_called = called.clone();
        let owned_input = b"returned intact on backpressure".to_vec();
        let returned = match client
            .try_stage(move || {
                stage_called.fetch_add(1, Ordering::SeqCst);
                Ok(owned_input)
            })
            .unwrap()
        {
            AoemSemanticGraphStageAdmissionV1::Backpressured(stage) => stage,
            AoemSemanticGraphStageAdmissionV1::Accepted(_) => panic!("full queue accepted a stage"),
        };
        assert_eq!(called.load(Ordering::SeqCst), 0);
        release.send(()).unwrap();
        complete(&mut running).unwrap();
        for (expected, handle) in queued.iter_mut().enumerate() {
            assert_eq!(complete(handle).unwrap(), expected);
        }
        let mut retried = accepted(client.try_stage(returned).unwrap());
        assert_eq!(
            complete(&mut retried).unwrap(),
            b"returned intact on backpressure"
        );
        assert_eq!(called.load(Ordering::SeqCst), 1);

        let stage_runtime = runtime.clone();
        let stage_path = path.to_owned();
        let stage_config = config.clone();
        let discarded = accepted(
            client
                .try_stage(move || {
                    let store =
                        AoemSemanticGraphStoreV1::open(&stage_runtime, &stage_path, &stage_config)?;
                    store.commit(scope_request(923, b"not-cancelled"))?;
                    Ok(())
                })
                .unwrap(),
        );
        drop(discarded);
        // FIFO owner processing means this real read observes the accepted write
        // even though its completion receiver was deliberately dropped.
        assert_eq!(
            client.get(b"head").unwrap(),
            Some(b"not-cancelled".to_vec())
        );
        owner.shutdown().unwrap();
    });
}

#[test]
#[ignore = "requires bundled AOEM and isolated RocksDB; run runtime tests serially"]
fn graph_owner_real_aoem_stage_panic_rejects_queued_work() {
    with_scope_runtime("owner-stage-panic", |runtime, path| {
        let owner =
            AoemSemanticGraphOwnerV1::start(runtime, path, &AoemStorageProviderConfigV1::default())
                .unwrap();
        let client = owner.client();
        let (entered, parked) = mpsc::sync_channel(1);
        let (release, waiting) = mpsc::sync_channel(1);
        let mut failed = accepted(
            client
                .try_stage(move || -> Result<()> {
                    entered.send(()).unwrap();
                    waiting.recv_timeout(Duration::from_secs(5))?;
                    panic!("injected stage panic");
                })
                .unwrap(),
        );
        parked.recv_timeout(Duration::from_secs(5)).unwrap();
        let called = Arc::new(AtomicBool::new(false));
        let queued_called = called.clone();
        let mut queued = accepted(
            client
                .try_stage(move || {
                    queued_called.store(true, Ordering::Release);
                    Ok(())
                })
                .unwrap(),
        );
        release.send(()).unwrap();
        assert!(complete(&mut failed).is_err());
        assert!(complete(&mut queued).is_err());
        assert!(!called.load(Ordering::Acquire));
        assert!(client.get(b"head").is_err());
        assert!(client.try_stage(|| Ok(())).is_err());
        assert!(owner.shutdown().is_err());
    });
}

#[test]
#[ignore = "requires bundled AOEM and isolated RocksDB; run runtime tests serially"]
fn graph_owner_real_aoem_stage_cannot_hide_shared_poison() {
    with_scope_runtime("owner-stage-poison", |runtime, path| {
        let config = AoemStorageProviderConfigV1::default();
        let owner = AoemSemanticGraphOwnerV1::start(runtime, path, &config).unwrap();
        let client = owner.client();
        let runtime = runtime.clone();
        let path = path.to_owned();
        let mut failed = accepted(
            client
                .try_stage(move || {
                    let store = AoemSemanticGraphStoreV1::open(&runtime, &path, &config)?;
                    store.commit(scope_request(924, b"before-poison"))?;
                    store.local_inner().poisoned.set(true);
                    Ok(()) // Deliberately swallow the injected provider failure.
                })
                .unwrap(),
        );
        assert!(complete(&mut failed)
            .unwrap_err()
            .to_string()
            .contains("poisoned"));
        assert!(client.get(b"head").is_err());
        assert!(client.enter().is_err());
        assert!(owner.shutdown().is_err());
    });
}

#[test]
fn graph_owner_command_bounds_and_send_contract() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<AoemSemanticGraphClientV1>();
    let request = scope_request(900, b"value");
    PreparedGraphV1::new(request.clone()).unwrap();
    let mut empty = request.clone();
    empty.steps.clear();
    assert!(PreparedGraphV1::new(empty).is_err());
    let mut many = request.clone();
    many.steps = vec![request.steps[0].clone(); 8_193];
    assert!(
        PreparedGraphV1::new(many).is_ok(),
        "remote admission must not invent a smaller graph limit"
    );
    let mut large = request;
    large.steps[0].task_payload = vec![0; MAX_TASK_PAYLOAD_BYTES_V1 + 1];
    assert!(PreparedGraphV1::new(large).is_err());
}

#[test]
#[ignore = "requires bundled AOEM and isolated RocksDB; run runtime tests serially"]
fn graph_owner_real_aoem_two_threads_one_provider_and_shutdown() {
    with_scope_runtime("remote-reuse", |runtime, path| {
        let config = AoemStorageProviderConfigV1::default();
        let owner = AoemSemanticGraphOwnerV1::start(runtime, path, &config).unwrap();
        let client = owner.client();
        let identity = inspect(&client);
        assert_ne!(identity.0, 0);
        assert_ne!(identity.1, thread::current().id());
        let scope = client.enter().unwrap();
        assert!(client.enter().is_err());
        assert!(AoemSemanticGraphSessionScopeV1::enter().is_err());
        let store = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        assert!(matches!(store.inner, GraphStoreBackendV1::Remote(_)));
        store.commit(scope_request(901, b"main")).unwrap();
        let worker_client = client.clone();
        let worker_runtime = runtime.clone();
        let worker_path = path.parent().unwrap().join(".").join("provider.rocksdb");
        let worker_config = config.clone();
        let result = thread::spawn(move || {
            let _scope = worker_client.enter().unwrap();
            let store =
                AoemSemanticGraphStoreV1::open(&worker_runtime, &worker_path, &worker_config)
                    .unwrap();
            assert_eq!(store.get(b"head").unwrap(), Some(b"main".to_vec()));
            store.commit(scope_request(902, b"worker")).unwrap();
            inspect(&worker_client)
        })
        .join()
        .unwrap();
        assert_eq!(
            identity, result,
            "both threads must reach the same real DB and storage thread"
        );
        assert_eq!(store.get(b"head").unwrap(), Some(b"worker".to_vec()));
        owner.shutdown().unwrap();
        assert!(store.get(b"head").is_err());
        assert!(store.commit(scope_request(903, b"after-stop")).is_err());
        assert!(AoemSemanticGraphStoreV1::open(runtime, path, &config).is_err());
        drop(store);
        drop(scope);
        assert!(client.enter().is_err());
        let reopened = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        assert_eq!(reopened.get(b"head").unwrap(), Some(b"worker".to_vec()));
    });
}

#[test]
#[ignore = "requires bundled AOEM and isolated RocksDB; run runtime tests serially"]
fn graph_owner_real_aoem_rejects_identity_drift_and_propagates_poison() {
    with_scope_runtime("remote-poison", |runtime, path| {
        let config = AoemStorageProviderConfigV1::default();
        let local = AoemSemanticGraphSessionScopeV1::enter().unwrap();
        assert!(AoemSemanticGraphOwnerV1::start(runtime, path, &config).is_err());
        drop(local);
        let owner = AoemSemanticGraphOwnerV1::start(runtime, path, &config).unwrap();
        let client = owner.client();
        let local = AoemSemanticGraphSessionScopeV1::enter().unwrap();
        assert!(client.enter().is_err());
        drop(local);
        let scope = client.enter().unwrap();
        let store = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        let mut different = config.clone();
        different.sync_every += 1;
        assert!(AoemSemanticGraphStoreV1::open(runtime, path, &different).is_err());
        let mut different = runtime.clone();
        different.ingress_workers = Some(runtime.ingress_workers.unwrap_or(1) + 1);
        assert!(AoemSemanticGraphStoreV1::open(&different, path, &config).is_err());
        let other = path.with_file_name("wrong.rocksdb");
        assert!(AoemSemanticGraphStoreV1::open(runtime, &other, &config).is_err());
        assert!(!other.exists());
        std::env::set_var("AOEM_FFI_GLOBAL_BUDGET", "47");
        assert!(AoemSemanticGraphStoreV1::open(runtime, path, &config).is_err());
        std::env::remove_var("AOEM_FFI_GLOBAL_BUDGET");
        let mut invalid = scope_request(904, b"invalid");
        invalid.steps[0].writes = vec![AoemAtomicGraphWriteV1::Put {
            key: b"value".to_vec(),
            value: vec![0; MAX_ATOMIC_WRITE_VALUE_BYTES_V1 + 1],
        }];
        assert!(
            store.commit(invalid).is_err(),
            "pre-admission invalid graph stays an error"
        );
        store.commit(scope_request(905, b"valid")).unwrap();
        assert_eq!(store.get(b"head").unwrap(), Some(b"valid".to_vec()));
        let (reply, result) = mpsc::sync_channel(1);
        client.state.commands.send(Command::Poison(reply)).unwrap();
        result.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(store.get(b"head").is_err());
        assert!(store.commit(scope_request(906, b"poisoned")).is_err());
        let other_client = client.clone();
        assert!(thread::spawn(move || other_client.enter().is_err())
            .join()
            .unwrap());
        assert!(AoemSemanticGraphStoreV1::open(runtime, path, &config).is_err());
        drop(store);
        drop(scope);
        assert!(
            owner.shutdown().is_err(),
            "shutdown reports the sticky failure"
        );
    });
}

#[test]
#[ignore = "requires bundled AOEM and isolated RocksDB; run runtime tests serially"]
fn graph_owner_real_aoem_queue_bound_scope_unwind_and_worker_failure() {
    with_scope_runtime("remote-queue", |runtime, path| {
        let owner =
            AoemSemanticGraphOwnerV1::start(runtime, path, &AoemStorageProviderConfigV1::default())
                .unwrap();
        let client = owner.client();
        assert!(catch_unwind(AssertUnwindSafe(|| {
            let _scope = client.enter().unwrap();
            panic!("unwind only the calling thread's remote scope");
        }))
        .is_err());
        let scope = client.enter().unwrap();
        drop(scope);
        let (entered, parked) = mpsc::sync_channel(1);
        let (release, waiting) = mpsc::sync_channel(1);
        client
            .state
            .commands
            .send(Command::Park(entered, waiting))
            .unwrap();
        parked.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut responses = Vec::new();
        for _ in 0..QUEUE_CAPACITY {
            let (reply, result) = mpsc::sync_channel(1);
            client
                .state
                .commands
                .try_send(Command::Get(b"missing".to_vec(), reply))
                .unwrap();
            responses.push(result);
        }
        let (reply, _) = mpsc::sync_channel(1);
        assert!(matches!(
            client
                .state
                .commands
                .try_send(Command::Get(b"overflow".to_vec(), reply)),
            Err(mpsc::TrySendError::Full(_))
        ));
        release.send(()).unwrap();
        for response in responses {
            assert_eq!(
                response
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .unwrap(),
                None
            );
        }
        client.state.commands.send(Command::Panic).unwrap();
        // A request after the injected panic either sees sticky failure or its
        // reply channel closes; it must not hang or silently create a new DB.
        assert!(client.get(b"head").is_err());
        assert!(owner.shutdown().is_err());
        assert!(client.enter().is_err());
    });
}
