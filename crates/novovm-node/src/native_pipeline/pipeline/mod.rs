//! Original-node resident candidate pipeline, migrated from a7db795. Signature/compilation and real
//! AOEM business work share a resident compute owner; frontier bulk reads and
//! atomic persistence share a separate resident I/O owner. The coordinator only
//! advances bounded capture cursors and passes owned messages. No per-key RPC,
//! per-batch engine initialization, live DB in callbacks, or legacy slots.
//!
//! This service does not choose a canonical parent, reserve a global nonce,
//! create a mempool ACK, vote, prove execution validity or finalize a block.
//! Callers supply independently authorized parent/domain policy. Different jobs
//! may deliberately be competing local candidates, not a selected chain.

mod admission;
mod authentication;
mod compute;
mod driver;
mod seed;
pub use crate::native_pipeline::ingress::batch::{AdmissionInput, AdmissionRow};
pub use admission::{
    RejectedSignatureAdmissionSubmission, SignatureAdmissionOutput, SignatureAdmissionRequest,
    SignatureAdmissionSubmission, SignatureAdmissionTicket,
};
pub use authentication::{
    AuthenticatedBody, AuthenticatedRequest, AuthenticatedSubmission, AuthenticationRequest,
    AuthenticationSubmission, AuthenticationTicket, RejectedAuthenticatedSubmission,
    RejectedAuthenticationSubmission,
};

use crate::native_pipeline::business::direct_nov_fee::DirectNovFeePolicy;
use crate::native_pipeline::business::nov_transfer_batch::ExecutionObservation;
use crate::native_pipeline::execution::plan::{BatchContext, PlanBudget};
use crate::native_pipeline::ingress::apfl::ApflTransferBatch;
use crate::native_pipeline::ingress::batch::{AuthenticationBudget, BatchSource};
use crate::native_pipeline::persistence::io::{
    IoBudget, IoMetadataClient, IoReadClient, IoService, IoTicket,
};
use crate::native_pipeline::persistence::metadata::{
    MetaKey, MetaOutcome, MetaTransition, MetadataSnapshot,
};
use crate::native_pipeline::persistence::{
    OpenMode, PersistedCandidate, PreparedCandidate, StoreConfig,
};
use crate::native_pipeline::state::frontier::CaptureBudget;
use crate::native_pipeline::state::tree::NodeHash;
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
    /// same native owner and database. Consensus metadata has another `io`
    /// quota so retained metadata replies cannot consume capture reservations.
    /// These are logical-content budgets, not a total allocator memory cap.
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
        Self::from_source(BatchSource::Raw(raw_transactions), context, policy)
    }

    pub fn from_apfl(
        batch: Arc<ApflTransferBatch>,
        context: BatchContext,
        policy: DirectNovFeePolicy,
    ) -> Result<Self> {
        Self::from_source(BatchSource::Apfl(batch), context, policy)
    }

    fn from_source(
        raw_transactions: BatchSource,
        context: BatchContext,
        policy: DirectNovFeePolicy,
    ) -> Result<Self> {
        ensure!(!raw_transactions.is_empty(), "empty pipeline request");
        policy.validate()?;
        let (body_bytes, max_transaction_bytes) = raw_transactions.sizes()?;
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
        Self::retained_reservation(self.body_bytes, config)
    }

    fn retained_reservation(body_bytes: usize, config: &PipelineConfig) -> Result<usize> {
        body_bytes
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

/// Admission failed BEFORE acceptance. The exact owned allocation is returned
/// so a latency-sensitive caller can retire it on its body/assembly owner.
/// Moving this value is O(1); dropping it may destroy a complete large body.
/// Deliberately not an `Error`: callers must explicitly handle the request
/// rather than silently discarding it through an error conversion.
pub struct RejectedSubmission {
    pub request: BatchRequest,
    pub error: anyhow::Error,
}

impl std::fmt::Debug for RejectedSubmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RejectedSubmission")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

/// Durable LOCAL content only. The packet cannot be substituted after execution
/// or promoted to finality by obtaining this reply.
pub struct DurableBatch {
    pub packet: Arc<PreparedCandidate>,
    pub persisted: PersistedCandidate,
    pub observation: ExecutionObservation,
    /// Read-locality counters, not proof, parallelism or throughput claims.
    pub capture: CaptureObservation,
    candidate: DurableCandidate,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaptureObservation {
    pub seed_nodes: usize,
    pub storage_requests: usize,
    pub storage_nodes: usize,
}

impl DurableBatch {
    /// Immutable content completion, independent of the public diagnostic fields.
    pub fn candidate(&self) -> &DurableCandidate {
        &self.candidate
    }
}

/// Local content verified by this resident pipeline after atomic persistence.
/// Not a cached permission to sign or publish. Consensus still checks the live
/// signing state, configured parent and round on EVERY action. A new pipeline
/// (including restart) cannot reuse this completion. No public constructor.
///
/// ```compile_fail
/// use novovm_node::native_pipeline::{persistence::PreparedCandidate, pipeline::DurableCandidate};
/// use std::sync::Arc;
/// fn forge(packet: Arc<PreparedCandidate>) -> DurableCandidate {
///     DurableCandidate { owner: Arc::new(()), packet }
/// }
/// ```
#[derive(Clone)]
pub struct DurableCandidate {
    owner: Arc<()>,
    packet: Arc<PreparedCandidate>,
}

impl DurableCandidate {
    pub fn packet(&self) -> &PreparedCandidate {
        &self.packet
    }

    pub(crate) fn bind_to(&self, owner: &Arc<()>) -> Result<&PreparedCandidate> {
        ensure!(
            Arc::ptr_eq(&self.owner, owner),
            "durable candidate belongs to another pipeline; recover content explicitly"
        );
        Ok(&self.packet)
    }
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
    background: usize,
    ingress: usize,
}
struct Permit {
    usage: Arc<Mutex<Usage>>,
    bytes: usize,
    background: bool,
    ingress: bool,
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut usage = self.usage.lock().unwrap_or_else(|error| error.into_inner());
        usage.batches -= usize::from(!self.ingress);
        usage.bytes -= self.bytes;
        usage.background -= usize::from(self.background);
        usage.ingress -= usize::from(self.ingress);
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
    background: bool,
}

enum DriverMessage {
    Batch(Command),
    Authenticate(authentication::AuthenticationCommand),
    Bind(authentication::BindCommand),
    Admission(admission::AdmissionCommand),
}

pub struct CandidatePipeline {
    config: PipelineConfig,
    sender: Option<mpsc::SyncSender<DriverMessage>>,
    worker: Option<JoinHandle<Result<()>>>,
    io: Option<Arc<IoService>>,
    queries: Option<IoReadClient>,
    metadata: Option<IoMetadataClient>,
    identity: Arc<()>,
    usage: Arc<Mutex<Usage>>,
}

impl CandidatePipeline {
    /// Blocking startup before network activation. Each native session is opened
    /// once inside its dedicated owner. No engine is initialized in try_submit.
    pub fn start(config: PipelineConfig, mode: OpenMode) -> Result<Self> {
        config.validate()?;
        let (sender, receiver) = mpsc::sync_channel(config.max_batches + 1);
        let (ready, ready_receiver) = mpsc::sync_channel(1);
        let worker_config = config.clone();
        let identity = Arc::new(());
        let worker_identity = identity.clone();
        let worker = thread::Builder::new()
            .name("novovm-candidate-pipeline".into())
            .spawn(move || driver::start(worker_config, mode, receiver, ready, worker_identity))
            .context("start candidate pipeline")?;
        let io = ready_receiver
            .recv()
            .context("candidate pipeline failed during startup")??;
        let queries = io.read_client()?;
        let metadata = io.metadata_client()?;
        Ok(Self {
            config,
            sender: Some(sender),
            worker: Some(worker),
            io: Some(io),
            queries: Some(queries),
            metadata: Some(metadata),
            identity,
            usage: Arc::new(Mutex::new(Usage::default())),
        })
    }

    /// Compatibility admission for synchronous callers/tests. Unlike
    /// `try_submit_owned`, this wrapper destroys rejected input on the caller;
    /// do not use it inside a latency-sensitive control loop.
    pub fn try_submit(&self, request: BatchRequest) -> Result<Submission> {
        self.try_submit_owned(request)
            .map_err(|rejected| rejected.error)
    }

    /// Every non-accepted path returns the exact original request, including
    /// invalid domain/limits, closed service, poisoned accounting and a broken
    /// channel. Successful enqueue ALWAYS returns an accepted ticket: a later
    /// coordinator failure has an unknown outcome, never permission to resubmit.
    pub fn try_submit_owned(
        &self,
        request: BatchRequest,
    ) -> std::result::Result<Submission, RejectedSubmission> {
        self.try_submit_with_priority(request, false)
    }

    /// At most one optional background job, reserving one whole ordinary job's
    /// maximum logical content and admission slot. This is scheduling only,
    /// never authority to use a speculative parent or publish its result.
    pub(crate) fn try_submit_background_owned(
        &self,
        request: BatchRequest,
    ) -> std::result::Result<Submission, RejectedSubmission> {
        self.try_submit_with_priority(request, true)
    }

    fn try_submit_with_priority(
        &self,
        request: BatchRequest,
        background: bool,
    ) -> std::result::Result<Submission, RejectedSubmission> {
        let bytes = match request.reservation(&self.config) {
            Ok(bytes) => bytes,
            Err(error) => return Err(RejectedSubmission { request, error }),
        };
        // Check all fallible local service prerequisites BEFORE reserving or
        // enqueueing. The worker cannot be looked up with `?` after try_send.
        let Some(sender) = self.sender.as_ref() else {
            return Err(RejectedSubmission {
                request,
                error: anyhow::anyhow!("pipeline closed"),
            });
        };
        let Some(worker) = self.worker.as_ref() else {
            return Err(RejectedSubmission {
                request,
                error: anyhow::anyhow!("pipeline worker unavailable"),
            });
        };
        let permit = match self.reserve(bytes, background) {
            Ok(Some(permit)) => permit,
            Ok(None) => return Ok(Submission::Backpressured(request)),
            Err(error) => return Err(RejectedSubmission { request, error }),
        };
        let (reply, receiver) = mpsc::channel();
        let command = Command {
            request,
            reply,
            permit: permit.clone(),
            background,
        };
        match sender.try_send(DriverMessage::Batch(command)) {
            Ok(()) => {
                worker.thread().unpark();
                Ok(Submission::Accepted(PipelineTicket {
                    receiver,
                    permit: Some(permit),
                }))
            }
            Err(mpsc::TrySendError::Full(DriverMessage::Batch(command))) => {
                Ok(Submission::Backpressured(command.request))
            }
            Err(mpsc::TrySendError::Disconnected(DriverMessage::Batch(command))) => {
                Err(RejectedSubmission {
                    request: command.request,
                    error: anyhow::anyhow!("pipeline unavailable; request not accepted"),
                })
            }
            Err(_) => unreachable!("typed pipeline command changed while sending"),
        }
    }

    fn reserve(&self, bytes: usize, background: bool) -> Result<Option<Arc<Permit>>> {
        ensure!(
            bytes <= self.config.max_retained_bytes,
            "one batch exceeds pipeline retained-content budget"
        );
        let current_reserve = if background {
            BatchRequest::retained_reservation(
                self.config
                    .authentication
                    .body_bytes
                    .min(self.config.plan.body_bytes),
                &self.config,
            )?
        } else {
            0
        };
        let mut usage = match self.usage.try_lock() {
            Ok(usage) => usage,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(_)) => {
                anyhow::bail!("pipeline admission accounting poisoned");
            }
        };
        if usage.batches >= self.config.max_batches
            || bytes > self.config.max_retained_bytes.saturating_sub(usage.bytes)
            || (background
                && (usage.background != 0
                    || usage.batches >= self.config.max_batches.saturating_sub(1)
                    || current_reserve
                        > self
                            .config
                            .max_retained_bytes
                            .saturating_sub(usage.bytes)
                            .saturating_sub(bytes)))
        {
            return Ok(None);
        }
        usage.batches += 1;
        usage.bytes += bytes;
        usage.background += usize::from(background);
        drop(usage);
        Ok(Some(Arc::new(Permit {
            usage: self.usage.clone(),
            bytes,
            background,
            ingress: false,
        })))
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

    pub(crate) fn try_read_consensus_metadata(
        &self,
        keys: Vec<MetaKey>,
    ) -> Result<Option<IoTicket<MetadataSnapshot>>> {
        self.metadata
            .as_ref()
            .context("pipeline metadata closed")?
            .try_read(keys)
    }

    pub(crate) fn try_recover_consensus_candidate(
        &self,
        candidate: NodeHash,
    ) -> Result<Option<IoTicket<Option<crate::native_pipeline::persistence::StoredCandidate>>>>
    {
        self.metadata
            .as_ref()
            .context("pipeline metadata closed")?
            .try_recover(candidate)
    }

    pub(crate) fn storage_domain(&self) -> crate::native_pipeline::persistence::StorageDomain {
        self.config.store.domain
    }

    /// One bounded optional proof lane on the existing storage owner. This
    /// grants neither signing access nor a second database/head. Drain/drop the
    /// proof owner before shutting down the pipeline.
    pub(crate) fn take_proof_io(
        &self,
    ) -> Result<crate::native_pipeline::persistence::io::IoProofClient> {
        self.io
            .as_ref()
            .context("pipeline I/O closed")?
            .proof_client()
    }

    pub(crate) fn owner_identity(&self) -> Arc<()> {
        self.identity.clone()
    }

    pub(crate) fn try_apply_consensus_metadata(
        &self,
        transition: MetaTransition,
    ) -> Result<Option<IoTicket<MetaOutcome>>> {
        self.metadata
            .as_ref()
            .context("pipeline metadata closed")?
            .try_apply(transition)
    }

    /// Administrative drain, not a network tick. A caller abandoning tickets
    /// does not abandon accepted work. Drop itself only disconnects/wakes.
    pub fn shutdown(mut self) -> Result<()> {
        self.sender.take();
        self.queries.take();
        self.metadata.take();
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
        self.metadata.take();
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
    }
}

#[cfg(test)]
mod tests;
