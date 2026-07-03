use serde::{Deserialize, Serialize};

use crate::control_plane::PeerId;
use crate::overlay::{AntiCensorshipProfile, OverlayHop, OverlayTransportProfile, RouteSet};
use crate::overlay_runtime::{
    decide_overlay_runtime_fallback_chain_v0, OverlayRouteHealthSnapshot, OverlayRuntimeDecision,
    OverlayRuntimeDecisionReason, OverlayRuntimeReachabilityClass, OverlayRuntimeSelectedPath,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdaptiveOverlayBindPolicy {
    Fixed { bind_addr: String },
    Floating,
    PortPool { bind_addrs: Vec<String> },
}

impl AdaptiveOverlayBindPolicy {
    pub fn default_floating() -> Self {
        Self::Floating
    }

    pub fn effective_bind_candidates(&self) -> Vec<String> {
        match self {
            Self::Fixed { bind_addr } => vec![bind_addr.clone()],
            Self::Floating => vec!["0.0.0.0:0".to_string()],
            Self::PortPool { bind_addrs } => bind_addrs.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveOverlayRelayBudget {
    pub max_relay_sessions: u32,
    pub max_relay_bytes_per_minute: u64,
}

impl AdaptiveOverlayRelayBudget {
    pub fn disabled() -> Self {
        Self {
            max_relay_sessions: 0,
            max_relay_bytes_per_minute: 0,
        }
    }

    pub fn light_default() -> Self {
        Self {
            max_relay_sessions: 16,
            max_relay_bytes_per_minute: 64 * 1024 * 1024,
        }
    }

    pub fn permits_relay(&self) -> bool {
        self.max_relay_sessions > 0 && self.max_relay_bytes_per_minute > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveOverlayNodeCapabilities {
    pub can_send: bool,
    pub can_receive: bool,
    pub relay_enabled: bool,
    pub queue_enabled: bool,
    pub relay_budget: AdaptiveOverlayRelayBudget,
}

impl AdaptiveOverlayNodeCapabilities {
    pub fn regular_node() -> Self {
        Self {
            can_send: true,
            can_receive: true,
            relay_enabled: false,
            queue_enabled: true,
            relay_budget: AdaptiveOverlayRelayBudget::disabled(),
        }
    }

    pub fn relay_node() -> Self {
        Self {
            can_send: true,
            can_receive: true,
            relay_enabled: true,
            queue_enabled: true,
            relay_budget: AdaptiveOverlayRelayBudget::light_default(),
        }
    }

    pub fn can_relay(&self) -> bool {
        self.relay_enabled && self.relay_budget.permits_relay()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveOverlayEndpointRecord {
    pub peer_id: PeerId,
    pub bind_policy: AdaptiveOverlayBindPolicy,
    pub advertised_endpoint: Option<String>,
    pub capabilities: AdaptiveOverlayNodeCapabilities,
}

impl AdaptiveOverlayEndpointRecord {
    pub fn zero_config(peer_id: PeerId) -> Self {
        Self {
            peer_id,
            bind_policy: AdaptiveOverlayBindPolicy::default_floating(),
            advertised_endpoint: None,
            capabilities: AdaptiveOverlayNodeCapabilities::regular_node(),
        }
    }

    pub fn with_advertised_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.advertised_endpoint = Some(endpoint.into());
        self
    }

    pub fn with_relay_enabled(mut self) -> Self {
        self.capabilities = AdaptiveOverlayNodeCapabilities::relay_node();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveOverlayNodeConfig {
    pub local_peer_id: PeerId,
    pub bind_policy: AdaptiveOverlayBindPolicy,
    pub capabilities: AdaptiveOverlayNodeCapabilities,
    pub bootstrap_peers: Vec<AdaptiveOverlayEndpointRecord>,
}

impl AdaptiveOverlayNodeConfig {
    pub fn zero_config(local_peer_id: PeerId) -> Self {
        Self {
            local_peer_id,
            bind_policy: AdaptiveOverlayBindPolicy::default_floating(),
            capabilities: AdaptiveOverlayNodeCapabilities::regular_node(),
            bootstrap_peers: Vec::new(),
        }
    }

    pub fn with_bootstrap_peers(
        mut self,
        bootstrap_peers: Vec<AdaptiveOverlayEndpointRecord>,
    ) -> Self {
        self.bootstrap_peers = bootstrap_peers;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveOverlayRoutePlan {
    pub local_peer_id: PeerId,
    pub target_peer_id: PeerId,
    pub decision: OverlayRuntimeDecision,
    pub candidate_route_count: usize,
    pub direct_candidate_count: usize,
    pub relay_candidate_count: usize,
    pub multihop_candidate_count: usize,
    pub queue_allowed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdaptiveOverlayRouteFamily {
    Direct,
    Relay,
    Multihop,
}

impl AdaptiveOverlayRouteFamily {
    pub fn classify(route: &RouteSet, target_peer_id: &PeerId) -> Self {
        if route.hops.len() == 1 && route.hops[0].peer_id == *target_peer_id {
            Self::Direct
        } else if route.hops.len() == 1 {
            Self::Relay
        } else {
            Self::Multihop
        }
    }
}

pub fn adaptive_overlay_candidate_routes_v0(
    target_peer_id: &PeerId,
    peers: &[AdaptiveOverlayEndpointRecord],
) -> Vec<RouteSet> {
    let mut routes = Vec::new();
    let Some(target) = peers.iter().find(|peer| peer.peer_id == *target_peer_id) else {
        return routes;
    };

    if target.capabilities.can_receive && target.advertised_endpoint.is_some() {
        routes.push(RouteSet::direct(target_peer_id.clone()));
    }

    let relay_peers = peers
        .iter()
        .filter(|peer| peer.peer_id != *target_peer_id && peer.capabilities.can_relay())
        .collect::<Vec<_>>();

    if let Some(relay) = relay_peers.first() {
        routes.push(RouteSet {
            target_peer_id: target_peer_id.clone(),
            hops: vec![OverlayHop {
                peer_id: relay.peer_id.clone(),
                transport: OverlayTransportProfile::RelayNovoRudp,
                route_token: None,
            }],
            content_address_hint: Some("adaptive-overlay-relay-v0".to_string()),
        });
    }

    let multihop_relays = if relay_peers.len() >= 3 {
        relay_peers.iter().skip(1).take(2).collect::<Vec<_>>()
    } else {
        relay_peers.iter().take(2).collect::<Vec<_>>()
    };

    if multihop_relays.len() >= 2 {
        routes.push(RouteSet {
            target_peer_id: target_peer_id.clone(),
            hops: multihop_relays
                .into_iter()
                .enumerate()
                .map(|(index, relay)| OverlayHop {
                    peer_id: relay.peer_id.clone(),
                    transport: if index == 0 {
                        OverlayTransportProfile::Libp2pCircuitRelay
                    } else {
                        OverlayTransportProfile::RelayNovoRudp
                    },
                    route_token: None,
                })
                .collect(),
            content_address_hint: Some("adaptive-overlay-multihop-v0".to_string()),
        });
    }

    routes
}

pub fn decide_adaptive_overlay_route_v0(
    config: &AdaptiveOverlayNodeConfig,
    target_peer_id: &PeerId,
    profile: &AntiCensorshipProfile,
    health: &OverlayRouteHealthSnapshot,
) -> AdaptiveOverlayRoutePlan {
    decide_adaptive_overlay_route_with_family_cooldown_v0(
        config,
        target_peer_id,
        profile,
        health,
        &[],
    )
}

pub fn decide_adaptive_overlay_route_with_family_cooldown_v0(
    config: &AdaptiveOverlayNodeConfig,
    target_peer_id: &PeerId,
    profile: &AntiCensorshipProfile,
    health: &OverlayRouteHealthSnapshot,
    cooldown_families: &[AdaptiveOverlayRouteFamily],
) -> AdaptiveOverlayRoutePlan {
    let original_routes =
        adaptive_overlay_candidate_routes_v0(target_peer_id, &config.bootstrap_peers);
    let routes = original_routes
        .iter()
        .cloned()
        .into_iter()
        .filter(|route| {
            let family = AdaptiveOverlayRouteFamily::classify(route, target_peer_id);
            !cooldown_families.contains(&family)
        })
        .collect::<Vec<_>>();
    if routes.is_empty() && !original_routes.is_empty() && !cooldown_families.is_empty() {
        return AdaptiveOverlayRoutePlan {
            local_peer_id: config.local_peer_id.clone(),
            target_peer_id: target_peer_id.clone(),
            decision: OverlayRuntimeDecision {
                target_peer_id: target_peer_id.clone(),
                selected_path: OverlayRuntimeSelectedPath::QueueFallback,
                route_set: None,
                direct_endpoint_candidates: Vec::new(),
                relay_candidates: Vec::new(),
                multi_hop_candidates: Vec::new(),
                reachability_class: OverlayRuntimeReachabilityClass::Unknown,
                reason: OverlayRuntimeDecisionReason::RouteHealthExhausted,
            },
            candidate_route_count: 0,
            direct_candidate_count: 0,
            relay_candidate_count: 0,
            multihop_candidate_count: 0,
            queue_allowed: config.capabilities.queue_enabled,
        };
    }
    let direct_candidate_count = routes
        .iter()
        .filter(|route| route.hops.len() == 1 && route.hops[0].peer_id == *target_peer_id)
        .count();
    let relay_candidate_count = routes
        .iter()
        .filter(|route| route.hops.len() == 1 && route.hops[0].peer_id != *target_peer_id)
        .count();
    let multihop_candidate_count = routes.iter().filter(|route| route.hops.len() > 1).count();
    let decision =
        decide_overlay_runtime_fallback_chain_v0(target_peer_id, &routes, profile, health);
    AdaptiveOverlayRoutePlan {
        local_peer_id: config.local_peer_id.clone(),
        target_peer_id: target_peer_id.clone(),
        decision,
        candidate_route_count: routes.len(),
        direct_candidate_count,
        relay_candidate_count,
        multihop_candidate_count,
        queue_allowed: config.capabilities.queue_enabled,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decide_adaptive_overlay_route_v0, AdaptiveOverlayBindPolicy, AdaptiveOverlayEndpointRecord,
        AdaptiveOverlayNodeConfig,
    };
    use crate::control_plane::PeerId;
    use crate::overlay::AntiCensorshipProfile;
    use crate::overlay_runtime::{
        OverlayHopHealth, OverlayRouteHealthSnapshot, OverlayRuntimeSelectedPath,
    };

    fn adaptive_config() -> AdaptiveOverlayNodeConfig {
        AdaptiveOverlayNodeConfig::zero_config(PeerId::new("node-a")).with_bootstrap_peers(vec![
            AdaptiveOverlayEndpointRecord::zero_config(PeerId::new("node-b"))
                .with_advertised_endpoint("192.168.71.56:41020"),
            AdaptiveOverlayEndpointRecord::zero_config(PeerId::new("relay-1"))
                .with_advertised_endpoint("192.168.71.9:41030")
                .with_relay_enabled(),
            AdaptiveOverlayEndpointRecord::zero_config(PeerId::new("relay-2"))
                .with_advertised_endpoint("192.168.71.54:41040")
                .with_relay_enabled(),
            AdaptiveOverlayEndpointRecord::zero_config(PeerId::new("relay-3"))
                .with_advertised_endpoint("192.168.71.55:41050")
                .with_relay_enabled(),
        ])
    }

    #[test]
    fn zero_config_uses_floating_bind() {
        let config = AdaptiveOverlayNodeConfig::zero_config(PeerId::new("node-a"));
        assert_eq!(
            config.bind_policy.effective_bind_candidates(),
            vec!["0.0.0.0:0".to_string()]
        );
        assert!(matches!(
            config.bind_policy,
            AdaptiveOverlayBindPolicy::Floating
        ));
    }

    #[test]
    fn adaptive_route_selects_direct_when_target_healthy() {
        let config = adaptive_config();
        let plan = decide_adaptive_overlay_route_v0(
            &config,
            &PeerId::new("node-b"),
            &AntiCensorshipProfile::default(),
            &OverlayRouteHealthSnapshot::new(100, Vec::new()),
        );
        assert_eq!(
            plan.decision.selected_path,
            OverlayRuntimeSelectedPath::DirectNovoRudp
        );
        assert_eq!(plan.candidate_route_count, 3);
    }

    #[test]
    fn adaptive_route_falls_back_to_relay_when_direct_cools_down() {
        let config = adaptive_config();
        let plan = decide_adaptive_overlay_route_v0(
            &config,
            &PeerId::new("node-b"),
            &AntiCensorshipProfile::default(),
            &OverlayRouteHealthSnapshot::new(
                100,
                vec![OverlayHopHealth::cooling_down(
                    PeerId::new("node-b"),
                    100,
                    1_000,
                )],
            ),
        );
        assert_eq!(
            plan.decision.selected_path,
            OverlayRuntimeSelectedPath::RelayNovoRudp
        );
    }

    #[test]
    fn adaptive_route_falls_back_to_multihop_when_direct_and_relay_cool_down() {
        let config = adaptive_config();
        let plan = decide_adaptive_overlay_route_v0(
            &config,
            &PeerId::new("node-b"),
            &AntiCensorshipProfile::default(),
            &OverlayRouteHealthSnapshot::new(
                100,
                vec![
                    OverlayHopHealth::cooling_down(PeerId::new("node-b"), 100, 1_000),
                    OverlayHopHealth::cooling_down(PeerId::new("relay-1"), 100, 1_000),
                ],
            ),
        );
        assert_eq!(
            plan.decision.selected_path,
            OverlayRuntimeSelectedPath::MultiHopRelay
        );
    }

    #[test]
    fn adaptive_route_queues_when_all_candidates_cool_down() {
        let config = adaptive_config();
        let plan = decide_adaptive_overlay_route_v0(
            &config,
            &PeerId::new("node-b"),
            &AntiCensorshipProfile::default(),
            &OverlayRouteHealthSnapshot::new(
                100,
                vec![
                    OverlayHopHealth::cooling_down(PeerId::new("node-b"), 100, 1_000),
                    OverlayHopHealth::cooling_down(PeerId::new("relay-1"), 100, 1_000),
                    OverlayHopHealth::cooling_down(PeerId::new("relay-2"), 100, 1_000),
                    OverlayHopHealth::cooling_down(PeerId::new("relay-3"), 100, 1_000),
                ],
            ),
        );
        assert_eq!(
            plan.decision.selected_path,
            OverlayRuntimeSelectedPath::QueueFallback
        );
    }

    #[test]
    fn adaptive_route_family_cooldown_can_skip_single_relay_but_keep_multihop() {
        let config = AdaptiveOverlayNodeConfig::zero_config(PeerId::new("node-a"))
            .with_bootstrap_peers(vec![
                AdaptiveOverlayEndpointRecord::zero_config(PeerId::new("node-b"))
                    .with_advertised_endpoint("192.168.71.56:41020"),
                AdaptiveOverlayEndpointRecord::zero_config(PeerId::new("relay-1"))
                    .with_advertised_endpoint("192.168.71.9:41030")
                    .with_relay_enabled(),
                AdaptiveOverlayEndpointRecord::zero_config(PeerId::new("relay-2"))
                    .with_advertised_endpoint("192.168.71.54:41040")
                    .with_relay_enabled(),
            ]);
        let plan = super::decide_adaptive_overlay_route_with_family_cooldown_v0(
            &config,
            &PeerId::new("node-b"),
            &AntiCensorshipProfile::default(),
            &OverlayRouteHealthSnapshot::new(100, Vec::new()),
            &[
                super::AdaptiveOverlayRouteFamily::Direct,
                super::AdaptiveOverlayRouteFamily::Relay,
            ],
        );
        assert_eq!(
            plan.decision.selected_path,
            OverlayRuntimeSelectedPath::MultiHopRelay
        );
        assert_eq!(plan.direct_candidate_count, 0);
        assert_eq!(plan.relay_candidate_count, 0);
        assert_eq!(plan.multihop_candidate_count, 1);
    }

    #[test]
    fn adaptive_route_family_cooldown_exhaustion_reports_health_exhausted() {
        let config = adaptive_config();
        let plan = super::decide_adaptive_overlay_route_with_family_cooldown_v0(
            &config,
            &PeerId::new("node-b"),
            &AntiCensorshipProfile::default(),
            &OverlayRouteHealthSnapshot::new(100, Vec::new()),
            &[
                super::AdaptiveOverlayRouteFamily::Direct,
                super::AdaptiveOverlayRouteFamily::Relay,
                super::AdaptiveOverlayRouteFamily::Multihop,
            ],
        );
        assert_eq!(
            plan.decision.selected_path,
            OverlayRuntimeSelectedPath::QueueFallback
        );
        assert_eq!(
            plan.decision.reason,
            crate::overlay_runtime::OverlayRuntimeDecisionReason::RouteHealthExhausted
        );
    }
}
