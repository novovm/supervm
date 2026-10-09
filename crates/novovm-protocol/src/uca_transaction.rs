//! Versioned identity-administration carrier. Decoding does not authenticate a
//! root, publish account state, consume a nonce, or enable native V4 admission.
//! Contains private UCA associations: never send it to chat peers or relays.
use crate::uca_delegation::{
    SignedUcaDelegationV1, UcaDelegationEncodingError, UcaDelegationPurposeV1, MAX_ENVELOPE_BYTES,
};
use crate::{
    decode_nov_native_tx_wire_v1, encode_nov_native_tx_wire_v1, native_tx_unsigned_commitment_v3,
    NativeTxWireError, NovExecutionTargetV1, NovNativeTxWireV1, NovTxKindV1,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const UCA_NATIVE_TX_CODEC_V4: &str = "novovm_uca_native_tx_wire_v4";
const MAGIC: &[u8; 5] = b"NNX1\x04";
const INNER_MAGIC: &[u8; 5] = b"NNX1\x03";
const DOMAIN: &[u8] = b"novovm-uca-native-transaction-v4\0";
pub const MAX_INNER_TX_BYTES: usize = 70 * 1024;
pub const MAX_TRANSACTION_BYTES: usize = 5 + 4 + MAX_INNER_TX_BYTES + 2 + MAX_ENVELOPE_BYTES;

#[derive(Debug, Error)]
pub enum UcaTransactionWireError {
    #[error("invalid UCA transaction framing or size")]
    Framing,
    #[error("UCA transaction and delegation binding mismatch")]
    Binding,
    #[error("invalid inner native transaction: {0}")]
    Native(#[from] NativeTxWireError),
    #[error("invalid UCA delegation: {0}")]
    Delegation(#[from] UcaDelegationEncodingError),
}
type Result<T> = std::result::Result<T, UcaTransactionWireError>;

/// Structurally checked only. Both signatures and independently authenticated
/// parent-state context still require verification by the native adapter.
/// No Debug/serde export: account/root associations are not chat metadata.
pub struct DecodedUcaTransactionV4 {
    transaction: NovNativeTxWireV1,
    delegation: SignedUcaDelegationV1,
    id: [u8; 32],
}
impl DecodedUcaTransactionV4 {
    pub fn transaction(&self) -> &NovNativeTxWireV1 {
        &self.transaction
    }
    pub fn delegation(&self) -> &SignedUcaDelegationV1 {
        &self.delegation
    }
    /// V4 wire identity, deliberately distinct from the inner V3 intent hash.
    pub fn transaction_id(&self) -> &[u8; 32] {
        &self.id
    }
}

fn validate_binding(tx: &NovNativeTxWireV1, proof: &SignedUcaDelegationV1) -> Result<()> {
    let claims = &proof.claims;
    claims.validate()?;
    let NovTxKindV1::Execute(exec) = &tx.kind else {
        return Err(UcaTransactionWireError::Binding);
    };
    let method = match claims.purpose {
        UcaDelegationPurposeV1::AuthorizeDevice => "authorize_device_v1",
        UcaDelegationPurposeV1::RevokeDevice => "revoke_device_v1",
    };
    // Bound local encode input too, before commitment construction/serialization.
    if exec.args.len() > 65_536
        || exec.fee_policy.pay_asset.len() > 128
        || tx.chain_id != claims.chain_id
        || exec.target != NovExecutionTargetV1::NativeModule("unified_account".into())
        || exec.method != method
        || exec.account_id.as_deref() != Some(claims.account_id.as_str())
        || exec.fee_owner_account_id.as_deref() != Some(claims.account_id.as_str())
        || exec.nonce_owner_account_id.as_deref() != Some(claims.account_id.as_str())
        || exec.caller != claims.delegate_public_key
        || tx.signature.len() != 96
        || tx.signature[..32] != claims.delegate_public_key
    {
        return Err(UcaTransactionWireError::Binding);
    }
    if native_tx_unsigned_commitment_v3(tx)? != claims.native_intent {
        return Err(UcaTransactionWireError::Binding);
    }
    Ok(())
}

/// Canonical layout: NNX1/4 || u32be(inner length) || canonical NNX1/3 tx
/// || u16be(delegation length) || canonical NUCD/1 delegation.
pub fn encode_uca_transaction_v4(
    tx: &NovNativeTxWireV1,
    proof: &SignedUcaDelegationV1,
) -> Result<Vec<u8>> {
    validate_binding(tx, proof)?;
    let inner = encode_nov_native_tx_wire_v1(tx)?;
    if inner.len() > MAX_INNER_TX_BYTES {
        return Err(UcaTransactionWireError::Framing);
    }
    let delegation = proof.encode()?;
    let mut bytes = Vec::with_capacity(11 + inner.len() + delegation.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&(inner.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&inner);
    bytes.extend_from_slice(&(delegation.len() as u16).to_be_bytes());
    bytes.extend_from_slice(&delegation);
    Ok(bytes)
}

pub fn decode_uca_transaction_v4(bytes: &[u8]) -> Result<DecodedUcaTransactionV4> {
    if bytes.len() < 11 || bytes.len() > MAX_TRANSACTION_BYTES || !bytes.starts_with(MAGIC) {
        return Err(UcaTransactionWireError::Framing);
    }
    let inner_len = u32::from_be_bytes(bytes[5..9].try_into().unwrap()) as usize;
    if !(5..=MAX_INNER_TX_BYTES).contains(&inner_len) {
        return Err(UcaTransactionWireError::Framing);
    }
    let inner_end = 9 + inner_len; // bounded above before arithmetic/slicing
    let length = bytes
        .get(inner_end..inner_end + 2)
        .ok_or(UcaTransactionWireError::Framing)?;
    let proof_len = u16::from_be_bytes(length.try_into().unwrap()) as usize;
    if proof_len > MAX_ENVELOPE_BYTES || inner_end + 2 + proof_len != bytes.len() {
        return Err(UcaTransactionWireError::Framing);
    }
    let inner = &bytes[9..inner_end];
    if !inner.starts_with(INNER_MAGIC) {
        return Err(UcaTransactionWireError::Framing);
    }
    let delegation = SignedUcaDelegationV1::decode(&bytes[inner_end + 2..])?;
    let transaction = decode_nov_native_tx_wire_v1(inner)?;
    validate_binding(&transaction, &delegation)?;
    let mut hash = Sha256::new();
    hash.update(DOMAIN);
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
    Ok(DecodedUcaTransactionV4 {
        transaction,
        delegation,
        id: hash.finalize().into(),
    })
}

#[cfg(test)]
#[path = "uca_transaction_tests.rs"]
mod tests;
