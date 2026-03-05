use anyhow::Result;
use novovm_exec::AoemCapabilityContract;
use novovm_prover::ProverCapabilityContract;
use serde_json::json;

fn case_missing_formal_fields() -> bool {
    let raw = json!({
        "execute_ops_v2": true,
        "backend_gpu_path": true,
        "fallback_reason_codes": ["gpu_unavailable"]
    });
    let aoem = AoemCapabilityContract::from_capabilities_json(raw);
    let prover = ProverCapabilityContract::from_aoem(&aoem);
    let schema_ok = aoem.zk_formal_fields_present && !prover.fallback_reason_codes.is_empty();
    !schema_ok
}

fn case_empty_reason_codes() -> bool {
    let raw = json!({
        "execute_ops_v2": true,
        "zkvm": {
            "prove_enabled": true,
            "verify_enabled": false
        },
        "msm": {
            "accel": true,
            "backend": "bls12_381_gpu"
        }
    });
    let aoem = AoemCapabilityContract::from_capabilities_json(raw);
    let prover = ProverCapabilityContract::from_aoem(&aoem);
    let schema_ok = aoem.zk_formal_fields_present && !prover.fallback_reason_codes.is_empty();
    !schema_ok
}

fn case_reason_normalization_stable() -> bool {
    let raw = json!({
        "execute_ops_v2": true,
        "zkvm": {
            "prove": true,
            "verify": true
        },
        "fallback": {
            "reason_codes": ["GPU Unavailable", "gpu-unavailable", "invalid input"]
        }
    });
    let aoem = AoemCapabilityContract::from_capabilities_json(raw);
    let prover = ProverCapabilityContract::from_aoem(&aoem);
    prover.fallback_reason_codes
        == vec![
            "gpu_unavailable".to_string(),
            "invalid_input".to_string(),
        ]
}

fn main() -> Result<()> {
    let missing_formal_fields = case_missing_formal_fields();
    let empty_reason_codes = case_empty_reason_codes();
    let reason_normalization_stable = case_reason_normalization_stable();
    let pass = missing_formal_fields && empty_reason_codes && reason_normalization_stable;

    println!(
        "prover_contract_negative_out: missing_formal_fields={} empty_reason_codes={} reason_normalization_stable={} pass={}",
        missing_formal_fields, empty_reason_codes, reason_normalization_stable, pass
    );
    Ok(())
}
