//! Replacement-local, authenticated NOV Transfer business compiler.
//!
//! This is a new complete direct-NOV economic effect layout, not the old Store,
//! per-transaction root/seal, or receipt codec. A private compiled input binds
//! the installed program, all resolved fee policy, body and exact parent input.
//! Independent dependency components run on AOEM; the last callback performs
//! ordered settlement/repair and one batch tree update. No host-computed final
//! monetary patch is submitted as if it were parallel business execution.
//!
//! Outputs remain tentative. No persistent admission, validity proof, cumulative
//! receipt tree, canonical publication or finality is established here.

use super::direct_nov_fee::{DirectNovFeePolicy, FeeState, TransferFeeRequest};
use super::quoted_transfer::Account;
use super::record_pages;
use crate::execution::plan::{BatchContext, BatchPlan, PlanBudget};
use crate::ingress::authentication::check_nonce_sequence;
use crate::ingress::batch::{
    SignatureCheckedBatch, SignatureCheckedCapture, SignatureCheckedInput, SignatureCheckedPlan,
};
use crate::state::frontier::{CaptureBudget, CaptureStep, DeclaredAccess};
use crate::state::tree::{NodeHash, StateChange, StateNodeReader};
use anyhow::{ensure, Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

mod execution;
pub use execution::{ExecutedNovBatch, ExecutionObservation, NovTransferReceipt};

const POLICY_PREFIX: &[u8] = b"novovm/replacement/nov-direct/v1/policy";
const FEE_PREFIX: &[u8] = b"novovm/replacement/nov-direct/v1/fees";
const POLICY_BYTES: usize = 2048;
const FEE_BYTES: usize = 8192;
pub const SEMANTIC_VERSION: u32 = 1;

pub fn program_id() -> NodeHash {
    Sha256::digest(b"novovm/replacement/nov-transfer-complete-effects/v1").into()
}

pub fn receipt_codec() -> NodeHash {
    Sha256::digest(b"novovm/replacement/nov-transfer-receipt/postcard/v1").into()
}

/// All effective policy fields, including provenance and TTL, are bound. The
/// old human-readable treasury policy id is not used as a cryptographic pin.
pub fn effect_contract(policy: &DirectNovFeePolicy) -> Result<NodeHash> {
    policy.validate()?;
    let bytes = postcard::to_allocvec(policy)?;
    ensure!(
        bytes.len() <= POLICY_BYTES,
        "fee policy record exceeds bound"
    );
    let mut digest = Sha256::new();
    digest.update(b"novovm/replacement/nov-direct-effects/v1\0");
    digest.update(program_id());
    digest.update(receipt_codec());
    digest.update(b"accounts:u128le;nonce:u64le;paged-postcard:v1;pages:256;policy:2048;fees:8192;quote-transfer-json:v1;fee-journal:receipt-output;direct-only\0");
    digest.update((bytes.len() as u64).to_be_bytes());
    digest.update(bytes);
    Ok(digest.finalize().into())
}

pub fn balance_key(account: &Account) -> Vec<u8> {
    [
        b"novovm/replacement/nov-direct/v1/balance/".as_slice(),
        account.as_bytes(),
    ]
    .concat()
}

pub fn nonce_key(identity: &[u8; 32]) -> Vec<u8> {
    [
        b"novovm/replacement/nov-direct/v1/nonce/".as_slice(),
        identity,
    ]
    .concat()
}

/// Explicit new-profile state construction, not production genesis authority.
/// No existing chain or legacy state is accepted/migrated by this function.
pub fn fee_record_changes(
    policy: &DirectNovFeePolicy,
    state: &FeeState,
) -> Result<Vec<StateChange>> {
    policy.validate()?;
    state.validate()?;
    let mut changes =
        record_pages::record_changes(POLICY_PREFIX, &postcard::to_allocvec(policy)?, POLICY_BYTES)?;
    changes.extend(record_pages::record_changes(
        FEE_PREFIX,
        &postcard::to_allocvec(state)?,
        FEE_BYTES,
    )?);
    Ok(changes)
}

fn decode_record<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T> {
    let (value, rest): (T, &[u8]) = postcard::take_from_bytes(bytes)?;
    ensure!(
        rest.is_empty() && postcard::to_allocvec(&value)? == bytes,
        "noncanonical business record"
    );
    Ok(value)
}

fn is_nov(asset: &str) -> bool {
    asset.trim().is_empty() || asset.trim().eq_ignore_ascii_case("NOV")
}

/// Parent-independent, authenticated NOV business input. This compiler alone
/// derives the typed requests and complete access declarations from the exact
/// signature-checked body. It owns no parent, root, timestamp, state witness,
/// balance/nonce approval, or permission to execute, persist or sign.
///
/// Binding consumes the body; a captured input cannot be relabelled with a new
/// parent. The caller must independently authorize every supplied context.
///
/// ```compile_fail
/// use novovm_host::business::nov_transfer_batch::NovTransferBody;
/// use novovm_host::execution::plan::BatchContext;
/// fn reuse(body: NovTransferBody, context: BatchContext) {
///     let first = body.bind(context);
///     let second = body.bind(context);
/// }
/// ```
///
/// ```compile_fail
/// use novovm_host::business::nov_transfer_batch::NovTransferBody;
/// fn replace_declarations(mut body: NovTransferBody) {
///     body.declarations.clear();
/// }
/// ```
pub struct NovTransferBody {
    batch: SignatureCheckedBatch,
    policy: DirectNovFeePolicy,
    requests: Vec<TransferFeeRequest>,
    declarations: Vec<DeclaredAccess>,
    effect_contract: NodeHash,
    budget: PlanBudget,
}

impl NovTransferBody {
    /// Authentication and canonical transaction deduplication have already
    /// succeeded in SignatureCheckedBatch. Plan limits are checked again here
    /// before retaining compiler-derived work; the structural plan rechecks
    /// them at binding. No parent state is read during preparation.
    pub fn prepare(
        batch: SignatureCheckedBatch,
        policy: DirectNovFeePolicy,
        budget: PlanBudget,
    ) -> Result<Self> {
        let effect_contract = effect_contract(&policy)?;
        let raw = batch.raw_transactions();
        // The same size predicates as BatchPlan::new, evaluated before a
        // parent exists. Its raw-content deduplication is already implied by
        // canonical transaction deduplication in this privately built batch;
        // bind still runs the canonical constructor, including all remaining
        // context-dependent checks. No provisional context is manufactured.
        ensure!(
            !raw.is_empty() && raw.len() <= budget.transactions,
            "batch transaction budget exceeded or empty"
        );
        let mut bytes = 0usize;
        for transaction in raw {
            ensure!(
                !transaction.is_empty() && transaction.len() <= budget.transaction_bytes,
                "batch raw transaction size exceeds budget or empty"
            );
            bytes = bytes
                .checked_add(transaction.len())
                .context("batch body overflow")?;
            ensure!(
                bytes <= budget.body_bytes,
                "batch body byte budget exceeded"
            );
        }
        let mut keys = BTreeSet::new();
        let mut requests = Vec::with_capacity(batch.transactions().len());
        for authenticated in batch.transactions() {
            let tx = authenticated.transfer();
            ensure!(
                is_nov(&tx.asset) && is_nov(&tx.fee_policy.pay_asset),
                "NOV direct profile does not support this asset or fee asset"
            );
            let payer = Account::try_from(tx.from.as_slice()).map_err(anyhow::Error::msg)?;
            let recipient = Account::try_from(tx.to.as_slice()).map_err(anyhow::Error::msg)?;
            keys.insert(balance_key(&payer));
            keys.insert(balance_key(&recipient));
            keys.insert(nonce_key(&authenticated.nonce_identity()));
            requests.push(TransferFeeRequest {
                tx_hash: authenticated.tx_hash(),
                payer,
                recipient,
                asset: tx.asset.clone(),
                amount: tx.amount,
                pay_asset: tx.fee_policy.pay_asset.clone(),
                max_pay_amount: tx.fee_policy.max_pay_amount,
                slippage_bps: tx.fee_policy.slippage_bps,
            });
        }
        let mut declarations: Vec<_> = keys
            .into_iter()
            .map(|key| DeclaredAccess {
                key,
                may_put: true,
                may_delete: false,
            })
            .collect();
        declarations.extend(record_pages::declarations(
            POLICY_PREFIX,
            POLICY_BYTES,
            false,
        )?);
        declarations.extend(record_pages::declarations(FEE_PREFIX, FEE_BYTES, true)?);
        // Preserve the tree's existing bound, not an after-compute surprise.
        ensure!(
            declarations.len() <= 4096,
            "NOV effect key budget exceeds tree bound"
        );
        ensure!(
            declarations.len() <= budget.access_keys,
            "batch access budget exceeded or empty"
        );
        Ok(Self {
            batch,
            policy,
            requests,
            declarations,
            effect_contract,
            budget,
        })
    }

    /// Bind only the immutable body/policy to this exact claimed context. The
    /// existing structural constructor still checks chain, parent shape,
    /// nonzero domain pins, height/version arithmetic, body and access limits.
    /// Successful binding is not validation of the parent's authority or state.
    pub fn bind(self, context: BatchContext) -> Result<NovTransferPlan> {
        ensure!(
            context.business_program == program_id(),
            "unsupported NOV business program"
        );
        ensure!(
            context.semantic_version == SEMANTIC_VERSION,
            "unsupported NOV semantic version"
        );
        ensure!(
            context.receipt_codec == receipt_codec(),
            "unsupported NOV receipt codec"
        );
        ensure!(
            context.effect_contract == self.effect_contract,
            "fee/effect policy commitment mismatch"
        );
        Ok(NovTransferPlan {
            plan: self.batch.bind(context, self.declarations, self.budget)?,
            policy: self.policy,
            requests: self.requests,
        })
    }
}

/// Only this compiler derives access declarations for the complete profile.
/// A caller cannot attach arbitrary declarations, metadata or another policy
/// to the result. Its claimed parent still needs independent chain authority.
pub struct NovTransferPlan {
    plan: SignatureCheckedPlan,
    policy: DirectNovFeePolicy,
    requests: Vec<TransferFeeRequest>,
}

impl NovTransferPlan {
    /// Compatibility entry on the same preparation/binding path. Independent
    /// policy/body errors can now precede context errors; no rejection or
    /// parent-state check is removed or converted into execution authority.
    pub fn compile(
        batch: SignatureCheckedBatch,
        context: BatchContext,
        policy: DirectNovFeePolicy,
        budget: PlanBudget,
    ) -> Result<Self> {
        NovTransferBody::prepare(batch, policy, budget)?.bind(context)
    }

    pub fn commitment(&self) -> NodeHash {
        self.plan.plan().commitment()
    }

    pub fn plan(&self) -> &BatchPlan {
        self.plan.plan()
    }

    pub fn capture(
        self,
        reader: &dyn StateNodeReader,
        budget: CaptureBudget,
    ) -> Result<NovTransferInput> {
        NovCapturedInput {
            input: self.plan.capture(reader, budget)?,
            policy: self.policy,
            requests: self.requests,
        }
        .finalize_capture()
    }

    pub fn begin_capture(self, budget: CaptureBudget) -> Result<NovTransferCapture> {
        Ok(NovTransferCapture {
            capture: self.plan.begin_capture(budget)?,
            policy: self.policy,
            requests: self.requests,
        })
    }

    pub(crate) fn capture_witness(
        self,
        wire: &[u8],
        budget: CaptureBudget,
    ) -> Result<NovTransferInput> {
        NovCapturedInput {
            input: self.plan.capture_witness(wire, budget)?,
            policy: self.policy,
            requests: self.requests,
        }
        .finalize_capture()
    }
}

/// Bounded incremental capture; neither state nor authenticated plan is replaceable.
pub struct NovTransferCapture {
    capture: SignatureCheckedCapture,
    policy: DirectNovFeePolicy,
    requests: Vec<TransferFeeRequest>,
}

impl NovTransferCapture {
    pub fn advance(&mut self, max_edge_steps: usize) -> Result<CaptureStep> {
        self.capture.advance(max_edge_steps)
    }

    pub fn next_request(&mut self) -> Result<Option<Vec<NodeHash>>> {
        self.capture.next_request()
    }

    pub fn accept(&mut self, values: Vec<Option<Vec<u8>>>) -> Result<()> {
        self.capture.accept(values)
    }

    /// Move-only completion. Policy/nonce decoding and dependency compilation
    /// remain on the compute owner, not the network/control loop.
    pub fn finish(self) -> Result<NovCapturedInput> {
        Ok(NovCapturedInput {
            input: self.capture.finish()?,
            policy: self.policy,
            requests: self.requests,
        })
    }
}

/// Detached exact capture, not yet a business-admitted input or an authority.
/// Private construction prevents replacing authenticated raw input/parent.
pub struct NovCapturedInput {
    input: SignatureCheckedInput,
    policy: DirectNovFeePolicy,
    requests: Vec<TransferFeeRequest>,
}

impl NovCapturedInput {
    pub fn plan(&self) -> &BatchPlan {
        self.input.plan()
    }

    /// Export on an input/proof worker, not the consensus poll. This contains
    /// original authenticated parent inputs, not predicted/candidate outputs.
    /// It confers no verification, publication or finality authority.
    pub fn execution_proof_input(&self) -> Result<Vec<u8>> {
        crate::proof::encode_input(
            self.plan(),
            &self.policy,
            &self.input.encode_witness(crate::proof::capture_budget())?,
        )
    }

    pub(crate) fn finalize_capture(self) -> Result<NovTransferInput> {
        let input = self.input;
        // Read the complete compiler-declared set in one authenticated tree
        // traversal, including every absent fee/policy tail page. This local
        // immutable projection belongs only to this exact owned parent input;
        // it carries no storage handle or authority for a different candidate.
        let keys: Vec<_> = input
            .plan()
            .declared_access()
            .iter()
            .map(|access| access.key.clone())
            .collect();
        let values = input.read_many(&keys)?;
        ensure!(values.len() == keys.len(), "NOV batch read length mismatch");
        let records: BTreeMap<_, _> = keys.into_iter().zip(values).collect();
        let read = |key: &[u8]| {
            records
                .get(key)
                .cloned()
                .context("NOV read outside complete parent record projection")
        };
        let captured_policy: DirectNovFeePolicy = decode_record(&record_pages::read_with(
            &read,
            POLICY_PREFIX,
            POLICY_BYTES,
        )?)?;
        captured_policy.validate()?;
        ensure!(
            captured_policy == self.policy,
            "fee policy differs from exact parent state"
        );
        let fees: FeeState =
            decode_record(&record_pages::read_with(&read, FEE_PREFIX, FEE_BYTES)?)?;
        fees.validate()?;
        let mut balances = BTreeMap::new();
        let mut nonces = BTreeMap::new();
        for (request, tx) in self.requests.iter().zip(input.transactions()) {
            for account in [&request.payer, &request.recipient] {
                if !balances.contains_key(account) {
                    let raw = read(&balance_key(account))?;
                    let value = raw
                        .as_ref()
                        .map(|raw| {
                            Ok::<_, anyhow::Error>(u128::from_le_bytes(
                                raw.as_slice()
                                    .try_into()
                                    .context("invalid balance record length")?,
                            ))
                        })
                        .transpose()?;
                    balances.insert(account.clone(), value);
                }
            }
            let identity = tx.nonce_identity();
            if let std::collections::btree_map::Entry::Vacant(entry) = nonces.entry(identity) {
                let raw = read(&nonce_key(&identity))?;
                // Only a proved absent key, not a missing input, means new signer.
                let value = raw
                    .as_ref()
                    .map(|raw| {
                        Ok::<_, anyhow::Error>(u64::from_le_bytes(
                            raw.as_slice()
                                .try_into()
                                .context("invalid nonce record length")?,
                        ))
                    })
                    .transpose()?
                    .unwrap_or(0);
                entry.insert(value);
            }
        }
        check_nonce_sequence(input.transactions(), &nonces)?;
        let identities: Vec<_> = input
            .transactions()
            .iter()
            .map(|tx| tx.nonce_identity())
            .collect();
        let tx_nonces = input
            .transactions()
            .iter()
            .map(|tx| tx.transfer().nonce)
            .collect();
        let (components, credit_only) = components(&self.requests, &identities, &balances);
        Ok(NovTransferInput {
            input,
            prepared: Prepared {
                policy: self.policy,
                requests: self.requests,
                identities,
                tx_nonces,
                balances,
                nonces,
                fees,
                components,
                credit_only,
            },
        })
    }
}

pub struct NovTransferInput {
    input: SignatureCheckedInput,
    prepared: Prepared,
}

impl NovTransferInput {
    /// Same parent-input export after admission. It neither runs the native
    /// executor nor turns the resulting bytes into proof/publication authority.
    pub fn execution_proof_input(&self) -> Result<Vec<u8>> {
        crate::proof::encode_input(
            self.input.plan(),
            &self.prepared.policy,
            &self.input.encode_witness(crate::proof::capture_budget())?,
        )
    }
}

struct Prepared {
    policy: DirectNovFeePolicy,
    requests: Vec<TransferFeeRequest>,
    identities: Vec<[u8; 32]>,
    tx_nonces: Vec<u64>,
    balances: BTreeMap<Account, Option<u128>>,
    nonces: BTreeMap<[u8; 32], u64>,
    fees: FeeState,
    components: Vec<Vec<usize>>,
    credit_only: BTreeSet<Account>,
}

fn components(
    requests: &[TransferFeeRequest],
    identities: &[[u8; 32]],
    balances: &BTreeMap<Account, Option<u128>>,
) -> (Vec<Vec<usize>>, BTreeSet<Account>) {
    let payers: BTreeSet<_> = requests
        .iter()
        .map(|request| request.payer.clone())
        .collect();
    let mut upper = BTreeMap::new();
    for request in requests {
        if !payers.contains(&request.recipient) {
            let bound = upper
                .entry(request.recipient.clone())
                .or_insert_with(|| Some(balances[&request.recipient].unwrap_or(0)));
            *bound = bound.and_then(|amount| amount.checked_add(request.amount));
        }
    }
    let credit_only: BTreeSet<_> = upper
        .into_iter()
        .filter_map(|(account, bound)| bound.map(|_| account))
        .collect();
    let mut parents: Vec<_> = (0..requests.len()).collect();
    let mut accounts = BTreeMap::new();
    let mut signers = BTreeMap::new();
    for (index, (request, identity)) in requests.iter().zip(identities).enumerate() {
        for account in [&request.payer, &request.recipient] {
            if credit_only.contains(account) {
                continue;
            }
            if let Some(previous) = accounts.insert(account.clone(), index) {
                union(&mut parents, previous, index);
            }
        }
        if let Some(previous) = signers.insert(*identity, index) {
            union(&mut parents, previous, index);
        }
    }
    let mut groups = BTreeMap::<usize, Vec<usize>>::new();
    for index in 0..requests.len() {
        groups
            .entry(find(&mut parents, index))
            .or_default()
            .push(index);
    }
    (groups.into_values().collect(), credit_only)
}

fn find(parents: &mut [usize], mut index: usize) -> usize {
    while parents[index] != index {
        parents[index] = parents[parents[index]];
        index = parents[index];
    }
    index
}

fn union(parents: &mut [usize], left: usize, right: usize) {
    let left = find(parents, left);
    let right = find(parents, right);
    parents[left.max(right)] = left.min(right);
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "nov_transfer_batch/body_tests.rs"]
mod body_tests;
