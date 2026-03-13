#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkRuntimeSyncStatus {
    pub peer_count: u64,
    pub starting_block: u64,
    pub current_block: u64,
    pub highest_block: u64,
}

impl NetworkRuntimeSyncStatus {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        if self.highest_block < self.current_block {
            self.highest_block = self.current_block;
        }
        if self.starting_block > self.current_block {
            self.starting_block = self.current_block;
        }
        self
    }
}

static NETWORK_RUNTIME_SYNC_STATUS: OnceLock<Mutex<HashMap<u64, NetworkRuntimeSyncStatus>>> =
    OnceLock::new();
static NETWORK_RUNTIME_NATIVE_SYNC_STATUS: OnceLock<
    Mutex<HashMap<u64, NetworkRuntimeNativeSyncStatusV1>>,
> = OnceLock::new();
static NETWORK_RUNTIME_SYNC_OBSERVED_STATE: OnceLock<Mutex<NetworkRuntimeSyncObservedState>> =
    OnceLock::new();

fn runtime_sync_status_map() -> &'static Mutex<HashMap<u64, NetworkRuntimeSyncStatus>> {
    NETWORK_RUNTIME_SYNC_STATUS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_sync_status_map() -> &'static Mutex<HashMap<u64, NetworkRuntimeNativeSyncStatusV1>>
{
    NETWORK_RUNTIME_NATIVE_SYNC_STATUS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Default)]
struct NetworkRuntimeSyncObservedState {
    local_head_by_chain: HashMap<u64, u64>,
    peer_height_by_chain: HashMap<u64, HashMap<u64, u64>>,
    peer_last_seen_millis_by_chain: HashMap<u64, HashMap<u64, u128>>,
    peer_observed_once_by_chain: HashSet<u64>,
    native_peer_count_by_chain: HashMap<u64, u64>,
    native_remote_best_by_chain: HashMap<u64, u64>,
    native_snapshot_updated_at_by_chain: HashMap<u64, u128>,
    sync_anchor_by_chain: HashMap<u64, u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkRuntimeNativeSyncSnapshotV1 {
    pub peer_count: u64,
    pub local_head: u64,
    pub highest_head: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkRuntimeNativeSyncPhaseV1 {
    Idle,
    Discovery,
    Headers,
    Bodies,
    State,
    Finalize,
}

impl NetworkRuntimeNativeSyncPhaseV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Discovery => "discovery",
            Self::Headers => "headers",
            Self::Bodies => "bodies",
            Self::State => "state",
            Self::Finalize => "finalize",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "idle" => Some(Self::Idle),
            "discovery" => Some(Self::Discovery),
            "headers" => Some(Self::Headers),
            "bodies" => Some(Self::Bodies),
            "state" => Some(Self::State),
            "finalize" | "finalizing" | "finality" => Some(Self::Finalize),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkRuntimeNativeSyncStatusV1 {
    pub phase: NetworkRuntimeNativeSyncPhaseV1,
    pub peer_count: u64,
    pub starting_block: u64,
    pub current_block: u64,
    pub highest_block: u64,
    pub updated_at_unix_millis: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkRuntimeSyncPullWindowV1 {
    pub chain_id: u64,
    pub phase: NetworkRuntimeNativeSyncPhaseV1,
    pub peer_count: u64,
    pub current_block: u64,
    pub highest_block: u64,
    pub from_block: u64,
    pub to_block: u64,
}

impl NetworkRuntimeNativeSyncStatusV1 {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        if self.highest_block < self.current_block {
            self.highest_block = self.current_block;
        }
        if self.starting_block > self.current_block {
            self.starting_block = self.current_block;
        }
        if matches!(self.phase, NetworkRuntimeNativeSyncPhaseV1::Idle) {
            self.starting_block = self.current_block;
            self.highest_block = self.current_block;
        }
        self
    }
}

#[must_use]
pub fn network_runtime_native_sync_is_active(status: &NetworkRuntimeNativeSyncStatusV1) -> bool {
    !matches!(status.phase, NetworkRuntimeNativeSyncPhaseV1::Idle)
        || status.highest_block > status.current_block
}

fn runtime_sync_observed_state_map() -> &'static Mutex<NetworkRuntimeSyncObservedState> {
    NETWORK_RUNTIME_SYNC_OBSERVED_STATE
        .get_or_init(|| Mutex::new(NetworkRuntimeSyncObservedState::default()))
}

fn empty_runtime_sync_status() -> NetworkRuntimeSyncStatus {
    NetworkRuntimeSyncStatus {
        peer_count: 0,
        starting_block: 0,
        current_block: 0,
        highest_block: 0,
    }
}

const DEFAULT_RUNTIME_PEER_STALE_TIMEOUT_MILLIS: u128 = 30_000;
const NATIVE_SYNC_GAP_HEADERS_THRESHOLD: u64 = 8_192;
const NATIVE_SYNC_GAP_BODIES_THRESHOLD: u64 = 1_024;
const NATIVE_SYNC_GAP_STATE_THRESHOLD: u64 = 128;
const NATIVE_SYNC_GAP_FINALIZE_THRESHOLD: u64 = 8;
const NATIVE_SYNC_PULL_HEADERS_BATCH: u64 = 2_048;
const NATIVE_SYNC_PULL_BODIES_BATCH: u64 = 256;
const NATIVE_SYNC_PULL_STATE_BATCH: u64 = 64;
const NATIVE_SYNC_PULL_FINALIZE_BATCH: u64 = 16;

fn now_unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis()
}

fn native_sync_phase_from_runtime_status(
    status: &NetworkRuntimeSyncStatus,
) -> NetworkRuntimeNativeSyncPhaseV1 {
    if status.highest_block <= status.current_block {
        return NetworkRuntimeNativeSyncPhaseV1::Idle;
    }
    if status.peer_count == 0 {
        return NetworkRuntimeNativeSyncPhaseV1::Discovery;
    }
    let gap = status.highest_block.saturating_sub(status.current_block);
    if gap >= NATIVE_SYNC_GAP_HEADERS_THRESHOLD {
        return NetworkRuntimeNativeSyncPhaseV1::Headers;
    }
    if gap >= NATIVE_SYNC_GAP_BODIES_THRESHOLD {
        return NetworkRuntimeNativeSyncPhaseV1::Bodies;
    }
    if gap >= NATIVE_SYNC_GAP_STATE_THRESHOLD {
        return NetworkRuntimeNativeSyncPhaseV1::State;
    }
    if gap >= NATIVE_SYNC_GAP_FINALIZE_THRESHOLD {
        return NetworkRuntimeNativeSyncPhaseV1::Finalize;
    }
    NetworkRuntimeNativeSyncPhaseV1::Finalize
}

fn native_sync_pull_batch_by_phase(phase: NetworkRuntimeNativeSyncPhaseV1) -> Option<u64> {
    match phase {
        NetworkRuntimeNativeSyncPhaseV1::Idle | NetworkRuntimeNativeSyncPhaseV1::Discovery => None,
        NetworkRuntimeNativeSyncPhaseV1::Headers => Some(NATIVE_SYNC_PULL_HEADERS_BATCH),
        NetworkRuntimeNativeSyncPhaseV1::Bodies => Some(NATIVE_SYNC_PULL_BODIES_BATCH),
        NetworkRuntimeNativeSyncPhaseV1::State => Some(NATIVE_SYNC_PULL_STATE_BATCH),
        NetworkRuntimeNativeSyncPhaseV1::Finalize => Some(NATIVE_SYNC_PULL_FINALIZE_BATCH),
    }
}

#[must_use]
pub fn plan_network_runtime_sync_pull_window(
    chain_id: u64,
) -> Option<NetworkRuntimeSyncPullWindowV1> {
    let runtime = get_network_runtime_sync_status(chain_id)?;
    if runtime.peer_count == 0 || runtime.highest_block <= runtime.current_block {
        return None;
    }

    let phase = get_network_runtime_native_sync_status(chain_id)
        .filter(network_runtime_native_sync_is_active)
        .map(|status| status.phase)
        .unwrap_or_else(|| native_sync_phase_from_runtime_status(&runtime));
    let batch_size = native_sync_pull_batch_by_phase(phase)?;
    let from_block = runtime.current_block.saturating_add(1);
    if from_block > runtime.highest_block {
        return None;
    }
    let to_block = runtime
        .highest_block
        .min(from_block.saturating_add(batch_size.saturating_sub(1)));
    Some(NetworkRuntimeSyncPullWindowV1 {
        chain_id,
        phase,
        peer_count: runtime.peer_count,
        current_block: runtime.current_block,
        highest_block: runtime.highest_block,
        from_block,
        to_block,
    })
}

#[must_use]
pub fn reconcile_network_runtime_native_sync_status(
    chain_id: u64,
) -> Option<NetworkRuntimeNativeSyncStatusV1> {
    let runtime = get_network_runtime_sync_status(chain_id)?;
    let phase = native_sync_phase_from_runtime_status(&runtime);
    if matches!(phase, NetworkRuntimeNativeSyncPhaseV1::Idle) {
        return finish_network_runtime_native_sync(
            chain_id,
            runtime.peer_count,
            runtime.current_block,
        );
    }
    let native_current = get_network_runtime_native_sync_status(chain_id);
    if native_current.is_none() {
        let _ = begin_network_runtime_native_sync(
            chain_id,
            runtime.peer_count,
            runtime.current_block,
            runtime.highest_block,
        );
    }
    advance_network_runtime_native_sync(
        chain_id,
        phase,
        runtime.peer_count,
        runtime.current_block,
        runtime.highest_block,
    )
}

fn prune_stale_runtime_peers(chain_id: u64, observed: &mut NetworkRuntimeSyncObservedState) {
    let now = now_unix_millis();
    let stale_peer_ids: Vec<u64> = observed
        .peer_last_seen_millis_by_chain
        .get(&chain_id)
        .map(|m| {
            m.iter()
                .filter_map(|(peer_id, last_seen)| {
                    if now.saturating_sub(*last_seen) > DEFAULT_RUNTIME_PEER_STALE_TIMEOUT_MILLIS {
                        Some(*peer_id)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    if !stale_peer_ids.is_empty() {
        if let Some(peer_heights) = observed.peer_height_by_chain.get_mut(&chain_id) {
            for peer_id in &stale_peer_ids {
                peer_heights.remove(peer_id);
            }
            if peer_heights.is_empty() {
                observed.peer_height_by_chain.remove(&chain_id);
            }
        }
        if let Some(peer_last_seen) = observed.peer_last_seen_millis_by_chain.get_mut(&chain_id) {
            for peer_id in &stale_peer_ids {
                peer_last_seen.remove(peer_id);
            }
            if peer_last_seen.is_empty() {
                observed.peer_last_seen_millis_by_chain.remove(&chain_id);
            }
        }
    }

    // Native snapshot hints (peer_count/remote_best) are valid only for a short
    // period. When no fresh native snapshot arrives, drop them to avoid
    // stale-syncing false positives.
    if let Some(updated_at) = observed
        .native_snapshot_updated_at_by_chain
        .get(&chain_id)
        .copied()
    {
        if now.saturating_sub(updated_at) > DEFAULT_RUNTIME_PEER_STALE_TIMEOUT_MILLIS {
            observed.native_peer_count_by_chain.remove(&chain_id);
            observed.native_remote_best_by_chain.remove(&chain_id);
            observed
                .native_snapshot_updated_at_by_chain
                .remove(&chain_id);
        }
    }
}

fn recompute_runtime_sync_status_from_observed(
    chain_id: u64,
    statuses: &mut HashMap<u64, NetworkRuntimeSyncStatus>,
    observed: &mut NetworkRuntimeSyncObservedState,
) -> NetworkRuntimeSyncStatus {
    prune_stale_runtime_peers(chain_id, observed);

    let mut status = statuses
        .get(&chain_id)
        .copied()
        .unwrap_or_else(empty_runtime_sync_status);
    let peer_map = observed.peer_height_by_chain.get(&chain_id);
    let native_peer_count = observed
        .native_peer_count_by_chain
        .get(&chain_id)
        .copied()
        .unwrap_or(0);
    let native_remote_best = observed.native_remote_best_by_chain.get(&chain_id).copied();
    let has_peer_observation_history = observed.peer_observed_once_by_chain.contains(&chain_id);
    let remote_best = peer_map.and_then(|m| m.values().copied().max());
    let effective_remote_best = match (remote_best, native_remote_best) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    let local_head = observed.local_head_by_chain.get(&chain_id).copied();
    let observed_peer_count = peer_map.map(|m| m.len() as u64).unwrap_or(0);
    let effective_peer_count = observed_peer_count.max(native_peer_count);

    if has_peer_observation_history {
        status.peer_count = effective_peer_count;
    } else if effective_peer_count > 0 {
        status.peer_count = effective_peer_count;
    }
    status.current_block = local_head.unwrap_or(status.current_block);
    status.highest_block = if let Some(remote_best) = effective_remote_best {
        status.current_block.max(remote_best)
    } else if has_peer_observation_history {
        status.current_block
    } else {
        status.highest_block.max(status.current_block)
    };
    if status.highest_block > status.current_block {
        let existing_anchor = observed.sync_anchor_by_chain.get(&chain_id).copied();
        let mut start_anchor = existing_anchor
            .unwrap_or_else(|| {
                if status.starting_block > 0 && status.starting_block <= status.current_block {
                    status.starting_block
                } else {
                    status.current_block
                }
            })
            .min(status.current_block);
        // When runtime first learns a non-zero local head during syncing,
        // reset an old zero anchor to the real local start.
        if local_head.is_some() && status.current_block > 0 && start_anchor == 0 {
            start_anchor = status.current_block;
        }
        observed.sync_anchor_by_chain.insert(chain_id, start_anchor);
        status.starting_block = start_anchor;
    } else {
        observed.sync_anchor_by_chain.remove(&chain_id);
        status.starting_block = status.current_block;
    }
    let normalized = status.normalized();
    statuses.insert(chain_id, normalized);
    normalized
}

pub fn set_network_runtime_sync_status(chain_id: u64, status: NetworkRuntimeSyncStatus) {
    let normalized = status.normalized();
    if let Ok(mut guard) = runtime_sync_status_map().lock() {
        guard.insert(chain_id, normalized);
    }
    let _ = reconcile_network_runtime_native_sync_status(chain_id);
}

pub fn set_network_runtime_native_sync_status(
    chain_id: u64,
    status: NetworkRuntimeNativeSyncStatusV1,
) {
    let normalized = status.normalized();
    if let Ok(mut guard) = runtime_native_sync_status_map().lock() {
        guard.insert(chain_id, normalized);
    }
}

pub fn set_network_runtime_peer_count(chain_id: u64, peer_count: u64) {
    if let Ok(mut guard) = runtime_sync_status_map().lock() {
        let mut status = guard
            .get(&chain_id)
            .copied()
            .unwrap_or_else(empty_runtime_sync_status);
        status.peer_count = peer_count;
        guard.insert(chain_id, status.normalized());
    }
    let _ = reconcile_network_runtime_native_sync_status(chain_id);
}

pub fn set_network_runtime_block_progress(
    chain_id: u64,
    starting_block: u64,
    current_block: u64,
    highest_block: u64,
) {
    if let Ok(mut guard) = runtime_sync_status_map().lock() {
        let mut status = guard
            .get(&chain_id)
            .copied()
            .unwrap_or_else(empty_runtime_sync_status);
        status.starting_block = starting_block;
        status.current_block = current_block;
        status.highest_block = highest_block;
        guard.insert(chain_id, status.normalized());
    }
    let _ = reconcile_network_runtime_native_sync_status(chain_id);
}

#[must_use]
pub fn register_network_runtime_peer(
    chain_id: u64,
    peer_id: u64,
) -> Option<NetworkRuntimeSyncStatus> {
    let mut statuses = runtime_sync_status_map().lock().ok()?;
    let mut observed = runtime_sync_observed_state_map().lock().ok()?;
    let now = now_unix_millis();
    observed.native_peer_count_by_chain.remove(&chain_id);
    observed
        .peer_height_by_chain
        .entry(chain_id)
        .or_default()
        .entry(peer_id)
        .or_insert(0);
    observed.peer_observed_once_by_chain.insert(chain_id);
    observed
        .peer_last_seen_millis_by_chain
        .entry(chain_id)
        .or_default()
        .insert(peer_id, now);
    let recomputed =
        recompute_runtime_sync_status_from_observed(chain_id, &mut statuses, &mut observed);
    drop(observed);
    drop(statuses);
    let _ = reconcile_network_runtime_native_sync_status(chain_id);
    Some(recomputed)
}

#[must_use]
pub fn unregister_network_runtime_peer(
    chain_id: u64,
    peer_id: u64,
) -> Option<NetworkRuntimeSyncStatus> {
    let mut statuses = runtime_sync_status_map().lock().ok()?;
    let mut observed = runtime_sync_observed_state_map().lock().ok()?;
    observed.native_peer_count_by_chain.remove(&chain_id);
    if let Some(peers) = observed.peer_height_by_chain.get_mut(&chain_id) {
        peers.remove(&peer_id);
        if peers.is_empty() {
            observed.peer_height_by_chain.remove(&chain_id);
        }
    }
    if let Some(peer_last_seen) = observed.peer_last_seen_millis_by_chain.get_mut(&chain_id) {
        peer_last_seen.remove(&peer_id);
        if peer_last_seen.is_empty() {
            observed.peer_last_seen_millis_by_chain.remove(&chain_id);
        }
    }
    let recomputed =
        recompute_runtime_sync_status_from_observed(chain_id, &mut statuses, &mut observed);
    drop(observed);
    drop(statuses);
    let _ = reconcile_network_runtime_native_sync_status(chain_id);
    Some(recomputed)
}

#[must_use]
pub fn observe_network_runtime_peer_head(
    chain_id: u64,
    peer_id: u64,
    peer_head: u64,
) -> Option<NetworkRuntimeSyncStatus> {
    let mut statuses = runtime_sync_status_map().lock().ok()?;
    let mut observed = runtime_sync_observed_state_map().lock().ok()?;
    let now = now_unix_millis();
    observed.native_peer_count_by_chain.remove(&chain_id);
    observed
        .peer_height_by_chain
        .entry(chain_id)
        .or_default()
        .insert(peer_id, peer_head);
    observed.peer_observed_once_by_chain.insert(chain_id);
    observed
        .peer_last_seen_millis_by_chain
        .entry(chain_id)
        .or_default()
        .insert(peer_id, now);
    let recomputed =
        recompute_runtime_sync_status_from_observed(chain_id, &mut statuses, &mut observed);
    drop(observed);
    drop(statuses);
    let _ = reconcile_network_runtime_native_sync_status(chain_id);
    Some(recomputed)
}

#[must_use]
pub fn observe_network_runtime_local_head(
    chain_id: u64,
    local_head: u64,
) -> Option<NetworkRuntimeSyncStatus> {
    let mut statuses = runtime_sync_status_map().lock().ok()?;
    let mut observed = runtime_sync_observed_state_map().lock().ok()?;
    observed.local_head_by_chain.insert(chain_id, local_head);
    let recomputed =
        recompute_runtime_sync_status_from_observed(chain_id, &mut statuses, &mut observed);
    drop(observed);
    drop(statuses);
    let _ = reconcile_network_runtime_native_sync_status(chain_id);
    Some(recomputed)
}

#[must_use]
pub fn ingest_network_runtime_native_sync_snapshot(
    chain_id: u64,
    snapshot: NetworkRuntimeNativeSyncSnapshotV1,
) -> Option<NetworkRuntimeSyncStatus> {
    let mut statuses = runtime_sync_status_map().lock().ok()?;
    let mut observed = runtime_sync_observed_state_map().lock().ok()?;
    let now = now_unix_millis();
    observed
        .local_head_by_chain
        .insert(chain_id, snapshot.local_head);
    observed
        .native_peer_count_by_chain
        .insert(chain_id, snapshot.peer_count);
    observed
        .native_remote_best_by_chain
        .insert(chain_id, snapshot.highest_head.max(snapshot.local_head));
    observed
        .native_snapshot_updated_at_by_chain
        .insert(chain_id, now);
    observed.peer_observed_once_by_chain.insert(chain_id);
    let recomputed =
        recompute_runtime_sync_status_from_observed(chain_id, &mut statuses, &mut observed);
    drop(observed);
    drop(statuses);

    let _ = reconcile_network_runtime_native_sync_status(chain_id);

    Some(recomputed)
}

#[must_use]
pub fn observe_network_runtime_local_head_max(
    chain_id: u64,
    local_head: u64,
) -> Option<NetworkRuntimeSyncStatus> {
    let mut statuses = runtime_sync_status_map().lock().ok()?;
    let mut observed = runtime_sync_observed_state_map().lock().ok()?;
    let merged = observed
        .local_head_by_chain
        .get(&chain_id)
        .copied()
        .unwrap_or_else(|| {
            statuses
                .get(&chain_id)
                .map(|s| s.current_block)
                .unwrap_or_default()
        })
        .max(local_head);
    observed.local_head_by_chain.insert(chain_id, merged);
    let recomputed =
        recompute_runtime_sync_status_from_observed(chain_id, &mut statuses, &mut observed);
    drop(observed);
    drop(statuses);
    let _ = reconcile_network_runtime_native_sync_status(chain_id);
    Some(recomputed)
}

#[must_use]
pub fn get_network_runtime_sync_status(chain_id: u64) -> Option<NetworkRuntimeSyncStatus> {
    let mut statuses = runtime_sync_status_map().lock().ok()?;
    let mut observed = runtime_sync_observed_state_map().lock().ok()?;
    let has_observed = observed.local_head_by_chain.contains_key(&chain_id)
        || observed.peer_height_by_chain.contains_key(&chain_id);
    if !statuses.contains_key(&chain_id) && !has_observed {
        return None;
    }
    if !has_observed {
        return statuses.get(&chain_id).copied();
    }
    Some(recompute_runtime_sync_status_from_observed(
        chain_id,
        &mut statuses,
        &mut observed,
    ))
}

#[must_use]
pub fn get_network_runtime_native_sync_status(
    chain_id: u64,
) -> Option<NetworkRuntimeNativeSyncStatusV1> {
    let guard = runtime_native_sync_status_map().lock().ok()?;
    guard.get(&chain_id).copied()
}

#[must_use]
pub fn get_network_runtime_peer_heads(chain_id: u64) -> Vec<(u64, u64)> {
    let mut statuses = match runtime_sync_status_map().lock() {
        Ok(guard) => guard,
        Err(_) => return Vec::new(),
    };
    let mut observed = match runtime_sync_observed_state_map().lock() {
        Ok(guard) => guard,
        Err(_) => return Vec::new(),
    };
    let _ = recompute_runtime_sync_status_from_observed(chain_id, &mut statuses, &mut observed);
    let mut out = observed
        .peer_height_by_chain
        .get(&chain_id)
        .map(|m| m.iter().map(|(peer_id, head)| (*peer_id, *head)).collect())
        .unwrap_or_else(Vec::new);
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

#[must_use]
pub fn begin_network_runtime_native_sync(
    chain_id: u64,
    peer_count: u64,
    local_head: u64,
    highest_head: u64,
) -> Option<NetworkRuntimeNativeSyncStatusV1> {
    let mut guard = runtime_native_sync_status_map().lock().ok()?;
    let status = NetworkRuntimeNativeSyncStatusV1 {
        phase: NetworkRuntimeNativeSyncPhaseV1::Discovery,
        peer_count,
        starting_block: local_head,
        current_block: local_head,
        highest_block: highest_head.max(local_head),
        updated_at_unix_millis: now_unix_millis(),
    }
    .normalized();
    guard.insert(chain_id, status);
    Some(status)
}

#[must_use]
pub fn advance_network_runtime_native_sync(
    chain_id: u64,
    phase: NetworkRuntimeNativeSyncPhaseV1,
    peer_count: u64,
    current_head: u64,
    highest_head: u64,
) -> Option<NetworkRuntimeNativeSyncStatusV1> {
    let mut guard = runtime_native_sync_status_map().lock().ok()?;
    let previous = guard.get(&chain_id).copied();
    let starting_block = previous
        .map(|s| {
            if matches!(s.phase, NetworkRuntimeNativeSyncPhaseV1::Idle) {
                current_head
            } else {
                s.starting_block.min(current_head)
            }
        })
        .unwrap_or(current_head);
    let status = NetworkRuntimeNativeSyncStatusV1 {
        phase,
        peer_count,
        starting_block,
        current_block: current_head,
        highest_block: highest_head.max(current_head),
        updated_at_unix_millis: now_unix_millis(),
    }
    .normalized();
    guard.insert(chain_id, status);
    Some(status)
}

#[must_use]
pub fn finish_network_runtime_native_sync(
    chain_id: u64,
    peer_count: u64,
    local_head: u64,
) -> Option<NetworkRuntimeNativeSyncStatusV1> {
    let mut guard = runtime_native_sync_status_map().lock().ok()?;
    let status = NetworkRuntimeNativeSyncStatusV1 {
        phase: NetworkRuntimeNativeSyncPhaseV1::Idle,
        peer_count,
        starting_block: local_head,
        current_block: local_head,
        highest_block: local_head,
        updated_at_unix_millis: now_unix_millis(),
    }
    .normalized();
    guard.insert(chain_id, status);
    Some(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear_runtime_sync_status_for_test(chain_id: u64) {
        if let Ok(mut statuses) = runtime_sync_status_map().lock() {
            statuses.remove(&chain_id);
        }
        if let Ok(mut native) = runtime_native_sync_status_map().lock() {
            native.remove(&chain_id);
        }
        if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
            observed.local_head_by_chain.remove(&chain_id);
            observed.peer_height_by_chain.remove(&chain_id);
            observed.peer_last_seen_millis_by_chain.remove(&chain_id);
            observed.peer_observed_once_by_chain.remove(&chain_id);
            observed.native_peer_count_by_chain.remove(&chain_id);
            observed.native_remote_best_by_chain.remove(&chain_id);
            observed
                .native_snapshot_updated_at_by_chain
                .remove(&chain_id);
            observed.sync_anchor_by_chain.remove(&chain_id);
        }
    }

    #[test]
    fn runtime_sync_status_normalizes_fields() {
        clear_runtime_sync_status_for_test(1);
        let status = NetworkRuntimeSyncStatus {
            peer_count: 9,
            starting_block: 20,
            current_block: 10,
            highest_block: 8,
        };
        set_network_runtime_sync_status(1, status);
        let loaded = get_network_runtime_sync_status(1).expect("status should exist");
        assert_eq!(loaded.peer_count, 9);
        assert_eq!(loaded.starting_block, 10);
        assert_eq!(loaded.current_block, 10);
        assert_eq!(loaded.highest_block, 10);
    }

    #[test]
    fn runtime_peer_count_update_preserves_progress() {
        clear_runtime_sync_status_for_test(137);
        set_network_runtime_sync_status(
            137,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 11,
                current_block: 22,
                highest_block: 33,
            },
        );
        set_network_runtime_peer_count(137, 9);
        let loaded = get_network_runtime_sync_status(137).expect("status should exist");
        assert_eq!(loaded.peer_count, 9);
        assert_eq!(loaded.starting_block, 11);
        assert_eq!(loaded.current_block, 22);
        assert_eq!(loaded.highest_block, 33);
    }

    #[test]
    fn observe_peer_and_local_head_recomputes_runtime_status() {
        let chain_id = 2026_u64;
        clear_runtime_sync_status_for_test(chain_id);
        register_network_runtime_peer(chain_id, 10).expect("register peer");
        observe_network_runtime_peer_head(chain_id, 10, 88).expect("observe peer");
        let status_after_peer =
            get_network_runtime_sync_status(chain_id).expect("status after peer");
        assert_eq!(status_after_peer.peer_count, 1);
        assert_eq!(status_after_peer.current_block, 0);
        assert_eq!(status_after_peer.highest_block, 88);
        assert_eq!(status_after_peer.starting_block, 0);

        observe_network_runtime_local_head(chain_id, 77).expect("observe local");
        let status_after_local =
            get_network_runtime_sync_status(chain_id).expect("status after local");
        assert_eq!(status_after_local.current_block, 77);
        assert_eq!(status_after_local.highest_block, 88);
        assert_eq!(status_after_local.starting_block, 77);

        observe_network_runtime_peer_head(chain_id, 10, 120).expect("observe peer upgrade");
        let status_after_upgrade =
            get_network_runtime_sync_status(chain_id).expect("status after upgrade");
        assert_eq!(status_after_upgrade.highest_block, 120);
        assert_eq!(status_after_upgrade.starting_block, 77);

        observe_network_runtime_local_head(chain_id, 120).expect("observe local catch up");
        let status_after_catch_up =
            get_network_runtime_sync_status(chain_id).expect("status after catch-up");
        assert_eq!(status_after_catch_up.current_block, 120);
        assert_eq!(status_after_catch_up.highest_block, 120);
        assert_eq!(status_after_catch_up.starting_block, 120);
        unregister_network_runtime_peer(chain_id, 10).expect("unregister peer");
        let status_after_remove =
            get_network_runtime_sync_status(chain_id).expect("status after remove");
        assert_eq!(status_after_remove.peer_count, 0);
        assert_eq!(status_after_remove.current_block, 120);
        assert_eq!(status_after_remove.highest_block, 120);
    }

    #[test]
    fn unregister_peer_drops_highest_to_local_when_remote_disappears() {
        let chain_id = 2027_u64;
        clear_runtime_sync_status_for_test(chain_id);
        observe_network_runtime_local_head(chain_id, 10).expect("observe local head");
        register_network_runtime_peer(chain_id, 1).expect("register peer");
        observe_network_runtime_peer_head(chain_id, 1, 100).expect("observe remote head");
        let status_before_remove =
            get_network_runtime_sync_status(chain_id).expect("status before remove");
        assert_eq!(status_before_remove.current_block, 10);
        assert_eq!(status_before_remove.highest_block, 100);
        unregister_network_runtime_peer(chain_id, 1).expect("unregister peer");
        let status_after_remove =
            get_network_runtime_sync_status(chain_id).expect("status after remove");
        assert_eq!(status_after_remove.peer_count, 0);
        assert_eq!(status_after_remove.current_block, 10);
        assert_eq!(status_after_remove.highest_block, 10);
    }

    #[test]
    fn stale_peer_is_pruned_during_recompute() {
        let chain_id = 2028_u64;
        clear_runtime_sync_status_for_test(chain_id);
        observe_network_runtime_local_head(chain_id, 10).expect("observe local");
        register_network_runtime_peer(chain_id, 42).expect("register peer");
        observe_network_runtime_peer_head(chain_id, 42, 100).expect("observe peer");
        let before = get_network_runtime_sync_status(chain_id).expect("status before prune");
        assert_eq!(before.peer_count, 1);
        assert_eq!(before.highest_block, 100);

        if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
            observed
                .peer_last_seen_millis_by_chain
                .entry(chain_id)
                .or_default()
                .insert(42, 0);
        }

        observe_network_runtime_local_head(chain_id, 10).expect("trigger recompute");
        let after = get_network_runtime_sync_status(chain_id).expect("status after prune");
        assert_eq!(after.peer_count, 0);
        assert_eq!(after.current_block, 10);
        assert_eq!(after.highest_block, 10);
    }

    #[test]
    fn stale_peer_is_pruned_on_read_path() {
        let chain_id = 2029_u64;
        clear_runtime_sync_status_for_test(chain_id);
        observe_network_runtime_local_head(chain_id, 10).expect("observe local");
        register_network_runtime_peer(chain_id, 7).expect("register peer");
        observe_network_runtime_peer_head(chain_id, 7, 120).expect("observe peer");

        if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
            observed
                .peer_last_seen_millis_by_chain
                .entry(chain_id)
                .or_default()
                .insert(7, 0);
        }

        let status = get_network_runtime_sync_status(chain_id).expect("status on read");
        assert_eq!(status.peer_count, 0);
        assert_eq!(status.current_block, 10);
        assert_eq!(status.highest_block, 10);
    }

    #[test]
    fn observe_local_head_max_keeps_monotonic_progress() {
        let chain_id = 2030_u64;
        clear_runtime_sync_status_for_test(chain_id);
        observe_network_runtime_local_head_max(chain_id, 20).expect("observe local 20");
        observe_network_runtime_local_head_max(chain_id, 10).expect("observe local 10");
        let status = get_network_runtime_sync_status(chain_id).expect("status");
        assert_eq!(status.current_block, 20);
        assert_eq!(status.highest_block, 20);
    }

    #[test]
    fn native_sync_snapshot_updates_peer_count_and_heights() {
        let chain_id = 2031_u64;
        clear_runtime_sync_status_for_test(chain_id);
        ingest_network_runtime_native_sync_snapshot(
            chain_id,
            NetworkRuntimeNativeSyncSnapshotV1 {
                peer_count: 9,
                local_head: 120,
                highest_head: 150,
            },
        )
        .expect("ingest native snapshot");
        let status = get_network_runtime_sync_status(chain_id).expect("status");
        assert_eq!(status.peer_count, 9);
        assert_eq!(status.current_block, 120);
        assert_eq!(status.highest_block, 150);
        assert_eq!(status.starting_block, 120);
    }

    #[test]
    fn native_sync_snapshot_zero_peers_keeps_local_head() {
        let chain_id = 2032_u64;
        clear_runtime_sync_status_for_test(chain_id);
        ingest_network_runtime_native_sync_snapshot(
            chain_id,
            NetworkRuntimeNativeSyncSnapshotV1 {
                peer_count: 0,
                local_head: 88,
                highest_head: 88,
            },
        )
        .expect("ingest native snapshot");
        let status = get_network_runtime_sync_status(chain_id).expect("status");
        assert_eq!(status.peer_count, 0);
        assert_eq!(status.current_block, 88);
        assert_eq!(status.highest_block, 88);
        assert_eq!(status.starting_block, 88);
    }

    #[test]
    fn native_sync_stage_flow_begin_advance_finish() {
        let chain_id = 2033_u64;
        clear_runtime_sync_status_for_test(chain_id);

        let begin =
            begin_network_runtime_native_sync(chain_id, 3, 120, 180).expect("begin native sync");
        assert_eq!(begin.phase, NetworkRuntimeNativeSyncPhaseV1::Discovery);
        assert_eq!(begin.starting_block, 120);
        assert_eq!(begin.current_block, 120);
        assert_eq!(begin.highest_block, 180);

        let advanced = advance_network_runtime_native_sync(
            chain_id,
            NetworkRuntimeNativeSyncPhaseV1::Headers,
            4,
            130,
            210,
        )
        .expect("advance native sync");
        assert_eq!(advanced.phase, NetworkRuntimeNativeSyncPhaseV1::Headers);
        assert_eq!(advanced.starting_block, 120);
        assert_eq!(advanced.current_block, 130);
        assert_eq!(advanced.highest_block, 210);
        assert!(network_runtime_native_sync_is_active(&advanced));

        let finished =
            finish_network_runtime_native_sync(chain_id, 2, 210).expect("finish native sync");
        assert_eq!(finished.phase, NetworkRuntimeNativeSyncPhaseV1::Idle);
        assert_eq!(finished.starting_block, 210);
        assert_eq!(finished.current_block, 210);
        assert_eq!(finished.highest_block, 210);
        assert!(!network_runtime_native_sync_is_active(&finished));
    }

    #[test]
    fn runtime_reconcile_auto_drives_native_sync_phase_by_gap() {
        let chain_id = 2034_u64;
        clear_runtime_sync_status_for_test(chain_id);

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 4,
                starting_block: 100,
                current_block: 100,
                highest_block: 20_000,
            },
        );
        let headers = get_network_runtime_native_sync_status(chain_id).expect("native headers");
        assert_eq!(headers.phase, NetworkRuntimeNativeSyncPhaseV1::Headers);
        assert!(network_runtime_native_sync_is_active(&headers));

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 4,
                starting_block: 100,
                current_block: 18_900,
                highest_block: 20_000,
            },
        );
        let bodies = get_network_runtime_native_sync_status(chain_id).expect("native bodies");
        assert_eq!(bodies.phase, NetworkRuntimeNativeSyncPhaseV1::Bodies);

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 4,
                starting_block: 100,
                current_block: 19_850,
                highest_block: 20_000,
            },
        );
        let state = get_network_runtime_native_sync_status(chain_id).expect("native state");
        assert_eq!(state.phase, NetworkRuntimeNativeSyncPhaseV1::State);

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 4,
                starting_block: 100,
                current_block: 19_995,
                highest_block: 20_000,
            },
        );
        let finalize = get_network_runtime_native_sync_status(chain_id).expect("native finalize");
        assert_eq!(finalize.phase, NetworkRuntimeNativeSyncPhaseV1::Finalize);

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 4,
                starting_block: 100,
                current_block: 20_000,
                highest_block: 20_000,
            },
        );
        let idle = get_network_runtime_native_sync_status(chain_id).expect("native idle");
        assert_eq!(idle.phase, NetworkRuntimeNativeSyncPhaseV1::Idle);
        assert!(!network_runtime_native_sync_is_active(&idle));
    }

    #[test]
    fn runtime_reconcile_uses_discovery_when_gap_exists_but_no_peers() {
        let chain_id = 2035_u64;
        clear_runtime_sync_status_for_test(chain_id);

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 0,
                starting_block: 10,
                current_block: 10,
                highest_block: 200,
            },
        );
        let discovery = get_network_runtime_native_sync_status(chain_id).expect("native discovery");
        assert_eq!(discovery.phase, NetworkRuntimeNativeSyncPhaseV1::Discovery);
        assert!(network_runtime_native_sync_is_active(&discovery));
    }

    #[test]
    fn peer_register_unregister_keeps_native_remote_best_gap_hint() {
        let chain_id = 2036_u64;
        clear_runtime_sync_status_for_test(chain_id);

        ingest_network_runtime_native_sync_snapshot(
            chain_id,
            NetworkRuntimeNativeSyncSnapshotV1 {
                peer_count: 5,
                local_head: 100,
                highest_head: 160,
            },
        )
        .expect("ingest native snapshot");
        let status_initial = get_network_runtime_sync_status(chain_id).expect("status initial");
        assert_eq!(status_initial.peer_count, 5);
        assert_eq!(status_initial.current_block, 100);
        assert_eq!(status_initial.highest_block, 160);

        register_network_runtime_peer(chain_id, 42).expect("register peer");
        let status_after_register =
            get_network_runtime_sync_status(chain_id).expect("status after register");
        assert_eq!(status_after_register.peer_count, 1);
        assert_eq!(status_after_register.current_block, 100);
        assert_eq!(
            status_after_register.highest_block, 160,
            "known remote best should not collapse on peer register before head update"
        );

        unregister_network_runtime_peer(chain_id, 42).expect("unregister peer");
        let status_after_unregister =
            get_network_runtime_sync_status(chain_id).expect("status after unregister");
        assert_eq!(status_after_unregister.peer_count, 0);
        assert_eq!(status_after_unregister.current_block, 100);
        assert_eq!(
            status_after_unregister.highest_block, 160,
            "known remote best should remain as gap hint after peer unregister"
        );
    }

    #[test]
    fn stale_native_snapshot_hint_is_pruned_on_recompute() {
        let chain_id = 2037_u64;
        clear_runtime_sync_status_for_test(chain_id);

        ingest_network_runtime_native_sync_snapshot(
            chain_id,
            NetworkRuntimeNativeSyncSnapshotV1 {
                peer_count: 4,
                local_head: 50,
                highest_head: 120,
            },
        )
        .expect("ingest native snapshot");
        let before = get_network_runtime_sync_status(chain_id).expect("status before prune");
        assert_eq!(before.peer_count, 4);
        assert_eq!(before.current_block, 50);
        assert_eq!(before.highest_block, 120);

        if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
            observed
                .native_snapshot_updated_at_by_chain
                .insert(chain_id, 0);
        }

        observe_network_runtime_local_head(chain_id, 50).expect("trigger recompute");
        let after = get_network_runtime_sync_status(chain_id).expect("status after prune");
        assert_eq!(after.peer_count, 0);
        assert_eq!(after.current_block, 50);
        assert_eq!(after.highest_block, 50);
    }

    #[test]
    fn plan_sync_pull_window_none_without_gap_or_peers() {
        let chain_id = 2038_u64;
        clear_runtime_sync_status_for_test(chain_id);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 0,
                starting_block: 100,
                current_block: 100,
                highest_block: 200,
            },
        );
        assert!(plan_network_runtime_sync_pull_window(chain_id).is_none());

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 3,
                starting_block: 100,
                current_block: 200,
                highest_block: 200,
            },
        );
        assert!(plan_network_runtime_sync_pull_window(chain_id).is_none());
    }

    #[test]
    fn plan_sync_pull_window_uses_phase_batch_span() {
        let chain_id = 2039_u64;
        clear_runtime_sync_status_for_test(chain_id);

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 4,
                starting_block: 100,
                current_block: 100,
                highest_block: 30_000,
            },
        );
        let window_headers =
            plan_network_runtime_sync_pull_window(chain_id).expect("headers window");
        assert_eq!(
            window_headers.phase,
            NetworkRuntimeNativeSyncPhaseV1::Headers
        );
        assert_eq!(window_headers.from_block, 101);
        assert_eq!(
            window_headers.to_block,
            101 + NATIVE_SYNC_PULL_HEADERS_BATCH - 1
        );

        set_network_runtime_native_sync_status(
            chain_id,
            NetworkRuntimeNativeSyncStatusV1 {
                phase: NetworkRuntimeNativeSyncPhaseV1::Finalize,
                peer_count: 4,
                starting_block: 100,
                current_block: 29_990,
                highest_block: 30_000,
                updated_at_unix_millis: now_unix_millis(),
            },
        );
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 4,
                starting_block: 100,
                current_block: 29_990,
                highest_block: 30_000,
            },
        );
        let window_finalize =
            plan_network_runtime_sync_pull_window(chain_id).expect("finalize window");
        assert_eq!(
            window_finalize.phase,
            NetworkRuntimeNativeSyncPhaseV1::Finalize
        );
        assert_eq!(window_finalize.from_block, 29_991);
        assert_eq!(window_finalize.to_block, 30_000);
    }

    #[test]
    fn runtime_peer_heads_snapshot_prefers_higher_head_first() {
        let chain_id = 2040_u64;
        clear_runtime_sync_status_for_test(chain_id);

        register_network_runtime_peer(chain_id, 101).expect("register peer 101");
        register_network_runtime_peer(chain_id, 102).expect("register peer 102");
        observe_network_runtime_peer_head(chain_id, 101, 88).expect("observe peer 101");
        observe_network_runtime_peer_head(chain_id, 102, 144).expect("observe peer 102");

        let heads = get_network_runtime_peer_heads(chain_id);
        assert_eq!(heads.first().copied(), Some((102, 144)));
        assert!(heads.iter().any(|(peer, head)| *peer == 101 && *head == 88));
    }
}
