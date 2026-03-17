use super::*;
use novovm_network::{
    get_network_runtime_native_sync_status, get_network_runtime_sync_status,
    network_runtime_native_sync_is_active, observe_network_runtime_local_head_max,
    set_network_runtime_sync_status, NetworkRuntimeSyncStatus,
};

pub(super) fn resolve_gateway_eth_sync_status(
    chain_id: u64,
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
) -> Result<GatewayEthSyncStatusV1> {
    // Pull a small batch from native transport cache to keep runtime sync state
    // driven by real network traffic even when current request is read-only.
    poll_gateway_eth_public_broadcast_native_runtime(chain_id, 32);

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

    // Production precedence: runtime observed peers/status -> native active sync state
    // -> local index baseline.
    let mut peer_count = 0u64;
    let mut starting_block = local_starting_block;
    let mut current_block = local_current_block;
    let mut highest_block = local_current_block;
    if let Some(runtime_sync) = get_network_runtime_sync_status(chain_id) {
        peer_count = runtime_sync.peer_count;
        starting_block = runtime_sync.starting_block;
        current_block = runtime_sync.current_block;
        highest_block = runtime_sync.highest_block;
    }

    let runtime_native_sync = get_network_runtime_native_sync_status(chain_id);
    let has_runtime_native_sync = runtime_native_sync
        .as_ref()
        .is_some_and(network_runtime_native_sync_is_active);
    if let Some(native_sync) = runtime_native_sync.filter(network_runtime_native_sync_is_active) {
        peer_count = peer_count.max(native_sync.peer_count);
        starting_block = native_sync.starting_block;
        current_block = native_sync.current_block;
        highest_block = native_sync.highest_block;
    }

    if !has_runtime_native_sync {
        // Keep local baseline as authoritative source when native sync status is not active.
        let _ = observe_network_runtime_local_head_max(chain_id, local_current_block);
    }

    current_block = current_block.max(local_current_block);
    if highest_block < current_block {
        highest_block = current_block;
    }
    if starting_block > current_block {
        starting_block = current_block;
    }

    set_network_runtime_sync_status(
        chain_id,
        NetworkRuntimeSyncStatus {
            peer_count,
            starting_block,
            current_block,
            highest_block,
        },
    );

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
    _pending_block_number: Option<u64>,
) -> serde_json::Value {
    let _ = sync_status.local_current_block;
    let highest_block = sync_status.highest_block;
    let current_block = sync_status.current_block.min(highest_block);
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
