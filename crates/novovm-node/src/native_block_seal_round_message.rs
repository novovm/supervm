//! Typed, authenticated inputs to the opt-in single-height round driver.
//!
//! These are not a network codec and do not widen the existing Overlay's
//! round-zero proposal contract. Serialization is used only to bound the total
//! nested input before signature verification. A caller must obtain the source
//! peer from an authenticated transport, never from an untrusted message field.

use super::newview::{NovNativeSealNewViewCertificateV1, NovNativeSealNewViewObservationV1};
use super::timeout::{
    NovNativeSealTimeoutCertificateV1, NovNativeSealTimeoutContextV1, NovNativeSealTimeoutVoteV1,
};
use super::*;
use crate::native_block_seal_overlay::{
    NovNativeSealEpochAuthorityV1, NOV_NATIVE_SEAL_OVERLAY_MAX_WIRE_BYTES_V1,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum NovNativeSealRoundMessageV1 {
    Timeout(Box<NovNativeSealTimeoutVoteV1>),
    TimeoutCertificate(Box<NovNativeSealTimeoutCertificateV1>),
    NewView {
        observation: Box<NovNativeSealNewViewObservationV1>,
        previous_timeout: Box<NovNativeSealTimeoutCertificateV1>,
    },
    Proposal {
        proposal: Box<NovNativeSealProposalV1>,
        certificate: Option<Box<NovNativeSealNewViewCertificateV1>>,
    },
    Vote {
        proposal: Box<NovNativeSealProposalV1>,
        vote: Box<NovNativeSealVoteV1>,
        certificate: Option<Box<NovNativeSealNewViewCertificateV1>>,
    },
    QuorumCertificate {
        proposal: Box<NovNativeSealProposalV1>,
        qc: Box<NovNativeSealQuorumCertificateV1>,
        certificate: Option<Box<NovNativeSealNewViewCertificateV1>>,
    },
}

impl NovNativeSealRoundMessageV1 {
    /// A TC identifies the round it timed out, not the next target round.
    pub fn round(&self) -> u64 {
        match self {
            Self::Timeout(vote) => vote.context.round,
            Self::TimeoutCertificate(certificate) => certificate.context.round,
            Self::NewView { observation, .. } => observation.context.round,
            Self::Proposal { proposal, .. }
            | Self::Vote { proposal, .. }
            | Self::QuorumCertificate { proposal, .. } => proposal.subject.round,
        }
    }

    pub fn proposal(&self) -> Option<&NovNativeSealProposalV1> {
        match self {
            Self::Proposal { proposal, .. }
            | Self::Vote { proposal, .. }
            | Self::QuorumCertificate { proposal, .. } => Some(proposal),
            Self::Timeout(_) | Self::TimeoutCertificate(_) | Self::NewView { .. } => None,
        }
    }

    pub fn certificate(&self) -> Option<&NovNativeSealNewViewCertificateV1> {
        match self {
            Self::Proposal { certificate, .. }
            | Self::Vote { certificate, .. }
            | Self::QuorumCertificate { certificate, .. } => certificate.as_deref(),
            Self::Timeout(_) | Self::TimeoutCertificate(_) | Self::NewView { .. } => None,
        }
    }

    pub fn validate_authenticated(
        &self,
        authority: &NovNativeSealEpochAuthorityV1,
        height: u64,
        source_peer_id: &str,
    ) -> Result<()> {
        // A fixed output buffer fails as soon as serialization exceeds the
        // budget, including nested QC/observation vectors and signature bytes.
        // Do not allocate an unbounded serialization merely to measure it.
        let mut bounded = vec![0u8; NOV_NATIVE_SEAL_OVERLAY_MAX_WIRE_BYTES_V1];
        postcard::to_slice(self, &mut bounded)
            .context("native round message exceeds the bounded typed-input size")?;
        authority.validate()?;
        if height < authority.activation_height || self.round() == u64::MAX {
            bail!("native round message is outside the local height/round domain");
        }
        let source_validator = authority
            .validator_for_transport_peer(source_peer_id)
            .context("native round message source is not an authority-bound peer")?;
        let direct_signer = match self {
            Self::Timeout(vote) => Some(vote.validator_id),
            Self::NewView { observation, .. } => Some(observation.validator_id),
            Self::Proposal { proposal, .. } => Some(proposal.proposer_id),
            Self::Vote { vote, .. } => Some(vote.validator_id),
            // Aggregate certificates may be relayed by any pinned validator;
            // relaying never substitutes the sender for the actual signers.
            Self::TimeoutCertificate(_) | Self::QuorumCertificate { .. } => None,
        };
        if direct_signer.is_some_and(|signer| signer != source_validator) {
            bail!("native round message signer differs from its authenticated source");
        }
        let expected = NovNativeSealTimeoutContextV1 {
            chain_id: authority.chain_id,
            genesis_block_hash: authority.genesis_block_hash,
            protocol_config_commitment: authority.protocol_config_commitment,
            epoch: authority.epoch,
            validator_set_hash: authority.validator_set.validator_set_hash,
            height,
            round: self.round(),
        };
        let set = &authority.validator_set;
        match self {
            Self::Timeout(vote) => {
                if vote.context != expected {
                    bail!("native round timeout does not match the pinned local context");
                }
                vote.verify(set)?;
            }
            Self::TimeoutCertificate(certificate) => {
                certificate.verify(&expected, set)?;
            }
            Self::NewView {
                observation,
                previous_timeout,
            } => {
                observation.verify(&expected, authority)?;
                let mut preceding = expected;
                preceding.round = preceding
                    .round
                    .checked_sub(1)
                    .context("native round new-view requires a nonzero target round")?;
                previous_timeout.verify(&preceding, set)?;
            }
            Self::Proposal { .. } | Self::Vote { .. } | Self::QuorumCertificate { .. } => {
                let proposal = self
                    .proposal()
                    .context("native round proposal is missing")?;
                proposal.verify(set)?;
                let subject = &proposal.subject;
                if subject.chain_id != expected.chain_id
                    || subject.genesis_block_hash != expected.genesis_block_hash
                    || subject.protocol_config_commitment != expected.protocol_config_commitment
                    || subject.epoch != expected.epoch
                    || subject.validator_set_hash != expected.validator_set_hash
                    || subject.height != expected.height
                    || proposal.proposer_id
                        != authority.scheduled_leader_v1(height, subject.round)?
                {
                    bail!("native round proposal does not match the authority or scheduled leader");
                }
                match (subject.round, self.certificate()) {
                    (0, None) => {}
                    (0, Some(_)) => {
                        bail!("native round zero proposal cannot carry new-view evidence")
                    }
                    (_, None) => bail!("native nonzero-round proposal requires new-view evidence"),
                    (_, Some(certificate)) => {
                        if let Some(highest) = certificate.verify(&expected, authority)? {
                            let mut highest_subject = highest.qc.subject;
                            let mut proposed_subject = subject.clone();
                            highest_subject.round = 0;
                            highest_subject.subject_hash = [0; 32];
                            proposed_subject.round = 0;
                            proposed_subject.subject_hash = [0; 32];
                            if highest_subject != proposed_subject {
                                bail!("native round proposal changes the selected highest-QC candidate");
                            }
                        }
                    }
                }
                match self {
                    Self::Vote { vote, .. } => vote.verify(subject, proposal.proposal_hash, set)?,
                    Self::QuorumCertificate { qc, .. } => {
                        qc.verify(set)?;
                        if qc.subject != *subject || qc.proposal_hash != proposal.proposal_hash {
                            bail!("native round QC does not bind its embedded proposal");
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
