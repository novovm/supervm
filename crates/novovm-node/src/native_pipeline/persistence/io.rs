//! Bounded, single native I/O owner. Submission/polling performs no disk I/O and
//! never waits for a candidate. Preflight/readback yield every 64 keys; queued
//! point/bulk reads run between those steps. Only one writer is active, so two
//! candidates cannot race read-before-write immutability checks.
//!
//! The atomic WAL write itself is a synchronous indivisible I/O quantum, not a
//! hard latency guarantee. Packet preparation and startup recovery run outside
//! the node control loop. This is not yet a node scheduler or measured TPS.

use super::metadata::{MetaKey, MetaOutcome, MetaTransition, MetadataSnapshot};
use super::proof::{validate_blob, validate_identity, MAX_PROOF_BLOB_BYTES};
use super::store::{PersistProgress, BULK_KEYS};
use super::{
    CandidateStore, OpenMode, PersistedCandidate, PreparedCandidate, StoreConfig, StoredCandidate,
};
use crate::native_pipeline::state::tree::{read_state_value, validate_state_node_bytes, NodeHash};
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
    proof_client_claimed: bool,
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
    ProofRead {
        candidate: NodeHash,
        image: [u32; 8],
        reply: mpsc::Sender<Result<Option<Vec<u8>>>>,
    },
    ProofWrite {
        candidate: NodeHash,
        image: [u32; 8],
        blob: Arc<Vec<u8>>,
        reply: mpsc::Sender<Result<bool>>,
    },
    Recover {
        candidate: NodeHash,
        reply: mpsc::Sender<Result<Option<StoredCandidate>>>,
    },
    MetadataRead {
        keys: Vec<MetaKey>,
        reply: mpsc::Sender<Result<MetadataSnapshot>>,
    },
    MetadataApply {
        transition: MetaTransition,
        reply: mpsc::Sender<Result<MetaOutcome>>,
    },
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
    recovery_bytes: usize,
}

/// A read-only admission lane, sharing the SAME native owner and database.
/// Separate bounded reply accounting prevents unconsumed public query replies
/// from reserving every candidate-capture/write permit and deadlocking drain.
pub(crate) struct IoReadClient {
    sender: mpsc::SyncSender<Command>,
    usage: Arc<Mutex<Usage>>,
    budget: IoBudget,
}

/// Separate bounded control-metadata admission, not another database/owner.
/// Neither abandoned public-query replies nor held metadata replies reserve
/// the internal candidate-capture/write quota. Only the pipeline creates this.
pub(crate) struct IoMetadataClient {
    sender: mpsc::SyncSender<Command>,
    usage: Arc<Mutex<Usage>>,
    budget: IoBudget,
    recovery_bytes: usize,
}

// Covers retained input, native-write chunk copies, complete readback and a
// bounded bulk-read buffer. Not a prover working-memory or native allocator cap.
const PROOF_IO_BYTES: usize = 4 * MAX_PROOF_BLOB_BYTES + 64 * 1024;

/// One independently accounted request on the SAME I/O owner/database. Only
/// the pipeline can obtain this once; it is neither Clone nor a generic KV API.
/// Drop before draining IoService. A lost accepted reply is not cancellation.
pub(crate) struct IoProofClient {
    sender: mpsc::SyncSender<Command>,
    usage: Arc<Mutex<Usage>>,
}

impl IoProofClient {
    fn enqueue<T>(
        &self,
        operation: impl FnOnce(mpsc::Sender<Result<T>>) -> Operation,
    ) -> Result<Option<IoTicket<T>>> {
        enqueue(
            &self.sender,
            reserve(
                &self.usage,
                IoBudget {
                    requests: 1,
                    bytes: PROOF_IO_BYTES,
                },
                PROOF_IO_BYTES,
            )?,
            operation,
        )
    }

    pub(crate) fn try_read_nodes(
        &self,
        hashes: Vec<NodeHash>,
    ) -> Result<Option<IoTicket<NodeReadReply>>> {
        ensure!(
            !hashes.is_empty() && hashes.len() <= BULK_KEYS,
            "proof node request must contain 1..=64 hashes"
        );
        self.enqueue(|reply| Operation::Nodes { hashes, reply })
    }

    /// Bytes are untrusted until independently verified against the requested
    /// candidate's expected journal and configured image by the proof owner.
    pub(crate) fn try_read_proof(
        &self,
        candidate: NodeHash,
        image: [u32; 8],
    ) -> Result<Option<IoTicket<Option<Vec<u8>>>>> {
        validate_identity(candidate, image)?;
        self.enqueue(|reply| Operation::ProofRead {
            candidate,
            image,
            reply,
        })
    }

    /// true means an identical immutable attachment already existed. Caller
    /// retains its Arc on None (backpressure); Err after acceptance is never
    /// authority to retry an uncertain write or open another storage session.
    pub(crate) fn try_write_proof(
        &self,
        candidate: NodeHash,
        image: [u32; 8],
        blob: Arc<Vec<u8>>,
    ) -> Result<Option<IoTicket<bool>>> {
        validate_identity(candidate, image)?;
        validate_blob(&blob)?;
        self.enqueue(|reply| Operation::ProofWrite {
            candidate,
            image,
            blob,
            reply,
        })
    }
}

impl IoMetadataClient {
    /// Cold-start only. The caller must not use complete-candidate recovery in
    /// an ordinary consensus tick. One candidate may occupy the I/O owner.
    pub(crate) fn try_recover(
        &self,
        candidate: NodeHash,
    ) -> Result<Option<IoTicket<Option<StoredCandidate>>>> {
        enqueue(
            &self.sender,
            reserve(&self.usage, self.budget, self.recovery_bytes)?,
            |reply| Operation::Recover { candidate, reply },
        )
    }
    pub(crate) fn try_read(
        &self,
        keys: Vec<MetaKey>,
    ) -> Result<Option<IoTicket<MetadataSnapshot>>> {
        ensure!(
            (1..=8).contains(&keys.len()),
            "invalid metadata read key count"
        );
        let bytes = keys.len() * (512 * 1024 + 128);
        enqueue(
            &self.sender,
            reserve(&self.usage, self.budget, bytes)?,
            |reply| Operation::MetadataRead { keys, reply },
        )
    }

    pub(crate) fn try_apply(
        &self,
        transition: MetaTransition,
    ) -> Result<Option<IoTicket<MetaOutcome>>> {
        // Account for the owned expected/new values and bounded actual/readback
        // contents. The native allocator and DB cache are not a heap hard cap.
        let bytes = transition.retained_bytes() + 8 * 512 * 1024 + 1024;
        enqueue(
            &self.sender,
            reserve(&self.usage, self.budget, bytes)?,
            |reply| Operation::MetadataApply { transition, reply },
        )
    }
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

    /// Completion wakeup for the resident pipeline coordinator. Notification
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
        let recovery_bytes = config
            .packet_budget
            .max_bytes
            .checked_mul(2)
            .and_then(|n| n.checked_add(4096))
            .context("recovery reservation overflow")?;
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
            recovery_bytes,
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

    pub(crate) fn metadata_client(&self) -> Result<IoMetadataClient> {
        let notify = self
            .usage
            .lock()
            .map_err(|_| anyhow::anyhow!("I/O accounting poisoned"))?
            .notify
            .clone();
        Ok(IoMetadataClient {
            sender: self.sender.clone(),
            budget: self.budget,
            recovery_bytes: self.recovery_bytes,
            usage: Arc::new(Mutex::new(Usage {
                notify,
                ..Usage::default()
            })),
        })
    }

    /// Exactly one optional proof lane, with no share of the execution-capture,
    /// public-query or consensus-metadata admission accounts. The native owner
    /// and bounded command channel remain the original ones.
    pub(crate) fn proof_client(&self) -> Result<IoProofClient> {
        let mut owner = self
            .usage
            .lock()
            .map_err(|_| anyhow::anyhow!("I/O accounting poisoned"))?;
        ensure!(
            !owner.proof_client_claimed,
            "proof I/O client already taken"
        );
        owner.proof_client_claimed = true;
        Ok(IoProofClient {
            sender: self.sender.clone(),
            usage: Arc::new(Mutex::new(Usage {
                notify: owner.notify.clone(),
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
                Operation::ProofRead {
                    candidate,
                    image,
                    reply,
                } => {
                    let _ = reply.send(store.read_proof(candidate, image));
                    completed = true;
                }
                Operation::ProofWrite {
                    candidate,
                    image,
                    blob,
                    reply,
                } => {
                    let _ = reply.send(store.write_proof(candidate, image, &blob));
                    completed = true;
                }
                Operation::Recover { candidate, reply } => {
                    let _ = reply.send(store.recover(candidate));
                    completed = true;
                }
                Operation::MetadataRead { keys, reply } => {
                    let _ = reply.send(store.read_metadata(&keys));
                    completed = true;
                }
                Operation::MetadataApply { transition, reply } => {
                    let _ = reply.send(store.apply_metadata(&transition));
                    completed = true;
                }
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

#[cfg(test)]
mod proof_tests {
    use super::*;
    use std::time::Duration;

    fn service(capacity: usize) -> (IoService, mpsc::Receiver<Command>) {
        let (sender, receiver) = mpsc::sync_channel(capacity);
        (
            IoService {
                sender,
                worker: thread::spawn(|| {}),
                usage: Arc::new(Mutex::new(Usage::default())),
                budget: IoBudget {
                    requests: 1,
                    bytes: 1024,
                },
                recovery_bytes: 1024,
            },
            receiver,
        )
    }

    #[test]
    fn proof_lane_is_single_take_single_request_and_independent_of_capture_account() {
        let (service, receiver) = service(3);
        let proof = service.proof_client().unwrap();
        assert!(service.proof_client().is_err());
        let mut ticket = proof.try_read_proof([3; 32], [4; 8]).unwrap().unwrap();
        assert!(proof.try_read_nodes(vec![[7; 32]]).unwrap().is_none());
        assert_eq!(service.usage.lock().unwrap().requests, 0);
        let capture = service.try_read_nodes(vec![[8; 32]]).unwrap().unwrap();
        assert_eq!(service.usage.lock().unwrap().requests, 1);
        let Command { operation, permit } = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        let Operation::ProofRead {
            candidate,
            image,
            reply,
        } = operation
        else {
            panic!("wrong proof operation")
        };
        assert_eq!(candidate, [3; 32]);
        assert_eq!(image, [4; 8]);
        reply.send(Ok(Some(vec![9]))).unwrap();
        drop(permit);
        assert!(proof.try_read_nodes(vec![[7; 32]]).unwrap().is_none());
        assert_eq!(ticket.try_take().unwrap(), Some(Some(vec![9])));
        assert!(ticket.try_take().is_err());
        assert!(proof.try_read_nodes(vec![[7; 32]]).unwrap().is_some());
        drop(capture);
        drop(proof);
        drop(receiver);
        service.shutdown().unwrap();
    }

    #[test]
    fn proof_lane_rejects_oversize_bad_identity_and_node_counts_before_admission() {
        let (service, receiver) = service(1);
        let proof = service.proof_client().unwrap();
        assert!(proof.try_read_nodes(Vec::new()).is_err());
        assert!(proof.try_read_nodes(vec![[7; 32]; 65]).is_err());
        assert!(proof.try_read_proof([0; 32], [4; 8]).is_err());
        assert!(proof.try_read_proof([3; 32], [0; 8]).is_err());
        assert!(proof
            .try_write_proof([3; 32], [4; 8], Arc::new(Vec::new()))
            .is_err());
        assert!(proof
            .try_write_proof([3; 32], [4; 8], Arc::new(vec![0; MAX_PROOF_BLOB_BYTES + 1]))
            .is_err());
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        assert_eq!(proof.usage.lock().unwrap().requests, 0);
        drop(proof);
        drop(receiver);
        service.shutdown().unwrap();
    }

    #[test]
    fn proof_lane_drop_does_not_cancel_write_and_queue_backpressure_releases_permit() {
        let (service, receiver) = service(1);
        let proof = service.proof_client().unwrap();
        let capture = service.try_read_nodes(vec![[8; 32]]).unwrap().unwrap();
        let blob = Arc::new(vec![9]);
        assert!(proof
            .try_write_proof([3; 32], [4; 8], blob.clone())
            .unwrap()
            .is_none());
        assert_eq!(proof.usage.lock().unwrap().requests, 0);
        drop(receiver.recv_timeout(Duration::from_secs(1)).unwrap());
        drop(capture);
        let ticket = proof
            .try_write_proof([3; 32], [4; 8], blob.clone())
            .unwrap()
            .unwrap();
        drop(ticket);
        assert_eq!(proof.usage.lock().unwrap().requests, 1);
        let Command { operation, permit } = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        let Operation::ProofWrite {
            candidate,
            image,
            blob: accepted,
            reply,
        } = operation
        else {
            panic!("accepted write lost")
        };
        assert_eq!(candidate, [3; 32]);
        assert_eq!(image, [4; 8]);
        assert!(Arc::ptr_eq(&blob, &accepted));
        assert!(reply.send(Ok(false)).is_err());
        drop(permit);
        assert_eq!(proof.usage.lock().unwrap().requests, 0);
        drop(proof);
        drop(receiver);
        service.shutdown().unwrap();
    }
}
