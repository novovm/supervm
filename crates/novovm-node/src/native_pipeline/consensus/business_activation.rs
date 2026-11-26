//! The original journal supplies the canonical parent for the approved boundary.
use super::{
    statement::ParentPoint,
    wire::{
        activation::{
            ActivationApproval, BusinessActivation, BusinessRules, VerifiedBusinessActivation,
        },
        Context, ValidatorSet,
    },
};
use crate::native_pipeline::{
    business::{direct_nov_fee::DirectNovFeePolicy, uca_registration},
    execution::plan::BatchContext,
};
use anyhow::{ensure, Result};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessActivationConfig {
    pub activation: BusinessActivation,
    pub approvals: Vec<ActivationApproval>,
}

pub(crate) struct BusinessSchedule {
    verified: VerifiedBusinessActivation,
}

fn rules(context: &BatchContext) -> BusinessRules {
    BusinessRules {
        program: context.business_program,
        semantic_version: context.semantic_version,
        effects: context.effect_contract,
        receipts: context.receipt_codec,
    }
}

impl BusinessSchedule {
    pub(crate) fn verify(
        config: &BusinessActivationConfig,
        genesis: &BatchContext,
        policy: &DirectNovFeePolicy,
        validators: &ValidatorSet,
    ) -> Result<Self> {
        let activation = config.activation;
        ensure!(
            activation.context.chain_id == genesis.chain_id
                && activation.context.genesis_config_commitment
                    == genesis.genesis_config_commitment
                && activation.context.protocol_commitment == genesis.protocol_commitment
                && activation.previous == rules(genesis)
                && activation.next
                    == (BusinessRules {
                        program: uca_registration::program_id(),
                        semantic_version: uca_registration::REGISTRATION_SEMANTIC_VERSION,
                        effects: uca_registration::effect_contract(policy)?,
                        receipts: uca_registration::receipt_codec(),
                    }),
            "business activation does not match installed genesis and supported rules"
        );
        Ok(Self {
            verified: VerifiedBusinessActivation::verify(
                activation,
                &config.approvals,
                validators,
            )?,
        })
    }

    pub(crate) fn matches(
        &self,
        candidate: &BatchContext,
        context: Context,
        parent: ParentPoint,
    ) -> bool {
        let activation = self.verified.subject();
        let boundary = activation.context.height;
        let height = context.height;
        if height == boundary
            && (context != activation.context
                || parent.height.checked_add(1) != Some(boundary)
                || parent.block_hash != activation.context.parent_block_hash
                || parent.decision_hash != activation.context.parent_decision_hash
                || parent.state_root != activation.parent_state_root
                || parent.receipt_batch_commitment != activation.parent_receipt_root
                || parent.state_version != activation.parent_state_version)
        {
            return false;
        }
        let expected = if height < boundary {
            activation.previous
        } else {
            activation.next
        };
        candidate.height == height && rules(candidate) == expected
    }
}

pub(crate) fn matches_configured(
    schedule: Option<&BusinessSchedule>,
    candidate: &BatchContext,
    context: Context,
    parent: ParentPoint,
) -> bool {
    schedule.map_or_else(
        || !uca_registration::is_profile(candidate),
        |schedule| schedule.matches(candidate, context, parent),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_pipeline::{business::nov_transfer_batch, pipeline::test_fee_policy};
    use novovm_consensus::round_bft::{
        crypto::ConsensusSigner, test_vectors::FixtureSigningKey, wire::Validator,
    };

    #[test]
    fn only_the_certified_parent_switches_business_rules() -> Result<()> {
        let key = FixtureSigningKey::from_bytes(&[5; 32]);
        let validator = Validator::new(key.public_key(), 1)?;
        let set = ValidatorSet::new_for_test(7, 1, 1, vec![validator.clone()])?;
        let policy = test_fee_policy();
        let parent = ParentPoint {
            height: 7,
            block_hash: [3; 32],
            decision_hash: [4; 32],
            state_root: [5; 32],
            receipt_batch_commitment: [6; 32],
            state_version: 12,
        };
        let boundary = Context {
            chain_id: 7,
            genesis_config_commitment: [1; 32],
            protocol_commitment: [2; 32],
            epoch: 1,
            validator_set_hash: set.hash(),
            height: 8,
            parent_block_hash: parent.block_hash,
            parent_decision_hash: parent.decision_hash,
        };
        let original = BatchContext {
            chain_id: boundary.chain_id,
            genesis_config_commitment: boundary.genesis_config_commitment,
            protocol_commitment: boundary.protocol_commitment,
            business_program: nov_transfer_batch::uca_program_id(),
            semantic_version: nov_transfer_batch::SEMANTIC_VERSION,
            effect_contract: nov_transfer_batch::uca_effect_contract(&policy)?,
            parent_block_hash: parent.block_hash,
            parent_height: parent.height,
            parent_state_root: parent.state_root,
            parent_receipt_root: parent.receipt_batch_commitment,
            parent_state_version: parent.state_version,
            receipt_codec: nov_transfer_batch::uca_receipt_codec(),
            height: boundary.height,
            slot: 0,
            timestamp_unix_ms: 8,
        };
        let upgraded = BatchContext {
            business_program: uca_registration::program_id(),
            semantic_version: uca_registration::REGISTRATION_SEMANTIC_VERSION,
            effect_contract: uca_registration::effect_contract(&policy)?,
            receipt_codec: uca_registration::receipt_codec(),
            ..original
        };
        let activation = BusinessActivation {
            context: boundary,
            parent_state_root: parent.state_root,
            parent_receipt_root: parent.receipt_batch_commitment,
            parent_state_version: parent.state_version,
            previous: rules(&original),
            next: rules(&upgraded),
        };
        let mut config = BusinessActivationConfig {
            activation,
            approvals: vec![],
        };
        assert!(BusinessSchedule::verify(&config, &original, &policy, &set).is_err());
        config.approvals.push(ActivationApproval {
            signer: validator.id(),
            signature: key.sign(&activation.signing_bytes()?).to_bytes(),
        });
        let schedule = BusinessSchedule::verify(&config, &original, &policy, &set)?;
        let before = Context {
            height: 7,
            ..boundary
        };
        assert!(schedule.matches(
            &BatchContext {
                height: 7,
                ..original
            },
            before,
            parent
        ));
        assert!(!schedule.matches(
            &BatchContext {
                height: 7,
                ..upgraded
            },
            before,
            parent
        ));
        assert!(schedule.matches(&upgraded, boundary, parent));
        assert!(!schedule.matches(&original, boundary, parent));
        assert!(!schedule.matches(
            &upgraded,
            boundary,
            ParentPoint {
                state_root: [11; 32],
                ..parent
            }
        ));
        assert!(!schedule.matches(
            &upgraded,
            boundary,
            ParentPoint {
                decision_hash: [12; 32],
                ..parent
            }
        ));
        assert!(!matches_configured(None, &upgraded, boundary, parent));
        Ok(())
    }
}
