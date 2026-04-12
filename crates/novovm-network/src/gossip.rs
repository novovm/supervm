#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock, Mutex as StdMutex};
use tokio::sync::{mpsc, Mutex as TokioMutex};

static RUNTIME_RELAY_MEMBERSHIP_INBOX: LazyLock<StdMutex<Vec<RelayMembership>>> =
    LazyLock::new(|| StdMutex::new(Vec::new()));
const RUNTIME_RELAY_MEMBERSHIP_INBOX_MAX_DEFAULT: usize = 4096;
const RUNTIME_RELAY_MEMBERSHIP_INBOX_MAX_MIN: usize = 1;
const RUNTIME_RELAY_MEMBERSHIP_INBOX_MAX_MAX: usize = 65_536;

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
    RelayMembership,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelayMembership {
    pub relay_id: String,
    pub region: Option<String>,
    pub addr: Option<String>,
    pub health: Option<String>,
    pub capacity_class: Option<String>,
    pub score_hint: Option<i32>,
    #[serde(default)]
    pub seen_unix_ms: Option<u64>,
}

/// In-memory gossip node backed by Tokio mpsc channels.
///
/// This mirrors the legacy Phase 4.1 API so higher layers can be de-coupled from the V2 stack.
#[derive(Debug)]
pub struct GossipNode {
    node_id: u32,
    peers: Vec<u32>,
    inbox_rx: Arc<TokioMutex<mpsc::Receiver<GossipMessage>>>,
    inbox_tx: mpsc::Sender<GossipMessage>,
    outbox_rx: Arc<TokioMutex<mpsc::Receiver<GossipMessage>>>,
    outbox_tx: mpsc::Sender<GossipMessage>,
    seq_counter: Arc<std::sync::atomic::AtomicU64>,
}

impl GossipNode {
    #[must_use]
    pub fn new(node_id: u32, peers: Vec<u32>) -> Self {
        let (inbox_tx, inbox_rx) = mpsc::channel(10_000);
        let (outbox_tx, outbox_rx) = mpsc::channel(10_000);

        Self {
            node_id,
            peers,
            inbox_rx: Arc::new(TokioMutex::new(inbox_rx)),
            inbox_tx,
            outbox_rx: Arc::new(TokioMutex::new(outbox_rx)),
            outbox_tx,
            seq_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Broadcast message to all peers (best-effort).
    pub async fn broadcast(&self, msg_type: MessageType, payload: Vec<u8>) -> Result<(), String> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "Time error".to_string())?
            .as_micros() as u64;

        let seq = self
            .seq_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        for peer_id in &self.peers {
            let msg = GossipMessage {
                from: self.node_id,
                to: *peer_id,
                msg_type: msg_type.clone(),
                payload: payload.clone(),
                timestamp,
                seq,
            };

            if self.outbox_tx.try_send(msg).is_err() {
                // Best-effort drop is acceptable for gossip.
            }
        }

        Ok(())
    }

    pub async fn broadcast_relay_membership(
        &self,
        entries: &[RelayMembership],
    ) -> Result<(), String> {
        let payload = serde_json::to_vec(entries)
            .map_err(|e| format!("Serialize relay membership failed: {e}"))?;
        self.broadcast(MessageType::RelayMembership, payload).await
    }

    /// Receive a message from inbox.
    pub async fn receive(&self) -> Option<GossipMessage> {
        let mut inbox_rx = self.inbox_rx.lock().await;
        inbox_rx.recv().await
    }

    /// Push a message to inbox (used by peer nodes / harness).
    pub async fn receive_message(&self, msg: GossipMessage) -> Result<(), String> {
        self.inbox_tx
            .try_send(msg)
            .map_err(|e| format!("Inbox full: {e}"))
    }

    /// Drain pending outbound messages (non-blocking).
    pub async fn get_pending_messages(&self, count: usize) -> Vec<GossipMessage> {
        let mut outbox_rx = self.outbox_rx.lock().await;
        let mut messages = Vec::with_capacity(count);
        for _ in 0..count {
            match outbox_rx.try_recv() {
                Ok(msg) => messages.push(msg),
                Err(_) => break,
            }
        }
        messages
    }

    pub fn node_id(&self) -> u32 {
        self.node_id
    }

    pub fn peers(&self) -> &[u32] {
        &self.peers
    }
}

pub fn decode_relay_membership_payload(payload: &[u8]) -> Result<Vec<RelayMembership>, String> {
    serde_json::from_slice(payload).map_err(|e| format!("Decode relay membership failed: {e}"))
}

pub fn decode_relay_membership_message(
    msg: &GossipMessage,
) -> Result<Vec<RelayMembership>, String> {
    if !matches!(msg.msg_type, MessageType::RelayMembership) {
        return Err("Message type is not relay membership".to_string());
    }
    decode_relay_membership_payload(&msg.payload)
}

pub fn ingest_runtime_relay_membership(entries: Vec<RelayMembership>) -> usize {
    fn membership_health_rank(raw: Option<&str>) -> i32 {
        match raw.map(str::trim).map(|v| v.to_ascii_lowercase()) {
            Some(v) if v == "healthy" => 2,
            Some(v) if v == "degraded" => 1,
            _ => 0,
        }
    }
    fn membership_capacity_rank(raw: Option<&str>) -> i32 {
        match raw.map(str::trim).map(|v| v.to_ascii_lowercase()) {
            Some(v) if v == "large" => 2,
            Some(v) if v == "medium" => 1,
            _ => 0,
        }
    }
    fn runtime_relay_membership_inbox_max_entries() -> usize {
        let raw = std::env::var("NOVOVM_L3_RUNTIME_MEMBERSHIP_INBOX_MAX")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(RUNTIME_RELAY_MEMBERSHIP_INBOX_MAX_DEFAULT);
        raw.clamp(
            RUNTIME_RELAY_MEMBERSHIP_INBOX_MAX_MIN,
            RUNTIME_RELAY_MEMBERSHIP_INBOX_MAX_MAX,
        )
    }
    let mut inbox = RUNTIME_RELAY_MEMBERSHIP_INBOX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let before = inbox.len();
    let max_entries = runtime_relay_membership_inbox_max_entries();
    for entry in entries {
        let relay_id = entry.relay_id.trim();
        if relay_id.is_empty() {
            continue;
        }
        if let Some(pos) = inbox.iter().position(|v| v.relay_id == relay_id) {
            let incoming_seen = entry.seen_unix_ms.unwrap_or(0);
            let existing_seen = inbox[pos].seen_unix_ms.unwrap_or(0);
            let incoming_score = entry.score_hint.unwrap_or(0);
            let existing_score = inbox[pos].score_hint.unwrap_or(0);
            let incoming_health = membership_health_rank(entry.health.as_deref());
            let existing_health = membership_health_rank(inbox[pos].health.as_deref());
            let incoming_capacity = membership_capacity_rank(entry.capacity_class.as_deref());
            let existing_capacity = membership_capacity_rank(inbox[pos].capacity_class.as_deref());
            let incoming_addr = entry.addr.as_deref().map(str::trim).unwrap_or("");
            let existing_addr = inbox[pos].addr.as_deref().map(str::trim).unwrap_or("");
            let incoming_region = entry.region.as_deref().map(str::trim).unwrap_or("");
            let existing_region = inbox[pos].region.as_deref().map(str::trim).unwrap_or("");
            if incoming_seen > existing_seen
                || (incoming_seen == existing_seen && incoming_score > existing_score)
                || (incoming_seen == existing_seen
                    && incoming_score == existing_score
                    && incoming_health > existing_health)
                || (incoming_seen == existing_seen
                    && incoming_score == existing_score
                    && incoming_health == existing_health
                    && incoming_capacity > existing_capacity)
                || (incoming_seen == existing_seen
                    && incoming_score == existing_score
                    && incoming_health == existing_health
                    && incoming_capacity == existing_capacity
                    && incoming_addr > existing_addr)
                || (incoming_seen == existing_seen
                    && incoming_score == existing_score
                    && incoming_health == existing_health
                    && incoming_capacity == existing_capacity
                    && incoming_addr == existing_addr
                    && incoming_region > existing_region)
            {
                inbox[pos] = entry;
            }
            continue;
        }
        inbox.push(entry);
        if inbox.len() > max_entries {
            let overflow = inbox.len().saturating_sub(max_entries);
            inbox.drain(0..overflow);
        }
    }
    inbox.len().saturating_sub(before)
}

pub fn ingest_runtime_relay_membership_message(msg: &GossipMessage) -> Result<usize, String> {
    let entries = decode_relay_membership_message(msg)?;
    Ok(ingest_runtime_relay_membership(entries))
}

pub fn drain_runtime_relay_membership() -> Vec<RelayMembership> {
    let mut inbox = RUNTIME_RELAY_MEMBERSHIP_INBOX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *inbox)
}

pub fn runtime_relay_membership_pending() -> usize {
    RUNTIME_RELAY_MEMBERSHIP_INBOX
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_broadcast_message() {
        let node = GossipNode::new(0, vec![1, 2, 3]);

        let result = node
            .broadcast(MessageType::TxProposal, vec![1, 2, 3, 4, 5])
            .await;

        assert!(result.is_ok());

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let messages = node.get_pending_messages(10).await;
        assert_eq!(messages.len(), 3);
        for (idx, msg) in messages.iter().enumerate() {
            assert_eq!(msg.from, 0);
            assert_eq!(msg.to, (idx + 1) as u32);
            assert_eq!(msg.msg_type, MessageType::TxProposal);
            assert_eq!(msg.payload, vec![1, 2, 3, 4, 5]);
        }
    }

    #[tokio::test]
    async fn test_receive_message() {
        let node = GossipNode::new(0, vec![1, 2, 3]);

        let msg = GossipMessage {
            from: 1,
            to: 0,
            msg_type: MessageType::PrepareVote,
            payload: vec![1],
            timestamp: 0,
            seq: 1,
        };

        let result = node.receive_message(msg.clone()).await;
        assert!(result.is_ok());

        let received = node.receive().await;
        assert_eq!(received, Some(msg));
    }

    #[tokio::test]
    async fn test_broadcast_and_decode_relay_membership() {
        let node = GossipNode::new(9, vec![11, 12]);
        let members = vec![
            RelayMembership {
                relay_id: "relay-a".to_string(),
                region: Some("ap-east".to_string()),
                addr: Some("10.0.0.1:9001".to_string()),
                health: Some("healthy".to_string()),
                capacity_class: Some("medium".to_string()),
                score_hint: Some(3),
                seen_unix_ms: Some(1_700_000_001_000),
            },
            RelayMembership {
                relay_id: "relay-b".to_string(),
                region: Some("ap-east".to_string()),
                addr: Some("10.0.0.2:9001".to_string()),
                health: Some("degraded".to_string()),
                capacity_class: Some("small".to_string()),
                score_hint: Some(-1),
                seen_unix_ms: Some(1_700_000_002_000),
            },
        ];

        node.broadcast_relay_membership(&members)
            .await
            .expect("broadcast relay membership");
        let pending = node.get_pending_messages(10).await;
        assert_eq!(pending.len(), 2);
        for msg in pending {
            let decoded =
                decode_relay_membership_message(&msg).expect("decode relay membership message");
            assert_eq!(decoded, members);
        }
    }

    #[test]
    fn test_runtime_relay_membership_inbox_roundtrip() {
        let initial_pending = runtime_relay_membership_pending();
        if initial_pending > 0 {
            let _ = drain_runtime_relay_membership();
        }

        let inserted = ingest_runtime_relay_membership(vec![RelayMembership {
            relay_id: "relay-runtime".to_string(),
            region: Some("ap-east".to_string()),
            addr: Some("10.0.0.9:9001".to_string()),
            health: Some("healthy".to_string()),
            capacity_class: Some("medium".to_string()),
            score_hint: Some(5),
            seen_unix_ms: Some(1_700_000_111_000),
        }]);
        assert_eq!(inserted, 1);
        assert_eq!(runtime_relay_membership_pending(), 1);

        let drained = drain_runtime_relay_membership();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].relay_id, "relay-runtime");
        assert_eq!(runtime_relay_membership_pending(), 0);
    }

}
