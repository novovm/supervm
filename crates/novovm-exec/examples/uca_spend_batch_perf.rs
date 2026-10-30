//! Diagnostic component timing on identical, independently signed NUAS intents.
//! Not chain TPS: no network, state authorization, execution or publication.
//!
//! With no arguments this process creates one public signed-input corpus, then
//! launches one child process per batch size. That isolation is intentional:
//! AOEM resolves several diagnostic settings through process-local `OnceLock`s,
//! and a previous batch must not warm the next batch's first-call observation.
use anyhow::{bail, ensure, Context, Result};
use novovm_exec::{pq_signature::AoemMldsaParameterSet, AoemExecFacade, AoemExecOpenOptions};
use novovm_protocol::uca_spend::{spend_key_ref_v1, AssetSpendIntentV2, SignedAssetSpendV2};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    hint::black_box,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

const BATCH_SIZES: [usize; 8] = [1, 8, 16, 32, 64, 71, 128, 1024];
const DEFAULT_STEADY_SAMPLES: usize = 31;
const SAMPLE_ENV: &str = "NOVOVM_UCA_BATCH_PERF_SAMPLES";

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [] => run_matrix(),
        [flag, size, dataset] if flag == "--batch-size" => {
            run_one(size.parse()?, Path::new(dataset))
        }
        _ => {
            bail!("usage: uca_spend_batch_perf [--batch-size <1|8|16|32|64|71|128|1024> <dataset>]")
        }
    }
}

fn run_matrix() -> Result<()> {
    let rows = signed_rows(*BATCH_SIZES.last().expect("nonempty batch sizes"))?;
    let transcript = signed_input_sha256(&rows)?;
    let dataset = DatasetFile::write(&rows)?;
    let executable = std::env::current_exe()?;
    let mut cases = Vec::with_capacity(BATCH_SIZES.len());
    for size in BATCH_SIZES {
        let output = Command::new(&executable)
            .args([
                "--batch-size",
                &size.to_string(),
                dataset.path().to_str().context("non-UTF-8 dataset path")?,
            ])
            .output()
            .with_context(|| format!("launch isolated batch-size {size} diagnostic"))?;
        ensure!(
            output.status.success(),
            "batch-size {size} diagnostic failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        cases.push(
            serde_json::from_slice::<Value>(&output.stdout)
                .with_context(|| format!("parse batch-size {size} diagnostic output"))?,
        );
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema":"novovm.nuasmldsa.batch-attribution.v2",
            "scope":"NUAS signature verification component only; NOT transaction TPS",
            "process_isolation":"one child process per batch size; AOEM OnceLock settings inherited from the parent environment",
            "batch_sizes":BATCH_SIZES,
            "shared_signed_rows":rows.len(),
            "shared_signed_input_sha256":transcript,
            "same_inputs_prefix_for_every_size":true,
            "cases":cases
        }))?
    );
    Ok(())
}

fn run_one(size: usize, dataset: &Path) -> Result<()> {
    ensure!(
        BATCH_SIZES.contains(&size),
        "unsupported diagnostic batch size"
    );
    let rows = read_rows(dataset)?;
    ensure!(rows.len() >= size, "diagnostic dataset is too short");
    let sample_rows = &rows[..size];
    let samples = steady_sample_count()?;
    let environment = [
        "AOEM_GPU_AUTO_THRESHOLD_ENFORCE",
        "AOEM_MLDSA_GPU_NTT_PREPASS",
        "AOEM_MLDSA_GPU_NTT_PREPASS_MIN_ITEMS",
        "AOEM_MLDSA_VERIFY_BATCH_PAR_MIN",
        "RAYON_NUM_THREADS",
    ]
    .into_iter()
    .map(|name| (name, std::env::var(name).ok()))
    .collect::<std::collections::BTreeMap<_, _>>();

    let path = aoem_bindings::default_host_dll_path().canonicalize()?;
    let started = Instant::now();
    let facade = AoemExecFacade::open(&path, AoemExecOpenOptions::default())?;
    let library_open_us = micros(started);

    let started = Instant::now();
    let verifier = facade.asset_spend_verifier_v2()?;
    let qualification_us = micros(started);

    // This is the first production-shaped verification after the verifier's
    // required compatibility qualification. No warm-up call precedes it.
    let first = measure_batch(&verifier, sample_rows)?;
    let mut steady = Vec::with_capacity(samples);
    for _ in 0..samples {
        steady.push(measure_batch(&verifier, sample_rows)?);
    }

    // Preserve the original invalid-row localization assertion, but keep it
    // after all timed observations so it cannot warm the first sample.
    let mut mixed = rows[..4].to_vec();
    mixed[1].signature[0] ^= 1;
    mixed[3].intent.amount += 1;
    let rejected = verifier.verify_batch(mixed)?;
    ensure!(
        rejected.iter().map(Result::is_ok).collect::<Vec<_>>() == [true, false, true, false],
        "batch must preserve original invalid-row positions"
    );

    let capabilities = facade.capabilities_json()?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "process_id":std::process::id(),
            "batch_size":size,
            "verified_rows_per_sample":size,
            "ffi_batch_calls_per_sample":1,
            "steady_sample_count":samples,
            "library":path,
            "library_sha256":format!("{:x}",Sha256::digest(fs::read(&path)?)),
            "environment":environment,
            "capabilities":capabilities,
            "parameter_set":"ML-DSA-65",
            "signature_context":"original SPEND_CONTEXT_V2",
            "signed_input_prefix_sha256":signed_input_sha256(sample_rows)?,
            "library_open_us":library_open_us,
            "qualification_us":qualification_us,
            "first_after_qualification_us":sample_json(&first),
            "steady_us":summarize(&steady),
            "invalid_row_localization_pass":true,
            "input_generation_and_signing_excluded":true,
            "phase_boundary":{
                "host_input_clone_us":"clone the exact signed rows before the public verifier call",
                "verify_call_us":"validation, canonical signing bytes, FFI framing/call, AOEM prepass plus definitive verification, and ordered result reconstruction",
                "host_result_consume_us":"consume and assert every returned row after the verifier call",
                "total_us":"the three measured phases above; exactly one batch call"
            },
            "limits":[
                "AOEM exposes no per-stage timing through this API; GPU prepass and definitive verification cannot be split directly here",
                "first means the first verification after process-local qualification, not process startup or library loading",
                "steady observations may include AOEM allocator/cache effects and are intentionally reported separately"
            ],
            "backend":"existing AOEM FFI; prepass configuration is not a GPU definitive-verification claim"
        }))?
    );
    Ok(())
}

#[derive(Clone, Copy)]
struct Sample {
    host_input_clone_us: f64,
    verify_call_us: f64,
    host_result_consume_us: f64,
    total_us: f64,
}

fn measure_batch(
    verifier: &novovm_exec::uca_spend::AssetSpendVerifierV2,
    rows: &[SignedAssetSpendV2],
) -> Result<Sample> {
    let total_started = Instant::now();
    let started = Instant::now();
    let input = rows.to_vec();
    let host_input_clone_us = micros(started);

    let started = Instant::now();
    let outcomes = verifier.verify_batch(input)?;
    let verify_call_us = micros(started);

    let started = Instant::now();
    let mut checked = 0usize;
    for outcome in outcomes {
        checked = checked.saturating_add(outcome?.signed().signature.len());
    }
    black_box(checked);
    let host_result_consume_us = micros(started);
    let total_us = micros(total_started);
    Ok(Sample {
        host_input_clone_us,
        verify_call_us,
        host_result_consume_us,
        total_us,
    })
}

fn summarize(samples: &[Sample]) -> Value {
    json!({
        "raw":samples.iter().map(sample_json).collect::<Vec<_>>(),
        "host_input_clone_us":percentiles(samples.iter().map(|sample| sample.host_input_clone_us)),
        "verify_call_us":percentiles(samples.iter().map(|sample| sample.verify_call_us)),
        "host_result_consume_us":percentiles(samples.iter().map(|sample| sample.host_result_consume_us)),
        "total_us":percentiles(samples.iter().map(|sample| sample.total_us)),
    })
}

fn sample_json(sample: &Sample) -> Value {
    json!({
        "host_input_clone_us":sample.host_input_clone_us,
        "verify_call_us":sample.verify_call_us,
        "host_result_consume_us":sample.host_result_consume_us,
        "total_us":sample.total_us,
    })
}

fn percentiles(values: impl Iterator<Item = f64>) -> Value {
    let mut values = values.collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    json!({
        "p50":nearest_rank(&values, 50),
        "p95":nearest_rank(&values, 95),
        "p99":nearest_rank(&values, 99),
    })
}

fn nearest_rank(values: &[f64], percentile: usize) -> f64 {
    let rank = (percentile * values.len()).div_ceil(100);
    values[rank.saturating_sub(1)]
}

fn micros(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1e6
}

fn steady_sample_count() -> Result<usize> {
    let samples = match std::env::var(SAMPLE_ENV) {
        Ok(value) => value
            .parse::<usize>()
            .with_context(|| format!("parse {SAMPLE_ENV}"))?,
        Err(std::env::VarError::NotPresent) => DEFAULT_STEADY_SAMPLES,
        Err(error) => return Err(error.into()),
    };
    ensure!(
        (3..=10_000).contains(&samples),
        "steady sample count must be 3..=10000"
    );
    Ok(samples)
}

fn signed_rows(count: usize) -> Result<Vec<SignedAssetSpendV2>> {
    let path = aoem_bindings::default_host_dll_path().canonicalize()?;
    let facade = AoemExecFacade::open(&path, AoemExecOpenOptions::default())?;
    let keys = facade.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let signer = facade.asset_spend_signer_v2()?;
    let pairs = (0..64)
        .map(|_| keys.generate_keypair())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    (0..count)
        .map(|i| {
            let key = &pairs[i % pairs.len()];
            signer.sign(
                AssetSpendIntentV2 {
                    chain_id: 991_026,
                    genesis: [91; 32],
                    payer: ((i % pairs.len()) as u128).to_be_bytes().repeat(2),
                    authorization_epoch: 1,
                    spend_key_ref: spend_key_ref_v1(&key.public_key)?,
                    recipient: vec![0xff; 32],
                    asset: novovm_protocol::uca_spend::NATIVE_NOV_ASSET,
                    amount: 1,
                    max_fee: 50,
                    nonce: (i / pairs.len()) as u64,
                    not_before_height: 1,
                    expires_at_height: 100,
                    authority_update: None,
                    recipient_creation: None,
                },
                &key.public_key,
                &key.expanded_secret_key,
            )
        })
        .collect()
}

fn signed_input_sha256(rows: &[SignedAssetSpendV2]) -> Result<String> {
    let mut transcript = Sha256::new();
    for row in rows {
        transcript.update(row.encode()?);
    }
    Ok(format!("{:x}", transcript.finalize()))
}

struct DatasetFile(PathBuf);

impl DatasetFile {
    fn write(rows: &[SignedAssetSpendV2]) -> Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "novovm-nuas-mldsa-batch-{}-{}.bin",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(rows.len() as u32).to_be_bytes());
        for row in rows {
            let encoded = row.encode()?;
            bytes.extend_from_slice(&(encoded.len() as u32).to_be_bytes());
            bytes.extend_from_slice(&encoded);
        }
        fs::write(&path, bytes)?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for DatasetFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn read_rows(path: &Path) -> Result<Vec<SignedAssetSpendV2>> {
    let bytes = fs::read(path)?;
    let mut cursor = 0usize;
    let count = read_u32(&bytes, &mut cursor)? as usize;
    ensure!(count <= 1024, "diagnostic dataset exceeds row limit");
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        let len = read_u32(&bytes, &mut cursor)? as usize;
        let end = cursor.checked_add(len).context("dataset length overflow")?;
        ensure!(end <= bytes.len(), "truncated diagnostic dataset");
        rows.push(SignedAssetSpendV2::decode(&bytes[cursor..end])?);
        cursor = end;
    }
    ensure!(cursor == bytes.len(), "trailing diagnostic dataset bytes");
    Ok(rows)
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32> {
    let end = cursor.checked_add(4).context("dataset offset overflow")?;
    let raw: [u8; 4] = bytes
        .get(*cursor..end)
        .context("truncated diagnostic dataset header")?
        .try_into()?;
    *cursor = end;
    Ok(u32::from_be_bytes(raw))
}
