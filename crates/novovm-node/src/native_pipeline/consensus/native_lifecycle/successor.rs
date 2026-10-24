//! Optional local execution lookahead on this lifecycle's existing body owner.
use super::*;
pub use crate::native_pipeline::consensus::bodies::SuccessorSelection;

impl NativeLifecycle {
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
