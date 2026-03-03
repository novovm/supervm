#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

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

/// In-memory gossip node backed by Tokio mpsc channels.
///
/// This mirrors the legacy Phase 4.1 API so higher layers can be de-coupled from the V2 stack.
#[derive(Debug)]
pub struct GossipNode {
    node_id: u32,
    peers: Vec<u32>,
    inbox_rx: Arc<Mutex<mpsc::Receiver<GossipMessage>>>,
    inbox_tx: mpsc::Sender<GossipMessage>,
    outbox_rx: Arc<Mutex<mpsc::Receiver<GossipMessage>>>,
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
            inbox_rx: Arc::new(Mutex::new(inbox_rx)),
            inbox_tx,
            outbox_rx: Arc::new(Mutex::new(outbox_rx)),
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
}
