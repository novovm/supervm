//! Prepare the initial identity effect from the original captured state.
use super::{
    record_pages,
    uca_spend_state::{identity_prefix, identity_record_changes},
};
use crate::native_pipeline::{
    execution::plan::{
        BatchContext, BatchPlan, BoundBatchCapture, OwnedBatchInput, PlanBudget,
        UnpublishedBatchEffects,
    },
    state::{
        frontier::{CaptureBudget, DeclaredAccess, OwnedStateInput},
        tree::{NodeHash, StateChange},
    },
};
use anyhow::{ensure, Result};
use novovm_adapter_api::unified_account::{
    UcaAccount, UcaKeyAlgo, UcaKeyProofType, UcaPrimaryKeyBinding, UcaStatus,
};
use novovm_exec::{uca_registration::VerifiedUcaRegistrationV1, AoemExecFacade};
use novovm_protocol::uca_registration::{SignedUcaRegistrationV1, UcaRegistrationIntentV1};
use sha2::{Digest, Sha256};

pub const REGISTRATION_SEMANTIC_VERSION: u32 = 5;

pub fn program_id() -> NodeHash {
    Sha256::digest(b"novovm/uca/native-identity-and-application/v5").into()
}

pub fn receipt_codec() -> NodeHash {
    Sha256::digest(b"novovm/uca/native-identity-and-application/receipt/v5").into()
}

pub fn effect_contract(policy: &super::direct_nov_fee::DirectNovFeePolicy) -> Result<NodeHash> {
    let mut hash = Sha256::new();
    hash.update(b"novovm/uca/native-identity-and-application/effects/v5\0");
    hash.update(super::uca_application::application_effect_contract_v1(
        policy,
    )?);
    hash.update(program_id());
    hash.update(receipt_codec());
    hash.update(b"NUIR1-NUIE1;MLDSA65-self-derived-root;one-registration;all-identity-pages-absent;no-spend-authority\0");
    Ok(hash.finalize().into())
}

pub(crate) fn is_profile(context: &BatchContext) -> bool {
    context.business_program == program_id()
        && context.semantic_version == REGISTRATION_SEMANTIC_VERSION
        && context.receipt_codec == receipt_codec()
}

pub(crate) fn receipt(proof_id: NodeHash) -> Vec<u8> {
    let mut encoded = b"NUIE\x01".to_vec();
    encoded.extend_from_slice(&proof_id);
    encoded
}

/// A signature-checked registration plan bound to the resolved business profile.
pub struct RegistrationPlan {
    plan: BatchPlan,
    proof: VerifiedUcaRegistrationV1,
}

impl RegistrationPlan {
    pub fn prepare(
        raw: Vec<u8>,
        context: BatchContext,
        policy: &super::direct_nov_fee::DirectNovFeePolicy,
        budget: PlanBudget,
        owner: &AoemExecFacade,
    ) -> Result<Self> {
        ensure!(
            is_profile(&context) && context.effect_contract == effect_contract(policy)?,
            "registration business contract mismatch"
        );
        ensure!(
            raw.len() <= budget.transaction_bytes && raw.len() <= budget.body_bytes,
            "registration body exceeds budget"
        );
        let signed = SignedUcaRegistrationV1::decode(&raw)?;
        let proof = owner.verify_uca_registration_v1(
            signed,
            context.chain_id,
            context.genesis_config_commitment,
            context.height,
        )?;
        let plan = BatchPlan::new(
            context,
            vec![raw],
            registration_declarations(proof.intent())?,
            budget,
        )?;
        Ok(Self { plan, proof })
    }

    pub fn plan(&self) -> &BatchPlan {
        &self.plan
    }

    pub(crate) fn begin_capture(
        self,
        budget: CaptureBudget,
    ) -> Result<(BoundBatchCapture, VerifiedUcaRegistrationV1)> {
        Ok((self.plan.begin_capture(budget)?, self.proof))
    }
}

pub(super) fn execute_captured(
    proof: VerifiedUcaRegistrationV1,
    input: OwnedBatchInput,
) -> Result<ExecutedRegistration> {
    let changes = register_identity(&proof, input.state_input(), input.plan().context())?;
    Ok(ExecutedRegistration {
        effects: input.stage(&changes)?,
        receipt: receipt(proof.proof_id()),
    })
}

/// The archive may only accept a complete, signature-checked registration effect.
pub struct ExecutedRegistration {
    effects: UnpublishedBatchEffects,
    receipt: Vec<u8>,
}

impl ExecutedRegistration {
    pub(crate) fn parts(self) -> (UnpublishedBatchEffects, Vec<u8>) {
        (self.effects, self.receipt)
    }
}

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
