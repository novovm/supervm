//! Isolated A/B/C comparison on the SAME signed NUAS corpus and loaded DLL.
//! A is a diagnostic-only direct-ABI reference, never a production fallback.
//! B/C use the original UCA verifier with explicit unified CPU/GPU policies.
//! No trusted-state authorization, ledger, network, finality or transaction TPS.
use anyhow::{ensure, Context, Result};
use aoem_bindings::{AoemDyn, AoemMldsaVerifyItemRef};
use novovm_exec::{
    pq_signature::{AoemMldsaParameterSet, MldsaVerificationBackendV1},
    uca_spend::AssetSpendVerifierV3,
    AoemExecFacade, AoemExecOpenOptions,
};
use novovm_protocol::uca_spend::{
    spend_key_ref_v1, AssetSpendIntentV3, SignedAssetSpendV3, SPEND_CONTEXT_V3,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, hint::black_box, time::Instant};

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        args.len() <= 2,
        "usage: uca_semantic_entry_compare [rows=71] [samples=31]"
    );
    let count = args.first().map(|v| v.parse()).transpose()?.unwrap_or(71);
    let samples = args.get(1).map(|v| v.parse()).transpose()?.unwrap_or(31);
    ensure!((1..=1024).contains(&count), "rows must be 1..=1024");
    ensure!((3..=1000).contains(&samples), "samples must be 3..=1000");
    // The legacy export has no execution receipt. Require its explicit CPU
    // policy BEFORE any DLL call, rather than infer a backend from timing.
    ensure!(
        std::env::var("AOEM_MLDSA_VERIFY_BACKEND").as_deref() == Ok("cpu"),
        "set AOEM_MLDSA_VERIFY_BACKEND=cpu before launching this isolated reference"
    );
    let path = aoem_bindings::default_host_dll_path().canonicalize()?;
    let library_sha256 = format!("{:x}", Sha256::digest(fs::read(&path)?));
    let facade = AoemExecFacade::open(&path, AoemExecOpenOptions::default())?;
    // SAFETY: same trusted, explicitly selected ABI-compatible library already
    // opened by the facade; the loaded owner outlives all reference invocations.
    let direct = unsafe { AoemDyn::load(&path)? };
    let cpu = facade.asset_spend_verifier_with_backend_v3(MldsaVerificationBackendV1::Cpu)?;
    let gpu =
        facade.asset_spend_verifier_with_backend_v3(MldsaVerificationBackendV1::GpuRequired)?;
    let (rows, wrong_context) = signed_rows(&facade, count.max(5))?;
    let rows_for_timing = &rows[..count];
    let mut corpus_hash = Sha256::new();
    for row in rows_for_timing {
        let bytes = row.encode()?;
        corpus_hash.update((bytes.len() as u64).to_le_bytes());
        corpus_hash.update(bytes);
    }
    let corpus_sha256 = format!("{:x}", corpus_hash.finalize());
    let mut first = Vec::new();
    let mut timings = [Vec::new(), Vec::new(), Vec::new()];
    for group in 0..3 {
        first.push(measure(group, &direct, &cpu, &gpu, rows_for_timing)?);
    }
    for round in 0..samples {
        // Reverse order every other round to expose, not erase, temporal bias.
        let order = if round % 2 == 0 { [0, 1, 2] } else { [2, 1, 0] };
        for group in order {
            timings[group].push(measure(group, &direct, &cpu, &gpu, rows_for_timing)?);
        }
    }
    let cpu_execution = execution_json(&cpu)?;
    let gpu_execution = execution_json(&gpu)?;
    // Keep invalid signatures, wrong context and local malformed rows out of
    // the timing corpus; they must preserve ORIGINAL output row positions.
    let mut mixed = rows[..5].to_vec();
    mixed[1].signature[0] ^= 1;
    mixed[2].intent.amount += 1;
    mixed[3] = wrong_context;
    mixed[4].public_key.pop();
    let expected = vec![true, false, false, false, false];
    ensure!(direct_rows(&direct, &mixed)? == expected, "A rejection map");
    for verifier in [&cpu, &gpu] {
        let actual = verifier.verify_batch(mixed.clone())?;
        ensure!(
            actual.iter().map(Result::is_ok).collect::<Vec<_>>() == expected,
            "unified rejection map or purpose binding differs"
        );
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema":"novovm.nuas.semantic-entry-comparison.v1",
            "scope":"signature consumer component; NOT authorization, state execution, chain TPS or finality",
            "library":path,"library_sha256":library_sha256,
            "signed_corpus_sha256":corpus_sha256,"rows":count,"samples":samples,
            "same_signed_corpus_all_groups":true,
            "sample_order":"ABC then CBA, alternating",
            "key_generation_signing_and_input_clone_timed":false,
            "first_after_qualification_us":first,
            "A":{"entry":"aoem_mldsa_verify_batch_v1","configured_backend":"cpu",
                "actual_backend_receipt":"UNAVAILABLE in legacy ABI; not inferred from elapsed time",
                "steady_us":summary(&timings[0])},
            "B":{"entry":"aoem_execute_ops_wire_v1 / compute.crypto.mldsa_verify_v1",
                "required_backend":"cpu","actual_last_execution":cpu_execution,"steady_us":summary(&timings[1])},
            "C":{"entry":"aoem_execute_ops_wire_v1 / compute.crypto.mldsa_verify_v1",
                "required_backend":"gpu","actual_last_execution":gpu_execution,"steady_us":summary(&timings[2])},
            "binding_counterexamples":"signature tamper, message tamper, wrong purpose context, malformed row; original indices retained",
            "fallback":false,
            "timing_boundary":"canonical NUAS validation/preparation, batch call, ordered verdict validation; B/C also construct signature-only evidence",
            "limitations":["A is isolated diagnostic reference, not a product selectable route",
                "last execution metadata is read immediately by this single caller, not a concurrent authorization token",
                "no state permissions, fees, nonce, balances, journal or receipt observations are measured"]
        }))?
    );
    Ok(())
}

fn measure(
    group: usize,
    direct: &AoemDyn,
    cpu: &AssetSpendVerifierV3,
    gpu: &AssetSpendVerifierV3,
    rows: &[SignedAssetSpendV3],
) -> Result<f64> {
    // Equal caller-owned input cloning is outside all three timing boundaries.
    let input = rows.to_vec();
    let started = Instant::now();
    if group == 0 {
        let verified = direct_rows(direct, &input)?;
        ensure!(
            verified.len() == rows.len() && verified.iter().all(|v| *v),
            "A invalid"
        );
        black_box(verified);
    } else {
        let verifier = if group == 1 { cpu } else { gpu };
        let verified = verifier.verify_batch(input)?;
        ensure!(
            verified.len() == rows.len() && verified.iter().all(Result::is_ok),
            "B/C invalid"
        );
        let execution = verifier
            .last_execution_v1()
            .context("missing semantic execution receipt")?;
        ensure!(
            execution.rows as usize == rows.len(),
            "wrong completed row count"
        );
        ensure!(
            execution.valid_count as usize == rows.len(),
            "wrong valid row count"
        );
        ensure!(
            execution.backend_kind == if group == 1 { 1 } else { 4 },
            "backend substitution"
        );
        black_box(verified);
    }
    Ok(started.elapsed().as_secs_f64() * 1e6)
}

fn direct_rows(runtime: &AoemDyn, rows: &[SignedAssetSpendV3]) -> Result<Vec<bool>> {
    let mut results = vec![false; rows.len()];
    let mut prepared = Vec::new();
    let mut total_message_bytes = 0usize;
    for (index, row) in rows.iter().enumerate() {
        if row.validate().is_err() {
            continue;
        }
        let message = row.intent.signing_bytes()?;
        // Test/reference-only expansion of the existing external-pure contract.
        // B/C use the real facade's sole framing owner. Never preframe their input.
        let mut framed = vec![0, u8::try_from(SPEND_CONTEXT_V3.len())?];
        framed.extend_from_slice(SPEND_CONTEXT_V3);
        framed.extend_from_slice(&message);
        total_message_bytes = total_message_bytes
            .checked_add(framed.len())
            .context("byte overflow")?;
        ensure!(
            total_message_bytes <= 1024 * 1024,
            "same facade message budget exceeded"
        );
        prepared.push((index, row, framed));
    }
    let input = prepared
        .iter()
        .map(|(_, row, message)| AoemMldsaVerifyItemRef {
            level: 65,
            pubkey: &row.public_key,
            message,
            signature: &row.signature,
        })
        .collect::<Vec<_>>();
    let verified = runtime.mldsa_verify_batch_v1(&input)?;
    ensure!(verified.len() == prepared.len(), "A row count mismatch");
    for ((index, _, _), valid) in prepared.into_iter().zip(verified) {
        results[index] = valid;
    }
    Ok(results)
}

fn signed_rows(
    facade: &AoemExecFacade,
    count: usize,
) -> Result<(Vec<SignedAssetSpendV3>, SignedAssetSpendV3)> {
    let runtime = facade.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let signer = facade.asset_spend_signer_v3()?;
    let pairs = (0..count.min(64))
        .map(|_| runtime.generate_keypair())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut rows = Vec::with_capacity(count);
    for index in 0..count {
        let key = &pairs[index % pairs.len()];
        rows.push(signer.sign(
            AssetSpendIntentV3 {
                authority: novovm_protocol::uca_spend::AssetSpendAuthorityV3::Controller,
                chain_id: 991_027,
                genesis: [93; 32],
                payer: ((index % pairs.len()) as u128).to_be_bytes().repeat(2),
                authorization_epoch: 1,
                spend_key_ref: spend_key_ref_v1(&key.public_key)?,
                recipient: vec![0xff; 32],
                asset: novovm_protocol::uca_spend::NATIVE_NOV_ASSET,
                amount: 1,
                max_fee: 50,
                nonce: (index / pairs.len()) as u64,
                not_before_height: 1,
                expires_at_height: 100,
                authority_update: None,
                recipient_creation: None,
            },
            &key.public_key,
            &key.expanded_secret_key,
        )?);
    }
    let mut wrong_context = rows[0].clone();
    wrong_context.signature = runtime.sign(
        &pairs[0].public_key,
        &pairs[0].expanded_secret_key,
        &wrong_context.intent.signing_bytes()?,
        b"NOT/NOVOVM/UCA/SPEND",
    )?;
    Ok((rows, wrong_context))
}

fn summary(samples: &[f64]) -> Value {
    let mut ordered = samples.to_vec();
    ordered.sort_by(f64::total_cmp);
    let rank = |percent: usize| ordered[(percent * ordered.len()).div_ceil(100) - 1];
    json!({"p50":rank(50),"p95":rank(95),"p99":rank(99),"raw":samples})
}

fn execution_json(verifier: &AssetSpendVerifierV3) -> Result<Value> {
    let value = verifier
        .last_execution_v1()
        .context("missing measured execution receipt")?;
    Ok(
        json!({"sequence":value.sequence,"backend_kind":value.backend_kind,
        "rows":value.rows,"valid_count":value.valid_count}),
    )
}
