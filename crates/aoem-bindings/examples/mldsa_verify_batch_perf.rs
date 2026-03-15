use anyhow::{bail, Context, Result};
use libloading::Library;
use serde_json::json;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

type AoemMldsaSupportedFn = unsafe extern "C" fn() -> u32;
type AoemMldsaPubkeySizeFn = unsafe extern "C" fn(level: u32) -> u32;
type AoemMldsaSignatureSizeFn = unsafe extern "C" fn(level: u32) -> u32;
type AoemMldsaKeygenFn = unsafe extern "C" fn(
    level: u32,
    out_pubkey_ptr: *mut *mut u8,
    out_pubkey_len: *mut usize,
    out_secret_key_ptr: *mut *mut u8,
    out_secret_key_len: *mut usize,
) -> i32;
type AoemMldsaSignFn = unsafe extern "C" fn(
    level: u32,
    secret_key_ptr: *const u8,
    secret_key_len: usize,
    message_ptr: *const u8,
    message_len: usize,
    out_signature_ptr: *mut *mut u8,
    out_signature_len: *mut usize,
) -> i32;
type AoemMldsaVerifyBatchFn = unsafe extern "C" fn(
    items_ptr: *const AoemMldsaVerifyItemV1,
    item_count: usize,
    out_results_ptr: *mut *mut u8,
    out_results_len: *mut usize,
    out_valid_count: *mut u32,
) -> i32;
type AoemFreeFn = unsafe extern "C" fn(ptr: *mut u8, len: usize);

#[repr(C)]
#[derive(Clone, Copy)]
struct AoemMldsaVerifyItemV1 {
    level: u32,
    pubkey_ptr: *const u8,
    pubkey_len: usize,
    message_ptr: *const u8,
    message_len: usize,
    signature_ptr: *const u8,
    signature_len: usize,
}

fn nearest_rank(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len() as f64;
    let rank = (q.clamp(0.0, 1.0) * n).ceil().max(1.0) as usize;
    sorted[rank.saturating_sub(1)]
}

fn parse_arg<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> Result<T> {
    let key = format!("--{name}");
    if let Some(idx) = args.iter().position(|a| a == &key) {
        let raw = args
            .get(idx + 1)
            .with_context(|| format!("missing value for {key}"))?;
        raw.parse::<T>()
            .map_err(|_| anyhow::anyhow!("invalid value for {key}: {raw}"))
    } else {
        Ok(default)
    }
}

fn parse_path_arg(args: &[String], name: &str, default: PathBuf) -> Result<PathBuf> {
    let key = format!("--{name}");
    if let Some(idx) = args.iter().position(|a| a == &key) {
        let raw = args
            .get(idx + 1)
            .with_context(|| format!("missing value for {key}"))?;
        Ok(PathBuf::from(raw))
    } else {
        Ok(default)
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let dll_path = parse_path_arg(
        &args,
        "dll",
        PathBuf::from(r"D:\WEB3_AI\SUPERVM\aoem\windows\core\bin\aoem_ffi.dll"),
    )?;
    let out_path = parse_path_arg(
        &args,
        "out",
        PathBuf::from("artifacts/migration/mldsa-verify-batch-perf-case.json"),
    )?;
    let count: usize = parse_arg(&args, "count", 1_000usize)?;
    let repeats: usize = parse_arg(&args, "repeats", 5usize)?;
    let level: u32 = parse_arg(&args, "level", 87u32)?;
    let message_size: usize = parse_arg(&args, "message-size", 32usize)?;

    if count == 0 {
        bail!("--count must be > 0");
    }
    if repeats == 0 {
        bail!("--repeats must be > 0");
    }
    if message_size == 0 {
        bail!("--message-size must be > 0");
    }

    let par_min = env::var("AOEM_MLDSA_VERIFY_BATCH_PAR_MIN")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);

    unsafe {
        let lib = Library::new(&dll_path)
            .with_context(|| format!("failed to load AOEM FFI dll: {}", dll_path.display()))?;
        let supported: AoemMldsaSupportedFn = *lib
            .get(b"aoem_mldsa_supported\0")
            .context("missing aoem_mldsa_supported")?;
        let pubkey_size_fn: AoemMldsaPubkeySizeFn = *lib
            .get(b"aoem_mldsa_pubkey_size\0")
            .context("missing aoem_mldsa_pubkey_size")?;
        let signature_size_fn: AoemMldsaSignatureSizeFn = *lib
            .get(b"aoem_mldsa_signature_size\0")
            .context("missing aoem_mldsa_signature_size")?;
        let keygen_fn: AoemMldsaKeygenFn = *lib
            .get(b"aoem_mldsa_keygen_v1\0")
            .context("missing aoem_mldsa_keygen_v1")?;
        let sign_fn: AoemMldsaSignFn = *lib
            .get(b"aoem_mldsa_sign_v1\0")
            .context("missing aoem_mldsa_sign_v1")?;
        let verify_batch_fn: AoemMldsaVerifyBatchFn = *lib
            .get(b"aoem_mldsa_verify_batch_v1\0")
            .context("missing aoem_mldsa_verify_batch_v1")?;
        let free_fn: AoemFreeFn = *lib
            .get(b"aoem_free\0")
            .context("missing aoem_free (required by batch outputs)")?;

        if supported() == 0 {
            bail!("aoem_mldsa_supported=0");
        }

        let expected_pubkey = pubkey_size_fn(level) as usize;
        let expected_signature = signature_size_fn(level) as usize;
        if expected_pubkey == 0 || expected_signature == 0 {
            bail!(
                "invalid mldsa level={level} sizes pubkey={} signature={}",
                expected_pubkey,
                expected_signature
            );
        }

        let mut pubkey_ptr: *mut u8 = std::ptr::null_mut();
        let mut pubkey_len: usize = 0;
        let mut secret_key_ptr: *mut u8 = std::ptr::null_mut();
        let mut secret_key_len: usize = 0;
        let keygen_rc = keygen_fn(
            level,
            &mut pubkey_ptr,
            &mut pubkey_len,
            &mut secret_key_ptr,
            &mut secret_key_len,
        );
        if keygen_rc != 0 {
            bail!("aoem_mldsa_keygen_v1 failed rc={keygen_rc}");
        }
        if pubkey_ptr.is_null() || secret_key_ptr.is_null() {
            bail!("aoem_mldsa_keygen_v1 returned null pointers");
        }
        let pubkey = std::slice::from_raw_parts(pubkey_ptr, pubkey_len).to_vec();
        let secret_key = std::slice::from_raw_parts(secret_key_ptr, secret_key_len).to_vec();
        free_fn(pubkey_ptr, pubkey_len);
        free_fn(secret_key_ptr, secret_key_len);

        if pubkey.len() != expected_pubkey {
            bail!(
                "pubkey length mismatch expected={} got={}",
                expected_pubkey,
                pubkey.len()
            );
        }

        let mut message = vec![0u8; message_size];
        for (i, b) in message.iter_mut().enumerate() {
            *b = ((i * 131 + 17) & 0xFF) as u8;
        }

        let mut signature_ptr: *mut u8 = std::ptr::null_mut();
        let mut signature_len: usize = 0;
        let sign_rc = sign_fn(
            level,
            secret_key.as_ptr(),
            secret_key.len(),
            message.as_ptr(),
            message.len(),
            &mut signature_ptr,
            &mut signature_len,
        );
        if sign_rc != 0 {
            bail!("aoem_mldsa_sign_v1 failed rc={sign_rc}");
        }
        if signature_ptr.is_null() {
            bail!("aoem_mldsa_sign_v1 returned null signature pointer");
        }
        let signature = std::slice::from_raw_parts(signature_ptr, signature_len).to_vec();
        free_fn(signature_ptr, signature_len);

        if signature.len() != expected_signature {
            bail!(
                "signature length mismatch expected={} got={}",
                expected_signature,
                signature.len()
            );
        }

        let item = AoemMldsaVerifyItemV1 {
            level,
            pubkey_ptr: pubkey.as_ptr(),
            pubkey_len: pubkey.len(),
            message_ptr: message.as_ptr(),
            message_len: message.len(),
            signature_ptr: signature.as_ptr(),
            signature_len: signature.len(),
        };
        let items = vec![item; count];
        let mut samples_ms = Vec::with_capacity(repeats);
        let mut samples_tps = Vec::with_capacity(repeats);

        for _ in 0..repeats {
            let mut out_results_ptr: *mut u8 = std::ptr::null_mut();
            let mut out_results_len: usize = 0;
            let mut out_valid_count: u32 = 0;
            let start = Instant::now();
            let rc = verify_batch_fn(
                items.as_ptr(),
                items.len(),
                &mut out_results_ptr,
                &mut out_results_len,
                &mut out_valid_count,
            );
            let elapsed = start.elapsed();
            if rc != 0 {
                bail!("aoem_mldsa_verify_batch_v1 failed rc={rc}");
            }
            if out_results_ptr.is_null() {
                bail!("aoem_mldsa_verify_batch_v1 returned null out_results_ptr");
            }
            if out_results_len != count {
                bail!(
                    "batch results length mismatch expected={} got={}",
                    count,
                    out_results_len
                );
            }
            if out_valid_count as usize != count {
                bail!(
                    "batch valid_count mismatch expected={} got={}",
                    count,
                    out_valid_count
                );
            }
            let results = std::slice::from_raw_parts(out_results_ptr, out_results_len);
            if results.iter().any(|v| *v != 1u8) {
                bail!("batch verify contains invalid entries");
            }
            free_fn(out_results_ptr, out_results_len);

            let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
            let tps = if elapsed_ms <= 0.0 {
                0.0
            } else {
                (count as f64) / (elapsed_ms / 1000.0)
            };
            samples_ms.push(elapsed_ms);
            samples_tps.push(tps);
        }

        let result = json!({
            "dll_path": dll_path.display().to_string(),
            "count": count,
            "repeats": repeats,
            "level": level,
            "message_size": message_size,
            "aoem_mldsa_verify_batch_par_min": par_min,
            "samples_ms": samples_ms,
            "samples_tps": samples_tps,
            "p50_tps": nearest_rank(&samples_tps, 0.50),
            "p90_tps": nearest_rank(&samples_tps, 0.90),
            "p99_tps": nearest_rank(&samples_tps, 0.99),
            "p50_ms": nearest_rank(&samples_ms, 0.50),
            "p90_ms": nearest_rank(&samples_ms, 0.90),
            "p99_ms": nearest_rank(&samples_ms, 0.99),
        });

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create output dir {}", parent.display()))?;
        }
        fs::write(
            &out_path,
            serde_json::to_string_pretty(&result).context("serialize result json failed")?,
        )
        .with_context(|| format!("failed to write {}", out_path.display()))?;
        println!("{}", out_path.display());
    }

    Ok(())
}
