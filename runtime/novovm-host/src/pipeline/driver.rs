use super::compute::{
    ComputeConfig, ComputeOwner, ComputeTicket, ComputedCandidate, PrepareRequest,
};
use super::*;
use crate::business::nov_transfer_batch::{NovCapturedInput, NovTransferCapture, NovTransferPlan};
use crate::persistence::io::NodeReadReply;
use crate::state::frontier::CaptureStep;
use std::collections::VecDeque;

pub(super) fn start(
    config: PipelineConfig,
    mode: OpenMode,
    receiver: mpsc::Receiver<Command>,
    ready: mpsc::SyncSender<Result<Arc<IoService>>>,
    identity: Arc<()>,
) -> Result<()> {
    let startup = (|| {
        let notify = Some(thread::current());
        let io = Arc::new(IoService::start_with_notify(
            config.store.clone(),
            mode,
            config.io,
            notify.clone(),
        )?);
        let compute = ComputeOwner::start(
            ComputeConfig {
                library: config.store.library.clone(),
                workers: config.workers,
                queue_capacity: config.max_batches,
                authentication: config.authentication,
                plan: config.plan,
                packet: config.store.packet_budget,
                timeout: config.compute_timeout,
                domain: config.store.domain,
            },
            notify,
        )?;
        Ok::<_, anyhow::Error>((io, compute))
    })();
    let (io, compute) = match startup {
        Ok(owners) => owners,
        Err(error) => {
            let _ = ready.send(Err(error));
            return Ok(());
        }
    };
    if ready.send(Ok(io.clone())).is_err() {
        return compute.shutdown();
    }
    run(&config, &io, &compute, receiver, &identity);
    // I/O shutdown belongs to CandidatePipeline after this coordinator releases
    // its Arc. All admitted jobs (including lost replies) have drained here.
    compute.shutdown()
}

enum Stage {
    Prepare(Box<PrepareRequest>),
    Preparing(ComputeTicket<NovTransferPlan>),
    Capture(Box<NovTransferCapture>),
    Reading {
        capture: Box<NovTransferCapture>,
        ticket: IoTicket<NodeReadReply>,
    },
    Execute(Box<NovCapturedInput>),
    Executing(ComputeTicket<ComputedCandidate>),
    Persist {
        packet: Arc<PreparedCandidate>,
        observation: ExecutionObservation,
    },
    Persisting {
        packet: Arc<PreparedCandidate>,
        observation: ExecutionObservation,
        ticket: IoTicket<PersistedCandidate>,
    },
}

impl Stage {
    fn label(&self) -> FailureStage {
        match self {
            Self::Prepare(_) | Self::Preparing(_) => FailureStage::Prepare,
            Self::Capture(_) | Self::Reading { .. } => FailureStage::Capture,
            Self::Execute(_) | Self::Executing(_) => FailureStage::Execute,
            Self::Persist { .. } | Self::Persisting { .. } => FailureStage::Persist,
        }
    }
}

struct Job {
    stage: Stage,
    candidate_id: Option<NodeHash>,
    reply: mpsc::Sender<Result<DurableBatch>>,
    _permit: Arc<Permit>,
}

impl From<Command> for Job {
    fn from(command: Command) -> Self {
        Self {
            stage: Stage::Prepare(command.request.request),
            candidate_id: None,
            reply: command.reply,
            _permit: command.permit,
        }
    }
}

enum Advancement {
    Pending { stage: Stage, progressed: bool },
    Complete(DurableBatch),
}

fn pending(stage: Stage, progressed: bool) -> Advancement {
    Advancement::Pending { stage, progressed }
}

fn advance(
    stage: Stage,
    id: &mut Option<NodeHash>,
    config: &PipelineConfig,
    io: &IoService,
    compute: &ComputeOwner,
    identity: &Arc<()>,
) -> Result<Advancement> {
    use super::compute::Submission as ComputeSubmission;
    match stage {
        Stage::Prepare(request) => Ok(match compute.try_prepare(*request)? {
            ComputeSubmission::Accepted(ticket) => pending(Stage::Preparing(ticket), true),
            ComputeSubmission::Backpressured(request) => {
                pending(Stage::Prepare(Box::new(request)), false)
            }
        }),
        Stage::Preparing(mut ticket) => {
            if let Some(plan) = ticket.try_take()? {
                *id = Some(plan.commitment());
                Ok(pending(
                    Stage::Capture(Box::new(plan.begin_capture(config.capture)?)),
                    true,
                ))
            } else {
                Ok(pending(Stage::Preparing(ticket), false))
            }
        }
        Stage::Capture(mut capture) => match capture.advance(config.capture_edge_quantum)? {
            CaptureStep::More => Ok(pending(Stage::Capture(capture), true)),
            CaptureStep::Complete => Ok(pending(Stage::Execute(Box::new(capture.finish()?)), true)),
            CaptureStep::NeedRead => {
                let hashes = capture
                    .next_request()?
                    .context("capture needs a read without requested hashes")?;
                // The response ticket never leaves this exact capture state.
                // There is only one outstanding frontier request per job.
                if let Some(ticket) = io.try_read_nodes(hashes)? {
                    Ok(pending(Stage::Reading { capture, ticket }, true))
                } else {
                    Ok(pending(Stage::Capture(capture), false))
                }
            }
        },
        Stage::Reading {
            mut capture,
            mut ticket,
        } => {
            if let Some(values) = ticket.try_take()? {
                capture.accept(values)?;
                Ok(pending(Stage::Capture(capture), true))
            } else {
                Ok(pending(Stage::Reading { capture, ticket }, false))
            }
        }
        Stage::Execute(input) => Ok(match compute.try_execute(*input)? {
            ComputeSubmission::Accepted(ticket) => pending(Stage::Executing(ticket), true),
            ComputeSubmission::Backpressured(input) => {
                pending(Stage::Execute(Box::new(input)), false)
            }
        }),
        Stage::Executing(mut ticket) => {
            if let Some(computed) = ticket.try_take()? {
                ensure!(
                    *id == Some(computed.packet.candidate_id()),
                    "computed packet belongs to another pipeline plan"
                );
                Ok(pending(
                    Stage::Persist {
                        packet: Arc::new(computed.packet),
                        observation: computed.observation,
                    },
                    true,
                ))
            } else {
                Ok(pending(Stage::Executing(ticket), false))
            }
        }
        Stage::Persist {
            packet,
            observation,
        } => {
            if let Some(ticket) = io.try_persist(packet.clone())? {
                Ok(pending(
                    Stage::Persisting {
                        packet,
                        observation,
                        ticket,
                    },
                    true,
                ))
            } else {
                Ok(pending(
                    Stage::Persist {
                        packet,
                        observation,
                    },
                    false,
                ))
            }
        }
        Stage::Persisting {
            packet,
            observation,
            mut ticket,
        } => {
            if let Some(persisted) = ticket.try_take()? {
                ensure!(
                    persisted.candidate_id == packet.candidate_id()
                        && persisted.state_root == packet.state_root()
                        && persisted.statement_commitment == packet.statement_commitment()
                        && persisted.document_digest == packet.document_digest(),
                    "stored reply differs from executed packet"
                );
                Ok(Advancement::Complete(DurableBatch {
                    candidate: DurableCandidate {
                        owner: identity.clone(),
                        packet: packet.clone(),
                    },
                    packet,
                    persisted,
                    observation,
                }))
            } else {
                Ok(pending(
                    Stage::Persisting {
                        packet,
                        observation,
                        ticket,
                    },
                    false,
                ))
            }
        }
    }
}

pub(super) fn run(
    config: &PipelineConfig,
    io: &IoService,
    compute: &ComputeOwner,
    receiver: mpsc::Receiver<Command>,
    identity: &Arc<()>,
) {
    let mut jobs = VecDeque::<Job>::new();
    let mut connected = true;
    while connected || !jobs.is_empty() {
        if jobs.is_empty() && connected {
            match receiver.recv() {
                Ok(command) => jobs.push_back(command.into()),
                Err(_) => {
                    connected = false;
                    continue;
                }
            }
        }
        while connected && jobs.len() < config.max_batches {
            match receiver.try_recv() {
                Ok(command) => jobs.push_back(command.into()),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => connected = false,
            }
        }
        let mut progressed = false;
        // One bounded transition per job per round. Large frontiers do not
        // restart from the root or monopolize already-computed persistence.
        for _ in 0..jobs.len() {
            let Job {
                stage,
                mut candidate_id,
                reply,
                _permit,
            } = jobs.pop_front().expect("fixed round length");
            let label = stage.label();
            match advance(stage, &mut candidate_id, config, io, compute, identity) {
                Ok(Advancement::Pending {
                    stage,
                    progressed: made_progress,
                }) => {
                    progressed |= made_progress;
                    jobs.push_back(Job {
                        stage,
                        candidate_id,
                        reply,
                        _permit,
                    });
                }
                Ok(Advancement::Complete(output)) => {
                    let _ = reply.send(Ok(output));
                    progressed = true;
                }
                Err(error) => {
                    let error = PipelineFailure {
                        stage: label,
                        candidate_id,
                        message: format!("{error:#}"),
                    };
                    let _ = reply.send(Err(error.into()));
                    progressed = true;
                }
            }
        }
        if !progressed && !jobs.is_empty() {
            // Normal completions and submissions unpark immediately. This is
            // only a bounded watchdog for a disconnected/panicked owner that
            // cannot notify, NOT a per-batch delay or a throughput timer.
            thread::park_timeout(Duration::from_millis(100));
        }
    }
}
