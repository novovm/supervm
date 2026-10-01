use anyhow::Result;
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions, ExecOpV2};
use std::path::PathBuf;

fn platform_dll_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "aoem_ffi.dll"
    }
    #[cfg(target_os = "linux")]
    {
        "libaoem_ffi.so"
    }
    #[cfg(target_os = "macos")]
    {
        "libaoem_ffi.dylib"
    }
}

fn resolve_default_dll_arg() -> Option<String> {
    for env_name in ["NOVOVM_AOEM_DLL", "AOEM_FFI_DLL"] {
        if let Ok(raw) = std::env::var(env_name) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    let dll_name = platform_dll_name();
    let candidates = [
        PathBuf::from("aoem").join("bin").join(dll_name),
        PathBuf::from("bin").join(dll_name),
        PathBuf::from(dll_name),
    ];
    for candidate in candidates {
        if candidate.exists() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

fn main() -> Result<()> {
    let dll = std::env::args()
        .nth(1)
        .or_else(resolve_default_dll_arg)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "missing AOEM dll path: pass argv[1] or set NOVOVM_AOEM_DLL/AOEM_FFI_DLL"
            )
        })?;

    let facade = AoemExecFacade::open(
        &dll,
        AoemExecOpenOptions {
            ingress_workers: Some(16),
        },
    )?;
    let session = facade.create_session()?;

    let mut key = 42u64.to_le_bytes();
    let mut value = 7u64.to_le_bytes();
    let op = ExecOpV2 {
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

    let out = session.submit_ops(&[op])?;
    println!(
        "ok: submitted={} processed={} success={} writes={} elapsed_us={}",
        out.metrics.submitted_ops,
        out.metrics.processed_ops,
        out.metrics.success_ops,
        out.metrics.total_writes,
        out.metrics.elapsed_us
    );
    Ok(())
}
