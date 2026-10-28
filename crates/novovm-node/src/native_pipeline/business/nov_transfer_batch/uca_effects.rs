//! Signed asset-account inputs lower into the ORIGINAL complete NOV plan.
//! No network listener, initializer, authority, storage or executor is added.
use super::execution::NovTransferReceipt;
use super::*;
use crate::native_pipeline::business::quoted_transfer::{
    BalanceDelta, TransferDelta, TransferFailure, TransferSnapshot,
};
use crate::native_pipeline::business::uca_spend_state::{
    self, AssetAccountAuthorityV2, AssetSpendStateAccessV2,
};
use novovm_adapter_api::uca_spend_policy::{SpendPolicyError, SuppliedAssetSpendGrantV2};
use novovm_exec::uca_spend::SignatureCheckedAssetSpendV2;
use novovm_protocol::uca_spend::AssetSpendIntentV2;

pub(super) struct SpendPlan {
    pub entries: Vec<SpendEntry>,
    pub grants: BTreeMap<NodeHash, SpendGrant>,
}
pub(super) struct SpendEntry {
    pub grant_id: NodeHash,
    pub reserve: u128,
    pub static_failure: Option<SpendPolicyError>,
    pub intent: AssetSpendIntentV2,
    /// Zero selects the captured authority; a positive value selects the
    /// successful earlier management row at (value - 1). Selection is executed,
    /// never presumed merely because an update was submitted.
    pub authority_options: Vec<AuthorityOption>,
}
pub(super) struct AuthorityOption {
    pub version: usize,
    pub failure: Option<SpendPolicyError>,
    /// A grant omitted by a replacement remains an old-epoch record only if
    /// it actually existed. Failed updates cannot invent that presence fact.
    pub absent_failure: Option<SpendPolicyError>,
    pub limit: u128,
}
pub(super) struct SpendGrant {
    pub intent: AssetSpendIntentV2,
    pub record: Option<SuppliedAssetSpendGrantV2>,
}

/// Additional lowering metadata has its own bound derived from the ORIGINAL
/// retained body budget. This runs before allocating per-row prefix options.
/// The submitted signed body keeps its existing, separate byte accounting.
pub(super) fn validate_authority_metadata<'a>(
    count: usize,
    intent_at: impl Fn(usize) -> &'a AssetSpendIntentV2,
    body_bytes: usize,
) -> Result<()> {
    let mut bytes = count
        .checked_mul(std::mem::size_of::<(usize, &AssetSpendIntentV2)>())
        .context("asset authority metadata overflow")?;
    for index in 0..count {
        let intent = intent_at(index);
        let prior = (0..index)
            .filter(|previous| {
                let previous = intent_at(*previous);
                previous.authority_update.is_some() && previous.payer == intent.payer
            })
            .count();
        let options = (prior + 1)
            .checked_mul(std::mem::size_of::<AuthorityOption>())
            .context("asset authority option budget overflow")?;
        // Count every possible newly retained grant conservatively, before
        // deduplication. Its location strips the management payload; only the
        // two signed addresses and record's account bytes remain on the heap.
        let grant_count = 1 + intent
            .authority_update
            .as_ref()
            .map_or(0, |update| update.grants.len());
        let grant_bytes = std::mem::size_of::<(NodeHash, SpendGrant)>()
            .checked_add(intent.payer.len().saturating_mul(2))
            .and_then(|bytes| bytes.checked_add(intent.recipient.len()))
            .and_then(|bytes| bytes.checked_mul(grant_count))
            .context("asset authority grant metadata overflow")?;
        bytes = bytes
            .checked_add(std::mem::size_of::<SpendEntry>())
            .and_then(|bytes| bytes.checked_add(intent.retained_heap_bytes()))
            .and_then(|bytes| bytes.checked_add(options))
            .and_then(|bytes| bytes.checked_add(grant_bytes))
            .context("asset authority metadata overflow")?;
        ensure!(
            bytes <= body_bytes,
            "asset authority metadata exceeds retained body budget"
        );
    }
    Ok(())
}

pub(super) fn grant_id(intent: &AssetSpendIntentV2, key: &NodeHash) -> Result<NodeHash> {
    let mut h = Sha256::new();
    h.update(b"novovm/asset-grant-budget/v2\0");
    h.update(intent.nonce_identity()?);
    h.update(key);
    Ok(h.finalize().into())
}

pub(super) fn update_authority(intent: &AssetSpendIntentV2) -> Option<AssetAccountAuthorityV2> {
    intent
        .authority_update
        .as_ref()
        .map(|update| AssetAccountAuthorityV2 {
            account: intent.payer.clone(),
            asset: intent.asset,
            active: update.active,
            current_spend_epoch: update.next_epoch,
            management_key_ref: update.management_key_ref,
            recovery_key_ref: update.recovery_key_ref,
        })
}

pub(super) fn update_grants(intent: &AssetSpendIntentV2) -> Vec<SuppliedAssetSpendGrantV2> {
    intent
        .authority_update
        .as_ref()
        .map(|update| {
            update
                .grants
                .iter()
                .map(|grant| SuppliedAssetSpendGrantV2 {
                    account: intent.payer.clone(),
                    asset: intent.asset,
                    key_ref: grant.key_ref,
                    authorization_epoch: update.next_epoch,
                    permits_nov_transfer: grant.permits_nov_transfer,
                    revoked: false,
                    not_before_height: grant.not_before_height,
                    expires_at_height: grant.expires_at_height,
                    max_debit_per_intent: grant.max_debit_per_intent,
                    remaining_debit_budget: grant.remaining_debit_budget,
                })
                .collect()
        })
        .unwrap_or_default()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpendOutput {
    pub failure: Option<SpendPolicyError>,
    pub remaining: u128,
}
pub(crate) fn authorization_code(error: impl Into<Option<SpendPolicyError>>) -> u128 {
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
pub(crate) fn authorization_error(code: u128) -> Result<Option<SpendPolicyError>> {
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
    Sha256::digest(b"novovm/nov-transfer-asset-complete-effects/v2").into()
}
pub fn uca_receipt_codec() -> NodeHash {
    Sha256::digest(b"novovm/nov-asset-receipt/authorization-budget/v2").into()
}
pub fn uca_effect_contract(policy: &DirectNovFeePolicy) -> Result<NodeHash> {
    let mut h = Sha256::new();
    h.update(b"novovm/asset-spend-effects/v2\0");
    h.update(effect_contract(policy)?);
    h.update(uca_program_id());
    h.update(uca_receipt_codec());
    h.update(b"auth-reject:no-fee-no-nonce;success:amount+actual-fee;business-failure:actual-fee;self:gross;stable-asset-nonce;ordered-authority-and-grant-prefix;management:actual-fee-no-spend-budget");
    Ok(h.finalize().into())
}

impl NovTransferInput {
    /// Compile already signature-checked concrete accounts with SAME-PARENT
    /// authority. Identity mapping is not part of this transaction path.
    pub fn capture_uca(
        signed: Vec<SignatureCheckedAssetSpendV2>,
        context: BatchContext,
        policy: DirectNovFeePolicy,
        reader: &dyn StateNodeReader,
        plan_budget: PlanBudget,
        capture_budget: CaptureBudget,
    ) -> Result<Self> {
        NovTransferBody::prepare_asset_spend(signed, policy, plan_budget)?
            .bind(context)?
            .capture(reader, capture_budget)
    }
}

pub(super) struct UcaMetadata {
    signed: Vec<SignatureCheckedAssetSpendV2>,
    accounts: Vec<(Account, Account)>,
    accesses: Vec<AssetSpendStateAccessV2>,
}

impl NovTransferBody {
    /// Compile only signature-checked bodies. Actual addresses derive from those
    /// bytes; all authority/nonce/balance reads occur in exact-parent capture.
    pub fn prepare_asset_spend(
        signed: Vec<SignatureCheckedAssetSpendV2>,
        policy: DirectNovFeePolicy,
        plan_budget: PlanBudget,
    ) -> Result<Self> {
        let effect_contract = uca_effect_contract(&policy)?;
        ensure!(
            !signed.is_empty() && signed.len() <= plan_budget.transactions,
            "UCA candidate shape/budget"
        );
        validate_authority_metadata(
            signed.len(),
            |index| signed[index].intent(),
            plan_budget.body_bytes,
        )?;
        let accounts = signed
            .iter()
            .map(|tx| {
                Ok((
                    Account::try_from(tx.intent().payer.as_slice()).map_err(anyhow::Error::msg)?,
                    Account::try_from(tx.intent().recipient.as_slice())
                        .map_err(anyhow::Error::msg)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut accesses = Vec::with_capacity(signed.len());
        let mut declarations = BTreeMap::<Vec<u8>, DeclaredAccess>::new();
        let mut raw = Vec::new();
        let mut body_bytes = 0usize;
        let mut seen = BTreeSet::new();
        for (tx, (payer, recipient)) in signed.iter().zip(&accounts) {
            let i = tx.intent();
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
            let access = AssetSpendStateAccessV2::new(i.clone(), payer.clone(), recipient.clone())?;
            for d in access.declarations() {
                declarations
                    .entry(d.key.clone())
                    .or_insert_with(|| d.clone());
            }
            // Ordinary spending mutates only budget, balance and nonce. A signed
            // management operation additionally declares its bounded replacement.
            for d in uca_spend_state::grant_write_declarations(i)? {
                declarations.insert(d.key.clone(), d);
            }
            if i.authority_update.is_some() {
                for d in uca_spend_state::account_write_declarations(i.chain_id, &i.genesis, payer)?
                {
                    declarations.insert(d.key.clone(), d);
                }
                for grant in update_grants(i) {
                    let mut location = i.clone();
                    location.spend_key_ref = grant.key_ref;
                    for d in uca_spend_state::grant_write_declarations(&location)? {
                        declarations.insert(d.key.clone(), d);
                    }
                }
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
        ensure!(
            declarations.len() <= 4096 && declarations.len() <= plan_budget.access_keys,
            "UCA tree effect budget"
        );
        Ok(Self {
            batch: BodyInput::Uca {
                raw,
                metadata: Box::new(UcaMetadata {
                    signed,
                    accounts,
                    accesses,
                }),
            },
            policy,
            requests: vec![],
            declarations: declarations.into_values().collect(),
            effect_contract,
            budget: plan_budget,
        })
    }
}

impl UcaMetadata {
    pub(super) fn check_domain(&self, context: &BatchContext) -> Result<()> {
        ensure!(
            self.signed.iter().all(|tx| {
                tx.intent().chain_id == context.chain_id
                    && tx.intent().genesis == context.genesis_config_commitment
            }),
            "UCA execution domain mismatch"
        );
        Ok(())
    }
    pub(super) fn finalize(
        self,
        input: crate::native_pipeline::execution::plan::OwnedBatchInput,
        policy: DirectNovFeePolicy,
    ) -> Result<NovTransferInput> {
        let context = *input.plan().context();
        let Self {
            signed,
            accounts,
            accesses,
        } = self;
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
        let states = AssetSpendStateAccessV2::read_captured_batch(
            &accesses,
            input.state_input(),
            context.parent_state_root,
        )?;
        // Borrow canonical intents; do not clone every full management payload
        // again into a prefix registry. Capacity was charged before preparation.
        let mut preceding_updates: Vec<(usize, &AssetSpendIntentV2)> =
            Vec::with_capacity(signed.len());
        for (index, ((tx, state), (payer, recipient))) in
            signed.iter().zip(states).zip(accounts).enumerate()
        {
            let state = state?;
            let i = tx.intent();
            let identity = i.nonce_identity()?;
            let grant_id = grant_id(i, &i.spend_key_ref)?;
            let initial_failure = state.static_failure(context.height);
            let prior_count = preceding_updates
                .iter()
                .filter(|(_, previous)| previous.payer == i.payer)
                .count();
            let mut authority_options = Vec::with_capacity(prior_count + 1);
            authority_options.push(AuthorityOption {
                version: 0,
                failure: initial_failure,
                absent_failure: if i.authority_update.is_some() {
                    initial_failure
                } else {
                    Some(SpendPolicyError::StateUnavailable)
                },
                limit: state.grant().map_or(0, |grant| grant.max_debit_per_intent),
            });
            for (version, previous) in preceding_updates
                .iter()
                .filter(|(_, previous)| previous.payer == i.payer)
            {
                let authority =
                    update_authority(previous).context("missing earlier authority update")?;
                let grants = update_grants(previous);
                let grant = grants.iter().find(|grant| grant.key_ref == i.spend_key_ref);
                // If an omitted record actually exists, a successful epoch
                // increment makes it stale. Preserve policy's inactive-before-
                // epoch priority without fabricating a usable grant record.
                // Presence itself remains an executed conditional below.
                let failure = if grant.is_some() || i.authority_update.is_some() {
                    uca_spend_state::authority_failure(
                        i,
                        context.parent_state_root,
                        context.height,
                        &authority,
                        grant,
                    )
                } else if !authority.active {
                    Some(SpendPolicyError::AccountInactive)
                } else {
                    Some(SpendPolicyError::WrongEpoch)
                };
                authority_options.push(AuthorityOption {
                    version: *version,
                    failure,
                    absent_failure: if i.authority_update.is_some() {
                        failure
                    } else {
                        Some(SpendPolicyError::StateUnavailable)
                    },
                    limit: grant.map_or(0, |grant| grant.max_debit_per_intent),
                });
            }
            spend.entries.push(SpendEntry {
                grant_id,
                reserve: i
                    .amount
                    .checked_add(i.max_fee)
                    .context("UCA reserve overflow")?,
                static_failure: initial_failure,
                intent: i.clone(),
                authority_options,
            });
            // The first row may carry an obsolete signed epoch. The record
            // codec must use the CAPTURED record's epoch, never rewrite a signed
            // intent or let a rejected row choose the grant's write identity.
            let record = state.grant().cloned();
            let captured = SpendGrant {
                intent: {
                    let mut record_identity = i.clone();
                    record_identity.authority_update = None;
                    if let Some(grant) = &record {
                        record_identity.authorization_epoch = grant.authorization_epoch;
                    }
                    record_identity
                },
                record,
            };
            if spend
                .grants
                .get(&grant_id)
                .is_none_or(|g| g.record.is_none())
            {
                spend.grants.insert(grant_id, captured);
            }
            if i.authority_update.is_some() {
                for grant in update_grants(i) {
                    let id = self::grant_id(i, &grant.key_ref)?;
                    spend.grants.entry(id).or_insert_with(|| {
                        let mut location = i.clone();
                        location.authority_update = None;
                        location.spend_key_ref = grant.key_ref;
                        location.authorization_epoch = grant.authorization_epoch;
                        SpendGrant {
                            intent: location,
                            record: None,
                        }
                    });
                }
                preceding_updates.push((index + 1, i));
            }
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
        Ok(NovTransferInput {
            input: ExecutionInput::Uca(input),
            prepared: p,
        })
    }
}
