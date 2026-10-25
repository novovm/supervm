//! One resident computation owner, separate from storage and control. A bounded
//! command carries owned data only. Signatures/business callbacks run on AOEM;
//! capture decoding, compilation and packet encoding run here, not on control.
//! This module neither changes AOEM's domain-neutral ABI nor publishes a head.

use crate::native_pipeline::business::direct_nov_fee::DirectNovFeePolicy;
use crate::native_pipeline::business::nov_transfer_batch::{
    ExecutionObservation, NovCapturedInput, NovTransferBody, NovTransferPlan,
    ResidentTransferResources,
};
use crate::native_pipeline::business::quoted_transfer::Account;
use crate::native_pipeline::execution::plan::{BatchContext, PlanBudget};
use crate::native_pipeline::ingress::batch::{
    admission_retained_bytes, admission_sizes, authenticate_admission_rows,
    authenticate_source_batch_observed, AdmissionInput, AdmissionRow, AdmissionRows,
    AuthenticationBudget, BatchSource, CheckedAdmission,
};
use crate::native_pipeline::persistence::{PacketBudget, PreparedCandidate, StorageDomain};
use crate::native_pipeline::profile;
use anyhow::{ensure, Context, Result};
use novovm_exec::resident::ComputeSession;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle, Thread};
use std::time::{Duration, Instant};

pub(crate) struct ComputeConfig {
    pub library: PathBuf,
    pub workers: u32,
    pub execution_backend: super::ExecutionBackend,
    pub queue_capacity: usize,
    pub authentication: AuthenticationBudget,
    pub plan: PlanBudget,
    pub packet: PacketBudget,
    pub capture: crate::native_pipeline::state::frontier::CaptureBudget,
    pub timeout: Duration,
    pub domain: StorageDomain,
    pub signatures: Arc<super::SignatureWork>,
}

/// All forms use the existing compute command and bounded preparation,
/// capture and execution. UCA is an explicitly typed input contract, not a
/// V3 reinterpretation. The admitted form is not externally constructible.
pub(crate) enum CandidateInput {
    Unverified(BatchSource),
    Admitted(super::AdmittedBatch),
    Uca {
        source: BatchSource,
        hints: Vec<(Account, Account)>,
    },
}

impl CandidateInput {
    pub(super) fn source(&self) -> &BatchSource {
        match self {
            Self::Unverified(source) => source,
            Self::Admitted(batch) => batch.source(),
            Self::Uca { source, .. } => source,
        }
    }
    pub(super) fn ensure_owner(&self, owner: &Arc<()>) -> Result<()> {
        match self {
            Self::Unverified(_) | Self::Uca { .. } => Ok(()),
            Self::Admitted(batch) => batch.ensure_owner(owner),
        }
    }
    #[cfg(test)]
    pub(crate) fn into_raw(self) -> Result<Vec<Vec<u8>>> {
        match self {
            Self::Unverified(source) => source.into_raw(),
            Self::Admitted(_) => anyhow::bail!("raw-only allocation fixture"),
            Self::Uca { .. } => anyhow::bail!("raw-only allocation fixture"),
        }
    }
}

impl From<Vec<Vec<u8>>> for CandidateInput {
    fn from(raw: Vec<Vec<u8>>) -> Self {
        Self::Unverified(BatchSource::Raw(raw))
    }
}

// Preserve allocation/backpressure fixture access, not a mutable product API.
#[cfg(test)]
impl std::ops::Deref for CandidateInput {
    type Target = BatchSource;
    fn deref(&self) -> &Self::Target {
        self.source()
    }
}
#[cfg(test)]
impl std::ops::DerefMut for CandidateInput {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Unverified(source) => source,
            Self::Admitted(_) => panic!("authenticated input is immutable"),
            Self::Uca { .. } => panic!("typed UCA input is immutable"),
        }
    }
}
#[cfg(test)]
impl PartialEq<Vec<Vec<u8>>> for CandidateInput {
    fn eq(&self, other: &Vec<Vec<u8>>) -> bool {
        self.source() == other
    }
}
#[cfg(test)]
impl std::fmt::Debug for CandidateInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CandidateInput")
            .field(self.source())
            .finish()
    }
}

pub(crate) struct PrepareRequest {
    pub raw_transactions: CandidateInput,
    pub context: BatchContext,
    pub policy: DirectNovFeePolicy,
}

pub(crate) struct AuthenticateRequest {
    pub raw_transactions: CandidateInput,
    pub policy: DirectNovFeePolicy,
}

pub(crate) struct BindRequest {
    pub body: NovTransferBody,
    pub context: BatchContext,
}

pub(crate) struct ComputedCandidate {
    pub packet: PreparedCandidate,
    pub observation: ExecutionObservation,
    pub seed: Option<crate::native_pipeline::state::frontier::PostStateSeed>,
}

/// Full means not admitted: return the exact owned input for a later attempt.
/// Dropping Accepted's ticket abandons a reply, never cancels an accepted graph.
pub(crate) enum Submission<R, T> {
    Accepted(ComputeTicket<T>),
    Backpressured(R),
}

pub(crate) struct ComputeTicket<T> {
    receiver: mpsc::Receiver<Result<T>>,
    consumed: bool,
}

impl<T> ComputeTicket<T> {
    pub(crate) fn try_take(&mut self) -> Result<Option<T>> {
        ensure!(!self.consumed, "compute ticket already consumed");
        match self.receiver.try_recv() {
            Ok(result) => {
                self.consumed = true;
                result.map(Some)
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.consumed = true;
                anyhow::bail!("compute owner disconnected; no execution result available")
            }
        }
    }

    #[cfg(test)]
    fn wait(self) -> Result<T> {
        self.receiver
            .recv()
            .context("compute owner disconnected; no execution result available")?
    }
}

enum Command {
    Admission {
        queued: Option<Instant>,
        inputs: Vec<AdmissionInput>,
        reply: mpsc::Sender<Result<AdmissionRows>>,
    },
    Authenticate {
        queued: Option<Instant>,
        request: Box<AuthenticateRequest>,
        reply: mpsc::Sender<Result<NovTransferBody>>,
    },
    Bind {
        queued: Option<Instant>,
        request: Box<BindRequest>,
        reply: mpsc::Sender<Result<NovTransferPlan>>,
    },
    Prepare {
        queued: Option<Instant>,
        request: Box<PrepareRequest>,
        reply: mpsc::Sender<Result<NovTransferPlan>>,
    },
    Execute {
        queued: Option<Instant>,
        input: Box<NovCapturedInput>,
        reply: mpsc::Sender<Result<ComputedCandidate>>,
    },
    #[cfg(test)]
    Hold {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    },
}

pub(crate) struct ComputeOwner {
    sender: lanes::Sender,
    worker: JoinHandle<()>,
    integer: Option<lanes::IntegerLane>,
}

mod lanes;

impl ComputeOwner {
    pub(crate) fn try_admit_signatures(
        &self,
        inputs: Vec<AdmissionInput>,
    ) -> Result<Submission<Vec<AdmissionInput>, AdmissionRows>> {
        let (reply, receiver) = mpsc::channel();
        match self.sender.try_send(Command::Admission {
            queued: profile::mark(),
            inputs,
            reply,
        }) {
            Ok(()) => Ok(Submission::Accepted(ComputeTicket {
                receiver,
                consumed: false,
            })),
            Err(mpsc::TrySendError::Full(Command::Admission { inputs, .. })) => {
                Ok(Submission::Backpressured(inputs))
            }
            Err(mpsc::TrySendError::Disconnected(_)) => anyhow::bail!("compute owner unavailable"),
            Err(mpsc::TrySendError::Full(_)) => unreachable!("typed command changed while sending"),
        }
    }

    pub(crate) fn try_authenticate(
        &self,
        request: AuthenticateRequest,
    ) -> Result<Submission<AuthenticateRequest, NovTransferBody>> {
        let (reply, receiver) = mpsc::channel();
        match self.sender.try_send(Command::Authenticate {
            queued: profile::mark(),
            request: Box::new(request),
            reply,
        }) {
            Ok(()) => Ok(Submission::Accepted(ComputeTicket {
                receiver,
                consumed: false,
            })),
            Err(mpsc::TrySendError::Full(Command::Authenticate { request, .. })) => {
                Ok(Submission::Backpressured(*request))
            }
            Err(mpsc::TrySendError::Disconnected(_)) => anyhow::bail!("compute owner unavailable"),
            Err(mpsc::TrySendError::Full(_)) => unreachable!("typed command changed while sending"),
        }
    }

    pub(crate) fn try_bind(
        &self,
        request: BindRequest,
    ) -> Result<Submission<BindRequest, NovTransferPlan>> {
        let (reply, receiver) = mpsc::channel();
        match self.sender.try_send(Command::Bind {
            queued: profile::mark(),
            request: Box::new(request),
            reply,
        }) {
            Ok(()) => Ok(Submission::Accepted(ComputeTicket {
                receiver,
                consumed: false,
            })),
            Err(mpsc::TrySendError::Full(Command::Bind { request, .. })) => {
                Ok(Submission::Backpressured(*request))
            }
            Err(mpsc::TrySendError::Disconnected(_)) => anyhow::bail!("compute owner unavailable"),
            Err(mpsc::TrySendError::Full(_)) => unreachable!("typed command changed while sending"),
        }
    }

    /// Startup only. The native session is constructed AND destroyed on this
    /// thread, never transferred or reopened between batches. The pipeline owns
    /// end-to-end logical byte/request permits, including unconsumed replies.
    pub(crate) fn start(config: ComputeConfig, notify: Option<Thread>) -> Result<Self> {
        ensure!(
            (1..=65_536).contains(&config.queue_capacity)
                && !config.timeout.is_zero()
                && config.domain.chain_id != 0
                && config.domain.genesis_config_commitment != [0; 32]
                && config.domain.protocol_commitment != [0; 32],
            "invalid compute owner configuration"
        );
        let config = Arc::new(config);
        let failed = Arc::new(AtomicBool::new(false));
        let (sender, receiver, integer) = match config.execution_backend {
            super::ExecutionBackend::CpuCallbacks => {
                let (sender, receiver) =
                    lanes::single_channel(config.queue_capacity, Arc::clone(&failed));
                (sender, receiver, None)
            }
            super::ExecutionBackend::AoemIntegerV7 => {
                let budget = lanes::Budget::new(config.queue_capacity);
                let (sender, receiver) = lanes::channel(Arc::clone(&budget), Arc::clone(&failed));
                let integer = lanes::IntegerLane::start(
                    Arc::clone(&config),
                    budget,
                    Arc::clone(&failed),
                    notify.clone(),
                )?;
                (sender, receiver, Some(integer))
            }
        };
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("novovm-compute-owner".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                // Unexpected unwinding/disconnection must wake control as well.
                let notification = Notification(notify);
                let _fault = lanes::UnwindLatch(Arc::clone(&failed));
                match ComputeSession::open(&config.library, config.workers) {
                    Ok(mut session) => {
                        if ready_tx.send(Ok(())).is_ok() {
                            run_owner(&mut session, &config, receiver, &notification, &failed);
                        }
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                    }
                }
            })
            .context("start resident compute owner");
        let worker = match worker {
            Ok(worker) => worker,
            Err(error) => {
                if let Some(integer) = integer {
                    let _ = integer.shutdown();
                }
                return Err(error);
            }
        };
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                sender,
                worker,
                integer,
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                if let Some(integer) = integer {
                    let _ = integer.shutdown();
                }
                Err(error.context("initialize resident compute owner"))
            }
            Err(error) => {
                let _ = worker.join();
                if let Some(integer) = integer {
                    let _ = integer.shutdown();
                }
                Err(error).context("compute owner failed during initialization")
            }
        }
    }

    pub(crate) fn try_prepare(
        &self,
        request: PrepareRequest,
    ) -> Result<Submission<PrepareRequest, NovTransferPlan>> {
        let (reply, receiver) = mpsc::channel();
        match self.sender.try_send(Command::Prepare {
            queued: profile::mark(),
            request: Box::new(request),
            reply,
        }) {
            Ok(()) => Ok(Submission::Accepted(ComputeTicket {
                receiver,
                consumed: false,
            })),
            Err(mpsc::TrySendError::Full(Command::Prepare { request, .. })) => {
                Ok(Submission::Backpressured(*request))
            }
            Err(mpsc::TrySendError::Disconnected(_)) => anyhow::bail!("compute owner unavailable"),
            Err(mpsc::TrySendError::Full(_)) => unreachable!("typed command changed while sending"),
        }
    }

    pub(crate) fn try_execute(
        &self,
        input: NovCapturedInput,
    ) -> Result<Submission<NovCapturedInput, ComputedCandidate>> {
        let (reply, receiver) = mpsc::channel();
        let sender = self
            .integer
            .as_ref()
            .map_or(&self.sender, |lane| &lane.sender);
        match sender.try_send(Command::Execute {
            queued: profile::mark(),
            input: Box::new(input),
            reply,
        }) {
            Ok(()) => Ok(Submission::Accepted(ComputeTicket {
                receiver,
                consumed: false,
            })),
            Err(mpsc::TrySendError::Full(Command::Execute { input, .. })) => {
                Ok(Submission::Backpressured(*input))
            }
            Err(mpsc::TrySendError::Disconnected(_)) => anyhow::bail!("compute owner unavailable"),
            Err(mpsc::TrySendError::Full(_)) => unreachable!("typed command changed while sending"),
        }
    }

    /// Deterministic control-isolation fixture only. Holds the actual owner
    /// between jobs, never an AOEM callback; dropping release also unblocks it.
    #[cfg(test)]
    pub(crate) fn hold_for_test(&self) -> Result<(mpsc::Receiver<()>, mpsc::Sender<()>)> {
        let (entered, observed) = mpsc::channel();
        let (release, held) = mpsc::channel();
        match self.sender.try_send(Command::Hold {
            entered,
            release: held,
        }) {
            Ok(()) => Ok((observed, release)),
            Err(mpsc::TrySendError::Full(_)) => anyhow::bail!("compute test hold queue full"),
            Err(mpsc::TrySendError::Disconnected(_)) => anyhow::bail!("compute owner unavailable"),
        }
    }

    /// Explicit administrative drain. Ordinary Drop disconnects the sender and
    /// detaches the thread; it does not block control or cancel accepted work.
    pub(crate) fn shutdown(self) -> Result<()> {
        drop(self.sender);
        let cpu = self.worker.join();
        let integer = self.integer.map(lanes::IntegerLane::shutdown).transpose();
        cpu.map_err(|_| anyhow::anyhow!("compute owner panicked"))?;
        integer?;
        Ok(())
    }
}

struct Notification(Option<Thread>);
impl Notification {
    fn wake(&self) {
        if let Some(thread) = &self.0 {
            thread.unpark();
        }
    }
}
impl Drop for Notification {
    fn drop(&mut self) {
        self.wake();
    }
}

fn domain_matches(domain: StorageDomain, context: &BatchContext) -> Result<()> {
    ensure!(
        domain.chain_id == context.chain_id
            && domain.genesis_config_commitment == context.genesis_config_commitment
            && domain.protocol_commitment == context.protocol_commitment,
        "compute request differs from configured chain domain"
    );
    Ok(())
}

fn prepare(
    session: &mut ComputeSession,
    uca_facade: &mut Option<novovm_exec::uca_spend::UcaSpendVerifierV1>,
    config: &ComputeConfig,
    request: PrepareRequest,
) -> Result<NovTransferPlan> {
    domain_matches(config.domain, &request.context)?;
    let body = authenticate(
        session,
        uca_facade,
        config,
        AuthenticateRequest {
            raw_transactions: request.raw_transactions,
            policy: request.policy,
        },
    )?;
    body.bind(request.context)
}

fn authenticate(
    session: &mut ComputeSession,
    uca_facade: &mut Option<novovm_exec::uca_spend::UcaSpendVerifierV1>,
    config: &ComputeConfig,
    request: AuthenticateRequest,
) -> Result<NovTransferBody> {
    let _span = crate::native_pipeline::profile::span(
        crate::native_pipeline::profile::Phase::Authentication,
    );
    let batch = match request.raw_transactions {
        CandidateInput::Uca { source, hints } => {
            let (bytes, max) = source.sizes()?;
            ensure!(
                !source.is_empty()
                    && source.len() == hints.len()
                    && source.len() <= config.authentication.transactions
                    && max <= config.authentication.transaction_bytes
                    && bytes <= config.authentication.body_bytes,
                "UCA authentication input budget"
            );
            ensure_uca_facade(uca_facade, config)?;
            // The resident owner's qualified verifier retains the same loaded
            // AOEM. Qualification is not repeated per row or per candidate.
            let verifier = uca_facade.as_ref().context("UCA verifier unavailable")?;
            ensure!(
                !matches!(source, BatchSource::Apfl(_)),
                "UCA input cannot use V3 APFL projection"
            );
            let mut signed = Vec::with_capacity(source.len());
            for index in 0..source.len() {
                let projected;
                let wire = match &source {
                    BatchSource::Raw(raw) => &raw[index],
                    BatchSource::Uca(batch) => {
                        projected = batch.raw_transaction(index)?;
                        &projected
                    }
                    BatchSource::Apfl(_) => unreachable!(),
                };
                let tx = novovm_protocol::uca_spend::SignedUcaSpendV1::decode(wire)?;
                ensure!(
                    tx.intent.chain_id == config.domain.chain_id
                        && tx.intent.genesis == config.domain.genesis_config_commitment,
                    "UCA authentication domain mismatch"
                );
                signed.push(tx);
            }
            let count = signed.len();
            config
                .signatures
                .uca_batch_calls
                .fetch_add(1, Ordering::Relaxed);
            let signed = verifier.verify_batch(signed)?;
            ensure!(signed.len() == count, "UCA batch verification result count");
            config.signatures.strict_verified.fetch_add(
                signed.iter().filter(|row| row.is_ok()).count() as u64,
                Ordering::Relaxed,
            );
            let signed = signed.into_iter().collect::<Result<Vec<_>>>()?;
            return NovTransferBody::prepare_uca(signed, hints, request.policy, config.plan);
        }
        CandidateInput::Unverified(source) => authenticate_source_batch_observed(
            session,
            config.domain.chain_id,
            source,
            config.authentication,
            config.timeout,
            Some(config.signatures.strict_verified.clone()),
        )?,
        CandidateInput::Admitted(batch) => {
            match batch.into_checked(
                config.domain.chain_id,
                config.domain.genesis_config_commitment,
                config.authentication,
            )? {
                super::admission::AdmittedSignatures::V3(checked) => {
                    config
                        .signatures
                        .reused
                        .fetch_add(checked.transactions().len() as u64, Ordering::Relaxed);
                    checked
                }
                super::admission::AdmittedSignatures::Uca {
                    source: BatchSource::Uca(source),
                    transactions,
                } => {
                    let hints = (0..source.len())
                        .map(|index| {
                            let (payer, recipient) = source.account_hints(index)?;
                            Ok((
                                payer.try_into().map_err(anyhow::Error::msg)?,
                                recipient.try_into().map_err(anyhow::Error::msg)?,
                            ))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    config
                        .signatures
                        .reused
                        .fetch_add(transactions.len() as u64, Ordering::Relaxed);
                    // Only signatures are reused. This original body compiler
                    // still captures the new exact parent and checks its UCA
                    // revocation, nonce, budget, balance and fee effects.
                    return NovTransferBody::prepare_uca(
                        transactions,
                        hints,
                        request.policy,
                        config.plan,
                    );
                }
                super::admission::AdmittedSignatures::Uca { .. } => {
                    anyhow::bail!("admitted UCA source contract mismatch")
                }
            }
        }
    };
    NovTransferBody::prepare(batch, request.policy, config.plan)
}

fn ensure_uca_facade(
    facade: &mut Option<novovm_exec::uca_spend::UcaSpendVerifierV1>,
    config: &ComputeConfig,
) -> Result<()> {
    if facade.is_none() {
        let selection = novovm_exec::resident::AoemLibrarySelection::new(
            &config.library,
            novovm_exec::AoemExecOpenOptions {
                ingress_workers: Some(config.workers),
            },
        )?;
        // Owned verification-only qualification retains this exact library;
        // neither a second loader nor signing/key-generation authority is used.
        *facade = Some(selection.open_wire_facade()?.uca_spend_verifier_v1()?);
    }
    Ok(())
}

/// Reuses the candidate verifier on this resident owner. A bad external row is
/// a row rejection, never an execution/session failure or an asset permission.
fn admit_uca_rows(
    facade: &mut Option<novovm_exec::uca_spend::UcaSpendVerifierV1>,
    config: &ComputeConfig,
    inputs: Vec<AdmissionInput>,
) -> Result<AdmissionRows> {
    let (bytes, max) = admission_sizes(&inputs)?;
    ensure!(
        !inputs.is_empty()
            && inputs.len() <= config.authentication.transactions
            && max <= config.authentication.transaction_bytes
            && bytes <= config.authentication.body_bytes
            && admission_retained_bytes(&inputs)? <= config.authentication.body_bytes,
        "UCA signature admission exceeds input budget"
    );
    ensure!(
        inputs
            .iter()
            .all(|input| matches!(input, AdmissionInput::Uca { .. })),
        "mixed UCA and V3 signature admission is forbidden"
    );
    ensure_uca_facade(facade, config)?;
    let verifier = facade.as_ref().context("UCA verifier unavailable")?;
    // Decode/domain failures stay at their original row. Only eligible rows go
    // through one bounded AOEM batch; no bad-row scalar retry or fallback.
    let mut errors = Vec::with_capacity(inputs.len());
    let mut signed = Vec::with_capacity(inputs.len());
    for input in &inputs {
        let decoded: Result<_> = (|| {
            let AdmissionInput::Uca { batch, index } = input else {
                unreachable!("checked contract");
            };
            let signed = novovm_protocol::uca_spend::SignedUcaSpendV1::decode(
                &batch.raw_transaction(*index)?,
            )?;
            ensure!(
                signed.intent.chain_id == config.domain.chain_id
                    && signed.intent.genesis == config.domain.genesis_config_commitment,
                "UCA authentication domain mismatch"
            );
            Ok(signed)
        })();
        match decoded {
            Ok(value) => {
                signed.push(value);
                errors.push(None);
            }
            Err(error) => errors.push(Some(error)),
        }
    }
    let count = signed.len();
    let verified = if count == 0 {
        Vec::new()
    } else {
        config
            .signatures
            .uca_batch_calls
            .fetch_add(1, Ordering::Relaxed);
        verifier.verify_batch(signed)?
    };
    ensure!(
        verified.len() == count,
        "UCA batch verification result count"
    );
    let mut verified = verified.into_iter();
    let mut rows = std::collections::VecDeque::with_capacity(inputs.len());
    for (input, error) in inputs.into_iter().zip(errors) {
        let result = if let Some(error) = error {
            Err(error)
        } else {
            verified
                .next()
                .context("UCA verification row missing")?
                .map(|checked| {
                    let AdmissionInput::Uca { batch, index } = &input else {
                        unreachable!("checked contract");
                    };
                    config
                        .signatures
                        .strict_verified
                        .fetch_add(1, Ordering::Relaxed);
                    CheckedAdmission::Uca {
                        checked,
                        batch: batch.clone(),
                        index: *index,
                    }
                })
        };
        rows.push_back(AdmissionRow { input, result });
    }
    ensure!(verified.next().is_none(), "extra UCA verification row");
    Ok(AdmissionRows {
        rows,
        peak_callbacks: 0,
    })
}

fn execute(
    session: &mut ComputeSession,
    config: &ComputeConfig,
    input: NovCapturedInput,
) -> Result<ComputedCandidate> {
    domain_matches(config.domain, input.plan().context())?;
    let input = {
        let _span = crate::native_pipeline::profile::span(
            crate::native_pipeline::profile::Phase::FinalizeCapture,
        );
        input.finalize_capture()?
    };
    let executed = input.execute(session, config.timeout)?;
    packet(config, executed)
}

fn packet(
    config: &ComputeConfig,
    executed: crate::native_pipeline::business::nov_transfer_batch::ExecutedNovBatch,
) -> Result<ComputedCandidate> {
    let observation = *executed.observation();
    let _span =
        crate::native_pipeline::profile::span(crate::native_pipeline::profile::Phase::Packet);
    let (packet, seed) =
        PreparedCandidate::from_executed_with_seed(executed, config.packet, config.capture)?;
    Ok(ComputedCandidate {
        packet,
        observation,
        seed,
    })
}

fn run_owner(
    session: &mut ComputeSession,
    config: &ComputeConfig,
    receiver: mpsc::Receiver<lanes::Queued>,
    notification: &Notification,
    failed: &AtomicBool,
) {
    // The qualified original unified verifier lives on this compute owner; it
    // owns no database or permissions. Reference-only runs open none.
    let mut uca_facade = None;
    while let Ok(queued) = receiver.recv() {
        let command = queued.into_command();
        match command {
            Command::Admission {
                queued,
                inputs,
                reply,
            } => {
                profile::elapsed(profile::Phase::QueueAdmission, queued);
                let _span = crate::native_pipeline::profile::span(
                    crate::native_pipeline::profile::Phase::Admission,
                );
                let result = lanes::checked(failed, || {
                    if inputs
                        .iter()
                        .any(|input| matches!(input, AdmissionInput::Uca { .. }))
                    {
                        return admit_uca_rows(&mut uca_facade, config, inputs);
                    }
                    authenticate_admission_rows(
                        session,
                        config.domain.chain_id,
                        inputs,
                        config.authentication,
                        config.timeout,
                        Some(config.signatures.strict_verified.clone()),
                    )
                });
                let _ = reply.send(result);
            }
            Command::Authenticate {
                queued,
                request,
                reply,
            } => {
                profile::elapsed(profile::Phase::QueueCandidate, queued);
                let result = lanes::checked(failed, || {
                    authenticate(session, &mut uca_facade, config, *request)
                });
                let _ = reply.send(result);
            }
            Command::Bind {
                queued,
                request,
                reply,
            } => {
                profile::elapsed(profile::Phase::QueueCandidate, queued);
                let _span = crate::native_pipeline::profile::span(
                    crate::native_pipeline::profile::Phase::Bind,
                );
                let result = lanes::checked(failed, || {
                    domain_matches(config.domain, &request.context)?;
                    request.body.bind(request.context)
                });
                let _ = reply.send(result);
            }
            Command::Prepare {
                queued,
                request,
                reply,
            } => {
                profile::elapsed(profile::Phase::QueueCandidate, queued);
                let result = lanes::checked(failed, || {
                    prepare(session, &mut uca_facade, config, *request)
                });
                let _ = reply.send(result);
            }
            Command::Execute {
                queued,
                input,
                reply,
            } => {
                profile::elapsed(profile::Phase::QueueCandidate, queued);
                let result = lanes::checked(failed, || execute(session, config, *input));
                let _ = reply.send(result);
            }
            #[cfg(test)]
            Command::Hold { entered, release } => {
                if entered.send(()).is_ok() {
                    let _ = release.recv();
                }
            }
        }
        notification.wake();
    }
}

fn run_checked<T>(panicked: &mut bool, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    ensure!(
        !*panicked,
        "compute owner previously panicked; explicit recovery required"
    );
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(result) => result,
        Err(payload) => {
            // Dropping an arbitrary panic payload can itself panic. Preserve
            // this one exceptional payload and permanently reject later work.
            std::mem::forget(payload);
            *panicked = true;
            anyhow::bail!("compute owner panicked; no replacement session will be opened")
        }
    }
}

#[cfg(test)]
pub(super) mod tests;
