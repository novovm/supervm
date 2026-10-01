//! Replacement-local candidate data pipeline. Signature/compilation and real
//! AOEM business work share a resident compute owner; frontier bulk reads and
//! atomic persistence share a separate resident I/O owner. The coordinator only
//! advances bounded capture cursors and passes owned messages. No per-key RPC,
//! per-batch engine initialization, live DB in callbacks, legacy node or slots.
//!
//! This service does not choose a canonical parent, reserve a global nonce,
//! create a mempool ACK, vote, prove execution validity or finalize a block.
//! Callers supply independently authorized parent/domain policy. Different jobs
//! may deliberately be competing local candidates, not a selected chain.

mod compute;
mod driver;

use crate::business::direct_nov_fee::DirectNovFeePolicy;
use crate::business::nov_transfer_batch::ExecutionObservation;
use crate::execution::plan::{BatchContext, PlanBudget};
use crate::ingress::batch::AuthenticationBudget;
use crate::persistence::io::{IoBudget, IoReadClient, IoService, IoTicket};
use crate::persistence::{OpenMode, PersistedCandidate, PreparedCandidate, StoreConfig};
use crate::state::frontier::CaptureBudget;
use crate::state::tree::NodeHash;
use anyhow::{ensure, Context, Result};
use std::sync::{mpsc, Arc, Mutex, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Local operational bounds, not production protocol activation parameters.
#[derive(Clone)]
pub struct PipelineConfig {
    pub store: StoreConfig,
    pub workers: u32,
    pub max_batches: usize,
    pub max_retained_bytes: usize,
    pub authentication: AuthenticationBudget,
    pub plan: PlanBudget,
    pub capture: CaptureBudget,
    pub compute_timeout: Duration,
    /// Internal capture/write quota. Public queries use a separate quota of
    /// `requests` tickets and at most `requests * 545` logical bytes, sharing the
    /// same native owner and database. Neither is a total allocator memory cap.
    pub io: IoBudget,
    pub capture_edge_quantum: usize,
}

impl PipelineConfig {
    fn validate(&self) -> Result<()> {
        ensure!(
            (1..=1024).contains(&self.max_batches) && self.max_retained_bytes > 0,
            "invalid pipeline admission budget"
        );
        ensure!(
            (1..=4096).contains(&self.capture_edge_quantum),
            "invalid capture scheduling quantum"
        );
        ensure!(
            self.workers > 0 && !self.compute_timeout.is_zero(),
            "invalid computation configuration"
        );
        ensure!(
            self.capture.keys > 0 && self.capture.nodes > 0 && self.capture.bytes > 0,
            "invalid capture resource bounds"
        );
        ensure!(
            self.authentication.transactions > 0
                && self.authentication.transaction_bytes > 0
                && self.authentication.body_bytes > 0
                && self.plan.transactions > 0
                && self.plan.transaction_bytes > 0
                && self.plan.body_bytes > 0
                && self.plan.access_keys > 0,
            "invalid input admission bounds"
        );
        Ok(())
    }
}

/// Construct on the ingress/assembly owner, not inside a latency-sensitive
/// network poll: counting an entire incoming body is intentionally done ONCE.
/// This is unverified input, not an authenticated transaction or nonce lease.
pub struct BatchRequest {
    request: Box<compute::PrepareRequest>,
    body_bytes: usize,
    max_transaction_bytes: usize,
    transaction_count: usize,
}

impl BatchRequest {
    pub fn new(
        raw_transactions: Vec<Vec<u8>>,
        context: BatchContext,
        policy: DirectNovFeePolicy,
    ) -> Result<Self> {
        ensure!(!raw_transactions.is_empty(), "empty pipeline request");
        policy.validate()?;
        let body_bytes = raw_transactions.iter().try_fold(0usize, |total, raw| {
            ensure!(!raw.is_empty(), "empty pipeline transaction");
            total
                .checked_add(raw.len())
                .context("pipeline body size overflow")
        })?;
        let max_transaction_bytes = raw_transactions.iter().map(Vec::len).max().unwrap_or(0);
        let transaction_count = raw_transactions.len();
        Ok(Self {
            request: Box::new(compute::PrepareRequest {
                raw_transactions,
                context,
                policy,
            }),
            body_bytes,
            max_transaction_bytes,
            transaction_count,
        })
    }

    fn reservation(&self, config: &PipelineConfig) -> Result<usize> {
        let c = &self.request.context;
        let domain = config.store.domain;
        ensure!(
            c.chain_id == domain.chain_id
                && c.genesis_config_commitment == domain.genesis_config_commitment
                && c.protocol_commitment == domain.protocol_commitment,
            "pipeline request/store domain mismatch"
        );
        ensure!(
            self.transaction_count
                <= config
                    .authentication
                    .transactions
                    .min(config.plan.transactions)
                && self.max_transaction_bytes
                    <= config
                        .authentication
                        .transaction_bytes
                        .min(config.plan.transaction_bytes)
                && self.body_bytes <= config.authentication.body_bytes.min(config.plan.body_bytes),
            "pipeline request exceeds input budget"
        );
        // Reserve the maximum retained logical batch content through all stages,
        // including an unconsumed completed reply. This is not a total allocator
        // or native-engine memory cap. No data-dependent widening after admission.
        self.body_bytes
            .checked_mul(3)
            .and_then(|n| n.checked_add(config.capture.bytes))
            .and_then(|n| {
                config
                    .capture
                    .keys
                    .checked_mul(256)
                    .and_then(|keys| n.checked_add(keys))
            })
            .and_then(|n| {
                config
                    .store
                    .packet_budget
                    .max_bytes
                    .checked_mul(2)
                    .and_then(|packet| n.checked_add(packet))
            })
            .and_then(|n| n.checked_add(4096))
            .context("pipeline retained-content budget overflow")
    }
}

pub enum Submission {
    Accepted(PipelineTicket),
    /// Not accepted; the exact owned request is returned unchanged.
    Backpressured(BatchRequest),
}

/// Durable LOCAL content only. The packet cannot be substituted after execution
/// or promoted to finality by obtaining this reply.
pub struct DurableBatch {
    pub packet: Arc<PreparedCandidate>,
    pub persisted: PersistedCandidate,
    pub observation: ExecutionObservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureStage {
    Prepare,
    Capture,
    Execute,
    Persist,
}

/// Kept on errors after a plan is known, especially for explicit recovery of an
/// ambiguous persistence result. Never automatically re-execute a failed write.
#[derive(Debug)]
pub struct PipelineFailure {
    pub stage: FailureStage,
    pub candidate_id: Option<NodeHash>,
    pub message: String,
}
impl std::fmt::Display for PipelineFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "candidate pipeline {:?} failed (candidate {:?}): {}",
            self.stage, self.candidate_id, self.message
        )
    }
}
impl std::error::Error for PipelineFailure {}

#[derive(Default)]
struct Usage {
    batches: usize,
    bytes: usize,
}
struct Permit {
    usage: Arc<Mutex<Usage>>,
    bytes: usize,
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut usage = self.usage.lock().unwrap_or_else(|error| error.into_inner());
        usage.batches -= 1;
        usage.bytes -= self.bytes;
    }
}

/// A lost/dropped reply is not cancellation. It also does not prove that no
/// candidate was stored. Query/recover the known candidate explicitly.
pub struct PipelineTicket {
    receiver: mpsc::Receiver<Result<DurableBatch>>,
    permit: Option<Arc<Permit>>,
}
impl PipelineTicket {
    pub fn try_take(&mut self) -> Result<Option<DurableBatch>> {
        ensure!(self.permit.is_some(), "pipeline ticket already consumed");
        match self.receiver.try_recv() {
            Ok(result) => {
                self.permit.take();
                result.map(Some)
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.permit.take();
                anyhow::bail!(
                    "candidate coordinator disconnected; accepted request outcome unknown"
                )
            }
        }
    }
    /// Explicit blocking startup/test operation, never the control loop.
    pub fn wait(self) -> Result<DurableBatch> {
        self.receiver
            .recv()
            .context("candidate coordinator disconnected; accepted request outcome unknown")?
    }
}

struct Command {
    request: BatchRequest,
    reply: mpsc::Sender<Result<DurableBatch>>,
    permit: Arc<Permit>,
}

pub struct CandidatePipeline {
    config: PipelineConfig,
    sender: Option<mpsc::SyncSender<Command>>,
    worker: Option<JoinHandle<Result<()>>>,
    io: Option<Arc<IoService>>,
    queries: Option<IoReadClient>,
    usage: Arc<Mutex<Usage>>,
}

impl CandidatePipeline {
    /// Blocking startup before network activation. Each native session is opened
    /// once inside its dedicated owner. No engine is initialized in try_submit.
    pub fn start(config: PipelineConfig, mode: OpenMode) -> Result<Self> {
        config.validate()?;
        let (sender, receiver) = mpsc::sync_channel(config.max_batches);
        let (ready, ready_receiver) = mpsc::sync_channel(1);
        let worker_config = config.clone();
        let worker = thread::Builder::new()
            .name("novovm-candidate-pipeline".into())
            .spawn(move || driver::start(worker_config, mode, receiver, ready))
            .context("start candidate pipeline")?;
        let io = ready_receiver
            .recv()
            .context("candidate pipeline failed during startup")??;
        let queries = io.read_client()?;
        Ok(Self {
            config,
            sender: Some(sender),
            worker: Some(worker),
            io: Some(io),
            queries: Some(queries),
            usage: Arc::new(Mutex::new(Usage::default())),
        })
    }

    pub fn try_submit(&self, request: BatchRequest) -> Result<Submission> {
        let bytes = request.reservation(&self.config)?;
        ensure!(
            bytes <= self.config.max_retained_bytes,
            "one batch exceeds pipeline retained-content budget"
        );
        let mut usage = match self.usage.try_lock() {
            Ok(usage) => usage,
            Err(TryLockError::WouldBlock) => return Ok(Submission::Backpressured(request)),
            Err(TryLockError::Poisoned(_)) => {
                anyhow::bail!("pipeline admission accounting poisoned")
            }
        };
        if usage.batches >= self.config.max_batches
            || bytes > self.config.max_retained_bytes - usage.bytes
        {
            return Ok(Submission::Backpressured(request));
        }
        usage.batches += 1;
        usage.bytes += bytes;
        drop(usage);
        let permit = Arc::new(Permit {
            usage: self.usage.clone(),
            bytes,
        });
        let (reply, receiver) = mpsc::channel();
        let command = Command {
            request,
            reply,
            permit: permit.clone(),
        };
        match self
            .sender
            .as_ref()
            .context("pipeline closed")?
            .try_send(command)
        {
            Ok(()) => {
                self.worker
                    .as_ref()
                    .context("pipeline worker unavailable")?
                    .thread()
                    .unpark();
                Ok(Submission::Accepted(PipelineTicket {
                    receiver,
                    permit: Some(permit),
                }))
            }
            Err(mpsc::TrySendError::Full(command)) => {
                Ok(Submission::Backpressured(command.request))
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                anyhow::bail!("pipeline unavailable; request not accepted")
            }
        }
    }

    pub fn try_read_value(
        &self,
        root: NodeHash,
        key: Vec<u8>,
    ) -> Result<Option<IoTicket<Option<Vec<u8>>>>> {
        self.queries
            .as_ref()
            .context("pipeline I/O closed")?
            .try_read_value(root, key)
    }

    /// Administrative drain, not a network tick. A caller abandoning tickets
    /// does not abandon accepted work. Drop itself only disconnects/wakes.
    pub fn shutdown(mut self) -> Result<()> {
        self.sender.take();
        self.queries.take();
        let worker = self.worker.take().context("pipeline already shut down")?;
        worker.thread().unpark();
        let result = worker
            .join()
            .map_err(|_| anyhow::anyhow!("candidate coordinator panicked; outcome unknown"))?;
        let io = self.io.take().context("pipeline I/O missing")?;
        Arc::try_unwrap(io)
            .map_err(|_| anyhow::anyhow!("pipeline retained an I/O client after drain"))?
            .shutdown()?;
        result
    }
}

impl Drop for CandidatePipeline {
    fn drop(&mut self) {
        self.sender.take();
        self.queries.take();
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
    }
}

#[cfg(test)]
mod tests;
