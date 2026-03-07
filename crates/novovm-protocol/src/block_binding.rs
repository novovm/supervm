#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CONSENSUS_PLUGIN_CLASS_CODE: u8 = 1;
pub const LOCAL_PLUGIN_CLASS_CODE: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsensusPluginBindingV1 {
    pub plugin_class_code: u8,
    pub adapter_hash: [u8; 32],
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ConsensusPluginBindingError {
    #[error(
        "plugin class mismatch: expected={expected}({expected_name}) got={actual}({actual_name})"
    )]
    ClassMismatch {
        expected: u8,
        expected_name: &'static str,
        actual: u8,
        actual_name: &'static str,
    },
    #[error("adapter hash mismatch: expected={expected:?} got={actual:?}")]
    AdapterHashMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
}

pub fn plugin_class_name(code: u8) -> &'static str {
    match code {
        CONSENSUS_PLUGIN_CLASS_CODE => "consensus",
        LOCAL_PLUGIN_CLASS_CODE => "local",
        _ => "unknown",
    }
}

pub fn verify_consensus_plugin_binding(
    expected: ConsensusPluginBindingV1,
    actual: ConsensusPluginBindingV1,
) -> Result<(), ConsensusPluginBindingError> {
    if expected.plugin_class_code != actual.plugin_class_code {
        return Err(ConsensusPluginBindingError::ClassMismatch {
            expected: expected.plugin_class_code,
            expected_name: plugin_class_name(expected.plugin_class_code),
            actual: actual.plugin_class_code,
            actual_name: plugin_class_name(actual.plugin_class_code),
        });
    }
    if expected.adapter_hash != actual.adapter_hash {
        return Err(ConsensusPluginBindingError::AdapterHashMismatch {
            expected: expected.adapter_hash,
            actual: actual.adapter_hash,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_binding_accepts_equal_values() {
        let a = ConsensusPluginBindingV1 {
            plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
            adapter_hash: [7u8; 32],
        };
        let b = a;
        assert!(verify_consensus_plugin_binding(a, b).is_ok());
    }

    #[test]
    fn verify_binding_rejects_class_mismatch() {
        let expected = ConsensusPluginBindingV1 {
            plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
            adapter_hash: [1u8; 32],
        };
        let actual = ConsensusPluginBindingV1 {
            plugin_class_code: LOCAL_PLUGIN_CLASS_CODE,
            adapter_hash: [1u8; 32],
        };
        let err = verify_consensus_plugin_binding(expected, actual)
            .unwrap_err()
            .to_string();
        assert!(err.contains("plugin class mismatch"));
    }

    #[test]
    fn verify_binding_rejects_hash_mismatch() {
        let expected = ConsensusPluginBindingV1 {
            plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
            adapter_hash: [1u8; 32],
        };
        let actual = ConsensusPluginBindingV1 {
            plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
            adapter_hash: [2u8; 32],
        };
        let err = verify_consensus_plugin_binding(expected, actual)
            .unwrap_err()
            .to_string();
        assert!(err.contains("adapter hash mismatch"));
    }
}
