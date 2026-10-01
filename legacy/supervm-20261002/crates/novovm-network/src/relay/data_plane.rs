use serde::{Deserialize, Serialize};

use crate::control_plane::PeerId;
use crate::novorudp::{NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpRelaySmokeInput {
    pub request_id: String,
    pub source_peer_id: PeerId,
    pub kind: NovoRudpTransportFrameKindV0,
    pub session_id: [u8; 16],
    pub stream_id: u64,
    pub object_id: u64,
    pub sequence: u64,
    pub ack_epoch: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpRelaySmokeReport {
    pub request_id: String,
    pub target_peer_id: PeerId,
    pub path: RelayDataPlanePath,
    pub delivered: bool,
    pub queued: bool,
    pub visited_hops: Vec<PeerId>,
    pub encoded_frame_bytes: usize,
    pub delivered_frame_decode_ok: bool,
    pub delivered_frame_decode_error: Option<String>,
    pub decoded_kind: Option<NovoRudpTransportFrameKindV0>,
    pub decoded_sequence: Option<u64>,
    pub decoded_payload_bytes: usize,
    pub payload_match: bool,
    pub queued_payload_preserved: bool,
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

pub fn run_novorudp_relay_data_plane_smoke_v0(
    relay_server: &RelayServer,
    decision: &OverlayRuntimeDecision,
    input: NovoRudpRelaySmokeInput,
) -> NovoRudpRelaySmokeReport {
    let frame = NovoRudpTransportFrameV0::new(
        input.kind,
        input.session_id,
        input.stream_id,
        input.object_id,
        input.sequence,
        input.ack_epoch,
        input.payload.clone(),
    );
    let encoded = frame.encode();
    let encoded_frame_bytes = encoded.len();
    let result = forward_novorudp_data_plane_v0(
        relay_server,
        decision,
        RelayDataPlaneInput {
            request_id: input.request_id,
            source_peer_id: input.source_peer_id,
            payload: encoded.clone(),
        },
    );

    let queued_payload_preserved = result.queued && result.payload == encoded;
    if !result.delivered {
        return NovoRudpRelaySmokeReport {
            request_id: result.request_id,
            target_peer_id: result.target_peer_id,
            path: result.path,
            delivered: result.delivered,
            queued: result.queued,
            visited_hops: result.visited_hops,
            encoded_frame_bytes,
            delivered_frame_decode_ok: false,
            delivered_frame_decode_error: None,
            decoded_kind: None,
            decoded_sequence: None,
            decoded_payload_bytes: 0,
            payload_match: false,
            queued_payload_preserved,
        };
    }

    match NovoRudpTransportFrameV0::decode(&result.payload) {
        Ok(decoded) => {
            let payload_match = decoded.payload == input.payload;
            let decoded_payload_bytes = decoded.payload.len();
            NovoRudpRelaySmokeReport {
                request_id: result.request_id,
                target_peer_id: result.target_peer_id,
                path: result.path,
                delivered: result.delivered,
                queued: result.queued,
                visited_hops: result.visited_hops,
                encoded_frame_bytes,
                delivered_frame_decode_ok: true,
                delivered_frame_decode_error: None,
                decoded_kind: Some(decoded.kind),
                decoded_sequence: Some(decoded.sequence),
                decoded_payload_bytes,
                payload_match,
                queued_payload_preserved,
            }
        }
        Err(error) => NovoRudpRelaySmokeReport {
            request_id: result.request_id,
            target_peer_id: result.target_peer_id,
            path: result.path,
            delivered: result.delivered,
            queued: result.queued,
            visited_hops: result.visited_hops,
            encoded_frame_bytes,
            delivered_frame_decode_ok: false,
            delivered_frame_decode_error: Some(error.to_string()),
            decoded_kind: None,
            decoded_sequence: None,
            decoded_payload_bytes: 0,
            payload_match: false,
            queued_payload_preserved,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        forward_novorudp_data_plane_v0, run_novorudp_relay_data_plane_smoke_v0,
        NovoRudpRelaySmokeInput, RelayDataPlaneInput, RelayDataPlanePath,
    };
    use crate::control_plane::{
        CapabilityAdvertisement, ControlPlaneRegistry, Libp2pControlPlaneConfig, PeerId,
    };
    use crate::novorudp::NovoRudpTransportFrameKindV0;
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

    fn novorudp_smoke_input(request_id: &str) -> NovoRudpRelaySmokeInput {
        NovoRudpRelaySmokeInput {
            request_id: request_id.into(),
            source_peer_id: PeerId::new("peer-source"),
            kind: NovoRudpTransportFrameKindV0::Data,
            session_id: [7u8; 16],
            stream_id: 11,
            object_id: 22,
            sequence: 33,
            ack_epoch: 44,
            payload: b"opaque-novorudp-data-frame-payload".to_vec(),
        }
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

    #[test]
    fn direct_smoke_carries_real_novorudp_frame_bytes() {
        let mut registry = registry();
        let peer_id = PeerId::new("peer-target");
        register_native_peer(&mut registry, &peer_id);
        registry.register_route_set(RouteSet::direct(peer_id.clone()));
        let decision = decide_overlay_runtime_route_v0(&registry, &peer_id);

        let report = run_novorudp_relay_data_plane_smoke_v0(
            &RelayServer::new("relay-root"),
            &decision,
            novorudp_smoke_input("smoke-direct"),
        );

        assert!(report.delivered);
        assert!(!report.queued);
        assert_eq!(report.path, RelayDataPlanePath::Direct);
        assert!(report.delivered_frame_decode_ok);
        assert_eq!(
            report.decoded_kind,
            Some(NovoRudpTransportFrameKindV0::Data)
        );
        assert_eq!(report.decoded_sequence, Some(33));
        assert!(report.payload_match);
        assert!(report.visited_hops.is_empty());
    }

    #[test]
    fn relay_smoke_carries_real_novorudp_frame_bytes_through_one_hop() {
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

        let report = run_novorudp_relay_data_plane_smoke_v0(
            &RelayServer::new("relay-root"),
            &decision,
            novorudp_smoke_input("smoke-relay"),
        );

        assert!(report.delivered);
        assert_eq!(report.path, RelayDataPlanePath::Relay);
        assert_eq!(report.visited_hops, vec![PeerId::new("peer-relay")]);
        assert!(report.delivered_frame_decode_ok);
        assert!(report.payload_match);
    }

    #[test]
    fn multihop_smoke_carries_real_novorudp_frame_bytes_through_ordered_hops() {
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

        let report = run_novorudp_relay_data_plane_smoke_v0(
            &RelayServer::new("relay-root"),
            &decision,
            novorudp_smoke_input("smoke-hop"),
        );

        assert!(report.delivered);
        assert_eq!(report.path, RelayDataPlanePath::MultiHop);
        assert_eq!(
            report.visited_hops,
            vec![PeerId::new("peer-relay-a"), PeerId::new("peer-relay-b")]
        );
        assert!(report.delivered_frame_decode_ok);
        assert_eq!(report.decoded_sequence, Some(33));
        assert!(report.payload_match);
    }

    #[test]
    fn queue_smoke_preserves_real_novorudp_frame_bytes_without_delivery() {
        let registry = registry();
        let peer_id = PeerId::new("peer-missing");
        let decision = decide_overlay_runtime_route_v0(&registry, &peer_id);

        let report = run_novorudp_relay_data_plane_smoke_v0(
            &RelayServer::new("relay-root"),
            &decision,
            novorudp_smoke_input("smoke-queue"),
        );

        assert!(!report.delivered);
        assert!(report.queued);
        assert_eq!(report.path, RelayDataPlanePath::QueueFallback);
        assert!(!report.delivered_frame_decode_ok);
        assert!(report.queued_payload_preserved);
        assert!(!report.payload_match);
    }
}
