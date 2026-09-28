//! V3 scheduling belongs to the explicit local service tick, never ingress.
use super::*;
use crate::native_block_seal::commit_v3::lifecycle::certificate_envelope;
use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;

impl NovNativeSealServiceV1 {
    pub(super) fn poll_decision_v3(
        &mut self,
        candidate_view: &NovNativeBlockLedgerV1,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<()> {
        if !self.config.decision_v3_enabled {
            return Ok(());
        }
        if self.decision.is_none() {
            let Some(qc) = self.bridge.prepared_qc().cloned() else {
                return Ok(());
            };
            let message = if let Some(stored) = self.store.load_decision_certificate_by_height_v3(
                self.config.chain_id,
                self.config.authority.epoch,
                self.config.height,
            )? {
                // Restart sends the complete archive; no re-signing an old vote.
                certificate_envelope(&self.store, stored)?
            } else {
                let vote = self.store.sign_local_decision_vote_v3(
                    candidate_view,
                    &qc,
                    &self.config.authority.validator_set,
                    &self.config.signer,
                )?;
                let proposal = self
                    .store
                    .load_proposal(qc.proposal_hash)?
                    .context("V3 service prepared proposal missing")?;
                let certificate = self
                    .store
                    .load_local_new_view_admission(
                        qc.subject.chain_id,
                        qc.subject.epoch,
                        qc.subject.height,
                        qc.subject.round,
                    )?
                    .map(|admission| Box::new(admission.certificate));
                Message::DecisionVoteV3 {
                    proposal: Box::new(proposal),
                    qc: Box::new(qc),
                    vote: Box::new(vote),
                    certificate,
                }
            };
            self.decision = Some(NovNativeSealDecisionLoopV3::attach(
                candidate_view,
                &self.store,
                self.config.authority.clone(),
                self.config.local_validator_id,
                message,
                runtime,
                now,
            )?);
        }
        let sent = self
            .decision
            .as_mut()
            .context("V3 service lifecycle missing")?
            .poll(candidate_view, &self.store, runtime, now)?;
        self.sent = self.sent.saturating_add(sent as u64);
        Ok(())
    }
}
