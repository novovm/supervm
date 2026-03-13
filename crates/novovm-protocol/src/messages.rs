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

/// Unified protocol message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtocolMessage {
    TwoPc(TwoPcMessage),
    Finality(FinalityMessage),
    Gossip(GossipMessage),
    Pacemaker(PacemakerMessage),
    /// 分布式 OCCC 兼容 gossip（结构化消息，用于迁移期与回归对齐）。
    DistributedOcccGossip(protocol_catalog::distributed_occc::gossip::GossipMessage),
}
