use std::time::{SystemTime, UNIX_EPOCH};

pub type NodeId = String;
pub type RegionId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeTier {
    L1,
    L2,
    L3,
    L4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum L4ParticipationLevel {
    ConsumerOnly,
    RoutingNode,
    RelayNode,
    NatAssistNode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reachability {
    Reachable,
    RelayOnly,
    LanOnly,
    Unreachable,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelayHealth {
    Healthy,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelayCapacityClass {
    Small,
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoutingSource {
    LocalObserved,
    PeerHinted,
    RegionalAnnounced,
    OperatorForced,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRef {
    pub node_id: NodeId,
    pub tier: NodeTier,
    pub region: Option<RegionId>,
    pub addr_hint: Option<String>,
    pub source: RoutingSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointHint {
    pub observed_addr: String,
    pub observed_unix_ms: u64,
    pub source: RoutingSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayRef {
    pub node_id: NodeId,
    pub region: RegionId,
    pub addr: String,
    pub capacity_class: RelayCapacityClass,
    pub health: RelayHealth,
    pub score: i32,
    pub source: RoutingSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayScoreView {
    pub relay_id: String,
    pub health: RelayHealth,
    pub score: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct L4PeerRef {
    pub node_id: NodeId,
    pub addr_hint: Option<String>,
    pub region: Option<RegionId>,
    pub role: L4ParticipationLevel,
    pub latency_ms: Option<u32>,
    pub reachability: Reachability,
    pub last_seen_unix_ms: u64,
    pub score: i32,
    pub source: RoutingSource,
}

impl L4PeerRef {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            addr_hint: None,
            region: None,
            role: L4ParticipationLevel::ConsumerOnly,
            latency_ms: None,
            reachability: Reachability::Unknown,
            last_seen_unix_ms: unix_ms_now(),
            score: 0,
            source: RoutingSource::LocalObserved,
        }
    }

    pub fn with_role(mut self, role: L4ParticipationLevel) -> Self {
        self.role = role;
        self
    }

    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    pub fn with_addr_hint(mut self, addr_hint: impl Into<String>) -> Self {
        self.addr_hint = Some(addr_hint.into());
        self
    }

    pub fn with_latency_ms(mut self, latency_ms: u32) -> Self {
        self.latency_ms = Some(latency_ms);
        self
    }

    pub fn with_reachability(mut self, reachability: Reachability) -> Self {
        self.reachability = reachability;
        self
    }

    pub fn with_score(mut self, score: i32) -> Self {
        self.score = score;
        self
    }

    pub fn with_source(mut self, source: RoutingSource) -> Self {
        self.source = source;
        self
    }

    pub fn with_last_seen_unix_ms(mut self, last_seen_unix_ms: u64) -> Self {
        self.last_seen_unix_ms = last_seen_unix_ms;
        self
    }

    pub fn touch_now(&mut self) {
        self.last_seen_unix_ms = unix_ms_now();
    }
}

pub fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
