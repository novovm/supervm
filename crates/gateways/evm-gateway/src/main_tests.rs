use super::*;
use std::cell::Cell;
use web30_core::privacy::generate_ring_keypair;

fn capture_env_vars(keys: &[&str]) -> Vec<(String, Option<String>)> {
    keys.iter()
        .map(|key| ((*key).to_string(), std::env::var(key).ok()))
        .collect()
}

fn restore_env_vars(captured: &[(String, Option<String>)]) {
    for (key, value) in captured {
        if let Some(value) = value {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }
}

thread_local! {
    static ENV_LOCK_DEPTH: Cell<u32> = const { Cell::new(0) };
}

struct EnvTestGuard {
    _guard: Option<std::sync::MutexGuard<'static, ()>>,
}

impl Drop for EnvTestGuard {
    fn drop(&mut self) {
        ENV_LOCK_DEPTH.with(|depth| {
            let current = depth.get();
            depth.set(current.saturating_sub(1));
        });
    }
}

fn env_test_guard() -> EnvTestGuard {
    let should_lock = ENV_LOCK_DEPTH.with(|depth| {
        let current = depth.get();
        depth.set(current.saturating_add(1));
        current == 0
    });
    let guard = if should_lock {
        Some(match super::gateway_env_mutex().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        })
    } else {
        None
    };
    if should_lock {
        reset_runtime_host_state_for_test();
    }
    EnvTestGuard { _guard: guard }
}

fn reset_runtime_host_state_for_test() {
    let drain_max = 1_000_000usize;
    loop {
        let executable = super::drain_executable_ingress_frames_for_host(drain_max);
        let pending = super::drain_pending_ingress_frames_for_host(drain_max);
        let settlement = super::drain_settlement_records_for_host(drain_max);
        let payout = super::drain_payout_instructions_for_host(drain_max);
        let atomic_ready = super::drain_atomic_broadcast_ready_for_host(drain_max);
        let atomic_receipts = super::drain_atomic_receipts_for_host(drain_max);
        if executable.is_empty()
            && pending.is_empty()
            && settlement.is_empty()
            && payout.is_empty()
            && atomic_ready.is_empty()
            && atomic_receipts.is_empty()
        {
            break;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_gateway_method(
    router: &mut UnifiedAccountRouter,
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    evm_settlement_index_by_id: &mut HashMap<String, GatewayEvmSettlementIndexEntry>,
    evm_settlement_index_by_tx: &mut HashMap<GatewaySettlementTxKey, String>,
    evm_pending_payout_by_settlement: &mut HashMap<String, EvmFeePayoutInstructionV1>,
    ctx: &mut GatewayMethodContext<'_>,
    method: &str,
    params: &serde_json::Value,
) -> Result<(serde_json::Value, bool)> {
    let _guard = env_test_guard();
    super::run_gateway_method(
        router,
        eth_tx_index,
        evm_settlement_index_by_id,
        evm_settlement_index_by_tx,
        evm_pending_payout_by_settlement,
        ctx,
        method,
        params,
    )
}

fn runtime_tap_ir_batch_v1(
    chain_type: novovm_adapter_api::ChainType,
    chain_id: u64,
    txs: &[TxIR],
    flags: u64,
) -> std::result::Result<novovm_adapter_evm_plugin::EvmRuntimeTapSummaryV1, i32> {
    let _guard = env_test_guard();
    super::runtime_tap_ir_batch_v1(chain_type, chain_id, txs, flags)
}

fn test_rlp_encode_len(prefix_small: u8, prefix_long: u8, len: usize) -> Vec<u8> {
    if len < 56 {
        return vec![prefix_small + len as u8];
    }
    let mut len_bytes = Vec::new();
    let mut n = len;
    while n > 0 {
        len_bytes.push((n & 0xff) as u8);
        n >>= 8;
    }
    len_bytes.reverse();
    let mut out = Vec::with_capacity(1 + len_bytes.len());
    out.push(prefix_long + len_bytes.len() as u8);
    out.extend_from_slice(&len_bytes);
    out
}

fn test_rlp_encode_bytes(bytes: &[u8]) -> Vec<u8> {
    if bytes.len() == 1 && bytes[0] < 0x80 {
        return vec![bytes[0]];
    }
    let mut out = test_rlp_encode_len(0x80, 0xb7, bytes.len());
    out.extend_from_slice(bytes);
    out
}

fn test_rlp_encode_u64(v: u64) -> Vec<u8> {
    if v == 0 {
        return test_rlp_encode_bytes(&[]);
    }
    let bytes = v.to_be_bytes();
    let first_non_zero = bytes
        .iter()
        .position(|value| *value != 0)
        .unwrap_or(bytes.len() - 1);
    test_rlp_encode_bytes(&bytes[first_non_zero..])
}

fn test_rlp_encode_u128(v: u128) -> Vec<u8> {
    if v == 0 {
        return test_rlp_encode_bytes(&[]);
    }
    let bytes = v.to_be_bytes();
    let first_non_zero = bytes
        .iter()
        .position(|value| *value != 0)
        .unwrap_or(bytes.len() - 1);
    test_rlp_encode_bytes(&bytes[first_non_zero..])
}

fn test_rlp_encode_list(items: &[Vec<u8>]) -> Vec<u8> {
    let payload_len: usize = items.iter().map(Vec::len).sum();
    let mut out = test_rlp_encode_len(0xc0, 0xf7, payload_len);
    for item in items {
        out.extend_from_slice(item);
    }
    out
}

fn decode_single_ops_wire_value(bytes: &[u8]) -> Result<Vec<u8>> {
    const HEADER_LEN: usize = 5 + 2 + 2 + 4;
    if bytes.len() < HEADER_LEN {
        bail!("ops-wire too short");
    }
    if &bytes[..5] != b"AOV2\0" {
        bail!("ops-wire magic mismatch");
    }
    let op_count = u32::from_le_bytes([bytes[9], bytes[10], bytes[11], bytes[12]]) as usize;
    if op_count != 1 {
        bail!("expected exactly one op, got {}", op_count);
    }
    let mut off = HEADER_LEN;
    if bytes.len() < off + 36 {
        bail!("ops-wire op header too short");
    }
    let key_len = u32::from_le_bytes([
        bytes[off + 4],
        bytes[off + 5],
        bytes[off + 6],
        bytes[off + 7],
    ]) as usize;
    let value_len = u32::from_le_bytes([
        bytes[off + 8],
        bytes[off + 9],
        bytes[off + 10],
        bytes[off + 11],
    ]) as usize;
    off += 36;
    if bytes.len() < off + key_len + value_len {
        bail!(
            "ops-wire payload truncated: off={} key_len={} value_len={} bytes={}",
            off,
            key_len,
            value_len,
            bytes.len()
        );
    }
    off += key_len;
    Ok(bytes[off..off + value_len].to_vec())
}

fn aoem_privacy_env_available() -> bool {
    string_env_nonempty("NOVOVM_AOEM_DLL")
        .or_else(|| string_env_nonempty("AOEM_DLL"))
        .or_else(|| string_env_nonempty("AOEM_FFI_DLL"))
        .is_some()
}

#[test]
fn parse_gateway_web30_privacy_plan_reads_required_fields() {
    let params = serde_json::json!({
        "privacy": {
            "value": "0x11",
            "gas_limit": "0x5208",
            "gas_price": "0x2",
            "view_key": format!("0x{}", "11".repeat(32)),
            "spend_key": format!("0x{}", "22".repeat(32)),
            "ring_members": [
                format!("0x{}", "33".repeat(32))
            ],
            "signer_index": 0,
            "private_key": format!("0x{}", "44".repeat(32)),
        }
    });
    let plan = parse_gateway_web30_privacy_plan(&params)
        .expect("parse privacy plan")
        .expect("privacy plan should exist");
    assert_eq!(plan.value, 0x11);
    assert_eq!(plan.gas_limit, 0x5208);
    assert_eq!(plan.gas_price, 0x2);
    assert_eq!(plan.ring_members.len(), 1);
    assert_eq!(plan.signer_index, 0);
}

#[test]
fn eth_chain_id_and_net_version_accept_chain_params() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-chain-id-net-version-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let (chain_default, changed_chain_default) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_chainId",
        &serde_json::json!({}),
    )
    .expect("eth_chainId default should work");
    assert!(!changed_chain_default);
    assert_eq!(chain_default.as_str(), Some("0x1"));

    let (chain_explicit, changed_chain_explicit) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_chainId",
        &serde_json::json!({ "chain_id": 56u64 }),
    )
    .expect("eth_chainId explicit chain_id should work");
    assert!(!changed_chain_explicit);
    assert_eq!(chain_explicit.as_str(), Some("0x38"));

    let (chain_tx_nested, changed_chain_tx_nested) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_chainId",
        &serde_json::json!({ "tx": { "chainId": 137u64 } }),
    )
    .expect("eth_chainId tx.chainId should work");
    assert!(!changed_chain_tx_nested);
    assert_eq!(chain_tx_nested.as_str(), Some("0x89"));

    let (net_default, changed_net_default) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "net_version",
        &serde_json::json!({}),
    )
    .expect("net_version default should work");
    assert!(!changed_net_default);
    assert_eq!(net_default.as_str(), Some("1"));

    let (net_explicit, changed_net_explicit) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "net_version",
        &serde_json::json!({ "chainId": 10u64 }),
    )
    .expect("net_version explicit chainId should work");
    assert!(!changed_net_explicit);
    assert_eq!(net_explicit.as_str(), Some("10"));

    let (net_tx_nested, changed_net_tx_nested) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "net_version",
        &serde_json::json!({ "tx": { "chain_id": 42161u64 } }),
    )
    .expect("net_version tx.chain_id should work");
    assert!(!changed_net_tx_nested);
    assert_eq!(net_tx_nested.as_str(), Some("42161"));

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_tx_hash_queries_respect_default_chain_scope_unless_overridden() {
    let _guard = env_test_guard();
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-tx-hash-chain-scope-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    // Indexed path: tx exists, but on non-default chain.
    let indexed_hash = [0xabu8; 32];
    let indexed_entry = GatewayEthTxIndexEntry {
        tx_hash: indexed_hash,
        uca_id: "uca-indexed-foreign-chain".to_string(),
        chain_id: 10,
        nonce: 3,
        tx_type: 0,
        from: vec![0x11; 20],
        to: Some(vec![0x22; 20]),
        value: 1,
        gas_limit: 21_000,
        gas_price: 2,
        input: vec![],
    };
    eth_tx_index.insert(indexed_hash, indexed_entry.clone());

    let (indexed_default_tx, changed_indexed_default_tx) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByHash",
        &serde_json::json!({
            "tx_hash": format!("0x{}", to_hex(&indexed_hash)),
        }),
    )
    .expect("indexed tx query on default chain should work");
    assert!(!changed_indexed_default_tx);
    assert!(indexed_default_tx.is_null());

    let (indexed_explicit_tx, changed_indexed_explicit_tx) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByHash",
        &serde_json::json!({
            "tx_hash": format!("0x{}", to_hex(&indexed_hash)),
            "chain_id": 10u64,
        }),
    )
    .expect("indexed tx query with explicit chain_id should work");
    assert!(!changed_indexed_explicit_tx);
    assert_eq!(indexed_explicit_tx["pending"].as_bool(), Some(false));

    let (indexed_default_receipt, changed_indexed_default_receipt) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionReceipt",
        &serde_json::json!({
            "tx_hash": format!("0x{}", to_hex(&indexed_hash)),
        }),
    )
    .expect("indexed receipt query on default chain should work");
    assert!(!changed_indexed_default_receipt);
    assert!(indexed_default_receipt.is_null());

    let (indexed_explicit_receipt, changed_indexed_explicit_receipt) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionReceipt",
        &serde_json::json!({
            "tx_hash": format!("0x{}", to_hex(&indexed_hash)),
            "chainId": 10u64,
        }),
    )
    .expect("indexed receipt query with explicit chainId should work");
    assert!(!changed_indexed_explicit_receipt);
    assert_eq!(indexed_explicit_receipt["pending"].as_bool(), Some(false));
    assert_eq!(indexed_explicit_receipt["status"].as_str(), Some("0x1"));

    // Runtime path: tx exists only in runtime txpool, on non-default chain.
    let runtime_chain_id = 42161u64;
    let mut runtime_tx = TxIR::transfer(vec![0x31; 20], vec![0x42; 20], 3, 7, runtime_chain_id);
    runtime_tx.compute_hash();
    let tap_summary = runtime_tap_ir_batch_v1(
        novovm_adapter_api::ChainType::EVM,
        runtime_chain_id,
        &[runtime_tx.clone()],
        0,
    )
    .expect("runtime tap should accept tx");
    assert_eq!(tap_summary.accepted, 1);

    let runtime_hash_hex = format!("0x{}", to_hex(&runtime_tx.hash));
    let (runtime_default_tx, changed_runtime_default_tx) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByHash",
        &serde_json::json!({
            "tx_hash": runtime_hash_hex,
        }),
    )
    .expect("runtime tx query on default chain should work");
    assert!(!changed_runtime_default_tx);
    assert!(runtime_default_tx.is_null());

    let runtime_hash_hex = format!("0x{}", to_hex(&runtime_tx.hash));
    let (runtime_explicit_tx, changed_runtime_explicit_tx) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByHash",
        &serde_json::json!({
            "tx_hash": runtime_hash_hex,
            "chain_id": runtime_chain_id,
        }),
    )
    .expect("runtime tx query with explicit chain_id should work");
    assert!(!changed_runtime_explicit_tx);
    assert_eq!(runtime_explicit_tx["pending"].as_bool(), Some(true));

    let runtime_hash_hex = format!("0x{}", to_hex(&runtime_tx.hash));
    let (runtime_default_receipt, changed_runtime_default_receipt) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionReceipt",
        &serde_json::json!({
            "tx_hash": runtime_hash_hex,
        }),
    )
    .expect("runtime receipt query on default chain should work");
    assert!(!changed_runtime_default_receipt);
    assert!(runtime_default_receipt.is_null());

    let runtime_hash_hex = format!("0x{}", to_hex(&runtime_tx.hash));
    let (runtime_explicit_receipt, changed_runtime_explicit_receipt) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionReceipt",
        &serde_json::json!({
            "tx_hash": runtime_hash_hex,
            "tx": { "chainId": runtime_chain_id },
        }),
    )
    .expect("runtime receipt query with tx.chainId should work");
    assert!(!changed_runtime_explicit_receipt);
    assert_eq!(runtime_explicit_receipt["pending"].as_bool(), Some(true));
    assert!(runtime_explicit_receipt["status"].is_null());
    assert!(runtime_explicit_receipt["blockNumber"].is_string());
    assert!(runtime_explicit_receipt["blockHash"].is_string());
    assert!(runtime_explicit_receipt["transactionIndex"].is_string());

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn web30_send_transaction_privacy_spools_signed_tx_ir_when_aoem_available() {
    if !aoem_privacy_env_available() {
        return;
    }
    let (decoy_pub, _decoy_secret) = match generate_ring_keypair() {
        Ok(v) => v,
        Err(_) => return,
    };
    let (real_pub, real_secret) = match generate_ring_keypair() {
        Ok(v) => v,
        Err(_) => return,
    };
    let from_hex = format!("0x{}", to_hex(&real_pub));
    let decoy_hex = format!("0x{}", to_hex(&decoy_pub));
    let real_hex = format!("0x{}", to_hex(&real_pub));
    let secret_hex = format!("0x{}", to_hex(&real_secret));
    let mut router = UnifiedAccountRouter::new();
    router
        .create_uca("uca-privacy".to_string(), vec![0xabu8; 32], 10)
        .expect("create uca");
    router
        .add_binding(
            "uca-privacy",
            AccountRole::Owner,
            PersonaAddress {
                persona_type: PersonaType::Web30,
                chain_id: 20260303,
                external_address: real_pub.to_vec(),
            },
            11,
        )
        .expect("bind web30 persona");
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-privacy-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let params = serde_json::json!({
        "uca_id": "uca-privacy",
        "role": "owner",
        "chain_id": 20260303u64,
        "nonce": 0u64,
        "external_address": from_hex,
        "privacy": {
            "value": "0x9",
            "gas_limit": "0x5208",
            "gas_price": "0x1",
            "view_key": format!("0x{}", "55".repeat(32)),
            "spend_key": format!("0x{}", "66".repeat(32)),
            "ring_members": [decoy_hex, real_hex],
            "signer_index": 1u64,
            "private_key": secret_hex,
        }
    });
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &GatewayEthTxIndexStoreBackend::Memory,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };
    let (response, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "web30_sendTransaction",
        &params,
    )
    .expect("web30 privacy send should succeed");
    assert!(changed);
    assert_eq!(
        response["payload_kind"].as_str(),
        Some("signed_privacy_tx_ir_bincode_v1")
    );
    assert_eq!(response["tx_ir_type"].as_str(), Some("privacy"));
    let spool_file = PathBuf::from(
        response["spool_file"]
            .as_str()
            .expect("spool_file should be present"),
    );
    let wire = fs::read(&spool_file).expect("read spool ops-wire");
    let value = decode_single_ops_wire_value(&wire).expect("decode ops-wire value");
    let record: GatewayIngressWeb30RecordV1 =
        bincode::deserialize(&value).expect("decode web30 ingress record");
    let tx = TxIR::deserialize(&record.payload, SerializationFormat::Bincode)
        .expect("decode signed privacy tx ir");
    assert_eq!(tx.tx_type, TxType::Privacy);
    assert!(tx.to.is_none());
    assert!(!tx.signature.is_empty());
    assert_eq!(tx.chain_id, 20260303);
    assert_eq!(tx.nonce, 0);
    assert_eq!(tx.value, 9);
    assert_eq!(record.tx_hash.to_vec(), tx.hash);
    let _ = fs::remove_file(&spool_file);
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn encode_gateway_evm_payout_ops_wire_tracks_instruction_count() {
    let instructions = vec![
        EvmFeePayoutInstructionV1 {
            settlement_id: "evm-settlement-0001".to_string(),
            chain_id: 1,
            income_tx_hash: vec![1u8; 32],
            reserve_currency_code: "ETH".to_string(),
            payout_token_code: "NOVO".to_string(),
            reserve_delta_wei: 21_000,
            payout_delta_units: 21_000,
            reserve_account: vec![0x11; 20],
            payout_account: vec![0x22; 20],
            generated_at_unix_ms: 1,
        },
        EvmFeePayoutInstructionV1 {
            settlement_id: "evm-settlement-0002".to_string(),
            chain_id: 137,
            income_tx_hash: vec![2u8; 32],
            reserve_currency_code: "MATIC".to_string(),
            payout_token_code: "NOVO".to_string(),
            reserve_delta_wei: 42_000,
            payout_delta_units: 42_000,
            reserve_account: vec![0x33; 20],
            payout_account: vec![0x44; 20],
            generated_at_unix_ms: 2,
        },
    ];
    let wire = encode_gateway_ingress_ops_wire_v1_evm_payout(&instructions)
        .expect("encode payout ops wire should succeed");
    assert!(wire.len() > 13);
    assert_eq!(&wire[..5], b"AOV2\0");
    let op_count = u32::from_le_bytes([wire[9], wire[10], wire[11], wire[12]]) as usize;
    assert_eq!(op_count, instructions.len() * 6);
}

#[test]
fn encode_gateway_evm_atomic_ready_ops_wire_tracks_record_count() {
    let mut leg_a = TxIR::transfer(vec![0x11; 20], vec![0x22; 20], 1, 1, 1);
    leg_a.compute_hash();
    let mut leg_b = TxIR::transfer(vec![0x33; 20], vec![0x44; 20], 2, 2, 1);
    leg_b.compute_hash();
    let ready_items = vec![
        AtomicBroadcastReadyV1 {
            intent: novovm_adapter_api::AtomicCrossChainIntentV1 {
                intent_id: "intent-0001".to_string(),
                source_chain: novovm_adapter_api::ChainType::EVM,
                destination_chain: novovm_adapter_api::ChainType::NovoVM,
                ttl_unix_ms: 1_900_000_001_000,
                legs: vec![leg_a],
            },
            ready_at_unix_ms: 1_900_000_000_001,
        },
        AtomicBroadcastReadyV1 {
            intent: novovm_adapter_api::AtomicCrossChainIntentV1 {
                intent_id: "intent-0002".to_string(),
                source_chain: novovm_adapter_api::ChainType::EVM,
                destination_chain: novovm_adapter_api::ChainType::NovoVM,
                ttl_unix_ms: 1_900_000_002_000,
                legs: vec![leg_b],
            },
            ready_at_unix_ms: 1_900_000_000_002,
        },
    ];
    let wire = encode_gateway_ops_wire_v1_evm_atomic_ready(&ready_items)
        .expect("encode atomic-ready ops wire should succeed");
    assert!(wire.len() > 13);
    assert_eq!(&wire[..5], b"AOV2\0");
    let op_count = u32::from_le_bytes([wire[9], wire[10], wire[11], wire[12]]) as usize;
    assert_eq!(op_count, ready_items.len());
}

#[test]
fn encode_gateway_evm_atomic_broadcast_queue_ops_wire_tracks_record_count() {
    let tickets = vec![
        GatewayEvmAtomicBroadcastTicketV1 {
            intent_id: "intent-bq-0001".to_string(),
            chain_id: 1,
            tx_hash: [0x11; 32],
            ready_at_unix_ms: 1_900_000_000_001,
        },
        GatewayEvmAtomicBroadcastTicketV1 {
            intent_id: "intent-bq-0002".to_string(),
            chain_id: 137,
            tx_hash: [0x22; 32],
            ready_at_unix_ms: 1_900_000_000_002,
        },
    ];
    let wire = encode_gateway_ops_wire_v1_evm_atomic_broadcast_queue(&tickets)
        .expect("encode atomic-broadcast queue ops wire should succeed");
    assert!(wire.len() > 13);
    assert_eq!(&wire[..5], b"AOV2\0");
    let op_count = u32::from_le_bytes([wire[9], wire[10], wire[11], wire[12]]) as usize;
    assert_eq!(op_count, tickets.len());
}

#[test]
fn encode_gateway_evm_settlement_ops_wire_tracks_record_count() {
    let records = vec![
        EvmFeeSettlementRecordV1 {
            income: novovm_adapter_api::EvmFeeIncomeRecordV1 {
                chain_id: 1,
                tx_hash: vec![1u8; 32],
                fee_amount_wei: 21_000,
                collector_address: vec![0x11; 20],
            },
            result: novovm_adapter_api::EvmFeeSettlementResultV1 {
                reserve_delta: 21_000,
                payout_delta: 21_000,
                settlement_id: "evm-settlement-0001".to_string(),
            },
            settled_at_unix_ms: 1,
        },
        EvmFeeSettlementRecordV1 {
            income: novovm_adapter_api::EvmFeeIncomeRecordV1 {
                chain_id: 137,
                tx_hash: vec![2u8; 32],
                fee_amount_wei: 42_000,
                collector_address: vec![0x22; 20],
            },
            result: novovm_adapter_api::EvmFeeSettlementResultV1 {
                reserve_delta: 42_000,
                payout_delta: 42_000,
                settlement_id: "evm-settlement-0002".to_string(),
            },
            settled_at_unix_ms: 2,
        },
    ];
    let wire = encode_gateway_ops_wire_v1_evm_settlement_records(&records)
        .expect("encode settlement ops wire should succeed");
    assert!(wire.len() > 13);
    assert_eq!(&wire[..5], b"AOV2\0");
    let op_count = u32::from_le_bytes([wire[9], wire[10], wire[11], wire[12]]) as usize;
    assert_eq!(op_count, records.len() * 4);
}

#[test]
fn evm_settlement_query_methods_hit_in_memory_index() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let record = EvmFeeSettlementRecordV1 {
        income: novovm_adapter_api::EvmFeeIncomeRecordV1 {
            chain_id: 1,
            tx_hash: vec![0xabu8; 32],
            fee_amount_wei: 21_000,
            collector_address: vec![0x11; 20],
        },
        result: novovm_adapter_api::EvmFeeSettlementResultV1 {
            reserve_delta: 21_000,
            payout_delta: 20_000,
            settlement_id: "evm-settlement-query-0001".to_string(),
        },
        settled_at_unix_ms: 123456,
    };
    upsert_gateway_evm_settlement_index(
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &backend,
        &record,
    )
    .expect("upsert settlement index");
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-settlement-query-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let (by_id, changed_by_id) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "evm_getSettlementById",
        &serde_json::json!({
            "settlement_id": "evm-settlement-query-0001",
        }),
    )
    .expect("query by settlement id");
    assert!(!changed_by_id);
    assert_eq!(
        by_id["settlement_id"].as_str(),
        Some("evm-settlement-query-0001")
    );
    assert_eq!(by_id["status"].as_str(), Some("settled_v1"));

    let (by_tx, changed_by_tx) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "evm_getSettlementByTxHash",
        &serde_json::json!({
            "chain_id": 1u64,
            "tx_hash": format!("0x{}", "ab".repeat(32)),
        }),
    )
    .expect("query by tx hash");
    assert!(!changed_by_tx);
    assert_eq!(
        by_tx["settlement_id"].as_str(),
        Some("evm-settlement-query-0001")
    );
    assert_eq!(by_tx["reserve_delta_wei"].as_str(), Some("0x5208"));
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn evm_replay_settlement_payout_clears_pending_and_updates_status() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let settlement = EvmFeeSettlementRecordV1 {
        income: novovm_adapter_api::EvmFeeIncomeRecordV1 {
            chain_id: 1,
            tx_hash: vec![0xcdu8; 32],
            fee_amount_wei: 31_000,
            collector_address: vec![0x11; 20],
        },
        result: novovm_adapter_api::EvmFeeSettlementResultV1 {
            reserve_delta: 31_000,
            payout_delta: 30_000,
            settlement_id: "evm-settlement-replay-0001".to_string(),
        },
        settled_at_unix_ms: 123_999,
    };
    upsert_gateway_evm_settlement_index(
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &backend,
        &settlement,
    )
    .expect("upsert settlement index");
    set_gateway_evm_settlement_status(
        &mut evm_settlement_index_by_id,
        &backend,
        "evm-settlement-replay-0001",
        EVM_SETTLEMENT_STATUS_COMPENSATE_PENDING_V1,
    );
    let pending_instruction = EvmFeePayoutInstructionV1 {
        settlement_id: "evm-settlement-replay-0001".to_string(),
        chain_id: 1,
        income_tx_hash: vec![0xcdu8; 32],
        reserve_currency_code: "ETH".to_string(),
        payout_token_code: "NOVO".to_string(),
        reserve_delta_wei: 31_000,
        payout_delta_units: 30_000,
        reserve_account: vec![0x11; 20],
        payout_account: vec![0x22; 20],
        generated_at_unix_ms: 123_999,
    };
    mark_gateway_pending_payout(
        &mut evm_pending_payout_by_settlement,
        &backend,
        &pending_instruction,
    );
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-settlement-replay-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };
    let (replayed, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "evm_replaySettlementPayout",
        &serde_json::json!({
            "settlement_id": "evm-settlement-replay-0001",
        }),
    )
    .expect("replay settlement payout");
    assert!(!changed);
    assert_eq!(replayed["replayed"].as_bool(), Some(true));
    assert_eq!(
        replayed["settlement_id"].as_str(),
        Some("evm-settlement-replay-0001")
    );
    assert!(!evm_pending_payout_by_settlement.contains_key("evm-settlement-replay-0001"));
    let status = evm_settlement_index_by_id
        .get("evm-settlement-replay-0001")
        .map(|entry| entry.status.as_str());
    assert_eq!(status, Some(EVM_SETTLEMENT_STATUS_COMPENSATED_V1));
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_query_block_number_balance_and_block_by_number_work() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-query-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };
    let addr_a = vec![0xaau8; 20];
    let addr_b = vec![0xbbu8; 20];
    eth_tx_index.insert(
        [0x11u8; 32],
        GatewayEthTxIndexEntry {
            tx_hash: [0x11u8; 32],
            uca_id: "uca-a".to_string(),
            chain_id: 1,
            nonce: 7,
            tx_type: 0,
            from: addr_a.clone(),
            to: Some(addr_b.clone()),
            value: 100,
            gas_limit: 21_000,
            gas_price: 1,
            input: Vec::new(),
        },
    );
    eth_tx_index.insert(
        [0x22u8; 32],
        GatewayEthTxIndexEntry {
            tx_hash: [0x22u8; 32],
            uca_id: "uca-b".to_string(),
            chain_id: 1,
            nonce: 8,
            tx_type: 0,
            from: addr_b.clone(),
            to: Some(addr_a.clone()),
            value: 30,
            gas_limit: 21_000,
            gas_price: 1,
            input: Vec::new(),
        },
    );

    let (block_number, changed_block_number) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_blockNumber",
        &serde_json::json!({ "chain_id": 1u64 }),
    )
    .expect("eth_blockNumber should work");
    assert!(!changed_block_number);
    assert_eq!(block_number.as_str(), Some("0x8"));

    let (balance, changed_balance) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBalance",
        &serde_json::json!({
            "chain_id": 1u64,
            "address": format!("0x{}", to_hex(&addr_a)),
        }),
    )
    .expect("eth_getBalance should work");
    assert!(!changed_balance);
    assert_eq!(balance.as_str(), Some("0x1e"));

    let (block_obj, changed_block_obj) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockByNumber",
        &serde_json::json!({
            "chain_id": 1u64,
            "block_number": "0x8",
            "full_transactions": false,
        }),
    )
    .expect("eth_getBlockByNumber should work");
    assert!(!changed_block_obj);
    assert_eq!(block_obj["number"].as_str(), Some("0x8"));
    let txs = block_obj["transactions"]
        .as_array()
        .expect("transactions should be array");
    assert_eq!(txs.len(), 1);
    let expected_hash = format!("0x{}", "22".repeat(32));
    assert_eq!(txs[0].as_str(), Some(expected_hash.as_str()));

    let (block_latest, changed_block_latest) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockByNumber",
        &serde_json::json!(["latest", true]),
    )
    .expect("eth_getBlockByNumber latest should work");
    assert!(!changed_block_latest);
    assert_eq!(block_latest["number"].as_str(), Some("0x8"));
    let txs_full = block_latest["transactions"]
        .as_array()
        .expect("full transactions should be array");
    assert_eq!(txs_full.len(), 1);
    assert_eq!(txs_full[0]["hash"].as_str(), Some(expected_hash.as_str()));
    assert_eq!(block_latest["gasUsed"].as_str(), Some("0x5208"));
    let expected_gas_limit = format!("0x{:x}", gateway_eth_fee_history_block_gas_limit());
    assert_eq!(
        block_latest["gasLimit"].as_str(),
        Some(expected_gas_limit.as_str())
    );
    let expected_base_fee = format!("0x{:x}", gateway_eth_base_fee_per_gas_wei());
    assert_eq!(
        block_latest["baseFeePerGas"].as_str(),
        Some(expected_base_fee.as_str())
    );
    assert_eq!(
        block_latest["sha3Uncles"].as_str(),
        Some(GATEWAY_ETH_EMPTY_UNCLES_HASH)
    );
    for key in ["transactionsRoot", "stateRoot", "receiptsRoot"] {
        let value = block_latest[key]
            .as_str()
            .expect("root field should be string");
        assert!(value.starts_with("0x"));
        assert_eq!(value.len(), 66);
    }

    let (block_safe, changed_block_safe) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockByNumber",
        &serde_json::json!(["safe", false]),
    )
    .expect("eth_getBlockByNumber safe should work");
    assert!(!changed_block_safe);
    assert_eq!(block_safe["number"].as_str(), Some("0x8"));

    let (block_finalized, changed_block_finalized) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockByNumber",
        &serde_json::json!(["finalized", false]),
    )
    .expect("eth_getBlockByNumber finalized should work");
    assert!(!changed_block_finalized);
    assert_eq!(block_finalized["number"].as_str(), Some("0x8"));

    let (tx_count_safe, changed_tx_count_safe) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockTransactionCountByNumber",
        &serde_json::json!({
            "chain_id": 1u64,
            "block": "safe",
        }),
    )
    .expect("eth_getBlockTransactionCountByNumber safe should work");
    assert!(!changed_tx_count_safe);
    assert_eq!(tx_count_safe.as_str(), Some("0x1"));

    let (tx_count_finalized, changed_tx_count_finalized) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockTransactionCountByNumber",
        &serde_json::json!({
            "chain_id": 1u64,
            "block": "finalized",
        }),
    )
    .expect("eth_getBlockTransactionCountByNumber finalized should work");
    assert!(!changed_tx_count_finalized);
    assert_eq!(tx_count_finalized.as_str(), Some("0x1"));

    let logs_bloom = block_finalized["logsBloom"]
        .as_str()
        .expect("logsBloom should be string");
    assert_eq!(logs_bloom.len(), 514);
    assert!(logs_bloom.starts_with("0x"));
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_query_block_by_hash_tx_by_block_index_and_logs_work() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-query-extended-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };
    let addr_a = vec![0xa1u8; 20];
    let addr_b = vec![0xb2u8; 20];
    let addr_c = vec![0xc3u8; 20];
    let tx_31 = GatewayEthTxIndexEntry {
        tx_hash: [0x31u8; 32],
        uca_id: "uca-a".to_string(),
        chain_id: 1,
        nonce: 9,
        tx_type: 0,
        from: addr_a.clone(),
        to: Some(addr_b.clone()),
        value: 11,
        gas_limit: 21_000,
        gas_price: 1,
        input: vec![0xaa],
    };
    let tx_32 = GatewayEthTxIndexEntry {
        tx_hash: [0x32u8; 32],
        uca_id: "uca-b".to_string(),
        chain_id: 1,
        nonce: 9,
        tx_type: 0,
        from: addr_b.clone(),
        to: Some(addr_a.clone()),
        value: 7,
        gas_limit: 21_000,
        gas_price: 1,
        input: vec![0xbb],
    };
    let tx_41 = GatewayEthTxIndexEntry {
        tx_hash: [0x41u8; 32],
        uca_id: "uca-c".to_string(),
        chain_id: 1,
        nonce: 10,
        tx_type: 0,
        from: addr_c,
        to: Some(addr_b.clone()),
        value: 5,
        gas_limit: 21_000,
        gas_price: 1,
        input: vec![0xcc],
    };
    eth_tx_index.insert(tx_31.tx_hash, tx_31.clone());
    eth_tx_index.insert(tx_32.tx_hash, tx_32.clone());
    eth_tx_index.insert(tx_41.tx_hash, tx_41);

    let block_hash = gateway_eth_block_hash_for_txs(1, 9, &[tx_31.clone(), tx_32.clone()]);
    let block_hash_hex = format!("0x{}", to_hex(&block_hash));

    let (block_by_hash, changed_block) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockByHash",
        &serde_json::json!([block_hash_hex, true]),
    )
    .expect("eth_getBlockByHash should work");
    assert!(!changed_block);
    assert_eq!(block_by_hash["number"].as_str(), Some("0x9"));
    let txs_full = block_by_hash["transactions"]
        .as_array()
        .expect("transactions should be array");
    assert_eq!(txs_full.len(), 2);
    assert_eq!(txs_full[0]["transactionIndex"].as_str(), Some("0x0"));
    assert_eq!(txs_full[1]["transactionIndex"].as_str(), Some("0x1"));

    let (tx_by_block_index, changed_tx_idx) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByBlockNumberAndIndex",
        &serde_json::json!(["0x9", "0x1"]),
    )
    .expect("eth_getTransactionByBlockNumberAndIndex should work");
    assert!(!changed_tx_idx);
    let expected_hash = format!("0x{}", "32".repeat(32));
    assert_eq!(
        tx_by_block_index["hash"].as_str(),
        Some(expected_hash.as_str())
    );
    assert_eq!(tx_by_block_index["blockNumber"].as_str(), Some("0x9"));
    assert_eq!(tx_by_block_index["transactionIndex"].as_str(), Some("0x1"));

    let (tx_by_block_hash_index, changed_tx_hash_idx) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByBlockHashAndIndex",
        &serde_json::json!([format!("0x{}", to_hex(&block_hash)), "0x0"]),
    )
    .expect("eth_getTransactionByBlockHashAndIndex should work");
    assert!(!changed_tx_hash_idx);
    let expected_hash0 = format!("0x{}", "31".repeat(32));
    assert_eq!(
        tx_by_block_hash_index["hash"].as_str(),
        Some(expected_hash0.as_str())
    );
    assert_eq!(
        tx_by_block_hash_index["transactionIndex"].as_str(),
        Some("0x0")
    );

    let (block_tx_count_by_number, changed_count_num) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockTransactionCountByNumber",
        &serde_json::json!(["0x9"]),
    )
    .expect("eth_getBlockTransactionCountByNumber should work");
    assert!(!changed_count_num);
    assert_eq!(block_tx_count_by_number.as_str(), Some("0x2"));

    let (block_tx_count_empty_by_number, changed_count_empty_num) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockTransactionCountByNumber",
        &serde_json::json!(["0x0"]),
    )
    .expect("eth_getBlockTransactionCountByNumber empty block should work");
    assert!(!changed_count_empty_num);
    assert_eq!(block_tx_count_empty_by_number.as_str(), Some("0x0"));

    let (block_tx_count_future_by_number, changed_count_future_num) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockTransactionCountByNumber",
        &serde_json::json!(["0x63"]),
    )
    .expect("eth_getBlockTransactionCountByNumber future block should work");
    assert!(!changed_count_future_num);
    assert!(block_tx_count_future_by_number.is_null());

    let (block_tx_count_by_hash, changed_count_hash) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockTransactionCountByHash",
        &serde_json::json!([format!("0x{}", to_hex(&block_hash))]),
    )
    .expect("eth_getBlockTransactionCountByHash should work");
    assert!(!changed_count_hash);
    assert_eq!(block_tx_count_by_hash.as_str(), Some("0x2"));

    let (block_receipts, changed_block_receipts) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockReceipts",
        &serde_json::json!([format!("0x{}", to_hex(&block_hash))]),
    )
    .expect("eth_getBlockReceipts should work");
    assert!(!changed_block_receipts);
    let receipts = block_receipts
        .as_array()
        .expect("block receipts should be array");
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0]["blockNumber"].as_str(), Some("0x9"));
    assert_eq!(receipts[1]["transactionIndex"].as_str(), Some("0x1"));
    assert_eq!(receipts[0]["cumulativeGasUsed"].as_str(), Some("0x5208"));
    assert_eq!(receipts[1]["cumulativeGasUsed"].as_str(), Some("0xa410"));

    let (tx_by_hash, changed_tx_by_hash) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByHash",
        &serde_json::json!([format!("0x{}", "31".repeat(32))]),
    )
    .expect("eth_getTransactionByHash should work");
    assert!(!changed_tx_by_hash);
    assert_eq!(tx_by_hash["blockNumber"].as_str(), Some("0x9"));
    assert_eq!(tx_by_hash["transactionIndex"].as_str(), Some("0x0"));
    assert_eq!(tx_by_hash["pending"].as_bool(), Some(false));

    let (tx_by_hash_unknown, changed_tx_by_hash_unknown) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByHash",
        &serde_json::json!([format!("0x{}", "dd".repeat(32))]),
    )
    .expect("eth_getTransactionByHash unknown hash should work");
    assert!(!changed_tx_by_hash_unknown);
    assert!(tx_by_hash_unknown.is_null());

    let (receipt_by_hash, changed_receipt_by_hash) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionReceipt",
        &serde_json::json!([format!("0x{}", "31".repeat(32))]),
    )
    .expect("eth_getTransactionReceipt should work");
    assert!(!changed_receipt_by_hash);
    assert_eq!(receipt_by_hash["blockNumber"].as_str(), Some("0x9"));
    assert_eq!(receipt_by_hash["transactionIndex"].as_str(), Some("0x0"));
    assert_eq!(receipt_by_hash["status"].as_str(), Some("0x1"));
    assert_eq!(receipt_by_hash["pending"].as_bool(), Some(false));
    assert_eq!(
        receipt_by_hash["cumulativeGasUsed"].as_str(),
        Some("0x5208")
    );

    let (receipt_unknown_hash, changed_receipt_unknown_hash) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionReceipt",
        &serde_json::json!([format!("0x{}", "ee".repeat(32))]),
    )
    .expect("eth_getTransactionReceipt unknown hash should work");
    assert!(!changed_receipt_unknown_hash);
    assert!(receipt_unknown_hash.is_null());

    let (block_receipts_empty_block, changed_block_receipts_empty_block) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockReceipts",
        &serde_json::json!(["0x0"]),
    )
    .expect("eth_getBlockReceipts empty block should work");
    assert!(!changed_block_receipts_empty_block);
    assert_eq!(
        block_receipts_empty_block
            .as_array()
            .map(std::vec::Vec::len),
        Some(0)
    );

    let (block_receipts_unknown_hash, changed_block_receipts_unknown_hash) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockReceipts",
        &serde_json::json!([format!("0x{}", "ff".repeat(32))]),
    )
    .expect("eth_getBlockReceipts unknown hash should work");
    assert!(!changed_block_receipts_unknown_hash);
    assert!(block_receipts_unknown_hash.is_null());

    let (block_receipts_future_block, changed_block_receipts_future_block) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockReceipts",
        &serde_json::json!(["0x63"]),
    )
    .expect("eth_getBlockReceipts future block should work");
    assert!(!changed_block_receipts_future_block);
    assert!(block_receipts_future_block.is_null());

    let (uncle_count_by_number, changed_uncle_number) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getUncleCountByBlockNumber",
        &serde_json::json!(["0x9"]),
    )
    .expect("eth_getUncleCountByBlockNumber should work");
    assert!(!changed_uncle_number);
    assert_eq!(uncle_count_by_number.as_str(), Some("0x0"));

    let (uncle_count_empty_by_number, changed_uncle_empty_number) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getUncleCountByBlockNumber",
        &serde_json::json!(["0x0"]),
    )
    .expect("eth_getUncleCountByBlockNumber empty block should work");
    assert!(!changed_uncle_empty_number);
    assert_eq!(uncle_count_empty_by_number.as_str(), Some("0x0"));

    let (uncle_count_by_hash, changed_uncle_hash) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getUncleCountByBlockHash",
        &serde_json::json!([format!("0x{}", to_hex(&block_hash))]),
    )
    .expect("eth_getUncleCountByBlockHash should work");
    assert!(!changed_uncle_hash);
    assert_eq!(uncle_count_by_hash.as_str(), Some("0x0"));

    let (block_zero, changed_block_zero) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockByNumber",
        &serde_json::json!(["0x0", false]),
    )
    .expect("eth_getBlockByNumber empty block should work");
    assert!(!changed_block_zero);
    assert_eq!(block_zero["number"].as_str(), Some("0x0"));
    assert_eq!(
        block_zero["transactions"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(0)
    );

    let (uncle_by_number, changed_uncle_by_number) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getUncleByBlockNumberAndIndex",
        &serde_json::json!(["0x9", "0x0"]),
    )
    .expect("eth_getUncleByBlockNumberAndIndex should work");
    assert!(!changed_uncle_by_number);
    assert!(uncle_by_number.is_null());

    let (uncle_by_hash, changed_uncle_by_hash) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getUncleByBlockHashAndIndex",
        &serde_json::json!([format!("0x{}", to_hex(&block_hash)), "0x0"]),
    )
    .expect("eth_getUncleByBlockHashAndIndex should work");
    assert!(!changed_uncle_by_hash);
    assert!(uncle_by_hash.is_null());

    let (syncing, changed_syncing) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_syncing",
        &serde_json::json!({}),
    )
    .expect("eth_syncing should work");
    assert!(!changed_syncing);
    assert!(
        syncing.is_boolean() || syncing.is_object(),
        "eth_syncing should be bool or progress object"
    );

    let (pending_txs, changed_pending_txs) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_pendingTransactions",
        &serde_json::json!({}),
    )
    .expect("eth_pendingTransactions should work");
    assert!(!changed_pending_txs);
    assert_eq!(pending_txs.as_array().map(std::vec::Vec::len), Some(0));

    let (client_version, changed_client_version) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "web3_clientVersion",
        &serde_json::json!({}),
    )
    .expect("web3_clientVersion should work");
    assert!(!changed_client_version);
    assert!(client_version
        .as_str()
        .expect("clientVersion should be string")
        .starts_with("novovm-evm-gateway/"));

    let (web3_sha3_from_array, changed_web3_sha3_from_array) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "web3_sha3",
        &serde_json::json!(["0x68656c6c6f20776f726c64"]),
    )
    .expect("web3_sha3 array params should work");
    assert!(!changed_web3_sha3_from_array);
    assert_eq!(
        web3_sha3_from_array.as_str(),
        Some("0x47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad")
    );

    let (web3_sha3_from_object, changed_web3_sha3_from_object) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "web3_sha3",
        &serde_json::json!({
            "data": "0x68656c6c6f20776f726c64",
        }),
    )
    .expect("web3_sha3 object params should work");
    assert!(!changed_web3_sha3_from_object);
    assert_eq!(
        web3_sha3_from_object.as_str(),
        Some("0x47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad")
    );

    let (protocol_version, changed_protocol_version) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_protocolVersion",
        &serde_json::json!({}),
    )
    .expect("eth_protocolVersion should work");
    assert!(!changed_protocol_version);
    assert!(protocol_version
        .as_str()
        .expect("protocolVersion should be string")
        .starts_with("0x"));

    let (net_listening, changed_net_listening) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "net_listening",
        &serde_json::json!({}),
    )
    .expect("net_listening should work");
    assert!(!changed_net_listening);
    assert_eq!(net_listening.as_bool(), Some(true));

    let (net_peer_count, changed_net_peer_count) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "net_peerCount",
        &serde_json::json!({}),
    )
    .expect("net_peerCount should work");
    assert!(!changed_net_peer_count);
    let peer_count = net_peer_count
        .as_str()
        .expect("net_peerCount should be hex string");
    assert!(peer_count.starts_with("0x"));

    let (eth_call_result, changed_eth_call) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_call",
        &serde_json::json!([
            {
                "from": format!("0x{}", to_hex(&addr_a)),
                "to": format!("0x{}", to_hex(&addr_b)),
                "data": "0x1234",
            },
            "latest"
        ]),
    )
    .expect("eth_call should work");
    assert!(!changed_eth_call);
    assert_eq!(eth_call_result.as_str(), Some("0x"));

    let (accounts, changed_accounts) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_accounts",
        &serde_json::json!({}),
    )
    .expect("eth_accounts should work");
    assert!(!changed_accounts);
    assert!(accounts.is_array());

    let (coinbase, changed_coinbase) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_coinbase",
        &serde_json::json!({}),
    )
    .expect("eth_coinbase should work");
    assert!(!changed_coinbase);
    let coinbase_str = coinbase.as_str().expect("coinbase should be string");
    assert!(coinbase_str.starts_with("0x"));
    assert_eq!(coinbase_str.len(), 42);

    let (mining, changed_mining) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_mining",
        &serde_json::json!({}),
    )
    .expect("eth_mining should work");
    assert!(!changed_mining);
    assert_eq!(mining.as_bool(), Some(false));

    let (hashrate, changed_hashrate) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_hashrate",
        &serde_json::json!({}),
    )
    .expect("eth_hashrate should work");
    assert!(!changed_hashrate);
    assert_eq!(hashrate.as_str(), Some("0x0"));

    let (max_priority_fee, changed_priority_fee) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_maxPriorityFeePerGas",
        &serde_json::json!({}),
    )
    .expect("eth_maxPriorityFeePerGas should work");
    assert!(!changed_priority_fee);
    assert!(max_priority_fee
        .as_str()
        .expect("maxPriorityFeePerGas should be string")
        .starts_with("0x"));

    let (fee_history, changed_fee_history) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_feeHistory",
        &serde_json::json!([2, "latest", [10.0, 50.0]]),
    )
    .expect("eth_feeHistory should work");
    assert!(!changed_fee_history);
    assert_eq!(fee_history["oldestBlock"].as_str(), Some("0x9"));
    assert_eq!(
        fee_history["baseFeePerGas"].as_array().map(|v| v.len()),
        Some(3)
    );
    let expected_base_fee = format!("0x{:x}", gateway_eth_base_fee_per_gas_wei());
    let fee_history_base_fees = fee_history["baseFeePerGas"]
        .as_array()
        .expect("baseFeePerGas should be array");
    assert!(
        fee_history_base_fees
            .iter()
            .all(|v| v.as_str() == Some(expected_base_fee.as_str())),
        "eth_feeHistory.baseFeePerGas should share block base fee source"
    );
    assert_eq!(
        fee_history["gasUsedRatio"].as_array().map(|v| v.len()),
        Some(2)
    );
    assert_eq!(fee_history["reward"].as_array().map(|v| v.len()), Some(2));

    let (logs_all, changed_logs_all) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getLogs",
        &serde_json::json!({
            "fromBlock": "0x9",
            "toBlock": "0x9",
        }),
    )
    .expect("eth_getLogs should work");
    assert!(!changed_logs_all);
    let logs_all_arr = logs_all.as_array().expect("logs should be array");
    assert_eq!(logs_all_arr.len(), 2);

    let (logs_by_address, changed_logs_addr) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getLogs",
        &serde_json::json!({
            "fromBlock": "0x9",
            "toBlock": "0x9",
            "address": format!("0x{}", to_hex(&addr_b)),
        }),
    )
    .expect("eth_getLogs address filter should work");
    assert!(!changed_logs_addr);
    let logs_by_address_arr = logs_by_address.as_array().expect("logs should be array");
    assert_eq!(logs_by_address_arr.len(), 1);

    let topic_hash = format!("0x{}", "32".repeat(32));
    let (logs_by_topic, changed_logs_topic) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getLogs",
        &serde_json::json!({
            "blockHash": format!("0x{}", to_hex(&block_hash)),
            "topics": [topic_hash],
        }),
    )
    .expect("eth_getLogs topic filter should work");
    assert!(!changed_logs_topic);
    let logs_by_topic_arr = logs_by_topic.as_array().expect("logs should be array");
    assert_eq!(logs_by_topic_arr.len(), 1);
    assert_eq!(
        logs_by_topic_arr[0]["transactionHash"].as_str(),
        Some(expected_hash.as_str())
    );

    let topic_hash_31 = format!("0x{}", "31".repeat(32));
    let (logs_by_topic_or, changed_logs_topic_or) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getLogs",
        &serde_json::json!({
            "blockHash": format!("0x{}", to_hex(&block_hash)),
            "topics": [[topic_hash_31, topic_hash]],
        }),
    )
    .expect("eth_getLogs topic[0] OR filter should work");
    assert!(!changed_logs_topic_or);
    assert_eq!(logs_by_topic_or.as_array().map(std::vec::Vec::len), Some(2));

    let (logs_by_topic_with_wildcard_second, changed_logs_topic_with_wildcard_second) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getLogs",
            &serde_json::json!({
                "blockHash": format!("0x{}", to_hex(&block_hash)),
                "topics": [topic_hash, null],
            }),
        )
        .expect("eth_getLogs topic wildcard second slot should work");
    assert!(!changed_logs_topic_with_wildcard_second);
    assert_eq!(
        logs_by_topic_with_wildcard_second
            .as_array()
            .map(std::vec::Vec::len),
        Some(1)
    );

    let (logs_by_unmatched_second_topic, changed_logs_unmatched_second_topic) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getLogs",
        &serde_json::json!({
            "blockHash": format!("0x{}", to_hex(&block_hash)),
            "topics": [topic_hash, [topic_hash]],
        }),
    )
    .expect("eth_getLogs second topic strict filter should work");
    assert!(!changed_logs_unmatched_second_topic);
    assert_eq!(
        logs_by_unmatched_second_topic
            .as_array()
            .map(std::vec::Vec::len),
        Some(0)
    );

    let err = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getLogs",
        &serde_json::json!({
            "blockHash": format!("0x{}", to_hex(&block_hash)),
            "topics": [topic_hash, 1],
        }),
    )
    .expect_err("eth_getLogs should reject non-string topic slot entry");
    assert!(err.to_string().contains("topics[1]"));
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_filter_and_txpool_methods_work_with_tx_index_state() {
    let _guard = env_test_guard();
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-filter-txpool-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };
    let addr_a = vec![0xaau8; 20];
    let addr_b = vec![0xbbu8; 20];
    let addr_c = vec![0xccu8; 20];
    eth_tx_index.insert(
        [0x11u8; 32],
        GatewayEthTxIndexEntry {
            tx_hash: [0x11u8; 32],
            uca_id: "uca-a".to_string(),
            chain_id: 1,
            nonce: 1,
            tx_type: 0,
            from: addr_a.clone(),
            to: Some(addr_b.clone()),
            value: 12,
            gas_limit: 21_000,
            gas_price: 2,
            input: vec![0x01],
        },
    );
    eth_tx_index.insert(
        [0x22u8; 32],
        GatewayEthTxIndexEntry {
            tx_hash: [0x22u8; 32],
            uca_id: "uca-c".to_string(),
            chain_id: 1,
            nonce: 2,
            tx_type: 0,
            from: addr_c.clone(),
            to: Some(addr_b.clone()),
            value: 7,
            gas_limit: 21_000,
            gas_price: 1,
            input: vec![0x02],
        },
    );

    let (txpool_content, changed_txpool_content) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "txpool_content",
        &serde_json::json!({ "chain_id": 1u64 }),
    )
    .expect("txpool_content should work");
    assert!(!changed_txpool_content);
    assert_eq!(
        txpool_content["pending"].as_object().map(|m| m.len()),
        Some(0)
    );
    assert_eq!(
        txpool_content["queued"].as_object().map(|m| m.len()),
        Some(0)
    );

    let (txpool_status, changed_txpool_status) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "txpool_status",
        &serde_json::json!({ "chain_id": 1u64 }),
    )
    .expect("txpool_status should work");
    assert!(!changed_txpool_status);
    assert_eq!(txpool_status["pending"].as_str(), Some("0x0"));
    assert_eq!(txpool_status["queued"].as_str(), Some("0x0"));

    let (txpool_inspect, changed_txpool_inspect) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "txpool_inspect",
        &serde_json::json!({ "chain_id": 1u64 }),
    )
    .expect("txpool_inspect should work");
    assert!(!changed_txpool_inspect);
    assert_eq!(
        txpool_inspect["pending"].as_object().map(|m| m.len()),
        Some(0)
    );
    assert_eq!(
        txpool_inspect["queued"].as_object().map(|m| m.len()),
        Some(0)
    );

    // Prefer plugin runtime txpool snapshots when available:
    // executable => pending, nonce-gap => queued.
    let runtime_chain_id = 993_377u64;
    let mut tx_exec = TxIR::transfer(addr_a.clone(), addr_b.clone(), 9, 1, runtime_chain_id);
    tx_exec.compute_hash();
    let mut tx_queued = TxIR::transfer(addr_a.clone(), addr_b.clone(), 11, 3, runtime_chain_id);
    tx_queued.compute_hash();
    let tap_summary = runtime_tap_ir_batch_v1(
        novovm_adapter_api::ChainType::EVM,
        runtime_chain_id,
        &[tx_exec.clone(), tx_queued.clone()],
        0,
    )
    .expect("runtime tap should accept plugin txpool samples");
    assert_eq!(tap_summary.accepted, 2);

    let (txpool_status_runtime, changed_txpool_status_runtime) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "txpool_status",
        &serde_json::json!({ "chain_id": runtime_chain_id }),
    )
    .expect("txpool_status runtime snapshot should work");
    assert!(!changed_txpool_status_runtime);
    assert_eq!(txpool_status_runtime["pending"].as_str(), Some("0x1"));
    assert_eq!(txpool_status_runtime["queued"].as_str(), Some("0x1"));

    let (txpool_content_runtime, changed_txpool_content_runtime) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "txpool_content",
        &serde_json::json!({ "chain_id": runtime_chain_id }),
    )
    .expect("txpool_content runtime snapshot should work");
    assert!(!changed_txpool_content_runtime);
    let runtime_sender_key = format!("0x{}", to_hex(&addr_a));
    assert!(txpool_content_runtime["pending"][runtime_sender_key.as_str()]["0x1"].is_object());
    assert!(txpool_content_runtime["queued"][runtime_sender_key.as_str()]["0x3"].is_object());

    let (txpool_content_from_runtime, changed_txpool_content_from_runtime) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "txpool_contentFrom",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "address": runtime_sender_key,
        }),
    )
    .expect("txpool_contentFrom runtime snapshot should work");
    assert!(!changed_txpool_content_from_runtime);
    assert!(txpool_content_from_runtime["pending"]["0x1"].is_object());
    assert!(txpool_content_from_runtime["queued"]["0x3"].is_object());

    let (txpool_content_from_runtime_mixed, changed_txpool_content_from_runtime_mixed) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "txpool_contentFrom",
            &serde_json::json!([
                { "chainId": runtime_chain_id },
                runtime_sender_key
            ]),
        )
        .expect("txpool_contentFrom mixed-array params should work");
    assert!(!changed_txpool_content_from_runtime_mixed);
    assert!(txpool_content_from_runtime_mixed["pending"]["0x1"].is_object());
    assert!(txpool_content_from_runtime_mixed["queued"]["0x3"].is_object());

    let (txpool_inspect_from_runtime, changed_txpool_inspect_from_runtime) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "txpool_inspectFrom",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "address": runtime_sender_key,
        }),
    )
    .expect("txpool_inspectFrom runtime snapshot should work");
    assert!(!changed_txpool_inspect_from_runtime);
    assert!(txpool_inspect_from_runtime["pending"]["0x1"]
        .as_str()
        .map(|s| s.contains("wei"))
        .unwrap_or(false));
    assert!(txpool_inspect_from_runtime["queued"]["0x3"]
        .as_str()
        .map(|s| s.contains("wei"))
        .unwrap_or(false));

    let (txpool_inspect_from_runtime_mixed, changed_txpool_inspect_from_runtime_mixed) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "txpool_inspectFrom",
            &serde_json::json!([
                { "chainId": runtime_chain_id },
                runtime_sender_key
            ]),
        )
        .expect("txpool_inspectFrom mixed-array params should work");
    assert!(!changed_txpool_inspect_from_runtime_mixed);
    assert!(txpool_inspect_from_runtime_mixed["pending"]["0x1"]
        .as_str()
        .map(|s| s.contains("wei"))
        .unwrap_or(false));
    assert!(txpool_inspect_from_runtime_mixed["queued"]["0x3"]
        .as_str()
        .map(|s| s.contains("wei"))
        .unwrap_or(false));

    let (txpool_status_from_runtime, changed_txpool_status_from_runtime) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "txpool_statusFrom",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "address": runtime_sender_key,
        }),
    )
    .expect("txpool_statusFrom runtime snapshot should work");
    assert!(!changed_txpool_status_from_runtime);
    assert_eq!(txpool_status_from_runtime["pending"].as_str(), Some("0x1"));
    assert_eq!(txpool_status_from_runtime["queued"].as_str(), Some("0x1"));

    let (txpool_status_from_runtime_mixed, changed_txpool_status_from_runtime_mixed) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "txpool_statusFrom",
            &serde_json::json!([
                { "chainId": runtime_chain_id },
                runtime_sender_key
            ]),
        )
        .expect("txpool_statusFrom mixed-array params should work");
    assert!(!changed_txpool_status_from_runtime_mixed);
    assert_eq!(
        txpool_status_from_runtime_mixed["pending"].as_str(),
        Some("0x1")
    );
    assert_eq!(
        txpool_status_from_runtime_mixed["queued"].as_str(),
        Some("0x1")
    );

    let (txpool_content_from_absent, changed_txpool_content_from_absent) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "txpool_contentFrom",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "address": format!("0x{}", to_hex(&addr_c)),
        }),
    )
    .expect("txpool_contentFrom absent sender should work");
    assert!(!changed_txpool_content_from_absent);
    assert_eq!(
        txpool_content_from_absent["pending"]
            .as_object()
            .map(|m| m.len()),
        Some(0)
    );
    assert_eq!(
        txpool_content_from_absent["queued"]
            .as_object()
            .map(|m| m.len()),
        Some(0)
    );

    let (txpool_inspect_from_absent, changed_txpool_inspect_from_absent) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "txpool_inspectFrom",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "address": format!("0x{}", to_hex(&addr_c)),
        }),
    )
    .expect("txpool_inspectFrom absent sender should work");
    assert!(!changed_txpool_inspect_from_absent);
    assert_eq!(
        txpool_inspect_from_absent["pending"]
            .as_object()
            .map(|m| m.len()),
        Some(0)
    );
    assert_eq!(
        txpool_inspect_from_absent["queued"]
            .as_object()
            .map(|m| m.len()),
        Some(0)
    );

    let (txpool_status_from_absent, changed_txpool_status_from_absent) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "txpool_statusFrom",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "address": format!("0x{}", to_hex(&addr_c)),
        }),
    )
    .expect("txpool_statusFrom absent sender should work");
    assert!(!changed_txpool_status_from_absent);
    assert_eq!(txpool_status_from_absent["pending"].as_str(), Some("0x0"));
    assert_eq!(txpool_status_from_absent["queued"].as_str(), Some("0x0"));

    let (runtime_tx_by_hash, changed_runtime_tx_by_hash) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByHash",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "tx_hash": format!("0x{}", to_hex(&tx_exec.hash)),
        }),
    )
    .expect("eth_getTransactionByHash runtime snapshot should work");
    assert!(!changed_runtime_tx_by_hash);
    let runtime_hash_hex = format!("0x{}", to_hex(&tx_exec.hash));
    assert_eq!(
        runtime_tx_by_hash["hash"].as_str(),
        Some(runtime_hash_hex.as_str())
    );
    assert_eq!(runtime_tx_by_hash["blockNumber"].as_str(), Some("0x1"));
    let runtime_tx_index = runtime_tx_by_hash["transactionIndex"]
        .as_str()
        .expect("runtime tx index should be string");
    assert!(runtime_tx_index == "0x0" || runtime_tx_index == "0x1");
    assert_eq!(runtime_tx_by_hash["pending"].as_bool(), Some(true));

    let (pending_transactions_runtime, changed_pending_transactions_runtime) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_pendingTransactions",
        &serde_json::json!({ "chain_id": runtime_chain_id }),
    )
    .expect("eth_pendingTransactions runtime snapshot should work");
    assert!(!changed_pending_transactions_runtime);
    assert_eq!(
        pending_transactions_runtime
            .as_array()
            .map(std::vec::Vec::len),
        Some(2)
    );

    let (pending_count_runtime_addr, changed_pending_count_runtime_addr) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionCount",
        &serde_json::json!({
            "address": format!("0x{}", to_hex(&addr_a)),
            "chain_id": runtime_chain_id,
            "tag": "pending",
        }),
    )
    .expect("eth_getTransactionCount pending(runtime addr) should work");
    assert!(!changed_pending_count_runtime_addr);
    assert_eq!(pending_count_runtime_addr.as_str(), Some("0x4"));

    let (pending_block_runtime, changed_pending_block_runtime) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockByNumber",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "block": "pending",
            "full_transactions": true,
        }),
    )
    .expect("eth_getBlockByNumber pending(runtime) should work");
    assert!(!changed_pending_block_runtime);
    assert_eq!(pending_block_runtime["number"].as_str(), Some("0x1"));
    assert_eq!(
        pending_block_runtime["transactions"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(2)
    );
    let pending_block_runtime_txs = pending_block_runtime["transactions"]
        .as_array()
        .expect("pending block full txs should be array");
    assert_eq!(
        pending_block_runtime_txs[0]["pending"].as_bool(),
        Some(true)
    );
    assert_eq!(
        pending_block_runtime_txs[1]["pending"].as_bool(),
        Some(true)
    );

    let (pending_block_count_runtime, changed_pending_block_count_runtime) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockTransactionCountByNumber",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "block": "pending",
        }),
    )
    .expect("eth_getBlockTransactionCountByNumber pending(runtime) should work");
    assert!(!changed_pending_block_count_runtime);
    assert_eq!(pending_block_count_runtime.as_str(), Some("0x2"));
    let (fee_history_pending_runtime, changed_fee_history_pending_runtime) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_feeHistory",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "block_count": 2,
            "newest_block": "pending",
            "rewardPercentiles": [10.0, 50.0],
        }),
    )
    .expect("eth_feeHistory pending(runtime) should work");
    assert!(!changed_fee_history_pending_runtime);
    assert_eq!(
        fee_history_pending_runtime["oldestBlock"].as_str(),
        Some("0x0")
    );
    assert_eq!(
        fee_history_pending_runtime["baseFeePerGas"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(3)
    );
    assert_eq!(
        fee_history_pending_runtime["gasUsedRatio"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(2)
    );
    let pending_ratios = fee_history_pending_runtime["gasUsedRatio"]
        .as_array()
        .expect("pending fee history gasUsedRatio should be array");
    assert!(
        pending_ratios
            .iter()
            .any(|value| value.as_f64().is_some_and(|ratio| ratio > 0.0)),
        "pending fee history should include non-zero ratio for runtime pending block"
    );
    assert_eq!(
        fee_history_pending_runtime["reward"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(2)
    );
    assert_eq!(
        fee_history_pending_runtime["reward"][0]
            .as_array()
            .map(std::vec::Vec::len),
        Some(2)
    );
    let pending_block_hash = pending_block_runtime["hash"]
        .as_str()
        .expect("pending block hash should be string")
        .to_string();

    let (pending_block_by_hash_runtime, changed_pending_block_by_hash_runtime) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getBlockByHash",
            &serde_json::json!({
                "chain_id": runtime_chain_id,
                "block_hash": pending_block_hash.clone(),
                "full_transactions": true,
            }),
        )
        .expect("eth_getBlockByHash pending(runtime) should work");
    assert!(!changed_pending_block_by_hash_runtime);
    assert_eq!(
        pending_block_by_hash_runtime["number"].as_str(),
        Some("0x1")
    );
    assert_eq!(
        pending_block_by_hash_runtime["transactions"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(2)
    );
    let (logs_by_pending_hash_runtime, changed_logs_by_pending_hash_runtime) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getLogs",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "blockHash": pending_block_hash.clone(),
        }),
    )
    .expect("eth_getLogs by pending block hash should work");
    assert!(!changed_logs_by_pending_hash_runtime);
    assert_eq!(
        logs_by_pending_hash_runtime
            .as_array()
            .map(std::vec::Vec::len),
        Some(2)
    );
    let (logs_by_pending_range_runtime, changed_logs_by_pending_range_runtime) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getLogs",
            &serde_json::json!({
                "chain_id": runtime_chain_id,
                "fromBlock": "latest",
                "toBlock": "pending",
            }),
        )
        .expect("eth_getLogs latest..pending should include pending runtime block");
    assert!(!changed_logs_by_pending_range_runtime);
    assert_eq!(
        logs_by_pending_range_runtime
            .as_array()
            .map(std::vec::Vec::len),
        Some(2)
    );
    let (pending_logs_filter_id_raw, changed_new_pending_logs_filter) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_newFilter",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "fromBlock": "latest",
            "toBlock": "pending",
        }),
    )
    .expect("eth_newFilter latest..pending should work");
    assert!(!changed_new_pending_logs_filter);
    let (pending_logs_changes_first, changed_pending_logs_changes_first) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterChanges",
        &serde_json::json!([pending_logs_filter_id_raw.clone()]),
    )
    .expect("eth_getFilterChanges for latest..pending logs filter should work");
    assert!(!changed_pending_logs_changes_first);
    assert_eq!(
        pending_logs_changes_first
            .as_array()
            .map(std::vec::Vec::len),
        Some(2)
    );
    let (pending_logs_changes_second, changed_pending_logs_changes_second) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterChanges",
        &serde_json::json!([pending_logs_filter_id_raw]),
    )
    .expect("eth_getFilterChanges for latest..pending logs filter second poll should work");
    assert!(!changed_pending_logs_changes_second);
    assert_eq!(
        pending_logs_changes_second
            .as_array()
            .map(std::vec::Vec::len),
        Some(0)
    );
    let logs_blockhash_conflict = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getLogs",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "blockHash": pending_block_hash.clone(),
            "fromBlock": "0x0",
        }),
    );
    assert!(logs_blockhash_conflict.is_err());
    let logs_blockhash_conflict_err = logs_blockhash_conflict
        .err()
        .expect("eth_getLogs blockHash+fromBlock must error")
        .to_string();
    assert!(logs_blockhash_conflict_err.contains("blockHash is mutually exclusive"));

    let (tx_by_pending_number_index, changed_tx_by_pending_number_index) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByBlockNumberAndIndex",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "block": "pending",
            "transaction_index": "0x0",
        }),
    )
    .expect("eth_getTransactionByBlockNumberAndIndex pending(runtime) should work");
    assert!(!changed_tx_by_pending_number_index);
    assert_eq!(
        tx_by_pending_number_index["blockNumber"].as_str(),
        Some("0x1")
    );
    assert_eq!(
        tx_by_pending_number_index["transactionIndex"].as_str(),
        Some("0x0")
    );
    assert_eq!(tx_by_pending_number_index["pending"].as_bool(), Some(true));

    let (tx_by_pending_hash_index, changed_tx_by_pending_hash_index) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByBlockHashAndIndex",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "block_hash": pending_block_runtime["hash"],
            "transaction_index": "0x1",
        }),
    )
    .expect("eth_getTransactionByBlockHashAndIndex pending(runtime) should work");
    assert!(!changed_tx_by_pending_hash_index);
    assert_eq!(
        tx_by_pending_hash_index["blockNumber"].as_str(),
        Some("0x1")
    );
    assert_eq!(
        tx_by_pending_hash_index["transactionIndex"].as_str(),
        Some("0x1")
    );
    assert_eq!(tx_by_pending_hash_index["pending"].as_bool(), Some(true));

    let (pending_count_by_hash_runtime, changed_pending_count_by_hash_runtime) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getBlockTransactionCountByHash",
            &serde_json::json!({
                "chain_id": runtime_chain_id,
                "block_hash": pending_block_runtime["hash"],
            }),
        )
        .expect("eth_getBlockTransactionCountByHash pending(runtime) should work");
    assert!(!changed_pending_count_by_hash_runtime);
    assert_eq!(pending_count_by_hash_runtime.as_str(), Some("0x2"));

    let (pending_receipts_by_number_runtime, changed_pending_receipts_by_number_runtime) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getBlockReceipts",
            &serde_json::json!({
                "chain_id": runtime_chain_id,
                "block": "pending",
            }),
        )
        .expect("eth_getBlockReceipts pending(runtime) should work");
    assert!(!changed_pending_receipts_by_number_runtime);
    assert_eq!(
        pending_receipts_by_number_runtime
            .as_array()
            .map(std::vec::Vec::len),
        Some(2)
    );
    let pending_receipts_by_number = pending_receipts_by_number_runtime
        .as_array()
        .expect("pending receipts by number should be array");
    assert_eq!(
        pending_receipts_by_number[0]["pending"].as_bool(),
        Some(true)
    );
    assert!(pending_receipts_by_number[0]["status"].is_null());
    assert_eq!(
        pending_receipts_by_number[0]["blockNumber"].as_str(),
        Some("0x1")
    );
    assert_eq!(
        pending_receipts_by_number[0]["cumulativeGasUsed"].as_str(),
        Some("0x5208")
    );
    assert_eq!(
        pending_receipts_by_number[1]["cumulativeGasUsed"].as_str(),
        Some("0xa410")
    );

    let (pending_receipts_by_hash_runtime, changed_pending_receipts_by_hash_runtime) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getBlockReceipts",
            &serde_json::json!({
                "chain_id": runtime_chain_id,
                "block_hash": pending_block_runtime["hash"],
            }),
        )
        .expect("eth_getBlockReceipts pending(runtime hash) should work");
    assert!(!changed_pending_receipts_by_hash_runtime);
    assert_eq!(
        pending_receipts_by_hash_runtime
            .as_array()
            .map(std::vec::Vec::len),
        Some(2)
    );
    let pending_receipts_by_hash = pending_receipts_by_hash_runtime
        .as_array()
        .expect("pending receipts by hash should be array");
    assert_eq!(pending_receipts_by_hash[1]["pending"].as_bool(), Some(true));
    assert!(pending_receipts_by_hash[1]["status"].is_null());
    assert_eq!(
        pending_receipts_by_hash[0]["cumulativeGasUsed"].as_str(),
        Some("0x5208")
    );
    assert_eq!(
        pending_receipts_by_hash[1]["cumulativeGasUsed"].as_str(),
        Some("0xa410")
    );

    let (
        pending_receipts_by_pending_number_runtime,
        changed_pending_receipts_by_pending_number_runtime,
    ) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockReceipts",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "block": pending_block_runtime["number"],
        }),
    )
    .expect("eth_getBlockReceipts pending(runtime number) should work");
    assert!(!changed_pending_receipts_by_pending_number_runtime);
    assert_eq!(
        pending_receipts_by_pending_number_runtime,
        pending_receipts_by_number_runtime
    );

    let (runtime_receipt_by_hash, changed_runtime_receipt_by_hash) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionReceipt",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "tx_hash": format!("0x{}", to_hex(&tx_exec.hash)),
        }),
    )
    .expect("eth_getTransactionReceipt runtime snapshot should work");
    assert!(!changed_runtime_receipt_by_hash);
    assert_eq!(runtime_receipt_by_hash["pending"].as_bool(), Some(true));
    assert!(runtime_receipt_by_hash["status"].is_null());
    assert_eq!(runtime_receipt_by_hash["blockNumber"].as_str(), Some("0x1"));
    assert_eq!(
        runtime_receipt_by_hash["transactionHash"].as_str(),
        Some(runtime_hash_hex.as_str())
    );
    let pending_receipt_from_block = pending_receipts_by_hash
        .iter()
        .find(|item| item["transactionHash"].as_str() == Some(runtime_hash_hex.as_str()))
        .expect("pending block receipts should include runtime tx receipt");
    assert_eq!(runtime_receipt_by_hash, pending_receipt_from_block.clone());

    let (runtime_syncing, changed_runtime_syncing) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_syncing",
        &serde_json::json!({ "chain_id": runtime_chain_id }),
    )
    .expect("eth_syncing runtime snapshot should work");
    assert!(!changed_runtime_syncing);
    assert!(runtime_syncing.is_object());
    assert_eq!(runtime_syncing["currentBlock"].as_str(), Some("0x0"));
    assert_eq!(runtime_syncing["highestBlock"].as_str(), Some("0x1"));
    assert_eq!(
        runtime_syncing["highestBlock"].as_str(),
        pending_block_runtime["number"].as_str()
    );

    let (runtime_block_number, changed_runtime_block_number) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_blockNumber",
        &serde_json::json!({ "chain_id": runtime_chain_id }),
    )
    .expect("eth_blockNumber runtime chain should work");
    assert!(!changed_runtime_block_number);
    assert_eq!(runtime_block_number.as_str(), Some("0x0"));
    assert_eq!(pending_block_runtime["number"].as_str(), Some("0x1"));

    let (runtime_balance_latest_receiver, changed_runtime_balance_latest_receiver) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getBalance",
            &serde_json::json!({
                "chain_id": runtime_chain_id,
                "address": format!("0x{}", to_hex(&addr_b)),
                "tag": "latest",
            }),
        )
        .expect("eth_getBalance latest(runtime chain receiver) should work");
    assert!(!changed_runtime_balance_latest_receiver);
    assert_eq!(runtime_balance_latest_receiver.as_str(), Some("0x0"));

    let (runtime_balance_pending_receiver, changed_runtime_balance_pending_receiver) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getBalance",
            &serde_json::json!({
                "chain_id": runtime_chain_id,
                "address": format!("0x{}", to_hex(&addr_b)),
                "tag": "pending",
            }),
        )
        .expect("eth_getBalance pending(runtime chain receiver) should work");
    assert!(!changed_runtime_balance_pending_receiver);
    assert_eq!(runtime_balance_pending_receiver.as_str(), Some("0x14"));

    let (proof_latest_runtime_receiver, changed_proof_latest_runtime_receiver) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getProof",
            &serde_json::json!({
                "chain_id": runtime_chain_id,
                "address": format!("0x{}", to_hex(&addr_b)),
                "storage_keys": ["0x1"],
                "tag": "latest",
            }),
        )
        .expect("eth_getProof latest(runtime chain) should work");
    assert!(!changed_proof_latest_runtime_receiver);
    assert_eq!(
        proof_latest_runtime_receiver["balance"].as_str(),
        Some("0x0")
    );
    assert_eq!(proof_latest_runtime_receiver["nonce"].as_str(), Some("0x0"));
    let latest_runtime_receiver_storage = proof_latest_runtime_receiver["storageProof"]
        .as_array()
        .expect("latest runtime receiver storageProof should be array");
    assert_eq!(latest_runtime_receiver_storage.len(), 1);
    assert_eq!(
        latest_runtime_receiver_storage[0]["value"].as_str(),
        Some(format!("0x{}", "00".repeat(32)).as_str())
    );

    let (proof_pending_runtime_receiver, changed_proof_pending_runtime_receiver) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getProof",
            &serde_json::json!({
                "chain_id": runtime_chain_id,
                "address": format!("0x{}", to_hex(&addr_b)),
                "storage_keys": ["0x1"],
                "tag": "pending",
            }),
        )
        .expect("eth_getProof pending(runtime chain receiver) should work");
    assert!(!changed_proof_pending_runtime_receiver);
    assert_eq!(
        proof_pending_runtime_receiver["balance"].as_str(),
        Some("0x14")
    );
    assert_eq!(
        proof_pending_runtime_receiver["balance"].as_str(),
        runtime_balance_pending_receiver.as_str()
    );
    assert_eq!(
        proof_pending_runtime_receiver["nonce"].as_str(),
        Some("0x0")
    );

    let (proof_pending_runtime_sender, changed_proof_pending_runtime_sender) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getProof",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "address": format!("0x{}", to_hex(&addr_a)),
            "storage_keys": ["0x1", "0x3"],
            "tag": "pending",
        }),
    )
    .expect("eth_getProof pending(runtime chain sender) should work");
    assert!(!changed_proof_pending_runtime_sender);
    assert_eq!(proof_pending_runtime_sender["nonce"].as_str(), Some("0x4"));
    assert_eq!(
        proof_pending_runtime_sender["nonce"].as_str(),
        pending_count_runtime_addr.as_str()
    );
    let pending_runtime_sender_storage = proof_pending_runtime_sender["storageProof"]
        .as_array()
        .expect("pending runtime sender storageProof should be array");
    assert_eq!(pending_runtime_sender_storage.len(), 2);
    assert_eq!(
        pending_runtime_sender_storage[0]["value"].as_str(),
        Some(format!("0x{}", to_hex(&tx_exec.hash)).as_str())
    );
    assert_eq!(
        pending_runtime_sender_storage[1]["value"].as_str(),
        Some(format!("0x{}", to_hex(&tx_queued.hash)).as_str())
    );

    let (pending_uncle_count_by_number_runtime, changed_pending_uncle_count_by_number_runtime) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getUncleCountByBlockNumber",
            &serde_json::json!({
                "chain_id": runtime_chain_id,
                "block": "pending",
            }),
        )
        .expect("eth_getUncleCountByBlockNumber pending(runtime) should work");
    assert!(!changed_pending_uncle_count_by_number_runtime);
    assert_eq!(pending_uncle_count_by_number_runtime.as_str(), Some("0x0"));

    let (pending_uncle_count_by_hash_runtime, changed_pending_uncle_count_by_hash_runtime) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getUncleCountByBlockHash",
            &serde_json::json!({
                "chain_id": runtime_chain_id,
                "block_hash": pending_block_runtime["hash"],
            }),
        )
        .expect("eth_getUncleCountByBlockHash pending(runtime) should work");
    assert!(!changed_pending_uncle_count_by_hash_runtime);
    assert_eq!(pending_uncle_count_by_hash_runtime.as_str(), Some("0x0"));

    let (pending_filter_runtime_raw, changed_pending_filter_runtime_raw) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_newPendingTransactionFilter",
        &serde_json::json!({ "chain_id": runtime_chain_id }),
    )
    .expect("eth_newPendingTransactionFilter runtime should work");
    assert!(!changed_pending_filter_runtime_raw);
    let pending_filter_runtime = pending_filter_runtime_raw
        .as_str()
        .expect("runtime pending filter id should be string")
        .to_string();
    let (pending_filter_runtime_first, changed_pending_filter_runtime_first) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterChanges",
        &serde_json::json!([pending_filter_runtime.clone()]),
    )
    .expect("eth_getFilterChanges runtime pending filter first poll should work");
    assert!(!changed_pending_filter_runtime_first);
    assert_eq!(
        pending_filter_runtime_first
            .as_array()
            .map(std::vec::Vec::len),
        Some(0)
    );

    let mut tx_runtime_new =
        TxIR::transfer(addr_a.clone(), addr_b.clone(), 15, 4, runtime_chain_id);
    tx_runtime_new.compute_hash();
    let tap_runtime_new = runtime_tap_ir_batch_v1(
        novovm_adapter_api::ChainType::EVM,
        runtime_chain_id,
        &[tx_runtime_new.clone()],
        0,
    )
    .expect("runtime tap should accept additional pending tx");
    assert_eq!(tap_runtime_new.accepted, 1);
    let (pending_filter_runtime_second, changed_pending_filter_runtime_second) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getFilterChanges",
            &serde_json::json!([pending_filter_runtime]),
        )
        .expect("eth_getFilterChanges runtime pending filter second poll should work");
    assert!(!changed_pending_filter_runtime_second);
    assert_eq!(
        pending_filter_runtime_second
            .as_array()
            .map(std::vec::Vec::len),
        Some(1)
    );

    // Transition consistency: once tx is indexed as confirmed, query-by-hash/receipt
    // must prefer confirmed view even if runtime pending snapshots still contain it.
    let mut runtime_tx_hash = [0u8; 32];
    runtime_tx_hash.copy_from_slice(tx_exec.hash.as_slice());
    eth_tx_index.insert(
        runtime_tx_hash,
        GatewayEthTxIndexEntry {
            tx_hash: runtime_tx_hash,
            uca_id: "uca-runtime-confirmed".to_string(),
            chain_id: runtime_chain_id,
            nonce: 1,
            tx_type: 0,
            from: tx_exec.from.clone(),
            to: tx_exec.to.clone(),
            value: tx_exec.value,
            gas_limit: tx_exec.gas_limit,
            gas_price: tx_exec.gas_price,
            input: tx_exec.data.clone(),
        },
    );

    let (runtime_tx_by_hash_after_confirmed, changed_runtime_tx_by_hash_after_confirmed) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getTransactionByHash",
            &serde_json::json!({
                "chain_id": runtime_chain_id,
                "tx_hash": runtime_hash_hex,
            }),
        )
        .expect("eth_getTransactionByHash should prefer confirmed index over runtime pending");
    assert!(!changed_runtime_tx_by_hash_after_confirmed);
    assert_eq!(
        runtime_tx_by_hash_after_confirmed["pending"].as_bool(),
        Some(false)
    );
    assert_eq!(
        runtime_tx_by_hash_after_confirmed["blockNumber"].as_str(),
        Some("0x1")
    );

    let (runtime_receipt_by_hash_after_confirmed, changed_runtime_receipt_by_hash_after_confirmed) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getTransactionReceipt",
            &serde_json::json!({
                "chain_id": runtime_chain_id,
                "tx_hash": format!("0x{}", to_hex(&tx_exec.hash)),
            }),
        )
        .expect("eth_getTransactionReceipt should prefer confirmed index over runtime pending");
    assert!(!changed_runtime_receipt_by_hash_after_confirmed);
    assert_eq!(
        runtime_receipt_by_hash_after_confirmed["pending"].as_bool(),
        Some(false)
    );
    assert_eq!(
        runtime_receipt_by_hash_after_confirmed["status"].as_str(),
        Some("0x1")
    );
    let confirmed_block_hash_after_confirmed = runtime_tx_by_hash_after_confirmed["blockHash"]
        .as_str()
        .expect("confirmed tx block hash should be string")
        .to_string();

    let (confirmed_block_by_hash_after_confirmed, changed_confirmed_block_by_hash_after_confirmed) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getBlockByHash",
            &serde_json::json!({
                "chain_id": runtime_chain_id,
                "block_hash": confirmed_block_hash_after_confirmed,
                "full_transactions": true,
            }),
        )
        .expect("eth_getBlockByHash confirmed hash should prefer confirmed view");
    assert!(!changed_confirmed_block_by_hash_after_confirmed);
    assert_eq!(
        confirmed_block_by_hash_after_confirmed["number"].as_str(),
        Some("0x1")
    );
    let confirmed_block_txs = confirmed_block_by_hash_after_confirmed["transactions"]
        .as_array()
        .expect("confirmed block txs should be array");
    assert_eq!(confirmed_block_txs.len(), 1);
    assert_eq!(confirmed_block_txs[0]["pending"].as_bool(), Some(false));

    let (
        confirmed_tx_by_block_hash_index_after_confirmed,
        changed_confirmed_tx_by_block_hash_index_after_confirmed,
    ) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByBlockHashAndIndex",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "block_hash": confirmed_block_hash_after_confirmed,
            "transaction_index": "0x0",
        }),
    )
    .expect("eth_getTransactionByBlockHashAndIndex confirmed hash should prefer confirmed view");
    assert!(!changed_confirmed_tx_by_block_hash_index_after_confirmed);
    assert_eq!(
        confirmed_tx_by_block_hash_index_after_confirmed["pending"].as_bool(),
        Some(false)
    );
    let (
        confirmed_tx_count_by_hash_after_confirmed,
        changed_confirmed_tx_count_by_hash_after_confirmed,
    ) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockTransactionCountByHash",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "block_hash": confirmed_block_hash_after_confirmed,
        }),
    )
    .expect("eth_getBlockTransactionCountByHash confirmed hash should prefer confirmed view");
    assert!(!changed_confirmed_tx_count_by_hash_after_confirmed);
    assert_eq!(
        confirmed_tx_count_by_hash_after_confirmed.as_str(),
        Some("0x1")
    );
    let (
        confirmed_uncle_count_by_hash_after_confirmed,
        changed_confirmed_uncle_count_by_hash_after_confirmed,
    ) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getUncleCountByBlockHash",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "block_hash": confirmed_block_hash_after_confirmed,
        }),
    )
    .expect("eth_getUncleCountByBlockHash confirmed hash should prefer confirmed view");
    assert!(!changed_confirmed_uncle_count_by_hash_after_confirmed);
    assert_eq!(
        confirmed_uncle_count_by_hash_after_confirmed.as_str(),
        Some("0x0")
    );

    let (
        confirmed_receipts_by_number_after_confirmed,
        changed_confirmed_receipts_by_number_after_confirmed,
    ) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockReceipts",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "block": "0x1",
        }),
    )
    .expect("eth_getBlockReceipts confirmed block should prefer confirmed view");
    assert!(!changed_confirmed_receipts_by_number_after_confirmed);
    let confirmed_receipts_by_number = confirmed_receipts_by_number_after_confirmed
        .as_array()
        .expect("confirmed receipts by number should be array");
    assert_eq!(confirmed_receipts_by_number.len(), 1);
    assert_eq!(
        confirmed_receipts_by_number[0]["pending"].as_bool(),
        Some(false)
    );
    assert_eq!(
        confirmed_receipts_by_number[0]["status"].as_str(),
        Some("0x1")
    );
    assert_eq!(
        confirmed_receipts_by_number[0]["cumulativeGasUsed"].as_str(),
        Some("0x5208")
    );
    let (
        confirmed_receipts_by_hash_after_confirmed,
        changed_confirmed_receipts_by_hash_after_confirmed,
    ) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockReceipts",
        &serde_json::json!({
            "chain_id": runtime_chain_id,
            "block_hash": confirmed_block_hash_after_confirmed,
        }),
    )
    .expect("eth_getBlockReceipts confirmed hash should prefer confirmed view");
    assert!(!changed_confirmed_receipts_by_hash_after_confirmed);
    let confirmed_receipts_by_hash = confirmed_receipts_by_hash_after_confirmed
        .as_array()
        .expect("confirmed receipts by hash should be array");
    assert_eq!(confirmed_receipts_by_hash.len(), 1);
    assert_eq!(
        confirmed_receipts_by_hash[0]["pending"].as_bool(),
        Some(false)
    );
    assert_eq!(
        confirmed_receipts_by_hash[0]["status"].as_str(),
        Some("0x1")
    );
    assert_eq!(
        confirmed_receipts_by_hash[0]["cumulativeGasUsed"].as_str(),
        Some("0x5208")
    );

    let (pending_receipts_after_confirmed, changed_pending_receipts_after_confirmed) =
        run_gateway_method(
            &mut router,
            &mut eth_tx_index,
            &mut evm_settlement_index_by_id,
            &mut evm_settlement_index_by_tx,
            &mut evm_pending_payout_by_settlement,
            &mut ctx,
            "eth_getBlockReceipts",
            &serde_json::json!({
                "chain_id": runtime_chain_id,
                "block": "pending",
            }),
        )
        .expect("eth_getBlockReceipts pending should keep pending view after confirm");
    assert!(!changed_pending_receipts_after_confirmed);
    let pending_receipts_after_confirmed = pending_receipts_after_confirmed
        .as_array()
        .expect("pending receipts after confirmed should be array");
    assert!(!pending_receipts_after_confirmed.is_empty());
    assert_eq!(
        pending_receipts_after_confirmed[0]["pending"].as_bool(),
        Some(true)
    );
    assert!(pending_receipts_after_confirmed[0]["status"].is_null());
    assert_eq!(
        pending_receipts_after_confirmed[0]["blockNumber"].as_str(),
        Some("0x2")
    );

    let (logs_filter_id_raw, changed_new_filter) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_newFilter",
        &serde_json::json!([{
            "chain_id": 1u64,
            "address": format!("0x{}", to_hex(&addr_b)),
            "fromBlock": "earliest",
            "toBlock": "latest",
        }]),
    )
    .expect("eth_newFilter should work");
    assert!(!changed_new_filter);
    let logs_filter_id = logs_filter_id_raw
        .as_str()
        .expect("filter id should be string")
        .to_string();

    let (filter_changes_1, changed_filter_changes_1) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterChanges",
        &serde_json::json!([logs_filter_id.clone()]),
    )
    .expect("eth_getFilterChanges first poll should work");
    assert!(!changed_filter_changes_1);
    assert_eq!(filter_changes_1.as_array().map(std::vec::Vec::len), Some(2));

    let (filter_changes_2, changed_filter_changes_2) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterChanges",
        &serde_json::json!([logs_filter_id.clone()]),
    )
    .expect("eth_getFilterChanges second poll should work");
    assert!(!changed_filter_changes_2);
    assert_eq!(filter_changes_2.as_array().map(std::vec::Vec::len), Some(0));

    let (filter_logs, changed_filter_logs) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterLogs",
        &serde_json::json!([logs_filter_id.clone()]),
    )
    .expect("eth_getFilterLogs should work");
    assert!(!changed_filter_logs);
    assert_eq!(filter_logs.as_array().map(std::vec::Vec::len), Some(2));

    let (pending_filter_id_raw, changed_new_pending_filter) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_newPendingTransactionFilter",
        &serde_json::json!({ "chain_id": 1u64 }),
    )
    .expect("eth_newPendingTransactionFilter should work");
    assert!(!changed_new_pending_filter);
    let pending_filter_id = pending_filter_id_raw
        .as_str()
        .expect("pending filter id should be string")
        .to_string();
    let (pending_changes_empty, changed_pending_changes_empty) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterChanges",
        &serde_json::json!([pending_filter_id.clone()]),
    )
    .expect("eth_getFilterChanges pending filter first poll should work");
    assert!(!changed_pending_changes_empty);
    assert_eq!(
        pending_changes_empty.as_array().map(std::vec::Vec::len),
        Some(0)
    );

    eth_tx_index.insert(
        [0x33u8; 32],
        GatewayEthTxIndexEntry {
            tx_hash: [0x33u8; 32],
            uca_id: "uca-a".to_string(),
            chain_id: 1,
            nonce: 3,
            tx_type: 0,
            from: addr_a,
            to: Some(addr_b),
            value: 5,
            gas_limit: 21_000,
            gas_price: 1,
            input: vec![0x03],
        },
    );
    let (pending_changes_after_insert, changed_pending_changes_after_insert) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterChanges",
        &serde_json::json!([pending_filter_id]),
    )
    .expect("eth_getFilterChanges pending filter second poll should work");
    assert!(!changed_pending_changes_after_insert);
    assert_eq!(
        pending_changes_after_insert
            .as_array()
            .map(std::vec::Vec::len),
        Some(0)
    );

    let (block_filter_id_raw, changed_new_block_filter) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_newBlockFilter",
        &serde_json::json!({ "chain_id": 1u64 }),
    )
    .expect("eth_newBlockFilter should work");
    assert!(!changed_new_block_filter);
    let block_filter_id = block_filter_id_raw
        .as_str()
        .expect("block filter id should be string")
        .to_string();

    eth_tx_index.insert(
        [0x44u8; 32],
        GatewayEthTxIndexEntry {
            tx_hash: [0x44u8; 32],
            uca_id: "uca-c".to_string(),
            chain_id: 1,
            nonce: 4,
            tx_type: 0,
            from: addr_c,
            to: None,
            value: 1,
            gas_limit: 21_000,
            gas_price: 1,
            input: vec![0x04],
        },
    );
    let (block_changes, changed_block_changes) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterChanges",
        &serde_json::json!([block_filter_id.clone()]),
    )
    .expect("eth_getFilterChanges block filter should work");
    assert!(!changed_block_changes);
    assert_eq!(block_changes.as_array().map(std::vec::Vec::len), Some(1));

    let (uninstalled, changed_uninstall) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_uninstallFilter",
        &serde_json::json!([block_filter_id]),
    )
    .expect("eth_uninstallFilter should work");
    assert!(!changed_uninstall);
    assert_eq!(uninstalled.as_bool(), Some(true));
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_block_filter_changes_recovers_new_blocks_from_store_when_memory_window_stale() {
    let _guard = env_test_guard();
    let chain_id = 77u64;
    let rocksdb_path = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-block-filter-store-recover-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: rocksdb_path.clone(),
    };
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-block-filter-store-recover-spool-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    for nonce in 1..=3u64 {
        let mut tx_hash = [0u8; 32];
        tx_hash[..8].copy_from_slice(&nonce.to_le_bytes());
        eth_tx_index.insert(
            tx_hash,
            GatewayEthTxIndexEntry {
                tx_hash,
                uca_id: format!("uca-mem-{}", nonce),
                chain_id,
                nonce,
                tx_type: 0,
                from: vec![0x11u8; 20],
                to: Some(vec![0x22u8; 20]),
                value: 1,
                gas_limit: 21_000,
                gas_price: 1,
                input: vec![0x00],
            },
        );
    }

    backend
        .save_eth_tx(&GatewayEthTxIndexEntry {
            tx_hash: [0x50u8; 32],
            uca_id: "uca-store-500".to_string(),
            chain_id,
            nonce: 500,
            tx_type: 0,
            from: vec![0x33u8; 20],
            to: Some(vec![0x44u8; 20]),
            value: 3,
            gas_limit: 25_000,
            gas_price: 2,
            input: vec![0x01],
        })
        .expect("save store block 500");

    let prev_scan_max = std::env::var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX").ok();
    std::env::set_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX", "3");

    let (filter_id_raw, changed_new_filter) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_newBlockFilter",
        &serde_json::json!({ "chain_id": chain_id }),
    )
    .expect("eth_newBlockFilter should work");
    assert!(!changed_new_filter);
    let filter_id = filter_id_raw
        .as_str()
        .expect("block filter id should be string")
        .to_string();

    let tx_501 = GatewayEthTxIndexEntry {
        tx_hash: [0x51u8; 32],
        uca_id: "uca-store-501".to_string(),
        chain_id,
        nonce: 501,
        tx_type: 0,
        from: vec![0x33u8; 20],
        to: Some(vec![0x55u8; 20]),
        value: 5,
        gas_limit: 25_000,
        gas_price: 3,
        input: vec![0x02],
    };
    backend.save_eth_tx(&tx_501).expect("save store block 501");

    let (changes_raw, changed_changes) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterChanges",
        &serde_json::json!([filter_id]),
    )
    .expect("eth_getFilterChanges should recover store block");
    assert!(!changed_changes);
    let changes = changes_raw
        .as_array()
        .expect("block filter changes should be array");
    assert_eq!(changes.len(), 1);
    let expected_hash = gateway_eth_block_hash_for_txs(chain_id, 501, &[tx_501]);
    assert_eq!(
        changes[0].as_str(),
        Some(format!("0x{}", to_hex(&expected_hash)).as_str())
    );

    if let Some(value) = prev_scan_max {
        std::env::set_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX", value);
    } else {
        std::env::remove_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX");
    }
    let _ = fs::remove_dir_all(&spool_dir);
    let _ = fs::remove_dir_all(&rocksdb_path);
}

#[test]
fn eth_gas_price_prefers_runtime_then_recent_chain_then_default() {
    let _guard = env_test_guard();
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-gas-price-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let addr_a = vec![0xaau8; 20];
    let addr_b = vec![0xbbu8; 20];

    let chain_from_index = 7_700_001u64;
    for (idx, gas_price) in [2u64, 4u64, 6u64].iter().copied().enumerate() {
        let tx_hash = [0x40u8.saturating_add(idx as u8); 32];
        eth_tx_index.insert(
            tx_hash,
            GatewayEthTxIndexEntry {
                tx_hash,
                uca_id: format!("uca-gas-{}", idx),
                chain_id: chain_from_index,
                nonce: (idx as u64).saturating_add(1),
                tx_type: 0,
                from: addr_a.clone(),
                to: Some(addr_b.clone()),
                value: 1,
                gas_limit: 21_000,
                gas_price,
                input: Vec::new(),
            },
        );
    }
    let (gas_price_from_index, changed_gas_price_from_index) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_gasPrice",
        &serde_json::json!({ "chain_id": chain_from_index }),
    )
    .expect("eth_gasPrice should work with recent chain entries");
    assert!(!changed_gas_price_from_index);
    assert_eq!(gas_price_from_index.as_str(), Some("0x4"));

    let runtime_chain_id = 7_700_002u64;
    let mut tx_low = TxIR::transfer(addr_a.clone(), addr_b.clone(), 1, 1, runtime_chain_id);
    tx_low.gas_price = 3;
    tx_low.compute_hash();
    let mut tx_mid = TxIR::transfer(addr_a.clone(), addr_b.clone(), 1, 2, runtime_chain_id);
    tx_mid.gas_price = 5;
    tx_mid.compute_hash();
    let mut tx_high = TxIR::transfer(addr_a.clone(), addr_b.clone(), 1, 3, runtime_chain_id);
    tx_high.gas_price = 7;
    tx_high.compute_hash();
    let tap_summary = runtime_tap_ir_batch_v1(
        novovm_adapter_api::ChainType::EVM,
        runtime_chain_id,
        &[tx_low, tx_mid, tx_high],
        0,
    )
    .expect("runtime tap should accept gas-price samples");
    assert_eq!(tap_summary.accepted, 3);

    let (gas_price_runtime, changed_gas_price_runtime) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_gasPrice",
        &serde_json::json!({ "chain_id": runtime_chain_id }),
    )
    .expect("eth_gasPrice should prefer runtime pending txpool when available");
    assert!(!changed_gas_price_runtime);
    assert_eq!(gas_price_runtime.as_str(), Some("0x5"));

    let chain_fallback = 7_700_003u64;
    let fallback = u64_env("NOVOVM_GATEWAY_ETH_DEFAULT_GAS_PRICE", 1);
    let expected_fallback = format!("0x{:x}", fallback);
    let (gas_price_fallback, changed_gas_price_fallback) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_gasPrice",
        &serde_json::json!({ "chain_id": chain_fallback }),
    )
    .expect("eth_gasPrice should fallback to default when no tx sample exists");
    assert!(!changed_gas_price_fallback);
    assert_eq!(
        gas_price_fallback.as_str(),
        Some(expected_fallback.as_str())
    );
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_syncing_json_enforces_pending_boundary_in_object_mode() {
    let syncing_object = gateway_eth_syncing_json(
        GatewayEthSyncStatusV1 {
            peer_count: 3,
            starting_block: 30,
            current_block: 12,
            highest_block: 13,
            local_current_block: 15,
        },
        Some(16),
    );
    assert!(syncing_object.is_object());
    assert_eq!(syncing_object["startingBlock"].as_str(), Some("0xc"));
    assert_eq!(syncing_object["currentBlock"].as_str(), Some("0xc"));
    assert_eq!(syncing_object["highestBlock"].as_str(), Some("0x10"));

    let syncing_without_pending = gateway_eth_syncing_json(
        GatewayEthSyncStatusV1 {
            peer_count: 3,
            starting_block: 30,
            current_block: 12,
            highest_block: 13,
            local_current_block: 15,
        },
        None,
    );
    assert_eq!(
        syncing_without_pending["highestBlock"].as_str(),
        Some("0xd")
    );

    let syncing_with_pending_boundary = gateway_eth_syncing_json(
        GatewayEthSyncStatusV1 {
            peer_count: 1,
            starting_block: 5,
            current_block: 7,
            highest_block: 7,
            local_current_block: 7,
        },
        Some(8),
    );
    assert!(syncing_with_pending_boundary.is_object());
    assert_eq!(
        syncing_with_pending_boundary["startingBlock"].as_str(),
        Some("0x5")
    );
    assert_eq!(
        syncing_with_pending_boundary["currentBlock"].as_str(),
        Some("0x7")
    );
    assert_eq!(
        syncing_with_pending_boundary["highestBlock"].as_str(),
        Some("0x8")
    );

    let syncing_with_external_current_and_pending = gateway_eth_syncing_json(
        GatewayEthSyncStatusV1 {
            peer_count: 2,
            starting_block: 100,
            current_block: 200,
            highest_block: 200,
            local_current_block: 150,
        },
        Some(151),
    );
    assert!(syncing_with_external_current_and_pending.is_object());
    assert_eq!(
        syncing_with_external_current_and_pending["startingBlock"].as_str(),
        Some("0x64")
    );
    assert_eq!(
        syncing_with_external_current_and_pending["currentBlock"].as_str(),
        Some("0xc8")
    );
    assert_eq!(
        syncing_with_external_current_and_pending["highestBlock"].as_str(),
        Some("0xc9")
    );

    let not_syncing = gateway_eth_syncing_json(
        GatewayEthSyncStatusV1 {
            peer_count: 1,
            starting_block: 5,
            current_block: 7,
            highest_block: 7,
            local_current_block: 7,
        },
        None,
    );
    assert_eq!(not_syncing, serde_json::Value::Bool(false));
}

#[test]
fn helper_extractors_accept_object_plus_scalar_mixed_array_params() {
    let tx_hash = format!("0x{}", "11".repeat(32));
    let block_hash = format!("0x{}", "22".repeat(32));
    let raw_tx = "0x02aabbcc".to_string();
    let address = format!("0x{}", "33".repeat(20));

    let tx_hash_params = serde_json::json!([
        {
            "chainId": 1u64
        },
        tx_hash.clone()
    ]);
    assert_eq!(
        extract_eth_tx_hash_query_param(&tx_hash_params).as_deref(),
        Some(tx_hash.as_str())
    );

    let block_hash_params = serde_json::json!([
        {
            "chain_id": 1u64
        },
        block_hash.clone()
    ]);
    assert_eq!(
        extract_eth_block_hash_param(&block_hash_params).as_deref(),
        Some(block_hash.as_str())
    );
    let parsed_block_hash =
        parse_eth_block_hash_from_params(&block_hash_params).expect("parse block hash");
    assert_eq!(
        parsed_block_hash,
        Some(parse_hex32_from_string(&block_hash, "block_hash").expect("decode hash"))
    );

    let raw_tx_params = serde_json::json!([
        {
            "chainId": 1u64
        },
        raw_tx
    ]);
    assert_eq!(
        extract_eth_raw_tx_param(&raw_tx_params).as_deref(),
        Some("0x02aabbcc")
    );

    let address_params = serde_json::json!([
        {
            "chainId": 1u64
        },
        address.clone()
    ]);
    assert_eq!(
        extract_eth_persona_address_param(&address_params).as_deref(),
        Some(address.as_str())
    );
}

#[test]
fn parse_eth_block_query_tag_accepts_object_plus_scalar_mixed_array() {
    let params_pending = serde_json::json!([
        {
            "chainId": 1u64
        },
        "pending"
    ]);
    assert_eq!(
        parse_eth_block_query_tag(&params_pending).as_deref(),
        Some("pending")
    );

    let params_number = serde_json::json!([
        {
            "chainId": 1u64
        },
        15
    ]);
    assert_eq!(
        parse_eth_block_query_tag(&params_number).as_deref(),
        Some("15")
    );
}

#[test]
fn receipt_and_block_receipts_accept_object_then_scalar_array_params() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let chain_id = 9u64;
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-mixed-array-receipt-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let entry = GatewayEthTxIndexEntry {
        chain_id,
        tx_hash: [0x44u8; 32],
        from: vec![0x11u8; 20],
        to: Some(vec![0x22u8; 20]),
        nonce: 3,
        input: vec![],
        value: 7,
        gas_limit: 21_000,
        gas_price: 10,
        tx_type: 0,
        uca_id: "uca:mixed-array".to_string(),
    };
    eth_tx_index.insert(entry.tx_hash, entry.clone());

    let (receipt, changed_receipt) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionReceipt",
        &serde_json::json!([
            {
                "chainId": chain_id
            },
            format!("0x{}", to_hex(&entry.tx_hash))
        ]),
    )
    .expect("eth_getTransactionReceipt mixed array params should work");
    assert!(!changed_receipt);
    assert_eq!(receipt["pending"].as_bool(), Some(false));
    assert_eq!(receipt["status"].as_str(), Some("0x1"));
    let expected_tx_hash = format!("0x{}", to_hex(&entry.tx_hash));
    assert_eq!(
        receipt["transactionHash"].as_str(),
        Some(expected_tx_hash.as_str())
    );

    let (block_receipts, changed_block_receipts) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockReceipts",
        &serde_json::json!([
            {
                "chainId": chain_id
            },
            format!("0x{:x}", entry.nonce)
        ]),
    )
    .expect("eth_getBlockReceipts mixed array params should work");
    assert!(!changed_block_receipts);
    assert_eq!(block_receipts.as_array().map(std::vec::Vec::len), Some(1));
    assert_eq!(
        block_receipts
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.get("pending"))
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn mixed_array_block_tag_and_storage_helpers_select_correct_positions() {
    let address = format!("0x{}", "11".repeat(20));

    let tx_count_tag_params = serde_json::json!([
        {
            "chainId": 1u64
        },
        address.clone(),
        "pending"
    ]);
    assert_eq!(
        parse_eth_tx_count_block_tag(&tx_count_tag_params).as_deref(),
        Some("pending")
    );

    let storage_slot_params = serde_json::json!([
        {
            "chainId": 1u64
        },
        address.clone(),
        "0x2",
        "latest"
    ]);
    assert_eq!(
        extract_eth_storage_slot_param(&storage_slot_params).as_deref(),
        Some("0x2")
    );

    let proof_params = serde_json::json!([
        {
            "chainId": 1u64
        },
        address.clone(),
        ["0x1", "0x2"],
        "pending"
    ]);
    let keys = parse_eth_get_proof_storage_keys(&proof_params).expect("parse proof keys");
    assert_eq!(keys, vec!["0x1".to_string(), "0x2".to_string()]);
    assert_eq!(
        parse_eth_get_proof_block_tag(&proof_params).as_deref(),
        Some("pending")
    );
}

#[test]
fn mixed_array_object_selection_supports_second_object_payload() {
    let address = format!("0x{}", "22".repeat(20));

    let proof_object_params = serde_json::json!([
        {
            "chainId": 1u64
        },
        {
            "address": address,
            "storage_keys": ["0x3"],
            "blockTag": "0x5"
        }
    ]);
    let keys =
        parse_eth_get_proof_storage_keys(&proof_object_params).expect("parse object proof keys");
    assert_eq!(keys, vec!["0x3".to_string()]);
    assert_eq!(
        parse_eth_get_proof_block_tag(&proof_object_params).as_deref(),
        Some("0x5")
    );

    let logs_params = serde_json::json!([
        {
            "chainId": 1u64
        },
        {
            "address": format!("0x{}", "33".repeat(20)),
            "topics": [format!("0x{}", "44".repeat(32))],
            "fromBlock": "0x1",
            "toBlock": "pending"
        }
    ]);
    let logs_query = parse_eth_logs_query_from_params(&logs_params, 10).expect("parse logs query");
    assert_eq!(
        logs_query.address_filters.as_ref().map(std::vec::Vec::len),
        Some(1)
    );
    assert_eq!(logs_query.from_block, Some(1));
    assert_eq!(logs_query.to_block, Some(11));
    assert!(logs_query.include_pending_block);
    assert_eq!(
        logs_query.topic_filters.as_ref().map(std::vec::Vec::len),
        Some(1)
    );

    let nested_filter_params = serde_json::json!([
        {
            "chainId": 1u64
        },
        "logs",
        {
            "filter": {
                "address": format!("0x{}", "55".repeat(20)),
                "topics": [format!("0x{}", "66".repeat(32))],
                "fromBlock": "0x2",
                "toBlock": "pending"
            }
        }
    ]);
    let nested_logs_query =
        parse_eth_logs_query_from_params(&nested_filter_params, 10).expect("parse nested logs");
    assert_eq!(
        nested_logs_query
            .address_filters
            .as_ref()
            .map(std::vec::Vec::len),
        Some(1)
    );
    assert_eq!(nested_logs_query.from_block, Some(2));
    assert_eq!(nested_logs_query.to_block, Some(11));
    assert!(nested_logs_query.include_pending_block);
    assert_eq!(
        nested_logs_query
            .topic_filters
            .as_ref()
            .map(std::vec::Vec::len),
        Some(1)
    );
}

#[test]
fn eth_subscribe_logs_accepts_nested_filter_mixed_array_params() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-subscribe-nested-filter-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let (sub_id_raw, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_subscribe",
        &serde_json::json!([
            {"chainId": 137u64},
            "logs",
            {
                "filter": {
                    "address": format!("0x{}", "77".repeat(20)),
                    "topics": [format!("0x{}", "88".repeat(32))]
                }
            }
        ]),
    )
    .expect("eth_subscribe logs with nested filter should work");
    assert!(!changed);
    let sub_id = parse_u64_decimal_or_hex(
        sub_id_raw
            .as_str()
            .expect("subscription id should be string"),
    )
    .expect("decode subscription id");
    let stored = eth_filters
        .filters
        .get(&sub_id)
        .cloned()
        .expect("stored subscription filter should exist");
    match stored {
        GatewayEthFilterKind::Logs(log_filter) => {
            assert_eq!(log_filter.chain_id, 137u64);
            assert_eq!(
                log_filter
                    .query
                    .address_filters
                    .as_ref()
                    .map(std::vec::Vec::len),
                Some(1)
            );
            assert_eq!(
                log_filter
                    .query
                    .topic_filters
                    .as_ref()
                    .map(std::vec::Vec::len),
                Some(1)
            );
        }
        _ => panic!("subscription should be logs filter"),
    }
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_subscribe_logs_accepts_object_kind_with_nested_filter() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-subscribe-object-kind-filter-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let (sub_id_raw, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_subscribe",
        &serde_json::json!({
            "kind": "logs",
            "chainId": 10u64,
            "filter": {
                "address": format!("0x{}", "99".repeat(20)),
                "topics": [format!("0x{}", "aa".repeat(32))],
                "fromBlock": "earliest",
                "toBlock": "latest",
            }
        }),
    )
    .expect("eth_subscribe logs object-kind with nested filter should work");
    assert!(!changed);
    let sub_id = parse_u64_decimal_or_hex(
        sub_id_raw
            .as_str()
            .expect("subscription id should be string"),
    )
    .expect("decode subscription id");
    let stored = eth_filters
        .filters
        .get(&sub_id)
        .cloned()
        .expect("stored subscription filter should exist");
    match stored {
        GatewayEthFilterKind::Logs(log_filter) => {
            assert_eq!(log_filter.chain_id, 10u64);
            assert_eq!(
                log_filter
                    .query
                    .address_filters
                    .as_ref()
                    .map(std::vec::Vec::len),
                Some(1)
            );
            assert_eq!(
                log_filter
                    .query
                    .topic_filters
                    .as_ref()
                    .map(std::vec::Vec::len),
                Some(1)
            );
        }
        _ => panic!("subscription should be logs filter"),
    }
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_call_accepts_chain_object_plus_call_object_plus_tag_array() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let chain_id = 77u64;
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-call-mixed-array-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let deployer = vec![0x41u8; 20];
    let deploy_nonce = 2u64;
    let contract = gateway_eth_derive_contract_address(&deployer, deploy_nonce);
    let deploy_entry = GatewayEthTxIndexEntry {
        chain_id,
        tx_hash: [0x55u8; 32],
        from: deployer,
        to: None,
        nonce: deploy_nonce,
        input: vec![0x60, 0x00],
        value: 0,
        gas_limit: 100_000,
        gas_price: 1,
        tx_type: 2,
        uca_id: "uca:mixed-array-call".to_string(),
    };
    eth_tx_index.insert(deploy_entry.tx_hash, deploy_entry);

    let (call_result, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_call",
        &serde_json::json!([
            {
                "chainId": chain_id
            },
            {
                "to": format!("0x{}", to_hex(&contract))
            },
            "latest"
        ]),
    )
    .expect("eth_call mixed array should work");
    assert!(!changed);
    assert_eq!(call_result.as_str(), Some("0x6000"));

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn parse_eth_block_query_tag_prefers_first_block_scalar_over_index_scalar() {
    let params = serde_json::json!([
        {
            "chainId": 1u64
        },
        "pending",
        "0x0"
    ]);
    assert_eq!(
        parse_eth_block_query_tag(&params).as_deref(),
        Some("pending")
    );
    assert_eq!(parse_eth_block_query_tx_index(&params), Some(0));
}

#[test]
fn eth_fee_history_accepts_chain_object_plus_standard_array_params() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let chain_id = 1u64;
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-fee-history-mixed-array-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let (fee_history, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_feeHistory",
        &serde_json::json!([
            {
                "chainId": chain_id
            },
            "0x2",
            "latest",
            [25, 75]
        ]),
    )
    .expect("eth_feeHistory mixed array should work");
    assert!(!changed);
    assert_eq!(fee_history["oldestBlock"].as_str(), Some("0x0"));
    assert_eq!(
        fee_history["baseFeePerGas"].as_array().map(|v| v.len()),
        Some(2)
    );
    assert_eq!(
        fee_history["gasUsedRatio"].as_array().map(|v| v.len()),
        Some(1)
    );
    assert_eq!(fee_history["reward"].as_array().map(|v| v.len()), Some(1));

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_syncing_prefers_chain_scoped_snapshot_fields() {
    let _guard = env_test_guard();
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-syncing-chain-scoped-snapshot-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };
    let sync_env_keys = [
        "NOVOVM_GATEWAY_ETH_SYNC_STARTING_BLOCK",
        "NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK",
        "NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK",
        "NOVOVM_GATEWAY_ETH_SYNC_STARTING_BLOCK_CHAIN_1",
        "NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK_CHAIN_1",
        "NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK_CHAIN_1",
        "NOVOVM_GATEWAY_ETH_SYNC_STARTING_BLOCK_CHAIN_137",
        "NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK_CHAIN_137",
        "NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK_CHAIN_137",
    ];
    let saved_sync_env = capture_env_vars(&sync_env_keys);
    for key in sync_env_keys {
        std::env::remove_var(key);
    }
    let path_env_keys = [
        "NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH",
        "NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_1",
        "NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_0x1",
        "NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_137",
        "NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_0x89",
    ];
    let saved_path_env = capture_env_vars(&path_env_keys);
    for key in path_env_keys {
        std::env::remove_var(key);
    }

    let snapshot_path = spool_dir.join("sync-status.json");
    let snapshot = serde_json::json!({
        "chains": {
            "1": {
                "peerCount": 3,
                "startingBlock": "0x1",
                "currentBlock": "0x10",
                "highestBlock": "0x20"
            },
            "0x89": {
                "peerCount": 9,
                "startingBlock": "0x5",
                "currentBlock": "0x30",
                "highestBlock": "0x40"
            }
        },
        "peerCount": 99,
        "startingBlock": "0x99",
        "currentBlock": "0x99",
        "highestBlock": "0x99"
    });
    fs::write(
        &snapshot_path,
        serde_json::to_vec(&snapshot).expect("serialize snapshot"),
    )
    .expect("write snapshot");

    std::env::set_var(
        "NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH",
        snapshot_path.to_string_lossy().to_string(),
    );

    let (syncing_chain_1, changed_chain_1) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_syncing",
        &serde_json::json!({ "chain_id": 1u64 }),
    )
    .expect("eth_syncing chain 1 should work");
    assert!(!changed_chain_1);
    assert_eq!(syncing_chain_1["startingBlock"].as_str(), Some("0x1"));
    assert_eq!(syncing_chain_1["currentBlock"].as_str(), Some("0x10"));
    assert_eq!(syncing_chain_1["highestBlock"].as_str(), Some("0x20"));

    let (syncing_chain_137, changed_chain_137) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_syncing",
        &serde_json::json!({ "chain_id": 137u64 }),
    )
    .expect("eth_syncing chain 137 should work");
    assert!(!changed_chain_137);
    assert_eq!(syncing_chain_137["startingBlock"].as_str(), Some("0x5"));
    assert_eq!(syncing_chain_137["currentBlock"].as_str(), Some("0x30"));
    assert_eq!(syncing_chain_137["highestBlock"].as_str(), Some("0x40"));

    restore_env_vars(&saved_sync_env);
    restore_env_vars(&saved_path_env);
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_syncing_chain_scoped_env_overrides_take_precedence() {
    let _guard = env_test_guard();
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-syncing-chain-scoped-env-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let sync_env_keys = [
        "NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH",
        "NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_137",
        "NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_0x89",
        "NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK",
        "NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK",
        "NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK_CHAIN_1",
        "NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK_CHAIN_1",
        "NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK_CHAIN_137",
        "NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK_CHAIN_137",
    ];
    let saved_sync_env = capture_env_vars(&sync_env_keys);
    for key in sync_env_keys {
        std::env::remove_var(key);
    }

    std::env::set_var("NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK", "0x10");
    std::env::set_var("NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK", "0x11");
    std::env::set_var("NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK_CHAIN_137", "0x30");
    std::env::set_var("NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK_CHAIN_137", "0x31");

    let (syncing_chain_1, changed_chain_1) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_syncing",
        &serde_json::json!({ "chain_id": 1u64 }),
    )
    .expect("eth_syncing chain 1 from env should work");
    assert!(!changed_chain_1);
    assert_eq!(syncing_chain_1["currentBlock"].as_str(), Some("0x10"));
    assert_eq!(syncing_chain_1["highestBlock"].as_str(), Some("0x11"));

    let (syncing_chain_137, changed_chain_137) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_syncing",
        &serde_json::json!({ "chain_id": 137u64 }),
    )
    .expect("eth_syncing chain 137 from chain-scoped env should work");
    assert!(!changed_chain_137);
    assert_eq!(syncing_chain_137["currentBlock"].as_str(), Some("0x30"));
    assert_eq!(syncing_chain_137["highestBlock"].as_str(), Some("0x31"));

    restore_env_vars(&saved_sync_env);
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_syncing_chain_scoped_status_path_overrides_global_status_path() {
    let _guard = env_test_guard();
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-syncing-chain-path-override-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };
    let sync_env_keys = [
        "NOVOVM_GATEWAY_ETH_SYNC_STARTING_BLOCK",
        "NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK",
        "NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK",
        "NOVOVM_GATEWAY_ETH_SYNC_STARTING_BLOCK_CHAIN_1",
        "NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK_CHAIN_1",
        "NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK_CHAIN_1",
        "NOVOVM_GATEWAY_ETH_SYNC_STARTING_BLOCK_CHAIN_137",
        "NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK_CHAIN_137",
        "NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK_CHAIN_137",
    ];
    let saved_sync_env = capture_env_vars(&sync_env_keys);
    for key in sync_env_keys {
        std::env::remove_var(key);
    }

    let global_snapshot_path = spool_dir.join("sync-status-global.json");
    let chain_snapshot_path = spool_dir.join("sync-status-chain-137.json");
    let global_snapshot = serde_json::json!({
        "chains": {
            "1": {
                "startingBlock": "0x1",
                "currentBlock": "0x10",
                "highestBlock": "0x20"
            },
            "137": {
                "startingBlock": "0x2",
                "currentBlock": "0x21",
                "highestBlock": "0x22"
            }
        }
    });
    let chain_snapshot = serde_json::json!({
        "startingBlock": "0x5",
        "currentBlock": "0x30",
        "highestBlock": "0x40"
    });
    fs::write(
        &global_snapshot_path,
        serde_json::to_vec(&global_snapshot).expect("serialize global snapshot"),
    )
    .expect("write global snapshot");
    fs::write(
        &chain_snapshot_path,
        serde_json::to_vec(&chain_snapshot).expect("serialize chain snapshot"),
    )
    .expect("write chain snapshot");

    let prev_status_path = std::env::var("NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH").ok();
    let prev_chain_status_path =
        std::env::var("NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_137").ok();
    std::env::set_var(
        "NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH",
        global_snapshot_path.to_string_lossy().to_string(),
    );
    std::env::set_var(
        "NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_137",
        chain_snapshot_path.to_string_lossy().to_string(),
    );

    let (syncing_chain_1, changed_chain_1) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_syncing",
        &serde_json::json!({ "chain_id": 1u64 }),
    )
    .expect("eth_syncing chain 1 should use global status path");
    assert!(!changed_chain_1);
    assert_eq!(syncing_chain_1["startingBlock"].as_str(), Some("0x1"));
    assert_eq!(syncing_chain_1["currentBlock"].as_str(), Some("0x10"));
    assert_eq!(syncing_chain_1["highestBlock"].as_str(), Some("0x20"));

    let (syncing_chain_137, changed_chain_137) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_syncing",
        &serde_json::json!({ "chain_id": 137u64 }),
    )
    .expect("eth_syncing chain 137 should use chain-scoped status path");
    assert!(!changed_chain_137);
    assert_eq!(syncing_chain_137["startingBlock"].as_str(), Some("0x5"));
    assert_eq!(syncing_chain_137["currentBlock"].as_str(), Some("0x30"));
    assert_eq!(syncing_chain_137["highestBlock"].as_str(), Some("0x40"));

    restore_env_vars(&saved_sync_env);
    if let Some(value) = prev_status_path {
        std::env::set_var("NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH", value);
    } else {
        std::env::remove_var("NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH");
    }
    if let Some(value) = prev_chain_status_path {
        std::env::set_var("NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_137", value);
    } else {
        std::env::remove_var("NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_137");
    }
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn evm_atomic_broadcast_chain_scoped_exec_env_overrides_take_precedence() {
    let _guard = env_test_guard();
    let env_keys = [
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC",
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_CHAIN_137",
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_CHAIN_0x89",
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY",
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_CHAIN_137",
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_TIMEOUT_MS",
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_TIMEOUT_MS_CHAIN_137",
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_BACKOFF_MS",
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_BACKOFF_MS_CHAIN_0x89",
    ];
    let saved_env = capture_env_vars(&env_keys);
    for key in env_keys {
        std::env::remove_var(key);
    }

    std::env::set_var("NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC", "global-exec");
    std::env::set_var(
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_CHAIN_137",
        "chain-137-exec",
    );
    std::env::set_var("NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY", "2");
    std::env::set_var(
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_CHAIN_137",
        "5",
    );
    std::env::set_var(
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_TIMEOUT_MS",
        "3000",
    );
    std::env::set_var(
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_TIMEOUT_MS_CHAIN_137",
        "4500",
    );
    std::env::set_var(
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_BACKOFF_MS",
        "20",
    );
    std::env::set_var(
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_BACKOFF_MS_CHAIN_0x89",
        "35",
    );

    assert_eq!(
        gateway_evm_atomic_broadcast_exec_path(1),
        Some(PathBuf::from("global-exec"))
    );
    assert_eq!(
        gateway_evm_atomic_broadcast_exec_path(137),
        Some(PathBuf::from("chain-137-exec"))
    );
    assert_eq!(gateway_evm_atomic_broadcast_exec_retry_default(1), 2);
    assert_eq!(gateway_evm_atomic_broadcast_exec_retry_default(137), 5);
    assert_eq!(
        gateway_evm_atomic_broadcast_exec_timeout_ms_default(1),
        3000
    );
    assert_eq!(
        gateway_evm_atomic_broadcast_exec_timeout_ms_default(137),
        4500
    );
    assert_eq!(
        gateway_evm_atomic_broadcast_exec_retry_backoff_ms_default(1),
        20
    );
    assert_eq!(
        gateway_evm_atomic_broadcast_exec_retry_backoff_ms_default(137),
        35
    );

    restore_env_vars(&saved_env);
}

#[test]
fn eth_syncing_env_values_override_snapshot_values_when_both_present() {
    let _guard = env_test_guard();
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-syncing-env-overrides-snapshot-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let snapshot_path = spool_dir.join("sync-status.json");
    let snapshot = serde_json::json!({
        "chains": {
            "1": {
                "startingBlock": "0x1",
                "currentBlock": "0x10",
                "highestBlock": "0x20"
            }
        }
    });
    fs::write(
        &snapshot_path,
        serde_json::to_vec(&snapshot).expect("serialize snapshot"),
    )
    .expect("write snapshot");

    let sync_env_keys = [
        "NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH",
        "NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_1",
        "NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_0x1",
        "NOVOVM_GATEWAY_ETH_SYNC_STARTING_BLOCK",
        "NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK",
        "NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK",
        "NOVOVM_GATEWAY_ETH_SYNC_STARTING_BLOCK_CHAIN_1",
        "NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK_CHAIN_1",
        "NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK_CHAIN_1",
        "NOVOVM_GATEWAY_ETH_SYNC_STARTING_BLOCK_CHAIN_0x1",
        "NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK_CHAIN_0x1",
        "NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK_CHAIN_0x1",
    ];
    let saved_sync_env = capture_env_vars(&sync_env_keys);
    for key in sync_env_keys {
        std::env::remove_var(key);
    }

    std::env::set_var(
        "NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH",
        snapshot_path.to_string_lossy().to_string(),
    );
    std::env::set_var("NOVOVM_GATEWAY_ETH_SYNC_STARTING_BLOCK", "0x5");
    std::env::set_var("NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK", "0x30");
    std::env::set_var("NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK", "0x31");

    let (syncing, changed_syncing) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_syncing",
        &serde_json::json!({ "chain_id": 1u64 }),
    )
    .expect("eth_syncing should prefer env overrides");
    assert!(!changed_syncing);
    assert_eq!(syncing["startingBlock"].as_str(), Some("0x5"));
    assert_eq!(syncing["currentBlock"].as_str(), Some("0x30"));
    assert_eq!(syncing["highestBlock"].as_str(), Some("0x31"));

    restore_env_vars(&saved_sync_env);
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_tx_and_receipt_query_fallback_keeps_confirmed_semantics_when_scan_window_truncated() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let chain_id = 9u64;
    let mut eth_tx_index = HashMap::new();

    for idx in 0..GATEWAY_ETH_QUERY_SCAN_MAX_DEFAULT {
        let mut tx_hash = [0u8; 32];
        tx_hash[..8].copy_from_slice(&(idx as u64).to_le_bytes());
        eth_tx_index.insert(
            tx_hash,
            GatewayEthTxIndexEntry {
                tx_hash,
                uca_id: format!("uca-scan-{}", idx),
                chain_id,
                nonce: idx as u64,
                tx_type: 2,
                from: vec![0x11u8; 20],
                to: Some(vec![0x22u8; 20]),
                value: 1,
                gas_limit: 21_000,
                gas_price: 3,
                input: vec![0x01],
            },
        );
    }

    let target = GatewayEthTxIndexEntry {
        tx_hash: [0xf3u8; 32],
        uca_id: "uca-target".to_string(),
        chain_id,
        nonce: 99_999,
        tx_type: 2,
        from: vec![0x33u8; 20],
        to: Some(vec![0x44u8; 20]),
        value: 7,
        gas_limit: 52_000,
        gas_price: 9,
        input: vec![0x60, 0x00],
    };
    backend
        .save_eth_tx(&target)
        .expect("save target tx into store");

    let tx_json = gateway_eth_tx_by_hash_query_json(&target, &eth_tx_index, &backend)
        .expect("query tx by hash should work");
    let expected_block_number = format!("0x{:x}", target.nonce);
    assert_eq!(tx_json["pending"].as_bool(), Some(false));
    assert_eq!(
        tx_json["blockNumber"].as_str(),
        Some(expected_block_number.as_str())
    );
    assert!(tx_json["blockHash"].is_null());
    assert!(tx_json["transactionIndex"].is_null());

    let receipt_json = gateway_eth_tx_receipt_query_json(&target, &eth_tx_index, &backend)
        .expect("query tx receipt should work");
    assert_eq!(receipt_json["pending"].as_bool(), Some(false));
    assert_eq!(
        receipt_json["blockNumber"].as_str(),
        Some(expected_block_number.as_str())
    );
    assert_eq!(receipt_json["status"].as_str(), Some("0x1"));
    assert!(receipt_json["blockHash"].is_null());
    assert!(receipt_json["transactionIndex"].is_null());
}

#[test]
fn eth_tx_and_receipt_query_recover_confirmed_position_from_store_block_index() {
    let _guard = env_test_guard();
    let chain_id = 9u64;
    let rocksdb_path = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-block-index-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: rocksdb_path.clone(),
    };
    let mut eth_tx_index = HashMap::new();
    for idx in 0..GATEWAY_ETH_QUERY_SCAN_MAX_DEFAULT {
        let mut tx_hash = [0u8; 32];
        tx_hash[..8].copy_from_slice(&(idx as u64).to_le_bytes());
        eth_tx_index.insert(
            tx_hash,
            GatewayEthTxIndexEntry {
                tx_hash,
                uca_id: format!("uca-fill-{}", idx),
                chain_id,
                nonce: idx as u64,
                tx_type: 0,
                from: vec![0x11u8; 20],
                to: Some(vec![0x22u8; 20]),
                value: 1,
                gas_limit: 21_000,
                gas_price: 3,
                input: vec![0x00],
            },
        );
    }

    let block_number = 99_999u64;
    let sibling = GatewayEthTxIndexEntry {
        tx_hash: [0x11u8; 32],
        uca_id: "uca-sibling".to_string(),
        chain_id,
        nonce: block_number,
        tx_type: 2,
        from: vec![0x33u8; 20],
        to: Some(vec![0x44u8; 20]),
        value: 3,
        gas_limit: 25_000,
        gas_price: 9,
        input: vec![0x60, 0x01],
    };
    let target = GatewayEthTxIndexEntry {
        tx_hash: [0xf3u8; 32],
        uca_id: "uca-target".to_string(),
        chain_id,
        nonce: block_number,
        tx_type: 2,
        from: vec![0x33u8; 20],
        to: Some(vec![0x55u8; 20]),
        value: 7,
        gas_limit: 52_000,
        gas_price: 11,
        input: vec![0x60, 0x02],
    };
    backend
        .save_eth_tx(&sibling)
        .expect("save sibling tx into store");
    backend
        .save_eth_tx(&target)
        .expect("save target tx into store");

    let tx_json = gateway_eth_tx_by_hash_query_json(&target, &eth_tx_index, &backend)
        .expect("query tx by hash should work");
    let expected_block_number = format!("0x{:x}", block_number);
    assert_eq!(tx_json["pending"].as_bool(), Some(false));
    assert_eq!(
        tx_json["blockNumber"].as_str(),
        Some(expected_block_number.as_str())
    );
    assert_eq!(tx_json["transactionIndex"].as_str(), Some("0x1"));
    let mut expected_block_txs = vec![sibling.clone(), target.clone()];
    sort_gateway_eth_block_txs(&mut expected_block_txs);
    let expected_block_hash =
        gateway_eth_block_hash_for_txs(chain_id, block_number, &expected_block_txs);
    assert_eq!(
        tx_json["blockHash"].as_str(),
        Some(format!("0x{}", to_hex(&expected_block_hash)).as_str())
    );

    let receipt_json = gateway_eth_tx_receipt_query_json(&target, &eth_tx_index, &backend)
        .expect("query tx receipt should work");
    assert_eq!(receipt_json["pending"].as_bool(), Some(false));
    assert_eq!(
        receipt_json["blockNumber"].as_str(),
        Some(expected_block_number.as_str())
    );
    assert_eq!(receipt_json["transactionIndex"].as_str(), Some("0x1"));
    assert_eq!(receipt_json["status"].as_str(), Some("0x1"));
    assert_eq!(
        receipt_json["blockHash"].as_str(),
        Some(format!("0x{}", to_hex(&expected_block_hash)).as_str())
    );
    let _ = fs::remove_dir_all(&rocksdb_path);
}

#[test]
fn eth_store_chain_scan_uses_latest_block_index_window() {
    let chain_id = 17u64;
    let rocksdb_path = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-chain-scan-latest-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: rocksdb_path.clone(),
    };
    let tx_low = GatewayEthTxIndexEntry {
        tx_hash: [0xffu8; 32],
        uca_id: "uca-low".to_string(),
        chain_id,
        nonce: 3,
        tx_type: 2,
        from: vec![0x11u8; 20],
        to: Some(vec![0x22u8; 20]),
        value: 1,
        gas_limit: 21_000,
        gas_price: 1,
        input: vec![],
    };
    let tx_high = GatewayEthTxIndexEntry {
        tx_hash: [0x00u8; 32],
        uca_id: "uca-high".to_string(),
        chain_id,
        nonce: 9,
        tx_type: 2,
        from: vec![0x33u8; 20],
        to: Some(vec![0x44u8; 20]),
        value: 2,
        gas_limit: 21_000,
        gas_price: 2,
        input: vec![],
    };
    backend.save_eth_tx(&tx_low).expect("save low tx");
    backend.save_eth_tx(&tx_high).expect("save high tx");

    let sampled = backend
        .load_eth_txs_by_chain(chain_id, 1)
        .expect("load chain entries should work");
    assert_eq!(sampled.len(), 1);
    assert_eq!(sampled[0].nonce, 9);
    assert_eq!(sampled[0].tx_hash, tx_high.tx_hash);

    let latest = backend
        .load_eth_latest_block_number(chain_id)
        .expect("load latest block should work");
    assert_eq!(latest, Some(9));
    let _ = fs::remove_dir_all(&rocksdb_path);
}

#[test]
fn collect_chain_entries_prefers_latest_window_for_memory_index() {
    let chain_id = 18u64;
    let mut eth_tx_index = HashMap::new();
    for nonce in 0..10u64 {
        let mut tx_hash = [0u8; 32];
        tx_hash[0] = nonce as u8;
        tx_hash[31] = (nonce * 3) as u8;
        eth_tx_index.insert(
            tx_hash,
            GatewayEthTxIndexEntry {
                tx_hash,
                uca_id: format!("uca-{}", nonce),
                chain_id,
                nonce,
                tx_type: 2,
                from: vec![0x11u8; 20],
                to: Some(vec![0x22u8; 20]),
                value: nonce as u128,
                gas_limit: 21_000,
                gas_price: 1,
                input: vec![],
            },
        );
    }
    let out = collect_gateway_eth_chain_entries(
        &eth_tx_index,
        &GatewayEthTxIndexStoreBackend::Memory,
        chain_id,
        3,
    )
    .expect("collect chain entries should work");
    assert_eq!(out.len(), 3);
    let nonces = out
        .into_iter()
        .map(|entry| entry.nonce)
        .collect::<Vec<u64>>();
    assert_eq!(nonces, vec![7, 8, 9]);
}

#[test]
fn eth_block_number_uses_store_latest_block_when_scan_window_small() {
    let _guard = env_test_guard();
    let chain_id = 27u64;
    let rocksdb_path = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-blocknumber-latest-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: rocksdb_path.clone(),
    };
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-blocknumber-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let tx_a = GatewayEthTxIndexEntry {
        tx_hash: [0x01u8; 32],
        uca_id: "uca-a".to_string(),
        chain_id,
        nonce: 4,
        tx_type: 2,
        from: vec![0x11u8; 20],
        to: Some(vec![0x22u8; 20]),
        value: 1,
        gas_limit: 21_000,
        gas_price: 1,
        input: vec![],
    };
    let tx_b = GatewayEthTxIndexEntry {
        tx_hash: [0x02u8; 32],
        uca_id: "uca-b".to_string(),
        chain_id,
        nonce: 11,
        tx_type: 2,
        from: vec![0x33u8; 20],
        to: Some(vec![0x44u8; 20]),
        value: 2,
        gas_limit: 21_000,
        gas_price: 2,
        input: vec![],
    };
    backend.save_eth_tx(&tx_a).expect("save tx_a");
    backend.save_eth_tx(&tx_b).expect("save tx_b");

    let prev_scan_max = std::env::var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX").ok();
    std::env::set_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX", "1");
    let (block_number_raw, changed_block_number) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_blockNumber",
        &serde_json::json!({ "chain_id": chain_id }),
    )
    .expect("eth_blockNumber should use latest store block");
    assert!(!changed_block_number);
    assert_eq!(block_number_raw.as_str(), Some("0xb"));
    if let Some(value) = prev_scan_max {
        std::env::set_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX", value);
    } else {
        std::env::remove_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX");
    }
    let _ = fs::remove_dir_all(&spool_dir);
    let _ = fs::remove_dir_all(&rocksdb_path);
}

#[test]
fn eth_syncing_uses_store_latest_block_when_scan_window_small() {
    let _guard = env_test_guard();
    let chain_id = 28u64;
    let rocksdb_path = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-syncing-latest-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: rocksdb_path.clone(),
    };
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-syncing-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let tx_a = GatewayEthTxIndexEntry {
        tx_hash: [0x11u8; 32],
        uca_id: "uca-a".to_string(),
        chain_id,
        nonce: 4,
        tx_type: 2,
        from: vec![0x11u8; 20],
        to: Some(vec![0x22u8; 20]),
        value: 1,
        gas_limit: 21_000,
        gas_price: 1,
        input: vec![],
    };
    let tx_b = GatewayEthTxIndexEntry {
        tx_hash: [0x12u8; 32],
        uca_id: "uca-b".to_string(),
        chain_id,
        nonce: 11,
        tx_type: 2,
        from: vec![0x33u8; 20],
        to: Some(vec![0x44u8; 20]),
        value: 2,
        gas_limit: 21_000,
        gas_price: 2,
        input: vec![],
    };
    backend.save_eth_tx(&tx_a).expect("save tx_a");
    backend.save_eth_tx(&tx_b).expect("save tx_b");

    let prev_scan_max = std::env::var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX").ok();
    let prev_sync_current = std::env::var("NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK").ok();
    let prev_sync_highest = std::env::var("NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK").ok();
    std::env::set_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX", "1");
    std::env::set_var("NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK", "0x3");
    std::env::set_var("NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK", "0x5");

    let (syncing_raw, changed_syncing) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_syncing",
        &serde_json::json!({ "chain_id": chain_id }),
    )
    .expect("eth_syncing should use latest store block");
    assert!(!changed_syncing);
    assert_eq!(syncing_raw, serde_json::Value::Bool(false));

    if let Some(value) = prev_scan_max {
        std::env::set_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX", value);
    } else {
        std::env::remove_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX");
    }
    if let Some(value) = prev_sync_current {
        std::env::set_var("NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK", value);
    } else {
        std::env::remove_var("NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK");
    }
    if let Some(value) = prev_sync_highest {
        std::env::set_var("NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK", value);
    } else {
        std::env::remove_var("NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK");
    }
    let _ = fs::remove_dir_all(&spool_dir);
    let _ = fs::remove_dir_all(&rocksdb_path);
}

#[test]
fn eth_pending_block_queries_return_null_without_runtime_pending_txs() {
    let _guard = env_test_guard();
    let chain_id = 18_001u64;
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-pending-boundary-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    eth_tx_index.insert(
        [0x41u8; 32],
        GatewayEthTxIndexEntry {
            tx_hash: [0x41u8; 32],
            uca_id: "uca-confirmed".to_string(),
            chain_id,
            nonce: 9,
            tx_type: 2,
            from: vec![0x11u8; 20],
            to: Some(vec![0x22u8; 20]),
            value: 1,
            gas_limit: 21_000,
            gas_price: 1,
            input: vec![],
        },
    );

    let (pending_block, changed_pending_block) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockByNumber",
        &serde_json::json!({
            "chain_id": chain_id,
            "block": "pending",
        }),
    )
    .expect("eth_getBlockByNumber pending should work");
    assert!(!changed_pending_block);
    assert!(pending_block.is_null());

    let (pending_receipts, changed_pending_receipts) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockReceipts",
        &serde_json::json!({
            "chain_id": chain_id,
            "block": "pending",
        }),
    )
    .expect("eth_getBlockReceipts pending should work");
    assert!(!changed_pending_receipts);
    assert!(pending_receipts.is_null());

    let (pending_tx_by_index, changed_pending_tx_by_index) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByBlockNumberAndIndex",
        &serde_json::json!({
            "chain_id": chain_id,
            "block": "pending",
            "transaction_index": "0x0",
        }),
    )
    .expect("eth_getTransactionByBlockNumberAndIndex pending should work");
    assert!(!changed_pending_tx_by_index);
    assert!(pending_tx_by_index.is_null());

    let (pending_count, changed_pending_count) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockTransactionCountByNumber",
        &serde_json::json!({
            "chain_id": chain_id,
            "block": "pending",
        }),
    )
    .expect("eth_getBlockTransactionCountByNumber pending should work");
    assert!(!changed_pending_count);
    assert!(pending_count.is_null());

    let (pending_uncles, changed_pending_uncles) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getUncleCountByBlockNumber",
        &serde_json::json!({
            "chain_id": chain_id,
            "block": "pending",
        }),
    )
    .expect("eth_getUncleCountByBlockNumber pending should work");
    assert!(!changed_pending_uncles);
    assert!(pending_uncles.is_null());

    let (syncing, changed_syncing) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_syncing",
        &serde_json::json!({
            "chain_id": chain_id,
        }),
    )
    .expect("eth_syncing should work");
    assert!(!changed_syncing);
    assert_eq!(syncing, serde_json::Value::Bool(false));

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_runtime_pending_tx_by_hash_uses_store_latest_height_when_memory_window_stale() {
    let _guard = env_test_guard();
    let chain_id = 99_301u64;
    let rocksdb_path = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-runtime-pending-latest-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: rocksdb_path.clone(),
    };
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-runtime-pending-latest-spool-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    eth_tx_index.insert(
        [0x51u8; 32],
        GatewayEthTxIndexEntry {
            tx_hash: [0x51u8; 32],
            uca_id: "uca-stale".to_string(),
            chain_id,
            nonce: 5,
            tx_type: 0,
            from: vec![0x11u8; 20],
            to: Some(vec![0x22u8; 20]),
            value: 1,
            gas_limit: 21_000,
            gas_price: 3,
            input: vec![0x00],
        },
    );
    backend
        .save_eth_tx(&GatewayEthTxIndexEntry {
            tx_hash: [0x61u8; 32],
            uca_id: "uca-store-latest".to_string(),
            chain_id,
            nonce: 500,
            tx_type: 0,
            from: vec![0x33u8; 20],
            to: Some(vec![0x44u8; 20]),
            value: 2,
            gas_limit: 25_000,
            gas_price: 5,
            input: vec![0x01],
        })
        .expect("save store latest tx");

    let mut runtime_tx = TxIR::transfer(vec![0x77u8; 20], vec![0x88u8; 20], 10, 1, chain_id);
    runtime_tx.compute_hash();
    let tap_summary = runtime_tap_ir_batch_v1(
        novovm_adapter_api::ChainType::EVM,
        chain_id,
        &[runtime_tx.clone()],
        0,
    )
    .expect("runtime tap should accept tx");
    assert_eq!(tap_summary.accepted, 1);

    let prev_scan_max = std::env::var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX").ok();
    std::env::set_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX", "1");

    let (tx_by_hash, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByHash",
        &serde_json::json!({
            "chain_id": chain_id,
            "tx_hash": format!("0x{}", to_hex(&runtime_tx.hash)),
        }),
    )
    .expect("eth_getTransactionByHash runtime should work");
    assert!(!changed);
    assert_eq!(tx_by_hash["pending"].as_bool(), Some(true));
    assert_eq!(tx_by_hash["blockNumber"].as_str(), Some("0x1f5"));

    if let Some(value) = prev_scan_max {
        std::env::set_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX", value);
    } else {
        std::env::remove_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX");
    }
    let _ = fs::remove_dir_all(&spool_dir);
    let _ = fs::remove_dir_all(&rocksdb_path);
}

#[test]
fn eth_get_block_receipts_recovers_confirmed_block_from_store_when_scan_window_truncated() {
    let chain_id = 9u64;
    let rocksdb_path = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-receipts-block-index-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: rocksdb_path.clone(),
    };
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-receipts-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    for idx in 0..GATEWAY_ETH_QUERY_SCAN_MAX_DEFAULT {
        let mut tx_hash = [0u8; 32];
        tx_hash[..8].copy_from_slice(&(idx as u64).to_le_bytes());
        eth_tx_index.insert(
            tx_hash,
            GatewayEthTxIndexEntry {
                tx_hash,
                uca_id: format!("uca-fill-{}", idx),
                chain_id,
                nonce: idx as u64,
                tx_type: 0,
                from: vec![0x11u8; 20],
                to: Some(vec![0x22u8; 20]),
                value: 1,
                gas_limit: 21_000,
                gas_price: 3,
                input: vec![0x00],
            },
        );
    }

    let block_number = 99_999u64;
    let tx_a = GatewayEthTxIndexEntry {
        tx_hash: [0x11u8; 32],
        uca_id: "uca-a".to_string(),
        chain_id,
        nonce: block_number,
        tx_type: 2,
        from: vec![0x33u8; 20],
        to: Some(vec![0x44u8; 20]),
        value: 3,
        gas_limit: 25_000,
        gas_price: 9,
        input: vec![0x60, 0x01],
    };
    let tx_b = GatewayEthTxIndexEntry {
        tx_hash: [0xf3u8; 32],
        uca_id: "uca-b".to_string(),
        chain_id,
        nonce: block_number,
        tx_type: 2,
        from: vec![0x33u8; 20],
        to: Some(vec![0x55u8; 20]),
        value: 7,
        gas_limit: 52_000,
        gas_price: 11,
        input: vec![0x60, 0x02],
    };
    backend.save_eth_tx(&tx_a).expect("save tx_a");
    backend.save_eth_tx(&tx_b).expect("save tx_b");

    let (receipts_raw, changed_receipts) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockReceipts",
        &serde_json::json!({
            "chain_id": chain_id,
            "block": format!("0x{:x}", block_number),
        }),
    )
    .expect("eth_getBlockReceipts should recover block from store block index");
    assert!(!changed_receipts);
    let receipts = receipts_raw
        .as_array()
        .expect("recovered receipts should be array");
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0]["pending"].as_bool(), Some(false));
    assert_eq!(receipts[1]["pending"].as_bool(), Some(false));
    assert_eq!(receipts[0]["status"].as_str(), Some("0x1"));
    assert_eq!(receipts[1]["status"].as_str(), Some("0x1"));
    assert_eq!(
        receipts[0]["blockNumber"].as_str(),
        Some(format!("0x{:x}", block_number).as_str())
    );
    assert_eq!(
        receipts[1]["blockNumber"].as_str(),
        Some(format!("0x{:x}", block_number).as_str())
    );
    let _ = fs::remove_dir_all(&spool_dir);
    let _ = fs::remove_dir_all(&rocksdb_path);
}

#[test]
fn eth_fee_history_recovers_block_usage_from_store_when_scan_window_truncated() {
    let _guard = env_test_guard();
    let chain_id = 108u64;
    let rocksdb_path = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-fee-history-store-recover-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: rocksdb_path.clone(),
    };
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-fee-history-store-recover-spool-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    eth_tx_index.insert(
        [0x01u8; 32],
        GatewayEthTxIndexEntry {
            tx_hash: [0x01u8; 32],
            uca_id: "uca-mem".to_string(),
            chain_id,
            nonce: 5,
            tx_type: 0,
            from: vec![0x11u8; 20],
            to: Some(vec![0x22u8; 20]),
            value: 1,
            gas_limit: 21_000,
            gas_price: 1,
            input: vec![0x00],
        },
    );

    let block_number = 500u64;
    backend
        .save_eth_tx(&GatewayEthTxIndexEntry {
            tx_hash: [0x55u8; 32],
            uca_id: "uca-store-500".to_string(),
            chain_id,
            nonce: block_number,
            tx_type: 2,
            from: vec![0x33u8; 20],
            to: Some(vec![0x44u8; 20]),
            value: 7,
            gas_limit: 15_000_000,
            gas_price: 9,
            input: vec![0x60, 0x01],
        })
        .expect("save tx at store block 500");

    let prev_scan_max = std::env::var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX").ok();
    std::env::set_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX", "1");

    let (fee_history_raw, changed_fee_history) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_feeHistory",
        &serde_json::json!({
            "chain_id": chain_id,
            "blockCount": 1,
            "newestBlock": format!("0x{:x}", block_number),
            "rewardPercentiles": [50.0],
        }),
    )
    .expect("eth_feeHistory should recover usage from store block index");
    assert!(!changed_fee_history);
    assert_eq!(fee_history_raw["oldestBlock"].as_str(), Some("0x1f4"));
    let ratios = fee_history_raw["gasUsedRatio"]
        .as_array()
        .expect("gasUsedRatio should be array");
    assert_eq!(ratios.len(), 1);
    let ratio = ratios[0].as_f64().expect("gasUsedRatio item should be f64");
    assert!((ratio - 0.5).abs() < 1e-9);
    assert_eq!(fee_history_raw["reward"][0][0].as_str(), Some("0x9"));

    if let Some(value) = prev_scan_max {
        std::env::set_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX", value);
    } else {
        std::env::remove_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX");
    }
    let _ = fs::remove_dir_all(&spool_dir);
    let _ = fs::remove_dir_all(&rocksdb_path);
}

#[test]
fn eth_block_number_queries_recover_from_store_when_scan_window_truncated() {
    let chain_id = 9u64;
    let rocksdb_path = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-block-query-index-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: rocksdb_path.clone(),
    };
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-block-query-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    for idx in 0..GATEWAY_ETH_QUERY_SCAN_MAX_DEFAULT {
        let mut tx_hash = [0u8; 32];
        tx_hash[..8].copy_from_slice(&(idx as u64).to_le_bytes());
        eth_tx_index.insert(
            tx_hash,
            GatewayEthTxIndexEntry {
                tx_hash,
                uca_id: format!("uca-fill-{}", idx),
                chain_id,
                nonce: idx as u64,
                tx_type: 0,
                from: vec![0x11u8; 20],
                to: Some(vec![0x22u8; 20]),
                value: 1,
                gas_limit: 21_000,
                gas_price: 3,
                input: vec![0x00],
            },
        );
    }

    let block_number = 99_999u64;
    let tx_a = GatewayEthTxIndexEntry {
        tx_hash: [0x11u8; 32],
        uca_id: "uca-a".to_string(),
        chain_id,
        nonce: block_number,
        tx_type: 2,
        from: vec![0x33u8; 20],
        to: Some(vec![0x44u8; 20]),
        value: 3,
        gas_limit: 25_000,
        gas_price: 9,
        input: vec![0x60, 0x01],
    };
    let tx_b = GatewayEthTxIndexEntry {
        tx_hash: [0xf3u8; 32],
        uca_id: "uca-b".to_string(),
        chain_id,
        nonce: block_number,
        tx_type: 2,
        from: vec![0x33u8; 20],
        to: Some(vec![0x55u8; 20]),
        value: 7,
        gas_limit: 52_000,
        gas_price: 11,
        input: vec![0x60, 0x02],
    };
    backend.save_eth_tx(&tx_a).expect("save tx_a");
    backend.save_eth_tx(&tx_b).expect("save tx_b");
    let mut expected_block_txs = vec![tx_a.clone(), tx_b.clone()];
    sort_gateway_eth_block_txs(&mut expected_block_txs);
    let expected_block_hash =
        gateway_eth_block_hash_for_txs(chain_id, block_number, &expected_block_txs);

    let (block_raw, changed_block) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockByNumber",
        &serde_json::json!({
            "chain_id": chain_id,
            "block": format!("0x{:x}", block_number),
            "full_transactions": true,
        }),
    )
    .expect("eth_getBlockByNumber should recover block from store block index");
    assert!(!changed_block);
    assert_eq!(
        block_raw["number"].as_str(),
        Some(format!("0x{:x}", block_number).as_str())
    );
    assert_eq!(
        block_raw["hash"].as_str(),
        Some(format!("0x{}", to_hex(&expected_block_hash)).as_str())
    );
    let block_txs = block_raw["transactions"]
        .as_array()
        .expect("transactions should be array");
    assert_eq!(block_txs.len(), 2);
    assert_eq!(block_txs[0]["pending"].as_bool(), Some(false));
    assert_eq!(block_txs[1]["pending"].as_bool(), Some(false));

    let (count_raw, changed_count) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockTransactionCountByNumber",
        &serde_json::json!({
            "chain_id": chain_id,
            "block": format!("0x{:x}", block_number),
        }),
    )
    .expect("eth_getBlockTransactionCountByNumber should recover count from store block index");
    assert!(!changed_count);
    assert_eq!(count_raw.as_str(), Some("0x2"));

    let (tx_raw, changed_tx) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByBlockNumberAndIndex",
        &serde_json::json!({
            "chain_id": chain_id,
            "block": format!("0x{:x}", block_number),
            "transaction_index": "0x1",
        }),
    )
    .expect("eth_getTransactionByBlockNumberAndIndex should recover tx from store block index");
    assert!(!changed_tx);
    assert_eq!(tx_raw["pending"].as_bool(), Some(false));
    assert_eq!(tx_raw["transactionIndex"].as_str(), Some("0x1"));
    assert_eq!(
        tx_raw["hash"].as_str(),
        Some(format!("0x{}", to_hex(&expected_block_txs[1].tx_hash)).as_str())
    );

    let (uncle_count_raw, changed_uncle_count) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getUncleCountByBlockNumber",
        &serde_json::json!({
            "chain_id": chain_id,
            "block": format!("0x{:x}", block_number),
        }),
    )
    .expect("eth_getUncleCountByBlockNumber should recover block existence from store block index");
    assert!(!changed_uncle_count);
    assert_eq!(uncle_count_raw.as_str(), Some("0x0"));

    let _ = fs::remove_dir_all(&spool_dir);
    let _ = fs::remove_dir_all(&rocksdb_path);
}

#[test]
fn eth_block_hash_queries_recover_from_store_when_scan_window_truncated() {
    let chain_id = 9u64;
    let rocksdb_path = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-block-hash-query-index-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: rocksdb_path.clone(),
    };
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-block-hash-query-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    for idx in 0..GATEWAY_ETH_QUERY_SCAN_MAX_DEFAULT {
        let mut tx_hash = [0u8; 32];
        tx_hash[..8].copy_from_slice(&(idx as u64).to_le_bytes());
        eth_tx_index.insert(
            tx_hash,
            GatewayEthTxIndexEntry {
                tx_hash,
                uca_id: format!("uca-fill-{}", idx),
                chain_id,
                nonce: idx as u64,
                tx_type: 0,
                from: vec![0x11u8; 20],
                to: Some(vec![0x22u8; 20]),
                value: 1,
                gas_limit: 21_000,
                gas_price: 3,
                input: vec![0x00],
            },
        );
    }

    let block_number = 99_999u64;
    let tx_a = GatewayEthTxIndexEntry {
        tx_hash: [0x11u8; 32],
        uca_id: "uca-a".to_string(),
        chain_id,
        nonce: block_number,
        tx_type: 2,
        from: vec![0x33u8; 20],
        to: Some(vec![0x44u8; 20]),
        value: 3,
        gas_limit: 25_000,
        gas_price: 9,
        input: vec![0x60, 0x01],
    };
    let tx_b = GatewayEthTxIndexEntry {
        tx_hash: [0xf3u8; 32],
        uca_id: "uca-b".to_string(),
        chain_id,
        nonce: block_number,
        tx_type: 2,
        from: vec![0x33u8; 20],
        to: Some(vec![0x55u8; 20]),
        value: 7,
        gas_limit: 52_000,
        gas_price: 11,
        input: vec![0x60, 0x02],
    };
    backend.save_eth_tx(&tx_a).expect("save tx_a");
    backend.save_eth_tx(&tx_b).expect("save tx_b");
    let mut expected_block_txs = vec![tx_a.clone(), tx_b.clone()];
    sort_gateway_eth_block_txs(&mut expected_block_txs);
    let expected_block_hash =
        gateway_eth_block_hash_for_txs(chain_id, block_number, &expected_block_txs);
    let expected_block_hash_hex = format!("0x{}", to_hex(&expected_block_hash));

    let (block_raw, changed_block) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockByHash",
        &serde_json::json!({
            "chain_id": chain_id,
            "block_hash": expected_block_hash_hex,
            "full_transactions": true,
        }),
    )
    .expect("eth_getBlockByHash should recover block from store hash index");
    assert!(!changed_block);
    assert_eq!(
        block_raw["number"].as_str(),
        Some(format!("0x{:x}", block_number).as_str())
    );
    assert_eq!(
        block_raw["hash"].as_str(),
        Some(format!("0x{}", to_hex(&expected_block_hash)).as_str())
    );
    let block_txs = block_raw["transactions"]
        .as_array()
        .expect("transactions should be array");
    assert_eq!(block_txs.len(), 2);
    assert_eq!(block_txs[0]["pending"].as_bool(), Some(false));
    assert_eq!(block_txs[1]["pending"].as_bool(), Some(false));

    let (tx_raw, changed_tx) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionByBlockHashAndIndex",
        &serde_json::json!({
            "chain_id": chain_id,
            "block_hash": format!("0x{}", to_hex(&expected_block_hash)),
            "transaction_index": "0x1",
        }),
    )
    .expect("eth_getTransactionByBlockHashAndIndex should recover tx from store hash index");
    assert!(!changed_tx);
    assert_eq!(tx_raw["pending"].as_bool(), Some(false));
    assert_eq!(tx_raw["transactionIndex"].as_str(), Some("0x1"));
    assert_eq!(
        tx_raw["hash"].as_str(),
        Some(format!("0x{}", to_hex(&expected_block_txs[1].tx_hash)).as_str())
    );

    let (count_raw, changed_count) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockTransactionCountByHash",
        &serde_json::json!({
            "chain_id": chain_id,
            "block_hash": format!("0x{}", to_hex(&expected_block_hash)),
        }),
    )
    .expect("eth_getBlockTransactionCountByHash should recover count from store hash index");
    assert!(!changed_count);
    assert_eq!(count_raw.as_str(), Some("0x2"));

    let (receipts_raw, changed_receipts) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBlockReceipts",
        &serde_json::json!({
            "chain_id": chain_id,
            "block_hash": format!("0x{}", to_hex(&expected_block_hash)),
        }),
    )
    .expect("eth_getBlockReceipts should recover block from store hash index");
    assert!(!changed_receipts);
    let receipts = receipts_raw
        .as_array()
        .expect("recovered receipts should be array");
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0]["pending"].as_bool(), Some(false));
    assert_eq!(receipts[1]["pending"].as_bool(), Some(false));

    let (uncle_count_raw, changed_uncle_count) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getUncleCountByBlockHash",
        &serde_json::json!({
            "chain_id": chain_id,
            "block_hash": format!("0x{}", to_hex(&expected_block_hash)),
        }),
    )
    .expect("eth_getUncleCountByBlockHash should recover block existence from store hash index");
    assert!(!changed_uncle_count);
    assert_eq!(uncle_count_raw.as_str(), Some("0x0"));

    let _ = fs::remove_dir_all(&spool_dir);
    let _ = fs::remove_dir_all(&rocksdb_path);
}

#[test]
fn eth_logs_by_hash_queries_recover_from_store_when_scan_window_truncated() {
    let chain_id = 9u64;
    let rocksdb_path = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-logs-hash-query-index-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: rocksdb_path.clone(),
    };
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-logs-hash-query-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    for idx in 0..GATEWAY_ETH_QUERY_SCAN_MAX_DEFAULT {
        let mut tx_hash = [0u8; 32];
        tx_hash[..8].copy_from_slice(&(idx as u64).to_le_bytes());
        eth_tx_index.insert(
            tx_hash,
            GatewayEthTxIndexEntry {
                tx_hash,
                uca_id: format!("uca-fill-{}", idx),
                chain_id,
                nonce: idx as u64,
                tx_type: 0,
                from: vec![0x11u8; 20],
                to: Some(vec![0x22u8; 20]),
                value: 1,
                gas_limit: 21_000,
                gas_price: 3,
                input: vec![0x00],
            },
        );
    }

    let addr_a = vec![0x44u8; 20];
    let addr_b = vec![0x55u8; 20];
    let block_number = 99_999u64;
    let tx_a = GatewayEthTxIndexEntry {
        tx_hash: [0x11u8; 32],
        uca_id: "uca-a".to_string(),
        chain_id,
        nonce: block_number,
        tx_type: 2,
        from: vec![0x33u8; 20],
        to: Some(addr_a.clone()),
        value: 3,
        gas_limit: 25_000,
        gas_price: 9,
        input: vec![0x60, 0x01],
    };
    let tx_b = GatewayEthTxIndexEntry {
        tx_hash: [0xf3u8; 32],
        uca_id: "uca-b".to_string(),
        chain_id,
        nonce: block_number,
        tx_type: 2,
        from: vec![0x33u8; 20],
        to: Some(addr_b.clone()),
        value: 7,
        gas_limit: 52_000,
        gas_price: 11,
        input: vec![0x60, 0x02],
    };
    backend.save_eth_tx(&tx_a).expect("save tx_a");
    backend.save_eth_tx(&tx_b).expect("save tx_b");
    let mut expected_block_txs = vec![tx_a.clone(), tx_b.clone()];
    sort_gateway_eth_block_txs(&mut expected_block_txs);
    let expected_block_hash =
        gateway_eth_block_hash_for_txs(chain_id, block_number, &expected_block_txs);
    let expected_block_hash_hex = format!("0x{}", to_hex(&expected_block_hash));

    let (logs_raw, changed_logs) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getLogs",
        &serde_json::json!({
            "chain_id": chain_id,
            "block_hash": expected_block_hash_hex,
            "address": format!("0x{}", to_hex(&addr_b)),
        }),
    )
    .expect("eth_getLogs should recover logs from store hash index");
    assert!(!changed_logs);
    let logs = logs_raw.as_array().expect("logs should be array");
    assert_eq!(logs.len(), 1);
    assert_eq!(
        logs[0]["transactionHash"].as_str(),
        Some(format!("0x{}", to_hex(&tx_b.tx_hash)).as_str())
    );
    assert_eq!(
        logs[0]["blockHash"].as_str(),
        Some(format!("0x{}", to_hex(&expected_block_hash)).as_str())
    );

    let (filter_id_raw, changed_new_filter) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_newFilter",
        &serde_json::json!([{
            "chain_id": chain_id,
            "block_hash": format!("0x{}", to_hex(&expected_block_hash)),
            "address": format!("0x{}", to_hex(&addr_b)),
        }]),
    )
    .expect("eth_newFilter by hash should work");
    assert!(!changed_new_filter);
    let filter_id = filter_id_raw
        .as_str()
        .expect("filter id should be string")
        .to_string();

    let (filter_logs_raw, changed_filter_logs) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterLogs",
        &serde_json::json!([filter_id.clone()]),
    )
    .expect("eth_getFilterLogs should recover logs from store hash index");
    assert!(!changed_filter_logs);
    let filter_logs = filter_logs_raw
        .as_array()
        .expect("filter logs should be array");
    assert_eq!(filter_logs.len(), 1);
    assert_eq!(
        filter_logs[0]["transactionHash"].as_str(),
        Some(format!("0x{}", to_hex(&tx_b.tx_hash)).as_str())
    );

    let (filter_changes_first_raw, changed_filter_changes_first) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterChanges",
        &serde_json::json!([filter_id.clone()]),
    )
    .expect("eth_getFilterChanges first by-hash poll should work");
    assert!(!changed_filter_changes_first);
    let filter_changes_first = filter_changes_first_raw
        .as_array()
        .expect("filter changes should be array");
    assert_eq!(filter_changes_first.len(), 1);
    assert_eq!(
        filter_changes_first[0]["transactionHash"].as_str(),
        Some(format!("0x{}", to_hex(&tx_b.tx_hash)).as_str())
    );

    let (filter_changes_second_raw, changed_filter_changes_second) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterChanges",
        &serde_json::json!([filter_id]),
    )
    .expect("eth_getFilterChanges second by-hash poll should be empty");
    assert!(!changed_filter_changes_second);
    let filter_changes_second = filter_changes_second_raw
        .as_array()
        .expect("filter changes second should be array");
    assert_eq!(filter_changes_second.len(), 0);

    let _ = fs::remove_dir_all(&spool_dir);
    let _ = fs::remove_dir_all(&rocksdb_path);
}

#[test]
fn eth_logs_by_block_range_queries_recover_from_store_when_scan_window_truncated() {
    let chain_id = 9u64;
    let rocksdb_path = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-logs-range-query-index-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: rocksdb_path.clone(),
    };
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-logs-range-query-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    for idx in 0..GATEWAY_ETH_QUERY_SCAN_MAX_DEFAULT {
        let mut tx_hash = [0u8; 32];
        tx_hash[..8].copy_from_slice(&(idx as u64).to_le_bytes());
        eth_tx_index.insert(
            tx_hash,
            GatewayEthTxIndexEntry {
                tx_hash,
                uca_id: format!("uca-fill-{}", idx),
                chain_id,
                nonce: idx as u64,
                tx_type: 0,
                from: vec![0x11u8; 20],
                to: Some(vec![0x22u8; 20]),
                value: 1,
                gas_limit: 21_000,
                gas_price: 3,
                input: vec![0x00],
            },
        );
    }

    let target_addr = vec![0x66u8; 20];
    let block_number = 99_999u64;
    let block_number_hex = format!("0x{:x}", block_number);
    let tx = GatewayEthTxIndexEntry {
        tx_hash: [0x21u8; 32],
        uca_id: "uca-range".to_string(),
        chain_id,
        nonce: block_number,
        tx_type: 2,
        from: vec![0x33u8; 20],
        to: Some(target_addr.clone()),
        value: 5,
        gas_limit: 30_000,
        gas_price: 7,
        input: vec![0xab, 0xcd],
    };
    backend.save_eth_tx(&tx).expect("save tx");

    let (logs_raw, changed_logs) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getLogs",
        &serde_json::json!({
            "chain_id": chain_id,
            "fromBlock": block_number_hex,
            "toBlock": format!("0x{:x}", block_number),
            "address": format!("0x{}", to_hex(&target_addr)),
        }),
    )
    .expect("eth_getLogs by range should recover logs from store block index");
    assert!(!changed_logs);
    let logs = logs_raw.as_array().expect("logs should be array");
    assert_eq!(logs.len(), 1);
    assert_eq!(
        logs[0]["transactionHash"].as_str(),
        Some(format!("0x{}", to_hex(&tx.tx_hash)).as_str())
    );

    let (filter_id_raw, changed_new_filter) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_newFilter",
        &serde_json::json!([{
            "chain_id": chain_id,
            "fromBlock": format!("0x{:x}", block_number),
            "toBlock": format!("0x{:x}", block_number),
            "address": format!("0x{}", to_hex(&target_addr)),
        }]),
    )
    .expect("eth_newFilter by range should work");
    assert!(!changed_new_filter);
    let filter_id = filter_id_raw
        .as_str()
        .expect("filter id should be string")
        .to_string();

    let (filter_logs_raw, changed_filter_logs) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterLogs",
        &serde_json::json!([filter_id.clone()]),
    )
    .expect("eth_getFilterLogs by range should recover logs from store block index");
    assert!(!changed_filter_logs);
    let filter_logs = filter_logs_raw
        .as_array()
        .expect("filter logs should be array");
    assert_eq!(filter_logs.len(), 1);
    assert_eq!(
        filter_logs[0]["transactionHash"].as_str(),
        Some(format!("0x{}", to_hex(&tx.tx_hash)).as_str())
    );

    let (filter_changes_first_raw, changed_filter_changes_first) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterChanges",
        &serde_json::json!([filter_id.clone()]),
    )
    .expect("eth_getFilterChanges first by-range poll should work");
    assert!(!changed_filter_changes_first);
    let filter_changes_first = filter_changes_first_raw
        .as_array()
        .expect("filter changes should be array");
    assert_eq!(filter_changes_first.len(), 1);
    assert_eq!(
        filter_changes_first[0]["transactionHash"].as_str(),
        Some(format!("0x{}", to_hex(&tx.tx_hash)).as_str())
    );

    let (filter_changes_second_raw, changed_filter_changes_second) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterChanges",
        &serde_json::json!([filter_id]),
    )
    .expect("eth_getFilterChanges second by-range poll should be empty");
    assert!(!changed_filter_changes_second);
    let filter_changes_second = filter_changes_second_raw
        .as_array()
        .expect("filter changes second should be array");
    assert_eq!(filter_changes_second.len(), 0);

    let _ = fs::remove_dir_all(&spool_dir);
    let _ = fs::remove_dir_all(&rocksdb_path);
}

#[test]
fn eth_receipt_contract_address_and_logs_bloom_shape_are_compatible() {
    let contract_entry = GatewayEthTxIndexEntry {
        tx_hash: [0xabu8; 32],
        uca_id: "uca-contract".to_string(),
        chain_id: 1,
        nonce: 42,
        tx_type: 2,
        from: vec![0x11u8; 20],
        to: None,
        value: 0,
        gas_limit: 120_000,
        gas_price: 7,
        input: vec![0x60, 0x00],
    };
    let transfer_entry = GatewayEthTxIndexEntry {
        tx_hash: [0xcdu8; 32],
        uca_id: "uca-transfer".to_string(),
        chain_id: 1,
        nonce: 43,
        tx_type: 0,
        from: vec![0x22u8; 20],
        to: Some(vec![0x33u8; 20]),
        value: 1,
        gas_limit: 21_000,
        gas_price: 1,
        input: Vec::new(),
    };
    let block_hash = [0x55u8; 32];

    let pending_receipt = gateway_eth_tx_receipt_json(&contract_entry);
    let pending_contract_address = pending_receipt["contractAddress"]
        .as_str()
        .expect("pending contract receipt should expose contractAddress");
    let expected_contract_address = format!(
        "0x{}",
        to_hex(&gateway_eth_derive_contract_address(
            &contract_entry.from,
            contract_entry.nonce
        ))
    );
    assert_eq!(pending_contract_address, expected_contract_address.as_str());
    let pending_logs_bloom = pending_receipt["logsBloom"]
        .as_str()
        .expect("pending logsBloom should be string");
    assert_eq!(pending_logs_bloom.len(), 514);
    assert!(pending_logs_bloom.starts_with("0x"));

    let confirmed_receipt =
        gateway_eth_tx_receipt_with_block_json(&contract_entry, 12, 0, &block_hash, 120_000);
    let confirmed_contract_address = confirmed_receipt["contractAddress"]
        .as_str()
        .expect("confirmed contract receipt should expose contractAddress");
    assert_eq!(
        confirmed_contract_address,
        expected_contract_address.as_str()
    );
    let confirmed_logs_bloom = confirmed_receipt["logsBloom"]
        .as_str()
        .expect("confirmed logsBloom should be string");
    assert_eq!(confirmed_logs_bloom.len(), 514);
    assert!(confirmed_logs_bloom.starts_with("0x"));

    let transfer_receipt =
        gateway_eth_tx_receipt_with_block_json(&transfer_entry, 12, 1, &block_hash, 141_000);
    assert!(transfer_receipt["contractAddress"].is_null());
}

#[test]
fn eth_logs_filter_latest_pending_without_runtime_pending_keeps_next_confirmed_block() {
    let _guard = env_test_guard();
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let chain_id = 1u64;
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-logs-filter-latest-pending-no-runtime-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let addr_a = vec![0x11u8; 20];
    let addr_b = vec![0x22u8; 20];
    let tx_hash_1 = [0x91u8; 32];
    eth_tx_index.insert(
        tx_hash_1,
        GatewayEthTxIndexEntry {
            tx_hash: tx_hash_1,
            uca_id: "uca-logs-filter-1".to_string(),
            chain_id,
            nonce: 1,
            tx_type: 0,
            from: addr_a.clone(),
            to: Some(addr_b.clone()),
            value: 1,
            gas_limit: 21_000,
            gas_price: 1,
            input: vec![0x01],
        },
    );

    let (filter_id_raw, changed_new_filter) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_newFilter",
        &serde_json::json!([{
            "chain_id": chain_id,
            "fromBlock": "latest",
            "toBlock": "pending",
        }]),
    )
    .expect("eth_newFilter latest..pending should work without runtime pending");
    assert!(!changed_new_filter);
    let filter_id = filter_id_raw
        .as_str()
        .expect("filter id should be string")
        .to_string();

    let (changes_first_raw, changed_changes_first) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterChanges",
        &serde_json::json!([filter_id.clone()]),
    )
    .expect("eth_getFilterChanges first poll should work");
    assert!(!changed_changes_first);
    let changes_first = changes_first_raw
        .as_array()
        .expect("first changes should be array");
    assert_eq!(changes_first.len(), 1);
    assert_eq!(
        changes_first[0]["transactionHash"].as_str(),
        Some(format!("0x{}", to_hex(&tx_hash_1)).as_str())
    );

    let tx_hash_2 = [0x92u8; 32];
    eth_tx_index.insert(
        tx_hash_2,
        GatewayEthTxIndexEntry {
            tx_hash: tx_hash_2,
            uca_id: "uca-logs-filter-2".to_string(),
            chain_id,
            nonce: 2,
            tx_type: 0,
            from: addr_a,
            to: Some(addr_b),
            value: 2,
            gas_limit: 21_000,
            gas_price: 2,
            input: vec![0x02],
        },
    );

    let (changes_second_raw, changed_changes_second) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getFilterChanges",
        &serde_json::json!([filter_id]),
    )
    .expect("eth_getFilterChanges second poll should include newly confirmed block");
    assert!(!changed_changes_second);
    let changes_second = changes_second_raw
        .as_array()
        .expect("second changes should be array");
    assert_eq!(changes_second.len(), 1);
    assert_eq!(
        changes_second[0]["transactionHash"].as_str(),
        Some(format!("0x{}", to_hex(&tx_hash_2)).as_str())
    );

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_effective_gas_price_type2_respects_base_fee_floor() {
    let type2_entry = GatewayEthTxIndexEntry {
        tx_hash: [0x99u8; 32],
        uca_id: "uca-type2".to_string(),
        chain_id: 1,
        nonce: 7,
        tx_type: 2,
        from: vec![0x11u8; 20],
        to: Some(vec![0x22u8; 20]),
        value: 1,
        gas_limit: 50_000,
        gas_price: 5,
        input: Vec::new(),
    };
    let legacy_entry = GatewayEthTxIndexEntry {
        tx_hash: [0x98u8; 32],
        uca_id: "uca-legacy".to_string(),
        chain_id: 1,
        nonce: 8,
        tx_type: 0,
        from: vec![0x33u8; 20],
        to: Some(vec![0x44u8; 20]),
        value: 1,
        gas_limit: 21_000,
        gas_price: 5,
        input: Vec::new(),
    };

    assert_eq!(gateway_eth_effective_gas_price_wei(&type2_entry, 9), 9);
    assert_eq!(gateway_eth_effective_gas_price_wei(&type2_entry, 3), 5);
    assert_eq!(gateway_eth_effective_gas_price_wei(&legacy_entry, 9), 5);
}

#[test]
fn eth_transaction_json_type2_fee_fields_are_compatible() {
    let type2_entry = GatewayEthTxIndexEntry {
        tx_hash: [0x71u8; 32],
        uca_id: "uca-type2-json".to_string(),
        chain_id: 1,
        nonce: 11,
        tx_type: 2,
        from: vec![0x11u8; 20],
        to: Some(vec![0x22u8; 20]),
        value: 7,
        gas_limit: 50_000,
        gas_price: 9,
        input: vec![0x60, 0x00],
    };
    let legacy_entry = GatewayEthTxIndexEntry {
        tx_hash: [0x72u8; 32],
        uca_id: "uca-legacy-json".to_string(),
        chain_id: 1,
        nonce: 12,
        tx_type: 0,
        from: vec![0x33u8; 20],
        to: Some(vec![0x44u8; 20]),
        value: 8,
        gas_limit: 21_000,
        gas_price: 3,
        input: Vec::new(),
    };

    let expected_priority =
        gateway_eth_default_max_priority_fee_per_gas_wei().min(type2_entry.gas_price);

    let type2_json = gateway_eth_tx_by_hash_json(&type2_entry);
    assert_eq!(type2_json["gasPrice"].as_str(), Some("0x9"));
    assert_eq!(type2_json["maxFeePerGas"].as_str(), Some("0x9"));
    assert_eq!(
        type2_json["maxPriorityFeePerGas"].as_str(),
        Some(format!("0x{:x}", expected_priority).as_str())
    );

    let type2_block_json = gateway_eth_tx_with_block_json(&type2_entry, 11, 0, &[0x55u8; 32]);
    assert_eq!(type2_block_json["maxFeePerGas"].as_str(), Some("0x9"));
    assert_eq!(
        type2_block_json["maxPriorityFeePerGas"].as_str(),
        Some(format!("0x{:x}", expected_priority).as_str())
    );

    let legacy_json = gateway_eth_tx_by_hash_json(&legacy_entry);
    assert!(legacy_json["maxFeePerGas"].is_null());
    assert!(legacy_json["maxPriorityFeePerGas"].is_null());
}

#[test]
fn eth_runtime_pending_tx_json_type2_fee_fields_follow_raw_signature() {
    let mut tx = TxIR::transfer(vec![0x81u8; 20], vec![0x82u8; 20], 1, 1, 1);
    tx.gas_price = 13;
    tx.signature = vec![0x02, 0xc0];
    tx.compute_hash();
    let expected_priority = gateway_eth_default_max_priority_fee_per_gas_wei().min(tx.gas_price);

    let pending_json = gateway_eth_pending_tx_by_hash_json_from_ir(&tx);
    assert_eq!(pending_json["gasPrice"].as_str(), Some("0xd"));
    assert_eq!(pending_json["maxFeePerGas"].as_str(), Some("0xd"));
    assert_eq!(
        pending_json["maxPriorityFeePerGas"].as_str(),
        Some(format!("0x{:x}", expected_priority).as_str())
    );

    let mut legacy = TxIR::transfer(vec![0x91u8; 20], vec![0x92u8; 20], 1, 2, 1);
    legacy.gas_price = 7;
    legacy.signature.clear();
    legacy.compute_hash();
    let legacy_json = gateway_eth_pending_tx_by_hash_json_from_ir(&legacy);
    assert!(legacy_json["maxFeePerGas"].is_null());
    assert!(legacy_json["maxPriorityFeePerGas"].is_null());
}

#[test]
fn eth_get_code_storage_and_call_read_path_use_tx_index_state() {
    let _guard = env_test_guard();
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-state-read-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let deployer = vec![0x11u8; 20];
    let caller = vec![0x22u8; 20];
    let holder = vec![0x33u8; 20];
    let funder = vec![0x44u8; 20];
    let deploy_nonce = 4u64;
    let call_nonce = 5u64;
    let deploy_input = vec![0x60, 0x00, 0x60, 0x00, 0xf3];
    let contract = gateway_eth_derive_contract_address(&deployer, deploy_nonce);
    let deploy_tx_hash = [0xd1u8; 32];
    let call_tx_hash = [0xc1u8; 32];
    let fund_tx_hash = [0xb1u8; 32];
    let deploy_entry = GatewayEthTxIndexEntry {
        tx_hash: deploy_tx_hash,
        uca_id: "uca-deploy".to_string(),
        chain_id: 1,
        nonce: deploy_nonce,
        tx_type: 2,
        from: deployer.clone(),
        to: None,
        value: 0,
        gas_limit: 2_000_000,
        gas_price: 1,
        input: deploy_input.clone(),
    };
    let call_entry = GatewayEthTxIndexEntry {
        tx_hash: call_tx_hash,
        uca_id: "uca-call".to_string(),
        chain_id: 1,
        nonce: call_nonce,
        tx_type: 1,
        from: caller,
        to: Some(contract.clone()),
        value: 0,
        gas_limit: 200_000,
        gas_price: 1,
        input: vec![0xaa, 0xbb],
    };
    let fund_entry = GatewayEthTxIndexEntry {
        tx_hash: fund_tx_hash,
        uca_id: "uca-fund".to_string(),
        chain_id: 1,
        nonce: 6,
        tx_type: 0,
        from: funder,
        to: Some(holder.clone()),
        value: 42,
        gas_limit: 21_000,
        gas_price: 1,
        input: Vec::new(),
    };
    eth_tx_index.insert(deploy_entry.tx_hash, deploy_entry);
    eth_tx_index.insert(call_entry.tx_hash, call_entry);
    eth_tx_index.insert(fund_entry.tx_hash, fund_entry);

    let (code, changed_code) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getCode",
        &serde_json::json!([format!("0x{}", to_hex(&contract)), "latest"]),
    )
    .expect("eth_getCode should work");
    assert!(!changed_code);
    assert_eq!(
        code.as_str(),
        Some(format!("0x{}", to_hex(&deploy_input)).as_str())
    );

    let (code_earliest, changed_code_earliest) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getCode",
        &serde_json::json!([format!("0x{}", to_hex(&contract)), "earliest"]),
    )
    .expect("eth_getCode earliest should work");
    assert!(!changed_code_earliest);
    assert_eq!(code_earliest.as_str(), Some("0x"));

    let (code_block3, changed_code_block3) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getCode",
        &serde_json::json!([format!("0x{}", to_hex(&contract)), "0x3"]),
    )
    .expect("eth_getCode by block number before deploy should work");
    assert!(!changed_code_block3);
    assert_eq!(code_block3.as_str(), Some("0x"));

    let (code_future, changed_code_future) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getCode",
        &serde_json::json!([format!("0x{}", to_hex(&contract)), "0x99"]),
    )
    .expect("eth_getCode future block should work");
    assert!(!changed_code_future);
    assert!(code_future.is_null());

    let deploy_code_hash: [u8; 32] = Sha256::digest(&deploy_input).into();
    let (slot0, changed_slot0) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getStorageAt",
        &serde_json::json!([format!("0x{}", to_hex(&contract)), "0x0", "latest"]),
    )
    .expect("eth_getStorageAt slot0 should work");
    assert!(!changed_slot0);
    assert_eq!(
        slot0.as_str(),
        Some(format!("0x{}", to_hex(&deploy_code_hash)).as_str())
    );

    let (slot_nonce, changed_slot_nonce) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getStorageAt",
        &serde_json::json!([
            format!("0x{}", to_hex(&contract)),
            format!("0x{:x}", call_nonce)
        ]),
    )
    .expect("eth_getStorageAt nonce-slot should work");
    assert!(!changed_slot_nonce);
    assert_eq!(
        slot_nonce.as_str(),
        Some(format!("0x{}", to_hex(&call_tx_hash)).as_str())
    );

    let zero_word_hex = format!("0x{}", "00".repeat(32));
    let (slot_nonce_block4, changed_slot_nonce_block4) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getStorageAt",
        &serde_json::json!([
            format!("0x{}", to_hex(&contract)),
            format!("0x{:x}", call_nonce),
            "0x4"
        ]),
    )
    .expect("eth_getStorageAt historical block should work");
    assert!(!changed_slot_nonce_block4);
    assert_eq!(slot_nonce_block4.as_str(), Some(zero_word_hex.as_str()));

    let (slot0_earliest, changed_slot0_earliest) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getStorageAt",
        &serde_json::json!([format!("0x{}", to_hex(&contract)), "0x0", "earliest"]),
    )
    .expect("eth_getStorageAt earliest should work");
    assert!(!changed_slot0_earliest);
    assert_eq!(slot0_earliest.as_str(), Some(zero_word_hex.as_str()));

    let (slot0_future, changed_slot0_future) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getStorageAt",
        &serde_json::json!([format!("0x{}", to_hex(&contract)), "0x0", "0x99"]),
    )
    .expect("eth_getStorageAt future block should work");
    assert!(!changed_slot0_future);
    assert!(slot0_future.is_null());

    let (proof, changed_proof) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getProof",
        &serde_json::json!([
            format!("0x{}", to_hex(&contract)),
            ["0x0", format!("0x{:x}", call_nonce)],
            "latest"
        ]),
    )
    .expect("eth_getProof should work");
    assert!(!changed_proof);
    assert_eq!(
        proof["address"].as_str(),
        Some(format!("0x{}", to_hex(&contract)).as_str())
    );
    assert_eq!(proof["balance"].as_str(), Some("0x0"));
    assert_eq!(proof["nonce"].as_str(), Some("0x0"));
    let code_hash: [u8; 32] = Keccak256::digest(&deploy_input).into();
    assert_eq!(
        proof["codeHash"].as_str(),
        Some(format!("0x{}", to_hex(&code_hash)).as_str())
    );
    assert_eq!(
        proof["storageHash"].as_str().map(str::len),
        Some(66),
        "storageHash must be 32-byte hex"
    );
    assert_eq!(
        proof["accountProof"].as_array().map(std::vec::Vec::len),
        Some(0)
    );
    let storage_proof = proof["storageProof"]
        .as_array()
        .expect("storageProof should be array");
    assert_eq!(storage_proof.len(), 2);
    assert_eq!(
        storage_proof[0]["key"].as_str(),
        Some("0x0000000000000000000000000000000000000000000000000000000000000000")
    );
    assert_eq!(
        storage_proof[0]["value"].as_str(),
        Some(format!("0x{}", to_hex(&deploy_code_hash)).as_str())
    );
    assert_eq!(
        storage_proof[1]["key"].as_str(),
        Some(format!("0x{:064x}", call_nonce).as_str())
    );
    assert_eq!(
        storage_proof[1]["value"].as_str(),
        Some(format!("0x{}", to_hex(&call_tx_hash)).as_str())
    );
    assert_eq!(
        storage_proof[0]["proof"].as_array().map(std::vec::Vec::len),
        Some(0)
    );

    let (proof_no_slots, changed_proof_no_slots) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getProof",
        &serde_json::json!([format!("0x{}", to_hex(&contract)), [], "latest"]),
    )
    .expect("eth_getProof without storage keys should work");
    assert!(!changed_proof_no_slots);
    assert_eq!(
        proof_no_slots["storageHash"].as_str(),
        Some(GATEWAY_ETH_EMPTY_TRIE_ROOT)
    );
    assert_eq!(
        proof_no_slots["storageProof"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(0)
    );

    let (proof_earliest, changed_proof_earliest) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getProof",
        &serde_json::json!([format!("0x{}", to_hex(&contract)), ["0x0"], "earliest"]),
    )
    .expect("eth_getProof earliest should work");
    assert!(!changed_proof_earliest);
    let empty_code_hash: [u8; 32] = Keccak256::digest([]).into();
    assert_eq!(
        proof_earliest["codeHash"].as_str(),
        Some(format!("0x{}", to_hex(&empty_code_hash)).as_str())
    );
    assert_eq!(proof_earliest["balance"].as_str(), Some("0x0"));
    assert_eq!(proof_earliest["nonce"].as_str(), Some("0x0"));
    let earliest_storage = proof_earliest["storageProof"]
        .as_array()
        .expect("earliest storageProof should be array");
    assert_eq!(earliest_storage.len(), 1);
    assert_eq!(
        earliest_storage[0]["value"].as_str(),
        Some(zero_word_hex.as_str())
    );

    let (proof_block4, changed_proof_block4) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getProof",
        &serde_json::json!([
            format!("0x{}", to_hex(&contract)),
            ["0x0", format!("0x{:x}", call_nonce)],
            "0x4"
        ]),
    )
    .expect("eth_getProof by block number should work");
    assert!(!changed_proof_block4);
    let block4_storage = proof_block4["storageProof"]
        .as_array()
        .expect("block4 storageProof should be array");
    assert_eq!(block4_storage.len(), 2);
    assert_eq!(
        block4_storage[0]["value"].as_str(),
        Some(format!("0x{}", to_hex(&deploy_code_hash)).as_str())
    );
    assert_eq!(
        block4_storage[1]["value"].as_str(),
        Some(zero_word_hex.as_str())
    );

    let (proof_block5, changed_proof_block5) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getProof",
        &serde_json::json!([
            format!("0x{}", to_hex(&contract)),
            [format!("0x{:x}", call_nonce)],
            "0x5"
        ]),
    )
    .expect("eth_getProof by later block number should work");
    assert!(!changed_proof_block5);
    let block5_storage = proof_block5["storageProof"]
        .as_array()
        .expect("block5 storageProof should be array");
    assert_eq!(block5_storage.len(), 1);
    assert_eq!(
        block5_storage[0]["value"].as_str(),
        Some(format!("0x{}", to_hex(&call_tx_hash)).as_str())
    );

    let (proof_safe_object, changed_proof_safe_object) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getProof",
        &serde_json::json!([{
            "address": format!("0x{}", to_hex(&contract)),
            "storageKeys": ["0x0"],
            "blockTag": "safe"
        }]),
    )
    .expect("eth_getProof object-style safe should work");
    assert!(!changed_proof_safe_object);
    assert_eq!(
        proof_safe_object["codeHash"].as_str(),
        Some(format!("0x{}", to_hex(&code_hash)).as_str())
    );
    let safe_storage = proof_safe_object["storageProof"]
        .as_array()
        .expect("safe storageProof should be array");
    assert_eq!(safe_storage.len(), 1);
    assert_eq!(
        safe_storage[0]["value"].as_str(),
        Some(format!("0x{}", to_hex(&deploy_code_hash)).as_str())
    );

    let (proof_pending, changed_proof_pending) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getProof",
        &serde_json::json!([
            format!("0x{}", to_hex(&contract)),
            [format!("0x{:x}", call_nonce)],
            "pending"
        ]),
    )
    .expect("eth_getProof pending should work");
    assert!(!changed_proof_pending);
    let pending_storage = proof_pending["storageProof"]
        .as_array()
        .expect("pending storageProof should be array");
    assert_eq!(pending_storage.len(), 1);
    assert_eq!(
        pending_storage[0]["value"].as_str(),
        Some(format!("0x{}", to_hex(&call_tx_hash)).as_str())
    );

    let (proof_future, changed_proof_future) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getProof",
        &serde_json::json!([format!("0x{}", to_hex(&contract)), ["0x0"], "0x99"]),
    )
    .expect("eth_getProof future block should work");
    assert!(!changed_proof_future);
    assert!(proof_future.is_null());

    let proof_bad_tag_err = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getProof",
        &serde_json::json!([format!("0x{}", to_hex(&contract)), ["0x0"], "bad-tag"]),
    )
    .expect_err("eth_getProof should reject invalid tag");
    assert!(proof_bad_tag_err
        .to_string()
        .contains("invalid block number/tag"));

    let (eth_call_code, changed_eth_call_code) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_call",
        &serde_json::json!([
            {
                "to": format!("0x{}", to_hex(&contract)),
                "data": "0x",
            },
            "latest"
        ]),
    )
    .expect("eth_call empty-data should work");
    assert!(!changed_eth_call_code);
    assert_eq!(
        eth_call_code.as_str(),
        Some(format!("0x{}", to_hex(&deploy_input)).as_str())
    );

    let mut erc20_balance_of = vec![0x70, 0xa0, 0x82, 0x31];
    erc20_balance_of.extend_from_slice(&[0u8; 12]);
    erc20_balance_of.extend_from_slice(&holder);
    let (eth_call_balance, changed_eth_call_balance) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_call",
        &serde_json::json!([
            {
                "to": format!("0x{}", to_hex(&contract)),
                "data": format!("0x{}", to_hex(&erc20_balance_of)),
            },
            "latest"
        ]),
    )
    .expect("eth_call balanceOf should work");
    assert!(!changed_eth_call_balance);
    assert_eq!(
        eth_call_balance.as_str(),
        Some("0x000000000000000000000000000000000000000000000000000000000000002a")
    );

    let (eth_call_balance_block5, changed_eth_call_balance_block5) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_call",
        &serde_json::json!([
            {
                "to": format!("0x{}", to_hex(&contract)),
                "data": format!("0x{}", to_hex(&erc20_balance_of)),
            },
            "0x5"
        ]),
    )
    .expect("eth_call balanceOf by historical block should work");
    assert!(!changed_eth_call_balance_block5);
    assert_eq!(
        eth_call_balance_block5.as_str(),
        Some("0x0000000000000000000000000000000000000000000000000000000000000000")
    );

    let (eth_call_code_earliest, changed_eth_call_code_earliest) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_call",
        &serde_json::json!([
            {
                "to": format!("0x{}", to_hex(&contract)),
                "data": "0x",
            },
            "earliest"
        ]),
    )
    .expect("eth_call empty-data earliest should work");
    assert!(!changed_eth_call_code_earliest);
    assert_eq!(eth_call_code_earliest.as_str(), Some("0x"));

    let (eth_call_future, changed_eth_call_future) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_call",
        &serde_json::json!([
            {
                "to": format!("0x{}", to_hex(&contract)),
                "data": "0x",
            },
            "0x99"
        ]),
    )
    .expect("eth_call future block should work");
    assert!(!changed_eth_call_future);
    assert!(eth_call_future.is_null());

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_state_read_returns_null_when_historical_block_outside_scan_window() {
    let _guard = env_test_guard();
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-state-window-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let addr_a = vec![0xa1u8; 20];
    let addr_b = vec![0xb2u8; 20];
    eth_tx_index.insert(
        [0x55u8; 32],
        GatewayEthTxIndexEntry {
            tx_hash: [0x55u8; 32],
            uca_id: "uca-window".to_string(),
            chain_id: 1,
            nonce: 5,
            tx_type: 0,
            from: addr_a,
            to: Some(addr_b.clone()),
            value: 42,
            gas_limit: 21_000,
            gas_price: 1,
            input: Vec::new(),
        },
    );

    let prev_scan_max = std::env::var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX").ok();
    std::env::set_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX", "1");

    let (balance_hist_outside, changed_balance_hist_outside) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getBalance",
        &serde_json::json!({
            "chain_id": 1u64,
            "address": format!("0x{}", to_hex(&addr_b)),
            "tag": "0x4",
        }),
    )
    .expect("eth_getBalance historical block outside window should work");
    assert!(!changed_balance_hist_outside);
    assert!(balance_hist_outside.is_null());

    let (proof_hist_outside, changed_proof_hist_outside) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getProof",
        &serde_json::json!({
            "chain_id": 1u64,
            "address": format!("0x{}", to_hex(&addr_b)),
            "storage_keys": ["0x0"],
            "tag": "0x4",
        }),
    )
    .expect("eth_getProof historical block outside window should work");
    assert!(!changed_proof_hist_outside);
    assert!(proof_hist_outside.is_null());

    if let Some(value) = prev_scan_max {
        std::env::set_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX", value);
    } else {
        std::env::remove_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX");
    }
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_estimate_gas_deploy_includes_access_list_intrinsic_cost() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-estimate-gas-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let deploy_data = vec![0x60, 0x00, 0x60, 0x00];
    let from = vec![0x11u8; 20];
    let access_addr = vec![0x22u8; 20];
    let params = serde_json::json!([
        {
            "chain_id": 1u64,
            "from": format!("0x{}", to_hex(&from)),
            "data": format!("0x{}", to_hex(&deploy_data)),
            "accessList": [
                {
                    "address": format!("0x{}", to_hex(&access_addr)),
                    "storageKeys": [
                        format!("0x{}", "01".repeat(32)),
                        format!("0x{}", "02".repeat(32)),
                    ]
                }
            ]
        }
    ]);
    let (estimated_raw, changed_estimated) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_estimateGas",
        &params,
    )
    .expect("eth_estimateGas should include deploy + accessList intrinsic gas");
    assert!(!changed_estimated);
    let mut tx_ir = TxIR {
        hash: Vec::new(),
        from,
        to: None,
        value: 0,
        gas_limit: u64::MAX,
        gas_price: 0,
        nonce: 0,
        data: deploy_data,
        signature: Vec::new(),
        chain_id: 1,
        tx_type: TxType::ContractDeploy,
        source_chain: None,
        target_chain: None,
    };
    tx_ir.compute_hash();
    let expected = estimate_intrinsic_gas_m0(&tx_ir)
        .saturating_add(estimate_access_list_intrinsic_extra_gas_m0(1, 2));
    assert_eq!(
        estimated_raw.as_str(),
        Some(format!("0x{:x}", expected).as_str())
    );

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_get_transaction_count_supports_latest_and_pending_without_forced_binding() {
    let _guard = env_test_guard();
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-tx-count-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let addr_latest = vec![0xabu8; 20];
    let addr_pending = vec![0xcdu8; 20];
    let receiver = vec![0xeeu8; 20];
    eth_tx_index.insert(
        [0x71u8; 32],
        GatewayEthTxIndexEntry {
            tx_hash: [0x71u8; 32],
            uca_id: "uca-latest".to_string(),
            chain_id: 1,
            nonce: 3,
            tx_type: 0,
            from: addr_latest.clone(),
            to: Some(receiver.clone()),
            value: 1,
            gas_limit: 21_000,
            gas_price: 1,
            input: Vec::new(),
        },
    );
    eth_tx_index.insert(
        [0x72u8; 32],
        GatewayEthTxIndexEntry {
            tx_hash: [0x72u8; 32],
            uca_id: "uca-latest".to_string(),
            chain_id: 1,
            nonce: 1,
            tx_type: 0,
            from: addr_latest.clone(),
            to: Some(receiver.clone()),
            value: 1,
            gas_limit: 21_000,
            gas_price: 1,
            input: Vec::new(),
        },
    );

    // Build a pending nonce in UA router for addr_pending (without any tx index history).
    let now = now_unix_sec();
    let uca_pending = "uca:pending-only".to_string();
    let persona_pending = PersonaAddress {
        persona_type: PersonaType::Evm,
        chain_id: 1,
        external_address: addr_pending.clone(),
    };
    router
        .create_uca(uca_pending.clone(), vec![1u8; 32], now)
        .expect("create uca");
    router
        .add_binding(
            &uca_pending,
            AccountRole::Owner,
            persona_pending.clone(),
            now,
        )
        .expect("add binding");
    router
        .route(RouteRequest {
            uca_id: uca_pending.clone(),
            persona: persona_pending,
            role: AccountRole::Owner,
            protocol: ProtocolKind::Eth,
            signature_domain: "evm:1".to_string(),
            nonce: 0,
            wants_cross_chain_atomic: false,
            tx_type4: false,
            session_expires_at: None,
            now: now.saturating_add(1),
        })
        .expect("route nonce 0");

    let (latest_count, changed_latest) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionCount",
        &serde_json::json!([format!("0x{}", to_hex(&addr_latest)), "latest"]),
    )
    .expect("eth_getTransactionCount latest should work");
    assert!(!changed_latest);
    assert_eq!(latest_count.as_str(), Some("0x4"));

    let (pending_count_latest_addr, changed_pending_latest_addr) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionCount",
        &serde_json::json!([format!("0x{}", to_hex(&addr_latest)), "pending"]),
    )
    .expect("eth_getTransactionCount pending(latest addr) should work");
    assert!(!changed_pending_latest_addr);
    assert_eq!(pending_count_latest_addr.as_str(), Some("0x4"));

    let (pending_count_router_addr, changed_pending_router_addr) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionCount",
        &serde_json::json!([format!("0x{}", to_hex(&addr_pending)), "pending"]),
    )
    .expect("eth_getTransactionCount pending(router addr) should work");
    assert!(!changed_pending_router_addr);
    assert_eq!(pending_count_router_addr.as_str(), Some("0x1"));

    let (earliest_count, changed_earliest) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionCount",
        &serde_json::json!([format!("0x{}", to_hex(&addr_latest)), "earliest"]),
    )
    .expect("eth_getTransactionCount earliest should work");
    assert!(!changed_earliest);
    assert_eq!(earliest_count.as_str(), Some("0x0"));

    let (historical_count_block1, changed_historical_block1) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionCount",
        &serde_json::json!([format!("0x{}", to_hex(&addr_latest)), "0x1"]),
    )
    .expect("eth_getTransactionCount block 0x1 should work");
    assert!(!changed_historical_block1);
    assert_eq!(historical_count_block1.as_str(), Some("0x2"));

    let (historical_count_block2, changed_historical_block2) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionCount",
        &serde_json::json!([format!("0x{}", to_hex(&addr_latest)), "0x2"]),
    )
    .expect("eth_getTransactionCount block 0x2 should work");
    assert!(!changed_historical_block2);
    assert_eq!(historical_count_block2.as_str(), Some("0x2"));

    let (future_count, changed_future_count) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionCount",
        &serde_json::json!([format!("0x{}", to_hex(&addr_latest)), "0x99"]),
    )
    .expect("eth_getTransactionCount future block should work");
    assert!(!changed_future_count);
    assert!(future_count.is_null());

    let prev_scan_max = std::env::var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX").ok();
    std::env::set_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX", "1");
    let (historical_count_outside_window, changed_historical_outside_window) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionCount",
        &serde_json::json!([format!("0x{}", to_hex(&addr_latest)), "0x0"]),
    )
    .expect("eth_getTransactionCount historical outside scan window should work");
    assert!(!changed_historical_outside_window);
    assert!(historical_count_outside_window.is_null());
    if let Some(value) = prev_scan_max {
        std::env::set_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX", value);
    } else {
        std::env::remove_var("NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX");
    }

    let err = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionCount",
        &serde_json::json!([format!("0x{}", to_hex(&addr_latest)), "bad-tag"]),
    )
    .expect_err("eth_getTransactionCount should reject invalid tag");
    assert!(err.to_string().contains("invalid block number/tag"));

    let err = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_getTransactionCount",
        &serde_json::json!({
            "address": format!("0x{}", to_hex(&addr_pending)),
            "uca_id": "uca:mismatch",
            "tag": "pending"
        }),
    )
    .expect_err("eth_getTransactionCount should reject mismatched uca_id");
    assert!(err.to_string().contains("uca_id mismatch"));

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_send_transaction_without_nonce_uses_pending_view_nonce() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-send-auto-nonce-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let chain_id = 770_001u64;
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let sender = vec![0x51u8; 20];
    let receiver = vec![0x61u8; 20];
    let uca_id = "uca:auto-nonce".to_string();
    let now = now_unix_sec();
    let persona = PersonaAddress {
        persona_type: PersonaType::Evm,
        chain_id,
        external_address: sender.clone(),
    };
    router
        .create_uca(uca_id.clone(), vec![0x11u8; 32], now)
        .expect("create uca");
    router
        .add_binding(&uca_id, AccountRole::Owner, persona.clone(), now)
        .expect("add binding");
    for nonce in 0..5u64 {
        router
            .route(RouteRequest {
                uca_id: uca_id.clone(),
                persona: persona.clone(),
                role: AccountRole::Owner,
                protocol: ProtocolKind::Eth,
                signature_domain: format!("evm:{chain_id}"),
                nonce,
                wants_cross_chain_atomic: false,
                tx_type4: false,
                session_expires_at: None,
                now: now.saturating_add(nonce).saturating_add(1),
            })
            .expect("prime router nonce");
    }
    eth_tx_index.insert(
        [0x31u8; 32],
        GatewayEthTxIndexEntry {
            tx_hash: [0x31u8; 32],
            uca_id: uca_id.clone(),
            chain_id,
            nonce: 3,
            tx_type: 0,
            from: sender.clone(),
            to: Some(receiver.clone()),
            value: 1,
            gas_limit: 21_000,
            gas_price: 1,
            input: Vec::new(),
        },
    );

    let (tx_hash_json, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_sendTransaction",
        &serde_json::json!([{
            "from": format!("0x{}", to_hex(&sender)),
            "to": format!("0x{}", to_hex(&receiver)),
            "value": "0x1",
            "gas": "0x5208",
            "gasPrice": "0x1"
        }]),
    )
    .expect("eth_sendTransaction without nonce should work");
    assert!(changed);
    let tx_hash_hex = tx_hash_json
        .as_str()
        .expect("eth_sendTransaction result should be tx hash string");
    let tx_hash_bytes = decode_hex_bytes(tx_hash_hex, "tx_hash").expect("decode tx hash");
    let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash").expect("tx hash bytes length");
    let indexed = eth_tx_index
        .get(&tx_hash)
        .expect("new tx should be indexed by hash");
    assert_eq!(indexed.chain_id, chain_id);
    assert_eq!(indexed.from, sender);
    assert_eq!(indexed.nonce, 5);

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_send_transaction_infers_type2_from_eip1559_fee_fields() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let mut eth_filters = GatewayEthFilterState::default();
    let chain_id = 770_011u64;
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-send-tx-eip1559-infer-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let sender = vec![0x71u8; 20];
    let receiver = vec![0x72u8; 20];
    let uca_id = "uca:eip1559-infer".to_string();
    let now = now_unix_sec();
    let persona = PersonaAddress {
        persona_type: PersonaType::Evm,
        chain_id,
        external_address: sender.clone(),
    };
    router
        .create_uca(uca_id.clone(), vec![0x33u8; 32], now)
        .expect("create uca");
    router
        .add_binding(&uca_id, AccountRole::Owner, persona, now)
        .expect("add binding");

    let (tx_hash_json, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_sendTransaction",
        &serde_json::json!([{
            "from": format!("0x{}", to_hex(&sender)),
            "to": format!("0x{}", to_hex(&receiver)),
            "nonce": "0x0",
            "value": "0x1",
            "gas": "0x5208",
            "maxFeePerGas": "0x5",
            "maxPriorityFeePerGas": "0x2"
        }]),
    )
    .expect("eth_sendTransaction eip1559 infer should work");
    assert!(changed);
    let tx_hash_hex = tx_hash_json
        .as_str()
        .expect("eth_sendTransaction result should be tx hash string");
    let tx_hash_bytes = decode_hex_bytes(tx_hash_hex, "tx_hash").expect("decode tx hash");
    let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash").expect("tx hash bytes length");
    let indexed = eth_tx_index
        .get(&tx_hash)
        .expect("new tx should be indexed by hash");
    assert_eq!(indexed.chain_id, chain_id);
    assert_eq!(indexed.from, sender);
    assert_eq!(indexed.nonce, 0);
    assert_eq!(indexed.tx_type, 2);
    assert_eq!(indexed.gas_price, 5);

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_send_transaction_accepts_camel_case_signature_domain_alias() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let mut eth_filters = GatewayEthFilterState::default();
    let chain_id = 770_111u64;
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-send-tx-signature-domain-camel-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let sender = vec![0x81u8; 20];
    let receiver = vec![0x82u8; 20];
    let uca_id = "uca:signature-domain-camel".to_string();
    let now = now_unix_sec();
    let persona = PersonaAddress {
        persona_type: PersonaType::Evm,
        chain_id,
        external_address: sender.clone(),
    };
    router
        .create_uca(uca_id.clone(), vec![0x55u8; 32], now)
        .expect("create uca");
    router
        .add_binding(&uca_id, AccountRole::Owner, persona, now)
        .expect("add binding");

    let signature_domain = format!("evm-personal:{chain_id}");
    let (tx_hash_json, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_sendTransaction",
        &serde_json::json!([{
            "from": format!("0x{}", to_hex(&sender)),
            "to": format!("0x{}", to_hex(&receiver)),
            "nonce": "0x0",
            "value": "0x1",
            "gas": "0x5208",
            "gasPrice": "0x2",
            "signatureDomain": signature_domain,
            "sessionExpiresAt": now.saturating_add(60),
            "wantsCrossChainAtomic": false
        }]),
    )
    .expect("eth_sendTransaction with signatureDomain alias should work");
    assert!(changed);
    let tx_hash_hex = tx_hash_json
        .as_str()
        .expect("eth_sendTransaction result should be tx hash string");
    let tx_hash_bytes = decode_hex_bytes(tx_hash_hex, "tx_hash").expect("decode tx hash");
    let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash").expect("tx hash bytes length");

    let expected_hash = compute_gateway_eth_tx_hash(&GatewayEthTxHashInput {
        uca_id: &uca_id,
        chain_id,
        nonce: 0,
        tx_type: 0,
        tx_type4: false,
        from: &sender,
        to: Some(receiver.as_slice()),
        value: 1,
        gas_limit: 21_000,
        gas_price: 2,
        data: &[],
        signature: &[],
        access_list_address_count: 0,
        access_list_storage_key_count: 0,
        signature_domain: signature_domain.as_str(),
        wants_cross_chain_atomic: false,
    });
    assert_eq!(tx_hash, expected_hash);

    let indexed = eth_tx_index
        .get(&tx_hash)
        .expect("new tx should be indexed by hash");
    assert_eq!(indexed.chain_id, chain_id);
    assert_eq!(indexed.nonce, 0);

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_send_transaction_infers_type1_from_access_list() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let mut eth_filters = GatewayEthFilterState::default();
    let chain_id = 770_012u64;
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-send-tx-access-list-infer-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let sender = vec![0x73u8; 20];
    let receiver = vec![0x74u8; 20];
    let access_addr = vec![0x75u8; 20];
    let storage_key = format!("0x{}", "11".repeat(32));
    let uca_id = "uca:access-list-infer".to_string();
    let now = now_unix_sec();
    let persona = PersonaAddress {
        persona_type: PersonaType::Evm,
        chain_id,
        external_address: sender.clone(),
    };
    router
        .create_uca(uca_id.clone(), vec![0x33u8; 32], now)
        .expect("create uca");
    router
        .add_binding(&uca_id, AccountRole::Owner, persona, now)
        .expect("add binding");

    let (tx_hash_json, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_sendTransaction",
        &serde_json::json!([{
            "from": format!("0x{}", to_hex(&sender)),
            "to": format!("0x{}", to_hex(&receiver)),
            "nonce": "0x0",
            "value": "0x1",
            "gas": "0x62d4",
            "gasPrice": "0x1",
            "accessList": [{
                "address": format!("0x{}", to_hex(&access_addr)),
                "storageKeys": [storage_key]
            }]
        }]),
    )
    .expect("eth_sendTransaction with accessList should infer type1");
    assert!(changed);
    let tx_hash_hex = tx_hash_json
        .as_str()
        .expect("eth_sendTransaction result should be tx hash string");
    let tx_hash_bytes = decode_hex_bytes(tx_hash_hex, "tx_hash").expect("decode tx hash");
    let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash").expect("tx hash bytes length");
    let indexed = eth_tx_index
        .get(&tx_hash)
        .expect("new tx should be indexed by hash");
    assert_eq!(indexed.chain_id, chain_id);
    assert_eq!(indexed.from, sender);
    assert_eq!(indexed.nonce, 0);
    assert_eq!(indexed.tx_type, 1);
    assert_eq!(indexed.gas_limit, 25_300);

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_send_transaction_rejects_low_gas_when_access_list_intrinsic_not_covered() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let mut eth_filters = GatewayEthFilterState::default();
    let chain_id = 770_013u64;
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-send-tx-access-list-low-gas-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let sender = vec![0x76u8; 20];
    let receiver = vec![0x77u8; 20];
    let access_addr = vec![0x78u8; 20];
    let storage_key = format!("0x{}", "22".repeat(32));
    let uca_id = "uca:access-list-low-gas".to_string();
    let now = now_unix_sec();
    let persona = PersonaAddress {
        persona_type: PersonaType::Evm,
        chain_id,
        external_address: sender.clone(),
    };
    router
        .create_uca(uca_id.clone(), vec![0x44u8; 32], now)
        .expect("create uca");
    router
        .add_binding(&uca_id, AccountRole::Owner, persona, now)
        .expect("add binding");

    let err = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_sendTransaction",
        &serde_json::json!([{
            "from": format!("0x{}", to_hex(&sender)),
            "to": format!("0x{}", to_hex(&receiver)),
            "nonce": "0x0",
            "value": "0x1",
            "gas": "0x5208",
            "gasPrice": "0x1",
            "accessList": [{
                "address": format!("0x{}", to_hex(&access_addr)),
                "storageKeys": [storage_key]
            }]
        }]),
    )
    .expect_err("eth_sendTransaction should reject low gas for accessList intrinsic");
    assert!(err.to_string().contains("gas too low for intrinsic cost"));

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_send_raw_transaction_without_uca_id_uses_binding_owner() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let chain_id = 1u64;
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-send-raw-auto-uca-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let sender = vec![0x71u8; 20];
    let receiver = vec![0x72u8; 20];
    let uca_id = "uca:raw-auto-owner".to_string();
    let now = now_unix_sec();
    let persona = PersonaAddress {
        persona_type: PersonaType::Evm,
        chain_id,
        external_address: sender.clone(),
    };
    router
        .create_uca(uca_id.clone(), vec![0x11u8; 32], now)
        .expect("create uca");
    router
        .add_binding(&uca_id, AccountRole::Owner, persona, now)
        .expect("add binding");

    // Minimal legacy raw tx: [nonce, gasPrice, gasLimit, to, value, data, v, r, s]
    // v=37 encodes chain_id=1 (EIP-155), nonce=0.
    let raw_tx = test_rlp_encode_list(&[
        test_rlp_encode_u64(0),
        test_rlp_encode_u64(1),
        test_rlp_encode_u64(21_000),
        test_rlp_encode_bytes(&receiver),
        test_rlp_encode_u128(1),
        test_rlp_encode_bytes(&[]),
        test_rlp_encode_u64(37),
        test_rlp_encode_u64(1),
        test_rlp_encode_u64(1),
    ]);
    let raw_tx_hex = format!("0x{}", to_hex(&raw_tx));

    let (tx_hash_json, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_sendRawTransaction",
        &serde_json::json!({
            "from": format!("0x{}", to_hex(&sender)),
            "raw_tx": raw_tx_hex
        }),
    )
    .expect("eth_sendRawTransaction without uca_id should work when from is bound");
    assert!(changed);
    let tx_hash_hex = tx_hash_json
        .as_str()
        .expect("eth_sendRawTransaction result should be tx hash string");
    let tx_hash_bytes = decode_hex_bytes(tx_hash_hex, "tx_hash").expect("decode tx hash");
    let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash").expect("tx hash bytes length");
    let indexed = eth_tx_index
        .get(&tx_hash)
        .expect("raw tx should be indexed by hash");
    assert_eq!(indexed.uca_id, uca_id);
    assert_eq!(indexed.chain_id, chain_id);
    assert_eq!(indexed.from, sender);
    assert_eq!(indexed.nonce, 0);

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_send_raw_transaction_rejects_explicit_uca_id_mismatch_with_binding_owner() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let chain_id = 1u64;
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-send-raw-mismatch-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let sender = vec![0x43u8; 20];
    let receiver = vec![0x44u8; 20];
    let owner_uca = "uca:raw-owner".to_string();
    let now = now_unix_sec();
    let persona = PersonaAddress {
        persona_type: PersonaType::Evm,
        chain_id,
        external_address: sender.clone(),
    };
    router
        .create_uca(owner_uca.clone(), vec![0x22u8; 32], now)
        .expect("create owner uca");
    router
        .add_binding(&owner_uca, AccountRole::Owner, persona, now)
        .expect("add binding");

    let raw_tx = test_rlp_encode_list(&[
        test_rlp_encode_u64(0),
        test_rlp_encode_u64(1),
        test_rlp_encode_u64(21_000),
        test_rlp_encode_bytes(&receiver),
        test_rlp_encode_u128(1),
        test_rlp_encode_bytes(&[]),
        test_rlp_encode_u64(37),
        test_rlp_encode_u64(1),
        test_rlp_encode_u64(1),
    ]);
    let raw_tx_hex = format!("0x{}", to_hex(&raw_tx));

    let err = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_sendRawTransaction",
        &serde_json::json!({
            "uca_id": "uca:not-owner",
            "from": format!("0x{}", to_hex(&sender)),
            "raw_tx": raw_tx_hex
        }),
    )
    .expect_err("eth_sendRawTransaction should reject mismatched uca_id");
    assert!(err.to_string().contains("uca_id mismatch"));

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn eth_send_raw_transaction_rejects_chain_id_mismatch_for_chain_id_alias() {
    let backend = GatewayEthTxIndexStoreBackend::Memory;
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let chain_id = 1u64;
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-eth-send-raw-chain-id-alias-mismatch-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: chain_id,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let sender = vec![0x45u8; 20];
    let receiver = vec![0x46u8; 20];
    let owner_uca = "uca:raw-chain-id-alias-owner".to_string();
    let now = now_unix_sec();
    let persona = PersonaAddress {
        persona_type: PersonaType::Evm,
        chain_id,
        external_address: sender.clone(),
    };
    router
        .create_uca(owner_uca.clone(), vec![0x22u8; 32], now)
        .expect("create owner uca");
    router
        .add_binding(&owner_uca, AccountRole::Owner, persona, now)
        .expect("add binding");

    // v=37 => raw tx inferred chain_id=1
    let raw_tx = test_rlp_encode_list(&[
        test_rlp_encode_u64(0),
        test_rlp_encode_u64(1),
        test_rlp_encode_u64(21_000),
        test_rlp_encode_bytes(&receiver),
        test_rlp_encode_u128(1),
        test_rlp_encode_bytes(&[]),
        test_rlp_encode_u64(37),
        test_rlp_encode_u64(1),
        test_rlp_encode_u64(1),
    ]);
    let raw_tx_hex = format!("0x{}", to_hex(&raw_tx));

    let err = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "eth_sendRawTransaction",
        &serde_json::json!({
            "from": format!("0x{}", to_hex(&sender)),
            "raw_tx": raw_tx_hex,
            "chainId": 2
        }),
    )
    .expect_err("eth_sendRawTransaction should reject chainId alias mismatch");
    assert!(err.to_string().contains("chain_id mismatch"));

    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn evm_replay_atomic_ready_clears_pending_and_updates_status() {
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-replay-atomic-ready-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: spool_dir.join("eth-tx-index.rocksdb"),
    };
    let intent_id = "intent-replay-0001";
    let mut leg = TxIR::transfer(vec![0x11; 20], vec![0x22; 20], 1, 1, 1);
    leg.compute_hash();
    let ready_item = AtomicBroadcastReadyV1 {
        intent: novovm_adapter_api::AtomicCrossChainIntentV1 {
            intent_id: intent_id.to_string(),
            source_chain: novovm_adapter_api::ChainType::EVM,
            destination_chain: novovm_adapter_api::ChainType::NovoVM,
            ttl_unix_ms: 1_900_000_000_111,
            legs: vec![leg],
        },
        ready_at_unix_ms: 1_900_000_000_000,
    };
    backend
        .save_pending_atomic_ready(&ready_item)
        .expect("save pending atomic-ready");
    upsert_gateway_evm_atomic_ready_index(
        &backend,
        &ready_item,
        EVM_ATOMIC_READY_STATUS_COMPENSATE_PENDING_V1,
        None,
        None,
    );
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };
    let (replayed, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "evm_replayAtomicReady",
        &serde_json::json!({
            "intent_id": intent_id,
        }),
    )
    .expect("replay atomic-ready");
    assert!(!changed);
    assert_eq!(replayed["replayed"].as_bool(), Some(true));
    assert_eq!(replayed["intent_id"].as_str(), Some(intent_id));
    let pending = backend
        .load_pending_atomic_ready(intent_id)
        .expect("load pending atomic-ready after replay");
    assert!(pending.is_none());
    let indexed = backend
        .load_evm_atomic_ready_by_intent(intent_id)
        .expect("load atomic-ready index after replay")
        .expect("atomic-ready index should exist");
    assert_eq!(indexed.status, EVM_ATOMIC_READY_STATUS_COMPENSATED_V1);
    let (queried, changed_query) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "evm_getAtomicReadyByIntentId",
        &serde_json::json!({
            "intent_id": intent_id,
        }),
    )
    .expect("query atomic-ready by intent");
    assert!(!changed_query);
    assert_eq!(queried["intent_id"].as_str(), Some(intent_id));
    assert_eq!(
        queried["status"].as_str(),
        Some(EVM_ATOMIC_READY_STATUS_COMPENSATED_V1)
    );
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn evm_queue_and_mark_atomic_broadcast_updates_status() {
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-queue-atomic-broadcast-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: spool_dir.join("eth-tx-index.rocksdb"),
    };
    let intent_id = "intent-broadcast-0001";
    let tx_hash = [0xabu8; 32];
    backend
        .save_evm_atomic_ready(&GatewayEvmAtomicReadyIndexEntry {
            intent_id: intent_id.to_string(),
            chain_id: 1,
            tx_hash,
            ready_at_unix_ms: 1_900_000_000_123,
            status: EVM_ATOMIC_READY_STATUS_SPOOLED_V1.to_string(),
        })
        .expect("save atomic-ready index");
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };
    let (queued, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "evm_queueAtomicBroadcast",
        &serde_json::json!({
            "intent_id": intent_id,
        }),
    )
    .expect("queue atomic broadcast");
    assert!(!changed);
    assert_eq!(queued["queued"].as_bool(), Some(true));
    assert_eq!(queued["intent_id"].as_str(), Some(intent_id));
    let spool_file = PathBuf::from(
        queued["spool_file"]
            .as_str()
            .expect("queue should return spool_file"),
    );
    let wire = fs::read(&spool_file).expect("read broadcast queue spool");
    let value = decode_single_ops_wire_value(&wire).expect("decode queue ops wire value");
    let ticket: GatewayEvmAtomicBroadcastTicketV1 =
        bincode::deserialize(&value).expect("decode atomic-broadcast ticket");
    assert_eq!(ticket.intent_id, intent_id);
    assert_eq!(ticket.chain_id, 1);
    assert_eq!(ticket.tx_hash, tx_hash);
    let pending_after_queue = backend
        .load_pending_atomic_broadcast_ticket(intent_id)
        .expect("load pending atomic-broadcast ticket after queue");
    assert!(pending_after_queue.is_some());

    let (before_mark, changed_before_mark) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "evm_getAtomicReadyByIntentId",
        &serde_json::json!({
            "intent_id": intent_id,
        }),
    )
    .expect("query atomic-ready before mark");
    assert!(!changed_before_mark);
    assert_eq!(
        before_mark["status"].as_str(),
        Some(EVM_ATOMIC_READY_STATUS_BROADCAST_QUEUED_V1)
    );

    let (marked, changed_marked) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "evm_markAtomicBroadcasted",
        &serde_json::json!({
            "intent_id": intent_id,
        }),
    )
    .expect("mark atomic broadcasted");
    assert!(!changed_marked);
    assert_eq!(marked["broadcasted"].as_bool(), Some(true));
    let pending_after_mark = backend
        .load_pending_atomic_broadcast_ticket(intent_id)
        .expect("load pending atomic-broadcast ticket after mark");
    assert!(pending_after_mark.is_none());

    let (after_mark, changed_after_mark) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "evm_getAtomicReadyByIntentId",
        &serde_json::json!({
            "intent_id": intent_id,
        }),
    )
    .expect("query atomic-ready after mark");
    assert!(!changed_after_mark);
    assert_eq!(
        after_mark["status"].as_str(),
        Some(EVM_ATOMIC_READY_STATUS_BROADCASTED_V1)
    );
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn evm_mark_failed_and_replay_atomic_broadcast_queue_updates_status() {
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-replay-atomic-broadcast-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: spool_dir.join("eth-tx-index.rocksdb"),
    };
    let intent_id = "intent-broadcast-replay-0001";
    let tx_hash = [0xceu8; 32];
    backend
        .save_evm_atomic_ready(&GatewayEvmAtomicReadyIndexEntry {
            intent_id: intent_id.to_string(),
            chain_id: 1,
            tx_hash,
            ready_at_unix_ms: 1_900_000_000_456,
            status: EVM_ATOMIC_READY_STATUS_BROADCAST_QUEUED_V1.to_string(),
        })
        .expect("save atomic-ready index");
    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };

    let (failed, changed_failed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "evm_markAtomicBroadcastFailed",
        &serde_json::json!({
            "intent_id": intent_id,
        }),
    )
    .expect("mark atomic broadcast failed");
    assert!(!changed_failed);
    assert_eq!(failed["failed"].as_bool(), Some(true));

    let pending = backend
        .load_pending_atomic_broadcast_ticket(intent_id)
        .expect("load pending atomic-broadcast ticket after fail");
    assert!(pending.is_some());

    let (after_fail, changed_after_fail) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "evm_getAtomicReadyByIntentId",
        &serde_json::json!({
            "intent_id": intent_id,
        }),
    )
    .expect("query atomic-ready after fail");
    assert!(!changed_after_fail);
    assert_eq!(
        after_fail["status"].as_str(),
        Some(EVM_ATOMIC_READY_STATUS_BROADCAST_FAILED_V1)
    );

    let (replayed, changed_replayed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "evm_replayAtomicBroadcastQueue",
        &serde_json::json!({
            "intent_id": intent_id,
        }),
    )
    .expect("replay atomic-broadcast queue");
    assert!(!changed_replayed);
    assert_eq!(replayed["replayed"].as_bool(), Some(true));
    let spool_file = PathBuf::from(
        replayed["spool_file"]
            .as_str()
            .expect("replay should return spool_file"),
    );
    let wire = fs::read(&spool_file).expect("read replay broadcast queue spool");
    let value = decode_single_ops_wire_value(&wire).expect("decode replay queue ops wire value");
    let ticket: GatewayEvmAtomicBroadcastTicketV1 =
        bincode::deserialize(&value).expect("decode replay atomic-broadcast ticket");
    assert_eq!(ticket.intent_id, intent_id);
    assert_eq!(ticket.chain_id, 1);
    assert_eq!(ticket.tx_hash, tx_hash);

    let pending_after = backend
        .load_pending_atomic_broadcast_ticket(intent_id)
        .expect("load pending atomic-broadcast ticket after replay");
    assert!(pending_after.is_some());

    let (after_replay, changed_after_replay) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "evm_getAtomicReadyByIntentId",
        &serde_json::json!({
            "intent_id": intent_id,
        }),
    )
    .expect("query atomic-ready after replay queue");
    assert!(!changed_after_replay);
    assert_eq!(
        after_replay["status"].as_str(),
        Some(EVM_ATOMIC_READY_STATUS_BROADCAST_QUEUED_V1)
    );
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn evm_execute_atomic_broadcast_native_forced_succeeds() {
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-exec-atomic-native-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: spool_dir.join("eth-tx-index.rocksdb"),
    };
    let intent_id = "intent-exec-native-0001";
    let mut leg = TxIR::transfer(vec![0x41; 20], vec![0x42; 20], 1, 9, 1);
    leg.compute_hash();
    let tx_hash = vec_to_32(&leg.hash, "tx_hash").expect("decode tx hash");
    let ready_item = AtomicBroadcastReadyV1 {
        intent: novovm_adapter_api::AtomicCrossChainIntentV1 {
            intent_id: intent_id.to_string(),
            source_chain: novovm_adapter_api::ChainType::EVM,
            destination_chain: novovm_adapter_api::ChainType::NovoVM,
            ttl_unix_ms: 1_900_000_001_001,
            legs: vec![leg.clone()],
        },
        ready_at_unix_ms: 1_900_000_001_000,
    };
    backend
        .save_pending_atomic_ready(&ready_item)
        .expect("save pending atomic-ready");
    upsert_gateway_evm_atomic_ready_index(
        &backend,
        &ready_item,
        EVM_ATOMIC_READY_STATUS_BROADCAST_QUEUED_V1,
        Some(leg.chain_id),
        Some(&tx_hash),
    );

    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };
    let (executed, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "evm_executeAtomicBroadcast",
        &serde_json::json!({
            "intent_id": intent_id,
            "native": true,
        }),
    )
    .expect("execute atomic broadcast native");
    assert!(!changed);
    assert_eq!(executed["broadcasted"].as_bool(), Some(true));
    assert_eq!(executed["executor"].as_str(), Some("native"));
    assert_eq!(executed["attempts"].as_u64(), Some(1));
    let spool_file = PathBuf::from(
        executed["spool_file"]
            .as_str()
            .expect("native execute should return spool_file"),
    );
    let wire = fs::read(&spool_file).expect("read native execute spool");
    let value = decode_single_ops_wire_value(&wire).expect("decode native execute ops-wire");
    let record: GatewayIngressEthRecordV1 =
        bincode::deserialize(&value).expect("decode ingress record");
    assert_eq!(record.tx_hash, tx_hash);
    assert!(eth_tx_index.contains_key(&tx_hash));
    assert!(!evm_settlement_index_by_id.is_empty());
    let pending_ticket = backend
        .load_pending_atomic_broadcast_ticket(intent_id)
        .expect("load pending ticket after native execute");
    assert!(pending_ticket.is_none());
    let indexed = backend
        .load_evm_atomic_ready_by_intent(intent_id)
        .expect("load atomic-ready index")
        .expect("atomic-ready should exist");
    assert_eq!(indexed.status, EVM_ATOMIC_READY_STATUS_BROADCASTED_V1);
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn evm_execute_pending_atomic_broadcasts_native_forced_succeeds() {
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-exec-atomic-native-batch-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: spool_dir.join("eth-tx-index.rocksdb"),
    };

    let intents = [
        ("intent-exec-native-batch-0001", 15u64),
        ("intent-exec-native-batch-0002", 16u64),
    ];
    for (intent_id, nonce) in intents {
        let mut leg = TxIR::transfer(vec![0x51; 20], vec![0x52; 20], 1, nonce, 1);
        leg.compute_hash();
        let tx_hash = vec_to_32(&leg.hash, "tx_hash").expect("decode tx hash");
        let ready_item = AtomicBroadcastReadyV1 {
            intent: novovm_adapter_api::AtomicCrossChainIntentV1 {
                intent_id: intent_id.to_string(),
                source_chain: novovm_adapter_api::ChainType::EVM,
                destination_chain: novovm_adapter_api::ChainType::NovoVM,
                ttl_unix_ms: 1_900_000_001_100,
                legs: vec![leg.clone()],
            },
            ready_at_unix_ms: 1_900_000_001_050,
        };
        backend
            .save_pending_atomic_ready(&ready_item)
            .expect("save pending atomic-ready");
        upsert_gateway_evm_atomic_ready_index(
            &backend,
            &ready_item,
            EVM_ATOMIC_READY_STATUS_BROADCAST_QUEUED_V1,
            Some(leg.chain_id),
            Some(&tx_hash),
        );
        backend
            .save_pending_atomic_broadcast_ticket(&GatewayEvmAtomicBroadcastTicketV1 {
                intent_id: intent_id.to_string(),
                chain_id: leg.chain_id,
                tx_hash,
                ready_at_unix_ms: 1_900_000_001_050,
            })
            .expect("save pending atomic-broadcast ticket");
    }

    let mut router = UnifiedAccountRouter::new();
    let mut eth_tx_index = HashMap::new();
    let mut evm_settlement_index_by_id = HashMap::new();
    let mut evm_settlement_index_by_tx = HashMap::new();
    let mut evm_pending_payout_by_settlement = HashMap::new();
    let mut eth_filters = GatewayEthFilterState::default();
    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &backend,
        eth_default_chain_id: 1,
        spool_dir: &spool_dir,
        eth_filters: &mut eth_filters,
    };
    let (result, changed) = run_gateway_method(
        &mut router,
        &mut eth_tx_index,
        &mut evm_settlement_index_by_id,
        &mut evm_settlement_index_by_tx,
        &mut evm_pending_payout_by_settlement,
        &mut ctx,
        "evm_executePendingAtomicBroadcasts",
        &serde_json::json!({
            "native": true,
            "max_items": 8,
        }),
    )
    .expect("execute pending atomic broadcasts native");
    assert!(!changed);
    assert_eq!(result["executor"].as_str(), Some("native"));
    assert_eq!(result["total"].as_u64(), Some(2));
    assert_eq!(result["executed"].as_u64(), Some(2));
    assert_eq!(result["failed"].as_u64(), Some(0));
    assert!(eth_tx_index.len() >= 2);
    for (intent_id, _) in intents {
        let pending = backend
            .load_pending_atomic_broadcast_ticket(intent_id)
            .expect("load pending ticket after batch native execute");
        assert!(pending.is_none());
        let indexed = backend
            .load_evm_atomic_ready_by_intent(intent_id)
            .expect("load atomic-ready index after batch native execute")
            .expect("atomic-ready entry should exist");
        assert_eq!(indexed.status, EVM_ATOMIC_READY_STATUS_BROADCASTED_V1);
    }
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn auto_replay_pending_payouts_respects_cap_and_advances_status() {
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-auto-replay-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let mut runtime = GatewayRuntime {
        bind: "127.0.0.1:0".to_string(),
        spool_dir: spool_dir.clone(),
        max_body_bytes: 1024,
        max_requests: 0,
        evm_payout_autoreplay_max: 1,
        evm_payout_autoreplay_cooldown_ms: 0,
        evm_payout_pending_warn_threshold: usize::MAX,
        evm_payout_last_autoreplay_at_ms: 0,
        evm_payout_last_warn_at_ms: 0,
        eth_default_chain_id: 1,
        ua_store: GatewayUaStoreBackend::BincodeFile {
            path: spool_dir.join("ua-store.bin"),
        },
        eth_tx_index_store: GatewayEthTxIndexStoreBackend::Memory,
        eth_tx_index: HashMap::new(),
        eth_filters: GatewayEthFilterState::default(),
        evm_settlement_index_by_id: HashMap::new(),
        evm_settlement_index_by_tx: HashMap::new(),
        evm_pending_payout_by_settlement: HashMap::new(),
        router: UnifiedAccountRouter::new(),
    };
    let settlement_a = EvmFeeSettlementRecordV1 {
        income: novovm_adapter_api::EvmFeeIncomeRecordV1 {
            chain_id: 1,
            tx_hash: vec![0x11u8; 32],
            fee_amount_wei: 20_000,
            collector_address: vec![0x11; 20],
        },
        result: novovm_adapter_api::EvmFeeSettlementResultV1 {
            reserve_delta: 20_000,
            payout_delta: 18_000,
            settlement_id: "evm-settlement-auto-0001".to_string(),
        },
        settled_at_unix_ms: 100,
    };
    let settlement_b = EvmFeeSettlementRecordV1 {
        income: novovm_adapter_api::EvmFeeIncomeRecordV1 {
            chain_id: 1,
            tx_hash: vec![0x22u8; 32],
            fee_amount_wei: 30_000,
            collector_address: vec![0x22; 20],
        },
        result: novovm_adapter_api::EvmFeeSettlementResultV1 {
            reserve_delta: 30_000,
            payout_delta: 27_000,
            settlement_id: "evm-settlement-auto-0002".to_string(),
        },
        settled_at_unix_ms: 200,
    };
    upsert_gateway_evm_settlement_index(
        &mut runtime.evm_settlement_index_by_id,
        &mut runtime.evm_settlement_index_by_tx,
        &runtime.eth_tx_index_store,
        &settlement_a,
    )
    .expect("upsert settlement a");
    upsert_gateway_evm_settlement_index(
        &mut runtime.evm_settlement_index_by_id,
        &mut runtime.evm_settlement_index_by_tx,
        &runtime.eth_tx_index_store,
        &settlement_b,
    )
    .expect("upsert settlement b");
    set_gateway_evm_settlement_status(
        &mut runtime.evm_settlement_index_by_id,
        &runtime.eth_tx_index_store,
        "evm-settlement-auto-0001",
        EVM_SETTLEMENT_STATUS_COMPENSATE_PENDING_V1,
    );
    set_gateway_evm_settlement_status(
        &mut runtime.evm_settlement_index_by_id,
        &runtime.eth_tx_index_store,
        "evm-settlement-auto-0002",
        EVM_SETTLEMENT_STATUS_COMPENSATE_PENDING_V1,
    );
    mark_gateway_pending_payout(
        &mut runtime.evm_pending_payout_by_settlement,
        &runtime.eth_tx_index_store,
        &EvmFeePayoutInstructionV1 {
            settlement_id: "evm-settlement-auto-0001".to_string(),
            chain_id: 1,
            income_tx_hash: vec![0x11u8; 32],
            reserve_currency_code: "ETH".to_string(),
            payout_token_code: "NOVO".to_string(),
            reserve_delta_wei: 20_000,
            payout_delta_units: 18_000,
            reserve_account: vec![0x11; 20],
            payout_account: vec![0x22; 20],
            generated_at_unix_ms: 100,
        },
    );
    mark_gateway_pending_payout(
        &mut runtime.evm_pending_payout_by_settlement,
        &runtime.eth_tx_index_store,
        &EvmFeePayoutInstructionV1 {
            settlement_id: "evm-settlement-auto-0002".to_string(),
            chain_id: 1,
            income_tx_hash: vec![0x22u8; 32],
            reserve_currency_code: "ETH".to_string(),
            payout_token_code: "NOVO".to_string(),
            reserve_delta_wei: 30_000,
            payout_delta_units: 27_000,
            reserve_account: vec![0x11; 20],
            payout_account: vec![0x22; 20],
            generated_at_unix_ms: 200,
        },
    );

    auto_replay_pending_payouts(&mut runtime);
    assert_eq!(runtime.evm_pending_payout_by_settlement.len(), 1);
    assert!(!runtime
        .evm_pending_payout_by_settlement
        .contains_key("evm-settlement-auto-0001"));
    assert!(runtime
        .evm_pending_payout_by_settlement
        .contains_key("evm-settlement-auto-0002"));
    let status_a = runtime
        .evm_settlement_index_by_id
        .get("evm-settlement-auto-0001")
        .map(|entry| entry.status.as_str());
    let status_b = runtime
        .evm_settlement_index_by_id
        .get("evm-settlement-auto-0002")
        .map(|entry| entry.status.as_str());
    assert_eq!(status_a, Some(EVM_SETTLEMENT_STATUS_COMPENSATED_V1));
    assert_eq!(status_b, Some(EVM_SETTLEMENT_STATUS_COMPENSATE_PENDING_V1));
    let file_count = fs::read_dir(&spool_dir)
        .expect("read spool dir")
        .filter_map(|entry| entry.ok())
        .count();
    assert!(file_count > 0);
    let _ = fs::remove_dir_all(&spool_dir);
}

#[test]
fn gateway_error_code_maps_txpool_reject_reasons() {
    let base = "gateway evm txpool rejected tx: chain=evm chain_id=1 tx_hash=0x00 requested=1 accepted=0 dropped=1";
    let underpriced = format!(
        "{base} reason=replacement_underpriced dropped_underpriced=1 dropped_nonce_gap=0 dropped_nonce_too_low=0 dropped_over_capacity=0"
    );
    let nonce_too_low = format!(
        "{base} reason=nonce_too_low dropped_underpriced=0 dropped_nonce_gap=0 dropped_nonce_too_low=1 dropped_over_capacity=0"
    );
    let nonce_gap = format!(
        "{base} reason=nonce_too_high dropped_underpriced=0 dropped_nonce_gap=1 dropped_nonce_too_low=0 dropped_over_capacity=0"
    );
    let over_capacity = format!(
        "{base} reason=pool_full dropped_underpriced=0 dropped_nonce_gap=0 dropped_nonce_too_low=0 dropped_over_capacity=1"
    );
    let unknown = format!(
        "{base} dropped_underpriced=0 dropped_nonce_gap=0 dropped_nonce_too_low=0 dropped_over_capacity=0"
    );
    assert_eq!(
        gateway_error_code_for_method("eth_sendRawTransaction", &underpriced),
        -32034
    );
    assert_eq!(
        gateway_error_code_for_method("eth_sendRawTransaction", &nonce_too_low),
        -32035
    );
    assert_eq!(
        gateway_error_code_for_method("eth_sendTransaction", &nonce_gap),
        -32037
    );
    assert_eq!(
        gateway_error_code_for_method("web30_sendTransaction", &over_capacity),
        -32038
    );
    assert_eq!(
        gateway_error_code_for_method("web30_sendRawTransaction", &unknown),
        -32030
    );
}

#[test]
fn gateway_error_message_maps_txpool_codes_to_geth_style_text() {
    let raw = "gateway evm txpool rejected tx: chain=evm chain_id=1 tx_hash=0x00 reason=replacement_underpriced requested=1 accepted=0 dropped=1 dropped_underpriced=1 dropped_nonce_gap=0 dropped_nonce_too_low=0 dropped_over_capacity=0";
    assert_eq!(
        gateway_error_message_for_method("eth_sendRawTransaction", -32034, raw),
        "replacement transaction underpriced"
    );
    assert_eq!(
        gateway_error_message_for_method("eth_sendTransaction", -32035, raw),
        "nonce too low"
    );
    assert_eq!(
        gateway_error_message_for_method("eth_sendTransaction", -32037, raw),
        "nonce too high"
    );
    assert_eq!(
        gateway_error_message_for_method("web30_sendTransaction", -32038, raw),
        "txpool is full"
    );
    assert_eq!(
        gateway_error_message_for_method("web30_sendRawTransaction", -32030, raw),
        "transaction rejected"
    );
    // Non-EVM write methods keep original message.
    assert_eq!(
        gateway_error_message_for_method("ua_createUca", -32010, "uca exists"),
        "uca exists"
    );
}

#[test]
fn gateway_error_data_for_txpool_reject_is_structured() {
    let raw = "gateway evm txpool rejected tx: chain=evm chain_id=1 tx_hash=0x00 reason=replacement_underpriced reasons=replacement_underpriced,pool_full requested=2 accepted=0 dropped=2 dropped_underpriced=2 dropped_nonce_gap=0 dropped_nonce_too_low=0 dropped_over_capacity=0";
    let data = gateway_error_data_for_method("eth_sendRawTransaction", -32034, raw)
        .expect("txpool reject should carry data");
    assert_eq!(data["category"].as_str(), Some("txpool_reject"));
    assert_eq!(data["reason"].as_str(), Some("replacement_underpriced"));
    assert_eq!(
        data["reasons"]
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item.as_str()),
        Some("replacement_underpriced")
    );
    assert_eq!(
        data["reasons"]
            .as_array()
            .and_then(|items| items.get(1))
            .and_then(|item| item.as_str()),
        Some("pool_full")
    );
    assert_eq!(data["requested"].as_u64(), Some(2));
    assert_eq!(data["accepted"].as_u64(), Some(0));
    assert_eq!(data["dropped"].as_u64(), Some(2));
    assert_eq!(data["dropped_underpriced"].as_u64(), Some(2));
}

#[test]
fn gateway_error_code_prefers_reason_token_over_counters() {
    let raw = "gateway evm txpool rejected tx: chain=evm chain_id=1 tx_hash=0x00 reason=nonce_too_low requested=1 accepted=0 dropped=1 dropped_underpriced=1 dropped_nonce_gap=0 dropped_nonce_too_low=0 dropped_over_capacity=0";
    assert_eq!(
        gateway_error_code_for_method("eth_sendRawTransaction", raw),
        -32035
    );
}

#[test]
fn gateway_error_code_prefers_reasons_list_over_counters() {
    let raw = "gateway evm txpool rejected tx: chain=evm chain_id=1 tx_hash=0x00 reasons=nonce_too_high,replacement_underpriced requested=1 accepted=0 dropped=1 dropped_underpriced=1 dropped_nonce_gap=0 dropped_nonce_too_low=0 dropped_over_capacity=0";
    assert_eq!(
        gateway_error_code_for_method("eth_sendRawTransaction", raw),
        -32037
    );
}

#[test]
fn gateway_error_code_maps_atomic_gate_failures() {
    let rejected = "plugin_atomic_gate_rejected: rejected_receipts=1 chain=evm chain_id=1 tx_hash=0x00 reasons=ttl_expired";
    let not_ready =
        "plugin_atomic_gate_not_ready: ready_items=0 matched_ready=0 chain=evm chain_id=1 tx_hash=0x00";
    assert_eq!(
        gateway_error_code_for_method("eth_sendRawTransaction", rejected),
        -32036
    );
    assert_eq!(
        gateway_error_code_for_method("web30_sendTransaction", not_ready),
        -32039
    );
}

#[test]
fn gateway_error_data_for_atomic_gate_is_structured() {
    let rejected = "plugin_atomic_gate_rejected: rejected_receipts=2 chain=evm chain_id=1 tx_hash=0x00 reasons=ttl_expired,nonce_replay";
    let data = gateway_error_data_for_method("eth_sendRawTransaction", -32036, rejected)
        .expect("atomic reject should carry data");
    assert_eq!(data["category"].as_str(), Some("atomic_gate"));
    assert_eq!(data["state"].as_str(), Some("rejected"));
    assert_eq!(data["rejected_receipts"].as_u64(), Some(2));
    assert_eq!(
        data["reasons"]
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item.as_str()),
        Some("ttl_expired")
    );
}

#[test]
fn gateway_error_code_and_data_for_public_broadcast_failure() {
    let raw = "public broadcast failed: chain_id=1 tx_hash=0x11 attempts=2 err=executor timeout";
    assert_eq!(
        gateway_error_code_for_method("eth_sendRawTransaction", raw),
        -32040
    );
    assert_eq!(
        gateway_error_message_for_method("eth_sendRawTransaction", -32040, raw),
        "public broadcast failed"
    );
    let data = gateway_error_data_for_method("eth_sendRawTransaction", -32040, raw)
        .expect("public broadcast failure should carry data");
    assert_eq!(data["category"].as_str(), Some("public_broadcast"));
    assert_eq!(data["reason"].as_str(), Some("broadcast_failed"));
    assert_eq!(data["attempts"].as_u64(), Some(2));
    assert_eq!(data["chain_id"].as_u64(), Some(1));
    assert_eq!(data["tx_hash"].as_str(), Some("0x11"));
}

#[test]
fn gateway_error_data_for_non_txpool_returns_none() {
    let data = gateway_error_data_for_method("ua_createUca", -32010, "uca exists");
    assert!(data.is_none());
}

#[test]
fn atomic_broadcast_executor_output_validation_accepts_matching_json() {
    let ticket = GatewayEvmAtomicBroadcastTicketV1 {
        intent_id: "intent-validation-0001".to_string(),
        chain_id: 1,
        tx_hash: [0x11u8; 32],
        ready_at_unix_ms: 1_900_000_000_777,
    };
    let output = serde_json::json!({
        "broadcasted": true,
        "intent_id": ticket.intent_id,
        "chain_id": "0x1",
        "tx_hash": format!("0x{}", to_hex(&ticket.tx_hash)),
    })
    .to_string();
    validate_gateway_atomic_broadcast_executor_output(&output, &ticket)
        .expect("matching json output should pass");
}

#[test]
fn atomic_broadcast_executor_output_validation_accepts_plain_text_legacy_output() {
    let ticket = GatewayEvmAtomicBroadcastTicketV1 {
        intent_id: "intent-validation-legacy".to_string(),
        chain_id: 1,
        tx_hash: [0x22u8; 32],
        ready_at_unix_ms: 1_900_000_000_778,
    };
    validate_gateway_atomic_broadcast_executor_output("ok", &ticket)
        .expect("legacy plain text output should pass");
}

#[test]
fn atomic_broadcast_executor_output_validation_rejects_mismatch() {
    let ticket = GatewayEvmAtomicBroadcastTicketV1 {
        intent_id: "intent-validation-0002".to_string(),
        chain_id: 1,
        tx_hash: [0x33u8; 32],
        ready_at_unix_ms: 1_900_000_000_779,
    };
    let output = serde_json::json!({
        "broadcasted": true,
        "intent_id": ticket.intent_id,
        "chain_id": "0x1",
        "tx_hash": format!("0x{}", "44".repeat(32)),
    })
    .to_string();
    let err = validate_gateway_atomic_broadcast_executor_output(&output, &ticket)
        .expect_err("mismatch tx_hash should fail");
    assert!(err.to_string().contains("tx_hash mismatch"));
}

#[test]
fn public_broadcast_executor_output_validation_accepts_matching_json() {
    let tx_hash = [0x55u8; 32];
    let output = serde_json::json!({
        "broadcasted": true,
        "chain_id": "0x1",
        "tx_hash": format!("0x{}", to_hex(&tx_hash)),
    })
    .to_string();
    validate_gateway_eth_public_broadcast_executor_output(&output, 1, &tx_hash)
        .expect("matching json output should pass");
}

#[test]
fn build_public_broadcast_request_supports_tx_ir_only() {
    let tx_hash = [0x44u8; 32];
    let tx_ir_bincode = [0x01u8, 0x02, 0x03];
    let req = build_gateway_eth_public_broadcast_executor_request(
        1,
        &tx_hash,
        GatewayEthPublicBroadcastPayload {
            raw_tx: None,
            tx_ir_bincode: Some(tx_ir_bincode.as_slice()),
        },
    );
    let expected_tx_hash = format!("0x{}", to_hex(&tx_hash));
    assert_eq!(req["chain_id"].as_str(), Some("0x1"));
    assert_eq!(req["tx_hash"].as_str(), Some(expected_tx_hash.as_str()));
    assert_eq!(req["tx_ir_bincode"].as_str(), Some("0x010203"));
    assert_eq!(req["tx_ir_format"].as_str(), Some("bincode_v1"));
    assert!(req.get("raw_tx").is_none());
    assert!(req.get("raw_tx_len").is_none());
}

#[test]
fn build_public_broadcast_request_supports_raw_and_tx_ir() {
    let tx_hash = [0x33u8; 32];
    let raw_tx = [0xaau8, 0xbb, 0xcc];
    let tx_ir_bincode = [0x10u8, 0x20];
    let req = build_gateway_eth_public_broadcast_executor_request(
        10,
        &tx_hash,
        GatewayEthPublicBroadcastPayload {
            raw_tx: Some(raw_tx.as_slice()),
            tx_ir_bincode: Some(tx_ir_bincode.as_slice()),
        },
    );
    assert_eq!(req["chain_id"].as_str(), Some("0xa"));
    assert_eq!(req["raw_tx"].as_str(), Some("0xaabbcc"));
    assert_eq!(req["raw_tx_len"].as_str(), Some("0x3"));
    assert_eq!(req["tx_ir_bincode"].as_str(), Some("0x1020"));
    assert_eq!(req["tx_ir_format"].as_str(), Some("bincode_v1"));
}

#[test]
fn public_broadcast_executor_output_validation_rejects_mismatch() {
    let tx_hash = [0x66u8; 32];
    let output = serde_json::json!({
        "broadcasted": true,
        "chain_id": "0x1",
        "tx_hash": format!("0x{}", "77".repeat(32)),
    })
    .to_string();
    let err = validate_gateway_eth_public_broadcast_executor_output(&output, 1, &tx_hash)
        .expect_err("mismatch tx_hash should fail");
    assert!(err.to_string().contains("tx_hash mismatch"));
}

#[test]
fn atomic_ready_index_entry_prefers_tx_hash_hint() {
    let mut leg_a = TxIR::transfer(vec![0x11; 20], vec![0x22; 20], 1, 10, 1);
    leg_a.compute_hash();
    let mut leg_b = TxIR::transfer(vec![0x33; 20], vec![0x44; 20], 2, 20, 1);
    leg_b.compute_hash();
    let hash_b = vec_to_32(&leg_b.hash, "hash_b").expect("hash_b");
    let item = AtomicBroadcastReadyV1 {
        intent: novovm_adapter_api::AtomicCrossChainIntentV1 {
            intent_id: "intent-hint-0001".to_string(),
            source_chain: novovm_adapter_api::ChainType::EVM,
            destination_chain: novovm_adapter_api::ChainType::NovoVM,
            ttl_unix_ms: 1_900_000_000_888,
            legs: vec![leg_a, leg_b.clone()],
        },
        ready_at_unix_ms: 1_900_000_000_889,
    };
    let entry = atomic_ready_index_entry_from_item(
        &item,
        EVM_ATOMIC_READY_STATUS_SPOOLED_V1,
        Some(leg_b.chain_id),
        Some(&hash_b),
    );
    assert_eq!(entry.chain_id, leg_b.chain_id);
    assert_eq!(entry.tx_hash, hash_b);
}

#[test]
fn atomic_ready_tx_ir_bincode_from_item_prefers_hint() {
    let mut leg_a = TxIR::transfer(vec![0x55; 20], vec![0x66; 20], 1, 30, 1);
    leg_a.compute_hash();
    let mut leg_b = TxIR::transfer(vec![0x77; 20], vec![0x88; 20], 2, 40, 1);
    leg_b.compute_hash();
    let hash_b = vec_to_32(&leg_b.hash, "hash_b").expect("hash_b");
    let expected = leg_b
        .serialize(SerializationFormat::Bincode)
        .expect("serialize bincode");
    let item = AtomicBroadcastReadyV1 {
        intent: novovm_adapter_api::AtomicCrossChainIntentV1 {
            intent_id: "intent-hint-0002".to_string(),
            source_chain: novovm_adapter_api::ChainType::EVM,
            destination_chain: novovm_adapter_api::ChainType::NovoVM,
            ttl_unix_ms: 1_900_000_000_890,
            legs: vec![leg_a, leg_b],
        },
        ready_at_unix_ms: 1_900_000_000_891,
    };
    let actual = atomic_ready_tx_ir_bincode_from_item(&item, None, Some(&hash_b));
    assert_eq!(actual, expected);
}

#[test]
fn build_atomic_broadcast_executor_request_embeds_tx_ir_bincode_when_present() {
    let ticket = GatewayEvmAtomicBroadcastTicketV1 {
        intent_id: "intent-request-0001".to_string(),
        chain_id: 1,
        tx_hash: [0xa1u8; 32],
        ready_at_unix_ms: 1_900_000_000_892,
    };
    let req = build_gateway_atomic_broadcast_executor_request(&ticket, Some(&[0x01, 0x02, 0x03]));
    assert_eq!(req["intent_id"].as_str(), Some("intent-request-0001"));
    assert_eq!(req["tx_ir_bincode"].as_str(), Some("0x010203"));
    assert_eq!(req["tx_ir_format"].as_str(), Some("bincode_v1"));
}

#[test]
fn decode_atomic_broadcast_tx_ir_bincode_accepts_single_tx_ir() {
    let mut tx = TxIR::transfer(vec![0x81; 20], vec![0x82; 20], 3, 77, 1);
    tx.compute_hash();
    let payload = tx
        .serialize(SerializationFormat::Bincode)
        .expect("serialize bincode");
    let decoded =
        decode_gateway_atomic_broadcast_tx_ir_bincode(&payload).expect("decode single tx_ir");
    assert_eq!(decoded.chain_id, tx.chain_id);
    assert_eq!(decoded.nonce, tx.nonce);
    assert_eq!(decoded.hash, tx.hash);
}

#[test]
fn decode_atomic_broadcast_tx_ir_bincode_accepts_singleton_vec_tx_ir() {
    let mut tx = TxIR::transfer(vec![0x83; 20], vec![0x84; 20], 5, 88, 1);
    tx.compute_hash();
    let payload = bincode::serialize(&vec![tx.clone()]).expect("serialize vec bincode");
    let decoded = decode_gateway_atomic_broadcast_tx_ir_bincode(&payload)
        .expect("decode singleton vec tx_ir");
    assert_eq!(decoded.chain_id, tx.chain_id);
    assert_eq!(decoded.nonce, tx.nonce);
    assert_eq!(decoded.hash, tx.hash);
}

#[test]
fn load_atomic_broadcast_tx_ir_bincode_uses_cached_payload_after_pending_ready_cleared() {
    let spool_dir = std::env::temp_dir().join(format!(
        "novovm-gateway-atomic-payload-cache-{}-{}",
        std::process::id(),
        now_unix_millis()
    ));
    fs::create_dir_all(&spool_dir).expect("create spool dir");
    let backend = GatewayEthTxIndexStoreBackend::RocksDb {
        path: spool_dir.join("eth-tx-index.rocksdb"),
    };
    let intent_id = "intent-payload-cache-0001";
    let mut leg = TxIR::transfer(vec![0x91; 20], vec![0x92; 20], 7, 88, 1);
    leg.compute_hash();
    let tx_hash = vec_to_32(&leg.hash, "tx_hash").expect("tx_hash");
    let expected = leg
        .serialize(SerializationFormat::Bincode)
        .expect("serialize bincode");
    let ready_item = AtomicBroadcastReadyV1 {
        intent: novovm_adapter_api::AtomicCrossChainIntentV1 {
            intent_id: intent_id.to_string(),
            source_chain: novovm_adapter_api::ChainType::EVM,
            destination_chain: novovm_adapter_api::ChainType::NovoVM,
            ttl_unix_ms: 1_900_000_000_999,
            legs: vec![leg],
        },
        ready_at_unix_ms: 1_900_000_000_998,
    };
    backend
        .save_pending_atomic_ready(&ready_item)
        .expect("save pending atomic-ready");

    let first =
        load_atomic_broadcast_tx_ir_bincode_from_pending_ready(&backend, intent_id, 7, &tx_hash)
            .expect("payload from pending ready");
    assert_eq!(first, expected);

    backend
        .delete_pending_atomic_ready(intent_id)
        .expect("delete pending atomic-ready");
    let second =
        load_atomic_broadcast_tx_ir_bincode_from_pending_ready(&backend, intent_id, 7, &tx_hash)
            .expect("payload from cached store");
    assert_eq!(second, expected);
    let _ = fs::remove_dir_all(&spool_dir);
}
