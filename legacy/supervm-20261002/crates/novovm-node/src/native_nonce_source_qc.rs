#![forbid(unsafe_code)]

//! Portable verification of prepare-QC attestations for complete nonce source
//! history. A prepare QC, even with an unbroken parent-QC chain, is not a commit
//! proof, independent AOEM execution verification, or permission to activate.
//! No database, environment, runtime, signer, or mutable source is consulted.

use super::native_nonce_bundle::{
    checkpoint_bundle_digest_v1, verified_nonce_checkpoint_history_v1,
    MAX_CHECKPOINT_BUNDLE_BYTES_V1,
};
use super::native_nonce_checkpoint::{NonceMigrationCheckpointV1, MAX_CHECKPOINT_BLOCKS_V1};
use crate::native_block_seal::{
    subject_from_block_v1, NovNativeSealProposalV1, NovNativeSealQuorumCertificateV1,
};
use crate::native_block_seal_overlay::{
    NovNativeSealEpochAuthorityV1, NovNativeSealOverlayArtifactV1,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const MAX_NONCE_SOURCE_QC_BYTES_V1: usize = 64 * 1024 * 1024;
const DOCUMENT_SCHEMA_V1: &str = "novovm-native-nonce-source-prepare-qc/v1";

pub struct NonceSourceQcInputsV1<'a> {
    pub bundle: &'a [u8],
    pub bundle_digest: &'a str,
    pub checkpoint: &'a NonceMigrationCheckpointV1,
    pub authority: &'a NovNativeSealEpochAuthorityV1,
    pub expected_authority_commitment: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NonceSourceQcEntryV1 {
    pub proposal: NovNativeSealProposalV1,
    pub qc: NovNativeSealQuorumCertificateV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NonceSourceQcDocumentV1 {
    pub schema: String,
    pub entries: Vec<NonceSourceQcEntryV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NonceSourceQcReportV1 {
    pub schema: &'static str,
    pub chain_id: u64,
    pub genesis_block_hash: String,
    pub namespace_digest: String,
    pub epoch: u64,
    pub checkpoint_height: u64,
    pub checkpoint_block_hash: String,
    pub old_protocol_config_commitment: String,
    pub source_bundle_digest: String,
    pub snapshot_digest: String,
    pub authority_commitment: String,
    pub validator_set_hash: String,
    pub old_state_root: String,
    pub receipt_root: String,
    pub state_version: u64,
    pub block_count: usize,
    pub qc_count: usize,
    pub tip_qc_hash: String,
    pub prepare_qc_chain_verified: bool,
    pub source_finality_verified: bool,
    pub aoem_execution_verified: bool,
    pub activation_ready: bool,
    pub import_performed: bool,
    pub chain_canonical: bool,
    pub proof_sealed: bool,
    pub safe: bool,
    pub finalized: bool,
    pub execution_replayed: bool,
}

fn parse_commitment(value: &str, label: &str) -> Result<[u8; 32]> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("nonce source QC {label} must be canonical lowercase 32-byte hex");
    }
    let mut parsed = [0u8; 32];
    for (index, byte) in parsed.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .context("decode nonce source QC commitment")?;
    }
    if parsed == [0; 32] {
        bail!("nonce source QC {label} must be nonzero");
    }
    Ok(parsed)
}

fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Verify a caller-supplied portable attestation document under an independently
/// pinned old authority. The typed decode rejects duplicate known fields; the
/// Value comparison also rejects unknown fields in the reused nested seal types.
/// Successful verification does not attest that this is the unique network tip.
pub fn verify_nonce_source_qc_json_v1(
    bytes: &[u8],
    inputs: &NonceSourceQcInputsV1<'_>,
) -> Result<NonceSourceQcReportV1> {
    if bytes.is_empty() || bytes.len() > MAX_NONCE_SOURCE_QC_BYTES_V1 {
        bail!("nonce source QC document exceeds its nonempty 64 MiB bound");
    }
    if inputs.bundle.is_empty() || inputs.bundle.len() > MAX_CHECKPOINT_BUNDLE_BYTES_V1 {
        bail!("nonce source QC bundle exceeds its nonempty byte bound");
    }
    let expected_authority =
        parse_commitment(inputs.expected_authority_commitment, "authority pin")?;
    inputs.authority.validate()?;
    if inputs.authority.authority_commitment != expected_authority {
        bail!("nonce source QC authority differs from independent pin");
    }
    parse_commitment(inputs.bundle_digest, "bundle digest")?;
    if checkpoint_bundle_digest_v1(inputs.bundle) != inputs.bundle_digest {
        bail!("nonce source QC bundle digest mismatch");
    }
    let document: NonceSourceQcDocumentV1 =
        serde_json::from_slice(bytes).context("decode nonce source prepare-QC document")?;
    if document.schema != DOCUMENT_SCHEMA_V1
        || document.entries.is_empty()
        || document.entries.len() > MAX_CHECKPOINT_BLOCKS_V1
    {
        bail!("nonce source QC schema or entry count is invalid");
    }
    let raw: serde_json::Value = serde_json::from_slice(bytes)?;
    if serde_json::to_value(&document)? != raw {
        bail!("nonce source QC document has unknown or noncanonical nested fields");
    }
    let history = verified_nonce_checkpoint_history_v1(inputs.bundle, inputs.checkpoint)?;
    if document.entries.len() != history.blocks.len() {
        bail!("nonce source QC requires exactly one proposal and QC for every source block");
    }
    let genesis = history
        .blocks
        .first()
        .context("nonce source QC has no genesis")?
        .header
        .block_hash;
    let old_protocol = parse_commitment(
        &inputs.checkpoint.legacy_protocol_config_commitment,
        "old protocol commitment",
    )?;
    if inputs.authority.chain_id != inputs.checkpoint.chain_id
        || inputs.authority.genesis_block_hash != genesis
        || inputs.authority.protocol_config_commitment != old_protocol
        || history.head.height < inputs.authority.activation_height
    {
        bail!("nonce source QC old authority does not bind the verified source domain");
    }
    let mut parent_qc = [0u8; 32];
    for (block, entry) in history.blocks.iter().zip(&document.entries) {
        // Recompute every committed field, including body/receipt roots and AOEM
        // evidence commitments. Equality of commitments is not proof of execution.
        let expected = subject_from_block_v1(
            block,
            &inputs.authority.validator_set,
            0,
            parent_qc,
            genesis,
            old_protocol,
        )?;
        if entry.proposal.subject != expected || entry.qc.subject != expected {
            bail!("nonce source QC subject does not match ordered source block and parent QC");
        }
        NovNativeSealOverlayArtifactV1::QuorumCertificate {
            proposal: Box::new(entry.proposal.clone()),
            qc: Box::new(entry.qc.clone()),
        }
        .validate(inputs.authority)
        .with_context(|| {
            format!(
                "verify nonce source prepare QC at height {}",
                block.header.height
            )
        })?;
        parent_qc = entry.qc.qc_hash;
    }
    Ok(NonceSourceQcReportV1 {
        schema: "novovm-native-nonce-source-prepare-qc-report/v1",
        chain_id: inputs.checkpoint.chain_id,
        genesis_block_hash: hex(&genesis),
        namespace_digest: inputs.checkpoint.namespace_digest.clone(),
        epoch: inputs.authority.epoch,
        checkpoint_height: history.head.height,
        checkpoint_block_hash: hex(&history.head.block_hash),
        old_protocol_config_commitment: inputs.checkpoint.legacy_protocol_config_commitment.clone(),
        source_bundle_digest: inputs.bundle_digest.to_string(),
        snapshot_digest: inputs.checkpoint.snapshot_digest.clone(),
        authority_commitment: hex(&expected_authority),
        validator_set_hash: hex(&inputs.authority.validator_set.validator_set_hash),
        old_state_root: hex(&history.head.post_state_root),
        receipt_root: hex(&history.head.cumulative_receipt_root),
        state_version: history.head.state_version,
        block_count: history.blocks.len(),
        qc_count: document.entries.len(),
        tip_qc_hash: hex(&parent_qc),
        prepare_qc_chain_verified: true,
        source_finality_verified: false,
        aoem_execution_verified: false,
        activation_ready: false,
        import_performed: false,
        chain_canonical: false,
        proof_sealed: false,
        safe: false,
        finalized: false,
        execution_replayed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::super::native_nonce_bundle::encode_bundle;
    use super::super::native_nonce_checkpoint::test_fixture_v1;
    use super::*;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StaticFixture {
        authority: NovNativeSealEpochAuthorityV1,
        expected_authority_commitment: String,
        source_qc: NonceSourceQcDocumentV1,
    }

    struct Fixture {
        bundle: Vec<u8>,
        digest: String,
        checkpoint: NonceMigrationCheckpointV1,
        data: StaticFixture,
    }

    impl Fixture {
        fn new() -> Self {
            let data = serde_json::from_str(include_str!(
                "../../novovmctl/tests/fixtures/native_nonce_source_qc_v1.json"
            ))
            .unwrap();
            let (snapshot, head, blocks, checkpoint, _) = test_fixture_v1();
            let bundle = encode_bundle(&snapshot, &head, &blocks).unwrap();
            let digest = checkpoint_bundle_digest_v1(&bundle);
            Self {
                bundle,
                digest,
                checkpoint,
                data,
            }
        }

        fn inputs(&self) -> NonceSourceQcInputsV1<'_> {
            NonceSourceQcInputsV1 {
                bundle: &self.bundle,
                bundle_digest: &self.digest,
                checkpoint: &self.checkpoint,
                authority: &self.data.authority,
                expected_authority_commitment: &self.data.expected_authority_commitment,
            }
        }

        fn bytes(&self) -> Vec<u8> {
            serde_json::to_vec(&self.data.source_qc).unwrap()
        }

        fn reject(&self, document: &NonceSourceQcDocumentV1) {
            assert!(verify_nonce_source_qc_json_v1(
                &serde_json::to_vec(document).unwrap(),
                &self.inputs()
            )
            .is_err());
        }
    }

    #[test]
    fn native_nonce_source_qc_complete_history_is_attested_not_finalized() {
        let fixture = Fixture::new();
        let before = fixture.bundle.clone();
        let report = verify_nonce_source_qc_json_v1(&fixture.bytes(), &fixture.inputs()).unwrap();
        assert_eq!(report.block_count, 2);
        assert_eq!(report.qc_count, 2);
        assert_eq!(
            report.tip_qc_hash,
            hex(&fixture.data.source_qc.entries[1].qc.qc_hash)
        );
        assert_eq!(report.source_bundle_digest, fixture.digest);
        assert_eq!(
            report.checkpoint_block_hash,
            fixture.checkpoint.tip_block_hash
        );
        assert_eq!(
            report.authority_commitment,
            fixture.data.expected_authority_commitment
        );
        assert!(report.prepare_qc_chain_verified);
        assert!(!report.source_finality_verified && !report.aoem_execution_verified);
        assert!(!report.activation_ready && !report.import_performed);
        assert!(
            !report.chain_canonical && !report.proof_sealed && !report.safe && !report.finalized
        );
        assert!(!report.execution_replayed);
        assert_eq!(fixture.bundle, before);
    }

    #[test]
    fn native_nonce_source_qc_rejects_missing_extra_reordered_or_duplicate_pairs() {
        let fixture = Fixture::new();
        for entries in [
            Vec::new(),
            vec![fixture.data.source_qc.entries[1].clone()],
            vec![fixture.data.source_qc.entries[0].clone()],
            vec![fixture.data.source_qc.entries[0].clone(); 2],
            vec![
                fixture.data.source_qc.entries[1].clone(),
                fixture.data.source_qc.entries[0].clone(),
            ],
            vec![fixture.data.source_qc.entries[0].clone(); 3],
            vec![fixture.data.source_qc.entries[0].clone(); MAX_CHECKPOINT_BLOCKS_V1 + 1],
        ] {
            fixture.reject(&NonceSourceQcDocumentV1 {
                schema: DOCUMENT_SCHEMA_V1.into(),
                entries,
            });
        }
    }

    #[test]
    fn native_nonce_source_qc_rejects_bad_quorum_signatures_and_proposal_binding() {
        let fixture = Fixture::new();
        let original = &fixture.data.source_qc;
        let mut changed = original.clone();
        changed.entries[0].qc.votes.pop();
        changed.entries[0].qc.signature_count = changed.entries[0].qc.votes.len() as u32;
        fixture.reject(&changed);
        changed = original.clone();
        changed.entries[0].qc.votes[1] = changed.entries[0].qc.votes[0].clone();
        fixture.reject(&changed);
        changed = original.clone();
        changed.entries[0].qc.votes[0].signature[0] ^= 1;
        fixture.reject(&changed);
        changed = original.clone();
        changed.entries[0].proposal.signature[0] ^= 1;
        fixture.reject(&changed);
        changed = original.clone();
        changed.entries[0].proposal.proposer_id = [0x99; 32];
        fixture.reject(&changed);
        changed = original.clone();
        changed.entries[0].qc.signed_weight += 1;
        fixture.reject(&changed);
        changed = original.clone();
        changed.entries[0].qc.threshold_satisfied = false;
        fixture.reject(&changed);
        changed = original.clone();
        changed.entries[0].qc.proposal_hash[0] ^= 1;
        fixture.reject(&changed);
        changed = original.clone();
        changed.entries[1].qc = changed.entries[0].qc.clone();
        fixture.reject(&changed);
    }

    #[test]
    fn native_nonce_source_qc_rejects_changed_subject_roots_domain_round_and_parent() {
        let fixture = Fixture::new();
        let original = &fixture.data.source_qc;
        for field in [
            "post_state_root",
            "pre_state_root",
            "block_receipt_root",
            "cumulative_receipt_root",
            "ordered_tx_root",
            "body_digest",
            "genesis_block_hash",
            "protocol_config_commitment",
            "validator_set_hash",
            "aoem_evidence_commitment",
            "justify_qc_hash",
        ] {
            let mut changed = serde_json::to_value(original).unwrap();
            let byte = changed["entries"][1]["proposal"]["subject"][field][0]
                .as_u64()
                .unwrap();
            changed["entries"][1]["proposal"]["subject"][field][0] = serde_json::json!(byte ^ 1);
            let result = verify_nonce_source_qc_json_v1(
                &serde_json::to_vec(&changed).unwrap(),
                &fixture.inputs(),
            );
            assert!(result.is_err(), "changed {field} was accepted");
        }
        for field in [
            "round",
            "height",
            "slot",
            "chain_id",
            "epoch",
            "state_version",
        ] {
            let mut changed = serde_json::to_value(original).unwrap();
            changed["entries"][0]["proposal"]["subject"][field] = serde_json::json!(u64::MAX);
            assert!(verify_nonce_source_qc_json_v1(
                &serde_json::to_vec(&changed).unwrap(),
                &fixture.inputs()
            )
            .is_err());
        }
        let mut changed = original.clone();
        changed.entries[0].proposal.subject.phase = "commit".into();
        fixture.reject(&changed);
    }

    #[test]
    fn native_nonce_source_qc_rejects_wrong_independent_pins_bundle_and_authority() {
        let fixture = Fixture::new();
        let bytes = fixture.bytes();
        for pin in [
            "00".repeat(32),
            "dd".repeat(32),
            "AB".repeat(32),
            "01".into(),
        ] {
            let mut inputs = fixture.inputs();
            inputs.expected_authority_commitment = &pin;
            assert!(verify_nonce_source_qc_json_v1(&bytes, &inputs).is_err());
        }
        let wrong_digest = "aa".repeat(32);
        let mut inputs = fixture.inputs();
        inputs.bundle_digest = &wrong_digest;
        assert!(verify_nonce_source_qc_json_v1(&bytes, &inputs).is_err());
        let mut corrupted_bundle = fixture.bundle.clone();
        corrupted_bundle.push(0);
        let corrupt_digest = checkpoint_bundle_digest_v1(&corrupted_bundle);
        inputs = fixture.inputs();
        inputs.bundle = &corrupted_bundle;
        inputs.bundle_digest = &corrupt_digest;
        assert!(verify_nonce_source_qc_json_v1(&bytes, &inputs).is_err());
        let mut wrong_authority = fixture.data.authority.clone();
        wrong_authority.genesis_block_hash[0] ^= 1;
        inputs = fixture.inputs();
        inputs.authority = &wrong_authority;
        assert!(verify_nonce_source_qc_json_v1(&bytes, &inputs).is_err());
        let mut checkpoint = fixture.checkpoint.clone();
        checkpoint.tip_block_hash = "ef".repeat(32);
        inputs = fixture.inputs();
        inputs.checkpoint = &checkpoint;
        assert!(verify_nonce_source_qc_json_v1(&bytes, &inputs).is_err());
    }

    #[test]
    fn native_nonce_source_qc_rejects_unknown_duplicate_trailing_and_oversized_json() {
        let fixture = Fixture::new();
        let original = serde_json::to_value(&fixture.data.source_qc).unwrap();
        for pointer in [
            "",
            "/entries/0",
            "/entries/0/proposal",
            "/entries/0/proposal/subject",
            "/entries/0/qc",
            "/entries/0/qc/subject",
            "/entries/0/qc/votes/0",
        ] {
            let mut changed = original.clone();
            changed
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("extra".into(), serde_json::json!(true));
            assert!(
                verify_nonce_source_qc_json_v1(
                    &serde_json::to_vec(&changed).unwrap(),
                    &fixture.inputs()
                )
                .is_err(),
                "unknown field at {pointer}"
            );
        }
        let canonical = String::from_utf8(fixture.bytes()).unwrap();
        for duplicated in [
            canonical.replacen("\"schema\":", "\"schema\":\"duplicate\",\"schema\":", 1),
            canonical.replacen("\"phase\":", "\"phase\":\"prepare\",\"phase\":", 1),
            canonical.replacen(
                "\"signature_count\":",
                "\"signature_count\":3,\"signature_count\":",
                1,
            ),
            format!("{canonical} {{}}"),
        ] {
            assert!(
                verify_nonce_source_qc_json_v1(duplicated.as_bytes(), &fixture.inputs()).is_err()
            );
        }
        assert!(verify_nonce_source_qc_json_v1(&[], &fixture.inputs()).is_err());
        assert!(verify_nonce_source_qc_json_v1(
            &vec![b' '; MAX_NONCE_SOURCE_QC_BYTES_V1 + 1],
            &fixture.inputs()
        )
        .is_err());
        assert!(verify_nonce_source_qc_json_v1(
            br#"{"schema":"legacy-consensus-qc","signatures":[]}"#,
            &fixture.inputs()
        )
        .is_err());
    }
}
