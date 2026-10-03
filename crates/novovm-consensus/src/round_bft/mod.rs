//! Versioned round-BFT evidence and safety rules, migrated from the committed
//! runtime consensus implementation at `2da3583c`.
//!
//! This is not native V3 and does not reinterpret its signatures or certificates.
//! The journal owns durable signing preparation and release after a trusted
//! node adapter acknowledges the exact atomic transition. Decision-only checking
//! still grants no signature or publication authority. Local execution/DA and
//! complete durable parent recovery remain the product adapter's responsibility.

pub mod collector;
pub mod journal;
pub mod pacemaker;
// Raw transitions and signing remain private to the consensus safety boundary.
pub(crate) mod round;
pub mod wire;

/// Independent peer/adversarial signatures for downstream test fixtures only.
/// Never enable this feature on a normal product dependency or use these
/// helpers as a durable local signer. They confer no execution/publication ACK.
#[cfg(feature = "test-vectors")]
pub mod test_vectors;

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
