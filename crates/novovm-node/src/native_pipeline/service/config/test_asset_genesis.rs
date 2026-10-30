//! Subject-bound approvals for synthetic fixtures only; no deployment custody.
use super::uca_genesis::{UcaGenesisApprovalV2, UcaGenesisManifestV2, INITIALIZATION_CONTEXT_V2};
use anyhow::{Context, Result};
use novovm_exec::{pq_signature::AoemMldsaParameterSet, AoemExecFacade};

/// Sign every required identity/asset purpose independently, using only an exact
/// public-key match. Fixtures cannot let identity possession stand in for an
/// account's management or explicitly configured recovery approval.
pub(crate) fn approve_manifest(
    facade: &AoemExecFacade,
    manifest: &UcaGenesisManifestV2,
    keys: &[(&[u8], &[u8])],
) -> Result<Vec<UcaGenesisApprovalV2>> {
    let signer = facade.mldsa_signer_v1(AoemMldsaParameterSet::MlDsa65)?;
    let plan = manifest.prepare_approvals()?;
    let keys: std::collections::BTreeMap<_, _> = keys.iter().copied().collect();
    plan.subjects()
        .map(|(subject, public_key)| {
            let secret = keys
                .get(public_key)
                .context("fixture lacks the exact subject-purpose signing key")?;
            let signature = signer.sign(
                public_key,
                secret,
                &plan.signing_bytes(subject)?,
                INITIALIZATION_CONTEXT_V2,
            )?;
            Ok(UcaGenesisApprovalV2 {
                subject: subject.clone(),
                signature,
            })
        })
        .collect()
}
