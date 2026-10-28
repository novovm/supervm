//! Migrated resident batch input contract, not an upgrade of the public block
//! or transaction codec. Raw bytes and access declarations remain UNVERIFIED:
//! signature/identity/nonce checking and deriving complete effects from the
//! pinned business program are separate, mandatory admission work.
//!
//! No caller-supplied transaction hashes, cached authority, storage path, worker
//! count, or candidate slot enters the statement. Local budgets do not affect
//! its identity. Constructing a plan never allocates a persistent candidate.

use crate::native_pipeline::ingress::batch::BatchSource;
use crate::native_pipeline::state::frontier::{
    BulkCapture, CaptureBudget, CaptureStep, DeclaredAccess, OwnedStateInput,
};
use crate::native_pipeline::state::tree::{
    self, NodeHash, StagedStateUpdate, StateChange, StateNodeReader,
};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::BTreeSet;

const PLAN_DOMAIN: &[u8] = b"novovm-replacement-batch-plan/v1\0";
const WIRE_DOMAIN: &[u8] = b"novovm-replacement-raw-wire/v1\0";

/// Claimed execution domain. The eventual ingress/compiler must compare it
/// with the configured chain and installed program, not trust peer values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchContext {
    pub chain_id: u64,
    pub genesis_config_commitment: NodeHash,
    pub protocol_commitment: NodeHash,
    pub business_program: NodeHash,
    pub semantic_version: u32,
    /// Complete effect/fee/nonce semantics, not a pre-settlement result label.
    pub effect_contract: NodeHash,
    pub parent_block_hash: NodeHash,
    pub parent_height: u64,
    pub parent_state_root: NodeHash,
    pub parent_receipt_root: NodeHash,
    pub parent_state_version: u64,
    pub receipt_codec: NodeHash,
    pub height: u64,
    pub slot: u64,
    pub timestamp_unix_ms: u64,
}

impl BatchContext {
    fn validate(&self) -> Result<()> {
        if self.chain_id == 0 || self.semantic_version == 0 {
            bail!("batch chain and semantic version must be nonzero");
        }
        if self.height
            != self
                .parent_height
                .checked_add(1)
                .context("batch parent height exhausted")?
        {
            bail!("batch height must follow its claimed parent");
        }
        // Preserve the existing first-block convention; the independently
        // pinned genesis configuration and state roots bind its actual anchor.
        if (self.parent_height == 0) != (self.parent_block_hash == [0; 32]) {
            bail!("batch first-block parent hash convention mismatch");
        }
        for hash in [
            self.genesis_config_commitment,
            self.protocol_commitment,
            self.business_program,
            self.effect_contract,
            self.parent_state_root,
            self.parent_receipt_root,
            self.receipt_codec,
        ] {
            if hash == [0; 32] {
                bail!("batch domain/root commitment must be nonzero");
            }
        }
        Ok(())
    }

    fn commit(&self, digest: &mut Sha256) -> Result<()> {
        digest.update(self.chain_id.to_be_bytes());
        digest.update(self.genesis_config_commitment);
        digest.update(self.protocol_commitment);
        digest.update(self.business_program);
        digest.update(self.semantic_version.to_be_bytes());
        digest.update(self.effect_contract);
        framed(digest, tree::STATE_TREE_CODEC_V1.as_bytes())?;
        digest.update(self.parent_block_hash);
        digest.update(self.parent_height.to_be_bytes());
        digest.update(self.parent_state_root);
        digest.update(self.parent_receipt_root);
        digest.update(self.parent_state_version.to_be_bytes());
        digest.update(self.receipt_codec);
        digest.update(self.height.to_be_bytes());
        digest.update(self.slot.to_be_bytes());
        digest.update(self.timestamp_unix_ms.to_be_bytes());
        Ok(())
    }
}

/// Per-request local admission bounds, not a production protocol choice.
#[derive(Clone, Copy)]
pub struct PlanBudget {
    pub transactions: usize,
    pub transaction_bytes: usize,
    pub body_bytes: usize,
    pub access_keys: usize,
}

/// Immutable structural plan; intentionally not named AuthenticatedBatch.
/// No Deserialize or writable accessors can substitute a body after capture.
pub struct BatchPlan {
    context: BatchContext,
    source: BatchSource,
    canonical_body_bytes: usize,
    max_transaction_bytes: usize,
    declared_access: Vec<DeclaredAccess>,
    commitment: NodeHash,
}

impl BatchPlan {
    pub fn new(
        context: BatchContext,
        raw_transactions: Vec<Vec<u8>>,
        declared_access: Vec<DeclaredAccess>,
        budget: PlanBudget,
    ) -> Result<Self> {
        Self::from_source(
            context,
            BatchSource::Raw(raw_transactions),
            declared_access,
            budget,
        )
    }

    pub(crate) fn from_source(
        context: BatchContext,
        source: BatchSource,
        mut declared_access: Vec<DeclaredAccess>,
        budget: PlanBudget,
    ) -> Result<Self> {
        context.validate()?;
        if source.is_empty() || source.len() > budget.transactions {
            bail!("batch transaction budget exceeded or empty");
        }
        if declared_access.is_empty() || declared_access.len() > budget.access_keys {
            bail!("batch access budget exceeded or empty");
        }
        let mut total = 0usize;
        let mut max_transaction_bytes = 0usize;
        let mut seen = BTreeSet::new();
        for index in 0..source.len() {
            let len = match &source {
                BatchSource::Raw(raw) => raw[index].len(),
                BatchSource::Apfl(batch) => batch.row(index)?.encoded_len()?,
                BatchSource::Uca(batch) => batch.raw_transaction(index)?.len(),
            };
            if len == 0 || len > budget.transaction_bytes {
                bail!("batch raw transaction size exceeds budget or empty");
            }
            total = total.checked_add(len).context("batch body overflow")?;
            max_transaction_bytes = max_transaction_bytes.max(len);
            if total > budget.body_bytes {
                bail!("batch body byte budget exceeded");
            }
            let mut digest = Sha256::new();
            digest.update(WIRE_DOMAIN);
            framed_source_row(&mut digest, &source, index)?;
            // A wire-content digest, NOT the protocol's canonical transaction ID.
            let wire_digest: NodeHash = digest.finalize().into();
            if !seen.insert(wire_digest) {
                bail!("batch contains duplicate raw transaction bytes");
            }
        }
        context
            .parent_state_version
            .checked_add(u64::try_from(source.len())?)
            .context("batch state version exhausted")?;
        for access in &declared_access {
            tree::state_key_hash(&access.key)?;
        }
        declared_access.sort_by(|left, right| left.key.cmp(&right.key));
        if declared_access
            .windows(2)
            .any(|pair| pair[0].key == pair[1].key)
        {
            bail!("batch declared access keys must be unique");
        }
        let mut digest = Sha256::new();
        digest.update(PLAN_DOMAIN);
        context.commit(&mut digest)?;
        digest.update(u64::try_from(source.len())?.to_be_bytes());
        for index in 0..source.len() {
            framed_source_row(&mut digest, &source, index)?;
        }
        digest.update(u64::try_from(declared_access.len())?.to_be_bytes());
        for access in &declared_access {
            framed(&mut digest, &access.key)?;
            digest.update([u8::from(access.may_put) | (u8::from(access.may_delete) << 1)]);
        }
        Ok(Self {
            context,
            source,
            canonical_body_bytes: total,
            max_transaction_bytes,
            declared_access,
            commitment: digest.finalize().into(),
        })
    }

    pub fn context(&self) -> &BatchContext {
        &self.context
    }

    pub fn commitment(&self) -> NodeHash {
        self.commitment
    }

    pub fn transaction_count(&self) -> usize {
        self.source.len()
    }

    pub fn canonical_body_bytes(&self) -> usize {
        self.canonical_body_bytes
    }

    pub fn max_transaction_bytes(&self) -> usize {
        self.max_transaction_bytes
    }

    /// Explicit codec-boundary projection of ONE row, never a whole-batch
    /// expansion. Raw archive bytes stay borrowed; APFL remains shared.
    pub fn canonical_transaction(&self, index: usize) -> Result<Cow<'_, [u8]>> {
        match &self.source {
            BatchSource::Raw(raw) => Ok(Cow::Borrowed(
                raw.get(index)
                    .context("batch transaction index out of bounds")?,
            )),
            BatchSource::Apfl(batch) => Ok(Cow::Owned(batch.canonical_raw(index)?)),
            BatchSource::Uca(batch) => Ok(Cow::Owned(batch.raw_transaction(index)?)),
        }
    }

    /// Decoded cold archives already own their raw records. Never materializes
    /// a structural batch merely to satisfy a legacy slice consumer.
    pub fn raw_transactions_if_materialized(&self) -> Option<&[Vec<u8>]> {
        match &self.source {
            BatchSource::Raw(raw) => Some(raw),
            BatchSource::Apfl(_) | BatchSource::Uca(_) => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn canonical_transactions_for_test(&self) -> Vec<Vec<u8>> {
        (0..self.transaction_count())
            .map(|index| self.canonical_transaction(index).unwrap().into_owned())
            .collect()
    }

    /// Immutable compiler-derived access declarations, retained with the exact
    /// body so durable candidate packets cannot substitute either after execution.
    pub(crate) fn declared_access(&self) -> &[DeclaredAccess] {
        &self.declared_access
    }

    /// The source must serve an independently trusted parent. This binds the
    /// exact plan root AND declarations; arbitrary prebuilt witnesses cannot
    /// be attached. It does not check current-parent/round/signing eligibility.
    pub fn capture(
        self,
        source: &dyn StateNodeReader,
        budget: CaptureBudget,
    ) -> Result<OwnedBatchInput> {
        let state = OwnedStateInput::capture(
            source,
            self.context.parent_state_root,
            &self.declared_access,
            budget,
        )?;
        Ok(OwnedBatchInput { plan: self, state })
    }

    /// Proof-only import still binds the witness to this exact compiled plan.
    /// Neither a claimed root nor wire permissions can replace that binding.
    pub(crate) fn capture_witness(
        self,
        wire: &[u8],
        budget: CaptureBudget,
    ) -> Result<OwnedBatchInput> {
        let state = OwnedStateInput::from_witness(
            self.context.parent_state_root,
            &self.declared_access,
            wire,
            budget,
        )?;
        Ok(OwnedBatchInput { plan: self, state })
    }

    /// Start incremental bulk capture bound to this exact plan. The returned
    /// state machine owns the plan; no caller can attach a substitute frontier,
    /// body, parent or wider declarations when finishing.
    pub fn begin_capture(self, budget: CaptureBudget) -> Result<BoundBatchCapture> {
        let capture = BulkCapture::new(
            self.context.parent_state_root,
            &self.declared_access,
            budget,
        )?;
        Ok(BoundBatchCapture {
            plan: self,
            capture,
        })
    }
}

pub struct BoundBatchCapture {
    plan: BatchPlan,
    capture: BulkCapture,
}

impl BoundBatchCapture {
    pub(crate) fn seed_hits(&self) -> usize {
        self.capture.seed_hits()
    }

    pub(crate) fn context(&self) -> &BatchContext {
        self.plan.context()
    }

    pub(crate) fn advance_with_seed(
        &mut self,
        max_edge_steps: usize,
        seed: Option<&crate::native_pipeline::state::frontier::StateReadSeed>,
    ) -> Result<CaptureStep> {
        self.capture.advance_with_seed(max_edge_steps, seed)
    }

    pub fn advance(&mut self, max_edge_steps: usize) -> Result<CaptureStep> {
        self.capture.advance(max_edge_steps)
    }
    pub fn next_request(&mut self) -> Result<Option<Vec<NodeHash>>> {
        self.capture.next_request()
    }
    pub fn accept(&mut self, values: Vec<Option<Vec<u8>>>) -> Result<()> {
        self.capture.accept(values)
    }
    pub fn finish(self) -> Result<OwnedBatchInput> {
        Ok(OwnedBatchInput {
            plan: self.plan,
            state: self.capture.finish()?,
        })
    }
}

/// Detached computation input. No storage handle/path, borrowed guard, signer,
/// live publication permission or caller-replaceable plan/witness pair.
pub struct OwnedBatchInput {
    plan: BatchPlan,
    state: OwnedStateInput,
}

impl OwnedBatchInput {
    pub(crate) fn state_input(&self) -> &OwnedStateInput {
        &self.state
    }
    pub fn plan(&self) -> &BatchPlan {
        &self.plan
    }

    pub fn read(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.state.read(key)
    }

    pub(crate) fn read_many(&self, keys: &[Vec<u8>]) -> Result<Vec<Option<Vec<u8>>>> {
        self.state.read_many(keys)
    }

    pub(crate) fn encode_witness(&self, budget: CaptureBudget) -> Result<Vec<u8>> {
        self.state.encode_witness(budget)
    }

    /// Record derived effects without claiming their business validity. The
    /// pinned business program must compute and validate them in the execution
    /// layer. This constructor cannot produce a durable/finalized certificate.
    pub fn stage(self, changes: &[StateChange]) -> Result<UnpublishedBatchEffects> {
        let update = self.state.stage(changes)?;
        Ok(UnpublishedBatchEffects {
            plan: self.plan,
            update,
            input: self.state,
        })
    }
}

/// Plan-bound tentative tree effects only. No successful receipt, economic
/// settlement, AOEM evidence or finality flag is fabricated by this module.
pub struct UnpublishedBatchEffects {
    plan: BatchPlan,
    update: StagedStateUpdate,
    // Move the actual captured witness through packaging; never recapture or
    // clone a complete frontier just to create a read-locality hint.
    input: OwnedStateInput,
}

impl UnpublishedBatchEffects {
    pub(crate) fn into_poststate_seed(
        self,
        budget: CaptureBudget,
    ) -> Result<Option<crate::native_pipeline::state::frontier::StateReadSeed>> {
        self.input.into_poststate_seed(self.update, budget)
    }

    pub fn plan_commitment(&self) -> NodeHash {
        self.plan.commitment
    }

    pub fn context(&self) -> &BatchContext {
        &self.plan.context
    }

    pub(crate) fn plan(&self) -> &BatchPlan {
        &self.plan
    }

    pub fn update(&self) -> &StagedStateUpdate {
        &self.update
    }
}

fn framed(digest: &mut Sha256, bytes: &[u8]) -> Result<()> {
    digest.update(u64::try_from(bytes.len())?.to_be_bytes());
    digest.update(bytes);
    Ok(())
}

fn framed_source_row(digest: &mut Sha256, source: &BatchSource, index: usize) -> Result<()> {
    match source {
        BatchSource::Raw(raw) => framed(digest, &raw[index]),
        BatchSource::Apfl(batch) => {
            let row = batch.row(index)?;
            digest.update(u64::try_from(row.encoded_len()?)?.to_be_bytes());
            row.update_canonical_digest(digest)
        }
        BatchSource::Uca(batch) => framed(digest, &batch.raw_transaction(index)?),
    }
}

#[cfg(test)]
mod tests;
