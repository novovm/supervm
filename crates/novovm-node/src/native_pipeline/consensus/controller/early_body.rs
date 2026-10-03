//! Optional one-body lookahead. Authentication is independent of state; the
//! first exact binding is immutable. A locally acknowledged parent is still
//! required by the existing execution/journal paths before publication.
use super::*;
use crate::native_pipeline::consensus::transport::EarlyBodyScope;
use crate::native_pipeline::pipeline::{
    AuthenticatedBody, AuthenticationRequest, AuthenticationSubmission,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EarlyOrigin {
    scope: EarlyBodyScope,
    id: Hash,
    generation: u64,
}

pub(super) struct EarlyWork {
    generation: u64,
    scope: EarlyBodyScope,
    source: String,
    prepared: Option<PreparedMessage>,
    request: Option<AuthenticationRequest>,
    ticket: Option<AuthenticationTicket>,
    authenticated: Option<AuthenticatedBody>,
    binding: Option<BatchContext>,
    binding_preparing: bool,
    canonical: Option<Ready>,
    promoted: Option<(Context, ParentPoint)>,
    local_coordinates: Option<(u64, u64)>,
}

impl EarlyWork {
    fn new(generation: u64, scope: EarlyBodyScope, source: String) -> Self {
        Self {
            generation,
            scope,
            source,
            prepared: None,
            request: None,
            ticket: None,
            authenticated: None,
            binding: None,
            binding_preparing: false,
            canonical: None,
            promoted: None,
            local_coordinates: None,
        }
    }
    fn origin(&self) -> Option<EarlyOrigin> {
        Some(EarlyOrigin {
            scope: self.scope,
            id: self.prepared.as_ref()?.early_id()?,
            generation: self.generation,
        })
    }
    pub(super) fn retained_bodies(&self) -> impl Iterator<Item = (Hash, usize)> + '_ {
        self.prepared
            .iter()
            .chain(self.canonical.iter().map(|ready| &ready.prepared))
            .map(|p| (p.fragment_id(), p.retained_bytes()))
    }
    pub(super) fn has_ticket(&self) -> bool {
        self.ticket.is_some()
    }
}

pub(super) fn survives_parent_ack(
    message: &Message,
    current: Context,
    parent: ParentPoint,
) -> bool {
    match message {
        Message::EarlyBody { scope, .. } | Message::ApflEarlyBody { scope, .. } => {
            scope.target_height == current.height
                && same_domain(scope.source, current)
                && scope.source.height == parent.height
        }
        Message::BindBody { scope, context, .. } => {
            scope.target_height == current.height
                && same_domain(scope.source, current)
                && context.height == current.height
                && context.parent_height == parent.height
                && context.parent_block_hash == parent.block_hash
                && context.parent_state_root == parent.state_root
                && context.parent_receipt_root == parent.receipt_batch_commitment
                && context.parent_state_version == parent.state_version
        }
        _ => false,
    }
}

impl Controller {
    /// Only the scheduled next-height round-zero proposer may announce raw
    /// transactions. This returns no guessed state root or parent capability.
    pub fn early_body_scope(&self) -> Result<Option<EarlyBodyScope>> {
        if self.is_recovering()
            || self.early.is_some()
            || self.successor.is_some()
            || self.config.limits.max_inflight < 2
            || !self.channel.supports_early_binding()
            || self.early_drain.is_some()
            || self.successor_drain.is_some()
        {
            return Ok(None);
        }
        let Some(target_height) = self.context().height.checked_add(1) else {
            return Ok(None);
        };
        if self.config.validators.leader(target_height, 0)? != self.config.local_validator {
            return Ok(None);
        }
        let scope = EarlyBodyScope {
            source: self.context(),
            source_round: self.round(),
            target_height,
        };
        Ok((self.early_attempted_scope != Some(scope)).then_some(scope))
    }

    /// Prevent a wallet from duplicating a locally retained early input when
    /// its target becomes current. Failure/retirement releases this hint.
    pub fn has_early_target(&self, height: u64) -> bool {
        self.early
            .as_ref()
            .is_some_and(|e| e.source == self.local_peer && e.scope.target_height == height)
            || self.bodies.values().any(|b| {
                b.source == self.local_peer
                    && !b.failed
                    && b.early_origin
                        .is_some_and(|o| o.scope.target_height == height)
            })
            || self.successor_has_local_early_target(height)
    }

    /// Coordinates are caller-proposed inputs, not protocol timing or authority.
    /// All hashing/encoding/raw cloning remains on HostChannel's owner.
    pub fn try_submit_early_body(
        &mut self,
        message: &Arc<Message>,
        slot: u64,
        timestamp_unix_ms: u64,
    ) -> Result<bool> {
        let (Message::EarlyBody { scope, .. } | Message::ApflEarlyBody { scope, .. }) =
            message.as_ref()
        else {
            anyhow::bail!("local early input is not an EarlyBody");
        };
        scope.validate_shape()?;
        if self.early_body_scope()? != Some(*scope)
            || !self.retired.is_empty()
            || self.preparing.len() >= self.preparation_limit()
        {
            return Ok(false);
        }
        let token = self.allocate_token()?;
        match self.channel.try_prepare(PrepareRequest {
            token,
            input: PrepareInput::New(message.clone()),
        })? {
            PrepareAdmission::Accepted => {
                let mut early = EarlyWork::new(token, *scope, self.local_peer.clone());
                early.local_coordinates = Some((slot, timestamp_unix_ms));
                self.early = Some(early);
                self.early_attempted_scope = Some(*scope);
                self.preparing.insert(
                    token,
                    Preparation {
                        request: None,
                        purpose: Purpose::EarlyBody(token),
                    },
                );
                Ok(true)
            }
            PrepareAdmission::Backpressure(_) => Ok(false),
            PrepareAdmission::Rejected { reason, .. } => anyhow::bail!(reason),
        }
    }

    fn accepts_early_source(&self, source: &str, scope: EarlyBodyScope) -> Result<bool> {
        scope.validate_shape()?;
        Ok(!self.is_recovering()
            && self.config.limits.max_inflight >= 2
            && self.channel.supports_early_binding()
            && self.early_attempted_scope != Some(scope)
            && scope.source == self.context()
            && scope.source_round == self.round()
            && self.peer_for(&self.config.validators.leader(scope.target_height, 0)?)
                == Some(source))
    }

    pub(super) fn early_prepared(&mut self, generation: u64, ready: Ready) -> Result<()> {
        if self
            .early
            .as_ref()
            .is_none_or(|e| e.generation != generation)
        {
            self.stats.stale_results += 1;
            self.retire(Retirement::Ready(ready));
            return Ok(());
        }
        self.keep_early(self.local_peer.clone(), ready, Some(generation))
    }

    pub(super) fn keep_early(
        &mut self,
        source: String,
        mut ready: Ready,
        generation: Option<u64>,
    ) -> Result<()> {
        let Some(body) = ready.early.as_ref() else {
            self.reject("early owner request missing");
            self.retire(Retirement::Ready(ready));
            return Ok(());
        };
        let scope = *body.scope();
        let id = body.id();
        // An already admitted local preparation may finish across precisely one
        // acknowledged parent transition; a new remote old-scope input may not.
        let admitted_local = generation.is_some_and(|g| {
            self.early
                .as_ref()
                .is_some_and(|e| e.generation == g && e.scope == scope && e.source == source)
        });
        if (!admitted_local && !self.accepts_early_source(&source, scope)?)
            || self.successor.is_some()
            || self
                .early
                .as_ref()
                .is_some_and(|e| !admitted_local || e.prepared.is_some())
        {
            // Duplicate announcements do not restart work or extend its scope.
            self.retire(Retirement::Ready(ready));
            return Ok(());
        }
        let charge = ready.prepared.retained_bytes();
        if self.retained_bodies().len() >= self.config.limits.max_bodies
            || charge
                > self
                    .config
                    .limits
                    .max_body_bytes
                    .saturating_sub(self.body_bytes())
        {
            if admitted_local {
                self.discard_early();
            }
            self.reject("early retained-content budget");
            self.retire(Retirement::Ready(ready));
            return Ok(());
        }
        let generation = match generation {
            Some(g) => g,
            None => self.allocate_token()?,
        };
        let mut body = ready.early.take().expect("checked early request");
        let early = self
            .early
            .get_or_insert_with(|| EarlyWork::new(generation, scope, source));
        ensure!(
            ready.prepared.early_id() == Some(id),
            "early owner identity changed"
        );
        early.request = body.take_request();
        early.prepared = Some(ready.prepared.clone());
        self.early_attempted_scope = Some(scope);
        if admitted_local {
            self.cache(ready.prepared.clone(), None);
        }
        self.retire(Retirement::Ready(ready));
        Ok(())
    }

    pub(super) fn receive_early_binding(
        &mut self,
        source: &str,
        scope: EarlyBodyScope,
        id: Hash,
        context: BatchContext,
    ) -> Result<()> {
        let Some(early) = &self.early else {
            return Ok(());
        }; // RequestBody heals missing announcements.
        if early.scope != scope
            || early.source != source
            || early.origin().is_none_or(|o| o.id != id)
        {
            self.reject("binding source/scope/announcement mismatch");
            return Ok(());
        }
        if let Some(bound) = early.binding {
            if bound != context {
                self.reject("early body already bound to another exact context");
            }
            return Ok(());
        }
        if let Err(error) = scope.validate_binding(&id, &context) {
            self.reject(error);
            return Ok(());
        }
        let pin = self.config.execution;
        if context.business_program != pin.business_program
            || context.semantic_version != pin.semantic_version
            || context.effect_contract != pin.effect_contract
            || context.receipt_codec != pin.receipt_codec
            || (early.promoted.is_some() && !self.matches_context(&context))
        {
            self.reject("early binding business or acknowledged parent mismatch");
            return Ok(());
        }
        // First binding is sticky even while the local parent is unavailable.
        self.early.as_mut().expect("checked early").binding = Some(context);
        Ok(())
    }

    pub(super) fn reconcile_early(&mut self, old_context: Context, old_round: u64) -> Result<()> {
        let Some(early) = &self.early else {
            return Ok(());
        };
        let current = self.context();
        let parent = self.parent();
        if !self.is_recovering()
            && ((early.scope.source == current && early.scope.source_round == self.round())
                || early.promoted == Some((current, parent))
                    && self.round() == 0
                    && self.head() == Some(parent))
        {
            return Ok(());
        }
        let crossed = !self.is_recovering()
            && early.promoted.is_none()
            && early.scope.source == old_context
            && early.scope.source_round == old_round
            && current.height == early.scope.target_height
            && same_domain(current, old_context)
            && parent.height == old_context.height
            && self.head() == Some(parent)
            && self.round() == 0
            && early
                .binding
                .as_ref()
                .is_none_or(|bound| self.matches_context(bound));
        if crossed {
            self.early.as_mut().expect("checked early").promoted = Some((current, parent));
        } else {
            self.discard_early();
        }
        Ok(())
    }

    pub(super) fn early_preparation_failed(&mut self, generation: u64) {
        if self
            .early
            .as_ref()
            .is_some_and(|e| e.generation == generation)
        {
            self.discard_early();
        }
    }

    pub(super) fn early_bound_prepared(&mut self, generation: u64, ready: Ready) -> Result<()> {
        let valid = self.early.as_ref().is_some_and(|e| {
            e.generation == generation
                && e.origin()
                    .is_some_and(|o| ready.bound_early == Some((o.scope, o.id)))
                && ready
                    .body
                    .as_ref()
                    .is_some_and(|body| Some(*body.context()) == e.binding)
                && e.canonical.is_none()
        });
        if !valid {
            self.stats.stale_results += 1;
            self.retire(Retirement::Ready(ready));
            return Ok(());
        }
        if self.retained_bodies().len() >= self.config.limits.max_bodies
            || ready.prepared.retained_bytes()
                > self
                    .config
                    .limits
                    .max_body_bytes
                    .saturating_sub(self.body_bytes())
        {
            self.discard_early();
            self.retire(Retirement::Ready(ready));
            return Ok(());
        }
        let early = self.early.as_mut().expect("checked early");
        early.binding_preparing = false;
        early.canonical = Some(ready);
        Ok(())
    }

    pub(super) fn live_early_origin(&self, origin: EarlyOrigin) -> bool {
        self.early.as_ref().and_then(EarlyWork::origin) == Some(origin)
            || self
                .bodies
                .values()
                .any(|b| !b.failed && b.early_origin == Some(origin))
            || self.successor_has_origin(origin)
    }

    pub(super) fn poll_early(&mut self, pipeline: &CandidatePipeline) -> Result<()> {
        if let Some(mut ticket) = self.early_drain.take() {
            match ticket.try_take() {
                Ok(None) => self.early_drain = Some(ticket),
                Ok(Some(body)) => {
                    self.stats.stale_results += 1;
                    self.retire(Retirement::AuthenticatedBody(body));
                }
                Err(error) => self.reject(error),
            }
        }
        self.reconcile_early(self.context(), self.round())?;
        if let Some(request) = self.early.as_ref().and_then(|e| e.request.as_ref()) {
            match pipeline.can_authenticate_background(request) {
                Ok(true) => {}
                Ok(false) => {
                    self.discard_early();
                    return Ok(());
                }
                Err(error) => {
                    self.reject(error);
                    self.discard_early();
                    return Ok(());
                }
            }
        }
        let Some(early) = &mut self.early else {
            return Ok(());
        };
        if let Some(mut ticket) = early.ticket.take() {
            match ticket.try_take() {
                Ok(None) => early.ticket = Some(ticket),
                Ok(Some(body)) => {
                    early.authenticated = Some(body);
                    self.stats.early_authentication_completed += 1;
                    if self.early.as_ref().is_some_and(|e| e.promoted.is_none())
                        && self
                            .early_parent(self.early.as_ref().and_then(|e| e.binding.as_ref()))?
                            .is_none()
                    {
                        self.stats.early_authentication_completed_before_parent += 1;
                    }
                }
                Err(error) => {
                    self.reject(error);
                    self.discard_early();
                    return Ok(());
                }
            }
        }
        if self.retired.is_empty()
            && self.early_drain.is_none()
            && self.successor_drain.is_none()
            && self.inflight_count() < self.config.limits.max_inflight.saturating_sub(1)
        {
            if let Some(request) = self.early.as_mut().and_then(|e| e.request.take()) {
                match pipeline.try_authenticate_owned(request) {
                    Ok(AuthenticationSubmission::Accepted(ticket)) => {
                        self.early.as_mut().expect("live early").ticket = Some(ticket);
                        self.stats.early_authentication_started += 1;
                    }
                    Ok(AuthenticationSubmission::Backpressured(request)) => {
                        self.early.as_mut().expect("live early").request = Some(request)
                    }
                    Err(rejected) => {
                        self.retire(Retirement::AuthenticationRequest(rejected.request));
                        self.reject(rejected.error);
                        self.discard_early();
                        return Ok(());
                    }
                }
            }
        }
        self.bind_local_early()?;
        self.prepare_early_canonical()?;
        self.install_early_canonical()?;
        Ok(())
    }

    fn bind_local_early(&mut self) -> Result<()> {
        let Some(early) = &self.early else {
            return Ok(());
        };
        let Some((slot, timestamp_unix_ms)) = early.local_coordinates else {
            return Ok(());
        };
        let Some(origin) = early.origin() else {
            return Ok(());
        };
        if early.binding.is_some() || !self.retired.is_empty() {
            return Ok(());
        }
        let parent = match early.promoted {
            Some((_, point)) => Some(point),
            None => self.early_parent(None)?,
        };
        let Some(parent) = parent else {
            return Ok(());
        };
        let context = BatchContext {
            height: origin.scope.target_height,
            parent_height: parent.height,
            parent_block_hash: parent.block_hash,
            parent_state_root: parent.state_root,
            parent_receipt_root: parent.receipt_batch_commitment,
            parent_state_version: parent.state_version,
            slot,
            timestamp_unix_ms,
            ..self.config.execution
        };
        let input = PrepareInput::New(Arc::new(Message::BindBody {
            scope: origin.scope,
            announcement_id: origin.id,
            context,
        }));
        if self.prepare(input, Purpose::EarlyBinding(origin))? {
            self.early.as_mut().expect("live early").binding = Some(context);
        }
        Ok(())
    }

    fn prepare_early_canonical(&mut self) -> Result<()> {
        let Some(early) = &self.early else {
            return Ok(());
        };
        let Some(context) = early.binding else {
            return Ok(());
        };
        if early.binding_preparing
            || early.canonical.is_some()
            || !self.retired.is_empty()
            || self.retained_bodies().len() >= self.config.limits.max_bodies
            || self.channel.preparation_charge()
                > self
                    .config
                    .limits
                    .max_body_bytes
                    .saturating_sub(self.body_bytes())
        {
            return Ok(());
        }
        // Do not allocate canonical full output for a parent not yet known on
        // this node. The small binding may wait, never authorize execution.
        if !self.matches_context(&context) && self.early_parent(Some(&context))?.is_none() {
            return Ok(());
        }
        let generation = early.generation;
        let Some(prepared) = early.prepared.clone() else {
            return Ok(());
        };
        if self.prepare(
            PrepareInput::BindEarly {
                early: prepared,
                context,
            },
            Purpose::BindEarly(generation),
        )? {
            self.early.as_mut().expect("live early").binding_preparing = true;
        }
        Ok(())
    }

    fn install_early_canonical(&mut self) -> Result<()> {
        let Some(early) = &self.early else {
            return Ok(());
        };
        let Some(context) = early.binding else {
            return Ok(());
        };
        if early.authenticated.is_none() || early.canonical.is_none() || !self.retired.is_empty() {
            return Ok(());
        }
        let current = self.matches_context(&context);
        if !current && self.early_parent(Some(&context))?.is_none() {
            return Ok(());
        }
        let origin = early.origin().context("bound early identity absent")?;
        let source = early.source.clone();
        let local = source == self.local_peer;
        let early = self.early.as_mut().expect("live early");
        let mut ready = early.canonical.take().expect("checked canonical");
        let id = ready
            .prepared
            .body_id()
            .context("canonical identity absent")?;
        // The owner constructs an unverified request for either layout, but this
        // route MUST retire it and consume the authenticated body exactly once.
        if let Some(request) = ready.body.as_mut().and_then(|body| body.take_request()) {
            self.retire(Retirement::Request(request));
        }
        if current {
            self.keep_body(source, ready, local.then_some(0))?;
        } else {
            self.keep_successor_body(source, ready, false)?;
        }
        let authenticated = self.early.as_mut().and_then(|e| e.authenticated.take());
        let executing = self.inflight.iter().any(|work| work.id == id);
        let target = if current {
            self.bodies.get_mut(&id)
        } else {
            self.successor_body_mut(id)
        };
        let installed = match (target, authenticated) {
            (Some(body), Some(authenticated))
                if !executing
                    && body.request.is_none()
                    && body.authenticated.is_none()
                    && body.candidate.is_none()
                    && !body.failed
                    && body.early_origin.is_none() =>
            {
                body.authenticated = Some(authenticated.bind(context));
                body.early_origin = Some(origin);
                body.local_round = local.then_some(0);
                true
            }
            (_, Some(authenticated)) => {
                self.retire(Retirement::AuthenticatedBody(authenticated));
                false
            }
            (_, None) => false,
        };
        if installed {
            self.stats.early_bind_reused += 1;
            self.release_early(false);
        } else {
            self.discard_early();
        }
        Ok(())
    }

    pub(super) fn preempt_early_for_body(&mut self, charge: usize) {
        if self.retained_bodies().len() >= self.config.limits.max_bodies
            || charge
                > self
                    .config
                    .limits
                    .max_body_bytes
                    .saturating_sub(self.body_bytes())
        {
            self.discard_early();
        }
    }

    fn discard_early(&mut self) {
        self.release_early(true);
    }

    pub(super) fn retire_early_origin_cache(&mut self, origin: EarlyOrigin) {
        self.prune_fixed(|fixed| match fixed.prepared.message().as_ref() {
            Message::EarlyBody { scope, .. } | Message::ApflEarlyBody { scope, .. } => {
                *scope != origin.scope || fixed.prepared.early_id() != Some(origin.id)
            }
            Message::BindBody {
                scope,
                announcement_id,
                ..
            } => *scope != origin.scope || *announcement_id != origin.id,
            _ => true,
        });
    }

    fn release_early(&mut self, discard: bool) {
        let Some(mut early) = self.early.take() else {
            return;
        };
        if discard {
            self.stats.early_discarded += 1;
            if let Some(origin) = early.origin() {
                self.retire_early_origin_cache(origin);
            }
        }
        if let Some(prepared) = early.prepared {
            self.retire(Retirement::Prepared(prepared));
        }
        if let Some(request) = early.request {
            self.retire(Retirement::AuthenticationRequest(request));
        }
        if let Some(body) = early.authenticated {
            self.retire(Retirement::AuthenticatedBody(body));
        }
        if let Some(ready) = early.canonical {
            self.retire(Retirement::Ready(ready));
        }
        if let Some(ticket) = early.ticket.take() {
            assert!(
                self.early_drain.is_none(),
                "multiple abandoned authentication jobs"
            );
            self.early_drain = Some(ticket);
        }
    }
}
