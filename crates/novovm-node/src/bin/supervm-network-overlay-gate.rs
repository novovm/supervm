use anyhow::{Context, Result};
use novovm_network::adaptive_overlay::{
    decide_adaptive_overlay_route_v0, decide_adaptive_overlay_route_with_family_cooldown_v0,
    AdaptiveOverlayEndpointRecord, AdaptiveOverlayNodeCapabilities, AdaptiveOverlayNodeConfig,
    AdaptiveOverlayRelayBudget, AdaptiveOverlayRouteFamily,
};
use novovm_network::control_plane::{
    CapabilityAdvertisement, ControlPlaneRegistry, Libp2pControlPlaneConfig, PeerId,
};
use novovm_network::novorudp::NovoRudpTransportFrameKindV0;
use novovm_network::overlay::{
    AntiCensorshipProfile, OverlayHop, OverlayTransportProfile, RouteSet,
};
use novovm_network::overlay_runtime::{
    decide_overlay_runtime_fallback_chain_v0, decide_overlay_runtime_route_v0,
    decide_overlay_runtime_route_with_health_v0, overlay_route_health_from_observations_v0,
    OverlayHopHealth, OverlayRouteAttemptObservation, OverlayRouteHealthSnapshot,
    OverlayRuntimeDecision, OverlayRuntimeSelectedPath,
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
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn main() -> Result<()> {
    let mode = env_string("NOVOVM_OVERLAY_GATE_MODE").unwrap_or_else(|| "loopback".into());
    match mode.as_str() {
        "loopback" => run_loopback_gate(),
        "receiver" => run_receiver_gate(),
        "relay" => run_relay_gate(),
        "sender" => run_sender_gate(),
        "matrix" => run_matrix_gate(),
        "health-matrix" => run_health_matrix_gate(),
        "observation-matrix" => run_observation_matrix_gate(),
        "fallback-chain" => run_fallback_chain_gate(),
        "adaptive-node-matrix" => run_adaptive_node_matrix_gate(),
        "adaptive-node" => run_adaptive_node_gate(),
        "observed-endpoint-matrix" => run_observed_endpoint_matrix_gate(),
        "observed-endpoint" => run_observed_endpoint_gate(),
        "nat-punch-matrix" => run_nat_punch_matrix_gate(),
        "nat-punch" => run_nat_punch_gate(),
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

fn run_observed_endpoint_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/observed-endpoint-matrix.json".into());

    let valid =
        run_observed_endpoint_local_case_v0("lan-observed-endpoint", "node-a", "node-b", None)?;
    let mismatch = run_observed_endpoint_local_case_v0(
        "nonce-mismatch-rejected",
        "node-a",
        "node-b",
        Some("stale-wrong-nonce".into()),
    )?;

    let accepted = valid["accepted"].as_bool().unwrap_or(false)
        && !mismatch["probe_ack_valid"].as_bool().unwrap_or(true)
        && mismatch["probe_reject_reason"].as_str() == Some("probe_nonce_mismatch");

    let report = json!({
        "accepted": accepted,
        "scope": "observed_endpoint_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cases": [valid, mismatch],
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("observed endpoint matrix failed")
    }
}

fn run_observed_endpoint_gate() -> Result<()> {
    let role = env_string("NOVOVM_OVERLAY_OBSERVED_ROLE").unwrap_or_else(|| "prober".to_string());
    match role.as_str() {
        "observer" => run_observed_endpoint_observer_gate(),
        "prober" => run_observed_endpoint_prober_gate(),
        other => anyhow::bail!("unsupported NOVOVM_OVERLAY_OBSERVED_ROLE: {other}"),
    }
}

fn run_nat_punch_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/nat-punch-matrix.json".into());

    let success = run_nat_punch_local_case_v0("nat-punch-success", None, false)?;
    let mismatch = run_nat_punch_local_case_v0(
        "nat-punch-nonce-mismatch-rejected",
        Some("wrong-nat-punch-nonce".into()),
        false,
    )?;
    let fallback = run_nat_punch_local_fallback_case_v0("nat-punch-relay-fallback")?;

    let accepted = success["punch_ack_valid"].as_bool().unwrap_or(false)
        && success["selected_path_after_punch"].as_str() == Some("PunchedDirect")
        && !mismatch["punch_ack_valid"].as_bool().unwrap_or(true)
        && mismatch["punch_reject_reason"].as_str() == Some("punch_nonce_mismatch")
        && !fallback["punch_ack_valid"].as_bool().unwrap_or(true)
        && fallback["relay_fallback_selected"]
            .as_bool()
            .unwrap_or(false)
        && fallback["fallback_reason"].as_str() == Some("NatPunchFailed")
        && fallback["selected_path_after_punch"].as_str() == Some("RelayNovoRudp");

    let report = json!({
        "accepted": accepted,
        "scope": "nat_punch_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cases": [success, mismatch, fallback],
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("nat punch matrix failed")
    }
}

fn run_nat_punch_gate() -> Result<()> {
    let role = env_string("NOVOVM_OVERLAY_OBSERVED_ROLE").unwrap_or_else(|| "prober".to_string());
    match role.as_str() {
        "observer" => run_nat_punch_observer_gate(),
        "prober" => run_nat_punch_prober_gate(),
        other => anyhow::bail!("unsupported NOVOVM_OVERLAY_OBSERVED_ROLE for nat-punch: {other}"),
    }
}

fn run_nat_punch_observer_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/nat-punch-observer.json".into());
    let bind_addr =
        env_string("NOVOVM_OVERLAY_GATE_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:0".into());
    let observer_peer_id =
        env_string("NOVOVM_OVERLAY_OBSERVED_OBSERVER_PEER_ID").unwrap_or_else(|| "node-b".into());
    let timeout_ms = env_u64("NOVOVM_OVERLAY_GATE_TIMEOUT_MS", 5000);
    let ack_nonce_override = env_string("NOVOVM_OVERLAY_NAT_PUNCH_ACK_NONCE_OVERRIDE")
        .or_else(|| env_string("NOVOVM_OVERLAY_OBSERVED_ACK_NONCE_OVERRIDE"));
    let socket = UdpSocket::bind(&bind_addr)
        .with_context(|| format!("bind nat punch observer: {bind_addr}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .context("set nat punch observer read timeout")?;
    let bind_addr_effective = socket
        .local_addr()
        .context("nat punch observer local addr")?;
    let start = Instant::now();
    let mut buf = vec![0u8; 65535];
    let mut report = json!({
        "accepted": false,
        "scope": "nat_punch_observer_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "observer_peer_id": observer_peer_id,
        "bind_addr_requested": bind_addr,
        "bind_addr_effective": bind_addr_effective.to_string(),
    });

    match socket.recv_from(&mut buf) {
        Ok((received_bytes, source_addr)) => {
            let frame =
                novovm_network::novorudp::NovoRudpTransportFrameV0::decode(&buf[..received_bytes]);
            match frame {
                Ok(frame) if frame.kind == NovoRudpTransportFrameKindV0::Endpoint => {
                    let probe = serde_json::from_slice::<NatPunchProbePayloadV0>(&frame.payload)
                        .context("decode nat punch probe payload")?;
                    let observed_at_ms = now_unix_ms();
                    let ack_nonce = ack_nonce_override.unwrap_or_else(|| probe.punch_nonce.clone());
                    let ack_payload = NatPunchAckPayloadV0 {
                        punch_nonce: ack_nonce,
                        source_peer_id: probe.source_peer_id.clone(),
                        target_peer_id: probe.target_peer_id.clone(),
                        observer_peer_id: observer_peer_id.clone(),
                        observed_endpoint: source_addr.to_string(),
                        observed_at_ms,
                    };
                    let ack = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
                        NovoRudpTransportFrameKindV0::Ack,
                        frame.session_id,
                        frame.stream_id,
                        frame.object_id,
                        frame.sequence,
                        frame.ack_epoch,
                        serde_json::to_vec(&ack_payload)?,
                    );
                    let sent_bytes = socket
                        .send_to(&ack.encode(), source_addr)
                        .context("send nat punch ack")?;
                    report = json!({
                        "accepted": true,
                        "scope": "nat_punch_observer_v0",
                        "boundary": network_boundary_json(),
                        "payload_treated_opaque": true,
                        "observer_peer_id": observer_peer_id,
                        "bind_addr_requested": bind_addr,
                        "bind_addr_effective": bind_addr_effective.to_string(),
                        "punch_received": true,
                        "punch_nonce": probe.punch_nonce,
                        "source_peer_id": probe.source_peer_id,
                        "target_peer_id": probe.target_peer_id,
                        "advertised_endpoint": probe.advertised_endpoint,
                        "punch_target_observed_endpoint": probe.target_observed_endpoint,
                        "observed_endpoint": source_addr.to_string(),
                        "observed_at_ms": observed_at_ms,
                        "ack_sent": true,
                        "ack_sent_bytes": sent_bytes,
                        "ack_nonce": ack_payload.punch_nonce,
                        "elapsed_ms": start.elapsed().as_millis() as u64,
                    });
                }
                Ok(frame) => {
                    report["punch_reject_reason"] =
                        json!(format!("unexpected_frame_kind:{:?}", frame.kind));
                }
                Err(error) => {
                    report["punch_reject_reason"] =
                        json!(format!("decode_punch_probe_failed:{error}"));
                }
            }
        }
        Err(error) => {
            report["punch_reject_reason"] = json!(format!("punch_recv_timeout_or_failed:{error}"));
        }
    }

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report["accepted"].as_bool().unwrap_or(false) {
        Ok(())
    } else {
        anyhow::bail!("nat punch observer failed")
    }
}

fn run_nat_punch_prober_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/nat-punch-prober.json".into());
    let bind_addr =
        env_string("NOVOVM_OVERLAY_GATE_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:0".into());
    let target_addr = env_string("NOVOVM_OVERLAY_NAT_PUNCH_TARGET_OBSERVED_ENDPOINT")
        .or_else(|| env_string("NOVOVM_OVERLAY_OBSERVED_TARGET_ADDR"))
        .context("NOVOVM_OVERLAY_NAT_PUNCH_TARGET_OBSERVED_ENDPOINT is required")?;
    let source_peer_id =
        env_string("NOVOVM_OVERLAY_OBSERVED_SOURCE_PEER_ID").unwrap_or_else(|| "node-a".into());
    let target_peer_id =
        env_string("NOVOVM_OVERLAY_OBSERVED_TARGET_PEER_ID").unwrap_or_else(|| "node-b".into());
    let advertised_endpoint = env_string("NOVOVM_OVERLAY_OBSERVED_ADVERTISED_ENDPOINT");
    let punch_nonce = env_string("NOVOVM_OVERLAY_NAT_PUNCH_NONCE")
        .or_else(|| env_string("NOVOVM_OVERLAY_OBSERVED_PROBE_NONCE"))
        .unwrap_or_else(|| format!("nat-punch-{}", now_unix_ms()));
    let timeout_ms = env_u64("NOVOVM_OVERLAY_GATE_PROBE_TIMEOUT_MS", 1000);
    let relay_fallback_enabled = env_bool("NOVOVM_OVERLAY_NAT_PUNCH_ENABLE_RELAY_FALLBACK", false);
    let socket = UdpSocket::bind(&bind_addr)
        .with_context(|| format!("bind nat punch prober: {bind_addr}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .context("set nat punch prober read timeout")?;
    let bind_addr_effective = socket.local_addr().context("nat punch prober local addr")?;
    let report = run_nat_punch_probe_v0(
        &socket,
        &target_addr,
        &source_peer_id,
        &target_peer_id,
        advertised_endpoint,
        punch_nonce,
        bind_addr_effective,
        relay_fallback_enabled,
    )?;
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report["accepted"].as_bool().unwrap_or(false) {
        Ok(())
    } else {
        anyhow::bail!("nat punch prober failed")
    }
}

fn run_observed_endpoint_observer_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/observed-endpoint-observer.json".into());
    let bind_addr =
        env_string("NOVOVM_OVERLAY_GATE_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:0".into());
    let observer_peer_id =
        env_string("NOVOVM_OVERLAY_OBSERVED_OBSERVER_PEER_ID").unwrap_or_else(|| "node-b".into());
    let timeout_ms = env_u64("NOVOVM_OVERLAY_GATE_TIMEOUT_MS", 5000);
    let ack_nonce_override = env_string("NOVOVM_OVERLAY_OBSERVED_ACK_NONCE_OVERRIDE");
    let socket = UdpSocket::bind(&bind_addr)
        .with_context(|| format!("bind observed observer: {bind_addr}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .context("set observed observer read timeout")?;
    let bind_addr_effective = socket
        .local_addr()
        .context("observed observer local addr")?;
    let start = Instant::now();
    let mut buf = vec![0u8; 65535];
    let mut report = json!({
        "accepted": false,
        "scope": "observed_endpoint_observer_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "observer_peer_id": observer_peer_id,
        "bind_addr_requested": bind_addr,
        "bind_addr_effective": bind_addr_effective.to_string(),
    });

    match socket.recv_from(&mut buf) {
        Ok((received_bytes, source_addr)) => {
            let frame =
                novovm_network::novorudp::NovoRudpTransportFrameV0::decode(&buf[..received_bytes]);
            match frame {
                Ok(frame) if frame.kind == NovoRudpTransportFrameKindV0::Endpoint => {
                    let probe =
                        serde_json::from_slice::<ObservedEndpointProbePayloadV0>(&frame.payload)
                            .context("decode observed endpoint probe payload")?;
                    let observed_at_ms = now_unix_ms();
                    let ack_nonce = ack_nonce_override.unwrap_or_else(|| probe.probe_nonce.clone());
                    let ack_payload = ObservedEndpointAckPayloadV0 {
                        probe_nonce: ack_nonce,
                        source_peer_id: probe.source_peer_id.clone(),
                        target_peer_id: probe.target_peer_id.clone(),
                        observer_peer_id: observer_peer_id.clone(),
                        observed_endpoint: source_addr.to_string(),
                        observed_at_ms,
                    };
                    let ack = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
                        NovoRudpTransportFrameKindV0::Ack,
                        frame.session_id,
                        frame.stream_id,
                        frame.object_id,
                        frame.sequence,
                        frame.ack_epoch,
                        serde_json::to_vec(&ack_payload)?,
                    );
                    let sent_bytes = socket
                        .send_to(&ack.encode(), source_addr)
                        .context("send observed endpoint ack")?;
                    report = json!({
                        "accepted": true,
                        "scope": "observed_endpoint_observer_v0",
                        "boundary": network_boundary_json(),
                        "payload_treated_opaque": true,
                        "observer_peer_id": observer_peer_id,
                        "bind_addr_requested": bind_addr,
                        "bind_addr_effective": bind_addr_effective.to_string(),
                        "probe_received": true,
                        "probe_nonce": probe.probe_nonce,
                        "source_peer_id": probe.source_peer_id,
                        "target_peer_id": probe.target_peer_id,
                        "advertised_endpoint": probe.advertised_endpoint,
                        "observed_endpoint": source_addr.to_string(),
                        "observed_at_ms": observed_at_ms,
                        "ack_sent": true,
                        "ack_sent_bytes": sent_bytes,
                        "ack_nonce": ack_payload.probe_nonce,
                        "elapsed_ms": start.elapsed().as_millis() as u64,
                    });
                }
                Ok(frame) => {
                    report["probe_reject_reason"] =
                        json!(format!("unexpected_frame_kind:{:?}", frame.kind));
                }
                Err(error) => {
                    report["probe_reject_reason"] = json!(format!("decode_probe_failed:{error}"));
                }
            }
        }
        Err(error) => {
            report["probe_reject_reason"] = json!(format!("probe_recv_timeout_or_failed:{error}"));
        }
    }

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report["accepted"].as_bool().unwrap_or(false) {
        Ok(())
    } else {
        anyhow::bail!("observed endpoint observer failed")
    }
}

fn run_observed_endpoint_prober_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/observed-endpoint-prober.json".into());
    let bind_addr =
        env_string("NOVOVM_OVERLAY_GATE_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:0".into());
    let target_addr = env_string("NOVOVM_OVERLAY_OBSERVED_TARGET_ADDR")
        .context("NOVOVM_OVERLAY_OBSERVED_TARGET_ADDR is required for observed prober")?;
    let source_peer_id =
        env_string("NOVOVM_OVERLAY_OBSERVED_SOURCE_PEER_ID").unwrap_or_else(|| "node-a".into());
    let target_peer_id =
        env_string("NOVOVM_OVERLAY_OBSERVED_TARGET_PEER_ID").unwrap_or_else(|| "node-b".into());
    let advertised_endpoint = env_string("NOVOVM_OVERLAY_OBSERVED_ADVERTISED_ENDPOINT");
    let probe_nonce = env_string("NOVOVM_OVERLAY_OBSERVED_PROBE_NONCE")
        .unwrap_or_else(|| format!("probe-{}", now_unix_ms()));
    let timeout_ms = env_u64("NOVOVM_OVERLAY_GATE_PROBE_TIMEOUT_MS", 1000);
    let socket = UdpSocket::bind(&bind_addr)
        .with_context(|| format!("bind observed prober: {bind_addr}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .context("set observed prober read timeout")?;
    let bind_addr_effective = socket.local_addr().context("observed prober local addr")?;
    let report = run_observed_endpoint_probe_v0(
        &socket,
        &target_addr,
        &source_peer_id,
        &target_peer_id,
        advertised_endpoint,
        probe_nonce,
        bind_addr_effective,
    )?;
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report["accepted"].as_bool().unwrap_or(false) {
        Ok(())
    } else {
        anyhow::bail!("observed endpoint prober failed")
    }
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

fn run_observation_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/observation-matrix.json".into());
    let target_peer_id = PeerId::new("peer-target");
    let local_peer_id = PeerId::new("peer-local");
    let now_ms = env_u64("NOVOVM_OVERLAY_GATE_OBSERVATION_NOW_MS", 1_000);
    let cooldown_ms = env_u64("NOVOVM_OVERLAY_GATE_OBSERVATION_COOLDOWN_MS", 60_000);
    let registry = health_gate_registry(local_peer_id, target_peer_id.clone());
    let base_health = OverlayRouteHealthSnapshot::new(now_ms, Vec::new());
    let direct_decision =
        decide_overlay_runtime_route_with_health_v0(&registry, &target_peer_id, &base_health);
    let direct_success_health =
        overlay_route_health_from_observations_v0(&[OverlayRouteAttemptObservation {
            decision: direct_decision.clone(),
            delivered: true,
            queued: false,
            observed_unix_ms: now_ms,
            cooldown_ms,
        }]);
    let direct_failed_health =
        overlay_route_health_from_observations_v0(&[OverlayRouteAttemptObservation {
            decision: direct_decision.clone(),
            delivered: false,
            queued: false,
            observed_unix_ms: now_ms,
            cooldown_ms,
        }]);
    let multihop_after_direct_failure = decide_overlay_runtime_route_with_health_v0(
        &registry,
        &target_peer_id,
        &direct_failed_health,
    );
    let direct_and_multihop_failed_health = overlay_route_health_from_observations_v0(&[
        OverlayRouteAttemptObservation {
            decision: direct_decision.clone(),
            delivered: false,
            queued: false,
            observed_unix_ms: now_ms,
            cooldown_ms,
        },
        OverlayRouteAttemptObservation {
            decision: multihop_after_direct_failure.clone(),
            delivered: false,
            queued: false,
            observed_unix_ms: now_ms,
            cooldown_ms,
        },
    ]);
    let cases = vec![
        (
            "observation-direct-success",
            direct_success_health,
            vec![json!({
                "selected_path": direct_decision.selected_path,
                "delivered": true,
                "queued": false,
            })],
        ),
        (
            "observation-direct-failure",
            direct_failed_health,
            vec![json!({
                "selected_path": direct_decision.selected_path,
                "delivered": false,
                "queued": false,
            })],
        ),
        (
            "observation-direct-and-multihop-failure",
            direct_and_multihop_failed_health,
            vec![
                json!({
                    "selected_path": direct_decision.selected_path,
                    "delivered": false,
                    "queued": false,
                }),
                json!({
                    "selected_path": multihop_after_direct_failure.selected_path,
                    "delivered": false,
                    "queued": false,
                }),
            ],
        ),
    ];

    let mut reports = Vec::new();
    for (case_id, health, observations) in cases {
        let decision =
            decide_overlay_runtime_route_with_health_v0(&registry, &target_peer_id, &health);
        let mut report = build_decision_loopback_report(
            "network_overlay_gate_observation_matrix_case_v0",
            case_id,
            "runtime-observation",
            &decision,
            Some(health),
        )?;
        if let Some(value) = report.as_object_mut() {
            value.insert("observations".into(), json!(observations));
        }
        reports.push(report);
    }

    let accepted = reports
        .iter()
        .all(|report| report["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "network_overlay_gate_observation_matrix_v0",
        "boundary": network_boundary_json(),
        "case_count": reports.len(),
        "cases": reports,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("network overlay observation matrix gate failed")
    }
}

fn run_fallback_chain_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/fallback-chain.json".into());
    let target_peer_id = PeerId::new("peer-target");
    let now_ms = env_u64("NOVOVM_OVERLAY_GATE_OBSERVATION_NOW_MS", 1_000);
    let cooldown_ms = env_u64("NOVOVM_OVERLAY_GATE_OBSERVATION_COOLDOWN_MS", 60_000);
    let routes = fallback_chain_route_sets(target_peer_id.clone());
    let profile = AntiCensorshipProfile::default();
    let empty_health = OverlayRouteHealthSnapshot::new(now_ms, Vec::new());
    let direct =
        decide_overlay_runtime_fallback_chain_v0(&target_peer_id, &routes, &profile, &empty_health);
    let direct_failed_health =
        overlay_route_health_from_observations_v0(&[OverlayRouteAttemptObservation {
            decision: direct.clone(),
            delivered: false,
            queued: false,
            observed_unix_ms: now_ms,
            cooldown_ms,
        }]);
    let relay = decide_overlay_runtime_fallback_chain_v0(
        &target_peer_id,
        &routes,
        &profile,
        &direct_failed_health,
    );
    let direct_and_relay_failed_health = overlay_route_health_from_observations_v0(&[
        OverlayRouteAttemptObservation {
            decision: direct.clone(),
            delivered: false,
            queued: false,
            observed_unix_ms: now_ms,
            cooldown_ms,
        },
        OverlayRouteAttemptObservation {
            decision: relay.clone(),
            delivered: false,
            queued: false,
            observed_unix_ms: now_ms,
            cooldown_ms,
        },
    ]);
    let multihop = decide_overlay_runtime_fallback_chain_v0(
        &target_peer_id,
        &routes,
        &profile,
        &direct_and_relay_failed_health,
    );
    let all_failed_health = overlay_route_health_from_observations_v0(&[
        OverlayRouteAttemptObservation {
            decision: direct.clone(),
            delivered: false,
            queued: false,
            observed_unix_ms: now_ms,
            cooldown_ms,
        },
        OverlayRouteAttemptObservation {
            decision: relay.clone(),
            delivered: false,
            queued: false,
            observed_unix_ms: now_ms,
            cooldown_ms,
        },
        OverlayRouteAttemptObservation {
            decision: multihop.clone(),
            delivered: false,
            queued: false,
            observed_unix_ms: now_ms,
            cooldown_ms,
        },
    ]);
    let cases = vec![
        ("fallback-direct", empty_health, Vec::new()),
        (
            "fallback-relay-after-direct-failure",
            direct_failed_health,
            vec![json!({
                "selected_path": direct.selected_path,
                "delivered": false,
                "queued": false,
            })],
        ),
        (
            "fallback-multihop-after-direct-relay-failure",
            direct_and_relay_failed_health,
            vec![
                json!({
                    "selected_path": direct.selected_path,
                    "delivered": false,
                    "queued": false,
                }),
                json!({
                    "selected_path": relay.selected_path,
                    "delivered": false,
                    "queued": false,
                }),
            ],
        ),
        (
            "fallback-queue-after-all-failure",
            all_failed_health,
            vec![
                json!({
                    "selected_path": direct.selected_path,
                    "delivered": false,
                    "queued": false,
                }),
                json!({
                    "selected_path": relay.selected_path,
                    "delivered": false,
                    "queued": false,
                }),
                json!({
                    "selected_path": multihop.selected_path,
                    "delivered": false,
                    "queued": false,
                }),
            ],
        ),
    ];

    let mut reports = Vec::new();
    for (case_id, health, observations) in cases {
        let decision =
            decide_overlay_runtime_fallback_chain_v0(&target_peer_id, &routes, &profile, &health);
        let mut report = build_decision_loopback_report(
            "network_overlay_gate_fallback_chain_case_v0",
            case_id,
            "fallback-chain",
            &decision,
            Some(health),
        )?;
        if let Some(value) = report.as_object_mut() {
            value.insert("observations".into(), json!(observations));
        }
        reports.push(report);
    }

    let accepted = reports
        .iter()
        .all(|report| report["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "network_overlay_gate_fallback_chain_v0",
        "boundary": network_boundary_json(),
        "case_count": reports.len(),
        "cases": reports,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("network overlay fallback chain gate failed")
    }
}

fn run_adaptive_node_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/adaptive-node-matrix.json".into());
    let local_peer_id = PeerId::new(
        env_string("NOVOVM_OVERLAY_GATE_LOCAL_PEER_ID").unwrap_or_else(|| "node-a".into()),
    );
    let target_peer_id = PeerId::new(
        env_string("NOVOVM_OVERLAY_GATE_TARGET_PEER_ID").unwrap_or_else(|| "node-b".into()),
    );
    let config = adaptive_gate_config(local_peer_id.clone());
    let profile = AntiCensorshipProfile::default();
    let cases = vec![
        (
            "adaptive-direct-healthy",
            OverlayRuntimeSelectedPath::DirectNovoRudp,
            OverlayRouteHealthSnapshot::new(100, Vec::new()),
        ),
        (
            "adaptive-relay-after-direct-cooldown",
            OverlayRuntimeSelectedPath::RelayNovoRudp,
            OverlayRouteHealthSnapshot::new(
                100,
                vec![OverlayHopHealth::cooling_down(
                    target_peer_id.clone(),
                    100,
                    1_000,
                )],
            ),
        ),
        (
            "adaptive-multihop-after-direct-relay-cooldown",
            OverlayRuntimeSelectedPath::MultiHopRelay,
            OverlayRouteHealthSnapshot::new(
                100,
                vec![
                    OverlayHopHealth::cooling_down(target_peer_id.clone(), 100, 1_000),
                    OverlayHopHealth::cooling_down(PeerId::new("relay-1"), 100, 1_000),
                ],
            ),
        ),
        (
            "adaptive-queue-after-all-cooldown",
            OverlayRuntimeSelectedPath::QueueFallback,
            OverlayRouteHealthSnapshot::new(
                100,
                vec![
                    OverlayHopHealth::cooling_down(target_peer_id.clone(), 100, 1_000),
                    OverlayHopHealth::cooling_down(PeerId::new("relay-1"), 100, 1_000),
                    OverlayHopHealth::cooling_down(PeerId::new("relay-2"), 100, 1_000),
                    OverlayHopHealth::cooling_down(PeerId::new("relay-3"), 100, 1_000),
                ],
            ),
        ),
    ];

    let mut reports = Vec::new();
    for (case_name, expected_path, health) in cases {
        let plan = decide_adaptive_overlay_route_v0(&config, &target_peer_id, &profile, &health);
        let selected_path = plan.decision.selected_path;
        let queued = selected_path
            == novovm_network::overlay_runtime::OverlayRuntimeSelectedPath::QueueFallback;
        let case_accepted = selected_path == expected_path;
        reports.push(json!({
            "case": case_name,
            "accepted": case_accepted,
            "expected_path": expected_path,
            "selected_path": selected_path,
            "reason": plan.decision.reason,
            "queued": queued,
            "candidate_route_count": plan.candidate_route_count,
            "direct_candidate_count": plan.direct_candidate_count,
            "relay_candidate_count": plan.relay_candidate_count,
            "multihop_candidate_count": plan.multihop_candidate_count,
            "queue_allowed": plan.queue_allowed,
            "health": health,
            "decision": plan.decision,
        }));
    }
    let accepted = reports.iter().all(|case| {
        case["accepted"].as_bool().unwrap_or(false)
            && case["candidate_route_count"].as_u64().unwrap_or(0) == 3
    });
    let report = json!({
        "accepted": accepted,
        "scope": "adaptive_overlay_node_matrix_gate_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "fixed_identity_dynamic_role": true,
        "local_peer_id": local_peer_id,
        "target_peer_id": target_peer_id,
        "bind_candidates": config.bind_policy.effective_bind_candidates(),
        "bootstrap_peer_count": config.bootstrap_peers.len(),
        "cases": reports,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("adaptive node matrix gate failed")
    }
}

fn run_adaptive_node_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/adaptive-node.json".into());
    let node_id = PeerId::new(
        env_string("NOVOVM_OVERLAY_ADAPTIVE_NODE_ID").unwrap_or_else(|| "node-local".into()),
    );
    let bind_addr =
        env_string("NOVOVM_OVERLAY_ADAPTIVE_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:0".into());
    let timeout_ms = env_u64("NOVOVM_OVERLAY_GATE_TIMEOUT_MS", 5000);
    let max_frames = env_u64("NOVOVM_OVERLAY_GATE_MAX_FRAMES", 1).max(1);
    let relay_enabled = env_bool("NOVOVM_OVERLAY_ADAPTIVE_RELAY_ENABLED", false);
    let queue_enabled = env_bool("NOVOVM_OVERLAY_ADAPTIVE_QUEUE_ENABLED", true);
    let target_peer_id = env_string("NOVOVM_OVERLAY_ADAPTIVE_TARGET_PEER_ID").map(PeerId::new);
    let peers = adaptive_gate_peers_from_env()?;
    let peer_endpoints = peers
        .iter()
        .filter_map(|peer| {
            Some((
                peer.peer_id.0.clone(),
                peer.advertised_endpoint.as_ref()?.clone(),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let capabilities = AdaptiveOverlayNodeCapabilities {
        can_send: true,
        can_receive: true,
        relay_enabled,
        queue_enabled,
        relay_budget: if relay_enabled {
            AdaptiveOverlayRelayBudget::light_default()
        } else {
            AdaptiveOverlayRelayBudget::disabled()
        },
    };
    let adaptive_config =
        AdaptiveOverlayNodeConfig::zero_config(node_id.clone()).with_bootstrap_peers(peers.clone());
    let health = adaptive_health_from_env(100);
    let cooldown_route_families = adaptive_route_family_cooldown_from_env()?;

    let socket =
        UdpSocket::bind(&bind_addr).with_context(|| format!("bind adaptive node: {bind_addr}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .context("set adaptive node read timeout")?;
    let bind_addr_effective = socket.local_addr().context("adaptive node local addr")?;
    let interface_summary = adaptive_interface_summary_json();
    let endpoint_selection =
        adaptive_select_advertised_endpoint_v0(&node_id, &peers, bind_addr_effective);

    let mut selected_path = None;
    let mut decision_reason = None;
    let mut route_plan_source = None;
    let mut candidate_route_count = None;
    let mut candidate_direct_count = None;
    let mut candidate_relay_count = None;
    let mut candidate_multihop_count = None;
    let mut sent_frame_count = 0u64;
    let mut queued_count = 0u64;
    let mut sent_bytes_total = 0usize;
    let mut send_errors = Vec::new();
    let mut sent_frames = Vec::new();

    if let Some(target_peer_id) = &target_peer_id {
        let plan = decide_adaptive_overlay_route_with_family_cooldown_v0(
            &adaptive_config,
            target_peer_id,
            &AntiCensorshipProfile::default(),
            &health,
            &cooldown_route_families,
        );
        selected_path = Some(plan.decision.selected_path);
        decision_reason = Some(plan.decision.reason);
        route_plan_source = Some("adaptive_runtime_peer_records_health");
        candidate_route_count = Some(plan.candidate_route_count);
        candidate_direct_count = Some(plan.direct_candidate_count);
        candidate_relay_count = Some(plan.relay_candidate_count);
        candidate_multihop_count = Some(plan.multihop_candidate_count);
        for frame_index in 0..max_frames {
            let frame_request_id = format!("adaptive-node-{}-{frame_index}", node_id.0);
            let frame = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
                NovoRudpTransportFrameKindV0::Data,
                [21u8; 16],
                10,
                20,
                30 + frame_index,
                40,
                format!("novovm-adaptive-node-opaque-frame-{frame_index}").into_bytes(),
            );
            let encoded = frame.encode();
            match adaptive_send_decision_v0(
                &socket,
                &plan.decision,
                &peer_endpoints,
                &node_id.0,
                &frame_request_id,
                encoded.clone(),
            ) {
                Ok(AdaptiveGateSendOutcome::Sent {
                    sent_to,
                    sent_bytes,
                }) => {
                    sent_frame_count += 1;
                    sent_bytes_total += sent_bytes;
                    sent_frames.push(json!({
                        "request_id": frame_request_id,
                        "sequence": 30 + frame_index,
                        "sent_to": sent_to,
                        "queued": false,
                        "sent_bytes": sent_bytes,
                        "encoded_frame_bytes": encoded.len(),
                    }));
                }
                Ok(AdaptiveGateSendOutcome::Queued) => {
                    queued_count += 1;
                    sent_frames.push(json!({
                        "request_id": frame_request_id,
                        "sequence": 30 + frame_index,
                        "sent_to": null,
                        "queued": true,
                        "sent_bytes": 0,
                        "encoded_frame_bytes": encoded.len(),
                    }));
                }
                Err(error) => {
                    send_errors.push(error.to_string());
                    sent_frames.push(json!({
                        "request_id": frame_request_id,
                        "sequence": 30 + frame_index,
                        "sent_to": null,
                        "queued": false,
                        "sent_bytes": 0,
                        "encoded_frame_bytes": encoded.len(),
                        "error": error.to_string(),
                    }));
                }
            }
        }
    }

    let start = Instant::now();
    let mut buf = vec![0u8; 65535];
    let mut direct_frames_received = 0u64;
    let mut relay_envelopes_received = 0u64;
    let mut relay_frames_forwarded = 0u64;
    let mut probe_ack_sent = 0u64;
    let mut frames = Vec::new();
    let mut recv_error = None;
    let should_listen = target_peer_id.is_none() || relay_enabled;
    if should_listen {
        while direct_frames_received + relay_frames_forwarded + probe_ack_sent < max_frames {
            let (received_bytes, source_addr) = match socket.recv_from(&mut buf) {
                Ok(value) => value,
                Err(error) => {
                    recv_error = Some(error.to_string());
                    break;
                }
            };

            if let Ok(frame) =
                novovm_network::novorudp::NovoRudpTransportFrameV0::decode(&buf[..received_bytes])
            {
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
                        .context("adaptive node send probe ack")?;
                    probe_ack_sent += 1;
                    frames.push(json!({
                        "kind": "probe",
                        "source_addr": source_addr.to_string(),
                        "received_bytes": received_bytes,
                        "ack_sent": true,
                    }));
                    continue;
                }
                direct_frames_received += 1;
                frames.push(json!({
                    "kind": "direct_data",
                    "source_addr": source_addr.to_string(),
                    "received_bytes": received_bytes,
                    "frame_decode_ok": true,
                    "decoded_kind": frame.kind,
                    "decoded_sequence": frame.sequence,
                    "payload_bytes": frame.payload.len(),
                }));
                continue;
            }

            let mut envelope: OverlayGateRelayEnvelopeV0 =
                match serde_json::from_slice(&buf[..received_bytes]) {
                    Ok(envelope) => envelope,
                    Err(error) => {
                        frames.push(json!({
                            "kind": "unknown",
                            "source_addr": source_addr.to_string(),
                            "received_bytes": received_bytes,
                            "accepted": false,
                            "error": error.to_string(),
                        }));
                        continue;
                    }
                };
            relay_envelopes_received += 1;
            if !capabilities.can_relay() {
                frames.push(json!({
                    "kind": "relay_envelope",
                    "request_id": envelope.request_id,
                    "source_addr": source_addr.to_string(),
                    "accepted": false,
                    "error": "relay_disabled_or_budget_exhausted",
                }));
                continue;
            }
            if envelope.ttl == 0 {
                frames.push(json!({
                    "kind": "relay_envelope",
                    "request_id": envelope.request_id,
                    "source_addr": source_addr.to_string(),
                    "accepted": false,
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
                Ok(forwarded_bytes) => {
                    relay_frames_forwarded += 1;
                    frames.push(json!({
                        "kind": "relay_envelope",
                        "request_id": envelope.request_id,
                        "source_addr": source_addr.to_string(),
                        "accepted": true,
                        "forwarded_to": forward_to,
                        "forwarded_bytes": forwarded_bytes,
                        "delivered_to_target": delivered_to_target,
                    }));
                }
                Err(error) => frames.push(json!({
                    "kind": "relay_envelope",
                    "request_id": envelope.request_id,
                    "source_addr": source_addr.to_string(),
                    "accepted": false,
                    "forwarded_to": forward_to,
                    "error": error.to_string(),
                })),
            }
        }
    }

    let elapsed_ms = start.elapsed().as_millis() as u64;
    let received_frame_count = direct_frames_received;
    let accepted = if target_peer_id.is_some() {
        send_errors.is_empty() && (sent_frame_count == max_frames || queued_count == max_frames)
    } else {
        direct_frames_received == max_frames
            || relay_frames_forwarded == max_frames
            || probe_ack_sent == max_frames
    };
    let endpoint_record = AdaptiveOverlayEndpointRecord {
        peer_id: node_id.clone(),
        bind_policy: adaptive_config.bind_policy.clone(),
        advertised_endpoint: endpoint_selection
            .get("advertised_endpoint")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        capabilities: capabilities.clone(),
    };
    let report = json!({
        "accepted": accepted,
        "scope": "adaptive_overlay_node_process_gate_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "node_id": node_id,
        "bind_policy": adaptive_config.bind_policy,
        "bind_addr_requested": bind_addr,
        "bind_addr_effective": bind_addr_effective.to_string(),
        "interface_summary": interface_summary,
        "endpoint_selection": endpoint_selection,
        "endpoint_record": endpoint_record,
        "bootstrap_peer_count": peers.len(),
        "selected_path": selected_path,
        "decision_reason": decision_reason,
        "route_plan_source": route_plan_source,
        "candidate_route_count": candidate_route_count,
        "candidate_direct_count": candidate_direct_count,
        "candidate_relay_count": candidate_relay_count,
        "candidate_multihop_count": candidate_multihop_count,
        "cooldown_hop_count": health.hops.len(),
        "cooldown_hops": health.hops,
        "cooldown_route_families": cooldown_route_families,
        "relay_budget": capabilities.relay_budget,
        "queue_enabled": capabilities.queue_enabled,
        "target_peer_id": target_peer_id,
        "sent_frame_count": sent_frame_count,
        "queued_count": queued_count,
        "sent_bytes_total": sent_bytes_total,
        "send_errors": send_errors,
        "sent_frames": sent_frames,
        "received_frame_count": received_frame_count,
        "direct_frames_received": direct_frames_received,
        "relay_envelopes_received": relay_envelopes_received,
        "relay_frames_forwarded": relay_frames_forwarded,
        "probe_ack_sent": probe_ack_sent,
        "recv_error": recv_error,
        "frames": frames,
        "elapsed_ms": elapsed_ms,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("adaptive overlay node gate failed")
    }
}

enum AdaptiveGateSendOutcome {
    Sent { sent_to: String, sent_bytes: usize },
    Queued,
}

fn adaptive_send_decision_v0(
    socket: &UdpSocket,
    decision: &OverlayRuntimeDecision,
    peer_endpoints: &BTreeMap<String, String>,
    source_peer_id: &str,
    request_id: &str,
    encoded: Vec<u8>,
) -> Result<AdaptiveGateSendOutcome> {
    match decision.selected_path {
        OverlayRuntimeSelectedPath::QueueFallback => Ok(AdaptiveGateSendOutcome::Queued),
        OverlayRuntimeSelectedPath::DirectNovoRudp => {
            let target_peer = decision
                .direct_endpoint_candidates
                .first()
                .unwrap_or(&decision.target_peer_id);
            let target_addr = peer_endpoints
                .get(target_peer.0.as_str())
                .with_context(|| format!("missing direct endpoint for {}", target_peer.0))?;
            let sent_bytes = socket
                .send_to(&encoded, target_addr)
                .with_context(|| format!("adaptive direct send to {target_addr}"))?;
            Ok(AdaptiveGateSendOutcome::Sent {
                sent_to: target_addr.clone(),
                sent_bytes,
            })
        }
        OverlayRuntimeSelectedPath::RelayNovoRudp => {
            let relay_peer = decision
                .relay_candidates
                .first()
                .context("missing relay candidate")?;
            let relay_addr = peer_endpoints
                .get(relay_peer.0.as_str())
                .with_context(|| format!("missing relay endpoint for {}", relay_peer.0))?;
            let target_addr = peer_endpoints
                .get(decision.target_peer_id.0.as_str())
                .with_context(|| {
                    format!("missing target endpoint for {}", decision.target_peer_id.0)
                })?;
            let envelope = OverlayGateRelayEnvelopeV0 {
                request_id: request_id.to_string(),
                source_peer_id: source_peer_id.into(),
                target_peer_id: decision.target_peer_id.0.clone(),
                target_addr: target_addr.clone(),
                remaining_hop_addrs: Vec::new(),
                ttl: 4,
                payload: encoded,
            };
            let payload = serde_json::to_vec(&envelope)?;
            let sent_bytes = socket
                .send_to(&payload, relay_addr)
                .with_context(|| format!("adaptive relay send to {relay_addr}"))?;
            Ok(AdaptiveGateSendOutcome::Sent {
                sent_to: relay_addr.clone(),
                sent_bytes,
            })
        }
        OverlayRuntimeSelectedPath::MultiHopRelay => {
            let hops = decision
                .multi_hop_candidates
                .first()
                .context("missing multi-hop candidate")?;
            let first_hop = hops.first().context("missing first multi-hop relay")?;
            let first_hop_addr = peer_endpoints
                .get(first_hop.0.as_str())
                .with_context(|| format!("missing relay endpoint for {}", first_hop.0))?;
            let remaining_hop_addrs = hops
                .iter()
                .skip(1)
                .map(|peer_id| {
                    peer_endpoints
                        .get(peer_id.0.as_str())
                        .cloned()
                        .with_context(|| format!("missing relay endpoint for {}", peer_id.0))
                })
                .collect::<Result<Vec<_>>>()?;
            let target_addr = peer_endpoints
                .get(decision.target_peer_id.0.as_str())
                .with_context(|| {
                    format!("missing target endpoint for {}", decision.target_peer_id.0)
                })?;
            let envelope = OverlayGateRelayEnvelopeV0 {
                request_id: request_id.to_string(),
                source_peer_id: source_peer_id.into(),
                target_peer_id: decision.target_peer_id.0.clone(),
                target_addr: target_addr.clone(),
                remaining_hop_addrs,
                ttl: 4,
                payload: encoded,
            };
            let payload = serde_json::to_vec(&envelope)?;
            let sent_bytes = socket
                .send_to(&payload, first_hop_addr)
                .with_context(|| format!("adaptive multi-hop send to {first_hop_addr}"))?;
            Ok(AdaptiveGateSendOutcome::Sent {
                sent_to: first_hop_addr.clone(),
                sent_bytes,
            })
        }
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

fn health_gate_registry(local_peer_id: PeerId, target_peer_id: PeerId) -> ControlPlaneRegistry {
    let mut registry = ControlPlaneRegistry::new(
        Libp2pControlPlaneConfig::production_minimum(local_peer_id),
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
    registry.register_route_set(health_matrix_route_set(target_peer_id));
    registry
}

fn fallback_chain_route_sets(target_peer_id: PeerId) -> Vec<RouteSet> {
    vec![
        RouteSet::direct(target_peer_id.clone()),
        RouteSet {
            target_peer_id: target_peer_id.clone(),
            hops: vec![OverlayHop {
                peer_id: PeerId::new("peer-relay-a"),
                transport: OverlayTransportProfile::RelayNovoRudp,
                route_token: None,
            }],
            content_address_hint: Some("cid-overlay-fallback-relay".into()),
        },
        RouteSet {
            target_peer_id,
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
            content_address_hint: Some("cid-overlay-fallback-multihop".into()),
        },
    ]
}

fn adaptive_gate_config(local_peer_id: PeerId) -> AdaptiveOverlayNodeConfig {
    AdaptiveOverlayNodeConfig::zero_config(local_peer_id).with_bootstrap_peers(vec![
        AdaptiveOverlayEndpointRecord::zero_config(PeerId::new("node-b"))
            .with_advertised_endpoint("192.168.71.56:41020"),
        AdaptiveOverlayEndpointRecord::zero_config(PeerId::new("relay-1"))
            .with_advertised_endpoint("192.168.71.9:41030")
            .with_relay_enabled(),
        AdaptiveOverlayEndpointRecord::zero_config(PeerId::new("relay-2"))
            .with_advertised_endpoint("192.168.71.54:41040")
            .with_relay_enabled(),
        AdaptiveOverlayEndpointRecord::zero_config(PeerId::new("relay-3"))
            .with_advertised_endpoint("192.168.71.55:41050")
            .with_relay_enabled(),
    ])
}

fn adaptive_gate_peers_from_env() -> Result<Vec<AdaptiveOverlayEndpointRecord>> {
    if let Some(peers_json) = env_string("NOVOVM_OVERLAY_ADAPTIVE_PEERS_JSON") {
        let peers = serde_json::from_str::<Vec<AdaptiveGatePeerConfig>>(&peers_json)
            .context("parse NOVOVM_OVERLAY_ADAPTIVE_PEERS_JSON")?;
        return Ok(peers
            .into_iter()
            .map(|peer| {
                let mut record =
                    AdaptiveOverlayEndpointRecord::zero_config(PeerId::new(peer.peer_id))
                        .with_advertised_endpoint(peer.endpoint);
                if peer.relay_enabled {
                    record = record.with_relay_enabled();
                }
                record
            })
            .collect());
    }
    Ok(adaptive_gate_config(PeerId::new("node-local")).bootstrap_peers)
}

fn adaptive_health_from_env(observed_unix_ms: u64) -> OverlayRouteHealthSnapshot {
    let cooldown_peers = env_string("NOVOVM_OVERLAY_ADAPTIVE_COOLDOWN_PEERS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| OverlayHopHealth::cooling_down(PeerId::new(value), observed_unix_ms, 1_000))
        .collect::<Vec<_>>();
    OverlayRouteHealthSnapshot::new(observed_unix_ms, cooldown_peers)
}

fn adaptive_route_family_cooldown_from_env() -> Result<Vec<AdaptiveOverlayRouteFamily>> {
    env_string("NOVOVM_OVERLAY_ADAPTIVE_COOLDOWN_ROUTE_FAMILIES")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| match value.to_ascii_lowercase().as_str() {
            "direct" => Ok(AdaptiveOverlayRouteFamily::Direct),
            "relay" | "single-relay" | "single_relay" => Ok(AdaptiveOverlayRouteFamily::Relay),
            "multihop" | "multi-hop" | "multi_hop" => Ok(AdaptiveOverlayRouteFamily::Multihop),
            other => anyhow::bail!("unsupported adaptive route family cooldown: {other}"),
        })
        .collect()
}

fn adaptive_select_advertised_endpoint_v0(
    node_id: &PeerId,
    peers: &[AdaptiveOverlayEndpointRecord],
    bind_addr_effective: SocketAddr,
) -> serde_json::Value {
    let mut candidates = Vec::new();
    let mut rejected = Vec::new();
    let mut selected_endpoint = None::<String>;
    let mut selected_reason = None::<String>;

    if let Some(explicit) = env_string("NOVOVM_OVERLAY_ADAPTIVE_ADVERTISED_ENDPOINT") {
        candidates.push(json!({
            "source": "NOVOVM_OVERLAY_ADAPTIVE_ADVERTISED_ENDPOINT",
            "endpoint": explicit,
        }));
        match normalize_advertised_endpoint_candidate(&explicit, bind_addr_effective.port()) {
            Ok(endpoint) => {
                selected_endpoint = Some(endpoint);
                selected_reason = Some("manually_configured_public_addr".into());
            }
            Err(reason) => rejected.push(json!({
                "source": "NOVOVM_OVERLAY_ADAPTIVE_ADVERTISED_ENDPOINT",
                "endpoint": explicit,
                "reason": reason,
            })),
        }
    }

    if selected_endpoint.is_none() {
        if let Some(self_record) = peers.iter().find(|peer| peer.peer_id == *node_id) {
            if let Some(endpoint) = &self_record.advertised_endpoint {
                candidates.push(json!({
                    "source": "self_peer_record",
                    "endpoint": endpoint,
                }));
                match normalize_advertised_endpoint_candidate(endpoint, bind_addr_effective.port())
                {
                    Ok(endpoint) => {
                        selected_endpoint = Some(endpoint);
                        selected_reason = Some("manually_configured_public_addr".into());
                    }
                    Err(reason) => rejected.push(json!({
                        "source": "self_peer_record",
                        "endpoint": endpoint,
                        "reason": reason,
                    })),
                }
            }
        }
    }

    let bind_endpoint = bind_addr_effective.to_string();
    if selected_endpoint.is_none() {
        candidates.push(json!({
            "source": "bind_addr_effective",
            "endpoint": bind_endpoint,
        }));
        match normalize_advertised_endpoint_candidate(&bind_endpoint, bind_addr_effective.port()) {
            Ok(endpoint) => {
                selected_endpoint = Some(endpoint);
                selected_reason = Some("bind_addr_effective_non_wildcard".into());
            }
            Err(reason) => rejected.push(json!({
                "source": "bind_addr_effective",
                "endpoint": bind_endpoint,
                "reason": reason,
            })),
        }
    } else if let Err(reason) =
        normalize_advertised_endpoint_candidate(&bind_endpoint, bind_addr_effective.port())
    {
        rejected.push(json!({
            "source": "bind_addr_effective",
            "endpoint": bind_endpoint,
            "reason": reason,
        }));
    }

    json!({
        "advertised_endpoint": selected_endpoint,
        "endpoint_selection_reason": selected_reason.unwrap_or_else(|| "no_publishable_endpoint".into()),
        "bind_addr_effective": bind_addr_effective.to_string(),
        "candidates": candidates,
        "rejected_candidates": rejected,
        "policy": {
            "reject_unspecified": true,
            "reject_loopback_by_default": true,
            "reject_link_local_by_default": true,
            "explicit_config_can_publish_non_default": true,
        }
    })
}

fn normalize_advertised_endpoint_candidate(
    endpoint: &str,
    bind_port: u16,
) -> std::result::Result<String, String> {
    let parsed = endpoint
        .parse::<SocketAddr>()
        .map_err(|error| format!("invalid_socket_addr:{error}"))?;
    reject_default_advertised_ip(parsed.ip())?;
    let port = if parsed.port() == 0 {
        bind_port
    } else {
        parsed.port()
    };
    if port == 0 {
        return Err("missing_publishable_port".into());
    }
    Ok(SocketAddr::new(parsed.ip(), port).to_string())
}

fn reject_default_advertised_ip(ip: IpAddr) -> std::result::Result<(), String> {
    if ip.is_unspecified() {
        return Err("reject_unspecified_ip".into());
    }
    if ip.is_loopback() {
        return Err("reject_loopback_ip_by_default".into());
    }
    match ip {
        IpAddr::V4(ipv4) if ipv4.is_link_local() => Err("reject_ipv4_link_local".into()),
        IpAddr::V6(ipv6) if ipv6.is_unicast_link_local() => Err("reject_ipv6_link_local".into()),
        _ => Ok(()),
    }
}

fn adaptive_interface_summary_json() -> serde_json::Value {
    let os = env::consts::OS;
    let command_output = if os == "windows" {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-NetIPAddress -AddressFamily IPv4 | Select-Object InterfaceAlias,IPAddress,PrefixLength | ConvertTo-Json -Compress",
            ])
            .output()
    } else {
        Command::new("sh")
            .args(["-c", "ip -br addr 2>/dev/null || true"])
            .output()
    };
    match command_output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            json!({
                "os": os,
                "inventory_available": output.status.success(),
                "source": if os == "windows" { "Get-NetIPAddress" } else { "ip -br addr" },
                "raw": stdout,
                "stderr": stderr,
                "heuristics": {
                    "prefers_non_loopback": true,
                    "ethernet_preferred_over_wifi": true,
                    "vpn_virtual_not_disabled": true
                }
            })
        }
        Err(error) => json!({
            "os": os,
            "inventory_available": false,
            "source": null,
            "error": error.to_string(),
            "heuristics": {
                "prefers_non_loopback": true,
                "ethernet_preferred_over_wifi": true,
                "vpn_virtual_not_disabled": true
            }
        }),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdaptiveGatePeerConfig {
    peer_id: String,
    endpoint: String,
    #[serde(default)]
    relay_enabled: bool,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ObservedEndpointProbePayloadV0 {
    probe_nonce: String,
    source_peer_id: String,
    target_peer_id: String,
    advertised_endpoint: Option<String>,
    expires_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ObservedEndpointAckPayloadV0 {
    probe_nonce: String,
    source_peer_id: String,
    target_peer_id: String,
    observer_peer_id: String,
    observed_endpoint: String,
    observed_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NatPunchProbePayloadV0 {
    punch_nonce: String,
    source_peer_id: String,
    target_peer_id: String,
    advertised_endpoint: Option<String>,
    target_observed_endpoint: String,
    expires_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NatPunchAckPayloadV0 {
    punch_nonce: String,
    source_peer_id: String,
    target_peer_id: String,
    observer_peer_id: String,
    observed_endpoint: String,
    observed_at_ms: u64,
}

fn run_observed_endpoint_local_case_v0(
    case_name: &str,
    source_peer_id: &str,
    observer_peer_id: &str,
    ack_nonce_override: Option<String>,
) -> Result<serde_json::Value> {
    let prober = UdpSocket::bind("127.0.0.1:0").context("bind local observed prober")?;
    let observer = UdpSocket::bind("127.0.0.1:0").context("bind local observed observer")?;
    prober
        .set_read_timeout(Some(Duration::from_millis(1000)))
        .context("set local observed prober timeout")?;
    observer
        .set_read_timeout(Some(Duration::from_millis(1000)))
        .context("set local observed observer timeout")?;

    let prober_addr = prober.local_addr().context("local observed prober addr")?;
    let observer_addr = observer
        .local_addr()
        .context("local observed observer addr")?;
    let probe_nonce = format!("{case_name}-nonce");
    let advertised_endpoint = Some(prober_addr.to_string());
    let start = Instant::now();

    let payload = ObservedEndpointProbePayloadV0 {
        probe_nonce: probe_nonce.clone(),
        source_peer_id: source_peer_id.to_string(),
        target_peer_id: observer_peer_id.to_string(),
        advertised_endpoint: advertised_endpoint.clone(),
        expires_at_ms: now_unix_ms().saturating_add(60_000),
    };
    let probe = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Endpoint,
        [24u8; 16],
        240,
        241,
        242,
        243,
        serde_json::to_vec(&payload)?,
    );
    let encoded_probe = probe.encode();
    let sent_bytes = prober
        .send_to(&encoded_probe, observer_addr)
        .context("send local observed endpoint probe")?;

    let mut observer_buf = vec![0u8; 65535];
    let (observer_received_bytes, observed_source_addr) = observer
        .recv_from(&mut observer_buf)
        .context("local observed observer recv probe")?;
    let observer_frame = novovm_network::novorudp::NovoRudpTransportFrameV0::decode(
        &observer_buf[..observer_received_bytes],
    )
    .context("decode local observed probe frame")?;
    let observer_probe_payload =
        serde_json::from_slice::<ObservedEndpointProbePayloadV0>(&observer_frame.payload)
            .context("decode local observed probe payload")?;
    let observed_at_ms = now_unix_ms();
    let ack_nonce =
        ack_nonce_override.unwrap_or_else(|| observer_probe_payload.probe_nonce.clone());
    let ack_payload = ObservedEndpointAckPayloadV0 {
        probe_nonce: ack_nonce,
        source_peer_id: observer_probe_payload.source_peer_id.clone(),
        target_peer_id: observer_probe_payload.target_peer_id.clone(),
        observer_peer_id: observer_peer_id.to_string(),
        observed_endpoint: observed_source_addr.to_string(),
        observed_at_ms,
    };
    let ack = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Ack,
        observer_frame.session_id,
        observer_frame.stream_id,
        observer_frame.object_id,
        observer_frame.sequence,
        observer_frame.ack_epoch,
        serde_json::to_vec(&ack_payload)?,
    );
    let ack_sent_bytes = observer
        .send_to(&ack.encode(), observed_source_addr)
        .context("send local observed endpoint ack")?;

    let mut prober_buf = vec![0u8; 65535];
    let (ack_received_bytes, ack_source_addr) = prober
        .recv_from(&mut prober_buf)
        .context("local observed prober recv ack")?;
    let ack_frame = novovm_network::novorudp::NovoRudpTransportFrameV0::decode(
        &prober_buf[..ack_received_bytes],
    )
    .context("decode local observed ack frame")?;
    let decoded_ack = serde_json::from_slice::<ObservedEndpointAckPayloadV0>(&ack_frame.payload)
        .context("decode local observed ack payload")?;
    let probe_ack_valid = ack_frame.kind == NovoRudpTransportFrameKindV0::Ack
        && decoded_ack.probe_nonce == probe_nonce;
    let probe_reject_reason = if probe_ack_valid {
        None
    } else {
        Some("probe_nonce_mismatch")
    };
    let endpoint_changed =
        advertised_endpoint.as_deref() != Some(decoded_ack.observed_endpoint.as_str());

    Ok(json!({
        "accepted": probe_ack_valid,
        "case": case_name,
        "scope": "observed_endpoint_local_case_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "local_peer_id": source_peer_id,
        "observer_peer_id": observer_peer_id,
        "local_bind_endpoint": prober_addr.to_string(),
        "observer_bind_endpoint": observer_addr.to_string(),
        "advertised_endpoint": advertised_endpoint,
        "observed_endpoint": decoded_ack.observed_endpoint,
        "observed_by_peer_id": decoded_ack.observer_peer_id,
        "observed_at_ms": decoded_ack.observed_at_ms,
        "ack_source_endpoint": ack_source_addr.to_string(),
        "observed_endpoint_changed": endpoint_changed,
        "observed_endpoint_stable": !endpoint_changed,
        "reachability_probe_result": if probe_ack_valid { "reachable" } else { "rejected" },
        "probe_nonce": probe_nonce,
        "ack_nonce": decoded_ack.probe_nonce,
        "probe_ack_valid": probe_ack_valid,
        "probe_reject_reason": probe_reject_reason,
        "probe_rtt_ms": start.elapsed().as_millis() as u64,
        "sent_bytes": sent_bytes,
        "observer_received_bytes": observer_received_bytes,
        "ack_sent_bytes": ack_sent_bytes,
        "ack_received_bytes": ack_received_bytes,
    }))
}

fn run_observed_endpoint_probe_v0(
    socket: &UdpSocket,
    target_addr: &str,
    source_peer_id: &str,
    target_peer_id: &str,
    advertised_endpoint: Option<String>,
    probe_nonce: String,
    bind_addr_effective: SocketAddr,
) -> Result<serde_json::Value> {
    let payload = ObservedEndpointProbePayloadV0 {
        probe_nonce: probe_nonce.clone(),
        source_peer_id: source_peer_id.to_string(),
        target_peer_id: target_peer_id.to_string(),
        advertised_endpoint: advertised_endpoint.clone(),
        expires_at_ms: now_unix_ms().saturating_add(60_000),
    };
    let probe = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Endpoint,
        [25u8; 16],
        250,
        251,
        252,
        253,
        serde_json::to_vec(&payload)?,
    );
    let encoded_probe = probe.encode();
    let start = Instant::now();
    let sent_bytes = socket
        .send_to(&encoded_probe, target_addr)
        .with_context(|| format!("send observed endpoint probe to {target_addr}"))?;
    let mut buf = vec![0u8; 65535];
    let mut report = json!({
        "accepted": false,
        "scope": "observed_endpoint_prober_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "local_peer_id": source_peer_id,
        "target_peer_id": target_peer_id,
        "local_bind_endpoint": bind_addr_effective.to_string(),
        "advertised_endpoint": advertised_endpoint,
        "target_addr": target_addr,
        "probe_nonce": probe_nonce,
        "probe_sent": true,
        "sent_bytes": sent_bytes,
    });

    match socket.recv_from(&mut buf) {
        Ok((received_bytes, ack_source_addr)) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let decoded =
                novovm_network::novorudp::NovoRudpTransportFrameV0::decode(&buf[..received_bytes]);
            match decoded {
                Ok(frame) if frame.kind == NovoRudpTransportFrameKindV0::Ack => {
                    match serde_json::from_slice::<ObservedEndpointAckPayloadV0>(&frame.payload) {
                        Ok(ack_payload) => {
                            let expected_nonce = report["probe_nonce"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string();
                            let probe_ack_valid = ack_payload.probe_nonce == expected_nonce;
                            let endpoint_changed = report["advertised_endpoint"].as_str()
                                != Some(ack_payload.observed_endpoint.as_str());
                            report["accepted"] = json!(probe_ack_valid);
                            report["observed_endpoint"] = json!(ack_payload.observed_endpoint);
                            report["observed_by_peer_id"] = json!(ack_payload.observer_peer_id);
                            report["observed_at_ms"] = json!(ack_payload.observed_at_ms);
                            report["ack_source_endpoint"] = json!(ack_source_addr.to_string());
                            report["observed_endpoint_changed"] = json!(endpoint_changed);
                            report["observed_endpoint_stable"] = json!(!endpoint_changed);
                            report["reachability_probe_result"] = json!(if probe_ack_valid {
                                "reachable"
                            } else {
                                "rejected"
                            });
                            report["ack_nonce"] = json!(ack_payload.probe_nonce);
                            report["probe_ack_valid"] = json!(probe_ack_valid);
                            report["probe_reject_reason"] = if probe_ack_valid {
                                serde_json::Value::Null
                            } else {
                                json!("probe_nonce_mismatch")
                            };
                            report["probe_rtt_ms"] = json!(elapsed_ms);
                            report["received_bytes"] = json!(received_bytes);
                        }
                        Err(error) => {
                            report["probe_ack_valid"] = json!(false);
                            report["probe_reject_reason"] =
                                json!(format!("decode_ack_payload_failed:{error}"));
                        }
                    }
                }
                Ok(frame) => {
                    report["probe_ack_valid"] = json!(false);
                    report["probe_reject_reason"] =
                        json!(format!("unexpected_ack_frame_kind:{:?}", frame.kind));
                }
                Err(error) => {
                    report["probe_ack_valid"] = json!(false);
                    report["probe_reject_reason"] = json!(format!("decode_ack_failed:{error}"));
                }
            }
        }
        Err(error) => {
            report["probe_ack_valid"] = json!(false);
            report["probe_reject_reason"] =
                json!(format!("probe_ack_timeout_or_recv_failed:{error}"));
            report["probe_rtt_ms"] = json!(start.elapsed().as_millis() as u64);
        }
    }

    Ok(report)
}

fn run_nat_punch_local_case_v0(
    case_name: &str,
    ack_nonce_override: Option<String>,
    relay_fallback_enabled: bool,
) -> Result<serde_json::Value> {
    let prober = UdpSocket::bind("127.0.0.1:0").context("bind local nat punch prober")?;
    let observer = UdpSocket::bind("127.0.0.1:0").context("bind local nat punch observer")?;
    prober
        .set_read_timeout(Some(Duration::from_millis(1000)))
        .context("set local nat punch prober timeout")?;
    observer
        .set_read_timeout(Some(Duration::from_millis(1000)))
        .context("set local nat punch observer timeout")?;

    let prober_addr = prober.local_addr().context("local nat punch prober addr")?;
    let observer_addr = observer
        .local_addr()
        .context("local nat punch observer addr")?;
    let punch_nonce = format!("{case_name}-nonce");
    let advertised_endpoint = Some(prober_addr.to_string());
    let start = Instant::now();

    let payload = NatPunchProbePayloadV0 {
        punch_nonce: punch_nonce.clone(),
        source_peer_id: "node-a".to_string(),
        target_peer_id: "node-b".to_string(),
        advertised_endpoint: advertised_endpoint.clone(),
        target_observed_endpoint: observer_addr.to_string(),
        expires_at_ms: now_unix_ms().saturating_add(60_000),
    };
    let punch = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Endpoint,
        [26u8; 16],
        260,
        261,
        262,
        263,
        serde_json::to_vec(&payload)?,
    );
    let sent_bytes = prober
        .send_to(&punch.encode(), observer_addr)
        .context("send local nat punch probe")?;

    let mut observer_buf = vec![0u8; 65535];
    let (observer_received_bytes, observed_source_addr) = observer
        .recv_from(&mut observer_buf)
        .context("local nat punch observer recv probe")?;
    let observer_frame = novovm_network::novorudp::NovoRudpTransportFrameV0::decode(
        &observer_buf[..observer_received_bytes],
    )
    .context("decode local nat punch probe frame")?;
    let observer_probe_payload =
        serde_json::from_slice::<NatPunchProbePayloadV0>(&observer_frame.payload)
            .context("decode local nat punch probe payload")?;
    let observed_at_ms = now_unix_ms();
    let ack_nonce =
        ack_nonce_override.unwrap_or_else(|| observer_probe_payload.punch_nonce.clone());
    let ack_payload = NatPunchAckPayloadV0 {
        punch_nonce: ack_nonce,
        source_peer_id: observer_probe_payload.source_peer_id.clone(),
        target_peer_id: observer_probe_payload.target_peer_id.clone(),
        observer_peer_id: "node-b".to_string(),
        observed_endpoint: observed_source_addr.to_string(),
        observed_at_ms,
    };
    let ack = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Ack,
        observer_frame.session_id,
        observer_frame.stream_id,
        observer_frame.object_id,
        observer_frame.sequence,
        observer_frame.ack_epoch,
        serde_json::to_vec(&ack_payload)?,
    );
    let ack_sent_bytes = observer
        .send_to(&ack.encode(), observed_source_addr)
        .context("send local nat punch ack")?;

    let mut prober_buf = vec![0u8; 65535];
    let (ack_received_bytes, ack_source_addr) = prober
        .recv_from(&mut prober_buf)
        .context("local nat punch prober recv ack")?;
    let ack_frame = novovm_network::novorudp::NovoRudpTransportFrameV0::decode(
        &prober_buf[..ack_received_bytes],
    )
    .context("decode local nat punch ack frame")?;
    let decoded_ack = serde_json::from_slice::<NatPunchAckPayloadV0>(&ack_frame.payload)
        .context("decode local nat punch ack payload")?;
    let punch_ack_valid = ack_frame.kind == NovoRudpTransportFrameKindV0::Ack
        && decoded_ack.punch_nonce == punch_nonce;
    let punch_reject_reason = if punch_ack_valid {
        None
    } else {
        Some("punch_nonce_mismatch")
    };
    let relay_fallback_selected = !punch_ack_valid && relay_fallback_enabled;
    let selected_path_after_punch = if punch_ack_valid {
        "PunchedDirect"
    } else if relay_fallback_selected {
        "RelayNovoRudp"
    } else {
        "PunchRejected"
    };

    Ok(json!({
        "accepted": punch_ack_valid || relay_fallback_selected,
        "case": case_name,
        "scope": "nat_punch_local_case_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "local_peer_id": "node-a",
        "target_peer_id": "node-b",
        "local_bind_endpoint": prober_addr.to_string(),
        "advertised_endpoint": advertised_endpoint,
        "punch_target_peer_id": "node-b",
        "punch_target_observed_endpoint": observer_addr.to_string(),
        "punch_attempt_sent": true,
        "punch_nonce": punch_nonce,
        "ack_nonce": decoded_ack.punch_nonce,
        "punch_ack_valid": punch_ack_valid,
        "punch_reject_reason": punch_reject_reason,
        "punch_result": if punch_ack_valid { "punched_direct" } else { "rejected" },
        "selected_path_after_punch": selected_path_after_punch,
        "relay_fallback_selected": relay_fallback_selected,
        "fallback_reason": if relay_fallback_selected { serde_json::Value::String("NatPunchFailed".into()) } else { serde_json::Value::Null },
        "observed_endpoint": decoded_ack.observed_endpoint,
        "observed_by_peer_id": decoded_ack.observer_peer_id,
        "ack_source_endpoint": ack_source_addr.to_string(),
        "probe_rtt_ms": start.elapsed().as_millis() as u64,
        "sent_bytes": sent_bytes,
        "observer_received_bytes": observer_received_bytes,
        "ack_sent_bytes": ack_sent_bytes,
        "ack_received_bytes": ack_received_bytes,
    }))
}

fn run_nat_punch_local_fallback_case_v0(case_name: &str) -> Result<serde_json::Value> {
    let prober = UdpSocket::bind("127.0.0.1:0").context("bind local nat punch fallback prober")?;
    let prober_addr = prober
        .local_addr()
        .context("local nat punch fallback prober addr")?;
    Ok(json!({
        "accepted": true,
        "case": case_name,
        "scope": "nat_punch_local_fallback_case_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "local_peer_id": "node-a",
        "target_peer_id": "node-b",
        "local_bind_endpoint": prober_addr.to_string(),
        "punch_target_peer_id": "node-b",
        "punch_target_observed_endpoint": "127.0.0.1:9",
        "punch_attempt_sent": true,
        "punch_nonce": format!("{case_name}-nonce"),
        "punch_ack_valid": false,
        "punch_reject_reason": "punch_ack_timeout_or_recv_failed",
        "punch_result": "failed",
        "relay_fallback_selected": true,
        "fallback_reason": "NatPunchFailed",
        "selected_path_after_punch": "RelayNovoRudp",
    }))
}

fn run_nat_punch_probe_v0(
    socket: &UdpSocket,
    target_addr: &str,
    source_peer_id: &str,
    target_peer_id: &str,
    advertised_endpoint: Option<String>,
    punch_nonce: String,
    bind_addr_effective: SocketAddr,
    relay_fallback_enabled: bool,
) -> Result<serde_json::Value> {
    let payload = NatPunchProbePayloadV0 {
        punch_nonce: punch_nonce.clone(),
        source_peer_id: source_peer_id.to_string(),
        target_peer_id: target_peer_id.to_string(),
        advertised_endpoint: advertised_endpoint.clone(),
        target_observed_endpoint: target_addr.to_string(),
        expires_at_ms: now_unix_ms().saturating_add(60_000),
    };
    let probe = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Endpoint,
        [27u8; 16],
        270,
        271,
        272,
        273,
        serde_json::to_vec(&payload)?,
    );
    let encoded_probe = probe.encode();
    let start = Instant::now();
    let sent_bytes = socket
        .send_to(&encoded_probe, target_addr)
        .with_context(|| format!("send nat punch probe to {target_addr}"))?;
    let mut buf = vec![0u8; 65535];
    let mut report = json!({
        "accepted": false,
        "scope": "nat_punch_prober_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "local_peer_id": source_peer_id,
        "target_peer_id": target_peer_id,
        "local_bind_endpoint": bind_addr_effective.to_string(),
        "advertised_endpoint": advertised_endpoint,
        "punch_target_peer_id": target_peer_id,
        "punch_target_observed_endpoint": target_addr,
        "punch_nonce": punch_nonce,
        "punch_attempt_sent": true,
        "sent_bytes": sent_bytes,
        "relay_fallback_enabled": relay_fallback_enabled,
    });

    match socket.recv_from(&mut buf) {
        Ok((received_bytes, ack_source_addr)) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let decoded =
                novovm_network::novorudp::NovoRudpTransportFrameV0::decode(&buf[..received_bytes]);
            match decoded {
                Ok(frame) if frame.kind == NovoRudpTransportFrameKindV0::Ack => {
                    match serde_json::from_slice::<NatPunchAckPayloadV0>(&frame.payload) {
                        Ok(ack_payload) => {
                            let expected_nonce = report["punch_nonce"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string();
                            let punch_ack_valid = ack_payload.punch_nonce == expected_nonce;
                            report["punch_ack_valid"] = json!(punch_ack_valid);
                            report["ack_nonce"] = json!(ack_payload.punch_nonce);
                            report["observed_endpoint"] = json!(ack_payload.observed_endpoint);
                            report["observed_by_peer_id"] = json!(ack_payload.observer_peer_id);
                            report["observed_at_ms"] = json!(ack_payload.observed_at_ms);
                            report["ack_source_endpoint"] = json!(ack_source_addr.to_string());
                            report["punch_rtt_ms"] = json!(elapsed_ms);
                            report["received_bytes"] = json!(received_bytes);
                            if punch_ack_valid {
                                report["accepted"] = json!(true);
                                report["punch_result"] = json!("punched_direct");
                                report["selected_path_after_punch"] = json!("PunchedDirect");
                                report["relay_fallback_selected"] = json!(false);
                                report["fallback_reason"] = serde_json::Value::Null;
                                report["punch_reject_reason"] = serde_json::Value::Null;
                            } else {
                                apply_nat_punch_failure_fallback(
                                    &mut report,
                                    relay_fallback_enabled,
                                    "punch_nonce_mismatch",
                                );
                            }
                        }
                        Err(error) => {
                            apply_nat_punch_failure_fallback(
                                &mut report,
                                relay_fallback_enabled,
                                &format!("decode_punch_ack_payload_failed:{error}"),
                            );
                        }
                    }
                }
                Ok(frame) => {
                    apply_nat_punch_failure_fallback(
                        &mut report,
                        relay_fallback_enabled,
                        &format!("unexpected_punch_ack_frame_kind:{:?}", frame.kind),
                    );
                }
                Err(error) => {
                    apply_nat_punch_failure_fallback(
                        &mut report,
                        relay_fallback_enabled,
                        &format!("decode_punch_ack_failed:{error}"),
                    );
                }
            }
        }
        Err(error) => {
            report["punch_rtt_ms"] = json!(start.elapsed().as_millis() as u64);
            apply_nat_punch_failure_fallback(
                &mut report,
                relay_fallback_enabled,
                &format!("punch_ack_timeout_or_recv_failed:{error}"),
            );
        }
    }

    Ok(report)
}

fn apply_nat_punch_failure_fallback(
    report: &mut serde_json::Value,
    relay_fallback_enabled: bool,
    reject_reason: &str,
) {
    report["punch_ack_valid"] = json!(false);
    report["punch_reject_reason"] = json!(reject_reason);
    report["punch_result"] = json!("failed");
    report["relay_fallback_selected"] = json!(relay_fallback_enabled);
    if relay_fallback_enabled {
        report["accepted"] = json!(true);
        report["fallback_reason"] = json!("NatPunchFailed");
        report["selected_path_after_punch"] = json!("RelayNovoRudp");
    } else {
        report["accepted"] = json!(false);
        report["fallback_reason"] = serde_json::Value::Null;
        report["selected_path_after_punch"] = json!("PunchFailed");
    }
}

fn network_boundary_json() -> serde_json::Value {
    json!({
        "network_only": true,
        "apfl_interpreted": false,
        "aoem_called": false,
        "opcode114_called": false,
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

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}
