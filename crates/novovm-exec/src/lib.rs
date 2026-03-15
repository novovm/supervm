use anyhow::{bail, Result};
use aoem_bindings::{AoemCreateOptionsV1, AoemDyn, AoemExecV2Result, AoemHandle, AoemOpV2};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

mod ingress_codec;

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
        current_platform_dir_name, default_dll_path, default_plugin_dir,
        dynlib_names_by_preference, pick_plugin_dir_from_candidates, plugin_names_for_variant,
        split_plugin_dir_list, AoemCapabilityContract, AoemRuntimeVariant,
    };
    use serde_json::json;
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
