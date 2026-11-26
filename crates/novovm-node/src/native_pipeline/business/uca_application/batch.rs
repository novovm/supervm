//! Typed application branch of the native business contract. Not a publisher.
use super::*;
use crate::native_pipeline::{
    business::{direct_nov_fee::DirectNovFeePolicy, nov_transfer_batch, uca_registration},
    execution::plan::{
        executed_statement_commitment, ordered_receipt_commitment, BatchContext, BatchPlan,
        BoundBatchCapture, OwnedBatchInput, PlanBudget, UnpublishedBatchEffects,
    },
    state::{
        frontier::{CaptureBudget, CaptureStep},
        tree::StateNodeReader,
    },
};
use novovm_exec::resident::{ComputeSession, ComputeTask};
use novovm_protocol::uca_application::ApplicationGrantReceiptV1;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::Duration,
};

pub const APPLICATION_SEMANTIC_VERSION_V1: u32 = 4;

// Explicit initialization approvals select these composite pins. They bind
// application and asset semantics without relabelling old packets.
pub fn application_program_id_v1() -> NodeHash {
    Sha256::digest(b"novovm/uca/native-asset-and-application/v4").into()
}
pub fn application_receipt_codec_v1() -> NodeHash {
    Sha256::digest(b"novovm/uca/native-asset-and-application/receipt/v4").into()
}
pub fn application_effect_contract_v1(policy: &DirectNovFeePolicy) -> Result<NodeHash> {
    let mut hash = Sha256::new();
    hash.update(b"novovm/uca/native-asset-and-application/effects/v4\0");
    hash.update(nov_transfer_batch::uca_effect_contract(policy)?);
    hash.update(application_program_id_v1());
    hash.update(application_receipt_codec_v1());
    hash.update(b"application:NUAP1-NUAR1;MLDSA65-current-initialized-identity;distinct-grants-per-batch;atomic-all-or-reject;revision-CAS;revoke-tombstone;only-application-pages-writable;no-asset-debit\0");
    Ok(hash.finalize().into())
}

/// Private verified commands and compiler-derived access. Preparation reads no state.
pub struct ApplicationGrantBodyV1 {
    checked: Vec<CheckedApplicationGrantV1>,
    raw: Vec<Vec<u8>>,
    accesses: Vec<ApplicationGrantAccessV1>,
    declarations: Vec<DeclaredAccess>,
    policy: DirectNovFeePolicy,
    budget: PlanBudget,
}
impl ApplicationGrantBodyV1 {
    pub fn prepare(
        signed: Vec<novovm_protocol::uca_application::SignedApplicationGrantV1>,
        facts: &[novovm_exec::uca_application::ApplicationGrantSignatureFactV1],
        policy: &DirectNovFeePolicy,
        budget: PlanBudget,
    ) -> Result<Self> {
        ensure!(
            !signed.is_empty()
                && signed.len() <= budget.transactions
                && signed.len() <= novovm_exec::pq_signature::MLDSA_VERIFY_BATCH_MAX_ITEMS
                && signed.len() == facts.len(),
            "application body count exceeds budget or facts"
        );
        let mut checked = Vec::with_capacity(signed.len());
        let mut raw = Vec::with_capacity(signed.len());
        let mut accesses = Vec::with_capacity(signed.len());
        let mut declarations = BTreeMap::<Vec<u8>, DeclaredAccess>::new();
        let mut grants = BTreeSet::new();
        let mut bytes = 0usize;
        for (signed, fact) in signed.into_iter().zip(facts) {
            let encoded = signed.encode()?;
            bytes = bytes
                .checked_add(encoded.len())
                .context("application body bytes overflow")?;
            ensure!(
                encoded.len() <= budget.transaction_bytes && bytes <= budget.body_bytes,
                "application body byte budget exceeded"
            );
            let command = fact.reattach(signed)?;
            ensure!(
                grants.insert(command.intent().selector.grant_id()?),
                "application batch contains repeated grant selector"
            );
            let access = ApplicationGrantAccessV1::new(command.intent().selector.clone())?;
            for declaration in access.declarations() {
                declarations
                    .entry(declaration.key.clone())
                    .and_modify(|existing| {
                        existing.may_put |= declaration.may_put;
                        existing.may_delete |= declaration.may_delete;
                    })
                    .or_insert_with(|| declaration.clone());
            }
            ensure!(
                declarations.len() <= budget.access_keys
                    && declarations.len()
                        <= crate::native_pipeline::state::tree::MAX_BATCH_READ_KEYS,
                "application access budget exceeded"
            );
            raw.push(encoded);
            checked.push(command);
            accesses.push(access);
        }
        Ok(Self {
            checked,
            raw,
            accesses,
            declarations: declarations.into_values().collect(),
            policy: policy.clone(),
            budget,
        })
    }

    pub fn bind(self, context: BatchContext) -> Result<ApplicationGrantPlanV1> {
        ensure!(
            if uca_registration::is_profile(&context) {
                context.effect_contract == uca_registration::effect_contract(&self.policy)?
            } else {
                context.business_program == application_program_id_v1()
                    && context.semantic_version == APPLICATION_SEMANTIC_VERSION_V1
                    && context.receipt_codec == application_receipt_codec_v1()
                    && context.effect_contract == application_effect_contract_v1(&self.policy)?
            },
            "application native business contract mismatch"
        );
        ensure!(
            self.checked.iter().all(|checked| {
                checked.intent().selector.chain_id == context.chain_id
                    && checked.intent().selector.genesis == context.genesis_config_commitment
            }),
            "application body domain mismatch"
        );
        Ok(ApplicationGrantPlanV1 {
            plan: BatchPlan::new(context, self.raw, self.declarations, self.budget)?,
            checked: self.checked,
            accesses: self.accesses,
        })
    }
}

pub struct ApplicationGrantPlanV1 {
    plan: BatchPlan,
    checked: Vec<CheckedApplicationGrantV1>,
    accesses: Vec<ApplicationGrantAccessV1>,
}
impl ApplicationGrantPlanV1 {
    pub fn plan(&self) -> &BatchPlan {
        &self.plan
    }
    pub fn capture(
        self,
        reader: &dyn StateNodeReader,
        budget: CaptureBudget,
    ) -> Result<ApplicationGrantInputV1> {
        Ok(ApplicationGrantInputV1 {
            input: self.plan.capture(reader, budget)?,
            checked: self.checked,
            accesses: self.accesses,
        })
    }
    pub fn begin_capture(self, budget: CaptureBudget) -> Result<ApplicationGrantCaptureV1> {
        Ok(ApplicationGrantCaptureV1 {
            capture: self.plan.begin_capture(budget)?,
            checked: self.checked,
            accesses: self.accesses,
        })
    }
}

pub struct ApplicationGrantCaptureV1 {
    capture: BoundBatchCapture,
    checked: Vec<CheckedApplicationGrantV1>,
    accesses: Vec<ApplicationGrantAccessV1>,
}
impl ApplicationGrantCaptureV1 {
    pub(crate) fn context(&self) -> &BatchContext {
        self.capture.context()
    }
    pub(crate) fn seed_hits(&self) -> usize {
        self.capture.seed_hits()
    }
    pub(crate) fn advance_with_seed(
        &mut self,
        steps: usize,
        seed: Option<&crate::native_pipeline::state::frontier::StateReadSeed>,
    ) -> Result<CaptureStep> {
        self.capture.advance_with_seed(steps, seed)
    }
    pub fn advance(&mut self, steps: usize) -> Result<CaptureStep> {
        self.capture.advance(steps)
    }
    pub fn next_request(&mut self) -> Result<Option<Vec<NodeHash>>> {
        self.capture.next_request()
    }
    pub fn accept(&mut self, values: Vec<Option<Vec<u8>>>) -> Result<()> {
        self.capture.accept(values)
    }
    pub fn finish(self) -> Result<ApplicationGrantInputV1> {
        Ok(ApplicationGrantInputV1 {
            input: self.capture.finish()?,
            checked: self.checked,
            accesses: self.accesses,
        })
    }
}

pub struct ApplicationGrantInputV1 {
    input: OwnedBatchInput,
    checked: Vec<CheckedApplicationGrantV1>,
    accesses: Vec<ApplicationGrantAccessV1>,
}
impl ApplicationGrantInputV1 {
    pub fn plan(&self) -> &BatchPlan {
        self.input.plan()
    }

    /// Execute inside the original AOEM callback owner, not the control loop.
    /// A rejected row returns no executed capability and stages no partial effects.
    pub fn execute(
        self,
        session: &mut ComputeSession,
        timeout: Duration,
    ) -> Result<ExecutedApplicationGrantBatchV1> {
        let commitment = self.input.plan().commitment();
        let mut rejection = Sha256::new();
        rejection.update(b"novovm/uca/application-execution-rejected/v1\0");
        rejection.update(commitment);
        let rejection: NodeHash = rejection.finalize().into();
        let output = Arc::new(Mutex::new(None));
        let target = Arc::clone(&output);
        let task: ComputeTask = Box::new(move || {
            // A business rejection is a completed decision, not a failed AOEM
            // callback. Panics/runtime failures still poison the original owner.
            let result = self.finish();
            let commitment = result
                .as_ref()
                .map_or(rejection, |result| result.statement_commitment);
            *target
                .lock()
                .map_err(|_| anyhow::anyhow!("application output poisoned"))? = Some(result);
            Ok(commitment.to_vec())
        });
        let report = session.execute(vec![task], timeout)?;
        let result = output
            .lock()
            .map_err(|_| anyhow::anyhow!("application output poisoned"))?
            .take()
            .context("application execution produced no result")?;
        ensure!(
            report.processed == 1 && report.failed == 0 && report.outputs.len() == 1,
            "application AOEM output/plan commitment mismatch"
        );
        match result {
            Ok(result) => {
                ensure!(
                    report.outputs[0].as_slice() == result.statement_commitment
                        && result.effects.plan_commitment() == commitment,
                    "application executed output commitment mismatch"
                );
                Ok(result)
            }
            Err(error) => {
                ensure!(
                    report.outputs[0].as_slice() == rejection,
                    "application rejection output commitment mismatch"
                );
                Err(error)
            }
        }
    }

    fn finish(self) -> Result<ExecutedApplicationGrantBatchV1> {
        let context = *self.input.plan().context();
        let mut changes = Vec::new();
        let mut receipts = Vec::with_capacity(self.checked.len());
        for (access, checked) in self.accesses.iter().zip(&self.checked) {
            let state = access.read_under_rules(
                self.input.state_input(),
                context.parent_state_root,
                context.parent_height,
                uca_registration::is_profile(&context),
            )?;
            let record = access.transition_record(
                &state,
                checked,
                context.parent_state_root,
                context.height,
            )?;
            let receipt = ApplicationGrantReceiptV1 {
                command_id: application_command_id(checked.intent())?,
                grant_id: record.grant_id,
                issuer_key_ref: record.issuer_key_ref,
                previous_revision: state.revision(),
                revision: record.revision,
                revoked: record.revoked,
                not_before_height: record.not_before_height,
                expires_at_height: record.expires_at_height,
            };
            receipt.validate()?;
            changes.extend(record_pages::record_changes(
                &access.prefix,
                &postcard::to_allocvec(&record)?,
                GRANT_BYTES,
            )?);
            receipts.push(receipt);
        }
        let encoded = receipts
            .iter()
            .map(ApplicationGrantReceiptV1::encode)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let receipt_batch_commitment = ordered_receipt_commitment(context.receipt_codec, &encoded);
        let effects = self.input.stage(&changes)?;
        let statement_commitment = executed_statement_commitment(
            effects.plan_commitment(),
            effects.update().root(),
            receipt_batch_commitment,
        );
        Ok(ExecutedApplicationGrantBatchV1 {
            effects,
            receipts,
            receipt_batch_commitment,
            statement_commitment,
        })
    }
}

pub(crate) fn application_command_id(
    intent: &novovm_protocol::uca_application::ApplicationGrantIntentV1,
) -> Result<NodeHash> {
    let mut digest = Sha256::new();
    digest.update(b"novovm/uca/application-command/v1\0");
    digest.update(intent.signing_bytes()?);
    Ok(digest.finalize().into())
}

/// Created only by the complete pinned compiler, never from caller StateChange.
/// This is tentative execution, not a packet, durable grant or finality proof.
///
/// ```compile_fail
/// use novovm_node::native_pipeline::business::uca_application::ExecutedApplicationGrantBatchV1;
/// use novovm_node::native_pipeline::execution::plan::UnpublishedBatchEffects;
/// fn forge(effects: UnpublishedBatchEffects) -> ExecutedApplicationGrantBatchV1 {
///     ExecutedApplicationGrantBatchV1 {
///         effects, receipts: vec![], receipt_batch_commitment: [1; 32], statement_commitment: [2; 32]
///     }
/// }
/// ```
pub struct ExecutedApplicationGrantBatchV1 {
    effects: UnpublishedBatchEffects,
    receipts: Vec<ApplicationGrantReceiptV1>,
    receipt_batch_commitment: NodeHash,
    statement_commitment: NodeHash,
}
impl ExecutedApplicationGrantBatchV1 {
    pub fn effects(&self) -> &UnpublishedBatchEffects {
        &self.effects
    }
    pub fn receipts(&self) -> &[ApplicationGrantReceiptV1] {
        &self.receipts
    }
    pub fn receipt_batch_commitment(&self) -> NodeHash {
        self.receipt_batch_commitment
    }
    pub fn statement_commitment(&self) -> NodeHash {
        self.statement_commitment
    }
}

#[cfg(test)]
mod tests;
