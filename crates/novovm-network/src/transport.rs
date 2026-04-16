#![forbid(unsafe_code)]

use crate::{
    build_eth_fullnode_native_bodies_request_v1, build_eth_fullnode_native_bootstrap_messages_v1,
    build_eth_fullnode_native_rlpx_status_v1, build_eth_fullnode_native_status_message_v1,
    build_eth_fullnode_native_sync_request_v1, default_eth_rlpx_capabilities_v1,
    derive_eth_fullnode_head_view_with_native_preference_v1,
    derive_eth_fullnode_sync_view_with_native_preference_v1, eth_rlpx_build_disconnect_payload_v1,
    eth_rlpx_build_get_block_bodies_payload_v1, eth_rlpx_build_get_block_headers_payload_v1,
    eth_rlpx_build_hello_payload_v1, eth_rlpx_build_status_payload_v1,
    eth_rlpx_build_transactions_payload_v1, eth_rlpx_default_client_name_v1,
    eth_rlpx_default_listen_port_v1, eth_rlpx_disconnect_reason_name_v1,
    eth_rlpx_handshake_initiator_v1, eth_rlpx_hello_profile_v1,
    eth_rlpx_parse_block_bodies_payload_v1, eth_rlpx_parse_block_headers_payload_v1,
    eth_rlpx_parse_disconnect_reason_v1, eth_rlpx_parse_hello_payload_v1,
    eth_rlpx_parse_status_payload_v1, eth_rlpx_parse_transactions_payload_v1,
    eth_rlpx_read_wire_frame_v1, eth_rlpx_select_shared_eth_version_v1,
    eth_rlpx_select_shared_snap_version_v1, eth_rlpx_write_wire_frame_v1,
    get_network_runtime_native_body_snapshot_v1, get_network_runtime_native_head_snapshot_v1,
    get_network_runtime_native_header_snapshot_v1, get_network_runtime_native_sync_status,
    get_network_runtime_peer_heads_top_k, get_network_runtime_sync_status,
    has_network_runtime_eth_peer_session, mark_network_runtime_eth_peer_session_ready_v1,
    observe_eth_native_bodies_pull, observe_eth_native_bodies_response,
    observe_eth_native_discovery, observe_eth_native_headers_pull,
    observe_eth_native_headers_response, observe_eth_native_hello, observe_eth_native_rlpx_auth,
    observe_eth_native_rlpx_auth_ack, observe_eth_native_snap_pull,
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
    set_network_runtime_native_body_snapshot_v1, set_network_runtime_native_budget_hooks_v1,
    set_network_runtime_native_head_snapshot_v1, set_network_runtime_native_header_snapshot_v1,
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
    snapshot_network_runtime_native_pending_txs_v1, unregister_network_runtime_peer,
    upsert_network_runtime_eth_peer_session, validate_eth_chain_config_peer_status_v1,
    write_eth_fullnode_native_worker_runtime_snapshot_default_path_v1,
    EthChainConfigPeerValidationReasonV1, EthFullnodeBudgetHooksV1,
    EthFullnodeNativePeerFailureSnapshotV1, EthFullnodeNativeWorkerRuntimeSnapshotV1,
    EthPeerLifecycleSummaryV1, EthPeerSelectionQualitySummaryV1,
    EthPeerSelectionRoundObservationV1, EthPeerSelectionScoreV1, EthRlpxBlockBodiesResponseV1,
    EthRlpxBlockHeadersResponseV1, EthRlpxFrameSessionV1, EthRlpxStatusV1,
    NetworkRuntimeNativePendingTxPropagationStopReasonV1, NetworkRuntimeNativeSyncPhaseV1,
    ETH_FULLNODE_NATIVE_WORKER_RUNTIME_SCHEMA_V1, ETH_RLPX_BASE_PROTOCOL_OFFSET,
    ETH_RLPX_ETH_BLOCK_BODIES_MSG, ETH_RLPX_ETH_BLOCK_HEADERS_MSG,
    ETH_RLPX_ETH_GET_BLOCK_BODIES_MSG, ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG, ETH_RLPX_ETH_STATUS_MSG,
    ETH_RLPX_ETH_TRANSACTIONS_MSG, ETH_RLPX_P2P_DISCONNECT_MSG, ETH_RLPX_P2P_HELLO_MSG,
    ETH_RLPX_P2P_PING_MSG, ETH_RLPX_P2P_PONG_MSG,
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
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

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
        let bootstrap_window = soft_limit.max(1);

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
        let plan = self.plan();
        set_network_runtime_native_budget_hooks_v1(plan.chain_id, plan.budget_hooks.clone());
        let mut report = EthFullnodeNativeRealDriveReportV1 {
            scheduled_bootstrap_peers: plan.bootstrap_peers.len(),
            scheduled_sync_peers: plan.sync_peers.len(),
            lifecycle_summary: plan.lifecycle_summary.clone(),
            selection_quality_summary: plan.selection_quality_summary.clone(),
            ..EthFullnodeNativeRealDriveReportV1::default()
        };
        for &peer in plan.bootstrap_peers.iter() {
            let Some(endpoint) = self.endpoint_for_peer(peer) else {
                report.skipped_missing_endpoint_peers =
                    report.skipped_missing_endpoint_peers.saturating_add(1);
                continue;
            };
            report.attempted_bootstrap_peers = report.attempted_bootstrap_peers.saturating_add(1);
            match connect_eth_fullnode_native_rlpx_peer_v1(
                plan.chain_id,
                plan.local_node,
                peer,
                &endpoint,
            ) {
                Ok(()) => {
                    report.connected_peers = report.connected_peers.saturating_add(1);
                    report.ready_peers = report.ready_peers.saturating_add(1);
                    report.status_updates = report.status_updates.saturating_add(1);
                }
                Err(err) => {
                    report.failed_bootstrap_peers = report.failed_bootstrap_peers.saturating_add(1);
                    report
                        .peer_failures
                        .push(build_eth_fullnode_peer_failure_report_v1(
                            plan.chain_id,
                            peer,
                            Some(&endpoint),
                            EthFullnodeNativePeerDrivePhaseV1::Bootstrap,
                            &err,
                        ));
                }
            }
        }
        for &peer in plan.sync_peers.iter() {
            let Some(endpoint) = self.endpoint_for_peer(peer) else {
                report.skipped_missing_endpoint_peers =
                    report.skipped_missing_endpoint_peers.saturating_add(1);
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
                    report.status_updates = report
                        .status_updates
                        .saturating_add(peer_report.status_updates);
                    report.header_updates = report
                        .header_updates
                        .saturating_add(peer_report.header_updates);
                    report.body_updates =
                        report.body_updates.saturating_add(peer_report.body_updates);
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
        observe_network_runtime_eth_peer_selection_round_v1(
            plan.chain_id,
            EthPeerSelectionRoundObservationV1 {
                peers: &plan.candidate_peers,
                selected_bootstrap_peers: &plan.bootstrap_peers,
                selected_sync_peers: &plan.sync_peers,
                header_success_peers: &report.header_updated_peer_ids,
                body_success_peers: &report.body_updated_peer_ids,
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
    pub connected_peers: usize,
    pub ready_peers: usize,
    pub status_updates: usize,
    pub header_updates: usize,
    pub body_updates: usize,
    pub header_updated_peer_ids: Vec<u64>,
    pub body_updated_peer_ids: Vec<u64>,
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
    remote_status: EthRlpxStatusV1,
    last_sync_request_unix_ms: u64,
    last_headers_request_id: Option<u64>,
    last_bodies_request_id: Option<u64>,
    last_tx_broadcast_unix_ms: u64,
    pending_body_headers: Vec<(u64, [u8; 32])>,
}

type EthFullnodeNativeRlpxSessionKeyV1 = (u64, u64);
type EthFullnodeNativeRlpxSessionMapV1 =
    HashMap<EthFullnodeNativeRlpxSessionKeyV1, EthFullnodeNativeRlpxLivePeerSessionV1>;
static ETH_FULLNODE_NATIVE_RLPX_SESSIONS: OnceLock<Mutex<EthFullnodeNativeRlpxSessionMapV1>> =
    OnceLock::new();

fn eth_fullnode_native_rlpx_sessions_v1() -> &'static Mutex<EthFullnodeNativeRlpxSessionMapV1> {
    ETH_FULLNODE_NATIVE_RLPX_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

static ETH_FULLNODE_NATIVE_RLPX_REQUEST_ID: OnceLock<std::sync::atomic::AtomicU64> =
    OnceLock::new();
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
        ommer_hashes: body.ommer_hashes.clone(),
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
    sync_requests: usize,
    inbound_frames: usize,
}

fn format_eth_fullnode_rlpx_disconnect_reason_v1(payload: &[u8], phase: &str) -> String {
    let reason = eth_rlpx_parse_disconnect_reason_v1(payload);
    format!(
        "rlpx_remote_disconnected_{phase}:reason_code={} reason={}",
        reason.unwrap_or(u64::MAX),
        eth_rlpx_disconnect_reason_name_v1(reason.unwrap_or(u64::MAX)),
    )
}

fn eth_fullnode_rlpx_error_is_timeout_v1(raw: &str) -> bool {
    raw.contains("timed out")
        || raw.contains("would block")
        || raw.contains("os error 10060")
        || raw.contains("os error 10035")
        || raw.contains("没有正确答复")
        || raw.contains("没有反应")
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
        NetworkError::Decode(_) => {
            observe_network_runtime_eth_peer_handshake_failure_v1(
                chain_id,
                peer_id,
                "connect_decode_failed",
            );
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

fn build_eth_fullnode_native_worker_runtime_snapshot_v1(
    plan: &EthFullnodeNativePeerWorkerPlanV1,
    report: &EthFullnodeNativeRealDriveReportV1,
) -> EthFullnodeNativeWorkerRuntimeSnapshotV1 {
    let runtime_config = resolve_eth_fullnode_native_runtime_config_v1(plan.chain_id);
    let (peer_selection_scores, selection_quality_summary, selection_long_term_summary) =
        snapshot_eth_fullnode_peer_selection_scores_v1(
            plan.chain_id,
            &plan.candidate_peers,
            &plan.bootstrap_peers,
            &plan.sync_peers,
        );
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
        &plan.candidate_peers,
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
    let mut sessions = eth_fullnode_native_rlpx_sessions_v1()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if sessions.contains_key(&key) {
        return Ok(());
    }

    let timeout = Duration::from_secs(5);
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
            observe_network_runtime_eth_peer_handshake_failure_v1(
                chain_id,
                peer.0,
                "rlpx_auth_failed",
            );
            NetworkError::Decode(format!(
                "{err}:endpoint={} hello_profile={}",
                endpoint.addr_hint, hello_profile
            ))
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
        NetworkError::Decode(format!(
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
        ))
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
    sessions.insert(
        key,
        EthFullnodeNativeRlpxLivePeerSessionV1 {
            endpoint: endpoint.clone(),
            stream,
            frame_session: handshake.session,
            _negotiated_eth_version: negotiated_eth_version.as_u8(),
            remote_status,
            last_sync_request_unix_ms: 0,
            last_headers_request_id: None,
            last_bodies_request_id: None,
            last_tx_broadcast_unix_ms: 0,
            pending_body_headers: Vec::new(),
        },
    );
    let _ = negotiated_snap;
    let _ = local_node;
    Ok(())
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

        loop {
            match eth_rlpx_read_wire_frame_v1(&mut session.stream, &mut session.frame_session) {
                Ok((code, payload)) => {
                    report.inbound_frames = report.inbound_frames.saturating_add(1);
                    if code == ETH_RLPX_P2P_PING_MSG {
                        eth_rlpx_write_wire_frame_v1(
                            &mut session.stream,
                            &mut session.frame_session,
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
                        ingest_real_rlpx_block_headers_v1(
                            chain_id,
                            peer.0,
                            session,
                            &headers,
                            &mut report,
                        )?;
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
                        ingest_real_rlpx_block_bodies_v1(
                            chain_id,
                            peer.0,
                            session,
                            &bodies,
                            &mut report,
                        );
                        continue;
                    }
                }
                Err(err) => {
                    if err.contains("timed out")
                        || err.contains("would block")
                        || err.contains("os error 10060")
                        || err.contains("os error 10035")
                        || err.contains("没有正确答复")
                        || err.contains("没有反应")
                    {
                        break;
                    }
                    observe_network_runtime_eth_peer_decode_failure_v1(
                        chain_id,
                        peer.0,
                        "frame_decode_failed",
                    );
                    return Err(NetworkError::Decode(err));
                }
            }
        }

        let now_ms = now_unix_ms();
        if !disconnected {
            if session.last_bodies_request_id.is_some()
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
            let Some(msg) = build_eth_fullnode_native_sync_request_v1(local_node, chain_id) else {
                return Ok(report);
            };
            if let ProtocolMessage::EvmNative(EvmNativeMessage::GetBlockHeaders {
                start_height,
                max,
                skip,
                reverse,
                ..
            }) = msg
            {
                let request_id = next_eth_fullnode_native_rlpx_request_id_v1();
                let payload = eth_rlpx_build_get_block_headers_payload_v1(
                    request_id,
                    start_height,
                    max,
                    skip,
                    reverse,
                );
                eth_rlpx_write_wire_frame_v1(
                    &mut session.stream,
                    &mut session.frame_session,
                    ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG,
                    payload.as_slice(),
                )
                .map_err(|err| {
                    observe_network_runtime_eth_peer_handshake_failure_v1(
                        chain_id,
                        peer.0,
                        "headers_request_write_failed",
                    );
                    NetworkError::Io(err)
                })?;
                observe_eth_native_headers_pull(chain_id);
                observe_network_runtime_eth_peer_syncing_v1(chain_id, peer.0);
                session.last_headers_request_id = Some(request_id);
                session.last_bodies_request_id = None;
                session.last_sync_request_unix_ms = now_ms;
                report.sync_requests = report.sync_requests.saturating_add(1);
            }
        }
        if !disconnected
            && now_ms.saturating_sub(session.last_tx_broadcast_unix_ms)
                >= budget_hooks.tx_broadcast_interval_ms.max(1)
        {
            dispatch_eth_fullnode_native_rlpx_tx_broadcast_v1(
                chain_id,
                local_node,
                peer,
                session,
                budget_hooks,
            )?;
            session.last_tx_broadcast_unix_ms = now_ms;
        }
    }
    if disconnected {
        sessions.remove(&(chain_id, peer.0));
    }
    if let Some(err) = disconnect_error {
        return Err(err);
    }
    Ok(report)
}

fn ingest_real_rlpx_block_headers_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    headers: &EthRlpxBlockHeadersResponseV1,
    report: &mut EthFullnodeNativeRlpxPeerTickReportV1,
) -> Result<(), NetworkError> {
    if session
        .last_headers_request_id
        .is_some_and(|request_id| request_id != headers.request_id)
    {
        return Ok(());
    }
    observe_eth_native_headers_response(chain_id);
    session.last_headers_request_id = Some(headers.request_id);
    if headers.headers.is_empty() {
        session.last_headers_request_id = None;
        return Ok(());
    }
    session.pending_body_headers = headers
        .headers
        .iter()
        .map(|header| (header.number, header.hash))
        .collect();
    if let Some(best) = headers.headers.iter().max_by_key(|header| header.number) {
        let header_wire = evm_native_header_wire_from_rlpx_header_v1(best);
        ingest_runtime_native_header_from_evm_wire(chain_id, source_peer_id, &header_wire);
        report.header_updates = report.header_updates.saturating_add(1);
    }
    let hashes = session
        .pending_body_headers
        .iter()
        .map(|(_, hash)| *hash)
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
            observe_network_runtime_eth_peer_handshake_failure_v1(
                chain_id,
                source_peer_id,
                "bodies_request_write_failed",
            );
            NetworkError::Io(err)
        })?;
        observe_eth_native_bodies_pull(chain_id);
        observe_network_runtime_eth_peer_syncing_v1(chain_id, source_peer_id);
        session.last_bodies_request_id = Some(request_id);
        session.last_sync_request_unix_ms = now_unix_ms();
        report.sync_requests = report.sync_requests.saturating_add(1);
    }
    Ok(())
}

fn ingest_real_rlpx_block_bodies_v1(
    chain_id: u64,
    source_peer_id: u64,
    session: &mut EthFullnodeNativeRlpxLivePeerSessionV1,
    bodies: &EthRlpxBlockBodiesResponseV1,
    report: &mut EthFullnodeNativeRlpxPeerTickReportV1,
) {
    if session
        .last_bodies_request_id
        .is_some_and(|request_id| request_id != bodies.request_id)
    {
        return;
    }
    observe_eth_native_bodies_response(chain_id);
    session.last_headers_request_id = None;
    for (idx, body) in bodies.bodies.iter().enumerate() {
        if let Some((number, hash)) = session.pending_body_headers.get(idx).copied() {
            let body_wire = evm_native_body_wire_from_rlpx_body_v1(number, hash, body);
            ingest_runtime_native_body_from_evm_wire(chain_id, source_peer_id, &body_wire);
            report.body_updates = report.body_updates.saturating_add(1);
        }
    }
    session.pending_body_headers.clear();
    session.last_bodies_request_id = None;
    mark_network_runtime_eth_peer_session_ready_v1(chain_id, source_peer_id, None);
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
    let payload = eth_rlpx_build_transactions_payload_v1(
        &candidates
            .iter()
            .map(|candidate| candidate.tx_payload.clone())
            .collect::<Vec<_>>(),
    );
    eth_rlpx_write_wire_frame_v1(
        &mut session.stream,
        &mut session.frame_session,
        ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_TRANSACTIONS_MSG,
        payload.as_slice(),
    )
    .map_err(|err| {
        for candidate in &candidates {
            observe_network_runtime_native_pending_tx_propagation_failure_v1(
                chain_id,
                candidate.tx_hash,
                Some(peer.0),
                NetworkRuntimeNativePendingTxPropagationStopReasonV1::IoWriteFailure,
                "transactions_dispatch",
            );
        }
        observe_network_runtime_native_pending_tx_broadcast_dispatch_v1(
            chain_id,
            peer.0,
            candidate_count,
            0,
            false,
        );
        observe_network_runtime_eth_peer_handshake_failure_v1(
            chain_id,
            peer.0,
            "transactions_write_failed",
        );
        NetworkError::Io(err)
    })?;
    for candidate in candidates {
        observe_network_runtime_native_pending_tx_propagated_with_context_v1(
            chain_id,
            candidate.tx_hash,
            Some(peer.0),
            Some("transactions_dispatch"),
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
                if let Some(header) = headers.iter().max_by_key(|header| header.number) {
                    ingest_runtime_native_header_from_evm_wire(chain_id, from.0, header);
                }
            }
            EvmNativeMessage::GetBlockBodies { from, .. } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
            }
            EvmNativeMessage::BlockBodies { from, bodies } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_eth_native_bodies_response(chain_id);
                let preferred =
                    get_network_runtime_native_head_snapshot_v1(chain_id).and_then(|head| {
                        bodies
                            .iter()
                            .find(|body| {
                                body.number == head.block_number
                                    && body.block_hash == head.block_hash
                            })
                            .cloned()
                    });
                if let Some(body) =
                    preferred.or_else(|| bodies.iter().max_by_key(|body| body.number).cloned())
                {
                    ingest_runtime_native_body_from_evm_wire(chain_id, from.0, &body);
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
    }
}

fn evm_native_block_body_wire_from_runtime_snapshot(
    snapshot: &crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1,
) -> EvmNativeBlockBodyWireV1 {
    EvmNativeBlockBodyWireV1 {
        number: snapshot.number,
        block_hash: snapshot.block_hash,
        tx_hashes: snapshot.tx_hashes.clone(),
        ommer_hashes: snapshot.ommer_hashes.clone(),
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
        ommer_hashes: body.ommer_hashes.clone(),
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
        get_network_runtime_native_body_snapshot_v1, get_network_runtime_native_head_snapshot_v1,
        get_network_runtime_native_header_snapshot_v1, get_network_runtime_native_sync_status,
        get_network_runtime_sync_status, parse_enode_endpoint,
        set_network_runtime_native_body_snapshot_v1, set_network_runtime_native_head_snapshot_v1,
        set_network_runtime_native_header_snapshot_v1, set_network_runtime_sync_status,
        snapshot_eth_fullnode_native_head_block_object_v1, snapshot_eth_native_sync_evidence,
        snapshot_network_runtime_eth_peer_sessions,
        snapshot_network_runtime_eth_peer_sessions_for_peers_v1, NetworkRuntimeSyncStatus,
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
    use std::collections::HashSet;
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    const LIVE_MAINNET_BOOTNODES: [&str; 4] = [
        "enode://d860a01f9722d78051619d1e2351aba3f43f943f6f00718d1b9baa4101932a1f5011f16bb2b1bb35db20d6fe28fa0bf09636d26a87d31de9ec6203eeedb1f666@18.138.108.67:30303",
        "enode://22a8232c3abc76a16ae9d6c3b164f98775fe226f0917b0ca871128a74a8e9630b458460865bab457221f1d448dd9791d24c4e5d88786180ac185df813a68d4de@3.209.45.79:30303",
        "enode://2b252ab6a1d0f971d9722cb839a42cb81db019ba44c08754628ab4a823487071b5695317c8ccd085219c3a03af063495b2f1da8d18218da2d6a82981b45e6ffc@65.108.70.101:30303",
        "enode://4aeb4ab6c14b23e2c4cfdce879c04b0748a20d8e9b59e25ded2a08143e265c6c25936e74cbc8e641e3312ca288673d91f2f93f8e277de3cfa444ecdaaf982052@157.90.35.166:30303",
    ];

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
                ommer_hashes: vec![[0xf1; 32]],
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
    fn native_peer_worker_plan_skips_cooldown_and_permanent_rejects() {
        let chain_id = 9_914_001_u64;
        let local = NodeId(1_105);
        let peer_a = NodeId(1_106);
        let peer_b = NodeId(1_107);
        let peer_c = NodeId(1_108);

        let _ = upsert_network_runtime_eth_peer_session(chain_id, peer_a.0, &[68, 70], &[1], None)
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
                protocol_version: 70,
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
        });

        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.active_native_peer_soft_limit = 2;
        budget.active_native_peer_hard_limit = 2;
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
        assert_eq!(report.peer_failures.len(), 1);
        assert_eq!(report.peer_failures[0].peer_id, bad_peer.0);
        assert_eq!(
            report.peer_failures[0].phase,
            EthFullnodeNativePeerDrivePhaseV1::Bootstrap
        );
        assert_eq!(
            report.peer_failures[0].class,
            EthFullnodeNativePeerFailureClassV1::AddressParse
        );
        assert_eq!(report.lifecycle_summary.permanently_rejected_count, 1);
        assert!(report.lifecycle_summary.ready_count >= 1);

        server.join().expect("server join");
    }

    #[test]
    fn native_peer_worker_prefers_highest_head_session_for_sync() {
        let chain_id = 9_915_u64;
        let local = NodeId(1_120);
        let peer_a = NodeId(1_121);
        let peer_b = NodeId(1_122);
        let _ =
            upsert_network_runtime_eth_peer_session(chain_id, peer_a.0, &[66, 68], &[1], Some(120))
                .expect("session a");
        let _ =
            upsert_network_runtime_eth_peer_session(chain_id, peer_b.0, &[66, 68], &[1], Some(240))
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
                ommer_hashes: vec![[0xb1; 32]],
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
                protocol_version: 70,
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
            let expected_local_status =
                crate::build_eth_fullnode_native_rlpx_status_v1(chain_id, 70);
            assert_eq!(peer_status.network_id, chain_id);
            assert_eq!(peer_status.protocol_version, 70);
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
        let status_after_connect = get_network_runtime_sync_status(chain_id).expect("sync status");
        assert_eq!(status_after_connect.highest_block, 120);

        let report1 = worker
            .drive_real_network_once()
            .expect("header request tick");
        assert_eq!(report1.sync_requests, 1);

        let report2 = worker
            .drive_real_network_once()
            .expect("header response tick");
        assert_eq!(report2.header_updates, 1);

        let header_snapshot =
            get_network_runtime_native_header_snapshot_v1(chain_id).expect("header snapshot");
        assert_eq!(header_snapshot.number, 120);
        let head_snapshot =
            get_network_runtime_native_head_snapshot_v1(chain_id).expect("head snapshot");
        assert_eq!(head_snapshot.block_number, 120);

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
                protocol_version: 70,
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
            crate::EthPeerLifecycleStageV1::Cooldown
        );
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
    }
}
