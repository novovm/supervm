// Fixed migration source: a7db795 runtime/novovm-aoem/src/storage.rs.
//! Same-thread, explicitly opened AOEM binary storage-provider session.
//!
//! The packaged provider (SDK source 56e9da15) maps opcode 5 to one RocksDB
//! WriteBatch with sync=true and WAL enabled, except for its benchmark environment
//! override, which this adapter rejects. An accepted write is not a graph and
//! retains no Host callbacks. Calls block their dedicated I/O owner; a caller's
//! timeout or dropped reply does not cancel a native call or prove no write.
//!
//! Open may create the database: the ABI has no existing-only/read-only option.
//! Reads bypass Host/engine value caches, NOT RocksDB memtables/block caches.
//! There is no flush/cache-bypass ABI; independent restart is a separate check.
//! Only use databases whose value sizes obey these limits. The native provider
//! cannot cap its response allocation before returning a corrupt/foreign value.

use anyhow::{bail, ensure, Context, Result};
use libloading::Library;
use std::ffi::{c_char, c_void, CStr, OsStr};
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::path::Path;
use std::ptr::NonNull;
use std::rc::Rc;

#[path = "storage_wire.rs"]
mod wire;

/// Local resource bounds, not an AOEM or application schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageLimits {
    pub max_items: usize,
    pub max_key_bytes: usize,
    pub max_value_bytes: usize,
    /// Includes the 12-byte wire request header.
    pub max_request_bytes: usize,
    /// Includes the 16-byte wire response header.
    pub max_response_bytes: usize,
}

impl Default for StorageLimits {
    fn default() -> Self {
        Self {
            max_items: 131_072,
            max_key_bytes: 4096,
            max_value_bytes: 1024 * 1024,
            max_request_bytes: 64 * 1024 * 1024,
            max_response_bytes: 128 * 1024 * 1024,
        }
    }
}

impl StorageLimits {
    fn validate(&self) -> Result<()> {
        ensure!(
            (1..=wire::MAX_ITEMS).contains(&self.max_items),
            "storage item limit invalid"
        );
        ensure!(
            self.max_key_bytes > 0 && self.max_key_bytes <= wire::MAX_WIRE_BYTES,
            "storage key limit invalid"
        );
        ensure!(
            self.max_value_bytes > 0 && self.max_value_bytes <= wire::MAX_WIRE_BYTES,
            "storage value limit invalid"
        );
        ensure!(
            (64..=wire::MAX_WIRE_BYTES).contains(&self.max_request_bytes),
            "storage request limit invalid"
        );
        ensure!(
            (64..=wire::MAX_WIRE_BYTES).contains(&self.max_response_bytes),
            "storage response limit invalid"
        );
        ensure!(
            self.max_value_bytes
                .checked_add(25)
                .is_some_and(|size| size <= self.max_response_bytes),
            "storage response limit cannot hold one bounded value"
        );
        Ok(())
    }
}

/// No relaxed durability option is exposed. The wire's sync_every is fixed to
/// one; opcode 5 itself uses the backend's always-synchronous write_batch path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageConfig {
    pub max_open_files: u32,
    pub write_buffer_bytes: u64,
    pub block_cache_bytes: u64,
    pub max_background_jobs: u32,
    pub compression: bool,
    pub limits: StorageLimits,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            max_open_files: 256,
            write_buffer_bytes: 16 * 1024 * 1024,
            block_cache_bytes: 32 * 1024 * 1024,
            max_background_jobs: 4,
            compression: true,
            limits: StorageLimits::default(),
        }
    }
}

impl StorageConfig {
    fn validate(&self) -> Result<()> {
        self.limits.validate()?;
        ensure!(
            self.max_open_files > 0 && i32::try_from(self.max_open_files).is_ok(),
            "storage max_open_files invalid"
        );
        ensure!(
            self.max_background_jobs > 0 && i32::try_from(self.max_background_jobs).is_ok(),
            "storage background jobs invalid"
        );
        ensure!(
            self.write_buffer_bytes > 0 && usize::try_from(self.write_buffer_bytes).is_ok(),
            "storage write buffer invalid"
        );
        ensure!(
            self.block_cache_bytes > 0 && usize::try_from(self.block_cache_bytes).is_ok(),
            "storage block cache invalid"
        );
        Ok(())
    }
}

/// Ordered mutations in a single atomic provider batch. Duplicate keys retain
/// their input order; an empty value is distinct from deletion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageWrite {
    Put { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
}

/// Construct, use and drop on the designated I/O owner. There is no raw handle,
/// global registry, implicit reopen, fallback database or cross-thread FFI access.
///
/// ```compile_fail
/// fn requires_send<T: Send>() {}
/// requires_send::<aoem_bindings::resident::StorageSession>();
/// ```
pub struct StorageSession {
    native: NativeStorage,
    database_id: u64,
    limits: StorageLimits,
    poisoned: bool,
    _same_thread: PhantomData<Rc<()>>,
}

impl StorageSession {
    pub fn open(library: &Path, database: &Path, config: StorageConfig) -> Result<Self> {
        config.validate()?;
        require_environment()?;
        ensure!(!database.as_os_str().is_empty(), "storage path is empty");
        // Resolve cwd now, but do not feed a Windows canonical \\?\ path to
        // RocksDB or invent a default repository-relative database location.
        let database = std::path::absolute(database).context("resolve explicit storage path")?;
        let database = database
            .to_str()
            .context("storage ABI requires a UTF-8 path")?;
        let request = wire::open_request(database, &config)?;
        let native = NativeStorage::open(library)?;
        let database_id = native.request(
            &request,
            wire::OPEN,
            config.limits.max_response_bytes,
            wire::database_id,
        )?;
        Ok(Self {
            native,
            database_id,
            limits: config.limits,
            poisoned: false,
            _same_thread: PhantomData,
        })
    }

    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    pub fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.ensure_usable()?;
        let request = wire::read_request(self.database_id, &[key], true, self.limits)?;
        let result = self.native.request(
            &request,
            wire::GET,
            self.limits.max_response_bytes,
            |payload| wire::values(payload, 1, self.limits),
        );
        match result {
            Ok(mut values) => Ok(values.remove(0)),
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }

    /// A single provider multi_get, preserving order, duplicates and missing
    /// values. No caller-visible prefix is returned on any failure. This method
    /// does not pin a snapshot across subsequent calls or return an authority.
    pub fn multi_get(&mut self, keys: &[Vec<u8>]) -> Result<Vec<Option<Vec<u8>>>> {
        self.ensure_usable()?;
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        // Check before allocating a second attacker-sized keys vector.
        ensure!(
            keys.len() <= self.limits.max_items,
            "storage read item limit exceeded"
        );
        let keys: Vec<_> = keys.iter().map(Vec::as_slice).collect();
        let request = wire::read_request(self.database_id, &keys, false, self.limits)?;
        let result = self.native.request(
            &request,
            wire::MULTI_GET,
            self.limits.max_response_bytes,
            |payload| wire::values(payload, keys.len(), self.limits),
        );
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }

    /// One whole atomic WriteBatch, never transparently split into smaller
    /// commits. Errors before native admission leave the session usable. Once
    /// called, any native/protocol error is UNKNOWN and permanently poisons the
    /// owner: it is not a promise of rollback or permission to retry.
    pub fn atomic_write_batch(&mut self, writes: &[StorageWrite]) -> Result<()> {
        self.ensure_usable()?;
        let request = wire::write_request(self.database_id, writes, self.limits)?;
        // Native code checks the benchmark variable on every write. These
        // read-only checks do not synchronize external environment mutation;
        // the embedding process must freeze its configuration before startup.
        if let Err(error) = require_environment() {
            self.poisoned = true;
            return Err(error);
        }
        self.poisoned = true;
        self.native.request(
            &request,
            wire::WRITE_BATCH,
            self.limits.max_response_bytes,
            |payload| wire::write_ack(payload, writes.len()),
        )?;
        self.poisoned = false;
        Ok(())
    }

    fn ensure_usable(&self) -> Result<()> {
        ensure!(
            !self.poisoned,
            "AOEM storage session poisoned; outcome unknown, no automatic retry/reopen"
        );
        Ok(())
    }
}

fn validate_environment(
    relaxed_sync: Option<&OsStr>,
    persistence_path: Option<&OsStr>,
) -> Result<()> {
    ensure!(relaxed_sync.is_none(), "AOEM durable storage requires AOEM_BENCH_RELAXED_SYNC to be absent (even an empty value is forbidden)");
    ensure!(
        persistence_path.is_none_or(|value| value.is_empty()),
        "AOEM explicit storage requires unset or empty AOEM_PERSISTENCE_PATH"
    );
    Ok(())
}

fn require_environment() -> Result<()> {
    validate_environment(
        std::env::var_os("AOEM_BENCH_RELAXED_SYNC").as_deref(),
        std::env::var_os("AOEM_PERSISTENCE_PATH").as_deref(),
    )
}

#[repr(C)]
struct CreateOptions {
    abi_version: u32,
    struct_size: u32,
    ingress_workers: u32,
    flags: u32,
}
type AbiVersion = unsafe extern "C" fn() -> u32;
type Init = unsafe extern "C" fn() -> i32;
type Create = unsafe extern "C" fn(*const CreateOptions) -> *mut c_void;
type Destroy = unsafe extern "C" fn(*mut c_void);
type Free = unsafe extern "C" fn(*mut u8, usize);
type StorageWire =
    unsafe extern "C" fn(*mut c_void, *const u8, usize, *mut *mut u8, *mut usize) -> i32;
type LastError = unsafe extern "C" fn(*mut c_void) -> *const c_char;

struct StorageLibrary {
    // AOEM has process-global workers and no global-shutdown/unload guarantee.
    // Keep the loaded module resident; normal session contexts still destroy.
    _library: ManuallyDrop<Library>,
    destroy: Destroy,
    free: Free,
    storage: StorageWire,
    last_error: LastError,
}

struct NativeStorage {
    library: StorageLibrary,
    handle: NonNull<c_void>,
}

impl NativeStorage {
    fn open(path: &Path) -> Result<Self> {
        let path = path
            .canonicalize()
            .context("resolve explicit AOEM storage library")?;
        ensure!(path.is_file(), "AOEM storage library path is not a file");
        // SAFETY: only an explicitly trusted library implementing the packaged
        // ABI is supported. Resolving symbols is not binary authentication.
        let library =
            ManuallyDrop::new(unsafe { Library::new(&path) }.context("load AOEM storage library")?);
        let (version, init, create, destroy, free, storage, last_error) = unsafe {
            (
                *library.get::<AbiVersion>(b"aoem_abi_version\0")?,
                *library.get::<Init>(b"aoem_global_init\0")?,
                *library.get::<Create>(b"aoem_create_with_options\0")?,
                *library.get::<Destroy>(b"aoem_destroy\0")?,
                *library.get::<Free>(b"aoem_free\0")?,
                *library.get::<StorageWire>(b"aoem_storage_provider_wire_v1\0")?,
                *library.get::<LastError>(b"aoem_last_error\0")?,
            )
        };
        ensure!(
            unsafe { version() } == 1,
            "AOEM storage ABI version mismatch"
        );
        ensure!(
            unsafe { init() } == 0,
            "AOEM storage global initialization failed"
        );
        let options = CreateOptions {
            abi_version: 1,
            struct_size: std::mem::size_of::<CreateOptions>() as u32,
            ingress_workers: 1,
            flags: 0,
        };
        let handle = NonNull::new(unsafe { create(&options) })
            .context("AOEM storage context creation failed")?;
        Ok(Self {
            library: StorageLibrary {
                _library: library,
                destroy,
                free,
                storage,
                last_error,
            },
            handle,
        })
    }

    fn request<T>(
        &self,
        request: &[u8],
        opcode: u16,
        max_response: usize,
        decode: impl FnOnce(&[u8]) -> Result<T>,
    ) -> Result<T> {
        let mut pointer = std::ptr::null_mut();
        let mut length = 0;
        // SAFETY: request lives through this synchronous call; out slots are
        // valid. The ABI retains neither request pointers nor Host callbacks.
        let status = unsafe {
            (self.library.storage)(
                self.handle.as_ptr(),
                request.as_ptr(),
                request.len(),
                &mut pointer,
                &mut length,
            )
        };
        let response = ResponseBuffer {
            pointer,
            length,
            free: self.library.free,
        };
        if pointer.is_null() {
            let detail = unsafe { (self.library.last_error)(self.handle.as_ptr()) };
            let detail = if detail.is_null() {
                "no native detail".into()
            } else {
                unsafe { CStr::from_ptr(detail) }
                    .to_string_lossy()
                    .into_owned()
            };
            bail!("AOEM storage missing response: status={status}, {detail}");
        }
        ensure!(
            length >= 16 && length <= max_response,
            "AOEM storage response length outside bound"
        );
        // SAFETY: a trusted provider returns one aoem_free-owned allocation of
        // this size. The size bound is checked before making a Rust slice.
        let bytes = unsafe { std::slice::from_raw_parts(response.pointer, response.length) };
        decode(wire::response_payload(bytes, opcode, status)?)
    }
}

struct ResponseBuffer {
    pointer: *mut u8,
    length: usize,
    free: Free,
}
impl Drop for ResponseBuffer {
    fn drop(&mut self) {
        if !self.pointer.is_null() {
            // SAFETY: the exact allocation is owned by this guard on all
            // success/error/unwind paths, using the allocating module's free.
            unsafe { (self.free)(self.pointer, self.length) };
        }
    }
}

impl Drop for NativeStorage {
    fn drop(&mut self) {
        // Every storage call has returned before the owner can drop. No Host
        // callbacks or admitted asynchronous graphs exist in this context.
        // A failed write's application result is unknown, not its call lifetime.
        unsafe { (self.library.destroy)(self.handle.as_ptr()) };
    }
}

#[cfg(test)]
mod tests;
