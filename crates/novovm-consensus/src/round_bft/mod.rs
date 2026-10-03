//! Versioned round-BFT evidence and safety rules, migrated from the committed
//! runtime consensus implementation at `2da3583c`.
//!
//! This is not native V3 and does not reinterpret its signatures or certificates.
//! The public entry point verifies decision evidence only. It does not authorize
//! a local signature, storage mutation, candidate publication, or finality.
//! Local execution/DA and the exact durable parent must still be established by
//! the node. Journal/pacemaker and publication integration remain separate.

pub mod collector;
// Preserve all reviewed transitions for their original tests and the later
// durable-journal integration. Only decision checking is used by this slice.
#[allow(dead_code)]
pub(crate) mod round;
pub mod wire;

use anyhow::{ensure, Result};

/// Cryptographically verified decision evidence pinned to a caller's exact
/// context and locally checked value. Neither argument becomes a durability or
/// execution capability by passing this check. No deserializer can manufacture
/// this wrapper, and its fields cannot be changed after verification.
#[derive(Clone, Debug)]
pub struct VerifiedDecision {
    proposal: wire::Proposal,
    certificate: wire::Quorum,
}

impl VerifiedDecision {
    pub fn verify(
        proposal: &wire::Proposal,
        certificate: &wire::Quorum,
        validators: &wire::ValidatorSet,
        expected_context: wire::Context,
        expected_value: wire::Hash,
    ) -> Result<Self> {
        expected_context.validate(validators)?;
        ensure!(
            expected_value != [0; 32]
                && proposal.context == expected_context
                && proposal.value == expected_value,
            "decision differs from expected context or locally checked value"
        );
        let proposal = proposal.verify(validators)?;
        let certificate = certificate.verify(validators)?;
        let step = round::State::new(expected_context)?.prepare_decision(
            &proposal,
            expected_value,
            &certificate,
        )?;
        ensure!(
            step.intent().is_none() && step.decision() == Some(expected_value),
            "decision verification produced unexpected vote or value"
        );
        Ok(Self {
            proposal: proposal.into_proposal(),
            certificate: certificate.into_quorum(),
        })
    }

    pub fn proposal(&self) -> &wire::Proposal {
        &self.proposal
    }

    pub fn certificate(&self) -> &wire::Quorum {
        &self.certificate
    }

    pub fn context(&self) -> &wire::Context {
        &self.proposal.context
    }

    pub fn value(&self) -> wire::Hash {
        self.proposal.value
    }
}

#[cfg(test)]
mod decision_tests;
