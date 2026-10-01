#![forbid(unsafe_code)]

/// 协议集合（生产命名）：
///
/// 这里存放不同阶段/子系统的 wire types（消息、枚举、标识等）。
/// 目标是把“协议类型/编码”从实现里解耦出来，供 network/consensus/adapter 复用。
///
/// 说明：当前先承接“分布式 OCCC”域的 gossip 类型作为兼容层。
///
/// 命名原则：避免使用工程进度名（例如 phase4_1），使用语义名。
pub mod distributed_occc {
    pub mod gossip {
        use serde::{Deserialize, Serialize};

        #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
        pub struct GossipMessage {
            pub from: u32,
            pub to: u32,
            pub msg_type: MessageType,
            pub payload: Vec<u8>,
            pub timestamp: u64,
            pub seq: u64,
        }

        #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
        pub enum MessageType {
            ShardState,
            TxProposal,
            PrepareVote,
            CommitAck,
            Heartbeat,
            StateSync,
        }
    }
}
