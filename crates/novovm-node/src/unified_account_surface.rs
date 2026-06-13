#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use aoem_bindings::mldsa_verify_v1_auto;
use ed25519_dalek::{
    Signature as Ed25519Signature, Verifier as Ed25519Verifier, VerifyingKey as Ed25519VerifyingKey,
};
use k256::ecdsa::{Signature as Secp256k1Signature, VerifyingKey as Secp256k1VerifyingKey};
use novovm_adapter_api::unified_account::{PersonaBinding, UcaAccount};
use novovm_adapter_api::{
    AccountAuditEvent, AccountPolicy, AccountRole, KycPolicyMode, MappedAssetLockProof,
    MappedAssetOperation, MappedAssetOperationKind, MappedAssetRecord, MappedAssetSourceChain,
    MappedAssetStatus, MappedLockProofFormat, NonceScope, PersonaAddress, PersonaType,
    ProtocolKind, RouteDecision, RouteRequest, Type4PolicyMode, UcaKeyAlgo, UcaKeyProofType,
    UcaPrimaryKeyBinding, UnifiedAccountError, UnifiedAccountRouter,
};
use novovm_governance_observability::{append_governance_event_auto, GovernanceEvent};
use rocksdb::{Options as RocksDbOptions, WriteBatch as RocksDbWriteBatch, DB as RocksDb};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sha3::Keccak256;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::tx_ingress::{
    load_nov_native_execution_store_v1, nov_native_execution_store_path_v1,
    save_nov_native_execution_store_v1, NovTreasurySettlementJournalEntryV1,
};

const UNIFIED_ACCOUNT_STORE_ENVELOPE_VERSION_V1: u32 = 1;
const UNIFIED_ACCOUNT_STORE_ENVELOPE_VERSION_V2: u32 = 2;
const UNIFIED_ACCOUNT_STORE_BACKEND_ROCKSDB: &str = "rocksdb";
const UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE: &str = "ua_store_state_v2";
const UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_AUDIT: &str = "ua_store_audit_v2";
const UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_STATE_ROUTER: &[u8] = b"ua_store:state:router:v2";
const UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_STATE_MAPPED_ASSET: &[u8] =
    b"ua_store:state:mapped_asset:v1";
const UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_AUDIT_CURSOR: &[u8] =
    b"ua_store:audit:flushed_event_count:v1";
const UNIFIED_ACCOUNT_AUDIT_BACKEND_JSONL: &str = "jsonl";
const UNIFIED_ACCOUNT_AUDIT_BACKEND_ROCKSDB: &str = "rocksdb";
const UNIFIED_ACCOUNT_AUDIT_LOG_NAME: &str = "ua-account-audit-events.jsonl";
const UNIFIED_ACCOUNT_AUDIT_DB_NAME: &str = "ua-account-audit-events.rocksdb";
const UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_SEQ: &[u8] = b"ua_audit:seq";
const UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_EVENT_PREFIX: &[u8] = b"ua_audit:event:";
const NOVOVM_UA_PHASE4_NOGO_ENFORCE_ENV: &str = "NOVOVM_UA_PHASE4_NOGO_ENFORCE";
const NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV: &str = "NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE";
const NOVOVM_UA_ETH_LOCK_CONTRACT_ADDRESS_ENV: &str = "NOVOVM_UA_ETH_LOCK_CONTRACT_ADDRESS";
const NOVOVM_UA_ETH_LOCK_MIN_CONFIRMATIONS_ENV: &str = "NOVOVM_UA_ETH_LOCK_MIN_CONFIRMATIONS";
const ETH_LOCK_EVENT_SIGNATURE_V1: &str = "Locked(address,bytes32,uint256,string)";

#[derive(Debug)]
struct UnifiedAccountStoreSnapshot {
    router: UnifiedAccountRouter,
    flushed_event_count: u64,
    mapped_asset_state: UnifiedMappedAssetState,
}

#[derive(Debug, Serialize, Deserialize)]
struct UnifiedAccountStoreEnvelopeV1 {
    version: u32,
    router: UnifiedAccountRouter,
    flushed_event_count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct UnifiedAccountStoreEnvelopeV2 {
    version: u32,
    router: UnifiedAccountRouter,
    flushed_event_count: u64,
    mapped_asset_state: UnifiedMappedAssetState,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct UnifiedMappedAssetState {
    records_by_mapping_id: BTreeMap<String, MappedAssetRecord>,
    mapping_id_by_lock_id: BTreeMap<String, String>,
    operations: Vec<MappedAssetOperation>,
}

#[derive(Debug, Clone)]
enum UnifiedAccountStoreBackend {
    BincodeFile { path: PathBuf },
    RocksDb { path: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UnifiedAccountAuditSinkRecord {
    at: u64,
    source: String,
    method: String,
    success: bool,
    router_changed: bool,
    event_cursor_from: u64,
    event_cursor_to: u64,
    router_events: Vec<AccountAuditEvent>,
    params: Value,
    error: Option<String>,
}

#[derive(Debug, Clone)]
enum UnifiedAccountAuditSinkBackend {
    JsonlFile { path: PathBuf },
    RocksDb { path: PathBuf },
}

pub fn is_mainline_unified_account_query_method(method: &str) -> bool {
    matches!(
        method,
        "ua_createUca"
            | "ua_rotatePrimaryKey"
            | "ua_setPolicy"
            | "ua_bindPersona"
            | "ua_revokePersona"
            | "ua_getBindingOwner"
            | "ua_getAuditEvents"
            | "ua_getAccount"
            | "ua_getPolicy"
            | "ua_listBindings"
            | "ua_getNextNonce"
            | "ua_checkRoute"
            | "ua_route"
            | "ua_registerMappedLock"
            | "ua_getMappedAsset"
            | "ua_burnMappedAsset"
            | "ua_releaseMappedLock"
            | "account_balance"
            | "account_assets"
    )
}

pub fn default_mainline_unified_account_store_path(query_store_path: &Path) -> PathBuf {
    if let Some(custom) = string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_DB") {
        return PathBuf::from(custom);
    }
    unified_account_store_path_for_backend(query_store_path, UNIFIED_ACCOUNT_STORE_BACKEND_ROCKSDB)
}

pub fn get_unified_account_key_algo_with_store_path_v1(
    store_path: &Path,
    account_id: &str,
) -> Result<Option<UcaKeyAlgo>> {
    let backend = string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_STORE_BACKEND")
        .unwrap_or_else(|| UNIFIED_ACCOUNT_STORE_BACKEND_ROCKSDB.to_string())
        .trim()
        .to_ascii_lowercase();
    let store = match backend.as_str() {
        "rocksdb" => UnifiedAccountStoreBackend::RocksDb {
            path: store_path.to_path_buf(),
        },
        "bincode_file" | "file" | "bincode" => {
            if bool_env_default("NOVOVM_ALLOW_NON_PROD_UA_BACKEND", false) {
                UnifiedAccountStoreBackend::BincodeFile {
                    path: store_path.to_path_buf(),
                }
            } else {
                bail!(
                    "NOVOVM_UNIFIED_ACCOUNT_STORE_BACKEND={} is non-production; use rocksdb or set NOVOVM_ALLOW_NON_PROD_UA_BACKEND=1 for explicit override",
                    backend
                )
            }
        }
        _ => bail!(
            "invalid NOVOVM_UNIFIED_ACCOUNT_STORE_BACKEND={}; valid: rocksdb|bincode_file|file|bincode",
            backend
        ),
    };
    let snapshot = store.load_snapshot()?;
    match snapshot.router.get_account(account_id) {
        Ok(account) => Ok(account.primary_key_binding.map(|binding| binding.key_algo)),
        Err(UnifiedAccountError::UcaNotFound { .. }) => Ok(None),
        Err(err) => Err(anyhow::anyhow!(
            "resolve unified account key algo failed for {}: {}",
            account_id,
            err
        )),
    }
}

#[cfg(test)]
pub(crate) fn seed_unified_account_key_algo_for_tests_v1(
    store_path: &Path,
    account_id: &str,
    key_algo: UcaKeyAlgo,
    now: u64,
) -> Result<()> {
    let store = UnifiedAccountStoreBackend::RocksDb {
        path: store_path.to_path_buf(),
    };
    let mut snapshot = store.load_snapshot()?;
    let public_key = match key_algo {
        UcaKeyAlgo::Secp256k1 => vec![0x02; 33],
        UcaKeyAlgo::Ed25519 => vec![0x11; 32],
        UcaKeyAlgo::Mldsa87 => vec![0x87; 32],
    };
    snapshot.router.create_uca_with_primary_key_binding(
        account_id.to_string(),
        vec![0x42; 32],
        Some(UcaPrimaryKeyBinding {
            key_algo,
            public_key,
            proof_type: UcaKeyProofType::SignatureV1,
            proof_payload: vec![0x99; 64],
            verified_at: now,
        }),
        now,
    )?;
    snapshot.flushed_event_count = snapshot.router.events().len() as u64;
    store.save_snapshot(&snapshot)?;
    Ok(())
}

pub fn run_mainline_unified_account_query(
    query_store_path: &Path,
    method: &str,
    params: &Value,
) -> Result<Value> {
    let store = resolve_unified_account_store(query_store_path, params)?;
    let audit_sink = resolve_unified_account_audit_sink(query_store_path, params)?;
    let mut snapshot = store.load_snapshot()?;
    let before_event_count = snapshot.router.events().len() as u64;
    let before_flushed_event_count = snapshot.flushed_event_count;
    let rpc_result = run_unified_account_surface_rpc(
        &mut snapshot.router,
        &mut snapshot.mapped_asset_state,
        &audit_sink,
        method,
        params,
    );
    let after_event_count = snapshot.router.events().len() as u64;
    let mut router_changed = match &rpc_result {
        Ok((_, changed)) => *changed,
        Err(_) => false,
    };
    if after_event_count != before_event_count {
        router_changed = true;
    }

    let (router_events, next_cursor) =
        unified_account_events_since(&snapshot.router, snapshot.flushed_event_count);
    let audit_record = UnifiedAccountAuditSinkRecord {
        at: now_unix_sec(),
        source: "mainline_query".to_string(),
        method: method.to_string(),
        success: rpc_result.is_ok(),
        router_changed,
        event_cursor_from: snapshot.flushed_event_count,
        event_cursor_to: next_cursor,
        router_events,
        params: params.clone(),
        error: rpc_result.as_ref().err().map(|err| err.to_string()),
    };
    audit_sink.append_record(&audit_record)?;
    snapshot.flushed_event_count = next_cursor;

    if router_changed || snapshot.flushed_event_count != before_flushed_event_count {
        store.save_snapshot(&snapshot)?;
    }

    rpc_result.map(|(value, _)| value)
}

fn run_unified_account_surface_rpc(
    router: &mut UnifiedAccountRouter,
    mapped_asset_state: &mut UnifiedMappedAssetState,
    audit_sink: &UnifiedAccountAuditSinkBackend,
    method: &str,
    params: &Value,
) -> Result<(Value, bool)> {
    match method {
        "ua_createUca" => {
            let account_id = parse_account_id(params)?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let (primary_key_ref, primary_key_binding) =
                resolve_primary_key_binding_v1(params, &account_id, "create", now, None)?;
            let key_algo = primary_key_binding
                .as_ref()
                .map(|binding| binding.key_algo.as_str());
            router.create_uca_with_primary_key_binding(
                account_id.clone(),
                primary_key_ref,
                primary_key_binding,
                now,
            )?;
            Ok((
                json!({
                    "method": method,
                    "created": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "key_algo": key_algo,
                }),
                true,
            ))
        }
        "ua_rotatePrimaryKey" => {
            let account_id = parse_account_id(params)?;
            let role = parse_account_role(params)?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let (next_primary_key_ref, next_primary_key_binding) = resolve_primary_key_binding_v1(
                params,
                &account_id,
                "rotate",
                now,
                Some("next_primary_key_ref"),
            )?;
            let key_algo = next_primary_key_binding
                .as_ref()
                .map(|binding| binding.key_algo.as_str());
            router.rotate_primary_key_with_binding(
                &account_id,
                role,
                next_primary_key_ref,
                next_primary_key_binding,
                now,
            )?;
            Ok((
                json!({
                    "method": method,
                    "rotated": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "key_algo": key_algo,
                }),
                true,
            ))
        }
        "ua_setPolicy" => {
            let account_id = parse_account_id(params)?;
            let role = parse_account_role(params)?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let nonce_scope = parse_nonce_scope(params)?;
            let allow_type4_with_delegate_or_session =
                param_as_bool(params, "allow_type4_with_delegate_or_session").unwrap_or(false);
            let type4_policy_mode =
                parse_type4_policy_mode(params, allow_type4_with_delegate_or_session)?;
            let kyc_policy_mode = parse_kyc_policy_mode(params)?;
            let policy = AccountPolicy {
                nonce_scope,
                type4_policy_mode,
                allow_type4_with_delegate_or_session,
                kyc_policy_mode,
            };
            router.update_policy(&account_id, role, policy, now)?;
            Ok((
                json!({
                    "method": method,
                    "updated": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "nonce_scope": nonce_scope_label(nonce_scope),
                    "type4_policy_mode": type4_policy_mode_label(type4_policy_mode),
                    "kyc_policy_mode": kyc_policy_mode_label(kyc_policy_mode),
                    "allow_type4_with_delegate_or_session": allow_type4_with_delegate_or_session,
                }),
                true,
            ))
        }
        "ua_bindPersona" => {
            let account_id = parse_account_id(params)?;
            let role = parse_account_role(params)?;
            let persona = parse_persona(params, false)?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            router.add_binding(&account_id, role, persona.clone(), now)?;
            Ok((
                json!({
                    "method": method,
                    "bound": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "persona_type": persona.persona_type.as_str(),
                    "chain_id": persona.chain_id,
                }),
                true,
            ))
        }
        "ua_revokePersona" => {
            let account_id = parse_account_id(params)?;
            let role = parse_account_role(params)?;
            let persona = parse_persona(params, false)?;
            let cooldown_seconds = param_as_u64(params, "cooldown_seconds").unwrap_or(0);
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            router.revoke_binding(&account_id, role, persona.clone(), cooldown_seconds, now)?;
            Ok((
                json!({
                    "method": method,
                    "revoked": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "persona_type": persona.persona_type.as_str(),
                    "chain_id": persona.chain_id,
                    "cooldown_seconds": cooldown_seconds,
                }),
                true,
            ))
        }
        "ua_getBindingOwner" => {
            let persona = parse_persona(params, false)?;
            let owner = router.resolve_binding_owner(&persona).map(str::to_string);
            Ok((
                json!({
                    "method": method,
                    "found": owner.is_some(),
                    "owner_account_id": owner,
                    "owner_uca_id": owner,
                    "persona_type": persona.persona_type.as_str(),
                    "chain_id": persona.chain_id,
                }),
                false,
            ))
        }
        "ua_getAccount" => {
            let account_id = parse_account_id(params)?;
            let account = router.get_account(&account_id)?;
            let policy = router.get_policy(&account_id)?;
            let bindings = router.list_bindings(&account_id)?;
            Ok((
                json!({
                    "method": method,
                    "found": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "account": uca_account_to_json(&account),
                    "policy": account_policy_to_json(&policy),
                    "binding_count": bindings.len(),
                }),
                false,
            ))
        }
        "ua_getPolicy" => {
            let account_id = parse_account_id(params)?;
            let policy = router.get_policy(&account_id)?;
            Ok((
                json!({
                    "method": method,
                    "found": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "policy": account_policy_to_json(&policy),
                }),
                false,
            ))
        }
        "ua_listBindings" => {
            let account_id = parse_account_id(params)?;
            let bindings = router.list_bindings(&account_id)?;
            let bindings_json = bindings
                .iter()
                .map(persona_binding_to_json)
                .collect::<Result<Vec<_>>>()?;
            Ok((
                json!({
                    "method": method,
                    "found": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "count": bindings_json.len(),
                    "bindings": bindings_json,
                }),
                false,
            ))
        }
        "ua_getNextNonce" => {
            let account_id = parse_account_id(params)?;
            let persona = parse_persona(params, false)?;
            let nonce = router.next_nonce_for_persona(&account_id, &persona)?;
            Ok((
                json!({
                    "method": method,
                    "found": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "persona_type": persona.persona_type.as_str(),
                    "chain_id": persona.chain_id,
                    "nonce": nonce,
                    "nonce_hex": format!("0x{nonce:x}"),
                }),
                false,
            ))
        }
        "ua_checkRoute" => {
            let account_id = parse_account_id(params)?;
            let role = parse_account_role(params)?;
            let protocol = parse_protocol_kind(params)?;
            let persona = parse_persona(params, true)?;
            let signature_domain = param_as_string_any(params, &["signature_domain"])
                .unwrap_or_else(|| default_signature_domain(&persona, &protocol));
            let nonce = match param_as_u64(params, "nonce") {
                Some(nonce) => nonce,
                None => router.next_nonce_for_persona(&account_id, &persona)?,
            };
            let wants_cross_chain_atomic =
                param_as_bool(params, "wants_cross_chain_atomic").unwrap_or(false);
            let tx_type4 = param_as_bool(params, "tx_type4").unwrap_or(false);
            let session_expires_at = param_as_u64(params, "session_expires_at");
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let kyc_attestation_provided =
                param_as_bool(params, "kyc_attestation_provided").unwrap_or(false);
            let kyc_verified = param_as_bool(params, "kyc_verified").unwrap_or(false);
            let mut probe = router.clone();
            let decision = probe.route(RouteRequest {
                uca_id: account_id.clone(),
                persona,
                role,
                protocol,
                signature_domain: signature_domain.clone(),
                nonce,
                kyc_attestation_provided,
                kyc_verified,
                wants_cross_chain_atomic,
                tx_type4,
                session_expires_at,
                now,
            })?;
            Ok((
                json!({
                    "method": method,
                    "accepted": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "decision": route_decision_to_json(&decision),
                    "signature_domain": signature_domain,
                    "nonce": nonce,
                    "tx_type4": tx_type4,
                    "wants_cross_chain_atomic": wants_cross_chain_atomic,
                    "kyc_attestation_provided": kyc_attestation_provided,
                    "kyc_verified": kyc_verified,
                    "session_expires_at": session_expires_at,
                    "read_only": true,
                }),
                false,
            ))
        }
        "ua_registerMappedLock" => {
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let proof = parse_mapped_asset_lock_proof(params)?;
            let phase4_mode = parse_phase4_mode_v1(params)?;
            let shadow_mode = is_shadow_phase4_mode_v1(&phase4_mode);
            let inflow_channel = if shadow_mode {
                "mapped_lock_register_shadow"
            } else {
                "mapped_lock_register"
            };
            let register_operation = if shadow_mode {
                "shadow_register_lock"
            } else {
                "register_lock"
            };
            emit_external_inflow_demand_observed_v1(
                inflow_channel,
                false,
                false,
                Some(proof.target_account_id.as_str()),
                Some(proof.source_chain.as_str()),
                Some(proof.amount),
                Some("raw_attempt"),
            );
            let account_id = validate_uca_id_policy(&proof.target_account_id)?;
            if let Err(err) = router.get_account(&account_id) {
                emit_mapped_asset_operation_observed_v1(
                    register_operation,
                    false,
                    Some(account_id.as_str()),
                    None,
                    Some("ERR_MAPPED_LOCK_ACCOUNT_NOT_FOUND"),
                    Some("unqualified"),
                );
                emit_external_inflow_demand_observed_v1(
                    inflow_channel,
                    false,
                    false,
                    Some(account_id.as_str()),
                    Some(proof.source_chain.as_str()),
                    Some(proof.amount),
                    Some("ERR_MAPPED_LOCK_ACCOUNT_NOT_FOUND"),
                );
                match err {
                    UnifiedAccountError::UcaNotFound { .. } => {
                        bail!(
                            "ERR_MAPPED_LOCK_ACCOUNT_NOT_FOUND: target account not found: {}",
                            account_id
                        );
                    }
                    other => {
                        bail!(
                            "ERR_MAPPED_LOCK_ACCOUNT_NOT_FOUND: resolve target account failed: {}",
                            other
                        );
                    }
                }
            }
            verify_mapped_lock_proof(&proof, params, !shadow_mode)?;
            let lock_key = mapped_asset_hex_id(&proof.lock_id);
            if mapped_asset_state
                .mapping_id_by_lock_id
                .contains_key(&lock_key)
            {
                emit_mapped_asset_operation_observed_v1(
                    register_operation,
                    false,
                    Some(account_id.as_str()),
                    None,
                    Some("ERR_MAPPED_LOCK_ALREADY_REGISTERED"),
                    Some("qualified"),
                );
                emit_external_inflow_demand_observed_v1(
                    inflow_channel,
                    true,
                    false,
                    Some(account_id.as_str()),
                    Some(proof.source_chain.as_str()),
                    Some(proof.amount),
                    Some("ERR_MAPPED_LOCK_ALREADY_REGISTERED"),
                );
                bail!(
                    "ERR_MAPPED_LOCK_ALREADY_REGISTERED: lock_id already registered: {}",
                    lock_key
                );
            }
            if phase4_shadow_mode_enforced_v1() && !shadow_mode {
                let lock_id_hex = mapped_asset_hex_id(&proof.lock_id);
                emit_mapped_asset_operation_observed_v1(
                    register_operation,
                    false,
                    Some(account_id.as_str()),
                    None,
                    Some("phase4_shadow_mode_required"),
                    Some("qualified"),
                );
                emit_external_inflow_demand_observed_v1(
                    inflow_channel,
                    true,
                    false,
                    Some(account_id.as_str()),
                    Some(proof.source_chain.as_str()),
                    Some(proof.amount),
                    Some("phase4_shadow_mode_required"),
                );
                emit_governance_event_best_effort_v1(GovernanceEvent::Phase4Blocked {
                    reason: "phase4_shadow_mode_required".to_string(),
                    context: format!(
                        "method=ua_registerMappedLock account_id={} lock_id={} phase4_mode={}",
                        proof.target_account_id, lock_id_hex, phase4_mode
                    ),
                    block_kind: Some("rule".to_string()),
                    demand_quality: Some("qualified".to_string()),
                });
                bail!(
                    "ERR_PHASE4_SHADOW_MODE_REQUIRED: ua_registerMappedLock requires phase4_mode=shadow while {} is enabled",
                    NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV
                );
            }
            if phase4_nogo_enforced_v1() && !shadow_mode {
                let lock_id_hex = mapped_asset_hex_id(&proof.lock_id);
                emit_mapped_asset_operation_observed_v1(
                    register_operation,
                    false,
                    Some(account_id.as_str()),
                    None,
                    Some("phase4_nogo_enforced"),
                    Some("qualified"),
                );
                emit_external_inflow_demand_observed_v1(
                    inflow_channel,
                    true,
                    false,
                    Some(account_id.as_str()),
                    Some(proof.source_chain.as_str()),
                    Some(proof.amount),
                    Some("phase4_nogo_enforced"),
                );
                emit_governance_event_best_effort_v1(GovernanceEvent::Phase4Blocked {
                    reason: "phase4_nogo_enforced".to_string(),
                    context: format!(
                        "method=ua_registerMappedLock account_id={} lock_id={}",
                        proof.target_account_id, lock_id_hex
                    ),
                    block_kind: Some("rule".to_string()),
                    demand_quality: Some("qualified".to_string()),
                });
                emit_governance_event_best_effort_v1(GovernanceEvent::TriggerEvaluated {
                    trigger_type: "Phase4".to_string(),
                    satisfied: false,
                    evidence_summary: "mapped asset register blocked by Phase4 No-Go policy"
                        .to_string(),
                    evidence_score: Some(0.0),
                });
                bail!(
                    "ERR_PHASE4_NOGO_BLOCKED: ua_registerMappedLock blocked by {}",
                    NOVOVM_UA_PHASE4_NOGO_ENFORCE_ENV
                );
            }
            let mapping_id = derive_mapping_id_from_lock_proof_v1(&proof);
            let mapping_key = mapped_asset_hex_id(&mapping_id);
            let record = MappedAssetRecord {
                mapping_id,
                lock_id: proof.lock_id,
                source_chain: proof.source_chain.clone(),
                source_asset_symbol: normalize_asset_view_symbol_v1(&proof.source_asset_symbol),
                source_tx_hash: proof.source_tx_hash.clone(),
                source_lock_ref: proof.source_lock_ref.clone(),
                external_owner_ref: proof.external_owner_ref.clone(),
                target_asset_symbol: "NETH".to_string(),
                target_account_id: account_id.clone(),
                amount: proof.amount,
                phase4_mode: phase4_mode.clone(),
                status: MappedAssetStatus::Active,
                created_at: now,
                updated_at: now,
                audit_ref: derive_mapped_asset_audit_ref_v1(
                    mapping_id,
                    MappedAssetStatus::Active,
                    now,
                ),
            };
            mapped_asset_state
                .mapping_id_by_lock_id
                .insert(lock_key, mapping_key.clone());
            mapped_asset_state
                .records_by_mapping_id
                .insert(mapping_key.clone(), record.clone());
            let op = build_mapped_asset_operation_v1(
                &record,
                MappedAssetOperationKind::RegisterLock,
                now,
                mapped_asset_state.operations.len() as u64 + 1,
            );
            mapped_asset_state.operations.push(op);
            let settlement = apply_live_mapped_lock_m2_credit_v1(
                &record,
                mapping_key.as_str(),
                proof.source_tx_hash.as_slice(),
                params,
                now,
            )?;
            emit_mapped_asset_operation_observed_v1(
                register_operation,
                true,
                Some(account_id.as_str()),
                Some(mapping_key.as_str()),
                None,
                Some("qualified"),
            );
            emit_external_inflow_demand_observed_v1(
                inflow_channel,
                true,
                true,
                Some(account_id.as_str()),
                Some(proof.source_chain.as_str()),
                Some(proof.amount),
                None,
            );
            Ok((
                json!({
                    "method": method,
                    "accepted": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "mapping_id": mapped_asset_hex_id(&record.mapping_id),
                    "lock_id": mapped_asset_hex_id(&record.lock_id),
                    "source_chain": record.source_chain.as_str(),
                    "source_asset_symbol": record.source_asset_symbol,
                    "target_asset_symbol": record.target_asset_symbol,
                    "amount": record.amount,
                    "status": record.status.as_str(),
                    "phase4_mode": mapped_asset_phase4_mode_v1(&record),
                    "settlement_effect": mapped_asset_settlement_effect_for_record_v1(&record),
                    "native_settlement": settlement,
                    "source_tx_hash": format!("0x{}", to_hex_lower(&proof.source_tx_hash)),
                    "proof_format": proof.proof_format.as_str(),
                }),
                true,
            ))
        }
        "ua_getMappedAsset" => {
            let mapping_key = resolve_mapped_asset_lookup_key(params, mapped_asset_state)?;
            let record = mapped_asset_state
                .records_by_mapping_id
                .get(mapping_key.as_str())
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "ERR_MAPPED_ASSET_NOT_FOUND: mapped asset not found: {}",
                        mapping_key
                    )
                })?;
            if let Some(request_account_raw) =
                param_as_string_any(params, &["account_id", "uca_id"])
            {
                let request_account_id = validate_uca_id_policy(&request_account_raw)?;
                if request_account_id != record.target_account_id {
                    bail!(
                        "ERR_MAPPED_ASSET_NOT_OWNED_BY_ACCOUNT: account {} does not own {}",
                        request_account_id,
                        mapping_key
                    );
                }
            }
            let operations = mapped_asset_state
                .operations
                .iter()
                .filter(|entry| entry.mapping_id == record.mapping_id)
                .map(mapped_asset_operation_to_json)
                .collect::<Vec<_>>();
            let record_account_id = record.target_account_id.clone();
            let record_uca_id = record_account_id.clone();
            Ok((
                json!({
                    "method": method,
                    "found": true,
                    "account_id": record_account_id,
                    "uca_id": record_uca_id,
                    "mapped_asset": mapped_asset_record_to_json(&record),
                    "operation_count": operations.len(),
                    "operations": operations,
                }),
                false,
            ))
        }
        "ua_burnMappedAsset" => {
            let account_id = parse_account_id(params)?;
            let mapping_key = resolve_mapped_asset_lookup_key(params, mapped_asset_state)?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let record = mapped_asset_state
                .records_by_mapping_id
                .get_mut(mapping_key.as_str())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "ERR_MAPPED_ASSET_NOT_FOUND: mapped asset not found: {}",
                        mapping_key
                    )
                })?;
            if record.target_account_id != account_id {
                bail!(
                    "ERR_MAPPED_ASSET_NOT_OWNED_BY_ACCOUNT: account {} does not own {}",
                    account_id,
                    mapping_key
                );
            }
            if record.status != MappedAssetStatus::Active {
                bail!(
                    "ERR_MAPPED_ASSET_STATUS_INVALID: expected active, got {}",
                    record.status.as_str()
                );
            }
            let settlement =
                apply_live_mapped_asset_m2_burn_v1(record, mapping_key.as_str(), params, now)?;
            record.status = MappedAssetStatus::BurnPending;
            record.updated_at = now;
            record.audit_ref =
                derive_mapped_asset_audit_ref_v1(record.mapping_id, record.status, now);
            let op = build_mapped_asset_operation_v1(
                record,
                MappedAssetOperationKind::BurnMapped,
                now,
                mapped_asset_state.operations.len() as u64 + 1,
            );
            mapped_asset_state.operations.push(op);
            let burn_operation = if mapped_asset_is_shadow_mode_v1(record) {
                "shadow_burn_mapped"
            } else {
                "burn_mapped"
            };
            emit_mapped_asset_operation_observed_v1(
                burn_operation,
                true,
                Some(account_id.as_str()),
                Some(mapping_key.as_str()),
                None,
                Some("qualified"),
            );
            Ok((
                json!({
                    "method": method,
                    "burned": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "mapping_id": mapping_key,
                    "status": record.status.as_str(),
                    "phase4_mode": mapped_asset_phase4_mode_v1(record),
                    "settlement_effect": mapped_asset_settlement_effect_for_record_v1(record),
                    "native_settlement": settlement,
                }),
                true,
            ))
        }
        "ua_releaseMappedLock" => {
            let account_id = parse_account_id(params)?;
            let mapping_key = resolve_mapped_asset_lookup_key(params, mapped_asset_state)?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let record = mapped_asset_state
                .records_by_mapping_id
                .get_mut(mapping_key.as_str())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "ERR_MAPPED_ASSET_NOT_FOUND: mapped asset not found: {}",
                        mapping_key
                    )
                })?;
            if record.target_account_id != account_id {
                bail!(
                    "ERR_MAPPED_ASSET_NOT_OWNED_BY_ACCOUNT: account {} does not own {}",
                    account_id,
                    mapping_key
                );
            }
            if record.status != MappedAssetStatus::BurnPending {
                bail!(
                    "ERR_MAPPED_RELEASE_REQUIRES_BURN: status must be burn_pending, got {}",
                    record.status.as_str()
                );
            }
            let settlement = apply_live_mapped_lock_source_release_v1(
                record,
                mapping_key.as_str(),
                params,
                now,
            )?;
            record.status = MappedAssetStatus::Released;
            record.updated_at = now;
            record.audit_ref =
                derive_mapped_asset_audit_ref_v1(record.mapping_id, record.status, now);
            let op = build_mapped_asset_operation_v1(
                record,
                MappedAssetOperationKind::ReleaseSource,
                now,
                mapped_asset_state.operations.len() as u64 + 1,
            );
            mapped_asset_state.operations.push(op);
            let release_operation = if mapped_asset_is_shadow_mode_v1(record) {
                "shadow_release_source"
            } else {
                "release_source"
            };
            emit_mapped_asset_operation_observed_v1(
                release_operation,
                true,
                Some(account_id.as_str()),
                Some(mapping_key.as_str()),
                None,
                Some("qualified"),
            );
            Ok((
                json!({
                    "method": method,
                    "released": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "mapping_id": mapping_key,
                    "status": record.status.as_str(),
                    "phase4_mode": mapped_asset_phase4_mode_v1(record),
                    "settlement_effect": mapped_asset_settlement_effect_for_record_v1(record),
                    "native_settlement": settlement,
                }),
                true,
            ))
        }
        "account_balance" => {
            let account_id = parse_account_id(params)?;
            let asset_id = normalize_asset_view_symbol_v1(
                &param_as_string_any(params, &["asset_id", "asset"])
                    .unwrap_or_else(|| "NOV".to_string()),
            );
            let store_path = native_execution_store_path_from_params_or_env_v1(params);
            let store = load_nov_native_execution_store_v1(store_path.as_path())?;
            let normalized_account = normalize_account_view_key_v1(&account_id);
            let balance = store
                .module_state
                .account_asset_balances
                .get(normalized_account.as_str())
                .and_then(|assets| assets.get(asset_id.as_str()).copied())
                .unwrap_or(0);
            let mapped_asset_active_balance = mapped_asset_state
                .records_by_mapping_id
                .values()
                .filter(|record| {
                    normalize_account_view_key_v1(&record.target_account_id) == normalized_account
                        && record.status == MappedAssetStatus::Active
                        && normalize_asset_view_symbol_v1(&record.target_asset_symbol) == asset_id
                })
                .fold(0u128, |acc, record| acc.saturating_add(record.amount));
            let mapped_asset_shadow_active_balance = mapped_asset_state
                .records_by_mapping_id
                .values()
                .filter(|record| {
                    normalize_account_view_key_v1(&record.target_account_id) == normalized_account
                        && record.status == MappedAssetStatus::Active
                        && normalize_asset_view_symbol_v1(&record.target_asset_symbol) == asset_id
                        && mapped_asset_is_shadow_mode_v1(record)
                })
                .fold(0u128, |acc, record| acc.saturating_add(record.amount));
            let mut locked_collateral = 0u128;
            let mut locked_collateral_positions = 0usize;
            let mut debt_outstanding = 0u128;
            let mut debt_positions = 0usize;
            for vault in
                store.module_state.credit_vaults.values().filter(|vault| {
                    normalize_account_view_key_v1(&vault.owner) == normalized_account
                })
            {
                if normalize_asset_view_symbol_v1(&vault.collateral_asset) == asset_id {
                    locked_collateral = locked_collateral.saturating_add(vault.collateral_amount);
                    locked_collateral_positions = locked_collateral_positions.saturating_add(1);
                }
                if normalize_asset_view_symbol_v1(&vault.debt_asset) == asset_id {
                    debt_outstanding = debt_outstanding.saturating_add(vault.debt_amount);
                    debt_positions = debt_positions.saturating_add(1);
                }
            }
            let mut treasury_source_flow = 0u128;
            let mut treasury_source_flow_entries = 0usize;
            let mut treasury_settled_nov = 0u128;
            let mut treasury_nov_entries = 0usize;
            let mut treasury_reserve_bucket_exposure_nov = 0i128;
            let mut treasury_fee_bucket_exposure_nov = 0i128;
            let mut treasury_risk_buffer_exposure_nov = 0i128;
            for entry in &store.module_state.treasury_settlement_journal {
                if normalize_account_view_key_v1(&entry.account_id) != normalized_account {
                    continue;
                }
                let source_asset = normalize_asset_view_symbol_v1(&entry.source_asset);
                if source_asset == asset_id {
                    treasury_source_flow = treasury_source_flow.saturating_add(entry.source_amount);
                    treasury_source_flow_entries = treasury_source_flow_entries.saturating_add(1);
                }
                if asset_id == "NOV" {
                    treasury_settled_nov = treasury_settled_nov.saturating_add(entry.settled_nov);
                    treasury_nov_entries = treasury_nov_entries.saturating_add(1);
                    treasury_reserve_bucket_exposure_nov = treasury_reserve_bucket_exposure_nov
                        .saturating_add(entry.reserve_bucket_delta_nov);
                    treasury_fee_bucket_exposure_nov =
                        treasury_fee_bucket_exposure_nov.saturating_add(entry.fee_bucket_delta_nov);
                    treasury_risk_buffer_exposure_nov = treasury_risk_buffer_exposure_nov
                        .saturating_add(entry.risk_buffer_delta_nov);
                }
            }
            let mut components = Vec::new();
            if balance > 0 {
                components.push(json!({
                    "classification": "liquid_balance",
                    "asset_id": asset_id,
                    "asset": asset_id,
                    "amount": balance,
                    "source": "native_execution_store.account_asset_balances",
                }));
            }
            if locked_collateral > 0 {
                components.push(json!({
                    "classification": "pledge_locked_collateral",
                    "asset_id": asset_id,
                    "asset": asset_id,
                    "amount": locked_collateral,
                    "position_count": locked_collateral_positions,
                    "source": "native_execution_store.credit_vaults",
                }));
            }
            if debt_outstanding > 0 {
                components.push(json!({
                    "classification": "debt_outstanding",
                    "asset_id": asset_id,
                    "asset": asset_id,
                    "amount": debt_outstanding,
                    "position_count": debt_positions,
                    "source": "native_execution_store.credit_vaults",
                }));
            }
            if mapped_asset_active_balance > 0 {
                let mapped_phase4_mode =
                    if mapped_asset_active_balance == mapped_asset_shadow_active_balance {
                        "shadow"
                    } else {
                        "mixed"
                    };
                components.push(json!({
                    "classification": "mapped_asset_active_balance",
                    "asset_id": asset_id,
                    "asset": asset_id,
                    "amount": mapped_asset_active_balance,
                    "source": "unified_account_store.mapped_asset_state",
                    "phase4_mode": mapped_phase4_mode,
                    "settlement_effect": mapped_asset_settlement_effect_for_mode_v1(mapped_phase4_mode),
                    "non_settlement": mapped_phase4_mode == "shadow",
                }));
            }
            if treasury_source_flow > 0 {
                components.push(json!({
                    "classification": "treasury_source_flow",
                    "asset_id": asset_id,
                    "asset": asset_id,
                    "amount": treasury_source_flow,
                    "journal_entry_count": treasury_source_flow_entries,
                    "source": "native_execution_store.treasury_settlement_journal",
                }));
            }
            if asset_id == "NOV" {
                if treasury_settled_nov > 0 {
                    components.push(json!({
                        "classification": "treasury_settled_nov",
                        "asset_id": "NOV",
                        "asset": "NOV",
                        "amount": treasury_settled_nov,
                        "journal_entry_count": treasury_nov_entries,
                        "source": "native_execution_store.treasury_settlement_journal",
                    }));
                }
                if treasury_reserve_bucket_exposure_nov != 0 {
                    components.push(json!({
                        "classification": "treasury_reserve_bucket_exposure",
                        "asset_id": "NOV",
                        "asset": "NOV",
                        "amount_nov": treasury_reserve_bucket_exposure_nov,
                        "source": "native_execution_store.treasury_settlement_journal",
                    }));
                }
                if treasury_fee_bucket_exposure_nov != 0 {
                    components.push(json!({
                        "classification": "treasury_fee_bucket_exposure",
                        "asset_id": "NOV",
                        "asset": "NOV",
                        "amount_nov": treasury_fee_bucket_exposure_nov,
                        "source": "native_execution_store.treasury_settlement_journal",
                    }));
                }
                if treasury_risk_buffer_exposure_nov != 0 {
                    components.push(json!({
                        "classification": "treasury_risk_buffer_exposure",
                        "asset_id": "NOV",
                        "asset": "NOV",
                        "amount_nov": treasury_risk_buffer_exposure_nov,
                        "source": "native_execution_store.treasury_settlement_journal",
                    }));
                }
            }
            components.sort_by(|left, right| {
                left["classification"]
                    .as_str()
                    .cmp(&right["classification"].as_str())
            });
            let found = !components.is_empty();
            Ok((
                json!({
                    "method": method,
                    "found": found,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "asset_id": asset_id,
                    "asset": asset_id,
                    "balance": balance,
                    "mapped_asset_active_balance": mapped_asset_active_balance,
                    "mapped_asset_shadow_active_balance": mapped_asset_shadow_active_balance,
                    "locked_collateral": locked_collateral,
                    "debt_outstanding": debt_outstanding,
                    "treasury_source_flow": treasury_source_flow,
                    "treasury_settled_nov": treasury_settled_nov,
                    "treasury_reserve_bucket_exposure_nov": treasury_reserve_bucket_exposure_nov,
                    "treasury_fee_bucket_exposure_nov": treasury_fee_bucket_exposure_nov,
                    "treasury_risk_buffer_exposure_nov": treasury_risk_buffer_exposure_nov,
                    "ownership_subject": "account_id",
                    "component_count": components.len(),
                    "components": components,
                    "sources": [
                        "native_execution_store.account_asset_balances",
                        "native_execution_store.credit_vaults",
                        "native_execution_store.treasury_settlement_journal",
                        "unified_account_store.mapped_asset_state",
                    ],
                    "store_path": store_path.display().to_string(),
                }),
                false,
            ))
        }
        "account_assets" => {
            let account_id = parse_account_id(params)?;
            let store_path = native_execution_store_path_from_params_or_env_v1(params);
            let store = load_nov_native_execution_store_v1(store_path.as_path())?;
            let normalized_account = normalize_account_view_key_v1(&account_id);
            let mut assets = store
                .module_state
                .account_asset_balances
                .get(normalized_account.as_str())
                .map(|balances| {
                    balances
                        .iter()
                        .map(|(asset_id, balance)| {
                            json!({
                                "asset_id": asset_id,
                                "asset": asset_id,
                                "balance": balance,
                                "classification": "liquid_balance",
                                "source": "native_execution_store.account_asset_balances",
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            assets
                .sort_by(|left, right| left["asset_id"].as_str().cmp(&right["asset_id"].as_str()));
            let mut pledge_by_asset: BTreeMap<String, (u128, usize)> = BTreeMap::new();
            let mut vaults = store
                .module_state
                .credit_vaults
                .values()
                .filter(|vault| normalize_account_view_key_v1(&vault.owner) == normalized_account)
                .map(|vault| {
                    let collateral_asset = normalize_asset_view_symbol_v1(&vault.collateral_asset);
                    let pledge_entry = pledge_by_asset.entry(collateral_asset.clone()).or_default();
                    pledge_entry.0 = pledge_entry.0.saturating_add(vault.collateral_amount);
                    pledge_entry.1 = pledge_entry.1.saturating_add(1);
                    json!({
                        "vault_id": vault.vault_id,
                        "collateral_asset": collateral_asset,
                        "collateral_amount": vault.collateral_amount,
                        "debt_asset": normalize_asset_view_symbol_v1(&vault.debt_asset),
                        "debt_amount": vault.debt_amount,
                        "min_collateral_ratio_bps": vault.min_collateral_ratio_bps,
                        "opened_at_unix_ms": vault.opened_at_unix_ms,
                        "source": "native_execution_store.credit_vaults",
                    })
                })
                .collect::<Vec<_>>();
            vaults.sort_by_key(|entry| entry["vault_id"].as_u64().unwrap_or(0));
            let pledges = pledge_by_asset
                .into_iter()
                .map(|(asset_id, (pledged_amount, vault_count))| {
                    json!({
                        "asset_id": asset_id,
                        "asset": asset_id,
                        "pledged_amount": pledged_amount,
                        "vault_count": vault_count,
                        "classification": "pledge",
                        "source": "native_execution_store.credit_vaults",
                    })
                })
                .collect::<Vec<_>>();
            let mut treasury_flow_by_asset: BTreeMap<String, (u128, u128, usize)> = BTreeMap::new();
            let mut treasury_reserve_bucket_exposure_nov = 0i128;
            let mut treasury_fee_bucket_exposure_nov = 0i128;
            let mut treasury_risk_buffer_exposure_nov = 0i128;
            for entry in &store.module_state.treasury_settlement_journal {
                if normalize_account_view_key_v1(&entry.account_id) != normalized_account {
                    continue;
                }
                let asset_id = normalize_asset_view_symbol_v1(&entry.source_asset);
                let flow_entry = treasury_flow_by_asset.entry(asset_id).or_default();
                flow_entry.0 = flow_entry.0.saturating_add(entry.source_amount);
                flow_entry.1 = flow_entry.1.saturating_add(entry.settled_nov);
                flow_entry.2 = flow_entry.2.saturating_add(1);
                treasury_reserve_bucket_exposure_nov = treasury_reserve_bucket_exposure_nov
                    .saturating_add(entry.reserve_bucket_delta_nov);
                treasury_fee_bucket_exposure_nov =
                    treasury_fee_bucket_exposure_nov.saturating_add(entry.fee_bucket_delta_nov);
                treasury_risk_buffer_exposure_nov =
                    treasury_risk_buffer_exposure_nov.saturating_add(entry.risk_buffer_delta_nov);
            }
            let mut treasury_exposures = treasury_flow_by_asset
                .into_iter()
                .map(
                    |(asset_id, (source_amount_total, settled_nov_total, journal_entry_count))| {
                        json!({
                            "asset_id": asset_id,
                            "asset": asset_id,
                            "source_amount_total": source_amount_total,
                            "settled_nov_total": settled_nov_total,
                            "journal_entry_count": journal_entry_count,
                            "classification": "treasury_source_flow",
                            "source": "native_execution_store.treasury_settlement_journal",
                        })
                    },
                )
                .collect::<Vec<_>>();
            if treasury_reserve_bucket_exposure_nov != 0 {
                treasury_exposures.push(json!({
                    "asset_id": "NOV",
                    "asset": "NOV",
                    "amount_nov": treasury_reserve_bucket_exposure_nov,
                    "classification": "treasury_reserve_bucket_exposure",
                    "source": "native_execution_store.treasury_settlement_journal",
                }));
            }
            if treasury_fee_bucket_exposure_nov != 0 {
                treasury_exposures.push(json!({
                    "asset_id": "NOV",
                    "asset": "NOV",
                    "amount_nov": treasury_fee_bucket_exposure_nov,
                    "classification": "treasury_fee_bucket_exposure",
                    "source": "native_execution_store.treasury_settlement_journal",
                }));
            }
            if treasury_risk_buffer_exposure_nov != 0 {
                treasury_exposures.push(json!({
                    "asset_id": "NOV",
                    "asset": "NOV",
                    "amount_nov": treasury_risk_buffer_exposure_nov,
                    "classification": "treasury_risk_buffer_exposure",
                    "source": "native_execution_store.treasury_settlement_journal",
                }));
            }
            let mut mapped_assets = mapped_asset_state
                .records_by_mapping_id
                .values()
                .filter(|record| {
                    normalize_account_view_key_v1(&record.target_account_id) == normalized_account
                        && record.status == MappedAssetStatus::Active
                })
                .map(mapped_asset_record_to_json)
                .collect::<Vec<_>>();
            mapped_assets.sort_by(|left, right| {
                left["mapping_id"]
                    .as_str()
                    .cmp(&right["mapping_id"].as_str())
            });
            treasury_exposures.sort_by(|left, right| {
                left["classification"]
                    .as_str()
                    .cmp(&right["classification"].as_str())
                    .then(left["asset_id"].as_str().cmp(&right["asset_id"].as_str()))
            });
            Ok((
                json!({
                    "method": method,
                    "found": !assets.is_empty() || !vaults.is_empty() || !pledges.is_empty() || !treasury_exposures.is_empty() || !mapped_assets.is_empty(),
                    "account_id": account_id,
                    "uca_id": account_id,
                    "ownership_subject": "account_id",
                    "asset_count": assets.len(),
                    "assets": assets,
                    "pledge_count": pledges.len(),
                    "pledges": pledges,
                    "vault_count": vaults.len(),
                    "vaults": vaults,
                    "treasury_exposure_count": treasury_exposures.len(),
                    "treasury_exposures": treasury_exposures,
                    "mapped_asset_count": mapped_assets.len(),
                    "mapped_assets": mapped_assets,
                    "sources": [
                        "native_execution_store.account_asset_balances",
                        "native_execution_store.credit_vaults",
                        "native_execution_store.treasury_settlement_journal",
                        "unified_account_store.mapped_asset_state",
                    ],
                    "store_path": store_path.display().to_string(),
                }),
                false,
            ))
        }
        "ua_getAuditEvents" => {
            let source = param_as_string_any(params, &["source"])
                .unwrap_or_else(|| "sink".to_string())
                .trim()
                .to_ascii_lowercase();
            let limit = param_as_u64(params, "limit").unwrap_or(50).clamp(1, 500) as usize;
            if source == "router" {
                let clear = param_as_bool(params, "clear").unwrap_or(false);
                let mut events = if clear {
                    router.take_events()
                } else {
                    router.events().to_vec()
                };
                if events.len() > limit {
                    let start = events.len().saturating_sub(limit);
                    events = events[start..].to_vec();
                }
                let events_json = events
                    .iter()
                    .map(account_audit_event_to_json)
                    .collect::<Result<Vec<_>>>()?;
                return Ok((
                    json!({
                        "method": method,
                        "source": "router",
                        "count": events_json.len(),
                        "clear": clear,
                        "events": events_json,
                    }),
                    clear,
                ));
            }
            if source == "sink" {
                let since_seq = param_as_u64(params, "since_seq").unwrap_or(0);
                let (head_seq, events, next_since_seq, has_more) =
                    load_unified_account_audit_records_for_rpc(audit_sink, since_seq, limit)?;
                return Ok((
                    json!({
                        "method": method,
                        "source": "sink",
                        "backend": audit_sink.backend_name(),
                        "path": audit_sink.path().display().to_string(),
                        "since_seq": since_seq,
                        "head_seq": head_seq,
                        "next_since_seq": next_since_seq,
                        "cursor": next_since_seq,
                        "has_more": has_more,
                        "count": events.len(),
                        "events": events,
                    }),
                    false,
                ));
            }
            bail!(
                "invalid source for ua_getAuditEvents: {}; valid: router|sink",
                source
            );
        }
        "ua_route" => {
            let account_id = parse_account_id(params)?;
            let role = parse_account_role(params)?;
            let protocol = parse_protocol_kind(params)?;
            let persona = parse_persona(params, true)?;
            let signature_domain = param_as_string_any(params, &["signature_domain"])
                .unwrap_or_else(|| default_signature_domain(&persona, &protocol));
            let nonce = match param_as_u64(params, "nonce") {
                Some(nonce) => nonce,
                None => router.next_nonce_for_persona(&account_id, &persona)?,
            };
            let wants_cross_chain_atomic =
                param_as_bool(params, "wants_cross_chain_atomic").unwrap_or(false);
            let tx_type4 = param_as_bool(params, "tx_type4").unwrap_or(false);
            let session_expires_at = param_as_u64(params, "session_expires_at");
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let kyc_attestation_provided =
                param_as_bool(params, "kyc_attestation_provided").unwrap_or(false);
            let kyc_verified = param_as_bool(params, "kyc_verified").unwrap_or(false);
            let decision = router.route(RouteRequest {
                uca_id: account_id.clone(),
                persona,
                role,
                protocol,
                signature_domain: signature_domain.clone(),
                nonce,
                kyc_attestation_provided,
                kyc_verified,
                wants_cross_chain_atomic,
                tx_type4,
                session_expires_at,
                now,
            })?;
            Ok((
                json!({
                    "method": method,
                    "accepted": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "decision": route_decision_to_json(&decision),
                    "signature_domain": signature_domain,
                    "nonce": nonce,
                    "tx_type4": tx_type4,
                    "wants_cross_chain_atomic": wants_cross_chain_atomic,
                    "kyc_attestation_provided": kyc_attestation_provided,
                    "kyc_verified": kyc_verified,
                    "session_expires_at": session_expires_at,
                }),
                true,
            ))
        }
        _ => bail!(
            "unsupported mainline unified account query method: {}",
            method
        ),
    }
}

fn string_env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn native_execution_store_path_from_params_or_env_v1(params: &Value) -> PathBuf {
    match params {
        Value::Object(map) => map
            .get("native_execution_store_path")
            .and_then(|value| value.as_str()),
        Value::Array(items) => items
            .first()
            .and_then(|value| value.get("native_execution_store_path"))
            .and_then(|value| value.as_str()),
        _ => None,
    }
    .map(|raw| raw.trim().to_string())
    .filter(|raw| !raw.is_empty())
    .map(PathBuf::from)
    .or_else(|| {
        string_env_nonempty("NOVOVM_MAINLINE_NATIVE_EXECUTION_STORE_PATH").map(PathBuf::from)
    })
    .unwrap_or_else(nov_native_execution_store_path_v1)
}

fn bool_env_default(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        },
        Err(_) => default,
    }
}

fn phase4_nogo_enforced_v1() -> bool {
    bool_env_default(NOVOVM_UA_PHASE4_NOGO_ENFORCE_ENV, false)
}

fn phase4_shadow_mode_enforced_v1() -> bool {
    bool_env_default(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, true)
}

fn parse_phase4_mode_v1(params: &Value) -> Result<String> {
    let raw = param_as_string_any(params, &["phase4_mode", "mapped_asset_mode", "mode"])
        .unwrap_or_else(|| "shadow".to_string())
        .trim()
        .to_ascii_lowercase();
    let normalized = raw
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match normalized.as_str() {
        "shadow" | "shadowmode" => Ok("shadow".to_string()),
        "live" | "production" | "prod" => Ok("live".to_string()),
        other => bail!(
            "ERR_PHASE4_MODE_INVALID: unsupported phase4_mode {}; valid values: shadow, live",
            other
        ),
    }
}

fn mapped_asset_phase4_mode_v1(record: &MappedAssetRecord) -> String {
    let normalized = record
        .phase4_mode
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match normalized.as_str() {
        "live" | "production" | "prod" => "live".to_string(),
        _ => "shadow".to_string(),
    }
}

fn mapped_asset_is_shadow_mode_v1(record: &MappedAssetRecord) -> bool {
    mapped_asset_phase4_mode_v1(record) == "shadow"
}

fn is_shadow_phase4_mode_v1(mode: &str) -> bool {
    mode == "shadow"
}

fn mapped_asset_settlement_effect_for_mode_v1(mode: &str) -> &'static str {
    if mode == "live" {
        "neth_m2_credit"
    } else {
        "none"
    }
}

fn mapped_asset_settlement_effect_for_record_v1(record: &MappedAssetRecord) -> &'static str {
    mapped_asset_settlement_effect_for_mode_v1(mapped_asset_phase4_mode_v1(record).as_str())
}

fn append_ua_treasury_journal_v1(
    store: &mut crate::tx_ingress::NovNativeExecutionStoreV1,
    mut entry: NovTreasurySettlementJournalEntryV1,
) {
    let next_seq = store
        .module_state
        .treasury_settlement_journal_next_seq
        .saturating_add(1);
    store.module_state.treasury_settlement_journal_next_seq = next_seq;
    entry.seq = next_seq;
    store.module_state.treasury_settlement_journal.push(entry);
}

fn mapped_asset_live_settlement_journal_v1(
    kind: &str,
    record: &MappedAssetRecord,
    mapping_key: &str,
    source_tx_hash: &str,
    status: &str,
    reason: &str,
    now: u64,
) -> NovTreasurySettlementJournalEntryV1 {
    NovTreasurySettlementJournalEntryV1 {
        seq: 0,
        unix_ms: u128::from(now).saturating_mul(1000),
        kind: kind.to_string(),
        tx_hash: source_tx_hash.to_string(),
        account_id: normalize_account_view_key_v1(&record.target_account_id),
        fee_owner_account_id: normalize_account_view_key_v1(&record.target_account_id),
        nonce_owner_account_id: normalize_account_view_key_v1(&record.target_account_id),
        key_algo: String::new(),
        execution_policy: "mapped_lock_m2_credit".to_string(),
        policy_enforced: true,
        policy_rejection_reason: None,
        source_asset: normalize_asset_view_symbol_v1(&record.target_asset_symbol),
        source_amount: record.amount,
        settled_nov: 0,
        reserve_bucket_delta_nov: 0,
        fee_bucket_delta_nov: 0,
        risk_buffer_delta_nov: 0,
        route_ref: mapping_key.to_string(),
        clearing_source: "mapped_lock:neth_m2_credit:no_nov_mint".to_string(),
        clearing_rate_ppm: 0,
        policy_version: 1,
        policy_source: "unified_account_surface".to_string(),
        policy_contract_id: "mapped_lock_m2_credit_v1".to_string(),
        policy_threshold_state: "m2_only_no_nov_mint".to_string(),
        policy_constrained_strategy: "treasury_policy_required_for_nov_mint".to_string(),
        policy_event_state: "external_lock_mapped_to_m2".to_string(),
        status: status.to_string(),
        reason: Some(reason.to_string()),
    }
}

fn apply_live_mapped_lock_m2_credit_v1(
    record: &MappedAssetRecord,
    mapping_key: &str,
    source_tx_hash: &[u8],
    params: &Value,
    now: u64,
) -> Result<Value> {
    if mapped_asset_is_shadow_mode_v1(record) {
        return Ok(json!({
            "applied": false,
            "mode": "shadow",
            "effect": "none",
            "reason": "shadow mode does not mutate native balances or treasury reserves",
        }));
    }
    let store_path = native_execution_store_path_from_params_or_env_v1(params);
    let account_key = normalize_account_view_key_v1(&record.target_account_id);
    let asset_key = normalize_asset_view_symbol_v1(&record.target_asset_symbol);
    let mut store = load_nov_native_execution_store_v1(store_path.as_path())?;
    let user_balance_after = {
        let balances = store
            .module_state
            .account_asset_balances
            .entry(account_key.clone())
            .or_default();
        let entry = balances.entry(asset_key.clone()).or_insert(0);
        *entry = entry.saturating_add(record.amount);
        *entry
    };
    let reserve_after = {
        let entry = store
            .module_state
            .treasury_reserves
            .entry(asset_key.clone())
            .or_insert(0);
        *entry = entry.saturating_add(record.amount);
        *entry
    };
    append_ua_treasury_journal_v1(
        &mut store,
        mapped_asset_live_settlement_journal_v1(
            "mapped_lock_m2_credit",
            record,
            mapping_key,
            format!("0x{}", to_hex_lower(source_tx_hash)).as_str(),
            "success",
            "ETH lock mapped to NETH M2 credit; NOV mint is not triggered",
            now,
        ),
    );
    store.last_updated_unix_ms = u128::from(now).saturating_mul(1000);
    save_nov_native_execution_store_v1(store_path.as_path(), &store)?;
    Ok(json!({
        "applied": true,
        "mode": "live",
        "effect": "neth_m2_credit",
        "asset": asset_key,
        "amount": record.amount,
        "account_balance_after": user_balance_after,
        "treasury_reserve_after": reserve_after,
        "nov_minted": 0,
        "store_path": store_path.display().to_string(),
    }))
}

fn apply_live_mapped_asset_m2_burn_v1(
    record: &MappedAssetRecord,
    mapping_key: &str,
    params: &Value,
    now: u64,
) -> Result<Value> {
    if mapped_asset_is_shadow_mode_v1(record) {
        return Ok(json!({
            "applied": false,
            "mode": "shadow",
            "effect": "none",
            "reason": "shadow mode does not mutate native balances",
        }));
    }
    let store_path = native_execution_store_path_from_params_or_env_v1(params);
    let account_key = normalize_account_view_key_v1(&record.target_account_id);
    let asset_key = normalize_asset_view_symbol_v1(&record.target_asset_symbol);
    let mut store = load_nov_native_execution_store_v1(store_path.as_path())?;
    let user_balance_after = {
        let balances = store
            .module_state
            .account_asset_balances
            .entry(account_key.clone())
            .or_default();
        let entry = balances.entry(asset_key.clone()).or_insert(0);
        if *entry < record.amount {
            bail!(
                "ERR_MAPPED_BURN_NATIVE_BALANCE_INSUFFICIENT: account={} asset={} requested={} available={}",
                account_key,
                asset_key,
                record.amount,
                *entry
            );
        }
        *entry = entry.saturating_sub(record.amount);
        *entry
    };
    append_ua_treasury_journal_v1(
        &mut store,
        mapped_asset_live_settlement_journal_v1(
            "mapped_asset_m2_burn_pending",
            record,
            mapping_key,
            mapped_asset_hex_id(&record.mapping_id).as_str(),
            "success",
            "NETH M2 credit burned before external source release",
            now,
        ),
    );
    store.last_updated_unix_ms = u128::from(now).saturating_mul(1000);
    save_nov_native_execution_store_v1(store_path.as_path(), &store)?;
    Ok(json!({
        "applied": true,
        "mode": "live",
        "effect": "neth_m2_burn_pending",
        "asset": asset_key,
        "amount": record.amount,
        "account_balance_after": user_balance_after,
        "store_path": store_path.display().to_string(),
    }))
}

fn apply_live_mapped_lock_source_release_v1(
    record: &MappedAssetRecord,
    mapping_key: &str,
    params: &Value,
    now: u64,
) -> Result<Value> {
    if mapped_asset_is_shadow_mode_v1(record) {
        return Ok(json!({
            "applied": false,
            "mode": "shadow",
            "effect": "none",
            "reason": "shadow mode does not mutate treasury reserves",
        }));
    }
    let store_path = native_execution_store_path_from_params_or_env_v1(params);
    let asset_key = normalize_asset_view_symbol_v1(&record.target_asset_symbol);
    let mut store = load_nov_native_execution_store_v1(store_path.as_path())?;
    let reserve_after = {
        let entry = store
            .module_state
            .treasury_reserves
            .entry(asset_key.clone())
            .or_insert(0);
        if *entry < record.amount {
            bail!(
                "ERR_MAPPED_RELEASE_TREASURY_RESERVE_INSUFFICIENT: asset={} requested={} available={}",
                asset_key,
                record.amount,
                *entry
            );
        }
        *entry = entry.saturating_sub(record.amount);
        *entry
    };
    append_ua_treasury_journal_v1(
        &mut store,
        mapped_asset_live_settlement_journal_v1(
            "mapped_lock_source_release",
            record,
            mapping_key,
            mapped_asset_hex_id(&record.mapping_id).as_str(),
            "success",
            "Treasury reserve released after mapped M2 burn; external unlock remains bridge responsibility",
            now,
        ),
    );
    store.last_updated_unix_ms = u128::from(now).saturating_mul(1000);
    save_nov_native_execution_store_v1(store_path.as_path(), &store)?;
    Ok(json!({
        "applied": true,
        "mode": "live",
        "effect": "source_release_reserve_debit",
        "asset": asset_key,
        "amount": record.amount,
        "treasury_reserve_after": reserve_after,
        "store_path": store_path.display().to_string(),
    }))
}

fn emit_governance_event_best_effort_v1(event: GovernanceEvent) {
    let _ = append_governance_event_auto("unified_account_surface", event);
}

fn emit_mapped_asset_operation_observed_v1(
    operation: &str,
    accepted: bool,
    account_id: Option<&str>,
    mapping_id: Option<&str>,
    reason: Option<&str>,
    demand_quality: Option<&str>,
) {
    emit_governance_event_best_effort_v1(GovernanceEvent::MappedAssetOperationObserved {
        operation: operation.to_string(),
        accepted,
        account_id: account_id.map(|value| value.to_string()),
        mapping_id: mapping_id.map(|value| value.to_string()),
        reason: reason.map(|value| value.to_string()),
        demand_quality: demand_quality.map(|value| value.to_string()),
    });
}

fn emit_external_inflow_demand_observed_v1(
    channel: &str,
    qualified: bool,
    accepted: bool,
    account_id: Option<&str>,
    source_chain: Option<&str>,
    amount: Option<u128>,
    reason: Option<&str>,
) {
    emit_governance_event_best_effort_v1(GovernanceEvent::ExternalInflowDemandObserved {
        channel: channel.to_string(),
        qualified,
        accepted,
        account_id: account_id.map(|value| value.to_string()),
        source_chain: source_chain.map(|value| value.to_string()),
        amount,
        reason: reason.map(|value| value.to_string()),
    });
}

fn param_as_string(params: &Value, key: &str) -> Option<String> {
    match params {
        Value::Object(map) => map.get(key).and_then(|value| match value {
            Value::String(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        }),
        Value::Array(items) => items
            .first()
            .and_then(|value| value.get(key))
            .and_then(|value| {
                value.as_str().and_then(|raw| {
                    let trimmed = raw.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                })
            }),
        _ => None,
    }
}

fn param_as_string_any(params: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| param_as_string(params, key))
}

fn parse_hex_u64(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u64::from_str_radix(normalized, 16).ok()
}

fn value_as_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(raw) => parse_hex_u64(raw).or_else(|| raw.trim().parse::<u64>().ok()),
        _ => None,
    }
}

fn param_as_u64(params: &Value, key: &str) -> Option<u64> {
    match params {
        Value::Object(map) => map.get(key).and_then(value_as_u64),
        Value::Array(items) => items
            .first()
            .and_then(|value| value.get(key))
            .and_then(value_as_u64),
        _ => None,
    }
}

fn param_as_bool(params: &Value, key: &str) -> Option<bool> {
    match params {
        Value::Object(map) => map.get(key).and_then(|value| match value {
            Value::Bool(v) => Some(*v),
            Value::String(raw) => match raw.trim().to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => Some(true),
                "0" | "false" | "no" | "off" => Some(false),
                _ => None,
            },
            _ => None,
        }),
        Value::Array(items) => items
            .first()
            .and_then(|value| value.get(key))
            .and_then(|value| match value {
                Value::Bool(v) => Some(*v),
                Value::String(raw) => match raw.trim().to_ascii_lowercase().as_str() {
                    "1" | "true" | "yes" | "on" => Some(true),
                    "0" | "false" | "no" | "off" => Some(false),
                    _ => None,
                },
                _ => None,
            }),
        _ => None,
    }
}

fn value_as_u128(value: &Value) -> Option<u128> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .map(u128::from)
            .or_else(|| number.as_i64().and_then(|raw| u128::try_from(raw).ok())),
        Value::String(raw) => {
            let trimmed = raw.trim();
            if let Some(normalized) = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"))
            {
                u128::from_str_radix(normalized, 16).ok()
            } else {
                trimmed.parse::<u128>().ok()
            }
        }
        _ => None,
    }
}

fn param_as_u128(params: &Value, key: &str) -> Option<u128> {
    match params {
        Value::Object(map) => map.get(key).and_then(value_as_u128),
        Value::Array(items) => items
            .first()
            .and_then(|value| value.get(key))
            .and_then(value_as_u128),
        _ => None,
    }
}

fn decode_hex_bytes(raw: &str, field: &str) -> Result<Vec<u8>> {
    let normalized = raw.trim().strip_prefix("0x").unwrap_or(raw.trim());
    if normalized.is_empty() {
        bail!("{field} is empty");
    }
    if !normalized.len().is_multiple_of(2) {
        bail!("{field} must have even hex length");
    }
    if !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("{field} must be hex");
    }
    let mut out = Vec::with_capacity(normalized.len() / 2);
    let bytes = normalized.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let pair = std::str::from_utf8(&bytes[index..index + 2])
            .with_context(|| format!("{field} contains invalid utf8"))?;
        out.push(
            u8::from_str_radix(pair, 16)
                .with_context(|| format!("{field} contains invalid hex byte {pair}"))?,
        );
        index += 2;
    }
    Ok(out)
}

fn to_hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn decode_hex_fixed_32(raw: &str, field: &str) -> Result<[u8; 32]> {
    let out = decode_hex_bytes(raw, field)?;
    out.try_into()
        .map_err(|_| anyhow::anyhow!("{field} must decode to 32 bytes"))
}

fn mapped_asset_hex_id(id: &[u8; 32]) -> String {
    format!("0x{}", to_hex_lower(id))
}

fn parse_mapped_source_chain(params: &Value) -> Result<MappedAssetSourceChain> {
    let raw = param_as_string_any(params, &["source_chain"])
        .unwrap_or_else(|| "ethereum".to_string())
        .trim()
        .to_ascii_lowercase();
    Ok(match raw.as_str() {
        "ethereum" | "eth" => MappedAssetSourceChain::Ethereum,
        other => MappedAssetSourceChain::Other(other.to_string()),
    })
}

fn parse_mapped_lock_proof_format(params: &Value) -> Result<MappedLockProofFormat> {
    let raw = param_as_string_any(params, &["proof_format", "lock_proof_format"])
        .unwrap_or_else(|| "ethereum_lock_event_v1".to_string())
        .trim()
        .to_ascii_lowercase();
    match raw.as_str() {
        "ethereum_lock_event_v1" | "eth_lock_event_v1" | "ethereumeventv1" => {
            Ok(MappedLockProofFormat::EthereumLockEventV1)
        }
        other => bail!(
            "ERR_MAPPED_LOCK_PROOF_INVALID: invalid proof_format {}; valid: ethereum_lock_event_v1",
            other
        ),
    }
}

fn parse_mapped_asset_lock_proof(params: &Value) -> Result<MappedAssetLockProof> {
    let lock_id_raw = param_as_string_any(params, &["lock_id"])
        .ok_or_else(|| anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: lock_id is required"))?;
    let source_tx_hash_raw = param_as_string_any(params, &["source_tx_hash", "tx_hash"])
        .ok_or_else(|| {
            anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: source_tx_hash is required")
        })?;
    let source_lock_ref_raw = param_as_string_any(params, &["source_lock_ref", "lock_ref"])
        .ok_or_else(|| {
            anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: source_lock_ref is required")
        })?;
    let external_owner_ref_raw =
        param_as_string_any(params, &["external_owner_ref", "external_address"]).ok_or_else(
            || anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: external_owner_ref is required"),
        )?;
    let target_account_id_raw = param_as_string_any(
        params,
        &["target_account_id", "account_id", "uca_id"],
    )
    .ok_or_else(|| {
        anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: target_account_id/account_id is required")
    })?;
    let source_asset_symbol = normalize_asset_view_symbol_v1(
        &param_as_string_any(params, &["source_asset_symbol", "source_asset"])
            .unwrap_or_else(|| "ETH".to_string()),
    );
    let amount = param_as_u128(params, "amount")
        .ok_or_else(|| anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: amount is required"))?;
    let proof_payload_raw = param_as_string_any(params, &["proof_payload", "lock_proof_payload"])
        .ok_or_else(|| {
        anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: proof_payload is required")
    })?;
    let target_account_id = validate_uca_id_policy(&target_account_id_raw)?;
    Ok(MappedAssetLockProof {
        lock_id: decode_hex_fixed_32(&lock_id_raw, "lock_id")?,
        source_chain: parse_mapped_source_chain(params)?,
        source_asset_symbol,
        source_tx_hash: decode_hex_bytes(&source_tx_hash_raw, "source_tx_hash")?,
        source_lock_ref: decode_hex_bytes(&source_lock_ref_raw, "source_lock_ref")?,
        external_owner_ref: decode_hex_bytes(&external_owner_ref_raw, "external_owner_ref")?,
        target_account_id,
        amount,
        proof_payload: decode_hex_bytes(&proof_payload_raw, "proof_payload")?,
        proof_format: parse_mapped_lock_proof_format(params)?,
    })
}

fn mapped_lock_proof_digest_v1(proof: &MappedAssetLockProof) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm-mapped-lock-proof-v1");
    hasher.update([0u8]);
    hasher.update(proof.lock_id);
    hasher.update([0u8]);
    hasher.update(proof.source_chain.as_str().as_bytes());
    hasher.update([0u8]);
    hasher.update(normalize_asset_view_symbol_v1(&proof.source_asset_symbol).as_bytes());
    hasher.update([0u8]);
    hasher.update(proof.source_tx_hash.as_slice());
    hasher.update([0u8]);
    hasher.update(proof.source_lock_ref.as_slice());
    hasher.update([0u8]);
    hasher.update(proof.external_owner_ref.as_slice());
    hasher.update([0u8]);
    hasher.update(proof.target_account_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(proof.amount.to_be_bytes());
    hasher.finalize().into()
}

#[derive(Debug, Clone)]
struct EthereumLockEventEvidenceV1 {
    contract_address: [u8; 20],
    topic0: [u8; 32],
    block_number: u64,
    finalized_block_number: u64,
    log_index: u64,
}

fn decode_hex_fixed_20(raw: &str, field: &str) -> Result<[u8; 20]> {
    let out = decode_hex_bytes(raw, field)?;
    out.try_into()
        .map_err(|_| anyhow::anyhow!("{field} must decode to 20 bytes"))
}

fn eth_lock_event_topic0_v1() -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(ETH_LOCK_EVENT_SIGNATURE_V1.as_bytes());
    hasher.finalize().into()
}

fn configured_eth_lock_contract_address_v1(params: &Value) -> Result<Option<[u8; 20]>> {
    if let Ok(raw) = std::env::var(NOVOVM_UA_ETH_LOCK_CONTRACT_ADDRESS_ENV) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return decode_hex_fixed_20(trimmed, NOVOVM_UA_ETH_LOCK_CONTRACT_ADDRESS_ENV).map(Some);
        }
    }
    param_as_string_any(
        params,
        &[
            "expected_lock_contract_address",
            "expected_contract_address",
            "configured_lock_contract_address",
        ],
    )
    .map(|raw| decode_hex_fixed_20(raw.as_str(), "expected_lock_contract_address"))
    .transpose()
}

fn eth_lock_min_confirmations_v1() -> u64 {
    std::env::var(NOVOVM_UA_ETH_LOCK_MIN_CONFIRMATIONS_ENV)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(12)
}

fn parse_ethereum_lock_event_evidence_v1(
    params: &Value,
) -> Result<Option<EthereumLockEventEvidenceV1>> {
    let has_structured = param_as_string_any(
        params,
        &[
            "lock_contract_address",
            "contract_address",
            "eth_lock_contract_address",
        ],
    )
    .is_some()
        || param_as_string_any(params, &["event_topic0", "topic0"]).is_some()
        || param_as_u64(params, "block_number").is_some()
        || param_as_u64(params, "finalized_block_number").is_some()
        || param_as_u64(params, "log_index").is_some();
    if !has_structured {
        return Ok(None);
    }
    let contract_raw = param_as_string_any(
        params,
        &[
            "lock_contract_address",
            "contract_address",
            "eth_lock_contract_address",
        ],
    )
    .ok_or_else(|| {
        anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: lock_contract_address is required")
    })?;
    let topic0_raw = param_as_string_any(params, &["event_topic0", "topic0"]).ok_or_else(|| {
        anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: event_topic0 is required")
    })?;
    let block_number = param_as_u64(params, "block_number").ok_or_else(|| {
        anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: block_number is required")
    })?;
    let finalized_block_number =
        param_as_u64(params, "finalized_block_number").ok_or_else(|| {
            anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: finalized_block_number is required")
        })?;
    let log_index = param_as_u64(params, "log_index")
        .ok_or_else(|| anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: log_index is required"))?;
    Ok(Some(EthereumLockEventEvidenceV1 {
        contract_address: decode_hex_fixed_20(contract_raw.as_str(), "lock_contract_address")?,
        topic0: decode_hex_fixed_32(topic0_raw.as_str(), "event_topic0")?,
        block_number,
        finalized_block_number,
        log_index,
    }))
}

fn ethereum_lock_event_ref_digest_v1(
    proof: &MappedAssetLockProof,
    evidence: &EthereumLockEventEvidenceV1,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm-ethereum-lock-event-ref-v1");
    hasher.update([0u8]);
    hasher.update(evidence.contract_address);
    hasher.update([0u8]);
    hasher.update(evidence.topic0);
    hasher.update([0u8]);
    hasher.update(evidence.block_number.to_be_bytes());
    hasher.update([0u8]);
    hasher.update(evidence.finalized_block_number.to_be_bytes());
    hasher.update([0u8]);
    hasher.update(evidence.log_index.to_be_bytes());
    hasher.update([0u8]);
    hasher.update(proof.source_tx_hash.as_slice());
    hasher.update([0u8]);
    hasher.update(proof.lock_id);
    hasher.update([0u8]);
    hasher.update(proof.external_owner_ref.as_slice());
    hasher.update([0u8]);
    hasher.update(proof.target_account_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(proof.amount.to_be_bytes());
    hasher.finalize().into()
}

fn verify_ethereum_lock_event_evidence_v1(
    proof: &MappedAssetLockProof,
    params: &Value,
    live_required: bool,
) -> Result<()> {
    let Some(evidence) = parse_ethereum_lock_event_evidence_v1(params)? else {
        if live_required {
            bail!(
                "ERR_MAPPED_LOCK_PROOF_INVALID: live mapped lock requires structured Ethereum lock event evidence"
            );
        }
        return Ok(());
    };
    if proof.source_tx_hash.len() != 32 {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: source_tx_hash must decode to 32 bytes");
    }
    if proof.external_owner_ref.len() != 20 {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: external_owner_ref must decode to 20 bytes");
    }
    let expected_topic0 = eth_lock_event_topic0_v1();
    if evidence.topic0 != expected_topic0 {
        bail!(
            "ERR_MAPPED_LOCK_PROOF_INVALID: event_topic0 must be {} topic",
            ETH_LOCK_EVENT_SIGNATURE_V1
        );
    }
    let configured_contract = configured_eth_lock_contract_address_v1(params)?.ok_or_else(|| {
        anyhow::anyhow!(
            "ERR_MAPPED_LOCK_PROOF_INVALID: live mapped lock requires {} or expected_lock_contract_address",
            NOVOVM_UA_ETH_LOCK_CONTRACT_ADDRESS_ENV
        )
    })?;
    if evidence.contract_address != configured_contract {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: lock_contract_address does not match configured contract");
    }
    let min_confirmations = eth_lock_min_confirmations_v1();
    let required_finalized = evidence.block_number.saturating_add(min_confirmations);
    if evidence.finalized_block_number < required_finalized {
        bail!(
            "ERR_MAPPED_LOCK_PROOF_INVALID: finalized_block_number {} is below required {}",
            evidence.finalized_block_number,
            required_finalized
        );
    }
    let expected_ref = ethereum_lock_event_ref_digest_v1(proof, &evidence);
    if proof.source_lock_ref.as_slice() != expected_ref.as_slice() {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: source_lock_ref does not match Ethereum lock event evidence");
    }
    Ok(())
}

fn verify_mapped_lock_proof(
    proof: &MappedAssetLockProof,
    params: &Value,
    live_required: bool,
) -> Result<()> {
    if proof.amount == 0 {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: amount must be > 0");
    }
    if proof.source_tx_hash.is_empty() {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: source_tx_hash must not be empty");
    }
    if proof.source_lock_ref.is_empty() {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: source_lock_ref must not be empty");
    }
    if proof.external_owner_ref.is_empty() {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: external_owner_ref must not be empty");
    }
    if normalize_asset_view_symbol_v1(&proof.source_asset_symbol) != "ETH" {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: source_asset_symbol must be ETH in MVP slice");
    }
    match &proof.source_chain {
        MappedAssetSourceChain::Ethereum => {}
        _ => bail!("ERR_MAPPED_LOCK_PROOF_INVALID: source_chain must be ethereum in MVP slice"),
    }
    match proof.proof_format {
        MappedLockProofFormat::EthereumLockEventV1 => {}
    }
    verify_ethereum_lock_event_evidence_v1(proof, params, live_required)?;
    let digest = mapped_lock_proof_digest_v1(proof);
    if proof.proof_payload.as_slice() != digest.as_slice() {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: proof payload digest mismatch");
    }
    Ok(())
}

fn derive_mapping_id_from_lock_proof_v1(proof: &MappedAssetLockProof) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm-mapped-asset-mapping-id-v1");
    hasher.update([0u8]);
    hasher.update(proof.lock_id);
    hasher.update([0u8]);
    hasher.update(proof.target_account_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(normalize_asset_view_symbol_v1(&proof.source_asset_symbol).as_bytes());
    hasher.update([0u8]);
    hasher.update(proof.amount.to_be_bytes());
    hasher.finalize().into()
}

fn derive_mapped_asset_audit_ref_v1(
    mapping_id: [u8; 32],
    status: MappedAssetStatus,
    now: u64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm-mapped-asset-audit-ref-v1");
    hasher.update([0u8]);
    hasher.update(mapping_id);
    hasher.update([0u8]);
    hasher.update(status.as_str().as_bytes());
    hasher.update([0u8]);
    hasher.update(now.to_be_bytes());
    hasher.finalize().into()
}

fn build_mapped_asset_operation_v1(
    record: &MappedAssetRecord,
    kind: MappedAssetOperationKind,
    now: u64,
    op_seq: u64,
) -> MappedAssetOperation {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm-mapped-asset-op-id-v1");
    hasher.update([0u8]);
    hasher.update(record.mapping_id);
    hasher.update([0u8]);
    hasher.update(kind.as_str().as_bytes());
    hasher.update([0u8]);
    hasher.update(record.target_account_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(record.amount.to_be_bytes());
    hasher.update([0u8]);
    hasher.update(now.to_be_bytes());
    hasher.update([0u8]);
    hasher.update(op_seq.to_be_bytes());
    MappedAssetOperation {
        op_id: hasher.finalize().into(),
        mapping_id: record.mapping_id,
        kind,
        account_id: record.target_account_id.clone(),
        amount: record.amount,
        created_at: now,
    }
}

fn mapped_asset_record_to_json(record: &MappedAssetRecord) -> Value {
    let account_id = record.target_account_id.clone();
    let target_asset_symbol = normalize_asset_view_symbol_v1(&record.target_asset_symbol);
    let asset_id = target_asset_symbol.clone();
    let asset = target_asset_symbol.clone();
    let target_account_id = account_id.clone();
    let uca_id = account_id.clone();
    let phase4_mode = mapped_asset_phase4_mode_v1(record);
    let classification = if phase4_mode == "shadow" {
        "mapped_asset_shadow_active"
    } else {
        "mapped_asset_active"
    };
    json!({
        "mapping_id": mapped_asset_hex_id(&record.mapping_id),
        "lock_id": mapped_asset_hex_id(&record.lock_id),
        "source_chain": record.source_chain.as_str(),
        "source_asset_symbol": normalize_asset_view_symbol_v1(&record.source_asset_symbol),
        "source_tx_hash": format!("0x{}", to_hex_lower(&record.source_tx_hash)),
        "source_lock_ref": format!("0x{}", to_hex_lower(&record.source_lock_ref)),
        "external_owner_ref": format!("0x{}", to_hex_lower(&record.external_owner_ref)),
        "target_asset_symbol": target_asset_symbol,
        "asset_id": asset_id,
        "asset": asset,
        "amount": record.amount,
        "balance": record.amount,
        "target_account_id": target_account_id,
        "account_id": account_id,
        "uca_id": uca_id,
        "status": record.status.as_str(),
        "classification": classification,
        "phase4_mode": phase4_mode,
        "settlement_effect": mapped_asset_settlement_effect_for_record_v1(record),
        "ownership_subject": "account_id",
        "source": "unified_account_store.mapped_asset_state",
        "audit_ref": mapped_asset_hex_id(&record.audit_ref),
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    })
}

fn mapped_asset_operation_to_json(op: &MappedAssetOperation) -> Value {
    let account_id = op.account_id.clone();
    let uca_id = account_id.clone();
    json!({
        "op_id": mapped_asset_hex_id(&op.op_id),
        "mapping_id": mapped_asset_hex_id(&op.mapping_id),
        "kind": op.kind.as_str(),
        "account_id": account_id,
        "uca_id": uca_id,
        "amount": op.amount,
        "created_at": op.created_at,
    })
}

fn resolve_mapped_asset_lookup_key(
    params: &Value,
    state: &UnifiedMappedAssetState,
) -> Result<String> {
    if let Some(mapping_id_raw) = param_as_string_any(params, &["mapping_id"]) {
        let mapping_id = decode_hex_fixed_32(&mapping_id_raw, "mapping_id")?;
        return Ok(mapped_asset_hex_id(&mapping_id));
    }
    if let Some(lock_id_raw) = param_as_string_any(params, &["lock_id"]) {
        let lock_id = decode_hex_fixed_32(&lock_id_raw, "lock_id")?;
        let lock_key = mapped_asset_hex_id(&lock_id);
        if let Some(mapping_key) = state.mapping_id_by_lock_id.get(lock_key.as_str()) {
            return Ok(mapping_key.clone());
        }
        bail!(
            "ERR_MAPPED_ASSET_NOT_FOUND: mapped asset not found for lock_id {}",
            lock_key
        );
    }
    bail!("ERR_MAPPED_ASSET_NOT_FOUND: mapping_id or lock_id is required");
}

fn normalize_asset_view_symbol_v1(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "NOV".to_string()
    } else {
        trimmed.to_ascii_uppercase()
    }
}

fn normalize_account_view_key_v1(raw: &str) -> String {
    let trimmed = raw.trim();
    let normalized_hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .filter(|token| !token.is_empty() && token.chars().all(|ch| ch.is_ascii_hexdigit()))
        .map(|token| format!("0x{}", token.to_ascii_lowercase()));
    normalized_hex.unwrap_or_else(|| trimmed.to_ascii_lowercase())
}

fn parse_account_id(params: &Value) -> Result<String> {
    let raw = param_as_string_any(params, &["account_id", "uca_id"])
        .ok_or_else(|| anyhow::anyhow!("account_id/uca_id is required"))?;
    validate_uca_id_policy(&raw)
}

#[derive(Debug)]
struct PendingPrimaryKeyBindingV1 {
    key_algo: UcaKeyAlgo,
    public_key: Vec<u8>,
    proof_type: UcaKeyProofType,
    proof_payload: Vec<u8>,
}

fn parse_primary_key_ref(params: &Value, account_id: &str, field_name: &str) -> Result<Vec<u8>> {
    if let Some(raw) = param_as_string_any(params, &[field_name]) {
        return decode_hex_bytes(&raw, field_name);
    }
    let mut hasher = Sha256::new();
    hasher.update(b"uca-primary-key-ref-v1");
    hasher.update(account_id.as_bytes());
    Ok(hasher.finalize().to_vec())
}

fn parse_optional_primary_key_binding_v1(
    params: &Value,
) -> Result<Option<PendingPrimaryKeyBindingV1>> {
    let has_any = [
        "key_algo",
        "public_key",
        "public_key_bytes",
        "proof_type",
        "key_proof_type",
        "proof_payload",
        "key_proof_payload",
    ]
    .iter()
    .any(|field| param_as_string_any(params, &[*field]).is_some());
    if !has_any {
        return Ok(None);
    }
    let key_algo = parse_key_algo_v1(params)?;
    let public_key = parse_public_key_v1(params)?;
    let proof_type = parse_key_proof_type_v1(params)?;
    let proof_payload = parse_key_proof_payload_v1(params)?;
    Ok(Some(PendingPrimaryKeyBindingV1 {
        key_algo,
        public_key,
        proof_type,
        proof_payload,
    }))
}

fn parse_key_algo_v1(params: &Value) -> Result<UcaKeyAlgo> {
    let raw = param_as_string_any(params, &["key_algo"])
        .ok_or_else(|| anyhow::anyhow!("key_algo is required when key metadata is provided"))?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "secp256k1" => Ok(UcaKeyAlgo::Secp256k1),
        "ed25519" => Ok(UcaKeyAlgo::Ed25519),
        "mldsa87" | "mldsa" | "ml-dsa-87" | "ml_dsa_87" => Ok(UcaKeyAlgo::Mldsa87),
        other => bail!(
            "invalid key_algo: {}; valid: secp256k1|ed25519|mldsa87",
            other
        ),
    }
}

fn parse_public_key_v1(params: &Value) -> Result<Vec<u8>> {
    let raw =
        param_as_string_any(params, &["public_key", "public_key_bytes"]).ok_or_else(|| {
            anyhow::anyhow!("public_key/public_key_bytes is required when key metadata is provided")
        })?;
    decode_hex_bytes(&raw, "public_key")
}

fn parse_key_proof_type_v1(params: &Value) -> Result<UcaKeyProofType> {
    let raw = param_as_string_any(params, &["proof_type", "key_proof_type"]).ok_or_else(|| {
        anyhow::anyhow!("proof_type/key_proof_type is required when key metadata is provided")
    })?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "signature_v1" | "signature" | "ownership_signature_v1" => Ok(UcaKeyProofType::SignatureV1),
        other => bail!("invalid proof_type: {}; valid: signature_v1", other),
    }
}

fn parse_key_proof_payload_v1(params: &Value) -> Result<Vec<u8>> {
    let raw =
        param_as_string_any(params, &["proof_payload", "key_proof_payload"]).ok_or_else(|| {
            anyhow::anyhow!(
                "proof_payload/key_proof_payload is required when key metadata is provided"
            )
        })?;
    decode_hex_bytes(&raw, "proof_payload")
}

fn derive_primary_key_ref_from_binding_v1(key_algo: UcaKeyAlgo, public_key: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"uca-primary-key-ref-v2");
    hasher.update(key_algo.as_str().as_bytes());
    hasher.update([0u8]);
    hasher.update(public_key);
    hasher.finalize().to_vec()
}

fn primary_key_proof_message_v1(
    account_id: &str,
    action: &str,
    key_algo: UcaKeyAlgo,
    public_key: &[u8],
    primary_key_ref: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"novovm-uca-primary-key-proof-v1");
    out.push(0);
    out.extend_from_slice(action.as_bytes());
    out.push(0);
    out.extend_from_slice(account_id.as_bytes());
    out.push(0);
    out.extend_from_slice(key_algo.as_str().as_bytes());
    out.push(0);
    out.extend_from_slice(format!("0x{}", to_hex_lower(public_key)).as_bytes());
    out.push(0);
    out.extend_from_slice(format!("0x{}", to_hex_lower(primary_key_ref)).as_bytes());
    out
}

fn validate_public_key_format_v1(key_algo: UcaKeyAlgo, public_key: &[u8]) -> Result<()> {
    match key_algo {
        UcaKeyAlgo::Secp256k1 => {
            Secp256k1VerifyingKey::from_sec1_bytes(public_key)
                .context("invalid secp256k1 public key")?;
        }
        UcaKeyAlgo::Ed25519 => {
            let raw: [u8; 32] = public_key
                .try_into()
                .map_err(|_| anyhow::anyhow!("ed25519 public key must be 32 bytes"))?;
            Ed25519VerifyingKey::from_bytes(&raw).context("invalid ed25519 public key")?;
        }
        UcaKeyAlgo::Mldsa87 => {
            if public_key.is_empty() {
                bail!("mldsa87 public key must not be empty");
            }
        }
    }
    Ok(())
}

fn verify_primary_key_binding_proof_v1(
    account_id: &str,
    action: &str,
    key_algo: UcaKeyAlgo,
    public_key: &[u8],
    primary_key_ref: &[u8],
    proof_type: UcaKeyProofType,
    proof_payload: &[u8],
) -> Result<()> {
    match proof_type {
        UcaKeyProofType::SignatureV1 => {}
    }
    let message =
        primary_key_proof_message_v1(account_id, action, key_algo, public_key, primary_key_ref);
    match key_algo {
        UcaKeyAlgo::Secp256k1 => {
            let key = Secp256k1VerifyingKey::from_sec1_bytes(public_key)
                .context("invalid secp256k1 public key")?;
            let signature = Secp256k1Signature::try_from(proof_payload)
                .context("invalid secp256k1 signature_v1 proof payload")?;
            key.verify(message.as_slice(), &signature)
                .context("invalid secp256k1 proof signature")?;
        }
        UcaKeyAlgo::Ed25519 => {
            let key_raw: [u8; 32] = public_key
                .try_into()
                .map_err(|_| anyhow::anyhow!("ed25519 public key must be 32 bytes"))?;
            let key =
                Ed25519VerifyingKey::from_bytes(&key_raw).context("invalid ed25519 public key")?;
            let signature = Ed25519Signature::from_slice(proof_payload)
                .context("invalid ed25519 signature_v1 proof payload")?;
            key.verify(message.as_slice(), &signature)
                .context("invalid ed25519 proof signature")?;
        }
        UcaKeyAlgo::Mldsa87 => {
            let valid = mldsa_verify_v1_auto(87, public_key, message.as_slice(), proof_payload)
                .map_err(|e| anyhow::anyhow!("aoem mldsa87 verify failed: {}", e))?
                .ok_or_else(|| anyhow::anyhow!("aoem mldsa87 verify unavailable"))?;
            if !valid {
                bail!("invalid mldsa87 proof signature");
            }
        }
    }
    Ok(())
}

fn resolve_primary_key_binding_v1(
    params: &Value,
    account_id: &str,
    action: &str,
    now: u64,
    primary_key_ref_field: Option<&str>,
) -> Result<(Vec<u8>, Option<UcaPrimaryKeyBinding>)> {
    let pending = parse_optional_primary_key_binding_v1(params)?;
    let primary_key_ref_field = primary_key_ref_field.unwrap_or("primary_key_ref");
    let default_primary_key_ref_seed = if action == "rotate" {
        format!("{}:rotated:{}", account_id, now)
    } else {
        account_id.to_string()
    };
    let explicit_primary_key_ref = param_as_string_any(params, &[primary_key_ref_field])
        .map(|raw| decode_hex_bytes(&raw, primary_key_ref_field))
        .transpose()?;

    if let Some(binding) = pending {
        validate_public_key_format_v1(binding.key_algo, binding.public_key.as_slice())?;
        let derived_primary_key_ref =
            derive_primary_key_ref_from_binding_v1(binding.key_algo, binding.public_key.as_slice());
        if let Some(explicit) = explicit_primary_key_ref.as_ref() {
            if explicit.as_slice() != derived_primary_key_ref.as_slice() {
                bail!(
                    "{} does not match derived primary_key_ref for key_algo={}",
                    primary_key_ref_field,
                    binding.key_algo.as_str()
                );
            }
        }
        verify_primary_key_binding_proof_v1(
            account_id,
            action,
            binding.key_algo,
            binding.public_key.as_slice(),
            derived_primary_key_ref.as_slice(),
            binding.proof_type,
            binding.proof_payload.as_slice(),
        )?;
        return Ok((
            derived_primary_key_ref,
            Some(UcaPrimaryKeyBinding {
                key_algo: binding.key_algo,
                public_key: binding.public_key,
                proof_type: binding.proof_type,
                proof_payload: binding.proof_payload,
                verified_at: now,
            }),
        ));
    }

    Ok((
        explicit_primary_key_ref.unwrap_or(parse_primary_key_ref(
            params,
            &default_primary_key_ref_seed,
            primary_key_ref_field,
        )?),
        None,
    ))
}

fn parse_account_role(params: &Value) -> Result<AccountRole> {
    let raw = param_as_string_any(params, &["role"])
        .unwrap_or_else(|| "owner".to_string())
        .to_ascii_lowercase();
    match raw.as_str() {
        "owner" => Ok(AccountRole::Owner),
        "delegate" => Ok(AccountRole::Delegate),
        "session" | "sessionkey" | "session_key" => Ok(AccountRole::SessionKey),
        _ => bail!("invalid role: {}; valid: owner|delegate|session_key", raw),
    }
}

fn parse_persona_type_value(raw: &str) -> PersonaType {
    match raw.trim().to_ascii_lowercase().as_str() {
        "web30" => PersonaType::Web30,
        "evm" => PersonaType::Evm,
        "bitcoin" | "btc" => PersonaType::Bitcoin,
        "solana" | "sol" => PersonaType::Solana,
        other => PersonaType::Other(other.to_string()),
    }
}

fn parse_persona(params: &Value, allow_infer_type: bool) -> Result<PersonaAddress> {
    let persona_type = if let Some(raw) = param_as_string_any(params, &["persona_type"]) {
        parse_persona_type_value(&raw)
    } else if allow_infer_type {
        match parse_protocol_kind(params)? {
            ProtocolKind::Eth => PersonaType::Evm,
            ProtocolKind::Web30 => PersonaType::Web30,
            ProtocolKind::Other(other) => PersonaType::Other(other),
        }
    } else {
        bail!("persona_type is required");
    };
    let chain_id =
        param_as_u64(params, "chain_id").ok_or_else(|| anyhow::anyhow!("chain_id is required"))?;
    let address_raw = param_as_string_any(params, &["external_address", "from"])
        .ok_or_else(|| anyhow::anyhow!("external_address/from is required"))?;
    let external_address = decode_hex_bytes(&address_raw, "external_address")?;
    Ok(PersonaAddress {
        persona_type,
        chain_id,
        external_address,
    })
}

fn parse_protocol_kind(params: &Value) -> Result<ProtocolKind> {
    let raw = param_as_string_any(params, &["protocol"]).unwrap_or_else(|| "other".to_string());
    Ok(match raw.trim().to_ascii_lowercase().as_str() {
        "eth" => ProtocolKind::Eth,
        "web30" | "nov" | "novovm" => ProtocolKind::Web30,
        other => ProtocolKind::Other(other.to_string()),
    })
}

fn parse_nonce_scope(params: &Value) -> Result<NonceScope> {
    match param_as_string_any(params, &["nonce_scope"])
        .unwrap_or_else(|| "persona".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "persona" => Ok(NonceScope::Persona),
        "chain" => Ok(NonceScope::Chain),
        "global" => Ok(NonceScope::Global),
        other => bail!(
            "invalid nonce_scope: {}; valid: persona|chain|global",
            other
        ),
    }
}

fn parse_type4_policy_mode(
    params: &Value,
    allow_type4_with_delegate_or_session: bool,
) -> Result<Type4PolicyMode> {
    if let Some(raw) = param_as_string_any(params, &["type4_policy_mode"]) {
        return Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "supported" => Type4PolicyMode::Supported,
            "rejected" => Type4PolicyMode::Rejected,
            "degraded" => Type4PolicyMode::Degraded,
            other => bail!(
                "invalid type4_policy_mode: {}; valid: supported|rejected|degraded",
                other
            ),
        });
    }
    Ok(if allow_type4_with_delegate_or_session {
        Type4PolicyMode::Supported
    } else {
        Type4PolicyMode::Rejected
    })
}

fn parse_kyc_policy_mode(params: &Value) -> Result<KycPolicyMode> {
    if let Some(raw) = param_as_string_any(params, &["kyc_policy_mode"]) {
        return Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "disabled" => KycPolicyMode::Disabled,
            "informational" => KycPolicyMode::Informational,
            "required_non_owner" | "required-non-owner" | "requiredfornonowner" => {
                KycPolicyMode::RequiredForNonOwner
            }
            other => bail!(
                "invalid kyc_policy_mode: {}; valid: disabled|informational|required_non_owner",
                other
            ),
        });
    }
    Ok(KycPolicyMode::Disabled)
}

fn nonce_scope_label(scope: NonceScope) -> &'static str {
    match scope {
        NonceScope::Persona => "persona",
        NonceScope::Chain => "chain",
        NonceScope::Global => "global",
    }
}

fn type4_policy_mode_label(mode: Type4PolicyMode) -> &'static str {
    match mode {
        Type4PolicyMode::Supported => "supported",
        Type4PolicyMode::Rejected => "rejected",
        Type4PolicyMode::Degraded => "degraded",
    }
}

fn kyc_policy_mode_label(mode: KycPolicyMode) -> &'static str {
    match mode {
        KycPolicyMode::Disabled => "disabled",
        KycPolicyMode::Informational => "informational",
        KycPolicyMode::RequiredForNonOwner => "required_non_owner",
    }
}

fn default_signature_domain(persona: &PersonaAddress, protocol: &ProtocolKind) -> String {
    match protocol {
        ProtocolKind::Eth => format!("evm:{}", persona.chain_id),
        ProtocolKind::Web30 => "web30:mainnet".to_string(),
        ProtocolKind::Other(_) => format!("{}:{}", persona.persona_type.as_str(), persona.chain_id),
    }
}

fn route_decision_to_json(decision: &RouteDecision) -> Value {
    match decision {
        RouteDecision::FastPath => json!({"kind": "fast_path"}),
        RouteDecision::Adapter { chain_id } => json!({"kind": "adapter", "chain_id": chain_id}),
    }
}

fn account_policy_to_json(policy: &AccountPolicy) -> Value {
    json!({
        "nonce_scope": nonce_scope_label(policy.nonce_scope),
        "type4_policy_mode": type4_policy_mode_label(policy.type4_policy_mode),
        "allow_type4_with_delegate_or_session": policy.allow_type4_with_delegate_or_session,
        "kyc_policy_mode": kyc_policy_mode_label(policy.kyc_policy_mode),
    })
}

fn primary_key_binding_to_json(binding: &UcaPrimaryKeyBinding) -> Value {
    json!({
        "key_algo": binding.key_algo.as_str(),
        "public_key": format!("0x{}", to_hex_lower(&binding.public_key)),
        "proof_type": binding.proof_type.as_str(),
        "proof_payload": format!("0x{}", to_hex_lower(&binding.proof_payload)),
        "verified_at": binding.verified_at,
    })
}

fn uca_account_to_json(account: &UcaAccount) -> Value {
    let mut out = json!({
        "account_id": account.uca_id,
        "uca_id": account.uca_id,
        "primary_key_ref": format!("0x{}", to_hex_lower(&account.primary_key_ref)),
        "status": format!("{:?}", account.status).to_ascii_lowercase(),
        "created_at": account.created_at,
        "updated_at": account.updated_at,
    });
    if let Some(binding) = &account.primary_key_binding {
        out["primary_key_binding"] = primary_key_binding_to_json(binding);
        out["key_algo"] = json!(binding.key_algo.as_str());
    }
    out
}

fn persona_binding_to_json(binding: &PersonaBinding) -> Result<Value> {
    Ok(json!({
        "account_id": binding.uca_id,
        "uca_id": binding.uca_id,
        "persona_type": binding.persona_type.as_str(),
        "chain_id": binding.chain_id,
        "external_address": format!("0x{}", to_hex_lower(&binding.external_address)),
        "binding_state": format!("{:?}", binding.binding_state).to_ascii_lowercase(),
        "bound_at": binding.bound_at,
        "revoked_at": binding.revoked_at,
        "cooldown_until": binding.cooldown_until,
    }))
}

fn validate_uca_id_policy(uca_id_raw: &str) -> Result<String> {
    let uca_id = uca_id_raw.trim();
    if uca_id.is_empty() {
        bail!("uca_id must not be empty");
    }
    if uca_id.len() > 128 {
        bail!("uca_id too long: {} (max 128)", uca_id.len());
    }
    if uca_id.chars().all(|ch| ch.is_ascii_digit()) {
        let numeric = uca_id
            .parse::<u64>()
            .with_context(|| format!("uca_id numeric segment parse failed: {}", uca_id))?;
        const UCA_BUSINESS_SEGMENT_START: u64 = 1_000_000;
        if numeric < UCA_BUSINESS_SEGMENT_START {
            bail!(
                "uca_id in reserved numeric segment: {} (business segment starts at {})",
                numeric,
                UCA_BUSINESS_SEGMENT_START
            );
        }
    }
    Ok(uca_id.to_string())
}

fn query_store_path_parent(query_store_path: &Path) -> Option<&Path> {
    query_store_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
}

fn unified_account_store_path_for_backend(query_store_path: &Path, backend: &str) -> PathBuf {
    let default_name = match backend {
        UNIFIED_ACCOUNT_STORE_BACKEND_ROCKSDB => "novovm-unified-account-router.rocksdb",
        _ => "novovm-unified-account-router.bin",
    };
    query_store_path_parent(query_store_path)
        .map(|parent| parent.join(default_name))
        .unwrap_or_else(|| PathBuf::from("artifacts").join(default_name))
}

fn unified_account_store_backend_kind(params: &Value) -> String {
    param_as_string_any(
        params,
        &["unified_account_store_backend", "ua_store_backend"],
    )
    .or_else(|| string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_STORE_BACKEND"))
    .unwrap_or_else(|| UNIFIED_ACCOUNT_STORE_BACKEND_ROCKSDB.to_string())
    .trim()
    .to_ascii_lowercase()
}

fn resolve_unified_account_store(
    query_store_path: &Path,
    params: &Value,
) -> Result<UnifiedAccountStoreBackend> {
    let backend = unified_account_store_backend_kind(params);
    let path = param_as_string_any(params, &["unified_account_store_path", "ua_store_path"])
        .map(PathBuf::from)
        .or_else(|| string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_DB").map(PathBuf::from))
        .unwrap_or_else(|| unified_account_store_path_for_backend(query_store_path, &backend));
    match backend.as_str() {
        "rocksdb" => Ok(UnifiedAccountStoreBackend::RocksDb { path }),
        "bincode_file" | "file" | "bincode" => {
            if bool_env_default("NOVOVM_ALLOW_NON_PROD_UA_BACKEND", false) {
                Ok(UnifiedAccountStoreBackend::BincodeFile { path })
            } else {
                bail!(
                    "NOVOVM_UNIFIED_ACCOUNT_STORE_BACKEND={} is non-production; use rocksdb or set NOVOVM_ALLOW_NON_PROD_UA_BACKEND=1 for explicit override",
                    backend
                )
            }
        }
        _ => bail!(
            "invalid NOVOVM_UNIFIED_ACCOUNT_STORE_BACKEND={}; valid: rocksdb|bincode_file|file|bincode",
            backend
        ),
    }
}

impl UnifiedAccountStoreBackend {
    fn load_snapshot(&self) -> Result<UnifiedAccountStoreSnapshot> {
        match self {
            UnifiedAccountStoreBackend::BincodeFile { path } => {
                if !path.exists() {
                    return Ok(empty_unified_account_snapshot());
                }
                let raw = fs::read(path).with_context(|| {
                    format!("read unified account db failed: {}", path.display())
                })?;
                decode_unified_account_snapshot(&raw, path)
            }
            UnifiedAccountStoreBackend::RocksDb { path } => {
                let db = open_unified_account_rocksdb(path)?;
                let state_cf = db
                    .cf_handle(UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing unified account rocksdb column family '{}' for {}",
                            UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let audit_cf = db
                    .cf_handle(UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_AUDIT)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing unified account rocksdb column family '{}' for {}",
                            UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_AUDIT,
                            path.display()
                        )
                    })?;
                let router_raw = db
                    .get_cf(state_cf, UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_STATE_ROUTER)
                    .with_context(|| {
                        format!(
                            "read unified account rocksdb state key from cf '{}' failed: {}",
                            UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let cursor_raw = db
                    .get_cf(audit_cf, UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_AUDIT_CURSOR)
                    .with_context(|| {
                        format!(
                            "read unified account rocksdb audit cursor key from cf '{}' failed: {}",
                            UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_AUDIT,
                            path.display()
                        )
                    })?;
                let mapped_asset_raw = db
                    .get_cf(state_cf, UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_STATE_MAPPED_ASSET)
                    .with_context(|| {
                        format!(
                            "read unified account rocksdb mapped-asset state key from cf '{}' failed: {}",
                            UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                if let Some(router_bytes) = router_raw {
                    let router: UnifiedAccountRouter =
                        crate::bincode_compat::deserialize(&router_bytes).with_context(|| {
                            format!(
                                "decode unified account rocksdb state router failed: {}",
                                path.display()
                            )
                        })?;
                    let flushed_event_count = match cursor_raw {
                        Some(bytes) => decode_u64_be(&bytes).with_context(|| {
                            format!(
                                "decode unified account rocksdb audit cursor failed: {}",
                                path.display()
                            )
                        })?,
                        None => router.events().len() as u64,
                    };
                    let mapped_asset_state = match mapped_asset_raw {
                        Some(bytes) => {
                            crate::bincode_compat::deserialize(&bytes).with_context(|| {
                                format!(
                                    "decode unified account rocksdb mapped-asset state failed: {}",
                                    path.display()
                                )
                            })?
                        }
                        None => UnifiedMappedAssetState::default(),
                    };
                    return Ok(UnifiedAccountStoreSnapshot {
                        router,
                        flushed_event_count,
                        mapped_asset_state,
                    });
                }
                Ok(empty_unified_account_snapshot())
            }
        }
    }

    fn save_snapshot(&self, snapshot: &UnifiedAccountStoreSnapshot) -> Result<()> {
        match self {
            UnifiedAccountStoreBackend::BincodeFile { path } => {
                let encoded = encode_unified_account_snapshot(snapshot)?;
                ensure_parent_dir(path, "unified account db")?;
                fs::write(path, encoded).with_context(|| {
                    format!("write unified account db failed: {}", path.display())
                })?;
                Ok(())
            }
            UnifiedAccountStoreBackend::RocksDb { path } => {
                let db = open_unified_account_rocksdb(path)?;
                let state_cf = db
                    .cf_handle(UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing unified account rocksdb column family '{}' for {}",
                            UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let audit_cf = db
                    .cf_handle(UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_AUDIT)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing unified account rocksdb column family '{}' for {}",
                            UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_AUDIT,
                            path.display()
                        )
                    })?;
                let router_encoded = crate::bincode_compat::serialize(&snapshot.router)
                    .with_context(|| {
                        format!(
                            "serialize unified account rocksdb state router failed: {}",
                            path.display()
                        )
                    })?;
                let mapped_asset_encoded = crate::bincode_compat::serialize(
                    &snapshot.mapped_asset_state,
                )
                .with_context(|| {
                    format!(
                        "serialize unified account rocksdb mapped-asset state failed: {}",
                        path.display()
                    )
                })?;
                let mut batch = RocksDbWriteBatch::default();
                batch.put_cf(
                    state_cf,
                    UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_STATE_ROUTER,
                    router_encoded.as_slice(),
                );
                batch.put_cf(
                    state_cf,
                    UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_STATE_MAPPED_ASSET,
                    mapped_asset_encoded.as_slice(),
                );
                batch.put_cf(
                    audit_cf,
                    UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_AUDIT_CURSOR,
                    snapshot.flushed_event_count.to_be_bytes(),
                );
                db.write(batch).with_context(|| {
                    format!(
                        "write unified account rocksdb namespace batch failed: {}",
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }
}

fn empty_unified_account_snapshot() -> UnifiedAccountStoreSnapshot {
    UnifiedAccountStoreSnapshot {
        router: UnifiedAccountRouter::new(),
        flushed_event_count: 0,
        mapped_asset_state: UnifiedMappedAssetState::default(),
    }
}

fn decode_unified_account_snapshot(raw: &[u8], path: &Path) -> Result<UnifiedAccountStoreSnapshot> {
    if raw.is_empty() {
        return Ok(empty_unified_account_snapshot());
    }
    if let Ok(envelope) = crate::bincode_compat::deserialize::<UnifiedAccountStoreEnvelopeV2>(raw) {
        if envelope.version == UNIFIED_ACCOUNT_STORE_ENVELOPE_VERSION_V2 {
            return Ok(UnifiedAccountStoreSnapshot {
                router: envelope.router,
                flushed_event_count: envelope.flushed_event_count,
                mapped_asset_state: envelope.mapped_asset_state,
            });
        }
    }
    if let Ok(envelope) = crate::bincode_compat::deserialize::<UnifiedAccountStoreEnvelopeV1>(raw) {
        if envelope.version != UNIFIED_ACCOUNT_STORE_ENVELOPE_VERSION_V1 {
            bail!(
                "unsupported unified account db version {} at {}",
                envelope.version,
                path.display()
            );
        }
        return Ok(UnifiedAccountStoreSnapshot {
            router: envelope.router,
            flushed_event_count: envelope.flushed_event_count,
            mapped_asset_state: UnifiedMappedAssetState::default(),
        });
    }
    let legacy_router: UnifiedAccountRouter = crate::bincode_compat::deserialize(raw)
        .with_context(|| format!("parse unified account db failed: {}", path.display()))?;
    Ok(UnifiedAccountStoreSnapshot {
        flushed_event_count: legacy_router.events().len() as u64,
        router: legacy_router,
        mapped_asset_state: UnifiedMappedAssetState::default(),
    })
}

fn encode_unified_account_snapshot(snapshot: &UnifiedAccountStoreSnapshot) -> Result<Vec<u8>> {
    #[derive(Serialize)]
    struct UnifiedAccountStoreEnvelopeRefV2<'a> {
        version: u32,
        router: &'a UnifiedAccountRouter,
        flushed_event_count: u64,
        mapped_asset_state: &'a UnifiedMappedAssetState,
    }

    let envelope = UnifiedAccountStoreEnvelopeRefV2 {
        version: UNIFIED_ACCOUNT_STORE_ENVELOPE_VERSION_V2,
        router: &snapshot.router,
        flushed_event_count: snapshot.flushed_event_count,
        mapped_asset_state: &snapshot.mapped_asset_state,
    };
    crate::bincode_compat::serialize(&envelope).context("serialize unified account router failed")
}

fn ensure_parent_dir(path: &Path, label: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create {} parent dir failed: {}", label, parent.display())
            })?;
        }
    }
    Ok(())
}

fn open_unified_account_rocksdb(path: &Path) -> Result<RocksDb> {
    ensure_parent_dir(path, "unified account rocksdb")?;
    let mut opts = RocksDbOptions::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);
    let mut cf_names =
        RocksDb::list_cf(&opts, path).unwrap_or_else(|_| vec!["default".to_string()]);
    if cf_names.is_empty() {
        cf_names.push("default".to_string());
    }
    for required in [
        UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE,
        UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_AUDIT,
    ] {
        if !cf_names.iter().any(|name| name == required) {
            cf_names.push(required.to_string());
        }
    }
    RocksDb::open_cf(&opts, path, cf_names)
        .with_context(|| format!("open unified account rocksdb failed: {}", path.display()))
}

fn unified_account_audit_log_path(query_store_path: &Path) -> PathBuf {
    if let Some(custom) = string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_AUDIT_LOG") {
        return PathBuf::from(custom);
    }
    if let Some(custom_dir) = string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_AUDIT_DIR") {
        return PathBuf::from(custom_dir).join(UNIFIED_ACCOUNT_AUDIT_LOG_NAME);
    }
    query_store_path_parent(query_store_path)
        .map(|parent| {
            parent
                .join("migration")
                .join("unifiedaccount")
                .join(UNIFIED_ACCOUNT_AUDIT_LOG_NAME)
        })
        .unwrap_or_else(|| {
            PathBuf::from("artifacts/migration/unifiedaccount").join(UNIFIED_ACCOUNT_AUDIT_LOG_NAME)
        })
}

fn unified_account_audit_db_path(query_store_path: &Path) -> PathBuf {
    if let Some(custom) = string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_AUDIT_DB") {
        return PathBuf::from(custom);
    }
    if let Some(custom_dir) = string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_AUDIT_DIR") {
        return PathBuf::from(custom_dir).join(UNIFIED_ACCOUNT_AUDIT_DB_NAME);
    }
    query_store_path_parent(query_store_path)
        .map(|parent| {
            parent
                .join("migration")
                .join("unifiedaccount")
                .join(UNIFIED_ACCOUNT_AUDIT_DB_NAME)
        })
        .unwrap_or_else(|| {
            PathBuf::from("artifacts/migration/unifiedaccount").join(UNIFIED_ACCOUNT_AUDIT_DB_NAME)
        })
}

fn unified_account_audit_backend_kind(params: &Value) -> String {
    param_as_string_any(
        params,
        &["unified_account_audit_backend", "ua_audit_backend"],
    )
    .or_else(|| string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_AUDIT_BACKEND"))
    .unwrap_or_else(|| UNIFIED_ACCOUNT_AUDIT_BACKEND_ROCKSDB.to_string())
    .trim()
    .to_ascii_lowercase()
}

fn resolve_unified_account_audit_sink(
    query_store_path: &Path,
    params: &Value,
) -> Result<UnifiedAccountAuditSinkBackend> {
    let backend = unified_account_audit_backend_kind(params);
    let path = param_as_string_any(params, &["unified_account_audit_path", "ua_audit_path"])
        .map(PathBuf::from)
        .or_else(|| {
            if backend == UNIFIED_ACCOUNT_AUDIT_BACKEND_ROCKSDB {
                string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_AUDIT_DB").map(PathBuf::from)
            } else {
                string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_AUDIT_LOG").map(PathBuf::from)
            }
        })
        .unwrap_or_else(|| {
            if backend == UNIFIED_ACCOUNT_AUDIT_BACKEND_ROCKSDB {
                unified_account_audit_db_path(query_store_path)
            } else {
                unified_account_audit_log_path(query_store_path)
            }
        });
    match backend.as_str() {
        "rocksdb" => Ok(UnifiedAccountAuditSinkBackend::RocksDb { path }),
        "jsonl" | "file" => {
            if bool_env_default("NOVOVM_ALLOW_NON_PROD_UA_BACKEND", false) {
                Ok(UnifiedAccountAuditSinkBackend::JsonlFile { path })
            } else {
                bail!(
                    "NOVOVM_UNIFIED_ACCOUNT_AUDIT_BACKEND={} is non-production; use rocksdb or set NOVOVM_ALLOW_NON_PROD_UA_BACKEND=1 for explicit override",
                    backend
                )
            }
        }
        _ => bail!(
            "invalid NOVOVM_UNIFIED_ACCOUNT_AUDIT_BACKEND={}; valid: rocksdb|jsonl|file",
            backend
        ),
    }
}

impl UnifiedAccountAuditSinkBackend {
    fn backend_name(&self) -> &'static str {
        match self {
            UnifiedAccountAuditSinkBackend::JsonlFile { .. } => UNIFIED_ACCOUNT_AUDIT_BACKEND_JSONL,
            UnifiedAccountAuditSinkBackend::RocksDb { .. } => UNIFIED_ACCOUNT_AUDIT_BACKEND_ROCKSDB,
        }
    }

    fn path(&self) -> &Path {
        match self {
            UnifiedAccountAuditSinkBackend::JsonlFile { path } => path.as_path(),
            UnifiedAccountAuditSinkBackend::RocksDb { path } => path.as_path(),
        }
    }

    fn append_record(&self, record: &UnifiedAccountAuditSinkRecord) -> Result<()> {
        match self {
            UnifiedAccountAuditSinkBackend::JsonlFile { path } => {
                append_unified_account_audit_record(path, record)
            }
            UnifiedAccountAuditSinkBackend::RocksDb { path } => {
                append_unified_account_audit_record_rocksdb(path, record)
            }
        }
    }
}

fn append_unified_account_audit_record(
    path: &Path,
    record: &UnifiedAccountAuditSinkRecord,
) -> Result<()> {
    ensure_parent_dir(path, "unified account audit log")?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open unified account audit log failed: {}", path.display()))?;
    let encoded =
        serde_json::to_string(record).context("serialize unified account audit record")?;
    writeln!(file, "{encoded}")
        .with_context(|| format!("write unified account audit log failed: {}", path.display()))?;
    Ok(())
}

fn decode_u64_be(bytes: &[u8]) -> Result<u64> {
    if bytes.len() != 8 {
        bail!("invalid u64 bytes length: expected 8, got {}", bytes.len());
    }
    let mut out = [0u8; 8];
    out.copy_from_slice(bytes);
    Ok(u64::from_be_bytes(out))
}

fn unified_account_audit_rocksdb_event_key(seq: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_EVENT_PREFIX.len() + 8);
    out.extend_from_slice(UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_EVENT_PREFIX);
    out.extend_from_slice(&seq.to_be_bytes());
    out
}

fn open_unified_account_audit_rocksdb(path: &Path) -> Result<RocksDb> {
    ensure_parent_dir(path, "unified account audit rocksdb")?;
    let mut opts = RocksDbOptions::default();
    opts.create_if_missing(true);
    RocksDb::open(&opts, path).with_context(|| {
        format!(
            "open unified account audit rocksdb failed: {}",
            path.display()
        )
    })
}

fn append_unified_account_audit_record_rocksdb(
    path: &Path,
    record: &UnifiedAccountAuditSinkRecord,
) -> Result<()> {
    let db = open_unified_account_audit_rocksdb(path)?;
    let current_seq = match db
        .get(UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_SEQ)
        .with_context(|| {
            format!(
                "read unified account audit rocksdb sequence failed: {}",
                path.display()
            )
        })? {
        Some(raw) => decode_u64_be(&raw).with_context(|| {
            format!(
                "decode unified account audit rocksdb sequence failed: {}",
                path.display()
            )
        })?,
        None => 0,
    };
    let next_seq = current_seq.saturating_add(1);
    let event_key = unified_account_audit_rocksdb_event_key(next_seq);
    let event_value = serde_json::to_vec(record)
        .context("serialize unified account audit record for rocksdb failed")?;
    let mut batch = RocksDbWriteBatch::default();
    batch.put(
        UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_SEQ,
        next_seq.to_be_bytes(),
    );
    batch.put(event_key, event_value);
    db.write(batch).with_context(|| {
        format!(
            "write unified account audit rocksdb batch failed: {}",
            path.display()
        )
    })?;
    Ok(())
}

fn unified_account_events_since(
    router: &UnifiedAccountRouter,
    cursor: u64,
) -> (Vec<AccountAuditEvent>, u64) {
    let events = router.events();
    let start = (cursor as usize).min(events.len());
    (events[start..].to_vec(), events.len() as u64)
}

fn account_audit_event_kind(event: &AccountAuditEvent) -> &'static str {
    match event {
        AccountAuditEvent::UcaCreated { .. } => "uca_created",
        AccountAuditEvent::BindingAdded { .. } => "binding_added",
        AccountAuditEvent::BindingConflictRejected { .. } => "binding_conflict_rejected",
        AccountAuditEvent::BindingRevoked { .. } => "binding_revoked",
        AccountAuditEvent::NonceReplayRejected { .. } => "nonce_replay_rejected",
        AccountAuditEvent::DomainMismatchRejected { .. } => "domain_mismatch_rejected",
        AccountAuditEvent::PermissionDenied { .. } => "permission_denied",
        AccountAuditEvent::KeyRotated { .. } => "key_rotated",
        AccountAuditEvent::SessionKeyExpired { .. } => "session_key_expired",
        AccountAuditEvent::Type4PolicyRejected { .. } => "type4_policy_rejected",
        AccountAuditEvent::Type4PolicyDegraded { .. } => "type4_policy_degraded",
        AccountAuditEvent::KycAttestationObserved { .. } => "kyc_attestation_observed",
        AccountAuditEvent::KycPolicyRejected { .. } => "kyc_policy_rejected",
    }
}

fn account_audit_event_uca_id(event: &AccountAuditEvent) -> &str {
    match event {
        AccountAuditEvent::UcaCreated { uca_id, .. }
        | AccountAuditEvent::BindingAdded { uca_id, .. }
        | AccountAuditEvent::BindingRevoked { uca_id, .. }
        | AccountAuditEvent::NonceReplayRejected { uca_id, .. }
        | AccountAuditEvent::DomainMismatchRejected { uca_id, .. }
        | AccountAuditEvent::PermissionDenied { uca_id, .. }
        | AccountAuditEvent::KeyRotated { uca_id, .. }
        | AccountAuditEvent::SessionKeyExpired { uca_id, .. }
        | AccountAuditEvent::Type4PolicyRejected { uca_id, .. }
        | AccountAuditEvent::Type4PolicyDegraded { uca_id, .. }
        | AccountAuditEvent::KycAttestationObserved { uca_id, .. }
        | AccountAuditEvent::KycPolicyRejected { uca_id, .. } => uca_id.as_str(),
        AccountAuditEvent::BindingConflictRejected { request_uca_id, .. } => {
            request_uca_id.as_str()
        }
    }
}

fn account_audit_event_code(event: &AccountAuditEvent) -> &'static str {
    match event {
        AccountAuditEvent::UcaCreated { .. } => "UA_AUDIT_UCA_CREATED",
        AccountAuditEvent::BindingAdded { .. } => "UA_AUDIT_BINDING_ADDED",
        AccountAuditEvent::BindingConflictRejected { .. } => "UA_AUDIT_BINDING_CONFLICT_REJECTED",
        AccountAuditEvent::BindingRevoked { .. } => "UA_AUDIT_BINDING_REVOKED",
        AccountAuditEvent::NonceReplayRejected { .. } => "UA_AUDIT_NONCE_REPLAY_REJECTED",
        AccountAuditEvent::DomainMismatchRejected { .. } => "UA_AUDIT_DOMAIN_MISMATCH_REJECTED",
        AccountAuditEvent::PermissionDenied { .. } => "UA_AUDIT_PERMISSION_DENIED",
        AccountAuditEvent::KeyRotated { .. } => "UA_AUDIT_KEY_ROTATED",
        AccountAuditEvent::SessionKeyExpired { .. } => "UA_AUDIT_SESSION_KEY_EXPIRED",
        AccountAuditEvent::Type4PolicyRejected { .. } => "UA_AUDIT_TYPE4_POLICY_REJECTED",
        AccountAuditEvent::Type4PolicyDegraded { .. } => "UA_AUDIT_TYPE4_POLICY_DEGRADED",
        AccountAuditEvent::KycAttestationObserved { .. } => "UA_AUDIT_KYC_ATTESTATION_OBSERVED",
        AccountAuditEvent::KycPolicyRejected { .. } => "UA_AUDIT_KYC_POLICY_REJECTED",
    }
}

fn account_audit_event_key_algo(event: &AccountAuditEvent) -> Option<&'static str> {
    match event {
        AccountAuditEvent::UcaCreated {
            key_algo: Some(key_algo),
            ..
        }
        | AccountAuditEvent::KeyRotated {
            key_algo: Some(key_algo),
            ..
        } => Some(key_algo.as_str()),
        _ => None,
    }
}

fn account_audit_event_to_json(event: &AccountAuditEvent) -> Result<Value> {
    let mut value = serde_json::to_value(event)
        .context("serialize unified account audit event to json failed")?;
    if let Value::Object(map) = &mut value {
        map.insert(
            "event_kind".to_string(),
            json!(account_audit_event_kind(event)),
        );
        map.insert(
            "event_code".to_string(),
            json!(account_audit_event_code(event)),
        );
        map.insert(
            "uca_id".to_string(),
            json!(account_audit_event_uca_id(event)),
        );
        if let Some(key_algo) = account_audit_event_key_algo(event) {
            map.insert("key_algo".to_string(), json!(key_algo));
        }
        return Ok(value);
    }
    Ok(json!({
        "event_kind": account_audit_event_kind(event),
        "event_code": account_audit_event_code(event),
        "uca_id": account_audit_event_uca_id(event),
        "event": value,
    }))
}

fn decode_unified_account_audit_record_json(
    raw: &[u8],
    path: &Path,
    seq: u64,
) -> Result<UnifiedAccountAuditSinkRecord> {
    serde_json::from_slice(raw).with_context(|| {
        format!(
            "decode unified account audit record failed: path={} seq={}",
            path.display(),
            seq
        )
    })
}

fn unified_account_audit_record_to_json(
    seq: u64,
    record: &UnifiedAccountAuditSinkRecord,
) -> Result<Value> {
    let router_events_json = record
        .router_events
        .iter()
        .map(account_audit_event_to_json)
        .collect::<Result<Vec<_>>>()?;
    let mut value = serde_json::to_value(record)
        .context("serialize unified account audit record to json failed")?;
    if let Value::Object(map) = &mut value {
        map.insert("seq".to_string(), json!(seq));
        map.insert(
            "router_events".to_string(),
            Value::Array(router_events_json),
        );
        return Ok(value);
    }
    Ok(json!({
        "seq": seq,
        "router_events": router_events_json,
        "record": value,
    }))
}

fn load_unified_account_audit_records_all(
    sink: &UnifiedAccountAuditSinkBackend,
) -> Result<Vec<(u64, UnifiedAccountAuditSinkRecord)>> {
    match sink {
        UnifiedAccountAuditSinkBackend::JsonlFile { path } => {
            if !path.exists() {
                return Ok(Vec::new());
            }
            let file = fs::File::open(path).with_context(|| {
                format!(
                    "open unified account audit jsonl for full load failed: {}",
                    path.display()
                )
            })?;
            let reader = BufReader::new(file);
            let mut seq = 0u64;
            let mut out = Vec::new();
            for line in reader.lines() {
                let line = line.with_context(|| {
                    format!(
                        "read unified account audit jsonl line failed: {}",
                        path.display()
                    )
                })?;
                if line.trim().is_empty() {
                    continue;
                }
                seq = seq.saturating_add(1);
                out.push((
                    seq,
                    decode_unified_account_audit_record_json(line.as_bytes(), path, seq)?,
                ));
            }
            Ok(out)
        }
        UnifiedAccountAuditSinkBackend::RocksDb { path } => {
            let db = open_unified_account_audit_rocksdb(path)?;
            let head_seq =
                match db
                    .get(UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_SEQ)
                    .with_context(|| {
                        format!(
                            "read unified account audit rocksdb sequence failed: {}",
                            path.display()
                        )
                    })? {
                    Some(raw) => decode_u64_be(&raw).with_context(|| {
                        format!(
                            "decode unified account audit rocksdb sequence failed: {}",
                            path.display()
                        )
                    })?,
                    None => 0,
                };
            let mut out = Vec::new();
            for seq in 1..=head_seq {
                let key = unified_account_audit_rocksdb_event_key(seq);
                let raw = db.get(&key).with_context(|| {
                    format!(
                        "read unified account audit rocksdb event failed: {} seq={}",
                        path.display(),
                        seq
                    )
                })?;
                let Some(bytes) = raw else {
                    continue;
                };
                out.push((
                    seq,
                    decode_unified_account_audit_record_json(&bytes, path, seq)?,
                ));
            }
            Ok(out)
        }
    }
}

fn load_unified_account_audit_records_for_rpc(
    sink: &UnifiedAccountAuditSinkBackend,
    since_seq: u64,
    limit: usize,
) -> Result<(u64, Vec<Value>, u64, bool)> {
    let records = load_unified_account_audit_records_all(sink)?;
    let head_seq = records.last().map(|(seq, _)| *seq).unwrap_or(0);
    let mut filtered = Vec::new();
    for (seq, record) in records {
        if seq <= since_seq {
            continue;
        }
        filtered.push((seq, unified_account_audit_record_to_json(seq, &record)?));
    }
    let has_more = filtered.len() > limit;
    if has_more {
        filtered.truncate(limit);
    }
    let next_since_seq = filtered.last().map(|(seq, _)| *seq).unwrap_or(since_seq);
    let events = filtered.into_iter().map(|(_, event)| event).collect();
    Ok((head_seq, events, next_since_seq, has_more))
}

fn now_unix_sec() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mainline_query::run_mainline_query_from_path;
    use crate::tx_ingress::{
        load_nov_native_execution_store_v1, save_nov_native_execution_store_v1,
        NovCreditVaultStateV1, NovTreasurySettlementJournalEntryV1,
    };
    use aoem_bindings::{default_host_dll_path, mldsa_keygen_v1_auto, mldsa_sign_v1_auto};
    use ed25519_dalek::{Signer as Ed25519Signer, SigningKey as Ed25519SigningKey};
    use k256::ecdsa::SigningKey as Secp256k1SigningKey;
    use std::collections::BTreeMap;

    static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn temp_paths(label: &str) -> (PathBuf, PathBuf, PathBuf) {
        let mut root = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be >= epoch")
            .as_nanos();
        root.push(format!("novovm-ua-mainline-{}-{}", label, nonce));
        let canonical = root.join("canonical.json");
        let store = root.join("ua-store.rocksdb");
        let audit = root.join("ua-audit.rocksdb");
        (canonical, store, audit)
    }

    fn params_with_paths(store: &Path, audit: &Path, extra: Value) -> Value {
        let mut map = match extra {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        map.insert(
            "unified_account_store_path".to_string(),
            Value::String(store.display().to_string()),
        );
        map.insert(
            "unified_account_audit_path".to_string(),
            Value::String(audit.display().to_string()),
        );
        Value::Object(map)
    }

    fn params_with_paths_and_native_store(
        store: &Path,
        audit: &Path,
        native_store: &Path,
        extra: Value,
    ) -> Value {
        let mut out = match params_with_paths(store, audit, extra) {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        out.insert(
            "native_execution_store_path".to_string(),
            Value::String(native_store.display().to_string()),
        );
        Value::Object(out)
    }

    fn ua_hex(byte: u8, bytes: usize) -> String {
        format!("0x{}", format!("{:02x}", byte).repeat(bytes))
    }

    fn run_query(base: &Path, method: &str, params: Value) -> Value {
        run_mainline_query_from_path(base, method, &params)
            .unwrap_or_else(|err| panic!("{method} should succeed: {err}"))
    }

    fn run_query_err(base: &Path, method: &str, params: Value) -> String {
        run_mainline_query_from_path(base, method, &params)
            .expect_err("query should fail")
            .to_string()
    }

    fn ua_create(base: &Path, store: &Path, audit: &Path, account_id: &str, now: u64) {
        let _ = run_query(
            base,
            "ua_createUca",
            params_with_paths(
                store,
                audit,
                json!({
                    "account_id": account_id,
                    "primary_key_ref": ua_hex(0x66, 32),
                    "now": now,
                }),
            ),
        );
    }

    fn ed25519_key_binding_params(account_id: &str, action: &str) -> Value {
        let signing = Ed25519SigningKey::from_bytes(&[0x11u8; 32]);
        let public_key = signing.verifying_key().to_bytes().to_vec();
        let primary_key_ref =
            derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Ed25519, public_key.as_slice());
        let message = primary_key_proof_message_v1(
            account_id,
            action,
            UcaKeyAlgo::Ed25519,
            public_key.as_slice(),
            primary_key_ref.as_slice(),
        );
        let proof = signing.sign(message.as_slice()).to_bytes().to_vec();
        json!({
            "key_algo": "ed25519",
            "public_key": format!("0x{}", to_hex_lower(&public_key)),
            "proof_type": "signature_v1",
            "proof_payload": format!("0x{}", to_hex_lower(&proof)),
            "primary_key_ref": format!("0x{}", to_hex_lower(&primary_key_ref)),
        })
    }

    fn secp256k1_key_binding_params(account_id: &str, action: &str) -> Value {
        let signing =
            Secp256k1SigningKey::from_bytes((&[0x22u8; 32]).into()).expect("parse secp key");
        let public_key = signing
            .verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec();
        let primary_key_ref =
            derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Secp256k1, public_key.as_slice());
        let message = primary_key_proof_message_v1(
            account_id,
            action,
            UcaKeyAlgo::Secp256k1,
            public_key.as_slice(),
            primary_key_ref.as_slice(),
        );
        let proof: Secp256k1Signature = signing.sign(message.as_slice());
        let proof = proof.to_bytes().to_vec();
        json!({
            "key_algo": "secp256k1",
            "public_key": format!("0x{}", to_hex_lower(&public_key)),
            "proof_type": "signature_v1",
            "proof_payload": format!("0x{}", to_hex_lower(&proof)),
            "primary_key_ref": format!("0x{}", to_hex_lower(&primary_key_ref)),
        })
    }

    fn mldsa87_key_binding_params(account_id: &str, action: &str) -> Value {
        let aoem_dll = default_host_dll_path();
        assert!(
            aoem_dll.exists(),
            "AOEM DLL should exist for mldsa key binding test at {}",
            aoem_dll.display()
        );
        let (public_key, secret_key) = mldsa_keygen_v1_auto(87)
            .expect("mldsa87 keygen should run")
            .expect("mldsa87 keygen should be available");
        let primary_key_ref =
            derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa87, public_key.as_slice());
        let message = primary_key_proof_message_v1(
            account_id,
            action,
            UcaKeyAlgo::Mldsa87,
            public_key.as_slice(),
            primary_key_ref.as_slice(),
        );
        let proof = mldsa_sign_v1_auto(87, secret_key.as_slice(), message.as_slice())
            .expect("mldsa87 sign should run")
            .expect("mldsa87 sign should be available");
        json!({
            "key_algo": "mldsa87",
            "public_key": format!("0x{}", to_hex_lower(&public_key)),
            "proof_type": "signature_v1",
            "proof_payload": format!("0x{}", to_hex_lower(&proof)),
            "primary_key_ref": format!("0x{}", to_hex_lower(&primary_key_ref)),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn ua_bind(
        base: &Path,
        store: &Path,
        audit: &Path,
        account_id: &str,
        role: &str,
        persona_type: &str,
        chain_id: u64,
        external_address: &str,
        now: u64,
    ) {
        let _ = run_query(
            base,
            "ua_bindPersona",
            params_with_paths(
                store,
                audit,
                json!({
                    "account_id": account_id,
                    "role": role,
                    "persona_type": persona_type,
                    "chain_id": chain_id,
                    "external_address": external_address,
                    "now": now,
                }),
            ),
        );
    }

    fn mapped_lock_proof_params(account_id: &str, lock_byte: u8, amount: u128) -> Value {
        let proof_template = MappedAssetLockProof {
            lock_id: [lock_byte; 32],
            source_chain: MappedAssetSourceChain::Ethereum,
            source_asset_symbol: "ETH".to_string(),
            source_tx_hash: vec![lock_byte.saturating_add(1); 32],
            source_lock_ref: vec![0xaa, lock_byte],
            external_owner_ref: vec![lock_byte.saturating_add(2); 20],
            target_account_id: account_id.to_string(),
            amount,
            proof_payload: Vec::new(),
            proof_format: MappedLockProofFormat::EthereumLockEventV1,
        };
        let proof_digest = mapped_lock_proof_digest_v1(&proof_template);
        json!({
            "lock_id": mapped_asset_hex_id(&proof_template.lock_id),
            "source_chain": "ethereum",
            "source_asset_symbol": "ETH",
            "source_tx_hash": format!("0x{}", to_hex_lower(&proof_template.source_tx_hash)),
            "source_lock_ref": format!("0x{}", to_hex_lower(&proof_template.source_lock_ref)),
            "external_owner_ref": format!("0x{}", to_hex_lower(&proof_template.external_owner_ref)),
            "target_account_id": account_id,
            "amount": amount.to_string(),
            "proof_format": "ethereum_lock_event_v1",
            "proof_payload": mapped_asset_hex_id(&proof_digest),
        })
    }

    fn mapped_lock_live_event_proof_params(account_id: &str, lock_byte: u8, amount: u128) -> Value {
        let contract_address = [0x11u8; 20];
        let topic0 = eth_lock_event_topic0_v1();
        let mut proof_template = MappedAssetLockProof {
            lock_id: [lock_byte; 32],
            source_chain: MappedAssetSourceChain::Ethereum,
            source_asset_symbol: "ETH".to_string(),
            source_tx_hash: vec![lock_byte.saturating_add(1); 32],
            source_lock_ref: Vec::new(),
            external_owner_ref: vec![lock_byte.saturating_add(2); 20],
            target_account_id: account_id.to_string(),
            amount,
            proof_payload: Vec::new(),
            proof_format: MappedLockProofFormat::EthereumLockEventV1,
        };
        let evidence = EthereumLockEventEvidenceV1 {
            contract_address,
            topic0,
            block_number: 100,
            finalized_block_number: 112,
            log_index: u64::from(lock_byte),
        };
        proof_template.source_lock_ref =
            ethereum_lock_event_ref_digest_v1(&proof_template, &evidence).to_vec();
        let proof_digest = mapped_lock_proof_digest_v1(&proof_template);
        json!({
            "lock_id": mapped_asset_hex_id(&proof_template.lock_id),
            "source_chain": "ethereum",
            "source_asset_symbol": "ETH",
            "source_tx_hash": format!("0x{}", to_hex_lower(&proof_template.source_tx_hash)),
            "source_lock_ref": format!("0x{}", to_hex_lower(&proof_template.source_lock_ref)),
            "external_owner_ref": format!("0x{}", to_hex_lower(&proof_template.external_owner_ref)),
            "target_account_id": account_id,
            "amount": amount.to_string(),
            "proof_format": "ethereum_lock_event_v1",
            "proof_payload": mapped_asset_hex_id(&proof_digest),
            "lock_contract_address": format!("0x{}", to_hex_lower(&contract_address)),
            "expected_lock_contract_address": format!("0x{}", to_hex_lower(&contract_address)),
            "event_topic0": mapped_asset_hex_id(&topic0),
            "block_number": evidence.block_number,
            "finalized_block_number": evidence.finalized_block_number,
            "log_index": evidence.log_index,
        })
    }

    fn ensure_native_store(native_store: &Path) {
        let store = load_nov_native_execution_store_v1(native_store)
            .expect("load native execution store for mapped-asset tests");
        save_nov_native_execution_store_v1(native_store, &store)
            .expect("save native execution store for mapped-asset tests");
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    #[test]
    fn mainline_query_recognizes_unified_account_methods() {
        for method in [
            "ua_createUca",
            "ua_rotatePrimaryKey",
            "ua_setPolicy",
            "ua_bindPersona",
            "ua_revokePersona",
            "ua_getBindingOwner",
            "ua_getAuditEvents",
            "ua_getAccount",
            "ua_getPolicy",
            "ua_listBindings",
            "ua_getNextNonce",
            "ua_checkRoute",
            "ua_route",
            "ua_registerMappedLock",
            "ua_getMappedAsset",
            "ua_burnMappedAsset",
            "ua_releaseMappedLock",
            "account_balance",
            "account_assets",
        ] {
            assert!(is_mainline_unified_account_query_method(method));
        }
    }

    #[test]
    fn unified_account_surface_executes_via_real_mainline_entry() {
        let (base, store, audit) = temp_paths("entry");
        let out = run_query(
            &base,
            "ua_createUca",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "acct-entry-a",
                    "primary_key_ref": ua_hex(0x55, 32),
                    "now": 10,
                }),
            ),
        );
        assert_eq!(out["method"].as_str(), Some("ua_createUca"));
        assert_eq!(out["account_id"].as_str(), Some("acct-entry-a"));
        let _ = fs::remove_dir_all(base.parent().unwrap_or_else(|| Path::new(".")));
    }

    #[test]
    fn unified_account_surface_cut_a_ed25519_key_binding_persists_metadata() {
        let (base, store, audit) = temp_paths("cut-a-ed25519");
        let mut params = match ed25519_key_binding_params("acct-key-ed25519", "create") {
            Value::Object(map) => map,
            other => panic!("expected object params, got {other:?}"),
        };
        params.insert(
            "account_id".to_string(),
            Value::String("acct-key-ed25519".to_string()),
        );
        params.insert("now".to_string(), Value::from(10u64));
        let out = run_query(
            &base,
            "ua_createUca",
            params_with_paths(&store, &audit, Value::Object(params)),
        );
        assert_eq!(out["created"].as_bool(), Some(true));
        assert_eq!(out["key_algo"].as_str(), Some("ed25519"));

        let account = run_query(
            &base,
            "ua_getAccount",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "acct-key-ed25519",
                }),
            ),
        );
        assert_eq!(
            account["account"]["primary_key_binding"]["key_algo"].as_str(),
            Some("ed25519")
        );
        assert_eq!(account["account"]["key_algo"].as_str(), Some("ed25519"));

        let audit_events = run_query(
            &base,
            "ua_getAuditEvents",
            params_with_paths(&store, &audit, json!({"source": "router"})),
        );
        let events = audit_events["events"]
            .as_array()
            .expect("events should be array");
        assert!(events.iter().any(|event| {
            event["event_kind"].as_str() == Some("uca_created")
                && event["uca_id"].as_str() == Some("acct-key-ed25519")
                && event["key_algo"].as_str() == Some("ed25519")
        }));
    }

    #[test]
    fn unified_account_surface_cut_a_secp256k1_key_binding_persists_metadata() {
        let (base, store, audit) = temp_paths("cut-a-secp256k1");
        let mut params = match secp256k1_key_binding_params("acct-key-secp256k1", "create") {
            Value::Object(map) => map,
            other => panic!("expected object params, got {other:?}"),
        };
        params.insert(
            "account_id".to_string(),
            Value::String("acct-key-secp256k1".to_string()),
        );
        params.insert("now".to_string(), Value::from(12u64));
        let out = run_query(
            &base,
            "ua_createUca",
            params_with_paths(&store, &audit, Value::Object(params)),
        );
        assert_eq!(out["created"].as_bool(), Some(true));
        assert_eq!(out["key_algo"].as_str(), Some("secp256k1"));

        let account = run_query(
            &base,
            "ua_getAccount",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "acct-key-secp256k1",
                }),
            ),
        );
        assert_eq!(
            account["account"]["primary_key_binding"]["key_algo"].as_str(),
            Some("secp256k1")
        );
    }

    #[test]
    fn unified_account_surface_cut_a_invalid_key_binding_proof_is_rejected_without_state_pollution()
    {
        let (base, store, audit) = temp_paths("cut-a-invalid-proof");
        let mut params = match ed25519_key_binding_params("acct-key-invalid", "create") {
            Value::Object(map) => map,
            other => panic!("expected object params, got {other:?}"),
        };
        params.insert(
            "account_id".to_string(),
            Value::String("acct-key-invalid".to_string()),
        );
        params.insert("now".to_string(), Value::from(15u64));
        params.insert("proof_payload".to_string(), Value::String(ua_hex(0x99, 64)));
        let err = run_query_err(
            &base,
            "ua_createUca",
            params_with_paths(&store, &audit, Value::Object(params)),
        );
        assert!(
            err.contains("invalid ed25519 proof signature"),
            "unexpected error: {err}"
        );

        let get_err = run_query_err(
            &base,
            "ua_getAccount",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "acct-key-invalid",
                }),
            ),
        );
        assert!(
            get_err.contains("uca not found") || get_err.contains("UCA not found"),
            "unexpected account error: {get_err}"
        );
    }

    #[test]
    fn unified_account_surface_cut_a_mldsa87_key_rotation_persists_metadata() {
        let (base, store, audit) = temp_paths("cut-a-mldsa87-rotate");
        ua_create(&base, &store, &audit, "acct-key-mldsa87", 20);
        let mut params = match mldsa87_key_binding_params("acct-key-mldsa87", "rotate") {
            Value::Object(map) => map,
            other => panic!("expected object params, got {other:?}"),
        };
        params.insert(
            "account_id".to_string(),
            Value::String("acct-key-mldsa87".to_string()),
        );
        params.insert("role".to_string(), Value::String("owner".to_string()));
        params.insert("now".to_string(), Value::from(21u64));
        let out = run_query(
            &base,
            "ua_rotatePrimaryKey",
            params_with_paths(&store, &audit, Value::Object(params)),
        );
        assert_eq!(out["rotated"].as_bool(), Some(true));
        assert_eq!(out["key_algo"].as_str(), Some("mldsa87"));

        let account = run_query(
            &base,
            "ua_getAccount",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "acct-key-mldsa87",
                }),
            ),
        );
        assert_eq!(
            account["account"]["primary_key_binding"]["key_algo"].as_str(),
            Some("mldsa87")
        );
    }

    #[test]
    fn unified_account_surface_account_asset_views_execute_via_real_mainline_entry() {
        let (base, store, audit) = temp_paths("asset-view");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ua_create(&base, &store, &audit, "acct-assets-a", 10);

        let mut native = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native execution store");
        native.module_state.account_asset_balances.insert(
            normalize_account_view_key_v1("acct-assets-a"),
            BTreeMap::from([
                ("NOV".to_string(), 1_000u128),
                ("NUSD".to_string(), 250u128),
            ]),
        );
        native.module_state.credit_vaults.insert(
            7,
            NovCreditVaultStateV1 {
                vault_id: 7,
                owner: "acct-assets-a".to_string(),
                collateral_asset: "ETH".to_string(),
                collateral_amount: 300,
                debt_asset: "NUSD".to_string(),
                debt_amount: 100,
                min_collateral_ratio_bps: 15_000,
                opened_at_unix_ms: 111,
            },
        );
        native
            .module_state
            .treasury_settlement_journal
            .push(NovTreasurySettlementJournalEntryV1 {
                seq: 1,
                unix_ms: 222,
                kind: "fee_settlement".to_string(),
                tx_hash: "0xasset-view".to_string(),
                account_id: "acct-assets-a".to_string(),
                fee_owner_account_id: "acct-assets-a".to_string(),
                nonce_owner_account_id: "acct-assets-a".to_string(),
                key_algo: String::new(),
                execution_policy: "standard".to_string(),
                policy_enforced: false,
                policy_rejection_reason: None,
                source_asset: "NUSD".to_string(),
                source_amount: 40,
                settled_nov: 20,
                reserve_bucket_delta_nov: 8,
                fee_bucket_delta_nov: 6,
                risk_buffer_delta_nov: 6,
                route_ref: "clearing.route".to_string(),
                clearing_source: "clearing".to_string(),
                clearing_rate_ppm: 1_000_000,
                policy_version: 1,
                policy_source: "unit_test".to_string(),
                policy_contract_id: "policy-test".to_string(),
                policy_threshold_state: "healthy".to_string(),
                policy_constrained_strategy: "none".to_string(),
                policy_event_state: "settled".to_string(),
                status: "applied".to_string(),
                reason: None,
            });
        save_nov_native_execution_store_v1(native_store.as_path(), &native)
            .expect("save native execution store");

        let nusd = run_query(
            &base,
            "account_balance",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-assets-a",
                    "asset_id": "NUSD",
                }),
            ),
        );
        assert_eq!(nusd["account_id"].as_str(), Some("acct-assets-a"));
        assert_eq!(nusd["balance"].as_u64(), Some(250));
        assert_eq!(nusd["debt_outstanding"].as_u64(), Some(100));
        assert_eq!(nusd["locked_collateral"].as_u64(), Some(0));
        assert_eq!(nusd["treasury_source_flow"].as_u64(), Some(40));
        assert_eq!(nusd["component_count"].as_u64(), Some(3));
        assert_eq!(
            nusd["components"][0]["classification"].as_str(),
            Some("debt_outstanding")
        );
        assert_eq!(
            nusd["components"][1]["classification"].as_str(),
            Some("liquid_balance")
        );
        assert_eq!(
            nusd["components"][2]["classification"].as_str(),
            Some("treasury_source_flow")
        );

        let eth = run_query(
            &base,
            "account_balance",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-assets-a",
                    "asset_id": "ETH",
                }),
            ),
        );
        assert_eq!(eth["balance"].as_u64(), Some(0));
        assert_eq!(eth["locked_collateral"].as_u64(), Some(300));
        assert_eq!(eth["debt_outstanding"].as_u64(), Some(0));
        assert_eq!(eth["component_count"].as_u64(), Some(1));
        assert_eq!(
            eth["components"][0]["classification"].as_str(),
            Some("pledge_locked_collateral")
        );

        let nov = run_query(
            &base,
            "account_balance",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-assets-a",
                    "asset_id": "NOV",
                }),
            ),
        );
        assert_eq!(nov["balance"].as_u64(), Some(1_000));
        assert_eq!(nov["treasury_settled_nov"].as_u64(), Some(20));
        assert_eq!(
            nov["treasury_reserve_bucket_exposure_nov"].as_i64(),
            Some(8)
        );
        assert_eq!(nov["treasury_fee_bucket_exposure_nov"].as_i64(), Some(6));
        assert_eq!(nov["treasury_risk_buffer_exposure_nov"].as_i64(), Some(6));

        let assets = run_query(
            &base,
            "account_assets",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-assets-a",
                }),
            ),
        );
        assert_eq!(assets["account_id"].as_str(), Some("acct-assets-a"));
        assert_eq!(assets["asset_count"].as_u64(), Some(2));
        assert_eq!(assets["pledge_count"].as_u64(), Some(1));
        assert_eq!(assets["vault_count"].as_u64(), Some(1));
        assert_eq!(assets["treasury_exposure_count"].as_u64(), Some(4));
        assert_eq!(assets["assets"][0]["asset_id"].as_str(), Some("NOV"));
        assert_eq!(assets["assets"][1]["asset_id"].as_str(), Some("NUSD"));
        assert_eq!(
            assets["pledges"][0]["classification"].as_str(),
            Some("pledge")
        );
        assert_eq!(assets["pledges"][0]["asset_id"].as_str(), Some("ETH"));
        assert_eq!(assets["pledges"][0]["pledged_amount"].as_u64(), Some(300));
        assert_eq!(assets["vaults"][0]["vault_id"].as_u64(), Some(7));
        assert_eq!(
            assets["vaults"][0]["collateral_asset"].as_str(),
            Some("ETH")
        );
        assert_eq!(assets["vaults"][0]["debt_asset"].as_str(), Some("NUSD"));
        assert_eq!(
            assets["treasury_exposures"][0]["classification"].as_str(),
            Some("treasury_fee_bucket_exposure")
        );
        assert_eq!(
            assets["treasury_exposures"][1]["classification"].as_str(),
            Some("treasury_reserve_bucket_exposure")
        );
        assert_eq!(
            assets["treasury_exposures"][2]["classification"].as_str(),
            Some("treasury_risk_buffer_exposure")
        );
        assert_eq!(
            assets["treasury_exposures"][3]["classification"].as_str(),
            Some("treasury_source_flow")
        );
        assert_eq!(
            assets["treasury_exposures"][3]["asset_id"].as_str(),
            Some("NUSD")
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_mapped_asset_mvp_lifecycle_is_internal_and_closed_loop() {
        let (base, store, audit) = temp_paths("mapped-lifecycle");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-a", 10);

        let mut register_map = match mapped_lock_proof_params("acct-map-a", 0x31, 500u128) {
            Value::Object(map) => map,
            other => panic!("expected mapped lock proof params object, got {other:?}"),
        };
        register_map.insert("now".to_string(), Value::from(11u64));
        let register = run_query(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(register_map),
            ),
        );
        assert_eq!(register["accepted"].as_bool(), Some(true));
        assert_eq!(register["status"].as_str(), Some("active"));
        assert_eq!(register["phase4_mode"].as_str(), Some("shadow"));
        assert_eq!(register["settlement_effect"].as_str(), Some("none"));
        let mapping_id = register["mapping_id"]
            .as_str()
            .expect("mapping_id should exist")
            .to_string();

        let assets_before_burn = run_query(
            &base,
            "account_assets",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({"account_id": "acct-map-a"}),
            ),
        );
        assert_eq!(assets_before_burn["mapped_asset_count"].as_u64(), Some(1));
        assert_eq!(
            assets_before_burn["mapped_assets"][0]["status"].as_str(),
            Some("active")
        );
        let balance_before_burn = run_query(
            &base,
            "account_balance",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({"account_id": "acct-map-a", "asset_id": "NETH"}),
            ),
        );
        assert_eq!(
            balance_before_burn["mapped_asset_active_balance"].as_u64(),
            Some(500)
        );

        let burn = run_query(
            &base,
            "ua_burnMappedAsset",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-a",
                    "mapping_id": mapping_id,
                    "now": 12u64,
                }),
            ),
        );
        assert_eq!(burn["burned"].as_bool(), Some(true));
        assert_eq!(burn["status"].as_str(), Some("burn_pending"));

        let assets_after_burn = run_query(
            &base,
            "account_assets",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({"account_id": "acct-map-a"}),
            ),
        );
        assert_eq!(assets_after_burn["mapped_asset_count"].as_u64(), Some(0));

        let release = run_query(
            &base,
            "ua_releaseMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-a",
                    "mapping_id": register["mapping_id"],
                    "now": 13u64,
                }),
            ),
        );
        assert_eq!(release["released"].as_bool(), Some(true));
        assert_eq!(release["status"].as_str(), Some("released"));
        assert_eq!(release["phase4_mode"].as_str(), Some("shadow"));
        assert_eq!(release["settlement_effect"].as_str(), Some("none"));

        let mapped = run_query(
            &base,
            "ua_getMappedAsset",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-a",
                    "mapping_id": register["mapping_id"],
                }),
            ),
        );
        assert_eq!(mapped["mapped_asset"]["status"].as_str(), Some("released"));
        assert_eq!(
            mapped["mapped_asset"]["phase4_mode"].as_str(),
            Some("shadow")
        );
        assert_eq!(
            mapped["mapped_asset"]["settlement_effect"].as_str(),
            Some("none")
        );
        let balance_after_release = run_query(
            &base,
            "account_balance",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({"account_id": "acct-map-a", "asset_id": "NETH"}),
            ),
        );
        assert_eq!(
            balance_after_release["mapped_asset_active_balance"].as_u64(),
            Some(0)
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_mapped_asset_shadow_mode_rejects_live_register_path() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let (base, store, audit) = temp_paths("mapped-shadow-enforce");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-shadow", 10);

        let mut register_map =
            match mapped_lock_live_event_proof_params("acct-map-shadow", 0x32, 210u128) {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        register_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        register_map.insert("now".to_string(), Value::from(11u64));
        let err = run_query_err(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(register_map),
            ),
        );
        assert!(
            err.contains("ERR_PHASE4_SHADOW_MODE_REQUIRED"),
            "live register path should be rejected by shadow mode guard, got: {err}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_live_mapped_lock_creates_neth_m2_credit_without_nov_mint() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-credit");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live", 10);

        let mut register_map =
            match mapped_lock_live_event_proof_params("acct-map-live", 0x33, 700u128) {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        register_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        register_map.insert("now".to_string(), Value::from(11u64));
        let register = run_query(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(register_map),
            ),
        );
        assert_eq!(register["accepted"].as_bool(), Some(true));
        assert_eq!(register["phase4_mode"].as_str(), Some("live"));
        assert_eq!(
            register["settlement_effect"].as_str(),
            Some("neth_m2_credit")
        );
        assert_eq!(
            register["native_settlement"]["effect"].as_str(),
            Some("neth_m2_credit")
        );
        assert_eq!(
            register["native_settlement"]["nov_minted"].as_u64(),
            Some(0)
        );

        let native_after_register = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store after live register");
        assert_eq!(
            native_after_register
                .module_state
                .account_asset_balances
                .get("acct-map-live")
                .and_then(|assets| assets.get("NETH"))
                .copied(),
            Some(700)
        );
        assert_eq!(
            native_after_register
                .module_state
                .treasury_reserves
                .get("NETH")
                .copied(),
            Some(700)
        );
        assert_eq!(
            native_after_register
                .module_state
                .treasury_settlement_journal
                .last()
                .map(|entry| entry.kind.as_str()),
            Some("mapped_lock_m2_credit")
        );
        assert_eq!(
            native_after_register
                .module_state
                .treasury_settlement_journal
                .last()
                .map(|entry| entry.settled_nov),
            Some(0)
        );

        let balance = run_query(
            &base,
            "account_balance",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({"account_id": "acct-map-live", "asset_id": "NETH"}),
            ),
        );
        assert_eq!(balance["balance"].as_u64(), Some(700));
        assert_eq!(balance["mapped_asset_active_balance"].as_u64(), Some(700));

        let burn = run_query(
            &base,
            "ua_burnMappedAsset",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live",
                    "mapping_id": register["mapping_id"],
                    "now": 12u64,
                }),
            ),
        );
        assert_eq!(burn["burned"].as_bool(), Some(true));
        assert_eq!(
            burn["native_settlement"]["effect"].as_str(),
            Some("neth_m2_burn_pending")
        );
        let native_after_burn = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store after live burn");
        assert_eq!(
            native_after_burn
                .module_state
                .account_asset_balances
                .get("acct-map-live")
                .and_then(|assets| assets.get("NETH"))
                .copied(),
            Some(0)
        );
        assert_eq!(
            native_after_burn
                .module_state
                .treasury_reserves
                .get("NETH")
                .copied(),
            Some(700)
        );

        let release = run_query(
            &base,
            "ua_releaseMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live",
                    "mapping_id": register["mapping_id"],
                    "now": 13u64,
                }),
            ),
        );
        assert_eq!(release["released"].as_bool(), Some(true));
        assert_eq!(
            release["native_settlement"]["effect"].as_str(),
            Some("source_release_reserve_debit")
        );
        let native_after_release = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store after live release");
        assert_eq!(
            native_after_release
                .module_state
                .treasury_reserves
                .get("NETH")
                .copied(),
            Some(0)
        );
        assert_eq!(
            native_after_release
                .module_state
                .treasury_settlement_journal
                .iter()
                .map(|entry| entry.kind.as_str())
                .collect::<Vec<_>>(),
            vec![
                "mapped_lock_m2_credit",
                "mapped_asset_m2_burn_pending",
                "mapped_lock_source_release"
            ]
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_live_mapped_lock_requires_structured_eth_event_evidence() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-proof-required");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live-proof", 10);

        let mut register_map = match mapped_lock_proof_params("acct-map-live-proof", 0x34, 80u128) {
            Value::Object(map) => map,
            other => panic!("expected mapped lock proof params object, got {other:?}"),
        };
        register_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        register_map.insert("now".to_string(), Value::from(11u64));
        let err = run_query_err(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(register_map),
            ),
        );
        assert!(
            err.contains("live mapped lock requires structured Ethereum lock event evidence"),
            "live register should reject digest-only proof, got: {err}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_live_mapped_lock_rejects_unfinalized_or_wrong_contract_evidence() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-proof-invalid");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live-invalid", 10);

        let mut unfinalized_map =
            match mapped_lock_live_event_proof_params("acct-map-live-invalid", 0x35, 90u128) {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        unfinalized_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        unfinalized_map.insert("finalized_block_number".to_string(), Value::from(101u64));
        let unfinalized_err = run_query_err(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(unfinalized_map),
            ),
        );
        assert!(
            unfinalized_err.contains("finalized_block_number"),
            "unfinalized proof should fail, got: {unfinalized_err}"
        );

        let mut wrong_contract_map =
            match mapped_lock_live_event_proof_params("acct-map-live-invalid", 0x36, 91u128) {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        wrong_contract_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        wrong_contract_map.insert(
            "expected_lock_contract_address".to_string(),
            Value::String(ua_hex(0x22, 20)),
        );
        let wrong_contract_err = run_query_err(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(wrong_contract_map),
            ),
        );
        assert!(
            wrong_contract_err.contains("lock_contract_address does not match configured contract"),
            "wrong contract proof should fail, got: {wrong_contract_err}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_mapped_asset_mvp_rejects_duplicate_and_invalid_proof() {
        let (base, store, audit) = temp_paths("mapped-errors");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-b", 10);

        let mut register_map = match mapped_lock_proof_params("acct-map-b", 0x41, 120u128) {
            Value::Object(map) => map,
            other => panic!("expected mapped lock proof params object, got {other:?}"),
        };
        register_map.insert("now".to_string(), Value::from(11u64));
        let register_params = params_with_paths_and_native_store(
            &store,
            &audit,
            &native_store,
            Value::Object(register_map.clone()),
        );
        let out = run_query(&base, "ua_registerMappedLock", register_params.clone());
        assert_eq!(out["accepted"].as_bool(), Some(true));

        let duplicate_err = run_query_err(&base, "ua_registerMappedLock", register_params);
        assert!(
            duplicate_err.contains("ERR_MAPPED_LOCK_ALREADY_REGISTERED"),
            "duplicate proof should fail with duplicate lock error, got: {duplicate_err}"
        );

        let mut invalid_map = match mapped_lock_proof_params("acct-map-b", 0x42, 130u128) {
            Value::Object(map) => map,
            other => panic!("expected mapped lock proof params object, got {other:?}"),
        };
        invalid_map.insert(
            "proof_payload".to_string(),
            Value::String("0xdeadbeef".to_string()),
        );
        invalid_map.insert("now".to_string(), Value::from(12u64));
        let invalid_err = run_query_err(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(invalid_map),
            ),
        );
        assert!(
            invalid_err.contains("ERR_MAPPED_LOCK_PROOF_INVALID"),
            "invalid proof should fail with proof error, got: {invalid_err}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_mapped_asset_mvp_release_requires_burn() {
        let (base, store, audit) = temp_paths("mapped-release-guard");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-c", 10);

        let mut register_map = match mapped_lock_proof_params("acct-map-c", 0x51, 200u128) {
            Value::Object(map) => map,
            other => panic!("expected mapped lock proof params object, got {other:?}"),
        };
        register_map.insert("now".to_string(), Value::from(11u64));
        let register = run_query(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(register_map),
            ),
        );
        let err = run_query_err(
            &base,
            "ua_releaseMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-c",
                    "mapping_id": register["mapping_id"],
                    "now": 12u64,
                }),
            ),
        );
        assert!(
            err.contains("ERR_MAPPED_RELEASE_REQUIRES_BURN"),
            "release without burn should fail with burn-required error, got: {err}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_mapped_asset_mvp_audit_trace_is_complete() {
        let (base, store, audit) = temp_paths("mapped-audit");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-d", 10);

        let lock_params = mapped_lock_proof_params("acct-map-d", 0x61, 330u128);
        let lock_id = lock_params["lock_id"]
            .as_str()
            .expect("lock_id should exist")
            .to_string();
        let source_tx_hash = lock_params["source_tx_hash"]
            .as_str()
            .expect("source_tx_hash should exist")
            .to_string();

        let register = run_query(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(&store, &audit, &native_store, lock_params.clone()),
        );
        let mapping_id = register["mapping_id"]
            .as_str()
            .expect("mapping_id should exist")
            .to_string();
        let _ = run_query(
            &base,
            "ua_burnMappedAsset",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-d",
                    "mapping_id": mapping_id,
                    "now": 12u64,
                }),
            ),
        );
        let _ = run_query(
            &base,
            "ua_releaseMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-d",
                    "mapping_id": register["mapping_id"],
                    "now": 13u64,
                }),
            ),
        );
        let audit_out = run_query(
            &base,
            "ua_getAuditEvents",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({"source": "sink", "limit": 200u64}),
            ),
        );
        let events = audit_out["events"]
            .as_array()
            .expect("events should be an array");
        let register_event = events.iter().find(|entry| {
            entry["method"].as_str() == Some("ua_registerMappedLock")
                && entry["success"].as_bool() == Some(true)
        });
        assert!(
            register_event.is_some(),
            "sink audit should include successful ua_registerMappedLock"
        );
        let register_event = register_event.expect("register event should exist");
        assert_eq!(
            register_event["params"]["lock_id"].as_str(),
            Some(lock_id.as_str())
        );
        assert_eq!(
            register_event["params"]["source_tx_hash"].as_str(),
            Some(source_tx_hash.as_str())
        );
        assert!(events.iter().any(|entry| {
            entry["method"].as_str() == Some("ua_burnMappedAsset")
                && entry["success"].as_bool() == Some(true)
        }));
        assert!(events.iter().any(|entry| {
            entry["method"].as_str() == Some("ua_releaseMappedLock")
                && entry["success"].as_bool() == Some(true)
        }));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_gate_ua_g01_mapping_bind_success() {
        let (base, store, audit) = temp_paths("g01");
        let evm_addr = ua_hex(0x11, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        let out = run_query(
            &base,
            "ua_bindPersona",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "now": 11,
                }),
            ),
        );
        assert_eq!(out["bound"].as_bool(), Some(true));
    }

    #[test]
    fn unified_account_gate_ua_g02_mapping_conflict_rejected() {
        let (base, store, audit) = temp_paths("g02");
        let evm_addr = ua_hex(0x12, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        ua_create(&base, &store, &audit, "uca-b", 10);
        ua_bind(
            &base, &store, &audit, "uca-a", "owner", "evm", 1, &evm_addr, 11,
        );
        let err = run_query_err(
            &base,
            "ua_bindPersona",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-b",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "now": 12,
                }),
            ),
        );
        assert!(err.contains("binding conflict"));
    }

    #[test]
    fn unified_account_gate_ua_g03_mapping_cooldown_rejects_rebind() {
        let (base, store, audit) = temp_paths("g03");
        let evm_addr = ua_hex(0x13, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        ua_bind(
            &base, &store, &audit, "uca-a", "owner", "evm", 1, &evm_addr, 11,
        );
        let _ = run_query(
            &base,
            "ua_revokePersona",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "cooldown_seconds": 60,
                    "now": 12,
                }),
            ),
        );
        let err = run_query_err(
            &base,
            "ua_bindPersona",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "now": 20,
                }),
            ),
        );
        assert!(err.contains("cooldown active"));
    }

    #[test]
    fn unified_account_gate_ua_g04_signature_domain_mismatch_rejected() {
        let (base, store, audit) = temp_paths("g04");
        let evm_addr = ua_hex(0x14, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        ua_bind(
            &base, &store, &audit, "uca-a", "owner", "evm", 1, &evm_addr, 11,
        );
        let err = run_query_err(
            &base,
            "ua_route",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "protocol": "eth",
                    "signature_domain": "web30:mainnet",
                    "nonce": 0,
                    "now": 12,
                }),
            ),
        );
        assert!(err.contains("domain mismatch"));
    }

    #[test]
    fn unified_account_gate_ua_g05_signature_domain_eip712_wrong_chain_rejected() {
        let (base, store, audit) = temp_paths("g05");
        let evm_addr = ua_hex(0x15, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        ua_bind(
            &base, &store, &audit, "uca-a", "owner", "evm", 1, &evm_addr, 11,
        );
        let err = run_query_err(
            &base,
            "ua_route",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "protocol": "eth",
                    "signature_domain": "eip712:2:demo",
                    "nonce": 0,
                    "now": 12,
                }),
            ),
        );
        assert!(err.contains("domain mismatch"));
    }

    #[test]
    fn unified_account_gate_ua_g06_nonce_replay_rejected() {
        let (base, store, audit) = temp_paths("g06");
        let evm_addr = ua_hex(0x16, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        ua_bind(
            &base, &store, &audit, "uca-a", "owner", "evm", 1, &evm_addr, 11,
        );
        let _ = run_query(
            &base,
            "ua_route",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "protocol": "eth",
                    "signature_domain": "evm:1",
                    "nonce": 0,
                    "now": 12,
                }),
            ),
        );
        let err = run_query_err(
            &base,
            "ua_route",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "protocol": "eth",
                    "signature_domain": "evm:1",
                    "nonce": 0,
                    "now": 13,
                }),
            ),
        );
        assert!(err.contains("nonce rejected"));
    }

    #[test]
    fn unified_account_gate_ua_g06b_check_route_is_read_only() {
        let (base, store, audit) = temp_paths("g06b");
        let evm_addr = ua_hex(0x66, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        ua_bind(
            &base, &store, &audit, "uca-a", "owner", "evm", 1, &evm_addr, 11,
        );
        let checked = run_query(
            &base,
            "ua_checkRoute",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr.as_str(),
                    "protocol": "eth",
                    "signature_domain": "evm:1",
                    "nonce": 0,
                    "now": 12,
                }),
            ),
        );
        assert_eq!(checked["accepted"].as_bool(), Some(true));
        assert_eq!(checked["read_only"].as_bool(), Some(true));

        let next_after_check = run_query(
            &base,
            "ua_getNextNonce",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr.as_str(),
                }),
            ),
        );
        assert_eq!(next_after_check["nonce"].as_u64(), Some(0));

        let routed = run_query(
            &base,
            "ua_route",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr.as_str(),
                    "protocol": "eth",
                    "signature_domain": "evm:1",
                    "nonce": 0,
                    "now": 13,
                }),
            ),
        );
        assert_eq!(routed["accepted"].as_bool(), Some(true));

        let next_after_route = run_query(
            &base,
            "ua_getNextNonce",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr.as_str(),
                }),
            ),
        );
        assert_eq!(next_after_route["nonce"].as_u64(), Some(1));
    }

    #[test]
    fn unified_account_gate_ua_g07_nonce_reverse_order_rejected() {
        let (base, store, audit) = temp_paths("g07");
        let evm_addr = ua_hex(0x17, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        ua_bind(
            &base, &store, &audit, "uca-a", "owner", "evm", 1, &evm_addr, 11,
        );
        let err = run_query_err(
            &base,
            "ua_route",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "protocol": "eth",
                    "signature_domain": "evm:1",
                    "nonce": 1,
                    "now": 12,
                }),
            ),
        );
        assert!(err.contains("nonce rejected"));
    }

    #[test]
    fn unified_account_gate_ua_g08_permission_delegate_cannot_update_policy() {
        let (base, store, audit) = temp_paths("g08");
        ua_create(&base, &store, &audit, "uca-a", 10);
        let err = run_query_err(
            &base,
            "ua_setPolicy",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "delegate",
                    "nonce_scope": "global",
                    "now": 11,
                }),
            ),
        );
        assert!(err.contains("permission denied"));
    }

    #[test]
    fn unified_account_gate_ua_g09_permission_expired_session_key_rejected() {
        let (base, store, audit) = temp_paths("g09");
        let evm_addr = ua_hex(0x19, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        ua_bind(
            &base, &store, &audit, "uca-a", "owner", "evm", 1, &evm_addr, 11,
        );
        let err = run_query_err(
            &base,
            "ua_route",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "session_key",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "protocol": "eth",
                    "signature_domain": "evm:1",
                    "nonce": 0,
                    "session_expires_at": 100,
                    "now": 101,
                }),
            ),
        );
        assert!(err.contains("session key expired"));
    }

    #[test]
    fn unified_account_gate_ua_g10_boundary_eth_cross_chain_atomic_rejected() {
        let (base, store, audit) = temp_paths("g10");
        let evm_addr = ua_hex(0x1a, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        ua_bind(
            &base, &store, &audit, "uca-a", "owner", "evm", 1, &evm_addr, 11,
        );
        let err = run_query_err(
            &base,
            "ua_route",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "protocol": "eth",
                    "signature_domain": "evm:1",
                    "nonce": 0,
                    "wants_cross_chain_atomic": true,
                    "now": 12,
                }),
            ),
        );
        assert!(err.contains("cross-chain atomic"));
    }

    #[test]
    fn unified_account_gate_ua_g11_boundary_web30_single_chain_passes_without_eth_pollution() {
        let (base, store, audit) = temp_paths("g11");
        let web30_addr = ua_hex(0x1b, 20);
        let evm_addr = ua_hex(0x2b, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        ua_bind(
            &base,
            &store,
            &audit,
            "uca-a",
            "owner",
            "web30",
            7,
            &web30_addr,
            11,
        );
        ua_bind(
            &base, &store, &audit, "uca-a", "owner", "evm", 1, &evm_addr, 12,
        );
        let web30_out = run_query(
            &base,
            "ua_route",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "persona_type": "web30",
                    "chain_id": 7,
                    "external_address": web30_addr,
                    "protocol": "web30",
                    "signature_domain": "web30:mainnet",
                    "nonce": 0,
                    "now": 13,
                }),
            ),
        );
        assert_eq!(web30_out["decision"]["kind"].as_str(), Some("fast_path"));
        let evm_out = run_query(
            &base,
            "ua_route",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "protocol": "eth",
                    "signature_domain": "evm:1",
                    "nonce": 0,
                    "now": 14,
                }),
            ),
        );
        assert_eq!(evm_out["decision"]["kind"].as_str(), Some("adapter"));
    }

    #[test]
    fn unified_account_gate_ua_g12_type4_supported_mode_passes() {
        let (base, store, audit) = temp_paths("g12");
        let evm_addr = ua_hex(0x1c, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        ua_bind(
            &base, &store, &audit, "uca-a", "owner", "evm", 1, &evm_addr, 11,
        );
        let _ = run_query(
            &base,
            "ua_setPolicy",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "type4_policy_mode": "supported",
                    "allow_type4_with_delegate_or_session": true,
                    "now": 12,
                }),
            ),
        );
        let out = run_query(
            &base,
            "ua_route",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "delegate",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "protocol": "eth",
                    "signature_domain": "evm:1",
                    "nonce": 0,
                    "tx_type4": true,
                    "now": 13,
                }),
            ),
        );
        assert_eq!(out["accepted"].as_bool(), Some(true));
    }

    #[test]
    fn unified_account_gate_ua_g13_type4_reject_mode_returns_fixed_error() {
        let (base, store, audit) = temp_paths("g13");
        let evm_addr = ua_hex(0x1d, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        ua_bind(
            &base, &store, &audit, "uca-a", "owner", "evm", 1, &evm_addr, 11,
        );
        let err = run_query_err(
            &base,
            "ua_route",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "protocol": "eth",
                    "signature_domain": "evm:1",
                    "nonce": 0,
                    "tx_type4": true,
                    "now": 12,
                }),
            ),
        );
        assert!(err.contains("ERR_UNSUPPORTED_TX_TYPE_4"));
    }

    #[test]
    fn unified_account_gate_ua_g14_type4_with_session_key_rejected_by_policy() {
        let (base, store, audit) = temp_paths("g14");
        let evm_addr = ua_hex(0x1e, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        ua_bind(
            &base, &store, &audit, "uca-a", "owner", "evm", 1, &evm_addr, 11,
        );
        let _ = run_query(
            &base,
            "ua_setPolicy",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "type4_policy_mode": "supported",
                    "allow_type4_with_delegate_or_session": false,
                    "now": 12,
                }),
            ),
        );
        let err = run_query_err(
            &base,
            "ua_route",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "session_key",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "protocol": "eth",
                    "signature_domain": "evm:1",
                    "nonce": 0,
                    "tx_type4": true,
                    "session_expires_at": 9999,
                    "now": 13,
                }),
            ),
        );
        assert!(err.contains("ERR_TYPE4_ROLE_MIX_FORBIDDEN"));
    }

    #[test]
    fn unified_account_gate_ua_g15_uniqueness_conflict_signal_blocks_second_owner() {
        let (base, store, audit) = temp_paths("g15");
        let evm_addr = ua_hex(0x1f, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        ua_create(&base, &store, &audit, "uca-b", 10);
        ua_bind(
            &base, &store, &audit, "uca-a", "owner", "evm", 1, &evm_addr, 11,
        );
        let err = run_query_err(
            &base,
            "ua_bindPersona",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-b",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "now": 12,
                }),
            ),
        );
        assert!(err.contains("binding conflict"));
        let owner = run_query(
            &base,
            "ua_getBindingOwner",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                }),
            ),
        );
        assert_eq!(owner["owner_account_id"].as_str(), Some("uca-a"));
    }

    #[test]
    fn unified_account_gate_ua_g16_recovery_rotate_then_revoke_emits_events() {
        let (base, store, audit) = temp_paths("g16");
        let evm_addr = ua_hex(0x2a, 20);
        ua_create(&base, &store, &audit, "uca-a", 10);
        ua_bind(
            &base, &store, &audit, "uca-a", "owner", "evm", 1, &evm_addr, 11,
        );
        let _ = run_query(
            &base,
            "ua_rotatePrimaryKey",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "next_primary_key_ref": ua_hex(0x55, 32),
                    "now": 12,
                }),
            ),
        );
        let _ = run_query(
            &base,
            "ua_revokePersona",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "uca-a",
                    "role": "owner",
                    "persona_type": "evm",
                    "chain_id": 1,
                    "external_address": evm_addr,
                    "cooldown_seconds": 30,
                    "now": 13,
                }),
            ),
        );
        let audit_events = run_query(
            &base,
            "ua_getAuditEvents",
            params_with_paths(&store, &audit, json!({"source": "router"})),
        );
        let events = audit_events["events"]
            .as_array()
            .expect("events should be array");
        assert!(events.iter().any(|event| {
            event["event_kind"].as_str() == Some("key_rotated")
                && event["uca_id"].as_str() == Some("uca-a")
        }));
        assert!(events.iter().any(|event| {
            event["event_kind"].as_str() == Some("binding_revoked")
                && event["uca_id"].as_str() == Some("uca-a")
        }));
    }
}
