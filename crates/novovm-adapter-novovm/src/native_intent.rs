//! Canonical native V3 -> TxIR conversion, moved unchanged from the product node.
//! Shared by product ingress and direct clients; encoding is not admission.
use anyhow::{Context, Result};
use novovm_adapter_api::{TxExecutionPolicyV1, TxIR, TxType};
use novovm_protocol::{NovExecutionPolicyV1, NovExecutionTargetV1, NovNativeTxWireV1, NovTxKindV1};

fn pseudo_target_address_v1(target: &NovExecutionTargetV1, method: &str) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    match target {
        NovExecutionTargetV1::NativeModule(name) => {
            hasher.update(b"native:");
            hasher.update(name.as_bytes());
        }
        NovExecutionTargetV1::WasmApp(app_id) => {
            hasher.update(b"wasm:");
            hasher.update(app_id.as_bytes());
        }
        NovExecutionTargetV1::Plugin(plugin_id) => {
            hasher.update(b"plugin:");
            hasher.update(plugin_id.as_bytes());
        }
    }
    hasher.update(b":");
    hasher.update(method.as_bytes());
    let digest = hasher.finalize();
    digest[..20].to_vec()
}

fn tx_execution_policy_from_nov_v1(policy: NovExecutionPolicyV1) -> TxExecutionPolicyV1 {
    match policy {
        NovExecutionPolicyV1::Standard => TxExecutionPolicyV1::Standard,
        NovExecutionPolicyV1::PqRequired => TxExecutionPolicyV1::PqRequired,
        NovExecutionPolicyV1::PrivacyRequired => TxExecutionPolicyV1::PrivacyRequired,
    }
}

pub fn nov_native_tx_to_adapter_tx_ir_v1(tx: &NovNativeTxWireV1) -> Result<TxIR> {
    let mut ir = match &tx.kind {
        NovTxKindV1::Transfer(transfer) => TxIR {
            hash: Vec::new(),
            from: transfer.from.clone(),
            account_id: None,
            fee_owner_account_id: None,
            nonce_owner_account_id: None,
            to: Some(transfer.to.clone()),
            value: transfer.amount,
            gas_limit: 21_000,
            gas_price: 1,
            nonce: transfer.nonce,
            data: transfer.asset.as_bytes().to_vec(),
            signature: tx.signature.to_vec(),
            chain_id: tx.chain_id,
            tx_type: TxType::Transfer,
            execution_policy: TxExecutionPolicyV1::Standard,
            evm_access_list: Vec::new(),
            source_chain: None,
            target_chain: None,
        },
        NovTxKindV1::Execute(execute) => {
            let target_addr = pseudo_target_address_v1(&execute.target, &execute.method);
            TxIR {
                hash: Vec::new(),
                from: execute.caller.clone(),
                account_id: execute.account_id.clone(),
                fee_owner_account_id: execute.fee_owner_account_id.clone(),
                nonce_owner_account_id: execute.nonce_owner_account_id.clone(),
                to: Some(target_addr),
                value: 0,
                gas_limit: execute.gas_like_limit.unwrap_or(300_000),
                gas_price: 1,
                nonce: execute.nonce,
                data: execute.args.clone(),
                signature: tx.signature.to_vec(),
                chain_id: tx.chain_id,
                tx_type: TxType::ContractCall,
                execution_policy: tx_execution_policy_from_nov_v1(execute.execution_policy),
                evm_access_list: Vec::new(),
                source_chain: None,
                target_chain: None,
            }
        }
        NovTxKindV1::Governance(governance) => TxIR {
            hash: Vec::new(),
            from: governance.proposer.clone(),
            account_id: None,
            fee_owner_account_id: None,
            nonce_owner_account_id: None,
            to: None,
            value: 0,
            gas_limit: 80_000,
            gas_price: 1,
            nonce: governance.nonce,
            data: governance.payload.clone(),
            signature: tx.signature.to_vec(),
            chain_id: tx.chain_id,
            tx_type: TxType::Privacy,
            execution_policy: TxExecutionPolicyV1::Standard,
            evm_access_list: Vec::new(),
            source_chain: None,
            target_chain: None,
        },
    };
    let signed_intent_commitment = novovm_protocol::native_tx_unsigned_commitment_v3(tx)
        .context("compute native signed-intent commitment failed")?;
    let original_data = std::mem::take(&mut ir.data);
    ir.data = Vec::with_capacity(33 + signed_intent_commitment.len() + original_data.len());
    ir.data
        .extend_from_slice(b"novovm-native-signed-intent-v3\0");
    ir.data.extend_from_slice(&signed_intent_commitment);
    ir.data.extend_from_slice(original_data.as_slice());
    ir.compute_hash();
    Ok(ir)
}
