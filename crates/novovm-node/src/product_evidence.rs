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
        _ => bail!("unsupported product evidence report scope: {scope}"),
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
        let manifest =
            build_product_evidence_manifest_v1(&root, &[report_path.clone()], &signer, 1_000)
                .unwrap();
        let manifest_path = root.join("evidence.json");
        write_product_evidence_manifest_v1(&manifest_path, &manifest).unwrap();
        assert!(verify_product_evidence_manifest_v1(&root, &manifest_path).accepted);
        fs::write(&report_path, b"{}".as_slice()).unwrap();
        assert!(!verify_product_evidence_manifest_v1(&root, &manifest_path).accepted);
        let _ = fs::remove_dir_all(root);
    }
}
