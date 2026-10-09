//! Strict one-intent delegation verification, NOT production admission.
//! The native state owner must supply the canonical parent snapshot and consume
//! the intent nonce atomically at execution. Never populate context from RPC JSON.
use anyhow::{ensure, Result};
use ed25519_dalek::{Signature, VerifyingKey};
use novovm_adapter_api::uca_key_binding::derive_primary_key_ref_from_binding_v1;
use novovm_adapter_api::unified_account::UcaAccount;
use novovm_adapter_api::{UcaKeyAlgo, UcaStatus};
use novovm_protocol::uca_delegation::{
    SignedUcaDelegationV1, UcaDelegationClaimsV1, UcaDelegationPurposeV1,
};
use novovm_protocol::uca_transaction::{decode_uca_transaction_v4, DecodedUcaTransactionV4};
use novovm_protocol::{
    native_tx_unsigned_commitment_v3, NovExecutionPolicyV1, NovExecutionTargetV1,
    NovNativeTxWireV1, NovTxKindV1,
};

/// Verified signatures and caller-supplied authenticated parent constraints.
/// Not a state transition, persisted nonce reservation or finality receipt.
pub struct VerifiedUcaTransactionV4 {
    decoded: DecodedUcaTransactionV4,
    proof: VerifiedUcaDelegationV1,
}
impl VerifiedUcaTransactionV4 {
    pub fn transaction_id(&self) -> &[u8; 32] {
        self.decoded.transaction_id()
    }
    pub fn transaction(&self) -> &NovNativeTxWireV1 {
        self.decoded.transaction()
    }
    pub fn native_intent(&self) -> &[u8; 32] {
        self.proof.native_intent()
    }
    pub fn parent_height(&self) -> u64 {
        self.proof.parent_height()
    }
}

/// Full-carrier preflight. Never strip a V4 envelope into the V3 admission path.
/// A canonical state owner must still recheck and consume the nonce atomically.
pub fn verify_uca_transaction_v4(
    wire: &[u8],
    context: &UcaDelegationContextV1<'_>,
) -> Result<VerifiedUcaTransactionV4> {
    let decoded = decode_uca_transaction_v4(wire)?;
    let proof = verify_uca_delegation_v1(
        &decoded.delegation().encode()?,
        decoded.transaction(),
        context,
    )?;
    Ok(VerifiedUcaTransactionV4 { decoded, proof })
}

/// LOCAL authenticated parent-state data. No Serialize/Deserialize: a peer
/// cannot establish authority by supplying its own "trusted" root and epoch.
pub struct UcaDelegationContextV1<'a> {
    pub account: &'a UcaAccount,
    pub delegation_epoch: u64,
    pub chain_id: u64,
    pub genesis: [u8; 32],
    pub parent_height: u64,
    pub next_nonce: u64,
    pub app_scope: [u8; 32],
}

/// A verified proof for one exact intent and parent snapshot, not a grant,
/// accepted transaction, durable replay marker, or finality receipt.
pub struct VerifiedUcaDelegationV1 {
    intent: [u8; 32],
    parent_height: u64,
}
impl VerifiedUcaDelegationV1 {
    pub fn native_intent(&self) -> &[u8; 32] {
        &self.intent
    }
    pub fn parent_height(&self) -> u64 {
        self.parent_height
    }
}

pub fn verify_uca_delegation_v1(
    envelope: &[u8],
    tx: &NovNativeTxWireV1,
    context: &UcaDelegationContextV1<'_>,
) -> Result<VerifiedUcaDelegationV1> {
    let proof = SignedUcaDelegationV1::decode(envelope)?;
    let claims = &proof.claims;
    let (root, intent) = validate_claims_and_device(claims, tx, context)?;
    root.verify_strict(
        &claims.signing_bytes()?,
        &Signature::from_bytes(&proof.signature),
    )?;
    Ok(VerifiedUcaDelegationV1 {
        intent,
        parent_height: context.parent_height,
    })
}

/// Local identity-owner operation. Validate context and the device signature
/// BEFORE requesting root approval. The signer must enforce its own consent and
/// purpose policy; no private key is accepted and no network request is made.
pub fn prepare_uca_delegation_v1(
    claims: UcaDelegationClaimsV1,
    tx: &NovNativeTxWireV1,
    context: &UcaDelegationContextV1<'_>,
    sign: impl FnOnce(&[u8]) -> Result<[u8; 64]>,
) -> Result<SignedUcaDelegationV1> {
    let (root, _) = validate_claims_and_device(&claims, tx, context)?;
    let message = claims.signing_bytes()?;
    let signature = sign(&message)?;
    root.verify_strict(&message, &Signature::from_bytes(&signature))?;
    Ok(SignedUcaDelegationV1 { claims, signature })
}

fn validate_claims_and_device(
    claims: &UcaDelegationClaimsV1,
    tx: &NovNativeTxWireV1,
    context: &UcaDelegationContextV1<'_>,
) -> Result<(VerifyingKey, [u8; 32])> {
    claims.validate()?;
    ensure!(
        matches!(context.account.status, UcaStatus::Active),
        "inactive UCA"
    );
    ensure!(
        context.chain_id == claims.chain_id
            && context.chain_id == tx.chain_id
            && context.genesis == claims.genesis
            && context.app_scope == claims.app_scope
            && context.account.uca_id == claims.account_id
            && context.delegation_epoch == claims.account_epoch,
        "delegation does not match canonical context"
    );
    ensure!(
        context.parent_height >= claims.not_before_height
            && context.parent_height < claims.expires_at_height,
        "delegation outside height window"
    );
    let binding = context
        .account
        .primary_key_binding
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("UCA binding unavailable"))?;
    ensure!(
        binding.key_algo == UcaKeyAlgo::Ed25519,
        "unsupported UCA delegation algorithm; no fallback"
    );
    ensure!(
        context.account.primary_key_ref
            == derive_primary_key_ref_from_binding_v1(binding.key_algo, &binding.public_key),
        "UCA root reference mismatch"
    );
    let root_bytes: [u8; 32] = binding.public_key.as_slice().try_into()?;
    let root = VerifyingKey::from_bytes(&root_bytes)?;
    ensure!(!root.is_weak(), "weak UCA authority key");
    let delegate = VerifyingKey::from_bytes(&claims.delegate_public_key)?;
    ensure!(!delegate.is_weak(), "weak administrative device key");

    let NovTxKindV1::Execute(execute) = &tx.kind else {
        anyhow::bail!("delegation is not spending authority");
    };
    ensure!(
        execute.args.len() <= 65_536 && execute.fee_policy.pay_asset.len() <= 128,
        "oversized delegation intent"
    );
    ensure!(
        execute.target == NovExecutionTargetV1::NativeModule("unified_account".into()),
        "wrong delegation module"
    );
    let method = match claims.purpose {
        UcaDelegationPurposeV1::AuthorizeDevice => "authorize_device_v1",
        UcaDelegationPurposeV1::RevokeDevice => "revoke_device_v1",
    };
    ensure!(execute.method == method, "wrong delegation purpose");
    ensure!(
        execute.execution_policy == NovExecutionPolicyV1::Standard,
        "unsupported security policy; no downgrade"
    );
    ensure!(
        execute.nonce == context.next_nonce && execute.nonce != u64::MAX,
        "delegation nonce replay or exhaustion"
    );
    ensure!(
        execute.account_id.as_deref() == Some(claims.account_id.as_str())
            && execute.fee_owner_account_id.as_deref() == Some(claims.account_id.as_str())
            && execute.nonce_owner_account_id.as_deref() == Some(claims.account_id.as_str()),
        "unbound delegation account/fee/nonce owner"
    );
    ensure!(
        execute.caller == claims.delegate_public_key
            && tx.signature.len() == 96
            && tx.signature[..32] == claims.delegate_public_key,
        "wrong administrative device signer"
    );
    let intent = native_tx_unsigned_commitment_v3(tx)?;
    ensure!(claims.native_intent == intent, "delegation intent mismatch");
    let ir = crate::native_intent::nov_native_tx_to_adapter_tx_ir_v1(tx)?;
    delegate.verify_strict(
        &novovm_adapter_api::native_signing::tx_signing_message_v1(&ir),
        &Signature::from_bytes(tx.signature[32..96].try_into()?),
    )?;
    // Parent/nonce must still be rechecked and consumed
    // atomically by the canonical state owner; this function performs no writes.
    Ok((root, intent))
}

#[cfg(test)]
#[path = "uca_delegation_tests.rs"]
mod tests;
