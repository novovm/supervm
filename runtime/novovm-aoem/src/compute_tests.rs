use super::*;

fn test_library() -> std::path::PathBuf {
    std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .expect("set NOVOVM_AOEM_TEST_LIBRARY to the explicit trusted AOEM DLL/SO")
        .into()
}

fn execute_once(path: &Path, tasks: Vec<ComputeTask>, timeout: Duration) -> Result<ComputeReport> {
    validate_task_batch(&tasks, timeout)?;
    ComputeSession::open(path, 4)?.execute(tasks, timeout)
}

fn context(tasks: Vec<ComputeTask>) -> Arc<ComputeContext> {
    Arc::new(ComputeContext::new(7, tasks, mpsc::channel().0))
}

fn run(context: &Arc<ComputeContext>, index: u64) -> i32 {
    let task = TaskDescriptor {
        graph_id: 7,
        task_id: index + 1,
        context_handle: index + 1,
        sequence: index,
        ..TaskDescriptor::default()
    };
    let mut output = StepOutput::default();
    let status =
        unsafe { execute_task(&task, &mut output, Arc::as_ptr(context).cast_mut().cast()) };
    assert_eq!(
        output.flags, 0,
        "compute must not generate writes or events"
    );
    status
}

#[test]
fn compute_callbacks_execute_once_and_keep_input_order() {
    let context = context(vec![Box::new(|| Ok(vec![0])), Box::new(|| Ok(vec![1]))]);
    assert_eq!(run(&context, 1), OK);
    assert_eq!(run(&context, 0), OK);
    assert_eq!(run(&context, 0), GRAPH_FAULTED);
    assert_eq!(*context.slots[0].output.lock().unwrap(), Some(vec![0]));
    assert_eq!(*context.slots[1].output.lock().unwrap(), Some(vec![1]));
    assert_eq!(run(&context, 2), INVALID_ARGUMENT);
}

#[test]
fn compute_callback_contains_panics_and_errors() {
    let context = context(vec![
        Box::new(|| panic!("test panic")),
        Box::new(|| bail!("test error")),
    ]);
    assert_eq!(run(&context, 0), CALLBACK_PANICKED);
    assert_eq!(context.inflight.load(Ordering::Acquire), 0);
    assert_eq!(run(&context, 1), GRAPH_FAULTED);
    assert!(context
        .slots
        .iter()
        .all(|slot| slot.output.lock().unwrap().is_none()));
}

#[test]
fn compute_rejects_invalid_descriptor_and_nulls() {
    let context = context(vec![Box::new(|| Ok(vec![1]))]);
    let mut output = StepOutput::default();
    let mut task = TaskDescriptor {
        graph_id: 8,
        task_id: 1,
        context_handle: 1,
        ..Default::default()
    };
    let raw = Arc::as_ptr(&context).cast_mut().cast();
    unsafe {
        assert_eq!(execute_task(&task, &mut output, raw), INVALID_ARGUMENT);
        task.graph_id = 7;
        task.sequence = 1;
        assert_eq!(execute_task(&task, &mut output, raw), INVALID_ARGUMENT);
        assert_eq!(
            execute_task(std::ptr::null(), &mut output, raw),
            INVALID_ARGUMENT
        );
        assert_eq!(validate_handle(0, raw), INVALID_ARGUMENT);
    }
    assert!(context.slots[0].task.lock().unwrap().is_some());
}

#[test]
fn compute_completion_is_owned_and_single_delivery() {
    let (sender, receiver) = mpsc::channel();
    let context = Arc::new(ComputeContext::new(
        7,
        vec![Box::new(|| Ok(vec![]))],
        sender,
    ));
    let completion = Completion {
        graph_id: 7,
        ..Default::default()
    };
    let raw = Arc::as_ptr(&context).cast_mut().cast();
    unsafe {
        complete_graph(&completion, raw);
        complete_graph(&completion, raw);
    }
    assert_eq!(receiver.try_recv().unwrap().graph_id, 7);
    assert!(receiver.try_recv().is_err());
    assert_eq!(Arc::strong_count(&context), 1);
}

#[test]
fn compute_rejects_invalid_wave_before_loading_runtime() {
    let runtime = Path::new("missing-compute-test-runtime.dll");
    let empty = execute_once(runtime, vec![], Duration::from_secs(1));
    assert!(empty.unwrap_err().to_string().contains("requires"));
    let zero = execute_once(runtime, vec![Box::new(|| Ok(vec![]))], Duration::ZERO);
    assert!(zero.unwrap_err().to_string().contains("requires"));
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; real session reuse acceptance"]
fn compute_session_reuses_graphs_and_preserves_poison_boundary() {
    let runtime = test_library();
    let mut session = ComputeSession::open(&runtime, 4).expect("compute owner");
    let weak = Rc::downgrade(&session.inner);
    let identity = Rc::as_ptr(&session.inner);
    let first = session
        .execute(vec![Box::new(|| Ok(vec![1]))], Duration::from_secs(5))
        .expect("first graph");
    assert_eq!(first.outputs, vec![vec![1]]);
    assert!(session.execute(vec![], Duration::from_secs(1)).is_err());
    assert!(session
        .execute(vec![Box::new(|| Ok(vec![0]))], Duration::ZERO)
        .is_err());
    assert!(!session.inner.poisoned.get());
    let second = session
        .execute(
            vec![Box::new(|| Ok(vec![2])), Box::new(|| Ok(vec![3]))],
            Duration::from_secs(5),
        )
        .expect("second graph after pre-admission rejection");
    assert_eq!(second.outputs, vec![vec![2], vec![3]]);
    assert_eq!(
        (second.processed, second.succeeded, second.failed),
        (2, 2, 0)
    );
    assert_eq!(Rc::as_ptr(&session.inner), identity);
    assert_eq!(
        Rc::strong_count(&session.inner),
        1,
        "drained flights released"
    );
    assert!(session
        .execute(
            vec![Box::new(|| bail!("task failure"))],
            Duration::from_secs(5)
        )
        .is_err());
    assert!(session.inner.poisoned.get());
    let called = Arc::new(AtomicBool::new(false));
    let task_called = Arc::clone(&called);
    let error = session
        .execute(
            vec![Box::new(move || {
                task_called.store(true, Ordering::Release);
                Ok(vec![])
            })],
            Duration::from_secs(5),
        )
        .expect_err("a failed owner cannot silently reopen");
    assert!(error.to_string().contains("poisoned"));
    assert!(!called.load(Ordering::Acquire));
    assert_eq!(Rc::as_ptr(&session.inner), identity);
    drop(session);
    assert!(
        weak.upgrade().is_none(),
        "drained failed owner may be released"
    );
}

fn wait_for(flag: &AtomicBool) {
    let started = Instant::now();
    while !flag.load(Ordering::Acquire) {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "callback did not progress"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

struct CaptureDrop(Arc<AtomicBool>);
impl Drop for CaptureDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

fn held_task(
    started: Arc<AtomicBool>,
    release: Arc<AtomicBool>,
    dropped: Arc<AtomicBool>,
) -> ComputeTask {
    let capture = CaptureDrop(dropped);
    Box::new(move || {
        let _capture = capture;
        started.store(true, Ordering::Release);
        let waiting = Instant::now();
        while !release.load(Ordering::Acquire) {
            if waiting.elapsed() > Duration::from_secs(10) {
                bail!("test callback release deadline");
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(vec![9])
    })
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; deliberately retains an undrained cancelled owner"]
fn compute_session_timeout_retains_the_complete_flight() {
    let runtime = test_library();
    let mut session = ComputeSession::open(&runtime, 4).expect("compute owner");
    let weak = Rc::downgrade(&session.inner);
    let started = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let dropped = Arc::new(AtomicBool::new(false));
    let error = session
        .execute_with_checkpoint(
            vec![held_task(started.clone(), release.clone(), dropped.clone())],
            Duration::from_millis(1),
            || wait_for(&started),
        )
        .expect_err("timeout cannot become a successful graph");
    assert!(error.to_string().contains("owner retained"), "{error:#}");
    assert!(session.inner.poisoned.get());
    assert!(!dropped.load(Ordering::Acquire));
    assert!(session
        .execute(vec![Box::new(|| Ok(vec![]))], Duration::from_secs(1))
        .unwrap_err()
        .to_string()
        .contains("poisoned"));
    drop(session);
    assert!(
        weak.upgrade().is_some(),
        "session/DLL remain with callback data"
    );
    release.store(true, Ordering::Release);
    wait_for(&dropped);
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; deliberately retains a Host-unwound live graph"]
fn compute_session_host_unwind_retains_the_complete_flight() {
    let runtime = test_library();
    let mut session = ComputeSession::open(&runtime, 4).expect("compute owner");
    let weak = Rc::downgrade(&session.inner);
    let started = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let dropped = Arc::new(AtomicBool::new(false));
    let result = catch_unwind(AssertUnwindSafe(|| {
        session.execute_with_checkpoint(
            vec![held_task(started.clone(), release.clone(), dropped.clone())],
            Duration::from_secs(5),
            || {
                wait_for(&started);
                panic!("injected Host unwind after live FFI admission");
            },
        )
    }));
    assert!(result.is_err());
    assert!(session.inner.poisoned.get());
    assert!(!dropped.load(Ordering::Acquire));
    assert!(session
        .execute(vec![Box::new(|| Ok(vec![]))], Duration::from_secs(1))
        .unwrap_err()
        .to_string()
        .contains("poisoned"));
    drop(session);
    assert!(
        weak.upgrade().is_some(),
        "unwinding cannot unload the live DLL"
    );
    release.store(true, Ordering::Release);
    wait_for(&dropped);
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; explicitly run for real worker acceptance"]
fn compute_real_aoem_parallel_order_panic_and_cancel() {
    let runtime = test_library();
    let owner_thread = std::thread::current().id();
    // Real deterministic CPU work, not a sleep/barrier engineered to raise the
    // overlap counter. This is a compute-adapter test, not a throughput claim.
    fn cpu_work(seed: u64) -> Vec<u8> {
        let mut value = seed ^ 0x9e37_79b9_7f4a_7c15;
        for round in 0..65_536_u64 {
            value = std::hint::black_box(value)
                .rotate_left(17)
                .wrapping_mul(0x2545_f491_4f6c_dd1d)
                .wrapping_add(round);
        }
        value.to_le_bytes().to_vec()
    }
    let expected = (0..32).map(cpu_work).collect::<Vec<_>>();
    let tasks = (0..32)
        .map(|index| {
            Box::new(move || {
                assert_ne!(std::thread::current().id(), owner_thread);
                Ok(cpu_work(index))
            }) as ComputeTask
        })
        .collect();
    let report =
        execute_once(runtime.as_ref(), tasks, Duration::from_secs(5)).expect("compute-only V2");
    assert!((1..=32).contains(&report.peak_inflight));
    assert_eq!(report.outputs, expected);
    assert_eq!(
        (report.processed, report.succeeded, report.failed),
        (32, 32, 0)
    );
    eprintln!("real AOEM compute peak_inflight={}", report.peak_inflight);
    assert!(execute_once(
        &runtime,
        vec![Box::new(|| panic!("contained AOEM worker panic"))],
        Duration::from_secs(5)
    )
    .is_err());
    assert!(execute_once(
        &runtime,
        vec![Box::new(|| {
            std::thread::sleep(Duration::from_millis(100));
            Ok(vec![9])
        })],
        Duration::from_millis(1)
    )
    .is_err());
    // ingress_workers configures ingress, not the semantic worker pool.
    // Single-task waves are a genuine serial reference without pretending
    // that the ingress setting can disable semantic scheduling concurrency.
    let mut serial_outputs = Vec::new();
    let mut serial_session = ComputeSession::open(&runtime, 4).expect("serial reference owner");
    for index in 0..32 {
        let report = serial_session
            .execute(
                vec![Box::new(move || Ok(cpu_work(index)))],
                Duration::from_secs(5),
            )
            .expect("single-task wave on the explicit reference owner");
        assert_eq!(report.peak_inflight, 1);
        serial_outputs.extend(report.outputs);
    }
    assert_eq!(serial_outputs, report.outputs);
}

#[test]
fn compute_result_budget_is_checked_before_retaining_an_output() {
    let context = context(vec![Box::new(|| Ok(vec![1]))]);
    context
        .result_bytes
        .store(MAX_RESULT_BYTES, Ordering::Release);
    assert_eq!(run(&context, 0), GRAPH_FAULTED);
    assert!(context.slots[0].output.lock().unwrap().is_none());
    assert_eq!(
        context.result_bytes.load(Ordering::Acquire),
        MAX_RESULT_BYTES
    );
    assert_eq!(context.inflight.load(Ordering::Acquire), 0);
}

#[test]
fn compute_rejects_task_count_overflow_before_loading_a_library() {
    let tasks = (0..=MAX_TASKS)
        .map(|_| Box::new(|| Ok(Vec::new())) as ComputeTask)
        .collect::<Vec<_>>();
    assert!(validate_task_batch(&tasks, Duration::from_secs(1)).is_err());
}

#[test]
fn compute_catches_a_panic_payload_with_a_panicking_destructor() {
    struct BadDrop;
    impl Drop for BadDrop {
        fn drop(&mut self) {
            panic!("must never unwind from a callback panic payload destructor");
        }
    }
    let context = context(vec![Box::new(|| std::panic::panic_any(BadDrop))]);
    assert_eq!(run(&context, 0), CALLBACK_PANICKED);
    assert_eq!(context.inflight.load(Ordering::Acquire), 0);
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; deliberately retains one cancelled owner"]
fn compute_real_aoem_undrained_timeout_retains_owned_capture() {
    let runtime = test_library();
    let executing = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicUsize::new(0));
    let task_executing = executing.clone();
    let task_finished = finished.clone();
    let result = execute_once(
        &runtime,
        vec![Box::new(move || {
            task_executing.store(1, Ordering::Release);
            std::thread::sleep(Duration::from_millis(2_500));
            task_finished.store(1, Ordering::Release);
            Ok(vec![42])
        })],
        Duration::from_millis(100),
    );
    let error = result.expect_err("late output cannot turn timeout into success");
    assert!(error.to_string().contains("owner retained"), "{error:#}");
    assert_eq!(executing.load(Ordering::Acquire), 1);
    assert_eq!(finished.load(Ordering::Acquire), 0);
    let started = Instant::now();
    while finished.load(Ordering::Acquire) == 0 {
        assert!(started.elapsed() < Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(5));
    }
    // The original caller and stack are gone; its owned task can finish
    // without accessing freed captures, callback pointers or an unloaded DLL.
    std::thread::sleep(Duration::from_millis(10));
}
