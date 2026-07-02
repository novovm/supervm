use anyhow::{Context, Result};
use novovm_network::control_plane::{
    CapabilityAdvertisement, ControlPlaneRegistry, Libp2pControlPlaneConfig, PeerId,
};
use novovm_network::novorudp::NovoRudpTransportFrameKindV0;
use novovm_network::overlay::{
    AntiCensorshipProfile, OverlayHop, OverlayTransportProfile, RouteSet,
};
use novovm_network::overlay_runtime::decide_overlay_runtime_route_v0;
use novovm_network::relay::{
    run_novorudp_overlay_relay_udp_loopback_smoke_v0, NovoRudpRelayUdpLoopbackInput,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::fs;
use std::net::UdpSocket;
use std::path::Path;
use std::time::{Duration, Instant};

fn main() -> Result<()> {
    let mode = env_string("NOVOVM_OVERLAY_GATE_MODE").unwrap_or_else(|| "loopback".into());
    match mode.as_str() {
        "loopback" => run_loopback_gate(),
        "receiver" => run_receiver_gate(),
        "relay" => run_relay_gate(),
        "sender" => run_sender_gate(),
        other => anyhow::bail!("unsupported NOVOVM_OVERLAY_GATE_MODE: {other}"),
    }
}

fn run_loopback_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/native-pipeline/network-overlay-gate/report.json".into());
    let route = env_string("NOVOVM_OVERLAY_GATE_ROUTE").unwrap_or_else(|| "direct".into());
    let request_id =
        env_string("NOVOVM_OVERLAY_GATE_REQUEST_ID").unwrap_or_else(|| "overlay-gate".into());
    let target_peer_id = PeerId::new(
        env_string("NOVOVM_OVERLAY_GATE_TARGET_PEER_ID").unwrap_or_else(|| "peer-target".into()),
    );
    let local_peer_id = PeerId::new(
        env_string("NOVOVM_OVERLAY_GATE_LOCAL_PEER_ID").unwrap_or_else(|| "peer-local".into()),
    );

    let mut registry = ControlPlaneRegistry::new(
        Libp2pControlPlaneConfig::production_minimum(local_peer_id),
        AntiCensorshipProfile::default(),
    );

    if route != "queue" {
        registry.register_advertisement(
            CapabilityAdvertisement {
                peer_id: target_peer_id.clone(),
                protocols: vec!["novorudp/0".into(), "native-pipeline/1".into()],
                no_ip_identity_routing: true,
            },
            100,
        );
        registry.register_route_set(route_set_for(&route, target_peer_id.clone())?);
    }

    let decision = decide_overlay_runtime_route_v0(&registry, &target_peer_id);
    let input = NovoRudpRelayUdpLoopbackInput {
        request_id: request_id.clone(),
        // The path is intentionally overwritten by the overlay decision bridge.
        path: novovm_network::relay::RelayUdpLoopbackPath::QueueFallback,
        kind: NovoRudpTransportFrameKindV0::Data,
        session_id: [11u8; 16],
        stream_id: 1,
        object_id: 2,
        sequence: 3,
        ack_epoch: 4,
        payload: b"novovm-overlay-gate-opaque-novorudp-frame".to_vec(),
    };
    let smoke = run_novorudp_overlay_relay_udp_loopback_smoke_v0(&decision, input)
        .map_err(|error| anyhow::anyhow!("run overlay relay UDP loopback smoke: {error}"))?;

    let accepted = match route.as_str() {
        "queue" => smoke.queued && !smoke.delivered,
        _ => smoke.delivered && smoke.frame_decode_ok && smoke.payload_match,
    };
    let report = json!({
        "accepted": accepted,
        "scope": "network_overlay_gate_v0",
        "boundary": {
            "network_only": true,
            "apfl_interpreted": false,
            "aoem_called": false,
            "ledger_semantics": false,
            "novorudp_wire_changed": false
        },
        "requested_route": route,
        "request_id": request_id,
        "target_peer_id": target_peer_id,
        "overlay_decision": decision,
        "loopback_report": smoke,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);

    if accepted {
        Ok(())
    } else {
        anyhow::bail!("network overlay gate rejected requested route")
    }
}

fn run_receiver_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/receiver.json".into());
    let bind_addr =
        env_string("NOVOVM_OVERLAY_GATE_BIND_ADDR").unwrap_or_else(|| "127.0.0.1:0".into());
    let timeout_ms = env_u64("NOVOVM_OVERLAY_GATE_TIMEOUT_MS", 5000);
    let socket =
        UdpSocket::bind(&bind_addr).with_context(|| format!("bind receiver: {bind_addr}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .context("set receiver read timeout")?;
    let local_addr = socket.local_addr().context("receiver local addr")?;
    let start = Instant::now();
    let mut buf = vec![0u8; 65535];
    let (received_bytes, source_addr) =
        socket.recv_from(&mut buf).context("receiver recv frame")?;
    let elapsed_ms = start.elapsed().as_millis() as u64;
    let decoded =
        novovm_network::novorudp::NovoRudpTransportFrameV0::decode(&buf[..received_bytes]);
    let (frame_decode_ok, frame_decode_error, decoded_kind, decoded_sequence, payload_bytes) =
        match decoded {
            Ok(frame) => (
                true,
                None,
                Some(frame.kind),
                Some(frame.sequence),
                frame.payload.len(),
            ),
            Err(error) => (false, Some(error.to_string()), None, None, 0),
        };
    let accepted = frame_decode_ok;
    let report = json!({
        "accepted": accepted,
        "scope": "network_overlay_gate_receiver_v0",
        "boundary": network_boundary_json(),
        "bind_addr_requested": bind_addr,
        "bind_addr_effective": local_addr.to_string(),
        "source_addr": source_addr.to_string(),
        "received_bytes": received_bytes,
        "elapsed_ms": elapsed_ms,
        "frame_decode_ok": frame_decode_ok,
        "frame_decode_error": frame_decode_error,
        "decoded_kind": decoded_kind,
        "decoded_sequence": decoded_sequence,
        "payload_bytes": payload_bytes,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("receiver failed to decode NOVORUDP frame")
    }
}

fn run_relay_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/relay.json".into());
    let bind_addr =
        env_string("NOVOVM_OVERLAY_GATE_BIND_ADDR").unwrap_or_else(|| "127.0.0.1:0".into());
    let relay_id = env_string("NOVOVM_OVERLAY_GATE_RELAY_ID").unwrap_or_else(|| "relay-a".into());
    let timeout_ms = env_u64("NOVOVM_OVERLAY_GATE_TIMEOUT_MS", 5000);
    let socket = UdpSocket::bind(&bind_addr).with_context(|| format!("bind relay: {bind_addr}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .context("set relay read timeout")?;
    let local_addr = socket.local_addr().context("relay local addr")?;
    let start = Instant::now();
    let mut buf = vec![0u8; 65535];
    let (received_bytes, source_addr) =
        socket.recv_from(&mut buf).context("relay recv envelope")?;
    let mut envelope: OverlayGateRelayEnvelopeV0 =
        serde_json::from_slice(&buf[..received_bytes]).context("decode relay envelope")?;
    if envelope.ttl == 0 {
        anyhow::bail!("relay ttl exhausted");
    }
    envelope.ttl = envelope.ttl.saturating_sub(1);
    let (forward_to, forward_payload, delivered_to_target) =
        if envelope.remaining_hop_addrs.is_empty() {
            (envelope.target_addr.clone(), envelope.payload.clone(), true)
        } else {
            let next_hop = envelope.remaining_hop_addrs.remove(0);
            (next_hop, serde_json::to_vec(&envelope)?, false)
        };
    let forwarded_bytes = socket
        .send_to(&forward_payload, &forward_to)
        .with_context(|| format!("relay forward to {forward_to}"))?;
    let elapsed_ms = start.elapsed().as_millis() as u64;
    let report = json!({
        "accepted": true,
        "scope": "network_overlay_gate_relay_v0",
        "boundary": network_boundary_json(),
        "relay_id": relay_id,
        "bind_addr_requested": bind_addr,
        "bind_addr_effective": local_addr.to_string(),
        "source_addr": source_addr.to_string(),
        "received_bytes": received_bytes,
        "forwarded_bytes": forwarded_bytes,
        "forwarded_to": forward_to,
        "delivered_to_target": delivered_to_target,
        "elapsed_ms": elapsed_ms,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn run_sender_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/sender.json".into());
    let route = env_string("NOVOVM_OVERLAY_GATE_ROUTE").unwrap_or_else(|| "direct".into());
    let target_addr =
        env_string("NOVOVM_OVERLAY_GATE_TARGET_ADDR").unwrap_or_else(|| "127.0.0.1:39011".into());
    let relay_addr = env_string("NOVOVM_OVERLAY_GATE_RELAY_ADDR");
    let next_hop_addr = env_string("NOVOVM_OVERLAY_GATE_NEXT_HOP_ADDR");
    let bind_addr =
        env_string("NOVOVM_OVERLAY_GATE_BIND_ADDR").unwrap_or_else(|| "127.0.0.1:0".into());
    let request_id =
        env_string("NOVOVM_OVERLAY_GATE_REQUEST_ID").unwrap_or_else(|| "overlay-gate".into());

    let (decision, target_peer_id) = decision_for_route(&route)?;
    let frame = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        [13u8; 16],
        10,
        20,
        30,
        40,
        b"novovm-overlay-three-process-opaque-frame".to_vec(),
    );
    let encoded = frame.encode();

    let socket =
        UdpSocket::bind(&bind_addr).with_context(|| format!("bind sender: {bind_addr}"))?;
    let local_addr = socket.local_addr().context("sender local addr")?;
    let start = Instant::now();
    let (sent, sent_to, queued) = match route.as_str() {
        "queue" => (0, None, true),
        "direct" => (
            socket
                .send_to(&encoded, &target_addr)
                .with_context(|| format!("direct send to {target_addr}"))?,
            Some(target_addr.clone()),
            false,
        ),
        "relay" => {
            let relay_addr = relay_addr.context("NOVOVM_OVERLAY_GATE_RELAY_ADDR required")?;
            let envelope = OverlayGateRelayEnvelopeV0 {
                request_id: request_id.clone(),
                source_peer_id: "peer-source".into(),
                target_peer_id: target_peer_id.0.clone(),
                target_addr: target_addr.clone(),
                remaining_hop_addrs: Vec::new(),
                ttl: 4,
                payload: encoded.clone(),
            };
            let payload = serde_json::to_vec(&envelope)?;
            (
                socket
                    .send_to(&payload, &relay_addr)
                    .with_context(|| format!("relay send to {relay_addr}"))?,
                Some(relay_addr),
                false,
            )
        }
        "multihop" | "multi-hop" => {
            let relay_addr = relay_addr.context("NOVOVM_OVERLAY_GATE_RELAY_ADDR required")?;
            let next_hop_addr =
                next_hop_addr.context("NOVOVM_OVERLAY_GATE_NEXT_HOP_ADDR required")?;
            let envelope = OverlayGateRelayEnvelopeV0 {
                request_id: request_id.clone(),
                source_peer_id: "peer-source".into(),
                target_peer_id: target_peer_id.0.clone(),
                target_addr: target_addr.clone(),
                remaining_hop_addrs: vec![next_hop_addr],
                ttl: 4,
                payload: encoded.clone(),
            };
            let payload = serde_json::to_vec(&envelope)?;
            (
                socket
                    .send_to(&payload, &relay_addr)
                    .with_context(|| format!("multi-hop send to {relay_addr}"))?,
                Some(relay_addr),
                false,
            )
        }
        other => anyhow::bail!("unsupported sender route: {other}"),
    };
    let elapsed_ms = start.elapsed().as_millis() as u64;
    let accepted = queued || sent > 0;
    let report = json!({
        "accepted": accepted,
        "scope": "network_overlay_gate_sender_v0",
        "boundary": network_boundary_json(),
        "requested_route": route,
        "request_id": request_id,
        "bind_addr_requested": bind_addr,
        "bind_addr_effective": local_addr.to_string(),
        "target_addr": target_addr,
        "sent_to": sent_to,
        "queued": queued,
        "sent_bytes": sent,
        "encoded_frame_bytes": encoded.len(),
        "elapsed_ms": elapsed_ms,
        "overlay_decision": decision,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("sender did not send or queue")
    }
}

fn route_set_for(route: &str, target_peer_id: PeerId) -> Result<RouteSet> {
    match route {
        "direct" => Ok(RouteSet::direct(target_peer_id)),
        "relay" => Ok(RouteSet {
            target_peer_id,
            hops: vec![OverlayHop {
                peer_id: PeerId::new("peer-relay"),
                transport: OverlayTransportProfile::RelayNovoRudp,
                route_token: None,
            }],
            content_address_hint: None,
        }),
        "multihop" | "multi-hop" => Ok(RouteSet {
            target_peer_id,
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
            content_address_hint: Some("cid-overlay-gate".into()),
        }),
        other => anyhow::bail!("unsupported NOVOVM_OVERLAY_GATE_ROUTE: {other}"),
    }
}

fn decision_for_route(
    route: &str,
) -> Result<(
    novovm_network::overlay_runtime::OverlayRuntimeDecision,
    PeerId,
)> {
    let target_peer_id = PeerId::new("peer-target");
    let mut registry = ControlPlaneRegistry::new(
        Libp2pControlPlaneConfig::production_minimum(PeerId::new("peer-local")),
        AntiCensorshipProfile::default(),
    );
    if route != "queue" {
        registry.register_advertisement(
            CapabilityAdvertisement {
                peer_id: target_peer_id.clone(),
                protocols: vec!["novorudp/0".into(), "native-pipeline/1".into()],
                no_ip_identity_routing: true,
            },
            100,
        );
        registry.register_route_set(route_set_for(route, target_peer_id.clone())?);
    }
    Ok((
        decide_overlay_runtime_route_v0(&registry, &target_peer_id),
        target_peer_id,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct OverlayGateRelayEnvelopeV0 {
    request_id: String,
    source_peer_id: String,
    target_peer_id: String,
    target_addr: String,
    remaining_hop_addrs: Vec<String>,
    ttl: u8,
    payload: Vec<u8>,
}

fn network_boundary_json() -> serde_json::Value {
    json!({
        "network_only": true,
        "apfl_interpreted": false,
        "aoem_called": false,
        "ledger_semantics": false,
        "novorudp_wire_changed": false
    })
}

fn write_json_report(path: &str, value: &serde_json::Value) -> Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).with_context(|| format!("create report dir: {parent:?}"))?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("write report: {path}"))
}

fn env_string(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(default)
}
