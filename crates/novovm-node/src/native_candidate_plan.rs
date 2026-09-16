#![forbid(unsafe_code)]

//! Explicit, deterministic inputs for local Host candidate execution.
//!
//! This is not an authenticated network proposal or an execution proof. A Host
//! must separately validate every raw transaction's signature, chain, identity,
//! canonical hash and nonce against its own verified parent state. Matching a
//! prepared/block input never attests to execution results or finality.

use anyhow::{bail, Context, Result};
use novovm_protocol::NovBlockExecutionContextV1;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

use crate::native_block_ledger::{
    NovNativeDurableBlockV1, NovNativePreparedAoemParentV1, NovNativePreparedBlockV1,
    NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1, NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1,
};

pub const NOV_NATIVE_CANDIDATE_EXECUTION_PLAN_SCHEMA_V1: &str =
    "novovm-native-candidate-execution-plan/v1";

const PLAN_COMMITMENT_DOMAIN_V1: &[u8] = b"novovm-native-candidate-execution-plan-v1\0";
const STATE_ROOT_CODEC_V1: &str = "novovm-consensus-native-state-wire/v1";
const RECEIPT_ROOT_CODEC_V1: &str = "novovm-consensus-receipt-wire/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeCandidateExecutionPlanV1 {
    pub schema: String,
    #[serde(deserialize_with = "deserialize_context_v1")]
    pub context: NovBlockExecutionContextV1,
    pub protocol_config_commitment: [u8; 32],
    pub pre_state_root: [u8; 32],
    #[serde(default, deserialize_with = "deserialize_aoem_parent_v1")]
    pub aoem_parent: Option<NovNativePreparedAoemParentV1>,
    pub tx_hashes: Vec<[u8; 32]>,
    pub raw_txs: Vec<Vec<u8>>,
    pub plan_commitment: [u8; 32],
}

impl NovNativeCandidateExecutionPlanV1 {
    pub fn new(
        context: NovBlockExecutionContextV1,
        protocol_config_commitment: [u8; 32],
        pre_state_root: [u8; 32],
        aoem_parent: Option<NovNativePreparedAoemParentV1>,
        tx_hashes: Vec<[u8; 32]>,
        raw_txs: Vec<Vec<u8>>,
    ) -> Result<Self> {
        let mut plan = Self {
            schema: NOV_NATIVE_CANDIDATE_EXECUTION_PLAN_SCHEMA_V1.to_string(),
            context,
            protocol_config_commitment,
            pre_state_root,
            aoem_parent,
            tx_hashes,
            raw_txs,
            plan_commitment: [0; 32],
        };
        plan.validate_inputs_v1()?;
        plan.plan_commitment = plan.compute_commitment_v1()?;
        Ok(plan)
    }

    /// Check structural bounds and the exact input commitment. This does not
    /// authenticate transactions or verify that the stated parent exists.
    pub fn validate(&self) -> Result<()> {
        self.validate_inputs_v1()?;
        if self.plan_commitment != self.compute_commitment_v1()? {
            bail!("NOV native candidate execution plan commitment mismatch");
        }
        Ok(())
    }

    /// Compare input fields only. The caller must obtain `prepared` through the
    /// local ledger's validation and must separately pin protocol configuration.
    pub fn validate_against_prepared(&self, prepared: &NovNativePreparedBlockV1) -> Result<()> {
        self.validate()?;
        if self.context != prepared.context
            || self.context.commitment()? != prepared.context_commitment
            || self.pre_state_root != prepared.pre_state_root
            || self.aoem_parent != prepared.aoem_parent
            || self.tx_hashes != prepared.tx_hashes
            || self.raw_txs != prepared.raw_txs
        {
            bail!("NOV native candidate plan does not match prepared execution inputs");
        }
        Ok(())
    }

    /// Compare input fields only, without trusting the block's state, receipt,
    /// AOEM result or lifecycle claims. Execution output verification is separate.
    pub fn validate_against_block(&self, block: &NovNativeDurableBlockV1) -> Result<()> {
        self.validate()?;
        if self.context != block.header.execution_context
            || self.context.commitment()? != block.header.execution_context_commitment
            || self.context.chain_id != block.header.chain_id
            || self.context.block_height != block.header.height
            || self.context.parent_block_hash != block.header.parent_block_hash
            || self.context.slot != block.header.slot
            || self.context.timestamp_unix_ms != block.header.timestamp_unix_ms
            || self.context.chain_id != block.body.chain_id
            || self.context.block_height != block.body.height
            || self.pre_state_root != block.header.pre_state_root
            || self.aoem_parent != block.header.aoem_parent
            || self.tx_hashes != block.body.tx_hashes
            || self.raw_txs != block.body.raw_txs
        {
            bail!("NOV native candidate plan does not match durable block execution inputs");
        }
        Ok(())
    }

    fn validate_inputs_v1(&self) -> Result<()> {
        if self.schema != NOV_NATIVE_CANDIDATE_EXECUTION_PLAN_SCHEMA_V1 {
            bail!("unsupported NOV native candidate execution plan schema");
        }
        self.context
            .validate()
            .context("invalid candidate plan context")?;
        if self.protocol_config_commitment == [0; 32] {
            bail!("NOV native candidate plan protocol configuration commitment is empty");
        }
        if self.tx_hashes.is_empty()
            || self.tx_hashes.len() > NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1
            || self.tx_hashes.len() != self.raw_txs.len()
        {
            bail!("NOV native candidate plan requires 1..=1024 aligned transactions");
        }
        let mut hashes = HashSet::with_capacity(self.tx_hashes.len());
        if self.tx_hashes.iter().any(|hash| !hashes.insert(*hash)) {
            bail!("NOV native candidate plan contains duplicate transaction hashes");
        }
        let mut body_bytes = 0usize;
        for raw in &self.raw_txs {
            if raw.is_empty() {
                bail!("NOV native candidate plan contains an empty raw transaction");
            }
            body_bytes = body_bytes
                .checked_add(raw.len())
                .context("candidate body size overflow")?;
            if body_bytes > NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1 {
                bail!("NOV native candidate plan body exceeds 2 MiB");
            }
        }
        if let Some(parent) = &self.aoem_parent {
            for (label, value) in [
                ("batch id", parent.batch_id.as_str()),
                ("batch result id", parent.batch_result_id.as_str()),
                ("state root codec", parent.state_root_codec.as_str()),
                ("receipt root codec", parent.receipt_root_codec.as_str()),
            ] {
                if value.is_empty()
                    || value.trim() != value
                    || !value.is_ascii()
                    || value.len() > 512
                {
                    bail!("candidate AOEM parent {label} must be canonical ASCII of 1..=512 bytes");
                }
            }
            if parent.batch_result_id.len() != 64
                || !parent
                    .batch_result_id
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                || parent.state_root_codec != STATE_ROOT_CODEC_V1
                || parent.receipt_root_codec != RECEIPT_ROOT_CODEC_V1
                || parent.state_root == [0; 32]
                || parent.cumulative_receipt_root == [0; 32]
                || parent.state_version == 0
                || parent.state_root != self.pre_state_root
            {
                bail!("NOV native candidate plan AOEM parent commitment is invalid");
            }
        }
        Ok(())
    }

    /// SHA256(domain || len64be(schema) || NBX1-context || protocol || pre-state
    /// || parent-presence:u8 || optional-parent-fields || tx-count:u64be
    /// || repeated(tx-hash:[u8;32] || len64be(raw))). Strings/raw values carry
    /// u64 big-endian byte lengths; roots are fixed 32 bytes and state_version
    /// is u64 big-endian. NBX1 retains its protocol-defined fixed-width codec.
    fn compute_commitment_v1(&self) -> Result<[u8; 32]> {
        let mut hasher = Sha256::new();
        hasher.update(PLAN_COMMITMENT_DOMAIN_V1);
        hash_length_prefixed_v1(&mut hasher, self.schema.as_bytes());
        hasher.update(self.context.encode()?);
        hasher.update(self.protocol_config_commitment);
        hasher.update(self.pre_state_root);
        match &self.aoem_parent {
            None => hasher.update([0]),
            Some(parent) => {
                hasher.update([1]);
                hash_length_prefixed_v1(&mut hasher, parent.batch_id.as_bytes());
                hash_length_prefixed_v1(&mut hasher, parent.batch_result_id.as_bytes());
                hasher.update(parent.state_root);
                hash_length_prefixed_v1(&mut hasher, parent.state_root_codec.as_bytes());
                hasher.update(parent.cumulative_receipt_root);
                hash_length_prefixed_v1(&mut hasher, parent.receipt_root_codec.as_bytes());
                hasher.update(parent.state_version.to_be_bytes());
            }
        }
        hasher.update((self.tx_hashes.len() as u64).to_be_bytes());
        for (hash, raw) in self.tx_hashes.iter().zip(&self.raw_txs) {
            hasher.update(hash);
            hash_length_prefixed_v1(&mut hasher, raw);
        }
        Ok(hasher.finalize().into())
    }
}

fn hash_length_prefixed_v1(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn deserialize_context_v1<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<NovBlockExecutionContextV1, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ContextWire {
        chain_id: u64,
        block_height: u64,
        parent_block_hash: [u8; 32],
        slot: u64,
        timestamp_unix_ms: u64,
    }
    let wire = ContextWire::deserialize(deserializer)?;
    Ok(NovBlockExecutionContextV1 {
        chain_id: wire.chain_id,
        block_height: wire.block_height,
        parent_block_hash: wire.parent_block_hash,
        slot: wire.slot,
        timestamp_unix_ms: wire.timestamp_unix_ms,
    })
}

fn deserialize_aoem_parent_v1<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<NovNativePreparedAoemParentV1>, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ParentWire {
        batch_id: String,
        batch_result_id: String,
        state_root: [u8; 32],
        state_root_codec: String,
        cumulative_receipt_root: [u8; 32],
        receipt_root_codec: String,
        state_version: u64,
    }
    Ok(
        Option::<ParentWire>::deserialize(deserializer)?.map(|wire| {
            NovNativePreparedAoemParentV1 {
                batch_id: wire.batch_id,
                batch_result_id: wire.batch_result_id,
                state_root: wire.state_root,
                state_root_codec: wire.state_root_codec,
                cumulative_receipt_root: wire.cumulative_receipt_root,
                receipt_root_codec: wire.receipt_root_codec,
                state_version: wire.state_version,
            }
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> NovNativeCandidateExecutionPlanV1 {
        NovNativeCandidateExecutionPlanV1::new(
            NovBlockExecutionContextV1 {
                chain_id: 9,
                block_height: 2,
                parent_block_hash: [1; 32],
                slot: 3,
                timestamp_unix_ms: 1_900_000_000_000,
            },
            [2; 32],
            [3; 32],
            Some(NovNativePreparedAoemParentV1 {
                batch_id: "native-parent-1".into(),
                batch_result_id: "04".repeat(32),
                state_root: [3; 32],
                state_root_codec: STATE_ROOT_CODEC_V1.into(),
                cumulative_receipt_root: [5; 32],
                receipt_root_codec: RECEIPT_ROOT_CODEC_V1.into(),
                state_version: 1,
            }),
            vec![[6; 32], [7; 32]],
            vec![vec![8, 9], vec![10, 11]],
        )
        .unwrap()
    }

    fn resign(
        mut value: NovNativeCandidateExecutionPlanV1,
    ) -> Result<NovNativeCandidateExecutionPlanV1> {
        value.validate_inputs_v1()?;
        value.plan_commitment = value.compute_commitment_v1()?;
        value.validate()?;
        Ok(value)
    }

    fn prepared(value: &NovNativeCandidateExecutionPlanV1) -> NovNativePreparedBlockV1 {
        NovNativePreparedBlockV1 {
            schema: "novovm-native-prepared-block/v1".into(),
            candidate_id: [12; 32],
            context: value.context,
            context_commitment: value.context.commitment().unwrap(),
            pre_state_root: value.pre_state_root,
            ordered_tx_root: [13; 32],
            body_digest: [14; 32],
            body_bytes: 4,
            tx_hashes: value.tx_hashes.clone(),
            raw_txs: value.raw_txs.clone(),
            aoem_parent: value.aoem_parent.clone(),
            expected_aoem_batch_id: None,
            expected_aoem_output_commitment: None,
        }
    }

    fn block(value: &NovNativeCandidateExecutionPlanV1) -> NovNativeDurableBlockV1 {
        use crate::native_block_ledger::{
            NovNativeBlockBodyV1, NovNativeBlockExecutionEvidenceV1, NovNativeBlockHeaderV1,
        };
        NovNativeDurableBlockV1 {
            header: NovNativeBlockHeaderV1 {
                schema: "novovm-native-block-header/v1".into(),
                candidate_kind: "local_unsealed_execution_candidate".into(),
                execution_context: value.context,
                chain_id: value.context.chain_id,
                height: value.context.block_height,
                slot: value.context.slot,
                timestamp_unix_ms: value.context.timestamp_unix_ms,
                parent_block_hash: value.context.parent_block_hash,
                block_hash: [15; 32],
                candidate_id: [12; 32],
                execution_context_commitment: value.context.commitment().unwrap(),
                pre_state_root: value.pre_state_root,
                aoem_parent: value.aoem_parent.clone(),
                post_state_root: [16; 32],
                post_state_root_codec: STATE_ROOT_CODEC_V1.into(),
                ordered_tx_root: [13; 32],
                block_receipt_root: [17; 32],
                cumulative_receipt_root: [18; 32],
                cumulative_receipt_root_codec: RECEIPT_ROOT_CODEC_V1.into(),
                body_digest: [14; 32],
                body_bytes: 4,
                tx_count: 2,
                receipt_count: 2,
                state_version: 2,
                aoem_batch_id: "local-result".into(),
                aoem_batch_result_id: "19".repeat(32),
                aoem_expected_output_commitment: "20".repeat(32),
                aoem_evidence_commitment: [21; 32],
                aoem_readback_verified: false,
                canonical_local: false,
                safe: false,
                finalized: false,
                proof_sealed: false,
            },
            body: NovNativeBlockBodyV1 {
                schema: "novovm-native-block-body/v1".into(),
                chain_id: value.context.chain_id,
                height: value.context.block_height,
                block_hash: [15; 32],
                ordered_tx_root: [13; 32],
                body_digest: [14; 32],
                body_bytes: 4,
                tx_hashes: value.tx_hashes.clone(),
                raw_txs: value.raw_txs.clone(),
            },
            execution_evidence: NovNativeBlockExecutionEvidenceV1 {
                schema: "novovm-native-block-execution-evidence/v1".into(),
                chain_id: value.context.chain_id,
                height: value.context.block_height,
                block_hash: [15; 32],
                aoem_batch_id: "local-result".into(),
                aoem_batch_result_id: "19".repeat(32),
                aoem_expected_output_commitment: "20".repeat(32),
                aoem_evidence_commitment: [21; 32],
                post_state_root: [16; 32],
                cumulative_receipt_root: [18; 32],
                block_receipt_root: [17; 32],
                per_block_receipt_commitments: vec![[22; 32], [23; 32]],
                state_version: 2,
                evidence_kind: "aoem_execution_commitment_not_consensus_seal".into(),
                proof_sealed: false,
            },
        }
    }

    #[test]
    fn plan_roundtrip_replay_and_exact_input_matching() {
        let value = plan();
        value.validate().unwrap();
        let encoded = serde_json::to_vec(&value).unwrap();
        let restored: NovNativeCandidateExecutionPlanV1 = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(restored, value);
        restored.validate().unwrap();
        value.validate_against_prepared(&prepared(&value)).unwrap();
        value.validate_against_block(&block(&value)).unwrap();
        assert_eq!(
            resign(value.clone()).unwrap().plan_commitment,
            value.plan_commitment
        );
        let mut genesis = value;
        genesis.context.block_height = 1;
        genesis.context.parent_block_hash = [0; 32];
        genesis.aoem_parent = None;
        let genesis = resign(genesis).unwrap();
        let restored: NovNativeCandidateExecutionPlanV1 =
            serde_json::from_value(serde_json::to_value(&genesis).unwrap()).unwrap();
        assert_eq!(restored, genesis);
        restored.validate().unwrap();
    }

    #[test]
    fn commitment_codec_has_an_independently_computed_golden_vector() {
        assert_eq!(
            plan().plan_commitment,
            [
                0x41, 0x35, 0x9d, 0xe6, 0xe5, 0xa6, 0x2b, 0x97, 0x68, 0x4e, 0xa7, 0xe2, 0xab, 0xe4,
                0xff, 0xc5, 0x3a, 0x03, 0x80, 0x4a, 0x4c, 0xbd, 0xd2, 0xe6, 0xc6, 0xcf, 0x3c, 0x56,
                0xba, 0xc9, 0x6c, 0x67,
            ]
        );
    }

    #[test]
    fn commitment_binds_body_order_context_parent_and_protocol() {
        let baseline = plan();
        let mut variants = Vec::new();
        let mut body = baseline.clone();
        body.raw_txs[0][0] ^= 1;
        variants.push(body);
        let mut order = baseline.clone();
        order.raw_txs.swap(0, 1);
        order.tx_hashes.swap(0, 1);
        variants.push(order);
        let mut hash = baseline.clone();
        hash.tx_hashes[0][0] ^= 1;
        variants.push(hash);
        let mut timestamp = baseline.clone();
        timestamp.context.timestamp_unix_ms += 1;
        variants.push(timestamp);
        let mut slot = baseline.clone();
        slot.context.slot += 1;
        variants.push(slot);
        let mut chain = baseline.clone();
        chain.context.chain_id += 1;
        variants.push(chain);
        let mut height = baseline.clone();
        height.context.block_height += 1;
        variants.push(height);
        let mut parent_block = baseline.clone();
        parent_block.context.parent_block_hash[0] ^= 1;
        variants.push(parent_block);
        let mut parent_id = baseline.clone();
        parent_id.aoem_parent.as_mut().unwrap().batch_id.push('x');
        variants.push(parent_id);
        let mut parent_result = baseline.clone();
        parent_result.aoem_parent.as_mut().unwrap().batch_result_id = "05".repeat(32);
        variants.push(parent_result);
        let mut parent_receipt = baseline.clone();
        parent_receipt
            .aoem_parent
            .as_mut()
            .unwrap()
            .cumulative_receipt_root[0] ^= 1;
        variants.push(parent_receipt);
        let mut parent_version = baseline.clone();
        parent_version.aoem_parent.as_mut().unwrap().state_version += 1;
        variants.push(parent_version);
        let mut state = baseline.clone();
        state.pre_state_root[0] ^= 1;
        state.aoem_parent.as_mut().unwrap().state_root = state.pre_state_root;
        variants.push(state);
        let mut protocol = baseline.clone();
        protocol.protocol_config_commitment[0] ^= 1;
        variants.push(protocol);
        let mut no_parent = baseline.clone();
        no_parent.aoem_parent = None;
        variants.push(no_parent);
        for value in variants {
            assert!(value.validate().is_err());
            assert_ne!(
                resign(value).unwrap().plan_commitment,
                baseline.plan_commitment
            );
        }
    }

    #[test]
    fn structural_bounds_and_parent_bindings_are_fail_closed() {
        let baseline = plan();
        let mut invalid = Vec::new();
        let mut empty = baseline.clone();
        empty.tx_hashes.clear();
        empty.raw_txs.clear();
        invalid.push(empty);
        let mut alignment = baseline.clone();
        alignment.raw_txs.pop();
        invalid.push(alignment);
        let mut duplicate = baseline.clone();
        duplicate.tx_hashes[1] = duplicate.tx_hashes[0];
        invalid.push(duplicate);
        let mut raw = baseline.clone();
        raw.raw_txs[0].clear();
        invalid.push(raw);
        let mut large = baseline.clone();
        large.raw_txs[0] = vec![1; NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1];
        invalid.push(large);
        let mut count = baseline.clone();
        count.tx_hashes = vec![[6; 32]; NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1 + 1];
        count.raw_txs = vec![vec![1]; count.tx_hashes.len()];
        invalid.push(count);
        let mut protocol = baseline.clone();
        protocol.protocol_config_commitment = [0; 32];
        invalid.push(protocol);
        let mut context = baseline.clone();
        context.context.chain_id = 0;
        invalid.push(context);
        let mut schema = baseline.clone();
        schema.schema.push('2');
        invalid.push(schema);
        let mut parent = baseline.clone();
        parent.aoem_parent.as_mut().unwrap().state_root = [4; 32];
        invalid.push(parent);
        let mut codec = baseline.clone();
        codec.aoem_parent.as_mut().unwrap().receipt_root_codec = "other".into();
        invalid.push(codec);
        let mut version = baseline.clone();
        version.aoem_parent.as_mut().unwrap().state_version = 0;
        invalid.push(version);
        let mut id = baseline.clone();
        id.aoem_parent.as_mut().unwrap().batch_id = " trailing ".into();
        invalid.push(id);
        let mut result = baseline.clone();
        result.aoem_parent.as_mut().unwrap().batch_result_id = "AB".repeat(32);
        invalid.push(result);
        for value in invalid {
            assert!(resign(value).is_err());
        }
        let mut at_limit = baseline;
        at_limit.raw_txs = vec![
            vec![1; NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1 - 1],
            vec![2],
        ];
        resign(at_limit).unwrap();
    }

    #[test]
    fn unknown_json_fields_are_rejected_at_every_plan_level() {
        let value = serde_json::to_value(plan()).unwrap();
        for location in ["", "context", "aoem_parent"] {
            let mut invalid = value.clone();
            let object = if location.is_empty() {
                &mut invalid
            } else {
                &mut invalid[location]
            };
            object["authority_namespace"] = serde_json::json!("peer-selected");
            assert!(serde_json::from_value::<NovNativeCandidateExecutionPlanV1>(invalid).is_err());
        }
    }

    #[test]
    fn mismatched_materialized_inputs_fail_without_claiming_output_verification() {
        let value = plan();
        let mut staged = prepared(&value);
        staged.raw_txs[0][0] ^= 1;
        assert!(value.validate_against_prepared(&staged).is_err());
        staged = prepared(&value);
        staged.context_commitment[0] ^= 1;
        assert!(value.validate_against_prepared(&staged).is_err());
        let mut artifact = block(&value);
        artifact.body.tx_hashes.swap(0, 1);
        assert!(value.validate_against_block(&artifact).is_err());
        artifact = block(&value);
        artifact.header.timestamp_unix_ms += 1;
        assert!(value.validate_against_block(&artifact).is_err());
        artifact = block(&value);
        artifact.header.post_state_root = [99; 32];
        // Input matching deliberately makes no claim about this result field.
        value.validate_against_block(&artifact).unwrap();
        assert!(!artifact.header.aoem_readback_verified);
        assert!(!artifact.header.finalized);
    }
}
