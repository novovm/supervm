use anyhow::{bail, Result};
use aoem_bindings::{AoemCreateOptionsV1, AoemDyn, AoemExecV2Result, AoemHandle, AoemOpV2};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

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
    pub ingress_workers: Option<u32>,
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
            .unwrap_or_else(|_| PathBuf::from(r"D:\WorksArea\SUPERVM\aoem"));

        let dll_path = std::env::var("NOVOVM_AOEM_DLL")
            .or_else(|_| std::env::var("AOEM_DLL"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_dll_path(&aoem_root, variant));

        let manifest_path = std::env::var("NOVOVM_AOEM_MANIFEST")
            .or_else(|_| std::env::var("AOEM_DLL_MANIFEST"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| aoem_root.join("manifest").join("aoem-manifest.json"));

        let runtime_profile_path = std::env::var("NOVOVM_AOEM_RUNTIME_PROFILE")
            .or_else(|_| std::env::var("AOEM_RUNTIME_PROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| aoem_root.join("config").join("aoem-runtime-profile.json"));

        let ingress_workers = parse_u32_env("NOVOVM_INGRESS_WORKERS")
            .or_else(|| parse_u32_env("AOEM_INGRESS_WORKERS"))
            .or(Some(16));

        Ok(Self {
            variant,
            aoem_root,
            dll_path,
            manifest_path,
            runtime_profile_path,
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
    pub msm_accel: bool,
    pub msm_backend: Option<String>,
    pub fallback_reason_codes: Vec<String>,
    pub inferred_from_legacy_fields: bool,
    pub raw: serde_json::Value,
}

impl AoemCapabilityContract {
    pub fn from_capabilities_json(raw: serde_json::Value) -> Self {
        let execute_ops_v2 = capability_bool(&raw, &["execute_ops_v2"]).unwrap_or(false);
        let zkvm_prove = capability_bool(&raw, &["zkvm_prove", "zkvm.prove"]).unwrap_or(false);
        let zkvm_verify = capability_bool(&raw, &["zkvm_verify", "zkvm.verify"]).unwrap_or(false);

        // Legacy AOEM capability set only exposed backend path fields.
        let msm_accel_direct = capability_bool(&raw, &["msm_accel", "msm.accel"]);
        let msm_accel_legacy = capability_bool(&raw, &["backend_gpu_path"]);
        let inferred_from_legacy_fields = msm_accel_direct.is_none() && msm_accel_legacy.is_some();
        let msm_accel = msm_accel_direct.or(msm_accel_legacy).unwrap_or(false);

        let msm_backend = capability_string(&raw, &["msm_backend", "msm.backend"]);
        let fallback_reason_codes = capability_string_list(
            &raw,
            &["fallback_reason_codes", "fallback_reasons", "msm.fallback_reason_codes"],
        );

        Self {
            execute_ops_v2,
            zkvm_prove,
            zkvm_verify,
            msm_accel,
            msm_backend,
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
        Ok(AoemCapabilityContract::from_capabilities_json(raw))
    }

    /// Convenience wrapper for tools that only need JSON output.
    pub fn capability_contract_json(&self) -> Result<serde_json::Value> {
        let contract = self.capability_contract()?;
        Ok(serde_json::to_value(contract)?)
    }

    /// Creates one execution session. Host can keep one session per worker thread.
    pub fn create_session(&self) -> Result<AoemExecSession<'_>> {
        let handle = self
            .dynlib
            .create_handle_with_ingress_workers(self.options.ingress_workers)?;
        Ok(AoemExecSession { handle })
    }
}

impl<'a> AoemExecSession<'a> {
    pub fn execute_ops_v2(&self, ops: &[AoemOpV2]) -> Result<AoemExecV2Result> {
        self.handle.execute_ops_v2(ops)
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
    } else if lower.contains("aoem_execute_ops_v2 failed") || lower.contains("execute_ops_v2") {
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

fn default_dll_path(root: &Path, variant: AoemRuntimeVariant) -> PathBuf {
    match variant {
        AoemRuntimeVariant::Core => root.join("bin").join("aoem_ffi.dll"),
        AoemRuntimeVariant::Persist => root
            .join("variants")
            .join("persist")
            .join("bin")
            .join("aoem_ffi.dll"),
        AoemRuntimeVariant::Wasm => root
            .join("variants")
            .join("wasm")
            .join("bin")
            .join("aoem_ffi.dll"),
    }
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

#[allow(dead_code)]
fn _assert_abi_struct_layout(_v: AoemCreateOptionsV1) {}

#[cfg(test)]
mod tests {
    use super::AoemCapabilityContract;
    use serde_json::json;

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
        assert!(c.msm_accel);
        assert_eq!(c.msm_backend.as_deref(), Some("bls12_381_gpu"));
        assert_eq!(c.fallback_reason_codes.len(), 2);
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
        assert!(c.msm_accel);
        assert!(c.inferred_from_legacy_fields);
    }
}
