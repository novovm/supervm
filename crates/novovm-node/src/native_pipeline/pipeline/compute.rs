//! One resident computation owner, separate from storage and control. A bounded
//! command carries owned data only. Signatures/business callbacks run on AOEM;
//! capture decoding, compilation and packet encoding run here, not on control.
//! This module neither changes AOEM's domain-neutral ABI nor publishes a head.

use crate::native_pipeline::business::direct_nov_fee::DirectNovFeePolicy;
use crate::native_pipeline::business::nov_transfer_batch::{
    ExecutionObservation, NovCapturedInput, NovTransferBody, NovTransferPlan,
};
use crate::native_pipeline::execution::plan::{BatchContext, PlanBudget};
use crate::native_pipeline::ingress::batch::{
    authenticate_source_batch, AuthenticationBudget, BatchSource,
};
use crate::native_pipeline::persistence::{PacketBudget, PreparedCandidate, StorageDomain};
use anyhow::{ensure, Context, Result};
use novovm_exec::resident::ComputeSession;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread::{self, JoinHandle, Thread};
use std::time::Duration;

pub(crate) struct ComputeConfig {
    pub library: PathBuf,
    pub workers: u32,
    pub queue_capacity: usize,
    pub authentication: AuthenticationBudget,
    pub plan: PlanBudget,
    pub packet: PacketBudget,
    pub capture: crate::native_pipeline::state::frontier::CaptureBudget,
    pub timeout: Duration,
    pub domain: StorageDomain,
}

pub(crate) struct PrepareRequest {
    pub raw_transactions: BatchSource,
    pub context: BatchContext,
    pub policy: DirectNovFeePolicy,
}

pub(crate) struct AuthenticateRequest {
    pub raw_transactions: BatchSource,
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
    Authenticate {
        request: Box<AuthenticateRequest>,
        reply: mpsc::Sender<Result<NovTransferBody>>,
    },
    Bind {
        request: Box<BindRequest>,
        reply: mpsc::Sender<Result<NovTransferPlan>>,
    },
    Prepare {
        request: Box<PrepareRequest>,
        reply: mpsc::Sender<Result<NovTransferPlan>>,
    },
    Execute {
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
    sender: mpsc::SyncSender<Command>,
    worker: JoinHandle<()>,
}

impl ComputeOwner {
    pub(crate) fn try_authenticate(
        &self,
        request: AuthenticateRequest,
    ) -> Result<Submission<AuthenticateRequest, NovTransferBody>> {
        let (reply, receiver) = mpsc::channel();
        match self.sender.try_send(Command::Authenticate {
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
        let (sender, receiver) = mpsc::sync_channel(config.queue_capacity);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("novovm-compute-owner".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                // Unexpected unwinding/disconnection must wake control as well.
                let notification = Notification(notify);
                match ComputeSession::open(&config.library, config.workers) {
                    Ok(mut session) => {
                        if ready_tx.send(Ok(())).is_ok() {
                            run_owner(&mut session, &config, receiver, &notification);
                        }
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                    }
                }
            })
            .context("start resident compute owner")?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self { sender, worker }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error.context("initialize resident compute owner"))
            }
            Err(error) => {
                let _ = worker.join();
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
        match self.sender.try_send(Command::Execute {
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
        self.worker
            .join()
            .map_err(|_| anyhow::anyhow!("compute owner panicked"))
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
    config: &ComputeConfig,
    request: PrepareRequest,
) -> Result<NovTransferPlan> {
    domain_matches(config.domain, &request.context)?;
    let body = authenticate(
        session,
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
    config: &ComputeConfig,
    request: AuthenticateRequest,
) -> Result<NovTransferBody> {
    let batch = authenticate_source_batch(
        session,
        config.domain.chain_id,
        request.raw_transactions,
        config.authentication,
        config.timeout,
    )?;
    NovTransferBody::prepare(batch, request.policy, config.plan)
}

fn execute(
    session: &mut ComputeSession,
    config: &ComputeConfig,
    input: NovCapturedInput,
) -> Result<ComputedCandidate> {
    domain_matches(config.domain, input.plan().context())?;
    let executed = input.finalize_capture()?.execute(session, config.timeout)?;
    let observation = *executed.observation();
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
    receiver: mpsc::Receiver<Command>,
    notification: &Notification,
) {
    let mut panicked = false;
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Authenticate { request, reply } => {
                let result = run_checked(&mut panicked, || authenticate(session, config, *request));
                let _ = reply.send(result);
            }
            Command::Bind { request, reply } => {
                let result = run_checked(&mut panicked, || {
                    domain_matches(config.domain, &request.context)?;
                    request.body.bind(request.context)
                });
                let _ = reply.send(result);
            }
            Command::Prepare { request, reply } => {
                let result = run_checked(&mut panicked, || prepare(session, config, *request));
                let _ = reply.send(result);
            }
            Command::Execute { input, reply } => {
                let result = run_checked(&mut panicked, || execute(session, config, *input));
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
