use anyhow::{anyhow, bail, Context, Result};
use libloading::Library;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr};
use std::fs;
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};

pub type AoemAbiVersion = unsafe extern "C" fn() -> u32;
pub type AoemVersionString = unsafe extern "C" fn() -> *const c_char;
pub type AoemGlobalInit = unsafe extern "C" fn() -> i32;
pub type AoemCapabilitiesJson = unsafe extern "C" fn() -> *const c_char;
pub type AoemRecommendParallelism = unsafe extern "C" fn(u64, u32, u64, f64) -> u32;
pub type AoemZkvmSupported = unsafe extern "C" fn() -> u32;
pub type AoemZkvmTraceFibProveVerify = unsafe extern "C" fn(u32, u64, u64) -> i32;
pub type AoemRingSignatureSupported = unsafe extern "C" fn() -> u32;
pub type AoemRingSignatureKeygenV1 =
    unsafe extern "C" fn(*mut *mut u8, *mut usize, *mut *mut u8, *mut usize) -> i32;
pub type AoemRingSignatureSignWeb30V1 = unsafe extern "C" fn(
    *const u8,
    usize,
    u32,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    u64,
    u64,
    *mut *mut u8,
    *mut usize,
) -> i32;
pub type AoemRingSignatureVerifyWeb30V1 =
    unsafe extern "C" fn(*const u8, usize, *const u8, usize, u64, u64, *mut u32) -> i32;
pub type AoemRingSignatureVerifyBatchWeb30V1 =
    unsafe extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize, *mut u32) -> i32;
pub type AoemBulletproofProveBatchV1 =
    unsafe extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize) -> i32;
pub type AoemBulletproofVerifyBatchV1 =
    unsafe extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize, *mut u32) -> i32;
pub type AoemGroth16ProveV1 = unsafe extern "C" fn(
    *const u8,
    usize,
    *mut *mut u8,
    *mut usize,
    *mut *mut u8,
    *mut usize,
    *mut *mut u8,
    *mut usize,
) -> i32;
pub type AoemGroth16ProveBatchV1 = unsafe extern "C" fn(
    *const u8,
    usize,
    *mut *mut u8,
    *mut usize,
    *mut *mut u8,
    *mut usize,
    *mut *mut u8,
    *mut usize,
) -> i32;
pub type AoemRingctProveBatchV1 =
    unsafe extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize) -> i32;
pub type AoemRingctVerifyBatchV1 =
    unsafe extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize, *mut u32) -> i32;
#[repr(C)]
pub struct AoemCreateOptionsV1 {
    pub abi_version: u32,
    pub struct_size: u32,
    pub ingress_workers: u32,
    pub flags: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AoemOpV2 {
    pub opcode: u8, // 1=read,2=write,3=add_i64,4=inc_i64
    pub flags: u8,
    pub reserved: u16,
    pub key_ptr: *const u8,
    pub key_len: u32,
    pub value_ptr: *const u8,
    pub value_len: u32,
    pub delta: i64,
    pub expect_version: u64, // u64::MAX means None
    pub plan_id: u64,        // 0 => auto
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct AoemExecV2Result {
    pub processed: u32,
    pub success: u32,
    pub failed_index: u32, // u32::MAX means none
    pub total_writes: u64,
}
pub type AoemCreate = unsafe extern "C" fn() -> *mut c_void;
pub type AoemCreateWithOptions = unsafe extern "C" fn(*const AoemCreateOptionsV1) -> *mut c_void;
pub type AoemDestroy = unsafe extern "C" fn(*mut c_void);
pub type AoemFree = unsafe extern "C" fn(*mut u8, usize);
pub type AoemExecuteOpsV2 =
    unsafe extern "C" fn(*mut c_void, *const AoemOpV2, u32, *mut AoemExecV2Result) -> i32;
pub type AoemExecuteOpsWireV1 =
    unsafe extern "C" fn(*mut c_void, *const u8, usize, *mut AoemExecV2Result) -> i32;
pub type AoemLastError = unsafe extern "C" fn(*mut c_void) -> *const c_char;

pub struct AoemDyn {
    _lib: ManuallyDrop<Library>,
    unload_on_drop: bool,
    library_path: PathBuf,
    abi_version: AoemAbiVersion,
    version_string: AoemVersionString,
    global_init: Option<AoemGlobalInit>,
    capabilities_json: AoemCapabilitiesJson,
    recommend_parallelism: Option<AoemRecommendParallelism>,
    zkvm_supported: Option<AoemZkvmSupported>,
    zkvm_trace_fib_prove_verify: Option<AoemZkvmTraceFibProveVerify>,
    ring_signature_supported: Option<AoemRingSignatureSupported>,
    ring_signature_keygen_v1: Option<AoemRingSignatureKeygenV1>,
    ring_signature_sign_web30_v1: Option<AoemRingSignatureSignWeb30V1>,
    ring_signature_verify_web30_v1: Option<AoemRingSignatureVerifyWeb30V1>,
    ring_signature_verify_batch_web30_v1: Option<AoemRingSignatureVerifyBatchWeb30V1>,
    groth16_prove_v1: Option<AoemGroth16ProveV1>,
    groth16_prove_batch_v1: Option<AoemGroth16ProveBatchV1>,
    bulletproof_prove_batch_v1: Option<AoemBulletproofProveBatchV1>,
    bulletproof_verify_batch_v1: Option<AoemBulletproofVerifyBatchV1>,
    ringct_prove_batch_v1: Option<AoemRingctProveBatchV1>,
    ringct_verify_batch_v1: Option<AoemRingctVerifyBatchV1>,
    create: AoemCreate,
    create_with_options: Option<AoemCreateWithOptions>,
    destroy: AoemDestroy,
    free: Option<AoemFree>,
    execute_ops_v2: Option<AoemExecuteOpsV2>,
    execute_ops_wire_v1: Option<AoemExecuteOpsWireV1>,
    last_error: AoemLastError,
}

pub struct AoemHandle<'a> {
    dynlib: &'a AoemDyn,
    raw: *mut c_void,
}

pub struct AoemHostHint {
    pub txs: u64,
    pub batch: u32,
    pub key_space: u64,
    pub rw: f64,
}

pub struct AoemHostAdaptiveDecision {
    pub hw_threads: usize,
    pub budget_threads: usize,
    pub recommended_threads: usize,
    pub reason: &'static str,
}

struct GlobalLaneScheduler {
    budget: AtomicUsize,
    inflight: AtomicUsize,
    lock: Mutex<()>,
    cv: Condvar,
}

pub struct GlobalLaneGuard<'a> {
    scheduler: &'a GlobalLaneScheduler,
}

static GLOBAL_LANE_SCHEDULER: OnceLock<GlobalLaneScheduler> = OnceLock::new();
type RecommendCacheKey = (u64, u32, u64, u32);
type RecommendCache = HashMap<RecommendCacheKey, usize>;
static AOEM_RECOMMEND_CACHE: OnceLock<Mutex<RecommendCache>> = OnceLock::new();
static AOEM_INSTALL_PROFILE_CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<Value>>>> =
    OnceLock::new();
static AOEM_MANIFEST_CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<Value>>>> = OnceLock::new();

const MAX_AOEM_OWNED_BUFFER_BYTES: usize = 64 * 1024 * 1024;

impl<'a> Drop for AoemHandle<'a> {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                (self.dynlib.destroy)(self.raw);
            }
            self.raw = ptr::null_mut();
        }
    }
}

impl AoemDyn {
    /// # Safety
    /// Caller must ensure the dynamic library path points to a trusted AOEM FFI build.
    pub unsafe fn load(path: impl AsRef<Path>) -> Result<Self> {
        let library_path = path.as_ref().to_path_buf();
        let lib = Library::new(path.as_ref())?;
        let abi_version: AoemAbiVersion = *lib.get::<AoemAbiVersion>(b"aoem_abi_version")?;
        let version_string: AoemVersionString =
            *lib.get::<AoemVersionString>(b"aoem_version_string")?;
        let global_init: Option<AoemGlobalInit> = lib
            .get::<AoemGlobalInit>(b"aoem_global_init")
            .ok()
            .map(|f| *f);
        let capabilities_json: AoemCapabilitiesJson =
            *lib.get::<AoemCapabilitiesJson>(b"aoem_capabilities_json")?;
        let recommend_parallelism: Option<AoemRecommendParallelism> = lib
            .get::<AoemRecommendParallelism>(b"aoem_recommend_parallelism")
            .ok()
            .map(|f| *f);
        let zkvm_supported: Option<AoemZkvmSupported> = lib
            .get::<AoemZkvmSupported>(b"aoem_zkvm_supported")
            .ok()
            .map(|f| *f);
        let zkvm_trace_fib_prove_verify: Option<AoemZkvmTraceFibProveVerify> = lib
            .get::<AoemZkvmTraceFibProveVerify>(b"aoem_zkvm_trace_fib_prove_verify")
            .ok()
            .map(|f| *f);
        let ring_signature_supported: Option<AoemRingSignatureSupported> = lib
            .get::<AoemRingSignatureSupported>(b"aoem_ring_signature_supported")
            .ok()
            .map(|f| *f);
        let ring_signature_keygen_v1: Option<AoemRingSignatureKeygenV1> = lib
            .get::<AoemRingSignatureKeygenV1>(b"aoem_ring_signature_keygen_v1")
            .ok()
            .map(|f| *f);
        let ring_signature_sign_web30_v1: Option<AoemRingSignatureSignWeb30V1> = lib
            .get::<AoemRingSignatureSignWeb30V1>(b"aoem_ring_signature_sign_web30_v1")
            .ok()
            .map(|f| *f);
        let ring_signature_verify_web30_v1: Option<AoemRingSignatureVerifyWeb30V1> = lib
            .get::<AoemRingSignatureVerifyWeb30V1>(b"aoem_ring_signature_verify_web30_v1")
            .ok()
            .map(|f| *f);
        let ring_signature_verify_batch_web30_v1: Option<AoemRingSignatureVerifyBatchWeb30V1> = lib
            .get::<AoemRingSignatureVerifyBatchWeb30V1>(
                b"aoem_ring_signature_verify_batch_web30_v1",
            )
            .ok()
            .map(|f| *f);
        let groth16_prove_v1: Option<AoemGroth16ProveV1> = lib
            .get::<AoemGroth16ProveV1>(b"aoem_groth16_prove_v1")
            .ok()
            .map(|f| *f);
        let groth16_prove_batch_v1: Option<AoemGroth16ProveBatchV1> = lib
            .get::<AoemGroth16ProveBatchV1>(b"aoem_groth16_prove_batch_v1")
            .ok()
            .map(|f| *f);
        let bulletproof_prove_batch_v1: Option<AoemBulletproofProveBatchV1> = lib
            .get::<AoemBulletproofProveBatchV1>(b"aoem_bulletproof_prove_batch_v1")
            .ok()
            .map(|f| *f);
        let bulletproof_verify_batch_v1: Option<AoemBulletproofVerifyBatchV1> = lib
            .get::<AoemBulletproofVerifyBatchV1>(b"aoem_bulletproof_verify_batch_v1")
            .ok()
            .map(|f| *f);
        let ringct_prove_batch_v1: Option<AoemRingctProveBatchV1> = lib
            .get::<AoemRingctProveBatchV1>(b"aoem_ringct_prove_batch_v1")
            .ok()
            .map(|f| *f);
        let ringct_verify_batch_v1: Option<AoemRingctVerifyBatchV1> = lib
            .get::<AoemRingctVerifyBatchV1>(b"aoem_ringct_verify_batch_v1")
            .ok()
            .map(|f| *f);
        let create: AoemCreate = *lib.get::<AoemCreate>(b"aoem_create")?;
        let create_with_options: Option<AoemCreateWithOptions> = lib
            .get::<AoemCreateWithOptions>(b"aoem_create_with_options")
            .ok()
            .map(|f| *f);
        let destroy: AoemDestroy = *lib.get::<AoemDestroy>(b"aoem_destroy")?;
        let free: Option<AoemFree> = lib.get::<AoemFree>(b"aoem_free").ok().map(|f| *f);
        let execute_ops_v2: Option<AoemExecuteOpsV2> = lib
            .get::<AoemExecuteOpsV2>(b"aoem_execute_ops_v2")
            .ok()
            .map(|f| *f);
        let execute_ops_wire_v1: Option<AoemExecuteOpsWireV1> = lib
            .get::<AoemExecuteOpsWireV1>(b"aoem_execute_ops_wire_v1")
            .ok()
            .map(|f| *f);
        let last_error: AoemLastError = *lib.get::<AoemLastError>(b"aoem_last_error")?;

        let dynlib = Self {
            _lib: ManuallyDrop::new(lib),
            unload_on_drop: should_unload_dll_on_drop(),
            library_path,
            abi_version,
            version_string,
            global_init,
            capabilities_json,
            recommend_parallelism,
            zkvm_supported,
            zkvm_trace_fib_prove_verify,
            ring_signature_supported,
            ring_signature_keygen_v1,
            ring_signature_sign_web30_v1,
            ring_signature_verify_web30_v1,
            ring_signature_verify_batch_web30_v1,
            groth16_prove_v1,
            groth16_prove_batch_v1,
            bulletproof_prove_batch_v1,
            bulletproof_verify_batch_v1,
            ringct_prove_batch_v1,
            ringct_verify_batch_v1,
            create,
            create_with_options,
            destroy,
            free,
            execute_ops_v2,
            execute_ops_wire_v1,
            last_error,
        };

        dynlib.run_global_init()?;
        // Startup hard gate: reject non-V1 ABI or non-V2-capable DLL immediately.
        dynlib.verify_startup_contract()?;
        Ok(dynlib)
    }

    fn run_global_init(&self) -> Result<()> {
        if let Some(f) = self.global_init {
            let rc = unsafe { f() };
            if rc != 0 {
                bail!("AOEM global init failed: rc={rc}");
            }
        }
        Ok(())
    }

    fn verify_startup_contract(&self) -> Result<()> {
        self.verify_manifest_hash()?;
        let abi = self.abi();
        if abi != 1 {
            bail!("AOEM ABI mismatch at load: expected 1, got {}", abi);
        }
        let caps = self
            .capabilities()
            .context("load-time capabilities json parse failed")?;
        let execute_ops_v2 = caps
            .get("execute_ops_v2")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !execute_ops_v2 {
            bail!("AOEM startup gate failed: execute_ops_v2=false");
        }
        Ok(())
    }

    fn verify_manifest_hash(&self) -> Result<()> {
        let manifest_path = default_manifest_path_for_dll(self.library_path());
        let required = parse_bool_env("AOEM_DLL_MANIFEST_REQUIRED").unwrap_or(false);

        if !manifest_path.exists() {
            if required {
                bail!(
                    "AOEM manifest required but not found: {}",
                    manifest_path.display()
                );
            }
            return Ok(());
        }

        let manifest = load_manifest_json(&manifest_path)
            .with_context(|| format!("failed to parse manifest: {}", manifest_path.display()))?;
        let Some(entries) = manifest.get("entries").and_then(|v| v.as_array()) else {
            bail!("invalid manifest format: missing entries array");
        };

        let variant = infer_variant_from_dll_path(self.library_path());
        let variant_entries: Vec<&Value> = entries
            .iter()
            .filter(|item| {
                item.get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.eq_ignore_ascii_case(variant))
                    .unwrap_or(false)
            })
            .collect();
        let lib_path_norm =
            normalize_path_for_match(self.library_path().to_string_lossy().as_ref());
        let entry = variant_entries
            .iter()
            .copied()
            .find(|item| {
                item.get("dll")
                    .and_then(|v| v.as_str())
                    .map(|dll| {
                        let rel = normalize_path_for_match(dll);
                        !rel.is_empty()
                            && (lib_path_norm.ends_with(&rel)
                                || lib_path_norm.ends_with(&format!("/{rel}")))
                    })
                    .unwrap_or(false)
            })
            .or_else(|| variant_entries.first().copied());
        let Some(entry) = entry else {
            if required {
                bail!("manifest entry not found for variant={variant}");
            }
            return Ok(());
        };

        let Some(expected_hash) = entry.get("sha256").and_then(|v| v.as_str()) else {
            bail!("invalid manifest entry: missing sha256 for variant={variant}");
        };
        let actual_hash = file_sha256(self.library_path())
            .with_context(|| format!("failed to hash DLL: {}", self.library_path().display()))?;
        if !expected_hash.eq_ignore_ascii_case(&actual_hash) {
            bail!(
                "AOEM DLL hash mismatch for variant={variant}: expected={}, actual={}",
                expected_hash,
                actual_hash
            );
        }

        if let Some(abi_expected) = entry.get("abi_expected").and_then(|v| v.as_u64()) {
            let abi_actual = self.abi() as u64;
            if abi_expected != abi_actual {
                bail!(
                    "AOEM manifest ABI mismatch for variant={variant}: expected={}, actual={}",
                    abi_expected,
                    abi_actual
                );
            }
        }

        if let Some(req_caps) = entry.get("capabilities_required") {
            let caps = self.capabilities()?;
            if !json_is_subset(req_caps, &caps) {
                bail!("AOEM manifest capabilities_required check failed for variant={variant}");
            }
        }

        Ok(())
    }

    pub fn abi(&self) -> u32 {
        unsafe { (self.abi_version)() }
    }

    pub fn version(&self) -> String {
        unsafe { cstr_to_string((self.version_string)()) }
            .unwrap_or_else(|| "<invalid>".to_string())
    }

    pub fn capabilities(&self) -> Result<Value> {
        let text = unsafe { cstr_to_string((self.capabilities_json)()) }
            .ok_or_else(|| anyhow!("aoem_capabilities_json returned null"))?;
        let parsed: Value = serde_json::from_str(&text)
            .with_context(|| format!("invalid capabilities json: {text}"))?;
        Ok(parsed)
    }

    pub fn create_handle(&self) -> Result<AoemHandle<'_>> {
        self.create_handle_with_ingress_workers(None)
    }

    pub fn create_handle_with_ingress_workers(
        &self,
        ingress_workers: Option<u32>,
    ) -> Result<AoemHandle<'_>> {
        let raw = if let (Some(create_with_options), Some(workers)) =
            (self.create_with_options, ingress_workers)
        {
            let opts = AoemCreateOptionsV1 {
                abi_version: 1,
                struct_size: std::mem::size_of::<AoemCreateOptionsV1>() as u32,
                ingress_workers: workers.max(1),
                flags: 0,
            };
            unsafe { create_with_options(&opts as *const AoemCreateOptionsV1) }
        } else {
            unsafe { (self.create)() }
        };
        if raw.is_null() {
            bail!("aoem_create returned null");
        }
        Ok(AoemHandle { dynlib: self, raw })
    }

    pub fn library_path(&self) -> &Path {
        &self.library_path
    }

    pub fn runtime_profile_path(&self) -> PathBuf {
        default_runtime_profile_path_for_dll(self.library_path())
    }

    pub fn supports_execute_ops_v2(&self) -> bool {
        self.execute_ops_v2.is_some()
    }

    pub fn supports_execute_ops_wire_v1(&self) -> bool {
        self.execute_ops_wire_v1.is_some()
    }

    /// True when AOEM FFI exports both zkVM probe symbols.
    pub fn supports_zkvm_probe(&self) -> bool {
        self.zkvm_supported.is_some() && self.zkvm_trace_fib_prove_verify.is_some()
    }

    /// True when AOEM FFI exports both ring-signature probe and verify symbols.
    pub fn supports_ring_signature_verify(&self) -> bool {
        self.ring_signature_supported.is_some() && self.ring_signature_verify_web30_v1.is_some()
    }

    pub fn supports_ring_signature_keygen_v1(&self) -> bool {
        self.ring_signature_keygen_v1.is_some() && self.free.is_some()
    }

    /// True when AOEM FFI exports Web30-compatible ring-signature sign symbols.
    pub fn supports_ring_signature_sign_web30_v1(&self) -> bool {
        self.ring_signature_sign_web30_v1.is_some() && self.free.is_some()
    }

    pub fn supports_ring_signature_verify_batch_web30_v1(&self) -> bool {
        self.ring_signature_verify_batch_web30_v1.is_some()
    }

    pub fn supports_groth16_prove_v1(&self) -> bool {
        self.groth16_prove_v1.is_some()
    }

    pub fn supports_groth16_prove_batch_v1(&self) -> bool {
        self.groth16_prove_batch_v1.is_some()
    }

    pub fn supports_groth16_prove_auto_path(&self) -> bool {
        self.groth16_prove_batch_v1.is_some() || self.groth16_prove_v1.is_some()
    }

    pub fn supports_bulletproof_batch_v1(&self) -> bool {
        self.bulletproof_prove_batch_v1.is_some() && self.bulletproof_verify_batch_v1.is_some()
    }

    pub fn supports_ringct_batch_v1(&self) -> bool {
        self.ringct_prove_batch_v1.is_some() && self.ringct_verify_batch_v1.is_some()
    }

    pub fn supports_privacy_batch_v1(&self) -> bool {
        self.supports_ring_signature_verify_batch_web30_v1()
            && self.supports_bulletproof_batch_v1()
            && self.supports_ringct_batch_v1()
    }

    /// Returns AOEM-provided zkVM capability bit from exported symbol.
    /// `None` means the loaded AOEM library does not export this symbol.
    pub fn zkvm_supported_flag(&self) -> Option<bool> {
        self.zkvm_supported.map(|f| unsafe { f() != 0 })
    }

    /// Executes AOEM built-in Trace/Fibonacci prove+verify probe and returns raw rc.
    /// `None` means the loaded AOEM library does not export this symbol.
    pub fn zkvm_trace_fib_probe_rc(
        &self,
        rounds: u32,
        witness_a: u64,
        witness_b: u64,
    ) -> Option<i32> {
        self.zkvm_trace_fib_prove_verify
            .map(|f| unsafe { f(rounds, witness_a, witness_b) })
    }

    /// Returns AOEM-provided ring signature capability bit from exported symbol.
    /// `None` means the loaded AOEM library does not export this symbol.
    pub fn ring_signature_supported_flag(&self) -> Option<bool> {
        self.ring_signature_supported.map(|f| unsafe { f() != 0 })
    }

    fn copy_aoem_owned_bytes(&self, ptr: *mut u8, len: usize, ctx: &str) -> Result<Vec<u8>> {
        let free_fn = self
            .free
            .ok_or_else(|| anyhow!("aoem_free not found in loaded DLL"))?;
        if ptr.is_null() {
            if len == 0 {
                return Ok(Vec::new());
            }
            bail!("{ctx} returned null buffer");
        }
        if len > MAX_AOEM_OWNED_BUFFER_BYTES {
            unsafe { free_fn(ptr, len) };
            bail!(
                "{ctx} returned oversized AOEM buffer: len={} > max={}",
                len,
                MAX_AOEM_OWNED_BUFFER_BYTES
            );
        }
        if len == 0 {
            unsafe { free_fn(ptr, len) };
            return Ok(Vec::new());
        }
        let out = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) }.to_vec();
        unsafe { free_fn(ptr, len) };
        Ok(out)
    }

    fn decode_len_prefixed_blob_list_wire_v1<'b>(
        &self,
        input: &'b [u8],
        label: &str,
    ) -> Result<Vec<&'b [u8]>> {
        if input.len() < 4 {
            bail!("{label} wire too short");
        }
        let mut cursor = 0usize;
        let count = u32::from_le_bytes([input[0], input[1], input[2], input[3]]) as usize;
        cursor += 4;
        if count == 0 {
            bail!("{label} wire has zero items");
        }
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            if cursor + 4 > input.len() {
                bail!("{label} wire truncated on length prefix");
            }
            let len = u32::from_le_bytes([
                input[cursor],
                input[cursor + 1],
                input[cursor + 2],
                input[cursor + 3],
            ]) as usize;
            cursor += 4;
            if len == 0 {
                bail!("{label} wire contains empty item");
            }
            if cursor + len > input.len() {
                bail!("{label} wire truncated on payload");
            }
            out.push(&input[cursor..cursor + len]);
            cursor += len;
        }
        if cursor != input.len() {
            bail!("{label} wire has trailing bytes");
        }
        Ok(out)
    }

    fn encode_len_prefixed_blob_list_wire_v1(
        &self,
        items: &[Vec<u8>],
        label: &str,
    ) -> Result<Vec<u8>> {
        if items.is_empty() {
            bail!("{label} wire requires at least one item");
        }
        let mut out =
            Vec::with_capacity(4 + items.iter().map(|item| 4 + item.len()).sum::<usize>());
        out.extend_from_slice(&(items.len() as u32).to_le_bytes());
        for item in items {
            if item.is_empty() {
                bail!("{label} wire does not allow empty item");
            }
            out.extend_from_slice(&(item.len() as u32).to_le_bytes());
            out.extend_from_slice(item);
        }
        Ok(out)
    }

    /// Generates a ring-signature public/secret keypair via AOEM FFI.
    pub fn ring_signature_keygen_v1(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        let Some(keygen_fn) = self.ring_signature_keygen_v1 else {
            bail!("aoem_ring_signature_keygen_v1 not found in loaded DLL");
        };
        let mut public_key_ptr: *mut u8 = ptr::null_mut();
        let mut public_key_len = 0usize;
        let mut secret_key_ptr: *mut u8 = ptr::null_mut();
        let mut secret_key_len = 0usize;
        let rc = unsafe {
            keygen_fn(
                &mut public_key_ptr as *mut *mut u8,
                &mut public_key_len as *mut usize,
                &mut secret_key_ptr as *mut *mut u8,
                &mut secret_key_len as *mut usize,
            )
        };
        if rc != 0 {
            bail!("aoem_ring_signature_keygen_v1 failed: rc={rc}");
        }
        let public_key =
            self.copy_aoem_owned_bytes(public_key_ptr, public_key_len, "ring keygen public key")?;
        let secret_key =
            self.copy_aoem_owned_bytes(secret_key_ptr, secret_key_len, "ring keygen secret key")?;
        Ok((public_key, secret_key))
    }

    /// Signs a Web30 ring-signature payload via AOEM FFI and returns JSON bytes.
    pub fn ring_signature_sign_web30_v1(
        &self,
        ring_json: &[u8],
        secret_index: u32,
        secret_key: &[u8],
        public_key: &[u8],
        message: &[u8],
        amount: u128,
    ) -> Result<Vec<u8>> {
        let Some(sign_fn) = self.ring_signature_sign_web30_v1 else {
            bail!("aoem_ring_signature_sign_web30_v1 not found in loaded DLL");
        };
        if ring_json.is_empty() {
            bail!("ring-signature ring_json must not be empty");
        }
        if secret_key.is_empty() {
            bail!("ring-signature secret_key must not be empty");
        }
        if public_key.is_empty() {
            bail!("ring-signature public_key must not be empty");
        }
        let mut signature_ptr: *mut u8 = ptr::null_mut();
        let mut signature_len = 0usize;
        let amount_lo = amount as u64;
        let amount_hi = (amount >> 64) as u64;
        let rc = unsafe {
            sign_fn(
                ring_json.as_ptr(),
                ring_json.len(),
                secret_index,
                secret_key.as_ptr(),
                secret_key.len(),
                public_key.as_ptr(),
                public_key.len(),
                message.as_ptr(),
                message.len(),
                amount_lo,
                amount_hi,
                &mut signature_ptr as *mut *mut u8,
                &mut signature_len as *mut usize,
            )
        };
        if rc != 0 {
            bail!("aoem_ring_signature_sign_web30_v1 failed: rc={rc}");
        }
        self.copy_aoem_owned_bytes(signature_ptr, signature_len, "ring sign signature")
    }

    /// Verifies a web30 ring-signature payload via AOEM FFI.
    /// Signature payload must be JSON bytes following AOEM web30 schema.
    pub fn ring_signature_verify_web30_v1(
        &self,
        signature_json: &[u8],
        message: &[u8],
        amount: u128,
    ) -> Result<bool> {
        let Some(verify_fn) = self.ring_signature_verify_web30_v1 else {
            bail!(
                "aoem_ring_signature_verify_web30_v1 not found in loaded DLL (requires AOEM ring-signature ABI build)"
            );
        };
        if signature_json.is_empty() {
            bail!("ring-signature payload must not be empty");
        }
        let mut out_valid = 0u32;
        let amount_lo = amount as u64;
        let amount_hi = (amount >> 64) as u64;
        let rc = unsafe {
            verify_fn(
                signature_json.as_ptr(),
                signature_json.len(),
                message.as_ptr(),
                message.len(),
                amount_lo,
                amount_hi,
                &mut out_valid as *mut u32,
            )
        };
        if rc != 0 {
            bail!("aoem_ring_signature_verify_web30_v1 failed: rc={rc}");
        }
        Ok(out_valid != 0)
    }

    /// Groth16 batch prove entry for host-side high-throughput usage.
    /// Input wire format:
    /// - [count:u32_le][len:u32_le][witness_bytes]...
    /// - witness bytes are same as single prove: 24 bytes [a:u64][b:u64][c:u64].
    ///
    /// Return:
    /// - vk bytes (shared)
    /// - proofs wire: [count:u32_le][len:u32_le][proof]...
    /// - public inputs wire: [count:u32_le][len:u32_le][FR_VEC_WIRE_V1]...
    ///
    /// Auto-path behavior:
    /// - Prefer `aoem_groth16_prove_batch_v1` when exported by DLL.
    /// - Fallback to `aoem_groth16_prove_v1` loop when batch symbol is missing.
    pub fn groth16_prove_batch_v1(
        &self,
        witnesses_wire: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        if witnesses_wire.is_empty() {
            bail!("groth16 witnesses wire must not be empty");
        }

        if let Some(batch_fn) = self.groth16_prove_batch_v1 {
            let mut vk_ptr: *mut u8 = ptr::null_mut();
            let mut vk_len = 0usize;
            let mut proofs_wire_ptr: *mut u8 = ptr::null_mut();
            let mut proofs_wire_len = 0usize;
            let mut inputs_wire_ptr: *mut u8 = ptr::null_mut();
            let mut inputs_wire_len = 0usize;
            let rc = unsafe {
                batch_fn(
                    witnesses_wire.as_ptr(),
                    witnesses_wire.len(),
                    &mut vk_ptr as *mut *mut u8,
                    &mut vk_len as *mut usize,
                    &mut proofs_wire_ptr as *mut *mut u8,
                    &mut proofs_wire_len as *mut usize,
                    &mut inputs_wire_ptr as *mut *mut u8,
                    &mut inputs_wire_len as *mut usize,
                )
            };
            if rc != 0 {
                bail!("aoem_groth16_prove_batch_v1 failed: rc={rc}");
            }
            let vk = self.copy_aoem_owned_bytes(vk_ptr, vk_len, "groth16 batch vk")?;
            let proofs_wire = self.copy_aoem_owned_bytes(
                proofs_wire_ptr,
                proofs_wire_len,
                "groth16 batch proofs wire",
            )?;
            let public_inputs_wire = self.copy_aoem_owned_bytes(
                inputs_wire_ptr,
                inputs_wire_len,
                "groth16 batch public inputs wire",
            )?;
            return Ok((vk, proofs_wire, public_inputs_wire));
        }

        let Some(single_fn) = self.groth16_prove_v1 else {
            bail!("aoem_groth16_prove_batch_v1/aoem_groth16_prove_v1 not found in loaded DLL");
        };

        let witness_items =
            self.decode_len_prefixed_blob_list_wire_v1(witnesses_wire, "groth16 witness batch")?;
        let mut shared_vk: Option<Vec<u8>> = None;
        let mut proofs = Vec::with_capacity(witness_items.len());
        let mut inputs = Vec::with_capacity(witness_items.len());
        for witness in witness_items {
            let mut vk_ptr: *mut u8 = ptr::null_mut();
            let mut vk_len = 0usize;
            let mut proof_ptr: *mut u8 = ptr::null_mut();
            let mut proof_len = 0usize;
            let mut input_ptr: *mut u8 = ptr::null_mut();
            let mut input_len = 0usize;
            let rc = unsafe {
                single_fn(
                    witness.as_ptr(),
                    witness.len(),
                    &mut vk_ptr as *mut *mut u8,
                    &mut vk_len as *mut usize,
                    &mut proof_ptr as *mut *mut u8,
                    &mut proof_len as *mut usize,
                    &mut input_ptr as *mut *mut u8,
                    &mut input_len as *mut usize,
                )
            };
            if rc != 0 {
                bail!("aoem_groth16_prove_v1 fallback failed: rc={rc}");
            }
            let vk = self.copy_aoem_owned_bytes(vk_ptr, vk_len, "groth16 fallback vk")?;
            if shared_vk.is_none() {
                shared_vk = Some(vk);
            }
            proofs.push(self.copy_aoem_owned_bytes(
                proof_ptr,
                proof_len,
                "groth16 fallback proof",
            )?);
            inputs.push(self.copy_aoem_owned_bytes(
                input_ptr,
                input_len,
                "groth16 fallback inputs",
            )?);
        }
        let vk = shared_vk.ok_or_else(|| anyhow!("groth16 witness batch is empty"))?;
        let proofs_wire =
            self.encode_len_prefixed_blob_list_wire_v1(&proofs, "groth16 proofs batch")?;
        let public_inputs_wire =
            self.encode_len_prefixed_blob_list_wire_v1(&inputs, "groth16 public inputs batch")?;
        Ok((vk, proofs_wire, public_inputs_wire))
    }

    pub fn recommend_parallelism(
        &self,
        txs: u64,
        batch: u32,
        key_space: u64,
        rw: f64,
    ) -> Option<usize> {
        let f = self.recommend_parallelism?;
        let rec = unsafe { f(txs, batch, key_space, rw) } as usize;
        Some(rec.max(1))
    }

    pub fn smoke(&self) -> Result<Value> {
        if self.abi() != 1 {
            bail!("AOEM ABI mismatch: expected 1, got {}", self.abi());
        }

        let handle = self.create_handle()?;
        let mut key = 107u64.to_le_bytes();
        let mut value = 1u64.to_le_bytes();
        let op = AoemOpV2 {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key_ptr: key.as_mut_ptr(),
            key_len: key.len() as u32,
            value_ptr: value.as_mut_ptr(),
            value_len: value.len() as u32,
            delta: 0,
            expect_version: u64::MAX,
            plan_id: 1,
        };
        let res = handle.execute_ops_v2(&[op])?;
        Ok(serde_json::json!({
            "processed": res.processed,
            "success": res.success,
            "failed_index": res.failed_index,
            "total_writes": res.total_writes
        }))
    }
}

impl Drop for AoemDyn {
    fn drop(&mut self) {
        if self.unload_on_drop {
            unsafe {
                ManuallyDrop::drop(&mut self._lib);
            }
        }
    }
}

pub fn default_runtime_profile_path_for_dll(dll_path: &Path) -> PathBuf {
    if let Ok(override_path) = std::env::var("AOEM_RUNTIME_PROFILE") {
        let trimmed = override_path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let parent = dll_path.parent().unwrap_or_else(|| Path::new("."));
    let parent_name = parent
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if parent_name == "bin" {
        let level1 = parent.parent().unwrap_or(parent);
        let level1_name = level1
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if level1_name == "persist" || level1_name == "wasm" {
            let level2 = level1.parent().unwrap_or(level1); // variants
            let level2_name = level2
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if level2_name == "variants" {
                let root = level2.parent().unwrap_or(level2);
                return root.join("config").join("aoem-runtime-profile.json");
            }
        }
        return level1.join("config").join("aoem-runtime-profile.json");
    }
    parent.join("aoem-runtime-profile.json")
}

pub fn default_manifest_path_for_dll(dll_path: &Path) -> PathBuf {
    if let Ok(override_path) = std::env::var("AOEM_DLL_MANIFEST") {
        let trimmed = override_path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let parent = dll_path.parent().unwrap_or_else(|| Path::new("."));
    let parent_name = parent
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if parent_name == "bin" {
        let level1 = parent.parent().unwrap_or(parent);
        let level1_name = level1
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if level1_name == "persist" || level1_name == "wasm" {
            let level2 = level1.parent().unwrap_or(level1); // variants
            let level2_name = level2
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if level2_name == "variants" {
                let root = level2.parent().unwrap_or(level2);
                return root.join("manifest").join("aoem-manifest.json");
            }
        }
        return level1.join("manifest").join("aoem-manifest.json");
    }
    parent.join("manifest").join("aoem-manifest.json")
}

fn infer_variant_from_dll_path(dll_path: &Path) -> &'static str {
    let p = normalize_path_for_match(dll_path.to_string_lossy().as_ref());
    if p.contains("/variants/persist/") {
        "persist"
    } else if p.contains("/variants/wasm/") {
        "wasm"
    } else {
        "core"
    }
}

fn normalize_path_for_match(path: &str) -> String {
    path.to_ascii_lowercase().replace('\\', "/")
}

fn load_manifest_json(path: &Path) -> Result<Value> {
    let cache = AOEM_MANIFEST_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    let entry = guard.entry(path.to_path_buf()).or_insert_with(|| {
        let text = fs::read_to_string(path).ok()?;
        serde_json::from_str::<Value>(&text).ok()
    });
    entry
        .clone()
        .ok_or_else(|| anyhow!("manifest parse failed: {}", path.display()))
}

fn parse_bool_env(name: &str) -> Option<bool> {
    std::env::var(name).ok().and_then(|v| {
        let s = v.trim();
        if s == "1" || s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("on") {
            Some(true)
        } else if s == "0" || s.eq_ignore_ascii_case("false") || s.eq_ignore_ascii_case("off") {
            Some(false)
        } else {
            None
        }
    })
}

fn should_unload_dll_on_drop() -> bool {
    if let Some(v) = parse_bool_env("AOEM_FFI_UNLOAD_DLL") {
        return v;
    }
    !cfg!(windows)
}

fn file_sha256(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    let digest = Sha256::digest(&bytes);
    Ok(to_hex_lower(&digest))
}

fn to_hex_lower(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(data.len() * 2);
    for b in data {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn json_is_subset(required: &Value, actual: &Value) -> bool {
    match (required, actual) {
        (Value::Object(r), Value::Object(a)) => r
            .iter()
            .all(|(k, rv)| a.get(k).map(|av| json_is_subset(rv, av)).unwrap_or(false)),
        (Value::Array(r), Value::Array(a)) => r.iter().all(|rv| a.iter().any(|av| av == rv)),
        _ => required == actual,
    }
}

fn hardware_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
}

fn parse_usize_env(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
}

fn default_budget_threads() -> usize {
    let hw = hardware_threads();
    let reserve = if hw >= 8 { 2 } else { 1 };
    hw.saturating_sub(reserve).max(1)
}

fn profile_condition_u64(item: &Value, key: &str) -> Option<u64> {
    item.get(key).and_then(|v| v.as_u64())
}

fn profile_condition_f64(item: &Value, key: &str) -> Option<f64> {
    item.get(key).and_then(|v| v.as_f64())
}

fn profile_matches(item: &Value, hint: &AoemHostHint) -> bool {
    if let Some(v) = profile_condition_u64(item, "min_txs") {
        if hint.txs < v {
            return false;
        }
    }
    if let Some(v) = profile_condition_u64(item, "max_txs") {
        if hint.txs > v {
            return false;
        }
    }
    if let Some(v) = profile_condition_u64(item, "min_batch") {
        if (hint.batch as u64) < v {
            return false;
        }
    }
    if let Some(v) = profile_condition_u64(item, "max_batch") {
        if hint.batch as u64 > v {
            return false;
        }
    }
    if let Some(v) = profile_condition_u64(item, "min_key_space") {
        if hint.key_space < v {
            return false;
        }
    }
    if let Some(v) = profile_condition_u64(item, "max_key_space") {
        if hint.key_space > v {
            return false;
        }
    }
    if let Some(v) = profile_condition_f64(item, "min_rw") {
        if hint.rw < v {
            return false;
        }
    }
    if let Some(v) = profile_condition_f64(item, "max_rw") {
        if hint.rw > v {
            return false;
        }
    }
    true
}

fn profile_recommended_threads(profile: &Value, hint: &AoemHostHint) -> Option<usize> {
    if let Some(items) = profile.get("profiles").and_then(|v| v.as_array()) {
        for item in items {
            if !profile_matches(item, hint) {
                continue;
            }
            if let Some(threads) = item.get("threads").and_then(|v| v.as_u64()) {
                return Some(threads as usize);
            }
        }
    }

    if let Some(threads) = profile
        .get("threads")
        .and_then(|v| v.get("default"))
        .and_then(|v| v.as_u64())
    {
        return Some(threads as usize);
    }

    profile
        .get("recommended_threads")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
}

fn select_profile_for_variant<'a>(profile: &'a Value, variant: &str) -> &'a Value {
    if let Some(variants) = profile.get("variants").and_then(|v| v.as_object()) {
        if let Some(selected) = variants.get(variant) {
            return selected;
        }
        if let Some(default_selected) = variants.get("default") {
            return default_selected;
        }
    }
    profile
}

fn recommend_threads_from_install_profile(
    dynlib: &AoemDyn,
    hint: &AoemHostHint,
    budget: usize,
) -> Option<usize> {
    let path = dynlib.runtime_profile_path();
    let cache = AOEM_INSTALL_PROFILE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    let entry = guard.entry(path.clone()).or_insert_with(|| {
        let text = fs::read_to_string(&path).ok()?;
        serde_json::from_str::<Value>(&text).ok()
    });
    let parsed = entry.as_ref()?;
    let variant = infer_variant_from_dll_path(dynlib.library_path());
    let variant_profile = select_profile_for_variant(parsed, variant);
    let rec = profile_recommended_threads(variant_profile, hint)?;
    Some(rec.min(budget).max(1))
}

fn init_global_lane_scheduler() -> &'static GlobalLaneScheduler {
    GLOBAL_LANE_SCHEDULER.get_or_init(|| {
        let budget = parse_usize_env("AOEM_FFI_GLOBAL_BUDGET")
            .unwrap_or_else(default_budget_threads)
            .max(1);
        GlobalLaneScheduler {
            budget: AtomicUsize::new(budget),
            inflight: AtomicUsize::new(0),
            lock: Mutex::new(()),
            cv: Condvar::new(),
        }
    })
}

pub fn set_global_parallel_budget(budget: usize) {
    let scheduler = init_global_lane_scheduler();
    scheduler.budget.store(budget.max(1), Ordering::Relaxed);
    scheduler.cv.notify_all();
}

pub fn global_parallel_budget() -> usize {
    init_global_lane_scheduler().budget.load(Ordering::Relaxed)
}

pub fn acquire_global_lane() -> GlobalLaneGuard<'static> {
    let scheduler = init_global_lane_scheduler();
    let mut guard = scheduler.lock.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        let inflight = scheduler.inflight.load(Ordering::Relaxed);
        let budget = scheduler.budget.load(Ordering::Relaxed).max(1);
        if inflight < budget {
            scheduler.inflight.store(inflight + 1, Ordering::Relaxed);
            break;
        }
        guard = scheduler.cv.wait(guard).unwrap_or_else(|e| e.into_inner());
    }
    drop(guard);
    GlobalLaneGuard { scheduler }
}

impl Drop for GlobalLaneGuard<'_> {
    fn drop(&mut self) {
        let prev = self.scheduler.inflight.fetch_sub(1, Ordering::Relaxed);
        if prev > 0 {
            self.scheduler.cv.notify_one();
        }
    }
}

pub fn recommend_threads_auto(hint: &AoemHostHint) -> AoemHostAdaptiveDecision {
    let hw = hardware_threads();
    let budget = global_parallel_budget().min(hw).max(1);
    let mut rec = budget;
    let mut reason = "throughput_default";

    if hint.txs <= 100_000 {
        rec = rec.min(4);
        reason = "small_txs";
    } else if hint.batch <= 256 {
        rec = rec.min((budget / 2).max(1));
        reason = "small_batch";
    } else if hint.key_space <= 256 && hint.rw >= 0.5 {
        rec = rec.min((budget * 3 / 4).max(1));
        reason = "high_contention_keyspace";
    }

    AoemHostAdaptiveDecision {
        hw_threads: hw,
        budget_threads: budget,
        recommended_threads: rec.max(1),
        reason,
    }
}

pub fn recommend_threads_from_aoem(
    dynlib: &AoemDyn,
    hint: &AoemHostHint,
) -> AoemHostAdaptiveDecision {
    let hw = hardware_threads();
    let budget = global_parallel_budget().min(hw).max(1);
    if let Some(rec) = recommend_threads_from_install_profile(dynlib, hint, budget) {
        return AoemHostAdaptiveDecision {
            hw_threads: hw,
            budget_threads: budget,
            recommended_threads: rec,
            reason: "aoem_install_profile",
        };
    }

    let rw_key = (hint.rw.clamp(0.0, 1.0) * 1000.0).round() as u32;
    let key = (hint.txs, hint.batch, hint.key_space, rw_key);
    let cache = AOEM_RECOMMEND_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(&rec) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return AoemHostAdaptiveDecision {
            hw_threads: hw,
            budget_threads: budget,
            recommended_threads: rec.min(budget).max(1),
            reason: "aoem_ffi_cache",
        };
    }

    if let Some(rec) = dynlib.recommend_parallelism(hint.txs, hint.batch, hint.key_space, hint.rw) {
        cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key, rec);
        return AoemHostAdaptiveDecision {
            hw_threads: hw,
            budget_threads: budget,
            recommended_threads: rec.min(budget).max(1),
            reason: "aoem_ffi",
        };
    }

    let mut fallback = budget;
    if hint.txs <= 100_000 {
        fallback = fallback.min(4);
    }
    AoemHostAdaptiveDecision {
        hw_threads: hw,
        budget_threads: budget,
        recommended_threads: fallback.max(1),
        reason: "ffi_missing_fallback",
    }
}

impl<'a> AoemHandle<'a> {
    pub fn execute_ops_v2(&self, ops: &[AoemOpV2]) -> Result<AoemExecV2Result> {
        let Some(exec_v2) = self.dynlib.execute_ops_v2 else {
            bail!("aoem_execute_ops_v2 not found in loaded DLL (requires AOEM FFI V2 build)");
        };
        if ops.len() > u32::MAX as usize {
            bail!("aoem_execute_ops_v2 input too large: {} ops", ops.len());
        }
        let mut result = AoemExecV2Result {
            failed_index: u32::MAX,
            ..AoemExecV2Result::default()
        };
        let rc = unsafe {
            exec_v2(
                self.raw,
                ops.as_ptr(),
                ops.len() as u32,
                &mut result as *mut AoemExecV2Result,
            )
        };
        if rc != 0 {
            let err = unsafe { cstr_to_string((self.dynlib.last_error)(self.raw)) }
                .unwrap_or_else(|| format!("aoem_execute_ops_v2 failed rc={rc} and no last_error"));
            bail!("aoem_execute_ops_v2 failed: rc={rc}, err={err}");
        }
        Ok(result)
    }

    pub fn execute_ops_wire_v1(&self, input: &[u8]) -> Result<AoemExecV2Result> {
        let Some(exec_wire_v1) = self.dynlib.execute_ops_wire_v1 else {
            bail!(
                "aoem_execute_ops_wire_v1 not found in loaded DLL (requires AOEM FFI wire ABI build)"
            );
        };
        if input.is_empty() {
            bail!("aoem_execute_ops_wire_v1 input must not be empty");
        }
        let mut result = AoemExecV2Result {
            failed_index: u32::MAX,
            ..AoemExecV2Result::default()
        };
        let rc = unsafe {
            exec_wire_v1(
                self.raw,
                input.as_ptr(),
                input.len(),
                &mut result as *mut AoemExecV2Result,
            )
        };
        if rc != 0 {
            let err = unsafe { cstr_to_string((self.dynlib.last_error)(self.raw)) }.unwrap_or_else(
                || format!("aoem_execute_ops_wire_v1 failed rc={rc} and no last_error"),
            );
            bail!("aoem_execute_ops_wire_v1 failed: rc={rc}, err={err}");
        }
        Ok(result)
    }
}

unsafe fn cstr_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok().map(|s| s.to_string())
}
