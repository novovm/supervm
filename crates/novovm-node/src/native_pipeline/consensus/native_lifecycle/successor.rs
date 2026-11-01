//! Optional execution lookahead on this lifecycle's existing body owner.
use super::*;
pub use crate::native_pipeline::consensus::bodies::SuccessorSelection;
use novovm_protocol::uca_spend_batch::AssetSpendBatchV3;

impl NativeLifecycle {
    /// The ordered input recipe travels on the existing body lane. The full
    /// signed input stays in Lookahead for execution and RequestBody fallback.
    /// Control/archive/request traffic gets its turn before this optional work.
    pub(super) fn fanout_successor(&mut self) -> Result<()> {
        let Some((id, message)) = self.bodies.successor_announcement(&self.progress) else {
            self.successor_outbox = None;
            self.successor_announced = None;
            return Ok(());
        };
        if self
            .successor_outbox
            .as_ref()
            .is_some_and(|(old, _)| *old != id)
        {
            self.successor_outbox = None;
        }
        if self.successor_announced == Some(id) {
            return Ok(());
        }
        if self.successor_outbox.is_none() {
            let Message::AssetBody { context, batch } = message.as_ref() else {
                anyhow::bail!("successor announcement requires signed asset input");
            };
            let reference =
                super::super::transport::uca_body_reference(context, batch, self.codec)?;
            self.successor_outbox = Some((
                id,
                Fanout {
                    prepared: self.channel.prepare(&reference)?,
                    pending: vec![true; self.peers.len()],
                },
            ));
        }
        let (id, mut fanout) = self.successor_outbox.take().expect("successor fanout");
        for peer in 0..self.peers.len() {
            if fanout.pending[peer] && self.enqueue(peer, fanout.prepared.clone()) {
                fanout.pending[peer] = false;
                self.successor_body_enqueued = self.successor_body_enqueued.saturating_add(1);
                self.successor_body_bytes = self
                    .successor_body_bytes
                    .saturating_add(fanout.prepared.byte_len() as u64);
            }
        }
        if fanout.pending.iter().any(|pending| *pending) {
            self.successor_outbox = Some((id, fanout));
        } else {
            self.successor_announced = Some(id);
        }
        Ok(())
    }

    pub(crate) fn take_asset_body_reference(&mut self) -> Result<Option<(Hash, Arc<Message>)>> {
        if !self.pending[1].as_ref().is_some_and(|incoming| {
            matches!(
                incoming.message.as_ref(),
                Message::AssetBodyReference { .. }
            )
        }) {
            return Ok(None);
        }
        let incoming = self.pending[1].take().expect("retained body reference");
        let Message::AssetBodyReference {
            context, body_id, ..
        } = incoming.message.as_ref()
        else {
            unreachable!("retained body reference kind");
        };
        if self.bodies.response(*body_id, &self.progress).is_some() {
            self.body_references_reused += 1;
            return Ok(None);
        }
        let source = self.peers[incoming.peer];
        if !self.bodies.accepts_asset_reference(
            context,
            *body_id,
            source,
            &self.progress,
            &self.validators,
        )? {
            self.rejected_messages += 1;
            return Ok(None);
        }
        if self.requested.iter().any(|((_, id), claim)| {
            claim.is_some_and(|claim| claim.height == context.height) && *id != *body_id
        }) {
            self.rejected_messages += 1;
            return Ok(None);
        }
        Ok(Some((source, incoming.message)))
    }

    pub(crate) fn resolve_asset_body_reference(
        &mut self,
        source: Hash,
        reference: &Arc<Message>,
        batch: Option<Arc<AssetSpendBatchV3>>,
        pipeline: &CandidatePipeline,
    ) -> Result<()> {
        let Message::AssetBodyReference {
            context, body_id, ..
        } = reference.as_ref()
        else {
            anyhow::bail!("asset body reference required");
        };
        if !self.bodies.accepts_asset_reference(
            context,
            *body_id,
            source,
            &self.progress,
            &self.validators,
        )? {
            return Ok(());
        }
        if let Some(batch) = batch {
            // Pool IDs locate intents, not randomized signature bytes. Only
            // the original complete body commitment qualifies this rebinding.
            if super::super::transport::uca_body_id(context, &batch, self.codec)? == *body_id {
                let body = Arc::new(Message::AssetBody {
                    context: *context,
                    batch,
                });
                let accepted = if context.height > self.progress.context().height {
                    self.bodies.offer_remote_successor(
                        &body,
                        source,
                        pipeline,
                        &self.progress,
                        &self.validators,
                    )
                } else {
                    self.bodies.offer(&body, false, &self.progress)
                };
                match accepted {
                    Ok(true) => {
                        self.body_references_reused += 1;
                    }
                    Ok(false) => {}
                    Err(_) => {
                        self.rejected_messages += 1;
                    }
                }
                return Ok(());
            }
        }
        let peer = self
            .peers
            .iter()
            .position(|peer| *peer == source)
            .context("body reference peer missing")?;
        let request = self
            .channel
            .prepare(&Message::RequestBody { body_id: *body_id })?
            .retain_body_request_until(context.height);
        if self.enqueue(peer, request) {
            self.requested.insert((source, *body_id), Some(*context));
            self.body_reference_full_requests += 1;
            self.requested_bodies += 1;
        } else {
            // The request was not handed off. Keep the SAME bounded slot;
            // once admitted, release it before a full reply can arrive.
            ensure!(self.pending[1].is_none(), "body reference handoff occupied");
            self.pending[1] = Some(Incoming {
                peer,
                message: reference.clone(),
                evidence: VerifiedEvidence::None,
            });
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn retain_body_reply_for_test(
        &mut self,
        source: Hash,
        message: Arc<Message>,
    ) -> Result<()> {
        ensure!(
            matches!(message.as_ref(), Message::AssetBody { .. }),
            "asset reply fixture required"
        );
        ensure!(self.pending[1].is_none(), "body fixture handoff occupied");
        let peer = self
            .peers
            .iter()
            .position(|peer| *peer == source)
            .context("unknown fixture source")?;
        let evidence = self.channel.qualify_control_for_test(&message)?;
        self.pending[1] = Some(Incoming {
            peer,
            message,
            evidence,
        });
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn remote_successor_waiting_for_test(&self, id: Hash) -> bool {
        self.pending[1].is_none() && self.bodies.remote_successor_waiting_for_test(id)
    }

    #[cfg(test)]
    pub(crate) fn offer_remote_successor_for_test(
        &mut self,
        message: &Arc<Message>,
        source: Hash,
        pipeline: &CandidatePipeline,
    ) -> Result<bool> {
        self.bodies.offer_remote_successor(
            message,
            source,
            pipeline,
            &self.progress,
            &self.validators,
        )
    }

    pub(crate) fn remote_successor_authentication_input(&self) -> Option<Arc<Message>> {
        self.bodies
            .remote_successor_authentication_input(&self.progress)
    }

    pub(crate) fn reuse_remote_successor_admitted(
        &mut self,
        message: &Arc<Message>,
        pipeline: &CandidatePipeline,
        admitted: &[&crate::native_pipeline::pipeline::AdmittedTransfer],
    ) -> Result<bool> {
        ensure!(!self.is_frozen(), "native lifecycle frozen");
        self.bodies
            .reuse_remote_successor_admitted(message, pipeline, &self.progress, admitted)
    }

    pub fn successor_selection(&self) -> Result<Option<SuccessorSelection>> {
        if self.is_frozen() {
            return Ok(None);
        }
        self.bodies
            .successor_selection(&self.progress, &self.validators)
    }
    pub fn offer_successor(
        &mut self,
        message: &Arc<Message>,
        selection: &SuccessorSelection,
        pipeline: &CandidatePipeline,
    ) -> Result<bool> {
        ensure!(!self.is_frozen(), "native lifecycle frozen");
        self.bodies.offer_successor(
            message,
            selection,
            pipeline,
            &self.progress,
            &self.validators,
        )
    }
    pub(crate) fn offer_successor_admitted(
        &mut self,
        message: &Arc<Message>,
        selection: &SuccessorSelection,
        pipeline: &CandidatePipeline,
        admitted: &[&crate::native_pipeline::pipeline::AdmittedTransfer],
    ) -> Result<bool> {
        ensure!(!self.is_frozen(), "native lifecycle frozen");
        self.bodies.offer_successor_admitted(
            message,
            selection,
            pipeline,
            &self.progress,
            &self.validators,
            admitted,
        )
    }
    pub fn has_successor_target(&self, height: u64) -> bool {
        !self.is_frozen() && self.bodies.has_successor_target(height, &self.progress)
    }
    /// [authentication started, execution started, completed before parent ACK,
    /// promoted after exact ACK, discarded]. Diagnostics only, not wire state.
    pub fn successor_metrics(&self) -> [u64; 5] {
        self.bodies.successor_metrics()
    }
}
