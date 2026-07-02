use serde::{Deserialize, Serialize};

use crate::control_plane::{ControlPlaneRegistry, ControlPlaneResolveError, PeerId};
use crate::overlay::{
    evaluate_overlay_route, AntiCensorshipProfile, OverlayRouteDecision, OverlayTransportProfile,
    RouteSet,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayRuntimeSelectedPath {
    DirectNovoRudp,
    RelayNovoRudp,
    MultiHopRelay,
    QueueFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayRuntimeReachabilityClass {
    Direct,
    RelayOnly,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayRuntimeDecisionReason {
    DirectAllowed,
    RelayRequired,
    MultiHopRelayRequired,
    DirectCoolingDown,
    RelayCoolingDown,
    RouteHealthExhausted,
    PeerUnknown,
    NovoRudpUnsupported,
    NativePipelineUnsupported,
    RouteSetMissing,
    IpAddressedRouteRejected,
    TooManyHops,
    MissingCamouflageProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayRuntimeDecision {
    pub target_peer_id: PeerId,
    pub selected_path: OverlayRuntimeSelectedPath,
    pub route_set: Option<RouteSet>,
    pub direct_endpoint_candidates: Vec<PeerId>,
    pub relay_candidates: Vec<PeerId>,
    pub multi_hop_candidates: Vec<Vec<PeerId>>,
    pub reachability_class: OverlayRuntimeReachabilityClass,
    pub reason: OverlayRuntimeDecisionReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayRouteHealthState {
    Healthy,
    Degraded,
    CoolingDown,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayHopHealth {
    pub peer_id: PeerId,
    pub state: OverlayRouteHealthState,
    pub last_failure_unix_ms: u64,
    pub cooldown_until_unix_ms: u64,
    pub observed_rtt_ms: Option<u32>,
}

impl OverlayHopHealth {
    pub fn healthy(peer_id: PeerId) -> Self {
        Self {
            peer_id,
            state: OverlayRouteHealthState::Healthy,
            last_failure_unix_ms: 0,
            cooldown_until_unix_ms: 0,
            observed_rtt_ms: None,
        }
    }

    pub fn cooling_down(peer_id: PeerId, now_unix_ms: u64, cooldown_until_unix_ms: u64) -> Self {
        Self {
            peer_id,
            state: OverlayRouteHealthState::CoolingDown,
            last_failure_unix_ms: now_unix_ms,
            cooldown_until_unix_ms,
            observed_rtt_ms: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayRouteHealthSnapshot {
    pub observed_unix_ms: u64,
    pub hops: Vec<OverlayHopHealth>,
}

impl OverlayRouteHealthSnapshot {
    pub fn new(observed_unix_ms: u64, hops: Vec<OverlayHopHealth>) -> Self {
        Self {
            observed_unix_ms,
            hops,
        }
    }

    pub fn hop_state(&self, peer_id: &PeerId) -> OverlayRouteHealthState {
        let Some(health) = self.hops.iter().find(|hop| hop.peer_id == *peer_id) else {
            return OverlayRouteHealthState::Healthy;
        };
        if health.cooldown_until_unix_ms > self.observed_unix_ms {
            return OverlayRouteHealthState::CoolingDown;
        }
        health.state
    }

    pub fn hop_is_usable(&self, peer_id: &PeerId) -> bool {
        matches!(
            self.hop_state(peer_id),
            OverlayRouteHealthState::Healthy | OverlayRouteHealthState::Degraded
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayRouteAttemptObservation {
    pub decision: OverlayRuntimeDecision,
    pub delivered: bool,
    pub queued: bool,
    pub observed_unix_ms: u64,
    pub cooldown_ms: u64,
}

pub fn overlay_route_health_from_observations_v0(
    observations: &[OverlayRouteAttemptObservation],
) -> OverlayRouteHealthSnapshot {
    let observed_unix_ms = observations
        .iter()
        .map(|observation| observation.observed_unix_ms)
        .max()
        .unwrap_or(0);
    let mut hops = Vec::new();
    for observation in observations {
        if observation.delivered || observation.queued {
            continue;
        }
        let cooldown_until_unix_ms = observation
            .observed_unix_ms
            .saturating_add(observation.cooldown_ms);
        for peer_id in failed_path_peer_ids(&observation.decision) {
            if !hops
                .iter()
                .any(|hop: &OverlayHopHealth| hop.peer_id == peer_id)
            {
                hops.push(OverlayHopHealth::cooling_down(
                    peer_id,
                    observation.observed_unix_ms,
                    cooldown_until_unix_ms,
                ));
            }
        }
    }
    OverlayRouteHealthSnapshot {
        observed_unix_ms,
        hops,
    }
}

fn failed_path_peer_ids(decision: &OverlayRuntimeDecision) -> Vec<PeerId> {
    match decision.selected_path {
        OverlayRuntimeSelectedPath::DirectNovoRudp => {
            if decision.direct_endpoint_candidates.is_empty() {
                vec![decision.target_peer_id.clone()]
            } else {
                decision.direct_endpoint_candidates.clone()
            }
        }
        OverlayRuntimeSelectedPath::RelayNovoRudp => decision
            .relay_candidates
            .first()
            .cloned()
            .into_iter()
            .collect(),
        OverlayRuntimeSelectedPath::MultiHopRelay => decision
            .multi_hop_candidates
            .first()
            .cloned()
            .unwrap_or_else(|| decision.relay_candidates.clone()),
        OverlayRuntimeSelectedPath::QueueFallback => Vec::new(),
    }
}

pub fn decide_overlay_runtime_route_v0(
    registry: &ControlPlaneRegistry,
    target_peer_id: &PeerId,
) -> OverlayRuntimeDecision {
    match registry.resolve_data_plane_route(target_peer_id) {
        Ok(resolved) => decision_from_route(
            target_peer_id.clone(),
            resolved.route_set,
            resolved.decision,
        ),
        Err(err) => queue_decision(target_peer_id.clone(), reason_from_resolve_error(err)),
    }
}

pub fn decide_overlay_runtime_route_with_health_v0(
    registry: &ControlPlaneRegistry,
    target_peer_id: &PeerId,
    health: &OverlayRouteHealthSnapshot,
) -> OverlayRuntimeDecision {
    match registry.resolve_data_plane_route(target_peer_id) {
        Ok(resolved) => decision_from_route_with_health(
            target_peer_id.clone(),
            resolved.route_set,
            resolved.decision,
            health,
        ),
        Err(err) => queue_decision(target_peer_id.clone(), reason_from_resolve_error(err)),
    }
}

pub fn decide_overlay_runtime_fallback_chain_v0(
    target_peer_id: &PeerId,
    candidate_route_sets: &[RouteSet],
    profile: &AntiCensorshipProfile,
    health: &OverlayRouteHealthSnapshot,
) -> OverlayRuntimeDecision {
    let mut last_queue_reason = OverlayRuntimeDecisionReason::RouteSetMissing;
    let mut matched_candidate_count = 0usize;
    for route_set in candidate_route_sets
        .iter()
        .filter(|route_set| route_set.target_peer_id == *target_peer_id)
    {
        matched_candidate_count += 1;
        let route_decision = evaluate_overlay_route(route_set, profile);
        let decision = decision_from_route_with_health(
            target_peer_id.clone(),
            route_set.clone(),
            route_decision,
            health,
        );
        if decision.selected_path != OverlayRuntimeSelectedPath::QueueFallback {
            return decision;
        }
        last_queue_reason = decision.reason;
    }
    if matched_candidate_count > 0
        && matches!(
            last_queue_reason,
            OverlayRuntimeDecisionReason::DirectCoolingDown
                | OverlayRuntimeDecisionReason::RelayCoolingDown
                | OverlayRuntimeDecisionReason::RouteHealthExhausted
        )
    {
        return queue_decision(
            target_peer_id.clone(),
            OverlayRuntimeDecisionReason::RouteHealthExhausted,
        );
    }
    queue_decision(target_peer_id.clone(), last_queue_reason)
}

fn decision_from_route(
    target_peer_id: PeerId,
    route_set: RouteSet,
    route_decision: OverlayRouteDecision,
) -> OverlayRuntimeDecision {
    let direct_endpoint_candidates: Vec<PeerId> = route_set
        .hops
        .iter()
        .filter(|hop| matches!(hop.transport, OverlayTransportProfile::DirectNovoRudp))
        .map(|hop| hop.peer_id.clone())
        .collect();
    let relay_candidates: Vec<PeerId> = route_set
        .hops
        .iter()
        .filter(|hop| {
            matches!(
                hop.transport,
                OverlayTransportProfile::RelayNovoRudp
                    | OverlayTransportProfile::Libp2pCircuitRelay
                    | OverlayTransportProfile::WebRtcRelay
            )
        })
        .map(|hop| hop.peer_id.clone())
        .collect();
    let multi_hop_candidates = if relay_candidates.len() > 1 {
        vec![relay_candidates.clone()]
    } else {
        Vec::new()
    };

    match route_decision {
        OverlayRouteDecision::DirectAllowed => OverlayRuntimeDecision {
            target_peer_id,
            selected_path: OverlayRuntimeSelectedPath::DirectNovoRudp,
            route_set: Some(route_set),
            direct_endpoint_candidates,
            relay_candidates,
            multi_hop_candidates,
            reachability_class: OverlayRuntimeReachabilityClass::Direct,
            reason: OverlayRuntimeDecisionReason::DirectAllowed,
        },
        OverlayRouteDecision::RelayRequired => {
            let selected_path = if multi_hop_candidates.is_empty() {
                OverlayRuntimeSelectedPath::RelayNovoRudp
            } else {
                OverlayRuntimeSelectedPath::MultiHopRelay
            };
            let reason = if selected_path == OverlayRuntimeSelectedPath::MultiHopRelay {
                OverlayRuntimeDecisionReason::MultiHopRelayRequired
            } else {
                OverlayRuntimeDecisionReason::RelayRequired
            };
            OverlayRuntimeDecision {
                target_peer_id,
                selected_path,
                route_set: Some(route_set),
                direct_endpoint_candidates,
                relay_candidates,
                multi_hop_candidates,
                reachability_class: OverlayRuntimeReachabilityClass::RelayOnly,
                reason,
            }
        }
        OverlayRouteDecision::RejectIpAddressedRoute => queue_decision(
            target_peer_id,
            OverlayRuntimeDecisionReason::IpAddressedRouteRejected,
        ),
        OverlayRouteDecision::RejectTooManyHops => {
            queue_decision(target_peer_id, OverlayRuntimeDecisionReason::TooManyHops)
        }
        OverlayRouteDecision::RejectMissingCamouflageProfile => queue_decision(
            target_peer_id,
            OverlayRuntimeDecisionReason::MissingCamouflageProfile,
        ),
    }
}

fn decision_from_route_with_health(
    target_peer_id: PeerId,
    route_set: RouteSet,
    route_decision: OverlayRouteDecision,
    health: &OverlayRouteHealthSnapshot,
) -> OverlayRuntimeDecision {
    match route_decision {
        OverlayRouteDecision::RejectIpAddressedRoute => {
            return queue_decision(
                target_peer_id,
                OverlayRuntimeDecisionReason::IpAddressedRouteRejected,
            );
        }
        OverlayRouteDecision::RejectTooManyHops => {
            return queue_decision(target_peer_id, OverlayRuntimeDecisionReason::TooManyHops);
        }
        OverlayRouteDecision::RejectMissingCamouflageProfile => {
            return queue_decision(
                target_peer_id,
                OverlayRuntimeDecisionReason::MissingCamouflageProfile,
            );
        }
        OverlayRouteDecision::DirectAllowed | OverlayRouteDecision::RelayRequired => {}
    }

    let direct_endpoint_candidates: Vec<PeerId> = route_set
        .hops
        .iter()
        .filter(|hop| matches!(hop.transport, OverlayTransportProfile::DirectNovoRudp))
        .map(|hop| hop.peer_id.clone())
        .collect();
    let relay_candidates: Vec<PeerId> = route_set
        .hops
        .iter()
        .filter(|hop| {
            matches!(
                hop.transport,
                OverlayTransportProfile::RelayNovoRudp
                    | OverlayTransportProfile::Libp2pCircuitRelay
                    | OverlayTransportProfile::WebRtcRelay
            )
        })
        .map(|hop| hop.peer_id.clone())
        .collect();
    let usable_direct: Vec<PeerId> = direct_endpoint_candidates
        .iter()
        .filter(|peer_id| health.hop_is_usable(peer_id))
        .cloned()
        .collect();
    let usable_relays: Vec<PeerId> = relay_candidates
        .iter()
        .filter(|peer_id| health.hop_is_usable(peer_id))
        .cloned()
        .collect();
    let multi_hop_candidates = if usable_relays.len() > 1 {
        vec![usable_relays.clone()]
    } else {
        Vec::new()
    };

    if !usable_direct.is_empty() {
        return OverlayRuntimeDecision {
            target_peer_id,
            selected_path: OverlayRuntimeSelectedPath::DirectNovoRudp,
            route_set: Some(route_set),
            direct_endpoint_candidates,
            relay_candidates,
            multi_hop_candidates,
            reachability_class: OverlayRuntimeReachabilityClass::Direct,
            reason: OverlayRuntimeDecisionReason::DirectAllowed,
        };
    }
    if usable_relays.len() > 1 {
        return OverlayRuntimeDecision {
            target_peer_id,
            selected_path: OverlayRuntimeSelectedPath::MultiHopRelay,
            route_set: Some(route_set),
            direct_endpoint_candidates,
            relay_candidates,
            multi_hop_candidates,
            reachability_class: OverlayRuntimeReachabilityClass::RelayOnly,
            reason: OverlayRuntimeDecisionReason::MultiHopRelayRequired,
        };
    }
    if usable_relays.len() == 1 {
        return OverlayRuntimeDecision {
            target_peer_id,
            selected_path: OverlayRuntimeSelectedPath::RelayNovoRudp,
            route_set: Some(route_set),
            direct_endpoint_candidates,
            relay_candidates,
            multi_hop_candidates,
            reachability_class: OverlayRuntimeReachabilityClass::RelayOnly,
            reason: OverlayRuntimeDecisionReason::DirectCoolingDown,
        };
    }

    let direct_unusable = direct_endpoint_candidates
        .iter()
        .any(|peer_id| !health.hop_is_usable(peer_id));
    let relay_unusable = relay_candidates
        .iter()
        .any(|peer_id| !health.hop_is_usable(peer_id));
    let reason = match (direct_unusable, relay_unusable) {
        (true, true) => OverlayRuntimeDecisionReason::RouteHealthExhausted,
        (true, false) => OverlayRuntimeDecisionReason::DirectCoolingDown,
        (false, true) => OverlayRuntimeDecisionReason::RelayCoolingDown,
        (false, false) => OverlayRuntimeDecisionReason::RouteHealthExhausted,
    };
    queue_decision(target_peer_id, reason)
}

fn queue_decision(
    target_peer_id: PeerId,
    reason: OverlayRuntimeDecisionReason,
) -> OverlayRuntimeDecision {
    OverlayRuntimeDecision {
        target_peer_id,
        selected_path: OverlayRuntimeSelectedPath::QueueFallback,
        route_set: None,
        direct_endpoint_candidates: Vec::new(),
        relay_candidates: Vec::new(),
        multi_hop_candidates: Vec::new(),
        reachability_class: OverlayRuntimeReachabilityClass::Unknown,
        reason,
    }
}

fn reason_from_resolve_error(err: ControlPlaneResolveError) -> OverlayRuntimeDecisionReason {
    match err {
        ControlPlaneResolveError::PeerUnknown => OverlayRuntimeDecisionReason::PeerUnknown,
        ControlPlaneResolveError::NovoRudpUnsupported => {
            OverlayRuntimeDecisionReason::NovoRudpUnsupported
        }
        ControlPlaneResolveError::NativePipelineUnsupported => {
            OverlayRuntimeDecisionReason::NativePipelineUnsupported
        }
        ControlPlaneResolveError::RouteSetMissing => OverlayRuntimeDecisionReason::RouteSetMissing,
        ControlPlaneResolveError::IpAddressedRouteRejected => {
            OverlayRuntimeDecisionReason::IpAddressedRouteRejected
        }
        ControlPlaneResolveError::TooManyHops => OverlayRuntimeDecisionReason::TooManyHops,
        ControlPlaneResolveError::MissingCamouflageProfile => {
            OverlayRuntimeDecisionReason::MissingCamouflageProfile
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decide_overlay_runtime_fallback_chain_v0, decide_overlay_runtime_route_v0,
        decide_overlay_runtime_route_with_health_v0, overlay_route_health_from_observations_v0,
        OverlayHopHealth, OverlayRouteAttemptObservation, OverlayRouteHealthSnapshot,
        OverlayRuntimeDecisionReason, OverlayRuntimeReachabilityClass, OverlayRuntimeSelectedPath,
    };
    use crate::control_plane::{
        CapabilityAdvertisement, ControlPlaneRegistry, Libp2pControlPlaneConfig, PeerId,
    };
    use crate::overlay::{AntiCensorshipProfile, OverlayHop, OverlayTransportProfile, RouteSet};

    fn registry() -> ControlPlaneRegistry {
        ControlPlaneRegistry::new(
            Libp2pControlPlaneConfig::production_minimum(PeerId::new("peer-local")),
            AntiCensorshipProfile::default(),
        )
    }

    fn register_native_peer(registry: &mut ControlPlaneRegistry, peer_id: &PeerId) {
        registry.register_advertisement(
            CapabilityAdvertisement {
                peer_id: peer_id.clone(),
                protocols: vec!["novorudp/0".into(), "native-pipeline/1".into()],
                no_ip_identity_routing: true,
            },
            100,
        );
    }

    fn mixed_direct_relay_route(peer_id: PeerId) -> RouteSet {
        RouteSet {
            target_peer_id: peer_id.clone(),
            hops: vec![
                OverlayHop {
                    peer_id,
                    transport: OverlayTransportProfile::DirectNovoRudp,
                    route_token: None,
                },
                OverlayHop {
                    peer_id: PeerId::new("peer-relay-a"),
                    transport: OverlayTransportProfile::Libp2pCircuitRelay,
                    route_token: None,
                },
                OverlayHop {
                    peer_id: PeerId::new("peer-relay-b"),
                    transport: OverlayTransportProfile::RelayNovoRudp,
                    route_token: None,
                },
            ],
            content_address_hint: Some("cid-route".into()),
        }
    }

    fn fallback_chain_routes(peer_id: PeerId) -> Vec<RouteSet> {
        vec![
            RouteSet::direct(peer_id.clone()),
            RouteSet {
                target_peer_id: peer_id.clone(),
                hops: vec![OverlayHop {
                    peer_id: PeerId::new("peer-relay-a"),
                    transport: OverlayTransportProfile::RelayNovoRudp,
                    route_token: None,
                }],
                content_address_hint: Some("cid-relay".into()),
            },
            RouteSet {
                target_peer_id: peer_id,
                hops: vec![
                    OverlayHop {
                        peer_id: PeerId::new("peer-relay-b"),
                        transport: OverlayTransportProfile::Libp2pCircuitRelay,
                        route_token: None,
                    },
                    OverlayHop {
                        peer_id: PeerId::new("peer-relay-c"),
                        transport: OverlayTransportProfile::RelayNovoRudp,
                        route_token: None,
                    },
                ],
                content_address_hint: Some("cid-multihop".into()),
            },
        ]
    }

    #[test]
    fn direct_route_selects_direct_novorudp() {
        let mut registry = registry();
        let peer_id = PeerId::new("peer-target");
        register_native_peer(&mut registry, &peer_id);
        registry.register_route_set(RouteSet::direct(peer_id.clone()));

        let decision = decide_overlay_runtime_route_v0(&registry, &peer_id);
        assert_eq!(
            decision.selected_path,
            OverlayRuntimeSelectedPath::DirectNovoRudp
        );
        assert_eq!(
            decision.reachability_class,
            OverlayRuntimeReachabilityClass::Direct
        );
        assert_eq!(decision.reason, OverlayRuntimeDecisionReason::DirectAllowed);
        assert_eq!(decision.direct_endpoint_candidates, vec![peer_id]);
    }

    #[test]
    fn relay_route_selects_relay_novorudp() {
        let mut registry = registry();
        let peer_id = PeerId::new("peer-target");
        register_native_peer(&mut registry, &peer_id);
        registry.register_route_set(RouteSet {
            target_peer_id: peer_id.clone(),
            hops: vec![OverlayHop {
                peer_id: PeerId::new("peer-relay"),
                transport: OverlayTransportProfile::Libp2pCircuitRelay,
                route_token: None,
            }],
            content_address_hint: Some("cid-route".into()),
        });

        let decision = decide_overlay_runtime_route_v0(&registry, &peer_id);
        assert_eq!(
            decision.selected_path,
            OverlayRuntimeSelectedPath::RelayNovoRudp
        );
        assert_eq!(
            decision.reachability_class,
            OverlayRuntimeReachabilityClass::RelayOnly
        );
        assert_eq!(decision.reason, OverlayRuntimeDecisionReason::RelayRequired);
        assert_eq!(decision.relay_candidates, vec![PeerId::new("peer-relay")]);
    }

    #[test]
    fn multi_hop_route_selects_multi_hop_relay() {
        let mut registry = registry();
        let peer_id = PeerId::new("peer-target");
        register_native_peer(&mut registry, &peer_id);
        registry.register_route_set(RouteSet {
            target_peer_id: peer_id.clone(),
            hops: vec![
                OverlayHop {
                    peer_id: PeerId::new("peer-relay-a"),
                    transport: OverlayTransportProfile::Libp2pCircuitRelay,
                    route_token: None,
                },
                OverlayHop {
                    peer_id: PeerId::new("peer-relay-b"),
                    transport: OverlayTransportProfile::RelayNovoRudp,
                    route_token: None,
                },
            ],
            content_address_hint: Some("cid-route".into()),
        });

        let decision = decide_overlay_runtime_route_v0(&registry, &peer_id);
        assert_eq!(
            decision.selected_path,
            OverlayRuntimeSelectedPath::MultiHopRelay
        );
        assert_eq!(
            decision.reason,
            OverlayRuntimeDecisionReason::MultiHopRelayRequired
        );
        assert_eq!(
            decision.multi_hop_candidates,
            vec![vec![
                PeerId::new("peer-relay-a"),
                PeerId::new("peer-relay-b")
            ]]
        );
    }

    #[test]
    fn unknown_peer_falls_back_to_queue() {
        let registry = registry();
        let peer_id = PeerId::new("peer-missing");

        let decision = decide_overlay_runtime_route_v0(&registry, &peer_id);
        assert_eq!(
            decision.selected_path,
            OverlayRuntimeSelectedPath::QueueFallback
        );
        assert_eq!(
            decision.reachability_class,
            OverlayRuntimeReachabilityClass::Unknown
        );
        assert_eq!(decision.reason, OverlayRuntimeDecisionReason::PeerUnknown);
    }

    #[test]
    fn health_aware_route_prefers_healthy_direct_in_mixed_route_set() {
        let mut registry = registry();
        let peer_id = PeerId::new("peer-target");
        register_native_peer(&mut registry, &peer_id);
        registry.register_route_set(mixed_direct_relay_route(peer_id.clone()));

        let health = OverlayRouteHealthSnapshot::new(1_000, Vec::new());
        let decision = decide_overlay_runtime_route_with_health_v0(&registry, &peer_id, &health);

        assert_eq!(
            decision.selected_path,
            OverlayRuntimeSelectedPath::DirectNovoRudp
        );
        assert_eq!(decision.reason, OverlayRuntimeDecisionReason::DirectAllowed);
    }

    #[test]
    fn health_aware_route_falls_back_to_multihop_when_direct_cools_down() {
        let mut registry = registry();
        let peer_id = PeerId::new("peer-target");
        register_native_peer(&mut registry, &peer_id);
        registry.register_route_set(mixed_direct_relay_route(peer_id.clone()));

        let health = OverlayRouteHealthSnapshot::new(
            1_000,
            vec![OverlayHopHealth::cooling_down(peer_id.clone(), 900, 2_000)],
        );
        let decision = decide_overlay_runtime_route_with_health_v0(&registry, &peer_id, &health);

        assert_eq!(
            decision.selected_path,
            OverlayRuntimeSelectedPath::MultiHopRelay
        );
        assert_eq!(
            decision.reason,
            OverlayRuntimeDecisionReason::MultiHopRelayRequired
        );
    }

    #[test]
    fn health_aware_route_falls_back_to_single_relay_when_one_relay_cools_down() {
        let mut registry = registry();
        let peer_id = PeerId::new("peer-target");
        register_native_peer(&mut registry, &peer_id);
        registry.register_route_set(mixed_direct_relay_route(peer_id.clone()));

        let health = OverlayRouteHealthSnapshot::new(
            1_000,
            vec![
                OverlayHopHealth::cooling_down(peer_id.clone(), 900, 2_000),
                OverlayHopHealth::cooling_down(PeerId::new("peer-relay-a"), 900, 2_000),
            ],
        );
        let decision = decide_overlay_runtime_route_with_health_v0(&registry, &peer_id, &health);

        assert_eq!(
            decision.selected_path,
            OverlayRuntimeSelectedPath::RelayNovoRudp
        );
        assert_eq!(
            decision.reason,
            OverlayRuntimeDecisionReason::DirectCoolingDown
        );
    }

    #[test]
    fn health_aware_route_queues_when_all_hops_cool_down() {
        let mut registry = registry();
        let peer_id = PeerId::new("peer-target");
        register_native_peer(&mut registry, &peer_id);
        registry.register_route_set(mixed_direct_relay_route(peer_id.clone()));

        let health = OverlayRouteHealthSnapshot::new(
            1_000,
            vec![
                OverlayHopHealth::cooling_down(peer_id.clone(), 900, 2_000),
                OverlayHopHealth::cooling_down(PeerId::new("peer-relay-a"), 900, 2_000),
                OverlayHopHealth::cooling_down(PeerId::new("peer-relay-b"), 900, 2_000),
            ],
        );
        let decision = decide_overlay_runtime_route_with_health_v0(&registry, &peer_id, &health);

        assert_eq!(
            decision.selected_path,
            OverlayRuntimeSelectedPath::QueueFallback
        );
        assert_eq!(
            decision.reason,
            OverlayRuntimeDecisionReason::RouteHealthExhausted
        );
    }

    #[test]
    fn failed_direct_observation_builds_health_snapshot_for_multihop_fallback() {
        let mut registry = registry();
        let peer_id = PeerId::new("peer-target");
        register_native_peer(&mut registry, &peer_id);
        registry.register_route_set(mixed_direct_relay_route(peer_id.clone()));

        let initial_health = OverlayRouteHealthSnapshot::new(1_000, Vec::new());
        let direct_decision =
            decide_overlay_runtime_route_with_health_v0(&registry, &peer_id, &initial_health);
        assert_eq!(
            direct_decision.selected_path,
            OverlayRuntimeSelectedPath::DirectNovoRudp
        );

        let health = overlay_route_health_from_observations_v0(&[OverlayRouteAttemptObservation {
            decision: direct_decision,
            delivered: false,
            queued: false,
            observed_unix_ms: 1_000,
            cooldown_ms: 60_000,
        }]);
        let next_decision =
            decide_overlay_runtime_route_with_health_v0(&registry, &peer_id, &health);

        assert_eq!(
            next_decision.selected_path,
            OverlayRuntimeSelectedPath::MultiHopRelay
        );
    }

    #[test]
    fn failed_multihop_observation_builds_health_snapshot_for_queue_fallback() {
        let mut registry = registry();
        let peer_id = PeerId::new("peer-target");
        register_native_peer(&mut registry, &peer_id);
        registry.register_route_set(mixed_direct_relay_route(peer_id.clone()));

        let direct_cooldown = OverlayRouteHealthSnapshot::new(
            1_000,
            vec![OverlayHopHealth::cooling_down(peer_id.clone(), 900, 2_000)],
        );
        let multihop_decision =
            decide_overlay_runtime_route_with_health_v0(&registry, &peer_id, &direct_cooldown);
        assert_eq!(
            multihop_decision.selected_path,
            OverlayRuntimeSelectedPath::MultiHopRelay
        );

        let health = overlay_route_health_from_observations_v0(&[
            OverlayRouteAttemptObservation {
                decision: multihop_decision,
                delivered: false,
                queued: false,
                observed_unix_ms: 1_000,
                cooldown_ms: 60_000,
            },
            OverlayRouteAttemptObservation {
                decision: decide_overlay_runtime_route_with_health_v0(
                    &registry,
                    &peer_id,
                    &OverlayRouteHealthSnapshot::new(1_000, Vec::new()),
                ),
                delivered: false,
                queued: false,
                observed_unix_ms: 1_000,
                cooldown_ms: 60_000,
            },
        ]);
        let next_decision =
            decide_overlay_runtime_route_with_health_v0(&registry, &peer_id, &health);

        assert_eq!(
            next_decision.selected_path,
            OverlayRuntimeSelectedPath::QueueFallback
        );
        assert_eq!(
            next_decision.reason,
            OverlayRuntimeDecisionReason::RouteHealthExhausted
        );
    }

    #[test]
    fn fallback_chain_steps_direct_relay_multihop_queue() {
        let peer_id = PeerId::new("peer-target");
        let routes = fallback_chain_routes(peer_id.clone());
        let profile = AntiCensorshipProfile::default();
        let empty_health = OverlayRouteHealthSnapshot::new(1_000, Vec::new());

        let direct =
            decide_overlay_runtime_fallback_chain_v0(&peer_id, &routes, &profile, &empty_health);
        assert_eq!(
            direct.selected_path,
            OverlayRuntimeSelectedPath::DirectNovoRudp
        );

        let direct_failed =
            overlay_route_health_from_observations_v0(&[OverlayRouteAttemptObservation {
                decision: direct.clone(),
                delivered: false,
                queued: false,
                observed_unix_ms: 1_000,
                cooldown_ms: 60_000,
            }]);
        let relay =
            decide_overlay_runtime_fallback_chain_v0(&peer_id, &routes, &profile, &direct_failed);
        assert_eq!(
            relay.selected_path,
            OverlayRuntimeSelectedPath::RelayNovoRudp
        );

        let direct_and_relay_failed = overlay_route_health_from_observations_v0(&[
            OverlayRouteAttemptObservation {
                decision: direct,
                delivered: false,
                queued: false,
                observed_unix_ms: 1_000,
                cooldown_ms: 60_000,
            },
            OverlayRouteAttemptObservation {
                decision: relay.clone(),
                delivered: false,
                queued: false,
                observed_unix_ms: 1_000,
                cooldown_ms: 60_000,
            },
        ]);
        let multihop = decide_overlay_runtime_fallback_chain_v0(
            &peer_id,
            &routes,
            &profile,
            &direct_and_relay_failed,
        );
        assert_eq!(
            multihop.selected_path,
            OverlayRuntimeSelectedPath::MultiHopRelay
        );

        let all_failed = overlay_route_health_from_observations_v0(&[
            OverlayRouteAttemptObservation {
                decision: relay,
                delivered: false,
                queued: false,
                observed_unix_ms: 1_000,
                cooldown_ms: 60_000,
            },
            OverlayRouteAttemptObservation {
                decision: multihop,
                delivered: false,
                queued: false,
                observed_unix_ms: 1_000,
                cooldown_ms: 60_000,
            },
            OverlayRouteAttemptObservation {
                decision: decide_overlay_runtime_fallback_chain_v0(
                    &peer_id,
                    &routes,
                    &profile,
                    &empty_health,
                ),
                delivered: false,
                queued: false,
                observed_unix_ms: 1_000,
                cooldown_ms: 60_000,
            },
        ]);
        let queue =
            decide_overlay_runtime_fallback_chain_v0(&peer_id, &routes, &profile, &all_failed);
        assert_eq!(
            queue.selected_path,
            OverlayRuntimeSelectedPath::QueueFallback
        );
        assert_eq!(
            queue.reason,
            OverlayRuntimeDecisionReason::RouteHealthExhausted
        );
    }
}
