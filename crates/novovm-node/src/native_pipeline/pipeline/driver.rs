use super::compute::{
    AuthenticateRequest, BindRequest, ComputeConfig, ComputeOwner, ComputeTicket,
    ComputedCandidate, PrepareRequest,
};
use super::seed::SeedCache;
use super::*;
use crate::native_pipeline::business::nov_transfer_batch::{
    NovCapturedInput, NovTransferBody, NovTransferCapture, NovTransferPlan,
};
use crate::native_pipeline::persistence::io::NodeReadReply;
use crate::native_pipeline::state::frontier::CaptureStep;
use crate::native_pipeline::state::frontier::PostStateSeed;
use std::collections::VecDeque;

pub(super) fn start(
    config: PipelineConfig,
    mode: OpenMode,
    receiver: mpsc::Receiver<DriverMessage>,
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
                capture: config.capture,
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
    Authenticate(Box<AuthenticateRequest>),
    Authenticating(ComputeTicket<NovTransferBody>),
    Bind(Box<BindRequest>),
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
        seed: Option<PostStateSeed>,
    },
    Persisting {
        packet: Arc<PreparedCandidate>,
        observation: ExecutionObservation,
        ticket: IoTicket<PersistedCandidate>,
        seed: Option<PostStateSeed>,
    },
}

impl Stage {
    fn label(&self) -> FailureStage {
        match self {
            Self::Authenticate(_)
            | Self::Authenticating(_)
            | Self::Bind(_)
            | Self::Prepare(_)
            | Self::Preparing(_) => FailureStage::Prepare,
            Self::Capture(_) | Self::Reading { .. } => FailureStage::Capture,
            Self::Execute(_) | Self::Executing(_) => FailureStage::Execute,
            Self::Persist { .. } | Self::Persisting { .. } => FailureStage::Persist,
        }
    }
}

struct Job {
    stage: Stage,
    candidate_id: Option<NodeHash>,
    reply: Reply,
    _permit: Arc<Permit>,
    background: bool,
    capture: CaptureObservation,
}

enum Reply {
    Durable(mpsc::Sender<Result<DurableBatch>>),
    Authenticated(mpsc::Sender<Result<AuthenticatedBody>>),
}

impl Reply {
    fn fail(self, error: anyhow::Error) {
        match self {
            Self::Durable(reply) => {
                let _ = reply.send(Err(error));
            }
            Self::Authenticated(reply) => {
                let _ = reply.send(Err(error));
            }
        }
    }
}

impl From<DriverMessage> for Job {
    fn from(message: DriverMessage) -> Self {
        match message {
            DriverMessage::Batch(command) => Self {
                capture: CaptureObservation::default(),
                stage: Stage::Prepare(command.request.request),
                candidate_id: None,
                reply: Reply::Durable(command.reply),
                _permit: command.permit,
                background: command.background,
            },
            DriverMessage::Authenticate(command) => Self {
                capture: CaptureObservation::default(),
                stage: Stage::Authenticate(command.request.request),
                candidate_id: None,
                reply: Reply::Authenticated(command.reply),
                _permit: command.permit,
                background: true,
            },
            DriverMessage::Bind(command) => Self {
                capture: CaptureObservation::default(),
                stage: Stage::Bind(Box::new(BindRequest {
                    body: *command.request.body.body,
                    context: *command.request.context,
                })),
                candidate_id: None,
                reply: Reply::Durable(command.reply),
                _permit: command.request.body.permit,
                background: command.background,
            },
        }
    }
}

enum Advancement {
    Pending { stage: Stage, progressed: bool },
    Complete(DurableBatch),
    Authenticated(NovTransferBody),
}

fn pending(stage: Stage, progressed: bool) -> Advancement {
    Advancement::Pending { stage, progressed }
}

fn advance(
    stage: Stage,
    progress: (&mut Option<NodeHash>, &mut CaptureObservation),
    config: &PipelineConfig,
    io: &IoService,
    compute: &ComputeOwner,
    identity: &Arc<()>,
    locality: (&mut Option<SeedCache>, &Arc<Mutex<Usage>>),
) -> Result<Advancement> {
    let (id, capture_observation) = progress;
    let (cache, usage) = locality;
    use super::compute::Submission as ComputeSubmission;
    match stage {
        Stage::Authenticate(request) => Ok(match compute.try_authenticate(*request)? {
            ComputeSubmission::Accepted(ticket) => pending(Stage::Authenticating(ticket), true),
            ComputeSubmission::Backpressured(request) => {
                pending(Stage::Authenticate(Box::new(request)), false)
            }
        }),
        Stage::Authenticating(mut ticket) => {
            if let Some(body) = ticket.try_take()? {
                Ok(Advancement::Authenticated(body))
            } else {
                Ok(pending(Stage::Authenticating(ticket), false))
            }
        }
        Stage::Bind(request) => Ok(match compute.try_bind(*request)? {
            ComputeSubmission::Accepted(ticket) => pending(Stage::Preparing(ticket), true),
            ComputeSubmission::Backpressured(request) => {
                pending(Stage::Bind(Box::new(request)), false)
            }
        }),
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
        Stage::Capture(mut capture) => match capture.advance_with_seed(
            config.capture_edge_quantum,
            cache
                .as_ref()
                .and_then(|cache| cache.for_context(capture.context())),
        )? {
            CaptureStep::More => Ok(pending(Stage::Capture(capture), true)),
            CaptureStep::Complete => {
                capture_observation.seed_nodes = capture.seed_hits();
                Ok(pending(Stage::Execute(Box::new(capture.finish()?)), true))
            }
            CaptureStep::NeedRead => {
                let hashes = capture
                    .next_request()?
                    .context("capture needs a read without requested hashes")?;
                // The response ticket never leaves this exact capture state.
                // There is only one outstanding frontier request per job.
                let count = hashes.len();
                if let Some(ticket) = io.try_read_nodes(hashes)? {
                    capture_observation.storage_requests += 1;
                    capture_observation.storage_nodes += count;
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
                        seed: computed.seed,
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
            seed,
        } => {
            if let Some(ticket) = io.try_persist(packet.clone())? {
                Ok(pending(
                    Stage::Persisting {
                        packet,
                        observation,
                        ticket,
                        seed,
                    },
                    true,
                ))
            } else {
                Ok(pending(
                    Stage::Persist {
                        packet,
                        observation,
                        seed,
                    },
                    false,
                ))
            }
        }
        Stage::Persisting {
            packet,
            observation,
            mut ticket,
            seed,
        } => {
            if let Some(persisted) = ticket.try_take()? {
                ensure!(
                    persisted.candidate_id == packet.candidate_id()
                        && persisted.state_root == packet.state_root()
                        && persisted.statement_commitment == packet.statement_commitment()
                        && persisted.document_digest == packet.document_digest(),
                    "stored reply differs from executed packet"
                );
                // Successful content completion, not head finality. Replacing
                // this one optional byte source never retains a parent slot.
                *cache = None;
                if let Some(seed) = seed {
                    *cache = SeedCache::try_install(seed, &packet, config, usage)?;
                }
                Ok(Advancement::Complete(DurableBatch {
                    candidate: DurableCandidate {
                        owner: identity.clone(),
                        packet: packet.clone(),
                    },
                    packet,
                    persisted,
                    observation,
                    capture: *capture_observation,
                }))
            } else {
                Ok(pending(
                    Stage::Persisting {
                        packet,
                        observation,
                        ticket,
                        seed,
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
    receiver: mpsc::Receiver<DriverMessage>,
    identity: &Arc<()>,
) {
    let mut jobs = VecDeque::<Job>::new();
    let mut connected = true;
    let mut cache = None;
    while connected || !jobs.is_empty() {
        if jobs.is_empty() && connected {
            match receiver.recv() {
                Ok(command) => enqueue(&mut jobs, command.into()),
                Err(_) => {
                    connected = false;
                    continue;
                }
            }
        }
        while connected && jobs.len() < config.max_batches {
            match receiver.try_recv() {
                Ok(command) => enqueue(&mut jobs, command.into()),
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
                background,
                mut capture,
            } = jobs.pop_front().expect("fixed round length");
            let label = stage.label();
            match advance(
                stage,
                (&mut candidate_id, &mut capture),
                config,
                io,
                compute,
                identity,
                (&mut cache, &_permit.usage),
            ) {
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
                        background,
                        capture,
                    });
                }
                Ok(Advancement::Complete(output)) => {
                    if let Reply::Durable(reply) = reply {
                        let _ = reply.send(Ok(output));
                    } else {
                        reply.fail(anyhow::anyhow!(
                            "durable result for authentication-only request"
                        ));
                    }
                    progressed = true;
                }
                Ok(Advancement::Authenticated(body)) => {
                    if let Reply::Authenticated(reply) = reply {
                        let _ = reply.send(Ok(AuthenticatedBody {
                            body: Box::new(body),
                            owner: identity.clone(),
                            permit: _permit,
                        }));
                    } else {
                        reply.fail(anyhow::anyhow!("authentication result for durable request"));
                    }
                    progressed = true;
                }
                Err(error) => {
                    let error = PipelineFailure {
                        stage: label,
                        candidate_id,
                        message: format!("{error:#}"),
                    };
                    reply.fail(error.into());
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

fn enqueue(jobs: &mut VecDeque<Job>, job: Job) {
    // Do not cancel or restart a background native operation already in flight.
    // On each bounded round ordinary jobs get their transition first; complete
    // round rotation preserves this order, including after a job completes.
    let index = if job.background {
        jobs.len()
    } else {
        jobs.iter()
            .position(|pending| pending.background)
            .unwrap_or(jobs.len())
    };
    jobs.insert(index, job);
}

#[cfg(test)]
mod tests;
