//! Bounded handoff between the main consensus owner and isolated preparation.
//! No workspace lock, signer, pool, transport or authority handle is sent to
//! the worker. It returns data, not permission to register or vote.
#[cfg(test)]
#[path = "native_fresh_candidate_pipeline_tests.rs"]
mod tests;

use super::*;
use crate::native_block_seal::newview::NovNativeSealNewViewCertificateV1;
use crate::native_block_seal::service_config::{
    FreshSuccessorPreparationV1, SuccessorOutputMismatch,
};
use crate::tx_ingress::candidate_workspace as workspace;

pub(super) enum CandidateOrigin {
    Local {
        certificate: Option<NovNativeSealNewViewCertificateV1>,
    },
    Received {
        certificate: Option<NovNativeSealNewViewCertificateV1>,
        manifest: Box<ProductMainlineOverlayInboundV1>,
    },
}

pub(super) struct PreparingCandidate {
    preparation: FreshSuccessorPreparationV1,
    round: u64,
    origin: CandidateOrigin,
    execution_ready: bool,
    durability: Option<DurabilityStage>,
}

type DurabilityTask = Box<dyn FnOnce() -> Result<workspace::ExecutionInfoV1> + Send>;

enum DurabilityStage {
    Waiting(DurabilityTask),
    Running(novovm_exec::AoemSemanticGraphStageHandleV1<workspace::ExecutionInfoV1>),
}

impl PreparingCandidate {
    pub(super) fn storage_busy(&self) -> bool {
        self.durability.is_some()
    }

    pub(super) fn completion_ready(&self) -> bool {
        match &self.durability {
            Some(DurabilityStage::Running(handle)) => handle.is_ready(),
            // A full owner queue is backpressure, not a request for busy-spin.
            Some(DurabilityStage::Waiting(_)) => false,
            // A consumed durable result still needs the next normal parent/
            // round fence before registration; do not pay another idle period.
            None => self.execution_ready,
        }
    }
}

impl FreshChainLifecycleV1 {
    pub(super) fn start_preparing_candidate(
        &mut self,
        mut preparation: FreshSuccessorPreparationV1,
        round: u64,
        origin: CandidateOrigin,
    ) -> Result<()> {
        let worker = self
            .candidate_worker
            .as_mut()
            .context("candidate worker missing")?;
        if self.preparing.is_some() || worker.is_busy() {
            bail!("candidate pipeline already has its single bounded job");
        }
        let execution_ready = match preparation.take_execution()? {
            workspace::ExecutionStartV1::Complete(_) => true,
            workspace::ExecutionStartV1::Job(job) => {
                worker.try_submit(*job)?;
                false
            }
        };
        self.preparing = Some(PreparingCandidate {
            preparation,
            round,
            origin,
            execution_ready,
            durability: None,
        });
        Ok(())
    }

    pub(super) fn poll_preparing_candidate(
        &mut self,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<()> {
        if self.candidate_storage_busy() {
            return self.poll_candidate_durability();
        }
        let pending = self
            .preparing
            .as_ref()
            .context("candidate continuation missing")?;
        let completed = if pending.execution_ready {
            None
        } else {
            let Some(completion) = self
                .candidate_worker
                .as_mut()
                .context("candidate worker missing")?
                .try_complete()?
            else {
                return Ok(());
            };
            Some(completion)
        };
        let mut pending = self
            .preparing
            .take()
            .context("candidate continuation missing")?;
        if completed.as_ref().is_some_and(|completion| {
            completion.workspace_id() != pending.preparation.workspace_id()
                || completion.chain_id() != pending.preparation.chain_id()
        }) {
            bail!("candidate completion identity differs from the admitted job");
        }
        let current = self
            .config
            .as_ref()
            .context("candidate parent configuration missing")?;
        if !pending.preparation.matches_parent(current)
            || !self
                .pacemaker
                .as_ref()
                .is_some_and(|p| p.permits_prepared_round(pending.round))
        {
            // Before durability this discards owned data without a new slot.
            // After durability the isolated workspace remains recoverable,
            // but is never registered/promoted by this stale continuation.
            // In both cases the original durable transaction pool remains.
            self.candidate_stale_completions = self.candidate_stale_completions.saturating_add(1);
            return Ok(());
        }
        if let Some(completion) = completed {
            if let Err(error) =
                crate::native_fresh_timing::measure("candidate.pipeline.check_computed", || {
                    pending
                        .preparation
                        .check_computed(&completion, &self.params)
                })
            {
                if error.is::<SuccessorOutputMismatch>() {
                    self.rejected = self.rejected.saturating_add(1);
                    return Ok(());
                }
                return Err(error);
            }
            // No signer/config/authority handle crosses this boundary. The
            // owner performs the existing locked isolated completion locally,
            // avoiding one synchronous cross-thread message for every KV read.
            // A ready output is still not registration or permission to vote.
            pending.durability = Some(DurabilityStage::Waiting(Box::new(move || {
                workspace::finish_execution_v1(completion)
            })));
            self.preparing = Some(pending);
            return self.poll_candidate_durability();
        }
        let next = match pending.preparation.finish(&self.params) {
            Ok(next) => next,
            Err(error) if error.is::<SuccessorOutputMismatch>() => {
                // A remote signed subject can be wrong. Reject that candidate,
                // but never confuse storage/execution failures with bad input.
                self.rejected = self.rejected.saturating_add(1);
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        let mut service = NovNativeSealServiceV1::open_configured(
            next.clone(),
            &self.ledger_path,
            &self.params,
            runtime,
            now,
        )?;
        let received = match pending.origin {
            CandidateOrigin::Local { certificate } => {
                if let Some(certificate) = &certificate {
                    service.admit_successor_new_view(certificate)?;
                }
                false
            }
            CandidateOrigin::Received {
                certificate,
                manifest,
            } => {
                if let Some(certificate) = &certificate {
                    service.admit_successor_new_view(certificate)?;
                }
                if !service.enqueue(*manifest) {
                    bail!("verified successor proposal was not admitted");
                }
                true
            }
        };
        self.config = Some(next);
        self.service = Some(Box::new(service));
        self.publication = None;
        self.bodies = None;
        self.pacemaker = None;
        self.proposal_window.clear();
        for queue in self.pending.values_mut() {
            queue.clear();
        }
        for queue in self.round_pending.values_mut() {
            queue.clear();
        }
        if received {
            self.received_successors = self.received_successors.saturating_add(1);
        } else {
            self.proposed_successors = self.proposed_successors.saturating_add(1);
        }
        Ok(())
    }

    fn poll_candidate_durability(&mut self) -> Result<()> {
        use novovm_exec::AoemSemanticGraphStageAdmissionV1 as Admission;
        let pending = self
            .preparing
            .as_mut()
            .context("durability continuation missing")?;
        let stage = pending
            .durability
            .take()
            .context("durability stage missing")?;
        let mut handle = match stage {
            DurabilityStage::Waiting(task) => match self
                .candidate_worker
                .as_ref()
                .context("candidate storage client missing")?
                .storage_client()
                .try_stage(task)?
            {
                Admission::Accepted(handle) => handle,
                Admission::Backpressured(task) => {
                    pending.durability = Some(DurabilityStage::Waiting(task));
                    return Ok(());
                }
            },
            DurabilityStage::Running(handle) => handle,
        };
        let Some(info) = handle.try_complete()? else {
            pending.durability = Some(DurabilityStage::Running(handle));
            return Ok(());
        };
        if info.workspace_id != pending.preparation.workspace_id() {
            bail!("durable candidate completion identity differs from admitted job");
        }
        pending.execution_ready = true;
        // Do not register here. The next normal lifecycle pass first applies
        // pending timeout/new-view work, then repeats the parent/round fence.
        // If that fence rejects, the isolated durable candidate is retained
        // for the original bounded retirement/recovery protocol, never promoted.
        Ok(())
    }
}
