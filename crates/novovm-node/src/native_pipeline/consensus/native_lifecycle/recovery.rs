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
}
impl Cached {
    fn charge(&self) -> usize {
        self.body.byte_len() + self.decision.byte_len()
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
}
impl ArchivePeer {
    #[cfg(test)]
    pub(super) fn charge(&self) -> usize {
        self.cached.as_ref().map_or(0, Cached::charge)
    }
}

impl NativeLifecycle {
    // Cancellation returns the original object via poll_io. Never reclaim an
    // admitted job early or turn its unknown completion into a transport ACK.
    fn retire_peer(&mut self, peer: usize, archive: bool) -> Result<()> {
        let mut index = 0;
        while index < self.deliveries[peer].len() {
            let delivery = &mut self.deliveries[peer][index];
            let retire = if archive {
                delivery.archive
            } else {
                !delivery.archive
                    && delivery
                        .prepared
                        .superseded(self.height, self.round, delivery.height)
            };
            if !retire {
                index += 1;
                continue;
            }
            if delivery.admitted {
                if !delivery.retiring {
                    ensure!(
                        self.channel.cancel_send(peer, &delivery.prepared)?,
                        "retiring delivery lost its original send handle"
                    );
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
            self.retire_peer(peer, false)?;
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
        self.requested.clear();
        Ok(())
    }

    pub(super) fn request_archive(&mut self, peer: usize, requested: VoteContext) -> Result<()> {
        // NativeChannel has checked the domain/set. Keep shape checking explicit
        // before remembering an untrusted peer's requested parent.
        if requested.validate(&self.validators).is_err() {
            self.rejected_messages += 1;
            return Ok(());
        }
        if self.archives[peer].requested != Some(requested) {
            self.retire_peer(peer, true)?;
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
        self.poll_body_response(peer)?;
        if let Some(reading) = &mut self.archives[peer].reading {
            if let Some(result) = reading.read.poll(pipeline)? {
                let context = reading.context;
                self.archives[peer].reading = None;
                if let Some(block) = result {
                    if self.archives[peer].requested == Some(context) {
                        if block.context() != context {
                            // The peer's arbitrary parent is not our decided
                            // prefix. Never synthesize a response for it.
                            self.rejected_messages += 1;
                            self.archives[peer].requested = None;
                        } else {
                            let body = transport::stored_body(block.stored(), self.codec)?;
                            let body_id = transport::candidate_body_id(&body, self.codec)?;
                            let body = self.channel.prepare(&body)?;
                            let decision = self.channel.prepare(&Message::Decision {
                                proposal: block.proposal().clone(),
                                certificate: block.certificate().clone(),
                                body_id,
                            })?;
                            let cached = Cached {
                                context,
                                body_id,
                                body,
                                decision,
                            };
                            // One cache per peer has a separate, explicit bound
                            // of two canonical wire messages. Delivery queues
                            // retain their original independent count/byte cap.
                            ensure!(
                                cached.charge() <= self.codec.message_bytes.saturating_mul(2),
                                "archive exceeds peer cache budget"
                            );
                            self.archives[peer].cached = Some(cached);
                            self.archives[peer].decision_pending = true;
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
        self.archives[peer].reading = Some(Reading {
            context,
            read: ArchiveRead::new(
                context.height,
                head,
                self.progress.context(),
                Arc::new(self.validators.clone()),
            )?,
        });
        Ok(())
    }

    pub(super) fn remember_body_request(&mut self, peer: usize, body_id: Hash) {
        // A hint is not an accepted transaction or durable evidence. Repeated
        // requests coalesce at one exact ID per configured peer; older missing
        // hints can be repeated by the requester's existing bounded retry.
        self.archives[peer].body_request = Some(body_id);
    }

    fn poll_body_response(&mut self, peer: usize) -> Result<()> {
        let Some(body_id) = self.archives[peer].body_request else {
            return Ok(());
        };
        let archive = self.archives[peer]
            .cached
            .as_ref()
            .filter(|c| c.body_id == body_id && Some(c.context) == self.archives[peer].requested)
            .map(|c| c.body.clone());
        let (prepared, from_archive) = if let Some(prepared) = archive {
            (prepared, true)
        } else if let Some(body) = self.bodies.response(body_id, &self.progress) {
            (self.channel.prepare(&body)?, false)
        } else {
            return Ok(());
        };
        if self.enqueue_kind(peer, prepared, from_archive) {
            self.archives[peer].body_request = None;
            self.served_bodies += 1;
        }
        Ok(())
    }
}
