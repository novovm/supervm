//! Owned, domain-neutral computation on AOEM's existing V2 semantic workers.
//! This adapter neither opens a state database nor binds an atomic writer.
//! Tasks must compute isolated results: cancellation cannot undo caller side effects.

use super::{AoemExecFacade, AoemExecSession, AoemRuntimeConfig};
use anyhow::{bail, Context, Result};
use aoem_bindings::{
    AoemGraphCallbacksV2, AoemGraphCompletionV2, AoemGraphSubmitOptionsV2, AoemStateEventV2,
    AoemTaskDescriptorV2, AoemTaskStepOutputV2, AOEM_ERROR_CALLBACK_PANICKED,
    AOEM_ERROR_GRAPH_FAULTED, AOEM_ERROR_INVALID_ARGUMENT, AOEM_SEMANTIC_GRAPH_ABI_V2,
    AOEM_STATUS_OK,
};
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_TASKS: usize = 65_536;
const MAX_RESULT_BYTES: usize = 64 * 1024 * 1024;
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
static NEXT_GRAPH_ID: AtomicU64 = AtomicU64::new(1);

/// A Host-owned computation, not a precomputed KV write or an AOEM business opcode.
/// Owned captures are required because cancellation may outlive the caller.
pub type AoemComputeTaskV1 = Box<dyn FnOnce() -> Result<Vec<u8>> + Send + 'static>;

#[derive(Debug, PartialEq, Eq)]
pub struct AoemComputeReportV1 {
    /// Results in input order, independent of worker completion order.
    pub outputs: Vec<Vec<u8>>,
    pub processed: u64,
    pub succeeded: u64,
    pub failed: u64,
    /// Actual simultaneously executing callbacks, not queued descriptors.
    pub peak_inflight: usize,
}

struct TaskSlot {
    task: Mutex<Option<AoemComputeTaskV1>>,
    output: Mutex<Option<Vec<u8>>>,
}

struct ComputeContext {
    graph_id: u64,
    slots: Vec<TaskSlot>,
    inflight: AtomicUsize,
    peak: AtomicUsize,
    result_bytes: AtomicUsize,
    completion_seen: AtomicBool,
    error: Mutex<Option<String>>,
    completion_tx: Mutex<Option<mpsc::Sender<AoemGraphCompletionV2>>>,
}

impl ComputeContext {
    fn new(
        graph_id: u64,
        tasks: Vec<AoemComputeTaskV1>,
        completion_tx: mpsc::Sender<AoemGraphCompletionV2>,
    ) -> Self {
        Self {
            graph_id,
            slots: tasks
                .into_iter()
                .map(|task| TaskSlot {
                    task: Mutex::new(Some(task)),
                    output: Mutex::new(None),
                })
                .collect(),
            inflight: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            result_bytes: AtomicUsize::new(0),
            completion_seen: AtomicBool::new(false),
            error: Mutex::new(None),
            completion_tx: Mutex::new(Some(completion_tx)),
        }
    }

    fn record_error(&self, message: String) {
        let mut error = self.error.lock().unwrap_or_else(|error| error.into_inner());
        if error.is_none() {
            *error = Some(message);
        }
    }

    fn index(&self, handle: u64) -> Option<usize> {
        usize::try_from(handle.checked_sub(1)?)
            .ok()
            .filter(|index| *index < self.slots.len())
    }
}

/// Owns every object reachable by asynchronous FFI, including the loaded DLL.
/// Do not release only the context when cancellation cannot drain the session.
struct ComputeOwner {
    // Rust drops fields in declaration order: destroy/drain the AOEM session
    // while callback data and descriptor storage are still owned and valid.
    session: AoemExecSession,
    context: Arc<ComputeContext>,
    seeds: Vec<AoemTaskDescriptorV2>,
    options: AoemGraphSubmitOptionsV2,
    callbacks: AoemGraphCallbacksV2,
}

/// Executes one bounded independent wave on a dedicated AOEM session.
///
/// No storage provider, atomic writes, authority marker, or fallback thread pool
/// is used. Callers own dependency analysis and deterministic state publication.
/// Any task error, panic, invalid completion, or deadline returns no successful
/// batch. A session that cannot drain after cancellation is deliberately retained
/// with its DLL and all captures; this is a fail-closed resource leak, not UAF.
pub fn execute_aoem_compute_tasks_v1(
    runtime: &AoemRuntimeConfig,
    tasks: Vec<AoemComputeTaskV1>,
    timeout: Duration,
) -> Result<AoemComputeReportV1> {
    if tasks.is_empty() || tasks.len() > MAX_TASKS || timeout.is_zero() {
        bail!("AOEM compute requires 1..={MAX_TASKS} tasks and a nonzero deadline");
    }
    let graph_id = NEXT_GRAPH_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .map_err(|_| anyhow::anyhow!("AOEM compute graph identifiers exhausted"))?;
    let facade =
        AoemExecFacade::open_with_runtime(runtime).context("open AOEM compute runtime failed")?;
    let session = facade.create_session()?;
    if !session.handle.supports_semantic_graph_v2() {
        bail!("AOEM domain-neutral semantic graph V2 symbols unavailable");
    }
    let (sender, receiver) = mpsc::channel();
    let context = Arc::new(ComputeContext::new(graph_id, tasks, sender));
    let seeds = (0..context.slots.len())
        .map(|index| AoemTaskDescriptorV2 {
            graph_id,
            task_id: index as u64 + 1,
            context_handle: index as u64 + 1,
            sequence: index as u64,
            ..AoemTaskDescriptorV2::default()
        })
        .collect::<Vec<_>>();
    let owner = Box::new(ComputeOwner {
        session,
        options: AoemGraphSubmitOptionsV2 {
            max_queued_tasks: seeds.len().max(2) as u32,
            event_capacity: 1,
            ..AoemGraphSubmitOptionsV2::default()
        },
        callbacks: AoemGraphCallbacksV2 {
            execute: Some(execute_task),
            retain_context: Some(validate_handle),
            release_context: Some(validate_handle),
            state_event: Some(reject_event),
            completion: Some(complete_graph),
            user_data: Arc::as_ptr(&context).cast_mut().cast(),
        },
        context,
        seeds,
    });
    let submitted = unsafe {
        owner.session.handle.submit_semantic_graph_v2(
            &owner.seeds,
            &owner.options,
            &owner.callbacks,
        )
    };
    if !matches!(submitted, Ok(AOEM_STATUS_OK)) {
        let _ = owner.session.cancel_semantic_graph_v2(graph_id);
        if !drained(&owner, DRAIN_TIMEOUT) {
            let _retained = Box::into_raw(owner);
        }
        bail!("AOEM compute-only graph admission failed: {submitted:?}");
    }
    let completion = match receiver.recv_timeout(timeout) {
        Ok(completion) => completion,
        Err(error) => {
            let _ = owner.session.cancel_semantic_graph_v2(graph_id);
            // The caller's timeout remains failure even if cancellation races success.
            if !drained(&owner, DRAIN_TIMEOUT) {
                let _retained = Box::into_raw(owner);
                bail!("AOEM compute deadline/channel failure ({error}); undrained owner retained");
            }
            bail!("AOEM compute deadline/channel failure: {error}");
        }
    };
    if !drained(&owner, DRAIN_TIMEOUT) {
        let _ = owner.session.cancel_semantic_graph_v2(graph_id);
        let _retained = Box::into_raw(owner);
        bail!("AOEM compute completion did not drain; owner retained");
    }
    let context = &owner.context;
    if let Some(error) = context
        .error
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        bail!("AOEM compute task failed: {error}");
    }
    let count = owner.seeds.len() as u64;
    if completion.abi_version != AOEM_SEMANTIC_GRAPH_ABI_V2
        || completion.graph_id != graph_id
        || completion.status != AOEM_STATUS_OK
        || completion.processed != count
        || completion.succeeded != count
        || completion.failed != 0
    {
        bail!("AOEM compute completion contract mismatch: {completion:?}");
    }
    let outputs = context
        .slots
        .iter()
        .map(|slot| {
            slot.output
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
                .context("AOEM compute completed without a task result")
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(AoemComputeReportV1 {
        outputs,
        processed: completion.processed,
        succeeded: completion.succeeded,
        failed: completion.failed,
        peak_inflight: context.peak.load(Ordering::Acquire),
    })
}

fn drained(owner: &ComputeOwner, timeout: Duration) -> bool {
    let started = Instant::now();
    loop {
        // A zero active count is not a guarantee that completion has started.
        if owner.context.completion_seen.load(Ordering::Acquire)
            && matches!(owner.session.semantic_graph_v2_active_count(), Ok(0))
            && owner.context.inflight.load(Ordering::Acquire) == 0
            && Arc::strong_count(&owner.context) == 1
        {
            return true;
        }
        if started.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn ffi_status(action: impl FnOnce() -> i32) -> i32 {
    match catch_unwind(AssertUnwindSafe(action)) {
        Ok(status) => status,
        Err(payload) => {
            // A user panic payload may itself have a panicking destructor.
            std::mem::forget(payload);
            AOEM_ERROR_CALLBACK_PANICKED
        }
    }
}

struct Inflight<'a>(&'a ComputeContext);

impl Drop for Inflight<'_> {
    fn drop(&mut self) {
        self.0.inflight.fetch_sub(1, Ordering::AcqRel);
    }
}

unsafe extern "C-unwind" fn execute_task(
    descriptor: *const AoemTaskDescriptorV2,
    output: *mut AoemTaskStepOutputV2,
    user_data: *mut c_void,
) -> i32 {
    ffi_status(|| {
        let (Some(context), Some(descriptor), Some(output)) = (
            user_data.cast::<ComputeContext>().as_ref(),
            descriptor.as_ref(),
            output.as_mut(),
        ) else {
            return AOEM_ERROR_INVALID_ARGUMENT;
        };
        *output = AoemTaskStepOutputV2::default();
        let Some(index) = context.index(descriptor.context_handle) else {
            return AOEM_ERROR_INVALID_ARGUMENT;
        };
        if descriptor.abi_version != AOEM_SEMANTIC_GRAPH_ABI_V2
            || descriptor.graph_id != context.graph_id
            || descriptor.task_id != index as u64 + 1
            || descriptor.sequence != index as u64
            || descriptor.payload_len != 0
        {
            return AOEM_ERROR_INVALID_ARGUMENT;
        }
        let slot = &context.slots[index];
        let task = slot.task.lock().unwrap_or_else(|e| e.into_inner()).take();
        let Some(task) = task else {
            context.record_error("duplicate task execution".into());
            return AOEM_ERROR_GRAPH_FAULTED;
        };
        let inflight = context.inflight.fetch_add(1, Ordering::AcqRel) + 1;
        context.peak.fetch_max(inflight, Ordering::AcqRel);
        let _inflight = Inflight(context);
        let result = match task() {
            Ok(output) => output,
            Err(error) => {
                context.record_error(format!("task {index}: {error:#}"));
                return AOEM_ERROR_GRAPH_FAULTED;
            }
        };
        if context
            .result_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |total| {
                total
                    .checked_add(result.len())
                    .filter(|bytes| *bytes <= MAX_RESULT_BYTES)
            })
            .is_err()
        {
            context.record_error("compute wave result byte budget exceeded".into());
            return AOEM_ERROR_GRAPH_FAULTED;
        }
        *slot.output.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
        AOEM_STATUS_OK
    })
}

unsafe extern "C-unwind" fn validate_handle(handle: u64, user_data: *mut c_void) -> i32 {
    ffi_status(|| match user_data.cast::<ComputeContext>().as_ref() {
        Some(context) if context.index(handle).is_some() => AOEM_STATUS_OK,
        _ => AOEM_ERROR_INVALID_ARGUMENT,
    })
}

unsafe extern "C-unwind" fn complete_graph(
    completion: *const AoemGraphCompletionV2,
    user_data: *mut c_void,
) {
    let _ = ffi_status(|| {
        let raw = user_data.cast::<ComputeContext>();
        if raw.is_null() {
            return AOEM_ERROR_INVALID_ARGUMENT;
        }
        // Keep context alive after waking the caller and until this callback exits.
        Arc::increment_strong_count(raw);
        let context = Arc::from_raw(raw);
        let Some(completion) = completion.as_ref() else {
            return AOEM_ERROR_INVALID_ARGUMENT;
        };
        if let Some(sender) = context
            .completion_tx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = sender.send(*completion);
        }
        context.completion_seen.store(true, Ordering::Release);
        AOEM_STATUS_OK
    });
}

unsafe extern "C-unwind" fn reject_event(_: *const AoemStateEventV2, _: *mut c_void) -> i32 {
    AOEM_ERROR_INVALID_ARGUMENT
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(tasks: Vec<AoemComputeTaskV1>) -> Arc<ComputeContext> {
        Arc::new(ComputeContext::new(7, tasks, mpsc::channel().0))
    }

    fn run(context: &Arc<ComputeContext>, index: u64) -> i32 {
        let task = AoemTaskDescriptorV2 {
            graph_id: 7,
            task_id: index + 1,
            context_handle: index + 1,
            sequence: index,
            ..AoemTaskDescriptorV2::default()
        };
        let mut output = AoemTaskStepOutputV2::default();
        let status =
            unsafe { execute_task(&task, &mut output, Arc::as_ptr(context).cast_mut().cast()) };
        assert_eq!(
            output.flags, 0,
            "compute must not generate writes or events"
        );
        status
    }

    #[test]
    fn semantic_compute_callbacks_execute_once_and_keep_input_order() {
        let context = context(vec![Box::new(|| Ok(vec![0])), Box::new(|| Ok(vec![1]))]);
        assert_eq!(run(&context, 1), AOEM_STATUS_OK);
        assert_eq!(run(&context, 0), AOEM_STATUS_OK);
        assert_eq!(run(&context, 0), AOEM_ERROR_GRAPH_FAULTED);
        assert_eq!(*context.slots[0].output.lock().unwrap(), Some(vec![0]));
        assert_eq!(*context.slots[1].output.lock().unwrap(), Some(vec![1]));
        assert_eq!(run(&context, 2), AOEM_ERROR_INVALID_ARGUMENT);
    }

    #[test]
    fn semantic_compute_callback_contains_panics_and_errors() {
        let context = context(vec![
            Box::new(|| panic!("test panic")),
            Box::new(|| bail!("test error")),
        ]);
        assert_eq!(run(&context, 0), AOEM_ERROR_CALLBACK_PANICKED);
        assert_eq!(context.inflight.load(Ordering::Acquire), 0);
        assert_eq!(run(&context, 1), AOEM_ERROR_GRAPH_FAULTED);
        assert!(context
            .slots
            .iter()
            .all(|slot| slot.output.lock().unwrap().is_none()));
    }

    #[test]
    fn semantic_compute_rejects_invalid_descriptor_and_nulls() {
        let context = context(vec![Box::new(|| Ok(vec![1]))]);
        let mut output = AoemTaskStepOutputV2::default();
        let mut task = AoemTaskDescriptorV2 {
            graph_id: 8,
            task_id: 1,
            context_handle: 1,
            ..Default::default()
        };
        let raw = Arc::as_ptr(&context).cast_mut().cast();
        unsafe {
            assert_eq!(
                execute_task(&task, &mut output, raw),
                AOEM_ERROR_INVALID_ARGUMENT
            );
            task.graph_id = 7;
            task.sequence = 1;
            assert_eq!(
                execute_task(&task, &mut output, raw),
                AOEM_ERROR_INVALID_ARGUMENT
            );
            assert_eq!(
                execute_task(std::ptr::null(), &mut output, raw),
                AOEM_ERROR_INVALID_ARGUMENT
            );
            assert_eq!(validate_handle(0, raw), AOEM_ERROR_INVALID_ARGUMENT);
        }
        assert!(context.slots[0].task.lock().unwrap().is_some());
    }

    #[test]
    fn semantic_compute_completion_is_owned_and_single_delivery() {
        let (sender, receiver) = mpsc::channel();
        let context = Arc::new(ComputeContext::new(
            7,
            vec![Box::new(|| Ok(vec![]))],
            sender,
        ));
        let completion = AoemGraphCompletionV2 {
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
    fn semantic_compute_rejects_invalid_wave_before_loading_runtime() {
        let mut runtime = AoemRuntimeConfig::from_env().unwrap();
        runtime.dll_path = "missing-compute-test-runtime.dll".into();
        let empty = execute_aoem_compute_tasks_v1(&runtime, vec![], Duration::from_secs(1));
        assert!(empty.unwrap_err().to_string().contains("requires"));
        let zero =
            execute_aoem_compute_tasks_v1(&runtime, vec![Box::new(|| Ok(vec![]))], Duration::ZERO);
        assert!(zero.unwrap_err().to_string().contains("requires"));
    }

    #[test]
    #[ignore = "requires the bundled AOEM DLL; explicitly run for real worker acceptance"]
    fn semantic_compute_real_aoem_no_writes_parallel_order_panic_and_cancel() {
        let mut runtime = AoemRuntimeConfig::from_env().expect("runtime");
        runtime.ingress_workers = Some(4);
        let arrivals = Arc::new(AtomicUsize::new(0));
        let tasks = (0..4)
            .map(|index| {
                let arrivals = arrivals.clone();
                Box::new(move || {
                    arrivals.fetch_add(1, Ordering::AcqRel);
                    let started = Instant::now();
                    while arrivals.load(Ordering::Acquire) < 2 {
                        if started.elapsed() > Duration::from_secs(2) {
                            bail!("AOEM callbacks did not overlap");
                        }
                        std::thread::yield_now();
                    }
                    std::thread::sleep(Duration::from_millis((4 - index) * 5));
                    Ok(vec![index as u8])
                }) as AoemComputeTaskV1
            })
            .collect();
        let report = execute_aoem_compute_tasks_v1(&runtime, tasks, Duration::from_secs(5))
            .expect("compute-only V2");
        assert!(
            report.peak_inflight >= 2,
            "actual callback overlap: {report:?}"
        );
        assert_eq!(report.outputs, vec![vec![0], vec![1], vec![2], vec![3]]);
        assert_eq!(
            (report.processed, report.succeeded, report.failed),
            (4, 4, 0)
        );
        eprintln!("real AOEM compute peak_inflight={}", report.peak_inflight);
        assert!(execute_aoem_compute_tasks_v1(
            &runtime,
            vec![Box::new(|| panic!("contained AOEM worker panic"))],
            Duration::from_secs(5)
        )
        .is_err());
        assert!(execute_aoem_compute_tasks_v1(
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
        for index in 0..4 {
            let report = execute_aoem_compute_tasks_v1(
                &runtime,
                vec![Box::new(move || Ok(vec![index]))],
                Duration::from_secs(5),
            )
            .expect("single-task wave after failed sessions");
            assert_eq!(report.peak_inflight, 1);
            serial_outputs.extend(report.outputs);
        }
        assert_eq!(serial_outputs, report.outputs);
    }

    #[test]
    #[ignore = "requires bundled AOEM; deliberately retains one cancelled owner"]
    fn semantic_compute_real_aoem_undrained_timeout_retains_owned_capture() {
        let runtime = AoemRuntimeConfig::from_env().expect("runtime");
        let executing = Arc::new(AtomicUsize::new(0));
        let finished = Arc::new(AtomicUsize::new(0));
        let task_executing = executing.clone();
        let task_finished = finished.clone();
        let result = execute_aoem_compute_tasks_v1(
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
}
