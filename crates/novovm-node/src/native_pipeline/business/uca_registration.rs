//! Prepare the initial identity effect from the original captured state.
use super::{
    record_pages,
    uca_spend_state::{identity_prefix, identity_record_changes},
};
use crate::native_pipeline::state::{
    frontier::{DeclaredAccess, OwnedStateInput},
    tree::StateChange,
};
use anyhow::{ensure, Result};
use novovm_adapter_api::unified_account::{
    UcaAccount, UcaKeyAlgo, UcaKeyProofType, UcaPrimaryKeyBinding, UcaStatus,
};
use novovm_exec::uca_registration::VerifiedUcaRegistrationV1;
use novovm_protocol::uca_registration::UcaRegistrationIntentV1;

pub fn registration_declarations(intent: &UcaRegistrationIntentV1) -> Result<Vec<DeclaredAccess>> {
    let prefix = identity_prefix(intent.chain_id, &intent.genesis, intent.identity.as_str())?;
    record_pages::declarations(&prefix, 8192, true)
}

/// The admitted proof and capture both belong to the block's existing domain.
/// The consensus owner still decides when registration execution is available.
pub fn register_identity(
    proof: &VerifiedUcaRegistrationV1,
    input: &OwnedStateInput,
    context: &crate::native_pipeline::execution::plan::BatchContext,
) -> Result<Vec<StateChange>> {
    let intent = proof.intent();
    ensure!(
        context.parent_state_root != [0; 32]
            && context.parent_state_root == input.parent_root()
            && context.parent_height.checked_add(1) == Some(context.height)
            && intent.chain_id == context.chain_id
            && intent.genesis == context.genesis_config_commitment
            && proof.height() == context.height,
        "registration capture or consensus domain mismatch"
    );
    let declarations = registration_declarations(intent)?;
    let keys: Vec<_> = declarations.into_iter().map(|entry| entry.key).collect();
    ensure!(
        input.read_many(&keys)?.iter().all(Option::is_none),
        "identity or orphan identity page already exists"
    );
    let timestamp = context.timestamp_unix_ms;
    let record = UcaAccount {
        uca_id: intent.identity.as_str().into(),
        primary_key_ref: intent.primary_key_ref.to_vec(),
        primary_key_binding: Some(UcaPrimaryKeyBinding {
            key_algo: UcaKeyAlgo::Mldsa65,
            public_key: proof.public_key().to_vec(),
            proof_type: UcaKeyProofType::NativeRegistrationV1,
            proof_payload: proof.proof_id().to_vec(),
            verified_at: timestamp,
        }),
        status: UcaStatus::Active,
        created_at: timestamp,
        updated_at: timestamp,
    };
    identity_record_changes(intent.chain_id, &intent.genesis, &record)
}
