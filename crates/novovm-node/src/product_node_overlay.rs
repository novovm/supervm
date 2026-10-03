//! Node-side bootstrap cache, signed relay discovery, rotation, and strategy receipts.
//!
//! This module is intentionally payload-blind. It persists only signed bootstrap manifests and
//! never turns a relay or bootstrap source into a NOVOVM trust authority.

use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    peer_id_from_ed25519_public_key_v1, resolve_bootstrap_sources_v1, sign_strategy_receipt_v1,
    validate_bootstrap_manifest_v1, BootstrapSourceKindV1, BootstrapSourceV1,
    BootstrapTrustPolicyV1, NatPunchAttemptV1, NatSelectedPathV1, RelayCandidatePoolConfigV1,
    RelayCandidatePoolV1, RelayCandidateSnapshotV1, SignedBootstrapManifestV1,
    SignedStrategyReceiptV1, StrategyDecisionV1, StrategyPathV1,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const PRODUCT_NODE_BOOTSTRAP_CACHE_SCHEMA_V1: &str = "novovm-product-node-bootstrap-cache/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductBootstrapSourceV1 {
    pub source_kind: BootstrapSourceKindV1,
    pub priority: u16,
    pub manifest: SignedBootstrapManifestV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductNodeOverlayConfigV1 {
    pub cache_path: PathBuf,
    pub trusted_signer_public_keys: Vec<[u8; 32]>,
    #[serde(default = "default_minimum_bootstrap_signatures_v1")]
    pub minimum_bootstrap_signatures: usize,
    #[serde(default)]
    pub embedded_sources: Vec<ProductBootstrapSourceV1>,
    #[serde(default)]
    pub cooldown_base_ms: Option<u64>,
    #[serde(default)]
    pub cooldown_max_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductBootstrapCacheV1 {
    pub schema: String,
    pub cached_at_ms: u64,
    pub manifests: Vec<SignedBootstrapManifestV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductNodeBootstrapStatusV1 {
    pub cache_loaded: bool,
    pub cache_path: PathBuf,
    pub selected_source: Option<BootstrapSourceKindV1>,
    pub accepted_sources: Vec<BootstrapSourceKindV1>,
    pub rejected_source_count: usize,
    pub valid_relay_candidate_count: usize,
    pub cache_written: bool,
    pub full_raw_ip_directory_exposed: bool,
    pub centralized_control_plane_required: bool,
    pub single_official_relay_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductNodeRoutePlanV1 {
    pub selected_path: StrategyPathV1,
    pub selected_relay: Option<RelayCandidateSnapshotV1>,
    pub fallback_reason: Option<String>,
    pub strategy_receipt: SignedStrategyReceiptV1,
}

pub struct ProductNodeOverlayRuntimeV1 {
    cache_path: PathBuf,
    pool: RelayCandidatePoolV1,
    status: ProductNodeBootstrapStatusV1,
}

impl ProductNodeOverlayRuntimeV1 {
    pub fn bootstrap(config: &ProductNodeOverlayConfigV1, now_ms: u64) -> Result<Self> {
        let policy = trust_policy_v1(config);
        let (cache_loaded, mut sources) = load_cached_sources_v1(&config.cache_path)?;
        sources.extend(
            config
                .embedded_sources
                .iter()
                .cloned()
                .map(source_to_network_v1),
        );
        let resolution = resolve_bootstrap_sources_v1(&sources, &policy, now_ms)
            .context("resolve signed product bootstrap sources")?;
        let mut pool = RelayCandidatePoolV1::new(RelayCandidatePoolConfigV1 {
            cooldown_base_ms: config.cooldown_base_ms.unwrap_or(2_000),
            cooldown_max_ms: config.cooldown_max_ms.unwrap_or(300_000),
        });
        for record in resolution.relay_records.iter().cloned() {
            pool.upsert_verified_record(record)
                .context("insert verified relay record into product node pool")?;
        }
        let cache = ProductBootstrapCacheV1 {
            schema: PRODUCT_NODE_BOOTSTRAP_CACHE_SCHEMA_V1.into(),
            cached_at_ms: now_ms,
            manifests: accepted_manifests_v1(&sources, &policy, now_ms),
        };
        write_bootstrap_cache_v1(&config.cache_path, &cache)?;
        let status = ProductNodeBootstrapStatusV1 {
            cache_loaded,
            cache_path: config.cache_path.clone(),
            selected_source: resolution.selected_source,
            accepted_sources: resolution.accepted_sources,
            rejected_source_count: resolution.rejected_source_count,
            valid_relay_candidate_count: pool.candidate_count(),
            cache_written: true,
            full_raw_ip_directory_exposed: false,
            centralized_control_plane_required: false,
            single_official_relay_required: false,
        };
        Ok(Self {
            cache_path: config.cache_path.clone(),
            pool,
            status,
        })
    }

    #[must_use]
    pub fn bootstrap_status(&self) -> &ProductNodeBootstrapStatusV1 {
        &self.status
    }

    #[must_use]
    pub fn candidate_count(&self) -> usize {
        self.pool.candidate_count()
    }

    pub fn select_relay_route(
        &self,
        node_identity: &SigningKey,
        receipt_id: impl Into<String>,
        target_peer_id: &str,
        now_ms: u64,
        apfl_advisory_hash: Option<[u8; 32]>,
        apfl_advisory_applied: bool,
    ) -> ProductNodeRoutePlanV1 {
        let selected_relay = self.pool.select(now_ms);
        let decision = strategy_decision_v1(
            selected_relay.as_ref(),
            self.pool.candidate_count(),
            apfl_advisory_hash,
            apfl_advisory_applied,
        );
        self.sign_route_plan_v1(
            node_identity,
            receipt_id.into(),
            target_peer_id,
            now_ms,
            selected_relay,
            decision,
        )
    }

    pub fn record_relay_failure_and_rotate(
        &mut self,
        node_identity: &SigningKey,
        receipt_id: impl Into<String>,
        target_peer_id: &str,
        failed_relay_peer_id: &str,
        now_ms: u64,
    ) -> ProductNodeRoutePlanV1 {
        let outcome = self.pool.rotate_after_failure(failed_relay_peer_id, now_ms);
        let mut decision = strategy_decision_v1(
            outcome.selected.as_ref(),
            self.pool.candidate_count(),
            None,
            false,
        );
        if outcome.rotated {
            decision.selection_reason =
                "previous relay failed; rotated to a signed candidate after cooldown".into();
        }
        self.sign_route_plan_v1(
            node_identity,
            receipt_id.into(),
            target_peer_id,
            now_ms,
            outcome.selected,
            decision,
        )
    }

    pub fn select_after_nat_attempt(
        &self,
        node_identity: &SigningKey,
        receipt_id: impl Into<String>,
        target_peer_id: &str,
        nat_attempt: &NatPunchAttemptV1,
        now_ms: u64,
    ) -> ProductNodeRoutePlanV1 {
        let (selected_relay, decision) = match nat_attempt.selected_path_after_punch {
            NatSelectedPathV1::PunchedDirect => (
                None,
                StrategyDecisionV1 {
                    selected_path: StrategyPathV1::DirectNovoRudp,
                    selected_relay_peer_id: None,
                    selected_transport: None,
                    selection_reason: "signed cooperative NAT punch ack validated".into(),
                    rejected_candidate_count: 0,
                    fallback_reason: None,
                    apfl_advisory_hash: None,
                    apfl_advisory_applied: false,
                    hard_policy_override_attempted: false,
                    hard_policy_override_rejected: false,
                },
            ),
            NatSelectedPathV1::RelayNovoRudp => {
                let selected = self.pool.select(now_ms);
                let mut decision = strategy_decision_v1(
                    selected.as_ref(),
                    self.pool.candidate_count(),
                    None,
                    false,
                );
                decision.selection_reason =
                    "NAT punch did not validate; selected signed relay fallback".into();
                (selected, decision)
            }
            NatSelectedPathV1::QueueFallback => (
                None,
                StrategyDecisionV1 {
                    selected_path: StrategyPathV1::QueueFallback,
                    selected_relay_peer_id: None,
                    selected_transport: None,
                    selection_reason:
                        "NAT punch did not validate and no reachable relay candidate exists".into(),
                    rejected_candidate_count: self.pool.candidate_count() as u32,
                    fallback_reason: nat_attempt.fallback_reason.clone(),
                    apfl_advisory_hash: None,
                    apfl_advisory_applied: false,
                    hard_policy_override_attempted: false,
                    hard_policy_override_rejected: false,
                },
            ),
        };
        self.sign_route_plan_v1(
            node_identity,
            receipt_id.into(),
            target_peer_id,
            now_ms,
            selected_relay,
            decision,
        )
    }

    pub fn record_relay_success(&mut self, relay_peer_id: &str, rtt_ms: u64, now_ms: u64) {
        self.pool.record_success(relay_peer_id, rtt_ms, now_ms);
    }

    #[must_use]
    pub fn cache_path(&self) -> &Path {
        &self.cache_path
    }

    fn sign_route_plan_v1(
        &self,
        node_identity: &SigningKey,
        receipt_id: String,
        target_peer_id: &str,
        now_ms: u64,
        selected_relay: Option<RelayCandidateSnapshotV1>,
        decision: StrategyDecisionV1,
    ) -> ProductNodeRoutePlanV1 {
        let strategy_input = serde_json::to_vec(&serde_json::json!({
            "target_peer_id": target_peer_id,
            "bootstrap_selected_source": self.status.selected_source,
            "candidate_count": self.pool.candidate_count(),
            "payload_treated_opaque": true
        }))
        .expect("serialize bounded product strategy input");
        let strategy_receipt = sign_strategy_receipt_v1(
            node_identity,
            receipt_id,
            now_ms,
            &strategy_input,
            decision.clone(),
        );
        ProductNodeRoutePlanV1 {
            selected_path: decision.selected_path.clone(),
            selected_relay,
            fallback_reason: decision.fallback_reason.clone(),
            strategy_receipt,
        }
    }
}

pub fn load_product_node_overlay_config_v1(
    path: impl AsRef<Path>,
) -> Result<ProductNodeOverlayConfigV1> {
    let path = path.as_ref();
    let bytes = fs::read(path)
        .with_context(|| format!("read product node overlay config: {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("decode product node overlay config: {}", path.display()))
}

fn strategy_decision_v1(
    selected: Option<&RelayCandidateSnapshotV1>,
    candidate_count: usize,
    apfl_advisory_hash: Option<[u8; 32]>,
    apfl_advisory_applied: bool,
) -> StrategyDecisionV1 {
    match selected {
        Some(selected) => StrategyDecisionV1 {
            selected_path: StrategyPathV1::RelayNovoRudp,
            selected_relay_peer_id: Some(selected.relay_peer_id.clone()),
            selected_transport: Some(selected.selected_endpoint.transport.clone()),
            selection_reason: "valid signed relay candidate selected by local score".into(),
            rejected_candidate_count: candidate_count.saturating_sub(1) as u32,
            fallback_reason: None,
            apfl_advisory_hash,
            apfl_advisory_applied,
            hard_policy_override_attempted: false,
            hard_policy_override_rejected: false,
        },
        None => StrategyDecisionV1 {
            selected_path: StrategyPathV1::QueueFallback,
            selected_relay_peer_id: None,
            selected_transport: None,
            selection_reason: "no reachable signed relay candidate outside cooldown".into(),
            rejected_candidate_count: candidate_count as u32,
            fallback_reason: Some("NoReachableRelayCandidate".into()),
            apfl_advisory_hash,
            apfl_advisory_applied: false,
            hard_policy_override_attempted: false,
            hard_policy_override_rejected: false,
        },
    }
}

fn trust_policy_v1(config: &ProductNodeOverlayConfigV1) -> BootstrapTrustPolicyV1 {
    BootstrapTrustPolicyV1 {
        allowed_signer_peer_ids: config
            .trusted_signer_public_keys
            .iter()
            .map(peer_id_from_ed25519_public_key_v1)
            .collect::<BTreeSet<_>>(),
        minimum_valid_signatures: config.minimum_bootstrap_signatures.max(1),
    }
}

fn source_to_network_v1(source: ProductBootstrapSourceV1) -> BootstrapSourceV1 {
    BootstrapSourceV1 {
        source_kind: source.source_kind,
        priority: source.priority,
        manifest: source.manifest,
    }
}

fn load_cached_sources_v1(path: &Path) -> Result<(bool, Vec<BootstrapSourceV1>)> {
    if !path.exists() {
        return Ok((false, Vec::new()));
    }
    let bytes = fs::read(path)
        .with_context(|| format!("read product bootstrap cache: {}", path.display()))?;
    let cache: ProductBootstrapCacheV1 = serde_json::from_slice(&bytes)
        .with_context(|| format!("decode product bootstrap cache: {}", path.display()))?;
    if cache.schema != PRODUCT_NODE_BOOTSTRAP_CACHE_SCHEMA_V1 {
        anyhow::bail!(
            "unsupported product bootstrap cache schema: {}",
            cache.schema
        );
    }
    Ok((
        true,
        cache
            .manifests
            .into_iter()
            .map(|manifest| BootstrapSourceV1 {
                source_kind: BootstrapSourceKindV1::LocalCache,
                priority: 0,
                manifest,
            })
            .collect(),
    ))
}

fn accepted_manifests_v1(
    sources: &[BootstrapSourceV1],
    policy: &BootstrapTrustPolicyV1,
    now_ms: u64,
) -> Vec<SignedBootstrapManifestV1> {
    let mut manifests = sources
        .iter()
        .filter(|source| validate_bootstrap_manifest_v1(&source.manifest, policy, now_ms).accepted)
        .map(|source| source.manifest.clone())
        .collect::<Vec<_>>();
    manifests.sort_by(|left, right| left.manifest_id.cmp(&right.manifest_id));
    manifests.dedup_by(|left, right| left.manifest_id == right.manifest_id);
    manifests
}

fn write_bootstrap_cache_v1(path: &Path, cache: &ProductBootstrapCacheV1) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "create product bootstrap cache directory: {}",
                parent.display()
            )
        })?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(cache)?)
        .with_context(|| format!("write product bootstrap cache: {}", temporary.display()))?;
    if path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("replace product bootstrap cache: {}", path.display()))?;
    }
    fs::rename(&temporary, path)
        .with_context(|| format!("persist product bootstrap cache: {}", path.display()))?;
    Ok(())
}

fn default_minimum_bootstrap_signatures_v1() -> usize {
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_network::{
        fallback_after_nat_failure_v1, sign_bootstrap_manifest_v1, sign_relay_record_v1,
        validate_strategy_receipt_v1, NatDiagnosisV1, PeerSignedRelayRecordV1, RelayEndpointV1,
        RelayTransportV1,
    };

    fn relay_record(
        key: &SigningKey,
        name: &str,
        transport: RelayTransportV1,
    ) -> PeerSignedRelayRecordV1 {
        let uri = match transport {
            RelayTransportV1::Wss443 => format!("wss://{name}.example:443/novovm"),
            RelayTransportV1::Quic443 => format!("quic://{name}.example:443"),
            RelayTransportV1::Udp => format!("udp://{name}.example:41030"),
        };
        sign_relay_record_v1(
            key,
            format!("{name}-record"),
            vec![RelayEndpointV1 {
                transport,
                uri,
                priority: 10,
                max_sessions: 32,
                max_bytes_per_minute: 1_000_000,
            }],
            1_000,
            20_000,
            1,
        )
        .unwrap()
    }

    fn signed_source(signer: &SigningKey) -> ProductBootstrapSourceV1 {
        let relay_a = SigningKey::from_bytes(&[111; 32]);
        let relay_b = SigningKey::from_bytes(&[112; 32]);
        let mut manifest = SignedBootstrapManifestV1 {
            version: 1,
            manifest_id: "node-runtime-manifest".into(),
            issued_at_ms: 1_000,
            expires_at_ms: 20_000,
            candidate_limit: 2,
            full_raw_ip_directory_embedded: false,
            requires_single_official_relay: false,
            requires_single_official_domain: false,
            relay_records: vec![
                relay_record(&relay_a, "relay-a", RelayTransportV1::Wss443),
                relay_record(&relay_b, "relay-b", RelayTransportV1::Quic443),
            ],
            signatures: Vec::new(),
        };
        sign_bootstrap_manifest_v1(&mut manifest, signer).unwrap();
        ProductBootstrapSourceV1 {
            source_kind: BootstrapSourceKindV1::EmbeddedInstall,
            priority: 10,
            manifest,
        }
    }

    #[test]
    fn node_runtime_persists_signed_bootstrap_cache_rotates_and_emits_receipts() {
        let root = std::env::temp_dir().join(format!("novovm-product-node-overlay-{}", 7_001));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let signer = SigningKey::from_bytes(&[110; 32]);
        let config = ProductNodeOverlayConfigV1 {
            cache_path: root.join("bootstrap-cache.json"),
            trusted_signer_public_keys: vec![signer.verifying_key().to_bytes()],
            minimum_bootstrap_signatures: 1,
            embedded_sources: vec![signed_source(&signer)],
            cooldown_base_ms: Some(1_000),
            cooldown_max_ms: Some(10_000),
        };
        let node_identity = SigningKey::from_bytes(&[113; 32]);
        let mut runtime = ProductNodeOverlayRuntimeV1::bootstrap(&config, 2_000).unwrap();
        assert!(!runtime.bootstrap_status().cache_loaded);
        assert_eq!(runtime.candidate_count(), 2);
        let initial = runtime.select_relay_route(
            &node_identity,
            "receipt-initial",
            "target-b",
            2_000,
            None,
            false,
        );
        assert_eq!(initial.selected_path, StrategyPathV1::RelayNovoRudp);
        let selected = initial.selected_relay.unwrap();
        assert_eq!(
            selected.selected_endpoint.transport,
            RelayTransportV1::Wss443
        );
        validate_strategy_receipt_v1(&initial.strategy_receipt).unwrap();
        let rotated = runtime.record_relay_failure_and_rotate(
            &node_identity,
            "receipt-rotated",
            "target-b",
            &selected.relay_peer_id,
            2_000,
        );
        assert_eq!(rotated.selected_path, StrategyPathV1::RelayNovoRudp);
        assert_ne!(
            rotated.selected_relay.unwrap().relay_peer_id,
            selected.relay_peer_id
        );
        validate_strategy_receipt_v1(&rotated.strategy_receipt).unwrap();
        let fallback = fallback_after_nat_failure_v1(
            NatDiagnosisV1::UdpReachabilityBlockedOrAckReturnFailed,
            true,
        );
        let after_nat = runtime.select_after_nat_attempt(
            &node_identity,
            "receipt-nat-fallback",
            "target-b",
            &fallback,
            2_100,
        );
        assert_eq!(after_nat.selected_path, StrategyPathV1::RelayNovoRudp);
        validate_strategy_receipt_v1(&after_nat.strategy_receipt).unwrap();
        let cached = ProductNodeOverlayRuntimeV1::bootstrap(
            &ProductNodeOverlayConfigV1 {
                embedded_sources: Vec::new(),
                ..config
            },
            2_100,
        )
        .unwrap();
        assert!(cached.bootstrap_status().cache_loaded);
        assert_eq!(cached.candidate_count(), 2);
        let _ = fs::remove_dir_all(root);
    }
}
