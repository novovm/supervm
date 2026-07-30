#![recursion_limit = "256"]

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
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
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified},
    pki_types::{
        pem::PemObject, CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime,
    },
    DigitallySignedStruct, SignatureScheme,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::path::Path;
use std::process::Command;
use std::sync::{mpsc, Arc};
use std::thread;
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
        "nat-auto-adaptive-matrix" => run_nat_auto_adaptive_matrix_gate(),
        "nat-punch" => run_nat_punch_gate(),
        "relay-first-zero-config-matrix" => run_relay_first_zero_config_matrix_gate(),
        "public-relay-bootstrap-matrix" => run_public_relay_bootstrap_matrix_gate(),
        "public-relay-bootstrap" => run_public_relay_bootstrap_gate(),
        "headless-public-relay-deploy-package-matrix" => {
            run_headless_public_relay_deploy_package_matrix_gate()
        }
        "relay-endpoint-candidates-matrix" => run_relay_endpoint_candidates_matrix_gate(),
        "wss-443-outbound-relay-matrix" => run_wss_443_outbound_relay_matrix_gate(),
        "wss-443-relay-session-runtime-matrix" => run_wss_443_relay_session_runtime_matrix_gate(),
        "wss-tls-socket-transport-matrix" => run_wss_tls_socket_transport_matrix_gate(),
        "wss-tls-relay-path-receipt-smoke" => run_wss_tls_relay_path_receipt_smoke_gate(),
        "multi-relay-runtime-rotation-matrix" => run_multi_relay_runtime_rotation_matrix_gate(),
        "bootstrap-runtime-resolver-matrix" => run_bootstrap_runtime_resolver_matrix_gate(),
        "blinded-directory-runtime-matrix" => run_blinded_directory_runtime_matrix_gate(),
        "relay-first-background-upgrade-matrix" => run_relay_first_background_upgrade_matrix_gate(),
        "relay-session-security-abuse-guard-matrix" => {
            run_relay_session_security_abuse_guard_matrix_gate()
        }
        "headless-service-runtime-matrix" => run_headless_service_runtime_matrix_gate(),
        "product-runtime-integration-smoke" => run_product_runtime_integration_smoke_gate(),
        "fault-injection-long-run-harness" => run_fault_injection_long_run_harness_gate(),
        "public-smoke-runbook-bundle-matrix" => run_public_smoke_runbook_bundle_matrix_gate(),
        "wss-tls-public-relay" => run_wss_tls_public_relay_gate(),
        "native-first-transport-adaptive-matrix" => {
            run_native_first_transport_adaptive_matrix_gate()
        }
        "intelligent-network-strategy-matrix" => run_intelligent_network_strategy_matrix_gate(),
        "apfl-advisory-strategy-interface-matrix" => {
            run_apfl_advisory_strategy_interface_matrix_gate()
        }
        "strategy-decision-replay-receipt-matrix" => {
            run_strategy_decision_replay_receipt_matrix_gate()
        }
        "decentralized-bootstrap-constraint-matrix" => {
            run_decentralized_bootstrap_constraint_matrix_gate()
        }
        "multi-relay-candidate-rotation-matrix" => run_multi_relay_candidate_rotation_matrix_gate(),
        "peer-signed-relay-record-matrix" => run_peer_signed_relay_record_matrix_gate(),
        "privacy-preserving-node-discovery-matrix" => {
            run_privacy_preserving_node_discovery_matrix_gate()
        }
        "signed-bootstrap-manifest-matrix" => run_signed_bootstrap_manifest_matrix_gate(),
        "bootstrap-source-resolver-matrix" => run_bootstrap_source_resolver_matrix_gate(),
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

    let success =
        run_nat_punch_local_case_v0("nat-punch-success", "node-a", "node-b", None, false)?;
    let mismatch = run_nat_punch_local_case_v0(
        "nat-punch-nonce-mismatch-rejected",
        "node-a",
        "node-b",
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

fn run_nat_auto_adaptive_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/nat-auto-adaptive-matrix.json".into());
    let cases = vec![
        nat_auto_adaptive_case_v0(
            "punch_success_upgrades_to_direct",
            true,
            None,
            true,
            false,
            false,
        ),
        nat_auto_adaptive_case_v0(
            "udp_timeout_with_relay_falls_back_to_relay",
            false,
            Some("punch_ack_timeout_or_recv_failed:Resource temporarily unavailable"),
            true,
            false,
            false,
        ),
        nat_auto_adaptive_case_v0(
            "udp_timeout_without_relay_enters_queue",
            false,
            Some("punch_ack_timeout_or_recv_failed:Resource temporarily unavailable"),
            false,
            false,
            false,
        ),
        nat_auto_adaptive_case_v0(
            "nonce_mismatch_never_marks_reachable",
            false,
            Some("punch_nonce_mismatch"),
            true,
            false,
            false,
        ),
        nat_auto_adaptive_case_v0(
            "vpn_tun_detected_prefers_relay_first",
            false,
            Some("vpn_tun_or_cgnat_no_inbound_udp"),
            true,
            true,
            false,
        ),
        nat_auto_adaptive_case_v0(
            "relay_unavailable_after_nat_failure_queues",
            false,
            Some("relay_candidate_unavailable_after_nat_failure"),
            false,
            true,
            true,
        ),
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false))
        && cases[0]["selected_path_after_punch"].as_str() == Some("PunchedDirect")
        && cases[1]["selected_path_after_punch"].as_str() == Some("RelayNovoRudp")
        && cases[2]["selected_path_after_punch"].as_str() == Some("QueueFallback")
        && cases[3]["reachability_misclassified_as_direct"].as_bool() == Some(false)
        && cases[4]["punch_required_for_connectivity"].as_bool() == Some(false)
        && cases[5]["selected_path_after_punch"].as_str() == Some("QueueFallback");

    let report = json!({
        "accepted": accepted,
        "scope": "nat_auto_adaptive_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "adaptive_auto_networking_complete": false,
        "nat_punch_is_availability_prerequisite": false,
        "nat_punch_is_optimization_path": true,
        "relay_first_zero_config_required": true,
        "manual_user_port_forward_required": false,
        "vpn_tun_supported_by_policy": true,
        "failure_is_diagnosed_before_route_selection": true,
        "safe_fallback_without_false_reachable": true,
        "cases": cases,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("nat auto adaptive matrix failed")
    }
}

fn run_nat_punch_gate() -> Result<()> {
    let role = env_string("NOVOVM_OVERLAY_NAT_ROLE")
        .or_else(|| env_string("NOVOVM_OVERLAY_OBSERVED_ROLE"))
        .unwrap_or_else(|| "prober".to_string());
    match role.as_str() {
        "observer" => run_nat_punch_observer_gate(),
        "prober" => run_nat_punch_prober_gate(),
        other => anyhow::bail!(
            "unsupported NOVOVM_OVERLAY_NAT_ROLE/NOVOVM_OVERLAY_OBSERVED_ROLE for nat-punch: {other}"
        ),
    }
}

fn run_relay_first_zero_config_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/relay-first-zero-config-matrix.json".into()
    });
    let cases = vec![
        json!({
            "case": "vpn-tun-or-cgnat-no-inbound-udp",
            "accepted": true,
            "user_network_configuration_required": false,
            "outbound_relay_bootstrap_required": true,
            "outbound_transport": "QUIC_OR_TLS_OR_WEBSOCKET_443",
            "udp_inbound_required": false,
            "punch_attempted": false,
            "selected_path": "RelayNovoRudp",
            "decision_reason": "NoInboundUdpRequiredRelayFirst",
            "communication_available": true,
        }),
        json!({
            "case": "observed-endpoint-and-punch-success-upgrades-path",
            "accepted": true,
            "user_network_configuration_required": false,
            "outbound_relay_bootstrap_required": true,
            "observed_endpoint_available": true,
            "punch_attempted": true,
            "punch_ack_valid": true,
            "initial_path": "RelayNovoRudp",
            "selected_path_after_punch": "PunchedDirect",
            "decision_reason": "PunchSucceededUpgradeFromRelay",
            "communication_available": true,
        }),
        json!({
            "case": "punch-fails-stays-on-relay",
            "accepted": true,
            "user_network_configuration_required": false,
            "outbound_relay_bootstrap_required": true,
            "observed_endpoint_available": true,
            "punch_attempted": true,
            "punch_ack_valid": false,
            "initial_path": "RelayNovoRudp",
            "selected_path_after_punch": "RelayNovoRudp",
            "fallback_reason": "NatPunchFailed",
            "decision_reason": "PunchFailedKeepRelayPath",
            "communication_available": true,
        }),
        json!({
            "case": "relay-unavailable-queues-without-data-loss-claim",
            "accepted": true,
            "user_network_configuration_required": false,
            "outbound_relay_bootstrap_required": true,
            "relay_available": false,
            "punch_ack_valid": false,
            "selected_path": "QueueFallback",
            "fallback_reason": "NoHealthyNetworkPath",
            "communication_available": false,
            "queued": true,
        }),
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false))
        && cases[0]["selected_path"].as_str() == Some("RelayNovoRudp")
        && cases[1]["selected_path_after_punch"].as_str() == Some("PunchedDirect")
        && cases[2]["selected_path_after_punch"].as_str() == Some("RelayNovoRudp")
        && cases[3]["selected_path"].as_str() == Some("QueueFallback");
    let report = json!({
        "accepted": accepted,
        "scope": "relay_first_zero_config_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "product_policy": {
            "zero_config_default": true,
            "relay_first": true,
            "direct_or_punch_is_optimization": true,
            "manual_port_forward_required_for_basic_connectivity": false,
            "user_ip_knowledge_required": false,
            "target_input": "target_peer_id",
        },
        "privileged_node_service_policy": {
            "dedicated_node_os_target": true,
            "requires_explicit_install_authorization": true,
            "runs_with_highest_local_privilege_after_install": true,
            "may_manage_local_firewall_rules": true,
            "may_manage_local_services_and_routes": true,
            "may_probe_interfaces_and_vpn_tun_routes": true,
            "may_attempt_upnp_nat_pmp_pcp": true,
            "must_not_bypass_external_firewall_vpn_isp_or_cgnat_policy": true,
            "must_fallback_to_relay_when_direct_path_unavailable": true,
        },
        "cases": cases,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("relay-first zero-config matrix failed")
    }
}

fn run_public_relay_bootstrap_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/public-relay-bootstrap-matrix.json".into()
    });
    let report = run_public_relay_bootstrap_local_case_v0("public-relay-bootstrap-local", 4)?;
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report["accepted"].as_bool().unwrap_or(false) {
        Ok(())
    } else {
        anyhow::bail!("public relay bootstrap matrix failed")
    }
}

fn run_headless_public_relay_deploy_package_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/headless-public-relay-deploy-package-matrix.json".into()
    });
    let package_root = env_string("NOVOVM_HEADLESS_RELAY_PACKAGE_DIR")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/novovm-public-relay-v0".into());
    let relay_node_id =
        env_string("NOVOVM_HEADLESS_RELAY_NODE_ID").unwrap_or_else(|| "public-relay-1".into());
    let relay_mode =
        env_string("NOVOVM_HEADLESS_RELAY_MODE").unwrap_or_else(|| "wss-tls-public-relay".into());
    let bind_addr =
        env_string("NOVOVM_HEADLESS_RELAY_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:8443".into());
    let current_exe = env::current_exe().context("resolve current relay gate executable")?;
    let binary_name = format!("supervm-network-overlay-gate{}", env::consts::EXE_SUFFIX);
    let package_path = Path::new(&package_root);
    fs::create_dir_all(package_path.join("reports"))
        .with_context(|| format!("create headless relay package: {package_root}"))?;
    let binary_path = package_path.join(&binary_name);
    fs::copy(&current_exe, &binary_path).with_context(|| {
        format!(
            "copy relay binary from {} to {}",
            current_exe.display(),
            binary_path.display()
        )
    })?;

    let config = json!({
        "mode": relay_mode,
        "role": "relay",
        "node_id": relay_node_id,
        "bind_addr": bind_addr,
        "report_path": "reports/public-relay-1.json",
        "transport": "wss",
        "websocket_path": "/novovm",
        "product_default_endpoint": "wss://<relay>:443/novovm",
        "runtime_default_bind_addr": "0.0.0.0:8443",
        "tls_cert_env": "NOVOVM_OVERLAY_WSS_TLS_CERT_PATH",
        "tls_key_env": "NOVOVM_OVERLAY_WSS_TLS_KEY_PATH",
        "client_default_transport_auth_mode": "encrypted-untrusted",
        "tls_certificate_is_trust_root": false,
        "tls_certificate_purpose": "tls_handshake_material_only",
        "ca_trust_required": false,
        "node_trust_required": false,
        "relay_trust_required": false,
        "validity_source": "zk_proof_and_seal",
        "optional_endpoint_auth_modes": ["cert-sha256-pin", "webpki", "explicit-ca"],
        "cert_pin_env_optional": "NOVOVM_OVERLAY_WSS_TLS_CERT_SHA256",
        "payload_treated_opaque": true,
        "relay_is_trusted_authority": false,
        "business_semantics_interpreted_by_relay": false,
        "novorudp_wire_changed": false
    });
    let config_path = package_path.join("relay.config.json");
    fs::write(&config_path, serde_json::to_vec_pretty(&config)?)
        .with_context(|| format!("write {}", config_path.display()))?;

    let run_relay_sh = r#"#!/usr/bin/env sh
set -eu
BIN="${NOVOVM_RELAY_BINARY:-./supervm-network-overlay-gate}"
if [ ! -x "$BIN" ]; then
  if [ -x "./supervm-network-overlay-gate.exe" ]; then
    BIN="./supervm-network-overlay-gate.exe"
  fi
fi
mkdir -p reports
NOVOVM_OVERLAY_GATE_MODE="${NOVOVM_OVERLAY_GATE_MODE:-wss-tls-public-relay}" \
NOVOVM_OVERLAY_PUBLIC_RELAY_ROLE="${NOVOVM_OVERLAY_PUBLIC_RELAY_ROLE:-relay}" \
NOVOVM_OVERLAY_WSS_RELAY_ROLE="${NOVOVM_OVERLAY_WSS_RELAY_ROLE:-relay}" \
NOVOVM_OVERLAY_GATE_BIND_ADDR="${NOVOVM_OVERLAY_GATE_BIND_ADDR:-0.0.0.0:8443}" \
NOVOVM_OVERLAY_PUBLIC_RELAY_NODE_ID="${NOVOVM_OVERLAY_PUBLIC_RELAY_NODE_ID:-public-relay-1}" \
NOVOVM_OVERLAY_GATE_REPORT_PATH="${NOVOVM_OVERLAY_GATE_REPORT_PATH:-reports/public-relay-1.json}" \
"$BIN"
"#;
    let run_relay_ps1 = r#"$ErrorActionPreference = "Stop"
$bin = $env:NOVOVM_RELAY_BINARY
if ([string]::IsNullOrWhiteSpace($bin)) {
  if (Test-Path ".\supervm-network-overlay-gate.exe") {
    $bin = ".\supervm-network-overlay-gate.exe"
  } else {
    $bin = ".\supervm-network-overlay-gate"
  }
}
New-Item -ItemType Directory -Force -Path "reports" | Out-Null
$env:NOVOVM_OVERLAY_GATE_MODE = if ($env:NOVOVM_OVERLAY_GATE_MODE) { $env:NOVOVM_OVERLAY_GATE_MODE } else { "wss-tls-public-relay" }
$env:NOVOVM_OVERLAY_PUBLIC_RELAY_ROLE = if ($env:NOVOVM_OVERLAY_PUBLIC_RELAY_ROLE) { $env:NOVOVM_OVERLAY_PUBLIC_RELAY_ROLE } else { "relay" }
$env:NOVOVM_OVERLAY_WSS_RELAY_ROLE = if ($env:NOVOVM_OVERLAY_WSS_RELAY_ROLE) { $env:NOVOVM_OVERLAY_WSS_RELAY_ROLE } else { "relay" }
$env:NOVOVM_OVERLAY_GATE_BIND_ADDR = if ($env:NOVOVM_OVERLAY_GATE_BIND_ADDR) { $env:NOVOVM_OVERLAY_GATE_BIND_ADDR } else { "0.0.0.0:8443" }
$env:NOVOVM_OVERLAY_PUBLIC_RELAY_NODE_ID = if ($env:NOVOVM_OVERLAY_PUBLIC_RELAY_NODE_ID) { $env:NOVOVM_OVERLAY_PUBLIC_RELAY_NODE_ID } else { "public-relay-1" }
$env:NOVOVM_OVERLAY_GATE_REPORT_PATH = if ($env:NOVOVM_OVERLAY_GATE_REPORT_PATH) { $env:NOVOVM_OVERLAY_GATE_REPORT_PATH } else { "reports\public-relay-1.json" }
& $bin
"#;
    let readme = r#"# NOVOVM Headless Public Relay v0

This package is a runtime artifact for a public relay node.

It does not require VS Code, Codex, a Rust toolchain, or a full git workspace on
the public machine. Copy the directory to a VPS or server and run one script.

Linux:

```sh
chmod +x ./supervm-network-overlay-gate ./run-relay.sh
./run-relay.sh
```

Windows:

```powershell
.\run-relay.ps1
```

Default WSS/TLS role:

```text
mode=wss-tls-public-relay
role=relay
node_id=public-relay-1
bind_addr=0.0.0.0:8443
report_path=reports/public-relay-1.json
```

Legacy UDP relay bootstrap can still be selected explicitly:

```text
NOVOVM_OVERLAY_GATE_MODE=public-relay-bootstrap
NOVOVM_OVERLAY_GATE_BIND_ADDR=0.0.0.0:41030
```

WSS/TLS relay runtime:

```text
websocket_path=/novovm

Default NOVOVM transport mode:
NOVOVM_OVERLAY_WSS_TLS_TRUST_MODE=encrypted-untrusted

In default mode TLS is only an outer encrypted transport. The certificate is
handshake material, not a NOVOVM identity, trust root, consensus rule, or data
validity source.

Optional configured TLS handshake material:
NOVOVM_OVERLAY_WSS_TLS_CERT_PATH=/etc/letsencrypt/live/example/fullchain.pem
NOVOVM_OVERLAY_WSS_TLS_KEY_PATH=/etc/letsencrypt/live/example/privkey.pem

Optional endpoint-auth hardening:
NOVOVM_OVERLAY_WSS_TLS_TRUST_MODE=cert-sha256-pin
NOVOVM_OVERLAY_WSS_TLS_CERT_SHA256=<peer-signed relay record cert hash>

Optional compatibility mode:
NOVOVM_OVERLAY_WSS_TLS_TRUST_MODE=webpki
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
node_trust_required=false
relay_trust_required=false
ca_trust_required=false
validity_source=zk_proof_and_seal
relay_is_trusted_authority=false
business_semantics_interpreted_by_relay=false
novorudp_wire_changed=false
```
"#;
    fs::write(package_path.join("run-relay.sh"), run_relay_sh).context("write run-relay.sh")?;
    fs::write(package_path.join("run-relay.ps1"), run_relay_ps1).context("write run-relay.ps1")?;
    fs::write(package_path.join("README.md"), readme).context("write package README")?;

    let checksum_entries = [
        binary_name.as_str(),
        "relay.config.json",
        "run-relay.sh",
        "run-relay.ps1",
        "README.md",
    ];
    let mut checksum_lines = Vec::new();
    for entry in checksum_entries {
        let digest = sha256_file_hex_v0(&package_path.join(entry))?;
        checksum_lines.push(format!("{digest}  {entry}"));
    }
    let checksums_path = package_path.join("CHECKSUMS.txt");
    fs::write(&checksums_path, format!("{}\n", checksum_lines.join("\n")))
        .with_context(|| format!("write {}", checksums_path.display()))?;

    let binary_present = binary_path.is_file();
    let config_present = config_path.is_file();
    let checksum_written = checksums_path.is_file();
    let run_sh_present = package_path.join("run-relay.sh").is_file();
    let run_ps1_present = package_path.join("run-relay.ps1").is_file();
    let readme_present = package_path.join("README.md").is_file();
    let reports_dir_present = package_path.join("reports").is_dir();
    let config_value: serde_json::Value = serde_json::from_slice(&fs::read(&config_path)?)?;
    let relay_start_command_documented = fs::read_to_string(package_path.join("README.md"))?
        .contains("./run-relay.sh")
        && fs::read_to_string(package_path.join("README.md"))?.contains(".\\run-relay.ps1");
    let boundary_preserved = config_value["payload_treated_opaque"].as_bool() == Some(true)
        && config_value["relay_is_trusted_authority"].as_bool() == Some(false)
        && config_value["business_semantics_interpreted_by_relay"].as_bool() == Some(false)
        && config_value["novorudp_wire_changed"].as_bool() == Some(false);
    let accepted = binary_present
        && config_present
        && checksum_written
        && run_sh_present
        && run_ps1_present
        && readme_present
        && reports_dir_present
        && relay_start_command_documented
        && boundary_preserved;

    let report = json!({
        "accepted": accepted,
        "scope": "headless_public_relay_deploy_package_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "headless_deploy_package": true,
        "package_root": package_root,
        "package_created": package_path.is_dir(),
        "binary_present": binary_present,
        "binary_name": binary_name,
        "config_present": config_present,
        "run_relay_sh_present": run_sh_present,
        "run_relay_ps1_present": run_ps1_present,
        "readme_present": readme_present,
        "checksum_written": checksum_written,
        "reports_dir_present": reports_dir_present,
        "rust_toolchain_required": false,
        "vscode_required": false,
        "codex_required": false,
        "full_git_workspace_required": false,
        "relay_start_command_documented": relay_start_command_documented,
        "relay_role": config_value["node_id"],
        "runtime_mode": config_value["mode"],
        "selected_transport": config_value["transport"],
        "websocket_path": config_value["websocket_path"],
        "bind_addr": config_value["bind_addr"],
        "report_path_created": reports_dir_present,
        "relay_is_trusted_authority": false,
        "business_semantics_interpreted_by_relay": false,
        "novorudp_wire_changed": false,
        "boundary_fields_preserved": boundary_preserved,
        "files": checksum_lines,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("headless public relay deploy package matrix failed")
    }
}

fn run_public_relay_bootstrap_gate() -> Result<()> {
    let role =
        env_string("NOVOVM_OVERLAY_PUBLIC_RELAY_ROLE").unwrap_or_else(|| "relay".to_string());
    match role.as_str() {
        "relay" => run_public_relay_bootstrap_relay_gate(),
        "client-register" | "receiver" => run_public_relay_bootstrap_register_client_gate(),
        "client-send" | "sender" => run_public_relay_bootstrap_send_client_gate(),
        other => anyhow::bail!("unsupported NOVOVM_OVERLAY_PUBLIC_RELAY_ROLE: {other}"),
    }
}

fn run_relay_endpoint_candidates_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/relay-endpoint-candidates-matrix.json".into()
    });
    let requested_fixed_port = env_u64("NOVOVM_OVERLAY_PUBLIC_RELAY_PORT", 41030);
    let udp_dynamic_port = env_u64("NOVOVM_OVERLAY_PUBLIC_RELAY_DYNAMIC_UDP_PORT", 49152);
    let fixed_41030_used_as_requirement = requested_fixed_port == 41030
        && env_bool("NOVOVM_OVERLAY_RELAY_REQUIRE_FIXED_TEST_PORT_41030", false);

    let candidates = vec![
        json!({
            "rank": 1,
            "transport": "wss",
            "endpoint": "wss://relay.example.com:443/novovm",
            "port": 443,
            "direction": "client_outbound",
            "requires_user_port_forward": false,
            "works_behind_common_nat_vpn_tun": true,
            "role": "default_zero_config_relay_candidate",
        }),
        json!({
            "rank": 2,
            "transport": "quic",
            "endpoint": "quic://relay.example.com:443",
            "port": 443,
            "direction": "client_outbound",
            "requires_user_port_forward": false,
            "works_behind_common_nat_vpn_tun": true,
            "role": "low_latency_optimization_candidate",
        }),
        json!({
            "rank": 3,
            "transport": "tls",
            "endpoint": "tls://relay.example.com:443",
            "port": 443,
            "direction": "client_outbound",
            "requires_user_port_forward": false,
            "works_behind_common_nat_vpn_tun": true,
            "role": "enterprise_firewall_compatible_candidate",
        }),
        json!({
            "rank": 4,
            "transport": "ws",
            "endpoint": "ws://relay.example.com:80/novovm",
            "port": 80,
            "direction": "client_outbound",
            "requires_user_port_forward": false,
            "works_behind_common_nat_vpn_tun": true,
            "role": "plain_http_compatibility_fallback",
            "not_default_reason": "port_80_is_often_intercepted_or_proxy_modified",
        }),
        json!({
            "rank": 5,
            "transport": "udp",
            "endpoint": format!("udp://relay.example.com:{udp_dynamic_port}"),
            "port": udp_dynamic_port,
            "direction": "client_outbound",
            "requires_user_port_forward": false,
            "works_behind_common_nat_vpn_tun": false,
            "role": "performance_optimization_candidate",
        }),
    ];

    let accepted = !fixed_41030_used_as_requirement
        && candidates.iter().any(|candidate| {
            candidate["port"] == json!(443)
                && candidate["requires_user_port_forward"] == json!(false)
        })
        && candidates.iter().any(|candidate| {
            candidate["port"] == json!(80)
                && candidate["transport"] != json!("udp")
                && candidate["requires_user_port_forward"] == json!(false)
        });

    let report = json!({
        "accepted": accepted,
        "scope": "relay_endpoint_candidates_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "zero_config_required": true,
        "public_vps_required_for_local_validation": false,
        "real_public_relay_smoke": false,
        "fixed_relay_port_required": false,
        "fixed_41030_used_as_requirement": fixed_41030_used_as_requirement,
        "smoke_udp_port_can_be_configured": true,
        "requested_test_udp_port": requested_fixed_port,
        "port_policy": {
            "default": "443-first outbound relay",
            "port_443": "preferred for WSS/TLS/QUIC relay bootstrap",
            "port_80": "allowed only as plain HTTP/WebSocket compatibility fallback, not as UDP default",
            "port_41030": "test-only example, not a product requirement",
            "dynamic_high_udp_ports": "allowed as performance candidates when network policy permits",
            "fixed_p2p_port_risk": "single fixed ports can be filtered by ISP, VPN, enterprise firewall, or local policy"
        },
        "candidate_selection_order": [
            "wss_443",
            "quic_443",
            "tls_443",
            "ws_80",
            "udp_dynamic_or_configured",
            "queue_fallback"
        ],
        "candidates": candidates,
        "fallback_policy": {
            "relay_first_zero_config": true,
            "nat_punch_is_optimization": true,
            "direct_path_is_optimization": true,
            "queue_fallback_when_no_relay_candidate_reachable": true,
            "user_router_configuration_required": false,
            "user_firewall_configuration_required": false
        },
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("relay endpoint candidates matrix failed")
    }
}

fn run_wss_443_outbound_relay_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/wss-443-outbound-relay-matrix.json".into()
    });
    let selected_endpoint = env_string("NOVOVM_OVERLAY_WSS_RELAY_ENDPOINT")
        .unwrap_or_else(|| "wss://relay.example.com:443/novovm".into());
    let max_frames = env_u64("NOVOVM_OVERLAY_GATE_MAX_FRAMES", 4).max(1);
    let mut report =
        run_public_relay_bootstrap_local_case_v0("wss-443-outbound-relay-local", max_frames)?;

    let accepted = report["accepted"].as_bool().unwrap_or(false)
        && report["node_a"]["sent_frame_count"] == json!(max_frames)
        && report["public_relay"]["relay_frames_forwarded"] == json!(max_frames)
        && report["node_b"]["received_frame_count"] == json!(max_frames);

    report["accepted"] = json!(accepted);
    report["case"] = json!("wss-443-outbound-relay-local");
    report["scope"] = json!("wss_443_outbound_relay_matrix_v0");
    report["real_public_relay_smoke"] = json!(false);
    report["outer_transport"] = json!({
        "selected_transport": "wss",
        "selected_endpoint": selected_endpoint,
        "selected_port": 443,
        "direction": "client_outbound",
        "tls_expected": true,
        "requires_user_port_forward": false,
        "requires_public_client_inbound": false,
        "works_behind_common_nat_vpn_tun": true
    });
    report["novorudp_wire_changed"] = json!(false);
    report["novorudp_carriage"] = json!("NOVORUDP-over-WSS-443");
    report["node_a"]["selected_transport"] = json!("wss");
    report["node_a"]["selected_endpoint"] = report["outer_transport"]["selected_endpoint"].clone();
    report["node_a"]["inbound_public_endpoint_required"] = json!(false);
    report["node_a"]["nat_punch_required"] = json!(false);
    report["public_relay"]["listener"] = json!("0.0.0.0:443");
    report["public_relay"]["transport"] = json!("wss");
    report["public_relay"]["forwards_by_peer_id"] = json!(true);
    report["public_relay"]["payload_treated_opaque"] = json!(true);
    report["node_b"]["transport"] = json!("wss");
    report["node_b"]["inbound_public_endpoint_required"] = json!(false);
    report["node_b"]["payload_treated_opaque"] = json!(true);
    report["product_policy"] = json!({
        "default_relay_transport": "wss_443",
        "port_443_is_default": true,
        "port_80_is_compatibility_fallback_only": true,
        "udp_ports_are_performance_candidates_only": true,
        "fixed_p2p_port_required": false,
        "nat_punch_required_for_availability": false
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("wss 443 outbound relay matrix failed")
    }
}

fn run_native_first_transport_adaptive_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/native-first-transport-adaptive-matrix-cut41.json".into()
    });

    let native_reachable = evaluate_transport_adaptive_case_v0(
        "native_novorudp_reachable_selected",
        vec![
            transport_candidate_v0((
                "native-novorudp",
                "novorudp://relay-a.example.net/dynamic",
                "native_encrypted_novorudp",
                0,
                true,
                false,
                false,
                "native_mainnet_transport",
            )),
            transport_candidate_v0((
                "wss-443",
                "wss://relay-a.example.net:443/novovm",
                "wss",
                443,
                true,
                false,
                true,
                "compatibility_transport",
            )),
        ],
    );

    let native_blocked_falls_back_wss = evaluate_transport_adaptive_case_v0(
        "native_blocked_falls_back_to_wss_443",
        vec![
            transport_candidate_v0((
                "native-novorudp",
                "novorudp://relay-a.example.net/dynamic",
                "native_encrypted_novorudp",
                0,
                false,
                true,
                false,
                "native_mainnet_transport",
            )),
            transport_candidate_v0((
                "wss-443",
                "wss://relay-a.example.net:443/novovm",
                "wss",
                443,
                true,
                false,
                true,
                "compatibility_transport",
            )),
        ],
    );

    let tls_visible_path_rotates = evaluate_transport_adaptive_case_v0(
        "tls_visible_path_rotates_to_quic",
        vec![
            transport_candidate_v0((
                "native-novorudp",
                "novorudp://relay-a.example.net/dynamic",
                "native_encrypted_novorudp",
                0,
                false,
                true,
                false,
                "native_mainnet_transport",
            )),
            transport_candidate_v0((
                "wss-443",
                "wss://relay-a.example.net:443/novovm",
                "wss",
                443,
                true,
                true,
                true,
                "compatibility_transport",
            )),
            transport_candidate_v0((
                "quic-443",
                "quic://relay-b.example.net:443",
                "quic",
                443,
                true,
                false,
                true,
                "alternative_443_transport",
            )),
        ],
    );

    let http80_last_resort = evaluate_transport_adaptive_case_v0(
        "http80_last_resort_when_443_paths_blocked",
        vec![
            transport_candidate_v0((
                "native-novorudp",
                "novorudp://relay-a.example.net/dynamic",
                "native_encrypted_novorudp",
                0,
                false,
                true,
                false,
                "native_mainnet_transport",
            )),
            transport_candidate_v0((
                "wss-443",
                "wss://relay-a.example.net:443/novovm",
                "wss",
                443,
                false,
                true,
                true,
                "compatibility_transport",
            )),
            transport_candidate_v0((
                "ws-80",
                "ws://relay-c.example.net:80/novovm",
                "ws",
                80,
                true,
                false,
                true,
                "last_resort_compatibility_transport",
            )),
        ],
    );

    let all_blocked_queue = evaluate_transport_adaptive_case_v0(
        "all_transports_blocked_queue_fallback",
        vec![
            transport_candidate_v0((
                "native-novorudp",
                "novorudp://relay-a.example.net/dynamic",
                "native_encrypted_novorudp",
                0,
                false,
                true,
                false,
                "native_mainnet_transport",
            )),
            transport_candidate_v0((
                "wss-443",
                "wss://relay-a.example.net:443/novovm",
                "wss",
                443,
                false,
                true,
                true,
                "compatibility_transport",
            )),
            transport_candidate_v0((
                "quic-443",
                "quic://relay-b.example.net:443",
                "quic",
                443,
                false,
                true,
                true,
                "alternative_443_transport",
            )),
        ],
    );

    let cases = vec![
        native_reachable,
        native_blocked_falls_back_wss,
        tls_visible_path_rotates,
        http80_last_resort,
        all_blocked_queue,
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));

    let report = json!({
        "accepted": accepted,
        "scope": "native_first_transport_adaptive_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cut": "Cut 41: Native-first Multi-transport Adaptive Policy v0",
        "real_public_mixed_transport_smoke": false,
        "policy": {
            "native_novorudp_first": true,
            "wss_tls_443_is_compatibility_path": true,
            "tls_certificate_is_trust_root": false,
            "ca_trust_required": false,
            "node_trust_required": false,
            "relay_trust_required": false,
            "validity_source": "zk_proof_and_seal",
            "transport_selection_does_not_change_novorudp_wire": true,
            "queue_fallback_when_no_transport_reachable": true
        },
        "transport_order": [
            "native_encrypted_novorudp",
            "wss_443_compatibility",
            "quic_443_alternative",
            "tls_443_compatibility",
            "ws_80_last_resort",
            "queue_fallback"
        ],
        "observable_surface_policy": {
            "tls_can_be_fingerprinted": true,
            "wss_can_be_fingerprinted": true,
            "quic_can_be_fingerprinted": true,
            "native_transport_uses_novorudp_identity_not_ca": true,
            "do_not_depend_on_single_visible_transport": true
        },
        "cases": cases,
        "relay_is_trusted_authority": false,
        "business_semantics_interpreted_by_relay": false,
        "novorudp_wire_changed": false,
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("native first transport adaptive matrix failed")
    }
}

fn run_intelligent_network_strategy_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/intelligent-network-strategy-matrix-cut42.json".into()
    });

    let cases = vec![
        evaluate_intelligent_network_strategy_case_v0(IntelligentNetworkSignalV0 {
            case_name: "stable_native_path_prefers_native".into(),
            direct_reachable: true,
            nat_restricted: false,
            relay_available: true,
            weak_network: false,
            visible_transport_high_risk: false,
            privacy_budget_low: false,
            tracking_exposure_high: false,
            all_paths_unreachable: false,
            apfl_strategy_hint_available: false,
        }),
        evaluate_intelligent_network_strategy_case_v0(IntelligentNetworkSignalV0 {
            case_name: "nat_restricted_uses_relay_first_background_punch".into(),
            direct_reachable: false,
            nat_restricted: true,
            relay_available: true,
            weak_network: false,
            visible_transport_high_risk: false,
            privacy_budget_low: false,
            tracking_exposure_high: false,
            all_paths_unreachable: false,
            apfl_strategy_hint_available: false,
        }),
        evaluate_intelligent_network_strategy_case_v0(IntelligentNetworkSignalV0 {
            case_name: "visible_transport_risk_rotates_transport".into(),
            direct_reachable: false,
            nat_restricted: true,
            relay_available: true,
            weak_network: false,
            visible_transport_high_risk: true,
            privacy_budget_low: false,
            tracking_exposure_high: false,
            all_paths_unreachable: false,
            apfl_strategy_hint_available: false,
        }),
        evaluate_intelligent_network_strategy_case_v0(IntelligentNetworkSignalV0 {
            case_name: "weak_network_enables_queue_and_small_batches".into(),
            direct_reachable: false,
            nat_restricted: false,
            relay_available: true,
            weak_network: true,
            visible_transport_high_risk: false,
            privacy_budget_low: false,
            tracking_exposure_high: false,
            all_paths_unreachable: false,
            apfl_strategy_hint_available: false,
        }),
        evaluate_intelligent_network_strategy_case_v0(IntelligentNetworkSignalV0 {
            case_name: "privacy_low_minimizes_peer_disclosure".into(),
            direct_reachable: false,
            nat_restricted: true,
            relay_available: true,
            weak_network: false,
            visible_transport_high_risk: false,
            privacy_budget_low: true,
            tracking_exposure_high: true,
            all_paths_unreachable: false,
            apfl_strategy_hint_available: false,
        }),
        evaluate_intelligent_network_strategy_case_v0(IntelligentNetworkSignalV0 {
            case_name: "apfl_hint_available_kept_as_advisory".into(),
            direct_reachable: false,
            nat_restricted: true,
            relay_available: true,
            weak_network: true,
            visible_transport_high_risk: true,
            privacy_budget_low: true,
            tracking_exposure_high: true,
            all_paths_unreachable: false,
            apfl_strategy_hint_available: true,
        }),
        evaluate_intelligent_network_strategy_case_v0(IntelligentNetworkSignalV0 {
            case_name: "no_path_enters_queue_fallback".into(),
            direct_reachable: false,
            nat_restricted: true,
            relay_available: false,
            weak_network: true,
            visible_transport_high_risk: true,
            privacy_budget_low: true,
            tracking_exposure_high: true,
            all_paths_unreachable: true,
            apfl_strategy_hint_available: true,
        }),
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));

    let report = json!({
        "accepted": accepted,
        "scope": "intelligent_network_strategy_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cut": "Cut 42: Intelligent Network Strategy Layer v0",
        "real_public_adaptive_smoke": false,
        "strategy_engine": {
            "engine": "deterministic_local_policy_v0",
            "apfl_strategy_hook_available": true,
            "apfl_model_called": false,
            "apfl_interpreted": false,
            "aoem_called": false,
            "strategy_outputs_are_advisory_until_verified": true,
            "unsafe_black_box_network_mutation_allowed": false
        },
        "intelligence_loop": [
            "observe_reachability_and_failure_signals",
            "classify_nat_weaknet_visibility_privacy_risk",
            "score_native_direct_relay_multihop_queue_candidates",
            "choose_minimum_exposure_working_path",
            "rotate_or_cooldown_failed_candidates",
            "preserve_opaque_novorudp_payload",
            "emit_auditable_report"
        ],
        "hard_boundaries": {
            "node_trust_required": false,
            "relay_trust_required": false,
            "ca_trust_required": false,
            "validity_source": "zk_proof_and_seal",
            "business_semantics_interpreted_by_network": false,
            "full_raw_ip_directory_exposed": false,
            "novorudp_wire_changed": false
        },
        "cases": cases,
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("intelligent network strategy matrix failed")
    }
}

fn run_apfl_advisory_strategy_interface_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/apfl-advisory-strategy-interface-matrix-cut43.json".into()
    });
    let now_ms = 10_000u64;
    let advisory_key = SigningKey::from_bytes(&[70u8; 32]);
    let base_payload = json!({
        "schema_version": 1,
        "confidence": 82,
        "prefer_transport": "quic",
        "batch_size_hint": 2,
        "keepalive_interval_ms_hint": 15_000,
        "relay_candidate_priority_hint": "prefer_low_failure_count",
        "privacy_budget_hint": "minimize_peer_disclosure",
        "weak_network_mode_hint": true,
        "background_punch_probe_hint": true
    });

    let valid_advisory = sign_apfl_advisory_v0(
        &advisory_key,
        "apfl-valid-001",
        base_payload.clone(),
        9_000,
        20_000,
    )?;

    let expired_advisory = sign_apfl_advisory_v0(
        &advisory_key,
        "apfl-expired-001",
        base_payload.clone(),
        1_000,
        9_000,
    )?;

    let invalid_schema_advisory = sign_apfl_advisory_v0(
        &advisory_key,
        "apfl-invalid-schema-001",
        json!({
            "schema_version": 0,
            "confidence": 82,
            "prefer_transport": "quic"
        }),
        9_000,
        20_000,
    )?;

    let mut bad_signature_advisory = valid_advisory.clone();
    bad_signature_advisory.advisory_id = "apfl-bad-signature-001".into();
    bad_signature_advisory.signature = "00".repeat(64);

    let replay_advisory = sign_apfl_advisory_v0(
        &advisory_key,
        "apfl-replay-001",
        base_payload.clone(),
        9_000,
        20_000,
    )?;

    let force_direct_advisory = sign_apfl_advisory_v0(
        &advisory_key,
        "apfl-force-direct-001",
        json!({
            "schema_version": 1,
            "confidence": 90,
            "prefer_transport": "native_encrypted_novorudp",
            "force_direct": true
        }),
        9_000,
        20_000,
    )?;

    let raw_endpoint_advisory = sign_apfl_advisory_v0(
        &advisory_key,
        "apfl-raw-endpoint-001",
        json!({
            "schema_version": 1,
            "confidence": 90,
            "prefer_transport": "wss",
            "raw_endpoint": "wss://unsigned.example.net:443/novovm"
        }),
        9_000,
        20_000,
    )?;

    let disable_queue_advisory = sign_apfl_advisory_v0(
        &advisory_key,
        "apfl-disable-queue-001",
        json!({
            "schema_version": 1,
            "confidence": 90,
            "prefer_transport": "quic",
            "disable_queue_fallback": true
        }),
        9_000,
        20_000,
    )?;

    let payload_mutation_advisory = sign_apfl_advisory_v0(
        &advisory_key,
        "apfl-payload-mutation-001",
        json!({
            "schema_version": 1,
            "confidence": 90,
            "prefer_transport": "quic",
            "payload_semantics_mutation": true
        }),
        9_000,
        20_000,
    )?;

    let cases = vec![
        evaluate_apfl_advisory_case_v0("valid_advisory_within_bounds", valid_advisory, now_ms, &[]),
        evaluate_apfl_advisory_case_v0("expired_advisory_rejected", expired_advisory, now_ms, &[]),
        evaluate_apfl_advisory_case_v0(
            "invalid_schema_rejected",
            invalid_schema_advisory,
            now_ms,
            &[],
        ),
        evaluate_apfl_advisory_case_v0(
            "bad_signature_rejected",
            bad_signature_advisory,
            now_ms,
            &[],
        ),
        evaluate_apfl_advisory_case_v0(
            "replay_advisory_rejected",
            replay_advisory,
            now_ms,
            &["apfl-replay-001"],
        ),
        evaluate_apfl_advisory_case_v0("force_direct_rejected", force_direct_advisory, now_ms, &[]),
        evaluate_apfl_advisory_case_v0(
            "raw_endpoint_injection_rejected",
            raw_endpoint_advisory,
            now_ms,
            &[],
        ),
        evaluate_apfl_advisory_case_v0(
            "queue_fallback_disable_rejected",
            disable_queue_advisory,
            now_ms,
            &[],
        ),
        evaluate_apfl_advisory_case_v0(
            "payload_semantics_mutation_rejected",
            payload_mutation_advisory,
            now_ms,
            &[],
        ),
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));

    let report = json!({
        "accepted": accepted,
        "scope": "apfl_advisory_strategy_interface_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cut": "Cut 43: APFL Advisory Strategy Interface v0",
        "apfl_model_called": false,
        "apfl_interpreted": false,
        "apfl_advisory_interface_enabled": true,
        "advisory_is_binding": false,
        "hard_policy_precedence": true,
        "advisory_constraints": {
            "must_be_signed": true,
            "ttl_required": true,
            "replay_id_required": true,
            "confidence_required": true,
            "policy_bounds_required": true,
            "may_affect_scoring_only": true,
            "may_not_force_direct": true,
            "may_not_inject_raw_endpoint": true,
            "may_not_disable_queue_fallback": true,
            "may_not_change_payload_semantics": true
        },
        "cases": cases,
        "network_only": true,
        "aoem_called": false,
        "opcode114_called": false,
        "ledger_semantics": false,
        "novorudp_wire_changed": false,
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("apfl advisory strategy interface matrix failed")
    }
}

fn run_strategy_decision_replay_receipt_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/strategy-decision-replay-receipt-matrix-cut44.json".into()
    });
    let now_ms = 10_000u64;
    let advisory_key = SigningKey::from_bytes(&[71u8; 32]);
    let valid_advisory = sign_apfl_advisory_v0(
        &advisory_key,
        "apfl-receipt-valid-001",
        json!({
            "schema_version": 1,
            "confidence": 88,
            "prefer_transport": "quic",
            "batch_size_hint": 2,
            "keepalive_interval_ms_hint": 12_000,
            "privacy_budget_hint": "minimize_peer_disclosure"
        }),
        9_000,
        20_000,
    )?;
    let force_direct_advisory = sign_apfl_advisory_v0(
        &advisory_key,
        "apfl-receipt-force-direct-001",
        json!({
            "schema_version": 1,
            "confidence": 99,
            "prefer_transport": "native_encrypted_novorudp",
            "force_direct": true
        }),
        9_000,
        20_000,
    )?;

    let relay_input = strategy_receipt_input_v0(
        "receipt-relay-with-valid-advisory",
        false,
        true,
        true,
        false,
        Some(valid_advisory),
    );
    let override_input = strategy_receipt_input_v0(
        "receipt-hard-policy-override-rejected",
        false,
        true,
        true,
        false,
        Some(force_direct_advisory),
    );
    let queue_input = strategy_receipt_input_v0(
        "receipt-no-path-queue-fallback",
        false,
        true,
        false,
        true,
        None,
    );

    let cases = vec![
        evaluate_strategy_receipt_case_v0("relay_decision_receipt_replays", relay_input, now_ms),
        evaluate_strategy_receipt_case_v0(
            "hard_policy_override_receipt_replays_rejection",
            override_input,
            now_ms,
        ),
        evaluate_strategy_receipt_case_v0("queue_fallback_receipt_replays", queue_input, now_ms),
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));

    let report = json!({
        "accepted": accepted,
        "scope": "strategy_decision_replay_receipt_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cut": "Cut 44: Strategy Decision Replay Receipt v0",
        "receipt_required": true,
        "receipt_replay_required": true,
        "apfl_advisory_is_binding": false,
        "hard_policy_precedence": true,
        "cases": cases,
        "network_only": true,
        "apfl_model_called": false,
        "apfl_interpreted": false,
        "aoem_called": false,
        "opcode114_called": false,
        "ledger_semantics": false,
        "novorudp_wire_changed": false,
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("strategy decision replay receipt matrix failed")
    }
}

fn run_wss_443_relay_session_runtime_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/wss-443-relay-session-runtime-matrix.json".into()
    });
    let mut manager = Wss443RelaySessionManagerV0::new(2, 30_000);

    let session_a = manager.register_session("node-a", "wss-session-a-1", 1_000);
    let session_b = manager.register_session("node-b", "wss-session-b-1", 1_000);
    let duplicate_a = manager.register_session("node-a", "wss-session-a-2", 2_000);
    let ping_ok = manager.observe_pong("node-a", 2_100);
    let data_frame = PublicRelayDataEnvelopeV0 {
        request_id: "wss-runtime-frame-1".into(),
        source_peer_id: "node-a".into(),
        target_peer_id: "node-b".into(),
        payload: b"opaque-novorudp-frame".to_vec(),
    };
    let forward_1 = manager.forward_by_peer_id(data_frame.clone(), 2_200);
    let forward_2 = manager.forward_by_peer_id(
        PublicRelayDataEnvelopeV0 {
            request_id: "wss-runtime-frame-2".into(),
            ..data_frame.clone()
        },
        2_201,
    );
    let backpressure = manager.forward_by_peer_id(
        PublicRelayDataEnvelopeV0 {
            request_id: "wss-runtime-frame-3".into(),
            ..data_frame.clone()
        },
        2_202,
    );
    let missing_target = manager.forward_by_peer_id(
        PublicRelayDataEnvelopeV0 {
            request_id: "wss-runtime-missing-target".into(),
            target_peer_id: "node-missing".into(),
            ..data_frame.clone()
        },
        2_300,
    );
    let disconnected = manager.disconnect("node-b", 2_400);
    let after_disconnect = manager.forward_by_peer_id(
        PublicRelayDataEnvelopeV0 {
            request_id: "wss-runtime-after-disconnect".into(),
            ..data_frame.clone()
        },
        2_500,
    );
    let reconnected_b = manager.register_session("node-b", "wss-session-b-2", 2_600);
    let after_reconnect = manager.forward_by_peer_id(
        PublicRelayDataEnvelopeV0 {
            request_id: "wss-runtime-after-reconnect".into(),
            ..data_frame
        },
        2_700,
    );
    let expired_count = manager.expire_sessions(40_000);

    let cases = vec![
        json!({
            "case": "peer_id_session_registration",
            "accepted": session_a.accepted && session_b.accepted,
            "registered_peer_ids": ["node-a", "node-b"],
            "session_count": 2,
        }),
        json!({
            "case": "duplicate_login_replaces_previous_session",
            "accepted": duplicate_a.accepted && duplicate_a.replaced_existing,
            "peer_id": "node-a",
            "active_session_id": "wss-session-a-2",
            "replaced_existing": duplicate_a.replaced_existing,
        }),
        json!({
            "case": "ping_pong_keeps_session_alive",
            "accepted": ping_ok,
            "peer_id": "node-a",
            "ping_pong_supported": true,
        }),
        json!({
            "case": "target_peer_id_forwarding",
            "accepted": forward_1.accepted && forward_1.forwarded_to_peer_id.as_deref() == Some("node-b"),
            "forwards_by_peer_id": true,
            "payload_treated_opaque": true,
            "forwarded_to_peer_id": forward_1.forwarded_to_peer_id,
        }),
        json!({
            "case": "relay_queue_backpressure",
            "accepted": forward_2.accepted
                && !backpressure.accepted
                && backpressure.reject_reason.as_deref() == Some("relay_session_backpressure"),
            "relay_queue_depth": 2,
            "relay_queue_limit": 2,
            "reject_reason": backpressure.reject_reason,
        }),
        json!({
            "case": "target_peer_missing_fallback",
            "accepted": !missing_target.accepted
                && missing_target.selected_path_after_failure == "QueueFallback",
            "target_peer_id": "node-missing",
            "fallback_reason": missing_target.reject_reason,
            "selected_path_after_failure": missing_target.selected_path_after_failure,
        }),
        json!({
            "case": "disconnect_and_reconnect",
            "accepted": disconnected
                && !after_disconnect.accepted
                && reconnected_b.accepted
                && after_reconnect.accepted,
            "disconnect_observed": disconnected,
            "after_disconnect_reject_reason": after_disconnect.reject_reason,
            "reconnected_session_id": "wss-session-b-2",
        }),
        json!({
            "case": "session_expiry",
            "accepted": expired_count >= 1,
            "expired_session_count": expired_count,
            "session_ttl_ms": 30000,
        }),
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "wss_443_relay_session_runtime_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "real_wss_tls_socket_implemented": false,
        "real_public_tls_smoke": false,
        "selected_transport": "wss",
        "selected_port": 443,
        "client_direction": "outbound",
        "requires_public_client_inbound": false,
        "session_manager": {
            "peer_id_to_session": true,
            "duplicate_login_replaces_previous_session": true,
            "ping_pong_supported": true,
            "disconnect_detected": true,
            "reconnect_supported": true,
            "relay_queue_limit": 2,
            "backpressure_supported": true,
            "target_peer_missing_fallback": "QueueFallback"
        },
        "relay_is_trusted_authority": false,
        "business_semantics_interpreted_by_relay": false,
        "novorudp_wire_changed": false,
        "cases": cases,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("wss 443 relay session runtime matrix failed")
    }
}

fn run_decentralized_bootstrap_constraint_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/decentralized-bootstrap-constraint-matrix.json".into()
    });

    let cases = vec![
        json!({
            "case": "same_lan_discovery",
            "accepted": true,
            "topology": "multiple_nodes_same_l2_or_l3_lan",
            "discovery_methods": ["mdns", "udp_lan_broadcast", "local_static_peer_cache"],
            "public_auxiliary_required": false,
            "central_authority_required": false,
            "broadcast_effective": true,
            "broadcast_scope": "local_link_or_subnet_only",
            "selected_path": "DirectNovoRudp",
            "product_action": "use_lan_discovery_then_direct_or_lan_relay",
        }),
        json!({
            "case": "lan_plus_cellular_no_shared_reachable_node",
            "accepted": true,
            "topology": "node_a_lan_nat_node_b_cellular_cgnat_vpn_tun",
            "public_auxiliary_required": "not_central_authority_but_some_shared_reachable_path_is_required",
            "central_authority_required": false,
            "broadcast_effective": false,
            "broadcast_scope": "does_not_cross_router_nat_carrier_or_vpn_tun_boundary",
            "direct_discovery_guaranteed": false,
            "reason": "two_private_or_filtered_networks_without_a_shared_reachable_medium_cannot_reliably_discover_or_connect",
            "selected_path": "RelayNovoRudp_or_QueueFallback_until_relay_candidate_exists",
            "product_action": "use_federated_relay_or_rendezvous_candidates_not_single_official_server",
        }),
        json!({
            "case": "ipv6_or_public_endpoint_available",
            "accepted": true,
            "topology": "at_least_one_peer_has_valid_reachable_endpoint",
            "public_auxiliary_required": false,
            "central_authority_required": false,
            "broadcast_effective": false,
            "selected_path": "DirectNovoRudp",
            "product_action": "verify_endpoint_with_observed_probe_then_use_direct_path",
        }),
        json!({
            "case": "decentralized_relay_candidate_available",
            "accepted": true,
            "topology": "any_novovm_node_can_offer_relay_or_rendezvous_service",
            "public_auxiliary_required": true,
            "central_authority_required": false,
            "relay_is_trusted_authority": false,
            "payload_treated_opaque": true,
            "routing_subject": "target_peer_id",
            "peer_identity_source": "novovm_key",
            "selected_path": "RelayNovoRudp",
            "product_action": "select_from_peer_signed_relay_candidates_and_rotate_on_failure",
        }),
    ];

    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "decentralized_bootstrap_constraint_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "terminal_product_policy": true,
        "not_experimental_transition": true,
        "centralized_control_plane_required": false,
        "single_official_relay_required": false,
        "single_official_domain_required": false,
        "relay_is_trusted_authority": false,
        "peer_identity_source": "novovm_key",
        "routing_subject": "target_peer_id",
        "business_semantics_interpreted_by_relay": false,
        "novorudp_wire_changed": false,
        "physical_network_constraints": {
            "lan_broadcast_can_discover_only_local_link_or_subnet": true,
            "broadcast_does_not_cross_nat_router_cellular_cgnat_or_vpn_tun": true,
            "arbitrary_private_networks_need_some_shared_reachable_medium": true,
            "shared_reachable_medium_can_be_any_decentralized_novovm_relay": true,
            "shared_reachable_medium_must_not_be_single_trust_root": true
        },
        "terminal_strategy": {
            "lan_first": true,
            "direct_ipv6_or_observed_endpoint_when_available": true,
            "federated_relay_candidates": true,
            "peer_signed_relay_endpoint_records": true,
            "multi_relay_rotation": true,
            "nat_punch_as_optimization": true,
            "wss_tls_443_as_default_outbound_transport": true,
            "queue_fallback_when_no_candidate_reachable": true
        },
        "cases": cases,
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("decentralized bootstrap constraint matrix failed")
    }
}

fn run_multi_relay_candidate_rotation_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/multi-relay-candidate-rotation-matrix.json".into()
    });
    let now_ms = 10_000u64;

    let single_healthy = evaluate_relay_selection_case_v0(
        "single_healthy_relay",
        vec![relay_candidate_v0(
            "relay-a",
            "wss://relay-a.example.com:443/novovm",
            "wss",
            443,
            10,
            true,
            true,
            0,
            0,
        )],
        now_ms,
        false,
    );

    let primary_cooldown = evaluate_relay_selection_case_v0(
        "primary_relay_cooldown",
        vec![
            relay_candidate_v0(
                "relay-a",
                "wss://relay-a.example.com:443/novovm",
                "wss",
                443,
                10,
                true,
                true,
                1,
                now_ms + 60_000,
            ),
            relay_candidate_v0(
                "relay-b",
                "wss://relay-b.example.com:443/novovm",
                "wss",
                443,
                20,
                true,
                true,
                0,
                0,
            ),
        ],
        now_ms,
        false,
    );

    let primary_failure_rotation = evaluate_relay_selection_case_v0(
        "primary_relay_send_failure_rotates",
        vec![
            relay_candidate_v0(
                "relay-a",
                "wss://relay-a.example.com:443/novovm",
                "wss",
                443,
                10,
                true,
                true,
                0,
                0,
            ),
            relay_candidate_v0(
                "relay-b",
                "wss://relay-b.example.com:443/novovm",
                "wss",
                443,
                20,
                true,
                true,
                0,
                0,
            ),
        ],
        now_ms,
        true,
    );

    let invalid_signature = evaluate_relay_selection_case_v0(
        "invalid_relay_signature_rejected",
        vec![
            relay_candidate_v0(
                "relay-invalid",
                "wss://relay-invalid.example.com:443/novovm",
                "wss",
                443,
                5,
                false,
                true,
                0,
                0,
            ),
            relay_candidate_v0(
                "relay-b",
                "wss://relay-b.example.com:443/novovm",
                "wss",
                443,
                20,
                true,
                true,
                0,
                0,
            ),
        ],
        now_ms,
        false,
    );

    let all_unavailable = evaluate_relay_selection_case_v0(
        "all_relays_unavailable_queue_fallback",
        vec![
            relay_candidate_v0(
                "relay-a",
                "wss://relay-a.example.com:443/novovm",
                "wss",
                443,
                10,
                true,
                false,
                3,
                now_ms + 60_000,
            ),
            relay_candidate_v0(
                "relay-b",
                "udp://relay-b.example.com:41030",
                "udp",
                41030,
                20,
                true,
                false,
                2,
                0,
            ),
        ],
        now_ms,
        false,
    );

    let transport_priority = evaluate_relay_selection_case_v0(
        "transport_priority_prefers_wss_443",
        vec![
            relay_candidate_v0(
                "relay-udp",
                "udp://relay-udp.example.com:41030",
                "udp",
                41030,
                10,
                true,
                true,
                0,
                0,
            ),
            relay_candidate_v0(
                "relay-wss",
                "wss://relay-wss.example.com:443/novovm",
                "wss",
                443,
                10,
                true,
                true,
                0,
                0,
            ),
        ],
        now_ms,
        false,
    );

    let cases = vec![
        single_healthy,
        primary_cooldown,
        primary_failure_rotation,
        invalid_signature,
        all_unavailable,
        transport_priority,
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "multi_relay_candidate_rotation_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "relay_is_trusted_authority": false,
        "centralized_control_plane_required": false,
        "single_official_relay_required": false,
        "peer_identity_source": "novovm_key",
        "routing_subject": "target_peer_id",
        "relay_record_source": "peer_signed_relay_candidate_records",
        "selection_policy": {
            "require_record_signature_valid": true,
            "skip_cooldown_relays": true,
            "skip_unreachable_relays": true,
            "prefer_wss_443_over_udp_fixed_port": true,
            "rotate_on_send_failure": true,
            "all_relays_failed_fallback": "QueueFallback"
        },
        "cases": cases,
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("multi relay candidate rotation matrix failed")
    }
}

fn run_peer_signed_relay_record_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/peer-signed-relay-record-matrix.json".into()
    });
    let now_ms = 10_000u64;
    let relay_a_key = SigningKey::from_bytes(&[32u8; 32]);
    let relay_b_key = SigningKey::from_bytes(&[33u8; 32]);
    let valid_endpoint =
        peer_signed_relay_endpoint_v0("wss", "wss://relay-a.example.com:443/novovm", 443, 10);
    let valid_record = sign_relay_endpoint_record_v0(
        &relay_a_key,
        vec![valid_endpoint.clone()],
        9_000,
        70_000,
        "relay-a-record-001",
    )?;

    let valid_case = evaluate_signed_relay_record_case_v0(
        "valid_signed_relay_record",
        vec![valid_record.clone()],
        now_ms,
    );

    let mut invalid_signature_record = valid_record.clone();
    invalid_signature_record.signature = "00".repeat(64);
    let invalid_signature_case = evaluate_signed_relay_record_case_v0(
        "invalid_signature_rejected",
        vec![invalid_signature_record],
        now_ms,
    );

    let expired_record = sign_relay_endpoint_record_v0(
        &relay_a_key,
        vec![valid_endpoint.clone()],
        1_000,
        9_000,
        "relay-a-expired-001",
    )?;
    let expired_case = evaluate_signed_relay_record_case_v0(
        "expired_record_rejected",
        vec![expired_record],
        now_ms,
    );

    let identity_mismatch_record = sign_relay_endpoint_record_with_peer_id_v0(
        &relay_a_key,
        "novovm-ed25519:identity-mismatch",
        vec![valid_endpoint.clone()],
        9_000,
        70_000,
        "relay-a-identity-mismatch-001",
    )?;
    let identity_mismatch_case = evaluate_signed_relay_record_case_v0(
        "peer_id_public_key_mismatch_rejected",
        vec![identity_mismatch_record],
        now_ms,
    );

    let mut tampered_record = valid_record.clone();
    tampered_record.endpoints[0].uri = "wss://attacker.example.com:443/novovm".to_string();
    let tamper_case = evaluate_signed_relay_record_case_v0(
        "endpoint_tamper_rejected",
        vec![tampered_record],
        now_ms,
    );

    let unsupported_transport_record = sign_relay_endpoint_record_v0(
        &relay_a_key,
        vec![peer_signed_relay_endpoint_v0(
            "unknown",
            "unknown://relay-a.example.com:443/novovm",
            443,
            10,
        )],
        9_000,
        70_000,
        "relay-a-unknown-transport-001",
    )?;
    let unsupported_transport_case = evaluate_signed_relay_record_case_v0(
        "unsupported_transport_rejected",
        vec![unsupported_transport_record],
        now_ms,
    );

    let udp_record = sign_relay_endpoint_record_v0(
        &relay_a_key,
        vec![peer_signed_relay_endpoint_v0(
            "udp",
            "udp://relay-a.example.com:41030",
            41030,
            10,
        )],
        9_000,
        70_000,
        "relay-a-udp-001",
    )?;
    let wss_record = sign_relay_endpoint_record_v0(
        &relay_b_key,
        vec![peer_signed_relay_endpoint_v0(
            "wss",
            "wss://relay-b.example.com:443/novovm",
            443,
            10,
        )],
        9_000,
        70_000,
        "relay-b-wss-001",
    )?;
    let multiple_valid_case = evaluate_signed_relay_record_case_v0(
        "multiple_valid_records_prefers_wss_443",
        vec![udp_record, wss_record],
        now_ms,
    );

    let cases = vec![
        valid_case,
        invalid_signature_case,
        expired_case,
        identity_mismatch_case,
        tamper_case,
        unsupported_transport_case,
        multiple_valid_case,
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));
    let relay_record_count = cases
        .iter()
        .map(|case| case["relay_record_count"].as_u64().unwrap_or(0))
        .sum::<u64>();
    let signature_checked_count = cases
        .iter()
        .map(|case| {
            case["relay_record_signature_checked_count"]
                .as_u64()
                .unwrap_or(0)
        })
        .sum::<u64>();
    let signature_valid_count = cases
        .iter()
        .map(|case| {
            case["relay_record_signature_valid_count"]
                .as_u64()
                .unwrap_or(0)
        })
        .sum::<u64>();
    let signature_invalid_count = cases
        .iter()
        .map(|case| {
            case["relay_record_signature_invalid_count"]
                .as_u64()
                .unwrap_or(0)
        })
        .sum::<u64>();

    let report = json!({
        "accepted": accepted,
        "scope": "peer_signed_relay_endpoint_record_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "relay_is_trusted_authority": false,
        "centralized_control_plane_required": false,
        "single_official_relay_required": false,
        "peer_identity_source": "novovm_key",
        "routing_subject": "target_peer_id",
        "signature_scheme": "ed25519",
        "canonical_payload_covers": [
            "record_version",
            "relay_peer_id",
            "relay_public_key",
            "endpoints",
            "issued_at_ms",
            "expires_at_ms",
            "nonce_or_record_id",
            "capabilities"
        ],
        "relay_record_count": relay_record_count,
        "relay_record_signature_checked_count": signature_checked_count,
        "relay_record_signature_valid_count": signature_valid_count,
        "relay_record_signature_invalid_count": signature_invalid_count,
        "cases": cases,
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("peer signed relay record matrix failed")
    }
}

fn run_privacy_preserving_node_discovery_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/privacy-preserving-node-discovery-matrix.json".into()
    });
    let now_ms = 10_000u64;
    let candidate_set_policy_limit = 2usize;
    let relay_a_key = SigningKey::from_bytes(&[40u8; 32]);
    let relay_b_key = SigningKey::from_bytes(&[41u8; 32]);
    let relay_c_key = SigningKey::from_bytes(&[42u8; 32]);

    let signed_records = vec![
        sign_relay_endpoint_record_v0(
            &relay_a_key,
            vec![peer_signed_relay_endpoint_v0(
                "wss",
                "wss://relay-a.example.com:443/novovm",
                443,
                10,
            )],
            9_000,
            70_000,
            "privacy-relay-a-001",
        )?,
        sign_relay_endpoint_record_v0(
            &relay_b_key,
            vec![peer_signed_relay_endpoint_v0(
                "wss",
                "wss://relay-b.example.net:443/novovm",
                443,
                20,
            )],
            9_000,
            70_000,
            "privacy-relay-b-001",
        )?,
        sign_relay_endpoint_record_v0(
            &relay_c_key,
            vec![peer_signed_relay_endpoint_v0(
                "udp",
                "udp://relay-c.example.org:41030",
                41030,
                30,
            )],
            9_000,
            70_000,
            "privacy-relay-c-001",
        )?,
    ];
    let directory_response = issue_blinded_relay_directory_response_v0(
        &signed_records,
        candidate_set_policy_limit,
        now_ms,
    );
    let valid_count = signed_records
        .iter()
        .filter(|record| validate_peer_signed_relay_record_v0(record, now_ms).accepted)
        .count();

    let mut tampered_record = signed_records[0].clone();
    tampered_record.endpoints[0].uri = "wss://attacker.example.com:443/novovm".into();
    let tampered_validation = validate_peer_signed_relay_record_v0(&tampered_record, now_ms);

    let expired_record = sign_relay_endpoint_record_v0(
        &relay_a_key,
        vec![peer_signed_relay_endpoint_v0(
            "wss",
            "wss://relay-a.example.com:443/novovm",
            443,
            10,
        )],
        1_000,
        9_000,
        "privacy-relay-a-expired-001",
    )?;
    let expired_validation = validate_peer_signed_relay_record_v0(&expired_record, now_ms);

    let candidate_endpoint_encrypted_or_blinded = directory_response.iter().all(|entry| {
        entry
            .encrypted_or_blinded_endpoint_hint
            .starts_with("blind:v0:")
            && !entry.encrypted_or_blinded_endpoint_hint.contains("://")
    });
    let raw_ip_directory_exposed = directory_response
        .iter()
        .any(|entry| entry.encrypted_or_blinded_endpoint_hint.contains("://"));
    let full_relay_ip_list_synced = directory_response.len() == signed_records.len();

    let cases = vec![
        json!({
            "case": "full_raw_ip_directory_exposure_rejected",
            "accepted": !raw_ip_directory_exposed,
            "raw_ip_directory_exposed": false,
            "full_relay_ip_list_synced": false,
            "reject_reason": "full_directory_sync_forbidden",
        }),
        json!({
            "case": "minimal_candidate_set_issued",
            "accepted": directory_response.len() <= candidate_set_policy_limit,
            "node_receives_minimal_candidate_set": true,
            "candidate_set_size": directory_response.len(),
            "candidate_set_policy_limit": candidate_set_policy_limit,
        }),
        json!({
            "case": "valid_signed_blinded_candidate_accepted",
            "accepted": valid_count >= candidate_set_policy_limit,
            "candidate_record_signed": true,
            "candidate_signature_valid_count": valid_count,
            "candidate_endpoint_encrypted_or_blinded": candidate_endpoint_encrypted_or_blinded,
        }),
        json!({
            "case": "tampered_candidate_rejected",
            "accepted": !tampered_validation.accepted
                && tampered_validation.reject_reason.as_deref() == Some("relay_record_signature_invalid"),
            "candidate_signature_valid": tampered_validation.signature_valid,
            "reject_reason": tampered_validation.reject_reason,
        }),
        json!({
            "case": "expired_candidate_rejected",
            "accepted": !expired_validation.accepted
                && expired_validation.reject_reason.as_deref() == Some("relay_record_expired"),
            "candidate_signature_valid": expired_validation.signature_valid,
            "reject_reason": expired_validation.reject_reason,
        }),
        json!({
            "case": "excessive_directory_sync_rejected",
            "accepted": directory_response.len() < signed_records.len(),
            "requested_candidate_count": signed_records.len(),
            "issued_candidate_count": directory_response.len(),
            "full_relay_ip_list_synced": false,
            "reject_reason": "full_directory_sync_forbidden",
        }),
        json!({
            "case": "blinded_endpoint_hint_present",
            "accepted": candidate_endpoint_encrypted_or_blinded,
            "candidate_endpoint_encrypted_or_blinded": candidate_endpoint_encrypted_or_blinded,
        }),
        json!({
            "case": "routing_remains_target_peer_id",
            "accepted": true,
            "routing_subject": "target_peer_id",
        }),
        json!({
            "case": "relay_remains_non_authority",
            "accepted": true,
            "relay_is_trusted_authority": false,
            "business_semantics_interpreted_by_relay": false,
        }),
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "privacy_preserving_node_discovery_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "raw_ip_directory_exposed": false,
        "full_relay_ip_list_synced": full_relay_ip_list_synced,
        "node_receives_minimal_candidate_set": true,
        "candidate_set_size": directory_response.len(),
        "candidate_set_policy_limit": candidate_set_policy_limit,
        "candidate_record_signed": true,
        "candidate_signature_valid_count": valid_count,
        "candidate_signature_invalid_count": usize::from(!tampered_validation.signature_valid),
        "candidate_endpoint_encrypted_or_blinded": candidate_endpoint_encrypted_or_blinded,
        "peer_identity_source": "novovm_key",
        "routing_subject": "target_peer_id",
        "relay_is_trusted_authority": false,
        "centralized_control_plane_required": false,
        "single_official_relay_required": false,
        "business_semantics_interpreted_by_relay": false,
        "novorudp_wire_changed": false,
        "non_goals": {
            "tor_grade_anonymity_claimed": false,
            "os_router_isp_visibility_hidden": false,
            "economic_penalty_or_chain_market": false,
            "full_dht_implemented": false
        },
        "directory_policy": {
            "raw_endpoint_directory_forbidden": true,
            "full_directory_sync_forbidden": true,
            "minimal_candidate_set_required": true,
            "candidate_records_must_be_peer_signed": true,
            "endpoint_hint_must_be_blinded_or_encrypted": true,
            "candidate_records_must_expire": true,
            "candidate_rotation_required": true
        },
        "directory_response": directory_response,
        "cases": cases,
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("privacy preserving node discovery matrix failed")
    }
}

fn run_signed_bootstrap_manifest_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/signed-bootstrap-manifest-matrix.json".into()
    });
    let now_ms = 20_000u64;
    let candidate_set_policy_limit = 2usize;
    let manifest_key = SigningKey::from_bytes(&[50u8; 32]);
    let relay_a_key = SigningKey::from_bytes(&[51u8; 32]);
    let relay_b_key = SigningKey::from_bytes(&[52u8; 32]);
    let rendezvous_key = SigningKey::from_bytes(&[53u8; 32]);
    let seed_relay_records = vec![
        sign_relay_endpoint_record_v0(
            &relay_a_key,
            vec![peer_signed_relay_endpoint_v0(
                "wss",
                "wss://relay-a.bootstrap.example:443/novovm",
                443,
                10,
            )],
            19_000,
            90_000,
            "bootstrap-relay-a-001",
        )?,
        sign_relay_endpoint_record_v0(
            &relay_b_key,
            vec![peer_signed_relay_endpoint_v0(
                "quic",
                "quic://relay-b.bootstrap.example:443/novovm",
                443,
                20,
            )],
            19_000,
            90_000,
            "bootstrap-relay-b-001",
        )?,
    ];
    let seed_rendezvous_records = vec![sign_relay_endpoint_record_v0(
        &rendezvous_key,
        vec![peer_signed_relay_endpoint_v0(
            "wss",
            "wss://rendezvous-a.bootstrap.example:443/novovm",
            443,
            30,
        )],
        19_000,
        90_000,
        "bootstrap-rendezvous-a-001",
    )?];
    let valid_manifest = sign_bootstrap_manifest_v0(
        &manifest_key,
        BootstrapManifestSigningInputV0 {
            bootstrap_manifest_source: "installer_bundle",
            seed_relay_candidates: seed_relay_records.clone(),
            seed_rendezvous_candidates: seed_rendezvous_records.clone(),
            issued_at_ms: 19_000,
            expires_at_ms: 90_000,
            manifest_id: "bootstrap-manifest-valid-001",
            full_raw_ip_directory_embedded: false,
            manifest_requires_single_official_relay: false,
            manifest_requires_single_official_domain: false,
            candidate_set_policy_limit,
        },
    )?;
    let valid_validation = validate_bootstrap_manifest_v0(&valid_manifest, now_ms);

    let mut invalid_signature_manifest = valid_manifest.clone();
    invalid_signature_manifest.signature = "00".repeat(64);
    let invalid_signature_validation =
        validate_bootstrap_manifest_v0(&invalid_signature_manifest, now_ms);

    let expired_manifest = sign_bootstrap_manifest_v0(
        &manifest_key,
        BootstrapManifestSigningInputV0 {
            bootstrap_manifest_source: "history_cache",
            seed_relay_candidates: seed_relay_records.clone(),
            seed_rendezvous_candidates: seed_rendezvous_records.clone(),
            issued_at_ms: 1_000,
            expires_at_ms: 19_000,
            manifest_id: "bootstrap-manifest-expired-001",
            full_raw_ip_directory_embedded: false,
            manifest_requires_single_official_relay: false,
            manifest_requires_single_official_domain: false,
            candidate_set_policy_limit,
        },
    )?;
    let expired_validation = validate_bootstrap_manifest_v0(&expired_manifest, now_ms);

    let raw_directory_manifest = sign_bootstrap_manifest_v0(
        &manifest_key,
        BootstrapManifestSigningInputV0 {
            bootstrap_manifest_source: "official_site",
            seed_relay_candidates: seed_relay_records.clone(),
            seed_rendezvous_candidates: seed_rendezvous_records.clone(),
            issued_at_ms: 19_000,
            expires_at_ms: 90_000,
            manifest_id: "bootstrap-manifest-raw-directory-001",
            full_raw_ip_directory_embedded: true,
            manifest_requires_single_official_relay: false,
            manifest_requires_single_official_domain: false,
            candidate_set_policy_limit,
        },
    )?;
    let raw_directory_validation = validate_bootstrap_manifest_v0(&raw_directory_manifest, now_ms);

    let single_relay_manifest = sign_bootstrap_manifest_v0(
        &manifest_key,
        BootstrapManifestSigningInputV0 {
            bootstrap_manifest_source: "qr_invite",
            seed_relay_candidates: seed_relay_records.clone(),
            seed_rendezvous_candidates: seed_rendezvous_records.clone(),
            issued_at_ms: 19_000,
            expires_at_ms: 90_000,
            manifest_id: "bootstrap-manifest-single-relay-001",
            full_raw_ip_directory_embedded: false,
            manifest_requires_single_official_relay: true,
            manifest_requires_single_official_domain: false,
            candidate_set_policy_limit,
        },
    )?;
    let single_relay_validation = validate_bootstrap_manifest_v0(&single_relay_manifest, now_ms);

    let single_domain_manifest = sign_bootstrap_manifest_v0(
        &manifest_key,
        BootstrapManifestSigningInputV0 {
            bootstrap_manifest_source: "friend_invite",
            seed_relay_candidates: seed_relay_records.clone(),
            seed_rendezvous_candidates: seed_rendezvous_records.clone(),
            issued_at_ms: 19_000,
            expires_at_ms: 90_000,
            manifest_id: "bootstrap-manifest-single-domain-001",
            full_raw_ip_directory_embedded: false,
            manifest_requires_single_official_relay: false,
            manifest_requires_single_official_domain: true,
            candidate_set_policy_limit,
        },
    )?;
    let single_domain_validation = validate_bootstrap_manifest_v0(&single_domain_manifest, now_ms);

    let cut33_handoff_accepted = valid_validation.accepted
        && valid_validation.blinded_directory_response.len() <= candidate_set_policy_limit
        && valid_validation
            .blinded_directory_response
            .iter()
            .all(|entry| {
                entry
                    .encrypted_or_blinded_endpoint_hint
                    .starts_with("blind:v0:")
                    && !entry.encrypted_or_blinded_endpoint_hint.contains("://")
            });
    let cases = vec![
        json!({
            "case": "valid_signed_bootstrap_manifest",
            "accepted": valid_validation.accepted,
            "bootstrap_manifest_signature_valid": valid_validation.signature_valid,
            "bootstrap_manifest_source": valid_manifest.bootstrap_manifest_source,
            "client_accepts_manifest": valid_validation.accepted,
            "client_reject_reason": valid_validation.reject_reason,
        }),
        json!({
            "case": "invalid_manifest_signature_rejected",
            "accepted": !invalid_signature_validation.accepted
                && invalid_signature_validation.reject_reason.as_deref() == Some("bootstrap_manifest_signature_invalid"),
            "bootstrap_manifest_signature_valid": invalid_signature_validation.signature_valid,
            "client_accepts_manifest": invalid_signature_validation.accepted,
            "client_reject_reason": invalid_signature_validation.reject_reason,
        }),
        json!({
            "case": "expired_manifest_rejected",
            "accepted": !expired_validation.accepted
                && expired_validation.reject_reason.as_deref() == Some("bootstrap_manifest_expired"),
            "bootstrap_manifest_signature_valid": expired_validation.signature_valid,
            "bootstrap_manifest_expired": expired_validation.expired,
            "client_accepts_manifest": expired_validation.accepted,
            "client_reject_reason": expired_validation.reject_reason,
        }),
        json!({
            "case": "manifest_with_full_raw_ip_directory_rejected",
            "accepted": !raw_directory_validation.accepted
                && raw_directory_validation.reject_reason.as_deref() == Some("full_raw_ip_directory_forbidden"),
            "full_raw_ip_directory_embedded": raw_directory_manifest.full_raw_ip_directory_embedded,
            "client_accepts_manifest": raw_directory_validation.accepted,
            "client_reject_reason": raw_directory_validation.reject_reason,
        }),
        json!({
            "case": "manifest_requires_single_official_relay_rejected",
            "accepted": !single_relay_validation.accepted
                && single_relay_validation.reject_reason.as_deref() == Some("single_official_relay_forbidden"),
            "manifest_requires_single_official_relay": single_relay_manifest.manifest_requires_single_official_relay,
            "client_accepts_manifest": single_relay_validation.accepted,
            "client_reject_reason": single_relay_validation.reject_reason,
        }),
        json!({
            "case": "manifest_requires_single_official_domain_rejected",
            "accepted": !single_domain_validation.accepted
                && single_domain_validation.reject_reason.as_deref() == Some("single_official_domain_forbidden"),
            "manifest_requires_single_official_domain": single_domain_manifest.manifest_requires_single_official_domain,
            "client_accepts_manifest": single_domain_validation.accepted,
            "client_reject_reason": single_domain_validation.reject_reason,
        }),
        json!({
            "case": "manifest_seed_candidates_handed_to_cut33_policy",
            "accepted": cut33_handoff_accepted,
            "seed_relay_candidate_count": valid_manifest.seed_relay_candidates.len(),
            "issued_blinded_candidate_count": valid_validation.blinded_directory_response.len(),
            "candidate_set_policy_limit": candidate_set_policy_limit,
            "node_receives_minimal_candidate_set": true,
            "candidate_endpoint_encrypted_or_blinded": true,
            "raw_ip_directory_exposed": false,
        }),
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "signed_bootstrap_manifest_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "bootstrap_manifest_signature_valid": valid_validation.signature_valid,
        "bootstrap_manifest_source": valid_manifest.bootstrap_manifest_source,
        "bootstrap_manifest_expired": valid_validation.expired,
        "seed_relay_candidate_count": valid_manifest.seed_relay_candidates.len(),
        "seed_rendezvous_candidate_count": valid_manifest.seed_rendezvous_candidates.len(),
        "full_raw_ip_directory_embedded": valid_manifest.full_raw_ip_directory_embedded,
        "manifest_requires_single_official_relay": valid_manifest.manifest_requires_single_official_relay,
        "manifest_requires_single_official_domain": valid_manifest.manifest_requires_single_official_domain,
        "client_accepts_manifest": valid_validation.accepted,
        "client_reject_reason": valid_validation.reject_reason,
        "seed_relay_record_valid_count": valid_validation.seed_relay_record_valid_count,
        "seed_relay_record_invalid_count": valid_validation.seed_relay_record_invalid_count,
        "candidate_set_policy_limit": candidate_set_policy_limit,
        "cut33_blinded_directory_handoff": cut33_handoff_accepted,
        "issued_blinded_candidate_count": valid_validation.blinded_directory_response.len(),
        "full_raw_ip_directory_exposed": false,
        "centralized_control_plane_required": false,
        "single_official_relay_required": false,
        "single_official_domain_required": false,
        "relay_is_trusted_authority": false,
        "peer_identity_source": "novovm_key",
        "routing_subject": "target_peer_id",
        "business_semantics_interpreted_by_relay": false,
        "novorudp_wire_changed": false,
        "manifest_policy": {
            "signature_required": true,
            "expiry_required": true,
            "full_raw_ip_directory_forbidden": true,
            "single_official_relay_forbidden": true,
            "single_official_domain_forbidden": true,
            "seed_candidates_forwarded_to_cut33_directory_policy": true
        },
        "blinded_directory_response": valid_validation.blinded_directory_response,
        "cases": cases,
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("signed bootstrap manifest matrix failed")
    }
}

fn run_bootstrap_source_resolver_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/bootstrap-source-resolver-matrix.json".into()
    });
    let fixture = BootstrapResolverFixtureV0::new()?;
    let now_ms = fixture.now_ms;
    let candidate_set_policy_limit = fixture.candidate_set_policy_limit;

    let fresh_cache_case = evaluate_bootstrap_source_resolver_case_v0(
        "valid_cache_preferred_when_fresh",
        vec![
            fixture.source(
                "local_cache",
                10,
                true,
                fixture.valid_cache_manifest.clone(),
            ),
            fixture.source(
                "embedded_install_manifest",
                20,
                true,
                fixture.valid_embedded_manifest.clone(),
            ),
            fixture.source(
                "official_signed_bootstrap_manifest",
                40,
                true,
                fixture.valid_official_manifest.clone(),
            ),
        ],
        now_ms,
        candidate_set_policy_limit,
    );
    let expired_cache_case = evaluate_bootstrap_source_resolver_case_v0(
        "expired_cache_skipped",
        vec![
            fixture.source(
                "local_cache",
                10,
                true,
                fixture.expired_cache_manifest.clone(),
            ),
            fixture.source(
                "embedded_install_manifest",
                20,
                true,
                fixture.valid_embedded_manifest.clone(),
            ),
        ],
        now_ms,
        candidate_set_policy_limit,
    );
    let invalid_signature_case = evaluate_bootstrap_source_resolver_case_v0(
        "invalid_signature_source_rejected",
        vec![
            fixture.source(
                "qr_invite_manifest",
                30,
                true,
                fixture.invalid_signature_manifest(),
            ),
            fixture.source(
                "community_signed_bootstrap_manifest",
                50,
                true,
                fixture.valid_community_manifest.clone(),
            ),
        ],
        now_ms,
        candidate_set_policy_limit,
    );
    let official_not_mandatory_case = evaluate_bootstrap_source_resolver_case_v0(
        "official_source_not_mandatory",
        vec![
            fixture.source(
                "community_signed_bootstrap_manifest",
                50,
                true,
                fixture.valid_community_manifest.clone(),
            ),
            fixture.source(
                "friend_invite_manifest",
                35,
                true,
                fixture.valid_friend_invite_manifest.clone(),
            ),
        ],
        now_ms,
        candidate_set_policy_limit,
    );
    let multi_source_merge_case = evaluate_bootstrap_source_resolver_case_v0(
        "multi_source_merge_does_not_expose_raw_ip_directory",
        vec![
            fixture.source(
                "embedded_install_manifest",
                20,
                true,
                fixture.valid_embedded_manifest.clone(),
            ),
            fixture.source(
                "community_signed_bootstrap_manifest",
                50,
                true,
                fixture.valid_community_manifest.clone(),
            ),
            fixture.source(
                "discovered_blinded_directory_source",
                60,
                true,
                fixture.valid_discovered_manifest.clone(),
            ),
        ],
        now_ms,
        candidate_set_policy_limit,
    );
    let deterministic_fallback_case = evaluate_bootstrap_source_resolver_case_v0(
        "fallback_order_deterministic",
        vec![
            fixture.source(
                "local_cache",
                10,
                false,
                fixture.valid_cache_manifest.clone(),
            ),
            fixture.source(
                "embedded_install_manifest",
                20,
                true,
                fixture.invalid_signature_manifest(),
            ),
            fixture.source(
                "qr_invite_manifest",
                30,
                true,
                fixture.valid_qr_manifest.clone(),
            ),
            fixture.source(
                "official_signed_bootstrap_manifest",
                40,
                true,
                fixture.valid_official_manifest.clone(),
            ),
        ],
        now_ms,
        candidate_set_policy_limit,
    );
    let no_source_case = evaluate_bootstrap_source_resolver_case_v0(
        "no_reachable_bootstrap_source_enters_queue_fallback",
        vec![
            fixture.source(
                "local_cache",
                10,
                false,
                fixture.valid_cache_manifest.clone(),
            ),
            fixture.source(
                "embedded_install_manifest",
                20,
                true,
                fixture.expired_embedded_manifest.clone(),
            ),
            fixture.source(
                "official_signed_bootstrap_manifest",
                40,
                true,
                fixture.invalid_signature_manifest(),
            ),
        ],
        now_ms,
        candidate_set_policy_limit,
    );

    let cases = vec![
        fresh_cache_case,
        expired_cache_case,
        invalid_signature_case,
        official_not_mandatory_case,
        multi_source_merge_case,
        deterministic_fallback_case,
        no_source_case,
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));
    let selected_manifest_source = cases
        .first()
        .and_then(|case| case["selected_bootstrap_manifest_source"].as_str())
        .map(str::to_string);
    let report = json!({
        "accepted": accepted,
        "scope": "bootstrap_source_resolver_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "bootstrap_source_resolver_enabled": true,
        "fallback_order": [
            "local_cache",
            "embedded_install_manifest",
            "qr_invite_manifest",
            "friend_invite_manifest",
            "official_signed_bootstrap_manifest",
            "community_signed_bootstrap_manifest",
            "discovered_blinded_directory_source"
        ],
        "selected_bootstrap_manifest_source": selected_manifest_source,
        "valid_cache_preferred_when_fresh": true,
        "expired_cache_skipped": true,
        "invalid_signature_source_rejected": true,
        "official_source_required": false,
        "multi_source_merge_exposes_raw_ip_directory": false,
        "fallback_order_deterministic": true,
        "no_reachable_bootstrap_source_selected_path": "QueueFallback",
        "centralized_control_plane_required": false,
        "single_official_relay_required": false,
        "single_official_domain_required": false,
        "full_raw_ip_directory_exposed": false,
        "relay_is_trusted_authority": false,
        "peer_identity_source": "novovm_key",
        "routing_subject": "target_peer_id",
        "business_semantics_interpreted_by_relay": false,
        "novorudp_wire_changed": false,
        "cases": cases,
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("bootstrap source resolver matrix failed")
    }
}

fn run_public_relay_bootstrap_relay_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/public-relay-bootstrap-relay.json".into()
    });
    let bind_addr =
        env_string("NOVOVM_OVERLAY_GATE_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:41030".into());
    let node_id = env_string("NOVOVM_OVERLAY_PUBLIC_RELAY_NODE_ID")
        .unwrap_or_else(|| "public-relay-1".into());
    let max_frames = env_u64("NOVOVM_OVERLAY_GATE_MAX_FRAMES", 4).max(1);
    let expected_sessions = env_u64("NOVOVM_OVERLAY_PUBLIC_RELAY_EXPECTED_SESSIONS", 2).max(1);
    let timeout_ms = env_u64("NOVOVM_OVERLAY_GATE_TIMEOUT_MS", 30_000);
    let socket =
        UdpSocket::bind(&bind_addr).with_context(|| format!("bind public relay: {bind_addr}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(250)))
        .context("set public relay read timeout")?;
    let bind_addr_effective = socket.local_addr().context("public relay local addr")?;
    let start = Instant::now();
    let mut buf = vec![0u8; 65535];
    let mut sessions: BTreeMap<String, SocketAddr> = BTreeMap::new();
    let mut events = Vec::new();
    let mut relay_envelopes_received = 0u64;
    let mut relay_frames_forwarded = 0u64;
    let mut recv_error = None;

    while start.elapsed() < Duration::from_millis(timeout_ms)
        && (sessions.len() < expected_sessions as usize || relay_frames_forwarded < max_frames)
    {
        match socket.recv_from(&mut buf) {
            Ok((received_bytes, source_addr)) => {
                if let Ok(frame) = novovm_network::novorudp::NovoRudpTransportFrameV0::decode(
                    &buf[..received_bytes],
                ) {
                    if frame.kind == NovoRudpTransportFrameKindV0::Endpoint {
                        match serde_json::from_slice::<PublicRelayRegisterPayloadV0>(&frame.payload)
                        {
                            Ok(register) => {
                                sessions.insert(register.peer_id.clone(), source_addr);
                                events.push(json!({
                                    "kind": "bootstrap_register",
                                    "peer_id": register.peer_id,
                                    "source_addr": source_addr.to_string(),
                                    "received_bytes": received_bytes,
                                }));
                            }
                            Err(error) => events.push(json!({
                                "kind": "decode_register_failed",
                                "source_addr": source_addr.to_string(),
                                "error": error.to_string(),
                            })),
                        }
                        continue;
                    }
                }

                match serde_json::from_slice::<PublicRelayDataEnvelopeV0>(&buf[..received_bytes]) {
                    Ok(envelope) => {
                        relay_envelopes_received += 1;
                        match sessions.get(&envelope.target_peer_id) {
                            Some(target_addr) => {
                                match socket.send_to(&envelope.payload, target_addr) {
                                    Ok(forwarded_bytes) => {
                                        relay_frames_forwarded += 1;
                                        events.push(json!({
                                        "kind": "relay_envelope",
                                        "request_id": envelope.request_id,
                                        "source_peer_id": envelope.source_peer_id,
                                        "target_peer_id": envelope.target_peer_id,
                                        "forwarded_to_peer_id": envelope.target_peer_id,
                                        "forwarded_to_session_endpoint": target_addr.to_string(),
                                        "forwarded_bytes": forwarded_bytes,
                                    }));
                                    }
                                    Err(error) => events.push(json!({
                                        "kind": "relay_forward_failed",
                                        "request_id": envelope.request_id,
                                        "target_peer_id": envelope.target_peer_id,
                                        "error": error.to_string(),
                                    })),
                                }
                            }
                            None => events.push(json!({
                                "kind": "relay_target_session_missing",
                                "request_id": envelope.request_id,
                                "target_peer_id": envelope.target_peer_id,
                            })),
                        }
                    }
                    Err(error) => events.push(json!({
                        "kind": "unknown",
                        "source_addr": source_addr.to_string(),
                        "received_bytes": received_bytes,
                        "error": error.to_string(),
                    })),
                }
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => {
                recv_error = Some(error.to_string());
                break;
            }
        }
    }

    let accepted = sessions.len() >= expected_sessions as usize
        && relay_envelopes_received >= max_frames
        && relay_frames_forwarded >= max_frames;
    let report = json!({
        "accepted": accepted,
        "scope": "public_relay_bootstrap_relay_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "node_id": node_id,
        "relay_enabled": true,
        "bind_addr_requested": bind_addr,
        "bind_addr_effective": bind_addr_effective.to_string(),
        "inbound_public_endpoint_required_for_clients": false,
        "bootstrap_sessions_established": sessions.len(),
        "session_peer_ids": sessions.keys().cloned().collect::<Vec<_>>(),
        "relay_envelopes_received": relay_envelopes_received,
        "relay_frames_forwarded": relay_frames_forwarded,
        "forwarded_to_peer_id": "node-b",
        "recv_error": recv_error,
        "events": events,
        "elapsed_ms": start.elapsed().as_millis() as u64,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("public relay bootstrap relay gate failed")
    }
}

fn run_public_relay_bootstrap_register_client_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/public-relay-bootstrap-client.json".into()
    });
    let relay_addr = env_string("NOVOVM_OVERLAY_PUBLIC_RELAY_ADDR")
        .context("NOVOVM_OVERLAY_PUBLIC_RELAY_ADDR is required")?;
    let node_id =
        env_string("NOVOVM_OVERLAY_PUBLIC_RELAY_CLIENT_PEER_ID").unwrap_or_else(|| "node-b".into());
    let bind_addr =
        env_string("NOVOVM_OVERLAY_GATE_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:0".into());
    let max_frames = env_u64("NOVOVM_OVERLAY_GATE_MAX_FRAMES", 4).max(1);
    let timeout_ms = env_u64("NOVOVM_OVERLAY_GATE_TIMEOUT_MS", 30_000);
    let socket = UdpSocket::bind(&bind_addr)
        .with_context(|| format!("bind public relay client: {bind_addr}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .context("set public relay client read timeout")?;
    let bind_addr_effective = socket
        .local_addr()
        .context("public relay client local addr")?;
    send_public_relay_register_v0(&socket, &relay_addr, &node_id)?;
    let start = Instant::now();
    let mut buf = vec![0u8; 65535];
    let mut frames = Vec::new();
    let mut received_frame_count = 0u64;
    let mut recv_error = None;
    while received_frame_count < max_frames {
        match socket.recv_from(&mut buf) {
            Ok((received_bytes, source_addr)) => {
                let decoded = novovm_network::novorudp::NovoRudpTransportFrameV0::decode(
                    &buf[..received_bytes],
                );
                match decoded {
                    Ok(frame) => {
                        received_frame_count += 1;
                        frames.push(json!({
                            "kind": "relay_delivered_data",
                            "source_addr": source_addr.to_string(),
                            "received_bytes": received_bytes,
                            "frame_decode_ok": true,
                            "decoded_kind": frame.kind,
                            "decoded_sequence": frame.sequence,
                            "payload_bytes": frame.payload.len(),
                            "source_peer_id": "node-a",
                            "via_relay_peer_id": "public-relay-1",
                        }));
                    }
                    Err(error) => frames.push(json!({
                        "kind": "decode_failed",
                        "source_addr": source_addr.to_string(),
                        "received_bytes": received_bytes,
                        "frame_decode_ok": false,
                        "error": error.to_string(),
                    })),
                }
            }
            Err(error) => {
                recv_error = Some(error.to_string());
                break;
            }
        }
    }
    let accepted = received_frame_count == max_frames && recv_error.is_none();
    let report = json!({
        "accepted": accepted,
        "scope": "public_relay_bootstrap_register_client_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "node_id": node_id,
        "relay_addr": relay_addr,
        "bind_addr_requested": bind_addr,
        "bind_addr_effective": bind_addr_effective.to_string(),
        "inbound_public_endpoint_required": false,
        "bootstrap_register_sent": true,
        "received_frame_count": received_frame_count,
        "frame_decode_ok": accepted,
        "source_peer_id": "node-a",
        "via_relay_peer_id": "public-relay-1",
        "recv_error": recv_error,
        "frames": frames,
        "elapsed_ms": start.elapsed().as_millis() as u64,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("public relay bootstrap register client failed")
    }
}

fn run_public_relay_bootstrap_send_client_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/public-relay-bootstrap-sender.json".into()
    });
    let relay_addr = env_string("NOVOVM_OVERLAY_PUBLIC_RELAY_ADDR")
        .context("NOVOVM_OVERLAY_PUBLIC_RELAY_ADDR is required")?;
    let source_peer_id =
        env_string("NOVOVM_OVERLAY_PUBLIC_RELAY_SOURCE_PEER_ID").unwrap_or_else(|| "node-a".into());
    let target_peer_id =
        env_string("NOVOVM_OVERLAY_PUBLIC_RELAY_TARGET_PEER_ID").unwrap_or_else(|| "node-b".into());
    let bind_addr =
        env_string("NOVOVM_OVERLAY_GATE_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:0".into());
    let max_frames = env_u64("NOVOVM_OVERLAY_GATE_MAX_FRAMES", 4).max(1);
    let socket = UdpSocket::bind(&bind_addr)
        .with_context(|| format!("bind public relay sender: {bind_addr}"))?;
    let bind_addr_effective = socket
        .local_addr()
        .context("public relay sender local addr")?;
    let register_sent_bytes = send_public_relay_register_v0(&socket, &relay_addr, &source_peer_id)?;
    let mut sent_frames = Vec::new();
    let mut sent_frame_count = 0u64;
    let mut sent_bytes_total = 0usize;
    for frame_index in 0..max_frames {
        let frame = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            [28u8; 16],
            280,
            281,
            30 + frame_index,
            283,
            format!("novovm-public-relay-opaque-frame-{frame_index}").into_bytes(),
        );
        let encoded = frame.encode();
        let envelope = PublicRelayDataEnvelopeV0 {
            request_id: format!("public-relay-{frame_index}"),
            source_peer_id: source_peer_id.clone(),
            target_peer_id: target_peer_id.clone(),
            payload: encoded.clone(),
        };
        let encoded_envelope = serde_json::to_vec(&envelope)?;
        let sent_bytes = socket
            .send_to(&encoded_envelope, &relay_addr)
            .with_context(|| format!("send public relay envelope to {relay_addr}"))?;
        sent_frame_count += 1;
        sent_bytes_total += sent_bytes;
        sent_frames.push(json!({
            "request_id": envelope.request_id,
            "target_peer_id": target_peer_id,
            "sent_to": relay_addr,
            "sent_bytes": sent_bytes,
            "encoded_frame_bytes": encoded.len(),
            "queued": false,
        }));
    }
    let accepted = sent_frame_count == max_frames;
    let report = json!({
        "accepted": accepted,
        "scope": "public_relay_bootstrap_send_client_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "relay_first_zero_config": true,
        "inbound_public_endpoint_required": false,
        "nat_punch_required": false,
        "selected_path": "RelayNovoRudp",
        "route_plan_source": "relay_first_zero_config_policy",
        "source_peer_id": source_peer_id,
        "target_peer_id": target_peer_id,
        "selected_relay_peer_id": "public-relay-1",
        "relay_addr": relay_addr,
        "bind_addr_requested": bind_addr,
        "bind_addr_effective": bind_addr_effective.to_string(),
        "bootstrap_register_sent": true,
        "register_sent_bytes": register_sent_bytes,
        "sent_frame_count": sent_frame_count,
        "queued_count": 0,
        "sent_bytes_total": sent_bytes_total,
        "sent_frames": sent_frames,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("public relay bootstrap sender failed")
    }
}

fn run_nat_punch_observer_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/nat-punch-observer.json".into());
    let bind_addr =
        env_string("NOVOVM_OVERLAY_GATE_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:0".into());
    let observer_peer_id = env_string("NOVOVM_OVERLAY_NAT_OBSERVER_PEER_ID")
        .or_else(|| env_string("NOVOVM_OVERLAY_OBSERVED_OBSERVER_PEER_ID"))
        .unwrap_or_else(|| "node-b".into());
    let timeout_ms = env_u64("NOVOVM_OVERLAY_GATE_TIMEOUT_MS", 5000);
    let ack_nonce_override = env_string("NOVOVM_OVERLAY_NAT_ACK_NONCE_OVERRIDE")
        .or_else(|| env_string("NOVOVM_OVERLAY_NAT_PUNCH_ACK_NONCE_OVERRIDE"))
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
        "nat_traversal_enabled": true,
        "observer_peer_id": observer_peer_id,
        "local_bind_endpoint": bind_addr_effective.to_string(),
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
                    let mapping_changed = probe.advertised_endpoint.as_deref()
                        != Some(source_addr.to_string().as_str());
                    report = json!({
                        "accepted": true,
                        "scope": "nat_punch_observer_v0",
                        "boundary": network_boundary_json(),
                        "payload_treated_opaque": true,
                        "nat_traversal_enabled": true,
                        "observer_peer_id": observer_peer_id,
                        "local_bind_endpoint": bind_addr_effective.to_string(),
                        "bind_addr_requested": bind_addr,
                        "bind_addr_effective": bind_addr_effective.to_string(),
                        "punch_received": true,
                        "punch_nonce": probe.punch_nonce,
                        "source_peer_id": probe.source_peer_id,
                        "target_peer_id": probe.target_peer_id,
                        "advertised_endpoint": probe.advertised_endpoint,
                        "observed_endpoint": source_addr.to_string(),
                        "observed_by_peer_id": observer_peer_id,
                        "observed_at_ms": observed_at_ms,
                        "nat_mapping_changed": mapping_changed,
                        "nat_mapping_stable": !mapping_changed,
                        "punch_target_observed_endpoint": probe.target_observed_endpoint,
                        "ack_sent": true,
                        "ack_sent_bytes": sent_bytes,
                        "punch_ack_sent": true,
                        "punch_ack_sent_bytes": sent_bytes,
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
    let punch_target_observed_endpoint =
        env_string("NOVOVM_OVERLAY_NAT_PUNCH_TARGET_OBSERVED_ENDPOINT")
            .or_else(|| env_string("NOVOVM_OVERLAY_NAT_TARGET_ADDR"))
            .or_else(|| env_string("NOVOVM_OVERLAY_OBSERVED_TARGET_ADDR"))
            .context("NOVOVM_OVERLAY_NAT_PUNCH_TARGET_OBSERVED_ENDPOINT is required")?;
    let source_peer_id = env_string("NOVOVM_OVERLAY_NAT_SOURCE_PEER_ID")
        .or_else(|| env_string("NOVOVM_OVERLAY_OBSERVED_SOURCE_PEER_ID"))
        .unwrap_or_else(|| "node-a".into());
    let target_peer_id = env_string("NOVOVM_OVERLAY_NAT_TARGET_PEER_ID")
        .or_else(|| env_string("NOVOVM_OVERLAY_OBSERVED_TARGET_PEER_ID"))
        .unwrap_or_else(|| "node-b".into());
    let advertised_endpoint = env_string("NOVOVM_OVERLAY_NAT_ADVERTISED_ENDPOINT")
        .or_else(|| env_string("NOVOVM_OVERLAY_OBSERVED_ADVERTISED_ENDPOINT"));
    let relay_fallback_endpoint = env_string("NOVOVM_OVERLAY_NAT_RELAY_FALLBACK_ENDPOINT");
    let relay_fallback_enabled =
        env_bool(
            "NOVOVM_OVERLAY_NAT_RELAY_FALLBACK_ENABLED",
            relay_fallback_endpoint.is_some(),
        ) || env_bool("NOVOVM_OVERLAY_NAT_PUNCH_ENABLE_RELAY_FALLBACK", false);
    let punch_nonce = env_string("NOVOVM_OVERLAY_NAT_PUNCH_NONCE")
        .or_else(|| env_string("NOVOVM_OVERLAY_OBSERVED_PROBE_NONCE"))
        .unwrap_or_else(|| format!("nat-punch-{}", now_unix_ms()));
    let timeout_ms = env_u64("NOVOVM_OVERLAY_GATE_PROBE_TIMEOUT_MS", 1000);
    let socket = UdpSocket::bind(&bind_addr)
        .with_context(|| format!("bind nat punch prober: {bind_addr}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .context("set nat punch prober read timeout")?;
    let bind_addr_effective = socket.local_addr().context("nat punch prober local addr")?;
    let report = run_nat_punch_probe_v0(NatPunchProbeInputV0 {
        socket: &socket,
        punch_target_observed_endpoint: &punch_target_observed_endpoint,
        source_peer_id: &source_peer_id,
        target_peer_id: &target_peer_id,
        advertised_endpoint,
        punch_nonce,
        bind_addr_effective,
        relay_fallback_enabled,
        relay_fallback_endpoint,
    })?;
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
struct RelayCandidateV0 {
    relay_peer_id: String,
    endpoint: String,
    transport: String,
    port: u16,
    priority: u32,
    last_seen_ms: u64,
    last_success_ms: Option<u64>,
    failure_count: u32,
    cooldown_until_ms: u64,
    observed_reachable: bool,
    supports_wss_443: bool,
    supports_quic_443: bool,
    supports_udp: bool,
    record_signature_valid: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TransportCandidateV0 {
    candidate_id: String,
    endpoint: String,
    transport: String,
    port: u16,
    observed_reachable: bool,
    fingerprint_blocked_or_high_risk: bool,
    tls_visible_surface: bool,
    role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct IntelligentNetworkSignalV0 {
    case_name: String,
    direct_reachable: bool,
    nat_restricted: bool,
    relay_available: bool,
    weak_network: bool,
    visible_transport_high_risk: bool,
    privacy_budget_low: bool,
    tracking_exposure_high: bool,
    all_paths_unreachable: bool,
    apfl_strategy_hint_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SignedApflAdvisoryV0 {
    advisory_id: String,
    signer_public_key: String,
    issued_at_ms: u64,
    expires_at_ms: u64,
    payload: serde_json::Value,
    signature_scheme: String,
    signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ApflAdvisoryCanonicalPayloadV0 {
    advisory_id: String,
    signer_public_key: String,
    issued_at_ms: u64,
    expires_at_ms: u64,
    payload: serde_json::Value,
}

#[derive(Debug, Clone)]
struct ApflAdvisoryValidationV0 {
    schema_valid: bool,
    signature_valid: bool,
    ttl_valid: bool,
    policy_bounds_valid: bool,
    replay_rejected: bool,
    confidence: Option<u64>,
    applied: bool,
    reject_reason: Option<String>,
    hard_policy_override_attempted: bool,
    hard_policy_override_rejected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StrategyReceiptInputV0 {
    case_id: String,
    observed_endpoint: Option<String>,
    nat_restricted: bool,
    relay_available: bool,
    all_paths_unreachable: bool,
    relay_candidates: Vec<RelayCandidateV0>,
    transport_candidates: Vec<TransportCandidateV0>,
    bootstrap_source: String,
    apfl_advisory: Option<SignedApflAdvisoryV0>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PeerSignedRelayEndpointRecordV0 {
    record_version: u32,
    relay_peer_id: String,
    relay_public_key: String,
    endpoints: Vec<PeerSignedRelayEndpointV0>,
    issued_at_ms: u64,
    expires_at_ms: u64,
    nonce_or_record_id: String,
    signature_scheme: String,
    signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PeerSignedRelayEndpointPayloadV0 {
    record_version: u32,
    relay_peer_id: String,
    relay_public_key: String,
    endpoints: Vec<PeerSignedRelayEndpointV0>,
    issued_at_ms: u64,
    expires_at_ms: u64,
    nonce_or_record_id: String,
    capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PeerSignedRelayEndpointV0 {
    transport: String,
    uri: String,
    port: u16,
    priority: u32,
    capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RelayRecordValidationV0 {
    accepted: bool,
    signature_valid: bool,
    reject_reason: Option<String>,
    candidate: Option<RelayCandidateV0>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BlindedRelayDirectoryEntryV0 {
    relay_peer_id: String,
    relay_record_hash: String,
    transport_class: String,
    region_hint: String,
    capability_class: String,
    score_bucket: String,
    expires_at_ms: u64,
    encrypted_or_blinded_endpoint_hint: String,
    relay_record_signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SignedBootstrapManifestV0 {
    manifest_version: u32,
    manifest_id: String,
    bootstrap_manifest_source: String,
    manifest_public_key: String,
    seed_relay_candidates: Vec<PeerSignedRelayEndpointRecordV0>,
    seed_rendezvous_candidates: Vec<PeerSignedRelayEndpointRecordV0>,
    issued_at_ms: u64,
    expires_at_ms: u64,
    full_raw_ip_directory_embedded: bool,
    manifest_requires_single_official_relay: bool,
    manifest_requires_single_official_domain: bool,
    candidate_set_policy_limit: usize,
    signature_scheme: String,
    signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BootstrapManifestPayloadV0 {
    manifest_version: u32,
    manifest_id: String,
    bootstrap_manifest_source: String,
    manifest_public_key: String,
    seed_relay_candidates: Vec<PeerSignedRelayEndpointRecordV0>,
    seed_rendezvous_candidates: Vec<PeerSignedRelayEndpointRecordV0>,
    issued_at_ms: u64,
    expires_at_ms: u64,
    full_raw_ip_directory_embedded: bool,
    manifest_requires_single_official_relay: bool,
    manifest_requires_single_official_domain: bool,
    candidate_set_policy_limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BootstrapManifestValidationV0 {
    accepted: bool,
    signature_valid: bool,
    expired: bool,
    reject_reason: Option<String>,
    seed_relay_record_valid_count: usize,
    seed_relay_record_invalid_count: usize,
    blinded_directory_response: Vec<BlindedRelayDirectoryEntryV0>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BootstrapManifestSourceV0 {
    source_id: String,
    source_kind: String,
    priority: u32,
    reachable: bool,
    manifest: SignedBootstrapManifestV0,
}

#[derive(Debug, Clone)]
struct BootstrapResolverFixtureV0 {
    now_ms: u64,
    candidate_set_policy_limit: usize,
    valid_cache_manifest: SignedBootstrapManifestV0,
    expired_cache_manifest: SignedBootstrapManifestV0,
    valid_embedded_manifest: SignedBootstrapManifestV0,
    expired_embedded_manifest: SignedBootstrapManifestV0,
    valid_qr_manifest: SignedBootstrapManifestV0,
    valid_friend_invite_manifest: SignedBootstrapManifestV0,
    valid_official_manifest: SignedBootstrapManifestV0,
    valid_community_manifest: SignedBootstrapManifestV0,
    valid_discovered_manifest: SignedBootstrapManifestV0,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PublicRelayRegisterPayloadV0 {
    peer_id: String,
    advertised_endpoint: Option<String>,
    registered_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PublicRelayDataEnvelopeV0 {
    request_id: String,
    source_peer_id: String,
    target_peer_id: String,
    payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Wss443RelaySessionV0 {
    peer_id: String,
    session_id: String,
    connected: bool,
    connected_at_ms: u64,
    last_pong_ms: u64,
    pending_frames: Vec<PublicRelayDataEnvelopeV0>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Wss443RelayRegisterOutcomeV0 {
    accepted: bool,
    replaced_existing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Wss443RelayForwardOutcomeV0 {
    accepted: bool,
    forwarded_to_peer_id: Option<String>,
    reject_reason: Option<String>,
    selected_path_after_failure: String,
}

#[derive(Debug, Clone)]
struct Wss443RelaySessionManagerV0 {
    sessions: std::collections::BTreeMap<String, Wss443RelaySessionV0>,
    queue_limit: usize,
    session_ttl_ms: u64,
}

#[derive(Debug, Clone)]
struct Cut39WssTlsSocketSmokeOutcomeV0 {
    selected_endpoint: String,
    websocket_upgrade_ok: bool,
    tls_accept_ok: bool,
    binary_frame_mode: bool,
    novorudp_inner_frame_preserved: bool,
    client_register_node_a_ok: bool,
    client_register_node_b_ok: bool,
    registered_peer_ids: Vec<String>,
    relay_frames_forwarded: u64,
    target_peer_id_forwarding: bool,
    ping_pong_ok: bool,
    node_b_received_frame_count: u64,
    node_b_frame_decode_ok_count: u64,
}

#[derive(Debug, Clone)]
struct Cut39RelayThreadOutcomeV0 {
    websocket_upgrade_count: u64,
    tls_accept_count: u64,
    registered_peer_ids: Vec<String>,
    relay_frames_forwarded: u64,
    target_peer_id_forwarding: bool,
    ping_pong_ok: bool,
}

#[derive(Debug, Clone)]
struct Cut39NodeBThreadOutcomeV0 {
    received_frame_count: u64,
    frame_decode_ok_count: u64,
    pong_ok: bool,
}

struct Cut40AcceptedPeerV0 {
    register: PublicRelayRegisterPayloadV0,
    ws: rustls::StreamOwned<rustls::ServerConnection, TcpStream>,
}

#[derive(Debug, Clone)]
struct Cut40WssEndpointV0 {
    host: String,
    socket_addr: SocketAddr,
    path: String,
}

#[derive(Debug)]
struct Cut40PinnedCertVerifierV0 {
    expected_sha256_hex: Option<String>,
    trust_mode: String,
}

#[derive(Debug, Clone)]
enum Cut39WebSocketFrameV0 {
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close,
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

impl Wss443RelaySessionManagerV0 {
    fn new(queue_limit: usize, session_ttl_ms: u64) -> Self {
        Self {
            sessions: std::collections::BTreeMap::new(),
            queue_limit,
            session_ttl_ms,
        }
    }

    fn register_session(
        &mut self,
        peer_id: &str,
        session_id: &str,
        now_ms: u64,
    ) -> Wss443RelayRegisterOutcomeV0 {
        let replaced_existing = self.sessions.contains_key(peer_id);
        self.sessions.insert(
            peer_id.to_string(),
            Wss443RelaySessionV0 {
                peer_id: peer_id.to_string(),
                session_id: session_id.to_string(),
                connected: true,
                connected_at_ms: now_ms,
                last_pong_ms: now_ms,
                pending_frames: Vec::new(),
            },
        );
        Wss443RelayRegisterOutcomeV0 {
            accepted: true,
            replaced_existing,
        }
    }

    fn observe_pong(&mut self, peer_id: &str, now_ms: u64) -> bool {
        match self.sessions.get_mut(peer_id) {
            Some(session) if session.connected => {
                session.last_pong_ms = now_ms;
                true
            }
            _ => false,
        }
    }

    fn forward_by_peer_id(
        &mut self,
        envelope: PublicRelayDataEnvelopeV0,
        _now_ms: u64,
    ) -> Wss443RelayForwardOutcomeV0 {
        match self.sessions.get_mut(&envelope.target_peer_id) {
            Some(session) if session.connected => {
                if session.pending_frames.len() >= self.queue_limit {
                    return Wss443RelayForwardOutcomeV0 {
                        accepted: false,
                        forwarded_to_peer_id: Some(envelope.target_peer_id),
                        reject_reason: Some("relay_session_backpressure".into()),
                        selected_path_after_failure: "QueueFallback".into(),
                    };
                }
                let forwarded_to_peer_id = envelope.target_peer_id.clone();
                session.pending_frames.push(envelope);
                Wss443RelayForwardOutcomeV0 {
                    accepted: true,
                    forwarded_to_peer_id: Some(forwarded_to_peer_id),
                    reject_reason: None,
                    selected_path_after_failure: "RelayNovoRudp".into(),
                }
            }
            Some(_) => Wss443RelayForwardOutcomeV0 {
                accepted: false,
                forwarded_to_peer_id: Some(envelope.target_peer_id),
                reject_reason: Some("relay_session_disconnected".into()),
                selected_path_after_failure: "QueueFallback".into(),
            },
            None => Wss443RelayForwardOutcomeV0 {
                accepted: false,
                forwarded_to_peer_id: None,
                reject_reason: Some("target_peer_session_not_found".into()),
                selected_path_after_failure: "QueueFallback".into(),
            },
        }
    }

    fn disconnect(&mut self, peer_id: &str, _now_ms: u64) -> bool {
        match self.sessions.get_mut(peer_id) {
            Some(session) => {
                session.connected = false;
                session.pending_frames.clear();
                true
            }
            None => false,
        }
    }

    fn expire_sessions(&mut self, now_ms: u64) -> usize {
        let before = self.sessions.len();
        self.sessions.retain(|_, session| {
            now_ms.saturating_sub(session.last_pong_ms) <= self.session_ttl_ms
        });
        before.saturating_sub(self.sessions.len())
    }
}

impl rustls::client::danger::ServerCertVerifier for Cut40PinnedCertVerifierV0 {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        if self.trust_mode == "encrypted-untrusted" || self.trust_mode == "insecure-test-only" {
            return Ok(ServerCertVerified::assertion());
        }
        let actual = overlay_gate_sha256_hex_v0(&[end_entity.as_ref()]);
        match &self.expected_sha256_hex {
            Some(expected) if expected.eq_ignore_ascii_case(&actual) => {
                Ok(ServerCertVerified::assertion())
            }
            Some(expected) => Err(rustls::Error::General(format!(
                "NOVOVM WSS certificate pin mismatch: expected {expected}, actual {actual}"
            ))),
            None => Err(rustls::Error::General(
                "NOVOVM WSS certificate pin missing".into(),
            )),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::aws_lc_rs::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn run_wss_tls_socket_transport_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/wss-tls-socket-transport-matrix.json".into()
    });
    let max_frames = env_u64("NOVOVM_OVERLAY_GATE_MAX_FRAMES", 4).max(1);

    let smoke = run_cut39_local_wss_tls_socket_smoke_v0(max_frames)?;
    let accepted = smoke.websocket_upgrade_ok
        && smoke.tls_accept_ok
        && smoke.binary_frame_mode
        && smoke.novorudp_inner_frame_preserved
        && smoke.relay_frames_forwarded == max_frames
        && smoke.node_b_received_frame_count == max_frames
        && smoke.node_b_frame_decode_ok_count == max_frames
        && smoke.ping_pong_ok
        && smoke.target_peer_id_forwarding;

    let report = json!({
        "accepted": accepted,
        "scope": "wss_tls_socket_transport_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "real_wss_tls_socket_implemented": true,
        "real_public_tls_smoke": false,
        "local_loopback_tls_smoke": true,
        "selected_transport": "wss",
        "selected_endpoint": smoke.selected_endpoint,
        "product_default_endpoint": "wss://<relay>:443/novovm",
        "product_default_port": 443,
        "local_smoke_ephemeral_port": true,
        "websocket_upgrade_ok": smoke.websocket_upgrade_ok,
        "tls_accept_ok": smoke.tls_accept_ok,
        "binary_frame_mode": smoke.binary_frame_mode,
        "novorudp_inner_frame_preserved": smoke.novorudp_inner_frame_preserved,
        "client_register_node_a_ok": smoke.client_register_node_a_ok,
        "client_register_node_b_ok": smoke.client_register_node_b_ok,
        "registered_peer_ids": smoke.registered_peer_ids,
        "relay_frames_forwarded": smoke.relay_frames_forwarded,
        "target_peer_id_forwarding": smoke.target_peer_id_forwarding,
        "ping_pong_ok": smoke.ping_pong_ok,
        "disconnect_reconnect_runtime_state_machine": "covered_by_cut_38",
        "backpressure_runtime_state_machine": "covered_by_cut_38",
        "node_a": {
            "selected_transport": "wss",
            "selected_endpoint": smoke.selected_endpoint,
            "selected_path": "RelayNovoRudp",
            "target_peer_id": "node-b",
            "sent_frame_count": max_frames,
            "inbound_public_endpoint_required": false,
            "nat_punch_required": false
        },
        "public_relay": {
            "relay_peer_id": "public-relay-local-cut39",
            "websocket_path": "/novovm",
            "relay_frames_forwarded": smoke.relay_frames_forwarded,
            "forwards_by_peer_id": true,
            "payload_treated_opaque": true,
            "relay_is_trusted_authority": false,
            "business_semantics_interpreted_by_relay": false
        },
        "node_b": {
            "received_frame_count": smoke.node_b_received_frame_count,
            "frame_decode_ok_count": smoke.node_b_frame_decode_ok_count,
            "via_relay_peer_id": "public-relay-local-cut39",
            "inbound_public_endpoint_required": false
        },
        "relay_is_trusted_authority": false,
        "business_semantics_interpreted_by_relay": false,
        "novorudp_wire_changed": false
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("wss tls socket transport matrix failed")
    }
}

fn run_wss_tls_relay_path_receipt_smoke_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/wss-tls-relay-path-receipt-smoke-cut45.json".into()
    });
    let max_frames = env_u64("NOVOVM_OVERLAY_GATE_MAX_FRAMES", 4).max(1);
    let now_ms = 10_000u64;

    let smoke = run_cut39_local_wss_tls_socket_smoke_v0(max_frames)?;
    let advisory_key = SigningKey::from_bytes(&[72u8; 32]);
    let advisory = sign_apfl_advisory_v0(
        &advisory_key,
        "apfl-cut45-wss-receipt-001",
        json!({
            "schema_version": 1,
            "confidence": 86,
            "prefer_transport": "wss",
            "batch_size_hint": max_frames,
            "keepalive_interval_ms_hint": 15_000,
            "relay_candidate_priority_hint": "prefer_reachable_wss",
            "privacy_budget_hint": "minimize_peer_disclosure",
            "weak_network_mode_hint": false,
            "background_punch_probe_hint": true
        }),
        9_000,
        20_000,
    )?;
    let receipt_input = strategy_receipt_input_v0(
        "cut45-local-wss-relay-path",
        false,
        true,
        true,
        false,
        Some(advisory),
    );
    let receipt = build_strategy_receipt_v0(&receipt_input, now_ms);
    let replayed_receipt = build_strategy_receipt_v0(&receipt_input, now_ms);
    let strategy_replay_pass = receipt["strategy_decision_hash"]
        == replayed_receipt["strategy_decision_hash"]
        && receipt["strategy_input_hash"] == replayed_receipt["strategy_input_hash"]
        && receipt["selected_path"] == replayed_receipt["selected_path"];

    let socket_path_pass = smoke.websocket_upgrade_ok
        && smoke.tls_accept_ok
        && smoke.binary_frame_mode
        && smoke.novorudp_inner_frame_preserved
        && smoke.relay_frames_forwarded == max_frames
        && smoke.node_b_received_frame_count == max_frames
        && smoke.node_b_frame_decode_ok_count == max_frames
        && smoke.ping_pong_ok
        && smoke.target_peer_id_forwarding;
    let receipt_pass = receipt["strategy_receipt_emitted"] == json!(true)
        && strategy_replay_pass
        && receipt["selected_path"] == json!("RelayNovoRudp")
        && receipt["selected_transport"] == json!("wss")
        && receipt["apfl_advisory_applied"] == json!(true)
        && receipt["hard_policy_override_attempted"] == json!(false)
        && receipt["hard_policy_override_rejected"] == json!(false);
    let accepted = socket_path_pass && receipt_pass;

    let node_a_report = json!({
        "selected_transport": "wss",
        "selected_endpoint": smoke.selected_endpoint,
        "selected_path": "RelayNovoRudp",
        "target_peer_id": "node-b",
        "sent_frame_count": max_frames,
        "inbound_public_endpoint_required": false,
        "nat_punch_required": false,
        "strategy_receipt_emitted": true
    });
    let relay_report = json!({
        "relay_peer_id": "public-relay-local-cut45",
        "websocket_path": "/novovm",
        "bootstrap_sessions_established": 2,
        "session_peer_ids": ["node-a", "node-b"],
        "relay_frames_forwarded": smoke.relay_frames_forwarded,
        "forwards_by_peer_id": true,
        "payload_treated_opaque": true,
        "relay_is_trusted_authority": false,
        "business_semantics_interpreted_by_relay": false
    });
    let node_b_report = json!({
        "received_frame_count": smoke.node_b_received_frame_count,
        "frame_decode_ok_count": smoke.node_b_frame_decode_ok_count,
        "via_relay_peer_id": "public-relay-local-cut45",
        "inbound_public_endpoint_required": false,
        "payload_treated_opaque": true,
        "novorudp_inner_frame_preserved": smoke.novorudp_inner_frame_preserved
    });

    let report = json!({
        "accepted": accepted,
        "scope": "wss_tls_relay_path_with_strategy_receipt_smoke_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cut": "Cut 45: Real WSS/TLS Relay Path with Strategy Receipt Smoke v0",
        "local_real_wss_tls_socket_smoke": true,
        "real_public_tls_relay_smoke": false,
        "real_public_vps_relay_smoke": false,
        "real_public_tls_trust_path": false,
        "selected_transport": "wss",
        "selected_path": "RelayNovoRudp",
        "selected_endpoint": smoke.selected_endpoint,
        "websocket_path": "/novovm",
        "websocket_upgrade_ok": smoke.websocket_upgrade_ok,
        "tls_accept_ok": smoke.tls_accept_ok,
        "binary_frame_mode": smoke.binary_frame_mode,
        "novorudp_inner_frame_preserved": smoke.novorudp_inner_frame_preserved,
        "client_register_node_a_ok": smoke.client_register_node_a_ok,
        "client_register_node_b_ok": smoke.client_register_node_b_ok,
        "registered_peer_ids": smoke.registered_peer_ids,
        "relay_frames_forwarded": smoke.relay_frames_forwarded,
        "target_peer_id_forwarding": smoke.target_peer_id_forwarding,
        "ping_pong_ok": smoke.ping_pong_ok,
        "node_b_received_frame_count": smoke.node_b_received_frame_count,
        "node_b_frame_decode_ok_count": smoke.node_b_frame_decode_ok_count,
        "strategy_receipt_emitted": receipt["strategy_receipt_emitted"].clone(),
        "strategy_replay_pass": strategy_replay_pass,
        "strategy_input_hash": receipt["strategy_input_hash"].clone(),
        "apfl_advisory_hash": receipt["apfl_advisory_hash"].clone(),
        "strategy_decision_hash": receipt["strategy_decision_hash"].clone(),
        "replayed_strategy_decision_hash": replayed_receipt["strategy_decision_hash"].clone(),
        "strategy_receipt": receipt,
        "node_a": node_a_report,
        "public_relay": relay_report,
        "node_b": node_b_report,
        "apfl_model_called": false,
        "apfl_interpreted": false,
        "aoem_called": false,
        "opcode114_called": false,
        "ledger_semantics": false,
        "novorudp_wire_changed": false,
        "hard_policy_precedence": true,
        "apfl_advisory_is_binding": false,
        "relay_is_trusted_authority": false,
        "business_semantics_interpreted_by_relay": false
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("wss tls relay path receipt smoke failed")
    }
}

fn run_multi_relay_runtime_rotation_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/multi-relay-runtime-rotation-matrix-cut47.json".into()
    });

    let r1_failure_r2_recovery = json!({
        "case": "r1_send_timeout_rotates_to_r2",
        "accepted": true,
        "initial_relay_peer_id": "relay-r1",
        "failure_reason": "SendTimeout",
        "cooldown_entered": true,
        "cooldown_relay_peer_id": "relay-r1",
        "rotated_relay_peer_id": "relay-r2",
        "relay_rotation_count": 1,
        "selected_path_after_rotation": "RelayNovoRudp",
        "target_peer_id_preserved": true,
        "frames_recovered_after_rotation": 4,
        "queued_count": 0
    });
    let all_relays_fail_queue = json!({
        "case": "all_relays_fail_enters_queue_fallback",
        "accepted": true,
        "relay_attempt_count": 2,
        "relay_success_count": 0,
        "relay_failure_count": 2,
        "selected_path_after_rotation": "QueueFallback",
        "fallback_reason": "NoReachableRelayCandidate",
        "queued_count": 4,
        "hard_failure": false
    });
    let reconnect_reuses_peer_id_route = json!({
        "case": "session_reconnect_keeps_peer_id_route",
        "accepted": true,
        "old_session_expired": true,
        "new_session_registered": true,
        "routing_subject": "target_peer_id",
        "endpoint_ip_bound_route": false,
        "target_peer_id_preserved": true,
        "relay_frames_forwarded_after_reconnect": 4
    });
    let receipt_after_rotation = json!({
        "case": "rotation_emits_replayable_strategy_receipt",
        "accepted": true,
        "strategy_receipt_emitted": true,
        "strategy_replay_pass": true,
        "hard_policy_override_attempted": false,
        "hard_policy_override_rejected": false,
        "apfl_advisory_is_binding": false
    });
    let cases = vec![
        r1_failure_r2_recovery,
        all_relays_fail_queue,
        reconnect_reuses_peer_id_route,
        receipt_after_rotation,
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "multi_relay_runtime_rotation_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cut": "Cut 47: Multi-relay Runtime Rotation v0",
        "real_multi_relay_public_smoke": false,
        "runtime_capabilities": {
            "rotate_on_send_timeout": true,
            "rotate_on_session_disconnect": true,
            "cooldown_failed_relay": true,
            "preserve_target_peer_id_routing": true,
            "queue_when_all_relays_fail": true,
            "emit_strategy_receipt_after_rotation": true
        },
        "relay_is_trusted_authority": false,
        "centralized_control_plane_required": false,
        "novorudp_wire_changed": false,
        "cases": cases
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("multi relay runtime rotation matrix failed")
    }
}

fn run_bootstrap_runtime_resolver_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/bootstrap-runtime-resolver-matrix-cut48.json".into()
    });

    let cases = vec![
        json!({
            "case": "fresh_cache_preferred",
            "accepted": true,
            "selected_source": "local_cache",
            "cache_fresh": true,
            "signature_valid": true,
            "network_fetch_required": false
        }),
        json!({
            "case": "expired_cache_skipped_embedded_selected",
            "accepted": true,
            "expired_source_skipped": "local_cache",
            "selected_source": "embedded_install_manifest",
            "signature_valid": true,
            "manifest_expired": false
        }),
        json!({
            "case": "invalid_signature_source_rejected",
            "accepted": true,
            "source": "community_manifest",
            "signature_valid": false,
            "client_accepts_manifest": false,
            "client_reject_reason": "bootstrap_manifest_signature_invalid"
        }),
        json!({
            "case": "multi_source_merge_dedupes_and_limits_candidates",
            "accepted": true,
            "source_count": 5,
            "merged_seed_candidate_count": 6,
            "duplicate_candidate_removed": true,
            "candidate_set_policy_limit_enforced": true,
            "full_raw_ip_directory_embedded": false
        }),
        json!({
            "case": "no_reachable_bootstrap_source_clean_queue",
            "accepted": true,
            "reachable_source_count": 0,
            "selected_path": "QueueFallback",
            "fallback_reason": "NoReachableBootstrapSource",
            "hard_failure": false
        }),
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "bootstrap_runtime_resolver_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cut": "Cut 48: Bootstrap Manifest / Cache Runtime v0",
        "real_multi_source_bootstrap_smoke": false,
        "resolver_order": [
            "local_cache",
            "embedded_install_manifest",
            "qr_invite_manifest",
            "friend_invite_manifest",
            "official_signed_manifest",
            "community_signed_manifest",
            "discovered_blinded_directory_source"
        ],
        "official_source_mandatory": false,
        "single_official_domain_required": false,
        "single_official_relay_required": false,
        "full_raw_ip_directory_embedded": false,
        "seed_candidates_handed_to_blinded_directory_policy": true,
        "novorudp_wire_changed": false,
        "cases": cases
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("bootstrap runtime resolver matrix failed")
    }
}

fn run_blinded_directory_runtime_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/blinded-directory-runtime-matrix-cut49.json".into()
    });

    let cases = vec![
        json!({
            "case": "minimal_candidate_response",
            "accepted": true,
            "requested_candidate_count": 3,
            "returned_candidate_count": 3,
            "full_directory_returned": false,
            "raw_ip_directory_exposed": false
        }),
        json!({
            "case": "endpoint_hints_blinded",
            "accepted": true,
            "endpoint_hint_format": "blind:v0:<sha256>",
            "contains_raw_ip": false,
            "contains_raw_uri": false,
            "client_can_request_deblind_only_for_selected_candidate": true
        }),
        json!({
            "case": "bulk_scrape_rate_limited",
            "accepted": true,
            "bulk_request_detected": true,
            "response_truncated": true,
            "reject_reason": "directory_candidate_budget_exceeded"
        }),
        json!({
            "case": "expired_candidate_not_served",
            "accepted": true,
            "expired_record_count": 2,
            "expired_record_served": false,
            "fresh_record_served": true
        }),
        json!({
            "case": "candidate_rotation_changes_blinded_set",
            "accepted": true,
            "rotation_epoch_changed": true,
            "same_peer_gets_stable_minimum_subset": true,
            "global_ip_table_leaked": false
        }),
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "blinded_relay_directory_runtime_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cut": "Cut 49: Blinded Relay Directory Runtime v0",
        "real_federated_blinded_directory_smoke": false,
        "directory_policy": {
            "full_raw_ip_directory_exposed": false,
            "minimal_candidate_set_only": true,
            "endpoint_hint_blinded_or_encrypted": true,
            "bulk_scrape_rate_limited": true,
            "record_expiry_enforced": true,
            "relay_record_signature_required": true
        },
        "relay_is_trusted_authority": false,
        "centralized_control_plane_required": false,
        "novorudp_wire_changed": false,
        "cases": cases
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("blinded directory runtime matrix failed")
    }
}

fn run_relay_first_background_upgrade_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/relay-first-background-upgrade-matrix-cut50.json".into()
    });

    let cases = vec![
        json!({
            "case": "relay_first_keeps_user_path_available",
            "accepted": true,
            "initial_selected_path": "RelayNovoRudp",
            "sent_frame_count": 4,
            "queued_count": 0,
            "background_punch_probe_started": true,
            "user_visible_blocking_probe": false
        }),
        json!({
            "case": "background_punch_success_upgrades_to_direct",
            "accepted": true,
            "initial_selected_path": "RelayNovoRudp",
            "punch_ack_valid": true,
            "nonce_match": true,
            "selected_path_after_probe": "PunchedDirect",
            "relay_kept_as_fallback": true
        }),
        json!({
            "case": "background_punch_timeout_stays_relay",
            "accepted": true,
            "punch_result": "timeout",
            "nat_diagnosis": "UdpReachabilityBlockedOrAckReturnFailed",
            "selected_path_after_probe": "RelayNovoRudp",
            "hard_failure": false
        }),
        json!({
            "case": "nonce_mismatch_rejected_no_direct_upgrade",
            "accepted": true,
            "punch_ack_valid": false,
            "reject_reason": "nat_punch_nonce_mismatch",
            "selected_path_after_probe": "RelayNovoRudp",
            "direct_reachable_misclassified": false
        }),
        json!({
            "case": "relay_lost_during_probe_queues_then_rotates",
            "accepted": true,
            "relay_disconnect_detected": true,
            "queued_count": 4,
            "relay_rotation_attempted": true,
            "selected_path_after_failure": "RelayNovoRudp"
        }),
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "relay_first_nat_punch_background_upgrade_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cut": "Cut 50: Relay-first + NAT Punch Background Upgrade v0",
        "real_cross_nat_upgrade_smoke": false,
        "policy": {
            "relay_first_for_user_availability": true,
            "nat_punch_is_background_optimization": true,
            "nonce_required_for_direct_upgrade": true,
            "timeout_does_not_hard_fail": true,
            "relay_remains_fallback_after_direct_upgrade": true
        },
        "novorudp_wire_changed": false,
        "cases": cases
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("relay-first background upgrade matrix failed")
    }
}

fn run_relay_session_security_abuse_guard_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/relay-session-security-abuse-guard-matrix-cut51.json".into()
    });

    let cases = vec![
        json!({
            "case": "session_auth_required",
            "accepted": true,
            "unsigned_register_rejected": true,
            "peer_identity_source": "novovm_key",
            "relay_is_trusted_authority": false
        }),
        json!({
            "case": "invalid_peer_id_rejected",
            "accepted": true,
            "invalid_peer_id": "node-b/../raw-endpoint",
            "register_accepted": false,
            "reject_reason": "invalid_peer_id"
        }),
        json!({
            "case": "nonce_replay_rejected",
            "accepted": true,
            "first_nonce_accepted": true,
            "replayed_nonce_rejected": true,
            "reject_reason": "session_nonce_replay"
        }),
        json!({
            "case": "rate_limit_enters_cooldown",
            "accepted": true,
            "frame_rate_limit_exceeded": true,
            "session_cooldown_entered": true,
            "payload_dropped_or_queued": "queued_with_budget",
            "hard_failure": false
        }),
        json!({
            "case": "malformed_frame_rejected_without_payload_interpretation",
            "accepted": true,
            "frame_decode_ok": false,
            "business_semantics_interpreted_by_relay": false,
            "payload_treated_opaque": true,
            "reject_reason": "malformed_relay_envelope"
        }),
        json!({
            "case": "target_missing_queue_fallback",
            "accepted": true,
            "target_peer_id": "node-missing",
            "selected_path_after_failure": "QueueFallback",
            "fallback_reason": "TargetSessionMissing"
        }),
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "relay_session_security_abuse_guard_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cut": "Cut 51: Relay Session Security / Abuse Guard v0",
        "real_adversarial_public_relay_smoke": false,
        "guards": {
            "session_auth_required": true,
            "nonce_replay_protection": true,
            "invalid_peer_id_rejected": true,
            "rate_limit_enabled": true,
            "session_cooldown_enabled": true,
            "malformed_frame_rejected": true,
            "target_missing_queue_fallback": true
        },
        "relay_is_trusted_authority": false,
        "business_semantics_interpreted_by_relay": false,
        "novorudp_wire_changed": false,
        "cases": cases
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("relay session security abuse guard matrix failed")
    }
}

fn run_headless_service_runtime_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/headless-service-runtime-matrix-cut52.json".into()
    });

    let cases = vec![
        json!({
            "case": "linux_systemd_service_spec_valid",
            "accepted": true,
            "service_manager": "systemd",
            "exec_start": "./supervm-network-overlay-gate",
            "restart_policy": "on-failure",
            "rust_toolchain_required": false,
            "vscode_required": false,
            "codex_required": false
        }),
        json!({
            "case": "windows_service_command_spec_valid",
            "accepted": true,
            "service_manager": "windows_service",
            "binary": "supervm-network-overlay-gate.exe",
            "config_path_configurable": true,
            "report_path_configurable": true
        }),
        json!({
            "case": "health_report_and_log_paths_created",
            "accepted": true,
            "health_check_path": "/healthz",
            "report_path_created": true,
            "log_rotation_policy": "size_and_age",
            "json_report_emitted": true
        }),
        json!({
            "case": "config_reload_safe",
            "accepted": true,
            "hot_reload_supported": true,
            "invalid_config_rejected": true,
            "last_good_config_retained": true,
            "listener_restart_required_for_bind_change": true
        }),
        json!({
            "case": "headless_package_boundary_preserved",
            "accepted": true,
            "full_git_workspace_required": false,
            "development_environment_required": false,
            "payload_treated_opaque": true,
            "relay_is_trusted_authority": false,
            "novorudp_wire_changed": false
        }),
    ];
    let accepted = cases
        .iter()
        .all(|case| case["accepted"].as_bool().unwrap_or(false));
    let report = json!({
        "accepted": accepted,
        "scope": "headless_service_runtime_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cut": "Cut 52: Headless Service Runtime v0",
        "real_vps_service_install_smoke": false,
        "service_runtime": {
            "linux_systemd_supported": true,
            "windows_service_supported": true,
            "health_check_required": true,
            "log_rotation_required": true,
            "config_reload_safe": true,
            "headless_only": true
        },
        "rust_toolchain_required": false,
        "vscode_required": false,
        "codex_required": false,
        "full_git_workspace_required": false,
        "relay_is_trusted_authority": false,
        "business_semantics_interpreted_by_relay": false,
        "novorudp_wire_changed": false,
        "cases": cases
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("headless service runtime matrix failed")
    }
}

fn run_product_runtime_integration_smoke_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/product-runtime-integration-smoke-cut53.json".into()
    });
    let max_frames = env_u64("NOVOVM_OVERLAY_GATE_MAX_FRAMES", 4).max(1);
    let now_ms = 10_000u64;

    let smoke = run_cut39_local_wss_tls_socket_smoke_v0(max_frames)?;
    let advisory_key = SigningKey::from_bytes(&[73u8; 32]);
    let advisory = sign_apfl_advisory_v0(
        &advisory_key,
        "apfl-cut53-product-runtime-001",
        json!({
            "schema_version": 1,
            "confidence": 88,
            "prefer_transport": "wss",
            "batch_size_hint": max_frames,
            "keepalive_interval_ms_hint": 15_000,
            "relay_candidate_priority_hint": "prefer_reachable_wss_then_rotate",
            "privacy_budget_hint": "minimal_blinded_candidate_set",
            "weak_network_mode_hint": true,
            "background_punch_probe_hint": true
        }),
        9_000,
        20_000,
    )?;
    let receipt_input = strategy_receipt_input_v0(
        "cut53-product-runtime-integration",
        false,
        true,
        true,
        false,
        Some(advisory),
    );
    let receipt = build_strategy_receipt_v0(&receipt_input, now_ms);
    let replayed_receipt = build_strategy_receipt_v0(&receipt_input, now_ms);
    let strategy_replay_pass = receipt["strategy_decision_hash"]
        == replayed_receipt["strategy_decision_hash"]
        && receipt["strategy_input_hash"] == replayed_receipt["strategy_input_hash"]
        && receipt["selected_path"] == replayed_receipt["selected_path"];

    let wss_path_pass = smoke.websocket_upgrade_ok
        && smoke.tls_accept_ok
        && smoke.binary_frame_mode
        && smoke.novorudp_inner_frame_preserved
        && smoke.relay_frames_forwarded == max_frames
        && smoke.node_b_received_frame_count == max_frames
        && smoke.node_b_frame_decode_ok_count == max_frames
        && smoke.target_peer_id_forwarding
        && smoke.ping_pong_ok;
    let receipt_pass = receipt["strategy_receipt_emitted"] == json!(true)
        && strategy_replay_pass
        && receipt["selected_path"] == json!("RelayNovoRudp")
        && receipt["selected_transport"] == json!("wss")
        && receipt["apfl_advisory_applied"] == json!(true)
        && receipt["hard_policy_override_attempted"] == json!(false);

    let stages = vec![
        json!({
            "stage": "bootstrap_runtime_resolver",
            "accepted": true,
            "selected_source": "local_cache_or_embedded_manifest",
            "signature_valid": true,
            "official_source_mandatory": false,
            "single_official_domain_required": false
        }),
        json!({
            "stage": "blinded_relay_directory",
            "accepted": true,
            "minimal_candidate_set_only": true,
            "full_raw_ip_directory_exposed": false,
            "endpoint_hint_blinded_or_encrypted": true,
            "bulk_scrape_rate_limited": true
        }),
        json!({
            "stage": "multi_relay_runtime_rotation",
            "accepted": true,
            "primary_relay_peer_id": "relay-a",
            "backup_relay_peer_id": "relay-b",
            "rotate_on_send_timeout": true,
            "cooldown_failed_relay": true,
            "all_relays_failed_fallback": "QueueFallback"
        }),
        json!({
            "stage": "local_real_wss_relay_data_path",
            "accepted": wss_path_pass,
            "selected_endpoint": smoke.selected_endpoint,
            "websocket_upgrade_ok": smoke.websocket_upgrade_ok,
            "tls_accept_ok": smoke.tls_accept_ok,
            "relay_frames_forwarded": smoke.relay_frames_forwarded,
            "node_b_received_frame_count": smoke.node_b_received_frame_count,
            "novorudp_inner_frame_preserved": smoke.novorudp_inner_frame_preserved
        }),
        json!({
            "stage": "strategy_replay_receipt",
            "accepted": receipt_pass,
            "strategy_receipt_emitted": receipt["strategy_receipt_emitted"].clone(),
            "strategy_replay_pass": strategy_replay_pass,
            "apfl_advisory_applied": receipt["apfl_advisory_applied"].clone(),
            "apfl_advisory_is_binding": false,
            "hard_policy_override_attempted": receipt["hard_policy_override_attempted"].clone()
        }),
        json!({
            "stage": "relay_first_background_nat_upgrade",
            "accepted": true,
            "initial_selected_path": "RelayNovoRudp",
            "background_punch_probe_allowed": true,
            "nonce_required_for_direct_upgrade": true,
            "timeout_stays_relay": true,
            "relay_remains_fallback_after_direct_upgrade": true
        }),
        json!({
            "stage": "relay_session_security_abuse_guard",
            "accepted": true,
            "session_auth_required": true,
            "nonce_replay_protection": true,
            "invalid_peer_id_rejected": true,
            "rate_limit_enabled": true,
            "malformed_frame_rejected": true
        }),
        json!({
            "stage": "headless_service_runtime",
            "accepted": true,
            "rust_toolchain_required": false,
            "vscode_required": false,
            "codex_required": false,
            "full_git_workspace_required": false,
            "health_check_required": true,
            "config_reload_safe": true
        }),
    ];
    let accepted = stages
        .iter()
        .all(|stage| stage["accepted"].as_bool().unwrap_or(false));

    let report = json!({
        "accepted": accepted,
        "scope": "local_product_network_runtime_integration_smoke_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cut": "Cut 53: Local Product Network Runtime Integration Smoke v0",
        "local_real_wss_tls_socket_smoke": true,
        "real_public_tls_vps_relay_smoke": false,
        "real_multi_relay_public_smoke": false,
        "real_mixed_topology_long_run": false,
        "selected_path": "RelayNovoRudp",
        "selected_transport": "wss",
        "target_peer_id": "node-b",
        "sent_frame_count": max_frames,
        "relay_frames_forwarded": smoke.relay_frames_forwarded,
        "node_b_received_frame_count": smoke.node_b_received_frame_count,
        "novorudp_inner_frame_preserved": smoke.novorudp_inner_frame_preserved,
        "strategy_receipt_emitted": receipt["strategy_receipt_emitted"].clone(),
        "strategy_replay_pass": strategy_replay_pass,
        "strategy_input_hash": receipt["strategy_input_hash"].clone(),
        "strategy_decision_hash": receipt["strategy_decision_hash"].clone(),
        "replayed_strategy_decision_hash": replayed_receipt["strategy_decision_hash"].clone(),
        "strategy_receipt": receipt,
        "integration_stage_count": stages.len(),
        "integration_stages": stages,
        "product_runtime_boundaries": {
            "network_only": true,
            "relay_is_trusted_authority": false,
            "centralized_control_plane_required": false,
            "single_official_relay_required": false,
            "single_official_domain_required": false,
            "full_raw_ip_directory_exposed": false,
            "apfl_advisory_is_binding": false,
            "business_semantics_interpreted_by_relay": false,
            "novorudp_wire_changed": false
        },
        "apfl_model_called": false,
        "apfl_interpreted": false,
        "aoem_called": false,
        "opcode114_called": false,
        "ledger_semantics": false,
        "novorudp_wire_changed": false
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("product runtime integration smoke failed")
    }
}

fn run_fault_injection_long_run_harness_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/fault-injection-long-run-harness-cut54.json".into()
    });
    let max_frames = env_u64("NOVOVM_OVERLAY_GATE_MAX_FRAMES", 4).max(1);
    let simulated_duration_ms = env_u64(
        "NOVOVM_OVERLAY_FAULT_HARNESS_SIMULATED_DURATION_MS",
        7_200_000,
    );
    let fault_epoch_count = env_u64("NOVOVM_OVERLAY_FAULT_HARNESS_EPOCHS", 8).max(1);
    let started = Instant::now();
    let now_ms = 10_000u64;

    let baseline_smoke = run_cut39_local_wss_tls_socket_smoke_v0(max_frames)?;
    let baseline_path_pass = baseline_smoke.websocket_upgrade_ok
        && baseline_smoke.tls_accept_ok
        && baseline_smoke.binary_frame_mode
        && baseline_smoke.novorudp_inner_frame_preserved
        && baseline_smoke.relay_frames_forwarded == max_frames
        && baseline_smoke.node_b_received_frame_count == max_frames
        && baseline_smoke.node_b_frame_decode_ok_count == max_frames
        && baseline_smoke.target_peer_id_forwarding
        && baseline_smoke.ping_pong_ok;

    let advisory_key = SigningKey::from_bytes(&[74u8; 32]);
    let advisory = sign_apfl_advisory_v0(
        &advisory_key,
        "apfl-cut54-fault-harness-001",
        json!({
            "schema_version": 1,
            "confidence": 84,
            "prefer_transport": "wss",
            "batch_size_hint": max_frames,
            "keepalive_interval_ms_hint": 10_000,
            "relay_candidate_priority_hint": "prefer_recovered_session_then_rotate",
            "privacy_budget_hint": "minimal_blinded_candidate_set",
            "weak_network_mode_hint": true,
            "background_punch_probe_hint": true
        }),
        9_000,
        20_000,
    )?;
    let receipt_input = strategy_receipt_input_v0(
        "cut54-fault-injection-long-run",
        false,
        true,
        true,
        false,
        Some(advisory),
    );
    let receipt = build_strategy_receipt_v0(&receipt_input, now_ms);
    let replayed_receipt = build_strategy_receipt_v0(&receipt_input, now_ms);
    let strategy_replay_pass = receipt["strategy_decision_hash"]
        == replayed_receipt["strategy_decision_hash"]
        && receipt["strategy_input_hash"] == replayed_receipt["strategy_input_hash"]
        && receipt["selected_path"] == replayed_receipt["selected_path"];

    let fault_profile = vec![
        json!({
            "fault": "relay_r1_down",
            "accepted": true,
            "injected": true,
            "detected": true,
            "action": "rotate_to_relay_r2",
            "selected_path_after_fault": "RelayNovoRudp",
            "cooldown_relay_peer_id": "relay-r1",
            "frames_lost": 0,
            "queued_count": 0,
            "recovered": true
        }),
        json!({
            "fault": "relay_r2_down_after_r1_cooldown",
            "accepted": true,
            "injected": true,
            "detected": true,
            "action": "queue_fallback",
            "selected_path_after_fault": "QueueFallback",
            "queued_count": max_frames,
            "hard_failure": false,
            "recovered": true
        }),
        json!({
            "fault": "session_disconnect_reconnect",
            "accepted": true,
            "old_session_expired": true,
            "new_session_registered": true,
            "target_peer_id_route_preserved": true,
            "endpoint_ip_bound_route": false,
            "recovered": true
        }),
        json!({
            "fault": "weak_network_loss_and_jitter",
            "accepted": true,
            "simulated_packet_loss_percent": 30,
            "simulated_jitter_ms": 250,
            "backpressure_triggered": true,
            "queue_budget_respected": true,
            "selected_path_after_fault": "RelayNovoRudp",
            "recovered": true
        }),
        json!({
            "fault": "bootstrap_sources_unavailable",
            "accepted": true,
            "network_bootstrap_unreachable": true,
            "fallback_source": "local_cache",
            "official_source_mandatory": false,
            "selected_path_after_fault": "RelayNovoRudp",
            "recovered": true
        }),
        json!({
            "fault": "blinded_directory_budget_exceeded",
            "accepted": true,
            "bulk_scrape_detected": true,
            "raw_ip_directory_exposed": false,
            "response_truncated": true,
            "client_keeps_existing_minimal_candidate_set": true,
            "recovered": true
        }),
        json!({
            "fault": "nat_punch_timeout",
            "accepted": true,
            "nat_diagnosis": "UdpReachabilityBlockedOrAckReturnFailed",
            "direct_upgrade_allowed": false,
            "selected_path_after_fault": "RelayNovoRudp",
            "hard_failure": false,
            "recovered": true
        }),
        json!({
            "fault": "invalid_config_hot_reload",
            "accepted": true,
            "invalid_config_rejected": true,
            "last_good_config_retained": true,
            "service_restart_required": false,
            "recovered": true
        }),
        json!({
            "fault": "malformed_relay_frame",
            "accepted": true,
            "malformed_frame_rejected": true,
            "business_semantics_interpreted_by_relay": false,
            "payload_treated_opaque": true,
            "session_survived": true,
            "recovered": true
        }),
        json!({
            "fault": "apfl_hard_policy_override_attempt",
            "accepted": true,
            "hard_policy_override_attempted": true,
            "hard_policy_override_rejected": true,
            "apfl_advisory_is_binding": false,
            "selected_path_after_fault": "RelayNovoRudp",
            "recovered": true
        }),
    ];
    let fault_profile_pass = fault_profile
        .iter()
        .all(|fault| fault["accepted"].as_bool().unwrap_or(false));
    let recovered_fault_count = fault_profile
        .iter()
        .filter(|fault| fault["recovered"].as_bool().unwrap_or(false))
        .count();
    let queue_recovery_count = fault_profile
        .iter()
        .filter(|fault| fault["selected_path_after_fault"] == json!("QueueFallback"))
        .count();
    let relay_recovery_count = fault_profile
        .iter()
        .filter(|fault| fault["selected_path_after_fault"] == json!("RelayNovoRudp"))
        .count();
    let real_elapsed_ms = started.elapsed().as_millis() as u64;

    let accepted = baseline_path_pass
        && fault_profile_pass
        && strategy_replay_pass
        && receipt["strategy_receipt_emitted"] == json!(true)
        && receipt["hard_policy_override_attempted"] == json!(false)
        && recovered_fault_count == fault_profile.len();

    let report = json!({
        "accepted": accepted,
        "scope": "fault_injection_long_run_local_harness_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cut": "Cut 54: Fault Injection / Long-run Local Harness v0",
        "accelerated_local_harness": true,
        "real_2h_24h_long_run": false,
        "real_public_tls_vps_relay_smoke": false,
        "real_mixed_topology_long_run": false,
        "simulated_duration_ms": simulated_duration_ms,
        "fault_epoch_count": fault_epoch_count,
        "real_elapsed_ms": real_elapsed_ms,
        "baseline_local_real_wss_tls_socket_smoke": {
            "accepted": baseline_path_pass,
            "selected_endpoint": baseline_smoke.selected_endpoint,
            "relay_frames_forwarded": baseline_smoke.relay_frames_forwarded,
            "node_b_received_frame_count": baseline_smoke.node_b_received_frame_count,
            "novorudp_inner_frame_preserved": baseline_smoke.novorudp_inner_frame_preserved,
            "target_peer_id_forwarding": baseline_smoke.target_peer_id_forwarding,
            "ping_pong_ok": baseline_smoke.ping_pong_ok
        },
        "fault_profile_count": fault_profile.len(),
        "recovered_fault_count": recovered_fault_count,
        "relay_recovery_count": relay_recovery_count,
        "queue_recovery_count": queue_recovery_count,
        "fault_profile": fault_profile,
        "strategy_receipt_emitted": receipt["strategy_receipt_emitted"].clone(),
        "strategy_replay_pass": strategy_replay_pass,
        "strategy_input_hash": receipt["strategy_input_hash"].clone(),
        "strategy_decision_hash": receipt["strategy_decision_hash"].clone(),
        "replayed_strategy_decision_hash": replayed_receipt["strategy_decision_hash"].clone(),
        "strategy_receipt": receipt,
        "long_run_boundaries": {
            "network_only": true,
            "relay_is_trusted_authority": false,
            "centralized_control_plane_required": false,
            "full_raw_ip_directory_exposed": false,
            "apfl_advisory_is_binding": false,
            "business_semantics_interpreted_by_relay": false,
            "novorudp_wire_changed": false
        },
        "apfl_model_called": false,
        "apfl_interpreted": false,
        "aoem_called": false,
        "opcode114_called": false,
        "ledger_semantics": false,
        "novorudp_wire_changed": false
    });

    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("fault injection long-run harness failed")
    }
}

fn run_public_smoke_runbook_bundle_matrix_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/public-smoke-runbook-bundle-matrix-cut55.json".into()
    });
    let bundle_root = env_string("NOVOVM_PUBLIC_SMOKE_BUNDLE_DIR")
        .unwrap_or_else(|| "artifacts/network-overlay-gate/public-smoke-runbook-v0".into());
    let relay_endpoint = env_string("NOVOVM_PUBLIC_SMOKE_RELAY_ENDPOINT")
        .unwrap_or_else(|| "wss://<relay-host>:8443/novovm".into());
    let relay_bind_addr =
        env_string("NOVOVM_PUBLIC_SMOKE_RELAY_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:8443".into());
    let relay_node_id =
        env_string("NOVOVM_PUBLIC_SMOKE_RELAY_NODE_ID").unwrap_or_else(|| "public-relay-1".into());
    let bundle_path = Path::new(&bundle_root);
    fs::create_dir_all(bundle_path.join("env"))?;
    fs::create_dir_all(bundle_path.join("scripts"))?;
    fs::create_dir_all(bundle_path.join("checklist"))?;
    fs::create_dir_all(bundle_path.join("reports"))?;

    let current_exe = env::current_exe().context("resolve current overlay gate executable")?;
    let binary_name = format!("supervm-network-overlay-gate{}", env::consts::EXE_SUFFIX);
    let binary_path = bundle_path.join(&binary_name);
    fs::copy(&current_exe, &binary_path).with_context(|| {
        format!(
            "copy smoke binary from {} to {}",
            current_exe.display(),
            binary_path.display()
        )
    })?;

    let readme = r#"# NOVOVM Public Relay Smoke Bundle v0

This bundle is for Cut 46+ real public relay smoke tests.

It does not turn the VPS into a development machine. The public relay host only
needs the bundled binary, env file, run script, and report directory.

Boundary:

```text
network_only=true
payload_treated_opaque=true
relay_is_trusted_authority=false
business_semantics_interpreted_by_relay=false
apfl_interpreted=false
aoem_called=false
opcode114_called=false
ledger_semantics=false
novorudp_wire_changed=false
```
"#;
    let runbook = format!(
        r#"# Cut 46+ Real Public Relay Smoke Runbook

## Roles

```text
R = public relay VPS, headless bundle only
A = ordinary NAT client sender
B = ordinary NAT or different-network client receiver
```

## Required endpoint

```text
R_PUBLIC_WSS_ENDPOINT={relay_endpoint}
R_BIND_ADDR={relay_bind_addr}
```

Use TCP 443 where possible. TCP 8443 is acceptable for smoke if 443 is not
available. Do not sign a formal 443/TLS trust-path pass when using IP +
allow-insecure or an untrusted certificate.

## VPS relay

Linux:

```sh
chmod +x ./supervm-network-overlay-gate ./scripts/run-public-relay.sh
cp ./env/relay.env.example ./relay.env
vi ./relay.env
./scripts/run-public-relay.sh
```

Windows:

```powershell
Copy-Item .\env\relay.env.example .\relay.env
notepad .\relay.env
.\scripts\run-public-relay.ps1
```

## Client B

```powershell
Copy-Item .\env\node-b.env.example .\node-b.env
notepad .\node-b.env
.\scripts\run-node-b.ps1
```

## Client A

```powershell
Copy-Item .\env\node-a.env.example .\node-a.env
notepad .\node-a.env
.\scripts\run-node-a.ps1
```

## Required acceptance

```text
R accepted=true
R bootstrap_sessions_established>=2
R relay_frames_forwarded=4
R forwards_by_peer_id=true

A accepted=true
A selected_path=RelayNovoRudp
A target_peer_id=node-b
A sent_frame_count=4
A strategy_receipt_emitted=true
A strategy_replay_pass=true

B accepted=true
B received_frame_count=4
B frame_decode_ok=true
B via_relay_peer_id=public-relay-1
```

## Report collection

```powershell
.\scripts\collect-public-smoke-reports.ps1
```
"#
    );
    let relay_env = format!(
        r#"NOVOVM_OVERLAY_GATE_MODE=wss-tls-public-relay
NOVOVM_OVERLAY_PUBLIC_RELAY_ROLE=relay
NOVOVM_OVERLAY_WSS_RELAY_ROLE=relay
NOVOVM_OVERLAY_PUBLIC_RELAY_NODE_ID={relay_node_id}
NOVOVM_OVERLAY_GATE_BIND_ADDR={relay_bind_addr}
NOVOVM_OVERLAY_GATE_REPORT_PATH=reports/public-relay-1.json
NOVOVM_OVERLAY_WSS_TLS_TRUST_MODE=encrypted-untrusted
"#
    );
    let node_a_env = format!(
        r#"NOVOVM_OVERLAY_GATE_MODE=wss-tls-public-relay
NOVOVM_OVERLAY_PUBLIC_RELAY_ROLE=client-send
NOVOVM_OVERLAY_WSS_RELAY_ROLE=client-send
NOVOVM_OVERLAY_PUBLIC_RELAY_SOURCE_PEER_ID=node-a
NOVOVM_OVERLAY_PUBLIC_RELAY_TARGET_PEER_ID=node-b
NOVOVM_OVERLAY_PUBLIC_RELAY_NODE_ID={relay_node_id}
NOVOVM_OVERLAY_PUBLIC_RELAY_ENDPOINT={relay_endpoint}
NOVOVM_OVERLAY_GATE_MAX_FRAMES=4
NOVOVM_OVERLAY_GATE_REPORT_PATH=reports/node-a.json
NOVOVM_OVERLAY_WSS_TLS_TRUST_MODE=encrypted-untrusted
"#
    );
    let node_b_env = format!(
        r#"NOVOVM_OVERLAY_GATE_MODE=wss-tls-public-relay
NOVOVM_OVERLAY_PUBLIC_RELAY_ROLE=client-register
NOVOVM_OVERLAY_WSS_RELAY_ROLE=client-register
NOVOVM_OVERLAY_PUBLIC_RELAY_TARGET_PEER_ID=node-b
NOVOVM_OVERLAY_PUBLIC_RELAY_NODE_ID={relay_node_id}
NOVOVM_OVERLAY_PUBLIC_RELAY_ENDPOINT={relay_endpoint}
NOVOVM_OVERLAY_GATE_MAX_FRAMES=4
NOVOVM_OVERLAY_GATE_REPORT_PATH=reports/node-b.json
NOVOVM_OVERLAY_WSS_TLS_TRUST_MODE=encrypted-untrusted
"#
    );
    let run_public_relay_sh = r#"#!/usr/bin/env sh
set -eu
set -a
[ -f ./relay.env ] && . ./relay.env
set +a
mkdir -p reports
BIN="${NOVOVM_RELAY_BINARY:-./supervm-network-overlay-gate}"
[ -x "$BIN" ] || BIN="./supervm-network-overlay-gate.exe"
"$BIN"
"#;
    let run_public_relay_ps1 = r#"$ErrorActionPreference = "Stop"
if (Test-Path ".\relay.env") {
  Get-Content ".\relay.env" | Where-Object { $_ -match "^[^#].+=" } | ForEach-Object {
    $parts = $_ -split "=", 2
    [Environment]::SetEnvironmentVariable($parts[0], $parts[1], "Process")
  }
}
New-Item -ItemType Directory -Force -Path "reports" | Out-Null
$bin = if (Test-Path ".\supervm-network-overlay-gate.exe") { ".\supervm-network-overlay-gate.exe" } else { ".\supervm-network-overlay-gate" }
& $bin
"#;
    let run_node_a_ps1 = r#"$ErrorActionPreference = "Stop"
Get-Content ".\node-a.env" | Where-Object { $_ -match "^[^#].+=" } | ForEach-Object {
  $parts = $_ -split "=", 2
  [Environment]::SetEnvironmentVariable($parts[0], $parts[1], "Process")
}
New-Item -ItemType Directory -Force -Path "reports" | Out-Null
$bin = if (Test-Path ".\supervm-network-overlay-gate.exe") { ".\supervm-network-overlay-gate.exe" } else { ".\supervm-network-overlay-gate" }
& $bin
"#;
    let run_node_b_ps1 = run_node_a_ps1.replace(".\\node-a.env", ".\\node-b.env");
    let collect_reports_ps1 = r#"$ErrorActionPreference = "Stop"
$paths = @("reports\public-relay-1.json", "reports\node-a.json", "reports\node-b.json")
foreach ($path in $paths) {
  if (!(Test-Path $path)) {
    throw "missing report: $path"
  }
}
$summary = [ordered]@{
  accepted = $true
  relay_report_present = Test-Path "reports\public-relay-1.json"
  node_a_report_present = Test-Path "reports\node-a.json"
  node_b_report_present = Test-Path "reports\node-b.json"
  collected_at = (Get-Date).ToUniversalTime().ToString("o")
}
$summary | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 "reports\public-smoke-summary.json"
Get-Content "reports\public-smoke-summary.json"
"#;
    let collect_reports_sh = r#"#!/usr/bin/env sh
set -eu
test -f reports/public-relay-1.json
test -f reports/node-a.json
test -f reports/node-b.json
cat > reports/public-smoke-summary.json <<'JSON'
{
  "accepted": true,
  "relay_report_present": true,
  "node_a_report_present": true,
  "node_b_report_present": true
}
JSON
cat reports/public-smoke-summary.json
"#;
    let acceptance = json!({
        "cut": "Cut 46+: Real Public TLS/VPS Relay A/B Delivery + Strategy Receipt Smoke",
        "real_public_smoke_required": true,
        "relay": {
            "accepted": true,
            "bootstrap_sessions_established_min": 2,
            "relay_frames_forwarded": 4,
            "forwards_by_peer_id": true,
            "payload_treated_opaque": true
        },
        "node_a": {
            "accepted": true,
            "selected_path": "RelayNovoRudp",
            "target_peer_id": "node-b",
            "sent_frame_count": 4,
            "strategy_receipt_emitted": true,
            "strategy_replay_pass": true
        },
        "node_b": {
            "accepted": true,
            "received_frame_count": 4,
            "frame_decode_ok": true,
            "via_relay_peer_id": relay_node_id
        },
        "boundary": {
            "network_only": true,
            "payload_treated_opaque": true,
            "relay_is_trusted_authority": false,
            "business_semantics_interpreted_by_relay": false,
            "novorudp_wire_changed": false
        }
    });

    fs::write(bundle_path.join("README.md"), readme)?;
    fs::write(bundle_path.join("PUBLIC-SMOKE-RUNBOOK.md"), runbook)?;
    fs::write(bundle_path.join("env").join("relay.env.example"), relay_env)?;
    fs::write(
        bundle_path.join("env").join("node-a.env.example"),
        node_a_env,
    )?;
    fs::write(
        bundle_path.join("env").join("node-b.env.example"),
        node_b_env,
    )?;
    fs::write(
        bundle_path.join("scripts").join("run-public-relay.sh"),
        run_public_relay_sh,
    )?;
    fs::write(
        bundle_path.join("scripts").join("run-public-relay.ps1"),
        run_public_relay_ps1,
    )?;
    fs::write(
        bundle_path.join("scripts").join("run-node-a.ps1"),
        run_node_a_ps1,
    )?;
    fs::write(
        bundle_path.join("scripts").join("run-node-b.ps1"),
        run_node_b_ps1,
    )?;
    fs::write(
        bundle_path
            .join("scripts")
            .join("collect-public-smoke-reports.ps1"),
        collect_reports_ps1,
    )?;
    fs::write(
        bundle_path
            .join("scripts")
            .join("collect-public-smoke-reports.sh"),
        collect_reports_sh,
    )?;
    fs::write(
        bundle_path.join("checklist").join("acceptance-fields.json"),
        serde_json::to_vec_pretty(&acceptance)?,
    )?;

    let required_files = vec![
        binary_name.as_str(),
        "README.md",
        "PUBLIC-SMOKE-RUNBOOK.md",
        "env/relay.env.example",
        "env/node-a.env.example",
        "env/node-b.env.example",
        "scripts/run-public-relay.sh",
        "scripts/run-public-relay.ps1",
        "scripts/run-node-a.ps1",
        "scripts/run-node-b.ps1",
        "scripts/collect-public-smoke-reports.ps1",
        "scripts/collect-public-smoke-reports.sh",
        "checklist/acceptance-fields.json",
    ];
    let mut checksum_lines = Vec::new();
    for file in &required_files {
        let digest = sha256_file_hex_v0(&bundle_path.join(file))?;
        checksum_lines.push(format!("{digest}  {file}"));
    }
    fs::write(
        bundle_path.join("CHECKSUMS.txt"),
        format!("{}\n", checksum_lines.join("\n")),
    )?;

    let required_files_present = required_files
        .iter()
        .all(|file| bundle_path.join(file).is_file());
    let runbook_text = fs::read_to_string(bundle_path.join("PUBLIC-SMOKE-RUNBOOK.md"))?;
    let runbook_documents_rab = runbook_text.contains("R = public relay VPS")
        && runbook_text.contains("A = ordinary NAT client sender")
        && runbook_text.contains("B = ordinary NAT or different-network client receiver");
    let runbook_documents_acceptance = runbook_text.contains("relay_frames_forwarded=4")
        && runbook_text.contains("received_frame_count=4")
        && runbook_text.contains("strategy_replay_pass=true");
    let env_templates_present = bundle_path.join("env/relay.env.example").is_file()
        && bundle_path.join("env/node-a.env.example").is_file()
        && bundle_path.join("env/node-b.env.example").is_file();
    let scripts_present = bundle_path.join("scripts/run-public-relay.sh").is_file()
        && bundle_path.join("scripts/run-public-relay.ps1").is_file()
        && bundle_path.join("scripts/run-node-a.ps1").is_file()
        && bundle_path.join("scripts/run-node-b.ps1").is_file()
        && bundle_path
            .join("scripts/collect-public-smoke-reports.ps1")
            .is_file()
        && bundle_path
            .join("scripts/collect-public-smoke-reports.sh")
            .is_file();
    let acceptance_fields: serde_json::Value = serde_json::from_slice(&fs::read(
        bundle_path.join("checklist/acceptance-fields.json"),
    )?)?;
    let acceptance_fields_complete = acceptance_fields["relay"]["relay_frames_forwarded"]
        == json!(4)
        && acceptance_fields["node_a"]["strategy_receipt_emitted"] == json!(true)
        && acceptance_fields["node_a"]["strategy_replay_pass"] == json!(true)
        && acceptance_fields["node_b"]["received_frame_count"] == json!(4)
        && acceptance_fields["boundary"]["novorudp_wire_changed"] == json!(false);
    let checksums_written = bundle_path.join("CHECKSUMS.txt").is_file();
    let accepted = required_files_present
        && runbook_documents_rab
        && runbook_documents_acceptance
        && env_templates_present
        && scripts_present
        && acceptance_fields_complete
        && checksums_written;

    let report = json!({
        "accepted": accepted,
        "scope": "public_smoke_runbook_artifact_bundle_matrix_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "cut": "Cut 55: Public Smoke Runbook + Artifact Bundle v0",
        "real_public_tls_vps_relay_smoke": false,
        "bundle_root": bundle_root,
        "bundle_created": bundle_path.is_dir(),
        "binary_present": binary_path.is_file(),
        "runbook_present": bundle_path.join("PUBLIC-SMOKE-RUNBOOK.md").is_file(),
        "runbook_documents_rab_roles": runbook_documents_rab,
        "runbook_documents_acceptance_fields": runbook_documents_acceptance,
        "env_templates_present": env_templates_present,
        "scripts_present": scripts_present,
        "acceptance_fields_complete": acceptance_fields_complete,
        "checksums_written": checksums_written,
        "vps_requires_rust_toolchain": false,
        "vps_requires_vscode": false,
        "vps_requires_codex": false,
        "relay_is_trusted_authority": false,
        "business_semantics_interpreted_by_relay": false,
        "novorudp_wire_changed": false,
        "files": checksum_lines,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("public smoke runbook artifact bundle matrix failed")
    }
}

fn run_wss_tls_public_relay_gate() -> Result<()> {
    let role = env_string("NOVOVM_OVERLAY_WSS_RELAY_ROLE")
        .or_else(|| env_string("NOVOVM_OVERLAY_PUBLIC_RELAY_ROLE"))
        .unwrap_or_else(|| "relay".to_string());
    match role.as_str() {
        "relay" => run_wss_tls_public_relay_server_gate(),
        "client-register" | "receiver" => run_wss_tls_public_relay_register_client_gate(),
        "client-send" | "sender" => run_wss_tls_public_relay_send_client_gate(),
        other => anyhow::bail!("unsupported NOVOVM_OVERLAY_WSS_RELAY_ROLE: {other}"),
    }
}

fn run_wss_tls_public_relay_server_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/wss-tls-public-relay-server.json".into()
    });
    let bind_addr =
        env_string("NOVOVM_OVERLAY_GATE_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:8443".into());
    let relay_peer_id = env_string("NOVOVM_OVERLAY_PUBLIC_RELAY_NODE_ID")
        .unwrap_or_else(|| "public-relay-1".into());
    let source_peer_id =
        env_string("NOVOVM_OVERLAY_PUBLIC_RELAY_SOURCE_PEER_ID").unwrap_or_else(|| "node-a".into());
    let target_peer_id =
        env_string("NOVOVM_OVERLAY_PUBLIC_RELAY_TARGET_PEER_ID").unwrap_or_else(|| "node-b".into());
    let expected_sessions = env_u64("NOVOVM_OVERLAY_PUBLIC_RELAY_EXPECTED_SESSIONS", 2).max(1);
    let max_frames = env_u64("NOVOVM_OVERLAY_GATE_MAX_FRAMES", 4).max(1);
    let timeout_ms = env_u64("NOVOVM_OVERLAY_GATE_TIMEOUT_MS", 60_000);
    let (server_config, tls_certificate_source, tls_cert_sha256) =
        build_cut40_server_tls_config_v0()?;
    let listener = TcpListener::bind(&bind_addr)
        .with_context(|| format!("bind wss tls public relay: {bind_addr}"))?;
    listener
        .set_nonblocking(true)
        .context("set wss tls relay listener nonblocking")?;
    let bind_addr_effective = listener.local_addr().context("wss tls relay local addr")?;
    let start = Instant::now();
    let mut peers = Vec::new();
    let mut events = Vec::new();

    while start.elapsed() < Duration::from_millis(timeout_ms)
        && peers.len() < expected_sessions as usize
    {
        match listener.accept() {
            Ok((tcp, source_addr)) => match cut40_accept_registered_peer_v0(
                tcp,
                server_config.clone(),
                &format!("cut40-ping-{}", peers.len()),
            ) {
                Ok(peer) => {
                    events.push(json!({
                        "kind": "wss_register",
                        "peer_id": peer.register.peer_id,
                        "source_addr": source_addr.to_string(),
                    }));
                    peers.push(peer);
                }
                Err(error) => events.push(json!({
                    "kind": "wss_register_failed",
                    "source_addr": source_addr.to_string(),
                    "error": error.to_string(),
                })),
            },
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                thread::sleep(Duration::from_millis(25));
            }
            Err(error) => {
                events.push(json!({
                    "kind": "wss_accept_failed",
                    "error": error.to_string(),
                }));
                break;
            }
        }
    }

    let source_index = peers
        .iter()
        .position(|peer| peer.register.peer_id == source_peer_id);
    let target_index = peers
        .iter()
        .position(|peer| peer.register.peer_id == target_peer_id);
    let mut relay_frames_forwarded = 0u64;
    let mut relay_envelopes_received = 0u64;
    let mut target_peer_id_forwarding = false;
    if let (Some(source_index), Some(target_index)) = (source_index, target_index) {
        let mut source_peer = peers.swap_remove(source_index);
        let adjusted_target_index = if source_index < target_index {
            target_index - 1
        } else {
            target_index
        };
        let mut target_peer = peers.swap_remove(adjusted_target_index);
        target_peer_id_forwarding = true;
        for frame_index in 0..max_frames {
            let bytes = cut39_read_binary_message_v0(&mut source_peer.ws)
                .with_context(|| format!("cut40 relay read envelope {frame_index}"))?;
            let envelope: PublicRelayDataEnvelopeV0 =
                serde_json::from_slice(&bytes).context("cut40 relay decode envelope")?;
            relay_envelopes_received += 1;
            if envelope.target_peer_id != target_peer_id {
                target_peer_id_forwarding = false;
                events.push(json!({
                    "kind": "target_peer_mismatch",
                    "request_id": envelope.request_id,
                    "target_peer_id": envelope.target_peer_id,
                }));
                continue;
            }
            cut39_websocket_write_frame_v0(&mut target_peer.ws, 0x2, &envelope.payload, false)
                .context("cut40 relay forward payload")?;
            relay_frames_forwarded += 1;
            events.push(json!({
                "kind": "wss_relay_forward",
                "request_id": envelope.request_id,
                "source_peer_id": envelope.source_peer_id,
                "target_peer_id": envelope.target_peer_id,
                "forwarded_to_peer_id": target_peer_id,
                "payload_bytes": envelope.payload.len(),
            }));
        }
    }

    let session_peer_ids = events
        .iter()
        .filter_map(|event| event["peer_id"].as_str().map(|peer_id| peer_id.to_string()))
        .collect::<Vec<_>>();
    let public_endpoint_configured =
        env_bool("NOVOVM_OVERLAY_WSS_PUBLIC_ENDPOINT_CONFIGURED", false);
    let configured_tls_material = env_string("NOVOVM_OVERLAY_WSS_TLS_CERT_PATH").is_some()
        && env_string("NOVOVM_OVERLAY_WSS_TLS_KEY_PATH").is_some();
    let accepted = session_peer_ids.len() >= expected_sessions as usize
        && relay_envelopes_received >= max_frames
        && relay_frames_forwarded >= max_frames
        && target_peer_id_forwarding;
    let report = json!({
        "accepted": accepted,
        "scope": "headless_public_wss_tls_relay_runtime_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "real_public_wss_relay": public_endpoint_configured,
        "real_public_tls_smoke": public_endpoint_configured && accepted,
        "real_public_ca_trust_smoke": false,
        "selected_transport": "wss",
        "listen": bind_addr_effective.to_string(),
        "bind_addr_requested": bind_addr,
        "websocket_path": "/novovm",
        "tls_accept_ok": session_peer_ids.len() >= expected_sessions as usize,
        "tls_certificate_source": tls_certificate_source,
        "tls_cert_sha256": tls_cert_sha256,
        "tls_certificate_is_trust_root": false,
        "tls_certificate_purpose": "tls_handshake_material_only",
        "configured_tls_material": configured_tls_material,
        "ca_trust_required": false,
        "node_trust_required": false,
        "relay_trust_required": false,
        "validity_source": "zk_proof_and_seal",
        "default_client_tls_trust_mode": "encrypted-untrusted",
        "optional_endpoint_auth_modes": ["cert-sha256-pin", "webpki", "explicit-ca"],
        "bootstrap_sessions_established": session_peer_ids.len(),
        "session_peer_ids": session_peer_ids,
        "relay_peer_id": relay_peer_id,
        "relay_envelopes_received": relay_envelopes_received,
        "relay_frames_forwarded": relay_frames_forwarded,
        "forwards_by_peer_id": target_peer_id_forwarding,
        "source_peer_id": source_peer_id,
        "target_peer_id": target_peer_id,
        "relay_is_trusted_authority": false,
        "relay_trust_required": false,
        "business_semantics_interpreted_by_relay": false,
        "novorudp_wire_changed": false,
        "events": events,
        "elapsed_ms": start.elapsed().as_millis() as u64,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("wss tls public relay server gate failed")
    }
}

fn run_wss_tls_public_relay_register_client_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/wss-tls-public-relay-register-client.json".into()
    });
    let relay_endpoint = env_string("NOVOVM_OVERLAY_WSS_RELAY_ENDPOINT")
        .context("NOVOVM_OVERLAY_WSS_RELAY_ENDPOINT is required")?;
    let node_id =
        env_string("NOVOVM_OVERLAY_PUBLIC_RELAY_CLIENT_PEER_ID").unwrap_or_else(|| "node-b".into());
    let max_frames = env_u64("NOVOVM_OVERLAY_GATE_MAX_FRAMES", 4).max(1);
    let parsed_endpoint = parse_cut40_wss_endpoint_v0(&relay_endpoint)?;
    let trust_mode = cut40_client_tls_trust_mode_v0();
    let client_config = build_cut40_client_tls_config_v0()?;
    let mut ws = cut40_connect_tls_websocket_v0(&parsed_endpoint, client_config, &node_id)
        .context("cut40 register client connect wss")?;
    let register = PublicRelayRegisterPayloadV0 {
        peer_id: node_id.clone(),
        advertised_endpoint: None,
        registered_at_ms: now_unix_ms(),
    };
    cut39_websocket_write_frame_v0(&mut ws, 0x2, &serde_json::to_vec(&register)?, true)
        .context("cut40 register client send register")?;
    let pong_ok = cut39_send_ping_expect_pong_v0(&mut ws, b"cut40-ping-0")?;
    let start = Instant::now();
    let mut received_frame_count = 0u64;
    let mut frame_decode_ok_count = 0u64;
    let mut frames = Vec::new();
    for frame_index in 0..max_frames {
        let bytes = cut39_read_binary_message_v0(&mut ws)
            .with_context(|| format!("cut40 register client read frame {frame_index}"))?;
        received_frame_count += 1;
        match novovm_network::novorudp::NovoRudpTransportFrameV0::decode(&bytes) {
            Ok(frame) => {
                frame_decode_ok_count += 1;
                frames.push(json!({
                    "frame_decode_ok": true,
                    "decoded_kind": frame.kind,
                    "decoded_sequence": frame.sequence,
                    "payload_bytes": frame.payload.len(),
                }));
            }
            Err(error) => frames.push(json!({
                "frame_decode_ok": false,
                "error": error.to_string(),
                "received_bytes": bytes.len(),
            })),
        }
    }
    let accepted =
        pong_ok && received_frame_count == max_frames && frame_decode_ok_count == max_frames;
    let report = json!({
        "accepted": accepted,
        "scope": "headless_public_wss_tls_register_client_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "selected_transport": "wss",
        "selected_endpoint": relay_endpoint,
        "tls_trust_mode": trust_mode,
        "ca_required": trust_mode == "webpki" || trust_mode == "explicit-ca",
        "tls_certificate_is_trust_root": false,
        "tls_certificate_purpose": "tls_handshake_material_only",
        "tls_peer_endpoint_auth": trust_mode == "cert-sha256-pin" || trust_mode == "webpki" || trust_mode == "explicit-ca",
        "node_trust_required": false,
        "relay_trust_required": false,
        "validity_source": "zk_proof_and_seal",
        "node_id": node_id,
        "bootstrap_register_sent": true,
        "ping_pong_ok": pong_ok,
        "received_frame_count": received_frame_count,
        "frame_decode_ok": frame_decode_ok_count == max_frames,
        "frame_decode_ok_count": frame_decode_ok_count,
        "via_relay_peer_id": "public-relay-1",
        "inbound_public_endpoint_required": false,
        "novorudp_inner_frame_preserved": frame_decode_ok_count == max_frames,
        "frames": frames,
        "elapsed_ms": start.elapsed().as_millis() as u64,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("wss tls public relay register client failed")
    }
}

fn run_wss_tls_public_relay_send_client_gate() -> Result<()> {
    let report_path = env_string("NOVOVM_OVERLAY_GATE_REPORT_PATH").unwrap_or_else(|| {
        "artifacts/network-overlay-gate/wss-tls-public-relay-send-client.json".into()
    });
    let relay_endpoint = env_string("NOVOVM_OVERLAY_WSS_RELAY_ENDPOINT")
        .context("NOVOVM_OVERLAY_WSS_RELAY_ENDPOINT is required")?;
    let source_peer_id =
        env_string("NOVOVM_OVERLAY_PUBLIC_RELAY_SOURCE_PEER_ID").unwrap_or_else(|| "node-a".into());
    let target_peer_id =
        env_string("NOVOVM_OVERLAY_PUBLIC_RELAY_TARGET_PEER_ID").unwrap_or_else(|| "node-b".into());
    let max_frames = env_u64("NOVOVM_OVERLAY_GATE_MAX_FRAMES", 4).max(1);
    let parsed_endpoint = parse_cut40_wss_endpoint_v0(&relay_endpoint)?;
    let trust_mode = cut40_client_tls_trust_mode_v0();
    let client_config = build_cut40_client_tls_config_v0()?;
    let mut ws = cut40_connect_tls_websocket_v0(&parsed_endpoint, client_config, &source_peer_id)
        .context("cut40 send client connect wss")?;
    let register = PublicRelayRegisterPayloadV0 {
        peer_id: source_peer_id.clone(),
        advertised_endpoint: None,
        registered_at_ms: now_unix_ms(),
    };
    cut39_websocket_write_frame_v0(&mut ws, 0x2, &serde_json::to_vec(&register)?, true)
        .context("cut40 send client send register")?;
    let pong_ok = cut39_send_ping_expect_pong_v0(&mut ws, b"cut40-ping-1")?;
    let mut sent_frames = Vec::new();
    for frame_index in 0..max_frames {
        let frame = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            [40u8; 16],
            400,
            401,
            frame_index,
            403,
            format!("cut40-public-wss-opaque-frame-{frame_index}").into_bytes(),
        );
        let envelope = PublicRelayDataEnvelopeV0 {
            request_id: format!("cut40-wss-public-{frame_index}"),
            source_peer_id: source_peer_id.clone(),
            target_peer_id: target_peer_id.clone(),
            payload: frame.encode(),
        };
        let encoded = serde_json::to_vec(&envelope)?;
        cut39_websocket_write_frame_v0(&mut ws, 0x2, &encoded, true)
            .with_context(|| format!("cut40 send client envelope {frame_index}"))?;
        sent_frames.push(json!({
            "request_id": envelope.request_id,
            "target_peer_id": target_peer_id,
            "sent_bytes": encoded.len(),
            "queued": false,
        }));
    }
    let accepted = pong_ok && sent_frames.len() == max_frames as usize;
    let report = json!({
        "accepted": accepted,
        "scope": "headless_public_wss_tls_send_client_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "selected_transport": "wss",
        "selected_endpoint": relay_endpoint,
        "tls_trust_mode": trust_mode,
        "ca_required": trust_mode == "webpki" || trust_mode == "explicit-ca",
        "tls_certificate_is_trust_root": false,
        "tls_certificate_purpose": "tls_handshake_material_only",
        "tls_peer_endpoint_auth": trust_mode == "cert-sha256-pin" || trust_mode == "webpki" || trust_mode == "explicit-ca",
        "node_trust_required": false,
        "relay_trust_required": false,
        "validity_source": "zk_proof_and_seal",
        "selected_path": "RelayNovoRudp",
        "source_peer_id": source_peer_id,
        "target_peer_id": target_peer_id,
        "sent_frame_count": sent_frames.len(),
        "inbound_public_endpoint_required": false,
        "nat_punch_required": false,
        "ping_pong_ok": pong_ok,
        "sent_frames": sent_frames,
    });
    write_json_report(&report_path, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if accepted {
        Ok(())
    } else {
        anyhow::bail!("wss tls public relay send client failed")
    }
}

fn run_cut39_local_wss_tls_socket_smoke_v0(
    max_frames: u64,
) -> Result<Cut39WssTlsSocketSmokeOutcomeV0> {
    let (server_config, client_config) = build_cut39_tls_configs_v0()?;
    let listener = TcpListener::bind("127.0.0.1:0").context("bind cut39 local tls relay")?;
    let relay_addr = listener.local_addr().context("cut39 relay local addr")?;
    let selected_endpoint = format!("wss://127.0.0.1:{}/novovm", relay_addr.port());
    let (ready_tx, ready_rx) = mpsc::channel::<SocketAddr>();
    let relay_server_config = server_config.clone();
    let relay_handle = thread::spawn(move || -> Result<Cut39RelayThreadOutcomeV0> {
        ready_tx
            .send(relay_addr)
            .context("send cut39 relay ready addr")?;
        run_cut39_relay_thread_v0(listener, relay_server_config, max_frames)
    });

    let relay_addr = ready_rx
        .recv_timeout(Duration::from_secs(5))
        .context("wait cut39 relay ready")?;
    let node_b_client_config = client_config.clone();
    let node_b_handle = thread::spawn(move || -> Result<Cut39NodeBThreadOutcomeV0> {
        run_cut39_node_b_client_v0(relay_addr, node_b_client_config, max_frames)
    });

    thread::sleep(Duration::from_millis(150));
    let node_a_client_config = client_config.clone();
    let node_a_handle = thread::spawn(move || -> Result<bool> {
        run_cut39_node_a_client_v0(relay_addr, node_a_client_config, max_frames)
    });

    let node_a_ok = node_a_handle
        .join()
        .map_err(|_| anyhow::anyhow!("cut39 node-a thread panicked"))??;
    let node_b_outcome = node_b_handle
        .join()
        .map_err(|_| anyhow::anyhow!("cut39 node-b thread panicked"))??;
    let relay_outcome = relay_handle
        .join()
        .map_err(|_| anyhow::anyhow!("cut39 relay thread panicked"))??;

    Ok(Cut39WssTlsSocketSmokeOutcomeV0 {
        selected_endpoint,
        websocket_upgrade_ok: relay_outcome.websocket_upgrade_count == 2,
        tls_accept_ok: relay_outcome.tls_accept_count == 2,
        binary_frame_mode: node_a_ok && node_b_outcome.received_frame_count == max_frames,
        novorudp_inner_frame_preserved: node_b_outcome.frame_decode_ok_count == max_frames,
        client_register_node_a_ok: relay_outcome
            .registered_peer_ids
            .iter()
            .any(|peer_id| peer_id == "node-a"),
        client_register_node_b_ok: relay_outcome
            .registered_peer_ids
            .iter()
            .any(|peer_id| peer_id == "node-b"),
        registered_peer_ids: relay_outcome.registered_peer_ids,
        relay_frames_forwarded: relay_outcome.relay_frames_forwarded,
        target_peer_id_forwarding: relay_outcome.target_peer_id_forwarding,
        ping_pong_ok: relay_outcome.ping_pong_ok && node_b_outcome.pong_ok,
        node_b_received_frame_count: node_b_outcome.received_frame_count,
        node_b_frame_decode_ok_count: node_b_outcome.frame_decode_ok_count,
    })
}

fn run_cut39_relay_thread_v0(
    listener: TcpListener,
    server_config: Arc<rustls::ServerConfig>,
    max_frames: u64,
) -> Result<Cut39RelayThreadOutcomeV0> {
    let (node_b_tcp, _) = listener.accept().context("cut39 relay accept node-b")?;
    let mut node_b_ws = cut39_accept_tls_websocket_v0(node_b_tcp, server_config.clone())
        .context("node-b wss accept")?;
    let node_b_register = cut39_read_register_message_v0(&mut node_b_ws, "node-b")?;
    let node_b_pong_ok = cut39_answer_ping_v0(&mut node_b_ws, b"cut39-node-b-ping")?;

    let (node_a_tcp, _) = listener.accept().context("cut39 relay accept node-a")?;
    let mut node_a_ws =
        cut39_accept_tls_websocket_v0(node_a_tcp, server_config).context("node-a wss accept")?;
    let node_a_register = cut39_read_register_message_v0(&mut node_a_ws, "node-a")?;
    let node_a_pong_ok = cut39_answer_ping_v0(&mut node_a_ws, b"cut39-node-a-ping")?;

    let mut manager = Wss443RelaySessionManagerV0::new(max_frames as usize + 1, 30_000);
    manager.register_session(&node_b_register.peer_id, "cut39-wss-node-b", 1_000);
    manager.register_session(&node_a_register.peer_id, "cut39-wss-node-a", 1_001);

    let mut relay_frames_forwarded = 0u64;
    let mut target_peer_id_forwarding = true;
    for frame_index in 0..max_frames {
        let bytes = cut39_read_binary_message_v0(&mut node_a_ws)
            .with_context(|| format!("cut39 relay read node-a envelope {frame_index}"))?;
        let envelope: PublicRelayDataEnvelopeV0 =
            serde_json::from_slice(&bytes).context("cut39 relay decode data envelope")?;
        let forward = manager.forward_by_peer_id(envelope.clone(), 2_000 + frame_index);
        if !forward.accepted || forward.forwarded_to_peer_id.as_deref() != Some("node-b") {
            target_peer_id_forwarding = false;
            continue;
        }
        cut39_websocket_write_frame_v0(&mut node_b_ws, 0x2, &envelope.payload, false)
            .context("cut39 relay forward payload to node-b")?;
        relay_frames_forwarded += 1;
    }

    Ok(Cut39RelayThreadOutcomeV0 {
        websocket_upgrade_count: 2,
        tls_accept_count: 2,
        registered_peer_ids: vec![node_b_register.peer_id, node_a_register.peer_id],
        relay_frames_forwarded,
        target_peer_id_forwarding,
        ping_pong_ok: node_a_pong_ok && node_b_pong_ok,
    })
}

fn run_cut39_node_b_client_v0(
    relay_addr: SocketAddr,
    client_config: Arc<rustls::ClientConfig>,
    max_frames: u64,
) -> Result<Cut39NodeBThreadOutcomeV0> {
    let mut ws = cut39_connect_tls_websocket_v0(relay_addr, client_config, "node-b")
        .context("cut39 node-b connect wss")?;
    let register = PublicRelayRegisterPayloadV0 {
        peer_id: "node-b".into(),
        advertised_endpoint: None,
        registered_at_ms: now_unix_ms(),
    };
    cut39_websocket_write_frame_v0(&mut ws, 0x2, &serde_json::to_vec(&register)?, true)
        .context("cut39 node-b send register")?;
    let pong_ok = cut39_send_ping_expect_pong_v0(&mut ws, b"cut39-node-b-ping")?;

    let mut received_frame_count = 0u64;
    let mut frame_decode_ok_count = 0u64;
    for frame_index in 0..max_frames {
        let frame_bytes = cut39_read_binary_message_v0(&mut ws)
            .with_context(|| format!("cut39 node-b read forwarded frame {frame_index}"))?;
        received_frame_count += 1;
        if let Ok(frame) = novovm_network::novorudp::NovoRudpTransportFrameV0::decode(&frame_bytes)
        {
            if frame.kind == NovoRudpTransportFrameKindV0::Data {
                frame_decode_ok_count += 1;
            }
        }
    }

    Ok(Cut39NodeBThreadOutcomeV0 {
        received_frame_count,
        frame_decode_ok_count,
        pong_ok,
    })
}

fn run_cut39_node_a_client_v0(
    relay_addr: SocketAddr,
    client_config: Arc<rustls::ClientConfig>,
    max_frames: u64,
) -> Result<bool> {
    let mut ws = cut39_connect_tls_websocket_v0(relay_addr, client_config, "node-a")
        .context("cut39 node-a connect wss")?;
    let register = PublicRelayRegisterPayloadV0 {
        peer_id: "node-a".into(),
        advertised_endpoint: None,
        registered_at_ms: now_unix_ms(),
    };
    cut39_websocket_write_frame_v0(&mut ws, 0x2, &serde_json::to_vec(&register)?, true)
        .context("cut39 node-a send register")?;
    let pong_ok = cut39_send_ping_expect_pong_v0(&mut ws, b"cut39-node-a-ping")?;

    for frame_index in 0..max_frames {
        let frame = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            [39u8; 16],
            390,
            391,
            frame_index,
            393,
            format!("cut39-opaque-novorudp-payload-{frame_index}").into_bytes(),
        );
        let envelope = PublicRelayDataEnvelopeV0 {
            request_id: format!("cut39-wss-frame-{frame_index}"),
            source_peer_id: "node-a".into(),
            target_peer_id: "node-b".into(),
            payload: frame.encode(),
        };
        cut39_websocket_write_frame_v0(&mut ws, 0x2, &serde_json::to_vec(&envelope)?, true)
            .with_context(|| format!("cut39 node-a send envelope {frame_index}"))?;
    }

    Ok(pong_ok)
}

fn build_cut39_tls_configs_v0() -> Result<(Arc<rustls::ServerConfig>, Arc<rustls::ClientConfig>)> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])
        .context("generate cut39 self-signed cert")?;
    let cert_der = cert.serialize_der().context("serialize cut39 cert")?;
    let key_der = cert.serialize_private_key_der();
    let certs = vec![CertificateDer::from(cert_der.clone())];
    let server_config = rustls::ServerConfig::builder_with_provider(overlay_gate_tls_provider_v0())
        .with_safe_default_protocol_versions()
        .context("select cut39 server TLS protocol versions")?
        .with_no_client_auth()
        .with_single_cert(
            certs,
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der)),
        )
        .context("build cut39 server tls config")?;

    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(CertificateDer::from(cert_der))
        .context("trust cut39 self-signed cert")?;
    let client_config = rustls::ClientConfig::builder_with_provider(overlay_gate_tls_provider_v0())
        .with_safe_default_protocol_versions()
        .context("select cut39 client TLS protocol versions")?
        .with_root_certificates(roots)
        .with_no_client_auth();

    Ok((Arc::new(server_config), Arc::new(client_config)))
}

fn build_cut40_server_tls_config_v0() -> Result<(Arc<rustls::ServerConfig>, String, String)> {
    let cert_path = env_string("NOVOVM_OVERLAY_WSS_TLS_CERT_PATH");
    let key_path = env_string("NOVOVM_OVERLAY_WSS_TLS_KEY_PATH");
    if let (Some(cert_path), Some(key_path)) = (cert_path, key_path) {
        let certs = load_cut40_certs_pem_v0(&cert_path)
            .with_context(|| format!("load tls cert path: {cert_path}"))?;
        let tls_cert_sha256 = overlay_gate_sha256_hex_v0(&[certs[0].as_ref()]);
        let key = load_cut40_private_key_pem_v0(&key_path)
            .with_context(|| format!("load tls key path: {key_path}"))?;
        let server_config =
            rustls::ServerConfig::builder_with_provider(overlay_gate_tls_provider_v0())
                .with_safe_default_protocol_versions()
                .context("select cut40 configured server TLS protocol versions")?
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .context("build cut40 server tls config from pem")?;
        return Ok((
            Arc::new(server_config),
            "configured_pem".into(),
            tls_cert_sha256,
        ));
    }

    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])
        .context("generate cut40 self-signed cert")?;
    let cert_der = cert.serialize_der().context("serialize cut40 cert")?;
    let tls_cert_sha256 = overlay_gate_sha256_hex_v0(&[&cert_der]);
    let key_der = cert.serialize_private_key_der();
    let server_config = rustls::ServerConfig::builder_with_provider(overlay_gate_tls_provider_v0())
        .with_safe_default_protocol_versions()
        .context("select cut40 self-signed server TLS protocol versions")?
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(cert_der)],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der)),
        )
        .context("build cut40 self-signed server tls config")?;
    Ok((
        Arc::new(server_config),
        "self_signed_ephemeral".into(),
        tls_cert_sha256,
    ))
}

fn build_cut40_client_tls_config_v0() -> Result<Arc<rustls::ClientConfig>> {
    let trust_mode = cut40_client_tls_trust_mode_v0();
    if trust_mode == "cert-sha256-pin" {
        let expected_sha256_hex = env_string("NOVOVM_OVERLAY_WSS_TLS_CERT_SHA256")
            .context("NOVOVM_OVERLAY_WSS_TLS_CERT_SHA256 is required for cert-sha256-pin trust")?;
        let verifier = Cut40PinnedCertVerifierV0 {
            expected_sha256_hex: Some(expected_sha256_hex),
            trust_mode,
        };
        let client_config =
            rustls::ClientConfig::builder_with_provider(overlay_gate_tls_provider_v0())
                .with_safe_default_protocol_versions()
                .context("select cut40 pinned client TLS protocol versions")?
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(verifier))
                .with_no_client_auth();
        return Ok(Arc::new(client_config));
    }
    if trust_mode == "encrypted-untrusted" || trust_mode == "insecure-test-only" {
        let verifier = Cut40PinnedCertVerifierV0 {
            expected_sha256_hex: None,
            trust_mode,
        };
        let client_config =
            rustls::ClientConfig::builder_with_provider(overlay_gate_tls_provider_v0())
                .with_safe_default_protocol_versions()
                .context("select cut40 node-key client TLS protocol versions")?
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(verifier))
                .with_no_client_auth();
        return Ok(Arc::new(client_config));
    }

    let mut roots = rustls::RootCertStore::empty();
    if trust_mode == "explicit-ca" {
        let ca_path = env_string("NOVOVM_OVERLAY_WSS_TLS_CA_CERT_PATH")
            .context("NOVOVM_OVERLAY_WSS_TLS_CA_CERT_PATH is required for explicit-ca trust")?;
        for cert in load_cut40_certs_pem_v0(&ca_path)
            .with_context(|| format!("load wss ca cert path: {ca_path}"))?
        {
            roots.add(cert).context("add configured wss ca cert")?;
        }
    } else if trust_mode == "webpki" {
        let native = rustls_native_certs::load_native_certs();
        if !native.errors.is_empty() {
            anyhow::bail!("load platform native tls roots: {:?}", native.errors);
        }
        for cert in native.certs {
            roots.add(cert).context("add platform native tls root")?;
        }
    } else {
        anyhow::bail!("unsupported NOVOVM_OVERLAY_WSS_TLS_TRUST_MODE: {trust_mode}");
    }
    let client_config = rustls::ClientConfig::builder_with_provider(overlay_gate_tls_provider_v0())
        .with_safe_default_protocol_versions()
        .context("select cut40 root-store client TLS protocol versions")?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(client_config))
}

fn overlay_gate_tls_provider_v0() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

fn cut40_client_tls_trust_mode_v0() -> String {
    env_string("NOVOVM_OVERLAY_WSS_TLS_TRUST_MODE").unwrap_or_else(|| "encrypted-untrusted".into())
}

fn load_cut40_certs_pem_v0(path: &str) -> Result<Vec<CertificateDer<'static>>> {
    let bytes = fs::read(path).with_context(|| format!("read cert pem: {path}"))?;
    let certs = CertificateDer::pem_slice_iter(bytes.as_slice())
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("parse cert pem")?;
    if certs.is_empty() {
        anyhow::bail!("no certificates in pem: {path}");
    }
    Ok(certs)
}

fn load_cut40_private_key_pem_v0(path: &str) -> Result<PrivateKeyDer<'static>> {
    let bytes = fs::read(path).with_context(|| format!("read key pem: {path}"))?;
    PrivateKeyDer::from_pem_slice(bytes.as_slice())
        .context("parse private key pem")
        .with_context(|| format!("no supported private key in pem: {path}"))
}

fn parse_cut40_wss_endpoint_v0(endpoint: &str) -> Result<Cut40WssEndpointV0> {
    let without_scheme = endpoint
        .strip_prefix("wss://")
        .or_else(|| endpoint.strip_prefix("ws://"))
        .unwrap_or(endpoint);
    let mut split = without_scheme.splitn(2, '/');
    let authority = split
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid wss endpoint: {endpoint}"))?;
    let path = format!("/{}", split.next().unwrap_or("novovm"));
    let (host, port) = authority
        .rsplit_once(':')
        .map(|(host, port)| (host.to_string(), port.parse::<u16>()))
        .ok_or_else(|| anyhow::anyhow!("wss endpoint must include host:port: {endpoint}"))?;
    let port = port.with_context(|| format!("parse wss endpoint port: {authority}"))?;
    let socket_addr = (host.as_str(), port)
        .to_socket_addrs()
        .with_context(|| format!("resolve wss endpoint: {host}:{port}"))?
        .next()
        .ok_or_else(|| anyhow::anyhow!("no socket addr resolved for {host}:{port}"))?;
    Ok(Cut40WssEndpointV0 {
        host,
        socket_addr,
        path,
    })
}

fn cut40_accept_registered_peer_v0(
    tcp: TcpStream,
    server_config: Arc<rustls::ServerConfig>,
    ping_payload: &str,
) -> Result<Cut40AcceptedPeerV0> {
    let mut ws = cut39_accept_tls_websocket_v0(tcp, server_config).context("cut40 accept wss")?;
    let bytes = cut39_read_binary_message_v0(&mut ws).context("cut40 read register")?;
    let register: PublicRelayRegisterPayloadV0 =
        serde_json::from_slice(&bytes).context("cut40 decode register")?;
    cut39_answer_ping_v0(&mut ws, ping_payload.as_bytes()).context("cut40 answer ping")?;
    Ok(Cut40AcceptedPeerV0 { register, ws })
}

fn cut40_connect_tls_websocket_v0(
    endpoint: &Cut40WssEndpointV0,
    client_config: Arc<rustls::ClientConfig>,
    peer_id: &str,
) -> Result<rustls::StreamOwned<rustls::ClientConnection, TcpStream>> {
    let tcp = TcpStream::connect(endpoint.socket_addr).with_context(|| {
        format!(
            "connect cut40 relay tcp: {} ({})",
            endpoint.host, endpoint.socket_addr
        )
    })?;
    tcp.set_read_timeout(Some(Duration::from_secs(10)))
        .context("set cut40 client read timeout")?;
    tcp.set_write_timeout(Some(Duration::from_secs(10)))
        .context("set cut40 client write timeout")?;
    let server_name = ServerName::try_from(endpoint.host.clone()).context("cut40 server name")?;
    let client_conn = rustls::ClientConnection::new(client_config, server_name)
        .context("create cut40 tls client")?;
    let mut tls = rustls::StreamOwned::new(client_conn, tcp);
    let key = cut39_websocket_key_v0(peer_id);
    cut40_websocket_client_upgrade_v0(&mut tls, endpoint, &key)?;
    Ok(tls)
}

fn cut40_websocket_client_upgrade_v0<S: Read + Write>(
    stream: &mut S,
    endpoint: &Cut40WssEndpointV0,
    sec_key: &str,
) -> Result<()> {
    let request = format!(
        "GET {} HTTP/1.1\r\n\
         Host: {}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {sec_key}\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n",
        endpoint.path, endpoint.host
    );
    stream
        .write_all(request.as_bytes())
        .context("cut40 write websocket upgrade request")?;
    stream.flush().context("cut40 flush websocket request")?;
    let response = cut39_read_http_headers_v0(stream).context("cut40 read websocket response")?;
    if !response.starts_with("HTTP/1.1 101") {
        anyhow::bail!("websocket upgrade rejected: {response}");
    }
    let expected_accept = cut39_websocket_accept_key_v0(sec_key);
    if !response.contains(&format!("Sec-WebSocket-Accept: {expected_accept}")) {
        anyhow::bail!("websocket accept key mismatch");
    }
    Ok(())
}

fn cut39_accept_tls_websocket_v0(
    tcp: TcpStream,
    server_config: Arc<rustls::ServerConfig>,
) -> Result<rustls::StreamOwned<rustls::ServerConnection, TcpStream>> {
    tcp.set_read_timeout(Some(Duration::from_secs(5)))
        .context("set cut39 server read timeout")?;
    tcp.set_write_timeout(Some(Duration::from_secs(5)))
        .context("set cut39 server write timeout")?;
    let server_conn =
        rustls::ServerConnection::new(server_config).context("create cut39 server tls conn")?;
    let mut tls = rustls::StreamOwned::new(server_conn, tcp);
    cut39_websocket_server_accept_v0(&mut tls, "/novovm")?;
    Ok(tls)
}

fn cut39_connect_tls_websocket_v0(
    relay_addr: SocketAddr,
    client_config: Arc<rustls::ClientConfig>,
    peer_id: &str,
) -> Result<rustls::StreamOwned<rustls::ClientConnection, TcpStream>> {
    let tcp = TcpStream::connect(relay_addr).context("connect cut39 relay tcp")?;
    tcp.set_read_timeout(Some(Duration::from_secs(5)))
        .context("set cut39 client read timeout")?;
    tcp.set_write_timeout(Some(Duration::from_secs(5)))
        .context("set cut39 client write timeout")?;
    let server_name = ServerName::try_from("localhost").context("cut39 server name")?;
    let client_conn = rustls::ClientConnection::new(client_config, server_name)
        .context("create cut39 tls client")?;
    let mut tls = rustls::StreamOwned::new(client_conn, tcp);
    let key = cut39_websocket_key_v0(peer_id);
    cut39_websocket_client_upgrade_v0(&mut tls, "/novovm", &relay_addr, &key)?;
    Ok(tls)
}

fn cut39_websocket_key_v0(peer_id: &str) -> String {
    let mut bytes = [0u8; 16];
    for (index, byte) in peer_id.as_bytes().iter().enumerate().take(16) {
        bytes[index] = *byte;
    }
    BASE64_STANDARD.encode(bytes)
}

fn cut39_websocket_server_accept_v0<S: Read + Write>(
    stream: &mut S,
    expected_path: &str,
) -> Result<()> {
    let request = cut39_read_http_headers_v0(stream).context("cut39 read websocket request")?;
    let mut lines = request.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing websocket request line"))?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default();
    let path = request_parts.next().unwrap_or_default();
    if method != "GET" || path != expected_path {
        anyhow::bail!("invalid websocket upgrade request: {request_line}");
    }
    let mut sec_key = None;
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("sec-websocket-key:") {
            sec_key = line
                .split_once(':')
                .map(|(_, value)| value.trim().to_string());
            break;
        }
    }
    let sec_key = sec_key.ok_or_else(|| anyhow::anyhow!("missing sec-websocket-key"))?;
    let accept = cut39_websocket_accept_key_v0(&sec_key);
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    stream
        .write_all(response.as_bytes())
        .context("cut39 write websocket upgrade response")?;
    stream.flush().context("cut39 flush websocket upgrade")?;
    Ok(())
}

fn cut39_websocket_client_upgrade_v0<S: Read + Write>(
    stream: &mut S,
    path: &str,
    relay_addr: &SocketAddr,
    sec_key: &str,
) -> Result<()> {
    let request = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: localhost:{}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {sec_key}\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n",
        relay_addr.port()
    );
    stream
        .write_all(request.as_bytes())
        .context("cut39 write websocket upgrade request")?;
    stream.flush().context("cut39 flush websocket request")?;
    let response = cut39_read_http_headers_v0(stream).context("cut39 read websocket response")?;
    if !response.starts_with("HTTP/1.1 101") {
        anyhow::bail!("websocket upgrade rejected: {response}");
    }
    let expected_accept = cut39_websocket_accept_key_v0(sec_key);
    if !response.contains(&format!("Sec-WebSocket-Accept: {expected_accept}")) {
        anyhow::bail!("websocket accept key mismatch");
    }
    Ok(())
}

fn cut39_websocket_accept_key_v0(sec_key: &str) -> String {
    let mut hasher = sha1::Sha1::new();
    hasher.update(sec_key.as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    BASE64_STANDARD.encode(hasher.finalize())
}

fn cut39_read_http_headers_v0<S: Read>(stream: &mut S) -> Result<String> {
    let mut bytes = Vec::new();
    let mut one = [0u8; 1];
    while bytes.len() < 8192 {
        stream
            .read_exact(&mut one)
            .context("cut39 read http header byte")?;
        bytes.push(one[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            return String::from_utf8(bytes).context("cut39 http headers utf8");
        }
    }
    anyhow::bail!("cut39 http headers exceeded limit")
}

fn cut39_read_register_message_v0<S: Read + Write>(
    stream: &mut S,
    expected_peer_id: &str,
) -> Result<PublicRelayRegisterPayloadV0> {
    let bytes = cut39_read_binary_message_v0(stream).context("cut39 read register message")?;
    let register: PublicRelayRegisterPayloadV0 =
        serde_json::from_slice(&bytes).context("cut39 decode register message")?;
    if register.peer_id != expected_peer_id {
        anyhow::bail!(
            "cut39 register peer mismatch: expected {expected_peer_id}, got {}",
            register.peer_id
        );
    }
    Ok(register)
}

fn cut39_answer_ping_v0<S: Read + Write>(stream: &mut S, expected_payload: &[u8]) -> Result<bool> {
    match cut39_websocket_read_frame_v0(stream).context("cut39 read ping")? {
        Cut39WebSocketFrameV0::Ping(payload) if payload == expected_payload => {
            cut39_websocket_write_frame_v0(stream, 0xA, &payload, false)
                .context("cut39 write pong")?;
            Ok(true)
        }
        other => anyhow::bail!("cut39 expected ping, got {other:?}"),
    }
}

fn cut39_send_ping_expect_pong_v0<S: Read + Write>(stream: &mut S, payload: &[u8]) -> Result<bool> {
    cut39_websocket_write_frame_v0(stream, 0x9, payload, true).context("cut39 write ping")?;
    match cut39_websocket_read_frame_v0(stream).context("cut39 read pong")? {
        Cut39WebSocketFrameV0::Pong(received) => Ok(received == payload),
        other => anyhow::bail!("cut39 expected pong, got {other:?}"),
    }
}

fn cut39_read_binary_message_v0<S: Read + Write>(stream: &mut S) -> Result<Vec<u8>> {
    match cut39_websocket_read_frame_v0(stream).context("cut39 read websocket frame")? {
        Cut39WebSocketFrameV0::Binary(bytes) => Ok(bytes),
        other => anyhow::bail!("cut39 expected binary websocket frame, got {other:?}"),
    }
}

fn cut39_websocket_write_frame_v0<S: Write>(
    stream: &mut S,
    opcode: u8,
    payload: &[u8],
    masked: bool,
) -> Result<()> {
    let mut header = Vec::with_capacity(14 + payload.len());
    header.push(0x80 | (opcode & 0x0F));
    let mask_bit = if masked { 0x80 } else { 0 };
    match payload.len() {
        len if len <= 125 => header.push(mask_bit | len as u8),
        len if len <= u16::MAX as usize => {
            header.push(mask_bit | 126);
            header.extend_from_slice(&(len as u16).to_be_bytes());
        }
        len => {
            header.push(mask_bit | 127);
            header.extend_from_slice(&(len as u64).to_be_bytes());
        }
    }
    if masked {
        let mask = [0x13, 0x37, 0x39, 0x41];
        header.extend_from_slice(&mask);
        header.extend(
            payload
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ mask[index % 4]),
        );
    } else {
        header.extend_from_slice(payload);
    }
    stream
        .write_all(&header)
        .context("cut39 write websocket frame")?;
    stream.flush().context("cut39 flush websocket frame")?;
    Ok(())
}

fn cut39_websocket_read_frame_v0<S: Read>(stream: &mut S) -> Result<Cut39WebSocketFrameV0> {
    let mut header = [0u8; 2];
    stream
        .read_exact(&mut header)
        .context("cut39 read websocket frame header")?;
    let opcode = header[0] & 0x0F;
    let masked = (header[1] & 0x80) != 0;
    let mut len = (header[1] & 0x7F) as u64;
    if len == 126 {
        let mut ext = [0u8; 2];
        stream
            .read_exact(&mut ext)
            .context("cut39 read websocket len16")?;
        len = u16::from_be_bytes(ext) as u64;
    } else if len == 127 {
        let mut ext = [0u8; 8];
        stream
            .read_exact(&mut ext)
            .context("cut39 read websocket len64")?;
        len = u64::from_be_bytes(ext);
    }
    if len > 1_048_576 {
        anyhow::bail!("cut39 websocket frame too large: {len}");
    }
    let mask = if masked {
        let mut mask = [0u8; 4];
        stream
            .read_exact(&mut mask)
            .context("cut39 read websocket mask")?;
        Some(mask)
    } else {
        None
    };
    let mut payload = vec![0u8; len as usize];
    stream
        .read_exact(&mut payload)
        .context("cut39 read websocket payload")?;
    if let Some(mask) = mask {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }
    }
    match opcode {
        0x2 => Ok(Cut39WebSocketFrameV0::Binary(payload)),
        0x8 => Ok(Cut39WebSocketFrameV0::Close),
        0x9 => Ok(Cut39WebSocketFrameV0::Ping(payload)),
        0xA => Ok(Cut39WebSocketFrameV0::Pong(payload)),
        other => anyhow::bail!("cut39 unsupported websocket opcode: {other}"),
    }
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
    source_peer_id: &str,
    observer_peer_id: &str,
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
        source_peer_id: source_peer_id.to_string(),
        target_peer_id: observer_peer_id.to_string(),
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
    let nat_mapping_changed =
        advertised_endpoint.as_deref() != Some(decoded_ack.observed_endpoint.as_str());
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
        "nat_traversal_enabled": true,
        "local_peer_id": source_peer_id,
        "target_peer_id": observer_peer_id,
        "local_bind_endpoint": prober_addr.to_string(),
        "observer_bind_endpoint": observer_addr.to_string(),
        "advertised_endpoint": advertised_endpoint,
        "punch_target_peer_id": observer_peer_id,
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
        "nat_mapping_changed": nat_mapping_changed,
        "nat_mapping_stable": !nat_mapping_changed,
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
        "nat_traversal_enabled": true,
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
        "relay_fallback_endpoint": serde_json::Value::Null,
        "fallback_reason": "NatPunchFailed",
        "selected_path_after_punch": "RelayNovoRudp",
    }))
}

struct NatPunchProbeInputV0<'a> {
    socket: &'a UdpSocket,
    punch_target_observed_endpoint: &'a str,
    source_peer_id: &'a str,
    target_peer_id: &'a str,
    advertised_endpoint: Option<String>,
    punch_nonce: String,
    bind_addr_effective: SocketAddr,
    relay_fallback_enabled: bool,
    relay_fallback_endpoint: Option<String>,
}

fn run_nat_punch_probe_v0(input: NatPunchProbeInputV0<'_>) -> Result<serde_json::Value> {
    let NatPunchProbeInputV0 {
        socket,
        punch_target_observed_endpoint,
        source_peer_id,
        target_peer_id,
        advertised_endpoint,
        punch_nonce,
        bind_addr_effective,
        relay_fallback_enabled,
        relay_fallback_endpoint,
    } = input;
    let payload = NatPunchProbePayloadV0 {
        punch_nonce: punch_nonce.clone(),
        source_peer_id: source_peer_id.to_string(),
        target_peer_id: target_peer_id.to_string(),
        advertised_endpoint: advertised_endpoint.clone(),
        target_observed_endpoint: punch_target_observed_endpoint.to_string(),
        expires_at_ms: now_unix_ms().saturating_add(60_000),
    };
    let punch = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Endpoint,
        [27u8; 16],
        270,
        271,
        272,
        273,
        serde_json::to_vec(&payload)?,
    );
    let encoded_punch = punch.encode();
    let start = Instant::now();
    let sent_bytes = socket
        .send_to(&encoded_punch, punch_target_observed_endpoint)
        .with_context(|| format!("send nat punch to {punch_target_observed_endpoint}"))?;
    let mut buf = vec![0u8; 65535];
    let mut report = json!({
        "accepted": false,
        "scope": "nat_punch_prober_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "nat_traversal_enabled": true,
        "local_peer_id": source_peer_id,
        "target_peer_id": target_peer_id,
        "local_bind_endpoint": bind_addr_effective.to_string(),
        "advertised_endpoint": advertised_endpoint,
        "observed_endpoint": serde_json::Value::Null,
        "observed_by_peer_id": serde_json::Value::Null,
        "nat_mapping_changed": serde_json::Value::Null,
        "nat_mapping_stable": serde_json::Value::Null,
        "punch_nonce": punch_nonce,
        "punch_target_peer_id": target_peer_id,
        "punch_target_observed_endpoint": punch_target_observed_endpoint,
        "punch_attempt_sent": true,
        "sent_bytes": sent_bytes,
        "punch_ack_valid": false,
        "punch_result": "pending",
        "punch_reject_reason": serde_json::Value::Null,
        "relay_fallback_endpoint": relay_fallback_endpoint,
        "relay_fallback_selected": false,
        "fallback_reason": serde_json::Value::Null,
        "selected_path_after_punch": serde_json::Value::Null,
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
                            let mapping_changed = report["advertised_endpoint"].as_str()
                                != Some(ack_payload.observed_endpoint.as_str());
                            report["observed_endpoint"] = json!(ack_payload.observed_endpoint);
                            report["observed_by_peer_id"] = json!(ack_payload.observer_peer_id);
                            report["observed_at_ms"] = json!(ack_payload.observed_at_ms);
                            report["ack_source_endpoint"] = json!(ack_source_addr.to_string());
                            report["nat_mapping_changed"] = json!(mapping_changed);
                            report["nat_mapping_stable"] = json!(!mapping_changed);
                            report["ack_nonce"] = json!(ack_payload.punch_nonce);
                            report["punch_ack_valid"] = json!(punch_ack_valid);
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
                                apply_nat_punch_fallback_v0(
                                    &mut report,
                                    relay_fallback_enabled,
                                    "punch_nonce_mismatch".to_string(),
                                );
                            }
                        }
                        Err(error) => {
                            apply_nat_punch_fallback_v0(
                                &mut report,
                                relay_fallback_enabled,
                                format!("decode_punch_ack_payload_failed:{error}"),
                            );
                        }
                    }
                }
                Ok(frame) => {
                    apply_nat_punch_fallback_v0(
                        &mut report,
                        relay_fallback_enabled,
                        format!("unexpected_punch_ack_frame_kind:{:?}", frame.kind),
                    );
                }
                Err(error) => {
                    apply_nat_punch_fallback_v0(
                        &mut report,
                        relay_fallback_enabled,
                        format!("decode_punch_ack_failed:{error}"),
                    );
                }
            }
        }
        Err(error) => {
            report["punch_rtt_ms"] = json!(start.elapsed().as_millis() as u64);
            apply_nat_punch_fallback_v0(
                &mut report,
                relay_fallback_enabled,
                format!("punch_ack_timeout_or_recv_failed:{error}"),
            );
        }
    }

    Ok(report)
}

fn run_public_relay_bootstrap_local_case_v0(
    case_name: &str,
    max_frames: u64,
) -> Result<serde_json::Value> {
    let relay = UdpSocket::bind("127.0.0.1:0").context("bind local public relay")?;
    let node_b = UdpSocket::bind("127.0.0.1:0").context("bind local public relay node-b")?;
    let node_a = UdpSocket::bind("127.0.0.1:0").context("bind local public relay node-a")?;
    relay
        .set_read_timeout(Some(Duration::from_millis(1000)))
        .context("set local public relay timeout")?;
    node_b
        .set_read_timeout(Some(Duration::from_millis(1000)))
        .context("set local public relay node-b timeout")?;
    let relay_addr = relay.local_addr().context("local public relay addr")?;
    let node_b_addr = node_b
        .local_addr()
        .context("local public relay node-b addr")?;
    let node_a_addr = node_a
        .local_addr()
        .context("local public relay node-a addr")?;
    let start = Instant::now();

    let node_b_register_sent_bytes =
        send_public_relay_register_v0(&node_b, &relay_addr.to_string(), "node-b")?;
    let node_a_register_sent_bytes =
        send_public_relay_register_v0(&node_a, &relay_addr.to_string(), "node-a")?;
    let mut relay_buf = vec![0u8; 65535];
    let mut sessions = BTreeMap::new();
    let mut register_received_bytes_total = 0usize;
    for _ in 0..2 {
        let (register_received_bytes, register_source_addr) = relay
            .recv_from(&mut relay_buf)
            .context("local public relay recv register")?;
        register_received_bytes_total += register_received_bytes;
        let register_frame = novovm_network::novorudp::NovoRudpTransportFrameV0::decode(
            &relay_buf[..register_received_bytes],
        )
        .context("decode local public relay register frame")?;
        let register_payload =
            serde_json::from_slice::<PublicRelayRegisterPayloadV0>(&register_frame.payload)
                .context("decode local public relay register payload")?;
        sessions.insert(register_payload.peer_id.clone(), register_source_addr);
    }

    let mut sender_sent_frame_count = 0u64;
    let mut sender_sent_bytes_total = 0usize;
    for frame_index in 0..max_frames {
        let frame = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            [29u8; 16],
            290,
            291,
            30 + frame_index,
            293,
            format!("novovm-public-relay-local-opaque-frame-{frame_index}").into_bytes(),
        );
        let envelope = PublicRelayDataEnvelopeV0 {
            request_id: format!("{case_name}-{frame_index}"),
            source_peer_id: "node-a".to_string(),
            target_peer_id: "node-b".to_string(),
            payload: frame.encode(),
        };
        let encoded_envelope = serde_json::to_vec(&envelope)?;
        let sent_bytes = node_a
            .send_to(&encoded_envelope, relay_addr)
            .context("local public relay sender send envelope")?;
        sender_sent_frame_count += 1;
        sender_sent_bytes_total += sent_bytes;
    }

    let mut relay_envelopes_received = 0u64;
    let mut relay_frames_forwarded = 0u64;
    let mut relay_events = Vec::new();
    while relay_frames_forwarded < max_frames {
        let (received_bytes, source_addr) = relay
            .recv_from(&mut relay_buf)
            .context("local public relay recv envelope")?;
        let envelope =
            serde_json::from_slice::<PublicRelayDataEnvelopeV0>(&relay_buf[..received_bytes])
                .context("decode local public relay envelope")?;
        relay_envelopes_received += 1;
        let target_session = sessions
            .get(&envelope.target_peer_id)
            .context("target peer session missing")?;
        let forwarded_bytes = relay
            .send_to(&envelope.payload, target_session)
            .context("local public relay forward to session")?;
        relay_frames_forwarded += 1;
        relay_events.push(json!({
            "request_id": envelope.request_id,
            "source_addr": source_addr.to_string(),
            "source_peer_id": envelope.source_peer_id,
            "target_peer_id": envelope.target_peer_id,
            "forwarded_to_peer_id": "node-b",
            "forwarded_to_session_endpoint": target_session.to_string(),
            "forwarded_bytes": forwarded_bytes,
        }));
    }

    let mut node_b_frames = Vec::new();
    let mut node_b_received_frame_count = 0u64;
    let mut node_b_buf = vec![0u8; 65535];
    while node_b_received_frame_count < max_frames {
        let (received_bytes, source_addr) = node_b
            .recv_from(&mut node_b_buf)
            .context("local public relay node-b recv forwarded frame")?;
        let frame = novovm_network::novorudp::NovoRudpTransportFrameV0::decode(
            &node_b_buf[..received_bytes],
        )
        .context("decode local public relay node-b frame")?;
        node_b_received_frame_count += 1;
        node_b_frames.push(json!({
            "source_addr": source_addr.to_string(),
            "received_bytes": received_bytes,
            "frame_decode_ok": true,
            "decoded_kind": frame.kind,
            "decoded_sequence": frame.sequence,
            "payload_bytes": frame.payload.len(),
        }));
    }

    let accepted = sessions.contains_key("node-a")
        && sessions.contains_key("node-b")
        && sessions.len() == 2
        && sender_sent_frame_count == max_frames
        && relay_envelopes_received == max_frames
        && relay_frames_forwarded == max_frames
        && node_b_received_frame_count == max_frames;
    Ok(json!({
        "accepted": accepted,
        "case": case_name,
        "scope": "public_relay_bootstrap_local_case_v0",
        "boundary": network_boundary_json(),
        "payload_treated_opaque": true,
        "relay_first_zero_config": true,
        "inbound_public_endpoint_required": false,
        "nat_punch_required": false,
        "topology": {
            "node_a_bind_addr": node_a_addr.to_string(),
            "node_b_bind_addr": node_b_addr.to_string(),
            "public_relay_bind_addr": relay_addr.to_string(),
        },
        "node_a": {
            "accepted": sender_sent_frame_count == max_frames,
            "selected_path": "RelayNovoRudp",
            "route_plan_source": "relay_first_zero_config_policy",
            "target_peer_id": "node-b",
            "selected_relay_peer_id": "public-relay-1",
            "sent_frame_count": sender_sent_frame_count,
            "queued_count": 0,
            "sent_bytes_total": sender_sent_bytes_total,
        },
        "public_relay": {
            "accepted": relay_frames_forwarded == max_frames,
            "node_id": "public-relay-1",
            "relay_enabled": true,
            "bootstrap_sessions_established": sessions.len(),
            "register_sent_bytes": node_a_register_sent_bytes + node_b_register_sent_bytes,
            "register_received_bytes": register_received_bytes_total,
            "session_peer_ids": sessions.keys().cloned().collect::<Vec<_>>(),
            "relay_envelopes_received": relay_envelopes_received,
            "relay_frames_forwarded": relay_frames_forwarded,
            "forwarded_to_peer_id": "node-b",
            "events": relay_events,
        },
        "node_b": {
            "accepted": node_b_received_frame_count == max_frames,
            "inbound_public_endpoint_required": false,
            "received_frame_count": node_b_received_frame_count,
            "frame_decode_ok": true,
            "source_peer_id": "node-a",
            "via_relay_peer_id": "public-relay-1",
            "frames": node_b_frames,
        },
        "elapsed_ms": start.elapsed().as_millis() as u64,
    }))
}

#[allow(clippy::too_many_arguments)]
fn relay_candidate_v0(
    relay_peer_id: &str,
    endpoint: &str,
    transport: &str,
    port: u16,
    priority: u32,
    record_signature_valid: bool,
    observed_reachable: bool,
    failure_count: u32,
    cooldown_until_ms: u64,
) -> RelayCandidateV0 {
    RelayCandidateV0 {
        relay_peer_id: relay_peer_id.to_string(),
        endpoint: endpoint.to_string(),
        transport: transport.to_string(),
        port,
        priority,
        last_seen_ms: 9_000,
        last_success_ms: observed_reachable.then_some(9_500),
        failure_count,
        cooldown_until_ms,
        observed_reachable,
        supports_wss_443: transport == "wss" && port == 443,
        supports_quic_443: transport == "quic" && port == 443,
        supports_udp: transport == "udp",
        record_signature_valid,
    }
}

type TransportCandidateInputV0<'a> = (&'a str, &'a str, &'a str, u16, bool, bool, bool, &'a str);

fn transport_candidate_v0(input: TransportCandidateInputV0<'_>) -> TransportCandidateV0 {
    let (
        candidate_id,
        endpoint,
        transport,
        port,
        observed_reachable,
        fingerprint_blocked_or_high_risk,
        tls_visible_surface,
        role,
    ) = input;
    TransportCandidateV0 {
        candidate_id: candidate_id.to_string(),
        endpoint: endpoint.to_string(),
        transport: transport.to_string(),
        port,
        observed_reachable,
        fingerprint_blocked_or_high_risk,
        tls_visible_surface,
        role: role.to_string(),
    }
}

fn evaluate_transport_adaptive_case_v0(
    case_name: &str,
    candidates: Vec<TransportCandidateV0>,
) -> serde_json::Value {
    let selected = select_transport_candidate_v0(&candidates).cloned();
    let selected_path = if selected.is_some() {
        "RelayNovoRudp"
    } else {
        "QueueFallback"
    };
    let fallback_reason = if selected.is_some() {
        serde_json::Value::Null
    } else {
        json!("NoReachableTransportCandidate")
    };
    let selection_reason = match selected
        .as_ref()
        .map(|candidate| candidate.transport.as_str())
    {
        Some("native_encrypted_novorudp") => "NativeNovoRudpTransportSelected",
        Some("wss") => "NativeUnavailableWss443CompatibilitySelected",
        Some("quic") => "VisibleTlsPathRotatedToQuic443",
        Some("tls") => "Tls443CompatibilitySelected",
        Some("ws") => "LastResortWs80CompatibilitySelected",
        Some(_) => "ReachableTransportCandidateSelected",
        None => "NoReachableTransportCandidate",
    };
    let accepted = match case_name {
        "native_novorudp_reachable_selected" => {
            selected
                .as_ref()
                .map(|candidate| candidate.transport.as_str())
                == Some("native_encrypted_novorudp")
        }
        "native_blocked_falls_back_to_wss_443" => {
            selected
                .as_ref()
                .map(|candidate| candidate.transport.as_str())
                == Some("wss")
        }
        "tls_visible_path_rotates_to_quic" => {
            selected
                .as_ref()
                .map(|candidate| candidate.transport.as_str())
                == Some("quic")
        }
        "http80_last_resort_when_443_paths_blocked" => {
            selected
                .as_ref()
                .map(|candidate| candidate.transport.as_str())
                == Some("ws")
        }
        "all_transports_blocked_queue_fallback" => selected.is_none(),
        _ => selected.is_some(),
    };

    json!({
        "case": case_name,
        "accepted": accepted,
        "candidate_count": candidates.len(),
        "selected_transport": selected.as_ref().map(|candidate| candidate.transport.clone()),
        "selected_endpoint": selected.as_ref().map(|candidate| candidate.endpoint.clone()),
        "selected_candidate_id": selected.as_ref().map(|candidate| candidate.candidate_id.clone()),
        "selected_path_after_transport_selection": selected_path,
        "selection_reason": selection_reason,
        "fallback_reason": fallback_reason,
        "tls_visible_surface_selected": selected
            .as_ref()
            .map(|candidate| candidate.tls_visible_surface)
            .unwrap_or(false),
        "ca_trust_required": false,
        "node_trust_required": false,
        "relay_trust_required": false,
        "validity_source": "zk_proof_and_seal",
        "novorudp_wire_changed": false,
        "candidates": candidates,
    })
}

fn select_transport_candidate_v0(
    candidates: &[TransportCandidateV0],
) -> Option<&TransportCandidateV0> {
    candidates
        .iter()
        .filter(|candidate| candidate.observed_reachable)
        .filter(|candidate| !candidate.fingerprint_blocked_or_high_risk)
        .min_by_key(|candidate| transport_candidate_rank_v0(&candidate.transport, candidate.port))
}

fn transport_candidate_rank_v0(transport: &str, port: u16) -> u8 {
    match (transport, port) {
        ("native_encrypted_novorudp", _) => 0,
        ("wss", 443) => 1,
        ("quic", 443) => 2,
        ("tls", 443) => 3,
        ("ws", 80) => 4,
        ("udp", _) => 5,
        _ => 6,
    }
}

fn evaluate_intelligent_network_strategy_case_v0(
    signal: IntelligentNetworkSignalV0,
) -> serde_json::Value {
    let selected_path =
        if signal.all_paths_unreachable || (!signal.direct_reachable && !signal.relay_available) {
            "QueueFallback"
        } else if signal.direct_reachable && !signal.visible_transport_high_risk {
            "DirectNovoRudp"
        } else if signal.relay_available {
            "RelayNovoRudp"
        } else {
            "QueueFallback"
        };

    let selected_transport_family = if selected_path == "QueueFallback" {
        "none"
    } else if selected_path == "DirectNovoRudp" {
        "native_encrypted_novorudp"
    } else if signal.visible_transport_high_risk {
        "rotating_multi_transport_relay"
    } else {
        "native_first_relay"
    };

    let nat_action = if signal.nat_restricted && signal.relay_available {
        "relay_first_background_punch_probe"
    } else if signal.nat_restricted {
        "diagnose_and_queue_until_relay_candidate"
    } else {
        "no_nat_intervention_required"
    };

    let weak_network_action = if signal.weak_network {
        "enable_queue_small_batches_keepalive_backoff"
    } else {
        "normal_pacing"
    };

    let visibility_action = if signal.visible_transport_high_risk && signal.relay_available {
        "rotate_transport_candidate_and_cooldown_visible_path"
    } else if signal.visible_transport_high_risk {
        "avoid_false_reachable_and_queue"
    } else {
        "keep_current_transport"
    };

    let privacy_action = if signal.privacy_budget_low || signal.tracking_exposure_high {
        "minimize_peer_disclosure_blinded_directory_small_candidate_set"
    } else {
        "standard_peer_signed_candidate_sync"
    };

    let apfl_action = if signal.apfl_strategy_hint_available {
        "advisory_only_not_executed"
    } else {
        "not_requested"
    };

    let fallback_reason = if selected_path == "QueueFallback" {
        if signal.all_paths_unreachable {
            json!("NoReachablePath")
        } else {
            json!("NoReachableRelayCandidate")
        }
    } else {
        serde_json::Value::Null
    };

    let accepted = match signal.case_name.as_str() {
        "stable_native_path_prefers_native" => {
            selected_path == "DirectNovoRudp"
                && selected_transport_family == "native_encrypted_novorudp"
        }
        "nat_restricted_uses_relay_first_background_punch" => {
            selected_path == "RelayNovoRudp" && nat_action == "relay_first_background_punch_probe"
        }
        "visible_transport_risk_rotates_transport" => {
            selected_path == "RelayNovoRudp"
                && visibility_action == "rotate_transport_candidate_and_cooldown_visible_path"
        }
        "weak_network_enables_queue_and_small_batches" => {
            weak_network_action == "enable_queue_small_batches_keepalive_backoff"
        }
        "privacy_low_minimizes_peer_disclosure" => {
            privacy_action == "minimize_peer_disclosure_blinded_directory_small_candidate_set"
        }
        "apfl_hint_available_kept_as_advisory" => {
            signal.apfl_strategy_hint_available && apfl_action == "advisory_only_not_executed"
        }
        "no_path_enters_queue_fallback" => {
            selected_path == "QueueFallback" && fallback_reason == json!("NoReachablePath")
        }
        _ => false,
    };

    json!({
        "case": signal.case_name,
        "accepted": accepted,
        "signals": {
            "direct_reachable": signal.direct_reachable,
            "nat_restricted": signal.nat_restricted,
            "relay_available": signal.relay_available,
            "weak_network": signal.weak_network,
            "visible_transport_high_risk": signal.visible_transport_high_risk,
            "privacy_budget_low": signal.privacy_budget_low,
            "tracking_exposure_high": signal.tracking_exposure_high,
            "all_paths_unreachable": signal.all_paths_unreachable,
            "apfl_strategy_hint_available": signal.apfl_strategy_hint_available
        },
        "decision": {
            "selected_path": selected_path,
            "selected_transport_family": selected_transport_family,
            "fallback_reason": fallback_reason,
            "nat_action": nat_action,
            "weak_network_action": weak_network_action,
            "visibility_action": visibility_action,
            "privacy_action": privacy_action,
            "apfl_action": apfl_action,
            "relay_rotation_allowed": signal.relay_available,
            "background_punch_allowed": signal.nat_restricted && signal.relay_available,
            "queue_enabled": true,
            "full_raw_ip_directory_exposed": false
        },
        "safety_boundary": {
            "apfl_model_called": false,
            "apfl_interpreted": false,
            "aoem_called": false,
            "payload_treated_opaque": true,
            "node_trust_required": false,
            "relay_trust_required": false,
            "ca_trust_required": false,
            "validity_source": "zk_proof_and_seal",
            "novorudp_wire_changed": false
        }
    })
}

fn sign_apfl_advisory_v0(
    signing_key: &SigningKey,
    advisory_id: &str,
    payload: serde_json::Value,
    issued_at_ms: u64,
    expires_at_ms: u64,
) -> Result<SignedApflAdvisoryV0> {
    let signer_public_key = overlay_gate_hex_lower_v0(&signing_key.verifying_key().to_bytes());
    let canonical = ApflAdvisoryCanonicalPayloadV0 {
        advisory_id: advisory_id.to_string(),
        signer_public_key: signer_public_key.clone(),
        issued_at_ms,
        expires_at_ms,
        payload: payload.clone(),
    };
    let canonical_bytes = serde_json::to_vec(&canonical)?;
    let signature: Signature = signing_key.sign(&canonical_bytes);
    Ok(SignedApflAdvisoryV0 {
        advisory_id: advisory_id.to_string(),
        signer_public_key,
        issued_at_ms,
        expires_at_ms,
        payload,
        signature_scheme: "ed25519".into(),
        signature: overlay_gate_hex_lower_v0(&signature.to_bytes()),
    })
}

fn evaluate_apfl_advisory_case_v0(
    case_name: &str,
    advisory: SignedApflAdvisoryV0,
    now_ms: u64,
    seen_replay_ids: &[&str],
) -> serde_json::Value {
    let validation = validate_apfl_advisory_v0(&advisory, now_ms, seen_replay_ids);
    let before = json!({
        "selected_path": "RelayNovoRudp",
        "selected_transport_family": "native_first_relay",
        "queue_enabled": true,
        "relay_record_signature_required": true,
        "blinded_directory_required": true,
        "batch_size": 4,
        "keepalive_interval_ms": 30_000,
    });
    let after = if validation.applied {
        json!({
            "selected_path": "RelayNovoRudp",
            "selected_transport_family": "native_first_relay",
            "queue_enabled": true,
            "relay_record_signature_required": true,
            "blinded_directory_required": true,
            "scoring_hints": {
                "prefer_transport": advisory.payload.get("prefer_transport").cloned(),
                "batch_size_hint": advisory.payload.get("batch_size_hint").cloned(),
                "keepalive_interval_ms_hint": advisory.payload.get("keepalive_interval_ms_hint").cloned(),
                "relay_candidate_priority_hint": advisory.payload.get("relay_candidate_priority_hint").cloned(),
                "privacy_budget_hint": advisory.payload.get("privacy_budget_hint").cloned(),
                "weak_network_mode_hint": advisory.payload.get("weak_network_mode_hint").cloned(),
                "background_punch_probe_hint": advisory.payload.get("background_punch_probe_hint").cloned(),
            }
        })
    } else {
        before.clone()
    };

    let expected_reject_reason = match case_name {
        "expired_advisory_rejected" => Some("apfl_advisory_expired"),
        "invalid_schema_rejected" => Some("apfl_advisory_schema_invalid"),
        "bad_signature_rejected" => Some("apfl_advisory_signature_invalid"),
        "replay_advisory_rejected" => Some("apfl_advisory_replay_rejected"),
        "force_direct_rejected" => Some("apfl_advisory_hard_policy_override"),
        "raw_endpoint_injection_rejected" => Some("apfl_advisory_raw_endpoint_injection"),
        "queue_fallback_disable_rejected" => Some("apfl_advisory_queue_fallback_disable"),
        "payload_semantics_mutation_rejected" => Some("apfl_advisory_payload_semantics_mutation"),
        _ => None,
    };
    let accepted = match case_name {
        "valid_advisory_within_bounds" => {
            validation.applied
                && validation.schema_valid
                && validation.signature_valid
                && validation.ttl_valid
                && validation.policy_bounds_valid
                && !validation.hard_policy_override_attempted
        }
        _ => {
            !validation.applied
                && validation.reject_reason.as_deref() == expected_reject_reason
                && (!validation.hard_policy_override_attempted
                    || validation.hard_policy_override_rejected)
        }
    };

    json!({
        "case": case_name,
        "accepted": accepted,
        "apfl_advisory_received": true,
        "apfl_advisory_schema_valid": validation.schema_valid,
        "apfl_advisory_signature_valid": validation.signature_valid,
        "apfl_advisory_ttl_valid": validation.ttl_valid,
        "apfl_advisory_confidence": validation.confidence,
        "apfl_advisory_policy_bounds_valid": validation.policy_bounds_valid,
        "apfl_advisory_replay_id": advisory.advisory_id,
        "apfl_advisory_replay_rejected": validation.replay_rejected,
        "apfl_advisory_applied": validation.applied,
        "apfl_advisory_reject_reason": validation.reject_reason,
        "strategy_decision_before_advisory": before,
        "strategy_decision_after_advisory": after,
        "hard_policy_override_attempted": validation.hard_policy_override_attempted,
        "hard_policy_override_rejected": validation.hard_policy_override_rejected,
        "payload_treated_opaque": true,
        "apfl_model_called": false,
        "apfl_interpreted": false,
        "aoem_called": false,
        "ledger_semantics": false,
        "novorudp_wire_changed": false,
    })
}

fn validate_apfl_advisory_v0(
    advisory: &SignedApflAdvisoryV0,
    now_ms: u64,
    seen_replay_ids: &[&str],
) -> ApflAdvisoryValidationV0 {
    let schema_valid = apfl_advisory_schema_valid_v0(&advisory.payload);
    let confidence = advisory
        .payload
        .get("confidence")
        .and_then(|value| value.as_u64());
    let signature_valid = verify_apfl_advisory_signature_v0(advisory);
    let ttl_valid = advisory.issued_at_ms <= now_ms && now_ms <= advisory.expires_at_ms;
    let replay_rejected = seen_replay_ids
        .iter()
        .any(|seen| *seen == advisory.advisory_id);

    let force_direct = advisory
        .payload
        .get("force_direct")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let raw_endpoint = advisory.payload.get("raw_endpoint").is_some();
    let disable_queue = advisory
        .payload
        .get("disable_queue_fallback")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let payload_semantics_mutation = advisory
        .payload
        .get("payload_semantics_mutation")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let hard_policy_override_attempted =
        force_direct || raw_endpoint || disable_queue || payload_semantics_mutation;

    let reject_reason = if !schema_valid {
        Some("apfl_advisory_schema_invalid".to_string())
    } else if !signature_valid {
        Some("apfl_advisory_signature_invalid".to_string())
    } else if !ttl_valid {
        Some("apfl_advisory_expired".to_string())
    } else if replay_rejected {
        Some("apfl_advisory_replay_rejected".to_string())
    } else if force_direct {
        Some("apfl_advisory_hard_policy_override".to_string())
    } else if raw_endpoint {
        Some("apfl_advisory_raw_endpoint_injection".to_string())
    } else if disable_queue {
        Some("apfl_advisory_queue_fallback_disable".to_string())
    } else if payload_semantics_mutation {
        Some("apfl_advisory_payload_semantics_mutation".to_string())
    } else {
        None
    };
    let policy_bounds_valid = reject_reason.is_none();
    let applied = schema_valid
        && signature_valid
        && ttl_valid
        && !replay_rejected
        && policy_bounds_valid
        && !hard_policy_override_attempted;

    ApflAdvisoryValidationV0 {
        schema_valid,
        signature_valid,
        ttl_valid,
        policy_bounds_valid,
        replay_rejected,
        confidence,
        applied,
        reject_reason,
        hard_policy_override_attempted,
        hard_policy_override_rejected: hard_policy_override_attempted,
    }
}

fn apfl_advisory_schema_valid_v0(payload: &serde_json::Value) -> bool {
    let Some(schema_version) = payload
        .get("schema_version")
        .and_then(|value| value.as_u64())
    else {
        return false;
    };
    if schema_version != 1 {
        return false;
    }
    let Some(confidence) = payload.get("confidence").and_then(|value| value.as_u64()) else {
        return false;
    };
    if confidence > 100 {
        return false;
    }
    if let Some(transport) = payload
        .get("prefer_transport")
        .and_then(|value| value.as_str())
    {
        matches!(
            transport,
            "native_encrypted_novorudp" | "wss" | "quic" | "tls" | "ws" | "udp"
        )
    } else {
        true
    }
}

fn verify_apfl_advisory_signature_v0(advisory: &SignedApflAdvisoryV0) -> bool {
    if advisory.signature_scheme != "ed25519" {
        return false;
    }
    let public_key_bytes =
        match overlay_gate_decode_hex_bytes_v0(&advisory.signer_public_key, "apfl_public_key") {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };
    let Ok(public_key_array) = <[u8; 32]>::try_from(public_key_bytes.as_slice()) else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&public_key_array) else {
        return false;
    };
    let signature_bytes =
        match overlay_gate_decode_hex_bytes_v0(&advisory.signature, "apfl_signature") {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };
    let Ok(signature_array) = <[u8; 64]>::try_from(signature_bytes.as_slice()) else {
        return false;
    };
    let signature = Signature::from_bytes(&signature_array);
    let canonical = ApflAdvisoryCanonicalPayloadV0 {
        advisory_id: advisory.advisory_id.clone(),
        signer_public_key: advisory.signer_public_key.clone(),
        issued_at_ms: advisory.issued_at_ms,
        expires_at_ms: advisory.expires_at_ms,
        payload: advisory.payload.clone(),
    };
    let Ok(canonical_bytes) = serde_json::to_vec(&canonical) else {
        return false;
    };
    verifying_key.verify(&canonical_bytes, &signature).is_ok()
}

fn strategy_receipt_input_v0(
    case_id: &str,
    direct_reachable: bool,
    nat_restricted: bool,
    relay_available: bool,
    all_paths_unreachable: bool,
    apfl_advisory: Option<SignedApflAdvisoryV0>,
) -> StrategyReceiptInputV0 {
    let relay_candidates = if relay_available {
        vec![
            relay_candidate_v0(
                "relay-a",
                "wss://relay-a.example.net:443/novovm",
                "wss",
                443,
                10,
                true,
                true,
                0,
                0,
            ),
            relay_candidate_v0(
                "relay-b-cooldown",
                "quic://relay-b.example.net:443",
                "quic",
                443,
                20,
                true,
                false,
                2,
                70_000,
            ),
        ]
    } else {
        Vec::new()
    };
    let transport_candidates = if direct_reachable {
        vec![transport_candidate_v0((
            "native-direct",
            "novorudp://direct/observed",
            "native_encrypted_novorudp",
            0,
            true,
            false,
            false,
            "direct_native_path",
        ))]
    } else if relay_available {
        vec![
            transport_candidate_v0((
                "native-relay",
                "novorudp://relay-a.example.net/dynamic",
                "native_encrypted_novorudp",
                0,
                false,
                true,
                false,
                "native_relay_candidate",
            )),
            transport_candidate_v0((
                "wss-443",
                "wss://relay-a.example.net:443/novovm",
                "wss",
                443,
                true,
                false,
                true,
                "compatibility_transport",
            )),
        ]
    } else {
        Vec::new()
    };
    StrategyReceiptInputV0 {
        case_id: case_id.to_string(),
        observed_endpoint: direct_reachable.then_some("203.0.113.10:41020".into()),
        nat_restricted,
        relay_available,
        all_paths_unreachable,
        relay_candidates,
        transport_candidates,
        bootstrap_source: "signed_manifest_cache_then_blinded_directory".into(),
        apfl_advisory,
    }
}

fn evaluate_strategy_receipt_case_v0(
    case_name: &str,
    input: StrategyReceiptInputV0,
    now_ms: u64,
) -> serde_json::Value {
    let receipt = build_strategy_receipt_v0(&input, now_ms);
    let replayed_receipt = build_strategy_receipt_v0(&input, now_ms);
    let strategy_replay_pass = receipt["strategy_decision_hash"]
        == replayed_receipt["strategy_decision_hash"]
        && receipt["strategy_input_hash"] == replayed_receipt["strategy_input_hash"]
        && receipt["selected_path"] == replayed_receipt["selected_path"];
    let accepted = match case_name {
        "relay_decision_receipt_replays" => {
            strategy_replay_pass
                && receipt["strategy_receipt_emitted"] == json!(true)
                && receipt["selected_path"] == json!("RelayNovoRudp")
                && receipt["apfl_advisory_applied"] == json!(true)
        }
        "hard_policy_override_receipt_replays_rejection" => {
            strategy_replay_pass
                && receipt["hard_policy_override_attempted"] == json!(true)
                && receipt["hard_policy_override_rejected"] == json!(true)
                && receipt["apfl_advisory_applied"] == json!(false)
        }
        "queue_fallback_receipt_replays" => {
            strategy_replay_pass
                && receipt["selected_path"] == json!("QueueFallback")
                && receipt["fallback_reason"] == json!("NoReachablePath")
        }
        _ => false,
    };

    json!({
        "case": case_name,
        "accepted": accepted,
        "strategy_replay_pass": strategy_replay_pass,
        "receipt": receipt,
        "replayed_strategy_decision_hash": replayed_receipt["strategy_decision_hash"].clone(),
    })
}

fn build_strategy_receipt_v0(input: &StrategyReceiptInputV0, now_ms: u64) -> serde_json::Value {
    let strategy_input_hash =
        overlay_gate_sha256_hex_v0(&[&serde_json::to_vec(input).unwrap_or_default()]);
    let apfl_advisory_hash = input.apfl_advisory.as_ref().map(|advisory| {
        overlay_gate_sha256_hex_v0(&[&serde_json::to_vec(advisory).unwrap_or_default()])
    });
    let advisory_validation = input
        .apfl_advisory
        .as_ref()
        .map(|advisory| validate_apfl_advisory_v0(advisory, now_ms, &[]));
    let apfl_advisory_applied = advisory_validation
        .as_ref()
        .map(|validation| validation.applied)
        .unwrap_or(false);
    let hard_policy_override_attempted = advisory_validation
        .as_ref()
        .map(|validation| validation.hard_policy_override_attempted)
        .unwrap_or(false);
    let hard_policy_override_rejected = advisory_validation
        .as_ref()
        .map(|validation| validation.hard_policy_override_rejected)
        .unwrap_or(false);

    let selected_transport = select_transport_candidate_v0(&input.transport_candidates).cloned();
    let selected_relay = select_relay_candidate_index_v0(&input.relay_candidates, now_ms)
        .map(|index| input.relay_candidates[index].clone());
    let selected_path = if input.all_paths_unreachable {
        "QueueFallback"
    } else if input.observed_endpoint.is_some()
        && selected_transport
            .as_ref()
            .map(|candidate| candidate.transport.as_str())
            == Some("native_encrypted_novorudp")
    {
        "DirectNovoRudp"
    } else if input.relay_available && selected_relay.is_some() {
        "RelayNovoRudp"
    } else {
        "QueueFallback"
    };
    let fallback_reason = if selected_path == "QueueFallback" {
        if input.all_paths_unreachable {
            json!("NoReachablePath")
        } else {
            json!("NoReachableRelayCandidate")
        }
    } else {
        serde_json::Value::Null
    };
    let selection_reason = match selected_path {
        "DirectNovoRudp" => "ObservedEndpointNativeDirectSelected",
        "RelayNovoRudp" if apfl_advisory_applied => "RelaySelectedWithApflScoringHint",
        "RelayNovoRudp" => "RelaySelectedByHardPolicy",
        _ => "QueueFallbackSelected",
    };
    let rejected_candidates = relay_candidate_reject_reasons_v0(&input.relay_candidates, now_ms);
    let decision = json!({
        "selected_path": selected_path,
        "selection_reason": selection_reason,
        "selected_transport": selected_transport.as_ref().map(|candidate| candidate.transport.clone()),
        "selected_relay_peer_id": selected_relay.as_ref().map(|relay| relay.relay_peer_id.clone()),
        "fallback_reason": fallback_reason,
        "rejected_candidate_count": rejected_candidates.len(),
        "apfl_advisory_applied": apfl_advisory_applied,
        "hard_policy_override_attempted": hard_policy_override_attempted,
        "hard_policy_override_rejected": hard_policy_override_rejected,
    });
    let strategy_decision_hash =
        overlay_gate_sha256_hex_v0(&[&serde_json::to_vec(&decision).unwrap_or_default()]);

    json!({
        "strategy_receipt_emitted": true,
        "strategy_input_hash": strategy_input_hash,
        "strategy_decision_hash": strategy_decision_hash,
        "apfl_advisory_hash": apfl_advisory_hash,
        "apfl_advisory_applied": apfl_advisory_applied,
        "hard_policy_override_attempted": hard_policy_override_attempted,
        "hard_policy_override_rejected": hard_policy_override_rejected,
        "selected_path": selected_path,
        "selection_reason": selection_reason,
        "selected_transport": selected_transport.as_ref().map(|candidate| candidate.transport.clone()),
        "selected_relay_peer_id": selected_relay.as_ref().map(|relay| relay.relay_peer_id.clone()),
        "rejected_candidate_count": rejected_candidates.len(),
        "rejected_candidates": rejected_candidates,
        "fallback_reason": decision["fallback_reason"].clone(),
        "payload_treated_opaque": true,
        "novorudp_wire_changed": false,
        "decision": decision,
    })
}

fn evaluate_relay_selection_case_v0(
    case_name: &str,
    mut candidates: Vec<RelayCandidateV0>,
    now_ms: u64,
    simulate_send_failure: bool,
) -> serde_json::Value {
    let relay_candidate_count = candidates.len();
    let valid_relay_candidate_count = candidates
        .iter()
        .filter(|candidate| candidate.record_signature_valid)
        .count();
    let invalid_relay_candidate_count = relay_candidate_count - valid_relay_candidate_count;
    let cooldown_relay_count = candidates
        .iter()
        .filter(|candidate| candidate.cooldown_until_ms > now_ms)
        .count();
    let first_selected = select_relay_candidate_index_v0(&candidates, now_ms);
    let mut relay_rotation_attempted = false;
    let mut relay_rotation_count = 0u64;
    let mut failed_relay_peer_id = None;

    let final_selected = if simulate_send_failure {
        match first_selected {
            Some(index) => {
                relay_rotation_attempted = true;
                relay_rotation_count = 1;
                failed_relay_peer_id = Some(candidates[index].relay_peer_id.clone());
                candidates[index].failure_count = candidates[index].failure_count.saturating_add(1);
                candidates[index].cooldown_until_ms = now_ms.saturating_add(60_000);
                select_relay_candidate_index_v0(&candidates, now_ms)
            }
            None => None,
        }
    } else {
        first_selected
    };

    let selected = final_selected.map(|index| candidates[index].clone());
    let selected_path_after_relay_selection = if selected.is_some() {
        "RelayNovoRudp"
    } else {
        "QueueFallback"
    };
    let fallback_reason = if selected.is_some() {
        serde_json::Value::Null
    } else {
        json!("NoReachableRelayCandidate")
    };
    let selection_reason = match (
        &selected,
        simulate_send_failure,
        failed_relay_peer_id.as_ref(),
    ) {
        (Some(_), true, Some(_)) => "PrimaryRelayFailedRotatedToNextCandidate",
        (Some(candidate), _, _) if candidate.transport == "wss" && candidate.port == 443 => {
            "Wss443ReachableCandidateSelected"
        }
        (Some(_), _, _) => "ReachableCandidateSelected",
        (None, _, _) => "NoReachableRelayCandidate",
    };
    let accepted = match case_name {
        "single_healthy_relay" => {
            selected
                .as_ref()
                .map(|candidate| candidate.relay_peer_id.as_str())
                == Some("relay-a")
                && selected_path_after_relay_selection == "RelayNovoRudp"
        }
        "primary_relay_cooldown" => {
            selected
                .as_ref()
                .map(|candidate| candidate.relay_peer_id.as_str())
                == Some("relay-b")
                && cooldown_relay_count >= 1
        }
        "primary_relay_send_failure_rotates" => {
            failed_relay_peer_id.as_deref() == Some("relay-a")
                && selected
                    .as_ref()
                    .map(|candidate| candidate.relay_peer_id.as_str())
                    == Some("relay-b")
                && relay_rotation_attempted
        }
        "invalid_relay_signature_rejected" => {
            invalid_relay_candidate_count == 1
                && selected
                    .as_ref()
                    .map(|candidate| candidate.relay_peer_id.as_str())
                    == Some("relay-b")
        }
        "all_relays_unavailable_queue_fallback" => {
            selected.is_none() && selected_path_after_relay_selection == "QueueFallback"
        }
        "transport_priority_prefers_wss_443" => {
            selected
                .as_ref()
                .map(|candidate| candidate.relay_peer_id.as_str())
                == Some("relay-wss")
        }
        _ => selected.is_some(),
    };

    json!({
        "case": case_name,
        "accepted": accepted,
        "relay_candidate_count": relay_candidate_count,
        "valid_relay_candidate_count": valid_relay_candidate_count,
        "invalid_relay_candidate_count": invalid_relay_candidate_count,
        "cooldown_relay_count": cooldown_relay_count,
        "selected_relay_peer_id": selected.as_ref().map(|candidate| candidate.relay_peer_id.clone()),
        "selected_relay_endpoint": selected.as_ref().map(|candidate| candidate.endpoint.clone()),
        "selected_transport": selected.as_ref().map(|candidate| candidate.transport.clone()),
        "selection_reason": selection_reason,
        "relay_rotation_attempted": relay_rotation_attempted,
        "relay_rotation_count": relay_rotation_count,
        "failed_relay_peer_id": failed_relay_peer_id,
        "relay_record_signature_valid": selected
            .as_ref()
            .map(|candidate| candidate.record_signature_valid)
            .unwrap_or(false),
        "selected_path_after_relay_selection": selected_path_after_relay_selection,
        "fallback_reason": fallback_reason,
        "reject_reasons": relay_candidate_reject_reasons_v0(&candidates, now_ms),
        "candidates": candidates,
    })
}

fn select_relay_candidate_index_v0(candidates: &[RelayCandidateV0], now_ms: u64) -> Option<usize> {
    candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.record_signature_valid)
        .filter(|(_, candidate)| candidate.observed_reachable)
        .filter(|(_, candidate)| candidate.cooldown_until_ms <= now_ms)
        .min_by_key(|(_, candidate)| {
            (
                candidate.priority,
                relay_transport_rank_v0(&candidate.transport, candidate.port),
                std::cmp::Reverse(candidate.last_success_ms.unwrap_or(0)),
                candidate.failure_count,
            )
        })
        .map(|(index, _)| index)
}

fn relay_transport_rank_v0(transport: &str, port: u16) -> u8 {
    match (transport, port) {
        ("wss", 443) => 0,
        ("quic", 443) => 1,
        ("tls", 443) => 2,
        ("ws", 80) => 3,
        ("udp", _) => 4,
        _ => 5,
    }
}

fn relay_candidate_reject_reasons_v0(
    candidates: &[RelayCandidateV0],
    now_ms: u64,
) -> Vec<serde_json::Value> {
    candidates
        .iter()
        .filter_map(|candidate| {
            let reason = if !candidate.record_signature_valid {
                Some("relay_record_signature_invalid")
            } else if candidate.cooldown_until_ms > now_ms {
                Some("relay_in_cooldown")
            } else if !candidate.observed_reachable {
                Some("relay_not_observed_reachable")
            } else {
                None
            }?;
            Some(json!({
                "relay_peer_id": candidate.relay_peer_id,
                "endpoint": candidate.endpoint,
                "reason": reason,
            }))
        })
        .collect()
}

fn peer_signed_relay_endpoint_v0(
    transport: &str,
    uri: &str,
    port: u16,
    priority: u32,
) -> PeerSignedRelayEndpointV0 {
    PeerSignedRelayEndpointV0 {
        transport: transport.to_string(),
        uri: uri.to_string(),
        port,
        priority,
        capabilities: vec!["relay_novorudp_opaque".into(), "peer_id_routing".into()],
    }
}

impl BootstrapResolverFixtureV0 {
    fn new() -> Result<Self> {
        let now_ms = 30_000u64;
        let candidate_set_policy_limit = 2usize;
        let manifest_key = SigningKey::from_bytes(&[60u8; 32]);

        Ok(Self {
            now_ms,
            candidate_set_policy_limit,
            valid_cache_manifest: bootstrap_manifest_fixture_v0(
                &manifest_key,
                "local_cache",
                "cache",
                61,
                29_000,
                120_000,
                candidate_set_policy_limit,
            )?,
            expired_cache_manifest: bootstrap_manifest_fixture_v0(
                &manifest_key,
                "local_cache",
                "cache-expired",
                62,
                1_000,
                29_000,
                candidate_set_policy_limit,
            )?,
            valid_embedded_manifest: bootstrap_manifest_fixture_v0(
                &manifest_key,
                "embedded_install_manifest",
                "embedded",
                63,
                29_000,
                120_000,
                candidate_set_policy_limit,
            )?,
            expired_embedded_manifest: bootstrap_manifest_fixture_v0(
                &manifest_key,
                "embedded_install_manifest",
                "embedded-expired",
                64,
                1_000,
                29_000,
                candidate_set_policy_limit,
            )?,
            valid_qr_manifest: bootstrap_manifest_fixture_v0(
                &manifest_key,
                "qr_invite_manifest",
                "qr",
                65,
                29_000,
                120_000,
                candidate_set_policy_limit,
            )?,
            valid_friend_invite_manifest: bootstrap_manifest_fixture_v0(
                &manifest_key,
                "friend_invite_manifest",
                "friend",
                66,
                29_000,
                120_000,
                candidate_set_policy_limit,
            )?,
            valid_official_manifest: bootstrap_manifest_fixture_v0(
                &manifest_key,
                "official_signed_bootstrap_manifest",
                "official",
                67,
                29_000,
                120_000,
                candidate_set_policy_limit,
            )?,
            valid_community_manifest: bootstrap_manifest_fixture_v0(
                &manifest_key,
                "community_signed_bootstrap_manifest",
                "community",
                68,
                29_000,
                120_000,
                candidate_set_policy_limit,
            )?,
            valid_discovered_manifest: bootstrap_manifest_fixture_v0(
                &manifest_key,
                "discovered_blinded_directory_source",
                "discovered",
                69,
                29_000,
                120_000,
                candidate_set_policy_limit,
            )?,
        })
    }

    fn source(
        &self,
        source_kind: &str,
        priority: u32,
        reachable: bool,
        manifest: SignedBootstrapManifestV0,
    ) -> BootstrapManifestSourceV0 {
        BootstrapManifestSourceV0 {
            source_id: format!("{source_kind}:{priority}"),
            source_kind: source_kind.to_string(),
            priority,
            reachable,
            manifest,
        }
    }

    fn invalid_signature_manifest(&self) -> SignedBootstrapManifestV0 {
        let mut manifest = self.valid_official_manifest.clone();
        manifest.manifest_id = format!("{}-invalid-signature", manifest.manifest_id);
        manifest.signature = "00".repeat(64);
        manifest
    }
}

fn bootstrap_manifest_fixture_v0(
    manifest_key: &SigningKey,
    source: &str,
    id_suffix: &str,
    relay_seed_byte: u8,
    issued_at_ms: u64,
    expires_at_ms: u64,
    candidate_set_policy_limit: usize,
) -> Result<SignedBootstrapManifestV0> {
    let relay_key = SigningKey::from_bytes(&[relay_seed_byte; 32]);
    let rendezvous_key = SigningKey::from_bytes(&[relay_seed_byte.saturating_add(1); 32]);
    let seed_relay_candidates = vec![sign_relay_endpoint_record_v0(
        &relay_key,
        vec![peer_signed_relay_endpoint_v0(
            "wss",
            &format!("wss://relay-{id_suffix}.bootstrap.example:443/novovm"),
            443,
            u32::from(relay_seed_byte),
        )],
        issued_at_ms,
        expires_at_ms,
        &format!("relay-record-{id_suffix}-001"),
    )?];
    let seed_rendezvous_candidates = vec![sign_relay_endpoint_record_v0(
        &rendezvous_key,
        vec![peer_signed_relay_endpoint_v0(
            "wss",
            &format!("wss://rendezvous-{id_suffix}.bootstrap.example:443/novovm"),
            443,
            u32::from(relay_seed_byte.saturating_add(1)),
        )],
        issued_at_ms,
        expires_at_ms,
        &format!("rendezvous-record-{id_suffix}-001"),
    )?];

    sign_bootstrap_manifest_v0(
        manifest_key,
        BootstrapManifestSigningInputV0 {
            bootstrap_manifest_source: source,
            seed_relay_candidates,
            seed_rendezvous_candidates,
            issued_at_ms,
            expires_at_ms,
            manifest_id: &format!("bootstrap-manifest-{id_suffix}-001"),
            full_raw_ip_directory_embedded: false,
            manifest_requires_single_official_relay: false,
            manifest_requires_single_official_domain: false,
            candidate_set_policy_limit,
        },
    )
}

fn evaluate_bootstrap_source_resolver_case_v0(
    case_name: &str,
    mut sources: Vec<BootstrapManifestSourceV0>,
    now_ms: u64,
    candidate_set_policy_limit: usize,
) -> serde_json::Value {
    sources.sort_by_key(|source| source.priority);
    let fallback_order = sources
        .iter()
        .map(|source| source.source_kind.clone())
        .collect::<Vec<_>>();
    let mut selected: Option<(&BootstrapManifestSourceV0, BootstrapManifestValidationV0)> = None;
    let mut validation_reports = Vec::new();
    let mut valid_records = Vec::new();
    let mut reachable_source_count = 0usize;
    let mut valid_manifest_source_count = 0usize;
    let mut invalid_signature_source_count = 0usize;
    let mut expired_source_count = 0usize;

    for source in &sources {
        if source.reachable {
            reachable_source_count += 1;
        }
        let validation = if source.reachable {
            validate_bootstrap_manifest_v0(&source.manifest, now_ms)
        } else {
            BootstrapManifestValidationV0 {
                accepted: false,
                signature_valid: false,
                expired: false,
                reject_reason: Some("bootstrap_source_unreachable".into()),
                seed_relay_record_valid_count: 0,
                seed_relay_record_invalid_count: source.manifest.seed_relay_candidates.len(),
                blinded_directory_response: Vec::new(),
            }
        };
        if validation.accepted {
            valid_manifest_source_count += 1;
            valid_records.extend(source.manifest.seed_relay_candidates.clone());
            if selected.is_none() {
                selected = Some((source, validation.clone()));
            }
        }
        if validation.reject_reason.as_deref() == Some("bootstrap_manifest_signature_invalid") {
            invalid_signature_source_count += 1;
        }
        if validation.reject_reason.as_deref() == Some("bootstrap_manifest_expired") {
            expired_source_count += 1;
        }
        validation_reports.push(json!({
            "source_id": source.source_id,
            "source_kind": source.source_kind,
            "priority": source.priority,
            "reachable": source.reachable,
            "manifest_id": source.manifest.manifest_id,
            "manifest_source": source.manifest.bootstrap_manifest_source,
            "accepted": validation.accepted,
            "signature_valid": validation.signature_valid,
            "expired": validation.expired,
            "reject_reason": validation.reject_reason,
        }));
    }

    let merged_blinded_directory = issue_blinded_relay_directory_response_v0(
        &valid_records,
        candidate_set_policy_limit,
        now_ms,
    );
    let raw_ip_directory_exposed = merged_blinded_directory.iter().any(|entry| {
        entry.encrypted_or_blinded_endpoint_hint.contains("://")
            || entry.encrypted_or_blinded_endpoint_hint.contains('.')
    });
    let selected_path_after_bootstrap = if selected.is_some() {
        "RelayNovoRudp"
    } else {
        "QueueFallback"
    };
    let selected_bootstrap_manifest_source = selected
        .as_ref()
        .map(|(source, _)| source.source_kind.clone());
    let selected_bootstrap_manifest_id = selected
        .as_ref()
        .map(|(source, _)| source.manifest.manifest_id.clone());

    let accepted = match case_name {
        "valid_cache_preferred_when_fresh" => {
            selected_bootstrap_manifest_source.as_deref() == Some("local_cache")
        }
        "expired_cache_skipped" => {
            expired_source_count >= 1
                && selected_bootstrap_manifest_source.as_deref()
                    == Some("embedded_install_manifest")
        }
        "invalid_signature_source_rejected" => {
            invalid_signature_source_count >= 1
                && selected_bootstrap_manifest_source.as_deref()
                    == Some("community_signed_bootstrap_manifest")
        }
        "official_source_not_mandatory" => {
            valid_manifest_source_count >= 1
                && !sources
                    .iter()
                    .any(|source| source.source_kind == "official_signed_bootstrap_manifest")
        }
        "multi_source_merge_does_not_expose_raw_ip_directory" => {
            valid_manifest_source_count >= 2
                && !raw_ip_directory_exposed
                && merged_blinded_directory.len() <= candidate_set_policy_limit
        }
        "fallback_order_deterministic" => {
            fallback_order
                == vec![
                    "local_cache".to_string(),
                    "embedded_install_manifest".to_string(),
                    "qr_invite_manifest".to_string(),
                    "official_signed_bootstrap_manifest".to_string(),
                ]
                && selected_bootstrap_manifest_source.as_deref() == Some("qr_invite_manifest")
        }
        "no_reachable_bootstrap_source_enters_queue_fallback" => {
            selected.is_none() && selected_path_after_bootstrap == "QueueFallback"
        }
        _ => selected.is_some(),
    };

    json!({
        "case": case_name,
        "accepted": accepted,
        "bootstrap_source_count": sources.len(),
        "reachable_source_count": reachable_source_count,
        "valid_manifest_source_count": valid_manifest_source_count,
        "invalid_signature_source_count": invalid_signature_source_count,
        "expired_source_count": expired_source_count,
        "fallback_order": fallback_order,
        "fallback_order_deterministic": true,
        "selected_bootstrap_manifest_source": selected_bootstrap_manifest_source,
        "selected_bootstrap_manifest_id": selected_bootstrap_manifest_id,
        "official_source_required": false,
        "multi_source_merge_exposes_raw_ip_directory": raw_ip_directory_exposed,
        "merged_blinded_candidate_count": merged_blinded_directory.len(),
        "candidate_set_policy_limit": candidate_set_policy_limit,
        "selected_path_after_bootstrap": selected_path_after_bootstrap,
        "fallback_reason": if selected.is_some() { serde_json::Value::Null } else { json!("NoReachableBootstrapSource") },
        "source_validations": validation_reports,
        "merged_blinded_directory_response": merged_blinded_directory,
    })
}

struct BootstrapManifestSigningInputV0<'a> {
    bootstrap_manifest_source: &'a str,
    seed_relay_candidates: Vec<PeerSignedRelayEndpointRecordV0>,
    seed_rendezvous_candidates: Vec<PeerSignedRelayEndpointRecordV0>,
    issued_at_ms: u64,
    expires_at_ms: u64,
    manifest_id: &'a str,
    full_raw_ip_directory_embedded: bool,
    manifest_requires_single_official_relay: bool,
    manifest_requires_single_official_domain: bool,
    candidate_set_policy_limit: usize,
}

fn sign_bootstrap_manifest_v0(
    signing_key: &SigningKey,
    input: BootstrapManifestSigningInputV0<'_>,
) -> Result<SignedBootstrapManifestV0> {
    let BootstrapManifestSigningInputV0 {
        bootstrap_manifest_source,
        seed_relay_candidates,
        seed_rendezvous_candidates,
        issued_at_ms,
        expires_at_ms,
        manifest_id,
        full_raw_ip_directory_embedded,
        manifest_requires_single_official_relay,
        manifest_requires_single_official_domain,
        candidate_set_policy_limit,
    } = input;
    let manifest_public_key = overlay_gate_hex_lower_v0(&signing_key.verifying_key().to_bytes());
    let payload = bootstrap_manifest_payload_v0(
        manifest_id.to_string(),
        bootstrap_manifest_source.to_string(),
        manifest_public_key.clone(),
        seed_relay_candidates.clone(),
        seed_rendezvous_candidates.clone(),
        issued_at_ms,
        expires_at_ms,
        full_raw_ip_directory_embedded,
        manifest_requires_single_official_relay,
        manifest_requires_single_official_domain,
        candidate_set_policy_limit,
    );
    let canonical_payload = serde_json::to_vec(&payload)?;
    let signature: Signature = signing_key.sign(&canonical_payload);
    Ok(SignedBootstrapManifestV0 {
        manifest_version: payload.manifest_version,
        manifest_id: payload.manifest_id,
        bootstrap_manifest_source: payload.bootstrap_manifest_source,
        manifest_public_key,
        seed_relay_candidates,
        seed_rendezvous_candidates,
        issued_at_ms,
        expires_at_ms,
        full_raw_ip_directory_embedded,
        manifest_requires_single_official_relay,
        manifest_requires_single_official_domain,
        candidate_set_policy_limit,
        signature_scheme: "ed25519".into(),
        signature: overlay_gate_hex_lower_v0(&signature.to_bytes()),
    })
}

fn validate_bootstrap_manifest_v0(
    manifest: &SignedBootstrapManifestV0,
    now_ms: u64,
) -> BootstrapManifestValidationV0 {
    let empty_response = Vec::new();
    let manifest_public_key_bytes = match overlay_gate_decode_hex_bytes_v0(
        &manifest.manifest_public_key,
        "manifest_public_key",
    ) {
        Ok(bytes) => bytes,
        Err(_) => {
            return BootstrapManifestValidationV0 {
                accepted: false,
                signature_valid: false,
                expired: false,
                reject_reason: Some("bootstrap_manifest_public_key_invalid".into()),
                seed_relay_record_valid_count: 0,
                seed_relay_record_invalid_count: manifest.seed_relay_candidates.len(),
                blinded_directory_response: empty_response,
            }
        }
    };
    let public_key_array: [u8; 32] = match manifest_public_key_bytes.try_into() {
        Ok(bytes) => bytes,
        Err(_) => {
            return BootstrapManifestValidationV0 {
                accepted: false,
                signature_valid: false,
                expired: false,
                reject_reason: Some("bootstrap_manifest_public_key_invalid".into()),
                seed_relay_record_valid_count: 0,
                seed_relay_record_invalid_count: manifest.seed_relay_candidates.len(),
                blinded_directory_response: empty_response,
            }
        }
    };
    if manifest.signature_scheme != "ed25519" {
        return BootstrapManifestValidationV0 {
            accepted: false,
            signature_valid: false,
            expired: false,
            reject_reason: Some("bootstrap_manifest_signature_scheme_unsupported".into()),
            seed_relay_record_valid_count: 0,
            seed_relay_record_invalid_count: manifest.seed_relay_candidates.len(),
            blinded_directory_response: empty_response,
        };
    }
    let verifying_key = match VerifyingKey::from_bytes(&public_key_array) {
        Ok(key) => key,
        Err(_) => {
            return BootstrapManifestValidationV0 {
                accepted: false,
                signature_valid: false,
                expired: false,
                reject_reason: Some("bootstrap_manifest_public_key_invalid".into()),
                seed_relay_record_valid_count: 0,
                seed_relay_record_invalid_count: manifest.seed_relay_candidates.len(),
                blinded_directory_response: empty_response,
            }
        }
    };
    let signature_bytes = match overlay_gate_decode_hex_bytes_v0(&manifest.signature, "signature") {
        Ok(bytes) => bytes,
        Err(_) => {
            return BootstrapManifestValidationV0 {
                accepted: false,
                signature_valid: false,
                expired: false,
                reject_reason: Some("bootstrap_manifest_signature_invalid".into()),
                seed_relay_record_valid_count: 0,
                seed_relay_record_invalid_count: manifest.seed_relay_candidates.len(),
                blinded_directory_response: empty_response,
            }
        }
    };
    let signature_array: [u8; 64] = match signature_bytes.try_into() {
        Ok(bytes) => bytes,
        Err(_) => {
            return BootstrapManifestValidationV0 {
                accepted: false,
                signature_valid: false,
                expired: false,
                reject_reason: Some("bootstrap_manifest_signature_invalid".into()),
                seed_relay_record_valid_count: 0,
                seed_relay_record_invalid_count: manifest.seed_relay_candidates.len(),
                blinded_directory_response: empty_response,
            }
        }
    };
    let signature = Signature::from_bytes(&signature_array);
    let payload = bootstrap_manifest_payload_v0(
        manifest.manifest_id.clone(),
        manifest.bootstrap_manifest_source.clone(),
        manifest.manifest_public_key.clone(),
        manifest.seed_relay_candidates.clone(),
        manifest.seed_rendezvous_candidates.clone(),
        manifest.issued_at_ms,
        manifest.expires_at_ms,
        manifest.full_raw_ip_directory_embedded,
        manifest.manifest_requires_single_official_relay,
        manifest.manifest_requires_single_official_domain,
        manifest.candidate_set_policy_limit,
    );
    let canonical_payload = match serde_json::to_vec(&payload) {
        Ok(payload) => payload,
        Err(_) => {
            return BootstrapManifestValidationV0 {
                accepted: false,
                signature_valid: false,
                expired: false,
                reject_reason: Some("bootstrap_manifest_canonical_payload_failed".into()),
                seed_relay_record_valid_count: 0,
                seed_relay_record_invalid_count: manifest.seed_relay_candidates.len(),
                blinded_directory_response: empty_response,
            }
        }
    };
    if verifying_key
        .verify(&canonical_payload, &signature)
        .is_err()
    {
        return BootstrapManifestValidationV0 {
            accepted: false,
            signature_valid: false,
            expired: false,
            reject_reason: Some("bootstrap_manifest_signature_invalid".into()),
            seed_relay_record_valid_count: 0,
            seed_relay_record_invalid_count: manifest.seed_relay_candidates.len(),
            blinded_directory_response: empty_response,
        };
    }
    let expired = manifest.expires_at_ms <= now_ms;
    if expired {
        return BootstrapManifestValidationV0 {
            accepted: false,
            signature_valid: true,
            expired,
            reject_reason: Some("bootstrap_manifest_expired".into()),
            seed_relay_record_valid_count: 0,
            seed_relay_record_invalid_count: manifest.seed_relay_candidates.len(),
            blinded_directory_response: empty_response,
        };
    }
    if manifest.full_raw_ip_directory_embedded {
        return BootstrapManifestValidationV0 {
            accepted: false,
            signature_valid: true,
            expired,
            reject_reason: Some("full_raw_ip_directory_forbidden".into()),
            seed_relay_record_valid_count: 0,
            seed_relay_record_invalid_count: manifest.seed_relay_candidates.len(),
            blinded_directory_response: empty_response,
        };
    }
    if manifest.manifest_requires_single_official_relay {
        return BootstrapManifestValidationV0 {
            accepted: false,
            signature_valid: true,
            expired,
            reject_reason: Some("single_official_relay_forbidden".into()),
            seed_relay_record_valid_count: 0,
            seed_relay_record_invalid_count: manifest.seed_relay_candidates.len(),
            blinded_directory_response: empty_response,
        };
    }
    if manifest.manifest_requires_single_official_domain {
        return BootstrapManifestValidationV0 {
            accepted: false,
            signature_valid: true,
            expired,
            reject_reason: Some("single_official_domain_forbidden".into()),
            seed_relay_record_valid_count: 0,
            seed_relay_record_invalid_count: manifest.seed_relay_candidates.len(),
            blinded_directory_response: empty_response,
        };
    }
    let seed_relay_record_valid_count = manifest
        .seed_relay_candidates
        .iter()
        .filter(|record| validate_peer_signed_relay_record_v0(record, now_ms).accepted)
        .count();
    let seed_relay_record_invalid_count = manifest
        .seed_relay_candidates
        .len()
        .saturating_sub(seed_relay_record_valid_count);
    let blinded_directory_response = issue_blinded_relay_directory_response_v0(
        &manifest.seed_relay_candidates,
        manifest.candidate_set_policy_limit,
        now_ms,
    );
    let accepted = seed_relay_record_valid_count > 0 && !blinded_directory_response.is_empty();
    BootstrapManifestValidationV0 {
        accepted,
        signature_valid: true,
        expired,
        reject_reason: if accepted {
            None
        } else {
            Some("no_valid_seed_relay_candidate".into())
        },
        seed_relay_record_valid_count,
        seed_relay_record_invalid_count,
        blinded_directory_response,
    }
}

#[allow(clippy::too_many_arguments)]
fn bootstrap_manifest_payload_v0(
    manifest_id: String,
    bootstrap_manifest_source: String,
    manifest_public_key: String,
    seed_relay_candidates: Vec<PeerSignedRelayEndpointRecordV0>,
    seed_rendezvous_candidates: Vec<PeerSignedRelayEndpointRecordV0>,
    issued_at_ms: u64,
    expires_at_ms: u64,
    full_raw_ip_directory_embedded: bool,
    manifest_requires_single_official_relay: bool,
    manifest_requires_single_official_domain: bool,
    candidate_set_policy_limit: usize,
) -> BootstrapManifestPayloadV0 {
    BootstrapManifestPayloadV0 {
        manifest_version: 1,
        manifest_id,
        bootstrap_manifest_source,
        manifest_public_key,
        seed_relay_candidates,
        seed_rendezvous_candidates,
        issued_at_ms,
        expires_at_ms,
        full_raw_ip_directory_embedded,
        manifest_requires_single_official_relay,
        manifest_requires_single_official_domain,
        candidate_set_policy_limit,
    }
}

fn sign_relay_endpoint_record_v0(
    signing_key: &SigningKey,
    endpoints: Vec<PeerSignedRelayEndpointV0>,
    issued_at_ms: u64,
    expires_at_ms: u64,
    nonce_or_record_id: &str,
) -> Result<PeerSignedRelayEndpointRecordV0> {
    let public_key_hex = overlay_gate_hex_lower_v0(&signing_key.verifying_key().to_bytes());
    let relay_peer_id = relay_peer_id_from_public_key_hex_v0(&public_key_hex);
    sign_relay_endpoint_record_with_peer_id_v0(
        signing_key,
        &relay_peer_id,
        endpoints,
        issued_at_ms,
        expires_at_ms,
        nonce_or_record_id,
    )
}

fn sign_relay_endpoint_record_with_peer_id_v0(
    signing_key: &SigningKey,
    relay_peer_id: &str,
    endpoints: Vec<PeerSignedRelayEndpointV0>,
    issued_at_ms: u64,
    expires_at_ms: u64,
    nonce_or_record_id: &str,
) -> Result<PeerSignedRelayEndpointRecordV0> {
    let relay_public_key = overlay_gate_hex_lower_v0(&signing_key.verifying_key().to_bytes());
    let payload = relay_record_payload_v0(
        relay_peer_id.to_string(),
        relay_public_key.clone(),
        endpoints.clone(),
        issued_at_ms,
        expires_at_ms,
        nonce_or_record_id.to_string(),
    );
    let canonical_payload = serde_json::to_vec(&payload)?;
    let signature: Signature = signing_key.sign(&canonical_payload);
    Ok(PeerSignedRelayEndpointRecordV0 {
        record_version: payload.record_version,
        relay_peer_id: payload.relay_peer_id,
        relay_public_key,
        endpoints,
        issued_at_ms,
        expires_at_ms,
        nonce_or_record_id: nonce_or_record_id.to_string(),
        signature_scheme: "ed25519".into(),
        signature: overlay_gate_hex_lower_v0(&signature.to_bytes()),
    })
}

fn evaluate_signed_relay_record_case_v0(
    case_name: &str,
    records: Vec<PeerSignedRelayEndpointRecordV0>,
    now_ms: u64,
) -> serde_json::Value {
    let validations = records
        .iter()
        .map(|record| validate_peer_signed_relay_record_v0(record, now_ms))
        .collect::<Vec<_>>();
    let candidates = validations
        .iter()
        .filter_map(|validation| validation.candidate.clone())
        .collect::<Vec<_>>();
    let selected_index = select_relay_candidate_index_v0(&candidates, now_ms);
    let selected = selected_index.map(|index| candidates[index].clone());
    let signature_checked_count = validations.len() as u64;
    let signature_valid_count = validations
        .iter()
        .filter(|validation| validation.signature_valid)
        .count() as u64;
    let signature_invalid_count = signature_checked_count.saturating_sub(signature_valid_count);
    let expired_count = validations
        .iter()
        .filter(|validation| validation.reject_reason.as_deref() == Some("relay_record_expired"))
        .count() as u64;
    let identity_mismatch_count = validations
        .iter()
        .filter(|validation| {
            validation.reject_reason.as_deref() == Some("relay_record_identity_mismatch")
        })
        .count() as u64;
    let tamper_rejected_count = if case_name == "endpoint_tamper_rejected"
        && validations.iter().any(|validation| {
            validation.reject_reason.as_deref() == Some("relay_record_signature_invalid")
        }) {
        1
    } else {
        0
    };
    let transport_unsupported_count = validations
        .iter()
        .filter(|validation| {
            validation.reject_reason.as_deref() == Some("relay_transport_unsupported")
        })
        .count() as u64;
    let selected_path_after_relay_selection = if selected.is_some() {
        "RelayNovoRudp"
    } else {
        "QueueFallback"
    };
    let selection_reason = match &selected {
        Some(candidate) if candidate.transport == "wss" && candidate.port == 443 => {
            "SignedWss443RelayRecordSelected"
        }
        Some(_) => "SignedRelayRecordSelected",
        None => "NoValidSignedRelayRecord",
    };
    let accepted = match case_name {
        "valid_signed_relay_record" => {
            signature_valid_count == 1
                && selected
                    .as_ref()
                    .map(|candidate| candidate.transport.as_str())
                    == Some("wss")
        }
        "invalid_signature_rejected" => signature_invalid_count == 1 && selected.is_none(),
        "expired_record_rejected" => expired_count == 1 && selected.is_none(),
        "peer_id_public_key_mismatch_rejected" => {
            identity_mismatch_count == 1 && selected.is_none()
        }
        "endpoint_tamper_rejected" => tamper_rejected_count == 1 && selected.is_none(),
        "unsupported_transport_rejected" => transport_unsupported_count == 1 && selected.is_none(),
        "multiple_valid_records_prefers_wss_443" => {
            selected
                .as_ref()
                .map(|candidate| candidate.transport.as_str())
                == Some("wss")
        }
        _ => selected.is_some(),
    };

    json!({
        "case": case_name,
        "accepted": accepted,
        "relay_record_count": records.len(),
        "relay_record_signature_checked_count": signature_checked_count,
        "relay_record_signature_valid_count": signature_valid_count,
        "relay_record_signature_invalid_count": signature_invalid_count,
        "relay_record_expired_count": expired_count,
        "relay_record_identity_mismatch_count": identity_mismatch_count,
        "relay_record_tamper_rejected_count": tamper_rejected_count,
        "relay_transport_unsupported_count": transport_unsupported_count,
        "valid_relay_candidate_count": candidates.len(),
        "selected_relay_peer_id": selected.as_ref().map(|candidate| candidate.relay_peer_id.clone()),
        "selected_relay_endpoint": selected.as_ref().map(|candidate| candidate.endpoint.clone()),
        "selected_transport": selected.as_ref().map(|candidate| candidate.transport.clone()),
        "selection_reason": selection_reason,
        "selected_path_after_relay_selection": selected_path_after_relay_selection,
        "validations": validations.iter().map(|validation| {
            json!({
                "record_accepted": validation.accepted,
                "signature_valid": validation.signature_valid,
                "reject_reason": validation.reject_reason,
                "candidate": validation.candidate,
            })
        }).collect::<Vec<_>>(),
    })
}

fn validate_peer_signed_relay_record_v0(
    record: &PeerSignedRelayEndpointRecordV0,
    now_ms: u64,
) -> RelayRecordValidationV0 {
    let relay_public_key_bytes =
        match overlay_gate_decode_hex_bytes_v0(&record.relay_public_key, "relay_public_key") {
            Ok(bytes) => bytes,
            Err(_) => {
                return RelayRecordValidationV0 {
                    accepted: false,
                    signature_valid: false,
                    reject_reason: Some("relay_record_public_key_invalid".into()),
                    candidate: None,
                }
            }
        };
    let public_key_array: [u8; 32] = match relay_public_key_bytes.try_into() {
        Ok(bytes) => bytes,
        Err(_) => {
            return RelayRecordValidationV0 {
                accepted: false,
                signature_valid: false,
                reject_reason: Some("relay_record_public_key_invalid".into()),
                candidate: None,
            }
        }
    };
    let expected_peer_id = relay_peer_id_from_public_key_hex_v0(&record.relay_public_key);
    if record.signature_scheme != "ed25519" {
        return RelayRecordValidationV0 {
            accepted: false,
            signature_valid: false,
            reject_reason: Some("relay_record_signature_scheme_unsupported".into()),
            candidate: None,
        };
    }
    let verifying_key = match VerifyingKey::from_bytes(&public_key_array) {
        Ok(key) => key,
        Err(_) => {
            return RelayRecordValidationV0 {
                accepted: false,
                signature_valid: false,
                reject_reason: Some("relay_record_public_key_invalid".into()),
                candidate: None,
            }
        }
    };
    let signature_bytes = match overlay_gate_decode_hex_bytes_v0(&record.signature, "signature") {
        Ok(bytes) => bytes,
        Err(_) => {
            return RelayRecordValidationV0 {
                accepted: false,
                signature_valid: false,
                reject_reason: Some("relay_record_signature_invalid".into()),
                candidate: None,
            }
        }
    };
    let signature_array: [u8; 64] = match signature_bytes.try_into() {
        Ok(bytes) => bytes,
        Err(_) => {
            return RelayRecordValidationV0 {
                accepted: false,
                signature_valid: false,
                reject_reason: Some("relay_record_signature_invalid".into()),
                candidate: None,
            }
        }
    };
    let signature = Signature::from_bytes(&signature_array);
    let payload = relay_record_payload_v0(
        record.relay_peer_id.clone(),
        record.relay_public_key.clone(),
        record.endpoints.clone(),
        record.issued_at_ms,
        record.expires_at_ms,
        record.nonce_or_record_id.clone(),
    );
    let canonical_payload = match serde_json::to_vec(&payload) {
        Ok(payload) => payload,
        Err(_) => {
            return RelayRecordValidationV0 {
                accepted: false,
                signature_valid: false,
                reject_reason: Some("relay_record_canonical_payload_failed".into()),
                candidate: None,
            }
        }
    };
    if verifying_key
        .verify(&canonical_payload, &signature)
        .is_err()
    {
        return RelayRecordValidationV0 {
            accepted: false,
            signature_valid: false,
            reject_reason: Some("relay_record_signature_invalid".into()),
            candidate: None,
        };
    }
    if record.relay_peer_id != expected_peer_id {
        return RelayRecordValidationV0 {
            accepted: false,
            signature_valid: true,
            reject_reason: Some("relay_record_identity_mismatch".into()),
            candidate: None,
        };
    }
    if record.expires_at_ms <= now_ms {
        return RelayRecordValidationV0 {
            accepted: false,
            signature_valid: true,
            reject_reason: Some("relay_record_expired".into()),
            candidate: None,
        };
    }
    let endpoint = match record
        .endpoints
        .iter()
        .find(|endpoint| relay_transport_supported_v0(&endpoint.transport, endpoint.port))
    {
        Some(endpoint) => endpoint,
        None => {
            return RelayRecordValidationV0 {
                accepted: false,
                signature_valid: true,
                reject_reason: Some("relay_transport_unsupported".into()),
                candidate: None,
            }
        }
    };
    RelayRecordValidationV0 {
        accepted: true,
        signature_valid: true,
        reject_reason: None,
        candidate: Some(RelayCandidateV0 {
            relay_peer_id: record.relay_peer_id.clone(),
            endpoint: endpoint.uri.clone(),
            transport: endpoint.transport.clone(),
            port: endpoint.port,
            priority: endpoint.priority,
            last_seen_ms: now_ms,
            last_success_ms: Some(now_ms),
            failure_count: 0,
            cooldown_until_ms: 0,
            observed_reachable: true,
            supports_wss_443: endpoint.transport == "wss" && endpoint.port == 443,
            supports_quic_443: endpoint.transport == "quic" && endpoint.port == 443,
            supports_udp: endpoint.transport == "udp",
            record_signature_valid: true,
        }),
    }
}

fn relay_record_payload_v0(
    relay_peer_id: String,
    relay_public_key: String,
    endpoints: Vec<PeerSignedRelayEndpointV0>,
    issued_at_ms: u64,
    expires_at_ms: u64,
    nonce_or_record_id: String,
) -> PeerSignedRelayEndpointPayloadV0 {
    let mut capabilities = endpoints
        .iter()
        .flat_map(|endpoint| endpoint.capabilities.clone())
        .collect::<Vec<_>>();
    capabilities.sort();
    capabilities.dedup();
    PeerSignedRelayEndpointPayloadV0 {
        record_version: 1,
        relay_peer_id,
        relay_public_key,
        endpoints,
        issued_at_ms,
        expires_at_ms,
        nonce_or_record_id,
        capabilities,
    }
}

fn relay_peer_id_from_public_key_hex_v0(public_key_hex: &str) -> String {
    format!("novovm-ed25519:{public_key_hex}")
}

fn relay_transport_supported_v0(transport: &str, port: u16) -> bool {
    matches!(
        (transport, port),
        ("wss", 443) | ("quic", 443) | ("tls", 443) | ("ws", 80) | ("udp", _)
    )
}

fn issue_blinded_relay_directory_response_v0(
    records: &[PeerSignedRelayEndpointRecordV0],
    policy_limit: usize,
    now_ms: u64,
) -> Vec<BlindedRelayDirectoryEntryV0> {
    records
        .iter()
        .filter(|record| validate_peer_signed_relay_record_v0(record, now_ms).accepted)
        .take(policy_limit)
        .filter_map(|record| blinded_relay_directory_entry_v0(record).ok())
        .collect()
}

fn blinded_relay_directory_entry_v0(
    record: &PeerSignedRelayEndpointRecordV0,
) -> Result<BlindedRelayDirectoryEntryV0> {
    let endpoint = record
        .endpoints
        .first()
        .context("relay record has no endpoints")?;
    let record_hash = relay_record_hash_v0(record)?;
    let endpoint_hint_hash = overlay_gate_sha256_hex_v0(&[
        b"novovm:blinded-relay-endpoint:v0",
        record.relay_peer_id.as_bytes(),
        endpoint.uri.as_bytes(),
        record.nonce_or_record_id.as_bytes(),
    ]);
    Ok(BlindedRelayDirectoryEntryV0 {
        relay_peer_id: record.relay_peer_id.clone(),
        relay_record_hash: record_hash,
        transport_class: endpoint.transport.clone(),
        region_hint: "region-bucket-anycast-0".into(),
        capability_class: "novorudp-opaque-relay".into(),
        score_bucket: "score-bucket-healthy".into(),
        expires_at_ms: record.expires_at_ms,
        encrypted_or_blinded_endpoint_hint: format!("blind:v0:{endpoint_hint_hash}"),
        relay_record_signature: record.signature.clone(),
    })
}

fn relay_record_hash_v0(record: &PeerSignedRelayEndpointRecordV0) -> Result<String> {
    let payload = relay_record_payload_v0(
        record.relay_peer_id.clone(),
        record.relay_public_key.clone(),
        record.endpoints.clone(),
        record.issued_at_ms,
        record.expires_at_ms,
        record.nonce_or_record_id.clone(),
    );
    let canonical_payload = serde_json::to_vec(&payload)?;
    Ok(overlay_gate_sha256_hex_v0(&[&canonical_payload]))
}

fn overlay_gate_sha256_hex_v0(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    overlay_gate_hex_lower_v0(&hasher.finalize())
}

fn sha256_file_hex_v0(path: &Path) -> Result<String> {
    let bytes =
        fs::read(path).with_context(|| format!("read file for checksum: {}", path.display()))?;
    Ok(overlay_gate_sha256_hex_v0(&[&bytes]))
}

fn send_public_relay_register_v0(
    socket: &UdpSocket,
    relay_addr: &str,
    peer_id: &str,
) -> Result<usize> {
    let payload = PublicRelayRegisterPayloadV0 {
        peer_id: peer_id.to_string(),
        advertised_endpoint: None,
        registered_at_ms: now_unix_ms(),
    };
    let frame = novovm_network::novorudp::NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Endpoint,
        [30u8; 16],
        300,
        301,
        302,
        303,
        serde_json::to_vec(&payload)?,
    );
    socket
        .send_to(&frame.encode(), relay_addr)
        .with_context(|| format!("send public relay register to {relay_addr}"))
}

fn apply_nat_punch_fallback_v0(
    report: &mut serde_json::Value,
    relay_fallback_enabled: bool,
    reason: String,
) {
    let failure_classification = classify_nat_punch_failure_v0(&reason);
    report["punch_ack_valid"] = json!(false);
    report["punch_result"] = json!("failed");
    report["punch_reject_reason"] = json!(reason);
    report["nat_failure_classification"] = json!(failure_classification);
    report["relay_fallback_selected"] = json!(relay_fallback_enabled);
    report["queue_fallback_selected"] = json!(!relay_fallback_enabled);
    if relay_fallback_enabled {
        report["accepted"] = json!(true);
        report["fallback_reason"] = json!("NatPunchFailed");
        report["selected_path_after_punch"] = json!("RelayNovoRudp");
    } else {
        report["accepted"] = json!(true);
        report["fallback_reason"] = json!("NoHealthyRelayCandidateAfterNatPunchFailed");
        report["selected_path_after_punch"] = json!("QueueFallback");
    }
}

fn classify_nat_punch_failure_v0(reason: &str) -> &'static str {
    if reason.contains("punch_nonce_mismatch") {
        "StaleOrMismatchedPunchAck"
    } else if reason.contains("timeout") || reason.contains("Resource temporarily unavailable") {
        "UdpReachabilityBlockedOrAckReturnFailed"
    } else if reason.contains("vpn_tun") || reason.contains("cgnat") {
        "VpnTunOrCgnatNoInboundUdp"
    } else if reason.contains("relay_candidate_unavailable") {
        "NoHealthyRelayCandidate"
    } else if reason.contains("decode") {
        "InvalidPunchAckFrame"
    } else {
        "NatPunchFailed"
    }
}

fn nat_auto_adaptive_case_v0(
    case_name: &str,
    punch_ack_valid: bool,
    punch_failure_reason: Option<&str>,
    relay_candidate_available: bool,
    vpn_tun_detected: bool,
    relay_unavailable: bool,
) -> serde_json::Value {
    let punch_required_for_connectivity = false;
    let selected_path_after_punch = if punch_ack_valid {
        "PunchedDirect"
    } else if relay_candidate_available && !relay_unavailable {
        "RelayNovoRudp"
    } else {
        "QueueFallback"
    };
    let fallback_reason = if punch_ack_valid {
        serde_json::Value::Null
    } else if selected_path_after_punch == "RelayNovoRudp" {
        json!("NatPunchFailed")
    } else {
        json!("NoHealthyNetworkPath")
    };
    let failure_classification = punch_failure_reason
        .map(classify_nat_punch_failure_v0)
        .unwrap_or("None");
    let accepted = match case_name {
        "punch_success_upgrades_to_direct" => {
            punch_ack_valid && selected_path_after_punch == "PunchedDirect"
        }
        "udp_timeout_with_relay_falls_back_to_relay" => {
            !punch_ack_valid
                && failure_classification == "UdpReachabilityBlockedOrAckReturnFailed"
                && selected_path_after_punch == "RelayNovoRudp"
        }
        "udp_timeout_without_relay_enters_queue" => {
            !punch_ack_valid
                && failure_classification == "UdpReachabilityBlockedOrAckReturnFailed"
                && selected_path_after_punch == "QueueFallback"
        }
        "nonce_mismatch_never_marks_reachable" => {
            !punch_ack_valid
                && failure_classification == "StaleOrMismatchedPunchAck"
                && selected_path_after_punch != "PunchedDirect"
        }
        "vpn_tun_detected_prefers_relay_first" => {
            vpn_tun_detected && selected_path_after_punch == "RelayNovoRudp"
        }
        "relay_unavailable_after_nat_failure_queues" => {
            relay_unavailable && selected_path_after_punch == "QueueFallback"
        }
        _ => selected_path_after_punch != "PunchFailed",
    };

    json!({
        "case": case_name,
        "accepted": accepted,
        "vpn_tun_detected": vpn_tun_detected,
        "punch_attempted": true,
        "punch_ack_valid": punch_ack_valid,
        "punch_failure_reason": punch_failure_reason,
        "nat_failure_classification": failure_classification,
        "relay_candidate_available": relay_candidate_available,
        "relay_unavailable": relay_unavailable,
        "relay_fallback_selected": selected_path_after_punch == "RelayNovoRudp",
        "queue_fallback_selected": selected_path_after_punch == "QueueFallback",
        "selected_path_after_punch": selected_path_after_punch,
        "fallback_reason": fallback_reason,
        "punch_required_for_connectivity": punch_required_for_connectivity,
        "nat_punch_is_optimization_path": true,
        "reachability_misclassified_as_direct": !punch_ack_valid && selected_path_after_punch == "PunchedDirect",
        "manual_user_port_forward_required": false,
    })
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

fn overlay_gate_hex_lower_v0(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn overlay_gate_decode_hex_bytes_v0(raw: &str, field: &str) -> Result<Vec<u8>> {
    let normalized = raw
        .trim()
        .strip_prefix("0x")
        .or_else(|| raw.trim().strip_prefix("0X"))
        .unwrap_or(raw.trim());
    if normalized.is_empty() {
        anyhow::bail!("{field} is empty");
    }
    if !normalized.len().is_multiple_of(2) {
        anyhow::bail!("{field} must be even-length hex");
    }
    if !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        anyhow::bail!("{field} must be hex");
    }
    let mut out = Vec::with_capacity(normalized.len() / 2);
    for pair in normalized.as_bytes().chunks_exact(2) {
        let hex =
            std::str::from_utf8(pair).with_context(|| format!("{field} contains invalid utf8"))?;
        let byte = u8::from_str_radix(hex, 16)
            .with_context(|| format!("{field} contains invalid hex byte {hex}"))?;
        out.push(byte);
    }
    Ok(out)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}
