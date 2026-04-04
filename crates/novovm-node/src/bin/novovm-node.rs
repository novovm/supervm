// Copyright (c) 2026 Xonovo Technology
// All rights reserved.
// Author: Xonovo Technology

#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_exec::{AoemRuntimeConfig, OpsWireOp, OpsWireV1Builder};
use novovm_node::tx_ingress::{
    available_ingress_codecs, load_exec_batch_from_wire_file, load_ops_wire_v1_file,
    load_ops_wire_v1_from_tx_wire_file, load_ops_wire_v1_payload_file,
    LOCAL_TX_WIRE_CODEC_WRITE_U64LE_V1,
};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_millis() as u64)
        .unwrap_or(0)
}

fn json_escape_minimal(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

fn build_l1_anchor_id(seq: u64, ts_unix_ms: u64, ok_ops: u64, failed_files: u64) -> String {
    format!(
        "l1a{:016x}{:016x}{:016x}{:016x}",
        seq, ts_unix_ms, ok_ops, failed_files
    )
}

struct L1L4AnchorRecord {
    anchor_id: String,
    plan_id: u64,
    json_bytes: Vec<u8>,
}

fn build_l1l4_anchor_record(
    node_id: &str,
    overlay_node_id: &str,
    overlay_session_id: &str,
    seq: u64,
    l4_ingress_ops: u64,
    l3_routed_batches: u64,
    l2_exec_ok_ops: u64,
    l2_exec_failed_files: u64,
) -> L1L4AnchorRecord {
    let ts_unix_ms = now_unix_ms();
    let anchor_id = build_l1_anchor_id(seq, ts_unix_ms, l2_exec_ok_ops, l2_exec_failed_files);
    let node_id_escaped = json_escape_minimal(node_id);
    let overlay_node_id_escaped = json_escape_minimal(overlay_node_id);
    let overlay_session_id_escaped = json_escape_minimal(overlay_session_id);
    let json = format!(
        "{{\"version\":1,\"anchor_id\":\"{}\",\"seq\":{},\"node_id\":\"{}\",\"overlay_node_id\":\"{}\",\"overlay_session_id\":\"{}\",\"ts_unix_ms\":{},\"l4_ingress_ops\":{},\"l3_routed_batches\":{},\"l2_exec_ok_ops\":{},\"l2_exec_failed_files\":{}}}\n",
        anchor_id,
        seq,
        node_id_escaped,
        overlay_node_id_escaped,
        overlay_session_id_escaped,
        ts_unix_ms,
        l4_ingress_ops,
        l3_routed_batches,
        l2_exec_ok_ops,
        l2_exec_failed_files
    );
    let plan_id = ((ts_unix_ms & 0xffff_ffff) << 32) | (seq & 0xffff_ffff);
    L1L4AnchorRecord {
        anchor_id,
        plan_id,
        json_bytes: json.into_bytes(),
    }
}

fn append_l1l4_anchor_record(anchor_path: &Path, record: &L1L4AnchorRecord) -> Result<()> {
    if let Some(parent) = anchor_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create L1 anchor dir failed: {}", parent.display()))?;
        }
    }
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(anchor_path)
        .with_context(|| format!("open L1 anchor file failed: {}", anchor_path.display()))?;
    f.write_all(&record.json_bytes)
        .with_context(|| format!("write L1 anchor file failed: {}", anchor_path.display()))?;
    f.flush()
        .with_context(|| format!("flush L1 anchor file failed: {}", anchor_path.display()))?;
    Ok(())
}

fn encode_l1l4_anchor_ledger_wire(record: &L1L4AnchorRecord, key_prefix: &[u8]) -> Result<Vec<u8>> {
    let mut key = Vec::with_capacity(key_prefix.len() + record.anchor_id.len());
    key.extend_from_slice(key_prefix);
    key.extend_from_slice(record.anchor_id.as_bytes());
    let mut builder = OpsWireV1Builder::new();
    builder.push(OpsWireOp {
        opcode: 2,
        flags: 0,
        reserved: 0,
        key: &key,
        value: &record.json_bytes,
        delta: 0,
        expect_version: None,
        plan_id: record.plan_id,
    })?;
    Ok(builder.finish().bytes)
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

fn u64_env_allow_zero(name: &str, default: u64) -> Result<u64> {
    let raw = std::env::var(name).unwrap_or_else(|_| default.to_string());
    raw.trim()
        .parse::<u64>()
        .with_context(|| format!("invalid {name}={raw}"))
}

fn usize_env_allow_zero(name: &str, default: usize) -> Result<usize> {
    let raw = std::env::var(name).unwrap_or_else(|_| default.to_string());
    raw.trim()
        .parse::<usize>()
        .with_context(|| format!("invalid {name}={raw}"))
}

fn ensure_ops_wire_dir(dir: &Path) -> Result<()> {
    if !dir.exists() {
        bail!("NOVOVM_OPS_WIRE_DIR does not exist: {}", dir.display());
    }
    if !dir.is_dir() {
        bail!("NOVOVM_OPS_WIRE_DIR is not a directory: {}", dir.display());
    }
    Ok(())
}

fn list_ops_wire_files(dir: &Path) -> Result<Vec<PathBuf>> {
    ensure_ops_wire_dir(dir)?;
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

fn list_ops_wire_files_for_watch(dir: &Path, max_files: usize) -> Result<Vec<PathBuf>> {
    ensure_ops_wire_dir(dir)?;
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
            if files.len() >= max_files {
                break;
            }
        }
    }
    Ok(files)
}

fn move_file_to_dir(src: &Path, dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(dir)
        .with_context(|| format!("create directory failed: {}", dir.display()))?;
    let base_name = src
        .file_name()
        .map(|v| v.to_string_lossy().to_string())
        .ok_or_else(|| anyhow::anyhow!("invalid source filename: {}", src.display()))?;
    let mut candidate = dir.join(&base_name);
    if candidate.exists() {
        let mut seq: u64 = 1;
        loop {
            let name = format!("{base_name}.{seq}");
            candidate = dir.join(name);
            if !candidate.exists() {
                break;
            }
            seq = seq.saturating_add(1);
            if seq > 1_000_000 {
                bail!(
                    "unable to allocate unique destination path for {} in {}",
                    src.display(),
                    dir.display()
                );
            }
        }
    }
    match fs::rename(src, &candidate) {
        Ok(_) => {}
        Err(_) => {
            fs::copy(src, &candidate).with_context(|| {
                format!(
                    "copy fallback failed: {} -> {}",
                    src.display(),
                    candidate.display()
                )
            })?;
            fs::remove_file(src)
                .with_context(|| format!("remove source after copy failed: {}", src.display()))?;
        }
    }
    Ok(candidate)
}

fn quarantine_failed_file(path: &Path, failed_dir: Option<&Path>) -> Result<()> {
    if let Some(dir) = failed_dir {
        move_file_to_dir(path, dir)?;
        return Ok(());
    }
    let file_name = path
        .file_name()
        .map(|v| v.to_string_lossy().to_string())
        .ok_or_else(|| anyhow::anyhow!("invalid failed filename: {}", path.display()))?;
    let failed = path.with_file_name(format!("{file_name}.failed"));
    if failed.exists() {
        fs::remove_file(&failed)
            .with_context(|| format!("remove stale failed file failed: {}", failed.display()))?;
    }
    match fs::rename(path, &failed) {
        Ok(_) => {}
        Err(_) => {
            fs::copy(path, &failed).with_context(|| {
                format!(
                    "copy fallback for failed file failed: {} -> {}",
                    path.display(),
                    failed.display()
                )
            })?;
            fs::remove_file(path).with_context(|| {
                format!(
                    "remove source after failed copy fallback failed: {}",
                    path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn finalize_success_file(path: &Path, done_dir: Option<&Path>) -> Result<()> {
    if let Some(dir) = done_dir {
        move_file_to_dir(path, dir)?;
    } else {
        fs::remove_file(path).with_context(|| {
            format!("remove processed ops wire file failed: {}", path.display())
        })?;
    }
    Ok(())
}

fn drop_failed_file(path: &Path) -> Result<()> {
    fs::remove_file(path)
        .with_context(|| format!("drop failed ops wire file failed: {}", path.display()))
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
    let watch_mode = bool_env("NOVOVM_OPS_WIRE_WATCH");
    let l1l4_anchor_file_path = string_env_nonempty("NOVOVM_L1L4_ANCHOR_PATH").map(PathBuf::from);
    let l1l4_anchor_ledger_enabled = bool_env("NOVOVM_L1L4_ANCHOR_LEDGER_ENABLED");
    let l1l4_anchor_ledger_key_prefix = string_env_nonempty("NOVOVM_L1L4_ANCHOR_LEDGER_KEY_PREFIX")
        .unwrap_or_else(|| "ledger:l1:l1l4_anchor:v1:".to_string())
        .into_bytes();
    let l1l4_anchor_any_sink = l1l4_anchor_file_path.is_some() || l1l4_anchor_ledger_enabled;
    let node_id = string_env_nonempty("NOVOVM_NODE_ID")
        .or_else(|| string_env_nonempty("HOSTNAME"))
        .or_else(|| string_env_nonempty("COMPUTERNAME"))
        .unwrap_or_else(|| "local".to_string());
    let overlay_node_id =
        string_env_nonempty("NOVOVM_OVERLAY_NODE_ID").unwrap_or_else(|| node_id.clone());
    let overlay_session_id = string_env_nonempty("NOVOVM_OVERLAY_SESSION_ID")
        .unwrap_or_else(|| format!("sess-{}-{}", now_unix_ms(), std::process::id()));
    let mut anchor_seq: u64 = 0;
    if watch_mode {
        if !use_wire_v1 {
            bail!("NOVOVM_OPS_WIRE_WATCH requires ops_wire_v1 ingress path");
        }
        if tx_wire_path.is_some() || ops_wire_path.is_some() || ops_wire_dir.is_none() {
            bail!(
                "NOVOVM_OPS_WIRE_WATCH requires NOVOVM_OPS_WIRE_DIR and forbids NOVOVM_TX_WIRE_FILE/NOVOVM_OPS_WIRE_FILE"
            );
        }
        if repeat_count != 1 {
            bail!("NOVOVM_OPS_WIRE_WATCH requires NOVOVM_TX_REPEAT_COUNT=1");
        }
        let watch_dir = ops_wire_dir
            .as_ref()
            .expect("ops wire dir must exist for watch mode");
        let poll_ms = u64_env_allow_zero("NOVOVM_OPS_WIRE_WATCH_POLL_MS", 200)?;
        if poll_ms == 0 {
            bail!("NOVOVM_OPS_WIRE_WATCH_POLL_MS must be >= 1");
        }
        let watch_batch_max_files =
            usize_env_allow_zero("NOVOVM_OPS_WIRE_WATCH_BATCH_MAX_FILES", 1024)?;
        if watch_batch_max_files == 0 {
            bail!("NOVOVM_OPS_WIRE_WATCH_BATCH_MAX_FILES must be >= 1");
        }
        let idle_exit_seconds = u64_env_allow_zero("NOVOVM_OPS_WIRE_WATCH_IDLE_EXIT_SECONDS", 0)?;
        let done_dir = string_env_nonempty("NOVOVM_OPS_WIRE_WATCH_DONE_DIR").map(PathBuf::from);
        let failed_dir = string_env_nonempty("NOVOVM_OPS_WIRE_WATCH_FAILED_DIR").map(PathBuf::from);
        let drop_failed = bool_env("NOVOVM_OPS_WIRE_WATCH_DROP_FAILED");
        if verbose {
            let done = done_dir
                .as_ref()
                .map(|v| v.display().to_string())
                .unwrap_or_else(|| "-".to_string());
            let failed = failed_dir
                .as_ref()
                .map(|v| v.display().to_string())
                .unwrap_or_else(|| "-".to_string());
            println!(
                "ops_wire_watch_in: dir={} poll_ms={} idle_exit_seconds={} batch_max_files={} done_dir={} failed_dir={} drop_failed={}",
                watch_dir.display(),
                poll_ms,
                idle_exit_seconds,
                watch_batch_max_files,
                done,
                failed,
                drop_failed
            );
        }

        let mut last_active = Instant::now();
        let mut ok_files: u64 = 0;
        let mut failed_files: u64 = 0;
        let mut ok_ops: u64 = 0;

        loop {
            let files = list_ops_wire_files_for_watch(watch_dir, watch_batch_max_files)?;
            if files.is_empty() {
                if idle_exit_seconds > 0 && last_active.elapsed().as_secs() >= idle_exit_seconds {
                    break;
                }
                std::thread::sleep(Duration::from_millis(poll_ms));
                continue;
            }
            last_active = Instant::now();
            let mut cycle_seen_files: u64 = 0;
            let mut cycle_ok_files: u64 = 0;
            let mut cycle_failed_files: u64 = 0;
            let mut cycle_ingress_ops: u64 = 0;
            let mut cycle_ok_ops: u64 = 0;
            for path in files {
                cycle_seen_files = cycle_seen_files.saturating_add(1);
                let payload = match load_ops_wire_v1_file(&path) {
                    Ok(v) => v,
                    Err(e) => {
                        cycle_failed_files = cycle_failed_files.saturating_add(1);
                        if drop_failed {
                            drop_failed_file(&path)?;
                        } else {
                            quarantine_failed_file(&path, failed_dir.as_deref())?;
                        }
                        if verbose {
                            println!(
                                "ops_wire_watch_decode_reject: file={} err={}",
                                path.display(),
                                e
                            );
                        }
                        continue;
                    }
                };
                cycle_ingress_ops = cycle_ingress_ops.saturating_add(payload.op_count as u64);

                let report = session.submit_ops_wire_report(&payload.bytes);
                if !report.ok {
                    cycle_failed_files = cycle_failed_files.saturating_add(1);
                    let err = report
                        .error
                        .as_ref()
                        .map(|v| v.message.as_str())
                        .unwrap_or("unknown");
                    if drop_failed {
                        drop_failed_file(&path)?;
                    } else {
                        quarantine_failed_file(&path, failed_dir.as_deref())?;
                    }
                    if verbose {
                        println!(
                            "ops_wire_watch_exec_reject: file={} rc={}({}) err={}",
                            path.display(),
                            report.return_code,
                            report.return_code_name,
                            err
                        );
                    }
                    continue;
                }

                finalize_success_file(&path, done_dir.as_deref())?;
                cycle_ok_files = cycle_ok_files.saturating_add(1);
                cycle_ok_ops = cycle_ok_ops.saturating_add(payload.op_count as u64);
            }
            ok_files = ok_files.saturating_add(cycle_ok_files);
            failed_files = failed_files.saturating_add(cycle_failed_files);
            ok_ops = ok_ops.saturating_add(cycle_ok_ops);
            if cycle_seen_files > 0 && l1l4_anchor_any_sink {
                anchor_seq = anchor_seq.saturating_add(1);
                let anchor_record = build_l1l4_anchor_record(
                    &node_id,
                    &overlay_node_id,
                    &overlay_session_id,
                    anchor_seq,
                    cycle_ingress_ops,
                    cycle_seen_files,
                    cycle_ok_ops,
                    cycle_failed_files,
                );
                if let Some(anchor_path) = l1l4_anchor_file_path.as_ref() {
                    append_l1l4_anchor_record(anchor_path, &anchor_record)?;
                }
                if l1l4_anchor_ledger_enabled {
                    let wire = encode_l1l4_anchor_ledger_wire(
                        &anchor_record,
                        &l1l4_anchor_ledger_key_prefix,
                    )?;
                    let report = session.submit_ops_wire_report(&wire);
                    if !report.ok {
                        let err = report
                            .error
                            .as_ref()
                            .map(|v| v.message.as_str())
                            .unwrap_or("unknown");
                        bail!(
                            "persist l1l4 anchor to ledger failed: anchor_id={} rc={}({}) err={}",
                            anchor_record.anchor_id,
                            report.return_code,
                            report.return_code_name,
                            err
                        );
                    }
                }
            }
        }

        if verbose {
            println!(
                "ops_wire_watch_out: ok_files={} failed_files={} ok_ops={} dir={}",
                ok_files,
                failed_files,
                ok_ops,
                watch_dir.display()
            );
        }
        drop(session);
        std::mem::forget(facade);
        return Ok(());
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

            submitted_total = submitted_total.saturating_add(out.metrics.submitted_ops as u64);
            processed_total = processed_total.saturating_add(out.metrics.processed_ops as u64);
            success_total = success_total.saturating_add(out.metrics.success_ops as u64);
            writes_total = writes_total.saturating_add(out.metrics.total_writes);
            aoem_exec_us_total = aoem_exec_us_total.saturating_add(out.metrics.elapsed_us);
        }
    }
    if l1l4_anchor_any_sink {
        anchor_seq = anchor_seq.saturating_add(1);
        let anchor_record = build_l1l4_anchor_record(
            &node_id,
            &overlay_node_id,
            &overlay_session_id,
            anchor_seq,
            submitted_total,
            prepared_batches.len() as u64,
            success_total,
            0,
        );
        if let Some(anchor_path) = l1l4_anchor_file_path.as_ref() {
            append_l1l4_anchor_record(anchor_path, &anchor_record)?;
        }
        if l1l4_anchor_ledger_enabled {
            let wire =
                encode_l1l4_anchor_ledger_wire(&anchor_record, &l1l4_anchor_ledger_key_prefix)?;
            let report = session.submit_ops_wire_report(&wire);
            if !report.ok {
                let err = report
                    .error
                    .as_ref()
                    .map(|v| v.message.as_str())
                    .unwrap_or("unknown");
                bail!(
                    "persist l1l4 anchor to ledger failed: anchor_id={} rc={}({}) err={}",
                    anchor_record.anchor_id,
                    report.return_code,
                    report.return_code_name,
                    err
                );
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
