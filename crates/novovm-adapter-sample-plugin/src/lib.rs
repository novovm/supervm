use novovm_adapter_api::{ChainConfig, ChainType, StateIR, TxIR, TxType};
use novovm_adapter_novovm::create_native_adapter;

pub const NOVOVM_ADAPTER_PLUGIN_ABI_V1: u32 = 1;
pub const NOVOVM_ADAPTER_PLUGIN_CAP_APPLY_IR_V1: u64 = 0x1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NovovmAdapterPluginApplyResultV1 {
    pub verified: u8,
    pub applied: u8,
    pub txs: u64,
    pub accounts: u64,
    pub state_root: [u8; 32],
    pub error_code: i32,
}

fn normalize_root32(root: &[u8]) -> [u8; 32] {
    if root.len() == 32 {
        let mut out = [0u8; 32];
        out.copy_from_slice(root);
        return out;
    }
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(root);
    hasher.finalize().into()
}

fn chain_type_from_code(code: u32) -> Option<ChainType> {
    Some(match code {
        0 => ChainType::NovoVM,
        1 => ChainType::EVM,
        6 => ChainType::BNB,
        13 => ChainType::Custom,
        _ => return None,
    })
}

fn apply_ir_batch(
    chain_type: ChainType,
    chain_id: u64,
    txs: &[TxIR],
) -> anyhow::Result<NovovmAdapterPluginApplyResultV1> {
    let config = ChainConfig {
        chain_type,
        chain_id,
        name: format!("plugin-{}", chain_type.as_str()),
        enabled: true,
        custom_config: None,
    };

    let mut adapter = create_native_adapter(config)?;
    adapter.initialize()?;

    let mut state = StateIR::new();
    let mut verified = true;
    let mut applied = true;
    for tx in txs {
        let tx_ok = adapter.verify_transaction(tx)?;
        verified = verified && tx_ok;
        if tx_ok {
            adapter.execute_transaction(tx, &mut state)?;
        } else {
            applied = false;
        }
    }

    let state_root = adapter.state_root()?;
    let accounts = state.accounts.len() as u64;
    adapter.shutdown()?;

    Ok(NovovmAdapterPluginApplyResultV1 {
        verified: u8::from(verified),
        applied: u8::from(applied),
        txs: txs.len() as u64,
        accounts,
        state_root: normalize_root32(&state_root),
        error_code: 0,
    })
}

#[no_mangle]
pub extern "C" fn novovm_adapter_plugin_version() -> u32 {
    NOVOVM_ADAPTER_PLUGIN_ABI_V1
}

#[no_mangle]
pub extern "C" fn novovm_adapter_plugin_capabilities() -> u64 {
    NOVOVM_ADAPTER_PLUGIN_CAP_APPLY_IR_V1
}

#[no_mangle]
pub unsafe extern "C" fn novovm_adapter_plugin_apply_v1(
    chain_type_code: u32,
    chain_id: u64,
    tx_ir_ptr: *const u8,
    tx_ir_len: usize,
    out_result: *mut NovovmAdapterPluginApplyResultV1,
) -> i32 {
    if tx_ir_ptr.is_null() || tx_ir_len == 0 || out_result.is_null() {
        return -1;
    }

    let chain_type = match chain_type_from_code(chain_type_code) {
        Some(v) => v,
        None => return -2,
    };

    let tx_bytes = std::slice::from_raw_parts(tx_ir_ptr, tx_ir_len);
    let txs: Vec<TxIR> = match bincode::deserialize(tx_bytes) {
        Ok(v) => v,
        Err(_) => return -3,
    };

    if txs.is_empty() {
        return -4;
    }

    if !txs.iter().all(|tx| tx.tx_type == TxType::Transfer) {
        return -5;
    }

    let result = match apply_ir_batch(chain_type, chain_id, &txs) {
        Ok(v) => v,
        Err(_) => return -6,
    };

    *out_result = result;
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_address(seed: u64) -> Vec<u8> {
        let mut out = vec![0u8; 20];
        out[12..20].copy_from_slice(&seed.to_be_bytes());
        out
    }

    fn sample_tx(chain_id: u64, nonce: u64) -> TxIR {
        let mut tx = TxIR {
            hash: Vec::new(),
            from: encode_address(1000),
            to: Some(encode_address(2000)),
            value: 5,
            gas_limit: 21_000,
            gas_price: 1,
            nonce,
            data: Vec::new(),
            signature: vec![2u8; 32],
            chain_id,
            tx_type: TxType::Transfer,
            source_chain: None,
            target_chain: None,
        };
        tx.compute_hash();
        tx
    }

    #[test]
    fn apply_ir_batch_smoke() {
        let txs = vec![sample_tx(20260303, 0), sample_tx(20260303, 1)];
        let result = apply_ir_batch(ChainType::NovoVM, 20260303, &txs).expect("apply should pass");
        assert_eq!(result.verified, 1);
        assert_eq!(result.applied, 1);
        assert_eq!(result.txs, 2);
        assert!(result.accounts >= 2);
    }

    #[test]
    fn chain_code_mapping_supports_non_novovm_samples() {
        assert_eq!(chain_type_from_code(0), Some(ChainType::NovoVM));
        assert_eq!(chain_type_from_code(1), Some(ChainType::EVM));
        assert_eq!(chain_type_from_code(6), Some(ChainType::BNB));
        assert_eq!(chain_type_from_code(13), Some(ChainType::Custom));
        assert_eq!(chain_type_from_code(999), None);
    }
}
