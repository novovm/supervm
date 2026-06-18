use serde::{Deserialize, Serialize};

use crate::control_plane::PeerId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayTransportProfile {
    DirectNovoRudp,
    RelayNovoRudp,
    Libp2pCircuitRelay,
    WebRtcRelay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteToken(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayHop {
    pub peer_id: PeerId,
    pub transport: OverlayTransportProfile,
    pub route_token: Option<RouteToken>,
}

impl OverlayHop {
    pub fn exposes_ip_address(&self) -> bool {
        self.peer_id.0.contains(':') || self.peer_id.0.parse::<std::net::IpAddr>().is_ok()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteSet {
    pub target_peer_id: PeerId,
    pub hops: Vec<OverlayHop>,
    pub content_address_hint: Option<String>,
}

impl RouteSet {
    pub fn direct(target_peer_id: PeerId) -> Self {
        Self {
            target_peer_id: target_peer_id.clone(),
            hops: vec![OverlayHop {
                peer_id: target_peer_id,
                transport: OverlayTransportProfile::DirectNovoRudp,
                route_token: None,
            }],
            content_address_hint: None,
        }
    }

    pub fn is_no_ip_identity_routed(&self) -> bool {
        !self.target_peer_id.0.trim().is_empty()
            && !self.target_peer_id.0.contains(':')
            && self.hops.iter().all(|hop| !hop.exposes_ip_address())
    }

    pub fn requires_relay(&self) -> bool {
        self.hops.iter().any(|hop| {
            matches!(
                hop.transport,
                OverlayTransportProfile::RelayNovoRudp
                    | OverlayTransportProfile::Libp2pCircuitRelay
                    | OverlayTransportProfile::WebRtcRelay
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AntiCensorshipProfile {
    pub no_ip_identity_routing: bool,
    pub relay_required_when_direct_blocked: bool,
    pub multi_hop_max: usize,
    pub camouflage_profile: Option<String>,
}

impl Default for AntiCensorshipProfile {
    fn default() -> Self {
        Self {
            no_ip_identity_routing: true,
            relay_required_when_direct_blocked: true,
            multi_hop_max: 3,
            camouflage_profile: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayRouteDecision {
    DirectAllowed,
    RelayRequired,
    RejectIpAddressedRoute,
    RejectTooManyHops,
    RejectMissingCamouflageProfile,
}

pub fn evaluate_overlay_route(
    route_set: &RouteSet,
    profile: &AntiCensorshipProfile,
) -> OverlayRouteDecision {
    if route_set.hops.len() > profile.multi_hop_max {
        return OverlayRouteDecision::RejectTooManyHops;
    }
    if profile.no_ip_identity_routing && !route_set.is_no_ip_identity_routed() {
        return OverlayRouteDecision::RejectIpAddressedRoute;
    }
    if profile.relay_required_when_direct_blocked
        && profile.camouflage_profile.as_ref().is_some_and(|profile| profile.trim().is_empty())
    {
        return OverlayRouteDecision::RejectMissingCamouflageProfile;
    }
    if route_set.requires_relay() {
        OverlayRouteDecision::RelayRequired
    } else if profile.relay_required_when_direct_blocked
        && profile.camouflage_profile.as_ref().is_some_and(|profile| !profile.trim().is_empty())
    {
        OverlayRouteDecision::RelayRequired
    } else {
        OverlayRouteDecision::DirectAllowed
    }
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_overlay_route, AntiCensorshipProfile, OverlayHop, OverlayRouteDecision,
        OverlayTransportProfile, RouteSet, RouteToken,
    };
    use crate::control_plane::PeerId;

    #[test]
    fn direct_no_ip_route_is_allowed() {
        let route = RouteSet::direct(PeerId::new("peer-target"));
        assert_eq!(
            evaluate_overlay_route(&route, &AntiCensorshipProfile::default()),
            OverlayRouteDecision::DirectAllowed
        );
    }

    #[test]
    fn ip_addressed_route_is_rejected_in_no_ip_profile() {
        let route = RouteSet::direct(PeerId::new("192.168.1.2:39001"));
        assert_eq!(
            evaluate_overlay_route(&route, &AntiCensorshipProfile::default()),
            OverlayRouteDecision::RejectIpAddressedRoute
        );
    }

    #[test]
    fn relay_route_is_explicitly_marked() {
        let route = RouteSet {
            target_peer_id: PeerId::new("peer-target"),
            hops: vec![OverlayHop {
                peer_id: PeerId::new("peer-relay"),
                transport: OverlayTransportProfile::Libp2pCircuitRelay,
                route_token: Some(RouteToken("token".into())),
            }],
            content_address_hint: Some("cid-example".into()),
        };
        assert_eq!(
            evaluate_overlay_route(&route, &AntiCensorshipProfile::default()),
            OverlayRouteDecision::RelayRequired
        );
    }

    #[test]
    fn camouflage_profile_can_force_relay_route() {
        let route = RouteSet::direct(PeerId::new("peer-target"));
        let profile = AntiCensorshipProfile {
            camouflage_profile: Some("webtransport-cover-v0".to_string()),
            ..AntiCensorshipProfile::default()
        };
        assert_eq!(
            evaluate_overlay_route(&route, &profile),
            OverlayRouteDecision::RelayRequired
        );
    }
}
