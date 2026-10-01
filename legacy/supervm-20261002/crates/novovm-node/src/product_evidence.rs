//! Signed, content-validated evidence manifests for product overlay runs.

use anyhow::{bail, Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use novovm_network::{
    peer_id_from_ed25519_public_key_v1, validate_strategy_receipt_v1, SignedStrategyReceiptV1,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::product_relay_daemon::{ProductRelayDaemonReportV1, PRODUCT_RELAY_DAEMON_VERSION_V2};

pub const PRODUCT_EVIDENCE_VERSION_V1: u16 = 1;
const PRODUCT_EVIDENCE_DOMAIN_V1: &[u8] = b"novovm-product-overlay-evidence-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductEvidenceArtifactV1 {
    pub relative_path: PathBuf,
    pub sha256_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductEvidenceManifestV1 {
    pub version: u16,
    pub scope: String,
    pub created_at_ms: u64,
    pub signer_peer_id: String,
    pub signer_public_key: [u8; 32],
    pub artifacts: Vec<ProductEvidenceArtifactV1>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductEvidenceVerificationV1 {
    pub accepted: bool,
    pub scope: String,
    pub manifest_signature_valid: bool,
    pub artifact_checksum_valid_count: usize,
    pub report_validation_count: usize,
    pub real_public_topology_proven: bool,
    pub real_cross_nat_proven: bool,
    pub reason: Option<String>,
}

pub fn build_product_evidence_manifest_v1(
    root: &Path,
    report_paths: &[PathBuf],
    signing_key: &SigningKey,
    created_at_ms: u64,
) -> Result<ProductEvidenceManifestV1> {
    let root = canonical_root_v1(root)?;
    if report_paths.is_empty() {
        bail!("evidence manifest requires at least one validated report");
    }
    let mut artifacts = Vec::new();
    for path in report_paths {
        let absolute = canonical_artifact_path_v1(&root, path)?;
        validate_product_report_v1(&absolute)?;
        let relative_path = absolute
            .strip_prefix(&root)
            .context("evidence artifact escaped root")?
            .to_path_buf();
        artifacts.push(ProductEvidenceArtifactV1 {
            relative_path,
            sha256_hex: sha256_file_hex_v1(&absolute)?,
        });
    }
    artifacts.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    artifacts.dedup_by(|left, right| left.relative_path == right.relative_path);
    let signer_public_key = signing_key.verifying_key().to_bytes();
    let mut manifest = ProductEvidenceManifestV1 {
        version: PRODUCT_EVIDENCE_VERSION_V1,
        scope: "novovm_product_overlay_evidence_v1".into(),
        created_at_ms,
        signer_peer_id: peer_id_from_ed25519_public_key_v1(&signer_public_key),
        signer_public_key,
        artifacts,
        signature: Vec::new(),
    };
    manifest.signature = signing_key
        .sign(&manifest_signing_bytes_v1(&manifest))
        .to_bytes()
        .to_vec();
    Ok(manifest)
}

pub fn write_product_evidence_manifest_v1(
    path: &Path,
    manifest: &ProductEvidenceManifestV1,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create evidence directory: {}", parent.display()))?;
    }
    fs::write(path, serde_json::to_vec_pretty(manifest)?)
        .with_context(|| format!("write evidence manifest: {}", path.display()))
}

pub fn verify_product_evidence_manifest_v1(
    root: &Path,
    manifest_path: &Path,
) -> ProductEvidenceVerificationV1 {
    match verify_product_evidence_manifest_inner_v1(root, manifest_path) {
        Ok((checksums, reports)) => ProductEvidenceVerificationV1 {
            accepted: true,
            scope: "novovm_product_overlay_evidence_v1".into(),
            manifest_signature_valid: true,
            artifact_checksum_valid_count: checksums,
            report_validation_count: reports,
            // Artifact integrity is not an external topology result.
            real_public_topology_proven: false,
            real_cross_nat_proven: false,
            reason: Some(
                "integrity_and_report_schema_verified; external topology evidence not included"
                    .into(),
            ),
        },
        Err(error) => ProductEvidenceVerificationV1 {
            accepted: false,
            scope: "novovm_product_overlay_evidence_v1".into(),
            manifest_signature_valid: false,
            artifact_checksum_valid_count: 0,
            report_validation_count: 0,
            real_public_topology_proven: false,
            real_cross_nat_proven: false,
            reason: Some(error.to_string()),
        },
    }
}

fn verify_product_evidence_manifest_inner_v1(
    root: &Path,
    manifest_path: &Path,
) -> Result<(usize, usize)> {
    let root = canonical_root_v1(root)?;
    let bytes = fs::read(manifest_path)
        .with_context(|| format!("read evidence manifest: {}", manifest_path.display()))?;
    let manifest: ProductEvidenceManifestV1 =
        serde_json::from_slice(&bytes).context("decode evidence manifest")?;
    if manifest.version != PRODUCT_EVIDENCE_VERSION_V1
        || manifest.scope != "novovm_product_overlay_evidence_v1"
    {
        bail!("unsupported product evidence manifest");
    }
    if manifest.signer_peer_id != peer_id_from_ed25519_public_key_v1(&manifest.signer_public_key) {
        bail!("evidence signer peer id does not match public key");
    }
    let verifying_key = VerifyingKey::from_bytes(&manifest.signer_public_key)
        .context("decode evidence signer key")?;
    let signature =
        Signature::from_slice(&manifest.signature).context("decode evidence signature")?;
    verifying_key
        .verify(&manifest_signing_bytes_v1(&manifest), &signature)
        .context("evidence signature invalid")?;
    if manifest.artifacts.is_empty() {
        bail!("evidence manifest has no artifacts");
    }
    let mut report_count = 0usize;
    for artifact in &manifest.artifacts {
        let path = canonical_artifact_path_v1(&root, &artifact.relative_path)?;
        if sha256_file_hex_v1(&path)? != artifact.sha256_hex {
            bail!(
                "evidence checksum mismatch: {}",
                artifact.relative_path.display()
            );
        }
        validate_product_report_v1(&path)?;
        report_count = report_count.saturating_add(1);
    }
    Ok((manifest.artifacts.len(), report_count))
}

fn validate_product_report_v1(path: &Path) -> Result<()> {
    let bytes =
        fs::read(path).with_context(|| format!("read product report: {}", path.display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("decode product report: {}", path.display()))?;
    let scope = value
        .get("scope")
        .and_then(Value::as_str)
        .context("report missing scope")?;
    if value.get("accepted").and_then(Value::as_bool) != Some(true) {
        bail!("report is not accepted: {}", path.display());
    }
    for field in ["payload_treated_opaque", "novorudp_wire_changed"] {
        let expected = field == "payload_treated_opaque";
        if value.get(field).and_then(Value::as_bool) != Some(expected) {
            bail!("report boundary field invalid: {field}");
        }
    }
    match scope {
        "novovm_product_relay_daemon_v1" => {
            if value
                .get("relay_is_trusted_authority")
                .and_then(Value::as_bool)
                != Some(false)
                || value
                    .get("node_identity_challenge_response_required")
                    .and_then(Value::as_bool)
                    != Some(true)
            {
                bail!("relay daemon trust boundary invalid");
            }
            let report: ProductRelayDaemonReportV1 = serde_json::from_value(value.clone())
                .context("decode bounded relay daemon report")?;
            validate_relay_daemon_report_v2(&report)?;
        }
        "novovm_product_nat_runtime_v1" => {
            if value.get("network_only").and_then(Value::as_bool) != Some(true) {
                bail!("NAT report is not network-only");
            }
            if let Some(attempt) = value.get("punch_attempt") {
                let path = attempt
                    .get("selected_path_after_punch")
                    .and_then(Value::as_str);
                let ack_valid = attempt.get("ack_valid").and_then(Value::as_bool);
                if path == Some("punched_direct") && ack_valid != Some(true) {
                    bail!("NAT direct path lacks a valid signed ACK");
                }
            }
        }
        "novovm_product_node_overlay_runtime_v1" => {
            let receipt: SignedStrategyReceiptV1 = serde_json::from_value(
                value
                    .get("route_plan")
                    .context("node overlay report missing route_plan")?
                    .get("strategy_receipt")
                    .context("node overlay report missing strategy receipt")?
                    .clone(),
            )
            .context("decode node strategy receipt")?;
            validate_strategy_receipt_v1(&receipt).context("node strategy receipt invalid")?;
            if value
                .get("centralized_control_plane_required")
                .and_then(Value::as_bool)
                != Some(false)
            {
                bail!("node overlay report requires centralized control plane");
            }
        }
        "novovm_product_peer_runtime_v1" => {
            let role = value
                .get("role")
                .and_then(Value::as_str)
                .context("peer report missing role")?;
            if !matches!(role, "sender" | "receiver") {
                bail!("peer report has unsupported role: {role}");
            }
            for field in ["local_peer_id", "remote_peer_id", "relay_peer_id"] {
                if value
                    .get(field)
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty)
                {
                    bail!("peer report missing {field}");
                }
            }
            if value.get("selected_path").and_then(Value::as_str) != Some("RelayNovoRudp")
                || value.get("selected_transport").and_then(Value::as_str) != Some("wss")
                || value
                    .get("peer_handshake_via_relay")
                    .and_then(Value::as_bool)
                    != Some(true)
                || value
                    .get("e2e_session_established")
                    .and_then(Value::as_bool)
                    != Some(true)
                || value
                    .get("novorudp_inner_frame_preserved")
                    .and_then(Value::as_bool)
                    != Some(true)
                || value
                    .get("relay_is_trusted_authority")
                    .and_then(Value::as_bool)
                    != Some(false)
                || value.get("network_only").and_then(Value::as_bool) != Some(true)
            {
                bail!("peer report relay or end-to-end boundary invalid");
            }
            for field in ["apfl_interpreted", "aoem_called", "ledger_semantics"] {
                if value.get(field).and_then(Value::as_bool) != Some(false) {
                    bail!("peer report non-network boundary invalid: {field}");
                }
            }
            let sent = value
                .get("sent_frame_count")
                .and_then(Value::as_u64)
                .context("peer report missing sent_frame_count")?;
            let received = value
                .get("received_frame_count")
                .and_then(Value::as_u64)
                .context("peer report missing received_frame_count")?;
            match role {
                "sender" if sent > 0 && received == 0 => {}
                "receiver" if sent == 0 && received > 0 => {}
                _ => bail!("peer report frame counters do not match role"),
            }
        }
        "novovm_product_mainline_topology_plan_v1" => {
            if value.get("full_mesh_symmetric").and_then(Value::as_bool) != Some(true)
                || value
                    .get("external_network_executed")
                    .and_then(Value::as_bool)
                    != Some(false)
                || value
                    .get("real_public_topology_proven")
                    .and_then(Value::as_bool)
                    != Some(false)
                || value.get("real_cross_nat_proven").and_then(Value::as_bool) != Some(false)
            {
                bail!("mainline topology preflight boundary invalid");
            }
            let node_count = value
                .get("node_count")
                .and_then(Value::as_u64)
                .context("topology preflight missing node_count")?;
            let directed_edges = value
                .get("directed_peer_edge_count")
                .and_then(Value::as_u64)
                .context("topology preflight missing directed_peer_edge_count")?;
            if !(2..=64).contains(&node_count)
                || directed_edges != node_count.saturating_mul(node_count.saturating_sub(1))
            {
                bail!("mainline topology preflight is not a complete mesh");
            }
        }
        _ => bail!("unsupported product evidence report scope: {scope}"),
    }
    Ok(())
}

fn validate_relay_daemon_report_v2(report: &ProductRelayDaemonReportV1) -> Result<()> {
    if report.daemon_version != PRODUCT_RELAY_DAEMON_VERSION_V2 {
        bail!("relay daemon evidence requires daemon_version={PRODUCT_RELAY_DAEMON_VERSION_V2}");
    }
    let runtime = &report.relay_runtime;
    let limits = &runtime.limits;
    if report.transport != "wss"
        || report.websocket_path != "/novovm"
        || !report.tls_transport_enabled
        || report.ca_trust_required_for_novovm_identity
        || report.business_semantics_interpreted_by_relay
        || report.listen_addr.is_empty()
        || report.report_updated_at_ms == 0
    {
        bail!("relay daemon transport or semantic boundary is invalid");
    }
    if report.max_connection_count == 0
        || report.max_connection_count <= limits.max_sessions
        || report.active_connection_count > report.max_connection_count
    {
        bail!("relay daemon physical connection limits are invalid");
    }
    if limits.max_sessions == 0
        || limits.max_tracked_sources < limits.max_sessions
        || limits.session_queue_capacity == 0
        || limits.session_queue_bytes == 0
        || limits.active_queue_total < limits.session_queue_capacity
        || limits.active_queue_bytes_total < limits.session_queue_bytes
        || limits.offline_queue_per_peer == 0
        || limits.offline_queue_bytes_per_peer == 0
        || limits.offline_queue_per_source == 0
        || limits.offline_queue_bytes_per_source == 0
        || limits.offline_queue_total < limits.offline_queue_per_peer
        || limits.offline_queue_total < limits.offline_queue_per_source
        || limits.offline_queue_bytes_total < limits.offline_queue_bytes_per_peer
        || limits.offline_queue_bytes_total < limits.offline_queue_bytes_per_source
        || limits.offline_queue_ttl_ms == 0
        || limits.session_ttl_ms == 0
        || limits.rate_limit_frames == 0
        || limits.max_frames_per_window < limits.rate_limit_frames
        || limits.rate_limit_window_ms == 0
        || limits.source_bytes_per_minute == 0
        || limits.max_bytes_per_minute < limits.source_bytes_per_minute
        || limits.max_wire_message_bytes != novovm_network::PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1
    {
        bail!("relay daemon runtime limit snapshot is invalid");
    }
    if runtime.active_session_count > limits.max_sessions
        || runtime.tracked_source_count > limits.max_tracked_sources
        || runtime.active_session_count != runtime.active_peer_ids.len()
        || runtime.active_session_count != runtime.active_sessions.len()
        || runtime.active_queued_frame_count > limits.active_queue_total
        || runtime.active_queued_bytes > limits.active_queue_bytes_total
        || runtime.offline_queued_frame_count > limits.offline_queue_total
        || runtime.offline_queued_bytes > limits.offline_queue_bytes_total
        || runtime.queued_frame_count
            != runtime
                .active_queued_frame_count
                .saturating_add(runtime.offline_queued_frame_count)
        || runtime.queued_bytes
            != runtime
                .active_queued_bytes
                .saturating_add(runtime.offline_queued_bytes)
        || runtime.active_sessions.iter().any(|session| {
            session.pending_delivery_capacity > limits.session_queue_capacity
                || session.pending_control_capacity > limits.session_queue_capacity
                || session.queued_frame_count > limits.session_queue_capacity
                || session.queued_bytes > limits.session_queue_bytes
        })
        || !runtime.payload_treated_opaque
        || runtime.relay_is_trusted_authority
        || (report.graceful_shutdown && runtime.accepting_new_work)
    {
        bail!("relay daemon runtime snapshot contradicts its bounded limits");
    }
    let mut peer_ids = runtime.active_peer_ids.clone();
    peer_ids.sort();
    peer_ids.dedup();
    let mut session_peer_ids = runtime
        .active_sessions
        .iter()
        .map(|session| session.peer_id.clone())
        .collect::<Vec<_>>();
    session_peer_ids.sort();
    session_peer_ids.dedup();
    if peer_ids.len() != runtime.active_peer_ids.len()
        || session_peer_ids.len() != runtime.active_sessions.len()
        || peer_ids != session_peer_ids
    {
        bail!("relay daemon active session identity snapshot is inconsistent");
    }
    Ok(())
}

fn canonical_root_v1(root: &Path) -> Result<PathBuf> {
    root.canonicalize()
        .with_context(|| format!("canonicalize evidence root: {}", root.display()))
}

fn canonical_artifact_path_v1(root: &Path, path: &Path) -> Result<PathBuf> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let canonical = candidate
        .canonicalize()
        .with_context(|| format!("canonicalize evidence artifact: {}", candidate.display()))?;
    if !canonical.starts_with(root) {
        bail!("evidence artifact escapes root: {}", path.display());
    }
    Ok(canonical)
}

fn sha256_file_hex_v1(path: &Path) -> Result<String> {
    let bytes =
        fs::read(path).with_context(|| format!("read evidence artifact: {}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn manifest_signing_bytes_v1(manifest: &ProductEvidenceManifestV1) -> Vec<u8> {
    let mut bytes = Vec::new();
    append_field_v1(&mut bytes, PRODUCT_EVIDENCE_DOMAIN_V1);
    append_field_v1(&mut bytes, &manifest.version.to_be_bytes());
    append_field_v1(&mut bytes, manifest.scope.as_bytes());
    append_field_v1(&mut bytes, &manifest.created_at_ms.to_be_bytes());
    append_field_v1(&mut bytes, manifest.signer_peer_id.as_bytes());
    append_field_v1(&mut bytes, &manifest.signer_public_key);
    for artifact in &manifest.artifacts {
        append_field_v1(
            &mut bytes,
            artifact.relative_path.to_string_lossy().as_bytes(),
        );
        append_field_v1(&mut bytes, artifact.sha256_hex.as_bytes());
    }
    bytes
}

fn append_field_v1(destination: &mut Vec<u8>, value: &[u8]) {
    destination.extend_from_slice(&(value.len() as u32).to_be_bytes());
    destination.extend_from_slice(value);
}

pub fn load_evidence_signing_key_v1(path: &Path) -> Result<SigningKey> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read evidence signing key: {}", path.display()))?;
    let text = text.trim();
    if text.len() != 64 {
        bail!("evidence signing key must contain exactly 64 hexadecimal characters");
    }
    let mut secret = [0u8; 32];
    for (index, output) in secret.iter_mut().enumerate() {
        *output = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
            .context("decode evidence signing key hex")?;
    }
    Ok(SigningKey::from_bytes(&secret))
}

pub fn now_ms_v1() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounded_relay_report_v2() -> Value {
        serde_json::json!({
            "accepted": true,
            "scope": "novovm_product_relay_daemon_v1",
            "daemon_version": 2,
            "listen_addr": "127.0.0.1:9443",
            "websocket_path": "/novovm",
            "transport": "wss",
            "report_updated_at_ms": 1_000,
            "graceful_shutdown": false,
            "tls_transport_enabled": true,
            "ca_trust_required_for_novovm_identity": false,
            "node_identity_challenge_response_required": true,
            "payload_treated_opaque": true,
            "relay_is_trusted_authority": false,
            "business_semantics_interpreted_by_relay": false,
            "novorudp_wire_changed": false,
            "max_connection_count": 8,
            "active_connection_count": 0,
            "rejected_connection_total": 0,
            "relay_runtime": {
                "accepting_new_work": true,
                "active_session_count": 0,
                "tracked_source_count": 0,
                "active_peer_ids": [],
                "active_sessions": [],
                "queued_frame_count": 0,
                "queued_bytes": 0,
                "active_queued_frame_count": 0,
                "active_queued_bytes": 0,
                "offline_queued_frame_count": 0,
                "offline_queued_bytes": 0,
                "limits": {
                    "max_sessions": 4,
                    "max_tracked_sources": 16,
                    "session_queue_capacity": 8,
                    "session_queue_bytes": 1048576,
                    "active_queue_total": 32,
                    "active_queue_bytes_total": 4194304,
                    "offline_queue_per_peer": 8,
                    "offline_queue_bytes_per_peer": 1048576,
                    "offline_queue_per_source": 16,
                    "offline_queue_bytes_per_source": 2097152,
                    "offline_queue_total": 16,
                    "offline_queue_bytes_total": 2097152,
                    "offline_queue_ttl_ms": 5000,
                    "session_ttl_ms": 5000,
                    "rate_limit_frames": 100,
                    "max_frames_per_window": 1000,
                    "rate_limit_window_ms": 1000,
                    "source_bytes_per_minute": 16777216,
                    "max_bytes_per_minute": 33554432,
                    "max_wire_message_bytes": 1048576
                },
                "registered_session_total": 0,
                "session_limit_rejection_total": 0,
                "shutdown_rejection_total": 0,
                "replaced_session_total": 0,
                "disconnected_session_total": 0,
                "expired_session_total": 0,
                "forwarded_frame_total": 0,
                "queued_frame_total": 0,
                "rate_limited_frame_total": 0,
                "aggregate_rate_limited_frame_total": 0,
                "wire_message_too_large_total": 0,
                "source_byte_limited_frame_total": 0,
                "aggregate_byte_limited_frame_total": 0,
                "admitted_wire_bytes_total": 0,
                "rejected_wire_bytes_total": 0,
                "active_queue_byte_limited_frame_total": 0,
                "active_queue_count_limited_frame_total": 0,
                "offline_peer_limited_frame_total": 0,
                "offline_source_limited_frame_total": 0,
                "offline_total_limited_frame_total": 0,
                "expired_queued_frame_total": 0,
                "expired_queued_bytes_total": 0,
                "offline_full_sweep_total": 0,
                "protocol_rejected_frame_total": 0,
                "rejected_frame_total": 0,
                "payload_treated_opaque": true,
                "relay_is_trusted_authority": false
            }
        })
    }

    #[test]
    fn relay_evidence_requires_self_consistent_bounded_daemon_v2() {
        let root = std::env::temp_dir().join(format!("novovm-relay-evidence-{}", now_ms_v1()));
        fs::create_dir_all(&root).unwrap();
        let report_path = root.join("relay.json");
        let mut report = bounded_relay_report_v2();
        fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
        validate_product_report_v1(&report_path).unwrap();

        report["daemon_version"] = Value::from(1);
        fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
        assert!(validate_product_report_v1(&report_path)
            .unwrap_err()
            .to_string()
            .contains("daemon_version=2"));

        report = bounded_relay_report_v2();
        report["relay_runtime"]["limits"]["max_frames_per_window"] = Value::from(99);
        fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
        assert!(validate_product_report_v1(&report_path)
            .unwrap_err()
            .to_string()
            .contains("limit snapshot"));

        report = bounded_relay_report_v2();
        report["relay_runtime"]["queued_frame_count"] = Value::from(1);
        fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
        assert!(validate_product_report_v1(&report_path)
            .unwrap_err()
            .to_string()
            .contains("contradicts"));

        report = bounded_relay_report_v2();
        report["transport"] = Value::from("ws");
        fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
        assert!(validate_product_report_v1(&report_path)
            .unwrap_err()
            .to_string()
            .contains("transport or semantic"));

        let active_session = serde_json::json!({
            "peer_id": "peer-a",
            "session_id": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
            "authenticated_at_ms": 1,
            "last_seen_ms": 2,
            "pending_delivery_capacity": 8,
            "pending_control_capacity": 8,
            "queued_frame_count": 9,
            "queued_bytes": 0
        });
        report = bounded_relay_report_v2();
        report["relay_runtime"]["active_session_count"] = Value::from(1);
        report["relay_runtime"]["active_peer_ids"] = serde_json::json!(["peer-a"]);
        report["relay_runtime"]["active_sessions"] = serde_json::json!([active_session]);
        fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
        assert!(validate_product_report_v1(&report_path)
            .unwrap_err()
            .to_string()
            .contains("contradicts"));

        let mut session_a = report["relay_runtime"]["active_sessions"][0].clone();
        session_a["queued_frame_count"] = Value::from(0);
        report = bounded_relay_report_v2();
        report["relay_runtime"]["active_session_count"] = Value::from(2);
        report["relay_runtime"]["active_peer_ids"] = serde_json::json!(["peer-a", "peer-b"]);
        report["relay_runtime"]["active_sessions"] =
            serde_json::json!([session_a.clone(), session_a]);
        fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
        assert!(validate_product_report_v1(&report_path)
            .unwrap_err()
            .to_string()
            .contains("identity snapshot"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn signed_evidence_rejects_tampered_validated_report() {
        let root = std::env::temp_dir().join(format!("novovm-evidence-{}", now_ms_v1()));
        fs::create_dir_all(&root).unwrap();
        let report_path = root.join("nat.json");
        fs::write(&report_path, serde_json::to_vec(&serde_json::json!({
            "accepted": true, "scope": "novovm_product_nat_runtime_v1", "network_only": true,
            "payload_treated_opaque": true, "novorudp_wire_changed": false, "punch_attempt": null
        })).unwrap()).unwrap();
        let signer = SigningKey::from_bytes(&[131; 32]);
        let manifest = build_product_evidence_manifest_v1(
            &root,
            std::slice::from_ref(&report_path),
            &signer,
            1_000,
        )
        .unwrap();
        let manifest_path = root.join("evidence.json");
        write_product_evidence_manifest_v1(&manifest_path, &manifest).unwrap();
        assert!(verify_product_evidence_manifest_v1(&root, &manifest_path).accepted);
        fs::write(&report_path, b"{}".as_slice()).unwrap();
        assert!(!verify_product_evidence_manifest_v1(&root, &manifest_path).accepted);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn signed_evidence_accepts_real_peer_runtime_boundary() {
        let root = std::env::temp_dir().join(format!("novovm-peer-evidence-{}", now_ms_v1()));
        fs::create_dir_all(&root).unwrap();
        let report_path = root.join("sender.json");
        fs::write(
            &report_path,
            serde_json::to_vec(&serde_json::json!({
                "accepted": true,
                "scope": "novovm_product_peer_runtime_v1",
                "role": "sender",
                "local_peer_id": "peer-a",
                "remote_peer_id": "peer-b",
                "relay_peer_id": "relay-a",
                "selected_path": "RelayNovoRudp",
                "selected_transport": "wss",
                "peer_handshake_via_relay": true,
                "e2e_session_established": true,
                "sent_frame_count": 1,
                "received_frame_count": 0,
                "novorudp_inner_frame_preserved": true,
                "payload_treated_opaque": true,
                "relay_is_trusted_authority": false,
                "network_only": true,
                "apfl_interpreted": false,
                "aoem_called": false,
                "ledger_semantics": false,
                "novorudp_wire_changed": false
            }))
            .unwrap(),
        )
        .unwrap();
        let signer = SigningKey::from_bytes(&[132; 32]);
        let manifest =
            build_product_evidence_manifest_v1(&root, &[report_path], &signer, 1_000).unwrap();
        let manifest_path = root.join("evidence.json");
        write_product_evidence_manifest_v1(&manifest_path, &manifest).unwrap();
        assert!(verify_product_evidence_manifest_v1(&root, &manifest_path).accepted);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn signed_evidence_accepts_offline_topology_without_external_claims() {
        let root = std::env::temp_dir().join(format!("novovm-topology-evidence-{}", now_ms_v1()));
        fs::create_dir_all(&root).unwrap();
        let report_path = root.join("topology.json");
        fs::write(
            &report_path,
            serde_json::to_vec(&serde_json::json!({
                "accepted": true,
                "scope": "novovm_product_mainline_topology_plan_v1",
                "payload_treated_opaque": true,
                "novorudp_wire_changed": false,
                "node_count": 4,
                "directed_peer_edge_count": 12,
                "full_mesh_symmetric": true,
                "external_network_executed": false,
                "real_public_topology_proven": false,
                "real_cross_nat_proven": false
            }))
            .unwrap(),
        )
        .unwrap();
        let signer = SigningKey::from_bytes(&[133; 32]);
        let manifest =
            build_product_evidence_manifest_v1(&root, &[report_path], &signer, 1_000).unwrap();
        let manifest_path = root.join("evidence.json");
        write_product_evidence_manifest_v1(&manifest_path, &manifest).unwrap();
        assert!(verify_product_evidence_manifest_v1(&root, &manifest_path).accepted);
        let _ = fs::remove_dir_all(root);
    }
}
