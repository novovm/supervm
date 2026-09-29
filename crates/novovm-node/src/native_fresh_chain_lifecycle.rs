//! Fresh-chain confirmation and opt-in received-successor handoff.
//! Enqueue never executes or signs. Poll verifies the live finalized parent,
//! assembles an authenticated body, executes locally, then opens the V3 service.
use super::*;
use crate::native_candidate_body::network::CandidateBodyInboxV1;
#[path = "native_fresh_chain_proposer.rs"]
mod proposer;

/// The existing main-node execution thread budget. Debug AOEM verification and
/// execution use a deep bounded call chain; exercise the same budget in integration tests.
pub const FRESH_CHAIN_LIFECYCLE_STACK_BYTES_V1: usize = 8 * 1024 * 1024;

/// Run on the main node's dedicated thread with FRESH_CHAIN_LIFECYCLE_STACK_BYTES_V1.
pub struct FreshChainLifecycleV1 {
    config: Option<NovNativeSealServiceConfigV1>,
    chain: u64,
    receive_successors: bool,
    ledger_path: PathBuf,
    params: serde_json::Value,
    service: Option<Box<NovNativeSealServiceV1>>,
    publication: Option<Box<FreshGenesisPublicationDriverV1>>,
    bodies: Option<CandidateBodyInboxV1>,
    pending: BTreeMap<String, VecDeque<ProductMainlineOverlayInboundV1>>,
    transaction_budgets: BTreeMap<String, (Instant, usize)>,
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
        validate_service_paths_v1(&config, ledger_path, &[], &[])?;
        let config = config.resolve_finalized_startup(params)?;
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
        let transaction_budgets = pending
            .keys()
            .map(|peer| (peer.clone(), (now, 0)))
            .collect();
        let mut this = Self {
            chain: config.chain_id,
            receive_successors: config.receive_successors,
            config: Some(config),
            ledger_path: ledger_path.into(),
            params: params.clone(),
            service,
            publication,
            bodies: None,
            pending,
            transaction_budgets,
            next_peer: 0,
            last_seen: now,
            halted: false,
            received_successors: 0,
            rejected: 0,
            proposed_successors: 0,
        };
        this.arm_body_reception(now)?;
        Ok(this)
    }

    fn arm_body_reception(&mut self, now: Instant) -> Result<()> {
        if self.receive_successors && self.publication.is_some() {
            let config = self.config.as_ref().context("successor identity missing")?;
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
        if let Some(service) = self.service.as_mut() {
            return service.enqueue(inbound);
        }
        let permitted_class = inbound.payload_class
            == ProductMainlineOverlayPayloadClassV1::NativeSeal
            || (inbound.payload_class == ProductMainlineOverlayPayloadClassV1::NativeTransaction
                && self.config.as_ref().is_some_and(|c| c.propose_successors));
        let admissible = self.bodies.is_some()
            && permitted_class
            && inbound.frame.stream_id == self.chain
            && inbound.frame.payload.len() <= crate::product_mainline_overlay::PRODUCT_MAINLINE_OVERLAY_MAX_CLASSIFIED_LOGICAL_PAYLOAD_BYTES_V1;
        if admissible {
            if let Some(queue) = self.pending.get_mut(&inbound.source_peer_id) {
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
        let result = self.poll_inner(runtime, now, wall_ms);
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
        }
        if let Some(service) = self.service.as_mut() {
            service.poll(runtime, now)?;
            if service.status_json()["decision_confirmed"] == true {
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
        self.publication
            .as_mut()
            .context("fresh lifecycle phase missing")?
            .poll(runtime, now)?;
        let Some(inbox) = self.bodies.as_mut() else {
            return Ok(());
        };
        inbox.expire(now)?;
        let config = self.config.as_ref().context("successor identity missing")?;
        // Round-robin bounded by four queued frames per peer; process at most
        // the configured ingress budget. No unbounded remote execution queue.
        let mut events = Vec::new();
        let peers = self.pending.keys().cloned().collect::<Vec<_>>();
        for _ in 0..PER_PEER_QUEUE * peers.len() {
            if events.len() == config.ingress_per_poll {
                break;
            }
            let peer = &peers[self.next_peer % peers.len()];
            self.next_peer = (self.next_peer + 1) % peers.len();
            if let Some(event) = self.pending.get_mut(peer).and_then(VecDeque::pop_front) {
                events.push(event);
            }
        }
        let mut transactions = Vec::new();
        for event in events {
            if event.payload_class == ProductMainlineOverlayPayloadClassV1::NativeTransaction {
                let budget = self
                    .transaction_budgets
                    .get_mut(&event.source_peer_id)
                    .context("transaction source is not pinned")?;
                if now.duration_since(budget.0) >= Duration::from_secs(1) {
                    *budget = (now, 0);
                }
                if budget.1 >= config.ingress_per_source_per_second {
                    self.rejected = self.rejected.saturating_add(1);
                    continue;
                }
                budget.1 += 1;
                transactions.push(event);
                continue;
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
            let next = match config
                .clone()
                .prepare_received_successor(body, &self.params)
            {
                Ok(next) => next,
                Err(_) => {
                    self.rejected = self.rejected.saturating_add(1);
                    continue;
                }
            };
            let mut service = NovNativeSealServiceV1::open_configured(
                next.clone(),
                &self.ledger_path,
                &self.params,
                runtime,
                now,
            )?;
            if !service.enqueue(manifest) {
                bail!("verified successor proposal was not admitted");
            }
            // No poll/signing in this handoff: next poll uses the unchanged V3
            // anti-equivocation locks and durable local execution verification.
            self.config = Some(next);
            self.service = Some(Box::new(service));
            self.publication = None;
            self.bodies = None;
            for queue in self.pending.values_mut() {
                queue.clear();
            }
            self.received_successors = self.received_successors.saturating_add(1);
            return Ok(());
        }
        self.propose_from_transactions(transactions, runtime, now, wall_ms)
    }

    pub fn status_json(&self) -> serde_json::Value {
        let mut value = self
            .service
            .as_ref()
            .map(|s| s.status_json())
            .or_else(|| self.publication.as_ref().map(|p| p.status_json()))
            .unwrap_or_else(|| serde_json::json!({}));
        value["successor_reception_enabled"] = self.receive_successors.into();
        value["automatic_proposal_enabled"] = self
            .config
            .as_ref()
            .is_some_and(|c| c.propose_successors)
            .into();
        value["proposed_successors"] = self.proposed_successors.into();
        value["successor_signer_retained"] =
            (self.receive_successors && self.config.is_some()).into();
        value["awaiting_successor_body"] = self.bodies.is_some().into();
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
