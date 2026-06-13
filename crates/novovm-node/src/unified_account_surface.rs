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
const NOVOVM_UA_MAPPED_ASSET_BRIDGE_PAUSED_ENV: &str = "NOVOVM_UA_MAPPED_ASSET_BRIDGE_PAUSED";
const NOVOVM_UA_MAPPED_LOCK_BRIDGE_PAUSED_ENV: &str = "NOVOVM_UA_MAPPED_LOCK_BRIDGE_PAUSED";
const NOVOVM_UA_MAPPED_ASSET_BURN_PAUSED_ENV: &str = "NOVOVM_UA_MAPPED_ASSET_BURN_PAUSED";
const NOVOVM_UA_MAPPED_ASSET_RELEASE_PAUSED_ENV: &str = "NOVOVM_UA_MAPPED_ASSET_RELEASE_PAUSED";
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
            | "ua_freezeMappedAsset"
            | "ua_unfreezeMappedAsset"
            | "ua_rollbackFrozenMappedAsset"
            | "ua_autoHealMappedAssets"
            | "ua_setMappedHeaderSourcePolicy"
            | "ua_getMappedHeaderSourcePolicy"
            | "ua_setMappedHeaderAttestationPolicy"
            | "ua_getMappedHeaderAttestationPolicy"
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
        "ua_setMappedHeaderSourcePolicy" => {
            let required = param_as_bool(params, "required")
                .or_else(|| param_as_bool(params, "mapped_header_source_required"))
                .unwrap_or(true);
            let allowed_peer_ids =
                param_as_u64_list_any(params, &["allowed_peer_ids", "source_peer_ids"])
                    .unwrap_or_default();
            if required && allowed_peer_ids.is_empty() {
                bail!(
                    "ERR_MAPPED_HEADER_SOURCE_POLICY_INVALID: required policy needs at least one allowed source peer"
                );
            }
            let min_quorum = param_as_u64(params, "min_source_quorum")
                .or_else(|| param_as_u64(params, "source_quorum"))
                .unwrap_or(1)
                .clamp(1, u64::from(u32::MAX)) as u32;
            if required && allowed_peer_ids.len() < min_quorum as usize {
                bail!(
                    "ERR_MAPPED_HEADER_SOURCE_POLICY_INVALID: allowed source peer count {} is below min_source_quorum {}",
                    allowed_peer_ids.len(),
                    min_quorum
                );
            }
            let source = param_as_string_any(params, &["source", "policy_source"])
                .unwrap_or_else(|| "governance_path".to_string());
            let version = param_as_u64(params, "policy_version")
                .unwrap_or(1)
                .clamp(1, u64::from(u32::MAX)) as u32;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let store_path = native_execution_store_path_from_params_or_env_v1(params);
            let mut store = load_nov_native_execution_store_v1(store_path.as_path())?;
            store.module_state.mapped_header_source_required = required;
            store.module_state.mapped_header_source_allowed_peer_ids = allowed_peer_ids;
            store.module_state.mapped_header_source_min_quorum = min_quorum;
            store.module_state.mapped_header_source_policy_source = source;
            store.module_state.mapped_header_source_policy_version = version;
            store
                .module_state
                .mapped_header_source_policy_updated_unix_ms = u128::from(now).saturating_mul(1000);
            store.last_updated_unix_ms = u128::from(now).saturating_mul(1000);
            save_nov_native_execution_store_v1(store_path.as_path(), &store)?;
            Ok((
                json!({
                    "method": method,
                    "updated": true,
                    "policy": mapped_header_source_policy_to_json_v1(&store),
                    "store_path": store_path.display().to_string(),
                }),
                true,
            ))
        }
        "ua_getMappedHeaderSourcePolicy" => {
            let store_path = native_execution_store_path_from_params_or_env_v1(params);
            let store = load_nov_native_execution_store_v1(store_path.as_path())?;
            Ok((
                json!({
                    "method": method,
                    "policy": mapped_header_source_policy_to_json_v1(&store),
                    "store_path": store_path.display().to_string(),
                }),
                false,
            ))
        }
        "ua_setMappedHeaderAttestationPolicy" => {
            let required = param_as_bool(params, "required")
                .or_else(|| param_as_bool(params, "mapped_header_attestation_required"))
                .unwrap_or(true);
            let allowed_signers = param_as_string_list_any(
                params,
                &[
                    "allowed_signers",
                    "attestation_signers",
                    "finality_source_signers",
                ],
            )
            .unwrap_or_default()
            .into_iter()
            .map(|raw| normalize_mapped_header_attestation_signer_v1(raw.as_str()))
            .filter(|raw| !raw.is_empty())
            .collect::<Vec<_>>();
            let mut allowed_signers = allowed_signers;
            allowed_signers.sort();
            allowed_signers.dedup();
            for signer in &allowed_signers {
                validate_mapped_header_attestation_signer_v1(signer)?;
            }
            let disabled_signers = param_as_string_list_any(
                params,
                &[
                    "disabled_signers",
                    "revoked_signers",
                    "disabled_attestation_signers",
                ],
            )
            .unwrap_or_default()
            .into_iter()
            .map(|raw| normalize_mapped_header_attestation_signer_v1(raw.as_str()))
            .filter(|raw| !raw.is_empty())
            .collect::<Vec<_>>();
            let mut disabled_signers = disabled_signers;
            disabled_signers.sort();
            disabled_signers.dedup();
            for signer in &disabled_signers {
                validate_mapped_header_attestation_signer_v1(signer)?;
            }
            let disabled_signer_reasons =
                parse_mapped_header_attestation_disabled_reasons_v1(params, &disabled_signers)?;
            let signer_rotations = parse_mapped_header_attestation_rotations_v1(
                params,
                &allowed_signers,
                &disabled_signers,
            )?;
            if required && allowed_signers.is_empty() {
                bail!(
                    "ERR_MAPPED_HEADER_ATTESTATION_POLICY_INVALID: required policy needs at least one allowed attestation signer"
                );
            }
            let min_quorum = param_as_u64(params, "min_attestation_quorum")
                .or_else(|| param_as_u64(params, "attestation_quorum"))
                .unwrap_or(1)
                .clamp(1, u64::from(u32::MAX)) as u32;
            if required && allowed_signers.len() < min_quorum as usize {
                bail!(
                    "ERR_MAPPED_HEADER_ATTESTATION_POLICY_INVALID: allowed attestation signer count {} is below min_attestation_quorum {}",
                    allowed_signers.len(),
                    min_quorum
                );
            }
            let source = param_as_string_any(params, &["source", "policy_source"])
                .unwrap_or_else(|| "governance_path".to_string());
            let version = param_as_u64(params, "policy_version")
                .unwrap_or(1)
                .clamp(1, u64::from(u32::MAX)) as u32;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let store_path = native_execution_store_path_from_params_or_env_v1(params);
            let mut store = load_nov_native_execution_store_v1(store_path.as_path())?;
            store.module_state.mapped_header_attestation_required = required;
            store.module_state.mapped_header_attestation_allowed_signers = allowed_signers;
            store
                .module_state
                .mapped_header_attestation_disabled_signers = disabled_signers;
            store
                .module_state
                .mapped_header_attestation_disabled_signer_reasons = disabled_signer_reasons;
            store
                .module_state
                .mapped_header_attestation_signer_rotations = signer_rotations;
            store.module_state.mapped_header_attestation_min_quorum = min_quorum;
            store.module_state.mapped_header_attestation_policy_source = source;
            store.module_state.mapped_header_attestation_policy_version = version;
            store
                .module_state
                .mapped_header_attestation_policy_updated_unix_ms =
                u128::from(now).saturating_mul(1000);
            store.last_updated_unix_ms = u128::from(now).saturating_mul(1000);
            save_nov_native_execution_store_v1(store_path.as_path(), &store)?;
            Ok((
                json!({
                    "method": method,
                    "updated": true,
                    "policy": mapped_header_attestation_policy_to_json_v1(&store),
                    "store_path": store_path.display().to_string(),
                }),
                true,
            ))
        }
        "ua_getMappedHeaderAttestationPolicy" => {
            let store_path = native_execution_store_path_from_params_or_env_v1(params);
            let store = load_nov_native_execution_store_v1(store_path.as_path())?;
            Ok((
                json!({
                    "method": method,
                    "policy": mapped_header_attestation_policy_to_json_v1(&store),
                    "store_path": store_path.display().to_string(),
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
            let eth_lock_evidence = verify_mapped_lock_proof(&proof, params, !shadow_mode)?;
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
                source_chain_id: eth_lock_evidence.as_ref().map(|evidence| evidence.chain_id),
                source_block_number: eth_lock_evidence
                    .as_ref()
                    .map(|evidence| evidence.block_number),
                source_block_hash: eth_lock_evidence
                    .as_ref()
                    .map(|evidence| evidence.block_hash.to_vec())
                    .unwrap_or_default(),
                source_receipts_root: eth_lock_evidence
                    .as_ref()
                    .map(|evidence| evidence.receipts_root.to_vec())
                    .unwrap_or_default(),
                source_finalized_block_number: eth_lock_evidence
                    .as_ref()
                    .map(|evidence| evidence.finalized_block_number),
                source_log_index: eth_lock_evidence
                    .as_ref()
                    .map(|evidence| evidence.log_index),
                source_receipt_index: eth_lock_evidence
                    .as_ref()
                    .map(|evidence| evidence.receipt_index),
                source_receipt_log_index: eth_lock_evidence
                    .as_ref()
                    .map(|evidence| evidence.receipt_log_index),
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
            if !shadow_mode {
                require_mapped_bridge_gate_open_v1(params, MappedBridgeGateV1::Register)?;
            }
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
            let source_anchor_status = mapped_asset_source_anchor_status_v1(&record);
            let record_account_id = record.target_account_id.clone();
            let record_uca_id = record_account_id.clone();
            Ok((
                json!({
                    "method": method,
                    "found": true,
                    "account_id": record_account_id,
                    "uca_id": record_uca_id,
                    "mapped_asset": mapped_asset_record_to_json(&record),
                    "source_anchor_status": mapped_asset_anchor_status_to_json_v1(&source_anchor_status),
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
            require_mapped_asset_anchor_safe_v1(record, "ua_burnMappedAsset")?;
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
        "ua_freezeMappedAsset" => {
            let account_id = parse_account_id(params)?;
            let mapping_key = resolve_mapped_asset_lookup_key(params, mapped_asset_state)?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let freeze_reason = param_as_string_any(params, &["reason", "freeze_reason"])
                .unwrap_or_else(|| "manual mapped asset freeze".to_string());
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
            if !matches!(
                record.status,
                MappedAssetStatus::Active | MappedAssetStatus::BurnPending
            ) {
                bail!(
                    "ERR_MAPPED_FREEZE_STATUS_INVALID: expected active or burn_pending, got {}",
                    record.status.as_str()
                );
            }
            let source_anchor_status = mapped_asset_source_anchor_status_v1(record);
            let settlement = apply_live_mapped_asset_m2_freeze_v1(
                record,
                mapping_key.as_str(),
                params,
                now,
                freeze_reason.as_str(),
            )?;
            record.status = MappedAssetStatus::Frozen;
            record.updated_at = now;
            record.audit_ref =
                derive_mapped_asset_audit_ref_v1(record.mapping_id, record.status, now);
            let op = build_mapped_asset_operation_v1(
                record,
                MappedAssetOperationKind::FreezeMapped,
                now,
                mapped_asset_state.operations.len() as u64 + 1,
            );
            mapped_asset_state.operations.push(op);
            emit_mapped_asset_operation_observed_v1(
                "freeze_mapped",
                true,
                Some(account_id.as_str()),
                Some(mapping_key.as_str()),
                None,
                Some("qualified"),
            );
            Ok((
                json!({
                    "method": method,
                    "frozen": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "mapping_id": mapping_key,
                    "status": record.status.as_str(),
                    "phase4_mode": mapped_asset_phase4_mode_v1(record),
                    "settlement_effect": "neth_m2_frozen",
                    "native_settlement": settlement,
                    "source_anchor_status": mapped_asset_anchor_status_to_json_v1(&source_anchor_status),
                }),
                true,
            ))
        }
        "ua_autoHealMappedAssets" => {
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let apply = param_as_bool(params, "apply").unwrap_or(false);
            let max_items = param_as_u64(params, "max_items")
                .unwrap_or(64)
                .clamp(1, 500) as usize;
            let store_path = native_execution_store_path_from_params_or_env_v1(params);
            let native_store = load_nov_native_execution_store_v1(store_path.as_path())?;
            if apply && !native_store.module_state.mapped_asset_auto_heal_enabled {
                bail!(
                    "ERR_MAPPED_AUTO_HEAL_DISABLED: apply=true requires governance-enabled mapped_asset_auto_heal_enabled"
                );
            }
            let account_filter = param_as_string_any(params, &["account_id", "uca_id"])
                .map(|raw| validate_uca_id_policy(&raw))
                .transpose()?;
            let reason = param_as_string_any(params, &["reason", "heal_reason"])
                .unwrap_or_else(|| "automatic mapped asset source anchor heal".to_string());
            let candidate_keys = mapped_asset_state
                .records_by_mapping_id
                .iter()
                .filter(|(_, record)| {
                    account_filter
                        .as_ref()
                        .map(|account_id| account_id == &record.target_account_id)
                        .unwrap_or(true)
                })
                .filter(|(_, record)| {
                    matches!(
                        record.status,
                        MappedAssetStatus::Active
                            | MappedAssetStatus::BurnPending
                            | MappedAssetStatus::Frozen
                    )
                })
                .filter_map(|(mapping_key, record)| {
                    let source_anchor_status = mapped_asset_source_anchor_status_v1(record);
                    match (record.status, source_anchor_status.state) {
                        (MappedAssetStatus::Active | MappedAssetStatus::BurnPending, "blocked") => {
                            Some((
                                mapping_key.clone(),
                                "freeze_unsafe_anchor",
                                source_anchor_status,
                            ))
                        }
                        (MappedAssetStatus::Frozen, "ok" | "not_required") => Some((
                            mapping_key.clone(),
                            "unfreeze_candidate_anchor_safe",
                            source_anchor_status,
                        )),
                        (MappedAssetStatus::Frozen, "blocked") => Some((
                            mapping_key.clone(),
                            "rollback_candidate_anchor_unsafe",
                            source_anchor_status,
                        )),
                        _ => None,
                    }
                })
                .take(max_items)
                .collect::<Vec<_>>();

            let mut reports = Vec::with_capacity(candidate_keys.len());
            let mut applied_count = 0usize;
            for (mapping_key, action, source_anchor_status) in candidate_keys {
                let Some(record) = mapped_asset_state
                    .records_by_mapping_id
                    .get_mut(mapping_key.as_str())
                else {
                    continue;
                };
                let before_status = record.status;
                let mut native_settlement = Value::Null;
                let mut applied = false;
                if apply && action == "freeze_unsafe_anchor" {
                    native_settlement = apply_live_mapped_asset_m2_freeze_v1(
                        record,
                        mapping_key.as_str(),
                        params,
                        now,
                        reason.as_str(),
                    )?;
                    record.status = MappedAssetStatus::Frozen;
                    record.updated_at = now;
                    record.audit_ref =
                        derive_mapped_asset_audit_ref_v1(record.mapping_id, record.status, now);
                    let op = build_mapped_asset_operation_v1(
                        record,
                        MappedAssetOperationKind::FreezeMapped,
                        now,
                        mapped_asset_state.operations.len() as u64 + 1,
                    );
                    mapped_asset_state.operations.push(op);
                    emit_mapped_asset_operation_observed_v1(
                        "auto_heal_freeze_mapped",
                        true,
                        Some(record.target_account_id.as_str()),
                        Some(mapping_key.as_str()),
                        None,
                        Some("qualified"),
                    );
                    applied = true;
                    applied_count = applied_count.saturating_add(1);
                }
                reports.push(json!({
                    "mapping_id": mapping_key,
                    "account_id": record.target_account_id,
                    "uca_id": record.target_account_id,
                    "asset": normalize_asset_view_symbol_v1(&record.target_asset_symbol),
                    "amount": record.amount,
                    "action": action,
                    "applied": applied,
                    "status_before": before_status.as_str(),
                    "status_after": record.status.as_str(),
                    "phase4_mode": mapped_asset_phase4_mode_v1(record),
                    "source_anchor_status": mapped_asset_anchor_status_to_json_v1(&source_anchor_status),
                    "native_settlement": native_settlement,
                }));
            }
            Ok((
                json!({
                    "method": method,
                    "apply": apply,
                    "dry_run": !apply,
                    "reason": reason,
                    "account_filter": account_filter,
                    "scanned_candidate_count": reports.len(),
                    "applied_count": applied_count,
                    "items": reports,
                    "scope": "internal_mapped_asset_reorg_heal_no_external_release_no_nov_mint",
                    "policy": {
                        "mapped_asset_auto_heal_enabled": native_store.module_state.mapped_asset_auto_heal_enabled,
                        "policy_source": native_store.module_state.treasury_policy_source,
                        "policy_version": native_store.module_state.treasury_policy_version,
                    },
                }),
                apply && applied_count > 0,
            ))
        }
        "ua_unfreezeMappedAsset" => {
            let account_id = parse_account_id(params)?;
            let mapping_key = resolve_mapped_asset_lookup_key(params, mapped_asset_state)?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let unfreeze_reason = param_as_string_any(params, &["reason", "unfreeze_reason"])
                .unwrap_or_else(|| "manual mapped asset unfreeze".to_string());
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
            if record.status != MappedAssetStatus::Frozen {
                bail!(
                    "ERR_MAPPED_UNFREEZE_STATUS_INVALID: expected frozen, got {}",
                    record.status.as_str()
                );
            }
            require_mapped_asset_anchor_safe_v1(record, "ua_unfreezeMappedAsset")?;
            let source_anchor_status = mapped_asset_source_anchor_status_v1(record);
            let settlement = apply_live_mapped_asset_m2_unfreeze_v1(
                record,
                mapping_key.as_str(),
                params,
                now,
                unfreeze_reason.as_str(),
            )?;
            record.status = MappedAssetStatus::Active;
            record.updated_at = now;
            record.audit_ref =
                derive_mapped_asset_audit_ref_v1(record.mapping_id, record.status, now);
            let op = build_mapped_asset_operation_v1(
                record,
                MappedAssetOperationKind::UnfreezeMapped,
                now,
                mapped_asset_state.operations.len() as u64 + 1,
            );
            mapped_asset_state.operations.push(op);
            emit_mapped_asset_operation_observed_v1(
                "unfreeze_mapped",
                true,
                Some(account_id.as_str()),
                Some(mapping_key.as_str()),
                None,
                Some("qualified"),
            );
            Ok((
                json!({
                    "method": method,
                    "unfrozen": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "mapping_id": mapping_key,
                    "status": record.status.as_str(),
                    "phase4_mode": mapped_asset_phase4_mode_v1(record),
                    "settlement_effect": "neth_m2_unfrozen",
                    "native_settlement": settlement,
                    "source_anchor_status": mapped_asset_anchor_status_to_json_v1(&source_anchor_status),
                }),
                true,
            ))
        }
        "ua_rollbackFrozenMappedAsset" => {
            let account_id = parse_account_id(params)?;
            let mapping_key = resolve_mapped_asset_lookup_key(params, mapped_asset_state)?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let rollback_reason = param_as_string_any(params, &["reason", "rollback_reason"])
                .unwrap_or_else(|| "governance mapped asset rollback".to_string());
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
            if record.status != MappedAssetStatus::Frozen {
                bail!(
                    "ERR_MAPPED_ROLLBACK_STATUS_INVALID: expected frozen, got {}",
                    record.status.as_str()
                );
            }
            let source_anchor_status = require_mapped_asset_anchor_rollback_eligible_v1(
                record,
                "ua_rollbackFrozenMappedAsset",
            )?;
            let settlement = apply_live_mapped_asset_m2_rollback_v1(
                record,
                mapping_key.as_str(),
                params,
                now,
                rollback_reason.as_str(),
            )?;
            record.status = MappedAssetStatus::Rejected;
            record.updated_at = now;
            record.audit_ref =
                derive_mapped_asset_audit_ref_v1(record.mapping_id, record.status, now);
            let op = build_mapped_asset_operation_v1(
                record,
                MappedAssetOperationKind::RollbackMapped,
                now,
                mapped_asset_state.operations.len() as u64 + 1,
            );
            mapped_asset_state.operations.push(op);
            emit_mapped_asset_operation_observed_v1(
                "rollback_mapped",
                true,
                Some(account_id.as_str()),
                Some(mapping_key.as_str()),
                None,
                Some("qualified"),
            );
            Ok((
                json!({
                    "method": method,
                    "rolled_back": true,
                    "account_id": account_id,
                    "uca_id": account_id,
                    "mapping_id": mapping_key,
                    "status": record.status.as_str(),
                    "phase4_mode": mapped_asset_phase4_mode_v1(record),
                    "settlement_effect": "neth_m2_rolled_back",
                    "native_settlement": settlement,
                    "source_anchor_status": mapped_asset_anchor_status_to_json_v1(&source_anchor_status),
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
            require_mapped_asset_anchor_safe_v1(record, "ua_releaseMappedLock")?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MappedBridgeGateV1 {
    Register,
    Burn,
    Release,
}

impl MappedBridgeGateV1 {
    fn error_code(self) -> &'static str {
        match self {
            Self::Register => "ERR_MAPPED_BRIDGE_PAUSED",
            Self::Burn => "ERR_MAPPED_BURN_PAUSED",
            Self::Release => "ERR_MAPPED_RELEASE_PAUSED",
        }
    }

    fn store_field(self) -> &'static str {
        match self {
            Self::Register => "mapped_lock_bridge_paused",
            Self::Burn => "mapped_asset_burn_paused",
            Self::Release => "mapped_asset_release_paused",
        }
    }

    fn env_name(self) -> &'static str {
        match self {
            Self::Register => NOVOVM_UA_MAPPED_LOCK_BRIDGE_PAUSED_ENV,
            Self::Burn => NOVOVM_UA_MAPPED_ASSET_BURN_PAUSED_ENV,
            Self::Release => NOVOVM_UA_MAPPED_ASSET_RELEASE_PAUSED_ENV,
        }
    }

    fn method(self) -> &'static str {
        match self {
            Self::Register => "ua_registerMappedLock",
            Self::Burn => "ua_burnMappedAsset",
            Self::Release => "ua_releaseMappedLock",
        }
    }
}

fn mapped_bridge_gate_paused_in_store_v1(
    store: &crate::tx_ingress::NovNativeExecutionStoreV1,
    gate: MappedBridgeGateV1,
) -> bool {
    match gate {
        MappedBridgeGateV1::Register => store.module_state.mapped_lock_bridge_paused,
        MappedBridgeGateV1::Burn => store.module_state.mapped_asset_burn_paused,
        MappedBridgeGateV1::Release => store.module_state.mapped_asset_release_paused,
    }
}

fn mapped_bridge_gate_paused_by_env_v1(gate: MappedBridgeGateV1) -> bool {
    bool_env_default(NOVOVM_UA_MAPPED_ASSET_BRIDGE_PAUSED_ENV, false)
        || bool_env_default(gate.env_name(), false)
}

fn require_mapped_bridge_gate_open_v1(params: &Value, gate: MappedBridgeGateV1) -> Result<()> {
    let store_path = native_execution_store_path_from_params_or_env_v1(params);
    let store = load_nov_native_execution_store_v1(store_path.as_path())?;
    let store_paused = mapped_bridge_gate_paused_in_store_v1(&store, gate);
    let env_paused = mapped_bridge_gate_paused_by_env_v1(gate);
    if store_paused || env_paused {
        bail!(
            "{}: {} paused by {}{}{}",
            gate.error_code(),
            gate.method(),
            if store_paused {
                gate.store_field()
            } else {
                gate.env_name()
            },
            if store_paused && env_paused { "+" } else { "" },
            if store_paused && env_paused {
                gate.env_name()
            } else {
                ""
            }
        );
    }
    Ok(())
}

fn mapped_header_source_policy_to_json_v1(
    store: &crate::tx_ingress::NovNativeExecutionStoreV1,
) -> Value {
    json!({
        "required": store.module_state.mapped_header_source_required,
        "allowed_peer_ids": store.module_state.mapped_header_source_allowed_peer_ids,
        "min_source_quorum": store.module_state.mapped_header_source_min_quorum,
        "policy_source": store.module_state.mapped_header_source_policy_source,
        "policy_version": store.module_state.mapped_header_source_policy_version,
        "updated_unix_ms": store.module_state.mapped_header_source_policy_updated_unix_ms,
    })
}

fn require_mapped_header_source_policy_v1(
    params: &Value,
    chain_id: u64,
    block_number: u64,
    block_hash: [u8; 32],
    source_peer_id: Option<u64>,
) -> Result<Value> {
    let store_path = native_execution_store_path_from_params_or_env_v1(params);
    let store = load_nov_native_execution_store_v1(store_path.as_path())?;
    let policy = mapped_header_source_policy_to_json_v1(&store);
    if !store.module_state.mapped_header_source_required {
        return Ok(json!({
            "state": "not_required",
            "policy": policy,
        }));
    }
    let Some(source_peer_id) = source_peer_id else {
        bail!(
            "ERR_MAPPED_HEADER_SOURCE_UNTRUSTED: required source policy has no source peer for chain_id={} block_number={}",
            chain_id,
            block_number
        );
    };
    if !store
        .module_state
        .mapped_header_source_allowed_peer_ids
        .contains(&source_peer_id)
    {
        bail!(
            "ERR_MAPPED_HEADER_SOURCE_UNTRUSTED: source_peer_id={} is not allowed for chain_id={} block_number={}",
            source_peer_id,
            chain_id,
            block_number
        );
    }
    let mut observed_source_peer_ids =
        novovm_network::snapshot_network_runtime_native_header_source_peers_v1(
            chain_id, block_hash,
        );
    if !observed_source_peer_ids.contains(&source_peer_id) {
        observed_source_peer_ids.push(source_peer_id);
        observed_source_peer_ids.sort_unstable();
    }
    observed_source_peer_ids.dedup();
    let observed_allowed_source_peer_ids = observed_source_peer_ids
        .iter()
        .copied()
        .filter(|peer_id| {
            store
                .module_state
                .mapped_header_source_allowed_peer_ids
                .contains(peer_id)
        })
        .collect::<Vec<_>>();
    let observed_source_quorum = observed_allowed_source_peer_ids.len() as u32;
    if observed_source_quorum < store.module_state.mapped_header_source_min_quorum {
        bail!(
            "ERR_MAPPED_HEADER_SOURCE_QUORUM_UNMET: observed_source_quorum={} min_source_quorum={} chain_id={} block_number={}",
            observed_source_quorum,
            store.module_state.mapped_header_source_min_quorum,
            chain_id,
            block_number
        );
    }
    Ok(json!({
        "state": "ok",
        "source_peer_id": source_peer_id,
        "observed_source_peer_ids": observed_source_peer_ids,
        "observed_allowed_source_peer_ids": observed_allowed_source_peer_ids,
        "observed_source_quorum": observed_source_quorum,
        "policy": policy,
    }))
}

fn mapped_header_attestation_policy_to_json_v1(
    store: &crate::tx_ingress::NovNativeExecutionStoreV1,
) -> Value {
    let active_allowed_signers = store
        .module_state
        .mapped_header_attestation_allowed_signers
        .iter()
        .filter(|signer| {
            !store
                .module_state
                .mapped_header_attestation_disabled_signers
                .contains(*signer)
        })
        .cloned()
        .collect::<Vec<_>>();
    json!({
        "required": store.module_state.mapped_header_attestation_required,
        "allowed_signers": store.module_state.mapped_header_attestation_allowed_signers,
        "disabled_signers": store.module_state.mapped_header_attestation_disabled_signers,
        "disabled_signer_reasons": store.module_state.mapped_header_attestation_disabled_signer_reasons,
        "signer_rotations": store.module_state.mapped_header_attestation_signer_rotations,
        "active_allowed_signers": active_allowed_signers,
        "min_attestation_quorum": store.module_state.mapped_header_attestation_min_quorum,
        "policy_source": store.module_state.mapped_header_attestation_policy_source,
        "policy_version": store.module_state.mapped_header_attestation_policy_version,
        "updated_unix_ms": store.module_state.mapped_header_attestation_policy_updated_unix_ms,
        "note": "governed ed25519 header attestation quorum",
    })
}

fn normalize_mapped_header_attestation_signer_v1(raw: &str) -> String {
    let lowered = raw.trim().to_ascii_lowercase();
    lowered
        .strip_prefix("0x")
        .unwrap_or(lowered.as_str())
        .to_string()
}

fn mapped_header_attestation_message_v1(
    chain_id: u64,
    block_number: u64,
    block_hash: [u8; 32],
    receipts_root: [u8; 32],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"novovm-mapped-header-attestation-v1");
    out.push(0);
    out.extend_from_slice(&chain_id.to_be_bytes());
    out.push(0);
    out.extend_from_slice(&block_number.to_be_bytes());
    out.push(0);
    out.extend_from_slice(&block_hash);
    out.push(0);
    out.extend_from_slice(&receipts_root);
    out
}

fn validate_mapped_header_attestation_signer_v1(signer: &str) -> Result<()> {
    let public_key = decode_hex_bytes(signer, "mapped_header_attestation_allowed_signer")?;
    let public_key: [u8; 32] = public_key.try_into().map_err(|_| {
        anyhow::anyhow!(
            "ERR_MAPPED_HEADER_ATTESTATION_POLICY_INVALID: ed25519 signer public key must be 32 bytes"
        )
    })?;
    Ed25519VerifyingKey::from_bytes(&public_key).map_err(|err| {
        anyhow::anyhow!(
            "ERR_MAPPED_HEADER_ATTESTATION_POLICY_INVALID: invalid ed25519 public key: {err}"
        )
    })?;
    Ok(())
}

fn parse_mapped_header_attestation_disabled_reasons_v1(
    params: &Value,
    disabled_signers: &[String],
) -> Result<BTreeMap<String, String>> {
    let default_reason = param_as_string_any(params, &["disable_reason", "slashing_reason"])
        .unwrap_or_else(|| "governance_disabled".to_string());
    let mut out = BTreeMap::new();
    if let Some(value) = param_value_any(
        params,
        &[
            "disabled_signer_reasons",
            "disabled_attestation_signer_reasons",
            "slashing_reasons",
        ],
    ) {
        let Value::Object(map) = value else {
            bail!(
                "ERR_MAPPED_HEADER_ATTESTATION_POLICY_INVALID: disabled_signer_reasons must be object"
            );
        };
        for (raw_signer, value) in map {
            let signer = normalize_mapped_header_attestation_signer_v1(raw_signer);
            validate_mapped_header_attestation_signer_v1(signer.as_str())?;
            if !disabled_signers.contains(&signer) {
                bail!(
                    "ERR_MAPPED_HEADER_ATTESTATION_POLICY_INVALID: disabled_signer_reasons contains non-disabled signer {}",
                    signer
                );
            }
            let Some(raw_reason) = value.as_str() else {
                bail!(
                    "ERR_MAPPED_HEADER_ATTESTATION_POLICY_INVALID: disabled_signer_reasons[{}] must be string",
                    signer
                );
            };
            let reason = raw_reason.trim();
            if reason.is_empty() {
                bail!(
                    "ERR_MAPPED_HEADER_ATTESTATION_POLICY_INVALID: disabled_signer_reasons[{}] must not be empty",
                    signer
                );
            }
            out.insert(signer, reason.to_string());
        }
    }
    for signer in disabled_signers {
        out.entry(signer.clone())
            .or_insert_with(|| default_reason.clone());
    }
    Ok(out)
}

fn parse_mapped_header_attestation_rotations_v1(
    params: &Value,
    allowed_signers: &[String],
    disabled_signers: &[String],
) -> Result<BTreeMap<String, String>> {
    let Some(value) = param_value_any(
        params,
        &[
            "signer_rotations",
            "attestation_signer_rotations",
            "rotated_signers",
        ],
    ) else {
        return Ok(BTreeMap::new());
    };
    let Value::Object(map) = value else {
        bail!("ERR_MAPPED_HEADER_ATTESTATION_POLICY_INVALID: signer_rotations must be object");
    };
    let mut out = BTreeMap::new();
    for (raw_old_signer, raw_new_value) in map {
        let old_signer = normalize_mapped_header_attestation_signer_v1(raw_old_signer);
        validate_mapped_header_attestation_signer_v1(old_signer.as_str())?;
        let Some(raw_new_signer) = raw_new_value.as_str() else {
            bail!(
                "ERR_MAPPED_HEADER_ATTESTATION_POLICY_INVALID: signer_rotations[{}] must be string",
                old_signer
            );
        };
        let new_signer = normalize_mapped_header_attestation_signer_v1(raw_new_signer);
        validate_mapped_header_attestation_signer_v1(new_signer.as_str())?;
        if old_signer == new_signer {
            bail!(
                "ERR_MAPPED_HEADER_ATTESTATION_POLICY_INVALID: signer rotation old and new signer are identical"
            );
        }
        if !disabled_signers.contains(&old_signer) {
            bail!(
                "ERR_MAPPED_HEADER_ATTESTATION_POLICY_INVALID: rotated old signer {} must be disabled",
                old_signer
            );
        }
        if !allowed_signers.contains(&new_signer) || disabled_signers.contains(&new_signer) {
            bail!(
                "ERR_MAPPED_HEADER_ATTESTATION_POLICY_INVALID: rotated new signer {} must be active allowed signer",
                new_signer
            );
        }
        out.insert(old_signer, new_signer);
    }
    Ok(out)
}

fn mapped_header_attestation_items_v1(params: &Value) -> Result<Vec<(String, Vec<u8>)>> {
    let Some(value) = param_value_any(params, &["header_attestations", "attestations"]) else {
        return Ok(Vec::new());
    };
    let Value::Array(items) = value else {
        bail!("ERR_MAPPED_HEADER_ATTESTATION_INVALID: header_attestations must be array");
    };
    items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let signer = item
                .get("signer")
                .or_else(|| item.get("public_key"))
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "ERR_MAPPED_HEADER_ATTESTATION_INVALID: header_attestations[{idx}].signer is required"
                    )
                })?;
            let signature = item
                .get("signature")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "ERR_MAPPED_HEADER_ATTESTATION_INVALID: header_attestations[{idx}].signature is required"
                    )
                })?;
            Ok((
                normalize_mapped_header_attestation_signer_v1(signer),
                decode_hex_bytes(
                    signature,
                    &format!("header_attestations[{idx}].signature"),
                )?,
            ))
        })
        .collect()
}

fn require_mapped_header_attestation_policy_v1(
    params: &Value,
    chain_id: u64,
    block_number: u64,
    block_hash: [u8; 32],
    receipts_root: [u8; 32],
) -> Result<Value> {
    let store_path = native_execution_store_path_from_params_or_env_v1(params);
    let store = load_nov_native_execution_store_v1(store_path.as_path())?;
    let policy = mapped_header_attestation_policy_to_json_v1(&store);
    if !store.module_state.mapped_header_attestation_required {
        return Ok(json!({
            "state": "not_required",
            "policy": policy,
        }));
    }
    let message =
        mapped_header_attestation_message_v1(chain_id, block_number, block_hash, receipts_root);
    let mut observed_signers = Vec::new();
    let mut observed_allowed_signers = Vec::new();
    for (signer, signature) in mapped_header_attestation_items_v1(params)? {
        if signer.is_empty() {
            continue;
        }
        observed_signers.push(signer.clone());
        if !store
            .module_state
            .mapped_header_attestation_allowed_signers
            .contains(&signer)
            || store
                .module_state
                .mapped_header_attestation_disabled_signers
                .contains(&signer)
        {
            continue;
        }
        let public_key = decode_hex_bytes(&signer, "header_attestation_signer")?;
        let public_key: [u8; 32] = public_key.try_into().map_err(|_| {
            anyhow::anyhow!(
                "ERR_MAPPED_HEADER_ATTESTATION_INVALID: ed25519 signer public key must be 32 bytes"
            )
        })?;
        let verifying_key = Ed25519VerifyingKey::from_bytes(&public_key).map_err(|err| {
            anyhow::anyhow!(
                "ERR_MAPPED_HEADER_ATTESTATION_INVALID: invalid ed25519 public key: {err}"
            )
        })?;
        let signature = Ed25519Signature::from_slice(signature.as_slice()).map_err(|err| {
            anyhow::anyhow!(
                "ERR_MAPPED_HEADER_ATTESTATION_INVALID: invalid ed25519 signature: {err}"
            )
        })?;
        verifying_key
            .verify(message.as_slice(), &signature)
            .map_err(|err| {
                anyhow::anyhow!(
                    "ERR_MAPPED_HEADER_ATTESTATION_INVALID: signature verification failed for signer {}: {}",
                    signer,
                    err
                )
            })?;
        observed_allowed_signers.push(signer);
    }
    observed_signers.sort();
    observed_signers.dedup();
    observed_allowed_signers.sort();
    observed_allowed_signers.dedup();
    let observed_attestation_quorum = observed_allowed_signers.len() as u32;
    if observed_attestation_quorum < store.module_state.mapped_header_attestation_min_quorum {
        bail!(
            "ERR_MAPPED_HEADER_ATTESTATION_QUORUM_UNMET: observed_attestation_quorum={} min_attestation_quorum={} chain_id={} block_number={}",
            observed_attestation_quorum,
            store.module_state.mapped_header_attestation_min_quorum,
            chain_id,
            block_number
        );
    }
    Ok(json!({
        "state": "ok",
        "observed_signers": observed_signers,
        "observed_allowed_signers": observed_allowed_signers,
        "observed_attestation_quorum": observed_attestation_quorum,
        "policy": policy,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MappedAssetAnchorStatusV1 {
    state: &'static str,
    reason: Option<String>,
    source_chain_id: Option<u64>,
    source_block_number: Option<u64>,
    canonical: Option<bool>,
    finalized: Option<bool>,
    receipts_root_match: Option<bool>,
}

impl MappedAssetAnchorStatusV1 {
    fn ok() -> Self {
        Self {
            state: "ok",
            reason: None,
            source_chain_id: None,
            source_block_number: None,
            canonical: Some(true),
            finalized: Some(true),
            receipts_root_match: Some(true),
        }
    }

    fn not_required() -> Self {
        Self {
            state: "not_required",
            reason: Some(
                "shadow or non-live mapped asset does not require source anchor recheck"
                    .to_string(),
            ),
            source_chain_id: None,
            source_block_number: None,
            canonical: None,
            finalized: None,
            receipts_root_match: None,
        }
    }

    fn blocked(reason: String, chain_id: Option<u64>, block_number: Option<u64>) -> Self {
        Self {
            state: "blocked",
            reason: Some(reason),
            source_chain_id: chain_id,
            source_block_number: block_number,
            canonical: None,
            finalized: None,
            receipts_root_match: None,
        }
    }
}

fn mapped_asset_anchor_status_to_json_v1(status: &MappedAssetAnchorStatusV1) -> Value {
    json!({
        "state": status.state,
        "reason": status.reason,
        "source_chain_id": status.source_chain_id,
        "source_block_number": status.source_block_number,
        "canonical": status.canonical,
        "finalized": status.finalized,
        "receipts_root_match": status.receipts_root_match,
    })
}

fn mapped_asset_source_anchor_status_v1(record: &MappedAssetRecord) -> MappedAssetAnchorStatusV1 {
    if mapped_asset_is_shadow_mode_v1(record) {
        return MappedAssetAnchorStatusV1::not_required();
    }
    let chain_id = record.source_chain_id;
    let block_number = record.source_block_number;
    let Some(chain_id) = chain_id else {
        return MappedAssetAnchorStatusV1::blocked(
            "missing source_chain_id anchor".to_string(),
            chain_id,
            block_number,
        );
    };
    let Some(block_number) = block_number else {
        return MappedAssetAnchorStatusV1::blocked(
            "missing source_block_number anchor".to_string(),
            Some(chain_id),
            block_number,
        );
    };
    let block_hash: [u8; 32] = match record.source_block_hash.clone().try_into() {
        Ok(value) => value,
        Err(_) => {
            return MappedAssetAnchorStatusV1::blocked(
                "missing or invalid source_block_hash anchor".to_string(),
                Some(chain_id),
                Some(block_number),
            );
        }
    };
    let receipts_root: [u8; 32] = match record.source_receipts_root.clone().try_into() {
        Ok(value) => value,
        Err(_) => {
            return MappedAssetAnchorStatusV1::blocked(
                "missing or invalid source_receipts_root anchor".to_string(),
                Some(chain_id),
                Some(block_number),
            );
        }
    };
    let blocks = novovm_network::snapshot_network_runtime_native_canonical_blocks_v1(chain_id, 0);
    let Some(block) = blocks
        .iter()
        .find(|block| block.number == block_number && block.hash == block_hash)
    else {
        return MappedAssetAnchorStatusV1::blocked(
            "trusted source block anchor is unavailable".to_string(),
            Some(chain_id),
            Some(block_number),
        );
    };
    let receipts_root_match = block.receipts_root == Some(receipts_root);
    let canonical = block.canonical && block.header_observed;
    let finalized = block.finalized;
    if canonical && finalized && receipts_root_match {
        let mut ok = MappedAssetAnchorStatusV1::ok();
        ok.source_chain_id = Some(chain_id);
        ok.source_block_number = Some(block_number);
        return ok;
    }
    MappedAssetAnchorStatusV1 {
        state: "blocked",
        reason: Some(format!(
            "source anchor unsafe: canonical={} finalized={} receipts_root_match={}",
            canonical, finalized, receipts_root_match
        )),
        source_chain_id: Some(chain_id),
        source_block_number: Some(block_number),
        canonical: Some(canonical),
        finalized: Some(finalized),
        receipts_root_match: Some(receipts_root_match),
    }
}

fn require_mapped_asset_anchor_safe_v1(record: &MappedAssetRecord, operation: &str) -> Result<()> {
    let status = mapped_asset_source_anchor_status_v1(record);
    if status.state == "ok" || status.state == "not_required" {
        return Ok(());
    }
    bail!(
        "ERR_MAPPED_ASSET_SOURCE_ANCHOR_UNSAFE: {} blocked for mapping_id={} reason={}",
        operation,
        mapped_asset_hex_id(&record.mapping_id),
        status
            .reason
            .unwrap_or_else(|| "source anchor unsafe".to_string())
    )
}

fn require_mapped_asset_anchor_rollback_eligible_v1(
    record: &MappedAssetRecord,
    operation: &str,
) -> Result<MappedAssetAnchorStatusV1> {
    let status = mapped_asset_source_anchor_status_v1(record);
    if status.state == "blocked" || status.state == "not_required" {
        return Ok(status);
    }
    bail!(
        "ERR_MAPPED_ROLLBACK_ANCHOR_STILL_SAFE: {} requires unsafe source anchor for mapping_id={}",
        operation,
        mapped_asset_hex_id(&record.mapping_id)
    )
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
    require_mapped_bridge_gate_open_v1(params, MappedBridgeGateV1::Register)?;
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
    require_mapped_bridge_gate_open_v1(params, MappedBridgeGateV1::Burn)?;
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

fn apply_live_mapped_asset_m2_freeze_v1(
    record: &MappedAssetRecord,
    mapping_key: &str,
    params: &Value,
    now: u64,
    reason: &str,
) -> Result<Value> {
    if mapped_asset_is_shadow_mode_v1(record) {
        return Ok(json!({
            "applied": false,
            "mode": "shadow",
            "effect": "none",
            "reason": "shadow mode freeze only updates mapped asset status",
        }));
    }
    let store_path = native_execution_store_path_from_params_or_env_v1(params);
    let account_key = normalize_account_view_key_v1(&record.target_account_id);
    let asset_key = normalize_asset_view_symbol_v1(&record.target_asset_symbol);
    let mut store = load_nov_native_execution_store_v1(store_path.as_path())?;
    let user_balance_after = if record.status == MappedAssetStatus::Active {
        let balances = store
            .module_state
            .account_asset_balances
            .entry(account_key.clone())
            .or_default();
        let entry = balances.entry(asset_key.clone()).or_insert(0);
        if *entry < record.amount {
            bail!(
                "ERR_MAPPED_FREEZE_NATIVE_BALANCE_INSUFFICIENT: account={} asset={} requested={} available={}",
                account_key,
                asset_key,
                record.amount,
                *entry
            );
        }
        *entry = entry.saturating_sub(record.amount);
        *entry
    } else {
        store
            .module_state
            .account_asset_balances
            .get(account_key.as_str())
            .and_then(|assets| assets.get(asset_key.as_str()).copied())
            .unwrap_or(0)
    };
    append_ua_treasury_journal_v1(
        &mut store,
        mapped_asset_live_settlement_journal_v1(
            "mapped_asset_m2_frozen",
            record,
            mapping_key,
            mapped_asset_hex_id(&record.mapping_id).as_str(),
            "frozen",
            reason,
            now,
        ),
    );
    store.last_updated_unix_ms = u128::from(now).saturating_mul(1000);
    save_nov_native_execution_store_v1(store_path.as_path(), &store)?;
    Ok(json!({
        "applied": true,
        "mode": "live",
        "effect": "neth_m2_frozen",
        "asset": asset_key,
        "amount": record.amount,
        "account_balance_after": user_balance_after,
        "treasury_reserve_unchanged": true,
        "reason": reason,
        "store_path": store_path.display().to_string(),
    }))
}

fn apply_live_mapped_asset_m2_unfreeze_v1(
    record: &MappedAssetRecord,
    mapping_key: &str,
    params: &Value,
    now: u64,
    reason: &str,
) -> Result<Value> {
    if mapped_asset_is_shadow_mode_v1(record) {
        return Ok(json!({
            "applied": false,
            "mode": "shadow",
            "effect": "none",
            "reason": "shadow mode unfreeze only updates mapped asset status",
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
    append_ua_treasury_journal_v1(
        &mut store,
        mapped_asset_live_settlement_journal_v1(
            "mapped_asset_m2_unfrozen",
            record,
            mapping_key,
            mapped_asset_hex_id(&record.mapping_id).as_str(),
            "active",
            reason,
            now,
        ),
    );
    store.last_updated_unix_ms = u128::from(now).saturating_mul(1000);
    save_nov_native_execution_store_v1(store_path.as_path(), &store)?;
    Ok(json!({
        "applied": true,
        "mode": "live",
        "effect": "neth_m2_unfrozen",
        "asset": asset_key,
        "amount": record.amount,
        "account_balance_after": user_balance_after,
        "treasury_reserve_unchanged": true,
        "reason": reason,
        "store_path": store_path.display().to_string(),
    }))
}

fn apply_live_mapped_asset_m2_rollback_v1(
    record: &MappedAssetRecord,
    mapping_key: &str,
    params: &Value,
    now: u64,
    reason: &str,
) -> Result<Value> {
    if mapped_asset_is_shadow_mode_v1(record) {
        return Ok(json!({
            "applied": false,
            "mode": "shadow",
            "effect": "none",
            "reason": "shadow mode rollback only updates mapped asset status",
        }));
    }
    let store_path = native_execution_store_path_from_params_or_env_v1(params);
    let account_key = normalize_account_view_key_v1(&record.target_account_id);
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
                "ERR_MAPPED_ROLLBACK_TREASURY_RESERVE_INSUFFICIENT: asset={} requested={} available={}",
                asset_key,
                record.amount,
                *entry
            );
        }
        *entry = entry.saturating_sub(record.amount);
        *entry
    };
    let account_balance_unchanged = store
        .module_state
        .account_asset_balances
        .get(account_key.as_str())
        .and_then(|assets| assets.get(asset_key.as_str()).copied())
        .unwrap_or(0);
    append_ua_treasury_journal_v1(
        &mut store,
        mapped_asset_live_settlement_journal_v1(
            "mapped_asset_m2_rolled_back",
            record,
            mapping_key,
            mapped_asset_hex_id(&record.mapping_id).as_str(),
            "rejected",
            reason,
            now,
        ),
    );
    store.last_updated_unix_ms = u128::from(now).saturating_mul(1000);
    save_nov_native_execution_store_v1(store_path.as_path(), &store)?;
    Ok(json!({
        "applied": true,
        "mode": "live",
        "effect": "neth_m2_rolled_back",
        "asset": asset_key,
        "amount": record.amount,
        "account_balance_unchanged": account_balance_unchanged,
        "treasury_reserve_after": reserve_after,
        "nov_minted": 0,
        "external_release_triggered": false,
        "reason": reason,
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
    require_mapped_bridge_gate_open_v1(params, MappedBridgeGateV1::Release)?;
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

fn value_as_u64_list(value: &Value) -> Option<Vec<u64>> {
    match value {
        Value::Array(items) => Some(items.iter().filter_map(value_as_u64).collect()),
        Value::String(raw) => Some(
            raw.split([',', ';', ' '])
                .filter_map(|item| {
                    let trimmed = item.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        parse_hex_u64(trimmed).or_else(|| trimmed.parse::<u64>().ok())
                    }
                })
                .collect(),
        ),
        Value::Number(_) => value_as_u64(value).map(|item| vec![item]),
        _ => None,
    }
}

fn param_as_u64_list(params: &Value, key: &str) -> Option<Vec<u64>> {
    match params {
        Value::Object(map) => map.get(key).and_then(value_as_u64_list),
        Value::Array(items) => items
            .first()
            .and_then(|value| value.get(key))
            .and_then(value_as_u64_list),
        _ => None,
    }
}

fn param_as_u64_list_any(params: &Value, keys: &[&str]) -> Option<Vec<u64>> {
    keys.iter().find_map(|key| param_as_u64_list(params, key))
}

fn value_as_string_list(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::Array(items) => Some(
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect(),
        ),
        Value::String(raw) => Some(
            raw.split([',', ';', ' '])
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect(),
        ),
        _ => None,
    }
}

fn param_as_string_list(params: &Value, key: &str) -> Option<Vec<String>> {
    match params {
        Value::Object(map) => map.get(key).and_then(value_as_string_list),
        Value::Array(items) => items
            .first()
            .and_then(|value| value.get(key))
            .and_then(value_as_string_list),
        _ => None,
    }
}

fn param_as_string_list_any(params: &Value, keys: &[&str]) -> Option<Vec<String>> {
    keys.iter()
        .find_map(|key| param_as_string_list(params, key))
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
    chain_id: u64,
    contract_address: [u8; 20],
    topic0: [u8; 32],
    block_number: u64,
    block_hash: [u8; 32],
    finalized_block_number: u64,
    log_index: u64,
    receipts_root: [u8; 32],
    receipt_index: u64,
    receipt_log_index: u64,
    receipt_proof: Vec<Vec<u8>>,
    receipt_envelope: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy)]
enum UaRlpItemV1<'a> {
    Bytes(&'a [u8]),
    List(&'a [u8]),
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

fn eth_lock_min_confirmations_from_policy_v1(params: &Value) -> Result<(u64, &'static str)> {
    let store_path = native_execution_store_path_from_params_or_env_v1(params);
    let store = load_nov_native_execution_store_v1(store_path.as_path())?;
    if store.module_state.mapped_lock_min_confirmations > 0 {
        return Ok((
            store.module_state.mapped_lock_min_confirmations,
            "governance_native_store",
        ));
    }
    Ok((eth_lock_min_confirmations_v1(), "env_or_default"))
}

fn ethereum_lock_chain_id_v1(params: &Value) -> u64 {
    param_value_any(params, &["source_chain_id", "chain_id", "eth_chain_id"])
        .and_then(value_as_u64)
        .unwrap_or(1)
}

fn param_value_any<'a>(params: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    match params {
        Value::Object(map) => keys.iter().find_map(|key| map.get(*key)),
        Value::Array(items) => items
            .first()
            .and_then(|value| keys.iter().find_map(|key| value.get(*key))),
        _ => None,
    }
}

fn parse_hex_vec_list_param_v1(params: &Value, keys: &[&str], field: &str) -> Result<Vec<Vec<u8>>> {
    let value = param_value_any(params, keys)
        .ok_or_else(|| anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: {field} is required"))?;
    let Value::Array(items) = value else {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: {field} must be array");
    };
    if items.is_empty() {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: {field} must not be empty");
    }
    items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let Some(raw) = item.as_str() else {
                bail!("ERR_MAPPED_LOCK_PROOF_INVALID: {field}[{idx}] must be hex string");
            };
            decode_hex_bytes(raw, &format!("{field}[{idx}]"))
        })
        .collect()
}

fn ua_rlp_encode_u64_v1(value: u64) -> Vec<u8> {
    if value == 0 {
        return vec![0x80];
    }
    if value < 0x80 {
        return vec![value as u8];
    }
    let mut bytes = Vec::new();
    let mut cursor = value;
    while cursor > 0 {
        bytes.push((cursor & 0xff) as u8);
        cursor >>= 8;
    }
    bytes.reverse();
    let mut out = Vec::with_capacity(1 + bytes.len());
    out.push(0x80 + bytes.len() as u8);
    out.extend(bytes);
    out
}

fn ua_rlp_parse_item_v1(input: &[u8]) -> Result<(UaRlpItemV1<'_>, usize)> {
    if input.is_empty() {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt rlp is empty");
    }
    let lead = input[0];
    match lead {
        0x00..=0x7f => Ok((UaRlpItemV1::Bytes(&input[..1]), 1)),
        0x80..=0xb7 => {
            let len = (lead - 0x80) as usize;
            if input.len() < 1 + len {
                bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt rlp short bytes");
            }
            Ok((UaRlpItemV1::Bytes(&input[1..1 + len]), 1 + len))
        }
        0xb8..=0xbf => {
            let len_of_len = (lead - 0xb7) as usize;
            if input.len() < 1 + len_of_len {
                bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt rlp short bytes len");
            }
            let mut len = 0usize;
            for byte in &input[1..1 + len_of_len] {
                len = (len << 8) | (*byte as usize);
            }
            if input.len() < 1 + len_of_len + len {
                bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt rlp short bytes payload");
            }
            Ok((
                UaRlpItemV1::Bytes(&input[1 + len_of_len..1 + len_of_len + len]),
                1 + len_of_len + len,
            ))
        }
        0xc0..=0xf7 => {
            let len = (lead - 0xc0) as usize;
            if input.len() < 1 + len {
                bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt rlp short list");
            }
            Ok((UaRlpItemV1::List(&input[1..1 + len]), 1 + len))
        }
        _ => {
            let len_of_len = (lead - 0xf7) as usize;
            if input.len() < 1 + len_of_len {
                bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt rlp short list len");
            }
            let mut len = 0usize;
            for byte in &input[1..1 + len_of_len] {
                len = (len << 8) | (*byte as usize);
            }
            if input.len() < 1 + len_of_len + len {
                bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt rlp short list payload");
            }
            Ok((
                UaRlpItemV1::List(&input[1 + len_of_len..1 + len_of_len + len]),
                1 + len_of_len + len,
            ))
        }
    }
}

fn ua_rlp_parse_list_items_v1(payload: &[u8]) -> Result<Vec<UaRlpItemV1<'_>>> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while cursor < payload.len() {
        let (item, consumed) = ua_rlp_parse_item_v1(&payload[cursor..])?;
        out.push(item);
        cursor = cursor.saturating_add(consumed);
    }
    if cursor != payload.len() {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt rlp list trailing");
    }
    Ok(out)
}

fn verify_receipt_log_matches_lock_event_v1(
    receipt_envelope: &[u8],
    receipt_log_index: u64,
    contract_address: &[u8; 20],
    topic0: &[u8; 32],
) -> Result<()> {
    let receipt_payload = if !receipt_envelope.is_empty()
        && receipt_envelope[0] <= 0x7f
        && receipt_envelope.len() > 1
    {
        let (item, consumed) = ua_rlp_parse_item_v1(&receipt_envelope[1..])?;
        if consumed + 1 != receipt_envelope.len() {
            bail!("ERR_MAPPED_LOCK_PROOF_INVALID: typed receipt rlp trailing");
        }
        let UaRlpItemV1::List(payload) = item else {
            bail!("ERR_MAPPED_LOCK_PROOF_INVALID: typed receipt payload must be list");
        };
        payload
    } else {
        let (item, consumed) = ua_rlp_parse_item_v1(receipt_envelope)?;
        if consumed != receipt_envelope.len() {
            bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt rlp trailing");
        }
        let UaRlpItemV1::List(payload) = item else {
            bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt payload must be list");
        };
        payload
    };
    let fields = ua_rlp_parse_list_items_v1(receipt_payload)?;
    if fields.len() != 4 {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt must have 4 fields");
    }
    let UaRlpItemV1::Bytes(status_or_post_state) = fields[0] else {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt status must be bytes");
    };
    if status_or_post_state != [1u8] {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: lock receipt status must be success");
    }
    let UaRlpItemV1::List(logs_payload) = fields[3] else {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt logs must be list");
    };
    let logs = ua_rlp_parse_list_items_v1(logs_payload)?;
    let idx = usize::try_from(receipt_log_index).map_err(|_| {
        anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt_log_index too large")
    })?;
    let Some(log) = logs.get(idx).copied() else {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt_log_index out of range");
    };
    let UaRlpItemV1::List(log_payload) = log else {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt log must be list");
    };
    let log_fields = ua_rlp_parse_list_items_v1(log_payload)?;
    if log_fields.len() < 2 {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt log fields missing");
    }
    let UaRlpItemV1::Bytes(address) = log_fields[0] else {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt log address must be bytes");
    };
    if address != contract_address {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt log address does not match lock contract");
    }
    let UaRlpItemV1::List(topics_payload) = log_fields[1] else {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt log topics must be list");
    };
    let topics = ua_rlp_parse_list_items_v1(topics_payload)?;
    let Some(first_topic) = topics.first().copied() else {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt log topic0 missing");
    };
    let UaRlpItemV1::Bytes(found_topic0) = first_topic else {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt log topic0 must be bytes");
    };
    if found_topic0 != topic0 {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt log topic0 mismatch");
    }
    Ok(())
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
        || param_as_string_any(params, &["block_hash", "eth_block_hash"]).is_some()
        || param_as_u64(params, "finalized_block_number").is_some()
        || param_as_u64(params, "log_index").is_some()
        || param_as_string_any(params, &["receipts_root", "receipt_root"]).is_some()
        || param_value_any(params, &["receipt_proof", "receipt_mpt_proof"]).is_some();
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
    let block_hash_raw = param_as_string_any(params, &["block_hash", "eth_block_hash"])
        .ok_or_else(|| anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: block_hash is required"))?;
    let finalized_block_number =
        param_as_u64(params, "finalized_block_number").ok_or_else(|| {
            anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: finalized_block_number is required")
        })?;
    let log_index = param_as_u64(params, "log_index")
        .ok_or_else(|| anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: log_index is required"))?;
    let receipts_root_raw = param_as_string_any(params, &["receipts_root", "receipt_root"])
        .ok_or_else(|| {
            anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: receipts_root is required")
        })?;
    let receipt_index = param_as_u64(params, "receipt_index").ok_or_else(|| {
        anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt_index is required")
    })?;
    let receipt_log_index = param_as_u64(params, "receipt_log_index").unwrap_or(0);
    let receipt_proof = parse_hex_vec_list_param_v1(
        params,
        &["receipt_proof", "receipt_mpt_proof"],
        "receipt_proof",
    )?;
    let receipt_envelope = param_as_string_any(params, &["receipt_envelope", "raw_receipt"])
        .map(|raw| decode_hex_bytes(raw.as_str(), "receipt_envelope"))
        .transpose()?;
    Ok(Some(EthereumLockEventEvidenceV1 {
        chain_id: ethereum_lock_chain_id_v1(params),
        contract_address: decode_hex_fixed_20(contract_raw.as_str(), "lock_contract_address")?,
        topic0: decode_hex_fixed_32(topic0_raw.as_str(), "event_topic0")?,
        block_number,
        block_hash: decode_hex_fixed_32(block_hash_raw.as_str(), "block_hash")?,
        finalized_block_number,
        log_index,
        receipts_root: decode_hex_fixed_32(receipts_root_raw.as_str(), "receipts_root")?,
        receipt_index,
        receipt_log_index,
        receipt_proof,
        receipt_envelope,
    }))
}

fn ethereum_lock_event_ref_digest_v1(
    proof: &MappedAssetLockProof,
    evidence: &EthereumLockEventEvidenceV1,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm-ethereum-lock-event-ref-v1");
    hasher.update([0u8]);
    hasher.update(evidence.chain_id.to_be_bytes());
    hasher.update([0u8]);
    hasher.update(evidence.contract_address);
    hasher.update([0u8]);
    hasher.update(evidence.topic0);
    hasher.update([0u8]);
    hasher.update(evidence.block_number.to_be_bytes());
    hasher.update([0u8]);
    hasher.update(evidence.block_hash);
    hasher.update([0u8]);
    hasher.update(evidence.finalized_block_number.to_be_bytes());
    hasher.update([0u8]);
    hasher.update(evidence.log_index.to_be_bytes());
    hasher.update([0u8]);
    hasher.update(evidence.receipts_root);
    hasher.update([0u8]);
    hasher.update(evidence.receipt_index.to_be_bytes());
    hasher.update([0u8]);
    hasher.update(evidence.receipt_log_index.to_be_bytes());
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

fn verify_ethereum_lock_event_trusted_anchor_v1(
    evidence: &EthereumLockEventEvidenceV1,
    params: &Value,
) -> Result<()> {
    let blocks =
        novovm_network::snapshot_network_runtime_native_canonical_blocks_v1(evidence.chain_id, 0);
    let Some(block) = blocks
        .iter()
        .find(|block| block.number == evidence.block_number && block.hash == evidence.block_hash)
    else {
        bail!(
            "ERR_MAPPED_LOCK_PROOF_INVALID: trusted Ethereum canonical block is unavailable for chain_id={} block_number={}",
            evidence.chain_id,
            evidence.block_number
        );
    };
    if !block.header_observed || !block.canonical {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: trusted Ethereum block is not canonical");
    }
    if !block.finalized {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: trusted Ethereum block is not finalized");
    }
    if block.receipts_root != Some(evidence.receipts_root) {
        bail!(
            "ERR_MAPPED_LOCK_PROOF_INVALID: receipts_root does not match trusted Ethereum header"
        );
    }
    require_mapped_header_source_policy_v1(
        params,
        evidence.chain_id,
        evidence.block_number,
        evidence.block_hash,
        block.source_peer_id,
    )?;
    require_mapped_header_attestation_policy_v1(
        params,
        evidence.chain_id,
        evidence.block_number,
        evidence.block_hash,
        evidence.receipts_root,
    )?;
    Ok(())
}

fn verify_ethereum_lock_event_evidence_v1(
    proof: &MappedAssetLockProof,
    params: &Value,
    live_required: bool,
) -> Result<Option<EthereumLockEventEvidenceV1>> {
    let Some(evidence) = parse_ethereum_lock_event_evidence_v1(params)? else {
        if live_required {
            bail!(
                "ERR_MAPPED_LOCK_PROOF_INVALID: live mapped lock requires structured Ethereum lock event evidence"
            );
        }
        return Ok(None);
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
    let (min_confirmations, min_confirmations_source) =
        eth_lock_min_confirmations_from_policy_v1(params)?;
    let required_finalized = evidence.block_number.saturating_add(min_confirmations);
    if evidence.finalized_block_number < required_finalized {
        bail!(
            "ERR_MAPPED_LOCK_PROOF_INVALID: finalized_block_number {} is below required {} min_confirmations={} source={}",
            evidence.finalized_block_number,
            required_finalized,
            min_confirmations,
            min_confirmations_source
        );
    }
    verify_ethereum_lock_event_trusted_anchor_v1(&evidence, params)?;
    let proven_receipt = novovm_network::eth_rlpx_mpt_verify_proof_value_v1(
        evidence.receipts_root,
        ua_rlp_encode_u64_v1(evidence.receipt_index).as_slice(),
        evidence.receipt_proof.as_slice(),
    )
    .map_err(|err| anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt proof invalid: {err}"))?
    .ok_or_else(|| anyhow::anyhow!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt proof missing value"))?;
    if let Some(expected) = &evidence.receipt_envelope {
        if expected.as_slice() != proven_receipt.as_slice() {
            bail!("ERR_MAPPED_LOCK_PROOF_INVALID: receipt_envelope does not match proof value");
        }
    }
    verify_receipt_log_matches_lock_event_v1(
        proven_receipt.as_slice(),
        evidence.receipt_log_index,
        &evidence.contract_address,
        &evidence.topic0,
    )?;
    let expected_ref = ethereum_lock_event_ref_digest_v1(proof, &evidence);
    if proof.source_lock_ref.as_slice() != expected_ref.as_slice() {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: source_lock_ref does not match Ethereum lock event evidence");
    }
    Ok(Some(evidence))
}

fn verify_mapped_lock_proof(
    proof: &MappedAssetLockProof,
    params: &Value,
    live_required: bool,
) -> Result<Option<EthereumLockEventEvidenceV1>> {
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
    let evidence = verify_ethereum_lock_event_evidence_v1(proof, params, live_required)?;
    let digest = mapped_lock_proof_digest_v1(proof);
    if proof.proof_payload.as_slice() != digest.as_slice() {
        bail!("ERR_MAPPED_LOCK_PROOF_INVALID: proof payload digest mismatch");
    }
    Ok(evidence)
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
        "source_chain_id": record.source_chain_id,
        "source_block_number": record.source_block_number,
        "source_block_hash": if record.source_block_hash.is_empty() {
            Value::Null
        } else {
            json!(format!("0x{}", to_hex_lower(&record.source_block_hash)))
        },
        "source_receipts_root": if record.source_receipts_root.is_empty() {
            Value::Null
        } else {
            json!(format!("0x{}", to_hex_lower(&record.source_receipts_root)))
        },
        "source_finalized_block_number": record.source_finalized_block_number,
        "source_log_index": record.source_log_index,
        "source_receipt_index": record.source_receipt_index,
        "source_receipt_log_index": record.source_receipt_log_index,
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

    fn test_rlp_encode_len(prefix_small: u8, prefix_long: u8, len: usize) -> Vec<u8> {
        if len <= 55 {
            return vec![prefix_small + len as u8];
        }
        let mut len_bytes = Vec::new();
        let mut cursor = len;
        while cursor > 0 {
            len_bytes.push((cursor & 0xff) as u8);
            cursor >>= 8;
        }
        len_bytes.reverse();
        let mut out = Vec::with_capacity(1 + len_bytes.len());
        out.push(prefix_long + len_bytes.len() as u8);
        out.extend(len_bytes);
        out
    }

    fn test_rlp_encode_bytes(bytes: &[u8]) -> Vec<u8> {
        if bytes.len() == 1 && bytes[0] < 0x80 {
            return vec![bytes[0]];
        }
        let mut out = test_rlp_encode_len(0x80, 0xb7, bytes.len());
        out.extend_from_slice(bytes);
        out
    }

    fn test_rlp_encode_list(items: &[Vec<u8>]) -> Vec<u8> {
        let payload_len = items.iter().map(Vec::len).sum();
        let mut out = test_rlp_encode_len(0xc0, 0xf7, payload_len);
        for item in items {
            out.extend_from_slice(item);
        }
        out
    }

    fn test_keccak32(bytes: &[u8]) -> [u8; 32] {
        let mut hasher = Keccak256::new();
        hasher.update(bytes);
        hasher.finalize().into()
    }

    fn mapped_lock_receipt_proof_fields(
        contract_address: &[u8; 20],
        topic0: &[u8; 32],
        receipt_index: u64,
    ) -> (Vec<u8>, [u8; 32], Vec<Vec<u8>>) {
        let log = test_rlp_encode_list(&[
            test_rlp_encode_bytes(contract_address),
            test_rlp_encode_list(&[test_rlp_encode_bytes(topic0)]),
            test_rlp_encode_bytes(&[]),
        ]);
        let receipt = test_rlp_encode_list(&[
            test_rlp_encode_bytes(&[1]),
            ua_rlp_encode_u64_v1(21_000),
            test_rlp_encode_bytes(&[0u8; 256]),
            test_rlp_encode_list(&[log]),
        ]);
        let proof_node = novovm_network::eth_rlpx_mpt_single_leaf_node_rlp_v1(
            ua_rlp_encode_u64_v1(receipt_index).as_slice(),
            receipt.as_slice(),
        );
        (
            receipt,
            test_keccak32(proof_node.as_slice()),
            vec![proof_node],
        )
    }

    fn mapped_lock_live_event_proof_params(account_id: &str, lock_byte: u8, amount: u128) -> Value {
        let contract_address = [0x11u8; 20];
        let topic0 = eth_lock_event_topic0_v1();
        let chain_id = 100_000u64 + u64::from(lock_byte);
        let block_hash = [lock_byte.saturating_add(3); 32];
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
            chain_id,
            contract_address,
            topic0,
            block_number: 100,
            block_hash,
            finalized_block_number: 112,
            log_index: u64::from(lock_byte),
            receipts_root: [0u8; 32],
            receipt_index: 0,
            receipt_log_index: 0,
            receipt_proof: Vec::new(),
            receipt_envelope: None,
        };
        let (receipt, receipts_root, receipt_proof) =
            mapped_lock_receipt_proof_fields(&contract_address, &topic0, evidence.receipt_index);
        let evidence = EthereumLockEventEvidenceV1 {
            receipts_root,
            receipt_proof: receipt_proof.clone(),
            receipt_envelope: Some(receipt.clone()),
            ..evidence
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
            "source_chain_id": chain_id,
            "lock_contract_address": format!("0x{}", to_hex_lower(&contract_address)),
            "expected_lock_contract_address": format!("0x{}", to_hex_lower(&contract_address)),
            "event_topic0": mapped_asset_hex_id(&topic0),
            "block_number": evidence.block_number,
            "block_hash": mapped_asset_hex_id(&block_hash),
            "finalized_block_number": evidence.finalized_block_number,
            "log_index": evidence.log_index,
            "receipt_index": evidence.receipt_index,
            "receipt_log_index": evidence.receipt_log_index,
            "receipt_envelope": format!("0x{}", to_hex_lower(&receipt)),
            "receipts_root": mapped_asset_hex_id(&receipts_root),
            "receipt_proof": receipt_proof
                .iter()
                .map(|node| Value::String(format!("0x{}", to_hex_lower(node))))
                .collect::<Vec<_>>(),
        })
    }

    fn mapped_lock_param_u64(params: &Value, key: &str) -> u64 {
        params
            .get(key)
            .and_then(value_as_u64)
            .unwrap_or_else(|| panic!("missing mapped lock param {key}"))
    }

    fn mapped_lock_param_hash(params: &Value, key: &str) -> [u8; 32] {
        let raw = params
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("missing mapped lock hash param {key}"));
        decode_hex_fixed_32(raw, key).expect("decode mapped lock hash param")
    }

    fn seed_mapped_lock_trusted_block(params: &Value, finalized: bool, receipts_root: [u8; 32]) {
        let chain_id = mapped_lock_param_u64(params, "source_chain_id");
        let number = mapped_lock_param_u64(params, "block_number");
        let hash = mapped_lock_param_hash(params, "block_hash");
        novovm_network::clear_network_runtime_native_state_for_host_tests_v1();
        novovm_network::set_network_runtime_native_header_snapshot_v1(
            chain_id,
            novovm_network::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number,
                hash,
                parent_hash: [0x09; 32],
                state_root: [0x21; 32],
                transactions_root: [0x31; 32],
                receipts_root,
                ommers_hash: [0x51; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: None,
                gas_used: None,
                timestamp: Some(1_700_000_000),
                base_fee_per_gas: None,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(1),
                observed_unix_ms: 1000,
            },
        );
        novovm_network::set_network_runtime_native_head_snapshot_v1(
            chain_id,
            novovm_network::NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: novovm_network::NetworkRuntimeNativeSyncPhaseV1::Finalize,
                peer_count: 1,
                block_number: number,
                block_hash: hash,
                parent_block_hash: [0x09; 32],
                state_root: [0x21; 32],
                canonical: true,
                safe: finalized,
                finalized,
                reorg_depth_hint: None,
                body_available: true,
                source_peer_id: Some(1),
                observed_unix_ms: 1001,
            },
        );
    }

    fn seed_mapped_lock_trusted_block_from_params(params: &Value, finalized: bool) {
        let receipts_root = mapped_lock_param_hash(params, "receipts_root");
        seed_mapped_lock_trusted_block(params, finalized, receipts_root);
    }

    fn observe_mapped_lock_trusted_header_source_peer_from_params(params: &Value, peer_id: u64) {
        let chain_id = mapped_lock_param_u64(params, "source_chain_id");
        let number = mapped_lock_param_u64(params, "block_number");
        let hash = mapped_lock_param_hash(params, "block_hash");
        let receipts_root = mapped_lock_param_hash(params, "receipts_root");
        novovm_network::set_network_runtime_native_header_snapshot_v1(
            chain_id,
            novovm_network::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number,
                hash,
                parent_hash: [0x09; 32],
                state_root: [0x21; 32],
                transactions_root: [0x31; 32],
                receipts_root,
                ommers_hash: [0x51; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: None,
                gas_used: None,
                timestamp: Some(1_700_000_000),
                base_fee_per_gas: None,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                block_access_list_hash: None,
                source_peer_id: Some(peer_id),
                observed_unix_ms: u128::from(2000u64.saturating_add(peer_id)),
            },
        );
    }

    fn reorg_mapped_lock_trusted_block_from_params(params: &Value) {
        let chain_id = mapped_lock_param_u64(params, "source_chain_id");
        let number = mapped_lock_param_u64(params, "block_number");
        let original_hash = mapped_lock_param_hash(params, "block_hash");
        let replacement_hash = original_hash.map(|byte| byte ^ 0xff);
        novovm_network::set_network_runtime_native_head_snapshot_v1(
            chain_id,
            novovm_network::NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: novovm_network::NetworkRuntimeNativeSyncPhaseV1::Finalize,
                peer_count: 1,
                block_number: number,
                block_hash: replacement_hash,
                parent_block_hash: [0xee; 32],
                state_root: [0x22; 32],
                canonical: true,
                safe: true,
                finalized: true,
                reorg_depth_hint: Some(1),
                body_available: true,
                source_peer_id: Some(2),
                observed_unix_ms: 2000,
            },
        );
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
        seed_mapped_lock_trusted_block_from_params(&Value::Object(register_map.clone()), true);
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
        seed_mapped_lock_trusted_block_from_params(&Value::Object(register_map.clone()), true);
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
    fn unified_account_live_mapped_lock_rejects_invalid_receipt_proof() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-receipt-proof-invalid");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live-receipt-invalid", 10);

        let mut bad_envelope_map = match mapped_lock_live_event_proof_params(
            "acct-map-live-receipt-invalid",
            0x37,
            92u128,
        ) {
            Value::Object(map) => map,
            other => panic!("expected mapped lock proof params object, got {other:?}"),
        };
        seed_mapped_lock_trusted_block_from_params(&Value::Object(bad_envelope_map.clone()), true);
        bad_envelope_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        bad_envelope_map.insert(
            "receipt_envelope".to_string(),
            Value::String("0x01".to_string()),
        );
        let bad_envelope_err = run_query_err(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(bad_envelope_map),
            ),
        );
        assert!(
            bad_envelope_err.contains("receipt_envelope does not match proof value"),
            "receipt envelope mismatch should fail, got: {bad_envelope_err}"
        );

        let mut wrong_root_map = match mapped_lock_live_event_proof_params(
            "acct-map-live-receipt-invalid",
            0x38,
            93u128,
        ) {
            Value::Object(map) => map,
            other => panic!("expected mapped lock proof params object, got {other:?}"),
        };
        seed_mapped_lock_trusted_block_from_params(&Value::Object(wrong_root_map.clone()), true);
        wrong_root_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        wrong_root_map.insert("receipts_root".to_string(), Value::String(ua_hex(0x44, 32)));
        let wrong_root_err = run_query_err(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(wrong_root_map),
            ),
        );
        assert!(
            wrong_root_err.contains("receipts_root does not match trusted Ethereum header"),
            "wrong receipts root should fail trusted header anchor, got: {wrong_root_err}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_live_mapped_lock_requires_trusted_finalized_header_anchor() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-header-anchor");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live-anchor", 10);

        let mut missing_header_map =
            match mapped_lock_live_event_proof_params("acct-map-live-anchor", 0x39, 94u128) {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        novovm_network::clear_network_runtime_native_state_for_host_tests_v1();
        missing_header_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        let missing_header_err = run_query_err(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(missing_header_map),
            ),
        );
        assert!(
            missing_header_err.contains("trusted Ethereum canonical block is unavailable"),
            "missing trusted header should fail closed, got: {missing_header_err}"
        );

        let mut unfinalized_anchor_map =
            match mapped_lock_live_event_proof_params("acct-map-live-anchor", 0x3a, 95u128) {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        seed_mapped_lock_trusted_block_from_params(
            &Value::Object(unfinalized_anchor_map.clone()),
            false,
        );
        unfinalized_anchor_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        let unfinalized_anchor_err = run_query_err(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(unfinalized_anchor_map),
            ),
        );
        assert!(
            unfinalized_anchor_err.contains("trusted Ethereum block is not finalized"),
            "unfinalized trusted block should fail closed, got: {unfinalized_anchor_err}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_live_mapped_lock_enforces_governed_header_source_policy() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-header-source-policy");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live-source-policy", 10);

        let mut rejected_map =
            match mapped_lock_live_event_proof_params("acct-map-live-source-policy", 0x42, 112u128)
            {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        seed_mapped_lock_trusted_block_from_params(&Value::Object(rejected_map.clone()), true);
        let policy = run_query(
            &base,
            "ua_setMappedHeaderSourcePolicy",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "required": true,
                    "allowed_peer_ids": [2u64],
                    "policy_source": "governance_test",
                    "policy_version": 7u64,
                    "now": 12u64,
                }),
            ),
        );
        assert_eq!(policy["updated"].as_bool(), Some(true));
        assert_eq!(policy["policy"]["required"].as_bool(), Some(true));
        rejected_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        let rejected_err = run_query_err(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(rejected_map),
            ),
        );
        assert!(
            rejected_err.contains("ERR_MAPPED_HEADER_SOURCE_UNTRUSTED"),
            "non-whitelisted source peer should fail closed, got: {rejected_err}"
        );

        let mut accepted_map =
            match mapped_lock_live_event_proof_params("acct-map-live-source-policy", 0x43, 113u128)
            {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        seed_mapped_lock_trusted_block_from_params(&Value::Object(accepted_map.clone()), true);
        let updated_policy = run_query(
            &base,
            "ua_setMappedHeaderSourcePolicy",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "required": true,
                    "allowed_peer_ids": [1u64, 2u64],
                    "min_source_quorum": 2u64,
                    "policy_source": "governance_test",
                    "policy_version": 8u64,
                    "now": 13u64,
                }),
            ),
        );
        assert_eq!(
            updated_policy["policy"]["allowed_peer_ids"][0].as_u64(),
            Some(1)
        );
        assert_eq!(
            updated_policy["policy"]["min_source_quorum"].as_u64(),
            Some(2)
        );
        accepted_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        let quorum_err = run_query_err(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(accepted_map.clone()),
            ),
        );
        assert!(
            quorum_err.contains("ERR_MAPPED_HEADER_SOURCE_QUORUM_UNMET"),
            "single observed source should not satisfy quorum=2, got: {quorum_err}"
        );

        observe_mapped_lock_trusted_header_source_peer_from_params(
            &Value::Object(accepted_map.clone()),
            2,
        );
        let accepted = run_query(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(accepted_map),
            ),
        );
        assert_eq!(accepted["accepted"].as_bool(), Some(true));
        assert_eq!(
            accepted["settlement_effect"].as_str(),
            Some("neth_m2_credit")
        );

        let get_policy = run_query(
            &base,
            "ua_getMappedHeaderSourcePolicy",
            params_with_paths_and_native_store(&store, &audit, &native_store, json!({})),
        );
        assert_eq!(get_policy["policy"]["policy_version"].as_u64(), Some(8));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_live_mapped_lock_enforces_governed_header_attestation_policy() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-header-attestation-policy");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live-attestation", 10);
        let signer_a = Ed25519SigningKey::from_bytes(&[0xa1u8; 32]);
        let signer_b = Ed25519SigningKey::from_bytes(&[0xb2u8; 32]);
        let signer_c = Ed25519SigningKey::from_bytes(&[0xc3u8; 32]);
        let signer_a_pub = signer_a.verifying_key().to_bytes();
        let signer_b_pub = signer_b.verifying_key().to_bytes();
        let signer_c_pub = signer_c.verifying_key().to_bytes();
        let signer_a_ref = to_hex_lower(&signer_a_pub);
        let signer_b_ref = to_hex_lower(&signer_b_pub);
        let signer_c_ref = to_hex_lower(&signer_c_pub);
        let mut disabled_reasons = serde_json::Map::new();
        disabled_reasons.insert(
            signer_b_ref.clone(),
            Value::String("key_rotation".to_string()),
        );
        let mut signer_rotations = serde_json::Map::new();
        signer_rotations.insert(
            signer_b_ref.clone(),
            Value::String(format!("0x{}", signer_c_ref)),
        );

        let policy = run_query(
            &base,
            "ua_setMappedHeaderAttestationPolicy",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "required": true,
                    "allowed_signers": [signer_a_ref, signer_c_ref],
                    "disabled_signers": [signer_b_ref],
                    "disabled_signer_reasons": Value::Object(disabled_reasons),
                    "signer_rotations": Value::Object(signer_rotations),
                    "min_attestation_quorum": 2u64,
                    "policy_source": "governance_test",
                    "policy_version": 11u64,
                    "now": 12u64,
                }),
            ),
        );
        assert_eq!(policy["updated"].as_bool(), Some(true));
        assert_eq!(policy["policy"]["required"].as_bool(), Some(true));
        assert_eq!(policy["policy"]["min_attestation_quorum"].as_u64(), Some(2));
        assert_eq!(
            policy["policy"]["active_allowed_signers"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(
            policy["policy"]["disabled_signer_reasons"][signer_b_ref.as_str()].as_str(),
            Some("key_rotation")
        );
        assert_eq!(
            policy["policy"]["signer_rotations"][signer_b_ref.as_str()].as_str(),
            Some(signer_c_ref.as_str())
        );

        let mut blocked_map =
            match mapped_lock_live_event_proof_params("acct-map-live-attestation", 0x47, 124u128) {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        seed_mapped_lock_trusted_block_from_params(&Value::Object(blocked_map.clone()), true);
        let blocked_message = mapped_header_attestation_message_v1(
            mapped_lock_param_u64(&Value::Object(blocked_map.clone()), "source_chain_id"),
            mapped_lock_param_u64(&Value::Object(blocked_map.clone()), "block_number"),
            mapped_lock_param_hash(&Value::Object(blocked_map.clone()), "block_hash"),
            mapped_lock_param_hash(&Value::Object(blocked_map.clone()), "receipts_root"),
        );
        let blocked_sig_a = signer_a.sign(blocked_message.as_slice()).to_bytes();
        let blocked_sig_b = signer_b.sign(blocked_message.as_slice()).to_bytes();
        blocked_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        blocked_map.insert(
            "header_attestations".to_string(),
            json!([
                {
                    "signer": format!("0x{}", signer_a_ref),
                    "signature": format!("0x{}", to_hex_lower(&blocked_sig_a)),
                },
                {
                    "signer": format!("0x{}", signer_b_ref),
                    "signature": format!("0x{}", to_hex_lower(&blocked_sig_b)),
                },
            ]),
        );
        let blocked_err = run_query_err(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(blocked_map),
            ),
        );
        assert!(
            blocked_err.contains("ERR_MAPPED_HEADER_ATTESTATION_QUORUM_UNMET"),
            "disabled signer should not satisfy quorum, got: {blocked_err}"
        );

        let mut accepted_map =
            match mapped_lock_live_event_proof_params("acct-map-live-attestation", 0x48, 125u128) {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        seed_mapped_lock_trusted_block_from_params(&Value::Object(accepted_map.clone()), true);
        let accepted_message = mapped_header_attestation_message_v1(
            mapped_lock_param_u64(&Value::Object(accepted_map.clone()), "source_chain_id"),
            mapped_lock_param_u64(&Value::Object(accepted_map.clone()), "block_number"),
            mapped_lock_param_hash(&Value::Object(accepted_map.clone()), "block_hash"),
            mapped_lock_param_hash(&Value::Object(accepted_map.clone()), "receipts_root"),
        );
        let accepted_sig_a = signer_a.sign(accepted_message.as_slice()).to_bytes();
        let accepted_sig_c = signer_c.sign(accepted_message.as_slice()).to_bytes();
        accepted_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        accepted_map.insert(
            "header_attestations".to_string(),
            json!([
                {
                    "signer": format!("0x{}", signer_a_ref),
                    "signature": format!("0x{}", to_hex_lower(&accepted_sig_a)),
                },
                {
                    "signer": format!("0x{}", signer_c_ref),
                    "signature": format!("0x{}", to_hex_lower(&accepted_sig_c)),
                },
            ]),
        );
        let accepted = run_query(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(accepted_map),
            ),
        );
        assert_eq!(accepted["accepted"].as_bool(), Some(true));
        assert_eq!(
            accepted["settlement_effect"].as_str(),
            Some("neth_m2_credit")
        );

        let get_policy = run_query(
            &base,
            "ua_getMappedHeaderAttestationPolicy",
            params_with_paths_and_native_store(&store, &audit, &native_store, json!({})),
        );
        assert_eq!(get_policy["policy"]["policy_version"].as_u64(), Some(11));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_live_mapped_lock_uses_governed_min_confirmations() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-min-confirmations-policy");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live-min-conf", 10);

        let mut policy_store = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store before min confirmations policy");
        policy_store.module_state.mapped_lock_min_confirmations = 18;
        policy_store.module_state.treasury_policy_source = "governance_test".to_string();
        policy_store.module_state.treasury_policy_version = 18;
        save_nov_native_execution_store_v1(native_store.as_path(), &policy_store)
            .expect("save min confirmations policy");

        let mut blocked_map =
            match mapped_lock_live_event_proof_params("acct-map-live-min-conf", 0x45, 122u128) {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        seed_mapped_lock_trusted_block_from_params(&Value::Object(blocked_map.clone()), true);
        blocked_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        let blocked_err = run_query_err(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(blocked_map.clone()),
            ),
        );
        assert!(
            blocked_err.contains("finalized_block_number 112 is below required 118")
                && blocked_err.contains("source=governance_native_store"),
            "governed min confirmations should fail closed, got: {blocked_err}"
        );

        let mut relaxed_policy_store = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store before relaxing min confirmations policy");
        relaxed_policy_store
            .module_state
            .mapped_lock_min_confirmations = 12;
        relaxed_policy_store.module_state.treasury_policy_version = 19;
        save_nov_native_execution_store_v1(native_store.as_path(), &relaxed_policy_store)
            .expect("save relaxed min confirmations policy");

        let accepted = run_query(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(blocked_map),
            ),
        );
        assert_eq!(accepted["accepted"].as_bool(), Some(true));
        assert_eq!(
            accepted["settlement_effect"].as_str(),
            Some("neth_m2_credit")
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_live_mapped_lock_bridge_pause_blocks_register_without_state() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-bridge-register-paused");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live-bridge-paused", 10);

        let mut register_map = match mapped_lock_live_event_proof_params(
            "acct-map-live-bridge-paused",
            0x3b,
            96u128,
        ) {
            Value::Object(map) => map,
            other => panic!("expected mapped lock proof params object, got {other:?}"),
        };
        seed_mapped_lock_trusted_block_from_params(&Value::Object(register_map.clone()), true);
        register_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        register_map.insert("now".to_string(), Value::from(11u64));

        let mut paused_store = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store before bridge pause");
        paused_store.module_state.mapped_lock_bridge_paused = true;
        save_nov_native_execution_store_v1(native_store.as_path(), &paused_store)
            .expect("save paused native store");
        let paused_err = run_query_err(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                Value::Object(register_map.clone()),
            ),
        );
        assert!(
            paused_err.contains("ERR_MAPPED_BRIDGE_PAUSED"),
            "paused register should fail closed, got: {paused_err}"
        );

        let mut unpaused_store = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store before bridge unpause");
        unpaused_store.module_state.mapped_lock_bridge_paused = false;
        save_nov_native_execution_store_v1(native_store.as_path(), &unpaused_store)
            .expect("save unpaused native store");
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
        assert_eq!(
            register["native_settlement"]["effect"].as_str(),
            Some("neth_m2_credit")
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_live_mapped_asset_bridge_pause_blocks_burn_and_release() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-bridge-burn-release-paused");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live-bridge-burn", 10);

        let mut register_map =
            match mapped_lock_live_event_proof_params("acct-map-live-bridge-burn", 0x3c, 97u128) {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        seed_mapped_lock_trusted_block_from_params(&Value::Object(register_map.clone()), true);
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
        let mapping_id = register["mapping_id"].clone();

        let mut paused_burn_store = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store before burn pause");
        paused_burn_store.module_state.mapped_asset_burn_paused = true;
        save_nov_native_execution_store_v1(native_store.as_path(), &paused_burn_store)
            .expect("save burn paused native store");
        let burn_err = run_query_err(
            &base,
            "ua_burnMappedAsset",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-bridge-burn",
                    "mapping_id": mapping_id,
                    "now": 12u64,
                }),
            ),
        );
        assert!(
            burn_err.contains("ERR_MAPPED_BURN_PAUSED"),
            "paused burn should fail closed, got: {burn_err}"
        );
        let after_burn_pause = run_query(
            &base,
            "ua_getMappedAsset",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "acct-map-live-bridge-burn",
                    "mapping_id": register["mapping_id"],
                }),
            ),
        );
        assert_eq!(
            after_burn_pause["mapped_asset"]["status"].as_str(),
            Some("active")
        );

        let mut unpaused_burn_store = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store before burn unpause");
        unpaused_burn_store.module_state.mapped_asset_burn_paused = false;
        save_nov_native_execution_store_v1(native_store.as_path(), &unpaused_burn_store)
            .expect("save burn unpaused native store");
        let burn = run_query(
            &base,
            "ua_burnMappedAsset",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-bridge-burn",
                    "mapping_id": register["mapping_id"],
                    "now": 13u64,
                }),
            ),
        );
        assert_eq!(burn["burned"].as_bool(), Some(true));

        let mut paused_release_store = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store before release pause");
        paused_release_store
            .module_state
            .mapped_asset_release_paused = true;
        save_nov_native_execution_store_v1(native_store.as_path(), &paused_release_store)
            .expect("save release paused native store");
        let release_err = run_query_err(
            &base,
            "ua_releaseMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-bridge-burn",
                    "mapping_id": register["mapping_id"],
                    "now": 14u64,
                }),
            ),
        );
        assert!(
            release_err.contains("ERR_MAPPED_RELEASE_PAUSED"),
            "paused release should fail closed, got: {release_err}"
        );
        let after_release_pause = run_query(
            &base,
            "ua_getMappedAsset",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "acct-map-live-bridge-burn",
                    "mapping_id": register["mapping_id"],
                }),
            ),
        );
        assert_eq!(
            after_release_pause["mapped_asset"]["status"].as_str(),
            Some("burn_pending")
        );

        let mut unpaused_release_store = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store before release unpause");
        unpaused_release_store
            .module_state
            .mapped_asset_release_paused = false;
        save_nov_native_execution_store_v1(native_store.as_path(), &unpaused_release_store)
            .expect("save release unpaused native store");
        let release = run_query(
            &base,
            "ua_releaseMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-bridge-burn",
                    "mapping_id": register["mapping_id"],
                    "now": 15u64,
                }),
            ),
        );
        assert_eq!(release["released"].as_bool(), Some(true));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_live_mapped_asset_reorg_blocks_burn_without_state_advance() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-reorg-blocked");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live-reorg", 10);

        let mut register_map =
            match mapped_lock_live_event_proof_params("acct-map-live-reorg", 0x3d, 98u128) {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        seed_mapped_lock_trusted_block_from_params(&Value::Object(register_map.clone()), true);
        register_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        register_map.insert("now".to_string(), Value::from(11u64));
        let register_params = Value::Object(register_map.clone());
        let register = run_query(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                register_params.clone(),
            ),
        );
        assert_eq!(register["accepted"].as_bool(), Some(true));

        reorg_mapped_lock_trusted_block_from_params(&register_params);
        let after_reorg = run_query(
            &base,
            "ua_getMappedAsset",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "acct-map-live-reorg",
                    "mapping_id": register["mapping_id"],
                }),
            ),
        );
        assert_eq!(
            after_reorg["source_anchor_status"]["state"].as_str(),
            Some("blocked")
        );
        assert_eq!(
            after_reorg["mapped_asset"]["status"].as_str(),
            Some("active")
        );

        let burn_err = run_query_err(
            &base,
            "ua_burnMappedAsset",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-reorg",
                    "mapping_id": register["mapping_id"],
                    "now": 12u64,
                }),
            ),
        );
        assert!(
            burn_err.contains("ERR_MAPPED_ASSET_SOURCE_ANCHOR_UNSAFE"),
            "reorged source anchor should block burn, got: {burn_err}"
        );
        let after_failed_burn = run_query(
            &base,
            "ua_getMappedAsset",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "acct-map-live-reorg",
                    "mapping_id": register["mapping_id"],
                }),
            ),
        );
        assert_eq!(
            after_failed_burn["mapped_asset"]["status"].as_str(),
            Some("active")
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_auto_heal_freezes_unsafe_live_mapped_asset() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-auto-heal");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live-auto-heal", 10);

        let mut register_map =
            match mapped_lock_live_event_proof_params("acct-map-live-auto-heal", 0x44, 121u128) {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        seed_mapped_lock_trusted_block_from_params(&Value::Object(register_map.clone()), true);
        register_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        register_map.insert("now".to_string(), Value::from(11u64));
        let register_params = Value::Object(register_map.clone());
        let register = run_query(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                register_params.clone(),
            ),
        );
        assert_eq!(register["accepted"].as_bool(), Some(true));

        reorg_mapped_lock_trusted_block_from_params(&register_params);
        let dry_run = run_query(
            &base,
            "ua_autoHealMappedAssets",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-auto-heal",
                    "apply": false,
                    "now": 12u64,
                }),
            ),
        );
        assert_eq!(dry_run["dry_run"].as_bool(), Some(true));
        assert_eq!(dry_run["applied_count"].as_u64(), Some(0));
        assert_eq!(
            dry_run["items"][0]["action"].as_str(),
            Some("freeze_unsafe_anchor")
        );
        assert_eq!(dry_run["items"][0]["applied"].as_bool(), Some(false));

        let still_active = run_query(
            &base,
            "ua_getMappedAsset",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "acct-map-live-auto-heal",
                    "mapping_id": register["mapping_id"],
                }),
            ),
        );
        assert_eq!(
            still_active["mapped_asset"]["status"].as_str(),
            Some("active")
        );

        let disabled_apply_err = run_query_err(
            &base,
            "ua_autoHealMappedAssets",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-auto-heal",
                    "apply": true,
                    "now": 13u64,
                }),
            ),
        );
        assert!(
            disabled_apply_err.contains("ERR_MAPPED_AUTO_HEAL_DISABLED"),
            "auto heal apply must be governance-enabled, got: {disabled_apply_err}"
        );

        let mut policy_store = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store before enabling auto heal");
        policy_store.module_state.mapped_asset_auto_heal_enabled = true;
        policy_store.module_state.treasury_policy_source = "governance_test".to_string();
        policy_store.module_state.treasury_policy_version = 2;
        save_nov_native_execution_store_v1(native_store.as_path(), &policy_store)
            .expect("save auto heal enabled native store");

        let applied = run_query(
            &base,
            "ua_autoHealMappedAssets",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-auto-heal",
                    "apply": true,
                    "reason": "auto heal unsafe source anchor",
                    "now": 13u64,
                }),
            ),
        );
        assert_eq!(applied["dry_run"].as_bool(), Some(false));
        assert_eq!(applied["applied_count"].as_u64(), Some(1));
        assert_eq!(
            applied["policy"]["mapped_asset_auto_heal_enabled"].as_bool(),
            Some(true)
        );
        assert_eq!(applied["items"][0]["applied"].as_bool(), Some(true));
        assert_eq!(applied["items"][0]["status_after"].as_str(), Some("frozen"));
        assert_eq!(
            applied["items"][0]["native_settlement"]["effect"].as_str(),
            Some("neth_m2_frozen")
        );

        let frozen = run_query(
            &base,
            "ua_getMappedAsset",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "acct-map-live-auto-heal",
                    "mapping_id": register["mapping_id"],
                }),
            ),
        );
        assert_eq!(frozen["mapped_asset"]["status"].as_str(), Some("frozen"));

        let assets = run_query(
            &base,
            "account_assets",
            params_with_paths(
                &store,
                &audit,
                json!({"account_id": "acct-map-live-auto-heal"}),
            ),
        );
        assert_eq!(
            assets["mapped_asset_active_balance"].as_u64().unwrap_or(0),
            0
        );
        let native_after = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store after auto heal");
        assert_eq!(
            native_after
                .module_state
                .account_asset_balances
                .get("acct-map-live-auto-heal")
                .and_then(|assets| assets.get("NETH").copied())
                .unwrap_or(0),
            0
        );
        assert_eq!(
            native_after
                .module_state
                .treasury_reserves
                .get("NETH")
                .copied(),
            Some(121)
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_live_mapped_asset_freeze_removes_liquid_neth_without_reserve_release() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-freeze");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live-freeze", 10);

        let mut register_map =
            match mapped_lock_live_event_proof_params("acct-map-live-freeze", 0x3f, 100u128) {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        seed_mapped_lock_trusted_block_from_params(&Value::Object(register_map.clone()), true);
        register_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        register_map.insert("now".to_string(), Value::from(11u64));
        let register_params = Value::Object(register_map.clone());
        let register = run_query(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                register_params.clone(),
            ),
        );
        assert_eq!(register["accepted"].as_bool(), Some(true));

        reorg_mapped_lock_trusted_block_from_params(&register_params);
        let freeze = run_query(
            &base,
            "ua_freezeMappedAsset",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-freeze",
                    "mapping_id": register["mapping_id"],
                    "reason": "source anchor reorg unsafe",
                    "now": 12u64,
                }),
            ),
        );
        assert_eq!(freeze["frozen"].as_bool(), Some(true));
        assert_eq!(freeze["status"].as_str(), Some("frozen"));
        assert_eq!(
            freeze["native_settlement"]["effect"].as_str(),
            Some("neth_m2_frozen")
        );
        assert_eq!(
            freeze["native_settlement"]["account_balance_after"].as_u64(),
            Some(0)
        );
        assert_eq!(
            freeze["native_settlement"]["treasury_reserve_unchanged"].as_bool(),
            Some(true)
        );

        let frozen_asset = run_query(
            &base,
            "ua_getMappedAsset",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "acct-map-live-freeze",
                    "mapping_id": register["mapping_id"],
                }),
            ),
        );
        assert_eq!(
            frozen_asset["mapped_asset"]["status"].as_str(),
            Some("frozen")
        );
        let balance = run_query(
            &base,
            "account_balance",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({"account_id": "acct-map-live-freeze", "asset_id": "NETH"}),
            ),
        );
        assert_eq!(balance["balance"].as_u64(), Some(0));
        assert_eq!(balance["mapped_asset_active_balance"].as_u64(), Some(0));
        let native_after_freeze = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store after freeze");
        assert_eq!(
            native_after_freeze
                .module_state
                .treasury_reserves
                .get("NETH")
                .copied(),
            Some(100)
        );
        assert_eq!(
            native_after_freeze
                .module_state
                .treasury_settlement_journal
                .last()
                .map(|entry| entry.kind.as_str()),
            Some("mapped_asset_m2_frozen")
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_live_mapped_asset_unfreeze_requires_safe_anchor_and_restores_neth() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-unfreeze");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live-unfreeze", 10);

        let mut register_map =
            match mapped_lock_live_event_proof_params("acct-map-live-unfreeze", 0x40, 101u128) {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        seed_mapped_lock_trusted_block_from_params(&Value::Object(register_map.clone()), true);
        register_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        register_map.insert("now".to_string(), Value::from(11u64));
        let register_params = Value::Object(register_map.clone());
        let register = run_query(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                register_params.clone(),
            ),
        );
        reorg_mapped_lock_trusted_block_from_params(&register_params);
        let freeze = run_query(
            &base,
            "ua_freezeMappedAsset",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-unfreeze",
                    "mapping_id": register["mapping_id"],
                    "reason": "source anchor unsafe",
                    "now": 12u64,
                }),
            ),
        );
        assert_eq!(freeze["status"].as_str(), Some("frozen"));

        let unsafe_unfreeze_err = run_query_err(
            &base,
            "ua_unfreezeMappedAsset",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-unfreeze",
                    "mapping_id": register["mapping_id"],
                    "reason": "unsafe recovery attempt",
                    "now": 13u64,
                }),
            ),
        );
        assert!(
            unsafe_unfreeze_err.contains("ERR_MAPPED_ASSET_SOURCE_ANCHOR_UNSAFE"),
            "unsafe anchor should block unfreeze, got: {unsafe_unfreeze_err}"
        );

        seed_mapped_lock_trusted_block_from_params(&register_params, true);
        let unfreeze = run_query(
            &base,
            "ua_unfreezeMappedAsset",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-unfreeze",
                    "mapping_id": register["mapping_id"],
                    "reason": "source anchor restored",
                    "now": 14u64,
                }),
            ),
        );
        assert_eq!(unfreeze["unfrozen"].as_bool(), Some(true));
        assert_eq!(unfreeze["status"].as_str(), Some("active"));
        assert_eq!(
            unfreeze["native_settlement"]["effect"].as_str(),
            Some("neth_m2_unfrozen")
        );
        assert_eq!(
            unfreeze["native_settlement"]["account_balance_after"].as_u64(),
            Some(101)
        );
        assert_eq!(
            unfreeze["source_anchor_status"]["state"].as_str(),
            Some("ok")
        );

        let balance = run_query(
            &base,
            "account_balance",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({"account_id": "acct-map-live-unfreeze", "asset_id": "NETH"}),
            ),
        );
        assert_eq!(balance["balance"].as_u64(), Some(101));
        assert_eq!(balance["mapped_asset_active_balance"].as_u64(), Some(101));
        let native_after_unfreeze = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store after unfreeze");
        assert_eq!(
            native_after_unfreeze
                .module_state
                .treasury_reserves
                .get("NETH")
                .copied(),
            Some(101)
        );
        assert_eq!(
            native_after_unfreeze
                .module_state
                .treasury_settlement_journal
                .last()
                .map(|entry| entry.kind.as_str()),
            Some("mapped_asset_m2_unfrozen")
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_live_mapped_asset_rollback_requires_unsafe_anchor_and_clears_reserve() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-rollback");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live-rollback", 10);

        let mut register_map =
            match mapped_lock_live_event_proof_params("acct-map-live-rollback", 0x41, 111u128) {
                Value::Object(map) => map,
                other => panic!("expected mapped lock proof params object, got {other:?}"),
            };
        seed_mapped_lock_trusted_block_from_params(&Value::Object(register_map.clone()), true);
        register_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        register_map.insert("now".to_string(), Value::from(11u64));
        let register_params = Value::Object(register_map.clone());
        let register = run_query(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                register_params.clone(),
            ),
        );
        let freeze = run_query(
            &base,
            "ua_freezeMappedAsset",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-rollback",
                    "mapping_id": register["mapping_id"],
                    "reason": "manual risk hold",
                    "now": 12u64,
                }),
            ),
        );
        assert_eq!(freeze["status"].as_str(), Some("frozen"));

        let safe_rollback_err = run_query_err(
            &base,
            "ua_rollbackFrozenMappedAsset",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-rollback",
                    "mapping_id": register["mapping_id"],
                    "reason": "safe anchor rollback must fail",
                    "now": 13u64,
                }),
            ),
        );
        assert!(
            safe_rollback_err.contains("ERR_MAPPED_ROLLBACK_ANCHOR_STILL_SAFE"),
            "safe source anchor should block rollback, got: {safe_rollback_err}"
        );

        reorg_mapped_lock_trusted_block_from_params(&register_params);
        let rollback = run_query(
            &base,
            "ua_rollbackFrozenMappedAsset",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-rollback",
                    "mapping_id": register["mapping_id"],
                    "reason": "source anchor reorg rollback",
                    "now": 14u64,
                }),
            ),
        );
        assert_eq!(rollback["rolled_back"].as_bool(), Some(true));
        assert_eq!(rollback["status"].as_str(), Some("rejected"));
        assert_eq!(
            rollback["native_settlement"]["effect"].as_str(),
            Some("neth_m2_rolled_back")
        );
        assert_eq!(
            rollback["native_settlement"]["treasury_reserve_after"].as_u64(),
            Some(0)
        );
        assert_eq!(
            rollback["native_settlement"]["nov_minted"].as_u64(),
            Some(0)
        );
        assert_eq!(
            rollback["native_settlement"]["external_release_triggered"].as_bool(),
            Some(false)
        );
        assert_eq!(
            rollback["source_anchor_status"]["state"].as_str(),
            Some("blocked")
        );

        let rejected_asset = run_query(
            &base,
            "ua_getMappedAsset",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "acct-map-live-rollback",
                    "mapping_id": register["mapping_id"],
                }),
            ),
        );
        assert_eq!(
            rejected_asset["mapped_asset"]["status"].as_str(),
            Some("rejected")
        );
        let assets = run_query(
            &base,
            "account_assets",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({"account_id": "acct-map-live-rollback"}),
            ),
        );
        assert_eq!(assets["mapped_asset_count"].as_u64(), Some(0));
        let native_after_rollback = load_nov_native_execution_store_v1(native_store.as_path())
            .expect("load native store after rollback");
        assert_eq!(
            native_after_rollback
                .module_state
                .account_asset_balances
                .get("acct-map-live-rollback")
                .and_then(|assets| assets.get("NETH"))
                .copied(),
            Some(0)
        );
        assert_eq!(
            native_after_rollback
                .module_state
                .treasury_reserves
                .get("NETH")
                .copied(),
            Some(0)
        );
        assert_eq!(
            native_after_rollback
                .module_state
                .treasury_settlement_journal
                .last()
                .map(|entry| entry.kind.as_str()),
            Some("mapped_asset_m2_rolled_back")
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_account_live_mapped_asset_reorg_blocks_release_without_state_advance() {
        let _env_lock = ENV_TEST_LOCK.lock().expect("env test lock poisoned");
        let _shadow_guard = EnvVarGuard::set(NOVOVM_UA_PHASE4_SHADOW_MODE_ENFORCE_ENV, "false");
        let (base, store, audit) = temp_paths("mapped-live-reorg-release-blocked");
        let root = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let native_store = root.join("native-execution-store.json");
        ensure_native_store(native_store.as_path());
        ua_create(&base, &store, &audit, "acct-map-live-reorg-release", 10);

        let mut register_map = match mapped_lock_live_event_proof_params(
            "acct-map-live-reorg-release",
            0x3e,
            99u128,
        ) {
            Value::Object(map) => map,
            other => panic!("expected mapped lock proof params object, got {other:?}"),
        };
        seed_mapped_lock_trusted_block_from_params(&Value::Object(register_map.clone()), true);
        register_map.insert("phase4_mode".to_string(), Value::String("live".to_string()));
        register_map.insert("now".to_string(), Value::from(11u64));
        let register_params = Value::Object(register_map.clone());
        let register = run_query(
            &base,
            "ua_registerMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                register_params.clone(),
            ),
        );
        let burn = run_query(
            &base,
            "ua_burnMappedAsset",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-reorg-release",
                    "mapping_id": register["mapping_id"],
                    "now": 12u64,
                }),
            ),
        );
        assert_eq!(burn["burned"].as_bool(), Some(true));

        reorg_mapped_lock_trusted_block_from_params(&register_params);
        let release_err = run_query_err(
            &base,
            "ua_releaseMappedLock",
            params_with_paths_and_native_store(
                &store,
                &audit,
                &native_store,
                json!({
                    "account_id": "acct-map-live-reorg-release",
                    "mapping_id": register["mapping_id"],
                    "now": 13u64,
                }),
            ),
        );
        assert!(
            release_err.contains("ERR_MAPPED_ASSET_SOURCE_ANCHOR_UNSAFE"),
            "reorged source anchor should block release, got: {release_err}"
        );
        let after_failed_release = run_query(
            &base,
            "ua_getMappedAsset",
            params_with_paths(
                &store,
                &audit,
                json!({
                    "account_id": "acct-map-live-reorg-release",
                    "mapping_id": register["mapping_id"],
                }),
            ),
        );
        assert_eq!(
            after_failed_release["mapped_asset"]["status"].as_str(),
            Some("burn_pending")
        );
        assert_eq!(
            after_failed_release["source_anchor_status"]["state"].as_str(),
            Some("blocked")
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
