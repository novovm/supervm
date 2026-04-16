use anyhow::{bail, Result};
use aoem_bindings::{AoemCreateOptionsV1, AoemDyn, AoemExecV2Result, AoemHandle, AoemOpV2};
use novovm_adapter_api::{ChainType, TxType};
use serde::{Deserialize, Serialize};
use sha2::{Digest as Sha2Digest, Sha256};
use sha3::Keccak256;
use std::path::{Path, PathBuf};
use std::time::Instant;

mod ingress_codec;

pub const AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1: &str = "novovm-exec/v1";

#[derive(Clone, Debug, Default)]
pub struct AoemExecOpenOptions {
    pub ingress_workers: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AoemRuntimeVariant {
    Core,
    Persist,
    Wasm,
}

impl AoemRuntimeVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Persist => "persist",
            Self::Wasm => "wasm",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "core" => Some(Self::Core),
            "persist" => Some(Self::Persist),
            "wasm" => Some(Self::Wasm),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AoemRuntimeConfig {
    pub variant: AoemRuntimeVariant,
    pub aoem_root: PathBuf,
    pub dll_path: PathBuf,
    pub manifest_path: PathBuf,
    pub runtime_profile_path: PathBuf,
    pub plugin_dir: Option<PathBuf>,
    pub persist_backend: String,
    pub wasm_runtime: String,
    pub zkvm_mode: String,
    pub mldsa_mode: String,
    pub ingress_workers: Option<u32>,
}

fn find_aoem_root_near(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let candidate = dir.join("aoem");
        if default_manifest_path(&candidate).exists() {
            return Some(candidate);
        }
        if dynlib_names_by_preference().iter().any(|name| {
            platform_roots_in_priority(&candidate).iter().any(|r| {
                r.join("core").join("bin").join(name).exists() || r.join("bin").join(name).exists()
            })
        }) {
            return Some(candidate);
        }
    }
    None
}

fn default_aoem_root() -> PathBuf {
    if let Ok(current_dir) = std::env::current_dir() {
        if let Some(found) = find_aoem_root_near(&current_dir) {
            return found;
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(found) = find_aoem_root_near(&manifest_dir) {
        return found;
    }

    manifest_dir.join("..").join("..").join("aoem")
}

impl AoemRuntimeConfig {
    pub fn from_env() -> Result<Self> {
        let variant_raw = std::env::var("NOVOVM_AOEM_VARIANT")
            .or_else(|_| std::env::var("AOEM_VARIANT"))
            .unwrap_or_else(|_| "core".to_string());
        let Some(variant) = AoemRuntimeVariant::parse(&variant_raw) else {
            bail!("invalid AOEM variant: {variant_raw}; valid: core|persist|wasm");
        };

        let aoem_root = std::env::var("NOVOVM_AOEM_ROOT")
            .or_else(|_| std::env::var("AOEM_ROOT"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_aoem_root());

        let dll_path = std::env::var("NOVOVM_AOEM_DLL")
            .or_else(|_| std::env::var("AOEM_DLL"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_dll_path(&aoem_root, variant));

        let plugin_dir = std::env::var("NOVOVM_AOEM_PLUGIN_DIR")
            .or_else(|_| std::env::var("AOEM_FFI_PLUGIN_DIR"))
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                let dirs = plugin_dirs_from_env_list();
                pick_plugin_dir_from_candidates(variant, &dirs)
            })
            .or_else(|| default_plugin_dir(&aoem_root, variant));

        let persist_backend = std::env::var("NOVOVM_AOEM_PERSIST_BACKEND")
            .or_else(|_| std::env::var("AOEM_FFI_PERSIST_BACKEND"))
            .unwrap_or_else(|_| match variant {
                AoemRuntimeVariant::Persist => "rocksdb".to_string(),
                _ => "none".to_string(),
            });

        let wasm_runtime = std::env::var("NOVOVM_AOEM_WASM_RUNTIME")
            .or_else(|_| std::env::var("AOEM_FFI_WASM_RUNTIME"))
            .unwrap_or_else(|_| match variant {
                AoemRuntimeVariant::Wasm => "wasmtime".to_string(),
                _ => "none".to_string(),
            });

        let zkvm_mode = std::env::var("NOVOVM_AOEM_ZKVM_MODE")
            .or_else(|_| std::env::var("AOEM_FFI_ZKVM_MODE"))
            .unwrap_or_else(|_| {
                if parse_bool_env("NOVOVM_AOEM_ENABLE_ZKVM")
                    .or_else(|| parse_bool_env("AOEM_FFI_ENABLE_ZKVM"))
                    .unwrap_or(false)
                {
                    "executor".to_string()
                } else {
                    "none".to_string()
                }
            });

        let mldsa_mode = std::env::var("NOVOVM_AOEM_MLDSA_MODE")
            .or_else(|_| std::env::var("AOEM_FFI_MLDSA_MODE"))
            .unwrap_or_else(|_| {
                if parse_bool_env("NOVOVM_AOEM_ENABLE_MLDSA")
                    .or_else(|| parse_bool_env("AOEM_FFI_ENABLE_MLDSA"))
                    .unwrap_or(false)
                {
                    "enabled".to_string()
                } else {
                    "none".to_string()
                }
            });

        let manifest_path = std::env::var("NOVOVM_AOEM_MANIFEST")
            .or_else(|_| std::env::var("AOEM_DLL_MANIFEST"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_manifest_path(&aoem_root));

        let runtime_profile_path = std::env::var("NOVOVM_AOEM_RUNTIME_PROFILE")
            .or_else(|_| std::env::var("AOEM_RUNTIME_PROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_runtime_profile_path(&aoem_root));

        let ingress_workers = parse_u32_env("NOVOVM_INGRESS_WORKERS")
            .or_else(|| parse_u32_env("AOEM_INGRESS_WORKERS"))
            .or(Some(16));

        Ok(Self {
            variant,
            aoem_root,
            dll_path,
            manifest_path,
            runtime_profile_path,
            plugin_dir,
            persist_backend,
            wasm_runtime,
            zkvm_mode,
            mldsa_mode,
            ingress_workers,
        })
    }

    pub fn open_options(&self) -> AoemExecOpenOptions {
        AoemExecOpenOptions {
            ingress_workers: self.ingress_workers,
        }
    }

    pub fn apply_process_env(&self) {
        std::env::set_var("AOEM_DLL", &self.dll_path);
        std::env::set_var("AOEM_DLL_MANIFEST", &self.manifest_path);
        std::env::set_var("AOEM_RUNTIME_PROFILE", &self.runtime_profile_path);
        std::env::set_var("AOEM_FFI_PERSIST_BACKEND", &self.persist_backend);
        std::env::set_var("AOEM_FFI_WASM_RUNTIME", &self.wasm_runtime);
        std::env::set_var("AOEM_FFI_ZKVM_MODE", &self.zkvm_mode);
        std::env::set_var("AOEM_FFI_MLDSA_MODE", &self.mldsa_mode);
        if let Some(dir) = &self.plugin_dir {
            std::env::set_var("AOEM_FFI_PLUGIN_DIR", dir);
            std::env::set_var("AOEM_FFI_PERSIST_PLUGIN_DIR", dir);
            std::env::set_var("AOEM_FFI_WASM_PLUGIN_DIR", dir);
            std::env::set_var("AOEM_FFI_ZKVM_PLUGIN_DIR", dir);
            std::env::set_var("AOEM_FFI_MLDSA_PLUGIN_DIR", dir);
        }
    }
}

pub struct AoemExecFacade {
    dynlib: AoemDyn,
    options: AoemExecOpenOptions,
}

pub struct AoemExecSession<'a> {
    handle: AoemHandle<'a>,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AoemExecReturnCode {
    Ok = 0,
    Partial = 1,
    InvalidInput = 1001,
    EngineExecFailed = 2001,
    StartupContractFailed = 3001,
    Unknown = 9000,
}

impl AoemExecReturnCode {
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Partial => "partial",
            Self::InvalidInput => "invalid_input",
            Self::EngineExecFailed => "engine_exec_failed",
            Self::StartupContractFailed => "startup_contract_failed",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AoemFailureClassV1 {
    Revert,
    OutOfGas,
    Invalid,
    ExecutionFailed,
}

impl AoemFailureClassV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Revert => "revert",
            Self::OutOfGas => "out_of_gas",
            Self::Invalid => "invalid",
            Self::ExecutionFailed => "execution_failed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AoemFailureClassSourceV1 {
    AnchorReturnCode,
    AnchorReturnCodeName,
    HeuristicNoArtifact,
    HeuristicRevertData,
    HeuristicGasUsedGeLimit,
    HeuristicDefault,
}

impl AoemFailureClassSourceV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AnchorReturnCode => "anchor_return_code",
            Self::AnchorReturnCodeName => "anchor_return_code_name",
            Self::HeuristicNoArtifact => "heuristic_no_artifact",
            Self::HeuristicRevertData => "heuristic_revert_data",
            Self::HeuristicGasUsedGeLimit => "heuristic_gas_used_ge_limit",
            Self::HeuristicDefault => "heuristic_default",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AoemFailureRecoverabilityV1 {
    Recoverable,
    NonRecoverable,
}

impl AoemFailureRecoverabilityV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recoverable => "recoverable",
            Self::NonRecoverable => "non_recoverable",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AoemFailureClassificationV1 {
    pub class: AoemFailureClassV1,
    pub source: AoemFailureClassSourceV1,
    pub recoverability: AoemFailureRecoverabilityV1,
}

pub fn aoem_failure_recoverability_from_class_v1(
    class: AoemFailureClassV1,
) -> AoemFailureRecoverabilityV1 {
    match class {
        AoemFailureClassV1::Invalid => AoemFailureRecoverabilityV1::NonRecoverable,
        _ => AoemFailureRecoverabilityV1::Recoverable,
    }
}

pub fn failure_class_from_anchor_return_code_v1(return_code: u32) -> Option<AoemFailureClassV1> {
    match return_code {
        13 => Some(AoemFailureClassV1::OutOfGas),
        14 | 1001 => Some(AoemFailureClassV1::Invalid),
        2001 | 3001 => Some(AoemFailureClassV1::ExecutionFailed),
        _ => None,
    }
}

pub fn failure_class_from_anchor_return_code_name_v1(
    return_code_name: &str,
) -> Option<AoemFailureClassV1> {
    let rc_name = return_code_name.to_ascii_lowercase();
    if rc_name.contains("out_of_gas")
        || rc_name.contains("out of gas")
        || rc_name.contains("gas_exhausted")
        || rc_name.contains("oog")
    {
        return Some(AoemFailureClassV1::OutOfGas);
    }
    if rc_name.contains("invalid")
        || rc_name.contains("bad instruction")
        || rc_name.contains("bad_opcode")
    {
        return Some(AoemFailureClassV1::Invalid);
    }
    if rc_name.contains("revert") {
        return Some(AoemFailureClassV1::Revert);
    }
    if rc_name.contains("engine_exec_failed")
        || rc_name.contains("startup_contract_failed")
        || rc_name.contains("execution_failed")
        || rc_name.contains("execution failed")
        || rc_name.contains("exec_failed")
        || rc_name.contains("vm_error")
    {
        return Some(AoemFailureClassV1::ExecutionFailed);
    }
    None
}

#[derive(Clone, Debug, Default)]
pub struct AoemExecMetrics {
    pub elapsed_us: u64,
    pub submitted_ops: u32,
    pub processed_ops: u32,
    pub success_ops: u32,
    pub total_writes: u64,
    pub failed_index: Option<u32>,
    pub return_code: u32,
    pub return_code_name: String,
    pub error_code: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct AoemExecOutput {
    pub result: AoemExecV2Result,
    pub metrics: AoemExecMetrics,
}

#[derive(Clone, Debug)]
pub struct AoemExecError {
    pub code: u32,
    pub code_name: String,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct AoemSubmitReport {
    pub return_code: u32,
    pub return_code_name: String,
    pub ok: bool,
    pub output: Option<AoemExecOutput>,
    pub error: Option<AoemExecError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AoemProjectedTxExecutionV1 {
    pub tx_index: u32,
    pub op_index: Option<u32>,
    pub tx_hash: Vec<u8>,
    pub gas_limit: u64,
    pub contract_address: Option<Vec<u8>>,
    pub log_emitter: Option<Vec<u8>>,
    pub event_logs: Vec<AoemEventLogV1>,
    pub receipt_type: Option<u8>,
    pub effective_gas_price: Option<u64>,
    pub runtime_code: Option<Vec<u8>>,
    pub runtime_code_hash: Option<Vec<u8>>,
    pub revert_data: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AoemCanonicalTxTypeV1 {
    Transfer,
    ContractCall,
    ContractDeploy,
    Privacy,
    CrossShard,
    CrossChainTransfer,
    CrossChainCall,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AoemFieldSourceV1 {
    #[default]
    Missing,
    AoemRaw,
    HostState,
    HostReconstruction,
    HostDerived,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AoemExecutionReconstructionSourcesV1 {
    pub status_source: AoemFieldSourceV1,
    pub gas_used_source: AoemFieldSourceV1,
    pub state_root_source: AoemFieldSourceV1,
    pub contract_address_source: AoemFieldSourceV1,
    pub runtime_code_source: AoemFieldSourceV1,
    pub runtime_code_hash_source: AoemFieldSourceV1,
    pub event_logs_source: AoemFieldSourceV1,
    pub log_bloom_source: AoemFieldSourceV1,
    pub revert_data_source: AoemFieldSourceV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AoemTxExecutionAnchorV1 {
    pub op_index: Option<u32>,
    pub processed_ops: u32,
    pub success_ops: u32,
    pub failed_index: Option<u32>,
    pub total_writes: u64,
    pub elapsed_us: u64,
    pub return_code: u32,
    pub return_code_name: String,
}

pub fn classify_failure_from_anchor_v1(
    anchor: &AoemTxExecutionAnchorV1,
) -> Option<AoemFailureClassificationV1> {
    if let Some(class) = failure_class_from_anchor_return_code_v1(anchor.return_code) {
        return Some(AoemFailureClassificationV1 {
            class,
            source: AoemFailureClassSourceV1::AnchorReturnCode,
            recoverability: aoem_failure_recoverability_from_class_v1(class),
        });
    }
    if let Some(class) = failure_class_from_anchor_return_code_name_v1(&anchor.return_code_name) {
        return Some(AoemFailureClassificationV1 {
            class,
            source: AoemFailureClassSourceV1::AnchorReturnCodeName,
            recoverability: aoem_failure_recoverability_from_class_v1(class),
        });
    }
    None
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AoemEventLogV1 {
    pub emitter: Vec<u8>,
    pub topics: Vec<[u8; 32]>,
    pub data: Vec<u8>,
    pub log_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervmEvmExecutionLogV1 {
    pub emitter: Vec<u8>,
    pub topics: Vec<[u8; 32]>,
    pub data: Vec<u8>,
    pub tx_index: u32,
    pub log_index: u32,
    pub state_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervmEvmExecutionReceiptV1 {
    pub chain_type: ChainType,
    pub chain_id: u64,
    pub tx_hash: Vec<u8>,
    pub tx_index: u32,
    pub tx_type: TxType,
    pub receipt_type: Option<u8>,
    pub status_ok: bool,
    pub gas_used: u64,
    pub cumulative_gas_used: u64,
    pub effective_gas_price: Option<u64>,
    pub log_bloom: Vec<u8>,
    pub revert_data: Option<Vec<u8>>,
    pub state_root: [u8; 32],
    pub state_version: u64,
    pub contract_address: Option<Vec<u8>>,
    pub logs: Vec<SupervmEvmExecutionLogV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervmEvmStateMirrorUpdateV1 {
    pub chain_type: ChainType,
    pub chain_id: u64,
    pub state_version: u64,
    pub state_root: [u8; 32],
    pub receipt_count: u64,
    pub accepted_receipt_count: u64,
    pub tx_hashes: Vec<Vec<u8>>,
    pub imported_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AoemExecutionAnchorLogDataV1 {
    pub tx_hash: Vec<u8>,
    pub tx_index: u32,
    pub state_root: [u8; 32],
    pub anchor: AoemTxExecutionAnchorV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AoemExecutionReconstructionInputV1 {
    pub tx_index: u32,
    pub tx_hash: Vec<u8>,
    pub tx_type: AoemCanonicalTxTypeV1,
    pub from: Vec<u8>,
    pub to: Option<Vec<u8>>,
    pub nonce: u64,
    pub gas_limit: u64,
    pub gas_used: Option<u64>,
    pub cumulative_gas_used: Option<u64>,
    pub gas_price: Option<u64>,
    pub receipt_type: Option<u8>,
    pub status_ok: bool,
    pub state_root: [u8; 32],
    pub contract_address: Option<Vec<u8>>,
    pub call_data: Vec<u8>,
    pub init_code: Option<Vec<u8>>,
    pub runtime_code: Option<Vec<u8>>,
    pub runtime_code_hash: Option<Vec<u8>>,
    pub revert_data: Option<Vec<u8>>,
    pub raw_event_logs: Vec<AoemEventLogV1>,
    pub raw_log_bloom: Option<Vec<u8>>,
    pub anchor: Option<AoemTxExecutionAnchorV1>,
    pub log_emitter: Option<Vec<u8>>,
    pub sources: AoemExecutionReconstructionSourcesV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AoemReceiptDerivationRulesV1 {
    pub derive_runtime_code_hash_when_missing: bool,
    pub derive_log_bloom_from_logs_when_missing: bool,
    pub rebuild_logs_from_runtime_code_when_missing: bool,
    pub rebuild_logs_requires_status_ok: bool,
    pub rebuild_logs_requires_runtime_code: bool,
    pub rebuild_logs_requires_call_data: bool,
    pub deploy_runtime_code_fallback_to_init_code: bool,
    pub derive_anchor_log_when_logs_empty: bool,
}

impl Default for AoemReceiptDerivationRulesV1 {
    fn default() -> Self {
        Self {
            derive_runtime_code_hash_when_missing: true,
            derive_log_bloom_from_logs_when_missing: true,
            rebuild_logs_from_runtime_code_when_missing: true,
            rebuild_logs_requires_status_ok: true,
            rebuild_logs_requires_runtime_code: true,
            rebuild_logs_requires_call_data: false,
            deploy_runtime_code_fallback_to_init_code: true,
            derive_anchor_log_when_logs_empty: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AoemTxExecutionArtifactV1 {
    pub tx_index: u32,
    pub tx_hash: Vec<u8>,
    pub status_ok: bool,
    pub gas_used: u64,
    pub cumulative_gas_used: u64,
    pub state_root: [u8; 32],
    pub contract_address: Option<Vec<u8>>,
    pub receipt_type: Option<u8>,
    pub effective_gas_price: Option<u64>,
    pub runtime_code: Option<Vec<u8>>,
    pub runtime_code_hash: Option<Vec<u8>>,
    pub event_logs: Vec<AoemEventLogV1>,
    pub log_bloom: Vec<u8>,
    pub revert_data: Option<Vec<u8>>,
    pub anchor: Option<AoemTxExecutionAnchorV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AoemBatchExecutionArtifactsV1 {
    pub state_root: [u8; 32],
    pub processed_ops: u32,
    pub success_ops: u32,
    pub failed_index: Option<u32>,
    pub total_writes: u64,
    pub tx_artifacts: Vec<AoemTxExecutionArtifactV1>,
}

pub fn aoem_op_succeeded_v1(output: &AoemExecOutput, op_index: u32) -> bool {
    let processed_ops = output.metrics.processed_ops;
    let success_ops = output.metrics.success_ops;
    if op_index >= processed_ops {
        return false;
    }
    if let Some(failed_index) = output.metrics.failed_index {
        return op_index < failed_index;
    }
    op_index < success_ops
}

pub const AOEM_LOG_BLOOM_BYTES_V1: usize = 256;
const AOEM_LOG_LAYER_MAX_STEPS_V1: usize = 200_000;
const AOEM_LOG_LAYER_MAX_STACK_V1: usize = 1024;
const AOEM_LOG_LAYER_MAX_MEMORY_BYTES_V1: usize = 8 * 1024 * 1024;

fn normalize_root32_v1(root: &[u8]) -> [u8; 32] {
    if root.len() == 32 {
        let mut out = [0u8; 32];
        out.copy_from_slice(root);
        return out;
    }
    Sha256::digest(root).into()
}

fn aoem_exec_log_topic_v1(seed: &[u8]) -> [u8; 32] {
    Sha256::digest(seed).into()
}

pub fn derive_runtime_code_hash_v1(runtime_code: &[u8]) -> Vec<u8> {
    Keccak256::digest(runtime_code).to_vec()
}

fn insert_log_bloom_bits_v1(bloom: &mut [u8], value: &[u8]) {
    let digest = Keccak256::digest(value);
    for offset in [0usize, 2, 4] {
        let bit = (((digest[offset] as usize) << 8) | digest[offset + 1] as usize) & 2047;
        let byte_index = bloom.len().saturating_sub(1).saturating_sub(bit / 8);
        let bit_mask = 1u8 << (bit % 8);
        if let Some(slot) = bloom.get_mut(byte_index) {
            *slot |= bit_mask;
        }
    }
}

pub fn build_log_bloom_v1(logs: &[AoemEventLogV1]) -> Vec<u8> {
    let mut bloom = vec![0u8; AOEM_LOG_BLOOM_BYTES_V1];
    for log in logs {
        if !log.emitter.is_empty() {
            insert_log_bloom_bits_v1(&mut bloom, &log.emitter);
        }
        for topic in &log.topics {
            insert_log_bloom_bits_v1(&mut bloom, topic);
        }
    }
    bloom
}

fn build_anchor_event_log_v1(
    projected: &AoemProjectedTxExecutionV1,
    state_root: [u8; 32],
    anchor: &AoemTxExecutionAnchorV1,
) -> Option<AoemEventLogV1> {
    let emitter = projected.log_emitter.clone()?;
    let data = serde_json::to_vec(&AoemExecutionAnchorLogDataV1 {
        tx_hash: projected.tx_hash.clone(),
        tx_index: projected.tx_index,
        state_root,
        anchor: anchor.clone(),
    })
    .ok()?;
    Some(AoemEventLogV1 {
        emitter,
        topics: vec![
            aoem_exec_log_topic_v1(b"supervm.evm.aoem.execution.v1"),
            normalize_root32_v1(projected.tx_hash.as_slice()),
            state_root,
        ],
        data,
        log_index: 0,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AoemExecutionLogRuntimeEntryV1 {
    topics: Vec<[u8; 32]>,
    data: Vec<u8>,
}

fn evm_word_to_usize_v1(word: [u8; 32], label: &str) -> Result<usize> {
    if word[..24].iter().any(|v| *v != 0) {
        bail!("{label} exceeds usize range");
    }
    let as_u64 = u64::from_be_bytes(word[24..32].try_into().expect("u64 slice"));
    usize::try_from(as_u64).map_err(|_| anyhow::anyhow!("{label} exceeds usize range"))
}

fn evm_memory_ensure_v1(memory: &mut Vec<u8>, required_len: usize) -> Result<()> {
    if required_len > AOEM_LOG_LAYER_MAX_MEMORY_BYTES_V1 {
        bail!(
            "evm log-layer memory limit exceeded: required={} limit={}",
            required_len,
            AOEM_LOG_LAYER_MAX_MEMORY_BYTES_V1
        );
    }
    if memory.len() < required_len {
        memory.resize(required_len, 0u8);
    }
    Ok(())
}

fn execute_runtime_log_opcode_layer_v1(
    code: &[u8],
    calldata: &[u8],
) -> Result<Vec<AoemExecutionLogRuntimeEntryV1>> {
    if code.is_empty() {
        return Ok(Vec::new());
    }
    let mut pc = 0usize;
    let mut steps = 0usize;
    let mut stack = Vec::<[u8; 32]>::new();
    let mut memory = Vec::<u8>::new();
    let mut logs = Vec::<AoemExecutionLogRuntimeEntryV1>::new();

    while pc < code.len() {
        if steps >= AOEM_LOG_LAYER_MAX_STEPS_V1 {
            bail!(
                "evm log-layer step limit exceeded: steps={}",
                AOEM_LOG_LAYER_MAX_STEPS_V1
            );
        }
        steps = steps.saturating_add(1);
        let opcode = code[pc];
        pc = pc.saturating_add(1);

        match opcode {
            0x00 | 0xf3 => break,
            0xfd => bail!("evm log-layer reverted"),
            0xfe => bail!("evm log-layer invalid opcode"),
            0x50 => {
                let _ = stack
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("evm log-layer stack underflow: POP"))?;
            }
            0x51 => {
                let offset = evm_word_to_usize_v1(
                    stack
                        .pop()
                        .ok_or_else(|| anyhow::anyhow!("evm log-layer stack underflow: MLOAD"))?,
                    "mload offset",
                )?;
                let end = offset
                    .checked_add(32)
                    .ok_or_else(|| anyhow::anyhow!("mload offset overflow"))?;
                evm_memory_ensure_v1(&mut memory, end)?;
                let mut word = [0u8; 32];
                word.copy_from_slice(&memory[offset..end]);
                if stack.len() >= AOEM_LOG_LAYER_MAX_STACK_V1 {
                    bail!("evm log-layer stack limit exceeded");
                }
                stack.push(word);
            }
            0x52 => {
                let offset = evm_word_to_usize_v1(
                    stack
                        .pop()
                        .ok_or_else(|| anyhow::anyhow!("evm log-layer stack underflow: MSTORE"))?,
                    "mstore offset",
                )?;
                let value = stack
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("evm log-layer stack underflow: MSTORE"))?;
                let end = offset
                    .checked_add(32)
                    .ok_or_else(|| anyhow::anyhow!("mstore offset overflow"))?;
                evm_memory_ensure_v1(&mut memory, end)?;
                memory[offset..end].copy_from_slice(&value);
            }
            0x5f => {
                if stack.len() >= AOEM_LOG_LAYER_MAX_STACK_V1 {
                    bail!("evm log-layer stack limit exceeded");
                }
                stack.push([0u8; 32]);
            }
            0x60..=0x7f => {
                let push_len = (opcode - 0x5f) as usize;
                let end = pc
                    .checked_add(push_len)
                    .ok_or_else(|| anyhow::anyhow!("push offset overflow"))?;
                if end > code.len() {
                    bail!("evm log-layer truncated PUSH data");
                }
                let mut word = [0u8; 32];
                word[32 - push_len..].copy_from_slice(&code[pc..end]);
                pc = end;
                if stack.len() >= AOEM_LOG_LAYER_MAX_STACK_V1 {
                    bail!("evm log-layer stack limit exceeded");
                }
                stack.push(word);
            }
            0x80..=0x8f => {
                let depth = (opcode - 0x7f) as usize;
                if stack.len() < depth {
                    bail!("evm log-layer stack underflow: DUP{}", depth);
                }
                let word = stack[stack.len() - depth];
                if stack.len() >= AOEM_LOG_LAYER_MAX_STACK_V1 {
                    bail!("evm log-layer stack limit exceeded");
                }
                stack.push(word);
            }
            0x90..=0x9f => {
                let depth = (opcode - 0x8f) as usize;
                if stack.len() <= depth {
                    bail!("evm log-layer stack underflow: SWAP{}", depth);
                }
                let top = stack.len() - 1;
                stack.swap(top, top - depth);
            }
            0xa0..=0xa4 => {
                let topic_count = (opcode - 0xa0) as usize;
                let offset = evm_word_to_usize_v1(
                    stack
                        .pop()
                        .ok_or_else(|| anyhow::anyhow!("evm log-layer stack underflow: LOG"))?,
                    "log offset",
                )?;
                let size = evm_word_to_usize_v1(
                    stack
                        .pop()
                        .ok_or_else(|| anyhow::anyhow!("evm log-layer stack underflow: LOG"))?,
                    "log size",
                )?;
                let end = offset
                    .checked_add(size)
                    .ok_or_else(|| anyhow::anyhow!("log slice overflow"))?;
                evm_memory_ensure_v1(&mut memory, end)?;
                let mut topics = Vec::<[u8; 32]>::with_capacity(topic_count);
                for _ in 0..topic_count {
                    topics.push(
                        stack
                            .pop()
                            .ok_or_else(|| anyhow::anyhow!("evm log-layer stack underflow: LOG"))?,
                    );
                }
                logs.push(AoemExecutionLogRuntimeEntryV1 {
                    topics,
                    data: memory[offset..end].to_vec(),
                });
            }
            0x35 => {
                let offset = evm_word_to_usize_v1(
                    stack.pop().ok_or_else(|| {
                        anyhow::anyhow!("evm log-layer stack underflow: CALLDATALOAD")
                    })?,
                    "calldata load offset",
                )?;
                let mut word = [0u8; 32];
                for (idx, out) in word.iter_mut().enumerate() {
                    let src = offset.saturating_add(idx);
                    if src < calldata.len() {
                        *out = calldata[src];
                    }
                }
                if stack.len() >= AOEM_LOG_LAYER_MAX_STACK_V1 {
                    bail!("evm log-layer stack limit exceeded");
                }
                stack.push(word);
            }
            0x36 => {
                let mut word = [0u8; 32];
                let len = calldata.len() as u64;
                word[24..32].copy_from_slice(&len.to_be_bytes());
                if stack.len() >= AOEM_LOG_LAYER_MAX_STACK_V1 {
                    bail!("evm log-layer stack limit exceeded");
                }
                stack.push(word);
            }
            0x37 => {
                let mem_offset = evm_word_to_usize_v1(
                    stack.pop().ok_or_else(|| {
                        anyhow::anyhow!("evm log-layer stack underflow: CALLDATACOPY")
                    })?,
                    "calldatacopy mem_offset",
                )?;
                let data_offset = evm_word_to_usize_v1(
                    stack.pop().ok_or_else(|| {
                        anyhow::anyhow!("evm log-layer stack underflow: CALLDATACOPY")
                    })?,
                    "calldatacopy data_offset",
                )?;
                let size = evm_word_to_usize_v1(
                    stack.pop().ok_or_else(|| {
                        anyhow::anyhow!("evm log-layer stack underflow: CALLDATACOPY")
                    })?,
                    "calldatacopy size",
                )?;
                let mem_end = mem_offset
                    .checked_add(size)
                    .ok_or_else(|| anyhow::anyhow!("calldatacopy mem overflow"))?;
                evm_memory_ensure_v1(&mut memory, mem_end)?;
                for i in 0..size {
                    let src = data_offset.saturating_add(i);
                    memory[mem_offset + i] = if src < calldata.len() {
                        calldata[src]
                    } else {
                        0u8
                    };
                }
            }
            _ => bail!("evm log-layer unsupported opcode: 0x{opcode:02x}"),
        }
    }

    Ok(logs)
}

fn can_rebuild_logs_from_runtime_code_v1(
    input: &AoemExecutionReconstructionInputV1,
    rules: &AoemReceiptDerivationRulesV1,
) -> bool {
    if !rules.rebuild_logs_from_runtime_code_when_missing {
        return false;
    }
    if rules.rebuild_logs_requires_status_ok && !input.status_ok {
        return false;
    }
    if rules.rebuild_logs_requires_runtime_code && input.runtime_code.is_none() {
        return false;
    }
    if rules.rebuild_logs_requires_call_data && input.call_data.is_empty() {
        return false;
    }
    matches!(input.tx_type, AoemCanonicalTxTypeV1::ContractCall)
}

pub fn reconstruct_tx_execution_artifact_v1(
    input: &AoemExecutionReconstructionInputV1,
    rules: &AoemReceiptDerivationRulesV1,
) -> Result<AoemTxExecutionArtifactV1> {
    let status_ok = input.status_ok;
    let runtime_code = if !status_ok {
        None
    } else {
        input.runtime_code.clone().or_else(|| {
            if rules.deploy_runtime_code_fallback_to_init_code
                && matches!(input.tx_type, AoemCanonicalTxTypeV1::ContractDeploy)
            {
                input.init_code.clone()
            } else {
                None
            }
        })
    };
    let runtime_code_hash = if !status_ok {
        None
    } else {
        input.runtime_code_hash.clone().or_else(|| {
            if rules.derive_runtime_code_hash_when_missing {
                runtime_code
                    .as_ref()
                    .map(|code| derive_runtime_code_hash_v1(code))
            } else {
                None
            }
        })
    };

    let mut event_logs = if status_ok {
        input.raw_event_logs.clone()
    } else {
        Vec::new()
    };
    if event_logs.is_empty() && can_rebuild_logs_from_runtime_code_v1(input, rules) {
        let emitter = input
            .to
            .clone()
            .or_else(|| input.contract_address.clone())
            .or_else(|| input.log_emitter.clone())
            .unwrap_or_else(|| input.from.clone());
        let runtime_entries = execute_runtime_log_opcode_layer_v1(
            runtime_code.as_deref().unwrap_or_default(),
            input.call_data.as_slice(),
        )?;
        event_logs = runtime_entries
            .into_iter()
            .enumerate()
            .map(|(idx, entry)| AoemEventLogV1 {
                emitter: emitter.clone(),
                topics: entry.topics,
                data: entry.data,
                log_index: idx as u32,
            })
            .collect();
    }
    if status_ok && event_logs.is_empty() && rules.derive_anchor_log_when_logs_empty {
        if let Some(anchor) = input.anchor.as_ref() {
            let projected = AoemProjectedTxExecutionV1 {
                tx_index: input.tx_index,
                op_index: anchor.op_index,
                tx_hash: input.tx_hash.clone(),
                gas_limit: input.gas_limit,
                contract_address: input.contract_address.clone(),
                log_emitter: input
                    .log_emitter
                    .clone()
                    .or_else(|| input.to.clone())
                    .or_else(|| Some(input.from.clone())),
                event_logs: Vec::new(),
                receipt_type: input.receipt_type,
                effective_gas_price: input.gas_price,
                runtime_code: runtime_code.clone(),
                runtime_code_hash: runtime_code_hash.clone(),
                revert_data: input.revert_data.clone(),
            };
            if let Some(anchor_log) =
                build_anchor_event_log_v1(&projected, input.state_root, anchor)
            {
                event_logs.push(anchor_log);
            }
        }
    }

    let mut log_bloom = input.raw_log_bloom.clone().unwrap_or_default();
    if log_bloom.len() != AOEM_LOG_BLOOM_BYTES_V1 {
        log_bloom.clear();
    }
    if !status_ok {
        log_bloom = vec![0u8; AOEM_LOG_BLOOM_BYTES_V1];
    } else if rules.derive_log_bloom_from_logs_when_missing
        && (log_bloom.is_empty()
            || (!event_logs.is_empty() && log_bloom.iter().all(|byte| *byte == 0))
            || (event_logs.is_empty() && log_bloom.iter().any(|byte| *byte != 0)))
    {
        log_bloom = if event_logs.is_empty() {
            vec![0u8; AOEM_LOG_BLOOM_BYTES_V1]
        } else {
            build_log_bloom_v1(event_logs.as_slice())
        };
    }

    let contract_address = if status_ok {
        input.contract_address.clone()
    } else {
        None
    };
    let gas_used = if status_ok {
        input.gas_used.unwrap_or(input.gas_limit)
    } else {
        input.gas_used.unwrap_or(0)
    };
    let cumulative_gas_used = input.cumulative_gas_used.unwrap_or(gas_used);
    Ok(AoemTxExecutionArtifactV1 {
        tx_index: input.tx_index,
        tx_hash: input.tx_hash.clone(),
        status_ok,
        gas_used,
        cumulative_gas_used,
        state_root: input.state_root,
        contract_address,
        receipt_type: input.receipt_type,
        effective_gas_price: input.gas_price,
        runtime_code,
        runtime_code_hash,
        event_logs,
        log_bloom,
        revert_data: if status_ok {
            None
        } else {
            input.revert_data.clone()
        },
        anchor: input.anchor.clone(),
    })
}

pub fn project_tx_execution_artifacts_v1(
    tx_count: usize,
    projected_txs: &[AoemProjectedTxExecutionV1],
    state_root: [u8; 32],
    output: &AoemExecOutput,
) -> AoemBatchExecutionArtifactsV1 {
    let max_projected_index = projected_txs
        .iter()
        .map(|item| item.tx_index as usize)
        .max()
        .map(|idx| idx.saturating_add(1))
        .unwrap_or(0);
    let artifact_len = tx_count.max(max_projected_index);
    let mut tx_artifacts = Vec::with_capacity(artifact_len);
    for tx_index in 0..artifact_len {
        tx_artifacts.push(AoemTxExecutionArtifactV1 {
            tx_index: tx_index as u32,
            tx_hash: Vec::new(),
            status_ok: false,
            gas_used: 0,
            cumulative_gas_used: 0,
            state_root,
            contract_address: None,
            receipt_type: None,
            effective_gas_price: None,
            runtime_code: None,
            runtime_code_hash: None,
            event_logs: Vec::new(),
            log_bloom: vec![0u8; AOEM_LOG_BLOOM_BYTES_V1],
            revert_data: None,
            anchor: None,
        });
    }

    for projected in projected_txs {
        let tx_index = projected.tx_index as usize;
        if tx_index >= tx_artifacts.len() {
            continue;
        }
        let status_ok = projected
            .op_index
            .map(|op_index| aoem_op_succeeded_v1(output, op_index))
            .unwrap_or(false);
        let gas_used = if status_ok { projected.gas_limit } else { 0 };
        let anchor = AoemTxExecutionAnchorV1 {
            op_index: projected.op_index,
            processed_ops: output.metrics.processed_ops,
            success_ops: output.metrics.success_ops,
            failed_index: output.metrics.failed_index,
            total_writes: output.metrics.total_writes,
            elapsed_us: output.metrics.elapsed_us,
            return_code: output.metrics.return_code,
            return_code_name: output.metrics.return_code_name.clone(),
        };
        let runtime_code = if status_ok {
            projected.runtime_code.clone()
        } else {
            None
        };
        let runtime_code_hash = if status_ok {
            projected.runtime_code_hash.clone().or_else(|| {
                runtime_code
                    .as_ref()
                    .map(|code| derive_runtime_code_hash_v1(code))
            })
        } else {
            None
        };
        let mut event_logs = if status_ok {
            projected.event_logs.clone()
        } else {
            Vec::new()
        };
        if status_ok && event_logs.is_empty() {
            if let Some(anchor_log) = build_anchor_event_log_v1(projected, state_root, &anchor) {
                event_logs.push(anchor_log);
            }
        }
        let log_bloom = build_log_bloom_v1(event_logs.as_slice());
        tx_artifacts[tx_index] = AoemTxExecutionArtifactV1 {
            tx_index: projected.tx_index,
            tx_hash: projected.tx_hash.clone(),
            status_ok,
            gas_used,
            cumulative_gas_used: 0,
            state_root,
            contract_address: if status_ok {
                projected.contract_address.clone()
            } else {
                None
            },
            receipt_type: projected.receipt_type,
            effective_gas_price: projected.effective_gas_price,
            runtime_code,
            runtime_code_hash,
            event_logs,
            log_bloom,
            revert_data: if status_ok {
                None
            } else {
                projected.revert_data.clone()
            },
            anchor: Some(anchor),
        };
    }

    let mut cumulative_gas_used = 0u64;
    for artifact in &mut tx_artifacts {
        cumulative_gas_used = cumulative_gas_used.saturating_add(artifact.gas_used);
        artifact.cumulative_gas_used = cumulative_gas_used;
    }

    AoemBatchExecutionArtifactsV1 {
        state_root,
        processed_ops: output.metrics.processed_ops,
        success_ops: output.metrics.success_ops,
        failed_index: output.metrics.failed_index,
        total_writes: output.metrics.total_writes,
        tx_artifacts,
    }
}

/// Stable capability contract consumed by NOVOVM host logic.
///
/// `raw` preserves AOEM original capabilities JSON so host can debug future fields
/// without recompiling this crate.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AoemCapabilityContract {
    pub execute_ops_v2: bool,
    pub zkvm_prove: bool,
    pub zkvm_verify: bool,
    pub zkvm_probe_api_present: bool,
    pub zkvm_symbol_supported: Option<bool>,
    pub zk_formal_fields_present: bool,
    pub msm_accel: bool,
    pub msm_backend: Option<String>,
    pub mldsa_verify: bool,
    pub fallback_reason: Option<String>,
    pub fallback_reason_codes: Vec<String>,
    pub inferred_from_legacy_fields: bool,
    pub raw: serde_json::Value,
}

impl AoemCapabilityContract {
    pub fn from_capabilities_json(raw: serde_json::Value) -> Self {
        let execute_ops_v2 = capability_bool(&raw, &["execute_ops_v2"]).unwrap_or(false);
        let zkvm_prove = capability_bool(
            &raw,
            &[
                "zkvm_prove",
                "zkvm.prove",
                "zkvm.prove_enabled",
                "zk.prove",
                "zk.prove_enabled",
            ],
        )
        .unwrap_or(false);
        let zkvm_verify = capability_bool(
            &raw,
            &[
                "zkvm_verify",
                "zkvm.verify",
                "zkvm.verify_enabled",
                "zk.verify",
                "zk.verify_enabled",
            ],
        )
        .unwrap_or(false);
        let zk_formal_fields_present = capability_bool(
            &raw,
            &[
                "zk_formal_fields_present",
                "zk.formal_fields_present",
                "zkvm.formal_fields_present",
            ],
        )
        .unwrap_or_else(|| {
            capability_exists(
                &raw,
                &[
                    "zkvm_prove",
                    "zkvm_verify",
                    "zk.prove",
                    "zk.verify",
                    "zk.prove_enabled",
                    "zk.verify_enabled",
                    "zkvm.prove",
                    "zkvm.verify",
                    "zkvm.prove_enabled",
                    "zkvm.verify_enabled",
                ],
            )
        });

        // Legacy AOEM capability set only exposed backend path fields.
        let msm_accel_direct = capability_bool(&raw, &["msm_accel", "msm.accel"]);
        let msm_accel_legacy = capability_bool(&raw, &["backend_gpu_path"]);
        let inferred_from_legacy_fields = msm_accel_direct.is_none() && msm_accel_legacy.is_some();
        let msm_accel = msm_accel_direct.or(msm_accel_legacy).unwrap_or(false);

        let msm_backend = capability_string(
            &raw,
            &[
                "msm_backend",
                "msm.backend",
                "msm.path_backend",
                "aoem.msm.backend",
            ],
        );
        let mldsa_verify =
            capability_bool(&raw, &["mldsa_verify", "mldsa.verify"]).unwrap_or(false);
        let fallback_reason_codes_raw = capability_string_list(
            &raw,
            &[
                "fallback_reason_codes",
                "fallback_reasons",
                "fallback.reason_codes",
                "fallback.reasons",
                "zkvm.fallback_reason_codes",
                "zkvm.reason_codes",
                "msm.fallback_reason_codes",
                "aoem.fallback_reason_codes",
            ],
        );
        let fallback_reason = capability_string(
            &raw,
            &[
                "fallback_reason",
                "fallback.reason",
                "zkvm.fallback_reason",
                "msm.fallback_reason",
            ],
        );
        let fallback_reason_codes =
            normalize_reason_codes(fallback_reason_codes_raw, fallback_reason.as_deref());
        let fallback_reason = fallback_reason
            .as_deref()
            .and_then(normalize_reason_code)
            .or_else(|| fallback_reason_codes.first().cloned());

        Self {
            execute_ops_v2,
            zkvm_prove,
            zkvm_verify,
            zkvm_probe_api_present: false,
            zkvm_symbol_supported: None,
            zk_formal_fields_present,
            msm_accel,
            msm_backend,
            mldsa_verify,
            fallback_reason,
            fallback_reason_codes,
            inferred_from_legacy_fields,
            raw,
        }
    }
}

impl AoemExecFacade {
    /// Opens AOEM from unified runtime config entry (core/persist/wasm).
    pub fn open_with_runtime(config: &AoemRuntimeConfig) -> Result<Self> {
        config.apply_process_env();
        Self::open(&config.dll_path, config.open_options())
    }

    /// Opens AOEM by resolving runtime config from environment variables.
    pub fn open_from_env() -> Result<Self> {
        let runtime = AoemRuntimeConfig::from_env()?;
        Self::open_with_runtime(&runtime)
    }

    /// Loads AOEM FFI DLL and validates startup contract (ABI + manifest + capabilities).
    pub fn open(dll_path: impl AsRef<Path>, options: AoemExecOpenOptions) -> Result<Self> {
        let dynlib = unsafe { AoemDyn::load(dll_path.as_ref()) }?;
        Ok(Self { dynlib, options })
    }

    pub fn abi(&self) -> u32 {
        self.dynlib.abi()
    }

    pub fn version(&self) -> String {
        self.dynlib.version()
    }

    pub fn capabilities_json(&self) -> Result<serde_json::Value> {
        self.dynlib.capabilities()
    }

    /// Returns normalized capability contract used by NOVOVM migration scripts and runtime checks.
    pub fn capability_contract(&self) -> Result<AoemCapabilityContract> {
        let raw = self.capabilities_json()?;
        let mut contract = AoemCapabilityContract::from_capabilities_json(raw);
        contract.zkvm_probe_api_present = self.dynlib.supports_zkvm_probe();
        contract.zkvm_symbol_supported = self.dynlib.zkvm_supported_flag();
        Ok(contract)
    }

    /// Convenience wrapper for tools that only need JSON output.
    pub fn capability_contract_json(&self) -> Result<serde_json::Value> {
        let contract = self.capability_contract()?;
        Ok(serde_json::to_value(contract)?)
    }

    /// AOEM-exported zkVM capability bit via symbol (`aoem_zkvm_supported`).
    /// `None` means loaded AOEM library does not export this symbol.
    pub fn zkvm_supported_by_symbol(&self) -> Option<bool> {
        self.dynlib.zkvm_supported_flag()
    }

    /// AOEM built-in Trace/Fibonacci zkVM prove+verify probe.
    /// Returns raw AOEM rc; fails only when symbol is not exported.
    pub fn zkvm_trace_fib_probe(&self, rounds: u32, witness_a: u64, witness_b: u64) -> Result<i32> {
        self.dynlib
            .zkvm_trace_fib_probe_rc(rounds, witness_a, witness_b)
            .ok_or_else(|| {
                anyhow::anyhow!("aoem_zkvm_trace_fib_prove_verify not exported by loaded AOEM FFI")
            })
    }

    /// Creates one execution session. Host can keep one session per worker thread.
    pub fn create_session(&self) -> Result<AoemExecSession<'_>> {
        let handle = self
            .dynlib
            .create_handle_with_ingress_workers(self.options.ingress_workers)?;
        Ok(AoemExecSession { handle })
    }

    pub fn supports_ops_wire_v1(&self) -> bool {
        self.dynlib.supports_execute_ops_wire_v1()
    }
}

impl<'a> AoemExecSession<'a> {
    pub fn execute_ops_v2(&self, ops: &[AoemOpV2]) -> Result<AoemExecV2Result> {
        self.handle.execute_ops_v2(ops)
    }

    pub fn execute_ops_wire_v1(&self, input: &[u8]) -> Result<AoemExecV2Result> {
        self.handle.execute_ops_wire_v1(input)
    }

    /// Host main-path stable entry: execute typed ops and return result+metrics in one object.
    pub fn submit_ops(&self, ops: &[AoemOpV2]) -> Result<AoemExecOutput> {
        if ops.is_empty() {
            anyhow::bail!("invalid op slice: op_count must be > 0");
        }
        let t0 = Instant::now();
        let result = self.execute_ops_v2(ops)?;
        let elapsed_us = t0.elapsed().as_micros() as u64;
        let code = classify_result_code(ops.len() as u32, &result);
        let metrics = AoemExecMetrics {
            elapsed_us,
            submitted_ops: ops.len() as u32,
            processed_ops: result.processed,
            success_ops: result.success,
            total_writes: result.total_writes,
            failed_index: if result.failed_index == u32::MAX {
                None
            } else {
                Some(result.failed_index)
            },
            return_code: code.as_u32(),
            return_code_name: code.as_str().to_string(),
            error_code: None,
        };
        Ok(AoemExecOutput { result, metrics })
    }

    /// Main-path report with unified return code + optional mapped error.
    pub fn submit_ops_report(&self, ops: &[AoemOpV2]) -> AoemSubmitReport {
        match self.submit_ops(ops) {
            Ok(out) => AoemSubmitReport {
                return_code: out.metrics.return_code,
                return_code_name: out.metrics.return_code_name.clone(),
                ok: out.metrics.return_code == AoemExecReturnCode::Ok.as_u32(),
                output: Some(out),
                error: None,
            },
            Err(err) => {
                let mapped = map_anyhow_error(&err);
                AoemSubmitReport {
                    return_code: mapped.code,
                    return_code_name: mapped.code_name.clone(),
                    ok: false,
                    output: None,
                    error: Some(mapped),
                }
            }
        }
    }

    /// Host main-path stable entry for generic binary ingress wire.
    pub fn submit_ops_wire(&self, input: &[u8]) -> Result<AoemExecOutput> {
        if input.is_empty() {
            anyhow::bail!("invalid wire slice: input_len must be > 0");
        }
        let t0 = Instant::now();
        let result = self.execute_ops_wire_v1(input)?;
        let elapsed_us = t0.elapsed().as_micros() as u64;
        let code = classify_result_code(result.processed, &result);
        let metrics = AoemExecMetrics {
            elapsed_us,
            submitted_ops: result.processed,
            processed_ops: result.processed,
            success_ops: result.success,
            total_writes: result.total_writes,
            failed_index: if result.failed_index == u32::MAX {
                None
            } else {
                Some(result.failed_index)
            },
            return_code: code.as_u32(),
            return_code_name: code.as_str().to_string(),
            error_code: None,
        };
        Ok(AoemExecOutput { result, metrics })
    }

    pub fn submit_ops_wire_report(&self, input: &[u8]) -> AoemSubmitReport {
        match self.submit_ops_wire(input) {
            Ok(out) => AoemSubmitReport {
                return_code: out.metrics.return_code,
                return_code_name: out.metrics.return_code_name.clone(),
                ok: out.metrics.return_code == AoemExecReturnCode::Ok.as_u32(),
                output: Some(out),
                error: None,
            },
            Err(err) => {
                let mapped = map_anyhow_error(&err);
                AoemSubmitReport {
                    return_code: mapped.code,
                    return_code_name: mapped.code_name.clone(),
                    ok: false,
                    output: None,
                    error: Some(mapped),
                }
            }
        }
    }
}

fn classify_result_code(submitted_ops: u32, result: &AoemExecV2Result) -> AoemExecReturnCode {
    if result.failed_index != u32::MAX
        || result.success < result.processed
        || result.processed < submitted_ops
    {
        AoemExecReturnCode::Partial
    } else {
        AoemExecReturnCode::Ok
    }
}

fn map_anyhow_error(err: &anyhow::Error) -> AoemExecError {
    let msg = err.to_string();
    let lower = msg.to_ascii_lowercase();
    let code = if lower.contains("invalid op slice")
        || lower.contains("op_count")
        || lower.contains("invalid input")
    {
        AoemExecReturnCode::InvalidInput
    } else if lower.contains("abi mismatch")
        || lower.contains("startup gate")
        || lower.contains("manifest")
        || lower.contains("capabilities")
    {
        AoemExecReturnCode::StartupContractFailed
    } else if lower.contains("aoem_execute_ops_v2 failed")
        || lower.contains("execute_ops_v2")
        || lower.contains("aoem_execute_ops_wire_v1 failed")
        || lower.contains("execute_ops_wire_v1")
    {
        AoemExecReturnCode::EngineExecFailed
    } else {
        AoemExecReturnCode::Unknown
    };

    AoemExecError {
        code: code.as_u32(),
        code_name: code.as_str().to_string(),
        message: msg,
    }
}

fn parse_u32_env(name: &str) -> Option<u32> {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
}

fn parse_bool_env(name: &str) -> Option<bool> {
    let raw = std::env::var(name).ok()?;
    let value = raw.trim().to_ascii_lowercase();
    match value.as_str() {
        "1" | "true" | "on" | "yes" => Some(true),
        "0" | "false" | "off" | "no" => Some(false),
        _ => None,
    }
}

fn dynlib_names_by_preference() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &["aoem_ffi.dll"]
    } else if cfg!(target_os = "macos") {
        &["libaoem_ffi.dylib"]
    } else {
        &["libaoem_ffi.so"]
    }
}

fn current_platform_dir_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn platform_roots_in_priority(root: &Path) -> Vec<PathBuf> {
    vec![root.join(current_platform_dir_name()), root.to_path_buf()]
}

fn default_manifest_path(root: &Path) -> PathBuf {
    let candidates = vec![
        root.join("manifest").join("aoem-manifest.json"),
        root.join(current_platform_dir_name())
            .join("manifest")
            .join("aoem-manifest.json"),
    ];
    for p in &candidates {
        if p.exists() {
            return p.clone();
        }
    }
    candidates[0].clone()
}

fn default_runtime_profile_path(root: &Path) -> PathBuf {
    let candidates = vec![
        root.join("config").join("aoem-runtime-profile.json"),
        root.join(current_platform_dir_name())
            .join("config")
            .join("aoem-runtime-profile.json"),
    ];
    for p in &candidates {
        if p.exists() {
            return p.clone();
        }
    }
    candidates[0].clone()
}

fn split_plugin_dir_list(raw: &str) -> Vec<PathBuf> {
    raw.split([';', ','])
        .filter_map(|item| {
            let trimmed = item.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
        .collect()
}

fn plugin_dirs_from_env_list() -> Vec<PathBuf> {
    std::env::var("NOVOVM_AOEM_PLUGIN_DIRS")
        .or_else(|_| std::env::var("AOEM_FFI_PLUGIN_DIRS"))
        .map(|raw| split_plugin_dir_list(&raw))
        .unwrap_or_default()
}

fn plugin_dir_match_score(dir: &Path, plugin_names: &[&str]) -> Option<(usize, u128)> {
    if plugin_names.is_empty() {
        return None;
    }
    let mut matched = 0usize;
    let mut latest_mtime_ns = 0u128;
    for name in plugin_names {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        matched = matched.saturating_add(1);
        if let Ok(meta) = std::fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                if let Ok(delta) = modified.duration_since(std::time::UNIX_EPOCH) {
                    let ts = delta.as_nanos();
                    if ts > latest_mtime_ns {
                        latest_mtime_ns = ts;
                    }
                }
            }
        }
    }
    if matched == 0 {
        None
    } else {
        Some((matched, latest_mtime_ns))
    }
}

fn pick_plugin_dir_from_candidates(
    variant: AoemRuntimeVariant,
    candidates: &[PathBuf],
) -> Option<PathBuf> {
    let plugin_names = plugin_names_for_variant(variant);
    if plugin_names.is_empty() {
        return None;
    }
    let mut best: Option<(PathBuf, usize, u128)> = None;
    for dir in candidates {
        let Some((matched, latest_mtime_ns)) = plugin_dir_match_score(dir, plugin_names) else {
            continue;
        };
        match &best {
            None => {
                best = Some((dir.clone(), matched, latest_mtime_ns));
            }
            Some((_, best_matched, best_mtime)) => {
                if matched > *best_matched
                    || (matched == *best_matched && latest_mtime_ns > *best_mtime)
                {
                    best = Some((dir.clone(), matched, latest_mtime_ns));
                }
            }
        }
    }
    best.map(|(path, _, _)| path)
}

fn plugin_names_for_variant(variant: AoemRuntimeVariant) -> &'static [&'static str] {
    match variant {
        AoemRuntimeVariant::Core => &[],
        AoemRuntimeVariant::Persist => {
            if cfg!(target_os = "windows") {
                &["aoem_ffi_persist_rocksdb.dll"]
            } else if cfg!(target_os = "macos") {
                &["libaoem_ffi_persist_rocksdb.dylib"]
            } else {
                &["libaoem_ffi_persist_rocksdb.so"]
            }
        }
        AoemRuntimeVariant::Wasm => {
            if cfg!(target_os = "windows") {
                &["aoem_ffi_runtime_wasm_wasmtime.dll"]
            } else if cfg!(target_os = "macos") {
                &["libaoem_ffi_runtime_wasm_wasmtime.dylib"]
            } else {
                &["libaoem_ffi_runtime_wasm_wasmtime.so"]
            }
        }
    }
}

fn default_plugin_dir(root: &Path, variant: AoemRuntimeVariant) -> Option<PathBuf> {
    if variant == AoemRuntimeVariant::Core {
        return None;
    }
    let variant_name = variant.as_str();
    let mut candidates = Vec::new();
    for platform_root in platform_roots_in_priority(root) {
        candidates.push(
            platform_root
                .join("core")
                .join("plugins")
                .join(variant_name),
        );
        candidates.push(platform_root.join("core").join("plugins"));
        candidates.push(platform_root.join("plugins").join(variant_name));
        candidates.push(platform_root.join("plugins"));
        candidates.push(platform_root.join("core").join("bin"));
        candidates.push(platform_root.join("bin"));
    }
    pick_plugin_dir_from_candidates(variant, &candidates)
}

fn default_dll_path(root: &Path, _variant: AoemRuntimeVariant) -> PathBuf {
    // Unified AOEM runtime: all variants load core dynlib and compose sidecars.
    for name in dynlib_names_by_preference() {
        for platform_root in platform_roots_in_priority(root) {
            for candidate in [
                platform_root.join("core").join("bin").join(name),
                platform_root.join("bin").join(name),
            ] {
                if candidate.exists() {
                    return candidate;
                }
            }
        }
    }
    root.join(current_platform_dir_name())
        .join("core")
        .join("bin")
        .join(dynlib_names_by_preference()[0])
}

fn capability_bool(root: &serde_json::Value, paths: &[&str]) -> Option<bool> {
    paths.iter().find_map(|p| {
        let mut cursor = root;
        for seg in p.split('.') {
            cursor = cursor.get(seg)?;
        }
        cursor.as_bool()
    })
}

fn capability_exists(root: &serde_json::Value, paths: &[&str]) -> bool {
    paths.iter().any(|p| {
        let mut cursor = root;
        for seg in p.split('.') {
            if let Some(next) = cursor.get(seg) {
                cursor = next;
            } else {
                return false;
            }
        }
        true
    })
}

fn capability_string(root: &serde_json::Value, paths: &[&str]) -> Option<String> {
    paths.iter().find_map(|p| {
        let mut cursor = root;
        for seg in p.split('.') {
            cursor = cursor.get(seg)?;
        }
        cursor.as_str().map(|s| s.to_string())
    })
}

fn capability_string_list(root: &serde_json::Value, paths: &[&str]) -> Vec<String> {
    for p in paths {
        let mut cursor = root;
        let mut ok = true;
        for seg in p.split('.') {
            if let Some(next) = cursor.get(seg) {
                cursor = next;
            } else {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        if let Some(arr) = cursor.as_array() {
            let out: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if !out.is_empty() {
                return out;
            }
        }
    }
    Vec::new()
}

fn normalize_reason_codes(codes: Vec<String>, single_reason: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    for c in codes {
        if let Some(v) = normalize_reason_code(&c) {
            if !out.contains(&v) {
                out.push(v);
            }
        }
    }
    if let Some(single) = single_reason.and_then(normalize_reason_code) {
        if !out.contains(&single) {
            out.push(single);
        }
    }
    out
}

fn normalize_reason_code(input: &str) -> Option<String> {
    let raw = input.trim();
    if raw.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(raw.len());
    let mut prev_underscore = false;
    for ch in raw.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '_'
        };
        if mapped == '_' {
            if prev_underscore {
                continue;
            }
            prev_underscore = true;
            out.push(mapped);
        } else {
            prev_underscore = false;
            out.push(mapped);
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub use aoem_bindings::acquire_global_lane;
pub use aoem_bindings::global_parallel_budget;
pub use aoem_bindings::recommend_threads_auto;
pub use aoem_bindings::recommend_threads_from_aoem;
pub use aoem_bindings::set_global_parallel_budget;
/// Re-export AOEM V2 op/result types for host integration.
pub use aoem_bindings::AoemExecV2Result as ExecResultV2;
pub use aoem_bindings::AoemHostAdaptiveDecision;
pub use aoem_bindings::AoemHostHint;
pub use aoem_bindings::AoemOpV2 as ExecOpV2;
pub use ingress_codec::EncodedOpsWire;
pub use ingress_codec::IngressCodecRegistry;
pub use ingress_codec::OpsWireOp;
pub use ingress_codec::OpsWireV1Builder;
pub use ingress_codec::RawIngressCodecRegistry;
pub use ingress_codec::AOEM_OPS_WIRE_V1_MAGIC;
pub use ingress_codec::AOEM_OPS_WIRE_V1_VERSION;

#[allow(dead_code)]
fn _assert_abi_struct_layout(_v: AoemCreateOptionsV1) {}

#[cfg(test)]
mod tests {
    use super::{
        aoem_op_succeeded_v1, classify_failure_from_anchor_v1, current_platform_dir_name,
        default_dll_path, default_plugin_dir, dynlib_names_by_preference,
        failure_class_from_anchor_return_code_name_v1, failure_class_from_anchor_return_code_v1,
        pick_plugin_dir_from_candidates, plugin_names_for_variant,
        project_tx_execution_artifacts_v1, reconstruct_tx_execution_artifact_v1,
        split_plugin_dir_list, AoemBatchExecutionArtifactsV1, AoemCanonicalTxTypeV1,
        AoemCapabilityContract, AoemExecMetrics, AoemExecOutput,
        AoemExecutionReconstructionInputV1, AoemExecutionReconstructionSourcesV1,
        AoemFailureClassSourceV1, AoemFailureClassV1, AoemFailureRecoverabilityV1,
        AoemProjectedTxExecutionV1, AoemReceiptDerivationRulesV1, AoemRuntimeVariant,
        AoemTxExecutionAnchorV1, AOEM_LOG_BLOOM_BYTES_V1,
    };
    use aoem_bindings::AoemExecV2Result;
    use serde_json::json;
    use sha3::{Digest, Keccak256};
    use std::fs;
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("novovm-exec-{name}-{nonce}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn capability_contract_reads_explicit_zk_msm_fields() {
        let raw = json!({
            "execute_ops_v2": true,
            "zkvm": { "prove": true, "verify": true },
            "msm": {
                "accel": true,
                "backend": "bls12_381_gpu",
                "fallback_reason_codes": ["gpu_unavailable", "invalid_input"]
            }
        });

        let c = AoemCapabilityContract::from_capabilities_json(raw);
        assert!(c.execute_ops_v2);
        assert!(c.zkvm_prove);
        assert!(c.zkvm_verify);
        assert!(!c.zkvm_probe_api_present);
        assert!(c.zkvm_symbol_supported.is_none());
        assert!(c.zk_formal_fields_present);
        assert!(c.msm_accel);
        assert_eq!(c.msm_backend.as_deref(), Some("bls12_381_gpu"));
        assert!(!c.mldsa_verify);
        assert_eq!(c.fallback_reason_codes.len(), 2);
        assert_eq!(c.fallback_reason.as_deref(), Some("gpu_unavailable"));
        assert!(!c.inferred_from_legacy_fields);
    }

    #[test]
    fn capability_contract_falls_back_to_legacy_gpu_field() {
        let raw = json!({
            "execute_ops_v2": true,
            "backend_gpu_path": true
        });

        let c = AoemCapabilityContract::from_capabilities_json(raw);
        assert!(c.execute_ops_v2);
        assert!(!c.zkvm_prove);
        assert!(!c.zkvm_verify);
        assert!(!c.zkvm_probe_api_present);
        assert!(c.zkvm_symbol_supported.is_none());
        assert!(!c.zk_formal_fields_present);
        assert!(c.msm_accel);
        assert!(!c.mldsa_verify);
        assert!(c.fallback_reason.is_none());
        assert!(c.inferred_from_legacy_fields);
    }

    #[test]
    fn capability_contract_treats_flat_zk_fields_as_formal() {
        let raw = json!({
            "execute_ops_v2": true,
            "zkvm_prove": true,
            "zkvm_verify": false
        });

        let c = AoemCapabilityContract::from_capabilities_json(raw);
        assert!(c.zkvm_prove);
        assert!(!c.zkvm_verify);
        assert!(!c.zkvm_probe_api_present);
        assert!(c.zkvm_symbol_supported.is_none());
        assert!(c.zk_formal_fields_present);
        assert!(!c.mldsa_verify);
    }

    #[test]
    fn capability_contract_normalizes_reason_codes_and_alias_fields() {
        let raw = json!({
            "execute_ops_v2": true,
            "zk": {
                "prove_enabled": true,
                "verify_enabled": false
            },
            "fallback": {
                "reason_codes": ["GPU Unavailable", "gpu-unavailable", "ffi missing fallback"]
            },
            "fallback_reason": "  invalid input  "
        });

        let c = AoemCapabilityContract::from_capabilities_json(raw);
        assert!(c.execute_ops_v2);
        assert!(c.zkvm_prove);
        assert!(!c.zkvm_verify);
        assert_eq!(
            c.fallback_reason_codes,
            vec![
                "gpu_unavailable".to_string(),
                "ffi_missing_fallback".to_string(),
                "invalid_input".to_string()
            ]
        );
        assert_eq!(c.fallback_reason.as_deref(), Some("invalid_input"));
    }

    #[test]
    fn anchor_return_code_failure_mapping_v1() {
        assert_eq!(
            failure_class_from_anchor_return_code_v1(13),
            Some(AoemFailureClassV1::OutOfGas)
        );
        assert_eq!(
            failure_class_from_anchor_return_code_v1(14),
            Some(AoemFailureClassV1::Invalid)
        );
        assert_eq!(
            failure_class_from_anchor_return_code_v1(1001),
            Some(AoemFailureClassV1::Invalid)
        );
        assert_eq!(
            failure_class_from_anchor_return_code_v1(2001),
            Some(AoemFailureClassV1::ExecutionFailed)
        );
        assert_eq!(
            failure_class_from_anchor_return_code_v1(3001),
            Some(AoemFailureClassV1::ExecutionFailed)
        );
        assert_eq!(failure_class_from_anchor_return_code_v1(0), None);
    }

    #[test]
    fn classify_failure_from_anchor_prefers_return_code_then_name_v1() {
        assert_eq!(
            failure_class_from_anchor_return_code_name_v1("revert"),
            Some(AoemFailureClassV1::Revert)
        );
        let anchor = AoemTxExecutionAnchorV1 {
            op_index: Some(1),
            processed_ops: 2,
            success_ops: 1,
            failed_index: Some(1),
            total_writes: 3,
            elapsed_us: 9,
            return_code: 13,
            return_code_name: "revert".to_string(),
        };
        let mapped = classify_failure_from_anchor_v1(&anchor).expect("mapped");
        assert_eq!(mapped.class, AoemFailureClassV1::OutOfGas);
        assert_eq!(mapped.source, AoemFailureClassSourceV1::AnchorReturnCode);
        assert_eq!(
            mapped.recoverability,
            AoemFailureRecoverabilityV1::Recoverable
        );

        let name_only_anchor = AoemTxExecutionAnchorV1 {
            return_code: 0,
            return_code_name: "invalid opcode".to_string(),
            ..anchor
        };
        let mapped = classify_failure_from_anchor_v1(&name_only_anchor).expect("mapped from name");
        assert_eq!(mapped.class, AoemFailureClassV1::Invalid);
        assert_eq!(
            mapped.source,
            AoemFailureClassSourceV1::AnchorReturnCodeName
        );
        assert_eq!(
            mapped.recoverability,
            AoemFailureRecoverabilityV1::NonRecoverable
        );
    }

    #[test]
    fn classify_failure_from_anchor_return_code_name_aliases_v1() {
        assert_eq!(
            failure_class_from_anchor_return_code_name_v1("out of gas"),
            Some(AoemFailureClassV1::OutOfGas)
        );
        assert_eq!(
            failure_class_from_anchor_return_code_name_v1("gas_exhausted"),
            Some(AoemFailureClassV1::OutOfGas)
        );
        assert_eq!(
            failure_class_from_anchor_return_code_name_v1("bad instruction"),
            Some(AoemFailureClassV1::Invalid)
        );
        assert_eq!(
            failure_class_from_anchor_return_code_name_v1("bad_opcode"),
            Some(AoemFailureClassV1::Invalid)
        );
        assert_eq!(
            failure_class_from_anchor_return_code_name_v1("execution failed"),
            Some(AoemFailureClassV1::ExecutionFailed)
        );
        assert_eq!(
            failure_class_from_anchor_return_code_name_v1("vm_error"),
            Some(AoemFailureClassV1::ExecutionFailed)
        );
    }

    #[test]
    fn aoem_tx_execution_artifacts_project_batch_status_and_gas() {
        let output = AoemExecOutput {
            result: AoemExecV2Result {
                processed: 2,
                success: 1,
                failed_index: 1,
                total_writes: 7,
            },
            metrics: AoemExecMetrics {
                elapsed_us: 88,
                submitted_ops: 2,
                processed_ops: 2,
                success_ops: 1,
                total_writes: 7,
                failed_index: Some(1),
                return_code: 0,
                return_code_name: "ok".to_string(),
                error_code: None,
            },
        };
        assert!(aoem_op_succeeded_v1(&output, 0));
        assert!(!aoem_op_succeeded_v1(&output, 1));
        let projected = vec![
            AoemProjectedTxExecutionV1 {
                tx_index: 0,
                op_index: Some(0),
                tx_hash: vec![0x11; 32],
                gas_limit: 21_000,
                contract_address: None,
                log_emitter: Some(vec![0xaa; 20]),
                event_logs: Vec::new(),
                receipt_type: Some(2),
                effective_gas_price: Some(7),
                runtime_code: Some(vec![0x60, 0x00, 0x60, 0x02]),
                runtime_code_hash: None,
                revert_data: None,
            },
            AoemProjectedTxExecutionV1 {
                tx_index: 1,
                op_index: Some(1),
                tx_hash: vec![0x22; 32],
                gas_limit: 55_000,
                contract_address: Some(vec![0x33; 20]),
                log_emitter: Some(vec![0xbb; 20]),
                event_logs: Vec::new(),
                receipt_type: Some(3),
                effective_gas_price: Some(9),
                runtime_code: Some(vec![0x60, 0x00, 0x60, 0x01]),
                runtime_code_hash: None,
                revert_data: Some(vec![0xde, 0xad]),
            },
        ];
        let artifacts: AoemBatchExecutionArtifactsV1 =
            project_tx_execution_artifacts_v1(3, projected.as_slice(), [0x44; 32], &output);
        assert_eq!(artifacts.tx_artifacts.len(), 3);
        assert_eq!(artifacts.state_root, [0x44; 32]);
        assert!(artifacts.tx_artifacts[0].status_ok);
        assert_eq!(artifacts.tx_artifacts[0].gas_used, 21_000);
        assert_eq!(artifacts.tx_artifacts[0].cumulative_gas_used, 21_000);
        assert_eq!(artifacts.tx_artifacts[0].receipt_type, Some(2));
        assert_eq!(artifacts.tx_artifacts[0].effective_gas_price, Some(7));
        assert_eq!(
            artifacts.tx_artifacts[0].runtime_code,
            Some(vec![0x60, 0x00, 0x60, 0x02])
        );
        assert_eq!(
            artifacts.tx_artifacts[0].runtime_code_hash,
            Some(Keccak256::digest([0x60, 0x00, 0x60, 0x02]).to_vec())
        );
        assert_eq!(artifacts.tx_artifacts[0].event_logs.len(), 1);
        assert_eq!(
            artifacts.tx_artifacts[0].event_logs[0].emitter,
            vec![0xaa; 20]
        );
        assert_eq!(
            artifacts.tx_artifacts[0].log_bloom.len(),
            AOEM_LOG_BLOOM_BYTES_V1
        );
        assert!(artifacts.tx_artifacts[0]
            .log_bloom
            .iter()
            .any(|byte| *byte != 0));
        assert_eq!(
            artifacts.tx_artifacts[0]
                .anchor
                .as_ref()
                .and_then(|a| a.op_index),
            Some(0)
        );
        assert!(!artifacts.tx_artifacts[1].status_ok);
        assert_eq!(artifacts.tx_artifacts[1].gas_used, 0);
        assert!(artifacts.tx_artifacts[1].contract_address.is_none());
        assert_eq!(artifacts.tx_artifacts[1].receipt_type, Some(3));
        assert_eq!(artifacts.tx_artifacts[1].effective_gas_price, Some(9));
        assert!(artifacts.tx_artifacts[1].event_logs.is_empty());
        assert!(artifacts.tx_artifacts[1].runtime_code.is_none());
        assert!(artifacts.tx_artifacts[1].runtime_code_hash.is_none());
        assert_eq!(
            artifacts.tx_artifacts[1].revert_data,
            Some(vec![0xde, 0xad])
        );
        assert_eq!(artifacts.tx_artifacts[2].tx_hash, Vec::<u8>::new());
        assert!(!artifacts.tx_artifacts[2].status_ok);
        assert_eq!(artifacts.tx_artifacts[2].cumulative_gas_used, 21_000);
        assert_eq!(
            artifacts.tx_artifacts[2].log_bloom,
            vec![0u8; AOEM_LOG_BLOOM_BYTES_V1]
        );
    }

    fn runtime_code_emit_single_log(topic0: [u8; 32], data_word: [u8; 32]) -> Vec<u8> {
        let mut code = Vec::new();
        code.push(0x7f);
        code.extend_from_slice(&data_word);
        code.push(0x60);
        code.push(0x00);
        code.push(0x52);
        code.push(0x7f);
        code.extend_from_slice(&topic0);
        code.push(0x60);
        code.push(0x20);
        code.push(0x60);
        code.push(0x00);
        code.push(0xa1);
        code.push(0x00);
        code
    }

    #[test]
    fn reconstruct_tx_execution_artifact_rebuilds_logs_from_runtime_code() {
        let topic0 = [0x55; 32];
        let data_word = [0x42; 32];
        let runtime_code = runtime_code_emit_single_log(topic0, data_word);
        let runtime_code_hash = Keccak256::digest(runtime_code.as_slice()).to_vec();
        let input = AoemExecutionReconstructionInputV1 {
            tx_index: 3,
            tx_hash: vec![0x11; 32],
            tx_type: AoemCanonicalTxTypeV1::ContractCall,
            from: vec![0xaa; 20],
            to: Some(vec![0xbb; 20]),
            nonce: 9,
            gas_limit: 80_000,
            gas_used: None,
            cumulative_gas_used: None,
            gas_price: Some(7),
            receipt_type: Some(2),
            status_ok: true,
            state_root: [0x33; 32],
            contract_address: None,
            call_data: vec![0xde, 0xad],
            init_code: None,
            runtime_code: Some(runtime_code),
            runtime_code_hash: None,
            revert_data: None,
            raw_event_logs: Vec::new(),
            raw_log_bloom: Some(vec![0u8; AOEM_LOG_BLOOM_BYTES_V1]),
            anchor: None,
            log_emitter: None,
            sources: AoemExecutionReconstructionSourcesV1::default(),
        };
        let artifact =
            reconstruct_tx_execution_artifact_v1(&input, &AoemReceiptDerivationRulesV1::default())
                .expect("reconstruct artifact");
        assert_eq!(artifact.tx_index, 3);
        assert_eq!(artifact.gas_used, 80_000);
        assert_eq!(artifact.event_logs.len(), 1);
        assert_eq!(artifact.event_logs[0].emitter, vec![0xbb; 20]);
        assert_eq!(artifact.event_logs[0].topics, vec![topic0]);
        assert_eq!(artifact.event_logs[0].data, data_word.to_vec());
        assert!(artifact.log_bloom.iter().any(|byte| *byte != 0));
        assert_eq!(artifact.runtime_code_hash, Some(runtime_code_hash));
    }

    #[test]
    fn capability_contract_reads_mldsa_flag() {
        let raw = json!({
            "execute_ops_v2": true,
            "zkvm_prove": false,
            "zkvm_verify": false,
            "mldsa_verify": true
        });

        let c = AoemCapabilityContract::from_capabilities_json(raw);
        assert!(c.execute_ops_v2);
        assert!(!c.zkvm_probe_api_present);
        assert!(c.zkvm_symbol_supported.is_none());
        assert!(c.mldsa_verify);
    }

    #[test]
    fn default_dll_path_prefers_host_name_when_present() {
        let root = temp_dir("default-dll-prefer-host");
        let bin = root
            .join(current_platform_dir_name())
            .join("core")
            .join("bin");
        fs::create_dir_all(&bin).expect("create bin dir");

        let host_name = dynlib_names_by_preference()[0];
        let host_path = bin.join(host_name);
        fs::write(&host_path, b"stub").expect("write host dylib");

        let selected = default_dll_path(&root, AoemRuntimeVariant::Core);
        assert_eq!(selected, host_path);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn default_dll_path_uses_host_default_when_host_binary_missing() {
        let root = temp_dir("default-dll-fallback");
        let bin = root
            .join(current_platform_dir_name())
            .join("core")
            .join("bin");
        fs::create_dir_all(&bin).expect("create bin dir");

        let dll = bin.join("aoem_ffi.dll");
        fs::write(&dll, b"stub").expect("write dll");

        let selected = default_dll_path(&root, AoemRuntimeVariant::Core);
        if cfg!(target_os = "windows") {
            assert_eq!(selected, dll);
        } else {
            let expected = bin.join(dynlib_names_by_preference()[0]);
            assert_eq!(selected, expected);
            assert_ne!(selected, dll);
        }

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn split_plugin_dir_list_parses_semicolon_and_comma() {
        let dirs = split_plugin_dir_list("C:\\p1; C:\\p2, C:\\p3 ,, ;");
        assert_eq!(dirs.len(), 3);
        assert_eq!(dirs[0], PathBuf::from("C:\\p1"));
        assert_eq!(dirs[1], PathBuf::from("C:\\p2"));
        assert_eq!(dirs[2], PathBuf::from("C:\\p3"));
    }

    #[test]
    fn pick_plugin_dir_prefers_newest_when_match_count_equal() {
        let root = temp_dir("plugin-dir-pick");
        let d1 = root.join("plugins_a");
        let d2 = root.join("plugins_b");
        fs::create_dir_all(&d1).expect("create d1");
        fs::create_dir_all(&d2).expect("create d2");
        let name = plugin_names_for_variant(AoemRuntimeVariant::Persist)[0];
        fs::write(d1.join(name), b"old").expect("write d1 plugin");
        thread::sleep(Duration::from_millis(5));
        fs::write(d2.join(name), b"new").expect("write d2 plugin");

        let picked =
            pick_plugin_dir_from_candidates(AoemRuntimeVariant::Persist, &[d1.clone(), d2.clone()])
                .expect("pick plugin dir");
        assert_eq!(picked, d2);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn default_plugin_dir_prefers_variant_subdir() {
        let root = temp_dir("plugin-default-variant-subdir");
        let variant_subdir = root
            .join(current_platform_dir_name())
            .join("core")
            .join("plugins")
            .join(AoemRuntimeVariant::Persist.as_str());
        fs::create_dir_all(&variant_subdir).expect("create variant subdir");
        let fallback_subdir = root
            .join(current_platform_dir_name())
            .join("core")
            .join("plugins");
        fs::create_dir_all(&fallback_subdir).expect("create fallback subdir");
        let name = plugin_names_for_variant(AoemRuntimeVariant::Persist)[0];
        fs::write(fallback_subdir.join(name), b"fallback").expect("write fallback plugin");
        thread::sleep(Duration::from_millis(5));
        fs::write(variant_subdir.join(name), b"variant").expect("write variant plugin");

        let picked = default_plugin_dir(&root, AoemRuntimeVariant::Persist).expect("pick default");
        assert_eq!(picked, variant_subdir);

        fs::remove_dir_all(root).expect("cleanup");
    }
}
