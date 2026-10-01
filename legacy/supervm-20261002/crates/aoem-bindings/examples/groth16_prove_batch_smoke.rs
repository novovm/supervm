use anyhow::{bail, Context, Result};
use aoem_bindings::AoemDyn;
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

fn encode_blob_list(items: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(items.len() as u32).to_le_bytes());
    for item in items {
        out.extend_from_slice(&(item.len() as u32).to_le_bytes());
        out.extend_from_slice(item);
    }
    out
}

fn decode_blob_list<'a>(input: &'a [u8], label: &str) -> Result<Vec<&'a [u8]>> {
    if input.len() < 4 {
        bail!("{label} wire too short");
    }
    let mut cursor = 0usize;
    let count = u32::from_le_bytes([input[0], input[1], input[2], input[3]]) as usize;
    cursor += 4;
    if count == 0 {
        bail!("{label} wire has zero items");
    }
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if cursor + 4 > input.len() {
            bail!("{label} wire truncated on length prefix");
        }
        let len = u32::from_le_bytes([
            input[cursor],
            input[cursor + 1],
            input[cursor + 2],
            input[cursor + 3],
        ]) as usize;
        cursor += 4;
        if len == 0 {
            bail!("{label} wire contains empty item");
        }
        if cursor + len > input.len() {
            bail!("{label} wire truncated on payload");
        }
        out.push(&input[cursor..cursor + len]);
        cursor += len;
    }
    if cursor != input.len() {
        bail!("{label} wire has trailing bytes");
    }
    Ok(out)
}

fn make_witness(a: u64, b: u64) -> Vec<u8> {
    let c = a.saturating_mul(b);
    let mut witness = Vec::with_capacity(24);
    witness.extend_from_slice(&a.to_le_bytes());
    witness.extend_from_slice(&b.to_le_bytes());
    witness.extend_from_slice(&c.to_le_bytes());
    witness
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

    let out_path = parse_arg(
        &args,
        "--out",
        Some("artifacts/migration/groth16-prove-batch-smoke.json"),
    )?;
    let out_path = PathBuf::from(out_path);

    let dynlib = unsafe { AoemDyn::load(&dll_path)? };
    if !dynlib.supports_groth16_prove_auto_path() {
        bail!("groth16 prove path is not available in loaded AOEM DLL");
    }

    let witnesses = vec![
        make_witness(7, 9),
        make_witness(11, 13),
        make_witness(17, 19),
    ];
    let witness_wire = encode_blob_list(&witnesses);

    let start = std::time::Instant::now();
    let (vk, proofs_wire, inputs_wire) = dynlib.groth16_prove_batch_v1(&witness_wire)?;
    let elapsed_us = start.elapsed().as_micros() as u64;

    let proofs = decode_blob_list(&proofs_wire, "proofs")?;
    let inputs = decode_blob_list(&inputs_wire, "inputs")?;
    let pass = !vk.is_empty()
        && proofs.len() == witnesses.len()
        && inputs.len() == witnesses.len()
        && proofs.iter().all(|p| !p.is_empty())
        && inputs.iter().all(|i| !i.is_empty());

    let summary = json!({
        "schema": "aoem_groth16_prove_batch_smoke_v1",
        "generated_at_unix": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        "dll_path": dll_path.to_string_lossy(),
        "supports_groth16_prove_v1": dynlib.supports_groth16_prove_v1(),
        "supports_groth16_prove_batch_v1": dynlib.supports_groth16_prove_batch_v1(),
        "supports_groth16_prove_auto_path": dynlib.supports_groth16_prove_auto_path(),
        "witness_count": witnesses.len(),
        "vk_len": vk.len(),
        "proof_count": proofs.len(),
        "inputs_count": inputs.len(),
        "elapsed_us": elapsed_us,
        "pass": pass
    });

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create output dir failed: {}", parent.display()))?;
    }
    std::fs::write(&out_path, serde_json::to_vec_pretty(&summary)?)
        .with_context(|| format!("write summary failed: {}", out_path.display()))?;

    println!(
        "aoem groth16 prove batch smoke generated: {}\n  pass={}\n  witness_count={}\n  elapsed_us={}",
        out_path.display(),
        pass,
        witnesses.len(),
        elapsed_us
    );
    if !pass {
        bail!("groth16 prove batch smoke failed");
    }
    Ok(())
}
