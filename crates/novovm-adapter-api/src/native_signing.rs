//! Shared TxIR signing bytes and signer binding; not a cryptographic verifier.
//! Keep the legacy v2 domain despite the historical v1 function name.
//! The signature payload itself is excluded; the supplied tx.hash IS committed.
//! Callers must still validate canonical transaction construction and signatures.
use crate::{TxIR, TxType};
use sha2::{Digest, Sha256};
const ADAPTER_TX_SIG_DOMAIN: &[u8] = b"novovm_adapter_tx_sig_v2";

fn tx_type_tag(tx_type: TxType) -> u8 {
    match tx_type {
        TxType::Transfer => 0,
        TxType::ContractCall => 1,
        TxType::ContractDeploy => 2,
        TxType::Privacy => 3,
        TxType::CrossShard => 4,
        TxType::CrossChainTransfer => 5,
        TxType::CrossChainCall => 6,
    }
}

pub fn tx_signing_message_v1(tx: &TxIR) -> [u8; 32] {
    fn update_len_prefixed(hasher: &mut Sha256, value: &[u8]) {
        hasher.update((value.len() as u64).to_le_bytes());
        hasher.update(value);
    }

    fn update_optional_string(hasher: &mut Sha256, value: Option<&str>) {
        match value {
            Some(value) => {
                hasher.update([1u8]);
                update_len_prefixed(hasher, value.as_bytes());
            }
            None => hasher.update([0u8]),
        }
    }

    let mut hasher = Sha256::new();
    hasher.update(ADAPTER_TX_SIG_DOMAIN);
    hasher.update(tx.chain_id.to_le_bytes());
    hasher.update([tx_type_tag(tx.tx_type)]);
    hasher.update(tx.nonce.to_le_bytes());
    hasher.update(tx.value.to_le_bytes());
    hasher.update(tx.gas_limit.to_le_bytes());
    hasher.update(tx.gas_price.to_le_bytes());
    update_len_prefixed(&mut hasher, &tx.from);
    update_optional_string(&mut hasher, tx.account_id.as_deref());
    update_optional_string(&mut hasher, tx.fee_owner_account_id.as_deref());
    update_optional_string(&mut hasher, tx.nonce_owner_account_id.as_deref());
    if let Some(to) = &tx.to {
        hasher.update([1u8]);
        update_len_prefixed(&mut hasher, to);
    } else {
        hasher.update([0u8]);
    }
    update_len_prefixed(&mut hasher, &tx.data);
    hasher.update([match tx.execution_policy {
        crate::TxExecutionPolicyV1::Standard => 0,
        crate::TxExecutionPolicyV1::PqRequired => 1,
        crate::TxExecutionPolicyV1::PrivacyRequired => 2,
    }]);
    hasher.update((tx.evm_access_list.len() as u64).to_le_bytes());
    for entry in &tx.evm_access_list {
        update_len_prefixed(&mut hasher, &entry.address);
        hasher.update((entry.storage_keys.len() as u64).to_le_bytes());
        for storage_key in &entry.storage_keys {
            update_len_prefixed(&mut hasher, storage_key);
        }
    }
    match tx.source_chain {
        Some(chain_id) => {
            hasher.update([1u8]);
            hasher.update(chain_id.to_le_bytes());
        }
        None => hasher.update([0u8]),
    }
    match tx.target_chain {
        Some(chain_id) => {
            hasher.update([1u8]);
            hasher.update(chain_id.to_le_bytes());
        }
        None => hasher.update([0u8]),
    }
    update_len_prefixed(&mut hasher, &tx.hash);
    hasher.finalize().into()
}

pub fn native_signer_address_v1(public_key: &[u8; 32]) -> [u8; 20] {
    let digest = Sha256::digest(public_key);
    digest[12..32]
        .try_into()
        .expect("fixed SHA256 address slice")
}

/// Only a binding check, to be used AFTER successful cryptographic verification.
pub fn native_signer_matches_v1(from: &[u8], public_key: &[u8; 32]) -> bool {
    match from.len() {
        20 => from == native_signer_address_v1(public_key),
        32 => from == public_key,
        _ => false,
    }
}
