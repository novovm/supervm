use anyhow::{bail, Context, Result};
use aoem_bindings::{AoemDyn, AoemOpV2};
use std::path::PathBuf;
use std::time::Instant;

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

fn percentile(sorted: &[u128], q: f64) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx]
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

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let dll = parse_arg(
        &args,
        "--dll",
        Some("D:\\WorksArea\\SUPERVM\\aoem\\bin\\aoem_ffi.dll"),
    )?;
    let warmup: usize = parse_arg(&args, "--warmup", Some("20"))?.parse()?;
    let iters: usize = parse_arg(&args, "--iters", Some("200"))?.parse()?;
    let points: u32 = parse_arg(&args, "--points", Some("1100"))?.parse()?;
    let key_space: u64 = parse_arg(&args, "--key-space", Some("251"))?.parse()?;
    let rw: f64 = parse_arg(&args, "--rw", Some("0.5"))?.parse()?;
    let seed: u64 = parse_arg(&args, "--seed", Some("123"))?.parse()?;

    if !(0.0..=1.0).contains(&rw) {
        bail!("rw must be in [0,1]");
    }

    let dll_path = PathBuf::from(&dll);
    if !dll_path.exists() {
        bail!("dll not found: {}", dll_path.display());
    }

    let dynlib = unsafe { AoemDyn::load(&dll_path)? };
    if dynlib.abi() != 1 {
        bail!("ABI mismatch: {}", dynlib.abi());
    }
    if !dynlib.supports_execute_ops_v2() {
        bail!("aoem_execute_ops_v2 not found in loaded DLL");
    }
    let caps = dynlib.capabilities()?;
    let version = dynlib.version();
    println!("dll={}", dll_path.display());
    println!("version={}", version);
    println!("capabilities={}", caps);

    let handle = dynlib.create_handle()?;
    let mut rng_state = seed;
    let write_threshold_ppm = (rw * 1_000_000.0) as u64;
    let mut keys: Vec<[u8; 8]> = Vec::new();
    let mut values: Vec<[u8; 8]> = Vec::new();
    let mut ops: Vec<AoemOpV2> = Vec::new();

    for _ in 0..warmup {
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
                "warmup failed: processed={}, success={}, failed_index={}",
                out.processed,
                out.success,
                out.failed_index
            );
        }
    }

    let mut us: Vec<u128> = Vec::with_capacity(iters);
    for _ in 0..iters {
        build_ops_v2(
            points,
            key_space,
            write_threshold_ppm,
            &mut rng_state,
            &mut keys,
            &mut values,
            &mut ops,
        );
        let t0 = Instant::now();
        let out = handle.execute_ops_v2(&ops)?;
        let dt = t0.elapsed().as_micros();
        if out.success != out.processed {
            bail!(
                "run failed: processed={}, success={}, failed_index={}",
                out.processed,
                out.success,
                out.failed_index
            );
        }
        us.push(dt);
    }

    us.sort_unstable();
    let p50 = percentile(&us, 0.50);
    let p95 = percentile(&us, 0.95);
    let mean = if us.is_empty() {
        0.0
    } else {
        us.iter().sum::<u128>() as f64 / us.len() as f64
    };
    let ops_tps_p50 = if p50 == 0 {
        0.0
    } else {
        (1_000_000.0 / p50 as f64) * points as f64
    };
    let ops_tps_mean = if mean <= 0.0 {
        0.0
    } else {
        (1_000_000.0 / mean) * points as f64
    };
    println!(
        "perf: mode=ffi_v2 warmup={}, iters={}, points={}, p50_us={}, p95_us={}, mean_us={:.2}, tps_unit=ops_per_s, tps_p50={:.2}, tps_mean={:.2}",
        warmup, iters, points, p50, p95, mean, ops_tps_p50, ops_tps_mean
    );

    Ok(())
}
