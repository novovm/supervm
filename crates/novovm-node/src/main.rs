// Copyright (c) 2026 Xonovo Technology
// All rights reserved.
// Author: Xonovo Technology

use anyhow::{bail, Result};
use novovm_exec::{AoemExecFacade, AoemRuntimeConfig, ExecOpV2};

fn exec_path_mode() -> String {
    std::env::var("NOVOVM_EXEC_PATH")
        .or_else(|_| std::env::var("SUPERVM_EXEC_PATH"))
        .unwrap_or_else(|_| "ffi_v2".to_string())
}

fn run_ffi_v2() -> Result<()> {
    let runtime = AoemRuntimeConfig::from_env()?;
    let facade = AoemExecFacade::open_with_runtime(&runtime)?;
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
    let report = session.submit_ops_report(&[op]);
    if !report.ok {
        let err = report
            .error
            .as_ref()
            .map(|e| e.message.as_str())
            .unwrap_or("unknown");
        bail!(
            "mode=ffi_v2 rc={}({}) err={}",
            report.return_code,
            report.return_code_name,
            err
        );
    }

    let out = report
        .output
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing output on success report"))?;
    println!(
        "mode=ffi_v2 variant={} dll={} rc={}({}) submitted={} processed={} success={} writes={} elapsed_us={}",
        runtime.variant.as_str(),
        runtime.dll_path.display(),
        report.return_code,
        report.return_code_name,
        out.metrics.submitted_ops,
        out.metrics.processed_ops,
        out.metrics.success_ops,
        out.metrics.total_writes,
        out.metrics.elapsed_us
    );
    // Keep AOEM DLL resident for process lifetime to avoid Windows teardown races at process exit.
    drop(session);
    std::mem::forget(facade);
    Ok(())
}

fn run_legacy_compat() -> Result<()> {
    // Keep legacy entrypoint for one compatibility window and forward to unified FFI V2 path.
    println!("mode=legacy_compat route=ffi_v2");
    run_ffi_v2()
}

fn main() -> Result<()> {
    let mode = exec_path_mode();
    match mode.as_str() {
        "ffi_v2" => run_ffi_v2(),
        "legacy" => run_legacy_compat(),
        _ => bail!("unknown exec path mode: {mode}; valid: ffi_v2|legacy"),
    }
}
