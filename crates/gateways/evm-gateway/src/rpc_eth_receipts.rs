use super::*;

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
}

pub(super) fn gateway_eth_default_max_priority_fee_per_gas_wei() -> u64 {
    u64_env(
        "NOVOVM_GATEWAY_ETH_DEFAULT_MAX_PRIORITY_FEE_PER_GAS",
        u64_env("NOVOVM_GATEWAY_ETH_DEFAULT_GAS_PRICE", 1),
    )
}

pub(super) fn gateway_eth_type2_fee_fields_json(
    max_fee_per_gas: u64,
) -> (serde_json::Value, serde_json::Value) {
    let max_priority_fee_per_gas =
        gateway_eth_default_max_priority_fee_per_gas_wei().min(max_fee_per_gas);
    (
        serde_json::Value::String(format!("0x{:x}", max_fee_per_gas)),
        serde_json::Value::String(format!("0x{:x}", max_priority_fee_per_gas)),
    )
}

pub(super) fn gateway_eth_tx_fee_fields_json_from_entry(
    entry: &GatewayEthTxIndexEntry,
) -> (serde_json::Value, serde_json::Value) {
    if entry.tx_type == 2 {
        gateway_eth_type2_fee_fields_json(entry.gas_price)
    } else {
        (serde_json::Value::Null, serde_json::Value::Null)
    }
}

pub(super) fn gateway_eth_tx_fee_fields_json_from_ir(
    tx: &TxIR,
) -> (serde_json::Value, serde_json::Value) {
    let is_type2 = resolve_raw_evm_tx_route_hint_m0(&tx.signature)
        .map(|hint| hint.envelope == EvmRawTxEnvelopeType::Type2DynamicFee)
        .unwrap_or(false);
    if is_type2 {
        gateway_eth_type2_fee_fields_json(tx.gas_price)
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
    let effective_gas_price =
        gateway_eth_effective_gas_price_wei(entry, gateway_eth_base_fee_per_gas_wei());
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
        "logs": [],
        "logsBloom": gateway_eth_empty_logs_bloom_hex(),
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
    let effective_gas_price =
        gateway_eth_effective_gas_price_wei(entry, gateway_eth_base_fee_per_gas_wei());
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
        "logs": [],
        "logsBloom": gateway_eth_empty_logs_bloom_hex(),
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
    let effective_gas_price =
        gateway_eth_effective_gas_price_wei(entry, gateway_eth_base_fee_per_gas_wei());
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
        "logs": [],
        "logsBloom": gateway_eth_empty_logs_bloom_hex(),
        "type": format!("0x{:x}", entry.tx_type),
        "status": "0x1",
        "pending": false,
        "uca_id": entry.uca_id.clone(),
    })
}

pub(super) fn gateway_eth_tx_receipt_pending_with_block_json(
    entry: &GatewayEthTxIndexEntry,
    block_number: u64,
    tx_index: usize,
    block_hash: &[u8; 32],
    cumulative_gas_used: u128,
) -> serde_json::Value {
    let contract_address = gateway_eth_contract_address_hex(entry);
    let effective_gas_price =
        gateway_eth_effective_gas_price_wei(entry, gateway_eth_base_fee_per_gas_wei());
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
        "logs": [],
        "logsBloom": gateway_eth_empty_logs_bloom_hex(),
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
    if entry.tx_type == 2 {
        entry.gas_price.max(base_fee_per_gas)
    } else {
        entry.gas_price
    }
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
