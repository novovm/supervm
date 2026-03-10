use anyhow::{bail, Context, Result};
use aoem_bindings::{default_runtime_profile_path_for_dll, global_parallel_budget, AoemDyn};
use serde_json::json;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn parse_arg(args: &[String], key: &str, default: Option<&str>) -> Result<String> {
    if let Some(idx) = args.iter().position(|v| v == key) {
        let val = args
            .get(idx + 1)
            .with_context(|| format!("missing value for {key}"))?;
        return Ok(val.clone());
    }
    if let Some(v) = default {
        return Ok(v.to_string());
    }
    bail!("missing required arg {key}");
}

fn hardware_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
}

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
    let args: Vec<String> = std::env::args().collect();
    let dll_default = resolve_default_dll_arg();
    let dll = parse_arg(&args, "--dll", dll_default.as_deref())?;
    let dll_path = PathBuf::from(&dll);
    if !dll_path.exists() {
        bail!(
            "dll not found: {} (pass --dll or set NOVOVM_AOEM_DLL/AOEM_FFI_DLL)",
            dll_path.display()
        );
    }

    let out_path = {
        let out = parse_arg(&args, "--out", None).ok();
        out.map(PathBuf::from)
            .unwrap_or_else(|| default_runtime_profile_path_for_dll(&dll_path))
    };

    let txs: u64 = parse_arg(&args, "--txs", Some("1000000"))?.parse()?;
    let batch: u32 = parse_arg(&args, "--batch", Some("1024"))?.parse()?;
    let key_space: u64 = parse_arg(&args, "--key-space", Some("1000000"))?.parse()?;
    let rw: f64 = parse_arg(&args, "--rw", Some("0.5"))?.parse()?;
    if !(0.0..=1.0).contains(&rw) {
        bail!("rw must be in [0,1]");
    }

    let dynlib = unsafe { AoemDyn::load(&dll_path)? };
    let hw = hardware_threads();
    let budget = global_parallel_budget().min(hw).max(1);
    let recommended = dynlib
        .recommend_parallelism(txs, batch, key_space, rw)
        .unwrap_or(budget)
        .min(budget)
        .max(1);

    let small_txs = recommended.clamp(1, 4);
    let small_batch = (recommended / 2).max(1);
    let high_contention = (recommended * 3 / 4).max(1);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let capabilities = dynlib
        .capabilities()
        .unwrap_or_else(|_| json!({"error": "capabilities_unavailable"}));
    let ffi_symbol_contract = json!({
        "execute_ops_v2": dynlib.supports_execute_ops_v2(),
        "execute_ops_wire_v1": dynlib.supports_execute_ops_wire_v1(),
        "zkvm_probe": dynlib.supports_zkvm_probe(),
        "ring_signature_verify_web30_v1": dynlib.supports_ring_signature_verify(),
        "ring_signature_verify_batch_web30_v1": dynlib.supports_ring_signature_verify_batch_web30_v1(),
        "bulletproof_batch_v1": dynlib.supports_bulletproof_batch_v1(),
        "ringct_batch_v1": dynlib.supports_ringct_batch_v1(),
        "privacy_batch_v1_all": dynlib.supports_privacy_batch_v1()
    });
    let profile = json!({
        "schema": "aoem-runtime-profile/v1",
        "generated_at_unix": now,
        "dll_path": dll_path.to_string_lossy(),
        "abi": dynlib.abi(),
        "version": dynlib.version(),
        "probe_hint": {
            "txs": txs,
            "batch": batch,
            "key_space": key_space,
            "rw": rw
        },
        "host": {
            "hw_threads": hw,
            "budget_threads": budget
        },
        "recommended_threads": recommended,
        "threads": {
            "default": recommended
        },
        "profiles": [
            {
                "name": "small_txs",
                "max_txs": 100000,
                "threads": small_txs
            },
            {
                "name": "small_batch",
                "max_batch": 256,
                "threads": small_batch
            },
            {
                "name": "high_contention",
                "max_key_space": 256,
                "min_rw": 0.5,
                "threads": high_contention
            },
            {
                "name": "throughput_default",
                "threads": recommended
            }
        ],
        "ffi_symbol_contract": ffi_symbol_contract,
        "capabilities": capabilities
    });

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create profile dir failed: {}", parent.display()))?;
    }
    std::fs::write(&out_path, serde_json::to_vec_pretty(&profile)?)
        .with_context(|| format!("write profile failed: {}", out_path.display()))?;

    println!(
        "aoem install profile generated: {}\n  recommended_threads={}\n  hw_threads={} budget_threads={}",
        out_path.display(),
        recommended,
        hw,
        budget
    );
    Ok(())
}
