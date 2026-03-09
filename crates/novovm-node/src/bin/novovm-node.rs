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
use std::fs;
use std::path::{Path, PathBuf};
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

fn list_ops_wire_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        bail!("NOVOVM_OPS_WIRE_DIR does not exist: {}", dir.display());
    }
    if !dir.is_dir() {
        bail!("NOVOVM_OPS_WIRE_DIR is not a directory: {}", dir.display());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)
        .with_context(|| format!("read NOVOVM_OPS_WIRE_DIR failed: {}", dir.display()))?
    {
        let entry = entry
            .with_context(|| format!("iterate NOVOVM_OPS_WIRE_DIR failed: {}", dir.display()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|v| v.to_str()) else {
            continue;
        };
        if ext.eq_ignore_ascii_case("opsw1") {
            files.push(path);
        }
    }
    files.sort_by(|a, b| {
        let a_name = a
            .file_name()
            .map(|v| v.to_string_lossy().to_string())
            .unwrap_or_default();
        let b_name = b
            .file_name()
            .map(|v| v.to_string_lossy().to_string())
            .unwrap_or_default();
        a_name.cmp(&b_name)
    });
    Ok(files)
}

struct PreparedBatch {
    tx_count: usize,
    batch: EitherBatch,
    source_detail: String,
}

fn main() -> Result<()> {
    let verbose = bool_env("NOVOVM_NODE_VERBOSE");
    let node_mode = std::env::var("NOVOVM_NODE_MODE").unwrap_or_else(|_| "full".to_string());
    if !node_mode.eq_ignore_ascii_case("full") {
        bail!("non-full node_mode is disabled: novovm-node keeps only production path");
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
    if verbose {
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
    }

    let tx_wire_path = string_env_nonempty("NOVOVM_TX_WIRE_FILE").map(PathBuf::from);
    let ops_wire_path = string_env_nonempty("NOVOVM_OPS_WIRE_FILE").map(PathBuf::from);
    let ops_wire_dir = string_env_nonempty("NOVOVM_OPS_WIRE_DIR").map(PathBuf::from);
    let source_count =
        tx_wire_path.iter().count() + ops_wire_path.iter().count() + ops_wire_dir.iter().count();
    if source_count != 1 {
        bail!(
            "ingress source conflict: set exactly one of NOVOVM_TX_WIRE_FILE | NOVOVM_OPS_WIRE_FILE | NOVOVM_OPS_WIRE_DIR"
        );
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
    let mut prepared_batches = Vec::new();
    let (input_source, d1_codec, aoem_ingress_path) = if use_wire_v1 {
        if selected_codec.is_some() && (ops_wire_path.is_some() || ops_wire_dir.is_some()) {
            bail!("NOVOVM_D1_CODEC is only allowed with NOVOVM_TX_WIRE_FILE (raw tx payload mode)");
        }
        let source = if let Some(path) = ops_wire_path.as_ref() {
            let payload = load_ops_wire_v1_file(path)?;
            prepared_batches.push(PreparedBatch {
                tx_count: payload.op_count,
                batch: EitherBatch::Wire(payload.bytes),
                source_detail: path.display().to_string(),
            });
            "ops_wire_v1".to_string()
        } else if let Some(dir) = ops_wire_dir.as_ref() {
            let files = list_ops_wire_files(dir)?;
            if files.is_empty() {
                bail!("NOVOVM_OPS_WIRE_DIR has no .opsw1 files: {}", dir.display());
            }
            for path in files {
                let payload = load_ops_wire_v1_file(&path)?;
                prepared_batches.push(PreparedBatch {
                    tx_count: payload.op_count,
                    batch: EitherBatch::Wire(payload.bytes),
                    source_detail: path.display().to_string(),
                });
            }
            "ops_wire_dir".to_string()
        } else {
            let path = tx_wire_path.as_ref().expect("tx wire path must exist");
            let codec = selected_codec
                .clone()
                .unwrap_or_else(|| LOCAL_TX_WIRE_CODEC_WRITE_U64LE_V1.to_string());
            let payload = if selected_codec.is_some() {
                load_ops_wire_v1_payload_file(path, &codec).with_context(|| {
                    format!(
                        "encode ingress payload with codec={codec} failed; available={:?}",
                        available_ingress_codecs()
                    )
                })?
            } else {
                load_ops_wire_v1_from_tx_wire_file(path)?
            };
            prepared_batches.push(PreparedBatch {
                tx_count: payload.op_count,
                batch: EitherBatch::Wire(payload.bytes),
                source_detail: path.display().to_string(),
            });
            "tx_wire".to_string()
        };
        let codec = if tx_wire_path.is_some() {
            selected_codec
                .clone()
                .unwrap_or_else(|| LOCAL_TX_WIRE_CODEC_WRITE_U64LE_V1.to_string())
        } else {
            "-".to_string()
        };
        if verbose {
            println!(
                "d1_ingress_mode: selected=ops_wire_v1 requested={:?} auto_supported={} codec={}",
                ingress_mode, supports_wire_v1, codec
            );
        }
        (source, codec, "ops_wire_v1".to_string())
    } else {
        if ops_wire_path.is_some() || ops_wire_dir.is_some() {
            bail!(
                "NOVOVM_OPS_WIRE_FILE/NOVOVM_OPS_WIRE_DIR require ops_wire_v1 path; current selected mode=ops_v2"
            );
        }
        if selected_codec.is_some() {
            bail!("NOVOVM_D1_CODEC requires ops_wire_v1 ingress; current selected mode=ops_v2");
        }
        let ingress_path = tx_wire_path.as_ref().expect("tx wire path must exist");
        let payload = load_exec_batch_from_wire_file(ingress_path, |_, rec| {
            (rec.account << 32) | rec.nonce.saturating_add(1)
        })?;
        prepared_batches.push(PreparedBatch {
            tx_count: payload.len(),
            batch: EitherBatch::Ops(payload.ops),
            source_detail: ingress_path.display().to_string(),
        });
        let path_mode = if ingress_mode == D1IngressMode::Auto && !supports_wire_v1 {
            "ops_v2_fallback".to_string()
        } else {
            "ops_v2_forced".to_string()
        };
        if verbose {
            println!(
                "d1_ingress_mode: selected=ops_v2 requested={:?} auto_supported={}",
                ingress_mode, supports_wire_v1
            );
        }
        ("tx_wire".to_string(), "-".to_string(), path_mode)
    };
    if verbose {
        let total_txs: usize = prepared_batches.iter().map(|b| b.tx_count).sum();
        println!(
            "d1_ingress_contract: mode={} source={} codec={} aoem_ingress_path={} batches={}",
            if use_wire_v1 { "ops_wire_v1" } else { "ops_v2" },
            input_source,
            d1_codec,
            aoem_ingress_path,
            prepared_batches.len()
        );
        println!(
            "tx_ingress_source: mode={} batches={} txs={} host_admission=false",
            input_source,
            prepared_batches.len(),
            total_txs
        );
    }
    let repeat_count = repeat_count_env()?;
    if ops_wire_dir.is_some() && repeat_count != 1 {
        bail!("NOVOVM_TX_REPEAT_COUNT must be 1 when NOVOVM_OPS_WIRE_DIR is used");
    }

    let exec_loop_sw = Instant::now();
    let mut submitted_total: u64 = 0;
    let mut processed_total: u64 = 0;
    let mut success_total: u64 = 0;
    let mut writes_total: u64 = 0;
    let mut aoem_exec_us_total: u64 = 0;

    for (batch_idx, prepared) in prepared_batches.iter().enumerate() {
        for idx in 0..repeat_count {
            let report = match &prepared.batch {
                EitherBatch::Ops(ops) => session.submit_ops_report(ops),
                EitherBatch::Wire(wire) => session.submit_ops_wire_report(wire),
            };
            if !report.ok {
                let err = report
                    .error
                    .as_ref()
                    .map(|e| e.message.as_str())
                    .unwrap_or("unknown");
                bail!(
                    "mode=ffi_v2 batch={}/{} run={}/{} source={} rc={}({}) err={}",
                    batch_idx + 1,
                    prepared_batches.len(),
                    idx + 1,
                    repeat_count,
                    prepared.source_detail,
                    report.return_code,
                    report.return_code_name,
                    err
                );
            }

            let out = report
                .output
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("missing output on success report"))?;

            if verbose {
                submitted_total = submitted_total.saturating_add(out.metrics.submitted_ops as u64);
                processed_total = processed_total.saturating_add(out.metrics.processed_ops as u64);
                success_total = success_total.saturating_add(out.metrics.success_ops as u64);
                writes_total = writes_total.saturating_add(out.metrics.total_writes);
                aoem_exec_us_total = aoem_exec_us_total.saturating_add(out.metrics.elapsed_us);
            }
        }
    }
    if verbose {
        let host_exec_us = exec_loop_sw.elapsed().as_micros() as u64;
        println!(
            "mode=ffi_v2_aggregate variant={} dll={} rc=0(ok) batches={} repeats={} submitted_total={} processed_total={} success_total={} writes_total={} host_exec_us={} aoem_exec_us={}",
            runtime.variant.as_str(),
            runtime.dll_path.display(),
            prepared_batches.len(),
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
