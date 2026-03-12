use super::*;

pub(super) fn gateway_eth_sync_status_path_from_env(chain_id: u64) -> Option<PathBuf> {
    let chain_key_dec = format!("NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_{chain_id}");
    let chain_key_hex = format!("NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH_CHAIN_0x{:x}", chain_id);
    string_env_nonempty(&chain_key_dec)
        .or_else(|| string_env_nonempty(&chain_key_hex))
        .or_else(|| string_env_nonempty("NOVOVM_GATEWAY_ETH_SYNC_STATUS_PATH"))
        .map(PathBuf::from)
}

pub(super) fn load_gateway_eth_sync_snapshot(path: &Path) -> Option<serde_json::Value> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<serde_json::Value>(&raw).ok()
}

pub(super) fn gateway_eth_sync_snapshot_u64(
    snapshot: &serde_json::Value,
    keys: &[&str],
) -> Option<u64> {
    let map = snapshot.as_object()?;
    keys.iter()
        .find_map(|key| map.get(*key))
        .and_then(value_to_u64)
}

pub(super) fn gateway_eth_sync_env_u64_for_chain(base_key: &str, chain_id: u64) -> Option<u64> {
    let chain_key_dec = format!("{base_key}_CHAIN_{chain_id}");
    let chain_key_hex = format!("{base_key}_CHAIN_0x{:x}", chain_id);
    string_env_nonempty(&chain_key_dec)
        .or_else(|| string_env_nonempty(&chain_key_hex))
        .or_else(|| string_env_nonempty(base_key))
        .and_then(|raw| parse_u64_decimal_or_hex(raw.trim()))
}

pub(super) fn gateway_eth_peer_count_from_env(chain_id: u64) -> Option<u64> {
    gateway_eth_sync_env_u64_for_chain("NOVOVM_GATEWAY_ETH_PEER_COUNT", chain_id)
        .or_else(|| gateway_eth_sync_env_u64_for_chain("NOVOVM_GATEWAY_NET_PEER_COUNT", chain_id))
}

pub(super) fn gateway_eth_sync_snapshot_for_chain<'a>(
    snapshot: &'a serde_json::Value,
    chain_id: u64,
) -> &'a serde_json::Value {
    let Some(root) = snapshot.as_object() else {
        return snapshot;
    };
    let chain_key_dec = chain_id.to_string();
    let chain_key_hex = format!("0x{:x}", chain_id);
    if let Some(chains) = root.get("chains").and_then(serde_json::Value::as_object) {
        if let Some(chain_snapshot) = chains
            .get(chain_key_dec.as_str())
            .or_else(|| chains.get(chain_key_hex.as_str()))
        {
            return chain_snapshot;
        }
    }
    if let Some(chain_snapshot) = root
        .get(chain_key_dec.as_str())
        .or_else(|| root.get(chain_key_hex.as_str()))
    {
        return chain_snapshot;
    }
    snapshot
}

pub(super) fn gateway_eth_sync_snapshot_peer_count(snapshot: &serde_json::Value) -> Option<u64> {
    gateway_eth_sync_snapshot_u64(snapshot, &["peer_count", "peerCount"]).or_else(|| {
        snapshot
            .as_object()
            .and_then(|map| map.get("peers"))
            .and_then(serde_json::Value::as_array)
            .map(|peers| peers.len() as u64)
    })
}

pub(super) fn resolve_gateway_eth_sync_status(
    chain_id: u64,
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
) -> Result<GatewayEthSyncStatusV1> {
    let entries = collect_gateway_eth_chain_entries(
        eth_tx_index,
        eth_tx_index_store,
        chain_id,
        gateway_eth_query_scan_max(),
    )?;
    let local_current_block =
        resolve_gateway_eth_latest_block_number(chain_id, &entries, eth_tx_index_store)?;
    let local_starting_block = entries
        .iter()
        .map(|entry| entry.nonce)
        .min()
        .unwrap_or(local_current_block);

    // Precedence: local index baseline -> snapshot file -> env overrides.
    let mut peer_count = 0u64;
    let mut starting_block = local_starting_block;
    let mut current_block = local_current_block;
    let mut highest_block = local_current_block;

    if let Some(path) = gateway_eth_sync_status_path_from_env(chain_id) {
        if let Some(snapshot) = load_gateway_eth_sync_snapshot(path.as_path()) {
            let scoped = gateway_eth_sync_snapshot_for_chain(&snapshot, chain_id);
            if let Some(v) = gateway_eth_sync_snapshot_peer_count(scoped)
                .or_else(|| gateway_eth_sync_snapshot_peer_count(&snapshot))
            {
                peer_count = v;
            }
            if let Some(v) = gateway_eth_sync_snapshot_u64(
                scoped,
                &["starting_block", "startingBlock"],
            )
            .or_else(|| {
                gateway_eth_sync_snapshot_u64(&snapshot, &["starting_block", "startingBlock"])
            }) {
                starting_block = v;
            }
            if let Some(v) =
                gateway_eth_sync_snapshot_u64(scoped, &["current_block", "currentBlock"]).or_else(
                    || gateway_eth_sync_snapshot_u64(&snapshot, &["current_block", "currentBlock"]),
                )
            {
                current_block = v;
            }
            if let Some(v) =
                gateway_eth_sync_snapshot_u64(scoped, &["highest_block", "highestBlock"]).or_else(
                    || gateway_eth_sync_snapshot_u64(&snapshot, &["highest_block", "highestBlock"]),
                )
            {
                highest_block = v;
            }
        }
    }

    if let Some(v) = gateway_eth_peer_count_from_env(chain_id) {
        peer_count = v;
    }
    if let Some(v) =
        gateway_eth_sync_env_u64_for_chain("NOVOVM_GATEWAY_ETH_SYNC_STARTING_BLOCK", chain_id)
    {
        starting_block = v;
    }
    if let Some(v) =
        gateway_eth_sync_env_u64_for_chain("NOVOVM_GATEWAY_ETH_SYNC_CURRENT_BLOCK", chain_id)
    {
        current_block = v;
    }
    if let Some(v) =
        gateway_eth_sync_env_u64_for_chain("NOVOVM_GATEWAY_ETH_SYNC_HIGHEST_BLOCK", chain_id)
    {
        highest_block = v;
    }

    current_block = current_block.max(local_current_block);
    if highest_block < current_block {
        highest_block = current_block;
    }
    if starting_block > current_block {
        starting_block = current_block;
    }

    Ok(GatewayEthSyncStatusV1 {
        peer_count,
        starting_block,
        current_block,
        highest_block,
        local_current_block,
    })
}

pub(super) fn gateway_eth_syncing_json(
    sync_status: GatewayEthSyncStatusV1,
    pending_block_number: Option<u64>,
) -> serde_json::Value {
    let _ = sync_status.local_current_block;
    let has_pending_block = pending_block_number.is_some();
    let mut highest_block = sync_status.highest_block;
    if let Some(pending_block_number) = pending_block_number {
        highest_block = highest_block.max(pending_block_number);
    }
    let current_block = sync_status.current_block.min(highest_block);
    if has_pending_block {
        highest_block = highest_block.max(current_block.saturating_add(1));
    }
    if current_block >= highest_block {
        return serde_json::Value::Bool(false);
    }
    let starting_block = sync_status.starting_block.min(current_block);
    serde_json::json!({
        "startingBlock": format!("0x{:x}", starting_block),
        "currentBlock": format!("0x{:x}", current_block),
        "highestBlock": format!("0x{:x}", highest_block),
    })
}
