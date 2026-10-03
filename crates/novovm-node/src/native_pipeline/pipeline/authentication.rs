//! Parent-independent signature work on the SAME resident AOEM owner. A prepared
//! body has no parent, state witness, nonce lease, persistence or voting power.
//! Its original whole-pipeline permit survives the pause before explicit bind.

use super::*;
use crate::native_pipeline::business::nov_transfer_batch::NovTransferBody;

pub struct AuthenticationRequest {
    pub(super) request: Box<compute::AuthenticateRequest>,
    body_bytes: usize,
    max_transaction_bytes: usize,
    transaction_count: usize,
}

impl AuthenticationRequest {
    /// Assembly-owner operation: checks lengths/policy, not signatures or state.
    pub fn new(raw_transactions: Vec<Vec<u8>>, policy: DirectNovFeePolicy) -> Result<Self> {
        ensure!(!raw_transactions.is_empty(), "empty pipeline request");
        policy.validate()?;
        let body_bytes = raw_transactions.iter().try_fold(0usize, |total, raw| {
            ensure!(!raw.is_empty(), "empty pipeline transaction");
            total
                .checked_add(raw.len())
                .context("pipeline body size overflow")
        })?;
        Ok(Self {
            max_transaction_bytes: raw_transactions.iter().map(Vec::len).max().unwrap_or(0),
            transaction_count: raw_transactions.len(),
            body_bytes,
            request: Box::new(compute::AuthenticateRequest {
                raw_transactions,
                policy,
            }),
        })
    }

    fn reservation(&self, config: &PipelineConfig) -> Result<usize> {
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
        BatchRequest::retained_reservation(self.body_bytes, config)
    }
}

pub enum AuthenticationSubmission {
    Accepted(AuthenticationTicket),
    Backpressured(AuthenticationRequest),
}

pub struct RejectedAuthenticationSubmission {
    pub request: AuthenticationRequest,
    pub error: anyhow::Error,
}

impl std::fmt::Debug for RejectedAuthenticationSubmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RejectedAuthenticationSubmission")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

pub struct AuthenticationTicket {
    receiver: mpsc::Receiver<Result<AuthenticatedBody>>,
    permit: Option<Arc<Permit>>,
}

impl AuthenticationTicket {
    /// A completed reply holds the permit itself; consuming this ticket does
    /// not release its retained bytes. Dropping a ticket never cancels AOEM.
    pub fn try_take(&mut self) -> Result<Option<AuthenticatedBody>> {
        ensure!(
            self.permit.is_some(),
            "authentication ticket already consumed"
        );
        match self.receiver.try_recv() {
            Ok(result) => {
                self.permit.take();
                result.map(Some)
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.permit.take();
                anyhow::bail!("candidate coordinator disconnected during authentication")
            }
        }
    }
}

/// Immutable verified NOV body retained by this pipeline, not chain admission.
/// No Clone, constructor, context setter, DB handle or publication authority.
pub struct AuthenticatedBody {
    pub(super) body: Box<NovTransferBody>,
    pub(super) owner: Arc<()>,
    pub(super) permit: Arc<Permit>,
}

impl AuthenticatedBody {
    /// O(1) consuming envelope only. The exact domain/program/parent shape is
    /// validated on admission/compute, then state is freshly captured as usual.
    /// This does NOT certify that the supplied parent is canonical.
    pub fn bind(self, context: BatchContext) -> AuthenticatedRequest {
        AuthenticatedRequest {
            body: self,
            context: Box::new(context),
        }
    }
}

pub struct AuthenticatedRequest {
    pub(super) body: AuthenticatedBody,
    pub(super) context: Box<BatchContext>,
}

pub enum AuthenticatedSubmission {
    Accepted(PipelineTicket),
    Backpressured(AuthenticatedRequest),
}

pub struct RejectedAuthenticatedSubmission {
    pub request: AuthenticatedRequest,
    pub error: anyhow::Error,
}

impl std::fmt::Debug for RejectedAuthenticatedSubmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RejectedAuthenticatedSubmission")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

pub(super) struct AuthenticationCommand {
    pub request: AuthenticationRequest,
    pub reply: mpsc::Sender<Result<AuthenticatedBody>>,
    pub permit: Arc<Permit>,
}

pub(super) struct BindCommand {
    pub request: AuthenticatedRequest,
    pub reply: mpsc::Sender<Result<DurableBatch>>,
    pub background: bool,
}

impl CandidatePipeline {
    /// Static feasibility, not queue availability or a reservation. Controllers
    /// must not hold current-height input forever when optional background
    /// authentication cannot fit even an otherwise empty configured pipeline.
    pub(crate) fn can_authenticate_background(
        &self,
        request: &AuthenticationRequest,
    ) -> Result<bool> {
        let bytes = request.reservation(&self.config)?;
        let current = BatchRequest::retained_reservation(
            self.config
                .authentication
                .body_bytes
                .min(self.config.plan.body_bytes),
            &self.config,
        )?;
        Ok(self.config.max_batches > 1
            && bytes
                .checked_add(current)
                .is_some_and(|total| total <= self.config.max_retained_bytes))
    }

    /// Optional work: at most one background permit and always reserve a full
    /// ordinary batch. The ordinary and preauthenticated routes share budgets,
    /// the compute session and all subsequent state/persistence validation.
    pub fn try_authenticate_owned(
        &self,
        request: AuthenticationRequest,
    ) -> std::result::Result<AuthenticationSubmission, RejectedAuthenticationSubmission> {
        let prerequisites = (|| {
            let bytes = request.reservation(&self.config)?;
            let sender = self.sender.as_ref().context("pipeline closed")?;
            let worker = self
                .worker
                .as_ref()
                .context("pipeline worker unavailable")?;
            Ok::<_, anyhow::Error>((sender, worker, self.reserve(bytes, true)?))
        })();
        let (sender, worker, permit) = match prerequisites {
            Ok(values) => values,
            Err(error) => return Err(RejectedAuthenticationSubmission { request, error }),
        };
        let Some(permit) = permit else {
            return Ok(AuthenticationSubmission::Backpressured(request));
        };
        let (reply, receiver) = mpsc::channel();
        let command = AuthenticationCommand {
            request,
            reply,
            permit: permit.clone(),
        };
        match sender.try_send(DriverMessage::Authenticate(command)) {
            Ok(()) => {
                worker.thread().unpark();
                Ok(AuthenticationSubmission::Accepted(AuthenticationTicket {
                    receiver,
                    permit: Some(permit),
                }))
            }
            Err(mpsc::TrySendError::Full(DriverMessage::Authenticate(command))) => {
                Ok(AuthenticationSubmission::Backpressured(command.request))
            }
            Err(mpsc::TrySendError::Disconnected(DriverMessage::Authenticate(command))) => {
                Err(RejectedAuthenticationSubmission {
                    request: command.request,
                    error: anyhow::anyhow!("pipeline unavailable; authentication not accepted"),
                })
            }
            Err(_) => unreachable!("typed authentication command changed while sending"),
        }
    }

    /// Continue an already reserved body; NEVER reserve twice or authenticate
    /// twice. Failed enqueue returns that same owned request with its permit.
    pub fn try_submit_authenticated_owned(
        &self,
        request: AuthenticatedRequest,
    ) -> std::result::Result<AuthenticatedSubmission, RejectedAuthenticatedSubmission> {
        self.try_submit_authenticated_with_priority(request, false)
    }

    /// Continue optional future work without promoting its scheduling priority
    /// before the exact parent ACK. This reuses its original background permit;
    /// it neither reserves a second slot nor grants any parent authority.
    pub(crate) fn try_submit_authenticated_background_owned(
        &self,
        request: AuthenticatedRequest,
    ) -> std::result::Result<AuthenticatedSubmission, RejectedAuthenticatedSubmission> {
        self.try_submit_authenticated_with_priority(request, true)
    }

    fn try_submit_authenticated_with_priority(
        &self,
        request: AuthenticatedRequest,
        background: bool,
    ) -> std::result::Result<AuthenticatedSubmission, RejectedAuthenticatedSubmission> {
        let prerequisites = (|| {
            ensure!(
                Arc::ptr_eq(&request.body.owner, &self.identity),
                "authenticated body belongs to another pipeline"
            );
            let domain = self.config.store.domain;
            let context = &request.context;
            ensure!(
                context.chain_id == domain.chain_id
                    && context.genesis_config_commitment == domain.genesis_config_commitment
                    && context.protocol_commitment == domain.protocol_commitment,
                "pipeline request/store domain mismatch"
            );
            Ok::<_, anyhow::Error>((
                self.sender.as_ref().context("pipeline closed")?,
                self.worker
                    .as_ref()
                    .context("pipeline worker unavailable")?,
            ))
        })();
        let (sender, worker) = match prerequisites {
            Ok(values) => values,
            Err(error) => return Err(RejectedAuthenticatedSubmission { request, error }),
        };
        let permit = request.body.permit.clone();
        let (reply, receiver) = mpsc::channel();
        match sender.try_send(DriverMessage::Bind(BindCommand {
            request,
            reply,
            background,
        })) {
            Ok(()) => {
                worker.thread().unpark();
                Ok(AuthenticatedSubmission::Accepted(PipelineTicket {
                    receiver,
                    permit: Some(permit),
                }))
            }
            Err(mpsc::TrySendError::Full(DriverMessage::Bind(command))) => {
                Ok(AuthenticatedSubmission::Backpressured(command.request))
            }
            Err(mpsc::TrySendError::Disconnected(DriverMessage::Bind(command))) => {
                Err(RejectedAuthenticatedSubmission {
                    request: command.request,
                    error: anyhow::anyhow!("pipeline unavailable; bound input not accepted"),
                })
            }
            Err(_) => unreachable!("typed binding command changed while sending"),
        }
    }
}

#[cfg(test)]
mod tests;
