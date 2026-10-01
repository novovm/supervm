//! Locally migrated from ee80271 semantic_compute.rs; no legacy dependency.
//! Owned, domain-neutral computation on the packaged V2 semantic workers.
//! The adapter does not bind a storage writer. AOEM initialization is described
//! separately: the native library may consult its own environment/configuration.
//! Tasks must compute isolated results: cancellation cannot undo side effects.

use crate::abi::*;
use crate::library::NativeSession;
use anyhow::{bail, Context, Result};
use std::cell::Cell;
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_TASKS: usize = 65_536;
const MAX_RESULT_BYTES: usize = 64 * 1024 * 1024;
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
static NEXT_GRAPH_ID: AtomicU64 = AtomicU64::new(1);

/// A Host-owned computation, not a precomputed KV write or an AOEM business opcode.
/// Owned captures are required because cancellation may outlive the caller.
pub type ComputeTask = Box<dyn FnOnce() -> Result<Vec<u8>> + Send + 'static>;

#[derive(Debug, PartialEq, Eq)]
pub struct ComputeReport {
    /// Results in input order, independent of worker completion order.
    pub outputs: Vec<Vec<u8>>,
    pub processed: u64,
    pub succeeded: u64,
    pub failed: u64,
    /// Actual simultaneously executing callbacks, not queued descriptors.
    pub peak_inflight: usize,
}

struct TaskSlot {
    task: Mutex<Option<ComputeTask>>,
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
    completion_tx: Mutex<Option<mpsc::Sender<Completion>>>,
}

impl ComputeContext {
    fn new(
        graph_id: u64,
        tasks: Vec<ComputeTask>,
        completion_tx: mpsc::Sender<Completion>,
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
    // The complete retained flight owns a session reference as well as its
    // callback data; releasing the public owner cannot unload a live graph.
    session: Rc<ComputeSessionInner>,
    context: Arc<ComputeContext>,
    seeds: Vec<TaskDescriptor>,
    options: SubmitOptions,
    callbacks: Callbacks,
}

struct ComputeSessionInner {
    session: NativeSession,
    poisoned: Cell<bool>,
}

/// An explicit, same-thread computation owner. It requests no storage provider,
/// atomic writer, business policy, or global/TLS registration. A caller can
/// submit multiple bounded graphs, but only after each previous graph drains.
/// Execute on a dedicated computation owner, never the Host control loop.
/// Any admitted failure permanently poisons this owner; reopening is never an
/// implicit recovery action. Drop the owner during normal thread execution.
/// Native modules remain resident until process exit because AOEM may own
/// process-global workers. Normally drained session contexts are destroyed.
///
/// ```compile_fail
/// fn requires_send<T: Send>() {}
/// requires_send::<novovm_aoem::ComputeSession>();
/// ```
pub struct ComputeSession {
    inner: Rc<ComputeSessionInner>,
}

/// Keeps every asynchronous FFI input alive on errors AND Host unwinding.
/// Retaining only callback data would allow the DLL/session to be destroyed.
struct ComputeFlight {
    owner: Option<Box<ComputeOwner>>,
    drained: bool,
}

impl ComputeFlight {
    fn new(owner: ComputeOwner) -> Self {
        Self {
            owner: Some(Box::new(owner)),
            drained: false,
        }
    }

    fn owner(&self) -> &ComputeOwner {
        self.owner
            .as_deref()
            .expect("compute submission owner is live")
    }
}

impl Drop for ComputeFlight {
    fn drop(&mut self) {
        if !self.drained {
            if let Some(owner) = self.owner.take() {
                let _retained = Box::into_raw(owner);
            }
        }
    }
}

fn validate_task_batch(tasks: &[ComputeTask], timeout: Duration) -> Result<()> {
    if tasks.is_empty() || tasks.len() > MAX_TASKS || timeout.is_zero() {
        bail!("AOEM compute requires 1..={MAX_TASKS} tasks and a nonzero deadline");
    }
    Ok(())
}

impl ComputeSession {
    /// Load an explicitly selected, trusted AOEM binary and create one context.
    /// `ingress_workers=0` retains the DLL default; this option configures
    /// ingress workers, not the semantic scheduler's worker count.
    /// No environment changes, profile discovery or alternative executor occur.
    pub fn open(path: &Path, ingress_workers: u32) -> Result<Self> {
        let session = NativeSession::open(path, ingress_workers)
            .context("open AOEM compute session failed")?;
        Ok(Self {
            inner: Rc::new(ComputeSessionInner {
                session,
                poisoned: Cell::new(false),
            }),
        })
    }

    /// Results retain input order. No input may borrow caller stack/state: a
    /// cancelled callback can outlive this call and even this session owner.
    /// Rejected pre-admission bounds do not poison an otherwise usable owner.
    pub fn execute(&mut self, tasks: Vec<ComputeTask>, timeout: Duration) -> Result<ComputeReport> {
        self.execute_with_checkpoint(tasks, timeout, || {})
    }

    fn execute_with_checkpoint(
        &mut self,
        tasks: Vec<ComputeTask>,
        timeout: Duration,
        after_admission: impl FnOnce(),
    ) -> Result<ComputeReport> {
        if self.inner.poisoned.get() {
            bail!("AOEM compute session is poisoned; discard this computation owner");
        }
        validate_task_batch(&tasks, timeout)?;
        let graph_id = NEXT_GRAPH_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| anyhow::anyhow!("AOEM compute graph identifiers exhausted"))?;
        let (sender, receiver) = mpsc::channel();
        let context = Arc::new(ComputeContext::new(graph_id, tasks, sender));
        let seeds = (0..context.slots.len())
            .map(|index| TaskDescriptor {
                graph_id,
                task_id: index as u64 + 1,
                context_handle: index as u64 + 1,
                sequence: index as u64,
                ..TaskDescriptor::default()
            })
            .collect::<Vec<_>>();
        let mut flight = ComputeFlight::new(ComputeOwner {
            session: Rc::clone(&self.inner),
            options: SubmitOptions {
                max_queued_tasks: seeds.len().max(2) as u32,
                event_capacity: 1,
                abi_version: GRAPH_ABI,
                flags: 0,
                initial_event_sequence: 0,
            },
            callbacks: Callbacks {
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
        // Set before FFI admission: an error or unwind must never allow reuse of
        // a possibly still-live graph, even when no completion was delivered.
        self.inner.poisoned.set(true);
        let owner = flight.owner();
        let submitted = unsafe {
            owner
                .session
                .session
                .submit(&owner.seeds, &owner.options, &owner.callbacks)
        };
        if !matches!(submitted, Ok(OK)) {
            let _ = owner.session.session.cancel(graph_id);
            let _ = drain(&mut flight, DRAIN_TIMEOUT);
            bail!("AOEM compute-only graph admission failed: {submitted:?}");
        }
        after_admission();
        let completion = match receiver.recv_timeout(timeout) {
            Ok(completion) => completion,
            Err(error) => {
                let _ = flight.owner().session.session.cancel(graph_id);
                // The caller's timeout remains failure even if cancellation races success.
                if !drain(&mut flight, DRAIN_TIMEOUT) {
                    bail!(
                        "AOEM compute deadline/channel failure ({error}); undrained owner retained"
                    );
                }
                bail!("AOEM compute deadline/channel failure: {error}");
            }
        };
        if !drain(&mut flight, DRAIN_TIMEOUT) {
            let _ = flight.owner().session.session.cancel(graph_id);
            bail!("AOEM compute completion did not drain; owner retained");
        }
        let owner = flight.owner();
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
        if completion.abi_version != GRAPH_ABI
            || completion.graph_id != graph_id
            || completion.status != OK
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
        let report = ComputeReport {
            outputs,
            processed: completion.processed,
            succeeded: completion.succeeded,
            failed: completion.failed,
            peak_inflight: context.peak.load(Ordering::Acquire),
        };
        self.inner.poisoned.set(false);
        Ok(report)
    }
}

fn drain(flight: &mut ComputeFlight, timeout: Duration) -> bool {
    let started = Instant::now();
    loop {
        let owner = flight.owner();
        // A zero active count is not a guarantee that completion has started.
        // In SDK source 56e9da15 semantic_graph_v2.rs:1468, complete_graph
        // removes the graph/reservations BEFORE invoking completion. The
        // callback's temporary Arc below must therefore also be released:
        // this proves no further access to Host callback state, not that the
        // native worker's epilogue has returned. That epilogue has no further
        // user_data access; the next graph has a distinct id and context.
        // Session destruction independently joins all native graph workers in
        // shutdown (same source:846, 887-893) before aoem_destroy frees its
        // context. The native module remains resident. Do not replace this
        // conjunction with active_count alone or treat it as DLL unload proof.
        if owner.context.completion_seen.load(Ordering::Acquire)
            && owner.session.session.active_count() == 0
            && owner.context.inflight.load(Ordering::Acquire) == 0
            && Arc::strong_count(&owner.context) == 1
        {
            flight.drained = true;
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
            CALLBACK_PANICKED
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
    descriptor: *const TaskDescriptor,
    output: *mut StepOutput,
    user_data: *mut c_void,
) -> i32 {
    // SAFETY: AOEM supplies header-compatible pointers and user_data points to
    // the context retained by the complete flight until all callbacks drain.
    ffi_status(|| unsafe {
        let (Some(context), Some(descriptor), Some(output)) = (
            user_data.cast::<ComputeContext>().as_ref(),
            descriptor.as_ref(),
            output.as_mut(),
        ) else {
            return INVALID_ARGUMENT;
        };
        *output = StepOutput::default();
        let Some(index) = context.index(descriptor.context_handle) else {
            return INVALID_ARGUMENT;
        };
        if descriptor.abi_version != GRAPH_ABI
            || descriptor.graph_id != context.graph_id
            || descriptor.task_id != index as u64 + 1
            || descriptor.sequence != index as u64
            || descriptor.payload_len != 0
        {
            return INVALID_ARGUMENT;
        }
        let slot = &context.slots[index];
        let task = slot.task.lock().unwrap_or_else(|e| e.into_inner()).take();
        let Some(task) = task else {
            context.record_error("duplicate task execution".into());
            return GRAPH_FAULTED;
        };
        let inflight = context.inflight.fetch_add(1, Ordering::AcqRel) + 1;
        context.peak.fetch_max(inflight, Ordering::AcqRel);
        let _inflight = Inflight(context);
        let result = match task() {
            Ok(output) => output,
            Err(error) => {
                context.record_error(format!("task {index}: {error:#}"));
                return GRAPH_FAULTED;
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
            return GRAPH_FAULTED;
        }
        *slot.output.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
        OK
    })
}

unsafe extern "C-unwind" fn validate_handle(handle: u64, user_data: *mut c_void) -> i32 {
    // The complete flight already owns all slots. Per-handle retain/release
    // validate identity; none can release that owner before graph drain.
    ffi_status(|| unsafe {
        match user_data.cast::<ComputeContext>().as_ref() {
            Some(context) if context.index(handle).is_some() => OK,
            _ => INVALID_ARGUMENT,
        }
    })
}

unsafe extern "C-unwind" fn complete_graph(completion: *const Completion, user_data: *mut c_void) {
    let _ = ffi_status(|| unsafe {
        let raw = user_data.cast::<ComputeContext>();
        if raw.is_null() {
            return INVALID_ARGUMENT;
        }
        // Keep context alive after waking the caller and until this callback exits.
        Arc::increment_strong_count(raw);
        let context = Arc::from_raw(raw);
        let Some(completion) = completion.as_ref() else {
            return INVALID_ARGUMENT;
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
        OK
    });
}

unsafe extern "C-unwind" fn reject_event(_: *const StateEvent, _: *mut c_void) -> i32 {
    INVALID_ARGUMENT
}

#[cfg(test)]
#[path = "compute_tests.rs"]
mod tests;
