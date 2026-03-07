# WEB3014: 去中心化消息协议标准（Messaging）

版本: v0.1.0  
状态: Draft  
作者: SuperVM Core Team

---

## 目标与范围

- 去中心化实时通信：基于 L4 网络（libp2p/gossipsub）的无中心消息发布/订阅。
- 端到端加密（E2E）：私聊与群聊消息默认加密，元数据最小泄露。
- 房间/频道语义：Topic = Room，支持权限与成员管理扩展。
- 离线与穿透：支持 DCUtR、Relay 中继与 Store-and-Forward（延迟可用性）。
- 可选锚定：历史消息/摘要可通过 WEB3006（存储）与 WEB3007（跨链消息）进行 L1/跨链锚定与通知。

---

## 设计分层

- L4 网络（传输）：libp2p（gossipsub、mDNS、DCUtR、Relay、QUIC/TCP）。
- L3/L2（可选服务）：离线转发、中继信誉、抗垃圾扩展。
- L1（锚定/可证据性）：消息摘要/批次 CID 上链，提供防篡改与可审计能力。
- 应用层（本标准）：房间、身份、E2E 加密、历史、订阅与事件模型。

参考实现（PoC）：`src/node-core/examples/chat_poc.rs`（gossipsub + mDNS + DCUtR + Relay）。

---

## Rust Trait 接口

```rust
use async_trait::async_trait;
use serde::{Serialize, Deserialize};

pub type RoomId = String;          // gossipsub topic
pub type PeerIdBytes = Vec<u8>;    // libp2p PeerId 原始字节
pub type MessageId = [u8; 32];
pub type SubscriptionId = [u8; 16];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentType {
    Text, Bytes, Json, Image, File, Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2EHeader {
    pub scheme: String,       // "noise_xx" | "x3dh+dr" | ...
    pub key_id: Vec<u8>,      // 发送方公钥指纹或会话ID
    pub nonce: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub room: RoomId,                 // 目标房间（或点对点使用对端派生room）
    pub from: PeerIdBytes,            // 发送者（可与链上账户映射）
    pub content_type: ContentType,
    pub ciphertext: Vec<u8>,          // E2E 密文
    pub header: E2EHeader,            // 加密头（算法/密钥ID/nonce）
    pub timestamp: u64,               // Unix 秒
    pub signature: Vec<u8>,           // 发送方对明文/密文的签名（最小可关联）
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange { pub start: u64, pub end: u64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessageRef {
    pub message_id: MessageId,
    pub cid: Vec<u8>,             // IPFS/内容地址（可选）
    pub timestamp: u64,
}

#[derive(thiserror::Error, Debug)]
pub enum MessagingError {
    #[error("network")] Network,
    #[error("encrypt")] Encrypt,
    #[error("decrypt")] Decrypt,
    #[error("room")] Room,
    #[error("not_found")] NotFound,
    #[error("other: {0}")] Other(String),
}

#[async_trait]
pub trait WEB3014Messaging {
    // ============ 会话与身份 ============

    /// 初始化或导入本地身份（与链上账户可建立映射）
    async fn init_identity(&self) -> Result<PeerIdBytes, MessagingError>;

    // ============ 房间/订阅 ============

    /// 加入房间（gossipsub 订阅）。返回订阅ID，供取消订阅使用。
    async fn join_room(&self, room: RoomId) -> Result<SubscriptionId, MessagingError>;

    /// 退出房间（取消订阅）。
    async fn leave_room(&self, room: RoomId) -> Result<(), MessagingError>;

    /// 订阅消息回调（流式）。实现可用 channel/回调注册等方式。
    async fn subscribe_messages(
        &self,
        room: RoomId,
    ) -> Result<SubscriptionId, MessagingError>;

    // ============ 消息发送 ============

    /// 发送房间消息（端到端加密）。
    async fn send_room_message(&self, msg: Message) -> Result<MessageId, MessagingError>;

    /// 私聊（点对点）。实现可用 per-peer topic 或 direct request/response。
    async fn send_direct_message(
        &self,
        peer: PeerIdBytes,
        msg: Message,
    ) -> Result<MessageId, MessagingError>;

    // ============ 历史与锚定 ============

    /// 拉取历史（本地缓存/外部存储/IPFS），时间范围过滤。
    async fn fetch_history(
        &self,
        room: RoomId,
        time_range: TimeRange,
        limit: u32,
    ) -> Result<Vec<StoredMessageRef>, MessagingError>;

    /// 批量锚定历史（将批次 CID 通过 WEB3006 存储，并可用 WEB3007 进行跨链通知）。
    async fn anchor_history_cids(
        &self,
        room: RoomId,
        cids: Vec<Vec<u8>>,
    ) -> Result<Vec<[u8; 32]>, MessagingError>; // 返回链上交易哈希或锚定ID
}
```

---

## 互操作与集成

- 传输与发现：libp2p（gossipsub、mDNS、DCUtR、Relay、QUIC/TCP）
- 存储锚定：WEB3006（例如 IPFS 内容地址 + 批次 Merkle 根）
- 跨链通知：WEB3007（新消息/批次上链时的跨链事件）
- 身份映射：链上账户 ↔ libp2p PeerId ↔ 应用身份（最小化可链接性）

---

## 参考 Solidity 接口（可选锚定）

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface IWeb3014Anchor {
    event RoomAnchored(bytes32 indexed roomHash, bytes cid, uint256 timestamp);

    function anchorCid(bytes32 roomHash, bytes calldata cid) external returns (bytes32 anchorId);
}
```

- 作用：在 L1 上记录房间的内容地址快照（例如每日/每小时批次），提供不可篡改证明。
- 建议：不直接上链明文消息；仅上链 CID/Merkle 根等摘要。

---

## 示例（基于 PoC 的概念用法）

```rust
// 1) 初始化
let peer_id = messaging.init_identity().await?;

// 2) 订阅房间
let sub = messaging.join_room("supervm-chat".to_string()).await?;

// 3) 发送加密消息
let msg = Message {
    room: "supervm-chat".into(),
    from: peer_id.clone(),
    content_type: ContentType::Text,
    ciphertext: encrypt_text("hello", &peer_id)?,
    header: E2EHeader { scheme: "noise_xx".into(), key_id: vec![], nonce: vec![0; 12] },
    timestamp: current_unix_ts(),
    signature: vec![],
};
let mid = messaging.send_room_message(msg).await?;

// 4) 锚定历史（可选）
let _anchors = messaging.anchor_history_cids(
    "supervm-chat".into(),
    vec![cid_bytes],
).await?;
```

---

## 安全与隐私

- 默认 E2E：Noise/X3DH+Double Ratchet 任一方案；密钥轮换与前向保密。
- 元数据最小化：不在 L1 写入可关联的身份/内容；仅上链摘要。
- 抗滥用扩展：速率限制、工作量证明、信誉滑动窗口（L3/L2 可选）。

---

## Roadmap

| 阶段 | 功能 | 状态 |
|------|------|------|
| Phase 1 | 房间/订阅 + E2E 加密 + PoC 对齐 | 设计中 |
| Phase 2 | 离线转发/Relay/Store-and-Forward | 规划中 |
| Phase 3 | 成员/权限与群管理 | 规划中 |
| Phase 4 | L1 锚定与跨链通知（WEB3006/3007） | 规划中 |
| Phase 5 | SDK（TS/Rust）、钱包集成 | 规划中 |

---

版本历史:
- v0.1.0 (2025-11-17): 初始草案
