use super::*;

pub(super) fn gateway_eth_tx_ir_with_hash(mut tx: TxIR) -> TxIR {
    if tx.hash.is_empty() {
        tx.compute_hash();
    }
    tx
}

pub(super) fn gateway_eth_txpool_tx_json_from_ir(tx: &TxIR) -> serde_json::Value {
    let normalized = gateway_eth_tx_ir_with_hash(tx.clone());
    let (max_fee_per_gas, max_priority_fee_per_gas) =
        gateway_eth_tx_fee_fields_json_from_ir(&normalized);
    serde_json::json!({
        "hash": format!("0x{}", to_hex(&normalized.hash)),
        "nonce": format!("0x{:x}", normalized.nonce),
        "from": format!("0x{}", to_hex(&normalized.from)),
        "to": normalized
            .to
            .as_ref()
            .map(|to| serde_json::Value::String(format!("0x{}", to_hex(to))))
            .unwrap_or(serde_json::Value::Null),
        "value": format!("0x{:x}", normalized.value),
        "gas": format!("0x{:x}", normalized.gas_limit),
        "gasPrice": format!("0x{:x}", normalized.gas_price),
        "maxFeePerGas": max_fee_per_gas,
        "maxPriorityFeePerGas": max_priority_fee_per_gas,
        "input": format!("0x{}", to_hex(&normalized.data)),
        "type": format!("0x{:x}", gateway_eth_tx_type_number_from_ir(normalized.tx_type)),
        "chainId": format!("0x{:x}", normalized.chain_id),
        "blockHash": serde_json::Value::Null,
        "blockNumber": serde_json::Value::Null,
        "transactionIndex": serde_json::Value::Null,
    })
}

pub(super) fn gateway_eth_pending_tx_by_hash_json_from_ir(tx: &TxIR) -> serde_json::Value {
    let normalized = gateway_eth_tx_ir_with_hash(tx.clone());
    let (max_fee_per_gas, max_priority_fee_per_gas) =
        gateway_eth_tx_fee_fields_json_from_ir(&normalized);
    serde_json::json!({
        "hash": format!("0x{}", to_hex(&normalized.hash)),
        "nonce": format!("0x{:x}", normalized.nonce),
        "blockHash": serde_json::Value::Null,
        "blockNumber": serde_json::Value::Null,
        "transactionIndex": serde_json::Value::Null,
        "from": format!("0x{}", to_hex(&normalized.from)),
        "to": normalized
            .to
            .as_ref()
            .map(|to| serde_json::Value::String(format!("0x{}", to_hex(to))))
            .unwrap_or(serde_json::Value::Null),
        "value": format!("0x{:x}", normalized.value),
        "gas": format!("0x{:x}", normalized.gas_limit),
        "gasPrice": format!("0x{:x}", normalized.gas_price),
        "maxFeePerGas": max_fee_per_gas,
        "maxPriorityFeePerGas": max_priority_fee_per_gas,
        "input": format!("0x{}", to_hex(&normalized.data)),
        "chainId": format!("0x{:x}", normalized.chain_id),
        "type": format!("0x{:x}", gateway_eth_tx_type_number_from_ir(normalized.tx_type)),
        "pending": true,
        "uca_id": serde_json::Value::Null,
    })
}

pub(super) fn gateway_eth_tx_index_entry_from_ir(tx: TxIR) -> GatewayEthTxIndexEntry {
    let normalized = gateway_eth_tx_ir_with_hash(tx);
    let mut tx_hash = [0u8; 32];
    if normalized.hash.len() == 32 {
        tx_hash.copy_from_slice(&normalized.hash);
    }
    GatewayEthTxIndexEntry {
        tx_hash,
        uca_id: String::new(),
        chain_id: normalized.chain_id,
        nonce: normalized.nonce,
        tx_type: gateway_eth_tx_type_number_from_ir(normalized.tx_type),
        from: normalized.from,
        to: normalized.to,
        value: normalized.value,
        gas_limit: normalized.gas_limit,
        gas_price: normalized.gas_price,
        input: normalized.data,
    }
}

pub(super) fn gateway_eth_tx_ir_from_index_entry(entry: &GatewayEthTxIndexEntry) -> TxIR {
    let tx_type = if entry.to.is_none() {
        TxType::ContractDeploy
    } else if entry.input.is_empty() {
        TxType::Transfer
    } else {
        TxType::ContractCall
    };
    TxIR {
        hash: entry.tx_hash.to_vec(),
        from: entry.from.clone(),
        to: entry.to.clone(),
        value: entry.value,
        gas_limit: entry.gas_limit,
        gas_price: entry.gas_price,
        nonce: entry.nonce,
        data: entry.input.clone(),
        signature: Vec::new(),
        chain_id: entry.chain_id,
        tx_type,
        source_chain: None,
        target_chain: None,
    }
}

pub(super) fn collect_gateway_eth_txpool_runtime_txs(chain_id: u64) -> (Vec<TxIR>, Vec<TxIR>) {
    let max_items = gateway_eth_query_scan_max().max(1);
    let executable = snapshot_executable_ingress_frames_for_host(max_items)
        .into_iter()
        .filter(|frame| frame.chain_id == chain_id)
        .filter_map(|frame| frame.parsed_tx.map(gateway_eth_tx_ir_with_hash))
        .collect::<Vec<TxIR>>();
    let queued = snapshot_pending_sender_buckets_for_host(max_items, max_items)
        .into_iter()
        .filter(|bucket| bucket.chain_id == chain_id)
        .flat_map(|bucket| bucket.txs.into_iter().map(gateway_eth_tx_ir_with_hash))
        .collect::<Vec<TxIR>>();
    (executable, queued)
}

pub(super) fn collect_gateway_eth_pending_hashes_runtime(chain_id: u64) -> BTreeSet<[u8; 32]> {
    let (pending_txs, queued_txs) = collect_gateway_eth_txpool_runtime_txs(chain_id);
    pending_txs
        .into_iter()
        .chain(queued_txs)
        .filter_map(|tx| {
            let normalized = gateway_eth_tx_ir_with_hash(tx);
            if normalized.hash.len() != 32 {
                return None;
            }
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&normalized.hash);
            Some(hash)
        })
        .collect::<BTreeSet<[u8; 32]>>()
}

pub(super) fn find_gateway_eth_runtime_tx_by_hash(
    tx_hash: [u8; 32],
    chain_hint: Option<u64>,
) -> Option<TxIR> {
    let max_items = gateway_eth_query_scan_max().max(1);
    for frame in snapshot_executable_ingress_frames_for_host(max_items) {
        if chain_hint.is_some_and(|chain_id| frame.chain_id != chain_id) {
            continue;
        }
        let Some(tx) = frame.parsed_tx else {
            continue;
        };
        let normalized = gateway_eth_tx_ir_with_hash(tx);
        if normalized.hash.as_slice() == tx_hash {
            return Some(normalized);
        }
    }
    for bucket in snapshot_pending_sender_buckets_for_host(max_items, max_items) {
        if chain_hint.is_some_and(|chain_id| bucket.chain_id != chain_id) {
            continue;
        }
        for tx in bucket.txs {
            let normalized = gateway_eth_tx_ir_with_hash(tx);
            if normalized.hash.as_slice() == tx_hash {
                return Some(normalized);
            }
        }
    }
    None
}

pub(super) fn gateway_eth_pending_nonce_from_runtime(chain_id: u64, address: &[u8]) -> Option<u64> {
    let (pending_txs, queued_txs) = collect_gateway_eth_txpool_runtime_txs(chain_id);
    pending_txs
        .into_iter()
        .chain(queued_txs)
        .filter(|tx| tx.from.as_slice() == address)
        .map(|tx| tx.nonce.saturating_add(1))
        .max()
}

pub(super) fn gateway_eth_pending_block_from_runtime(
    chain_id: u64,
    latest_block_number: u64,
    allow_empty: bool,
) -> Option<GatewayResolvedBlock> {
    let (pending_txs, queued_txs) = collect_gateway_eth_txpool_runtime_txs(chain_id);
    if pending_txs.is_empty() && queued_txs.is_empty() && !allow_empty {
        return None;
    }
    let mut pending_entries = pending_txs
        .into_iter()
        .chain(queued_txs)
        .map(gateway_eth_tx_index_entry_from_ir)
        .collect::<Vec<GatewayEthTxIndexEntry>>();
    sort_gateway_eth_block_txs(&mut pending_entries);
    let block_number = latest_block_number.saturating_add(1);
    let block_hash = gateway_eth_block_hash_for_txs(chain_id, block_number, &pending_entries);
    Some((block_number, block_hash, pending_entries))
}

pub(super) fn build_gateway_eth_txpool_content_from_ir(
    pending_txs: Vec<TxIR>,
    queued_txs: Vec<TxIR>,
) -> serde_json::Value {
    let mut pending: BTreeMap<String, BTreeMap<String, serde_json::Value>> = BTreeMap::new();
    for tx in pending_txs {
        let from = format!("0x{}", to_hex(&tx.from));
        let nonce = format!("0x{:x}", tx.nonce);
        pending
            .entry(from)
            .or_default()
            .insert(nonce, gateway_eth_txpool_tx_json_from_ir(&tx));
    }
    let mut queued: BTreeMap<String, BTreeMap<String, serde_json::Value>> = BTreeMap::new();
    for tx in queued_txs {
        let from = format!("0x{}", to_hex(&tx.from));
        let nonce = format!("0x{:x}", tx.nonce);
        queued
            .entry(from)
            .or_default()
            .insert(nonce, gateway_eth_txpool_tx_json_from_ir(&tx));
    }
    let pending_json = pending
        .into_iter()
        .map(|(addr, by_nonce)| {
            let nonce_json = by_nonce
                .into_iter()
                .collect::<serde_json::Map<String, serde_json::Value>>();
            (addr, serde_json::Value::Object(nonce_json))
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    let queued_json = queued
        .into_iter()
        .map(|(addr, by_nonce)| {
            let nonce_json = by_nonce
                .into_iter()
                .collect::<serde_json::Map<String, serde_json::Value>>();
            (addr, serde_json::Value::Object(nonce_json))
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    serde_json::json!({
        "pending": pending_json,
        "queued": queued_json,
    })
}

pub(super) fn build_gateway_eth_txpool_content_from_ir_for_sender(
    pending_txs: Vec<TxIR>,
    queued_txs: Vec<TxIR>,
    sender: &[u8],
) -> serde_json::Value {
    let mut pending = BTreeMap::<String, serde_json::Value>::new();
    for tx in pending_txs {
        if tx.from.as_slice() != sender {
            continue;
        }
        let nonce = format!("0x{:x}", tx.nonce);
        pending.insert(nonce, gateway_eth_txpool_tx_json_from_ir(&tx));
    }
    let mut queued = BTreeMap::<String, serde_json::Value>::new();
    for tx in queued_txs {
        if tx.from.as_slice() != sender {
            continue;
        }
        let nonce = format!("0x{:x}", tx.nonce);
        queued.insert(nonce, gateway_eth_txpool_tx_json_from_ir(&tx));
    }
    serde_json::json!({
        "pending": pending.into_iter().collect::<serde_json::Map<String, serde_json::Value>>(),
        "queued": queued.into_iter().collect::<serde_json::Map<String, serde_json::Value>>(),
    })
}

pub(super) fn build_gateway_eth_txpool_inspect_from_ir(
    pending_txs: Vec<TxIR>,
    queued_txs: Vec<TxIR>,
) -> serde_json::Value {
    let mut pending: BTreeMap<String, BTreeMap<String, serde_json::Value>> = BTreeMap::new();
    for tx in pending_txs {
        let from = format!("0x{}", to_hex(&tx.from));
        let nonce = format!("0x{:x}", tx.nonce);
        let to_label = match tx.to.as_ref() {
            Some(to) if !to.is_empty() => format!("0x{}", to_hex(to)),
            _ => "contract_creation".to_string(),
        };
        let summary = format!(
            "{}: {} wei + {} gas x {} wei",
            to_label, tx.value, tx.gas_limit, tx.gas_price
        );
        pending
            .entry(from)
            .or_default()
            .insert(nonce, serde_json::Value::String(summary));
    }
    let mut queued: BTreeMap<String, BTreeMap<String, serde_json::Value>> = BTreeMap::new();
    for tx in queued_txs {
        let from = format!("0x{}", to_hex(&tx.from));
        let nonce = format!("0x{:x}", tx.nonce);
        let to_label = match tx.to.as_ref() {
            Some(to) if !to.is_empty() => format!("0x{}", to_hex(to)),
            _ => "contract_creation".to_string(),
        };
        let summary = format!(
            "{}: {} wei + {} gas x {} wei",
            to_label, tx.value, tx.gas_limit, tx.gas_price
        );
        queued
            .entry(from)
            .or_default()
            .insert(nonce, serde_json::Value::String(summary));
    }
    let pending_json = pending
        .into_iter()
        .map(|(addr, by_nonce)| {
            let nonce_json = by_nonce
                .into_iter()
                .collect::<serde_json::Map<String, serde_json::Value>>();
            (addr, serde_json::Value::Object(nonce_json))
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    let queued_json = queued
        .into_iter()
        .map(|(addr, by_nonce)| {
            let nonce_json = by_nonce
                .into_iter()
                .collect::<serde_json::Map<String, serde_json::Value>>();
            (addr, serde_json::Value::Object(nonce_json))
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    serde_json::json!({
        "pending": pending_json,
        "queued": queued_json,
    })
}

pub(super) fn build_gateway_eth_txpool_inspect_from_ir_for_sender(
    pending_txs: Vec<TxIR>,
    queued_txs: Vec<TxIR>,
    sender: &[u8],
) -> serde_json::Value {
    let mut pending = BTreeMap::<String, serde_json::Value>::new();
    for tx in pending_txs {
        if tx.from.as_slice() != sender {
            continue;
        }
        let nonce = format!("0x{:x}", tx.nonce);
        let to_label = match tx.to.as_ref() {
            Some(to) if !to.is_empty() => format!("0x{}", to_hex(to)),
            _ => "contract_creation".to_string(),
        };
        let summary = format!(
            "{}: {} wei + {} gas x {} wei",
            to_label, tx.value, tx.gas_limit, tx.gas_price
        );
        pending.insert(nonce, serde_json::Value::String(summary));
    }
    let mut queued = BTreeMap::<String, serde_json::Value>::new();
    for tx in queued_txs {
        if tx.from.as_slice() != sender {
            continue;
        }
        let nonce = format!("0x{:x}", tx.nonce);
        let to_label = match tx.to.as_ref() {
            Some(to) if !to.is_empty() => format!("0x{}", to_hex(to)),
            _ => "contract_creation".to_string(),
        };
        let summary = format!(
            "{}: {} wei + {} gas x {} wei",
            to_label, tx.value, tx.gas_limit, tx.gas_price
        );
        queued.insert(nonce, serde_json::Value::String(summary));
    }
    serde_json::json!({
        "pending": pending.into_iter().collect::<serde_json::Map<String, serde_json::Value>>(),
        "queued": queued.into_iter().collect::<serde_json::Map<String, serde_json::Value>>(),
    })
}

pub(super) fn build_gateway_eth_txpool_status_from_ir(
    pending_txs: &[TxIR],
    queued_txs: &[TxIR],
) -> serde_json::Value {
    serde_json::json!({
        "pending": format!("0x{:x}", pending_txs.len()),
        "queued": format!("0x{:x}", queued_txs.len()),
    })
}

pub(super) fn build_gateway_eth_txpool_status_from_ir_for_sender(
    pending_txs: &[TxIR],
    queued_txs: &[TxIR],
    sender: &[u8],
) -> serde_json::Value {
    let pending_count = pending_txs
        .iter()
        .filter(|tx| tx.from.as_slice() == sender)
        .count();
    let queued_count = queued_txs
        .iter()
        .filter(|tx| tx.from.as_slice() == sender)
        .count();
    serde_json::json!({
        "pending": format!("0x{:x}", pending_count),
        "queued": format!("0x{:x}", queued_count),
    })
}

pub(super) fn build_gateway_eth_txpool_content(
    entries: Vec<GatewayEthTxIndexEntry>,
) -> serde_json::Value {
    let mut sorted = entries;
    sort_gateway_eth_block_txs(&mut sorted);
    let mut pending: BTreeMap<String, BTreeMap<String, serde_json::Value>> = BTreeMap::new();
    for entry in sorted {
        let from = format!("0x{}", to_hex(&entry.from));
        let nonce = format!("0x{:x}", entry.nonce);
        pending
            .entry(from)
            .or_default()
            .insert(nonce, gateway_eth_tx_by_hash_json(&entry));
    }
    let pending_json = pending
        .into_iter()
        .map(|(addr, by_nonce)| {
            let nonce_json = by_nonce
                .into_iter()
                .collect::<serde_json::Map<String, serde_json::Value>>();
            (addr, serde_json::Value::Object(nonce_json))
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    serde_json::json!({
        "pending": pending_json,
        "queued": serde_json::json!({}),
    })
}

pub(super) fn build_gateway_eth_txpool_inspect(
    entries: Vec<GatewayEthTxIndexEntry>,
) -> serde_json::Value {
    let mut sorted = entries;
    sort_gateway_eth_block_txs(&mut sorted);
    let mut pending: BTreeMap<String, BTreeMap<String, serde_json::Value>> = BTreeMap::new();
    for entry in sorted {
        let from = format!("0x{}", to_hex(&entry.from));
        let nonce = format!("0x{:x}", entry.nonce);
        let to_label = match entry.to.as_ref() {
            Some(to) if !to.is_empty() => format!("0x{}", to_hex(to)),
            _ => "contract_creation".to_string(),
        };
        let summary = format!(
            "{}: {} wei + {} gas x {} wei",
            to_label, entry.value, entry.gas_limit, entry.gas_price
        );
        pending
            .entry(from)
            .or_default()
            .insert(nonce, serde_json::Value::String(summary));
    }
    let pending_json = pending
        .into_iter()
        .map(|(addr, by_nonce)| {
            let nonce_json = by_nonce
                .into_iter()
                .collect::<serde_json::Map<String, serde_json::Value>>();
            (addr, serde_json::Value::Object(nonce_json))
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    serde_json::json!({
        "pending": pending_json,
        "queued": serde_json::json!({}),
    })
}
