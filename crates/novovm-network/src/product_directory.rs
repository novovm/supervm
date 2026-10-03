use crate::product_overlay::peer_id_from_ed25519_public_key_v1;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const PRODUCT_DIRECTORY_VERSION_V1: u16 = 1;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProductDirectoryErrorV1 {
    #[error("unsupported product directory version: {0}")]
    UnsupportedVersion(u16),
    #[error("record or manifest has expired")]
    Expired,
    #[error("relay peer id does not match its public key")]
    RelayIdentityMismatch,
    #[error("signature is invalid")]
    InvalidSignature,
    #[error("signature key material is invalid")]
    InvalidKeyMaterial,
    #[error("relay endpoint is unsupported: {0}")]
    UnsupportedEndpoint(String),
    #[error("bootstrap manifest attempts to expose a raw IP directory")]
    RawDirectoryForbidden,
    #[error("bootstrap manifest requires a single official service")]
    SingleOfficialServiceForbidden,
    #[error("bootstrap manifest exceeds its candidate disclosure limit")]
    CandidateDisclosureLimitExceeded,
    #[error("bootstrap manifest does not meet its trusted signer threshold")]
    InsufficientTrustedSigners,
    #[error("bootstrap resolution found no trusted manifest")]
    NoTrustedBootstrapSource,
    #[error("strategy receipt subject does not match its public key")]
    ReceiptIdentityMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RelayTransportV1 {
    Wss443,
    Quic443,
    Udp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayEndpointV1 {
    pub transport: RelayTransportV1,
    pub uri: String,
    pub priority: u16,
    pub max_sessions: u32,
    pub max_bytes_per_minute: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerSignedRelayRecordV1 {
    pub version: u16,
    pub record_id: String,
    pub relay_peer_id: String,
    pub relay_public_key: [u8; 32],
    pub endpoints: Vec<RelayEndpointV1>,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub sequence: u64,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedRelayRecordV1 {
    pub record: PeerSignedRelayRecordV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayCandidatePoolConfigV1 {
    pub cooldown_base_ms: u64,
    pub cooldown_max_ms: u64,
}

impl Default for RelayCandidatePoolConfigV1 {
    fn default() -> Self {
        Self {
            cooldown_base_ms: 2_000,
            cooldown_max_ms: 300_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayCandidateSnapshotV1 {
    pub relay_peer_id: String,
    pub selected_endpoint: RelayEndpointV1,
    pub failure_count: u32,
    pub cooldown_until_ms: u64,
    pub last_success_ms: Option<u64>,
    pub average_rtt_ms: Option<u64>,
    pub score: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayRotationOutcomeV1 {
    pub selected: Option<RelayCandidateSnapshotV1>,
    pub previous_relay_peer_id: Option<String>,
    pub rotated: bool,
}

#[derive(Debug, Clone)]
pub struct RelayCandidatePoolV1 {
    config: RelayCandidatePoolConfigV1,
    candidates: BTreeMap<String, RelayCandidateRuntimeV1>,
}

#[derive(Debug, Clone)]
struct RelayCandidateRuntimeV1 {
    record: PeerSignedRelayRecordV1,
    failure_count: u32,
    cooldown_until_ms: u64,
    last_success_ms: Option<u64>,
    average_rtt_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapManifestSignatureV1 {
    pub signer_peer_id: String,
    pub signer_public_key: [u8; 32],
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedBootstrapManifestV1 {
    pub version: u16,
    pub manifest_id: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub candidate_limit: u16,
    pub full_raw_ip_directory_embedded: bool,
    pub requires_single_official_relay: bool,
    pub requires_single_official_domain: bool,
    pub relay_records: Vec<PeerSignedRelayRecordV1>,
    pub signatures: Vec<BootstrapManifestSignatureV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapTrustPolicyV1 {
    pub allowed_signer_peer_ids: BTreeSet<String>,
    pub minimum_valid_signatures: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapValidationV1 {
    pub accepted: bool,
    pub trusted_signature_count: usize,
    pub valid_relay_record_count: usize,
    pub rejected_relay_record_count: usize,
    pub reject_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapSourceKindV1 {
    LocalCache,
    EmbeddedInstall,
    QrInvite,
    FriendInvite,
    Community,
    Official,
    DiscoveredDirectory,
}

#[derive(Debug, Clone)]
pub struct BootstrapSourceV1 {
    pub source_kind: BootstrapSourceKindV1,
    pub priority: u16,
    pub manifest: SignedBootstrapManifestV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapResolutionV1 {
    pub selected_source: Option<BootstrapSourceKindV1>,
    pub accepted_sources: Vec<BootstrapSourceKindV1>,
    pub relay_records: Vec<ValidatedRelayRecordV1>,
    pub rejected_source_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyPathV1 {
    DirectNovoRudp,
    RelayNovoRudp,
    MultiHopRelay,
    QueueFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyDecisionV1 {
    pub selected_path: StrategyPathV1,
    pub selected_relay_peer_id: Option<String>,
    pub selected_transport: Option<RelayTransportV1>,
    pub selection_reason: String,
    pub rejected_candidate_count: u32,
    pub fallback_reason: Option<String>,
    pub apfl_advisory_hash: Option<[u8; 32]>,
    pub apfl_advisory_applied: bool,
    pub hard_policy_override_attempted: bool,
    pub hard_policy_override_rejected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedStrategyReceiptV1 {
    pub version: u16,
    pub receipt_id: String,
    pub subject_peer_id: String,
    pub subject_public_key: [u8; 32],
    pub issued_at_ms: u64,
    pub input_hash: [u8; 32],
    pub decision: StrategyDecisionV1,
    pub decision_hash: [u8; 32],
    pub signature: Vec<u8>,
}

pub fn sign_relay_record_v1(
    signing_key: &SigningKey,
    record_id: impl Into<String>,
    endpoints: Vec<RelayEndpointV1>,
    issued_at_ms: u64,
    expires_at_ms: u64,
    sequence: u64,
) -> Result<PeerSignedRelayRecordV1, ProductDirectoryErrorV1> {
    let relay_public_key = signing_key.verifying_key().to_bytes();
    let relay_peer_id = peer_id_from_ed25519_public_key_v1(&relay_public_key);
    let mut record = PeerSignedRelayRecordV1 {
        version: PRODUCT_DIRECTORY_VERSION_V1,
        record_id: record_id.into(),
        relay_peer_id,
        relay_public_key,
        endpoints,
        issued_at_ms,
        expires_at_ms,
        sequence,
        signature: Vec::new(),
    };
    validate_relay_record_shape_v1(&record)?;
    record.signature = signing_key
        .sign(&relay_record_signing_bytes_v1(&record))
        .to_bytes()
        .to_vec();
    Ok(record)
}

pub fn validate_relay_record_v1(
    record: &PeerSignedRelayRecordV1,
    now_ms: u64,
) -> Result<ValidatedRelayRecordV1, ProductDirectoryErrorV1> {
    validate_relay_record_shape_v1(record)?;
    if record.expires_at_ms <= now_ms || record.issued_at_ms > now_ms {
        return Err(ProductDirectoryErrorV1::Expired);
    }
    if record.relay_peer_id != peer_id_from_ed25519_public_key_v1(&record.relay_public_key) {
        return Err(ProductDirectoryErrorV1::RelayIdentityMismatch);
    }
    verify_signature_v1(
        &record.relay_public_key,
        &relay_record_signing_bytes_v1(record),
        &record.signature,
    )?;
    Ok(ValidatedRelayRecordV1 {
        record: record.clone(),
    })
}

impl RelayCandidatePoolV1 {
    #[must_use]
    pub fn new(config: RelayCandidatePoolConfigV1) -> Self {
        Self {
            config,
            candidates: BTreeMap::new(),
        }
    }

    pub fn upsert_verified_record(
        &mut self,
        record: ValidatedRelayRecordV1,
    ) -> Result<(), ProductDirectoryErrorV1> {
        let peer_id = record.record.relay_peer_id.clone();
        let existing = self.candidates.get(&peer_id);
        if existing.is_some_and(|current| current.record.sequence > record.record.sequence) {
            return Ok(());
        }
        self.candidates.insert(
            peer_id,
            RelayCandidateRuntimeV1 {
                record: record.record,
                failure_count: existing.map_or(0, |current| current.failure_count),
                cooldown_until_ms: existing.map_or(0, |current| current.cooldown_until_ms),
                last_success_ms: existing.and_then(|current| current.last_success_ms),
                average_rtt_ms: existing.and_then(|current| current.average_rtt_ms),
            },
        );
        Ok(())
    }

    #[must_use]
    pub fn select(&self, now_ms: u64) -> Option<RelayCandidateSnapshotV1> {
        self.select_excluding(now_ms, None)
    }

    #[must_use]
    pub fn rotate_after_failure(
        &mut self,
        failed_peer_id: &str,
        now_ms: u64,
    ) -> RelayRotationOutcomeV1 {
        self.record_failure(failed_peer_id, now_ms);
        let selected = self.select_excluding(now_ms, Some(failed_peer_id));
        RelayRotationOutcomeV1 {
            rotated: selected
                .as_ref()
                .is_some_and(|candidate| candidate.relay_peer_id != failed_peer_id),
            selected,
            previous_relay_peer_id: Some(failed_peer_id.to_string()),
        }
    }

    pub fn record_success(&mut self, peer_id: &str, rtt_ms: u64, now_ms: u64) {
        if let Some(candidate) = self.candidates.get_mut(peer_id) {
            candidate.failure_count = 0;
            candidate.cooldown_until_ms = 0;
            candidate.last_success_ms = Some(now_ms);
            candidate.average_rtt_ms = Some(match candidate.average_rtt_ms {
                Some(previous) => previous.saturating_mul(7).saturating_add(rtt_ms) / 8,
                None => rtt_ms,
            });
        }
    }

    pub fn record_failure(&mut self, peer_id: &str, now_ms: u64) {
        if let Some(candidate) = self.candidates.get_mut(peer_id) {
            candidate.failure_count = candidate.failure_count.saturating_add(1);
            let exponent = candidate.failure_count.saturating_sub(1).min(16);
            let cooldown = self
                .config
                .cooldown_base_ms
                .saturating_mul(1u64 << exponent)
                .min(self.config.cooldown_max_ms);
            candidate.cooldown_until_ms = now_ms.saturating_add(cooldown);
        }
    }

    #[must_use]
    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    fn select_excluding(
        &self,
        now_ms: u64,
        excluded_peer_id: Option<&str>,
    ) -> Option<RelayCandidateSnapshotV1> {
        self.candidates
            .values()
            .filter(|candidate| candidate.record.expires_at_ms > now_ms)
            .filter(|candidate| candidate.cooldown_until_ms <= now_ms)
            .filter(|candidate| Some(candidate.record.relay_peer_id.as_str()) != excluded_peer_id)
            .filter_map(candidate_snapshot_v1)
            .max_by(|left, right| {
                left.score
                    .cmp(&right.score)
                    .then_with(|| right.relay_peer_id.cmp(&left.relay_peer_id))
            })
    }
}

impl BootstrapTrustPolicyV1 {
    #[must_use]
    pub fn single_signer(signer_public_key: [u8; 32]) -> Self {
        let signer_peer_id = peer_id_from_ed25519_public_key_v1(&signer_public_key);
        Self {
            allowed_signer_peer_ids: BTreeSet::from([signer_peer_id]),
            minimum_valid_signatures: 1,
        }
    }
}

pub fn sign_bootstrap_manifest_v1(
    manifest: &mut SignedBootstrapManifestV1,
    signing_key: &SigningKey,
) -> Result<(), ProductDirectoryErrorV1> {
    let signer_public_key = signing_key.verifying_key().to_bytes();
    let signer_peer_id = peer_id_from_ed25519_public_key_v1(&signer_public_key);
    let signature = signing_key
        .sign(&bootstrap_manifest_signing_bytes_v1(manifest))
        .to_bytes()
        .to_vec();
    manifest
        .signatures
        .retain(|existing| existing.signer_peer_id != signer_peer_id);
    manifest.signatures.push(BootstrapManifestSignatureV1 {
        signer_peer_id,
        signer_public_key,
        signature,
    });
    manifest
        .signatures
        .sort_by(|left, right| left.signer_peer_id.cmp(&right.signer_peer_id));
    Ok(())
}

pub fn validate_bootstrap_manifest_v1(
    manifest: &SignedBootstrapManifestV1,
    policy: &BootstrapTrustPolicyV1,
    now_ms: u64,
) -> BootstrapValidationV1 {
    let result = validate_bootstrap_manifest_inner_v1(manifest, policy, now_ms);
    match result {
        Ok((trusted_signature_count, valid_relay_record_count, rejected_relay_record_count)) => {
            BootstrapValidationV1 {
                accepted: true,
                trusted_signature_count,
                valid_relay_record_count,
                rejected_relay_record_count,
                reject_reason: None,
            }
        }
        Err(error) => BootstrapValidationV1 {
            accepted: false,
            trusted_signature_count: 0,
            valid_relay_record_count: 0,
            rejected_relay_record_count: manifest.relay_records.len(),
            reject_reason: Some(error.to_string()),
        },
    }
}

pub fn resolve_bootstrap_sources_v1(
    sources: &[BootstrapSourceV1],
    policy: &BootstrapTrustPolicyV1,
    now_ms: u64,
) -> Result<BootstrapResolutionV1, ProductDirectoryErrorV1> {
    let mut sources = sources.to_vec();
    sources.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.source_kind.cmp(&right.source_kind))
    });
    let mut accepted_sources = Vec::new();
    let mut rejected_source_count = 0usize;
    let mut records = BTreeMap::<String, ValidatedRelayRecordV1>::new();
    let mut selected_source = None;
    for source in sources {
        let validation = validate_bootstrap_manifest_v1(&source.manifest, policy, now_ms);
        if !validation.accepted {
            rejected_source_count = rejected_source_count.saturating_add(1);
            continue;
        }
        selected_source.get_or_insert(source.source_kind);
        accepted_sources.push(source.source_kind);
        for record in &source.manifest.relay_records {
            if let Ok(validated) = validate_relay_record_v1(record, now_ms) {
                let replace = records
                    .get(&validated.record.relay_peer_id)
                    .is_none_or(|current| current.record.sequence < validated.record.sequence);
                if replace {
                    records.insert(validated.record.relay_peer_id.clone(), validated);
                }
            }
        }
    }
    let Some(selected_source) = selected_source else {
        return Err(ProductDirectoryErrorV1::NoTrustedBootstrapSource);
    };
    Ok(BootstrapResolutionV1 {
        selected_source: Some(selected_source),
        accepted_sources,
        relay_records: records.into_values().collect(),
        rejected_source_count,
    })
}

pub fn sign_strategy_receipt_v1(
    signing_key: &SigningKey,
    receipt_id: impl Into<String>,
    issued_at_ms: u64,
    strategy_input: &[u8],
    decision: StrategyDecisionV1,
) -> SignedStrategyReceiptV1 {
    let subject_public_key = signing_key.verifying_key().to_bytes();
    let subject_peer_id = peer_id_from_ed25519_public_key_v1(&subject_public_key);
    let input_hash = hash_v1(b"novovm-product-overlay-strategy-input-v1", strategy_input);
    let decision_hash = hash_v1(
        b"novovm-product-overlay-strategy-decision-v1",
        &strategy_decision_bytes_v1(&decision),
    );
    let mut receipt = SignedStrategyReceiptV1 {
        version: PRODUCT_DIRECTORY_VERSION_V1,
        receipt_id: receipt_id.into(),
        subject_peer_id,
        subject_public_key,
        issued_at_ms,
        input_hash,
        decision,
        decision_hash,
        signature: Vec::new(),
    };
    receipt.signature = signing_key
        .sign(&strategy_receipt_signing_bytes_v1(&receipt))
        .to_bytes()
        .to_vec();
    receipt
}

pub fn validate_strategy_receipt_v1(
    receipt: &SignedStrategyReceiptV1,
) -> Result<(), ProductDirectoryErrorV1> {
    if receipt.version != PRODUCT_DIRECTORY_VERSION_V1 {
        return Err(ProductDirectoryErrorV1::UnsupportedVersion(receipt.version));
    }
    if receipt.subject_peer_id != peer_id_from_ed25519_public_key_v1(&receipt.subject_public_key) {
        return Err(ProductDirectoryErrorV1::ReceiptIdentityMismatch);
    }
    let expected_decision_hash = hash_v1(
        b"novovm-product-overlay-strategy-decision-v1",
        &strategy_decision_bytes_v1(&receipt.decision),
    );
    if receipt.decision_hash != expected_decision_hash {
        return Err(ProductDirectoryErrorV1::InvalidSignature);
    }
    verify_signature_v1(
        &receipt.subject_public_key,
        &strategy_receipt_signing_bytes_v1(receipt),
        &receipt.signature,
    )
}

fn validate_relay_record_shape_v1(
    record: &PeerSignedRelayRecordV1,
) -> Result<(), ProductDirectoryErrorV1> {
    if record.version != PRODUCT_DIRECTORY_VERSION_V1 {
        return Err(ProductDirectoryErrorV1::UnsupportedVersion(record.version));
    }
    if record.record_id.is_empty()
        || record.endpoints.is_empty()
        || record.expires_at_ms <= record.issued_at_ms
    {
        return Err(ProductDirectoryErrorV1::UnsupportedEndpoint(
            "record must have an id, endpoint, and positive lifetime".into(),
        ));
    }
    for endpoint in &record.endpoints {
        validate_endpoint_v1(endpoint)?;
    }
    Ok(())
}

fn validate_endpoint_v1(endpoint: &RelayEndpointV1) -> Result<(), ProductDirectoryErrorV1> {
    let valid = match endpoint.transport {
        RelayTransportV1::Wss443 => {
            endpoint.uri.starts_with("wss://") && endpoint.uri.contains(":443/")
        }
        RelayTransportV1::Quic443 => {
            endpoint.uri.starts_with("quic://") && endpoint.uri.contains(":443")
        }
        RelayTransportV1::Udp => endpoint.uri.starts_with("udp://"),
    };
    if !valid || endpoint.max_sessions == 0 || endpoint.max_bytes_per_minute == 0 {
        return Err(ProductDirectoryErrorV1::UnsupportedEndpoint(
            endpoint.uri.clone(),
        ));
    }
    Ok(())
}

fn candidate_snapshot_v1(candidate: &RelayCandidateRuntimeV1) -> Option<RelayCandidateSnapshotV1> {
    let selected_endpoint = candidate
        .record
        .endpoints
        .iter()
        .min_by(|left, right| {
            endpoint_score_v1(left)
                .cmp(&endpoint_score_v1(right))
                .then_with(|| left.priority.cmp(&right.priority))
        })?
        .clone();
    let transport_score = match selected_endpoint.transport {
        RelayTransportV1::Wss443 => 300i64,
        RelayTransportV1::Quic443 => 250,
        RelayTransportV1::Udp => 150,
    };
    let success_bonus = candidate.last_success_ms.map_or(0, |_| 80);
    let rtt_penalty = candidate.average_rtt_ms.unwrap_or(500).min(1_000) as i64 / 4;
    let failure_penalty = i64::from(candidate.failure_count).saturating_mul(100);
    let priority_penalty = i64::from(selected_endpoint.priority).saturating_mul(2);
    Some(RelayCandidateSnapshotV1 {
        relay_peer_id: candidate.record.relay_peer_id.clone(),
        selected_endpoint,
        failure_count: candidate.failure_count,
        cooldown_until_ms: candidate.cooldown_until_ms,
        last_success_ms: candidate.last_success_ms,
        average_rtt_ms: candidate.average_rtt_ms,
        score: transport_score + success_bonus - rtt_penalty - failure_penalty - priority_penalty,
    })
}

fn endpoint_score_v1(endpoint: &RelayEndpointV1) -> u8 {
    match endpoint.transport {
        RelayTransportV1::Wss443 => 0,
        RelayTransportV1::Quic443 => 1,
        RelayTransportV1::Udp => 2,
    }
}

fn validate_bootstrap_manifest_inner_v1(
    manifest: &SignedBootstrapManifestV1,
    policy: &BootstrapTrustPolicyV1,
    now_ms: u64,
) -> Result<(usize, usize, usize), ProductDirectoryErrorV1> {
    if manifest.version != PRODUCT_DIRECTORY_VERSION_V1 {
        return Err(ProductDirectoryErrorV1::UnsupportedVersion(
            manifest.version,
        ));
    }
    if manifest.issued_at_ms > now_ms || manifest.expires_at_ms <= now_ms {
        return Err(ProductDirectoryErrorV1::Expired);
    }
    if manifest.full_raw_ip_directory_embedded {
        return Err(ProductDirectoryErrorV1::RawDirectoryForbidden);
    }
    if manifest.requires_single_official_relay || manifest.requires_single_official_domain {
        return Err(ProductDirectoryErrorV1::SingleOfficialServiceForbidden);
    }
    if manifest.candidate_limit == 0
        || manifest.relay_records.len() > usize::from(manifest.candidate_limit)
    {
        return Err(ProductDirectoryErrorV1::CandidateDisclosureLimitExceeded);
    }
    let mut trusted_signers = BTreeSet::new();
    for signature in &manifest.signatures {
        let expected_peer_id = peer_id_from_ed25519_public_key_v1(&signature.signer_public_key);
        if signature.signer_peer_id != expected_peer_id
            || !policy
                .allowed_signer_peer_ids
                .contains(&signature.signer_peer_id)
        {
            continue;
        }
        if verify_signature_v1(
            &signature.signer_public_key,
            &bootstrap_manifest_signing_bytes_v1(manifest),
            &signature.signature,
        )
        .is_ok()
        {
            trusted_signers.insert(signature.signer_peer_id.clone());
        }
    }
    if trusted_signers.len() < policy.minimum_valid_signatures.max(1) {
        return Err(ProductDirectoryErrorV1::InsufficientTrustedSigners);
    }
    let mut valid_relay_record_count = 0usize;
    let mut rejected_relay_record_count = 0usize;
    for record in &manifest.relay_records {
        if validate_relay_record_v1(record, now_ms).is_ok() {
            valid_relay_record_count = valid_relay_record_count.saturating_add(1);
        } else {
            rejected_relay_record_count = rejected_relay_record_count.saturating_add(1);
        }
    }
    Ok((
        trusted_signers.len(),
        valid_relay_record_count,
        rejected_relay_record_count,
    ))
}

fn relay_record_signing_bytes_v1(record: &PeerSignedRelayRecordV1) -> Vec<u8> {
    let mut out = Vec::new();
    append_field_v1(&mut out, b"novovm-product-relay-record-v1");
    out.extend_from_slice(&record.version.to_be_bytes());
    append_field_v1(&mut out, record.record_id.as_bytes());
    append_field_v1(&mut out, record.relay_peer_id.as_bytes());
    out.extend_from_slice(&record.relay_public_key);
    append_endpoints_v1(&mut out, &record.endpoints);
    out.extend_from_slice(&record.issued_at_ms.to_be_bytes());
    out.extend_from_slice(&record.expires_at_ms.to_be_bytes());
    out.extend_from_slice(&record.sequence.to_be_bytes());
    out
}

fn bootstrap_manifest_signing_bytes_v1(manifest: &SignedBootstrapManifestV1) -> Vec<u8> {
    let mut out = Vec::new();
    append_field_v1(&mut out, b"novovm-product-bootstrap-manifest-v1");
    out.extend_from_slice(&manifest.version.to_be_bytes());
    append_field_v1(&mut out, manifest.manifest_id.as_bytes());
    out.extend_from_slice(&manifest.issued_at_ms.to_be_bytes());
    out.extend_from_slice(&manifest.expires_at_ms.to_be_bytes());
    out.extend_from_slice(&manifest.candidate_limit.to_be_bytes());
    out.push(u8::from(manifest.full_raw_ip_directory_embedded));
    out.push(u8::from(manifest.requires_single_official_relay));
    out.push(u8::from(manifest.requires_single_official_domain));
    out.extend_from_slice(&(manifest.relay_records.len() as u64).to_be_bytes());
    for record in &manifest.relay_records {
        append_field_v1(&mut out, &relay_record_signing_bytes_v1(record));
        append_field_v1(&mut out, &record.signature);
    }
    out
}

fn strategy_receipt_signing_bytes_v1(receipt: &SignedStrategyReceiptV1) -> Vec<u8> {
    let mut out = Vec::new();
    append_field_v1(&mut out, b"novovm-product-strategy-receipt-v1");
    out.extend_from_slice(&receipt.version.to_be_bytes());
    append_field_v1(&mut out, receipt.receipt_id.as_bytes());
    append_field_v1(&mut out, receipt.subject_peer_id.as_bytes());
    out.extend_from_slice(&receipt.subject_public_key);
    out.extend_from_slice(&receipt.issued_at_ms.to_be_bytes());
    out.extend_from_slice(&receipt.input_hash);
    append_field_v1(&mut out, &strategy_decision_bytes_v1(&receipt.decision));
    out.extend_from_slice(&receipt.decision_hash);
    out
}

fn strategy_decision_bytes_v1(decision: &StrategyDecisionV1) -> Vec<u8> {
    let mut out = Vec::new();
    append_field_v1(
        &mut out,
        match decision.selected_path {
            StrategyPathV1::DirectNovoRudp => b"direct_novorudp",
            StrategyPathV1::RelayNovoRudp => b"relay_novorudp",
            StrategyPathV1::MultiHopRelay => b"multi_hop_relay",
            StrategyPathV1::QueueFallback => b"queue_fallback",
        },
    );
    append_optional_string_v1(&mut out, decision.selected_relay_peer_id.as_deref());
    append_optional_string_v1(
        &mut out,
        decision
            .selected_transport
            .as_ref()
            .map(|transport| match transport {
                RelayTransportV1::Wss443 => "wss_443",
                RelayTransportV1::Quic443 => "quic_443",
                RelayTransportV1::Udp => "udp",
            }),
    );
    append_field_v1(&mut out, decision.selection_reason.as_bytes());
    out.extend_from_slice(&decision.rejected_candidate_count.to_be_bytes());
    append_optional_string_v1(&mut out, decision.fallback_reason.as_deref());
    match decision.apfl_advisory_hash {
        Some(hash) => {
            out.push(1);
            out.extend_from_slice(&hash);
        }
        None => out.push(0),
    }
    out.push(u8::from(decision.apfl_advisory_applied));
    out.push(u8::from(decision.hard_policy_override_attempted));
    out.push(u8::from(decision.hard_policy_override_rejected));
    out
}

fn append_endpoints_v1(out: &mut Vec<u8>, endpoints: &[RelayEndpointV1]) {
    out.extend_from_slice(&(endpoints.len() as u64).to_be_bytes());
    for endpoint in endpoints {
        append_field_v1(
            out,
            match endpoint.transport {
                RelayTransportV1::Wss443 => b"wss_443",
                RelayTransportV1::Quic443 => b"quic_443",
                RelayTransportV1::Udp => b"udp",
            },
        );
        append_field_v1(out, endpoint.uri.as_bytes());
        out.extend_from_slice(&endpoint.priority.to_be_bytes());
        out.extend_from_slice(&endpoint.max_sessions.to_be_bytes());
        out.extend_from_slice(&endpoint.max_bytes_per_minute.to_be_bytes());
    }
}

fn append_optional_string_v1(out: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => {
            out.push(1);
            append_field_v1(out, value.as_bytes());
        }
        None => out.push(0),
    }
}

fn append_field_v1(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn hash_v1(domain: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update((payload.len() as u64).to_be_bytes());
    hasher.update(payload);
    hasher.finalize().into()
}

fn verify_signature_v1(
    public_key: &[u8; 32],
    bytes: &[u8],
    signature: &[u8],
) -> Result<(), ProductDirectoryErrorV1> {
    let verifying_key = VerifyingKey::from_bytes(public_key)
        .map_err(|_| ProductDirectoryErrorV1::InvalidKeyMaterial)?;
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| ProductDirectoryErrorV1::InvalidSignature)?;
    verifying_key
        .verify(bytes, &Signature::from_bytes(&signature))
        .map_err(|_| ProductDirectoryErrorV1::InvalidSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(transport: RelayTransportV1, host: &str, priority: u16) -> RelayEndpointV1 {
        let uri = match transport {
            RelayTransportV1::Wss443 => format!("wss://{host}:443/novovm"),
            RelayTransportV1::Quic443 => format!("quic://{host}:443"),
            RelayTransportV1::Udp => format!("udp://{host}:41030"),
        };
        RelayEndpointV1 {
            transport,
            uri,
            priority,
            max_sessions: 256,
            max_bytes_per_minute: 8 * 1024 * 1024,
        }
    }

    fn record(
        key: &SigningKey,
        id: &str,
        transport: RelayTransportV1,
        sequence: u64,
    ) -> PeerSignedRelayRecordV1 {
        sign_relay_record_v1(
            key,
            id,
            vec![endpoint(transport, id, 10)],
            1_000,
            10_000,
            sequence,
        )
        .expect("sign relay record")
    }

    #[test]
    fn signed_relay_records_reject_tamper_expiry_and_identity_mismatch() {
        let key = SigningKey::from_bytes(&[81u8; 32]);
        let valid = record(&key, "relay-a", RelayTransportV1::Wss443, 1);
        assert!(validate_relay_record_v1(&valid, 2_000).is_ok());

        let mut tampered = valid.clone();
        tampered.endpoints[0].uri = "wss://attacker.example:443/novovm".into();
        assert_eq!(
            validate_relay_record_v1(&tampered, 2_000),
            Err(ProductDirectoryErrorV1::InvalidSignature)
        );

        let mut wrong_identity = valid.clone();
        wrong_identity.relay_peer_id = "novovm-ed25519:wrong".into();
        assert_eq!(
            validate_relay_record_v1(&wrong_identity, 2_000),
            Err(ProductDirectoryErrorV1::RelayIdentityMismatch)
        );

        assert_eq!(
            validate_relay_record_v1(&valid, 10_000),
            Err(ProductDirectoryErrorV1::Expired)
        );
    }

    #[test]
    fn candidate_pool_rotates_cools_down_and_prefers_wss() {
        let key_a = SigningKey::from_bytes(&[82u8; 32]);
        let key_b = SigningKey::from_bytes(&[83u8; 32]);
        let mut pool = RelayCandidatePoolV1::new(RelayCandidatePoolConfigV1 {
            cooldown_base_ms: 1_000,
            cooldown_max_ms: 10_000,
        });
        let relay_a = record(&key_a, "relay-a", RelayTransportV1::Wss443, 1);
        let relay_b = record(&key_b, "relay-b", RelayTransportV1::Quic443, 1);
        pool.upsert_verified_record(validate_relay_record_v1(&relay_a, 2_000).expect("valid a"))
            .expect("upsert a");
        pool.upsert_verified_record(validate_relay_record_v1(&relay_b, 2_000).expect("valid b"))
            .expect("upsert b");
        let selected = pool.select(2_000).expect("selected relay");
        assert_eq!(selected.relay_peer_id, relay_a.relay_peer_id);
        let rotated = pool.rotate_after_failure(&relay_a.relay_peer_id, 2_000);
        assert!(rotated.rotated);
        assert_eq!(
            rotated.selected.expect("backup relay").relay_peer_id,
            relay_b.relay_peer_id
        );
        pool.record_success(&relay_b.relay_peer_id, 35, 2_100);
        assert_eq!(pool.candidate_count(), 2);
    }

    #[test]
    fn bootstrap_requires_configured_trusted_signers_and_merges_sources() {
        let signer_a = SigningKey::from_bytes(&[84u8; 32]);
        let signer_b = SigningKey::from_bytes(&[85u8; 32]);
        let relay_a = SigningKey::from_bytes(&[86u8; 32]);
        let relay_b = SigningKey::from_bytes(&[87u8; 32]);
        let mut manifest = SignedBootstrapManifestV1 {
            version: PRODUCT_DIRECTORY_VERSION_V1,
            manifest_id: "manifest-a".into(),
            issued_at_ms: 1_000,
            expires_at_ms: 10_000,
            candidate_limit: 4,
            full_raw_ip_directory_embedded: false,
            requires_single_official_relay: false,
            requires_single_official_domain: false,
            relay_records: vec![
                record(&relay_a, "relay-a", RelayTransportV1::Wss443, 1),
                record(&relay_b, "relay-b", RelayTransportV1::Quic443, 1),
            ],
            signatures: Vec::new(),
        };
        sign_bootstrap_manifest_v1(&mut manifest, &signer_a).expect("sign a");
        let policy = BootstrapTrustPolicyV1 {
            allowed_signer_peer_ids: BTreeSet::from([
                peer_id_from_ed25519_public_key_v1(&signer_a.verifying_key().to_bytes()),
                peer_id_from_ed25519_public_key_v1(&signer_b.verifying_key().to_bytes()),
            ]),
            minimum_valid_signatures: 2,
        };
        assert!(!validate_bootstrap_manifest_v1(&manifest, &policy, 2_000).accepted);
        sign_bootstrap_manifest_v1(&mut manifest, &signer_b).expect("sign b");
        let validation = validate_bootstrap_manifest_v1(&manifest, &policy, 2_000);
        assert!(validation.accepted);
        assert_eq!(validation.trusted_signature_count, 2);
        let resolution = resolve_bootstrap_sources_v1(
            &[
                BootstrapSourceV1 {
                    source_kind: BootstrapSourceKindV1::Community,
                    priority: 20,
                    manifest: manifest.clone(),
                },
                BootstrapSourceV1 {
                    source_kind: BootstrapSourceKindV1::LocalCache,
                    priority: 1,
                    manifest,
                },
            ],
            &policy,
            2_000,
        )
        .expect("resolve sources");
        assert_eq!(
            resolution.selected_source,
            Some(BootstrapSourceKindV1::LocalCache)
        );
        assert_eq!(resolution.relay_records.len(), 2);
    }

    #[test]
    fn bootstrap_rejects_raw_directory_and_single_official_requirements() {
        let signer = SigningKey::from_bytes(&[88u8; 32]);
        let policy = BootstrapTrustPolicyV1::single_signer(signer.verifying_key().to_bytes());
        let mut manifest = SignedBootstrapManifestV1 {
            version: PRODUCT_DIRECTORY_VERSION_V1,
            manifest_id: "bad-manifest".into(),
            issued_at_ms: 1_000,
            expires_at_ms: 10_000,
            candidate_limit: 1,
            full_raw_ip_directory_embedded: true,
            requires_single_official_relay: false,
            requires_single_official_domain: false,
            relay_records: Vec::new(),
            signatures: Vec::new(),
        };
        sign_bootstrap_manifest_v1(&mut manifest, &signer).expect("sign manifest");
        let validation = validate_bootstrap_manifest_v1(&manifest, &policy, 2_000);
        assert!(!validation.accepted);
        assert!(validation
            .reject_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("raw IP directory")));
    }

    #[test]
    fn bootstrap_rejects_records_beyond_declared_minimal_candidate_limit() {
        let signer = SigningKey::from_bytes(&[91u8; 32]);
        let relay_a = SigningKey::from_bytes(&[92u8; 32]);
        let relay_b = SigningKey::from_bytes(&[93u8; 32]);
        let policy = BootstrapTrustPolicyV1::single_signer(signer.verifying_key().to_bytes());
        let mut manifest = SignedBootstrapManifestV1 {
            version: PRODUCT_DIRECTORY_VERSION_V1,
            manifest_id: "over-disclosure".into(),
            issued_at_ms: 1_000,
            expires_at_ms: 10_000,
            candidate_limit: 1,
            full_raw_ip_directory_embedded: false,
            requires_single_official_relay: false,
            requires_single_official_domain: false,
            relay_records: vec![
                record(&relay_a, "relay-a", RelayTransportV1::Wss443, 1),
                record(&relay_b, "relay-b", RelayTransportV1::Quic443, 1),
            ],
            signatures: Vec::new(),
        };
        sign_bootstrap_manifest_v1(&mut manifest, &signer).expect("sign manifest");
        let validation = validate_bootstrap_manifest_v1(&manifest, &policy, 2_000);
        assert!(!validation.accepted);
        assert!(validation
            .reject_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("candidate disclosure limit")));
    }

    #[test]
    fn signed_strategy_receipt_is_replayable_and_tamper_evident() {
        let key = SigningKey::from_bytes(&[89u8; 32]);
        let decision = StrategyDecisionV1 {
            selected_path: StrategyPathV1::RelayNovoRudp,
            selected_relay_peer_id: Some("relay-a".into()),
            selected_transport: Some(RelayTransportV1::Wss443),
            selection_reason: "valid signed relay selected".into(),
            rejected_candidate_count: 1,
            fallback_reason: None,
            apfl_advisory_hash: Some([0x90; 32]),
            apfl_advisory_applied: true,
            hard_policy_override_attempted: false,
            hard_policy_override_rejected: false,
        };
        let receipt =
            sign_strategy_receipt_v1(&key, "receipt-1", 2_000, b"strategy-input", decision);
        validate_strategy_receipt_v1(&receipt).expect("valid receipt");
        let mut tampered = receipt.clone();
        tampered.decision.selected_relay_peer_id = Some("relay-attacker".into());
        assert_eq!(
            validate_strategy_receipt_v1(&tampered),
            Err(ProductDirectoryErrorV1::InvalidSignature)
        );
    }
}
