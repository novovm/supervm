use anyhow::{anyhow, bail, Context, Result};
use aoem_bindings::{
    acquire_global_lane, recommend_threads_from_aoem, AoemDyn, AoemHostHint, AoemOpV2,
};
use std::path::PathBuf;
use std::thread;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Preset {
    A1Parity,
    BatchStress,
}

impl Preset {
    fn as_cli_name(self) -> &'static str {
        match self {
            Preset::A1Parity => "cpu_parity",
            Preset::BatchStress => "cpu_batch_stress",
        }
    }
}

#[derive(Clone, Debug)]
struct BenchCfg {
    preset: Preset,
    dll_path: PathBuf,
    txs: u64,
    key_space: u64,
    rw: f64,
    submit_ops: u32,
    seed: u64,
    warmup_calls: u32,
    threads: usize,
    engine_workers: usize,
}

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

fn parse_usize_arg(raw: &str, key: &str) -> Result<usize> {
    raw.parse::<usize>()
        .with_context(|| format!("invalid {key} value: {raw}"))
}

fn load_cfg() -> Result<BenchCfg> {
    let args: Vec<String> = std::env::args().collect();
    let preset = match parse_arg(&args, "--preset", Some("cpu_parity"))?
        .to_ascii_lowercase()
        .as_str()
    {
        "cpu_parity" | "a1_parity" => Preset::A1Parity,
        "cpu_batch_stress" | "batch_stress" => Preset::BatchStress,
        v => bail!(
            "invalid --preset: {v} (expected cpu_parity|cpu_batch_stress; legacy aliases: a1_parity|batch_stress)"
        ),
    };

    let txs_default = "1000000";
    let key_space_default = "128";
    let rw_default = "0.5";
    let submit_ops_default = match preset {
        Preset::A1Parity => "1",
        Preset::BatchStress => "1024",
    };
    let warmup_default = "10";

    let txs: u64 = parse_arg(&args, "--txs", Some(txs_default))?.parse()?;
    let key_space: u64 = parse_arg(&args, "--key-space", Some(key_space_default))?.parse()?;
    let rw: f64 = parse_arg(&args, "--rw", Some(rw_default))?.parse()?;
    // Submit granularity of each aoem_execute_ops_v2 call.
    // For compatibility, --batch is accepted as alias of --submit-ops.
    let submit_ops: u32 = if args.iter().any(|v| v == "--submit-ops") {
        parse_arg(&args, "--submit-ops", Some(submit_ops_default))?.parse()?
    } else {
        parse_arg(&args, "--batch", Some(submit_ops_default))?.parse()?
    };
    let seed: u64 = parse_arg(&args, "--seed", Some("123"))?.parse()?;
    let warmup_calls: u32 = parse_arg(&args, "--warmup-calls", Some(warmup_default))?.parse()?;
    let dll_path = PathBuf::from(parse_arg(
        &args,
        "--dll",
        Some("D:\\WorksArea\\SUPERVM\\aoem\\bin\\aoem_ffi.dll"),
    )?);

    if !(0.0..=1.0).contains(&rw) {
        bail!("rw must be in [0,1]");
    }
    if submit_ops == 0 {
        bail!("submit_ops must be > 0");
    }
    let hint = AoemHostHint {
        txs,
        batch: submit_ops,
        key_space,
        rw,
    };
    let threads_raw = if args.iter().any(|v| v == "--threads") {
        parse_arg(&args, "--threads", Some("1"))?
    } else {
        match preset {
            Preset::A1Parity => "1".to_string(),
            Preset::BatchStress => "auto".to_string(),
        }
    };
    let engine_workers_raw = if args.iter().any(|v| v == "--engine-workers") {
        parse_arg(&args, "--engine-workers", Some("auto"))?
    } else {
        match preset {
            Preset::A1Parity => {
                if threads_raw.eq_ignore_ascii_case("auto") {
                    "auto".to_string()
                } else {
                    "16".to_string()
                }
            }
            Preset::BatchStress => "auto".to_string(),
        }
    };
    let threads_auto = threads_raw.eq_ignore_ascii_case("auto");
    let engine_workers_auto = engine_workers_raw.eq_ignore_ascii_case("auto");

    let (threads, engine_workers) = if threads_auto || engine_workers_auto {
        let dyn_probe = unsafe { AoemDyn::load(&dll_path)? };
        let decision = recommend_threads_from_aoem(&dyn_probe, &hint);
        let hw_threads = decision.hw_threads.max(1);
        let worker_budget = decision.budget_threads.max(1);
        let mut threads = if threads_auto {
            decision.recommended_threads.max(1)
        } else {
            parse_usize_arg(&threads_raw, "--threads")?
        };
        let mut engine_workers = if engine_workers_auto {
            // Keep A1 single-engine parity semantics when threads=1; otherwise derive from budget.
            if matches!(preset, Preset::A1Parity) && threads == 1 {
                16usize.min(worker_budget.max(1))
            } else {
                (worker_budget / threads.max(1)).max(1)
            }
        } else {
            parse_usize_arg(&engine_workers_raw, "--engine-workers")?
        };

        // Joint guardrail: avoid over-subscription by default in auto mode.
        if threads.saturating_mul(engine_workers) > worker_budget {
            if threads_auto && !engine_workers_auto {
                threads = (worker_budget / engine_workers.max(1)).max(1);
            } else if !threads_auto && engine_workers_auto {
                engine_workers = (worker_budget / threads.max(1)).max(1);
            } else {
                // both auto: prefer preserving thread selection first
                engine_workers = (worker_budget / threads.max(1)).max(1);
                if threads.saturating_mul(engine_workers) > worker_budget {
                    threads = (worker_budget / engine_workers.max(1)).max(1);
                }
            }
        }

        if threads == 0 || engine_workers == 0 {
            bail!("adaptive parallelism resolved to zero");
        }

        println!(
            "adaptive_parallelism: threads_auto={} engine_workers_auto={} hw_threads={} budget_threads={} selected_threads={} selected_engine_workers={} total_workers={} reason={}",
            threads_auto,
            engine_workers_auto,
            hw_threads,
            worker_budget,
            threads,
            engine_workers,
            threads.saturating_mul(engine_workers),
            decision.reason
        );

        (threads, engine_workers)
    } else {
        let threads = parse_usize_arg(&threads_raw, "--threads")?;
        let engine_workers = parse_usize_arg(&engine_workers_raw, "--engine-workers")?;
        if threads == 0 {
            bail!("threads must be > 0");
        }
        if engine_workers == 0 {
            bail!("engine-workers must be > 0");
        }
        (threads, engine_workers)
    };

    Ok(BenchCfg {
        preset,
        dll_path,
        txs,
        key_space,
        rw,
        submit_ops,
        seed,
        warmup_calls,
        threads,
        engine_workers,
    })
}

fn run_ffi_v2(cfg: &BenchCfg) -> Result<()> {
    if !cfg.dll_path.exists() {
        bail!("dll not found: {}", cfg.dll_path.display());
    }
    let write_threshold_ppm = (cfg.rw * 1_000_000.0) as u64;

    println!(
        "worldline: mode=ffi_v2 preset={} dll={} threads={} engine_workers={} txs={} key_space={} rw={} submit_ops={} seed={} warmup_calls={}",
        cfg.preset.as_cli_name(),
        cfg.dll_path.display(),
        cfg.threads,
        cfg.engine_workers,
        cfg.txs,
        cfg.key_space,
        cfg.rw,
        cfg.submit_ops,
        cfg.seed,
        cfg.warmup_calls
    );
    if cfg.threads == 1 {
        println!("mode: single_engine");
    } else {
        println!("mode: host_parallel (one AOEM handle per worker thread)");
    }

    let per_thread = cfg.txs / cfg.threads as u64;
    let remainder = cfg.txs % cfg.threads as u64;
    let mut handles = Vec::with_capacity(cfg.threads);
    let t0 = Instant::now();

    for tid in 0..cfg.threads {
        let cfg = cfg.clone();
        let mut tx_count = per_thread;
        if (tid as u64) < remainder {
            tx_count += 1;
        }
        handles.push(thread::spawn(move || -> Result<(u64, u64, u64)> {
            let _lane = acquire_global_lane();
            let dynlib = unsafe { AoemDyn::load(&cfg.dll_path)? };
            if dynlib.abi() != 1 {
                bail!("ABI mismatch: {}", dynlib.abi());
            }
            if !dynlib.supports_execute_ops_v2() {
                bail!("loaded AOEM DLL does not expose aoem_execute_ops_v2");
            }
            let handle =
                dynlib.create_handle_with_ingress_workers(Some(cfg.engine_workers as u32))?;

            let tid_mix = (tid as u64 + 1).wrapping_mul(0x9E3779B97F4A7C15);
            let mut rng_state = cfg.seed ^ tid_mix;
            let mut keys: Vec<[u8; 8]> = Vec::new();
            let mut values: Vec<[u8; 8]> = Vec::new();
            let mut ops: Vec<AoemOpV2> = Vec::new();

            for _ in 0..cfg.warmup_calls {
                build_ops_v2(
                    cfg.submit_ops,
                    cfg.key_space,
                    write_threshold_ppm,
                    &mut rng_state,
                    &mut keys,
                    &mut values,
                    &mut ops,
                );
                let out = handle.execute_ops_v2(&ops)?;
                if out.success != out.processed {
                    bail!(
                        "warmup v2 failed: processed={}, success={}, failed_index={}",
                        out.processed,
                        out.success,
                        out.failed_index
                    );
                }
            }

            let mut remaining = tx_count;
            let mut done_ops = 0u64;
            let mut done_plans = 0u64;
            let mut done_calls = 0u64;

            while remaining > 0 {
                let n = remaining.min(cfg.submit_ops as u64) as u32;
                build_ops_v2(
                    n,
                    cfg.key_space,
                    write_threshold_ppm,
                    &mut rng_state,
                    &mut keys,
                    &mut values,
                    &mut ops,
                );
                let out = handle.execute_ops_v2(&ops)?;
                if out.success != out.processed {
                    bail!(
                        "v2 call failed: processed={}, success={}, failed_index={}",
                        out.processed,
                        out.success,
                        out.failed_index
                    );
                }
                done_calls += 1;
                done_ops += out.processed as u64;
                done_plans += 1;
                remaining -= n as u64;
            }

            // Keep AOEM DLL resident for process lifetime.
            // In current AOEM FFI V2 async lane, unloading immediately after handle drop
            // can race with ingress worker teardown and hit STATUS_ACCESS_VIOLATION on Windows.
            drop(handle);
            std::mem::forget(dynlib);

            Ok((done_ops, done_plans, done_calls))
        }));
    }

    let mut done_ops = 0u64;
    let mut done_plans = 0u64;
    let mut done_calls = 0u64;
    for h in handles {
        let (ops, plans, calls) = h.join().map_err(|_| anyhow!("thread panicked"))??;
        done_ops += ops;
        done_plans += plans;
        done_calls += calls;
    }
    let elapsed = t0.elapsed().as_secs_f64();
    print_result(elapsed, done_ops, done_plans, done_calls, "ffi_v2_calls");
    Ok(())
}

fn print_result(
    elapsed_sec: f64,
    done_ops: u64,
    done_plans: u64,
    done_calls: u64,
    call_label: &str,
) {
    let ops_tps = if elapsed_sec > 0.0 {
        done_ops as f64 / elapsed_sec
    } else {
        0.0
    };
    let plans_tps = if elapsed_sec > 0.0 {
        done_plans as f64 / elapsed_sec
    } else {
        0.0
    };
    let calls_tps = if elapsed_sec > 0.0 {
        done_calls as f64 / elapsed_sec
    } else {
        0.0
    };
    println!(
        "result: elapsed_sec={:.3}, done_ops={}, done_plans={}, done_calls={}, tps_unit=ops_per_s, tps={:.2}, plans_per_s={:.2}, {}_per_s={:.2}, avg_ops_per_plan={:.2}, avg_ops_per_call={:.2}",
        elapsed_sec,
        done_ops,
        done_plans,
        done_calls,
        ops_tps,
        plans_tps,
        call_label,
        calls_tps,
        if done_plans > 0 {
            done_ops as f64 / done_plans as f64
        } else {
            0.0
        },
        if done_calls > 0 {
            done_ops as f64 / done_calls as f64
        } else {
            0.0
        },
    );
}

fn main() -> Result<()> {
    let cfg = load_cfg()?;
    run_ffi_v2(&cfg)
}
