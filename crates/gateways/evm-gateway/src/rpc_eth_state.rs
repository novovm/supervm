use super::*;

pub(super) fn gateway_eth_query_scan_max() -> usize {
    let raw = u64_env(
        "NOVOVM_GATEWAY_ETH_QUERY_SCAN_MAX",
        GATEWAY_ETH_QUERY_SCAN_MAX_DEFAULT as u64,
    );
    raw.clamp(1, 200_000) as usize
}

pub(super) fn collect_gateway_eth_chain_entries(
    eth_tx_index: &HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    chain_id: u64,
    max_items: usize,
) -> Result<Vec<GatewayEthTxIndexEntry>> {
    if max_items == 0 {
        return Ok(Vec::new());
    }
    let mut by_hash = HashMap::<[u8; 32], GatewayEthTxIndexEntry>::new();
    let mut in_mem_entries = eth_tx_index
        .values()
        .filter(|entry| entry.chain_id == chain_id)
        .cloned()
        .collect::<Vec<GatewayEthTxIndexEntry>>();
    in_mem_entries.sort_by(|a, b| {
        a.nonce
            .cmp(&b.nonce)
            .then_with(|| a.tx_hash.cmp(&b.tx_hash))
    });
    if in_mem_entries.len() > max_items {
        in_mem_entries = in_mem_entries.split_off(in_mem_entries.len().saturating_sub(max_items));
    }
    for entry in in_mem_entries {
        by_hash.insert(entry.tx_hash, entry);
    }
    if by_hash.len() < max_items {
        let remain = max_items.saturating_sub(by_hash.len());
        let persisted = eth_tx_index_store.load_eth_txs_by_chain(chain_id, remain)?;
        for entry in persisted {
            by_hash.entry(entry.tx_hash).or_insert(entry);
            if by_hash.len() >= max_items {
                break;
            }
        }
    }
    let mut out: Vec<GatewayEthTxIndexEntry> = by_hash.into_values().collect();
    out.sort_by(|a, b| {
        a.nonce
            .cmp(&b.nonce)
            .then_with(|| a.tx_hash.cmp(&b.tx_hash))
    });
    if out.len() > max_items {
        out = out.split_off(out.len().saturating_sub(max_items));
    }
    Ok(out)
}

pub(super) fn resolve_gateway_eth_latest_block_number(
    chain_id: u64,
    entries: &[GatewayEthTxIndexEntry],
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
) -> Result<u64> {
    let latest_from_entries = entries.iter().map(|entry| entry.nonce).max().unwrap_or(0);
    let latest_from_store = eth_tx_index_store
        .load_eth_latest_block_number(chain_id)?
        .unwrap_or(0);
    Ok(latest_from_entries.max(latest_from_store))
}

pub(super) fn collect_gateway_eth_block_entries_precise(
    eth_tx_index: &HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    chain_id: u64,
    block_number: u64,
    max_items: usize,
) -> Result<Vec<GatewayEthTxIndexEntry>> {
    if max_items == 0 {
        return Ok(Vec::new());
    }
    let mut by_hash = HashMap::<[u8; 32], GatewayEthTxIndexEntry>::new();
    for entry in eth_tx_index.values() {
        if entry.chain_id != chain_id || entry.nonce != block_number {
            continue;
        }
        by_hash.insert(entry.tx_hash, entry.clone());
        if by_hash.len() >= max_items {
            break;
        }
    }
    if by_hash.len() < max_items {
        let remain = max_items.saturating_sub(by_hash.len());
        let persisted = eth_tx_index_store.load_eth_txs_by_block(chain_id, block_number, remain)?;
        for entry in persisted {
            by_hash.entry(entry.tx_hash).or_insert(entry);
            if by_hash.len() >= max_items {
                break;
            }
        }
    }
    let mut out: Vec<GatewayEthTxIndexEntry> = by_hash.into_values().collect();
    sort_gateway_eth_block_txs(&mut out);
    Ok(out)
}

pub(super) fn collect_gateway_eth_block_entries_by_hash_precise(
    eth_tx_index: &HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    chain_id: u64,
    block_hash: &[u8; 32],
    max_items: usize,
) -> Result<Option<(u64, Vec<GatewayEthTxIndexEntry>)>> {
    let Some(block_number) =
        eth_tx_index_store.load_eth_block_number_by_hash(chain_id, block_hash)?
    else {
        return Ok(None);
    };
    let block_txs = collect_gateway_eth_block_entries_precise(
        eth_tx_index,
        eth_tx_index_store,
        chain_id,
        block_number,
        max_items,
    )?;
    if block_txs.is_empty() {
        return Ok(None);
    }
    let candidate = gateway_eth_block_hash_for_txs(chain_id, block_number, &block_txs);
    if candidate != *block_hash {
        return Ok(None);
    }
    Ok(Some((block_number, block_txs)))
}

pub(super) fn gateway_eth_select_median_gas_price_wei(mut prices: Vec<u64>) -> Option<u64> {
    prices.retain(|price| *price > 0);
    if prices.is_empty() {
        return None;
    }
    prices.sort_unstable();
    Some(prices[prices.len() / 2])
}

pub(super) fn gateway_eth_suggest_gas_price_wei(
    chain_id: u64,
    eth_tx_index: &HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    fallback: u64,
) -> Result<u64> {
    let (pending_txs, queued_txs) = collect_gateway_eth_txpool_runtime_txs(chain_id);
    let runtime_prices = pending_txs
        .into_iter()
        .chain(queued_txs)
        .map(|tx| tx.gas_price)
        .collect::<Vec<u64>>();
    if let Some(price) = gateway_eth_select_median_gas_price_wei(runtime_prices) {
        return Ok(price);
    }

    let recent_prices = collect_gateway_eth_chain_entries(
        eth_tx_index,
        eth_tx_index_store,
        chain_id,
        gateway_eth_query_scan_max(),
    )?
    .iter()
    .rev()
    .take(64)
    .map(|entry| entry.gas_price)
    .collect::<Vec<u64>>();
    if let Some(price) = gateway_eth_select_median_gas_price_wei(recent_prices) {
        return Ok(price);
    }

    Ok(fallback)
}

pub(super) fn gateway_eth_balance_from_entries(
    entries: &[GatewayEthTxIndexEntry],
    address: &[u8],
) -> u128 {
    let mut balance = 0u128;
    for entry in entries {
        if entry.from == address {
            balance = balance.saturating_sub(entry.value);
        }
        if entry.to.as_ref().is_some_and(|to| to == address) {
            balance = balance.saturating_add(entry.value);
        }
    }
    balance
}

pub(super) fn gateway_eth_empty_logs_bloom_hex() -> String {
    format!("0x{}", "00".repeat(256))
}

pub(super) fn gateway_eth_contract_address_hex(
    entry: &GatewayEthTxIndexEntry,
) -> serde_json::Value {
    if entry.to.is_some() {
        return serde_json::Value::Null;
    }
    let address = gateway_eth_derive_contract_address(&entry.from, entry.nonce);
    serde_json::Value::String(format!("0x{}", to_hex(&address)))
}

pub(super) fn gateway_eth_derive_contract_address(from: &[u8], nonce: u64) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm_contract_address_v1");
    hasher.update(from);
    hasher.update(nonce.to_le_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    digest[12..32].to_vec()
}

pub(super) fn gateway_eth_resolve_code_from_entries(
    entries: &[GatewayEthTxIndexEntry],
    address: &[u8],
) -> Option<Vec<u8>> {
    let mut code: Option<Vec<u8>> = None;
    for entry in entries {
        if entry.tx_type != 2 || entry.to.is_some() || entry.input.is_empty() {
            continue;
        }
        let deployed = gateway_eth_derive_contract_address(&entry.from, entry.nonce);
        if deployed == address {
            code = Some(entry.input.clone());
        }
    }
    code
}

pub(super) fn gateway_eth_resolve_storage_word_from_entries(
    entries: &[GatewayEthTxIndexEntry],
    address: &[u8],
    slot: u128,
) -> Option<[u8; 32]> {
    let mut word: Option<[u8; 32]> = None;
    if let Ok(slot_nonce) = u64::try_from(slot) {
        for entry in entries {
            if entry.nonce != slot_nonce {
                continue;
            }
            // Mirrors current adapter semantics:
            // - tx_type=1 (contract call): storage owner is `to`, slot is `nonce`, value is `tx_hash`
            // - other tx types: storage owner is `from`, slot is `nonce`, value is `tx_hash`
            let owner_match = if entry.tx_type == 1 {
                entry.to.as_ref().is_some_and(|to| to == address)
            } else {
                entry.from == address
            };
            if owner_match {
                word = Some(entry.tx_hash);
            }
        }
    }

    // Deploy path compatibility: expose code-hash at slot 0 for latest matching deploy.
    if word.is_none() && slot == 0 {
        for entry in entries {
            if entry.tx_type != 2 || entry.to.is_some() || entry.input.is_empty() {
                continue;
            }
            let deployed = gateway_eth_derive_contract_address(&entry.from, entry.nonce);
            if deployed == address {
                let digest: [u8; 32] = Sha256::digest(&entry.input).into();
                word = Some(digest);
            }
        }
    }
    word
}

pub(super) fn gateway_eth_keccak_hex(bytes: &[u8]) -> String {
    let digest = Keccak256::digest(bytes);
    format!("0x{}", to_hex(digest.as_slice()))
}

pub(super) fn gateway_eth_storage_hash_hex(
    address: &[u8],
    storage_items: &[(u128, [u8; 32])],
) -> String {
    if storage_items.is_empty() {
        return GATEWAY_ETH_EMPTY_TRIE_ROOT.to_string();
    }
    let mut sorted = storage_items.to_vec();
    sorted.sort_by(|(slot_a, _), (slot_b, _)| slot_a.cmp(slot_b));
    let mut hasher = Keccak256::new();
    hasher.update(b"novovm_gateway_eth_storage_hash_v1");
    hasher.update(address);
    for (slot, value) in sorted {
        hasher.update(slot.to_be_bytes());
        hasher.update(value);
    }
    let digest = hasher.finalize();
    format!("0x{}", to_hex(digest.as_slice()))
}

pub(super) fn sort_gateway_eth_block_txs(txs: &mut [GatewayEthTxIndexEntry]) {
    txs.sort_by(|a, b| a.tx_hash.cmp(&b.tx_hash));
}

pub(super) fn gateway_eth_group_entries_by_block(
    entries: Vec<GatewayEthTxIndexEntry>,
) -> BTreeMap<u64, Vec<GatewayEthTxIndexEntry>> {
    let mut blocks = BTreeMap::<u64, Vec<GatewayEthTxIndexEntry>>::new();
    for entry in entries {
        blocks.entry(entry.nonce).or_default().push(entry);
    }
    for txs in blocks.values_mut() {
        sort_gateway_eth_block_txs(txs);
    }
    blocks
}

pub(super) fn parse_eth_block_hash_from_params(
    params: &serde_json::Value,
) -> Result<Option<[u8; 32]>> {
    if let Some(map) = params_object_with_any_keys(params, &["block_hash", "blockHash", "hash"]) {
        if let Some(raw_hash) = map
            .get("block_hash")
            .or_else(|| map.get("blockHash"))
            .or_else(|| map.get("hash"))
            .and_then(value_to_string)
        {
            return Ok(Some(parse_hex32_from_string(&raw_hash, "block_hash")?));
        }
    }
    if let Some(text) = first_scalar_param_string(params) {
        let trimmed = text.trim();
        if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
            if let Ok(hash) = parse_hex32_from_string(trimmed, "block_hash") {
                return Ok(Some(hash));
            }
        }
    }
    Ok(None)
}

pub(super) fn gateway_eth_coinbase_from_env() -> Result<Vec<u8>> {
    let Some(raw) = string_env_nonempty("NOVOVM_GATEWAY_ETH_COINBASE") else {
        return Ok(vec![0u8; 20]);
    };
    let decoded =
        decode_hex_bytes(&raw, "NOVOVM_GATEWAY_ETH_COINBASE").unwrap_or_else(|_| vec![0u8; 20]);
    if decoded.len() == 20 {
        Ok(decoded)
    } else {
        Ok(vec![0u8; 20])
    }
}

pub(super) fn gateway_web3_client_version_from_env() -> String {
    string_env_nonempty("NOVOVM_GATEWAY_WEB3_CLIENT_VERSION")
        .unwrap_or_else(|| format!("novovm-evm-gateway/{}", env!("CARGO_PKG_VERSION")))
}

pub(super) fn gateway_eth_protocol_version_from_env() -> String {
    let Some(raw) = string_env_nonempty("NOVOVM_GATEWAY_ETH_PROTOCOL_VERSION") else {
        return "0x41".to_string();
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "0x41".to_string()
    } else if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        trimmed.to_string()
    } else if let Ok(as_u64) = trimmed.parse::<u64>() {
        format!("0x{:x}", as_u64)
    } else {
        "0x41".to_string()
    }
}

pub(super) fn gateway_eth_accounts_from_env() -> Result<Vec<Vec<u8>>> {
    let Some(raw) = string_env_nonempty("NOVOVM_GATEWAY_ETH_ACCOUNTS") else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for token in raw.split([',', ';']) {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(decoded) = decode_hex_bytes(trimmed, "NOVOVM_GATEWAY_ETH_ACCOUNTS") else {
            continue;
        };
        if decoded.len() != 20 {
            continue;
        }
        if !out.contains(&decoded) {
            out.push(decoded);
        }
    }
    Ok(out)
}
