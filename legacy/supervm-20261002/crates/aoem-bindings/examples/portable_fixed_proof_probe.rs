//! Diagnostic only: fixed a*b=c proof portability, NOT NOV execution validity.
//! The controller pins the test verification key independently of the bundle.
//! Run from the repository root, with an existing parent and a fresh output dir:
//! cargo run -p aoem-bindings --example portable_fixed_proof_probe -- run
//!   aoem/windows/core/bin/aoem_ffi.dll artifacts/audit/portable-proof-new-run
//! Never use this runtime-generated test VK as a production trust anchor.
//! No NOV balances, nonce, signatures or state transitions are constrained here.
use anyhow::{bail, Context, Result};
use aoem_bindings::AoemDyn;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path, process::Command};

fn bytes(value: &Value, key: &str) -> Result<Vec<u8>> {
    Ok(serde_json::from_value(value[key].clone())?)
}

fn fingerprint(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn single(wire: Vec<u8>) -> Result<Vec<u8>> {
    if wire.len() < 8 || wire[..4] != 1u32.to_le_bytes() {
        bail!("expected one proof/public-input item");
    }
    let len = u32::from_le_bytes(wire[4..8].try_into()?) as usize;
    if len == 0 || wire.len() != len + 8 {
        bail!("bad single-item wire");
    }
    Ok(wire[8..].to_vec())
}

fn write_new(path: &Path, value: &Value) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    Ok(())
}

fn child(mode: &str, library: &str, bundle: &str, pin: Option<&str>) -> Result<Value> {
    let mut command = Command::new(std::env::current_exe()?);
    command.args([mode, library, bundle]);
    if let Some(pin) = pin {
        command.arg(pin);
    }
    let output = command.output()?;
    if !output.status.success() {
        bail!(
            "{mode} child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let text = String::from_utf8(output.stdout)?;
    let line = text
        .lines()
        .find_map(|line| line.strip_prefix("PROOF_PROBE_JSON="))
        .context("missing child diagnostic JSON")?;
    Ok(serde_json::from_str(line)?)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        bail!("usage: portable_fixed_proof_probe run <library> <new-output-directory>");
    }
    let result = match args[0].as_str() {
        "run" if args.len() == 3 => {
            let directory = Path::new(&args[2]);
            fs::create_dir(directory)
                .context("output directory must not already exist; parent must exist")?;
            let bundle = directory.join("fixed-circuit-proof.json");
            let bundle_arg = bundle.to_str().context("output path must be Unicode")?;
            // Generation process terminates before a fresh verification process starts.
            let generation = child("generate", &args[1], bundle_arg, None)?;
            let pin = generation["vk_sha256"]
                .as_str()
                .context("missing test VK pin")?;
            let verification = child("verify", &args[1], bundle_arg, Some(pin))?;
            let report = json!({
                "schema": "aoem-portable-fixed-proof-probe/v1",
                "scope": "fixed_mul_a_times_b_equals_c_only",
                "generation": generation, "verification": verification,
                "producer_exited_before_verifier_started": true,
                "nov_state_transition_proven": false,
                "proof_sealed": false, "finalized": false,
                "vk_trust": "controller_pinned_test_setup_not_production_authority",
                "library_sha256": fingerprint(&fs::read(&args[1])?),
            });
            write_new(&directory.join("report.json"), &report)?;
            report
        }
        "generate" if args.len() == 3 => {
            let library = unsafe { AoemDyn::load(Path::new(&args[1]))? };
            let witness: Vec<u8> = [7u64, 9, 63]
                .into_iter()
                .flat_map(u64::to_le_bytes)
                .collect();
            let mut wire = Vec::from(1u32.to_le_bytes());
            wire.extend_from_slice(&(witness.len() as u32).to_le_bytes());
            wire.extend_from_slice(&witness);
            let (vk, proofs, inputs) = library.groth16_prove_batch_v1(&wire)?;
            let proof = single(proofs)?;
            let public_inputs = single(inputs)?;
            let report = json!({"pid": std::process::id(), "vk_sha256": fingerprint(&vk),
                "proof_bytes": proof.len(), "public_input_bytes": public_inputs.len()});
            // Do not export the private witness to the verifier process.
            write_new(
                Path::new(&args[2]),
                &json!({"vk": vk, "proof": proof, "public_inputs": public_inputs}),
            )?;
            report
        }
        "verify" if args.len() == 4 => {
            if fs::metadata(&args[2])?.len() > 1024 * 1024 {
                bail!("test bundle too large");
            }
            let data = fs::read(&args[2])?;
            let bundle: Value = serde_json::from_slice(&data)?;
            let vk = bytes(&bundle, "vk")?;
            if fingerprint(&vk) != args[3] {
                bail!("test VK pin mismatch");
            }
            let proof = bytes(&bundle, "proof")?;
            let inputs = bytes(&bundle, "public_inputs")?;
            // Fixed BLS12-381 scalar c=63 in the documented uncompressed FR_VEC wire.
            let mut expected = Vec::from(1u32.to_le_bytes());
            expected.extend_from_slice(&63u64.to_le_bytes());
            expected.resize(36, 0);
            if inputs != expected {
                bail!("unexpected fixed-circuit public statement");
            }
            let library = unsafe { AoemDyn::load(Path::new(&args[1]))? };
            if !library.groth16_verify_v1(&vk, &proof, &inputs)? {
                bail!("valid proof rejected");
            }
            let mut wrong_inputs = inputs.clone();
            wrong_inputs[4] = 64;
            // Well-formed but different public result must return false, not a decode error.
            if library.groth16_verify_v1(&vk, &proof, &wrong_inputs)? {
                bail!("wrong output accepted");
            }
            let mut wrong_proof = proof.clone();
            let first = wrong_proof.first_mut().context("empty proof")?;
            *first ^= 1;
            let tamper_result = library.groth16_verify_v1(&vk, &wrong_proof, &inputs);
            let tamper_kind = match tamper_result {
                Ok(true) => bail!("tampered proof accepted"),
                Ok(false) => "verification_false",
                Err(_) => "decode_or_verification_error",
            };
            json!({"pid": std::process::id(), "valid_proof_accepted": true,
                "wrong_public_output_rejected": true, "tampered_proof_rejected": true,
                "tamper_rejection_kind": tamper_kind, "witness_received": false})
        }
        _ => bail!("invalid diagnostic arguments"),
    };
    println!("PROOF_PROBE_JSON={}", serde_json::to_string(&result)?);
    Ok(())
}
