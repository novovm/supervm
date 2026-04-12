use super::{
    l4_local::L4LocalRoutingSnapshot,
    types::{L4ParticipationLevel, NodeId, RegionId},
};

#[derive(Debug, Clone)]
pub struct RoutingSummary {
    pub node_id: NodeId,
    pub region: Option<RegionId>,
    pub participation_level: L4ParticipationLevel,
    pub local_peer_count: u16,
    pub relay_capable_count: u16,
    pub last_sync_unix_ms: u64,
}

#[derive(Debug, Clone)]
pub struct QueueSummary {
    pub pending_count: u32,
    pub oldest_age_sec: u32,
    pub replay_ready: bool,
}

#[derive(Debug, Clone)]
pub struct PathHint {
    pub target_region: RegionId,
    pub via_node_id: NodeId,
    pub success_score: u16,
    pub last_success_unix_ms: u64,
}

impl From<&L4LocalRoutingSnapshot> for RoutingSummary {
    fn from(snapshot: &L4LocalRoutingSnapshot) -> Self {
        let relay_capable_count = snapshot
            .peers
            .iter()
            .filter(|p| {
                matches!(
                    p.role,
                    L4ParticipationLevel::RelayNode | L4ParticipationLevel::NatAssistNode
                )
            })
            .count() as u16;

        Self {
            node_id: snapshot.owner_node_id.clone(),
            region: None,
            participation_level: L4ParticipationLevel::RoutingNode,
            local_peer_count: snapshot.peers.len() as u16,
            relay_capable_count,
            last_sync_unix_ms: snapshot.last_sync_unix_ms,
        }
    }
}
