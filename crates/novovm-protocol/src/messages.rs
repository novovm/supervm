#![forbid(unsafe_code)]

use crate::{CheckpointId, NodeId, ShardId, TxId};
use serde::{Deserialize, Serialize};

use crate::protocol_catalog;

/// Phase 4.x operation classification (kept minimal).
///
/// Note: AOEM V3 typed-op (Set/Add/Inc) lives in AOEM types;
/// this is protocol-level coarse classing for routing/policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationClass {
    /// Commutative / idempotent (may be eligible for no-2PC policy depending on app).
    TypeA,
    /// Conditional / saga-like.
    TypeB,
    /// Strong order / non-commutative.
    TypeC,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxEnvelope {
    pub tx_id: TxId,
    pub from: NodeId,
    pub target_shards: Vec<ShardId>,
    pub op_class: OperationClass,
    pub payload: Vec<u8>,
}

/// 2PC coordinator messages (protocol-level only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TwoPcMessage {
    Propose {
        tx: TxEnvelope,
    },
    Prepare {
        tx_id: TxId,
    },
    Vote {
        tx_id: TxId,
        shard: ShardId,
        yes: bool,
    },
    Decide {
        tx_id: TxId,
        commit: bool,
    },
}

/// Finality plane messages (minimal skeleton).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FinalityMessage {
    CheckpointPropose {
        id: CheckpointId,
        from: NodeId,
        payload: Vec<u8>,
    },
    Vote {
        id: CheckpointId,
        from: NodeId,
        sig: Vec<u8>,
    },
    Cert {
        id: CheckpointId,
        from: NodeId,
        sigs: Vec<Vec<u8>>,
    },
}

/// Gossip plane messages (minimal).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMessage {
    Heartbeat { from: NodeId, shard: ShardId },
    PeerList { from: NodeId, peers: Vec<NodeId> },
}

/// Pacemaker / view-sync plane messages (network-level liveness).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PacemakerMessage {
    /// Timeout-driven view sync notice.
    ViewSync {
        from: NodeId,
        height: u64,
        view: u64,
        leader: NodeId,
    },
    /// New-view notification carrying a high QC height hint.
    NewView {
        from: NodeId,
        height: u64,
        view: u64,
        high_qc_height: u64,
    },
}

/// EVM native protocol plane messages (discovery + eth/snap sync skeleton).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmNativeBlockHeaderWireV1 {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmNativeBlockBodyWireV1 {
    pub number: u64,
    pub block_hash: [u8; 32],
    pub tx_hashes: Vec<[u8; 32]>,
    pub ommer_hashes: Vec<[u8; 32]>,
    pub withdrawal_count: Option<usize>,
    pub body_available: bool,
    pub txs_materialized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvmNativeMessage {
    DiscoveryPing {
        from: NodeId,
        chain_id: u64,
        tcp_port: u16,
        udp_port: u16,
    },
    DiscoveryPong {
        from: NodeId,
        chain_id: u64,
    },
    DiscoveryFindNode {
        from: NodeId,
        target: NodeId,
    },
    DiscoveryNeighbors {
        from: NodeId,
        peers: Vec<NodeId>,
    },
    RlpxAuth {
        from: NodeId,
        chain_id: u64,
        network_id: u64,
        auth_tag: [u8; 32],
    },
    RlpxAuthAck {
        from: NodeId,
        chain_id: u64,
        network_id: u64,
        ack_tag: [u8; 32],
    },
    Hello {
        from: NodeId,
        chain_id: u64,
        eth_versions: Vec<u8>,
        snap_versions: Vec<u8>,
        network_id: u64,
        total_difficulty: u128,
        head_hash: [u8; 32],
        genesis_hash: [u8; 32],
    },
    Status {
        from: NodeId,
        chain_id: u64,
        total_difficulty: u128,
        head_height: u64,
        head_hash: [u8; 32],
        genesis_hash: [u8; 32],
    },
    NewBlockHashes {
        from: NodeId,
        blocks: Vec<([u8; 32], u64)>,
    },
    Transactions {
        from: NodeId,
        chain_id: u64,
        tx_hash: [u8; 32],
        tx_count: u64,
        payload: Vec<u8>,
    },
    GetBlockHeaders {
        from: NodeId,
        start_height: u64,
        max: u64,
        skip: u64,
        reverse: bool,
    },
    BlockHeaders {
        from: NodeId,
        headers: Vec<EvmNativeBlockHeaderWireV1>,
    },
    GetBlockBodies {
        from: NodeId,
        hashes: Vec<[u8; 32]>,
    },
    BlockBodies {
        from: NodeId,
        bodies: Vec<EvmNativeBlockBodyWireV1>,
    },
    SnapGetAccountRange {
        from: NodeId,
        block_hash: [u8; 32],
        origin: [u8; 32],
        limit: u64,
    },
    SnapAccountRange {
        from: NodeId,
        account_count: u64,
        proof_node_count: u64,
    },
}

/// Unified protocol message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtocolMessage {
    TwoPc(TwoPcMessage),
    Finality(FinalityMessage),
    Gossip(GossipMessage),
    Pacemaker(PacemakerMessage),
    EvmNative(EvmNativeMessage),
    /// 分布式 OCCC 兼容 gossip（结构化消息，用于迁移期与回归对齐）。
    DistributedOcccGossip(protocol_catalog::distributed_occc::gossip::GossipMessage),
}
