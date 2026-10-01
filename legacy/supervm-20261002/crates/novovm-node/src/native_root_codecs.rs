#![forbid(unsafe_code)]

//! Consensus root profiles, not physical record-storage or single-receipt codecs.
//!
//! A profile is indivisible. Changing it requires a separately pinned fresh-chain
//! protocol; recognizing a profile here does not authorize an existing-chain
//! transition or attest to the execution evidence represented by an opaque hash.

use anyhow::{bail, Result};

pub const LEGACY_STATE_ROOT_CODEC_V3: &str = "novovm-consensus-native-state-wire/v1";
pub const LEGACY_RECEIPT_ROOT_CODEC_V2: &str = "novovm-consensus-receipt-wire/v1";
pub const LEGACY_EXECUTION_EVIDENCE_CODEC_V1: &str =
    "novovm-aoem-native-batch-consensus-evidence/v1";
pub const RECORD_STATE_ROOT_CODEC_V1: &str = "novovm-consensus-native-record-tree/v1";
pub const RECORD_RECEIPT_ROOT_CODEC_V1: &str = "novovm-consensus-receipt-tree/v1";
pub const RECORD_EXECUTION_EVIDENCE_CODEC_V1: &str = "novovm-aoem-native-record-evidence/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeRootCodecProfileV1 {
    LegacyWireV1,
    RecordTreeV1,
}

impl NativeRootCodecProfileV1 {
    pub const fn state_root_codec(self) -> &'static str {
        match self {
            Self::LegacyWireV1 => LEGACY_STATE_ROOT_CODEC_V3,
            Self::RecordTreeV1 => RECORD_STATE_ROOT_CODEC_V1,
        }
    }

    pub const fn receipt_root_codec(self) -> &'static str {
        match self {
            Self::LegacyWireV1 => LEGACY_RECEIPT_ROOT_CODEC_V2,
            Self::RecordTreeV1 => RECORD_RECEIPT_ROOT_CODEC_V1,
        }
    }

    pub const fn execution_evidence_codec(self) -> &'static str {
        match self {
            Self::LegacyWireV1 => LEGACY_EXECUTION_EVIDENCE_CODEC_V1,
            Self::RecordTreeV1 => RECORD_EXECUTION_EVIDENCE_CODEC_V1,
        }
    }

    pub fn from_root_codecs(state: &str, receipt: &str) -> Result<Self> {
        match (state, receipt) {
            (LEGACY_STATE_ROOT_CODEC_V3, LEGACY_RECEIPT_ROOT_CODEC_V2) => Ok(Self::LegacyWireV1),
            (RECORD_STATE_ROOT_CODEC_V1, RECORD_RECEIPT_ROOT_CODEC_V1) => Ok(Self::RecordTreeV1),
            _ => bail!("unsupported or mixed NOV native consensus root codecs"),
        }
    }

    pub fn from_codecs(state: &str, receipt: &str, evidence: &str) -> Result<Self> {
        let profile = Self::from_root_codecs(state, receipt)?;
        if evidence != profile.execution_evidence_codec() {
            bail!("NOV native execution evidence codec differs from root profile");
        }
        Ok(profile)
    }

    /// This version defines no in-place root-profile migration.
    pub fn validate_successor_of(self, parent: Self) -> Result<()> {
        if self != parent {
            bail!("NOV native parent/child consensus root codec discontinuity");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_codec_profiles_accept_only_complete_matching_triples() {
        for state in [
            NativeRootCodecProfileV1::LegacyWireV1,
            NativeRootCodecProfileV1::RecordTreeV1,
        ] {
            for receipt in [
                NativeRootCodecProfileV1::LegacyWireV1,
                NativeRootCodecProfileV1::RecordTreeV1,
            ] {
                for evidence in [
                    NativeRootCodecProfileV1::LegacyWireV1,
                    NativeRootCodecProfileV1::RecordTreeV1,
                ] {
                    let actual = NativeRootCodecProfileV1::from_codecs(
                        state.state_root_codec(),
                        receipt.receipt_root_codec(),
                        evidence.execution_evidence_codec(),
                    );
                    assert_eq!(actual.is_ok(), state == receipt && state == evidence);
                }
                assert_eq!(
                    state.validate_successor_of(receipt).is_ok(),
                    state == receipt
                );
            }
        }
    }

    #[test]
    fn root_codec_profiles_reject_unknown_empty_and_noncanonical_labels() {
        let legacy = NativeRootCodecProfileV1::LegacyWireV1;
        for bad in [
            "",
            "unknown",
            " novovm-consensus-native-state-wire/v1",
            "novovm-consensus-native-state-wire/v1\0",
        ] {
            assert!(
                NativeRootCodecProfileV1::from_root_codecs(bad, legacy.receipt_root_codec())
                    .is_err()
            );
            assert!(
                NativeRootCodecProfileV1::from_root_codecs(legacy.state_root_codec(), bad).is_err()
            );
            assert!(NativeRootCodecProfileV1::from_codecs(
                legacy.state_root_codec(),
                legacy.receipt_root_codec(),
                bad
            )
            .is_err());
        }
    }
}
