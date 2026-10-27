//! Bounded delayed RPC replies, not a second mempool or execution owner.
//! Signatures run on the pipeline's resident AOEM session. Only completed,
//! ordered results reach the control-owned nonce reservations and pending pool.
use super::*;
use crate::native_fresh_rpc::{DeferredRpcHandler, DeferredRpcReply};
use crate::native_pipeline::ingress::wire::decode_transfer_view_v3;
use crate::native_pipeline::pipeline::{
    AdmissionInput, SignatureAdmissionRequest, SignatureAdmissionSubmission,
    SignatureAdmissionTicket,
};

const MAX_HTTP_WORK: usize = 8;

struct HttpWork {
    requests: VecDeque<Value>,
    replies: Vec<Value>,
    array: bool,
    waiting: bool,
}

enum ResponseSlot {
    Ready(Value),
    Checked(Value),
}

enum Target {
    Http {
        token: u64,
        slots: Vec<ResponseSlot>,
    },
    Gossip {
        complete_body: bool,
        // A full UCA source is owned by this work, not the mutable next-source
        // cursor. Completing it must never clear a newer incoming body.
        detached_body: bool,
        broadcast: bool,
    },
}

enum Phase {
    Queued(SignatureAdmissionRequest),
    Checking(SignatureAdmissionTicket),
    Completed(Result<crate::native_pipeline::pipeline::SignatureAdmissionOutput>),
}

struct Work {
    target: Target,
    phase: Phase,
    rows: usize,
}

#[derive(Default)]
pub(super) struct AdmissionState {
    pub(super) made_progress: bool,
    http: BTreeMap<u64, HttpWork>,
    next_token: u64,
    last_http: u64,
    prefer_gossip: bool,
    prefer_client: bool,
    work: Option<Work>,
    uca: Option<UcaWork>,
    cached_body_boundary_waiting: bool,
    next_signature_submitted_while_capture: u64,
    signature_completed_while_capture: u64,
    max_retained_stage_slots: usize,
    submitted_batches: u64,
    completed_batches: u64,
    checked_rows: u64,
    rejected_rows: u64,
    cancelled_http: u64,
    control_polls_while_checking: u64,
    backpressured_polls: u64,
    uca_capture: UcaCaptureTotals,
}

// Retain the original compute output/permit while authority I/O is pending.
// Popped rows stay in ONE batch protected by that same original byte permit,
// parked in the sole authority slot after signature completion. This is never
// an independently admitted queue or another execution owner.
struct UcaWork {
    _output: crate::native_pipeline::pipeline::SignatureAdmissionOutput,
    rows: Vec<(AdmissionInput, AdmittedTransfer)>,
    read: Option<native_query::UcaBatchStateRead>,
    parent: Option<ParentPoint>,
    capture_started: Option<Instant>,
    complete_body: bool,
    detached_body: bool,
    broadcast: bool,
}

#[derive(Default)]
struct UcaCaptureTotals {
    batches: u64,
    declaration_keys: u64,
    unique_keys: u64,
    io_requests: u64,
    io_nodes: u64,
    parent_restarts: u64,
    elapsed_us: u64,
    max_elapsed_us: u64,
}

impl UcaCaptureTotals {
    fn record(&mut self, read: &native_query::UcaBatchStateRead, started: Option<Instant>) {
        self.record_stats(read.stats(), started);
    }

    fn record_stats(&mut self, stats: native_query::UcaCaptureStats, started: Option<Instant>) {
        self.batches = self.batches.saturating_add(1);
        self.declaration_keys = self
            .declaration_keys
            .saturating_add(stats.declaration_keys as u64);
        self.unique_keys = self.unique_keys.saturating_add(stats.unique_keys as u64);
        self.io_requests = self.io_requests.saturating_add(stats.io_requests as u64);
        self.io_nodes = self.io_nodes.saturating_add(stats.io_nodes as u64);
        if let Some(started) = started {
            let elapsed = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
            self.elapsed_us = self.elapsed_us.saturating_add(elapsed);
            self.max_elapsed_us = self.max_elapsed_us.max(elapsed);
        }
    }
}

impl AdmissionState {
    pub(super) fn has_pending_native_admission(&self) -> bool {
        self.work.is_some() || self.uca.is_some()
    }

    fn record_retained_stages(&mut self) {
        self.max_retained_stage_slots = self
            .max_retained_stage_slots
            .max(usize::from(self.work.is_some()) + usize::from(self.uca.is_some()));
    }

    fn cached_body_completed(&mut self) -> bool {
        if self.has_pending_native_admission() {
            self.cached_body_boundary_waiting = true;
            false
        } else {
            true
        }
    }

    fn authority_body_completed(&mut self, complete_body: bool) -> bool {
        let cached_boundary = std::mem::take(&mut self.cached_body_boundary_waiting);
        complete_body || cached_boundary
    }

    pub(super) fn status(&self) -> Value {
        json!({"owner":"same_resident_aoem_compute_session",
            "http_waiting":self.http.len(),"signature_batch_pending":self.work.is_some(),
            "uca_authority_pending":self.uca.is_some(),
            "signature_completed_waiting_for_authority":self.work.as_ref().is_some_and(|work| matches!(work.phase, Phase::Completed(_))),
            "next_signature_submitted_while_capture":self.next_signature_submitted_while_capture,
            "signature_completed_while_capture":self.signature_completed_while_capture,
            "max_retained_stage_slots":self.max_retained_stage_slots,
            "submitted_batches":self.submitted_batches,"completed_batches":self.completed_batches,
            "checked_rows":self.checked_rows,"rejected_rows":self.rejected_rows,
            "cancelled_http":self.cancelled_http,
            "control_polls_while_checking":self.control_polls_while_checking,
            "backpressured_polls":self.backpressured_polls,
            "uca_state_capture":{
                "batches":self.uca_capture.batches,
                "declared_keys":self.uca_capture.declaration_keys,
                "unique_keys":self.uca_capture.unique_keys,
                "io_requests":self.uca_capture.io_requests,
                "io_nodes":self.uca_capture.io_nodes,
                "parent_restarts":self.uca_capture.parent_restarts,
                "elapsed_us":self.uca_capture.elapsed_us,
                "max_elapsed_us":self.uca_capture.max_elapsed_us,
                "timing_enabled":crate::native_pipeline::profile::enabled(),
                "scope":"completed, failed or parent-restarted capture attempts; active attempt excluded; clock only with PROFILE_STAGES; not authorization consumption or disk-operation count"
            },
            "scope":"local signature checks, not unique transactions or finality"})
    }
}

impl DeferredRpcHandler for RpcLifecycle {
    fn start(&mut self, request: Value) -> DeferredRpcReply {
        let (requests, array) = match request {
            Value::Array(requests) => {
                if requests.is_empty()
                    || requests.len() > 1024
                    || requests.iter().any(|r| !r.is_object())
                {
                    return DeferredRpcReply::Ready(error(
                        Value::Null,
                        "JSON-RPC batch count exceeds bound",
                    ));
                }
                (requests, true)
            }
            request => (vec![request], false),
        };
        if !requests
            .iter()
            .any(|r| r["method"] == "nov_sendRawTransaction")
        {
            let replies: Vec<_> = requests.into_iter().map(|r| self.handle(r)).collect();
            return DeferredRpcReply::Ready(join_replies(replies, array));
        }
        if self.admission.http.len() == MAX_HTTP_WORK || self.admission.next_token == u64::MAX {
            let replies = requests
                .into_iter()
                .map(|r| {
                    error(
                        r.get("id").cloned().unwrap_or(Value::Null),
                        "RPC signature backpressure; transaction not accepted",
                    )
                })
                .collect();
            return DeferredRpcReply::Ready(join_replies(replies, array));
        }
        self.admission.next_token += 1;
        let token = self.admission.next_token;
        self.admission.http.insert(
            token,
            HttpWork {
                requests: requests.into(),
                replies: Vec::new(),
                array,
                waiting: false,
            },
        );
        DeferredRpcReply::Pending(token)
    }

    fn poll(&mut self, token: u64) -> Option<Value> {
        let work = self.admission.http.get(&token)?;
        if work.waiting || !work.requests.is_empty() {
            return None;
        }
        let work = self
            .admission
            .http
            .remove(&token)
            .expect("completed HTTP work");
        Some(join_replies(work.replies, work.array))
    }

    fn cancel(&mut self, token: u64) {
        if self.admission.http.remove(&token).is_some() {
            self.admission.cancelled_http = self.admission.cancelled_http.saturating_add(1);
        }
        // Accepted native work still drains. A cancelled HTTP waiter cannot
        // cause its later signature reply to be delivered to another client.
    }
}

fn join_replies(mut replies: Vec<Value>, array: bool) -> Value {
    if array {
        Value::Array(replies)
    } else {
        replies.pop().expect("one RPC response")
    }
}

fn response(id: Value, result: Result<Value>) -> Value {
    match result {
        Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
        Err(err) => error(id, &err.to_string()),
    }
}

impl RpcLifecycle {
    /// Admission-only counterexample on a real original service and real signed
    /// NUAS bytes. Does not poll consensus/network or publish a candidate.
    #[cfg(test)]
    pub(crate) fn assert_uca_overlap_body_boundaries_for_test(
        &mut self,
        cached: Arc<novovm_protocol::uca_spend_batch::UcaSpendBatch>,
        partial: Arc<novovm_protocol::uca_spend_batch::UcaSpendBatch>,
        newer_bad: Arc<novovm_protocol::uca_spend_batch::UcaSpendBatch>,
    ) -> Result<()> {
        use crate::native_pipeline::consensus::transport::TransactionsScope;
        ensure!(self.is_uca(), "UCA fixture required");
        // Reduce only this control-side service quantum. Keep the original
        // pipeline/native source and retained-byte budgets unchanged: a two-row
        // source can now exercise a partial body without exceeding those caps.
        let original_batch_size = self.batch_size;
        self.batch_size = 1;
        ensure!(partial.len() > self.batch_size && newer_bad.len() > self.batch_size);
        let expected_pending = partial.len(); // cached row plus uncached prefix
        let bad_rows = newer_bad.len() as u64;
        let parent = self.node.controller.parent();
        let head = self.node.controller.head();
        let executed = self.node.controller.executed();
        let position = (
            self.node.controller.context().height,
            self.node.controller.round(),
        );
        let scope = TransactionsScope {
            chain_id: self.node.template.chain_id,
            genesis: self.node.template.genesis_config_commitment,
            protocol: self.node.template.protocol_commitment,
            epoch: 1,
            validator_set_hash: self.node.validators.hash(),
            session: [8; 32],
            sequence: 1,
        };
        let message = |batch| Arc::new(Message::UcaTransactions { scope, batch });
        let deadline = Instant::now() + Duration::from_secs(30);
        self.incoming_from_client = false;
        self.incoming = Some((message(cached), 0));
        while self.incoming.is_some() || self.admission.has_pending_native_admission() {
            ensure!(
                Instant::now() < deadline,
                "cached prefix admission timed out"
            );
            self.poll_admission()?;
            std::thread::yield_now();
        }
        ensure!(self.pending.len() == 1, "real cached row not admitted");

        self.incoming = Some((message(partial), 0));
        while self.admission.uca.is_none() {
            ensure!(
                Instant::now() < deadline,
                "partial body signature timed out: {}",
                self.admission.status()
            );
            self.poll_admission()?;
            std::thread::yield_now();
        }
        ensure!(self.admission.cached_body_boundary_waiting);
        ensure!(
            !self.ingress_batch_boundary,
            "cached tail released unfinished prefix"
        );
        ensure!(self.incoming.is_none(), "cached final chunk not consumed");
        let newer = message(newer_bad);
        self.incoming = Some((newer.clone(), 0));
        self.admission.work = self.prepare_gossip_admission()?;
        ensure!(self.admission.work.is_some(), "next signature work missing");
        self.admission.record_retained_stages();
        // First bulk capture poll submits its read and yields. The original
        // next Work is then submitted through its ordinary compute owner.
        self.poll_admission()?;
        ensure!(self.admission.next_signature_submitted_while_capture > 0);
        while self.admission.uca.is_some() {
            ensure!(Instant::now() < deadline, "old authority prefix timed out");
            self.poll_uca_admission()?;
            std::thread::yield_now();
        }
        let (remaining, index) = self
            .incoming
            .as_ref()
            .context("older completion lost next body")?;
        ensure!(Arc::ptr_eq(remaining, &newer) && *index == self.batch_size);
        ensure!(self.ingress_batch_boundary && !self.admission.cached_body_boundary_waiting);
        ensure!(self.admission.work.is_some(), "next output lost its slot");
        let rejected_before = self.gossip_rejected;
        while self.incoming.is_some() || self.admission.has_pending_native_admission() {
            ensure!(
                Instant::now() < deadline,
                "bad next body admission timed out"
            );
            self.poll_admission()?;
            std::thread::yield_now();
        }
        ensure!(
            self.pending.len() == expected_pending,
            "bad signature admitted or old rows lost"
        );
        ensure!(self.gossip_rejected >= rejected_before + bad_rows);
        ensure!(self.admission.max_retained_stage_slots == 2);
        ensure!(!self.admission.cached_body_boundary_waiting);
        ensure!(
            self.node.controller.parent() == parent,
            "admission published state"
        );
        self.batch_size = original_batch_size;
        ensure!(
            self.node.controller.head() == head,
            "admission changed durable head"
        );
        ensure!(
            self.node.controller.executed() == executed,
            "admission executed a candidate"
        );
        ensure!(
            (
                self.node.controller.context().height,
                self.node.controller.round()
            ) == position,
            "admission advanced consensus position"
        );
        Ok(())
    }

    /// Unit-level injection at the existing bounded consumer, not a claim of
    /// native wire delivery. Called with a real initialized original service.
    #[cfg(test)]
    pub(crate) fn assert_uca_legacy_asset_rejection_for_test(
        &mut self,
        nuas: Vec<u8>,
    ) -> Result<()> {
        use crate::native_pipeline::consensus::transport::TransactionsScope;
        let raw = crate::native_pipeline::service::tests::signed_raw(77, 0);
        let inputs = vec![raw.clone(), nuas];
        let before = self.node.controller.parent();
        for input in &inputs {
            assert!(self
                .cached_admission(&AdmissionInput::Raw(input.clone()), true)
                .is_err());
            let request = json!({"jsonrpc":"2.0", "id":1, "method":"nov_sendRawTransaction", "params":[hex(input)]});
            let reply = match DeferredRpcHandler::start(self, request) {
                DeferredRpcReply::Ready(reply) => reply,
                DeferredRpcReply::Pending(token) => {
                    self.poll_admission()?;
                    DeferredRpcHandler::poll(self, token)
                        .context("closed admission did not complete")?
                }
            };
            assert!(reply.get("error").is_some());
            assert!(reply.get("result").is_none());
        }
        let scope = TransactionsScope {
            chain_id: self.node.template.chain_id,
            genesis: self.node.template.genesis_config_commitment,
            protocol: self.node.template.protocol_commitment,
            epoch: 1,
            validator_set_hash: self.node.validators.hash(),
            session: [9; 32],
            sequence: 1,
        };
        for message in [
            Message::Transactions {
                scope,
                raw_transactions: inputs,
            },
            Message::ApflTransactions {
                scope,
                batch: apfl_batch(std::slice::from_ref(&raw), 2)?,
            },
        ] {
            self.incoming = Some((Arc::new(message), 0));
            self.incoming_from_client = true;
            assert!(self.prepare_gossip_admission()?.is_none());
            assert!(
                self.incoming.is_none(),
                "rejected body retained the bounded input slot"
            );
        }
        let body = Arc::new(Message::Body {
            context: self.node.template,
            raw_transactions: vec![raw],
        });
        assert!(self.node.controller.offer(&body).is_err());
        // Rejecting bad input does not freeze or abort the lifecycle.
        self.poll(std::time::Instant::now())?;
        assert_eq!(self.gossip_rejected, 3);
        assert_eq!(self.gossip_verified, 0);
        assert!(
            self.pending.is_empty() && self.reservations.is_empty() && self.receipts.is_empty()
        );
        assert!(self.gossip_order.is_empty() && self.gossip_queued.is_empty());
        assert!(self.admission.work.is_none() && self.admission.http.is_empty());
        assert_eq!(self.admission.submitted_batches, 0);
        assert_eq!(self.node.pipeline.signature_counts(), (0, 0));
        assert_eq!(self.node.controller.executed(), 0);
        assert_eq!(self.node.controller.parent(), before);
        assert!(self.node.controller.head().is_none());
        Ok(())
    }

    /// Exact pending-byte reuse only. Canonical transaction hashes omit the
    /// signature, so a hash hit by itself NEVER skips cryptographic admission.
    fn cached_admission(
        &mut self,
        input: &AdmissionInput,
        broadcast: bool,
    ) -> Result<Option<Hash>> {
        if matches!(input, AdmissionInput::Uca { .. }) {
            return self.cached_uca(input, broadcast);
        }
        ensure!(
            self.node.template.business_program == crate::native_pipeline::business::nov_transfer_batch::program_id(),
            "UCA assets require explicit native NUAS structure; legacy V3/raw carriers are unavailable"
        );
        ensure!(
            self.projection_error.is_none(),
            "RPC projection unavailable"
        );
        let view = match input {
            AdmissionInput::Raw(raw) => decode_transfer_view_v3(raw, 1024)?,
            AdmissionInput::Apfl { batch, index } => batch.row(*index)?,
            AdmissionInput::Uca { .. } => unreachable!("handled above"),
        };
        let hash = view.canonical_tx_hash()?;
        let exact = match self.pending.get(&hash) {
            Some(entry) => entry.matches(view)?,
            None => false,
        };
        if !exact {
            return Ok(None);
        }
        self.authentication_cache_hits = self.authentication_cache_hits.saturating_add(1);
        if broadcast {
            self.queue_gossip(hash);
        }
        Ok(Some(hash))
    }

    pub(super) fn poll_admission(&mut self) -> Result<()> {
        self.admission.made_progress = false;
        self.ingress_batch_boundary = false;
        if self.admission.uca.is_some() {
            self.poll_uca_admission()?;
        }
        if let Some(mut work) = self.admission.work.take() {
            match work.phase {
                Phase::Queued(request) => {
                    match self.node.pipeline.try_admit_signatures_owned(request) {
                        Ok(SignatureAdmissionSubmission::Accepted(ticket)) => {
                            self.admission.made_progress = true;
                            self.admission.submitted_batches += 1;
                            if self.admission.uca.is_some() {
                                self.admission.next_signature_submitted_while_capture = self
                                    .admission
                                    .next_signature_submitted_while_capture
                                    .saturating_add(1);
                            }
                            work.phase = Phase::Checking(ticket);
                            self.admission.work = Some(work);
                        }
                        Ok(SignatureAdmissionSubmission::Backpressured(request)) => {
                            self.admission.backpressured_polls += 1;
                            work.phase = Phase::Queued(request);
                            self.admission.work = Some(work);
                        }
                        Err(rejected) => {
                            self.admission.made_progress = true;
                            work.phase = Phase::Completed(Err(rejected.error));
                            self.admission.work = Some(work);
                        }
                    }
                }
                Phase::Checking(mut ticket) => match ticket.try_take() {
                    Ok(None) => {
                        self.admission.control_polls_while_checking += 1;
                        work.phase = Phase::Checking(ticket);
                        self.admission.work = Some(work);
                    }
                    output => {
                        self.admission.made_progress = true;
                        self.admission.completed_batches += 1;
                        if self.admission.uca.is_some() {
                            self.admission.signature_completed_while_capture = self
                                .admission
                                .signature_completed_while_capture
                                .saturating_add(1);
                        }
                        crate::native_pipeline::profile::critical_event(
                            crate::native_pipeline::profile::CriticalEvent::SignatureBatchDone,
                            self.node.controller.context().height,
                            self.node.controller.round(),
                            None,
                            None,
                        );
                        work.phase = Phase::Completed(output.map(Option::unwrap));
                        self.admission.work = Some(work);
                    }
                },
                Phase::Completed(output) => {
                    work.phase = Phase::Completed(output);
                    self.admission.work = Some(work);
                }
            }
        }
        // Only the oldest output can enter authority capture. The one next
        // signature job may finish meanwhile, but retains its original active
        // ingress credit/bytes until this FIFO handoff is possible.
        if self.admission.uca.is_none()
            && self
                .admission
                .work
                .as_ref()
                .is_some_and(|work| matches!(work.phase, Phase::Completed(_)))
        {
            let work = self.admission.work.take().expect("completed admission");
            let Phase::Completed(output) = work.phase else {
                unreachable!("completed phase checked")
            };
            self.finish_admission(work.target, work.rows, output)?;
        }
        if self.admission.work.is_some() {
            return Ok(());
        }
        // Historical V3 remains single-stage. UCA alone may overlap its one
        // authority capture with one next job in the SAME compute/I/O owners.
        if self.admission.uca.is_some() && !self.is_uca() {
            return Ok(());
        }
        // Alternate bounded sources; a continuous HTTP or peer stream cannot
        // monopolize the one admission graph. Current candidate work retains
        // its own reserved pipeline slots and is never cancelled here.
        let gossip_first = self.admission.prefer_gossip;
        let first = if gossip_first {
            self.prepare_gossip_admission()?
        } else {
            self.prepare_http_admission()?
        };
        let work = match first {
            Some(work) => Some(work),
            None if gossip_first => self.prepare_http_admission()?,
            None => self.prepare_gossip_admission()?,
        };
        if let Some(work) = work {
            self.admission.made_progress = true;
            self.admission.prefer_gossip = matches!(work.target, Target::Http { .. });
            self.admission.work = Some(work);
            self.admission.record_retained_stages();
        }
        Ok(())
    }

    fn prepare_http_admission(&mut self) -> Result<Option<Work>> {
        let mut tokens: Vec<_> = self.admission.http.keys().copied().collect();
        tokens.sort_by_key(|token| (*token <= self.admission.last_http, *token));
        for token in tokens {
            let mut work = self
                .admission
                .http
                .remove(&token)
                .expect("HTTP work present");
            if work.waiting {
                self.admission.http.insert(token, work);
                continue;
            }
            let mut inputs = Vec::new();
            let mut slots = Vec::new();
            let mut bytes = 0usize;
            let mut count = 0usize;
            while let Some(request) = work.requests.pop_front() {
                self.admission.made_progress = true;
                if request["method"] != "nov_sendRawTransaction" {
                    if !slots.is_empty() {
                        work.requests.push_front(request);
                        break;
                    }
                    work.replies.push(self.handle(request));
                    continue;
                }
                if count == self.batch_size {
                    work.requests.push_front(request);
                    break;
                }
                let id = request.get("id").cloned().unwrap_or(Value::Null);
                let parsed = (|| {
                    ensure!(
                        request["jsonrpc"] == "2.0"
                            && (id.is_null() || id.is_string() || id.is_number()),
                        "invalid JSON-RPC request"
                    );
                    one_hex_param(&request, 1024)
                })();
                match parsed {
                    Err(err) => slots.push(ResponseSlot::Ready(error(id, &err.to_string()))),
                    Ok(raw) => {
                        if bytes + raw.len() > super::super::body_byte_limit(self.batch_size) {
                            work.requests.push_front(request);
                            break;
                        }
                        bytes += raw.len();
                        let input = AdmissionInput::Raw(raw);
                        match self.cached_admission(&input, true) {
                            Ok(Some(hash)) => slots.push(ResponseSlot::Ready(response(
                                id,
                                Ok(self.receipt_status_json(hash)),
                            ))),
                            Err(err) => slots.push(ResponseSlot::Ready(response(id, Err(err)))),
                            Ok(None) => {
                                inputs.push(input);
                                slots.push(ResponseSlot::Checked(id));
                            }
                        }
                    }
                }
                count += 1;
            }
            let rows = inputs.len();
            if rows == 0 {
                work.replies
                    .extend(slots.into_iter().map(|slot| match slot {
                        ResponseSlot::Ready(value) => value,
                        ResponseSlot::Checked(_) => unreachable!("no native rows"),
                    }));
                self.admission.http.insert(token, work);
                continue;
            }
            let request = SignatureAdmissionRequest::new(inputs)?;
            work.waiting = true;
            self.admission.http.insert(token, work);
            self.admission.last_http = token;
            return Ok(Some(Work {
                target: Target::Http { token, slots },
                phase: Phase::Queued(request),
                rows,
            }));
        }
        Ok(None)
    }

    fn prepare_gossip_admission(&mut self) -> Result<Option<Work>> {
        if self.projection_error.is_some() {
            return Ok(None);
        }
        if self.incoming.is_none() {
            let source = if self.admission.prefer_client {
                self.node
                    .controller
                    .take_client_transactions()
                    .map(|m| (m, true))
                    .or_else(|| self.node.controller.take_transactions().map(|m| (m, false)))
            } else {
                self.node
                    .controller
                    .take_transactions()
                    .map(|m| (m, false))
                    .or_else(|| {
                        self.node
                            .controller
                            .take_client_transactions()
                            .map(|m| (m, true))
                    })
            };
            if let Some((message, client)) = source {
                self.incoming_from_client = client;
                self.admission.prefer_client = !client;
                self.incoming = Some((message, 0));
            }
        }
        let Some((message, mut index)) = self.incoming.take() else {
            return Ok(None);
        };
        let count = match message.as_ref() {
            Message::Transactions {
                raw_transactions, ..
            } => raw_transactions.len(),
            Message::ApflTransactions { batch, .. } => batch.len(),
            Message::UcaTransactions { batch, .. } => batch.len(),
            _ => anyhow::bail!("transaction channel returned non-transaction message"),
        };
        let mut inputs = Vec::new();
        let end = (index + self.batch_size).min(count);
        while index < end {
            self.admission.made_progress = true;
            let input = match message.as_ref() {
                Message::Transactions {
                    raw_transactions, ..
                } => AdmissionInput::Raw(raw_transactions[index].clone()),
                Message::ApflTransactions { batch, .. } => AdmissionInput::Apfl {
                    batch: batch.clone(),
                    index,
                },
                Message::UcaTransactions { batch, .. } => AdmissionInput::Uca {
                    batch: batch.clone(),
                    index,
                },
                _ => unreachable!(),
            };
            index += 1;
            match self.cached_admission(&input, self.incoming_from_client) {
                Ok(Some(_)) => self.gossip_verified += 1,
                Err(_) => self.gossip_rejected += 1,
                Ok(None) => inputs.push(input),
            }
        }
        let complete_body = index == count;
        let detached_body = self.is_uca() && complete_body;
        // Retain the body marker until the whole verified prefix is admitted;
        // a partially checked body must not cause tiny proposal fragments. A
        // full UCA source moves to its work; the admission-state selection gate
        // retains its boundary while the next source can begin verification.
        if !detached_body {
            self.incoming = Some((message, index));
        }
        if inputs.is_empty() {
            if complete_body {
                self.incoming = None;
                if self.is_uca() {
                    // This can be the cached-only final chunk of a body larger
                    // than batch_size. Its earlier authority prefix must finish
                    // before this boundary becomes eligible for selection.
                    self.ingress_batch_boundary |= self.admission.cached_body_completed();
                } else {
                    self.ingress_batch_boundary = true;
                }
            }
            return Ok(None);
        }
        let rows = inputs.len();
        Ok(Some(Work {
            target: Target::Gossip {
                complete_body,
                detached_body,
                broadcast: self.incoming_from_client,
            },
            phase: Phase::Queued(SignatureAdmissionRequest::new(inputs)?),
            rows,
        }))
    }

    fn finish_admission(
        &mut self,
        target: Target,
        rows: usize,
        output: Result<crate::native_pipeline::pipeline::SignatureAdmissionOutput>,
    ) -> Result<()> {
        if let Ok(output) = &output {
            ensure!(
                output.len() == rows,
                "signature admission result count mismatch"
            );
        }
        let output = match (self.is_uca(), &target, output) {
            (
                true,
                Target::Gossip {
                    complete_body,
                    detached_body,
                    broadcast,
                },
                Ok(mut output),
            ) => {
                ensure!(rows <= self.batch_size, "UCA admission batch limit");
                ensure!(self.admission.uca.is_none(), "UCA authority slot occupied");
                output.park_for_authority()?;
                let mut checked_rows = Vec::with_capacity(rows);
                while let Some(row) = output.pop_admitted() {
                    self.admission.checked_rows += 1;
                    // Reject malformed rows locally. Valid rows preserve their
                    // original relative order and the same output permit.
                    match row.result.and_then(|checked| {
                        uca_capture_input(&row.input, &checked)?;
                        Ok(checked)
                    }) {
                        Ok(checked) => checked_rows.push((row.input, checked)),
                        Err(_) => {
                            self.admission.rejected_rows += 1;
                            self.gossip_rejected += 1;
                        }
                    }
                }
                self.admission.uca = Some(UcaWork {
                    _output: output,
                    rows: checked_rows,
                    read: None,
                    parent: None,
                    capture_started: None,
                    complete_body: *complete_body,
                    detached_body: *detached_body,
                    broadcast: *broadcast,
                });
                self.admission.record_retained_stages();
                return Ok(());
            }
            (_, _, output) => output,
        };
        let mut results = VecDeque::with_capacity(rows);
        match output {
            Ok(mut output) => {
                while let Some(row) = output.pop_admitted() {
                    self.admission.checked_rows += 1;
                    if row.result.is_err() {
                        self.admission.rejected_rows += 1;
                    }
                    results.push_back(row.result.map(|checked| (row.input, checked)));
                }
            }
            Err(err) => {
                for _ in 0..rows {
                    results.push_back(Err(anyhow::anyhow!("signature owner failed: {err:#}")));
                }
            }
        }
        match target {
            Target::Http { token, slots } => {
                if let Some(mut work) = self.admission.http.remove(&token) {
                    for slot in slots {
                        work.replies.push(match slot {
                            ResponseSlot::Ready(value) => value,
                            ResponseSlot::Checked(id) => response(
                                id,
                                results.pop_front().expect("count checked").and_then(
                                    |(input, checked)| {
                                        self.admit_checked(input, checked, true)
                                            .map(|hash| self.receipt_status_json(hash))
                                    },
                                ),
                            ),
                        });
                    }
                    work.waiting = false;
                    self.admission.http.insert(token, work);
                }
            }
            Target::Gossip {
                complete_body,
                detached_body,
                broadcast,
            } => {
                for result in results {
                    match result
                        .and_then(|(input, checked)| self.admit_checked(input, checked, broadcast))
                    {
                        Ok(_) => self.gossip_verified += 1,
                        Err(_) => self.gossip_rejected += 1,
                    }
                }
                if complete_body {
                    if !detached_body {
                        self.incoming = None;
                    }
                    self.ingress_batch_boundary = true;
                }
            }
        }
        Ok(())
    }

    fn poll_uca_admission(&mut self) -> Result<()> {
        let mut work = self.admission.uca.take().expect("pending UCA work");
        if work.rows.is_empty() {
            self.finish_uca_admission(work);
            return Ok(());
        }
        let parent = self.node.controller.parent();
        if work.parent != Some(parent) {
            if let Some(read) = work.read.take() {
                self.admission
                    .uca_capture
                    .record(&read, work.capture_started.take());
                self.admission.uca_capture.parent_restarts =
                    self.admission.uca_capture.parent_restarts.saturating_add(1);
            }
            work.parent = Some(parent);
        }
        let result = (|| {
            if work.read.is_none() {
                work.capture_started = crate::native_pipeline::profile::mark();
                let inputs = work
                    .rows
                    .iter()
                    .map(|(input, checked)| uca_capture_input(input, checked))
                    .collect::<Result<Vec<_>>>()?;
                work.read = Some(native_query::UcaBatchStateRead::new(
                    parent.state_root,
                    inputs,
                    self.node.pipeline.capture_budget(),
                )?);
            }
            work.read
                .as_mut()
                .expect("UCA batch capture")
                .poll(&self.node.pipeline)
        })();
        self.admission.made_progress |= work.read.as_ref().is_some_and(|read| read.made_progress());
        let states = match result {
            Ok(None) => {
                self.admission.uca = Some(work);
                return Ok(());
            }
            Ok(Some(states)) if states.len() == work.rows.len() => Some(states),
            // Missing/corrupt I/O or an invalid result shape rejects this
            // entire snapshot, never fabricates absent accounts or a prefix.
            _ => None,
        };
        if let Some(read) = work.read.take() {
            self.admission
                .uca_capture
                .record(&read, work.capture_started.take());
        }
        // The control-owned parent cannot change during this bounded in-memory
        // loop. No I/O or state consumption occurs here. Original nonce/pool
        // reservations are applied in input order by the unchanged checker;
        // final grant/balance/fee consumption remains the candidate plan's job.
        let mut states = states.map(Vec::into_iter);
        for (input, checked) in work.rows.drain(..) {
            let state = states.as_mut().and_then(Iterator::next);
            let admitted = match state {
                Some(Ok(native_query::UcaReadResult::Present(state))) => {
                    self.admit_uca_checked(input, checked, &state, work.broadcast)
                }
                _ => Err(anyhow::anyhow!("UCA authoritative input unavailable")),
            };
            if admitted.is_ok() {
                self.gossip_verified += 1;
            } else {
                self.gossip_rejected += 1;
                self.admission.rejected_rows += 1;
            }
        }
        self.finish_uca_admission(work);
        Ok(())
    }

    fn finish_uca_admission(&mut self, work: UcaWork) {
        if self.admission.authority_body_completed(work.complete_body) {
            if !work.detached_body && work.complete_body {
                self.incoming = None;
            }
            self.ingress_batch_boundary = true;
        }
        crate::native_pipeline::profile::critical_event(
            crate::native_pipeline::profile::CriticalEvent::AuthorityAdmissionDone,
            self.node.controller.context().height,
            self.node.controller.round(),
            None,
            None,
        );
        self.admission.made_progress = true;
    }
}

fn uca_capture_input(
    input: &AdmissionInput,
    checked: &AdmittedTransfer,
) -> Result<(
    novovm_protocol::uca_spend::UcaSpendIntentV1,
    Account,
    Account,
)> {
    let intent = checked
        .checked_uca()
        .context("UCA signature type")?
        .intent()
        .clone();
    let AdmissionInput::Uca { batch, index } = input else {
        anyhow::bail!("UCA source type");
    };
    ensure!(
        checked.matches_uca_source(batch, *index),
        "UCA source capability mismatch"
    );
    let (payer, recipient) = batch.account_hints(*index)?;
    let payer = Account::try_from(payer).map_err(anyhow::Error::msg)?;
    let recipient = Account::try_from(recipient).map_err(anyhow::Error::msg)?;
    Ok((intent, payer, recipient))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completed_failure() -> Work {
        Work {
            target: Target::Gossip {
                complete_body: true,
                detached_body: true,
                broadcast: false,
            },
            phase: Phase::Completed(Err(anyhow::anyhow!("injected signature owner error"))),
            rows: 1,
        }
    }

    #[test]
    fn completed_signature_error_retains_the_existing_work_slot_and_selection_gate() {
        let mut state = AdmissionState {
            work: Some(completed_failure()),
            ..AdmissionState::default()
        };
        state.record_retained_stages();
        assert!(state.has_pending_native_admission());
        assert_eq!(
            state.status()["signature_completed_waiting_for_authority"],
            true
        );
        assert_eq!(state.status()["max_retained_stage_slots"], 1);
        assert_eq!(state.status()["next_signature_submitted_while_capture"], 0);
        assert_eq!(state.status()["signature_completed_while_capture"], 0);
        let work = state.work.take().unwrap();
        assert!(matches!(work.phase, Phase::Completed(Err(_))));
        assert!(!state.has_pending_native_admission());
    }

    #[test]
    fn cached_final_chunk_does_not_release_an_older_unfinished_authority_prefix() {
        let mut state = AdmissionState {
            work: Some(completed_failure()),
            ..AdmissionState::default()
        };
        // Model an older pending stage without fabricating a checked signature
        // or permission. The boundary rule depends on stage ownership only.
        assert!(!state.cached_body_completed());
        assert!(state.cached_body_boundary_waiting);
        assert!(!state.cached_body_completed());
        drop(state.work.take());
        // An authority prefix whose final chunk was entirely cached now closes
        // the same finite source body, exactly once.
        assert!(state.authority_body_completed(false));
        assert!(!state.authority_body_completed(false));
        assert!(state.cached_body_completed());
        assert!(state.authority_body_completed(true));
    }

    #[test]
    fn uca_capture_counters_accumulate_and_saturate_without_clock_samples() {
        let stats = native_query::UcaCaptureStats {
            declaration_keys: 74,
            unique_keys: 37,
            io_requests: 2,
            io_nodes: 64,
        };
        let mut totals = UcaCaptureTotals::default();
        totals.record_stats(stats, None);
        totals.record_stats(stats, None);
        assert_eq!(totals.batches, 2);
        assert_eq!(totals.declaration_keys, 148);
        assert_eq!(totals.unique_keys, 74);
        assert_eq!(totals.io_requests, 4);
        assert_eq!(totals.io_nodes, 128);
        assert_eq!(totals.elapsed_us, 0);
        assert_eq!(totals.max_elapsed_us, 0);

        totals.batches = u64::MAX;
        totals.declaration_keys = u64::MAX;
        totals.unique_keys = u64::MAX;
        totals.io_requests = u64::MAX;
        totals.io_nodes = u64::MAX;
        totals.record_stats(stats, None);
        assert_eq!(totals.batches, u64::MAX);
        assert_eq!(totals.declaration_keys, u64::MAX);
        assert_eq!(totals.unique_keys, u64::MAX);
        assert_eq!(totals.io_requests, u64::MAX);
        assert_eq!(totals.io_nodes, u64::MAX);
    }

    #[test]
    fn uca_capture_status_starts_empty_and_reports_diagnostic_clock_policy() {
        let state = AdmissionState::default();
        let status = state.status();
        let capture = &status["uca_state_capture"];
        for name in [
            "batches",
            "declared_keys",
            "unique_keys",
            "io_requests",
            "io_nodes",
            "parent_restarts",
            "elapsed_us",
            "max_elapsed_us",
        ] {
            assert_eq!(capture[name], 0, "unexpected initial {name}");
        }
        assert_eq!(
            capture["timing_enabled"],
            crate::native_pipeline::profile::enabled()
        );
        assert_eq!(status["uca_authority_pending"], false);
    }
}
