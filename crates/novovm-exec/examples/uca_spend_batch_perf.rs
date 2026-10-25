//! Diagnostic component A/B on identical, independently signed NUAS intents.
//! Not chain TPS: no network, state authorization, execution or publication.
use anyhow::{ensure, Result};
use novovm_exec::{pq_signature::AoemMldsaParameterSet, AoemExecFacade, AoemExecOpenOptions};
use novovm_protocol::uca_spend::{spend_key_ref_v1, SignedUcaSpendV1, UcaSpendIntentV1};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::time::Instant;

fn main() -> Result<()> {
    let path = aoem_bindings::default_host_dll_path().canonicalize()?;
    let facade = AoemExecFacade::open(&path, AoemExecOpenOptions::default())?;
    let capabilities = facade.capabilities_json()?;
    let environment = [
        "AOEM_GPU_AUTO_THRESHOLD_ENFORCE",
        "AOEM_MLDSA_GPU_NTT_PREPASS",
        "AOEM_MLDSA_VERIFY_BATCH_PAR_MIN",
        "RAYON_NUM_THREADS",
    ]
    .into_iter()
    .map(|name| (name, std::env::var(name).ok()))
    .collect::<std::collections::BTreeMap<_, _>>();
    let keys = facade.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let signer = facade.uca_spend_signer_v1()?;
    let pairs = (0..64)
        .map(|_| keys.generate_keypair())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let rows = (0..1024)
        .map(|i| {
            let key = &pairs[i % pairs.len()];
            signer.sign(
                UcaSpendIntentV1 {
                    chain_id: 991_026,
                    genesis: [91; 32],
                    account_id: format!("uca:perf-payer-{}", i % pairs.len()),
                    authorization_epoch: 1,
                    spend_key_ref: spend_key_ref_v1(&key.public_key)?,
                    recipient_uca: "uca:perf-recipient".into(),
                    amount: 1,
                    max_fee: 50,
                    nonce: (i / pairs.len()) as u64,
                    not_before_height: 1,
                    expires_at_height: 100,
                },
                &key.public_key,
                &key.expanded_secret_key,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let mut transcript = Sha256::new();
    for row in &rows {
        transcript.update(row.encode()?);
    }
    let started = Instant::now();
    let verifier = facade.uca_spend_verifier_v1()?;
    let qualification_us = started.elapsed().as_secs_f64() * 1e6;
    // Qualification/warm-up and invalid-row localization are outside samples.
    let warm = verifier.verify_batch(rows[..4].to_vec())?;
    ensure!(warm.iter().all(Result::is_ok), "warm-up signature failure");
    let mut mixed = rows[..4].to_vec();
    mixed[1].signature[0] ^= 1;
    mixed[3].intent.amount += 1;
    let rejected = verifier.verify_batch(mixed)?;
    ensure!(
        rejected.iter().map(Result::is_ok).collect::<Vec<_>>() == [true, false, true, false],
        "batch must preserve original invalid-row positions"
    );
    let mut cases = Vec::new();
    for size in [1, 8, 32, 64, 72, 128, 1024] {
        let mut scalar_us = Vec::new();
        let mut batch_us = Vec::new();
        // Alternating order reduces (but does not eliminate) warm/thermal bias.
        for repetition in 0..11 {
            for batch in [repetition % 2 == 0, repetition % 2 != 0] {
                let inputs: Vec<Vec<SignedUcaSpendV1>> =
                    rows.chunks(size).map(<[_]>::to_vec).collect();
                let start = Instant::now();
                for input in inputs {
                    if batch {
                        for result in verifier.verify_batch(input)? {
                            result?;
                        }
                    } else {
                        for row in input {
                            verifier.verify(row)?;
                        }
                    }
                }
                let elapsed = start.elapsed().as_secs_f64() * 1e6;
                if batch {
                    batch_us.push(elapsed);
                } else {
                    scalar_us.push(elapsed);
                }
            }
        }
        cases.push(json!({"batch_size":size,"verified_rows_per_sample":rows.len(),
            "scalar_us":scalar_us,"batch_us":batch_us,
            "note":"last chunk may be shorter; input cloning and signature generation excluded; canonical framing/result validation included"}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "scope":"NUAS signature verification component only; NOT transaction TPS",
        "environment":environment,"capabilities":capabilities,
            "library":path,"library_sha256":format!("{:x}",Sha256::digest(std::fs::read(&path)?)),
            "parameter_set":"ML-DSA-65","signature_context":"original SPEND_CONTEXT_V1",
            "independent_intents":rows.len(),"independent_public_keys":pairs.len(),
            "signed_input_sha256":format!("{:x}",transcript.finalize()),
            "same_inputs_for_scalar_and_batch":true,"qualification_us":qualification_us,
            "invalid_row_localization_pass":true,"backend":"existing AOEM FFI; not a GPU execution claim",
            "cases":cases
        }))?
    );
    Ok(())
}
