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
use serde_json::json;
use std::env;
use std::fs;
use std::path::Path;

fn main() -> Result<()> {
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
