//! One owned candidate execution in flight, including an unconsumed result.
//! This worker cannot sign, publish, retire, or remove pending transactions.
#![forbid(unsafe_code)]

use super::FRESH_CHAIN_LIFECYCLE_STACK_BYTES_V1;
use crate::tx_ingress::{candidate_workspace as workspace, NativeAoemSemanticSessionScopeV1};
use anyhow::{anyhow, bail, Context, Result};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};

pub(super) struct CandidateWorker {
    inner: SingleWorker<workspace::ExecutionJobV1, workspace::PreparedExecutionV1>,
    storage: novovm_exec::AoemSemanticGraphClientV1,
}

impl CandidateWorker {
    pub(super) fn start(client: novovm_exec::AoemSemanticGraphClientV1) -> Result<Self> {
        Ok(Self {
            storage: client.clone(),
            inner: SingleWorker::start(
                move || {
                    let remote = client.enter()?;
                    let semantic = NativeAoemSemanticSessionScopeV1::default();
                    let compute = novovm_exec::AoemComputeSessionScopeV1::enter()?;
                    // Tuple fields drop in order: close the thread-local AOEM
                    // session while the remote graph scope is still present.
                    Ok((compute, semantic, remote))
                },
                workspace::ExecutionJobV1::run,
            )?,
        })
    }

    pub(super) fn storage_client(&self) -> &novovm_exec::AoemSemanticGraphClientV1 {
        &self.storage
    }

    /// A rejection consumes only this owned job, never its durable workspace
    /// or pending transactions. The caller must stop/recover on submission error.
    pub(super) fn try_submit(&mut self, job: workspace::ExecutionJobV1) -> Result<()> {
        self.inner.try_submit(job)
    }

    pub(super) fn try_complete(&mut self) -> Result<Option<workspace::PreparedExecutionV1>> {
        self.inner.try_complete()
    }

    pub(super) fn is_busy(&self) -> bool {
        self.inner.busy
    }

    pub(super) fn completion_ready(&self) -> bool {
        self.inner.ready.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(super) fn pause_next_for_test(&mut self) -> Result<(Receiver<()>, SyncSender<()>)> {
        self.inner.pause_next_for_test()
    }

    pub(super) fn status_json(&self) -> serde_json::Value {
        serde_json::json!({
            "max_inflight": 1,
            "inflight": usize::from(self.inner.busy),
            "busy": self.inner.busy,
            "completion_ready": self.completion_ready(),
            "poisoned": self.inner.poisoned,
            "submitted": self.inner.submitted,
            "completed": self.inner.completed,
            "failed": self.inner.failed,
        })
    }
}

// The generic core is the production queue and run loop, also exercised by
// bounded pure tests without loading a DLL or manufacturing candidate evidence.
struct WorkItem<J> {
    job: J,
    #[cfg(test)]
    pause: Option<(SyncSender<()>, Receiver<()>)>,
}

struct SingleWorker<J, C> {
    jobs: Option<SyncSender<WorkItem<J>>>,
    completions: Receiver<Result<C>>,
    thread: Option<JoinHandle<()>>,
    busy: bool,
    poisoned: bool,
    submitted: u64,
    completed: u64,
    failed: u64,
    ready: Arc<AtomicBool>,
    #[cfg(test)]
    next_pause: Option<(SyncSender<()>, Receiver<()>)>,
}

impl<J: Send + 'static, C: Send + 'static> SingleWorker<J, C> {
    fn start<G: 'static>(
        initialize: impl FnOnce() -> Result<G> + Send + 'static,
        mut execute: impl FnMut(J) -> Result<C> + Send + 'static,
    ) -> Result<Self> {
        let (jobs, incoming) = mpsc::sync_channel::<WorkItem<J>>(1);
        let (outgoing, completions) = mpsc::sync_channel(1);
        let (started, startup) = mpsc::sync_channel::<Result<()>>(1);
        let ready = Arc::new(AtomicBool::new(false));
        let completion_ready = ready.clone();
        let caller = thread::current();
        let thread = thread::Builder::new()
            .name("novovm-candidate".to_owned())
            .stack_size(FRESH_CHAIN_LIFECYCLE_STACK_BYTES_V1)
            .spawn(move || {
                // Also wake the owner on unexpected scope teardown failure;
                // only an actually queued result sets completion_ready.
                struct WakeOnExit(thread::Thread);
                impl Drop for WakeOnExit {
                    fn drop(&mut self) {
                        self.0.unpark();
                    }
                }
                let _wake_on_exit = WakeOnExit(caller.clone());
                // G may be !Send. It is created and dropped on this ordinary
                // thread stack, never transported or deferred to TLS teardown.
                let initialized = catch_unwind(AssertUnwindSafe(initialize))
                    .unwrap_or_else(|_| Err(anyhow!("candidate worker initialization panicked")));
                let _scope = match initialized {
                    Ok(scope) => scope,
                    Err(error) => {
                        let _ = started.send(Err(error));
                        return;
                    }
                };
                if started.send(Ok(())).is_err() {
                    return;
                }
                while let Ok(item) = incoming.recv() {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        #[cfg(test)]
                        if let Some((entered, release)) = item.pause {
                            entered
                                .send(())
                                .context("candidate worker test pause disconnected")?;
                            release
                                .recv_timeout(std::time::Duration::from_secs(10))
                                .context("candidate worker test pause release failed")?;
                        }
                        execute(item.job)
                    }))
                    .unwrap_or_else(|_| Err(anyhow!("candidate execution worker panicked")));
                    let failed = result.is_err();
                    if outgoing.send(result).is_err() {
                        break;
                    }
                    completion_ready.store(true, Ordering::Release);
                    caller.unpark();
                    if failed {
                        break;
                    }
                }
            })
            .context("start candidate execution worker")?;
        let initialized = startup
            .recv()
            .context("candidate worker initialization disconnected")
            .and_then(|result| result);
        if let Err(error) = initialized {
            drop(jobs);
            let _ = thread.join();
            return Err(error.context("initialize candidate execution worker"));
        }
        Ok(Self {
            jobs: Some(jobs),
            completions,
            thread: Some(thread),
            busy: false,
            poisoned: false,
            submitted: 0,
            completed: 0,
            failed: 0,
            ready,
            #[cfg(test)]
            next_pause: None,
        })
    }

    fn try_submit(&mut self, job: J) -> Result<()> {
        if self.poisoned {
            bail!("candidate execution worker is poisoned; restart required");
        }
        if self.busy {
            bail!("candidate execution worker already has an in-flight job");
        }
        let Some(sender) = self.jobs.as_ref() else {
            self.poisoned = true;
            bail!("candidate execution worker is closed; restart required");
        };
        let item = WorkItem {
            job,
            #[cfg(test)]
            pause: self.next_pause.take(),
        };
        match sender.try_send(item) {
            Ok(()) => {
                self.busy = true;
                self.submitted = self.submitted.saturating_add(1);
                Ok(())
            }
            Err(TrySendError::Full(_)) => {
                self.poisoned = true;
                bail!("candidate execution worker queue invariant failed; restart required")
            }
            Err(TrySendError::Disconnected(_)) => {
                self.poisoned = true;
                bail!("candidate execution worker disconnected; restart required")
            }
        }
    }

    #[cfg(test)]
    fn pause_next_for_test(&mut self) -> Result<(Receiver<()>, SyncSender<()>)> {
        if self.poisoned || self.busy || self.next_pause.is_some() {
            bail!("candidate worker test pause requires an idle usable worker");
        }
        let (entered, arrival) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        self.next_pause = Some((entered, released));
        Ok((arrival, release))
    }

    fn try_complete(&mut self) -> Result<Option<C>> {
        if self.poisoned {
            bail!("candidate execution worker is poisoned; restart required");
        }
        // Do not consume a result in the tiny send-before-ready interval:
        // otherwise the producer could set ready after the consumer clears it.
        if !self.ready.load(Ordering::Acquire)
            && !self.thread.as_ref().is_some_and(JoinHandle::is_finished)
        {
            return Ok(None);
        }
        match self.completions.try_recv() {
            Ok(result) => {
                self.ready.swap(false, Ordering::AcqRel);
                if !self.busy {
                    self.poisoned = true;
                    bail!("candidate execution worker returned an unsolicited completion");
                }
                self.busy = false;
                self.completed = self.completed.saturating_add(1);
                match result {
                    Ok(completed) => Ok(Some(completed)),
                    Err(error) => {
                        self.poisoned = true;
                        self.failed = self.failed.saturating_add(1);
                        Err(error.context("candidate execution worker failed; restart required"))
                    }
                }
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                self.poisoned = true;
                self.failed = self.failed.saturating_add(1);
                bail!("candidate execution worker disconnected; restart required")
            }
        }
    }
}

impl<J, C> Drop for SingleWorker<J, C> {
    fn drop(&mut self) {
        // Keep the result receiver alive through join. A finishing job can
        // deposit its sole result even when shutdown does not consume it.
        drop(self.jobs.take());
        if let Some(thread) = self.thread.take() {
            // Execution panics become error completions; an unexpected scope
            // destructor panic is observed as disconnect while still running.
            // Drop cannot report errors, but must not detach the worker.
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::time::{Duration, Instant};

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    fn complete<J: Send + 'static, C: Send + 'static>(
        worker: &mut SingleWorker<J, C>,
    ) -> Result<C> {
        let deadline = Instant::now() + TEST_TIMEOUT;
        loop {
            if let Some(result) = worker.try_complete()? {
                return Ok(result);
            }
            assert!(Instant::now() < deadline, "worker completion timed out");
            thread::yield_now();
        }
    }

    #[test]
    fn one_inflight_and_nonblocking_completion_preserve_owned_result() {
        let (entered, started) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        let mut worker = SingleWorker::start(
            || Ok(()),
            move |job: u64| {
                entered.send(job)?;
                released.recv_timeout(TEST_TIMEOUT)?;
                Ok(job + 1)
            },
        )
        .unwrap();
        assert!(worker.try_complete().unwrap().is_none());
        worker.try_submit(40).unwrap();
        assert_eq!(started.recv_timeout(TEST_TIMEOUT).unwrap(), 40);
        assert!(worker.busy);
        assert!(worker.try_complete().unwrap().is_none());
        assert!(worker.try_submit(900).is_err());
        assert!(!worker.poisoned);
        release.send(()).unwrap();
        assert!(worker.busy);
        assert_eq!(complete(&mut worker).unwrap(), 41);
        assert!(!worker.busy);
        worker.try_submit(41).unwrap();
        assert_eq!(started.recv_timeout(TEST_TIMEOUT).unwrap(), 41);
        release.send(()).unwrap();
        assert_eq!(complete(&mut worker).unwrap(), 42);
        assert_eq!(
            (worker.submitted, worker.completed, worker.failed),
            (2, 2, 0)
        );
    }

    #[test]
    fn execution_error_is_not_a_business_completion_and_poison_is_sticky() {
        let mut worker = SingleWorker::start(
            || Ok(()),
            |_: ()| -> Result<()> { bail!("injected execution infrastructure failure") },
        )
        .unwrap();
        worker.try_submit(()).unwrap();
        let error = complete(&mut worker).unwrap_err();
        assert!(format!("{error:#}").contains("injected execution infrastructure failure"));
        assert!(worker.poisoned);
        assert!(!worker.busy);
        assert_eq!(
            (worker.submitted, worker.completed, worker.failed),
            (1, 1, 1)
        );
        assert!(worker.try_submit(()).is_err());
        assert!(worker.try_complete().is_err());
    }

    #[test]
    fn real_runloop_pause_and_ready_flag_preserve_unconsumed_backpressure() {
        let executed = Arc::new(AtomicBool::new(false));
        let execution_flag = executed.clone();
        let mut worker = SingleWorker::start(
            || Ok(()),
            move |job: u64| {
                execution_flag.store(true, Ordering::Release);
                Ok(job)
            },
        )
        .unwrap();
        let (entered, release) = worker.pause_next_for_test().unwrap();
        assert!(worker.pause_next_for_test().is_err());
        worker.try_submit(7).unwrap();
        entered.recv_timeout(TEST_TIMEOUT).unwrap();
        assert!(!executed.load(Ordering::Acquire));
        assert!(!worker.ready.load(Ordering::Acquire));
        assert!(worker.try_complete().unwrap().is_none());
        assert!(worker.pause_next_for_test().is_err());
        release.send(()).unwrap();
        let deadline = Instant::now() + TEST_TIMEOUT;
        while !worker.ready.load(Ordering::Acquire) {
            assert!(
                Instant::now() < deadline,
                "completion notification timed out"
            );
            thread::park_timeout(Duration::from_millis(1));
        }
        assert!(executed.load(Ordering::Acquire));
        assert!(worker.busy);
        assert!(worker.try_submit(8).is_err());
        assert_eq!(worker.try_complete().unwrap(), Some(7));
        assert!(!worker.ready.load(Ordering::Acquire));
        assert!(worker.try_complete().unwrap().is_none());
        worker.try_submit(8).unwrap();
        assert_eq!(complete(&mut worker).unwrap(), 8);
        assert!(!worker.ready.load(Ordering::Acquire));
    }

    #[test]
    fn execution_panic_becomes_infrastructure_error() {
        let mut worker = SingleWorker::start(
            || Ok(()),
            |_: ()| -> Result<()> { panic!("injected worker panic") },
        )
        .unwrap();
        worker.try_submit(()).unwrap();
        assert!(format!("{:#}", complete(&mut worker).unwrap_err())
            .contains("candidate execution worker panicked"));
        assert!(worker.poisoned);
        assert!(worker.try_submit(()).is_err());
    }

    #[test]
    fn startup_error_and_panic_fail_before_admitting_any_job() {
        let failed = SingleWorker::start(
            || -> Result<()> { bail!("injected startup failure") },
            |_: ()| Ok(()),
        );
        assert!(format!("{:#}", failed.err().unwrap()).contains("injected startup failure"));
        let panicked = SingleWorker::start(
            || -> Result<()> { panic!("injected startup panic") },
            |_: ()| Ok(()),
        );
        assert!(format!("{:#}", panicked.err().unwrap())
            .contains("candidate worker initialization panicked"));
    }

    #[test]
    fn drop_joins_inflight_and_drops_thread_affine_scope_on_worker() {
        struct ThreadScope {
            owner: thread::ThreadId,
            dropped: Arc<AtomicBool>,
            _not_send: Rc<()>,
        }
        impl Drop for ThreadScope {
            fn drop(&mut self) {
                assert_eq!(self.owner, thread::current().id());
                self.dropped.store(true, Ordering::Release);
            }
        }
        let scope_dropped = Arc::new(AtomicBool::new(false));
        let scope_flag = scope_dropped.clone();
        let (entered, started) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        let mut worker = SingleWorker::start(
            move || {
                Ok(ThreadScope {
                    owner: thread::current().id(),
                    dropped: scope_flag,
                    _not_send: Rc::new(()),
                })
            },
            move |_: ()| {
                entered.send(())?;
                released.recv_timeout(TEST_TIMEOUT)?;
                Ok(())
            },
        )
        .unwrap();
        worker.try_submit(()).unwrap();
        started.recv_timeout(TEST_TIMEOUT).unwrap();
        let (drop_started, dropping) = mpsc::sync_channel(1);
        let (drop_done, dropped) = mpsc::sync_channel(1);
        let shutdown = thread::spawn(move || {
            drop_started.send(()).unwrap();
            drop(worker);
            drop_done.send(()).unwrap();
        });
        dropping.recv_timeout(TEST_TIMEOUT).unwrap();
        assert!(matches!(dropped.try_recv(), Err(TryRecvError::Empty)));
        assert!(!scope_dropped.load(Ordering::Acquire));
        release.send(()).unwrap();
        dropped.recv_timeout(TEST_TIMEOUT).unwrap();
        shutdown.join().unwrap();
        assert!(scope_dropped.load(Ordering::Acquire));
    }
}
