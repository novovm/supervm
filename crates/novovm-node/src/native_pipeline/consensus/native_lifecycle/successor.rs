//! Optional execution lookahead on this lifecycle's existing body owner.
use super::*;
pub use crate::native_pipeline::consensus::bodies::SuccessorSelection;

impl NativeLifecycle {
    /// The selected input travels in the existing AssetBody lane, once, using
    /// existing per-peer delivery reservations. It is not a proposal or a vote.
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
            self.successor_outbox = Some((
                id,
                Fanout {
                    prepared: self.channel.prepare(&message)?,
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
