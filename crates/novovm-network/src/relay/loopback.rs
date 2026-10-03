use serde::{Deserialize, Serialize};
use std::net::{SocketAddr, UdpSocket};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::novorudp::{NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0};
use crate::overlay_runtime::{OverlayRuntimeDecision, OverlayRuntimeSelectedPath};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelayUdpLoopbackPath {
    Direct,
    Relay,
    MultiHop,
    QueueFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpRelayUdpLoopbackInput {
    pub request_id: String,
    pub path: RelayUdpLoopbackPath,
    pub kind: NovoRudpTransportFrameKindV0,
    pub session_id: [u8; 16],
    pub stream_id: u64,
    pub object_id: u64,
    pub sequence: u64,
    pub ack_epoch: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayUdpLoopbackHopReport {
    pub relay_id: String,
    pub bind_addr: String,
    pub received_bytes: usize,
    pub forwarded_bytes: usize,
    pub forwarded_to: Option<String>,
    pub visited: bool,
    pub delivered_to_target: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpRelayUdpLoopbackReport {
    pub request_id: String,
    pub path: RelayUdpLoopbackPath,
    pub delivered: bool,
    pub queued: bool,
    pub encoded_frame_bytes: usize,
    pub target_received_bytes: usize,
    pub queued_payload_bytes: usize,
    pub relay_hop_count: usize,
    pub relay_hops: Vec<RelayUdpLoopbackHopReport>,
    pub frame_decode_ok: bool,
    pub frame_decode_error: Option<String>,
    pub decoded_kind: Option<NovoRudpTransportFrameKindV0>,
    pub decoded_sequence: Option<u64>,
    pub payload_match: bool,
    pub queued_payload_preserved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RelayUdpLoopbackEnvelopeV0 {
    request_id: String,
    source_peer_id: String,
    target_peer_id: String,
    target_addr: String,
    remaining_hop_addrs: Vec<String>,
    ttl: u8,
    payload: Vec<u8>,
}

pub fn run_novorudp_relay_udp_loopback_smoke_v0(
    input: NovoRudpRelayUdpLoopbackInput,
) -> Result<NovoRudpRelayUdpLoopbackReport, String> {
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

    if input.path == RelayUdpLoopbackPath::QueueFallback {
        return Ok(NovoRudpRelayUdpLoopbackReport {
            request_id: input.request_id,
            path: input.path,
            delivered: false,
            queued: true,
            encoded_frame_bytes,
            target_received_bytes: 0,
            queued_payload_bytes: encoded.len(),
            relay_hop_count: 0,
            relay_hops: Vec::new(),
            frame_decode_ok: false,
            frame_decode_error: None,
            decoded_kind: None,
            decoded_sequence: None,
            payload_match: false,
            queued_payload_preserved: encoded.len() == encoded_frame_bytes,
        });
    }

    let target = UdpSocket::bind("127.0.0.1:0")
        .map_err(|error| format!("bind loopback target failed: {error}"))?;
    target
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("set target read timeout failed: {error}"))?;
    let target_addr = target
        .local_addr()
        .map_err(|error| format!("read target addr failed: {error}"))?;

    let mut relay_handles = Vec::new();
    let first_destination = match input.path {
        RelayUdpLoopbackPath::Direct => target_addr,
        RelayUdpLoopbackPath::Relay => {
            let (relay_addr, handle) = spawn_loopback_relay_once("relay-a")?;
            relay_handles.push(handle);
            relay_addr
        }
        RelayUdpLoopbackPath::MultiHop => {
            let (_relay_b_addr, relay_b) = spawn_loopback_relay_once("relay-b")?;
            let (relay_a_addr, relay_a) = spawn_loopback_relay_once("relay-a")?;
            relay_handles.push(relay_a);
            relay_handles.push(relay_b);
            relay_a_addr
        }
        RelayUdpLoopbackPath::QueueFallback => unreachable!("queue handled above"),
    };

    let sender = UdpSocket::bind("127.0.0.1:0")
        .map_err(|error| format!("bind loopback sender failed: {error}"))?;
    match input.path {
        RelayUdpLoopbackPath::Direct => {
            sender
                .send_to(&encoded, first_destination)
                .map_err(|error| format!("direct send failed: {error}"))?;
        }
        RelayUdpLoopbackPath::Relay | RelayUdpLoopbackPath::MultiHop => {
            let remaining_hop_addrs = if input.path == RelayUdpLoopbackPath::MultiHop {
                relay_handles
                    .get(1)
                    .map(|handle| handle.bind_addr.clone())
                    .into_iter()
                    .collect()
            } else {
                Vec::new()
            };
            let envelope = RelayUdpLoopbackEnvelopeV0 {
                request_id: input.request_id.clone(),
                source_peer_id: "peer-source".into(),
                target_peer_id: "peer-target".into(),
                target_addr: target_addr.to_string(),
                remaining_hop_addrs,
                ttl: 4,
                payload: encoded.clone(),
            };
            let envelope_bytes = serde_json::to_vec(&envelope)
                .map_err(|error| format!("encode relay envelope failed: {error}"))?;
            sender
                .send_to(&envelope_bytes, first_destination)
                .map_err(|error| format!("relay send failed: {error}"))?;
        }
        RelayUdpLoopbackPath::QueueFallback => unreachable!("queue handled above"),
    }

    let mut target_buf = vec![0u8; 65535];
    let (target_received_bytes, _) = target
        .recv_from(&mut target_buf)
        .map_err(|error| format!("target recv failed: {error}"))?;
    let delivered_bytes = target_buf[..target_received_bytes].to_vec();

    let mut relay_hops = Vec::new();
    for handle in relay_handles {
        let relay_id = handle.relay_id;
        let bind_addr = handle.bind_addr;
        relay_hops.push(
            handle
                .join
                .join()
                .unwrap_or_else(|_| RelayUdpLoopbackHopReport {
                    relay_id,
                    bind_addr,
                    received_bytes: 0,
                    forwarded_bytes: 0,
                    forwarded_to: None,
                    visited: false,
                    delivered_to_target: false,
                    error: Some("relay thread panicked".into()),
                }),
        );
    }

    let relay_hop_count = relay_hops.iter().filter(|hop| hop.visited).count();
    let decoded = NovoRudpTransportFrameV0::decode(&delivered_bytes);
    match decoded {
        Ok(decoded) => Ok(NovoRudpRelayUdpLoopbackReport {
            request_id: input.request_id,
            path: input.path,
            delivered: true,
            queued: false,
            encoded_frame_bytes,
            target_received_bytes,
            queued_payload_bytes: 0,
            relay_hop_count,
            relay_hops,
            frame_decode_ok: true,
            frame_decode_error: None,
            decoded_kind: Some(decoded.kind),
            decoded_sequence: Some(decoded.sequence),
            payload_match: decoded.payload == input.payload,
            queued_payload_preserved: false,
        }),
        Err(error) => Ok(NovoRudpRelayUdpLoopbackReport {
            request_id: input.request_id,
            path: input.path,
            delivered: true,
            queued: false,
            encoded_frame_bytes,
            target_received_bytes,
            queued_payload_bytes: 0,
            relay_hop_count,
            relay_hops,
            frame_decode_ok: false,
            frame_decode_error: Some(error.to_string()),
            decoded_kind: None,
            decoded_sequence: None,
            payload_match: false,
            queued_payload_preserved: false,
        }),
    }
}

pub fn relay_udp_loopback_path_from_overlay_decision_v0(
    decision: &OverlayRuntimeDecision,
) -> RelayUdpLoopbackPath {
    match decision.selected_path {
        OverlayRuntimeSelectedPath::DirectNovoRudp => RelayUdpLoopbackPath::Direct,
        OverlayRuntimeSelectedPath::RelayNovoRudp => RelayUdpLoopbackPath::Relay,
        OverlayRuntimeSelectedPath::MultiHopRelay => RelayUdpLoopbackPath::MultiHop,
        OverlayRuntimeSelectedPath::QueueFallback => RelayUdpLoopbackPath::QueueFallback,
    }
}

pub fn run_novorudp_overlay_relay_udp_loopback_smoke_v0(
    decision: &OverlayRuntimeDecision,
    mut input: NovoRudpRelayUdpLoopbackInput,
) -> Result<NovoRudpRelayUdpLoopbackReport, String> {
    input.path = relay_udp_loopback_path_from_overlay_decision_v0(decision);
    run_novorudp_relay_udp_loopback_smoke_v0(input)
}

struct RelayThreadHandle {
    relay_id: String,
    bind_addr: String,
    join: JoinHandle<RelayUdpLoopbackHopReport>,
}

fn spawn_loopback_relay_once(relay_id: &str) -> Result<(SocketAddr, RelayThreadHandle), String> {
    let socket = UdpSocket::bind("127.0.0.1:0")
        .map_err(|error| format!("bind loopback relay failed: {error}"))?;
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("set relay read timeout failed: {error}"))?;
    let bind_addr = socket
        .local_addr()
        .map_err(|error| format!("read relay addr failed: {error}"))?;
    let relay_id_string = relay_id.to_string();
    let bind_addr_string = bind_addr.to_string();
    let handle_relay_id = relay_id_string.clone();
    let handle_bind_addr = bind_addr_string.clone();

    let join = thread::spawn(move || {
        let mut buf = vec![0u8; 65535];
        let (received_bytes, _) = match socket.recv_from(&mut buf) {
            Ok(value) => value,
            Err(error) => {
                return RelayUdpLoopbackHopReport {
                    relay_id: relay_id_string,
                    bind_addr: bind_addr_string,
                    received_bytes: 0,
                    forwarded_bytes: 0,
                    forwarded_to: None,
                    visited: false,
                    delivered_to_target: false,
                    error: Some(format!("relay recv failed: {error}")),
                };
            }
        };

        let mut envelope: RelayUdpLoopbackEnvelopeV0 =
            match serde_json::from_slice(&buf[..received_bytes]) {
                Ok(envelope) => envelope,
                Err(error) => {
                    return RelayUdpLoopbackHopReport {
                        relay_id: relay_id_string,
                        bind_addr: bind_addr_string,
                        received_bytes,
                        forwarded_bytes: 0,
                        forwarded_to: None,
                        visited: true,
                        delivered_to_target: false,
                        error: Some(format!("decode relay envelope failed: {error}")),
                    };
                }
            };

        if envelope.ttl == 0 {
            return RelayUdpLoopbackHopReport {
                relay_id: relay_id_string,
                bind_addr: bind_addr_string,
                received_bytes,
                forwarded_bytes: 0,
                forwarded_to: None,
                visited: true,
                delivered_to_target: false,
                error: Some("relay ttl exhausted".into()),
            };
        }
        envelope.ttl = envelope.ttl.saturating_sub(1);

        let (forward_to, forward_payload, delivered_to_target) =
            if envelope.remaining_hop_addrs.is_empty() {
                (envelope.target_addr.clone(), envelope.payload.clone(), true)
            } else {
                let next_hop = envelope.remaining_hop_addrs.remove(0);
                match serde_json::to_vec(&envelope) {
                    Ok(bytes) => (next_hop, bytes, false),
                    Err(error) => {
                        return RelayUdpLoopbackHopReport {
                            relay_id: relay_id_string,
                            bind_addr: bind_addr_string,
                            received_bytes,
                            forwarded_bytes: 0,
                            forwarded_to: None,
                            visited: true,
                            delivered_to_target: false,
                            error: Some(format!("encode relay envelope failed: {error}")),
                        };
                    }
                }
            };

        match socket.send_to(&forward_payload, &forward_to) {
            Ok(forwarded_bytes) => RelayUdpLoopbackHopReport {
                relay_id: relay_id_string,
                bind_addr: bind_addr_string,
                received_bytes,
                forwarded_bytes,
                forwarded_to: Some(forward_to),
                visited: true,
                delivered_to_target,
                error: None,
            },
            Err(error) => RelayUdpLoopbackHopReport {
                relay_id: relay_id_string,
                bind_addr: bind_addr_string,
                received_bytes,
                forwarded_bytes: 0,
                forwarded_to: Some(forward_to),
                visited: true,
                delivered_to_target,
                error: Some(format!("relay forward failed: {error}")),
            },
        }
    });

    Ok((
        bind_addr,
        RelayThreadHandle {
            relay_id: handle_relay_id,
            bind_addr: handle_bind_addr,
            join,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        relay_udp_loopback_path_from_overlay_decision_v0,
        run_novorudp_overlay_relay_udp_loopback_smoke_v0, run_novorudp_relay_udp_loopback_smoke_v0,
        NovoRudpRelayUdpLoopbackInput, RelayUdpLoopbackPath,
    };
    use crate::control_plane::{
        CapabilityAdvertisement, ControlPlaneRegistry, Libp2pControlPlaneConfig, PeerId,
    };
    use crate::novorudp::NovoRudpTransportFrameKindV0;
    use crate::overlay::{AntiCensorshipProfile, OverlayHop, OverlayTransportProfile, RouteSet};
    use crate::overlay_runtime::decide_overlay_runtime_route_v0;

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

    fn input(path: RelayUdpLoopbackPath) -> NovoRudpRelayUdpLoopbackInput {
        NovoRudpRelayUdpLoopbackInput {
            request_id: format!("loopback-{path:?}"),
            path,
            kind: NovoRudpTransportFrameKindV0::Data,
            session_id: [9u8; 16],
            stream_id: 10,
            object_id: 20,
            sequence: 30,
            ack_epoch: 40,
            payload: b"opaque-network-frame-bytes-only".to_vec(),
        }
    }

    #[test]
    fn direct_loopback_delivers_real_novorudp_frame_bytes() {
        let report = run_novorudp_relay_udp_loopback_smoke_v0(input(RelayUdpLoopbackPath::Direct))
            .expect("direct loopback smoke");

        assert!(report.delivered);
        assert!(!report.queued);
        assert_eq!(report.relay_hop_count, 0);
        assert!(report.frame_decode_ok);
        assert_eq!(
            report.decoded_kind,
            Some(NovoRudpTransportFrameKindV0::Data)
        );
        assert_eq!(report.decoded_sequence, Some(30));
        assert!(report.payload_match);
    }

    #[test]
    fn relay_loopback_delivers_real_novorudp_frame_bytes() {
        let report = run_novorudp_relay_udp_loopback_smoke_v0(input(RelayUdpLoopbackPath::Relay))
            .expect("relay loopback smoke");

        assert!(report.delivered);
        assert_eq!(report.relay_hop_count, 1);
        assert_eq!(report.relay_hops.len(), 1);
        assert!(report.relay_hops[0].visited);
        assert!(report.relay_hops[0].delivered_to_target);
        assert!(report.frame_decode_ok);
        assert!(report.payload_match);
    }

    #[test]
    fn multihop_loopback_delivers_real_novorudp_frame_bytes_in_order() {
        let report =
            run_novorudp_relay_udp_loopback_smoke_v0(input(RelayUdpLoopbackPath::MultiHop))
                .expect("multi-hop loopback smoke");

        assert!(report.delivered);
        assert_eq!(report.relay_hop_count, 2);
        assert_eq!(report.relay_hops.len(), 2);
        assert_eq!(report.relay_hops[0].relay_id, "relay-a");
        assert_eq!(report.relay_hops[1].relay_id, "relay-b");
        assert!(report.relay_hops[0].visited);
        assert!(report.relay_hops[1].visited);
        assert!(!report.relay_hops[0].delivered_to_target);
        assert!(report.relay_hops[1].delivered_to_target);
        assert!(report.frame_decode_ok);
        assert_eq!(report.decoded_sequence, Some(30));
        assert!(report.payload_match);
    }

    #[test]
    fn queue_loopback_preserves_frame_without_socket_delivery() {
        let report =
            run_novorudp_relay_udp_loopback_smoke_v0(input(RelayUdpLoopbackPath::QueueFallback))
                .expect("queue loopback smoke");

        assert!(!report.delivered);
        assert!(report.queued);
        assert_eq!(report.relay_hop_count, 0);
        assert!(!report.frame_decode_ok);
        assert_eq!(report.queued_payload_bytes, report.encoded_frame_bytes);
        assert!(report.queued_payload_preserved);
    }

    #[test]
    fn overlay_decision_drives_direct_udp_loopback_path() {
        let mut registry = registry();
        let peer_id = PeerId::new("peer-target");
        register_native_peer(&mut registry, &peer_id);
        registry.register_route_set(RouteSet::direct(peer_id.clone()));
        let decision = decide_overlay_runtime_route_v0(&registry, &peer_id);

        assert_eq!(
            relay_udp_loopback_path_from_overlay_decision_v0(&decision),
            RelayUdpLoopbackPath::Direct
        );
        let report = run_novorudp_overlay_relay_udp_loopback_smoke_v0(
            &decision,
            input(RelayUdpLoopbackPath::QueueFallback),
        )
        .expect("overlay direct loopback smoke");
        assert_eq!(report.path, RelayUdpLoopbackPath::Direct);
        assert!(report.delivered);
        assert!(report.frame_decode_ok);
        assert!(report.payload_match);
    }

    #[test]
    fn overlay_decision_drives_relay_udp_loopback_path() {
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
            relay_udp_loopback_path_from_overlay_decision_v0(&decision),
            RelayUdpLoopbackPath::Relay
        );
        let report = run_novorudp_overlay_relay_udp_loopback_smoke_v0(
            &decision,
            input(RelayUdpLoopbackPath::Direct),
        )
        .expect("overlay relay loopback smoke");
        assert_eq!(report.path, RelayUdpLoopbackPath::Relay);
        assert!(report.delivered);
        assert_eq!(report.relay_hop_count, 1);
        assert!(report.frame_decode_ok);
    }

    #[test]
    fn overlay_decision_drives_multihop_udp_loopback_path() {
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
            relay_udp_loopback_path_from_overlay_decision_v0(&decision),
            RelayUdpLoopbackPath::MultiHop
        );
        let report = run_novorudp_overlay_relay_udp_loopback_smoke_v0(
            &decision,
            input(RelayUdpLoopbackPath::Direct),
        )
        .expect("overlay multi-hop loopback smoke");
        assert_eq!(report.path, RelayUdpLoopbackPath::MultiHop);
        assert!(report.delivered);
        assert_eq!(report.relay_hop_count, 2);
        assert!(report.frame_decode_ok);
    }

    #[test]
    fn overlay_decision_drives_queue_udp_loopback_path() {
        let registry = registry();
        let peer_id = PeerId::new("peer-missing");
        let decision = decide_overlay_runtime_route_v0(&registry, &peer_id);

        assert_eq!(
            relay_udp_loopback_path_from_overlay_decision_v0(&decision),
            RelayUdpLoopbackPath::QueueFallback
        );
        let report = run_novorudp_overlay_relay_udp_loopback_smoke_v0(
            &decision,
            input(RelayUdpLoopbackPath::Direct),
        )
        .expect("overlay queue loopback smoke");
        assert_eq!(report.path, RelayUdpLoopbackPath::QueueFallback);
        assert!(!report.delivered);
        assert!(report.queued);
        assert!(report.queued_payload_preserved);
    }
}
