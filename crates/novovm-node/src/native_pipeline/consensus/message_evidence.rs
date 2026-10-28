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

impl VerifiedEvidence {
    /// Reuse only locally qualified, immutable evidence for these exact bytes.
    /// The enum is public, so matching its variant alone is NOT sufficient:
    /// callers may combine unrelated genuine proofs. No signature is repeated,
    /// but validator/verifier ownership, full payload and certificate relations
    /// remain checked before the stateful consensus owner sees the message.
    pub(super) fn ensure_matches(&self, message: &Message, config: &MessageDomain) -> Result<()> {
        match (message, self) {
            (Message::Vote(vote), Self::Vote(verified)) => {
                verified.ensure_verified_by(&config.validators)?;
                ensure!(verified.vote() == vote, "qualified vote payload mismatch");
                fixed_context(&vote.context, config)?;
            }
            (
                Message::Proposal {
                    proposal,
                    valid_quorum,
                    ..
                },
                Self::Proposal {
                    proposal: verified,
                    valid_quorum: valid,
                },
            ) => {
                verified.ensure_verified_by(&config.validators)?;
                ensure!(
                    verified.proposal() == proposal,
                    "qualified proposal payload mismatch"
                );
                fixed_context(&proposal.context, config)?;
                ensure!(
                    valid.as_ref().map(VerifiedQuorum::quorum) == valid_quorum.as_ref(),
                    "qualified valid-round certificate payload mismatch"
                );
                if let Some(qc) = valid {
                    qc.ensure_verified_by(&config.validators)?;
                }
                ensure!(
                    match (valid, proposal.valid_round) {
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
            }
            (
                Message::Decision {
                    proposal,
                    certificate,
                    ..
                },
                Self::Decision {
                    proposal: verified,
                    certificate: qc,
                },
            ) => {
                verified.ensure_verified_by(&config.validators)?;
                qc.ensure_verified_by(&config.validators)?;
                ensure!(
                    verified.proposal() == proposal,
                    "qualified decision proposal mismatch"
                );
                ensure!(
                    qc.quorum() == certificate,
                    "qualified decision certificate mismatch"
                );
                fixed_context(&proposal.context, config)?;
                ensure!(
                    qc.context() == &proposal.context
                        && qc.round() == proposal.round
                        && qc.phase() == Phase::Precommit
                        && qc.value() == Some(proposal.value),
                    "decision certificate mismatch"
                );
            }
            _ => anyhow::bail!("qualified consensus control kind mismatch"),
        }
        Ok(())
    }
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
        | Message::AssetBody { context, .. } => {
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
        | Message::AssetTransactions { scope, .. }
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

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_consensus::round_bft::{
        test_vectors::{sign_proposal, sign_vote, FixtureSigningKey, FixtureVerifier},
        wire::{Context, Proposal, Quorum, Validator},
    };

    struct Fixture {
        keys: Vec<FixtureSigningKey>,
        domain: MessageDomain,
        context: Context,
    }
    impl Fixture {
        fn new() -> Result<Self> {
            let keys = (1..=4)
                .map(|i| FixtureSigningKey::from_bytes(&[i; 32]))
                .collect::<Vec<_>>();
            let members = keys
                .iter()
                .map(|key| Validator::new(key.verifying_key().to_bytes(), 1))
                .collect::<Result<Vec<_>>>()?;
            let set = Arc::new(ValidatorSet::new_for_test(7, 1, 1, members)?);
            let context = Context {
                chain_id: 7,
                genesis_config_commitment: [1; 32],
                protocol_commitment: [2; 32],
                epoch: 1,
                validator_set_hash: set.hash(),
                height: 1,
                parent_block_hash: [0; 32],
                parent_decision_hash: [0; 32],
            };
            Ok(Self {
                keys,
                domain: MessageDomain {
                    chain_id: 7,
                    genesis: [1; 32],
                    protocol: [2; 32],
                    validators: set,
                },
                context,
            })
        }
        fn proposal(&self, round: u64, valid: Option<u64>, value: Hash) -> Result<Proposal> {
            let set = &self.domain.validators;
            let leader = set.leader(self.context.height, round)?;
            let key = self
                .keys
                .iter()
                .find(|key| {
                    Validator::new(key.verifying_key().to_bytes(), 1)
                        .unwrap()
                        .id()
                        == leader
                })
                .unwrap();
            sign_proposal(self.context, round, value, valid, set, key)
        }
        fn quorum(&self, phase: Phase, value: Hash) -> Result<Quorum> {
            let votes = self.keys[..3]
                .iter()
                .map(|key| {
                    sign_vote(
                        self.context,
                        0,
                        phase,
                        Some(value),
                        &self.domain.validators,
                        key,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            Quorum::from_votes(&self.domain.validators, votes)
        }
    }

    #[test]
    fn qualified_control_reuse_requires_exact_payload_domain_and_verifier_owner() -> Result<()> {
        let f = Fixture::new()?;
        let vote = sign_vote(
            f.context,
            0,
            Phase::Prevote,
            Some([9; 32]),
            &f.domain.validators,
            &f.keys[0],
        )?;
        let message = Message::Vote(vote.clone());
        let evidence = verify(&message, &f.domain)?;
        evidence.ensure_matches(&message, &f.domain)?;
        let mut changed = vote.clone();
        changed.signature[0] ^= 1;
        assert!(evidence
            .ensure_matches(&Message::Vote(changed), &f.domain)
            .is_err());
        let mut changed = vote;
        changed.round = 1;
        assert!(evidence
            .ensure_matches(&Message::Vote(changed), &f.domain)
            .is_err());
        let mut foreign = f.domain.clone();
        foreign.genesis = [44; 32];
        assert!(evidence.ensure_matches(&message, &foreign).is_err());
        foreign = f.domain.clone();
        foreign.validators = Arc::new(
            (*foreign.validators)
                .clone()
                .with_verifier(Arc::new(FixtureVerifier)),
        );
        assert!(evidence.ensure_matches(&message, &foreign).is_err());
        assert!(VerifiedEvidence::None
            .ensure_matches(&message, &f.domain)
            .is_err());
        assert!(evidence
            .ensure_matches(&Message::RequestBody { body_id: [1; 32] }, &f.domain)
            .is_err());
        Ok(())
    }

    #[test]
    fn qualified_proposal_reuse_rejects_spliced_valid_round_evidence() -> Result<()> {
        let f = Fixture::new()?;
        let proposal = f.proposal(1, Some(0), [9; 32])?;
        let qc = f.quorum(Phase::Prevote, [9; 32])?;
        let message = Message::Proposal {
            proposal: proposal.clone(),
            valid_quorum: Some(qc.clone()),
            body_id: [3; 32],
        };
        let evidence = verify(&message, &f.domain)?;
        evidence.ensure_matches(&message, &f.domain)?;
        let bad_qc = f.quorum(Phase::Prevote, [10; 32])?;
        // Both pieces have genuine signatures under OUR set. Neither a public
        // enum constructor nor exact payload matching grants their relationship.
        let spliced = Message::Proposal {
            proposal: proposal.clone(),
            valid_quorum: Some(bad_qc.clone()),
            body_id: [3; 32],
        };
        let proof = VerifiedEvidence::Proposal {
            proposal: proposal.verify(&f.domain.validators)?,
            valid_quorum: Some(bad_qc.verify(&f.domain.validators)?),
        };
        assert!(proof.ensure_matches(&spliced, &f.domain).is_err());
        assert!(evidence.ensure_matches(&spliced, &f.domain).is_err());
        let absent = Message::Proposal {
            proposal: proposal.clone(),
            valid_quorum: None,
            body_id: [3; 32],
        };
        let absent_proof = VerifiedEvidence::Proposal {
            proposal: proposal.verify(&f.domain.validators)?,
            valid_quorum: None,
        };
        assert!(absent_proof.ensure_matches(&absent, &f.domain).is_err());
        Ok(())
    }

    #[test]
    fn qualified_decision_reuse_requires_exact_precommit_certificate() -> Result<()> {
        let f = Fixture::new()?;
        let proposal = f.proposal(0, None, [9; 32])?;
        let certificate = f.quorum(Phase::Precommit, [9; 32])?;
        let message = Message::Decision {
            proposal: proposal.clone(),
            certificate,
            body_id: [3; 32],
        };
        let evidence = verify(&message, &f.domain)?;
        evidence.ensure_matches(&message, &f.domain)?;
        for bad in [
            f.quorum(Phase::Prevote, [9; 32])?,
            f.quorum(Phase::Precommit, [10; 32])?,
        ] {
            let spliced = Message::Decision {
                proposal: proposal.clone(),
                certificate: bad.clone(),
                body_id: [3; 32],
            };
            let proof = VerifiedEvidence::Decision {
                proposal: proposal.verify(&f.domain.validators)?,
                certificate: bad.verify(&f.domain.validators)?,
            };
            assert!(proof.ensure_matches(&spliced, &f.domain).is_err());
            assert!(evidence.ensure_matches(&spliced, &f.domain).is_err());
        }
        Ok(())
    }
}
