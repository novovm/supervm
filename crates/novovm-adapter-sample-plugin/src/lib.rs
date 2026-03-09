use novovm_adapter_api::{ChainConfig, ChainType, StateIR, TxIR, TxType};
use novovm_adapter_novovm::create_native_adapter;

pub const NOVOVM_ADAPTER_PLUGIN_ABI_V1: u32 = 1;
pub const NOVOVM_ADAPTER_PLUGIN_CAP_APPLY_IR_V1: u64 = 0x1;
pub const NOVOVM_ADAPTER_PLUGIN_CAP_UA_SELF_GUARD_V1: u64 = 0x2;
// Stable FFI return codes (external contract):
//  0=ok, -1=invalid_arg, -2=unsupported_chain, -3=decode_failed,
// -4=empty_batch, -5=unsupported_tx_type, -6=apply_failed,
// -8=payload_too_large, -9=batch_too_large.
pub const NOVOVM_ADAPTER_PLUGIN_RC_OK: i32 = 0;
pub const NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG: i32 = -1;
pub const NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_CHAIN: i32 = -2;
pub const NOVOVM_ADAPTER_PLUGIN_RC_DECODE_FAILED: i32 = -3;
pub const NOVOVM_ADAPTER_PLUGIN_RC_EMPTY_BATCH: i32 = -4;
pub const NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_TX_TYPE: i32 = -5;
pub const NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED: i32 = -6;
pub const NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE: i32 = -8;
pub const NOVOVM_ADAPTER_PLUGIN_RC_BATCH_TOO_LARGE: i32 = -9;
const MAX_PLUGIN_TX_IR_BYTES: usize = 16 * 1024 * 1024;
const MAX_PLUGIN_TX_COUNT: usize = 100_000;

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
        5 => ChainType::Polygon,
        6 => ChainType::BNB,
        7 => ChainType::Avalanche,
        13 => ChainType::Custom,
        _ => return None,
    })
}

fn decode_plugin_apply_inputs(
    chain_type_code: u32,
    tx_ir_ptr: *const u8,
    tx_ir_len: usize,
) -> Result<(ChainType, Vec<TxIR>), i32> {
    if tx_ir_ptr.is_null() || tx_ir_len == 0 {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG);
    }
    if tx_ir_len > MAX_PLUGIN_TX_IR_BYTES || tx_ir_len > isize::MAX as usize {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE);
    }

    let chain_type = match chain_type_from_code(chain_type_code) {
        Some(v) => v,
        None => return Err(NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_CHAIN),
    };

    let tx_bytes = unsafe { std::slice::from_raw_parts(tx_ir_ptr, tx_ir_len) };
    let txs: Vec<TxIR> = match bincode::deserialize(tx_bytes) {
        Ok(v) => v,
        Err(_) => return Err(NOVOVM_ADAPTER_PLUGIN_RC_DECODE_FAILED),
    };

    if txs.is_empty() {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_EMPTY_BATCH);
    }
    if txs.len() > MAX_PLUGIN_TX_COUNT {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_BATCH_TOO_LARGE);
    }
    if !txs.iter().all(|tx| tx.tx_type == TxType::Transfer) {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_TX_TYPE);
    }

    Ok((chain_type, txs))
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
    NOVOVM_ADAPTER_PLUGIN_CAP_APPLY_IR_V1 | NOVOVM_ADAPTER_PLUGIN_CAP_UA_SELF_GUARD_V1
}

#[no_mangle]
/// Apply a serialized `Vec<TxIR>` batch through the native adapter plugin ABI.
///
/// # Safety
/// - `tx_ir_ptr` must point to a readable buffer of `tx_ir_len` bytes for the full call.
/// - `out_result` must be a valid, writable pointer to `NovovmAdapterPluginApplyResultV1`.
/// - The pointed memory regions must not alias in a way that violates Rust aliasing rules.
pub unsafe extern "C" fn novovm_adapter_plugin_apply_v1(
    chain_type_code: u32,
    chain_id: u64,
    tx_ir_ptr: *const u8,
    tx_ir_len: usize,
    out_result: *mut NovovmAdapterPluginApplyResultV1,
) -> i32 {
    if out_result.is_null() {
        return NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG;
    }
    let (chain_type, txs) = match decode_plugin_apply_inputs(chain_type_code, tx_ir_ptr, tx_ir_len)
    {
        Ok(v) => v,
        Err(rc) => return rc,
    };

    let result = match apply_ir_batch(chain_type, chain_id, &txs) {
        Ok(v) => v,
        Err(_) => return NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED,
    };

    *out_result = result;
    NOVOVM_ADAPTER_PLUGIN_RC_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_adapter_novovm::{address_from_seed_v1, signature_payload_with_seed_v1};
    const TEST_SIGN_SEED: [u8; 32] = [11u8; 32];

    fn encode_address(seed: u64) -> Vec<u8> {
        let mut out = vec![0u8; 20];
        out[12..20].copy_from_slice(&seed.to_be_bytes());
        out
    }

    fn sample_tx(chain_id: u64, nonce: u64) -> TxIR {
        let mut tx = TxIR {
            hash: Vec::new(),
            from: address_from_seed_v1(TEST_SIGN_SEED),
            to: Some(encode_address(2000)),
            value: 5,
            gas_limit: 21_000,
            gas_price: 1,
            nonce,
            data: Vec::new(),
            signature: Vec::new(),
            chain_id,
            tx_type: TxType::Transfer,
            source_chain: None,
            target_chain: None,
        };
        tx.compute_hash();
        tx.signature = signature_payload_with_seed_v1(&tx, TEST_SIGN_SEED);
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
        assert_eq!(chain_type_from_code(5), Some(ChainType::Polygon));
        assert_eq!(chain_type_from_code(6), Some(ChainType::BNB));
        assert_eq!(chain_type_from_code(7), Some(ChainType::Avalanche));
        assert_eq!(chain_type_from_code(13), Some(ChainType::Custom));
        assert_eq!(chain_type_from_code(999), None);
    }

    #[test]
    fn plugin_capabilities_include_ua_self_guard_contract_bit() {
        let caps = novovm_adapter_plugin_capabilities();
        assert!(caps & NOVOVM_ADAPTER_PLUGIN_CAP_APPLY_IR_V1 != 0);
        assert!(caps & NOVOVM_ADAPTER_PLUGIN_CAP_UA_SELF_GUARD_V1 != 0);
    }

    #[test]
    fn plugin_return_codes_are_stable_contract() {
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_OK, 0);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG, -1);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_CHAIN, -2);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_DECODE_FAILED, -3);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_EMPTY_BATCH, -4);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_TX_TYPE, -5);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED, -6);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE, -8);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_BATCH_TOO_LARGE, -9);
    }

    #[test]
    fn plugin_apply_v1_rejects_invalid_chain_type() {
        let txs = vec![sample_tx(20260303, 0)];
        let tx_bytes = bincode::serialize(&txs).expect("tx encode");
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                999,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_CHAIN);
    }

    #[test]
    fn plugin_apply_v1_rejects_empty_batch() {
        let txs: Vec<TxIR> = Vec::new();
        let tx_bytes = bincode::serialize(&txs).expect("tx encode");
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                0,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_EMPTY_BATCH);
    }

    #[test]
    fn plugin_apply_v1_rejects_oversized_payload_before_decode() {
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let marker = [0u8; 1];
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                0,
                1,
                marker.as_ptr(),
                MAX_PLUGIN_TX_IR_BYTES + 1,
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE);
    }
}
