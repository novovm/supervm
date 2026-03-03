#![forbid(unsafe_code)]

use dashmap::DashMap;
use novovm_protocol::{decode as protocol_decode, encode as protocol_encode, NodeId, ProtocolMessage};
use std::collections::VecDeque;
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
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
    socket: Arc<UdpSocket>,
    peers: Arc<DashMap<NodeId, SocketAddr>>,
    max_packet_size: usize,
}

impl UdpTransport {
    pub fn bind(node: NodeId, listen_addr: &str) -> Result<Self, NetworkError> {
        Self::bind_with_packet_size(node, listen_addr, 64 * 1024)
    }

    pub fn bind_with_packet_size(
        node: NodeId,
        listen_addr: &str,
        max_packet_size: usize,
    ) -> Result<Self, NetworkError> {
        let socket = UdpSocket::bind(listen_addr).map_err(|e| NetworkError::Io(e.to_string()))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| NetworkError::Io(e.to_string()))?;
        Ok(Self {
            node,
            socket: Arc::new(socket),
            peers: Arc::new(DashMap::new()),
            max_packet_size: max_packet_size.max(1024),
        })
    }

    pub fn register_peer(&self, node: NodeId, addr: &str) -> Result<(), NetworkError> {
        let parsed: SocketAddr = addr
            .parse()
            .map_err(|e: std::net::AddrParseError| NetworkError::AddressParse(e.to_string()))?;
        self.peers.insert(node, parsed);
        Ok(())
    }

    pub fn local_addr(&self) -> Result<SocketAddr, NetworkError> {
        self.socket
            .local_addr()
            .map_err(|e| NetworkError::Io(e.to_string()))
    }
}

impl Transport for UdpTransport {
    fn send(&self, to: NodeId, msg: ProtocolMessage) -> Result<(), NetworkError> {
        let peer = self
            .peers
            .get(&to)
            .ok_or(NetworkError::PeerNotFound(to))?;
        let encoded = protocol_encode(&msg).map_err(|e| NetworkError::Encode(e.to_string()))?;
        let sent = self
            .socket
            .send_to(&encoded, *peer)
            .map_err(|e| NetworkError::Io(e.to_string()))?;
        if sent != encoded.len() {
            return Err(NetworkError::Io(format!(
                "partial udp send: sent={sent} expected={}",
                encoded.len()
            )));
        }
        Ok(())
    }

    fn try_recv(&self, me: NodeId) -> Result<Option<ProtocolMessage>, NetworkError> {
        if me != self.node {
            return Err(NetworkError::LocalNodeMismatch {
                expected: self.node,
                got: me,
            });
        }

        let mut buf = vec![0u8; self.max_packet_size];
        match self.socket.recv_from(&mut buf) {
            Ok((n, _src)) => protocol_decode(&buf[..n])
                .map(Some)
                .map_err(|e| NetworkError::Decode(e.to_string())),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut
                    || e.raw_os_error() == Some(10054) =>
            {
                Ok(None)
            }
            Err(e) => Err(NetworkError::Io(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_protocol::{
        protocol_catalog::distributed_occc::gossip::{
            GossipMessage as DistributedGossipMessage, MessageType as DistributedMessageType,
        },
        GossipMessage, ShardId,
    };
    use std::collections::HashSet;
    use std::thread;
    use std::time::Duration;

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

        let send_triplet = |from: NodeId,
                            to: NodeId,
                            transport: &UdpTransport,
                            peers: Vec<NodeId>| {
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
}
