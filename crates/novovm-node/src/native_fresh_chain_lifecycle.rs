//! Fresh-chain confirmation and opt-in received-successor handoff.
//! Enqueue never executes or signs. Poll verifies the live finalized parent,
//! assembles an authenticated body, executes locally, then opens the V3 service.
use super::*;
use crate::native_candidate_body::network::CandidateBodyInboxV1;
use crate::native_fresh_timing::{measure, Span};
#[path = "native_fresh_chain_clock.rs"]
mod clock;
#[path = "native_fresh_chain_history.rs"]
mod history;
#[path = "native_fresh_chain_pacemaker.rs"]
mod pacemaker;
#[cfg(test)]
pub(crate) use pacemaker::exercise_parent_pacemaker;
#[path = "native_fresh_chain_proposer.rs"]
mod proposer;
#[path = "native_fresh_transaction_transport.rs"]
mod transaction_transport;
#[path = "native_fresh_chain_transactions.rs"]
mod transactions;
use crate::tx_ingress::{
    candidate_workspace::FinalizedParentViewV1, fresh_pool::FreshTransactionPool,
};

/// The existing main-node execution thread budget. Debug AOEM verification and
/// execution use a deep bounded call chain; exercise the same budget in integration tests.
pub const FRESH_CHAIN_LIFECYCLE_STACK_BYTES_V1: usize = 8 * 1024 * 1024;

/// Run on the main node's dedicated thread with FRESH_CHAIN_LIFECYCLE_STACK_BYTES_V1.
pub struct FreshChainLifecycleV1 {
    clock_waiting: bool,
    future_timestamp_rejected: u64,
    pool: Option<FreshTransactionPool>,
    finalized_parent: Option<FinalizedParentViewV1>,
    transaction_transport: transaction_transport::TransactionTransport,
    history: Option<history::HistorySync>,
    config: Option<NovNativeSealServiceConfigV1>,
    chain: u64,
    receive_successors: bool,
    ledger_path: PathBuf,
    params: serde_json::Value,
    service: Option<Box<NovNativeSealServiceV1>>,
    publication: Option<Box<FreshGenesisPublicationDriverV1>>,
    bodies: Option<CandidateBodyInboxV1>,
    pacemaker: Option<pacemaker::ParentPacemaker>,
    pending: BTreeMap<String, VecDeque<ProductMainlineOverlayInboundV1>>,
    round_pending: BTreeMap<String, VecDeque<ProductMainlineOverlayInboundV1>>,
    next_peer: usize,
    last_seen: Instant,
    halted: bool,
    received_successors: u64,
    rejected: u64,
    proposed_successors: u64,
}

impl FreshChainLifecycleV1 {
    pub fn open(
        config: NovNativeSealServiceConfigV1,
        ledger_path: &Path,
        params: &serde_json::Value,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<Self> {
        if !config.is_fresh_genesis() {
            bail!("fresh lifecycle requires pinned genesis");
        }
        // Runtime/path checks precede any startup recovery or workspace opening.
        config.validate(runtime.chain_id())?;
        check_runtime(&config, runtime)?;
        let writes = crate::tx_ingress::native_persistence_write_paths_v1(params)
            .into_iter()
            .filter(|(label, _)| *label != "native block ledger")
            .map(|(_, path)| path)
            .collect::<Vec<_>>();
        validate_service_paths_v1(&config, ledger_path, &writes, &[])?;
        let config = config.resolve_lifecycle_startup(params)?;
        let publication =
            FreshGenesisPublicationDriverV1::open(&config, ledger_path, params, runtime, now)?
                .map(Box::new);
        let service = if publication.is_none() {
            Some(Box::new(NovNativeSealServiceV1::open_configured(
                config.clone(),
                ledger_path,
                params,
                runtime,
                now,
            )?))
        } else {
            None
        };
        let pending: BTreeMap<_, _> = config
            .authority
            .transport_bindings
            .iter()
            .filter(|b| b.validator_id != config.local_validator_id)
            .map(|b| (b.transport_peer_id.clone(), VecDeque::new()))
            .collect();
        let transaction_transport = transaction_transport::TransactionTransport::new(
            pending.keys().cloned(),
            config.transaction_transport_limits(),
            now,
        );
        let mut this = Self {
            clock_waiting: false,
            future_timestamp_rejected: 0,
            pool: config
                .transaction_pool_path()
                .map(|path| {
                    FreshTransactionPool::open(
                        &path,
                        config.chain_id,
                        config
                            .fresh_genesis_config_commitment
                            .expect("fresh config validated"),
                        params,
                    )
                })
                .transpose()?,
            finalized_parent: None,
            transaction_transport,
            history: config
                .receive_successors
                .then(|| history::HistorySync::new(&config, params, now))
                .transpose()?,
            chain: config.chain_id,
            receive_successors: config.receive_successors,
            config: Some(config),
            ledger_path: ledger_path.into(),
            params: params.clone(),
            service,
            publication,
            bodies: None,
            pacemaker: None,
            round_pending: pending
                .keys()
                .map(|peer| (peer.clone(), VecDeque::new()))
                .collect(),
            pending,
            next_peer: 0,
            last_seen: now,
            halted: false,
            received_successors: 0,
            rejected: 0,
            proposed_successors: 0,
        };
        this.arm_body_reception(now)?;
        if this.finalized_parent.is_none() && this.pool.is_some() {
            // The publication driver above completes a durable pending QC
            // before caching a tip. Otherwise open_configured has already
            // required a fully finalized parent. Do not move this strict view
            // into the pending-tolerant startup-artifact discovery phase.
            if let Some(parent_id) = this
                .config
                .as_ref()
                .and_then(|config| config.finalized_parent_workspace_id)
            {
                let config = this
                    .config
                    .as_ref()
                    .context("recovery configuration missing")?;
                let parent = crate::tx_ingress::candidate_workspace::load_finalized_parent_view_v1(
                    config.chain_id,
                    parent_id,
                    config
                        .fresh_genesis_config_commitment
                        .context("recovery genesis missing")?,
                    params,
                )?;
                if let Some(pool) = &mut this.pool {
                    pool.reconcile_rooted(&parent, params)?;
                }
                this.finalized_parent = Some(parent);
            }
        }
        Ok(this)
    }

    fn arm_body_reception(&mut self, now: Instant) -> Result<()> {
        if self.receive_successors && self.publication.is_some() {
            let config = self.config.as_ref().context("successor identity missing")?;
            let parent = crate::tx_ingress::candidate_workspace::load_finalized_parent_view_v1(
                config.chain_id,
                config
                    .isolated_workspace_id
                    .context("parent workspace missing")?,
                config
                    .fresh_genesis_config_commitment
                    .context("parent genesis missing")?,
                &self.params,
            )?;
            if let Some(pool) = &mut self.pool {
                pool.reconcile_rooted(&parent, &self.params)?;
            }
            self.finalized_parent = Some(parent);
            self.pacemaker = Some(pacemaker::ParentPacemaker::open(config, &self.params, now)?);
            self.bodies = Some(CandidateBodyInboxV1::new(
                config.authority.clone(),
                config
                    .height
                    .checked_add(1)
                    .context("successor height overflow")?,
                now,
            )?);
        } else if self.publication.is_some() {
            // Preserve the original keyless-relay boundary when reception is
            // disabled. Only the explicit continuous mode retains the signer.
            self.config = None;
        }
        Ok(())
    }

    /// Bounded authenticated runtime events only; no execution, writes or votes.
    pub fn enqueue(&mut self, inbound: ProductMainlineOverlayInboundV1) -> bool {
        if self.halted {
            return false;
        }
        if history::HistorySync::is_request(&inbound) {
            return match (&mut self.history, &self.config) {
                (Some(history), Some(config)) => history.enqueue(&inbound, config, self.last_seen),
                _ => false,
            };
        }
        if inbound.payload_class == ProductMainlineOverlayPayloadClassV1::NativeTransaction {
            if self.pool.is_some()
                && inbound.frame.stream_id == self.chain
                && self.transaction_transport.enqueue(inbound)
            {
                return true;
            }
            self.rejected = self.rejected.saturating_add(1);
            return false;
        }
        if let Some(service) = self.service.as_mut() {
            return service.enqueue(inbound);
        }
        let permitted_class =
            inbound.payload_class == ProductMainlineOverlayPayloadClassV1::NativeSeal;
        let admissible = self.bodies.is_some()
            && permitted_class
            && inbound.frame.stream_id == self.chain
            && inbound.frame.payload.len() <= crate::product_mainline_overlay::PRODUCT_MAINLINE_OVERLAY_MAX_CLASSIFIED_LOGICAL_PAYLOAD_BYTES_V1;
        if admissible {
            let queues = if is_nov_native_seal_round_wire_v1(&inbound.frame.payload)
                && inbound
                    .frame
                    .payload
                    .get(10)
                    .is_some_and(|kind| matches!(kind, 1..=3))
            {
                &mut self.round_pending
            } else {
                &mut self.pending
            };
            if let Some(queue) = queues.get_mut(&inbound.source_peer_id) {
                if queue.len() < PER_PEER_QUEUE {
                    queue.push_back(inbound);
                    return true;
                }
            }
        }
        self.rejected = self.rejected.saturating_add(1);
        false
    }

    pub fn poll(&mut self, runtime: &ProductMainlineOverlayRuntimeV1, now: Instant) -> Result<()> {
        let wall_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
        self.poll_with_wall_time(runtime, now, u64::try_from(wall_ms.as_millis())?)
    }

    pub(crate) fn poll_with_wall_time(
        &mut self,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
        wall_ms: u64,
    ) -> Result<()> {
        if self.halted {
            bail!("fresh lifecycle halted; inspect and restart");
        }
        let result = self.poll_inner(runtime, now, wall_ms).and_then(|()| {
            // Give this tick's body/vote traffic the first opportunity to use
            // the shared overlay outbox. Transactions have separate bounded
            // staging/work budgets; a failed submission retains its peer cursor.
            measure("lifecycle.transactions_gossip", || {
                self.gossip_transactions(runtime, now)
            })
        });
        if result.is_err() {
            self.halted = true;
        }
        result
    }

    fn poll_inner(
        &mut self,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
        wall_ms: u64,
    ) -> Result<()> {
        if now < self.last_seen {
            bail!("fresh lifecycle monotonic clock moved backwards");
        }
        self.last_seen = now;
        if let Some(config) = &self.config {
            check_runtime(config, runtime)?;
            if let Some(history) = &mut self.history {
                measure("lifecycle.history_poll", || {
                    history.poll(
                        config,
                        &self.ledger_path,
                        self.publication.is_some(),
                        runtime,
                        now,
                    )
                })?;
            }
        }
        measure("lifecycle.transactions_poll", || {
            self.poll_transactions(now)
        })?;
        self.clock_waiting = false;
        if let Some(service) = self.service.as_mut() {
            if !clock::timestamp_allowed(service.candidate_timestamp_unix_ms, wall_ms) {
                self.clock_waiting = true;
                return Ok(());
            }
            measure("lifecycle.service_poll", || service.poll(runtime, now))?;
            if service.status_json()["decision_confirmed"] == true {
                let _span = Span::start("lifecycle.complete_publication_and_arm");
                self.publication = service
                    .complete_fresh_publication(runtime, now)?
                    .map(Box::new);
                if self.publication.is_none() {
                    bail!("confirmed decision has no durable archive");
                }
                self.service = None;
                self.arm_body_reception(now)?;
            }
            return Ok(());
        }
        // Reject corrupt/moved parent authority before processing remote work.
        measure("lifecycle.publication_poll", || {
            self.publication
                .as_mut()
                .context("fresh lifecycle phase missing")?
                .poll(runtime, now)
        })?;
        let Some(inbox) = self.bodies.as_mut() else {
            return Ok(());
        };
        inbox.expire(now)?;
        let config = self.config.as_ref().context("successor identity missing")?;
        if self.finalized_parent.as_ref().is_some_and(|parent| {
            !clock::timestamp_allowed(parent.block().header.timestamp_unix_ms, wall_ms)
        }) {
            self.clock_waiting = true;
            return Ok(());
        }
        let mut events = Vec::new();
        let peers = self.pending.keys().cloned().collect::<Vec<_>>();
        for _ in 0..2 * PER_PEER_QUEUE * peers.len() {
            if events.len() == config.ingress_per_poll {
                break;
            }
            let cursor = self.next_peer % (2 * peers.len());
            let peer = &peers[cursor / 2];
            self.next_peer = (cursor + 1) % (2 * peers.len());
            let queues = if cursor.is_multiple_of(2) {
                &mut self.pending
            } else {
                &mut self.round_pending
            };
            if let Some(event) = queues.get_mut(peer).and_then(VecDeque::pop_front) {
                events.push(event);
            }
        }
        for event in events {
            if is_nov_native_seal_round_wire_v1(&event.frame.payload) {
                if let Some(pacemaker) = &mut self.pacemaker {
                    match pacemaker.ingest(config, &event, now) {
                        Ok(true) => continue,
                        Ok(false) => (),
                        Err(_) => {
                            self.rejected = self.rejected.saturating_add(1);
                            continue;
                        }
                    }
                }
            }
            let completed = match inbox.accept_with_manifest(&event, now) {
                Ok(body) => body,
                Err(_) => {
                    self.rejected = self.rejected.saturating_add(1);
                    continue;
                }
            };
            let Some((body, manifest)) = completed else {
                continue;
            };
            if !self
                .pacemaker
                .as_mut()
                .context("successor pacemaker missing")?
                .admit_body_round(config, &self.params, &body.message, now)?
            {
                self.rejected = self.rejected.saturating_add(1);
                continue;
            }
            if !clock::timestamp_allowed(
                body.message
                    .proposal()
                    .context("verified body proposal missing")?
                    .subject
                    .timestamp_unix_ms,
                wall_ms,
            ) {
                self.future_timestamp_rejected = self.future_timestamp_rejected.saturating_add(1);
                self.rejected = self.rejected.saturating_add(1);
                continue;
            }
            let certificate = body.message.certificate().cloned();
            let next = match measure("lifecycle.received_successor_prepare", || {
                config
                    .clone()
                    .prepare_received_successor(body, &self.params)
            }) {
                Ok(next) => next,
                Err(_) => {
                    self.rejected = self.rejected.saturating_add(1);
                    continue;
                }
            };
            let mut service = measure("lifecycle.received_successor_open", || {
                NovNativeSealServiceV1::open_configured(
                    next.clone(),
                    &self.ledger_path,
                    &self.params,
                    runtime,
                    now,
                )
            })?;
            if let Some(certificate) = &certificate {
                service.admit_successor_new_view(certificate)?;
            }
            if !service.enqueue(manifest) {
                bail!("verified successor proposal was not admitted");
            }
            // No poll/signing in this handoff: next poll uses the unchanged V3
            // anti-equivocation locks and durable local execution verification.
            self.config = Some(next);
            self.service = Some(Box::new(service));
            self.publication = None;
            self.bodies = None;
            self.pacemaker = None;
            for queue in self.pending.values_mut() {
                queue.clear();
            }
            for queue in self.round_pending.values_mut() {
                queue.clear();
            }
            self.received_successors = self.received_successors.saturating_add(1);
            return Ok(());
        }
        measure("lifecycle.pacemaker_poll", || {
            self.pacemaker
                .as_mut()
                .context("successor pacemaker missing")?
                .poll(config, &self.params, runtime, now)
        })?;
        measure("lifecycle.propose_from_pool", || {
            self.propose_from_pool(runtime, now, wall_ms)
        })
    }

    pub fn status_json(&self) -> serde_json::Value {
        let mut value = self
            .service
            .as_ref()
            .map(|s| s.status_json())
            .or_else(|| self.publication.as_ref().map(|p| p.status_json()))
            .unwrap_or_else(|| serde_json::json!({}));
        value["successor_reception_enabled"] = self.receive_successors.into();
        value["clock_waiting"] = self.clock_waiting.into();
        value["future_timestamp_rejected"] = self.future_timestamp_rejected.into();
        value["max_future_block_time_ms"] = clock::MAX_FUTURE_BLOCK_TIME_MS.into();
        value["durable_pending_transactions"] = self
            .pool
            .as_ref()
            .map_or(0, FreshTransactionPool::len)
            .into();
        value["transaction_transport"] = self.transaction_transport.status_json();
        value["history_responses_served"] = self
            .history
            .as_ref()
            .map_or(0, history::HistorySync::served)
            .into();
        value["automatic_proposal_enabled"] = self
            .config
            .as_ref()
            .is_some_and(|c| c.propose_successors)
            .into();
        value["proposal_max_transactions"] = self
            .config
            .as_ref()
            .map(|c| serde_json::json!(c.proposal_max_transactions))
            .unwrap_or(serde_json::Value::Null);
        value["proposed_successors"] = self.proposed_successors.into();
        value["successor_signer_retained"] =
            (self.receive_successors && self.config.is_some()).into();
        value["awaiting_successor_body"] = self.bodies.is_some().into();
        value["parent_pacemaker"] = self
            .pacemaker
            .as_ref()
            .map(pacemaker::ParentPacemaker::status_json)
            .unwrap_or(serde_json::Value::Null);
        value["parent_round_signing_enabled"] = (self.pacemaker.is_some() && !self.halted).into();
        value["received_successors"] = self.received_successors.into();
        value["successor_rejected"] = self.rejected.into();
        value["lifecycle_halted"] = self.halted.into();
        if self.halted {
            for flag in [
                "ok",
                "safe",
                "finalized",
                "proof_sealed",
                "chain_canonical",
                "decision_confirmed",
            ] {
                value[flag] = false.into();
            }
            value["halted"] = true.into();
        }
        value
    }
}
