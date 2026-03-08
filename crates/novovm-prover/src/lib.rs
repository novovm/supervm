#![forbid(unsafe_code)]

use novovm_exec::AoemCapabilityContract;
use serde::{Deserialize, Serialize};

/// Stable fallback reason buckets used by prover-side diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProverFallbackKind {
    InvalidInput,
    EngineUnavailable,
    FeatureDisabled,
    ResourceUnavailable,
    Unknown,
}

impl ProverFallbackKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::EngineUnavailable => "engine_unavailable",
            Self::FeatureDisabled => "feature_disabled",
            Self::ResourceUnavailable => "resource_unavailable",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProverFallbackReason {
    pub code: String,
    pub kind: ProverFallbackKind,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProverCapabilityContract {
    pub execute_ops_v2: bool,
    pub zkvm_prove: bool,
    pub zkvm_verify: bool,
    pub msm_accel: bool,
    pub msm_backend: Option<String>,
    pub zk_formal_fields_present: bool,
    pub fallback_reason: Option<String>,
    pub fallback_reason_codes: Vec<String>,
    pub fallback_reasons: Vec<ProverFallbackReason>,
    pub zk_ready: bool,
    pub prover_ready: bool,
}

impl ProverCapabilityContract {
    #[must_use]
    pub fn from_aoem(contract: &AoemCapabilityContract) -> Self {
        let fallback_reasons = contract
            .fallback_reason_codes
            .iter()
            .map(|code| ProverFallbackReason {
                code: code.clone(),
                kind: classify_fallback_kind(code),
            })
            .collect::<Vec<_>>();
        let zk_ready = contract.zkvm_prove || contract.zkvm_verify;
        let prover_ready = contract.execute_ops_v2 && zk_ready;

        Self {
            execute_ops_v2: contract.execute_ops_v2,
            zkvm_prove: contract.zkvm_prove,
            zkvm_verify: contract.zkvm_verify,
            msm_accel: contract.msm_accel,
            msm_backend: contract.msm_backend.clone(),
            zk_formal_fields_present: contract.zk_formal_fields_present,
            fallback_reason: contract.fallback_reason.clone(),
            fallback_reason_codes: contract.fallback_reason_codes.clone(),
            fallback_reasons,
            zk_ready,
            prover_ready,
        }
    }
}

fn classify_fallback_kind(code: &str) -> ProverFallbackKind {
    let c = code.trim().to_ascii_lowercase();
    if c.contains("invalid") || c.contains("bad_input") || c.contains("unsupported_op") {
        ProverFallbackKind::InvalidInput
    } else if c.contains("ffi_missing") || c.contains("engine_missing") || c.contains("not_linked")
    {
        ProverFallbackKind::EngineUnavailable
    } else if c.contains("disabled") || c.contains("feature_off") || c.contains("no_zkvm") {
        ProverFallbackKind::FeatureDisabled
    } else if c.contains("gpu_unavailable")
        || c.contains("out_of_memory")
        || c.contains("resource_busy")
    {
        ProverFallbackKind::ResourceUnavailable
    } else {
        ProverFallbackKind::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_exec::AoemCapabilityContract;
    use serde_json::json;

    #[test]
    fn contract_mapping_is_stable() {
        let aoem = AoemCapabilityContract::from_capabilities_json(json!({
            "execute_ops_v2": true,
            "zkvm": { "prove": true, "verify": false },
            "msm": {
                "accel": true,
                "backend": "bls12_381_gpu",
                "fallback_reason_codes": ["gpu_unavailable", "invalid_input"]
            }
        }));
        let prover = ProverCapabilityContract::from_aoem(&aoem);
        assert!(prover.execute_ops_v2);
        assert!(prover.zkvm_prove);
        assert!(!prover.zkvm_verify);
        assert!(prover.zk_ready);
        assert!(prover.prover_ready);
        assert_eq!(prover.fallback_reasons.len(), 2);
        assert_eq!(
            prover.fallback_reasons[0].kind,
            ProverFallbackKind::ResourceUnavailable
        );
        assert_eq!(
            prover.fallback_reasons[1].kind,
            ProverFallbackKind::InvalidInput
        );
    }

    #[test]
    fn prover_not_ready_without_zk_flags() {
        let aoem = AoemCapabilityContract::from_capabilities_json(json!({
            "execute_ops_v2": true,
            "backend_gpu_path": true
        }));
        let prover = ProverCapabilityContract::from_aoem(&aoem);
        assert!(!prover.zk_ready);
        assert!(!prover.prover_ready);
    }
}
