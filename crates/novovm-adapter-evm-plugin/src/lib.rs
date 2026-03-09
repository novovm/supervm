use novovm_adapter_api::{
    AccountAuditEvent, AccountRole, ChainConfig, ChainType, PersonaAddress, PersonaType,
    ProtocolKind, RouteRequest, StateIR, TxIR, TxType, UnifiedAccountError, UnifiedAccountRouter,
};
use novovm_adapter_evm_core::{
    active_precompile_set_m0, resolve_evm_profile, validate_tx_semantics_m0,
};
use novovm_adapter_novovm::create_native_adapter;
use rocksdb::Options as RocksDbOptions;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const NOVOVM_ADAPTER_PLUGIN_ABI_V1: u32 = 1;
pub const NOVOVM_ADAPTER_PLUGIN_CAP_APPLY_IR_V1: u64 = 0x1;
pub const NOVOVM_ADAPTER_PLUGIN_CAP_UA_SELF_GUARD_V1: u64 = 0x2;
pub const NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_UA_SELF_GUARD_V1: u64 = 0x1;
// Stable FFI return codes (external contract):
//  0=ok, -1=invalid_arg, -2=unsupported_chain, -3=decode_failed,
// -4=empty_batch, -5=unsupported_tx_type, -6=apply_failed,
// -7=ua_self_guard_failed, -8=payload_too_large, -9=batch_too_large.
pub const NOVOVM_ADAPTER_PLUGIN_RC_OK: i32 = 0;
pub const NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG: i32 = -1;
pub const NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_CHAIN: i32 = -2;
pub const NOVOVM_ADAPTER_PLUGIN_RC_DECODE_FAILED: i32 = -3;
pub const NOVOVM_ADAPTER_PLUGIN_RC_EMPTY_BATCH: i32 = -4;
pub const NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_TX_TYPE: i32 = -5;
pub const NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED: i32 = -6;
pub const NOVOVM_ADAPTER_PLUGIN_RC_UA_SELF_GUARD_FAILED: i32 = -7;
pub const NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE: i32 = -8;
pub const NOVOVM_ADAPTER_PLUGIN_RC_BATCH_TOO_LARGE: i32 = -9;

const UA_PLUGIN_STORE_VERSION_V1: u32 = 1;
const UA_PLUGIN_STORE_KEY_V1: &[u8] = b"ua_plugin:store:router:v1";
const UA_PLUGIN_AUDIT_HEAD_KEY_V1: &[u8] = b"ua_plugin:audit:head:v1";
const UA_PLUGIN_AUDIT_SEQ_KEY_PREFIX_V1: &str = "ua_plugin:audit:seq:v1:";
const UA_PLUGIN_ARTIFACTS_SUBDIR: &str = "artifacts/migration/unifiedaccount";
const MAX_PLUGIN_TX_IR_BYTES: usize = 16 * 1024 * 1024;
const MAX_PLUGIN_TX_COUNT: usize = 100_000;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NovovmAdapterPluginApplyResultV1 {
    pub verified: u8,
    pub applied: u8,
    pub txs: u64,
    pub accounts: u64,
    pub state_root: [u8; 32],
    pub error_code: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NovovmAdapterPluginApplyOptionsV1 {
    pub flags: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UaPluginStoreBackend {
    Memory,
    BincodeFile,
    Rocksdb,
}

impl UaPluginStoreBackend {
    fn as_str(self) -> &'static str {
        match self {
            UaPluginStoreBackend::Memory => "memory",
            UaPluginStoreBackend::BincodeFile => "bincode_file",
            UaPluginStoreBackend::Rocksdb => "rocksdb",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UaPluginAuditBackend {
    None,
    Jsonl,
    Rocksdb,
}

impl UaPluginAuditBackend {
    fn as_str(self) -> &'static str {
        match self {
            UaPluginAuditBackend::None => "none",
            UaPluginAuditBackend::Jsonl => "jsonl",
            UaPluginAuditBackend::Rocksdb => "rocksdb",
        }
    }
}

#[derive(Debug)]
struct UaPluginStandaloneConfig {
    store_backend: UaPluginStoreBackend,
    store_path: PathBuf,
    audit_backend: UaPluginAuditBackend,
    audit_path: PathBuf,
}

#[derive(Debug, Default)]
struct UaPluginRuntime {
    router: UnifiedAccountRouter,
    audit_seq: u64,
}

#[derive(Debug, Deserialize)]
struct UaPluginStoreEnvelopeV1 {
    version: u32,
    router: UnifiedAccountRouter,
    audit_seq: u64,
}

#[derive(Debug, Serialize)]
struct UaPluginStoreEnvelopeRefV1<'a> {
    version: u32,
    router: &'a UnifiedAccountRouter,
    audit_seq: u64,
}

#[derive(Debug, Serialize)]
struct UaPluginAuditRecordV1 {
    seq: u64,
    at: u64,
    source: String,
    chain_id: u64,
    tx_count: usize,
    success: bool,
    error: Option<String>,
    store_backend: String,
    audit_backend: String,
    events: Vec<AccountAuditEvent>,
}

static UA_PLUGIN_RUNTIME: OnceLock<Mutex<UaPluginRuntime>> = OnceLock::new();
static UA_PLUGIN_STANDALONE_CONFIG: OnceLock<UaPluginStandaloneConfig> = OnceLock::new();

fn normalize_root32(root: &[u8]) -> [u8; 32] {
    if root.len() == 32 {
        let mut out = [0u8; 32];
        out.copy_from_slice(root);
        return out;
    }
    let mut hasher = Sha256::new();
    hasher.update(root);
    hasher.finalize().into()
}

fn now_unix_sec() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs()
}

fn to_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn derive_primary_key_ref(uca_id: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"ua-plugin-self-guard-primary-key-ref-v1");
    hasher.update(uca_id.as_bytes());
    hasher.finalize().to_vec()
}

fn current_workdir_or_dot() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn default_plugin_store_path(backend: UaPluginStoreBackend) -> PathBuf {
    let base = current_workdir_or_dot().join(UA_PLUGIN_ARTIFACTS_SUBDIR);
    match backend {
        UaPluginStoreBackend::Memory => PathBuf::new(),
        UaPluginStoreBackend::BincodeFile => base.join("ua-plugin-self-guard-router.bin"),
        UaPluginStoreBackend::Rocksdb => base.join("ua-plugin-self-guard-router.rocksdb"),
    }
}

fn default_plugin_audit_path(backend: UaPluginAuditBackend) -> PathBuf {
    let base = current_workdir_or_dot().join(UA_PLUGIN_ARTIFACTS_SUBDIR);
    match backend {
        UaPluginAuditBackend::None => PathBuf::new(),
        UaPluginAuditBackend::Jsonl => base.join("ua-plugin-self-guard-audit.jsonl"),
        UaPluginAuditBackend::Rocksdb => base.join("ua-plugin-self-guard-audit.rocksdb"),
    }
}

fn parse_store_backend(raw: &str) -> UaPluginStoreBackend {
    match raw.trim().to_ascii_lowercase().as_str() {
        "memory" | "" => UaPluginStoreBackend::Memory,
        "bincode_file" | "bincode" | "file" => UaPluginStoreBackend::BincodeFile,
        "rocksdb" => UaPluginStoreBackend::Rocksdb,
        _ => UaPluginStoreBackend::Memory,
    }
}

fn parse_audit_backend(raw: &str) -> UaPluginAuditBackend {
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" | "" => UaPluginAuditBackend::None,
        "jsonl" => UaPluginAuditBackend::Jsonl,
        "rocksdb" => UaPluginAuditBackend::Rocksdb,
        _ => UaPluginAuditBackend::None,
    }
}

fn resolve_ua_plugin_standalone_config() -> &'static UaPluginStandaloneConfig {
    UA_PLUGIN_STANDALONE_CONFIG.get_or_init(|| {
        let store_backend = parse_store_backend(
            &std::env::var("NOVOVM_ADAPTER_PLUGIN_UA_STORE_BACKEND")
                .unwrap_or_else(|_| "memory".to_string()),
        );
        let store_path = std::env::var("NOVOVM_ADAPTER_PLUGIN_UA_STORE_PATH")
            .ok()
            .map(|raw| raw.trim().to_string())
            .filter(|raw| !raw.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| default_plugin_store_path(store_backend));

        let audit_backend = parse_audit_backend(
            &std::env::var("NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_BACKEND")
                .unwrap_or_else(|_| "none".to_string()),
        );
        let audit_path = std::env::var("NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_PATH")
            .ok()
            .map(|raw| raw.trim().to_string())
            .filter(|raw| !raw.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| default_plugin_audit_path(audit_backend));

        UaPluginStandaloneConfig {
            store_backend,
            store_path,
            audit_backend,
            audit_path,
        }
    })
}

fn open_rocksdb(path: &Path) -> anyhow::Result<rocksdb::DB> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("create rocksdb parent dir failed: {e}"))?;
    }
    let mut opts = RocksDbOptions::default();
    opts.create_if_missing(true);
    rocksdb::DB::open(&opts, path)
        .map_err(|e| anyhow::anyhow!("open rocksdb failed: {} ({})", path.display(), e))
}

fn decode_store_envelope(raw: &[u8]) -> anyhow::Result<UaPluginStoreEnvelopeV1> {
    if let Ok(envelope) = bincode::deserialize::<UaPluginStoreEnvelopeV1>(raw) {
        if envelope.version == UA_PLUGIN_STORE_VERSION_V1 {
            return Ok(envelope);
        }
        anyhow::bail!(
            "unsupported ua plugin store envelope version={}",
            envelope.version
        );
    }

    // Backward compatibility: older payload persisted router directly.
    let router: UnifiedAccountRouter = bincode::deserialize(raw).map_err(|e| {
        anyhow::anyhow!("decode ua plugin store envelope failed (router fallback): {e}")
    })?;
    Ok(UaPluginStoreEnvelopeV1 {
        version: 0,
        router,
        audit_seq: 0,
    })
}

fn load_runtime_from_bincode_file(path: &Path) -> anyhow::Result<Option<UaPluginRuntime>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read(path).map_err(|e| {
        anyhow::anyhow!(
            "read ua plugin store file failed: {} ({})",
            path.display(),
            e
        )
    })?;
    let envelope = decode_store_envelope(&raw)?;
    Ok(Some(UaPluginRuntime {
        router: envelope.router,
        audit_seq: envelope.audit_seq,
    }))
}

fn load_runtime_from_rocksdb(path: &Path) -> anyhow::Result<Option<UaPluginRuntime>> {
    let db = open_rocksdb(path)?;
    let raw = db
        .get(UA_PLUGIN_STORE_KEY_V1)
        .map_err(|e| anyhow::anyhow!("rocksdb read ua plugin store failed: {}", e))?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let envelope = decode_store_envelope(&raw)?;
    Ok(Some(UaPluginRuntime {
        router: envelope.router,
        audit_seq: envelope.audit_seq,
    }))
}

fn load_runtime_from_store(config: &UaPluginStandaloneConfig) -> anyhow::Result<UaPluginRuntime> {
    let runtime = match config.store_backend {
        UaPluginStoreBackend::Memory => None,
        UaPluginStoreBackend::BincodeFile => load_runtime_from_bincode_file(&config.store_path)?,
        UaPluginStoreBackend::Rocksdb => load_runtime_from_rocksdb(&config.store_path)?,
    };
    Ok(runtime.unwrap_or_default())
}

fn save_runtime_to_bincode_file(path: &Path, runtime: &UaPluginRuntime) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("create ua plugin store dir failed: {e}"))?;
    }
    let envelope = UaPluginStoreEnvelopeRefV1 {
        version: UA_PLUGIN_STORE_VERSION_V1,
        router: &runtime.router,
        audit_seq: runtime.audit_seq,
    };
    let payload = bincode::serialize(&envelope)
        .map_err(|e| anyhow::anyhow!("encode ua plugin store payload failed: {e}"))?;
    fs::write(path, payload).map_err(|e| {
        anyhow::anyhow!(
            "write ua plugin store file failed: {} ({})",
            path.display(),
            e
        )
    })
}

fn save_runtime_to_rocksdb(path: &Path, runtime: &UaPluginRuntime) -> anyhow::Result<()> {
    let db = open_rocksdb(path)?;
    let envelope = UaPluginStoreEnvelopeRefV1 {
        version: UA_PLUGIN_STORE_VERSION_V1,
        router: &runtime.router,
        audit_seq: runtime.audit_seq,
    };
    let payload = bincode::serialize(&envelope)
        .map_err(|e| anyhow::anyhow!("encode ua plugin store payload failed: {e}"))?;
    db.put(UA_PLUGIN_STORE_KEY_V1, payload)
        .map_err(|e| anyhow::anyhow!("rocksdb write ua plugin store failed: {}", e))
}

fn save_runtime_to_store(
    config: &UaPluginStandaloneConfig,
    runtime: &UaPluginRuntime,
) -> anyhow::Result<()> {
    match config.store_backend {
        UaPluginStoreBackend::Memory => Ok(()),
        UaPluginStoreBackend::BincodeFile => {
            save_runtime_to_bincode_file(&config.store_path, runtime)
        }
        UaPluginStoreBackend::Rocksdb => save_runtime_to_rocksdb(&config.store_path, runtime),
    }
}

fn parse_u64_be(raw: &[u8]) -> Option<u64> {
    if raw.len() != 8 {
        return None;
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(raw);
    Some(u64::from_be_bytes(buf))
}

fn append_audit_jsonl(path: &Path, record: &UaPluginAuditRecordV1) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("create ua plugin audit dir failed: {e}"))?;
    }
    let payload = serde_json::to_string(record)
        .map_err(|e| anyhow::anyhow!("serialize ua plugin audit record failed: {e}"))?;
    let mut writer = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| {
            anyhow::anyhow!(
                "open ua plugin audit jsonl failed: {} ({})",
                path.display(),
                e
            )
        })?;
    writer
        .write_all(payload.as_bytes())
        .and_then(|_| writer.write_all(b"\n"))
        .map_err(|e| {
            anyhow::anyhow!(
                "append ua plugin audit jsonl failed: {} ({})",
                path.display(),
                e
            )
        })
}

fn append_audit_rocksdb(path: &Path, record: &UaPluginAuditRecordV1) -> anyhow::Result<()> {
    let db = open_rocksdb(path)?;
    let key = format!("{}{:020}", UA_PLUGIN_AUDIT_SEQ_KEY_PREFIX_V1, record.seq);
    let payload = serde_json::to_vec(record)
        .map_err(|e| anyhow::anyhow!("serialize ua plugin audit record failed: {e}"))?;
    db.put(key.as_bytes(), payload)
        .map_err(|e| anyhow::anyhow!("rocksdb write ua plugin audit record failed: {}", e))?;
    db.put(UA_PLUGIN_AUDIT_HEAD_KEY_V1, record.seq.to_be_bytes())
        .map_err(|e| anyhow::anyhow!("rocksdb write ua plugin audit head failed: {}", e))
}

fn append_plugin_audit_record(
    config: &UaPluginStandaloneConfig,
    runtime: &mut UaPluginRuntime,
    chain_id: u64,
    tx_count: usize,
    success: bool,
    error: Option<&str>,
    events: Vec<AccountAuditEvent>,
) -> anyhow::Result<()> {
    if config.audit_backend == UaPluginAuditBackend::None {
        return Ok(());
    }

    runtime.audit_seq = runtime.audit_seq.saturating_add(1);
    let record = UaPluginAuditRecordV1 {
        seq: runtime.audit_seq,
        at: now_unix_sec(),
        source: "plugin_self_guard".to_string(),
        chain_id,
        tx_count,
        success,
        error: error.map(ToOwned::to_owned),
        store_backend: config.store_backend.as_str().to_string(),
        audit_backend: config.audit_backend.as_str().to_string(),
        events,
    };

    match config.audit_backend {
        UaPluginAuditBackend::None => Ok(()),
        UaPluginAuditBackend::Jsonl => append_audit_jsonl(&config.audit_path, &record),
        UaPluginAuditBackend::Rocksdb => append_audit_rocksdb(&config.audit_path, &record),
    }
}

fn reconcile_audit_seq_from_backend(
    config: &UaPluginStandaloneConfig,
    runtime: &mut UaPluginRuntime,
) -> anyhow::Result<()> {
    if runtime.audit_seq != 0 || config.audit_backend != UaPluginAuditBackend::Rocksdb {
        return Ok(());
    }
    let db = open_rocksdb(&config.audit_path)?;
    let head = db
        .get(UA_PLUGIN_AUDIT_HEAD_KEY_V1)
        .map_err(|e| anyhow::anyhow!("rocksdb read ua plugin audit head failed: {}", e))?;
    if let Some(head) = head.and_then(|raw| parse_u64_be(&raw)) {
        runtime.audit_seq = head;
    }
    Ok(())
}

fn ua_plugin_runtime(
    config: &UaPluginStandaloneConfig,
) -> anyhow::Result<&'static Mutex<UaPluginRuntime>> {
    if let Some(runtime) = UA_PLUGIN_RUNTIME.get() {
        return Ok(runtime);
    }
    let mut runtime = load_runtime_from_store(config)?;
    reconcile_audit_seq_from_backend(config, &mut runtime)?;
    let _ = UA_PLUGIN_RUNTIME.set(Mutex::new(runtime));
    UA_PLUGIN_RUNTIME
        .get()
        .ok_or_else(|| anyhow::anyhow!("initialize ua plugin runtime failed"))
}

fn map_create_uca_error(err: UnifiedAccountError) -> anyhow::Result<()> {
    match err {
        UnifiedAccountError::UcaAlreadyExists { .. } => Ok(()),
        other => Err(anyhow::anyhow!(
            "plugin ua self-guard create_uca failed: {}",
            other
        )),
    }
}

fn route_txs_via_plugin_ua_self_guard(chain_id: u64, txs: &[TxIR]) -> anyhow::Result<()> {
    let config = resolve_ua_plugin_standalone_config();
    let runtime = ua_plugin_runtime(config)?;
    let mut runtime = runtime
        .lock()
        .map_err(|_| anyhow::anyhow!("plugin ua self-guard mutex poisoned"))?;
    let base_now = now_unix_sec();
    let mut route_error: Option<anyhow::Error> = None;

    for (idx, tx) in txs.iter().enumerate() {
        if tx.from.is_empty() {
            route_error = Some(anyhow::anyhow!(
                "plugin ua self-guard requires non-empty tx.from"
            ));
            break;
        }
        let now = base_now.saturating_add(idx as u64);
        let persona = PersonaAddress {
            persona_type: PersonaType::Evm,
            chain_id,
            external_address: tx.from.clone(),
        };
        let uca_id = format!("uca:plugin:{}:{}", chain_id, to_lower_hex(&tx.from));
        if let Err(err) =
            runtime
                .router
                .create_uca(uca_id.clone(), derive_primary_key_ref(&uca_id), now)
        {
            if let Err(mapped) = map_create_uca_error(err) {
                route_error = Some(mapped);
                break;
            }
        }

        match runtime.router.resolve_binding_owner(&persona) {
            Some(owner) if owner == uca_id => {}
            Some(owner) => {
                route_error = Some(anyhow::anyhow!(
                    "plugin ua self-guard binding conflict: owner={} expected={}",
                    owner,
                    uca_id
                ));
                break;
            }
            None => {
                if let Err(err) =
                    runtime
                        .router
                        .add_binding(&uca_id, AccountRole::Owner, persona.clone(), now)
                {
                    route_error = Some(anyhow::anyhow!(
                        "plugin ua self-guard add_binding failed: {}",
                        err
                    ));
                    break;
                }
            }
        }

        let request = RouteRequest {
            uca_id,
            persona,
            role: AccountRole::Owner,
            protocol: ProtocolKind::Eth,
            signature_domain: format!("evm:{}", chain_id),
            nonce: tx.nonce,
            wants_cross_chain_atomic: false,
            tx_type4: false,
            session_expires_at: None,
            now,
        };
        if let Err(err) = runtime.router.route(request) {
            route_error = Some(anyhow::anyhow!(
                "plugin ua self-guard route failed: {}",
                err
            ));
            break;
        }
    }

    let events = runtime.router.take_events();
    let success = route_error.is_none();
    let error_text = route_error.as_ref().map(|err| err.to_string());
    append_plugin_audit_record(
        config,
        &mut runtime,
        chain_id,
        txs.len(),
        success,
        error_text.as_deref(),
        events,
    )?;
    save_runtime_to_store(config, &runtime)?;

    if let Some(err) = route_error {
        return Err(err);
    }
    Ok(())
}

fn chain_type_from_code(code: u32) -> Option<ChainType> {
    Some(match code {
        1 => ChainType::EVM,
        5 => ChainType::Polygon,
        6 => ChainType::BNB,
        7 => ChainType::Avalanche,
        _ => return None,
    })
}

fn decode_plugin_apply_inputs(
    chain_type_code: u32,
    tx_ir_ptr: *const u8,
    tx_ir_len: usize,
) -> Result<(ChainType, Vec<TxIR>), i32> {
    if tx_ir_ptr.is_null() || tx_ir_len == 0 {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG);
    }
    if tx_ir_len > MAX_PLUGIN_TX_IR_BYTES || tx_ir_len > isize::MAX as usize {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE);
    }

    let chain_type = match chain_type_from_code(chain_type_code) {
        Some(v) => v,
        None => return Err(NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_CHAIN),
    };

    let tx_bytes = unsafe { std::slice::from_raw_parts(tx_ir_ptr, tx_ir_len) };
    let txs: Vec<TxIR> = match bincode::deserialize(tx_bytes) {
        Ok(v) => v,
        Err(_) => return Err(NOVOVM_ADAPTER_PLUGIN_RC_DECODE_FAILED),
    };
    if txs.is_empty() {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_EMPTY_BATCH);
    }
    if txs.len() > MAX_PLUGIN_TX_COUNT {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_BATCH_TOO_LARGE);
    }
    if !txs.iter().all(|tx| {
        matches!(
            tx.tx_type,
            TxType::Transfer | TxType::ContractCall | TxType::ContractDeploy
        )
    }) {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_TX_TYPE);
    }
    Ok((chain_type, txs))
}

fn apply_ir_batch(
    chain_type: ChainType,
    chain_id: u64,
    txs: &[TxIR],
) -> anyhow::Result<NovovmAdapterPluginApplyResultV1> {
    let profile = resolve_evm_profile(chain_type, chain_id)?;
    let _active_precompiles = active_precompile_set_m0(&profile);

    let config = ChainConfig {
        chain_type,
        chain_id,
        name: format!("evm-plugin-{}", chain_type.as_str()),
        enabled: true,
        custom_config: None,
    };

    let mut adapter = create_native_adapter(config)?;
    adapter.initialize()?;

    let mut state = StateIR::new();
    let mut verified = true;
    let mut applied = true;
    for tx in txs {
        validate_tx_semantics_m0(&profile, tx)?;
        let tx_ok = adapter.verify_transaction(tx)?;
        verified = verified && tx_ok;
        if tx_ok {
            adapter.execute_transaction(tx, &mut state)?;
        } else {
            applied = false;
        }
    }

    let state_root = adapter.state_root()?;
    let accounts = state.accounts.len() as u64;
    adapter.shutdown()?;

    Ok(NovovmAdapterPluginApplyResultV1 {
        verified: u8::from(verified),
        applied: u8::from(applied),
        txs: txs.len() as u64,
        accounts,
        state_root: normalize_root32(&state_root),
        error_code: 0,
    })
}

#[no_mangle]
pub extern "C" fn novovm_adapter_plugin_version() -> u32 {
    NOVOVM_ADAPTER_PLUGIN_ABI_V1
}

#[no_mangle]
pub extern "C" fn novovm_adapter_plugin_capabilities() -> u64 {
    NOVOVM_ADAPTER_PLUGIN_CAP_APPLY_IR_V1 | NOVOVM_ADAPTER_PLUGIN_CAP_UA_SELF_GUARD_V1
}

#[no_mangle]
/// Apply a serialized TxIR batch through the EVM adapter plugin (ABI v1).
///
/// # Safety
/// - `tx_ir_ptr` must be valid for reads of `tx_ir_len` bytes for the duration of this call.
/// - `out_result` must be a valid, writable pointer to `NovovmAdapterPluginApplyResultV1`.
/// - The memory behind `tx_ir_ptr` must contain a bincode-encoded `Vec<TxIR>`.
pub unsafe extern "C" fn novovm_adapter_plugin_apply_v1(
    chain_type_code: u32,
    chain_id: u64,
    tx_ir_ptr: *const u8,
    tx_ir_len: usize,
    out_result: *mut NovovmAdapterPluginApplyResultV1,
) -> i32 {
    if out_result.is_null() {
        return NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG;
    }
    let (chain_type, txs) = match decode_plugin_apply_inputs(chain_type_code, tx_ir_ptr, tx_ir_len)
    {
        Ok(v) => v,
        Err(rc) => return rc,
    };

    let result = match apply_ir_batch(chain_type, chain_id, &txs) {
        Ok(v) => v,
        Err(_) => return NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED,
    };

    *out_result = result;
    NOVOVM_ADAPTER_PLUGIN_RC_OK
}

#[no_mangle]
/// Apply a serialized TxIR batch through the EVM adapter plugin (ABI v2 with options).
///
/// # Safety
/// - `tx_ir_ptr` must be valid for reads of `tx_ir_len` bytes for the duration of this call.
/// - `options_ptr` may be null; when non-null it must point to a valid `NovovmAdapterPluginApplyOptionsV1`.
/// - `out_result` must be a valid, writable pointer to `NovovmAdapterPluginApplyResultV1`.
/// - The memory behind `tx_ir_ptr` must contain a bincode-encoded `Vec<TxIR>`.
pub unsafe extern "C" fn novovm_adapter_plugin_apply_v2(
    chain_type_code: u32,
    chain_id: u64,
    tx_ir_ptr: *const u8,
    tx_ir_len: usize,
    options_ptr: *const NovovmAdapterPluginApplyOptionsV1,
    out_result: *mut NovovmAdapterPluginApplyResultV1,
) -> i32 {
    if out_result.is_null() {
        return NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG;
    }
    let (chain_type, txs) = match decode_plugin_apply_inputs(chain_type_code, tx_ir_ptr, tx_ir_len)
    {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    let flags = if options_ptr.is_null() {
        0
    } else {
        (*options_ptr).flags
    };
    if flags & NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_UA_SELF_GUARD_V1 != 0
        && route_txs_via_plugin_ua_self_guard(chain_id, &txs).is_err()
    {
        return NOVOVM_ADAPTER_PLUGIN_RC_UA_SELF_GUARD_FAILED;
    }

    let result = match apply_ir_batch(chain_type, chain_id, &txs) {
        Ok(v) => v,
        Err(_) => return NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED,
    };

    *out_result = result;
    NOVOVM_ADAPTER_PLUGIN_RC_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_adapter_novovm::{address_from_seed_v1, signature_payload_with_seed_v1};
    const TEST_SIGN_SEED: [u8; 32] = [13u8; 32];

    fn encode_address(seed: u64) -> Vec<u8> {
        let mut out = vec![0u8; 20];
        out[12..20].copy_from_slice(&seed.to_be_bytes());
        out
    }

    fn sample_tx(chain_id: u64, nonce: u64) -> TxIR {
        let mut tx = TxIR {
            hash: Vec::new(),
            from: address_from_seed_v1(TEST_SIGN_SEED),
            to: Some(encode_address(2000)),
            value: 5,
            gas_limit: 21_000,
            gas_price: 1,
            nonce,
            data: Vec::new(),
            signature: Vec::new(),
            chain_id,
            tx_type: TxType::Transfer,
            source_chain: None,
            target_chain: None,
        };
        tx.compute_hash();
        tx.signature = signature_payload_with_seed_v1(&tx, TEST_SIGN_SEED);
        tx
    }

    fn sample_contract_call_tx(chain_id: u64, nonce: u64) -> TxIR {
        let mut tx = sample_tx(chain_id, nonce);
        tx.tx_type = TxType::ContractCall;
        tx.data = vec![0xab, 0xcd, 0xef];
        tx.gas_limit = 24_000;
        tx.compute_hash();
        tx.signature = signature_payload_with_seed_v1(&tx, TEST_SIGN_SEED);
        tx
    }

    fn sample_contract_deploy_tx(chain_id: u64, nonce: u64) -> TxIR {
        let mut tx = sample_tx(chain_id, nonce);
        tx.tx_type = TxType::ContractDeploy;
        tx.to = None;
        tx.data = vec![0x60, 0x00, 0x60, 0x00];
        tx.gas_limit = 60_000;
        tx.compute_hash();
        tx.signature = signature_payload_with_seed_v1(&tx, TEST_SIGN_SEED);
        tx
    }

    #[test]
    fn apply_ir_batch_smoke_for_evm_chain() {
        let txs = vec![sample_tx(1, 0), sample_tx(1, 1)];
        let result = apply_ir_batch(ChainType::EVM, 1, &txs).expect("apply should pass");
        assert_eq!(result.verified, 1);
        assert_eq!(result.applied, 1);
        assert_eq!(result.txs, 2);
        assert!(result.accounts >= 2);
    }

    #[test]
    fn apply_ir_batch_rejects_intrinsic_gas_too_low() {
        let mut tx = sample_tx(1, 0);
        tx.gas_limit = 20_999;
        let err = apply_ir_batch(ChainType::EVM, 1, &[tx]).expect_err("must reject low gas");
        assert!(err.to_string().contains("intrinsic gas too low"));
    }

    #[test]
    fn apply_ir_batch_accepts_contract_call_and_deploy() {
        let txs = vec![
            sample_contract_call_tx(1, 0),
            sample_contract_deploy_tx(1, 1),
        ];
        let result = apply_ir_batch(ChainType::EVM, 1, &txs).expect("apply should pass");
        assert_eq!(result.verified, 1);
        assert_eq!(result.applied, 1);
        assert_eq!(result.txs, 2);
        assert!(result.accounts >= 2);
    }

    #[test]
    fn chain_code_mapping_supports_only_evm_family() {
        assert_eq!(chain_type_from_code(1), Some(ChainType::EVM));
        assert_eq!(chain_type_from_code(5), Some(ChainType::Polygon));
        assert_eq!(chain_type_from_code(6), Some(ChainType::BNB));
        assert_eq!(chain_type_from_code(7), Some(ChainType::Avalanche));
        assert_eq!(chain_type_from_code(0), None);
        assert_eq!(chain_type_from_code(13), None);
    }

    #[test]
    fn plugin_capabilities_include_ua_self_guard_contract_bit() {
        let caps = novovm_adapter_plugin_capabilities();
        assert!(caps & NOVOVM_ADAPTER_PLUGIN_CAP_APPLY_IR_V1 != 0);
        assert!(caps & NOVOVM_ADAPTER_PLUGIN_CAP_UA_SELF_GUARD_V1 != 0);
    }

    #[test]
    fn plugin_return_codes_are_stable_contract() {
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_OK, 0);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG, -1);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_CHAIN, -2);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_DECODE_FAILED, -3);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_EMPTY_BATCH, -4);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_TX_TYPE, -5);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED, -6);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_UA_SELF_GUARD_FAILED, -7);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE, -8);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_BATCH_TOO_LARGE, -9);
    }

    #[test]
    fn plugin_apply_v1_rejects_invalid_chain_type() {
        let txs = vec![sample_tx(1, 0)];
        let tx_bytes = bincode::serialize(&txs).expect("tx encode");
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                0,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_CHAIN);
    }

    #[test]
    fn plugin_apply_v1_rejects_malformed_payload() {
        let payload = [1u8, 2u8, 3u8];
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                1,
                1,
                payload.as_ptr(),
                payload.len(),
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_DECODE_FAILED);
    }

    #[test]
    fn plugin_apply_v1_rejects_empty_batch() {
        let txs: Vec<TxIR> = Vec::new();
        let tx_bytes = bincode::serialize(&txs).expect("tx encode");
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                1,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_EMPTY_BATCH);
    }

    #[test]
    fn plugin_apply_v1_rejects_unsupported_tx_type() {
        let mut tx = sample_tx(1, 0);
        tx.tx_type = TxType::Privacy;
        let tx_bytes = bincode::serialize(&vec![tx]).expect("tx encode");
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                1,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_TX_TYPE);
    }

    #[test]
    fn plugin_apply_v1_maps_engine_failure_to_apply_failed() {
        let mut tx = sample_tx(1, 0);
        tx.gas_limit = 20_999;
        let tx_bytes = bincode::serialize(&vec![tx]).expect("tx encode");
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                1,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED);
    }

    #[test]
    fn plugin_apply_v2_self_guard_rejects_replay_nonce() {
        let txs = vec![sample_tx(1, 0)];
        let tx_bytes = bincode::serialize(&txs).expect("tx encode");
        let options = NovovmAdapterPluginApplyOptionsV1 {
            flags: NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_UA_SELF_GUARD_V1,
        };
        let mut out = NovovmAdapterPluginApplyResultV1::default();

        let rc_first = unsafe {
            novovm_adapter_plugin_apply_v2(
                1,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &options as *const NovovmAdapterPluginApplyOptionsV1,
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc_first, NOVOVM_ADAPTER_PLUGIN_RC_OK);

        let rc_second = unsafe {
            novovm_adapter_plugin_apply_v2(
                1,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &options as *const NovovmAdapterPluginApplyOptionsV1,
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc_second, NOVOVM_ADAPTER_PLUGIN_RC_UA_SELF_GUARD_FAILED);
    }

    #[test]
    fn plugin_apply_v1_rejects_oversized_payload_before_decode() {
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let marker = [0u8; 1];
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                1,
                1,
                marker.as_ptr(),
                MAX_PLUGIN_TX_IR_BYTES + 1,
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE);
    }
}
