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
    },
}

enum Phase {
    Queued(SignatureAdmissionRequest),
    Checking(SignatureAdmissionTicket),
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
    work: Option<Work>,
    submitted_batches: u64,
    completed_batches: u64,
    checked_rows: u64,
    rejected_rows: u64,
    cancelled_http: u64,
    control_polls_while_checking: u64,
    backpressured_polls: u64,
}

impl AdmissionState {
    pub(super) fn status(&self) -> Value {
        json!({"owner":"same_resident_aoem_compute_session",
            "http_waiting":self.http.len(),"signature_batch_pending":self.work.is_some(),
            "submitted_batches":self.submitted_batches,"completed_batches":self.completed_batches,
            "checked_rows":self.checked_rows,"rejected_rows":self.rejected_rows,
            "cancelled_http":self.cancelled_http,
            "control_polls_while_checking":self.control_polls_while_checking,
            "backpressured_polls":self.backpressured_polls,
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
    /// Exact pending-byte reuse only. Canonical transaction hashes omit the
    /// signature, so a hash hit by itself NEVER skips cryptographic admission.
    fn cached_admission(
        &mut self,
        input: &AdmissionInput,
        broadcast: bool,
    ) -> Result<Option<Value>> {
        ensure!(
            self.projection_error.is_none(),
            "RPC projection unavailable"
        );
        let view = match input {
            AdmissionInput::Raw(raw) => decode_transfer_view_v3(raw, 1024)?,
            AdmissionInput::Apfl { batch, index } => batch.row(*index)?,
        };
        let hash = view.canonical_tx_hash()?;
        let exact = match self.pending.get(&hash) {
            Some(entry) => view.matches_canonical_bytes(&entry.raw)?,
            None => false,
        };
        if !exact {
            return Ok(None);
        }
        self.authentication_cache_hits = self.authentication_cache_hits.saturating_add(1);
        if broadcast {
            self.queue_gossip(hash);
        }
        Ok(Some(self.pending_status(hash)))
    }

    pub(super) fn poll_admission(&mut self) -> Result<()> {
        self.admission.made_progress = false;
        self.ingress_batch_boundary = false;
        if let Some(mut work) = self.admission.work.take() {
            match work.phase {
                Phase::Queued(request) => {
                    match self.node.pipeline.try_admit_signatures_owned(request) {
                        Ok(SignatureAdmissionSubmission::Accepted(ticket)) => {
                            self.admission.made_progress = true;
                            self.admission.submitted_batches += 1;
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
                            self.finish_admission(work.target, work.rows, Err(rejected.error))?
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
                        self.finish_admission(work.target, work.rows, output.map(Option::unwrap))?;
                    }
                },
            }
        }
        if self.admission.work.is_some() {
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
                            Ok(Some(value)) => {
                                slots.push(ResponseSlot::Ready(response(id, Ok(value))))
                            }
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
            self.incoming = self
                .node
                .controller
                .take_transactions()
                .map(|r| (r.message, 0));
        }
        let Some((message, mut index)) = self.incoming.take() else {
            return Ok(None);
        };
        let count = match message.as_ref() {
            Message::Transactions {
                raw_transactions, ..
            } => raw_transactions.len(),
            Message::ApflTransactions { batch, .. } => batch.len(),
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
                _ => unreachable!(),
            };
            index += 1;
            match self.cached_admission(&input, false) {
                Ok(Some(_)) => self.gossip_verified += 1,
                Err(_) => self.gossip_rejected += 1,
                Ok(None) => inputs.push(input),
            }
        }
        let complete_body = index == count;
        // Retain the body marker until the whole verified prefix is admitted;
        // a partially checked body must not cause tiny proposal fragments.
        self.incoming = Some((message, index));
        if inputs.is_empty() {
            if complete_body {
                self.incoming = None;
                self.ingress_batch_boundary = true;
            }
            return Ok(None);
        }
        let rows = inputs.len();
        Ok(Some(Work {
            target: Target::Gossip { complete_body },
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
        let mut results = VecDeque::with_capacity(rows);
        match output {
            Ok(mut output) => {
                while let Some(row) = output.pop_front() {
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
                                    |(input, checked)| self.admit_checked(input, checked, true),
                                ),
                            ),
                        });
                    }
                    work.waiting = false;
                    self.admission.http.insert(token, work);
                }
            }
            Target::Gossip { complete_body } => {
                for result in results {
                    match result
                        .and_then(|(input, checked)| self.admit_checked(input, checked, false))
                    {
                        Ok(_) => self.gossip_verified += 1,
                        Err(_) => self.gossip_rejected += 1,
                    }
                }
                if complete_body {
                    self.incoming = None;
                    self.ingress_batch_boundary = true;
                }
            }
        }
        Ok(())
    }
}
