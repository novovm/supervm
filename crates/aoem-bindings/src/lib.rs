use anyhow::{anyhow, bail, Context, Result};
use libloading::Library;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
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
pub type AoemZkvmProveVerifyV1 =
    unsafe extern "C" fn(u32, *const u8, usize, *const u8, usize, *mut u32) -> i32;
pub type AoemZkvmTraceFibProveVerify = unsafe extern "C" fn(u32, u64, u64) -> i32;
pub type AoemMldsaSupported = unsafe extern "C" fn() -> u32;
pub type AoemMldsaPubkeySize = unsafe extern "C" fn(u32) -> u32;
pub type AoemMldsaSignatureSize = unsafe extern "C" fn(u32) -> u32;
pub type AoemMldsaSecretKeySize = unsafe extern "C" fn(u32) -> u32;
pub type AoemMldsaKeygenV1 =
    unsafe extern "C" fn(u32, *mut *mut u8, *mut usize, *mut *mut u8, *mut usize) -> i32;
pub type AoemMldsaSignV1 =
    unsafe extern "C" fn(u32, *const u8, usize, *const u8, usize, *mut *mut u8, *mut usize) -> i32;
pub type AoemMldsaVerify = unsafe extern "C" fn(
    u32,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *mut u32,
) -> i32;
pub type AoemMldsaVerifyAuto =
    unsafe extern "C" fn(*const u8, usize, *const u8, usize, *const u8, usize, *mut u32) -> i32;
pub type AoemSha256V1 = unsafe extern "C" fn(*const u8, usize, *mut u8) -> i32;
pub type AoemKeccak256V1 = unsafe extern "C" fn(*const u8, usize, *mut u8) -> i32;
pub type AoemBlake3256V1 = unsafe extern "C" fn(*const u8, usize, *mut u8) -> i32;
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
pub type AoemGroth16VerifyV1 =
    unsafe extern "C" fn(*const u8, usize, *const u8, usize, *const u8, usize, *mut u32) -> i32;
pub type AoemGroth16VerifyBatchV1 = unsafe extern "C" fn(
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *mut *mut u8,
    *mut usize,
    *mut u32,
) -> i32;
pub type AoemBulletproofProveV1 =
    unsafe extern "C" fn(u64, u64, u32, *mut *mut u8, *mut usize, *mut *mut u8, *mut usize) -> i32;
pub type AoemBulletproofVerifyV1 =
    unsafe extern "C" fn(*const u8, usize, *const u8, usize, u32, *mut u32) -> i32;
pub type AoemRingctProveV1 =
    unsafe extern "C" fn(*const u8, usize, u64, u64, u32, *mut *mut u8, *mut usize) -> i32;
pub type AoemRingctVerifyV1 = unsafe extern "C" fn(*const u8, usize, u32, *mut u32) -> i32;
pub type AoemKmsSignV1 =
    unsafe extern "C" fn(u32, *const u8, usize, *const u8, usize, *mut *mut u8, *mut usize) -> i32;
pub type AoemHsmSignV1 =
    unsafe extern "C" fn(u32, *const u8, usize, *const u8, usize, *mut *mut u8, *mut usize) -> i32;
pub type AoemExecute =
    unsafe extern "C" fn(*mut c_void, *const u8, usize, *mut *mut u8, *mut usize) -> i32;
pub type AoemExecuteBatch =
    unsafe extern "C" fn(*mut c_void, *const u8, usize, *mut *mut u8, *mut usize) -> i32;
pub type AoemEd25519VerifyV1 =
    unsafe extern "C" fn(*const u8, usize, *const u8, usize, *const u8, usize, *mut u32) -> i32;
pub type AoemSecp256k1VerifyV1 =
    unsafe extern "C" fn(*const u8, usize, *const u8, usize, *const u8, usize, *mut u32) -> i32;
pub type AoemSecp256k1RecoverPubkeyV1 =
    unsafe extern "C" fn(*const u8, usize, *const u8, usize, *mut *mut u8, *mut usize) -> i32;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AoemEd25519VerifyItemV1 {
    pub pubkey_ptr: *const u8,
    pub pubkey_len: usize,
    pub message_ptr: *const u8,
    pub message_len: usize,
    pub signature_ptr: *const u8,
    pub signature_len: usize,
}
pub type AoemEd25519VerifyBatchV1 = unsafe extern "C" fn(
    *const AoemEd25519VerifyItemV1,
    usize,
    *mut *mut u8,
    *mut usize,
    *mut u32,
) -> i32;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AoemMldsaVerifyItemV1 {
    pub level: u32,
    pub pubkey_ptr: *const u8,
    pub pubkey_len: usize,
    pub message_ptr: *const u8,
    pub message_len: usize,
    pub signature_ptr: *const u8,
    pub signature_len: usize,
}
pub type AoemMldsaVerifyBatchV1 = unsafe extern "C" fn(
    *const AoemMldsaVerifyItemV1,
    usize,
    *mut *mut u8,
    *mut usize,
    *mut u32,
) -> i32;
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
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct AoemPrimitiveResultV1 {
    pub primitive: u32,
    pub backend_kind: u32,
    pub stage_count: u32,
    pub values_len: u32,
    pub indices_len: u32,
    pub output_hash: u64,
}
pub type AoemExecutePrimitiveV1 = unsafe extern "C" fn(
    *mut c_void,
    u32,
    u32,
    u32,
    *const u32,
    u32,
    *const u32,
    u32,
    *mut AoemPrimitiveResultV1,
    *mut *mut u8,
    *mut usize,
) -> i32;
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
    zkvm_prove_verify_v1: Option<AoemZkvmProveVerifyV1>,
    zkvm_trace_fib_prove_verify: Option<AoemZkvmTraceFibProveVerify>,
    mldsa_supported: Option<AoemMldsaSupported>,
    mldsa_pubkey_size: Option<AoemMldsaPubkeySize>,
    mldsa_signature_size: Option<AoemMldsaSignatureSize>,
    mldsa_secret_key_size: Option<AoemMldsaSecretKeySize>,
    mldsa_keygen_v1: Option<AoemMldsaKeygenV1>,
    mldsa_sign_v1: Option<AoemMldsaSignV1>,
    mldsa_verify: Option<AoemMldsaVerify>,
    mldsa_verify_auto: Option<AoemMldsaVerifyAuto>,
    mldsa_verify_batch_v1: Option<AoemMldsaVerifyBatchV1>,
    sha256_v1: Option<AoemSha256V1>,
    keccak256_v1: Option<AoemKeccak256V1>,
    blake3_256_v1: Option<AoemBlake3256V1>,
    ring_signature_supported: Option<AoemRingSignatureSupported>,
    ring_signature_keygen_v1: Option<AoemRingSignatureKeygenV1>,
    ring_signature_sign_web30_v1: Option<AoemRingSignatureSignWeb30V1>,
    ring_signature_verify_web30_v1: Option<AoemRingSignatureVerifyWeb30V1>,
    ring_signature_verify_batch_web30_v1: Option<AoemRingSignatureVerifyBatchWeb30V1>,
    groth16_prove_v1: Option<AoemGroth16ProveV1>,
    groth16_prove_batch_v1: Option<AoemGroth16ProveBatchV1>,
    groth16_verify_v1: Option<AoemGroth16VerifyV1>,
    groth16_verify_batch_v1: Option<AoemGroth16VerifyBatchV1>,
    bulletproof_prove_v1: Option<AoemBulletproofProveV1>,
    bulletproof_verify_v1: Option<AoemBulletproofVerifyV1>,
    bulletproof_prove_batch_v1: Option<AoemBulletproofProveBatchV1>,
    bulletproof_verify_batch_v1: Option<AoemBulletproofVerifyBatchV1>,
    ringct_prove_v1: Option<AoemRingctProveV1>,
    ringct_verify_v1: Option<AoemRingctVerifyV1>,
    ringct_prove_batch_v1: Option<AoemRingctProveBatchV1>,
    ringct_verify_batch_v1: Option<AoemRingctVerifyBatchV1>,
    kms_sign_v1: Option<AoemKmsSignV1>,
    hsm_sign_v1: Option<AoemHsmSignV1>,
    ed25519_verify_v1: Option<AoemEd25519VerifyV1>,
    ed25519_verify_batch_v1: Option<AoemEd25519VerifyBatchV1>,
    secp256k1_verify_v1: Option<AoemSecp256k1VerifyV1>,
    secp256k1_recover_pubkey_v1: Option<AoemSecp256k1RecoverPubkeyV1>,
    execute: Option<AoemExecute>,
    execute_batch: Option<AoemExecuteBatch>,
    execute_primitive_v1: Option<AoemExecutePrimitiveV1>,
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

#[derive(Clone, Copy)]
pub struct AoemEd25519VerifyItemRef<'a> {
    pub pubkey: &'a [u8],
    pub message: &'a [u8],
    pub signature: &'a [u8],
}

#[derive(Clone, Copy)]
pub struct AoemMldsaVerifyItemRef<'a> {
    pub level: u32,
    pub pubkey: &'a [u8],
    pub message: &'a [u8],
    pub signature: &'a [u8],
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
thread_local! {
    static AOEM_HOST_DYNLIB: RefCell<Option<Result<AoemDyn, String>>> = const { RefCell::new(None) };
}

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
        let zkvm_prove_verify_v1: Option<AoemZkvmProveVerifyV1> = lib
            .get::<AoemZkvmProveVerifyV1>(b"aoem_zkvm_prove_verify_v1")
            .ok()
            .map(|f| *f);
        let zkvm_trace_fib_prove_verify: Option<AoemZkvmTraceFibProveVerify> = lib
            .get::<AoemZkvmTraceFibProveVerify>(b"aoem_zkvm_trace_fib_prove_verify")
            .ok()
            .map(|f| *f);
        let mldsa_supported: Option<AoemMldsaSupported> = lib
            .get::<AoemMldsaSupported>(b"aoem_mldsa_supported")
            .ok()
            .map(|f| *f);
        let mldsa_pubkey_size: Option<AoemMldsaPubkeySize> = lib
            .get::<AoemMldsaPubkeySize>(b"aoem_mldsa_pubkey_size")
            .ok()
            .map(|f| *f);
        let mldsa_signature_size: Option<AoemMldsaSignatureSize> = lib
            .get::<AoemMldsaSignatureSize>(b"aoem_mldsa_signature_size")
            .ok()
            .map(|f| *f);
        let mldsa_secret_key_size: Option<AoemMldsaSecretKeySize> = lib
            .get::<AoemMldsaSecretKeySize>(b"aoem_mldsa_secret_key_size")
            .ok()
            .map(|f| *f);
        let mldsa_keygen_v1: Option<AoemMldsaKeygenV1> = lib
            .get::<AoemMldsaKeygenV1>(b"aoem_mldsa_keygen_v1")
            .ok()
            .map(|f| *f);
        let mldsa_sign_v1: Option<AoemMldsaSignV1> = lib
            .get::<AoemMldsaSignV1>(b"aoem_mldsa_sign_v1")
            .ok()
            .map(|f| *f);
        let mldsa_verify: Option<AoemMldsaVerify> = lib
            .get::<AoemMldsaVerify>(b"aoem_mldsa_verify")
            .ok()
            .map(|f| *f);
        let mldsa_verify_auto: Option<AoemMldsaVerifyAuto> = lib
            .get::<AoemMldsaVerifyAuto>(b"aoem_mldsa_verify_auto")
            .ok()
            .map(|f| *f);
        let mldsa_verify_batch_v1: Option<AoemMldsaVerifyBatchV1> = lib
            .get::<AoemMldsaVerifyBatchV1>(b"aoem_mldsa_verify_batch_v1")
            .ok()
            .map(|f| *f);
        let sha256_v1: Option<AoemSha256V1> =
            lib.get::<AoemSha256V1>(b"aoem_sha256_v1").ok().map(|f| *f);
        let keccak256_v1: Option<AoemKeccak256V1> = lib
            .get::<AoemKeccak256V1>(b"aoem_keccak256_v1")
            .ok()
            .map(|f| *f);
        let blake3_256_v1: Option<AoemBlake3256V1> = lib
            .get::<AoemBlake3256V1>(b"aoem_blake3_256_v1")
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
        let groth16_verify_v1: Option<AoemGroth16VerifyV1> = lib
            .get::<AoemGroth16VerifyV1>(b"aoem_groth16_verify_v1")
            .ok()
            .map(|f| *f);
        let groth16_verify_batch_v1: Option<AoemGroth16VerifyBatchV1> = lib
            .get::<AoemGroth16VerifyBatchV1>(b"aoem_groth16_verify_batch_v1")
            .ok()
            .map(|f| *f);
        let bulletproof_prove_v1: Option<AoemBulletproofProveV1> = lib
            .get::<AoemBulletproofProveV1>(b"aoem_bulletproof_prove_v1")
            .ok()
            .map(|f| *f);
        let bulletproof_verify_v1: Option<AoemBulletproofVerifyV1> = lib
            .get::<AoemBulletproofVerifyV1>(b"aoem_bulletproof_verify_v1")
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
        let ringct_prove_v1: Option<AoemRingctProveV1> = lib
            .get::<AoemRingctProveV1>(b"aoem_ringct_prove_v1")
            .ok()
            .map(|f| *f);
        let ringct_verify_v1: Option<AoemRingctVerifyV1> = lib
            .get::<AoemRingctVerifyV1>(b"aoem_ringct_verify_v1")
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
        let kms_sign_v1: Option<AoemKmsSignV1> = lib
            .get::<AoemKmsSignV1>(b"aoem_kms_sign_v1")
            .ok()
            .map(|f| *f);
        let hsm_sign_v1: Option<AoemHsmSignV1> = lib
            .get::<AoemHsmSignV1>(b"aoem_hsm_sign_v1")
            .ok()
            .map(|f| *f);
        let ed25519_verify_v1: Option<AoemEd25519VerifyV1> = lib
            .get::<AoemEd25519VerifyV1>(b"aoem_ed25519_verify_v1")
            .ok()
            .map(|f| *f);
        let ed25519_verify_batch_v1: Option<AoemEd25519VerifyBatchV1> = lib
            .get::<AoemEd25519VerifyBatchV1>(b"aoem_ed25519_verify_batch_v1")
            .ok()
            .map(|f| *f);
        let secp256k1_verify_v1: Option<AoemSecp256k1VerifyV1> = lib
            .get::<AoemSecp256k1VerifyV1>(b"aoem_secp256k1_verify_v1")
            .ok()
            .map(|f| *f);
        let secp256k1_recover_pubkey_v1: Option<AoemSecp256k1RecoverPubkeyV1> = lib
            .get::<AoemSecp256k1RecoverPubkeyV1>(b"aoem_secp256k1_recover_pubkey_v1")
            .ok()
            .map(|f| *f);
        let execute: Option<AoemExecute> = lib.get::<AoemExecute>(b"aoem_execute").ok().map(|f| *f);
        let execute_batch: Option<AoemExecuteBatch> = lib
            .get::<AoemExecuteBatch>(b"aoem_execute_batch")
            .ok()
            .map(|f| *f);
        let execute_primitive_v1: Option<AoemExecutePrimitiveV1> = lib
            .get::<AoemExecutePrimitiveV1>(b"aoem_execute_primitive_v1")
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
            zkvm_prove_verify_v1,
            zkvm_trace_fib_prove_verify,
            mldsa_supported,
            mldsa_pubkey_size,
            mldsa_signature_size,
            mldsa_secret_key_size,
            mldsa_keygen_v1,
            mldsa_sign_v1,
            mldsa_verify,
            mldsa_verify_auto,
            mldsa_verify_batch_v1,
            sha256_v1,
            keccak256_v1,
            blake3_256_v1,
            ring_signature_supported,
            ring_signature_keygen_v1,
            ring_signature_sign_web30_v1,
            ring_signature_verify_web30_v1,
            ring_signature_verify_batch_web30_v1,
            groth16_prove_v1,
            groth16_prove_batch_v1,
            groth16_verify_v1,
            groth16_verify_batch_v1,
            bulletproof_prove_v1,
            bulletproof_verify_v1,
            bulletproof_prove_batch_v1,
            bulletproof_verify_batch_v1,
            ringct_prove_v1,
            ringct_verify_v1,
            ringct_prove_batch_v1,
            ringct_verify_batch_v1,
            kms_sign_v1,
            hsm_sign_v1,
            ed25519_verify_v1,
            ed25519_verify_batch_v1,
            secp256k1_verify_v1,
            secp256k1_recover_pubkey_v1,
            execute,
            execute_batch,
            execute_primitive_v1,
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

    pub fn supports_zkvm_prove_verify_v1(&self) -> bool {
        self.zkvm_prove_verify_v1.is_some()
    }

    pub fn supports_mldsa_size_v1(&self) -> bool {
        self.mldsa_supported.is_some()
            && self.mldsa_pubkey_size.is_some()
            && self.mldsa_signature_size.is_some()
            && self.mldsa_secret_key_size.is_some()
    }

    pub fn supports_mldsa_keygen_v1(&self) -> bool {
        self.mldsa_keygen_v1.is_some() && self.free.is_some()
    }

    pub fn supports_mldsa_sign_v1(&self) -> bool {
        self.mldsa_sign_v1.is_some() && self.free.is_some()
    }

    pub fn supports_mldsa_verify_v1(&self) -> bool {
        self.mldsa_verify.is_some()
    }

    pub fn supports_mldsa_verify_auto_v1(&self) -> bool {
        self.mldsa_verify_auto.is_some()
    }

    pub fn supports_mldsa_verify_batch_v1(&self) -> bool {
        self.mldsa_verify_batch_v1.is_some() && self.free.is_some()
    }

    pub fn supports_hash_v1(&self) -> bool {
        self.sha256_v1.is_some() && self.keccak256_v1.is_some() && self.blake3_256_v1.is_some()
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

    pub fn supports_groth16_verify_v1(&self) -> bool {
        self.groth16_verify_v1.is_some()
    }

    pub fn supports_groth16_verify_batch_v1(&self) -> bool {
        self.groth16_verify_batch_v1.is_some() && self.free.is_some()
    }

    pub fn supports_groth16_prove_auto_path(&self) -> bool {
        self.groth16_prove_batch_v1.is_some() || self.groth16_prove_v1.is_some()
    }

    pub fn supports_bulletproof_v1(&self) -> bool {
        self.bulletproof_prove_v1.is_some() && self.bulletproof_verify_v1.is_some()
    }

    pub fn supports_bulletproof_batch_v1(&self) -> bool {
        self.bulletproof_prove_batch_v1.is_some() && self.bulletproof_verify_batch_v1.is_some()
    }

    pub fn supports_ringct_v1(&self) -> bool {
        self.ringct_prove_v1.is_some() && self.ringct_verify_v1.is_some()
    }

    pub fn supports_ringct_batch_v1(&self) -> bool {
        self.ringct_prove_batch_v1.is_some() && self.ringct_verify_batch_v1.is_some()
    }

    pub fn supports_kms_sign_v1(&self) -> bool {
        self.kms_sign_v1.is_some() && self.free.is_some()
    }

    pub fn supports_hsm_sign_v1(&self) -> bool {
        self.hsm_sign_v1.is_some() && self.free.is_some()
    }

    pub fn supports_ed25519_verify_v1(&self) -> bool {
        self.ed25519_verify_v1.is_some()
    }

    pub fn supports_ed25519_verify_batch_v1(&self) -> bool {
        self.ed25519_verify_batch_v1.is_some() && self.free.is_some()
    }

    pub fn supports_secp256k1_verify_v1(&self) -> bool {
        self.secp256k1_verify_v1.is_some()
    }

    pub fn supports_secp256k1_recover_pubkey_v1(&self) -> bool {
        self.secp256k1_recover_pubkey_v1.is_some() && self.free.is_some()
    }

    pub fn supports_privacy_batch_v1(&self) -> bool {
        self.supports_ring_signature_verify_batch_web30_v1()
            && self.supports_bulletproof_batch_v1()
            && self.supports_ringct_batch_v1()
    }

    pub fn supports_execute_legacy(&self) -> bool {
        self.execute.is_some() && self.free.is_some()
    }

    pub fn supports_execute_batch_legacy(&self) -> bool {
        self.execute_batch.is_some() && self.free.is_some()
    }

    pub fn supports_execute_primitive_v1(&self) -> bool {
        self.execute_primitive_v1.is_some() && self.free.is_some()
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

    pub fn zkvm_prove_verify_v1(
        &self,
        backend: u32,
        program: &[u8],
        witness: &[u8],
    ) -> Result<bool> {
        let Some(verify_fn) = self.zkvm_prove_verify_v1 else {
            bail!("aoem_zkvm_prove_verify_v1 not found in loaded DLL");
        };
        let mut out_verified = 0u32;
        let rc = unsafe {
            verify_fn(
                backend,
                program.as_ptr(),
                program.len(),
                witness.as_ptr(),
                witness.len(),
                &mut out_verified as *mut u32,
            )
        };
        if rc != 0 {
            bail!("aoem_zkvm_prove_verify_v1 failed: rc={rc}");
        }
        Ok(out_verified != 0)
    }

    pub fn mldsa_supported_flag(&self) -> Option<bool> {
        self.mldsa_supported.map(|f| unsafe { f() != 0 })
    }

    pub fn mldsa_pubkey_size_v1(&self, level: u32) -> Result<usize> {
        let Some(size_fn) = self.mldsa_pubkey_size else {
            bail!("aoem_mldsa_pubkey_size not found in loaded DLL");
        };
        let size = unsafe { size_fn(level) } as usize;
        if size == 0 {
            bail!("aoem_mldsa_pubkey_size returned 0 for level={level}");
        }
        Ok(size)
    }

    pub fn mldsa_signature_size_v1(&self, level: u32) -> Result<usize> {
        let Some(size_fn) = self.mldsa_signature_size else {
            bail!("aoem_mldsa_signature_size not found in loaded DLL");
        };
        let size = unsafe { size_fn(level) } as usize;
        if size == 0 {
            bail!("aoem_mldsa_signature_size returned 0 for level={level}");
        }
        Ok(size)
    }

    pub fn mldsa_secret_key_size_v1(&self, level: u32) -> Result<usize> {
        let Some(size_fn) = self.mldsa_secret_key_size else {
            bail!("aoem_mldsa_secret_key_size not found in loaded DLL");
        };
        let size = unsafe { size_fn(level) } as usize;
        if size == 0 {
            bail!("aoem_mldsa_secret_key_size returned 0 for level={level}");
        }
        Ok(size)
    }

    pub fn mldsa_keygen_v1(&self, level: u32) -> Result<(Vec<u8>, Vec<u8>)> {
        let Some(keygen_fn) = self.mldsa_keygen_v1 else {
            bail!("aoem_mldsa_keygen_v1 not found in loaded DLL");
        };
        let mut pubkey_ptr: *mut u8 = ptr::null_mut();
        let mut pubkey_len = 0usize;
        let mut secret_key_ptr: *mut u8 = ptr::null_mut();
        let mut secret_key_len = 0usize;
        let rc = unsafe {
            keygen_fn(
                level,
                &mut pubkey_ptr as *mut *mut u8,
                &mut pubkey_len as *mut usize,
                &mut secret_key_ptr as *mut *mut u8,
                &mut secret_key_len as *mut usize,
            )
        };
        if rc != 0 {
            bail!("aoem_mldsa_keygen_v1 failed: rc={rc}");
        }
        let pubkey = self.copy_aoem_owned_bytes(pubkey_ptr, pubkey_len, "mldsa keygen pubkey")?;
        let secret_key =
            self.copy_aoem_owned_bytes(secret_key_ptr, secret_key_len, "mldsa keygen secret key")?;
        Ok((pubkey, secret_key))
    }

    pub fn mldsa_sign_v1(&self, level: u32, secret_key: &[u8], message: &[u8]) -> Result<Vec<u8>> {
        let Some(sign_fn) = self.mldsa_sign_v1 else {
            bail!("aoem_mldsa_sign_v1 not found in loaded DLL");
        };
        if secret_key.is_empty() {
            bail!("mldsa secret_key must not be empty");
        }
        let mut signature_ptr: *mut u8 = ptr::null_mut();
        let mut signature_len = 0usize;
        let rc = unsafe {
            sign_fn(
                level,
                secret_key.as_ptr(),
                secret_key.len(),
                message.as_ptr(),
                message.len(),
                &mut signature_ptr as *mut *mut u8,
                &mut signature_len as *mut usize,
            )
        };
        if rc != 0 {
            bail!("aoem_mldsa_sign_v1 failed: rc={rc}");
        }
        self.copy_aoem_owned_bytes(signature_ptr, signature_len, "mldsa signature")
    }

    pub fn mldsa_verify_v1(
        &self,
        level: u32,
        pubkey: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<bool> {
        let Some(verify_fn) = self.mldsa_verify else {
            bail!("aoem_mldsa_verify not found in loaded DLL");
        };
        if pubkey.is_empty() {
            bail!("mldsa pubkey must not be empty");
        }
        if signature.is_empty() {
            bail!("mldsa signature must not be empty");
        }
        let mut out_valid = 0u32;
        let rc = unsafe {
            verify_fn(
                level,
                pubkey.as_ptr(),
                pubkey.len(),
                message.as_ptr(),
                message.len(),
                signature.as_ptr(),
                signature.len(),
                &mut out_valid as *mut u32,
            )
        };
        if rc != 0 {
            bail!("aoem_mldsa_verify failed: rc={rc}");
        }
        Ok(out_valid != 0)
    }

    pub fn mldsa_verify_auto_v1(
        &self,
        pubkey: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<bool> {
        let Some(verify_fn) = self.mldsa_verify_auto else {
            bail!("aoem_mldsa_verify_auto not found in loaded DLL");
        };
        if pubkey.is_empty() {
            bail!("mldsa pubkey must not be empty");
        }
        if signature.is_empty() {
            bail!("mldsa signature must not be empty");
        }
        let mut out_valid = 0u32;
        let rc = unsafe {
            verify_fn(
                pubkey.as_ptr(),
                pubkey.len(),
                message.as_ptr(),
                message.len(),
                signature.as_ptr(),
                signature.len(),
                &mut out_valid as *mut u32,
            )
        };
        if rc != 0 {
            bail!("aoem_mldsa_verify_auto failed: rc={rc}");
        }
        Ok(out_valid != 0)
    }

    pub fn mldsa_verify_batch_v1(&self, items: &[AoemMldsaVerifyItemRef<'_>]) -> Result<Vec<bool>> {
        let Some(batch_fn) = self.mldsa_verify_batch_v1 else {
            bail!("aoem_mldsa_verify_batch_v1 not found in loaded DLL");
        };
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let mut raw_items = Vec::with_capacity(items.len());
        for item in items {
            if item.pubkey.is_empty() {
                bail!("mldsa batch pubkey must not be empty");
            }
            if item.signature.is_empty() {
                bail!("mldsa batch signature must not be empty");
            }
            raw_items.push(AoemMldsaVerifyItemV1 {
                level: item.level,
                pubkey_ptr: item.pubkey.as_ptr(),
                pubkey_len: item.pubkey.len(),
                message_ptr: item.message.as_ptr(),
                message_len: item.message.len(),
                signature_ptr: item.signature.as_ptr(),
                signature_len: item.signature.len(),
            });
        }
        let mut out_results_ptr: *mut u8 = ptr::null_mut();
        let mut out_results_len = 0usize;
        let mut out_valid_count = 0u32;
        let rc = unsafe {
            batch_fn(
                raw_items.as_ptr(),
                raw_items.len(),
                &mut out_results_ptr as *mut *mut u8,
                &mut out_results_len as *mut usize,
                &mut out_valid_count as *mut u32,
            )
        };
        if rc != 0 {
            bail!("aoem_mldsa_verify_batch_v1 failed: rc={rc}");
        }
        let out_results =
            self.copy_aoem_owned_bytes(out_results_ptr, out_results_len, "mldsa batch results")?;
        if out_results.len() != items.len() {
            bail!(
                "mldsa batch results length mismatch: expected {}, got {}",
                items.len(),
                out_results.len()
            );
        }
        Ok(out_results.into_iter().map(|v| v != 0).collect())
    }

    pub fn sha256_v1(&self, data: &[u8]) -> Result<[u8; 32]> {
        let Some(hash_fn) = self.sha256_v1 else {
            bail!("aoem_sha256_v1 not found in loaded DLL");
        };
        self.run_hash32_v1(data, hash_fn, "aoem_sha256_v1")
    }

    pub fn keccak256_v1(&self, data: &[u8]) -> Result<[u8; 32]> {
        let Some(hash_fn) = self.keccak256_v1 else {
            bail!("aoem_keccak256_v1 not found in loaded DLL");
        };
        self.run_hash32_v1(data, hash_fn, "aoem_keccak256_v1")
    }

    pub fn blake3_256_v1(&self, data: &[u8]) -> Result<[u8; 32]> {
        let Some(hash_fn) = self.blake3_256_v1 else {
            bail!("aoem_blake3_256_v1 not found in loaded DLL");
        };
        self.run_hash32_v1(data, hash_fn, "aoem_blake3_256_v1")
    }

    fn run_hash32_v1(
        &self,
        data: &[u8],
        hash_fn: unsafe extern "C" fn(*const u8, usize, *mut u8) -> i32,
        label: &str,
    ) -> Result<[u8; 32]> {
        let mut out = [0u8; 32];
        let rc = unsafe { hash_fn(data.as_ptr(), data.len(), out.as_mut_ptr()) };
        if rc != 0 {
            bail!("{label} failed: rc={rc}");
        }
        Ok(out)
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

    pub fn ed25519_verify_v1(
        &self,
        pubkey: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<bool> {
        let Some(verify_fn) = self.ed25519_verify_v1 else {
            bail!("aoem_ed25519_verify_v1 not found in loaded DLL");
        };
        if pubkey.is_empty() {
            bail!("ed25519 pubkey must not be empty");
        }
        if signature.is_empty() {
            bail!("ed25519 signature must not be empty");
        }
        let mut out_valid = 0u32;
        let rc = unsafe {
            verify_fn(
                pubkey.as_ptr(),
                pubkey.len(),
                message.as_ptr(),
                message.len(),
                signature.as_ptr(),
                signature.len(),
                &mut out_valid as *mut u32,
            )
        };
        if rc != 0 {
            bail!("aoem_ed25519_verify_v1 failed: rc={rc}");
        }
        Ok(out_valid != 0)
    }

    pub fn ed25519_verify_batch_v1(
        &self,
        items: &[AoemEd25519VerifyItemRef<'_>],
    ) -> Result<Vec<bool>> {
        let Some(batch_fn) = self.ed25519_verify_batch_v1 else {
            bail!("aoem_ed25519_verify_batch_v1 not found in loaded DLL");
        };
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let mut raw_items = Vec::with_capacity(items.len());
        for item in items {
            if item.pubkey.is_empty() {
                bail!("ed25519 batch pubkey must not be empty");
            }
            if item.signature.is_empty() {
                bail!("ed25519 batch signature must not be empty");
            }
            raw_items.push(AoemEd25519VerifyItemV1 {
                pubkey_ptr: item.pubkey.as_ptr(),
                pubkey_len: item.pubkey.len(),
                message_ptr: item.message.as_ptr(),
                message_len: item.message.len(),
                signature_ptr: item.signature.as_ptr(),
                signature_len: item.signature.len(),
            });
        }
        let mut out_results_ptr: *mut u8 = ptr::null_mut();
        let mut out_results_len = 0usize;
        let mut out_valid_count = 0u32;
        let rc = unsafe {
            batch_fn(
                raw_items.as_ptr(),
                raw_items.len(),
                &mut out_results_ptr as *mut *mut u8,
                &mut out_results_len as *mut usize,
                &mut out_valid_count as *mut u32,
            )
        };
        if rc != 0 {
            bail!("aoem_ed25519_verify_batch_v1 failed: rc={rc}");
        }
        let out_results =
            self.copy_aoem_owned_bytes(out_results_ptr, out_results_len, "ed25519 batch results")?;
        if out_results.len() != items.len() {
            bail!(
                "ed25519 batch results length mismatch: expected {}, got {}",
                items.len(),
                out_results.len()
            );
        }
        Ok(out_results.into_iter().map(|v| v != 0).collect())
    }

    pub fn secp256k1_verify_v1(
        &self,
        message32: &[u8],
        signature65: &[u8],
        pubkey: &[u8],
    ) -> Result<bool> {
        let Some(verify_fn) = self.secp256k1_verify_v1 else {
            bail!("aoem_secp256k1_verify_v1 not found in loaded DLL");
        };
        if message32.len() != 32 {
            bail!(
                "secp256k1 message32 must be 32 bytes, got {}",
                message32.len()
            );
        }
        if signature65.len() != 65 {
            bail!(
                "secp256k1 signature65 must be 65 bytes, got {}",
                signature65.len()
            );
        }
        if pubkey.is_empty() {
            bail!("secp256k1 pubkey must not be empty");
        }
        let mut out_valid = 0u32;
        let rc = unsafe {
            verify_fn(
                message32.as_ptr(),
                message32.len(),
                signature65.as_ptr(),
                signature65.len(),
                pubkey.as_ptr(),
                pubkey.len(),
                &mut out_valid as *mut u32,
            )
        };
        if rc != 0 {
            bail!("aoem_secp256k1_verify_v1 failed: rc={rc}");
        }
        Ok(out_valid != 0)
    }

    pub fn secp256k1_recover_pubkey_v1(
        &self,
        message32: &[u8],
        signature65: &[u8],
    ) -> Result<Vec<u8>> {
        let Some(recover_fn) = self.secp256k1_recover_pubkey_v1 else {
            bail!("aoem_secp256k1_recover_pubkey_v1 not found in loaded DLL");
        };
        if message32.len() != 32 {
            bail!(
                "secp256k1 message32 must be 32 bytes, got {}",
                message32.len()
            );
        }
        if signature65.len() != 65 {
            bail!(
                "secp256k1 signature65 must be 65 bytes, got {}",
                signature65.len()
            );
        }
        let mut out_pubkey_ptr: *mut u8 = ptr::null_mut();
        let mut out_pubkey_len = 0usize;
        let rc = unsafe {
            recover_fn(
                message32.as_ptr(),
                message32.len(),
                signature65.as_ptr(),
                signature65.len(),
                &mut out_pubkey_ptr as *mut *mut u8,
                &mut out_pubkey_len as *mut usize,
            )
        };
        if rc != 0 {
            bail!("aoem_secp256k1_recover_pubkey_v1 failed: rc={rc}");
        }
        self.copy_aoem_owned_bytes(out_pubkey_ptr, out_pubkey_len, "secp256k1 recovered pubkey")
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

    pub fn groth16_verify_v1(&self, vk: &[u8], proof: &[u8], public_inputs: &[u8]) -> Result<bool> {
        let Some(verify_fn) = self.groth16_verify_v1 else {
            bail!("aoem_groth16_verify_v1 not found in loaded DLL");
        };
        if vk.is_empty() {
            bail!("groth16 vk must not be empty");
        }
        if proof.is_empty() {
            bail!("groth16 proof must not be empty");
        }
        if public_inputs.is_empty() {
            bail!("groth16 public_inputs must not be empty");
        }
        let mut out_valid = 0u32;
        let rc = unsafe {
            verify_fn(
                vk.as_ptr(),
                vk.len(),
                proof.as_ptr(),
                proof.len(),
                public_inputs.as_ptr(),
                public_inputs.len(),
                &mut out_valid as *mut u32,
            )
        };
        if rc != 0 {
            bail!("aoem_groth16_verify_v1 failed: rc={rc}");
        }
        Ok(out_valid != 0)
    }

    pub fn groth16_verify_batch_v1(
        &self,
        vk: &[u8],
        proofs_wire: &[u8],
        public_inputs_wire: &[u8],
    ) -> Result<Vec<bool>> {
        let Some(batch_fn) = self.groth16_verify_batch_v1 else {
            bail!("aoem_groth16_verify_batch_v1 not found in loaded DLL");
        };
        if vk.is_empty() {
            bail!("groth16 batch vk must not be empty");
        }
        if proofs_wire.is_empty() {
            bail!("groth16 batch proofs_wire must not be empty");
        }
        if public_inputs_wire.is_empty() {
            bail!("groth16 batch public_inputs_wire must not be empty");
        }
        let mut out_results_ptr: *mut u8 = ptr::null_mut();
        let mut out_results_len = 0usize;
        let mut out_valid_count = 0u32;
        let rc = unsafe {
            batch_fn(
                vk.as_ptr(),
                vk.len(),
                proofs_wire.as_ptr(),
                proofs_wire.len(),
                public_inputs_wire.as_ptr(),
                public_inputs_wire.len(),
                &mut out_results_ptr as *mut *mut u8,
                &mut out_results_len as *mut usize,
                &mut out_valid_count as *mut u32,
            )
        };
        if rc != 0 {
            bail!("aoem_groth16_verify_batch_v1 failed: rc={rc}");
        }
        let out_results = self.copy_aoem_owned_bytes(
            out_results_ptr,
            out_results_len,
            "groth16 batch verify results",
        )?;
        Ok(out_results.into_iter().map(|v| v != 0).collect())
    }

    pub fn bulletproof_prove_v1(&self, amount: u128, bits: u32) -> Result<(Vec<u8>, Vec<u8>)> {
        let Some(prove_fn) = self.bulletproof_prove_v1 else {
            bail!("aoem_bulletproof_prove_v1 not found in loaded DLL");
        };
        let amount_lo = amount as u64;
        let amount_hi = (amount >> 64) as u64;
        let mut commitment_ptr: *mut u8 = ptr::null_mut();
        let mut commitment_len = 0usize;
        let mut proof_ptr: *mut u8 = ptr::null_mut();
        let mut proof_len = 0usize;
        let rc = unsafe {
            prove_fn(
                amount_lo,
                amount_hi,
                bits,
                &mut commitment_ptr as *mut *mut u8,
                &mut commitment_len as *mut usize,
                &mut proof_ptr as *mut *mut u8,
                &mut proof_len as *mut usize,
            )
        };
        if rc != 0 {
            bail!("aoem_bulletproof_prove_v1 failed: rc={rc}");
        }
        let commitment =
            self.copy_aoem_owned_bytes(commitment_ptr, commitment_len, "bulletproof commitment")?;
        let proof = self.copy_aoem_owned_bytes(proof_ptr, proof_len, "bulletproof proof")?;
        Ok((commitment, proof))
    }

    pub fn bulletproof_verify_v1(
        &self,
        commitment: &[u8],
        proof: &[u8],
        bits: u32,
    ) -> Result<bool> {
        let Some(verify_fn) = self.bulletproof_verify_v1 else {
            bail!("aoem_bulletproof_verify_v1 not found in loaded DLL");
        };
        if commitment.is_empty() {
            bail!("bulletproof commitment must not be empty");
        }
        if proof.is_empty() {
            bail!("bulletproof proof must not be empty");
        }
        let mut out_valid = 0u32;
        let rc = unsafe {
            verify_fn(
                commitment.as_ptr(),
                commitment.len(),
                proof.as_ptr(),
                proof.len(),
                bits,
                &mut out_valid as *mut u32,
            )
        };
        if rc != 0 {
            bail!("aoem_bulletproof_verify_v1 failed: rc={rc}");
        }
        Ok(out_valid != 0)
    }

    pub fn ringct_prove_v1(&self, message: &[u8], amount: u128, ring_size: u32) -> Result<Vec<u8>> {
        let Some(prove_fn) = self.ringct_prove_v1 else {
            bail!("aoem_ringct_prove_v1 not found in loaded DLL");
        };
        if ring_size < 2 {
            bail!("ringct ring_size must be >= 2");
        }
        let amount_lo = amount as u64;
        let amount_hi = (amount >> 64) as u64;
        let mut payload_ptr: *mut u8 = ptr::null_mut();
        let mut payload_len = 0usize;
        let rc = unsafe {
            prove_fn(
                message.as_ptr(),
                message.len(),
                amount_lo,
                amount_hi,
                ring_size,
                &mut payload_ptr as *mut *mut u8,
                &mut payload_len as *mut usize,
            )
        };
        if rc != 0 {
            bail!("aoem_ringct_prove_v1 failed: rc={rc}");
        }
        self.copy_aoem_owned_bytes(payload_ptr, payload_len, "ringct tx payload")
    }

    pub fn ringct_verify_v1(&self, tx_payload: &[u8], tx_encoding: u32) -> Result<bool> {
        let Some(verify_fn) = self.ringct_verify_v1 else {
            bail!("aoem_ringct_verify_v1 not found in loaded DLL");
        };
        if tx_payload.is_empty() {
            bail!("ringct tx_payload must not be empty");
        }
        let mut out_valid = 0u32;
        let rc = unsafe {
            verify_fn(
                tx_payload.as_ptr(),
                tx_payload.len(),
                tx_encoding,
                &mut out_valid as *mut u32,
            )
        };
        if rc != 0 {
            bail!("aoem_ringct_verify_v1 failed: rc={rc}");
        }
        Ok(out_valid != 0)
    }

    pub fn kms_sign_v1(&self, level: u32, key_material: &[u8], message: &[u8]) -> Result<Vec<u8>> {
        let Some(sign_fn) = self.kms_sign_v1 else {
            bail!("aoem_kms_sign_v1 not found in loaded DLL");
        };
        if key_material.is_empty() {
            bail!("kms key_material must not be empty");
        }
        let mut signature_ptr: *mut u8 = ptr::null_mut();
        let mut signature_len = 0usize;
        let rc = unsafe {
            sign_fn(
                level,
                key_material.as_ptr(),
                key_material.len(),
                message.as_ptr(),
                message.len(),
                &mut signature_ptr as *mut *mut u8,
                &mut signature_len as *mut usize,
            )
        };
        if rc != 0 {
            bail!("aoem_kms_sign_v1 failed: rc={rc}");
        }
        self.copy_aoem_owned_bytes(signature_ptr, signature_len, "kms signature")
    }

    pub fn hsm_sign_v1(&self, level: u32, key_material: &[u8], message: &[u8]) -> Result<Vec<u8>> {
        let Some(sign_fn) = self.hsm_sign_v1 else {
            bail!("aoem_hsm_sign_v1 not found in loaded DLL");
        };
        if key_material.is_empty() {
            bail!("hsm key_material must not be empty");
        }
        let mut signature_ptr: *mut u8 = ptr::null_mut();
        let mut signature_len = 0usize;
        let rc = unsafe {
            sign_fn(
                level,
                key_material.as_ptr(),
                key_material.len(),
                message.as_ptr(),
                message.len(),
                &mut signature_ptr as *mut *mut u8,
                &mut signature_len as *mut usize,
            )
        };
        if rc != 0 {
            bail!("aoem_hsm_sign_v1 failed: rc={rc}");
        }
        self.copy_aoem_owned_bytes(signature_ptr, signature_len, "hsm signature")
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
        if level1_name == "core" {
            let platform_root = level1.parent().unwrap_or(level1);
            let repo_root = platform_root.parent().unwrap_or(platform_root);
            let common = repo_root.join("config").join("aoem-runtime-profile.json");
            if common.exists() {
                return common;
            }
            return platform_root
                .join("config")
                .join("aoem-runtime-profile.json");
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
        if level1_name == "core" {
            let platform_root = level1.parent().unwrap_or(level1);
            let repo_root = platform_root.parent().unwrap_or(platform_root);
            let common = repo_root.join("manifest").join("aoem-manifest.json");
            if common.exists() {
                return common;
            }
            return platform_root.join("manifest").join("aoem-manifest.json");
        }
        return level1.join("manifest").join("aoem-manifest.json");
    }
    parent.join("manifest").join("aoem-manifest.json")
}

fn default_host_dll_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "aoem_ffi.dll"
    } else if cfg!(target_os = "macos") {
        "libaoem_ffi.dylib"
    } else {
        "libaoem_ffi.so"
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

pub fn default_host_dll_path() -> PathBuf {
    if let Ok(explicit) = std::env::var("NOVOVM_AOEM_DLL").or_else(|_| std::env::var("AOEM_DLL")) {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for base in [
        manifest_dir.join("..").join(".."),
        manifest_dir.join("..").join("..").join(".."),
    ] {
        let aoem_root = base.join("aoem");
        for candidate in [
            aoem_root
                .join(current_platform_dir_name())
                .join("core")
                .join("bin")
                .join(default_host_dll_name()),
            aoem_root.join("bin").join(default_host_dll_name()),
        ] {
            if candidate.exists() {
                return candidate;
            }
        }
    }
    manifest_dir
        .join("..")
        .join("..")
        .join("aoem")
        .join(current_platform_dir_name())
        .join("core")
        .join("bin")
        .join(default_host_dll_name())
}

fn with_default_host_dynlib<T>(f: impl FnOnce(&AoemDyn) -> Result<T>) -> Result<Option<T>> {
    AOEM_HOST_DYNLIB.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            let loaded: std::result::Result<AoemDyn, String> = {
                let dll_path = default_host_dll_path();
                unsafe { AoemDyn::load(&dll_path) }
            }
            .map_err(|err| err.to_string());
            *slot = Some(loaded);
        }
        match slot.as_ref().expect("aoem host dynlib slot initialized") {
            Ok(dynlib) => f(dynlib).map(Some),
            Err(_) => Ok(None),
        }
    })
}

pub fn ed25519_verify_v1_auto(
    pubkey: &[u8],
    message: &[u8],
    signature: &[u8],
) -> Result<Option<bool>> {
    with_default_host_dynlib(|dynlib| {
        if !dynlib.supports_ed25519_verify_v1() {
            bail!("aoem_ed25519_verify_v1 not supported by loaded DLL");
        }
        dynlib.ed25519_verify_v1(pubkey, message, signature)
    })
    .or_else(|_| Ok(None))
}

pub fn zkvm_prove_verify_v1_auto(
    backend: u32,
    program: &[u8],
    witness: &[u8],
) -> Result<Option<bool>> {
    with_default_host_dynlib(|dynlib| {
        if !dynlib.supports_zkvm_prove_verify_v1() {
            bail!("aoem_zkvm_prove_verify_v1 not supported by loaded DLL");
        }
        dynlib.zkvm_prove_verify_v1(backend, program, witness)
    })
    .or_else(|_| Ok(None))
}

pub fn mldsa_keygen_v1_auto(level: u32) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
    with_default_host_dynlib(|dynlib| {
        if !dynlib.supports_mldsa_keygen_v1() {
            bail!("aoem_mldsa_keygen_v1 not supported by loaded DLL");
        }
        dynlib.mldsa_keygen_v1(level)
    })
    .or_else(|_| Ok(None))
}

pub fn mldsa_sign_v1_auto(
    level: u32,
    secret_key: &[u8],
    message: &[u8],
) -> Result<Option<Vec<u8>>> {
    with_default_host_dynlib(|dynlib| {
        if !dynlib.supports_mldsa_sign_v1() {
            bail!("aoem_mldsa_sign_v1 not supported by loaded DLL");
        }
        dynlib.mldsa_sign_v1(level, secret_key, message)
    })
    .or_else(|_| Ok(None))
}

pub fn mldsa_verify_v1_auto(
    level: u32,
    pubkey: &[u8],
    message: &[u8],
    signature: &[u8],
) -> Result<Option<bool>> {
    with_default_host_dynlib(|dynlib| {
        if !dynlib.supports_mldsa_verify_v1() {
            bail!("aoem_mldsa_verify not supported by loaded DLL");
        }
        dynlib.mldsa_verify_v1(level, pubkey, message, signature)
    })
    .or_else(|_| Ok(None))
}

pub fn mldsa_verify_auto_v1_auto(
    pubkey: &[u8],
    message: &[u8],
    signature: &[u8],
) -> Result<Option<bool>> {
    with_default_host_dynlib(|dynlib| {
        if !dynlib.supports_mldsa_verify_auto_v1() {
            bail!("aoem_mldsa_verify_auto not supported by loaded DLL");
        }
        dynlib.mldsa_verify_auto_v1(pubkey, message, signature)
    })
    .or_else(|_| Ok(None))
}

pub fn mldsa_verify_batch_v1_auto(
    items: &[AoemMldsaVerifyItemRef<'_>],
) -> Result<Option<Vec<bool>>> {
    if items.is_empty() {
        return Ok(Some(Vec::new()));
    }
    with_default_host_dynlib(|dynlib| {
        if !dynlib.supports_mldsa_verify_batch_v1() {
            bail!("aoem_mldsa_verify_batch_v1 not supported by loaded DLL");
        }
        dynlib.mldsa_verify_batch_v1(items)
    })
    .or_else(|_| Ok(None))
}

pub fn sha256_v1_auto(data: &[u8]) -> Result<Option<[u8; 32]>> {
    with_default_host_dynlib(|dynlib| {
        if dynlib.sha256_v1.is_none() {
            bail!("aoem_sha256_v1 not supported by loaded DLL");
        }
        dynlib.sha256_v1(data)
    })
    .or_else(|_| Ok(None))
}

pub fn keccak256_v1_auto(data: &[u8]) -> Result<Option<[u8; 32]>> {
    with_default_host_dynlib(|dynlib| {
        if dynlib.keccak256_v1.is_none() {
            bail!("aoem_keccak256_v1 not supported by loaded DLL");
        }
        dynlib.keccak256_v1(data)
    })
    .or_else(|_| Ok(None))
}

pub fn blake3_256_v1_auto(data: &[u8]) -> Result<Option<[u8; 32]>> {
    with_default_host_dynlib(|dynlib| {
        if dynlib.blake3_256_v1.is_none() {
            bail!("aoem_blake3_256_v1 not supported by loaded DLL");
        }
        dynlib.blake3_256_v1(data)
    })
    .or_else(|_| Ok(None))
}

pub fn ed25519_verify_batch_v1_auto(
    items: &[AoemEd25519VerifyItemRef<'_>],
) -> Result<Option<Vec<bool>>> {
    if items.is_empty() {
        return Ok(Some(Vec::new()));
    }
    with_default_host_dynlib(|dynlib| {
        if !dynlib.supports_ed25519_verify_batch_v1() {
            bail!("aoem_ed25519_verify_batch_v1 not supported by loaded DLL");
        }
        dynlib.ed25519_verify_batch_v1(items)
    })
    .or_else(|_| Ok(None))
}

pub fn secp256k1_verify_v1_auto(
    message32: &[u8],
    signature65: &[u8],
    pubkey: &[u8],
) -> Result<Option<bool>> {
    with_default_host_dynlib(|dynlib| {
        if !dynlib.supports_secp256k1_verify_v1() {
            bail!("aoem_secp256k1_verify_v1 not supported by loaded DLL");
        }
        dynlib.secp256k1_verify_v1(message32, signature65, pubkey)
    })
    .or_else(|_| Ok(None))
}

pub fn secp256k1_recover_pubkey_v1_auto(
    message32: &[u8],
    signature65: &[u8],
) -> Result<Option<Vec<u8>>> {
    with_default_host_dynlib(|dynlib| {
        if !dynlib.supports_secp256k1_recover_pubkey_v1() {
            bail!("aoem_secp256k1_recover_pubkey_v1 not supported by loaded DLL");
        }
        dynlib.secp256k1_recover_pubkey_v1(message32, signature65)
    })
    .or_else(|_| Ok(None))
}

pub fn bulletproof_prove_v1_auto(amount: u128, bits: u32) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
    with_default_host_dynlib(|dynlib| {
        if !dynlib.supports_bulletproof_v1() {
            bail!("aoem_bulletproof_prove_v1 not supported by loaded DLL");
        }
        dynlib.bulletproof_prove_v1(amount, bits)
    })
    .or_else(|_| Ok(None))
}

pub fn bulletproof_verify_v1_auto(
    commitment: &[u8],
    proof: &[u8],
    bits: u32,
) -> Result<Option<bool>> {
    with_default_host_dynlib(|dynlib| {
        if !dynlib.supports_bulletproof_v1() {
            bail!("aoem_bulletproof_verify_v1 not supported by loaded DLL");
        }
        dynlib.bulletproof_verify_v1(commitment, proof, bits)
    })
    .or_else(|_| Ok(None))
}

pub fn ringct_prove_v1_auto(
    message: &[u8],
    amount: u128,
    ring_size: u32,
) -> Result<Option<Vec<u8>>> {
    with_default_host_dynlib(|dynlib| {
        if !dynlib.supports_ringct_v1() {
            bail!("aoem_ringct_prove_v1 not supported by loaded DLL");
        }
        dynlib.ringct_prove_v1(message, amount, ring_size)
    })
    .or_else(|_| Ok(None))
}

pub fn ringct_verify_v1_auto(tx_payload: &[u8], tx_encoding: u32) -> Result<Option<bool>> {
    with_default_host_dynlib(|dynlib| {
        if !dynlib.supports_ringct_v1() {
            bail!("aoem_ringct_verify_v1 not supported by loaded DLL");
        }
        dynlib.ringct_verify_v1(tx_payload, tx_encoding)
    })
    .or_else(|_| Ok(None))
}

fn encode_single_blob_wire_v1(blob: &[u8], label: &str) -> Result<Vec<u8>> {
    if blob.is_empty() {
        bail!("{label} blob must not be empty");
    }
    let len_u32 =
        u32::try_from(blob.len()).map_err(|_| anyhow!("{label} blob too large: {}", blob.len()))?;
    let mut out = Vec::with_capacity(8usize.saturating_add(blob.len()));
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&len_u32.to_le_bytes());
    out.extend_from_slice(blob);
    Ok(out)
}

fn decode_single_blob_wire_v1(wire: &[u8], label: &str) -> Result<Vec<u8>> {
    if wire.len() < 8 {
        bail!("{label} wire too short");
    }
    let count = u32::from_le_bytes([wire[0], wire[1], wire[2], wire[3]]);
    if count != 1 {
        bail!("{label} wire expected count=1, got {}", count);
    }
    let len = u32::from_le_bytes([wire[4], wire[5], wire[6], wire[7]]) as usize;
    let end = 8usize.saturating_add(len);
    if len == 0 {
        bail!("{label} wire item must not be empty");
    }
    if wire.len() != end {
        bail!(
            "{label} wire size mismatch: expected {}, got {}",
            end,
            wire.len()
        );
    }
    Ok(wire[8..end].to_vec())
}

pub fn groth16_prove_v1_auto(witness: &[u8]) -> Result<Option<(Vec<u8>, Vec<u8>, Vec<u8>)>> {
    with_default_host_dynlib(|dynlib| {
        if !dynlib.supports_groth16_prove_auto_path() {
            bail!("aoem_groth16_prove_auto path not supported by loaded DLL");
        }
        let witness_wire = encode_single_blob_wire_v1(witness, "groth16 witness")?;
        let (vk, proofs_wire, public_inputs_wire) = dynlib.groth16_prove_batch_v1(&witness_wire)?;
        let proof = decode_single_blob_wire_v1(&proofs_wire, "groth16 proofs batch")?;
        let public_inputs =
            decode_single_blob_wire_v1(&public_inputs_wire, "groth16 public inputs batch")?;
        Ok((vk, proof, public_inputs))
    })
    .or_else(|_| Ok(None))
}

pub fn groth16_verify_v1_auto(
    vk: &[u8],
    proof: &[u8],
    public_inputs: &[u8],
) -> Result<Option<bool>> {
    with_default_host_dynlib(|dynlib| {
        if dynlib.supports_groth16_verify_v1() {
            return dynlib.groth16_verify_v1(vk, proof, public_inputs);
        }
        if dynlib.supports_groth16_verify_batch_v1() {
            let proofs_wire = encode_single_blob_wire_v1(proof, "groth16 proof")?;
            let public_inputs_wire =
                encode_single_blob_wire_v1(public_inputs, "groth16 public inputs")?;
            let out = dynlib.groth16_verify_batch_v1(vk, &proofs_wire, &public_inputs_wire)?;
            return Ok(*out.first().unwrap_or(&false));
        }
        bail!("aoem_groth16_verify_v1/verify_batch_v1 not supported by loaded DLL");
    })
    .or_else(|_| Ok(None))
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
    pub fn execute(&self, input: &[u8]) -> Result<Vec<u8>> {
        let Some(exec) = self.dynlib.execute else {
            bail!("aoem_execute not found in loaded DLL");
        };
        let mut output_ptr: *mut u8 = ptr::null_mut();
        let mut output_len = 0usize;
        let rc = unsafe {
            exec(
                self.raw,
                input.as_ptr(),
                input.len(),
                &mut output_ptr as *mut *mut u8,
                &mut output_len as *mut usize,
            )
        };
        if rc != 0 {
            let err = unsafe { cstr_to_string((self.dynlib.last_error)(self.raw)) }
                .unwrap_or_else(|| format!("aoem_execute failed rc={rc} and no last_error"));
            bail!("aoem_execute failed: rc={rc}, err={err}");
        }
        self.dynlib
            .copy_aoem_owned_bytes(output_ptr, output_len, "aoem_execute output")
    }

    pub fn execute_batch(&self, input: &[u8]) -> Result<Vec<u8>> {
        let Some(exec) = self.dynlib.execute_batch else {
            bail!("aoem_execute_batch not found in loaded DLL");
        };
        let mut output_ptr: *mut u8 = ptr::null_mut();
        let mut output_len = 0usize;
        let rc = unsafe {
            exec(
                self.raw,
                input.as_ptr(),
                input.len(),
                &mut output_ptr as *mut *mut u8,
                &mut output_len as *mut usize,
            )
        };
        if rc != 0 {
            let err = unsafe { cstr_to_string((self.dynlib.last_error)(self.raw)) }
                .unwrap_or_else(|| format!("aoem_execute_batch failed rc={rc} and no last_error"));
            bail!("aoem_execute_batch failed: rc={rc}, err={err}");
        }
        self.dynlib
            .copy_aoem_owned_bytes(output_ptr, output_len, "aoem_execute_batch output")
    }

    pub fn execute_primitive_v1(
        &self,
        primitive_kind: u32,
        backend_request: u32,
        vendor_id: u32,
        values: &[u32],
        indices: &[u32],
    ) -> Result<(AoemPrimitiveResultV1, Vec<u8>)> {
        let Some(exec) = self.dynlib.execute_primitive_v1 else {
            bail!("aoem_execute_primitive_v1 not found in loaded DLL");
        };
        if values.len() > u32::MAX as usize {
            bail!(
                "aoem_execute_primitive_v1 values too large: {}",
                values.len()
            );
        }
        if indices.len() > u32::MAX as usize {
            bail!(
                "aoem_execute_primitive_v1 indices too large: {}",
                indices.len()
            );
        }
        let values_ptr = if values.is_empty() {
            ptr::null()
        } else {
            values.as_ptr()
        };
        let indices_ptr = if indices.is_empty() {
            ptr::null()
        } else {
            indices.as_ptr()
        };
        let mut result = AoemPrimitiveResultV1::default();
        let mut output_ptr: *mut u8 = ptr::null_mut();
        let mut output_len = 0usize;
        let rc = unsafe {
            exec(
                self.raw,
                primitive_kind,
                backend_request,
                vendor_id,
                values_ptr,
                values.len() as u32,
                indices_ptr,
                indices.len() as u32,
                &mut result as *mut AoemPrimitiveResultV1,
                &mut output_ptr as *mut *mut u8,
                &mut output_len as *mut usize,
            )
        };
        if rc != 0 {
            let err = unsafe { cstr_to_string((self.dynlib.last_error)(self.raw)) }.unwrap_or_else(
                || format!("aoem_execute_primitive_v1 failed rc={rc} and no last_error"),
            );
            bail!("aoem_execute_primitive_v1 failed: rc={rc}, err={err}");
        }
        let output = self.dynlib.copy_aoem_owned_bytes(
            output_ptr,
            output_len,
            "aoem_execute_primitive output",
        )?;
        Ok((result, output))
    }

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
