// Fixed migration source: 595a092 (portable AORCP002) runtime/novovm-aoem/src/receipt.rs.
//! Portable C ABI v1, with the AOEM RISC0 2.3.2 `AORCP002` envelope.
//! Requires an explicitly selected matching proof library, not any older bundle.
//! No trace/profile fallback, native computation handle or Host business policy.

use crate::resident::abi::{AbiVersion, GlobalInit};
use anyhow::{ensure, Context, Result};
use libloading::Library;
use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::path::Path;

const MAX_ELF_BYTES: usize = 64 * 1024 * 1024;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"AORCP002";

// image_id addresses eight host-endian u32 words, not an ELF SHA256. All
// slices remain alive through these synchronous calls and do not alias output.
type Prove = unsafe extern "C" fn(
    *const u8,
    usize,
    *const u8,
    usize,
    *const u32,
    *mut *mut u8,
    *mut usize,
) -> i32;
type Verify = unsafe extern "C" fn(*const u8, usize, *const u32, *const u8, usize) -> i32;
type Free = unsafe extern "C" fn(*mut u8, usize);

/// Admission/copy limits; these do not bound the native prover's working memory
/// or runtime. Limits can tighten, but never exceed, the packaged ABI ceilings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReceiptLimits {
    pub elf_bytes: usize,
    pub input_bytes: usize,
    pub receipt_bytes: usize,
    pub journal_bytes: usize,
}

impl Default for ReceiptLimits {
    fn default() -> Self {
        Self {
            elf_bytes: MAX_ELF_BYTES,
            input_bytes: MAX_BYTES,
            receipt_bytes: MAX_BYTES,
            journal_bytes: MAX_BYTES,
        }
    }
}

impl ReceiptLimits {
    fn validate(self) -> Result<()> {
        ensure!(
            (1..=MAX_ELF_BYTES).contains(&self.elf_bytes)
                && self.input_bytes <= MAX_BYTES
                && (MAGIC.len()..=MAX_BYTES).contains(&self.receipt_bytes)
                && self.journal_bytes <= MAX_BYTES,
            "receipt limits exceed ABI bounds or cannot hold an envelope"
        );
        Ok(())
    }

    fn validate_prove(self, elf: usize, input: usize) -> Result<()> {
        self.validate()?;
        ensure!(
            elf > 0 && elf <= self.elf_bytes && input <= self.input_bytes,
            "receipt ELF/input length outside configured bounds"
        );
        Ok(())
    }

    fn validate_verify(self, receipt: &[u8], journal: usize) -> Result<()> {
        self.validate()?;
        ensure!(
            receipt.len() <= self.receipt_bytes
                && receipt.starts_with(MAGIC)
                && journal <= self.journal_bytes,
            "receipt envelope/journal length outside configured bounds"
        );
        Ok(())
    }
}

/// Native status -5. Callers can distinguish unavailable backends using
/// `anyhow::Error::downcast_ref`, without parsing diagnostic strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReceiptBackendUnavailable {
    operation: &'static str,
}

impl ReceiptBackendUnavailable {
    pub fn operation(&self) -> &'static str {
        self.operation
    }
}

impl fmt::Display for ReceiptBackendUnavailable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "AOEM receipt backend unavailable for {}",
            self.operation
        )
    }
}

impl std::error::Error for ReceiptBackendUnavailable {}

fn require_success(operation: &'static str, status: i32) -> Result<()> {
    if status == -5 {
        return Err(ReceiptBackendUnavailable { operation }.into());
    }
    ensure!(
        status == 0,
        "AOEM receipt {operation} failed: status={status}"
    );
    Ok(())
}

fn initialize(version: AbiVersion, init: GlobalInit) -> Result<()> {
    ensure!(
        unsafe { version() } == 1,
        "AOEM receipt ABI version mismatch"
    );
    let status = unsafe { init() };
    ensure!(
        status == 0,
        "AOEM receipt initialization failed: status={status}"
    );
    Ok(())
}

struct ReceiptApi {
    prove: Prove,
    verify: Verify,
    free: Free,
}

struct ReceiptLibrary {
    // Like compute/storage, native load/init may establish process-resident
    // workers. No global shutdown ABI permits unloading, even on open failure.
    _library: ManuallyDrop<Library>,
    api: ReceiptApi,
}

/// Synchronous, domain-neutral receipt adapter for a designated proof owner.
///
/// Calls have no cancellation/deadline ABI. A caller-side timeout must not
/// abandon native work, release its inputs or unload its library. Hard resource
/// isolation requires a separate process, not a detached Rust worker thread.
///
/// The session is movable to its owner but not shareable for concurrent calls.
/// Opening another session does not establish native parallel-call safety.
///
/// ```compile_fail
/// fn shared<T: Sync>() {}
/// shared::<aoem_bindings::resident::ReceiptSession>();
/// ```
pub struct ReceiptSession {
    library: ReceiptLibrary,
    limits: ReceiptLimits,
    _single_owner: PhantomData<Cell<()>>,
}

impl ReceiptSession {
    /// Opens exactly this existing trusted library. ABI/version checks do not
    /// authenticate a binary. Configure and freeze native sidecar/environment
    /// policy before startup; this adapter neither reads discovery overrides nor
    /// modifies the environment. Native initialization can itself perform I/O.
    ///
    /// No AOEM computation/storage context is created. Missing symbols fail
    /// closed; exported symbols alone do not establish backend availability.
    pub fn open(path: &Path, limits: ReceiptLimits) -> Result<Self> {
        limits.validate()?;
        let path = path
            .canonicalize()
            .context("resolve explicit AOEM receipt library")?;
        ensure!(path.is_file(), "AOEM receipt library path is not a file");
        // SAFETY: the caller supplies trusted code with the packaged C ABI.
        let library =
            ManuallyDrop::new(unsafe { Library::new(&path) }.context("load AOEM receipt library")?);
        // Resolve free before any producer call can allocate. Keep the module
        // resident also when symbol resolution, ABI checking or init fails.
        let (version, init, free, prove, verify) = unsafe {
            (
                *library.get::<AbiVersion>(b"aoem_abi_version\0")?,
                *library.get::<GlobalInit>(b"aoem_global_init\0")?,
                *library.get::<Free>(b"aoem_free\0")?,
                *library.get::<Prove>(b"aoem_risc0_prove_v1\0")?,
                *library.get::<Verify>(b"aoem_risc0_verify_v1\0")?,
            )
        };
        initialize(version, init)?;
        Ok(Self {
            library: ReceiptLibrary {
                _library: library,
                api: ReceiptApi {
                    prove,
                    verify,
                    free,
                },
            },
            limits,
            _single_owner: PhantomData,
        })
    }

    pub fn limits(&self) -> ReceiptLimits {
        self.limits
    }

    /// Runs a trusted local guest and returns an opaque, NOT YET VERIFIED
    /// receipt. The owner must separately verify independently selected image
    /// and exact journal pins. This method grants no candidate/finality rights.
    /// The combined user+kernel program/image must come from trusted build
    /// policy, not from a remote receipt. The v1 ABI still calls this input ELF.
    pub fn prove(&mut self, elf: &[u8], input: &[u8], image: &[u32; 8]) -> Result<Vec<u8>> {
        self.library.api.prove(self.limits, elf, input, image)
    }

    /// Performs real backend verification against independently trusted image
    /// and complete expected journal bytes. Empty journal means exactly empty,
    /// never skip/wildcard. `AORCP002` alone does not establish proof validity.
    /// Successful verification proves only this pinned guest's statement; it
    /// does not validate unrelated business claims or authorize publication.
    pub fn verify(
        &mut self,
        receipt: &[u8],
        image: &[u32; 8],
        expected_journal: &[u8],
    ) -> Result<()> {
        self.library
            .api
            .verify(self.limits, receipt, image, expected_journal)
    }
}

impl ReceiptApi {
    fn prove(
        &self,
        limits: ReceiptLimits,
        elf: &[u8],
        input: &[u8],
        image: &[u32; 8],
    ) -> Result<Vec<u8>> {
        limits.validate_prove(elf.len(), input.len())?;
        let mut pointer = std::ptr::null_mut();
        let mut length = 0;
        // SAFETY: input regions/out slots are disjoint and valid until this
        // synchronous call returns. This ABI retains no caller pointers.
        let status = unsafe {
            (self.prove)(
                elf.as_ptr(),
                elf.len(),
                input.as_ptr(),
                input.len(),
                image.as_ptr(),
                &mut pointer,
                &mut length,
            )
        };
        let response = ReceiptBuffer {
            pointer,
            length,
            free: self.free,
        };
        require_success("prove", status)?;
        ensure!(
            !response.pointer.is_null()
                && (MAGIC.len()..=limits.receipt_bytes).contains(&response.length),
            "AOEM receipt producer returned an invalid buffer"
        );
        // SAFETY: the trusted ABI promises this allocation/length. Bound it
        // before constructing a slice; never dereference a failed/oversize result.
        let bytes = unsafe { std::slice::from_raw_parts(response.pointer, response.length) };
        ensure!(
            bytes.starts_with(MAGIC),
            "AOEM producer returned a non-receipt envelope"
        );
        let mut owned = Vec::new();
        owned
            .try_reserve_exact(bytes.len())
            .context("allocate owned receipt copy")?;
        owned.extend_from_slice(bytes);
        // RAII releases the native allocation on success, any error, or unwind.
        Ok(owned)
    }

    fn verify(
        &self,
        limits: ReceiptLimits,
        receipt: &[u8],
        image: &[u32; 8],
        expected_journal: &[u8],
    ) -> Result<()> {
        limits.validate_verify(receipt, expected_journal.len())?;
        // SAFETY: all pinned inputs are readable for the whole synchronous call.
        let status = unsafe {
            (self.verify)(
                receipt.as_ptr(),
                receipt.len(),
                image.as_ptr(),
                expected_journal.as_ptr(),
                expected_journal.len(),
            )
        };
        require_success("verify", status)
    }
}

struct ReceiptBuffer {
    pointer: *mut u8,
    length: usize,
    free: Free,
}

impl Drop for ReceiptBuffer {
    fn drop(&mut self) {
        if !self.pointer.is_null() {
            // SAFETY: the exact allocation belongs to this guard, including
            // malformed producer/error responses, and uses its allocating DLL.
            unsafe { (self.free)(self.pointer, self.length) };
        }
    }
}

#[cfg(test)]
mod tests;
