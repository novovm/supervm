// Copyright (c) 2026 Xonovo Technology
// All rights reserved.
// Author: Xonovo Technology

use anyhow::{bail, Result};
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions, ExecOpV2};

fn exec_path_mode() -> String {
    std::env::var("NOVOVM_EXEC_PATH")
        .or_else(|_| std::env::var("SUPERVM_EXEC_PATH"))
        .unwrap_or_else(|_| "ffi_v2".to_string())
}

fn aoem_dll_path() -> String {
    std::env::var("NOVOVM_AOEM_DLL")
        .or_else(|_| std::env::var("AOEM_DLL"))
        .unwrap_or_else(|_| "D:\\WorksArea\\SUPERVM\\aoem\\bin\\aoem_ffi.dll".to_string())
}

fn ingress_workers() -> Option<u32> {
    let raw = std::env::var("NOVOVM_INGRESS_WORKERS")
        .or_else(|_| std::env::var("AOEM_INGRESS_WORKERS"));
    match raw {
        Ok(v) => v.parse::<u32>().ok(),
        Err(_) => Some(16),
    }
}

fn run_ffi_v2() -> Result<()> {
    let dll = aoem_dll_path();
    let facade = AoemExecFacade::open(
        &dll,
        AoemExecOpenOptions {
            ingress_workers: ingress_workers(),
        },
    )?;
    let session = facade.create_session()?;

    // Phase2 first main-path cutover: host submit goes through novovm-exec facade.
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
        "mode=ffi_v2 submitted={} processed={} success={} writes={} elapsed_us={}",
        out.metrics.submitted_ops,
        out.metrics.processed_ops,
        out.metrics.success_ops,
        out.metrics.total_writes,
        out.metrics.elapsed_us
    );
    Ok(())
}

fn main() -> Result<()> {
    let mode = exec_path_mode();
    match mode.as_str() {
        "ffi_v2" => run_ffi_v2(),
        "legacy" => {
            bail!("legacy exec path is not migrated in SUPERVM skeleton; use NOVOVM_EXEC_PATH=ffi_v2")
        }
        _ => bail!("unknown exec path mode: {mode}; valid: ffi_v2|legacy"),
    }
}
