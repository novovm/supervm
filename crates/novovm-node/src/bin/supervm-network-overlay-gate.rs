use anyhow::{Context, Result};
use novovm_network::control_plane::{
    CapabilityAdvertisement, ControlPlaneRegistry, Libp2pControlPlaneConfig, PeerId,
};
use novovm_network::novorudp::NovoRudpTransportFrameKindV0;
use novovm_network::overlay::{
    AntiCensorshipProfile, OverlayHop, OverlayTransportProfile, RouteSet,
};
use novovm_network::overlay_runtime::{
    decide_overlay_runtime_route_v0, decide_overlay_runtime_route_with_health_v0, OverlayHopHealth,
    OverlayRouteHealthSnapshot, OverlayRuntimeDecision,
};
use novovm_network::reachability::{
    decide_reachability_probe_v0, FloatingPortMode, ReachabilityProbeDecision,
    ReachabilityProbeInput, ReachabilityProbeStatus,
};
use novovm_network::relay::{
    run_novorudp_overlay_relay_udp_loopback_smoke_v0, NovoRudpRelayUdpLoopbackInput,
};
use novovm_network::routing::RoutingSource;
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
        "matrix" => run_matrix_gate(),
        "health-matrix" => run_health_matrix_gate(),
        other => anyhow::bail!("unsupported NOVOVM_OVERLAY_GATE_MODE: {other}"),
    }
}

fn run_loopback_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/native-pipeline/network-overlay-gate/report.json".into());
    let requested_route =
        env_string("NOVOVM_OVERLAY_GATE_ROUTE").unwrap_or_else(|| "direct".into());
    let request_id =
        env_string("NOVOVM_OVERLAY_GATE_REQUEST_ID").unwrap_or_else(|| "overlay-gate".into());
    let target_peer_id = PeerId::new(
        env_string("NOVOVM_OVERLAY_GATE_TARGET_PEER_ID").unwrap_or_else(|| "peer-target".into()),
    );
    let local_peer_id = PeerId::new(
        env_string("NOVOVM_OVERLAY_GATE_LOCAL_PEER_ID").unwrap_or_else(|| "peer-local".into()),
    );

    let route_plan = route_plan_for(&requested_route, &target_peer_id)?;
    let report = build_loopback_case_report(
        "network_overlay_gate_v0",
        &requested_route,
        route_plan,
        &request_id,
        target_peer_id,
        local_peer_id,
    )?;
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);

    if report["accepted"].as_bool().unwrap_or(false) {
        Ok(())
    } else {
        anyhow::bail!("network overlay gate rejected requested route")
    }
}

fn build_loopback_case_report(
    scope: &str,
    requested_route: &str,
    route_plan: OverlayGateRoutePlan,
    request_id: &str,
    target_peer_id: PeerId,
    local_peer_id: PeerId,
) -> Result<serde_json::Value> {
    let mut registry = ControlPlaneRegistry::new(
        Libp2pControlPlaneConfig::production_minimum(local_peer_id),
        AntiCensorshipProfile::default(),
    );

    if route_plan.effective_route != "queue" {
        registry.register_advertisement(
            CapabilityAdvertisement {
                peer_id: target_peer_id.clone(),
                protocols: vec!["novorudp/0".into(), "native-pipeline/1".into()],
                no_ip_identity_routing: true,
            },
            100,
        );
        registry.register_route_set(route_set_for(
            &route_plan.effective_route,
            target_peer_id.clone(),
        )?);
    }

    let decision = decide_overlay_runtime_route_v0(&registry, &target_peer_id);
    let input = NovoRudpRelayUdpLoopbackInput {
        request_id: request_id.to_string(),
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

    let accepted = match route_plan.effective_route.as_str() {
        "queue" => smoke.queued && !smoke.delivered,
        _ => smoke.delivered && smoke.frame_decode_ok && smoke.payload_match,
    };

    Ok(json!({
        "accepted": accepted,
        "scope": scope,
        "boundary": network_boundary_json(),
        "requested_route": requested_route,
        "effective_route": route_plan.effective_route,
        "route_plan_source": route_plan.route_plan_source,
        "runtime_probe_used": route_plan.runtime_probe_used,
        "auto_relay_hops": route_plan.auto_relay_hops,
        "reachability_probe_decision": route_plan.reachability_probe_decision,
        "request_id": request_id,
        "target_peer_id": target_peer_id,
        "overlay_decision": decision,
        "loopback_report": smoke,
    }))
}

fn run_receiver_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/receiver.json".into());
    let bind_addr =
        env_string("NOVOVM_OVERLAY_GATE_BIND_ADDR").unwrap_or_else(|| "127.0.0.1:0".into());
    let timeout_ms = env_u64("NOVOVM_OVERLAY_GATE_TIMEOUT_MS", 5000);
    let max_frames = env_u64("NOVOVM_OVERLAY_GATE_MAX_FRAMES", 1).max(1);
    let socket =
        UdpSocket::bind(&bind_addr).with_context(|| format!("bind receiver: {bind_addr}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .context("set receiver read timeout")?;
    let local_addr = socket.local_addr().context("receiver local addr")?;
    let start = Instant::now();
    let mut buf = vec![0u8; 65535];
    let mut probe_ack_sent = false;
    let mut probe_source_addr = None;
    let mut frames = Vec::new();
    let mut recv_error = None;
    while frames.len() < max_frames as usize {
        match socket.recv_from(&mut buf) {
            Ok((received_bytes, source_addr)) => {
                let frame = novovm_network::novorudp::NovoRudpTransportFrameV0::decode(
                    &buf[..received_bytes],
                );
                if let Ok(frame) = &frame {
                    if frame.kind == NovoRudpTransportFrameKindV0::Endpoint {
                        let ack = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
                            NovoRudpTransportFrameKindV0::Ack,
                            frame.session_id,
                            frame.stream_id,
                            frame.object_id,
                            frame.sequence,
                            frame.ack_epoch,
                            frame.payload.clone(),
                        );
                        socket
                            .send_to(&ack.encode(), source_addr)
                            .context("receiver send probe ack")?;
                        probe_ack_sent = true;
                        probe_source_addr = Some(source_addr.to_string());
                        continue;
                    }
                }
                let (
                    frame_decode_ok,
                    frame_decode_error,
                    decoded_kind,
                    decoded_sequence,
                    payload_bytes,
                ) = match frame {
                    Ok(frame) => (
                        true,
                        None,
                        Some(frame.kind),
                        Some(frame.sequence),
                        frame.payload.len(),
                    ),
                    Err(error) => (false, Some(error.to_string()), None, None, 0),
                };
                frames.push(json!({
                    "source_addr": source_addr.to_string(),
                    "received_bytes": received_bytes,
                    "frame_decode_ok": frame_decode_ok,
                    "frame_decode_error": frame_decode_error,
                    "decoded_kind": decoded_kind,
                    "decoded_sequence": decoded_sequence,
                    "payload_bytes": payload_bytes,
                }));
            }
            Err(error) => {
                recv_error = Some(error.to_string());
                break;
            }
        }
    }
    let elapsed_ms = start.elapsed().as_millis() as u64;
    let data_frames_received = frames.len() as u64;
    let accepted = data_frames_received == max_frames
        && frames
            .iter()
            .all(|frame| frame["frame_decode_ok"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "network_overlay_gate_receiver_v0",
        "boundary": network_boundary_json(),
        "bind_addr_requested": bind_addr,
        "bind_addr_effective": local_addr.to_string(),
        "max_frames": max_frames,
        "data_frames_received": data_frames_received,
        "probe_ack_sent": probe_ack_sent,
        "probe_source_addr": probe_source_addr,
        "recv_error": recv_error,
        "elapsed_ms": elapsed_ms,
        "frames": frames,
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
    let max_frames = env_u64("NOVOVM_OVERLAY_GATE_MAX_FRAMES", 1).max(1);
    let socket = UdpSocket::bind(&bind_addr).with_context(|| format!("bind relay: {bind_addr}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .context("set relay read timeout")?;
    let local_addr = socket.local_addr().context("relay local addr")?;
    let start = Instant::now();
    let mut buf = vec![0u8; 65535];
    let mut frames = Vec::new();
    let mut recv_error = None;
    while frames.len() < max_frames as usize {
        let (received_bytes, source_addr) = match socket.recv_from(&mut buf) {
            Ok(value) => value,
            Err(error) => {
                recv_error = Some(error.to_string());
                break;
            }
        };
        let mut envelope: OverlayGateRelayEnvelopeV0 =
            match serde_json::from_slice(&buf[..received_bytes]) {
                Ok(envelope) => envelope,
                Err(error) => {
                    frames.push(json!({
                        "accepted": false,
                        "source_addr": source_addr.to_string(),
                        "received_bytes": received_bytes,
                        "forwarded_bytes": 0,
                        "forwarded_to": null,
                        "delivered_to_target": false,
                        "error": format!("decode relay envelope failed: {error}"),
                    }));
                    continue;
                }
            };
        if envelope.ttl == 0 {
            frames.push(json!({
                "accepted": false,
                "request_id": envelope.request_id,
                "source_addr": source_addr.to_string(),
                "received_bytes": received_bytes,
                "forwarded_bytes": 0,
                "forwarded_to": null,
                "delivered_to_target": false,
                "error": "relay ttl exhausted",
            }));
            continue;
        }
        envelope.ttl = envelope.ttl.saturating_sub(1);
        let (forward_to, forward_payload, delivered_to_target) =
            if envelope.remaining_hop_addrs.is_empty() {
                (envelope.target_addr.clone(), envelope.payload.clone(), true)
            } else {
                let next_hop = envelope.remaining_hop_addrs.remove(0);
                (next_hop, serde_json::to_vec(&envelope)?, false)
            };
        match socket.send_to(&forward_payload, &forward_to) {
            Ok(forwarded_bytes) => frames.push(json!({
                "accepted": true,
                "request_id": envelope.request_id,
                "source_addr": source_addr.to_string(),
                "received_bytes": received_bytes,
                "forwarded_bytes": forwarded_bytes,
                "forwarded_to": forward_to,
                "delivered_to_target": delivered_to_target,
                "error": null,
            })),
            Err(error) => frames.push(json!({
                "accepted": false,
                "request_id": envelope.request_id,
                "source_addr": source_addr.to_string(),
                "received_bytes": received_bytes,
                "forwarded_bytes": 0,
                "forwarded_to": forward_to,
                "delivered_to_target": delivered_to_target,
                "error": format!("relay forward failed: {error}"),
            })),
        }
    }
    let elapsed_ms = start.elapsed().as_millis() as u64;
    let frames_received = frames.len() as u64;
    let accepted = frames_received == max_frames
        && frames
            .iter()
            .all(|frame| frame["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "network_overlay_gate_relay_v0",
        "boundary": network_boundary_json(),
        "relay_id": relay_id,
        "bind_addr_requested": bind_addr,
        "bind_addr_effective": local_addr.to_string(),
        "max_frames": max_frames,
        "frames_received": frames_received,
        "recv_error": recv_error,
        "frames": frames,
        "elapsed_ms": elapsed_ms,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("relay failed to forward all requested frames")
    }
}

fn run_sender_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/sender.json".into());
    let requested_route =
        env_string("NOVOVM_OVERLAY_GATE_ROUTE").unwrap_or_else(|| "direct".into());
    let target_addr =
        env_string("NOVOVM_OVERLAY_GATE_TARGET_ADDR").unwrap_or_else(|| "127.0.0.1:39011".into());
    let relay_target_addr =
        env_string("NOVOVM_OVERLAY_GATE_RELAY_TARGET_ADDR").unwrap_or_else(|| target_addr.clone());
    let relay_addr = env_string("NOVOVM_OVERLAY_GATE_RELAY_ADDR");
    let next_hop_addr = env_string("NOVOVM_OVERLAY_GATE_NEXT_HOP_ADDR");
    let bind_addr =
        env_string("NOVOVM_OVERLAY_GATE_BIND_ADDR").unwrap_or_else(|| "127.0.0.1:0".into());
    let request_id =
        env_string("NOVOVM_OVERLAY_GATE_REQUEST_ID").unwrap_or_else(|| "overlay-gate".into());
    let max_frames = env_u64("NOVOVM_OVERLAY_GATE_MAX_FRAMES", 1).max(1);

    let socket =
        UdpSocket::bind(&bind_addr).with_context(|| format!("bind sender: {bind_addr}"))?;
    let local_addr = socket.local_addr().context("sender local addr")?;
    let runtime_probe =
        if requested_route == "auto" && env_bool("NOVOVM_OVERLAY_GATE_RUNTIME_PROBE", false) {
            Some(run_sender_direct_probe_v0(
                &socket,
                &target_addr,
                &request_id,
            )?)
        } else {
            None
        };
    let route_plan = route_plan_for_with_runtime_probe(
        &requested_route,
        &PeerId::new("peer-target"),
        runtime_probe.as_ref(),
        Some(local_addr.to_string()),
    )?;
    let (decision, target_peer_id) = decision_for_route(&route_plan.effective_route)?;

    let start = Instant::now();
    let mut frames = Vec::new();
    let mut sent_bytes_total = 0usize;
    let mut queued_count = 0u64;
    for frame_index in 0..max_frames {
        let frame_request_id = if max_frames == 1 {
            request_id.clone()
        } else {
            format!("{request_id}-{frame_index}")
        };
        let frame = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            [13u8; 16],
            10,
            20,
            30 + frame_index,
            40,
            format!("novovm-overlay-three-process-opaque-frame-{frame_index}").into_bytes(),
        );
        let encoded = frame.encode();
        let (sent, sent_to, queued) = match route_plan.effective_route.as_str() {
            "queue" => (0, None, true),
            "direct" => (
                socket
                    .send_to(&encoded, &target_addr)
                    .with_context(|| format!("direct send to {target_addr}"))?,
                Some(target_addr.clone()),
                false,
            ),
            "relay" => {
                let relay_addr = relay_addr
                    .clone()
                    .context("NOVOVM_OVERLAY_GATE_RELAY_ADDR required")?;
                let envelope = OverlayGateRelayEnvelopeV0 {
                    request_id: frame_request_id.clone(),
                    source_peer_id: "peer-source".into(),
                    target_peer_id: target_peer_id.0.clone(),
                    target_addr: relay_target_addr.clone(),
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
                let relay_addr = relay_addr
                    .clone()
                    .context("NOVOVM_OVERLAY_GATE_RELAY_ADDR required")?;
                let next_hop_addr = next_hop_addr
                    .clone()
                    .context("NOVOVM_OVERLAY_GATE_NEXT_HOP_ADDR required")?;
                let envelope = OverlayGateRelayEnvelopeV0 {
                    request_id: frame_request_id.clone(),
                    source_peer_id: "peer-source".into(),
                    target_peer_id: target_peer_id.0.clone(),
                    target_addr: relay_target_addr.clone(),
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
        sent_bytes_total += sent;
        if queued {
            queued_count += 1;
        }
        frames.push(json!({
            "request_id": frame_request_id,
            "sequence": 30 + frame_index,
            "sent_to": sent_to,
            "queued": queued,
            "sent_bytes": sent,
            "encoded_frame_bytes": encoded.len(),
        }));
    }
    let elapsed_ms = start.elapsed().as_millis() as u64;
    let sent_frame_count = frames
        .iter()
        .filter(|frame| frame["sent_bytes"].as_u64().unwrap_or(0) > 0)
        .count() as u64;
    let accepted = queued_count == max_frames || sent_frame_count == max_frames;
    let report = json!({
        "accepted": accepted,
        "scope": "network_overlay_gate_sender_v0",
        "boundary": network_boundary_json(),
        "requested_route": requested_route,
        "effective_route": route_plan.effective_route,
        "route_plan_source": route_plan.route_plan_source,
        "runtime_probe_used": route_plan.runtime_probe_used,
        "auto_relay_hops": route_plan.auto_relay_hops,
        "reachability_probe_decision": route_plan.reachability_probe_decision,
        "runtime_probe_report": runtime_probe,
        "request_id": request_id,
        "bind_addr_requested": bind_addr,
        "bind_addr_effective": local_addr.to_string(),
        "target_addr": target_addr,
        "relay_target_addr": relay_target_addr,
        "max_frames": max_frames,
        "sent_frame_count": sent_frame_count,
        "queued_count": queued_count,
        "sent_bytes_total": sent_bytes_total,
        "frames": frames,
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

fn run_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/matrix.json".into());
    let target_peer_id = PeerId::new("peer-target");
    let local_peer_id = PeerId::new("peer-local");
    let cases = vec![
        (
            "manual-direct",
            "direct",
            OverlayGateRoutePlan::manual("direct"),
        ),
        (
            "manual-relay",
            "relay",
            OverlayGateRoutePlan::manual("relay"),
        ),
        (
            "manual-multihop",
            "multihop",
            OverlayGateRoutePlan::manual("multihop"),
        ),
        (
            "manual-queue",
            "queue",
            OverlayGateRoutePlan::manual("queue"),
        ),
        (
            "auto-direct",
            "auto",
            OverlayGateRoutePlan::simulated(
                "direct",
                1,
                simulated_probe_decision(
                    &target_peer_id,
                    ReachabilityProbeStatus::LanReachable,
                    true,
                    true,
                )?,
            ),
        ),
        (
            "auto-relay",
            "auto",
            OverlayGateRoutePlan::simulated(
                "relay",
                1,
                simulated_probe_decision(
                    &target_peer_id,
                    ReachabilityProbeStatus::RelayOnly,
                    true,
                    false,
                )?,
            ),
        ),
        (
            "auto-multihop",
            "auto",
            OverlayGateRoutePlan::simulated(
                "multihop",
                2,
                simulated_probe_decision(
                    &target_peer_id,
                    ReachabilityProbeStatus::RelayOnly,
                    true,
                    false,
                )?,
            ),
        ),
        (
            "auto-queue",
            "auto",
            OverlayGateRoutePlan::simulated(
                "queue",
                1,
                simulated_probe_decision(
                    &target_peer_id,
                    ReachabilityProbeStatus::Unreachable,
                    false,
                    false,
                )?,
            ),
        ),
    ];

    let mut reports = Vec::new();
    for (case_id, requested_route, route_plan) in cases {
        reports.push(build_loopback_case_report(
            "network_overlay_gate_matrix_case_v0",
            requested_route,
            route_plan,
            case_id,
            target_peer_id.clone(),
            local_peer_id.clone(),
        )?);
    }
    let accepted = reports
        .iter()
        .all(|report| report["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "network_overlay_gate_matrix_v0",
        "boundary": network_boundary_json(),
        "case_count": reports.len(),
        "cases": reports,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("network overlay matrix gate failed")
    }
}

fn run_health_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/health-matrix.json".into());
    let target_peer_id = PeerId::new("peer-target");
    let local_peer_id = PeerId::new("peer-local");
    let now_ms = env_u64("NOVOVM_OVERLAY_GATE_HEALTH_NOW_MS", 1_000);
    let cooldown_until_ms = env_u64(
        "NOVOVM_OVERLAY_GATE_HEALTH_COOLDOWN_UNTIL_MS",
        now_ms + 60_000,
    );
    let cases = vec![
        (
            "health-direct",
            OverlayRouteHealthSnapshot::new(now_ms, Vec::new()),
        ),
        (
            "health-direct-cooldown-multihop",
            OverlayRouteHealthSnapshot::new(
                now_ms,
                vec![OverlayHopHealth::cooling_down(
                    target_peer_id.clone(),
                    now_ms,
                    cooldown_until_ms,
                )],
            ),
        ),
        (
            "health-single-relay-fallback",
            OverlayRouteHealthSnapshot::new(
                now_ms,
                vec![
                    OverlayHopHealth::cooling_down(
                        target_peer_id.clone(),
                        now_ms,
                        cooldown_until_ms,
                    ),
                    OverlayHopHealth::cooling_down(
                        PeerId::new("peer-relay-a"),
                        now_ms,
                        cooldown_until_ms,
                    ),
                ],
            ),
        ),
        (
            "health-queue-fallback",
            OverlayRouteHealthSnapshot::new(
                now_ms,
                vec![
                    OverlayHopHealth::cooling_down(
                        target_peer_id.clone(),
                        now_ms,
                        cooldown_until_ms,
                    ),
                    OverlayHopHealth::cooling_down(
                        PeerId::new("peer-relay-a"),
                        now_ms,
                        cooldown_until_ms,
                    ),
                    OverlayHopHealth::cooling_down(
                        PeerId::new("peer-relay-b"),
                        now_ms,
                        cooldown_until_ms,
                    ),
                ],
            ),
        ),
    ];

    let mut reports = Vec::new();
    for (case_id, health) in cases {
        let mut registry = ControlPlaneRegistry::new(
            Libp2pControlPlaneConfig::production_minimum(local_peer_id.clone()),
            AntiCensorshipProfile::default(),
        );
        registry.register_advertisement(
            CapabilityAdvertisement {
                peer_id: target_peer_id.clone(),
                protocols: vec!["novorudp/0".into(), "native-pipeline/1".into()],
                no_ip_identity_routing: true,
            },
            100,
        );
        registry.register_route_set(health_matrix_route_set(target_peer_id.clone()));
        let decision =
            decide_overlay_runtime_route_with_health_v0(&registry, &target_peer_id, &health);
        reports.push(build_decision_loopback_report(
            "network_overlay_gate_health_matrix_case_v0",
            case_id,
            "health-aware",
            &decision,
            Some(health),
        )?);
    }

    let accepted = reports
        .iter()
        .all(|report| report["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "network_overlay_gate_health_matrix_v0",
        "boundary": network_boundary_json(),
        "case_count": reports.len(),
        "cases": reports,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("network overlay health matrix gate failed")
    }
}

fn build_decision_loopback_report(
    scope: &str,
    request_id: &str,
    route_plan_source: &str,
    decision: &OverlayRuntimeDecision,
    health: Option<OverlayRouteHealthSnapshot>,
) -> Result<serde_json::Value> {
    let input = NovoRudpRelayUdpLoopbackInput {
        request_id: request_id.to_string(),
        path: novovm_network::relay::RelayUdpLoopbackPath::QueueFallback,
        kind: NovoRudpTransportFrameKindV0::Data,
        session_id: [17u8; 16],
        stream_id: 1,
        object_id: 2,
        sequence: 3,
        ack_epoch: 4,
        payload: b"novovm-overlay-health-matrix-opaque-frame".to_vec(),
    };
    let smoke = run_novorudp_overlay_relay_udp_loopback_smoke_v0(decision, input)
        .map_err(|error| anyhow::anyhow!("run health matrix loopback smoke: {error}"))?;
    let accepted = match decision.selected_path {
        novovm_network::overlay_runtime::OverlayRuntimeSelectedPath::QueueFallback => {
            smoke.queued && !smoke.delivered
        }
        _ => smoke.delivered && smoke.frame_decode_ok && smoke.payload_match,
    };
    Ok(json!({
        "accepted": accepted,
        "scope": scope,
        "boundary": network_boundary_json(),
        "request_id": request_id,
        "route_plan_source": route_plan_source,
        "health_snapshot": health,
        "overlay_decision": decision,
        "loopback_report": smoke,
    }))
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

fn health_matrix_route_set(target_peer_id: PeerId) -> RouteSet {
    RouteSet {
        target_peer_id: target_peer_id.clone(),
        hops: vec![
            OverlayHop {
                peer_id: target_peer_id,
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
        content_address_hint: Some("cid-overlay-health-matrix".into()),
    }
}

#[derive(Debug, Clone)]
struct OverlayGateRoutePlan {
    effective_route: String,
    route_plan_source: String,
    runtime_probe_used: bool,
    auto_relay_hops: u64,
    reachability_probe_decision: Option<ReachabilityProbeDecision>,
}

impl OverlayGateRoutePlan {
    fn manual(route: &str) -> Self {
        Self {
            effective_route: route.to_string(),
            route_plan_source: "manual".into(),
            runtime_probe_used: false,
            auto_relay_hops: 0,
            reachability_probe_decision: None,
        }
    }

    fn simulated(route: &str, auto_relay_hops: u64, decision: ReachabilityProbeDecision) -> Self {
        Self {
            effective_route: route.to_string(),
            route_plan_source: "simulated_probe".into(),
            runtime_probe_used: false,
            auto_relay_hops,
            reachability_probe_decision: Some(decision),
        }
    }
}

fn route_plan_for(requested_route: &str, target_peer_id: &PeerId) -> Result<OverlayGateRoutePlan> {
    route_plan_for_with_runtime_probe(requested_route, target_peer_id, None, None)
}

fn route_plan_for_with_runtime_probe(
    requested_route: &str,
    target_peer_id: &PeerId,
    runtime_probe: Option<&OverlayGateRuntimeProbeReport>,
    local_bind_addr_override: Option<String>,
) -> Result<OverlayGateRoutePlan> {
    if requested_route != "auto" {
        return Ok(OverlayGateRoutePlan::manual(requested_route));
    }

    let direct_probe_ack = runtime_probe
        .map(|probe| probe.ack_received)
        .unwrap_or_else(|| env_bool("NOVOVM_OVERLAY_GATE_DIRECT_PROBE_ACK", false));
    let direct_probe_sent = runtime_probe
        .map(|probe| probe.probe_sent)
        .unwrap_or_else(|| env_bool("NOVOVM_OVERLAY_GATE_DIRECT_PROBE_SENT", true));
    let relay_available = env_bool("NOVOVM_OVERLAY_GATE_RELAY_AVAILABLE", false);
    let floating_port_mode = match env_string("NOVOVM_OVERLAY_GATE_FLOATING_PORT_MODE")
        .unwrap_or_else(|| "ephemeral".into())
        .as_str()
    {
        "fixed" => FloatingPortMode::Fixed,
        "ephemeral" | "ephemeral_allowed" => FloatingPortMode::EphemeralAllowed,
        other => anyhow::bail!("unsupported NOVOVM_OVERLAY_GATE_FLOATING_PORT_MODE: {other}"),
    };
    let decision = decide_reachability_probe_v0(ReachabilityProbeInput {
        peer_id: target_peer_id.0.clone(),
        configured_addr_hint: env_string("NOVOVM_OVERLAY_GATE_CONFIGURED_ADDR_HINT"),
        observed_addr: runtime_probe
            .and_then(|probe| probe.observed_addr.clone())
            .or_else(|| env_string("NOVOVM_OVERLAY_GATE_OBSERVED_ADDR")),
        local_bind_addr: local_bind_addr_override
            .or_else(|| env_string("NOVOVM_OVERLAY_GATE_LOCAL_BIND_ADDR"))
            .or_else(|| env_string("NOVOVM_OVERLAY_GATE_BIND_ADDR")),
        floating_port_mode,
        direct_probe_sent,
        direct_probe_ack,
        relay_available,
        rtt_ms: runtime_probe
            .and_then(|probe| probe.rtt_ms)
            .or_else(|| env_u32("NOVOVM_OVERLAY_GATE_PROBE_RTT_MS")),
        observed_unix_ms: env_u64("NOVOVM_OVERLAY_GATE_OBSERVED_UNIX_MS", 0),
        source: RoutingSource::LocalObserved,
    });
    let relay_hops = env_u64("NOVOVM_OVERLAY_GATE_AUTO_RELAY_HOPS", 1);
    let effective_route = match decision.status {
        ReachabilityProbeStatus::DirectReachable | ReachabilityProbeStatus::LanReachable => {
            "direct"
        }
        ReachabilityProbeStatus::RelayOnly if relay_hops >= 2 => "multihop",
        ReachabilityProbeStatus::RelayOnly => "relay",
        ReachabilityProbeStatus::Unreachable | ReachabilityProbeStatus::Unknown => "queue",
    }
    .to_string();

    Ok(OverlayGateRoutePlan {
        effective_route,
        route_plan_source: if runtime_probe.is_some() {
            "runtime_probe".into()
        } else {
            "simulated_probe".into()
        },
        runtime_probe_used: runtime_probe.is_some(),
        auto_relay_hops: relay_hops,
        reachability_probe_decision: Some(decision),
    })
}

fn simulated_probe_decision(
    target_peer_id: &PeerId,
    status: ReachabilityProbeStatus,
    relay_available: bool,
    direct_probe_ack: bool,
) -> Result<ReachabilityProbeDecision> {
    let (configured_addr_hint, observed_addr) = match status {
        ReachabilityProbeStatus::DirectReachable => (
            Some("8.8.8.8:39011".to_string()),
            Some("8.8.8.8:39011".to_string()),
        ),
        ReachabilityProbeStatus::LanReachable => (
            Some("127.0.0.1:39011".to_string()),
            Some("127.0.0.1:39011".to_string()),
        ),
        ReachabilityProbeStatus::RelayOnly
        | ReachabilityProbeStatus::Unreachable
        | ReachabilityProbeStatus::Unknown => (Some("203.0.113.10:39011".to_string()), None),
    };
    Ok(decide_reachability_probe_v0(ReachabilityProbeInput {
        peer_id: target_peer_id.0.clone(),
        configured_addr_hint,
        observed_addr,
        local_bind_addr: Some("127.0.0.1:0".into()),
        floating_port_mode: FloatingPortMode::EphemeralAllowed,
        direct_probe_sent: true,
        direct_probe_ack,
        relay_available,
        rtt_ms: direct_probe_ack.then_some(1),
        observed_unix_ms: 0,
        source: RoutingSource::LocalObserved,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct OverlayGateRuntimeProbeReport {
    probe_sent: bool,
    ack_received: bool,
    target_addr: String,
    observed_addr: Option<String>,
    sent_bytes: usize,
    received_bytes: usize,
    elapsed_ms: u64,
    rtt_ms: Option<u32>,
    error: Option<String>,
}

fn run_sender_direct_probe_v0(
    socket: &UdpSocket,
    target_addr: &str,
    request_id: &str,
) -> Result<OverlayGateRuntimeProbeReport> {
    let timeout_ms = env_u64("NOVOVM_OVERLAY_GATE_PROBE_TIMEOUT_MS", 500);
    socket
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .context("set sender probe read timeout")?;
    let payload = format!("novovm-overlay-probe:{request_id}").into_bytes();
    let probe = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Endpoint,
        [15u8; 16],
        100,
        200,
        300,
        400,
        payload.clone(),
    );
    let encoded = probe.encode();
    let start = Instant::now();
    let sent_bytes = socket
        .send_to(&encoded, target_addr)
        .with_context(|| format!("send direct probe to {target_addr}"))?;
    let mut buf = vec![0u8; 65535];
    match socket.recv_from(&mut buf) {
        Ok((received_bytes, observed_addr)) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let decoded =
                novovm_network::novorudp::NovoRudpTransportFrameV0::decode(&buf[..received_bytes]);
            match decoded {
                Ok(frame)
                    if frame.kind == NovoRudpTransportFrameKindV0::Ack
                        && frame.payload == payload =>
                {
                    Ok(OverlayGateRuntimeProbeReport {
                        probe_sent: true,
                        ack_received: true,
                        target_addr: target_addr.to_string(),
                        observed_addr: Some(observed_addr.to_string()),
                        sent_bytes,
                        received_bytes,
                        elapsed_ms,
                        rtt_ms: Some(elapsed_ms.min(u32::MAX as u64) as u32),
                        error: None,
                    })
                }
                Ok(frame) => Ok(OverlayGateRuntimeProbeReport {
                    probe_sent: true,
                    ack_received: false,
                    target_addr: target_addr.to_string(),
                    observed_addr: Some(observed_addr.to_string()),
                    sent_bytes,
                    received_bytes,
                    elapsed_ms,
                    rtt_ms: None,
                    error: Some(format!("unexpected probe ack frame kind: {:?}", frame.kind)),
                }),
                Err(error) => Ok(OverlayGateRuntimeProbeReport {
                    probe_sent: true,
                    ack_received: false,
                    target_addr: target_addr.to_string(),
                    observed_addr: Some(observed_addr.to_string()),
                    sent_bytes,
                    received_bytes,
                    elapsed_ms,
                    rtt_ms: None,
                    error: Some(format!("decode probe ack failed: {error}")),
                }),
            }
        }
        Err(error) => Ok(OverlayGateRuntimeProbeReport {
            probe_sent: true,
            ack_received: false,
            target_addr: target_addr.to_string(),
            observed_addr: None,
            sent_bytes,
            received_bytes: 0,
            elapsed_ms: start.elapsed().as_millis() as u64,
            rtt_ms: None,
            error: Some(format!("probe ack timeout or recv failed: {error}")),
        }),
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

fn env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|raw| matches!(raw.trim(), "1" | "true" | "TRUE" | "yes" | "on"))
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_u32(name: &str) -> Option<u32> {
    env::var(name).ok().and_then(|raw| raw.parse::<u32>().ok())
}
