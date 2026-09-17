#![forbid(unsafe_code)]

//! Pure legacy-to-V2 nonce transition candidates. An artifact is an explicitly
//! proposed Host snapshot, not an AOEM execution result or an authority import.
//! No environment, runtime, filesystem, protocol activation, or live storage is
//! consulted. Callers must establish checkpoint provenance and runtime policy.

use super::native_nonce_bundle::{
    checkpoint_bundle_digest_v1, verified_nonce_checkpoint_inputs_v1,
    MAX_CHECKPOINT_BUNDLE_BYTES_V1,
};
use super::native_nonce_checkpoint::NonceMigrationCheckpointV1;
use super::*;
use std::io::Write;

pub const MAX_NONCE_UPGRADE_BYTES_V1: usize = 32 * 1024 * 1024;
const SCHEMA_V1: &str = "novovm-native-nonce-upgrade-transition/v1";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NonceUpgradeTransitionV1 {
    pub schema: &'static str,
    pub transition_id: String,
    pub source_bundle_digest: String,
    pub checkpoint: NonceMigrationCheckpointV1,
    pub target_protocol_config_commitment: String,
    pub legacy_state_root: String,
    pub proposed_state_root: String,
    pub receipt_root: String,
    pub state_version: u64,
    pub history_transaction_count: usize,
    pub ordered_history_commitment: String,
    pub business_state_preserved: bool,
    pub historical_receipts_preserved: bool,
    pub target_runtime_compatibility_verified: bool,
    pub independent_provenance_verified: bool,
    pub execution_replayed: bool,
    pub authority_state_published: bool,
    pub activation_ready: bool,
    pub import_performed: bool,
    pub aoem_evidence_verified: bool,
    pub qc_verified: bool,
    pub chain_canonical: bool,
    // Deliberately not exposed as an authority-import API. Serialization embeds
    // this proposed snapshot inside the separate transition schema only.
    proposed_store: NovNativeExecutionStoreV1,
}

fn require_commitment(value: &str, label: &str) -> Result<()> {
    let parsed = parse_fixed_hex_32_v1(value, label)?;
    if to_hex(&parsed) != value {
        bail!("nonce upgrade {label} must be canonical lowercase 32-byte hex");
    }
    Ok(())
}

struct BoundedJson {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other(
                "nonce upgrade JSON exceeds byte limit",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn encode_bounded(value: &impl serde::Serialize, limit: usize) -> Result<Vec<u8>> {
    let mut writer = BoundedJson {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut writer, value).context("encode nonce upgrade artifact")?;
    Ok(writer.bytes)
}

fn transition_id(transition: &NonceUpgradeTransitionV1) -> Result<String> {
    let mut commitment = transition.clone();
    commitment.transition_id.clear();
    let bytes = encode_bounded(&commitment, MAX_NONCE_UPGRADE_BYTES_V1)?;
    Ok(to_hex(&sha256_bytes_v1(&[
        b"novovm-native-nonce-upgrade-transition-v1\0",
        &bytes,
    ])))
}

/// Generate an isolated proposal from fully verified checkpoint inputs. The
/// target commitment is explicit, canonical and different from the old pin;
/// this pure function does not assert it matches any machine's configuration.
pub fn plan_nonce_upgrade_v1(
    bundle: &[u8],
    expected_bundle_digest: &str,
    checkpoint: &NonceMigrationCheckpointV1,
    target_protocol: &str,
) -> Result<NonceUpgradeTransitionV1> {
    if bundle.is_empty() || bundle.len() > MAX_CHECKPOINT_BUNDLE_BYTES_V1 {
        bail!("nonce upgrade source bundle exceeds its nonempty size bound");
    }
    require_commitment(expected_bundle_digest, "source bundle digest")?;
    require_commitment(target_protocol, "target protocol commitment")?;
    if target_protocol == checkpoint.legacy_protocol_config_commitment {
        bail!("nonce upgrade requires an explicit changed protocol commitment");
    }
    if checkpoint_bundle_digest_v1(bundle) != expected_bundle_digest {
        bail!("nonce upgrade source bundle digest mismatch");
    }
    let (snapshot, head, verified, _) = verified_nonce_checkpoint_inputs_v1(bundle, checkpoint)?;
    let source: NovNativeExecutionStoreV1 = serde_json::from_slice(snapshot)?;
    let source_value: serde_json::Value = serde_json::from_slice(snapshot)?;
    let typed_source_value = serde_json::to_value(&source)
        .context("nonce upgrade source cannot be represented without numeric loss")?;
    // A lenient Host reader may ignore unknown fields or supply old defaults.
    // An explicit conversion must not silently discard or invent those fields.
    // The legacy absent identity marker roundtrips because serde skips it when empty.
    if source_value != typed_source_value {
        bail!("nonce upgrade requires an exact complete snapshot schema; unknown, omitted, or normalized fields reject");
    }

    let mut proposed = source.clone();
    proposed.module_state.native_auth_nonce_identity_scheme =
        NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2.to_string();
    proposed.module_state.native_auth_next_nonces =
        verified.migration.proposed_native_auth_next_nonces.clone();
    proposed.module_state.native_auth_nonce_reservations = verified
        .migration
        .proposed_native_auth_nonce_reservations
        .clone();
    proposed.module_state.protocol_config_commitment = target_protocol.to_string();

    // Structural equality after restoring the four authorized fields ensures
    // receipts, business state, timestamps and semantic transaction versions are
    // carried unchanged. Upgrade metadata must not pretend another tx executed.
    let mut restored = proposed.clone();
    restored.module_state.native_auth_nonce_identity_scheme = source
        .module_state
        .native_auth_nonce_identity_scheme
        .clone();
    restored.module_state.native_auth_next_nonces =
        source.module_state.native_auth_next_nonces.clone();
    restored.module_state.native_auth_nonce_reservations =
        source.module_state.native_auth_nonce_reservations.clone();
    restored.module_state.protocol_config_commitment =
        source.module_state.protocol_config_commitment.clone();
    if restored != source {
        bail!("nonce upgrade modified state outside the four authorized fields");
    }
    // Shared root helpers expect representable JSON. Check before their
    // infallible internal projection to avoid a panic on unsupported numbers.
    serde_json::to_value(&proposed.module_state)?;
    let legacy_state_root = native_semantic_ledger_state_digest_v1(&source.module_state);
    let proposed_state_root = native_semantic_ledger_state_digest_v1(&proposed.module_state);
    let receipt_root = native_execution_receipt_root_v2(&source)?;
    if legacy_state_root != to_hex(&head.post_state_root)
        || receipt_root != to_hex(&head.cumulative_receipt_root)
        || receipt_root != native_execution_receipt_root_v2(&proposed)?
        || proposed.module_state.aoem_semantic_ledger_sequence != head.state_version
        || legacy_state_root == proposed_state_root
    {
        bail!("nonce upgrade roots, receipts, or semantic transaction version mismatch");
    }
    let mut transition = NonceUpgradeTransitionV1 {
        schema: SCHEMA_V1,
        transition_id: String::new(),
        source_bundle_digest: expected_bundle_digest.to_string(),
        checkpoint: checkpoint.clone(),
        target_protocol_config_commitment: target_protocol.to_string(),
        legacy_state_root,
        proposed_state_root,
        receipt_root,
        state_version: head.state_version,
        history_transaction_count: verified.tx_count,
        ordered_history_commitment: verified.migration.ordered_history_commitment,
        business_state_preserved: true,
        historical_receipts_preserved: true,
        target_runtime_compatibility_verified: false,
        independent_provenance_verified: false,
        execution_replayed: false,
        authority_state_published: false,
        activation_ready: false,
        import_performed: false,
        aoem_evidence_verified: false,
        qc_verified: false,
        chain_canonical: false,
        proposed_store: proposed,
    };
    transition.transition_id = transition_id(&transition)?;
    // Final serialized size includes the populated transition ID.
    encode_nonce_upgrade_v1(&transition)?;
    Ok(transition)
}

/// Encode the deterministic separate transition envelope. This is not a
/// substitute for verification against the original bundle and independent pins.
pub fn encode_nonce_upgrade_v1(transition: &NonceUpgradeTransitionV1) -> Result<Vec<u8>> {
    if transition.schema != SCHEMA_V1 || transition.transition_id != transition_id(transition)? {
        bail!("nonce upgrade transition identity mismatch");
    }
    encode_bounded(transition, MAX_NONCE_UPGRADE_BYTES_V1)
}

/// Recompute all proposed state and metadata from the pinned source; never
/// trust deserialized success flags, roots, IDs, or a caller-edited snapshot.
/// Exact canonical bytes reject extra fields, duplicate keys and trailing data.
pub fn verify_nonce_upgrade_v1(
    artifact: &[u8],
    bundle: &[u8],
    expected_digest: &str,
    checkpoint: &NonceMigrationCheckpointV1,
    target_protocol: &str,
) -> Result<NonceUpgradeTransitionV1> {
    if artifact.is_empty() || artifact.len() > MAX_NONCE_UPGRADE_BYTES_V1 {
        bail!("nonce upgrade artifact exceeds its nonempty 32 MiB bound");
    }
    let expected = plan_nonce_upgrade_v1(bundle, expected_digest, checkpoint, target_protocol)?;
    if artifact != encode_nonce_upgrade_v1(&expected)? {
        bail!("nonce upgrade artifact differs from the deterministic pinned transition");
    }
    Ok(expected)
}

#[cfg(test)]
mod tests {
    use super::super::native_nonce_bundle::{encode_bundle, verify_nonce_checkpoint_bundle_v1};
    use super::super::native_nonce_checkpoint::test_fixture_v1;
    use super::*;

    const TARGET: &str = "efefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefef";

    fn fixture() -> (Vec<u8>, NonceMigrationCheckpointV1, Vec<u8>) {
        let (snapshot, head, blocks, checkpoint, _) = test_fixture_v1();
        let bundle = encode_bundle(&snapshot, &head, &blocks).unwrap();
        (bundle, checkpoint, snapshot)
    }

    fn with_snapshot(bundle: &[u8], snapshot: &[u8]) -> Vec<u8> {
        let previous_len = u32::from_le_bytes(bundle[8..12].try_into().unwrap()) as usize;
        let mut changed = bundle[..8].to_vec();
        changed.extend_from_slice(&(snapshot.len() as u32).to_le_bytes());
        changed.extend_from_slice(snapshot);
        changed.extend_from_slice(&bundle[12 + previous_len..]);
        changed
    }

    fn reanchor_snapshot(checkpoint: &mut NonceMigrationCheckpointV1, snapshot: &[u8]) {
        checkpoint.snapshot_digest = to_hex(&sha256_bytes_v1(&[
            b"novovm-native-nonce-migration-source-snapshot-v1\0",
            snapshot,
        ]));
    }

    #[test]
    fn native_nonce_upgrade_is_deterministic_pure_and_preserves_business_history() {
        let (bundle, checkpoint, snapshot) = fixture();
        let original_bundle = bundle.clone();
        let digest = checkpoint_bundle_digest_v1(&bundle);
        let transition = plan_nonce_upgrade_v1(&bundle, &digest, &checkpoint, TARGET).unwrap();
        assert_eq!(
            transition,
            plan_nonce_upgrade_v1(&bundle, &digest, &checkpoint, TARGET).unwrap()
        );
        let bytes = encode_nonce_upgrade_v1(&transition).unwrap();
        // Even the lenient Host deserializer cannot turn the envelope into a
        // runnable V2 store: its schema differs and its absent module is legacy.
        let mistaken_store: NovNativeExecutionStoreV1 = serde_json::from_slice(&bytes).unwrap();
        assert_ne!(mistaken_store.schema, NOV_NATIVE_EXECUTION_STORE_SCHEMA_V1);
        assert!(verify_native_nonce_identity_scheme_v2(&mistaken_store).is_err());
        assert_eq!(
            transition,
            verify_nonce_upgrade_v1(&bytes, &bundle, &digest, &checkpoint, TARGET).unwrap()
        );
        assert_eq!(bundle, original_bundle);
        assert_ne!(transition.legacy_state_root, transition.proposed_state_root);
        assert_eq!(
            transition.history_transaction_count as u64,
            transition.state_version
        );
        assert!(transition.business_state_preserved && transition.historical_receipts_preserved);
        assert!(
            !transition.authority_state_published
                && !transition.activation_ready
                && !transition.import_performed
                && !transition.aoem_evidence_verified
                && !transition.qc_verified
                && !transition.chain_canonical
                && !transition.target_runtime_compatibility_verified
                && !transition.independent_provenance_verified
                && !transition.execution_replayed
        );
        let source: NovNativeExecutionStoreV1 = serde_json::from_slice(&snapshot).unwrap();
        let proposed = &transition.proposed_store;
        assert_eq!(proposed.receipts, source.receipts);
        assert_eq!(proposed.last_updated_unix_ms, source.last_updated_unix_ms);
        let mut actual: serde_json::Value = serde_json::to_value(proposed).unwrap();
        let expected = serde_json::to_value(&source).unwrap();
        for field in [
            "native_auth_next_nonces",
            "native_auth_nonce_reservations",
            "protocol_config_commitment",
        ] {
            actual["module_state"][field] = expected["module_state"][field].clone();
        }
        actual["module_state"]
            .as_object_mut()
            .unwrap()
            .remove("native_auth_nonce_identity_scheme");
        assert_eq!(actual, expected);
        assert_eq!(
            proposed.module_state.native_auth_nonce_identity_scheme,
            NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2
        );
        assert_eq!(proposed.module_state.protocol_config_commitment, TARGET);
        assert_ne!(
            proposed.module_state.native_auth_next_nonces,
            source.module_state.native_auth_next_nonces
        );
    }

    #[test]
    fn native_nonce_upgrade_rejects_wrong_digest_target_and_checkpoint() {
        let (bundle, checkpoint, _) = fixture();
        let digest = checkpoint_bundle_digest_v1(&bundle);
        for target in [
            "",
            "EF",
            "EFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEFEF",
            checkpoint.legacy_protocol_config_commitment.as_str(),
        ] {
            assert!(plan_nonce_upgrade_v1(&bundle, &digest, &checkpoint, target).is_err());
        }
        assert!(plan_nonce_upgrade_v1(&bundle, &"00".repeat(32), &checkpoint, TARGET).is_err());
        assert!(
            plan_nonce_upgrade_v1(&bundle, &digest.to_ascii_uppercase(), &checkpoint, TARGET)
                .is_err()
        );
        let mut changed = checkpoint.clone();
        changed.tip_block_hash = "11".repeat(32);
        assert!(plan_nonce_upgrade_v1(&bundle, &digest, &changed, TARGET).is_err());
        let other = plan_nonce_upgrade_v1(&bundle, &digest, &checkpoint, &"12".repeat(32)).unwrap();
        let original = plan_nonce_upgrade_v1(&bundle, &digest, &checkpoint, TARGET).unwrap();
        assert_ne!(other.transition_id, original.transition_id);
        assert_ne!(other.proposed_state_root, original.proposed_state_root);
    }

    #[test]
    fn native_nonce_upgrade_rejects_reanchored_unknown_or_omitted_snapshot_fields() {
        let (bundle, checkpoint, snapshot) = fixture();
        for mode in 0..4 {
            let mut value: serde_json::Value = serde_json::from_slice(&snapshot).unwrap();
            match mode {
                0 => {
                    value["unknown_future_authority"] = serde_json::json!({"balance": 123});
                }
                1 => {
                    value["module_state"]["unknown_future_rule"] = serde_json::json!(true);
                }
                2 => {
                    value
                        .as_object_mut()
                        .unwrap()
                        .remove("last_updated_unix_ms");
                }
                _ => {
                    value["module_state"]
                        .as_object_mut()
                        .unwrap()
                        .remove("next_governance_proposal_id");
                }
            }
            let changed_snapshot = serde_json::to_vec(&value).unwrap();
            let changed_bundle = with_snapshot(&bundle, &changed_snapshot);
            let mut changed_checkpoint = checkpoint.clone();
            reanchor_snapshot(&mut changed_checkpoint, &changed_snapshot);
            // Existing checkpoint verification is intentionally a permissive
            // reader; explicit conversion must be stricter about information loss.
            verify_nonce_checkpoint_bundle_v1(&changed_bundle, &changed_checkpoint).unwrap();
            let error = plan_nonce_upgrade_v1(
                &changed_bundle,
                &checkpoint_bundle_digest_v1(&changed_bundle),
                &changed_checkpoint,
                TARGET,
            )
            .unwrap_err();
            assert!(error.to_string().contains("exact complete snapshot schema"));
        }
    }

    #[test]
    fn native_nonce_upgrade_rejects_tampered_and_recommitted_transition_fields() {
        let (bundle, checkpoint, _) = fixture();
        let digest = checkpoint_bundle_digest_v1(&bundle);
        let transition = plan_nonce_upgrade_v1(&bundle, &digest, &checkpoint, TARGET).unwrap();
        for mode in 0..8 {
            let mut changed = transition.clone();
            match mode {
                0 => changed.activation_ready = true,
                1 => changed.state_version += 1,
                2 => {
                    changed
                        .proposed_store
                        .module_state
                        .aoem_semantic_ledger_sequence += 1
                }
                3 => {
                    changed
                        .proposed_store
                        .module_state
                        .treasury_reserves
                        .insert("NOV".to_string(), 123);
                }
                4 => {
                    changed
                        .proposed_store
                        .receipts
                        .values_mut()
                        .next()
                        .unwrap()
                        .status ^= true
                }
                5 => changed.proposed_state_root = "ab".repeat(32),
                6 => changed
                    .proposed_store
                    .module_state
                    .native_auth_next_nonces
                    .clear(),
                _ => changed.target_runtime_compatibility_verified = true,
            }
            // Even a fully self-consistent attacker-recomputed artifact ID
            // cannot replace verification against the pinned source history.
            changed.transition_id = transition_id(&changed).unwrap();
            let artifact = encode_nonce_upgrade_v1(&changed).unwrap();
            assert!(
                verify_nonce_upgrade_v1(&artifact, &bundle, &digest, &checkpoint, TARGET).is_err()
            );
        }
    }

    #[test]
    fn native_nonce_upgrade_rejects_noncanonical_partial_and_oversized_artifacts() {
        let (bundle, checkpoint, _) = fixture();
        let digest = checkpoint_bundle_digest_v1(&bundle);
        let transition = plan_nonce_upgrade_v1(&bundle, &digest, &checkpoint, TARGET).unwrap();
        let bytes = encode_nonce_upgrade_v1(&transition).unwrap();
        for length in [0, 1, bytes.len() - 1] {
            assert!(verify_nonce_upgrade_v1(
                &bytes[..length],
                &bundle,
                &digest,
                &checkpoint,
                TARGET
            )
            .is_err());
        }
        let mut changed = bytes.clone();
        changed.push(b'\n');
        assert!(verify_nonce_upgrade_v1(&changed, &bundle, &digest, &checkpoint, TARGET).is_err());
        let pretty = serde_json::to_vec_pretty(&transition).unwrap();
        assert!(verify_nonce_upgrade_v1(&pretty, &bundle, &digest, &checkpoint, TARGET).is_err());
        let mut unknown = serde_json::to_value(&transition).unwrap();
        unknown["activation_authorized"] = serde_json::json!(true);
        assert!(verify_nonce_upgrade_v1(
            &serde_json::to_vec(&unknown).unwrap(),
            &bundle,
            &digest,
            &checkpoint,
            TARGET
        )
        .is_err());
        let oversized = vec![b' '; MAX_NONCE_UPGRADE_BYTES_V1 + 1];
        assert!(verify_nonce_upgrade_v1(&oversized, &[], "", &checkpoint, TARGET).is_err());
        assert!(encode_bounded(&transition, bytes.len() - 1).is_err());
        assert_eq!(encode_bounded(&transition, bytes.len()).unwrap(), bytes);
        let mut changed = transition;
        changed.transition_id = "00".repeat(32);
        assert!(encode_nonce_upgrade_v1(&changed).is_err());
    }
}
