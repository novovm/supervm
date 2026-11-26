//! Representation dispatch inside the original bounded candidate lifecycle.
//! Each branch keeps its own typed compiler, state checks and output contract.
#![allow(
    clippy::large_enum_variant,
    reason = "Preserve the bounded asset lifecycle's inline layout without per-phase allocations"
)]
use super::{
    direct_nov_fee::DirectNovFeePolicy, nov_transfer_batch as asset, uca_application as app,
    uca_registration as registration,
};
use crate::native_pipeline::{
    execution::plan::{BatchContext, BatchPlan, BoundBatchCapture, OwnedBatchInput, PlanBudget},
    ingress::batch::SignatureCheckedBatch,
    state::{
        frontier::{CaptureBudget, CaptureStep, StateReadSeed},
        tree::NodeHash,
    },
};
use anyhow::Result;
use novovm_exec::uca_registration::VerifiedUcaRegistrationV1;
use std::sync::Arc;

pub(crate) enum Body {
    Asset(asset::NovTransferBody),
    Application(app::ApplicationGrantBodyV1),
}
impl Body {
    pub fn prepare(
        batch: SignatureCheckedBatch,
        policy: DirectNovFeePolicy,
        budget: PlanBudget,
    ) -> Result<Self> {
        asset::NovTransferBody::prepare(batch, policy, budget).map(Self::Asset)
    }
    pub fn prepare_asset_spend(
        signed: Vec<novovm_exec::uca_spend::SignatureCheckedAssetSpendV3>,
        policy: DirectNovFeePolicy,
        budget: PlanBudget,
    ) -> Result<Self> {
        asset::NovTransferBody::prepare_asset_spend(signed, policy, budget).map(Self::Asset)
    }
    pub fn prepare_asset_spend_from_uca(
        source: Arc<novovm_protocol::uca_spend_batch::AssetSpendBatchV3>,
        signed: Vec<novovm_exec::uca_spend::SignatureCheckedAssetSpendV3>,
        policy: DirectNovFeePolicy,
        budget: PlanBudget,
    ) -> Result<Self> {
        asset::NovTransferBody::prepare_asset_spend_from_uca(source, signed, policy, budget)
            .map(Self::Asset)
    }
    pub fn bind(self, context: BatchContext) -> Result<Plan> {
        match self {
            Self::Asset(body) => body.bind(context).map(Plan::Asset),
            Self::Application(body) => body.bind(context).map(Plan::Application),
        }
    }
}

pub(crate) enum Plan {
    Asset(asset::NovTransferPlan),
    Application(app::ApplicationGrantPlanV1),
    Registration(registration::RegistrationPlan),
}
impl Plan {
    pub fn plan(&self) -> &BatchPlan {
        match self {
            Self::Asset(value) => value.plan(),
            Self::Application(value) => value.plan(),
            Self::Registration(value) => value.plan(),
        }
    }
    pub fn commitment(&self) -> NodeHash {
        self.plan().commitment()
    }
    pub fn begin_capture(self, budget: CaptureBudget) -> Result<Capture> {
        match self {
            Self::Asset(value) => value.begin_capture(budget).map(Capture::Asset),
            Self::Application(value) => value.begin_capture(budget).map(Capture::Application),
            Self::Registration(value) => {
                let (capture, proof) = value.begin_capture(budget)?;
                Ok(Capture::Registration(capture, proof))
            }
        }
    }
}

pub(crate) enum Capture {
    Asset(asset::NovTransferCapture),
    Application(app::ApplicationGrantCaptureV1),
    Registration(BoundBatchCapture, VerifiedUcaRegistrationV1),
}
impl Capture {
    pub fn context(&self) -> &BatchContext {
        match self {
            Self::Asset(value) => value.context(),
            Self::Application(value) => value.context(),
            Self::Registration(capture, _) => capture.context(),
        }
    }
    pub fn seed_hits(&self) -> usize {
        match self {
            Self::Asset(value) => value.seed_hits(),
            Self::Application(value) => value.seed_hits(),
            Self::Registration(capture, _) => capture.seed_hits(),
        }
    }
    pub fn advance_with_seed(
        &mut self,
        steps: usize,
        seed: Option<&StateReadSeed>,
    ) -> Result<CaptureStep> {
        match self {
            Self::Asset(value) => value.advance_with_seed(steps, seed),
            Self::Application(value) => value.advance_with_seed(steps, seed),
            Self::Registration(capture, _) => capture.advance_with_seed(steps, seed),
        }
    }
    pub fn next_request(&mut self) -> Result<Option<Vec<NodeHash>>> {
        match self {
            Self::Asset(value) => value.next_request(),
            Self::Application(value) => value.next_request(),
            Self::Registration(capture, _) => capture.next_request(),
        }
    }
    pub fn accept(&mut self, values: Vec<Option<Vec<u8>>>) -> Result<()> {
        match self {
            Self::Asset(value) => value.accept(values),
            Self::Application(value) => value.accept(values),
            Self::Registration(capture, _) => capture.accept(values),
        }
    }
    pub fn finish(self) -> Result<Captured> {
        match self {
            Self::Asset(value) => value.finish().map(Captured::Asset),
            Self::Application(value) => value.finish().map(Captured::Application),
            Self::Registration(capture, proof) => Ok(Captured::Registration(RegistrationInput {
                input: capture.finish()?,
                proof,
            })),
        }
    }
    #[cfg(test)]
    pub fn advance(&mut self, steps: usize) -> Result<CaptureStep> {
        self.advance_with_seed(steps, None)
    }
}

pub(crate) struct RegistrationInput {
    input: OwnedBatchInput,
    proof: VerifiedUcaRegistrationV1,
}

impl RegistrationInput {
    pub fn execute(self) -> Result<registration::ExecutedRegistration> {
        registration::execute_captured(self.proof, self.input)
    }
}

pub(crate) enum Captured {
    Asset(asset::NovCapturedInput),
    Application(app::ApplicationGrantInputV1),
    Registration(RegistrationInput),
}
impl Captured {
    pub fn plan(&self) -> &BatchPlan {
        match self {
            Self::Asset(value) => value.plan(),
            Self::Application(value) => value.plan(),
            Self::Registration(value) => value.input.plan(),
        }
    }
    pub fn is_asset(&self) -> bool {
        matches!(self, Self::Asset(_))
    }
    pub fn finalize_capture(self) -> Result<asset::NovTransferInput> {
        match self {
            Self::Asset(value) => value.finalize_capture(),
            Self::Application(_) | Self::Registration(_) => {
                anyhow::bail!("candidate is not an asset execution input")
            }
        }
    }
}
