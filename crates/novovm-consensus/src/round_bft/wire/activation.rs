//! Explicit validator consent for a business-rule activation on the existing chain.
use super::{Context, Hash, ValidatorSet, MLDSA65_SIGNATURE_BYTES};
use anyhow::{ensure, Context as _, Result};
use serde::{Deserialize, Serialize};

const DOMAIN: &[u8] = b"novovm-round-bft/v2/mldsa65/business-activation/v1\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessRules {
    pub program: Hash,
    pub semantic_version: u32,
    pub effects: Hash,
    pub receipts: Hash,
}

impl BusinessRules {
    fn append(self, bytes: &mut Vec<u8>) -> Result<()> {
        ensure!(
            self.semantic_version != 0
                && [self.program, self.effects, self.receipts]
                    .iter()
                    .all(|pin| *pin != [0; 32]),
            "invalid business rules"
        );
        bytes.extend_from_slice(&self.program);
        bytes.extend_from_slice(&self.semantic_version.to_be_bytes());
        bytes.extend_from_slice(&self.effects);
        bytes.extend_from_slice(&self.receipts);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessActivation {
    /// Existing vote domain for the first block under the new rules.
    pub context: Context,
    pub parent_state_root: Hash,
    pub parent_receipt_root: Hash,
    pub parent_state_version: u64,
    pub previous: BusinessRules,
    pub next: BusinessRules,
}

impl BusinessActivation {
    pub fn signing_bytes(self) -> Result<Vec<u8>> {
        let context = self.context;
        ensure!(
            context.chain_id != 0
                && context.epoch != 0
                && context.height > 1
                && [
                    context.genesis_config_commitment,
                    context.protocol_commitment,
                    context.validator_set_hash,
                    context.parent_block_hash,
                    context.parent_decision_hash,
                    self.parent_state_root,
                    self.parent_receipt_root
                ]
                .iter()
                .all(|pin| *pin != [0; 32])
                && self.next.semantic_version > self.previous.semantic_version,
            "invalid existing-chain activation anchor or rule versions"
        );
        let mut bytes = DOMAIN.to_vec();
        bytes.extend_from_slice(&context.chain_id.to_be_bytes());
        bytes.extend_from_slice(&context.genesis_config_commitment);
        bytes.extend_from_slice(&context.protocol_commitment);
        bytes.extend_from_slice(&context.epoch.to_be_bytes());
        bytes.extend_from_slice(&context.validator_set_hash);
        bytes.extend_from_slice(&context.height.to_be_bytes());
        bytes.extend_from_slice(&context.parent_block_hash);
        bytes.extend_from_slice(&context.parent_decision_hash);
        bytes.extend_from_slice(&self.parent_state_root);
        bytes.extend_from_slice(&self.parent_receipt_root);
        bytes.extend_from_slice(&self.parent_state_version.to_be_bytes());
        self.previous.append(&mut bytes)?;
        self.next.append(&mut bytes)?;
        Ok(bytes)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationApproval {
    pub signer: Hash,
    pub signature: Vec<u8>,
}

/// Quorum consent only. The caller must independently pin the canonical parent
/// and enforce activation and recovery before accepting an upgraded block.
pub struct VerifiedBusinessActivation(BusinessActivation);

impl VerifiedBusinessActivation {
    pub fn verify(
        expected: BusinessActivation,
        approvals: &[ActivationApproval],
        set: &ValidatorSet,
    ) -> Result<Self> {
        let context = expected.context;
        ensure!(
            context.chain_id == set.chain_id()
                && context.epoch == set.epoch()
                && context.validator_set_hash == set.hash()
                && context.height >= set.activation_height(),
            "activation does not use the pinned active validator set"
        );
        ensure!(
            !approvals.is_empty() && approvals.len() <= set.members().len(),
            "activation approval count outside validator set"
        );
        let message = expected.signing_bytes()?;
        let mut weight = 0u64;
        let mut previous = None;
        for approval in approvals {
            ensure!(
                previous.is_none_or(|last| last < approval.signer),
                "activation approvers must be distinct and sorted"
            );
            previous = Some(approval.signer);
            ensure!(
                approval.signature.len() == MLDSA65_SIGNATURE_BYTES,
                "invalid activation signature length"
            );
            let member = set
                .member(&approval.signer)
                .context("unknown activation approver")?;
            set.verify_signature(member.public_key(), &message, &approval.signature)?;
            weight = weight
                .checked_add(member.weight())
                .context("activation weight overflow")?;
        }
        ensure!(
            weight >= set.quorum_weight(),
            "activation has insufficient validator weight"
        );
        Ok(Self(expected))
    }

    pub fn subject(&self) -> &BusinessActivation {
        &self.0
    }
}

#[cfg(test)]
mod tests;
