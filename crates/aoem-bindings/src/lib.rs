use anyhow::{bail, Result};
use libloading::Library;
use std::path::Path;

pub type AoemAbiVersion = unsafe extern "C" fn() -> u32;
pub type AoemCreate = unsafe extern "C" fn() -> *mut std::ffi::c_void;
pub type AoemDestroy = unsafe extern "C" fn(*mut std::ffi::c_void);
pub type AoemExecute = unsafe extern "C" fn(*mut std::ffi::c_void, *const u8, usize) -> i32;

pub struct AoemDyn {
    _lib: Library,
    abi_version: AoemAbiVersion,
    create: AoemCreate,
    destroy: AoemDestroy,
    execute: AoemExecute,
}

impl AoemDyn {
    pub unsafe fn load(path: impl AsRef<Path>) -> Result<Self> {
        let lib = Library::new(path.as_ref())?;
        let abi_version: AoemAbiVersion = *lib.get::<AoemAbiVersion>(b"aoem_abi_version")?;
        let create: AoemCreate = *lib.get::<AoemCreate>(b"aoem_create")?;
        let destroy: AoemDestroy = *lib.get::<AoemDestroy>(b"aoem_destroy")?;
        let execute: AoemExecute = *lib.get::<AoemExecute>(b"aoem_execute")?;

        Ok(Self {
            _lib: lib,
            abi_version,
            create,
            destroy,
            execute,
        })
    }

    pub unsafe fn smoke(&self) -> Result<i32> {
        let abi = (self.abi_version)();
        if abi != 1 {
            bail!("AOEM ABI mismatch: expected 1, got {abi}");
        }

        let handle = (self.create)();
        if handle.is_null() {
            bail!("aoem_create returned null");
        }

        let payload = b"hello";
        let rc = (self.execute)(handle, payload.as_ptr(), payload.len());
        (self.destroy)(handle);
        Ok(rc)
    }
}
