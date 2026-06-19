#![forbid(unsafe_code)]

use crate::{
    default_eth_fullnode_budget_hooks_v1,
    eth_rlpx::{
        eth_rlpx_validate_block_access_list_rlp_v1,
        eth_rlpx_validate_transaction_envelope_payload_v1,
    },
    EthFullnodeBudgetHooksV1, NovoRudpSequenceLifecycleLedger,
};
use novovm_protocol::{decode_nov_native_tx_wire_v1, NovTxKindV1};
use serde::{Deserialize, Serialize};
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
static NETWORK_RUNTIME_NATIVE_HEADER_SNAPSHOTS: OnceLock<
    Mutex<HashMap<u64, NetworkRuntimeNativeHeaderSnapshotV1>>,
> = OnceLock::new();
static NETWORK_RUNTIME_NATIVE_BODY_SNAPSHOTS: OnceLock<
    Mutex<HashMap<u64, NetworkRuntimeNativeBodySnapshotV1>>,
> = OnceLock::new();
type NetworkRuntimeNativeHeaderRlpByHashV1 = HashMap<[u8; 32], Vec<u8>>;
type NetworkRuntimeNativeHeaderRlpByChainV1 = HashMap<u64, NetworkRuntimeNativeHeaderRlpByHashV1>;
static NETWORK_RUNTIME_NATIVE_HEADER_RLPS: OnceLock<Mutex<NetworkRuntimeNativeHeaderRlpByChainV1>> =
    OnceLock::new();
type NetworkRuntimeNativeReceiptSnapshotByHashV1 =
    HashMap<[u8; 32], NetworkRuntimeNativeReceiptSnapshotV1>;
type NetworkRuntimeNativeReceiptSnapshotByChainV1 =
    HashMap<u64, NetworkRuntimeNativeReceiptSnapshotByHashV1>;
static NETWORK_RUNTIME_NATIVE_RECEIPT_SNAPSHOTS: OnceLock<
    Mutex<NetworkRuntimeNativeReceiptSnapshotByChainV1>,
> = OnceLock::new();
type NetworkRuntimeNativeBlockAccessListPayloadByHashV1 = HashMap<[u8; 32], Vec<u8>>;
type NetworkRuntimeNativeBlockAccessListPayloadByChainV1 =
    HashMap<u64, NetworkRuntimeNativeBlockAccessListPayloadByHashV1>;
static NETWORK_RUNTIME_NATIVE_BLOCK_ACCESS_LIST_PAYLOADS: OnceLock<
    Mutex<NetworkRuntimeNativeBlockAccessListPayloadByChainV1>,
> = OnceLock::new();
type NetworkRuntimeNativeSnapAccountKeyV1 = ([u8; 32], [u8; 32]);
type NetworkRuntimeNativeSnapAccountSnapshotByKeyV1 =
    HashMap<NetworkRuntimeNativeSnapAccountKeyV1, NetworkRuntimeNativeSnapAccountSnapshotV1>;
type NetworkRuntimeNativeSnapAccountSnapshotByChainV1 =
    HashMap<u64, NetworkRuntimeNativeSnapAccountSnapshotByKeyV1>;
static NETWORK_RUNTIME_NATIVE_SNAP_ACCOUNT_SNAPSHOTS: OnceLock<
    Mutex<NetworkRuntimeNativeSnapAccountSnapshotByChainV1>,
> = OnceLock::new();
type NetworkRuntimeNativeSnapStorageSnapshotByKeyV1 =
    HashMap<NetworkRuntimeNativeSnapAccountKeyV1, NetworkRuntimeNativeSnapAccountStorageSnapshotV1>;
type NetworkRuntimeNativeSnapStorageSnapshotByChainV1 =
    HashMap<u64, NetworkRuntimeNativeSnapStorageSnapshotByKeyV1>;
static NETWORK_RUNTIME_NATIVE_SNAP_STORAGE_SNAPSHOTS: OnceLock<
    Mutex<NetworkRuntimeNativeSnapStorageSnapshotByChainV1>,
> = OnceLock::new();
type NetworkRuntimeNativeSnapCodeSnapshotByHashV1 =
    HashMap<[u8; 32], NetworkRuntimeNativeSnapCodeSnapshotV1>;
type NetworkRuntimeNativeSnapCodeSnapshotByChainV1 =
    HashMap<u64, NetworkRuntimeNativeSnapCodeSnapshotByHashV1>;
static NETWORK_RUNTIME_NATIVE_SNAP_CODE_SNAPSHOTS: OnceLock<
    Mutex<NetworkRuntimeNativeSnapCodeSnapshotByChainV1>,
> = OnceLock::new();
type NetworkRuntimeNativeSnapTrieNodePathKeyV1 = ([u8; 32], Vec<u8>);
type NetworkRuntimeNativeSnapTrieNodeSnapshotByPathV1 =
    HashMap<NetworkRuntimeNativeSnapTrieNodePathKeyV1, NetworkRuntimeNativeSnapTrieNodeSnapshotV1>;
type NetworkRuntimeNativeSnapTrieNodeSnapshotByChainV1 =
    HashMap<u64, NetworkRuntimeNativeSnapTrieNodeSnapshotByPathV1>;
static NETWORK_RUNTIME_NATIVE_SNAP_TRIE_NODE_SNAPSHOTS: OnceLock<
    Mutex<NetworkRuntimeNativeSnapTrieNodeSnapshotByChainV1>,
> = OnceLock::new();
type NetworkRuntimeNativeSnapAccountRangeProgressByRootV1 =
    HashMap<[u8; 32], NetworkRuntimeNativeSnapAccountRangeProgressV1>;
type NetworkRuntimeNativeSnapAccountRangeProgressByChainV1 =
    HashMap<u64, NetworkRuntimeNativeSnapAccountRangeProgressByRootV1>;
static NETWORK_RUNTIME_NATIVE_SNAP_ACCOUNT_RANGE_PROGRESS: OnceLock<
    Mutex<NetworkRuntimeNativeSnapAccountRangeProgressByChainV1>,
> = OnceLock::new();
static NETWORK_RUNTIME_NATIVE_HEAD_SNAPSHOTS: OnceLock<
    Mutex<HashMap<u64, NetworkRuntimeNativeHeadSnapshotV1>>,
> = OnceLock::new();
static NETWORK_RUNTIME_NATIVE_CANONICAL_CHAINS: OnceLock<
    Mutex<HashMap<u64, NetworkRuntimeNativeCanonicalChainStateInternalV1>>,
> = OnceLock::new();
type NetworkRuntimeNativeHeaderSourcePeersByHashV1 = HashMap<[u8; 32], HashSet<u64>>;
type NetworkRuntimeNativeHeaderSourcePeersByChainV1 =
    HashMap<u64, NetworkRuntimeNativeHeaderSourcePeersByHashV1>;
static NETWORK_RUNTIME_NATIVE_HEADER_SOURCE_PEERS: OnceLock<
    Mutex<NetworkRuntimeNativeHeaderSourcePeersByChainV1>,
> = OnceLock::new();
type NetworkRuntimeNativePendingTxStateByHashV1 =
    HashMap<[u8; 32], NetworkRuntimeNativePendingTxStateV1>;
type NetworkRuntimeNativePendingTxPayloadByHashV1 = HashMap<[u8; 32], Vec<u8>>;
type NetworkRuntimeNativePendingTxTombstoneByHashV1 =
    HashMap<[u8; 32], NetworkRuntimeNativePendingTxTombstoneV1>;
type NetworkRuntimeNativePendingTxStateByChainV1 =
    HashMap<u64, NetworkRuntimeNativePendingTxStateByHashV1>;
type NetworkRuntimeNativePendingTxPayloadByChainV1 =
    HashMap<u64, NetworkRuntimeNativePendingTxPayloadByHashV1>;
type NetworkRuntimeNativePendingTxTombstoneByChainV1 =
    HashMap<u64, NetworkRuntimeNativePendingTxTombstoneByHashV1>;

static NETWORK_RUNTIME_NATIVE_PENDING_TXS: OnceLock<
    Mutex<NetworkRuntimeNativePendingTxStateByChainV1>,
> = OnceLock::new();
static NETWORK_RUNTIME_NATIVE_PENDING_TX_PAYLOADS: OnceLock<
    Mutex<NetworkRuntimeNativePendingTxPayloadByChainV1>,
> = OnceLock::new();
static NETWORK_RUNTIME_NATIVE_PENDING_TX_TOMBSTONES: OnceLock<
    Mutex<NetworkRuntimeNativePendingTxTombstoneByChainV1>,
> = OnceLock::new();
static NETWORK_RUNTIME_NATIVE_PENDING_TX_BROADCAST_RUNTIME: OnceLock<
    Mutex<HashMap<u64, NetworkRuntimeNativePendingTxBroadcastRuntimeSummaryV1>>,
> = OnceLock::new();
static NETWORK_RUNTIME_NATIVE_REPAIR_PROBE: OnceLock<
    Mutex<HashMap<u64, NetworkRuntimeNativeRepairProbeStateV1>>,
> = OnceLock::new();
static NETWORK_RUNTIME_NOVORUDP_SEQUENCE_LEDGER: OnceLock<
    Mutex<HashMap<u64, NovoRudpSequenceLifecycleLedger>>,
> = OnceLock::new();
static NETWORK_RUNTIME_NATIVE_EXECUTION_BUDGET_RUNTIME: OnceLock<
    Mutex<HashMap<u64, NetworkRuntimeNativeExecutionBudgetRuntimeSummaryV1>>,
> = OnceLock::new();
static NETWORK_RUNTIME_NATIVE_BUDGET_HOOKS: OnceLock<
    Mutex<HashMap<u64, EthFullnodeBudgetHooksV1>>,
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

fn runtime_native_header_snapshot_map(
) -> &'static Mutex<HashMap<u64, NetworkRuntimeNativeHeaderSnapshotV1>> {
    NETWORK_RUNTIME_NATIVE_HEADER_SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_body_snapshot_map(
) -> &'static Mutex<HashMap<u64, NetworkRuntimeNativeBodySnapshotV1>> {
    NETWORK_RUNTIME_NATIVE_BODY_SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_header_rlp_map() -> &'static Mutex<NetworkRuntimeNativeHeaderRlpByChainV1> {
    NETWORK_RUNTIME_NATIVE_HEADER_RLPS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_receipt_snapshot_map(
) -> &'static Mutex<NetworkRuntimeNativeReceiptSnapshotByChainV1> {
    NETWORK_RUNTIME_NATIVE_RECEIPT_SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_block_access_list_payload_map(
) -> &'static Mutex<NetworkRuntimeNativeBlockAccessListPayloadByChainV1> {
    NETWORK_RUNTIME_NATIVE_BLOCK_ACCESS_LIST_PAYLOADS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_snap_account_snapshot_map(
) -> &'static Mutex<NetworkRuntimeNativeSnapAccountSnapshotByChainV1> {
    NETWORK_RUNTIME_NATIVE_SNAP_ACCOUNT_SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_snap_storage_snapshot_map(
) -> &'static Mutex<NetworkRuntimeNativeSnapStorageSnapshotByChainV1> {
    NETWORK_RUNTIME_NATIVE_SNAP_STORAGE_SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_snap_code_snapshot_map(
) -> &'static Mutex<NetworkRuntimeNativeSnapCodeSnapshotByChainV1> {
    NETWORK_RUNTIME_NATIVE_SNAP_CODE_SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_snap_trie_node_snapshot_map(
) -> &'static Mutex<NetworkRuntimeNativeSnapTrieNodeSnapshotByChainV1> {
    NETWORK_RUNTIME_NATIVE_SNAP_TRIE_NODE_SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_snap_account_range_progress_map(
) -> &'static Mutex<NetworkRuntimeNativeSnapAccountRangeProgressByChainV1> {
    NETWORK_RUNTIME_NATIVE_SNAP_ACCOUNT_RANGE_PROGRESS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_head_snapshot_map(
) -> &'static Mutex<HashMap<u64, NetworkRuntimeNativeHeadSnapshotV1>> {
    NETWORK_RUNTIME_NATIVE_HEAD_SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_canonical_chain_map(
) -> &'static Mutex<HashMap<u64, NetworkRuntimeNativeCanonicalChainStateInternalV1>> {
    NETWORK_RUNTIME_NATIVE_CANONICAL_CHAINS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_header_source_peer_map(
) -> &'static Mutex<NetworkRuntimeNativeHeaderSourcePeersByChainV1> {
    NETWORK_RUNTIME_NATIVE_HEADER_SOURCE_PEERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn observe_network_runtime_native_header_source_peer_v1(
    chain_id: u64,
    block_hash: [u8; 32],
    source_peer_id: Option<u64>,
) {
    let Some(source_peer_id) = source_peer_id else {
        return;
    };
    if let Ok(mut guard) = runtime_native_header_source_peer_map().lock() {
        guard
            .entry(chain_id)
            .or_default()
            .entry(block_hash)
            .or_default()
            .insert(source_peer_id);
    }
}

fn runtime_native_pending_tx_map() -> &'static Mutex<NetworkRuntimeNativePendingTxStateByChainV1> {
    NETWORK_RUNTIME_NATIVE_PENDING_TXS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_pending_tx_payload_map(
) -> &'static Mutex<NetworkRuntimeNativePendingTxPayloadByChainV1> {
    NETWORK_RUNTIME_NATIVE_PENDING_TX_PAYLOADS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_pending_tx_tombstone_map(
) -> &'static Mutex<NetworkRuntimeNativePendingTxTombstoneByChainV1> {
    NETWORK_RUNTIME_NATIVE_PENDING_TX_TOMBSTONES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_pending_tx_broadcast_runtime_map(
) -> &'static Mutex<HashMap<u64, NetworkRuntimeNativePendingTxBroadcastRuntimeSummaryV1>> {
    NETWORK_RUNTIME_NATIVE_PENDING_TX_BROADCAST_RUNTIME.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_repair_probe_map(
) -> &'static Mutex<HashMap<u64, NetworkRuntimeNativeRepairProbeStateV1>> {
    NETWORK_RUNTIME_NATIVE_REPAIR_PROBE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_novorudp_sequence_ledger_map(
) -> &'static Mutex<HashMap<u64, NovoRudpSequenceLifecycleLedger>> {
    NETWORK_RUNTIME_NOVORUDP_SEQUENCE_LEDGER.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_execution_budget_runtime_map(
) -> &'static Mutex<HashMap<u64, NetworkRuntimeNativeExecutionBudgetRuntimeSummaryV1>> {
    NETWORK_RUNTIME_NATIVE_EXECUTION_BUDGET_RUNTIME.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_native_budget_hooks_map() -> &'static Mutex<HashMap<u64, EthFullnodeBudgetHooksV1>> {
    NETWORK_RUNTIME_NATIVE_BUDGET_HOOKS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[must_use]
pub fn get_network_runtime_native_budget_hooks_v1(chain_id: u64) -> EthFullnodeBudgetHooksV1 {
    runtime_native_budget_hooks_map()
        .lock()
        .ok()
        .and_then(|guard| guard.get(&chain_id).cloned())
        .unwrap_or_else(default_eth_fullnode_budget_hooks_v1)
}

pub fn set_network_runtime_native_budget_hooks_v1(chain_id: u64, budget: EthFullnodeBudgetHooksV1) {
    if let Ok(mut guard) = runtime_native_budget_hooks_map().lock() {
        guard.insert(chain_id, budget);
    }
}

#[derive(Debug, Default)]
struct NetworkRuntimeSyncObservedState {
    local_head_by_chain: HashMap<u64, u64>,
    peer_height_by_chain: HashMap<u64, HashMap<u64, u64>>,
    peer_last_seen_millis_by_chain: HashMap<u64, HashMap<u64, u128>>,
    next_stale_check_at_by_chain: HashMap<u64, u128>,
    dirty_chains: HashSet<u64>,
    peer_observed_once_by_chain: HashSet<u64>,
    native_peer_count_by_chain: HashMap<u64, u64>,
    native_remote_best_by_chain: HashMap<u64, u64>,
    native_snapshot_updated_at_by_chain: HashMap<u64, u128>,
    remote_best_hint_by_chain: HashMap<u64, u64>,
    remote_best_hint_updated_at_by_chain: HashMap<u64, u128>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRuntimeNativeHeaderSnapshotV1 {
    pub chain_id: u64,
    pub number: u64,
    pub hash: [u8; 32],
    pub parent_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub transactions_root: [u8; 32],
    pub receipts_root: [u8; 32],
    pub ommers_hash: [u8; 32],
    pub logs_bloom: Vec<u8>,
    pub gas_limit: Option<u64>,
    pub gas_used: Option<u64>,
    pub timestamp: Option<u64>,
    pub base_fee_per_gas: Option<u128>,
    pub withdrawals_root: Option<[u8; 32]>,
    pub blob_gas_used: Option<u64>,
    pub excess_blob_gas: Option<u64>,
    pub block_access_list_hash: Option<[u8; 32]>,
    pub source_peer_id: Option<u64>,
    pub observed_unix_ms: u128,
}

impl NetworkRuntimeNativeHeaderSnapshotV1 {
    #[must_use]
    pub fn normalized(mut self, chain_id: u64) -> Self {
        self.chain_id = chain_id;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRuntimeNativeBodySnapshotV1 {
    pub chain_id: u64,
    pub number: u64,
    pub block_hash: [u8; 32],
    pub tx_hashes: Vec<[u8; 32]>,
    pub raw_tx_rlps: Vec<Vec<u8>>,
    pub ommer_hashes: Vec<[u8; 32]>,
    pub withdrawal_rlp_items: Option<Vec<Vec<u8>>>,
    pub withdrawal_count: Option<usize>,
    pub body_available: bool,
    pub txs_materialized: bool,
    pub observed_unix_ms: u128,
}

impl NetworkRuntimeNativeBodySnapshotV1 {
    #[must_use]
    pub fn normalized(mut self, chain_id: u64) -> Self {
        self.chain_id = chain_id;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRuntimeNativeReceiptSnapshotV1 {
    pub chain_id: u64,
    pub number: u64,
    pub block_hash: [u8; 32],
    pub receipts_root: [u8; 32],
    pub raw_receipts: Vec<Vec<u8>>,
    pub receipt_count: usize,
    pub receipts_available: bool,
    pub source_peer_id: Option<u64>,
    pub observed_unix_ms: u128,
}

impl NetworkRuntimeNativeReceiptSnapshotV1 {
    #[must_use]
    pub fn normalized(mut self, chain_id: u64) -> Self {
        self.chain_id = chain_id;
        self.receipt_count = self.raw_receipts.len();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRuntimeNativeSnapAccountSnapshotV1 {
    pub chain_id: u64,
    pub state_root: [u8; 32],
    pub account_hash: [u8; 32],
    pub body_rlp: Vec<u8>,
    pub proof_nodes: Vec<Vec<u8>>,
    pub storage_root: Option<[u8; 32]>,
    pub code_hash: Option<[u8; 32]>,
    pub has_storage: bool,
    pub has_code: bool,
    pub source_peer_id: Option<u64>,
    pub observed_unix_ms: u128,
}

impl NetworkRuntimeNativeSnapAccountSnapshotV1 {
    #[must_use]
    pub fn normalized(mut self, chain_id: u64) -> Self {
        self.chain_id = chain_id;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRuntimeNativeSnapStorageSlotSnapshotV1 {
    pub hash: [u8; 32],
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRuntimeNativeSnapAccountStorageSnapshotV1 {
    pub chain_id: u64,
    pub state_root: [u8; 32],
    pub account_hash: [u8; 32],
    pub slots: Vec<NetworkRuntimeNativeSnapStorageSlotSnapshotV1>,
    pub proof_nodes: Vec<Vec<u8>>,
    pub source_peer_id: Option<u64>,
    pub observed_unix_ms: u128,
}

impl NetworkRuntimeNativeSnapAccountStorageSnapshotV1 {
    #[must_use]
    pub fn normalized(mut self, chain_id: u64) -> Self {
        self.chain_id = chain_id;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRuntimeNativeSnapCodeSnapshotV1 {
    pub chain_id: u64,
    pub code_hash: [u8; 32],
    pub code: Vec<u8>,
    pub source_peer_id: Option<u64>,
    pub observed_unix_ms: u128,
}

impl NetworkRuntimeNativeSnapCodeSnapshotV1 {
    #[must_use]
    pub fn normalized(mut self, chain_id: u64) -> Self {
        self.chain_id = chain_id;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRuntimeNativeSnapTrieNodeSnapshotV1 {
    pub chain_id: u64,
    pub state_root: [u8; 32],
    pub path_segments: Vec<Vec<u8>>,
    pub node_hash: [u8; 32],
    pub node_rlp: Vec<u8>,
    pub source_peer_id: Option<u64>,
    pub observed_unix_ms: u128,
}

impl NetworkRuntimeNativeSnapTrieNodeSnapshotV1 {
    #[must_use]
    pub fn normalized(mut self, chain_id: u64) -> Self {
        self.chain_id = chain_id;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRuntimeNativeSnapAccountRangeProgressV1 {
    pub chain_id: u64,
    pub state_root: [u8; 32],
    pub next_account_origin: Option<[u8; 32]>,
    pub limit: [u8; 32],
    pub completed: bool,
    pub source_peer_id: Option<u64>,
    pub observed_unix_ms: u128,
}

impl NetworkRuntimeNativeSnapAccountRangeProgressV1 {
    #[must_use]
    pub fn normalized(mut self, chain_id: u64) -> Self {
        self.chain_id = chain_id;
        if self.completed {
            self.next_account_origin = None;
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRuntimeNativeHeadSnapshotV1 {
    pub chain_id: u64,
    pub phase: NetworkRuntimeNativeSyncPhaseV1,
    pub peer_count: u64,
    pub block_number: u64,
    pub block_hash: [u8; 32],
    pub parent_block_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub canonical: bool,
    pub safe: bool,
    pub finalized: bool,
    pub reorg_depth_hint: Option<u64>,
    pub body_available: bool,
    pub source_peer_id: Option<u64>,
    pub observed_unix_ms: u128,
}

impl NetworkRuntimeNativeHeadSnapshotV1 {
    #[must_use]
    pub fn normalized(mut self, chain_id: u64) -> Self {
        self.chain_id = chain_id;
        if self.finalized {
            self.safe = true;
            self.canonical = true;
        } else if self.safe {
            self.canonical = true;
        }
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkRuntimeNativeCanonicalLifecycleStageV1 {
    Initial,
    Advanced,
    Reorg,
    Refreshed,
}

impl NetworkRuntimeNativeCanonicalLifecycleStageV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::Advanced => "advanced",
            Self::Reorg => "reorg",
            Self::Refreshed => "refreshed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkRuntimeNativeBlockLifecycleStageV1 {
    Seen,
    HeaderOnly,
    BodyReady,
    Canonical,
    NonCanonical,
    ReorgedOut,
}

impl NetworkRuntimeNativeBlockLifecycleStageV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Seen => "seen",
            Self::HeaderOnly => "header_only",
            Self::BodyReady => "body_ready",
            Self::Canonical => "canonical",
            Self::NonCanonical => "non_canonical",
            Self::ReorgedOut => "reorged_out",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NetworkRuntimeNativeBlockLifecycleSummaryV1 {
    pub seen_count: usize,
    pub header_only_count: usize,
    pub body_ready_count: usize,
    pub canonical_count: usize,
    pub non_canonical_count: usize,
    pub reorged_out_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkRuntimeNativeCanonicalBlockStateV1 {
    pub chain_id: u64,
    pub number: u64,
    pub hash: [u8; 32],
    pub parent_hash: [u8; 32],
    pub state_root: [u8; 32],
    #[serde(default)]
    pub transactions_root: Option<[u8; 32]>,
    pub header_observed: bool,
    pub body_available: bool,
    #[serde(default)]
    pub tx_hashes: Vec<[u8; 32]>,
    #[serde(default)]
    pub raw_tx_rlps: Vec<Vec<u8>>,
    #[serde(default)]
    pub ommer_hashes: Vec<[u8; 32]>,
    #[serde(default)]
    pub withdrawal_rlp_items: Option<Vec<Vec<u8>>>,
    #[serde(default)]
    pub withdrawal_count: Option<usize>,
    #[serde(default)]
    pub receipts_available: bool,
    #[serde(default)]
    pub receipt_count: Option<usize>,
    #[serde(default)]
    pub raw_receipts: Vec<Vec<u8>>,
    #[serde(default)]
    pub receipts_root: Option<[u8; 32]>,
    #[serde(default)]
    pub ommers_hash: Option<[u8; 32]>,
    #[serde(default)]
    pub state_root_validated: bool,
    #[serde(default)]
    pub state_root_validation_method: Option<String>,
    pub lifecycle_stage: NetworkRuntimeNativeBlockLifecycleStageV1,
    pub canonical: bool,
    pub safe: bool,
    pub finalized: bool,
    pub source_peer_id: Option<u64>,
    pub observed_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkRuntimeNativeCanonicalChainStateV1 {
    pub chain_id: u64,
    pub lifecycle_stage: NetworkRuntimeNativeCanonicalLifecycleStageV1,
    pub head: Option<NetworkRuntimeNativeCanonicalBlockStateV1>,
    pub retained_block_count: usize,
    pub canonical_block_count: usize,
    pub canonical_update_count: u64,
    pub reorg_count: u64,
    pub last_reorg_depth: Option<u64>,
    pub last_reorg_unix_ms: Option<u128>,
    pub last_head_change_unix_ms: Option<u128>,
    pub block_lifecycle_summary: NetworkRuntimeNativeBlockLifecycleSummaryV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkRuntimeNativePendingTxLifecycleStageV1 {
    Seen,
    Pending,
    Propagated,
    IncludedCanonical,
    IncludedNonCanonical,
    ReorgedBackToPending,
    Dropped,
    Rejected,
}

impl NetworkRuntimeNativePendingTxLifecycleStageV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Seen => "seen",
            Self::Pending => "pending",
            Self::Propagated => "propagated",
            Self::IncludedCanonical => "included_canonical",
            Self::IncludedNonCanonical => "included_non_canonical",
            Self::ReorgedBackToPending => "reorged_back_to_pending",
            Self::Dropped => "dropped",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkRuntimeNativePendingTxPropagationDispositionV1 {
    Propagated,
    Dropped,
    Rejected,
}

impl NetworkRuntimeNativePendingTxPropagationDispositionV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Propagated => "propagated",
            Self::Dropped => "dropped",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkRuntimeNativePendingTxPropagationRecoverabilityV1 {
    Recoverable,
    NonRecoverable,
}

impl NetworkRuntimeNativePendingTxPropagationRecoverabilityV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recoverable => "recoverable",
            Self::NonRecoverable => "non_recoverable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkRuntimeNativePendingTxRetrySuppressedReasonV1 {
    BudgetLimit,
    CoolingDown,
    NonRecoverable,
    IncludedCanonical,
}

impl NetworkRuntimeNativePendingTxRetrySuppressedReasonV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BudgetLimit => "budget_limit",
            Self::CoolingDown => "cooling_down",
            Self::NonRecoverable => "non_recoverable",
            Self::IncludedCanonical => "included_canonical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkRuntimeNativePendingTxFinalDispositionV1 {
    Retained,
    Evicted,
    Expired,
}

impl NetworkRuntimeNativePendingTxFinalDispositionV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Retained => "retained",
            Self::Evicted => "evicted",
            Self::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkRuntimeNativePendingTxPropagationStopReasonV1 {
    BudgetLimit,
    IoWriteFailure,
    NoAvailablePeer,
    PeerBackpressure,
    PhaseStall,
    TemporaryTimeout,
    InvalidEnvelope,
    InvalidTxPayload,
    DecodeFailure,
    UnsupportedTxType,
    PolicyRejected,
}

impl NetworkRuntimeNativePendingTxPropagationStopReasonV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BudgetLimit => "budget_limit",
            Self::IoWriteFailure => "io_write_failure",
            Self::NoAvailablePeer => "no_available_peer",
            Self::PeerBackpressure => "peer_backpressure",
            Self::PhaseStall => "phase_stall",
            Self::TemporaryTimeout => "temporary_timeout",
            Self::InvalidEnvelope => "invalid_envelope",
            Self::InvalidTxPayload => "invalid_tx_payload",
            Self::DecodeFailure => "decode_failure",
            Self::UnsupportedTxType => "unsupported_tx_type",
            Self::PolicyRejected => "policy_rejected",
        }
    }
}

#[inline]
fn runtime_native_pending_tx_stop_reason_recoverability_v1(
    reason: NetworkRuntimeNativePendingTxPropagationStopReasonV1,
) -> NetworkRuntimeNativePendingTxPropagationRecoverabilityV1 {
    match reason {
        NetworkRuntimeNativePendingTxPropagationStopReasonV1::BudgetLimit
        | NetworkRuntimeNativePendingTxPropagationStopReasonV1::IoWriteFailure
        | NetworkRuntimeNativePendingTxPropagationStopReasonV1::NoAvailablePeer
        | NetworkRuntimeNativePendingTxPropagationStopReasonV1::PeerBackpressure
        | NetworkRuntimeNativePendingTxPropagationStopReasonV1::PhaseStall
        | NetworkRuntimeNativePendingTxPropagationStopReasonV1::TemporaryTimeout => {
            NetworkRuntimeNativePendingTxPropagationRecoverabilityV1::Recoverable
        }
        NetworkRuntimeNativePendingTxPropagationStopReasonV1::InvalidEnvelope
        | NetworkRuntimeNativePendingTxPropagationStopReasonV1::InvalidTxPayload
        | NetworkRuntimeNativePendingTxPropagationStopReasonV1::DecodeFailure
        | NetworkRuntimeNativePendingTxPropagationStopReasonV1::UnsupportedTxType
        | NetworkRuntimeNativePendingTxPropagationStopReasonV1::PolicyRejected => {
            NetworkRuntimeNativePendingTxPropagationRecoverabilityV1::NonRecoverable
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NetworkRuntimeNativePendingTxOriginV1 {
    #[default]
    Unknown,
    Local,
    Remote,
}

impl NetworkRuntimeNativePendingTxOriginV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Local => "local",
            Self::Remote => "remote",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkRuntimeNativePendingTxStateV1 {
    pub chain_id: u64,
    pub tx_hash: [u8; 32],
    pub lifecycle_stage: NetworkRuntimeNativePendingTxLifecycleStageV1,
    #[serde(default)]
    pub origin: NetworkRuntimeNativePendingTxOriginV1,
    pub source_peer_id: Option<u64>,
    pub first_seen_unix_ms: u128,
    pub last_updated_unix_ms: u128,
    pub last_block_number: Option<u64>,
    pub last_block_hash: Option<[u8; 32]>,
    pub canonical_inclusion: Option<bool>,
    pub ingress_count: u64,
    pub propagation_count: u64,
    pub inclusion_count: u64,
    pub reorg_back_count: u64,
    pub drop_count: u64,
    pub reject_count: u64,
    #[serde(default)]
    pub propagation_attempt_count: u64,
    #[serde(default)]
    pub propagation_success_count: u64,
    #[serde(default)]
    pub propagation_failure_count: u64,
    #[serde(default)]
    pub propagated_peer_count: u64,
    #[serde(default)]
    pub last_propagation_unix_ms: Option<u128>,
    #[serde(default)]
    pub last_propagation_attempt_unix_ms: Option<u128>,
    #[serde(default)]
    pub last_propagation_failure_unix_ms: Option<u128>,
    #[serde(default)]
    pub last_propagation_failure_class: Option<String>,
    #[serde(default)]
    pub last_propagation_failure_phase: Option<String>,
    #[serde(default)]
    pub last_propagation_peer_id: Option<u64>,
    #[serde(default)]
    pub last_propagated_peer_id: Option<u64>,
    #[serde(default)]
    pub propagation_disposition: Option<NetworkRuntimeNativePendingTxPropagationDispositionV1>,
    #[serde(default)]
    pub propagation_stop_reason: Option<NetworkRuntimeNativePendingTxPropagationStopReasonV1>,
    #[serde(default)]
    pub propagation_recoverability:
        Option<NetworkRuntimeNativePendingTxPropagationRecoverabilityV1>,
    #[serde(default)]
    pub retry_eligible: bool,
    #[serde(default)]
    pub retry_after_unix_ms: Option<u128>,
    #[serde(default)]
    pub retry_backoff_level: u32,
    #[serde(default)]
    pub retry_suppressed_reason: Option<NetworkRuntimeNativePendingTxRetrySuppressedReasonV1>,
    #[serde(default = "runtime_native_pending_tx_final_disposition_default_v1")]
    pub pending_final_disposition: NetworkRuntimeNativePendingTxFinalDispositionV1,
}

fn runtime_native_pending_tx_final_disposition_default_v1(
) -> NetworkRuntimeNativePendingTxFinalDispositionV1 {
    NetworkRuntimeNativePendingTxFinalDispositionV1::Retained
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkRuntimeNativePendingTxTombstoneV1 {
    pub chain_id: u64,
    pub tx_hash: [u8; 32],
    pub lifecycle_stage: NetworkRuntimeNativePendingTxLifecycleStageV1,
    #[serde(default)]
    pub origin: NetworkRuntimeNativePendingTxOriginV1,
    pub final_disposition: NetworkRuntimeNativePendingTxFinalDispositionV1,
    #[serde(default)]
    pub propagation_disposition: Option<NetworkRuntimeNativePendingTxPropagationDispositionV1>,
    #[serde(default)]
    pub propagation_stop_reason: Option<NetworkRuntimeNativePendingTxPropagationStopReasonV1>,
    #[serde(default)]
    pub propagation_recoverability:
        Option<NetworkRuntimeNativePendingTxPropagationRecoverabilityV1>,
    pub last_updated_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NetworkRuntimeNativeRepairSequenceRangeV1 {
    pub start: u64,
    pub end_inclusive: u64,
    pub count: u64,
}

#[derive(Debug, Clone, Default)]
struct NetworkRuntimeNativeRepairProbeStateV1 {
    packet_received_count: u64,
    packet_decode_failed_count: u64,
    sequence_received_count: u64,
    sequence_received_min: Option<u64>,
    sequence_received_max: Option<u64>,
    sequence_seen: HashSet<u64>,
    tx_hash_seen: HashSet<[u8; 32]>,
    tx_hash_to_sequence: HashMap<[u8; 32], u64>,
    sequence_to_tx_hash: HashMap<u64, [u8; 32]>,
    repair_payload_by_tx_hash: HashMap<[u8; 32], Vec<u8>>,
    sequence_payload_index_seen: HashSet<u64>,
    sequence_payload_missing_seen: HashSet<u64>,
    sequence_duplicate_seen: HashSet<u64>,
    sequence_enqueued_seen: HashSet<u64>,
    sequence_already_receipted_seen: HashSet<u64>,
    sequence_admitted_to_aoem_seen: HashSet<u64>,
    sequence_actual_batch_seen: HashSet<u64>,
    sequence_receipt_written_seen: HashSet<u64>,
    sequence_attempted_unreceipted_seen: HashSet<u64>,
    sequence_attempted_unreceipted_requeued_seen: HashSet<u64>,
    sequence_accepted_count: u64,
    sequence_duplicate_count: u64,
    sequence_rejected_count: u64,
    sequence_already_receipted_count: u64,
    sequence_wrong_run_id_count: u64,
    sequence_wrong_chain_id_count: u64,
    sequence_out_of_range_count: u64,
    sequence_stale_count: u64,
    sequence_enqueued_count: u64,
    sequence_admitted_to_aoem_count: u64,
    final_missing_actual_batch_count: u64,
    final_missing_receipt_written_count: u64,
    attempted_unreceipted_count: u64,
    attempted_unreceipted_final_missing_overlap_count: u64,
    attempted_unreceipted_requeued_count: u64,
    attempted_unreceipted_requeue_failed_count: u64,
    final_missing_payload_available_count: u64,
    final_missing_payload_available_but_inactive_count: u64,
    final_missing_invariant_violation_count: u64,
    final_missing_sequence_to_tx_hash_count: u64,
    final_missing_tx_hash_payload_hit_count: u64,
    final_missing_payload_missing_by_sequence_count: u64,
    sequence_payload_index_evicted_count: u64,
    final_missing_payload_recovered_count: u64,
    final_missing_payload_recovered_requeued_count: u64,
    reject_reason_counts: HashMap<String, u64>,
    reject_reason_samples: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NetworkRuntimeNativePendingTxSummaryV1 {
    pub chain_id: u64,
    pub tx_count: usize,
    pub local_origin_count: usize,
    pub remote_origin_count: usize,
    pub unknown_origin_count: usize,
    pub seen_count: usize,
    pub pending_count: usize,
    pub propagated_count: usize,
    pub included_canonical_count: usize,
    pub included_non_canonical_count: usize,
    pub reorged_back_to_pending_count: usize,
    pub dropped_count: usize,
    pub rejected_count: usize,
    pub retry_eligible_count: usize,
    pub budget_suppressed_count: usize,
    pub io_write_failure_count: usize,
    pub non_recoverable_count: usize,
    pub propagation_attempt_total: u64,
    pub propagation_success_total: u64,
    pub propagation_failure_total: u64,
    pub propagated_peer_total: u64,
    pub evicted_count: usize,
    pub expired_count: usize,
    pub broadcast_dispatch_total: u64,
    pub broadcast_dispatch_success_total: u64,
    pub broadcast_dispatch_failed_total: u64,
    pub broadcast_candidate_tx_total: u64,
    pub broadcast_tx_total: u64,
    pub last_broadcast_peer_id: Option<u64>,
    pub last_broadcast_candidate_count: u64,
    pub last_broadcast_tx_count: u64,
    pub last_broadcast_unix_ms: Option<u128>,
    pub repair_packet_received_count: u64,
    pub repair_packet_decode_failed_count: u64,
    pub repair_sequence_received_count: u64,
    pub repair_sequence_received_min: Option<u64>,
    pub repair_sequence_received_max: Option<u64>,
    pub repair_sequence_received_ranges_sample: Vec<NetworkRuntimeNativeRepairSequenceRangeV1>,
    pub repair_sequence_accepted_ranges_sample: Vec<NetworkRuntimeNativeRepairSequenceRangeV1>,
    pub repair_sequence_enqueued_ranges_sample: Vec<NetworkRuntimeNativeRepairSequenceRangeV1>,
    pub repair_sequence_already_receipted_ranges_sample:
        Vec<NetworkRuntimeNativeRepairSequenceRangeV1>,
    pub repair_sequence_duplicate_ranges_sample: Vec<NetworkRuntimeNativeRepairSequenceRangeV1>,
    pub repair_sequence_admitted_to_aoem_ranges_sample:
        Vec<NetworkRuntimeNativeRepairSequenceRangeV1>,
    pub repair_sequence_accepted_count: u64,
    pub repair_sequence_duplicate_count: u64,
    pub repair_sequence_rejected_count: u64,
    pub repair_reject_reason_counts: HashMap<String, u64>,
    pub repair_reject_reason_samples: Vec<String>,
    pub repair_sequence_already_receipted_count: u64,
    pub repair_sequence_wrong_run_id_count: u64,
    pub repair_sequence_wrong_chain_id_count: u64,
    pub repair_sequence_out_of_range_count: u64,
    pub repair_sequence_stale_count: u64,
    pub repair_sequence_enqueued_count: u64,
    pub repair_sequence_admitted_to_aoem_count: u64,
    pub ledger_expected_range_start: Option<u64>,
    pub ledger_expected_range_end: Option<u64>,
    pub ledger_expected_count: u64,
    pub ledger_completed_count: u64,
    pub ledger_durable_missing_count: u64,
    pub ledger_durable_missing_ranges_sample: Vec<NetworkRuntimeNativeRepairSequenceRangeV1>,
    pub ledger_durable_missing_bitmap_available: bool,
    pub ledger_durable_missing_source: String,
    pub ledger_durable_missing_derived_from_expected_range: bool,
    pub ledger_missing_closed_by_receipt_count: u64,
    pub ledger_missing_closed_by_canonical_count: u64,
    pub ledger_missing_incorrectly_closed_by_received_count: u64,
    pub ledger_missing_incorrectly_closed_by_enqueued_count: u64,
    pub ledger_candidate_empty_but_durable_missing_count: u64,
    pub ledger_missing_without_candidate_count: u64,
    pub ledger_missing_without_retryable_count: u64,
    pub ledger_candidate_rehydrated_count: u64,
    pub ledger_candidate_count_exceeds_durable_missing_invariant_violation_count: u64,
    pub ledger_final_missing_without_durable_missing_count: u64,
    pub ledger_final_missing_actual_batch_count: u64,
    pub ledger_final_missing_actual_batch_ranges_sample:
        Vec<NetworkRuntimeNativeRepairSequenceRangeV1>,
    pub ledger_final_missing_raw_txs_count: u64,
    pub ledger_final_missing_batch_result_count: u64,
    pub ledger_final_missing_receipt_written_count: u64,
    pub ledger_final_missing_receipt_missing_after_admission_count: u64,
    pub ledger_final_missing_admitted_but_no_receipt_invariant_violation_count: u64,
    pub ledger_admission_counter_is_actual_batch: bool,
    pub ledger_admission_counter_mismatch_reason: String,
    pub repair_attempted_unreceipted_count: u64,
    pub repair_attempted_unreceipted_final_missing_overlap_count: u64,
    pub repair_attempted_unreceipted_requeued_count: u64,
    pub repair_attempted_unreceipted_requeue_failed_count: u64,
    pub repair_final_missing_payload_available_count: u64,
    pub repair_final_missing_payload_available_but_inactive_count: u64,
    pub repair_final_missing_invariant_violation_count: u64,
    pub repair_final_missing_sequence_to_tx_hash_count: u64,
    pub repair_final_missing_tx_hash_payload_hit_count: u64,
    pub repair_final_missing_payload_missing_by_sequence_count: u64,
    pub repair_final_missing_payload_missing_ranges_sample:
        Vec<NetworkRuntimeNativeRepairSequenceRangeV1>,
    pub repair_sequence_payload_index_count: u64,
    pub repair_sequence_payload_index_final_missing_overlap_count: u64,
    pub repair_sequence_payload_index_evicted_count: u64,
    pub repair_payload_retention_false_negative_suspected: bool,
    pub repair_final_missing_payload_recovered_count: u64,
    pub repair_final_missing_payload_recovered_requeued_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NetworkRuntimeNativePendingTxHistoryCompactionSummaryV1 {
    pub chain_id: u64,
    pub retain_recent: usize,
    pub historical_before: usize,
    pub historical_after: usize,
    pub included_before: usize,
    pub included_after: usize,
    pub dropped_before: usize,
    pub dropped_after: usize,
    pub rejected_before: usize,
    pub rejected_after: usize,
    pub historical_compacted_total: usize,
    pub included_compacted_total: usize,
    pub dropped_compacted_total: usize,
    pub rejected_compacted_total: usize,
    pub tombstones_inserted: usize,
    pub tombstone_retained_count: usize,
    pub tombstone_evicted_count: usize,
    pub historical_payload_bytes_freed: usize,
    pub included_payload_bytes_retained: usize,
    pub dropped_payload_bytes_retained: usize,
    pub historical_payload_bytes_retained: usize,
    pub receipt_index_backed_dedup_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkRuntimeNativePendingTxBroadcastCandidateV1 {
    pub chain_id: u64,
    pub tx_hash: [u8; 32],
    pub lifecycle_stage: NetworkRuntimeNativePendingTxLifecycleStageV1,
    pub propagation_count: u64,
    pub ingress_count: u64,
    pub last_updated_unix_ms: u128,
    pub tx_payload_len: usize,
    pub tx_payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NetworkRuntimeNativePendingTxBroadcastRuntimeSummaryV1 {
    pub chain_id: u64,
    pub dispatch_total: u64,
    pub dispatch_success_total: u64,
    pub dispatch_failed_total: u64,
    pub candidate_tx_total: u64,
    pub broadcast_tx_total: u64,
    pub last_peer_id: Option<u64>,
    pub last_candidate_count: u64,
    pub last_broadcast_tx_count: u64,
    pub last_updated_unix_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NetworkRuntimeNativeExecutionBudgetRuntimeSummaryV1 {
    pub chain_id: u64,
    pub execution_budget_hit_count: u64,
    pub execution_deferred_count: u64,
    pub execution_time_slice_exceeded_count: u64,
    pub hard_budget_per_tick: Option<u64>,
    pub hard_time_slice_ms: Option<u64>,
    pub target_budget_per_tick: Option<u64>,
    pub target_time_slice_ms: Option<u64>,
    pub effective_budget_per_tick: Option<u64>,
    pub effective_time_slice_ms: Option<u64>,
    pub last_execution_target_reason: Option<String>,
    pub last_execution_throttle_reason: Option<String>,
    pub last_updated_unix_ms: Option<u128>,
}

#[derive(Debug, Clone)]
struct NetworkRuntimeNativeCanonicalChainStateInternalV1 {
    snapshot: NetworkRuntimeNativeCanonicalChainStateV1,
    blocks_by_hash: HashMap<[u8; 32], NetworkRuntimeNativeCanonicalBlockStateV1>,
    canonical_hash_by_number: HashMap<u64, [u8; 32]>,
}

const NETWORK_RUNTIME_NATIVE_CANONICAL_CHAIN_RETENTION_MAX_V1: usize = 4096;

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

fn runtime_native_canonical_chain_internal_default_v1(
    chain_id: u64,
) -> NetworkRuntimeNativeCanonicalChainStateInternalV1 {
    NetworkRuntimeNativeCanonicalChainStateInternalV1 {
        snapshot: NetworkRuntimeNativeCanonicalChainStateV1 {
            chain_id,
            lifecycle_stage: NetworkRuntimeNativeCanonicalLifecycleStageV1::Initial,
            head: None,
            retained_block_count: 0,
            canonical_block_count: 0,
            canonical_update_count: 0,
            reorg_count: 0,
            last_reorg_depth: None,
            last_reorg_unix_ms: None,
            last_head_change_unix_ms: None,
            block_lifecycle_summary: NetworkRuntimeNativeBlockLifecycleSummaryV1::default(),
        },
        blocks_by_hash: HashMap::new(),
        canonical_hash_by_number: HashMap::new(),
    }
}

fn runtime_native_canonical_chain_infer_block_lifecycle_v1(
    block: &NetworkRuntimeNativeCanonicalBlockStateV1,
    canonical_hash_by_number: &HashMap<u64, [u8; 32]>,
) -> NetworkRuntimeNativeBlockLifecycleStageV1 {
    if canonical_hash_by_number
        .get(&block.number)
        .is_some_and(|hash| *hash == block.hash)
    {
        return NetworkRuntimeNativeBlockLifecycleStageV1::Canonical;
    }
    if matches!(
        block.lifecycle_stage,
        NetworkRuntimeNativeBlockLifecycleStageV1::ReorgedOut
    ) {
        return NetworkRuntimeNativeBlockLifecycleStageV1::ReorgedOut;
    }
    if canonical_hash_by_number.contains_key(&block.number) {
        return NetworkRuntimeNativeBlockLifecycleStageV1::NonCanonical;
    }
    if block.header_observed && block.body_available {
        return NetworkRuntimeNativeBlockLifecycleStageV1::BodyReady;
    }
    if block.header_observed {
        return NetworkRuntimeNativeBlockLifecycleStageV1::HeaderOnly;
    }
    NetworkRuntimeNativeBlockLifecycleStageV1::Seen
}

fn runtime_native_canonical_chain_refresh_snapshot_v1(
    state: &mut NetworkRuntimeNativeCanonicalChainStateInternalV1,
) {
    state.snapshot.retained_block_count = state.blocks_by_hash.len();
    state.snapshot.canonical_block_count = state.canonical_hash_by_number.len();
    let mut lifecycle_summary = NetworkRuntimeNativeBlockLifecycleSummaryV1::default();
    for block in state.blocks_by_hash.values() {
        match block.lifecycle_stage {
            NetworkRuntimeNativeBlockLifecycleStageV1::Seen => lifecycle_summary.seen_count += 1,
            NetworkRuntimeNativeBlockLifecycleStageV1::HeaderOnly => {
                lifecycle_summary.header_only_count += 1;
            }
            NetworkRuntimeNativeBlockLifecycleStageV1::BodyReady => {
                lifecycle_summary.body_ready_count += 1;
            }
            NetworkRuntimeNativeBlockLifecycleStageV1::Canonical => {
                lifecycle_summary.canonical_count += 1;
            }
            NetworkRuntimeNativeBlockLifecycleStageV1::NonCanonical => {
                lifecycle_summary.non_canonical_count += 1;
            }
            NetworkRuntimeNativeBlockLifecycleStageV1::ReorgedOut => {
                lifecycle_summary.reorged_out_count += 1;
            }
        }
    }
    state.snapshot.block_lifecycle_summary = lifecycle_summary;
}

#[inline]
fn runtime_native_pending_tx_stage_from_block_lifecycle_v1(
    stage: NetworkRuntimeNativeBlockLifecycleStageV1,
) -> NetworkRuntimeNativePendingTxLifecycleStageV1 {
    match stage {
        NetworkRuntimeNativeBlockLifecycleStageV1::Canonical => {
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
        }
        NetworkRuntimeNativeBlockLifecycleStageV1::NonCanonical
        | NetworkRuntimeNativeBlockLifecycleStageV1::ReorgedOut
        | NetworkRuntimeNativeBlockLifecycleStageV1::BodyReady
        | NetworkRuntimeNativeBlockLifecycleStageV1::HeaderOnly
        | NetworkRuntimeNativeBlockLifecycleStageV1::Seen => {
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedNonCanonical
        }
    }
}

fn runtime_native_pending_tx_summarize_v1(
    chain_id: u64,
    txs: &HashMap<[u8; 32], NetworkRuntimeNativePendingTxStateV1>,
) -> NetworkRuntimeNativePendingTxSummaryV1 {
    let mut summary = NetworkRuntimeNativePendingTxSummaryV1 {
        chain_id,
        tx_count: txs.len(),
        ..Default::default()
    };
    for tx in txs.values() {
        match tx.origin {
            NetworkRuntimeNativePendingTxOriginV1::Local => summary.local_origin_count += 1,
            NetworkRuntimeNativePendingTxOriginV1::Remote => summary.remote_origin_count += 1,
            NetworkRuntimeNativePendingTxOriginV1::Unknown => summary.unknown_origin_count += 1,
        }
        match tx.lifecycle_stage {
            NetworkRuntimeNativePendingTxLifecycleStageV1::Seen => summary.seen_count += 1,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Pending => summary.pending_count += 1,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Propagated => {
                summary.propagated_count += 1;
            }
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical => {
                summary.included_canonical_count += 1;
            }
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedNonCanonical => {
                summary.included_non_canonical_count += 1;
            }
            NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending => {
                summary.reorged_back_to_pending_count += 1;
            }
            NetworkRuntimeNativePendingTxLifecycleStageV1::Dropped => summary.dropped_count += 1,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Rejected => {
                summary.rejected_count += 1;
            }
        }
        if tx.retry_eligible {
            summary.retry_eligible_count += 1;
        }
        if matches!(
            tx.retry_suppressed_reason,
            Some(NetworkRuntimeNativePendingTxRetrySuppressedReasonV1::BudgetLimit)
        ) {
            summary.budget_suppressed_count += 1;
        }
        if matches!(
            tx.propagation_stop_reason,
            Some(NetworkRuntimeNativePendingTxPropagationStopReasonV1::IoWriteFailure)
        ) {
            summary.io_write_failure_count += 1;
        }
        if matches!(
            tx.propagation_recoverability,
            Some(NetworkRuntimeNativePendingTxPropagationRecoverabilityV1::NonRecoverable)
        ) {
            summary.non_recoverable_count += 1;
        }
        summary.propagation_attempt_total = summary
            .propagation_attempt_total
            .saturating_add(tx.propagation_attempt_count);
        summary.propagation_success_total = summary
            .propagation_success_total
            .saturating_add(tx.propagation_success_count);
        summary.propagation_failure_total = summary
            .propagation_failure_total
            .saturating_add(tx.propagation_failure_count);
        summary.propagated_peer_total = summary
            .propagated_peer_total
            .saturating_add(tx.propagated_peer_count);
    }
    summary
}

fn network_runtime_native_repair_sequence_ranges_sample_v1(
    sequences: &HashSet<u64>,
    limit: usize,
) -> Vec<NetworkRuntimeNativeRepairSequenceRangeV1> {
    if sequences.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut sorted = sequences.iter().copied().collect::<Vec<_>>();
    sorted.sort_unstable();
    let mut out = Vec::new();
    let mut start = sorted[0];
    let mut last = sorted[0];
    for sequence in sorted.into_iter().skip(1) {
        if sequence == last.saturating_add(1) {
            last = sequence;
            continue;
        }
        out.push(NetworkRuntimeNativeRepairSequenceRangeV1 {
            start,
            end_inclusive: last,
            count: last.saturating_sub(start).saturating_add(1),
        });
        if out.len() >= limit {
            return out;
        }
        start = sequence;
        last = sequence;
    }
    if out.len() < limit {
        out.push(NetworkRuntimeNativeRepairSequenceRangeV1 {
            start,
            end_inclusive: last,
            count: last.saturating_sub(start).saturating_add(1),
        });
    }
    out
}

fn observe_novorudp_sequence_repair_received_v1(
    chain_id: u64,
    sequence: u64,
    tx_hash: [u8; 32],
    payload_retained: bool,
) {
    if let Ok(mut guard) = runtime_novorudp_sequence_ledger_map().lock() {
        let ledger = guard
            .entry(chain_id)
            .or_insert_with(|| NovoRudpSequenceLifecycleLedger::new(0, 64));
        if ledger.expected_total <= sequence {
            ledger.expected_total = sequence.saturating_add(1);
            ledger.mark_expected_range(0, sequence, now_unix_millis(), "runtime_extend_expected");
        }
        ledger.observe_repair_received(sequence, tx_hash, payload_retained, now_unix_millis());
    }
}

pub fn ensure_runtime_novorudp_sequence_expected_total_v1(
    chain_id: u64,
    expected_total: u64,
    window_size: u64,
) {
    if expected_total == 0 {
        return;
    }
    if let Ok(mut guard) = runtime_novorudp_sequence_ledger_map().lock() {
        let ledger = guard.entry(chain_id).or_insert_with(|| {
            NovoRudpSequenceLifecycleLedger::new(expected_total, window_size.max(1))
        });
        ledger.ensure_expected_total(expected_total, now_unix_millis(), "runtime_expected_total");
    }
}

pub fn observe_runtime_novorudp_sequence_completion_prefix_v1(chain_id: u64, completed_total: u64) {
    if completed_total == 0 {
        return;
    }
    if let Ok(mut guard) = runtime_novorudp_sequence_ledger_map().lock() {
        let ledger = guard
            .entry(chain_id)
            .or_insert_with(|| NovoRudpSequenceLifecycleLedger::new(completed_total, 64));
        if ledger.expected_total < completed_total {
            ledger.ensure_expected_total(
                completed_total,
                now_unix_millis(),
                "runtime_completion_prefix_extend_expected",
            );
        }
        let now_ms = now_unix_millis();
        for sequence in 0..completed_total.min(ledger.expected_total) {
            ledger.mark_canonical_included(sequence, now_ms);
        }
    }
}

fn observe_novorudp_sequence_pending_view_v1(
    chain_id: u64,
    sequence: Option<u64>,
    already_receipted: bool,
) {
    let Some(sequence) = sequence else {
        return;
    };
    if let Ok(mut guard) = runtime_novorudp_sequence_ledger_map().lock() {
        let ledger = guard.entry(chain_id).or_insert_with(|| {
            NovoRudpSequenceLifecycleLedger::new(sequence.saturating_add(1), 64)
        });
        if ledger.expected_total <= sequence {
            ledger.expected_total = sequence.saturating_add(1);
            ledger.mark_expected_range(0, sequence, now_unix_millis(), "runtime_extend_expected");
        }
        if already_receipted {
            ledger.mark_receipt_written(sequence, now_unix_millis());
        } else {
            ledger.mark_pending_active(sequence, now_unix_millis(), "runtime_pending_view");
        }
    }
}

fn observe_novorudp_sequence_admitted_to_aoem_v1(chain_id: u64, sequence: Option<u64>) {
    let Some(sequence) = sequence else {
        return;
    };
    if let Ok(mut guard) = runtime_novorudp_sequence_ledger_map().lock() {
        let ledger = guard.entry(chain_id).or_insert_with(|| {
            NovoRudpSequenceLifecycleLedger::new(sequence.saturating_add(1), 64)
        });
        if ledger.expected_total <= sequence {
            ledger.expected_total = sequence.saturating_add(1);
            ledger.mark_expected_range(0, sequence, now_unix_millis(), "runtime_extend_expected");
        }
        ledger.mark_admitted_to_aoem(sequence, now_unix_millis());
    }
}

fn network_runtime_native_repair_probe_summary_v1(
    chain_id: u64,
) -> NetworkRuntimeNativePendingTxSummaryV1 {
    let Ok(guard) = runtime_native_repair_probe_map().lock() else {
        return NetworkRuntimeNativePendingTxSummaryV1 {
            chain_id,
            ..Default::default()
        };
    };
    let Some(state) = guard.get(&chain_id) else {
        return NetworkRuntimeNativePendingTxSummaryV1 {
            chain_id,
            ..Default::default()
        };
    };
    let admitted_seen_count: u64 = state
        .sequence_admitted_to_aoem_seen
        .len()
        .try_into()
        .unwrap_or(u64::MAX);
    let actual_batch_count = state.final_missing_actual_batch_count;
    let (ledger_admission_counter_is_actual_batch, ledger_admission_counter_mismatch_reason) =
        if admitted_seen_count > 0 && actual_batch_count == 0 {
            (false, "admitted_counter_without_actual_batch".to_string())
        } else if actual_batch_count > 0 && admitted_seen_count != actual_batch_count {
            (
                false,
                "admitted_counter_differs_from_actual_batch".to_string(),
            )
        } else {
            (true, String::new())
        };

    NetworkRuntimeNativePendingTxSummaryV1 {
        chain_id,
        repair_packet_received_count: state.packet_received_count,
        repair_packet_decode_failed_count: state.packet_decode_failed_count,
        repair_sequence_received_count: state.sequence_received_count,
        repair_sequence_received_min: state.sequence_received_min,
        repair_sequence_received_max: state.sequence_received_max,
        repair_sequence_received_ranges_sample:
            network_runtime_native_repair_sequence_ranges_sample_v1(&state.sequence_seen, 64),
        repair_sequence_accepted_ranges_sample:
            network_runtime_native_repair_sequence_ranges_sample_v1(&state.sequence_seen, 64),
        repair_sequence_enqueued_ranges_sample:
            network_runtime_native_repair_sequence_ranges_sample_v1(
                &state.sequence_enqueued_seen,
                64,
            ),
        repair_sequence_already_receipted_ranges_sample:
            network_runtime_native_repair_sequence_ranges_sample_v1(
                &state.sequence_already_receipted_seen,
                64,
            ),
        repair_sequence_duplicate_ranges_sample:
            network_runtime_native_repair_sequence_ranges_sample_v1(
                &state.sequence_duplicate_seen,
                64,
            ),
        repair_sequence_admitted_to_aoem_ranges_sample:
            network_runtime_native_repair_sequence_ranges_sample_v1(
                &state.sequence_admitted_to_aoem_seen,
                64,
            ),
        repair_sequence_accepted_count: state.sequence_accepted_count,
        repair_sequence_duplicate_count: state.sequence_duplicate_count,
        repair_sequence_rejected_count: state.sequence_rejected_count,
        repair_reject_reason_counts: state.reject_reason_counts.clone(),
        repair_reject_reason_samples: state.reject_reason_samples.clone(),
        repair_sequence_already_receipted_count: state.sequence_already_receipted_count,
        repair_sequence_wrong_run_id_count: state.sequence_wrong_run_id_count,
        repair_sequence_wrong_chain_id_count: state.sequence_wrong_chain_id_count,
        repair_sequence_out_of_range_count: state.sequence_out_of_range_count,
        repair_sequence_stale_count: state.sequence_stale_count,
        repair_sequence_enqueued_count: state.sequence_enqueued_count,
        repair_sequence_admitted_to_aoem_count: state.sequence_admitted_to_aoem_count,
        ledger_final_missing_actual_batch_count: state.final_missing_actual_batch_count,
        ledger_final_missing_actual_batch_ranges_sample:
            network_runtime_native_repair_sequence_ranges_sample_v1(
                &state.sequence_actual_batch_seen,
                64,
            ),
        ledger_final_missing_raw_txs_count: state.final_missing_actual_batch_count,
        ledger_final_missing_batch_result_count: state.final_missing_receipt_written_count,
        ledger_final_missing_receipt_written_count: state.final_missing_receipt_written_count,
        ledger_final_missing_receipt_missing_after_admission_count: state
            .sequence_admitted_to_aoem_seen
            .difference(&state.sequence_receipt_written_seen)
            .count()
            .try_into()
            .unwrap_or(u64::MAX),
        ledger_final_missing_admitted_but_no_receipt_invariant_violation_count: state
            .sequence_admitted_to_aoem_seen
            .difference(&state.sequence_receipt_written_seen)
            .count()
            .try_into()
            .unwrap_or(u64::MAX),
        ledger_admission_counter_is_actual_batch,
        ledger_admission_counter_mismatch_reason,
        repair_attempted_unreceipted_count: state.attempted_unreceipted_count,
        repair_attempted_unreceipted_final_missing_overlap_count: state
            .attempted_unreceipted_final_missing_overlap_count,
        repair_attempted_unreceipted_requeued_count: state.attempted_unreceipted_requeued_count,
        repair_attempted_unreceipted_requeue_failed_count: state
            .attempted_unreceipted_requeue_failed_count,
        repair_final_missing_payload_available_count: state.final_missing_payload_available_count,
        repair_final_missing_payload_available_but_inactive_count: state
            .final_missing_payload_available_but_inactive_count,
        repair_final_missing_invariant_violation_count: state
            .final_missing_invariant_violation_count,
        repair_final_missing_sequence_to_tx_hash_count: state
            .final_missing_sequence_to_tx_hash_count,
        repair_final_missing_tx_hash_payload_hit_count: state
            .final_missing_tx_hash_payload_hit_count,
        repair_final_missing_payload_missing_by_sequence_count: state
            .final_missing_payload_missing_by_sequence_count,
        repair_final_missing_payload_missing_ranges_sample:
            network_runtime_native_repair_sequence_ranges_sample_v1(
                &state.sequence_payload_missing_seen,
                64,
            ),
        repair_sequence_payload_index_count: state
            .sequence_payload_index_seen
            .len()
            .try_into()
            .unwrap_or(u64::MAX),
        repair_sequence_payload_index_final_missing_overlap_count: state
            .final_missing_tx_hash_payload_hit_count,
        repair_sequence_payload_index_evicted_count: state.sequence_payload_index_evicted_count,
        repair_payload_retention_false_negative_suspected: state
            .final_missing_payload_missing_by_sequence_count
            > 0,
        repair_final_missing_payload_recovered_count: state.final_missing_payload_recovered_count,
        repair_final_missing_payload_recovered_requeued_count: state
            .final_missing_payload_recovered_requeued_count,
        ..Default::default()
    }
}

fn apply_network_runtime_native_repair_probe_summary_v1(
    summary: &mut NetworkRuntimeNativePendingTxSummaryV1,
    repair: &NetworkRuntimeNativePendingTxSummaryV1,
) {
    summary.repair_packet_received_count = repair.repair_packet_received_count;
    summary.repair_packet_decode_failed_count = repair.repair_packet_decode_failed_count;
    summary.repair_sequence_received_count = repair.repair_sequence_received_count;
    summary.repair_sequence_received_min = repair.repair_sequence_received_min;
    summary.repair_sequence_received_max = repair.repair_sequence_received_max;
    summary.repair_sequence_received_ranges_sample =
        repair.repair_sequence_received_ranges_sample.clone();
    summary.repair_sequence_accepted_ranges_sample =
        repair.repair_sequence_accepted_ranges_sample.clone();
    summary.repair_sequence_enqueued_ranges_sample =
        repair.repair_sequence_enqueued_ranges_sample.clone();
    summary.repair_sequence_already_receipted_ranges_sample = repair
        .repair_sequence_already_receipted_ranges_sample
        .clone();
    summary.repair_sequence_duplicate_ranges_sample =
        repair.repair_sequence_duplicate_ranges_sample.clone();
    summary.repair_sequence_admitted_to_aoem_ranges_sample = repair
        .repair_sequence_admitted_to_aoem_ranges_sample
        .clone();
    summary.repair_sequence_accepted_count = repair.repair_sequence_accepted_count;
    summary.repair_sequence_duplicate_count = repair.repair_sequence_duplicate_count;
    summary.repair_sequence_rejected_count = repair.repair_sequence_rejected_count;
    summary.repair_reject_reason_counts = repair.repair_reject_reason_counts.clone();
    summary.repair_reject_reason_samples = repair.repair_reject_reason_samples.clone();
    summary.repair_sequence_already_receipted_count =
        repair.repair_sequence_already_receipted_count;
    summary.repair_sequence_wrong_run_id_count = repair.repair_sequence_wrong_run_id_count;
    summary.repair_sequence_wrong_chain_id_count = repair.repair_sequence_wrong_chain_id_count;
    summary.repair_sequence_out_of_range_count = repair.repair_sequence_out_of_range_count;
    summary.repair_sequence_stale_count = repair.repair_sequence_stale_count;
    summary.repair_sequence_enqueued_count = repair.repair_sequence_enqueued_count;
    summary.repair_sequence_admitted_to_aoem_count = repair.repair_sequence_admitted_to_aoem_count;
    summary.ledger_final_missing_actual_batch_count =
        repair.ledger_final_missing_actual_batch_count;
    summary.ledger_final_missing_actual_batch_ranges_sample = repair
        .ledger_final_missing_actual_batch_ranges_sample
        .clone();
    summary.ledger_final_missing_raw_txs_count = repair.ledger_final_missing_raw_txs_count;
    summary.ledger_final_missing_batch_result_count =
        repair.ledger_final_missing_batch_result_count;
    summary.ledger_final_missing_receipt_written_count =
        repair.ledger_final_missing_receipt_written_count;
    summary.ledger_final_missing_receipt_missing_after_admission_count =
        repair.ledger_final_missing_receipt_missing_after_admission_count;
    summary.ledger_final_missing_admitted_but_no_receipt_invariant_violation_count =
        repair.ledger_final_missing_admitted_but_no_receipt_invariant_violation_count;
    summary.ledger_admission_counter_is_actual_batch =
        repair.ledger_admission_counter_is_actual_batch;
    summary.ledger_admission_counter_mismatch_reason =
        repair.ledger_admission_counter_mismatch_reason.clone();
    summary.repair_attempted_unreceipted_count = repair.repair_attempted_unreceipted_count;
    summary.repair_attempted_unreceipted_final_missing_overlap_count =
        repair.repair_attempted_unreceipted_final_missing_overlap_count;
    summary.repair_attempted_unreceipted_requeued_count =
        repair.repair_attempted_unreceipted_requeued_count;
    summary.repair_attempted_unreceipted_requeue_failed_count =
        repair.repair_attempted_unreceipted_requeue_failed_count;
    summary.repair_final_missing_payload_available_count =
        repair.repair_final_missing_payload_available_count;
    summary.repair_final_missing_payload_available_but_inactive_count =
        repair.repair_final_missing_payload_available_but_inactive_count;
    summary.repair_final_missing_invariant_violation_count =
        repair.repair_final_missing_invariant_violation_count;
    summary.repair_final_missing_sequence_to_tx_hash_count =
        repair.repair_final_missing_sequence_to_tx_hash_count;
    summary.repair_final_missing_tx_hash_payload_hit_count =
        repair.repair_final_missing_tx_hash_payload_hit_count;
    summary.repair_final_missing_payload_missing_by_sequence_count =
        repair.repair_final_missing_payload_missing_by_sequence_count;
    summary.repair_final_missing_payload_missing_ranges_sample = repair
        .repair_final_missing_payload_missing_ranges_sample
        .clone();
    summary.repair_sequence_payload_index_count = repair.repair_sequence_payload_index_count;
    summary.repair_sequence_payload_index_final_missing_overlap_count =
        repair.repair_sequence_payload_index_final_missing_overlap_count;
    summary.repair_sequence_payload_index_evicted_count =
        repair.repair_sequence_payload_index_evicted_count;
    summary.repair_payload_retention_false_negative_suspected =
        repair.repair_payload_retention_false_negative_suspected;
    summary.repair_final_missing_payload_recovered_count =
        repair.repair_final_missing_payload_recovered_count;
    summary.repair_final_missing_payload_recovered_requeued_count =
        repair.repair_final_missing_payload_recovered_requeued_count;
}

fn apply_novorudp_durable_missing_summary_v1(
    summary: &mut NetworkRuntimeNativePendingTxSummaryV1,
    chain_id: u64,
) {
    let ledger = runtime_novorudp_sequence_ledger_map()
        .lock()
        .ok()
        .and_then(|guard| guard.get(&chain_id).cloned())
        .unwrap_or_default();
    let missing_ranges = ledger.ack_missing_bitmap();
    let durable_missing_count = missing_ranges
        .iter()
        .fold(0u64, |total, range| total.saturating_add(range.count()));
    let expected_count = ledger.expected_total;
    let completed_count = (0..ledger.expected_total)
        .filter(|sequence| {
            ledger
                .records
                .get(sequence)
                .is_some_and(|record| record.receipt_written || record.canonical_included)
        })
        .count()
        .try_into()
        .unwrap_or(u64::MAX);
    let receipt_closed_count = (0..ledger.expected_total)
        .filter(|sequence| {
            ledger
                .records
                .get(sequence)
                .is_some_and(|record| record.receipt_written)
        })
        .count()
        .try_into()
        .unwrap_or(u64::MAX);
    let canonical_closed_count = (0..ledger.expected_total)
        .filter(|sequence| {
            ledger
                .records
                .get(sequence)
                .is_some_and(|record| record.canonical_included)
        })
        .count()
        .try_into()
        .unwrap_or(u64::MAX);
    summary.ledger_expected_range_start = (expected_count > 0).then_some(0);
    summary.ledger_expected_range_end =
        (expected_count > 0).then_some(expected_count.saturating_sub(1));
    summary.ledger_expected_count = expected_count;
    summary.ledger_completed_count = completed_count;
    summary.ledger_durable_missing_count = durable_missing_count;
    summary.ledger_durable_missing_ranges_sample = missing_ranges
        .iter()
        .take(64)
        .map(|range| NetworkRuntimeNativeRepairSequenceRangeV1 {
            start: range.start,
            end_inclusive: range.end_inclusive,
            count: range.count(),
        })
        .collect();
    summary.ledger_durable_missing_bitmap_available =
        durable_missing_count > 0 && !summary.ledger_durable_missing_ranges_sample.is_empty();
    summary.ledger_durable_missing_source = if summary.ledger_durable_missing_bitmap_available {
        "sequence_lifecycle_ledger".to_string()
    } else {
        "none".to_string()
    };
    summary.ledger_durable_missing_derived_from_expected_range = expected_count > 0;
    summary.ledger_missing_closed_by_receipt_count = receipt_closed_count;
    summary.ledger_missing_closed_by_canonical_count = canonical_closed_count;
    summary.ledger_missing_incorrectly_closed_by_received_count = 0;
    summary.ledger_missing_incorrectly_closed_by_enqueued_count = 0;
    // Candidate/admission buckets live in a separate snapshot. Keep this
    // runtime summary as the durable ledger view; aggregate/wrapper layers
    // compute candidate-empty diagnostics once both views are available.
}

fn network_runtime_native_repair_probe_reject_v1(
    state: &mut NetworkRuntimeNativeRepairProbeStateV1,
    reason: &str,
) {
    state.sequence_rejected_count = state.sequence_rejected_count.saturating_add(1);
    *state
        .reject_reason_counts
        .entry(reason.to_string())
        .or_default() += 1;
    if state.reject_reason_samples.len() < 32 {
        state.reject_reason_samples.push(reason.to_string());
    }
    match reason {
        "decode_failed" => {
            state.packet_decode_failed_count = state.packet_decode_failed_count.saturating_add(1);
        }
        "wrong_chain_id" => {
            state.sequence_wrong_chain_id_count =
                state.sequence_wrong_chain_id_count.saturating_add(1);
        }
        "non_execute_tx" => {
            state.sequence_out_of_range_count = state.sequence_out_of_range_count.saturating_add(1);
        }
        _ => {}
    }
}

fn network_runtime_native_repair_probe_observe_pending_view_v1(
    chain_id: u64,
    tx_hash: [u8; 32],
    already_receipted: bool,
) {
    let Ok(mut guard) = runtime_native_repair_probe_map().lock() else {
        return;
    };
    let Some(state) = guard.get_mut(&chain_id) else {
        return;
    };
    if !state.tx_hash_seen.contains(&tx_hash) {
        return;
    }
    let sequence = state.tx_hash_to_sequence.get(&tx_hash).copied();
    if already_receipted {
        state.sequence_already_receipted_count =
            state.sequence_already_receipted_count.saturating_add(1);
        if let Some(sequence) = sequence {
            state.sequence_already_receipted_seen.insert(sequence);
        }
        observe_novorudp_sequence_pending_view_v1(chain_id, sequence, true);
        return;
    }
    state.sequence_enqueued_count = state.sequence_enqueued_count.saturating_add(1);
    if let Some(sequence) = sequence {
        state.sequence_enqueued_seen.insert(sequence);
    }
    observe_novorudp_sequence_pending_view_v1(chain_id, sequence, false);
}

fn network_runtime_native_repair_probe_tx_hashes_v1(chain_id: u64) -> HashSet<[u8; 32]> {
    runtime_native_repair_probe_map()
        .lock()
        .ok()
        .and_then(|guard| guard.get(&chain_id).map(|state| state.tx_hash_seen.clone()))
        .unwrap_or_default()
}

fn network_runtime_native_repair_probe_sequence_by_tx_hash_v1(
    chain_id: u64,
) -> HashMap<[u8; 32], u64> {
    runtime_native_repair_probe_map()
        .lock()
        .ok()
        .and_then(|guard| {
            guard
                .get(&chain_id)
                .map(|state| state.tx_hash_to_sequence.clone())
        })
        .unwrap_or_default()
}

pub fn observe_network_runtime_native_pending_tx_repair_probe_v1(
    chain_id: u64,
    tx_hash: [u8; 32],
    repair_copy_index: u64,
    tx_payload: Option<&[u8]>,
) {
    if repair_copy_index == 0 {
        return;
    }
    let Ok(mut guard) = runtime_native_repair_probe_map().lock() else {
        return;
    };
    let state = guard.entry(chain_id).or_default();
    state.packet_received_count = state.packet_received_count.saturating_add(1);
    let Some(payload) = tx_payload.filter(|payload| !payload.is_empty()) else {
        network_runtime_native_repair_probe_reject_v1(state, "missing_payload");
        return;
    };
    let Ok(tx) = decode_nov_native_tx_wire_v1(payload) else {
        network_runtime_native_repair_probe_reject_v1(state, "decode_failed");
        return;
    };
    if tx.chain_id != chain_id {
        network_runtime_native_repair_probe_reject_v1(state, "wrong_chain_id");
        return;
    }
    let sequence = match tx.kind {
        NovTxKindV1::Execute(execute) => execute.nonce.saturating_sub(1),
        NovTxKindV1::Transfer(_) | NovTxKindV1::Governance(_) => {
            network_runtime_native_repair_probe_reject_v1(state, "non_execute_tx");
            return;
        }
    };
    state.sequence_received_count = state.sequence_received_count.saturating_add(1);
    state.sequence_received_min = Some(
        state
            .sequence_received_min
            .map(|current| current.min(sequence))
            .unwrap_or(sequence),
    );
    state.sequence_received_max = Some(
        state
            .sequence_received_max
            .map(|current| current.max(sequence))
            .unwrap_or(sequence),
    );
    if !state.sequence_seen.insert(sequence) {
        state.sequence_duplicate_count = state.sequence_duplicate_count.saturating_add(1);
        state.sequence_duplicate_seen.insert(sequence);
    }
    state.tx_hash_seen.insert(tx_hash);
    state.tx_hash_to_sequence.insert(tx_hash, sequence);
    state.sequence_to_tx_hash.insert(sequence, tx_hash);
    state
        .repair_payload_by_tx_hash
        .insert(tx_hash, payload.to_vec());
    state.sequence_payload_index_seen.insert(sequence);
    state.sequence_accepted_count = state.sequence_accepted_count.saturating_add(1);
    observe_novorudp_sequence_repair_received_v1(chain_id, sequence, tx_hash, true);
}

pub fn observe_network_runtime_native_pending_tx_repair_aoem_admission_v1(
    chain_id: u64,
    tx_hash: [u8; 32],
) {
    let Ok(mut guard) = runtime_native_repair_probe_map().lock() else {
        return;
    };
    let Some(state) = guard.get_mut(&chain_id) else {
        return;
    };
    if !state.tx_hash_seen.contains(&tx_hash) {
        return;
    }
    state.sequence_admitted_to_aoem_count = state.sequence_admitted_to_aoem_count.saturating_add(1);
    let sequence = state.tx_hash_to_sequence.get(&tx_hash).copied();
    if let Some(sequence) = sequence {
        state.sequence_admitted_to_aoem_seen.insert(sequence);
    }
    observe_novorudp_sequence_admitted_to_aoem_v1(chain_id, sequence);
}

pub fn observe_network_runtime_native_pending_tx_repair_actual_batch_v1(
    chain_id: u64,
    tx_hash: [u8; 32],
) {
    let sequence = runtime_native_repair_probe_map()
        .lock()
        .ok()
        .and_then(|guard| {
            guard
                .get(&chain_id)
                .and_then(|state| state.tx_hash_to_sequence.get(&tx_hash).copied())
        });
    let Some(sequence) = sequence else {
        return;
    };
    if let Ok(mut guard) = runtime_native_repair_probe_map().lock() {
        let state = guard.entry(chain_id).or_default();
        if state.sequence_actual_batch_seen.insert(sequence) {
            state.final_missing_actual_batch_count =
                state.final_missing_actual_batch_count.saturating_add(1);
        }
        if state.sequence_admitted_to_aoem_seen.insert(sequence) {
            state.sequence_admitted_to_aoem_count =
                state.sequence_admitted_to_aoem_count.saturating_add(1);
        }
    }
    observe_novorudp_sequence_admitted_to_aoem_v1(chain_id, Some(sequence));
}

pub fn observe_network_runtime_native_pending_tx_repair_receipt_canonical_v1(
    chain_id: u64,
    tx_hash: [u8; 32],
) {
    let sequence = runtime_native_repair_probe_map()
        .lock()
        .ok()
        .and_then(|guard| {
            guard
                .get(&chain_id)
                .and_then(|state| state.tx_hash_to_sequence.get(&tx_hash).copied())
        });
    let Some(sequence) = sequence else {
        return;
    };
    if let Ok(mut guard) = runtime_novorudp_sequence_ledger_map().lock() {
        let ledger = guard.entry(chain_id).or_insert_with(|| {
            NovoRudpSequenceLifecycleLedger::new(sequence.saturating_add(1), 64)
        });
        if ledger.expected_total <= sequence {
            ledger.expected_total = sequence.saturating_add(1);
            ledger.mark_expected_range(0, sequence, now_unix_millis(), "runtime_extend_expected");
        }
        ledger.mark_canonical_included(sequence, now_unix_millis());
    }
    if let Ok(mut guard) = runtime_native_repair_probe_map().lock() {
        let state = guard.entry(chain_id).or_default();
        if state.sequence_receipt_written_seen.insert(sequence) {
            state.final_missing_receipt_written_count =
                state.final_missing_receipt_written_count.saturating_add(1);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NetworkRuntimeNativeRepairPendingRequeueSummaryV1 {
    pub chain_id: u64,
    pub final_missing_sequence_start: Option<u64>,
    pub repair_attempted_unreceipted_count: u64,
    pub repair_attempted_unreceipted_final_missing_overlap_count: u64,
    pub repair_attempted_unreceipted_requeued_count: u64,
    pub repair_attempted_unreceipted_requeue_failed_count: u64,
    pub repair_final_missing_payload_available_count: u64,
    pub repair_final_missing_payload_available_but_inactive_count: u64,
    pub repair_final_missing_invariant_violation_count: u64,
    pub repair_final_missing_sequence_to_tx_hash_count: u64,
    pub repair_final_missing_tx_hash_payload_hit_count: u64,
    pub repair_final_missing_payload_missing_by_sequence_count: u64,
    pub repair_final_missing_payload_missing_ranges_sample:
        Vec<NetworkRuntimeNativeRepairSequenceRangeV1>,
    pub repair_sequence_payload_index_count: u64,
    pub repair_sequence_payload_index_final_missing_overlap_count: u64,
    pub repair_sequence_payload_index_evicted_count: u64,
    pub repair_payload_retention_false_negative_suspected: bool,
    pub repair_final_missing_payload_recovered_count: u64,
    pub repair_final_missing_payload_recovered_requeued_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NetworkRuntimeNovoRudpLedgerFinalMissingAdmissionSnapshotV1 {
    pub chain_id: u64,
    pub final_missing_sequence_start: Option<u64>,
    pub ledger_final_missing_candidate_count: u64,
    pub ledger_final_missing_candidate_ranges_sample:
        Vec<NetworkRuntimeNativeRepairSequenceRangeV1>,
    pub ledger_final_missing_requeued_before_admission_count: u64,
    pub ledger_final_missing_admission_skipped_count: u64,
    pub ledger_final_missing_admission_skip_reason_counts: HashMap<String, u64>,
    pub admission_used_ledger_final_missing_bucket: bool,
    pub repair_requeue: NetworkRuntimeNativeRepairPendingRequeueSummaryV1,
    pub candidates: Vec<NetworkRuntimeNativePendingTxStateV1>,
}

#[inline]
fn runtime_native_pending_tx_active_stage_v1(
    stage: NetworkRuntimeNativePendingTxLifecycleStageV1,
) -> bool {
    matches!(
        stage,
        NetworkRuntimeNativePendingTxLifecycleStageV1::Seen
            | NetworkRuntimeNativePendingTxLifecycleStageV1::Pending
            | NetworkRuntimeNativePendingTxLifecycleStageV1::Propagated
            | NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending
    )
}

#[must_use]
pub fn requeue_network_runtime_native_repair_pending_for_final_missing_v1(
    chain_id: u64,
    final_missing_sequence_start: Option<u64>,
) -> NetworkRuntimeNativeRepairPendingRequeueSummaryV1 {
    let Some(start_sequence) = final_missing_sequence_start else {
        return NetworkRuntimeNativeRepairPendingRequeueSummaryV1 {
            chain_id,
            final_missing_sequence_start,
            ..Default::default()
        };
    };
    let now = now_unix_millis();
    let repair_state = runtime_native_repair_probe_map()
        .lock()
        .ok()
        .and_then(|guard| guard.get(&chain_id).cloned());
    let Some(repair_state) = repair_state else {
        return NetworkRuntimeNativeRepairPendingRequeueSummaryV1 {
            chain_id,
            final_missing_sequence_start,
            ..Default::default()
        };
    };
    let payload_available = runtime_native_pending_tx_payload_map()
        .lock()
        .ok()
        .and_then(|guard| guard.get(&chain_id).cloned())
        .unwrap_or_default();
    let mut summary = NetworkRuntimeNativeRepairPendingRequeueSummaryV1 {
        chain_id,
        final_missing_sequence_start,
        repair_sequence_payload_index_count: repair_state
            .sequence_payload_index_seen
            .len()
            .try_into()
            .unwrap_or(u64::MAX),
        ..Default::default()
    };
    let mut sequence_attempted_unreceipted_seen = Vec::<u64>::new();
    let mut sequence_requeued_seen = Vec::<u64>::new();
    let mut sequence_payload_missing_seen = Vec::<u64>::new();
    let mut restored_payloads = Vec::<([u8; 32], Vec<u8>)>::new();
    let mut sequence_requeue_failed_count = 0_u64;
    if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
        let chain_txs = guard.entry(chain_id).or_default();
        for (tx_hash, sequence) in repair_state.tx_hash_to_sequence.iter() {
            if *sequence < start_sequence {
                continue;
            }
            if repair_state.sequence_to_tx_hash.contains_key(sequence) {
                summary.repair_final_missing_sequence_to_tx_hash_count = summary
                    .repair_final_missing_sequence_to_tx_hash_count
                    .saturating_add(1);
            }
            if repair_state.sequence_payload_index_seen.contains(sequence) {
                summary.repair_sequence_payload_index_final_missing_overlap_count = summary
                    .repair_sequence_payload_index_final_missing_overlap_count
                    .saturating_add(1);
            }
            let pending_payload_exists = payload_available.contains_key(tx_hash);
            let repair_payload = repair_state.repair_payload_by_tx_hash.get(tx_hash);
            let payload_exists = pending_payload_exists || repair_payload.is_some();
            if payload_exists {
                summary.repair_final_missing_payload_available_count = summary
                    .repair_final_missing_payload_available_count
                    .saturating_add(1);
                summary.repair_final_missing_tx_hash_payload_hit_count = summary
                    .repair_final_missing_tx_hash_payload_hit_count
                    .saturating_add(1);
            } else {
                summary.repair_final_missing_payload_missing_by_sequence_count = summary
                    .repair_final_missing_payload_missing_by_sequence_count
                    .saturating_add(1);
                sequence_payload_missing_seen.push(*sequence);
            }
            let active = chain_txs
                .get(tx_hash)
                .is_some_and(|tx| runtime_native_pending_tx_active_stage_v1(tx.lifecycle_stage));
            let attempted = repair_state
                .sequence_admitted_to_aoem_seen
                .contains(sequence);
            if attempted {
                summary.repair_attempted_unreceipted_count =
                    summary.repair_attempted_unreceipted_count.saturating_add(1);
                summary.repair_attempted_unreceipted_final_missing_overlap_count = summary
                    .repair_attempted_unreceipted_final_missing_overlap_count
                    .saturating_add(1);
                sequence_attempted_unreceipted_seen.push(*sequence);
            }
            if active {
                if !pending_payload_exists {
                    if let Some(payload) = repair_payload {
                        restored_payloads.push((*tx_hash, payload.clone()));
                        summary.repair_final_missing_payload_recovered_count = summary
                            .repair_final_missing_payload_recovered_count
                            .saturating_add(1);
                    }
                }
                continue;
            }
            if payload_exists {
                summary.repair_final_missing_payload_available_but_inactive_count = summary
                    .repair_final_missing_payload_available_but_inactive_count
                    .saturating_add(1);
                let recovered_from_repair_index =
                    !pending_payload_exists && repair_payload.is_some();
                if let Some(payload) = repair_payload.filter(|_| !pending_payload_exists) {
                    restored_payloads.push((*tx_hash, payload.clone()));
                    summary.repair_final_missing_payload_recovered_count = summary
                        .repair_final_missing_payload_recovered_count
                        .saturating_add(1);
                }
                let tx = chain_txs.entry(*tx_hash).or_insert_with(|| {
                    NetworkRuntimeNativePendingTxStateV1 {
                        chain_id,
                        tx_hash: *tx_hash,
                        lifecycle_stage: NetworkRuntimeNativePendingTxLifecycleStageV1::Pending,
                        origin: NetworkRuntimeNativePendingTxOriginV1::Remote,
                        source_peer_id: None,
                        first_seen_unix_ms: now,
                        last_updated_unix_ms: now,
                        last_block_number: None,
                        last_block_hash: None,
                        canonical_inclusion: None,
                        ingress_count: 0,
                        propagation_count: 0,
                        inclusion_count: 0,
                        reorg_back_count: 0,
                        drop_count: 0,
                        reject_count: 0,
                        propagation_attempt_count: 0,
                        propagation_success_count: 0,
                        propagation_failure_count: 0,
                        propagated_peer_count: 0,
                        last_propagation_unix_ms: None,
                        last_propagation_attempt_unix_ms: None,
                        last_propagation_failure_unix_ms: None,
                        last_propagation_failure_class: None,
                        last_propagation_failure_phase: None,
                        last_propagation_peer_id: None,
                        last_propagated_peer_id: None,
                        propagation_disposition: None,
                        propagation_stop_reason: None,
                        propagation_recoverability: None,
                        retry_eligible: true,
                        retry_after_unix_ms: None,
                        retry_backoff_level: 0,
                        retry_suppressed_reason: None,
                        pending_final_disposition:
                            NetworkRuntimeNativePendingTxFinalDispositionV1::Retained,
                    }
                });
                tx.lifecycle_stage = NetworkRuntimeNativePendingTxLifecycleStageV1::Pending;
                tx.origin = NetworkRuntimeNativePendingTxOriginV1::Remote;
                tx.canonical_inclusion = None;
                tx.pending_final_disposition =
                    NetworkRuntimeNativePendingTxFinalDispositionV1::Retained;
                tx.last_updated_unix_ms = now;
                tx.ingress_count = tx.ingress_count.saturating_add(1);
                runtime_native_pending_tx_mark_retry_eligible_v1(tx);
                summary.repair_attempted_unreceipted_requeued_count = summary
                    .repair_attempted_unreceipted_requeued_count
                    .saturating_add(1);
                if recovered_from_repair_index {
                    summary.repair_final_missing_payload_recovered_requeued_count = summary
                        .repair_final_missing_payload_recovered_requeued_count
                        .saturating_add(1);
                }
                sequence_requeued_seen.push(*sequence);
            } else if attempted {
                sequence_requeue_failed_count = sequence_requeue_failed_count.saturating_add(1);
                summary.repair_attempted_unreceipted_requeue_failed_count = summary
                    .repair_attempted_unreceipted_requeue_failed_count
                    .saturating_add(1);
                summary.repair_final_missing_invariant_violation_count = summary
                    .repair_final_missing_invariant_violation_count
                    .saturating_add(1);
            }
        }
    }
    if !restored_payloads.is_empty() {
        if let Ok(mut guard) = runtime_native_pending_tx_payload_map().lock() {
            let chain_payloads = guard.entry(chain_id).or_default();
            for (tx_hash, payload) in restored_payloads {
                chain_payloads.insert(tx_hash, payload);
            }
        }
    }
    summary.repair_final_missing_payload_missing_ranges_sample =
        network_runtime_native_repair_sequence_ranges_sample_v1(
            &sequence_payload_missing_seen
                .iter()
                .copied()
                .collect::<HashSet<_>>(),
            64,
        );
    summary.repair_payload_retention_false_negative_suspected =
        summary.repair_final_missing_payload_missing_by_sequence_count > 0;
    if let Ok(mut guard) = runtime_native_repair_probe_map().lock() {
        let state = guard.entry(chain_id).or_default();
        state.attempted_unreceipted_count = state
            .attempted_unreceipted_count
            .saturating_add(summary.repair_attempted_unreceipted_count);
        state.attempted_unreceipted_final_missing_overlap_count = state
            .attempted_unreceipted_final_missing_overlap_count
            .saturating_add(summary.repair_attempted_unreceipted_final_missing_overlap_count);
        state.attempted_unreceipted_requeued_count = state
            .attempted_unreceipted_requeued_count
            .saturating_add(summary.repair_attempted_unreceipted_requeued_count);
        state.attempted_unreceipted_requeue_failed_count = state
            .attempted_unreceipted_requeue_failed_count
            .saturating_add(summary.repair_attempted_unreceipted_requeue_failed_count);
        state.final_missing_payload_available_count = state
            .final_missing_payload_available_count
            .saturating_add(summary.repair_final_missing_payload_available_count);
        state.final_missing_payload_available_but_inactive_count = state
            .final_missing_payload_available_but_inactive_count
            .saturating_add(summary.repair_final_missing_payload_available_but_inactive_count);
        state.final_missing_invariant_violation_count = state
            .final_missing_invariant_violation_count
            .saturating_add(summary.repair_final_missing_invariant_violation_count);
        state.final_missing_sequence_to_tx_hash_count = state
            .final_missing_sequence_to_tx_hash_count
            .saturating_add(summary.repair_final_missing_sequence_to_tx_hash_count);
        state.final_missing_tx_hash_payload_hit_count = state
            .final_missing_tx_hash_payload_hit_count
            .saturating_add(summary.repair_final_missing_tx_hash_payload_hit_count);
        state.final_missing_payload_missing_by_sequence_count = state
            .final_missing_payload_missing_by_sequence_count
            .saturating_add(summary.repair_final_missing_payload_missing_by_sequence_count);
        state.final_missing_payload_recovered_count = state
            .final_missing_payload_recovered_count
            .saturating_add(summary.repair_final_missing_payload_recovered_count);
        state.final_missing_payload_recovered_requeued_count = state
            .final_missing_payload_recovered_requeued_count
            .saturating_add(summary.repair_final_missing_payload_recovered_requeued_count);
        state
            .sequence_attempted_unreceipted_seen
            .extend(sequence_attempted_unreceipted_seen);
        state
            .sequence_attempted_unreceipted_requeued_seen
            .extend(sequence_requeued_seen);
        state
            .sequence_payload_missing_seen
            .extend(sequence_payload_missing_seen);
        if sequence_requeue_failed_count > 0 {
            *state
                .reject_reason_counts
                .entry("attempted_unreceipted_requeue_missing_payload".to_string())
                .or_default() += sequence_requeue_failed_count;
        }
    }
    summary
}

#[must_use]
pub fn snapshot_network_runtime_novorudp_ledger_final_missing_admission_candidates_v1(
    chain_id: u64,
    limit: usize,
    final_missing_sequence_start: Option<u64>,
) -> NetworkRuntimeNovoRudpLedgerFinalMissingAdmissionSnapshotV1 {
    let repair_requeue = requeue_network_runtime_native_repair_pending_for_final_missing_v1(
        chain_id,
        final_missing_sequence_start,
    );
    let Some(start_sequence) = final_missing_sequence_start else {
        return NetworkRuntimeNovoRudpLedgerFinalMissingAdmissionSnapshotV1 {
            chain_id,
            final_missing_sequence_start,
            repair_requeue,
            ..Default::default()
        };
    };
    runtime_native_pending_tx_cleanup_v1(chain_id, now_unix_millis());
    let mut skip_reason_counts = HashMap::<String, u64>::new();
    let mut ledger_entries = runtime_novorudp_sequence_ledger_map()
        .lock()
        .ok()
        .and_then(|guard| guard.get(&chain_id).cloned())
        .map(|ledger| {
            ledger
                .records
                .into_iter()
                .filter(|(sequence, record)| {
                    *sequence >= start_sequence
                        && record.expected
                        && record.payload_retained
                        && !record.receipt_written
                        && !record.canonical_included
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    ledger_entries.sort_by_key(|(sequence, _)| *sequence);

    let payloads = runtime_native_pending_tx_payload_map()
        .lock()
        .ok()
        .and_then(|guard| guard.get(&chain_id).cloned())
        .unwrap_or_default();
    let pending = runtime_native_pending_tx_map()
        .lock()
        .ok()
        .and_then(|guard| guard.get(&chain_id).cloned())
        .unwrap_or_default();

    let mut candidate_sequences = Vec::<u64>::new();
    let mut candidates = Vec::<NetworkRuntimeNativePendingTxStateV1>::new();
    for (sequence, record) in ledger_entries {
        let Some(tx_hash) = record.tx_hash else {
            *skip_reason_counts
                .entry("missing_tx_hash".to_string())
                .or_default() += 1;
            continue;
        };
        if !payloads.contains_key(&tx_hash) {
            *skip_reason_counts
                .entry("missing_payload".to_string())
                .or_default() += 1;
            continue;
        }
        let Some(pending_tx) = pending.get(&tx_hash) else {
            *skip_reason_counts
                .entry("missing_pending_state".to_string())
                .or_default() += 1;
            continue;
        };
        if !runtime_native_pending_tx_active_stage_v1(pending_tx.lifecycle_stage) {
            *skip_reason_counts
                .entry("inactive_pending_stage".to_string())
                .or_default() += 1;
            continue;
        }
        candidate_sequences.push(sequence);
        if limit == 0 || candidates.len() < limit {
            candidates.push(pending_tx.clone());
        }
    }
    let candidate_count = candidate_sequences.len().try_into().unwrap_or(u64::MAX);
    let skipped_count = skip_reason_counts.values().copied().sum::<u64>();
    NetworkRuntimeNovoRudpLedgerFinalMissingAdmissionSnapshotV1 {
        chain_id,
        final_missing_sequence_start,
        ledger_final_missing_candidate_count: candidate_count,
        ledger_final_missing_candidate_ranges_sample:
            network_runtime_native_repair_sequence_ranges_sample_v1(
                &candidate_sequences.into_iter().collect::<HashSet<_>>(),
                64,
            ),
        ledger_final_missing_requeued_before_admission_count: repair_requeue
            .repair_attempted_unreceipted_requeued_count,
        ledger_final_missing_admission_skipped_count: skipped_count,
        ledger_final_missing_admission_skip_reason_counts: skip_reason_counts,
        admission_used_ledger_final_missing_bucket: !candidates.is_empty(),
        repair_requeue,
        candidates,
    }
}

const NETWORK_RUNTIME_NATIVE_PENDING_TX_RETRY_BASE_DELAY_MS_V1: u128 = 500;
const NETWORK_RUNTIME_NATIVE_PENDING_TX_RETRY_MAX_DELAY_MS_V1: u128 = 60_000;
const NETWORK_RUNTIME_NATIVE_PENDING_TX_RETRY_MAX_BACKOFF_LEVEL_V1: u32 = 8;

#[inline]
fn runtime_native_pending_tx_backoff_delay_ms_v1(level: u32) -> u128 {
    if level == 0 {
        return 0;
    }
    let clamped = level.min(NETWORK_RUNTIME_NATIVE_PENDING_TX_RETRY_MAX_BACKOFF_LEVEL_V1);
    let scale = 1_u128 << (clamped.saturating_sub(1));
    NETWORK_RUNTIME_NATIVE_PENDING_TX_RETRY_BASE_DELAY_MS_V1
        .saturating_mul(scale)
        .min(NETWORK_RUNTIME_NATIVE_PENDING_TX_RETRY_MAX_DELAY_MS_V1)
}

#[inline]
fn runtime_native_pending_tx_mark_retry_eligible_v1(tx: &mut NetworkRuntimeNativePendingTxStateV1) {
    tx.retry_eligible = true;
    tx.retry_after_unix_ms = None;
    tx.retry_suppressed_reason = None;
    tx.retry_backoff_level = 0;
    tx.pending_final_disposition = NetworkRuntimeNativePendingTxFinalDispositionV1::Retained;
}

#[inline]
fn runtime_native_pending_tx_refresh_retry_window_v1(
    tx: &mut NetworkRuntimeNativePendingTxStateV1,
    now: u128,
) {
    if tx.retry_eligible {
        return;
    }
    if matches!(
        tx.retry_suppressed_reason,
        Some(NetworkRuntimeNativePendingTxRetrySuppressedReasonV1::CoolingDown)
    ) && tx
        .retry_after_unix_ms
        .is_some_and(|retry_after| retry_after <= now)
    {
        tx.retry_eligible = true;
        tx.retry_after_unix_ms = None;
        tx.retry_suppressed_reason = None;
    }
}

#[inline]
fn runtime_native_pending_tx_set_propagation_success_metadata_v1(
    tx: &mut NetworkRuntimeNativePendingTxStateV1,
    now: u128,
    source_peer_id: Option<u64>,
) {
    tx.last_propagation_attempt_unix_ms = Some(now);
    tx.propagation_attempt_count = tx.propagation_attempt_count.saturating_add(1);
    tx.propagation_success_count = tx.propagation_success_count.saturating_add(1);
    tx.last_propagation_unix_ms = Some(now);
    tx.last_propagation_failure_unix_ms = None;
    tx.last_propagation_failure_class = None;
    tx.last_propagation_failure_phase = None;
    tx.propagation_disposition =
        Some(NetworkRuntimeNativePendingTxPropagationDispositionV1::Propagated);
    tx.propagation_stop_reason = None;
    tx.propagation_recoverability = None;
    runtime_native_pending_tx_mark_retry_eligible_v1(tx);
    if let Some(peer_id) = source_peer_id {
        tx.last_propagation_peer_id = Some(peer_id);
        if tx.last_propagated_peer_id != Some(peer_id) {
            tx.propagated_peer_count = tx.propagated_peer_count.saturating_add(1);
        }
        tx.last_propagated_peer_id = Some(peer_id);
    }
}

#[inline]
fn runtime_native_pending_tx_set_propagation_failure_metadata_v1(
    tx: &mut NetworkRuntimeNativePendingTxStateV1,
    now: u128,
    source_peer_id: Option<u64>,
    stop_reason: NetworkRuntimeNativePendingTxPropagationStopReasonV1,
    failure_phase: Option<&str>,
    disposition: NetworkRuntimeNativePendingTxPropagationDispositionV1,
) {
    tx.last_propagation_attempt_unix_ms = Some(now);
    tx.propagation_attempt_count = tx.propagation_attempt_count.saturating_add(1);
    tx.propagation_failure_count = tx.propagation_failure_count.saturating_add(1);
    tx.last_propagation_failure_unix_ms = Some(now);
    tx.last_propagation_failure_class = Some(stop_reason.as_str().to_string());
    tx.last_propagation_failure_phase = failure_phase.map(str::to_string);
    tx.propagation_disposition = Some(disposition);
    tx.propagation_stop_reason = Some(stop_reason);
    let recoverability = runtime_native_pending_tx_stop_reason_recoverability_v1(stop_reason);
    tx.propagation_recoverability = Some(recoverability);
    match recoverability {
        NetworkRuntimeNativePendingTxPropagationRecoverabilityV1::Recoverable => {
            if matches!(
                stop_reason,
                NetworkRuntimeNativePendingTxPropagationStopReasonV1::BudgetLimit
            ) {
                tx.retry_eligible = false;
                tx.retry_after_unix_ms = None;
                tx.retry_suppressed_reason =
                    Some(NetworkRuntimeNativePendingTxRetrySuppressedReasonV1::BudgetLimit);
            } else {
                tx.retry_backoff_level = tx
                    .retry_backoff_level
                    .saturating_add(1)
                    .min(NETWORK_RUNTIME_NATIVE_PENDING_TX_RETRY_MAX_BACKOFF_LEVEL_V1);
                tx.retry_eligible = false;
                tx.retry_after_unix_ms = Some(now.saturating_add(
                    runtime_native_pending_tx_backoff_delay_ms_v1(tx.retry_backoff_level),
                ));
                tx.retry_suppressed_reason =
                    Some(NetworkRuntimeNativePendingTxRetrySuppressedReasonV1::CoolingDown);
            }
        }
        NetworkRuntimeNativePendingTxPropagationRecoverabilityV1::NonRecoverable => {
            tx.retry_eligible = false;
            tx.retry_after_unix_ms = None;
            tx.retry_suppressed_reason =
                Some(NetworkRuntimeNativePendingTxRetrySuppressedReasonV1::NonRecoverable);
        }
    }
    if let Some(peer_id) = source_peer_id {
        tx.last_propagation_peer_id = Some(peer_id);
    }
}

#[inline]
fn runtime_native_pending_tx_tombstone_from_state_v1(
    tx: &NetworkRuntimeNativePendingTxStateV1,
) -> NetworkRuntimeNativePendingTxTombstoneV1 {
    NetworkRuntimeNativePendingTxTombstoneV1 {
        chain_id: tx.chain_id,
        tx_hash: tx.tx_hash,
        lifecycle_stage: tx.lifecycle_stage,
        origin: tx.origin,
        final_disposition: tx.pending_final_disposition,
        propagation_disposition: tx.propagation_disposition,
        propagation_stop_reason: tx.propagation_stop_reason,
        propagation_recoverability: tx.propagation_recoverability,
        last_updated_unix_ms: tx.last_updated_unix_ms,
    }
}

fn runtime_native_pending_tx_cleanup_v1(chain_id: u64, now: u128) {
    let budget = get_network_runtime_native_budget_hooks_v1(chain_id);
    let canonical_retain_depth = budget.pending_tx_canonical_retain_depth.max(1);
    let no_success_attempt_limit = budget.pending_tx_no_success_attempt_limit.max(1);
    let pending_ttl_ms = (budget.pending_tx_ttl_ms.max(1)) as u128;
    let pending_reorg_return_window_ms = (budget.pending_tx_reorg_return_window_ms.max(1)) as u128;
    let tombstone_retention_max = budget.pending_tx_tombstone_retention_max.max(1) as usize;
    let repair_tx_hashes = network_runtime_native_repair_probe_tx_hashes_v1(chain_id);
    let canonical_head_number =
        runtime_native_canonical_chain_map()
            .lock()
            .ok()
            .and_then(|guard| {
                guard
                    .get(&chain_id)
                    .and_then(|state| state.snapshot.head.as_ref().map(|head| head.number))
            });
    let mut removed_tombstones = Vec::<NetworkRuntimeNativePendingTxTombstoneV1>::new();
    let mut removed_hashes = Vec::<[u8; 32]>::new();
    if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
        let Some(chain_txs) = guard.get_mut(&chain_id) else {
            return;
        };
        let mut remove_candidates =
            Vec::<([u8; 32], NetworkRuntimeNativePendingTxFinalDispositionV1)>::new();
        for (tx_hash, tx) in chain_txs.iter_mut() {
            runtime_native_pending_tx_refresh_retry_window_v1(tx, now);
            let tx_included_in_block = matches!(
                tx.lifecycle_stage,
                NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
                    | NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedNonCanonical
            );
            if tx_included_in_block {
                if let (Some(head_number), Some(block_number)) =
                    (canonical_head_number, tx.last_block_number)
                {
                    if head_number.saturating_sub(block_number) > canonical_retain_depth {
                        remove_candidates.push((
                            *tx_hash,
                            NetworkRuntimeNativePendingTxFinalDispositionV1::Evicted,
                        ));
                    }
                }
                continue;
            }
            let repair_active_pending = repair_tx_hashes.contains(tx_hash)
                && matches!(
                    tx.lifecycle_stage,
                    NetworkRuntimeNativePendingTxLifecycleStageV1::Seen
                        | NetworkRuntimeNativePendingTxLifecycleStageV1::Pending
                        | NetworkRuntimeNativePendingTxLifecycleStageV1::Propagated
                        | NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending
                );
            if !repair_active_pending
                && tx.propagation_attempt_count >= no_success_attempt_limit
                && tx.propagation_success_count == 0
            {
                remove_candidates.push((
                    *tx_hash,
                    NetworkRuntimeNativePendingTxFinalDispositionV1::Evicted,
                ));
                continue;
            }
            let ttl_anchor = if repair_active_pending {
                tx.last_updated_unix_ms
            } else {
                tx.first_seen_unix_ms
            };
            if now.saturating_sub(ttl_anchor) > pending_ttl_ms {
                remove_candidates.push((
                    *tx_hash,
                    NetworkRuntimeNativePendingTxFinalDispositionV1::Expired,
                ));
                continue;
            }
            if matches!(
                tx.lifecycle_stage,
                NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending
            ) && now.saturating_sub(tx.last_updated_unix_ms) > pending_reorg_return_window_ms
            {
                remove_candidates.push((
                    *tx_hash,
                    NetworkRuntimeNativePendingTxFinalDispositionV1::Expired,
                ));
            }
        }
        for (tx_hash, final_disposition) in remove_candidates {
            if let Some(mut tx) = chain_txs.remove(&tx_hash) {
                tx.pending_final_disposition = final_disposition;
                removed_tombstones.push(runtime_native_pending_tx_tombstone_from_state_v1(&tx));
                removed_hashes.push(tx_hash);
            }
        }
    }
    if removed_hashes.is_empty() {
        return;
    }
    if let Ok(mut payloads_guard) = runtime_native_pending_tx_payload_map().lock() {
        if let Some(chain_payloads) = payloads_guard.get_mut(&chain_id) {
            for tx_hash in &removed_hashes {
                chain_payloads.remove(tx_hash);
            }
        }
    }
    if let Ok(mut tombstones_guard) = runtime_native_pending_tx_tombstone_map().lock() {
        let chain_tombstones = tombstones_guard.entry(chain_id).or_default();
        for tombstone in removed_tombstones {
            chain_tombstones.insert(tombstone.tx_hash, tombstone);
        }
        if chain_tombstones.len() > tombstone_retention_max {
            let mut ordered = chain_tombstones
                .values()
                .map(|value| (value.tx_hash, value.last_updated_unix_ms))
                .collect::<Vec<_>>();
            ordered.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
            for (tx_hash, _) in ordered.into_iter().skip(tombstone_retention_max) {
                chain_tombstones.remove(&tx_hash);
            }
        }
    }
}

#[inline]
fn runtime_native_pending_tx_is_historical_stage_v1(
    stage: NetworkRuntimeNativePendingTxLifecycleStageV1,
) -> bool {
    matches!(
        stage,
        NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
            | NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedNonCanonical
            | NetworkRuntimeNativePendingTxLifecycleStageV1::Dropped
            | NetworkRuntimeNativePendingTxLifecycleStageV1::Rejected
    )
}

pub fn compact_network_runtime_native_pending_tx_history_v1(
    chain_id: u64,
    retain_recent: usize,
) -> NetworkRuntimeNativePendingTxHistoryCompactionSummaryV1 {
    runtime_native_pending_tx_cleanup_v1(chain_id, now_unix_millis());
    let budget = get_network_runtime_native_budget_hooks_v1(chain_id);
    let tombstone_retention_max = budget.pending_tx_tombstone_retention_max.max(1) as usize;
    let retain_recent = retain_recent.min(tombstone_retention_max);
    let mut summary = NetworkRuntimeNativePendingTxHistoryCompactionSummaryV1 {
        chain_id,
        retain_recent,
        ..Default::default()
    };

    let mut removed_tombstones = Vec::<NetworkRuntimeNativePendingTxTombstoneV1>::new();
    let mut removed_hashes = Vec::<[u8; 32]>::new();
    let mut final_hashes = Vec::<[u8; 32]>::new();

    if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
        if let Some(chain_txs) = guard.get_mut(&chain_id) {
            for tx in chain_txs.values() {
                if runtime_native_pending_tx_is_historical_stage_v1(tx.lifecycle_stage) {
                    summary.historical_before = summary.historical_before.saturating_add(1);
                }
                match tx.lifecycle_stage {
                    NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical => {
                        summary.included_before = summary.included_before.saturating_add(1);
                        final_hashes.push(tx.tx_hash);
                    }
                    NetworkRuntimeNativePendingTxLifecycleStageV1::Dropped => {
                        summary.dropped_before = summary.dropped_before.saturating_add(1);
                        final_hashes.push(tx.tx_hash);
                    }
                    NetworkRuntimeNativePendingTxLifecycleStageV1::Rejected => {
                        summary.rejected_before = summary.rejected_before.saturating_add(1);
                        final_hashes.push(tx.tx_hash);
                    }
                    _ => {}
                }
            }

            let mut final_entries = chain_txs
                .values()
                .filter(|tx| runtime_native_pending_tx_is_historical_stage_v1(tx.lifecycle_stage))
                .map(|tx| (tx.tx_hash, tx.last_updated_unix_ms, tx.lifecycle_stage))
                .collect::<Vec<_>>();
            final_entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));

            for (tx_hash, _, _) in final_entries.into_iter().skip(retain_recent) {
                if let Some(mut tx) = chain_txs.remove(&tx_hash) {
                    tx.pending_final_disposition =
                        NetworkRuntimeNativePendingTxFinalDispositionV1::Evicted;
                    match tx.lifecycle_stage {
                        NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical => {
                            summary.included_compacted_total =
                                summary.included_compacted_total.saturating_add(1);
                        }
                        NetworkRuntimeNativePendingTxLifecycleStageV1::Dropped => {
                            summary.dropped_compacted_total =
                                summary.dropped_compacted_total.saturating_add(1);
                        }
                        NetworkRuntimeNativePendingTxLifecycleStageV1::Rejected => {
                            summary.rejected_compacted_total =
                                summary.rejected_compacted_total.saturating_add(1);
                        }
                        _ => {}
                    }
                    removed_tombstones.push(runtime_native_pending_tx_tombstone_from_state_v1(&tx));
                    removed_hashes.push(tx_hash);
                }
            }

            summary.historical_compacted_total = removed_hashes.len();
            for tx in chain_txs.values() {
                if runtime_native_pending_tx_is_historical_stage_v1(tx.lifecycle_stage) {
                    summary.historical_after = summary.historical_after.saturating_add(1);
                }
                match tx.lifecycle_stage {
                    NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical => {
                        summary.included_after = summary.included_after.saturating_add(1);
                    }
                    NetworkRuntimeNativePendingTxLifecycleStageV1::Dropped => {
                        summary.dropped_after = summary.dropped_after.saturating_add(1);
                    }
                    NetworkRuntimeNativePendingTxLifecycleStageV1::Rejected => {
                        summary.rejected_after = summary.rejected_after.saturating_add(1);
                    }
                    _ => {}
                }
            }
        }
    }

    let final_hash_set = final_hashes.into_iter().collect::<HashSet<_>>();
    if let Ok(mut payloads_guard) = runtime_native_pending_tx_payload_map().lock() {
        if let Some(chain_payloads) = payloads_guard.get_mut(&chain_id) {
            let payload_hashes = chain_payloads.keys().copied().collect::<Vec<_>>();
            for tx_hash in payload_hashes {
                if final_hash_set.contains(&tx_hash) || removed_hashes.contains(&tx_hash) {
                    if let Some(payload) = chain_payloads.remove(&tx_hash) {
                        summary.historical_payload_bytes_freed = summary
                            .historical_payload_bytes_freed
                            .saturating_add(payload.len());
                    }
                }
            }
            summary.historical_payload_bytes_retained = chain_payloads
                .iter()
                .filter(|(tx_hash, _)| final_hash_set.contains(*tx_hash))
                .map(|(_, payload)| payload.len())
                .sum();
        }
    }

    if let Ok(mut tombstones_guard) = runtime_native_pending_tx_tombstone_map().lock() {
        let chain_tombstones = tombstones_guard.entry(chain_id).or_default();
        for tombstone in removed_tombstones {
            chain_tombstones.insert(tombstone.tx_hash, tombstone);
            summary.tombstones_inserted = summary.tombstones_inserted.saturating_add(1);
        }
        if chain_tombstones.len() > tombstone_retention_max {
            let before = chain_tombstones.len();
            let mut ordered = chain_tombstones
                .values()
                .map(|value| (value.tx_hash, value.last_updated_unix_ms))
                .collect::<Vec<_>>();
            ordered.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
            for (tx_hash, _) in ordered.into_iter().skip(tombstone_retention_max) {
                chain_tombstones.remove(&tx_hash);
            }
            summary.tombstone_evicted_count = before.saturating_sub(chain_tombstones.len());
        }
        summary.tombstone_retained_count = chain_tombstones.len();
    }

    summary.receipt_index_backed_dedup_count = summary
        .included_compacted_total
        .saturating_add(summary.dropped_compacted_total)
        .saturating_add(summary.rejected_compacted_total);
    summary
}

#[inline]
fn runtime_native_pending_tx_set_rejected_v1(
    tx: &mut NetworkRuntimeNativePendingTxStateV1,
    now: u128,
    source_peer_id: Option<u64>,
    stop_reason: NetworkRuntimeNativePendingTxPropagationStopReasonV1,
    failure_phase: Option<&str>,
) {
    if matches!(
        tx.lifecycle_stage,
        NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
    ) {
        return;
    }
    tx.lifecycle_stage = NetworkRuntimeNativePendingTxLifecycleStageV1::Rejected;
    tx.canonical_inclusion = None;
    if source_peer_id.is_some() {
        tx.source_peer_id = source_peer_id;
    }
    tx.reject_count = tx.reject_count.saturating_add(1);
    tx.last_updated_unix_ms = now;
    runtime_native_pending_tx_set_propagation_failure_metadata_v1(
        tx,
        now,
        source_peer_id,
        stop_reason,
        failure_phase,
        NetworkRuntimeNativePendingTxPropagationDispositionV1::Rejected,
    );
}

#[inline]
fn runtime_native_pending_tx_set_dropped_v1(
    tx: &mut NetworkRuntimeNativePendingTxStateV1,
    now: u128,
    source_peer_id: Option<u64>,
    stop_reason: NetworkRuntimeNativePendingTxPropagationStopReasonV1,
    failure_phase: Option<&str>,
) {
    if matches!(
        tx.lifecycle_stage,
        NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
    ) {
        return;
    }
    tx.lifecycle_stage = NetworkRuntimeNativePendingTxLifecycleStageV1::Dropped;
    tx.canonical_inclusion = None;
    tx.drop_count = tx.drop_count.saturating_add(1);
    tx.last_updated_unix_ms = now;
    runtime_native_pending_tx_set_propagation_failure_metadata_v1(
        tx,
        now,
        source_peer_id,
        stop_reason,
        failure_phase,
        NetworkRuntimeNativePendingTxPropagationDispositionV1::Dropped,
    );
}

#[inline]
fn runtime_native_pending_tx_broadcast_eligible_stage_v1(
    stage: NetworkRuntimeNativePendingTxLifecycleStageV1,
) -> bool {
    matches!(
        stage,
        NetworkRuntimeNativePendingTxLifecycleStageV1::Seen
            | NetworkRuntimeNativePendingTxLifecycleStageV1::Pending
            | NetworkRuntimeNativePendingTxLifecycleStageV1::Propagated
            | NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedNonCanonical
            | NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending
    )
}

#[inline]
fn runtime_native_pending_tx_broadcast_stage_priority_v1(
    stage: NetworkRuntimeNativePendingTxLifecycleStageV1,
) -> u8 {
    match stage {
        NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending => 0,
        NetworkRuntimeNativePendingTxLifecycleStageV1::Pending => 1,
        NetworkRuntimeNativePendingTxLifecycleStageV1::Seen => 2,
        NetworkRuntimeNativePendingTxLifecycleStageV1::Propagated => 3,
        NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedNonCanonical => 4,
        NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical => 5,
        NetworkRuntimeNativePendingTxLifecycleStageV1::Dropped => 6,
        NetworkRuntimeNativePendingTxLifecycleStageV1::Rejected => 7,
    }
}

fn runtime_native_pending_tx_observe_body_v1(
    chain_id: u64,
    body: &NetworkRuntimeNativeBodySnapshotV1,
) {
    let block_lifecycle_stage = runtime_native_canonical_chain_map()
        .lock()
        .ok()
        .and_then(|guard| {
            guard
                .get(&chain_id)
                .and_then(|state| state.blocks_by_hash.get(&body.block_hash))
                .map(|block| block.lifecycle_stage)
        })
        .unwrap_or(NetworkRuntimeNativeBlockLifecycleStageV1::Seen);
    let next_stage = runtime_native_pending_tx_stage_from_block_lifecycle_v1(block_lifecycle_stage);
    let mut canonical_tx_hashes = Vec::<[u8; 32]>::new();
    if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
        let chain_txs = guard.entry(chain_id).or_default();
        for tx_hash in &body.tx_hashes {
            let tx =
                chain_txs
                    .entry(*tx_hash)
                    .or_insert_with(|| NetworkRuntimeNativePendingTxStateV1 {
                        chain_id,
                        tx_hash: *tx_hash,
                        lifecycle_stage: NetworkRuntimeNativePendingTxLifecycleStageV1::Seen,
                        origin: NetworkRuntimeNativePendingTxOriginV1::Unknown,
                        source_peer_id: None,
                        first_seen_unix_ms: body.observed_unix_ms,
                        last_updated_unix_ms: body.observed_unix_ms,
                        last_block_number: None,
                        last_block_hash: None,
                        canonical_inclusion: None,
                        ingress_count: 0,
                        propagation_count: 0,
                        inclusion_count: 0,
                        reorg_back_count: 0,
                        drop_count: 0,
                        reject_count: 0,
                        propagation_attempt_count: 0,
                        propagation_success_count: 0,
                        propagation_failure_count: 0,
                        propagated_peer_count: 0,
                        last_propagation_unix_ms: None,
                        last_propagation_attempt_unix_ms: None,
                        last_propagation_failure_unix_ms: None,
                        last_propagation_failure_class: None,
                        last_propagation_failure_phase: None,
                        last_propagation_peer_id: None,
                        last_propagated_peer_id: None,
                        propagation_disposition: None,
                        propagation_stop_reason: None,
                        propagation_recoverability: None,
                        retry_eligible: true,
                        retry_after_unix_ms: None,
                        retry_backoff_level: 0,
                        retry_suppressed_reason: None,
                        pending_final_disposition:
                            NetworkRuntimeNativePendingTxFinalDispositionV1::Retained,
                    });
            tx.lifecycle_stage = next_stage;
            tx.last_block_number = Some(body.number);
            tx.last_block_hash = Some(body.block_hash);
            tx.canonical_inclusion = Some(matches!(
                next_stage,
                NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
            ));
            if matches!(
                next_stage,
                NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
            ) {
                tx.retry_eligible = false;
                tx.retry_after_unix_ms = None;
                tx.retry_suppressed_reason =
                    Some(NetworkRuntimeNativePendingTxRetrySuppressedReasonV1::IncludedCanonical);
                canonical_tx_hashes.push(*tx_hash);
            } else {
                runtime_native_pending_tx_mark_retry_eligible_v1(tx);
            }
            tx.inclusion_count = tx.inclusion_count.saturating_add(1);
            tx.last_updated_unix_ms = body.observed_unix_ms;
        }
    }
    for tx_hash in canonical_tx_hashes {
        observe_network_runtime_native_pending_tx_repair_receipt_canonical_v1(chain_id, tx_hash);
    }
}

fn runtime_native_pending_tx_reconcile_against_canonical_chain_v1(chain_id: u64) {
    let lifecycle_by_hash = runtime_native_canonical_chain_map()
        .lock()
        .ok()
        .and_then(|guard| guard.get(&chain_id).cloned())
        .map(|state| {
            state
                .blocks_by_hash
                .into_iter()
                .map(|(hash, block)| (hash, block.lifecycle_stage))
                .collect::<HashMap<[u8; 32], NetworkRuntimeNativeBlockLifecycleStageV1>>()
        })
        .unwrap_or_default();
    let now = now_unix_millis();
    if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
        let Some(chain_txs) = guard.get_mut(&chain_id) else {
            return;
        };
        for tx in chain_txs.values_mut() {
            let Some(block_hash) = tx.last_block_hash else {
                continue;
            };
            let Some(block_stage) = lifecycle_by_hash.get(&block_hash).copied() else {
                continue;
            };
            match tx.lifecycle_stage {
                NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical => {
                    if !matches!(
                        block_stage,
                        NetworkRuntimeNativeBlockLifecycleStageV1::Canonical
                    ) {
                        tx.lifecycle_stage =
                            NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending;
                        tx.canonical_inclusion = Some(false);
                        runtime_native_pending_tx_mark_retry_eligible_v1(tx);
                        tx.reorg_back_count = tx.reorg_back_count.saturating_add(1);
                        tx.last_updated_unix_ms = now;
                    }
                }
                NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedNonCanonical => {
                    if matches!(
                        block_stage,
                        NetworkRuntimeNativeBlockLifecycleStageV1::ReorgedOut
                    ) {
                        tx.lifecycle_stage =
                            NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending;
                        tx.canonical_inclusion = Some(false);
                        runtime_native_pending_tx_mark_retry_eligible_v1(tx);
                        tx.reorg_back_count = tx.reorg_back_count.saturating_add(1);
                        tx.last_updated_unix_ms = now;
                    } else if matches!(
                        block_stage,
                        NetworkRuntimeNativeBlockLifecycleStageV1::Canonical
                    ) {
                        tx.lifecycle_stage =
                            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical;
                        tx.canonical_inclusion = Some(true);
                        tx.retry_eligible = false;
                        tx.retry_after_unix_ms = None;
                        tx.retry_suppressed_reason = Some(
                            NetworkRuntimeNativePendingTxRetrySuppressedReasonV1::IncludedCanonical,
                        );
                        tx.last_updated_unix_ms = now;
                    }
                }
                NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending => {
                    if matches!(
                        block_stage,
                        NetworkRuntimeNativeBlockLifecycleStageV1::Canonical
                    ) {
                        tx.lifecycle_stage =
                            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical;
                        tx.canonical_inclusion = Some(true);
                        tx.retry_eligible = false;
                        tx.retry_after_unix_ms = None;
                        tx.retry_suppressed_reason = Some(
                            NetworkRuntimeNativePendingTxRetrySuppressedReasonV1::IncludedCanonical,
                        );
                        tx.last_updated_unix_ms = now;
                    }
                }
                _ => {}
            }
        }
    }
}

fn runtime_native_canonical_chain_prune_v1(
    state: &mut NetworkRuntimeNativeCanonicalChainStateInternalV1,
) {
    if state.blocks_by_hash.len() <= NETWORK_RUNTIME_NATIVE_CANONICAL_CHAIN_RETENTION_MAX_V1 {
        runtime_native_canonical_chain_refresh_snapshot_v1(state);
        return;
    }
    let mut removable = state
        .blocks_by_hash
        .values()
        .filter(|block| !block.canonical)
        .map(|block| (block.observed_unix_ms, block.hash))
        .collect::<Vec<_>>();
    removable.sort_by(|a, b| a.0.cmp(&b.0));
    let mut excess = state
        .blocks_by_hash
        .len()
        .saturating_sub(NETWORK_RUNTIME_NATIVE_CANONICAL_CHAIN_RETENTION_MAX_V1);
    for (_, hash) in removable {
        if excess == 0 {
            break;
        }
        state.blocks_by_hash.remove(&hash);
        excess = excess.saturating_sub(1);
    }
    runtime_native_canonical_chain_refresh_snapshot_v1(state);
}

fn runtime_native_canonical_chain_upsert_header_v1(
    state: &mut NetworkRuntimeNativeCanonicalChainStateInternalV1,
    header: &NetworkRuntimeNativeHeaderSnapshotV1,
) {
    let entry = state.blocks_by_hash.entry(header.hash).or_insert_with(|| {
        NetworkRuntimeNativeCanonicalBlockStateV1 {
            chain_id: header.chain_id,
            number: header.number,
            hash: header.hash,
            parent_hash: header.parent_hash,
            state_root: header.state_root,
            transactions_root: Some(header.transactions_root),
            header_observed: true,
            body_available: false,
            tx_hashes: Vec::new(),
            raw_tx_rlps: Vec::new(),
            ommer_hashes: Vec::new(),
            withdrawal_rlp_items: None,
            withdrawal_count: None,
            receipts_available: false,
            receipt_count: None,
            raw_receipts: Vec::new(),
            receipts_root: Some(header.receipts_root),
            ommers_hash: Some(header.ommers_hash),
            state_root_validated: false,
            state_root_validation_method: None,
            lifecycle_stage: NetworkRuntimeNativeBlockLifecycleStageV1::HeaderOnly,
            canonical: false,
            safe: false,
            finalized: false,
            source_peer_id: header.source_peer_id,
            observed_unix_ms: header.observed_unix_ms,
        }
    });
    entry.chain_id = header.chain_id;
    entry.number = header.number;
    entry.parent_hash = header.parent_hash;
    entry.state_root = header.state_root;
    entry.transactions_root = Some(header.transactions_root);
    entry.header_observed = true;
    entry.receipts_root = Some(header.receipts_root);
    entry.ommers_hash = Some(header.ommers_hash);
    entry.source_peer_id = header.source_peer_id.or(entry.source_peer_id);
    entry.observed_unix_ms = entry.observed_unix_ms.max(header.observed_unix_ms);
    entry.lifecycle_stage = runtime_native_canonical_chain_infer_block_lifecycle_v1(
        entry,
        &state.canonical_hash_by_number,
    );
    runtime_native_canonical_chain_prune_v1(state);
}

fn runtime_native_canonical_chain_upsert_body_v1(
    state: &mut NetworkRuntimeNativeCanonicalChainStateInternalV1,
    body: &NetworkRuntimeNativeBodySnapshotV1,
) {
    let entry = state
        .blocks_by_hash
        .entry(body.block_hash)
        .or_insert_with(|| NetworkRuntimeNativeCanonicalBlockStateV1 {
            chain_id: body.chain_id,
            number: body.number,
            hash: body.block_hash,
            parent_hash: [0u8; 32],
            state_root: [0u8; 32],
            transactions_root: None,
            header_observed: false,
            body_available: body.body_available,
            tx_hashes: body.tx_hashes.clone(),
            raw_tx_rlps: body.raw_tx_rlps.clone(),
            ommer_hashes: body.ommer_hashes.clone(),
            withdrawal_rlp_items: body.withdrawal_rlp_items.clone(),
            withdrawal_count: body.withdrawal_count,
            receipts_available: false,
            receipt_count: None,
            raw_receipts: Vec::new(),
            receipts_root: None,
            ommers_hash: None,
            state_root_validated: false,
            state_root_validation_method: None,
            lifecycle_stage: NetworkRuntimeNativeBlockLifecycleStageV1::Seen,
            canonical: false,
            safe: false,
            finalized: false,
            source_peer_id: None,
            observed_unix_ms: body.observed_unix_ms,
        });
    entry.chain_id = body.chain_id;
    entry.number = body.number;
    entry.body_available |= body.body_available;
    if !body.tx_hashes.is_empty() {
        entry.tx_hashes = body.tx_hashes.clone();
    }
    if !body.raw_tx_rlps.is_empty() {
        entry.raw_tx_rlps = body.raw_tx_rlps.clone();
    }
    entry.ommer_hashes = body.ommer_hashes.clone();
    if body.withdrawal_rlp_items.is_some() {
        entry.withdrawal_rlp_items = body.withdrawal_rlp_items.clone();
    }
    if body.withdrawal_count.is_some() {
        entry.withdrawal_count = body.withdrawal_count;
    }
    let fill_canonical_gap = state
        .snapshot
        .head
        .as_ref()
        .is_some_and(|head| head.canonical && body.number <= head.number)
        && state
            .canonical_hash_by_number
            .get(&body.number)
            .is_none_or(|hash| *hash == body.block_hash);
    if fill_canonical_gap {
        state
            .canonical_hash_by_number
            .insert(body.number, body.block_hash);
        entry.canonical = true;
        entry.lifecycle_stage = NetworkRuntimeNativeBlockLifecycleStageV1::Canonical;
    }
    entry.observed_unix_ms = entry.observed_unix_ms.max(body.observed_unix_ms);
    entry.lifecycle_stage = runtime_native_canonical_chain_infer_block_lifecycle_v1(
        entry,
        &state.canonical_hash_by_number,
    );
    runtime_native_canonical_chain_prune_v1(state);
}

fn runtime_native_canonical_chain_upsert_receipt_v1(
    state: &mut NetworkRuntimeNativeCanonicalChainStateInternalV1,
    receipt: &NetworkRuntimeNativeReceiptSnapshotV1,
) {
    let entry = state
        .blocks_by_hash
        .entry(receipt.block_hash)
        .or_insert_with(|| NetworkRuntimeNativeCanonicalBlockStateV1 {
            chain_id: receipt.chain_id,
            number: receipt.number,
            hash: receipt.block_hash,
            parent_hash: [0u8; 32],
            state_root: [0u8; 32],
            transactions_root: None,
            header_observed: false,
            body_available: false,
            tx_hashes: Vec::new(),
            raw_tx_rlps: Vec::new(),
            ommer_hashes: Vec::new(),
            withdrawal_rlp_items: None,
            withdrawal_count: None,
            receipts_available: receipt.receipts_available,
            receipt_count: Some(receipt.receipt_count),
            raw_receipts: receipt.raw_receipts.clone(),
            receipts_root: Some(receipt.receipts_root),
            ommers_hash: None,
            state_root_validated: false,
            state_root_validation_method: None,
            lifecycle_stage: NetworkRuntimeNativeBlockLifecycleStageV1::Seen,
            canonical: false,
            safe: false,
            finalized: false,
            source_peer_id: receipt.source_peer_id,
            observed_unix_ms: receipt.observed_unix_ms,
        });
    entry.chain_id = receipt.chain_id;
    entry.number = receipt.number;
    entry.receipts_available |= receipt.receipts_available;
    entry.receipt_count = Some(receipt.receipt_count);
    entry.raw_receipts = receipt.raw_receipts.clone();
    entry.receipts_root = Some(receipt.receipts_root);
    entry.source_peer_id = receipt.source_peer_id.or(entry.source_peer_id);
    entry.observed_unix_ms = entry.observed_unix_ms.max(receipt.observed_unix_ms);
    entry.lifecycle_stage = runtime_native_canonical_chain_infer_block_lifecycle_v1(
        entry,
        &state.canonical_hash_by_number,
    );
    runtime_native_canonical_chain_prune_v1(state);
}

fn runtime_native_canonical_chain_find_common_ancestor_hash_v1(
    state: &NetworkRuntimeNativeCanonicalChainStateInternalV1,
    current_head_hash: [u8; 32],
    next_head_hash: [u8; 32],
) -> Option<[u8; 32]> {
    let mut seen = HashSet::new();
    let mut cursor = Some(current_head_hash);
    for _ in 0..NETWORK_RUNTIME_NATIVE_CANONICAL_CHAIN_RETENTION_MAX_V1 {
        let Some(hash) = cursor else {
            break;
        };
        if !seen.insert(hash) {
            break;
        }
        let Some(block) = state.blocks_by_hash.get(&hash) else {
            break;
        };
        if block.parent_hash == hash {
            break;
        }
        cursor = Some(block.parent_hash);
    }

    let mut cursor = Some(next_head_hash);
    for _ in 0..NETWORK_RUNTIME_NATIVE_CANONICAL_CHAIN_RETENTION_MAX_V1 {
        let Some(hash) = cursor else {
            break;
        };
        if seen.contains(&hash) {
            return Some(hash);
        }
        let Some(block) = state.blocks_by_hash.get(&hash) else {
            break;
        };
        if block.parent_hash == hash {
            break;
        }
        cursor = Some(block.parent_hash);
    }
    None
}

fn runtime_native_canonical_chain_collect_branch_v1(
    state: &NetworkRuntimeNativeCanonicalChainStateInternalV1,
    head_hash: [u8; 32],
    stop_hash: Option<[u8; 32]>,
) -> Vec<[u8; 32]> {
    let mut out = Vec::new();
    let mut cursor = Some(head_hash);
    for _ in 0..NETWORK_RUNTIME_NATIVE_CANONICAL_CHAIN_RETENTION_MAX_V1 {
        let Some(hash) = cursor else {
            break;
        };
        if Some(hash) == stop_hash {
            break;
        }
        out.push(hash);
        let Some(block) = state.blocks_by_hash.get(&hash) else {
            break;
        };
        if block.parent_hash == hash {
            break;
        }
        cursor = Some(block.parent_hash);
    }
    out
}

fn runtime_native_canonical_chain_mark_current_head_flags_v1(
    state: &mut NetworkRuntimeNativeCanonicalChainStateInternalV1,
) {
    let canonical_hash_by_number = state.canonical_hash_by_number.clone();
    for block in state.blocks_by_hash.values_mut() {
        block.canonical = canonical_hash_by_number
            .get(&block.number)
            .is_some_and(|hash| *hash == block.hash);
        if block.canonical {
            block.lifecycle_stage = NetworkRuntimeNativeBlockLifecycleStageV1::Canonical;
        } else {
            block.lifecycle_stage = runtime_native_canonical_chain_infer_block_lifecycle_v1(
                block,
                &canonical_hash_by_number,
            );
            block.safe = false;
            block.finalized = false;
        }
    }
    if let Some(head) = state.snapshot.head.as_ref() {
        if let Some(block) = state.blocks_by_hash.get_mut(&head.hash) {
            block.canonical = true;
            block.safe = head.safe;
            block.finalized = head.finalized;
            block.body_available = head.body_available;
            block.state_root = head.state_root;
            block.source_peer_id = head.source_peer_id;
            block.observed_unix_ms = block.observed_unix_ms.max(head.observed_unix_ms);
            block.lifecycle_stage = NetworkRuntimeNativeBlockLifecycleStageV1::Canonical;
        }
    }
    if let Some(head_hash) = state.snapshot.head.as_ref().map(|head| head.hash) {
        if let Some(block) = state.blocks_by_hash.get(&head_hash).cloned() {
            state.snapshot.head = Some(block);
        }
    }
}

fn runtime_native_canonical_chain_apply_head_v1(
    state: &mut NetworkRuntimeNativeCanonicalChainStateInternalV1,
    head: &NetworkRuntimeNativeHeadSnapshotV1,
) {
    {
        let entry = state
            .blocks_by_hash
            .entry(head.block_hash)
            .or_insert_with(|| NetworkRuntimeNativeCanonicalBlockStateV1 {
                chain_id: head.chain_id,
                number: head.block_number,
                hash: head.block_hash,
                parent_hash: head.parent_block_hash,
                state_root: head.state_root,
                transactions_root: None,
                header_observed: true,
                body_available: head.body_available,
                tx_hashes: Vec::new(),
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: None,
                receipts_available: false,
                receipt_count: None,
                raw_receipts: Vec::new(),
                receipts_root: None,
                ommers_hash: None,
                state_root_validated: false,
                state_root_validation_method: None,
                lifecycle_stage: NetworkRuntimeNativeBlockLifecycleStageV1::Canonical,
                canonical: head.canonical,
                safe: head.safe,
                finalized: head.finalized,
                source_peer_id: head.source_peer_id,
                observed_unix_ms: head.observed_unix_ms,
            });
        entry.chain_id = head.chain_id;
        entry.number = head.block_number;
        entry.parent_hash = head.parent_block_hash;
        entry.state_root = head.state_root;
        entry.header_observed = true;
        entry.body_available = head.body_available;
        entry.canonical = head.canonical;
        entry.safe = head.safe;
        entry.finalized = head.finalized;
        entry.source_peer_id = head.source_peer_id.or(entry.source_peer_id);
        entry.observed_unix_ms = entry.observed_unix_ms.max(head.observed_unix_ms);
        entry.lifecycle_stage = if head.canonical {
            NetworkRuntimeNativeBlockLifecycleStageV1::Canonical
        } else {
            runtime_native_canonical_chain_infer_block_lifecycle_v1(
                entry,
                &state.canonical_hash_by_number,
            )
        };
    }
    let head_block = state
        .blocks_by_hash
        .get(&head.block_hash)
        .cloned()
        .unwrap_or(NetworkRuntimeNativeCanonicalBlockStateV1 {
            chain_id: head.chain_id,
            number: head.block_number,
            hash: head.block_hash,
            parent_hash: head.parent_block_hash,
            state_root: head.state_root,
            transactions_root: None,
            header_observed: true,
            body_available: head.body_available,
            tx_hashes: Vec::new(),
            raw_tx_rlps: Vec::new(),
            ommer_hashes: Vec::new(),
            withdrawal_rlp_items: None,
            withdrawal_count: None,
            receipts_available: false,
            receipt_count: None,
            raw_receipts: Vec::new(),
            receipts_root: None,
            ommers_hash: None,
            state_root_validated: false,
            state_root_validation_method: None,
            lifecycle_stage: NetworkRuntimeNativeBlockLifecycleStageV1::Canonical,
            canonical: head.canonical,
            safe: head.safe,
            finalized: head.finalized,
            source_peer_id: head.source_peer_id,
            observed_unix_ms: head.observed_unix_ms,
        });

    let previous_head = state.snapshot.head.clone();
    let lifecycle_stage = match previous_head.as_ref() {
        None => NetworkRuntimeNativeCanonicalLifecycleStageV1::Initial,
        Some(previous) if previous.hash == head.block_hash => {
            NetworkRuntimeNativeCanonicalLifecycleStageV1::Refreshed
        }
        Some(previous)
            if previous.number.saturating_add(1) == head.block_number
                && previous.hash == head.parent_block_hash =>
        {
            NetworkRuntimeNativeCanonicalLifecycleStageV1::Advanced
        }
        Some(_) => NetworkRuntimeNativeCanonicalLifecycleStageV1::Reorg,
    };
    state.snapshot.lifecycle_stage = lifecycle_stage;

    if !matches!(
        lifecycle_stage,
        NetworkRuntimeNativeCanonicalLifecycleStageV1::Refreshed
    ) {
        state.snapshot.canonical_update_count =
            state.snapshot.canonical_update_count.saturating_add(1);
        state.snapshot.last_head_change_unix_ms = Some(head.observed_unix_ms);
    }

    if matches!(
        lifecycle_stage,
        NetworkRuntimeNativeCanonicalLifecycleStageV1::Reorg
    ) {
        let common_ancestor = previous_head.as_ref().and_then(|previous| {
            runtime_native_canonical_chain_find_common_ancestor_hash_v1(
                state,
                previous.hash,
                head.block_hash,
            )
        });
        let reorg_depth = previous_head.as_ref().map(|previous| {
            common_ancestor
                .and_then(|hash| state.blocks_by_hash.get(&hash).map(|block| block.number))
                .map(|ancestor_number| previous.number.saturating_sub(ancestor_number))
                .or(head.reorg_depth_hint)
                .unwrap_or(previous.number.saturating_add(1))
        });
        state.snapshot.reorg_count = state.snapshot.reorg_count.saturating_add(1);
        state.snapshot.last_reorg_depth = reorg_depth;
        state.snapshot.last_reorg_unix_ms = Some(head.observed_unix_ms);

        let ancestor_number = common_ancestor
            .and_then(|hash| state.blocks_by_hash.get(&hash).map(|block| block.number));
        let removed_numbers = state
            .canonical_hash_by_number
            .keys()
            .copied()
            .filter(|number| {
                ancestor_number
                    .map(|ancestor| *number > ancestor)
                    .unwrap_or(true)
            })
            .collect::<Vec<_>>();
        for number in removed_numbers {
            if let Some(hash) = state.canonical_hash_by_number.remove(&number) {
                if let Some(block) = state.blocks_by_hash.get_mut(&hash) {
                    block.canonical = false;
                    block.safe = false;
                    block.finalized = false;
                    block.lifecycle_stage = NetworkRuntimeNativeBlockLifecycleStageV1::ReorgedOut;
                }
            }
        }
        let branch_hashes = runtime_native_canonical_chain_collect_branch_v1(
            state,
            head.block_hash,
            common_ancestor,
        );
        for hash in branch_hashes.into_iter().rev() {
            if let Some(block) = state.blocks_by_hash.get_mut(&hash) {
                block.canonical = true;
                block.safe = false;
                block.finalized = false;
                block.lifecycle_stage = NetworkRuntimeNativeBlockLifecycleStageV1::Canonical;
                state.canonical_hash_by_number.insert(block.number, hash);
            }
        }
    } else {
        state
            .canonical_hash_by_number
            .insert(head.block_number, head.block_hash);
    }

    state.snapshot.head = Some(head_block);
    runtime_native_canonical_chain_mark_current_head_flags_v1(state);
    runtime_native_canonical_chain_prune_v1(state);
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

pub fn clear_network_runtime_native_state_for_host_tests_v1() {
    if let Ok(mut guard) = runtime_sync_status_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_sync_status_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_header_snapshot_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_body_snapshot_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_header_rlp_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_receipt_snapshot_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_block_access_list_payload_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_snap_account_snapshot_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_snap_storage_snapshot_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_snap_code_snapshot_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_snap_trie_node_snapshot_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_snap_account_range_progress_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_head_snapshot_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_canonical_chain_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_header_source_peer_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_pending_tx_payload_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_pending_tx_tombstone_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_pending_tx_broadcast_runtime_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_novorudp_sequence_ledger_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_execution_budget_runtime_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_native_budget_hooks_map().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = runtime_sync_observed_state_map().lock() {
        *guard = NetworkRuntimeSyncObservedState::default();
    }
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
const DEFAULT_RUNTIME_REMOTE_BEST_HINT_TIMEOUT_MILLIS: u128 = 300_000;
const NATIVE_SYNC_GAP_HEADERS_THRESHOLD: u64 = 8_192;
const NATIVE_SYNC_GAP_BODIES_THRESHOLD: u64 = 1_024;
const NATIVE_SYNC_GAP_STATE_THRESHOLD: u64 = 128;
const NATIVE_SYNC_GAP_FINALIZE_THRESHOLD: u64 = 8;

fn now_unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis()
}

fn hint_runtime_stale_check_deadline(
    observed: &mut NetworkRuntimeSyncObservedState,
    chain_id: u64,
    now: u128,
) {
    let deadline = now.saturating_add(DEFAULT_RUNTIME_PEER_STALE_TIMEOUT_MILLIS);
    observed
        .next_stale_check_at_by_chain
        .entry(chain_id)
        .and_modify(|due| *due = (*due).min(deadline))
        .or_insert(deadline);
}

fn recompute_runtime_stale_check_deadline(
    chain_id: u64,
    observed: &mut NetworkRuntimeSyncObservedState,
) {
    let mut next_due: Option<u128> = observed
        .peer_last_seen_millis_by_chain
        .get(&chain_id)
        .and_then(|peer_last_seen| peer_last_seen.values().copied().min())
        .map(|last_seen| last_seen.saturating_add(DEFAULT_RUNTIME_PEER_STALE_TIMEOUT_MILLIS));

    if let Some(snapshot_updated_at) = observed
        .native_snapshot_updated_at_by_chain
        .get(&chain_id)
        .copied()
    {
        let snapshot_due =
            snapshot_updated_at.saturating_add(DEFAULT_RUNTIME_PEER_STALE_TIMEOUT_MILLIS);
        next_due = Some(next_due.map_or(snapshot_due, |existing| existing.min(snapshot_due)));
    }

    if let Some(hint_updated_at) = observed
        .remote_best_hint_updated_at_by_chain
        .get(&chain_id)
        .copied()
    {
        let hint_due =
            hint_updated_at.saturating_add(DEFAULT_RUNTIME_REMOTE_BEST_HINT_TIMEOUT_MILLIS);
        next_due = Some(next_due.map_or(hint_due, |existing| existing.min(hint_due)));
    }

    if let Some(due) = next_due {
        observed.next_stale_check_at_by_chain.insert(chain_id, due);
    } else {
        observed.next_stale_check_at_by_chain.remove(&chain_id);
    }
}

#[inline]
fn mark_runtime_sync_observed_dirty(observed: &mut NetworkRuntimeSyncObservedState, chain_id: u64) {
    observed.dirty_chains.insert(chain_id);
}

fn runtime_sync_recompute_due(
    chain_id: u64,
    statuses: &HashMap<u64, NetworkRuntimeSyncStatus>,
    observed: &NetworkRuntimeSyncObservedState,
    has_observed: bool,
) -> bool {
    if !has_observed {
        return false;
    }
    if !statuses.contains_key(&chain_id) {
        return true;
    }
    if observed.dirty_chains.contains(&chain_id) {
        return true;
    }
    observed
        .next_stale_check_at_by_chain
        .get(&chain_id)
        .copied()
        .map(|due| now_unix_millis() >= due)
        .unwrap_or(false)
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

fn native_sync_pull_batch_by_phase(
    phase: NetworkRuntimeNativeSyncPhaseV1,
    budget: &EthFullnodeBudgetHooksV1,
) -> Option<u64> {
    match phase {
        NetworkRuntimeNativeSyncPhaseV1::Idle | NetworkRuntimeNativeSyncPhaseV1::Discovery => None,
        NetworkRuntimeNativeSyncPhaseV1::Headers => Some(budget.sync_pull_headers_batch.max(1)),
        NetworkRuntimeNativeSyncPhaseV1::Bodies => Some(budget.sync_pull_bodies_batch.max(1)),
        NetworkRuntimeNativeSyncPhaseV1::State => Some(budget.sync_pull_state_batch.max(1)),
        NetworkRuntimeNativeSyncPhaseV1::Finalize => Some(budget.sync_pull_finalize_batch.max(1)),
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

    let runtime_phase = native_sync_phase_from_runtime_status(&runtime);
    let phase = get_network_runtime_native_sync_status(chain_id)
        .filter(network_runtime_native_sync_is_active)
        .map(|status| status.phase)
        .filter(|phase| {
            !matches!(
                phase,
                NetworkRuntimeNativeSyncPhaseV1::Idle | NetworkRuntimeNativeSyncPhaseV1::Discovery
            )
        })
        .unwrap_or(runtime_phase);
    let budget = get_network_runtime_native_budget_hooks_v1(chain_id);
    let batch_size = native_sync_pull_batch_by_phase(phase, &budget)?;
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
    reconcile_network_runtime_native_sync_status_for_runtime(chain_id, runtime)
}

fn reconcile_network_runtime_native_sync_status_for_runtime(
    chain_id: u64,
    runtime: NetworkRuntimeSyncStatus,
) -> Option<NetworkRuntimeNativeSyncStatusV1> {
    let phase = native_sync_phase_from_runtime_status(&runtime);
    let mut native = runtime_native_sync_status_map().lock().ok()?;
    let previous = native.get(&chain_id).copied();
    let status = if matches!(phase, NetworkRuntimeNativeSyncPhaseV1::Idle) {
        NetworkRuntimeNativeSyncStatusV1 {
            phase: NetworkRuntimeNativeSyncPhaseV1::Idle,
            peer_count: runtime.peer_count,
            starting_block: runtime.current_block,
            current_block: runtime.current_block,
            highest_block: runtime.current_block,
            updated_at_unix_millis: now_unix_millis(),
        }
        .normalized()
    } else {
        let starting_block = previous
            .map(|s| {
                if matches!(s.phase, NetworkRuntimeNativeSyncPhaseV1::Idle) {
                    runtime.current_block
                } else {
                    s.starting_block.min(runtime.current_block)
                }
            })
            .unwrap_or(runtime.current_block);
        NetworkRuntimeNativeSyncStatusV1 {
            phase,
            peer_count: runtime.peer_count,
            starting_block,
            current_block: runtime.current_block,
            highest_block: runtime.highest_block.max(runtime.current_block),
            updated_at_unix_millis: now_unix_millis(),
        }
        .normalized()
    };
    native.insert(chain_id, status);
    Some(status)
}

fn prune_stale_runtime_peers(chain_id: u64, observed: &mut NetworkRuntimeSyncObservedState) {
    let now = now_unix_millis();
    if !observed
        .peer_last_seen_millis_by_chain
        .contains_key(&chain_id)
        && !observed
            .native_snapshot_updated_at_by_chain
            .contains_key(&chain_id)
        && !observed
            .remote_best_hint_updated_at_by_chain
            .contains_key(&chain_id)
    {
        observed.next_stale_check_at_by_chain.remove(&chain_id);
        return;
    }
    if let Some(next_due) = observed
        .next_stale_check_at_by_chain
        .get(&chain_id)
        .copied()
    {
        if now < next_due {
            return;
        }
    }
    if let Some(mut peer_last_seen) = observed.peer_last_seen_millis_by_chain.remove(&chain_id) {
        let before = peer_last_seen.len();
        peer_last_seen.retain(|_, last_seen| {
            now.saturating_sub(*last_seen) <= DEFAULT_RUNTIME_PEER_STALE_TIMEOUT_MILLIS
        });
        let peers_changed = peer_last_seen.len() != before;
        if peer_last_seen.is_empty() {
            observed.peer_height_by_chain.remove(&chain_id);
        } else {
            if peers_changed {
                if let Some(peer_heights) = observed.peer_height_by_chain.get_mut(&chain_id) {
                    peer_heights.retain(|peer_id, _| peer_last_seen.contains_key(peer_id));
                    if peer_heights.is_empty() {
                        observed.peer_height_by_chain.remove(&chain_id);
                    }
                }
            }
            observed
                .peer_last_seen_millis_by_chain
                .insert(chain_id, peer_last_seen);
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
    if let Some(updated_at) = observed
        .remote_best_hint_updated_at_by_chain
        .get(&chain_id)
        .copied()
    {
        if now.saturating_sub(updated_at) > DEFAULT_RUNTIME_REMOTE_BEST_HINT_TIMEOUT_MILLIS {
            observed.remote_best_hint_by_chain.remove(&chain_id);
            observed
                .remote_best_hint_updated_at_by_chain
                .remove(&chain_id);
        }
    }
    recompute_runtime_stale_check_deadline(chain_id, observed);
}

fn recompute_runtime_sync_status_from_observed(
    chain_id: u64,
    statuses: &mut HashMap<u64, NetworkRuntimeSyncStatus>,
    observed: &mut NetworkRuntimeSyncObservedState,
) -> NetworkRuntimeSyncStatus {
    let had_native_remote_best_hint = observed.native_remote_best_by_chain.contains_key(&chain_id);
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
    let remote_best_hint = observed.remote_best_hint_by_chain.get(&chain_id).copied();
    let has_peer_observation_history = observed.peer_observed_once_by_chain.contains(&chain_id);
    let remote_best = peer_map.and_then(|m| m.values().copied().max());
    let effective_remote_best = [remote_best, native_remote_best, remote_best_hint]
        .into_iter()
        .flatten()
        .max();
    let local_head = observed.local_head_by_chain.get(&chain_id).copied();
    let observed_peer_count = peer_map.map(|m| m.len() as u64).unwrap_or(0);
    let effective_peer_count = observed_peer_count.max(native_peer_count);
    let has_observed_runtime =
        local_head.is_some() || has_peer_observation_history || effective_peer_count > 0;

    if has_peer_observation_history || effective_peer_count > 0 {
        status.peer_count = effective_peer_count;
    }
    status.current_block = local_head.unwrap_or(status.current_block);
    status.highest_block = if let Some(remote_best) = effective_remote_best {
        status
            .highest_block
            .max(status.current_block)
            .max(remote_best)
    } else if has_observed_runtime && had_native_remote_best_hint {
        status.current_block
    } else {
        status.highest_block.max(status.current_block)
    };
    if status.highest_block > status.current_block {
        let existing_anchor = observed.sync_anchor_by_chain.get(&chain_id).copied();
        let mut start_anchor = existing_anchor
            .unwrap_or({
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
    observed.dirty_chains.remove(&chain_id);
    normalized
}

pub fn set_network_runtime_sync_status(chain_id: u64, status: NetworkRuntimeSyncStatus) {
    let normalized = status.normalized();
    let mut changed = true;
    if let Ok(mut guard) = runtime_sync_status_map().lock() {
        changed = guard.get(&chain_id).copied() != Some(normalized);
        if changed {
            guard.insert(chain_id, normalized);
        }
    }
    if changed {
        let _ = reconcile_network_runtime_native_sync_status_for_runtime(chain_id, normalized);
    }
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

pub fn set_network_runtime_native_header_snapshot_v1(
    chain_id: u64,
    snapshot: NetworkRuntimeNativeHeaderSnapshotV1,
) {
    let normalized = snapshot.normalized(chain_id);
    observe_network_runtime_native_header_source_peer_v1(
        chain_id,
        normalized.hash,
        normalized.source_peer_id,
    );
    if let Ok(mut guard) = runtime_native_header_snapshot_map().lock() {
        let should_replace = guard
            .get(&chain_id)
            .is_none_or(|current| runtime_native_header_should_replace_v1(current, &normalized));
        if should_replace {
            guard.insert(chain_id, normalized.clone());
        }
    }
    if let Ok(mut guard) = runtime_native_canonical_chain_map().lock() {
        let state = guard
            .entry(chain_id)
            .or_insert_with(|| runtime_native_canonical_chain_internal_default_v1(chain_id));
        runtime_native_canonical_chain_upsert_header_v1(state, &normalized);
    }
    if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
        observed
            .native_snapshot_updated_at_by_chain
            .insert(chain_id, normalized.observed_unix_ms);
        hint_runtime_stale_check_deadline(&mut observed, chain_id, normalized.observed_unix_ms);
    }
}

fn runtime_native_header_should_replace_v1(
    current: &NetworkRuntimeNativeHeaderSnapshotV1,
    next: &NetworkRuntimeNativeHeaderSnapshotV1,
) -> bool {
    next.number > current.number
        || (next.number == current.number && next.observed_unix_ms >= current.observed_unix_ms)
}

pub fn set_network_runtime_native_body_snapshot_v1(
    chain_id: u64,
    snapshot: NetworkRuntimeNativeBodySnapshotV1,
) {
    let normalized = snapshot.normalized(chain_id);
    if let Ok(mut guard) = runtime_native_body_snapshot_map().lock() {
        guard.insert(chain_id, normalized.clone());
    }
    if let Ok(mut guard) = runtime_native_canonical_chain_map().lock() {
        let state = guard
            .entry(chain_id)
            .or_insert_with(|| runtime_native_canonical_chain_internal_default_v1(chain_id));
        runtime_native_canonical_chain_upsert_body_v1(state, &normalized);
        if state
            .snapshot
            .head
            .as_ref()
            .is_some_and(|head| head.hash == normalized.block_hash && normalized.body_available)
        {
            if let Some(head) = state.snapshot.head.as_mut() {
                head.body_available = true;
                if let Some(block) = state.blocks_by_hash.get_mut(&head.hash) {
                    block.body_available = true;
                }
            }
        }
        runtime_native_canonical_chain_refresh_snapshot_v1(state);
    }
    if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
        observed
            .native_snapshot_updated_at_by_chain
            .insert(chain_id, normalized.observed_unix_ms);
        hint_runtime_stale_check_deadline(&mut observed, chain_id, normalized.observed_unix_ms);
    }
    runtime_native_pending_tx_observe_body_v1(chain_id, &normalized);
    runtime_native_pending_tx_reconcile_against_canonical_chain_v1(chain_id);
    runtime_native_pending_tx_cleanup_v1(chain_id, normalized.observed_unix_ms);
}

pub fn set_network_runtime_native_header_rlp_v1(
    chain_id: u64,
    block_hash: [u8; 32],
    raw_rlp: &[u8],
) {
    if raw_rlp.is_empty() {
        return;
    }
    if let Ok(mut guard) = runtime_native_header_rlp_map().lock() {
        guard
            .entry(chain_id)
            .or_default()
            .insert(block_hash, raw_rlp.to_vec());
    }
}

pub fn set_network_runtime_native_receipt_snapshot_v1(
    chain_id: u64,
    snapshot: NetworkRuntimeNativeReceiptSnapshotV1,
) {
    let normalized = snapshot.normalized(chain_id);
    observe_network_runtime_native_header_source_peer_v1(
        chain_id,
        normalized.block_hash,
        normalized.source_peer_id,
    );
    if let Ok(mut guard) = runtime_native_receipt_snapshot_map().lock() {
        guard
            .entry(chain_id)
            .or_default()
            .insert(normalized.block_hash, normalized.clone());
    }
    if let Ok(mut guard) = runtime_native_canonical_chain_map().lock() {
        let state = guard
            .entry(chain_id)
            .or_insert_with(|| runtime_native_canonical_chain_internal_default_v1(chain_id));
        runtime_native_canonical_chain_upsert_receipt_v1(state, &normalized);
        if state
            .snapshot
            .head
            .as_ref()
            .is_some_and(|head| head.hash == normalized.block_hash)
        {
            if let Some(head) = state.snapshot.head.as_mut() {
                head.receipts_available |= normalized.receipts_available;
                head.receipt_count = Some(normalized.receipt_count);
                head.receipts_root = Some(normalized.receipts_root);
                head.observed_unix_ms = head.observed_unix_ms.max(normalized.observed_unix_ms);
            }
        }
        runtime_native_canonical_chain_refresh_snapshot_v1(state);
    }
    if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
        observed
            .native_snapshot_updated_at_by_chain
            .insert(chain_id, normalized.observed_unix_ms);
        hint_runtime_stale_check_deadline(&mut observed, chain_id, normalized.observed_unix_ms);
    }
}

pub fn set_network_runtime_native_state_root_validation_v1(
    chain_id: u64,
    block_hash: [u8; 32],
    validated: bool,
    method: &str,
    observed_unix_ms: u128,
) {
    if let Ok(mut guard) = runtime_native_canonical_chain_map().lock() {
        let state = guard
            .entry(chain_id)
            .or_insert_with(|| runtime_native_canonical_chain_internal_default_v1(chain_id));
        if let Some(block) = state.blocks_by_hash.get_mut(&block_hash) {
            block.state_root_validated = validated;
            block.state_root_validation_method = if method.is_empty() {
                None
            } else {
                Some(method.to_string())
            };
            block.observed_unix_ms = block.observed_unix_ms.max(observed_unix_ms);
        }
        if state
            .snapshot
            .head
            .as_ref()
            .is_some_and(|head| head.hash == block_hash)
        {
            if let Some(head) = state.snapshot.head.as_mut() {
                head.state_root_validated = validated;
                head.state_root_validation_method = if method.is_empty() {
                    None
                } else {
                    Some(method.to_string())
                };
                head.observed_unix_ms = head.observed_unix_ms.max(observed_unix_ms);
            }
        }
        runtime_native_canonical_chain_refresh_snapshot_v1(state);
    }
    if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
        observed
            .native_snapshot_updated_at_by_chain
            .insert(chain_id, observed_unix_ms);
        hint_runtime_stale_check_deadline(&mut observed, chain_id, observed_unix_ms);
    }
}

pub fn set_network_runtime_native_block_access_list_payload_v1(
    chain_id: u64,
    block_hash: [u8; 32],
    raw_rlp: &[u8],
) -> Result<(), String> {
    if !eth_rlpx_validate_block_access_list_rlp_v1(raw_rlp) {
        return Err("native_block_access_list_payload_not_rlp_list".to_string());
    }
    let mut guard = runtime_native_block_access_list_payload_map()
        .lock()
        .map_err(|_| "native_block_access_list_payload_lock_poisoned".to_string())?;
    guard
        .entry(chain_id)
        .or_default()
        .insert(block_hash, raw_rlp.to_vec());
    Ok(())
}

#[must_use]
pub fn get_network_runtime_native_block_access_list_payload_v1(
    chain_id: u64,
    block_hash: [u8; 32],
) -> Option<Vec<u8>> {
    let guard = runtime_native_block_access_list_payload_map().lock().ok()?;
    guard
        .get(&chain_id)
        .and_then(|payloads| payloads.get(&block_hash).cloned())
}

pub fn set_network_runtime_native_snap_account_snapshot_v1(
    chain_id: u64,
    snapshot: NetworkRuntimeNativeSnapAccountSnapshotV1,
) {
    let normalized = snapshot.normalized(chain_id);
    if let Ok(mut guard) = runtime_native_snap_account_snapshot_map().lock() {
        guard.entry(chain_id).or_default().insert(
            (normalized.state_root, normalized.account_hash),
            normalized.clone(),
        );
    }
    if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
        observed
            .native_snapshot_updated_at_by_chain
            .insert(chain_id, normalized.observed_unix_ms);
        hint_runtime_stale_check_deadline(&mut observed, chain_id, normalized.observed_unix_ms);
    }
}

#[must_use]
pub fn get_network_runtime_native_snap_account_snapshot_v1(
    chain_id: u64,
    state_root: [u8; 32],
    account_hash: [u8; 32],
) -> Option<NetworkRuntimeNativeSnapAccountSnapshotV1> {
    let guard = runtime_native_snap_account_snapshot_map().lock().ok()?;
    guard
        .get(&chain_id)
        .and_then(|accounts| accounts.get(&(state_root, account_hash)).cloned())
}

#[must_use]
pub fn snapshot_network_runtime_native_snap_account_snapshots_v1(
    chain_id: u64,
    state_root: [u8; 32],
    limit: usize,
) -> Vec<NetworkRuntimeNativeSnapAccountSnapshotV1> {
    if limit == 0 {
        return Vec::new();
    }
    let Ok(guard) = runtime_native_snap_account_snapshot_map().lock() else {
        return Vec::new();
    };
    let mut out = guard
        .get(&chain_id)
        .map(|accounts| {
            accounts
                .values()
                .filter(|snapshot| snapshot.state_root == state_root)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.account_hash.cmp(&b.account_hash));
    out.truncate(limit);
    out
}

pub fn set_network_runtime_native_snap_account_storage_snapshot_v1(
    chain_id: u64,
    snapshot: NetworkRuntimeNativeSnapAccountStorageSnapshotV1,
) {
    let normalized = snapshot.normalized(chain_id);
    if let Ok(mut guard) = runtime_native_snap_storage_snapshot_map().lock() {
        guard.entry(chain_id).or_default().insert(
            (normalized.state_root, normalized.account_hash),
            normalized.clone(),
        );
    }
    if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
        observed
            .native_snapshot_updated_at_by_chain
            .insert(chain_id, normalized.observed_unix_ms);
        hint_runtime_stale_check_deadline(&mut observed, chain_id, normalized.observed_unix_ms);
    }
}

#[must_use]
pub fn get_network_runtime_native_snap_account_storage_snapshot_v1(
    chain_id: u64,
    state_root: [u8; 32],
    account_hash: [u8; 32],
) -> Option<NetworkRuntimeNativeSnapAccountStorageSnapshotV1> {
    let guard = runtime_native_snap_storage_snapshot_map().lock().ok()?;
    guard
        .get(&chain_id)
        .and_then(|accounts| accounts.get(&(state_root, account_hash)).cloned())
}

#[must_use]
pub fn snapshot_network_runtime_native_snap_account_storage_snapshots_v1(
    chain_id: u64,
    state_root: [u8; 32],
    limit: usize,
) -> Vec<NetworkRuntimeNativeSnapAccountStorageSnapshotV1> {
    if limit == 0 {
        return Vec::new();
    }
    let Ok(guard) = runtime_native_snap_storage_snapshot_map().lock() else {
        return Vec::new();
    };
    let mut out = guard
        .get(&chain_id)
        .map(|accounts| {
            accounts
                .values()
                .filter(|snapshot| snapshot.state_root == state_root)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.account_hash.cmp(&b.account_hash));
    out.truncate(limit);
    out
}

pub fn set_network_runtime_native_snap_code_snapshot_v1(
    chain_id: u64,
    snapshot: NetworkRuntimeNativeSnapCodeSnapshotV1,
) {
    let normalized = snapshot.normalized(chain_id);
    if let Ok(mut guard) = runtime_native_snap_code_snapshot_map().lock() {
        guard
            .entry(chain_id)
            .or_default()
            .insert(normalized.code_hash, normalized.clone());
    }
    if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
        observed
            .native_snapshot_updated_at_by_chain
            .insert(chain_id, normalized.observed_unix_ms);
        hint_runtime_stale_check_deadline(&mut observed, chain_id, normalized.observed_unix_ms);
    }
}

#[must_use]
pub fn get_network_runtime_native_snap_code_snapshot_v1(
    chain_id: u64,
    code_hash: [u8; 32],
) -> Option<NetworkRuntimeNativeSnapCodeSnapshotV1> {
    let guard = runtime_native_snap_code_snapshot_map().lock().ok()?;
    guard
        .get(&chain_id)
        .and_then(|codes| codes.get(&code_hash).cloned())
}

#[must_use]
pub fn snapshot_network_runtime_native_snap_code_snapshots_v1(
    chain_id: u64,
    code_hashes: &[[u8; 32]],
    limit: usize,
) -> Vec<NetworkRuntimeNativeSnapCodeSnapshotV1> {
    if limit == 0 || code_hashes.is_empty() {
        return Vec::new();
    }
    let Ok(guard) = runtime_native_snap_code_snapshot_map().lock() else {
        return Vec::new();
    };
    let Some(codes) = guard.get(&chain_id) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for hash in code_hashes {
        if !seen.insert(*hash) {
            continue;
        }
        if let Some(snapshot) = codes.get(hash) {
            out.push(snapshot.clone());
            if out.len() >= limit {
                break;
            }
        }
    }
    out.sort_by(|a, b| a.code_hash.cmp(&b.code_hash));
    out
}

fn network_runtime_native_snap_trie_node_path_key_v1(
    state_root: [u8; 32],
    path_segments: &[Vec<u8>],
) -> NetworkRuntimeNativeSnapTrieNodePathKeyV1 {
    let mut encoded = Vec::new();
    for segment in path_segments {
        encoded.extend_from_slice(&(segment.len() as u32).to_be_bytes());
        encoded.extend_from_slice(segment.as_slice());
    }
    (state_root, encoded)
}

pub fn set_network_runtime_native_snap_trie_node_snapshot_v1(
    chain_id: u64,
    snapshot: NetworkRuntimeNativeSnapTrieNodeSnapshotV1,
) {
    let normalized = snapshot.normalized(chain_id);
    let key = network_runtime_native_snap_trie_node_path_key_v1(
        normalized.state_root,
        normalized.path_segments.as_slice(),
    );
    if let Ok(mut guard) = runtime_native_snap_trie_node_snapshot_map().lock() {
        guard
            .entry(chain_id)
            .or_default()
            .insert(key, normalized.clone());
    }
    if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
        observed
            .native_snapshot_updated_at_by_chain
            .insert(chain_id, normalized.observed_unix_ms);
        hint_runtime_stale_check_deadline(&mut observed, chain_id, normalized.observed_unix_ms);
    }
}

#[must_use]
pub fn get_network_runtime_native_snap_trie_node_snapshot_v1(
    chain_id: u64,
    state_root: [u8; 32],
    path_segments: &[Vec<u8>],
) -> Option<NetworkRuntimeNativeSnapTrieNodeSnapshotV1> {
    let key = network_runtime_native_snap_trie_node_path_key_v1(state_root, path_segments);
    let guard = runtime_native_snap_trie_node_snapshot_map().lock().ok()?;
    guard
        .get(&chain_id)
        .and_then(|nodes| nodes.get(&key).cloned())
}

#[must_use]
pub fn snapshot_network_runtime_native_snap_trie_node_snapshots_v1(
    chain_id: u64,
    state_root: [u8; 32],
    limit: usize,
) -> Vec<NetworkRuntimeNativeSnapTrieNodeSnapshotV1> {
    if limit == 0 {
        return Vec::new();
    }
    let Ok(guard) = runtime_native_snap_trie_node_snapshot_map().lock() else {
        return Vec::new();
    };
    let mut out = guard
        .get(&chain_id)
        .map(|nodes| {
            nodes
                .values()
                .filter(|snapshot| snapshot.state_root == state_root)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| {
        a.path_segments
            .cmp(&b.path_segments)
            .then_with(|| a.node_hash.cmp(&b.node_hash))
    });
    out.truncate(limit);
    out
}

pub fn set_network_runtime_native_snap_account_range_progress_v1(
    chain_id: u64,
    progress: NetworkRuntimeNativeSnapAccountRangeProgressV1,
) {
    let normalized = progress.normalized(chain_id);
    if let Ok(mut guard) = runtime_native_snap_account_range_progress_map().lock() {
        guard
            .entry(chain_id)
            .or_default()
            .insert(normalized.state_root, normalized.clone());
    }
    if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
        observed
            .native_snapshot_updated_at_by_chain
            .insert(chain_id, normalized.observed_unix_ms);
        hint_runtime_stale_check_deadline(&mut observed, chain_id, normalized.observed_unix_ms);
    }
}

#[must_use]
pub fn get_network_runtime_native_snap_account_range_progress_v1(
    chain_id: u64,
    state_root: [u8; 32],
) -> Option<NetworkRuntimeNativeSnapAccountRangeProgressV1> {
    let guard = runtime_native_snap_account_range_progress_map()
        .lock()
        .ok()?;
    guard
        .get(&chain_id)
        .and_then(|progress| progress.get(&state_root).cloned())
}

#[must_use]
pub fn snapshot_network_runtime_native_snap_account_range_progress_v1(
    chain_id: u64,
    state_root: [u8; 32],
) -> Option<NetworkRuntimeNativeSnapAccountRangeProgressV1> {
    get_network_runtime_native_snap_account_range_progress_v1(chain_id, state_root)
}

pub fn set_network_runtime_native_head_snapshot_v1(
    chain_id: u64,
    snapshot: NetworkRuntimeNativeHeadSnapshotV1,
) {
    let normalized = snapshot.normalized(chain_id);
    observe_network_runtime_native_header_source_peer_v1(
        chain_id,
        normalized.block_hash,
        normalized.source_peer_id,
    );
    let observed_unix_ms = normalized.observed_unix_ms;
    let block_number = normalized.block_number;
    let mut should_replace_head = true;
    if let Ok(mut guard) = runtime_native_head_snapshot_map().lock() {
        should_replace_head = guard
            .get(&chain_id)
            .is_none_or(|current| runtime_native_head_should_replace_v1(current, &normalized));
        if should_replace_head {
            guard.insert(chain_id, normalized.clone());
        }
    }
    if should_replace_head {
        if let Ok(mut guard) = runtime_native_canonical_chain_map().lock() {
            let state = guard
                .entry(chain_id)
                .or_insert_with(|| runtime_native_canonical_chain_internal_default_v1(chain_id));
            runtime_native_canonical_chain_apply_head_v1(state, &normalized);
        }
    } else if let Ok(mut guard) = runtime_native_canonical_chain_map().lock() {
        let state = guard
            .entry(chain_id)
            .or_insert_with(|| runtime_native_canonical_chain_internal_default_v1(chain_id));
        if let Some(block) = state.blocks_by_hash.get_mut(&normalized.block_hash) {
            block.safe |= normalized.safe;
            block.finalized |= normalized.finalized;
            block.source_peer_id = normalized.source_peer_id.or(block.source_peer_id);
            block.observed_unix_ms = block.observed_unix_ms.max(normalized.observed_unix_ms);
        }
    }
    if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
        observed
            .native_snapshot_updated_at_by_chain
            .insert(chain_id, observed_unix_ms);
        hint_runtime_stale_check_deadline(&mut observed, chain_id, observed_unix_ms);
    }
    let _ = observe_network_runtime_local_head_max(chain_id, block_number);
    runtime_native_pending_tx_reconcile_against_canonical_chain_v1(chain_id);
    runtime_native_pending_tx_cleanup_v1(chain_id, observed_unix_ms);
}

pub fn restore_network_runtime_native_material_head_v1(
    chain_id: u64,
    header: NetworkRuntimeNativeHeaderSnapshotV1,
    body: NetworkRuntimeNativeBodySnapshotV1,
    receipt: NetworkRuntimeNativeReceiptSnapshotV1,
    head: NetworkRuntimeNativeHeadSnapshotV1,
) -> bool {
    let header = header.normalized(chain_id);
    let body = body.normalized(chain_id);
    let receipt = receipt.normalized(chain_id);
    let head = head.normalized(chain_id);
    if body.number != header.number
        || body.block_hash != header.hash
        || !body.body_available
        || !body.txs_materialized
        || receipt.number != header.number
        || receipt.block_hash != header.hash
        || receipt.receipts_root != header.receipts_root
        || !receipt.receipts_available
        || head.block_number != header.number
        || head.block_hash != header.hash
        || head.parent_block_hash != header.parent_hash
        || head.state_root != header.state_root
        || !head.body_available
    {
        return false;
    }
    let observed_unix_ms = header
        .observed_unix_ms
        .max(body.observed_unix_ms)
        .max(receipt.observed_unix_ms)
        .max(head.observed_unix_ms);
    if let Ok(mut guard) = runtime_native_header_snapshot_map().lock() {
        guard.insert(chain_id, header.clone());
    }
    if let Ok(mut guard) = runtime_native_body_snapshot_map().lock() {
        guard.insert(chain_id, body.clone());
    }
    if let Ok(mut guard) = runtime_native_receipt_snapshot_map().lock() {
        guard
            .entry(chain_id)
            .or_default()
            .insert(receipt.block_hash, receipt.clone());
    }
    if let Ok(mut guard) = runtime_native_head_snapshot_map().lock() {
        guard.insert(chain_id, head.clone());
    }
    if let Ok(mut guard) = runtime_native_canonical_chain_map().lock() {
        let state = guard
            .entry(chain_id)
            .or_insert_with(|| runtime_native_canonical_chain_internal_default_v1(chain_id));
        runtime_native_canonical_chain_upsert_header_v1(state, &header);
        runtime_native_canonical_chain_upsert_body_v1(state, &body);
        runtime_native_canonical_chain_upsert_receipt_v1(state, &receipt);
        runtime_native_canonical_chain_apply_head_v1(state, &head);
    }
    if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
        observed
            .native_snapshot_updated_at_by_chain
            .insert(chain_id, observed_unix_ms);
        hint_runtime_stale_check_deadline(&mut observed, chain_id, observed_unix_ms);
    }
    let _ = observe_network_runtime_local_head(chain_id, header.number);
    runtime_native_pending_tx_reconcile_against_canonical_chain_v1(chain_id);
    runtime_native_pending_tx_cleanup_v1(chain_id, observed_unix_ms);
    true
}

fn runtime_native_head_should_replace_v1(
    current: &NetworkRuntimeNativeHeadSnapshotV1,
    next: &NetworkRuntimeNativeHeadSnapshotV1,
) -> bool {
    next.block_number > current.block_number
        || (next.block_number == current.block_number
            && next.observed_unix_ms >= current.observed_unix_ms)
}

pub fn observe_network_runtime_native_pending_tx_ingress_v1(
    chain_id: u64,
    source_peer_id: u64,
    tx_hash: [u8; 32],
) {
    observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
        chain_id,
        source_peer_id,
        tx_hash,
        None,
    );
}

pub fn observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
    chain_id: u64,
    source_peer_id: u64,
    tx_hash: [u8; 32],
    tx_payload: Option<&[u8]>,
) {
    let now = now_unix_millis();
    let invalid_payload = tx_payload
        .filter(|payload| !payload.is_empty())
        .is_some_and(|payload| !eth_rlpx_validate_transaction_envelope_payload_v1(payload));
    if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
        let chain_txs = guard.entry(chain_id).or_default();
        let tx = chain_txs
            .entry(tx_hash)
            .or_insert_with(|| NetworkRuntimeNativePendingTxStateV1 {
                chain_id,
                tx_hash,
                lifecycle_stage: NetworkRuntimeNativePendingTxLifecycleStageV1::Seen,
                origin: NetworkRuntimeNativePendingTxOriginV1::Remote,
                source_peer_id: Some(source_peer_id),
                first_seen_unix_ms: now,
                last_updated_unix_ms: now,
                last_block_number: None,
                last_block_hash: None,
                canonical_inclusion: None,
                ingress_count: 0,
                propagation_count: 0,
                inclusion_count: 0,
                reorg_back_count: 0,
                drop_count: 0,
                reject_count: 0,
                propagation_attempt_count: 0,
                propagation_success_count: 0,
                propagation_failure_count: 0,
                propagated_peer_count: 0,
                last_propagation_unix_ms: None,
                last_propagation_attempt_unix_ms: None,
                last_propagation_failure_unix_ms: None,
                last_propagation_failure_class: None,
                last_propagation_failure_phase: None,
                last_propagation_peer_id: None,
                last_propagated_peer_id: None,
                propagation_disposition: None,
                propagation_stop_reason: None,
                propagation_recoverability: None,
                retry_eligible: true,
                retry_after_unix_ms: None,
                retry_backoff_level: 0,
                retry_suppressed_reason: None,
                pending_final_disposition:
                    NetworkRuntimeNativePendingTxFinalDispositionV1::Retained,
            });
        if invalid_payload {
            runtime_native_pending_tx_set_rejected_v1(
                tx,
                now,
                Some(source_peer_id),
                NetworkRuntimeNativePendingTxPropagationStopReasonV1::InvalidEnvelope,
                Some("remote_ingress"),
            );
        } else if !matches!(
            tx.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
        ) {
            tx.lifecycle_stage = NetworkRuntimeNativePendingTxLifecycleStageV1::Pending;
            runtime_native_pending_tx_mark_retry_eligible_v1(tx);
        }
        tx.origin = NetworkRuntimeNativePendingTxOriginV1::Remote;
        tx.source_peer_id = Some(source_peer_id);
        tx.ingress_count = tx.ingress_count.saturating_add(1);
        tx.last_updated_unix_ms = now;
    }
    if let Ok(mut payloads_guard) = runtime_native_pending_tx_payload_map().lock() {
        let chain_payloads = payloads_guard.entry(chain_id).or_default();
        if invalid_payload {
            chain_payloads.remove(&tx_hash);
        } else if let Some(payload) = tx_payload.filter(|payload| !payload.is_empty()) {
            chain_payloads.insert(tx_hash, payload.to_vec());
        }
    }
}

pub fn observe_network_runtime_native_pending_tx_local_ingress_with_payload_v1(
    chain_id: u64,
    tx_hash: [u8; 32],
    tx_payload: Option<&[u8]>,
) {
    let now = now_unix_millis();
    let invalid_payload = tx_payload
        .filter(|payload| !payload.is_empty())
        .is_some_and(|payload| !eth_rlpx_validate_transaction_envelope_payload_v1(payload));
    if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
        let chain_txs = guard.entry(chain_id).or_default();
        let tx = chain_txs
            .entry(tx_hash)
            .or_insert_with(|| NetworkRuntimeNativePendingTxStateV1 {
                chain_id,
                tx_hash,
                lifecycle_stage: NetworkRuntimeNativePendingTxLifecycleStageV1::Seen,
                origin: NetworkRuntimeNativePendingTxOriginV1::Local,
                source_peer_id: None,
                first_seen_unix_ms: now,
                last_updated_unix_ms: now,
                last_block_number: None,
                last_block_hash: None,
                canonical_inclusion: None,
                ingress_count: 0,
                propagation_count: 0,
                inclusion_count: 0,
                reorg_back_count: 0,
                drop_count: 0,
                reject_count: 0,
                propagation_attempt_count: 0,
                propagation_success_count: 0,
                propagation_failure_count: 0,
                propagated_peer_count: 0,
                last_propagation_unix_ms: None,
                last_propagation_attempt_unix_ms: None,
                last_propagation_failure_unix_ms: None,
                last_propagation_failure_class: None,
                last_propagation_failure_phase: None,
                last_propagation_peer_id: None,
                last_propagated_peer_id: None,
                propagation_disposition: None,
                propagation_stop_reason: None,
                propagation_recoverability: None,
                retry_eligible: true,
                retry_after_unix_ms: None,
                retry_backoff_level: 0,
                retry_suppressed_reason: None,
                pending_final_disposition:
                    NetworkRuntimeNativePendingTxFinalDispositionV1::Retained,
            });
        if invalid_payload {
            runtime_native_pending_tx_set_rejected_v1(
                tx,
                now,
                None,
                NetworkRuntimeNativePendingTxPropagationStopReasonV1::InvalidEnvelope,
                Some("local_ingress"),
            );
        } else if !matches!(
            tx.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
        ) {
            tx.lifecycle_stage = NetworkRuntimeNativePendingTxLifecycleStageV1::Pending;
            runtime_native_pending_tx_mark_retry_eligible_v1(tx);
        }
        tx.origin = NetworkRuntimeNativePendingTxOriginV1::Local;
        tx.source_peer_id = None;
        tx.ingress_count = tx.ingress_count.saturating_add(1);
        tx.last_updated_unix_ms = now;
    }
    if let Ok(mut payloads_guard) = runtime_native_pending_tx_payload_map().lock() {
        let chain_payloads = payloads_guard.entry(chain_id).or_default();
        if invalid_payload {
            chain_payloads.remove(&tx_hash);
        } else if let Some(payload) = tx_payload.filter(|payload| !payload.is_empty()) {
            chain_payloads.insert(tx_hash, payload.to_vec());
        }
    }
}

pub fn observe_network_runtime_native_pending_tx_local_native_payload_v1(
    chain_id: u64,
    tx_hash: [u8; 32],
    tx_payload: Option<&[u8]>,
) {
    let now = now_unix_millis();
    let mut retain_payload = false;
    let mut already_receipted = false;
    if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
        let chain_txs = guard.entry(chain_id).or_default();
        let tx = chain_txs
            .entry(tx_hash)
            .or_insert_with(|| NetworkRuntimeNativePendingTxStateV1 {
                chain_id,
                tx_hash,
                lifecycle_stage: NetworkRuntimeNativePendingTxLifecycleStageV1::Seen,
                origin: NetworkRuntimeNativePendingTxOriginV1::Local,
                source_peer_id: None,
                first_seen_unix_ms: now,
                last_updated_unix_ms: now,
                last_block_number: None,
                last_block_hash: None,
                canonical_inclusion: None,
                ingress_count: 0,
                propagation_count: 0,
                inclusion_count: 0,
                reorg_back_count: 0,
                drop_count: 0,
                reject_count: 0,
                propagation_attempt_count: 0,
                propagation_success_count: 0,
                propagation_failure_count: 0,
                propagated_peer_count: 0,
                last_propagation_unix_ms: None,
                last_propagation_attempt_unix_ms: None,
                last_propagation_failure_unix_ms: None,
                last_propagation_failure_class: None,
                last_propagation_failure_phase: None,
                last_propagation_peer_id: None,
                last_propagated_peer_id: None,
                propagation_disposition: None,
                propagation_stop_reason: None,
                propagation_recoverability: None,
                retry_eligible: true,
                retry_after_unix_ms: None,
                retry_backoff_level: 0,
                retry_suppressed_reason: None,
                pending_final_disposition:
                    NetworkRuntimeNativePendingTxFinalDispositionV1::Retained,
            });
        already_receipted = matches!(
            tx.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
        );
        if !matches!(
            tx.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
        ) {
            tx.lifecycle_stage = NetworkRuntimeNativePendingTxLifecycleStageV1::Pending;
            runtime_native_pending_tx_mark_retry_eligible_v1(tx);
            retain_payload = true;
        }
        tx.origin = NetworkRuntimeNativePendingTxOriginV1::Local;
        tx.source_peer_id = None;
        tx.ingress_count = tx.ingress_count.saturating_add(1);
        tx.last_updated_unix_ms = now;
    }
    if let Ok(mut payloads_guard) = runtime_native_pending_tx_payload_map().lock() {
        let chain_payloads = payloads_guard.entry(chain_id).or_default();
        if retain_payload {
            if let Some(payload) = tx_payload.filter(|payload| !payload.is_empty()) {
                chain_payloads.insert(tx_hash, payload.to_vec());
            }
        } else {
            chain_payloads.remove(&tx_hash);
        }
    }
    network_runtime_native_repair_probe_observe_pending_view_v1(
        chain_id,
        tx_hash,
        already_receipted,
    );
}

pub fn observe_network_runtime_native_pending_tx_remote_native_payload_v1(
    chain_id: u64,
    source_peer_id: u64,
    tx_hash: [u8; 32],
    tx_payload: Option<&[u8]>,
) {
    let now = now_unix_millis();
    let mut retain_payload = false;
    let mut already_receipted = false;
    if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
        let chain_txs = guard.entry(chain_id).or_default();
        let tx = chain_txs
            .entry(tx_hash)
            .or_insert_with(|| NetworkRuntimeNativePendingTxStateV1 {
                chain_id,
                tx_hash,
                lifecycle_stage: NetworkRuntimeNativePendingTxLifecycleStageV1::Seen,
                origin: NetworkRuntimeNativePendingTxOriginV1::Remote,
                source_peer_id: Some(source_peer_id),
                first_seen_unix_ms: now,
                last_updated_unix_ms: now,
                last_block_number: None,
                last_block_hash: None,
                canonical_inclusion: None,
                ingress_count: 0,
                propagation_count: 0,
                inclusion_count: 0,
                reorg_back_count: 0,
                drop_count: 0,
                reject_count: 0,
                propagation_attempt_count: 0,
                propagation_success_count: 0,
                propagation_failure_count: 0,
                propagated_peer_count: 0,
                last_propagation_unix_ms: None,
                last_propagation_attempt_unix_ms: None,
                last_propagation_failure_unix_ms: None,
                last_propagation_failure_class: None,
                last_propagation_failure_phase: None,
                last_propagation_peer_id: None,
                last_propagated_peer_id: None,
                propagation_disposition: None,
                propagation_stop_reason: None,
                propagation_recoverability: None,
                retry_eligible: true,
                retry_after_unix_ms: None,
                retry_backoff_level: 0,
                retry_suppressed_reason: None,
                pending_final_disposition:
                    NetworkRuntimeNativePendingTxFinalDispositionV1::Retained,
            });
        already_receipted = matches!(
            tx.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
        );
        if !matches!(
            tx.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
        ) {
            tx.lifecycle_stage = NetworkRuntimeNativePendingTxLifecycleStageV1::Pending;
            runtime_native_pending_tx_mark_retry_eligible_v1(tx);
            retain_payload = true;
        }
        tx.origin = NetworkRuntimeNativePendingTxOriginV1::Remote;
        tx.source_peer_id = Some(source_peer_id);
        tx.ingress_count = tx.ingress_count.saturating_add(1);
        tx.last_updated_unix_ms = now;
    }
    if let Ok(mut payloads_guard) = runtime_native_pending_tx_payload_map().lock() {
        let chain_payloads = payloads_guard.entry(chain_id).or_default();
        if retain_payload {
            if let Some(payload) = tx_payload.filter(|payload| !payload.is_empty()) {
                chain_payloads.insert(tx_hash, payload.to_vec());
            }
        } else {
            chain_payloads.remove(&tx_hash);
        }
    }
    network_runtime_native_repair_probe_observe_pending_view_v1(
        chain_id,
        tx_hash,
        already_receipted,
    );
}

pub fn observe_network_runtime_native_pending_tx_propagated_v1(chain_id: u64, tx_hash: [u8; 32]) {
    observe_network_runtime_native_pending_tx_propagated_with_context_v1(
        chain_id, tx_hash, None, None, None,
    );
}

pub fn observe_network_runtime_native_pending_tx_propagated_with_context_v1(
    chain_id: u64,
    tx_hash: [u8; 32],
    source_peer_id: Option<u64>,
    phase: Option<&str>,
    max_propagation_count: Option<u64>,
) {
    let now = now_unix_millis();
    if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
        let chain_txs = guard.entry(chain_id).or_default();
        let tx = chain_txs
            .entry(tx_hash)
            .or_insert_with(|| NetworkRuntimeNativePendingTxStateV1 {
                chain_id,
                tx_hash,
                lifecycle_stage: NetworkRuntimeNativePendingTxLifecycleStageV1::Seen,
                origin: NetworkRuntimeNativePendingTxOriginV1::Local,
                source_peer_id: None,
                first_seen_unix_ms: now,
                last_updated_unix_ms: now,
                last_block_number: None,
                last_block_hash: None,
                canonical_inclusion: None,
                ingress_count: 0,
                propagation_count: 0,
                inclusion_count: 0,
                reorg_back_count: 0,
                drop_count: 0,
                reject_count: 0,
                propagation_attempt_count: 0,
                propagation_success_count: 0,
                propagation_failure_count: 0,
                propagated_peer_count: 0,
                last_propagation_unix_ms: None,
                last_propagation_attempt_unix_ms: None,
                last_propagation_failure_unix_ms: None,
                last_propagation_failure_class: None,
                last_propagation_failure_phase: None,
                last_propagation_peer_id: None,
                last_propagated_peer_id: None,
                propagation_disposition: None,
                propagation_stop_reason: None,
                propagation_recoverability: None,
                retry_eligible: true,
                retry_after_unix_ms: None,
                retry_backoff_level: 0,
                retry_suppressed_reason: None,
                pending_final_disposition:
                    NetworkRuntimeNativePendingTxFinalDispositionV1::Retained,
            });
        if !matches!(
            tx.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
        ) {
            tx.lifecycle_stage = NetworkRuntimeNativePendingTxLifecycleStageV1::Propagated;
        }
        if matches!(tx.origin, NetworkRuntimeNativePendingTxOriginV1::Unknown) {
            tx.origin = NetworkRuntimeNativePendingTxOriginV1::Local;
        }
        tx.propagation_count = tx.propagation_count.saturating_add(1);
        tx.last_updated_unix_ms = now;
        runtime_native_pending_tx_set_propagation_success_metadata_v1(tx, now, source_peer_id);
        if let Some(max) = max_propagation_count.filter(|max| *max > 0) {
            if tx.propagation_count >= max
                && !matches!(
                    tx.lifecycle_stage,
                    NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
                )
            {
                runtime_native_pending_tx_set_dropped_v1(
                    tx,
                    now,
                    source_peer_id,
                    NetworkRuntimeNativePendingTxPropagationStopReasonV1::BudgetLimit,
                    phase.or(Some("broadcast_candidate")),
                );
            }
        }
    }
}

pub fn observe_network_runtime_native_pending_tx_propagation_failure_v1(
    chain_id: u64,
    tx_hash: [u8; 32],
    source_peer_id: Option<u64>,
    stop_reason: NetworkRuntimeNativePendingTxPropagationStopReasonV1,
    failure_phase: &str,
) {
    let now = now_unix_millis();
    if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
        let chain_txs = guard.entry(chain_id).or_default();
        let tx = chain_txs
            .entry(tx_hash)
            .or_insert_with(|| NetworkRuntimeNativePendingTxStateV1 {
                chain_id,
                tx_hash,
                lifecycle_stage: NetworkRuntimeNativePendingTxLifecycleStageV1::Seen,
                origin: if source_peer_id.is_some() {
                    NetworkRuntimeNativePendingTxOriginV1::Remote
                } else {
                    NetworkRuntimeNativePendingTxOriginV1::Local
                },
                source_peer_id,
                first_seen_unix_ms: now,
                last_updated_unix_ms: now,
                last_block_number: None,
                last_block_hash: None,
                canonical_inclusion: None,
                ingress_count: 0,
                propagation_count: 0,
                inclusion_count: 0,
                reorg_back_count: 0,
                drop_count: 0,
                reject_count: 0,
                propagation_attempt_count: 0,
                propagation_success_count: 0,
                propagation_failure_count: 0,
                propagated_peer_count: 0,
                last_propagation_unix_ms: None,
                last_propagation_attempt_unix_ms: None,
                last_propagation_failure_unix_ms: None,
                last_propagation_failure_class: None,
                last_propagation_failure_phase: None,
                last_propagation_peer_id: None,
                last_propagated_peer_id: None,
                propagation_disposition: None,
                propagation_stop_reason: None,
                propagation_recoverability: None,
                retry_eligible: true,
                retry_after_unix_ms: None,
                retry_backoff_level: 0,
                retry_suppressed_reason: None,
                pending_final_disposition:
                    NetworkRuntimeNativePendingTxFinalDispositionV1::Retained,
            });
        if matches!(
            runtime_native_pending_tx_stop_reason_recoverability_v1(stop_reason),
            NetworkRuntimeNativePendingTxPropagationRecoverabilityV1::NonRecoverable
        ) {
            runtime_native_pending_tx_set_rejected_v1(
                tx,
                now,
                source_peer_id,
                stop_reason,
                Some(failure_phase),
            );
        } else {
            runtime_native_pending_tx_set_dropped_v1(
                tx,
                now,
                source_peer_id,
                stop_reason,
                Some(failure_phase),
            );
        }
    }
    if matches!(
        runtime_native_pending_tx_stop_reason_recoverability_v1(stop_reason),
        NetworkRuntimeNativePendingTxPropagationRecoverabilityV1::NonRecoverable
    ) {
        if let Ok(mut payloads_guard) = runtime_native_pending_tx_payload_map().lock() {
            if let Some(chain_payloads) = payloads_guard.get_mut(&chain_id) {
                chain_payloads.remove(&tx_hash);
            }
        }
    }
}

pub fn observe_network_runtime_native_pending_tx_rejected_v1(
    chain_id: u64,
    tx_hash: [u8; 32],
    source_peer_id: Option<u64>,
) {
    let now = now_unix_millis();
    if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
        let chain_txs = guard.entry(chain_id).or_default();
        let tx = chain_txs
            .entry(tx_hash)
            .or_insert_with(|| NetworkRuntimeNativePendingTxStateV1 {
                chain_id,
                tx_hash,
                lifecycle_stage: NetworkRuntimeNativePendingTxLifecycleStageV1::Seen,
                origin: if source_peer_id.is_some() {
                    NetworkRuntimeNativePendingTxOriginV1::Remote
                } else {
                    NetworkRuntimeNativePendingTxOriginV1::Local
                },
                source_peer_id,
                first_seen_unix_ms: now,
                last_updated_unix_ms: now,
                last_block_number: None,
                last_block_hash: None,
                canonical_inclusion: None,
                ingress_count: 0,
                propagation_count: 0,
                inclusion_count: 0,
                reorg_back_count: 0,
                drop_count: 0,
                reject_count: 0,
                propagation_attempt_count: 0,
                propagation_success_count: 0,
                propagation_failure_count: 0,
                propagated_peer_count: 0,
                last_propagation_unix_ms: None,
                last_propagation_attempt_unix_ms: None,
                last_propagation_failure_unix_ms: None,
                last_propagation_failure_class: None,
                last_propagation_failure_phase: None,
                last_propagation_peer_id: None,
                last_propagated_peer_id: None,
                propagation_disposition: None,
                propagation_stop_reason: None,
                propagation_recoverability: None,
                retry_eligible: true,
                retry_after_unix_ms: None,
                retry_backoff_level: 0,
                retry_suppressed_reason: None,
                pending_final_disposition:
                    NetworkRuntimeNativePendingTxFinalDispositionV1::Retained,
            });
        runtime_native_pending_tx_set_rejected_v1(
            tx,
            now,
            source_peer_id,
            NetworkRuntimeNativePendingTxPropagationStopReasonV1::PolicyRejected,
            Some("runtime_observe"),
        );
        if source_peer_id.is_some() {
            tx.origin = NetworkRuntimeNativePendingTxOriginV1::Remote;
        } else {
            tx.origin = NetworkRuntimeNativePendingTxOriginV1::Local;
            tx.source_peer_id = None;
        }
    }
    if let Ok(mut payloads_guard) = runtime_native_pending_tx_payload_map().lock() {
        if let Some(chain_payloads) = payloads_guard.get_mut(&chain_id) {
            chain_payloads.remove(&tx_hash);
        }
    }
}

pub fn observe_network_runtime_native_pending_tx_dropped_v1(chain_id: u64, tx_hash: [u8; 32]) {
    let now = now_unix_millis();
    if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
        let chain_txs = guard.entry(chain_id).or_default();
        let tx = chain_txs
            .entry(tx_hash)
            .or_insert_with(|| NetworkRuntimeNativePendingTxStateV1 {
                chain_id,
                tx_hash,
                lifecycle_stage: NetworkRuntimeNativePendingTxLifecycleStageV1::Seen,
                origin: NetworkRuntimeNativePendingTxOriginV1::Unknown,
                source_peer_id: None,
                first_seen_unix_ms: now,
                last_updated_unix_ms: now,
                last_block_number: None,
                last_block_hash: None,
                canonical_inclusion: None,
                ingress_count: 0,
                propagation_count: 0,
                inclusion_count: 0,
                reorg_back_count: 0,
                drop_count: 0,
                reject_count: 0,
                propagation_attempt_count: 0,
                propagation_success_count: 0,
                propagation_failure_count: 0,
                propagated_peer_count: 0,
                last_propagation_unix_ms: None,
                last_propagation_attempt_unix_ms: None,
                last_propagation_failure_unix_ms: None,
                last_propagation_failure_class: None,
                last_propagation_failure_phase: None,
                last_propagation_peer_id: None,
                last_propagated_peer_id: None,
                propagation_disposition: None,
                propagation_stop_reason: None,
                propagation_recoverability: None,
                retry_eligible: true,
                retry_after_unix_ms: None,
                retry_backoff_level: 0,
                retry_suppressed_reason: None,
                pending_final_disposition:
                    NetworkRuntimeNativePendingTxFinalDispositionV1::Retained,
            });
        runtime_native_pending_tx_set_dropped_v1(
            tx,
            now,
            None,
            NetworkRuntimeNativePendingTxPropagationStopReasonV1::PhaseStall,
            Some("runtime_observe"),
        );
    }
    if let Ok(mut payloads_guard) = runtime_native_pending_tx_payload_map().lock() {
        if let Some(chain_payloads) = payloads_guard.get_mut(&chain_id) {
            chain_payloads.remove(&tx_hash);
        }
    }
}

pub fn observe_network_runtime_native_pending_tx_broadcast_dispatch_v1(
    chain_id: u64,
    peer_id: u64,
    candidate_count: u64,
    broadcast_count: u64,
    success: bool,
) {
    let now = now_unix_millis();
    if let Ok(mut guard) = runtime_native_pending_tx_broadcast_runtime_map().lock() {
        let summary = guard.entry(chain_id).or_insert_with(|| {
            NetworkRuntimeNativePendingTxBroadcastRuntimeSummaryV1 {
                chain_id,
                ..Default::default()
            }
        });
        summary.chain_id = chain_id;
        summary.dispatch_total = summary.dispatch_total.saturating_add(1);
        if success {
            summary.dispatch_success_total = summary.dispatch_success_total.saturating_add(1);
        } else {
            summary.dispatch_failed_total = summary.dispatch_failed_total.saturating_add(1);
        }
        summary.candidate_tx_total = summary.candidate_tx_total.saturating_add(candidate_count);
        summary.broadcast_tx_total = summary.broadcast_tx_total.saturating_add(broadcast_count);
        summary.last_peer_id = Some(peer_id);
        summary.last_candidate_count = candidate_count;
        summary.last_broadcast_tx_count = broadcast_count;
        summary.last_updated_unix_ms = Some(now);
    }
}

pub fn observe_network_runtime_native_execution_budget_throttle_v1(
    chain_id: u64,
    reason: &str,
    deferred_count: u64,
    budget_hit: bool,
    time_slice_exceeded: bool,
) {
    let now = now_unix_millis();
    if let Ok(mut guard) = runtime_native_execution_budget_runtime_map().lock() {
        let summary = guard.entry(chain_id).or_insert_with(|| {
            NetworkRuntimeNativeExecutionBudgetRuntimeSummaryV1 {
                chain_id,
                ..Default::default()
            }
        });
        summary.chain_id = chain_id;
        if budget_hit {
            summary.execution_budget_hit_count =
                summary.execution_budget_hit_count.saturating_add(1);
        }
        if time_slice_exceeded {
            summary.execution_time_slice_exceeded_count = summary
                .execution_time_slice_exceeded_count
                .saturating_add(1);
        }
        summary.execution_deferred_count = summary
            .execution_deferred_count
            .saturating_add(deferred_count);
        summary.last_execution_throttle_reason = Some(reason.to_string());
        summary.last_updated_unix_ms = Some(now);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRuntimeNativeExecutionBudgetTargetObservationV1 {
    pub hard_budget_per_tick: u64,
    pub hard_time_slice_ms: u64,
    pub target_budget_per_tick: u64,
    pub target_time_slice_ms: u64,
    pub effective_budget_per_tick: u64,
    pub effective_time_slice_ms: u64,
    pub reason: Option<String>,
}

pub fn observe_network_runtime_native_execution_budget_target_v1(
    chain_id: u64,
    observation: &NetworkRuntimeNativeExecutionBudgetTargetObservationV1,
) {
    let now = now_unix_millis();
    let hard_budget_per_tick = observation.hard_budget_per_tick.max(1);
    let hard_time_slice_ms = observation.hard_time_slice_ms.max(1);
    if let Ok(mut guard) = runtime_native_execution_budget_runtime_map().lock() {
        let summary = guard.entry(chain_id).or_insert_with(|| {
            NetworkRuntimeNativeExecutionBudgetRuntimeSummaryV1 {
                chain_id,
                ..Default::default()
            }
        });
        summary.chain_id = chain_id;
        summary.hard_budget_per_tick = Some(hard_budget_per_tick);
        summary.hard_time_slice_ms = Some(hard_time_slice_ms);
        summary.target_budget_per_tick = Some(
            observation
                .target_budget_per_tick
                .max(1)
                .min(hard_budget_per_tick),
        );
        summary.target_time_slice_ms = Some(
            observation
                .target_time_slice_ms
                .max(1)
                .min(hard_time_slice_ms),
        );
        summary.effective_budget_per_tick = Some(
            observation
                .effective_budget_per_tick
                .max(1)
                .min(hard_budget_per_tick),
        );
        summary.effective_time_slice_ms = Some(
            observation
                .effective_time_slice_ms
                .max(1)
                .min(hard_time_slice_ms),
        );
        summary.last_execution_target_reason = observation.reason.clone();
        summary.last_updated_unix_ms = Some(now);
    }
}

#[must_use]
pub fn snapshot_network_runtime_native_execution_budget_runtime_summary_v1(
    chain_id: u64,
) -> NetworkRuntimeNativeExecutionBudgetRuntimeSummaryV1 {
    runtime_native_execution_budget_runtime_map()
        .lock()
        .ok()
        .and_then(|guard| guard.get(&chain_id).cloned())
        .unwrap_or(NetworkRuntimeNativeExecutionBudgetRuntimeSummaryV1 {
            chain_id,
            ..Default::default()
        })
}

#[cfg(test)]
pub fn clear_network_runtime_native_snapshots_for_chain_v1(chain_id: u64) {
    if let Ok(mut guard) = runtime_native_header_snapshot_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_body_snapshot_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_header_rlp_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_receipt_snapshot_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_block_access_list_payload_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_snap_account_snapshot_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_snap_storage_snapshot_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_snap_code_snapshot_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_snap_trie_node_snapshot_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_snap_account_range_progress_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_head_snapshot_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_sync_status_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_canonical_chain_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_pending_tx_payload_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_pending_tx_tombstone_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_pending_tx_broadcast_runtime_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_repair_probe_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_novorudp_sequence_ledger_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_execution_budget_runtime_map().lock() {
        guard.remove(&chain_id);
    }
    if let Ok(mut guard) = runtime_native_budget_hooks_map().lock() {
        guard.remove(&chain_id);
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
        observed.remote_best_hint_by_chain.remove(&chain_id);
        observed
            .remote_best_hint_updated_at_by_chain
            .remove(&chain_id);
        observed.next_stale_check_at_by_chain.remove(&chain_id);
        observed.dirty_chains.remove(&chain_id);
        observed.sync_anchor_by_chain.remove(&chain_id);
    }
    crate::eth_fullnode::clear_eth_fullnode_test_state_for_chain_v1(chain_id);
}

pub fn set_network_runtime_peer_count(chain_id: u64, peer_count: u64) {
    let mut normalized: Option<NetworkRuntimeSyncStatus> = None;
    let mut changed = true;
    if let Ok(mut guard) = runtime_sync_status_map().lock() {
        let mut status = guard
            .get(&chain_id)
            .copied()
            .unwrap_or_else(empty_runtime_sync_status);
        status.peer_count = peer_count;
        let next = status.normalized();
        changed = guard.get(&chain_id).copied() != Some(next);
        if changed {
            guard.insert(chain_id, next);
            normalized = Some(next);
        }
    }
    if let Some(next) = normalized.filter(|_| changed) {
        let _ = reconcile_network_runtime_native_sync_status_for_runtime(chain_id, next);
    }
}

pub fn set_network_runtime_block_progress(
    chain_id: u64,
    starting_block: u64,
    current_block: u64,
    highest_block: u64,
) {
    let mut normalized: Option<NetworkRuntimeSyncStatus> = None;
    let mut changed = true;
    if let Ok(mut guard) = runtime_sync_status_map().lock() {
        let mut status = guard
            .get(&chain_id)
            .copied()
            .unwrap_or_else(empty_runtime_sync_status);
        status.starting_block = starting_block;
        status.current_block = current_block;
        status.highest_block = highest_block;
        let next = status.normalized();
        changed = guard.get(&chain_id).copied() != Some(next);
        if changed {
            guard.insert(chain_id, next);
            normalized = Some(next);
        }
    }
    if let Some(next) = normalized.filter(|_| changed) {
        let _ = reconcile_network_runtime_native_sync_status_for_runtime(chain_id, next);
    }
}

#[must_use]
pub fn register_network_runtime_peer(
    chain_id: u64,
    peer_id: u64,
) -> Option<NetworkRuntimeSyncStatus> {
    let mut statuses = runtime_sync_status_map().lock().ok()?;
    let mut observed = runtime_sync_observed_state_map().lock().ok()?;
    let now = now_unix_millis();
    let cleared_native_hint = observed
        .native_peer_count_by_chain
        .remove(&chain_id)
        .is_some();
    let peers = observed.peer_height_by_chain.entry(chain_id).or_default();
    let peer_was_present = peers.contains_key(&peer_id);
    peers.entry(peer_id).or_insert(0);
    observed.peer_observed_once_by_chain.insert(chain_id);
    observed
        .peer_last_seen_millis_by_chain
        .entry(chain_id)
        .or_default()
        .insert(peer_id, now);
    hint_runtime_stale_check_deadline(&mut observed, chain_id, now);
    if peer_was_present && !cleared_native_hint {
        let current = statuses.get(&chain_id).copied();
        drop(observed);
        drop(statuses);
        return current;
    }
    mark_runtime_sync_observed_dirty(&mut observed, chain_id);
    let recomputed =
        recompute_runtime_sync_status_from_observed(chain_id, &mut statuses, &mut observed);
    drop(observed);
    drop(statuses);
    let _ = reconcile_network_runtime_native_sync_status_for_runtime(chain_id, recomputed);
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
    observed.next_stale_check_at_by_chain.remove(&chain_id);
    mark_runtime_sync_observed_dirty(&mut observed, chain_id);
    let recomputed =
        recompute_runtime_sync_status_from_observed(chain_id, &mut statuses, &mut observed);
    drop(observed);
    drop(statuses);
    let _ = reconcile_network_runtime_native_sync_status_for_runtime(chain_id, recomputed);
    Some(recomputed)
}

#[must_use]
pub fn observe_network_runtime_peer_head(
    chain_id: u64,
    peer_id: u64,
    peer_head: u64,
) -> Option<NetworkRuntimeSyncStatus> {
    observe_network_runtime_peer_head_with_local_head_max(chain_id, peer_id, peer_head, None)
}

#[must_use]
pub fn observe_network_runtime_peer_head_with_local_head_max(
    chain_id: u64,
    peer_id: u64,
    peer_head: u64,
    local_head_max: Option<u64>,
) -> Option<NetworkRuntimeSyncStatus> {
    let mut statuses = runtime_sync_status_map().lock().ok()?;
    let mut observed = runtime_sync_observed_state_map().lock().ok()?;
    let now = now_unix_millis();
    let cleared_native_hint = observed
        .native_peer_count_by_chain
        .remove(&chain_id)
        .is_some();
    let previous_peer_head = observed
        .peer_height_by_chain
        .get(&chain_id)
        .and_then(|m| m.get(&peer_id))
        .copied();
    observed
        .peer_height_by_chain
        .entry(chain_id)
        .or_default()
        .insert(peer_id, peer_head);
    observed
        .remote_best_hint_by_chain
        .entry(chain_id)
        .and_modify(|best| *best = (*best).max(peer_head))
        .or_insert(peer_head);
    observed
        .remote_best_hint_updated_at_by_chain
        .insert(chain_id, now);
    observed.peer_observed_once_by_chain.insert(chain_id);
    observed
        .peer_last_seen_millis_by_chain
        .entry(chain_id)
        .or_default()
        .insert(peer_id, now);
    hint_runtime_stale_check_deadline(&mut observed, chain_id, now);
    let peer_changed = previous_peer_head != Some(peer_head);
    let mut local_changed = false;
    if let Some(local_head) = local_head_max {
        let previous_local = observed.local_head_by_chain.get(&chain_id).copied();
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
        local_changed = previous_local != Some(merged);
    }
    if !peer_changed && !local_changed && !cleared_native_hint {
        let current = statuses.get(&chain_id).copied();
        drop(observed);
        drop(statuses);
        return current;
    }
    mark_runtime_sync_observed_dirty(&mut observed, chain_id);
    let recomputed =
        recompute_runtime_sync_status_from_observed(chain_id, &mut statuses, &mut observed);
    drop(observed);
    drop(statuses);
    let _ = reconcile_network_runtime_native_sync_status_for_runtime(chain_id, recomputed);
    Some(recomputed)
}

#[must_use]
pub fn observe_network_runtime_local_head(
    chain_id: u64,
    local_head: u64,
) -> Option<NetworkRuntimeSyncStatus> {
    let mut statuses = runtime_sync_status_map().lock().ok()?;
    let mut observed = runtime_sync_observed_state_map().lock().ok()?;
    let previous_local = observed.local_head_by_chain.insert(chain_id, local_head);
    if previous_local == Some(local_head)
        && !runtime_sync_recompute_due(chain_id, &statuses, &observed, true)
    {
        let current = statuses.get(&chain_id).copied();
        drop(observed);
        drop(statuses);
        return current;
    }
    mark_runtime_sync_observed_dirty(&mut observed, chain_id);
    let recomputed =
        recompute_runtime_sync_status_from_observed(chain_id, &mut statuses, &mut observed);
    drop(observed);
    drop(statuses);
    let _ = reconcile_network_runtime_native_sync_status_for_runtime(chain_id, recomputed);
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
    let next_remote_best = snapshot.highest_head.max(snapshot.local_head);
    let previous_local = observed
        .local_head_by_chain
        .insert(chain_id, snapshot.local_head);
    let previous_peer_count = observed
        .native_peer_count_by_chain
        .insert(chain_id, snapshot.peer_count);
    let previous_remote_best = observed
        .native_remote_best_by_chain
        .insert(chain_id, next_remote_best);
    let previous_remote_best_hint = observed.remote_best_hint_by_chain.get(&chain_id).copied();
    observed
        .remote_best_hint_by_chain
        .entry(chain_id)
        .and_modify(|best| *best = (*best).max(next_remote_best))
        .or_insert(next_remote_best);
    observed
        .remote_best_hint_updated_at_by_chain
        .insert(chain_id, now);
    let observed_once_before = observed.peer_observed_once_by_chain.contains(&chain_id);
    observed
        .native_snapshot_updated_at_by_chain
        .insert(chain_id, now);
    hint_runtime_stale_check_deadline(&mut observed, chain_id, now);
    observed.peer_observed_once_by_chain.insert(chain_id);
    let snapshot_changed = previous_local != Some(snapshot.local_head)
        || previous_peer_count != Some(snapshot.peer_count)
        || previous_remote_best != Some(next_remote_best)
        || previous_remote_best_hint.is_none_or(|best| next_remote_best > best)
        || !observed_once_before;
    if !snapshot_changed && !runtime_sync_recompute_due(chain_id, &statuses, &observed, true) {
        let current = statuses.get(&chain_id).copied();
        drop(observed);
        drop(statuses);
        return current;
    }
    mark_runtime_sync_observed_dirty(&mut observed, chain_id);
    let recomputed =
        recompute_runtime_sync_status_from_observed(chain_id, &mut statuses, &mut observed);
    drop(observed);
    drop(statuses);

    let _ = reconcile_network_runtime_native_sync_status_for_runtime(chain_id, recomputed);

    Some(recomputed)
}

#[must_use]
pub fn observe_network_runtime_local_head_max(
    chain_id: u64,
    local_head: u64,
) -> Option<NetworkRuntimeSyncStatus> {
    let mut statuses = runtime_sync_status_map().lock().ok()?;
    let mut observed = runtime_sync_observed_state_map().lock().ok()?;
    let previous_local = observed
        .local_head_by_chain
        .get(&chain_id)
        .copied()
        .unwrap_or_else(|| {
            statuses
                .get(&chain_id)
                .map(|s| s.current_block)
                .unwrap_or_default()
        });
    let merged = previous_local.max(local_head);
    observed.local_head_by_chain.insert(chain_id, merged);
    if merged == previous_local && !runtime_sync_recompute_due(chain_id, &statuses, &observed, true)
    {
        let current = statuses.get(&chain_id).copied();
        drop(observed);
        drop(statuses);
        return current;
    }
    mark_runtime_sync_observed_dirty(&mut observed, chain_id);
    let recomputed =
        recompute_runtime_sync_status_from_observed(chain_id, &mut statuses, &mut observed);
    drop(observed);
    drop(statuses);
    let _ = reconcile_network_runtime_native_sync_status_for_runtime(chain_id, recomputed);
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
    if !runtime_sync_recompute_due(chain_id, &statuses, &observed, has_observed) {
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
pub fn get_network_runtime_native_header_snapshot_v1(
    chain_id: u64,
) -> Option<NetworkRuntimeNativeHeaderSnapshotV1> {
    let guard = runtime_native_header_snapshot_map().lock().ok()?;
    guard.get(&chain_id).cloned()
}

#[must_use]
pub fn get_network_runtime_native_body_snapshot_v1(
    chain_id: u64,
) -> Option<NetworkRuntimeNativeBodySnapshotV1> {
    let guard = runtime_native_body_snapshot_map().lock().ok()?;
    guard.get(&chain_id).cloned()
}

#[must_use]
pub fn get_network_runtime_native_header_rlp_v1(
    chain_id: u64,
    block_hash: [u8; 32],
) -> Option<Vec<u8>> {
    let guard = runtime_native_header_rlp_map().lock().ok()?;
    guard
        .get(&chain_id)
        .and_then(|headers| headers.get(&block_hash).cloned())
}

#[must_use]
pub fn snapshot_network_runtime_native_header_source_peers_v1(
    chain_id: u64,
    block_hash: [u8; 32],
) -> Vec<u64> {
    let Ok(guard) = runtime_native_header_source_peer_map().lock() else {
        return Vec::new();
    };
    let mut peers = guard
        .get(&chain_id)
        .and_then(|by_hash| by_hash.get(&block_hash))
        .map(|source_peers| source_peers.iter().copied().collect::<Vec<_>>())
        .unwrap_or_default();
    peers.sort_unstable();
    peers
}

#[must_use]
pub fn get_network_runtime_native_receipt_snapshot_v1(
    chain_id: u64,
    block_hash: [u8; 32],
) -> Option<NetworkRuntimeNativeReceiptSnapshotV1> {
    let guard = runtime_native_receipt_snapshot_map().lock().ok()?;
    guard
        .get(&chain_id)
        .and_then(|receipts| receipts.get(&block_hash).cloned())
}

#[must_use]
pub fn get_network_runtime_native_head_snapshot_v1(
    chain_id: u64,
) -> Option<NetworkRuntimeNativeHeadSnapshotV1> {
    let guard = runtime_native_head_snapshot_map().lock().ok()?;
    guard.get(&chain_id).cloned()
}

#[must_use]
pub fn snapshot_network_runtime_native_canonical_chain_v1(
    chain_id: u64,
) -> Option<NetworkRuntimeNativeCanonicalChainStateV1> {
    let guard = runtime_native_canonical_chain_map().lock().ok()?;
    guard.get(&chain_id).map(|state| state.snapshot.clone())
}

#[must_use]
pub fn snapshot_network_runtime_native_canonical_blocks_v1(
    chain_id: u64,
    limit: usize,
) -> Vec<NetworkRuntimeNativeCanonicalBlockStateV1> {
    let guard = match runtime_native_canonical_chain_map().lock() {
        Ok(guard) => guard,
        Err(_) => return Vec::new(),
    };
    let Some(state) = guard.get(&chain_id) else {
        return Vec::new();
    };
    let mut blocks = state.blocks_by_hash.values().cloned().collect::<Vec<_>>();
    blocks.sort_by(|a, b| {
        b.number
            .cmp(&a.number)
            .then_with(|| b.canonical.cmp(&a.canonical))
            .then_with(|| b.observed_unix_ms.cmp(&a.observed_unix_ms))
            .then_with(|| b.hash.cmp(&a.hash))
    });
    if limit == 0 || blocks.len() <= limit {
        return blocks;
    }
    blocks.truncate(limit);
    blocks
}

#[must_use]
pub fn snapshot_network_runtime_native_pending_tx_summary_v1(
    chain_id: u64,
) -> NetworkRuntimeNativePendingTxSummaryV1 {
    runtime_native_pending_tx_cleanup_v1(chain_id, now_unix_millis());
    let broadcast_runtime =
        snapshot_network_runtime_native_pending_tx_broadcast_runtime_summary_v1(chain_id);
    let repair_probe = network_runtime_native_repair_probe_summary_v1(chain_id);
    let tombstone_counts = runtime_native_pending_tx_tombstone_map()
        .lock()
        .ok()
        .and_then(|guard| guard.get(&chain_id).cloned())
        .map(|tombstones| {
            let evicted = tombstones
                .values()
                .filter(|entry| {
                    matches!(
                        entry.final_disposition,
                        NetworkRuntimeNativePendingTxFinalDispositionV1::Evicted
                    )
                })
                .count();
            let expired = tombstones
                .values()
                .filter(|entry| {
                    matches!(
                        entry.final_disposition,
                        NetworkRuntimeNativePendingTxFinalDispositionV1::Expired
                    )
                })
                .count();
            (evicted, expired)
        })
        .unwrap_or((0, 0));
    let guard = match runtime_native_pending_tx_map().lock() {
        Ok(guard) => guard,
        Err(_) => {
            let mut summary = NetworkRuntimeNativePendingTxSummaryV1 {
                chain_id,
                ..Default::default()
            };
            summary.broadcast_dispatch_total = broadcast_runtime.dispatch_total;
            summary.broadcast_dispatch_success_total = broadcast_runtime.dispatch_success_total;
            summary.broadcast_dispatch_failed_total = broadcast_runtime.dispatch_failed_total;
            summary.broadcast_candidate_tx_total = broadcast_runtime.candidate_tx_total;
            summary.broadcast_tx_total = broadcast_runtime.broadcast_tx_total;
            summary.evicted_count = tombstone_counts.0;
            summary.expired_count = tombstone_counts.1;
            summary.last_broadcast_peer_id = broadcast_runtime.last_peer_id;
            summary.last_broadcast_candidate_count = broadcast_runtime.last_candidate_count;
            summary.last_broadcast_tx_count = broadcast_runtime.last_broadcast_tx_count;
            summary.last_broadcast_unix_ms = broadcast_runtime.last_updated_unix_ms;
            apply_network_runtime_native_repair_probe_summary_v1(&mut summary, &repair_probe);
            apply_novorudp_durable_missing_summary_v1(&mut summary, chain_id);
            return summary;
        }
    };
    let Some(chain_txs) = guard.get(&chain_id) else {
        let mut summary = NetworkRuntimeNativePendingTxSummaryV1 {
            chain_id,
            ..Default::default()
        };
        summary.broadcast_dispatch_total = broadcast_runtime.dispatch_total;
        summary.broadcast_dispatch_success_total = broadcast_runtime.dispatch_success_total;
        summary.broadcast_dispatch_failed_total = broadcast_runtime.dispatch_failed_total;
        summary.broadcast_candidate_tx_total = broadcast_runtime.candidate_tx_total;
        summary.broadcast_tx_total = broadcast_runtime.broadcast_tx_total;
        summary.evicted_count = tombstone_counts.0;
        summary.expired_count = tombstone_counts.1;
        summary.last_broadcast_peer_id = broadcast_runtime.last_peer_id;
        summary.last_broadcast_candidate_count = broadcast_runtime.last_candidate_count;
        summary.last_broadcast_tx_count = broadcast_runtime.last_broadcast_tx_count;
        summary.last_broadcast_unix_ms = broadcast_runtime.last_updated_unix_ms;
        apply_network_runtime_native_repair_probe_summary_v1(&mut summary, &repair_probe);
        apply_novorudp_durable_missing_summary_v1(&mut summary, chain_id);
        return summary;
    };
    let mut summary = runtime_native_pending_tx_summarize_v1(chain_id, chain_txs);
    summary.broadcast_dispatch_total = broadcast_runtime.dispatch_total;
    summary.broadcast_dispatch_success_total = broadcast_runtime.dispatch_success_total;
    summary.broadcast_dispatch_failed_total = broadcast_runtime.dispatch_failed_total;
    summary.broadcast_candidate_tx_total = broadcast_runtime.candidate_tx_total;
    summary.broadcast_tx_total = broadcast_runtime.broadcast_tx_total;
    summary.evicted_count = tombstone_counts.0;
    summary.expired_count = tombstone_counts.1;
    summary.last_broadcast_peer_id = broadcast_runtime.last_peer_id;
    summary.last_broadcast_candidate_count = broadcast_runtime.last_candidate_count;
    summary.last_broadcast_tx_count = broadcast_runtime.last_broadcast_tx_count;
    summary.last_broadcast_unix_ms = broadcast_runtime.last_updated_unix_ms;
    apply_network_runtime_native_repair_probe_summary_v1(&mut summary, &repair_probe);
    apply_novorudp_durable_missing_summary_v1(&mut summary, chain_id);
    summary
}

#[must_use]
pub fn snapshot_network_runtime_native_pending_tx_broadcast_runtime_summary_v1(
    chain_id: u64,
) -> NetworkRuntimeNativePendingTxBroadcastRuntimeSummaryV1 {
    let guard = match runtime_native_pending_tx_broadcast_runtime_map().lock() {
        Ok(guard) => guard,
        Err(_) => {
            return NetworkRuntimeNativePendingTxBroadcastRuntimeSummaryV1 {
                chain_id,
                ..Default::default()
            };
        }
    };
    guard.get(&chain_id).cloned().unwrap_or(
        NetworkRuntimeNativePendingTxBroadcastRuntimeSummaryV1 {
            chain_id,
            ..Default::default()
        },
    )
}

#[must_use]
pub fn snapshot_network_runtime_native_pending_txs_v1(
    chain_id: u64,
    limit: usize,
) -> Vec<NetworkRuntimeNativePendingTxStateV1> {
    runtime_native_pending_tx_cleanup_v1(chain_id, now_unix_millis());
    let guard = match runtime_native_pending_tx_map().lock() {
        Ok(guard) => guard,
        Err(_) => return Vec::new(),
    };
    let Some(chain_txs) = guard.get(&chain_id) else {
        return Vec::new();
    };
    let mut ordered = chain_txs.values().collect::<Vec<_>>();
    ordered.sort_by(|a, b| {
        b.last_updated_unix_ms
            .cmp(&a.last_updated_unix_ms)
            .then_with(|| b.tx_hash.cmp(&a.tx_hash))
    });
    if limit > 0 && ordered.len() > limit {
        ordered.truncate(limit);
    }
    ordered.into_iter().cloned().collect()
}

#[must_use]
pub fn snapshot_network_runtime_native_active_pending_txs_v1(
    chain_id: u64,
    limit: usize,
) -> Vec<NetworkRuntimeNativePendingTxStateV1> {
    snapshot_network_runtime_native_active_pending_txs_for_repair_window_v1(chain_id, limit, None)
}

#[must_use]
pub fn snapshot_network_runtime_native_active_pending_txs_for_repair_window_v1(
    chain_id: u64,
    limit: usize,
    final_missing_sequence_start: Option<u64>,
) -> Vec<NetworkRuntimeNativePendingTxStateV1> {
    if final_missing_sequence_start.is_some() {
        let _ = requeue_network_runtime_native_repair_pending_for_final_missing_v1(
            chain_id,
            final_missing_sequence_start,
        );
    }
    runtime_native_pending_tx_cleanup_v1(chain_id, now_unix_millis());
    let repair_tx_hashes = network_runtime_native_repair_probe_tx_hashes_v1(chain_id);
    let repair_sequence_by_tx_hash =
        network_runtime_native_repair_probe_sequence_by_tx_hash_v1(chain_id);
    let guard = match runtime_native_pending_tx_map().lock() {
        Ok(guard) => guard,
        Err(_) => return Vec::new(),
    };
    let Some(chain_txs) = guard.get(&chain_id) else {
        return Vec::new();
    };
    let mut ordered = chain_txs
        .values()
        .filter(|tx| {
            matches!(
                tx.lifecycle_stage,
                NetworkRuntimeNativePendingTxLifecycleStageV1::Seen
                    | NetworkRuntimeNativePendingTxLifecycleStageV1::Pending
                    | NetworkRuntimeNativePendingTxLifecycleStageV1::Propagated
                    | NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending
            )
        })
        .collect::<Vec<_>>();
    ordered.sort_by(|a, b| {
        let a_repair = repair_tx_hashes.contains(&a.tx_hash);
        let b_repair = repair_tx_hashes.contains(&b.tx_hash);
        let a_sequence = repair_sequence_by_tx_hash.get(&a.tx_hash).copied();
        let b_sequence = repair_sequence_by_tx_hash.get(&b.tx_hash).copied();
        let a_final_missing_repair = a_repair
            && final_missing_sequence_start
                .zip(a_sequence)
                .is_some_and(|(start, sequence)| sequence >= start);
        let b_final_missing_repair = b_repair
            && final_missing_sequence_start
                .zip(b_sequence)
                .is_some_and(|(start, sequence)| sequence >= start);
        b_final_missing_repair
            .cmp(&a_final_missing_repair)
            .then_with(|| b_repair.cmp(&a_repair))
            .then_with(|| match (a_sequence, b_sequence) {
                (Some(a_sequence), Some(b_sequence)) => b_sequence.cmp(&a_sequence),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| b.last_updated_unix_ms.cmp(&a.last_updated_unix_ms))
            .then_with(|| b.tx_hash.cmp(&a.tx_hash))
    });
    if limit > 0 && ordered.len() > limit {
        ordered.truncate(limit);
    }
    ordered.into_iter().cloned().collect()
}

#[must_use]
pub fn snapshot_network_runtime_native_pending_tx_broadcast_candidates_v1(
    chain_id: u64,
    limit: usize,
    max_propagation_count: u64,
) -> Vec<NetworkRuntimeNativePendingTxBroadcastCandidateV1> {
    snapshot_network_runtime_native_pending_tx_broadcast_candidates_inner_v1(
        chain_id,
        limit,
        max_propagation_count,
        true,
    )
}

#[must_use]
pub fn snapshot_network_runtime_native_pending_tx_broadcast_candidates_including_native_v1(
    chain_id: u64,
    limit: usize,
    max_propagation_count: u64,
) -> Vec<NetworkRuntimeNativePendingTxBroadcastCandidateV1> {
    snapshot_network_runtime_native_pending_tx_broadcast_candidates_inner_v1(
        chain_id,
        limit,
        max_propagation_count,
        false,
    )
}

fn snapshot_network_runtime_native_pending_tx_broadcast_candidates_inner_v1(
    chain_id: u64,
    limit: usize,
    max_propagation_count: u64,
    require_eth_envelope: bool,
) -> Vec<NetworkRuntimeNativePendingTxBroadcastCandidateV1> {
    if limit == 0 {
        return Vec::new();
    }
    runtime_native_pending_tx_cleanup_v1(chain_id, now_unix_millis());
    let mut tx_guard = match runtime_native_pending_tx_map().lock() {
        Ok(guard) => guard,
        Err(_) => return Vec::new(),
    };
    let payload_guard = match runtime_native_pending_tx_payload_map().lock() {
        Ok(guard) => guard,
        Err(_) => return Vec::new(),
    };
    let Some(chain_txs) = tx_guard.get_mut(&chain_id) else {
        return Vec::new();
    };
    let Some(chain_payloads) = payload_guard.get(&chain_id) else {
        return Vec::new();
    };
    let now = now_unix_millis();
    let mut out = chain_txs
        .iter_mut()
        .filter_map(|(tx_hash, state)| {
            runtime_native_pending_tx_refresh_retry_window_v1(state, now);
            if !runtime_native_pending_tx_broadcast_eligible_stage_v1(state.lifecycle_stage) {
                return None;
            }
            if !state.retry_eligible {
                return None;
            }
            if state.propagation_count >= max_propagation_count {
                return None;
            }
            let payload = chain_payloads.get(tx_hash)?;
            if payload.is_empty() {
                return None;
            }
            if require_eth_envelope
                && !eth_rlpx_validate_transaction_envelope_payload_v1(payload.as_slice())
            {
                return None;
            }
            Some(NetworkRuntimeNativePendingTxBroadcastCandidateV1 {
                chain_id,
                tx_hash: *tx_hash,
                lifecycle_stage: state.lifecycle_stage,
                propagation_count: state.propagation_count,
                ingress_count: state.ingress_count,
                last_updated_unix_ms: state.last_updated_unix_ms,
                tx_payload_len: payload.len(),
                tx_payload: payload.clone(),
            })
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| {
        runtime_native_pending_tx_broadcast_stage_priority_v1(a.lifecycle_stage)
            .cmp(&runtime_native_pending_tx_broadcast_stage_priority_v1(
                b.lifecycle_stage,
            ))
            .then_with(|| a.propagation_count.cmp(&b.propagation_count))
            .then_with(|| b.last_updated_unix_ms.cmp(&a.last_updated_unix_ms))
            .then_with(|| b.tx_hash.cmp(&a.tx_hash))
    });
    if out.len() <= limit {
        return out;
    }
    out.truncate(limit);
    out
}

#[must_use]
pub fn get_network_runtime_native_pending_tx_v1(
    chain_id: u64,
    tx_hash: [u8; 32],
) -> Option<NetworkRuntimeNativePendingTxStateV1> {
    let guard = runtime_native_pending_tx_map().lock().ok()?;
    guard
        .get(&chain_id)
        .and_then(|chain| chain.get(&tx_hash).cloned())
}

#[must_use]
pub fn get_network_runtime_native_pending_tx_payload_v1(
    chain_id: u64,
    tx_hash: [u8; 32],
) -> Option<Vec<u8>> {
    let guard = runtime_native_pending_tx_payload_map().lock().ok()?;
    guard
        .get(&chain_id)
        .and_then(|chain| chain.get(&tx_hash).cloned())
}

#[must_use]
pub fn snapshot_network_runtime_native_pending_tx_tombstones_v1(
    chain_id: u64,
    limit: usize,
) -> Vec<NetworkRuntimeNativePendingTxTombstoneV1> {
    let guard = match runtime_native_pending_tx_tombstone_map().lock() {
        Ok(guard) => guard,
        Err(_) => return Vec::new(),
    };
    let Some(chain_tombstones) = guard.get(&chain_id) else {
        return Vec::new();
    };
    let mut out = chain_tombstones.values().cloned().collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.last_updated_unix_ms
            .cmp(&a.last_updated_unix_ms)
            .then_with(|| b.tx_hash.cmp(&a.tx_hash))
    });
    if limit == 0 || out.len() <= limit {
        return out;
    }
    out.truncate(limit);
    out
}

#[must_use]
pub fn get_network_runtime_native_pending_tx_tombstone_v1(
    chain_id: u64,
    tx_hash: [u8; 32],
) -> Option<NetworkRuntimeNativePendingTxTombstoneV1> {
    let guard = runtime_native_pending_tx_tombstone_map().lock().ok()?;
    guard
        .get(&chain_id)
        .and_then(|chain| chain.get(&tx_hash).cloned())
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
    let has_observed_peers = observed
        .peer_height_by_chain
        .get(&chain_id)
        .map(|m| !m.is_empty())
        .unwrap_or(false);
    if !has_observed_peers {
        return Vec::new();
    }
    if runtime_sync_recompute_due(chain_id, &statuses, &observed, true) {
        let _ = recompute_runtime_sync_status_from_observed(chain_id, &mut statuses, &mut observed);
    }
    let mut out = observed
        .peer_height_by_chain
        .get(&chain_id)
        .map(|m| m.iter().map(|(peer_id, head)| (*peer_id, *head)).collect())
        .unwrap_or_else(Vec::new);
    if out.len() > 1 {
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    }
    out
}

#[inline]
fn runtime_peer_head_better(lhs: (u64, u64), rhs: (u64, u64)) -> bool {
    lhs.1 > rhs.1 || (lhs.1 == rhs.1 && lhs.0 < rhs.0)
}

#[must_use]
pub fn get_network_runtime_peer_heads_top_k(chain_id: u64, k: usize) -> Vec<(u64, u64)> {
    if k == 0 {
        return Vec::new();
    }
    let mut statuses = match runtime_sync_status_map().lock() {
        Ok(guard) => guard,
        Err(_) => return Vec::new(),
    };
    let mut observed = match runtime_sync_observed_state_map().lock() {
        Ok(guard) => guard,
        Err(_) => return Vec::new(),
    };
    let has_observed_peers = observed
        .peer_height_by_chain
        .get(&chain_id)
        .map(|m| !m.is_empty())
        .unwrap_or(false);
    if !has_observed_peers {
        return Vec::new();
    }
    if runtime_sync_recompute_due(chain_id, &statuses, &observed, true) {
        let _ = recompute_runtime_sync_status_from_observed(chain_id, &mut statuses, &mut observed);
    }
    let Some(peer_map) = observed.peer_height_by_chain.get(&chain_id) else {
        return Vec::new();
    };
    if peer_map.is_empty() {
        return Vec::new();
    }
    if k == 1 {
        if let Some((peer_id, head)) =
            peer_map
                .iter()
                .max_by(|(peer_id_a, head_a), (peer_id_b, head_b)| {
                    head_a.cmp(head_b).then_with(|| peer_id_b.cmp(peer_id_a))
                })
        {
            return vec![(*peer_id, *head)];
        }
        return Vec::new();
    }
    if k >= peer_map.len() {
        let mut out: Vec<(u64, u64)> = peer_map
            .iter()
            .map(|(peer_id, head)| (*peer_id, *head))
            .collect();
        if out.len() > 1 {
            out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        }
        return out;
    }

    let mut top = Vec::<(u64, u64)>::with_capacity(k);
    for (peer_id, head) in peer_map {
        let entry = (*peer_id, *head);
        if top.len() == k {
            let worst = *top.last().unwrap_or(&entry);
            if !runtime_peer_head_better(entry, worst) {
                continue;
            }
        }
        let insert_at = top
            .iter()
            .position(|existing| runtime_peer_head_better(entry, *existing));
        if top.len() < k {
            if let Some(idx) = insert_at {
                top.insert(idx, entry);
            } else {
                top.push(entry);
            }
        } else if let Some(idx) = insert_at {
            top.insert(idx, entry);
            top.truncate(k);
        }
    }
    top
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

    fn build_native_repair_payload_for_test(chain_id: u64, sequence: u64) -> Vec<u8> {
        let tx = novovm_protocol::NovNativeTxWireV1 {
            chain_id,
            kind: NovTxKindV1::Execute(novovm_protocol::NovExecuteTxV1 {
                caller: vec![(sequence % 251) as u8; 20],
                account_id: Some(format!("repair-{sequence}")),
                fee_owner_account_id: Some(format!("repair-{sequence}")),
                nonce_owner_account_id: Some(format!("repair-{sequence}")),
                target: novovm_protocol::NovExecutionTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": sequence.saturating_add(1)
                }))
                .expect("encode args"),
                execution_mode: novovm_protocol::NovExecutionModeV1::Batch,
                execution_policy: novovm_protocol::NovExecutionPolicyV1::Standard,
                privacy_mode: novovm_protocol::NovPrivacyModeV1::Public,
                verification_mode: novovm_protocol::NovVerificationModeV1::Standard,
                fee_policy: novovm_protocol::NovFeePolicyV1 {
                    pay_asset: "USDT".to_string(),
                    max_pay_amount: 50,
                    slippage_bps: 100,
                },
                gas_like_limit: Some(90_000),
                nonce: sequence.saturating_add(1),
            }),
            signature: [0x33; 32],
        };
        novovm_protocol::encode_nov_native_tx_wire_v1(&tx).expect("encode native repair tx")
    }

    fn clear_runtime_sync_status_for_test(chain_id: u64) {
        if let Ok(mut statuses) = runtime_sync_status_map().lock() {
            statuses.remove(&chain_id);
        }
        if let Ok(mut native) = runtime_native_sync_status_map().lock() {
            native.remove(&chain_id);
        }
        if let Ok(mut budgets) = runtime_native_budget_hooks_map().lock() {
            budgets.remove(&chain_id);
        }
        if let Ok(mut headers) = runtime_native_header_snapshot_map().lock() {
            headers.remove(&chain_id);
        }
        if let Ok(mut bodies) = runtime_native_body_snapshot_map().lock() {
            bodies.remove(&chain_id);
        }
        if let Ok(mut heads) = runtime_native_head_snapshot_map().lock() {
            heads.remove(&chain_id);
        }
        if let Ok(mut accounts) = runtime_native_snap_account_snapshot_map().lock() {
            accounts.remove(&chain_id);
        }
        if let Ok(mut storages) = runtime_native_snap_storage_snapshot_map().lock() {
            storages.remove(&chain_id);
        }
        if let Ok(mut codes) = runtime_native_snap_code_snapshot_map().lock() {
            codes.remove(&chain_id);
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
            observed.remote_best_hint_by_chain.remove(&chain_id);
            observed
                .remote_best_hint_updated_at_by_chain
                .remove(&chain_id);
            observed.next_stale_check_at_by_chain.remove(&chain_id);
            observed.dirty_chains.remove(&chain_id);
            observed.sync_anchor_by_chain.remove(&chain_id);
        }
    }

    fn clear_novorudp_sequence_ledger_for_test(chain_id: u64) {
        if let Ok(mut guard) = runtime_novorudp_sequence_ledger_map().lock() {
            guard.remove(&chain_id);
        }
        if let Ok(mut guard) = runtime_native_repair_probe_map().lock() {
            guard.remove(&chain_id);
        }
    }

    #[test]
    fn receipt_write_closes_durable_missing_sequence() {
        let chain_id = 9_998_771;
        clear_novorudp_sequence_ledger_for_test(chain_id);
        ensure_runtime_novorudp_sequence_expected_total_v1(chain_id, 14_400, 64);
        observe_runtime_novorudp_sequence_completion_prefix_v1(chain_id, 1);

        let summary = snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
        assert_eq!(summary.ledger_expected_count, 14_400);
        assert_eq!(summary.ledger_completed_count, 1);
        assert_eq!(summary.ledger_missing_closed_by_receipt_count, 1);
        assert_eq!(summary.ledger_durable_missing_count, 14_399);
        assert_eq!(summary.ledger_durable_missing_ranges_sample[0].start, 1);
    }

    #[test]
    fn canonical_include_closes_durable_missing_sequence() {
        let chain_id = 9_998_772;
        clear_novorudp_sequence_ledger_for_test(chain_id);
        ensure_runtime_novorudp_sequence_expected_total_v1(chain_id, 14_400, 64);
        observe_runtime_novorudp_sequence_completion_prefix_v1(chain_id, 1);

        let summary = snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
        assert_eq!(summary.ledger_completed_count, 1);
        assert_eq!(summary.ledger_missing_closed_by_canonical_count, 1);
        assert_eq!(summary.ledger_durable_missing_count, 14_399);
        assert_eq!(summary.ledger_durable_missing_ranges_sample[0].start, 1);
    }

    #[test]
    fn completion_close_uses_tx_hash_to_sequence_mapping() {
        let chain_id = 9_998_773;
        let sequence = 14_120;
        let tx_hash = [0x7a; 32];
        let payload = build_native_repair_payload_for_test(chain_id, sequence);
        clear_novorudp_sequence_ledger_for_test(chain_id);
        ensure_runtime_novorudp_sequence_expected_total_v1(chain_id, 14_400, 64);
        observe_network_runtime_native_pending_tx_repair_probe_v1(
            chain_id,
            tx_hash,
            1,
            Some(payload.as_slice()),
        );
        observe_network_runtime_native_pending_tx_repair_receipt_canonical_v1(chain_id, tx_hash);

        let summary = snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
        assert_eq!(summary.ledger_completed_count, 1);
        assert_eq!(summary.ledger_durable_missing_count, 14_399);
        let missing_ranges = summary.ledger_durable_missing_ranges_sample;
        assert!(missing_ranges
            .iter()
            .all(|range| sequence < range.start || sequence > range.end_inclusive));
    }

    #[test]
    fn completion_close_reports_missing_mapping() {
        let chain_id = 9_998_774;
        clear_novorudp_sequence_ledger_for_test(chain_id);
        ensure_runtime_novorudp_sequence_expected_total_v1(chain_id, 14_400, 64);
        observe_network_runtime_native_pending_tx_repair_receipt_canonical_v1(chain_id, [0x5a; 32]);

        let summary = snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
        assert_eq!(summary.ledger_completed_count, 0);
        assert_eq!(summary.ledger_durable_missing_count, 14_400);
    }

    #[test]
    fn ack_window_moves_after_receipt_completion() {
        let chain_id = 9_998_775;
        clear_novorudp_sequence_ledger_for_test(chain_id);
        ensure_runtime_novorudp_sequence_expected_total_v1(chain_id, 14_400, 64);
        observe_runtime_novorudp_sequence_completion_prefix_v1(chain_id, 14_104);

        let summary = snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
        assert_eq!(summary.ledger_completed_count, 14_104);
        assert_eq!(summary.ledger_durable_missing_count, 296);
        assert_eq!(
            summary.ledger_durable_missing_ranges_sample[0].start,
            14_104
        );
        assert_ne!(summary.ledger_durable_missing_ranges_sample[0].start, 0);
    }

    #[test]
    fn progress_cannot_exceed_ledger_completed_without_invariant() {
        let chain_id = 9_998_776;
        clear_novorudp_sequence_ledger_for_test(chain_id);
        ensure_runtime_novorudp_sequence_expected_total_v1(chain_id, 14_400, 64);

        let summary_before_completion =
            snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
        assert_eq!(summary_before_completion.ledger_completed_count, 0);
        assert_eq!(
            summary_before_completion.ledger_durable_missing_count,
            14_400
        );

        observe_runtime_novorudp_sequence_completion_prefix_v1(chain_id, 14_104);

        let summary_after_completion =
            snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
        assert_eq!(summary_after_completion.ledger_completed_count, 14_104);
        assert_eq!(summary_after_completion.ledger_durable_missing_count, 296);
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
    fn lagging_peer_observation_does_not_lower_known_highest() {
        let chain_id = 2026_1_u64;
        clear_runtime_sync_status_for_test(chain_id);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 900,
                current_block: 900,
                highest_block: 25_274_604,
            },
        );

        register_network_runtime_peer(chain_id, 10).expect("register lagging peer");
        observe_network_runtime_peer_head(chain_id, 10, 23_935_723).expect("observe lagging peer");
        let after_lagging_peer =
            get_network_runtime_sync_status(chain_id).expect("status after lagging peer");
        assert_eq!(after_lagging_peer.current_block, 900);
        assert_eq!(after_lagging_peer.highest_block, 25_274_604);

        observe_network_runtime_peer_head(chain_id, 11, 25_274_632).expect("observe higher peer");
        let after_higher_peer =
            get_network_runtime_sync_status(chain_id).expect("status after higher peer");
        assert_eq!(after_higher_peer.highest_block, 25_274_632);
    }

    #[test]
    fn unregister_peer_keeps_known_highest_after_remote_best_hint_expiry() {
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
        assert_eq!(status_after_remove.highest_block, 100);

        if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
            observed
                .remote_best_hint_updated_at_by_chain
                .insert(chain_id, 0);
            observed.next_stale_check_at_by_chain.insert(chain_id, 0);
        }
        observe_network_runtime_local_head(chain_id, 10).expect("expire remote best hint");
        let status_after_hint_expiry =
            get_network_runtime_sync_status(chain_id).expect("status after hint expiry");
        assert_eq!(status_after_hint_expiry.peer_count, 0);
        assert_eq!(status_after_hint_expiry.current_block, 10);
        assert_eq!(status_after_hint_expiry.highest_block, 100);
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
            observed.next_stale_check_at_by_chain.insert(chain_id, 0);
        }

        observe_network_runtime_local_head(chain_id, 10).expect("trigger recompute");
        let after = get_network_runtime_sync_status(chain_id).expect("status after prune");
        assert_eq!(after.peer_count, 0);
        assert_eq!(after.current_block, 10);
        assert_eq!(after.highest_block, 100);
    }

    #[test]
    fn same_head_observation_renews_peer_liveness() {
        let chain_id = 2028_1_u64;
        clear_runtime_sync_status_for_test(chain_id);
        observe_network_runtime_local_head(chain_id, 10).expect("observe local");
        register_network_runtime_peer(chain_id, 42).expect("register peer");
        observe_network_runtime_peer_head(chain_id, 42, 100).expect("observe peer");

        if let Ok(mut observed) = runtime_sync_observed_state_map().lock() {
            observed
                .peer_last_seen_millis_by_chain
                .entry(chain_id)
                .or_default()
                .insert(42, 0);
            observed.next_stale_check_at_by_chain.insert(chain_id, 0);
        }

        observe_network_runtime_peer_head(chain_id, 42, 100).expect("renew same head");
        let status = get_network_runtime_sync_status(chain_id).expect("status after renew");
        assert_eq!(status.peer_count, 1);
        assert_eq!(status.current_block, 10);
        assert_eq!(status.highest_block, 100);
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
            observed.next_stale_check_at_by_chain.insert(chain_id, 0);
        }

        let status = get_network_runtime_sync_status(chain_id).expect("status on read");
        assert_eq!(status.peer_count, 0);
        assert_eq!(status.current_block, 10);
        assert_eq!(status.highest_block, 120);
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
    fn native_canonical_chain_tracks_advancing_head() {
        let chain_id = 20335_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 100,
                hash: [0x10; 32],
                parent_hash: [0x09; 32],
                state_root: [0x21; 32],
                transactions_root: [0x31; 32],
                receipts_root: [0x41; 32],
                ommers_hash: [0x51; 32],
                logs_bloom: vec![0u8; 8],
                gas_limit: None,
                gas_used: None,
                timestamp: Some(1_700_000_000),
                base_fee_per_gas: None,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(1),
                observed_unix_ms: 1000,
            },
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 100,
                block_hash: [0x10; 32],
                tx_hashes: vec![[0x61; 32]],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: None,
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 1001,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 2,
                block_number: 100,
                block_hash: [0x10; 32],
                parent_block_hash: [0x09; 32],
                state_root: [0x21; 32],
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: true,
                source_peer_id: Some(1),
                observed_unix_ms: 1002,
            },
        );
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 101,
                hash: [0x11; 32],
                parent_hash: [0x10; 32],
                state_root: [0x22; 32],
                transactions_root: [0x32; 32],
                receipts_root: [0x42; 32],
                ommers_hash: [0x52; 32],
                logs_bloom: vec![0u8; 8],
                gas_limit: None,
                gas_used: None,
                timestamp: Some(1_700_000_012),
                base_fee_per_gas: None,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(1),
                observed_unix_ms: 1100,
            },
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 101,
                block_hash: [0x11; 32],
                tx_hashes: vec![[0x62; 32]],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: None,
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 1101,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 2,
                block_number: 101,
                block_hash: [0x11; 32],
                parent_block_hash: [0x10; 32],
                state_root: [0x22; 32],
                canonical: true,
                safe: true,
                finalized: false,
                reorg_depth_hint: None,
                body_available: true,
                source_peer_id: Some(1),
                observed_unix_ms: 1102,
            },
        );

        let chain =
            snapshot_network_runtime_native_canonical_chain_v1(chain_id).expect("canonical chain");
        assert_eq!(
            chain.lifecycle_stage,
            NetworkRuntimeNativeCanonicalLifecycleStageV1::Advanced
        );
        assert_eq!(chain.canonical_update_count, 2);
        assert_eq!(chain.reorg_count, 0);
        assert_eq!(chain.canonical_block_count, 2);
        assert_eq!(chain.head.as_ref().map(|head| head.number), Some(101));
        assert_eq!(
            chain
                .head
                .as_ref()
                .map(|head| head.lifecycle_stage.as_str()),
            Some("canonical")
        );
        assert_eq!(chain.head.as_ref().map(|head| head.canonical), Some(true));
        assert_eq!(
            chain.head.as_ref().map(|head| head.body_available),
            Some(true)
        );
        assert_eq!(chain.block_lifecycle_summary.canonical_count, 2);
        assert_eq!(chain.block_lifecycle_summary.reorged_out_count, 0);
    }

    #[test]
    fn native_header_and_head_snapshots_do_not_regress_to_lower_height() {
        let chain_id = 20336_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 200,
                hash: [0x20; 32],
                parent_hash: [0x19; 32],
                state_root: [0x21; 32],
                transactions_root: [0x31; 32],
                receipts_root: [0x41; 32],
                ommers_hash: [0x51; 32],
                logs_bloom: Vec::new(),
                gas_limit: None,
                gas_used: None,
                timestamp: Some(1_700_000_200),
                base_fee_per_gas: None,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(1),
                observed_unix_ms: 2_000,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 2,
                block_number: 200,
                block_hash: [0x20; 32],
                parent_block_hash: [0x19; 32],
                state_root: [0x21; 32],
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: false,
                source_peer_id: Some(1),
                observed_unix_ms: 2_001,
            },
        );

        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 160,
                hash: [0x16; 32],
                parent_hash: [0x15; 32],
                state_root: [0x22; 32],
                transactions_root: [0x32; 32],
                receipts_root: [0x42; 32],
                ommers_hash: [0x52; 32],
                logs_bloom: Vec::new(),
                gas_limit: None,
                gas_used: None,
                timestamp: Some(1_700_000_160),
                base_fee_per_gas: None,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(2),
                observed_unix_ms: 3_000,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 2,
                block_number: 160,
                block_hash: [0x16; 32],
                parent_block_hash: [0x15; 32],
                state_root: [0x22; 32],
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: false,
                source_peer_id: Some(2),
                observed_unix_ms: 3_001,
            },
        );

        assert_eq!(
            get_network_runtime_native_header_snapshot_v1(chain_id)
                .expect("latest header")
                .number,
            200
        );
        assert_eq!(
            get_network_runtime_native_head_snapshot_v1(chain_id)
                .expect("latest head")
                .block_number,
            200
        );
        let retained = snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 0);
        assert!(retained
            .iter()
            .any(|block| block.number == 160 && block.hash == [0x16; 32]));
    }

    #[test]
    fn native_canonical_chain_marks_late_body_material_under_current_head() {
        let chain_id = 20338_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 2,
                block_number: 200,
                block_hash: [0x20; 32],
                parent_block_hash: [0x1f; 32],
                state_root: [0xa0; 32],
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: false,
                source_peer_id: Some(3),
                observed_unix_ms: 2_000,
            },
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 196,
                block_hash: [0x96; 32],
                tx_hashes: vec![[0x61; 32]],
                raw_tx_rlps: vec![vec![0xc0]],
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: Some(Vec::new()),
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 2_001,
            },
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 196,
                block_hash: [0x97; 32],
                tx_hashes: vec![[0x62; 32]],
                raw_tx_rlps: vec![vec![0xc1]],
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: Some(Vec::new()),
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 2_002,
            },
        );

        let blocks = snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 0);
        let canonical_gap = blocks
            .iter()
            .find(|block| block.hash == [0x96; 32])
            .expect("late body material");
        assert!(canonical_gap.canonical);
        assert_eq!(
            canonical_gap.lifecycle_stage,
            NetworkRuntimeNativeBlockLifecycleStageV1::Canonical
        );
        let competing_gap = blocks
            .iter()
            .find(|block| block.hash == [0x97; 32])
            .expect("competing body material");
        assert!(!competing_gap.canonical);

        let chain =
            snapshot_network_runtime_native_canonical_chain_v1(chain_id).expect("canonical chain");
        assert_eq!(chain.canonical_block_count, 2);
        assert_eq!(chain.block_lifecycle_summary.canonical_count, 2);
        assert_eq!(chain.block_lifecycle_summary.non_canonical_count, 1);
    }

    #[test]
    fn native_canonical_chain_records_reorg_depth_and_count() {
        let chain_id = 20336_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        for (number, hash, parent_hash, observed_unix_ms) in [
            (100_u64, [0x20; 32], [0x19; 32], 2_000_u128),
            (101_u64, [0x21; 32], [0x20; 32], 2_100_u128),
            (102_u64, [0x22; 32], [0x21; 32], 2_200_u128),
        ] {
            set_network_runtime_native_header_snapshot_v1(
                chain_id,
                NetworkRuntimeNativeHeaderSnapshotV1 {
                    chain_id,
                    number,
                    hash,
                    parent_hash,
                    state_root: [hash[0].saturating_add(1); 32],
                    transactions_root: [0x31; 32],
                    receipts_root: [0x41; 32],
                    ommers_hash: [0x51; 32],
                    logs_bloom: vec![0u8; 8],
                    gas_limit: None,
                    gas_used: None,
                    timestamp: Some(1_700_000_000 + number),
                    base_fee_per_gas: None,
                    withdrawals_root: None,
                    blob_gas_used: None,
                    excess_blob_gas: None,
                    block_access_list_hash: None,
                    source_peer_id: Some(2),
                    observed_unix_ms,
                },
            );
        }
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 2,
                block_number: 102,
                block_hash: [0x22; 32],
                parent_block_hash: [0x21; 32],
                state_root: [0x23; 32],
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: false,
                source_peer_id: Some(2),
                observed_unix_ms: 2201,
            },
        );
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 101,
                hash: [0x31; 32],
                parent_hash: [0x20; 32],
                state_root: [0x32; 32],
                transactions_root: [0x33; 32],
                receipts_root: [0x43; 32],
                ommers_hash: [0x53; 32],
                logs_bloom: vec![0u8; 8],
                gas_limit: None,
                gas_used: None,
                timestamp: Some(1_700_000_500),
                base_fee_per_gas: None,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(3),
                observed_unix_ms: 2300,
            },
        );
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 102,
                hash: [0x32; 32],
                parent_hash: [0x31; 32],
                state_root: [0x33; 32],
                transactions_root: [0x34; 32],
                receipts_root: [0x44; 32],
                ommers_hash: [0x54; 32],
                logs_bloom: vec![0u8; 8],
                gas_limit: None,
                gas_used: None,
                timestamp: Some(1_700_000_501),
                base_fee_per_gas: None,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(3),
                observed_unix_ms: 2301,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 2,
                block_number: 102,
                block_hash: [0x32; 32],
                parent_block_hash: [0x31; 32],
                state_root: [0x33; 32],
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: Some(1),
                body_available: false,
                source_peer_id: Some(3),
                observed_unix_ms: 2302,
            },
        );

        let chain =
            snapshot_network_runtime_native_canonical_chain_v1(chain_id).expect("canonical chain");
        assert_eq!(
            chain.lifecycle_stage,
            NetworkRuntimeNativeCanonicalLifecycleStageV1::Reorg
        );
        assert_eq!(chain.reorg_count, 1);
        assert_eq!(chain.last_reorg_depth, Some(2));
        assert_eq!(chain.head.as_ref().map(|head| head.hash), Some([0x32; 32]));
        assert_eq!(chain.block_lifecycle_summary.canonical_count, 2);
        assert_eq!(chain.block_lifecycle_summary.non_canonical_count, 1);
        assert_eq!(chain.block_lifecycle_summary.reorged_out_count, 1);
    }

    #[test]
    fn native_canonical_chain_tracks_block_lifecycle_states() {
        let chain_id = 20337_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 87,
                block_hash: [0x87; 32],
                tx_hashes: vec![[0x51; 32]],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: None,
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 1_000,
            },
        );
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 87,
                hash: [0x87; 32],
                parent_hash: [0x86; 32],
                state_root: [0xa7; 32],
                transactions_root: [0xb7; 32],
                receipts_root: [0xc7; 32],
                ommers_hash: [0xd7; 32],
                logs_bloom: vec![0u8; 8],
                gas_limit: None,
                gas_used: None,
                timestamp: Some(87),
                base_fee_per_gas: None,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(7),
                observed_unix_ms: 1_001,
            },
        );
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 88,
                hash: [0x88; 32],
                parent_hash: [0x87; 32],
                state_root: [0xa8; 32],
                transactions_root: [0xb8; 32],
                receipts_root: [0xc8; 32],
                ommers_hash: [0xd8; 32],
                logs_bloom: vec![0u8; 8],
                gas_limit: None,
                gas_used: None,
                timestamp: Some(88),
                base_fee_per_gas: None,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(8),
                observed_unix_ms: 1_002,
            },
        );
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 89,
                hash: [0x89; 32],
                parent_hash: [0x88; 32],
                state_root: [0xa9; 32],
                transactions_root: [0xb9; 32],
                receipts_root: [0xc9; 32],
                ommers_hash: [0xd9; 32],
                logs_bloom: vec![0u8; 8],
                gas_limit: None,
                gas_used: None,
                timestamp: Some(89),
                base_fee_per_gas: None,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(9),
                observed_unix_ms: 1_003,
            },
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 89,
                block_hash: [0x89; 32],
                tx_hashes: vec![[0x61; 32]],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: None,
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 1_004,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 2,
                block_number: 89,
                block_hash: [0x89; 32],
                parent_block_hash: [0x88; 32],
                state_root: [0xa9; 32],
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: true,
                source_peer_id: Some(9),
                observed_unix_ms: 1_005,
            },
        );
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 89,
                hash: [0x99; 32],
                parent_hash: [0x88; 32],
                state_root: [0xaa; 32],
                transactions_root: [0xba; 32],
                receipts_root: [0xca; 32],
                ommers_hash: [0xda; 32],
                logs_bloom: vec![0u8; 8],
                gas_limit: None,
                gas_used: None,
                timestamp: Some(90),
                base_fee_per_gas: None,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(10),
                observed_unix_ms: 1_006,
            },
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 89,
                block_hash: [0x99; 32],
                tx_hashes: vec![[0x62; 32]],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: None,
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 1_007,
            },
        );

        let chain =
            snapshot_network_runtime_native_canonical_chain_v1(chain_id).expect("canonical chain");
        assert_eq!(chain.block_lifecycle_summary.seen_count, 0);
        assert_eq!(chain.block_lifecycle_summary.header_only_count, 1);
        assert_eq!(chain.block_lifecycle_summary.body_ready_count, 1);
        assert_eq!(chain.block_lifecycle_summary.canonical_count, 1);
        assert_eq!(chain.block_lifecycle_summary.non_canonical_count, 1);
        assert_eq!(chain.block_lifecycle_summary.reorged_out_count, 0);
        assert_eq!(
            chain
                .head
                .as_ref()
                .map(|head| head.lifecycle_stage.as_str()),
            Some("canonical")
        );
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
            observed
                .remote_best_hint_updated_at_by_chain
                .insert(chain_id, 0);
            observed.next_stale_check_at_by_chain.insert(chain_id, 0);
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
    fn plan_sync_pull_window_falls_back_from_discovery_with_runtime_gap_v1() {
        let chain_id = 20_386_u64;
        clear_runtime_sync_status_for_test(chain_id);
        set_network_runtime_native_sync_status(
            chain_id,
            NetworkRuntimeNativeSyncStatusV1 {
                phase: NetworkRuntimeNativeSyncPhaseV1::Discovery,
                peer_count: 0,
                starting_block: 100,
                current_block: 100,
                highest_block: 100,
                updated_at_unix_millis: now_unix_millis(),
            },
        );
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 2,
                starting_block: 100,
                current_block: 100,
                highest_block: 10_000,
            },
        );

        let window = plan_network_runtime_sync_pull_window(chain_id).expect("headers window");
        assert_eq!(window.phase, NetworkRuntimeNativeSyncPhaseV1::Headers);
        assert_eq!(window.from_block, 101);
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
            101 + default_eth_fullnode_budget_hooks_v1().sync_pull_headers_batch - 1
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
    fn plan_sync_pull_window_uses_runtime_budget_finalize_batch_v1() {
        let chain_id = 20_395_u64;
        clear_runtime_sync_status_for_test(chain_id);
        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.sync_pull_finalize_batch = 64;
        set_network_runtime_native_budget_hooks_v1(chain_id, budget);

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 2,
                starting_block: 100,
                current_block: 29_900,
                highest_block: 30_000,
            },
        );

        let window = plan_network_runtime_sync_pull_window(chain_id).expect("finalize window");
        assert_eq!(window.phase, NetworkRuntimeNativeSyncPhaseV1::Finalize);
        assert_eq!(window.from_block, 29_901);
        assert_eq!(window.to_block, 29_964);
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

    #[test]
    fn runtime_peer_heads_top_k_returns_expected_order() {
        let chain_id = 2041_u64;
        clear_runtime_sync_status_for_test(chain_id);

        register_network_runtime_peer(chain_id, 201).expect("register peer 201");
        register_network_runtime_peer(chain_id, 202).expect("register peer 202");
        register_network_runtime_peer(chain_id, 203).expect("register peer 203");
        register_network_runtime_peer(chain_id, 204).expect("register peer 204");

        observe_network_runtime_peer_head(chain_id, 201, 300).expect("observe 201");
        observe_network_runtime_peer_head(chain_id, 202, 500).expect("observe 202");
        observe_network_runtime_peer_head(chain_id, 203, 500).expect("observe 203");
        observe_network_runtime_peer_head(chain_id, 204, 100).expect("observe 204");

        let top_2 = get_network_runtime_peer_heads_top_k(chain_id, 2);
        assert_eq!(top_2.len(), 2);
        assert_eq!(top_2[0], (202, 500));
        assert_eq!(top_2[1], (203, 500));

        let top_all = get_network_runtime_peer_heads_top_k(chain_id, usize::MAX);
        assert_eq!(top_all.first().copied(), Some((202, 500)));
        assert_eq!(top_all.get(1).copied(), Some((203, 500)));
        assert_eq!(top_all.last().copied(), Some((204, 100)));
    }

    #[test]
    fn native_header_body_head_snapshots_roundtrip_and_head_advances_local_progress() {
        let chain_id = 2042_u64;
        clear_runtime_sync_status_for_test(chain_id);

        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id: 0,
                number: 88,
                hash: [0x11; 32],
                parent_hash: [0x10; 32],
                state_root: [0x21; 32],
                transactions_root: [0x22; 32],
                receipts_root: [0x23; 32],
                ommers_hash: [0x24; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(21_000),
                timestamp: Some(7),
                base_fee_per_gas: Some(9),
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(7),
                observed_unix_ms: 10,
            },
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeBodySnapshotV1 {
                chain_id: 0,
                number: 88,
                block_hash: [0x11; 32],
                tx_hashes: vec![[0x31; 32], [0x32; 32]],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: vec![[0x41; 32]],
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 11,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id: 0,
                phase: NetworkRuntimeNativeSyncPhaseV1::Headers,
                peer_count: 3,
                block_number: 88,
                block_hash: [0x11; 32],
                parent_block_hash: [0x10; 32],
                state_root: [0x21; 32],
                canonical: false,
                safe: true,
                finalized: false,
                reorg_depth_hint: Some(1),
                body_available: true,
                source_peer_id: Some(7),
                observed_unix_ms: 12,
            },
        );

        let header = get_network_runtime_native_header_snapshot_v1(chain_id).expect("header");
        let body = get_network_runtime_native_body_snapshot_v1(chain_id).expect("body");
        let head = get_network_runtime_native_head_snapshot_v1(chain_id).expect("head");
        let runtime = get_network_runtime_sync_status(chain_id).expect("runtime");

        assert_eq!(header.chain_id, chain_id);
        assert_eq!(header.number, 88);
        assert_eq!(body.chain_id, chain_id);
        assert_eq!(body.number, 88);
        assert_eq!(head.chain_id, chain_id);
        assert!(head.canonical);
        assert!(head.safe);
        assert_eq!(runtime.current_block, 88);
        assert_eq!(runtime.highest_block, 88);
    }

    #[test]
    fn native_header_source_peer_snapshot_tracks_same_block_observations() {
        let chain_id = 2042_100_u64;
        clear_network_runtime_native_state_for_host_tests_v1();

        for (source_peer_id, observed_unix_ms) in [(7_u64, 10_u128), (3_u64, 11_u128), (7, 12)] {
            set_network_runtime_native_header_snapshot_v1(
                chain_id,
                NetworkRuntimeNativeHeaderSnapshotV1 {
                    chain_id,
                    number: 88,
                    hash: [0x11; 32],
                    parent_hash: [0x10; 32],
                    state_root: [0x21; 32],
                    transactions_root: [0x22; 32],
                    receipts_root: [0x23; 32],
                    ommers_hash: [0x24; 32],
                    logs_bloom: vec![0u8; 256],
                    gas_limit: None,
                    gas_used: None,
                    timestamp: Some(7),
                    base_fee_per_gas: None,
                    withdrawals_root: None,
                    blob_gas_used: None,
                    excess_blob_gas: None,
                    block_access_list_hash: None,
                    source_peer_id: Some(source_peer_id),
                    observed_unix_ms,
                },
            );
        }

        assert_eq!(
            snapshot_network_runtime_native_header_source_peers_v1(chain_id, [0x11; 32]),
            vec![3, 7]
        );
        assert!(
            snapshot_network_runtime_native_header_source_peers_v1(chain_id, [0x12; 32]).is_empty()
        );

        clear_network_runtime_native_state_for_host_tests_v1();
        assert!(
            snapshot_network_runtime_native_header_source_peers_v1(chain_id, [0x11; 32]).is_empty()
        );
    }

    #[test]
    fn native_snap_snapshot_lists_filter_by_state_root_and_limit() {
        let chain_id = 2043_u64;
        clear_runtime_sync_status_for_test(chain_id);
        let state_root = [0x21; 32];
        let other_state_root = [0x22; 32];
        let account_a = [0x01; 32];
        let account_b = [0x02; 32];
        let code_hash = [0x03; 32];

        set_network_runtime_native_snap_account_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapAccountSnapshotV1 {
                chain_id,
                state_root,
                account_hash: account_b,
                body_rlp: vec![0xb2],
                proof_nodes: Vec::new(),
                storage_root: None,
                code_hash: Some(code_hash),
                has_storage: false,
                has_code: true,
                source_peer_id: Some(2),
                observed_unix_ms: 2,
            },
        );
        set_network_runtime_native_snap_account_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapAccountSnapshotV1 {
                chain_id,
                state_root,
                account_hash: account_a,
                body_rlp: vec![0xa1],
                proof_nodes: Vec::new(),
                storage_root: None,
                code_hash: None,
                has_storage: false,
                has_code: false,
                source_peer_id: Some(1),
                observed_unix_ms: 1,
            },
        );
        set_network_runtime_native_snap_account_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapAccountSnapshotV1 {
                chain_id,
                state_root: other_state_root,
                account_hash: [0xff; 32],
                body_rlp: vec![0xff],
                proof_nodes: Vec::new(),
                storage_root: None,
                code_hash: None,
                has_storage: false,
                has_code: false,
                source_peer_id: None,
                observed_unix_ms: 3,
            },
        );
        set_network_runtime_native_snap_account_storage_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapAccountStorageSnapshotV1 {
                chain_id,
                state_root,
                account_hash: account_b,
                slots: vec![NetworkRuntimeNativeSnapStorageSlotSnapshotV1 {
                    hash: [0x04; 32],
                    body: vec![0x05],
                }],
                proof_nodes: Vec::new(),
                source_peer_id: Some(2),
                observed_unix_ms: 4,
            },
        );
        set_network_runtime_native_snap_code_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapCodeSnapshotV1 {
                chain_id,
                code_hash,
                code: vec![0x60, 0x00],
                source_peer_id: Some(2),
                observed_unix_ms: 5,
            },
        );

        let accounts =
            snapshot_network_runtime_native_snap_account_snapshots_v1(chain_id, state_root, 1);
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].account_hash, account_a);
        let storages = snapshot_network_runtime_native_snap_account_storage_snapshots_v1(
            chain_id, state_root, 8,
        );
        assert_eq!(storages.len(), 1);
        assert_eq!(storages[0].account_hash, account_b);
        let codes =
            snapshot_network_runtime_native_snap_code_snapshots_v1(chain_id, &[code_hash], 8);
        assert_eq!(codes.len(), 1);
        assert_eq!(codes[0].code, vec![0x60, 0x00]);
    }

    #[test]
    fn native_pending_tx_lifecycle_tracks_canonical_and_reorg_transitions() {
        let chain_id = 2055_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let tx_hash = [0x91; 32];
        let canonical_hash = [0x11; 32];
        let reorg_hash = [0x22; 32];

        observe_network_runtime_native_pending_tx_ingress_v1(chain_id, 7, tx_hash);
        let pending = get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).expect("pending");
        assert_eq!(
            pending.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Pending
        );

        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 88,
                block_hash: canonical_hash,
                tx_hashes: vec![tx_hash],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 10,
            },
        );
        let included_non_canonical =
            get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).expect("pending");
        assert_eq!(
            included_non_canonical.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedNonCanonical
        );

        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 2,
                block_number: 88,
                block_hash: canonical_hash,
                parent_block_hash: [0x10; 32],
                state_root: [0x21; 32],
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: true,
                source_peer_id: Some(7),
                observed_unix_ms: 11,
            },
        );
        let canonical =
            get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).expect("pending");
        assert_eq!(
            canonical.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
        );

        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 2,
                block_number: 88,
                block_hash: reorg_hash,
                parent_block_hash: [0x10; 32],
                state_root: [0x31; 32],
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: Some(1),
                body_available: false,
                source_peer_id: Some(8),
                observed_unix_ms: 12,
            },
        );
        let reorged = get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).expect("pending");
        assert_eq!(
            reorged.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending
        );
        assert_eq!(reorged.reorg_back_count, 1);

        let summary = snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
        assert_eq!(summary.tx_count, 1);
        assert_eq!(summary.reorged_back_to_pending_count, 1);
    }

    #[test]
    fn included_pending_tx_cleanup_respects_canonical_depth_only() {
        let chain_id = 2056_u64;
        let budget = get_network_runtime_native_budget_hooks_v1(chain_id);
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let tx_hash = [0x92; 32];
        let canonical_hash = [0x12; 32];
        let included_block_number = 100_u64;

        observe_network_runtime_native_pending_tx_ingress_v1(chain_id, 7, tx_hash);
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: included_block_number,
                block_hash: canonical_hash,
                tx_hashes: vec![tx_hash],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 10,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 2,
                block_number: included_block_number,
                block_hash: canonical_hash,
                parent_block_hash: [0x11; 32],
                state_root: [0x22; 32],
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: true,
                source_peer_id: Some(7),
                observed_unix_ms: 11,
            },
        );

        if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
            let chain_txs = guard.get_mut(&chain_id).expect("pending map");
            let tx = chain_txs.get_mut(&tx_hash).expect("pending tx");
            assert_eq!(
                tx.lifecycle_stage,
                NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
            );
            tx.propagation_attempt_count = budget.pending_tx_no_success_attempt_limit + 5;
            tx.propagation_success_count = 0;
            tx.first_seen_unix_ms = 0;
            tx.last_updated_unix_ms = 0;
        }

        let cleanup_now = budget.pending_tx_ttl_ms as u128 + 1_000;
        runtime_native_pending_tx_cleanup_v1(chain_id, cleanup_now);
        let retained =
            get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).expect("retained tx");
        assert_eq!(
            retained.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
        );
        assert!(get_network_runtime_native_pending_tx_tombstone_v1(chain_id, tx_hash).is_none());

        if let Ok(mut guard) = runtime_native_canonical_chain_map().lock() {
            let chain = guard.get_mut(&chain_id).expect("canonical chain");
            let head = chain.snapshot.head.as_mut().expect("canonical head");
            head.number = included_block_number + budget.pending_tx_canonical_retain_depth + 1;
        }

        runtime_native_pending_tx_cleanup_v1(chain_id, cleanup_now + 1);
        assert!(get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).is_none());
        let tombstone =
            get_network_runtime_native_pending_tx_tombstone_v1(chain_id, tx_hash).expect("tomb");
        assert_eq!(
            tombstone.final_disposition,
            NetworkRuntimeNativePendingTxFinalDispositionV1::Evicted
        );
        assert_eq!(
            tombstone.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
        );
    }

    #[test]
    fn included_non_canonical_pending_tx_cleanup_skips_generic_eviction_paths() {
        let chain_id = 2057_u64;
        let budget = get_network_runtime_native_budget_hooks_v1(chain_id);
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let tx_hash = [0x93; 32];

        observe_network_runtime_native_pending_tx_ingress_v1(chain_id, 8, tx_hash);
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 101,
                block_hash: [0x13; 32],
                tx_hashes: vec![tx_hash],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 20,
            },
        );

        if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
            let chain_txs = guard.get_mut(&chain_id).expect("pending map");
            let tx = chain_txs.get_mut(&tx_hash).expect("pending tx");
            assert_eq!(
                tx.lifecycle_stage,
                NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedNonCanonical
            );
            tx.propagation_attempt_count = budget.pending_tx_no_success_attempt_limit + 5;
            tx.propagation_success_count = 0;
            tx.first_seen_unix_ms = 0;
            tx.last_updated_unix_ms = 0;
        }

        runtime_native_pending_tx_cleanup_v1(chain_id, budget.pending_tx_ttl_ms as u128 + 1_000);
        let retained =
            get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).expect("retained tx");
        assert_eq!(
            retained.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedNonCanonical
        );
        assert!(get_network_runtime_native_pending_tx_tombstone_v1(chain_id, tx_hash).is_none());
    }

    #[test]
    fn repair_observed_active_pending_cleanup_uses_refresh_window() {
        let chain_id = 20570_u64;
        let budget = get_network_runtime_native_budget_hooks_v1(chain_id);
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let repair_tx = [0x97; 32];
        let normal_tx = [0x98; 32];
        let repair_last_updated = budget.pending_tx_ttl_ms as u128;
        let cleanup_now = repair_last_updated + 1_000;

        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            8,
            repair_tx,
            Some(b"repair-native-payload"),
        );
        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            8,
            normal_tx,
            Some(b"normal-native-payload"),
        );
        {
            let mut guard = runtime_native_repair_probe_map()
                .lock()
                .expect("repair probe lock");
            let state = guard.entry(chain_id).or_default();
            state.tx_hash_seen.insert(repair_tx);
            state.tx_hash_to_sequence.insert(repair_tx, 14_399);
            state.sequence_seen.insert(14_399);
        }
        if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
            let chain_txs = guard.get_mut(&chain_id).expect("pending map");
            let repair = chain_txs.get_mut(&repair_tx).expect("repair pending");
            repair.first_seen_unix_ms = 0;
            repair.last_updated_unix_ms = repair_last_updated;
            repair.propagation_attempt_count = budget.pending_tx_no_success_attempt_limit + 5;
            repair.propagation_success_count = 0;

            let normal = chain_txs.get_mut(&normal_tx).expect("normal pending");
            normal.first_seen_unix_ms = 0;
            normal.last_updated_unix_ms = repair_last_updated;
            normal.propagation_attempt_count = budget.pending_tx_no_success_attempt_limit + 5;
            normal.propagation_success_count = 0;
        }

        runtime_native_pending_tx_cleanup_v1(chain_id, cleanup_now);

        let retained =
            get_network_runtime_native_pending_tx_v1(chain_id, repair_tx).expect("repair retained");
        assert_eq!(
            retained.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Pending
        );
        assert!(
            get_network_runtime_native_pending_tx_payload_v1(chain_id, repair_tx).is_some(),
            "fresh repair pending payload must remain available for AOEM admission"
        );
        assert!(
            get_network_runtime_native_pending_tx_tombstone_v1(chain_id, repair_tx).is_none(),
            "fresh repair pending must not be tombstoned by original first_seen"
        );

        assert!(
            get_network_runtime_native_pending_tx_v1(chain_id, normal_tx).is_none(),
            "normal pending must still obey existing cleanup semantics"
        );
        let tombstone = get_network_runtime_native_pending_tx_tombstone_v1(chain_id, normal_tx)
            .expect("normal tombstone");
        assert_eq!(
            tombstone.final_disposition,
            NetworkRuntimeNativePendingTxFinalDispositionV1::Evicted
        );
    }

    #[test]
    fn pending_tx_broadcast_candidates_respect_stage_and_propagation_budget() {
        let chain_id = 2058_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let tx_pending = [0xa1; 32];
        let tx_over_propagated = [0xa2; 32];
        let tx_canonical = [0xa3; 32];

        observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
            chain_id,
            7,
            tx_pending,
            Some(&[0xf8, 0x01, 0x01]),
        );
        observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
            chain_id,
            8,
            tx_over_propagated,
            Some(&[0xf8, 0x02, 0x02]),
        );
        observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
            chain_id,
            9,
            tx_canonical,
            Some(&[0xf8, 0x03, 0x03]),
        );

        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 90,
                block_hash: [0xb1; 32],
                tx_hashes: vec![tx_canonical],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 100,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 2,
                block_number: 90,
                block_hash: [0xb1; 32],
                parent_block_hash: [0xb0; 32],
                state_root: [0xc1; 32],
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: true,
                source_peer_id: Some(7),
                observed_unix_ms: 101,
            },
        );
        observe_network_runtime_native_pending_tx_propagated_v1(chain_id, tx_pending);
        observe_network_runtime_native_pending_tx_propagated_v1(chain_id, tx_over_propagated);
        observe_network_runtime_native_pending_tx_propagated_v1(chain_id, tx_over_propagated);
        observe_network_runtime_native_pending_tx_propagated_v1(chain_id, tx_over_propagated);

        let candidates =
            snapshot_network_runtime_native_pending_tx_broadcast_candidates_v1(chain_id, 10, 3);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].tx_hash, tx_pending);
        assert_eq!(
            candidates[0].lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Propagated
        );
    }

    #[test]
    fn native_pending_payload_broadcast_candidates_do_not_pollute_eth_only_candidates() {
        let chain_id = 2067_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let tx_native = [0x67; 32];
        observe_network_runtime_native_pending_tx_local_native_payload_v1(
            chain_id,
            tx_native,
            Some(b"NNX1-native-payload"),
        );

        let eth_only =
            snapshot_network_runtime_native_pending_tx_broadcast_candidates_v1(chain_id, 10, 3);
        assert!(
            eth_only.is_empty(),
            "native NOV payload must not enter Ethereum RLPx tx broadcast candidates"
        );

        let native_candidates =
            snapshot_network_runtime_native_pending_tx_broadcast_candidates_including_native_v1(
                chain_id, 10, 3,
            );
        assert_eq!(native_candidates.len(), 1);
        assert_eq!(native_candidates[0].tx_hash, tx_native);
        assert_eq!(
            native_candidates[0].lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Pending
        );
        assert_eq!(native_candidates[0].tx_payload, b"NNX1-native-payload");
    }

    #[test]
    fn reorged_back_to_pending_tx_reenters_broadcast_candidates_with_priority() {
        let chain_id = 2065_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let tx_reorg = [0xf1; 32];
        let tx_pending = [0xf2; 32];
        let canonical_hash = [0xb2; 32];
        let reorg_hash = [0xb3; 32];

        observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
            chain_id,
            31,
            tx_reorg,
            Some(&[0xf8, 0x01, 0x01]),
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 120,
                block_hash: canonical_hash,
                tx_hashes: vec![tx_reorg],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 10,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 2,
                block_number: 120,
                block_hash: canonical_hash,
                parent_block_hash: [0xb1; 32],
                state_root: [0xc1; 32],
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: true,
                source_peer_id: Some(31),
                observed_unix_ms: 11,
            },
        );

        let included =
            get_network_runtime_native_pending_tx_v1(chain_id, tx_reorg).expect("included tx");
        assert_eq!(
            included.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
        );
        let candidates_before_reorg =
            snapshot_network_runtime_native_pending_tx_broadcast_candidates_v1(chain_id, 16, 3);
        assert!(
            candidates_before_reorg
                .iter()
                .all(|candidate| candidate.tx_hash != tx_reorg),
            "included canonical tx must not be selected for broadcast candidates"
        );

        observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
            chain_id,
            32,
            tx_pending,
            Some(&[0xf8, 0x01, 0x01]),
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 2,
                block_number: 120,
                block_hash: reorg_hash,
                parent_block_hash: [0xb1; 32],
                state_root: [0xc2; 32],
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: Some(1),
                body_available: false,
                source_peer_id: Some(32),
                observed_unix_ms: 12,
            },
        );

        let reorged =
            get_network_runtime_native_pending_tx_v1(chain_id, tx_reorg).expect("reorged tx");
        assert_eq!(
            reorged.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending
        );
        assert_eq!(reorged.reorg_back_count, 1);

        let candidates_after_reorg =
            snapshot_network_runtime_native_pending_tx_broadcast_candidates_v1(chain_id, 16, 3);
        assert_eq!(candidates_after_reorg.len(), 2);
        assert_eq!(candidates_after_reorg[0].tx_hash, tx_reorg);
        assert_eq!(
            candidates_after_reorg[0].lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending
        );
        assert_eq!(candidates_after_reorg[1].tx_hash, tx_pending);
        assert_eq!(
            candidates_after_reorg[1].lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Pending
        );
    }

    #[test]
    fn pending_tx_broadcast_runtime_summary_tracks_dispatch_outcomes() {
        let chain_id = 2059_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        observe_network_runtime_native_pending_tx_broadcast_dispatch_v1(chain_id, 11, 2, 2, true);
        observe_network_runtime_native_pending_tx_broadcast_dispatch_v1(chain_id, 12, 3, 0, false);

        let runtime =
            snapshot_network_runtime_native_pending_tx_broadcast_runtime_summary_v1(chain_id);
        assert_eq!(runtime.chain_id, chain_id);
        assert_eq!(runtime.dispatch_total, 2);
        assert_eq!(runtime.dispatch_success_total, 1);
        assert_eq!(runtime.dispatch_failed_total, 1);
        assert_eq!(runtime.candidate_tx_total, 5);
        assert_eq!(runtime.broadcast_tx_total, 2);
        assert_eq!(runtime.last_peer_id, Some(12));
        assert_eq!(runtime.last_candidate_count, 3);
        assert_eq!(runtime.last_broadcast_tx_count, 0);
        assert!(runtime.last_updated_unix_ms.is_some());

        let summary = snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
        assert_eq!(summary.broadcast_dispatch_total, 2);
        assert_eq!(summary.broadcast_dispatch_success_total, 1);
        assert_eq!(summary.broadcast_dispatch_failed_total, 1);
        assert_eq!(summary.broadcast_candidate_tx_total, 5);
        assert_eq!(summary.broadcast_tx_total, 2);
        assert_eq!(summary.last_broadcast_peer_id, Some(12));
        assert_eq!(summary.last_broadcast_candidate_count, 3);
        assert_eq!(summary.last_broadcast_tx_count, 0);
        assert!(summary.last_broadcast_unix_ms.is_some());
    }

    #[test]
    fn execution_budget_runtime_summary_tracks_throttle_events() {
        let chain_id = 2060_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        observe_network_runtime_native_execution_budget_target_v1(
            chain_id,
            &NetworkRuntimeNativeExecutionBudgetTargetObservationV1 {
                hard_budget_per_tick: 64,
                hard_time_slice_ms: 10,
                target_budget_per_tick: 48,
                target_time_slice_ms: 8,
                effective_budget_per_tick: 40,
                effective_time_slice_ms: 7,
                reason: Some("sync_pressure_high+recent_execution_throttle".to_string()),
            },
        );
        observe_network_runtime_native_execution_budget_throttle_v1(
            chain_id,
            "host_exec_budget_per_tick_exhausted",
            3,
            true,
            false,
        );
        observe_network_runtime_native_execution_budget_throttle_v1(
            chain_id,
            "host_exec_time_slice_exceeded",
            5,
            true,
            true,
        );

        let summary = snapshot_network_runtime_native_execution_budget_runtime_summary_v1(chain_id);
        assert_eq!(summary.chain_id, chain_id);
        assert_eq!(summary.execution_budget_hit_count, 2);
        assert_eq!(summary.execution_deferred_count, 8);
        assert_eq!(summary.execution_time_slice_exceeded_count, 1);
        assert_eq!(summary.hard_budget_per_tick, Some(64));
        assert_eq!(summary.hard_time_slice_ms, Some(10));
        assert_eq!(summary.target_budget_per_tick, Some(48));
        assert_eq!(summary.target_time_slice_ms, Some(8));
        assert_eq!(summary.effective_budget_per_tick, Some(40));
        assert_eq!(summary.effective_time_slice_ms, Some(7));
        assert_eq!(
            summary.last_execution_target_reason.as_deref(),
            Some("sync_pressure_high+recent_execution_throttle")
        );
        assert_eq!(
            summary.last_execution_throttle_reason.as_deref(),
            Some("host_exec_time_slice_exceeded")
        );
        assert!(summary.last_updated_unix_ms.is_some());
    }

    #[test]
    fn pending_tx_invalid_payload_is_marked_rejected_and_not_broadcast_candidate() {
        let chain_id = 2061_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let tx_hash = [0xc1; 32];

        observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
            chain_id,
            9,
            tx_hash,
            Some(&[0x01, 0x02, 0x03]),
        );

        let state =
            get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).expect("pending tx state");
        assert_eq!(
            state.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Rejected
        );
        assert_eq!(state.reject_count, 1);
        assert_eq!(
            state.propagation_disposition,
            Some(NetworkRuntimeNativePendingTxPropagationDispositionV1::Rejected)
        );
        assert_eq!(
            state.propagation_stop_reason,
            Some(NetworkRuntimeNativePendingTxPropagationStopReasonV1::InvalidEnvelope)
        );
        assert_eq!(
            state.propagation_recoverability,
            Some(NetworkRuntimeNativePendingTxPropagationRecoverabilityV1::NonRecoverable)
        );

        let summary = snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
        assert_eq!(summary.tx_count, 1);
        assert_eq!(summary.rejected_count, 1);

        let candidates =
            snapshot_network_runtime_native_pending_tx_broadcast_candidates_v1(chain_id, 16, 3);
        assert!(candidates.is_empty());
    }

    #[test]
    fn pending_tx_observe_dropped_and_rejected_stage_transitions() {
        let chain_id = 2062_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let dropped_tx = [0xd1; 32];
        let rejected_tx = [0xd2; 32];

        observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
            chain_id,
            11,
            dropped_tx,
            Some(&[0xf8, 0xd1]),
        );
        observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
            chain_id,
            12,
            rejected_tx,
            Some(&[0xf8, 0xd2]),
        );

        observe_network_runtime_native_pending_tx_dropped_v1(chain_id, dropped_tx);
        observe_network_runtime_native_pending_tx_rejected_v1(chain_id, rejected_tx, Some(12));

        let dropped_state =
            get_network_runtime_native_pending_tx_v1(chain_id, dropped_tx).expect("dropped state");
        assert_eq!(
            dropped_state.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Dropped
        );
        assert_eq!(dropped_state.drop_count, 1);

        let rejected_state = get_network_runtime_native_pending_tx_v1(chain_id, rejected_tx)
            .expect("rejected state");
        assert_eq!(
            rejected_state.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Rejected
        );
        assert!(rejected_state.reject_count >= 1);

        let summary = snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
        assert_eq!(summary.tx_count, 2);
        assert_eq!(summary.dropped_count, 1);
        assert_eq!(summary.rejected_count, 1);
        assert!(
            get_network_runtime_native_pending_tx_payload_v1(chain_id, dropped_tx).is_none(),
            "dropped pending tx payload must not be retained"
        );
        assert!(
            get_network_runtime_native_pending_tx_payload_v1(chain_id, rejected_tx).is_none(),
            "rejected pending tx payload must not be retained"
        );
    }

    #[test]
    fn active_pending_snapshot_excludes_historical_and_reentry_payloads() {
        let chain_id = 20620_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let active_tx = [0xa1; 32];
        let dropped_tx = [0xa2; 32];
        let included_tx = [0xa3; 32];
        let canonical_hash = [0xb3; 32];

        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            31,
            active_tx,
            Some(b"active-native-payload"),
        );
        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            31,
            dropped_tx,
            Some(b"dropped-native-payload"),
        );
        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            31,
            included_tx,
            Some(b"included-native-payload"),
        );
        observe_network_runtime_native_pending_tx_dropped_v1(chain_id, dropped_tx);

        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 7,
                block_hash: canonical_hash,
                tx_hashes: vec![included_tx],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 10,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 1,
                block_number: 7,
                block_hash: canonical_hash,
                parent_block_hash: [0x11; 32],
                state_root: [0x22; 32],
                canonical: true,
                safe: true,
                finalized: true,
                reorg_depth_hint: Some(0),
                body_available: true,
                source_peer_id: Some(31),
                observed_unix_ms: 11,
            },
        );

        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            31,
            included_tx,
            Some(b"tail-repair-duplicate-after-included"),
        );

        let active = snapshot_network_runtime_native_active_pending_txs_v1(chain_id, 16);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].tx_hash, active_tx);
        assert!(
            get_network_runtime_native_pending_tx_payload_v1(chain_id, dropped_tx).is_none(),
            "dropped payload must not remain in active retention"
        );
        assert!(
            get_network_runtime_native_pending_tx_payload_v1(chain_id, included_tx).is_none(),
            "duplicate reentry after canonical inclusion must not retain payload"
        );
    }

    #[test]
    fn active_pending_snapshot_prioritizes_repair_observed_pending() {
        let chain_id = 20622_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let normal_tx = [0xb1; 32];
        let repair_tx = [0xb2; 32];

        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            31,
            repair_tx,
            Some(b"repair-native-payload"),
        );
        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            31,
            normal_tx,
            Some(b"normal-native-payload"),
        );
        {
            let mut guard = runtime_native_repair_probe_map()
                .lock()
                .expect("repair probe lock");
            let state = guard.entry(chain_id).or_default();
            state.tx_hash_seen.insert(repair_tx);
            state.tx_hash_to_sequence.insert(repair_tx, 1);
            state.sequence_seen.insert(1);
        }

        let active = snapshot_network_runtime_native_active_pending_txs_v1(chain_id, 1);
        assert_eq!(active.len(), 1);
        assert_eq!(
            active[0].tx_hash, repair_tx,
            "repair-observed pending must stay inside the AOEM scan window"
        );
    }

    #[test]
    fn active_pending_snapshot_orders_repair_pending_by_descending_sequence() {
        let chain_id = 20624_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let high_repair_tx = [0xd1; 32];
        let low_repair_tx = [0xd2; 32];
        let normal_tx = [0xd3; 32];

        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            31,
            high_repair_tx,
            Some(b"high-repair-native-payload"),
        );
        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            31,
            normal_tx,
            Some(b"normal-native-payload"),
        );
        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            31,
            low_repair_tx,
            Some(b"low-repair-native-payload"),
        );
        {
            let mut guard = runtime_native_repair_probe_map()
                .lock()
                .expect("repair probe lock");
            let state = guard.entry(chain_id).or_default();
            state.tx_hash_seen.insert(high_repair_tx);
            state.tx_hash_seen.insert(low_repair_tx);
            state.tx_hash_to_sequence.insert(high_repair_tx, 14399);
            state.tx_hash_to_sequence.insert(low_repair_tx, 14104);
            state.sequence_seen.insert(14399);
            state.sequence_seen.insert(14104);
        }

        let active = snapshot_network_runtime_native_active_pending_txs_v1(chain_id, 1);
        assert_eq!(active.len(), 1);
        assert_eq!(
            active[0].tx_hash, high_repair_tx,
            "highest repair sequence must enter the AOEM scan window first for tail repair"
        );
    }

    #[test]
    fn active_pending_snapshot_prioritizes_final_missing_repair_window() {
        let chain_id = 20625_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let normal_tx = [0xe1; 32];
        let old_repair_tx = [0xe2; 32];
        let final_missing_repair_tx = [0xe3; 32];

        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            31,
            normal_tx,
            Some(b"normal-native-payload"),
        );
        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            31,
            old_repair_tx,
            Some(b"old-repair-native-payload"),
        );
        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            31,
            final_missing_repair_tx,
            Some(b"final-missing-repair-native-payload"),
        );
        {
            let mut guard = runtime_native_repair_probe_map()
                .lock()
                .expect("repair probe lock");
            let state = guard.entry(chain_id).or_default();
            state.tx_hash_seen.insert(old_repair_tx);
            state.tx_hash_seen.insert(final_missing_repair_tx);
            state.tx_hash_to_sequence.insert(old_repair_tx, 11_999);
            state
                .tx_hash_to_sequence
                .insert(final_missing_repair_tx, 13_600);
            state.sequence_seen.insert(11_999);
            state.sequence_seen.insert(13_600);
        }

        let active = snapshot_network_runtime_native_active_pending_txs_for_repair_window_v1(
            chain_id,
            1,
            Some(13_600),
        );
        assert_eq!(active.len(), 1);
        assert_eq!(
            active[0].tx_hash, final_missing_repair_tx,
            "final-missing repair pending must not be truncated by normal or old repair pending"
        );
    }

    #[test]
    fn repair_attempted_unreceipted_final_missing_is_requeued() {
        let chain_id = 20626_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let repair_tx = [0xe4; 32];
        let sequence = 14_168_u64;

        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            41,
            repair_tx,
            Some(b"repair-final-missing-payload"),
        );
        {
            let mut guard = runtime_native_repair_probe_map()
                .lock()
                .expect("repair probe lock");
            let state = guard.entry(chain_id).or_default();
            state.tx_hash_seen.insert(repair_tx);
            state.tx_hash_to_sequence.insert(repair_tx, sequence);
            state.sequence_seen.insert(sequence);
            state.sequence_enqueued_seen.insert(sequence);
            state.sequence_admitted_to_aoem_seen.insert(sequence);
        }
        {
            let mut guard = runtime_native_pending_tx_map()
                .lock()
                .expect("pending tx lock");
            let tx = guard
                .get_mut(&chain_id)
                .and_then(|chain_txs| chain_txs.get_mut(&repair_tx))
                .expect("repair tx");
            tx.lifecycle_stage = NetworkRuntimeNativePendingTxLifecycleStageV1::Dropped;
            tx.drop_count = tx.drop_count.saturating_add(1);
        }

        let before = snapshot_network_runtime_native_active_pending_txs_for_repair_window_v1(
            chain_id,
            8,
            Some(sequence),
        );
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].tx_hash, repair_tx);
        assert!(get_network_runtime_native_pending_tx_payload_v1(chain_id, repair_tx).is_some());

        let requeue = requeue_network_runtime_native_repair_pending_for_final_missing_v1(
            chain_id,
            Some(sequence),
        );
        assert_eq!(requeue.repair_attempted_unreceipted_count, 1);
        assert_eq!(
            requeue.repair_attempted_unreceipted_final_missing_overlap_count,
            1
        );
        assert_eq!(requeue.repair_final_missing_payload_available_count, 1);
        assert_eq!(
            requeue.repair_final_missing_payload_available_but_inactive_count,
            0
        );
        assert_eq!(requeue.repair_attempted_unreceipted_requeued_count, 0);
        assert_eq!(requeue.repair_attempted_unreceipted_requeue_failed_count, 0);

        let repaired =
            get_network_runtime_native_pending_tx_v1(chain_id, repair_tx).expect("repair state");
        assert_eq!(
            repaired.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Pending
        );
        let active = snapshot_network_runtime_native_active_pending_txs_for_repair_window_v1(
            chain_id,
            1,
            Some(sequence),
        );
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].tx_hash, repair_tx);
    }

    #[test]
    fn repair_final_missing_payload_exists_cannot_timeout_with_empty_active_pending() {
        let chain_id = 20627_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let repair_tx = [0xe5; 32];
        let sequence = 14_399_u64;

        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            42,
            repair_tx,
            Some(b"tail-repair-final-missing-payload"),
        );
        {
            let mut guard = runtime_native_repair_probe_map()
                .lock()
                .expect("repair probe lock");
            let state = guard.entry(chain_id).or_default();
            state.tx_hash_seen.insert(repair_tx);
            state.tx_hash_to_sequence.insert(repair_tx, sequence);
            state.sequence_seen.insert(sequence);
            state.sequence_enqueued_seen.insert(sequence);
        }
        {
            let mut guard = runtime_native_pending_tx_map()
                .lock()
                .expect("pending tx lock");
            let tx = guard
                .get_mut(&chain_id)
                .and_then(|chain_txs| chain_txs.get_mut(&repair_tx))
                .expect("repair tx");
            tx.lifecycle_stage = NetworkRuntimeNativePendingTxLifecycleStageV1::Dropped;
        }

        let auto_requeued = snapshot_network_runtime_native_active_pending_txs_for_repair_window_v1(
            chain_id,
            4,
            Some(sequence),
        );
        assert_eq!(auto_requeued.len(), 1);
        assert_eq!(auto_requeued[0].tx_hash, repair_tx);
        let requeue = requeue_network_runtime_native_repair_pending_for_final_missing_v1(
            chain_id,
            Some(sequence),
        );
        assert_eq!(requeue.repair_final_missing_payload_available_count, 1);
        assert_eq!(
            requeue.repair_final_missing_payload_available_but_inactive_count,
            0
        );
        assert_eq!(requeue.repair_attempted_unreceipted_requeued_count, 0);
        assert_eq!(requeue.repair_final_missing_invariant_violation_count, 0);
        assert_eq!(
            snapshot_network_runtime_native_active_pending_txs_for_repair_window_v1(
                chain_id,
                4,
                Some(sequence),
            )
            .len(),
            1
        );
    }

    #[test]
    fn repair_final_missing_sequence_maps_to_payload_after_remote_repair_accept() {
        let chain_id = 20628_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let repair_tx = [0xe6; 32];
        let sequence = 14_399_u64;
        let payload = build_native_repair_payload_for_test(chain_id, sequence);

        observe_network_runtime_native_pending_tx_repair_probe_v1(
            chain_id,
            repair_tx,
            1,
            Some(payload.as_slice()),
        );
        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            44,
            repair_tx,
            Some(payload.as_slice()),
        );

        let requeue = requeue_network_runtime_native_repair_pending_for_final_missing_v1(
            chain_id,
            Some(sequence),
        );
        assert_eq!(requeue.repair_final_missing_sequence_to_tx_hash_count, 1);
        assert_eq!(requeue.repair_sequence_payload_index_count, 1);
        assert_eq!(
            requeue.repair_sequence_payload_index_final_missing_overlap_count,
            1
        );
        assert_eq!(requeue.repair_final_missing_tx_hash_payload_hit_count, 1);
        assert_eq!(requeue.repair_final_missing_payload_available_count, 1);
        assert_eq!(
            get_network_runtime_native_pending_tx_payload_v1(chain_id, repair_tx),
            Some(payload)
        );
    }

    #[test]
    fn repair_final_missing_payload_available_requeues_by_sequence() {
        let chain_id = 20629_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let repair_tx = [0xe7; 32];
        let sequence = 14_168_u64;
        let payload = build_native_repair_payload_for_test(chain_id, sequence);

        observe_network_runtime_native_pending_tx_repair_probe_v1(
            chain_id,
            repair_tx,
            1,
            Some(payload.as_slice()),
        );
        if let Ok(mut guard) = runtime_native_pending_tx_payload_map().lock() {
            guard.entry(chain_id).or_default().remove(&repair_tx);
        }

        let requeue = requeue_network_runtime_native_repair_pending_for_final_missing_v1(
            chain_id,
            Some(sequence),
        );
        assert_eq!(requeue.repair_final_missing_payload_available_count, 1);
        assert_eq!(requeue.repair_final_missing_payload_recovered_count, 1);
        assert_eq!(
            requeue.repair_final_missing_payload_recovered_requeued_count,
            1
        );
        assert_eq!(requeue.repair_attempted_unreceipted_requeued_count, 1);
        assert_eq!(
            requeue.repair_final_missing_payload_missing_by_sequence_count,
            0
        );
        assert_eq!(
            get_network_runtime_native_pending_tx_payload_v1(chain_id, repair_tx),
            Some(payload)
        );
        let active = snapshot_network_runtime_native_active_pending_txs_for_repair_window_v1(
            chain_id,
            1,
            Some(sequence),
        );
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].tx_hash, repair_tx);
    }

    #[test]
    fn repair_payload_retention_survives_cleanup_for_receipt_miss_final_missing() {
        let chain_id = 20630_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        set_network_runtime_native_budget_hooks_v1(
            chain_id,
            EthFullnodeBudgetHooksV1 {
                pending_tx_ttl_ms: 100,
                ..default_eth_fullnode_budget_hooks_v1()
            },
        );
        let repair_tx = [0xe8; 32];
        let sequence = 14_200_u64;
        let payload = build_native_repair_payload_for_test(chain_id, sequence);

        observe_network_runtime_native_pending_tx_repair_probe_v1(
            chain_id,
            repair_tx,
            1,
            Some(payload.as_slice()),
        );
        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            45,
            repair_tx,
            Some(payload.as_slice()),
        );
        if let Ok(mut guard) = runtime_native_pending_tx_map().lock() {
            let tx = guard
                .get_mut(&chain_id)
                .and_then(|chain_txs| chain_txs.get_mut(&repair_tx))
                .expect("repair pending");
            tx.lifecycle_stage = NetworkRuntimeNativePendingTxLifecycleStageV1::Dropped;
            tx.first_seen_unix_ms = 1;
            tx.last_updated_unix_ms = 1;
        }
        runtime_native_pending_tx_cleanup_v1(chain_id, 10_000);
        assert!(
            get_network_runtime_native_pending_tx_payload_v1(chain_id, repair_tx).is_none(),
            "normal pending payload map can be cleaned"
        );

        let requeue = requeue_network_runtime_native_repair_pending_for_final_missing_v1(
            chain_id,
            Some(sequence),
        );
        assert_eq!(requeue.repair_final_missing_sequence_to_tx_hash_count, 1);
        assert_eq!(requeue.repair_final_missing_payload_available_count, 1);
        assert_eq!(requeue.repair_final_missing_payload_recovered_count, 1);
        assert_eq!(
            requeue.repair_final_missing_payload_missing_by_sequence_count,
            0
        );
        assert_eq!(
            requeue.repair_payload_retention_false_negative_suspected,
            false
        );
        assert_eq!(
            get_network_runtime_native_pending_tx_payload_v1(chain_id, repair_tx),
            Some(payload)
        );
    }

    #[test]
    fn native_pending_history_compaction_evicts_final_state_and_payloads() {
        let chain_id = 20621_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let active_tx = [0xc1; 32];
        let dropped_tx = [0xc2; 32];
        let included_tx = [0xc3; 32];
        let canonical_hash = [0xd3; 32];

        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            41,
            active_tx,
            Some(b"active-native-payload"),
        );
        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            41,
            dropped_tx,
            Some(b"dropped-native-payload"),
        );
        observe_network_runtime_native_pending_tx_remote_native_payload_v1(
            chain_id,
            41,
            included_tx,
            Some(b"included-native-payload"),
        );
        observe_network_runtime_native_pending_tx_dropped_v1(chain_id, dropped_tx);
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 9,
                block_hash: canonical_hash,
                tx_hashes: vec![included_tx],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 20,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 1,
                block_number: 9,
                block_hash: canonical_hash,
                parent_block_hash: [0x11; 32],
                state_root: [0x22; 32],
                canonical: true,
                safe: true,
                finalized: true,
                reorg_depth_hint: Some(0),
                body_available: true,
                source_peer_id: Some(41),
                observed_unix_ms: 21,
            },
        );

        let summary = compact_network_runtime_native_pending_tx_history_v1(chain_id, 0);
        assert_eq!(summary.included_before, 1);
        assert_eq!(summary.dropped_before, 1);
        assert_eq!(summary.historical_compacted_total, 2);
        assert_eq!(summary.historical_after, 0);
        assert!(summary.historical_payload_bytes_freed > 0);

        let active = snapshot_network_runtime_native_active_pending_txs_v1(chain_id, 16);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].tx_hash, active_tx);
        assert!(get_network_runtime_native_pending_tx_v1(chain_id, dropped_tx).is_none());
        assert!(get_network_runtime_native_pending_tx_v1(chain_id, included_tx).is_none());
        assert!(get_network_runtime_native_pending_tx_payload_v1(chain_id, dropped_tx).is_none());
        assert!(get_network_runtime_native_pending_tx_payload_v1(chain_id, included_tx).is_none());
        assert!(get_network_runtime_native_pending_tx_tombstone_v1(chain_id, dropped_tx).is_some());
        assert!(
            get_network_runtime_native_pending_tx_tombstone_v1(chain_id, included_tx).is_some()
        );
    }

    #[test]
    fn pending_tx_propagation_context_tracks_peer_and_budget_drop() {
        let chain_id = 2062_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let tx_hash = [0xe1; 32];

        observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
            chain_id,
            21,
            tx_hash,
            Some(&[0xf8, 0x01, 0x01]),
        );
        observe_network_runtime_native_pending_tx_propagated_with_context_v1(
            chain_id,
            tx_hash,
            Some(21),
            Some("transactions_dispatch"),
            Some(2),
        );
        let first = get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).expect("first");
        assert_eq!(
            first.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Propagated
        );
        assert_eq!(first.propagation_count, 1);
        assert_eq!(first.last_propagation_peer_id, Some(21));
        assert_eq!(first.last_propagation_failure_class, None);

        observe_network_runtime_native_pending_tx_propagated_with_context_v1(
            chain_id,
            tx_hash,
            Some(21),
            Some("transactions_dispatch"),
            Some(2),
        );
        let second = get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).expect("second");
        assert_eq!(
            second.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Dropped
        );
        assert_eq!(second.drop_count, 1);
        assert_eq!(second.propagation_count, 2);
        assert_eq!(
            second.last_propagation_failure_class.as_deref(),
            Some("budget_limit")
        );
        assert_eq!(
            second.last_propagation_failure_phase.as_deref(),
            Some("transactions_dispatch")
        );
        assert_eq!(
            second.propagation_disposition,
            Some(NetworkRuntimeNativePendingTxPropagationDispositionV1::Dropped)
        );
        assert_eq!(
            second.propagation_stop_reason,
            Some(NetworkRuntimeNativePendingTxPropagationStopReasonV1::BudgetLimit)
        );
        assert_eq!(
            second.propagation_recoverability,
            Some(NetworkRuntimeNativePendingTxPropagationRecoverabilityV1::Recoverable)
        );
    }

    #[test]
    fn pending_tx_propagation_failure_marks_rejected_and_removes_payload() {
        let chain_id = 2063_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let tx_hash = [0xe2; 32];

        observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
            chain_id,
            22,
            tx_hash,
            Some(&[0xf8, 0x01, 0x02]),
        );
        observe_network_runtime_native_pending_tx_propagation_failure_v1(
            chain_id,
            tx_hash,
            Some(22),
            NetworkRuntimeNativePendingTxPropagationStopReasonV1::InvalidTxPayload,
            "transactions_dispatch",
        );

        let state = get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).expect("state");
        assert_eq!(
            state.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Rejected
        );
        assert_eq!(state.reject_count, 1);
        assert_eq!(
            state.last_propagation_failure_class.as_deref(),
            Some("invalid_tx_payload")
        );
        assert_eq!(
            state.last_propagation_failure_phase.as_deref(),
            Some("transactions_dispatch")
        );
        assert_eq!(
            state.propagation_disposition,
            Some(NetworkRuntimeNativePendingTxPropagationDispositionV1::Rejected)
        );
        assert_eq!(
            state.propagation_stop_reason,
            Some(NetworkRuntimeNativePendingTxPropagationStopReasonV1::InvalidTxPayload)
        );
        assert_eq!(
            state.propagation_recoverability,
            Some(NetworkRuntimeNativePendingTxPropagationRecoverabilityV1::NonRecoverable)
        );
        let candidates =
            snapshot_network_runtime_native_pending_tx_broadcast_candidates_v1(chain_id, 16, 3);
        assert!(candidates.is_empty());
    }

    #[test]
    fn pending_tx_propagation_failure_io_write_marks_dropped_recoverable() {
        let chain_id = 2064_u64;
        clear_runtime_sync_status_for_test(chain_id);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let tx_hash = [0xe3; 32];

        observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
            chain_id,
            23,
            tx_hash,
            Some(&[0xf8, 0x01, 0x03]),
        );
        observe_network_runtime_native_pending_tx_propagation_failure_v1(
            chain_id,
            tx_hash,
            Some(23),
            NetworkRuntimeNativePendingTxPropagationStopReasonV1::IoWriteFailure,
            "transactions_dispatch",
        );

        let state = get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).expect("state");
        assert_eq!(
            state.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Dropped
        );
        assert_eq!(
            state.propagation_disposition,
            Some(NetworkRuntimeNativePendingTxPropagationDispositionV1::Dropped)
        );
        assert_eq!(
            state.propagation_stop_reason,
            Some(NetworkRuntimeNativePendingTxPropagationStopReasonV1::IoWriteFailure)
        );
        assert_eq!(
            state.propagation_recoverability,
            Some(NetworkRuntimeNativePendingTxPropagationRecoverabilityV1::Recoverable)
        );
    }
}
