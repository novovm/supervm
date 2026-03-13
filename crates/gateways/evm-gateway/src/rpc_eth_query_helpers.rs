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

pub(super) fn parse_eth_blob_intrinsic_fields(params: &serde_json::Value) -> Result<(u64, u64)> {
    let max_fee_per_blob_gas =
        param_as_u64_any_with_tx(params, &["max_fee_per_blob_gas", "maxFeePerBlobGas"])
            .unwrap_or(0);
    const BLOB_HASH_KEYS: &[&str] = &[
        "blobVersionedHashes",
        "blob_versioned_hashes",
        "blobHashes",
        "blob_hashes",
    ];
    let blob_hashes_value = params_object_with_any_keys(params, BLOB_HASH_KEYS)
        .and_then(|map| BLOB_HASH_KEYS.iter().find_map(|key| map.get(*key)))
        .or_else(|| {
            param_tx_object(params).and_then(|tx| {
                tx.as_object()
                    .and_then(|map| BLOB_HASH_KEYS.iter().find_map(|key| map.get(*key)))
            })
        });
    let Some(blob_hashes_value) = blob_hashes_value else {
        return Ok((max_fee_per_blob_gas, 0));
    };
    let Some(blob_hashes) = blob_hashes_value.as_array() else {
        bail!("blobVersionedHashes must be string[]");
    };
    let mut blob_hash_count = 0u64;
    for (idx, item) in blob_hashes.iter().enumerate() {
        let hash_raw = value_to_string(item)
            .ok_or_else(|| anyhow::anyhow!("blobVersionedHashes[{}] must be hex string", idx))?;
        let decoded = decode_hex_bytes(&hash_raw, "blobVersionedHashes")?;
        if decoded.len() != 32 {
            bail!(
                "blobVersionedHashes[{}] must be 32 bytes hex, got {}",
                idx,
                decoded.len()
            );
        }
        blob_hash_count = blob_hash_count.saturating_add(1);
    }
    Ok((max_fee_per_blob_gas, blob_hash_count))
}

fn gateway_eth_chain_bool_env(chain_id: u64, base_key: &str, default: bool) -> bool {
    let chain_key_dec = format!("{base_key}_CHAIN_{chain_id}");
    let chain_key_hex = format!("{base_key}_CHAIN_0x{:x}", chain_id);
    let chain_key_hex_upper = format!("{base_key}_CHAIN_0x{:X}", chain_id);
    string_env_nonempty(&chain_key_dec)
        .or_else(|| string_env_nonempty(&chain_key_hex))
        .or_else(|| string_env_nonempty(&chain_key_hex_upper))
        .and_then(|raw| {
            let normalized = raw.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "1" | "true" | "yes" | "on" => Some(true),
                "0" | "false" | "no" | "off" => Some(false),
                _ => None,
            }
        })
        .unwrap_or_else(|| bool_env(base_key, default))
}

pub(super) fn gateway_eth_chain_u64_env(chain_id: u64, base_key: &str, default: u64) -> u64 {
    let chain_key_dec = format!("{base_key}_CHAIN_{chain_id}");
    let chain_key_hex = format!("{base_key}_CHAIN_0x{:x}", chain_id);
    let chain_key_hex_upper = format!("{base_key}_CHAIN_0x{:X}", chain_id);
    string_env_nonempty(&chain_key_dec)
        .or_else(|| string_env_nonempty(&chain_key_hex))
        .or_else(|| string_env_nonempty(&chain_key_hex_upper))
        .and_then(|raw| parse_u64_decimal_or_hex(raw.trim()))
        .unwrap_or_else(|| u64_env(base_key, default))
}

pub(super) fn gateway_eth_type3_write_enabled(chain_id: u64) -> bool {
    gateway_eth_chain_bool_env(chain_id, "NOVOVM_EVM_ENABLE_TYPE3_WRITE", false)
}

pub(super) fn gateway_eth_type2_write_enabled(chain_id: u64) -> bool {
    gateway_eth_chain_bool_env(chain_id, "NOVOVM_EVM_ENABLE_TYPE2_WRITE", true)
}

pub(super) fn gateway_eth_type1_write_enabled(chain_id: u64) -> bool {
    gateway_eth_chain_bool_env(chain_id, "NOVOVM_EVM_ENABLE_TYPE1_WRITE", true)
}

pub(super) fn gateway_eth_london_fork_block(chain_id: u64) -> u64 {
    gateway_eth_chain_u64_env(chain_id, "NOVOVM_GATEWAY_ETH_FORK_LONDON_BLOCK", 0)
}

pub(super) fn gateway_eth_cancun_fork_block(chain_id: u64) -> u64 {
    gateway_eth_chain_u64_env(chain_id, "NOVOVM_GATEWAY_ETH_FORK_CANCUN_BLOCK", 0)
}

pub(super) fn gateway_eth_amsterdam_fork_block(chain_id: u64) -> u64 {
    gateway_eth_chain_u64_env(chain_id, "NOVOVM_GATEWAY_ETH_FORK_AMSTERDAM_BLOCK", 0)
}

pub(super) fn gateway_eth_london_active(chain_id: u64, block_number: u64) -> bool {
    block_number >= gateway_eth_london_fork_block(chain_id)
}

pub(super) fn gateway_eth_cancun_active(chain_id: u64, block_number: u64) -> bool {
    block_number >= gateway_eth_cancun_fork_block(chain_id)
}

pub(super) fn gateway_eth_amsterdam_active(chain_id: u64, block_number: u64) -> bool {
    block_number >= gateway_eth_amsterdam_fork_block(chain_id)
}

pub(super) fn gateway_eth_max_initcode_size_bytes(
    chain_id: u64,
    pending_block_number: u64,
) -> usize {
    // EIP-7954 raises limits at Amsterdam:
    // pre-Amsterdam: 49_152 (24_576 * 2), Amsterdam+: 65_536 (32_768 * 2).
    if gateway_eth_amsterdam_active(chain_id, pending_block_number) {
        65_536
    } else {
        49_152
    }
}

pub(super) fn validate_gateway_eth_contract_deploy_initcode_size(
    chain_id: u64,
    pending_block_number: u64,
    initcode_len: usize,
) -> Result<()> {
    let max_len = gateway_eth_max_initcode_size_bytes(chain_id, pending_block_number);
    if initcode_len > max_len {
        bail!(
            "contract deploy init code too large for chain_id={} pending_block={} len={} max={} (amsterdam_fork_block={})",
            chain_id,
            pending_block_number,
            initcode_len,
            max_len,
            gateway_eth_amsterdam_fork_block(chain_id)
        );
    }
    Ok(())
}

pub(super) fn validate_gateway_eth_tx_type_fork_activation(
    chain_id: u64,
    tx_type: u8,
    pending_block_number: u64,
) -> Result<()> {
    if tx_type >= 2 && !gateway_eth_london_active(chain_id, pending_block_number) {
        bail!(
            "london fork not active for chain_id={} pending_block={} required_block={}",
            chain_id,
            pending_block_number,
            gateway_eth_london_fork_block(chain_id)
        );
    }
    if tx_type == 3 && !gateway_eth_cancun_active(chain_id, pending_block_number) {
        bail!(
            "cancun fork not active for chain_id={} pending_block={} required_block={}",
            chain_id,
            pending_block_number,
            gateway_eth_cancun_fork_block(chain_id)
        );
    }
    Ok(())
}

pub(super) fn resolve_gateway_eth_write_tx_type(
    chain_id: u64,
    explicit_tx_type: Option<u64>,
    has_eip1559_fee_fields: bool,
    has_access_list_intrinsic: bool,
    max_fee_per_blob_gas: u64,
    blob_hash_count: u64,
) -> Result<u8> {
    let has_blob_fields = max_fee_per_blob_gas > 0 || blob_hash_count > 0;
    let tx_type_u64 = explicit_tx_type.unwrap_or(if has_blob_fields {
        3
    } else if has_eip1559_fee_fields {
        2
    } else if has_access_list_intrinsic {
        1
    } else {
        0
    });
    if tx_type_u64 > u8::MAX as u64 {
        bail!("tx_type out of range: {}", tx_type_u64);
    }
    let tx_type = tx_type_u64 as u8;
    if has_access_list_intrinsic && tx_type != 1 && tx_type != 2 && tx_type != 3 {
        bail!(
            "accessList requires tx_type 1 (EIP-2930), 2 (EIP-1559), or 3 (blob), got {}",
            tx_type
        );
    }
    if has_blob_fields && tx_type != 3 {
        bail!("blob fields require tx_type 3 (EIP-4844), got {}", tx_type);
    }
    if has_eip1559_fee_fields && tx_type == 0 {
        bail!("legacy tx (type 0) cannot include EIP-1559 fee fields");
    }
    if has_eip1559_fee_fields && tx_type == 1 {
        bail!("access-list tx (type 1) cannot include EIP-1559 fee fields");
    }
    if tx_type == 1 && !gateway_eth_type1_write_enabled(chain_id) {
        bail!(
            "access-list (type 1) write path disabled for chain_id={}",
            chain_id
        );
    }
    if tx_type == 2 && !gateway_eth_type2_write_enabled(chain_id) {
        bail!(
            "dynamic-fee (type 2) write path disabled for chain_id={}",
            chain_id
        );
    }
    if tx_type == 3 {
        if !gateway_eth_type3_write_enabled(chain_id) {
            bail!(
                "blob (type 3) write path disabled for chain_id={}",
                chain_id
            );
        }
        if max_fee_per_blob_gas == 0 {
            bail!("blob tx maxFeePerBlobGas must be non-zero");
        }
        if blob_hash_count == 0 {
            bail!("blob tx requires blobVersionedHashes");
        }
    }
    Ok(tx_type)
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
    gateway_eth_block_hash_from_tx_hashes(chain_id, block_number, &tx_hashes)
}

pub(super) fn gateway_eth_block_hash_from_tx_hashes(
    chain_id: u64,
    block_number: u64,
    tx_hashes: &[[u8; 32]],
) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(b"gateway-evm-block-hash-v1");
    hasher.update(chain_id.to_le_bytes());
    hasher.update(block_number.to_le_bytes());
    for tx_hash in tx_hashes {
        hasher.update(tx_hash);
    }
    hasher.finalize().into()
}

pub(super) fn gateway_eth_block_merkle_root_from_sorted_txs(
    _domain: &[u8],
    _chain_id: u64,
    _block_number: u64,
    sorted_txs: &[GatewayEthTxIndexEntry],
) -> [u8; 32] {
    if sorted_txs.is_empty() {
        return gateway_eth_empty_trie_root_bytes_for_block_roots();
    }
    let kv_pairs = sorted_txs
        .iter()
        .enumerate()
        .map(|(idx, tx)| {
            (
                gateway_eth_rlp_encode_u64(idx as u64),
                gateway_eth_transaction_leaf_payload(tx),
            )
        })
        .collect::<Vec<(Vec<u8>, Vec<u8>)>>();
    gateway_eth_mpt_root_from_kv_pairs(&kv_pairs)
}

fn gateway_eth_empty_trie_root_bytes_for_block_roots() -> [u8; 32] {
    let mut out = [0u8; 32];
    if let Ok(bytes) = decode_hex_bytes(GATEWAY_ETH_EMPTY_TRIE_ROOT, "empty_trie_root") {
        if bytes.len() == 32 {
            out.copy_from_slice(&bytes);
        }
    }
    out
}

fn gateway_eth_rlp_encode_length(prefix_short: u8, prefix_long: u8, len: usize) -> Vec<u8> {
    if len <= 55 {
        return vec![prefix_short + len as u8];
    }
    let mut len_bytes = Vec::new();
    let mut v = len;
    while v > 0 {
        len_bytes.push((v & 0xff) as u8);
        v >>= 8;
    }
    if len_bytes.is_empty() {
        len_bytes.push(0);
    }
    len_bytes.reverse();
    let mut out = Vec::with_capacity(1 + len_bytes.len());
    out.push(prefix_long + len_bytes.len() as u8);
    out.extend_from_slice(&len_bytes);
    out
}

fn gateway_eth_rlp_encode_bytes(bytes: &[u8]) -> Vec<u8> {
    if bytes.is_empty() {
        return vec![0x80];
    }
    if bytes.len() == 1 && bytes[0] < 0x80 {
        return vec![bytes[0]];
    }
    let mut out = gateway_eth_rlp_encode_length(0x80, 0xb7, bytes.len());
    out.extend_from_slice(bytes);
    out
}

fn gateway_eth_rlp_encode_u64(v: u64) -> Vec<u8> {
    if v == 0 {
        return gateway_eth_rlp_encode_bytes(&[]);
    }
    let bytes = v.to_be_bytes();
    let first_non_zero = bytes
        .iter()
        .position(|b| *b != 0)
        .unwrap_or(bytes.len() - 1);
    gateway_eth_rlp_encode_bytes(&bytes[first_non_zero..])
}

fn gateway_eth_rlp_encode_u128(v: u128) -> Vec<u8> {
    if v == 0 {
        return gateway_eth_rlp_encode_bytes(&[]);
    }
    let bytes = v.to_be_bytes();
    let first_non_zero = bytes
        .iter()
        .position(|b| *b != 0)
        .unwrap_or(bytes.len() - 1);
    gateway_eth_rlp_encode_bytes(&bytes[first_non_zero..])
}

fn gateway_eth_rlp_encode_list(items: &[Vec<u8>]) -> Vec<u8> {
    let payload_len = items.iter().map(Vec::len).sum();
    let mut out = gateway_eth_rlp_encode_length(0xc0, 0xf7, payload_len);
    for item in items {
        out.extend_from_slice(item);
    }
    out
}

fn gateway_eth_transaction_leaf_payload(tx: &GatewayEthTxIndexEntry) -> Vec<u8> {
    let to = tx.to.as_deref().unwrap_or(&[]);
    gateway_eth_rlp_encode_list(&[
        gateway_eth_rlp_encode_bytes(&tx.tx_hash),
        gateway_eth_rlp_encode_u64(tx.tx_type as u64),
        gateway_eth_rlp_encode_bytes(tx.from.as_slice()),
        gateway_eth_rlp_encode_bytes(to),
        gateway_eth_rlp_encode_u128(tx.value),
        gateway_eth_rlp_encode_u64(tx.gas_limit),
        gateway_eth_rlp_encode_u64(tx.gas_price),
        gateway_eth_rlp_encode_bytes(tx.input.as_slice()),
    ])
}

fn gateway_eth_receipt_leaf_payload(
    tx: &GatewayEthTxIndexEntry,
    cumulative_gas_used: u128,
    effective_gas_price: u64,
    contract_address: &[u8],
    status_flag: u8,
) -> Vec<u8> {
    gateway_eth_rlp_encode_list(&[
        gateway_eth_rlp_encode_u64(status_flag as u64),
        gateway_eth_rlp_encode_u128(cumulative_gas_used),
        gateway_eth_rlp_encode_u64(tx.gas_limit),
        gateway_eth_rlp_encode_u64(effective_gas_price),
        gateway_eth_rlp_encode_bytes(&tx.tx_hash),
        gateway_eth_rlp_encode_u64(tx.tx_type as u64),
        gateway_eth_rlp_encode_bytes(contract_address),
    ])
}

pub(super) fn gateway_eth_receipts_root_from_sorted_txs(
    chain_id: u64,
    _block_number: u64,
    sorted_txs: &[GatewayEthTxIndexEntry],
    is_pending_block: bool,
) -> [u8; 32] {
    if sorted_txs.is_empty() {
        return gateway_eth_empty_trie_root_bytes_for_block_roots();
    }

    let base_fee_per_gas = gateway_eth_base_fee_per_gas_wei(chain_id);
    let mut kv_pairs = Vec::<(Vec<u8>, Vec<u8>)>::with_capacity(sorted_txs.len());
    for (tx_index, tx) in sorted_txs.iter().enumerate() {
        let cumulative_gas_used = gateway_eth_block_cumulative_gas_used(sorted_txs, tx_index);
        let effective_gas_price = gateway_eth_effective_gas_price_wei(tx, base_fee_per_gas);
        let contract_address = if tx.to.is_none() {
            gateway_eth_derive_contract_address(&tx.from, tx.nonce)
        } else {
            Vec::new()
        };
        let status_flag: u8 = if is_pending_block { 0 } else { 1 };
        let payload = gateway_eth_receipt_leaf_payload(
            tx,
            cumulative_gas_used,
            effective_gas_price,
            contract_address.as_slice(),
            status_flag,
        );
        kv_pairs.push((gateway_eth_rlp_encode_u64(tx_index as u64), payload));
    }
    gateway_eth_mpt_root_from_kv_pairs(&kv_pairs)
}

pub(super) fn gateway_eth_block_by_number_json(
    chain_id: u64,
    block_number: u64,
    txs: &[GatewayEthTxIndexEntry],
    full_transactions: bool,
    is_pending_block: bool,
    state_root_override: Option<[u8; 32]>,
) -> serde_json::Value {
    let mut sorted_txs = txs.to_vec();
    sort_gateway_eth_block_txs(&mut sorted_txs);
    let block_hash = gateway_eth_block_hash_for_txs(chain_id, block_number, &sorted_txs);
    let transactions_root = gateway_eth_block_merkle_root_from_sorted_txs(
        b"gateway-evm-transactions-root-v1",
        chain_id,
        block_number,
        &sorted_txs,
    );
    let receipts_root = gateway_eth_receipts_root_from_sorted_txs(
        chain_id,
        block_number,
        &sorted_txs,
        is_pending_block,
    );
    let state_root =
        state_root_override.unwrap_or_else(|| gateway_eth_state_root_from_entries(&sorted_txs));
    let gas_used = sorted_txs
        .iter()
        .fold(0u128, |acc, tx| acc.saturating_add(tx.gas_limit as u128));
    let gas_limit = gateway_eth_fee_history_block_gas_limit();
    let parent_hash = if block_number == 0 {
        [0u8; 32]
    } else {
        gateway_eth_block_hash_from_tx_hashes(chain_id, block_number.saturating_sub(1), &[])
    };
    let base_fee_per_gas = gateway_eth_base_fee_per_gas_wei(chain_id);
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

pub(super) fn gateway_eth_default_gas_price_wei(chain_id: u64) -> u64 {
    gateway_eth_chain_u64_env(chain_id, "NOVOVM_GATEWAY_ETH_DEFAULT_GAS_PRICE", 1)
}

pub(super) fn gateway_eth_base_fee_per_gas_wei(chain_id: u64) -> u64 {
    gateway_eth_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_DEFAULT_BASE_FEE_PER_GAS",
        gateway_eth_default_gas_price_wei(chain_id),
    )
}
