use super::*;
use novovm_network::observe_network_runtime_local_head_max;

pub(super) fn gateway_evm_settlement_json(
    entry: &GatewayEvmSettlementIndexEntry,
) -> serde_json::Value {
    serde_json::json!({
        "settlement_id": entry.settlement_id,
        "chain_id": format!("0x{:x}", entry.chain_id),
        "income_tx_hash": format!("0x{}", to_hex(&entry.income_tx_hash)),
        "reserve_delta_wei": format!("0x{:x}", entry.reserve_delta_wei),
        "payout_delta_units": format!("0x{:x}", entry.payout_delta_units),
        "settled_at_unix_ms": entry.settled_at_unix_ms,
        "status": entry.status,
    })
}

pub(super) fn gateway_evm_atomic_ready_json(
    entry: &GatewayEvmAtomicReadyIndexEntry,
) -> serde_json::Value {
    serde_json::json!({
        "intent_id": entry.intent_id,
        "chain_id": format!("0x{:x}", entry.chain_id),
        "tx_hash": format!("0x{}", to_hex(&entry.tx_hash)),
        "ready_at_unix_ms": entry.ready_at_unix_ms,
        "status": entry.status,
    })
}

pub(super) fn upsert_gateway_eth_tx_index(
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    record: &GatewayIngressEthRecordV1,
) {
    let entry = GatewayEthTxIndexEntry {
        tx_hash: record.tx_hash,
        uca_id: record.uca_id.clone(),
        chain_id: record.chain_id,
        nonce: record.nonce,
        tx_type: record.tx_type,
        from: record.from.clone(),
        to: record.to.clone(),
        value: record.value,
        gas_limit: record.gas_limit,
        gas_price: record.gas_price,
        input: record.data.clone(),
    };
    eth_tx_index.insert(record.tx_hash, entry.clone());
    if let Err(e) = eth_tx_index_store.save_eth_tx(&entry) {
        if gateway_warn_enabled() {
            eprintln!(
                "gateway_warn: persist eth tx index failed for hash=0x{} backend={} err={}",
                to_hex(&record.tx_hash),
                eth_tx_index_store.backend_name(),
                e
            );
        }
    }
    let _ = observe_network_runtime_local_head_max(record.chain_id, record.nonce);
}

pub(super) fn gateway_eth_default_max_priority_fee_per_gas_wei(chain_id: u64) -> u64 {
    gateway_eth_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_DEFAULT_MAX_PRIORITY_FEE_PER_GAS",
        gateway_eth_default_gas_price_wei(chain_id),
    )
}

pub(super) fn gateway_eth_type2_fee_fields_json(
    max_fee_per_gas: u64,
    chain_id: u64,
) -> (serde_json::Value, serde_json::Value) {
    let max_priority_fee_per_gas =
        gateway_eth_default_max_priority_fee_per_gas_wei(chain_id).min(max_fee_per_gas);
    (
        serde_json::Value::String(format!("0x{:x}", max_fee_per_gas)),
        serde_json::Value::String(format!("0x{:x}", max_priority_fee_per_gas)),
    )
}

pub(super) fn gateway_eth_tx_fee_fields_json_from_entry(
    entry: &GatewayEthTxIndexEntry,
) -> (serde_json::Value, serde_json::Value) {
    if entry.tx_type == 2 || entry.tx_type == 3 {
        gateway_eth_type2_fee_fields_json(entry.gas_price, entry.chain_id)
    } else {
        (serde_json::Value::Null, serde_json::Value::Null)
    }
}

pub(super) fn gateway_eth_tx_fee_fields_json_from_ir(
    tx: &TxIR,
) -> (serde_json::Value, serde_json::Value) {
    let is_dynamic_fee = resolve_raw_evm_tx_route_hint_m0(&tx.signature)
        .map(|hint| {
            hint.envelope == EvmRawTxEnvelopeType::Type2DynamicFee
                || hint.envelope == EvmRawTxEnvelopeType::Type3Blob
        })
        .unwrap_or(false);
    if is_dynamic_fee {
        gateway_eth_type2_fee_fields_json(tx.gas_price, tx.chain_id)
    } else {
        (serde_json::Value::Null, serde_json::Value::Null)
    }
}

pub(super) fn gateway_eth_tx_by_hash_json(entry: &GatewayEthTxIndexEntry) -> serde_json::Value {
    let (max_fee_per_gas, max_priority_fee_per_gas) =
        gateway_eth_tx_fee_fields_json_from_entry(entry);
    serde_json::json!({
        "hash": format!("0x{}", to_hex(&entry.tx_hash)),
        "nonce": format!("0x{:x}", entry.nonce),
        "blockHash": serde_json::Value::Null,
        "blockNumber": serde_json::Value::Null,
        "transactionIndex": serde_json::Value::Null,
        "from": format!("0x{}", to_hex(&entry.from)),
        "to": entry.to.as_ref().map(|v| format!("0x{}", to_hex(v))),
        "value": format!("0x{:x}", entry.value),
        "gas": format!("0x{:x}", entry.gas_limit),
        "gasPrice": format!("0x{:x}", entry.gas_price),
        "maxFeePerGas": max_fee_per_gas,
        "maxPriorityFeePerGas": max_priority_fee_per_gas,
        "input": format!("0x{}", to_hex(&entry.input)),
        "chainId": format!("0x{:x}", entry.chain_id),
        "type": format!("0x{:x}", entry.tx_type),
        "pending": true,
        "uca_id": entry.uca_id.clone(),
    })
}

pub(super) fn gateway_eth_tx_confirmed_without_position_json(
    entry: &GatewayEthTxIndexEntry,
) -> serde_json::Value {
    let (max_fee_per_gas, max_priority_fee_per_gas) =
        gateway_eth_tx_fee_fields_json_from_entry(entry);
    serde_json::json!({
        "hash": format!("0x{}", to_hex(&entry.tx_hash)),
        "nonce": format!("0x{:x}", entry.nonce),
        "blockHash": serde_json::Value::Null,
        "blockNumber": format!("0x{:x}", entry.nonce),
        "transactionIndex": serde_json::Value::Null,
        "from": format!("0x{}", to_hex(&entry.from)),
        "to": entry.to.as_ref().map(|v| format!("0x{}", to_hex(v))),
        "value": format!("0x{:x}", entry.value),
        "gas": format!("0x{:x}", entry.gas_limit),
        "gasPrice": format!("0x{:x}", entry.gas_price),
        "maxFeePerGas": max_fee_per_gas,
        "maxPriorityFeePerGas": max_priority_fee_per_gas,
        "input": format!("0x{}", to_hex(&entry.input)),
        "chainId": format!("0x{:x}", entry.chain_id),
        "type": format!("0x{:x}", entry.tx_type),
        "pending": false,
        "uca_id": entry.uca_id.clone(),
    })
}

pub(super) fn gateway_eth_tx_with_block_json(
    entry: &GatewayEthTxIndexEntry,
    block_number: u64,
    tx_index: usize,
    block_hash: &[u8; 32],
) -> serde_json::Value {
    let (max_fee_per_gas, max_priority_fee_per_gas) =
        gateway_eth_tx_fee_fields_json_from_entry(entry);
    serde_json::json!({
        "hash": format!("0x{}", to_hex(&entry.tx_hash)),
        "nonce": format!("0x{:x}", entry.nonce),
        "blockHash": format!("0x{}", to_hex(block_hash)),
        "blockNumber": format!("0x{:x}", block_number),
        "transactionIndex": format!("0x{:x}", tx_index),
        "from": format!("0x{}", to_hex(&entry.from)),
        "to": entry.to.as_ref().map(|v| format!("0x{}", to_hex(v))),
        "value": format!("0x{:x}", entry.value),
        "gas": format!("0x{:x}", entry.gas_limit),
        "gasPrice": format!("0x{:x}", entry.gas_price),
        "maxFeePerGas": max_fee_per_gas,
        "maxPriorityFeePerGas": max_priority_fee_per_gas,
        "input": format!("0x{}", to_hex(&entry.input)),
        "chainId": format!("0x{:x}", entry.chain_id),
        "type": format!("0x{:x}", entry.tx_type),
        "pending": false,
        "uca_id": entry.uca_id.clone(),
    })
}

pub(super) fn gateway_eth_tx_pending_with_block_json(
    entry: &GatewayEthTxIndexEntry,
    _block_number: u64,
    _tx_index: usize,
    _block_hash: &[u8; 32],
) -> serde_json::Value {
    let (max_fee_per_gas, max_priority_fee_per_gas) =
        gateway_eth_tx_fee_fields_json_from_entry(entry);
    serde_json::json!({
        "hash": format!("0x{}", to_hex(&entry.tx_hash)),
        "nonce": format!("0x{:x}", entry.nonce),
        "blockHash": serde_json::Value::Null,
        "blockNumber": serde_json::Value::Null,
        "transactionIndex": serde_json::Value::Null,
        "from": format!("0x{}", to_hex(&entry.from)),
        "to": entry.to.as_ref().map(|v| format!("0x{}", to_hex(v))),
        "value": format!("0x{:x}", entry.value),
        "gas": format!("0x{:x}", entry.gas_limit),
        "gasPrice": format!("0x{:x}", entry.gas_price),
        "maxFeePerGas": max_fee_per_gas,
        "maxPriorityFeePerGas": max_priority_fee_per_gas,
        "input": format!("0x{}", to_hex(&entry.input)),
        "chainId": format!("0x{:x}", entry.chain_id),
        "type": format!("0x{:x}", entry.tx_type),
        "pending": true,
        "uca_id": entry.uca_id.clone(),
    })
}

pub(super) fn gateway_eth_tx_by_hash_query_json(
    entry: &GatewayEthTxIndexEntry,
    eth_tx_index: &HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
) -> Result<serde_json::Value> {
    let chain_entries = collect_gateway_eth_chain_entries(
        eth_tx_index,
        eth_tx_index_store,
        entry.chain_id,
        gateway_eth_query_scan_max(),
    )?;
    let blocks = gateway_eth_group_entries_by_block(chain_entries);
    if let Some(block_txs) = blocks.get(&entry.nonce) {
        let mut sorted = block_txs.clone();
        sort_gateway_eth_block_txs(&mut sorted);
        if let Some(tx_index) = sorted.iter().position(|tx| tx.tx_hash == entry.tx_hash) {
            let block_hash = gateway_eth_block_hash_for_txs(entry.chain_id, entry.nonce, &sorted);
            return Ok(gateway_eth_tx_with_block_json(
                entry,
                entry.nonce,
                tx_index,
                &block_hash,
            ));
        }
    }
    let precise_block_txs = collect_gateway_eth_block_entries_precise(
        eth_tx_index,
        eth_tx_index_store,
        entry.chain_id,
        entry.nonce,
        gateway_eth_query_scan_max(),
    )?;
    if !precise_block_txs.is_empty() {
        let mut sorted = precise_block_txs;
        sort_gateway_eth_block_txs(&mut sorted);
        if let Some(tx_index) = sorted.iter().position(|tx| tx.tx_hash == entry.tx_hash) {
            let block_hash = gateway_eth_block_hash_for_txs(entry.chain_id, entry.nonce, &sorted);
            return Ok(gateway_eth_tx_with_block_json(
                entry,
                entry.nonce,
                tx_index,
                &block_hash,
            ));
        }
    }
    Ok(gateway_eth_tx_confirmed_without_position_json(entry))
}

pub(super) fn gateway_eth_tx_receipt_json(entry: &GatewayEthTxIndexEntry) -> serde_json::Value {
    let contract_address = gateway_eth_contract_address_hex(entry);
    let effective_gas_price = gateway_eth_effective_gas_price_wei(
        entry,
        gateway_eth_base_fee_per_gas_wei(entry.chain_id),
    );
    let logs = gateway_eth_receipt_logs_json(entry, None, None, None);
    let logs_bloom = gateway_eth_receipt_logs_bloom_hex(entry);
    serde_json::json!({
        "transactionHash": format!("0x{}", to_hex(&entry.tx_hash)),
        "transactionIndex": serde_json::Value::Null,
        "blockHash": serde_json::Value::Null,
        "blockNumber": serde_json::Value::Null,
        "from": format!("0x{}", to_hex(&entry.from)),
        "to": entry.to.as_ref().map(|v| format!("0x{}", to_hex(v))),
        "cumulativeGasUsed": format!("0x{:x}", entry.gas_limit),
        "gasUsed": format!("0x{:x}", entry.gas_limit),
        "effectiveGasPrice": format!("0x{:x}", effective_gas_price),
        "contractAddress": contract_address,
        "logs": logs,
        "logsBloom": logs_bloom,
        "type": format!("0x{:x}", entry.tx_type),
        "status": serde_json::Value::Null,
        "pending": true,
        "uca_id": entry.uca_id.clone(),
    })
}

pub(super) fn gateway_eth_tx_receipt_confirmed_without_position_json(
    entry: &GatewayEthTxIndexEntry,
) -> serde_json::Value {
    let contract_address = gateway_eth_contract_address_hex(entry);
    let effective_gas_price = gateway_eth_effective_gas_price_wei(
        entry,
        gateway_eth_base_fee_per_gas_wei(entry.chain_id),
    );
    let logs = gateway_eth_receipt_logs_json(entry, Some(entry.nonce), None, None);
    let logs_bloom = gateway_eth_receipt_logs_bloom_hex(entry);
    serde_json::json!({
        "transactionHash": format!("0x{}", to_hex(&entry.tx_hash)),
        "transactionIndex": serde_json::Value::Null,
        "blockHash": serde_json::Value::Null,
        "blockNumber": format!("0x{:x}", entry.nonce),
        "from": format!("0x{}", to_hex(&entry.from)),
        "to": entry.to.as_ref().map(|v| format!("0x{}", to_hex(v))),
        "cumulativeGasUsed": format!("0x{:x}", entry.gas_limit),
        "gasUsed": format!("0x{:x}", entry.gas_limit),
        "effectiveGasPrice": format!("0x{:x}", effective_gas_price),
        "contractAddress": contract_address,
        "logs": logs,
        "logsBloom": logs_bloom,
        "type": format!("0x{:x}", entry.tx_type),
        "status": "0x1",
        "pending": false,
        "uca_id": entry.uca_id.clone(),
    })
}

pub(super) fn gateway_eth_tx_receipt_with_block_json(
    entry: &GatewayEthTxIndexEntry,
    block_number: u64,
    tx_index: usize,
    block_hash: &[u8; 32],
    cumulative_gas_used: u128,
) -> serde_json::Value {
    let contract_address = gateway_eth_contract_address_hex(entry);
    let effective_gas_price = gateway_eth_effective_gas_price_wei(
        entry,
        gateway_eth_base_fee_per_gas_wei(entry.chain_id),
    );
    let logs =
        gateway_eth_receipt_logs_json(entry, Some(block_number), Some(tx_index), Some(block_hash));
    let logs_bloom = gateway_eth_receipt_logs_bloom_hex(entry);
    serde_json::json!({
        "transactionHash": format!("0x{}", to_hex(&entry.tx_hash)),
        "transactionIndex": format!("0x{:x}", tx_index),
        "blockHash": format!("0x{}", to_hex(block_hash)),
        "blockNumber": format!("0x{:x}", block_number),
        "from": format!("0x{}", to_hex(&entry.from)),
        "to": entry.to.as_ref().map(|v| format!("0x{}", to_hex(v))),
        "cumulativeGasUsed": format!("0x{:x}", cumulative_gas_used),
        "gasUsed": format!("0x{:x}", entry.gas_limit),
        "effectiveGasPrice": format!("0x{:x}", effective_gas_price),
        "contractAddress": contract_address,
        "logs": logs,
        "logsBloom": logs_bloom,
        "type": format!("0x{:x}", entry.tx_type),
        "status": "0x1",
        "pending": false,
        "uca_id": entry.uca_id.clone(),
    })
}

pub(super) fn gateway_eth_tx_receipt_pending_with_block_json(
    entry: &GatewayEthTxIndexEntry,
    _block_number: u64,
    _tx_index: usize,
    _block_hash: &[u8; 32],
    cumulative_gas_used: u128,
) -> serde_json::Value {
    let contract_address = gateway_eth_contract_address_hex(entry);
    let effective_gas_price = gateway_eth_effective_gas_price_wei(
        entry,
        gateway_eth_base_fee_per_gas_wei(entry.chain_id),
    );
    let logs = gateway_eth_receipt_logs_json(entry, None, None, None);
    let logs_bloom = gateway_eth_receipt_logs_bloom_hex(entry);
    serde_json::json!({
        "transactionHash": format!("0x{}", to_hex(&entry.tx_hash)),
        "transactionIndex": serde_json::Value::Null,
        "blockHash": serde_json::Value::Null,
        "blockNumber": serde_json::Value::Null,
        "from": format!("0x{}", to_hex(&entry.from)),
        "to": entry.to.as_ref().map(|v| format!("0x{}", to_hex(v))),
        "cumulativeGasUsed": format!("0x{:x}", cumulative_gas_used),
        "gasUsed": format!("0x{:x}", entry.gas_limit),
        "effectiveGasPrice": format!("0x{:x}", effective_gas_price),
        "contractAddress": contract_address,
        "logs": logs,
        "logsBloom": logs_bloom,
        "type": format!("0x{:x}", entry.tx_type),
        "status": serde_json::Value::Null,
        "pending": true,
        "uca_id": entry.uca_id.clone(),
    })
}

pub(super) fn gateway_eth_tx_receipts_pending_with_block_json(
    block_number: u64,
    block_hash: &[u8; 32],
    block_txs: &[GatewayEthTxIndexEntry],
) -> Vec<serde_json::Value> {
    block_txs
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let cumulative_gas_used = gateway_eth_block_cumulative_gas_used(block_txs, idx);
            gateway_eth_tx_receipt_pending_with_block_json(
                entry,
                block_number,
                idx,
                block_hash,
                cumulative_gas_used,
            )
        })
        .collect()
}

pub(super) fn gateway_eth_tx_receipt_pending_query_json_by_hash(
    block_number: u64,
    block_hash: &[u8; 32],
    block_txs: &[GatewayEthTxIndexEntry],
    tx_hash: &[u8; 32],
) -> Option<serde_json::Value> {
    let tx_index = block_txs
        .iter()
        .position(|entry| entry.tx_hash == *tx_hash)?;
    let cumulative_gas_used = gateway_eth_block_cumulative_gas_used(block_txs, tx_index);
    Some(gateway_eth_tx_receipt_pending_with_block_json(
        &block_txs[tx_index],
        block_number,
        tx_index,
        block_hash,
        cumulative_gas_used,
    ))
}

pub(super) fn gateway_eth_effective_gas_price_wei(
    entry: &GatewayEthTxIndexEntry,
    base_fee_per_gas: u64,
) -> u64 {
    if entry.tx_type == 2 || entry.tx_type == 3 {
        // tx.gas_price stores maxFeePerGas for EIP-1559 style txs.
        let priority_fee_per_gas = gateway_eth_default_max_priority_fee_per_gas_wei(entry.chain_id);
        let candidate = base_fee_per_gas.saturating_add(priority_fee_per_gas);
        let capped = entry.gas_price.min(candidate);
        capped.max(base_fee_per_gas)
    } else {
        entry.gas_price
    }
}

fn gateway_eth_receipt_log_address(entry: &GatewayEthTxIndexEntry) -> &[u8] {
    entry.to.as_deref().unwrap_or(entry.from.as_slice())
}

fn gateway_eth_bloom_insert_data(bloom: &mut [u8; 256], raw: &[u8]) {
    let hash: [u8; 32] = Keccak256::digest(raw).into();
    for idx in 0..3 {
        let bit = (u16::from(hash[idx * 2]) << 8 | u16::from(hash[idx * 2 + 1])) & 2047;
        let byte_index = 255usize.saturating_sub((bit / 8) as usize);
        let mask = 1u8 << (bit % 8);
        bloom[byte_index] |= mask;
    }
}

fn gateway_eth_receipt_logs_json(
    entry: &GatewayEthTxIndexEntry,
    block_number: Option<u64>,
    tx_index: Option<usize>,
    block_hash: Option<&[u8; 32]>,
) -> Vec<serde_json::Value> {
    let log_index = tx_index.map(|value| format!("0x{:x}", value));
    let tx_index_hex = tx_index.map(|value| format!("0x{:x}", value));
    let block_number_hex = block_number.map(|value| format!("0x{:x}", value));
    let block_hash_hex = block_hash.map(|value| format!("0x{}", to_hex(value)));
    vec![serde_json::json!({
        "removed": false,
        "logIndex": log_index,
        "transactionIndex": tx_index_hex,
        "transactionHash": format!("0x{}", to_hex(&entry.tx_hash)),
        "blockHash": block_hash_hex,
        "blockNumber": block_number_hex,
        "address": format!("0x{}", to_hex(gateway_eth_receipt_log_address(entry))),
        "data": format!("0x{}", to_hex(&entry.input)),
        "topics": [format!("0x{}", to_hex(&entry.tx_hash))],
    })]
}

fn gateway_eth_receipt_logs_bloom_hex(entry: &GatewayEthTxIndexEntry) -> String {
    let mut bloom = [0u8; 256];
    gateway_eth_bloom_insert_data(&mut bloom, gateway_eth_receipt_log_address(entry));
    gateway_eth_bloom_insert_data(&mut bloom, &entry.tx_hash);
    format!("0x{}", to_hex(&bloom))
}

pub(super) fn gateway_eth_tx_receipt_query_json(
    entry: &GatewayEthTxIndexEntry,
    eth_tx_index: &HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
) -> Result<serde_json::Value> {
    let chain_entries = collect_gateway_eth_chain_entries(
        eth_tx_index,
        eth_tx_index_store,
        entry.chain_id,
        gateway_eth_query_scan_max(),
    )?;
    let blocks = gateway_eth_group_entries_by_block(chain_entries);
    if let Some(block_txs) = blocks.get(&entry.nonce) {
        let mut sorted = block_txs.clone();
        sort_gateway_eth_block_txs(&mut sorted);
        if let Some(tx_index) = sorted.iter().position(|tx| tx.tx_hash == entry.tx_hash) {
            let block_hash = gateway_eth_block_hash_for_txs(entry.chain_id, entry.nonce, &sorted);
            let cumulative_gas_used = gateway_eth_block_cumulative_gas_used(&sorted, tx_index);
            return Ok(gateway_eth_tx_receipt_with_block_json(
                entry,
                entry.nonce,
                tx_index,
                &block_hash,
                cumulative_gas_used,
            ));
        }
    }
    let precise_block_txs = collect_gateway_eth_block_entries_precise(
        eth_tx_index,
        eth_tx_index_store,
        entry.chain_id,
        entry.nonce,
        gateway_eth_query_scan_max(),
    )?;
    if !precise_block_txs.is_empty() {
        let mut sorted = precise_block_txs;
        sort_gateway_eth_block_txs(&mut sorted);
        if let Some(tx_index) = sorted.iter().position(|tx| tx.tx_hash == entry.tx_hash) {
            let block_hash = gateway_eth_block_hash_for_txs(entry.chain_id, entry.nonce, &sorted);
            let cumulative_gas_used = gateway_eth_block_cumulative_gas_used(&sorted, tx_index);
            return Ok(gateway_eth_tx_receipt_with_block_json(
                entry,
                entry.nonce,
                tx_index,
                &block_hash,
                cumulative_gas_used,
            ));
        }
    }
    Ok(gateway_eth_tx_receipt_confirmed_without_position_json(
        entry,
    ))
}

pub(super) fn gateway_eth_block_cumulative_gas_used(
    block_txs: &[GatewayEthTxIndexEntry],
    tx_index: usize,
) -> u128 {
    block_txs
        .iter()
        .take(tx_index.saturating_add(1))
        .fold(0u128, |acc, tx| acc.saturating_add(tx.gas_limit as u128))
}
