#![forbid(unsafe_code)]

//! Bounded, pure offline review of a legacy nonce snapshot. The caller must
//! independently establish the provenance of the complete Host store JSON and
//! ordered, committed raw history. This does not read live stores, verify block
//! or QC evidence, execute transactions, activate a protocol, or import state.

use super::*;
use std::collections::BTreeSet;

pub const MAX_SNAPSHOT_BYTES_V1: usize = 16 * 1024 * 1024;
pub const MAX_HISTORY_TRANSACTIONS_V1: usize = 65_536;
pub const MAX_HISTORY_BYTES_V1: usize = 64 * 1024 * 1024;
pub const MAX_TRANSACTION_BYTES_V1: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NonceMigrationPlanV1 {
    pub schema: &'static str,
    pub chain_id: u64,
    pub namespace_digest: String,
    pub legacy_protocol_config_commitment: String,
    pub legacy_nonce_identity_scheme: &'static str,
    pub proposed_nonce_identity_scheme: &'static str,
    pub source_snapshot_digest: String,
    pub ordered_history_commitment: String,
    pub history_transaction_count: usize,
    pub history_bytes: usize,
    pub receipt_count: usize,
    pub legacy_identity_count: usize,
    pub proposed_identity_count: usize,
    pub reservation_count: usize,
    pub proposed_native_auth_next_nonces: BTreeMap<String, u64>,
    pub proposed_native_auth_nonce_reservations: BTreeMap<String, String>,
    pub legacy_maps_exactly_reconstructed: bool,
    pub transaction_signatures_verified: bool,
    pub receipt_metadata_checked: bool,
    pub snapshot_provenance_verified: bool,
    pub execution_results_verified: bool,
    pub block_history_verified: bool,
    pub qc_verified: bool,
    pub activation_ready: bool,
    pub import_performed: bool,
}

fn legacy_identity_v1(tx: &NovNativeTxWireV1, ir: &TxIR) -> Vec<u8> {
    // Frozen pre-V2 algorithm, intentionally confined to this offline audit.
    if let NovTxKindV1::Execute(execute) = &tx.kind {
        if let Some(account) = execute
            .nonce_owner_account_id
            .as_deref()
            .or(execute.account_id.as_deref())
        {
            let mut identity = b"account:".to_vec();
            identity.extend_from_slice(account.trim().to_ascii_lowercase().as_bytes());
            return identity;
        }
    }
    let mut identity = b"signer:".to_vec();
    identity.extend_from_slice(&ir.from);
    identity
}

fn legacy_keys_v1(chain_id: u64, identity: &[u8], nonce: u64) -> (String, String) {
    // Freeze the old domains too: live helpers may evolve independently.
    let identity_key = to_hex(&sha256_bytes_v1(&[
        b"novovm-native-auth-nonce-identity-v1",
        &chain_id.to_be_bytes(),
        identity,
    ]));
    let ledger_key = to_hex(&sha256_bytes_v1(&[
        b"novovm-native-auth-durable-nonce-key-v1",
        &chain_id.to_be_bytes(),
        identity,
        &nonce.to_be_bytes(),
    ]));
    (identity_key, ledger_key)
}

fn legacy_reservation_id_v1(tx_hash: &[u8; 32], signature: &[u8]) -> String {
    to_hex(&sha256_bytes_v1(&[
        b"novovm-native-auth-nonce-reservation-v1",
        tx_hash,
        signature,
    ]))
}

fn require_canonical_commitment_v1(value: &str, label: &str) -> Result<()> {
    let parsed = parse_fixed_hex_32_v1(value, label)?;
    if to_hex(&parsed) != value {
        bail!("nonce migration {label} must be canonical lowercase 32-byte hex");
    }
    Ok(())
}

fn validate_receipt_metadata_v1(
    receipt: &NovNativeExecutionReceiptV1,
    tx: &NovNativeTxWireV1,
    tx_hash: &str,
) -> Result<()> {
    let NovTxKindV1::Execute(execute) = &tx.kind else {
        bail!("nonce migration supports only execute history");
    };
    let subject = subject_meta_from_execute_tx_v1(execute);
    let request = nov_native_tx_to_execution_request_v1(tx)?
        .context("nonce migration history has no execution request")?;
    let policy = effective_execution_policy_for_fee_asset_v1(
        execute.execution_policy,
        execute.fee_policy.pay_asset.as_str(),
    );
    if receipt.tx_hash != tx_hash
        || receipt.account_id != subject.account_id
        || receipt.fee_owner_account_id != subject.fee_owner_account_id
        || receipt.nonce_owner_account_id != subject.nonce_owner_account_id
        || receipt.target != execution_target_label_v1(&request.target)
        || receipt.key_algo != UcaKeyAlgo::Ed25519.as_str()
        || receipt.execution_policy != policy.as_str()
        || receipt.module.trim().is_empty()
        || receipt.method.trim().is_empty()
    {
        bail!("nonce migration receipt identity or execution metadata mismatch for {tx_hash}");
    }
    let expected_module = match &request.target {
        NovExecutionRequestTargetV1::NativeModule(module) => module.trim().to_ascii_lowercase(),
        _ => "unsupported".to_string(),
    };
    let failed_before_dispatch = !receipt.status
        && matches!(
            (receipt.module.as_str(), receipt.method.as_str()),
            ("execution_policy", "enforce")
                | ("fee", "quote" | "settlement")
                | ("aoem", "semantic_ingress")
        );
    if !failed_before_dispatch
        && (receipt.module != expected_module || receipt.method != request.method)
    {
        bail!("nonce migration receipt execution phase mismatch for {tx_hash}");
    }
    if receipt.status {
        if receipt.failure_reason.is_some()
            || receipt.policy_rejection_reason.is_some()
            || !receipt.policy_enforced
            || !matches!(request.target, NovExecutionRequestTargetV1::NativeModule(_))
        {
            bail!("nonce migration successful receipt metadata is inconsistent for {tx_hash}");
        }
    } else if receipt
        .failure_reason
        .as_deref()
        .is_none_or(|reason| reason.trim().is_empty())
        || (!receipt.policy_enforced
            && receipt
                .policy_rejection_reason
                .as_deref()
                .is_none_or(|reason| reason.trim().is_empty()))
        || (receipt.policy_enforced && receipt.policy_rejection_reason.is_some())
    {
        bail!("nonce migration failed receipt metadata is incomplete for {tx_hash}");
    }
    Ok(())
}

/// Review a trusted, complete legacy Host store snapshot and its complete,
/// ordered committed Execute history, without any filesystem/runtime mutation.
///
/// The output contains proposed nonce maps only, NOT a new authority snapshot,
/// state root, protocol pin, or import authorization. Receipt execution results
/// and source provenance remain caller-established facts, not verified proofs.
pub fn plan_nonce_migration_v1(
    snapshot_json: &[u8],
    raw_history: &[Vec<u8>],
    expected_chain_id: u64,
    expected_namespace: &str,
    expected_legacy_protocol: &str,
) -> Result<NonceMigrationPlanV1> {
    if snapshot_json.is_empty() || snapshot_json.len() > MAX_SNAPSHOT_BYTES_V1 {
        bail!("nonce migration snapshot exceeds its nonempty 16 MiB bound");
    }
    if raw_history.len() > MAX_HISTORY_TRANSACTIONS_V1 {
        bail!("nonce migration history transaction count exceeds its bound");
    }
    let mut history_bytes = 0usize;
    for raw in raw_history {
        if raw.is_empty() || raw.len() > MAX_TRANSACTION_BYTES_V1 {
            bail!("nonce migration raw transaction exceeds its nonempty 1 MiB bound");
        }
        history_bytes = history_bytes
            .checked_add(raw.len())
            .context("nonce migration history byte count overflow")?;
        if history_bytes > MAX_HISTORY_BYTES_V1 {
            bail!("nonce migration history exceeds its 64 MiB bound");
        }
    }
    if expected_chain_id == 0 {
        bail!("nonce migration expected chain must be nonzero");
    }
    require_canonical_commitment_v1(expected_namespace, "namespace digest")?;
    require_canonical_commitment_v1(expected_legacy_protocol, "legacy protocol commitment")?;
    let store: NovNativeExecutionStoreV1 = serde_json::from_slice(snapshot_json)
        .context("decode complete legacy Host store snapshot for nonce migration")?;
    if store.schema != NOV_NATIVE_EXECUTION_STORE_SCHEMA_V1
        || store.authority_chain_id != Some(expected_chain_id)
        || store.authority_namespace_digest != expected_namespace
        || store.module_state.protocol_config_commitment != expected_legacy_protocol
    {
        bail!("nonce migration snapshot schema, chain, namespace, or legacy protocol mismatch");
    }
    if !store
        .module_state
        .native_auth_nonce_identity_scheme
        .is_empty()
    {
        bail!("nonce migration accepts only the unversioned legacy identity scheme");
    }
    if store.receipts.len() != raw_history.len() {
        bail!("nonce migration requires complete receipt and raw history coverage");
    }

    let mut old_nonces = BTreeMap::<String, u64>::new();
    let mut old_reservations = BTreeMap::<String, String>::new();
    let mut seen_hashes = BTreeSet::new();
    let mut new_reservations = Vec::with_capacity(raw_history.len());
    use sha2::{Digest, Sha256};
    let mut history_hasher = Sha256::new();
    history_hasher.update(b"novovm-native-nonce-migration-ordered-history-v1\0");
    history_hasher.update(expected_chain_id.to_be_bytes());
    history_hasher.update((raw_history.len() as u64).to_be_bytes());
    for (index, raw) in raw_history.iter().enumerate() {
        let tx = decode_nov_native_tx_wire_v1(raw)
            .with_context(|| format!("decode nonce migration history transaction {index}"))?;
        if tx.chain_id != expected_chain_id || !matches!(tx.kind, NovTxKindV1::Execute(_)) {
            bail!("nonce migration transaction {index} chain or kind mismatch");
        }
        let ir = nov_native_tx_to_adapter_tx_ir_v1(&tx)?;
        if tx.signature.len() != 96 || !novovm_adapter_novovm::verify_native_tx_signature_v1(&ir)? {
            bail!("nonce migration transaction {index} signature verification failed");
        }
        // Unlike the ingress verifier, this pure helper does not consult the
        // machine-local chain environment or reserve any runtime nonce.
        verify_nov_native_execute_subject_authority_v1(&serde_json::Value::Null, &tx, &ir)?;
        let tx_hash = tx_hash_array_from_ir_v1(&ir);
        let hash_hex = to_hex(&tx_hash);
        if !seen_hashes.insert(hash_hex.clone()) {
            bail!("nonce migration duplicate committed transaction at index {index}");
        }
        let receipt = store
            .receipts
            .get(&hash_hex)
            .context("nonce migration raw transaction has no exact receipt key")?;
        validate_receipt_metadata_v1(receipt, &tx, &hash_hex)?;
        let nonce = native_tx_nonce_for_auth_v1(&tx);
        let identity = legacy_identity_v1(&tx, &ir);
        let (identity_key, ledger_key) = legacy_keys_v1(expected_chain_id, &identity, nonce);
        let next = old_nonces.entry(identity_key).or_insert(0);
        if nonce != *next {
            bail!("nonce migration legacy history sequence mismatch at transaction {index}");
        }
        *next = nonce
            .checked_add(1)
            .context("nonce migration legacy nonce overflow")?;
        if old_reservations
            .insert(
                ledger_key,
                legacy_reservation_id_v1(&tx_hash, &tx.signature),
            )
            .is_some()
        {
            bail!("nonce migration duplicate legacy reservation at transaction {index}");
        }
        new_reservations.push(nov_native_durable_auth_reservation_v1(&tx, &ir, tx_hash)?);
        history_hasher.update((raw.len() as u64).to_be_bytes());
        history_hasher.update(raw);
    }
    if old_nonces != store.module_state.native_auth_next_nonces
        || old_reservations != store.module_state.native_auth_nonce_reservations
        || !seen_hashes.iter().eq(store.receipts.keys())
    {
        bail!("nonce migration legacy nonce maps or receipt set are not exactly reconstructed");
    }

    let mut proposed_nonces = BTreeMap::<String, u64>::new();
    let mut proposed_reservations = BTreeMap::new();
    for (index, reservation) in new_reservations.iter().enumerate() {
        // Two consumed aliases at the same canonical nonce are invalid history,
        // even if each old alias bucket was individually contiguous.
        if proposed_reservations.contains_key(&reservation.ledger_key) {
            bail!("nonce migration canonical identity historical nonce conflict at transaction {index}; manual history recovery is required");
        }
        let next = proposed_nonces
            .entry(reservation.identity_key.clone())
            .or_insert(0);
        if reservation.nonce != *next {
            bail!("nonce migration canonical history sequence mismatch at transaction {index}");
        }
        *next = reservation
            .nonce
            .checked_add(1)
            .context("nonce migration canonical nonce overflow")?;
        proposed_reservations.insert(
            reservation.ledger_key.clone(),
            reservation.reservation_id.clone(),
        );
    }
    Ok(NonceMigrationPlanV1 {
        schema: "novovm-native-nonce-migration-plan/v1",
        chain_id: expected_chain_id,
        namespace_digest: expected_namespace.to_string(),
        legacy_protocol_config_commitment: expected_legacy_protocol.to_string(),
        legacy_nonce_identity_scheme: "unversioned-account-spelling-or-signer/v1",
        proposed_nonce_identity_scheme: NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2,
        source_snapshot_digest: to_hex(&sha256_bytes_v1(&[
            b"novovm-native-nonce-migration-source-snapshot-v1\0",
            snapshot_json,
        ])),
        ordered_history_commitment: to_hex(&history_hasher.finalize()),
        history_transaction_count: raw_history.len(),
        history_bytes,
        receipt_count: store.receipts.len(),
        legacy_identity_count: old_nonces.len(),
        proposed_identity_count: proposed_nonces.len(),
        reservation_count: proposed_reservations.len(),
        proposed_native_auth_next_nonces: proposed_nonces,
        proposed_native_auth_nonce_reservations: proposed_reservations,
        legacy_maps_exactly_reconstructed: true,
        transaction_signatures_verified: true,
        receipt_metadata_checked: true,
        snapshot_provenance_verified: false,
        execution_results_verified: false,
        block_history_verified: false,
        qc_verified: false,
        activation_ready: false,
        import_performed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_protocol::{NovExecutionModeV1, NovFeePolicyV1, NovVerificationModeV1};

    const CHAIN: u64 = 98_917_901;

    fn signed_tx(nonce: u64, alias: u8, seed: [u8; 32]) -> NovNativeTxWireV1 {
        let caller = novovm_adapter_novovm::address_from_seed_v1(seed);
        let account = to_hex_prefixed_v1(&caller);
        let mut tx = NovNativeTxWireV1 {
            chain_id: CHAIN,
            kind: NovTxKindV1::Execute(novovm_protocol::NovExecuteTxV1 {
                caller,
                account_id: match alias {
                    1 => Some(account[2..].to_string()),
                    2 => None,
                    _ => Some(account.clone()),
                },
                fee_owner_account_id: Some(account.clone()),
                nonce_owner_account_id: match alias {
                    1 => Some(account[2..].to_string()),
                    2 => None,
                    _ => Some(account),
                },
                target: NovExecutionTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: br#"{"asset":"NOV","amount":1}"#.to_vec(),
                execution_mode: NovExecutionModeV1::Standard,
                execution_policy: NovExecutionPolicyV1::Standard,
                privacy_mode: NovPrivacyModeV1::Public,
                verification_mode: NovVerificationModeV1::Standard,
                fee_policy: NovFeePolicyV1 {
                    pay_asset: "NOV".to_string(),
                    max_pay_amount: 100,
                    slippage_bps: 0,
                },
                gas_like_limit: Some(90_000),
                nonce,
            }),
            signature: Vec::new(),
        };
        let ir = nov_native_tx_to_adapter_tx_ir_v1(&tx).unwrap();
        tx.signature = novovm_adapter_novovm::signature_payload_with_seed_v1(&ir, seed);
        tx
    }

    fn fixture(txs: &[NovNativeTxWireV1]) -> (NovNativeExecutionStoreV1, Vec<Vec<u8>>) {
        let mut store = NovNativeExecutionStoreV1 {
            authority_chain_id: Some(CHAIN),
            authority_namespace_digest: "ab".repeat(32),
            ..Default::default()
        };
        store.module_state.native_auth_nonce_identity_scheme.clear();
        store.module_state.protocol_config_commitment = "cd".repeat(32);
        let mut history = Vec::new();
        for tx in txs {
            let ir = nov_native_tx_to_adapter_tx_ir_v1(tx).unwrap();
            let hash = tx_hash_array_from_ir_v1(&ir);
            let nonce = native_tx_nonce_for_auth_v1(tx);
            let (identity, ledger) = legacy_keys_v1(CHAIN, &legacy_identity_v1(tx, &ir), nonce);
            store
                .module_state
                .native_auth_next_nonces
                .insert(identity, nonce.checked_add(1).unwrap());
            store
                .module_state
                .native_auth_nonce_reservations
                .insert(ledger, legacy_reservation_id_v1(&hash, &tx.signature));
            let NovTxKindV1::Execute(execute) = &tx.kind else {
                unreachable!()
            };
            let mut subject = subject_meta_from_execute_tx_v1(execute);
            subject.key_algo = "ed25519".to_string();
            subject.policy_enforced = true;
            let request = nov_native_tx_to_execution_request_v1(tx).unwrap().unwrap();
            let receipt = build_success_native_receipt_v1(
                &request,
                &unresolved_settled_fee_v1(&request),
                &subject,
                "treasury",
                &request.method,
                Vec::new(),
            );
            store.receipts.insert(to_hex(&hash), receipt);
            history.push(novovm_protocol::encode_nov_native_tx_wire_v1(tx).unwrap());
        }
        (store, history)
    }

    fn plan(
        store: &NovNativeExecutionStoreV1,
        history: &[Vec<u8>],
    ) -> Result<NonceMigrationPlanV1> {
        plan_nonce_migration_v1(
            &serde_json::to_vec(store).unwrap(),
            history,
            CHAIN,
            &"ab".repeat(32),
            &"cd".repeat(32),
        )
    }

    #[test]
    fn nonce_migration_complete_history_is_pure_and_not_activation() {
        let (store, history) = fixture(&[
            signed_tx(0, 1, [0x51; 32]),
            signed_tx(1, 1, [0x51; 32]),
            signed_tx(0, 2, [0x52; 32]),
        ]);
        let before = serde_json::to_vec(&store).unwrap();
        let report = plan(&store, &history).unwrap();
        assert_eq!(report, plan(&store, &history).unwrap());
        assert_eq!(before, serde_json::to_vec(&store).unwrap());
        assert_eq!(report.history_transaction_count, 3);
        assert_eq!(report.proposed_identity_count, 2);
        assert_eq!(report.reservation_count, 3);
        assert!(report.transaction_signatures_verified && report.legacy_maps_exactly_reconstructed);
        assert!(
            !report.activation_ready
                && !report.import_performed
                && !report.block_history_verified
                && !report.qc_verified
                && !report.execution_results_verified
                && !report.snapshot_provenance_verified
        );
        let mut failed = store.clone();
        let receipt = failed.receipts.values_mut().next().unwrap();
        receipt.status = false;
        receipt.failure_reason = Some("historical business rejection".to_string());
        assert_eq!(
            plan(&failed, &history)
                .unwrap()
                .proposed_native_auth_next_nonces,
            report.proposed_native_auth_next_nonces
        );
    }

    #[test]
    fn nonce_migration_alias_and_signer_double_spends_are_not_merged() {
        for alias in [1, 2] {
            let (store, history) =
                fixture(&[signed_tx(0, 0, [0x53; 32]), signed_tx(0, alias, [0x53; 32])]);
            assert!(plan(&store, &history)
                .unwrap_err()
                .to_string()
                .contains("historical nonce conflict"));
        }
    }

    #[test]
    fn nonce_migration_requires_exact_old_maps_and_receipt_set() {
        let (store, history) = fixture(&[signed_tx(0, 0, [0x54; 32]), signed_tx(1, 0, [0x54; 32])]);
        assert!(plan(&store, &history[..1]).is_err());
        assert!(plan(&store, &[history[1].clone(), history[0].clone()]).is_err());
        assert!(plan(&store, &[history[0].clone(), history[0].clone()]).is_err());
        let mut changed = store.clone();
        *changed
            .module_state
            .native_auth_next_nonces
            .values_mut()
            .next()
            .unwrap() = 99;
        assert!(plan(&changed, &history).is_err());
        changed = store.clone();
        *changed
            .module_state
            .native_auth_nonce_reservations
            .values_mut()
            .next()
            .unwrap() = "ef".repeat(32);
        assert!(plan(&changed, &history).is_err());
        changed = store.clone();
        changed
            .module_state
            .native_auth_next_nonces
            .insert("ef".repeat(32), 1);
        assert!(plan(&changed, &history).is_err());
        changed = store.clone();
        let (_, receipt) = changed.receipts.pop_first().unwrap();
        changed.receipts.insert("ef".repeat(32), receipt);
        assert!(plan(&changed, &history).is_err());
        changed = store.clone();
        changed
            .receipts
            .values_mut()
            .next()
            .unwrap()
            .key_algo
            .clear();
        assert!(plan(&changed, &history).is_err());
        changed = store.clone();
        changed.receipts.values_mut().next().unwrap().status = false;
        assert!(plan(&changed, &history).is_err());
    }

    #[test]
    fn nonce_migration_rejects_signature_domain_version_and_input_bounds() {
        let (store, history) = fixture(&[signed_tx(0, 0, [0x55; 32])]);
        let mut invalid = signed_tx(0, 0, [0x55; 32]);
        invalid.signature[50] ^= 1;
        assert!(plan(
            &store,
            &[novovm_protocol::encode_nov_native_tx_wire_v1(&invalid).unwrap()]
        )
        .is_err());
        for field in 0..4 {
            let mut changed = store.clone();
            match field {
                0 => changed.authority_chain_id = Some(CHAIN + 1),
                1 => changed.authority_namespace_digest = "ef".repeat(32),
                2 => changed.module_state.protocol_config_commitment = "ef".repeat(32),
                _ => {
                    changed.module_state.native_auth_nonce_identity_scheme =
                        NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2.to_string()
                }
            }
            assert!(plan(&changed, &history).is_err());
        }
        assert!(plan_nonce_migration_v1(
            &vec![b' '; MAX_SNAPSHOT_BYTES_V1 + 1],
            &[],
            CHAIN,
            &"ab".repeat(32),
            &"cd".repeat(32)
        )
        .is_err());
        assert!(plan(&store, &vec![Vec::new(); MAX_HISTORY_TRANSACTIONS_V1 + 1]).is_err());
        assert!(plan(&store, &[vec![0; MAX_TRANSACTION_BYTES_V1 + 1]]).is_err());
        assert!(plan(&store, &[Vec::new()]).is_err());
        let mut snapshot = serde_json::to_value(&store).unwrap();
        snapshot["module_state"]
            .as_object_mut()
            .unwrap()
            .remove("native_auth_nonce_identity_scheme");
        assert!(plan_nonce_migration_v1(
            &serde_json::to_vec(&snapshot).unwrap(),
            &history,
            CHAIN,
            &"ab".repeat(32),
            &"cd".repeat(32)
        )
        .is_ok());
    }

    #[test]
    fn nonce_migration_missing_marker_stays_legacy_in_json_and_shards() {
        let new_store = NovNativeExecutionStoreV1::default();
        assert_eq!(
            new_store.module_state.native_auth_nonce_identity_scheme,
            NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2
        );
        verify_native_nonce_identity_scheme_v2(&new_store).unwrap();

        let mut legacy = new_store;
        legacy
            .module_state
            .native_auth_nonce_identity_scheme
            .clear();
        let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
        let legacy_value: serde_json::Value = serde_json::from_slice(&legacy_bytes).unwrap();
        assert!(legacy_value["module_state"]
            .get("native_auth_nonce_identity_scheme")
            .is_none());
        let decoded: NovNativeExecutionStoreV1 = serde_json::from_slice(&legacy_bytes).unwrap();
        assert!(decoded
            .module_state
            .native_auth_nonce_identity_scheme
            .is_empty());
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), legacy_bytes);
        assert!(verify_native_nonce_identity_scheme_v2(&decoded).is_err());

        let shard =
            native_module_state_shard_value_v1(&legacy.module_state, "native_execution").unwrap();
        let shard_value: serde_json::Value = serde_json::from_slice(&shard).unwrap();
        assert!(shard_value
            .get("native_auth_nonce_identity_scheme")
            .is_none());
        let mut target = NovNativeExecutionModuleStateV1::default();
        assert_eq!(
            target.native_auth_nonce_identity_scheme,
            NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2
        );
        native_apply_module_state_shard_v1(&mut target, "native_execution", &shard).unwrap();
        assert!(
            target.native_auth_nonce_identity_scheme.is_empty(),
            "old shard must clear the fresh-genesis marker"
        );
        assert_eq!(
            native_module_state_shard_value_v1(&target, "native_execution").unwrap(),
            shard
        );
    }

    #[test]
    fn nonce_migration_legacy_and_unknown_markers_reject_before_any_mutation() {
        let tx = signed_tx(0, 0, [0x56; 32]);
        let ir = nov_native_tx_to_adapter_tx_ir_v1(&tx).unwrap();
        let reservation =
            nov_native_durable_auth_reservation_v1(&tx, &ir, tx_hash_array_from_ir_v1(&ir))
                .unwrap();
        let request = nov_native_tx_to_execution_request_v1(&tx).unwrap().unwrap();
        for marker in ["", "future-or-corrupt-scheme/v999"] {
            let mut store = NovNativeExecutionStoreV1::default();
            store.module_state.native_auth_nonce_identity_scheme = marker.to_string();
            // Leave protocol binding empty: a rejected bind must not fill it.
            let before = store.clone();
            assert!(verify_native_nonce_identity_scheme_v2(&store).is_err());
            assert!(bind_native_business_protocol_config_v1(&mut store)
                .unwrap_err()
                .to_string()
                .contains("nonce identity scheme"));
            assert_eq!(store, before);
            assert!(find_nov_native_durable_auth_receipt_v1(&store, &reservation).is_err());
            assert!(check_nov_native_durable_auth_reservation_v1(&store, &reservation).is_err());
            assert!(
                commit_nov_native_durable_auth_reservation_v1(&mut store, &reservation).is_err()
            );
            assert_eq!(store, before);
            for auth_reservation in [None, Some(&reservation)] {
                let mut mirrors = Vec::new();
                let result = dispatch_nov_execution_request_into_loaded_store_v1(
                    &mut store,
                    &request,
                    NovExecutionRequestDispatchContextV1 {
                        mirror_base_path: Path::new(""),
                        subject_meta: None,
                        requested_behavior: None,
                        authenticated_key_algo: Some(UcaKeyAlgo::Ed25519),
                        unified_account_store_path: None,
                        emit_policy_observability: false,
                        durable_auth_reservation: auth_reservation,
                        aoem_semantic_ingress_override: None,
                        mirror_records: Some(&mut mirrors),
                        now_ms: 1,
                    },
                );
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("nonce identity scheme"));
                assert_eq!(store, before);
                assert!(mirrors.is_empty());
            }
        }
    }

    #[test]
    fn nonce_migration_persisted_rocksdb_missing_marker_never_infers_current_scheme() {
        // Root test orchestration pins TEMP/TMP into its isolated audit fixture.
        // No configured native/AOEM persistence paths or live stores are used.
        let fixture_dir = std::env::temp_dir().join(format!(
            "novovm-nonce-migration-marker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        fs::create_dir(&fixture_dir).unwrap();
        for variant in 0..4 {
            let path = fixture_dir.join(format!("legacy-{variant}.rocksdb"));
            let mut store = NovNativeExecutionStoreV1::default();
            if variant != 3 {
                store.module_state.native_auth_nonce_identity_scheme.clear();
            }
            let meta = serde_json::to_vec(&native_rocksdb_snapshot_meta_v1(&store)).unwrap();
            let db = open_nov_native_execution_store_rocksdb_v1(&path).unwrap();
            db.put(
                NOV_NATIVE_EXECUTION_STORE_ROCKSDB_SNAPSHOT_META_CURRENT_V1,
                &meta,
            )
            .unwrap();
            // 0: no module shards. 1: other shards but no native_execution.
            // 2: native_execution without marker. 3: explicit current marker.
            if variant == 1 {
                db.put(
                    NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_TREASURY_V1,
                    b"{}",
                )
                .unwrap();
            }
            let native_shard = if variant >= 2 {
                let raw =
                    native_module_state_shard_value_v1(&store.module_state, "native_execution")
                        .unwrap();
                db.put(
                    NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_NATIVE_EXECUTION_V1,
                    &raw,
                )
                .unwrap();
                Some(raw)
            } else {
                None
            };
            drop(db);
            let loaded = load_nov_native_execution_store_rocksdb_v1(&path).unwrap();
            assert_eq!(
                loaded.module_state.native_auth_nonce_identity_scheme,
                store.module_state.native_auth_nonce_identity_scheme
            );
            assert_eq!(
                verify_native_nonce_identity_scheme_v2(&loaded).is_ok(),
                variant == 3
            );
            let db = open_nov_native_execution_store_rocksdb_v1(&path).unwrap();
            assert_eq!(
                db.get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_SNAPSHOT_META_CURRENT_V1)
                    .unwrap(),
                Some(meta)
            );
            assert_eq!(
                db.get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_NATIVE_EXECUTION_V1)
                    .unwrap(),
                native_shard
            );
        }
    }
}
