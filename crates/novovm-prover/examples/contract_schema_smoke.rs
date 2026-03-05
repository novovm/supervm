use anyhow::Result;
use novovm_exec::AoemCapabilityContract;
use novovm_prover::ProverCapabilityContract;
use serde_json::json;

fn main() -> Result<()> {
    let raw = json!({
        "execute_ops_v2": true,
        "zkvm": {
            "prove_enabled": false,
            "verify_enabled": false
        },
        "msm": {
            "accel": true,
            "backend": "bls12_381_gpu",
            "fallback_reason_codes": ["GPU Unavailable", "invalid input"]
        }
    });

    let aoem = AoemCapabilityContract::from_capabilities_json(raw);
    let prover = ProverCapabilityContract::from_aoem(&aoem);

    let normalized = prover
        .fallback_reason_codes
        .iter()
        .all(|c| c.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'));
    let schema_ok = aoem.zk_formal_fields_present && !prover.fallback_reason_codes.is_empty();

    println!(
        "prover_contract_out: schema_ok={} normalized_reason_codes={} fallback_codes={} prover_ready={} zk_ready={} msm_backend={}",
        schema_ok,
        normalized,
        prover.fallback_reason_codes.len(),
        prover.prover_ready,
        prover.zk_ready,
        prover.msm_backend.as_deref().unwrap_or("")
    );

    Ok(())
}

