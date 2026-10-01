use crate::abi::*;
use anyhow::{bail, Context, Result};
use libloading::Library;
use std::ffi::{c_void, CStr, OsStr};
use std::mem::ManuallyDrop;
use std::path::Path;
use std::ptr::NonNull;

/// Only the required compute ABI. No global Host registry or runtime facade.
struct ComputeLibrary {
    // AOEM global initialization can own process-resident workers. Retain the
    // native module until process exit, including initialization failure. The
    // API has no global shutdown contract that would permit safe DLL unloading.
    // Session handles are still destroyed when their complete flights drain.
    _library: ManuallyDrop<Library>,
    destroy: Destroy,
    submit: Submit,
    cancel: Cancel,
    active_count: ActiveCount,
    last_error: LastError,
}

pub(crate) struct NativeSession {
    library: ComputeLibrary,
    handle: NonNull<c_void>,
}

impl NativeSession {
    pub(crate) fn open(path: &Path, ingress_workers: u32) -> Result<Self> {
        // The packaged create ABI has no per-context persistence-disable flag.
        // Reject rather than mutate process-wide settings or accidentally open
        // another component's database. This cannot synchronize environment
        // changes made outside this crate; the caller must freeze configuration.
        require_compute_environment(std::env::var_os("AOEM_PERSISTENCE_PATH").as_deref())?;
        // Resolve the caller's exact existing file before the platform loader;
        // never search a repository, environment override, or default DLL name.
        let path = path.canonicalize().context("resolve AOEM library path")?;
        if !path.is_file() {
            bail!("AOEM library path is not a file");
        }
        // SAFETY: the supplied path must name trusted native code implementing
        // the packaged ABI. Symbol types below are the exact header subset.
        let library = ManuallyDrop::new(
            unsafe { Library::new(&path) }
                .with_context(|| format!("load AOEM library {}", path.display()))?,
        );
        // Keep the module resident even if symbols/init fail: load-time native
        // initialization itself may have created process-resident workers.
        let (abi, init, create, destroy, submit, cancel, active_count, last_error) = unsafe {
            (
                *library.get::<AbiVersion>(b"aoem_abi_version\0")?,
                *library.get::<GlobalInit>(b"aoem_global_init\0")?,
                *library.get::<Create>(b"aoem_create_with_options\0")?,
                *library.get::<Destroy>(b"aoem_destroy\0")?,
                *library.get::<Submit>(b"aoem_submit_semantic_graph_v2\0")?,
                *library.get::<Cancel>(b"aoem_cancel_semantic_graph_v2\0")?,
                *library.get::<ActiveCount>(b"aoem_semantic_graph_v2_active_count\0")?,
                *library.get::<LastError>(b"aoem_last_error\0")?,
            )
        };
        let version = unsafe { abi() };
        if version != 1 {
            bail!("AOEM library ABI mismatch: expected 1, got {version}");
        }
        let status = unsafe { init() };
        if status != OK {
            bail!("AOEM global initialization failed: status={status}");
        }
        let options = CreateOptions {
            abi_version: 1,
            struct_size: std::mem::size_of::<CreateOptions>() as u32,
            ingress_workers,
            flags: 0,
        };
        let library = ComputeLibrary {
            _library: library,
            destroy,
            submit,
            cancel,
            active_count,
            last_error,
        };
        let handle = NonNull::new(unsafe { create(&options) }).with_context(|| {
            format!(
                "AOEM create failed: {}",
                library.error(std::ptr::null_mut())
            )
        })?;
        Ok(Self { library, handle })
    }

    /// All pointees are owned by the caller's complete, retained ComputeFlight.
    pub(crate) unsafe fn submit(
        &self,
        seeds: &[TaskDescriptor],
        options: &SubmitOptions,
        callbacks: &Callbacks,
    ) -> Result<i32> {
        let count = u32::try_from(seeds.len()).context("too many AOEM graph seeds")?;
        let status = unsafe {
            (self.library.submit)(
                self.handle.as_ptr(),
                seeds.as_ptr(),
                count,
                options,
                callbacks,
            )
        };
        if status < 0 {
            bail!(
                "AOEM graph submit failed: status={status}, {}",
                self.library.error(self.handle.as_ptr())
            );
        }
        Ok(status)
    }

    pub(crate) fn cancel(&self, graph_id: u64) -> Result<()> {
        let status = unsafe { (self.library.cancel)(self.handle.as_ptr(), graph_id) };
        if status != OK {
            bail!(
                "AOEM graph cancel failed: status={status}, {}",
                self.library.error(self.handle.as_ptr())
            );
        }
        Ok(())
    }

    pub(crate) fn active_count(&self) -> u64 {
        unsafe { (self.library.active_count)(self.handle.as_ptr()) }
    }
}

fn require_compute_environment(persistence_path: Option<&OsStr>) -> Result<()> {
    if persistence_path.is_some_and(|value| !value.is_empty()) {
        bail!("AOEM computation requires an unset or empty AOEM_PERSISTENCE_PATH; configure the process before startup");
    }
    Ok(())
}

impl ComputeLibrary {
    fn error(&self, handle: *mut c_void) -> String {
        let error = unsafe { (self.last_error)(handle) };
        if error.is_null() {
            return "no native error detail".into();
        }
        unsafe { CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned()
    }
}

impl Drop for NativeSession {
    fn drop(&mut self) {
        // The private session can drop only with no flight, or after the flight
        // proved Host-state drain. SDK 56e9da15 aoem_destroy first shuts down
        // semantic_graph_v2 and joins every worker, including completion's
        // native epilogue, before dropping the native context. Unknown/admitted
        // work retains the whole session owner instead of entering destroy.
        unsafe { (self.library.destroy)(self.handle.as_ptr()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computation_rejects_ambient_persistence_without_mutating_environment() {
        assert!(require_compute_environment(None).is_ok());
        assert!(require_compute_environment(Some(OsStr::new(""))).is_ok());
        for path in ["private-db-path", " ", "\t"] {
            let error = require_compute_environment(Some(OsStr::new(path)))
                .unwrap_err()
                .to_string();
            assert!(error.contains("AOEM_PERSISTENCE_PATH"));
            assert!(!error.contains("private-db-path"));
        }
    }
}
