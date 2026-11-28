//! Content checks on the original archive owner, not signature or state authority.
use super::*;
use crate::native_pipeline::business::uca_application::{
    application_command_id, application_program_id_v1, application_receipt_codec_v1,
    ApplicationGrantAccessV1, APPLICATION_SEMANTIC_VERSION_V1,
};
use novovm_protocol::uca_application::{
    ApplicationGrantActionV1, ApplicationGrantReceiptV1, SignedApplicationGrantV1,
};

// Shared V4 pins cover both branches; outputs/header select the receipt type.
pub(super) fn is_profile(context: &BatchContext) -> bool {
    context.business_program == application_program_id_v1()
        && context.receipt_codec == application_receipt_codec_v1()
        && context.semantic_version == APPLICATION_SEMANTIC_VERSION_V1
}

#[cfg(test)]
mod tests;

pub(super) fn validate_receipts(
    plan: &BatchPlan,
    receipts: &[Vec<u8>],
    budget: PacketBudget,
) -> Result<()> {
    ensure!(
        (is_profile(plan.context()) || super::registration::is_profile(plan.context()))
            && receipts.len() == plan.transaction_count(),
        "application receipt profile/count mismatch"
    );
    let context = plan.context();
    let mut grants = BTreeSet::new();
    let mut expected = BTreeMap::<Vec<u8>, DeclaredAccess>::new();
    for (index, encoded) in receipts.iter().enumerate() {
        ensure!(
            encoded.len() <= budget.max_receipt_bytes,
            "application receipt exceeds budget"
        );
        let raw = plan.canonical_transaction(index)?;
        let signed = SignedApplicationGrantV1::decode(raw.as_ref())?;
        let intent = &signed.intent;
        ensure!(
            intent.selector.chain_id == context.chain_id
                && intent.selector.genesis == context.genesis_config_commitment,
            "application archive domain mismatch"
        );
        ensure!(
            intent.valid_from_height <= context.height
                && context.height < intent.valid_until_height,
            "application archive command outside validity window"
        );
        ensure!(
            intent.issuer_key_ref.as_slice()
                == novovm_adapter_api::uca_key_binding::derive_primary_key_ref_from_binding_v1(
                    novovm_adapter_api::UcaKeyAlgo::Mldsa65,
                    &signed.public_key
                )
                .as_slice()
                && intent.selector.device != intent.issuer_key_ref,
            "application archive issuer/device mismatch"
        );
        let grant_id = intent.selector.grant_id()?;
        ensure!(
            grants.insert(grant_id),
            "application archive repeated selector"
        );
        let receipt = ApplicationGrantReceiptV1::decode(encoded)?;
        ensure!(
            receipt.command_id == application_command_id(intent)?
                && receipt.grant_id == grant_id
                && receipt.issuer_key_ref == intent.issuer_key_ref
                && receipt.previous_revision == intent.expected_revision,
            "application receipt command/issuer/revision mismatch"
        );
        match intent.action {
            ApplicationGrantActionV1::Authorize {
                not_before_height,
                expires_at_height,
            } => {
                ensure!(
                    !receipt.revoked
                        && receipt.not_before_height == not_before_height
                        && receipt.expires_at_height == expires_at_height
                        && context.height < expires_at_height,
                    "application authorization receipt mismatch"
                );
            }
            ApplicationGrantActionV1::ReplaceDevice {
                not_before_height,
                expires_at_height,
                ..
            } => {
                ensure!(
                    crate::native_pipeline::business::uca_registration::supports_device_replacement(
                        context
                    ) && receipt.revoked
                        && receipt.previous_revision > 0
                        && not_before_height <= context.height
                        && context.height < expires_at_height,
                    "application replacement receipt mismatch"
                );
            }
            ApplicationGrantActionV1::Revoke => {
                ensure!(
                    receipt.revoked && receipt.previous_revision > 0,
                    "application revocation receipt mismatch"
                );
            }
        }
        let primary = ApplicationGrantAccessV1::new(intent.selector.clone())?;
        let replacement = intent
            .replacement_effects()
            .map(|[_, next]| {
                ensure!(
                    grants.insert(next.selector.grant_id()?),
                    "application archive repeated replacement selector"
                );
                ApplicationGrantAccessV1::new(next.selector)
            })
            .transpose()?;
        for access in std::iter::once(&primary)
            .chain(replacement.iter())
            .flat_map(|target| target.declarations())
        {
            expected
                .entry(access.key.clone())
                .and_modify(|value| {
                    value.may_put |= access.may_put;
                    value.may_delete |= access.may_delete;
                })
                .or_insert_with(|| access.clone());
        }
    }
    ensure!(
        expected.len() == plan.declared_access().len()
            && expected
                .into_values()
                .zip(plan.declared_access())
                .all(|(expected, actual)| {
                    expected.key == actual.key
                        && expected.may_put == actual.may_put
                        && expected.may_delete == actual.may_delete
                }),
        "application archive access declarations differ from compiler"
    );
    Ok(())
}
