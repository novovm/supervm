use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::{
        CapabilityAdvertisement, ControlPlaneFeature, ControlPlaneReadiness,
        Libp2pControlPlaneConfig, PeerId,
    };

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
}
