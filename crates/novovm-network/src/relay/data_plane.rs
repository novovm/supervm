use serde::{Deserialize, Serialize};

use crate::control_plane::PeerId;
use crate::overlay_runtime::{OverlayRuntimeDecision, OverlayRuntimeSelectedPath};
use crate::relay::{MultiHopRelayFrame, RelayServer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelayDataPlanePath {
    Direct,
    Relay,
    MultiHop,
    QueueFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayDataPlaneInput {
    pub request_id: String,
    pub source_peer_id: PeerId,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayDataPlaneResult {
    pub request_id: String,
    pub source_peer_id: PeerId,
    pub target_peer_id: PeerId,
    pub path: RelayDataPlanePath,
    pub delivered: bool,
    pub visited_hops: Vec<PeerId>,
    pub queued: bool,
    pub payload: Vec<u8>,
}

pub fn forward_novorudp_data_plane_v0(
    relay_server: &RelayServer,
    decision: &OverlayRuntimeDecision,
    input: RelayDataPlaneInput,
) -> RelayDataPlaneResult {
    match decision.selected_path {
        OverlayRuntimeSelectedPath::DirectNovoRudp => RelayDataPlaneResult {
            request_id: input.request_id,
            source_peer_id: input.source_peer_id,
            target_peer_id: decision.target_peer_id.clone(),
            path: RelayDataPlanePath::Direct,
            delivered: true,
            visited_hops: Vec::new(),
            queued: false,
            payload: input.payload,
        },
        OverlayRuntimeSelectedPath::RelayNovoRudp => {
            let hop_ids = decision
                .relay_candidates
                .iter()
                .map(|peer| peer.0.clone())
                .collect::<Vec<_>>();
            let frame = MultiHopRelayFrame::new(
                input.request_id.clone(),
                input.source_peer_id.0.clone(),
                decision.target_peer_id.0.clone(),
                hop_ids,
                Vec::new(),
                3,
                input.payload,
            );
            let result = relay_server.forward_multihop(frame);
            RelayDataPlaneResult {
                request_id: result.request_id,
                source_peer_id: input.source_peer_id,
                target_peer_id: PeerId::new(result.target_peer_id),
                path: RelayDataPlanePath::Relay,
                delivered: result.delivered,
                visited_hops: result.visited_hops.into_iter().map(PeerId::new).collect(),
                queued: false,
                payload: result.payload,
            }
        }
        OverlayRuntimeSelectedPath::MultiHopRelay => {
            let hop_ids = decision
                .multi_hop_candidates
                .first()
                .cloned()
                .unwrap_or_else(|| decision.relay_candidates.clone())
                .into_iter()
                .map(|peer| peer.0)
                .collect::<Vec<_>>();
            let ttl = hop_ids.len().saturating_add(1).min(u8::MAX as usize) as u8;
            let frame = MultiHopRelayFrame::new(
                input.request_id.clone(),
                input.source_peer_id.0.clone(),
                decision.target_peer_id.0.clone(),
                hop_ids,
                Vec::new(),
                ttl,
                input.payload,
            );
            let result = relay_server.forward_multihop(frame);
            RelayDataPlaneResult {
                request_id: result.request_id,
                source_peer_id: input.source_peer_id,
                target_peer_id: PeerId::new(result.target_peer_id),
                path: RelayDataPlanePath::MultiHop,
                delivered: result.delivered,
                visited_hops: result.visited_hops.into_iter().map(PeerId::new).collect(),
                queued: false,
                payload: result.payload,
            }
        }
        OverlayRuntimeSelectedPath::QueueFallback => RelayDataPlaneResult {
            request_id: input.request_id,
            source_peer_id: input.source_peer_id,
            target_peer_id: decision.target_peer_id.clone(),
            path: RelayDataPlanePath::QueueFallback,
            delivered: false,
            visited_hops: Vec::new(),
            queued: true,
            payload: input.payload,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{forward_novorudp_data_plane_v0, RelayDataPlaneInput, RelayDataPlanePath};
    use crate::control_plane::{
        CapabilityAdvertisement, ControlPlaneRegistry, Libp2pControlPlaneConfig, PeerId,
    };
    use crate::overlay::{AntiCensorshipProfile, OverlayHop, OverlayTransportProfile, RouteSet};
    use crate::overlay_runtime::{decide_overlay_runtime_route_v0, OverlayRuntimeSelectedPath};
    use crate::relay::RelayServer;

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
    fn direct_decision_delivers_opaque_novorudp_payload_without_relay_hops() {
        let mut registry = registry();
        let peer_id = PeerId::new("peer-target");
        register_native_peer(&mut registry, &peer_id);
        registry.register_route_set(RouteSet::direct(peer_id.clone()));
        let decision = decide_overlay_runtime_route_v0(&registry, &peer_id);
        assert_eq!(
            decision.selected_path,
            OverlayRuntimeSelectedPath::DirectNovoRudp
        );

        let relay_server = RelayServer::new("relay-root");
        let result = forward_novorudp_data_plane_v0(
            &relay_server,
            &decision,
            RelayDataPlaneInput {
                request_id: "req-direct".into(),
                source_peer_id: PeerId::new("peer-source"),
                payload: b"novorudp-frame".to_vec(),
            },
        );

        assert!(result.delivered);
        assert_eq!(result.path, RelayDataPlanePath::Direct);
        assert!(result.visited_hops.is_empty());
        assert_eq!(result.payload, b"novorudp-frame".to_vec());
    }

    #[test]
    fn relay_decision_forwards_opaque_novorudp_payload_through_relay() {
        let mut registry = registry();
        let peer_id = PeerId::new("peer-target");
        register_native_peer(&mut registry, &peer_id);
        registry.register_route_set(RouteSet {
            target_peer_id: peer_id.clone(),
            hops: vec![OverlayHop {
                peer_id: PeerId::new("peer-relay"),
                transport: OverlayTransportProfile::RelayNovoRudp,
                route_token: None,
            }],
            content_address_hint: None,
        });
        let decision = decide_overlay_runtime_route_v0(&registry, &peer_id);
        assert_eq!(
            decision.selected_path,
            OverlayRuntimeSelectedPath::RelayNovoRudp
        );

        let relay_server = RelayServer::new("relay-root");
        let result = forward_novorudp_data_plane_v0(
            &relay_server,
            &decision,
            RelayDataPlaneInput {
                request_id: "req-relay".into(),
                source_peer_id: PeerId::new("peer-source"),
                payload: b"novorudp-frame".to_vec(),
            },
        );

        assert!(result.delivered);
        assert_eq!(result.path, RelayDataPlanePath::Relay);
        assert_eq!(result.visited_hops, vec![PeerId::new("peer-relay")]);
        assert_eq!(result.target_peer_id, peer_id);
        assert_eq!(result.payload, b"novorudp-frame".to_vec());
    }

    #[test]
    fn multi_hop_decision_preserves_hop_order_and_payload() {
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

        let relay_server = RelayServer::new("relay-root");
        let result = forward_novorudp_data_plane_v0(
            &relay_server,
            &decision,
            RelayDataPlaneInput {
                request_id: "req-hop".into(),
                source_peer_id: PeerId::new("peer-source"),
                payload: b"novorudp-frame".to_vec(),
            },
        );

        assert!(result.delivered);
        assert_eq!(result.path, RelayDataPlanePath::MultiHop);
        assert_eq!(
            result.visited_hops,
            vec![PeerId::new("peer-relay-a"), PeerId::new("peer-relay-b")]
        );
        assert_eq!(result.payload, b"novorudp-frame".to_vec());
    }

    #[test]
    fn queue_fallback_does_not_deliver_payload() {
        let registry = registry();
        let peer_id = PeerId::new("peer-missing");
        let decision = decide_overlay_runtime_route_v0(&registry, &peer_id);
        assert_eq!(
            decision.selected_path,
            OverlayRuntimeSelectedPath::QueueFallback
        );

        let relay_server = RelayServer::new("relay-root");
        let result = forward_novorudp_data_plane_v0(
            &relay_server,
            &decision,
            RelayDataPlaneInput {
                request_id: "req-queue".into(),
                source_peer_id: PeerId::new("peer-source"),
                payload: b"novorudp-frame".to_vec(),
            },
        );

        assert!(!result.delivered);
        assert!(result.queued);
        assert_eq!(result.path, RelayDataPlanePath::QueueFallback);
        assert_eq!(result.payload, b"novorudp-frame".to_vec());
    }
}
