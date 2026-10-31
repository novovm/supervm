//! Representation adapters for the SAME bounded plan/capture lifecycle.
//! UCA carries checked signatures to the existing capture; it gains no parent,
//! signing or publication authority from its input representation.
use super::*;
use crate::native_pipeline::execution::plan::{BoundBatchCapture, OwnedBatchInput};
use crate::native_pipeline::ingress::batch::BatchSource;
use crate::native_pipeline::state::frontier::StateReadSeed;

pub(super) enum BodyInput {
    Reference(SignatureCheckedBatch),
    Uca {
        source: BatchSource,
        metadata: Box<uca_effects::UcaMetadata>,
    },
}
impl BodyInput {
    pub(super) fn bind(
        self,
        context: BatchContext,
        declarations: Vec<DeclaredAccess>,
        budget: PlanBudget,
    ) -> Result<PlanInput> {
        Ok(match self {
            Self::Reference(batch) => {
                PlanInput::Reference(batch.bind(context, declarations, budget)?)
            }
            Self::Uca { source, metadata } => {
                metadata.check_domain(&context)?;
                PlanInput::Uca {
                    plan: BatchPlan::from_source(context, source, declarations, budget)?,
                    metadata,
                }
            }
        })
    }
}

pub(super) enum PlanInput {
    Reference(SignatureCheckedPlan),
    Uca {
        plan: BatchPlan,
        metadata: Box<uca_effects::UcaMetadata>,
    },
}
impl PlanInput {
    pub(super) fn plan(&self) -> &BatchPlan {
        match self {
            Self::Reference(plan) => plan.plan(),
            Self::Uca { plan, .. } => plan,
        }
    }
    pub(super) fn capture(
        self,
        reader: &dyn StateNodeReader,
        budget: CaptureBudget,
    ) -> Result<CapturedInput> {
        Ok(match self {
            Self::Reference(plan) => CapturedInput::Reference(plan.capture(reader, budget)?),
            Self::Uca { plan, metadata } => CapturedInput::Uca {
                input: plan.capture(reader, budget)?,
                metadata,
            },
        })
    }
    pub(super) fn capture_witness(
        self,
        wire: &[u8],
        budget: CaptureBudget,
    ) -> Result<CapturedInput> {
        match self {
            Self::Reference(plan) => Ok(CapturedInput::Reference(
                plan.capture_witness(wire, budget)?,
            )),
            Self::Uca { .. } => anyhow::bail!("UCA proof relation not yet integrated"),
        }
    }
    pub(super) fn begin_capture(self, budget: CaptureBudget) -> Result<CaptureInput> {
        Ok(match self {
            Self::Reference(plan) => CaptureInput::Reference(plan.begin_capture(budget)?),
            Self::Uca { plan, metadata } => CaptureInput::Uca {
                capture: plan.begin_capture(budget)?,
                metadata,
            },
        })
    }
}

pub(super) enum CaptureInput {
    Reference(SignatureCheckedCapture),
    Uca {
        capture: BoundBatchCapture,
        metadata: Box<uca_effects::UcaMetadata>,
    },
}
impl CaptureInput {
    pub(super) fn seed_hits(&self) -> usize {
        match self {
            Self::Reference(v) => v.seed_hits(),
            Self::Uca { capture, .. } => capture.seed_hits(),
        }
    }
    pub(super) fn context(&self) -> &BatchContext {
        match self {
            Self::Reference(v) => v.context(),
            Self::Uca { capture, .. } => capture.context(),
        }
    }
    pub(super) fn advance_with_seed(
        &mut self,
        steps: usize,
        seed: Option<&StateReadSeed>,
    ) -> Result<CaptureStep> {
        match self {
            Self::Reference(v) => v.advance_with_seed(steps, seed),
            Self::Uca { capture, .. } => capture.advance_with_seed(steps, seed),
        }
    }
    pub(super) fn advance(&mut self, steps: usize) -> Result<CaptureStep> {
        self.advance_with_seed(steps, None)
    }
    pub(super) fn next_request(&mut self) -> Result<Option<Vec<NodeHash>>> {
        match self {
            Self::Reference(v) => v.next_request(),
            Self::Uca { capture, .. } => capture.next_request(),
        }
    }
    pub(super) fn accept(&mut self, values: Vec<Option<Vec<u8>>>) -> Result<()> {
        match self {
            Self::Reference(v) => v.accept(values),
            Self::Uca { capture, .. } => capture.accept(values),
        }
    }
    pub(super) fn finish(self) -> Result<CapturedInput> {
        Ok(match self {
            Self::Reference(v) => CapturedInput::Reference(v.finish()?),
            Self::Uca { capture, metadata } => CapturedInput::Uca {
                input: capture.finish()?,
                metadata,
            },
        })
    }
}

pub(super) enum CapturedInput {
    Reference(SignatureCheckedInput),
    Uca {
        input: OwnedBatchInput,
        metadata: Box<uca_effects::UcaMetadata>,
    },
}
impl CapturedInput {
    pub(super) fn plan(&self) -> &BatchPlan {
        match self {
            Self::Reference(v) => v.plan(),
            Self::Uca { input, .. } => input.plan(),
        }
    }
    pub(super) fn encode_witness(&self, budget: CaptureBudget) -> Result<Vec<u8>> {
        match self {
            Self::Reference(v) => v.encode_witness(budget),
            Self::Uca { .. } => anyhow::bail!("UCA proof relation not yet integrated"),
        }
    }
}
