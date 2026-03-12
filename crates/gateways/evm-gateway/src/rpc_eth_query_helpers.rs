use super::*;

pub(super) fn parse_eth_fee_history_block_count(params: &serde_json::Value) -> Option<u64> {
    if let Some(v) = param_as_u64(params, "block_count")
        .or_else(|| param_as_u64(params, "blockCount"))
        .or_else(|| param_as_u64(params, "count"))
    {
        return Some(v);
    }
    non_object_param_at(params, 0).and_then(value_to_u64)
}

pub(super) fn parse_eth_fee_history_newest_block_tag(params: &serde_json::Value) -> Option<String> {
    if let Some(v) = param_as_string(params, "newest_block")
        .or_else(|| param_as_string(params, "newestBlock"))
        .or_else(|| param_as_string(params, "newest"))
        .or_else(|| param_as_string(params, "block"))
    {
        return Some(v);
    }
    non_object_param_at(params, 1).and_then(value_to_string)
}

pub(super) fn parse_eth_fee_history_newest_block_number(
    tag: &str,
    latest: u64,
    pending_block_number: Option<u64>,
) -> Result<Option<u64>> {
    let normalized = tag.trim().trim_matches('"');
    if normalized.eq_ignore_ascii_case("pending") {
        return Ok(Some(pending_block_number.unwrap_or(latest)));
    }
    parse_eth_block_number_from_tag(tag, latest)
}

pub(super) fn value_to_f64(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

pub(super) fn parse_eth_fee_history_reward_percentiles(
    params: &serde_json::Value,
) -> Result<Option<Vec<f64>>> {
    let from_object = params_primary_object(params).and_then(|map| {
        map.get("rewardPercentiles")
            .or_else(|| map.get("reward_percentiles"))
    });
    let from_array = params
        .as_array()
        .and_then(|arr| arr.iter().rev().find(|v| !v.is_object() && v.is_array()));
    let Some(raw) = from_object.or(from_array) else {
        return Ok(None);
    };
    let Some(items) = raw.as_array() else {
        bail!("rewardPercentiles must be number[]");
    };
    let mut out = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        let value = value_to_f64(item)
            .ok_or_else(|| anyhow::anyhow!("rewardPercentiles[{}] must be number", idx))?;
        if !(0.0..=100.0).contains(&value) {
            bail!("rewardPercentiles[{}] out of range [0,100]", idx);
        }
        out.push(value);
    }
    Ok(Some(out))
}

pub(super) fn gateway_eth_fee_history_block_gas_limit() -> u128 {
    u64_env("NOVOVM_GATEWAY_ETH_BLOCK_GAS_LIMIT", 30_000_000) as u128
}

pub(super) fn gateway_eth_fee_history_reward_row_hex(
    txs: &[GatewayEthTxIndexEntry],
    percentiles: &[f64],
    default_priority_fee: u128,
) -> Vec<String> {
    if txs.is_empty() {
        return percentiles
            .iter()
            .map(|_| format!("0x{:x}", default_priority_fee))
            .collect();
    }
    let mut gas_prices: Vec<u128> = txs.iter().map(|tx| tx.gas_price as u128).collect();
    gas_prices.sort_unstable();
    percentiles
        .iter()
        .map(|percentile| {
            let idx =
                (((*percentile / 100.0) * (gas_prices.len().saturating_sub(1) as f64)).round()
                    as usize)
                    .min(gas_prices.len().saturating_sub(1));
            format!("0x{:x}", gas_prices[idx])
        })
        .collect()
}

pub(super) fn resolve_gateway_eth_block_txs(
    chain_id: u64,
    params: &serde_json::Value,
    entries: Vec<GatewayEthTxIndexEntry>,
) -> Result<Option<GatewayResolvedBlock>> {
    if entries.is_empty() {
        return Ok(None);
    }
    let blocks = gateway_eth_group_entries_by_block(entries);
    if let Some(block_hash) = parse_eth_block_hash_from_params(params)? {
        for (block_number, block_txs) in blocks {
            let candidate = gateway_eth_block_hash_for_txs(chain_id, block_number, &block_txs);
            if candidate == block_hash {
                return Ok(Some((block_number, candidate, block_txs)));
            }
        }
        return Ok(None);
    }
    let latest = blocks.keys().next_back().copied().unwrap_or(0);
    let block_tag = parse_eth_block_query_tag(params).unwrap_or_else(|| "latest".to_string());
    let Some(block_number) = parse_eth_block_number_from_tag(&block_tag, latest)? else {
        return Ok(None);
    };
    let Some(block_txs) = blocks.get(&block_number) else {
        return Ok(None);
    };
    let block_hash = gateway_eth_block_hash_for_txs(chain_id, block_number, block_txs);
    Ok(Some((block_number, block_hash, block_txs.clone())))
}

pub(super) fn parse_eth_block_query_tag(params: &serde_json::Value) -> Option<String> {
    let from_object = params_object_with_any_keys(
        params,
        &["block_number", "blockNumber", "number", "block", "tag"],
    )
    .and_then(|map| {
        ["block_number", "blockNumber", "number", "block", "tag"]
            .iter()
            .find_map(|key| map.get(*key))
            .map(|v| match v {
                serde_json::Value::String(s) => s.trim().to_string(),
                serde_json::Value::Number(_) => v.to_string(),
                other => other.to_string(),
            })
    });
    if from_object.is_some() {
        return from_object;
    }
    params.as_array().and_then(|arr| {
        arr.iter().find_map(|v| {
            if v.is_object() || v.is_array() {
                return None;
            }
            value_to_string(v).and_then(|text| {
                if is_block_tag_candidate(&text) {
                    Some(text)
                } else {
                    None
                }
            })
        })
    })
}

pub(super) fn parse_eth_block_query_full_transactions(params: &serde_json::Value) -> bool {
    if let Some(v) = param_as_bool(params, "full_transactions")
        .or_else(|| param_as_bool(params, "fullTransactions"))
        .or_else(|| param_as_bool(params, "full"))
    {
        return v;
    }
    params
        .as_array()
        .and_then(|arr| {
            arr.iter().rev().find_map(|v| match v {
                serde_json::Value::Bool(value) => Some(*value),
                _ => None,
            })
        })
        .unwrap_or(false)
}

pub(super) fn parse_eth_block_query_tx_index(params: &serde_json::Value) -> Option<u64> {
    if let Some(map) = params_primary_object(params) {
        let from_object = [
            "transaction_index",
            "transactionIndex",
            "index",
            "tx_index",
            "txIndex",
        ]
        .iter()
        .find_map(|key| map.get(*key))
        .and_then(value_to_u64);
        if from_object.is_some() {
            return from_object;
        }
    }
    params.as_array().and_then(|arr| {
        arr.iter()
            .rev()
            .find_map(|v| if v.is_object() { None } else { value_to_u64(v) })
    })
}

pub(super) fn parse_eth_tx_count_block_tag(params: &serde_json::Value) -> Option<String> {
    if let Some(map) = params_object_with_any_keys(
        params,
        &[
            "block",
            "tag",
            "block_tag",
            "blockTag",
            "default_block",
            "defaultBlock",
        ],
    ) {
        let from_object = [
            "block",
            "tag",
            "block_tag",
            "blockTag",
            "default_block",
            "defaultBlock",
        ]
        .iter()
        .find_map(|key| map.get(*key))
        .and_then(value_to_string);
        if from_object.is_some() {
            return from_object;
        }
    }
    if let Some(tag_like) = last_block_tag_like_param_string(params) {
        return Some(tag_like);
    }
    non_object_param_at(params, 1).and_then(value_to_string)
}

pub(super) fn parse_eth_access_list_intrinsic_counts(
    params: &serde_json::Value,
) -> Result<(u64, u64)> {
    let access_list_value = params_object_with_any_keys(params, &["accessList", "access_list"])
        .and_then(|map| map.get("accessList").or_else(|| map.get("access_list")))
        .or_else(|| {
            param_tx_object(params).and_then(|tx| {
                tx.as_object()
                    .and_then(|map| map.get("accessList").or_else(|| map.get("access_list")))
            })
        });
    let Some(access_list_value) = access_list_value else {
        return Ok((0, 0));
    };
    let Some(access_list) = access_list_value.as_array() else {
        bail!("accessList must be array");
    };
    let mut address_count = 0u64;
    let mut storage_key_count = 0u64;
    for (entry_idx, item) in access_list.iter().enumerate() {
        let Some(item_map) = item.as_object() else {
            bail!("accessList[{}] must be object", entry_idx);
        };
        let address_raw = item_map
            .get("address")
            .and_then(value_to_string)
            .ok_or_else(|| anyhow::anyhow!("accessList[{}].address is required", entry_idx))?;
        let address = decode_hex_bytes(&address_raw, "accessList.address")?;
        if address.len() != 20 {
            bail!(
                "accessList[{}].address must be 20 bytes hex, got {}",
                entry_idx,
                address.len()
            );
        }
        address_count = address_count.saturating_add(1);

        let Some(storage_keys_value) = item_map
            .get("storageKeys")
            .or_else(|| item_map.get("storage_keys"))
        else {
            continue;
        };
        let Some(storage_keys) = storage_keys_value.as_array() else {
            bail!("accessList[{}].storageKeys must be string[]", entry_idx);
        };
        for (key_idx, key) in storage_keys.iter().enumerate() {
            let key_raw = value_to_string(key).ok_or_else(|| {
                anyhow::anyhow!(
                    "accessList[{}].storageKeys[{}] must be hex string",
                    entry_idx,
                    key_idx
                )
            })?;
            let decoded = decode_hex_bytes(&key_raw, "accessList.storageKeys")?;
            if decoded.len() != 32 {
                bail!(
                    "accessList[{}].storageKeys[{}] must be 32 bytes hex, got {}",
                    entry_idx,
                    key_idx,
                    decoded.len()
                );
            }
            storage_key_count = storage_key_count.saturating_add(1);
        }
    }
    Ok((address_count, storage_key_count))
}

pub(super) fn parse_eth_block_number_from_tag(tag: &str, latest: u64) -> Result<Option<u64>> {
    let normalized = tag.trim().trim_matches('"');
    if normalized.is_empty()
        || normalized.eq_ignore_ascii_case("latest")
        || normalized.eq_ignore_ascii_case("safe")
        || normalized.eq_ignore_ascii_case("finalized")
    {
        return Ok(Some(latest));
    }
    if normalized.eq_ignore_ascii_case("pending") {
        return Ok(Some(latest));
    }
    if normalized.eq_ignore_ascii_case("earliest") {
        return Ok(Some(0));
    }
    if let Some(number) = parse_u64_decimal_or_hex(normalized) {
        return Ok(Some(number));
    }
    bail!("invalid block number/tag: {}", tag)
}

pub(super) fn gateway_eth_block_hash_for_txs(
    chain_id: u64,
    block_number: u64,
    txs: &[GatewayEthTxIndexEntry],
) -> [u8; 32] {
    let mut tx_hashes: Vec<[u8; 32]> = txs.iter().map(|item| item.tx_hash).collect();
    tx_hashes.sort();
    gateway_eth_pseudo_block_hash(chain_id, block_number, &tx_hashes)
}

pub(super) fn gateway_eth_pseudo_block_hash(
    chain_id: u64,
    block_number: u64,
    tx_hashes: &[[u8; 32]],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"gateway-evm-pseudo-block-v1");
    hasher.update(chain_id.to_le_bytes());
    hasher.update(block_number.to_le_bytes());
    for tx_hash in tx_hashes {
        hasher.update(tx_hash);
    }
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest[..32]);
    out
}

pub(super) fn gateway_eth_pseudo_block_root_from_sorted_txs(
    domain: &[u8],
    chain_id: u64,
    block_number: u64,
    sorted_txs: &[GatewayEthTxIndexEntry],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(chain_id.to_le_bytes());
    hasher.update(block_number.to_le_bytes());
    for tx in sorted_txs {
        hasher.update(tx.tx_hash);
        hasher.update(tx.tx_type.to_le_bytes());
        hasher.update(tx.gas_limit.to_le_bytes());
        hasher.update(tx.gas_price.to_le_bytes());
    }
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest[..32]);
    out
}

pub(super) fn gateway_eth_block_by_number_json(
    chain_id: u64,
    block_number: u64,
    txs: &[GatewayEthTxIndexEntry],
    full_transactions: bool,
    is_pending_block: bool,
) -> serde_json::Value {
    let mut sorted_txs = txs.to_vec();
    sort_gateway_eth_block_txs(&mut sorted_txs);
    let block_hash = gateway_eth_block_hash_for_txs(chain_id, block_number, &sorted_txs);
    let transactions_root = gateway_eth_pseudo_block_root_from_sorted_txs(
        b"gateway-evm-transactions-root-v1",
        chain_id,
        block_number,
        &sorted_txs,
    );
    let receipts_root = gateway_eth_pseudo_block_root_from_sorted_txs(
        b"gateway-evm-receipts-root-v1",
        chain_id,
        block_number,
        &sorted_txs,
    );
    let state_root = gateway_eth_pseudo_block_root_from_sorted_txs(
        b"gateway-evm-state-root-v1",
        chain_id,
        block_number,
        &sorted_txs,
    );
    let gas_used = sorted_txs
        .iter()
        .fold(0u128, |acc, tx| acc.saturating_add(tx.gas_limit as u128));
    let gas_limit = gateway_eth_fee_history_block_gas_limit();
    let parent_hash = if block_number == 0 {
        [0u8; 32]
    } else {
        gateway_eth_pseudo_block_hash(chain_id, block_number.saturating_sub(1), &[])
    };
    let base_fee_per_gas = gateway_eth_base_fee_per_gas_wei();
    let txs_json: Vec<serde_json::Value> = if full_transactions {
        sorted_txs
            .iter()
            .enumerate()
            .map(|(idx, tx)| {
                if is_pending_block {
                    gateway_eth_tx_pending_with_block_json(tx, block_number, idx, &block_hash)
                } else {
                    gateway_eth_tx_with_block_json(tx, block_number, idx, &block_hash)
                }
            })
            .collect()
    } else {
        sorted_txs
            .iter()
            .map(|tx| serde_json::Value::String(format!("0x{}", to_hex(&tx.tx_hash))))
            .collect()
    };
    serde_json::json!({
        "number": format!("0x{:x}", block_number),
        "hash": format!("0x{}", to_hex(&block_hash)),
        "parentHash": format!("0x{}", to_hex(&parent_hash)),
        "nonce": "0x0",
        "sha3Uncles": GATEWAY_ETH_EMPTY_UNCLES_HASH,
        "logsBloom": gateway_eth_empty_logs_bloom_hex(),
        "transactionsRoot": format!("0x{}", to_hex(&transactions_root)),
        "stateRoot": format!("0x{}", to_hex(&state_root)),
        "receiptsRoot": format!("0x{}", to_hex(&receipts_root)),
        "miner": "0x0000000000000000000000000000000000000000",
        "difficulty": "0x0",
        "totalDifficulty": "0x0",
        "extraData": "0x",
        "size": format!("0x{:x}", sorted_txs.len()),
        "gasLimit": format!("0x{:x}", gas_limit),
        "gasUsed": format!("0x{:x}", gas_used),
        "timestamp": format!("0x{:x}", now_unix_sec()),
        "transactions": txs_json,
        "uncles": [],
        "baseFeePerGas": format!("0x{:x}", base_fee_per_gas),
        "chainId": format!("0x{:x}", chain_id),
    })
}

pub(super) fn gateway_eth_base_fee_per_gas_wei() -> u64 {
    u64_env(
        "NOVOVM_GATEWAY_ETH_DEFAULT_BASE_FEE_PER_GAS",
        u64_env("NOVOVM_GATEWAY_ETH_DEFAULT_GAS_PRICE", 1),
    )
}
