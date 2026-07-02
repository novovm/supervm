use serde::{Deserialize, Serialize};

use crate::control_plane::{ControlPlaneRegistry, ControlPlaneResolveError, PeerId};
use crate::overlay::{OverlayRouteDecision, OverlayTransportProfile, RouteSet};

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
        decide_overlay_runtime_route_v0, OverlayRuntimeDecisionReason,
        OverlayRuntimeReachabilityClass, OverlayRuntimeSelectedPath,
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
}
