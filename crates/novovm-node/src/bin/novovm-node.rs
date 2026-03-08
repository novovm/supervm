// Copyright (c) 2026 Xonovo Technology
// All rights reserved.
// Author: Xonovo Technology

#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_exec::AoemRuntimeConfig;
use novovm_node::tx_ingress::{
    available_ingress_codecs, load_exec_batch_from_wire_file, load_ops_wire_v1_file,
    load_ops_wire_v1_from_tx_wire_file, load_ops_wire_v1_payload_file,
    LOCAL_TX_WIRE_CODEC_WRITE_U64LE_V1,
};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum D1IngressMode {
    Auto,
    OpsWireV1,
    OpsV2,
}

fn ingress_mode_env() -> Result<D1IngressMode> {
    let raw = std::env::var("NOVOVM_D1_INGRESS_MODE").unwrap_or_else(|_| "auto".to_string());
    match raw.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(D1IngressMode::Auto),
        "ops_wire_v1" | "wire_v1" => Ok(D1IngressMode::OpsWireV1),
        "ops_v2" | "v2" => Ok(D1IngressMode::OpsV2),
        _ => bail!("invalid NOVOVM_D1_INGRESS_MODE={raw}; valid: auto|ops_wire_v1|ops_v2"),
    }
}

fn bool_env(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            let v = v.trim();
            v == "1"
                || v.eq_ignore_ascii_case("true")
                || v.eq_ignore_ascii_case("on")
                || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}

fn string_env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn exec_path_mode() -> String {
    std::env::var("NOVOVM_EXEC_PATH")
        .or_else(|_| std::env::var("SUPERVM_EXEC_PATH"))
        .unwrap_or_else(|_| "ffi_v2".to_string())
}

fn repeat_count_env() -> Result<usize> {
    let raw = std::env::var("NOVOVM_TX_REPEAT_COUNT").unwrap_or_else(|_| "1".to_string());
    let parsed: usize = raw
        .trim()
        .parse()
        .with_context(|| format!("invalid NOVOVM_TX_REPEAT_COUNT={raw}"))?;
    if parsed == 0 {
        bail!("NOVOVM_TX_REPEAT_COUNT must be >= 1");
    }
    Ok(parsed)
}

fn main() -> Result<()> {
    let node_mode = std::env::var("NOVOVM_NODE_MODE").unwrap_or_else(|_| "full".to_string());
    if !node_mode.eq_ignore_ascii_case("full") {
        bail!(
            "non-full node_mode is disabled: novovm-node keeps only production path"
        );
    }

    let mode = exec_path_mode();
    if mode != "ffi_v2" {
        bail!("non-production exec path mode ({mode}) is disabled: only ffi_v2 is allowed");
    }

    if bool_env("NOVOVM_ENABLE_HOST_ADMISSION") {
        bail!("NOVOVM_ENABLE_HOST_ADMISSION=1 is disabled on novovm-node production binary");
    }

    let runtime = AoemRuntimeConfig::from_env()?;
    let facade = novovm_exec::AoemExecFacade::open_with_runtime(&runtime)?;
    let session = facade.create_session()?;
    let runtime_plugin_dir = runtime
        .plugin_dir
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "-".to_string());
    println!(
        "aoem_runtime_in: variant={} dll={} persist_backend={} wasm_runtime={} zkvm_mode={} mldsa_mode={} ingress_workers={} plugin_dir={}",
        runtime.variant.as_str(),
        runtime.dll_path.display(),
        runtime.persist_backend,
        runtime.wasm_runtime,
        runtime.zkvm_mode,
        runtime.mldsa_mode,
        runtime.ingress_workers.unwrap_or(0),
        runtime_plugin_dir
    );

    let tx_wire_path = string_env_nonempty("NOVOVM_TX_WIRE_FILE").map(PathBuf::from);
    let ops_wire_path = string_env_nonempty("NOVOVM_OPS_WIRE_FILE").map(PathBuf::from);
    if tx_wire_path.is_some() && ops_wire_path.is_some() {
        bail!("ingress source conflict: set only one of NOVOVM_TX_WIRE_FILE or NOVOVM_OPS_WIRE_FILE");
    }
    if tx_wire_path.is_none() && ops_wire_path.is_none() {
        bail!("no ingress supplied for production path: set NOVOVM_OPS_WIRE_FILE=<path> or NOVOVM_TX_WIRE_FILE=<path>");
    }
    let selected_codec = string_env_nonempty("NOVOVM_D1_CODEC");
    let ingress_mode = ingress_mode_env()?;
    let supports_wire_v1 = facade.supports_ops_wire_v1();
    let use_wire_v1 = match ingress_mode {
        D1IngressMode::Auto => supports_wire_v1,
        D1IngressMode::OpsWireV1 => {
            if !supports_wire_v1 {
                bail!("NOVOVM_D1_INGRESS_MODE=ops_wire_v1 requested, but loaded AOEM does not export aoem_execute_ops_wire_v1");
            }
            true
        }
        D1IngressMode::OpsV2 => false,
    };

    let (tx_count, batch, input_source, d1_codec, aoem_ingress_path) = if use_wire_v1 {
        let payload = if let Some(path) = ops_wire_path.as_ref() {
            if selected_codec.is_some() {
                bail!("NOVOVM_D1_CODEC is not allowed with NOVOVM_OPS_WIRE_FILE (already encoded ops wire)");
            }
            load_ops_wire_v1_file(path)?
        } else {
            let path = tx_wire_path.as_ref().expect("tx wire path must exist");
            let codec = selected_codec
                .clone()
                .unwrap_or_else(|| LOCAL_TX_WIRE_CODEC_WRITE_U64LE_V1.to_string());
            if selected_codec.is_some() {
                load_ops_wire_v1_payload_file(path, &codec).with_context(|| {
                    format!(
                        "encode ingress payload with codec={codec} failed; available={:?}",
                        available_ingress_codecs()
                    )
                })?
            } else {
                load_ops_wire_v1_from_tx_wire_file(path)?
            }
        };
        let codec = if ops_wire_path.is_some() {
            "-".to_string()
        } else {
            selected_codec
                .clone()
                .unwrap_or_else(|| LOCAL_TX_WIRE_CODEC_WRITE_U64LE_V1.to_string())
        };
        let path_mode = "ops_wire_v1".to_string();
        let source = if ops_wire_path.is_some() {
            "ops_wire_v1".to_string()
        } else {
            "tx_wire".to_string()
        };
        println!(
            "d1_ingress_mode: selected=ops_wire_v1 requested={:?} auto_supported={} codec={}",
            ingress_mode,
            supports_wire_v1,
            codec
        );
        (payload.op_count, EitherBatch::Wire(payload.bytes), source, codec, path_mode)
    } else {
        if ops_wire_path.is_some() {
            bail!("NOVOVM_OPS_WIRE_FILE requires ops_wire_v1 path; current selected mode=ops_v2");
        }
        if selected_codec.is_some() {
            bail!("NOVOVM_D1_CODEC requires ops_wire_v1 ingress; current selected mode=ops_v2");
        }
        let ingress_path = tx_wire_path.as_ref().expect("tx wire path must exist");
        let payload = load_exec_batch_from_wire_file(&ingress_path, |_, rec| {
            (rec.account << 32) | rec.nonce.saturating_add(1)
        })?;
        let path_mode = if ingress_mode == D1IngressMode::Auto && !supports_wire_v1 {
            "ops_v2_fallback".to_string()
        } else {
            "ops_v2_forced".to_string()
        };
        println!(
            "d1_ingress_mode: selected=ops_v2 requested={:?} auto_supported={}",
            ingress_mode, supports_wire_v1
        );
        (
            payload.len(),
            EitherBatch::Ops(payload.ops),
            "tx_wire".to_string(),
            "-".to_string(),
            path_mode,
        )
    };
    println!(
        "d1_ingress_contract: mode={} source={} codec={} aoem_ingress_path={}",
        if use_wire_v1 { "ops_wire_v1" } else { "ops_v2" },
        input_source,
        d1_codec,
        aoem_ingress_path
    );
    println!(
        "tx_ingress_source: mode={} txs={} host_admission=false",
        input_source, tx_count
    );
    let repeat_count = repeat_count_env()?;

    let exec_loop_sw = Instant::now();
    let mut submitted_total: u64 = 0;
    let mut processed_total: u64 = 0;
    let mut success_total: u64 = 0;
    let mut writes_total: u64 = 0;
    let mut aoem_exec_us_total: u64 = 0;

    for idx in 0..repeat_count {
        let iter_sw = Instant::now();
        let report = match &batch {
            EitherBatch::Ops(ops) => session.submit_ops_report(ops),
            EitherBatch::Wire(wire) => session.submit_ops_wire_report(wire),
        };
        let host_elapsed_us = iter_sw.elapsed().as_micros() as u64;
        if !report.ok {
            let err = report
                .error
                .as_ref()
                .map(|e| e.message.as_str())
                .unwrap_or("unknown");
            bail!(
                "mode=ffi_v2 run={}/{} rc={}({}) err={}",
                idx + 1,
                repeat_count,
                report.return_code,
                report.return_code_name,
                err
            );
        }

        let out = report
            .output
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing output on success report"))?;

        submitted_total = submitted_total.saturating_add(out.metrics.submitted_ops as u64);
        processed_total = processed_total.saturating_add(out.metrics.processed_ops as u64);
        success_total = success_total.saturating_add(out.metrics.success_ops as u64);
        writes_total = writes_total.saturating_add(out.metrics.total_writes);
        aoem_exec_us_total = aoem_exec_us_total.saturating_add(out.metrics.elapsed_us);

        if repeat_count == 1 {
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
        } else {
            println!(
                "mode=ffi_v2 run={}/{} variant={} dll={} rc={}({}) submitted={} processed={} success={} writes={} elapsed_us={} host_elapsed_us={}",
                idx + 1,
                repeat_count,
                runtime.variant.as_str(),
                runtime.dll_path.display(),
                report.return_code,
                report.return_code_name,
                out.metrics.submitted_ops,
                out.metrics.processed_ops,
                out.metrics.success_ops,
                out.metrics.total_writes,
                out.metrics.elapsed_us,
                host_elapsed_us
            );
        }
    }
    let host_exec_us = exec_loop_sw.elapsed().as_micros() as u64;
    if repeat_count > 1 {
        println!(
            "mode=ffi_v2_aggregate variant={} dll={} rc=0(ok) repeats={} submitted_total={} processed_total={} success_total={} writes_total={} host_exec_us={} aoem_exec_us={}",
            runtime.variant.as_str(),
            runtime.dll_path.display(),
            repeat_count,
            submitted_total,
            processed_total,
            success_total,
            writes_total,
            host_exec_us,
            aoem_exec_us_total
        );
    }
    drop(session);
    std::mem::forget(facade);
    Ok(())
}

enum EitherBatch {
    Ops(Vec<novovm_exec::ExecOpV2>),
    Wire(Vec<u8>),
}
