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
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessActivationConfig {
    pub activation: BusinessActivation,
    pub approvals: Vec<ActivationApproval>,
}

pub const MAX_ACTIVATION_HEADER_BYTES: usize = super::statement::finalized_header::MAX_BYTES;

pub struct ReviewedBusinessActivation {
    subject: BusinessActivation,
    validators: ValidatorSet,
    signing_key_file: std::path::PathBuf,
}

impl ReviewedBusinessActivation {
    pub fn subject(&self) -> &BusinessActivation {
        &self.subject
    }

    pub fn approve(&self, runtime: &novovm_exec::AoemExecFacade) -> Result<ActivationApproval> {
        use novovm_consensus::round_bft::crypto::ConsensusSigner;
        use novovm_relay::purpose_keys::load_validator;
        let crypto = super::crypto::AoemConsensusCrypto::new(runtime)?;
        let key = load_validator(
            &self.signing_key_file,
            self.subject.context.chain_id,
            |public, secret, challenge| crypto.check_key_pair(public, secret, challenge),
        )?;
        let signer = self
            .validators
            .members()
            .iter()
            .find(|member| member.public_key() == key.public_key.as_slice())
            .ok_or_else(|| anyhow::anyhow!("local key is not in the original validator set"))?
            .id();
        let signature = crypto
            .signer(key)?
            .sign_mldsa65(&self.subject.signing_bytes()?)?;
        Ok(ActivationApproval { signer, signature })
    }

    pub fn assemble(
        &self,
        mut approvals: Vec<ActivationApproval>,
    ) -> Result<BusinessActivationConfig> {
        approvals.sort_by_key(|approval| approval.signer);
        VerifiedBusinessActivation::verify(self.subject, &approvals, &self.validators)?;
        Ok(BusinessActivationConfig {
            activation: self.subject,
            approvals,
        })
    }
}

/// Rebuild the subject from a certified parent and independently reviewed
/// checkpoint under the resident node's original native genesis and fee policy.
pub fn review_activation(
    config: &crate::native_pipeline::service::config::ResidentConfig,
    runtime: &novovm_exec::AoemExecFacade,
    preceding: &ParentPoint,
    header: &[u8],
) -> Result<ReviewedBusinessActivation> {
    use super::{crypto::AoemConsensusCrypto, statement::finalized_header, wire::Validator};
    use crate::native_pipeline::business::{nov_transfer_batch as asset, uca_application as app};
    use std::sync::Arc;
    let initialization = config
        .uca_initialization
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("activation requires the original UCA initialization"))?;
    let genesis = &config.genesis;
    let validators = ValidatorSet::new_with_verifier(
        genesis.chain_id,
        genesis.validator_epoch,
        1,
        genesis
            .validators
            .iter()
            .map(|row| Validator::new(&row.public_key, row.weight))
            .collect::<Result<Vec<_>>>()?,
        Arc::new(AoemConsensusCrypto::new(runtime)?),
    )?;
    let context = Context {
        chain_id: genesis.chain_id,
        genesis_config_commitment: genesis.genesis_config_commitment,
        protocol_commitment: genesis.protocol_commitment,
        epoch: validators.epoch(),
        validator_set_hash: validators.hash(),
        height: preceding
            .height
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("checkpoint height exhausted"))?,
        parent_block_hash: preceding.block_hash,
        parent_decision_hash: preceding.decision_hash,
    };
    let anchor = finalized_header::verify(header, &validators, context, preceding)?;
    let point = anchor.point();
    let previous = if initialization.application_authority_enabled() {
        BusinessRules {
            program: app::application_program_id_v1(),
            semantic_version: app::APPLICATION_SEMANTIC_VERSION_V1,
            effects: app::application_effect_contract_v1(&genesis.policy)?,
            receipts: app::application_receipt_codec_v1(),
        }
    } else {
        BusinessRules {
            program: asset::uca_program_id(),
            semantic_version: asset::SEMANTIC_VERSION,
            effects: asset::uca_effect_contract(&genesis.policy)?,
            receipts: asset::uca_receipt_codec(),
        }
    };
    ensure!(
        rules(anchor.execution_context()) == previous,
        "certified anchor does not use the resident genesis business rules"
    );
    let activation = BusinessActivation {
        context: Context {
            height: point
                .height
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("activation height exhausted"))?,
            parent_block_hash: point.block_hash,
            parent_decision_hash: point.decision_hash,
            ..context
        },
        parent_state_root: point.state_root,
        parent_receipt_root: point.receipt_batch_commitment,
        parent_state_version: point.state_version,
        previous,
        next: replacement_rules(&genesis.policy)?,
    };
    activation.signing_bytes()?;
    Ok(ReviewedBusinessActivation {
        subject: activation,
        validators,
        signing_key_file: config.signing_key_file.clone(),
    })
}

fn registration_rules(policy: &DirectNovFeePolicy) -> Result<BusinessRules> {
    Ok(BusinessRules {
        program: uca_registration::program_id(),
        semantic_version: uca_registration::REGISTRATION_SEMANTIC_VERSION,
        effects: uca_registration::effect_contract(policy)?,
        receipts: uca_registration::receipt_codec(),
    })
}

fn replacement_rules(policy: &DirectNovFeePolicy) -> Result<BusinessRules> {
    Ok(BusinessRules {
        program: uca_registration::replacement_program_id(),
        semantic_version: uca_registration::DEVICE_REPLACEMENT_SEMANTIC_VERSION,
        effects: uca_registration::replacement_effect_contract(policy)?,
        receipts: uca_registration::receipt_codec(),
    })
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
    pub(crate) fn is_active_at(&self, height: u64) -> bool {
        height >= self.verified.subject().context.height
    }

    pub(crate) fn supports_device_replacement_at(&self, height: u64) -> bool {
        self.is_active_at(height)
            && self.verified.subject().next.semantic_version
                == uca_registration::DEVICE_REPLACEMENT_SEMANTIC_VERSION
    }

    pub(crate) fn apply(&self, template: &mut BatchContext, height: u64) {
        if self.is_active_at(height) {
            let next = self.verified.subject().next;
            template.business_program = next.program;
            template.semantic_version = next.semantic_version;
            template.effect_contract = next.effects;
            template.receipt_codec = next.receipts;
        }
    }

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
                && (activation.next == registration_rules(policy)?
                    || activation.next == replacement_rules(policy)?),
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
        let next = replacement_rules(&policy)?;
        let replacement = BatchContext {
            business_program: next.program,
            semantic_version: next.semantic_version,
            effect_contract: next.effects,
            receipt_codec: next.receipts,
            ..upgraded
        };
        assert!(!schedule.supports_device_replacement_at(boundary.height));
        assert!(!schedule.matches(&replacement, boundary, parent));
        config.activation.next = next;
        assert!(BusinessSchedule::verify(&config, &original, &policy, &set).is_err());
        config.approvals[0].signature = key.sign(&config.activation.signing_bytes()?).to_bytes();
        let replacement_schedule = BusinessSchedule::verify(&config, &original, &policy, &set)?;
        assert!(!replacement_schedule.supports_device_replacement_at(parent.height));
        assert!(replacement_schedule.supports_device_replacement_at(boundary.height));
        assert!(replacement_schedule.matches(&replacement, boundary, parent));
        assert!(!replacement_schedule.matches(&upgraded, boundary, parent));
        Ok(())
    }
}
