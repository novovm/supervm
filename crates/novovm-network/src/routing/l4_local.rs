use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::RwLock;

use super::types::{
    unix_ms_now, L4ParticipationLevel, L4PeerRef, NodeId, Reachability, RoutingSource,
};

#[derive(Debug, Clone)]
pub struct L4LocalRoutingSnapshot {
    pub owner_node_id: String,
    pub version: u64,
    pub peers: Vec<L4PeerRef>,
    pub max_peers: usize,
    pub last_sync_unix_ms: u64,
}

pub struct L4LocalRoutingTable {
    owner_node_id: String,
    version: RwLock<u64>,
    peers: RwLock<HashMap<NodeId, L4PeerRef>>,
    max_peers: usize,
    last_sync_unix_ms: RwLock<u64>,
}

impl L4LocalRoutingTable {
    pub fn new(owner_node_id: impl Into<String>, max_peers: usize) -> Self {
        Self {
            owner_node_id: owner_node_id.into(),
            version: RwLock::new(1),
            peers: RwLock::new(HashMap::new()),
            max_peers,
            last_sync_unix_ms: RwLock::new(unix_ms_now()),
        }
    }

    pub fn owner_node_id(&self) -> &str {
        &self.owner_node_id
    }

    pub fn max_peers(&self) -> usize {
        self.max_peers
    }

    pub fn len(&self) -> usize {
        self.peers.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.peers.read().unwrap().is_empty()
    }

    pub fn contains_peer(&self, node_id: &str) -> bool {
        self.peers.read().unwrap().contains_key(node_id)
    }

    pub fn upsert_peer(&self, peer: L4PeerRef) {
        let mut peers = self.peers.write().unwrap();

        if peers.len() >= self.max_peers && !peers.contains_key(&peer.node_id) {
            if let Some(evict_id) = Self::select_eviction_candidate(&peers) {
                peers.remove(&evict_id);
            }
        }

        peers.insert(peer.node_id.clone(), peer);
        drop(peers);
        self.touch_meta();
    }

    pub fn get_peer(&self, node_id: &str) -> Option<L4PeerRef> {
        self.peers.read().unwrap().get(node_id).cloned()
    }

    pub fn remove_peer(&self, node_id: &str) -> Option<L4PeerRef> {
        let removed = self.peers.write().unwrap().remove(node_id);
        if removed.is_some() {
            self.touch_meta();
        }
        removed
    }

    pub fn all_peers(&self) -> Vec<L4PeerRef> {
        self.peers.read().unwrap().values().cloned().collect()
    }

    pub fn update_reachability(&self, node_id: &str, reachability: Reachability) -> bool {
        let mut peers = self.peers.write().unwrap();
        if let Some(peer) = peers.get_mut(node_id) {
            peer.reachability = reachability;
            peer.last_seen_unix_ms = unix_ms_now();
            drop(peers);
            self.touch_meta();
            true
        } else {
            false
        }
    }

    pub fn update_latency(&self, node_id: &str, latency_ms: u32) -> bool {
        let mut peers = self.peers.write().unwrap();
        if let Some(peer) = peers.get_mut(node_id) {
            peer.latency_ms = Some(latency_ms);
            peer.last_seen_unix_ms = unix_ms_now();
            drop(peers);
            self.touch_meta();
            true
        } else {
            false
        }
    }

    pub fn update_role(&self, node_id: &str, role: L4ParticipationLevel) -> bool {
        let mut peers = self.peers.write().unwrap();
        if let Some(peer) = peers.get_mut(node_id) {
            peer.role = role;
            peer.last_seen_unix_ms = unix_ms_now();
            drop(peers);
            self.touch_meta();
            true
        } else {
            false
        }
    }

    pub fn bump_score(&self, node_id: &str, delta: i32) -> bool {
        let mut peers = self.peers.write().unwrap();
        if let Some(peer) = peers.get_mut(node_id) {
            peer.score = peer.score.saturating_add(delta);
            peer.last_seen_unix_ms = unix_ms_now();
            drop(peers);
            self.touch_meta();
            true
        } else {
            false
        }
    }

    pub fn best_direct_candidates(&self, limit: usize) -> Vec<L4PeerRef> {
        let mut peers: Vec<_> = self
            .peers
            .read()
            .unwrap()
            .values()
            .filter(|p| {
                matches!(
                    p.reachability,
                    Reachability::Reachable | Reachability::LanOnly
                )
            })
            .cloned()
            .collect();

        peers.sort_by_key(|p| {
            (
                Reverse(p.score),
                p.latency_ms.unwrap_or(u32::MAX),
                Reverse(p.last_seen_unix_ms),
            )
        });

        peers.truncate(limit);
        peers
    }

    pub fn best_relay_candidates(&self, limit: usize) -> Vec<L4PeerRef> {
        let mut peers: Vec<_> = self
            .peers
            .read()
            .unwrap()
            .values()
            .filter(|p| {
                matches!(
                    p.reachability,
                    Reachability::Reachable | Reachability::RelayOnly
                )
            })
            .filter(|p| {
                matches!(
                    p.role,
                    L4ParticipationLevel::RelayNode | L4ParticipationLevel::NatAssistNode
                )
            })
            .cloned()
            .collect();

        peers.sort_by_key(|p| {
            (
                Reverse(p.score),
                p.latency_ms.unwrap_or(u32::MAX),
                Reverse(p.last_seen_unix_ms),
            )
        });

        peers.truncate(limit);
        peers
    }

    pub fn best_lan_candidates(&self, limit: usize) -> Vec<L4PeerRef> {
        let mut peers: Vec<_> = self
            .peers
            .read()
            .unwrap()
            .values()
            .filter(|p| matches!(p.reachability, Reachability::LanOnly))
            .cloned()
            .collect();

        peers.sort_by_key(|p| {
            (
                Reverse(p.score),
                p.latency_ms.unwrap_or(u32::MAX),
                Reverse(p.last_seen_unix_ms),
            )
        });

        peers.truncate(limit);
        peers
    }

    pub fn expire_stale_local_observed(&self, now_unix_ms: u64, freshness_ms: u64) {
        let mut peers = self.peers.write().unwrap();
        let mut changed = false;

        for peer in peers.values_mut() {
            if peer.source != RoutingSource::LocalObserved
                || peer.reachability != Reachability::Reachable
            {
                continue;
            }

            if now_unix_ms.saturating_sub(peer.last_seen_unix_ms) > freshness_ms {
                peer.reachability = Reachability::Unknown;
                changed = true;
            }
        }

        drop(peers);
        if changed {
            self.touch_meta();
        }
    }

    pub fn snapshot(&self) -> L4LocalRoutingSnapshot {
        L4LocalRoutingSnapshot {
            owner_node_id: self.owner_node_id.clone(),
            version: *self.version.read().unwrap(),
            peers: self.all_peers(),
            max_peers: self.max_peers,
            last_sync_unix_ms: *self.last_sync_unix_ms.read().unwrap(),
        }
    }

    fn touch_meta(&self) {
        *self.version.write().unwrap() += 1;
        *self.last_sync_unix_ms.write().unwrap() = unix_ms_now();
    }

    fn select_eviction_candidate(peers: &HashMap<NodeId, L4PeerRef>) -> Option<NodeId> {
        peers
            .iter()
            .min_by_key(|(_, peer)| {
                (
                    peer.score,
                    peer.latency_ms.unwrap_or(u32::MAX),
                    peer.last_seen_unix_ms,
                )
            })
            .map(|(node_id, _)| node_id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::types::{L4PeerRef, Reachability, RoutingSource};

    #[test]
    fn upsert_and_select_direct_candidates() {
        let table = L4LocalRoutingTable::new("self-node", 8);

        table.upsert_peer(
            L4PeerRef::new("peer-a".to_string())
                .with_reachability(Reachability::Reachable)
                .with_latency_ms(20)
                .with_score(10),
        );

        table.upsert_peer(
            L4PeerRef::new("peer-b".to_string())
                .with_reachability(Reachability::Reachable)
                .with_latency_ms(10)
                .with_score(5),
        );

        let best = table.best_direct_candidates(1);
        assert_eq!(best.len(), 1);
        assert_eq!(best[0].node_id, "peer-a");
    }

    #[test]
    fn relay_candidates_require_role_and_reachability() {
        let table = L4LocalRoutingTable::new("self-node", 8);

        table.upsert_peer(
            L4PeerRef::new("peer-a".to_string())
                .with_role(L4ParticipationLevel::RelayNode)
                .with_reachability(Reachability::RelayOnly)
                .with_score(3),
        );

        table.upsert_peer(
            L4PeerRef::new("peer-b".to_string())
                .with_role(L4ParticipationLevel::ConsumerOnly)
                .with_reachability(Reachability::RelayOnly)
                .with_score(100),
        );

        let relays = table.best_relay_candidates(8);
        assert_eq!(relays.len(), 1);
        assert_eq!(relays[0].node_id, "peer-a");
    }

    #[test]
    fn evicts_worst_peer_when_capacity_reached() {
        let table = L4LocalRoutingTable::new("self-node", 2);

        table.upsert_peer(L4PeerRef::new("peer-a".to_string()).with_score(1));
        table.upsert_peer(L4PeerRef::new("peer-b".to_string()).with_score(2));
        table.upsert_peer(L4PeerRef::new("peer-c".to_string()).with_score(10));

        assert_eq!(table.len(), 2);
        assert!(table.contains_peer("peer-c"));
    }

    #[test]
    fn stale_local_observed_expires_to_unknown_and_exits_direct_candidates() {
        let table = L4LocalRoutingTable::new("self-node", 8);
        table.upsert_peer(
            L4PeerRef::new("peer-observed".to_string())
                .with_reachability(Reachability::Reachable)
                .with_source(RoutingSource::LocalObserved)
                .with_last_seen_unix_ms(1_000),
        );

        table.expire_stale_local_observed(62_000, 60_000);

        let peer = table.get_peer("peer-observed").expect("peer should remain");
        assert_eq!(peer.source, RoutingSource::LocalObserved);
        assert_eq!(peer.reachability, Reachability::Unknown);
        assert!(table.best_direct_candidates(1).is_empty());
    }
}
