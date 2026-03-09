#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_adapter_api::{
    AccountPolicy, AccountRole, NonceScope, PersonaAddress, PersonaType, ProtocolKind,
    RouteDecision, RouteRequest, UnifiedAccountRouter,
};
use novovm_adapter_evm_core::{translate_raw_evm_tx_fields_m0, tx_ir_from_raw_fields_m0};
use novovm_exec::{OpsWireOp, OpsWireV1Builder};
use rocksdb::{
    ColumnFamilyDescriptor, Options as RocksDbOptions, DB as RocksDb, DEFAULT_COLUMN_FAMILY_NAME,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

const GATEWAY_UA_STORE_ENVELOPE_VERSION: u32 = 1;
const GATEWAY_UA_STORE_BACKEND_FILE: &str = "bincode_file";
const GATEWAY_UA_STORE_BACKEND_ROCKSDB: &str = "rocksdb";
const GATEWAY_UA_STORE_ROCKSDB_CF_STATE: &str = "ua_gateway_state_v1";
const GATEWAY_UA_STORE_ROCKSDB_KEY_ROUTER: &[u8] = b"ua_gateway:router:v1";
const GATEWAY_UA_PRIMARY_KEY_DOMAIN: &[u8] = b"novovm_gateway_uca_primary_key_ref_v1";
const GATEWAY_INGRESS_RECORD_VERSION: u16 = 1;
const GATEWAY_INGRESS_PROTOCOL_ETH: u8 = 1;
const GATEWAY_INGRESS_PROTOCOL_WEB30: u8 = 2;

static SPOOL_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize)]
struct GatewayUaStoreEnvelopeV1 {
    version: u32,
    router: UnifiedAccountRouter,
}

#[derive(Debug, Clone)]
enum GatewayUaStoreBackend {
    BincodeFile { path: PathBuf },
    RocksDb { path: PathBuf },
}

#[derive(Debug, Serialize, Deserialize)]
struct GatewayIngressEthRecordV1 {
    version: u16,
    protocol: u8,
    uca_id: String,
    chain_id: u64,
    nonce: u64,
    tx_type: u8,
    tx_type4: bool,
    from: Vec<u8>,
    to: Option<Vec<u8>>,
    value: u128,
    gas_limit: u64,
    gas_price: u64,
    data: Vec<u8>,
    signature: Vec<u8>,
    tx_hash: [u8; 32],
    signature_domain: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GatewayIngressWeb30RecordV1 {
    version: u16,
    protocol: u8,
    uca_id: String,
    chain_id: u64,
    nonce: u64,
    from: Vec<u8>,
    payload: Vec<u8>,
    is_raw: bool,
    signature_domain: String,
    wants_cross_chain_atomic: bool,
    tx_hash: [u8; 32],
}

struct GatewayWeb30TxHashInput<'a> {
    uca_id: &'a str,
    chain_id: u64,
    nonce: u64,
    from: &'a [u8],
    payload: &'a [u8],
    signature_domain: &'a str,
    is_raw: bool,
    wants_cross_chain_atomic: bool,
}

struct GatewayEthTxHashInput<'a> {
    uca_id: &'a str,
    chain_id: u64,
    nonce: u64,
    tx_type: u8,
    tx_type4: bool,
    from: &'a [u8],
    to: Option<&'a [u8]>,
    value: u128,
    gas_limit: u64,
    gas_price: u64,
    data: &'a [u8],
    signature: &'a [u8],
    signature_domain: &'a str,
    wants_cross_chain_atomic: bool,
}

#[derive(Debug, Clone)]
struct GatewayEthTxIndexEntry {
    tx_hash: [u8; 32],
    uca_id: String,
    chain_id: u64,
    nonce: u64,
    tx_type: u8,
    from: Vec<u8>,
    to: Option<Vec<u8>>,
    value: u128,
    gas_limit: u64,
    gas_price: u64,
    input: Vec<u8>,
}

#[derive(Debug)]
struct GatewayRuntime {
    bind: String,
    spool_dir: PathBuf,
    max_body_bytes: usize,
    max_requests: u32,
    ua_store: GatewayUaStoreBackend,
    eth_tx_index: HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    router: UnifiedAccountRouter,
}

fn main() -> Result<()> {
    let mut runtime = GatewayRuntime::from_env()?;
    println!(
        "gateway_in: bind={} spool_dir={} max_body={} max_requests={} ua_store_backend={} ua_store_path={} internal_ingress=ops_wire_v1",
        runtime.bind,
        runtime.spool_dir.display(),
        runtime.max_body_bytes,
        runtime.max_requests,
        runtime.ua_store.backend_name(),
        runtime.ua_store.path().display()
    );

    let server = tiny_http::Server::http(&runtime.bind)
        .map_err(|e| anyhow::anyhow!("start gateway server failed on {}: {}", runtime.bind, e))?;
    let mut processed = 0u32;
    for request in server.incoming_requests() {
        handle_gateway_request(&mut runtime, request)?;
        processed = processed.saturating_add(1);
        if runtime.max_requests > 0 && processed >= runtime.max_requests {
            break;
        }
    }
    println!(
        "gateway_out: bind={} processed={} max_requests={}",
        runtime.bind, processed, runtime.max_requests
    );
    Ok(())
}

impl GatewayRuntime {
    fn from_env() -> Result<Self> {
        let bind = string_env("NOVOVM_GATEWAY_BIND", "127.0.0.1:9899");
        let spool_dir = PathBuf::from(string_env(
            "NOVOVM_GATEWAY_SPOOL_DIR",
            "artifacts/ingress/spool",
        ));
        let max_body_bytes = u64_env("NOVOVM_GATEWAY_MAX_BODY_BYTES", 64 * 1024) as usize;
        let max_requests = u32_env_allow_zero("NOVOVM_GATEWAY_MAX_REQUESTS", 0);
        let ua_store = resolve_gateway_ua_store_backend()?;
        let router = ua_store.load_router()?;
        Ok(Self {
            bind,
            spool_dir,
            max_body_bytes,
            max_requests,
            ua_store,
            eth_tx_index: HashMap::new(),
            router,
        })
    }
}

impl GatewayUaStoreBackend {
    fn backend_name(&self) -> &'static str {
        match self {
            GatewayUaStoreBackend::BincodeFile { .. } => GATEWAY_UA_STORE_BACKEND_FILE,
            GatewayUaStoreBackend::RocksDb { .. } => GATEWAY_UA_STORE_BACKEND_ROCKSDB,
        }
    }

    fn path(&self) -> &Path {
        match self {
            GatewayUaStoreBackend::BincodeFile { path } => path.as_path(),
            GatewayUaStoreBackend::RocksDb { path } => path.as_path(),
        }
    }

    fn load_router(&self) -> Result<UnifiedAccountRouter> {
        match self {
            GatewayUaStoreBackend::BincodeFile { path } => {
                if !path.exists() {
                    return Ok(UnifiedAccountRouter::new());
                }
                let raw = fs::read(path)
                    .with_context(|| format!("read gateway ua store failed: {}", path.display()))?;
                if raw.is_empty() {
                    return Ok(UnifiedAccountRouter::new());
                }
                if let Ok(envelope) = bincode::deserialize::<GatewayUaStoreEnvelopeV1>(&raw) {
                    if envelope.version != GATEWAY_UA_STORE_ENVELOPE_VERSION {
                        bail!(
                            "unsupported gateway ua store version {} at {}",
                            envelope.version,
                            path.display()
                        );
                    }
                    return Ok(envelope.router);
                }
                let router: UnifiedAccountRouter =
                    bincode::deserialize(&raw).with_context(|| {
                        format!("decode legacy gateway ua store failed: {}", path.display())
                    })?;
                Ok(router)
            }
            GatewayUaStoreBackend::RocksDb { path } => {
                let db = open_gateway_ua_rocksdb(path)?;
                let state_cf =
                    db.cf_handle(GATEWAY_UA_STORE_ROCKSDB_CF_STATE)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "missing gateway ua rocksdb column family '{}' for {}",
                                GATEWAY_UA_STORE_ROCKSDB_CF_STATE,
                                path.display()
                            )
                        })?;
                let mut raw = db
                    .get_cf(state_cf, GATEWAY_UA_STORE_ROCKSDB_KEY_ROUTER)
                    .with_context(|| {
                        format!(
                            "read gateway ua router key from cf '{}' failed: {}",
                            GATEWAY_UA_STORE_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                if raw.is_none() {
                    raw = db
                        .get(GATEWAY_UA_STORE_ROCKSDB_KEY_ROUTER)
                        .with_context(|| {
                            format!(
                                "read gateway ua legacy router key from default cf failed: {}",
                                path.display()
                            )
                        })?;
                }
                let Some(raw) = raw else {
                    return Ok(UnifiedAccountRouter::new());
                };
                if raw.is_empty() {
                    return Ok(UnifiedAccountRouter::new());
                }
                if let Ok(envelope) = bincode::deserialize::<GatewayUaStoreEnvelopeV1>(&raw) {
                    if envelope.version != GATEWAY_UA_STORE_ENVELOPE_VERSION {
                        bail!(
                            "unsupported gateway ua store version {} at {}",
                            envelope.version,
                            path.display()
                        );
                    }
                    return Ok(envelope.router);
                }
                let router: UnifiedAccountRouter =
                    bincode::deserialize(&raw).with_context(|| {
                        format!(
                            "decode legacy gateway ua rocksdb state failed: {}",
                            path.display()
                        )
                    })?;
                Ok(router)
            }
        }
    }

    fn save_router(&self, router: &UnifiedAccountRouter) -> Result<()> {
        #[derive(Serialize)]
        struct GatewayUaStoreEnvelopeRef<'a> {
            version: u32,
            router: &'a UnifiedAccountRouter,
        }
        let envelope = GatewayUaStoreEnvelopeRef {
            version: GATEWAY_UA_STORE_ENVELOPE_VERSION,
            router,
        };
        let encoded =
            bincode::serialize(&envelope).context("serialize gateway ua store envelope failed")?;
        match self {
            GatewayUaStoreBackend::BincodeFile { path } => {
                ensure_parent_dir(path, "gateway ua store")?;
                fs::write(path, encoded).with_context(|| {
                    format!("write gateway ua store failed: {}", path.display())
                })?;
                Ok(())
            }
            GatewayUaStoreBackend::RocksDb { path } => {
                let db = open_gateway_ua_rocksdb(path)?;
                let state_cf =
                    db.cf_handle(GATEWAY_UA_STORE_ROCKSDB_CF_STATE)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "missing gateway ua rocksdb column family '{}' for {}",
                                GATEWAY_UA_STORE_ROCKSDB_CF_STATE,
                                path.display()
                            )
                        })?;
                db.put_cf(state_cf, GATEWAY_UA_STORE_ROCKSDB_KEY_ROUTER, encoded)
                    .with_context(|| {
                        format!(
                            "write gateway ua router key into cf '{}' failed: {}",
                            GATEWAY_UA_STORE_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                Ok(())
            }
        }
    }
}

fn resolve_gateway_ua_store_backend() -> Result<GatewayUaStoreBackend> {
    let backend = string_env(
        "NOVOVM_GATEWAY_UA_STORE_BACKEND",
        GATEWAY_UA_STORE_BACKEND_ROCKSDB,
    )
    .trim()
    .to_ascii_lowercase();
    let path = if let Some(custom) = string_env_nonempty("NOVOVM_GATEWAY_UA_STORE_PATH") {
        PathBuf::from(custom)
    } else {
        match backend.as_str() {
            GATEWAY_UA_STORE_BACKEND_ROCKSDB => {
                PathBuf::from("artifacts/gateway/unified-account-router.rocksdb")
            }
            _ => PathBuf::from("artifacts/gateway/unified-account-router.bin"),
        }
    };
    match backend.as_str() {
        "bincode_file" | "file" | "bincode" => Ok(GatewayUaStoreBackend::BincodeFile { path }),
        "rocksdb" => Ok(GatewayUaStoreBackend::RocksDb { path }),
        _ => bail!(
            "invalid NOVOVM_GATEWAY_UA_STORE_BACKEND={}; valid: rocksdb|bincode_file|file|bincode",
            backend
        ),
    }
}

fn open_gateway_ua_rocksdb(path: &Path) -> Result<RocksDb> {
    ensure_parent_dir(path, "gateway ua rocksdb")?;
    let mut opts = RocksDbOptions::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);
    let cf_descriptors = vec![
        ColumnFamilyDescriptor::new(DEFAULT_COLUMN_FAMILY_NAME, RocksDbOptions::default()),
        ColumnFamilyDescriptor::new(GATEWAY_UA_STORE_ROCKSDB_CF_STATE, RocksDbOptions::default()),
    ];
    RocksDb::open_cf_descriptors(&opts, path, cf_descriptors)
        .with_context(|| format!("open gateway ua rocksdb failed: {}", path.display()))
}

fn handle_gateway_request(
    runtime: &mut GatewayRuntime,
    mut request: tiny_http::Request,
) -> Result<()> {
    if request.method() != &tiny_http::Method::Post {
        let body = rpc_error_body(
            serde_json::Value::Null,
            -32600,
            "only HTTP POST is supported on gateway endpoint",
        );
        respond_json_http(request, 405, &body)?;
        return Ok(());
    }

    let mut body_bytes = Vec::new();
    request
        .as_reader()
        .take((runtime.max_body_bytes as u64).saturating_add(1))
        .read_to_end(&mut body_bytes)
        .context("read gateway request body failed")?;
    if body_bytes.is_empty() {
        let body = rpc_error_body(serde_json::Value::Null, -32600, "request body is empty");
        respond_json_http(request, 400, &body)?;
        return Ok(());
    }
    if body_bytes.len() > runtime.max_body_bytes {
        let body = rpc_error_body(serde_json::Value::Null, -32600, "request body too large");
        respond_json_http(request, 413, &body)?;
        return Ok(());
    }

    let payload: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            let body = rpc_error_body(
                serde_json::Value::Null,
                -32700,
                &format!("invalid JSON payload: {e}"),
            );
            respond_json_http(request, 400, &body)?;
            return Ok(());
        }
    };
    let id = payload
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let method = match payload.get("method").and_then(|v| v.as_str()) {
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => {
            let body = rpc_error_body(id, -32600, "missing jsonrpc method");
            respond_json_http(request, 400, &body)?;
            return Ok(());
        }
    };
    let params = payload
        .get("params")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    match run_gateway_method(
        &mut runtime.router,
        &mut runtime.eth_tx_index,
        method,
        &params,
        &runtime.spool_dir,
    ) {
        Ok((result, changed)) => {
            if changed {
                runtime.ua_store.save_router(&runtime.router)?;
            }
            let body = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result,
            });
            respond_json_http(request, 200, &body)?;
        }
        Err(e) => {
            // Keep state consistency if any runtime step failed after mutation.
            if let Ok(restored) = runtime.ua_store.load_router() {
                runtime.router = restored;
            }
            let code = gateway_error_code_for_method(method, &e.to_string());
            let body = rpc_error_body(id, code, &e.to_string());
            respond_json_http(request, 200, &body)?;
        }
    }
    Ok(())
}

fn run_gateway_method(
    router: &mut UnifiedAccountRouter,
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    method: &str,
    params: &serde_json::Value,
    spool_dir: &Path,
) -> Result<(serde_json::Value, bool)> {
    match method {
        "ua_createUca" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for ua_createUca"))?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let primary_key_ref = parse_primary_key_ref(params, &uca_id)?;
            router.create_uca(uca_id.clone(), primary_key_ref, now)?;
            Ok((
                serde_json::json!({
                    "method": method,
                    "created": true,
                    "uca_id": uca_id,
                }),
                true,
            ))
        }
        "ua_rotatePrimaryKey" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for ua_rotatePrimaryKey"))?;
            let role = parse_account_role(params)?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let next_primary_key_ref =
                if let Some(raw) = param_as_string(params, "next_primary_key_ref") {
                    decode_hex_bytes(&raw, "next_primary_key_ref")?
                } else {
                    parse_primary_key_ref(params, &format!("{}:rotated:{}", uca_id, now))?
                };
            router.rotate_primary_key(&uca_id, role, next_primary_key_ref, now)?;
            Ok((
                serde_json::json!({
                    "method": method,
                    "rotated": true,
                    "uca_id": uca_id,
                }),
                true,
            ))
        }
        "ua_bindPersona" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for ua_bindPersona"))?;
            let role = parse_account_role(params)?;
            let persona_type = parse_persona_type(params, "persona_type")?;
            let chain_id = param_as_u64(params, "chain_id")
                .ok_or_else(|| anyhow::anyhow!("chain_id is required for ua_bindPersona"))?;
            let external_address = parse_external_address(params, "external_address")?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let persona = PersonaAddress {
                persona_type,
                chain_id,
                external_address,
            };
            router.add_binding(&uca_id, role, persona.clone(), now)?;
            Ok((
                serde_json::json!({
                    "method": method,
                    "bound": true,
                    "uca_id": uca_id,
                    "persona_type": persona.persona_type.as_str(),
                    "chain_id": persona.chain_id,
                }),
                true,
            ))
        }
        "ua_revokePersona" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for ua_revokePersona"))?;
            let role = parse_account_role(params)?;
            let persona_type = parse_persona_type(params, "persona_type")?;
            let chain_id = param_as_u64(params, "chain_id")
                .ok_or_else(|| anyhow::anyhow!("chain_id is required for ua_revokePersona"))?;
            let external_address = parse_external_address(params, "external_address")?;
            let cooldown_seconds = param_as_u64(params, "cooldown_seconds").unwrap_or(0);
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let persona = PersonaAddress {
                persona_type,
                chain_id,
                external_address,
            };
            router.revoke_binding(&uca_id, role, persona.clone(), cooldown_seconds, now)?;
            Ok((
                serde_json::json!({
                    "method": method,
                    "revoked": true,
                    "uca_id": uca_id,
                    "persona_type": persona.persona_type.as_str(),
                    "chain_id": persona.chain_id,
                    "cooldown_seconds": cooldown_seconds,
                }),
                true,
            ))
        }
        "ua_getBindingOwner" => {
            let persona_type = parse_persona_type(params, "persona_type")?;
            let chain_id = param_as_u64(params, "chain_id")
                .ok_or_else(|| anyhow::anyhow!("chain_id is required for ua_getBindingOwner"))?;
            let external_address = parse_external_address(params, "external_address")?;
            let persona = PersonaAddress {
                persona_type,
                chain_id,
                external_address,
            };
            let owner = router.resolve_binding_owner(&persona).map(str::to_string);
            Ok((
                serde_json::json!({
                    "method": method,
                    "found": owner.is_some(),
                    "owner_uca_id": owner,
                    "persona_type": persona.persona_type.as_str(),
                    "chain_id": persona.chain_id,
                }),
                false,
            ))
        }
        "eth_getTransactionCount" => {
            let chain_id = param_as_u64(params, "chain_id").unwrap_or(1);
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address (or from/external_address) is required"))?;
            let external_address = decode_hex_bytes(&address_raw, "address")?;
            let persona = PersonaAddress {
                persona_type: PersonaType::Evm,
                chain_id,
                external_address: external_address.clone(),
            };
            let owner = router.resolve_binding_owner(&persona).map(str::to_string);
            let explicit_uca_id = param_as_string(params, "uca_id");
            let uca_id = match (explicit_uca_id, owner) {
                (Some(explicit), Some(owner_id)) => {
                    if explicit != owner_id {
                        bail!(
                            "uca_id mismatch for address binding: explicit={} binding_owner={}",
                            explicit,
                            owner_id
                        );
                    }
                    explicit
                }
                (Some(explicit), None) => {
                    bail!(
                        "binding not found for address on chain_id={} (uca_id={})",
                        chain_id,
                        explicit
                    );
                }
                (None, Some(owner_id)) => owner_id,
                (None, None) => {
                    bail!("binding not found for address on chain_id={}", chain_id);
                }
            };
            let nonce = router.next_nonce_for_persona(&uca_id, &persona)?;
            Ok((
                serde_json::json!({
                    "method": method,
                    "uca_id": uca_id,
                    "chain_id": chain_id,
                    "address": format!("0x{}", to_hex(&external_address)),
                    "nonce": nonce,
                    "nonce_hex": format!("0x{:x}", nonce),
                }),
                false,
            ))
        }
        "eth_getTransactionByHash" => {
            let tx_hash_raw = extract_eth_tx_hash_query_param(params)
                .ok_or_else(|| anyhow::anyhow!("tx_hash (or hash) is required"))?;
            let tx_hash_bytes = decode_hex_bytes(&tx_hash_raw, "tx_hash")?;
            let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash")?;
            if let Some(entry) = eth_tx_index.get(&tx_hash) {
                Ok((gateway_eth_tx_by_hash_json(entry), false))
            } else {
                Ok((serde_json::Value::Null, false))
            }
        }
        "eth_getTransactionReceipt" => {
            let tx_hash_raw = extract_eth_tx_hash_query_param(params)
                .ok_or_else(|| anyhow::anyhow!("tx_hash (or hash) is required"))?;
            let tx_hash_bytes = decode_hex_bytes(&tx_hash_raw, "tx_hash")?;
            let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash")?;
            if let Some(entry) = eth_tx_index.get(&tx_hash) {
                Ok((gateway_eth_tx_receipt_json(entry), false))
            } else {
                Ok((serde_json::Value::Null, false))
            }
        }
        "ua_setPolicy" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for ua_setPolicy"))?;
            let role = parse_account_role(params)?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let nonce_scope = match param_as_string(params, "nonce_scope")
                .unwrap_or_else(|| "persona".to_string())
                .to_ascii_lowercase()
                .as_str()
            {
                "persona" => NonceScope::Persona,
                "chain" => NonceScope::Chain,
                "global" => NonceScope::Global,
                other => bail!("invalid nonce_scope: {}; valid: persona|chain|global", other),
            };
            let allow_type4_with_delegate_or_session =
                param_as_bool(params, "allow_type4_with_delegate_or_session").unwrap_or(false);
            router.update_policy(
                &uca_id,
                role,
                AccountPolicy {
                    nonce_scope,
                    allow_type4_with_delegate_or_session,
                },
                now,
            )?;
            Ok((
                serde_json::json!({
                    "method": method,
                    "updated": true,
                    "uca_id": uca_id,
                    "nonce_scope": match nonce_scope {
                        NonceScope::Persona => "persona",
                        NonceScope::Chain => "chain",
                        NonceScope::Global => "global",
                    },
                    "allow_type4_with_delegate_or_session": allow_type4_with_delegate_or_session,
                }),
                true,
            ))
        }
        "eth_sendRawTransaction" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for eth_sendRawTransaction"))?;
            let role = parse_account_role(params)?;
            let raw_tx_hex = extract_eth_raw_tx_param(params)
                .ok_or_else(|| anyhow::anyhow!("raw_tx is required for eth_sendRawTransaction"))?;
            let raw_tx = decode_hex_bytes(&raw_tx_hex, "raw_tx")?;
            let fields = translate_raw_evm_tx_fields_m0(&raw_tx)?;

            let explicit_chain_id = param_as_u64(params, "chain_id");
            if let (Some(explicit), Some(inferred)) = (explicit_chain_id, fields.chain_id) {
                if explicit != inferred {
                    bail!(
                        "chain_id mismatch: explicit={} inferred_from_raw={}",
                        explicit,
                        inferred
                    );
                }
            }
            let chain_id = explicit_chain_id.or(fields.chain_id).ok_or_else(|| {
                anyhow::anyhow!("chain_id is required for eth_sendRawTransaction")
            })?;

            let explicit_nonce = param_as_u64(params, "nonce");
            if let (Some(explicit), Some(inferred)) = (explicit_nonce, fields.nonce) {
                if explicit != inferred {
                    bail!(
                        "nonce mismatch: explicit={} inferred_from_raw={}",
                        explicit,
                        inferred
                    );
                }
            }
            let nonce = explicit_nonce
                .or(fields.nonce)
                .ok_or_else(|| anyhow::anyhow!("nonce is required for eth_sendRawTransaction"))?;

            let from_raw = extract_eth_persona_address_param(params).ok_or_else(|| {
                anyhow::anyhow!("from (or external_address) is required for eth_sendRawTransaction")
            })?;
            let from = decode_hex_bytes(&from_raw, "from")?;
            let signature_domain = param_as_string(params, "signature_domain")
                .unwrap_or_else(|| format!("evm:{chain_id}"));
            let wants_cross_chain_atomic =
                param_as_bool(params, "wants_cross_chain_atomic").unwrap_or(false);
            let session_expires_at = param_as_u64(params, "session_expires_at");
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);

            let decision = router.route(RouteRequest {
                uca_id: uca_id.clone(),
                persona: PersonaAddress {
                    persona_type: PersonaType::Evm,
                    chain_id,
                    external_address: from.clone(),
                },
                role,
                protocol: ProtocolKind::Eth,
                signature_domain: signature_domain.clone(),
                nonce,
                wants_cross_chain_atomic,
                tx_type4: fields.hint.tx_type4,
                session_expires_at,
                now,
            })?;
            let tx_ir = tx_ir_from_raw_fields_m0(&fields, &raw_tx, from.clone(), chain_id);
            let tx_hash = vec_to_32(&tx_ir.hash, "tx_hash")?;
            let record = GatewayIngressEthRecordV1 {
                version: GATEWAY_INGRESS_RECORD_VERSION,
                protocol: GATEWAY_INGRESS_PROTOCOL_ETH,
                uca_id: uca_id.clone(),
                chain_id,
                nonce,
                tx_type: fields.hint.tx_type_number,
                tx_type4: fields.hint.tx_type4,
                from,
                to: tx_ir.to.clone(),
                value: tx_ir.value,
                gas_limit: tx_ir.gas_limit,
                gas_price: tx_ir.gas_price,
                data: tx_ir.data.clone(),
                signature: raw_tx,
                tx_hash,
                signature_domain: signature_domain.clone(),
            };
            let wire = encode_gateway_ingress_ops_wire_v1_eth(&record)?;
            let spool_file = write_spool_ops_wire_v1(spool_dir, &wire)?;
            upsert_gateway_eth_tx_index(eth_tx_index, &record);

            Ok((
                serde_json::json!({
                    "method": method,
                    "accepted": true,
                    "uca_id": uca_id,
                    "decision": match decision {
                        RouteDecision::FastPath => serde_json::json!({"kind": "fast_path"}),
                        RouteDecision::Adapter { chain_id } => serde_json::json!({"kind": "adapter", "chain_id": chain_id}),
                    },
                    "signature_domain": signature_domain,
                    "nonce": nonce,
                    "tx_type": fields.hint.tx_type_number,
                    "tx_type4": fields.hint.tx_type4,
                    "tx_hash": format!("0x{}", to_hex(&tx_hash)),
                    "spool_file": spool_file.display().to_string(),
                    "ingress_codec": "ops_wire_v1",
                }),
                true,
            ))
        }
        "eth_sendTransaction" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for eth_sendTransaction"))?;
            let role = parse_account_role(params)?;
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .ok_or_else(|| anyhow::anyhow!("chain_id is required for eth_sendTransaction"))?;
            let nonce = param_as_u64_any_with_tx(params, &["nonce"])
                .ok_or_else(|| anyhow::anyhow!("nonce is required for eth_sendTransaction"))?;
            let from_raw = extract_eth_persona_address_param(params).ok_or_else(|| {
                anyhow::anyhow!("from (or external_address) is required for eth_sendTransaction")
            })?;
            let from = decode_hex_bytes(&from_raw, "from")?;
            let to = match param_as_string_any_with_tx(params, &["to"]) {
                Some(raw_to) => {
                    let trimmed = raw_to.trim();
                    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("0x") {
                        None
                    } else {
                        Some(decode_hex_bytes(trimmed, "to")?)
                    }
                }
                None => None,
            };
            let data = match param_as_string_any_with_tx(params, &["data", "input"]) {
                Some(raw_data) => {
                    let trimmed = raw_data.trim();
                    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("0x") {
                        Vec::new()
                    } else {
                        decode_hex_bytes(trimmed, "data")?
                    }
                }
                None => Vec::new(),
            };
            let value = param_as_u128_any_with_tx(params, &["value"]).unwrap_or(0);
            let gas_limit = param_as_u64_any_with_tx(params, &["gas_limit", "gasLimit", "gas"])
                .unwrap_or(21_000);
            let gas_price = param_as_u64_any_with_tx(
                params,
                &[
                    "gas_price",
                    "gasPrice",
                    "max_fee_per_gas",
                    "maxFeePerGas",
                    "max_priority_fee_per_gas",
                    "maxPriorityFeePerGas",
                ],
            )
            .unwrap_or(1);
            let tx_type_u64 =
                param_as_u64_any_with_tx(params, &["tx_type", "txType", "type"]).unwrap_or(0);
            if tx_type_u64 > u8::MAX as u64 {
                bail!("tx_type out of range: {}", tx_type_u64);
            }
            let tx_type = tx_type_u64 as u8;
            if tx_type == 3 {
                bail!("blob (type 3) write path disabled");
            }
            let tx_type4 =
                param_as_bool_any_with_tx(params, &["tx_type4"]).unwrap_or(false) || tx_type == 4;
            let signature = match param_as_string_any_with_tx(
                params,
                &["signature", "raw_signature", "signed_tx"],
            ) {
                Some(raw_sig) => decode_hex_bytes(&raw_sig, "signature")?,
                None => Vec::new(),
            };
            let signature_domain = param_as_string(params, "signature_domain")
                .or_else(|| param_as_string_any_with_tx(params, &["signature_domain"]))
                .unwrap_or_else(|| format!("evm:{chain_id}"));
            let wants_cross_chain_atomic =
                param_as_bool(params, "wants_cross_chain_atomic").unwrap_or(false);
            let session_expires_at = param_as_u64(params, "session_expires_at");
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);

            let decision = router.route(RouteRequest {
                uca_id: uca_id.clone(),
                persona: PersonaAddress {
                    persona_type: PersonaType::Evm,
                    chain_id,
                    external_address: from.clone(),
                },
                role,
                protocol: ProtocolKind::Eth,
                signature_domain: signature_domain.clone(),
                nonce,
                wants_cross_chain_atomic,
                tx_type4,
                session_expires_at,
                now,
            })?;
            let tx_hash = compute_gateway_eth_tx_hash(&GatewayEthTxHashInput {
                uca_id: &uca_id,
                chain_id,
                nonce,
                tx_type,
                tx_type4,
                from: &from,
                to: to.as_deref(),
                value,
                gas_limit,
                gas_price,
                data: &data,
                signature: &signature,
                signature_domain: &signature_domain,
                wants_cross_chain_atomic,
            });
            let record = GatewayIngressEthRecordV1 {
                version: GATEWAY_INGRESS_RECORD_VERSION,
                protocol: GATEWAY_INGRESS_PROTOCOL_ETH,
                uca_id: uca_id.clone(),
                chain_id,
                nonce,
                tx_type,
                tx_type4,
                from,
                to: to.clone(),
                value,
                gas_limit,
                gas_price,
                data: data.clone(),
                signature,
                tx_hash,
                signature_domain: signature_domain.clone(),
            };
            let wire = encode_gateway_ingress_ops_wire_v1_eth(&record)?;
            let spool_file = write_spool_ops_wire_v1(spool_dir, &wire)?;
            upsert_gateway_eth_tx_index(eth_tx_index, &record);

            Ok((
                serde_json::json!({
                    "method": method,
                    "accepted": true,
                    "uca_id": uca_id,
                    "decision": match decision {
                        RouteDecision::FastPath => serde_json::json!({"kind": "fast_path"}),
                        RouteDecision::Adapter { chain_id } => serde_json::json!({"kind": "adapter", "chain_id": chain_id}),
                    },
                    "signature_domain": signature_domain,
                    "nonce": nonce,
                    "tx_type": tx_type,
                    "tx_type4": tx_type4,
                    "tx_hash": format!("0x{}", to_hex(&record.tx_hash)),
                    "spool_file": spool_file.display().to_string(),
                    "ingress_codec": "ops_wire_v1",
                }),
                true,
            ))
        }
        "web30_sendRawTransaction" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for web30_sendRawTransaction"))?;
            let role = parse_account_role(params)?;
            let chain_id = param_as_u64(params, "chain_id")
                .ok_or_else(|| anyhow::anyhow!("chain_id is required for web30_sendRawTransaction"))?;
            let nonce = param_as_u64(params, "nonce")
                .ok_or_else(|| anyhow::anyhow!("nonce is required for web30_sendRawTransaction"))?;
            let from_raw = extract_eth_persona_address_param(params).ok_or_else(|| {
                anyhow::anyhow!(
                    "external_address (or from/address) is required for web30_sendRawTransaction"
                )
            })?;
            let from = decode_hex_bytes(&from_raw, "external_address")?;
            let raw_payload = extract_web30_raw_payload_param(params).ok_or_else(|| {
                anyhow::anyhow!("raw_tx/raw_transaction/raw/payload_hex is required for web30_sendRawTransaction")
            })?;
            let signature_domain = param_as_string(params, "signature_domain")
                .unwrap_or_else(|| "web30:mainnet".to_string());
            let wants_cross_chain_atomic =
                param_as_bool(params, "wants_cross_chain_atomic").unwrap_or(false);
            let session_expires_at = param_as_u64(params, "session_expires_at");
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);

            let decision = router.route(RouteRequest {
                uca_id: uca_id.clone(),
                persona: PersonaAddress {
                    persona_type: PersonaType::Web30,
                    chain_id,
                    external_address: from.clone(),
                },
                role,
                protocol: ProtocolKind::Web30,
                signature_domain: signature_domain.clone(),
                nonce,
                wants_cross_chain_atomic,
                tx_type4: false,
                session_expires_at,
                now,
            })?;
            let tx_hash = compute_gateway_web30_tx_hash(&GatewayWeb30TxHashInput {
                uca_id: &uca_id,
                chain_id,
                nonce,
                from: &from,
                payload: &raw_payload,
                signature_domain: &signature_domain,
                is_raw: true,
                wants_cross_chain_atomic,
            });
            let record = GatewayIngressWeb30RecordV1 {
                version: GATEWAY_INGRESS_RECORD_VERSION,
                protocol: GATEWAY_INGRESS_PROTOCOL_WEB30,
                uca_id: uca_id.clone(),
                chain_id,
                nonce,
                from,
                payload: raw_payload,
                is_raw: true,
                signature_domain: signature_domain.clone(),
                wants_cross_chain_atomic,
                tx_hash,
            };
            let wire = encode_gateway_ingress_ops_wire_v1_web30(&record)?;
            let spool_file = write_spool_ops_wire_v1(spool_dir, &wire)?;

            Ok((
                serde_json::json!({
                    "method": method,
                    "accepted": true,
                    "uca_id": uca_id,
                    "decision": match decision {
                        RouteDecision::FastPath => serde_json::json!({"kind": "fast_path"}),
                        RouteDecision::Adapter { chain_id } => serde_json::json!({"kind": "adapter", "chain_id": chain_id}),
                    },
                    "signature_domain": signature_domain,
                    "nonce": nonce,
                    "tx_hash": format!("0x{}", to_hex(&record.tx_hash)),
                    "spool_file": spool_file.display().to_string(),
                    "ingress_codec": "ops_wire_v1",
                }),
                true,
            ))
        }
        "web30_sendTransaction" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for web30_sendTransaction"))?;
            let role = parse_account_role(params)?;
            let chain_id = param_as_u64(params, "chain_id")
                .ok_or_else(|| anyhow::anyhow!("chain_id is required for web30_sendTransaction"))?;
            let nonce = param_as_u64(params, "nonce")
                .ok_or_else(|| anyhow::anyhow!("nonce is required for web30_sendTransaction"))?;
            let from_raw = extract_eth_persona_address_param(params).ok_or_else(|| {
                anyhow::anyhow!("external_address (or from/address) is required for web30_sendTransaction")
            })?;
            let from = decode_hex_bytes(&from_raw, "external_address")?;
            let payload = extract_web30_tx_payload(params)?;
            let signature_domain = param_as_string(params, "signature_domain")
                .unwrap_or_else(|| "web30:mainnet".to_string());
            let wants_cross_chain_atomic =
                param_as_bool(params, "wants_cross_chain_atomic").unwrap_or(false);
            let session_expires_at = param_as_u64(params, "session_expires_at");
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);

            let decision = router.route(RouteRequest {
                uca_id: uca_id.clone(),
                persona: PersonaAddress {
                    persona_type: PersonaType::Web30,
                    chain_id,
                    external_address: from.clone(),
                },
                role,
                protocol: ProtocolKind::Web30,
                signature_domain: signature_domain.clone(),
                nonce,
                wants_cross_chain_atomic,
                tx_type4: false,
                session_expires_at,
                now,
            })?;
            let tx_hash = compute_gateway_web30_tx_hash(&GatewayWeb30TxHashInput {
                uca_id: &uca_id,
                chain_id,
                nonce,
                from: &from,
                payload: &payload,
                signature_domain: &signature_domain,
                is_raw: false,
                wants_cross_chain_atomic,
            });
            let record = GatewayIngressWeb30RecordV1 {
                version: GATEWAY_INGRESS_RECORD_VERSION,
                protocol: GATEWAY_INGRESS_PROTOCOL_WEB30,
                uca_id: uca_id.clone(),
                chain_id,
                nonce,
                from,
                payload,
                is_raw: false,
                signature_domain: signature_domain.clone(),
                wants_cross_chain_atomic,
                tx_hash,
            };
            let wire = encode_gateway_ingress_ops_wire_v1_web30(&record)?;
            let spool_file = write_spool_ops_wire_v1(spool_dir, &wire)?;

            Ok((
                serde_json::json!({
                    "method": method,
                    "accepted": true,
                    "uca_id": uca_id,
                    "decision": match decision {
                        RouteDecision::FastPath => serde_json::json!({"kind": "fast_path"}),
                        RouteDecision::Adapter { chain_id } => serde_json::json!({"kind": "adapter", "chain_id": chain_id}),
                    },
                    "signature_domain": signature_domain,
                    "nonce": nonce,
                    "tx_hash": format!("0x{}", to_hex(&record.tx_hash)),
                    "spool_file": spool_file.display().to_string(),
                    "ingress_codec": "ops_wire_v1",
                }),
                true,
            ))
        }
        _ => bail!(
            "unknown method: {}; valid: ua_createUca|ua_rotatePrimaryKey|ua_bindPersona|ua_revokePersona|ua_getBindingOwner|ua_setPolicy|eth_sendRawTransaction|eth_sendTransaction|eth_getTransactionCount|eth_getTransactionByHash|eth_getTransactionReceipt|web30_sendRawTransaction|web30_sendTransaction",
            method
        ),
    }
}

fn encode_gateway_ingress_ops_wire_v1_eth(record: &GatewayIngressEthRecordV1) -> Result<Vec<u8>> {
    let value =
        bincode::serialize(record).context("serialize gateway ingress eth record failed")?;
    let key = gateway_ingress_key(record.protocol, &record.tx_hash);
    let plan_id = ((record.chain_id & 0xffff_ffff) << 32) | (record.nonce & 0xffff_ffff);
    encode_gateway_ingress_ops_wire_v1_record(&key, &value, plan_id)
}

fn encode_gateway_ingress_ops_wire_v1_web30(
    record: &GatewayIngressWeb30RecordV1,
) -> Result<Vec<u8>> {
    let value =
        bincode::serialize(record).context("serialize gateway ingress web30 record failed")?;
    let key = gateway_ingress_key(record.protocol, &record.tx_hash);
    let plan_id = ((record.chain_id & 0xffff_ffff) << 32) | (record.nonce & 0xffff_ffff);
    encode_gateway_ingress_ops_wire_v1_record(&key, &value, plan_id)
}

fn encode_gateway_ingress_ops_wire_v1_record(
    key: &[u8],
    value: &[u8],
    plan_id: u64,
) -> Result<Vec<u8>> {
    let mut builder = OpsWireV1Builder::new();
    builder.push(OpsWireOp {
        opcode: 2, // write
        flags: 0,
        reserved: 0,
        key,
        value,
        delta: 0,
        expect_version: None,
        plan_id,
    })?;
    Ok(builder.finish().bytes)
}

fn gateway_ingress_key(protocol: u8, tx_hash: &[u8; 32]) -> Vec<u8> {
    let prefix: &[u8] = if protocol == GATEWAY_INGRESS_PROTOCOL_WEB30 {
        b"gw:web30:tx:v1:"
    } else {
        b"gw:eth:tx:v1:"
    };
    let mut out = Vec::with_capacity(prefix.len() + tx_hash.len());
    out.extend_from_slice(prefix);
    out.extend_from_slice(tx_hash);
    out
}

fn upsert_gateway_eth_tx_index(
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    record: &GatewayIngressEthRecordV1,
) {
    let entry = GatewayEthTxIndexEntry {
        tx_hash: record.tx_hash,
        uca_id: record.uca_id.clone(),
        chain_id: record.chain_id,
        nonce: record.nonce,
        tx_type: record.tx_type,
        from: record.from.clone(),
        to: record.to.clone(),
        value: record.value,
        gas_limit: record.gas_limit,
        gas_price: record.gas_price,
        input: record.data.clone(),
    };
    eth_tx_index.insert(record.tx_hash, entry);
}

fn gateway_eth_tx_by_hash_json(entry: &GatewayEthTxIndexEntry) -> serde_json::Value {
    serde_json::json!({
        "hash": format!("0x{}", to_hex(&entry.tx_hash)),
        "nonce": format!("0x{:x}", entry.nonce),
        "blockHash": serde_json::Value::Null,
        "blockNumber": serde_json::Value::Null,
        "transactionIndex": serde_json::Value::Null,
        "from": format!("0x{}", to_hex(&entry.from)),
        "to": entry.to.as_ref().map(|v| format!("0x{}", to_hex(v))),
        "value": format!("0x{:x}", entry.value),
        "gas": format!("0x{:x}", entry.gas_limit),
        "gasPrice": format!("0x{:x}", entry.gas_price),
        "input": format!("0x{}", to_hex(&entry.input)),
        "chainId": format!("0x{:x}", entry.chain_id),
        "type": format!("0x{:x}", entry.tx_type),
        "pending": true,
        "uca_id": entry.uca_id.clone(),
    })
}

fn gateway_eth_tx_receipt_json(entry: &GatewayEthTxIndexEntry) -> serde_json::Value {
    serde_json::json!({
        "transactionHash": format!("0x{}", to_hex(&entry.tx_hash)),
        "transactionIndex": serde_json::Value::Null,
        "blockHash": serde_json::Value::Null,
        "blockNumber": serde_json::Value::Null,
        "from": format!("0x{}", to_hex(&entry.from)),
        "to": entry.to.as_ref().map(|v| format!("0x{}", to_hex(v))),
        "cumulativeGasUsed": format!("0x{:x}", entry.gas_limit),
        "gasUsed": format!("0x{:x}", entry.gas_limit),
        "effectiveGasPrice": format!("0x{:x}", entry.gas_price),
        "contractAddress": serde_json::Value::Null,
        "logs": [],
        "logsBloom": "0x0",
        "type": format!("0x{:x}", entry.tx_type),
        "status": serde_json::Value::Null,
        "pending": true,
        "uca_id": entry.uca_id.clone(),
    })
}

fn write_spool_ops_wire_v1(spool_dir: &Path, bytes: &[u8]) -> Result<PathBuf> {
    ensure_dir(spool_dir, "gateway spool dir")?;
    let now_ms = now_unix_millis();
    let seq = SPOOL_SEQ.fetch_add(1, Ordering::Relaxed);
    let base = format!("ingress-{now_ms}-{seq}");
    let tmp_path = spool_dir.join(format!("{base}.opsw1.tmp"));
    let out_path = spool_dir.join(format!("{base}.opsw1"));
    fs::write(&tmp_path, bytes).with_context(|| {
        format!(
            "write gateway spool temp file failed: {}",
            tmp_path.display()
        )
    })?;
    fs::rename(&tmp_path, &out_path).with_context(|| {
        format!(
            "atomic rename gateway spool file failed: {} -> {}",
            tmp_path.display(),
            out_path.display()
        )
    })?;
    Ok(out_path)
}

fn gateway_error_code_for_method(method: &str, message: &str) -> i64 {
    let lower = message.to_ascii_lowercase();
    if method == "eth_sendRawTransaction" || method == "eth_sendTransaction" {
        if lower.contains("blob (type 3) write path disabled") {
            return -32031;
        }
        if lower.contains("nonce mismatch")
            || lower.contains("chain_id mismatch")
            || lower.contains("binding")
            || lower.contains("domain mismatch")
        {
            return -32033;
        }
    }
    if method == "eth_getTransactionCount"
        && (lower.contains("binding")
            || lower.contains("uca_id mismatch")
            || lower.contains("nonce")
            || lower.contains("chain_id"))
    {
        return -32033;
    }
    if (method == "eth_getTransactionByHash" || method == "eth_getTransactionReceipt")
        && (lower.contains("tx_hash")
            || lower.contains("hash")
            || lower.contains("hex")
            || lower.contains("size mismatch"))
    {
        return -32033;
    }
    if (method == "web30_sendRawTransaction" || method == "web30_sendTransaction")
        && (lower.contains("nonce mismatch")
            || lower.contains("chain_id mismatch")
            || lower.contains("binding")
            || lower.contains("domain mismatch")
            || lower.contains("nonce")
            || lower.contains("address")
            || lower.contains("external_address")
            || lower.contains("payload"))
    {
        return -32033;
    }
    if method == "ua_createUca"
        || method == "ua_rotatePrimaryKey"
        || method == "ua_bindPersona"
        || method == "ua_revokePersona"
        || method == "ua_getBindingOwner"
        || method == "ua_setPolicy"
    {
        return -32010;
    }
    -32000
}

fn rpc_error_body(id: serde_json::Value, code: i64, message: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        }
    })
}

fn respond_json_http(
    request: tiny_http::Request,
    status: u16,
    body: &serde_json::Value,
) -> Result<()> {
    let payload = serde_json::to_string(body).context("serialize rpc response json failed")?;
    let mut response =
        tiny_http::Response::from_string(payload).with_status_code(tiny_http::StatusCode(status));
    if let Ok(header) =
        tiny_http::Header::from_bytes(b"Content-Type".to_vec(), b"application/json".to_vec())
    {
        response = response.with_header(header);
    }
    request
        .respond(response)
        .map_err(|e| anyhow::anyhow!("gateway response send failed: {e}"))?;
    Ok(())
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

fn ensure_dir(path: &Path, label: &str) -> Result<()> {
    fs::create_dir_all(path)
        .with_context(|| format!("create {} failed: {}", label, path.display()))?;
    Ok(())
}

fn vec_to_32(raw: &[u8], field: &str) -> Result<[u8; 32]> {
    if raw.len() != 32 {
        bail!("{} size mismatch: expected 32 got {}", field, raw.len());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(raw);
    Ok(out)
}

fn value_to_u64(v: &serde_json::Value) -> Option<u64> {
    match v {
        serde_json::Value::Number(n) => n.as_u64(),
        serde_json::Value::String(s) => parse_u64_decimal_or_hex(s),
        _ => None,
    }
}

fn value_to_u128(v: &serde_json::Value) -> Option<u128> {
    match v {
        serde_json::Value::Number(n) => n.as_u64().map(|v| v as u128),
        serde_json::Value::String(s) => parse_u128_decimal_or_hex(s),
        _ => None,
    }
}

fn parse_u64_decimal_or_hex(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        if hex.is_empty() {
            Some(0)
        } else {
            u64::from_str_radix(hex, 16).ok()
        }
    } else {
        trimmed.parse::<u64>().ok()
    }
}

fn parse_u128_decimal_or_hex(raw: &str) -> Option<u128> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        if hex.is_empty() {
            Some(0)
        } else {
            u128::from_str_radix(hex, 16).ok()
        }
    } else {
        trimmed.parse::<u128>().ok()
    }
}

fn value_to_string(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.trim().to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn params_primary_object(
    params: &serde_json::Value,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    match params {
        serde_json::Value::Object(map) => Some(map),
        serde_json::Value::Array(arr) => arr.first().and_then(serde_json::Value::as_object),
        _ => None,
    }
}

fn param_as_u128(params: &serde_json::Value, key: &str) -> Option<u128> {
    params_primary_object(params)
        .and_then(|map| map.get(key))
        .and_then(value_to_u128)
}

fn param_tx_object(params: &serde_json::Value) -> Option<&serde_json::Value> {
    let map = params_primary_object(params)?;
    match map.get("tx") {
        Some(tx_obj @ serde_json::Value::Object(_)) => Some(tx_obj),
        _ => None,
    }
}

fn param_as_u64_any_with_tx(params: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(value) = param_as_u64(params, key) {
            return Some(value);
        }
    }
    let tx = param_tx_object(params)?;
    for key in keys {
        if let Some(value) = param_as_u64(tx, key) {
            return Some(value);
        }
    }
    None
}

fn param_as_u128_any_with_tx(params: &serde_json::Value, keys: &[&str]) -> Option<u128> {
    for key in keys {
        if let Some(value) = param_as_u128(params, key) {
            return Some(value);
        }
    }
    let tx = param_tx_object(params)?;
    for key in keys {
        if let Some(value) = param_as_u128(tx, key) {
            return Some(value);
        }
    }
    None
}

fn param_as_string_any_with_tx(params: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = param_as_string(params, key) {
            return Some(value);
        }
    }
    let tx = param_tx_object(params)?;
    for key in keys {
        if let Some(value) = param_as_string(tx, key) {
            return Some(value);
        }
    }
    None
}

fn param_as_bool_any_with_tx(params: &serde_json::Value, keys: &[&str]) -> Option<bool> {
    for key in keys {
        if let Some(value) = param_as_bool(params, key) {
            return Some(value);
        }
    }
    let tx = param_tx_object(params)?;
    for key in keys {
        if let Some(value) = param_as_bool(tx, key) {
            return Some(value);
        }
    }
    None
}

fn value_to_bool(v: &serde_json::Value) -> Option<bool> {
    match v {
        serde_json::Value::Bool(b) => Some(*b),
        serde_json::Value::String(s) => {
            let t = s.trim();
            if t.eq_ignore_ascii_case("true") || t == "1" {
                Some(true)
            } else if t.eq_ignore_ascii_case("false") || t == "0" {
                Some(false)
            } else {
                None
            }
        }
        serde_json::Value::Number(n) => n.as_u64().map(|v| v != 0),
        _ => None,
    }
}

fn param_as_u64(params: &serde_json::Value, key: &str) -> Option<u64> {
    params_primary_object(params)
        .and_then(|map| map.get(key))
        .and_then(value_to_u64)
}

fn param_as_string(params: &serde_json::Value, key: &str) -> Option<String> {
    params_primary_object(params)
        .and_then(|map| map.get(key))
        .and_then(value_to_string)
}

fn param_as_bool(params: &serde_json::Value, key: &str) -> Option<bool> {
    params_primary_object(params)
        .and_then(|map| map.get(key))
        .and_then(value_to_bool)
}

fn parse_account_role(params: &serde_json::Value) -> Result<AccountRole> {
    let raw = param_as_string(params, "role")
        .unwrap_or_else(|| "owner".to_string())
        .to_ascii_lowercase();
    match raw.as_str() {
        "owner" => Ok(AccountRole::Owner),
        "delegate" => Ok(AccountRole::Delegate),
        "session" | "sessionkey" | "session_key" => Ok(AccountRole::SessionKey),
        _ => bail!("invalid role: {}; valid: owner|delegate|session_key", raw),
    }
}

fn parse_persona_type(params: &serde_json::Value, key: &str) -> Result<PersonaType> {
    let raw = param_as_string(params, key)
        .ok_or_else(|| anyhow::anyhow!("{} is required", key))?
        .to_ascii_lowercase();
    Ok(match raw.as_str() {
        "web30" => PersonaType::Web30,
        "evm" => PersonaType::Evm,
        "bitcoin" | "btc" => PersonaType::Bitcoin,
        "solana" | "sol" => PersonaType::Solana,
        other => PersonaType::Other(other.to_string()),
    })
}

fn parse_primary_key_ref(params: &serde_json::Value, uca_id: &str) -> Result<Vec<u8>> {
    if let Some(raw) = param_as_string(params, "primary_key_ref") {
        return decode_hex_bytes(&raw, "primary_key_ref");
    }
    let mut hasher = Sha256::new();
    hasher.update(GATEWAY_UA_PRIMARY_KEY_DOMAIN);
    hasher.update(uca_id.as_bytes());
    Ok(hasher.finalize().to_vec())
}

fn parse_external_address(params: &serde_json::Value, key: &str) -> Result<Vec<u8>> {
    let raw = param_as_string(params, key).ok_or_else(|| anyhow::anyhow!("{} is required", key))?;
    decode_hex_bytes(&raw, key)
}

fn pick_first_nonempty_string(
    map: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    for key in keys {
        if let Some(value) = map.get(*key).and_then(value_to_string) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn extract_web30_raw_payload_param(params: &serde_json::Value) -> Option<Vec<u8>> {
    const CANDIDATE_KEYS: &[&str] = &[
        "raw_tx",
        "rawTransaction",
        "raw_transaction",
        "raw",
        "payload_hex",
    ];
    let raw = match params {
        serde_json::Value::Object(map) => pick_first_nonempty_string(map, CANDIDATE_KEYS),
        serde_json::Value::Array(arr) => match arr.first() {
            Some(serde_json::Value::Object(map)) => pick_first_nonempty_string(map, CANDIDATE_KEYS),
            Some(first) => value_to_string(first).and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }),
            None => None,
        },
        _ => None,
    }?;
    decode_hex_bytes(&raw, "raw_tx").ok()
}

fn extract_web30_tx_payload(params: &serde_json::Value) -> Result<Vec<u8>> {
    if let Some(raw_hex) = extract_web30_raw_payload_param(params) {
        return Ok(raw_hex);
    }
    if let serde_json::Value::Object(map) = params {
        if let Some(value) = map.get("payload").and_then(value_to_string) {
            let trimmed = value.trim();
            if let Some(hex) = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"))
            {
                if !hex.is_empty() {
                    return decode_hex_bytes(trimmed, "payload");
                }
            }
            if !trimmed.is_empty() {
                return Ok(trimmed.as_bytes().to_vec());
            }
        }
        if let Some(tx_obj) = map.get("tx") {
            return serde_json::to_vec(tx_obj)
                .context("serialize tx object for web30 payload failed");
        }
    }
    serde_json::to_vec(params).context("serialize web30 transaction params payload failed")
}

fn compute_gateway_web30_tx_hash(input: &GatewayWeb30TxHashInput<'_>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm_gateway_web30_tx_hash_v1");
    hasher.update(input.uca_id.as_bytes());
    hasher.update(input.chain_id.to_le_bytes());
    hasher.update(input.nonce.to_le_bytes());
    hasher.update((input.from.len() as u64).to_le_bytes());
    hasher.update(input.from);
    hasher.update((input.payload.len() as u64).to_le_bytes());
    hasher.update(input.payload);
    hasher.update(input.signature_domain.as_bytes());
    hasher.update([if input.is_raw { 1 } else { 0 }]);
    hasher.update([if input.wants_cross_chain_atomic { 1 } else { 0 }]);
    let digest: [u8; 32] = hasher.finalize().into();
    digest
}

fn compute_gateway_eth_tx_hash(input: &GatewayEthTxHashInput<'_>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm_gateway_eth_tx_hash_v1");
    hasher.update(input.uca_id.as_bytes());
    hasher.update(input.chain_id.to_le_bytes());
    hasher.update(input.nonce.to_le_bytes());
    hasher.update([input.tx_type]);
    hasher.update([if input.tx_type4 { 1 } else { 0 }]);
    hasher.update((input.from.len() as u64).to_le_bytes());
    hasher.update(input.from);
    match input.to {
        Some(to) => {
            hasher.update([1]);
            hasher.update((to.len() as u64).to_le_bytes());
            hasher.update(to);
        }
        None => {
            hasher.update([0]);
        }
    }
    hasher.update(input.value.to_le_bytes());
    hasher.update(input.gas_limit.to_le_bytes());
    hasher.update(input.gas_price.to_le_bytes());
    hasher.update((input.data.len() as u64).to_le_bytes());
    hasher.update(input.data);
    hasher.update((input.signature.len() as u64).to_le_bytes());
    hasher.update(input.signature);
    hasher.update(input.signature_domain.as_bytes());
    hasher.update([if input.wants_cross_chain_atomic { 1 } else { 0 }]);
    hasher.finalize().into()
}

fn extract_eth_raw_tx_param(params: &serde_json::Value) -> Option<String> {
    const CANDIDATE_KEYS: &[&str] = &[
        "raw_tx",
        "rawTransaction",
        "raw_transaction",
        "raw",
        "signed_tx",
    ];
    match params {
        serde_json::Value::Object(map) => pick_first_nonempty_string(map, CANDIDATE_KEYS),
        serde_json::Value::Array(arr) => match arr.first() {
            Some(serde_json::Value::Object(map)) => pick_first_nonempty_string(map, CANDIDATE_KEYS),
            Some(first) => value_to_string(first).and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }),
            None => None,
        },
        _ => None,
    }
}

fn extract_eth_tx_hash_query_param(params: &serde_json::Value) -> Option<String> {
    const CANDIDATE_KEYS: &[&str] = &["tx_hash", "txHash", "transaction_hash", "hash"];
    match params {
        serde_json::Value::Object(map) => pick_first_nonempty_string(map, CANDIDATE_KEYS),
        serde_json::Value::Array(arr) => match arr.first() {
            Some(serde_json::Value::Object(map)) => pick_first_nonempty_string(map, CANDIDATE_KEYS),
            Some(first) => value_to_string(first).and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }),
            None => None,
        },
        _ => None,
    }
}

fn extract_eth_persona_address_param(params: &serde_json::Value) -> Option<String> {
    const CANDIDATE_KEYS: &[&str] = &["external_address", "from", "address"];
    match params {
        serde_json::Value::Object(map) => {
            if let Some(found) = pick_first_nonempty_string(map, CANDIDATE_KEYS) {
                return Some(found);
            }
            if let Some(serde_json::Value::Object(tx_obj)) = map.get("tx") {
                if let Some(found) = pick_first_nonempty_string(tx_obj, CANDIDATE_KEYS) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => match arr.first() {
            Some(serde_json::Value::Object(map)) => {
                if let Some(found) = pick_first_nonempty_string(map, CANDIDATE_KEYS) {
                    return Some(found);
                }
                if let Some(serde_json::Value::Object(tx_obj)) = map.get("tx") {
                    return pick_first_nonempty_string(tx_obj, CANDIDATE_KEYS);
                }
                None
            }
            Some(first) => value_to_string(first).and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }),
            None => None,
        },
        _ => None,
    }
}

fn decode_hex_bytes(raw: &str, field: &str) -> Result<Vec<u8>> {
    let trimmed = raw.trim();
    let normalized = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if normalized.is_empty() {
        bail!("{} is empty", field);
    }
    if !normalized.len().is_multiple_of(2) {
        bail!("{} must have even hex length", field);
    }
    if !normalized.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("{} must be hex", field);
    }
    let mut out = Vec::with_capacity(normalized.len() / 2);
    let bytes = normalized.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let pair = std::str::from_utf8(&bytes[idx..idx + 2])
            .with_context(|| format!("{} contains invalid utf8", field))?;
        let v = u8::from_str_radix(pair, 16)
            .with_context(|| format!("{} contains invalid hex byte {}", field, pair))?;
        out.push(v);
        idx += 2;
    }
    Ok(out)
}

fn to_hex(raw: &[u8]) -> String {
    let mut out = String::with_capacity(raw.len() * 2);
    for b in raw {
        out.push_str(&format!("{:02x}", b));
    }
    out
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

fn string_env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn u64_env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

fn u32_env_allow_zero(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(default)
}

fn now_unix_sec() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}

fn now_unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis()
}
