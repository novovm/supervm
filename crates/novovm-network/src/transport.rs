#![forbid(unsafe_code)]

use crate::{
    build_eth_fullnode_native_bodies_request_v1, build_eth_fullnode_native_bootstrap_messages_v1,
    build_eth_fullnode_native_rlpx_status_v1, build_eth_fullnode_native_status_message_v1,
    build_eth_fullnode_native_sync_request_v1, default_eth_rlpx_capabilities_v1,
    derive_eth_fullnode_head_view_with_native_preference_v1,
    derive_eth_fullnode_sync_view_with_native_preference_v1,
    eth_rlpx_block_access_list_hash_from_raw_rlp_v1, eth_rlpx_build_account_range_payload_v1,
    eth_rlpx_build_block_access_lists_payload_v1, eth_rlpx_build_block_bodies_payload_v1,
    eth_rlpx_build_block_headers_payload_v1, eth_rlpx_build_byte_codes_payload_v1,
    eth_rlpx_build_disconnect_payload_v1, eth_rlpx_build_get_account_range_payload_v1,
    eth_rlpx_build_get_block_access_lists_payload_v1, eth_rlpx_build_get_block_bodies_payload_v1,
    eth_rlpx_build_get_block_headers_by_hash_payload_v1,
    eth_rlpx_build_get_block_headers_payload_v1, eth_rlpx_build_get_byte_codes_payload_v1,
    eth_rlpx_build_get_pooled_transactions_payload_v1, eth_rlpx_build_get_receipts_payload_v1,
    eth_rlpx_build_get_storage_ranges_payload_v1, eth_rlpx_build_get_trie_nodes_payload_v1,
    eth_rlpx_build_hello_payload_v1, eth_rlpx_build_new_pooled_transaction_hashes_payload_v1,
    eth_rlpx_build_pooled_transactions_payload_v1, eth_rlpx_build_receipts_payload_v1,
    eth_rlpx_build_status_payload_v1, eth_rlpx_build_storage_ranges_payload_v1,
    eth_rlpx_build_trie_nodes_payload_v1, eth_rlpx_code_hash_v1, eth_rlpx_default_client_name_v1,
    eth_rlpx_default_listen_port_v1, eth_rlpx_disconnect_reason_name_v1,
    eth_rlpx_handshake_initiator_v1, eth_rlpx_hello_profile_v1,
    eth_rlpx_mpt_proof_has_element_in_range_v1, eth_rlpx_mpt_proof_has_right_element_v1,
    eth_rlpx_mpt_verify_proof_value_v1, eth_rlpx_parse_account_range_payload_v1,
    eth_rlpx_parse_block_access_lists_payload_v1, eth_rlpx_parse_block_bodies_payload_v1,
    eth_rlpx_parse_block_headers_payload_v1, eth_rlpx_parse_block_range_update_payload_v1,
    eth_rlpx_parse_byte_codes_payload_v1, eth_rlpx_parse_disconnect_reason_v1,
    eth_rlpx_parse_get_account_range_payload_v1, eth_rlpx_parse_get_block_access_lists_payload_v1,
    eth_rlpx_parse_get_block_bodies_payload_v1, eth_rlpx_parse_get_block_headers_payload_v1,
    eth_rlpx_parse_get_byte_codes_payload_v1, eth_rlpx_parse_get_pooled_transactions_payload_v1,
    eth_rlpx_parse_get_receipts_payload_v1, eth_rlpx_parse_get_storage_ranges_payload_v1,
    eth_rlpx_parse_get_trie_nodes_payload_v1, eth_rlpx_parse_hello_payload_v1,
    eth_rlpx_parse_new_block_hashes_payload_v1, eth_rlpx_parse_new_block_payload_v1,
    eth_rlpx_parse_new_pooled_transaction_hashes_payload_v1,
    eth_rlpx_parse_pooled_transactions_payload_v1, eth_rlpx_parse_receipts_payload_v1,
    eth_rlpx_parse_snap_slim_account_fields_v1, eth_rlpx_parse_status_payload_v1,
    eth_rlpx_parse_storage_ranges_payload_v1, eth_rlpx_parse_transactions_payload_v1,
    eth_rlpx_parse_trie_nodes_payload_v1, eth_rlpx_read_wire_frame_v1,
    eth_rlpx_receipts_root_from_raw_receipts_v1, eth_rlpx_select_shared_eth_version_v1,
    eth_rlpx_select_shared_snap_version_v1, eth_rlpx_snap_base_offset_v1,
    eth_rlpx_snap_full_account_rlp_from_slim_v1, eth_rlpx_snap_storage_root_from_range_v1,
    eth_rlpx_trie_node_hash_v1, eth_rlpx_validate_block_access_list_rlp_context_v1,
    eth_rlpx_validate_block_empty_body_roots_v1, eth_rlpx_validate_trie_node_rlp_v1,
    eth_rlpx_write_wire_frame_v1, get_network_runtime_native_block_access_list_payload_v1,
    get_network_runtime_native_body_snapshot_v1, get_network_runtime_native_head_snapshot_v1,
    get_network_runtime_native_header_rlp_v1, get_network_runtime_native_header_snapshot_v1,
    get_network_runtime_native_pending_tx_payload_v1,
    get_network_runtime_native_receipt_snapshot_v1,
    get_network_runtime_native_snap_account_snapshot_v1,
    get_network_runtime_native_snap_account_storage_snapshot_v1,
    get_network_runtime_native_snap_code_snapshot_v1,
    get_network_runtime_native_snap_trie_node_snapshot_v1, get_network_runtime_native_sync_status,
    get_network_runtime_peer_heads_top_k, get_network_runtime_sync_status,
    has_network_runtime_eth_peer_session, mark_network_runtime_eth_peer_session_closed_v1,
    mark_network_runtime_eth_peer_session_ready_v1, observe_eth_native_bodies_pull,
    observe_eth_native_bodies_response, observe_eth_native_discovery,
    observe_eth_native_headers_pull, observe_eth_native_headers_response, observe_eth_native_hello,
    observe_eth_native_rlpx_auth, observe_eth_native_rlpx_auth_ack, observe_eth_native_snap_pull,
    observe_eth_native_snap_response, observe_eth_native_status,
    observe_network_runtime_eth_peer_body_success_v1,
    observe_network_runtime_eth_peer_connect_failure_v1,
    observe_network_runtime_eth_peer_connected_v1, observe_network_runtime_eth_peer_connecting_v1,
    observe_network_runtime_eth_peer_decode_failure_v1,
    observe_network_runtime_eth_peer_disconnect_v1, observe_network_runtime_eth_peer_discovered_v1,
    observe_network_runtime_eth_peer_handshake_failure_v1, observe_network_runtime_eth_peer_head,
    observe_network_runtime_eth_peer_header_success_v1,
    observe_network_runtime_eth_peer_hello_ok_v1,
    observe_network_runtime_eth_peer_selection_round_v1,
    observe_network_runtime_eth_peer_status_ok_v1, observe_network_runtime_eth_peer_syncing_v1,
    observe_network_runtime_eth_peer_timeout_v1,
    observe_network_runtime_eth_peer_validation_reject_v1, observe_network_runtime_local_head_max,
    observe_network_runtime_native_pending_tx_broadcast_dispatch_v1,
    observe_network_runtime_native_pending_tx_ingress_with_payload_v1,
    observe_network_runtime_native_pending_tx_propagated_v1,
    observe_network_runtime_native_pending_tx_propagated_with_context_v1,
    observe_network_runtime_native_pending_tx_propagation_failure_v1,
    observe_network_runtime_peer_head, observe_network_runtime_peer_head_with_local_head_max,
    plan_network_runtime_sync_pull_window, register_network_runtime_peer,
    resolve_eth_chain_config_v1, resolve_eth_fullnode_native_runtime_config_v1,
    route::PluginPeerEndpoint, select_eth_fullnode_native_bootstrap_candidates_v1,
    select_eth_fullnode_native_sync_targets_v1, set_eth_fullnode_native_worker_runtime_snapshot_v1,
    set_network_runtime_native_block_access_list_payload_v1,
    set_network_runtime_native_body_snapshot_v1, set_network_runtime_native_budget_hooks_v1,
    set_network_runtime_native_head_snapshot_v1, set_network_runtime_native_header_rlp_v1,
    set_network_runtime_native_header_snapshot_v1, set_network_runtime_native_receipt_snapshot_v1,
    set_network_runtime_native_snap_account_range_progress_v1,
    set_network_runtime_native_snap_account_snapshot_v1,
    set_network_runtime_native_snap_account_storage_snapshot_v1,
    set_network_runtime_native_snap_code_snapshot_v1,
    set_network_runtime_native_snap_trie_node_snapshot_v1,
    set_network_runtime_native_state_root_validation_v1,
    snapshot_eth_fullnode_native_head_block_object_v1,
    snapshot_eth_fullnode_peer_selection_scores_v1,
    snapshot_network_runtime_eth_peer_lifecycle_summary_v1,
    snapshot_network_runtime_eth_peer_sessions_for_peers_v1,
    snapshot_network_runtime_native_canonical_blocks_v1,
    snapshot_network_runtime_native_canonical_chain_v1,
    snapshot_network_runtime_native_execution_budget_runtime_summary_v1,
    snapshot_network_runtime_native_pending_tx_broadcast_candidates_v1,
    snapshot_network_runtime_native_pending_tx_broadcast_runtime_summary_v1,
    snapshot_network_runtime_native_pending_tx_summary_v1,
    snapshot_network_runtime_native_pending_txs_v1,
    snapshot_network_runtime_native_snap_account_snapshots_v1, unregister_network_runtime_peer,
    upsert_network_runtime_eth_peer_session, validate_eth_chain_config_peer_status_v1,
    write_eth_fullnode_native_worker_runtime_snapshot_default_path_v1,
    EthChainConfigPeerValidationReasonV1, EthFullnodeBudgetHooksV1,
    EthFullnodeNativePeerFailureSnapshotV1, EthFullnodeNativeWorkerRuntimeSnapshotV1,
    EthPeerLifecycleSummaryV1, EthPeerSelectionQualitySummaryV1,
    EthPeerSelectionRoundObservationV1, EthPeerSelectionScoreV1, EthRlpxAccountRangeResponseV1,
    EthRlpxBlockAccessListsResponseV1, EthRlpxBlockBodiesResponseV1, EthRlpxBlockBodyPayloadV1,
    EthRlpxBlockHeaderRecordV1, EthRlpxBlockHeadersResponseV1, EthRlpxByteCodesResponseV1,
    EthRlpxFrameSessionV1, EthRlpxGetAccountRangeRequestV1, EthRlpxGetBlockBodiesRequestV1,
    EthRlpxGetBlockHeadersRequestV1, EthRlpxGetByteCodesRequestV1,
    EthRlpxGetStorageRangesRequestV1, EthRlpxGetTrieNodesRequestV1, EthRlpxNewBlockPayloadV1,
    EthRlpxPooledTransactionsPayloadV1, EthRlpxReceiptsResponseV1, EthRlpxStatusV1,
    EthRlpxStorageRangesResponseV1, EthRlpxTrieNodesResponseV1,
    NetworkRuntimeNativePendingTxPropagationStopReasonV1, NetworkRuntimeNativeReceiptSnapshotV1,
    NetworkRuntimeNativeSnapAccountRangeProgressV1, NetworkRuntimeNativeSnapAccountSnapshotV1,
    NetworkRuntimeNativeSnapAccountStorageSnapshotV1, NetworkRuntimeNativeSnapCodeSnapshotV1,
    NetworkRuntimeNativeSnapStorageSlotSnapshotV1, NetworkRuntimeNativeSnapTrieNodeSnapshotV1,
    NetworkRuntimeNativeSyncPhaseV1, ETH_FULLNODE_DEFAULT_SYNC_PULL_FINALIZE_BATCH,
    ETH_FULLNODE_NATIVE_WORKER_RUNTIME_SCHEMA_V1, ETH_RLPX_BASE_PROTOCOL_OFFSET,
    ETH_RLPX_ETH_BLOCK_ACCESS_LISTS_MSG, ETH_RLPX_ETH_BLOCK_BODIES_MSG,
    ETH_RLPX_ETH_BLOCK_HEADERS_MSG, ETH_RLPX_ETH_BLOCK_RANGE_UPDATE_MSG,
    ETH_RLPX_ETH_GET_BLOCK_ACCESS_LISTS_MSG, ETH_RLPX_ETH_GET_BLOCK_BODIES_MSG,
    ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG, ETH_RLPX_ETH_GET_POOLED_TRANSACTIONS_MSG,
    ETH_RLPX_ETH_GET_RECEIPTS_MSG, ETH_RLPX_ETH_NEW_BLOCK_HASHES_MSG, ETH_RLPX_ETH_NEW_BLOCK_MSG,
    ETH_RLPX_ETH_NEW_POOLED_TRANSACTION_HASHES_MSG, ETH_RLPX_ETH_POOLED_TRANSACTIONS_MSG,
    ETH_RLPX_ETH_RECEIPTS_MSG, ETH_RLPX_ETH_STATUS_MSG, ETH_RLPX_ETH_TRANSACTIONS_MSG,
    ETH_RLPX_P2P_DISCONNECT_MSG, ETH_RLPX_P2P_HELLO_MSG, ETH_RLPX_P2P_PING_MSG,
    ETH_RLPX_P2P_PONG_MSG, ETH_RLPX_SNAP_ACCOUNT_RANGE_MSG, ETH_RLPX_SNAP_BYTE_CODES_MSG,
    ETH_RLPX_SNAP_DEFAULT_ACCOUNT_RANGE_BYTES, ETH_RLPX_SNAP_GET_ACCOUNT_RANGE_MSG,
    ETH_RLPX_SNAP_GET_BYTE_CODES_MSG, ETH_RLPX_SNAP_GET_STORAGE_RANGES_MSG,
    ETH_RLPX_SNAP_GET_TRIE_NODES_MSG, ETH_RLPX_SNAP_STORAGE_RANGES_MSG,
    ETH_RLPX_SNAP_TRIE_NODES_MSG,
};
use dashmap::DashMap;
use novovm_protocol::{
    decode as protocol_decode, decode_block_header_wire_v1, encode as protocol_encode,
    encode_block_header_wire_v1,
    protocol_catalog::distributed_occc::gossip::MessageType as DistributedOcccMessageType,
    BlockHeaderWireV1, ConsensusPluginBindingV1, EvmNativeBlockBodyWireV1,
    EvmNativeBlockHeaderWireV1, EvmNativeMessage, FinalityMessage,
    GossipMessage as ProtocolGossipMessage, NodeId, PacemakerMessage, ProtocolMessage,
    TwoPcMessage, CONSENSUS_PLUGIN_CLASS_CODE,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;

const ETH_FULLNODE_NATIVE_HEADERS_SERVE_MAX_V1: usize = 192;
const ETH_FULLNODE_NATIVE_BODIES_SERVE_MAX_V1: usize = 128;
const ETH_FULLNODE_NATIVE_STATUS_HEAD_PIVOT_MIN_GAP_V1: u64 = 8_192;
const ETH_FULLNODE_NATIVE_RUNTIME_PEER_DETAIL_LIMIT_V1: usize = 64;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("peer not found: {0:?}")]
    PeerNotFound(NodeId),
    #[error("queue full")]
    QueueFull,
    #[error("local node mismatch: expected {expected:?}, got {got:?}")]
    LocalNodeMismatch { expected: NodeId, got: NodeId },
    #[error("address parse failed: {0}")]
    AddressParse(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("encode failed: {0}")]
    Encode(String),
    #[error("decode failed: {0}")]
    Decode(String),
}

/// Minimal transport interface.
///
/// V3 intent: keep protocol concerns in novovm-protocol, keep transport concerns here.
/// Higher-level routing/consensus lives elsewhere.
pub trait Transport: Send + Sync {
    fn send(&self, to: NodeId, msg: ProtocolMessage) -> Result<(), NetworkError>;
    fn try_recv(&self, me: NodeId) -> Result<Option<ProtocolMessage>, NetworkError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthFullnodeNativePeerWorkerConfigV1 {
    pub chain_id: u64,
    pub local_node: NodeId,
    pub peers: Vec<NodeId>,
    pub peer_endpoints: Vec<PluginPeerEndpoint>,
    pub recv_budget: usize,
    pub sync_target_fanout: usize,
    pub budget_hooks: EthFullnodeBudgetHooksV1,
}

impl EthFullnodeNativePeerWorkerConfigV1 {
    #[must_use]
    pub fn normalized(&self) -> Self {
        let hard_limit = self.budget_hooks.active_native_peer_hard_limit.max(1) as usize;
        let recv_budget_cap = self.budget_hooks.native_recv_budget_per_tick.max(1) as usize;
        let sync_fanout_cap = self.budget_hooks.sync_target_fanout.max(1) as usize;
        let mut peers = Vec::new();
        let mut peer_endpoints = Vec::new();
        for endpoint in &self.peer_endpoints {
            if endpoint.node_hint == self.local_node.0
                || peer_endpoints
                    .iter()
                    .any(|existing: &PluginPeerEndpoint| existing.node_hint == endpoint.node_hint)
            {
                continue;
            }
            let peer = NodeId(endpoint.node_hint.max(1));
            if !peers.contains(&peer) {
                peers.push(peer);
            }
            peer_endpoints.push(endpoint.clone());
            if peer_endpoints.len() >= hard_limit {
                break;
            }
        }
        for peer in &self.peers {
            if *peer == self.local_node || peers.contains(peer) {
                continue;
            }
            peers.push(*peer);
            if peers.len() >= hard_limit {
                break;
            }
        }
        Self {
            chain_id: self.chain_id,
            local_node: self.local_node,
            peers,
            peer_endpoints,
            recv_budget: self.recv_budget.max(1).min(recv_budget_cap),
            sync_target_fanout: self.sync_target_fanout.max(1).min(sync_fanout_cap),
            budget_hooks: self.budget_hooks.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthFullnodeNativePeerWorkerPlanV1 {
    pub chain_id: u64,
    pub local_node: NodeId,
    pub candidate_peers: Vec<NodeId>,
    pub candidate_peer_endpoints: Vec<PluginPeerEndpoint>,
    pub lifecycle_summary: EthPeerLifecycleSummaryV1,
    pub selection_quality_summary: EthPeerSelectionQualitySummaryV1,
    pub selection_scores: Vec<EthPeerSelectionScoreV1>,
    pub bootstrap_peers: Vec<NodeId>,
    pub sync_peers: Vec<NodeId>,
    pub recv_budget: usize,
    pub budget_hooks: EthFullnodeBudgetHooksV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthFullnodeNativePeerWorkerV1 {
    config: EthFullnodeNativePeerWorkerConfigV1,
}

impl EthFullnodeNativePeerWorkerV1 {
    #[must_use]
    pub fn new(config: EthFullnodeNativePeerWorkerConfigV1) -> Self {
        Self {
            config: config.normalized(),
        }
    }

    #[must_use]
    pub fn config(&self) -> &EthFullnodeNativePeerWorkerConfigV1 {
        &self.config
    }

    #[must_use]
    pub fn plan(&self) -> EthFullnodeNativePeerWorkerPlanV1 {
        let soft_limit = self
            .config
            .budget_hooks
            .active_native_peer_soft_limit
            .max(1) as usize;
        let candidate_peers = self.config.peers.clone();
        let bootstrap_window = soft_limit.min(self.config.sync_target_fanout.max(4)).max(1);

        let mut session_peers = Vec::new();
        for peer in &candidate_peers {
            if has_network_runtime_eth_peer_session(self.config.chain_id, peer.0) {
                session_peers.push(*peer);
            }
        }
        let bootstrap_candidates = select_eth_fullnode_native_bootstrap_candidates_v1(
            self.config.chain_id,
            &candidate_peers,
            bootstrap_window,
        );

        let active_session_count = session_peers.len().min(soft_limit);
        let bootstrap_budget = soft_limit.saturating_sub(active_session_count);
        let bootstrap_peers = if bootstrap_budget == 0 && session_peers.is_empty() {
            bootstrap_candidates
                .into_iter()
                .take(soft_limit)
                .collect::<Vec<_>>()
        } else {
            bootstrap_candidates
                .into_iter()
                .take(bootstrap_budget)
                .collect::<Vec<_>>()
        };
        let sync_fanout = self.config.sync_target_fanout.min(soft_limit);
        let sync_peers = select_eth_fullnode_native_sync_targets_v1(
            self.config.chain_id,
            &session_peers,
            sync_fanout,
        );
        let (selection_scores, selection_quality_summary, _) =
            snapshot_eth_fullnode_peer_selection_scores_v1(
                self.config.chain_id,
                &candidate_peers,
                &bootstrap_peers,
                &sync_peers,
            );
        let lifecycle_summary = snapshot_network_runtime_eth_peer_lifecycle_summary_v1(
            self.config.chain_id,
            candidate_peers.as_slice(),
        );

        EthFullnodeNativePeerWorkerPlanV1 {
            chain_id: self.config.chain_id,
            local_node: self.config.local_node,
            candidate_peers,
            candidate_peer_endpoints: self.config.peer_endpoints.clone(),
            lifecycle_summary,
            selection_quality_summary,
            selection_scores,
            bootstrap_peers,
            sync_peers,
            recv_budget: self.config.recv_budget,
            budget_hooks: self.config.budget_hooks.clone(),
        }
    }

    pub fn drive_once<T: Transport>(
        &self,
        transport: &T,
    ) -> Result<EthFullnodeNativeDriveReportV1, NetworkError> {
        let plan = self.plan();
        set_network_runtime_native_budget_hooks_v1(plan.chain_id, plan.budget_hooks.clone());
        let mut report = EthFullnodeNativeDriveReportV1 {
            lifecycle_summary: plan.lifecycle_summary.clone(),
            selection_quality_summary: plan.selection_quality_summary.clone(),
            ..EthFullnodeNativeDriveReportV1::default()
        };
        for &peer in &plan.bootstrap_peers {
            for msg in build_eth_fullnode_native_bootstrap_messages_v1(
                plan.local_node,
                peer,
                plan.chain_id,
            ) {
                transport.send(peer, msg)?;
                report.outbound_messages = report.outbound_messages.saturating_add(1);
            }
            report.bootstrapped_peers = report.bootstrapped_peers.saturating_add(1);
        }

        for &peer in &plan.sync_peers {
            if dispatch_eth_fullnode_native_sync_from_runtime_v1(
                transport,
                plan.local_node,
                peer,
                plan.chain_id,
            )? {
                report.outbound_messages = report.outbound_messages.saturating_add(1);
                report.sync_requested_peers = report.sync_requested_peers.saturating_add(1);
            }
        }

        for _ in 0..plan.recv_budget {
            if transport.try_recv(plan.local_node)?.is_some() {
                report.inbound_messages = report.inbound_messages.saturating_add(1);
            } else {
                break;
            }
        }

        report.lifecycle_summary = snapshot_network_runtime_eth_peer_lifecycle_summary_v1(
            plan.chain_id,
            plan.candidate_peers.as_slice(),
        );
        Ok(report)
    }

    fn endpoint_for_peer(&self, peer: NodeId) -> Option<PluginPeerEndpoint> {
        self.config
            .peer_endpoints
            .iter()
            .find(|endpoint| endpoint.node_hint == peer.0)
            .cloned()
    }

    pub fn drive_real_network_once(
        &self,
    ) -> Result<EthFullnodeNativeRealDriveReportV1, NetworkError> {
        reconcile_eth_fullnode_native_rlpx_runtime_sessions_with_live_sessions_v1(
            self.config.chain_id,
            self.config.peers.as_slice(),
        );
        let plan = self.plan();
        set_network_runtime_native_budget_hooks_v1(plan.chain_id, plan.budget_hooks.clone());
        let mut report = EthFullnodeNativeRealDriveReportV1 {
            scheduled_bootstrap_peers: plan.bootstrap_peers.len(),
            scheduled_sync_peers: plan.sync_peers.len(),
            lifecycle_summary: plan.lifecycle_summary.clone(),
            selection_quality_summary: plan.selection_quality_summary.clone(),
            ..EthFullnodeNativeRealDriveReportV1::default()
        };
        let bootstrap_started = Instant::now();
        let bootstrap_tick_budget = eth_fullnode_native_rlpx_bootstrap_tick_budget_v1();
        let mut bootstrap_jobs = Vec::new();
        for &peer in plan.bootstrap_peers.iter() {
            if !bootstrap_jobs.is_empty() && bootstrap_started.elapsed() >= bootstrap_tick_budget {
                report.skipped_bootstrap_budget_peers = plan
                    .bootstrap_peers
                    .len()
                    .saturating_sub(bootstrap_jobs.len())
                    .saturating_sub(report.skipped_missing_endpoint_peers);
                break;
            }
            let Some(endpoint) = self.endpoint_for_peer(peer) else {
                report.skipped_missing_endpoint_peers =
                    report.skipped_missing_endpoint_peers.saturating_add(1);
                continue;
            };
            bootstrap_jobs.push((peer, endpoint));
        }
        report.attempted_bootstrap_peers = bootstrap_jobs.len();
        let bootstrap_results = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(bootstrap_jobs.len());
            for (peer, endpoint) in bootstrap_jobs {
                let budget_hooks = plan.budget_hooks.clone();
                handles.push(scope.spawn(move || {
                    drive_eth_fullnode_native_rlpx_bootstrap_peer_once_v1(
                        plan.chain_id,
                        plan.local_node,
                        peer,
                        endpoint,
                        budget_hooks,
                    )
                }));
            }
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|_| EthFullnodeNativeBootstrapPeerDriveResultV1 {
                            peer: NodeId(0),
                            endpoint: None,
                            connected: false,
                            outcome: Err(NetworkError::Io(
                                "rlpx_bootstrap_thread_panic".to_string(),
                            )),
                        })
                })
                .collect::<Vec<_>>()
        });
        for bootstrap_result in bootstrap_results {
            if bootstrap_result.connected {
                report.connected_peers = report.connected_peers.saturating_add(1);
                report.ready_peers = report.ready_peers.saturating_add(1);
                report.status_updates = report.status_updates.saturating_add(1);
            }
            match bootstrap_result.outcome {
                Ok(peer_report) => {
                    absorb_eth_fullnode_native_rlpx_peer_tick_report_v1(
                        &mut report,
                        bootstrap_result.peer,
                        peer_report,
                    );
                }
                Err(err) if bootstrap_result.connected => {
                    report.failed_sync_peers = report.failed_sync_peers.saturating_add(1);
                    report
                        .peer_failures
                        .push(build_eth_fullnode_peer_failure_report_v1(
                            plan.chain_id,
                            bootstrap_result.peer,
                            bootstrap_result.endpoint.as_ref(),
                            EthFullnodeNativePeerDrivePhaseV1::Sync,
                            &err,
                        ));
                }
                Err(err) => {
                    report.failed_bootstrap_peers = report.failed_bootstrap_peers.saturating_add(1);
                    report
                        .peer_failures
                        .push(build_eth_fullnode_peer_failure_report_v1(
                            plan.chain_id,
                            bootstrap_result.peer,
                            bootstrap_result.endpoint.as_ref(),
                            EthFullnodeNativePeerDrivePhaseV1::Bootstrap,
                            &err,
                        ));
                }
            }
        }
        let sync_started = Instant::now();
        let sync_tick_budget = eth_fullnode_native_rlpx_bootstrap_tick_budget_v1();
        let mut skipped_sync_missing_endpoint_peers = 0usize;
        for &peer in plan.sync_peers.iter() {
            if report.attempted_sync_peers > 0 && sync_started.elapsed() >= sync_tick_budget {
                report.skipped_sync_budget_peers = plan
                    .sync_peers
                    .len()
                    .saturating_sub(report.attempted_sync_peers)
                    .saturating_sub(skipped_sync_missing_endpoint_peers);
                break;
            }
            let Some(endpoint) = self.endpoint_for_peer(peer) else {
                report.skipped_missing_endpoint_peers =
                    report.skipped_missing_endpoint_peers.saturating_add(1);
                skipped_sync_missing_endpoint_peers =
                    skipped_sync_missing_endpoint_peers.saturating_add(1);
                continue;
            };
            report.attempted_sync_peers = report.attempted_sync_peers.saturating_add(1);
            match drive_eth_fullnode_native_rlpx_peer_session_once_v1(
                plan.chain_id,
                plan.local_node,
                peer,
                &endpoint,
                &plan.budget_hooks,
            ) {
                Ok(peer_report) => {
                    report.ready_peers = report.ready_peers.saturating_add(1);
                    absorb_eth_fullnode_native_rlpx_peer_tick_report_v1(
                        &mut report,
                        peer,
                        peer_report,
                    );
                }
                Err(err) => {
                    report.failed_sync_peers = report.failed_sync_peers.saturating_add(1);
                    report
                        .peer_failures
                        .push(build_eth_fullnode_peer_failure_report_v1(
                            plan.chain_id,
                            peer,
                            Some(&endpoint),
                            EthFullnodeNativePeerDrivePhaseV1::Sync,
                            &err,
                        ));
                }
            }
        }
        report.lifecycle_summary = snapshot_network_runtime_eth_peer_lifecycle_summary_v1(
            plan.chain_id,
            plan.candidate_peers.as_slice(),
        );
        let connect_failure_peers = report
            .peer_failures
            .iter()
            .filter(|failure| {
                matches!(
                    failure.lifecycle_class,
                    Some(crate::EthPeerFailureClassV1::ConnectFailure)
                )
            })
            .map(|failure| failure.peer_id)
            .collect::<Vec<_>>();
        let handshake_failure_peers = report
            .peer_failures
            .iter()
            .filter(|failure| {
                matches!(
                    failure.lifecycle_class,
                    Some(crate::EthPeerFailureClassV1::HandshakeFailure)
                )
            })
            .map(|failure| failure.peer_id)
            .collect::<Vec<_>>();
        let decode_failure_peers = report
            .peer_failures
            .iter()
            .filter(|failure| {
                matches!(
                    failure.lifecycle_class,
                    Some(crate::EthPeerFailureClassV1::DecodeFailure)
                )
            })
            .map(|failure| failure.peer_id)
            .collect::<Vec<_>>();
        let timeout_failure_peers = report
            .peer_failures
            .iter()
            .filter(|failure| {
                matches!(
                    failure.lifecycle_class,
                    Some(crate::EthPeerFailureClassV1::Timeout)
                )
            })
            .map(|failure| failure.peer_id)
            .collect::<Vec<_>>();
        let validation_reject_peers = report
            .peer_failures
            .iter()
            .filter(|failure| {
                matches!(
                    failure.lifecycle_class,
                    Some(crate::EthPeerFailureClassV1::ValidationReject)
                )
            })
            .map(|failure| failure.peer_id)
            .collect::<Vec<_>>();
        let disconnect_peers = report
            .peer_failures
            .iter()
            .filter(|failure| {
                matches!(
                    failure.lifecycle_class,
                    Some(crate::EthPeerFailureClassV1::Disconnect)
                )
            })
            .map(|failure| failure.peer_id)
            .collect::<Vec<_>>();
        let capacity_reject_peers = report
            .peer_failures
            .iter()
            .filter(|failure| {
                failure.reason_name.as_deref() == Some("too_many_peers")
                    || failure.reason_code == Some(0x04)
            })
            .map(|failure| failure.peer_id)
            .collect::<Vec<_>>();
        let material_success_peers = eth_fullnode_native_material_success_peer_ids_v1(&report);
        observe_network_runtime_eth_peer_selection_round_v1(
            plan.chain_id,
            EthPeerSelectionRoundObservationV1 {
                peers: &plan.candidate_peers,
                selected_bootstrap_peers: &plan.bootstrap_peers,
                selected_sync_peers: &plan.sync_peers,
                header_success_peers: &report.header_updated_peer_ids,
                body_success_peers: &material_success_peers,
                connect_failure_peers: &connect_failure_peers,
                handshake_failure_peers: &handshake_failure_peers,
                decode_failure_peers: &decode_failure_peers,
                timeout_failure_peers: &timeout_failure_peers,
                validation_reject_peers: &validation_reject_peers,
                disconnect_peers: &disconnect_peers,
                capacity_reject_peers: &capacity_reject_peers,
            },
        );
        let runtime_snapshot = build_eth_fullnode_native_worker_runtime_snapshot_v1(&plan, &report);
        set_eth_fullnode_native_worker_runtime_snapshot_v1(plan.chain_id, runtime_snapshot.clone());
        let _ =
            write_eth_fullnode_native_worker_runtime_snapshot_default_path_v1(&runtime_snapshot);
        Ok(report)
    }
}

pub fn bootstrap_eth_fullnode_native_peer_v1<T: Transport>(
    transport: &T,
    local_node: NodeId,
    peer: NodeId,
    chain_id: u64,
) -> Result<(), NetworkError> {
    for msg in build_eth_fullnode_native_bootstrap_messages_v1(local_node, peer, chain_id) {
        transport.send(peer, msg)?;
    }
    Ok(())
}

pub fn dispatch_eth_fullnode_native_sync_from_runtime_v1<T: Transport>(
    transport: &T,
    local_node: NodeId,
    peer: NodeId,
    chain_id: u64,
) -> Result<bool, NetworkError> {
    let Some(msg) = build_eth_fullnode_native_sync_request_v1(local_node, chain_id) else {
        return Ok(false);
    };
    transport.send(peer, msg)?;
    Ok(true)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EthFullnodeNativeDriveReportV1 {
    pub bootstrapped_peers: usize,
    pub sync_requested_peers: usize,
    pub outbound_messages: usize,
    pub inbound_messages: usize,
    pub lifecycle_summary: EthPeerLifecycleSummaryV1,
    pub selection_quality_summary: EthPeerSelectionQualitySummaryV1,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EthFullnodeNativeRealDriveReportV1 {
    pub scheduled_bootstrap_peers: usize,
    pub scheduled_sync_peers: usize,
    pub attempted_bootstrap_peers: usize,
    pub attempted_sync_peers: usize,
    pub failed_bootstrap_peers: usize,
    pub failed_sync_peers: usize,
    pub skipped_missing_endpoint_peers: usize,
    pub skipped_bootstrap_budget_peers: usize,
    pub skipped_sync_budget_peers: usize,
    pub connected_peers: usize,
    pub ready_peers: usize,
    pub status_updates: usize,
    pub header_updates: usize,
    pub body_updates: usize,
    pub receipt_updates: usize,
    pub header_updated_peer_ids: Vec<u64>,
    pub body_updated_peer_ids: Vec<u64>,
    pub receipt_updated_peer_ids: Vec<u64>,
    pub sync_requests: usize,
    pub inbound_frames: usize,
    pub peer_failures: Vec<EthFullnodeNativePeerFailureV1>,
    pub lifecycle_summary: EthPeerLifecycleSummaryV1,
    pub selection_quality_summary: EthPeerSelectionQualitySummaryV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EthFullnodeNativePeerDrivePhaseV1 {
    Bootstrap,
    Sync,
}

impl EthFullnodeNativePeerDrivePhaseV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::Sync => "sync",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EthFullnodeNativePeerFailureClassV1 {
    PeerNotFound,
    QueueFull,
    LocalNodeMismatch,
    AddressParse,
    Io,
    Timeout,
    Encode,
    Decode,
}

impl EthFullnodeNativePeerFailureClassV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PeerNotFound => "peer_not_found",
            Self::QueueFull => "queue_full",
            Self::LocalNodeMismatch => "local_node_mismatch",
            Self::AddressParse => "address_parse",
            Self::Io => "io",
            Self::Timeout => "timeout",
            Self::Encode => "encode",
            Self::Decode => "decode",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthFullnodeNativePeerFailureV1 {
    pub peer_id: u64,
    pub endpoint: Option<String>,
    pub phase: EthFullnodeNativePeerDrivePhaseV1,
    pub class: EthFullnodeNativePeerFailureClassV1,
    pub lifecycle_class: Option<crate::EthPeerFailureClassV1>,
    pub reason_code: Option<u64>,
    pub reason_name: Option<String>,
    pub error: String,
}

struct EthFullnodeNativeRlpxLivePeerSessionV1 {
    endpoint: PluginPeerEndpoint,
    stream: TcpStream,
    frame_session: EthRlpxFrameSessionV1,
    _negotiated_eth_version: u8,
    _negotiated_snap_version: Option<u8>,
    remote_status: EthRlpxStatusV1,
    last_sync_request_unix_ms: u64,
    last_headers_request_id: Option<u64>,
    pending_headers_request: Option<EthRlpxGetBlockHeadersRequestV1>,
    last_bodies_request_id: Option<u64>,
    last_receipts_request_id: Option<u64>,
    last_snap_account_range_request_id: Option<u64>,
    last_snap_storage_ranges_request_id: Option<u64>,
    last_snap_byte_codes_request_id: Option<u64>,
    last_snap_trie_nodes_request_id: Option<u64>,
    last_snap_state_root: Option<[u8; 32]>,
    last_snap_account_origin: Option<[u8; 32]>,
    last_snap_account_limit: Option<[u8; 32]>,
    pending_snap_next_account_origin: Option<[u8; 32]>,
    pending_snap_storage_accounts: Vec<[u8; 32]>,
    pending_snap_storage_origin: Vec<u8>,
    pending_snap_storage_limit: Vec<u8>,
    pending_snap_storage_deferred_accounts: Vec<[u8; 32]>,
    pending_snap_code_hashes: Vec<[u8; 32]>,
    pending_snap_trie_node_pathsets: Vec<Vec<Vec<u8>>>,
    pending_snap_trie_node_hashes: Vec<[u8; 32]>,
    pending_snap_trie_node_retry_count: u8,
    last_block_access_lists_request_id: Option<u64>,
    queued_block_access_lists: Vec<EthFullnodeNativePendingBlockAccessListV1>,
    pending_block_access_lists: Vec<EthFullnodeNativePendingBlockAccessListV1>,
    last_pooled_transactions_request_id: Option<u64>,
    last_tx_broadcast_unix_ms: u64,
    pending_body_headers: Vec<EthFullnodeNativePendingBodyHeaderV1>,
    pending_body_request_offset: usize,
    pending_receipt_request_offset: usize,
    pending_pooled_transaction_hashes: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EthFullnodeNativePendingBodyHeaderV1 {
    number: u64,
    hash: [u8; 32],
    parent_hash: [u8; 32],
    state_root: [u8; 32],
    transactions_root: [u8; 32],
    receipts_root: [u8; 32],
    tx_count: Option<usize>,
    withdrawal_count: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EthFullnodeNativePendingBlockAccessListV1 {
    block_hash: [u8; 32],
    block_access_list_hash: [u8; 32],
    gas_limit: Option<u64>,
    tx_count: Option<usize>,
}

const ETH_FULLNODE_NATIVE_MISSING_BODY_RECOVERY_BATCH_MAX_V1: usize = 4;
const ETH_FULLNODE_NATIVE_MISSING_BODY_CHASE_HEAD_BATCH_MAX_V1: usize = 1;
const ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_BODY_KIND_V1: u8 = 1;
const ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_RECEIPT_KIND_V1: u8 = 2;
const ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_TTL_MS_V1: u64 = 30_000;
const ETH_FULLNODE_NATIVE_HEADER_INFLIGHT_TTL_MS_V1: u64 = 30_000;

type EthFullnodeNativeRlpxSessionKeyV1 = (u64, u64);
type EthFullnodeNativeRlpxSessionMapV1 =
    HashMap<EthFullnodeNativeRlpxSessionKeyV1, EthFullnodeNativeRlpxLivePeerSessionV1>;
static ETH_FULLNODE_NATIVE_RLPX_SESSIONS: OnceLock<Mutex<EthFullnodeNativeRlpxSessionMapV1>> =
    OnceLock::new();
type EthFullnodeNativeRecoveryInflightKeyV1 = (u64, u8, [u8; 32]);
type EthFullnodeNativeRecoveryInflightMapV1 =
    HashMap<EthFullnodeNativeRecoveryInflightKeyV1, EthFullnodeNativeRecoveryInflightV1>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EthFullnodeNativeRecoveryInflightV1 {
    peer_id: u64,
    request_id: u64,
    observed_unix_ms: u64,
}

static ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT: OnceLock<
    Mutex<EthFullnodeNativeRecoveryInflightMapV1>,
> = OnceLock::new();
type EthFullnodeNativeHeaderInflightKeyV1 = (u64, u64, u64, bool);
type EthFullnodeNativeHeaderInflightMapV1 =
    HashMap<EthFullnodeNativeHeaderInflightKeyV1, EthFullnodeNativeHeaderInflightV1>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EthFullnodeNativeHeaderInflightV1 {
    peer_id: u64,
    request_id: u64,
    observed_unix_ms: u64,
}

static ETH_FULLNODE_NATIVE_HEADER_INFLIGHT: OnceLock<Mutex<EthFullnodeNativeHeaderInflightMapV1>> =
    OnceLock::new();

fn eth_fullnode_native_rlpx_sessions_v1() -> &'static Mutex<EthFullnodeNativeRlpxSessionMapV1> {
    ETH_FULLNODE_NATIVE_RLPX_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn eth_fullnode_native_recovery_inflight_v1(
) -> &'static Mutex<EthFullnodeNativeRecoveryInflightMapV1> {
    ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT.get_or_init(|| Mutex::new(HashMap::new()))
}

fn eth_fullnode_native_header_inflight_v1() -> &'static Mutex<EthFullnodeNativeHeaderInflightMapV1>
{
    ETH_FULLNODE_NATIVE_HEADER_INFLIGHT.get_or_init(|| Mutex::new(HashMap::new()))
}

fn eth_fullnode_native_rlpx_live_peer_ids_v1(chain_id: u64) -> HashSet<u64> {
    eth_fullnode_native_rlpx_sessions_v1()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .keys()
        .filter_map(|(session_chain_id, peer_id)| {
            if *session_chain_id == chain_id {
                Some(*peer_id)
            } else {
                None
            }
        })
        .collect()
}

fn prune_eth_fullnode_native_recovery_inflight_locked_v1(
    inflight: &mut EthFullnodeNativeRecoveryInflightMapV1,
    now_ms: u64,
) {
    inflight.retain(|_, entry| {
        now_ms.saturating_sub(entry.observed_unix_ms)
            <= ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_TTL_MS_V1
    });
}

fn filter_eth_fullnode_native_recovery_inflight_headers_v1(
    chain_id: u64,
    peer_id: u64,
    kind: u8,
    pending_headers: Vec<EthFullnodeNativePendingBodyHeaderV1>,
) -> Vec<EthFullnodeNativePendingBodyHeaderV1> {
    if pending_headers.is_empty() {
        return pending_headers;
    }
    let now_ms = now_unix_ms();
    let mut inflight = eth_fullnode_native_recovery_inflight_v1()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    prune_eth_fullnode_native_recovery_inflight_locked_v1(&mut inflight, now_ms);
    pending_headers
        .into_iter()
        .filter(|pending| {
            inflight
                .get(&(chain_id, kind, pending.hash))
                .map_or(true, |entry| entry.peer_id == peer_id)
        })
        .collect()
}

fn mark_eth_fullnode_native_recovery_inflight_v1(
    chain_id: u64,
    peer_id: u64,
    request_id: u64,
    kind: u8,
    pending_headers: &[EthFullnodeNativePendingBodyHeaderV1],
) {
    if pending_headers.is_empty() {
        return;
    }
    let now_ms = now_unix_ms();
    let mut inflight = eth_fullnode_native_recovery_inflight_v1()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    prune_eth_fullnode_native_recovery_inflight_locked_v1(&mut inflight, now_ms);
    for pending in pending_headers {
        inflight.insert(
            (chain_id, kind, pending.hash),
            EthFullnodeNativeRecoveryInflightV1 {
                peer_id,
                request_id,
                observed_unix_ms: now_ms,
            },
        );
    }
}

fn clear_eth_fullnode_native_recovery_inflight_request_v1(
    chain_id: u64,
    peer_id: u64,
    request_id: u64,
    kind: u8,
) {
    let mut inflight = eth_fullnode_native_recovery_inflight_v1()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    inflight.retain(|(entry_chain_id, entry_kind, _), entry| {
        *entry_chain_id != chain_id
            || *entry_kind != kind
            || entry.peer_id != peer_id
            || entry.request_id != request_id
    });
}

fn clear_eth_fullnode_native_recovery_inflight_peer_v1(chain_id: u64, peer_id: u64) {
    let mut inflight = eth_fullnode_native_recovery_inflight_v1()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    inflight.retain(|(entry_chain_id, _, _), entry| {
        *entry_chain_id != chain_id || entry.peer_id != peer_id
    });
}

fn prune_eth_fullnode_native_header_inflight_locked_v1(
    inflight: &mut EthFullnodeNativeHeaderInflightMapV1,
    now_ms: u64,
) {
    inflight.retain(|_, entry| {
        now_ms.saturating_sub(entry.observed_unix_ms)
            <= ETH_FULLNODE_NATIVE_HEADER_INFLIGHT_TTL_MS_V1
    });
}

fn should_dispatch_eth_fullnode_native_header_request_v1(
    chain_id: u64,
    peer_id: u64,
    start_height: u64,
    skip: u64,
    reverse: bool,
) -> bool {
    let now_ms = now_unix_ms();
    let mut inflight = eth_fullnode_native_header_inflight_v1()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    prune_eth_fullnode_native_header_inflight_locked_v1(&mut inflight, now_ms);
    inflight
        .get(&(chain_id, start_height, skip, reverse))
        .map_or(true, |entry| entry.peer_id == peer_id)
}

fn mark_eth_fullnode_native_header_inflight_v1(
    chain_id: u64,
    peer_id: u64,
    request_id: u64,
    start_height: u64,
    skip: u64,
    reverse: bool,
) {
    let now_ms = now_unix_ms();
    let mut inflight = eth_fullnode_native_header_inflight_v1()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    prune_eth_fullnode_native_header_inflight_locked_v1(&mut inflight, now_ms);
    inflight.insert(
        (chain_id, start_height, skip, reverse),
        EthFullnodeNativeHeaderInflightV1 {
            peer_id,
            request_id,
            observed_unix_ms: now_ms,
        },
    );
}

fn clear_eth_fullnode_native_header_inflight_request_v1(
    chain_id: u64,
    peer_id: u64,
    request_id: u64,
) {
    let mut inflight = eth_fullnode_native_header_inflight_v1()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    inflight.retain(|(entry_chain_id, _, _, _), entry| {
        *entry_chain_id != chain_id || entry.peer_id != peer_id || entry.request_id != request_id
    });
}

fn clear_eth_fullnode_native_header_inflight_peer_v1(chain_id: u64, peer_id: u64) {
    let mut inflight = eth_fullnode_native_header_inflight_v1()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    inflight.retain(|(entry_chain_id, _, _, _), entry| {
        *entry_chain_id != chain_id || entry.peer_id != peer_id
    });
}

fn reconcile_eth_fullnode_native_rlpx_runtime_sessions_with_live_sessions_v1(
    chain_id: u64,
    peers: &[NodeId],
) {
    if peers.is_empty() {
        return;
    }
    let live_peer_ids = eth_fullnode_native_rlpx_live_peer_ids_v1(chain_id);
    for peer in peers {
        if has_network_runtime_eth_peer_session(chain_id, peer.0)
            && !live_peer_ids.contains(&peer.0)
        {
            mark_network_runtime_eth_peer_session_closed_v1(chain_id, peer.0);
        }
    }
}

static ETH_FULLNODE_NATIVE_RLPX_REQUEST_ID: OnceLock<std::sync::atomic::AtomicU64> =
    OnceLock::new();
const ETH_FULLNODE_NATIVE_SNAP_TRIE_NODE_RETRY_LIMIT_V1: u8 = 2;
fn next_eth_fullnode_native_rlpx_request_id_v1() -> u64 {
    ETH_FULLNODE_NATIVE_RLPX_REQUEST_ID
        .get_or_init(|| std::sync::atomic::AtomicU64::new(1))
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .max(1)
}

fn connect_eth_fullnode_native_rlpx_addr_v1(
    addr_hint: &str,
    timeout: Duration,
) -> Result<TcpStream, NetworkError> {
    let mut last_err = None;
    for addr in addr_hint
        .to_socket_addrs()
        .map_err(|e| NetworkError::AddressParse(format!("{addr_hint}:{e}")))?
    {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => return Ok(stream),
            Err(err) => {
                last_err = Some(err);
            }
        }
    }
    Err(NetworkError::Io(
        last_err
            .map(|err| format!("connect_failed({addr_hint}):{err}"))
            .unwrap_or_else(|| format!("connect_failed({addr_hint}):no_resolved_addr")),
    ))
}

fn eth_fullnode_native_rlpx_connect_timeout_v1() -> Duration {
    let timeout_ms = std::env::var("NOVOVM_ETH_RLPX_CONNECT_TIMEOUT_MS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(750)
        .clamp(250, 5_000);
    Duration::from_millis(timeout_ms)
}

fn eth_fullnode_native_rlpx_bootstrap_tick_budget_v1() -> Duration {
    let timeout_ms = std::env::var("NOVOVM_ETH_RLPX_BOOTSTRAP_TICK_BUDGET_MS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(12_000)
        .clamp(1_000, 60_000);
    Duration::from_millis(timeout_ms)
}

fn evm_native_header_wire_from_rlpx_header_v1(
    header: &crate::EthRlpxBlockHeaderRecordV1,
) -> EvmNativeBlockHeaderWireV1 {
    EvmNativeBlockHeaderWireV1 {
        number: header.number,
        hash: header.hash,
        parent_hash: header.parent_hash,
        state_root: header.state_root,
        transactions_root: header.transactions_root,
        receipts_root: header.receipts_root,
        ommers_hash: header.ommers_hash,
        logs_bloom: header.logs_bloom.clone(),
        gas_limit: header.gas_limit,
        gas_used: header.gas_used,
        timestamp: header.timestamp,
        base_fee_per_gas: header.base_fee_per_gas,
        withdrawals_root: header.withdrawals_root,
        blob_gas_used: header.blob_gas_used,
        excess_blob_gas: header.excess_blob_gas,
        block_access_list_hash: header.block_access_list_hash,
    }
}

fn evm_native_body_wire_from_rlpx_body_v1(
    number: u64,
    block_hash: [u8; 32],
    body: &crate::EthRlpxBlockBodyRecordV1,
) -> EvmNativeBlockBodyWireV1 {
    EvmNativeBlockBodyWireV1 {
        number,
        block_hash,
        tx_hashes: body.tx_hashes.clone(),
        raw_tx_rlps: body.tx_rlp_items.clone(),
        ommer_hashes: body.ommer_hashes.clone(),
        withdrawal_rlp_items: body.withdrawal_rlp_items.clone(),
        withdrawal_count: body.withdrawal_count,
        body_available: body.body_available,
        txs_materialized: body.txs_materialized,
    }
}

#[derive(Default)]
struct EthFullnodeNativeRlpxPeerTickReportV1 {
    status_updates: usize,
    header_updates: usize,
    body_updates: usize,
    receipt_updates: usize,
    sync_requests: usize,
    inbound_frames: usize,
}

struct EthFullnodeNativeBootstrapPeerDriveResultV1 {
    peer: NodeId,
    endpoint: Option<PluginPeerEndpoint>,
    connected: bool,
    outcome: Result<EthFullnodeNativeRlpxPeerTickReportV1, NetworkError>,
}

fn absorb_eth_fullnode_native_rlpx_peer_tick_report_v1(
    report: &mut EthFullnodeNativeRealDriveReportV1,
    peer: NodeId,
    peer_report: EthFullnodeNativeRlpxPeerTickReportV1,
) {
    report.status_updates = report
        .status_updates
        .saturating_add(peer_report.status_updates);
    report.header_updates = report
        .header_updates
        .saturating_add(peer_report.header_updates);
    report.body_updates = report.body_updates.saturating_add(peer_report.body_updates);
    report.receipt_updates = report
        .receipt_updates
        .saturating_add(peer_report.receipt_updates);
    report.sync_requests = report
        .sync_requests
        .saturating_add(peer_report.sync_requests);
    report.inbound_frames = report
        .inbound_frames
        .saturating_add(peer_report.inbound_frames);
    if peer_report.header_updates > 0 {
        report.header_updated_peer_ids.push(peer.0);
    }
    if peer_report.body_updates > 0 {
        report.body_updated_peer_ids.push(peer.0);
    }
    if peer_report.receipt_updates > 0 {
        report.receipt_updated_peer_ids.push(peer.0);
    }
}

fn eth_fullnode_native_material_success_peer_ids_v1(
    report: &EthFullnodeNativeRealDriveReportV1,
) -> Vec<u64> {
    let mut peers = Vec::with_capacity(
        report
            .body_updated_peer_ids
            .len()
            .saturating_add(report.receipt_updated_peer_ids.len()),
    );
    for peer_id in report
        .body_updated_peer_ids
        .iter()
        .chain(report.receipt_updated_peer_ids.iter())
    {
        if !peers.contains(peer_id) {
            peers.push(*peer_id);
        }
    }
    peers
}

fn drive_eth_fullnode_native_rlpx_bootstrap_peer_once_v1(
    chain_id: u64,
    local_node: NodeId,
    peer: NodeId,
    endpoint: PluginPeerEndpoint,
    budget_hooks: EthFullnodeBudgetHooksV1,
) -> EthFullnodeNativeBootstrapPeerDriveResultV1 {
    match connect_eth_fullnode_native_rlpx_peer_v1(chain_id, local_node, peer, &endpoint) {
        Ok(()) => EthFullnodeNativeBootstrapPeerDriveResultV1 {
            peer,
            endpoint: Some(endpoint.clone()),
            connected: true,
            outcome: drive_eth_fullnode_native_rlpx_peer_session_once_v1(
                chain_id,
                local_node,
                peer,
                &endpoint,
                &budget_hooks,
            ),
        },
        Err(err) => EthFullnodeNativeBootstrapPeerDriveResultV1 {
            peer,
            endpoint: Some(endpoint),
            connected: false,
            outcome: Err(err),
        },
    }
}

fn format_eth_fullnode_rlpx_disconnect_reason_v1(payload: &[u8], phase: &str) -> String {
    let reason = eth_rlpx_parse_disconnect_reason_v1(payload);
    format!(
        "rlpx_remote_disconnected_{phase}:reason_code={} reason={}",
        reason.unwrap_or(u64::MAX),
        eth_rlpx_disconnect_reason_name_v1(reason.unwrap_or(u64::MAX)),
    )
}

fn hex32_v1(bytes: &[u8; 32]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn hex_dynamic_v1(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "empty".to_string();
    }
    let body = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("0x{body}")
}

fn eth_fullnode_rlpx_error_is_timeout_v1(raw: &str) -> bool {
    raw.contains("timed out")
        || raw.contains("would block")
        || raw.contains("partial_read_timeout")
        || raw.contains("os error 10060")
        || raw.contains("os error 10035")
        || raw.contains("没有正确答复")
        || raw.contains("没有反应")
}

fn eth_fullnode_rlpx_error_is_remote_closed_v1(raw: &str) -> bool {
    if eth_fullnode_rlpx_error_is_timeout_v1(raw) {
        return false;
    }
    raw.contains("rlpx_remote_disconnected_")
        || raw.contains("eof read=0")
        || raw.contains("failed:eof")
        || raw.contains("read=0/")
        || raw.contains("os error 10053")
        || raw.contains("os error 10054")
        || raw.contains("远程主机强迫关闭")
}

fn eth_fullnode_rlpx_error_is_session_desync_v1(raw: &str) -> bool {
    raw.contains("rlpx_frame_header_mac_mismatch") || raw.contains("rlpx_frame_mac_mismatch")
}

fn eth_fullnode_rlpx_error_disconnect_reason_code_v1(raw: &str) -> Option<u64> {
    let marker = "reason_code=";
    let start = raw.find(marker)?.saturating_add(marker.len());
    let code = raw[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if code.is_empty() {
        return None;
    }
    code.parse::<u64>()
        .ok()
        .filter(|parsed| *parsed != u64::MAX)
}

fn observe_eth_fullnode_rlpx_request_write_error_v1(
    chain_id: u64,
    peer_id: u64,
    failure_reason: &'static str,
    err: &str,
) {
    if eth_fullnode_rlpx_error_is_timeout_v1(err) {
        observe_network_runtime_eth_peer_timeout_v1(chain_id, peer_id, failure_reason);
    } else if eth_fullnode_rlpx_error_is_remote_closed_v1(err)
        || eth_fullnode_rlpx_error_is_session_desync_v1(err)
    {
        observe_network_runtime_eth_peer_disconnect_v1(
            chain_id,
            peer_id,
            eth_fullnode_rlpx_error_disconnect_reason_code_v1(err),
        );
    } else {
        observe_network_runtime_eth_peer_handshake_failure_v1(chain_id, peer_id, failure_reason);
    }
}

fn observe_eth_fullnode_connect_error_v1(chain_id: u64, peer_id: u64, err: &NetworkError) {
    match err {
        NetworkError::AddressParse(_) => observe_network_runtime_eth_peer_connect_failure_v1(
            chain_id,
            peer_id,
            "address_parse",
            true,
        ),
        NetworkError::Io(raw) if eth_fullnode_rlpx_error_is_timeout_v1(raw) => {
            observe_network_runtime_eth_peer_timeout_v1(chain_id, peer_id, "connect_timeout");
        }
        NetworkError::Io(_) => {
            observe_network_runtime_eth_peer_connect_failure_v1(
                chain_id,
                peer_id,
                "connect_failed",
                false,
            );
        }
        NetworkError::Decode(raw) if eth_fullnode_rlpx_error_is_timeout_v1(raw) => {
            observe_network_runtime_eth_peer_timeout_v1(chain_id, peer_id, "connect_timeout");
        }
        NetworkError::Decode(raw) => {
            observe_network_runtime_eth_peer_decode_failure_v1(chain_id, peer_id, raw.as_str());
        }
        _ => observe_network_runtime_eth_peer_connect_failure_v1(
            chain_id,
            peer_id,
            "connect_failed",
            false,
        ),
    }
}

fn classify_eth_fullnode_peer_failure_v1(
    err: &NetworkError,
) -> EthFullnodeNativePeerFailureClassV1 {
    match err {
        NetworkError::PeerNotFound(_) => EthFullnodeNativePeerFailureClassV1::PeerNotFound,
        NetworkError::QueueFull => EthFullnodeNativePeerFailureClassV1::QueueFull,
        NetworkError::LocalNodeMismatch { .. } => {
            EthFullnodeNativePeerFailureClassV1::LocalNodeMismatch
        }
        NetworkError::AddressParse(_) => EthFullnodeNativePeerFailureClassV1::AddressParse,
        NetworkError::Io(_) => EthFullnodeNativePeerFailureClassV1::Io,
        NetworkError::Encode(_) => EthFullnodeNativePeerFailureClassV1::Encode,
        NetworkError::Decode(raw) if eth_fullnode_rlpx_error_is_timeout_v1(raw) => {
            EthFullnodeNativePeerFailureClassV1::Timeout
        }
        NetworkError::Decode(raw) if eth_fullnode_rlpx_error_is_session_desync_v1(raw) => {
            EthFullnodeNativePeerFailureClassV1::Io
        }
        NetworkError::Decode(_) => EthFullnodeNativePeerFailureClassV1::Decode,
    }
}

fn build_eth_fullnode_peer_failure_report_v1(
    chain_id: u64,
    peer: NodeId,
    endpoint: Option<&PluginPeerEndpoint>,
    phase: EthFullnodeNativePeerDrivePhaseV1,
    err: &NetworkError,
) -> EthFullnodeNativePeerFailureV1 {
    let session = snapshot_network_runtime_eth_peer_sessions_for_peers_v1(chain_id, &[peer])
        .into_iter()
        .next();
    EthFullnodeNativePeerFailureV1 {
        peer_id: peer.0,
        endpoint: endpoint.map(|endpoint| endpoint.addr_hint.clone()),
        phase,
        class: classify_eth_fullnode_peer_failure_v1(err),
        lifecycle_class: session.as_ref().and_then(|value| value.last_failure_class),
        reason_code: session
            .as_ref()
            .and_then(|value| value.last_failure_reason_code),
        reason_name: session
            .as_ref()
            .and_then(|value| value.last_failure_reason_name.clone()),
        error: err.to_string(),
    }
}

fn push_eth_fullnode_native_runtime_detail_peer_v1(
    peers: &mut Vec<NodeId>,
    seen: &mut HashSet<u64>,
    peer: NodeId,
) {
    if peers.len() < ETH_FULLNODE_NATIVE_RUNTIME_PEER_DETAIL_LIMIT_V1 && seen.insert(peer.0) {
        peers.push(peer);
    }
}

fn eth_fullnode_native_runtime_detail_peers_v1(
    plan: &EthFullnodeNativePeerWorkerPlanV1,
    report: &EthFullnodeNativeRealDriveReportV1,
) -> Vec<NodeId> {
    let mut peers = Vec::new();
    let mut seen = HashSet::new();
    for &peer in plan.bootstrap_peers.iter().chain(plan.sync_peers.iter()) {
        push_eth_fullnode_native_runtime_detail_peer_v1(&mut peers, &mut seen, peer);
    }
    for peer_id in report
        .header_updated_peer_ids
        .iter()
        .chain(report.body_updated_peer_ids.iter())
        .chain(report.receipt_updated_peer_ids.iter())
    {
        push_eth_fullnode_native_runtime_detail_peer_v1(&mut peers, &mut seen, NodeId(*peer_id));
    }
    for failure in &report.peer_failures {
        push_eth_fullnode_native_runtime_detail_peer_v1(
            &mut peers,
            &mut seen,
            NodeId(failure.peer_id),
        );
    }
    for &peer in &plan.candidate_peers {
        push_eth_fullnode_native_runtime_detail_peer_v1(&mut peers, &mut seen, peer);
        if peers.len() >= ETH_FULLNODE_NATIVE_RUNTIME_PEER_DETAIL_LIMIT_V1 {
            break;
        }
    }
    peers
}

fn build_eth_fullnode_native_worker_runtime_snapshot_v1(
    plan: &EthFullnodeNativePeerWorkerPlanV1,
    report: &EthFullnodeNativeRealDriveReportV1,
) -> EthFullnodeNativeWorkerRuntimeSnapshotV1 {
    let mut runtime_config = resolve_eth_fullnode_native_runtime_config_v1(plan.chain_id);
    runtime_config.budget_hooks = plan.budget_hooks.clone();
    let (peer_selection_scores, selection_quality_summary, selection_long_term_summary) =
        snapshot_eth_fullnode_peer_selection_scores_v1(
            plan.chain_id,
            &plan.candidate_peers,
            &plan.bootstrap_peers,
            &plan.sync_peers,
        );
    let runtime_detail_peers = eth_fullnode_native_runtime_detail_peers_v1(plan, report);
    let runtime_detail_peer_ids = runtime_detail_peers
        .iter()
        .map(|peer| peer.0)
        .collect::<HashSet<_>>();
    let peer_selection_scores = peer_selection_scores
        .into_iter()
        .filter(|score| runtime_detail_peer_ids.contains(&score.peer_id))
        .collect::<Vec<_>>();
    let native_head_block = snapshot_eth_fullnode_native_head_block_object_v1(plan.chain_id);
    let native_canonical_chain = snapshot_network_runtime_native_canonical_chain_v1(plan.chain_id);
    let native_canonical_blocks = snapshot_network_runtime_native_canonical_blocks_v1(
        plan.chain_id,
        plan.budget_hooks.runtime_block_snapshot_limit.max(1) as usize,
    );
    let native_pending_tx_summary =
        snapshot_network_runtime_native_pending_tx_summary_v1(plan.chain_id);
    let native_pending_tx_broadcast_runtime =
        snapshot_network_runtime_native_pending_tx_broadcast_runtime_summary_v1(plan.chain_id);
    let native_execution_budget_runtime =
        snapshot_network_runtime_native_execution_budget_runtime_summary_v1(plan.chain_id);
    let native_pending_txs = snapshot_network_runtime_native_pending_txs_v1(
        plan.chain_id,
        plan.budget_hooks.runtime_pending_tx_snapshot_limit.max(1) as usize,
    );
    let runtime_sync = get_network_runtime_sync_status(plan.chain_id);
    let runtime_native_sync = get_network_runtime_native_sync_status(plan.chain_id);
    let head_view = derive_eth_fullnode_head_view_with_native_preference_v1(
        None,
        native_head_block.as_ref(),
        native_canonical_chain.as_ref(),
        runtime_native_sync,
    );
    let sync_view = derive_eth_fullnode_sync_view_with_native_preference_v1(
        None,
        native_head_block.as_ref(),
        native_canonical_chain.as_ref(),
        runtime_sync,
        runtime_native_sync,
    );
    let peer_sessions = snapshot_network_runtime_eth_peer_sessions_for_peers_v1(
        plan.chain_id,
        &runtime_detail_peers,
    );
    let peer_failures = report
        .peer_failures
        .iter()
        .map(|failure| EthFullnodeNativePeerFailureSnapshotV1 {
            peer_id: failure.peer_id,
            endpoint: failure.endpoint.clone(),
            phase: failure.phase.as_str().to_string(),
            class: failure.class.as_str().to_string(),
            lifecycle_class: failure
                .lifecycle_class
                .map(|value| value.as_str().to_string()),
            reason_code: failure.reason_code,
            reason_name: failure.reason_name.clone(),
            error: failure.error.clone(),
        })
        .collect::<Vec<_>>();
    EthFullnodeNativeWorkerRuntimeSnapshotV1 {
        schema: ETH_FULLNODE_NATIVE_WORKER_RUNTIME_SCHEMA_V1.to_string(),
        chain_id: plan.chain_id,
        updated_at_unix_ms: now_unix_ms(),
        candidate_peer_ids: plan.candidate_peers.iter().map(|peer| peer.0).collect(),
        scheduled_bootstrap_peers: report.scheduled_bootstrap_peers as u64,
        scheduled_sync_peers: report.scheduled_sync_peers as u64,
        attempted_bootstrap_peers: report.attempted_bootstrap_peers as u64,
        attempted_sync_peers: report.attempted_sync_peers as u64,
        failed_bootstrap_peers: report.failed_bootstrap_peers as u64,
        failed_sync_peers: report.failed_sync_peers as u64,
        skipped_missing_endpoint_peers: report.skipped_missing_endpoint_peers as u64,
        skipped_bootstrap_budget_peers: report.skipped_bootstrap_budget_peers as u64,
        skipped_sync_budget_peers: report.skipped_sync_budget_peers as u64,
        connected_peers: report.connected_peers as u64,
        ready_peers: report.ready_peers as u64,
        status_updates: report.status_updates as u64,
        header_updates: report.header_updates as u64,
        body_updates: report.body_updates as u64,
        sync_requests: report.sync_requests as u64,
        inbound_frames: report.inbound_frames as u64,
        head_view,
        sync_view,
        native_canonical_chain,
        native_canonical_blocks,
        native_pending_tx_summary,
        native_pending_tx_broadcast_runtime,
        native_execution_budget_runtime,
        native_pending_txs,
        native_head_body_available: native_head_block.as_ref().map(|block| block.body.is_some()),
        native_head_canonical: native_head_block.as_ref().map(|block| block.canonical),
        native_head_safe: native_head_block.as_ref().map(|block| block.safe),
        native_head_finalized: native_head_block.as_ref().map(|block| block.finalized),
        lifecycle_summary: report.lifecycle_summary.clone(),
        selection_quality_summary,
        selection_long_term_summary,
        selection_window_policy: runtime_config.selection_window_policy.clone(),
        runtime_config,
        peer_selection_scores,
        peer_sessions,
        peer_failures,
    }
}

fn eth_fullnode_peer_validation_disconnect_reason_code_v1(
    _reason: EthChainConfigPeerValidationReasonV1,
) -> u64 {
    0x03
}

fn format_eth_fullnode_peer_validation_error_v1(
    reason: EthChainConfigPeerValidationReasonV1,
    local_status: &EthRlpxStatusV1,
    remote_status: &EthRlpxStatusV1,
) -> String {
    let hex32 = |bytes: &[u8; 32]| -> String {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    let hex4 = |bytes: &[u8; 4]| -> String {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    format!(
        "rlpx_remote_status_rejected:reason={} local_network_id={} remote_network_id={} local_genesis=0x{} remote_genesis=0x{} local_fork=0x{}:{} remote_fork=0x{}:{}",
        reason.as_str(),
        local_status.network_id,
        remote_status.network_id,
        hex32(&local_status.genesis_hash),
        hex32(&remote_status.genesis_hash),
        hex4(&local_status.fork_id.hash),
        local_status.fork_id.next,
        hex4(&remote_status.fork_id.hash),
        remote_status.fork_id.next,
    )
}

fn eth_fullnode_native_head_time_for_validation_v1(chain_id: u64) -> u64 {
    get_network_runtime_native_header_snapshot_v1(chain_id)
        .and_then(|snapshot| snapshot.timestamp)
        .unwrap_or(0)
}

fn connect_eth_fullnode_native_rlpx_peer_v1(
    chain_id: u64,
    local_node: NodeId,
    peer: NodeId,
    endpoint: &PluginPeerEndpoint,
) -> Result<(), NetworkError> {
    let key = (chain_id, peer.0);
    observe_network_runtime_eth_peer_discovered_v1(chain_id, peer.0);
    {
        let sessions = eth_fullnode_native_rlpx_sessions_v1()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if sessions.contains_key(&key) {
            return Ok(());
        }
    }

    let timeout = eth_fullnode_native_rlpx_connect_timeout_v1();
    observe_network_runtime_eth_peer_connecting_v1(chain_id, peer.0);
    let mut stream = connect_eth_fullnode_native_rlpx_addr_v1(endpoint.addr_hint.as_str(), timeout)
        .inspect_err(|err| {
            observe_eth_fullnode_connect_error_v1(chain_id, peer.0, err);
        })?;
    observe_network_runtime_eth_peer_connected_v1(chain_id, peer.0);
    stream.set_read_timeout(Some(timeout)).map_err(|e| {
        let err = NetworkError::Io(format!(
            "set_read_timeout_failed:{}:{e}",
            endpoint.addr_hint
        ));
        observe_network_runtime_eth_peer_connect_failure_v1(
            chain_id,
            peer.0,
            "set_read_timeout_failed",
            false,
        );
        err
    })?;
    stream.set_write_timeout(Some(timeout)).map_err(|e| {
        let err = NetworkError::Io(format!(
            "set_write_timeout_failed:{}:{e}",
            endpoint.addr_hint
        ));
        observe_network_runtime_eth_peer_connect_failure_v1(
            chain_id,
            peer.0,
            "set_write_timeout_failed",
            false,
        );
        err
    })?;

    let hello_profile = eth_rlpx_hello_profile_v1();
    observe_eth_native_discovery(chain_id);
    observe_eth_native_rlpx_auth(chain_id);
    let mut handshake = eth_rlpx_handshake_initiator_v1(endpoint.endpoint.as_str(), &mut stream)
        .map_err(|err| {
            let err = format!(
                "{err}:endpoint={} hello_profile={}",
                endpoint.addr_hint, hello_profile
            );
            if eth_fullnode_rlpx_error_is_remote_closed_v1(err.as_str()) {
                observe_network_runtime_eth_peer_disconnect_v1(chain_id, peer.0, None);
                NetworkError::Io(format!(
                    "rlpx_remote_closed_before_auth:endpoint={}:{}",
                    endpoint.addr_hint, err
                ))
            } else if eth_fullnode_rlpx_error_is_timeout_v1(err.as_str()) {
                observe_network_runtime_eth_peer_timeout_v1(chain_id, peer.0, "auth_timeout");
                NetworkError::Decode(err)
            } else {
                observe_network_runtime_eth_peer_handshake_failure_v1(
                    chain_id,
                    peer.0,
                    "rlpx_auth_failed",
                );
                NetworkError::Decode(err)
            }
        })?;
    observe_eth_native_rlpx_auth_ack(chain_id);

    let local_caps = default_eth_rlpx_capabilities_v1();
    let local_client_name = eth_rlpx_default_client_name_v1();
    let hello_payload = eth_rlpx_build_hello_payload_v1(
        &handshake.local_static_pub,
        local_caps.as_slice(),
        local_client_name.as_str(),
        eth_rlpx_default_listen_port_v1(),
    );
    eth_rlpx_write_wire_frame_v1(
        &mut stream,
        &mut handshake.session,
        ETH_RLPX_P2P_HELLO_MSG,
        hello_payload.as_slice(),
    )
    .map_err(NetworkError::Io)?;

    let remote_hello = loop {
        let (code, payload) = eth_rlpx_read_wire_frame_v1(&mut stream, &mut handshake.session)
            .map_err(|err| {
                if eth_fullnode_rlpx_error_is_remote_closed_v1(err.as_str()) {
                    observe_network_runtime_eth_peer_disconnect_v1(chain_id, peer.0, None);
                    return NetworkError::Io(format!(
                        "rlpx_remote_closed_before_hello:endpoint={}:{}",
                        endpoint.addr_hint, err
                    ));
                }
                if eth_fullnode_rlpx_error_is_timeout_v1(err.as_str()) {
                    observe_network_runtime_eth_peer_timeout_v1(chain_id, peer.0, "hello_timeout");
                } else {
                    observe_network_runtime_eth_peer_decode_failure_v1(
                        chain_id,
                        peer.0,
                        "hello_frame_decode_failed",
                    );
                }
                NetworkError::Decode(err)
            })?;
        if code == ETH_RLPX_P2P_HELLO_MSG {
            break eth_rlpx_parse_hello_payload_v1(payload.as_slice()).map_err(|err| {
                observe_network_runtime_eth_peer_decode_failure_v1(
                    chain_id,
                    peer.0,
                    "hello_payload_decode_failed",
                );
                NetworkError::Decode(err)
            })?;
        }
        if code == ETH_RLPX_P2P_PING_MSG {
            eth_rlpx_write_wire_frame_v1(
                &mut stream,
                &mut handshake.session,
                ETH_RLPX_P2P_PONG_MSG,
                &[],
            )
            .map_err(NetworkError::Io)?;
            continue;
        }
        if code == ETH_RLPX_P2P_DISCONNECT_MSG {
            observe_network_runtime_eth_peer_disconnect_v1(
                chain_id,
                peer.0,
                eth_rlpx_parse_disconnect_reason_v1(payload.as_slice()),
            );
            return Err(NetworkError::Io(
                format_eth_fullnode_rlpx_disconnect_reason_v1(payload.as_slice(), "before_hello"),
            ));
        }
    };
    observe_eth_native_hello(chain_id);
    observe_network_runtime_eth_peer_hello_ok_v1(chain_id, peer.0);
    if remote_hello.protocol_version >= 5 {
        handshake.session.set_snappy(true);
    }
    let negotiated_eth_version = eth_rlpx_select_shared_eth_version_v1(
        local_caps.as_slice(),
        remote_hello.capabilities.as_slice(),
    )
    .ok_or_else(|| {
        let reason = format!(
            "rlpx_eth_capability_not_found:local_caps={} remote_caps={} endpoint={} hello_profile={}",
            local_caps
                .iter()
                .map(|cap| format!("{}/{}", cap.name, cap.version))
                .collect::<Vec<_>>()
                .join(","),
            remote_hello
                .capabilities
                .iter()
                .map(|cap| format!("{}/{}", cap.name, cap.version))
                .collect::<Vec<_>>()
                .join(","),
            endpoint.addr_hint,
            hello_profile,
        );
        observe_network_runtime_eth_peer_decode_failure_v1(chain_id, peer.0, reason.as_str());
        NetworkError::Decode(reason)
    })?;
    let negotiated_snap = eth_rlpx_select_shared_snap_version_v1(
        local_caps.as_slice(),
        remote_hello.capabilities.as_slice(),
    );
    let remote_eth_versions = remote_hello
        .capabilities
        .iter()
        .filter(|cap| cap.name.eq_ignore_ascii_case("eth"))
        .map(|cap| cap.version as u8)
        .collect::<Vec<_>>();
    let remote_snap_versions = remote_hello
        .capabilities
        .iter()
        .filter(|cap| cap.name.eq_ignore_ascii_case("snap"))
        .map(|cap| cap.version as u8)
        .collect::<Vec<_>>();

    let eth_offset = ETH_RLPX_BASE_PROTOCOL_OFFSET;
    let remote_status_payload = loop {
        let (code, payload) = eth_rlpx_read_wire_frame_v1(&mut stream, &mut handshake.session)
            .map_err(|err| {
                if eth_fullnode_rlpx_error_is_remote_closed_v1(err.as_str()) {
                    observe_network_runtime_eth_peer_disconnect_v1(chain_id, peer.0, None);
                    return NetworkError::Io(format!(
                        "rlpx_remote_closed_before_status:endpoint={}:{}",
                        endpoint.addr_hint, err
                    ));
                }
                if eth_fullnode_rlpx_error_is_timeout_v1(err.as_str()) {
                    observe_network_runtime_eth_peer_timeout_v1(chain_id, peer.0, "status_timeout");
                } else {
                    observe_network_runtime_eth_peer_decode_failure_v1(
                        chain_id,
                        peer.0,
                        "status_frame_decode_failed",
                    );
                }
                NetworkError::Decode(err)
            })?;
        if code == eth_offset + ETH_RLPX_ETH_STATUS_MSG {
            break payload;
        }
        if code == ETH_RLPX_P2P_PING_MSG {
            eth_rlpx_write_wire_frame_v1(
                &mut stream,
                &mut handshake.session,
                ETH_RLPX_P2P_PONG_MSG,
                &[],
            )
            .map_err(NetworkError::Io)?;
            continue;
        }
        if code == ETH_RLPX_P2P_DISCONNECT_MSG {
            observe_network_runtime_eth_peer_disconnect_v1(
                chain_id,
                peer.0,
                eth_rlpx_parse_disconnect_reason_v1(payload.as_slice()),
            );
            return Err(NetworkError::Io(
                format_eth_fullnode_rlpx_disconnect_reason_v1(payload.as_slice(), "before_status"),
            ));
        }
    };
    let remote_status = eth_rlpx_parse_status_payload_v1(remote_status_payload.as_slice())
        .map_err(|err| {
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                peer.0,
                "status_payload_decode_failed",
            );
            NetworkError::Decode(err)
        })?;
    let local_status =
        build_eth_fullnode_native_rlpx_status_v1(chain_id, negotiated_eth_version.as_u8() as u32);
    if let Err(reason) = validate_eth_chain_config_peer_status_v1(
        &resolve_eth_chain_config_v1(chain_id),
        local_status.latest_block,
        eth_fullnode_native_head_time_for_validation_v1(chain_id),
        &remote_status,
    ) {
        observe_network_runtime_eth_peer_validation_reject_v1(chain_id, peer.0, reason);
        let disconnect_payload = eth_rlpx_build_disconnect_payload_v1(
            eth_fullnode_peer_validation_disconnect_reason_code_v1(reason),
        );
        let _ = eth_rlpx_write_wire_frame_v1(
            &mut stream,
            &mut handshake.session,
            ETH_RLPX_P2P_DISCONNECT_MSG,
            disconnect_payload.as_slice(),
        );
        return Err(NetworkError::Decode(
            format_eth_fullnode_peer_validation_error_v1(reason, &local_status, &remote_status),
        ));
    }
    observe_network_runtime_eth_peer_status_ok_v1(
        chain_id,
        peer.0,
        Some(remote_status.latest_block),
    );
    let local_status_payload = eth_rlpx_build_status_payload_v1(local_status);
    eth_rlpx_write_wire_frame_v1(
        &mut stream,
        &mut handshake.session,
        eth_offset + ETH_RLPX_ETH_STATUS_MSG,
        local_status_payload.as_slice(),
    )
    .map_err(|err| {
        observe_network_runtime_eth_peer_handshake_failure_v1(
            chain_id,
            peer.0,
            "status_write_failed",
        );
        NetworkError::Io(err)
    })?;

    let _ = register_network_runtime_peer(chain_id, peer.0);
    observe_eth_native_status(chain_id);
    let _ = observe_network_runtime_peer_head(chain_id, peer.0, remote_status.latest_block);
    observe_network_runtime_eth_peer_head(chain_id, peer.0, remote_status.latest_block);
    let _ = upsert_network_runtime_eth_peer_session(
        chain_id,
        peer.0,
        remote_eth_versions.as_slice(),
        remote_snap_versions.as_slice(),
        Some(remote_status.latest_block),
    );
    eth_fullnode_native_rlpx_sessions_v1()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(
            key,
            EthFullnodeNativeRlpxLivePeerSessionV1 {
                endpoint: endpoint.clone(),
                stream,
                frame_session: handshake.session,
                _negotiated_eth_version: negotiated_eth_version.as_u8(),
                _negotiated_snap_version: negotiated_snap.map(|version| version.as_u8()),
                remote_status,
                last_sync_request_unix_ms: 0,
                last_headers_request_id: None,
                pending_headers_request: None,
                last_bodies_request_id: None,
                last_receipts_request_id: None,
                last_snap_account_range_request_id: None,
                last_snap_storage_ranges_request_id: None,
                last_snap_byte_codes_request_id: None,
                last_snap_trie_nodes_request_id: None,
                last_snap_state_root: None,
                last_snap_account_origin: None,
                last_snap_account_limit: None,
                pending_snap_next_account_origin: None,
                pending_snap_storage_accounts: Vec::new(),
                pending_snap_storage_origin: Vec::new(),
                pending_snap_storage_limit: Vec::new(),
                pending_snap_storage_deferred_accounts: Vec::new(),
                pending_snap_code_hashes: Vec::new(),
                pending_snap_trie_node_pathsets: Vec::new(),
                pending_snap_trie_node_hashes: Vec::new(),
                pending_snap_trie_node_retry_count: 0,
                last_block_access_lists_request_id: None,
                queued_block_access_lists: Vec::new(),
                pending_block_access_lists: Vec::new(),
                last_pooled_transactions_request_id: None,
                last_tx_broadcast_unix_ms: 0,
                pending_body_headers: Vec::new(),
                pending_body_request_offset: 0,
                pending_receipt_request_offset: 0,
                pending_pooled_transaction_hashes: Vec::new(),
            },
        );
    let _ = local_node;
    Ok(())
}

fn mark_eth_fullnode_native_rlpx_session_disconnected_v1(
    chain_id: u64,
    peer_id: u64,
    disconnected: &mut bool,
    disconnect_error: &mut Option<NetworkError>,
    err: NetworkError,
) {
    clear_eth_fullnode_native_recovery_inflight_peer_v1(chain_id, peer_id);
    clear_eth_fullnode_native_header_inflight_peer_v1(chain_id, peer_id);
    let _ = unregister_network_runtime_peer(chain_id, peer_id);
    *disconnected = true;
    *disconnect_error = Some(err);
}

fn drive_eth_fullnode_native_rlpx_peer_session_once_v1(
    chain_id: u64,
    local_node: NodeId,
    peer: NodeId,
    endpoint: &PluginPeerEndpoint,
    budget_hooks: &EthFullnodeBudgetHooksV1,
) -> Result<EthFullnodeNativeRlpxPeerTickReportV1, NetworkError> {
    connect_eth_fullnode_native_rlpx_peer_v1(chain_id, local_node, peer, endpoint)?;
    let mut sessions = eth_fullnode_native_rlpx_sessions_v1()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut report = EthFullnodeNativeRlpxPeerTickReportV1::default();
    let mut disconnected = false;
    let mut disconnect_error = None::<NetworkError>;
    {
        let Some(session) = sessions.get_mut(&(chain_id, peer.0)) else {
            return Ok(report);
        };
        session
            .stream
            .set_read_timeout(Some(Duration::from_millis(150)))
            .map_err(|e| {
                NetworkError::Io(format!(
                    "set_session_read_timeout_failed:{}:{e}",
                    session.endpoint.addr_hint
                ))
            })?;
        let _ =
            observe_network_runtime_peer_head(chain_id, peer.0, session.remote_status.latest_block);

        loop {
            match eth_rlpx_read_wire_frame_v1(&mut session.stream, &mut session.frame_session) {
                Ok((code, payload)) => {
                    report.inbound_frames = report.inbound_frames.saturating_add(1);
                    if code == ETH_RLPX_P2P_PING_MSG {
                        if let Err(err) = eth_rlpx_write_wire_frame_v1(
                            &mut session.stream,
                            &mut session.frame_session,
                            ETH_RLPX_P2P_PONG_MSG,
                            &[],
                        )
                        .map_err(NetworkError::Io)
                        {
                            mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                chain_id,
                                peer.0,
                                &mut disconnected,
                                &mut disconnect_error,
                                err,
                            );
                            break;
                        }
                        continue;
                    }
                    if code == ETH_RLPX_P2P_DISCONNECT_MSG {
                        observe_network_runtime_eth_peer_disconnect_v1(
                            chain_id,
                            peer.0,
                            eth_rlpx_parse_disconnect_reason_v1(payload.as_slice()),
                        );
                        let _ = unregister_network_runtime_peer(chain_id, peer.0);
                        disconnected = true;
                        disconnect_error = Some(NetworkError::Io(
                            format_eth_fullnode_rlpx_disconnect_reason_v1(
                                payload.as_slice(),
                                "ingest",
                            ),
                        ));
                        break;
                    }
                    let eth_offset = ETH_RLPX_BASE_PROTOCOL_OFFSET;
                    if code == eth_offset + ETH_RLPX_ETH_STATUS_MSG {
                        let status = eth_rlpx_parse_status_payload_v1(payload.as_slice()).map_err(
                            |err| {
                                observe_network_runtime_eth_peer_decode_failure_v1(
                                    chain_id,
                                    peer.0,
                                    "status_payload_decode_failed",
                                );
                                NetworkError::Decode(err)
                            },
                        )?;
                        let local_status = build_eth_fullnode_native_rlpx_status_v1(
                            chain_id,
                            session._negotiated_eth_version as u32,
                        );
                        if let Err(reason) = validate_eth_chain_config_peer_status_v1(
                            &resolve_eth_chain_config_v1(chain_id),
                            local_status.latest_block,
                            eth_fullnode_native_head_time_for_validation_v1(chain_id),
                            &status,
                        ) {
                            observe_network_runtime_eth_peer_validation_reject_v1(
                                chain_id, peer.0, reason,
                            );
                            let disconnect_payload = eth_rlpx_build_disconnect_payload_v1(
                                eth_fullnode_peer_validation_disconnect_reason_code_v1(reason),
                            );
                            let _ = eth_rlpx_write_wire_frame_v1(
                                &mut session.stream,
                                &mut session.frame_session,
                                ETH_RLPX_P2P_DISCONNECT_MSG,
                                disconnect_payload.as_slice(),
                            );
                            let _ = unregister_network_runtime_peer(chain_id, peer.0);
                            disconnected = true;
                            disconnect_error = Some(NetworkError::Decode(
                                format_eth_fullnode_peer_validation_error_v1(
                                    reason,
                                    &local_status,
                                    &status,
                                ),
                            ));
                            break;
                        }
                        observe_network_runtime_eth_peer_status_ok_v1(
                            chain_id,
                            peer.0,
                            Some(status.latest_block),
                        );
                        session.remote_status = status;
                        observe_eth_native_status(chain_id);
                        let _ = observe_network_runtime_peer_head(
                            chain_id,
                            peer.0,
                            status.latest_block,
                        );
                        observe_network_runtime_eth_peer_head(
                            chain_id,
                            peer.0,
                            status.latest_block,
                        );
                        mark_network_runtime_eth_peer_session_ready_v1(
                            chain_id,
                            peer.0,
                            Some(status.latest_block),
                        );
                        report.status_updates = report.status_updates.saturating_add(1);
                        continue;
                    }
                    if code == eth_offset + ETH_RLPX_ETH_BLOCK_RANGE_UPDATE_MSG {
                        let update =
                            eth_rlpx_parse_block_range_update_payload_v1(payload.as_slice())
                                .map_err(|err| {
                                    observe_network_runtime_eth_peer_decode_failure_v1(
                                        chain_id,
                                        peer.0,
                                        "block_range_update_payload_decode_failed",
                                    );
                                    NetworkError::Decode(err)
                                })?;
                        session.remote_status.earliest_block = update.earliest_block;
                        session.remote_status.latest_block = update.latest_block;
                        session.remote_status.latest_block_hash = update.latest_block_hash;
                        let _ = observe_network_runtime_peer_head(
                            chain_id,
                            peer.0,
                            update.latest_block,
                        );
                        observe_network_runtime_eth_peer_head(
                            chain_id,
                            peer.0,
                            update.latest_block,
                        );
                        report.status_updates = report.status_updates.saturating_add(1);
                        continue;
                    }
                    if code == eth_offset + ETH_RLPX_ETH_NEW_BLOCK_HASHES_MSG {
                        let announced =
                            eth_rlpx_parse_new_block_hashes_payload_v1(payload.as_slice())
                                .map_err(|err| {
                                    observe_network_runtime_eth_peer_decode_failure_v1(
                                        chain_id,
                                        peer.0,
                                        "new_block_hashes_payload_decode_failed",
                                    );
                                    NetworkError::Decode(err)
                                })?;
                        if let Some(best) = announced.iter().max_by_key(|block| block.number) {
                            let _ =
                                observe_network_runtime_peer_head(chain_id, peer.0, best.number);
                            observe_network_runtime_eth_peer_head(chain_id, peer.0, best.number);
                        }
                        continue;
                    }
                    if code == eth_offset + ETH_RLPX_ETH_NEW_BLOCK_MSG {
                        let block = eth_rlpx_parse_new_block_payload_v1(payload.as_slice())
                            .map_err(|err| {
                                observe_network_runtime_eth_peer_decode_failure_v1(
                                    chain_id,
                                    peer.0,
                                    "new_block_payload_decode_failed",
                                );
                                NetworkError::Decode(err)
                            })?;
                        if let Err(err) = ingest_real_rlpx_new_block_v1(
                            chain_id,
                            peer.0,
                            session,
                            &block,
                            &mut report,
                        ) {
                            mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                chain_id,
                                peer.0,
                                &mut disconnected,
                                &mut disconnect_error,
                                err,
                            );
                            break;
                        }
                        continue;
                    }
                    if code == eth_offset + ETH_RLPX_ETH_TRANSACTIONS_MSG {
                        let txs = eth_rlpx_parse_transactions_payload_v1(payload.as_slice())
                            .map_err(|err| {
                                observe_network_runtime_eth_peer_decode_failure_v1(
                                    chain_id,
                                    peer.0,
                                    "transactions_payload_decode_failed",
                                );
                                NetworkError::Decode(err)
                            })?;
                        for (idx, tx_hash) in txs.tx_hashes.iter().enumerate() {
                            let tx_payload = txs.tx_rlp_items.get(idx).map(|item| item.as_slice());
                            observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
                                chain_id, peer.0, *tx_hash, tx_payload,
                            );
                        }
                        continue;
                    }
                    if code == eth_offset + ETH_RLPX_ETH_NEW_POOLED_TRANSACTION_HASHES_MSG {
                        let announcement = eth_rlpx_parse_new_pooled_transaction_hashes_payload_v1(
                            payload.as_slice(),
                        )
                        .map_err(|err| {
                            observe_network_runtime_eth_peer_decode_failure_v1(
                                chain_id,
                                peer.0,
                                "new_pooled_transaction_hashes_payload_decode_failed",
                            );
                            NetworkError::Decode(err)
                        })?;
                        for tx_hash in &announcement.tx_hashes {
                            observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
                                chain_id, peer.0, *tx_hash, None,
                            );
                        }
                        if !announcement.tx_hashes.is_empty() {
                            let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
                            let request_payload = eth_rlpx_build_get_pooled_transactions_payload_v1(
                                request_id,
                                announcement.tx_hashes.as_slice(),
                            );
                            if let Err(err) = eth_rlpx_write_wire_frame_v1(
                                &mut session.stream,
                                &mut session.frame_session,
                                eth_offset + ETH_RLPX_ETH_GET_POOLED_TRANSACTIONS_MSG,
                                request_payload.as_slice(),
                            )
                            .map_err(NetworkError::Io)
                            {
                                mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                    chain_id,
                                    peer.0,
                                    &mut disconnected,
                                    &mut disconnect_error,
                                    err,
                                );
                                break;
                            }
                            session.last_pooled_transactions_request_id = Some(request_id);
                            session.pending_pooled_transaction_hashes = announcement.tx_hashes;
                        }
                        continue;
                    }
                    if code == eth_offset + ETH_RLPX_ETH_GET_POOLED_TRANSACTIONS_MSG {
                        let request =
                            eth_rlpx_parse_get_pooled_transactions_payload_v1(payload.as_slice())
                                .map_err(|err| {
                                observe_network_runtime_eth_peer_decode_failure_v1(
                                    chain_id,
                                    peer.0,
                                    "get_pooled_transactions_payload_decode_failed",
                                );
                                NetworkError::Decode(err)
                            })?;
                        let response_txs =
                            build_eth_fullnode_native_pooled_transactions_response_v1(
                                chain_id,
                                request.hashes.as_slice(),
                            );
                        let response_payload = eth_rlpx_build_pooled_transactions_payload_v1(
                            request.request_id,
                            response_txs.as_slice(),
                        );
                        if let Err(err) = eth_rlpx_write_wire_frame_v1(
                            &mut session.stream,
                            &mut session.frame_session,
                            eth_offset + ETH_RLPX_ETH_POOLED_TRANSACTIONS_MSG,
                            response_payload.as_slice(),
                        )
                        .map_err(NetworkError::Io)
                        {
                            mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                chain_id,
                                peer.0,
                                &mut disconnected,
                                &mut disconnect_error,
                                err,
                            );
                            break;
                        }
                        continue;
                    }
                    if code == eth_offset + ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG {
                        let request =
                            eth_rlpx_parse_get_block_headers_payload_v1(payload.as_slice())
                                .map_err(|err| {
                                    observe_network_runtime_eth_peer_decode_failure_v1(
                                        chain_id,
                                        peer.0,
                                        "get_block_headers_payload_decode_failed",
                                    );
                                    NetworkError::Decode(err)
                                })?;
                        let headers =
                            build_eth_fullnode_native_block_headers_response_v1(chain_id, &request);
                        let response_payload = eth_rlpx_build_block_headers_payload_v1(
                            request.request_id,
                            headers.as_slice(),
                        );
                        if let Err(err) = eth_rlpx_write_wire_frame_v1(
                            &mut session.stream,
                            &mut session.frame_session,
                            eth_offset + ETH_RLPX_ETH_BLOCK_HEADERS_MSG,
                            response_payload.as_slice(),
                        )
                        .map_err(NetworkError::Io)
                        {
                            mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                chain_id,
                                peer.0,
                                &mut disconnected,
                                &mut disconnect_error,
                                err,
                            );
                            break;
                        }
                        continue;
                    }
                    if code == eth_offset + ETH_RLPX_ETH_GET_BLOCK_BODIES_MSG {
                        let request =
                            eth_rlpx_parse_get_block_bodies_payload_v1(payload.as_slice())
                                .map_err(|err| {
                                    observe_network_runtime_eth_peer_decode_failure_v1(
                                        chain_id,
                                        peer.0,
                                        "get_block_bodies_payload_decode_failed",
                                    );
                                    NetworkError::Decode(err)
                                })?;
                        let bodies =
                            build_eth_fullnode_native_block_bodies_response_v1(chain_id, &request);
                        let response_payload = eth_rlpx_build_block_bodies_payload_v1(
                            request.request_id,
                            bodies.as_slice(),
                        );
                        if let Err(err) = eth_rlpx_write_wire_frame_v1(
                            &mut session.stream,
                            &mut session.frame_session,
                            eth_offset + ETH_RLPX_ETH_BLOCK_BODIES_MSG,
                            response_payload.as_slice(),
                        )
                        .map_err(NetworkError::Io)
                        {
                            mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                chain_id,
                                peer.0,
                                &mut disconnected,
                                &mut disconnect_error,
                                err,
                            );
                            break;
                        }
                        continue;
                    }
                    if code == eth_offset + ETH_RLPX_ETH_POOLED_TRANSACTIONS_MSG {
                        let txs = eth_rlpx_parse_pooled_transactions_payload_v1(payload.as_slice())
                            .map_err(|err| {
                                observe_network_runtime_eth_peer_decode_failure_v1(
                                    chain_id,
                                    peer.0,
                                    "pooled_transactions_payload_decode_failed",
                                );
                                NetworkError::Decode(err)
                            })?;
                        if let Err(err) =
                            ingest_real_rlpx_pooled_transactions_v1(chain_id, peer.0, session, &txs)
                        {
                            mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                chain_id,
                                peer.0,
                                &mut disconnected,
                                &mut disconnect_error,
                                err,
                            );
                            break;
                        }
                        continue;
                    }
                    if code == eth_offset + ETH_RLPX_ETH_BLOCK_HEADERS_MSG {
                        let headers = eth_rlpx_parse_block_headers_payload_v1(payload.as_slice())
                            .map_err(|err| {
                            observe_network_runtime_eth_peer_decode_failure_v1(
                                chain_id,
                                peer.0,
                                "headers_payload_decode_failed",
                            );
                            NetworkError::Decode(err)
                        })?;
                        if let Err(err) = ingest_real_rlpx_block_headers_v1(
                            chain_id,
                            peer.0,
                            session,
                            &headers,
                            budget_hooks,
                            &mut report,
                        ) {
                            mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                chain_id,
                                peer.0,
                                &mut disconnected,
                                &mut disconnect_error,
                                err,
                            );
                            break;
                        }
                        continue;
                    }
                    if code == eth_offset + ETH_RLPX_ETH_BLOCK_BODIES_MSG {
                        let bodies = eth_rlpx_parse_block_bodies_payload_v1(payload.as_slice())
                            .map_err(|err| {
                                observe_network_runtime_eth_peer_decode_failure_v1(
                                    chain_id,
                                    peer.0,
                                    "bodies_payload_decode_failed",
                                );
                                NetworkError::Decode(err)
                            })?;
                        if let Err(err) = ingest_real_rlpx_block_bodies_v1(
                            chain_id,
                            peer.0,
                            session,
                            &bodies,
                            &mut report,
                        ) {
                            mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                chain_id,
                                peer.0,
                                &mut disconnected,
                                &mut disconnect_error,
                                err,
                            );
                            break;
                        }
                        continue;
                    }
                    if code == eth_offset + ETH_RLPX_ETH_GET_RECEIPTS_MSG {
                        let request = eth_rlpx_parse_get_receipts_payload_v1(payload.as_slice())
                            .map_err(|err| {
                                observe_network_runtime_eth_peer_decode_failure_v1(
                                    chain_id,
                                    peer.0,
                                    "get_receipts_payload_decode_failed",
                                );
                                NetworkError::Decode(err)
                            })?;
                        let response_blocks = build_eth_fullnode_native_receipts_response_blocks_v1(
                            chain_id,
                            request.hashes.as_slice(),
                        );
                        let response_payload = eth_rlpx_build_receipts_payload_v1(
                            request.request_id,
                            false,
                            response_blocks.as_slice(),
                            session._negotiated_eth_version,
                        );
                        if let Err(err) = eth_rlpx_write_wire_frame_v1(
                            &mut session.stream,
                            &mut session.frame_session,
                            eth_offset + ETH_RLPX_ETH_RECEIPTS_MSG,
                            response_payload.as_slice(),
                        )
                        .map_err(NetworkError::Io)
                        {
                            mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                chain_id,
                                peer.0,
                                &mut disconnected,
                                &mut disconnect_error,
                                err,
                            );
                            break;
                        }
                        continue;
                    }
                    if code == eth_offset + ETH_RLPX_ETH_RECEIPTS_MSG {
                        let receipts = eth_rlpx_parse_receipts_payload_v1(payload.as_slice())
                            .map_err(|err| {
                                observe_network_runtime_eth_peer_decode_failure_v1(
                                    chain_id,
                                    peer.0,
                                    "receipts_payload_decode_failed",
                                );
                                NetworkError::Decode(err)
                            })?;
                        if let Err(err) = ingest_real_rlpx_receipts_v1(
                            chain_id,
                            peer.0,
                            session,
                            &receipts,
                            &mut report,
                        ) {
                            mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                chain_id,
                                peer.0,
                                &mut disconnected,
                                &mut disconnect_error,
                                err,
                            );
                            break;
                        }
                        continue;
                    }
                    if let Some(snap_offset) = eth_rlpx_snap_base_offset_v1(
                        session._negotiated_eth_version,
                        session._negotiated_snap_version,
                    ) {
                        if code == snap_offset + ETH_RLPX_SNAP_GET_ACCOUNT_RANGE_MSG {
                            let request =
                                eth_rlpx_parse_get_account_range_payload_v1(payload.as_slice())
                                    .map_err(|err| {
                                        observe_network_runtime_eth_peer_decode_failure_v1(
                                            chain_id,
                                            peer.0,
                                            "snap_get_account_range_payload_decode_failed",
                                        );
                                        NetworkError::Decode(err)
                                    })?;
                            let response_payload =
                                build_eth_fullnode_native_snap_account_range_response_payload_v1(
                                    chain_id, &request,
                                );
                            if let Err(err) = eth_rlpx_write_wire_frame_v1(
                                &mut session.stream,
                                &mut session.frame_session,
                                snap_offset + ETH_RLPX_SNAP_ACCOUNT_RANGE_MSG,
                                response_payload.as_slice(),
                            )
                            .map_err(NetworkError::Io)
                            {
                                mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                    chain_id,
                                    peer.0,
                                    &mut disconnected,
                                    &mut disconnect_error,
                                    err,
                                );
                                break;
                            }
                            continue;
                        }
                        if code == snap_offset + ETH_RLPX_SNAP_GET_STORAGE_RANGES_MSG {
                            let request =
                                eth_rlpx_parse_get_storage_ranges_payload_v1(payload.as_slice())
                                    .map_err(|err| {
                                        observe_network_runtime_eth_peer_decode_failure_v1(
                                            chain_id,
                                            peer.0,
                                            "snap_get_storage_ranges_payload_decode_failed",
                                        );
                                        NetworkError::Decode(err)
                                    })?;
                            let response_payload =
                                build_eth_fullnode_native_snap_storage_ranges_response_payload_v1(
                                    chain_id, &request,
                                );
                            if let Err(err) = eth_rlpx_write_wire_frame_v1(
                                &mut session.stream,
                                &mut session.frame_session,
                                snap_offset + ETH_RLPX_SNAP_STORAGE_RANGES_MSG,
                                response_payload.as_slice(),
                            )
                            .map_err(NetworkError::Io)
                            {
                                mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                    chain_id,
                                    peer.0,
                                    &mut disconnected,
                                    &mut disconnect_error,
                                    err,
                                );
                                break;
                            }
                            continue;
                        }
                        if code == snap_offset + ETH_RLPX_SNAP_GET_BYTE_CODES_MSG {
                            let request =
                                eth_rlpx_parse_get_byte_codes_payload_v1(payload.as_slice())
                                    .map_err(|err| {
                                        observe_network_runtime_eth_peer_decode_failure_v1(
                                            chain_id,
                                            peer.0,
                                            "snap_get_byte_codes_payload_decode_failed",
                                        );
                                        NetworkError::Decode(err)
                                    })?;
                            let response_payload =
                                build_eth_fullnode_native_snap_byte_codes_response_payload_v1(
                                    chain_id, &request,
                                );
                            if let Err(err) = eth_rlpx_write_wire_frame_v1(
                                &mut session.stream,
                                &mut session.frame_session,
                                snap_offset + ETH_RLPX_SNAP_BYTE_CODES_MSG,
                                response_payload.as_slice(),
                            )
                            .map_err(NetworkError::Io)
                            {
                                mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                    chain_id,
                                    peer.0,
                                    &mut disconnected,
                                    &mut disconnect_error,
                                    err,
                                );
                                break;
                            }
                            continue;
                        }
                        if code == snap_offset + ETH_RLPX_SNAP_GET_TRIE_NODES_MSG {
                            let request =
                                eth_rlpx_parse_get_trie_nodes_payload_v1(payload.as_slice())
                                    .map_err(|err| {
                                        observe_network_runtime_eth_peer_decode_failure_v1(
                                            chain_id,
                                            peer.0,
                                            "snap_get_trie_nodes_payload_decode_failed",
                                        );
                                        NetworkError::Decode(err)
                                    })?;
                            let response_payload =
                                build_eth_fullnode_native_snap_trie_nodes_response_payload_v1(
                                    chain_id, &request,
                                );
                            if let Err(err) = eth_rlpx_write_wire_frame_v1(
                                &mut session.stream,
                                &mut session.frame_session,
                                snap_offset + ETH_RLPX_SNAP_TRIE_NODES_MSG,
                                response_payload.as_slice(),
                            )
                            .map_err(NetworkError::Io)
                            {
                                mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                    chain_id,
                                    peer.0,
                                    &mut disconnected,
                                    &mut disconnect_error,
                                    err,
                                );
                                break;
                            }
                            continue;
                        }
                        if code == snap_offset + ETH_RLPX_SNAP_ACCOUNT_RANGE_MSG {
                            let response =
                                eth_rlpx_parse_account_range_payload_v1(payload.as_slice())
                                    .map_err(|err| {
                                        observe_network_runtime_eth_peer_decode_failure_v1(
                                            chain_id,
                                            peer.0,
                                            "snap_account_range_payload_decode_failed",
                                        );
                                        NetworkError::Decode(err)
                                    })?;
                            if let Err(err) = ingest_real_rlpx_snap_account_range_v1(
                                chain_id, peer.0, session, &response,
                            ) {
                                mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                    chain_id,
                                    peer.0,
                                    &mut disconnected,
                                    &mut disconnect_error,
                                    err,
                                );
                                break;
                            }
                            continue;
                        }
                        if code == snap_offset + ETH_RLPX_SNAP_STORAGE_RANGES_MSG {
                            let response =
                                eth_rlpx_parse_storage_ranges_payload_v1(payload.as_slice())
                                    .map_err(|err| {
                                        observe_network_runtime_eth_peer_decode_failure_v1(
                                            chain_id,
                                            peer.0,
                                            "snap_storage_ranges_payload_decode_failed",
                                        );
                                        NetworkError::Decode(err)
                                    })?;
                            if let Err(err) = ingest_real_rlpx_snap_storage_ranges_v1(
                                chain_id, peer.0, session, &response,
                            ) {
                                mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                    chain_id,
                                    peer.0,
                                    &mut disconnected,
                                    &mut disconnect_error,
                                    err,
                                );
                                break;
                            }
                            continue;
                        }
                        if code == snap_offset + ETH_RLPX_SNAP_BYTE_CODES_MSG {
                            let response = eth_rlpx_parse_byte_codes_payload_v1(payload.as_slice())
                                .map_err(|err| {
                                    observe_network_runtime_eth_peer_decode_failure_v1(
                                        chain_id,
                                        peer.0,
                                        "snap_byte_codes_payload_decode_failed",
                                    );
                                    NetworkError::Decode(err)
                                })?;
                            if let Err(err) = ingest_real_rlpx_snap_byte_codes_v1(
                                chain_id, peer.0, session, &response,
                            ) {
                                mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                    chain_id,
                                    peer.0,
                                    &mut disconnected,
                                    &mut disconnect_error,
                                    err,
                                );
                                break;
                            }
                            continue;
                        }
                        if code == snap_offset + ETH_RLPX_SNAP_TRIE_NODES_MSG {
                            let response = eth_rlpx_parse_trie_nodes_payload_v1(payload.as_slice())
                                .map_err(|err| {
                                    observe_network_runtime_eth_peer_decode_failure_v1(
                                        chain_id,
                                        peer.0,
                                        "snap_trie_nodes_payload_decode_failed",
                                    );
                                    NetworkError::Decode(err)
                                })?;
                            if let Err(err) = ingest_real_rlpx_snap_trie_nodes_v1(
                                chain_id, peer.0, session, &response,
                            ) {
                                mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                    chain_id,
                                    peer.0,
                                    &mut disconnected,
                                    &mut disconnect_error,
                                    err,
                                );
                                break;
                            }
                            continue;
                        }
                    }
                    if (session._negotiated_eth_version >= 71
                        || session._negotiated_snap_version.is_none())
                        && code == eth_offset + ETH_RLPX_ETH_GET_BLOCK_ACCESS_LISTS_MSG
                    {
                        let request =
                            eth_rlpx_parse_get_block_access_lists_payload_v1(payload.as_slice())
                                .map_err(|err| {
                                    observe_network_runtime_eth_peer_decode_failure_v1(
                                        chain_id,
                                        peer.0,
                                        "get_block_access_lists_payload_decode_failed",
                                    );
                                    NetworkError::Decode(err)
                                })?;
                        let response_lists = request
                            .hashes
                            .iter()
                            .map(|hash| {
                                get_network_runtime_native_block_access_list_payload_v1(
                                    chain_id, *hash,
                                )
                            })
                            .collect::<Vec<_>>();
                        let response_payload = eth_rlpx_build_block_access_lists_payload_v1(
                            request.request_id,
                            response_lists.as_slice(),
                        );
                        if let Err(err) = eth_rlpx_write_wire_frame_v1(
                            &mut session.stream,
                            &mut session.frame_session,
                            eth_offset + ETH_RLPX_ETH_BLOCK_ACCESS_LISTS_MSG,
                            response_payload.as_slice(),
                        )
                        .map_err(NetworkError::Io)
                        {
                            mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                chain_id,
                                peer.0,
                                &mut disconnected,
                                &mut disconnect_error,
                                err,
                            );
                            break;
                        }
                        continue;
                    }
                    if (session._negotiated_eth_version >= 71
                        || session._negotiated_snap_version.is_none())
                        && code == eth_offset + ETH_RLPX_ETH_BLOCK_ACCESS_LISTS_MSG
                    {
                        let response =
                            eth_rlpx_parse_block_access_lists_payload_v1(payload.as_slice())
                                .map_err(|err| {
                                    observe_network_runtime_eth_peer_decode_failure_v1(
                                        chain_id,
                                        peer.0,
                                        "block_access_lists_payload_decode_failed",
                                    );
                                    NetworkError::Decode(err)
                                })?;
                        if let Err(err) = ingest_real_rlpx_block_access_lists_v1(
                            chain_id, peer.0, session, &response,
                        ) {
                            mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                chain_id,
                                peer.0,
                                &mut disconnected,
                                &mut disconnect_error,
                                err,
                            );
                            break;
                        }
                        continue;
                    }
                }
                Err(err) => {
                    if err.contains("timed out")
                        || err.contains("would block")
                        || err.contains("partial_read_timeout")
                        || err.contains("os error 10060")
                        || err.contains("os error 10035")
                        || err.contains("没有正确答复")
                        || err.contains("没有反应")
                    {
                        break;
                    }
                    if eth_fullnode_rlpx_error_is_remote_closed_v1(err.as_str())
                        || eth_fullnode_rlpx_error_is_session_desync_v1(err.as_str())
                    {
                        let pending_request =
                            eth_fullnode_native_rlpx_session_has_pending_request_v1(session);
                        if pending_request {
                            observe_network_runtime_eth_peer_disconnect_v1(chain_id, peer.0, None);
                        } else {
                            mark_network_runtime_eth_peer_session_closed_v1(chain_id, peer.0);
                        }
                        let _ = unregister_network_runtime_peer(chain_id, peer.0);
                        disconnected = true;
                        if pending_request {
                            disconnect_error = Some(NetworkError::Io(format!(
                                "rlpx_session_closed:endpoint={}:{}",
                                session.endpoint.addr_hint, err
                            )));
                        }
                        break;
                    }
                    observe_network_runtime_eth_peer_decode_failure_v1(
                        chain_id,
                        peer.0,
                        "frame_decode_failed",
                    );
                    let _ = unregister_network_runtime_peer(chain_id, peer.0);
                    disconnected = true;
                    disconnect_error = Some(NetworkError::Decode(err));
                    break;
                }
            }
        }

        let now_ms = now_unix_ms();
        if !disconnected {
            if session.last_snap_storage_ranges_request_id.is_some()
                && now_ms.saturating_sub(session.last_sync_request_unix_ms)
                    >= budget_hooks.rlpx_request_timeout_ms.max(1)
            {
                observe_network_runtime_eth_peer_timeout_v1(
                    chain_id,
                    peer.0,
                    "snap_storage_ranges_timeout",
                );
                let _ = unregister_network_runtime_peer(chain_id, peer.0);
                disconnected = true;
                disconnect_error = Some(NetworkError::Io(format!(
                    "rlpx_request_timeout:snap_storage_ranges:endpoint={}",
                    session.endpoint.addr_hint
                )));
            } else if session.last_snap_byte_codes_request_id.is_some()
                && now_ms.saturating_sub(session.last_sync_request_unix_ms)
                    >= budget_hooks.rlpx_request_timeout_ms.max(1)
            {
                observe_network_runtime_eth_peer_timeout_v1(
                    chain_id,
                    peer.0,
                    "snap_byte_codes_timeout",
                );
                let _ = unregister_network_runtime_peer(chain_id, peer.0);
                disconnected = true;
                disconnect_error = Some(NetworkError::Io(format!(
                    "rlpx_request_timeout:snap_byte_codes:endpoint={}",
                    session.endpoint.addr_hint
                )));
            } else if session.last_snap_trie_nodes_request_id.is_some()
                && now_ms.saturating_sub(session.last_sync_request_unix_ms)
                    >= budget_hooks.rlpx_request_timeout_ms.max(1)
            {
                observe_network_runtime_eth_peer_timeout_v1(
                    chain_id,
                    peer.0,
                    "snap_trie_nodes_timeout",
                );
                let _ = unregister_network_runtime_peer(chain_id, peer.0);
                disconnected = true;
                disconnect_error = Some(NetworkError::Io(format!(
                    "rlpx_request_timeout:snap_trie_nodes:endpoint={}",
                    session.endpoint.addr_hint
                )));
            } else if session.last_snap_account_range_request_id.is_some()
                && now_ms.saturating_sub(session.last_sync_request_unix_ms)
                    >= budget_hooks.rlpx_request_timeout_ms.max(1)
            {
                observe_network_runtime_eth_peer_timeout_v1(
                    chain_id,
                    peer.0,
                    "snap_account_range_timeout",
                );
                let _ = unregister_network_runtime_peer(chain_id, peer.0);
                disconnected = true;
                disconnect_error = Some(NetworkError::Io(format!(
                    "rlpx_request_timeout:snap_account_range:endpoint={}",
                    session.endpoint.addr_hint
                )));
            } else if session.last_receipts_request_id.is_some()
                && now_ms.saturating_sub(session.last_sync_request_unix_ms)
                    >= budget_hooks.rlpx_request_timeout_ms.max(1)
            {
                observe_network_runtime_eth_peer_timeout_v1(chain_id, peer.0, "receipts_timeout");
                let _ = unregister_network_runtime_peer(chain_id, peer.0);
                disconnected = true;
                disconnect_error = Some(NetworkError::Io(format!(
                    "rlpx_request_timeout:receipts:endpoint={}",
                    session.endpoint.addr_hint
                )));
            } else if session.last_block_access_lists_request_id.is_some()
                && now_ms.saturating_sub(session.last_sync_request_unix_ms)
                    >= budget_hooks.rlpx_request_timeout_ms.max(1)
            {
                observe_network_runtime_eth_peer_timeout_v1(
                    chain_id,
                    peer.0,
                    "block_access_lists_timeout",
                );
                let _ = unregister_network_runtime_peer(chain_id, peer.0);
                disconnected = true;
                disconnect_error = Some(NetworkError::Io(format!(
                    "rlpx_request_timeout:block_access_lists:endpoint={}",
                    session.endpoint.addr_hint
                )));
            } else if session.last_bodies_request_id.is_some()
                && now_ms.saturating_sub(session.last_sync_request_unix_ms)
                    >= budget_hooks.rlpx_request_timeout_ms.max(1)
            {
                observe_network_runtime_eth_peer_timeout_v1(chain_id, peer.0, "bodies_timeout");
                let _ = unregister_network_runtime_peer(chain_id, peer.0);
                disconnected = true;
                disconnect_error = Some(NetworkError::Io(format!(
                    "rlpx_request_timeout:bodies:endpoint={}",
                    session.endpoint.addr_hint
                )));
            } else if session.last_headers_request_id.is_some()
                && session.pending_body_headers.is_empty()
                && now_ms.saturating_sub(session.last_sync_request_unix_ms)
                    >= budget_hooks.rlpx_request_timeout_ms.max(1)
            {
                observe_network_runtime_eth_peer_timeout_v1(chain_id, peer.0, "headers_timeout");
                let _ = unregister_network_runtime_peer(chain_id, peer.0);
                disconnected = true;
                disconnect_error = Some(NetworkError::Io(format!(
                    "rlpx_request_timeout:headers:endpoint={}",
                    session.endpoint.addr_hint
                )));
            }
        }
        if !disconnected
            && session.pending_body_headers.is_empty()
            && now_ms.saturating_sub(session.last_sync_request_unix_ms)
                >= budget_hooks.sync_request_interval_ms.max(1)
        {
            let recovered_missing_body =
                match dispatch_eth_fullnode_native_rlpx_missing_body_recovery_v1(
                    chain_id,
                    peer,
                    session,
                    &mut report,
                ) {
                    Ok(recovered) => recovered,
                    Err(err) => {
                        mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                            chain_id,
                            peer.0,
                            &mut disconnected,
                            &mut disconnect_error,
                            err,
                        );
                        false
                    }
                };
            if !disconnected && !recovered_missing_body {
                let recovered_missing_receipts =
                    match dispatch_eth_fullnode_native_rlpx_missing_receipts_recovery_v1(
                        chain_id,
                        peer,
                        session,
                        &mut report,
                    ) {
                        Ok(recovered) => recovered,
                        Err(err) => {
                            mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                chain_id,
                                peer.0,
                                &mut disconnected,
                                &mut disconnect_error,
                                err,
                            );
                            false
                        }
                    };
                // Public peers often disconnect when we append a forward header
                // pull immediately after they served body/receipt material.
                if !disconnected
                    && !recovered_missing_receipts
                    && report.body_updates == 0
                    && report.receipt_updates == 0
                {
                    let probed_status_head =
                        match dispatch_eth_fullnode_native_rlpx_status_head_pivot_probe_v1(
                            chain_id,
                            peer,
                            session,
                            &mut report,
                        ) {
                            Ok(probed) => probed,
                            Err(err) => {
                                mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                    chain_id,
                                    peer.0,
                                    &mut disconnected,
                                    &mut disconnect_error,
                                    err,
                                );
                                false
                            }
                        };
                    if !disconnected && !probed_status_head {
                        let Some(msg) =
                            build_eth_fullnode_native_sync_request_v1(local_node, chain_id)
                        else {
                            return Ok(report);
                        };
                        match msg {
                            ProtocolMessage::EvmNative(EvmNativeMessage::GetBlockHeaders {
                                start_height,
                                max,
                                skip,
                                reverse,
                                ..
                            }) => {
                                let max = eth_fullnode_native_budget_capped_headers_batch_v1(
                                    max,
                                    budget_hooks,
                                );
                                if !should_dispatch_eth_fullnode_native_header_request_v1(
                                    chain_id,
                                    peer.0,
                                    start_height,
                                    skip,
                                    reverse,
                                ) {
                                    return Ok(report);
                                }
                                let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
                                let payload = eth_rlpx_build_get_block_headers_payload_v1(
                                    request_id,
                                    start_height,
                                    max,
                                    skip,
                                    reverse,
                                );
                                if let Err(err) = eth_rlpx_write_wire_frame_v1(
                                    &mut session.stream,
                                    &mut session.frame_session,
                                    ETH_RLPX_BASE_PROTOCOL_OFFSET
                                        + ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG,
                                    payload.as_slice(),
                                )
                                .map_err(|err| {
                                    observe_eth_fullnode_rlpx_request_write_error_v1(
                                        chain_id,
                                        peer.0,
                                        "headers_request_write_failed",
                                        err.as_str(),
                                    );
                                    NetworkError::Io(err)
                                }) {
                                    mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                        chain_id,
                                        peer.0,
                                        &mut disconnected,
                                        &mut disconnect_error,
                                        err,
                                    );
                                } else {
                                    observe_eth_native_headers_pull(chain_id);
                                    observe_network_runtime_eth_peer_syncing_v1(chain_id, peer.0);
                                    mark_eth_fullnode_native_header_inflight_v1(
                                        chain_id,
                                        peer.0,
                                        request_id,
                                        start_height,
                                        skip,
                                        reverse,
                                    );
                                    session.last_headers_request_id = Some(request_id);
                                    session.pending_headers_request =
                                        Some(EthRlpxGetBlockHeadersRequestV1 {
                                            request_id,
                                            start_height,
                                            origin_hash: None,
                                            max_headers: max,
                                            skip,
                                            reverse,
                                        });
                                    session.last_bodies_request_id = None;
                                    session.pending_body_request_offset = 0;
                                    session.last_receipts_request_id = None;
                                    session.pending_receipt_request_offset = 0;
                                    clear_eth_fullnode_native_snap_request_state_v1(session);
                                    session.last_sync_request_unix_ms = now_ms;
                                    report.sync_requests = report.sync_requests.saturating_add(1);
                                    eprintln!(
                                        "network_info: rlpx stage headers_requested chain_id={} peer={} endpoint={} request_id={} start={} max={} skip={} reverse={}",
                                        chain_id,
                                        peer.0,
                                        session.endpoint.addr_hint,
                                        request_id,
                                        start_height,
                                        max,
                                        skip,
                                        reverse
                                    );
                                }
                            }
                            ProtocolMessage::EvmNative(EvmNativeMessage::SnapGetAccountRange {
                                block_hash,
                                origin,
                                ..
                            }) => {
                                if eth_rlpx_snap_base_offset_v1(
                                    session._negotiated_eth_version,
                                    session._negotiated_snap_version,
                                )
                                .is_none()
                                {
                                    return Ok(report);
                                }
                                let root = eth_fullnode_native_rlpx_snap_root_hint_v1(
                                    chain_id, block_hash,
                                );
                                let limit_hash = [0xffu8; 32];
                                if let Err(err) =
                                    dispatch_eth_fullnode_native_snap_account_range_request_v1(
                                        chain_id,
                                        peer.0,
                                        session,
                                        root,
                                        origin,
                                        limit_hash,
                                        ETH_RLPX_SNAP_DEFAULT_ACCOUNT_RANGE_BYTES,
                                        "snap_account_range_request_write_failed",
                                    )
                                {
                                    mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                                        chain_id,
                                        peer.0,
                                        &mut disconnected,
                                        &mut disconnect_error,
                                        err,
                                    );
                                } else {
                                    report.sync_requests = report.sync_requests.saturating_add(1);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        if !disconnected
            && now_ms.saturating_sub(session.last_tx_broadcast_unix_ms)
                >= budget_hooks.tx_broadcast_interval_ms.max(1)
        {
            if let Err(err) = dispatch_eth_fullnode_native_rlpx_tx_broadcast_v1(
                chain_id,
                local_node,
                peer,
                session,
                budget_hooks,
            ) {
                mark_eth_fullnode_native_rlpx_session_disconnected_v1(
                    chain_id,
                    peer.0,
                    &mut disconnected,
                    &mut disconnect_error,
                    err,
                );
            } else {
                session.last_tx_broadcast_unix_ms = now_ms;
            }
        }
    }
    if disconnected {
        clear_eth_fullnode_native_recovery_inflight_peer_v1(chain_id, peer.0);
        clear_eth_fullnode_native_header_inflight_peer_v1(chain_id, peer.0);
        sessions.remove(&(chain_id, peer.0));
    }
    if let Some(err) = disconnect_error {
        return Err(err);
    }
    Ok(report)
}

fn build_eth_fullnode_native_receipts_response_blocks_v1(
    chain_id: u64,
    hashes: &[[u8; 32]],
) -> Vec<Vec<Vec<u8>>> {
    let body = get_network_runtime_native_body_snapshot_v1(chain_id);
    let mut out = Vec::new();
    for hash in hashes {
        if let Some(receipt) = get_network_runtime_native_receipt_snapshot_v1(chain_id, *hash) {
            if !receipt.receipts_available {
                break;
            }
            out.push(receipt.raw_receipts);
            continue;
        }
        let Some(body) = body.as_ref() else {
            break;
        };
        if body.block_hash != *hash || !body.body_available {
            break;
        }
        if !body.tx_hashes.is_empty() {
            break;
        }
        out.push(Vec::new());
    }
    out
}

fn eth_fullnode_native_header_record_from_canonical_block_v1(
    chain_id: u64,
    block: &crate::runtime_status::NetworkRuntimeNativeCanonicalBlockStateV1,
) -> Option<EthRlpxBlockHeaderRecordV1> {
    let raw_rlp = get_network_runtime_native_header_rlp_v1(chain_id, block.hash)?;
    let probe = EthRlpxBlockHeaderRecordV1 {
        number: block.number,
        hash: block.hash,
        parent_hash: block.parent_hash,
        state_root: block.state_root,
        transactions_root: block
            .transactions_root
            .unwrap_or_else(crate::eth_rlpx_empty_trie_root_v1),
        receipts_root: block
            .receipts_root
            .unwrap_or_else(crate::eth_rlpx_empty_trie_root_v1),
        ommers_hash: block
            .ommers_hash
            .unwrap_or_else(crate::eth_rlpx_empty_ommers_hash_v1),
        logs_bloom: Vec::new(),
        gas_limit: None,
        gas_used: None,
        timestamp: None,
        base_fee_per_gas: None,
        withdrawals_root: None,
        blob_gas_used: None,
        excess_blob_gas: None,
        block_access_list_hash: None,
        raw_rlp: Some(raw_rlp),
    };
    let wrapped = eth_rlpx_build_block_headers_payload_v1(0, std::slice::from_ref(&probe));
    let mut parsed = eth_rlpx_parse_block_headers_payload_v1(wrapped.as_slice())
        .ok()?
        .headers;
    let header = parsed.pop()?;
    if header.hash != block.hash || header.number != block.number {
        return None;
    }
    Some(header)
}

fn build_eth_fullnode_native_block_headers_response_v1(
    chain_id: u64,
    request: &EthRlpxGetBlockHeadersRequestV1,
) -> Vec<EthRlpxBlockHeaderRecordV1> {
    let max_headers = usize::try_from(request.max_headers)
        .unwrap_or(usize::MAX)
        .min(ETH_FULLNODE_NATIVE_HEADERS_SERVE_MAX_V1);
    if max_headers == 0 {
        return Vec::new();
    }
    let mut blocks = snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 4096)
        .into_iter()
        .filter(|block| block.canonical && block.header_observed)
        .collect::<Vec<_>>();
    blocks.sort_by(|a, b| a.number.cmp(&b.number).then_with(|| a.hash.cmp(&b.hash)));
    let origin_number = if let Some(origin_hash) = request.origin_hash {
        let Some(origin) = blocks.iter().find(|block| block.hash == origin_hash) else {
            return Vec::new();
        };
        origin.number
    } else {
        request.start_height
    };
    let Some(step) = request.skip.checked_add(1) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let mut cursor = origin_number;
    while out.len() < max_headers {
        let Some(block) = blocks.iter().find(|block| block.number == cursor) else {
            break;
        };
        let Some(header) =
            eth_fullnode_native_header_record_from_canonical_block_v1(chain_id, block)
        else {
            break;
        };
        out.push(header);
        if request.reverse {
            let Some(next) = cursor.checked_sub(step) else {
                break;
            };
            cursor = next;
        } else {
            let Some(next) = cursor.checked_add(step) else {
                break;
            };
            cursor = next;
        }
    }
    out
}

fn eth_fullnode_native_block_body_payload_from_canonical_block_v1(
    block: &crate::runtime_status::NetworkRuntimeNativeCanonicalBlockStateV1,
) -> Option<EthRlpxBlockBodyPayloadV1> {
    if !block.canonical
        || !block.body_available
        || block.raw_tx_rlps.len() != block.tx_hashes.len()
        || !block.ommer_hashes.is_empty()
    {
        return None;
    }
    Some(EthRlpxBlockBodyPayloadV1 {
        tx_rlp_items: block.raw_tx_rlps.clone(),
        ommer_header_rlp_items: Vec::new(),
        withdrawal_rlp_items: block.withdrawal_rlp_items.clone(),
    })
}

fn build_eth_fullnode_native_block_bodies_response_v1(
    chain_id: u64,
    request: &EthRlpxGetBlockBodiesRequestV1,
) -> Vec<EthRlpxBlockBodyPayloadV1> {
    let blocks = snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 4096);
    let mut out = Vec::new();
    for hash in request
        .hashes
        .iter()
        .take(ETH_FULLNODE_NATIVE_BODIES_SERVE_MAX_V1.saturating_mul(2))
    {
        if out.len() >= ETH_FULLNODE_NATIVE_BODIES_SERVE_MAX_V1 {
            break;
        }
        let Some(block) = blocks
            .iter()
            .find(|block| block.hash == *hash && block.header_observed)
        else {
            break;
        };
        let Some(body) = eth_fullnode_native_block_body_payload_from_canonical_block_v1(block)
        else {
            break;
        };
        out.push(body);
    }
    out
}

fn build_eth_fullnode_native_missing_receipts_pending_v1(
    chain_id: u64,
) -> Option<EthFullnodeNativePendingBodyHeaderV1> {
    let header = get_network_runtime_native_header_snapshot_v1(chain_id)?;
    let (tx_count, withdrawal_count) =
        eth_fullnode_native_receipt_recovery_body_hint_v1(chain_id, &header)?;
    if get_network_runtime_native_receipt_snapshot_v1(chain_id, header.hash)
        .is_some_and(|receipt| receipt.receipts_available)
    {
        return None;
    }
    Some(EthFullnodeNativePendingBodyHeaderV1 {
        number: header.number,
        hash: header.hash,
        parent_hash: header.parent_hash,
        state_root: header.state_root,
        transactions_root: header.transactions_root,
        receipts_root: header.receipts_root,
        tx_count: Some(tx_count),
        withdrawal_count,
    })
}

fn eth_fullnode_native_receipt_recovery_body_hint_v1(
    chain_id: u64,
    header: &crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1,
) -> Option<(usize, Option<usize>)> {
    if let Some(body) = get_network_runtime_native_body_snapshot_v1(chain_id) {
        if body.number == header.number && body.block_hash == header.hash && body.body_available {
            return Some((body.tx_hashes.len(), body.withdrawal_count));
        }
    }
    snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 4096)
        .into_iter()
        .find(|block| {
            block.number == header.number
                && block.hash == header.hash
                && block.header_observed
                && block.body_available
        })
        .map(|block| (block.tx_hashes.len(), block.withdrawal_count))
}

#[cfg(test)]
fn build_eth_fullnode_native_missing_body_pending_v1(
    chain_id: u64,
) -> Option<EthFullnodeNativePendingBodyHeaderV1> {
    build_eth_fullnode_native_missing_body_pending_headers_v1(chain_id)
        .into_iter()
        .next()
}

fn build_eth_fullnode_native_latest_missing_body_pending_v1(
    chain_id: u64,
) -> Option<EthFullnodeNativePendingBodyHeaderV1> {
    let header = get_network_runtime_native_header_snapshot_v1(chain_id)?;
    if !eth_fullnode_native_header_can_recover_missing_body_v1(&header) {
        return None;
    }
    if get_network_runtime_native_body_snapshot_v1(chain_id).is_some_and(|body| {
        body.block_hash == header.hash && body.body_available && body.txs_materialized
    }) {
        return None;
    }
    Some(EthFullnodeNativePendingBodyHeaderV1 {
        number: header.number,
        hash: header.hash,
        parent_hash: header.parent_hash,
        state_root: header.state_root,
        transactions_root: header.transactions_root,
        receipts_root: header.receipts_root,
        tx_count: None,
        withdrawal_count: None,
    })
}

fn eth_fullnode_native_is_chasing_remote_head_v1(chain_id: u64) -> bool {
    get_network_runtime_sync_status(chain_id)
        .is_some_and(|status| status.highest_block > status.current_block)
}

fn build_eth_fullnode_native_missing_body_pending_headers_v1(
    chain_id: u64,
) -> Vec<EthFullnodeNativePendingBodyHeaderV1> {
    let mut pending = Vec::new();
    let mut seen = HashSet::<[u8; 32]>::new();
    if let Some(latest) = build_eth_fullnode_native_latest_missing_body_pending_v1(chain_id) {
        seen.insert(latest.hash);
        pending.push(latest);
    }

    if eth_fullnode_native_is_chasing_remote_head_v1(chain_id) {
        if let Some(latest) = pending.first().copied() {
            let retained_blocks =
                snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 4096);
            let mut expected_hash = latest.parent_hash;
            let mut expected_number = latest.number.checked_sub(1);
            while pending.len() < ETH_FULLNODE_NATIVE_MISSING_BODY_CHASE_HEAD_BATCH_MAX_V1 {
                let Some(number) = expected_number else {
                    break;
                };
                let Some(block) = retained_blocks
                    .iter()
                    .find(|block| block.number == number && block.hash == expected_hash)
                else {
                    break;
                };
                if seen.contains(&block.hash)
                    || !block.header_observed
                    || block.body_available
                    || !eth_fullnode_native_canonical_block_can_recover_missing_body_v1(block)
                {
                    break;
                }
                if get_network_runtime_native_body_snapshot_v1(chain_id).is_some_and(|body| {
                    body.block_hash == block.hash && body.body_available && body.txs_materialized
                }) {
                    break;
                }
                let (Some(transactions_root), Some(receipts_root)) =
                    (block.transactions_root, block.receipts_root)
                else {
                    break;
                };
                seen.insert(block.hash);
                pending.push(EthFullnodeNativePendingBodyHeaderV1 {
                    number: block.number,
                    hash: block.hash,
                    parent_hash: block.parent_hash,
                    state_root: block.state_root,
                    transactions_root,
                    receipts_root,
                    tx_count: None,
                    withdrawal_count: None,
                });
                expected_hash = block.parent_hash;
                expected_number = block.number.checked_sub(1);
            }
        }
        // Public peers commonly return a short prefix under churn; recover the current head first.
        pending.truncate(ETH_FULLNODE_NATIVE_MISSING_BODY_CHASE_HEAD_BATCH_MAX_V1);
        return pending;
    }

    let mut retained_blocks = snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 4096);
    retained_blocks.sort_by(|a, b| {
        a.number
            .cmp(&b.number)
            .then_with(|| a.observed_unix_ms.cmp(&b.observed_unix_ms))
            .then_with(|| a.hash.cmp(&b.hash))
    });
    for block in retained_blocks {
        if pending.len() >= ETH_FULLNODE_NATIVE_MISSING_BODY_RECOVERY_BATCH_MAX_V1 {
            break;
        }
        if seen.contains(&block.hash)
            || !block.header_observed
            || block.body_available
            || !eth_fullnode_native_canonical_block_can_recover_missing_body_v1(&block)
        {
            continue;
        }
        if get_network_runtime_native_body_snapshot_v1(chain_id).is_some_and(|body| {
            body.block_hash == block.hash && body.body_available && body.txs_materialized
        }) {
            continue;
        }
        let (Some(transactions_root), Some(receipts_root)) =
            (block.transactions_root, block.receipts_root)
        else {
            continue;
        };
        seen.insert(block.hash);
        pending.push(EthFullnodeNativePendingBodyHeaderV1 {
            number: block.number,
            hash: block.hash,
            parent_hash: block.parent_hash,
            state_root: block.state_root,
            transactions_root,
            receipts_root,
            tx_count: None,
            withdrawal_count: None,
        });
    }

    pending.sort_by(|a, b| a.number.cmp(&b.number).then_with(|| a.hash.cmp(&b.hash)));
    pending.truncate(ETH_FULLNODE_NATIVE_MISSING_BODY_RECOVERY_BATCH_MAX_V1);
    pending
}

fn eth_fullnode_native_canonical_block_can_recover_missing_body_v1(
    block: &crate::runtime_status::NetworkRuntimeNativeCanonicalBlockStateV1,
) -> bool {
    let looks_like_minimal_operator_anchor = block.transactions_root
        == Some(crate::eth_rlpx_empty_trie_root_v1())
        && block.receipts_root == Some(crate::eth_rlpx_empty_trie_root_v1())
        && block.ommers_hash == Some(crate::eth_rlpx_empty_ommers_hash_v1())
        && block.source_peer_id.is_none();
    !looks_like_minimal_operator_anchor
}

fn eth_fullnode_native_header_can_recover_missing_body_v1(
    header: &crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1,
) -> bool {
    let looks_like_minimal_operator_anchor = header.source_peer_id.is_none()
        && header.transactions_root == crate::eth_rlpx_empty_trie_root_v1()
        && header.receipts_root == crate::eth_rlpx_empty_trie_root_v1()
        && header.ommers_hash == crate::eth_rlpx_empty_ommers_hash_v1()
        && header.logs_bloom.is_empty()
        && header.gas_limit.is_none()
        && header.gas_used.is_none()
        && header.timestamp.is_none();
    !looks_like_minimal_operator_anchor
}

fn dispatch_eth_fullnode_native_rlpx_missing_body_recovery_v1(
    chain_id: u64,
    peer: NodeId,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    report: &mut EthFullnodeNativeRlpxPeerTickReportV1,
) -> Result<bool, NetworkError> {
    if eth_fullnode_native_rlpx_session_has_pending_request_v1(session) {
        return Ok(false);
    }
    let pending_headers = build_eth_fullnode_native_missing_body_pending_headers_v1(chain_id);
    if pending_headers.is_empty() {
        return Ok(false);
    };
    let latest_missing_hash = build_eth_fullnode_native_latest_missing_body_pending_v1(chain_id)
        .map(|pending| pending.hash);
    let latest_missing_in_original = latest_missing_hash
        .is_some_and(|hash| pending_headers.iter().any(|pending| pending.hash == hash));
    let pending_headers = filter_eth_fullnode_native_recovery_inflight_headers_v1(
        chain_id,
        peer.0,
        ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_BODY_KIND_V1,
        pending_headers,
    );
    if pending_headers.is_empty() {
        return Ok(latest_missing_in_original);
    }
    let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
    let hashes = pending_headers
        .iter()
        .map(|pending| pending.hash)
        .collect::<Vec<_>>();
    let payload = eth_rlpx_build_get_block_bodies_payload_v1(request_id, hashes.as_slice());
    eth_rlpx_write_wire_frame_v1(
        &mut session.stream,
        &mut session.frame_session,
        ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_GET_BLOCK_BODIES_MSG,
        payload.as_slice(),
    )
    .map_err(|err| {
        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            peer.0,
            "missing_body_recovery_request_write_failed",
            err.as_str(),
        );
        NetworkError::Io(err)
    })?;
    observe_eth_native_bodies_pull(chain_id);
    observe_network_runtime_eth_peer_syncing_v1(chain_id, peer.0);
    session.pending_body_headers = pending_headers;
    mark_eth_fullnode_native_recovery_inflight_v1(
        chain_id,
        peer.0,
        request_id,
        ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_BODY_KIND_V1,
        session.pending_body_headers.as_slice(),
    );
    clear_eth_fullnode_native_headers_request_state_v1(session);
    session.last_bodies_request_id = Some(request_id);
    session.pending_body_request_offset = 0;
    session.last_receipts_request_id = None;
    session.pending_receipt_request_offset = 0;
    clear_eth_fullnode_native_snap_request_state_v1(session);
    session.last_sync_request_unix_ms = now_unix_ms();
    report.sync_requests = report.sync_requests.saturating_add(1);
    eprintln!(
        "network_info: rlpx stage missing_bodies_requested chain_id={} peer={} endpoint={} request_id={} blocks={}",
        chain_id,
        peer.0,
        session.endpoint.addr_hint,
        request_id,
        session.pending_body_headers.len()
    );
    Ok(true)
}

fn dispatch_eth_fullnode_native_rlpx_missing_receipts_recovery_v1(
    chain_id: u64,
    peer: NodeId,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    report: &mut EthFullnodeNativeRlpxPeerTickReportV1,
) -> Result<bool, NetworkError> {
    if session.last_headers_request_id.is_some()
        || session.last_bodies_request_id.is_some()
        || session.last_receipts_request_id.is_some()
        || session.last_snap_account_range_request_id.is_some()
        || session.last_snap_storage_ranges_request_id.is_some()
        || session.last_snap_byte_codes_request_id.is_some()
        || session.last_snap_trie_nodes_request_id.is_some()
        || !session.pending_body_headers.is_empty()
    {
        return Ok(false);
    }
    let Some(pending) = build_eth_fullnode_native_missing_receipts_pending_v1(chain_id) else {
        return Ok(false);
    };
    let mut pending_headers = vec![pending];
    let empty_receipts = materialize_empty_receipts_for_pending_body_headers_v1(
        chain_id,
        peer.0,
        &mut pending_headers,
    )?;
    report.receipt_updates = report.receipt_updates.saturating_add(empty_receipts);
    if pending_headers.is_empty() {
        mark_network_runtime_eth_peer_session_ready_v1(chain_id, peer.0, None);
        return Ok(true);
    }
    let pending_headers = filter_eth_fullnode_native_recovery_inflight_headers_v1(
        chain_id,
        peer.0,
        ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_RECEIPT_KIND_V1,
        pending_headers,
    );
    if pending_headers.is_empty() {
        return Ok(true);
    }
    let hashes = pending_headers
        .iter()
        .map(|pending| pending.hash)
        .collect::<Vec<_>>();
    let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
    let payload = eth_rlpx_build_get_receipts_payload_v1(
        request_id,
        0,
        hashes.as_slice(),
        session._negotiated_eth_version,
    );
    eth_rlpx_write_wire_frame_v1(
        &mut session.stream,
        &mut session.frame_session,
        ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_GET_RECEIPTS_MSG,
        payload.as_slice(),
    )
    .map_err(|err| {
        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            peer.0,
            "missing_receipts_recovery_request_write_failed",
            err.as_str(),
        );
        NetworkError::Io(err)
    })?;
    observe_network_runtime_eth_peer_syncing_v1(chain_id, peer.0);
    session.pending_body_headers = pending_headers;
    mark_eth_fullnode_native_recovery_inflight_v1(
        chain_id,
        peer.0,
        request_id,
        ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_RECEIPT_KIND_V1,
        session.pending_body_headers.as_slice(),
    );
    clear_eth_fullnode_native_headers_request_state_v1(session);
    session.last_bodies_request_id = None;
    session.last_receipts_request_id = Some(request_id);
    session.pending_receipt_request_offset = 0;
    clear_eth_fullnode_native_snap_request_state_v1(session);
    session.last_sync_request_unix_ms = now_unix_ms();
    report.sync_requests = report.sync_requests.saturating_add(1);
    eprintln!(
        "network_info: rlpx stage missing_receipts_requested chain_id={} peer={} endpoint={} request_id={} blocks={}",
        chain_id,
        peer.0,
        session.endpoint.addr_hint,
        request_id,
        session.pending_body_headers.len()
    );
    Ok(true)
}

fn eth_fullnode_native_should_probe_status_head_pivot_v1(
    chain_id: u64,
    session: &EthFullnodeNativeRlpxLivePeerSessionV1,
) -> bool {
    let remote_head = session.remote_status.latest_block;
    if remote_head == 0 || session.remote_status.latest_block_hash == [0u8; 32] {
        return false;
    }
    let Some(status) = get_network_runtime_sync_status(chain_id) else {
        return false;
    };
    remote_head.saturating_sub(status.current_block)
        >= ETH_FULLNODE_NATIVE_STATUS_HEAD_PIVOT_MIN_GAP_V1
}

fn dispatch_eth_fullnode_native_rlpx_status_head_pivot_probe_v1(
    chain_id: u64,
    peer: NodeId,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    report: &mut EthFullnodeNativeRlpxPeerTickReportV1,
) -> Result<bool, NetworkError> {
    if eth_fullnode_native_rlpx_session_has_pending_request_v1(session)
        || !eth_fullnode_native_should_probe_status_head_pivot_v1(chain_id, session)
    {
        return Ok(false);
    }
    let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
    let origin_hash = session.remote_status.latest_block_hash;
    let start_height = session.remote_status.latest_block;
    let payload =
        eth_rlpx_build_get_block_headers_by_hash_payload_v1(request_id, origin_hash, 1, 0, false);
    eth_rlpx_write_wire_frame_v1(
        &mut session.stream,
        &mut session.frame_session,
        ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG,
        payload.as_slice(),
    )
    .map_err(|err| {
        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            peer.0,
            "status_head_pivot_headers_request_write_failed",
            err.as_str(),
        );
        NetworkError::Io(err)
    })?;
    eprintln!(
        "network_info: rlpx stage status_head_pivot_headers_requested chain_id={} peer={} endpoint={} request_id={} remote_head={} hash=0x{}",
        chain_id,
        peer.0,
        session.endpoint.addr_hint,
        request_id,
        start_height,
        hex32_v1(&origin_hash)
    );
    observe_eth_native_headers_pull(chain_id);
    observe_network_runtime_eth_peer_syncing_v1(chain_id, peer.0);
    session.last_headers_request_id = Some(request_id);
    session.pending_headers_request = Some(EthRlpxGetBlockHeadersRequestV1 {
        request_id,
        start_height,
        origin_hash: Some(origin_hash),
        max_headers: 1,
        skip: 0,
        reverse: false,
    });
    session.last_bodies_request_id = None;
    session.pending_body_request_offset = 0;
    session.last_receipts_request_id = None;
    session.pending_receipt_request_offset = 0;
    clear_eth_fullnode_native_snap_request_state_v1(session);
    session.last_sync_request_unix_ms = now_unix_ms();
    report.sync_requests = report.sync_requests.saturating_add(1);
    Ok(true)
}

fn build_eth_fullnode_native_snap_account_range_response_payload_v1(
    chain_id: u64,
    request: &EthRlpxGetAccountRangeRequestV1,
) -> Vec<u8> {
    let fallback = || {
        let root_path = eth_fullnode_native_snap_root_trie_pathset_v1();
        let proof = get_network_runtime_native_snap_trie_node_snapshot_v1(
            chain_id,
            request.root,
            root_path.as_slice(),
        )
        .map(|snapshot| vec![snapshot.node_rlp])
        .unwrap_or_default();
        if !proof.is_empty()
            && eth_rlpx_mpt_proof_has_right_element_v1(
                request.root,
                request.origin.as_slice(),
                proof.as_slice(),
            )
            .is_ok_and(|has_right| !has_right)
            && eth_rlpx_mpt_verify_proof_value_v1(
                request.root,
                request.origin.as_slice(),
                proof.as_slice(),
            )
            .is_ok_and(|value| value.is_none())
        {
            return eth_rlpx_build_account_range_payload_v1(
                request.request_id,
                &[],
                proof.as_slice(),
            );
        }
        eth_rlpx_build_account_range_payload_v1(request.request_id, &[], &[])
    };

    let mut used = 0u64;
    let mut accounts = Vec::<crate::EthRlpxSnapAccountDataV1>::new();
    let mut proof = Vec::<Vec<u8>>::new();
    let mut seen_proof_nodes = HashSet::<[u8; 32]>::new();
    for snapshot in
        snapshot_network_runtime_native_snap_account_snapshots_v1(chain_id, request.root, 4096)
    {
        if snapshot.account_hash < request.origin || snapshot.account_hash > request.limit {
            continue;
        }
        if snapshot.body_rlp.is_empty() || snapshot.proof_nodes.is_empty() {
            break;
        }
        let next_len = snapshot
            .body_rlp
            .len()
            .saturating_add(snapshot.account_hash.len());
        if !eth_fullnode_native_snap_budget_allows_v1(used, next_len, request.byte_limit) {
            break;
        }
        used = used.saturating_add(next_len as u64);
        for node in &snapshot.proof_nodes {
            let node_hash = eth_rlpx_trie_node_hash_v1(node.as_slice());
            if !seen_proof_nodes.insert(node_hash) {
                continue;
            }
            if !eth_fullnode_native_snap_budget_allows_v1(used, node.len(), request.byte_limit) {
                return fallback();
            }
            used = used.saturating_add(node.len() as u64);
            proof.push(node.clone());
        }
        accounts.push(crate::EthRlpxSnapAccountDataV1 {
            hash: snapshot.account_hash,
            body_rlp: snapshot.body_rlp,
        });
    }
    if accounts.is_empty() || proof.is_empty() {
        return fallback();
    }
    let response = EthRlpxAccountRangeResponseV1 {
        request_id: request.request_id,
        accounts,
        proof,
    };
    if validate_snap_account_range_proof_semantics_v1(
        chain_id,
        0,
        Some(request.root),
        request.origin,
        request.limit,
        &response,
    )
    .is_err()
    {
        return fallback();
    }
    eth_rlpx_build_account_range_payload_v1(
        response.request_id,
        response.accounts.as_slice(),
        response.proof.as_slice(),
    )
}

fn eth_fullnode_native_snap_budget_allows_v1(used: u64, next_len: usize, byte_limit: u64) -> bool {
    byte_limit > 0 && used.saturating_add(next_len as u64) <= byte_limit
}

fn build_eth_fullnode_native_snap_storage_ranges_response_payload_v1(
    chain_id: u64,
    request: &EthRlpxGetStorageRangesRequestV1,
) -> Vec<u8> {
    let mut used = 0u64;
    let mut slots = Vec::<Vec<crate::EthRlpxSnapStorageDataV1>>::new();
    let mut proof = Vec::<Vec<u8>>::new();
    let mut seen_proof_nodes = HashSet::<[u8; 32]>::new();
    for account in &request.accounts {
        let Some(snapshot) = get_network_runtime_native_snap_account_storage_snapshot_v1(
            chain_id,
            request.root,
            *account,
        ) else {
            break;
        };
        let mut slotset = Vec::new();
        let mut snapshot_slots = snapshot.slots;
        snapshot_slots.sort_by(|a, b| a.hash.cmp(&b.hash));
        for slot in snapshot_slots {
            if !request.origin.is_empty() && slot.hash.as_slice() < request.origin.as_slice() {
                continue;
            }
            if !request.limit.is_empty() && slot.hash.as_slice() > request.limit.as_slice() {
                continue;
            }
            let next_len = slot.body.len().saturating_add(slot.hash.len());
            if !eth_fullnode_native_snap_budget_allows_v1(used, next_len, request.byte_limit) {
                break;
            }
            used = used.saturating_add(next_len as u64);
            slotset.push(crate::EthRlpxSnapStorageDataV1 {
                hash: slot.hash,
                body: slot.body,
            });
        }
        for node in snapshot.proof_nodes {
            let node_hash = eth_rlpx_trie_node_hash_v1(node.as_slice());
            if !seen_proof_nodes.insert(node_hash) {
                continue;
            }
            if !eth_fullnode_native_snap_budget_allows_v1(used, node.len(), request.byte_limit) {
                continue;
            }
            used = used.saturating_add(node.len() as u64);
            proof.push(node);
        }
        slots.push(slotset);
    }
    eth_rlpx_build_storage_ranges_payload_v1(request.request_id, slots.as_slice(), proof.as_slice())
}

fn build_eth_fullnode_native_snap_byte_codes_response_payload_v1(
    chain_id: u64,
    request: &EthRlpxGetByteCodesRequestV1,
) -> Vec<u8> {
    let mut used = 0u64;
    let mut codes = Vec::<Vec<u8>>::new();
    let mut seen = HashSet::<[u8; 32]>::new();
    for hash in &request.hashes {
        if !seen.insert(*hash) {
            continue;
        }
        let Some(snapshot) = get_network_runtime_native_snap_code_snapshot_v1(chain_id, *hash)
        else {
            break;
        };
        if eth_rlpx_code_hash_v1(snapshot.code.as_slice()) != *hash {
            break;
        }
        if !eth_fullnode_native_snap_budget_allows_v1(used, snapshot.code.len(), request.byte_limit)
        {
            break;
        }
        used = used.saturating_add(snapshot.code.len() as u64);
        codes.push(snapshot.code);
    }
    eth_rlpx_build_byte_codes_payload_v1(request.request_id, codes.as_slice())
}

fn build_eth_fullnode_native_snap_trie_nodes_response_payload_v1(
    chain_id: u64,
    request: &EthRlpxGetTrieNodesRequestV1,
) -> Vec<u8> {
    let mut used = 0u64;
    let mut nodes = Vec::<Vec<u8>>::new();
    for pathset in &request.paths {
        let Some(snapshot) =
            get_network_runtime_native_snap_trie_node_snapshot_v1(chain_id, request.root, pathset)
        else {
            break;
        };
        if eth_rlpx_trie_node_hash_v1(snapshot.node_rlp.as_slice()) != snapshot.node_hash {
            break;
        }
        if !eth_fullnode_native_snap_budget_allows_v1(
            used,
            snapshot.node_rlp.len(),
            request.byte_limit,
        ) {
            break;
        }
        used = used.saturating_add(snapshot.node_rlp.len() as u64);
        nodes.push(snapshot.node_rlp);
    }
    eth_rlpx_build_trie_nodes_payload_v1(request.request_id, nodes.as_slice())
}

fn eth_fullnode_native_rlpx_snap_root_hint_v1(chain_id: u64, fallback: [u8; 32]) -> [u8; 32] {
    get_network_runtime_native_head_snapshot_v1(chain_id)
        .map(|head| head.state_root)
        .unwrap_or(fallback)
}

fn eth_rlpx_account_hash_next_v1(mut value: [u8; 32]) -> Option<[u8; 32]> {
    for byte in value.iter_mut().rev() {
        if *byte < u8::MAX {
            *byte = byte.saturating_add(1);
            return Some(value);
        }
        *byte = 0;
    }
    None
}

fn eth_rlpx_snap_account_range_next_origin_v1(
    origin: [u8; 32],
    limit: [u8; 32],
    response: &EthRlpxAccountRangeResponseV1,
    has_continuation: bool,
) -> Result<Option<[u8; 32]>, NetworkError> {
    let Some(last) = response.accounts.last().map(|account| account.hash) else {
        return Ok(None);
    };
    let mut previous = None;
    for (idx, account) in response.accounts.iter().enumerate() {
        if account.hash < origin || account.hash > limit {
            return Err(NetworkError::Decode(format!(
                "snap_account_range_account_out_of_bounds:idx={} origin=0x{} limit=0x{} account=0x{}",
                idx,
                hex32_v1(&origin),
                hex32_v1(&limit),
                hex32_v1(&account.hash)
            )));
        }
        if previous.is_some_and(|prev| account.hash <= prev) {
            return Err(NetworkError::Decode(format!(
                "snap_account_range_account_not_monotonic:idx={} account=0x{}",
                idx,
                hex32_v1(&account.hash)
            )));
        }
        previous = Some(account.hash);
    }
    if !has_continuation {
        return Ok(None);
    }
    if last >= limit {
        return Ok(None);
    }
    Ok(eth_rlpx_account_hash_next_v1(last).filter(|next| *next <= limit))
}

fn eth_fullnode_native_snap_account_range_has_continuation_v1(
    chain_id: u64,
    source_peer_id: u64,
    state_root: Option<[u8; 32]>,
    response: &EthRlpxAccountRangeResponseV1,
) -> Result<bool, NetworkError> {
    let Some(last_account) = response.accounts.last() else {
        return Ok(false);
    };
    let Some(state_root) = state_root else {
        let reason = "snap_account_range_state_root_missing_for_continuation".to_string();
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    };
    eth_rlpx_mpt_proof_has_right_element_v1(
        state_root,
        last_account.hash.as_slice(),
        response.proof.as_slice(),
    )
    .map_err(|err| {
        let reason = format!(
            "snap_account_range_continuation_verify_failed:last=0x{} err={}",
            hex32_v1(&last_account.hash),
            err
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        NetworkError::Decode(reason)
    })
}

fn clear_eth_fullnode_native_snap_request_state_v1(
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
) {
    session.last_snap_account_range_request_id = None;
    session.last_snap_storage_ranges_request_id = None;
    session.last_snap_byte_codes_request_id = None;
    session.last_snap_trie_nodes_request_id = None;
    session.last_snap_state_root = None;
    session.last_snap_account_origin = None;
    session.last_snap_account_limit = None;
    session.pending_snap_next_account_origin = None;
    session.pending_snap_storage_accounts.clear();
    session.pending_snap_storage_origin.clear();
    session.pending_snap_storage_limit.clear();
    session.pending_snap_storage_deferred_accounts.clear();
    session.pending_snap_code_hashes.clear();
    session.pending_snap_trie_node_pathsets.clear();
    session.pending_snap_trie_node_hashes.clear();
    session.pending_snap_trie_node_retry_count = 0;
}

fn clear_eth_fullnode_native_headers_request_state_v1(
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
) {
    session.last_headers_request_id = None;
    session.pending_headers_request = None;
}

fn clear_eth_fullnode_native_block_access_list_request_state_v1(
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
) {
    session.last_block_access_lists_request_id = None;
    session.pending_block_access_lists.clear();
}

fn eth_fullnode_native_rlpx_session_has_pending_request_v1(
    session: &EthFullnodeNativeRlpxLivePeerSessionV1,
) -> bool {
    session.last_headers_request_id.is_some()
        || session.last_bodies_request_id.is_some()
        || session.last_receipts_request_id.is_some()
        || session.last_snap_account_range_request_id.is_some()
        || session.last_snap_storage_ranges_request_id.is_some()
        || session.last_snap_byte_codes_request_id.is_some()
        || session.last_snap_trie_nodes_request_id.is_some()
        || session.last_block_access_lists_request_id.is_some()
        || !session.pending_body_headers.is_empty()
}

fn eth_fullnode_native_budget_capped_headers_batch_v1(
    requested: u64,
    budget_hooks: &EthFullnodeBudgetHooksV1,
) -> u64 {
    requested
        .min(budget_hooks.sync_pull_headers_batch.max(1))
        .max(1)
}

fn eth_fullnode_native_rlpx_supports_block_access_lists_v1(
    session: &EthFullnodeNativeRlpxLivePeerSessionV1,
) -> bool {
    session._negotiated_eth_version >= 71 || session._negotiated_snap_version.is_none()
}

fn queue_eth_fullnode_native_block_access_list_hash_v1(
    chain_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    block_hash: [u8; 32],
    block_access_list_hash: Option<[u8; 32]>,
    gas_limit: Option<u64>,
    tx_count: Option<usize>,
) {
    let Some(block_access_list_hash) = block_access_list_hash else {
        return;
    };
    if get_network_runtime_native_block_access_list_payload_v1(chain_id, block_hash).is_some()
        || session
            .queued_block_access_lists
            .iter()
            .any(|pending| pending.block_hash == block_hash)
        || session
            .pending_block_access_lists
            .iter()
            .any(|pending| pending.block_hash == block_hash)
    {
        return;
    }
    session
        .queued_block_access_lists
        .push(EthFullnodeNativePendingBlockAccessListV1 {
            block_hash,
            block_access_list_hash,
            gas_limit,
            tx_count,
        });
}

fn update_eth_fullnode_native_block_access_list_body_context_v1(
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    block_hash: [u8; 32],
    tx_count: usize,
) {
    for pending in session
        .queued_block_access_lists
        .iter_mut()
        .chain(session.pending_block_access_lists.iter_mut())
    {
        if pending.block_hash == block_hash {
            pending.tx_count = Some(tx_count);
        }
    }
}

fn dispatch_eth_fullnode_native_rlpx_queued_block_access_lists_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    report: &mut EthFullnodeNativeRlpxPeerTickReportV1,
) -> Result<bool, NetworkError> {
    if session.last_block_access_lists_request_id.is_some()
        || session.queued_block_access_lists.is_empty()
    {
        return Ok(false);
    }
    if !eth_fullnode_native_rlpx_supports_block_access_lists_v1(session) {
        session.queued_block_access_lists.clear();
        return Ok(false);
    }
    let mut seen = HashSet::<[u8; 32]>::new();
    let pending = session
        .queued_block_access_lists
        .drain(..)
        .filter(|pending| seen.insert(pending.block_hash))
        .filter(|pending| {
            get_network_runtime_native_block_access_list_payload_v1(chain_id, pending.block_hash)
                .is_none()
        })
        .collect::<Vec<_>>();
    if pending.is_empty() {
        return Ok(false);
    }
    let hashes = pending
        .iter()
        .map(|pending| pending.block_hash)
        .collect::<Vec<_>>();
    let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
    let payload = eth_rlpx_build_get_block_access_lists_payload_v1(request_id, hashes.as_slice());
    eth_rlpx_write_wire_frame_v1(
        &mut session.stream,
        &mut session.frame_session,
        ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_GET_BLOCK_ACCESS_LISTS_MSG,
        payload.as_slice(),
    )
    .map_err(|err| {
        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            source_peer_id,
            "block_access_lists_request_write_failed",
            err.as_str(),
        );
        NetworkError::Io(err)
    })?;
    observe_network_runtime_eth_peer_syncing_v1(chain_id, source_peer_id);
    session.last_block_access_lists_request_id = Some(request_id);
    session.pending_block_access_lists = pending;
    session.last_sync_request_unix_ms = now_unix_ms();
    report.sync_requests = report.sync_requests.saturating_add(1);
    Ok(true)
}

fn dispatch_eth_fullnode_native_snap_account_range_request_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    root: [u8; 32],
    origin: [u8; 32],
    limit: [u8; 32],
    byte_limit: u64,
    failure_reason: &'static str,
) -> Result<u64, NetworkError> {
    let Some(snap_offset) = eth_rlpx_snap_base_offset_v1(
        session._negotiated_eth_version,
        session._negotiated_snap_version,
    ) else {
        return Err(NetworkError::Decode(
            "snap_account_range_without_negotiated_snap".to_string(),
        ));
    };
    let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
    let payload =
        eth_rlpx_build_get_account_range_payload_v1(request_id, root, origin, limit, byte_limit);
    eth_rlpx_write_wire_frame_v1(
        &mut session.stream,
        &mut session.frame_session,
        snap_offset + ETH_RLPX_SNAP_GET_ACCOUNT_RANGE_MSG,
        payload.as_slice(),
    )
    .map_err(|err| {
        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            source_peer_id,
            failure_reason,
            err.as_str(),
        );
        NetworkError::Io(err)
    })?;
    observe_eth_native_snap_pull(chain_id);
    observe_network_runtime_eth_peer_syncing_v1(chain_id, source_peer_id);
    clear_eth_fullnode_native_headers_request_state_v1(session);
    session.last_bodies_request_id = None;
    session.last_receipts_request_id = None;
    session.pending_receipt_request_offset = 0;
    session.last_snap_storage_ranges_request_id = None;
    session.last_snap_byte_codes_request_id = None;
    session.last_snap_trie_nodes_request_id = None;
    session.last_snap_state_root = Some(root);
    session.last_snap_account_origin = Some(origin);
    session.last_snap_account_limit = Some(limit);
    session.pending_snap_next_account_origin = None;
    session.pending_snap_storage_accounts.clear();
    session.pending_snap_storage_origin.clear();
    session.pending_snap_storage_limit.clear();
    session.pending_snap_storage_deferred_accounts.clear();
    session.pending_snap_code_hashes.clear();
    session.pending_snap_trie_node_pathsets.clear();
    session.pending_snap_trie_node_hashes.clear();
    session.pending_snap_trie_node_retry_count = 0;
    session.last_snap_account_range_request_id = Some(request_id);
    session.last_sync_request_unix_ms = now_unix_ms();
    Ok(request_id)
}

fn record_eth_fullnode_native_snap_account_range_progress_v1(
    chain_id: u64,
    source_peer_id: u64,
    root: [u8; 32],
    next_account_origin: Option<[u8; 32]>,
    limit: [u8; 32],
    completed: bool,
) {
    set_network_runtime_native_snap_account_range_progress_v1(
        chain_id,
        NetworkRuntimeNativeSnapAccountRangeProgressV1 {
            chain_id,
            state_root: root,
            next_account_origin,
            limit,
            completed,
            source_peer_id: Some(source_peer_id),
            observed_unix_ms: now_unix_ms() as u128,
        },
    );
}

fn eth_fullnode_native_snap_root_trie_pathset_v1() -> Vec<Vec<u8>> {
    vec![vec![0u8]]
}

fn eth_fullnode_native_snap_storage_root_trie_pathset_v1(account_hash: [u8; 32]) -> Vec<Vec<u8>> {
    vec![account_hash.to_vec(), vec![0u8]]
}

fn request_eth_fullnode_native_snap_trie_nodes_batch_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    root: [u8; 32],
    pending: &[(Vec<Vec<u8>>, [u8; 32])],
    retry_count: u8,
) -> Result<bool, NetworkError> {
    if session.last_snap_trie_nodes_request_id.is_some() {
        return Ok(false);
    }
    if pending.is_empty() {
        return Ok(false);
    }
    let Some(snap_offset) = eth_rlpx_snap_base_offset_v1(
        session._negotiated_eth_version,
        session._negotiated_snap_version,
    ) else {
        return Ok(false);
    };
    let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
    let pathsets = pending
        .iter()
        .map(|(pathset, _)| pathset.clone())
        .collect::<Vec<_>>();
    let expected_hashes = pending
        .iter()
        .map(|(_, expected_hash)| *expected_hash)
        .collect::<Vec<_>>();
    let payload = eth_rlpx_build_get_trie_nodes_payload_v1(
        request_id,
        root,
        pathsets.as_slice(),
        ETH_RLPX_SNAP_DEFAULT_ACCOUNT_RANGE_BYTES,
    );
    eth_rlpx_write_wire_frame_v1(
        &mut session.stream,
        &mut session.frame_session,
        snap_offset + ETH_RLPX_SNAP_GET_TRIE_NODES_MSG,
        payload.as_slice(),
    )
    .map_err(|err| {
        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            source_peer_id,
            "snap_trie_nodes_request_write_failed",
            err.as_str(),
        );
        NetworkError::Io(err)
    })?;
    observe_eth_native_snap_pull(chain_id);
    observe_network_runtime_eth_peer_syncing_v1(chain_id, source_peer_id);
    session.last_snap_trie_nodes_request_id = Some(request_id);
    session.pending_snap_trie_node_pathsets = pathsets;
    session.pending_snap_trie_node_hashes = expected_hashes;
    session.pending_snap_trie_node_retry_count = retry_count;
    session.last_sync_request_unix_ms = now_unix_ms();
    eprintln!(
        "network_info: rlpx stage snap_trie_nodes_requested chain_id={} peer={} endpoint={} request_id={} paths={} retry={} root=0x{}",
        chain_id,
        source_peer_id,
        session.endpoint.addr_hint,
        request_id,
        session.pending_snap_trie_node_pathsets.len(),
        retry_count,
        hex32_v1(&root),
    );
    Ok(true)
}

fn maybe_request_eth_fullnode_native_snap_trie_nodes_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    candidates: &[(Vec<Vec<u8>>, [u8; 32])],
) -> Result<bool, NetworkError> {
    let Some(root) = session.last_snap_state_root else {
        return Ok(false);
    };
    let empty_root = crate::eth_rlpx_empty_trie_root_v1();
    let root_path = eth_fullnode_native_snap_root_trie_pathset_v1();
    let mut pending = Vec::<(Vec<Vec<u8>>, [u8; 32])>::new();
    if get_network_runtime_native_snap_trie_node_snapshot_v1(chain_id, root, root_path.as_slice())
        .is_none()
    {
        pending.push((root_path, root));
    }
    for (pathset, expected_hash) in candidates {
        if *expected_hash == empty_root
            || pending.iter().any(|(existing, _)| existing == pathset)
            || get_network_runtime_native_snap_trie_node_snapshot_v1(
                chain_id,
                root,
                pathset.as_slice(),
            )
            .is_some()
        {
            continue;
        }
        pending.push((pathset.clone(), *expected_hash));
    }
    request_eth_fullnode_native_snap_trie_nodes_batch_v1(
        chain_id,
        source_peer_id,
        session,
        root,
        pending.as_slice(),
        0,
    )
}

fn maybe_continue_eth_fullnode_native_snap_account_range_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
) -> Result<bool, NetworkError> {
    if session.last_snap_account_range_request_id.is_some()
        || session.last_snap_storage_ranges_request_id.is_some()
        || session.last_snap_byte_codes_request_id.is_some()
        || session.last_snap_trie_nodes_request_id.is_some()
    {
        return Ok(false);
    }
    let Some(root) = session.last_snap_state_root else {
        return Ok(false);
    };
    let Some(origin) = session.pending_snap_next_account_origin.take() else {
        let limit = session.last_snap_account_limit.unwrap_or([0xffu8; 32]);
        record_eth_fullnode_native_snap_account_range_progress_v1(
            chain_id,
            source_peer_id,
            root,
            None,
            limit,
            true,
        );
        clear_eth_fullnode_native_snap_request_state_v1(session);
        return Ok(false);
    };
    let limit = session.last_snap_account_limit.unwrap_or([0xffu8; 32]);
    if origin > limit {
        record_eth_fullnode_native_snap_account_range_progress_v1(
            chain_id,
            source_peer_id,
            root,
            None,
            limit,
            true,
        );
        clear_eth_fullnode_native_snap_request_state_v1(session);
        return Ok(false);
    }
    record_eth_fullnode_native_snap_account_range_progress_v1(
        chain_id,
        source_peer_id,
        root,
        Some(origin),
        limit,
        false,
    );
    let request_id = dispatch_eth_fullnode_native_snap_account_range_request_v1(
        chain_id,
        source_peer_id,
        session,
        root,
        origin,
        limit,
        ETH_RLPX_SNAP_DEFAULT_ACCOUNT_RANGE_BYTES,
        "snap_account_range_continuation_request_write_failed",
    )?;
    eprintln!(
        "network_info: rlpx stage snap_account_range_continuation_requested chain_id={} peer={} endpoint={} request_id={} origin=0x{} limit=0x{} root=0x{}",
        chain_id,
        source_peer_id,
        session.endpoint.addr_hint,
        request_id,
        hex32_v1(&origin),
        hex32_v1(&limit),
        hex32_v1(&root),
    );
    Ok(true)
}

fn match_eth_fullnode_native_snap_trie_nodes_v1(
    expected_hashes: &[[u8; 32]],
    nodes: &[Vec<u8>],
) -> Result<Vec<(usize, [u8; 32])>, String> {
    let mut matched = Vec::new();
    let mut search_from = 0usize;
    for (response_idx, node) in nodes.iter().enumerate() {
        if !eth_rlpx_validate_trie_node_rlp_v1(node.as_slice()) {
            return Err(format!(
                "snap_trie_nodes_node_rlp_invalid:idx={response_idx}"
            ));
        }
        let node_hash = eth_rlpx_trie_node_hash_v1(node.as_slice());
        let mut matched_index = None;
        while search_from < expected_hashes.len() {
            if expected_hashes[search_from] == node_hash {
                matched_index = Some(search_from);
                search_from = search_from.saturating_add(1);
                break;
            }
            search_from = search_from.saturating_add(1);
        }
        let Some(expected_idx) = matched_index else {
            return Err(format!(
                "snap_trie_nodes_unexpected_hash:idx={} got=0x{}",
                response_idx,
                hex32_v1(&node_hash)
            ));
        };
        matched.push((expected_idx, node_hash));
    }
    Ok(matched)
}

fn build_eth_fullnode_native_pooled_transactions_response_v1(
    chain_id: u64,
    hashes: &[[u8; 32]],
) -> Vec<Vec<u8>> {
    hashes
        .iter()
        .filter_map(|hash| get_network_runtime_native_pending_tx_payload_v1(chain_id, *hash))
        .collect()
}

fn ingest_real_rlpx_pooled_transactions_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    txs: &EthRlpxPooledTransactionsPayloadV1,
) -> Result<(), NetworkError> {
    let Some(request_id) = session.last_pooled_transactions_request_id else {
        let err = format!(
            "rlpx_pooled_transactions_unexpected_response:request_id={}",
            txs.request_id
        );
        observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
        return Err(NetworkError::Decode(err));
    };
    if request_id != txs.request_id {
        return Ok(());
    }
    validate_eth_fullnode_native_pooled_transactions_match_request_v1(
        session.pending_pooled_transaction_hashes.as_slice(),
        txs.tx_hashes.as_slice(),
    )
    .map_err(|err| {
        observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
        NetworkError::Decode(err)
    })?;
    for (idx, tx_hash) in txs.tx_hashes.iter().enumerate() {
        let tx_payload = txs.tx_rlp_items.get(idx).map(|item| item.as_slice());
        observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
            chain_id,
            source_peer_id,
            *tx_hash,
            tx_payload,
        );
    }
    session.last_pooled_transactions_request_id = None;
    session.pending_pooled_transaction_hashes.clear();
    Ok(())
}

fn validate_eth_fullnode_native_pooled_transactions_match_request_v1(
    requested_hashes: &[[u8; 32]],
    response_hashes: &[[u8; 32]],
) -> Result<(), String> {
    let mut next_search_from = 0usize;
    let mut seen = HashSet::<[u8; 32]>::new();
    for response_hash in response_hashes {
        if !seen.insert(*response_hash) {
            return Err(format!(
                "rlpx_pooled_transactions_duplicate_hash:hash=0x{}",
                hex32_v1(response_hash)
            ));
        }
        let Some(relative_idx) = requested_hashes
            .iter()
            .skip(next_search_from)
            .position(|requested| requested == response_hash)
        else {
            return Err(format!(
                "rlpx_pooled_transactions_unrequested_hash:hash=0x{}",
                hex32_v1(response_hash)
            ));
        };
        next_search_from = next_search_from
            .saturating_add(relative_idx)
            .saturating_add(1);
    }
    Ok(())
}

fn validate_eth_fullnode_native_block_access_list_commitment_v1(
    pending: &EthFullnodeNativePendingBlockAccessListV1,
    raw_rlp: &[u8],
) -> Result<[u8; 32], String> {
    let gas_limit = pending.gas_limit.ok_or_else(|| {
        format!(
            "rlpx_block_access_list_context_missing:block=0x{}:gas_limit",
            hex32_v1(&pending.block_hash)
        )
    })?;
    let tx_count = pending.tx_count.ok_or_else(|| {
        format!(
            "rlpx_block_access_list_context_missing:block=0x{}:tx_count",
            hex32_v1(&pending.block_hash)
        )
    })?;
    eth_rlpx_validate_block_access_list_rlp_context_v1(raw_rlp, gas_limit, tx_count).map_err(
        |err| {
            format!(
                "rlpx_block_access_list_payload_invalid:block=0x{}:{err}",
                hex32_v1(&pending.block_hash)
            )
        },
    )?;
    let observed_hash =
        eth_rlpx_block_access_list_hash_from_raw_rlp_v1(raw_rlp).map_err(|err| {
            format!(
                "rlpx_block_access_list_payload_invalid:block=0x{}:{err}",
                hex32_v1(&pending.block_hash)
            )
        })?;
    if observed_hash != pending.block_access_list_hash {
        return Err(format!(
            "rlpx_block_access_list_hash_mismatch:block=0x{}:expected=0x{}:observed=0x{}",
            hex32_v1(&pending.block_hash),
            hex32_v1(&pending.block_access_list_hash),
            hex32_v1(&observed_hash)
        ));
    }
    Ok(observed_hash)
}

fn ingest_real_rlpx_block_access_lists_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    response: &EthRlpxBlockAccessListsResponseV1,
) -> Result<(), NetworkError> {
    let Some(request_id) = session.last_block_access_lists_request_id else {
        let err = format!(
            "rlpx_block_access_lists_unexpected_response:request_id={}",
            response.request_id
        );
        observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
        return Err(NetworkError::Decode(err));
    };
    if request_id != response.request_id {
        return Ok(());
    }
    if response.lists.len() > session.pending_block_access_lists.len() {
        let err = format!(
            "rlpx_block_access_lists_count_mismatch:expected={} observed={}",
            session.pending_block_access_lists.len(),
            response.lists.len()
        );
        observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
        return Err(NetworkError::Decode(err));
    }
    let mut materialized = 0usize;
    let mut missing = 0usize;
    for (pending, item) in session
        .pending_block_access_lists
        .iter()
        .zip(response.lists.iter())
    {
        let Some(raw_rlp) = item.raw_rlp.as_deref() else {
            missing = missing.saturating_add(1);
            continue;
        };
        validate_eth_fullnode_native_block_access_list_commitment_v1(pending, raw_rlp).map_err(
            |reason| {
                observe_network_runtime_eth_peer_decode_failure_v1(
                    chain_id,
                    source_peer_id,
                    reason.as_str(),
                );
                NetworkError::Decode(reason)
            },
        )?;
        set_network_runtime_native_block_access_list_payload_v1(
            chain_id,
            pending.block_hash,
            raw_rlp,
        )
        .map_err(|err| {
            let reason = format!(
                "rlpx_block_access_list_payload_invalid:block=0x{}:{err}",
                hex32_v1(&pending.block_hash)
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            NetworkError::Decode(reason)
        })?;
        materialized = materialized.saturating_add(1);
    }
    missing = missing.saturating_add(
        session
            .pending_block_access_lists
            .len()
            .saturating_sub(response.lists.len()),
    );
    eprintln!(
        "network_info: rlpx stage block_access_lists_received chain_id={} peer={} endpoint={} negotiated_eth={} request_id={} lists={} materialized={} missing={}",
        chain_id,
        source_peer_id,
        session.endpoint.addr_hint,
        session._negotiated_eth_version,
        response.request_id,
        response.lists.len(),
        materialized,
        missing,
    );
    clear_eth_fullnode_native_block_access_list_request_state_v1(session);
    session.pending_body_headers.clear();
    mark_network_runtime_eth_peer_session_ready_v1(chain_id, source_peer_id, None);
    Ok(())
}

fn validate_snap_proof_nodes_match_roots_v1(
    chain_id: u64,
    source_peer_id: u64,
    kind: &str,
    expected_roots: &[[u8; 32]],
    proof: &[Vec<u8>],
) -> Result<(), NetworkError> {
    if expected_roots.is_empty() {
        let reason = format!(
            "{kind}_proof_expected_roots_missing:proof_nodes={}",
            proof.len()
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    }
    let mut root_seen = vec![false; expected_roots.len()];
    for (idx, node) in proof.iter().enumerate() {
        if !eth_rlpx_validate_trie_node_rlp_v1(node.as_slice()) {
            let reason = format!("{kind}_proof_node_rlp_invalid:idx={idx}");
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        }
        let node_hash = eth_rlpx_trie_node_hash_v1(node.as_slice());
        for (root_idx, expected_root) in expected_roots.iter().enumerate() {
            if *expected_root == node_hash {
                root_seen[root_idx] = true;
            }
        }
    }
    let empty_root = crate::eth_rlpx_empty_trie_root_v1();
    for (idx, expected_root) in expected_roots.iter().enumerate() {
        if *expected_root == empty_root {
            if !proof.is_empty() {
                let reason = format!(
                    "{kind}_proof_unexpected_for_empty_root:idx={} proof_nodes={}",
                    idx,
                    proof.len()
                );
                observe_network_runtime_eth_peer_decode_failure_v1(
                    chain_id,
                    source_peer_id,
                    reason.as_str(),
                );
                return Err(NetworkError::Decode(reason));
            }
            continue;
        }
        if !root_seen[idx] {
            let reason = format!(
                "{kind}_proof_root_missing:idx={} root=0x{} proof_nodes={}",
                idx,
                hex32_v1(expected_root),
                proof.len()
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        }
    }
    Ok(())
}

fn snap_proof_value_missing_is_tolerated_v1(err: &str) -> bool {
    err.contains("rlpx_mpt_proof_node_missing") || err.contains("rlpx_mpt_proof_root_missing")
}

fn validate_snap_account_range_response_preconditions_v1(
    chain_id: u64,
    source_peer_id: u64,
    origin: [u8; 32],
    limit: [u8; 32],
    response: &EthRlpxAccountRangeResponseV1,
) -> Result<(), NetworkError> {
    let mut previous = None;
    for (idx, account) in response.accounts.iter().enumerate() {
        if account.body_rlp.is_empty() {
            let reason = format!(
                "snap_account_range_deletion:idx={} account=0x{}",
                idx,
                hex32_v1(&account.hash)
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        }
        if account.hash < origin || account.hash > limit {
            let reason = format!(
                "snap_account_range_account_out_of_bounds:idx={} origin=0x{} limit=0x{} account=0x{}",
                idx,
                hex32_v1(&origin),
                hex32_v1(&limit),
                hex32_v1(&account.hash)
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        }
        if previous.is_some_and(|prev| account.hash <= prev) {
            let reason = format!(
                "snap_account_range_account_not_monotonic:idx={} account=0x{}",
                idx,
                hex32_v1(&account.hash)
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        }
        eth_rlpx_snap_full_account_rlp_from_slim_v1(account.body_rlp.as_slice()).map_err(
            |err| {
                let reason = format!(
                    "snap_account_range_account_body_invalid:idx={} account=0x{} err={}",
                    idx,
                    hex32_v1(&account.hash),
                    err
                );
                observe_network_runtime_eth_peer_decode_failure_v1(
                    chain_id,
                    source_peer_id,
                    reason.as_str(),
                );
                NetworkError::Decode(reason)
            },
        )?;
        previous = Some(account.hash);
    }
    Ok(())
}

fn validate_snap_storage_ranges_response_preconditions_v1(
    chain_id: u64,
    source_peer_id: u64,
    origin: &[u8],
    limit: &[u8],
    response: &EthRlpxStorageRangesResponseV1,
) -> Result<(), NetworkError> {
    for (slotset_idx, slots) in response.slots.iter().enumerate() {
        let mut previous = None;
        for (idx, slot) in slots.iter().enumerate() {
            if slot.body.is_empty() {
                let reason = format!(
                    "snap_storage_ranges_deletion:slotset={} idx={} slot=0x{}",
                    slotset_idx,
                    idx,
                    hex32_v1(&slot.hash)
                );
                observe_network_runtime_eth_peer_decode_failure_v1(
                    chain_id,
                    source_peer_id,
                    reason.as_str(),
                );
                return Err(NetworkError::Decode(reason));
            }
            if !origin.is_empty() && slot.hash.as_slice() < origin {
                let reason = format!(
                    "snap_storage_ranges_slot_before_origin:slotset={} idx={} slot=0x{}",
                    slotset_idx,
                    idx,
                    hex32_v1(&slot.hash)
                );
                observe_network_runtime_eth_peer_decode_failure_v1(
                    chain_id,
                    source_peer_id,
                    reason.as_str(),
                );
                return Err(NetworkError::Decode(reason));
            }
            if !limit.is_empty() && slot.hash.as_slice() > limit {
                let reason = format!(
                    "snap_storage_ranges_slot_after_limit:slotset={} idx={} slot=0x{}",
                    slotset_idx,
                    idx,
                    hex32_v1(&slot.hash)
                );
                observe_network_runtime_eth_peer_decode_failure_v1(
                    chain_id,
                    source_peer_id,
                    reason.as_str(),
                );
                return Err(NetworkError::Decode(reason));
            }
            if previous.is_some_and(|prev| slot.hash <= prev) {
                let reason = format!(
                    "snap_storage_ranges_slot_not_monotonic:slotset={} idx={} slot=0x{}",
                    slotset_idx,
                    idx,
                    hex32_v1(&slot.hash)
                );
                observe_network_runtime_eth_peer_decode_failure_v1(
                    chain_id,
                    source_peer_id,
                    reason.as_str(),
                );
                return Err(NetworkError::Decode(reason));
            }
            previous = Some(slot.hash);
        }
    }
    Ok(())
}

fn validate_snap_account_range_proof_values_v1(
    chain_id: u64,
    source_peer_id: u64,
    root: [u8; 32],
    response: &EthRlpxAccountRangeResponseV1,
) -> Result<(), NetworkError> {
    let mut indices = Vec::new();
    if !response.accounts.is_empty() {
        indices.push(0usize);
        let last = response.accounts.len().saturating_sub(1);
        if last != 0 {
            indices.push(last);
        }
    }
    for idx in indices {
        let account = &response.accounts[idx];
        let proven = match eth_rlpx_mpt_verify_proof_value_v1(
            root,
            account.hash.as_slice(),
            response.proof.as_slice(),
        ) {
            Ok(value) => value,
            Err(err) if snap_proof_value_missing_is_tolerated_v1(err.as_str()) => continue,
            Err(err) => {
                let reason = format!(
                    "snap_account_range_proof_value_verify_failed:idx={} account=0x{} err={}",
                    idx,
                    hex32_v1(&account.hash),
                    err
                );
                observe_network_runtime_eth_peer_decode_failure_v1(
                    chain_id,
                    source_peer_id,
                    reason.as_str(),
                );
                return Err(NetworkError::Decode(reason));
            }
        };
        let Some(proven_value) = proven else {
            continue;
        };
        let expected =
            eth_rlpx_snap_full_account_rlp_from_slim_v1(account.body_rlp.as_slice()).map_err(
                |err| {
                    let reason = format!(
                        "snap_account_range_proof_value_account_decode_failed:idx={} account=0x{} err={}",
                        idx,
                        hex32_v1(&account.hash),
                        err
                    );
                    observe_network_runtime_eth_peer_decode_failure_v1(
                        chain_id,
                        source_peer_id,
                        reason.as_str(),
                    );
                    NetworkError::Decode(reason)
                },
            )?;
        if proven_value != expected {
            let reason = format!(
                "snap_account_range_proof_value_mismatch:idx={} account=0x{} expected_bytes={} proven_bytes={}",
                idx,
                hex32_v1(&account.hash),
                expected.len(),
                proven_value.len()
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        }
    }
    Ok(())
}

fn validate_snap_account_range_empty_proof_no_more_v1(
    chain_id: u64,
    source_peer_id: u64,
    root: [u8; 32],
    origin: [u8; 32],
    response: &EthRlpxAccountRangeResponseV1,
) -> Result<(), NetworkError> {
    if !response.accounts.is_empty() {
        return Ok(());
    }
    let has_right =
        eth_rlpx_mpt_proof_has_right_element_v1(root, origin.as_slice(), response.proof.as_slice())
            .map_err(|err| {
                let reason = format!(
                    "snap_account_range_empty_proof_no_more_verify_failed:origin=0x{} err={}",
                    hex32_v1(&origin),
                    err
                );
                observe_network_runtime_eth_peer_decode_failure_v1(
                    chain_id,
                    source_peer_id,
                    reason.as_str(),
                );
                NetworkError::Decode(reason)
            })?;
    let proven_at_origin =
        eth_rlpx_mpt_verify_proof_value_v1(root, origin.as_slice(), response.proof.as_slice())
            .map_err(|err| {
                let reason = format!(
                    "snap_account_range_empty_proof_origin_value_verify_failed:origin=0x{} err={}",
                    hex32_v1(&origin),
                    err
                );
                observe_network_runtime_eth_peer_decode_failure_v1(
                    chain_id,
                    source_peer_id,
                    reason.as_str(),
                );
                NetworkError::Decode(reason)
            })?
            .is_some();
    if has_right || proven_at_origin {
        let reason = format!(
            "snap_account_range_empty_proof_more_entries:origin=0x{} has_right={} origin_value={}",
            hex32_v1(&origin),
            has_right,
            proven_at_origin
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    }
    Ok(())
}

fn validate_snap_account_range_origin_value_v1(
    chain_id: u64,
    source_peer_id: u64,
    root: [u8; 32],
    origin: [u8; 32],
    response: &EthRlpxAccountRangeResponseV1,
) -> Result<(), NetworkError> {
    let Some(first) = response.accounts.first() else {
        return Ok(());
    };
    let proven = match eth_rlpx_mpt_verify_proof_value_v1(
        root,
        origin.as_slice(),
        response.proof.as_slice(),
    ) {
        Ok(value) => value,
        Err(err) if snap_proof_value_missing_is_tolerated_v1(err.as_str()) => return Ok(()),
        Err(err) => {
            let reason = format!(
                "snap_account_range_origin_value_verify_failed:origin=0x{} err={}",
                hex32_v1(&origin),
                err
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        }
    };
    if proven.is_some() && first.hash != origin {
        let reason = format!(
            "snap_account_range_origin_value_omitted:origin=0x{} first=0x{}",
            hex32_v1(&origin),
            hex32_v1(&first.hash)
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    }
    Ok(())
}

fn validate_snap_account_range_left_boundary_v1(
    chain_id: u64,
    source_peer_id: u64,
    root: [u8; 32],
    origin: [u8; 32],
    response: &EthRlpxAccountRangeResponseV1,
) -> Result<(), NetworkError> {
    let Some(first) = response.accounts.first() else {
        return Ok(());
    };
    if first.hash <= origin {
        return Ok(());
    }
    let has_left_gap = eth_rlpx_mpt_proof_has_element_in_range_v1(
        root,
        origin.as_slice(),
        first.hash.as_slice(),
        response.proof.as_slice(),
    )
    .map_err(|err| {
        let reason = format!(
            "snap_account_range_left_boundary_verify_failed:origin=0x{} first=0x{} err={}",
            hex32_v1(&origin),
            hex32_v1(&first.hash),
            err
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        NetworkError::Decode(reason)
    })?;
    if has_left_gap {
        let reason = format!(
            "snap_account_range_left_gap:origin=0x{} first=0x{}",
            hex32_v1(&origin),
            hex32_v1(&first.hash)
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    }
    Ok(())
}

fn validate_snap_account_range_internal_gaps_v1(
    chain_id: u64,
    source_peer_id: u64,
    root: [u8; 32],
    response: &EthRlpxAccountRangeResponseV1,
) -> Result<(), NetworkError> {
    for pair in response.accounts.windows(2) {
        let previous = pair[0].hash;
        let current = pair[1].hash;
        let Some(lower) = eth_rlpx_account_hash_next_v1(previous) else {
            continue;
        };
        if lower >= current {
            continue;
        }
        let has_gap = eth_rlpx_mpt_proof_has_element_in_range_v1(
            root,
            lower.as_slice(),
            current.as_slice(),
            response.proof.as_slice(),
        )
        .map_err(|err| {
            let reason = format!(
                "snap_account_range_internal_gap_verify_failed:previous=0x{} current=0x{} err={}",
                hex32_v1(&previous),
                hex32_v1(&current),
                err
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            NetworkError::Decode(reason)
        })?;
        if has_gap {
            let reason = format!(
                "snap_account_range_internal_gap:previous=0x{} current=0x{}",
                hex32_v1(&previous),
                hex32_v1(&current)
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        }
    }
    Ok(())
}

fn validate_snap_storage_range_slotset_proof_values_v1(
    chain_id: u64,
    source_peer_id: u64,
    slotset_idx: usize,
    root: [u8; 32],
    slots: &[crate::EthRlpxSnapStorageDataV1],
    proof: &[Vec<u8>],
) -> Result<(), NetworkError> {
    let mut indices = Vec::new();
    if !slots.is_empty() {
        indices.push(0usize);
        let last = slots.len().saturating_sub(1);
        if last != 0 {
            indices.push(last);
        }
    }
    for idx in indices {
        let slot = &slots[idx];
        let proven = match eth_rlpx_mpt_verify_proof_value_v1(root, slot.hash.as_slice(), proof) {
            Ok(value) => value,
            Err(err) if snap_proof_value_missing_is_tolerated_v1(err.as_str()) => continue,
            Err(err) => {
                let reason = format!(
                    "snap_storage_ranges_proof_value_verify_failed:slotset={} idx={} slot=0x{} err={}",
                    slotset_idx,
                    idx,
                    hex32_v1(&slot.hash),
                    err
                );
                observe_network_runtime_eth_peer_decode_failure_v1(
                    chain_id,
                    source_peer_id,
                    reason.as_str(),
                );
                return Err(NetworkError::Decode(reason));
            }
        };
        let Some(proven_value) = proven else {
            continue;
        };
        if proven_value != slot.body {
            let reason = format!(
                "snap_storage_ranges_proof_value_mismatch:slotset={} idx={} slot=0x{} expected_bytes={} proven_bytes={}",
                slotset_idx,
                idx,
                hex32_v1(&slot.hash),
                slot.body.len(),
                proven_value.len()
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        }
    }
    Ok(())
}

fn validate_snap_storage_range_slotset_empty_proof_no_more_v1(
    chain_id: u64,
    source_peer_id: u64,
    slotset_idx: usize,
    root: [u8; 32],
    origin: &[u8],
    slots: &[crate::EthRlpxSnapStorageDataV1],
    proof: &[Vec<u8>],
) -> Result<(), NetworkError> {
    if !slots.is_empty() {
        return Ok(());
    }
    let has_right =
        eth_rlpx_mpt_proof_has_right_element_v1(root, origin, proof).map_err(|err| {
            let reason = format!(
                "snap_storage_ranges_empty_proof_no_more_verify_failed:slotset={} err={}",
                slotset_idx, err
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            NetworkError::Decode(reason)
        })?;
    let proven_at_origin = eth_rlpx_mpt_verify_proof_value_v1(root, origin, proof)
        .map_err(|err| {
            let reason = format!(
                "snap_storage_ranges_empty_proof_origin_value_verify_failed:slotset={} err={}",
                slotset_idx, err
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            NetworkError::Decode(reason)
        })?
        .is_some();
    if has_right || proven_at_origin {
        let reason = format!(
            "snap_storage_ranges_empty_proof_more_entries:slotset={} has_right={} origin_value={}",
            slotset_idx, has_right, proven_at_origin
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    }
    Ok(())
}

fn validate_snap_storage_range_slotset_left_boundary_v1(
    chain_id: u64,
    source_peer_id: u64,
    slotset_idx: usize,
    root: [u8; 32],
    origin: &[u8],
    slots: &[crate::EthRlpxSnapStorageDataV1],
    proof: &[Vec<u8>],
) -> Result<(), NetworkError> {
    let Some(first) = slots.first() else {
        return Ok(());
    };
    let has_left_gap =
        eth_rlpx_mpt_proof_has_element_in_range_v1(root, origin, first.hash.as_slice(), proof)
            .map_err(|err| {
                let reason = format!(
                    "snap_storage_ranges_left_boundary_verify_failed:slotset={} first=0x{} err={}",
                    slotset_idx,
                    hex32_v1(&first.hash),
                    err
                );
                observe_network_runtime_eth_peer_decode_failure_v1(
                    chain_id,
                    source_peer_id,
                    reason.as_str(),
                );
                NetworkError::Decode(reason)
            })?;
    if has_left_gap {
        let reason = format!(
            "snap_storage_ranges_left_gap:slotset={} first=0x{}",
            slotset_idx,
            hex32_v1(&first.hash)
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    }
    Ok(())
}

fn validate_snap_storage_range_slotset_internal_gaps_v1(
    chain_id: u64,
    source_peer_id: u64,
    slotset_idx: usize,
    root: [u8; 32],
    slots: &[crate::EthRlpxSnapStorageDataV1],
    proof: &[Vec<u8>],
) -> Result<(), NetworkError> {
    for pair in slots.windows(2) {
        let previous = pair[0].hash;
        let current = pair[1].hash;
        let Some(lower) = eth_rlpx_account_hash_next_v1(previous) else {
            continue;
        };
        if lower >= current {
            continue;
        }
        let has_gap = eth_rlpx_mpt_proof_has_element_in_range_v1(
            root,
            lower.as_slice(),
            current.as_slice(),
            proof,
        )
        .map_err(|err| {
            let reason = format!(
                "snap_storage_ranges_internal_gap_verify_failed:slotset={} previous=0x{} current=0x{} err={}",
                slotset_idx,
                hex32_v1(&previous),
                hex32_v1(&current),
                err
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            NetworkError::Decode(reason)
        })?;
        if has_gap {
            let reason = format!(
                "snap_storage_ranges_internal_gap:slotset={} previous=0x{} current=0x{}",
                slotset_idx,
                hex32_v1(&previous),
                hex32_v1(&current)
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        }
    }
    Ok(())
}

fn validate_snap_account_range_proof_semantics_v1(
    chain_id: u64,
    source_peer_id: u64,
    state_root: Option<[u8; 32]>,
    origin: [u8; 32],
    limit: [u8; 32],
    response: &EthRlpxAccountRangeResponseV1,
) -> Result<(), NetworkError> {
    if response.proof.is_empty() {
        if response.accounts.is_empty() {
            let reason = "snap_account_range_empty_without_proof".to_string();
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        }
        let reason = format!(
            "snap_account_range_non_empty_without_proof:accounts={}",
            response.accounts.len()
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    }
    validate_snap_account_range_response_preconditions_v1(
        chain_id,
        source_peer_id,
        origin,
        limit,
        response,
    )?;
    let Some(root) = state_root else {
        let reason = "snap_account_range_state_root_missing_for_proof".to_string();
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    };
    validate_snap_proof_nodes_match_roots_v1(
        chain_id,
        source_peer_id,
        "snap_account_range",
        &[root],
        response.proof.as_slice(),
    )?;
    validate_snap_account_range_empty_proof_no_more_v1(
        chain_id,
        source_peer_id,
        root,
        origin,
        response,
    )?;
    validate_snap_account_range_origin_value_v1(chain_id, source_peer_id, root, origin, response)?;
    validate_snap_account_range_left_boundary_v1(chain_id, source_peer_id, root, origin, response)?;
    validate_snap_account_range_internal_gaps_v1(chain_id, source_peer_id, root, response)?;
    validate_snap_account_range_proof_values_v1(chain_id, source_peer_id, root, response)
}

fn snap_storage_range_expected_roots_v1(
    chain_id: u64,
    source_peer_id: u64,
    state_root: [u8; 32],
    pending_accounts: &[[u8; 32]],
    slotset_count: usize,
) -> Result<Vec<[u8; 32]>, NetworkError> {
    let mut roots = Vec::new();
    for idx in 0..slotset_count {
        let Some(account_hash) = pending_accounts.get(idx).copied() else {
            let reason = format!(
                "snap_storage_ranges_slotset_account_missing:idx={} slotsets={} requested={}",
                idx,
                slotset_count,
                pending_accounts.len()
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        };
        let Some(account) =
            get_network_runtime_native_snap_account_snapshot_v1(chain_id, state_root, account_hash)
        else {
            let reason = format!(
                "snap_storage_ranges_account_root_missing:idx={} account=0x{}",
                idx,
                hex32_v1(&account_hash)
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        };
        roots.push(
            account
                .storage_root
                .unwrap_or_else(crate::eth_rlpx_empty_trie_root_v1),
        );
    }
    Ok(roots)
}

fn validate_snap_storage_ranges_proof_semantics_v1(
    chain_id: u64,
    source_peer_id: u64,
    state_root: Option<[u8; 32]>,
    pending_accounts: &[[u8; 32]],
    origin: &[u8],
    limit: &[u8],
    response: &EthRlpxStorageRangesResponseV1,
) -> Result<(), NetworkError> {
    if response.proof.is_empty() && response.slots.is_empty() {
        let reason = "snap_storage_ranges_empty_without_proof".to_string();
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    }
    validate_snap_storage_ranges_response_preconditions_v1(
        chain_id,
        source_peer_id,
        origin,
        limit,
        response,
    )?;
    let Some(state_root) = state_root else {
        let reason = "snap_storage_ranges_state_root_missing".to_string();
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    };
    if !response.proof.is_empty() {
        let slotset_count = if response.slots.is_empty() {
            pending_accounts.len().min(1)
        } else {
            response.slots.len()
        };
        let expected_roots = snap_storage_range_expected_roots_v1(
            chain_id,
            source_peer_id,
            state_root,
            pending_accounts,
            slotset_count,
        )?;
        let proof_slotset_idx = response.slots.len().saturating_sub(1);
        for (idx, slots) in response.slots.iter().enumerate() {
            if idx >= proof_slotset_idx {
                break;
            }
            let Some(expected) = expected_roots.get(idx).copied() else {
                continue;
            };
            let account_hash = pending_accounts[idx];
            let actual =
                eth_rlpx_snap_storage_root_from_range_v1(slots.as_slice()).map_err(|err| {
                    let reason = format!("snap_storage_ranges_root_rebuild_failed:{err}");
                    observe_network_runtime_eth_peer_decode_failure_v1(
                        chain_id,
                        source_peer_id,
                        reason.as_str(),
                    );
                    NetworkError::Decode(reason)
                })?;
            if actual != expected {
                let reason = format!(
                    "snap_storage_ranges_root_mismatch:idx={} account=0x{} expected=0x{} got=0x{}",
                    idx,
                    hex32_v1(&account_hash),
                    hex32_v1(&expected),
                    hex32_v1(&actual)
                );
                observe_network_runtime_eth_peer_decode_failure_v1(
                    chain_id,
                    source_peer_id,
                    reason.as_str(),
                );
                return Err(NetworkError::Decode(reason));
            }
        }
        let Some(proof_root) = expected_roots.get(proof_slotset_idx).copied() else {
            let reason = format!(
                "snap_storage_ranges_proof_slotset_root_missing:idx={} roots={}",
                proof_slotset_idx,
                expected_roots.len()
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        };
        validate_snap_proof_nodes_match_roots_v1(
            chain_id,
            source_peer_id,
            "snap_storage_ranges",
            &[proof_root],
            response.proof.as_slice(),
        )?;
        let proof_slots = response
            .slots
            .get(proof_slotset_idx)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        validate_snap_storage_range_slotset_proof_values_v1(
            chain_id,
            source_peer_id,
            proof_slotset_idx,
            proof_root,
            proof_slots,
            response.proof.as_slice(),
        )?;
        validate_snap_storage_range_slotset_empty_proof_no_more_v1(
            chain_id,
            source_peer_id,
            proof_slotset_idx,
            proof_root,
            origin,
            proof_slots,
            response.proof.as_slice(),
        )?;
        validate_snap_storage_range_slotset_left_boundary_v1(
            chain_id,
            source_peer_id,
            proof_slotset_idx,
            proof_root,
            origin,
            proof_slots,
            response.proof.as_slice(),
        )?;
        validate_snap_storage_range_slotset_internal_gaps_v1(
            chain_id,
            source_peer_id,
            proof_slotset_idx,
            proof_root,
            proof_slots,
            response.proof.as_slice(),
        )?;
        return Ok(());
    }
    let expected_roots = snap_storage_range_expected_roots_v1(
        chain_id,
        source_peer_id,
        state_root,
        pending_accounts,
        response.slots.len(),
    )?;
    for (idx, slots) in response.slots.iter().enumerate() {
        let Some(expected) = expected_roots.get(idx).copied() else {
            continue;
        };
        let account_hash = pending_accounts[idx];
        let actual = eth_rlpx_snap_storage_root_from_range_v1(slots.as_slice()).map_err(|err| {
            let reason = format!("snap_storage_ranges_root_rebuild_failed:{err}");
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            NetworkError::Decode(reason)
        })?;
        if actual != expected {
            let reason = format!(
                "snap_storage_ranges_root_mismatch:idx={} account=0x{} expected=0x{} got=0x{}",
                idx,
                hex32_v1(&account_hash),
                hex32_v1(&expected),
                hex32_v1(&actual)
            );
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        }
    }
    Ok(())
}

fn eth_fullnode_native_snap_storage_ranges_completed_slotsets_v1(
    response: &EthRlpxStorageRangesResponseV1,
) -> usize {
    if response.slots.is_empty() && !response.proof.is_empty() {
        1
    } else {
        response.slots.len()
    }
}

fn eth_fullnode_native_snap_storage_ranges_missing_accounts_v1(
    pending_accounts: &[[u8; 32]],
    response: &EthRlpxStorageRangesResponseV1,
) -> Vec<[u8; 32]> {
    let completed = eth_fullnode_native_snap_storage_ranges_completed_slotsets_v1(response)
        .min(pending_accounts.len());
    pending_accounts[completed..].to_vec()
}

fn eth_fullnode_native_snap_storage_ranges_continuation_v1(
    chain_id: u64,
    source_peer_id: u64,
    state_root: Option<[u8; 32]>,
    pending_accounts: &[[u8; 32]],
    response: &EthRlpxStorageRangesResponseV1,
) -> Result<Option<([u8; 32], [u8; 32])>, NetworkError> {
    if response.proof.is_empty() || response.slots.is_empty() {
        return Ok(None);
    }
    let slotset_idx = response.slots.len().saturating_sub(1);
    let Some(slots) = response.slots.get(slotset_idx) else {
        return Ok(None);
    };
    let Some(last_slot) = slots.last() else {
        return Ok(None);
    };
    let Some(state_root) = state_root else {
        let reason = "snap_storage_ranges_state_root_missing_for_continuation".to_string();
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    };
    let expected_roots = snap_storage_range_expected_roots_v1(
        chain_id,
        source_peer_id,
        state_root,
        pending_accounts,
        slotset_idx.saturating_add(1),
    )?;
    let Some(proof_root) = expected_roots.get(slotset_idx).copied() else {
        return Ok(None);
    };
    let has_right = eth_rlpx_mpt_proof_has_right_element_v1(
        proof_root,
        last_slot.hash.as_slice(),
        response.proof.as_slice(),
    )
    .map_err(|err| {
        let reason = format!(
            "snap_storage_ranges_continuation_verify_failed:slotset={} last=0x{} err={}",
            slotset_idx,
            hex32_v1(&last_slot.hash),
            err
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        NetworkError::Decode(reason)
    })?;
    if !has_right {
        return Ok(None);
    }
    let Some(account_hash) = pending_accounts.get(slotset_idx).copied() else {
        return Ok(None);
    };
    let Some(next_origin) = eth_rlpx_account_hash_next_v1(last_slot.hash) else {
        let reason = format!(
            "snap_storage_ranges_continuation_origin_overflow:slotset={} last=0x{}",
            slotset_idx,
            hex32_v1(&last_slot.hash)
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    };
    Ok(Some((account_hash, next_origin)))
}

fn set_or_merge_eth_fullnode_native_snap_account_storage_snapshot_v1(
    chain_id: u64,
    snapshot: NetworkRuntimeNativeSnapAccountStorageSnapshotV1,
) {
    let Some(mut existing) = crate::get_network_runtime_native_snap_account_storage_snapshot_v1(
        chain_id,
        snapshot.state_root,
        snapshot.account_hash,
    ) else {
        set_network_runtime_native_snap_account_storage_snapshot_v1(chain_id, snapshot);
        return;
    };
    for slot in snapshot.slots {
        if let Some(existing_slot) = existing
            .slots
            .iter_mut()
            .find(|existing_slot| existing_slot.hash == slot.hash)
        {
            *existing_slot = slot;
        } else {
            existing.slots.push(slot);
        }
    }
    existing.slots.sort_by(|a, b| a.hash.cmp(&b.hash));
    existing.proof_nodes = snapshot.proof_nodes;
    existing.source_peer_id = snapshot.source_peer_id;
    existing.observed_unix_ms = snapshot.observed_unix_ms;
    set_network_runtime_native_snap_account_storage_snapshot_v1(chain_id, existing);
}

fn dispatch_eth_fullnode_native_snap_storage_ranges_request_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    root: [u8; 32],
    accounts: &[[u8; 32]],
    origin: &[u8],
    limit: &[u8],
    failure_reason: &'static str,
) -> Result<u64, NetworkError> {
    if accounts.is_empty() {
        return Err(NetworkError::Encode(
            "snap_storage_ranges_empty_request".to_string(),
        ));
    }
    let Some(snap_offset) = eth_rlpx_snap_base_offset_v1(
        session._negotiated_eth_version,
        session._negotiated_snap_version,
    ) else {
        return Err(NetworkError::Decode(
            "snap_storage_ranges_without_negotiated_snap".to_string(),
        ));
    };
    let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
    let payload = eth_rlpx_build_get_storage_ranges_payload_v1(
        request_id,
        root,
        accounts,
        origin,
        limit,
        ETH_RLPX_SNAP_DEFAULT_ACCOUNT_RANGE_BYTES,
    );
    eth_rlpx_write_wire_frame_v1(
        &mut session.stream,
        &mut session.frame_session,
        snap_offset + ETH_RLPX_SNAP_GET_STORAGE_RANGES_MSG,
        payload.as_slice(),
    )
    .map_err(|err| {
        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            source_peer_id,
            failure_reason,
            err.as_str(),
        );
        NetworkError::Io(err)
    })?;
    observe_eth_native_snap_pull(chain_id);
    observe_network_runtime_eth_peer_syncing_v1(chain_id, source_peer_id);
    session.last_snap_storage_ranges_request_id = Some(request_id);
    session.pending_snap_storage_accounts = accounts.to_vec();
    session.pending_snap_storage_origin = origin.to_vec();
    session.pending_snap_storage_limit = limit.to_vec();
    session.last_sync_request_unix_ms = now_unix_ms();
    eprintln!(
        "network_info: rlpx stage snap_storage_ranges_requested chain_id={} peer={} endpoint={} request_id={} accounts={} origin={} limit={} root=0x{}",
        chain_id,
        source_peer_id,
        session.endpoint.addr_hint,
        request_id,
        accounts.len(),
        hex_dynamic_v1(origin),
        hex_dynamic_v1(limit),
        hex32_v1(&root),
    );
    Ok(request_id)
}

fn ingest_real_rlpx_snap_account_range_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    response: &EthRlpxAccountRangeResponseV1,
) -> Result<(), NetworkError> {
    let Some(request_id) = session.last_snap_account_range_request_id else {
        let reason = format!(
            "snap_account_range_unexpected_response:request_id={}",
            response.request_id
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    };
    if request_id != response.request_id {
        return Ok(());
    }
    observe_eth_native_snap_response(chain_id);
    eprintln!(
        "network_info: rlpx stage snap_account_range_received chain_id={} peer={} endpoint={} negotiated_eth={} negotiated_snap={:?} request_id={} accounts={} proof_nodes={}",
        chain_id,
        source_peer_id,
        session.endpoint.addr_hint,
        session._negotiated_eth_version,
        session._negotiated_snap_version,
        response.request_id,
        response.accounts.len(),
        response.proof.len(),
    );
    let account_origin = session.last_snap_account_origin.unwrap_or([0u8; 32]);
    let account_limit = session.last_snap_account_limit.unwrap_or([0xffu8; 32]);
    validate_snap_account_range_proof_semantics_v1(
        chain_id,
        source_peer_id,
        session.last_snap_state_root,
        account_origin,
        account_limit,
        response,
    )?;
    let has_account_continuation = eth_fullnode_native_snap_account_range_has_continuation_v1(
        chain_id,
        source_peer_id,
        session.last_snap_state_root,
        response,
    )?;
    session.pending_snap_next_account_origin = eth_rlpx_snap_account_range_next_origin_v1(
        account_origin,
        account_limit,
        response,
        has_account_continuation,
    )?;
    session.last_snap_account_range_request_id = None;
    let Some(root) = session.last_snap_state_root else {
        mark_network_runtime_eth_peer_session_ready_v1(chain_id, source_peer_id, None);
        return Ok(());
    };
    let mut storage_accounts = Vec::new();
    let mut code_hashes = Vec::new();
    let mut trie_node_candidates = Vec::<(Vec<Vec<u8>>, [u8; 32])>::new();
    let mut seen_code_hashes = HashSet::new();
    let observed_unix_ms = now_unix_ms() as u128;
    for (idx, account) in response.accounts.iter().enumerate() {
        let parsed = eth_rlpx_parse_snap_slim_account_fields_v1(account.body_rlp.as_slice()).ok();
        let storage_root = parsed.map(|fields| fields.storage_root);
        let code_hash = parsed.map(|fields| fields.code_hash);
        set_network_runtime_native_snap_account_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapAccountSnapshotV1 {
                chain_id,
                state_root: root,
                account_hash: account.hash,
                body_rlp: account.body_rlp.clone(),
                proof_nodes: response.proof.clone(),
                storage_root,
                code_hash,
                has_storage: parsed.is_some_and(|fields| fields.has_storage),
                has_code: parsed.is_some_and(|fields| fields.has_code),
                source_peer_id: Some(source_peer_id),
                observed_unix_ms,
            },
        );
        if idx >= 256 {
            continue;
        }
        if match parsed {
            Some(fields) => fields.has_storage,
            None => true,
        } {
            storage_accounts.push(account.hash);
            if let Some(fields) = parsed {
                trie_node_candidates.push((
                    eth_fullnode_native_snap_storage_root_trie_pathset_v1(account.hash),
                    fields.storage_root,
                ));
            }
        }
        if let Some(fields) = parsed {
            if fields.has_code && seen_code_hashes.insert(fields.code_hash) {
                code_hashes.push(fields.code_hash);
            }
        }
    }
    if !storage_accounts.is_empty() {
        dispatch_eth_fullnode_native_snap_storage_ranges_request_v1(
            chain_id,
            source_peer_id,
            session,
            root,
            storage_accounts.as_slice(),
            &[],
            &[],
            "snap_storage_ranges_request_write_failed",
        )?;
    }
    if !code_hashes.is_empty() {
        dispatch_eth_fullnode_native_snap_byte_codes_request_v1(
            chain_id,
            source_peer_id,
            session,
            code_hashes.as_slice(),
            "snap_byte_codes_request_write_failed",
        )?;
    }
    let _ = maybe_request_eth_fullnode_native_snap_trie_nodes_v1(
        chain_id,
        source_peer_id,
        session,
        trie_node_candidates.as_slice(),
    )?;
    if session.last_snap_storage_ranges_request_id.is_none()
        && session.last_snap_byte_codes_request_id.is_none()
        && session.last_snap_trie_nodes_request_id.is_none()
    {
        if maybe_continue_eth_fullnode_native_snap_account_range_v1(
            chain_id,
            source_peer_id,
            session,
        )? {
            return Ok(());
        }
        mark_network_runtime_eth_peer_session_ready_v1(chain_id, source_peer_id, None);
    }
    Ok(())
}

fn ingest_real_rlpx_snap_storage_ranges_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    response: &EthRlpxStorageRangesResponseV1,
) -> Result<(), NetworkError> {
    let Some(request_id) = session.last_snap_storage_ranges_request_id else {
        let reason = format!(
            "snap_storage_ranges_unexpected_response:request_id={}",
            response.request_id
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    };
    if request_id != response.request_id {
        return Ok(());
    }
    if response.slots.len() > session.pending_snap_storage_accounts.len() {
        let reason = format!(
            "snap_storage_ranges_slotset_count_exceeds_requested:slotsets={} requested={}",
            response.slots.len(),
            session.pending_snap_storage_accounts.len()
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    }
    let requested_storage_accounts = session.pending_snap_storage_accounts.clone();
    let requested_storage_origin = session.pending_snap_storage_origin.clone();
    let requested_storage_limit = session.pending_snap_storage_limit.clone();
    validate_snap_storage_ranges_proof_semantics_v1(
        chain_id,
        source_peer_id,
        session.last_snap_state_root,
        requested_storage_accounts.as_slice(),
        requested_storage_origin.as_slice(),
        requested_storage_limit.as_slice(),
        response,
    )?;
    let completed_slotsets =
        eth_fullnode_native_snap_storage_ranges_completed_slotsets_v1(response)
            .min(requested_storage_accounts.len());
    let missing_storage_accounts = eth_fullnode_native_snap_storage_ranges_missing_accounts_v1(
        requested_storage_accounts.as_slice(),
        response,
    );
    let previous_deferred_storage_accounts = session.pending_snap_storage_deferred_accounts.clone();
    let storage_continuation = eth_fullnode_native_snap_storage_ranges_continuation_v1(
        chain_id,
        source_peer_id,
        session.last_snap_state_root,
        requested_storage_accounts.as_slice(),
        response,
    )?;
    observe_eth_native_snap_response(chain_id);
    eprintln!(
        "network_info: rlpx stage snap_storage_ranges_received chain_id={} peer={} endpoint={} negotiated_eth={} negotiated_snap={:?} request_id={} slotsets={} proof_nodes={}",
        chain_id,
        source_peer_id,
        session.endpoint.addr_hint,
        session._negotiated_eth_version,
        session._negotiated_snap_version,
        response.request_id,
        response.slots.len(),
        response.proof.len(),
    );
    if let Some(root) = session.last_snap_state_root {
        let observed_unix_ms = now_unix_ms() as u128;
        for idx in 0..completed_slotsets {
            let Some(account_hash) = requested_storage_accounts.get(idx).copied() else {
                continue;
            };
            let slots = response.slots.get(idx).map(Vec::as_slice).unwrap_or(&[]);
            set_or_merge_eth_fullnode_native_snap_account_storage_snapshot_v1(
                chain_id,
                NetworkRuntimeNativeSnapAccountStorageSnapshotV1 {
                    chain_id,
                    state_root: root,
                    account_hash,
                    slots: slots
                        .iter()
                        .map(|slot| NetworkRuntimeNativeSnapStorageSlotSnapshotV1 {
                            hash: slot.hash,
                            body: slot.body.clone(),
                        })
                        .collect(),
                    proof_nodes: response.proof.clone(),
                    source_peer_id: Some(source_peer_id),
                    observed_unix_ms,
                },
            );
        }
    }
    session.last_snap_storage_ranges_request_id = None;
    session.pending_snap_storage_accounts.clear();
    session.pending_snap_storage_origin.clear();
    session.pending_snap_storage_limit.clear();
    if let Some((account_hash, next_origin)) = storage_continuation {
        let Some(root) = session.last_snap_state_root else {
            let reason =
                "snap_storage_ranges_state_root_missing_for_continuation_retry".to_string();
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        };
        let mut deferred_accounts = missing_storage_accounts.clone();
        deferred_accounts.extend(previous_deferred_storage_accounts);
        session.pending_snap_storage_deferred_accounts = deferred_accounts;
        dispatch_eth_fullnode_native_snap_storage_ranges_request_v1(
            chain_id,
            source_peer_id,
            session,
            root,
            &[account_hash],
            next_origin.as_slice(),
            requested_storage_limit.as_slice(),
            "snap_storage_ranges_continuation_request_write_failed",
        )?;
        return Ok(());
    }
    let mut next_storage_accounts = missing_storage_accounts;
    next_storage_accounts.extend(previous_deferred_storage_accounts);
    session.pending_snap_storage_deferred_accounts.clear();
    if !next_storage_accounts.is_empty() {
        let Some(root) = session.last_snap_state_root else {
            let reason = "snap_storage_ranges_state_root_missing_for_retry".to_string();
            observe_network_runtime_eth_peer_decode_failure_v1(
                chain_id,
                source_peer_id,
                reason.as_str(),
            );
            return Err(NetworkError::Decode(reason));
        };
        dispatch_eth_fullnode_native_snap_storage_ranges_request_v1(
            chain_id,
            source_peer_id,
            session,
            root,
            next_storage_accounts.as_slice(),
            &[],
            &[],
            "snap_storage_ranges_retry_request_write_failed",
        )?;
        return Ok(());
    }
    if session.last_snap_account_range_request_id.is_none()
        && session.last_snap_byte_codes_request_id.is_none()
        && session.last_snap_trie_nodes_request_id.is_none()
    {
        if maybe_continue_eth_fullnode_native_snap_account_range_v1(
            chain_id,
            source_peer_id,
            session,
        )? {
            return Ok(());
        }
        mark_network_runtime_eth_peer_session_ready_v1(chain_id, source_peer_id, None);
    }
    Ok(())
}

fn validate_eth_fullnode_native_snap_byte_codes_match_request_v1(
    requested_hashes: &[[u8; 32]],
    codes: &[Vec<u8>],
) -> Result<Vec<[u8; 32]>, String> {
    if codes.is_empty() {
        return Err("snap_byte_codes_empty_response".to_string());
    }
    let mut matched = Vec::with_capacity(codes.len());
    let mut request_idx = 0usize;
    for code in codes {
        let code_hash = eth_rlpx_code_hash_v1(code.as_slice());
        while request_idx < requested_hashes.len() && requested_hashes[request_idx] != code_hash {
            request_idx = request_idx.saturating_add(1);
        }
        if request_idx >= requested_hashes.len() {
            return Err(format!(
                "snap_byte_codes_unrequested_or_out_of_order_hash:hash=0x{}",
                hex32_v1(&code_hash)
            ));
        }
        matched.push(code_hash);
        request_idx = request_idx.saturating_add(1);
    }
    Ok(matched)
}

fn eth_fullnode_native_snap_byte_codes_missing_hashes_v1(
    requested_hashes: &[[u8; 32]],
    matched_hashes: &[[u8; 32]],
) -> Vec<[u8; 32]> {
    requested_hashes
        .iter()
        .copied()
        .filter(|hash| !matched_hashes.contains(hash))
        .collect()
}

fn dispatch_eth_fullnode_native_snap_byte_codes_request_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    code_hashes: &[[u8; 32]],
    failure_reason: &'static str,
) -> Result<u64, NetworkError> {
    if code_hashes.is_empty() {
        return Err(NetworkError::Encode(
            "snap_byte_codes_empty_request".to_string(),
        ));
    }
    let Some(snap_offset) = eth_rlpx_snap_base_offset_v1(
        session._negotiated_eth_version,
        session._negotiated_snap_version,
    ) else {
        return Err(NetworkError::Decode(
            "snap_byte_codes_without_negotiated_snap".to_string(),
        ));
    };
    let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
    let payload = eth_rlpx_build_get_byte_codes_payload_v1(
        request_id,
        code_hashes,
        ETH_RLPX_SNAP_DEFAULT_ACCOUNT_RANGE_BYTES,
    );
    eth_rlpx_write_wire_frame_v1(
        &mut session.stream,
        &mut session.frame_session,
        snap_offset + ETH_RLPX_SNAP_GET_BYTE_CODES_MSG,
        payload.as_slice(),
    )
    .map_err(|err| {
        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            source_peer_id,
            failure_reason,
            err.as_str(),
        );
        NetworkError::Io(err)
    })?;
    observe_eth_native_snap_pull(chain_id);
    observe_network_runtime_eth_peer_syncing_v1(chain_id, source_peer_id);
    session.last_snap_byte_codes_request_id = Some(request_id);
    session.pending_snap_code_hashes = code_hashes.to_vec();
    session.last_sync_request_unix_ms = now_unix_ms();
    eprintln!(
        "network_info: rlpx stage snap_byte_codes_requested chain_id={} peer={} endpoint={} request_id={} hashes={}",
        chain_id,
        source_peer_id,
        session.endpoint.addr_hint,
        request_id,
        code_hashes.len(),
    );
    Ok(request_id)
}

fn ingest_real_rlpx_snap_byte_codes_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    response: &EthRlpxByteCodesResponseV1,
) -> Result<(), NetworkError> {
    let Some(request_id) = session.last_snap_byte_codes_request_id else {
        let reason = format!(
            "snap_byte_codes_unexpected_response:request_id={}",
            response.request_id
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    };
    if request_id != response.request_id {
        return Ok(());
    }
    if response.codes.len() > session.pending_snap_code_hashes.len() {
        let reason = format!(
            "snap_byte_codes_count_exceeds_requested:codes={} requested={}",
            response.codes.len(),
            session.pending_snap_code_hashes.len()
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    }
    let requested_code_hashes = session.pending_snap_code_hashes.clone();
    let matched_code_hashes = validate_eth_fullnode_native_snap_byte_codes_match_request_v1(
        requested_code_hashes.as_slice(),
        response.codes.as_slice(),
    )
    .map_err(|reason| {
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        NetworkError::Decode(reason)
    })?;
    let missing_code_hashes = eth_fullnode_native_snap_byte_codes_missing_hashes_v1(
        requested_code_hashes.as_slice(),
        matched_code_hashes.as_slice(),
    );
    observe_eth_native_snap_response(chain_id);
    eprintln!(
        "network_info: rlpx stage snap_byte_codes_received chain_id={} peer={} endpoint={} negotiated_eth={} negotiated_snap={:?} request_id={} codes={}",
        chain_id,
        source_peer_id,
        session.endpoint.addr_hint,
        session._negotiated_eth_version,
        session._negotiated_snap_version,
        response.request_id,
        response.codes.len(),
    );
    let observed_unix_ms = now_unix_ms() as u128;
    for (code, code_hash) in response.codes.iter().zip(matched_code_hashes.iter()) {
        set_network_runtime_native_snap_code_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapCodeSnapshotV1 {
                chain_id,
                code_hash: *code_hash,
                code: code.clone(),
                source_peer_id: Some(source_peer_id),
                observed_unix_ms,
            },
        );
    }
    session.last_snap_byte_codes_request_id = None;
    session.pending_snap_code_hashes.clear();
    if !missing_code_hashes.is_empty() {
        dispatch_eth_fullnode_native_snap_byte_codes_request_v1(
            chain_id,
            source_peer_id,
            session,
            missing_code_hashes.as_slice(),
            "snap_byte_codes_retry_request_write_failed",
        )?;
        return Ok(());
    }
    if session.last_snap_account_range_request_id.is_none()
        && session.last_snap_storage_ranges_request_id.is_none()
        && session.last_snap_trie_nodes_request_id.is_none()
    {
        if maybe_continue_eth_fullnode_native_snap_account_range_v1(
            chain_id,
            source_peer_id,
            session,
        )? {
            return Ok(());
        }
        mark_network_runtime_eth_peer_session_ready_v1(chain_id, source_peer_id, None);
    }
    Ok(())
}

fn ingest_real_rlpx_snap_trie_nodes_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    response: &EthRlpxTrieNodesResponseV1,
) -> Result<(), NetworkError> {
    let Some(request_id) = session.last_snap_trie_nodes_request_id else {
        let reason = format!(
            "snap_trie_nodes_unexpected_response:request_id={}",
            response.request_id
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    };
    if request_id != response.request_id {
        return Ok(());
    }
    if response.nodes.len() > session.pending_snap_trie_node_pathsets.len() {
        let reason = format!(
            "snap_trie_nodes_count_exceeds_requested:nodes={} requested={}",
            response.nodes.len(),
            session.pending_snap_trie_node_pathsets.len()
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    }
    if response.nodes.is_empty() {
        let reason = "snap_trie_nodes_empty_response".to_string();
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    }
    let Some(root) = session.last_snap_state_root else {
        let reason = "snap_trie_nodes_state_root_missing".to_string();
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        return Err(NetworkError::Decode(reason));
    };
    let mut expected_hashes = session.pending_snap_trie_node_hashes.clone();
    while expected_hashes.len() < session.pending_snap_trie_node_pathsets.len() {
        expected_hashes.push(root);
    }
    let matched_nodes = match_eth_fullnode_native_snap_trie_nodes_v1(
        expected_hashes.as_slice(),
        response.nodes.as_slice(),
    )
    .map_err(|reason| {
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            source_peer_id,
            reason.as_str(),
        );
        NetworkError::Decode(reason)
    })?;
    let matched_indices = matched_nodes
        .iter()
        .map(|(expected_idx, _)| *expected_idx)
        .collect::<HashSet<_>>();
    let missing_nodes = session
        .pending_snap_trie_node_pathsets
        .iter()
        .enumerate()
        .filter_map(|(idx, pathset)| {
            if matched_indices.contains(&idx) {
                return None;
            }
            let expected_hash = expected_hashes.get(idx).copied().unwrap_or(root);
            if get_network_runtime_native_snap_trie_node_snapshot_v1(
                chain_id,
                root,
                pathset.as_slice(),
            )
            .is_some()
            {
                return None;
            }
            Some((pathset.clone(), expected_hash))
        })
        .collect::<Vec<_>>();
    observe_eth_native_snap_response(chain_id);
    eprintln!(
        "network_info: rlpx stage snap_trie_nodes_received chain_id={} peer={} endpoint={} negotiated_eth={} negotiated_snap={:?} request_id={} nodes={} matched={} missing={}",
        chain_id,
        source_peer_id,
        session.endpoint.addr_hint,
        session._negotiated_eth_version,
        session._negotiated_snap_version,
        response.request_id,
        response.nodes.len(),
        matched_nodes.len(),
        session
            .pending_snap_trie_node_pathsets
            .len()
            .saturating_sub(matched_nodes.len()),
    );
    let observed_unix_ms = now_unix_ms() as u128;
    for (response_idx, (expected_idx, node_hash)) in matched_nodes.iter().enumerate() {
        let Some(node) = response.nodes.get(response_idx) else {
            continue;
        };
        let Some(path_segments) = session
            .pending_snap_trie_node_pathsets
            .get(*expected_idx)
            .cloned()
        else {
            continue;
        };
        set_network_runtime_native_snap_trie_node_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapTrieNodeSnapshotV1 {
                chain_id,
                state_root: root,
                path_segments,
                node_hash: *node_hash,
                node_rlp: node.clone(),
                source_peer_id: Some(source_peer_id),
                observed_unix_ms,
            },
        );
    }
    let next_retry_count = session.pending_snap_trie_node_retry_count.saturating_add(1);
    session.last_snap_trie_nodes_request_id = None;
    if !missing_nodes.is_empty()
        && session.pending_snap_trie_node_retry_count
            < ETH_FULLNODE_NATIVE_SNAP_TRIE_NODE_RETRY_LIMIT_V1
        && request_eth_fullnode_native_snap_trie_nodes_batch_v1(
            chain_id,
            source_peer_id,
            session,
            root,
            missing_nodes.as_slice(),
            next_retry_count,
        )?
    {
        eprintln!(
            "network_info: rlpx stage snap_trie_nodes_retry_requested chain_id={} peer={} endpoint={} missing={} retry={} root=0x{}",
            chain_id,
            source_peer_id,
            session.endpoint.addr_hint,
            missing_nodes.len(),
            next_retry_count,
            hex32_v1(&root),
        );
        return Ok(());
    }
    if !missing_nodes.is_empty() {
        eprintln!(
            "network_warn: rlpx stage snap_trie_nodes_missing_after_retries chain_id={} peer={} endpoint={} missing={} retry={} root=0x{}",
            chain_id,
            source_peer_id,
            session.endpoint.addr_hint,
            missing_nodes.len(),
            session.pending_snap_trie_node_retry_count,
            hex32_v1(&root),
        );
    }
    session.pending_snap_trie_node_pathsets.clear();
    session.pending_snap_trie_node_hashes.clear();
    session.pending_snap_trie_node_retry_count = 0;
    if session.last_snap_account_range_request_id.is_none()
        && session.last_snap_storage_ranges_request_id.is_none()
        && session.last_snap_byte_codes_request_id.is_none()
    {
        if maybe_continue_eth_fullnode_native_snap_account_range_v1(
            chain_id,
            source_peer_id,
            session,
        )? {
            return Ok(());
        }
        mark_network_runtime_eth_peer_session_ready_v1(chain_id, source_peer_id, None);
    }
    Ok(())
}

fn ingest_real_rlpx_new_block_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    block: &EthRlpxNewBlockPayloadV1,
    report: &mut EthFullnodeNativeRlpxPeerTickReportV1,
) -> Result<(), NetworkError> {
    eth_rlpx_validate_block_empty_body_roots_v1(&block.header, &block.body).map_err(|err| {
        observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, err.as_str());
        NetworkError::Decode(err)
    })?;
    if let Some(raw_rlp) = block.header.raw_rlp.as_deref() {
        set_network_runtime_native_header_rlp_v1(chain_id, block.header.hash, raw_rlp);
    }
    let header_wire = evm_native_header_wire_from_rlpx_header_v1(&block.header);
    ingest_runtime_native_header_from_evm_wire(chain_id, source_peer_id, &header_wire);
    report.header_updates = report.header_updates.saturating_add(1);
    let body_wire =
        evm_native_body_wire_from_rlpx_body_v1(block.header.number, block.header.hash, &block.body);
    ingest_runtime_native_body_from_evm_wire(chain_id, source_peer_id, &body_wire);
    report.body_updates = report.body_updates.saturating_add(1);
    queue_eth_fullnode_native_block_access_list_hash_v1(
        chain_id,
        session,
        block.header.hash,
        block.header.block_access_list_hash,
        block.header.gas_limit,
        Some(block.body.tx_hashes.len()),
    );
    let _ = observe_network_runtime_peer_head(chain_id, source_peer_id, block.header.number);
    observe_network_runtime_eth_peer_head(chain_id, source_peer_id, block.header.number);

    session.pending_body_headers = vec![EthFullnodeNativePendingBodyHeaderV1 {
        number: block.header.number,
        hash: block.header.hash,
        parent_hash: block.header.parent_hash,
        state_root: block.header.state_root,
        transactions_root: block.header.transactions_root,
        receipts_root: block.header.receipts_root,
        tx_count: Some(block.body.tx_hashes.len()),
        withdrawal_count: block.body.withdrawal_count,
    }];
    let empty_receipts = materialize_empty_receipts_for_pending_body_headers_v1(
        chain_id,
        source_peer_id,
        &mut session.pending_body_headers,
    )?;
    report.receipt_updates = report.receipt_updates.saturating_add(empty_receipts);
    if session.pending_body_headers.is_empty() {
        clear_eth_fullnode_native_headers_request_state_v1(session);
        session.last_bodies_request_id = None;
        session.last_receipts_request_id = None;
        session.pending_receipt_request_offset = 0;
        clear_eth_fullnode_native_snap_request_state_v1(session);
        if dispatch_eth_fullnode_native_rlpx_queued_block_access_lists_v1(
            chain_id,
            source_peer_id,
            session,
            report,
        )? {
            return Ok(());
        }
        mark_network_runtime_eth_peer_session_ready_v1(chain_id, source_peer_id, None);
        return Ok(());
    }
    let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
    let payload = eth_rlpx_build_get_receipts_payload_v1(
        request_id,
        0,
        &[block.header.hash],
        session._negotiated_eth_version,
    );
    eth_rlpx_write_wire_frame_v1(
        &mut session.stream,
        &mut session.frame_session,
        ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_GET_RECEIPTS_MSG,
        payload.as_slice(),
    )
    .map_err(|err| {
        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            source_peer_id,
            "new_block_receipts_request_write_failed",
            err.as_str(),
        );
        NetworkError::Io(err)
    })?;
    clear_eth_fullnode_native_headers_request_state_v1(session);
    session.last_bodies_request_id = None;
    session.last_receipts_request_id = Some(request_id);
    session.pending_receipt_request_offset = 0;
    mark_eth_fullnode_native_recovery_inflight_v1(
        chain_id,
        source_peer_id,
        request_id,
        ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_RECEIPT_KIND_V1,
        session.pending_body_headers.as_slice(),
    );
    clear_eth_fullnode_native_snap_request_state_v1(session);
    session.last_sync_request_unix_ms = now_unix_ms();
    report.sync_requests = report.sync_requests.saturating_add(1);
    eprintln!(
        "network_info: rlpx stage receipts_requested chain_id={} peer={} endpoint={} request_id={} blocks={}",
        chain_id,
        source_peer_id,
        session.endpoint.addr_hint,
        request_id,
        session.pending_body_headers.len()
    );
    Ok(())
}

fn ingest_real_rlpx_block_headers_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    headers: &EthRlpxBlockHeadersResponseV1,
    budget_hooks: &EthFullnodeBudgetHooksV1,
    report: &mut EthFullnodeNativeRlpxPeerTickReportV1,
) -> Result<(), NetworkError> {
    let Some(pending_request) = session.pending_headers_request else {
        let err = format!(
            "rlpx_block_headers_unexpected_response:request_id={}",
            headers.request_id
        );
        observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
        return Err(NetworkError::Decode(err));
    };
    if pending_request.request_id != headers.request_id {
        return Ok(());
    }
    clear_eth_fullnode_native_header_inflight_request_v1(
        chain_id,
        source_peer_id,
        pending_request.request_id,
    );
    observe_eth_native_headers_response(chain_id);
    let all_headers = headers.headers.iter().collect::<Vec<_>>();
    if let Err(err) = validate_eth_fullnode_native_rlpx_block_headers_response_matches_request_v1(
        &pending_request,
        all_headers.as_slice(),
    ) {
        observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
        return Err(NetworkError::Decode(err));
    }
    if headers.headers.is_empty() {
        clear_eth_fullnode_native_headers_request_state_v1(session);
        return Ok(());
    }
    clear_eth_fullnode_native_headers_request_state_v1(session);
    let body_request_cap = usize::try_from(budget_hooks.sync_pull_bodies_batch.max(1))
        .unwrap_or(usize::MAX)
        .max(1);
    let imported_headers = headers
        .headers
        .iter()
        .take(body_request_cap)
        .collect::<Vec<_>>();
    let body_followup_headers = select_eth_fullnode_native_header_followup_body_headers_v1(
        chain_id,
        &imported_headers,
        budget_hooks,
    );
    session.pending_body_headers = body_followup_headers
        .iter()
        .map(|header| EthFullnodeNativePendingBodyHeaderV1 {
            number: header.number,
            hash: header.hash,
            parent_hash: header.parent_hash,
            state_root: header.state_root,
            transactions_root: header.transactions_root,
            receipts_root: header.receipts_root,
            tx_count: None,
            withdrawal_count: None,
        })
        .collect();
    for header in imported_headers.iter().copied() {
        if let Some(raw_rlp) = header.raw_rlp.as_deref() {
            set_network_runtime_native_header_rlp_v1(chain_id, header.hash, raw_rlp);
        }
        let header_wire = evm_native_header_wire_from_rlpx_header_v1(header);
        ingest_runtime_native_header_from_evm_wire(chain_id, source_peer_id, &header_wire);
        queue_eth_fullnode_native_block_access_list_hash_v1(
            chain_id,
            session,
            header.hash,
            header.block_access_list_hash,
            header.gas_limit,
            None,
        );
        report.header_updates = report.header_updates.saturating_add(1);
    }
    let hashes = session
        .pending_body_headers
        .iter()
        .map(|pending| pending.hash)
        .collect::<Vec<_>>();
    if !hashes.is_empty() {
        let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
        let payload = eth_rlpx_build_get_block_bodies_payload_v1(request_id, hashes.as_slice());
        eth_rlpx_write_wire_frame_v1(
            &mut session.stream,
            &mut session.frame_session,
            ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_GET_BLOCK_BODIES_MSG,
            payload.as_slice(),
        )
        .map_err(|err| {
            observe_eth_fullnode_rlpx_request_write_error_v1(
                chain_id,
                source_peer_id,
                "bodies_request_write_failed",
                err.as_str(),
            );
            NetworkError::Io(err)
        })?;
        observe_eth_native_bodies_pull(chain_id);
        observe_network_runtime_eth_peer_syncing_v1(chain_id, source_peer_id);
        session.last_bodies_request_id = Some(request_id);
        session.pending_body_request_offset = 0;
        session.last_receipts_request_id = None;
        session.pending_receipt_request_offset = 0;
        mark_eth_fullnode_native_recovery_inflight_v1(
            chain_id,
            source_peer_id,
            request_id,
            ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_BODY_KIND_V1,
            session.pending_body_headers.as_slice(),
        );
        clear_eth_fullnode_native_snap_request_state_v1(session);
        session.last_sync_request_unix_ms = now_unix_ms();
        report.sync_requests = report.sync_requests.saturating_add(1);
        eprintln!(
            "network_info: rlpx stage bodies_requested chain_id={} peer={} endpoint={} request_id={} blocks={}",
            chain_id,
            source_peer_id,
            session.endpoint.addr_hint,
            request_id,
            session.pending_body_headers.len()
        );
    }
    Ok(())
}

fn select_eth_fullnode_native_header_followup_body_headers_v1<'a>(
    chain_id: u64,
    headers: &[&'a EthRlpxBlockHeaderRecordV1],
    budget_hooks: &EthFullnodeBudgetHooksV1,
) -> Vec<&'a EthRlpxBlockHeaderRecordV1> {
    let Some(latest) = headers.last().copied() else {
        return Vec::new();
    };
    let product_current_head_first =
        budget_hooks.sync_pull_finalize_batch > ETH_FULLNODE_DEFAULT_SYNC_PULL_FINALIZE_BATCH;
    if headers.len() > 1
        && (product_current_head_first
            || get_network_runtime_sync_status(chain_id)
                .is_some_and(|status| status.highest_block > latest.number))
    {
        return vec![latest];
    }
    headers.to_vec()
}

fn validate_eth_fullnode_native_rlpx_block_headers_response_matches_request_v1(
    request: &EthRlpxGetBlockHeadersRequestV1,
    headers: &[&EthRlpxBlockHeaderRecordV1],
) -> Result<(), String> {
    if headers.is_empty() {
        return Ok(());
    }
    if request.max_headers == 0 {
        return Err(format!(
            "rlpx_block_headers_unrequested_nonempty:request_id={} observed={}",
            request.request_id,
            headers.len()
        ));
    }
    if headers.len() as u64 > request.max_headers {
        return Err(format!(
            "rlpx_block_headers_count_exceeds_request:request_id={} requested={} observed={}",
            request.request_id,
            request.max_headers,
            headers.len()
        ));
    }
    let first = headers[0];
    if let Some(origin_hash) = request.origin_hash {
        if first.hash != origin_hash {
            return Err(format!(
                "rlpx_block_headers_origin_hash_mismatch:request_id={} observed=0x{} expected=0x{}",
                request.request_id,
                hex32_v1(&first.hash),
                hex32_v1(&origin_hash)
            ));
        }
        if request.start_height > 0 && first.number != request.start_height {
            return Err(format!(
                "rlpx_block_headers_origin_number_mismatch:request_id={} observed={} expected={}",
                request.request_id, first.number, request.start_height
            ));
        }
    } else if first.number != request.start_height {
        return Err(format!(
            "rlpx_block_headers_origin_number_mismatch:request_id={} observed={} expected={}",
            request.request_id, first.number, request.start_height
        ));
    }
    let Some(step) = request.skip.checked_add(1) else {
        return Err(format!(
            "rlpx_block_headers_step_overflow:request_id={} skip={}",
            request.request_id, request.skip
        ));
    };
    for pair in headers.windows(2) {
        let parent = pair[0];
        let child = pair[1];
        let expected_number = if request.reverse {
            let Some(expected_number) = parent.number.checked_sub(step) else {
                return Err(format!(
                    "rlpx_block_headers_number_underflow:request_id={} parent_number={} parent_hash=0x{}",
                    request.request_id,
                    parent.number,
                    hex32_v1(&parent.hash)
                ));
            };
            expected_number
        } else {
            let Some(expected_number) = parent.number.checked_add(step) else {
                return Err(format!(
                    "rlpx_block_headers_number_overflow:request_id={} parent_number={} parent_hash=0x{}",
                    request.request_id,
                    parent.number,
                    hex32_v1(&parent.hash)
                ));
            };
            expected_number
        };
        if child.number != expected_number {
            return Err(format!(
                "rlpx_block_headers_number_gap:request_id={} parent_number={} child_number={} expected_child_number={} parent_hash=0x{} child_hash=0x{}",
                request.request_id,
                parent.number,
                child.number,
                expected_number,
                hex32_v1(&parent.hash),
                hex32_v1(&child.hash)
            ));
        }
        if step != 1 {
            continue;
        }
        if request.reverse {
            if parent.parent_hash != child.hash {
                return Err(format!(
                    "rlpx_block_headers_reverse_parent_mismatch:request_id={} parent_number={} observed_parent=0x{} expected_parent=0x{}",
                    request.request_id,
                    parent.number,
                    hex32_v1(&parent.parent_hash),
                    hex32_v1(&child.hash)
                ));
            }
        } else if child.parent_hash != parent.hash {
            return Err(format!(
                "rlpx_block_headers_parent_mismatch:request_id={} child_number={} child_parent=0x{} expected_parent=0x{}",
                request.request_id,
                child.number,
                hex32_v1(&child.parent_hash),
                hex32_v1(&parent.hash)
            ));
        }
    }
    Ok(())
}

fn ingest_real_rlpx_block_bodies_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    bodies: &EthRlpxBlockBodiesResponseV1,
    report: &mut EthFullnodeNativeRlpxPeerTickReportV1,
) -> Result<(), NetworkError> {
    let Some(request_id) = session.last_bodies_request_id else {
        let err = format!(
            "rlpx_block_bodies_unexpected_response:request_id={}",
            bodies.request_id
        );
        observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
        return Err(NetworkError::Decode(err));
    };
    if request_id != bodies.request_id {
        return Ok(());
    }
    clear_eth_fullnode_native_recovery_inflight_request_v1(
        chain_id,
        source_peer_id,
        request_id,
        ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_BODY_KIND_V1,
    );
    observe_eth_native_bodies_response(chain_id);
    clear_eth_fullnode_native_headers_request_state_v1(session);
    let request_offset = session
        .pending_body_request_offset
        .min(session.pending_body_headers.len());
    let expected_bodies = session
        .pending_body_headers
        .len()
        .saturating_sub(request_offset);
    if bodies.bodies.len() > expected_bodies {
        let err = format!(
            "rlpx_block_bodies_count_mismatch:expected={} observed={}",
            expected_bodies,
            bodies.bodies.len()
        );
        observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
        return Err(NetworkError::Decode(err));
    }
    let observed_bodies = bodies.bodies.len();
    let mut matched_pending_indices = HashSet::<usize>::new();
    for (idx, body) in bodies.bodies.iter().enumerate() {
        let pending_idx = request_offset.saturating_add(idx);
        let target_idx = session
            .pending_body_headers
            .get(pending_idx)
            .filter(|pending| {
                pending.tx_count.is_none() && pending.transactions_root == body.transactions_root
            })
            .map(|_| pending_idx)
            .or_else(|| {
                let matches = session
                    .pending_body_headers
                    .iter()
                    .enumerate()
                    .filter(|(candidate_idx, pending)| {
                        !matched_pending_indices.contains(candidate_idx)
                            && pending.tx_count.is_none()
                            && pending.transactions_root == body.transactions_root
                    })
                    .map(|(candidate_idx, _)| candidate_idx)
                    .collect::<Vec<_>>();
                if matches.len() == 1 {
                    matches.first().copied()
                } else {
                    None
                }
            });
        let Some(target_idx) = target_idx else {
            let expected = session.pending_body_headers.get(pending_idx).or_else(|| {
                session
                    .pending_body_headers
                    .iter()
                    .find(|pending| pending.tx_count.is_none())
            });
            let err = if let Some(pending) = expected {
                format!(
                    "rlpx_block_body_transactions_root_mismatch:number={number} hash=0x{}",
                    hex32_v1(&pending.hash),
                    number = pending.number
                )
            } else {
                "rlpx_block_body_transactions_root_mismatch:no_pending_body_header".to_string()
            };
            observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
            return Err(NetworkError::Decode(err));
        };
        matched_pending_indices.insert(target_idx);
        let Some((pending_number, pending_hash)) = session
            .pending_body_headers
            .get_mut(target_idx)
            .map(|pending| {
                pending.tx_count = Some(body.tx_hashes.len());
                pending.withdrawal_count = body.withdrawal_count;
                (pending.number, pending.hash)
            })
        else {
            continue;
        };
        update_eth_fullnode_native_block_access_list_body_context_v1(
            session,
            pending_hash,
            body.tx_hashes.len(),
        );
        let body_wire = evm_native_body_wire_from_rlpx_body_v1(pending_number, pending_hash, body);
        ingest_runtime_native_body_from_evm_wire(chain_id, source_peer_id, &body_wire);
        report.body_updates = report.body_updates.saturating_add(1);
    }
    session.last_bodies_request_id = None;
    session.pending_body_request_offset = 0;
    eprintln!(
        "network_info: rlpx stage bodies_received chain_id={} peer={} endpoint={} negotiated_eth={} request_id={} blocks={} expected_blocks={}",
        chain_id,
        source_peer_id,
        session.endpoint.addr_hint,
        session._negotiated_eth_version,
        bodies.request_id,
        observed_bodies,
        expected_bodies,
    );
    if observed_bodies == 0 && expected_bodies > 0 {
        return Err(NetworkError::Io(format!(
            "rlpx_block_bodies_empty_response:request_id={} expected_blocks={}",
            bodies.request_id, expected_bodies
        )));
    }
    if observed_bodies < expected_bodies
        || session
            .pending_body_headers
            .iter()
            .skip(request_offset)
            .any(|pending| pending.tx_count.is_none())
    {
        let next_request_offset = session
            .pending_body_headers
            .iter()
            .enumerate()
            .skip(request_offset)
            .find(|(_, pending)| pending.tx_count.is_none())
            .map(|(idx, _)| idx)
            .unwrap_or(session.pending_body_headers.len());
        let missing_hashes = session
            .pending_body_headers
            .iter()
            .skip(next_request_offset)
            .filter(|pending| pending.tx_count.is_none())
            .map(|pending| pending.hash)
            .collect::<Vec<_>>();
        if !missing_hashes.is_empty() {
            let retry_pending_headers = session
                .pending_body_headers
                .iter()
                .skip(next_request_offset)
                .filter(|pending| pending.tx_count.is_none())
                .copied()
                .collect::<Vec<_>>();
            let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
            let payload =
                eth_rlpx_build_get_block_bodies_payload_v1(request_id, missing_hashes.as_slice());
            eth_rlpx_write_wire_frame_v1(
                &mut session.stream,
                &mut session.frame_session,
                ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_GET_BLOCK_BODIES_MSG,
                payload.as_slice(),
            )
            .map_err(|err| {
                observe_eth_fullnode_rlpx_request_write_error_v1(
                    chain_id,
                    source_peer_id,
                    "partial_bodies_retry_request_write_failed",
                    err.as_str(),
                );
                NetworkError::Io(err)
            })?;
            observe_eth_native_bodies_pull(chain_id);
            observe_network_runtime_eth_peer_syncing_v1(chain_id, source_peer_id);
            session.last_bodies_request_id = Some(request_id);
            session.pending_body_request_offset = next_request_offset;
            mark_eth_fullnode_native_recovery_inflight_v1(
                chain_id,
                source_peer_id,
                request_id,
                ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_BODY_KIND_V1,
                retry_pending_headers.as_slice(),
            );
            session.last_sync_request_unix_ms = now_unix_ms();
            report.sync_requests = report.sync_requests.saturating_add(1);
            eprintln!(
                "network_info: rlpx stage block_bodies_partial_retry chain_id={} peer={} endpoint={} request_id={} received={} missing={}",
                chain_id,
                source_peer_id,
                session.endpoint.addr_hint,
                request_id,
                observed_bodies,
                missing_hashes.len()
            );
            return Ok(());
        }
    }
    let empty_receipts = materialize_empty_receipts_for_pending_body_headers_v1(
        chain_id,
        source_peer_id,
        &mut session.pending_body_headers,
    )?;
    report.receipt_updates = report.receipt_updates.saturating_add(empty_receipts);
    let hashes = session
        .pending_body_headers
        .iter()
        .map(|pending| pending.hash)
        .collect::<Vec<_>>();
    if hashes.is_empty() {
        session.pending_body_headers.clear();
        if dispatch_eth_fullnode_native_rlpx_queued_block_access_lists_v1(
            chain_id,
            source_peer_id,
            session,
            report,
        )? {
            return Ok(());
        }
        mark_network_runtime_eth_peer_session_ready_v1(chain_id, source_peer_id, None);
        return Ok(());
    }
    let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
    let payload = eth_rlpx_build_get_receipts_payload_v1(
        request_id,
        0,
        hashes.as_slice(),
        session._negotiated_eth_version,
    );
    eth_rlpx_write_wire_frame_v1(
        &mut session.stream,
        &mut session.frame_session,
        ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_GET_RECEIPTS_MSG,
        payload.as_slice(),
    )
    .map_err(|err| {
        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            source_peer_id,
            "receipts_request_write_failed",
            err.as_str(),
        );
        NetworkError::Io(err)
    })?;
    observe_network_runtime_eth_peer_syncing_v1(chain_id, source_peer_id);
    session.last_receipts_request_id = Some(request_id);
    session.pending_receipt_request_offset = 0;
    mark_eth_fullnode_native_recovery_inflight_v1(
        chain_id,
        source_peer_id,
        request_id,
        ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_RECEIPT_KIND_V1,
        session.pending_body_headers.as_slice(),
    );
    clear_eth_fullnode_native_snap_request_state_v1(session);
    session.last_sync_request_unix_ms = now_unix_ms();
    report.sync_requests = report.sync_requests.saturating_add(1);
    Ok(())
}

fn pending_body_header_can_materialize_empty_receipts_v1(
    pending: &EthFullnodeNativePendingBodyHeaderV1,
) -> bool {
    pending.tx_count == Some(0) && pending.receipts_root == crate::eth_rlpx_empty_trie_root_v1()
}

fn materialize_empty_receipts_for_pending_body_headers_v1(
    chain_id: u64,
    source_peer_id: u64,
    pending_body_headers: &mut Vec<EthFullnodeNativePendingBodyHeaderV1>,
) -> Result<usize, NetworkError> {
    validate_real_rlpx_state_root_continuity_v1(
        chain_id,
        source_peer_id,
        pending_body_headers.as_slice(),
    )?;
    let mut remaining = Vec::with_capacity(pending_body_headers.len());
    let mut materialized = 0usize;
    for pending in pending_body_headers.drain(..) {
        if pending_body_header_can_materialize_empty_receipts_v1(&pending) {
            set_network_runtime_native_receipt_snapshot_v1(
                chain_id,
                NetworkRuntimeNativeReceiptSnapshotV1 {
                    chain_id,
                    number: pending.number,
                    block_hash: pending.hash,
                    receipts_root: pending.receipts_root,
                    raw_receipts: Vec::new(),
                    receipt_count: 0,
                    receipts_available: true,
                    source_peer_id: Some(source_peer_id),
                    observed_unix_ms: now_unix_millis_u128(),
                },
            );
            materialized = materialized.saturating_add(1);
        } else {
            remaining.push(pending);
        }
    }
    *pending_body_headers = remaining;
    Ok(materialized)
}

fn ingest_real_rlpx_receipts_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    receipts: &EthRlpxReceiptsResponseV1,
    report: &mut EthFullnodeNativeRlpxPeerTickReportV1,
) -> Result<(), NetworkError> {
    let Some(request_id) = session.last_receipts_request_id else {
        let err = format!(
            "rlpx_receipts_unexpected_response:request_id={}",
            receipts.request_id
        );
        observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
        return Err(NetworkError::Decode(err));
    };
    if request_id != receipts.request_id {
        return Ok(());
    }
    clear_eth_fullnode_native_recovery_inflight_request_v1(
        chain_id,
        source_peer_id,
        request_id,
        ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_RECEIPT_KIND_V1,
    );
    let request_offset = session
        .pending_receipt_request_offset
        .min(session.pending_body_headers.len());
    let expected_receipts = session
        .pending_body_headers
        .len()
        .saturating_sub(request_offset);
    let observed_receipts = receipts.blocks.len();
    let receipt_end = request_offset.saturating_add(observed_receipts);
    let Some(pending_receipt_headers) = session
        .pending_body_headers
        .get(request_offset..receipt_end)
    else {
        let err = format!(
            "rlpx_receipts_block_count_mismatch:expected={} observed={}",
            expected_receipts, observed_receipts
        );
        observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
        return Err(NetworkError::Decode(err));
    };
    validate_real_rlpx_receipts_response_v1(
        chain_id,
        source_peer_id,
        pending_receipt_headers,
        expected_receipts,
        receipts,
    )?;
    validate_real_rlpx_state_root_continuity_v1(chain_id, source_peer_id, pending_receipt_headers)?;
    ingest_validated_real_rlpx_receipt_snapshots_v1(
        chain_id,
        source_peer_id,
        pending_receipt_headers,
        receipts,
    );
    report.receipt_updates = report.receipt_updates.saturating_add(receipts.blocks.len());
    eprintln!(
        "network_info: rlpx stage receipts_received chain_id={} peer={} endpoint={} negotiated_eth={} request_id={} blocks={} last_block_incomplete={}",
        chain_id,
        source_peer_id,
        session.endpoint.addr_hint,
        session._negotiated_eth_version,
        receipts.request_id,
        receipts.blocks.len(),
        receipts.last_block_incomplete,
    );
    session.last_receipts_request_id = None;
    session.pending_receipt_request_offset = 0;
    if observed_receipts < expected_receipts {
        let next_request_offset = receipt_end;
        let missing_hashes = session
            .pending_body_headers
            .iter()
            .skip(next_request_offset)
            .map(|pending| pending.hash)
            .collect::<Vec<_>>();
        if !missing_hashes.is_empty() {
            let retry_pending_headers = session
                .pending_body_headers
                .iter()
                .skip(next_request_offset)
                .copied()
                .collect::<Vec<_>>();
            let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
            let payload = eth_rlpx_build_get_receipts_payload_v1(
                request_id,
                0,
                missing_hashes.as_slice(),
                session._negotiated_eth_version,
            );
            eth_rlpx_write_wire_frame_v1(
                &mut session.stream,
                &mut session.frame_session,
                ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_GET_RECEIPTS_MSG,
                payload.as_slice(),
            )
            .map_err(|err| {
                observe_eth_fullnode_rlpx_request_write_error_v1(
                    chain_id,
                    source_peer_id,
                    "partial_receipts_retry_request_write_failed",
                    err.as_str(),
                );
                NetworkError::Io(err)
            })?;
            observe_network_runtime_eth_peer_syncing_v1(chain_id, source_peer_id);
            session.last_receipts_request_id = Some(request_id);
            session.pending_receipt_request_offset = next_request_offset;
            mark_eth_fullnode_native_recovery_inflight_v1(
                chain_id,
                source_peer_id,
                request_id,
                ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_RECEIPT_KIND_V1,
                retry_pending_headers.as_slice(),
            );
            session.last_sync_request_unix_ms = now_unix_ms();
            report.sync_requests = report.sync_requests.saturating_add(1);
            eprintln!(
                "network_info: rlpx stage receipts_partial_retry chain_id={} peer={} endpoint={} request_id={} received={} missing={}",
                chain_id,
                source_peer_id,
                session.endpoint.addr_hint,
                request_id,
                observed_receipts,
                missing_hashes.len()
            );
            return Ok(());
        }
    }
    session.pending_body_headers.clear();
    if dispatch_eth_fullnode_native_rlpx_queued_block_access_lists_v1(
        chain_id,
        source_peer_id,
        session,
        report,
    )? {
        return Ok(());
    }
    mark_network_runtime_eth_peer_session_ready_v1(chain_id, source_peer_id, None);
    Ok(())
}

fn ingest_validated_real_rlpx_receipt_snapshots_v1(
    chain_id: u64,
    source_peer_id: u64,
    pending_body_headers: &[EthFullnodeNativePendingBodyHeaderV1],
    receipts: &EthRlpxReceiptsResponseV1,
) {
    for (pending, block_receipts) in pending_body_headers.iter().zip(receipts.blocks.iter()) {
        let receipts_root =
            eth_rlpx_receipts_root_from_raw_receipts_v1(block_receipts.raw_receipts.as_slice());
        set_network_runtime_native_receipt_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeReceiptSnapshotV1 {
                chain_id,
                number: pending.number,
                block_hash: pending.hash,
                receipts_root,
                raw_receipts: block_receipts.raw_receipts.clone(),
                receipt_count: block_receipts.receipt_count,
                receipts_available: block_receipts.receipts_available,
                source_peer_id: Some(source_peer_id),
                observed_unix_ms: now_unix_millis_u128(),
            },
        );
    }
}

fn validate_real_rlpx_state_root_continuity_v1(
    chain_id: u64,
    source_peer_id: u64,
    pending_body_headers: &[EthFullnodeNativePendingBodyHeaderV1],
) -> Result<(), NetworkError> {
    let retained_blocks = snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 4096);
    for pending in pending_body_headers {
        if pending.tx_count != Some(0) || pending.withdrawal_count != Some(0) {
            continue;
        }
        let Some(parent) = retained_blocks
            .iter()
            .find(|block| block.hash == pending.parent_hash)
        else {
            continue;
        };
        if pending.state_root != parent.state_root {
            let err = format!(
                "rlpx_state_root_continuity_mismatch:number={} hash=0x{} parent=0x{}",
                pending.number,
                hex32_v1(&pending.hash),
                hex32_v1(&pending.parent_hash)
            );
            observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
            return Err(NetworkError::Decode(err));
        }
        set_network_runtime_native_state_root_validation_v1(
            chain_id,
            pending.hash,
            true,
            "empty_body_parent_state_root_continuity",
            now_unix_millis_u128(),
        );
    }
    Ok(())
}

fn validate_real_rlpx_receipts_response_v1(
    chain_id: u64,
    source_peer_id: u64,
    pending_body_headers: &[EthFullnodeNativePendingBodyHeaderV1],
    expected_receipt_blocks: usize,
    receipts: &EthRlpxReceiptsResponseV1,
) -> Result<(), NetworkError> {
    if receipts.last_block_incomplete {
        let err = "rlpx_receipts_last_block_incomplete".to_string();
        observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
        return Err(NetworkError::Decode(err));
    }
    if receipts.blocks.len() > expected_receipt_blocks {
        let err = format!(
            "rlpx_receipts_block_count_mismatch:expected={} observed={}",
            expected_receipt_blocks,
            receipts.blocks.len()
        );
        observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
        return Err(NetworkError::Decode(err));
    }
    for (idx, block_receipts) in receipts.blocks.iter().enumerate() {
        let Some(pending) = pending_body_headers.get(idx).copied() else {
            break;
        };
        let Some(expected_tx_count) = pending.tx_count else {
            let err = format!(
                "rlpx_receipts_without_materialized_body:number={} hash=0x{}",
                pending.number,
                hex32_v1(&pending.hash)
            );
            observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
            return Err(NetworkError::Decode(err));
        };
        if block_receipts.receipt_count != expected_tx_count {
            let err = format!(
                "rlpx_receipts_count_mismatch:number={} hash=0x{} expected={} observed={}",
                pending.number,
                hex32_v1(&pending.hash),
                expected_tx_count,
                block_receipts.receipt_count
            );
            observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
            return Err(NetworkError::Decode(err));
        }
        let observed_receipts_root =
            eth_rlpx_receipts_root_from_raw_receipts_v1(block_receipts.raw_receipts.as_slice());
        if observed_receipts_root != pending.receipts_root {
            let err = format!(
                "rlpx_receipts_root_mismatch:number={} hash=0x{}",
                pending.number,
                hex32_v1(&pending.hash)
            );
            observe_network_runtime_eth_peer_decode_failure_v1(chain_id, source_peer_id, &err);
            return Err(NetworkError::Decode(err));
        }
    }
    Ok(())
}

fn dispatch_eth_fullnode_native_rlpx_tx_broadcast_v1(
    chain_id: u64,
    _local_node: NodeId,
    peer: NodeId,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    budget_hooks: &EthFullnodeBudgetHooksV1,
) -> Result<(), NetworkError> {
    let candidates = snapshot_network_runtime_native_pending_tx_broadcast_candidates_v1(
        chain_id,
        budget_hooks.tx_broadcast_max_per_tick.max(1) as usize,
        budget_hooks.tx_broadcast_max_propagations.max(1),
    );
    if candidates.is_empty() {
        return Ok(());
    }
    let candidate_count = candidates.len() as u64;
    let tx_types = candidates
        .iter()
        .map(|candidate| {
            eth_rlpx_tx_announce_type_from_envelope_v1(candidate.tx_payload.as_slice())
        })
        .collect::<Vec<_>>();
    let tx_sizes = candidates
        .iter()
        .map(|candidate| candidate.tx_payload_len.min(u32::MAX as usize) as u32)
        .collect::<Vec<_>>();
    let tx_hashes = candidates
        .iter()
        .map(|candidate| candidate.tx_hash)
        .collect::<Vec<_>>();
    let payload = eth_rlpx_build_new_pooled_transaction_hashes_payload_v1(
        tx_types.as_slice(),
        tx_sizes.as_slice(),
        tx_hashes.as_slice(),
    );
    eth_rlpx_write_wire_frame_v1(
        &mut session.stream,
        &mut session.frame_session,
        ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_NEW_POOLED_TRANSACTION_HASHES_MSG,
        payload.as_slice(),
    )
    .map_err(|err| {
        for candidate in &candidates {
            observe_network_runtime_native_pending_tx_propagation_failure_v1(
                chain_id,
                candidate.tx_hash,
                Some(peer.0),
                NetworkRuntimeNativePendingTxPropagationStopReasonV1::IoWriteFailure,
                "pooled_tx_hash_announce_dispatch",
            );
        }
        observe_network_runtime_native_pending_tx_broadcast_dispatch_v1(
            chain_id,
            peer.0,
            candidate_count,
            0,
            false,
        );
        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            peer.0,
            "pooled_tx_hashes_write_failed",
            err.as_str(),
        );
        NetworkError::Io(err)
    })?;
    for candidate in candidates {
        observe_network_runtime_native_pending_tx_propagated_with_context_v1(
            chain_id,
            candidate.tx_hash,
            Some(peer.0),
            Some("pooled_tx_hash_announce_dispatch"),
            Some(budget_hooks.tx_broadcast_max_propagations.max(1)),
        );
    }
    observe_network_runtime_native_pending_tx_broadcast_dispatch_v1(
        chain_id,
        peer.0,
        candidate_count,
        candidate_count,
        true,
    );
    Ok(())
}

fn eth_rlpx_tx_announce_type_from_envelope_v1(envelope: &[u8]) -> u8 {
    if envelope.len() > 1 && envelope[0] <= 0x7f {
        envelope[0]
    } else {
        0
    }
}

pub fn drive_eth_fullnode_native_peer_once_v1<T: Transport>(
    transport: &T,
    local_node: NodeId,
    peer: NodeId,
    chain_id: u64,
    recv_budget: usize,
) -> Result<EthFullnodeNativeDriveReportV1, NetworkError> {
    drive_eth_fullnode_native_peers_once_v1(
        transport,
        local_node,
        std::slice::from_ref(&peer),
        chain_id,
        recv_budget,
    )
}

pub fn drive_eth_fullnode_native_peers_once_v1<T: Transport>(
    transport: &T,
    local_node: NodeId,
    peers: &[NodeId],
    chain_id: u64,
    recv_budget: usize,
) -> Result<EthFullnodeNativeDriveReportV1, NetworkError> {
    let runtime_config = resolve_eth_fullnode_native_runtime_config_v1(chain_id);
    let budget_hooks = runtime_config.budget_hooks;
    let effective_recv_budget = if recv_budget == 0 {
        budget_hooks.native_recv_budget_per_tick.max(1) as usize
    } else {
        recv_budget
            .min(budget_hooks.native_recv_budget_per_tick.max(1) as usize)
            .max(1)
    };
    EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
        chain_id,
        local_node,
        peers: peers.to_vec(),
        peer_endpoints: Vec::new(),
        recv_budget: effective_recv_budget,
        sync_target_fanout: budget_hooks.sync_target_fanout.max(1) as usize,
        budget_hooks,
    })
    .drive_once(transport)
}

pub fn drive_eth_fullnode_native_peer_endpoints_once_v1(
    local_node: NodeId,
    peer_endpoints: &[PluginPeerEndpoint],
    chain_id: u64,
    recv_budget: usize,
) -> Result<EthFullnodeNativeRealDriveReportV1, NetworkError> {
    let runtime_config = resolve_eth_fullnode_native_runtime_config_v1(chain_id);
    let budget_hooks = runtime_config.budget_hooks;
    let effective_recv_budget = if recv_budget == 0 {
        budget_hooks.native_recv_budget_per_tick.max(1) as usize
    } else {
        recv_budget
            .min(budget_hooks.native_recv_budget_per_tick.max(1) as usize)
            .max(1)
    };
    EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
        chain_id,
        local_node,
        peers: peer_endpoints
            .iter()
            .map(|endpoint| NodeId(endpoint.node_hint.max(1)))
            .collect(),
        peer_endpoints: peer_endpoints.to_vec(),
        recv_budget: effective_recv_budget,
        sync_target_fanout: budget_hooks.sync_target_fanout.max(1) as usize,
        budget_hooks,
    })
    .drive_real_network_once()
}

const RUNTIME_SYNC_PULL_REQUEST_MAGIC: [u8; 4] = *b"NSP1";
const RUNTIME_SYNC_PULL_REQUEST_LEN: usize = 4 + 1 + 8 + 8 + 8;
const RUNTIME_SYNC_PULL_RESPONSE_BATCH_MAX: u64 = 128;
const DEFAULT_TCP_CONNECT_RETRY_ATTEMPTS: usize = 2;
const DEFAULT_TCP_CONNECT_RETRY_BACKOFF_MS: u64 = 0;
const PEER_IP_HINT_AMBIGUOUS: u64 = u64::MAX;
const RUNTIME_SYNC_PULL_PREFETCH_MARGIN_HEADERS: u64 = 8;
const RUNTIME_SYNC_PULL_PREFETCH_MARGIN_BODIES: u64 = 4;
const RUNTIME_SYNC_PULL_PREFETCH_MARGIN_STATE: u64 = 2;
const RUNTIME_SYNC_PULL_PREFETCH_MARGIN_FINALIZE: u64 = 1;
const DEFAULT_RUNTIME_SYNC_PULL_FOLLOWUP_FANOUT_MAX: usize = 1;
const HARD_MAX_RUNTIME_SYNC_PULL_FOLLOWUP_FANOUT: usize = 8;
static RUNTIME_SYNC_PULL_FOLLOWUP_FANOUT_MAX_CACHE: OnceLock<usize> = OnceLock::new();
static LOCAL_OBSERVED_PEERS: OnceLock<DashMap<String, LocalObservedPeer>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalObservedPeer {
    pub node_id: String,
    pub addr_hint: String,
    pub last_seen_unix_ms: u64,
}

pub fn snapshot_local_observed_peers() -> Vec<LocalObservedPeer> {
    let mut peers: Vec<_> = local_observed_peers_registry()
        .iter()
        .map(|entry| entry.value().clone())
        .collect();
    peers.sort_by(|left, right| {
        left.node_id
            .cmp(&right.node_id)
            .then_with(|| left.addr_hint.cmp(&right.addr_hint))
    });
    peers
}

fn local_observed_peers_registry() -> &'static DashMap<String, LocalObservedPeer> {
    LOCAL_OBSERVED_PEERS.get_or_init(DashMap::new)
}

#[cfg(test)]
fn clear_local_observed_peers_registry() {
    local_observed_peers_registry().clear();
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// Source-rank guardrail for routing seeding:
// LocalObserved > OperatorForced.
// Only exact peer_addr_index hits may enter this registry.
fn observe_local_observed_peer(peer: &NodeId, addr: SocketAddr) {
    local_observed_peers_registry().insert(
        peer.0.to_string(),
        LocalObservedPeer {
            node_id: peer.0.to_string(),
            addr_hint: addr.to_string(),
            last_seen_unix_ms: now_unix_ms(),
        },
    );
}

fn observe_local_observed_peer_from_exact_addr_index(
    peer_addr_index: &DashMap<SocketAddr, NodeId>,
    addr: SocketAddr,
) {
    if let Some(peer) = peer_addr_index.get(&addr) {
        observe_local_observed_peer(peer.value(), addr);
    }
}

fn observe_local_observed_peer_from_confirmed_sender(
    peers: &DashMap<NodeId, SocketAddr>,
    msg_peer_id: Option<u64>,
    addr: SocketAddr,
) -> bool {
    let Some(msg_peer_id) = msg_peer_id else {
        return false;
    };
    let peer = NodeId(msg_peer_id);
    let Some(registered_addr) = peers.get(&peer) else {
        return false;
    };
    if *registered_addr.value() != addr {
        return false;
    }
    observe_local_observed_peer(&peer, addr);
    true
}

fn observe_local_observed_peer_from_transport_evidence(
    peers: &DashMap<NodeId, SocketAddr>,
    peer_addr_index: &DashMap<SocketAddr, NodeId>,
    msg_peer_id: Option<u64>,
    addr: SocketAddr,
) {
    if observe_local_observed_peer_from_confirmed_sender(peers, msg_peer_id, addr) {
        return;
    }
    observe_local_observed_peer_from_exact_addr_index(peer_addr_index, addr);
}

#[cfg(test)]
mod local_observed_tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddrV4};
    use std::sync::{Mutex, OnceLock};

    fn local_observed_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn exact_addr_index_observation_enters_snapshot() {
        let _guard = local_observed_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_local_observed_peers_registry();
        let peer_addr_index = DashMap::new();
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 30303));
        peer_addr_index.insert(addr, NodeId(7));

        observe_local_observed_peer_from_exact_addr_index(&peer_addr_index, addr);

        let snapshot = snapshot_local_observed_peers();
        assert!(snapshot
            .iter()
            .any(|peer| peer.node_id == "7" && peer.addr_hint == "127.0.0.1:30303"));
        clear_local_observed_peers_registry();
    }

    #[test]
    fn confirmed_sender_with_exact_registered_addr_enters_snapshot() {
        let _guard = local_observed_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_local_observed_peers_registry();
        let peers = DashMap::new();
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 40404));
        peers.insert(NodeId(9), addr);

        assert!(observe_local_observed_peer_from_confirmed_sender(
            &peers,
            Some(9),
            addr
        ));

        let snapshot = snapshot_local_observed_peers();
        assert!(snapshot
            .iter()
            .any(|peer| peer.node_id == "9" && peer.addr_hint == "127.0.0.1:40404"));
        clear_local_observed_peers_registry();
    }

    #[test]
    fn confirmed_sender_rejects_non_exact_registered_addr() {
        let _guard = local_observed_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_local_observed_peers_registry();
        let peers = DashMap::new();
        let registered = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 50505));
        let src = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 50506));
        peers.insert(NodeId(11), registered);

        assert!(!observe_local_observed_peer_from_confirmed_sender(
            &peers,
            Some(11),
            src
        ));
        assert!(!snapshot_local_observed_peers()
            .iter()
            .any(|peer| { peer.node_id == "11" && peer.addr_hint == "127.0.0.1:50506" }));
        clear_local_observed_peers_registry();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeSyncPullRequest {
    phase: NetworkRuntimeNativeSyncPhaseV1,
    chain_id: u64,
    from_block: u64,
    to_block: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct RuntimeSyncPullMessageContext {
    is_sync_pull: bool,
    request: Option<RuntimeSyncPullRequest>,
    header_height: Option<u64>,
}

#[derive(Debug, Clone)]
struct RuntimeSyncPullResponsePlan {
    to: NodeId,
    to_wire: u32,
    msg_type: DistributedOcccMessageType,
    response_from: u64,
    response_to: u64,
    timestamp: u64,
}

#[derive(Debug, Clone, Copy)]
struct RuntimeSyncPullTargetState {
    to_block: u64,
    followup_trigger_block: u64,
}

type RuntimeSyncPullTargetMap = DashMap<(u64, u64, u64), RuntimeSyncPullTargetState>;
static RUNTIME_SYNC_PULL_TARGETS: OnceLock<RuntimeSyncPullTargetMap> = OnceLock::new();

fn runtime_sync_pull_target_map() -> &'static RuntimeSyncPullTargetMap {
    RUNTIME_SYNC_PULL_TARGETS.get_or_init(DashMap::new)
}

#[cfg(test)]
fn set_runtime_sync_pull_target(
    chain_id: u64,
    local_node: NodeId,
    remote_peer: NodeId,
    to_block: u64,
) {
    set_runtime_sync_pull_target_with_trigger(
        chain_id,
        local_node,
        remote_peer,
        to_block,
        to_block,
    );
}

fn set_runtime_sync_pull_target_with_trigger(
    chain_id: u64,
    local_node: NodeId,
    remote_peer: NodeId,
    to_block: u64,
    followup_trigger_block: u64,
) {
    runtime_sync_pull_target_map().insert(
        (chain_id, local_node.0, remote_peer.0),
        RuntimeSyncPullTargetState {
            to_block,
            followup_trigger_block: followup_trigger_block.min(to_block),
        },
    );
}

#[cfg(test)]
fn get_runtime_sync_pull_target(
    chain_id: u64,
    local_node: NodeId,
    remote_peer: NodeId,
) -> Option<u64> {
    runtime_sync_pull_target_map()
        .get(&(chain_id, local_node.0, remote_peer.0))
        .map(|target| target.to_block)
}

fn clear_runtime_sync_pull_target(chain_id: u64, local_node: NodeId, remote_peer: NodeId) {
    runtime_sync_pull_target_map().remove(&(chain_id, local_node.0, remote_peer.0));
}

fn should_wait_runtime_sync_pull_target_window(
    chain_id: u64,
    local_node: NodeId,
    remote_peer: NodeId,
    observed_height: u64,
) -> bool {
    let key = (chain_id, local_node.0, remote_peer.0);
    let target_map = runtime_sync_pull_target_map();
    if let Some(target) = target_map.get(&key) {
        let target_to = target.to_block;
        let trigger = target.followup_trigger_block;
        drop(target);
        if observed_height < trigger {
            return true;
        }
        if observed_height >= target_to {
            target_map.remove(&key);
            return false;
        }
        // Prefetch trigger: near the tail of current window, start requesting
        // next window to hide pull RTT while preserving deterministic ordering.
        target_map.remove(&key);
    }
    false
}

fn parse_env_usize(name: &str, fallback: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(fallback)
}

fn parse_env_u64(name: &str, fallback: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(fallback)
}

fn runtime_sync_pull_followup_fanout_max() -> usize {
    *RUNTIME_SYNC_PULL_FOLLOWUP_FANOUT_MAX_CACHE.get_or_init(|| {
        parse_env_usize(
            "NOVOVM_NETWORK_SYNC_PULL_FOLLOWUP_FANOUT_MAX",
            DEFAULT_RUNTIME_SYNC_PULL_FOLLOWUP_FANOUT_MAX,
        )
        .clamp(1, HARD_MAX_RUNTIME_SYNC_PULL_FOLLOWUP_FANOUT)
    })
}

fn runtime_sync_pull_followup_targets(chain_id: u64, fallback_target: NodeId) -> Vec<NodeId> {
    let fanout_max = runtime_sync_pull_followup_fanout_max();
    if fanout_max == 1 {
        // Fast path: default fanout is 1, keep pulling on current response peer.
        // Avoid per-message top-k query overhead in the common path.
        return vec![fallback_target];
    }

    let mut targets: Vec<NodeId> = get_network_runtime_peer_heads_top_k(chain_id, fanout_max)
        .into_iter()
        .map(|(peer_id, _)| NodeId(peer_id))
        .collect();
    if targets.is_empty() {
        targets.push(fallback_target);
        return targets;
    }
    if !targets.contains(&fallback_target) && targets.len() < fanout_max {
        targets.push(fallback_target);
    }
    if targets.len() > fanout_max {
        targets.truncate(fanout_max);
    }
    targets
}

/// Simple in-memory transport for tests/bench harnesses.
///
/// This intentionally avoids async to keep the skeleton lightweight and portable.
#[derive(Debug, Clone)]
pub struct InMemoryTransport {
    inner: Arc<DashMap<NodeId, VecDeque<ProtocolMessage>>>,
    max_queue_len: usize,
}

impl InMemoryTransport {
    #[must_use]
    pub fn new(max_queue_len: usize) -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            max_queue_len,
        }
    }

    pub fn register(&self, node: NodeId) {
        self.inner
            .entry(node)
            .or_insert_with(|| VecDeque::with_capacity(self.max_queue_len.min(1024)));
    }
}

impl Transport for InMemoryTransport {
    fn send(&self, to: NodeId, msg: ProtocolMessage) -> Result<(), NetworkError> {
        let mut q = self
            .inner
            .get_mut(&to)
            .ok_or(NetworkError::PeerNotFound(to))?;
        if q.len() >= self.max_queue_len {
            return Err(NetworkError::QueueFull);
        }
        q.push_back(msg);
        Ok(())
    }

    fn try_recv(&self, me: NodeId) -> Result<Option<ProtocolMessage>, NetworkError> {
        let mut q = self
            .inner
            .get_mut(&me)
            .ok_or(NetworkError::PeerNotFound(me))?;
        Ok(q.pop_front())
    }
}

/// UDP transport for multi-process probe and lightweight local-node networking.
#[derive(Debug, Clone)]
pub struct UdpTransport {
    node: NodeId,
    chain_id: u64,
    socket: Arc<UdpSocket>,
    peers: Arc<DashMap<NodeId, SocketAddr>>,
    peer_addr_index: Arc<DashMap<SocketAddr, NodeId>>,
    peer_ip_hint_index: Arc<DashMap<IpAddr, u64>>,
    runtime_peer_registered: Arc<DashMap<NodeId, ()>>,
    recv_buf: Arc<Mutex<Vec<u8>>>,
}

/// TCP transport for multi-process / multi-host cluster probes.
///
/// This implementation intentionally prefers simplicity over throughput:
/// each `send` opens a short-lived TCP connection and sends a single frame.
#[derive(Debug, Clone)]
pub struct TcpTransport {
    node: NodeId,
    chain_id: u64,
    listener: Arc<TcpListener>,
    peers: Arc<DashMap<NodeId, SocketAddr>>,
    peer_addr_index: Arc<DashMap<SocketAddr, NodeId>>,
    peer_ip_hint_index: Arc<DashMap<IpAddr, u64>>,
    outbound_streams: Arc<DashMap<NodeId, Arc<Mutex<TcpStream>>>>,
    max_packet_size: usize,
    recv_frame_buf: Arc<Mutex<Vec<u8>>>,
    connect_timeout_ms: u64,
    connect_retry_attempts: usize,
    connect_retry_backoff_ms: u64,
}

impl TcpTransport {
    const DEFAULT_CHAIN_ID: u64 = 1;

    pub fn bind(node: NodeId, listen_addr: &str) -> Result<Self, NetworkError> {
        Self::bind_with_packet_size_for_chain(node, listen_addr, 64 * 1024, Self::DEFAULT_CHAIN_ID)
    }

    pub fn bind_with_packet_size(
        node: NodeId,
        listen_addr: &str,
        max_packet_size: usize,
    ) -> Result<Self, NetworkError> {
        Self::bind_with_packet_size_for_chain(
            node,
            listen_addr,
            max_packet_size,
            Self::DEFAULT_CHAIN_ID,
        )
    }

    pub fn bind_for_chain(
        node: NodeId,
        listen_addr: &str,
        chain_id: u64,
    ) -> Result<Self, NetworkError> {
        Self::bind_with_packet_size_for_chain(node, listen_addr, 64 * 1024, chain_id)
    }

    pub fn bind_with_packet_size_for_chain(
        node: NodeId,
        listen_addr: &str,
        max_packet_size: usize,
        chain_id: u64,
    ) -> Result<Self, NetworkError> {
        let listener =
            TcpListener::bind(listen_addr).map_err(|e| NetworkError::Io(e.to_string()))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| NetworkError::Io(e.to_string()))?;
        Ok(Self {
            node,
            chain_id,
            listener: Arc::new(listener),
            peers: Arc::new(DashMap::new()),
            peer_addr_index: Arc::new(DashMap::new()),
            peer_ip_hint_index: Arc::new(DashMap::new()),
            outbound_streams: Arc::new(DashMap::new()),
            max_packet_size: max_packet_size.max(1024),
            recv_frame_buf: Arc::new(Mutex::new(vec![0u8; max_packet_size.max(1024)])),
            connect_timeout_ms: 500,
            connect_retry_attempts: parse_env_usize(
                "NOVOVM_NETWORK_TCP_CONNECT_RETRY_ATTEMPTS",
                DEFAULT_TCP_CONNECT_RETRY_ATTEMPTS,
            )
            .max(1),
            connect_retry_backoff_ms: parse_env_u64(
                "NOVOVM_NETWORK_TCP_CONNECT_RETRY_BACKOFF_MS",
                DEFAULT_TCP_CONNECT_RETRY_BACKOFF_MS,
            ),
        })
    }

    pub fn register_peer(&self, node: NodeId, addr: &str) -> Result<(), NetworkError> {
        let parsed: SocketAddr = addr
            .parse()
            .map_err(|e: std::net::AddrParseError| NetworkError::AddressParse(e.to_string()))?;
        if let Some(old_addr) = self.peers.insert(node, parsed) {
            self.peer_addr_index.remove(&old_addr);
            if old_addr.ip() != parsed.ip() {
                refresh_peer_ip_hint_for_ip(&self.peers, &self.peer_ip_hint_index, old_addr.ip());
            }
        }
        self.peer_addr_index.insert(parsed, node);
        refresh_peer_ip_hint_for_ip(&self.peers, &self.peer_ip_hint_index, parsed.ip());
        let _ = register_network_runtime_peer(self.chain_id, node.0);
        Ok(())
    }

    pub fn unregister_peer(&self, node: NodeId) -> Result<(), NetworkError> {
        let Some((_, removed_addr)) = self.peers.remove(&node) else {
            return Err(NetworkError::PeerNotFound(node));
        };
        self.peer_addr_index.remove(&removed_addr);
        refresh_peer_ip_hint_for_ip(&self.peers, &self.peer_ip_hint_index, removed_addr.ip());
        clear_runtime_sync_pull_target(self.chain_id, self.node, node);
        self.outbound_streams.remove(&node);
        let _ = unregister_network_runtime_peer(self.chain_id, node.0);
        Ok(())
    }

    pub fn local_addr(&self) -> Result<SocketAddr, NetworkError> {
        self.listener
            .local_addr()
            .map_err(|e| NetworkError::Io(e.to_string()))
    }

    pub fn set_connect_timeout_ms(&mut self, timeout_ms: u64) {
        self.connect_timeout_ms = timeout_ms.max(1);
    }

    pub fn set_connect_retry_attempts(&mut self, attempts: usize) {
        self.connect_retry_attempts = attempts.max(1);
    }

    pub fn set_connect_retry_backoff_ms(&mut self, backoff_ms: u64) {
        self.connect_retry_backoff_ms = backoff_ms;
    }

    fn send_internal(&self, to: NodeId, msg: &ProtocolMessage) -> Result<(), NetworkError> {
        let peer = *self.peers.get(&to).ok_or(NetworkError::PeerNotFound(to))?;
        let encoded = protocol_encode(msg).map_err(|e| NetworkError::Encode(e.to_string()))?;
        if let Some(stream_arc) = self
            .outbound_streams
            .get(&to)
            .map(|entry| Arc::clone(entry.value()))
        {
            let write_result = {
                let mut guard = stream_arc
                    .lock()
                    .map_err(|_| NetworkError::Io("tcp stream lock poisoned".to_string()))?;
                write_tcp_frame(&mut guard, &encoded)
            };
            match write_result {
                Ok(()) => {
                    maybe_update_runtime_sync_local_progress_from_send(
                        self.chain_id,
                        self.node,
                        msg,
                    );
                    return Ok(());
                }
                Err(e) => {
                    self.outbound_streams.remove(&to);
                    if should_mark_peer_disconnected(&e) {
                        clear_runtime_sync_pull_target(self.chain_id, self.node, to);
                        let _ = unregister_network_runtime_peer(self.chain_id, to.0);
                    }
                }
            }
        }

        let mut last_err = None;
        let mut last_connect_io_error: Option<std::io::Error> = None;
        let mut stream_opt = None;
        for attempt_idx in 0..self.connect_retry_attempts {
            match TcpStream::connect_timeout(&peer, Duration::from_millis(self.connect_timeout_ms))
            {
                Ok(s) => {
                    stream_opt = Some(s);
                    break;
                }
                Err(e) => {
                    last_err = Some(e.to_string());
                    last_connect_io_error = Some(e);
                    let should_backoff = attempt_idx + 1 < self.connect_retry_attempts
                        && self.connect_retry_backoff_ms > 0;
                    if should_backoff {
                        std::thread::sleep(Duration::from_millis(self.connect_retry_backoff_ms));
                    }
                }
            }
        }
        if stream_opt.is_none() {
            if let Some(io_err) = last_connect_io_error.as_ref() {
                if should_mark_peer_disconnected(io_err) {
                    clear_runtime_sync_pull_target(self.chain_id, self.node, to);
                    let _ = unregister_network_runtime_peer(self.chain_id, to.0);
                }
            }
        }
        let mut stream = stream_opt.ok_or_else(|| {
            NetworkError::Io(format!(
                "tcp connect failed after retries: {}",
                last_err.unwrap_or_else(|| "unknown".to_string())
            ))
        })?;
        stream
            .set_nodelay(true)
            .map_err(|e| NetworkError::Io(e.to_string()))?;
        write_tcp_frame(&mut stream, &encoded).map_err(|e| {
            if should_mark_peer_disconnected(&e) {
                clear_runtime_sync_pull_target(self.chain_id, self.node, to);
                let _ = unregister_network_runtime_peer(self.chain_id, to.0);
            }
            NetworkError::Io(e.to_string())
        })?;
        self.outbound_streams
            .insert(to, Arc::new(Mutex::new(stream)));
        let _ = register_network_runtime_peer(self.chain_id, to.0);
        maybe_update_runtime_sync_local_progress_from_send(self.chain_id, self.node, msg);
        Ok(())
    }
}

impl UdpTransport {
    const DEFAULT_CHAIN_ID: u64 = 1;

    pub fn bind(node: NodeId, listen_addr: &str) -> Result<Self, NetworkError> {
        Self::bind_with_packet_size_for_chain(node, listen_addr, 64 * 1024, Self::DEFAULT_CHAIN_ID)
    }

    pub fn bind_with_packet_size(
        node: NodeId,
        listen_addr: &str,
        max_packet_size: usize,
    ) -> Result<Self, NetworkError> {
        Self::bind_with_packet_size_for_chain(
            node,
            listen_addr,
            max_packet_size,
            Self::DEFAULT_CHAIN_ID,
        )
    }

    pub fn bind_for_chain(
        node: NodeId,
        listen_addr: &str,
        chain_id: u64,
    ) -> Result<Self, NetworkError> {
        Self::bind_with_packet_size_for_chain(node, listen_addr, 64 * 1024, chain_id)
    }

    pub fn bind_with_packet_size_for_chain(
        node: NodeId,
        listen_addr: &str,
        max_packet_size: usize,
        chain_id: u64,
    ) -> Result<Self, NetworkError> {
        let socket = UdpSocket::bind(listen_addr).map_err(|e| NetworkError::Io(e.to_string()))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| NetworkError::Io(e.to_string()))?;
        Ok(Self {
            node,
            chain_id,
            socket: Arc::new(socket),
            peers: Arc::new(DashMap::new()),
            peer_addr_index: Arc::new(DashMap::new()),
            peer_ip_hint_index: Arc::new(DashMap::new()),
            runtime_peer_registered: Arc::new(DashMap::new()),
            recv_buf: Arc::new(Mutex::new(vec![0u8; max_packet_size.max(1024)])),
        })
    }

    pub fn register_peer(&self, node: NodeId, addr: &str) -> Result<(), NetworkError> {
        let parsed: SocketAddr = addr
            .parse()
            .map_err(|e: std::net::AddrParseError| NetworkError::AddressParse(e.to_string()))?;
        if let Some(old_addr) = self.peers.insert(node, parsed) {
            self.peer_addr_index.remove(&old_addr);
            if old_addr.ip() != parsed.ip() {
                refresh_peer_ip_hint_for_ip(&self.peers, &self.peer_ip_hint_index, old_addr.ip());
            }
        }
        self.peer_addr_index.insert(parsed, node);
        refresh_peer_ip_hint_for_ip(&self.peers, &self.peer_ip_hint_index, parsed.ip());
        if self.runtime_peer_registered.insert(node, ()).is_none() {
            let _ = register_network_runtime_peer(self.chain_id, node.0);
        }
        Ok(())
    }

    pub fn unregister_peer(&self, node: NodeId) -> Result<(), NetworkError> {
        let Some((_, removed_addr)) = self.peers.remove(&node) else {
            return Err(NetworkError::PeerNotFound(node));
        };
        self.peer_addr_index.remove(&removed_addr);
        refresh_peer_ip_hint_for_ip(&self.peers, &self.peer_ip_hint_index, removed_addr.ip());
        clear_runtime_sync_pull_target(self.chain_id, self.node, node);
        self.runtime_peer_registered.remove(&node);
        let _ = unregister_network_runtime_peer(self.chain_id, node.0);
        Ok(())
    }

    pub fn local_addr(&self) -> Result<SocketAddr, NetworkError> {
        self.socket
            .local_addr()
            .map_err(|e| NetworkError::Io(e.to_string()))
    }

    fn send_internal(&self, to: NodeId, msg: &ProtocolMessage) -> Result<(), NetworkError> {
        let peer = *self.peers.get(&to).ok_or(NetworkError::PeerNotFound(to))?;
        let encoded = protocol_encode(msg).map_err(|e| NetworkError::Encode(e.to_string()))?;
        let sent = match self.socket.send_to(&encoded, peer) {
            Ok(sent) => sent,
            Err(e) => {
                if should_mark_peer_disconnected(&e) {
                    clear_runtime_sync_pull_target(self.chain_id, self.node, to);
                    self.runtime_peer_registered.remove(&to);
                    let _ = unregister_network_runtime_peer(self.chain_id, to.0);
                }
                return Err(NetworkError::Io(e.to_string()));
            }
        };
        if sent != encoded.len() {
            return Err(NetworkError::Io(format!(
                "partial udp send: sent={sent} expected={}",
                encoded.len()
            )));
        }
        if self.runtime_peer_registered.insert(to, ()).is_none() {
            let _ = register_network_runtime_peer(self.chain_id, to.0);
        }
        maybe_update_runtime_sync_local_progress_from_send(self.chain_id, self.node, msg);
        Ok(())
    }
}

#[cfg(test)]
fn maybe_update_runtime_sync_from_protocol_message(
    chain_id: u64,
    msg: &ProtocolMessage,
    msg_peer_id: Option<u64>,
    source_peer_id_hint: Option<u64>,
) {
    let sync_ctx = runtime_sync_pull_message_context(msg);
    maybe_update_runtime_sync_from_protocol_message_with_context(
        chain_id,
        msg,
        msg_peer_id,
        source_peer_id_hint,
        &sync_ctx,
    );
}

fn maybe_update_runtime_sync_from_protocol_message_with_context(
    chain_id: u64,
    msg: &ProtocolMessage,
    msg_peer_id: Option<u64>,
    source_peer_id_hint: Option<u64>,
    sync_ctx: &RuntimeSyncPullMessageContext,
) {
    let fallback_peer_id = msg_peer_id.or(source_peer_id_hint);

    match msg {
        ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList { from, peers }) => {
            let _ = register_network_runtime_peer(chain_id, from.0);
            for peer in peers {
                if peer.0 != from.0 {
                    let _ = register_network_runtime_peer(chain_id, peer.0);
                }
            }
        }
        ProtocolMessage::Pacemaker(PacemakerMessage::ViewSync { from, height, .. }) => {
            let _ = observe_network_runtime_peer_head_with_local_head_max(
                chain_id, from.0, *height, None,
            );
        }
        ProtocolMessage::Pacemaker(PacemakerMessage::NewView {
            from,
            height,
            high_qc_height,
            ..
        }) => {
            let _ = observe_network_runtime_peer_head_with_local_head_max(
                chain_id,
                from.0,
                (*height).max(*high_qc_height),
                None,
            );
        }
        ProtocolMessage::DistributedOcccGossip(gossip_msg) => {
            if sync_ctx.is_sync_pull {
                if let Some(height) = sync_ctx.header_height {
                    // Treat downloader state headers as local progress.
                    // This keeps runtime current_block advancing from real ingress
                    // messages instead of waiting for external snapshot injection.
                    let _ = observe_network_runtime_peer_head_with_local_head_max(
                        chain_id,
                        gossip_msg.from as u64,
                        height,
                        Some(height),
                    );
                } else {
                    let _ = register_network_runtime_peer(chain_id, gossip_msg.from as u64);
                }
            } else {
                let _ = register_network_runtime_peer(chain_id, gossip_msg.from as u64);
            }
        }
        ProtocolMessage::EvmNative(native_msg) => match native_msg {
            EvmNativeMessage::DiscoveryPing { from, .. }
            | EvmNativeMessage::DiscoveryPong { from, .. }
            | EvmNativeMessage::DiscoveryFindNode { from, .. }
            | EvmNativeMessage::DiscoveryNeighbors { from, .. } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_eth_native_discovery(chain_id);
            }
            EvmNativeMessage::RlpxAuth { from, .. } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_eth_native_rlpx_auth(chain_id);
            }
            EvmNativeMessage::RlpxAuthAck { from, .. } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_eth_native_rlpx_auth_ack(chain_id);
            }
            EvmNativeMessage::Hello {
                from,
                eth_versions,
                snap_versions,
                ..
            } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_eth_native_hello(chain_id);
                let _ = upsert_network_runtime_eth_peer_session(
                    chain_id,
                    from.0,
                    eth_versions.as_slice(),
                    snap_versions.as_slice(),
                    None,
                );
            }
            EvmNativeMessage::Status {
                from, head_height, ..
            } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_eth_native_status(chain_id);
                mark_network_runtime_eth_peer_session_ready_v1(
                    chain_id,
                    from.0,
                    Some(*head_height),
                );
                let _ = observe_network_runtime_peer_head(chain_id, from.0, *head_height);
                observe_network_runtime_eth_peer_head(chain_id, from.0, *head_height);
            }
            EvmNativeMessage::NewBlockHashes { from, blocks } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                if let Some((_, height)) = blocks.iter().max_by_key(|(_, height)| *height) {
                    let _ = observe_network_runtime_peer_head(chain_id, from.0, *height);
                    observe_network_runtime_eth_peer_head(chain_id, from.0, *height);
                }
            }
            EvmNativeMessage::Transactions {
                from,
                tx_hash,
                payload,
                ..
            } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_network_runtime_native_pending_tx_ingress_with_payload_v1(
                    chain_id,
                    from.0,
                    *tx_hash,
                    Some(payload.as_slice()),
                );
            }
            EvmNativeMessage::GetBlockHeaders { from, .. } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
            }
            EvmNativeMessage::BlockHeaders { from, headers } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_eth_native_headers_response(chain_id);
                for header in headers {
                    ingest_runtime_native_header_from_evm_wire(chain_id, from.0, header);
                }
            }
            EvmNativeMessage::GetBlockBodies { from, .. } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
            }
            EvmNativeMessage::BlockBodies { from, bodies } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_eth_native_bodies_response(chain_id);
                for body in bodies {
                    ingest_runtime_native_body_from_evm_wire(chain_id, from.0, body);
                }
            }
            EvmNativeMessage::SnapGetAccountRange { from, .. } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
            }
            EvmNativeMessage::SnapAccountRange { from, .. } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_eth_native_snap_response(chain_id);
            }
        },
        ProtocolMessage::Finality(FinalityMessage::Vote { id, from, .. }) => {
            let _ =
                observe_network_runtime_peer_head_with_local_head_max(chain_id, from.0, id.0, None);
        }
        ProtocolMessage::Finality(FinalityMessage::CheckpointPropose { id, from, .. })
        | ProtocolMessage::Finality(FinalityMessage::Cert { id, from, .. }) => {
            let _ =
                observe_network_runtime_peer_head_with_local_head_max(chain_id, from.0, id.0, None);
        }
        _ => {
            if let Some(peer_id) = fallback_peer_id {
                let _ = register_network_runtime_peer(chain_id, peer_id);
            }
        }
    }
}

fn maybe_update_runtime_sync_local_progress_from_send(
    chain_id: u64,
    local_node: NodeId,
    msg: &ProtocolMessage,
) {
    match msg {
        ProtocolMessage::Pacemaker(PacemakerMessage::ViewSync { from, height, .. }) => {
            if *from == local_node {
                let _ = observe_network_runtime_local_head_max(chain_id, *height);
            }
        }
        ProtocolMessage::Pacemaker(PacemakerMessage::NewView {
            from,
            height,
            high_qc_height,
            ..
        }) => {
            if *from == local_node {
                let _ = observe_network_runtime_local_head_max(
                    chain_id,
                    (*height).max(*high_qc_height),
                );
            }
        }
        ProtocolMessage::DistributedOcccGossip(gossip_msg) => {
            maybe_track_runtime_sync_pull_request_outbound(chain_id, local_node, msg);
            if gossip_msg.from == local_node.0 as u32
                && is_runtime_sync_pull_msg_type(&gossip_msg.msg_type)
            {
                if let Ok(header) = decode_block_header_wire_v1(&gossip_msg.payload) {
                    let _ = observe_network_runtime_local_head_max(chain_id, header.height);
                }
            }
        }
        ProtocolMessage::EvmNative(native_msg) => match native_msg {
            EvmNativeMessage::DiscoveryPing { from, .. }
            | EvmNativeMessage::DiscoveryPong { from, .. }
            | EvmNativeMessage::DiscoveryFindNode { from, .. }
            | EvmNativeMessage::DiscoveryNeighbors { from, .. } => {
                if *from == local_node {
                    observe_eth_native_discovery(chain_id);
                }
            }
            EvmNativeMessage::RlpxAuth { from, .. } => {
                if *from == local_node {
                    observe_eth_native_rlpx_auth(chain_id);
                }
            }
            EvmNativeMessage::RlpxAuthAck { from, .. } => {
                if *from == local_node {
                    observe_eth_native_rlpx_auth_ack(chain_id);
                }
            }
            EvmNativeMessage::Hello { from, .. } => {
                if *from == local_node {
                    observe_eth_native_hello(chain_id);
                }
            }
            EvmNativeMessage::Status { from, .. } => {
                if *from == local_node {
                    observe_eth_native_status(chain_id);
                }
            }
            EvmNativeMessage::GetBlockHeaders { from, .. } => {
                if *from == local_node {
                    observe_eth_native_headers_pull(chain_id);
                }
            }
            EvmNativeMessage::BlockHeaders { from, .. } => {
                if *from == local_node {
                    observe_eth_native_headers_response(chain_id);
                }
            }
            EvmNativeMessage::GetBlockBodies { from, .. } => {
                if *from == local_node {
                    observe_eth_native_bodies_pull(chain_id);
                }
            }
            EvmNativeMessage::BlockBodies { from, .. } => {
                if *from == local_node {
                    observe_eth_native_bodies_response(chain_id);
                }
            }
            EvmNativeMessage::SnapGetAccountRange { from, .. } => {
                if *from == local_node {
                    observe_eth_native_snap_pull(chain_id);
                }
            }
            EvmNativeMessage::SnapAccountRange { from, .. } => {
                if *from == local_node {
                    observe_eth_native_snap_response(chain_id);
                }
            }
            EvmNativeMessage::NewBlockHashes { .. } => {}
            EvmNativeMessage::Transactions { from, tx_hash, .. } => {
                if *from == local_node {
                    observe_network_runtime_native_pending_tx_propagated_v1(chain_id, *tx_hash);
                }
            }
        },
        ProtocolMessage::Finality(FinalityMessage::Vote { id, from, .. }) => {
            if *from == local_node {
                let _ = observe_network_runtime_local_head_max(chain_id, id.0);
            }
        }
        ProtocolMessage::Finality(FinalityMessage::CheckpointPropose { id, from, .. })
        | ProtocolMessage::Finality(FinalityMessage::Cert { id, from, .. }) => {
            if *from == local_node {
                let _ = observe_network_runtime_local_head_max(chain_id, id.0);
            }
        }
        _ => {}
    }
}

fn is_runtime_sync_pull_msg_type(msg_type: &DistributedOcccMessageType) -> bool {
    matches!(
        msg_type,
        DistributedOcccMessageType::StateSync | DistributedOcccMessageType::ShardState
    )
}

fn decode_runtime_sync_pull_request(payload: &[u8]) -> Option<RuntimeSyncPullRequest> {
    if payload.len() < RUNTIME_SYNC_PULL_REQUEST_LEN {
        return None;
    }
    if payload.get(0..4)? != RUNTIME_SYNC_PULL_REQUEST_MAGIC {
        return None;
    }
    let phase = decode_runtime_sync_phase_byte(*payload.get(4)?);
    let chain_id = u64::from_le_bytes(payload.get(5..13)?.try_into().ok()?);
    let from_block = u64::from_le_bytes(payload.get(13..21)?.try_into().ok()?);
    let to_block = u64::from_le_bytes(payload.get(21..29)?.try_into().ok()?);
    Some(RuntimeSyncPullRequest {
        phase,
        chain_id,
        from_block,
        to_block,
    })
}

fn runtime_sync_pull_message_context(msg: &ProtocolMessage) -> RuntimeSyncPullMessageContext {
    let ProtocolMessage::DistributedOcccGossip(gossip_msg) = msg else {
        return RuntimeSyncPullMessageContext::default();
    };
    if !is_runtime_sync_pull_msg_type(&gossip_msg.msg_type) {
        return RuntimeSyncPullMessageContext::default();
    }
    let request = decode_runtime_sync_pull_request(&gossip_msg.payload);
    let header_height = if request.is_none() {
        decode_block_header_wire_v1(&gossip_msg.payload)
            .ok()
            .map(|header| header.height)
    } else {
        None
    };
    RuntimeSyncPullMessageContext {
        is_sync_pull: true,
        request,
        header_height,
    }
}

fn decode_runtime_sync_phase_byte(raw: u8) -> NetworkRuntimeNativeSyncPhaseV1 {
    match raw {
        0 => NetworkRuntimeNativeSyncPhaseV1::Idle,
        1 => NetworkRuntimeNativeSyncPhaseV1::Discovery,
        2 => NetworkRuntimeNativeSyncPhaseV1::Headers,
        3 => NetworkRuntimeNativeSyncPhaseV1::Bodies,
        4 => NetworkRuntimeNativeSyncPhaseV1::State,
        5 => NetworkRuntimeNativeSyncPhaseV1::Finalize,
        _ => NetworkRuntimeNativeSyncPhaseV1::Headers,
    }
}

fn runtime_sync_pull_msg_type_for_phase(
    phase: NetworkRuntimeNativeSyncPhaseV1,
) -> DistributedOcccMessageType {
    match phase {
        NetworkRuntimeNativeSyncPhaseV1::Headers => DistributedOcccMessageType::StateSync,
        _ => DistributedOcccMessageType::ShardState,
    }
}

fn runtime_sync_pull_response_batch_max_by_phase(phase: NetworkRuntimeNativeSyncPhaseV1) -> u64 {
    match phase {
        NetworkRuntimeNativeSyncPhaseV1::Headers => RUNTIME_SYNC_PULL_RESPONSE_BATCH_MAX,
        NetworkRuntimeNativeSyncPhaseV1::Bodies => 64,
        NetworkRuntimeNativeSyncPhaseV1::State => 32,
        NetworkRuntimeNativeSyncPhaseV1::Finalize => 16,
        NetworkRuntimeNativeSyncPhaseV1::Discovery | NetworkRuntimeNativeSyncPhaseV1::Idle => 16,
    }
}

fn encode_runtime_sync_phase_byte(phase: NetworkRuntimeNativeSyncPhaseV1) -> u8 {
    match phase {
        NetworkRuntimeNativeSyncPhaseV1::Idle => 0,
        NetworkRuntimeNativeSyncPhaseV1::Discovery => 1,
        NetworkRuntimeNativeSyncPhaseV1::Headers => 2,
        NetworkRuntimeNativeSyncPhaseV1::Bodies => 3,
        NetworkRuntimeNativeSyncPhaseV1::State => 4,
        NetworkRuntimeNativeSyncPhaseV1::Finalize => 5,
    }
}

fn encode_runtime_sync_pull_request_payload(
    chain_id: u64,
    phase: NetworkRuntimeNativeSyncPhaseV1,
    from_block: u64,
    to_block: u64,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(RUNTIME_SYNC_PULL_REQUEST_LEN);
    payload.extend_from_slice(&RUNTIME_SYNC_PULL_REQUEST_MAGIC);
    payload.push(encode_runtime_sync_phase_byte(phase));
    payload.extend_from_slice(&chain_id.to_le_bytes());
    payload.extend_from_slice(&from_block.to_le_bytes());
    payload.extend_from_slice(&to_block.to_le_bytes());
    payload
}

fn runtime_sync_pull_response_cap_to(request: &RuntimeSyncPullRequest) -> u64 {
    let phase_batch = runtime_sync_pull_response_batch_max_by_phase(request.phase).max(1);
    request.to_block.min(
        request
            .from_block
            .saturating_add(phase_batch.saturating_sub(1)),
    )
}

fn runtime_sync_pull_prefetch_margin_by_phase(phase: NetworkRuntimeNativeSyncPhaseV1) -> u64 {
    match phase {
        NetworkRuntimeNativeSyncPhaseV1::Headers => RUNTIME_SYNC_PULL_PREFETCH_MARGIN_HEADERS,
        NetworkRuntimeNativeSyncPhaseV1::Bodies => RUNTIME_SYNC_PULL_PREFETCH_MARGIN_BODIES,
        NetworkRuntimeNativeSyncPhaseV1::State => RUNTIME_SYNC_PULL_PREFETCH_MARGIN_STATE,
        NetworkRuntimeNativeSyncPhaseV1::Finalize => RUNTIME_SYNC_PULL_PREFETCH_MARGIN_FINALIZE,
        NetworkRuntimeNativeSyncPhaseV1::Discovery | NetworkRuntimeNativeSyncPhaseV1::Idle => 0,
    }
}

fn runtime_sync_pull_followup_trigger_height(
    request: &RuntimeSyncPullRequest,
    capped_target_to: u64,
) -> u64 {
    let window_span = capped_target_to.saturating_sub(request.from_block);
    let phase_margin = runtime_sync_pull_prefetch_margin_by_phase(request.phase);
    let bounded_margin = phase_margin.min(window_span / 2);
    capped_target_to.saturating_sub(bounded_margin)
}

fn maybe_track_runtime_sync_pull_request_outbound(
    chain_id: u64,
    local_node: NodeId,
    msg: &ProtocolMessage,
) {
    let ProtocolMessage::DistributedOcccGossip(gossip_msg) = msg else {
        return;
    };
    if !is_runtime_sync_pull_msg_type(&gossip_msg.msg_type) {
        return;
    }
    if gossip_msg.from != local_node.0 as u32 {
        return;
    }
    let Some(request) = decode_runtime_sync_pull_request(&gossip_msg.payload) else {
        return;
    };
    if request.chain_id != chain_id {
        return;
    }
    let capped_target_to = runtime_sync_pull_response_cap_to(&request);
    let followup_trigger = runtime_sync_pull_followup_trigger_height(&request, capped_target_to);
    set_runtime_sync_pull_target_with_trigger(
        chain_id,
        local_node,
        NodeId(gossip_msg.to as u64),
        capped_target_to,
        followup_trigger,
    );
}

fn encode_runtime_sync_block_header_payload(response_height: u64) -> Vec<u8> {
    let header = BlockHeaderWireV1 {
        height: response_height,
        epoch_id: 0,
        parent_hash: [0u8; 32],
        state_root: [0u8; 32],
        governance_chain_audit_root: [0u8; 32],
        tx_count: 0,
        batch_count: 0,
        consensus_binding: ConsensusPluginBindingV1 {
            plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
            adapter_hash: [0u8; 32],
        },
    };
    encode_block_header_wire_v1(&header)
}

fn compute_runtime_sync_pull_response_range(
    chain_id: u64,
    phase: NetworkRuntimeNativeSyncPhaseV1,
    from_block: u64,
    to_block: u64,
) -> Option<(u64, u64)> {
    let local_head = get_network_runtime_sync_status(chain_id)
        .map(|s| s.current_block)
        .unwrap_or(0);
    if local_head < from_block {
        return None;
    }
    let response_to = local_head.min(to_block);
    let phase_batch = runtime_sync_pull_response_batch_max_by_phase(phase).max(1);
    let capped_to = response_to.min(from_block.saturating_add(phase_batch.saturating_sub(1)));
    if capped_to < from_block {
        return None;
    }
    Some((from_block, capped_to))
}

fn maybe_plan_runtime_sync_pull_responses_with_context(
    chain_id: u64,
    local_node: NodeId,
    msg: &ProtocolMessage,
    sync_ctx: &RuntimeSyncPullMessageContext,
) -> Option<RuntimeSyncPullResponsePlan> {
    let ProtocolMessage::DistributedOcccGossip(gossip_msg) = msg else {
        return None;
    };
    if !sync_ctx.is_sync_pull {
        return None;
    }
    if gossip_msg.to != local_node.0 as u32 {
        return None;
    }
    let request = sync_ctx.request?;
    if request.chain_id != chain_id {
        return None;
    }
    // Pull request provides remote desired sync edge; ingest as remote progress hint.
    let _ = observe_network_runtime_peer_head(chain_id, gossip_msg.from as u64, request.to_block);

    let (response_from, response_to) = compute_runtime_sync_pull_response_range(
        chain_id,
        request.phase,
        request.from_block,
        request.to_block,
    )?;
    Some(RuntimeSyncPullResponsePlan {
        to: NodeId(gossip_msg.from as u64),
        to_wire: gossip_msg.from,
        msg_type: runtime_sync_pull_msg_type_for_phase(request.phase),
        response_from,
        response_to,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    })
}

fn now_unix_millis_u128() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn evm_native_block_header_wire_from_runtime_snapshot(
    snapshot: &crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1,
) -> EvmNativeBlockHeaderWireV1 {
    EvmNativeBlockHeaderWireV1 {
        number: snapshot.number,
        hash: snapshot.hash,
        parent_hash: snapshot.parent_hash,
        state_root: snapshot.state_root,
        transactions_root: snapshot.transactions_root,
        receipts_root: snapshot.receipts_root,
        ommers_hash: snapshot.ommers_hash,
        logs_bloom: snapshot.logs_bloom.clone(),
        gas_limit: snapshot.gas_limit,
        gas_used: snapshot.gas_used,
        timestamp: snapshot.timestamp,
        base_fee_per_gas: snapshot.base_fee_per_gas,
        withdrawals_root: snapshot.withdrawals_root,
        blob_gas_used: snapshot.blob_gas_used,
        excess_blob_gas: snapshot.excess_blob_gas,
        block_access_list_hash: snapshot.block_access_list_hash,
    }
}

fn evm_native_block_body_wire_from_runtime_snapshot(
    snapshot: &crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1,
) -> EvmNativeBlockBodyWireV1 {
    EvmNativeBlockBodyWireV1 {
        number: snapshot.number,
        block_hash: snapshot.block_hash,
        tx_hashes: snapshot.tx_hashes.clone(),
        raw_tx_rlps: snapshot.raw_tx_rlps.clone(),
        ommer_hashes: snapshot.ommer_hashes.clone(),
        withdrawal_rlp_items: snapshot.withdrawal_rlp_items.clone(),
        withdrawal_count: snapshot.withdrawal_count,
        body_available: snapshot.body_available,
        txs_materialized: snapshot.txs_materialized,
    }
}

fn runtime_native_header_snapshot_from_evm_wire(
    chain_id: u64,
    source_peer_id: u64,
    observed_unix_ms: u128,
    header: &EvmNativeBlockHeaderWireV1,
) -> crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
    crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
        chain_id,
        number: header.number,
        hash: header.hash,
        parent_hash: header.parent_hash,
        state_root: header.state_root,
        transactions_root: header.transactions_root,
        receipts_root: header.receipts_root,
        ommers_hash: header.ommers_hash,
        logs_bloom: header.logs_bloom.clone(),
        gas_limit: header.gas_limit,
        gas_used: header.gas_used,
        timestamp: header.timestamp,
        base_fee_per_gas: header.base_fee_per_gas,
        withdrawals_root: header.withdrawals_root,
        blob_gas_used: header.blob_gas_used,
        excess_blob_gas: header.excess_blob_gas,
        block_access_list_hash: header.block_access_list_hash,
        source_peer_id: Some(source_peer_id),
        observed_unix_ms,
    }
}

fn runtime_native_body_snapshot_from_evm_wire(
    chain_id: u64,
    observed_unix_ms: u128,
    body: &EvmNativeBlockBodyWireV1,
) -> crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1 {
    crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1 {
        chain_id,
        number: body.number,
        block_hash: body.block_hash,
        tx_hashes: body.tx_hashes.clone(),
        raw_tx_rlps: body.raw_tx_rlps.clone(),
        ommer_hashes: body.ommer_hashes.clone(),
        withdrawal_rlp_items: body.withdrawal_rlp_items.clone(),
        withdrawal_count: body.withdrawal_count,
        body_available: body.body_available,
        txs_materialized: body.txs_materialized,
        observed_unix_ms,
    }
}

fn runtime_native_head_snapshot_from_evm_header(
    chain_id: u64,
    source_peer_id: u64,
    peer_count: u64,
    observed_unix_ms: u128,
    header: &EvmNativeBlockHeaderWireV1,
    body_available: bool,
) -> crate::runtime_status::NetworkRuntimeNativeHeadSnapshotV1 {
    crate::runtime_status::NetworkRuntimeNativeHeadSnapshotV1 {
        chain_id,
        phase: NetworkRuntimeNativeSyncPhaseV1::Headers,
        peer_count: peer_count.max(1),
        block_number: header.number,
        block_hash: header.hash,
        parent_block_hash: header.parent_hash,
        state_root: header.state_root,
        canonical: false,
        safe: false,
        finalized: false,
        reorg_depth_hint: None,
        body_available,
        source_peer_id: Some(source_peer_id),
        observed_unix_ms,
    }
}

fn ingest_runtime_native_header_from_evm_wire(
    chain_id: u64,
    source_peer_id: u64,
    header: &EvmNativeBlockHeaderWireV1,
) {
    let observed_unix_ms = now_unix_millis_u128();
    let body_available = get_network_runtime_native_body_snapshot_v1(chain_id)
        .map(|body| body.number == header.number && body.block_hash == header.hash)
        .unwrap_or(false);
    let snapshot = runtime_native_header_snapshot_from_evm_wire(
        chain_id,
        source_peer_id,
        observed_unix_ms,
        header,
    );
    set_network_runtime_native_header_snapshot_v1(chain_id, snapshot);
    let peer_count = get_network_runtime_sync_status(chain_id)
        .map(|status| status.peer_count)
        .unwrap_or(0);
    let head_snapshot = runtime_native_head_snapshot_from_evm_header(
        chain_id,
        source_peer_id,
        peer_count,
        observed_unix_ms,
        header,
        body_available,
    );
    set_network_runtime_native_head_snapshot_v1(chain_id, head_snapshot);
    observe_network_runtime_eth_peer_head(chain_id, source_peer_id, header.number);
    observe_network_runtime_eth_peer_header_success_v1(chain_id, source_peer_id, header.number);
}

fn ingest_runtime_native_body_from_evm_wire(
    chain_id: u64,
    source_peer_id: u64,
    body: &EvmNativeBlockBodyWireV1,
) {
    let observed_unix_ms = now_unix_millis_u128();
    let snapshot = runtime_native_body_snapshot_from_evm_wire(chain_id, observed_unix_ms, body);
    set_network_runtime_native_body_snapshot_v1(chain_id, snapshot);
    if let Some(mut head_snapshot) = get_network_runtime_native_head_snapshot_v1(chain_id) {
        if head_snapshot.block_number == body.number && head_snapshot.block_hash == body.block_hash
        {
            head_snapshot.body_available = body.body_available;
            head_snapshot.source_peer_id = Some(source_peer_id);
            head_snapshot.observed_unix_ms = observed_unix_ms;
            set_network_runtime_native_head_snapshot_v1(chain_id, head_snapshot);
        }
    }
    observe_network_runtime_eth_peer_body_success_v1(chain_id, source_peer_id, body.number);
}

fn maybe_build_evm_native_sync_response(
    chain_id: u64,
    local_node: NodeId,
    msg: &ProtocolMessage,
) -> Option<(NodeId, ProtocolMessage)> {
    let ProtocolMessage::EvmNative(native_msg) = msg else {
        return None;
    };
    match native_msg {
        EvmNativeMessage::DiscoveryPing {
            from,
            chain_id: ping_chain_id,
            ..
        } => {
            if *from == local_node || *ping_chain_id != chain_id {
                return None;
            }
            Some((
                *from,
                ProtocolMessage::EvmNative(EvmNativeMessage::DiscoveryPong {
                    from: local_node,
                    chain_id,
                }),
            ))
        }
        EvmNativeMessage::RlpxAuth {
            from,
            chain_id: auth_chain_id,
            network_id,
            auth_tag,
        } => {
            if *from == local_node || *auth_chain_id != chain_id {
                return None;
            }
            let mut ack_tag = *auth_tag;
            ack_tag.reverse();
            Some((
                *from,
                ProtocolMessage::EvmNative(EvmNativeMessage::RlpxAuthAck {
                    from: local_node,
                    chain_id,
                    network_id: *network_id,
                    ack_tag,
                }),
            ))
        }
        EvmNativeMessage::Hello {
            from,
            chain_id: hello_chain_id,
            ..
        } => {
            if *from == local_node || *hello_chain_id != chain_id {
                return None;
            }
            Some((
                *from,
                build_eth_fullnode_native_status_message_v1(local_node, chain_id),
            ))
        }
        EvmNativeMessage::Status {
            from,
            chain_id: status_chain_id,
            ..
        } => {
            if *from == local_node || *status_chain_id != chain_id {
                return None;
            }
            build_eth_fullnode_native_sync_request_v1(local_node, chain_id)
                .map(|request| (*from, request))
        }
        EvmNativeMessage::GetBlockHeaders {
            from,
            start_height,
            max,
            skip,
            reverse,
        } => {
            if *from == local_node {
                return None;
            }
            let head = get_network_runtime_native_head_snapshot_v1(chain_id)
                .map(|snapshot| snapshot.block_number)
                .or_else(|| {
                    get_network_runtime_sync_status(chain_id)
                        .map(|s| s.current_block.max(s.highest_block))
                })
                .unwrap_or(0);
            let max_count = (*max).clamp(1, 256) as usize;
            let step = skip.saturating_add(1);
            let mut heights = Vec::with_capacity(max_count);
            let mut cursor = *start_height;
            for _ in 0..max_count {
                if *reverse {
                    heights.push(cursor);
                    if cursor < step {
                        break;
                    }
                    cursor = cursor.saturating_sub(step);
                } else {
                    if head > 0 && cursor > head {
                        break;
                    }
                    heights.push(cursor);
                    cursor = cursor.saturating_add(step);
                }
            }
            let headers = get_network_runtime_native_header_snapshot_v1(chain_id)
                .into_iter()
                .filter(|snapshot| heights.contains(&snapshot.number))
                .map(|snapshot| evm_native_block_header_wire_from_runtime_snapshot(&snapshot))
                .collect();
            Some((
                *from,
                ProtocolMessage::EvmNative(EvmNativeMessage::BlockHeaders {
                    from: local_node,
                    headers,
                }),
            ))
        }
        EvmNativeMessage::BlockHeaders { from, headers } => {
            if *from == local_node {
                return None;
            }
            let hashes = headers.iter().map(|header| header.hash).collect::<Vec<_>>();
            build_eth_fullnode_native_bodies_request_v1(local_node, hashes.as_slice())
                .map(|request| (*from, request))
        }
        EvmNativeMessage::GetBlockBodies { from, hashes } => {
            if *from == local_node {
                return None;
            }
            let bodies = get_network_runtime_native_body_snapshot_v1(chain_id)
                .into_iter()
                .filter(|snapshot| hashes.contains(&snapshot.block_hash))
                .map(|snapshot| evm_native_block_body_wire_from_runtime_snapshot(&snapshot))
                .collect();
            Some((
                *from,
                ProtocolMessage::EvmNative(EvmNativeMessage::BlockBodies {
                    from: local_node,
                    bodies,
                }),
            ))
        }
        EvmNativeMessage::SnapGetAccountRange { from, limit, .. } => {
            if *from == local_node {
                return None;
            }
            let account_count = (*limit).min(2048);
            let proof_node_count = account_count.saturating_div(8).max(1);
            Some((
                *from,
                ProtocolMessage::EvmNative(EvmNativeMessage::SnapAccountRange {
                    from: local_node,
                    account_count,
                    proof_node_count,
                }),
            ))
        }
        _ => None,
    }
}
fn emit_runtime_sync_pull_responses(
    local_node: NodeId,
    plan: &RuntimeSyncPullResponsePlan,
    mut send_one: impl FnMut(NodeId, &ProtocolMessage) -> bool,
    mut send_one_fallback: impl FnMut(&ProtocolMessage),
) {
    for (offset, height) in (plan.response_from..=plan.response_to).enumerate() {
        let response_payload = encode_runtime_sync_block_header_payload(height);
        let seq = plan.timestamp.saturating_add(offset as u64);
        let response = ProtocolMessage::DistributedOcccGossip(
            novovm_protocol::protocol_catalog::distributed_occc::gossip::GossipMessage {
                from: local_node.0 as u32,
                to: plan.to_wire,
                msg_type: plan.msg_type.clone(),
                payload: response_payload,
                timestamp: plan.timestamp,
                seq,
            },
        );
        if !send_one(plan.to, &response) {
            send_one_fallback(&response);
        }
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn maybe_build_runtime_sync_pull_responses(
    chain_id: u64,
    local_node: NodeId,
    msg: &ProtocolMessage,
) -> Option<(NodeId, Vec<ProtocolMessage>)> {
    let sync_ctx = runtime_sync_pull_message_context(msg);
    let plan =
        maybe_plan_runtime_sync_pull_responses_with_context(chain_id, local_node, msg, &sync_ctx)?;
    let response_count = plan
        .response_to
        .saturating_sub(plan.response_from)
        .saturating_add(1);
    let mut responses = Vec::with_capacity(response_count as usize);
    for (offset, height) in (plan.response_from..=plan.response_to).enumerate() {
        let response_payload = encode_runtime_sync_block_header_payload(height);
        let seq = plan.timestamp.saturating_add(offset as u64);
        responses.push(ProtocolMessage::DistributedOcccGossip(
            novovm_protocol::protocol_catalog::distributed_occc::gossip::GossipMessage {
                from: local_node.0 as u32,
                to: plan.to_wire,
                msg_type: plan.msg_type.clone(),
                payload: response_payload,
                timestamp: plan.timestamp,
                seq,
            },
        ));
    }
    Some((plan.to, responses))
}

#[cfg(test)]
fn maybe_build_runtime_sync_pull_followup_request(
    chain_id: u64,
    local_node: NodeId,
    msg: &ProtocolMessage,
) -> Option<(NodeId, ProtocolMessage)> {
    let sync_ctx = runtime_sync_pull_message_context(msg);
    maybe_build_runtime_sync_pull_followup_requests_with_context(
        chain_id, local_node, msg, &sync_ctx,
    )
    .into_iter()
    .next()
}

fn maybe_build_runtime_sync_pull_followup_requests_with_context(
    chain_id: u64,
    local_node: NodeId,
    msg: &ProtocolMessage,
    sync_ctx: &RuntimeSyncPullMessageContext,
) -> Vec<(NodeId, ProtocolMessage)> {
    let ProtocolMessage::DistributedOcccGossip(gossip_msg) = msg else {
        return Vec::new();
    };
    if !sync_ctx.is_sync_pull {
        return Vec::new();
    }
    if gossip_msg.to != local_node.0 as u32 {
        return Vec::new();
    }
    // Incoming NSP1 is already a pull request, not a downloaded sync result.
    if sync_ctx.request.is_some() {
        return Vec::new();
    }
    // Only continue pull loop when response payload is a valid sync header.
    let Some(response_height) = sync_ctx.header_height else {
        return Vec::new();
    };
    let sender_target = NodeId(gossip_msg.from as u64);
    let Some(window) = plan_network_runtime_sync_pull_window(chain_id) else {
        return Vec::new();
    };
    if window.from_block > window.to_block {
        return Vec::new();
    }

    let payload = encode_runtime_sync_pull_request_payload(
        chain_id,
        window.phase,
        window.from_block,
        window.to_block,
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let mut out = Vec::new();
    for (idx, target) in runtime_sync_pull_followup_targets(chain_id, sender_target)
        .into_iter()
        .enumerate()
    {
        // Keep consuming current window replies until reaching requested upper bound.
        if should_wait_runtime_sync_pull_target_window(
            chain_id,
            local_node,
            target,
            response_height,
        ) {
            continue;
        }
        let Ok(to_wire) = u32::try_from(target.0) else {
            continue;
        };
        let request = ProtocolMessage::DistributedOcccGossip(
            novovm_protocol::protocol_catalog::distributed_occc::gossip::GossipMessage {
                from: local_node.0 as u32,
                to: to_wire,
                msg_type: runtime_sync_pull_msg_type_for_phase(window.phase),
                payload: payload.clone(),
                timestamp: now,
                seq: now.saturating_add(idx as u64),
            },
        );
        out.push((target, request));
    }
    out
}

fn runtime_peer_id_from_protocol_message(msg: &ProtocolMessage) -> Option<u64> {
    match msg {
        ProtocolMessage::Gossip(ProtocolGossipMessage::Heartbeat { from, .. })
        | ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList { from, .. })
        | ProtocolMessage::Pacemaker(PacemakerMessage::ViewSync { from, .. })
        | ProtocolMessage::Pacemaker(PacemakerMessage::NewView { from, .. })
        | ProtocolMessage::Finality(FinalityMessage::Vote { from, .. })
        | ProtocolMessage::Finality(FinalityMessage::CheckpointPropose { from, .. })
        | ProtocolMessage::Finality(FinalityMessage::Cert { from, .. }) => Some(from.0),
        ProtocolMessage::EvmNative(
            EvmNativeMessage::DiscoveryPing { from, .. }
            | EvmNativeMessage::DiscoveryPong { from, .. }
            | EvmNativeMessage::DiscoveryFindNode { from, .. }
            | EvmNativeMessage::DiscoveryNeighbors { from, .. }
            | EvmNativeMessage::RlpxAuth { from, .. }
            | EvmNativeMessage::RlpxAuthAck { from, .. }
            | EvmNativeMessage::Hello { from, .. }
            | EvmNativeMessage::Status { from, .. }
            | EvmNativeMessage::NewBlockHashes { from, .. }
            | EvmNativeMessage::Transactions { from, .. }
            | EvmNativeMessage::GetBlockHeaders { from, .. }
            | EvmNativeMessage::BlockHeaders { from, .. }
            | EvmNativeMessage::GetBlockBodies { from, .. }
            | EvmNativeMessage::BlockBodies { from, .. }
            | EvmNativeMessage::SnapGetAccountRange { from, .. }
            | EvmNativeMessage::SnapAccountRange { from, .. },
        ) => Some(from.0),
        ProtocolMessage::TwoPc(TwoPcMessage::Propose { tx }) => Some(tx.from.0),
        ProtocolMessage::DistributedOcccGossip(gossip_msg) => Some(gossip_msg.from as u64),
        _ => None,
    }
}

fn refresh_peer_ip_hint_for_ip(
    peers: &DashMap<NodeId, SocketAddr>,
    peer_ip_hint_index: &DashMap<IpAddr, u64>,
    ip: IpAddr,
) {
    let mut found: Option<u64> = None;
    for entry in peers.iter() {
        if entry.value().ip() != ip {
            continue;
        }
        let peer_id = entry.key().0;
        if found.is_some() {
            peer_ip_hint_index.insert(ip, PEER_IP_HINT_AMBIGUOUS);
            return;
        }
        found = Some(peer_id);
    }
    if let Some(peer_id) = found {
        peer_ip_hint_index.insert(ip, peer_id);
    } else {
        peer_ip_hint_index.remove(&ip);
    }
}

fn maybe_learn_peer_addr(
    peers: &DashMap<NodeId, SocketAddr>,
    peer_addr_index: &DashMap<SocketAddr, NodeId>,
    peer_ip_hint_index: &DashMap<IpAddr, u64>,
    local_node: NodeId,
    src: SocketAddr,
    msg_peer_id: Option<u64>,
) {
    let Some(peer_id) = msg_peer_id else {
        return;
    };
    if peer_id == local_node.0 {
        return;
    }
    let peer_node = NodeId(peer_id);
    let should_update = peers
        .get(&peer_node)
        .map(|existing| {
            let existing_addr = *existing;
            if existing_addr.ip() != src.ip() {
                return false;
            }
            existing_addr != src
        })
        .unwrap_or(true);
    if should_update {
        if let Some(old_addr) = peers.insert(peer_node, src) {
            peer_addr_index.remove(&old_addr);
            if old_addr.ip() != src.ip() {
                refresh_peer_ip_hint_for_ip(peers, peer_ip_hint_index, old_addr.ip());
            }
        }
        peer_addr_index.insert(src, peer_node);
        refresh_peer_ip_hint_for_ip(peers, peer_ip_hint_index, src.ip());
    }
}

fn infer_peer_id_from_src_addr(
    peers: &DashMap<NodeId, SocketAddr>,
    src: SocketAddr,
) -> Option<u64> {
    let mut same_ip_peer: Option<u64> = None;
    for entry in peers.iter() {
        let addr = *entry.value();
        if addr == src {
            return Some(entry.key().0);
        }
        if addr.ip() == src.ip() {
            if same_ip_peer.is_some() {
                return None;
            }
            same_ip_peer = Some(entry.key().0);
        }
    }
    same_ip_peer
}

fn infer_peer_id_from_src_addr_with_index(
    peers: &DashMap<NodeId, SocketAddr>,
    peer_addr_index: &DashMap<SocketAddr, NodeId>,
    peer_ip_hint_index: &DashMap<IpAddr, u64>,
    src: SocketAddr,
) -> Option<u64> {
    if let Some(peer) = peer_addr_index.get(&src) {
        return Some(peer.value().0);
    }
    if let Some(peer_hint) = peer_ip_hint_index.get(&src.ip()) {
        let peer_id = *peer_hint;
        if peer_id != PEER_IP_HINT_AMBIGUOUS {
            return Some(peer_id);
        }
        return None;
    }
    infer_peer_id_from_src_addr(peers, src)
}

fn should_mark_peer_disconnected(io_err: &std::io::Error) -> bool {
    matches!(
        io_err.kind(),
        std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::BrokenPipe
    ) || matches!(
        io_err.raw_os_error(),
        Some(10051 | 10054 | 10060 | 10061 | 111 | 113)
    )
}

impl Transport for UdpTransport {
    fn send(&self, to: NodeId, msg: ProtocolMessage) -> Result<(), NetworkError> {
        self.send_internal(to, &msg)
    }

    fn try_recv(&self, me: NodeId) -> Result<Option<ProtocolMessage>, NetworkError> {
        if me != self.node {
            return Err(NetworkError::LocalNodeMismatch {
                expected: self.node,
                got: me,
            });
        }

        let mut recv_buf = {
            let mut shared = self
                .recv_buf
                .lock()
                .map_err(|_| NetworkError::Io("udp recv buffer lock poisoned".to_string()))?;
            std::mem::take(&mut *shared)
        };
        if recv_buf.is_empty() {
            recv_buf.resize(1024, 0);
        }
        let recv_outcome = self.socket.recv_from(recv_buf.as_mut_slice());
        let decode_outcome = match recv_outcome {
            Ok((n, src)) => protocol_decode(&recv_buf[..n])
                .map(|decoded| Some((decoded, src)))
                .map_err(|e| NetworkError::Decode(e.to_string())),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut
                    || e.raw_os_error() == Some(10054) =>
            {
                Ok(None)
            }
            Err(e) => Err(NetworkError::Io(e.to_string())),
        };
        let _ = self.recv_buf.lock().map(|mut shared| {
            *shared = recv_buf;
        });
        let (decoded, src) = match decode_outcome {
            Ok(Some(v)) => v,
            Ok(None) => return Ok(None),
            Err(e) => return Err(e),
        };
        let msg_peer_id = runtime_peer_id_from_protocol_message(&decoded);
        let source_peer_id_hint = if msg_peer_id.is_none() {
            infer_peer_id_from_src_addr_with_index(
                &self.peers,
                &self.peer_addr_index,
                &self.peer_ip_hint_index,
                src,
            )
        } else {
            None
        };
        let sync_ctx = runtime_sync_pull_message_context(&decoded);
        observe_local_observed_peer_from_transport_evidence(
            &self.peers,
            &self.peer_addr_index,
            msg_peer_id,
            src,
        );
        maybe_learn_peer_addr(
            &self.peers,
            &self.peer_addr_index,
            &self.peer_ip_hint_index,
            self.node,
            src,
            msg_peer_id,
        );
        maybe_update_runtime_sync_from_protocol_message_with_context(
            self.chain_id,
            &decoded,
            msg_peer_id,
            source_peer_id_hint,
            &sync_ctx,
        );
        if let Some((to, response)) =
            maybe_build_evm_native_sync_response(self.chain_id, self.node, &decoded)
        {
            if self.send_internal(to, &response).is_err() {
                if let Ok(encoded) = protocol_encode(&response) {
                    let _ = self.socket.send_to(&encoded, src);
                }
            }
        }
        if let Some(plan) = maybe_plan_runtime_sync_pull_responses_with_context(
            self.chain_id,
            self.node,
            &decoded,
            &sync_ctx,
        ) {
            emit_runtime_sync_pull_responses(
                self.node,
                &plan,
                |to, response| {
                    // Prefer registry route to keep peer activity updates on send path.
                    self.send_internal(to, response).is_ok()
                },
                |response| {
                    // Fallback to raw src addr for cases where peer registry is stale.
                    if let Ok(encoded) = protocol_encode(response) {
                        let _ = self.socket.send_to(&encoded, src);
                    }
                },
            );
        }
        let fallback_sender = if let ProtocolMessage::DistributedOcccGossip(gossip) = &decoded {
            Some(NodeId(gossip.from as u64))
        } else {
            None
        };
        for (to, followup) in maybe_build_runtime_sync_pull_followup_requests_with_context(
            self.chain_id,
            self.node,
            &decoded,
            &sync_ctx,
        ) {
            if self.send_internal(to, &followup).is_ok() {
                continue;
            }
            if fallback_sender != Some(to) {
                continue;
            }
            if let Ok(encoded) = protocol_encode(&followup) {
                if self.socket.send_to(&encoded, src).is_ok() {
                    // `send` path already tracks outbound pull targets on success.
                    // Fallback path should track only when raw socket send succeeds.
                    maybe_track_runtime_sync_pull_request_outbound(
                        self.chain_id,
                        self.node,
                        &followup,
                    );
                }
            }
        }
        Ok(Some(decoded))
    }
}

impl Transport for TcpTransport {
    fn send(&self, to: NodeId, msg: ProtocolMessage) -> Result<(), NetworkError> {
        self.send_internal(to, &msg)
    }

    fn try_recv(&self, me: NodeId) -> Result<Option<ProtocolMessage>, NetworkError> {
        if me != self.node {
            return Err(NetworkError::LocalNodeMismatch {
                expected: self.node,
                got: me,
            });
        }

        let (mut stream, addr) = match self.listener.accept() {
            Ok(v) => v,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                return Ok(None);
            }
            Err(e) => return Err(NetworkError::Io(e.to_string())),
        };
        stream
            .set_nonblocking(false)
            .map_err(|e| NetworkError::Io(e.to_string()))?;

        let mut len_buf = [0u8; 4];
        stream
            .read_exact(&mut len_buf)
            .map_err(|e| NetworkError::Io(e.to_string()))?;
        let frame_len = u32::from_le_bytes(len_buf) as usize;
        if frame_len == 0 || frame_len > self.max_packet_size {
            return Err(NetworkError::Decode(format!(
                "invalid tcp frame len={frame_len}, max={}",
                self.max_packet_size
            )));
        }
        let mut recv_frame_buf = {
            let mut shared = self
                .recv_frame_buf
                .lock()
                .map_err(|_| NetworkError::Io("tcp recv buffer lock poisoned".to_string()))?;
            std::mem::take(&mut *shared)
        };
        if recv_frame_buf.len() < frame_len {
            recv_frame_buf.resize(frame_len, 0);
        }
        stream
            .read_exact(&mut recv_frame_buf[..frame_len])
            .map_err(|e| NetworkError::Io(e.to_string()))?;
        let decode_outcome = protocol_decode(&recv_frame_buf[..frame_len])
            .map_err(|e| NetworkError::Decode(e.to_string()));
        let _ = self.recv_frame_buf.lock().map(|mut shared| {
            *shared = recv_frame_buf;
        });
        let decoded = decode_outcome?;
        let msg_peer_id = runtime_peer_id_from_protocol_message(&decoded);
        let source_peer_id_hint = if msg_peer_id.is_none() {
            infer_peer_id_from_src_addr_with_index(
                &self.peers,
                &self.peer_addr_index,
                &self.peer_ip_hint_index,
                addr,
            )
        } else {
            None
        };
        let sync_ctx = runtime_sync_pull_message_context(&decoded);
        observe_local_observed_peer_from_transport_evidence(
            &self.peers,
            &self.peer_addr_index,
            msg_peer_id,
            addr,
        );
        maybe_learn_peer_addr(
            &self.peers,
            &self.peer_addr_index,
            &self.peer_ip_hint_index,
            self.node,
            addr,
            msg_peer_id,
        );
        maybe_update_runtime_sync_from_protocol_message_with_context(
            self.chain_id,
            &decoded,
            msg_peer_id,
            source_peer_id_hint,
            &sync_ctx,
        );
        if let Some((to, response)) =
            maybe_build_evm_native_sync_response(self.chain_id, self.node, &decoded)
        {
            if self.send_internal(to, &response).is_err() {
                if let Ok(encoded) = protocol_encode(&response) {
                    let _ = write_tcp_frame(&mut stream, &encoded);
                }
            }
        }
        if let Some(plan) = maybe_plan_runtime_sync_pull_responses_with_context(
            self.chain_id,
            self.node,
            &decoded,
            &sync_ctx,
        ) {
            emit_runtime_sync_pull_responses(
                self.node,
                &plan,
                |to, response| self.send_internal(to, response).is_ok(),
                |response| {
                    if let Ok(encoded) = protocol_encode(response) {
                        let _ = write_tcp_frame(&mut stream, &encoded);
                    }
                },
            );
        }
        let fallback_sender = if let ProtocolMessage::DistributedOcccGossip(gossip) = &decoded {
            Some(NodeId(gossip.from as u64))
        } else {
            None
        };
        for (to, followup) in maybe_build_runtime_sync_pull_followup_requests_with_context(
            self.chain_id,
            self.node,
            &decoded,
            &sync_ctx,
        ) {
            if self.send_internal(to, &followup).is_ok() {
                continue;
            }
            if fallback_sender != Some(to) {
                continue;
            }
            if let Ok(encoded) = protocol_encode(&followup) {
                if write_tcp_frame(&mut stream, &encoded).is_ok() {
                    // `send` path already tracks outbound pull targets on success.
                    // Fallback path should track only when raw tcp write succeeds.
                    maybe_track_runtime_sync_pull_request_outbound(
                        self.chain_id,
                        self.node,
                        &followup,
                    );
                }
            }
        }
        Ok(Some(decoded))
    }
}

fn write_tcp_frame(stream: &mut TcpStream, payload: &[u8]) -> Result<(), std::io::Error> {
    let len_u32 = u32::try_from(payload.len())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "tcp frame too large"))?;
    stream.write_all(&len_u32.to_le_bytes())?;
    stream.write_all(payload)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        clear_network_runtime_native_snapshots_for_chain_v1,
        current_eth_native_parity_progress_for_chain, default_eth_fullnode_budget_hooks_v1,
        derive_eth_fullnode_head_view_with_native_preference_v1,
        derive_eth_fullnode_sync_view_with_native_preference_v1,
        get_network_runtime_native_block_access_list_payload_v1,
        get_network_runtime_native_body_snapshot_v1, get_network_runtime_native_head_snapshot_v1,
        get_network_runtime_native_header_snapshot_v1, get_network_runtime_native_pending_tx_v1,
        get_network_runtime_native_receipt_snapshot_v1,
        get_network_runtime_native_snap_code_snapshot_v1, get_network_runtime_native_sync_status,
        get_network_runtime_sync_status,
        observe_network_runtime_native_pending_tx_local_ingress_with_payload_v1,
        parse_enode_endpoint, set_network_runtime_native_block_access_list_payload_v1,
        set_network_runtime_native_body_snapshot_v1, set_network_runtime_native_head_snapshot_v1,
        set_network_runtime_native_header_snapshot_v1, set_network_runtime_sync_status,
        snapshot_eth_fullnode_native_head_block_object_v1, snapshot_eth_native_sync_evidence,
        snapshot_network_runtime_eth_peer_sessions,
        snapshot_network_runtime_eth_peer_sessions_for_peers_v1,
        snapshot_network_runtime_native_pending_tx_broadcast_candidates_v1,
        snapshot_network_runtime_native_pending_tx_summary_v1,
        NetworkRuntimeNativePendingTxLifecycleStageV1, NetworkRuntimeNativePendingTxOriginV1,
        NetworkRuntimeSyncStatus,
    };
    use novovm_protocol::{
        encode_block_header_wire_v1,
        protocol_catalog::distributed_occc::gossip::{
            GossipMessage as DistributedGossipMessage, MessageType as DistributedMessageType,
        },
        BlockHeaderWireV1, CheckpointId, ConsensusPluginBindingV1, EvmNativeBlockBodyWireV1,
        EvmNativeBlockHeaderWireV1, FinalityMessage, GossipMessage, PacemakerMessage, ShardId,
        CONSENSUS_PLUGIN_CLASS_CODE,
    };
    use std::collections::{HashMap, HashSet};
    use std::net::TcpListener;
    use std::sync::{Mutex, OnceLock};
    use std::thread;
    use std::time::{Duration, Instant};

    fn eth_rlpx_env_test_lock_v1() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvVarRestoreV1 {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl Drop for EnvVarRestoreV1 {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.as_ref() {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    fn set_test_env_var_v1(key: &'static str, value: &'static str) -> EnvVarRestoreV1 {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        EnvVarRestoreV1 { key, previous }
    }

    const LIVE_MAINNET_BOOTNODES: [&str; 4] = [
        "enode://d860a01f9722d78051619d1e2351aba3f43f943f6f00718d1b9baa4101932a1f5011f16bb2b1bb35db20d6fe28fa0bf09636d26a87d31de9ec6203eeedb1f666@18.138.108.67:30303",
        "enode://22a8232c3abc76a16ae9d6c3b164f98775fe226f0917b0ca871128a74a8e9630b458460865bab457221f1d448dd9791d24c4e5d88786180ac185df813a68d4de@3.209.45.79:30303",
        "enode://2b252ab6a1d0f971d9722cb839a42cb81db019ba44c08754628ab4a823487071b5695317c8ccd085219c3a03af063495b2f1da8d18218da2d6a82981b45e6ffc@65.108.70.101:30303",
        "enode://4aeb4ab6c14b23e2c4cfdce879c04b0748a20d8e9b59e25ded2a08143e265c6c25936e74cbc8e641e3312ca288673d91f2f93f8e277de3cfa444ecdaaf982052@157.90.35.166:30303",
    ];

    #[test]
    fn native_runtime_snapshot_bounds_peer_detail_payload_v1() {
        let chain_id = 9_926_201_u64;
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let candidate_peers = (1_u64..=100).map(NodeId).collect::<Vec<_>>();
        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.runtime_block_snapshot_limit = 8;
        let plan = EthFullnodeNativePeerWorkerPlanV1 {
            chain_id,
            local_node: NodeId(1),
            candidate_peers: candidate_peers.clone(),
            candidate_peer_endpoints: Vec::new(),
            lifecycle_summary: EthPeerLifecycleSummaryV1::default(),
            selection_quality_summary: EthPeerSelectionQualitySummaryV1::default(),
            selection_scores: Vec::new(),
            bootstrap_peers: candidate_peers.iter().copied().take(10).collect(),
            sync_peers: vec![NodeId(90)],
            recv_budget: 1,
            budget_hooks: budget,
        };
        let report = EthFullnodeNativeRealDriveReportV1 {
            peer_failures: vec![EthFullnodeNativePeerFailureV1 {
                peer_id: 88,
                endpoint: Some("127.0.0.1:30303".to_string()),
                phase: EthFullnodeNativePeerDrivePhaseV1::Bootstrap,
                class: EthFullnodeNativePeerFailureClassV1::Io,
                lifecycle_class: Some(crate::EthPeerFailureClassV1::ConnectFailure),
                reason_code: None,
                reason_name: Some("connect_failed".to_string()),
                error: "connect_failed".to_string(),
            }],
            ..EthFullnodeNativeRealDriveReportV1::default()
        };

        let snapshot = build_eth_fullnode_native_worker_runtime_snapshot_v1(&plan, &report);

        assert_eq!(snapshot.candidate_peer_ids.len(), 100);
        assert_eq!(
            snapshot.peer_sessions.len(),
            ETH_FULLNODE_NATIVE_RUNTIME_PEER_DETAIL_LIMIT_V1
        );
        assert!(
            snapshot.peer_selection_scores.len()
                <= ETH_FULLNODE_NATIVE_RUNTIME_PEER_DETAIL_LIMIT_V1 * 2
        );
        assert!(snapshot
            .peer_sessions
            .iter()
            .any(|session| session.peer_id == 88));
        assert_eq!(snapshot.selection_quality_summary.candidate_peer_count, 100);
    }

    fn dummy_rlpx_live_session_pair(
        chain_id: u64,
    ) -> (
        EthFullnodeNativeRlpxLivePeerSessionV1,
        std::net::TcpStream,
        EthRlpxFrameSessionV1,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind dummy listener");
        let addr = listener.local_addr().expect("dummy listener addr");
        let stream = TcpStream::connect(addr).expect("connect dummy stream");
        let (accepted, _) = listener.accept().expect("accept dummy stream");
        let frame_session = EthRlpxFrameSessionV1::from_secrets(
            [0x44; 32],
            [0x55; 32],
            b"supervm:a->b:init",
            b"supervm:b->a:init",
        )
        .expect("dummy frame session");
        let peer_frame_session = EthRlpxFrameSessionV1::from_secrets(
            [0x44; 32],
            [0x55; 32],
            b"supervm:b->a:init",
            b"supervm:a->b:init",
        )
        .expect("dummy peer frame session");
        let session = EthFullnodeNativeRlpxLivePeerSessionV1 {
            endpoint: PluginPeerEndpoint {
                endpoint: "enode://dummy@127.0.0.1:0".to_string(),
                node_hint: 1,
                addr_hint: addr.to_string(),
            },
            stream,
            frame_session,
            _negotiated_eth_version: 71,
            _negotiated_snap_version: Some(1),
            remote_status: EthRlpxStatusV1 {
                protocol_version: 71,
                network_id: chain_id,
                genesis_hash: [0u8; 32],
                fork_id: crate::EthForkIdV1 {
                    hash: [0u8; 4],
                    next: 0,
                },
                earliest_block: 0,
                latest_block: 120,
                latest_block_hash: [0x55; 32],
            },
            last_sync_request_unix_ms: 0,
            last_headers_request_id: None,
            pending_headers_request: None,
            last_bodies_request_id: None,
            last_receipts_request_id: None,
            last_snap_account_range_request_id: None,
            last_snap_storage_ranges_request_id: None,
            last_snap_byte_codes_request_id: None,
            last_snap_trie_nodes_request_id: None,
            last_snap_state_root: None,
            last_snap_account_origin: None,
            last_snap_account_limit: None,
            pending_snap_next_account_origin: None,
            pending_snap_storage_accounts: Vec::new(),
            pending_snap_storage_origin: Vec::new(),
            pending_snap_storage_limit: Vec::new(),
            pending_snap_storage_deferred_accounts: Vec::new(),
            pending_snap_code_hashes: Vec::new(),
            pending_snap_trie_node_pathsets: Vec::new(),
            pending_snap_trie_node_hashes: Vec::new(),
            pending_snap_trie_node_retry_count: 0,
            last_block_access_lists_request_id: None,
            queued_block_access_lists: Vec::new(),
            pending_block_access_lists: Vec::new(),
            last_pooled_transactions_request_id: None,
            last_tx_broadcast_unix_ms: 0,
            pending_body_headers: Vec::new(),
            pending_body_request_offset: 0,
            pending_receipt_request_offset: 0,
            pending_pooled_transaction_hashes: Vec::new(),
        };
        (session, accepted, peer_frame_session)
    }

    fn dummy_rlpx_live_session(chain_id: u64) -> EthFullnodeNativeRlpxLivePeerSessionV1 {
        let (session, _accepted, _peer_frame_session) = dummy_rlpx_live_session_pair(chain_id);
        session
    }

    #[test]
    fn real_rlpx_worker_reconciles_stale_runtime_ready_without_live_session_v1() {
        let chain_id = 9_926_800_u64;
        let local = NodeId(9_926_800_001);
        let remote = NodeId(9_926_800_002);
        let endpoint = PluginPeerEndpoint {
            endpoint: "enode://stale-runtime-ready@127.0.0.1:30303".to_string(),
            node_hint: remote.0,
            addr_hint: "127.0.0.1:30303".to_string(),
        };
        let _ = upsert_network_runtime_eth_peer_session(
            chain_id,
            remote.0,
            &[69, 70],
            &[1],
            Some(25_282_008),
        )
        .expect("runtime ready session");
        eth_fullnode_native_rlpx_sessions_v1()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&(chain_id, remote.0));

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });
        assert_eq!(worker.plan().sync_peers, vec![remote]);

        reconcile_eth_fullnode_native_rlpx_runtime_sessions_with_live_sessions_v1(
            chain_id,
            worker.config().peers.as_slice(),
        );

        let snapshot = snapshot_network_runtime_eth_peer_sessions_for_peers_v1(chain_id, &[remote])
            .into_iter()
            .next()
            .expect("peer snapshot");
        assert!(!snapshot.session_ready);
        assert_eq!(
            snapshot.lifecycle_stage,
            crate::EthPeerLifecycleStageV1::Discovered
        );
        assert!(worker.plan().sync_peers.is_empty());
    }

    #[test]
    fn rlpx_block_headers_validation_rejects_non_contiguous_batch_v1() {
        let empty_root = crate::eth_rlpx_empty_trie_root_v1();
        let empty_ommers_hash = crate::eth_rlpx_empty_ommers_hash_v1();
        let make_header =
            |number: u64, hash_byte: u8, parent_hash: [u8; 32]| crate::EthRlpxBlockHeaderRecordV1 {
                number,
                hash: [hash_byte; 32],
                parent_hash,
                state_root: [0x20; 32],
                transactions_root: empty_root,
                receipts_root: empty_root,
                ommers_hash: empty_ommers_hash,
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(0),
                timestamp: Some(1_234_567 + number),
                base_fee_per_gas: Some(15),
                withdrawals_root: Some(empty_root),
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                raw_rlp: None,
            };

        let header_a = make_header(120, 0xa1, [0x90; 32]);
        let header_b = make_header(121, 0xa2, header_a.hash);
        let linked = [&header_a, &header_b];
        let request = EthRlpxGetBlockHeadersRequestV1 {
            request_id: 7,
            start_height: 120,
            origin_hash: None,
            max_headers: 2,
            skip: 0,
            reverse: false,
        };
        validate_eth_fullnode_native_rlpx_block_headers_response_matches_request_v1(
            &request,
            linked.as_slice(),
        )
        .expect("linked headers must pass");

        let origin_mismatch = EthRlpxGetBlockHeadersRequestV1 {
            start_height: 119,
            ..request
        };
        let err = validate_eth_fullnode_native_rlpx_block_headers_response_matches_request_v1(
            &origin_mismatch,
            linked.as_slice(),
        )
        .expect_err("origin mismatch must reject");
        assert!(
            err.contains("rlpx_block_headers_origin_number_mismatch"),
            "unexpected error: {err}"
        );

        let origin_hash_request = EthRlpxGetBlockHeadersRequestV1 {
            request_id: 8,
            start_height: 120,
            origin_hash: Some(header_a.hash),
            max_headers: 1,
            skip: 0,
            reverse: false,
        };
        validate_eth_fullnode_native_rlpx_block_headers_response_matches_request_v1(
            &origin_hash_request,
            &[&header_a],
        )
        .expect("by-hash origin with matching announced number must pass");
        let origin_hash_number_mismatch = EthRlpxGetBlockHeadersRequestV1 {
            start_height: 121,
            ..origin_hash_request
        };
        let err = validate_eth_fullnode_native_rlpx_block_headers_response_matches_request_v1(
            &origin_hash_number_mismatch,
            &[&header_a],
        )
        .expect_err("by-hash origin with wrong announced number must reject");
        assert!(
            err.contains("rlpx_block_headers_origin_number_mismatch"),
            "unexpected error: {err}"
        );

        let header_gap = make_header(123, 0xa3, header_b.hash);
        let gap = [&header_b, &header_gap];
        let gap_request = EthRlpxGetBlockHeadersRequestV1 {
            start_height: 121,
            max_headers: 2,
            ..request
        };
        let err = validate_eth_fullnode_native_rlpx_block_headers_response_matches_request_v1(
            &gap_request,
            gap.as_slice(),
        )
        .expect_err("number gap must reject");
        assert!(
            err.contains("rlpx_block_headers_number_gap"),
            "unexpected error: {err}"
        );

        let header_wrong_parent = make_header(122, 0xa4, [0xff; 32]);
        let wrong_parent = [&header_b, &header_wrong_parent];
        let err = validate_eth_fullnode_native_rlpx_block_headers_response_matches_request_v1(
            &gap_request,
            wrong_parent.as_slice(),
        )
        .expect_err("wrong parent hash must reject");
        assert!(
            err.contains("rlpx_block_headers_parent_mismatch"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rlpx_block_headers_ingest_rejects_unrequested_response_v1() {
        let chain_id = 9_928_u64;
        let mut session = dummy_rlpx_live_session(chain_id);
        let header = crate::EthRlpxBlockHeaderRecordV1 {
            number: 120,
            hash: [0xa1; 32],
            parent_hash: [0x90; 32],
            state_root: [0x20; 32],
            transactions_root: crate::eth_rlpx_empty_trie_root_v1(),
            receipts_root: crate::eth_rlpx_empty_trie_root_v1(),
            ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
            logs_bloom: vec![0u8; 256],
            gas_limit: Some(30_000_000),
            gas_used: Some(0),
            timestamp: Some(1_234_687),
            base_fee_per_gas: Some(15),
            withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
            blob_gas_used: None,
            excess_blob_gas: None,
            block_access_list_hash: None,
            raw_rlp: None,
        };
        let response = EthRlpxBlockHeadersResponseV1 {
            request_id: 99,
            headers: vec![header],
        };
        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.sync_pull_bodies_batch = 1;
        let mut report = EthFullnodeNativeRlpxPeerTickReportV1::default();

        let err = ingest_real_rlpx_block_headers_v1(
            chain_id,
            77,
            &mut session,
            &response,
            &budget,
            &mut report,
        )
        .expect_err("unsolicited headers must reject");
        assert!(
            err.to_string()
                .contains("rlpx_block_headers_unexpected_response"),
            "unexpected error: {err}"
        );
        assert!(
            get_network_runtime_native_header_snapshot_v1(chain_id).is_none(),
            "unsolicited header must not materialize"
        );
    }

    #[test]
    fn rlpx_header_batch_import_requests_current_body_only_while_chasing_v1() {
        let chain_id = 9_928_001_u64;
        let peer_id = 77_u64;
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 120,
                current_block: 120,
                highest_block: 256,
            },
        );

        let (mut session, _accepted, _peer_frame_session) = dummy_rlpx_live_session_pair(chain_id);
        session.pending_headers_request = Some(EthRlpxGetBlockHeadersRequestV1 {
            request_id: 11,
            start_height: 121,
            origin_hash: None,
            max_headers: 16,
            skip: 0,
            reverse: false,
        });

        let mut parent_hash = [0x40; 32];
        let mut headers = Vec::new();
        for offset in 0..16u8 {
            let number = 121 + u64::from(offset);
            let hash = [0x50 + offset; 32];
            headers.push(crate::EthRlpxBlockHeaderRecordV1 {
                number,
                hash,
                parent_hash,
                state_root: [0x60 + offset; 32],
                transactions_root: [0x70 + offset; 32],
                receipts_root: [0x80 + offset; 32],
                ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(21_000),
                timestamp: Some(1_900_000_000 + u64::from(offset)),
                base_fee_per_gas: Some(7),
                withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                raw_rlp: None,
            });
            parent_hash = hash;
        }
        let response = EthRlpxBlockHeadersResponseV1 {
            request_id: 11,
            headers,
        };
        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.sync_pull_bodies_batch = 16;
        let mut report = EthFullnodeNativeRlpxPeerTickReportV1::default();

        ingest_real_rlpx_block_headers_v1(
            chain_id,
            peer_id,
            &mut session,
            &response,
            &budget,
            &mut report,
        )
        .expect("header batch ingest");

        assert_eq!(report.header_updates, 16);
        assert_eq!(session.pending_body_headers.len(), 1);
        assert_eq!(session.pending_body_headers[0].number, 136);
        assert_eq!(session.pending_body_headers[0].hash, [0x5f; 32]);
        assert!(
            session.last_bodies_request_id.is_some(),
            "header ingest must dispatch a follow-up body request"
        );

        let retained = snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 32);
        assert!(
            retained.iter().any(|block| block.number == 121),
            "first imported header must remain in canonical history"
        );
        assert!(
            retained.iter().any(|block| block.number == 136),
            "latest imported header must remain in canonical history"
        );
        let head = get_network_runtime_native_header_snapshot_v1(chain_id).expect("head header");
        assert_eq!(head.number, 136);
        assert_eq!(head.hash, [0x5f; 32]);
    }

    #[test]
    fn rlpx_large_header_batch_import_requests_current_body_only_at_highest_v1() {
        let chain_id = 9_928_002_u64;
        let peer_id = 78_u64;
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 200,
                current_block: 200,
                highest_block: 261,
            },
        );

        let (mut session, _accepted, _peer_frame_session) = dummy_rlpx_live_session_pair(chain_id);
        session.pending_headers_request = Some(EthRlpxGetBlockHeadersRequestV1 {
            request_id: 12,
            start_height: 201,
            origin_hash: None,
            max_headers: 61,
            skip: 0,
            reverse: false,
        });

        let mut parent_hash = [0x90; 32];
        let mut headers = Vec::new();
        for offset in 0..61u8 {
            let number = 201 + u64::from(offset);
            let hash = [0x91 + offset; 32];
            headers.push(crate::EthRlpxBlockHeaderRecordV1 {
                number,
                hash,
                parent_hash,
                state_root: [0x30 + offset; 32],
                transactions_root: [0x40 + offset; 32],
                receipts_root: [0x50 + offset; 32],
                ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(21_000),
                timestamp: Some(1_900_001_000 + u64::from(offset)),
                base_fee_per_gas: Some(7),
                withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                raw_rlp: None,
            });
            parent_hash = hash;
        }
        let response = EthRlpxBlockHeadersResponseV1 {
            request_id: 12,
            headers,
        };
        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.sync_pull_bodies_batch = 128;
        budget.sync_pull_finalize_batch = 64;
        let mut report = EthFullnodeNativeRlpxPeerTickReportV1::default();

        ingest_real_rlpx_block_headers_v1(
            chain_id,
            peer_id,
            &mut session,
            &response,
            &budget,
            &mut report,
        )
        .expect("header batch ingest");

        assert_eq!(report.header_updates, 61);
        assert_eq!(
            session.pending_body_headers.len(),
            1,
            "large near-head batches must materialize current head first"
        );
        assert_eq!(session.pending_body_headers[0].number, 261);
        assert_eq!(session.pending_body_headers[0].hash, [0xcd; 32]);
        let retained = snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 128);
        assert!(retained.iter().any(|block| block.number == 201));
        assert!(retained.iter().any(|block| block.number == 261));
    }

    #[test]
    fn rlpx_small_header_batch_import_requests_current_body_only_at_highest_v1() {
        let chain_id = 9_928_003_u64;
        let peer_id = 79_u64;
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 300,
                current_block: 300,
                highest_block: 304,
            },
        );

        let (mut session, _accepted, _peer_frame_session) = dummy_rlpx_live_session_pair(chain_id);
        session.pending_headers_request = Some(EthRlpxGetBlockHeadersRequestV1 {
            request_id: 13,
            start_height: 301,
            origin_hash: None,
            max_headers: 4,
            skip: 0,
            reverse: false,
        });

        let mut parent_hash = [0x20; 32];
        let mut headers = Vec::new();
        for offset in 0..4u8 {
            let number = 301 + u64::from(offset);
            let hash = [0x21 + offset; 32];
            headers.push(crate::EthRlpxBlockHeaderRecordV1 {
                number,
                hash,
                parent_hash,
                state_root: [0x31 + offset; 32],
                transactions_root: [0x41 + offset; 32],
                receipts_root: [0x51 + offset; 32],
                ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(21_000),
                timestamp: Some(1_900_002_000 + u64::from(offset)),
                base_fee_per_gas: Some(7),
                withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                raw_rlp: None,
            });
            parent_hash = hash;
        }
        let response = EthRlpxBlockHeadersResponseV1 {
            request_id: 13,
            headers,
        };
        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.sync_pull_bodies_batch = 128;
        budget.sync_pull_finalize_batch = 64;
        let mut report = EthFullnodeNativeRlpxPeerTickReportV1::default();

        ingest_real_rlpx_block_headers_v1(
            chain_id,
            peer_id,
            &mut session,
            &response,
            &budget,
            &mut report,
        )
        .expect("header batch ingest");

        assert_eq!(report.header_updates, 4);
        assert_eq!(session.pending_body_headers.len(), 1);
        assert_eq!(session.pending_body_headers[0].number, 304);
        assert_eq!(session.pending_body_headers[0].hash, [0x24; 32]);
    }

    #[test]
    fn rlpx_response_ingest_rejects_unrequested_response_messages_v1() {
        let chain_id = 9_929_u64;
        let mut session = dummy_rlpx_live_session(chain_id);
        let mut report = EthFullnodeNativeRlpxPeerTickReportV1::default();
        let tx_hash = [0xa1; 32];
        let pooled = EthRlpxPooledTransactionsPayloadV1 {
            request_id: 10,
            tx_rlp_items: vec![vec![0xc0]],
            tx_hashes: vec![tx_hash],
        };
        let err = ingest_real_rlpx_pooled_transactions_v1(chain_id, 77, &mut session, &pooled)
            .expect_err("unsolicited pooled transactions must reject");
        assert!(
            err.to_string()
                .contains("rlpx_pooled_transactions_unexpected_response"),
            "unexpected error: {err}"
        );
        assert!(
            get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).is_none(),
            "unsolicited pooled transaction must not materialize"
        );

        let bodies = EthRlpxBlockBodiesResponseV1 {
            request_id: 11,
            bodies: Vec::new(),
        };
        let err =
            ingest_real_rlpx_block_bodies_v1(chain_id, 77, &mut session, &bodies, &mut report)
                .expect_err("unsolicited block bodies must reject");
        assert!(
            err.to_string()
                .contains("rlpx_block_bodies_unexpected_response"),
            "unexpected error: {err}"
        );

        let receipts = EthRlpxReceiptsResponseV1 {
            request_id: 12,
            last_block_incomplete: false,
            blocks: Vec::new(),
        };
        let err = ingest_real_rlpx_receipts_v1(chain_id, 77, &mut session, &receipts, &mut report)
            .expect_err("unsolicited receipts must reject");
        assert!(
            err.to_string()
                .contains("rlpx_receipts_unexpected_response"),
            "unexpected error: {err}"
        );

        session.pending_body_headers = vec![EthFullnodeNativePendingBodyHeaderV1 {
            number: 120,
            hash: [0xb1; 32],
            parent_hash: [0xb0; 32],
            state_root: [0xb2; 32],
            transactions_root: crate::eth_rlpx_empty_trie_root_v1(),
            receipts_root: crate::eth_rlpx_empty_trie_root_v1(),
            tx_count: Some(0),
            withdrawal_count: Some(0),
        }];
        let bal = EthRlpxBlockAccessListsResponseV1 {
            request_id: 13,
            lists: Vec::new(),
        };
        let err = ingest_real_rlpx_block_access_lists_v1(chain_id, 77, &mut session, &bal)
            .expect_err("unsolicited block access lists must reject");
        assert!(
            err.to_string()
                .contains("rlpx_block_access_lists_unexpected_response"),
            "unexpected error: {err}"
        );
        assert_eq!(
            session.pending_body_headers.len(),
            1,
            "unsolicited BAL response must not clear pending body headers"
        );
    }

    #[test]
    fn rlpx_block_bodies_empty_response_does_not_retry_same_peer_v1() {
        let chain_id = 9_929_001_u64;
        let (mut session, _accepted, _peer_frame_session) = dummy_rlpx_live_session_pair(chain_id);
        session.last_bodies_request_id = Some(77);
        session.pending_body_headers = vec![EthFullnodeNativePendingBodyHeaderV1 {
            number: 8_000,
            hash: [0xa1; 32],
            parent_hash: [0xa0; 32],
            state_root: [0xa2; 32],
            transactions_root: [0xa3; 32],
            receipts_root: [0xa4; 32],
            tx_count: None,
            withdrawal_count: None,
        }];
        let mut report = EthFullnodeNativeRlpxPeerTickReportV1::default();
        let bodies = EthRlpxBlockBodiesResponseV1 {
            request_id: 77,
            bodies: Vec::new(),
        };

        let err =
            ingest_real_rlpx_block_bodies_v1(chain_id, 77, &mut session, &bodies, &mut report)
                .expect_err("empty body response should release this peer");

        assert!(
            err.to_string().contains("rlpx_block_bodies_empty_response"),
            "unexpected error: {err}"
        );
        assert_eq!(report.sync_requests, 0);
        assert!(
            session.last_bodies_request_id.is_none(),
            "same peer must not be immediately retried for an empty body response"
        );
    }

    #[test]
    fn rlpx_snap_response_ingest_rejects_unrequested_response_messages_v1() {
        let chain_id = 9_930_u64;
        let mut session = dummy_rlpx_live_session(chain_id);

        let account_range = EthRlpxAccountRangeResponseV1 {
            request_id: 21,
            accounts: Vec::new(),
            proof: Vec::new(),
        };
        let err =
            ingest_real_rlpx_snap_account_range_v1(chain_id, 77, &mut session, &account_range)
                .expect_err("unsolicited AccountRange must reject");
        assert!(
            err.to_string()
                .contains("snap_account_range_unexpected_response"),
            "unexpected error: {err}"
        );

        let storage_ranges = EthRlpxStorageRangesResponseV1 {
            request_id: 22,
            slots: Vec::new(),
            proof: Vec::new(),
        };
        let err =
            ingest_real_rlpx_snap_storage_ranges_v1(chain_id, 77, &mut session, &storage_ranges)
                .expect_err("unsolicited StorageRanges must reject");
        assert!(
            err.to_string()
                .contains("snap_storage_ranges_unexpected_response"),
            "unexpected error: {err}"
        );

        let code = vec![0x60, 0x00];
        let code_hash = eth_rlpx_code_hash_v1(code.as_slice());
        let byte_codes = EthRlpxByteCodesResponseV1 {
            request_id: 23,
            codes: vec![code],
        };
        let err = ingest_real_rlpx_snap_byte_codes_v1(chain_id, 77, &mut session, &byte_codes)
            .expect_err("unsolicited ByteCodes must reject");
        assert!(
            err.to_string()
                .contains("snap_byte_codes_unexpected_response"),
            "unexpected error: {err}"
        );
        assert!(
            get_network_runtime_native_snap_code_snapshot_v1(chain_id, code_hash).is_none(),
            "unsolicited ByteCodes response must not cache code"
        );

        let trie_nodes = EthRlpxTrieNodesResponseV1 {
            request_id: 24,
            nodes: vec![vec![0xc0]],
        };
        let err = ingest_real_rlpx_snap_trie_nodes_v1(chain_id, 77, &mut session, &trie_nodes)
            .expect_err("unsolicited TrieNodes must reject");
        assert!(
            err.to_string()
                .contains("snap_trie_nodes_unexpected_response"),
            "unexpected error: {err}"
        );

        assert!(
            session.last_snap_account_range_request_id.is_none()
                && session.last_snap_storage_ranges_request_id.is_none()
                && session.last_snap_byte_codes_request_id.is_none()
                && session.last_snap_trie_nodes_request_id.is_none(),
            "unsolicited snap responses must not synthesize pending requests"
        );
    }

    #[test]
    fn rlpx_snap_byte_codes_response_must_match_requested_ordered_subset_v1() {
        let code_a = vec![0x60, 0x01];
        let code_b = vec![0x60, 0x02];
        let code_c = vec![0x60, 0x03];
        let code_x = vec![0x60, 0xff];
        let hash_a = eth_rlpx_code_hash_v1(code_a.as_slice());
        let hash_b = eth_rlpx_code_hash_v1(code_b.as_slice());
        let hash_c = eth_rlpx_code_hash_v1(code_c.as_slice());
        let requested = [hash_a, hash_b, hash_c];

        let matched = validate_eth_fullnode_native_snap_byte_codes_match_request_v1(
            &requested,
            &[code_a.clone(), code_c.clone()],
        )
        .expect("ordered subset with gaps is valid");
        assert_eq!(matched, vec![hash_a, hash_c]);
        assert_eq!(
            eth_fullnode_native_snap_byte_codes_missing_hashes_v1(&requested, &matched),
            vec![hash_b],
            "missing ByteCodes hashes must be re-requested like geth codeTasks"
        );

        let empty_err =
            validate_eth_fullnode_native_snap_byte_codes_match_request_v1(&requested, &[])
                .expect_err("empty ByteCodes response must reject");
        assert!(empty_err.contains("snap_byte_codes_empty_response"));

        let unexpected_err = validate_eth_fullnode_native_snap_byte_codes_match_request_v1(
            &requested,
            std::slice::from_ref(&code_x),
        )
        .expect_err("unrequested bytecode must reject");
        assert!(unexpected_err.contains("snap_byte_codes_unrequested_or_out_of_order_hash"));

        let out_of_order_err = validate_eth_fullnode_native_snap_byte_codes_match_request_v1(
            &requested,
            &[code_c, code_a],
        )
        .expect_err("out-of-order bytecode response must reject");
        assert!(out_of_order_err.contains("snap_byte_codes_unrequested_or_out_of_order_hash"));
    }

    #[test]
    fn rlpx_snap_byte_codes_partial_ingest_retries_missing_hashes_v1() {
        let chain_id = 9_953_u64;
        let code_a = vec![0x60, 0x01];
        let code_b = vec![0x60, 0x02];
        let code_c = vec![0x60, 0x03];
        let hash_a = eth_rlpx_code_hash_v1(code_a.as_slice());
        let hash_b = eth_rlpx_code_hash_v1(code_b.as_slice());
        let hash_c = eth_rlpx_code_hash_v1(code_c.as_slice());
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let (mut session, mut accepted, mut peer_frame_session) =
            dummy_rlpx_live_session_pair(chain_id);
        accepted
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set accepted read timeout");
        session.last_snap_byte_codes_request_id = Some(92);
        session.pending_snap_code_hashes = vec![hash_a, hash_b, hash_c];
        let response = EthRlpxByteCodesResponseV1 {
            request_id: 92,
            codes: vec![code_a.clone(), code_c.clone()],
        };

        ingest_real_rlpx_snap_byte_codes_v1(chain_id, 77, &mut session, &response)
            .expect("partial ByteCodes response must request missing hashes");

        let snap_offset = crate::eth_rlpx_snap_base_offset_v1(71, Some(1)).expect("snap offset");
        let (code, payload) =
            crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut peer_frame_session)
                .expect("read retry get byte codes");
        assert_eq!(code, snap_offset + crate::ETH_RLPX_SNAP_GET_BYTE_CODES_MSG);
        let retry = crate::eth_rlpx_parse_get_byte_codes_payload_v1(payload.as_slice())
            .expect("parse retry get byte codes");
        assert_eq!(retry.hashes, vec![hash_b]);
        assert_eq!(session.pending_snap_code_hashes, vec![hash_b]);
        assert_eq!(
            session.last_snap_byte_codes_request_id,
            Some(retry.request_id)
        );
        assert_eq!(
            crate::get_network_runtime_native_snap_code_snapshot_v1(chain_id, hash_a)
                .expect("code a cached")
                .code,
            code_a
        );
        assert!(
            crate::get_network_runtime_native_snap_code_snapshot_v1(chain_id, hash_b).is_none(),
            "missing bytecode must not be synthesized before retry"
        );
        assert_eq!(
            crate::get_network_runtime_native_snap_code_snapshot_v1(chain_id, hash_c)
                .expect("code c cached")
                .code,
            code_c
        );
    }

    #[test]
    fn rlpx_snap_storage_ranges_missing_accounts_preserves_unreturned_accounts_v1() {
        let requested = [[0x01; 32], [0x02; 32], [0x03; 32]];
        let partial = EthRlpxStorageRangesResponseV1 {
            request_id: 41,
            slots: vec![vec![crate::EthRlpxSnapStorageDataV1 {
                hash: [0x10; 32],
                body: vec![0x80],
            }]],
            proof: Vec::new(),
        };

        assert_eq!(
            eth_fullnode_native_snap_storage_ranges_completed_slotsets_v1(&partial),
            1
        );
        assert_eq!(
            eth_fullnode_native_snap_storage_ranges_missing_accounts_v1(&requested, &partial),
            vec![requested[1], requested[2]],
            "accounts not covered by StorageRanges must be re-requested like geth stateTasks"
        );

        let empty_first_account_with_proof = EthRlpxStorageRangesResponseV1 {
            request_id: 42,
            slots: Vec::new(),
            proof: vec![vec![0xc0]],
        };
        assert_eq!(
            eth_fullnode_native_snap_storage_ranges_completed_slotsets_v1(
                &empty_first_account_with_proof
            ),
            1
        );
        assert_eq!(
            eth_fullnode_native_snap_storage_ranges_missing_accounts_v1(
                &requested,
                &empty_first_account_with_proof
            ),
            vec![requested[1], requested[2]],
            "empty slots plus proof completes only the first requested account"
        );

        let full = EthRlpxStorageRangesResponseV1 {
            request_id: 43,
            slots: vec![
                Vec::<crate::EthRlpxSnapStorageDataV1>::new(),
                Vec::<crate::EthRlpxSnapStorageDataV1>::new(),
                Vec::<crate::EthRlpxSnapStorageDataV1>::new(),
            ],
            proof: Vec::new(),
        };
        assert!(
            eth_fullnode_native_snap_storage_ranges_missing_accounts_v1(&requested, &full)
                .is_empty(),
            "a slotset for every requested account leaves no missing account"
        );
    }

    #[test]
    fn rlpx_snap_storage_ranges_partial_ingest_retries_missing_accounts_v1() {
        let chain_id = 9_951_u64;
        let state_root = [0xa5; 32];
        let account_a = [0x31; 32];
        let account_b = [0x32; 32];
        let storage_slot = crate::EthRlpxSnapStorageDataV1 {
            hash: [0x41; 32],
            body: vec![0x80],
        };
        let storage_root =
            crate::eth_rlpx_snap_storage_root_from_range_v1(std::slice::from_ref(&storage_slot))
                .expect("storage root");
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        for account_hash in [account_a, account_b] {
            set_network_runtime_native_snap_account_snapshot_v1(
                chain_id,
                NetworkRuntimeNativeSnapAccountSnapshotV1 {
                    chain_id,
                    state_root,
                    account_hash,
                    body_rlp: Vec::new(),
                    proof_nodes: Vec::new(),
                    storage_root: Some(storage_root),
                    code_hash: None,
                    has_storage: true,
                    has_code: false,
                    source_peer_id: Some(1),
                    observed_unix_ms: 1,
                },
            );
        }

        let (mut session, mut accepted, mut peer_frame_session) =
            dummy_rlpx_live_session_pair(chain_id);
        accepted
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set accepted read timeout");
        session.last_snap_storage_ranges_request_id = Some(88);
        session.last_snap_state_root = Some(state_root);
        session.pending_snap_storage_accounts = vec![account_a, account_b];
        let response = EthRlpxStorageRangesResponseV1 {
            request_id: 88,
            slots: vec![vec![storage_slot.clone()]],
            proof: Vec::new(),
        };

        ingest_real_rlpx_snap_storage_ranges_v1(chain_id, 77, &mut session, &response)
            .expect("partial StorageRanges must schedule retry for missing account");

        let snap_offset = crate::eth_rlpx_snap_base_offset_v1(71, Some(1)).expect("snap offset");
        let (code, payload) =
            crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut peer_frame_session)
                .expect("read retry get storage ranges");
        assert_eq!(
            code,
            snap_offset + crate::ETH_RLPX_SNAP_GET_STORAGE_RANGES_MSG
        );
        let retry = crate::eth_rlpx_parse_get_storage_ranges_payload_v1(payload.as_slice())
            .expect("parse retry get storage ranges");
        assert_eq!(retry.root, state_root);
        assert_eq!(retry.accounts, vec![account_b]);
        assert!(retry.origin.is_empty());
        assert!(retry.limit.is_empty());
        assert_eq!(session.pending_snap_storage_accounts, vec![account_b]);
        assert_eq!(
            session.last_snap_storage_ranges_request_id,
            Some(retry.request_id)
        );
        assert!(
            crate::get_network_runtime_native_snap_account_storage_snapshot_v1(
                chain_id, state_root, account_a
            )
            .is_some(),
            "returned account storage must still cache before retrying missing accounts"
        );
        assert!(
            crate::get_network_runtime_native_snap_account_storage_snapshot_v1(
                chain_id, state_root, account_b
            )
            .is_none(),
            "missing account storage must not be synthesized before retry"
        );
    }

    #[test]
    fn rlpx_snap_storage_ranges_continuation_retries_same_account_before_deferred_v1() {
        let chain_id = 9_952_u64;
        let state_root = [0xa6; 32];
        let account_a = [0x33; 32];
        let account_b = [0x34; 32];
        let left_slot = crate::EthRlpxSnapStorageDataV1 {
            hash: [0x41; 32],
            body: vec![0x80],
        };
        let right_slot = crate::EthRlpxSnapStorageDataV1 {
            hash: [0x42; 32],
            body: vec![0x80],
        };
        let right_proof_node = crate::eth_rlpx_mpt_single_leaf_node_rlp_v1(
            &right_slot.hash,
            right_slot.body.as_slice(),
        );
        let right_storage_root = crate::eth_rlpx_trie_node_hash_v1(right_proof_node.as_slice());
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        for (account_hash, storage_root) in [
            (account_a, right_storage_root),
            (account_b, crate::eth_rlpx_empty_trie_root_v1()),
        ] {
            set_network_runtime_native_snap_account_snapshot_v1(
                chain_id,
                NetworkRuntimeNativeSnapAccountSnapshotV1 {
                    chain_id,
                    state_root,
                    account_hash,
                    body_rlp: Vec::new(),
                    proof_nodes: Vec::new(),
                    storage_root: Some(storage_root),
                    code_hash: None,
                    has_storage: true,
                    has_code: false,
                    source_peer_id: Some(1),
                    observed_unix_ms: 1,
                },
            );
        }

        let (mut session, mut accepted, mut peer_frame_session) =
            dummy_rlpx_live_session_pair(chain_id);
        accepted
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set accepted read timeout");
        session.last_snap_storage_ranges_request_id = Some(91);
        session.last_snap_state_root = Some(state_root);
        session.pending_snap_storage_accounts = vec![account_a, account_b];
        let response = EthRlpxStorageRangesResponseV1 {
            request_id: 91,
            slots: vec![vec![left_slot.clone()]],
            proof: vec![right_proof_node.clone()],
        };

        ingest_real_rlpx_snap_storage_ranges_v1(chain_id, 77, &mut session, &response)
            .expect("chunked StorageRanges must request same account continuation");

        let snap_offset = crate::eth_rlpx_snap_base_offset_v1(71, Some(1)).expect("snap offset");
        let (code, payload) =
            crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut peer_frame_session)
                .expect("read same-account continuation request");
        assert_eq!(
            code,
            snap_offset + crate::ETH_RLPX_SNAP_GET_STORAGE_RANGES_MSG
        );
        let continuation = crate::eth_rlpx_parse_get_storage_ranges_payload_v1(payload.as_slice())
            .expect("parse continuation get storage ranges");
        let expected_next_origin =
            eth_rlpx_account_hash_next_v1(left_slot.hash).expect("next storage slot origin");
        assert_eq!(continuation.root, state_root);
        assert_eq!(continuation.accounts, vec![account_a]);
        assert_eq!(continuation.origin, expected_next_origin.to_vec());
        assert!(continuation.limit.is_empty());
        assert_eq!(session.pending_snap_storage_accounts, vec![account_a]);
        assert_eq!(
            session.pending_snap_storage_origin,
            expected_next_origin.to_vec()
        );
        assert_eq!(
            session.pending_snap_storage_deferred_accounts,
            vec![account_b]
        );

        let final_response = EthRlpxStorageRangesResponseV1 {
            request_id: continuation.request_id,
            slots: vec![vec![right_slot.clone()]],
            proof: vec![right_proof_node],
        };
        ingest_real_rlpx_snap_storage_ranges_v1(chain_id, 77, &mut session, &final_response)
            .expect("completed continuation must release deferred accounts");

        let (code, payload) =
            crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut peer_frame_session)
                .expect("read deferred account storage request");
        assert_eq!(
            code,
            snap_offset + crate::ETH_RLPX_SNAP_GET_STORAGE_RANGES_MSG
        );
        let deferred = crate::eth_rlpx_parse_get_storage_ranges_payload_v1(payload.as_slice())
            .expect("parse deferred get storage ranges");
        assert_eq!(deferred.root, state_root);
        assert_eq!(deferred.accounts, vec![account_b]);
        assert!(deferred.origin.is_empty());
        assert!(deferred.limit.is_empty());
        assert_eq!(session.pending_snap_storage_accounts, vec![account_b]);
        assert!(session.pending_snap_storage_deferred_accounts.is_empty());

        let storage = crate::get_network_runtime_native_snap_account_storage_snapshot_v1(
            chain_id, state_root, account_a,
        )
        .expect("merged account storage snapshot");
        assert_eq!(
            storage
                .slots
                .iter()
                .map(|slot| slot.hash)
                .collect::<Vec<_>>(),
            vec![left_slot.hash, right_slot.hash],
            "continuation responses must merge storage slots instead of overwriting"
        );
    }

    #[test]
    fn rlpx_snap_byte_codes_empty_response_keeps_pending_request_v1() {
        let chain_id = 9_931_u64;
        let mut session = dummy_rlpx_live_session(chain_id);
        let code = vec![0x60, 0x00];
        let code_hash = eth_rlpx_code_hash_v1(code.as_slice());
        session.last_snap_byte_codes_request_id = Some(44);
        session.pending_snap_code_hashes = vec![code_hash];

        let response = EthRlpxByteCodesResponseV1 {
            request_id: 44,
            codes: Vec::new(),
        };
        let err = ingest_real_rlpx_snap_byte_codes_v1(chain_id, 77, &mut session, &response)
            .expect_err("empty ByteCodes must be treated as peer state rejection");
        assert!(
            err.to_string().contains("snap_byte_codes_empty_response"),
            "unexpected error: {err}"
        );
        assert_eq!(session.last_snap_byte_codes_request_id, Some(44));
        assert_eq!(session.pending_snap_code_hashes, vec![code_hash]);
        assert!(
            get_network_runtime_native_snap_code_snapshot_v1(chain_id, code_hash).is_none(),
            "empty ByteCodes response must not cache or synthesize code"
        );
    }

    #[test]
    fn rlpx_snap_trie_nodes_empty_response_keeps_pending_request_v1() {
        let chain_id = 9_932_u64;
        let mut session = dummy_rlpx_live_session(chain_id);
        let root = [0x44; 32];
        let pathset = vec![vec![0_u8]];
        let expected_hash = [0x55; 32];
        session.last_snap_state_root = Some(root);
        session.last_snap_trie_nodes_request_id = Some(45);
        session.pending_snap_trie_node_pathsets = vec![pathset.clone()];
        session.pending_snap_trie_node_hashes = vec![expected_hash];

        let response = EthRlpxTrieNodesResponseV1 {
            request_id: 45,
            nodes: Vec::new(),
        };
        let err = ingest_real_rlpx_snap_trie_nodes_v1(chain_id, 77, &mut session, &response)
            .expect_err("empty TrieNodes must be treated as peer state rejection");
        assert!(
            err.to_string().contains("snap_trie_nodes_empty_response"),
            "unexpected error: {err}"
        );
        assert_eq!(session.last_snap_trie_nodes_request_id, Some(45));
        assert_eq!(
            session.pending_snap_trie_node_pathsets,
            vec![pathset.clone()]
        );
        assert_eq!(session.pending_snap_trie_node_hashes, vec![expected_hash]);
        assert!(
            get_network_runtime_native_snap_trie_node_snapshot_v1(
                chain_id,
                root,
                pathset.as_slice()
            )
            .is_none(),
            "empty TrieNodes response must not cache or synthesize trie nodes"
        );
    }

    #[test]
    fn rlpx_pooled_transactions_response_must_match_requested_hashes_v1() {
        validate_eth_fullnode_native_pooled_transactions_match_request_v1(
            &[[0x11; 32], [0x22; 32], [0x33; 32]],
            &[[0x11; 32], [0x33; 32]],
        )
        .expect("ordered subset must pass");

        let err = validate_eth_fullnode_native_pooled_transactions_match_request_v1(
            &[[0x11; 32], [0x22; 32]],
            &[[0x33; 32]],
        )
        .expect_err("unrequested hash must reject");
        assert!(
            err.contains("rlpx_pooled_transactions_unrequested_hash"),
            "unexpected error: {err}"
        );

        let err = validate_eth_fullnode_native_pooled_transactions_match_request_v1(
            &[[0x11; 32], [0x22; 32]],
            &[[0x22; 32], [0x11; 32]],
        )
        .expect_err("out-of-order response must reject");
        assert!(
            err.contains("rlpx_pooled_transactions_unrequested_hash"),
            "unexpected error: {err}"
        );
    }

    fn parse_live_smoke_peer_endpoints() -> Vec<PluginPeerEndpoint> {
        let raw = std::env::var("NOVOVM_ETH_LIVE_SMOKE_ENODES")
            .unwrap_or_else(|_| LIVE_MAINNET_BOOTNODES.join(","));
        raw.split([',', ';', '\n', '\r', '\t', ' '])
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .filter_map(|entry| {
                let (node_hint, addr_hint) = parse_enode_endpoint(entry)?;
                Some(PluginPeerEndpoint {
                    endpoint: entry.to_string(),
                    node_hint,
                    addr_hint,
                })
            })
            .collect()
    }

    #[test]
    fn rlpx_receipts_validation_rejects_incomplete_and_mismatched_counts() {
        let chain_id = 9_926_100_u64;
        let peer_id = 1_300_100_u64;
        let raw_receipts = vec![vec![0xc0]];
        let pending = vec![EthFullnodeNativePendingBodyHeaderV1 {
            number: 120,
            hash: [0x12; 32],
            parent_hash: [0x11; 32],
            state_root: [0x22; 32],
            transactions_root: crate::eth_rlpx_empty_trie_root_v1(),
            receipts_root: crate::eth_rlpx_receipts_root_from_raw_receipts_v1(
                raw_receipts.as_slice(),
            ),
            tx_count: Some(1),
            withdrawal_count: Some(0),
        }];
        let valid = crate::EthRlpxReceiptsResponseV1 {
            request_id: 1,
            last_block_incomplete: false,
            blocks: vec![crate::EthRlpxReceiptBlockV1 {
                raw_receipts: raw_receipts.clone(),
                receipt_count: 1,
                receipts_available: true,
            }],
        };
        assert!(validate_real_rlpx_receipts_response_v1(
            chain_id,
            peer_id,
            pending.as_slice(),
            pending.len(),
            &valid
        )
        .is_ok());

        let mut incomplete = valid.clone();
        incomplete.last_block_incomplete = true;
        assert!(validate_real_rlpx_receipts_response_v1(
            chain_id,
            peer_id,
            pending.as_slice(),
            pending.len(),
            &incomplete
        )
        .expect_err("incomplete receipts must reject")
        .to_string()
        .contains("rlpx_receipts_last_block_incomplete"));

        let mut extra_block = valid.clone();
        extra_block.blocks.push(crate::EthRlpxReceiptBlockV1 {
            raw_receipts: raw_receipts.clone(),
            receipt_count: 1,
            receipts_available: true,
        });
        assert!(validate_real_rlpx_receipts_response_v1(
            chain_id,
            peer_id,
            pending.as_slice(),
            pending.len(),
            &extra_block
        )
        .expect_err("extra receipts block must reject")
        .to_string()
        .contains("rlpx_receipts_block_count_mismatch"));

        let mut short_response = valid.clone();
        short_response.blocks.clear();
        assert!(validate_real_rlpx_receipts_response_v1(
            chain_id,
            peer_id,
            pending.as_slice(),
            pending.len(),
            &short_response
        )
        .is_ok());

        let mut count_mismatch = valid.clone();
        count_mismatch.blocks[0].raw_receipts.clear();
        count_mismatch.blocks[0].receipt_count = 0;
        assert!(validate_real_rlpx_receipts_response_v1(
            chain_id,
            peer_id,
            pending.as_slice(),
            pending.len(),
            &count_mismatch
        )
        .expect_err("receipt count mismatch must reject")
        .to_string()
        .contains("rlpx_receipts_count_mismatch"));

        let mut root_mismatch_pending = pending.clone();
        root_mismatch_pending[0].receipts_root = [0x99; 32];
        assert!(validate_real_rlpx_receipts_response_v1(
            chain_id,
            peer_id,
            root_mismatch_pending.as_slice(),
            root_mismatch_pending.len(),
            &valid
        )
        .expect_err("receipt root mismatch must reject")
        .to_string()
        .contains("rlpx_receipts_root_mismatch"));
    }

    #[test]
    fn rlpx_empty_body_materializes_empty_receipts_without_remote_receipts() {
        let chain_id = 9_926_102_u64;
        let peer_id = 1_300_102_u64;
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let block_hash = [0x23; 32];
        let empty_root = crate::eth_rlpx_empty_trie_root_v1();
        let mut pending = vec![EthFullnodeNativePendingBodyHeaderV1 {
            number: 1_024,
            hash: block_hash,
            parent_hash: [0x22; 32],
            state_root: [0x33; 32],
            transactions_root: empty_root,
            receipts_root: empty_root,
            tx_count: Some(0),
            withdrawal_count: None,
        }];

        let materialized =
            materialize_empty_receipts_for_pending_body_headers_v1(chain_id, peer_id, &mut pending)
                .expect("materialize empty receipts");
        assert_eq!(materialized, 1);
        assert!(pending.is_empty());

        let receipt = get_network_runtime_native_receipt_snapshot_v1(chain_id, block_hash)
            .expect("empty receipt snapshot");
        assert_eq!(receipt.number, 1_024);
        assert_eq!(receipt.receipts_root, empty_root);
        assert!(receipt.raw_receipts.is_empty());
        assert_eq!(receipt.receipt_count, 0);
        assert!(receipt.receipts_available);

        let block = snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 8)
            .into_iter()
            .find(|block| block.hash == block_hash)
            .expect("canonical block receipt state");
        assert!(block.receipts_available);
        assert_eq!(block.receipt_count, Some(0));
        assert_eq!(block.receipts_root, Some(empty_root));
    }

    #[test]
    fn rlpx_missing_receipts_recovery_rebuilds_pending_from_latest_body_header() {
        let chain_id = 9_926_103_u64;
        let peer_id = 1_300_103_u64;
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let block_hash = [0x73; 32];
        let parent_hash = [0x72; 32];
        let tx_hashes = vec![[0x91; 32], [0x92; 32]];
        let raw_receipts = vec![vec![0xc0], vec![0xc1]];
        let receipts_root =
            crate::eth_rlpx_receipts_root_from_raw_receipts_v1(raw_receipts.as_slice());
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 1_024,
                hash: block_hash,
                parent_hash,
                state_root: [0x74; 32],
                transactions_root: [0x75; 32],
                receipts_root,
                ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(21_000),
                timestamp: Some(1_900_000_000),
                base_fee_per_gas: Some(7),
                withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(peer_id),
                observed_unix_ms: 1,
            },
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 1_024,
                block_hash,
                tx_hashes: tx_hashes.clone(),
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 2,
            },
        );

        let pending = build_eth_fullnode_native_missing_receipts_pending_v1(chain_id)
            .expect("missing receipts pending");
        assert_eq!(pending.number, 1_024);
        assert_eq!(pending.hash, block_hash);
        assert_eq!(pending.parent_hash, parent_hash);
        assert_eq!(pending.receipts_root, receipts_root);
        assert_eq!(pending.tx_count, Some(tx_hashes.len()));
        assert_eq!(pending.withdrawal_count, Some(0));

        set_network_runtime_native_receipt_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeReceiptSnapshotV1 {
                chain_id,
                number: 1_024,
                block_hash,
                receipts_root,
                raw_receipts,
                receipt_count: tx_hashes.len(),
                receipts_available: true,
                source_peer_id: Some(peer_id),
                observed_unix_ms: 3,
            },
        );
        assert!(
            build_eth_fullnode_native_missing_receipts_pending_v1(chain_id).is_none(),
            "available receipts must not be requested again"
        );
    }

    #[test]
    fn rlpx_missing_receipts_recovery_uses_canonical_body_when_latest_body_differs_v1() {
        let chain_id = 9_926_107_u64;
        let peer_id = 1_300_108_u64;
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let block_hash = [0x93; 32];
        let parent_hash = [0x92; 32];
        let tx_hashes = vec![[0x91; 32], [0x92; 32], [0x93; 32]];
        let raw_receipts = vec![vec![0xc0], vec![0xc1], vec![0xc2]];
        let receipts_root =
            crate::eth_rlpx_receipts_root_from_raw_receipts_v1(raw_receipts.as_slice());
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 2_024,
                hash: block_hash,
                parent_hash,
                state_root: [0x94; 32],
                transactions_root: [0x95; 32],
                receipts_root,
                ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(42_000),
                timestamp: Some(1_900_000_010),
                base_fee_per_gas: Some(8),
                withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(peer_id),
                observed_unix_ms: 1,
            },
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 2_024,
                block_hash,
                tx_hashes: tx_hashes.clone(),
                raw_tx_rlps: vec![vec![0x01], vec![0x02], vec![0x03]],
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 2,
            },
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 2_025,
                block_hash: [0x99; 32],
                tx_hashes: Vec::new(),
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 3,
            },
        );

        let pending = build_eth_fullnode_native_missing_receipts_pending_v1(chain_id)
            .expect("canonical current body should still drive receipt recovery");
        assert_eq!(pending.number, 2_024);
        assert_eq!(pending.hash, block_hash);
        assert_eq!(pending.parent_hash, parent_hash);
        assert_eq!(pending.receipts_root, receipts_root);
        assert_eq!(pending.tx_count, Some(tx_hashes.len()));
        assert_eq!(pending.withdrawal_count, Some(0));
    }

    #[test]
    fn rlpx_missing_body_recovery_rebuilds_pending_from_latest_header() {
        let chain_id = 9_926_105_u64;
        let peer_id = 1_300_106_u64;
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let block_hash = [0x83; 32];
        let parent_hash = [0x82; 32];
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 2_048,
                hash: block_hash,
                parent_hash,
                state_root: [0x84; 32],
                transactions_root: [0x85; 32],
                receipts_root: [0x86; 32],
                ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(21_000),
                timestamp: Some(1_900_000_001),
                base_fee_per_gas: Some(9),
                withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(peer_id),
                observed_unix_ms: 1,
            },
        );

        let pending = build_eth_fullnode_native_missing_body_pending_v1(chain_id)
            .expect("missing body pending");
        assert_eq!(pending.number, 2_048);
        assert_eq!(pending.hash, block_hash);
        assert_eq!(pending.parent_hash, parent_hash);
        assert_eq!(pending.tx_count, None);
        assert_eq!(pending.withdrawal_count, None);

        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 2_048,
                block_hash,
                tx_hashes: Vec::new(),
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 2,
            },
        );
        assert!(
            build_eth_fullnode_native_missing_body_pending_v1(chain_id).is_none(),
            "available body must not be requested again"
        );
    }

    #[test]
    fn rlpx_minimal_trusted_pivot_skips_body_recovery_and_pulls_next_headers() {
        let chain_id = 9_926_106_u64;
        let local = NodeId(1_300_107);
        let pivot_number = 25_271_223_u64;
        let pivot_hash = [0xc8; 32];
        let parent_hash = [0xb7; 32];
        let state_root = [0xff; 32];
        let empty_root = crate::eth_rlpx_empty_trie_root_v1();
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: pivot_number,
                hash: pivot_hash,
                parent_hash,
                state_root,
                transactions_root: empty_root,
                receipts_root: empty_root,
                ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
                logs_bloom: Vec::new(),
                gas_limit: None,
                gas_used: None,
                timestamp: None,
                base_fee_per_gas: None,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: None,
                observed_unix_ms: 1,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: crate::runtime_status::NetworkRuntimeNativeSyncPhaseV1::State,
                peer_count: 1,
                block_number: pivot_number,
                block_hash: pivot_hash,
                parent_block_hash: parent_hash,
                state_root,
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: false,
                source_peer_id: None,
                observed_unix_ms: 2,
            },
        );
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: pivot_number,
                current_block: pivot_number,
                highest_block: pivot_number + 64,
            },
        );

        assert!(
            build_eth_fullnode_native_missing_body_pending_v1(chain_id).is_none(),
            "minimal trusted pivot is a sync anchor, not a body validation target"
        );
        let request = build_eth_fullnode_native_sync_request_v1(local, chain_id)
            .expect("header pull after trusted pivot");
        match request {
            ProtocolMessage::EvmNative(EvmNativeMessage::GetBlockHeaders {
                start_height, ..
            }) => assert_eq!(start_height, pivot_number + 1),
            other => panic!("expected GetBlockHeaders after trusted pivot, got {other:?}"),
        }
    }

    #[test]
    fn rlpx_missing_body_recovery_rebuilds_batch_from_retained_headers() {
        let chain_id = 9_926_107_u64;
        let peer_id = 1_300_108_u64;
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        for offset in 0..3u8 {
            let number = 3_000 + u64::from(offset);
            set_network_runtime_native_header_snapshot_v1(
                chain_id,
                crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                    chain_id,
                    number,
                    hash: [0x90 + offset; 32],
                    parent_hash: [0x80 + offset; 32],
                    state_root: [0xa0 + offset; 32],
                    transactions_root: [0xb0 + offset; 32],
                    receipts_root: [0xc0 + offset; 32],
                    ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
                    logs_bloom: vec![0u8; 256],
                    gas_limit: Some(30_000_000),
                    gas_used: Some(21_000),
                    timestamp: Some(1_900_000_000 + u64::from(offset)),
                    base_fee_per_gas: Some(7),
                    withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                    blob_gas_used: None,
                    excess_blob_gas: None,
                    block_access_list_hash: None,
                    source_peer_id: Some(peer_id),
                    observed_unix_ms: 1 + u128::from(offset),
                },
            );
        }
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 3_001,
                block_hash: [0x91; 32],
                tx_hashes: Vec::new(),
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 8,
            },
        );

        let pending = build_eth_fullnode_native_missing_body_pending_headers_v1(chain_id);
        assert_eq!(
            pending
                .iter()
                .map(|header| header.number)
                .collect::<Vec<_>>(),
            vec![3_000, 3_002]
        );
        assert_eq!(pending[0].transactions_root, [0xb0; 32]);
        assert_eq!(pending[1].transactions_root, [0xb2; 32]);
    }

    #[test]
    fn rlpx_missing_body_idle_backfill_caps_historical_batch_v1() {
        let chain_id = 9_926_119_u64;
        let peer_id = 1_300_119_u64;
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        for offset in 0..6u8 {
            let number = 6_000 + u64::from(offset);
            set_network_runtime_native_header_snapshot_v1(
                chain_id,
                crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                    chain_id,
                    number,
                    hash: [0x20 + offset; 32],
                    parent_hash: [0x10 + offset; 32],
                    state_root: [0x30 + offset; 32],
                    transactions_root: [0x40 + offset; 32],
                    receipts_root: [0x50 + offset; 32],
                    ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
                    logs_bloom: vec![0u8; 256],
                    gas_limit: Some(30_000_000),
                    gas_used: Some(21_000),
                    timestamp: Some(1_900_020_000 + u64::from(offset)),
                    base_fee_per_gas: Some(7),
                    withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                    blob_gas_used: None,
                    excess_blob_gas: None,
                    block_access_list_hash: None,
                    source_peer_id: Some(peer_id),
                    observed_unix_ms: 1 + u128::from(offset),
                },
            );
        }

        let pending = build_eth_fullnode_native_missing_body_pending_headers_v1(chain_id);
        assert_eq!(
            pending.len(),
            ETH_FULLNODE_NATIVE_MISSING_BODY_RECOVERY_BATCH_MAX_V1
        );
        assert_eq!(
            pending
                .iter()
                .map(|header| header.number)
                .collect::<Vec<_>>(),
            vec![6_000, 6_001, 6_002, 6_005]
        );
    }

    #[test]
    fn rlpx_recovery_inflight_filter_skips_other_peer_hashes_v1() {
        let chain_id = 9_926_120_u64;
        let peer_a = 1_300_120_u64;
        let peer_b = 1_300_121_u64;
        clear_eth_fullnode_native_recovery_inflight_peer_v1(chain_id, peer_a);
        clear_eth_fullnode_native_recovery_inflight_peer_v1(chain_id, peer_b);

        let pending = (0..4u8)
            .map(|offset| EthFullnodeNativePendingBodyHeaderV1 {
                number: 7_000 + u64::from(offset),
                hash: [0x60 + offset; 32],
                parent_hash: [0x50 + offset; 32],
                state_root: [0x70 + offset; 32],
                transactions_root: [0x80 + offset; 32],
                receipts_root: [0x90 + offset; 32],
                tx_count: None,
                withdrawal_count: None,
            })
            .collect::<Vec<_>>();

        mark_eth_fullnode_native_recovery_inflight_v1(
            chain_id,
            peer_a,
            41,
            ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_BODY_KIND_V1,
            &pending[0..2],
        );
        let filtered_for_other = filter_eth_fullnode_native_recovery_inflight_headers_v1(
            chain_id,
            peer_b,
            ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_BODY_KIND_V1,
            pending.clone(),
        );
        assert_eq!(
            filtered_for_other
                .iter()
                .map(|header| header.number)
                .collect::<Vec<_>>(),
            vec![7_002, 7_003]
        );

        let filtered_for_owner = filter_eth_fullnode_native_recovery_inflight_headers_v1(
            chain_id,
            peer_a,
            ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_BODY_KIND_V1,
            pending.clone(),
        );
        assert_eq!(filtered_for_owner.len(), pending.len());

        clear_eth_fullnode_native_recovery_inflight_request_v1(
            chain_id,
            peer_a,
            41,
            ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_BODY_KIND_V1,
        );
        let filtered_after_clear = filter_eth_fullnode_native_recovery_inflight_headers_v1(
            chain_id,
            peer_b,
            ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_BODY_KIND_V1,
            pending.clone(),
        );
        assert_eq!(filtered_after_clear.len(), pending.len());

        mark_eth_fullnode_native_recovery_inflight_v1(
            chain_id,
            peer_a,
            42,
            ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_RECEIPT_KIND_V1,
            &pending[1..3],
        );
        let receipt_filtered_for_other = filter_eth_fullnode_native_recovery_inflight_headers_v1(
            chain_id,
            peer_b,
            ETH_FULLNODE_NATIVE_RECOVERY_INFLIGHT_RECEIPT_KIND_V1,
            pending,
        );
        assert_eq!(
            receipt_filtered_for_other
                .iter()
                .map(|header| header.number)
                .collect::<Vec<_>>(),
            vec![7_000, 7_003]
        );
    }

    #[test]
    fn rlpx_header_inflight_filter_skips_duplicate_forward_start_v1() {
        let chain_id = 9_926_121_u64;
        let peer_a = 1_300_122_u64;
        let peer_b = 1_300_123_u64;
        clear_eth_fullnode_native_header_inflight_peer_v1(chain_id, peer_a);
        clear_eth_fullnode_native_header_inflight_peer_v1(chain_id, peer_b);

        assert!(should_dispatch_eth_fullnode_native_header_request_v1(
            chain_id, peer_a, 8_000, 0, false
        ));
        mark_eth_fullnode_native_header_inflight_v1(chain_id, peer_a, 51, 8_000, 0, false);
        assert!(
            should_dispatch_eth_fullnode_native_header_request_v1(
                chain_id, peer_a, 8_000, 0, false
            ),
            "the owning peer may continue its own pending header request"
        );
        assert!(
            !should_dispatch_eth_fullnode_native_header_request_v1(
                chain_id, peer_b, 8_000, 0, false
            ),
            "another peer must not duplicate the same forward header start"
        );
        assert!(
            should_dispatch_eth_fullnode_native_header_request_v1(
                chain_id, peer_b, 8_001, 0, false
            ),
            "a different start remains dispatchable"
        );

        clear_eth_fullnode_native_header_inflight_request_v1(chain_id, peer_a, 51);
        assert!(
            should_dispatch_eth_fullnode_native_header_request_v1(
                chain_id, peer_b, 8_000, 0, false
            ),
            "response/failure cleanup must release the header start"
        );
    }

    #[test]
    fn rlpx_missing_body_recovery_does_not_block_forward_chase_with_old_gaps() {
        let chain_id = 9_926_117_u64;
        let peer_id = 1_300_118_u64;
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        for offset in 0..3u8 {
            let number = 5_000 + u64::from(offset);
            set_network_runtime_native_header_snapshot_v1(
                chain_id,
                crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                    chain_id,
                    number,
                    hash: [0xd0 + offset; 32],
                    parent_hash: [0xc0 + offset; 32],
                    state_root: [0xe0 + offset; 32],
                    transactions_root: [0xf0 + offset; 32],
                    receipts_root: [0x70 + offset; 32],
                    ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
                    logs_bloom: vec![0u8; 256],
                    gas_limit: Some(30_000_000),
                    gas_used: Some(21_000),
                    timestamp: Some(1_900_010_000 + u64::from(offset)),
                    base_fee_per_gas: Some(7),
                    withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                    blob_gas_used: None,
                    excess_blob_gas: None,
                    block_access_list_hash: None,
                    source_peer_id: Some(peer_id),
                    observed_unix_ms: 1 + u128::from(offset),
                },
            );
        }
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 5_002,
                block_hash: [0xd2; 32],
                tx_hashes: Vec::new(),
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 8,
            },
        );
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 5_000,
                current_block: 5_002,
                highest_block: 5_064,
            },
        );

        assert!(
            build_eth_fullnode_native_missing_body_pending_headers_v1(chain_id).is_empty(),
            "historical body gaps must not block forward header pulls while highest > current"
        );
    }

    #[test]
    fn rlpx_missing_body_recovery_batches_current_header_only_suffix_while_chasing() {
        let chain_id = 9_926_118_u64;
        let peer_id = 1_300_119_u64;
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 5_000,
                hash: [0x44; 32],
                parent_hash: [0x43; 32],
                state_root: [0x45; 32],
                transactions_root: [0x46; 32],
                receipts_root: [0x47; 32],
                ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(21_000),
                timestamp: Some(1_900_020_000),
                base_fee_per_gas: Some(7),
                withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(peer_id),
                observed_unix_ms: 1,
            },
        );

        let base_number = 6_000_u64;
        let mut parent_hash = [0x90; 32];
        for offset in 0..17u8 {
            let number = base_number + u64::from(offset);
            let hash = [0xa0 + offset; 32];
            set_network_runtime_native_header_snapshot_v1(
                chain_id,
                crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                    chain_id,
                    number,
                    hash,
                    parent_hash,
                    state_root: [0xb0 + offset; 32],
                    transactions_root: [0xc0 + offset; 32],
                    receipts_root: [0xd0 + offset; 32],
                    ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
                    logs_bloom: vec![0u8; 256],
                    gas_limit: Some(30_000_000),
                    gas_used: Some(21_000),
                    timestamp: Some(1_900_020_100 + u64::from(offset)),
                    base_fee_per_gas: Some(7),
                    withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                    blob_gas_used: None,
                    excess_blob_gas: None,
                    block_access_list_hash: None,
                    source_peer_id: Some(peer_id),
                    observed_unix_ms: 10 + u128::from(offset),
                },
            );
            parent_hash = hash;
        }
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: base_number,
                current_block: base_number + 4,
                highest_block: base_number + 128,
            },
        );

        let pending = build_eth_fullnode_native_missing_body_pending_headers_v1(chain_id);
        assert_eq!(
            pending
                .iter()
                .map(|header| header.number)
                .collect::<Vec<_>>(),
            vec![6_016]
        );
        assert!(
            pending.iter().all(|header| header.number >= base_number),
            "old historical gaps must stay out of the chase-time suffix batch"
        );
    }

    #[test]
    fn rlpx_missing_body_recovery_rebuilds_batch_without_source_peer_id() {
        let chain_id = 9_926_108_u64;
        let peer_id = 1_300_109_u64;
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let zero_root = [0x10_u8; 32];
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 4_002,
                hash: [0xa2; 32],
                parent_hash: [0xa1; 32],
                state_root: [0xb0; 32],
                transactions_root: zero_root,
                receipts_root: zero_root,
                ommers_hash: [0x33; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(21_000),
                timestamp: Some(1_900_000_000),
                base_fee_per_gas: Some(7),
                withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: None,
                observed_unix_ms: 1,
            },
        );
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 4_001,
                hash: [0xa1; 32],
                parent_hash: [0xa0; 32],
                state_root: [0xaf; 32],
                transactions_root: [0x12; 32],
                receipts_root: [0x13; 32],
                ommers_hash: [0x33; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(21_000),
                timestamp: Some(1_900_000_001),
                base_fee_per_gas: Some(7),
                withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(peer_id),
                observed_unix_ms: 2,
            },
        );
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 4_000,
                hash: [0xa0; 32],
                parent_hash: [0x9f; 32],
                state_root: [0xae; 32],
                transactions_root: [0x14; 32],
                receipts_root: [0x15; 32],
                ommers_hash: [0x33; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(21_000),
                timestamp: Some(1_900_000_002),
                base_fee_per_gas: Some(7),
                withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(peer_id),
                observed_unix_ms: 3,
            },
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 4_000,
                block_hash: [0xa0; 32],
                tx_hashes: Vec::new(),
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 9,
            },
        );

        let pending = build_eth_fullnode_native_missing_body_pending_headers_v1(chain_id);
        assert_eq!(
            pending
                .iter()
                .map(|header| header.number)
                .collect::<Vec<_>>(),
            vec![4_001, 4_002]
        );
    }

    #[test]
    fn real_rlpx_worker_recovers_missing_receipts_before_new_header_pull() {
        let chain_id = 9_926_104_u64;
        let local = NodeId(1_300_104);
        let remote = NodeId(1_300_105);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let block_hash = [0xa3; 32];
        let raw_receipts = vec![vec![0xc0]];
        let receipts_root =
            crate::eth_rlpx_receipts_root_from_raw_receipts_v1(raw_receipts.as_slice());
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 1_024,
                hash: block_hash,
                parent_hash: [0xa2; 32],
                state_root: [0xa4; 32],
                transactions_root: [0xa5; 32],
                receipts_root,
                ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(21_000),
                timestamp: Some(1_900_000_000),
                base_fee_per_gas: Some(7),
                withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(remote.0),
                observed_unix_ms: 1,
            },
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 1_024,
                block_hash,
                tx_hashes: vec![[0xb1; 32]],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 2,
            },
        );
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 1_024,
                current_block: 1_024,
                highest_block: 2_048,
            },
        );

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        let server_receipts = raw_receipts.clone();
        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/missing-receipts-recovery-test",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: [0u8; 32],
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    0,
                    0,
                ),
                earliest_block: 1,
                latest_block: 2_048,
                latest_block_hash: [0xd1; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, _) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );

            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read worker request");
                assert_ne!(
                    code,
                    crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG,
                    "missing receipt recovery must run before a new header pull"
                );
                if code
                    == crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_GET_RECEIPTS_MSG
                {
                    let request = crate::eth_rlpx_parse_get_receipts_payload_v1(payload.as_slice())
                        .expect("parse get receipts");
                    assert_eq!(request.hashes, vec![block_hash]);
                    assert_eq!(request.first_block_receipt_index, 0);
                    let response_blocks = vec![server_receipts];
                    let receipts_payload = crate::eth_rlpx_build_receipts_payload_v1(
                        request.request_id,
                        false,
                        response_blocks.as_slice(),
                        crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION,
                    );
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_RECEIPTS_MSG,
                        receipts_payload.as_slice(),
                    )
                    .expect("write receipts");
                    accepted
                        .set_read_timeout(Some(Duration::from_millis(500)))
                        .expect("set post-receipts read timeout");
                    if let Ok((followup_code, _)) =
                        crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    {
                        assert_ne!(
                            followup_code,
                            crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                                + crate::ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG,
                            "fresh receipt material must defer the next header pull to a later tick"
                        );
                    }
                    break;
                }
            }
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        assert_eq!(report0.sync_requests, 1);

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(5)
            && get_network_runtime_native_receipt_snapshot_v1(chain_id, block_hash).is_none()
        {
            let _ = worker
                .drive_real_network_once()
                .expect("receipts response tick");
            thread::sleep(Duration::from_millis(5));
        }
        let receipt = get_network_runtime_native_receipt_snapshot_v1(chain_id, block_hash)
            .expect("recovered receipt snapshot");
        assert!(receipt.receipts_available);
        assert_eq!(receipt.receipt_count, 1);
        assert_eq!(receipt.raw_receipts, raw_receipts);

        server.join().expect("server join");
    }

    #[test]
    fn rlpx_state_root_continuity_validates_empty_withdrawal_block() {
        let chain_id = 9_926_101_u64;
        let peer_id = 1_300_101_u64;
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let parent_hash = [0x41; 32];
        let child_hash = [0x42; 32];
        let parent_state_root = [0x51; 32];
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 120,
                hash: parent_hash,
                parent_hash: [0x40; 32],
                state_root: parent_state_root,
                transactions_root: crate::eth_rlpx_empty_trie_root_v1(),
                receipts_root: crate::eth_rlpx_empty_trie_root_v1(),
                ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(0),
                timestamp: Some(1_900_000_000),
                base_fee_per_gas: Some(7),
                withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(peer_id),
                observed_unix_ms: 1,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: crate::runtime_status::NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 1,
                block_number: 120,
                block_hash: parent_hash,
                parent_block_hash: [0x40; 32],
                state_root: parent_state_root,
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: true,
                source_peer_id: Some(peer_id),
                observed_unix_ms: 2,
            },
        );
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 121,
                hash: child_hash,
                parent_hash,
                state_root: parent_state_root,
                transactions_root: crate::eth_rlpx_empty_trie_root_v1(),
                receipts_root: crate::eth_rlpx_empty_trie_root_v1(),
                ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(0),
                timestamp: Some(1_900_000_012),
                base_fee_per_gas: Some(7),
                withdrawals_root: Some(crate::eth_rlpx_empty_trie_root_v1()),
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(peer_id),
                observed_unix_ms: 3,
            },
        );

        let valid = vec![EthFullnodeNativePendingBodyHeaderV1 {
            number: 121,
            hash: child_hash,
            parent_hash,
            state_root: parent_state_root,
            transactions_root: crate::eth_rlpx_empty_trie_root_v1(),
            receipts_root: crate::eth_rlpx_empty_trie_root_v1(),
            tx_count: Some(0),
            withdrawal_count: Some(0),
        }];
        validate_real_rlpx_state_root_continuity_v1(chain_id, peer_id, valid.as_slice())
            .expect("empty block stateRoot continuity");
        let child = snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 8)
            .into_iter()
            .find(|block| block.hash == child_hash)
            .expect("child canonical block state");
        assert!(child.state_root_validated);
        assert_eq!(
            child.state_root_validation_method.as_deref(),
            Some("empty_body_parent_state_root_continuity")
        );

        let invalid = vec![EthFullnodeNativePendingBodyHeaderV1 {
            state_root: [0x99; 32],
            ..valid[0]
        }];
        assert!(
            validate_real_rlpx_state_root_continuity_v1(chain_id, peer_id, invalid.as_slice())
                .expect_err("stateRoot mismatch must reject")
                .to_string()
                .contains("rlpx_state_root_continuity_mismatch")
        );
    }

    #[test]
    fn in_memory_transport_roundtrip() {
        let t = InMemoryTransport::new(8);
        let n0 = NodeId(0);
        let n1 = NodeId(1);
        t.register(n0);
        t.register(n1);
        let msg = ProtocolMessage::Gossip(GossipMessage::Heartbeat {
            from: n0,
            shard: ShardId(1),
        });
        t.send(n1, msg).unwrap();
        let recv = t.try_recv(n1).unwrap();
        assert!(matches!(
            recv,
            Some(ProtocolMessage::Gossip(GossipMessage::Heartbeat { .. }))
        ));
    }

    #[test]
    fn rlpx_remote_closed_errors_are_not_plain_timeouts() {
        let eof = "rlpx_frame_header_read_failed:eof read=0/16";
        let auth_ack_eof = "rlpx_ack_prefix_read_failed:eof read=0/2";
        let auth_ack_timeout =
            "rlpx_ack_prefix_read_failed:connection attempt failed (os error 10060) read=0/2";
        let aborted = "rlpx_frame_body_read_failed:connection aborted (os error 10053) read=0/48";
        let reset_mid_body = "rlpx_frame_body_read_failed:远程主机强迫关闭了一个现有的连接。 (os error 10054) read=107008/171040";
        assert!(eth_fullnode_rlpx_error_is_remote_closed_v1(eof));
        assert!(eth_fullnode_rlpx_error_is_remote_closed_v1(auth_ack_eof));
        assert!(!eth_fullnode_rlpx_error_is_remote_closed_v1(
            auth_ack_timeout
        ));
        assert!(eth_fullnode_rlpx_error_is_timeout_v1(auth_ack_timeout));
        assert!(eth_fullnode_rlpx_error_is_timeout_v1(
            "rlpx_frame_body_read_failed:partial_read_timeout read=184284/400576 deadline_ms=4000"
        ));
        assert!(eth_fullnode_rlpx_error_is_remote_closed_v1(aborted));
        assert!(eth_fullnode_rlpx_error_is_remote_closed_v1(reset_mid_body));
        assert!(eth_fullnode_rlpx_error_is_session_desync_v1(
            "rlpx_frame_header_mac_mismatch"
        ));
        assert!(eth_fullnode_rlpx_error_is_session_desync_v1(
            "rlpx_frame_mac_mismatch"
        ));
        assert!(eth_fullnode_rlpx_error_is_remote_closed_v1(
            "rlpx_remote_disconnected_ingest:reason_code=4 reason=too_many_peers"
        ));
        assert_eq!(
            eth_fullnode_rlpx_error_disconnect_reason_code_v1(
                "rlpx_remote_disconnected_ingest:reason_code=4 reason=too_many_peers"
            ),
            Some(0x04)
        );
        assert_eq!(
            eth_fullnode_rlpx_error_disconnect_reason_code_v1(
                "rlpx_remote_disconnected_ingest:reason_code=18446744073709551615 reason=unknown"
            ),
            None
        );
        assert!(!eth_fullnode_rlpx_error_is_timeout_v1(eof));
        assert!(eth_fullnode_rlpx_error_is_timeout_v1(
            "operation would block"
        ));
    }

    #[test]
    fn rlpx_request_write_errors_use_runtime_failure_class_v1() {
        let chain_id = 99_160_418_u64;
        let remote_close_peer = NodeId(418_001);
        let mac_desync_peer = NodeId(418_002);
        let timeout_peer = NodeId(418_003);
        let unknown_peer = NodeId(418_004);
        let capacity_reject_peer = NodeId(418_005);

        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            remote_close_peer.0,
            "headers_request_write_failed",
            "rlpx_frame_mac_write_failed:远程主机强迫关闭了一个现有的连接。 (os error 10054)",
        );
        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            mac_desync_peer.0,
            "headers_request_write_failed",
            "rlpx_frame_header_mac_mismatch",
        );
        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            timeout_peer.0,
            "headers_request_write_failed",
            "rlpx_frame_body_read_failed:partial_read_timeout read=8/16 deadline_ms=4000",
        );
        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            unknown_peer.0,
            "headers_request_write_failed",
            "broken pipe",
        );
        observe_eth_fullnode_rlpx_request_write_error_v1(
            chain_id,
            capacity_reject_peer.0,
            "headers_request_write_failed",
            "rlpx_remote_disconnected_ingest:reason_code=4 reason=too_many_peers",
        );

        let snapshots = snapshot_network_runtime_eth_peer_sessions_for_peers_v1(
            chain_id,
            &[
                remote_close_peer,
                mac_desync_peer,
                timeout_peer,
                unknown_peer,
                capacity_reject_peer,
            ],
        )
        .into_iter()
        .map(|snapshot| (snapshot.peer_id, snapshot))
        .collect::<HashMap<_, _>>();

        let remote_close = snapshots.get(&remote_close_peer.0).expect("remote close");
        assert_eq!(
            remote_close.last_failure_class,
            Some(crate::EthPeerFailureClassV1::Disconnect)
        );
        assert_eq!(remote_close.disconnect_count, 1);
        assert_eq!(remote_close.handshake_failure_count, 0);

        let mac_desync = snapshots.get(&mac_desync_peer.0).expect("mac desync");
        assert_eq!(
            mac_desync.last_failure_class,
            Some(crate::EthPeerFailureClassV1::Disconnect)
        );
        assert_eq!(mac_desync.disconnect_count, 1);
        assert_eq!(mac_desync.handshake_failure_count, 0);

        let timeout = snapshots.get(&timeout_peer.0).expect("timeout");
        assert_eq!(
            timeout.last_failure_class,
            Some(crate::EthPeerFailureClassV1::Timeout)
        );
        assert_eq!(timeout.timeout_count, 1);

        let unknown = snapshots.get(&unknown_peer.0).expect("unknown");
        assert_eq!(
            unknown.last_failure_class,
            Some(crate::EthPeerFailureClassV1::HandshakeFailure)
        );
        assert_eq!(unknown.handshake_failure_count, 1);

        let capacity_reject = snapshots
            .get(&capacity_reject_peer.0)
            .expect("capacity reject");
        assert_eq!(
            capacity_reject.last_failure_class,
            Some(crate::EthPeerFailureClassV1::Disconnect)
        );
        assert_eq!(capacity_reject.last_failure_reason_code, Some(0x04));
        assert_eq!(capacity_reject.last_disconnect_reason_code, Some(0x04));
        assert_eq!(capacity_reject.disconnect_too_many_peers_count, 1);
    }

    #[test]
    fn rlpx_receipt_updates_count_as_material_peer_success_v1() {
        let mut report = EthFullnodeNativeRealDriveReportV1 {
            body_updated_peer_ids: vec![11, 12],
            receipt_updated_peer_ids: vec![12, 13],
            ..EthFullnodeNativeRealDriveReportV1::default()
        };

        assert_eq!(
            eth_fullnode_native_material_success_peer_ids_v1(&report),
            vec![11, 12, 13]
        );

        report.body_updated_peer_ids.clear();
        assert_eq!(
            eth_fullnode_native_material_success_peer_ids_v1(&report),
            vec![12, 13]
        );
    }

    #[test]
    fn rlpx_headers_request_batch_respects_runtime_budget_v1() {
        let mut budget = EthFullnodeBudgetHooksV1::default();
        budget.sync_pull_headers_batch = 64;
        assert_eq!(
            eth_fullnode_native_budget_capped_headers_batch_v1(2_048, &budget),
            64
        );
        assert_eq!(
            eth_fullnode_native_budget_capped_headers_batch_v1(32, &budget),
            32
        );
    }

    #[test]
    fn udp_transport_roundtrip() {
        let n0 = NodeId(0);
        let n1 = NodeId(1);
        let t0 = UdpTransport::bind(n0, "127.0.0.1:0").unwrap();
        let t1 = UdpTransport::bind(n1, "127.0.0.1:0").unwrap();
        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let msg = ProtocolMessage::Gossip(GossipMessage::Heartbeat {
            from: n0,
            shard: ShardId(7),
        });
        t0.send(n1, msg).unwrap();

        let started = std::time::Instant::now();
        let mut got = None;
        while started.elapsed() < Duration::from_millis(500) {
            if let Some(m) = t1.try_recv(n1).unwrap() {
                got = Some(m);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            got,
            Some(ProtocolMessage::Gossip(GossipMessage::Heartbeat { .. }))
        ));
    }

    #[test]
    fn udp_register_peer_updates_runtime_sync_peer_count() {
        let chain_id = 9_991_u64;
        let n0 = NodeId(100);
        let n1 = NodeId(101);
        let n2 = NodeId(102);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let t2 = UdpTransport::bind_for_chain(n2, "127.0.0.1:0", chain_id).unwrap();
        let a1 = t1.local_addr().unwrap();
        let a2 = t2.local_addr().unwrap();

        t0.register_peer(n1, &a1.to_string()).unwrap();
        t0.register_peer(n2, &a2.to_string()).unwrap();

        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.peer_count, 2);
    }

    #[test]
    fn udp_unregister_peer_updates_runtime_sync_peer_count() {
        let chain_id = 9_994_u64;
        let n0 = NodeId(120);
        let n1 = NodeId(121);
        let n2 = NodeId(122);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let t2 = UdpTransport::bind_for_chain(n2, "127.0.0.1:0", chain_id).unwrap();
        let a1 = t1.local_addr().unwrap();
        let a2 = t2.local_addr().unwrap();

        t0.register_peer(n1, &a1.to_string()).unwrap();
        t0.register_peer(n2, &a2.to_string()).unwrap();
        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.peer_count, 2);

        t0.unregister_peer(n1).unwrap();
        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.peer_count, 1);

        t0.unregister_peer(n2).unwrap();
        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.peer_count, 0);
        assert_eq!(status.highest_block, status.current_block);
    }

    #[test]
    fn tcp_send_connect_failure_marks_runtime_peer_disconnected() {
        let chain_id = 9_995_u64;
        let n0 = NodeId(130);
        let n1 = NodeId(131);

        let mut t0 = TcpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        t0.set_connect_timeout_ms(20);

        let tmp_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer_addr = tmp_listener.local_addr().unwrap();
        drop(tmp_listener);
        t0.register_peer(n1, &peer_addr.to_string()).unwrap();

        let before =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(before.peer_count, 1);

        let msg = ProtocolMessage::Gossip(GossipMessage::Heartbeat {
            from: n0,
            shard: ShardId(1),
        });
        let res = t0.send(n1, msg);
        assert!(res.is_err());

        let after =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(after.peer_count, 0);
    }

    #[test]
    fn udp_try_recv_updates_runtime_progress_from_pacemaker_messages() {
        let chain_id = 9_992_u64;
        let n0 = NodeId(200);
        let n1 = NodeId(201);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let msg = ProtocolMessage::Pacemaker(PacemakerMessage::NewView {
            from: n0,
            height: 12,
            view: 3,
            high_qc_height: 19,
        });
        t0.send(n1, msg).unwrap();

        let started = std::time::Instant::now();
        let mut got = None;
        while started.elapsed() < Duration::from_millis(500) {
            if let Some(m) = t1.try_recv(n1).unwrap() {
                got = Some(m);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            got,
            Some(ProtocolMessage::Pacemaker(PacemakerMessage::NewView { .. }))
        ));
        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 19);
        assert_eq!(status.highest_block, 19);
        assert_eq!(status.starting_block, 19);
    }

    #[test]
    fn udp_try_recv_registers_runtime_peer_from_message_sender() {
        let chain_id = 9_996_u64;
        let n0 = NodeId(220);
        let n1 = NodeId(221);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();

        let before =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(before.peer_count, 1);

        let msg = ProtocolMessage::Gossip(GossipMessage::Heartbeat {
            from: n0,
            shard: ShardId(5),
        });
        t0.send(n1, msg).unwrap();

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(500) {
            if t1.try_recv(n1).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let after =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(after.peer_count, 2);
    }

    #[test]
    fn udp_try_recv_autolearns_sender_addr_for_reply_send() {
        let chain_id = 9_997_u64;
        let n0 = NodeId(230);
        let n1 = NodeId(231);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();

        t0.send(
            n1,
            ProtocolMessage::Gossip(GossipMessage::Heartbeat {
                from: n0,
                shard: ShardId(8),
            }),
        )
        .unwrap();

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(500) {
            if t1.try_recv(n1).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let send_back = t1.send(
            n0,
            ProtocolMessage::Gossip(GossipMessage::Heartbeat {
                from: n1,
                shard: ShardId(9),
            }),
        );
        assert!(send_back.is_ok());

        let started = std::time::Instant::now();
        let mut got_back = false;
        while started.elapsed() < Duration::from_millis(500) {
            if t0.try_recv(n0).unwrap().is_some() {
                got_back = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(got_back);
    }

    #[test]
    fn udp_try_recv_updates_runtime_progress_from_state_sync_block_header_wire() {
        let chain_id = 9_993_u64;
        let n0 = NodeId(210);
        let n1 = NodeId(211);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let header = BlockHeaderWireV1 {
            height: 88,
            epoch_id: 7,
            parent_hash: [1u8; 32],
            state_root: [2u8; 32],
            governance_chain_audit_root: [3u8; 32],
            tx_count: 5,
            batch_count: 2,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [4u8; 32],
            },
        };
        let state_sync = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: n0.0 as u32,
            to: n1.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_block_header_wire_v1(&header),
            timestamp: 0,
            seq: 1,
        });
        t0.send(n1, state_sync).unwrap();

        let started = std::time::Instant::now();
        let mut got = None;
        while started.elapsed() < Duration::from_millis(500) {
            if let Some(m) = t1.try_recv(n1).unwrap() {
                got = Some(m);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            got,
            Some(ProtocolMessage::DistributedOcccGossip(_))
        ));
        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 88);
        assert_eq!(status.highest_block, 88);
        assert_eq!(status.starting_block, 88);
    }

    #[test]
    fn udp_try_recv_state_sync_advances_local_progress_when_sender_field_is_remote() {
        let chain_id = 9_877_u64;
        let n0 = NodeId(240);
        let n1 = NodeId(241);
        let remote_sender = NodeId(999);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let header = BlockHeaderWireV1 {
            height: 233,
            epoch_id: 5,
            parent_hash: [9u8; 32],
            state_root: [8u8; 32],
            governance_chain_audit_root: [7u8; 32],
            tx_count: 4,
            batch_count: 1,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [6u8; 32],
            },
        };
        let state_sync = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: remote_sender.0 as u32,
            to: n1.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_block_header_wire_v1(&header),
            timestamp: 0,
            seq: 3,
        });
        t0.send(n1, state_sync).unwrap();

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(500) {
            if t1.try_recv(n1).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 233);
        assert_eq!(status.highest_block, 233);
        assert_eq!(status.starting_block, 233);
    }

    #[test]
    fn udp_try_recv_updates_runtime_progress_from_shard_state_block_header_wire() {
        let chain_id = 9_883_u64;
        let n0 = NodeId(212);
        let n1 = NodeId(213);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let header = BlockHeaderWireV1 {
            height: 144,
            epoch_id: 11,
            parent_hash: [5u8; 32],
            state_root: [6u8; 32],
            governance_chain_audit_root: [7u8; 32],
            tx_count: 9,
            batch_count: 3,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [8u8; 32],
            },
        };
        let shard_state = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: n0.0 as u32,
            to: n1.0 as u32,
            msg_type: DistributedMessageType::ShardState,
            payload: encode_block_header_wire_v1(&header),
            timestamp: 0,
            seq: 2,
        });
        t0.send(n1, shard_state).unwrap();

        let started = std::time::Instant::now();
        let mut got = None;
        while started.elapsed() < Duration::from_millis(500) {
            if let Some(m) = t1.try_recv(n1).unwrap() {
                got = Some(m);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            got,
            Some(ProtocolMessage::DistributedOcccGossip(_))
        ));
        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 144);
        assert_eq!(status.highest_block, 144);
        assert_eq!(status.starting_block, 144);
    }

    #[test]
    fn runtime_sync_receive_path_treats_shard_state_as_local_progress() {
        let chain_id = 9_888_u64;
        let remote = NodeId(901);
        let local = NodeId(902);

        let header = BlockHeaderWireV1 {
            height: 777,
            epoch_id: 13,
            parent_hash: [0x11u8; 32],
            state_root: [0x22u8; 32],
            governance_chain_audit_root: [0x33u8; 32],
            tx_count: 3,
            batch_count: 1,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [0x44u8; 32],
            },
        };
        let shard_state = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: remote.0 as u32,
            to: local.0 as u32,
            msg_type: DistributedMessageType::ShardState,
            payload: encode_block_header_wire_v1(&header),
            timestamp: 0,
            seq: 1,
        });

        maybe_update_runtime_sync_from_protocol_message(chain_id, &shard_state, None, None);

        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 777);
        assert_eq!(status.highest_block, 777);
    }

    #[test]
    fn runtime_sync_pull_request_payload_decodes_nsp1() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&RUNTIME_SYNC_PULL_REQUEST_MAGIC);
        payload.push(3);
        payload.extend_from_slice(&55u64.to_le_bytes());
        payload.extend_from_slice(&101u64.to_le_bytes());
        payload.extend_from_slice(&164u64.to_le_bytes());

        let decoded = decode_runtime_sync_pull_request(&payload).expect("decode nsp1 payload");
        assert_eq!(decoded.phase, NetworkRuntimeNativeSyncPhaseV1::Bodies);
        assert_eq!(decoded.chain_id, 55);
        assert_eq!(decoded.from_block, 101);
        assert_eq!(decoded.to_block, 164);
    }

    #[test]
    fn runtime_sync_pull_tracking_uses_capped_response_target() {
        let chain_id = 9_892_u64;
        let local = NodeId(940);
        let remote = NodeId(941);
        clear_runtime_sync_pull_target(chain_id, local, remote);

        let outbound = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: local.0 as u32,
            to: remote.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_runtime_sync_pull_request_payload(
                chain_id,
                NetworkRuntimeNativeSyncPhaseV1::Headers,
                1_000,
                4_000,
            ),
            timestamp: 0,
            seq: 1,
        });
        maybe_track_runtime_sync_pull_request_outbound(chain_id, local, &outbound);
        let tracked_to =
            get_runtime_sync_pull_target(chain_id, local, remote).expect("target should exist");
        assert_eq!(
            tracked_to,
            1_000 + RUNTIME_SYNC_PULL_RESPONSE_BATCH_MAX - 1,
            "tracked target should follow single-response capped upper bound"
        );
        clear_runtime_sync_pull_target(chain_id, local, remote);
    }

    #[test]
    fn runtime_sync_pull_shard_state_request_triggers_shard_state_response() {
        let chain_id = 9_893_u64;
        let requester = NodeId(950);
        let responder = NodeId(951);

        let tx = UdpTransport::bind_for_chain(requester, "127.0.0.1:0", chain_id).unwrap();
        let rx = UdpTransport::bind_for_chain(responder, "127.0.0.1:0", chain_id).unwrap();
        let tx_addr = tx.local_addr().unwrap();
        let rx_addr = rx.local_addr().unwrap();
        tx.register_peer(responder, &rx_addr.to_string()).unwrap();
        rx.register_peer(requester, &tx_addr.to_string()).unwrap();

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 2,
                starting_block: 400,
                current_block: 420,
                highest_block: 520,
            },
        );

        let pull_request = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: requester.0 as u32,
            to: responder.0 as u32,
            msg_type: DistributedMessageType::ShardState,
            payload: encode_runtime_sync_pull_request_payload(
                chain_id,
                NetworkRuntimeNativeSyncPhaseV1::Bodies,
                410,
                415,
            ),
            timestamp: 0,
            seq: 1,
        });
        tx.send(responder, pull_request).unwrap();

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(500) {
            if rx.try_recv(responder).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let started_reply = std::time::Instant::now();
        let mut got_reply = false;
        while started_reply.elapsed() < Duration::from_millis(500) {
            if let Some(msg) = tx.try_recv(requester).unwrap() {
                let ProtocolMessage::DistributedOcccGossip(reply) = msg else {
                    continue;
                };
                if matches!(reply.msg_type, DistributedMessageType::ShardState)
                    && decode_block_header_wire_v1(&reply.payload).is_ok()
                {
                    got_reply = true;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(got_reply, "expected shard-state sync response");
    }

    #[test]
    fn runtime_sync_pull_followup_request_builds_next_window() {
        let chain_id = 9_890_u64;
        let local = NodeId(920);
        let remote = NodeId(921);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 3,
                starting_block: 600,
                current_block: 640,
                highest_block: 700,
            },
        );
        set_runtime_sync_pull_target(chain_id, local, remote, 650);

        let header_before_target = BlockHeaderWireV1 {
            height: 640,
            epoch_id: 1,
            parent_hash: [0x11u8; 32],
            state_root: [0x22u8; 32],
            governance_chain_audit_root: [0x33u8; 32],
            tx_count: 0,
            batch_count: 0,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [0x44u8; 32],
            },
        };
        let sync_reply_before_target =
            ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                from: remote.0 as u32,
                to: local.0 as u32,
                msg_type: DistributedMessageType::StateSync,
                payload: encode_block_header_wire_v1(&header_before_target),
                timestamp: 0,
                seq: 1,
            });
        assert!(
            maybe_build_runtime_sync_pull_followup_request(
                chain_id,
                local,
                &sync_reply_before_target
            )
            .is_none(),
            "should wait until current window target is reached"
        );

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 3,
                starting_block: 600,
                current_block: 650,
                highest_block: 700,
            },
        );
        let header_on_target = BlockHeaderWireV1 {
            height: 650,
            epoch_id: 1,
            parent_hash: [0x11u8; 32],
            state_root: [0x22u8; 32],
            governance_chain_audit_root: [0x33u8; 32],
            tx_count: 0,
            batch_count: 0,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [0x44u8; 32],
            },
        };
        let sync_reply_on_target =
            ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                from: remote.0 as u32,
                to: local.0 as u32,
                msg_type: DistributedMessageType::StateSync,
                payload: encode_block_header_wire_v1(&header_on_target),
                timestamp: 0,
                seq: 2,
            });

        let (target, followup) =
            maybe_build_runtime_sync_pull_followup_request(chain_id, local, &sync_reply_on_target)
                .expect("followup request should be generated");
        assert_eq!(target, remote);
        let ProtocolMessage::DistributedOcccGossip(followup_msg) = followup else {
            panic!("followup should be distributed gossip");
        };
        assert!(matches!(
            followup_msg.msg_type,
            DistributedMessageType::ShardState
        ));
        let payload = decode_runtime_sync_pull_request(&followup_msg.payload)
            .expect("followup payload should be NSP1");
        assert_eq!(payload.phase, NetworkRuntimeNativeSyncPhaseV1::Finalize);
        assert_eq!(payload.chain_id, chain_id);
        assert_eq!(payload.from_block, 651);
        assert!(payload.to_block >= payload.from_block);
        assert!(payload.to_block <= 700);
    }

    #[test]
    fn runtime_sync_pull_state_phase_uses_smaller_response_cap() {
        let chain_id = 9_895_u64;
        let local = NodeId(970);
        let remote = NodeId(971);
        clear_runtime_sync_pull_target(chain_id, local, remote);

        let outbound = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: local.0 as u32,
            to: remote.0 as u32,
            msg_type: DistributedMessageType::ShardState,
            payload: encode_runtime_sync_pull_request_payload(
                chain_id,
                NetworkRuntimeNativeSyncPhaseV1::State,
                1_000,
                4_000,
            ),
            timestamp: 0,
            seq: 1,
        });
        maybe_track_runtime_sync_pull_request_outbound(chain_id, local, &outbound);
        let tracked_to =
            get_runtime_sync_pull_target(chain_id, local, remote).expect("target should exist");
        assert_eq!(tracked_to, 1_031);
        clear_runtime_sync_pull_target(chain_id, local, remote);
    }

    #[test]
    fn runtime_sync_pull_headers_prefetch_can_trigger_followup_before_window_tail() {
        let chain_id = 9_896_u64;
        let local = NodeId(972);
        let remote = NodeId(973);
        clear_runtime_sync_pull_target(chain_id, local, remote);

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 3,
                starting_block: 600,
                current_block: 640,
                highest_block: 700,
            },
        );

        let outbound = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: local.0 as u32,
            to: remote.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_runtime_sync_pull_request_payload(
                chain_id,
                NetworkRuntimeNativeSyncPhaseV1::Headers,
                641,
                700,
            ),
            timestamp: 0,
            seq: 1,
        });
        maybe_track_runtime_sync_pull_request_outbound(chain_id, local, &outbound);
        let tracked_to =
            get_runtime_sync_pull_target(chain_id, local, remote).expect("target should exist");
        assert_eq!(tracked_to, 700);

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 3,
                starting_block: 600,
                current_block: 691,
                highest_block: 700,
            },
        );
        let before_prefetch = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: remote.0 as u32,
            to: local.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_block_header_wire_v1(&BlockHeaderWireV1 {
                height: 691,
                epoch_id: 1,
                parent_hash: [0x11u8; 32],
                state_root: [0x22u8; 32],
                governance_chain_audit_root: [0x33u8; 32],
                tx_count: 0,
                batch_count: 0,
                consensus_binding: ConsensusPluginBindingV1 {
                    plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                    adapter_hash: [0x44u8; 32],
                },
            }),
            timestamp: 0,
            seq: 2,
        });
        assert!(
            maybe_build_runtime_sync_pull_followup_request(chain_id, local, &before_prefetch)
                .is_none(),
            "should still wait before prefetch trigger height"
        );

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 3,
                starting_block: 600,
                current_block: 692,
                highest_block: 700,
            },
        );
        let on_prefetch = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: remote.0 as u32,
            to: local.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_block_header_wire_v1(&BlockHeaderWireV1 {
                height: 692,
                epoch_id: 1,
                parent_hash: [0x11u8; 32],
                state_root: [0x22u8; 32],
                governance_chain_audit_root: [0x33u8; 32],
                tx_count: 0,
                batch_count: 0,
                consensus_binding: ConsensusPluginBindingV1 {
                    plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                    adapter_hash: [0x44u8; 32],
                },
            }),
            timestamp: 0,
            seq: 3,
        });
        let (_target, followup) =
            maybe_build_runtime_sync_pull_followup_request(chain_id, local, &on_prefetch)
                .expect("prefetch trigger should generate followup");
        let ProtocolMessage::DistributedOcccGossip(followup_msg) = followup else {
            panic!("followup should be distributed gossip");
        };
        let payload = decode_runtime_sync_pull_request(&followup_msg.payload)
            .expect("followup payload should be NSP1");
        assert_eq!(payload.chain_id, chain_id);
        assert_eq!(payload.from_block, 693);
        assert!(payload.to_block >= payload.from_block);
        clear_runtime_sync_pull_target(chain_id, local, remote);
    }

    #[test]
    fn runtime_sync_pull_followup_preserves_shard_state_channel() {
        let chain_id = 9_894_u64;
        let local = NodeId(960);
        let remote = NodeId(961);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 2,
                starting_block: 50,
                current_block: 60,
                highest_block: 90,
            },
        );
        set_runtime_sync_pull_target(chain_id, local, remote, 60);

        let reply_header = BlockHeaderWireV1 {
            height: 60,
            epoch_id: 1,
            parent_hash: [0x11u8; 32],
            state_root: [0x22u8; 32],
            governance_chain_audit_root: [0x33u8; 32],
            tx_count: 0,
            batch_count: 0,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [0x44u8; 32],
            },
        };
        let shard_reply = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: remote.0 as u32,
            to: local.0 as u32,
            msg_type: DistributedMessageType::ShardState,
            payload: encode_block_header_wire_v1(&reply_header),
            timestamp: 0,
            seq: 1,
        });
        let (_target, followup) =
            maybe_build_runtime_sync_pull_followup_request(chain_id, local, &shard_reply)
                .expect("followup should exist");
        let ProtocolMessage::DistributedOcccGossip(next_msg) = followup else {
            panic!("followup should be distributed gossip");
        };
        assert!(
            matches!(next_msg.msg_type, DistributedMessageType::ShardState),
            "followup should preserve request channel"
        );
        assert!(
            decode_runtime_sync_pull_request(&next_msg.payload).is_some(),
            "followup payload should remain NSP1"
        );
        clear_runtime_sync_pull_target(chain_id, local, remote);
    }

    #[test]
    fn udp_state_sync_pull_request_triggers_block_header_response() {
        let chain_id = 9_889_u64;
        let requester = NodeId(910);
        let responder = NodeId(911);

        let tx = UdpTransport::bind_for_chain(requester, "127.0.0.1:0", chain_id).unwrap();
        let rx = UdpTransport::bind_for_chain(responder, "127.0.0.1:0", chain_id).unwrap();
        let tx_addr = tx.local_addr().unwrap();
        let rx_addr = rx.local_addr().unwrap();
        tx.register_peer(responder, &rx_addr.to_string()).unwrap();
        rx.register_peer(requester, &tx_addr.to_string()).unwrap();

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 2,
                starting_block: 500,
                current_block: 500,
                highest_block: 800,
            },
        );

        let mut pull_payload = Vec::new();
        pull_payload.extend_from_slice(&RUNTIME_SYNC_PULL_REQUEST_MAGIC);
        pull_payload.push(2);
        pull_payload.extend_from_slice(&chain_id.to_le_bytes());
        pull_payload.extend_from_slice(&490u64.to_le_bytes());
        pull_payload.extend_from_slice(&520u64.to_le_bytes());
        let pull_request = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: requester.0 as u32,
            to: responder.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: pull_payload,
            timestamp: 0,
            seq: 1,
        });
        tx.send(responder, pull_request).unwrap();

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(500) {
            if rx.try_recv(responder).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let started_reply = std::time::Instant::now();
        let mut response_heights = Vec::<u64>::new();
        while started_reply.elapsed() < Duration::from_millis(500) {
            if let Some(msg) = tx.try_recv(requester).unwrap() {
                let ProtocolMessage::DistributedOcccGossip(reply) = msg else {
                    continue;
                };
                if !matches!(reply.msg_type, DistributedMessageType::StateSync) {
                    continue;
                }
                if let Ok(header) = decode_block_header_wire_v1(&reply.payload) {
                    response_heights.push(header.height);
                }
            } else if !response_heights.is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert!(
            !response_heights.is_empty(),
            "expected at least one state-sync response"
        );
        assert_eq!(response_heights.first().copied(), Some(490));
        assert_eq!(response_heights.last().copied(), Some(500));
        for pair in response_heights.windows(2) {
            assert_eq!(pair[1], pair[0].saturating_add(1));
        }

        let status = get_network_runtime_sync_status(chain_id).expect("runtime status");
        assert!(status.highest_block >= 520);
    }

    #[test]
    fn udp_state_sync_pull_request_without_local_range_updates_peer_hint_only() {
        let chain_id = 9_891_u64;
        let requester = NodeId(930);
        let responder = NodeId(931);

        let tx = UdpTransport::bind_for_chain(requester, "127.0.0.1:0", chain_id).unwrap();
        let rx = UdpTransport::bind_for_chain(responder, "127.0.0.1:0", chain_id).unwrap();
        let tx_addr = tx.local_addr().unwrap();
        let rx_addr = rx.local_addr().unwrap();
        tx.register_peer(responder, &rx_addr.to_string()).unwrap();
        rx.register_peer(requester, &tx_addr.to_string()).unwrap();

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 2,
                starting_block: 700,
                current_block: 700,
                highest_block: 700,
            },
        );

        let mut pull_payload = Vec::new();
        pull_payload.extend_from_slice(&RUNTIME_SYNC_PULL_REQUEST_MAGIC);
        pull_payload.push(2);
        pull_payload.extend_from_slice(&chain_id.to_le_bytes());
        pull_payload.extend_from_slice(&701u64.to_le_bytes());
        pull_payload.extend_from_slice(&740u64.to_le_bytes());
        let pull_request = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: requester.0 as u32,
            to: responder.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: pull_payload,
            timestamp: 0,
            seq: 1,
        });
        tx.send(responder, pull_request).unwrap();

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(500) {
            if rx.try_recv(responder).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let started_reply = std::time::Instant::now();
        let mut got_reply = false;
        while started_reply.elapsed() < Duration::from_millis(200) {
            if tx.try_recv(requester).unwrap().is_some() {
                got_reply = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !got_reply,
            "should not reply when local head < requested from"
        );

        let status = get_network_runtime_sync_status(chain_id).expect("runtime status");
        assert!(status.highest_block >= 740);
    }

    #[test]
    fn udp_send_updates_runtime_local_progress_from_state_sync_block_header_wire() {
        let chain_id = 9_881_u64;
        let n0 = NodeId(300);
        let n1 = NodeId(301);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();

        let header = BlockHeaderWireV1 {
            height: 321,
            epoch_id: 7,
            parent_hash: [1u8; 32],
            state_root: [2u8; 32],
            governance_chain_audit_root: [3u8; 32],
            tx_count: 5,
            batch_count: 2,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [4u8; 32],
            },
        };
        let state_sync = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: n0.0 as u32,
            to: n1.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_block_header_wire_v1(&header),
            timestamp: 0,
            seq: 1,
        });
        t0.send(n1, state_sync).unwrap();

        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 321);
        assert_eq!(status.highest_block, 321);
    }

    #[test]
    fn tcp_send_updates_runtime_local_progress_from_state_sync_block_header_wire() {
        let chain_id = 9_882_u64;
        let n0 = NodeId(302);
        let n1 = NodeId(303);

        let t0 = TcpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = TcpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a1 = t1.listener.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();

        let header = BlockHeaderWireV1 {
            height: 654,
            epoch_id: 9,
            parent_hash: [1u8; 32],
            state_root: [2u8; 32],
            governance_chain_audit_root: [3u8; 32],
            tx_count: 7,
            batch_count: 3,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [4u8; 32],
            },
        };
        let state_sync = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: n0.0 as u32,
            to: n1.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_block_header_wire_v1(&header),
            timestamp: 0,
            seq: 1,
        });
        t0.send(n1, state_sync).unwrap();

        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 654);
        assert_eq!(status.highest_block, 654);
    }

    #[test]
    fn tcp_try_recv_updates_runtime_progress_from_checkpoint_propose_with_same_ip_hint() {
        let chain_id = 9_878_u64;
        let n0 = NodeId(304);
        let n1 = NodeId(305);

        let t0 = TcpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = TcpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a0 = t0.listener.local_addr().unwrap();
        let a1 = t1.listener.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let msg = ProtocolMessage::Finality(FinalityMessage::CheckpointPropose {
            id: CheckpointId(777),
            from: n0,
            payload: vec![0x01, 0x02, 0x03],
        });
        t0.send(n1, msg).unwrap();

        let started = std::time::Instant::now();
        let mut got = None;
        while started.elapsed() < Duration::from_millis(500) {
            if let Some(m) = t1.try_recv(n1).unwrap() {
                got = Some(m);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            got,
            Some(ProtocolMessage::Finality(
                FinalityMessage::CheckpointPropose { .. }
            ))
        ));

        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.highest_block, 777);
        assert!(status.peer_count >= 1);
    }

    #[test]
    fn infer_peer_id_from_src_addr_prefers_exact_and_unique_same_ip() {
        let peers = DashMap::new();
        peers.insert(NodeId(1), "127.0.0.1:12001".parse().expect("addr node1"));

        let exact =
            infer_peer_id_from_src_addr(&peers, "127.0.0.1:12001".parse().expect("src exact"));
        assert_eq!(exact, Some(1));

        let unique_same_ip =
            infer_peer_id_from_src_addr(&peers, "127.0.0.1:55000".parse().expect("src same ip"));
        assert_eq!(unique_same_ip, Some(1));

        peers.insert(NodeId(2), "127.0.0.1:12002".parse().expect("addr node2"));
        let ambiguous_same_ip =
            infer_peer_id_from_src_addr(&peers, "127.0.0.1:56000".parse().expect("src ambiguous"));
        assert_eq!(ambiguous_same_ip, None);
    }

    #[test]
    fn udp_try_recv_updates_runtime_progress_from_finality_vote() {
        let chain_id = 9_999_u64;
        let n0 = NodeId(240);
        let n1 = NodeId(241);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let msg = ProtocolMessage::Finality(FinalityMessage::Vote {
            id: CheckpointId(55),
            from: n0,
            sig: vec![1u8; 64],
        });
        t0.send(n1, msg).unwrap();

        let started = std::time::Instant::now();
        let mut got = None;
        while started.elapsed() < Duration::from_millis(500) {
            if let Some(m) = t1.try_recv(n1).unwrap() {
                got = Some(m);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            got,
            Some(ProtocolMessage::Finality(FinalityMessage::Vote { .. }))
        ));
        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 55);
        assert_eq!(status.highest_block, 55);
        assert_eq!(status.starting_block, 55);
    }

    #[test]
    fn udp_try_recv_registers_runtime_peers_from_peerlist_payload() {
        let chain_id = 5_555u64;
        let n0 = NodeId(10);
        let n1 = NodeId(11);
        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let msg = ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList {
            from: n0,
            peers: vec![NodeId(12), NodeId(13)],
        });
        t0.send(n1, msg).unwrap();
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(500) {
            if t1.try_recv(n1).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let status = get_network_runtime_sync_status(chain_id).expect("runtime sync status");
        assert!(
            status.peer_count >= 3,
            "peer_count should include sender + peerlist payload peers"
        );
    }

    #[test]
    fn udp_transport_mesh_three_nodes_closure() {
        let n0 = NodeId(0);
        let n1 = NodeId(1);
        let n2 = NodeId(2);
        let t0 = UdpTransport::bind(n0, "127.0.0.1:0").unwrap();
        let t1 = UdpTransport::bind(n1, "127.0.0.1:0").unwrap();
        let t2 = UdpTransport::bind(n2, "127.0.0.1:0").unwrap();

        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        let a2 = t2.local_addr().unwrap();

        t0.register_peer(n1, &a1.to_string()).unwrap();
        t0.register_peer(n2, &a2.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();
        t1.register_peer(n2, &a2.to_string()).unwrap();
        t2.register_peer(n0, &a0.to_string()).unwrap();
        t2.register_peer(n1, &a1.to_string()).unwrap();

        let send_triplet =
            |from: NodeId, to: NodeId, transport: &UdpTransport, peers: Vec<NodeId>| {
                transport
                    .send(
                        to,
                        ProtocolMessage::Gossip(GossipMessage::PeerList { from, peers }),
                    )
                    .unwrap();
                transport
                    .send(
                        to,
                        ProtocolMessage::Gossip(GossipMessage::Heartbeat {
                            from,
                            shard: ShardId((from.0 as u32).saturating_add(1)),
                        }),
                    )
                    .unwrap();
                transport
                    .send(
                        to,
                        ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                            from: from.0 as u32,
                            to: to.0 as u32,
                            msg_type: DistributedMessageType::StateSync,
                            payload: vec![from.0 as u8, to.0 as u8],
                            timestamp: 0,
                            seq: from.0,
                        }),
                    )
                    .unwrap();
            };

        send_triplet(n0, n1, &t0, vec![n1, n2]);
        send_triplet(n0, n2, &t0, vec![n1, n2]);
        send_triplet(n1, n0, &t1, vec![n0, n2]);
        send_triplet(n1, n2, &t1, vec![n0, n2]);
        send_triplet(n2, n0, &t2, vec![n0, n1]);
        send_triplet(n2, n1, &t2, vec![n0, n1]);

        let mut d0: HashSet<u64> = HashSet::new();
        let mut g0: HashSet<u64> = HashSet::new();
        let mut s0: HashSet<u64> = HashSet::new();
        let mut d1: HashSet<u64> = HashSet::new();
        let mut g1: HashSet<u64> = HashSet::new();
        let mut s1: HashSet<u64> = HashSet::new();
        let mut d2: HashSet<u64> = HashSet::new();
        let mut g2: HashSet<u64> = HashSet::new();
        let mut s2: HashSet<u64> = HashSet::new();

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(1_500) {
            while let Some(msg) = t0.try_recv(n0).unwrap() {
                match msg {
                    ProtocolMessage::Gossip(GossipMessage::PeerList { from, .. }) => {
                        d0.insert(from.0);
                    }
                    ProtocolMessage::Gossip(GossipMessage::Heartbeat { from, .. }) => {
                        g0.insert(from.0);
                    }
                    ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                        from,
                        msg_type: DistributedMessageType::StateSync,
                        ..
                    }) => {
                        s0.insert(from as u64);
                    }
                    _ => {}
                }
            }
            while let Some(msg) = t1.try_recv(n1).unwrap() {
                match msg {
                    ProtocolMessage::Gossip(GossipMessage::PeerList { from, .. }) => {
                        d1.insert(from.0);
                    }
                    ProtocolMessage::Gossip(GossipMessage::Heartbeat { from, .. }) => {
                        g1.insert(from.0);
                    }
                    ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                        from,
                        msg_type: DistributedMessageType::StateSync,
                        ..
                    }) => {
                        s1.insert(from as u64);
                    }
                    _ => {}
                }
            }
            while let Some(msg) = t2.try_recv(n2).unwrap() {
                match msg {
                    ProtocolMessage::Gossip(GossipMessage::PeerList { from, .. }) => {
                        d2.insert(from.0);
                    }
                    ProtocolMessage::Gossip(GossipMessage::Heartbeat { from, .. }) => {
                        g2.insert(from.0);
                    }
                    ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                        from,
                        msg_type: DistributedMessageType::StateSync,
                        ..
                    }) => {
                        s2.insert(from as u64);
                    }
                    _ => {}
                }
            }

            let ok0 = d0.len() == 2 && g0.len() == 2 && s0.len() == 2;
            let ok1 = d1.len() == 2 && g1.len() == 2 && s1.len() == 2;
            let ok2 = d2.len() == 2 && g2.len() == 2 && s2.len() == 2;
            if ok0 && ok1 && ok2 {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert_eq!(d0.len(), 2, "node0 discovery set: {d0:?}");
        assert_eq!(g0.len(), 2, "node0 gossip set: {g0:?}");
        assert_eq!(s0.len(), 2, "node0 sync set: {s0:?}");
        assert_eq!(d1.len(), 2, "node1 discovery set: {d1:?}");
        assert_eq!(g1.len(), 2, "node1 gossip set: {g1:?}");
        assert_eq!(s1.len(), 2, "node1 sync set: {s1:?}");
        assert_eq!(d2.len(), 2, "node2 discovery set: {d2:?}");
        assert_eq!(g2.len(), 2, "node2 gossip set: {g2:?}");
        assert_eq!(s2.len(), 2, "node2 sync set: {s2:?}");
    }

    #[test]
    fn evm_native_get_block_headers_response_uses_runtime_native_snapshot() {
        let chain_id = 9_910_u64;
        let local = NodeId(991);
        let remote = NodeId(992);

        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 77,
                hash: [0xa1; 32],
                parent_hash: [0xa0; 32],
                state_root: [0xb1; 32],
                transactions_root: [0xb2; 32],
                receipts_root: [0xb3; 32],
                ommers_hash: [0xb4; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(42_000),
                timestamp: Some(17),
                base_fee_per_gas: Some(9),
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(remote.0),
                observed_unix_ms: 10,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Headers,
                peer_count: 1,
                block_number: 77,
                block_hash: [0xa1; 32],
                parent_block_hash: [0xa0; 32],
                state_root: [0xb1; 32],
                canonical: false,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: false,
                source_peer_id: Some(remote.0),
                observed_unix_ms: 11,
            },
        );

        let request = ProtocolMessage::EvmNative(EvmNativeMessage::GetBlockHeaders {
            from: remote,
            start_height: 77,
            max: 4,
            skip: 0,
            reverse: false,
        });
        let (to, response) =
            maybe_build_evm_native_sync_response(chain_id, local, &request).expect("response");
        assert_eq!(to, remote);

        let ProtocolMessage::EvmNative(EvmNativeMessage::BlockHeaders { from, headers }) = response
        else {
            panic!("expected native block headers response");
        };
        assert_eq!(from, local);
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].number, 77);
        assert_eq!(headers[0].hash, [0xa1; 32]);
        assert_eq!(headers[0].parent_hash, [0xa0; 32]);
    }

    #[test]
    fn evm_native_block_headers_and_bodies_ingest_runtime_native_snapshots() {
        let chain_id = 9_911_u64;
        let remote = NodeId(993);

        let header_msg = ProtocolMessage::EvmNative(EvmNativeMessage::BlockHeaders {
            from: remote,
            headers: vec![EvmNativeBlockHeaderWireV1 {
                number: 88,
                hash: [0xc1; 32],
                parent_hash: [0xc0; 32],
                state_root: [0xd1; 32],
                transactions_root: [0xd2; 32],
                receipts_root: [0xd3; 32],
                ommers_hash: [0xd4; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(21_000),
                timestamp: Some(20),
                base_fee_per_gas: Some(7),
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
            }],
        });
        let header_ctx = runtime_sync_pull_message_context(&header_msg);
        maybe_update_runtime_sync_from_protocol_message_with_context(
            chain_id,
            &header_msg,
            None,
            None,
            &header_ctx,
        );

        let header = get_network_runtime_native_header_snapshot_v1(chain_id).expect("header");
        let head = get_network_runtime_native_head_snapshot_v1(chain_id).expect("head");
        let runtime = get_network_runtime_sync_status(chain_id).expect("runtime");
        assert_eq!(header.number, 88);
        assert_eq!(header.hash, [0xc1; 32]);
        assert_eq!(head.block_number, 88);
        assert_eq!(head.block_hash, [0xc1; 32]);
        assert!(!head.body_available);
        assert_eq!(runtime.current_block, 88);
        assert_eq!(runtime.highest_block, 88);

        let body_msg = ProtocolMessage::EvmNative(EvmNativeMessage::BlockBodies {
            from: remote,
            bodies: vec![EvmNativeBlockBodyWireV1 {
                number: 88,
                block_hash: [0xc1; 32],
                tx_hashes: vec![[0xe1; 32], [0xe2; 32]],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: vec![[0xf1; 32]],
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
            }],
        });
        let body_ctx = runtime_sync_pull_message_context(&body_msg);
        maybe_update_runtime_sync_from_protocol_message_with_context(
            chain_id, &body_msg, None, None, &body_ctx,
        );

        let body = get_network_runtime_native_body_snapshot_v1(chain_id).expect("body");
        let head = get_network_runtime_native_head_snapshot_v1(chain_id).expect("head after body");
        assert_eq!(body.number, 88);
        assert_eq!(body.block_hash, [0xc1; 32]);
        assert_eq!(body.tx_hashes.len(), 2);
        assert!(head.body_available);
        assert_eq!(head.block_number, 88);
        assert_eq!(head.block_hash, [0xc1; 32]);
    }

    #[test]
    fn bootstrap_eth_fullnode_native_peer_emits_proven_sequence() {
        let chain_id = 9_912_u64;
        let local = NodeId(994);
        let peer = NodeId(995);
        let transport = InMemoryTransport::new(8);
        transport.register(local);
        transport.register(peer);

        bootstrap_eth_fullnode_native_peer_v1(&transport, local, peer, chain_id)
            .expect("bootstrap sequence");

        let msg0 = transport.try_recv(peer).expect("recv0").expect("msg0");
        let msg1 = transport.try_recv(peer).expect("recv1").expect("msg1");
        let msg2 = transport.try_recv(peer).expect("recv2").expect("msg2");
        let msg3 = transport.try_recv(peer).expect("recv3").expect("msg3");

        assert!(matches!(
            msg0,
            ProtocolMessage::EvmNative(EvmNativeMessage::DiscoveryPing { from, chain_id: c, .. })
                if from == local && c == chain_id
        ));
        assert!(matches!(
            msg1,
            ProtocolMessage::EvmNative(EvmNativeMessage::RlpxAuth { from, chain_id: c, network_id, .. })
                if from == local && c == chain_id && network_id == chain_id
        ));
        assert!(matches!(
            msg2,
            ProtocolMessage::EvmNative(EvmNativeMessage::Hello { from, chain_id: c, network_id, .. })
                if from == local && c == chain_id && network_id == chain_id
        ));
        assert!(matches!(
            msg3,
            ProtocolMessage::EvmNative(EvmNativeMessage::Status { from, chain_id: c, .. })
                if from == local && c == chain_id
        ));
    }

    #[test]
    fn native_peer_worker_plan_is_multi_peer_but_budget_bounded() {
        let chain_id = 9_914_u64;
        let local = NodeId(1_100);
        let peers = vec![NodeId(1_101), NodeId(1_102), NodeId(1_103), NodeId(1_104)];
        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 2;
        budget.active_native_peer_hard_limit = 3;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers,
            peer_endpoints: Vec::new(),
            recv_budget: 4,
            sync_target_fanout: 2,
            budget_hooks: budget,
        });

        let plan = worker.plan();
        assert_eq!(plan.candidate_peers.len(), 3);
        assert_eq!(plan.bootstrap_peers.len(), 2);
        assert!(plan.sync_peers.is_empty());
    }

    #[test]
    fn native_peer_worker_plan_caps_public_bootstrap_fanout_per_tick() {
        let chain_id = 9_914_000_201_u64;
        let local = NodeId(1_100_201);
        let peers = (1_100_202..1_100_210).map(NodeId).collect::<Vec<_>>();
        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 8;
        budget.active_native_peer_hard_limit = 8;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers,
            peer_endpoints: Vec::new(),
            recv_budget: 16,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let plan = worker.plan();
        assert_eq!(plan.bootstrap_peers.len(), 4);
        assert!(plan.sync_peers.is_empty());
    }

    #[test]
    fn native_peer_worker_plan_skips_cooldown_and_permanent_rejects() {
        let chain_id = 9_914_001_u64;
        let local = NodeId(1_105);
        let peer_a = NodeId(1_106);
        let peer_b = NodeId(1_107);
        let peer_c = NodeId(1_108);

        let _ = upsert_network_runtime_eth_peer_session(chain_id, peer_a.0, &[69, 70], &[1], None)
            .expect("hello-only peer");
        observe_network_runtime_eth_peer_disconnect_v1(chain_id, peer_a.0, Some(0x04));
        observe_network_runtime_eth_peer_validation_reject_v1(
            chain_id,
            peer_b.0,
            EthChainConfigPeerValidationReasonV1::WrongNetwork,
        );

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 3;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![peer_a, peer_b, peer_c],
            peer_endpoints: Vec::new(),
            recv_budget: 2,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let plan = worker.plan();
        assert_eq!(plan.bootstrap_peers, vec![peer_c]);
        assert!(plan.sync_peers.is_empty());
        assert_eq!(plan.lifecycle_summary.cooldown_count, 1);
        assert_eq!(plan.lifecycle_summary.permanently_rejected_count, 1);
        assert_eq!(plan.lifecycle_summary.retry_eligible_count, 1);
    }

    #[test]
    fn real_rlpx_connect_failure_updates_lifecycle_backoff_state() {
        let chain_id = 9_914_002_u64;
        let local = NodeId(1_109);
        let peer = NodeId(1_110);
        let endpoint = PluginPeerEndpoint {
            endpoint: "enode://00@127.0.0.1:30303".to_string(),
            node_hint: peer.0,
            addr_hint: "not-a-real-socket".to_string(),
        };
        let err = connect_eth_fullnode_native_rlpx_peer_v1(chain_id, local, peer, &endpoint)
            .expect_err("invalid addr must fail");
        assert!(matches!(err, NetworkError::AddressParse(_)));
        let snapshot =
            snapshot_network_runtime_eth_peer_sessions_for_peers_v1(chain_id, &[peer])[0].clone();
        assert_eq!(
            snapshot.lifecycle_stage,
            crate::EthPeerLifecycleStageV1::PermanentlyRejected
        );
        assert_eq!(
            snapshot.last_failure_class,
            Some(crate::EthPeerFailureClassV1::ConnectFailure)
        );
        assert_eq!(
            snapshot.last_failure_reason_name.as_deref(),
            Some("address_parse")
        );
        assert!(!snapshot.retry_eligible);
    }

    #[test]
    fn rlpx_connect_decode_wrapped_timeout_records_timeout_lifecycle_state() {
        let chain_id = 9_914_002_100_u64;
        let peer = NodeId(1_110_100);
        let err = NetworkError::Decode(
            "rlpx_ack_prefix_read_failed: connection attempt failed (os error 10060) read=0/2"
                .to_string(),
        );
        assert_eq!(
            classify_eth_fullnode_peer_failure_v1(&err),
            EthFullnodeNativePeerFailureClassV1::Timeout
        );

        observe_eth_fullnode_connect_error_v1(chain_id, peer.0, &err);

        let snapshot =
            snapshot_network_runtime_eth_peer_sessions_for_peers_v1(chain_id, &[peer])[0].clone();
        assert_eq!(
            snapshot.last_failure_class,
            Some(crate::EthPeerFailureClassV1::Timeout)
        );
        assert_eq!(
            snapshot.last_failure_reason_name.as_deref(),
            Some("connect_timeout")
        );
        assert_eq!(snapshot.timeout_count, 1);
        assert_eq!(snapshot.decode_failure_count, 0);
    }

    #[test]
    fn real_rlpx_worker_keeps_running_other_peers_when_one_bootstrap_fails() {
        let chain_id = 9_914_003_u64;
        let local = NodeId(1_111);
        let bad_peer = NodeId(1_112);
        let good_peer = NodeId(1_113);

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");

        let bad_endpoint = PluginPeerEndpoint {
            endpoint: "enode://00@127.0.0.1:30303".to_string(),
            node_hint: bad_peer.0,
            addr_hint: "not-a-real-socket".to_string(),
        };
        let good_endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: good_peer.0,
            addr_hint: listen_addr.to_string(),
        };

        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/best-effort-test",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    0,
                    0,
                ),
                earliest_block: 0,
                latest_block: 64,
                latest_block_hash: [0x64; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, _) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            thread::sleep(Duration::from_millis(300));
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 2;
        budget.active_native_peer_hard_limit = 2;
        budget.sync_request_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![bad_peer, good_peer],
            peer_endpoints: vec![bad_endpoint, good_endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report = worker
            .drive_real_network_once()
            .expect("best effort bootstrap tick");
        assert_eq!(report.scheduled_bootstrap_peers, 2);
        assert_eq!(report.attempted_bootstrap_peers, 2);
        assert_eq!(report.connected_peers, 1);
        assert_eq!(report.failed_bootstrap_peers, 1);
        let bootstrap_failures = report
            .peer_failures
            .iter()
            .filter(|failure| failure.phase == EthFullnodeNativePeerDrivePhaseV1::Bootstrap)
            .collect::<Vec<_>>();
        assert_eq!(bootstrap_failures.len(), 1);
        assert_eq!(bootstrap_failures[0].peer_id, bad_peer.0);
        assert_eq!(
            bootstrap_failures[0].phase,
            EthFullnodeNativePeerDrivePhaseV1::Bootstrap
        );
        assert_eq!(
            bootstrap_failures[0].class,
            EthFullnodeNativePeerFailureClassV1::AddressParse
        );
        assert_eq!(report.lifecycle_summary.permanently_rejected_count, 1);
        assert!(report.lifecycle_summary.ready_count >= 1);

        server.join().expect("server join");
    }

    #[test]
    fn real_rlpx_parallel_bootstrap_bounds_slow_connects_v1() {
        let _guard = eth_rlpx_env_test_lock_v1()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _connect_timeout = set_test_env_var_v1("NOVOVM_ETH_RLPX_CONNECT_TIMEOUT_MS", "250");
        let _tick_budget = set_test_env_var_v1("NOVOVM_ETH_RLPX_BOOTSTRAP_TICK_BUDGET_MS", "1000");

        let chain_id = 99_160_332_u64;
        let local = NodeId(555_100);
        let mut listeners = Vec::new();
        let mut endpoints = Vec::new();
        let mut peers = Vec::new();
        for idx in 0..6_u64 {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind stalled peer");
            let listen_addr = listener.local_addr().expect("listener addr");
            let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
            let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
            let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
                .expect("derive stalled pubkey");
            let peer = NodeId(555_101 + idx);
            peers.push(peer);
            endpoints.push(PluginPeerEndpoint {
                endpoint: format!(
                    "enode://{}@{}",
                    responder_pub
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>(),
                    listen_addr
                ),
                node_hint: peer.0,
                addr_hint: listen_addr.to_string(),
            });
            listeners.push(listener);
        }

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 6;
        budget.active_native_peer_hard_limit = 6;
        budget.sync_target_fanout = 6;
        budget.sync_request_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers,
            peer_endpoints: endpoints,
            recv_budget: 1,
            sync_target_fanout: 6,
            budget_hooks: budget,
        });

        let started = Instant::now();
        let report = worker
            .drive_real_network_once()
            .expect("bootstrap tick should be budgeted");
        assert_eq!(report.scheduled_bootstrap_peers, 6);
        assert_eq!(
            report.attempted_bootstrap_peers, report.scheduled_bootstrap_peers,
            "parallel bootstrap should attempt the selected public fanout in one tick"
        );
        assert_eq!(report.skipped_bootstrap_budget_peers, 0);
        assert_eq!(
            report.attempted_bootstrap_peers
                + report.skipped_bootstrap_budget_peers
                + report.skipped_missing_endpoint_peers,
            report.scheduled_bootstrap_peers
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "bootstrap tick should stay bounded even with stalled local peers"
        );
        drop(listeners);
    }

    #[test]
    fn real_rlpx_sync_peers_are_bounded_by_tick_budget_v1() {
        let _guard = eth_rlpx_env_test_lock_v1()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _connect_timeout = set_test_env_var_v1("NOVOVM_ETH_RLPX_CONNECT_TIMEOUT_MS", "250");
        let _tick_budget = set_test_env_var_v1("NOVOVM_ETH_RLPX_BOOTSTRAP_TICK_BUDGET_MS", "1000");

        let chain_id = 99_160_333_u64;
        let local = NodeId(555_200);
        let mut listeners = Vec::new();
        let mut accepted_streams = Vec::new();
        let mut endpoints = Vec::new();
        let mut peers = Vec::new();
        for idx in 0..40_u64 {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind stalled sync peer");
            let listen_addr = listener.local_addr().expect("listener addr");
            let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
            let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
            let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
                .expect("derive stalled pubkey");
            let peer = NodeId(555_201 + idx);
            let _ = upsert_network_runtime_eth_peer_session(
                chain_id,
                peer.0,
                &[69, 70],
                &[1],
                Some(64),
            );
            let (mut live_session, accepted, _peer_frame_session) =
                dummy_rlpx_live_session_pair(chain_id);
            live_session.endpoint.node_hint = peer.0;
            eth_fullnode_native_rlpx_sessions_v1()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert((chain_id, peer.0), live_session);
            accepted_streams.push(accepted);
            peers.push(peer);
            endpoints.push(PluginPeerEndpoint {
                endpoint: format!(
                    "enode://{}@{}",
                    responder_pub
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>(),
                    listen_addr
                ),
                node_hint: peer.0,
                addr_hint: listen_addr.to_string(),
            });
            listeners.push(listener);
        }

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 40;
        budget.active_native_peer_hard_limit = 40;
        budget.sync_target_fanout = 40;
        budget.sync_request_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers,
            peer_endpoints: endpoints,
            recv_budget: 1,
            sync_target_fanout: 40,
            budget_hooks: budget,
        });

        let started = Instant::now();
        let report = worker
            .drive_real_network_once()
            .expect("sync tick should be budgeted");
        assert_eq!(report.scheduled_bootstrap_peers, 0);
        assert_eq!(report.scheduled_sync_peers, 40);
        assert!(report.attempted_sync_peers > 0);
        assert!(
            report.attempted_sync_peers < report.scheduled_sync_peers,
            "sync tick should stop before serially exhausting every stalled peer"
        );
        assert!(report.skipped_sync_budget_peers > 0);
        assert_eq!(
            report.attempted_sync_peers
                + report.skipped_sync_budget_peers
                + report.skipped_missing_endpoint_peers,
            report.scheduled_sync_peers
        );
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "sync tick should stay bounded under stalled ready peers"
        );
        drop(accepted_streams);
        drop(listeners);
    }

    #[test]
    fn native_peer_worker_prefers_highest_head_session_for_sync() {
        let chain_id = 9_915_u64;
        let local = NodeId(1_120);
        let peer_a = NodeId(1_121);
        let peer_b = NodeId(1_122);
        let _ =
            upsert_network_runtime_eth_peer_session(chain_id, peer_a.0, &[69, 70], &[1], Some(120))
                .expect("session a");
        let _ =
            upsert_network_runtime_eth_peer_session(chain_id, peer_b.0, &[69, 70], &[1], Some(240))
                .expect("session b");
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 2,
                starting_block: 100,
                current_block: 100,
                highest_block: 140,
            },
        );

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 2;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![peer_a, peer_b],
            peer_endpoints: Vec::new(),
            recv_budget: 2,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });
        let plan = worker.plan();
        assert_eq!(plan.bootstrap_peers, Vec::<NodeId>::new());
        assert_eq!(plan.sync_peers, vec![peer_b]);
    }

    #[test]
    fn udp_eth_fullnode_native_peer_drive_runs_bootstrap_and_dispatches_header_sync() {
        let chain_id = 9_916_u64;
        let local = NodeId(1_010);
        let remote = NodeId(1_011);
        let t0 = UdpTransport::bind_for_chain(local, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(remote, "127.0.0.1:0", chain_id).unwrap();
        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(remote, &a1.to_string()).unwrap();
        t1.register_peer(local, &a0.to_string()).unwrap();

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 56,
                current_block: 56,
                highest_block: 72,
            },
        );
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 72,
                hash: [0x91; 32],
                parent_hash: [0x90; 32],
                state_root: [0x81; 32],
                transactions_root: [0x82; 32],
                receipts_root: [0x83; 32],
                ommers_hash: [0x84; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(84_000),
                timestamp: Some(33),
                base_fee_per_gas: Some(15),
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(remote.0),
                observed_unix_ms: 1,
            },
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 72,
                block_hash: [0x91; 32],
                tx_hashes: vec![[0xa1; 32], [0xa2; 32]],
                raw_tx_rlps: Vec::new(),
                ommer_hashes: vec![[0xb1; 32]],
                withdrawal_rlp_items: None,
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 2,
            },
        );

        let first = drive_eth_fullnode_native_peer_once_v1(&t0, local, remote, chain_id, 0)
            .expect("bootstrap round");
        assert_eq!(first.bootstrapped_peers, 1);
        assert_eq!(first.sync_requested_peers, 0);
        assert_eq!(first.outbound_messages, 4);

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(750) {
            let _ = drive_eth_fullnode_native_peer_once_v1(&t1, remote, local, chain_id, 8)
                .expect("remote round");
            let _ = drive_eth_fullnode_native_peer_once_v1(&t0, local, remote, chain_id, 8)
                .expect("local round");

            let evidence = snapshot_eth_native_sync_evidence(chain_id);
            let sessions = snapshot_network_runtime_eth_peer_sessions(chain_id);
            if evidence.discovery_seen
                && evidence.rlpx_auth_seen
                && evidence.rlpx_auth_ack_seen
                && evidence.hello_seen
                && evidence.status_seen
                && sessions.iter().any(|session| session.peer_id == remote.0)
            {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let evidence = snapshot_eth_native_sync_evidence(chain_id);
        assert!(evidence.discovery_seen);
        assert!(evidence.rlpx_auth_seen);
        assert!(evidence.rlpx_auth_ack_seen);
        assert!(evidence.hello_seen);
        assert!(evidence.status_seen);

        let sessions = snapshot_network_runtime_eth_peer_sessions(chain_id);
        assert!(sessions.iter().any(|session| session.peer_id == remote.0));

        let progress = current_eth_native_parity_progress_for_chain(chain_id);
        assert!(progress.native_eth_handshake);

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 40,
                current_block: 40,
                highest_block: 72,
            },
        );
        let sync_round = drive_eth_fullnode_native_peer_once_v1(&t0, local, remote, chain_id, 0)
            .expect("sync round");
        assert_eq!(sync_round.bootstrapped_peers, 0);
        assert_eq!(sync_round.sync_requested_peers, 1);
        assert_eq!(sync_round.outbound_messages, 1);

        let remote_sync = drive_eth_fullnode_native_peer_once_v1(&t1, remote, local, chain_id, 8)
            .expect("remote sync round");
        let local_sync = drive_eth_fullnode_native_peer_once_v1(&t0, local, remote, chain_id, 8)
            .expect("local sync round");
        assert!(remote_sync.inbound_messages > 0);
        assert!(local_sync.inbound_messages > 0);

        let evidence = snapshot_eth_native_sync_evidence(chain_id);
        assert!(evidence.headers_pull_seen);
        assert!(evidence.headers_response_seen);
    }

    #[test]
    fn real_rlpx_peer_worker_pivots_to_status_head_by_hash_v1() {
        let chain_id = 99_170_021_u64;
        let local = NodeId(1_214);
        let remote = NodeId(1_215);
        let status_head_hash = [0xab; 32];
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 120,
                current_block: 120,
                highest_block: 50_000,
            },
        );

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        let (request_tx, request_rx) = std::sync::mpsc::channel();
        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/status-head-pivot-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    0,
                    0,
                ),
                earliest_block: 1,
                latest_block: 50_000,
                latest_block_hash: status_head_hash,
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);

            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read status head pivot worker frame");
                if code
                    == crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG
                {
                    let request =
                        crate::eth_rlpx_parse_get_block_headers_payload_v1(payload.as_slice())
                            .expect("parse get block headers");
                    request_tx.send(request).expect("send observed request");
                    break;
                }
            }
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        assert_eq!(report0.sync_requests, 1);
        let request = request_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("status head pivot request");
        assert_eq!(request.origin_hash, Some(status_head_hash));
        assert_eq!(request.max_headers, 1);
        assert_eq!(request.skip, 0);
        assert!(!request.reverse);

        server.join().expect("server join");
    }

    #[test]
    fn real_rlpx_peer_worker_ingests_runtime_native_snapshots() {
        let chain_id = 9_917_u64;
        let local = NodeId(1_210);
        let remote = NodeId(1_211);
        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 119,
                current_block: 119,
                highest_block: 120,
            },
        );
        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/transport-test",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: [0u8; 32],
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    0,
                    0,
                ),
                earliest_block: 1,
                latest_block: 120,
                latest_block_hash: [0x42; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            let expected_local_status = crate::build_eth_fullnode_native_rlpx_status_v1(
                chain_id,
                crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
            );
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(
                peer_status.protocol_version,
                crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32
            );
            assert_eq!(peer_status.latest_block, expected_local_status.latest_block);
            assert_eq!(
                peer_status.genesis_hash, expected_local_status.genesis_hash,
                "local Status genesis must come from local chain facts"
            );
            assert_eq!(
                peer_status.latest_block_hash,
                expected_local_status.latest_block_hash
            );
            assert_eq!(peer_status.fork_id, expected_local_status.fork_id);

            let header_record = crate::EthRlpxBlockHeaderRecordV1 {
                number: 120,
                hash: [0u8; 32],
                parent_hash: [0x10; 32],
                state_root: [0x20; 32],
                transactions_root: [0x30; 32],
                receipts_root: [0x40; 32],
                ommers_hash: [0x50; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(100_000),
                timestamp: Some(1_234_567),
                base_fee_per_gas: Some(15),
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                raw_rlp: None,
            };
            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read worker frame");
                if code
                    == crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG
                {
                    let request =
                        crate::eth_rlpx_parse_get_block_headers_payload_v1(payload.as_slice())
                            .expect("parse get block headers");
                    let headers_payload = crate::eth_rlpx_build_block_headers_payload_v1(
                        request.request_id,
                        std::slice::from_ref(&header_record),
                    );
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                            + crate::ETH_RLPX_ETH_BLOCK_HEADERS_MSG,
                        headers_payload.as_slice(),
                    )
                    .expect("write block headers");
                    thread::sleep(Duration::from_millis(500));
                    break;
                }
            }
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        assert_eq!(report0.sync_requests, 1);
        let status_after_connect = get_network_runtime_sync_status(chain_id).expect("sync status");
        assert_eq!(status_after_connect.highest_block, 120);

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(5)
            && get_network_runtime_native_header_snapshot_v1(chain_id).is_none()
        {
            let _ = worker
                .drive_real_network_once()
                .expect("header response tick");
            thread::sleep(Duration::from_millis(5));
        }

        let header_snapshot =
            get_network_runtime_native_header_snapshot_v1(chain_id).expect("header snapshot");
        assert_eq!(header_snapshot.number, 120);
        let head_snapshot =
            get_network_runtime_native_head_snapshot_v1(chain_id).expect("head snapshot");
        assert_eq!(head_snapshot.block_number, 120);

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_block_body_import_gate_v3() {
        let chain_id = 9_921_u64;
        let local = NodeId(1_250);
        let remote = NodeId(1_251);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let raw_tx = crate::eth_rlpx_decode_hex_v1(
            "f86c098504a817c800825208943535353535353535353535353535353535353535880de0b6b3a76400008025a028ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276a067cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83",
        )
        .expect("decode raw transaction");
        let tx_hash = crate::eth_rlpx_transaction_hash_v1(raw_tx.as_slice());
        let status_head_hash = [0x77; 32];
        let empty_root = crate::eth_rlpx_empty_trie_root_v1();
        let empty_ommers_hash = crate::eth_rlpx_empty_ommers_hash_v1();
        let receipt_blocks = vec![vec![vec![0xc0]]];
        let header_record_1 = crate::EthRlpxBlockHeaderRecordV1 {
            number: 120,
            hash: [0u8; 32],
            parent_hash: [0x10; 32],
            state_root: [0x20; 32],
            transactions_root: crate::eth_rlpx_transactions_root_from_raw_txs_v1(&[raw_tx.clone()]),
            receipts_root: crate::eth_rlpx_receipts_root_from_raw_receipts_v1(&receipt_blocks[0]),
            ommers_hash: empty_ommers_hash,
            logs_bloom: vec![0u8; 256],
            gas_limit: Some(30_000_000),
            gas_used: Some(21_000),
            timestamp: Some(1_234_567),
            base_fee_per_gas: Some(15),
            withdrawals_root: Some(empty_root),
            blob_gas_used: None,
            excess_blob_gas: None,
            block_access_list_hash: None,
            raw_rlp: None,
        };
        let header_hash = crate::eth_rlpx_parse_block_headers_payload_v1(
            crate::eth_rlpx_build_block_headers_payload_v1(
                1,
                std::slice::from_ref(&header_record_1),
            )
            .as_slice(),
        )
        .expect("derive wire header hash")
        .headers[0]
            .hash;
        let header_record_2 = crate::EthRlpxBlockHeaderRecordV1 {
            number: 121,
            hash: [0u8; 32],
            parent_hash: header_hash,
            state_root: [0x20; 32],
            transactions_root: empty_root,
            receipts_root: empty_root,
            ommers_hash: empty_ommers_hash,
            logs_bloom: vec![0u8; 256],
            gas_limit: Some(30_000_000),
            gas_used: Some(0),
            timestamp: Some(1_234_579),
            base_fee_per_gas: Some(15),
            withdrawals_root: Some(empty_root),
            blob_gas_used: None,
            excess_blob_gas: None,
            block_access_list_hash: None,
            raw_rlp: None,
        };
        let header_hash_2 = crate::eth_rlpx_parse_block_headers_payload_v1(
            crate::eth_rlpx_build_block_headers_payload_v1(
                1,
                std::slice::from_ref(&header_record_2),
            )
            .as_slice(),
        )
        .expect("derive second wire header hash")
        .headers[0]
            .hash;
        let server_header_records = vec![header_record_1.clone(), header_record_2.clone()];

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 119,
                current_block: 119,
                highest_block: 121,
            },
        );

        let expected_raw_tx = raw_tx.clone();
        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/block-body-import-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    0,
                    0,
                ),
                earliest_block: 1,
                latest_block: 121,
                latest_block_hash: status_head_hash,
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(
                peer_status.protocol_version,
                crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32
            );

            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read worker frame");
                if code
                    == crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG
                {
                    let request =
                        crate::eth_rlpx_parse_get_block_headers_payload_v1(payload.as_slice())
                            .expect("parse get block headers");
                    let headers_payload = crate::eth_rlpx_build_block_headers_payload_v1(
                        request.request_id,
                        server_header_records.as_slice(),
                    );
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                            + crate::ETH_RLPX_ETH_BLOCK_HEADERS_MSG,
                        headers_payload.as_slice(),
                    )
                    .expect("write block headers");
                    continue;
                }
                if code
                    == crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_GET_BLOCK_BODIES_MSG
                {
                    let request =
                        crate::eth_rlpx_parse_get_block_bodies_payload_v1(payload.as_slice())
                            .expect("parse get block bodies");
                    if request.hashes == vec![header_hash, header_hash_2] {
                        let bodies_payload = crate::eth_rlpx_build_block_bodies_payload_v1(
                            request.request_id,
                            &[crate::EthRlpxBlockBodyPayloadV1 {
                                tx_rlp_items: vec![raw_tx.clone()],
                                ommer_header_rlp_items: Vec::new(),
                                withdrawal_rlp_items: Some(Vec::new()),
                            }],
                        );
                        crate::eth_rlpx_write_wire_frame_v1(
                            &mut accepted,
                            &mut responder.session,
                            crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                                + crate::ETH_RLPX_ETH_BLOCK_BODIES_MSG,
                            bodies_payload.as_slice(),
                        )
                        .expect("write partial block bodies");
                        thread::sleep(Duration::from_millis(250));
                        break;
                    }
                    panic!("unexpected body retry hashes");
                }
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                panic!("unexpected worker frame code {code}");
            }
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        assert_eq!(report0.sync_requests, 1);
        let _report1 = worker
            .drive_real_network_once()
            .expect("headers/body request tick");
        let _report2 = worker
            .drive_real_network_once()
            .expect("headers/bodies tick");
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(5)
            && (get_network_runtime_native_body_snapshot_v1(chain_id)
                .is_none_or(|body| body.block_hash != header_hash))
        {
            let _report = worker
                .drive_real_network_once()
                .expect("body/receipt response tick");
            thread::sleep(Duration::from_millis(5));
        }

        let header_snapshot =
            get_network_runtime_native_header_snapshot_v1(chain_id).expect("header snapshot");
        assert_eq!(header_snapshot.number, 121);
        assert_eq!(header_snapshot.hash, header_hash_2);
        let body_snapshot =
            get_network_runtime_native_body_snapshot_v1(chain_id).expect("body snapshot");
        assert_eq!(body_snapshot.number, 120);
        assert_eq!(body_snapshot.block_hash, header_hash);
        assert_eq!(body_snapshot.tx_hashes, vec![tx_hash]);
        assert_eq!(body_snapshot.raw_tx_rlps, vec![expected_raw_tx]);
        assert_eq!(body_snapshot.withdrawal_count, Some(0));
        assert!(body_snapshot.body_available);
        assert!(body_snapshot.txs_materialized);

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_header_body_service_gate_v3() {
        let chain_id = 9_925_u64;
        let local = NodeId(1_290);
        let remote = NodeId(1_291);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let empty_root = crate::eth_rlpx_empty_trie_root_v1();
        let empty_ommers_hash = crate::eth_rlpx_empty_ommers_hash_v1();
        let raw_tx = crate::eth_rlpx_decode_hex_v1(
            "f86c098504a817c800825208943535353535353535353535353535353535353535880de0b6b3a76400008025a028ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276a067cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83",
        )
        .expect("decode service gate raw transaction");
        let tx_hash = crate::eth_rlpx_transaction_hash_v1(raw_tx.as_slice());
        let header_template = crate::EthRlpxBlockHeaderRecordV1 {
            number: 120,
            hash: [0u8; 32],
            parent_hash: [0x80; 32],
            state_root: [0x81; 32],
            transactions_root: crate::eth_rlpx_transactions_root_from_raw_txs_v1(&[raw_tx.clone()]),
            receipts_root: empty_root,
            ommers_hash: empty_ommers_hash,
            logs_bloom: vec![0u8; 256],
            gas_limit: Some(30_000_000),
            gas_used: Some(21_000),
            timestamp: Some(1_234_568),
            base_fee_per_gas: Some(15),
            withdrawals_root: Some(empty_root),
            blob_gas_used: None,
            excess_blob_gas: None,
            block_access_list_hash: None,
            raw_rlp: None,
        };
        let parsed_header = crate::eth_rlpx_parse_block_headers_payload_v1(
            crate::eth_rlpx_build_block_headers_payload_v1(
                1,
                std::slice::from_ref(&header_template),
            )
            .as_slice(),
        )
        .expect("derive service header")
        .headers
        .into_iter()
        .next()
        .expect("service header");
        let header_hash = parsed_header.hash;
        let raw_header_rlp = parsed_header
            .raw_rlp
            .clone()
            .expect("parsed header raw rlp");

        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: parsed_header.number,
                hash: parsed_header.hash,
                parent_hash: parsed_header.parent_hash,
                state_root: parsed_header.state_root,
                transactions_root: parsed_header.transactions_root,
                receipts_root: parsed_header.receipts_root,
                ommers_hash: parsed_header.ommers_hash,
                logs_bloom: parsed_header.logs_bloom.clone(),
                gas_limit: parsed_header.gas_limit,
                gas_used: parsed_header.gas_used,
                timestamp: parsed_header.timestamp,
                base_fee_per_gas: parsed_header.base_fee_per_gas,
                withdrawals_root: parsed_header.withdrawals_root,
                blob_gas_used: parsed_header.blob_gas_used,
                excess_blob_gas: parsed_header.excess_blob_gas,
                block_access_list_hash: parsed_header.block_access_list_hash,
                source_peer_id: Some(local.0),
                observed_unix_ms: 1,
            },
        );
        crate::set_network_runtime_native_header_rlp_v1(
            chain_id,
            header_hash,
            raw_header_rlp.as_slice(),
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 1,
                block_number: parsed_header.number,
                block_hash: header_hash,
                parent_block_hash: parsed_header.parent_hash,
                state_root: parsed_header.state_root,
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: true,
                source_peer_id: Some(local.0),
                observed_unix_ms: 2,
            },
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: parsed_header.number,
                block_hash: header_hash,
                tx_hashes: vec![tx_hash],
                raw_tx_rlps: vec![raw_tx.clone()],
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: Some(Vec::new()),
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 3,
            },
        );
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: parsed_header.number,
                current_block: parsed_header.number,
                highest_block: parsed_header.number,
            },
        );
        assert!(
            snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 8)
                .iter()
                .any(|block| block.hash == header_hash && block.canonical && block.body_available)
        );

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let expected_raw_tx = raw_tx.clone();
        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/header-body-service-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    0,
                    0,
                ),
                earliest_block: 120,
                latest_block: 120,
                latest_block_hash: header_hash,
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(peer_status.latest_block_hash, header_hash);

            let headers_request = crate::eth_rlpx_build_get_block_headers_by_hash_payload_v1(
                9_001,
                header_hash,
                1,
                0,
                false,
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG,
                headers_request.as_slice(),
            )
            .expect("write get block headers");
            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read header service response");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(
                    code,
                    crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_BLOCK_HEADERS_MSG
                );
                let response = crate::eth_rlpx_parse_block_headers_payload_v1(payload.as_slice())
                    .expect("parse header service response");
                assert_eq!(response.request_id, 9_001);
                assert_eq!(response.headers.len(), 1);
                assert_eq!(response.headers[0].hash, header_hash);
                assert_eq!(
                    response.headers[0].raw_rlp.as_deref(),
                    Some(raw_header_rlp.as_slice())
                );
                break;
            }

            let bodies_request =
                crate::eth_rlpx_build_get_block_bodies_payload_v1(9_002, &[header_hash]);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_GET_BLOCK_BODIES_MSG,
                bodies_request.as_slice(),
            )
            .expect("write get block bodies");
            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read body service response");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(
                    code,
                    crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_BLOCK_BODIES_MSG
                );
                let response = crate::eth_rlpx_parse_block_bodies_payload_v1(payload.as_slice())
                    .expect("parse body service response");
                assert_eq!(response.request_id, 9_002);
                assert_eq!(response.bodies.len(), 1);
                assert_eq!(response.bodies[0].tx_rlp_items, vec![expected_raw_tx]);
                assert_eq!(response.bodies[0].withdrawal_count, Some(0));
                done_tx.send(()).expect("signal service gate");
                break;
            }
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        let started = std::time::Instant::now();
        let mut served = false;
        while started.elapsed() < Duration::from_secs(2) {
            if done_rx.try_recv().is_ok() {
                served = true;
                break;
            }
            let _ = worker
                .drive_real_network_once()
                .expect("header/body service tick");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            served,
            "SUPERVM must serve canonical headers and bodies over real RLPx"
        );

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_receipts_gate_v3() {
        let chain_id = 9_926_u64;
        let local = NodeId(1_300);
        let remote = NodeId(1_301);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let empty_root = crate::eth_rlpx_empty_trie_root_v1();
        let empty_ommers_hash = crate::eth_rlpx_empty_ommers_hash_v1();
        let raw_tx = crate::eth_rlpx_decode_hex_v1(
            "f86c098504a817c800825208943535353535353535353535353535353535353535880de0b6b3a76400008025a028ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276a067cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83",
        )
        .expect("decode receipts gate raw transaction");
        let tx_hash = crate::eth_rlpx_transaction_hash_v1(raw_tx.as_slice());
        let receipt_blocks = vec![vec![vec![0xc0]], vec![vec![0xc0]]];
        let header_record_1 = crate::EthRlpxBlockHeaderRecordV1 {
            number: 120,
            hash: [0u8; 32],
            parent_hash: [0x10; 32],
            state_root: [0x20; 32],
            transactions_root: crate::eth_rlpx_transactions_root_from_raw_txs_v1(&[raw_tx.clone()]),
            receipts_root: crate::eth_rlpx_receipts_root_from_raw_receipts_v1(&receipt_blocks[0]),
            ommers_hash: empty_ommers_hash,
            logs_bloom: vec![0u8; 256],
            gas_limit: Some(30_000_000),
            gas_used: Some(21_000),
            timestamp: Some(1_234_567),
            base_fee_per_gas: Some(15),
            withdrawals_root: Some(empty_root),
            blob_gas_used: None,
            excess_blob_gas: None,
            block_access_list_hash: None,
            raw_rlp: None,
        };
        let header_hash = crate::eth_rlpx_parse_block_headers_payload_v1(
            crate::eth_rlpx_build_block_headers_payload_v1(
                1,
                std::slice::from_ref(&header_record_1),
            )
            .as_slice(),
        )
        .expect("derive wire header hash")
        .headers[0]
            .hash;
        let header_record_2 = crate::EthRlpxBlockHeaderRecordV1 {
            number: 121,
            hash: [0u8; 32],
            parent_hash: header_hash,
            state_root: [0x21; 32],
            transactions_root: crate::eth_rlpx_transactions_root_from_raw_txs_v1(&[raw_tx.clone()]),
            receipts_root: crate::eth_rlpx_receipts_root_from_raw_receipts_v1(&receipt_blocks[1]),
            ommers_hash: empty_ommers_hash,
            logs_bloom: vec![0u8; 256],
            gas_limit: Some(30_000_000),
            gas_used: Some(21_000),
            timestamp: Some(1_234_568),
            base_fee_per_gas: Some(15),
            withdrawals_root: Some(empty_root),
            blob_gas_used: None,
            excess_blob_gas: None,
            block_access_list_hash: None,
            raw_rlp: None,
        };
        let header_hash_2 = crate::eth_rlpx_parse_block_headers_payload_v1(
            crate::eth_rlpx_build_block_headers_payload_v1(
                1,
                std::slice::from_ref(&header_record_2),
            )
            .as_slice(),
        )
        .expect("derive second wire header hash")
        .headers[0]
            .hash;
        let server_header_records = vec![header_record_1.clone(), header_record_2.clone()];
        let server_raw_tx = raw_tx.clone();
        let server_receipt_blocks = receipt_blocks.clone();

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 119,
                current_block: 119,
                highest_block: 121,
            },
        );

        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/receipts-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    0,
                    0,
                ),
                earliest_block: 1,
                latest_block: 121,
                latest_block_hash: [0x77; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(
                peer_status.protocol_version,
                crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32
            );

            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read worker frame");
                if code
                    == crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG
                {
                    let request =
                        crate::eth_rlpx_parse_get_block_headers_payload_v1(payload.as_slice())
                            .expect("parse get block headers");
                    let headers_payload = crate::eth_rlpx_build_block_headers_payload_v1(
                        request.request_id,
                        server_header_records.as_slice(),
                    );
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                            + crate::ETH_RLPX_ETH_BLOCK_HEADERS_MSG,
                        headers_payload.as_slice(),
                    )
                    .expect("write block headers");
                    continue;
                }
                if code
                    == crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_GET_BLOCK_BODIES_MSG
                {
                    let request =
                        crate::eth_rlpx_parse_get_block_bodies_payload_v1(payload.as_slice())
                            .expect("parse get block bodies");
                    assert_eq!(request.hashes, vec![header_hash, header_hash_2]);
                    let bodies_payload = crate::eth_rlpx_build_block_bodies_payload_v1(
                        request.request_id,
                        &[
                            crate::EthRlpxBlockBodyPayloadV1 {
                                tx_rlp_items: vec![server_raw_tx.clone()],
                                ommer_header_rlp_items: Vec::new(),
                                withdrawal_rlp_items: Some(Vec::new()),
                            },
                            crate::EthRlpxBlockBodyPayloadV1 {
                                tx_rlp_items: vec![server_raw_tx.clone()],
                                ommer_header_rlp_items: Vec::new(),
                                withdrawal_rlp_items: Some(Vec::new()),
                            },
                        ],
                    );
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_BLOCK_BODIES_MSG,
                        bodies_payload.as_slice(),
                    )
                    .expect("write block bodies");
                    continue;
                }
                if code
                    == crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_GET_RECEIPTS_MSG
                {
                    let request = crate::eth_rlpx_parse_get_receipts_payload_v1(payload.as_slice())
                        .expect("parse get receipts");
                    assert_eq!(request.first_block_receipt_index, 0);
                    if request.hashes == vec![header_hash, header_hash_2] {
                        let receipts_payload = crate::eth_rlpx_build_receipts_payload_v1(
                            request.request_id,
                            false,
                            &server_receipt_blocks[0..1],
                            70,
                        );
                        crate::eth_rlpx_write_wire_frame_v1(
                            &mut accepted,
                            &mut responder.session,
                            crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_RECEIPTS_MSG,
                            receipts_payload.as_slice(),
                        )
                        .expect("write partial receipts");
                        continue;
                    }
                    if request.hashes == vec![header_hash_2] {
                        let receipts_payload = crate::eth_rlpx_build_receipts_payload_v1(
                            request.request_id,
                            false,
                            &server_receipt_blocks[1..2],
                            70,
                        );
                        crate::eth_rlpx_write_wire_frame_v1(
                            &mut accepted,
                            &mut responder.session,
                            crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_RECEIPTS_MSG,
                            receipts_payload.as_slice(),
                        )
                        .expect("write retried receipts");
                        thread::sleep(Duration::from_millis(250));
                        break;
                    }
                    panic!("unexpected receipt hashes");
                }
            }
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        let started = std::time::Instant::now();
        let mut body_updates = 0usize;
        let mut receipt_updates = 0usize;
        while started.elapsed() < Duration::from_secs(2)
            && (body_updates < 2 || receipt_updates < 2)
        {
            let report = worker.drive_real_network_once().expect("network tick");
            body_updates = body_updates.saturating_add(report.body_updates);
            receipt_updates = receipt_updates.saturating_add(report.receipt_updates);
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(body_updates, 2);
        assert_eq!(receipt_updates, 2);
        let body_snapshot =
            get_network_runtime_native_body_snapshot_v1(chain_id).expect("body snapshot");
        assert_eq!(body_snapshot.block_hash, header_hash_2);
        assert_eq!(body_snapshot.tx_hashes, vec![tx_hash]);
        let receipt_snapshot =
            get_network_runtime_native_receipt_snapshot_v1(chain_id, header_hash_2)
                .expect("receipt snapshot");
        assert_eq!(receipt_snapshot.number, 121);
        assert_eq!(receipt_snapshot.block_hash, header_hash_2);
        assert_eq!(receipt_snapshot.raw_receipts, receipt_blocks[1]);
        assert_eq!(receipt_snapshot.receipt_count, 1);
        assert!(receipt_snapshot.receipts_available);
        assert_eq!(
            build_eth_fullnode_native_receipts_response_blocks_v1(
                chain_id,
                &[header_hash, header_hash_2]
            ),
            receipt_blocks
        );
        let blocks = snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 8);
        let imported = blocks
            .iter()
            .find(|block| block.hash == header_hash_2)
            .expect("canonical block state for receipt snapshot");
        assert!(imported.receipts_available);
        assert_eq!(imported.receipt_count, Some(1));
        assert_eq!(imported.receipts_root, Some(header_record_2.receipts_root));

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_reorg_gate_v3() {
        let chain_id = 9_922_u64;
        let local = NodeId(1_260);
        let remote = NodeId(1_261);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let ancestor_hash = [0x19; 32];
        set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 119,
                hash: ancestor_hash,
                parent_hash: [0x18; 32],
                state_root: [0xa9; 32],
                transactions_root: [0xb9; 32],
                receipts_root: [0xc9; 32],
                ommers_hash: [0xd9; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(90_000),
                timestamp: Some(1_234_566),
                base_fee_per_gas: Some(15),
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(local.0),
                observed_unix_ms: 1,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 1,
                block_number: 119,
                block_hash: ancestor_hash,
                parent_block_hash: [0x18; 32],
                state_root: [0xa9; 32],
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: true,
                source_peer_id: Some(local.0),
                observed_unix_ms: 2,
            },
        );
        set_network_runtime_native_body_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1 {
                chain_id,
                number: 119,
                block_hash: ancestor_hash,
                tx_hashes: Vec::new(),
                raw_tx_rlps: Vec::new(),
                ommer_hashes: Vec::new(),
                withdrawal_rlp_items: None,
                withdrawal_count: None,
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 3,
            },
        );
        set_network_runtime_native_receipt_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeReceiptSnapshotV1 {
                chain_id,
                number: 119,
                block_hash: ancestor_hash,
                receipts_root: [0xc9; 32],
                raw_receipts: Vec::new(),
                receipt_count: 0,
                receipts_available: true,
                source_peer_id: Some(local.0),
                observed_unix_ms: 4,
            },
        );
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 119,
                current_block: 119,
                highest_block: 121,
            },
        );

        let raw_tx = crate::eth_rlpx_decode_hex_v1(
            "f86c098504a817c800825208943535353535353535353535353535353535353535880de0b6b3a76400008025a028ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276a067cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83",
        )
        .expect("decode raw transaction");
        let tx_hash = crate::eth_rlpx_transaction_hash_v1(raw_tx.as_slice());
        observe_network_runtime_native_pending_tx_local_ingress_with_payload_v1(
            chain_id,
            tx_hash,
            Some(raw_tx.as_slice()),
        );

        let empty_root = crate::eth_rlpx_empty_trie_root_v1();
        let empty_ommers_hash = crate::eth_rlpx_empty_ommers_hash_v1();
        let header_a_receipts = vec![vec![0xc0]];
        let header_a = crate::EthRlpxBlockHeaderRecordV1 {
            number: 120,
            hash: [0u8; 32],
            parent_hash: ancestor_hash,
            state_root: [0x20; 32],
            transactions_root: crate::eth_rlpx_transactions_root_from_raw_txs_v1(&[raw_tx.clone()]),
            receipts_root: crate::eth_rlpx_receipts_root_from_raw_receipts_v1(&header_a_receipts),
            ommers_hash: empty_ommers_hash,
            logs_bloom: vec![0u8; 256],
            gas_limit: Some(30_000_000),
            gas_used: Some(21_000),
            timestamp: Some(1_234_567),
            base_fee_per_gas: Some(15),
            withdrawals_root: Some(empty_root),
            blob_gas_used: None,
            excess_blob_gas: None,
            block_access_list_hash: None,
            raw_rlp: None,
        };
        let header_b = crate::EthRlpxBlockHeaderRecordV1 {
            number: 121,
            hash: [0u8; 32],
            parent_hash: ancestor_hash,
            state_root: [0x21; 32],
            transactions_root: empty_root,
            receipts_root: empty_root,
            ommers_hash: empty_ommers_hash,
            logs_bloom: vec![0u8; 256],
            gas_limit: Some(30_000_000),
            gas_used: Some(0),
            timestamp: Some(1_234_568),
            base_fee_per_gas: Some(15),
            withdrawals_root: Some(empty_root),
            blob_gas_used: None,
            excess_blob_gas: None,
            block_access_list_hash: None,
            raw_rlp: None,
        };
        let derive_header_hash = |header: &crate::EthRlpxBlockHeaderRecordV1| {
            crate::eth_rlpx_parse_block_headers_payload_v1(
                crate::eth_rlpx_build_block_headers_payload_v1(1, std::slice::from_ref(header))
                    .as_slice(),
            )
            .expect("derive wire header hash")
            .headers[0]
                .hash
        };
        let header_a_hash = derive_header_hash(&header_a);
        let header_b_hash = derive_header_hash(&header_b);
        let server_raw_tx = raw_tx.clone();

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };

        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/reorg-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    0,
                    0,
                ),
                earliest_block: 119,
                latest_block: 121,
                latest_block_hash: header_b_hash,
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(
                peer_status.protocol_version,
                crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32
            );

            let mut header_response_count = 0usize;
            let mut body_response_count = 0usize;
            let mut receipt_response_count = 0usize;
            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read worker frame");
                if code
                    == crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG
                {
                    let request =
                        crate::eth_rlpx_parse_get_block_headers_payload_v1(payload.as_slice())
                            .expect("parse get block headers");
                    let selected_header = if header_response_count == 0 {
                        &header_a
                    } else {
                        &header_b
                    };
                    header_response_count = header_response_count.saturating_add(1);
                    let headers_payload = crate::eth_rlpx_build_block_headers_payload_v1(
                        request.request_id,
                        std::slice::from_ref(selected_header),
                    );
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                            + crate::ETH_RLPX_ETH_BLOCK_HEADERS_MSG,
                        headers_payload.as_slice(),
                    )
                    .expect("write block headers");
                    continue;
                }
                if code
                    == crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_GET_BLOCK_BODIES_MSG
                {
                    let request =
                        crate::eth_rlpx_parse_get_block_bodies_payload_v1(payload.as_slice())
                            .expect("parse get block bodies");
                    let body = if request.hashes == vec![header_a_hash] {
                        crate::EthRlpxBlockBodyPayloadV1 {
                            tx_rlp_items: vec![server_raw_tx.clone()],
                            ommer_header_rlp_items: Vec::new(),
                            withdrawal_rlp_items: Some(Vec::new()),
                        }
                    } else {
                        assert_eq!(request.hashes, vec![header_b_hash]);
                        crate::EthRlpxBlockBodyPayloadV1 {
                            tx_rlp_items: Vec::new(),
                            ommer_header_rlp_items: Vec::new(),
                            withdrawal_rlp_items: Some(Vec::new()),
                        }
                    };
                    let bodies_payload =
                        crate::eth_rlpx_build_block_bodies_payload_v1(request.request_id, &[body]);
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_BLOCK_BODIES_MSG,
                        bodies_payload.as_slice(),
                    )
                    .expect("write block bodies");
                    body_response_count = body_response_count.saturating_add(1);
                    if body_response_count >= 2 && receipt_response_count >= 1 {
                        thread::sleep(Duration::from_millis(250));
                        break;
                    }
                    continue;
                }
                if code
                    == crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_GET_RECEIPTS_MSG
                {
                    let request = crate::eth_rlpx_parse_get_receipts_payload_v1(payload.as_slice())
                        .expect("parse get receipts");
                    let receipt_blocks = if request.hashes == vec![header_a_hash] {
                        vec![vec![0xc0]]
                    } else {
                        assert_eq!(request.hashes, vec![header_b_hash]);
                        Vec::new()
                    };
                    let receipts_payload = crate::eth_rlpx_build_receipts_payload_v1(
                        request.request_id,
                        false,
                        &[receipt_blocks],
                        70,
                    );
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_RECEIPTS_MSG,
                        receipts_payload.as_slice(),
                    )
                    .expect("write receipts");
                    receipt_response_count = receipt_response_count.saturating_add(1);
                    if body_response_count >= 2 && receipt_response_count >= 1 {
                        thread::sleep(Duration::from_millis(250));
                        break;
                    }
                    continue;
                }
            }
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.sync_request_interval_ms = 1;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(2) {
            let _ = worker.drive_real_network_once().expect("network tick");
            let reorg_ready = snapshot_network_runtime_native_canonical_chain_v1(chain_id)
                .is_some_and(|chain| {
                    chain.reorg_count == 1
                        && chain.last_reorg_depth == Some(1)
                        && chain.block_lifecycle_summary.reorged_out_count == 1
                });
            let body_ready = get_network_runtime_native_body_snapshot_v1(chain_id)
                .is_some_and(|body| body.block_hash == header_b_hash);
            let tx_ready =
                get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).is_some_and(|tx| {
                    tx.lifecycle_stage
                        == NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending
                });
            if reorg_ready && body_ready && tx_ready {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let chain =
            snapshot_network_runtime_native_canonical_chain_v1(chain_id).expect("canonical chain");
        assert_eq!(chain.reorg_count, 1);
        assert_eq!(chain.last_reorg_depth, Some(1));
        assert_eq!(
            chain.head.as_ref().map(|head| (head.number, head.hash)),
            Some((121, header_b_hash))
        );
        assert_eq!(chain.block_lifecycle_summary.reorged_out_count, 1);

        let body_snapshot =
            get_network_runtime_native_body_snapshot_v1(chain_id).expect("body snapshot");
        assert_eq!(body_snapshot.number, 121);
        assert_eq!(body_snapshot.block_hash, header_b_hash);
        assert!(body_snapshot.tx_hashes.is_empty());

        let reorged =
            get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).expect("reorged tx");
        assert_eq!(
            reorged.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending
        );
        assert_eq!(reorged.reorg_back_count, 1);
        assert_eq!(reorged.last_block_hash, Some(header_a_hash));
        assert_eq!(reorged.last_block_number, Some(120));

        let summary = snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
        assert_eq!(summary.reorged_back_to_pending_count, 1);
        let candidates =
            snapshot_network_runtime_native_pending_tx_broadcast_candidates_v1(chain_id, 16, 3);
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.tx_hash == tx_hash)
            .expect("reorged tx must re-enter broadcast candidates");
        assert_eq!(
            candidate.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending
        );
        assert_eq!(candidate.tx_payload, raw_tx);

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_tx_ingress_gate_v3() {
        let chain_id = 9_919_u64;
        let local = NodeId(1_230);
        let remote = NodeId(1_231);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 120,
                current_block: 120,
                highest_block: 120,
            },
        );

        let raw_tx = crate::eth_rlpx_decode_hex_v1(
            "f86c098504a817c800825208943535353535353535353535353535353535353535880de0b6b3a76400008025a028ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276a067cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83",
        )
        .expect("decode raw transaction");
        let tx_hash = crate::eth_rlpx_transaction_hash_v1(raw_tx.as_slice());
        let server_tx = raw_tx.clone();

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };

        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/tx-ingress-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    120,
                    0,
                ),
                earliest_block: 120,
                latest_block: 120,
                latest_block_hash: [0x42; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(
                peer_status.protocol_version,
                crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32
            );

            let tx_payload = crate::eth_rlpx_build_transactions_payload_v1(&[server_tx]);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_TRANSACTIONS_MSG,
                tx_payload.as_slice(),
            )
            .expect("write transactions");
            thread::sleep(Duration::from_millis(500));
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.sync_request_interval_ms = u64::MAX;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(2)
            && get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).is_none()
        {
            let _ = worker.drive_real_network_once().expect("transactions tick");
            thread::sleep(Duration::from_millis(5));
        }

        let pending =
            get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).expect("pending tx");
        assert_eq!(
            pending.origin,
            NetworkRuntimeNativePendingTxOriginV1::Remote
        );
        assert_eq!(pending.source_peer_id, Some(remote.0));
        assert_eq!(
            pending.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Pending
        );
        assert_eq!(pending.ingress_count, 1);
        assert_eq!(pending.propagation_count, 0);

        let summary = snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
        assert_eq!(summary.remote_origin_count, 1);
        assert_eq!(summary.pending_count, 1);
        assert_eq!(summary.propagated_count, 0);

        let candidates =
            snapshot_network_runtime_native_pending_tx_broadcast_candidates_v1(chain_id, 16, 3);
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.tx_hash == tx_hash)
            .expect("raw tx must remain broadcast-eligible");
        assert_eq!(candidate.tx_payload, raw_tx);

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_pooled_tx_gate_v3() {
        let chain_id = 9_927_u64;
        let local = NodeId(1_310);
        let remote = NodeId(1_311);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 120,
                current_block: 120,
                highest_block: 120,
            },
        );

        let raw_tx = crate::eth_rlpx_decode_hex_v1(
            "f86c098504a817c800825208943535353535353535353535353535353535353535880de0b6b3a76400008025a028ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276a067cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83",
        )
        .expect("decode raw transaction");
        let tx_hash = crate::eth_rlpx_transaction_hash_v1(raw_tx.as_slice());
        let server_tx = raw_tx.clone();

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };

        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/pooled-tx-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    120,
                    0,
                ),
                earliest_block: 120,
                latest_block: 120,
                latest_block_hash: [0x42; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(
                peer_status.protocol_version,
                crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32
            );

            let announcement = crate::eth_rlpx_build_new_pooled_transaction_hashes_payload_v1(
                &[0],
                &[server_tx.len() as u32],
                &[tx_hash],
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                    + crate::ETH_RLPX_ETH_NEW_POOLED_TRANSACTION_HASHES_MSG,
                announcement.as_slice(),
            )
            .expect("write pooled tx hashes");

            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read pooled tx request");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(
                    code,
                    crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_GET_POOLED_TRANSACTIONS_MSG
                );
                let request =
                    crate::eth_rlpx_parse_get_pooled_transactions_payload_v1(payload.as_slice())
                        .expect("parse get pooled txs");
                assert_eq!(request.hashes, vec![tx_hash]);
                let response = crate::eth_rlpx_build_pooled_transactions_payload_v1(
                    request.request_id,
                    &[server_tx.clone()],
                );
                crate::eth_rlpx_write_wire_frame_v1(
                    &mut accepted,
                    &mut responder.session,
                    crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_POOLED_TRANSACTIONS_MSG,
                    response.as_slice(),
                )
                .expect("write pooled txs");
                thread::sleep(Duration::from_millis(250));
                break;
            }
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.sync_request_interval_ms = u64::MAX;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        let started = std::time::Instant::now();
        let mut materialized = false;
        while started.elapsed() < Duration::from_secs(2) {
            let _ = worker.drive_real_network_once().expect("pooled tx tick");
            let candidates =
                snapshot_network_runtime_native_pending_tx_broadcast_candidates_v1(chain_id, 16, 3);
            materialized = candidates
                .iter()
                .any(|candidate| candidate.tx_hash == tx_hash && candidate.tx_payload == raw_tx);
            if materialized {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            materialized,
            "pooled tx hash announcement must fetch and materialize raw tx"
        );
        let pending =
            get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).expect("pending tx");
        assert_eq!(
            pending.origin,
            NetworkRuntimeNativePendingTxOriginV1::Remote
        );
        assert_eq!(pending.source_peer_id, Some(remote.0));
        assert!(pending.ingress_count >= 2);

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_pooled_tx_response_gate_v3() {
        let chain_id = 9_928_u64;
        let local = NodeId(1_320);
        let remote = NodeId(1_321);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 120,
                current_block: 120,
                highest_block: 120,
            },
        );

        let raw_tx = crate::eth_rlpx_decode_hex_v1(
            "f86c098504a817c800825208943535353535353535353535353535353535353535880de0b6b3a76400008025a028ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276a067cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83",
        )
        .expect("decode raw transaction");
        let tx_hash = crate::eth_rlpx_transaction_hash_v1(raw_tx.as_slice());
        observe_network_runtime_native_pending_tx_local_ingress_with_payload_v1(
            chain_id,
            tx_hash,
            Some(raw_tx.as_slice()),
        );

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        let (done_tx, done_rx) = std::sync::mpsc::channel();

        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/pooled-tx-response-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    120,
                    0,
                ),
                earliest_block: 120,
                latest_block: 120,
                latest_block_hash: [0x42; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(
                peer_status.protocol_version,
                crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32
            );

            let request_id = 77;
            let request =
                crate::eth_rlpx_build_get_pooled_transactions_payload_v1(request_id, &[tx_hash]);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                    + crate::ETH_RLPX_ETH_GET_POOLED_TRANSACTIONS_MSG,
                request.as_slice(),
            )
            .expect("write get pooled txs");

            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read pooled tx response");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(
                    code,
                    crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_POOLED_TRANSACTIONS_MSG
                );
                let response =
                    crate::eth_rlpx_parse_pooled_transactions_payload_v1(payload.as_slice())
                        .expect("parse pooled tx response");
                assert_eq!(response.request_id, request_id);
                assert_eq!(response.tx_hashes, vec![tx_hash]);
                done_tx.send(()).expect("signal pooled tx response gate");
                break;
            }
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.sync_request_interval_ms = u64::MAX;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        let started = std::time::Instant::now();
        let mut responded = false;
        while started.elapsed() < Duration::from_secs(2) {
            if done_rx.try_recv().is_ok() {
                responded = true;
                break;
            }
            let _ = worker
                .drive_real_network_once()
                .expect("pooled tx response tick");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            responded,
            "GetPooledTransactions must receive raw tx response"
        );

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_tx_outbound_broadcast_gate_v3() {
        let chain_id = 9_920_u64;
        let local = NodeId(1_240);
        let remote = NodeId(1_241);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 120,
                current_block: 120,
                highest_block: 120,
            },
        );

        let raw_tx = crate::eth_rlpx_decode_hex_v1(
            "f86c098504a817c800825208943535353535353535353535353535353535353535880de0b6b3a76400008025a028ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276a067cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83",
        )
        .expect("decode raw transaction");
        let tx_hash = crate::eth_rlpx_transaction_hash_v1(raw_tx.as_slice());
        observe_network_runtime_native_pending_tx_local_ingress_with_payload_v1(
            chain_id,
            tx_hash,
            Some(raw_tx.as_slice()),
        );
        let expected_tx = raw_tx.clone();
        let expected_size = expected_tx.len() as u32;

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        let (done_tx, done_rx) = std::sync::mpsc::channel();

        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/tx-outbound-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    120,
                    0,
                ),
                earliest_block: 120,
                latest_block: 120,
                latest_block_hash: [0x42; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(
                peer_status.protocol_version,
                crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32
            );

            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read outbound worker frame");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(
                    code,
                    crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_NEW_POOLED_TRANSACTION_HASHES_MSG
                );
                let announcement = crate::eth_rlpx_parse_new_pooled_transaction_hashes_payload_v1(
                    payload.as_slice(),
                )
                .expect("parse outbound pooled tx hash announce");
                assert_eq!(announcement.tx_types, vec![0]);
                assert_eq!(announcement.tx_sizes, vec![expected_size]);
                assert_eq!(announcement.tx_hashes, vec![tx_hash]);

                let request_id = 88;
                let request = crate::eth_rlpx_build_get_pooled_transactions_payload_v1(
                    request_id,
                    &[tx_hash],
                );
                crate::eth_rlpx_write_wire_frame_v1(
                    &mut accepted,
                    &mut responder.session,
                    crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_GET_POOLED_TRANSACTIONS_MSG,
                    request.as_slice(),
                )
                .expect("write get pooled txs after announce");
                loop {
                    let (response_code, response_payload) =
                        crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                            .expect("read pooled tx response after announce");
                    if response_code == crate::ETH_RLPX_P2P_PING_MSG {
                        crate::eth_rlpx_write_wire_frame_v1(
                            &mut accepted,
                            &mut responder.session,
                            crate::ETH_RLPX_P2P_PONG_MSG,
                            &[],
                        )
                        .expect("write pong");
                        continue;
                    }
                    assert_eq!(
                        response_code,
                        crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                            + crate::ETH_RLPX_ETH_POOLED_TRANSACTIONS_MSG
                    );
                    let response = crate::eth_rlpx_parse_pooled_transactions_payload_v1(
                        response_payload.as_slice(),
                    )
                    .expect("parse pooled tx response after announce");
                    assert_eq!(response.request_id, request_id);
                    assert_eq!(response.tx_hashes, vec![tx_hash]);
                    assert_eq!(response.tx_rlp_items, vec![expected_tx]);
                    done_tx
                        .send(())
                        .expect("signal outbound hash announce gate");
                    break;
                }
                break;
            }
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.sync_request_interval_ms = u64::MAX;
        budget.tx_broadcast_interval_ms = 1;
        budget.tx_broadcast_max_per_tick = 1;
        budget.tx_broadcast_max_propagations = 3;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        let _report1 = worker.drive_real_network_once().expect("broadcast tick");
        let started = std::time::Instant::now();
        let mut responded = false;
        while started.elapsed() < Duration::from_secs(2) {
            if done_rx.try_recv().is_ok() {
                responded = true;
                break;
            }
            let _ = worker
                .drive_real_network_once()
                .expect("pooled tx response tick");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            responded,
            "hash-only announcement must serve GetPooledTransactions"
        );

        let pending =
            get_network_runtime_native_pending_tx_v1(chain_id, tx_hash).expect("pending tx");
        assert_eq!(pending.origin, NetworkRuntimeNativePendingTxOriginV1::Local);
        assert_eq!(
            pending.lifecycle_stage,
            NetworkRuntimeNativePendingTxLifecycleStageV1::Propagated
        );
        assert_eq!(pending.ingress_count, 1);
        assert_eq!(pending.propagation_count, 1);
        assert_eq!(pending.last_propagated_peer_id, Some(remote.0));

        let summary = snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
        assert_eq!(summary.local_origin_count, 1);
        assert_eq!(summary.propagated_count, 1);
        assert_eq!(summary.broadcast_dispatch_success_total, 1);
        assert_eq!(summary.broadcast_tx_total, 1);
        assert_eq!(summary.last_broadcast_peer_id, Some(remote.0));
        assert_eq!(summary.last_broadcast_tx_count, 1);

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_bal_response_gate_v3() {
        let chain_id = 9_924_u64;
        let local = NodeId(1_280);
        let remote = NodeId(1_281);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 120,
                current_block: 120,
                highest_block: 120,
            },
        );

        let available_hash = [0x11; 32];
        let mut bal_builder = novovm_protocol::EvmConstructionBlockAccessListV1::new();
        let bal_account = [0x44u8; 20];
        bal_builder.account_read(bal_account);
        bal_builder.storage_write(0, bal_account, [0x55; 32], [0x66; 32]);
        bal_builder.balance_change(1, bal_account, [0x77; 32]);
        let expected_bal_rlp =
            novovm_protocol::evm_block_access_list_rlp_bytes_v1(&bal_builder.to_access_list())
                .expect("BAL RLP");
        set_network_runtime_native_block_access_list_payload_v1(
            chain_id,
            available_hash,
            expected_bal_rlp.as_slice(),
        )
        .expect("store native BAL payload");

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        let (done_tx, done_rx) = std::sync::mpsc::channel();

        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_caps = crate::default_eth_rlpx_capabilities_v1()
                .into_iter()
                .filter(|cap| !cap.name.eq_ignore_ascii_case("snap"))
                .collect::<Vec<_>>();
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                responder_caps.as_slice(),
                "SuperVM/bal-plugin-response-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    120,
                    0,
                ),
                earliest_block: 120,
                latest_block: 120,
                latest_block_hash: [0x42; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(
                peer_status.protocol_version,
                crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32
            );

            let requested_hashes = vec![available_hash, [0x22; 32], [0x33; 32]];
            let get_payload =
                crate::eth_rlpx_build_get_block_access_lists_payload_v1(77, &requested_hashes);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                    + crate::ETH_RLPX_ETH_GET_BLOCK_ACCESS_LISTS_MSG,
                get_payload.as_slice(),
            )
            .expect("write get block access lists");

            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read BAL response frame");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(
                    code,
                    crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_BLOCK_ACCESS_LISTS_MSG
                );
                let response =
                    crate::eth_rlpx_parse_block_access_lists_payload_v1(payload.as_slice())
                        .expect("parse BAL response");
                assert_eq!(response.request_id, 77);
                assert_eq!(response.lists.len(), requested_hashes.len());
                assert_eq!(
                    response.lists[0].raw_rlp.as_deref(),
                    Some(expected_bal_rlp.as_slice())
                );
                assert_eq!(response.lists[0].account_count, Some(1));
                assert!(response.lists[1..]
                    .iter()
                    .all(|item| item.raw_rlp.is_none()));
                assert!(response.lists[1..]
                    .iter()
                    .all(|item| item.account_count.is_none()));
                done_tx.send(()).expect("signal BAL response gate");
                break;
            }
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.sync_request_interval_ms = u64::MAX;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        let started = std::time::Instant::now();
        let mut responded = false;
        while started.elapsed() < Duration::from_secs(2) {
            if done_rx.try_recv().is_ok() {
                responded = true;
                break;
            }
            let _ = worker.drive_real_network_once().expect("BAL response tick");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(responded, "BAL request must receive protocol response");

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_bal_request_materializes_gate_v3() {
        let chain_id = 9_925_u64;
        let local = NodeId(1_282);
        let remote = NodeId(1_283);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let mut bal_builder = novovm_protocol::EvmConstructionBlockAccessListV1::new();
        let bal_account = [0x45u8; 20];
        bal_builder.account_read(bal_account);
        bal_builder.storage_write(0, bal_account, [0x56; 32], [0x67; 32]);
        let access_list = bal_builder.to_access_list();
        let expected_bal_rlp =
            novovm_protocol::evm_block_access_list_rlp_bytes_v1(&access_list).expect("BAL RLP");
        let bal_hash =
            novovm_protocol::evm_block_access_list_hash_v1(&access_list).expect("BAL hash");

        let empty_root = crate::eth_rlpx_empty_trie_root_v1();
        let header_record = crate::EthRlpxBlockHeaderRecordV1 {
            number: 120,
            hash: [0u8; 32],
            parent_hash: [0x10; 32],
            state_root: [0x20; 32],
            transactions_root: empty_root,
            receipts_root: empty_root,
            ommers_hash: crate::eth_rlpx_empty_ommers_hash_v1(),
            logs_bloom: vec![0u8; 256],
            gas_limit: Some(30_000_000),
            gas_used: Some(0),
            timestamp: Some(1_234_600),
            base_fee_per_gas: Some(15),
            withdrawals_root: Some(empty_root),
            blob_gas_used: None,
            excess_blob_gas: None,
            block_access_list_hash: Some(bal_hash),
            raw_rlp: None,
        };
        let header_hash = crate::eth_rlpx_parse_block_headers_payload_v1(
            crate::eth_rlpx_build_block_headers_payload_v1(1, std::slice::from_ref(&header_record))
                .as_slice(),
        )
        .expect("derive BAL header hash")
        .headers[0]
            .hash;
        let status_head_hash = [0x77; 32];

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 119,
                current_block: 119,
                highest_block: 120,
            },
        );
        let (done_tx, done_rx) = std::sync::mpsc::channel();

        let expected_bal_for_server = expected_bal_rlp.clone();
        let server_header = header_record.clone();
        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/bal-request-materialize-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    0,
                    0,
                ),
                earliest_block: 120,
                latest_block: 120,
                latest_block_hash: status_head_hash,
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(
                peer_status.protocol_version,
                crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32
            );

            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read BAL materialize worker frame");
                if code
                    == crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG
                {
                    let request =
                        crate::eth_rlpx_parse_get_block_headers_payload_v1(payload.as_slice())
                            .expect("parse get block headers");
                    let headers_payload = crate::eth_rlpx_build_block_headers_payload_v1(
                        request.request_id,
                        std::slice::from_ref(&server_header),
                    );
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                            + crate::ETH_RLPX_ETH_BLOCK_HEADERS_MSG,
                        headers_payload.as_slice(),
                    )
                    .expect("write BAL block header");
                    continue;
                }
                if code
                    == crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_GET_BLOCK_BODIES_MSG
                {
                    let request =
                        crate::eth_rlpx_parse_get_block_bodies_payload_v1(payload.as_slice())
                            .expect("parse get block bodies");
                    assert_eq!(request.hashes, vec![header_hash]);
                    let bodies_payload = crate::eth_rlpx_build_block_bodies_payload_v1(
                        request.request_id,
                        &[crate::EthRlpxBlockBodyPayloadV1 {
                            tx_rlp_items: Vec::new(),
                            ommer_header_rlp_items: Vec::new(),
                            withdrawal_rlp_items: Some(Vec::new()),
                        }],
                    );
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_BLOCK_BODIES_MSG,
                        bodies_payload.as_slice(),
                    )
                    .expect("write empty BAL block body");
                    continue;
                }
                if code
                    == crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_GET_BLOCK_ACCESS_LISTS_MSG
                {
                    let request =
                        crate::eth_rlpx_parse_get_block_access_lists_payload_v1(payload.as_slice())
                            .expect("parse get block access lists");
                    assert_eq!(request.hashes, vec![header_hash]);
                    let response_payload = crate::eth_rlpx_build_block_access_lists_payload_v1(
                        request.request_id,
                        &[Some(expected_bal_for_server.clone())],
                    );
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                            + crate::ETH_RLPX_ETH_BLOCK_ACCESS_LISTS_MSG,
                        response_payload.as_slice(),
                    )
                    .expect("write block access lists response");
                    done_tx.send(()).expect("signal BAL materialized");
                    break;
                }
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                panic!("unexpected BAL materialize code {code}");
            }
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.sync_request_interval_ms = 1;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        assert_eq!(report0.sync_requests, 1);
        let started = std::time::Instant::now();
        let mut responded = false;
        while started.elapsed() < Duration::from_secs(5) {
            if done_rx.try_recv().is_ok() {
                responded = true;
                break;
            }
            let _ = worker
                .drive_real_network_once()
                .expect("BAL materialize tick");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(responded, "SUPERVM must request missing block access list");

        let materialized =
            get_network_runtime_native_block_access_list_payload_v1(chain_id, header_hash)
                .expect("materialized BAL payload");
        assert_eq!(materialized, expected_bal_rlp);

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_bal_commitment_rejects_mismatch_gate_v1() {
        let mut bal_builder = novovm_protocol::EvmConstructionBlockAccessListV1::new();
        let account = [0x91u8; 20];
        bal_builder.account_read(account);
        bal_builder.storage_write(0, account, [0x92; 32], [0x93; 32]);
        let access_list = bal_builder.to_access_list();
        let expected_hash =
            novovm_protocol::evm_block_access_list_hash_v1(&access_list).expect("BAL hash");
        let empty_bal_rlp =
            novovm_protocol::evm_block_access_list_rlp_bytes_v1(&Default::default())
                .expect("empty BAL RLP");
        let pending = EthFullnodeNativePendingBlockAccessListV1 {
            block_hash: [0x94; 32],
            block_access_list_hash: expected_hash,
            gas_limit: Some(30_000_000),
            tx_count: Some(0),
        };

        let err = validate_eth_fullnode_native_block_access_list_commitment_v1(
            &pending,
            empty_bal_rlp.as_slice(),
        )
        .expect_err("BAL payload hash must match header BlockAccessListHash");
        assert!(
            err.contains("rlpx_block_access_list_hash_mismatch"),
            "{err}"
        );
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_bal_context_rejects_index_excess_gate_v1() {
        let mut bal_builder = novovm_protocol::EvmConstructionBlockAccessListV1::new();
        let account = [0xa1u8; 20];
        bal_builder.storage_write(3, account, [0xa2; 32], [0xa3; 32]);
        let access_list = bal_builder.to_access_list();
        let bal_hash =
            novovm_protocol::evm_block_access_list_hash_v1(&access_list).expect("BAL hash");
        let bal_rlp =
            novovm_protocol::evm_block_access_list_rlp_bytes_v1(&access_list).expect("BAL RLP");
        let pending = EthFullnodeNativePendingBlockAccessListV1 {
            block_hash: [0xa4; 32],
            block_access_list_hash: bal_hash,
            gas_limit: Some(30_000_000),
            tx_count: Some(0),
        };

        let err = validate_eth_fullnode_native_block_access_list_commitment_v1(
            &pending,
            bal_rlp.as_slice(),
        )
        .expect_err("BAL block access index must fit body tx_count context");
        assert!(
            err.contains("rlpx_bal_block_access_index_exceeds_limit"),
            "{err}"
        );
    }

    #[test]
    fn rlpx_snap_range_proof_semantics_match_geth_complete_storage_v1() {
        let chain_id = 9_943;
        let state_root = [0x44; 32];
        let account_hash = [0x23; 32];
        let account_root_node = {
            let mut node = vec![0xd1_u8];
            node.extend(std::iter::repeat(0x80_u8).take(17));
            node
        };
        let account_state_root = crate::eth_rlpx_trie_node_hash_v1(account_root_node.as_slice());
        let valid_slim_account = vec![0xc4, 0x01, 0x80, 0x80, 0x80];
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);

        let account_without_proof = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 1,
            accounts: vec![crate::EthRlpxSnapAccountDataV1 {
                hash: [0x11; 32],
                body_rlp: vec![0xc0],
            }],
            proof: Vec::new(),
        };
        let account_err = validate_snap_account_range_proof_semantics_v1(
            9_943,
            1,
            Some(account_state_root),
            [0u8; 32],
            [0xffu8; 32],
            &account_without_proof,
        )
        .expect_err("non-empty AccountRange without proof must be rejected");
        assert!(
            account_err
                .to_string()
                .contains("snap_account_range_non_empty_without_proof"),
            "{account_err}"
        );

        let empty_account_without_proof = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 2,
            accounts: Vec::new(),
            proof: Vec::new(),
        };
        let empty_account_err = validate_snap_account_range_proof_semantics_v1(
            9_943,
            1,
            Some(account_state_root),
            [0u8; 32],
            [0xffu8; 32],
            &empty_account_without_proof,
        )
        .expect_err("empty AccountRange without proof must be treated as peer state rejection");
        assert!(
            empty_account_err
                .to_string()
                .contains("snap_account_range_empty_without_proof"),
            "{empty_account_err}"
        );

        let empty_account_with_terminal_proof = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 3,
            accounts: Vec::new(),
            proof: vec![account_root_node.clone()],
        };
        validate_snap_account_range_proof_semantics_v1(
            9_943,
            1,
            Some(account_state_root),
            [0u8; 32],
            [0xffu8; 32],
            &empty_account_with_terminal_proof,
        )
        .expect("empty AccountRange proof with no right-side elements may complete");

        let account_with_valid_proof = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 6,
            accounts: vec![crate::EthRlpxSnapAccountDataV1 {
                hash: [0x12; 32],
                body_rlp: valid_slim_account.clone(),
            }],
            proof: vec![account_root_node.clone()],
        };
        validate_snap_account_range_proof_semantics_v1(
            9_943,
            1,
            Some(account_state_root),
            [0u8; 32],
            [0xffu8; 32],
            &account_with_valid_proof,
        )
        .expect("AccountRange proof root node must match requested stateRoot");

        let account_with_corrupt_proof = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 7,
            proof: vec![vec![0x99, 0x02]],
            ..account_with_valid_proof
        };
        let proof_err = validate_snap_account_range_proof_semantics_v1(
            9_943,
            1,
            Some(account_state_root),
            [0u8; 32],
            [0xffu8; 32],
            &account_with_corrupt_proof,
        )
        .expect_err("corrupt AccountRange proof node must be rejected");
        assert!(
            proof_err
                .to_string()
                .contains("snap_account_range_proof_node_rlp_invalid"),
            "{proof_err}"
        );

        let leaf_account_hash = [0x13; 32];
        let leaf_slim_account = vec![0xc4, 0x01, 0x80, 0x80, 0x80];
        let leaf_full_account =
            crate::eth_rlpx_snap_full_account_rlp_from_slim_v1(leaf_slim_account.as_slice())
                .expect("full account from slim");
        let account_leaf_node = crate::eth_rlpx_mpt_single_leaf_node_rlp_v1(
            &leaf_account_hash,
            leaf_full_account.as_slice(),
        );
        let account_leaf_root = crate::eth_rlpx_trie_node_hash_v1(account_leaf_node.as_slice());
        let account_with_leaf_proof = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 10,
            accounts: vec![crate::EthRlpxSnapAccountDataV1 {
                hash: leaf_account_hash,
                body_rlp: leaf_slim_account,
            }],
            proof: vec![account_leaf_node.clone()],
        };
        validate_snap_account_range_proof_semantics_v1(
            9_943,
            1,
            Some(account_leaf_root),
            [0u8; 32],
            [0xffu8; 32],
            &account_with_leaf_proof,
        )
        .expect("AccountRange proof value must match response account body");

        let account_omits_origin_value = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 13,
            accounts: vec![crate::EthRlpxSnapAccountDataV1 {
                hash: [0x14; 32],
                body_rlp: valid_slim_account.clone(),
            }],
            proof: vec![account_leaf_node.clone()],
        };
        let account_omits_origin_err = validate_snap_account_range_proof_semantics_v1(
            9_943,
            1,
            Some(account_leaf_root),
            leaf_account_hash,
            [0xffu8; 32],
            &account_omits_origin_value,
        )
        .expect_err("AccountRange must include origin when proof proves origin value");
        assert!(
            account_omits_origin_err
                .to_string()
                .contains("snap_account_range_origin_value_omitted"),
            "{account_omits_origin_err}"
        );

        let empty_account_with_more_right_proof = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 14,
            accounts: Vec::new(),
            proof: vec![account_leaf_node.clone()],
        };
        let empty_more_err = validate_snap_account_range_proof_semantics_v1(
            9_943,
            1,
            Some(account_leaf_root),
            [0u8; 32],
            [0xffu8; 32],
            &empty_account_with_more_right_proof,
        )
        .expect_err("empty AccountRange proof must prove there are no more entries");
        assert!(
            empty_more_err
                .to_string()
                .contains("snap_account_range_empty_proof_more_entries"),
            "{empty_more_err}"
        );

        let account_with_mismatched_leaf_proof = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 15,
            accounts: vec![crate::EthRlpxSnapAccountDataV1 {
                hash: leaf_account_hash,
                body_rlp: vec![0xc4, 0x02, 0x80, 0x80, 0x80],
            }],
            proof: vec![account_leaf_node.clone()],
        };
        let account_value_err = validate_snap_account_range_proof_semantics_v1(
            9_943,
            1,
            Some(account_leaf_root),
            [0u8; 32],
            [0xffu8; 32],
            &account_with_mismatched_leaf_proof,
        )
        .expect_err("AccountRange proof leaf value mismatch must be rejected");
        assert!(
            account_value_err
                .to_string()
                .contains("snap_account_range_proof_value_mismatch"),
            "{account_value_err}"
        );

        let account_before_origin = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 16,
            accounts: vec![crate::EthRlpxSnapAccountDataV1 {
                hash: leaf_account_hash,
                body_rlp: valid_slim_account.clone(),
            }],
            proof: vec![account_leaf_node.clone()],
        };
        let account_before_origin_err = validate_snap_account_range_proof_semantics_v1(
            9_943,
            1,
            Some(account_leaf_root),
            [0x20; 32],
            [0xffu8; 32],
            &account_before_origin,
        )
        .expect_err("AccountRange keys before origin must be rejected before cache");
        assert!(
            account_before_origin_err
                .to_string()
                .contains("snap_account_range_account_out_of_bounds"),
            "{account_before_origin_err}"
        );

        let account_duplicate = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 17,
            accounts: vec![
                crate::EthRlpxSnapAccountDataV1 {
                    hash: leaf_account_hash,
                    body_rlp: valid_slim_account.clone(),
                },
                crate::EthRlpxSnapAccountDataV1 {
                    hash: leaf_account_hash,
                    body_rlp: valid_slim_account.clone(),
                },
            ],
            proof: vec![account_leaf_node.clone()],
        };
        let account_duplicate_err = validate_snap_account_range_proof_semantics_v1(
            9_943,
            1,
            Some(account_leaf_root),
            [0u8; 32],
            [0xffu8; 32],
            &account_duplicate,
        )
        .expect_err("AccountRange must be strictly monotonic");
        assert!(
            account_duplicate_err
                .to_string()
                .contains("snap_account_range_account_not_monotonic"),
            "{account_duplicate_err}"
        );

        let account_invalid_body = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 18,
            accounts: vec![crate::EthRlpxSnapAccountDataV1 {
                hash: leaf_account_hash,
                body_rlp: vec![0xc0],
            }],
            proof: vec![account_leaf_node.clone()],
        };
        let account_invalid_body_err = validate_snap_account_range_proof_semantics_v1(
            9_943,
            1,
            Some(account_leaf_root),
            [0u8; 32],
            [0xffu8; 32],
            &account_invalid_body,
        )
        .expect_err("AccountRange slim account body must decode before cache");
        assert!(
            account_invalid_body_err
                .to_string()
                .contains("snap_account_range_account_body_invalid"),
            "{account_invalid_body_err}"
        );

        let gap_account_hash = [0x12; 32];
        let gap_response_hash = [0x13; 32];
        let gap_full_account =
            crate::eth_rlpx_snap_full_account_rlp_from_slim_v1(valid_slim_account.as_slice())
                .expect("full account from slim");
        let account_gap_leaf_node = crate::eth_rlpx_mpt_single_leaf_node_rlp_v1(
            &gap_account_hash,
            gap_full_account.as_slice(),
        );
        let account_gap_root = crate::eth_rlpx_trie_node_hash_v1(account_gap_leaf_node.as_slice());
        let account_with_left_gap = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 19,
            accounts: vec![crate::EthRlpxSnapAccountDataV1 {
                hash: gap_response_hash,
                body_rlp: valid_slim_account.clone(),
            }],
            proof: vec![account_gap_leaf_node],
        };
        let account_left_gap_err = validate_snap_account_range_proof_semantics_v1(
            9_943,
            1,
            Some(account_gap_root),
            [0u8; 32],
            [0xffu8; 32],
            &account_with_left_gap,
        )
        .expect_err("AccountRange proof must reject omitted accounts before first response key");
        assert!(
            account_left_gap_err
                .to_string()
                .contains("snap_account_range_left_gap"),
            "{account_left_gap_err}"
        );

        let internal_gap_account_hash = [0x15; 32];
        let internal_gap_leaf_node = crate::eth_rlpx_mpt_single_leaf_node_rlp_v1(
            &internal_gap_account_hash,
            gap_full_account.as_slice(),
        );
        let internal_gap_root =
            crate::eth_rlpx_trie_node_hash_v1(internal_gap_leaf_node.as_slice());
        let account_with_internal_gap = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 20,
            accounts: vec![
                crate::EthRlpxSnapAccountDataV1 {
                    hash: [0x14; 32],
                    body_rlp: valid_slim_account.clone(),
                },
                crate::EthRlpxSnapAccountDataV1 {
                    hash: [0x16; 32],
                    body_rlp: valid_slim_account.clone(),
                },
            ],
            proof: vec![internal_gap_leaf_node],
        };
        let account_internal_gap_err = validate_snap_account_range_proof_semantics_v1(
            9_943,
            1,
            Some(internal_gap_root),
            [0u8; 32],
            [0xffu8; 32],
            &account_with_internal_gap,
        )
        .expect_err("AccountRange proof must reject omitted accounts inside response range");
        assert!(
            account_internal_gap_err
                .to_string()
                .contains("snap_account_range_internal_gap"),
            "{account_internal_gap_err}"
        );

        let complete_slot = crate::EthRlpxSnapStorageDataV1 {
            hash: [0x22; 32],
            body: vec![0x80],
        };
        let complete_storage_root =
            crate::eth_rlpx_snap_storage_root_from_range_v1(std::slice::from_ref(&complete_slot))
                .expect("complete storage root");
        set_network_runtime_native_snap_account_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapAccountSnapshotV1 {
                chain_id,
                state_root,
                account_hash,
                body_rlp: Vec::new(),
                proof_nodes: Vec::new(),
                storage_root: Some(complete_storage_root),
                code_hash: None,
                has_storage: true,
                has_code: false,
                source_peer_id: Some(1),
                observed_unix_ms: 1,
            },
        );
        let storage_without_proof = crate::EthRlpxStorageRangesResponseV1 {
            request_id: 3,
            slots: vec![vec![complete_slot]],
            proof: Vec::new(),
        };
        validate_snap_storage_ranges_proof_semantics_v1(
            chain_id,
            1,
            Some(state_root),
            &[account_hash],
            &[],
            &[],
            &storage_without_proof,
        )
        .expect("complete StorageRanges without proof must validate by rebuilt root");

        let proof_account_hash = [0x24; 32];
        let storage_proof_node = {
            let mut node = vec![0xd1_u8];
            node.extend(std::iter::repeat(0x80_u8).take(17));
            node
        };
        let proof_storage_root = crate::eth_rlpx_trie_node_hash_v1(storage_proof_node.as_slice());
        set_network_runtime_native_snap_account_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapAccountSnapshotV1 {
                chain_id,
                state_root,
                account_hash: proof_account_hash,
                body_rlp: Vec::new(),
                proof_nodes: Vec::new(),
                storage_root: Some(proof_storage_root),
                code_hash: None,
                has_storage: true,
                has_code: false,
                source_peer_id: Some(1),
                observed_unix_ms: 1,
            },
        );
        let storage_with_valid_proof = crate::EthRlpxStorageRangesResponseV1 {
            request_id: 8,
            slots: vec![Vec::new()],
            proof: vec![storage_proof_node],
        };
        validate_snap_storage_ranges_proof_semantics_v1(
            chain_id,
            1,
            Some(state_root),
            &[proof_account_hash],
            &[],
            &[],
            &storage_with_valid_proof,
        )
        .expect("empty StorageRanges proof with no right-side slots may complete");

        let storage_with_corrupt_proof = crate::EthRlpxStorageRangesResponseV1 {
            request_id: 9,
            proof: vec![vec![0x99, 0x03]],
            ..storage_with_valid_proof
        };
        let storage_proof_err = validate_snap_storage_ranges_proof_semantics_v1(
            chain_id,
            1,
            Some(state_root),
            &[proof_account_hash],
            &[],
            &[],
            &storage_with_corrupt_proof,
        )
        .expect_err("corrupt StorageRanges proof node must be rejected");
        assert!(
            storage_proof_err
                .to_string()
                .contains("snap_storage_ranges_proof_node_rlp_invalid"),
            "{storage_proof_err}"
        );

        let prev_slot = crate::EthRlpxSnapStorageDataV1 {
            hash: [0x27; 32],
            body: vec![0x80],
        };
        let prev_storage_root =
            crate::eth_rlpx_snap_storage_root_from_range_v1(std::slice::from_ref(&prev_slot))
                .expect("previous complete slotset root");
        let prev_account_hash = [0x28; 32];
        let last_account_hash = [0x29; 32];
        let last_empty_proof_node = {
            let mut node = vec![0xd1_u8];
            node.extend(std::iter::repeat(0x80_u8).take(17));
            node
        };
        let last_empty_root = crate::eth_rlpx_trie_node_hash_v1(last_empty_proof_node.as_slice());
        for (account_hash, storage_root) in [
            (prev_account_hash, prev_storage_root),
            (last_account_hash, last_empty_root),
        ] {
            set_network_runtime_native_snap_account_snapshot_v1(
                chain_id,
                NetworkRuntimeNativeSnapAccountSnapshotV1 {
                    chain_id,
                    state_root,
                    account_hash,
                    body_rlp: Vec::new(),
                    proof_nodes: Vec::new(),
                    storage_root: Some(storage_root),
                    code_hash: None,
                    has_storage: true,
                    has_code: false,
                    source_peer_id: Some(1),
                    observed_unix_ms: 1,
                },
            );
        }
        let storage_multi_slotset_last_proof = crate::EthRlpxStorageRangesResponseV1 {
            request_id: 10,
            slots: vec![vec![prev_slot], Vec::new()],
            proof: vec![last_empty_proof_node],
        };
        validate_snap_storage_ranges_proof_semantics_v1(
            chain_id,
            1,
            Some(state_root),
            &[prev_account_hash, last_account_hash],
            &[],
            &[],
            &storage_multi_slotset_last_proof,
        )
        .expect("StorageRanges proof applies only to the final slotset like geth");

        let leaf_storage_account_hash = [0x25; 32];
        let leaf_slot = crate::EthRlpxSnapStorageDataV1 {
            hash: [0x26; 32],
            body: vec![0x80],
        };
        let storage_leaf_node =
            crate::eth_rlpx_mpt_single_leaf_node_rlp_v1(&leaf_slot.hash, leaf_slot.body.as_slice());
        let leaf_storage_root = crate::eth_rlpx_trie_node_hash_v1(storage_leaf_node.as_slice());
        set_network_runtime_native_snap_account_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapAccountSnapshotV1 {
                chain_id,
                state_root,
                account_hash: leaf_storage_account_hash,
                body_rlp: Vec::new(),
                proof_nodes: Vec::new(),
                storage_root: Some(leaf_storage_root),
                code_hash: None,
                has_storage: true,
                has_code: false,
                source_peer_id: Some(1),
                observed_unix_ms: 1,
            },
        );
        let storage_with_leaf_proof = crate::EthRlpxStorageRangesResponseV1 {
            request_id: 12,
            slots: vec![vec![leaf_slot.clone()]],
            proof: vec![storage_leaf_node.clone()],
        };
        validate_snap_storage_ranges_proof_semantics_v1(
            chain_id,
            1,
            Some(state_root),
            &[leaf_storage_account_hash],
            &[],
            &[],
            &storage_with_leaf_proof,
        )
        .expect("StorageRanges proof value must match response slot body");

        let gap_storage_account_hash = [0x2a; 32];
        let gap_slot = crate::EthRlpxSnapStorageDataV1 {
            hash: [0x10; 32],
            body: vec![0x80],
        };
        let gap_storage_leaf_node =
            crate::eth_rlpx_mpt_single_leaf_node_rlp_v1(&gap_slot.hash, gap_slot.body.as_slice());
        let gap_storage_root = crate::eth_rlpx_trie_node_hash_v1(gap_storage_leaf_node.as_slice());
        set_network_runtime_native_snap_account_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapAccountSnapshotV1 {
                chain_id,
                state_root,
                account_hash: gap_storage_account_hash,
                body_rlp: Vec::new(),
                proof_nodes: Vec::new(),
                storage_root: Some(gap_storage_root),
                code_hash: None,
                has_storage: true,
                has_code: false,
                source_peer_id: Some(1),
                observed_unix_ms: 1,
            },
        );
        let storage_with_left_gap = crate::EthRlpxStorageRangesResponseV1 {
            request_id: 13,
            slots: vec![vec![crate::EthRlpxSnapStorageDataV1 {
                hash: [0x20; 32],
                body: vec![0x80],
            }]],
            proof: vec![gap_storage_leaf_node],
        };
        let storage_left_gap_err = validate_snap_storage_ranges_proof_semantics_v1(
            chain_id,
            1,
            Some(state_root),
            &[gap_storage_account_hash],
            &[],
            &[],
            &storage_with_left_gap,
        )
        .expect_err("StorageRanges proof must reject omitted slots before first response key");
        assert!(
            storage_left_gap_err
                .to_string()
                .contains("snap_storage_ranges_left_gap"),
            "{storage_left_gap_err}"
        );

        let internal_gap_storage_account_hash = [0x2b; 32];
        let internal_gap_slot = crate::EthRlpxSnapStorageDataV1 {
            hash: [0x15; 32],
            body: vec![0x80],
        };
        let internal_gap_storage_leaf_node = crate::eth_rlpx_mpt_single_leaf_node_rlp_v1(
            &internal_gap_slot.hash,
            internal_gap_slot.body.as_slice(),
        );
        let internal_gap_storage_root =
            crate::eth_rlpx_trie_node_hash_v1(internal_gap_storage_leaf_node.as_slice());
        set_network_runtime_native_snap_account_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapAccountSnapshotV1 {
                chain_id,
                state_root,
                account_hash: internal_gap_storage_account_hash,
                body_rlp: Vec::new(),
                proof_nodes: Vec::new(),
                storage_root: Some(internal_gap_storage_root),
                code_hash: None,
                has_storage: true,
                has_code: false,
                source_peer_id: Some(1),
                observed_unix_ms: 1,
            },
        );
        let storage_with_internal_gap = crate::EthRlpxStorageRangesResponseV1 {
            request_id: 14,
            slots: vec![vec![
                crate::EthRlpxSnapStorageDataV1 {
                    hash: [0x10; 32],
                    body: vec![0x80],
                },
                crate::EthRlpxSnapStorageDataV1 {
                    hash: [0x20; 32],
                    body: vec![0x80],
                },
            ]],
            proof: vec![internal_gap_storage_leaf_node],
        };
        let storage_internal_gap_err = validate_snap_storage_ranges_proof_semantics_v1(
            chain_id,
            1,
            Some(state_root),
            &[internal_gap_storage_account_hash],
            &[],
            &[],
            &storage_with_internal_gap,
        )
        .expect_err("StorageRanges proof must reject omitted slots inside response range");
        assert!(
            storage_internal_gap_err
                .to_string()
                .contains("snap_storage_ranges_internal_gap"),
            "{storage_internal_gap_err}"
        );

        let storage_empty_with_more_right_proof = crate::EthRlpxStorageRangesResponseV1 {
            request_id: 15,
            slots: vec![Vec::new()],
            proof: vec![storage_leaf_node.clone()],
        };
        let storage_empty_more_err = validate_snap_storage_ranges_proof_semantics_v1(
            chain_id,
            1,
            Some(state_root),
            &[leaf_storage_account_hash],
            &[],
            &[],
            &storage_empty_with_more_right_proof,
        )
        .expect_err("empty StorageRanges proof must prove there are no more slots");
        assert!(
            storage_empty_more_err
                .to_string()
                .contains("snap_storage_ranges_empty_proof_more_entries"),
            "{storage_empty_more_err}"
        );

        let storage_duplicate = crate::EthRlpxStorageRangesResponseV1 {
            request_id: 16,
            slots: vec![vec![leaf_slot.clone(), leaf_slot.clone()]],
            proof: vec![storage_leaf_node.clone()],
        };
        let storage_duplicate_err = validate_snap_storage_ranges_proof_semantics_v1(
            chain_id,
            1,
            Some(state_root),
            &[leaf_storage_account_hash],
            &[],
            &[],
            &storage_duplicate,
        )
        .expect_err("StorageRanges slots must be strictly monotonic");
        assert!(
            storage_duplicate_err
                .to_string()
                .contains("snap_storage_ranges_slot_not_monotonic"),
            "{storage_duplicate_err}"
        );

        let storage_deletion = crate::EthRlpxStorageRangesResponseV1 {
            request_id: 17,
            slots: vec![vec![crate::EthRlpxSnapStorageDataV1 {
                hash: leaf_slot.hash,
                body: Vec::new(),
            }]],
            proof: vec![storage_leaf_node.clone()],
        };
        let storage_deletion_err = validate_snap_storage_ranges_proof_semantics_v1(
            chain_id,
            1,
            Some(state_root),
            &[leaf_storage_account_hash],
            &[],
            &[],
            &storage_deletion,
        )
        .expect_err("StorageRanges deletion values must be rejected");
        assert!(
            storage_deletion_err
                .to_string()
                .contains("snap_storage_ranges_deletion"),
            "{storage_deletion_err}"
        );

        let storage_with_mismatched_leaf_proof = crate::EthRlpxStorageRangesResponseV1 {
            request_id: 18,
            slots: vec![vec![crate::EthRlpxSnapStorageDataV1 {
                hash: leaf_slot.hash,
                body: vec![0x01],
            }]],
            proof: vec![storage_leaf_node],
        };
        let storage_value_err = validate_snap_storage_ranges_proof_semantics_v1(
            chain_id,
            1,
            Some(state_root),
            &[leaf_storage_account_hash],
            &[],
            &[],
            &storage_with_mismatched_leaf_proof,
        )
        .expect_err("StorageRanges proof leaf value mismatch must be rejected");
        assert!(
            storage_value_err
                .to_string()
                .contains("snap_storage_ranges_proof_value_mismatch"),
            "{storage_value_err}"
        );

        let mismatched_storage_without_proof = crate::EthRlpxStorageRangesResponseV1 {
            request_id: 4,
            slots: vec![vec![crate::EthRlpxSnapStorageDataV1 {
                hash: [0x22; 32],
                body: vec![0x81, 0x01],
            }]],
            proof: Vec::new(),
        };
        let storage_err = validate_snap_storage_ranges_proof_semantics_v1(
            chain_id,
            1,
            Some(state_root),
            &[account_hash],
            &[],
            &[],
            &mismatched_storage_without_proof,
        )
        .expect_err("StorageRanges without proof must reject root mismatch");
        assert!(
            storage_err
                .to_string()
                .contains("snap_storage_ranges_root_mismatch"),
            "{storage_err}"
        );

        let empty_storage_without_proof = crate::EthRlpxStorageRangesResponseV1 {
            request_id: 5,
            slots: Vec::new(),
            proof: Vec::new(),
        };
        let empty_storage_err = validate_snap_storage_ranges_proof_semantics_v1(
            chain_id,
            1,
            Some(state_root),
            &[account_hash],
            &[],
            &[],
            &empty_storage_without_proof,
        )
        .expect_err("empty StorageRanges response without proof must not complete storage range");
        assert!(
            empty_storage_err
                .to_string()
                .contains("snap_storage_ranges_empty_without_proof"),
            "{empty_storage_err}"
        );
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
    }

    #[test]
    fn rlpx_snap_account_range_cursor_advances_and_rejects_bad_ranges_v1() {
        let origin = [0u8; 32];
        let mut limit = [0xffu8; 32];
        limit[31] = 0x10;
        let mut account_a = [0u8; 32];
        account_a[31] = 0x01;
        let mut account_b = [0u8; 32];
        account_b[31] = 0x02;
        let response = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 1,
            accounts: vec![
                crate::EthRlpxSnapAccountDataV1 {
                    hash: account_a,
                    body_rlp: vec![0xc0],
                },
                crate::EthRlpxSnapAccountDataV1 {
                    hash: account_b,
                    body_rlp: vec![0xc0],
                },
            ],
            proof: vec![vec![0x01]],
        };

        let next = eth_rlpx_snap_account_range_next_origin_v1(origin, limit, &response, true)
            .expect("next cursor")
            .expect("continuation");
        let mut expected = [0u8; 32];
        expected[31] = 0x03;
        assert_eq!(next, expected);
        assert!(
            eth_rlpx_snap_account_range_next_origin_v1(origin, limit, &response, false)
                .expect("terminal cursor")
                .is_none(),
            "proof-level terminal account range must not advance cursor"
        );

        let bad_order = crate::EthRlpxAccountRangeResponseV1 {
            accounts: vec![
                crate::EthRlpxSnapAccountDataV1 {
                    hash: account_b,
                    body_rlp: vec![0xc0],
                },
                crate::EthRlpxSnapAccountDataV1 {
                    hash: account_a,
                    body_rlp: vec![0xc0],
                },
            ],
            ..response.clone()
        };
        assert!(
            eth_rlpx_snap_account_range_next_origin_v1(origin, limit, &bad_order, true).is_err()
        );

        let out_of_bounds = crate::EthRlpxAccountRangeResponseV1 {
            accounts: vec![crate::EthRlpxSnapAccountDataV1 {
                hash: [0xff; 32],
                body_rlp: vec![0xc0],
            }],
            ..response
        };
        assert!(
            eth_rlpx_snap_account_range_next_origin_v1(origin, limit, &out_of_bounds, true)
                .is_err()
        );
    }

    #[test]
    fn rlpx_snap_account_range_terminal_proof_completes_progress_v1() {
        let chain_id = 9_954_u64;
        let source_peer_id = 77_u64;
        let account_hash = [0x35; 32];
        let slim_account = vec![0xc4, 0x01, 0x80, 0x80, 0x80];
        let full_account =
            crate::eth_rlpx_snap_full_account_rlp_from_slim_v1(slim_account.as_slice())
                .expect("full account from slim");
        let account_leaf_node =
            crate::eth_rlpx_mpt_single_leaf_node_rlp_v1(&account_hash, full_account.as_slice());
        let state_root = crate::eth_rlpx_trie_node_hash_v1(account_leaf_node.as_slice());
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        set_network_runtime_native_snap_trie_node_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapTrieNodeSnapshotV1 {
                chain_id,
                state_root,
                path_segments: eth_fullnode_native_snap_root_trie_pathset_v1(),
                node_hash: state_root,
                node_rlp: account_leaf_node.clone(),
                source_peer_id: Some(source_peer_id),
                observed_unix_ms: 1,
            },
        );

        let mut session = dummy_rlpx_live_session(chain_id);
        session.last_snap_account_range_request_id = Some(93);
        session.last_snap_state_root = Some(state_root);
        session.last_snap_account_origin = Some([0u8; 32]);
        session.last_snap_account_limit = Some([0xff; 32]);
        let response = crate::EthRlpxAccountRangeResponseV1 {
            request_id: 93,
            accounts: vec![crate::EthRlpxSnapAccountDataV1 {
                hash: account_hash,
                body_rlp: slim_account,
            }],
            proof: vec![account_leaf_node],
        };

        ingest_real_rlpx_snap_account_range_v1(chain_id, source_peer_id, &mut session, &response)
            .expect("terminal account range proof must complete");

        assert!(session.last_snap_account_range_request_id.is_none());
        assert!(session.pending_snap_next_account_origin.is_none());
        assert!(session.last_snap_storage_ranges_request_id.is_none());
        assert!(session.last_snap_byte_codes_request_id.is_none());
        assert!(session.last_snap_trie_nodes_request_id.is_none());
        let progress =
            crate::get_network_runtime_native_snap_account_range_progress_v1(chain_id, state_root)
                .expect("snap account range progress");
        assert!(progress.completed);
        assert!(progress.next_account_origin.is_none());
        assert_eq!(progress.limit, [0xff; 32]);
        let account = crate::get_network_runtime_native_snap_account_snapshot_v1(
            chain_id,
            state_root,
            account_hash,
        )
        .expect("snap account snapshot");
        assert!(!account.has_storage);
        assert!(!account.has_code);
    }

    #[test]
    fn rlpx_snap_trie_nodes_partial_response_matches_geth_heal_semantics_v1() {
        let root_node = {
            let mut node = vec![0xd1_u8];
            node.extend(std::iter::repeat(0x80_u8).take(17));
            node
        };
        let storage_node = {
            let mut node = vec![0xd1_u8, 0x01];
            node.extend(std::iter::repeat(0x80_u8).take(16));
            node
        };
        let unexpected_node = {
            let mut node = vec![0xd1_u8, 0x02];
            node.extend(std::iter::repeat(0x80_u8).take(16));
            node
        };
        let root_hash = crate::eth_rlpx_trie_node_hash_v1(root_node.as_slice());
        let storage_hash = crate::eth_rlpx_trie_node_hash_v1(storage_node.as_slice());
        let expected = vec![root_hash, storage_hash];

        let partial =
            match_eth_fullnode_native_snap_trie_nodes_v1(expected.as_slice(), &[storage_node])
                .expect("missing earlier trie node should remain a heal gap");
        assert_eq!(partial, vec![(1, storage_hash)]);

        let full = match_eth_fullnode_native_snap_trie_nodes_v1(
            expected.as_slice(),
            &[root_node.clone(), {
                let mut node = vec![0xd1_u8, 0x01];
                node.extend(std::iter::repeat(0x80_u8).take(16));
                node
            }],
        )
        .expect("complete ordered trie nodes");
        assert_eq!(full, vec![(0, root_hash), (1, storage_hash)]);

        let out_of_order = match_eth_fullnode_native_snap_trie_nodes_v1(
            expected.as_slice(),
            &[
                {
                    let mut node = vec![0xd1_u8, 0x01];
                    node.extend(std::iter::repeat(0x80_u8).take(16));
                    node
                },
                root_node,
            ],
        )
        .expect_err("out-of-order trie nodes are not geth-compatible");
        assert!(out_of_order.contains("snap_trie_nodes_unexpected_hash"));

        let unexpected =
            match_eth_fullnode_native_snap_trie_nodes_v1(expected.as_slice(), &[unexpected_node])
                .expect_err("unrequested trie node must be rejected");
        assert!(unexpected.contains("snap_trie_nodes_unexpected_hash"));
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_snap_account_range_gate_v3() {
        let chain_id = 9_934_u64;
        let local = NodeId(1_380);
        let remote = NodeId(1_381);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let root_trie_node = {
            let mut node = vec![0xd1_u8];
            node.extend(std::iter::repeat(0x80_u8).take(17));
            node
        };
        let local_state_root = crate::eth_rlpx_trie_node_hash_v1(root_trie_node.as_slice());
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 19_800,
                current_block: 19_872,
                highest_block: 20_000,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::State,
                peer_count: 1,
                block_number: 19_872,
                block_hash: [0xb5; 32],
                parent_block_hash: [0xb4; 32],
                state_root: local_state_root,
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: true,
                source_peer_id: Some(local.0),
                observed_unix_ms: 1,
            },
        );
        crate::runtime_status::set_network_runtime_native_sync_status(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeSyncStatusV1 {
                phase: NetworkRuntimeNativeSyncPhaseV1::State,
                peer_count: 1,
                starting_block: 19_800,
                current_block: 19_872,
                highest_block: 20_000,
                updated_at_unix_millis: 1,
            },
        );
        set_network_runtime_native_snap_account_range_progress_v1(
            chain_id,
            NetworkRuntimeNativeSnapAccountRangeProgressV1 {
                chain_id,
                state_root: local_state_root,
                next_account_origin: Some([0u8; 32]),
                limit: [0xff; 32],
                completed: false,
                source_peer_id: Some(local.0),
                observed_unix_ms: 1,
            },
        );

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        let (done_tx, done_rx) = std::sync::mpsc::channel();

        let server = thread::spawn(move || {
            let root_trie_node = root_trie_node;
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/snap-account-range-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    20_000,
                    0,
                ),
                earliest_block: 1,
                latest_block: 20_000,
                latest_block_hash: [0x42; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(
                peer_status.protocol_version,
                crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32
            );
            let snap_offset =
                crate::eth_rlpx_snap_base_offset_v1(peer_status.protocol_version as u8, Some(1))
                    .expect("snap offset");

            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read snap request");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(
                    code,
                    snap_offset + crate::ETH_RLPX_SNAP_GET_ACCOUNT_RANGE_MSG
                );
                let request =
                    crate::eth_rlpx_parse_get_account_range_payload_v1(payload.as_slice())
                        .expect("parse snap get account range");
                assert_eq!(request.root, local_state_root);
                assert_eq!(request.origin, [0u8; 32]);
                assert_eq!(request.limit, [0xff; 32]);
                assert_eq!(
                    request.byte_limit,
                    crate::ETH_RLPX_SNAP_DEFAULT_ACCOUNT_RANGE_BYTES
                );
                let response = crate::eth_rlpx_build_account_range_payload_v1(
                    request.request_id,
                    &[],
                    &[root_trie_node.clone()],
                );
                crate::eth_rlpx_write_wire_frame_v1(
                    &mut accepted,
                    &mut responder.session,
                    snap_offset + crate::ETH_RLPX_SNAP_ACCOUNT_RANGE_MSG,
                    response.as_slice(),
                )
                .expect("write snap account range");
                done_tx.send(()).expect("signal snap gate");
                thread::sleep(Duration::from_millis(250));
                break;
            }
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.sync_request_interval_ms = 1;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        let started = std::time::Instant::now();
        let mut requested = false;
        while started.elapsed() < Duration::from_secs(2) {
            if done_rx.try_recv().is_ok() {
                requested = true;
            }
            let _ = worker.drive_real_network_once().expect("snap tick");
            let evidence = snapshot_eth_native_sync_evidence(chain_id);
            if requested && evidence.snap_pull_seen && evidence.snap_response_seen {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(requested, "State phase must send snap GetAccountRange");
        let evidence = snapshot_eth_native_sync_evidence(chain_id);
        assert!(evidence.snap_pull_seen);
        assert!(evidence.snap_response_seen);
        let sessions = snapshot_network_runtime_eth_peer_sessions(chain_id);
        assert_eq!(
            sessions[0].negotiated.snap_version,
            Some(crate::SnapWireVersion::V1)
        );

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_snap_account_range_continuation_gate_v3() {
        fn test_rlp_encode_len(prefix: u8, len: usize) -> Vec<u8> {
            if len <= 55 {
                return vec![prefix + len as u8];
            }
            let bytes = len.to_be_bytes();
            let first = bytes
                .iter()
                .position(|byte| *byte != 0)
                .unwrap_or(bytes.len() - 1);
            let len_bytes = &bytes[first..];
            let mut out = vec![prefix + 55 + len_bytes.len() as u8];
            out.extend_from_slice(len_bytes);
            out
        }

        fn test_rlp_encode_bytes(bytes: &[u8]) -> Vec<u8> {
            if bytes.len() == 1 && bytes[0] < 0x80 {
                return vec![bytes[0]];
            }
            let mut out = test_rlp_encode_len(0x80, bytes.len());
            out.extend_from_slice(bytes);
            out
        }

        fn test_rlp_encode_list(items: &[Vec<u8>]) -> Vec<u8> {
            let payload_len = items.iter().map(Vec::len).sum::<usize>();
            let mut out = test_rlp_encode_len(0xc0, payload_len);
            for item in items {
                out.extend_from_slice(item);
            }
            out
        }

        fn test_mpt_nibbles(key: &[u8]) -> Vec<u8> {
            let mut out = Vec::with_capacity(key.len() * 2);
            for byte in key {
                out.push(byte >> 4);
                out.push(byte & 0x0f);
            }
            out
        }

        fn test_mpt_hex_prefix(path_nibbles: &[u8], is_leaf: bool) -> Vec<u8> {
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
                out.push(((path_nibbles[idx] & 0x0f) << 4) | (path_nibbles[idx + 1] & 0x0f));
                idx += 2;
            }
            out
        }

        fn test_mpt_leaf_from_path(path_nibbles: &[u8], value: &[u8]) -> Vec<u8> {
            test_rlp_encode_list(&[
                test_rlp_encode_bytes(test_mpt_hex_prefix(path_nibbles, true).as_slice()),
                test_rlp_encode_bytes(value),
            ])
        }

        fn test_branch_with_two_leaf_children(
            left_hash: &[u8; 32],
            left_value: &[u8],
            right_hash: &[u8; 32],
            right_value: &[u8],
        ) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
            let left_nibbles = test_mpt_nibbles(left_hash);
            let right_nibbles = test_mpt_nibbles(right_hash);
            assert_ne!(left_nibbles[0], right_nibbles[0]);
            let left_leaf = test_mpt_leaf_from_path(&left_nibbles[1..], left_value);
            let right_leaf = test_mpt_leaf_from_path(&right_nibbles[1..], right_value);
            let mut items = Vec::with_capacity(17);
            for idx in 0..16 {
                if idx == left_nibbles[0] as usize {
                    items.push(test_rlp_encode_bytes(&crate::eth_rlpx_trie_node_hash_v1(
                        left_leaf.as_slice(),
                    )));
                } else if idx == right_nibbles[0] as usize {
                    items.push(test_rlp_encode_bytes(&crate::eth_rlpx_trie_node_hash_v1(
                        right_leaf.as_slice(),
                    )));
                } else {
                    items.push(test_rlp_encode_bytes(&[]));
                }
            }
            items.push(test_rlp_encode_bytes(&[]));
            (test_rlp_encode_list(&items), left_leaf, right_leaf)
        }

        let chain_id = 9_944_u64;
        let local = NodeId(1_394);
        let remote = NodeId(1_395);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let mut account_hash = [0u8; 32];
        account_hash[0] = 0x10;
        account_hash[31] = 0x07;
        let mut expected_next_origin = account_hash;
        expected_next_origin[31] = 0x08;
        let mut right_account_hash = [0u8; 32];
        right_account_hash[0] = 0x20;
        let empty_slim_account_body = vec![0xc4, 0x01, 0x80, 0x80, 0x80];
        let full_account =
            crate::eth_rlpx_snap_full_account_rlp_from_slim_v1(empty_slim_account_body.as_slice())
                .expect("full account from slim");
        let (root_trie_node, account_leaf_node, right_leaf_node) =
            test_branch_with_two_leaf_children(
                &account_hash,
                full_account.as_slice(),
                &right_account_hash,
                full_account.as_slice(),
            );
        let local_state_root = crate::eth_rlpx_trie_node_hash_v1(root_trie_node.as_slice());
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 22_800,
                current_block: 22_872,
                highest_block: 23_000,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::State,
                peer_count: 1,
                block_number: 22_872,
                block_hash: [0xb7; 32],
                parent_block_hash: [0xb6; 32],
                state_root: local_state_root,
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: true,
                source_peer_id: Some(local.0),
                observed_unix_ms: 1,
            },
        );
        crate::runtime_status::set_network_runtime_native_sync_status(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeSyncStatusV1 {
                phase: NetworkRuntimeNativeSyncPhaseV1::State,
                peer_count: 1,
                starting_block: 22_800,
                current_block: 22_872,
                highest_block: 23_000,
                updated_at_unix_millis: 1,
            },
        );
        set_network_runtime_native_snap_account_range_progress_v1(
            chain_id,
            NetworkRuntimeNativeSnapAccountRangeProgressV1 {
                chain_id,
                state_root: local_state_root,
                next_account_origin: Some([0u8; 32]),
                limit: [0xff; 32],
                completed: false,
                source_peer_id: Some(local.0),
                observed_unix_ms: 1,
            },
        );

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        let (done_tx, done_rx) = std::sync::mpsc::channel();

        let server = thread::spawn(move || {
            let empty_slim_account_body = vec![0xc4, 0x01, 0x80, 0x80, 0x80];
            let root_trie_node = root_trie_node;
            let account_leaf_node = account_leaf_node;
            let right_leaf_node = right_leaf_node;

            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/snap-account-range-continuation-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    23_000,
                    0,
                ),
                earliest_block: 1,
                latest_block: 23_000,
                latest_block_hash: [0x45; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);
            let snap_offset =
                crate::eth_rlpx_snap_base_offset_v1(peer_status.protocol_version as u8, Some(1))
                    .expect("snap offset");

            let first_request = loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read first snap account request");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(
                    code,
                    snap_offset + crate::ETH_RLPX_SNAP_GET_ACCOUNT_RANGE_MSG
                );
                break crate::eth_rlpx_parse_get_account_range_payload_v1(payload.as_slice())
                    .expect("parse first snap get account range");
            };
            assert_eq!(first_request.root, local_state_root);
            assert_eq!(first_request.origin, [0u8; 32]);
            assert_eq!(first_request.limit, [0xff; 32]);
            assert_eq!(
                first_request.byte_limit,
                crate::ETH_RLPX_SNAP_DEFAULT_ACCOUNT_RANGE_BYTES
            );

            let account = crate::EthRlpxSnapAccountDataV1 {
                hash: account_hash,
                body_rlp: empty_slim_account_body.clone(),
            };
            let response = crate::eth_rlpx_build_account_range_payload_v1(
                first_request.request_id,
                &[account],
                &[
                    root_trie_node.clone(),
                    account_leaf_node.clone(),
                    right_leaf_node.clone(),
                ],
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                snap_offset + crate::ETH_RLPX_SNAP_ACCOUNT_RANGE_MSG,
                response.as_slice(),
            )
            .expect("write first account range");

            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read root trie nodes request");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(code, snap_offset + crate::ETH_RLPX_SNAP_GET_TRIE_NODES_MSG);
                let request = crate::eth_rlpx_parse_get_trie_nodes_payload_v1(payload.as_slice())
                    .expect("parse root trie nodes request");
                assert_eq!(request.root, local_state_root);
                assert_eq!(request.paths, vec![vec![vec![0_u8]]]);
                let response = crate::eth_rlpx_build_trie_nodes_payload_v1(
                    request.request_id,
                    &[root_trie_node.clone()],
                );
                crate::eth_rlpx_write_wire_frame_v1(
                    &mut accepted,
                    &mut responder.session,
                    snap_offset + crate::ETH_RLPX_SNAP_TRIE_NODES_MSG,
                    response.as_slice(),
                )
                .expect("write root trie nodes response");
                break;
            }

            let second_request = loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read continuation snap account request");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(
                    code,
                    snap_offset + crate::ETH_RLPX_SNAP_GET_ACCOUNT_RANGE_MSG
                );
                break crate::eth_rlpx_parse_get_account_range_payload_v1(payload.as_slice())
                    .expect("parse continuation snap get account range");
            };
            assert_ne!(second_request.request_id, first_request.request_id);
            assert_eq!(second_request.root, local_state_root);
            assert_eq!(second_request.origin, expected_next_origin);
            assert_eq!(second_request.limit, [0xff; 32]);
            assert_eq!(
                second_request.byte_limit,
                crate::ETH_RLPX_SNAP_DEFAULT_ACCOUNT_RANGE_BYTES
            );

            let right_account = crate::EthRlpxSnapAccountDataV1 {
                hash: right_account_hash,
                body_rlp: empty_slim_account_body,
            };
            let done_response = crate::eth_rlpx_build_account_range_payload_v1(
                second_request.request_id,
                &[right_account],
                &[root_trie_node.clone(), right_leaf_node.clone()],
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                snap_offset + crate::ETH_RLPX_SNAP_ACCOUNT_RANGE_MSG,
                done_response.as_slice(),
            )
            .expect("write final empty account range");
            done_tx
                .send(())
                .expect("signal snap account continuation gate");
            thread::sleep(Duration::from_millis(50));
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.sync_request_interval_ms = 1;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        let started = std::time::Instant::now();
        let mut continued = false;
        while started.elapsed() < Duration::from_secs(5) {
            if done_rx.try_recv().is_ok() {
                continued = true;
                break;
            }
            let _ = worker
                .drive_real_network_once()
                .expect("snap continuation tick");
            if done_rx.try_recv().is_ok() {
                continued = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            continued,
            "AccountRange response must drive cursor continuation request"
        );
        let evidence = snapshot_eth_native_sync_evidence(chain_id);
        assert!(evidence.snap_pull_seen);
        assert!(evidence.snap_response_seen);
        let account = crate::get_network_runtime_native_snap_account_snapshot_v1(
            chain_id,
            local_state_root,
            account_hash,
        )
        .expect("snap account snapshot");
        assert!(!account.has_storage);
        assert!(!account.has_code);

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_snap_service_sidecars_gate_v3() {
        let chain_id = 9_941_u64;
        let local = NodeId(1_390);
        let remote = NodeId(1_391);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let state_root = [0xa5; 32];
        let account_hash = [0x01; 32];
        let account_range_hash = [0x21; 32];
        let account_slim_body = vec![0xc4, 0x01, 0x80, 0x80, 0x80];
        let account_full_body =
            crate::eth_rlpx_snap_full_account_rlp_from_slim_v1(account_slim_body.as_slice())
                .expect("full account from slim");
        let account_proof_node = crate::eth_rlpx_mpt_single_leaf_node_rlp_v1(
            &account_range_hash,
            account_full_body.as_slice(),
        );
        let account_state_root = crate::eth_rlpx_trie_node_hash_v1(account_proof_node.as_slice());
        let slot_hash = [0x10; 32];
        let slot_body = vec![0x80];
        let storage_proof_node =
            crate::eth_rlpx_mpt_single_leaf_node_rlp_v1(&slot_hash, slot_body.as_slice());
        let bytecode = vec![0x60, 0x00];
        let code_hash = crate::eth_rlpx_code_hash_v1(bytecode.as_slice());
        let trie_path = vec![vec![0x01], vec![0x02]];
        let trie_node = {
            let mut node = vec![0xd1_u8, 0x03];
            node.extend(std::iter::repeat(0x80_u8).take(16));
            node
        };
        let trie_node_hash = crate::eth_rlpx_trie_node_hash_v1(trie_node.as_slice());
        set_network_runtime_native_snap_account_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapAccountSnapshotV1 {
                chain_id,
                state_root: account_state_root,
                account_hash: account_range_hash,
                body_rlp: account_slim_body.clone(),
                proof_nodes: vec![account_proof_node.clone()],
                storage_root: None,
                code_hash: None,
                has_storage: false,
                has_code: false,
                source_peer_id: Some(local.0),
                observed_unix_ms: 1,
            },
        );
        set_network_runtime_native_snap_account_storage_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapAccountStorageSnapshotV1 {
                chain_id,
                state_root,
                account_hash,
                slots: vec![NetworkRuntimeNativeSnapStorageSlotSnapshotV1 {
                    hash: slot_hash,
                    body: slot_body.clone(),
                }],
                proof_nodes: vec![storage_proof_node.clone()],
                source_peer_id: Some(local.0),
                observed_unix_ms: 1,
            },
        );
        set_network_runtime_native_snap_code_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapCodeSnapshotV1 {
                chain_id,
                code_hash,
                code: bytecode.clone(),
                source_peer_id: Some(local.0),
                observed_unix_ms: 1,
            },
        );
        set_network_runtime_native_snap_trie_node_snapshot_v1(
            chain_id,
            NetworkRuntimeNativeSnapTrieNodeSnapshotV1 {
                chain_id,
                state_root,
                path_segments: trie_path.clone(),
                node_hash: trie_node_hash,
                node_rlp: trie_node.clone(),
                source_peer_id: Some(local.0),
                observed_unix_ms: 1,
            },
        );
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 21_000,
                current_block: 21_000,
                highest_block: 21_000,
            },
        );

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        let (done_tx, done_rx) = std::sync::mpsc::channel();

        let server = thread::spawn(move || {
            let account_state_root = account_state_root;
            let account_range_hash = account_range_hash;
            let account_slim_body = account_slim_body;
            let account_proof_node = account_proof_node;
            let state_root = state_root;
            let account_hash = account_hash;
            let slot_hash = slot_hash;
            let slot_body = slot_body;
            let storage_proof_node = storage_proof_node;
            let bytecode = bytecode;
            let code_hash = code_hash;
            let trie_path = trie_path;
            let trie_node = trie_node;
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/snap-sidecars-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    21_000,
                    0,
                ),
                earliest_block: 1,
                latest_block: 21_000,
                latest_block_hash: [0x43; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(
                peer_status.protocol_version,
                crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32
            );
            let snap_offset =
                crate::eth_rlpx_snap_base_offset_v1(peer_status.protocol_version as u8, Some(1))
                    .expect("snap offset");

            let account_request = crate::eth_rlpx_build_get_account_range_payload_v1(
                10,
                account_state_root,
                [0u8; 32],
                [0xff; 32],
                4096,
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                snap_offset + crate::ETH_RLPX_SNAP_GET_ACCOUNT_RANGE_MSG,
                account_request.as_slice(),
            )
            .expect("write get account range");
            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read account range response");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(code, snap_offset + crate::ETH_RLPX_SNAP_ACCOUNT_RANGE_MSG);
                let response = crate::eth_rlpx_parse_account_range_payload_v1(payload.as_slice())
                    .expect("parse account range response");
                assert_eq!(response.request_id, 10);
                assert_eq!(response.accounts.len(), 1);
                assert_eq!(response.accounts[0].hash, account_range_hash);
                assert_eq!(response.accounts[0].body_rlp, account_slim_body);
                assert_eq!(response.proof, vec![account_proof_node.clone()]);
                break;
            }

            let storage_request = crate::eth_rlpx_build_get_storage_ranges_payload_v1(
                11,
                state_root,
                &[account_hash],
                &[0x00],
                &[0xff],
                4096,
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                snap_offset + crate::ETH_RLPX_SNAP_GET_STORAGE_RANGES_MSG,
                storage_request.as_slice(),
            )
            .expect("write get storage ranges");
            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read storage ranges response");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(code, snap_offset + crate::ETH_RLPX_SNAP_STORAGE_RANGES_MSG);
                let response = crate::eth_rlpx_parse_storage_ranges_payload_v1(payload.as_slice())
                    .expect("parse storage ranges response");
                assert_eq!(response.request_id, 11);
                assert_eq!(response.slots.len(), 1);
                assert_eq!(response.slots[0].len(), 1);
                assert_eq!(response.slots[0][0].hash, slot_hash);
                assert_eq!(response.slots[0][0].body, slot_body);
                assert_eq!(response.proof, vec![storage_proof_node.clone()]);
                break;
            }

            let byte_codes_request =
                crate::eth_rlpx_build_get_byte_codes_payload_v1(12, &[code_hash], 4096);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                snap_offset + crate::ETH_RLPX_SNAP_GET_BYTE_CODES_MSG,
                byte_codes_request.as_slice(),
            )
            .expect("write get byte codes");
            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read byte codes response");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(code, snap_offset + crate::ETH_RLPX_SNAP_BYTE_CODES_MSG);
                let response = crate::eth_rlpx_parse_byte_codes_payload_v1(payload.as_slice())
                    .expect("parse byte codes response");
                assert_eq!(response.request_id, 12);
                assert_eq!(response.codes, vec![bytecode.clone()]);
                break;
            }

            let trie_nodes_request = crate::eth_rlpx_build_get_trie_nodes_payload_v1(
                13,
                state_root,
                std::slice::from_ref(&trie_path),
                4096,
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                snap_offset + crate::ETH_RLPX_SNAP_GET_TRIE_NODES_MSG,
                trie_nodes_request.as_slice(),
            )
            .expect("write get trie nodes");
            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read trie nodes response");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(code, snap_offset + crate::ETH_RLPX_SNAP_TRIE_NODES_MSG);
                let response = crate::eth_rlpx_parse_trie_nodes_payload_v1(payload.as_slice())
                    .expect("parse trie nodes response");
                assert_eq!(response.request_id, 13);
                assert_eq!(response.nodes, vec![trie_node.clone()]);
                break;
            }

            done_tx.send(()).expect("signal snap sidecars gate");
            thread::sleep(Duration::from_millis(50));
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.sync_request_interval_ms = u64::MAX;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        let started = std::time::Instant::now();
        let mut responded = false;
        while started.elapsed() < Duration::from_secs(2) {
            if done_rx.try_recv().is_ok() {
                responded = true;
                break;
            }
            let _ = worker
                .drive_real_network_once()
                .expect("snap sidecars tick");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            responded,
            "snap sidecar requests must receive protocol responses"
        );

        let sessions = snapshot_network_runtime_eth_peer_sessions(chain_id);
        assert_eq!(
            sessions[0].negotiated.snap_version,
            Some(crate::SnapWireVersion::V1)
        );

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_snap_account_to_storage_code_gate_v3() {
        let chain_id = 9_942_u64;
        let local = NodeId(1_392);
        let remote = NodeId(1_393);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let root_trie_node = {
            let mut node = vec![0xd1_u8];
            node.extend(std::iter::repeat(0x80_u8).take(17));
            node
        };
        let storage_slot_hash = [0x37; 32];
        let storage_slot_body = vec![0x80];
        let storage_root_trie_node = crate::eth_rlpx_mpt_single_leaf_node_rlp_v1(
            &storage_slot_hash,
            storage_slot_body.as_slice(),
        );
        let local_state_root = crate::eth_rlpx_trie_node_hash_v1(root_trie_node.as_slice());
        let storage_root = crate::eth_rlpx_trie_node_hash_v1(storage_root_trie_node.as_slice());
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 21_900,
                current_block: 21_972,
                highest_block: 22_100,
            },
        );
        set_network_runtime_native_head_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: NetworkRuntimeNativeSyncPhaseV1::State,
                peer_count: 1,
                block_number: 21_972,
                block_hash: [0xb6; 32],
                parent_block_hash: [0xb5; 32],
                state_root: local_state_root,
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: true,
                source_peer_id: Some(local.0),
                observed_unix_ms: 1,
            },
        );
        crate::runtime_status::set_network_runtime_native_sync_status(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeSyncStatusV1 {
                phase: NetworkRuntimeNativeSyncPhaseV1::State,
                peer_count: 1,
                starting_block: 21_900,
                current_block: 21_972,
                highest_block: 22_100,
                updated_at_unix_millis: 1,
            },
        );
        set_network_runtime_native_snap_account_range_progress_v1(
            chain_id,
            NetworkRuntimeNativeSnapAccountRangeProgressV1 {
                chain_id,
                state_root: local_state_root,
                next_account_origin: Some([0u8; 32]),
                limit: [0xff; 32],
                completed: false,
                source_peer_id: Some(local.0),
                observed_unix_ms: 1,
            },
        );

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let expected_storage_proof_node = storage_root_trie_node.clone();

        let server = thread::spawn(move || {
            fn slim_account_body(storage_root: [u8; 32], code_hash: [u8; 32]) -> Vec<u8> {
                let mut out = vec![0xf8, 0x44, 0x01, 0x80, 0xa0];
                out.extend_from_slice(&storage_root);
                out.push(0xa0);
                out.extend_from_slice(&code_hash);
                out
            }

            let account_hash = [0x34; 32];
            let bytecode = vec![0x60, 0x00];
            let code_hash = crate::eth_rlpx_code_hash_v1(bytecode.as_slice());
            let storage_slot = crate::EthRlpxSnapStorageDataV1 {
                hash: storage_slot_hash,
                body: storage_slot_body.clone(),
            };
            let root_trie_node = root_trie_node;
            let storage_root_trie_node = storage_root_trie_node;
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/snap-account-storage-code-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    22_100,
                    0,
                ),
                earliest_block: 1,
                latest_block: 22_100,
                latest_block_hash: [0x44; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            let snap_offset =
                crate::eth_rlpx_snap_base_offset_v1(peer_status.protocol_version as u8, Some(1))
                    .expect("snap offset");

            let account_request_id = loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read snap account request");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(
                    code,
                    snap_offset + crate::ETH_RLPX_SNAP_GET_ACCOUNT_RANGE_MSG
                );
                let request =
                    crate::eth_rlpx_parse_get_account_range_payload_v1(payload.as_slice())
                        .expect("parse snap get account range");
                assert_eq!(request.root, local_state_root);
                break request.request_id;
            };
            let account = crate::EthRlpxSnapAccountDataV1 {
                hash: account_hash,
                body_rlp: slim_account_body(storage_root, code_hash),
            };
            let response = crate::eth_rlpx_build_account_range_payload_v1(
                account_request_id,
                &[account],
                &[root_trie_node.clone()],
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                snap_offset + crate::ETH_RLPX_SNAP_ACCOUNT_RANGE_MSG,
                response.as_slice(),
            )
            .expect("write account range");

            let mut saw_storage = false;
            let mut saw_byte_codes = false;
            let mut saw_trie_nodes = false;
            let mut trie_nodes_requests = 0usize;
            while !(saw_storage && saw_byte_codes && saw_trie_nodes) {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read snap follow-up request");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                if code == snap_offset + crate::ETH_RLPX_SNAP_GET_STORAGE_RANGES_MSG {
                    let request =
                        crate::eth_rlpx_parse_get_storage_ranges_payload_v1(payload.as_slice())
                            .expect("parse get storage ranges");
                    assert_eq!(request.root, local_state_root);
                    assert_eq!(request.accounts, vec![account_hash]);
                    assert!(request.origin.is_empty());
                    assert!(request.limit.is_empty());
                    let slotsets = vec![vec![storage_slot.clone()]];
                    let response = crate::eth_rlpx_build_storage_ranges_payload_v1(
                        request.request_id,
                        slotsets.as_slice(),
                        &[storage_root_trie_node.clone()],
                    );
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        snap_offset + crate::ETH_RLPX_SNAP_STORAGE_RANGES_MSG,
                        response.as_slice(),
                    )
                    .expect("write storage ranges");
                    saw_storage = true;
                    continue;
                }
                if code == snap_offset + crate::ETH_RLPX_SNAP_GET_BYTE_CODES_MSG {
                    let request =
                        crate::eth_rlpx_parse_get_byte_codes_payload_v1(payload.as_slice())
                            .expect("parse get byte codes");
                    assert_eq!(request.hashes, vec![code_hash]);
                    let response = crate::eth_rlpx_build_byte_codes_payload_v1(
                        request.request_id,
                        &[bytecode.clone()],
                    );
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        snap_offset + crate::ETH_RLPX_SNAP_BYTE_CODES_MSG,
                        response.as_slice(),
                    )
                    .expect("write byte codes");
                    saw_byte_codes = true;
                    continue;
                }
                if code == snap_offset + crate::ETH_RLPX_SNAP_GET_TRIE_NODES_MSG {
                    trie_nodes_requests = trie_nodes_requests.saturating_add(1);
                    let request =
                        crate::eth_rlpx_parse_get_trie_nodes_payload_v1(payload.as_slice())
                            .expect("parse get trie nodes");
                    assert_eq!(request.root, local_state_root);
                    let response = if trie_nodes_requests == 1 {
                        assert_eq!(
                            request.paths,
                            vec![vec![vec![0_u8]], vec![account_hash.to_vec(), vec![0_u8]]]
                        );
                        crate::eth_rlpx_build_trie_nodes_payload_v1(
                            request.request_id,
                            &[storage_root_trie_node.clone()],
                        )
                    } else {
                        assert_eq!(trie_nodes_requests, 2);
                        assert_eq!(request.paths, vec![vec![vec![0_u8]]]);
                        saw_trie_nodes = true;
                        crate::eth_rlpx_build_trie_nodes_payload_v1(
                            request.request_id,
                            &[root_trie_node.clone()],
                        )
                    };
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        snap_offset + crate::ETH_RLPX_SNAP_TRIE_NODES_MSG,
                        response.as_slice(),
                    )
                    .expect("write trie nodes");
                    continue;
                }
                panic!("unexpected snap follow-up code {code}");
            }
            done_tx.send(()).expect("signal snap follow-up gate");
            thread::sleep(Duration::from_millis(50));
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.sync_request_interval_ms = 1;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        let started = std::time::Instant::now();
        let mut followed_up = false;
        while started.elapsed() < Duration::from_secs(5) {
            if done_rx.try_recv().is_ok() {
                followed_up = true;
                break;
            }
            let _ = worker
                .drive_real_network_once()
                .expect("snap follow-up tick");
            if done_rx.try_recv().is_ok() {
                followed_up = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            followed_up,
            "AccountRange with state/code must drive snap follow-up requests"
        );
        let evidence = snapshot_eth_native_sync_evidence(chain_id);
        assert!(evidence.snap_pull_seen);
        assert!(evidence.snap_response_seen);
        let account = crate::get_network_runtime_native_snap_account_snapshot_v1(
            chain_id,
            local_state_root,
            [0x34; 32],
        )
        .expect("snap account snapshot");
        assert_eq!(account.storage_root, Some(storage_root));
        assert_eq!(
            account.code_hash,
            Some(crate::eth_rlpx_code_hash_v1(&[0x60, 0x00]))
        );
        assert!(account.has_storage);
        assert!(account.has_code);
        let storage = crate::get_network_runtime_native_snap_account_storage_snapshot_v1(
            chain_id,
            local_state_root,
            [0x34; 32],
        )
        .expect("snap storage snapshot");
        assert_eq!(storage.slots.len(), 1);
        assert_eq!(storage.slots[0].hash, [0x37; 32]);
        assert_eq!(storage.slots[0].body, vec![0x80]);
        assert_eq!(storage.proof_nodes, vec![expected_storage_proof_node]);
        let code = crate::get_network_runtime_native_snap_code_snapshot_v1(
            chain_id,
            crate::eth_rlpx_code_hash_v1(&[0x60, 0x00]),
        )
        .expect("snap code snapshot");
        assert_eq!(code.code, vec![0x60, 0x00]);
        let trie_node = crate::get_network_runtime_native_snap_trie_node_snapshot_v1(
            chain_id,
            local_state_root,
            &[vec![0_u8]],
        )
        .expect("snap root trie node snapshot");
        assert_eq!(trie_node.node_hash, local_state_root);
        let storage_trie_node = crate::get_network_runtime_native_snap_trie_node_snapshot_v1(
            chain_id,
            local_state_root,
            &[vec![0x34; 32], vec![0_u8]],
        )
        .expect("snap storage root trie node snapshot");
        assert_eq!(storage_trie_node.node_hash, storage_root);

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_new_block_gate_v3() {
        let chain_id = 9_929_u64;
        let local = NodeId(1_330);
        let remote = NodeId(1_331);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 120,
                current_block: 120,
                highest_block: 120,
            },
        );

        let empty_root = crate::eth_rlpx_empty_trie_root_v1();
        let empty_ommers_hash = crate::eth_rlpx_empty_ommers_hash_v1();
        let raw_tx = crate::eth_rlpx_decode_hex_v1(
            "f86c098504a817c800825208943535353535353535353535353535353535353535880de0b6b3a76400008025a028ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276a067cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83",
        )
        .expect("decode new block raw transaction");
        let expected_tx_hash = crate::eth_rlpx_transaction_hash_v1(raw_tx.as_slice());
        let body_payload = crate::EthRlpxBlockBodyPayloadV1 {
            tx_rlp_items: vec![raw_tx],
            ommer_header_rlp_items: Vec::new(),
            withdrawal_rlp_items: Some(Vec::new()),
        };
        let receipt_blocks = vec![vec![vec![0xc0]]];
        let header_record = crate::EthRlpxBlockHeaderRecordV1 {
            number: 121,
            hash: [0u8; 32],
            parent_hash: [0x90; 32],
            state_root: [0x91; 32],
            transactions_root: crate::eth_rlpx_transactions_root_from_raw_txs_v1(
                body_payload.tx_rlp_items.as_slice(),
            ),
            receipts_root: crate::eth_rlpx_receipts_root_from_raw_receipts_v1(&receipt_blocks[0]),
            ommers_hash: empty_ommers_hash,
            logs_bloom: vec![0u8; 256],
            gas_limit: Some(30_000_000),
            gas_used: Some(21_000),
            timestamp: Some(1_234_568),
            base_fee_per_gas: Some(15),
            withdrawals_root: Some(empty_root),
            blob_gas_used: None,
            excess_blob_gas: None,
            block_access_list_hash: None,
            raw_rlp: None,
        };
        let parsed_new_block = crate::eth_rlpx_parse_new_block_payload_v1(
            crate::eth_rlpx_build_new_block_payload_v1(&header_record, &body_payload, 1_000)
                .as_slice(),
        )
        .expect("derive new block hash");
        let new_block_hash = parsed_new_block.header.hash;
        let server_receipt_blocks = receipt_blocks.clone();

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };

        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/new-block-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    120,
                    0,
                ),
                earliest_block: 120,
                latest_block: 120,
                latest_block_hash: [0x42; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(
                peer_status.protocol_version,
                crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32
            );

            let new_block =
                crate::eth_rlpx_build_new_block_payload_v1(&header_record, &body_payload, 1_000);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_NEW_BLOCK_MSG,
                new_block.as_slice(),
            )
            .expect("write new block");

            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read new block follow-up");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(
                    code,
                    crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_GET_RECEIPTS_MSG
                );
                let request = crate::eth_rlpx_parse_get_receipts_payload_v1(payload.as_slice())
                    .expect("parse new block get receipts");
                assert_eq!(request.hashes, vec![new_block_hash]);
                let receipts = crate::eth_rlpx_build_receipts_payload_v1(
                    request.request_id,
                    false,
                    server_receipt_blocks.as_slice(),
                    70,
                );
                crate::eth_rlpx_write_wire_frame_v1(
                    &mut accepted,
                    &mut responder.session,
                    crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_RECEIPTS_MSG,
                    receipts.as_slice(),
                )
                .expect("write new block receipts");
                thread::sleep(Duration::from_millis(250));
                break;
            }
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.sync_request_interval_ms = u64::MAX;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        let started = std::time::Instant::now();
        let mut header_updates = report0.header_updates;
        let mut body_updates = report0.body_updates;
        let mut receipt_updates = report0.receipt_updates;
        while started.elapsed() < Duration::from_secs(2)
            && (header_updates == 0 || body_updates == 0 || receipt_updates == 0)
        {
            let report = worker.drive_real_network_once().expect("new block tick");
            header_updates = header_updates.saturating_add(report.header_updates);
            body_updates = body_updates.saturating_add(report.body_updates);
            receipt_updates = receipt_updates.saturating_add(report.receipt_updates);
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(header_updates, 1);
        assert_eq!(body_updates, 1);
        assert_eq!(receipt_updates, 1);
        let header =
            get_network_runtime_native_header_snapshot_v1(chain_id).expect("header snapshot");
        assert_eq!(header.number, 121);
        assert_eq!(header.hash, new_block_hash);
        let body = get_network_runtime_native_body_snapshot_v1(chain_id).expect("body snapshot");
        assert_eq!(body.block_hash, new_block_hash);
        assert_eq!(body.tx_hashes, vec![expected_tx_hash]);
        assert_eq!(body.withdrawal_count, Some(0));
        let sync_status = get_network_runtime_sync_status(chain_id).expect("sync status");
        assert_eq!(sync_status.highest_block, 121);

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_block_range_update_gate_v3() {
        let chain_id = 9_945_u64;
        let local = NodeId(1_450);
        let remote = NodeId(1_451);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 120,
                current_block: 120,
                highest_block: 120,
            },
        );

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };

        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/block-range-update-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    120,
                    0,
                ),
                earliest_block: 120,
                latest_block: 120,
                latest_block_hash: [0x42; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);

            let update = crate::EthRlpxBlockRangeUpdateV1 {
                earliest_block: 64,
                latest_block: 512,
                latest_block_hash: [0x51; 32],
            };
            let payload = crate::eth_rlpx_build_block_range_update_payload_v1(update);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_BLOCK_RANGE_UPDATE_MSG,
                payload.as_slice(),
            )
            .expect("write block range update");
            thread::sleep(Duration::from_millis(250));
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.sync_request_interval_ms = u64::MAX;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(2) {
            let sync_status = get_network_runtime_sync_status(chain_id).expect("sync status");
            if sync_status.highest_block >= 512 {
                break;
            }
            let _ = worker
                .drive_real_network_once()
                .expect("block range update tick");
            thread::sleep(Duration::from_millis(5));
        }

        let sync_status = get_network_runtime_sync_status(chain_id).expect("sync status");
        assert_eq!(sync_status.highest_block, 512);
        let peer_heads = get_network_runtime_peer_heads_top_k(chain_id, 1);
        assert_eq!(peer_heads, vec![(remote.0, 512)]);
        let snapshot =
            snapshot_network_runtime_eth_peer_sessions_for_peers_v1(chain_id, &[remote])[0].clone();
        assert_eq!(snapshot.last_head_height, 512);

        server.join().expect("server join");
    }

    #[test]
    fn evm_protocol_observable_equivalence_network_rlpx_new_block_hashes_gate_v3() {
        let chain_id = 9_925_u64;
        let local = NodeId(1_290);
        let remote = NodeId(1_291);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 120,
                current_block: 120,
                highest_block: 120,
            },
        );

        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        let (done_tx, done_rx) = std::sync::mpsc::channel();

        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/new-block-hashes-gate",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id,
                genesis_hash: crate::eth_chain_config_genesis_hash_v1(chain_id),
                fork_id: crate::build_eth_fork_id_from_chain_config_v1(
                    &crate::resolve_eth_chain_config_v1(chain_id),
                    120,
                    0,
                ),
                earliest_block: 120,
                latest_block: 120,
                latest_block_hash: [0x42; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (peer_status_code, peer_status_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status =
                crate::eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                    .expect("parse peer status");
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(
                peer_status.protocol_version,
                crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32
            );

            let announced_hash = [0x91; 32];
            let announcement = crate::eth_rlpx_build_new_block_hashes_payload_v1(&[
                crate::EthRlpxNewBlockHashV1 {
                    hash: announced_hash,
                    number: 121,
                },
            ]);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_NEW_BLOCK_HASHES_MSG,
                announcement.as_slice(),
            )
            .expect("write new block hashes");

            loop {
                let (code, payload) =
                    crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                        .expect("read post-announcement worker frame");
                if code == crate::ETH_RLPX_P2P_PING_MSG {
                    crate::eth_rlpx_write_wire_frame_v1(
                        &mut accepted,
                        &mut responder.session,
                        crate::ETH_RLPX_P2P_PONG_MSG,
                        &[],
                    )
                    .expect("write pong");
                    continue;
                }
                assert_eq!(
                    code,
                    crate::ETH_RLPX_BASE_PROTOCOL_OFFSET
                        + crate::ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG
                );
                let request =
                    crate::eth_rlpx_parse_get_block_headers_payload_v1(payload.as_slice())
                        .expect("parse get block headers after new block hashes");
                assert_eq!(request.start_height, 121);
                assert_eq!(request.max_headers, 1);
                done_tx
                    .send(announced_hash)
                    .expect("signal new block hashes gate");
                break;
            }
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        budget.sync_request_interval_ms = 1;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report0 = worker.drive_real_network_once().expect("connect tick");
        assert_eq!(report0.connected_peers, 1);
        let started = std::time::Instant::now();
        let mut requested = false;
        while started.elapsed() < Duration::from_secs(2) {
            if done_rx.try_recv().is_ok() {
                requested = true;
                break;
            }
            let _ = worker
                .drive_real_network_once()
                .expect("new block hashes tick");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            requested,
            "NewBlockHashes must trigger a follow-up header request"
        );
        let sync_status = get_network_runtime_sync_status(chain_id).expect("sync status");
        assert_eq!(sync_status.highest_block, 121);

        server.join().expect("server join");
    }

    #[test]
    fn real_rlpx_peer_worker_rejects_wrong_network_status() {
        let chain_id = 9_918_u64;
        let local = NodeId(1_220);
        let remote = NodeId(1_221);
        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };
        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/transport-test",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            let wrong_status = crate::EthRlpxStatusV1 {
                protocol_version: crate::ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION as u32,
                network_id: chain_id + 1,
                genesis_hash: [0u8; 32],
                fork_id: crate::EthForkIdV1 {
                    hash: [0x2d, 0x10, 0xff, 0xf0],
                    next: 0,
                },
                earliest_block: 0,
                latest_block: 120,
                latest_block_hash: [0x42; 32],
            };
            let status_payload = crate::eth_rlpx_build_status_payload_v1(wrong_status);
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write responder status");
            let (code, payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read disconnect");
            assert_eq!(code, crate::ETH_RLPX_P2P_DISCONNECT_MSG);
            assert_eq!(
                crate::eth_rlpx_parse_disconnect_reason_v1(payload.as_slice()),
                Some(0x03)
            );
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 1;
        budget.active_native_peer_hard_limit = 1;
        let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
            chain_id,
            local_node: local,
            peers: vec![remote],
            peer_endpoints: vec![endpoint],
            recv_budget: 1,
            sync_target_fanout: 1,
            budget_hooks: budget,
        });

        let report = worker
            .drive_real_network_once()
            .expect("wrong-network peer should be isolated into report");
        assert_eq!(report.failed_bootstrap_peers, 1);
        assert_eq!(report.peer_failures.len(), 1);
        assert_eq!(
            report.peer_failures[0].phase,
            EthFullnodeNativePeerDrivePhaseV1::Bootstrap
        );
        assert_eq!(
            report.peer_failures[0].class,
            EthFullnodeNativePeerFailureClassV1::Decode
        );
        assert!(report.peer_failures[0].error.contains("wrong_network"));

        server.join().expect("server join");
    }

    #[test]
    fn real_rlpx_peer_worker_records_decode_failures_in_lifecycle_state() {
        let chain_id = 9_918_001_u64;
        let local = NodeId(1_222);
        let remote = NodeId(1_223);
        let responder_signing = k256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = crate::eth_rlpx_pubkey_from_nodekey_bytes_v1(&responder_nodekey)
            .expect("derive responder pubkey");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind rlpx listener");
        let listen_addr = listener.local_addr().expect("rlpx listener addr");
        let endpoint = PluginPeerEndpoint {
            endpoint: format!(
                "enode://{}@{}",
                responder_pub
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                listen_addr
            ),
            node_hint: remote.0,
            addr_hint: listen_addr.to_string(),
        };

        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept rlpx");
            accepted
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set server read timeout");
            accepted
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("set server write timeout");
            let mut responder = crate::eth_rlpx_handshake_responder_with_nodekey_v1(
                &responder_nodekey,
                &mut accepted,
            )
            .expect("responder handshake");
            let (hello_code, hello_payload) =
                crate::eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read initiator hello");
            assert_eq!(hello_code, crate::ETH_RLPX_P2P_HELLO_MSG);
            let initiator_hello = crate::eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice())
                .expect("parse initiator hello");
            let responder_hello = crate::eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                crate::default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/decode-test",
                listen_addr.port().into(),
            );
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write responder hello");
            if initiator_hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }
            crate::eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                crate::ETH_RLPX_BASE_PROTOCOL_OFFSET + crate::ETH_RLPX_ETH_STATUS_MSG,
                &[0x01, 0x02, 0x03],
            )
            .expect("write malformed status");
        });

        let err = connect_eth_fullnode_native_rlpx_peer_v1(chain_id, local, remote, &endpoint)
            .expect_err("malformed status must fail");
        assert!(matches!(err, NetworkError::Decode(_)));
        let snapshot =
            snapshot_network_runtime_eth_peer_sessions_for_peers_v1(chain_id, &[remote])[0].clone();
        assert_eq!(
            snapshot.last_failure_class,
            Some(crate::EthPeerFailureClassV1::DecodeFailure)
        );
        assert_eq!(
            snapshot.last_failure_reason_name.as_deref(),
            Some("status_payload_decode_failed")
        );
        assert_eq!(
            snapshot.lifecycle_stage,
            crate::EthPeerLifecycleStageV1::PermanentlyRejected
        );
        assert!(snapshot.permanently_rejected);
        server.join().expect("server join");
    }

    #[test]
    #[ignore = "live mainnet peer smoke"]
    fn live_mainnet_peer_smoke_updates_native_preferred_views() {
        let chain_id = std::env::var("NOVOVM_ETH_LIVE_SMOKE_CHAIN_ID")
            .ok()
            .and_then(|raw| raw.parse::<u64>().ok())
            .unwrap_or(1);
        let local = NodeId(
            std::env::var("NOVOVM_ETH_LIVE_SMOKE_LOCAL_NODE")
                .ok()
                .and_then(|raw| raw.parse::<u64>().ok())
                .unwrap_or(9_990_001),
        );
        let max_peers = std::env::var("NOVOVM_ETH_LIVE_SMOKE_MAX_PEERS")
            .ok()
            .and_then(|raw| raw.parse::<usize>().ok())
            .unwrap_or(4)
            .clamp(1, 4);
        let ticks = std::env::var("NOVOVM_ETH_LIVE_SMOKE_TICKS")
            .ok()
            .and_then(|raw| raw.parse::<usize>().ok())
            .unwrap_or(8)
            .clamp(2, 24);
        let sleep_ms = std::env::var("NOVOVM_ETH_LIVE_SMOKE_SLEEP_MS")
            .ok()
            .and_then(|raw| raw.parse::<u64>().ok())
            .unwrap_or(600)
            .clamp(50, 5_000);
        let peer_endpoints = parse_live_smoke_peer_endpoints()
            .into_iter()
            .take(max_peers)
            .collect::<Vec<_>>();
        assert!(
            !peer_endpoints.is_empty(),
            "no live smoke enodes resolved from NOVOVM_ETH_LIVE_SMOKE_ENODES/defaults"
        );

        let mut failures = Vec::<String>::new();
        for endpoint in peer_endpoints {
            clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
            set_network_runtime_sync_status(
                chain_id,
                NetworkRuntimeSyncStatus {
                    peer_count: 0,
                    starting_block: 0,
                    current_block: 0,
                    highest_block: 0,
                },
            );

            let mut budget = default_eth_fullnode_budget_hooks_v1();
            budget.active_native_peer_soft_limit = 1;
            budget.active_native_peer_hard_limit = 1;
            budget.sync_pull_headers_batch = 16;
            budget.sync_pull_bodies_batch = 16;
            let worker = EthFullnodeNativePeerWorkerV1::new(EthFullnodeNativePeerWorkerConfigV1 {
                chain_id,
                local_node: local,
                peers: vec![NodeId(endpoint.node_hint.max(1))],
                peer_endpoints: vec![endpoint.clone()],
                recv_budget: 1,
                sync_target_fanout: 1,
                budget_hooks: budget,
            });

            let mut saw_status = false;
            let mut saw_sync_request = false;
            let mut saw_body_update = false;
            let mut last_error = None::<String>;
            for _ in 0..ticks {
                let report = worker
                    .drive_real_network_once()
                    .expect("best-effort real worker should not short-circuit live smoke");
                if report.status_updates > 0 {
                    saw_status = true;
                }
                if report.sync_requests > 0 {
                    saw_sync_request = true;
                }
                if report.body_updates > 0 {
                    saw_body_update = true;
                }
                if let Some(failure) = report.peer_failures.last() {
                    last_error = Some(format!(
                        "{}:{}:{}",
                        failure.phase.as_str(),
                        failure.class.as_str(),
                        failure.error
                    ));
                }
                if get_network_runtime_native_header_snapshot_v1(chain_id).is_some()
                    && get_network_runtime_native_body_snapshot_v1(chain_id).is_some()
                {
                    let sync_status =
                        get_network_runtime_sync_status(chain_id).expect("live sync status");
                    let native_sync =
                        get_network_runtime_native_sync_status(chain_id).expect("live native sync");
                    let native_block = snapshot_eth_fullnode_native_head_block_object_v1(chain_id)
                        .expect("live native block");
                    let native_canonical_chain =
                        snapshot_network_runtime_native_canonical_chain_v1(chain_id);
                    let head_view = derive_eth_fullnode_head_view_with_native_preference_v1(
                        None,
                        Some(&native_block),
                        native_canonical_chain.as_ref(),
                        Some(native_sync),
                    )
                    .expect("live head view");
                    let sync_view = derive_eth_fullnode_sync_view_with_native_preference_v1(
                        None,
                        Some(&native_block),
                        native_canonical_chain.as_ref(),
                        Some(sync_status),
                        Some(native_sync),
                    )
                    .expect("live sync view");
                    assert!(saw_status, "live smoke never observed remote Status");
                    assert!(
                        saw_sync_request,
                        "live smoke never dispatched GetBlockHeaders"
                    );
                    assert!(saw_body_update, "live smoke never ingested BlockBodies");
                    assert!(
                        head_view.block_number > 0,
                        "live head view block number stayed zero"
                    );
                    assert!(
                        matches!(
                            head_view.source,
                            crate::EthFullnodeBlockViewSource::NativeChainSync
                        ),
                        "live head view did not prioritize native chain sync"
                    );
                    assert!(
                        sync_view.highest_block_number >= head_view.block_number,
                        "live sync view highest block did not cover head view"
                    );
                    assert!(
                        native_block
                            .body
                            .as_ref()
                            .is_some_and(|body| body.body_available),
                        "live native block object did not include an available body"
                    );
                    eprintln!(
                        "live_smoke_ok endpoint={} head={} hash=0x{} highest={} source={} body_available={}",
                        endpoint.addr_hint,
                        head_view.block_number,
                        head_view
                            .block_hash
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<String>(),
                        sync_view.highest_block_number,
                        head_view.source.as_str(),
                        native_block
                            .body
                            .as_ref()
                            .is_some_and(|body| body.body_available),
                    );
                    return;
                }
                thread::sleep(Duration::from_millis(sleep_ms));
            }
            failures.push(format!(
                "{}:{}",
                endpoint.addr_hint,
                last_error.unwrap_or_else(|| {
                    format!(
                        "status={} sync_request={} body_update={} header_snapshot={} body_snapshot={}",
                        saw_status,
                        saw_sync_request,
                        saw_body_update,
                        get_network_runtime_native_header_snapshot_v1(chain_id).is_some(),
                        get_network_runtime_native_body_snapshot_v1(chain_id).is_some(),
                    )
                })
            ));
        }
        panic!(
            "live mainnet smoke failed for all candidate peers: {}",
            failures.join(" | ")
        );
    }

    #[test]
    fn evm_native_status_response_triggers_header_pull_from_runtime_gap() {
        let chain_id = 9_913_u64;
        let local = NodeId(996);
        let remote = NodeId(997);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 50,
                current_block: 50,
                highest_block: 77,
            },
        );

        let status = ProtocolMessage::EvmNative(EvmNativeMessage::Status {
            from: remote,
            chain_id,
            total_difficulty: 77,
            head_height: 77,
            head_hash: [0xaa; 32],
            genesis_hash: [0u8; 32],
        });
        let (to, response) =
            maybe_build_evm_native_sync_response(chain_id, local, &status).expect("response");
        assert_eq!(to, remote);
        let planned = plan_network_runtime_sync_pull_window(chain_id).expect("planned window");
        let ProtocolMessage::EvmNative(EvmNativeMessage::GetBlockHeaders {
            from,
            start_height,
            max,
            skip,
            reverse,
        }) = response
        else {
            panic!("expected get block headers");
        };
        assert_eq!(from, local);
        assert_eq!(start_height, planned.from_block);
        assert_eq!(max, planned.to_block - planned.from_block + 1);
        assert_eq!(skip, 0);
        assert!(!reverse);
    }

    #[test]
    fn evm_native_block_headers_response_triggers_body_pull_request() {
        let chain_id = 9_914_u64;
        let local = NodeId(998);
        let remote = NodeId(999);
        clear_network_runtime_native_snapshots_for_chain_v1(chain_id);
        let block_headers = ProtocolMessage::EvmNative(EvmNativeMessage::BlockHeaders {
            from: remote,
            headers: vec![
                EvmNativeBlockHeaderWireV1 {
                    number: 60,
                    hash: [0x61; 32],
                    parent_hash: [0x60; 32],
                    state_root: [0x71; 32],
                    transactions_root: [0x72; 32],
                    receipts_root: [0x73; 32],
                    ommers_hash: [0x74; 32],
                    logs_bloom: vec![0u8; 256],
                    gas_limit: None,
                    gas_used: None,
                    timestamp: None,
                    base_fee_per_gas: None,
                    withdrawals_root: None,
                    blob_gas_used: None,
                    excess_blob_gas: None,
                    block_access_list_hash: None,
                },
                EvmNativeBlockHeaderWireV1 {
                    number: 61,
                    hash: [0x62; 32],
                    parent_hash: [0x61; 32],
                    state_root: [0x81; 32],
                    transactions_root: [0x82; 32],
                    receipts_root: [0x83; 32],
                    ommers_hash: [0x84; 32],
                    logs_bloom: vec![0u8; 256],
                    gas_limit: None,
                    gas_used: None,
                    timestamp: None,
                    base_fee_per_gas: None,
                    withdrawals_root: None,
                    blob_gas_used: None,
                    excess_blob_gas: None,
                    block_access_list_hash: None,
                },
            ],
        });
        let (to, response) = maybe_build_evm_native_sync_response(chain_id, local, &block_headers)
            .expect("body pull response");
        assert_eq!(to, remote);
        let ProtocolMessage::EvmNative(EvmNativeMessage::GetBlockBodies { from, hashes }) =
            response
        else {
            panic!("expected get block bodies");
        };
        assert_eq!(from, local);
        assert_eq!(hashes, vec![[0x61; 32], [0x62; 32]]);

        maybe_update_runtime_sync_from_protocol_message(
            chain_id,
            &block_headers,
            Some(remote.0),
            None,
        );
        let retained = snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 8);
        let block_60 = retained
            .iter()
            .find(|block| block.number == 60)
            .expect("first header retained");
        let block_61 = retained
            .iter()
            .find(|block| block.number == 61)
            .expect("second header retained");
        assert_eq!(block_60.transactions_root, Some([0x72; 32]));
        assert_eq!(block_60.receipts_root, Some([0x73; 32]));
        assert_eq!(block_61.transactions_root, Some([0x82; 32]));
        assert_eq!(block_61.receipts_root, Some([0x83; 32]));
    }
}
