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

pub(super) fn gateway_eth_balances_map_from_entries(
    entries: &[GatewayEthTxIndexEntry],
) -> HashMap<Vec<u8>, u128> {
    let mut balances = HashMap::<Vec<u8>, u128>::new();
    for entry in entries {
        if !entry.from.is_empty() {
            let from_balance = balances.entry(entry.from.clone()).or_default();
            *from_balance = from_balance.saturating_sub(entry.value);
        }
        if let Some(to) = entry.to.as_ref() {
            let to_balance = balances.entry(to.clone()).or_default();
            *to_balance = to_balance.saturating_add(entry.value);
        }
    }
    balances
}

pub(super) fn gateway_eth_total_supply_from_entries(entries: &[GatewayEthTxIndexEntry]) -> u128 {
    gateway_eth_balances_map_from_entries(entries)
        .into_values()
        .fold(0u128, |acc, value| acc.saturating_add(value))
}

pub(super) fn gateway_eth_has_code_for_address(
    entries: &[GatewayEthTxIndexEntry],
    address: &[u8],
) -> bool {
    gateway_eth_resolve_code_from_entries(entries, address)
        .map(|code| !code.is_empty())
        .unwrap_or(false)
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

fn gateway_eth_rlp_encode_bytes(bytes: &[u8]) -> Vec<u8> {
    if bytes.is_empty() {
        return vec![0x80];
    }
    if bytes.len() == 1 && bytes[0] < 0x80 {
        return vec![bytes[0]];
    }
    if bytes.len() <= 55 {
        let mut out = Vec::with_capacity(1 + bytes.len());
        out.push(0x80 + bytes.len() as u8);
        out.extend_from_slice(bytes);
        return out;
    }
    let len_bytes = rlp_encode_length_bytes(bytes.len());
    let mut out = Vec::with_capacity(1 + len_bytes.len() + bytes.len());
    out.push(0xb7 + len_bytes.len() as u8);
    out.extend_from_slice(&len_bytes);
    out.extend_from_slice(bytes);
    out
}

fn rlp_encode_length_bytes(mut value: usize) -> Vec<u8> {
    let mut out = Vec::new();
    while value > 0 {
        out.push((value & 0xff) as u8);
        value >>= 8;
    }
    if out.is_empty() {
        out.push(0);
    }
    out.reverse();
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
    let payload_len = items.iter().map(Vec::len).sum::<usize>();
    let mut out = Vec::with_capacity(payload_len + 8);
    if payload_len <= 55 {
        out.push(0xc0 + payload_len as u8);
    } else {
        let len_bytes = rlp_encode_length_bytes(payload_len);
        out.push(0xf7 + len_bytes.len() as u8);
        out.extend_from_slice(&len_bytes);
    }
    for item in items {
        out.extend_from_slice(item);
    }
    out
}

fn gateway_eth_rlp_encode_contract_create(sender: &[u8], nonce: u64) -> Vec<u8> {
    let sender_rlp = gateway_eth_rlp_encode_bytes(sender);
    let nonce_raw = if nonce == 0 {
        Vec::new()
    } else {
        let bytes = nonce.to_be_bytes();
        let first_non_zero = bytes
            .iter()
            .position(|v| *v != 0)
            .unwrap_or(bytes.len() - 1);
        bytes[first_non_zero..].to_vec()
    };
    let nonce_rlp = gateway_eth_rlp_encode_bytes(&nonce_raw);
    let payload_len = sender_rlp.len() + nonce_rlp.len();
    let mut out = Vec::with_capacity(payload_len + 8);
    if payload_len <= 55 {
        out.push(0xc0 + payload_len as u8);
    } else {
        let len_bytes = rlp_encode_length_bytes(payload_len);
        out.push(0xf7 + len_bytes.len() as u8);
        out.extend_from_slice(&len_bytes);
    }
    out.extend_from_slice(&sender_rlp);
    out.extend_from_slice(&nonce_rlp);
    out
}

pub(super) fn gateway_eth_derive_contract_address(from: &[u8], nonce: u64) -> Vec<u8> {
    if from.len() != 20 {
        let mut fallback = Keccak256::new();
        fallback.update(b"novovm_gateway_invalid_from_contract_address_v1");
        fallback.update(from);
        fallback.update(nonce.to_be_bytes());
        let digest: [u8; 32] = fallback.finalize().into();
        return digest[12..32].to_vec();
    }
    let encoded = gateway_eth_rlp_encode_contract_create(from, nonce);
    let digest: [u8; 32] = Keccak256::digest(encoded).into();
    digest[12..32].to_vec()
}

pub(super) fn gateway_eth_resolve_code_from_entries(
    entries: &[GatewayEthTxIndexEntry],
    address: &[u8],
) -> Option<Vec<u8>> {
    let mut code: Option<Vec<u8>> = None;
    for entry in entries {
        if entry.to.is_some() || entry.input.is_empty() {
            continue;
        }
        let deployed = gateway_eth_derive_contract_address(&entry.from, entry.nonce);
        if deployed == address {
            code = Some(entry.input.clone());
        }
    }
    code
}

pub(super) fn gateway_eth_resolve_storage_word_from_entries_by_key(
    entries: &[GatewayEthTxIndexEntry],
    address: &[u8],
    slot_key: [u8; 32],
) -> Option<[u8; 32]> {
    gateway_eth_collect_storage_items_for_address(entries, address)
        .into_iter()
        .find_map(|(candidate_key, value)| {
            if candidate_key == slot_key {
                Some(value)
            } else {
                None
            }
        })
}

#[derive(Debug, Clone)]
pub(super) struct GatewayEthProofAccountView {
    pub(super) balance: u128,
    pub(super) nonce: u64,
    pub(super) code_hash: [u8; 32],
    pub(super) storage_items: Vec<([u8; 32], [u8; 32])>,
    pub(super) account_proof: Vec<Vec<u8>>,
}

fn gateway_eth_empty_trie_root_bytes() -> [u8; 32] {
    let mut out = [0u8; 32];
    if let Ok(bytes) = decode_hex_bytes(GATEWAY_ETH_EMPTY_TRIE_ROOT, "empty_trie_root") {
        if bytes.len() == 32 {
            out.copy_from_slice(&bytes);
        }
    }
    out
}

fn gateway_eth_slot_to_be32(slot: u128) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[16..].copy_from_slice(&slot.to_be_bytes());
    out
}

fn gateway_eth_storage_key_hash(slot_key: [u8; 32]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(slot_key);
    hasher.finalize().into()
}

fn gateway_eth_account_key_hash(address: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(address);
    hasher.finalize().into()
}

#[derive(Debug, Clone)]
enum GatewayEthMptNode {
    Leaf {
        path_nibbles: Vec<u8>,
        value: Vec<u8>,
    },
    Extension {
        path_nibbles: Vec<u8>,
        child: Box<GatewayEthMptNode>,
    },
    Branch {
        children: [Option<Box<GatewayEthMptNode>>; 16],
        value: Option<Vec<u8>>,
    },
}

fn gateway_eth_mpt_nibbles_from_key(key: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(key.len() * 2);
    for byte in key {
        out.push(byte >> 4);
        out.push(byte & 0x0f);
    }
    out
}

fn gateway_eth_mpt_hex_prefix_encode(path_nibbles: &[u8], is_leaf: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(path_nibbles.len() / 2 + 1);
    let flag = if is_leaf { 2u8 } else { 0u8 };
    let mut idx = 0usize;
    if path_nibbles.len() % 2 == 1 {
        out.push(((flag + 1) << 4) | (path_nibbles[0] & 0x0f));
        idx = 1;
    } else {
        out.push(flag << 4);
    }
    while idx < path_nibbles.len() {
        let high = path_nibbles[idx] & 0x0f;
        let low = path_nibbles[idx + 1] & 0x0f;
        out.push((high << 4) | low);
        idx += 2;
    }
    out
}

fn gateway_eth_mpt_common_prefix_len(keys: &[Vec<u8>]) -> usize {
    if keys.is_empty() {
        return 0;
    }
    let mut prefix = keys[0].len();
    for key in keys.iter().skip(1) {
        let mut idx = 0usize;
        let max = prefix.min(key.len());
        while idx < max && keys[0][idx] == key[idx] {
            idx += 1;
        }
        prefix = idx;
        if prefix == 0 {
            break;
        }
    }
    prefix
}

fn gateway_eth_mpt_build_from_nibbles(
    entries: Vec<(Vec<u8>, Vec<u8>)>,
) -> Option<GatewayEthMptNode> {
    if entries.is_empty() {
        return None;
    }
    if entries.len() == 1 {
        let (path_nibbles, value) = entries.into_iter().next()?;
        return Some(GatewayEthMptNode::Leaf {
            path_nibbles,
            value,
        });
    }
    let keys = entries
        .iter()
        .map(|(key, _)| key.clone())
        .collect::<Vec<Vec<u8>>>();
    let common = gateway_eth_mpt_common_prefix_len(&keys);
    if common > 0 {
        let prefix = keys[0][..common].to_vec();
        let stripped = entries
            .into_iter()
            .map(|(key, value)| (key[common..].to_vec(), value))
            .collect::<Vec<(Vec<u8>, Vec<u8>)>>();
        let child = gateway_eth_mpt_build_from_nibbles(stripped)?;
        return Some(GatewayEthMptNode::Extension {
            path_nibbles: prefix,
            child: Box::new(child),
        });
    }

    let mut buckets: [Vec<(Vec<u8>, Vec<u8>)>; 16] = std::array::from_fn(|_| Vec::new());
    let mut branch_value: Option<Vec<u8>> = None;
    for (key, value) in entries {
        if key.is_empty() {
            branch_value = Some(value);
            continue;
        }
        let idx = key[0] as usize;
        if idx < 16 {
            buckets[idx].push((key[1..].to_vec(), value));
        }
    }
    let mut children: [Option<Box<GatewayEthMptNode>>; 16] = std::array::from_fn(|_| None);
    for (idx, bucket) in buckets.iter_mut().enumerate() {
        if bucket.is_empty() {
            continue;
        }
        if let Some(child) = gateway_eth_mpt_build_from_nibbles(std::mem::take(bucket)) {
            children[idx] = Some(Box::new(child));
        }
    }
    Some(GatewayEthMptNode::Branch {
        children,
        value: branch_value,
    })
}

fn gateway_eth_mpt_node_rlp(node: &GatewayEthMptNode) -> Vec<u8> {
    match node {
        GatewayEthMptNode::Leaf {
            path_nibbles,
            value,
        } => {
            let compact = gateway_eth_mpt_hex_prefix_encode(path_nibbles, true);
            gateway_eth_rlp_encode_list(&[
                gateway_eth_rlp_encode_bytes(&compact),
                gateway_eth_rlp_encode_bytes(value),
            ])
        }
        GatewayEthMptNode::Extension {
            path_nibbles,
            child,
        } => {
            let compact = gateway_eth_mpt_hex_prefix_encode(path_nibbles, false);
            let child_rlp = gateway_eth_mpt_node_rlp(child);
            let child_ref = if child_rlp.len() < 32 {
                child_rlp
            } else {
                let child_hash: [u8; 32] = Keccak256::digest(&child_rlp).into();
                gateway_eth_rlp_encode_bytes(&child_hash)
            };
            gateway_eth_rlp_encode_list(&[gateway_eth_rlp_encode_bytes(&compact), child_ref])
        }
        GatewayEthMptNode::Branch { children, value } => {
            let mut items = Vec::with_capacity(17);
            for child in children {
                if let Some(child_node) = child {
                    let child_rlp = gateway_eth_mpt_node_rlp(child_node);
                    if child_rlp.len() < 32 {
                        items.push(child_rlp);
                    } else {
                        let child_hash: [u8; 32] = Keccak256::digest(&child_rlp).into();
                        items.push(gateway_eth_rlp_encode_bytes(&child_hash));
                    }
                } else {
                    items.push(gateway_eth_rlp_encode_bytes(&[]));
                }
            }
            items.push(gateway_eth_rlp_encode_bytes(
                value.as_deref().unwrap_or_default(),
            ));
            gateway_eth_rlp_encode_list(&items)
        }
    }
}

fn gateway_eth_mpt_collect_proof(
    node: &GatewayEthMptNode,
    key_nibbles: &[u8],
    out: &mut Vec<Vec<u8>>,
) {
    out.push(gateway_eth_mpt_node_rlp(node));
    match node {
        GatewayEthMptNode::Leaf { .. } => {}
        GatewayEthMptNode::Extension {
            path_nibbles,
            child,
        } => {
            if key_nibbles.starts_with(path_nibbles) {
                gateway_eth_mpt_collect_proof(child, &key_nibbles[path_nibbles.len()..], out);
            }
        }
        GatewayEthMptNode::Branch { children, .. } => {
            if key_nibbles.is_empty() {
                return;
            }
            let idx = key_nibbles[0] as usize;
            if idx >= 16 {
                return;
            }
            if let Some(child) = children[idx].as_ref() {
                gateway_eth_mpt_collect_proof(child, &key_nibbles[1..], out);
            }
        }
    }
}

fn gateway_eth_mpt_build_from_kv_pairs(
    kv_pairs: &[(Vec<u8>, Vec<u8>)],
) -> Option<GatewayEthMptNode> {
    let mut dedup = BTreeMap::<Vec<u8>, Vec<u8>>::new();
    for (key, value) in kv_pairs {
        dedup.insert(gateway_eth_mpt_nibbles_from_key(key), value.clone());
    }
    let entries = dedup.into_iter().collect::<Vec<(Vec<u8>, Vec<u8>)>>();
    gateway_eth_mpt_build_from_nibbles(entries)
}

pub(super) fn gateway_eth_mpt_root_from_kv_pairs(kv_pairs: &[(Vec<u8>, Vec<u8>)]) -> [u8; 32] {
    let Some(root_node) = gateway_eth_mpt_build_from_kv_pairs(kv_pairs) else {
        return gateway_eth_empty_trie_root_bytes();
    };
    let root_rlp = gateway_eth_mpt_node_rlp(&root_node);
    Keccak256::digest(root_rlp).into()
}

pub(super) fn gateway_eth_mpt_proof_for_key_from_kv_pairs(
    kv_pairs: &[(Vec<u8>, Vec<u8>)],
    key: &[u8],
) -> Vec<Vec<u8>> {
    let Some(root_node) = gateway_eth_mpt_build_from_kv_pairs(kv_pairs) else {
        return Vec::new();
    };
    let mut out = Vec::<Vec<u8>>::new();
    let key_nibbles = gateway_eth_mpt_nibbles_from_key(key);
    gateway_eth_mpt_collect_proof(&root_node, &key_nibbles, &mut out);
    out
}

fn gateway_eth_storage_value_trie_payload(value: [u8; 32]) -> Vec<u8> {
    let first_non_zero = value.iter().position(|b| *b != 0).unwrap_or(value.len());
    if first_non_zero == value.len() {
        gateway_eth_rlp_encode_bytes(&[])
    } else {
        gateway_eth_rlp_encode_bytes(&value[first_non_zero..])
    }
}

fn gateway_eth_storage_trie_kv_pairs(
    storage_items: &[([u8; 32], [u8; 32])],
) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut out = BTreeMap::<Vec<u8>, Vec<u8>>::new();
    for (slot_key, value) in storage_items {
        let key_hash = gateway_eth_storage_key_hash(*slot_key);
        out.insert(
            key_hash.to_vec(),
            gateway_eth_storage_value_trie_payload(*value),
        );
    }
    out.into_iter().collect()
}

pub(super) fn gateway_eth_storage_root_from_items(
    storage_items: &[([u8; 32], [u8; 32])],
) -> [u8; 32] {
    let kv_pairs = gateway_eth_storage_trie_kv_pairs(storage_items);
    gateway_eth_mpt_root_from_kv_pairs(&kv_pairs)
}

pub(super) fn gateway_eth_collect_storage_items_for_address(
    entries: &[GatewayEthTxIndexEntry],
    address: &[u8],
) -> Vec<([u8; 32], [u8; 32])> {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| {
        a.nonce
            .cmp(&b.nonce)
            .then_with(|| a.tx_hash.cmp(&b.tx_hash))
    });
    let mut slots = BTreeMap::<[u8; 32], [u8; 32]>::new();
    for entry in &sorted {
        let slot_key = gateway_eth_slot_to_be32(entry.nonce as u128);
        if entry.to.as_ref().is_some_and(|to| to == address) || entry.from == address {
            slots.insert(slot_key, entry.tx_hash);
        }
    }
    for entry in &sorted {
        if entry.to.is_some() || entry.input.is_empty() {
            continue;
        }
        let deployed = gateway_eth_derive_contract_address(&entry.from, entry.nonce);
        if deployed == address {
            let digest: [u8; 32] = Keccak256::digest(&entry.input).into();
            slots.insert([0u8; 32], digest);
        }
    }
    slots.into_iter().collect()
}

type GatewayEthStorageProofEntry = ([u8; 32], [u8; 32], Vec<Vec<u8>>);

pub(super) fn gateway_eth_storage_proof_for_slots(
    storage_items: &[([u8; 32], [u8; 32])],
    slots: &[[u8; 32]],
) -> ([u8; 32], Vec<GatewayEthStorageProofEntry>) {
    let kv_pairs = gateway_eth_storage_trie_kv_pairs(storage_items);
    let storage_root = gateway_eth_mpt_root_from_kv_pairs(&kv_pairs);
    let values = storage_items
        .iter()
        .map(|(slot_key, value)| (*slot_key, *value))
        .collect::<BTreeMap<[u8; 32], [u8; 32]>>();
    let mut out = Vec::with_capacity(slots.len());
    for slot_key in slots {
        let value = values.get(slot_key).copied().unwrap_or([0u8; 32]);
        let key_hash = gateway_eth_storage_key_hash(*slot_key);
        let proof = gateway_eth_mpt_proof_for_key_from_kv_pairs(&kv_pairs, &key_hash);
        out.push((*slot_key, value, proof));
    }
    (storage_root, out)
}

fn gateway_eth_collect_account_addresses(entries: &[GatewayEthTxIndexEntry]) -> Vec<Vec<u8>> {
    let mut out = BTreeSet::<Vec<u8>>::new();
    for entry in entries {
        out.insert(entry.from.clone());
        if let Some(to) = entry.to.as_ref() {
            out.insert(to.clone());
        }
        if entry.to.is_none() && !entry.input.is_empty() {
            out.insert(gateway_eth_derive_contract_address(
                &entry.from,
                entry.nonce,
            ));
        }
    }
    out.into_iter().collect()
}

pub(super) fn gateway_eth_resolve_account_proof_view(
    entries: &[GatewayEthTxIndexEntry],
    address: &[u8],
) -> GatewayEthProofAccountView {
    let mut target_storage_items = gateway_eth_collect_storage_items_for_address(entries, address);
    let mut target_balance = gateway_eth_balance_from_entries(entries, address);
    let mut target_nonce = entries
        .iter()
        .filter(|entry| entry.from == address)
        .map(|entry| entry.nonce.saturating_add(1))
        .max()
        .unwrap_or(0);
    let target_code_hash = {
        let code = gateway_eth_resolve_code_from_entries(entries, address).unwrap_or_default();
        Keccak256::digest(code).into()
    };
    let mut account_kv_pairs = Vec::<(Vec<u8>, Vec<u8>)>::new();
    for account in gateway_eth_collect_account_addresses(entries) {
        let storage_items = gateway_eth_collect_storage_items_for_address(entries, &account);
        let storage_root = gateway_eth_storage_root_from_items(&storage_items);
        let balance = gateway_eth_balance_from_entries(entries, &account);
        let nonce = entries
            .iter()
            .filter(|entry| entry.from == account)
            .map(|entry| entry.nonce.saturating_add(1))
            .max()
            .unwrap_or(0);
        let code_hash = {
            let code = gateway_eth_resolve_code_from_entries(entries, &account).unwrap_or_default();
            let digest: [u8; 32] = Keccak256::digest(code).into();
            digest
        };
        if account == address {
            target_storage_items = storage_items.clone();
            target_balance = balance;
            target_nonce = nonce;
        }
        let key_hash = gateway_eth_account_key_hash(&account);
        let account_payload = gateway_eth_rlp_encode_list(&[
            gateway_eth_rlp_encode_u64(nonce),
            gateway_eth_rlp_encode_u128(balance),
            gateway_eth_rlp_encode_bytes(&storage_root),
            gateway_eth_rlp_encode_bytes(&code_hash),
        ]);
        account_kv_pairs.push((key_hash.to_vec(), account_payload));
    }

    let target_key = gateway_eth_account_key_hash(address);
    let account_proof = gateway_eth_mpt_proof_for_key_from_kv_pairs(&account_kv_pairs, &target_key);

    GatewayEthProofAccountView {
        balance: target_balance,
        nonce: target_nonce,
        code_hash: target_code_hash,
        storage_items: target_storage_items,
        account_proof,
    }
}

pub(super) fn gateway_eth_state_root_from_entries(entries: &[GatewayEthTxIndexEntry]) -> [u8; 32] {
    let mut account_kv_pairs = Vec::<(Vec<u8>, Vec<u8>)>::new();
    for account in gateway_eth_collect_account_addresses(entries) {
        let storage_items = gateway_eth_collect_storage_items_for_address(entries, &account);
        let storage_root = gateway_eth_storage_root_from_items(&storage_items);
        let balance = gateway_eth_balance_from_entries(entries, &account);
        let nonce = entries
            .iter()
            .filter(|entry| entry.from == account)
            .map(|entry| entry.nonce.saturating_add(1))
            .max()
            .unwrap_or(0);
        let code_hash: [u8; 32] = {
            let code = gateway_eth_resolve_code_from_entries(entries, &account).unwrap_or_default();
            Keccak256::digest(code).into()
        };
        let key_hash = gateway_eth_account_key_hash(&account);
        let account_payload = gateway_eth_rlp_encode_list(&[
            gateway_eth_rlp_encode_u64(nonce),
            gateway_eth_rlp_encode_u128(balance),
            gateway_eth_rlp_encode_bytes(&storage_root),
            gateway_eth_rlp_encode_bytes(&code_hash),
        ]);
        account_kv_pairs.push((key_hash.to_vec(), account_payload));
    }
    gateway_eth_mpt_root_from_kv_pairs(&account_kv_pairs)
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
