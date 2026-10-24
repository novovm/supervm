//! Message qualification extracted from channel.rs at 58e3aade. Signature,
//! valid-round and fixed-domain checks are unchanged; this is not signing or
//! publication authority. Transport does not own consensus transitions.
use super::transport::{EarlyBodyScope, Message};
use super::wire::{Hash, Phase, ValidatorSet, VerifiedProposal, VerifiedQuorum, VerifiedVote};
use crate::native_pipeline::execution::plan::BatchContext;
use anyhow::{ensure, Result};
use std::sync::Arc;

#[derive(Clone)]
pub struct MessageDomain {
    pub chain_id: u64,
    pub genesis: Hash,
    pub protocol: Hash,
    pub validators: Arc<ValidatorSet>,
}

#[derive(Clone, Debug)]
pub enum VerifiedEvidence {
    None,
    Proposal {
        proposal: VerifiedProposal,
        valid_quorum: Option<VerifiedQuorum>,
    },
    Vote(VerifiedVote),
    Decision {
        proposal: VerifiedProposal,
        certificate: VerifiedQuorum,
    },
}

fn fixed_context(context: &super::wire::Context, config: &MessageDomain) -> Result<()> {
    context.validate(&config.validators)?;
    ensure!(
        context.genesis_config_commitment == config.genesis
            && context.protocol_commitment == config.protocol,
        "message differs from local genesis/protocol"
    );
    Ok(())
}

fn fixed_early(scope: &EarlyBodyScope, config: &MessageDomain) -> Result<()> {
    scope.validate_shape()?;
    fixed_context(&scope.source, config)
}

/// Shape only: all nonzero parent roots, program pins and time fields remain
/// untrusted claims. The controller/compiler must independently authorize them.
fn fixed_binding(
    scope: &EarlyBodyScope,
    id: &Hash,
    context: &BatchContext,
    config: &MessageDomain,
) -> Result<()> {
    fixed_early(scope, config)?;
    scope.validate_binding(id, context)?;
    ensure!(
        context.semantic_version != 0,
        "empty bound semantic version"
    );
    ensure!(
        (context.parent_height == 0) == (context.parent_block_hash == [0; 32]),
        "bound first-block parent hash convention mismatch"
    );
    for hash in [
        context.business_program,
        context.effect_contract,
        context.parent_state_root,
        context.parent_receipt_root,
        context.receipt_codec,
    ] {
        ensure!(hash != [0; 32], "empty bound domain/root commitment");
    }
    Ok(())
}

pub fn verify(message: &Message, config: &MessageDomain) -> Result<VerifiedEvidence> {
    match message {
        Message::Body { context, .. }
        | Message::ApflBody { context, .. }
        | Message::UcaBody { context, .. } => {
            ensure!(
                context.chain_id == config.chain_id
                    && context.genesis_config_commitment == config.genesis
                    && context.protocol_commitment == config.protocol,
                "body differs from local chain domain"
            );
            Ok(VerifiedEvidence::None)
        }
        Message::EarlyBody { scope, .. } | Message::ApflEarlyBody { scope, .. } => {
            fixed_early(scope, config)?;
            Ok(VerifiedEvidence::None)
        }
        Message::Transactions { scope, .. }
        | Message::ApflTransactions { scope, .. }
        | Message::TransactionsTaken { scope, .. } => {
            scope.validate_shape()?;
            ensure!(
                scope.chain_id == config.chain_id
                    && scope.genesis == config.genesis
                    && scope.protocol == config.protocol
                    && scope.epoch == config.validators.epoch()
                    && scope.validator_set_hash == config.validators.hash(),
                "input gossip differs from pinned domain"
            );
            Ok(VerifiedEvidence::None)
        }
        Message::BindBody {
            scope,
            announcement_id,
            context,
        } => {
            fixed_binding(scope, announcement_id, context, config)?;
            Ok(VerifiedEvidence::None)
        }
        Message::Proposal {
            proposal,
            valid_quorum,
            ..
        } => {
            fixed_context(&proposal.context, config)?;
            let verified = proposal.verify(&config.validators)?;
            let valid = valid_quorum
                .as_ref()
                .map(|qc| qc.verify(&config.validators))
                .transpose()?;
            ensure!(
                match (&valid, proposal.valid_round) {
                    (None, None) => true,
                    (Some(qc), Some(round)) =>
                        qc.context() == &proposal.context
                            && qc.phase() == Phase::Prevote
                            && qc.round() == round
                            && qc.value() == Some(proposal.value),
                    _ => false,
                },
                "proposal valid-round certificate mismatch"
            );
            Ok(VerifiedEvidence::Proposal {
                proposal: verified,
                valid_quorum: valid,
            })
        }
        Message::Vote(vote) => {
            fixed_context(&vote.context, config)?;
            Ok(VerifiedEvidence::Vote(vote.verify(&config.validators)?))
        }
        Message::Decision {
            proposal,
            certificate,
            ..
        } => {
            fixed_context(&proposal.context, config)?;
            let proposal = proposal.verify(&config.validators)?;
            let qc = certificate.verify(&config.validators)?;
            ensure!(
                qc.context() == &proposal.proposal().context
                    && qc.round() == proposal.proposal().round
                    && qc.phase() == Phase::Precommit
                    && qc.value() == Some(proposal.proposal().value),
                "decision certificate mismatch"
            );
            Ok(VerifiedEvidence::Decision {
                proposal,
                certificate: qc,
            })
        }
        Message::RequestBody { .. } => Ok(VerifiedEvidence::None),
        Message::RequestDecision { context } => {
            fixed_context(context, config)?;
            Ok(VerifiedEvidence::None)
        }
    }
}
