//! Verify registration archive content without creating a signature capability.
use super::*;
use crate::native_pipeline::business::uca_registration as business;
use novovm_protocol::uca_registration::SignedUcaRegistrationV1;

pub(super) fn is_profile(context: &BatchContext) -> bool {
    business::is_profile(context)
}

pub(super) fn validate_receipts(plan: &BatchPlan, receipts: &[Vec<u8>]) -> Result<()> {
    ensure!(
        plan.transaction_count() == 1 && receipts.len() == 1,
        "registration archive must contain one command and receipt"
    );
    let signed = SignedUcaRegistrationV1::decode(plan.canonical_transaction(0)?.as_ref())?;
    let context = plan.context();
    let message = novovm_exec::uca_registration::registration_transcript_v1(
        &signed,
        context.chain_id,
        context.genesis_config_commitment,
        context.height,
    )?;
    ensure!(
        receipts[0] == business::receipt(Sha256::digest(message).into()),
        "registration receipt does not match the command"
    );
    let mut expected = business::registration_declarations(&signed.intent)?;
    expected.sort_by(|a, b| a.key.cmp(&b.key));
    ensure!(
        expected.len() == plan.declared_access().len()
            && expected
                .iter()
                .zip(plan.declared_access())
                .all(|(a, b)| a.key == b.key
                    && a.may_put == b.may_put
                    && a.may_delete == b.may_delete),
        "registration archive access declarations differ from compiler"
    );
    Ok(())
}
