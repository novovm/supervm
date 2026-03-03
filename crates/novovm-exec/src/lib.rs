use anyhow::{bail, Result};
use aoem_bindings::{AoemCreateOptionsV1, AoemDyn, AoemExecV2Result, AoemHandle, AoemOpV2};
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
