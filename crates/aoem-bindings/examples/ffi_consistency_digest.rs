use anyhow::{bail, Context, Result};
use aoem_bindings::{AoemDyn, AoemOpV2};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

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

fn next_u64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    x.wrapping_mul(0x2545F4914F6CDD1D)
}

fn build_ops_v2(
    op_count: u32,
    key_space: u64,
    write_threshold_ppm: u64,
    rng_state: &mut u64,
    keys: &mut Vec<[u8; 8]>,
    values: &mut Vec<[u8; 8]>,
    ops: &mut Vec<AoemOpV2>,
) {
    keys.clear();
    values.clear();
    ops.clear();

    let n = op_count as usize;
    keys.resize(n, [0u8; 8]);
    values.resize(n, [0u8; 8]);
    ops.reserve(n);

    let effective_key_space = key_space.max(2);
    let write_space = (effective_key_space / 2).max(1);
    let read_space = effective_key_space - write_space;

    for i in 0..n {
        let is_write = (next_u64(rng_state) % 1_000_000) < write_threshold_ppm;
        if is_write {
            let key_id = next_u64(rng_state) % write_space;
            keys[i] = key_id.to_le_bytes();
            values[i] = next_u64(rng_state).to_le_bytes();
            ops.push(AoemOpV2 {
                opcode: 2,
                flags: 0,
                reserved: 0,
                key_ptr: keys[i].as_ptr(),
                key_len: keys[i].len() as u32,
                value_ptr: values[i].as_ptr(),
                value_len: values[i].len() as u32,
                delta: 0,
                expect_version: u64::MAX,
                plan_id: 0,
            });
        } else {
            let key_id = write_space + (next_u64(rng_state) % read_space);
            keys[i] = key_id.to_le_bytes();
            ops.push(AoemOpV2 {
                opcode: 1,
                flags: 0,
                reserved: 0,
                key_ptr: keys[i].as_ptr(),
                key_len: keys[i].len() as u32,
                value_ptr: std::ptr::null(),
                value_len: 0,
                delta: 0,
                expect_version: u64::MAX,
                plan_id: 0,
            });
        }
    }
}

fn to_hex_lower(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(data.len() * 2);
    for b in data {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
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
    let rounds: usize = parse_arg(&args, "--rounds", Some("200"))?.parse()?;
    let points: u32 = parse_arg(&args, "--points", Some("1024"))?.parse()?;
    let key_space: u64 = parse_arg(&args, "--key-space", Some("251"))?.parse()?;
    let rw: f64 = parse_arg(&args, "--rw", Some("0.5"))?.parse()?;
    let seed: u64 = parse_arg(&args, "--seed", Some("123"))?.parse()?;

    if !(0.0..=1.0).contains(&rw) {
        bail!("rw must be in [0,1]");
    }
    if rounds == 0 {
        bail!("rounds must be > 0");
    }
    if points == 0 {
        bail!("points must be > 0");
    }

    let dll_path = PathBuf::from(&dll);
    if !dll_path.exists() {
        bail!(
            "dll not found: {} (pass --dll or set NOVOVM_AOEM_DLL/AOEM_FFI_DLL)",
            dll_path.display()
        );
    }

    let dynlib = unsafe { AoemDyn::load(&dll_path)? };
    if dynlib.abi() != 1 {
        bail!("ABI mismatch: {}", dynlib.abi());
    }
    if !dynlib.supports_execute_ops_v2() {
        bail!("aoem_execute_ops_v2 not found in loaded DLL");
    }
    let handle = dynlib.create_handle()?;

    let write_threshold_ppm = (rw * 1_000_000.0) as u64;
    let mut rng_state = seed;
    let mut keys: Vec<[u8; 8]> = Vec::new();
    let mut values: Vec<[u8; 8]> = Vec::new();
    let mut ops: Vec<AoemOpV2> = Vec::new();

    let mut digest = Sha256::new();
    let mut total_processed = 0u64;
    let mut total_success = 0u64;
    let mut total_writes = 0u64;

    for _ in 0..rounds {
        build_ops_v2(
            points,
            key_space,
            write_threshold_ppm,
            &mut rng_state,
            &mut keys,
            &mut values,
            &mut ops,
        );
        let out = handle.execute_ops_v2(&ops)?;
        if out.success != out.processed {
            bail!(
                "consistency run failed: processed={}, success={}, failed_index={}",
                out.processed,
                out.success,
                out.failed_index
            );
        }
        digest.update(out.processed.to_le_bytes());
        digest.update(out.success.to_le_bytes());
        digest.update(out.failed_index.to_le_bytes());
        digest.update(out.total_writes.to_le_bytes());

        total_processed += out.processed as u64;
        total_success += out.success as u64;
        total_writes += out.total_writes;
    }

    let digest_hex = to_hex_lower(&digest.finalize());
    println!(
        "consistency: rounds={} points={} key_space={} rw={} seed={} digest={} total_processed={} total_success={} total_writes={}",
        rounds, points, key_space, rw, seed, digest_hex, total_processed, total_success, total_writes
    );

    Ok(())
}
