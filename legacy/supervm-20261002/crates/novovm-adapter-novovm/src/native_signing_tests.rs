//! Frozen pre-extraction encoder is a test oracle, never a production path.
use super::tests::sample_tx;
use super::*;
use novovm_adapter_api::{EvmAccessListEntryV1, TxExecutionPolicyV1};
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
fn legacy_signing_message_v1(tx: &TxIR) -> [u8; 32] {
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
        novovm_adapter_api::TxExecutionPolicyV1::Standard => 0,
        novovm_adapter_api::TxExecutionPolicyV1::PqRequired => 1,
        novovm_adapter_api::TxExecutionPolicyV1::PrivacyRequired => 2,
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

fn variations(base: &TxIR) -> Vec<TxIR> {
    let mut out = Vec::new();
    macro_rules! changed {
        ($field:ident, $value:expr) => {{
            let mut tx = base.clone();
            tx.$field = $value;
            out.push(tx);
        }};
    }
    changed!(hash, vec![42; 32]);
    changed!(from, vec![42; 20]);
    changed!(account_id, Some(String::new()));
    changed!(fee_owner_account_id, Some("fee".into()));
    changed!(nonce_owner_account_id, Some("nonce".into()));
    changed!(to, None);
    changed!(to, Some(Vec::new()));
    changed!(value, u128::MAX);
    changed!(gas_limit, u64::MAX);
    changed!(gas_price, u64::MAX);
    changed!(nonce, u64::MAX);
    changed!(data, vec![0, 1, 255]);
    changed!(chain_id, u64::MAX);
    changed!(source_chain, Some(0));
    changed!(target_chain, Some(u64::MAX));
    changed!(
        evm_access_list,
        vec![EvmAccessListEntryV1 {
            address: vec![1; 20],
            storage_keys: vec![vec![2; 32], Vec::new()]
        }]
    );
    for kind in [
        TxType::ContractCall,
        TxType::ContractDeploy,
        TxType::Privacy,
        TxType::CrossShard,
        TxType::CrossChainTransfer,
        TxType::CrossChainCall,
    ] {
        changed!(tx_type, kind);
    }
    for policy in [
        TxExecutionPolicyV1::PqRequired,
        TxExecutionPolicyV1::PrivacyRequired,
    ] {
        changed!(execution_policy, policy);
    }
    out
}

#[test]
fn shared_native_signing_preserves_legacy_messages_and_signatures() {
    let base = sample_tx(TxType::Transfer);
    let mut cases = variations(&base);
    cases.push(base);
    // Exercise every type/policy pair, including populated optional fields.
    let mut rich = sample_tx(TxType::Transfer);
    rich.evm_access_list = vec![EvmAccessListEntryV1 {
        address: vec![1; 20],
        storage_keys: vec![vec![2; 32], Vec::new()],
    }];
    for kind in [
        TxType::Transfer,
        TxType::ContractCall,
        TxType::ContractDeploy,
        TxType::Privacy,
        TxType::CrossShard,
        TxType::CrossChainTransfer,
        TxType::CrossChainCall,
    ] {
        for policy in [
            TxExecutionPolicyV1::Standard,
            TxExecutionPolicyV1::PqRequired,
            TxExecutionPolicyV1::PrivacyRequired,
        ] {
            let mut tx = rich.clone();
            tx.tx_type = kind;
            tx.execution_policy = policy;
            tx.account_id = Some("account".into());
            tx.fee_owner_account_id = Some(String::new());
            tx.nonce_owner_account_id = Some("nonce".into());
            tx.source_chain = Some(0);
            tx.target_chain = Some(u64::MAX);
            cases.push(tx);
        }
    }
    let key = SigningKey::from_bytes(&[7; 32]);
    for mut tx in cases {
        assert_eq!(tx_signing_message_v1(&tx), legacy_signing_message_v1(&tx));
        // An old encoder's actual signature must still verify when the caller is bound.
        tx.from = address_from_pubkey_v1(&key.verifying_key());
        let signature = key.sign(&legacy_signing_message_v1(&tx));
        tx.signature = key.verifying_key().to_bytes().to_vec();
        tx.signature.extend_from_slice(&signature.to_bytes());
        assert!(verify_native_tx_signature_v1(&tx).unwrap());
    }
}

#[test]
fn shared_native_signing_rejects_each_signed_field_tamper() {
    let tx = sample_tx(TxType::Transfer);
    assert!(verify_native_tx_signature_v1(&tx).unwrap());
    for tampered in variations(&tx) {
        assert_ne!(tx_signing_message_v1(&tx), tx_signing_message_v1(&tampered));
        assert!(!verify_native_tx_signature_v1(&tampered).unwrap());
    }
    let mut changed_signature = tx.clone();
    changed_signature.signature[40] ^= 1;
    assert_eq!(
        tx_signing_message_v1(&tx),
        tx_signing_message_v1(&changed_signature)
    );
    assert!(!verify_native_tx_signature_v1(&changed_signature).unwrap());
}

#[test]
fn shared_native_signer_binding_accepts_aliases_rejects_wrong_signer() {
    let key = SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes();
    for from in [address_from_seed_v1([7; 32]), key.to_vec()] {
        let mut tx = sample_tx(TxType::Transfer);
        tx.from = from;
        tx.signature = signature_payload_with_seed_v1(&tx, [7; 32]);
        assert!(verify_native_tx_signature_v1(&tx).unwrap());
        assert!(NovoVmAdapter::tx_from_matches_pubkey_bytes(&tx, &key));
    }
    for from in [
        vec![],
        vec![0; 19],
        vec![0; 20],
        vec![0; 31],
        vec![0; 32],
        vec![0; 33],
    ] {
        let mut tx = sample_tx(TxType::Transfer);
        tx.from = from;
        // Cryptographically valid signature but unauthorized from.
        tx.signature = signature_payload_with_seed_v1(&tx, [7; 32]);
        assert!(!verify_native_tx_signature_v1(&tx).unwrap());
        assert!(!NovoVmAdapter::tx_from_matches_pubkey_bytes(&tx, &key));
    }
    for len in [0, 32, 95, 97] {
        let mut tx = sample_tx(TxType::Transfer);
        tx.signature.resize(len, 0);
        assert!(!verify_native_tx_signature_v1(&tx).unwrap());
    }
}

#[test]
fn shared_native_signing_binds_access_list_contents_and_order() {
    let mut tx = sample_tx(TxType::Transfer);
    tx.evm_access_list = vec![
        EvmAccessListEntryV1 {
            address: vec![1; 20],
            storage_keys: vec![vec![2; 32], vec![3; 32]],
        },
        EvmAccessListEntryV1 {
            address: vec![4; 20],
            storage_keys: vec![],
        },
    ];
    tx.signature = signature_payload_with_seed_v1(&tx, [7; 32]);
    assert!(verify_native_tx_signature_v1(&tx).unwrap());
    for index in 0..5 {
        let mut changed = tx.clone();
        match index {
            0 => changed.evm_access_list[0].address[0] ^= 1,
            1 => changed.evm_access_list[0].storage_keys[0][0] ^= 1,
            2 => changed.evm_access_list[0].storage_keys.swap(0, 1),
            3 => changed.evm_access_list.swap(0, 1),
            _ => changed.evm_access_list[1].storage_keys.push(Vec::new()),
        }
        assert_eq!(
            tx_signing_message_v1(&changed),
            legacy_signing_message_v1(&changed)
        );
        assert!(!verify_native_tx_signature_v1(&changed).unwrap());
    }
}
