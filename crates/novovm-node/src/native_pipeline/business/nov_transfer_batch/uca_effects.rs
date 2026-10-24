//! UCA inputs lower into the ORIGINAL complete NOV economic plan and stage.
//! No network listener, initializer, authority, storage or executor is added.
use super::execution::NovTransferReceipt;
use super::*;
use crate::native_pipeline::business::quoted_transfer::{
    BalanceDelta, TransferDelta, TransferFailure, TransferSnapshot,
};
use crate::native_pipeline::business::uca_spend_state::{self, UcaSpendStateAccessV1};
use novovm_adapter_api::uca_spend_policy::{SpendPolicyError, SuppliedSpendGrantV1};
use novovm_exec::uca_spend::SignatureCheckedUcaSpendV1;
use novovm_protocol::uca_spend::UcaSpendIntentV1;

pub(super) struct SpendPlan {
    pub entries: Vec<SpendEntry>,
    pub grants: BTreeMap<NodeHash, SpendGrant>,
}
pub(super) struct SpendEntry {
    pub grant_id: NodeHash,
    pub reserve: u128,
    pub static_failure: Option<SpendPolicyError>,
}
pub(super) struct SpendGrant {
    pub intent: UcaSpendIntentV1,
    pub record: SuppliedSpendGrantV1,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpendOutput {
    pub failure: Option<SpendPolicyError>,
    pub remaining: u128,
}
pub(super) fn authorization_code(error: impl Into<Option<SpendPolicyError>>) -> u128 {
    use SpendPolicyError::*;
    match error.into() {
        None => 0,
        Some(MalformedIntent) => 1,
        Some(StateUnavailable) => 2,
        Some(WrongParent) => 3,
        Some(WrongDomain) => 4,
        Some(AccountInactive) => 5,
        Some(WrongAccount) => 6,
        Some(WrongKey) => 7,
        Some(WrongEpoch) => 8,
        Some(NotSpendPurpose) => 9,
        Some(Revoked) => 10,
        Some(Expired) => 11,
        Some(NonceMismatch) => 20,
        Some(LimitExceeded) => 21,
    }
}
pub(super) fn authorization_error(code: u128) -> Result<Option<SpendPolicyError>> {
    use SpendPolicyError::*;
    Ok(match code {
        0 => None,
        1 => Some(MalformedIntent),
        2 => Some(StateUnavailable),
        3 => Some(WrongParent),
        4 => Some(WrongDomain),
        5 => Some(AccountInactive),
        6 => Some(WrongAccount),
        7 => Some(WrongKey),
        8 => Some(WrongEpoch),
        9 => Some(NotSpendPurpose),
        10 => Some(Revoked),
        11 => Some(Expired),
        20 => Some(NonceMismatch),
        21 => Some(LimitExceeded),
        _ => anyhow::bail!("unknown UCA authorization result"),
    })
}
pub(super) fn rejected_receipt(
    p: &Prepared,
    index: usize,
    s: &TransferSnapshot,
    failure: SpendPolicyError,
) -> NovTransferReceipt {
    let r = &p.requests[index];
    let identity = p.identities[index]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    NovTransferReceipt {
        tx_hash: r.tx_hash,
        signer_identity: p.identities[index],
        delta: TransferDelta {
            tx_hash: r.tx_hash,
            payer: BalanceDelta {
                account: r.payer.clone(),
                before: s.payer_balance,
                after: s.payer_balance,
            },
            recipient: BalanceDelta {
                account: r.recipient.clone(),
                before: s.recipient_balance,
                after: s.recipient_balance,
            },
            nonce_identity: identity,
            nonce_before: s.next_nonce,
            nonce_after: s.next_nonce,
            fee_funding_delta: 0,
        },
        failure: Some(TransferFailure::Authorization(format!("{failure:?}"))),
        quote: None,
        fee_failure: None,
        journal: None,
        clear_clearing_candidates: false,
    }
}

pub fn uca_program_id() -> NodeHash {
    Sha256::digest(b"novovm/nov-transfer-uca-complete-effects/v1").into()
}
pub fn uca_receipt_codec() -> NodeHash {
    Sha256::digest(b"novovm/nov-uca-receipt/authorization-budget/v1").into()
}
pub fn uca_effect_contract(policy: &DirectNovFeePolicy) -> Result<NodeHash> {
    let mut h = Sha256::new();
    h.update(b"novovm/uca-spend-effects/v1\0");
    h.update(effect_contract(policy)?);
    h.update(uca_program_id());
    h.update(uca_receipt_codec());
    h.update(b"auth-reject:no-fee-no-nonce;success:amount+actual-fee;business-failure:actual-fee;self:gross;stable-uca-nonce;ordered-grant-prefix");
    Ok(h.finalize().into())
}

impl NovTransferInput {
    /// Compile already signature-checked intents with SAME-PARENT authority.
    /// Location hints never authorize spending. This is the existing candidate
    /// compiler's new contract, not activation of the V3 transport or genesis.
    #[allow(clippy::too_many_arguments)]
    pub fn capture_uca(
        signed: Vec<SignatureCheckedUcaSpendV1>,
        hints: Vec<(Account, Account)>,
        context: BatchContext,
        policy: DirectNovFeePolicy,
        reader: &dyn StateNodeReader,
        plan_budget: PlanBudget,
        capture_budget: CaptureBudget,
    ) -> Result<Self> {
        ensure!(
            context.business_program == uca_program_id()
                && context.receipt_codec == uca_receipt_codec()
                && context.semantic_version == 1
                && context.effect_contract == uca_effect_contract(&policy)?,
            "UCA economic contract mismatch"
        );
        ensure!(
            !signed.is_empty()
                && signed.len() == hints.len()
                && signed.len() <= plan_budget.transactions,
            "UCA candidate shape/budget"
        );
        let mut accesses = Vec::with_capacity(signed.len());
        let mut declarations = BTreeMap::<Vec<u8>, DeclaredAccess>::new();
        let mut raw = Vec::new();
        let mut body_bytes = 0usize;
        let mut seen = BTreeSet::new();
        for (tx, (payer, recipient)) in signed.iter().zip(&hints) {
            let i = tx.intent();
            ensure!(
                i.chain_id == context.chain_id && i.genesis == context.genesis_config_commitment,
                "UCA execution domain mismatch"
            );
            ensure!(seen.insert(i.signing_bytes()?), "duplicate UCA intent");
            let bytes = tx.signed().encode()?;
            body_bytes = body_bytes
                .checked_add(bytes.len())
                .context("UCA body size overflow")?;
            ensure!(
                bytes.len() <= plan_budget.transaction_bytes
                    && body_bytes <= plan_budget.body_bytes,
                "UCA signed body byte budget"
            );
            raw.push(bytes);
            let access = UcaSpendStateAccessV1::new(i.clone(), payer.clone(), recipient.clone())?;
            for d in access.declarations() {
                declarations
                    .entry(d.key.clone())
                    .or_insert_with(|| d.clone());
            }
            // Only mutable budgets, balances and stable nonce may be changed by spending.
            for d in uca_spend_state::grant_write_declarations(i)? {
                declarations.insert(d.key.clone(), d);
            }
            for key in [
                balance_key(payer),
                balance_key(recipient),
                nonce_key(&i.nonce_identity()?),
            ] {
                declarations
                    .get_mut(&key)
                    .context("economic key not declared")?
                    .may_put = true;
            }
            accesses.push(access);
            ensure!(
                declarations.len() <= plan_budget.access_keys && declarations.len() <= 4096,
                "UCA access budget"
            );
        }
        for d in record_pages::declarations(POLICY_PREFIX, POLICY_BYTES, false)?
            .into_iter()
            .chain(record_pages::declarations(FEE_PREFIX, FEE_BYTES, true)?)
        {
            declarations.insert(d.key.clone(), d);
        }
        ensure!(declarations.len() <= 4096, "UCA tree effect budget");
        let input = BatchPlan::new(
            context,
            raw,
            declarations.into_values().collect(),
            plan_budget,
        )?
        .capture(reader, capture_budget)?;
        let policy_bytes = record_pages::read_with(|k| input.read(k), POLICY_PREFIX, POLICY_BYTES)?;
        let captured: DirectNovFeePolicy = decode_record(&policy_bytes)?;
        ensure!(captured == policy, "UCA fee policy differs from parent");
        let fees: FeeState = decode_record(&record_pages::read_with(
            |k| input.read(k),
            FEE_PREFIX,
            FEE_BYTES,
        )?)?;
        fees.validate()?;
        let mut p = Prepared {
            spend: None,
            policy,
            requests: vec![],
            identities: vec![],
            tx_nonces: vec![],
            balances: BTreeMap::new(),
            nonces: BTreeMap::new(),
            fees,
            components: vec![],
            credit_only: BTreeSet::new(),
        };
        let mut spend = SpendPlan {
            entries: vec![],
            grants: BTreeMap::new(),
        };
        for ((tx, access), (payer, recipient)) in signed.iter().zip(accesses).zip(hints) {
            let i = tx.intent();
            let state = access.read(input.state_input(), context.parent_state_root)?;
            let identity = i.nonce_identity()?;
            let mut h = Sha256::new();
            h.update(b"novovm/uca-grant-budget/v1\0");
            h.update(identity);
            h.update(i.spend_key_ref);
            let grant_id: NodeHash = h.finalize().into();
            spend.entries.push(SpendEntry {
                grant_id,
                reserve: i
                    .amount
                    .checked_add(i.max_fee)
                    .context("UCA reserve overflow")?,
                static_failure: state.static_failure(context.height),
            });
            // The first row may carry an obsolete signed epoch. The record
            // codec must use the CAPTURED record's epoch, never rewrite a signed
            // intent or let a rejected row choose the grant's write identity.
            spend.grants.entry(grant_id).or_insert_with(|| {
                let mut record_identity = i.clone();
                record_identity.authorization_epoch = state.grant().authorization_epoch;
                SpendGrant {
                    intent: record_identity,
                    record: state.grant().clone(),
                }
            });
            let (pb, rb) = state.balances();
            p.balances.insert(payer.clone(), Some(pb));
            p.balances.insert(recipient.clone(), Some(rb));
            p.nonces.insert(identity, state.next_nonce());
            p.identities.push(identity);
            p.tx_nonces.push(i.nonce);
            p.requests.push(TransferFeeRequest {
                tx_hash: Sha256::digest(i.signing_bytes()?).into(),
                payer,
                recipient,
                asset: "NOV".into(),
                amount: i.amount,
                pay_asset: "NOV".into(),
                max_pay_amount: i.max_fee,
                slippage_bps: 0,
            });
        }
        (p.components, p.credit_only) = components(&p.requests, &p.identities, &p.balances);
        p.spend = Some(spend);
        Ok(Self {
            input: ExecutionInput::Uca(input),
            prepared: p,
        })
    }
}
