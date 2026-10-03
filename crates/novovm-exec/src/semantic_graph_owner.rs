//! One explicit storage thread owns the non-Send AOEM session and database.
//! Clients carry only owned bounded commands, never an FFI handle or authority.

use super::session_scope::ProviderIdentity;
use super::*;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::{mpsc::SyncSender, OnceLock, Weak};
use std::thread::{self, JoinHandle};

const QUEUE_CAPACITY: usize = 8;
const OWNER_STACK_BYTES: usize = 8 * 1024 * 1024;

type OwnerStageV1 = Box<dyn FnOnce(&ClientState, &SemanticGraphStoreInnerV1) + Send>;

thread_local! {
    // Discovery never owns a worker and never joins one under Windows TLS teardown.
    static ACTIVE_CLIENT: RefCell<Weak<ClientState>> = const { RefCell::new(Weak::new()) };
}

enum Command {
    Get(Vec<u8>, SyncSender<Result<Option<Vec<u8>>>>),
    Commit(
        Box<PreparedGraphV1>,
        SyncSender<Result<AoemAtomicGraphCommitReportV1>>,
    ),
    Stage(OwnerStageV1),
    Stop,
    #[cfg(test)]
    Poison(SyncSender<()>),
    #[cfg(test)]
    Inspect(SyncSender<(u64, thread::ThreadId)>),
    #[cfg(test)]
    Park(SyncSender<()>, mpsc::Receiver<()>),
    #[cfg(test)]
    Panic,
}

impl std::fmt::Debug for Command {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Channel errors must not dump opaque keys, values or task payloads.
        formatter.write_str("AoemStorageCommand")
    }
}

struct ClientState {
    identity: ProviderIdentity,
    commands: SyncSender<Command>,
    stopping: AtomicBool,
    failure: Mutex<Option<String>>,
    owner_thread: OnceLock<thread::ThreadId>,
}

impl ClientState {
    fn ensure_caller_thread(&self) -> Result<()> {
        if self.owner_thread.get() == Some(&thread::current().id()) {
            bail!(
                "AOEM storage owner cannot send a command to itself; open its local store instead"
            );
        }
        Ok(())
    }

    fn ensure_usable(&self) -> Result<()> {
        if let Some(reason) = self
            .failure
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            bail!("AOEM storage owner is terminal: {reason}");
        }
        if self.stopping.load(Ordering::Acquire) {
            bail!("AOEM storage owner has stopped");
        }
        Ok(())
    }

    fn fail(&self, reason: String) {
        let mut failure = self.failure.lock().unwrap_or_else(|e| e.into_inner());
        if failure.is_none() {
            *failure = Some(reason);
        }
    }
}

/// Explicit owner of the storage thread. Keep this value outside TLS and drop it
/// after its callers finish. Clients cannot keep the worker alive after shutdown.
/// The owner itself stays on its creating thread; no AOEM Rc crosses a thread.
pub struct AoemSemanticGraphOwnerV1 {
    client: AoemSemanticGraphClientV1,
    worker: Option<JoinHandle<()>>,
    _thread: PhantomData<Rc<()>>,
}

/// Cloneable endpoint, not a database/session handle or a permission to publish.
/// Store reads/commits remain synchronous; only `try_stage` admission and its
/// completion polling are nonblocking. Callers must avoid these synchronous
/// operations while an accepted stage holds a lock that they also need.
#[derive(Clone)]
pub struct AoemSemanticGraphClientV1 {
    state: Arc<ClientState>,
}

/// Register a client on one calling thread. Drop only clears discovery; it never
/// stops or joins the worker. Existing remote stores retain their same endpoint.
#[must_use = "keep the remote graph scope alive while opening graph stores"]
pub struct AoemSemanticGraphClientScopeV1 {
    state: Arc<ClientState>,
    _thread: PhantomData<Rc<()>>,
}

/// Backpressure returns the original, unstarted owned closure to its caller.
/// Acceptance is not execution success and confers no publication authority.
#[must_use = "retain an accepted handle or explicitly handle backpressure"]
pub enum AoemSemanticGraphStageAdmissionV1<T, F> {
    Accepted(AoemSemanticGraphStageHandleV1<T>),
    Backpressured(F),
}

/// A single nonblocking result. Dropping it does not cancel accepted work, join
/// the owner, or establish whether a storage mutation completed successfully.
#[must_use = "poll the accepted stage for its actual completion result"]
pub struct AoemSemanticGraphStageHandleV1<T> {
    response: mpsc::Receiver<Result<T>>,
    ready: Arc<AtomicBool>,
    consumed: bool,
}

impl<T> AoemSemanticGraphStageHandleV1<T> {
    /// A completion (including disconnection) is available, not necessarily a
    /// successful result. The submitting thread is also unparked on completion.
    pub fn is_ready(&self) -> bool {
        self.consumed || self.ready.load(Ordering::Acquire)
    }

    pub fn try_complete(&mut self) -> Result<Option<T>> {
        if self.consumed {
            bail!("AOEM storage stage completion was already consumed");
        }
        match self.response.try_recv() {
            Ok(result) => {
                self.consumed = true;
                result.map(Some)
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.consumed = true;
                bail!("AOEM storage stage response disconnected; completion is unknown");
            }
        }
    }
}

struct StageReplyV1<T> {
    reply: Option<SyncSender<Result<T>>>,
    ready: Arc<AtomicBool>,
    caller: thread::Thread,
}

impl<T> StageReplyV1<T> {
    fn complete(mut self, result: Result<T>) {
        // This private single-result channel has capacity one and is never full.
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(result);
        }
    }
}

impl<T> Drop for StageReplyV1<T> {
    fn drop(&mut self) {
        // Close the sender before marking ready, including panic/queue teardown.
        drop(self.reply.take());
        self.ready.store(true, Ordering::Release);
        self.caller.unpark();
    }
}

impl AoemSemanticGraphOwnerV1 {
    /// Start before concurrent callers. Runtime environment initialization and
    /// the one physical database open complete before this method returns.
    /// The queue holds at most eight commands. Graphs retain the local adapter's
    /// exact ABI bounds; callers retain their existing per-job memory budgets.
    pub fn start(
        runtime: &AoemRuntimeConfig,
        path: &Path,
        storage: &AoemStorageProviderConfigV1,
    ) -> Result<Self> {
        validate_storage_config(storage)?;
        if has_active_scope() || session_scope::has_active_scope() {
            bail!("start AOEM storage owner outside all graph scopes");
        }
        let identity = ProviderIdentity::new(runtime, path, storage)?;
        let (commands, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let state = Arc::new(ClientState {
            identity,
            commands,
            stopping: AtomicBool::new(false),
            failure: Mutex::new(None),
            owner_thread: OnceLock::new(),
        });
        let (ready, opened) = mpsc::sync_channel(1);
        let thread_state = state.clone();
        let runtime = runtime.clone();
        let path = path.to_path_buf();
        let storage = storage.clone();
        let worker = thread::Builder::new()
            .name("aoem-storage-owner".into())
            .stack_size(OWNER_STACK_BYTES)
            .spawn(move || {
                let _ = thread_state.owner_thread.set(thread::current().id());
                let result = catch_unwind(AssertUnwindSafe(|| -> Result<()> {
                    let inner = AoemSemanticGraphStoreV1::open_uncached(&runtime, &path, &storage)?;
                    // Reject environment/path drift during initialization rather
                    // than installing an owner whose declared identity is stale.
                    if !thread_state
                        .identity
                        .matches(&ProviderIdentity::new(&runtime, &path, &storage)?)
                    {
                        bail!("AOEM storage owner identity changed while opening");
                    }
                    let store = AoemSemanticGraphStoreV1 {
                        inner: GraphStoreBackendV1::Local(inner.clone()),
                        path,
                    };
                    let _local_scope = AoemSemanticGraphSessionScopeV1::enter_with_provider(
                        thread_state.identity.clone(),
                        inner.clone(),
                    )?;
                    let _ = ready.send(Ok(()));
                    run(&thread_state, receiver, &store, &inner);
                    Ok(())
                }));
                match result {
                    Ok(Ok(())) => (),
                    Ok(Err(error)) => {
                        thread_state.fail(format!("{error:#}"));
                        let _ = ready.send(Err(error));
                    }
                    Err(_) => {
                        thread_state.fail("storage worker panicked; restart required".into());
                        let _ = ready.send(Err(anyhow::anyhow!("AOEM storage worker panicked")));
                    }
                }
                thread_state.stopping.store(true, Ordering::Release);
            })
            .context("spawn AOEM storage owner")?;
        match opened.recv() {
            Ok(Ok(())) => Ok(Self {
                client: AoemSemanticGraphClientV1 { state },
                worker: Some(worker),
                _thread: PhantomData,
            }),
            result => {
                let _ = worker.join();
                match result {
                    Ok(Err(error)) => Err(error),
                    Err(error) => Err(error).context("AOEM storage owner startup disconnected"),
                    Ok(Ok(())) => unreachable!(),
                }
            }
        }
    }

    pub fn client(&self) -> AoemSemanticGraphClientV1 {
        self.client.clone()
    }

    /// Stop admission, reject queued work, finish the current bounded commit,
    /// and join in ordinary thread execution. No unknown commit is called a pass.
    pub fn shutdown(mut self) -> Result<()> {
        self.stop()
    }

    fn stop(&mut self) -> Result<()> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        self.client.state.stopping.store(true, Ordering::Release);
        // Sending Stop wakes recv; once stopping is set no new caller is admitted.
        // A caller racing this transition is rejected by the worker before I/O.
        let _ = self.client.state.commands.send(Command::Stop);
        worker
            .join()
            .map_err(|_| anyhow::anyhow!("AOEM storage owner join failed"))?;
        if let Some(reason) = self
            .client
            .state
            .failure
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            bail!("AOEM storage owner stopped after failure: {reason}");
        }
        Ok(())
    }
}

impl Drop for AoemSemanticGraphOwnerV1 {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl AoemSemanticGraphClientV1 {
    pub fn enter(&self) -> Result<AoemSemanticGraphClientScopeV1> {
        self.state.ensure_caller_thread()?;
        self.state.ensure_usable()?;
        if session_scope::has_active_scope() {
            bail!("AOEM local and remote graph scopes cannot overlap");
        }
        ACTIVE_CLIENT.with(|slot| {
            let mut slot = slot
                .try_borrow_mut()
                .context("AOEM remote graph scope is busy")?;
            if slot.upgrade().is_some() {
                bail!("AOEM remote graph scopes cannot be nested");
            }
            *slot = Arc::downgrade(&self.state);
            Ok(AoemSemanticGraphClientScopeV1 {
                state: self.state.clone(),
                _thread: PhantomData,
            })
        })
    }

    /// Try to enqueue one bounded storage stage without waiting for the owner.
    /// The closure runs on the same provider thread and may open local stores
    /// using the owner's exact path/runtime/configuration. It must own its input,
    /// avoid synchronous calls to this client, and not wait for work that needs
    /// this owner. Callers retain their per-stage memory/work and lock-order
    /// bounds; CPU candidate execution is not a storage stage.
    pub fn try_stage<T, F>(&self, stage: F) -> Result<AoemSemanticGraphStageAdmissionV1<T, F>>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T> + Send + 'static,
    {
        self.state.ensure_caller_thread()?;
        self.state.ensure_usable()?;
        let stage = Arc::new(Mutex::new(Some(stage)));
        let accepted_stage = stage.clone();
        let (reply, response) = mpsc::sync_channel(1);
        let ready = Arc::new(AtomicBool::new(false));
        let completion = StageReplyV1 {
            reply: Some(reply),
            ready: ready.clone(),
            caller: thread::current(),
        };
        let command = Command::Stage(Box::new(move |state, inner| {
            let result = state.ensure_usable().and_then(|()| {
                inner.ensure_usable()?;
                let stage = accepted_stage
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take()
                    .context("AOEM storage stage was already taken")?;
                match catch_unwind(AssertUnwindSafe(stage)) {
                    Ok(result) => result,
                    Err(_) => {
                        inner.poisoned.set(true);
                        state.fail("storage stage panicked; restart required".into());
                        bail!("AOEM storage stage panicked; restart required");
                    }
                }
            });
            // A closure cannot hide a failed admitted graph by swallowing its
            // error and returning Ok. The same local poison remains terminal.
            let result = if inner.poisoned.get() {
                state.fail("poisoned AOEM storage session after stage".into());
                result.and_then(|_| {
                    bail!("AOEM storage stage left its session poisoned; restart required")
                })
            } else {
                result
            };
            completion.complete(result);
        }));
        match self.state.commands.try_send(command) {
            Ok(()) => Ok(AoemSemanticGraphStageAdmissionV1::Accepted(
                AoemSemanticGraphStageHandleV1 {
                    response,
                    ready,
                    consumed: false,
                },
            )),
            Err(mpsc::TrySendError::Full(command)) => {
                drop(command);
                let stage = stage
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take()
                    .context("AOEM unstarted storage stage was lost")?;
                Ok(AoemSemanticGraphStageAdmissionV1::Backpressured(stage))
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                bail!("AOEM storage stage admission disconnected")
            }
        }
    }

    pub(super) fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.state.ensure_caller_thread()?;
        self.state.ensure_usable()?;
        validate_get_key(key)?;
        let (reply, result) = mpsc::sync_channel(1);
        self.state
            .commands
            .send(Command::Get(key.to_vec(), reply))
            .map_err(|_| anyhow::anyhow!("AOEM storage owner read admission disconnected"))?;
        result
            .recv()
            .context("AOEM storage owner read response disconnected")?
    }

    pub(super) fn commit(
        &self,
        request: AoemAtomicGraphRequestV1,
    ) -> Result<AoemAtomicGraphCommitReportV1> {
        self.state.ensure_caller_thread()?;
        self.state.ensure_usable()?;
        let prepared = PreparedGraphV1::new(request)?;
        let (reply, result) = mpsc::sync_channel(1);
        self.state
            .commands
            .send(Command::Commit(Box::new(prepared), reply))
            .map_err(|_| anyhow::anyhow!("AOEM storage owner commit admission disconnected"))?;
        result
            .recv()
            .context("AOEM storage owner commit response disconnected")?
    }
}

fn validate_get_key(key: &[u8]) -> Result<()> {
    if key.is_empty() {
        bail!("AOEM storage provider key must not be empty");
    }
    // Match the existing wire payload (database ID + read version + length).
    u32::try_from(
        key.len()
            .checked_add(20)
            .context("AOEM storage request size overflow")?,
    )
    .context("AOEM storage request payload exceeds u32")?;
    Ok(())
}

impl Drop for AoemSemanticGraphClientScopeV1 {
    fn drop(&mut self) {
        let _ = ACTIVE_CLIENT.try_with(|slot| {
            if let Ok(mut slot) = slot.try_borrow_mut() {
                if slot.ptr_eq(&Arc::downgrade(&self.state)) {
                    *slot = Weak::new();
                }
            }
        });
    }
}

pub(super) fn has_active_scope() -> bool {
    ACTIVE_CLIENT.with(|slot| slot.borrow().upgrade().is_some())
}

pub(super) fn scoped_client(
    runtime: &AoemRuntimeConfig,
    path: &Path,
    storage: &AoemStorageProviderConfigV1,
) -> Result<Option<AoemSemanticGraphClientV1>> {
    ACTIVE_CLIENT.with(|slot| {
        let Some(state) = slot.borrow().upgrade() else { return Ok(None); };
        state.ensure_usable()?;
        if !state.identity.matches(&ProviderIdentity::new(runtime, path, storage)?) {
            bail!("AOEM remote graph scope requires its unchanged database, runtime and storage configuration");
        }
        Ok(Some(AoemSemanticGraphClientV1 { state }))
    })
}

fn run(
    state: &ClientState,
    receiver: mpsc::Receiver<Command>,
    store: &AoemSemanticGraphStoreV1,
    inner: &SemanticGraphStoreInnerV1,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Stop => break,
            Command::Stage(stage) => stage(state, inner),
            Command::Get(key, reply) => {
                let result = state.ensure_usable().and_then(|()| store.get(&key));
                let _ = reply.send(result);
            }
            Command::Commit(prepared, reply) => {
                let result = state
                    .ensure_usable()
                    .and_then(|()| store.commit_prepared(*prepared));
                if inner.poisoned.get() {
                    state.fail(
                        result
                            .as_ref()
                            .err()
                            .map(|error| format!("{error:#}"))
                            .unwrap_or_else(|| "poisoned AOEM storage session".into()),
                    );
                }
                let _ = reply.send(result);
            }
            #[cfg(test)]
            Command::Poison(reply) => {
                inner.poisoned.set(true);
                state.fail("injected storage session poison".into());
                let _ = reply.send(());
            }
            #[cfg(test)]
            Command::Inspect(reply) => {
                let _ = reply.send((inner.database_id, thread::current().id()));
            }
            #[cfg(test)]
            Command::Park(entered, release) => {
                let _ = entered.send(());
                let _ = release.recv_timeout(Duration::from_secs(2));
            }
            #[cfg(test)]
            Command::Panic => panic!("injected AOEM storage owner panic"),
        }
    }
}

#[cfg(test)]
#[path = "semantic_graph_owner_tests.rs"]
mod tests;
