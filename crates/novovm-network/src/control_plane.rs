use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::overlay::{
    evaluate_overlay_route, AntiCensorshipProfile, OverlayRouteDecision, RouteSet,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId(pub String);

impl PeerId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn is_valid_identity_hint(&self) -> bool {
        let value = self.0.trim();
        !value.is_empty() && !value.contains('/') && !value.contains(':')
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlPlaneFeature {
    PeerId,
    Dht,
    Identify,
    AutoNat,
    CircuitRelay,
    CapabilityExchange,
    RouteSetDiscovery,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Libp2pControlPlaneConfig {
    pub peer_id: PeerId,
    pub enable_dht: bool,
    pub enable_identify: bool,
    pub enable_autonat: bool,
    pub enable_circuit_relay: bool,
    pub enable_capability_exchange: bool,
    pub enable_routeset_discovery: bool,
}

impl Libp2pControlPlaneConfig {
    pub fn production_minimum(peer_id: PeerId) -> Self {
        Self {
            peer_id,
            enable_dht: true,
            enable_identify: true,
            enable_autonat: true,
            enable_circuit_relay: true,
            enable_capability_exchange: true,
            enable_routeset_discovery: true,
        }
    }

    pub fn enabled_features(&self) -> Vec<ControlPlaneFeature> {
        let mut features = vec![ControlPlaneFeature::PeerId];
        if self.enable_dht {
            features.push(ControlPlaneFeature::Dht);
        }
        if self.enable_identify {
            features.push(ControlPlaneFeature::Identify);
        }
        if self.enable_autonat {
            features.push(ControlPlaneFeature::AutoNat);
        }
        if self.enable_circuit_relay {
            features.push(ControlPlaneFeature::CircuitRelay);
        }
        if self.enable_capability_exchange {
            features.push(ControlPlaneFeature::CapabilityExchange);
        }
        if self.enable_routeset_discovery {
            features.push(ControlPlaneFeature::RouteSetDiscovery);
        }
        features
    }

    pub fn readiness(&self) -> ControlPlaneReadiness {
        if !self.peer_id.is_valid_identity_hint() {
            return ControlPlaneReadiness::InvalidIdentity;
        }
        if self.enable_dht
            && self.enable_identify
            && self.enable_autonat
            && self.enable_circuit_relay
            && self.enable_capability_exchange
            && self.enable_routeset_discovery
        {
            ControlPlaneReadiness::ProductionMinimum
        } else {
            ControlPlaneReadiness::Partial
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlPlaneReadiness {
    ProductionMinimum,
    Partial,
    InvalidIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityAdvertisement {
    pub peer_id: PeerId,
    pub protocols: Vec<String>,
    pub no_ip_identity_routing: bool,
}

impl CapabilityAdvertisement {
    pub fn supports_novorudp(&self) -> bool {
        self.protocols
            .iter()
            .any(|protocol| protocol == "novorudp/0")
    }

    pub fn supports_native_pipeline(&self) -> bool {
        self.protocols
            .iter()
            .any(|protocol| protocol == "native-pipeline/1")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerControlRecord {
    pub advertisement: CapabilityAdvertisement,
    pub route_set: Option<RouteSet>,
    pub last_seen_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlPlaneRegistry {
    pub local: Libp2pControlPlaneConfig,
    pub anti_censorship_profile: AntiCensorshipProfile,
    peers: BTreeMap<String, PeerControlRecord>,
}

impl ControlPlaneRegistry {
    pub fn new(
        local: Libp2pControlPlaneConfig,
        anti_censorship_profile: AntiCensorshipProfile,
    ) -> Self {
        Self {
            local,
            anti_censorship_profile,
            peers: BTreeMap::new(),
        }
    }

    pub fn register_advertisement(
        &mut self,
        advertisement: CapabilityAdvertisement,
        last_seen_unix_ms: u64,
    ) -> ControlPlaneRegisterResult {
        if !advertisement.peer_id.is_valid_identity_hint() {
            return ControlPlaneRegisterResult::RejectedInvalidPeerId;
        }
        if self.anti_censorship_profile.no_ip_identity_routing
            && !advertisement.no_ip_identity_routing
        {
            return ControlPlaneRegisterResult::RejectedNoIpRoutingRequired;
        }
        let key = advertisement.peer_id.0.clone();
        let existed = self.peers.contains_key(&key);
        let route_set = self
            .peers
            .get(&key)
            .and_then(|record| record.route_set.clone());
        self.peers.insert(
            key,
            PeerControlRecord {
                advertisement,
                route_set,
                last_seen_unix_ms,
            },
        );
        if existed {
            ControlPlaneRegisterResult::Updated
        } else {
            ControlPlaneRegisterResult::Inserted
        }
    }

    pub fn register_route_set(&mut self, route_set: RouteSet) -> ControlPlaneRegisterResult {
        if !route_set.target_peer_id.is_valid_identity_hint() {
            return ControlPlaneRegisterResult::RejectedInvalidPeerId;
        }
        match evaluate_overlay_route(&route_set, &self.anti_censorship_profile) {
            OverlayRouteDecision::RejectIpAddressedRoute => {
                return ControlPlaneRegisterResult::RejectedIpAddressedRoute;
            }
            OverlayRouteDecision::RejectTooManyHops => {
                return ControlPlaneRegisterResult::RejectedTooManyHops;
            }
            OverlayRouteDecision::RejectMissingCamouflageProfile => {
                return ControlPlaneRegisterResult::RejectedMissingCamouflageProfile;
            }
            OverlayRouteDecision::DirectAllowed | OverlayRouteDecision::RelayRequired => {}
        }
        let key = route_set.target_peer_id.0.clone();
        if let Some(record) = self.peers.get_mut(&key) {
            record.route_set = Some(route_set);
            ControlPlaneRegisterResult::Updated
        } else {
            self.peers.insert(
                key.clone(),
                PeerControlRecord {
                    advertisement: CapabilityAdvertisement {
                        peer_id: PeerId::new(key),
                        protocols: Vec::new(),
                        no_ip_identity_routing: true,
                    },
                    route_set: Some(route_set),
                    last_seen_unix_ms: 0,
                },
            );
            ControlPlaneRegisterResult::Inserted
        }
    }

    pub fn peer(&self, peer_id: &PeerId) -> Option<&PeerControlRecord> {
        self.peers.get(peer_id.0.as_str())
    }

    pub fn route_set_for(&self, peer_id: &PeerId) -> Option<&RouteSet> {
        self.peer(peer_id)?.route_set.as_ref()
    }

    pub fn novorudp_capable_peers(&self) -> Vec<PeerId> {
        self.peers
            .values()
            .filter(|record| {
                record.advertisement.supports_novorudp()
                    && record.advertisement.supports_native_pipeline()
                    && record.route_set.is_some()
            })
            .map(|record| record.advertisement.peer_id.clone())
            .collect()
    }

    pub fn resolve_data_plane_route(
        &self,
        peer_id: &PeerId,
    ) -> Result<ResolvedDataPlaneRoute, ControlPlaneResolveError> {
        let record = self
            .peer(peer_id)
            .ok_or(ControlPlaneResolveError::PeerUnknown)?;
        if !record.advertisement.supports_novorudp() {
            return Err(ControlPlaneResolveError::NovoRudpUnsupported);
        }
        if !record.advertisement.supports_native_pipeline() {
            return Err(ControlPlaneResolveError::NativePipelineUnsupported);
        }
        let route_set = record
            .route_set
            .clone()
            .ok_or(ControlPlaneResolveError::RouteSetMissing)?;
        let decision = evaluate_overlay_route(&route_set, &self.anti_censorship_profile);
        match decision {
            OverlayRouteDecision::RejectIpAddressedRoute => {
                Err(ControlPlaneResolveError::IpAddressedRouteRejected)
            }
            OverlayRouteDecision::RejectTooManyHops => Err(ControlPlaneResolveError::TooManyHops),
            OverlayRouteDecision::RejectMissingCamouflageProfile => {
                Err(ControlPlaneResolveError::MissingCamouflageProfile)
            }
            OverlayRouteDecision::DirectAllowed | OverlayRouteDecision::RelayRequired => {
                Ok(ResolvedDataPlaneRoute {
                    peer_id: peer_id.clone(),
                    route_set,
                    decision,
                })
            }
        }
    }

    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlPlaneRegisterResult {
    Inserted,
    Updated,
    RejectedInvalidPeerId,
    RejectedNoIpRoutingRequired,
    RejectedIpAddressedRoute,
    RejectedTooManyHops,
    RejectedMissingCamouflageProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedDataPlaneRoute {
    pub peer_id: PeerId,
    pub route_set: RouteSet,
    pub decision: OverlayRouteDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlPlaneResolveError {
    PeerUnknown,
    NovoRudpUnsupported,
    NativePipelineUnsupported,
    RouteSetMissing,
    IpAddressedRouteRejected,
    TooManyHops,
    MissingCamouflageProfile,
}

#[cfg(test)]
mod tests {
    use super::{
        CapabilityAdvertisement, ControlPlaneFeature, ControlPlaneReadiness,
        ControlPlaneRegisterResult, ControlPlaneRegistry, ControlPlaneResolveError,
        Libp2pControlPlaneConfig, PeerId,
    };
    use crate::overlay::{
        AntiCensorshipProfile, OverlayHop, OverlayRouteDecision, OverlayTransportProfile, RouteSet,
    };
    use crate::{build_repair_plan, NovoRudpRange, NovoRudpWindowConfig};

    #[test]
    fn production_minimum_enables_required_features() {
        let config = Libp2pControlPlaneConfig::production_minimum(PeerId::new("peer-abc"));
        assert_eq!(config.readiness(), ControlPlaneReadiness::ProductionMinimum);
        assert!(config
            .enabled_features()
            .contains(&ControlPlaneFeature::Dht));
        assert!(config
            .enabled_features()
            .contains(&ControlPlaneFeature::CircuitRelay));
    }

    #[test]
    fn peer_id_rejects_ip_like_address_hints() {
        let config = Libp2pControlPlaneConfig::production_minimum(PeerId::new("127.0.0.1:39001"));
        assert_eq!(config.readiness(), ControlPlaneReadiness::InvalidIdentity);
    }

    #[test]
    fn advertisement_checks_protocol_support() {
        let ad = CapabilityAdvertisement {
            peer_id: PeerId::new("peer-abc"),
            protocols: vec!["novorudp/0".into(), "native-pipeline/1".into()],
            no_ip_identity_routing: true,
        };
        assert!(ad.supports_novorudp());
        assert!(ad.supports_native_pipeline());
    }

    #[test]
    fn registry_resolves_novorudp_relay_route_without_ip_identity() {
        let local = Libp2pControlPlaneConfig::production_minimum(PeerId::new("peer-local"));
        let mut registry = ControlPlaneRegistry::new(local, AntiCensorshipProfile::default());
        let peer_id = PeerId::new("peer-target");
        let inserted = registry.register_advertisement(
            CapabilityAdvertisement {
                peer_id: peer_id.clone(),
                protocols: vec!["novorudp/0".into(), "native-pipeline/1".into()],
                no_ip_identity_routing: true,
            },
            100,
        );
        assert_eq!(inserted, ControlPlaneRegisterResult::Inserted);
        let route_result = registry.register_route_set(RouteSet {
            target_peer_id: peer_id.clone(),
            hops: vec![OverlayHop {
                peer_id: PeerId::new("peer-relay"),
                transport: OverlayTransportProfile::Libp2pCircuitRelay,
                route_token: None,
            }],
            content_address_hint: Some("cid-route".into()),
        });
        assert_eq!(route_result, ControlPlaneRegisterResult::Updated);

        let resolved = registry
            .resolve_data_plane_route(&peer_id)
            .expect("resolved route");
        assert_eq!(resolved.decision, OverlayRouteDecision::RelayRequired);
        assert_eq!(registry.novorudp_capable_peers(), vec![peer_id]);
    }

    #[test]
    fn registry_rejects_ip_addressed_route_in_no_ip_profile() {
        let local = Libp2pControlPlaneConfig::production_minimum(PeerId::new("peer-local"));
        let mut registry = ControlPlaneRegistry::new(local, AntiCensorshipProfile::default());
        let result = registry.register_route_set(RouteSet::direct(PeerId::new("127.0.0.1:39001")));
        assert_eq!(result, ControlPlaneRegisterResult::RejectedInvalidPeerId);
    }

    #[test]
    fn registry_requires_novorudp_for_data_plane_resolution() {
        let local = Libp2pControlPlaneConfig::production_minimum(PeerId::new("peer-local"));
        let mut registry = ControlPlaneRegistry::new(local, AntiCensorshipProfile::default());
        let peer_id = PeerId::new("peer-target");
        registry.register_advertisement(
            CapabilityAdvertisement {
                peer_id: peer_id.clone(),
                protocols: vec!["native-pipeline/1".into()],
                no_ip_identity_routing: true,
            },
            100,
        );
        registry.register_route_set(RouteSet::direct(peer_id.clone()));
        assert_eq!(
            registry.resolve_data_plane_route(&peer_id),
            Err(ControlPlaneResolveError::NovoRudpUnsupported)
        );
    }

    #[test]
    fn resolved_peer_can_build_novorudp_window_repair_plan() {
        let local = Libp2pControlPlaneConfig::production_minimum(PeerId::new("peer-local"));
        let mut registry = ControlPlaneRegistry::new(local, AntiCensorshipProfile::default());
        let peer_id = PeerId::new("peer-target");
        registry.register_advertisement(
            CapabilityAdvertisement {
                peer_id: peer_id.clone(),
                protocols: vec!["novorudp/0".into(), "native-pipeline/1".into()],
                no_ip_identity_routing: true,
            },
            100,
        );
        registry.register_route_set(RouteSet::direct(peer_id.clone()));
        let resolved = registry
            .resolve_data_plane_route(&peer_id)
            .expect("resolved novorudp route");
        assert_eq!(resolved.decision, OverlayRouteDecision::DirectAllowed);

        let plan = build_repair_plan(
            &[NovoRudpRange::new(14112, 14399)],
            14400,
            &NovoRudpWindowConfig::default(),
        )
        .expect("repair plan");
        assert_eq!(plan.window.range, NovoRudpRange::new(14112, 14175));
        assert_eq!(plan.window.missing_count, 64);
    }
}
