//! Bounded, single native I/O owner. Submission/polling performs no disk I/O and
//! never waits for a candidate. Preflight/readback yield every 64 keys; queued
//! point/bulk reads run between those steps. Only one writer is active, so two
//! candidates cannot race read-before-write immutability checks.
//!
//! The atomic WAL write itself is a synchronous indivisible I/O quantum, not a
//! hard latency guarantee. Packet preparation and startup recovery run outside
//! the node control loop. This is not yet a node scheduler or measured TPS.

use super::store::{PersistProgress, BULK_KEYS};
use super::{CandidateStore, OpenMode, PersistedCandidate, PreparedCandidate, StoreConfig};
use crate::state::tree::{read_state_value, validate_state_node_bytes, NodeHash};
use anyhow::{ensure, Context, Result};
use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex, TryLockError};
use std::thread::{self, JoinHandle};

pub type NodeReadReply = Vec<Option<Vec<u8>>>;

/// Local admission limits. Bytes count retained logical request content plus
/// bounded reply payload, not Rust/FFI/DB allocator overhead or shared Arc copies.
#[derive(Clone, Copy, Debug)]
pub struct IoBudget {
    pub requests: usize,
    pub bytes: usize,
}

impl Default for IoBudget {
    fn default() -> Self {
        Self {
            requests: 16,
            bytes: 128 * 1024 * 1024,
        }
    }
}

#[derive(Default)]
struct Usage {
    requests: usize,
    bytes: usize,
    notify: Option<thread::Thread>,
}

struct Permit {
    usage: Arc<Mutex<Usage>>,
    bytes: usize,
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut usage = self.usage.lock().unwrap_or_else(|error| error.into_inner());
        usage.requests -= 1;
        usage.bytes -= self.bytes;
        let notify = usage.notify.clone();
        drop(usage);
        if let Some(thread) = &notify {
            thread.unpark();
        }
    }
}

/// Dropping a ticket abandons notification, not accepted storage work. A permit
/// remains owned by the worker until it finishes, and by this ticket until the
/// reply is consumed/dropped. A disconnected reply is an UNKNOWN outcome; the
/// caller must explicitly reopen/recover, never infer that no write occurred.
pub struct IoTicket<T> {
    receiver: mpsc::Receiver<Result<T>>,
    permit: Option<Arc<Permit>>,
}

impl<T> IoTicket<T> {
    pub fn try_take(&mut self) -> Result<Option<T>> {
        ensure!(self.permit.is_some(), "I/O ticket already consumed");
        match self.receiver.try_recv() {
            Ok(result) => {
                self.permit.take();
                result.map(Some)
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.permit.take();
                anyhow::bail!("I/O owner disconnected; operation outcome unknown")
            }
        }
    }

    /// Blocking test/startup convenience, never a node-control-loop operation.
    pub fn wait(self) -> Result<T> {
        self.receiver
            .recv()
            .context("I/O owner disconnected; operation outcome unknown")?
    }
}

enum Operation {
    Persist {
        packet: Arc<PreparedCandidate>,
        reply: mpsc::Sender<Result<PersistedCandidate>>,
    },
    Nodes {
        hashes: Vec<NodeHash>,
        reply: mpsc::Sender<Result<Vec<Option<Vec<u8>>>>>,
    },
    Value {
        root: NodeHash,
        key: Vec<u8>,
        reply: mpsc::Sender<Result<Option<Vec<u8>>>>,
    },
}

struct Command {
    operation: Operation,
    permit: Arc<Permit>,
}

struct WriteJob {
    packet: Arc<PreparedCandidate>,
    progress: PersistProgress,
    reply: mpsc::Sender<Result<PersistedCandidate>>,
    _permit: Arc<Permit>,
}

pub struct IoService {
    sender: mpsc::SyncSender<Command>,
    worker: JoinHandle<()>,
    usage: Arc<Mutex<Usage>>,
    budget: IoBudget,
}

/// A read-only admission lane, sharing the SAME native owner and database.
/// Separate bounded reply accounting prevents unconsumed public query replies
/// from reserving every candidate-capture/write permit and deadlocking drain.
pub(crate) struct IoReadClient {
    sender: mpsc::SyncSender<Command>,
    usage: Arc<Mutex<Usage>>,
    budget: IoBudget,
}

impl IoReadClient {
    pub(crate) fn try_read_value(
        &self,
        root: NodeHash,
        key: Vec<u8>,
    ) -> Result<Option<IoTicket<Option<Vec<u8>>>>> {
        ensure!(
            !key.is_empty() && key.len() <= 256,
            "I/O state key length invalid"
        );
        let permit = reserve(&self.usage, self.budget, key.len() + 32 + 257)?;
        enqueue(&self.sender, permit, |reply| Operation::Value {
            root,
            key,
            reply,
        })
    }
}

fn reserve(
    accounting: &Arc<Mutex<Usage>>,
    budget: IoBudget,
    bytes: usize,
) -> Result<Option<Arc<Permit>>> {
    ensure!(bytes <= budget.bytes, "request exceeds I/O byte budget");
    let mut usage = match accounting.try_lock() {
        Ok(usage) => usage,
        Err(TryLockError::WouldBlock) => return Ok(None),
        Err(TryLockError::Poisoned(_)) => anyhow::bail!("I/O admission accounting poisoned"),
    };
    if usage.requests >= budget.requests || bytes > budget.bytes - usage.bytes {
        return Ok(None);
    }
    usage.requests += 1;
    usage.bytes += bytes;
    Ok(Some(Arc::new(Permit {
        usage: accounting.clone(),
        bytes,
    })))
}

fn enqueue<T>(
    sender: &mpsc::SyncSender<Command>,
    permit: Option<Arc<Permit>>,
    operation: impl FnOnce(mpsc::Sender<Result<T>>) -> Operation,
) -> Result<Option<IoTicket<T>>> {
    let Some(permit) = permit else {
        return Ok(None);
    };
    let (reply, receiver) = mpsc::channel();
    let command = Command {
        operation: operation(reply),
        permit: permit.clone(),
    };
    match sender.try_send(command) {
        Ok(()) => Ok(Some(IoTicket {
            receiver,
            permit: Some(permit),
        })),
        Err(mpsc::TrySendError::Full(_)) => Ok(None),
        Err(mpsc::TrySendError::Disconnected(_)) => {
            anyhow::bail!("candidate I/O owner unavailable")
        }
    }
}

impl IoService {
    /// Blocking initialization on a new thread; call before activating network
    /// service. Opening never transfers a live native handle across threads.
    pub fn start(config: StoreConfig, mode: OpenMode, budget: IoBudget) -> Result<Self> {
        Self::start_with_notify(config, mode, budget, None)
    }

    /// Completion wakeup for the replacement pipeline coordinator. Notification
    /// has no data/authority; the receiving ticket still owns the exact result.
    pub(crate) fn start_with_notify(
        config: StoreConfig,
        mode: OpenMode,
        budget: IoBudget,
        notify: Option<thread::Thread>,
    ) -> Result<Self> {
        ensure!(
            budget.requests > 0 && budget.requests <= 65_536 && budget.bytes > 0,
            "invalid I/O admission budget"
        );
        let (sender, receiver) = mpsc::sync_channel(budget.requests);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let usage = Arc::new(Mutex::new(Usage {
            notify: notify.clone(),
            ..Usage::default()
        }));
        let worker = thread::Builder::new()
            .name("novovm-candidate-io".into())
            .spawn(move || match CandidateStore::open(config, mode) {
                Ok(store) => {
                    if ready_tx.send(Ok(())).is_ok() {
                        run_owner(store, receiver, notify);
                    }
                }
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                }
            })
            .context("start candidate I/O owner")?;
        ready_rx
            .recv()
            .context("candidate I/O owner failed during startup")??;
        Ok(Self {
            sender,
            worker,
            usage,
            budget,
        })
    }

    fn reserve(&self, bytes: usize) -> Result<Option<Arc<Permit>>> {
        reserve(&self.usage, self.budget, bytes)
    }

    fn enqueue<T>(
        &self,
        bytes: usize,
        operation: impl FnOnce(mpsc::Sender<Result<T>>) -> Operation,
    ) -> Result<Option<IoTicket<T>>> {
        enqueue(&self.sender, self.reserve(bytes)?, operation)
    }

    /// One separately bounded public query lane for the pipeline. This opens no
    /// new engine or DB. Must drop its sender before joining the I/O owner.
    pub(crate) fn read_client(&self) -> Result<IoReadClient> {
        let budget = IoBudget {
            requests: self.budget.requests,
            bytes: self.budget.requests * (256 + 32 + 257),
        };
        let notify = self
            .usage
            .lock()
            .map_err(|_| anyhow::anyhow!("I/O accounting poisoned"))?
            .notify
            .clone();
        Ok(IoReadClient {
            sender: self.sender.clone(),
            budget,
            usage: Arc::new(Mutex::new(Usage {
                notify,
                ..Usage::default()
            })),
        })
    }

    /// None is backpressure, NOT acceptance. The caller retains its Arc and can
    /// submit later. Once accepted, a lost reply never authorizes blind retry.
    pub fn try_persist(
        &self,
        packet: Arc<PreparedCandidate>,
    ) -> Result<Option<IoTicket<PersistedCandidate>>> {
        let bytes = packet
            .record_bytes()
            .checked_mul(2)
            .and_then(|n| n.checked_add(256))
            .context("candidate I/O retained byte size overflow")?;
        self.enqueue(bytes, |reply| Operation::Persist { packet, reply })
    }

    /// Bulk frontier reads, not one cross-thread RPC per state key. A consumer
    /// must schedule bounded frontier levels; no synchronous proxy is provided.
    pub fn try_read_nodes(&self, hashes: Vec<NodeHash>) -> Result<Option<IoTicket<NodeReadReply>>> {
        ensure!(
            !hashes.is_empty() && hashes.len() <= BULK_KEYS,
            "I/O node request must contain 1..=64 hashes"
        );
        let bytes = hashes.len() * (32 + 291 + 1);
        self.enqueue(bytes, |reply| Operation::Nodes { hashes, reply })
    }

    /// One authenticated-tree path, at most 257 nodes; not a whole-state scan.
    pub fn try_read_value(
        &self,
        root: NodeHash,
        key: Vec<u8>,
    ) -> Result<Option<IoTicket<Option<Vec<u8>>>>> {
        ensure!(
            !key.is_empty() && key.len() <= 256,
            "I/O state key length invalid"
        );
        self.enqueue(key.len() + 32 + 257, |reply| Operation::Value {
            root,
            key,
            reply,
        })
    }

    /// Explicit blocking administrative drain. Ordinary Drop only disconnects
    /// submission and lets the thread finish accepted work without joining.
    pub fn shutdown(self) -> Result<()> {
        drop(self.sender);
        self.worker
            .join()
            .map_err(|_| anyhow::anyhow!("candidate I/O owner panicked; in-flight outcome unknown"))
    }
}

fn run_owner(
    store: CandidateStore,
    receiver: mpsc::Receiver<Command>,
    notify: Option<thread::Thread>,
) {
    let mut pending = VecDeque::new();
    let mut active: Option<WriteJob> = None;
    let mut connected = true;
    while connected || active.is_some() || !pending.is_empty() {
        let mut completed = false;
        // At most one admitted command before advancing the active writer:
        // reads cannot starve writes; writer preflight cannot monopolize reads.
        let command = if active.is_none() && pending.is_empty() {
            receiver.recv().ok()
        } else {
            match receiver.try_recv() {
                Ok(command) => Some(command),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    connected = false;
                    None
                }
            }
        };
        if let Some(command) = command {
            match command.operation {
                Operation::Persist { packet, reply } => {
                    pending.push_back((packet, reply, command.permit))
                }
                Operation::Nodes { hashes, reply } => {
                    let keys: Vec<_> = hashes
                        .iter()
                        .map(|hash| super::packet::node_key(*hash))
                        .collect();
                    let result = store.read_relative(&keys).and_then(|values| {
                        for (hash, value) in hashes.iter().zip(&values) {
                            if let Some(bytes) = value {
                                validate_state_node_bytes(hash, bytes)?;
                            }
                        }
                        Ok(values)
                    });
                    let _ = reply.send(result);
                    completed = true;
                }
                Operation::Value { root, key, reply } => {
                    let _ = reply.send(read_state_value(&store, root, &key));
                    completed = true;
                }
            }
        } else if active.is_none() && pending.is_empty() {
            connected = false;
        }
        if active.is_none() {
            if let Some((packet, reply, permit)) = pending.pop_front() {
                match PersistProgress::new(&store, &packet) {
                    Ok(progress) => {
                        active = Some(WriteJob {
                            packet,
                            progress,
                            reply,
                            _permit: permit,
                        })
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error));
                        completed = true;
                    }
                }
            }
        }
        if let Some(job) = &mut active {
            match job.progress.step(&store, &job.packet) {
                Ok(None) => {}
                result => {
                    let result = result
                        .and_then(|done| done.context("candidate persistence completion missing"));
                    let _ = job.reply.send(result);
                    completed = true;
                    active = None;
                }
            }
        }
        if completed {
            if let Some(thread) = &notify {
                thread.unpark();
            }
        }
    }
}

#[cfg(test)]
mod tests;
