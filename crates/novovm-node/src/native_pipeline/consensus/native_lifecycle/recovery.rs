//! Recovery adapters migrated from the preserved controller's request_archive,
//! poll_archives and request_missing. One bounded read/cache per configured peer;
//! the original RoundProgress/AOEM/journal still validates and applies each block.
use super::*;
use crate::native_pipeline::consensus::{transport, wire::Context as VoteContext, ArchiveRead};

struct Cached {
    context: VoteContext,
    body_id: Hash,
    body: PreparedMessage,
    decision: PreparedMessage,
    precommitters: Arc<[Hash]>,
    body_pending: bool,
}
impl Cached {
    fn charge(&self) -> usize {
        self.body.byte_len()
            + self.decision.byte_len()
            + std::mem::size_of_val(self.precommitters.as_ref())
    }

    fn for_peer(&self) -> Self {
        Self {
            context: self.context,
            body_id: self.body_id,
            body: self.body.clone(),
            decision: self.decision.clone(),
            precommitters: self.precommitters.clone(),
            // Immutable checked wire bytes are shared, never delivery credit.
            // Only THIS peer's actual decision completion arms its body.
            body_pending: false,
        }
    }
}
struct Reading {
    context: VoteContext,
    read: ArchiveRead,
}
#[derive(Default)]
pub(super) struct ArchivePeer {
    requested: Option<VoteContext>,
    reading: Option<Reading>,
    cached: Option<Cached>,
    decision_pending: bool,
    body_request: Option<Hash>,
    // A future body requested before its parent ACK must not lose its reply
    // responsibility when an ordinary archive request refreshes the hint.
    successor_body_request: Option<Hash>,
}
impl ArchivePeer {
    #[cfg(test)]
    pub(super) fn charge(&self) -> usize {
        self.cached.as_ref().map_or(0, Cached::charge)
    }
}

impl NativeLifecycle {
    #[cfg(test)]
    pub(crate) fn defer_archive_discovery_until_for_test(&mut self, due: Instant) {
        self.decision_due = Some(due);
    }

    // Retire the business obligation, not an already-admitted transport send.
    // Local cancellation has no remote reassembly-release message: abandoning
    // a partial object strands the peer's bounded slot until TTL. The SAME
    // sender therefore drains under its ORIGINAL deadline; poll_io returns its
    // real terminal state. Unsent work can be reclaimed immediately.
    fn retire_peer(
        &mut self,
        peer: usize,
        archive: bool,
        keep_archive_height: Option<u64>,
    ) -> Result<()> {
        let mut index = 0;
        while index < self.deliveries[peer].len() {
            let delivery = &mut self.deliveries[peer][index];
            if archive {
                // A requested live body can precede RequestDecision for the
                // SAME height. Preserve it on that first/context refresh.
                if keep_archive_height.is_none_or(|keep| {
                    delivery
                        .prepared
                        .height()
                        .is_none_or(|height| height < keep)
                }) {
                    delivery.archive = false;
                }
            } else if delivery
                .prepared
                .superseded(self.height, self.round, delivery.height)
            {
                delivery.realtime = false;
            }
            if delivery.archive || delivery.realtime {
                index += 1;
                continue;
            }
            if !delivery.retiring {
                crate::native_pipeline::profile::delivery_event(
                    "retired",
                    delivery.prepared.height().unwrap_or(0),
                    self.peers[peer],
                    delivery.prepared.fragment_id(),
                    0,
                );
            }
            if delivery.admitted {
                if !delivery.retiring {
                    delivery.retiring = true;
                    self.retired_sends += 1;
                }
                index += 1;
            } else {
                let done = self.deliveries[peer]
                    .remove(index)
                    .expect("retired queued delivery");
                self.bytes[peer] -= done.prepared.byte_len();
                self.retired_sends += 1;
            }
        }
        Ok(())
    }

    pub(super) fn retire_realtime(&mut self) -> Result<()> {
        for peer in 0..self.peers.len() {
            self.retire_peer(peer, false, None)?;
        }
        Ok(())
    }

    pub(super) fn request_decisions(&mut self, now: Instant) -> Result<()> {
        if self.progress.is_recovering() || self.decision_due.is_some_and(|due| now < due) {
            return Ok(());
        }
        self.decision_due = Some(
            now.checked_add(Duration::from_millis(250))
                .context("request deadline overflow")?,
        );
        // Requests grant no signing or head authority. The exact locally pinned
        // next parent is sufficient; no remote head announcement is trusted.
        let prepared = self.channel.prepare(&Message::RequestDecision {
            context: self.progress.context(),
        })?;
        for peer in 0..self.peers.len() {
            self.enqueue(peer, prepared.clone());
        }
        // A transport DONE may precede the server's archive readiness. Repeat
        // exact missing-body requests under the same bounded periodic budget.
        self.requested.retain(|_, claim| claim.is_some());
        Ok(())
    }

    pub(super) fn request_archive(&mut self, peer: usize, requested: VoteContext) -> Result<()> {
        // NativeChannel has checked the domain/set. Keep shape checking explicit
        // before remembering an untrusted peer's requested parent.
        if requested.validate(&self.validators).is_err() {
            self.rejected_messages += 1;
            return Ok(());
        }
        // Object delivery is not ordered: an admitted request from an older
        // height may finish draining after the next request. Do not let it
        // replace the responsibility already retained for this peer. Exact
        // retries can be transport-deduplicated, so the newer owner must live
        // independently of another application delivery of that request.
        // Requesters derive this context from the ACKed durable journal; an
        // original-database restart cannot legitimately regress its height.
        // This is only the current per-peer hint, not a chain-head authority or
        // permanent watermark. Same-height parents still require ArchiveRead.
        if self.archives[peer]
            .requested
            .is_some_and(|current| requested.height < current.height)
        {
            return Ok(());
        }
        if self.archives[peer].requested != Some(requested) {
            self.retire_peer(peer, true, Some(requested.height))?;
            self.archives[peer].requested = Some(requested);
            self.archives[peer].cached = None;
        }
        if self.archives[peer]
            .cached
            .as_ref()
            .is_some_and(|cache| cache.context == requested)
        {
            self.archives[peer].decision_pending = true;
        }
        Ok(())
    }

    pub(super) fn poll_archives(&mut self, pipeline: &CandidatePipeline) -> Result<()> {
        if self.peers.is_empty() || self.progress.is_recovering() {
            return Ok(());
        }
        let peer = self.archive_turn;
        self.archive_turn = (peer + 1) % self.peers.len();
        if let (Some(context), Some(head)) = (self.archives[peer].requested, self.progress.head()) {
            if context.height <= head.height
                && !self.archives[peer]
                    .cached
                    .as_ref()
                    .is_some_and(|cached| cached.context == context)
            {
                // poll_inner has checked this lifecycle's pipeline owner. The
                // exact context binds chain/genesis/protocol/epoch/set/parents,
                // and the actual ACKed head still bounds the requested height.
                // Acquire BEFORE body response so an existing checked recovery
                // wrapper is not needlessly re-encoded from a live body.
                if let Some(cached) = self.archives.iter().find_map(|state| {
                    state
                        .cached
                        .as_ref()
                        .filter(|cached| cached.context == context)
                        .map(Cached::for_peer)
                }) {
                    self.archives[peer].cached = Some(cached);
                    self.archives[peer].decision_pending = true;
                    self.archive_cache_reused += 1;
                }
            }
        }
        self.poll_body_response(peer)?;
        if let Some(reading) = &mut self.archives[peer].reading {
            if let Some(result) = reading.read.poll(pipeline)? {
                let context = reading.context;
                self.archives[peer].reading = None;
                if let Some(block) = result {
                    if self
                        .archives
                        .iter()
                        .any(|state| state.requested == Some(context))
                    {
                        if block.context() != context {
                            // The peer's arbitrary parent is not our decided
                            // prefix. Never synthesize a response for it.
                            for state in &mut self.archives {
                                if state.requested == Some(context) {
                                    self.rejected_messages += 1;
                                    state.requested = None;
                                }
                            }
                        } else {
                            let body = transport::stored_body(block.stored(), self.codec)?;
                            let body_id = transport::candidate_body_id(&body, self.codec)?;
                            let body = self.channel.prepare_recovery_reply(&body)?;
                            let decision =
                                self.channel.prepare_recovery_reply(&Message::Decision {
                                    proposal: block.proposal().clone(),
                                    certificate: block.certificate().clone(),
                                    body_id,
                                })?;
                            let cached = Cached {
                                context,
                                body_id,
                                body,
                                decision,
                                precommitters: block
                                    .certificate()
                                    .votes
                                    .iter()
                                    .map(|vote| vote.validator_id)
                                    .collect::<Vec<_>>()
                                    .into(),
                                body_pending: false,
                            };
                            // One cache per peer has a separate, explicit bound
                            // of two canonical wire messages. Delivery queues
                            // retain their original independent count/byte cap.
                            ensure!(
                                cached.charge() <= self.codec.message_bytes.saturating_mul(2),
                                "archive exceeds peer cache budget"
                            );
                            // The original requester may already have advanced.
                            // Its accepted read was still drained above; hand
                            // the checked result to every remaining exact-context
                            // waiter, without replacing another peer's ACK state
                            // or abandoning an already accepted read ticket.
                            for (index, state) in self.archives.iter_mut().enumerate() {
                                if state.requested == Some(context)
                                    && !state
                                        .cached
                                        .as_ref()
                                        .is_some_and(|existing| existing.context == context)
                                {
                                    state.cached = Some(cached.for_peer());
                                    state.decision_pending = true;
                                    if index != peer {
                                        self.archive_cache_reused += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if self.archives[peer].decision_pending {
            if let Some(prepared) = self.archives[peer]
                .cached
                .as_ref()
                .map(|c| c.decision.clone())
            {
                if self.enqueue_kind(peer, prepared, true) {
                    self.archives[peer].decision_pending = false;
                    self.archive_decisions += 1;
                }
            }
        }
        self.poll_archive_body(peer);
        let Some(context) = self.archives[peer].requested else {
            return Ok(());
        };
        let Some(head) = self.progress.head() else {
            return Ok(());
        };
        if context.height > head.height
            || self.archives[peer].reading.is_some()
            || self.archives[peer]
                .cached
                .as_ref()
                .is_some_and(|c| c.context == context)
        {
            return Ok(());
        }
        if self.archives.iter().any(|state| {
            state
                .reading
                .as_ref()
                .is_some_and(|reading| reading.context == context)
        }) {
            // No extra queue or ticket: this peer retains its original request,
            // and round-robin polling continues to drain the read's owner even
            // when that owner's requested context changes in the meantime.
            return Ok(());
        }
        self.archives[peer].reading = Some(Reading {
            context,
            read: ArchiveRead::new(
                context.height,
                head,
                self.progress.context(),
                Arc::new(self.validators.clone()),
            )?,
        });
        self.archive_reads_started += 1;
        Ok(())
    }

    pub(super) fn remember_body_request(&mut self, peer: usize, body_id: Hash) {
        // A hint is not an accepted transaction or durable evidence. Repeated
        // requests coalesce at one exact ID per configured peer; older missing
        // hints can be repeated by the requester's existing bounded retry.
        if self
            .bodies
            .successor_response(body_id, &self.progress)
            .is_some()
        {
            self.archives[peer].successor_body_request = Some(body_id);
        } else {
            self.archives[peer].body_request = Some(body_id);
        }
    }

    // Called ONLY for the original sender's actual Delivered completion.
    // The exact checked archive already owns both objects. Once the decision
    // reached the peer, continue its body under the SAME bounded owner rather
    // than rely on an identical RequestBody escaping transport dedup again.
    pub(super) fn archive_delivery_completed(&mut self, peer: usize, fragment: Hash) {
        let state = &mut self.archives[peer];
        let Some(cached) = &mut state.cached else {
            return;
        };
        if state.requested != Some(cached.context) {
            return;
        }
        if fragment == cached.body.fragment_id() {
            cached.body_pending = false;
        } else if fragment == cached.decision.fragment_id() {
            // This exact checked PRECOMMIT proves the peer already persisted
            // the candidate before voting. Decision delivery alone proves no
            // such possession. Explicit RequestBody still serves full bytes,
            // including after a voter restarts and needs its original store.
            if cached.precommitters.contains(&self.peers[peer]) {
                return;
            }
            // Each actual decision completion renews this bounded continuation.
            // Do not cache "body delivered forever" across receiver restarts.
            cached.body_pending = true;
            crate::native_pipeline::profile::delivery_event(
                "archive_body_ready_after_decision_done",
                cached.context.height,
                self.peers[peer],
                cached.body.fragment_id(),
                2,
            );
        }
    }

    fn poll_archive_body(&mut self, peer: usize) {
        let prepared = self.archives[peer].cached.as_ref().and_then(|cached| {
            (cached.body_pending && Some(cached.context) == self.archives[peer].requested)
                .then(|| cached.body.clone())
        });
        if let Some(prepared) = prepared {
            if self.enqueue_kind(peer, prepared, true) {
                self.archives[peer]
                    .cached
                    .as_mut()
                    .expect("retained archive")
                    .body_pending = false;
                self.served_bodies += 1;
            }
        }
    }

    fn poll_body_response(&mut self, peer: usize) -> Result<()> {
        let successor = self.archives[peer].successor_body_request.is_some();
        let Some(body_id) = self.archives[peer]
            .successor_body_request
            .or(self.archives[peer].body_request)
        else {
            return Ok(());
        };
        let archive = self.archives[peer]
            .cached
            .as_ref()
            .filter(|c| c.body_id == body_id && Some(c.context) == self.archives[peer].requested)
            .map(|c| c.body.clone());
        let prepared = if let Some(prepared) = archive {
            prepared
        } else if let Some(body) = self.bodies.response(body_id, &self.progress) {
            self.channel.prepare_recovery_reply(&body)?
        } else {
            if successor {
                // A retired optional fork must not keep this peer's ordinary
                // archive request behind an input this owner no longer holds.
                self.archives[peer].successor_body_request = None;
                return self.poll_body_response(peer);
            }
            return Ok(());
        };
        // Purpose, not the source cache, owns the requested reply. Advancing
        // the local realtime height cannot cancel the peer's recovery work.
        if self.enqueue_kind(peer, prepared, true) {
            if successor {
                self.archives[peer].successor_body_request = None;
            } else {
                self.archives[peer].body_request = None;
            }
            self.served_bodies += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
