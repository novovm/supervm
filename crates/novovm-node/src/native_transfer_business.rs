#![forbid(unsafe_code)]
//! Owned, declared NOV/NOV business input and AOEM-computed ordered effects.
//! No database, root finalizer, historical ledger or authority capability enters
//! this module. The original node applies effects only to its isolated candidate.
use super::*;
use serde::Serialize;

#[path = "native_transfer_business_journal.rs"]
mod journal;
use journal::FeeJournalDeltaV1;

#[derive(Clone)]
pub(super) struct OwnedItem {
    pub transaction: NovNativeTxWireV1,
    pub request: NovExecutionRequestV1,
    pub subject: NovExecutionSubjectMetaV1,
    pub reservation: NovNativeDurableAuthReservationV1,
}
impl OwnedItem {
    pub fn capture(item: &Item<'_>) -> Result<Self> {
        Ok(Self {
            transaction: item.transaction.clone(),
            request: item.request.clone(),
            subject: enforce_requested_execution_behavior_with_observability_v1(
                item.subject,
                None,
                Some(UcaKeyAlgo::Ed25519),
                None,
                false,
            )
            .map_err(|_| anyhow::anyhow!("standard transfer policy unexpectedly rejected"))?,
            reservation: item.reservation.clone(),
        })
    }
}

/// Capture only the existing Transfer access footprint, never clone the full
/// module, receipt history, unrelated accounts/assets or growing nonce maps.
pub(super) fn capture_store(
    source: &NovNativeExecutionStoreV1,
    items: &[OwnedItem],
) -> Result<NovNativeExecutionStoreV1> {
    if source.module_state.treasury_settlement_journal.len()
        > NOV_TREASURY_SETTLEMENT_JOURNAL_MAX_ENTRIES_V1
    {
        bail!("transfer business fee journal exceeds fixed bound");
    }
    let mut result = NovNativeExecutionStoreV1::default();
    result.schema.clone_from(&source.schema);
    result.authority_chain_id = source.authority_chain_id;
    result
        .authority_namespace_digest
        .clone_from(&source.authority_namespace_digest);
    result
        .module_state
        .native_auth_nonce_identity_scheme
        .clone_from(&source.module_state.native_auth_nonce_identity_scheme);
    result
        .module_state
        .protocol_config_commitment
        .clone_from(&source.module_state.protocol_config_commitment);
    result
        .module_state
        .treasury_policy_version
        .clone_from(&source.module_state.treasury_policy_version);
    result
        .module_state
        .treasury_policy_source
        .clone_from(&source.module_state.treasury_policy_source);
    result
        .module_state
        .treasury_reserve_share_bps
        .clone_from(&source.module_state.treasury_reserve_share_bps);
    result
        .module_state
        .treasury_fee_share_bps
        .clone_from(&source.module_state.treasury_fee_share_bps);
    result
        .module_state
        .treasury_risk_buffer_share_bps
        .clone_from(&source.module_state.treasury_risk_buffer_share_bps);
    result
        .module_state
        .treasury_min_reserve_bucket_nov
        .clone_from(&source.module_state.treasury_min_reserve_bucket_nov);
    result
        .module_state
        .treasury_min_fee_bucket_nov
        .clone_from(&source.module_state.treasury_min_fee_bucket_nov);
    result
        .module_state
        .treasury_min_risk_buffer_nov
        .clone_from(&source.module_state.treasury_min_risk_buffer_nov);
    result
        .module_state
        .treasury_settlement_paused
        .clone_from(&source.module_state.treasury_settlement_paused);
    result
        .module_state
        .treasury_redeem_paused
        .clone_from(&source.module_state.treasury_redeem_paused);
    result
        .module_state
        .mapped_lock_bridge_paused
        .clone_from(&source.module_state.mapped_lock_bridge_paused);
    result
        .module_state
        .mapped_lock_min_confirmations
        .clone_from(&source.module_state.mapped_lock_min_confirmations);
    result
        .module_state
        .mapped_lock_contract_address
        .clone_from(&source.module_state.mapped_lock_contract_address);
    result
        .module_state
        .mapped_asset_burn_paused
        .clone_from(&source.module_state.mapped_asset_burn_paused);
    result
        .module_state
        .mapped_asset_release_paused
        .clone_from(&source.module_state.mapped_asset_release_paused);
    result
        .module_state
        .mapped_asset_auto_heal_enabled
        .clone_from(&source.module_state.mapped_asset_auto_heal_enabled);
    result
        .module_state
        .mapped_asset_auto_heal_rollback_enabled
        .clone_from(&source.module_state.mapped_asset_auto_heal_rollback_enabled);
    result
        .module_state
        .clearing_enabled
        .clone_from(&source.module_state.clearing_enabled);
    result
        .module_state
        .clearing_require_healthy_risk_buffer
        .clone_from(&source.module_state.clearing_require_healthy_risk_buffer);
    result
        .module_state
        .clearing_constrained_max_slippage_bps
        .clone_from(&source.module_state.clearing_constrained_max_slippage_bps);
    result
        .module_state
        .clearing_constrained_daily_usage_bps
        .clone_from(&source.module_state.clearing_constrained_daily_usage_bps);
    result
        .module_state
        .clearing_constrained_strategy
        .clone_from(&source.module_state.clearing_constrained_strategy);
    result
        .module_state
        .clearing_daily_nov_hard_limit
        .clone_from(&source.module_state.clearing_daily_nov_hard_limit);
    result
        .module_state
        .last_clearing_route
        .clone_from(&source.module_state.last_clearing_route);
    result
        .module_state
        .treasury_settled_nov_total
        .clone_from(&source.module_state.treasury_settled_nov_total);
    result
        .module_state
        .treasury_settlements
        .clone_from(&source.module_state.treasury_settlements);
    result
        .module_state
        .treasury_reserve_bucket_nov
        .clone_from(&source.module_state.treasury_reserve_bucket_nov);
    result
        .module_state
        .treasury_fee_bucket_nov
        .clone_from(&source.module_state.treasury_fee_bucket_nov);
    result
        .module_state
        .treasury_risk_buffer_nov
        .clone_from(&source.module_state.treasury_risk_buffer_nov);
    result
        .module_state
        .treasury_settlement_journal
        .clone_from(&source.module_state.treasury_settlement_journal);
    result
        .module_state
        .treasury_settlement_journal_next_seq
        .clone_from(&source.module_state.treasury_settlement_journal_next_seq);
    result
        .module_state
        .clearing_daily_window_day
        .clone_from(&source.module_state.clearing_daily_window_day);
    result
        .module_state
        .clearing_daily_nov_used
        .clone_from(&source.module_state.clearing_daily_nov_used);
    result
        .module_state
        .last_clearing_failure_code
        .clone_from(&source.module_state.last_clearing_failure_code);
    result
        .module_state
        .last_clearing_failure_reason
        .clone_from(&source.module_state.last_clearing_failure_reason);
    result
        .module_state
        .last_clearing_failure_unix_ms
        .clone_from(&source.module_state.last_clearing_failure_unix_ms);
    result
        .module_state
        .last_clearing_candidates
        .clone_from(&source.module_state.last_clearing_candidates);
    result
        .module_state
        .last_fee_quote
        .clone_from(&source.module_state.last_fee_quote);
    result
        .module_state
        .last_fee_quote_failure
        .clone_from(&source.module_state.last_fee_quote_failure);
    if let Some(value) = source.module_state.treasury_reserves.get("NOV") {
        result
            .module_state
            .treasury_reserves
            .insert("NOV".into(), *value);
    }
    if let Some(value) = source.module_state.treasury_settled_by_asset.get("NOV") {
        result
            .module_state
            .treasury_settled_by_asset
            .insert("NOV".into(), *value);
    }
    if let Some(value) = source
        .module_state
        .treasury_settlement_failure_counts
        .get("policy_fallback")
    {
        result
            .module_state
            .treasury_settlement_failure_counts
            .insert("policy_fallback".into(), *value);
    }
    if let Some(value) = source
        .module_state
        .treasury_settlement_failure_counts
        .get("settlement_paused")
    {
        result
            .module_state
            .treasury_settlement_failure_counts
            .insert("settlement_paused".into(), *value);
    }
    if let Some(value) = source
        .module_state
        .treasury_settlement_failure_counts
        .get("amount_overflow")
    {
        result
            .module_state
            .treasury_settlement_failure_counts
            .insert("amount_overflow".into(), *value);
    }
    if let Some(value) = source
        .module_state
        .clearing_failure_counts
        .get("NOV:quote_expired")
    {
        result
            .module_state
            .clearing_failure_counts
            .insert("NOV:quote_expired".into(), *value);
    }
    if let Some(value) = source
        .module_state
        .clearing_failure_counts
        .get("NOV:insufficient_user_balance")
    {
        result
            .module_state
            .clearing_failure_counts
            .insert("NOV:insufficient_user_balance".into(), *value);
    }
    if let Some(value) = source
        .module_state
        .fee_quote_failure_counts
        .get("NOV:max_pay_exceeded")
    {
        result
            .module_state
            .fee_quote_failure_counts
            .insert("NOV:max_pay_exceeded".into(), *value);
    }
    for item in items {
        let NovTxKindV1::Transfer(tx) = &item.transaction.kind else {
            bail!("business capture requires Transfer");
        };
        for raw in [&tx.from, &tx.to] {
            let account = to_hex_prefixed_v1(raw);
            if let Some(assets) = source.module_state.account_asset_balances.get(&account) {
                let target = result
                    .module_state
                    .account_asset_balances
                    .entry(account)
                    .or_default();
                if let Some(value) = assets.get("NOV") {
                    target.insert("NOV".into(), *value);
                }
            }
        }
        let r = &item.reservation;
        if let Some(value) = source
            .module_state
            .native_auth_next_nonces
            .get(&r.identity_key)
        {
            result
                .module_state
                .native_auth_next_nonces
                .insert(r.identity_key.clone(), *value);
        }
        if let Some(value) = source
            .module_state
            .native_auth_nonce_reservations
            .get(&r.ledger_key)
        {
            result
                .module_state
                .native_auth_nonce_reservations
                .insert(r.ledger_key.clone(), value.clone());
        }
        if let Some(value) = source.receipts.get(&r.tx_hash) {
            result.receipts.insert(r.tx_hash.clone(), value.clone());
        }
    }
    Ok(result)
}

type FeeProjection = BTreeMap<String, Vec<u8>>;
fn fee_projection(store: &NovNativeExecutionStoreV1) -> Result<FeeProjection> {
    let m = &store.module_state;
    let mut result = BTreeMap::new();
    result.insert(
        "treasury_settled_nov_total".into(),
        serde_json::to_vec(&m.treasury_settled_nov_total)?,
    );
    result.insert(
        "treasury_settlements".into(),
        serde_json::to_vec(&m.treasury_settlements)?,
    );
    result.insert(
        "treasury_reserve_bucket_nov".into(),
        serde_json::to_vec(&m.treasury_reserve_bucket_nov)?,
    );
    result.insert(
        "treasury_fee_bucket_nov".into(),
        serde_json::to_vec(&m.treasury_fee_bucket_nov)?,
    );
    result.insert(
        "treasury_risk_buffer_nov".into(),
        serde_json::to_vec(&m.treasury_risk_buffer_nov)?,
    );
    result.insert(
        "clearing_daily_window_day".into(),
        serde_json::to_vec(&m.clearing_daily_window_day)?,
    );
    result.insert(
        "clearing_daily_nov_used".into(),
        serde_json::to_vec(&m.clearing_daily_nov_used)?,
    );
    result.insert(
        "last_clearing_failure_code".into(),
        serde_json::to_vec(&m.last_clearing_failure_code)?,
    );
    result.insert(
        "last_clearing_failure_reason".into(),
        serde_json::to_vec(&m.last_clearing_failure_reason)?,
    );
    result.insert(
        "last_clearing_failure_unix_ms".into(),
        serde_json::to_vec(&m.last_clearing_failure_unix_ms)?,
    );
    result.insert(
        "last_clearing_candidates".into(),
        serde_json::to_vec(&m.last_clearing_candidates)?,
    );
    result.insert(
        "last_fee_quote".into(),
        serde_json::to_vec(&m.last_fee_quote)?,
    );
    result.insert(
        "last_fee_quote_failure".into(),
        serde_json::to_vec(&m.last_fee_quote_failure)?,
    );
    result.insert(
        "treasury_reserves/NOV".into(),
        serde_json::to_vec(&m.treasury_reserves.get("NOV"))?,
    );
    result.insert(
        "treasury_settled_by_asset/NOV".into(),
        serde_json::to_vec(&m.treasury_settled_by_asset.get("NOV"))?,
    );
    result.insert(
        "treasury_settlement_failure_counts/policy_fallback".into(),
        serde_json::to_vec(&m.treasury_settlement_failure_counts.get("policy_fallback"))?,
    );
    result.insert(
        "treasury_settlement_failure_counts/settlement_paused".into(),
        serde_json::to_vec(
            &m.treasury_settlement_failure_counts
                .get("settlement_paused"),
        )?,
    );
    result.insert(
        "treasury_settlement_failure_counts/amount_overflow".into(),
        serde_json::to_vec(&m.treasury_settlement_failure_counts.get("amount_overflow"))?,
    );
    result.insert(
        "clearing_failure_counts/NOV:quote_expired".into(),
        serde_json::to_vec(&m.clearing_failure_counts.get("NOV:quote_expired"))?,
    );
    result.insert(
        "clearing_failure_counts/NOV:insufficient_user_balance".into(),
        serde_json::to_vec(
            &m.clearing_failure_counts
                .get("NOV:insufficient_user_balance"),
        )?,
    );
    result.insert(
        "fee_quote_failure_counts/NOV:max_pay_exceeded".into(),
        serde_json::to_vec(&m.fee_quote_failure_counts.get("NOV:max_pay_exceeded"))?,
    );
    Ok(result)
}

#[derive(Serialize)]
struct FeeChange {
    before: Vec<u8>,
    after: Vec<u8>,
}

#[derive(Serialize)]
pub(super) struct Step {
    before: TransferSnapshot,
    payer: Option<Option<u128>>,
    recipient: Option<Option<u128>>,
    fee_changes: BTreeMap<String, FeeChange>,
    journal: FeeJournalDeltaV1,
    pub fee: NovSettledFeeV1,
    pub subject: NovExecutionSubjectMetaV1,
    pub receipt: NovNativeExecutionReceiptV1,
    nonce_after: u64,
}
fn balance(store: &NovNativeExecutionStoreV1, account: &str) -> Option<Option<u128>> {
    store
        .module_state
        .account_asset_balances
        .get(account)
        .map(|assets| assets.get("NOV").copied())
}
fn install_balance(
    store: &mut NovNativeExecutionStoreV1,
    account: String,
    value: Option<Option<u128>>,
) {
    // NOV Transfer never removes an account or existing balance. Preserve
    // unrelated assets in the destination, including an empty NOV projection.
    if let Some(value) = value {
        let assets = store
            .module_state
            .account_asset_balances
            .entry(account)
            .or_default();
        if let Some(value) = value {
            assets.insert("NOV".into(), value);
        }
    }
}
impl Step {
    pub fn apply(
        &self,
        store: &mut NovNativeExecutionStoreV1,
        intent: &TransferIntent,
    ) -> Result<()> {
        if snapshot_v1(store, intent) != self.before
            || self.nonce_after
                != intent
                    .nonce
                    .checked_add(1)
                    .context("business nonce exhausted")?
        {
            bail!("AOEM business effect differs from ordered candidate prefix");
        }
        // Verify every changed fee leaf before mutating any destination. The
        // fixed projection excludes the journal and unrelated asset/history
        // maps; journal shape/sequence has its own bounded prefix check below.
        let current_fees = fee_projection(store)?;
        for (key, change) in &self.fee_changes {
            if current_fees.get(key) != Some(&change.before) {
                bail!("AOEM fee effect differs from ordered candidate prefix: {key}");
            }
        }
        let m = &mut store.module_state;
        self.journal.validate_prefix(
            &m.treasury_settlement_journal,
            m.treasury_settlement_journal_next_seq,
        )?;
        for (key, change) in &self.fee_changes {
            let bytes = &change.after;
            match key.as_str() {
                "treasury_settled_nov_total" => {
                    m.treasury_settled_nov_total = serde_json::from_slice(bytes)?
                }
                "treasury_settlements" => m.treasury_settlements = serde_json::from_slice(bytes)?,
                "treasury_reserve_bucket_nov" => {
                    m.treasury_reserve_bucket_nov = serde_json::from_slice(bytes)?
                }
                "treasury_fee_bucket_nov" => {
                    m.treasury_fee_bucket_nov = serde_json::from_slice(bytes)?
                }
                "treasury_risk_buffer_nov" => {
                    m.treasury_risk_buffer_nov = serde_json::from_slice(bytes)?
                }
                "clearing_daily_window_day" => {
                    m.clearing_daily_window_day = serde_json::from_slice(bytes)?
                }
                "clearing_daily_nov_used" => {
                    m.clearing_daily_nov_used = serde_json::from_slice(bytes)?
                }
                "last_clearing_failure_code" => {
                    m.last_clearing_failure_code = serde_json::from_slice(bytes)?
                }
                "last_clearing_failure_reason" => {
                    m.last_clearing_failure_reason = serde_json::from_slice(bytes)?
                }
                "last_clearing_failure_unix_ms" => {
                    m.last_clearing_failure_unix_ms = serde_json::from_slice(bytes)?
                }
                "last_clearing_candidates" => {
                    m.last_clearing_candidates = serde_json::from_slice(bytes)?
                }
                "last_fee_quote" => m.last_fee_quote = serde_json::from_slice(bytes)?,
                "last_fee_quote_failure" => {
                    m.last_fee_quote_failure = serde_json::from_slice(bytes)?
                }
                "treasury_reserves/NOV" => {
                    if let Some(value) = serde_json::from_slice(bytes)? {
                        m.treasury_reserves.insert("NOV".into(), value);
                    } else {
                        m.treasury_reserves.remove("NOV");
                    }
                }
                "treasury_settled_by_asset/NOV" => {
                    if let Some(value) = serde_json::from_slice(bytes)? {
                        m.treasury_settled_by_asset.insert("NOV".into(), value);
                    } else {
                        m.treasury_settled_by_asset.remove("NOV");
                    }
                }
                "treasury_settlement_failure_counts/policy_fallback" => {
                    if let Some(value) = serde_json::from_slice(bytes)? {
                        m.treasury_settlement_failure_counts
                            .insert("policy_fallback".into(), value);
                    } else {
                        m.treasury_settlement_failure_counts
                            .remove("policy_fallback");
                    }
                }
                "treasury_settlement_failure_counts/settlement_paused" => {
                    if let Some(value) = serde_json::from_slice(bytes)? {
                        m.treasury_settlement_failure_counts
                            .insert("settlement_paused".into(), value);
                    } else {
                        m.treasury_settlement_failure_counts
                            .remove("settlement_paused");
                    }
                }
                "treasury_settlement_failure_counts/amount_overflow" => {
                    if let Some(value) = serde_json::from_slice(bytes)? {
                        m.treasury_settlement_failure_counts
                            .insert("amount_overflow".into(), value);
                    } else {
                        m.treasury_settlement_failure_counts
                            .remove("amount_overflow");
                    }
                }
                "clearing_failure_counts/NOV:quote_expired" => {
                    if let Some(value) = serde_json::from_slice(bytes)? {
                        m.clearing_failure_counts
                            .insert("NOV:quote_expired".into(), value);
                    } else {
                        m.clearing_failure_counts.remove("NOV:quote_expired");
                    }
                }
                "clearing_failure_counts/NOV:insufficient_user_balance" => {
                    if let Some(value) = serde_json::from_slice(bytes)? {
                        m.clearing_failure_counts
                            .insert("NOV:insufficient_user_balance".into(), value);
                    } else {
                        m.clearing_failure_counts
                            .remove("NOV:insufficient_user_balance");
                    }
                }
                "fee_quote_failure_counts/NOV:max_pay_exceeded" => {
                    if let Some(value) = serde_json::from_slice(bytes)? {
                        m.fee_quote_failure_counts
                            .insert("NOV:max_pay_exceeded".into(), value);
                    } else {
                        m.fee_quote_failure_counts.remove("NOV:max_pay_exceeded");
                    }
                }
                _ => bail!("undeclared AOEM fee effect"),
            }
        }
        self.journal.apply(
            &mut m.treasury_settlement_journal,
            &mut m.treasury_settlement_journal_next_seq,
        )?;
        install_balance(store, intent.from.to_hex_prefixed(), self.payer);
        install_balance(store, intent.to.to_hex_prefixed(), self.recipient);
        // The old finalizer consumes this checked nonce once while constructing
        // the unchanged per-transaction roots/receipt. In particular, a finalizer
        // failure before that point must not install this or any later nonce.
        Ok(())
    }
}

pub(super) struct Batch {
    pub steps: Vec<Step>,
    pub recomputed: usize,
    pub components: usize,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn reduce(
    mut store: NovNativeExecutionStoreV1,
    items: Vec<OwnedItem>,
    intents: Vec<TransferIntent>,
    quotes: Vec<std::result::Result<NovFeeQuoteV1, String>>,
    policy: NovTreasurySettlementPolicyV1,
    outcomes: Vec<TransferExecutionOutcomeV1>,
    components: Vec<usize>,
    component_count: usize,
    now_ms: u128,
) -> Result<(Batch, [u8; 32])> {
    let mut previous = fee_projection(&store)?;
    // Keep one bounded comparison window, never one complete journal per Step.
    let mut previous_journal = store.module_state.treasury_settlement_journal.clone();
    let mut previous_journal_seq = store.module_state.treasury_settlement_journal_next_seq;
    let mut invalidated = vec![false; component_count];
    let mut steps = Vec::with_capacity(items.len());
    let mut recomputed = 0;
    for (index, mut outcome) in outcomes.into_iter().enumerate() {
        let item = &items[index];
        let intent = &intents[index];
        let payer = intent.from.to_hex_prefixed();
        let recipient = intent.to.to_hex_prefixed();
        if check_nov_native_durable_auth_reservation_v1(&store, &item.reservation)?.is_some() {
            bail!("candidate transfer unexpectedly replays a committed transaction");
        }
        outcome_binding_v1(&outcome, intent)?;
        let before = snapshot_v1(&store, intent);
        let component = components[index];
        if invalidated[component] || !outcome_matches_snapshot_v1(&outcome, before) {
            invalidated[component] = true;
            // Still on the SAME last AOEM callback. Never submit a nested graph
            // or repeatedly recompute a whole invalidated suffix.
            outcome = crate::native_transfer_delta::compute_outcome_v1(
                intent,
                &before,
                quotes[index].as_ref().err().map(String::as_str),
            )
            .context("AOEM ordered business repair failed")?;
            recomputed += 1;
        }
        outcome_binding_v1(&outcome, intent)?;
        let computed_digest = to_hex(&sha256_bytes_v1(&[
            b"novovm-native-transfer-compute-output-v1\0",
            &serde_json::to_vec(&outcome)?,
        ]));
        let subject = item.subject.clone();
        // Preserve the existing quote observation semantics using the captured
        // quote (and TTL), without reading environment inside the callback.
        let quote = match &quotes[index] {
            Ok(quote) => {
                store.module_state.last_fee_quote = Some(quote.clone());
                store.module_state.last_fee_quote_failure = None;
                Ok(quote.clone())
            }
            Err(reason) => {
                let code = fee_reason_code_v1(reason, NOV_FEE_FAILURE_QUOTE_PREFIX_V1)
                    .unwrap_or("rate_unavailable");
                increment_quote_failure_v1(&mut store, "NOV", code);
                store.module_state.last_fee_quote_failure = Some(reason.clone());
                Err(anyhow::anyhow!(reason.clone()))
            }
        };
        let settled = match quote {
            Ok(quote) => settle_fee_quote_with_policy_v1(
                &mut store,
                &quote,
                &item.reservation.tx_hash,
                &subject,
                now_ms,
                &policy,
            ),
            Err(error) => Err(error),
        };
        let (settled_fee, mut receipt) = match settled {
            Err(error) => {
                let reason = error.to_string();
                outcome = outcome.reject_fee(reason.clone());
                if native_account_asset_balance_v1(&store, &payer, "NOV")
                    != outcome.delta().payer.before
                    || native_account_asset_balance_v1(&store, &recipient, "NOV")
                        != outcome.delta().recipient.before
                {
                    bail!("rejected direct NOV fee changed a monetary balance");
                }
                let fee = unresolved_settled_fee_v1(&item.request);
                let method = if is_fee_quote_reason_v1(&reason) {
                    "quote"
                } else {
                    "settlement"
                };
                let receipt = build_failed_native_receipt_v1(
                    &item.request,
                    &fee,
                    &subject,
                    "fee".into(),
                    method.into(),
                    reason,
                );
                (fee, receipt)
            }
            Ok(fee) => {
                if matches!(outcome.failure(), Some(TransferExecutionFailureV1::Fee(_)))
                    || outcome.delta().fee_funding_delta != fee.nov_amount
                    || fee.source_amount != fee.nov_amount
                    || fee.source_asset != "NOV"
                    || native_account_asset_balance_v1(&store, &payer, "NOV")
                        != outcome
                            .delta()
                            .payer
                            .before
                            .checked_sub(fee.nov_amount)
                            .context("settled fee exceeds input balance")?
                {
                    bail!("AOEM transfer fee outcome differs from unified settlement");
                }
                // Settlement already debited the fee. These are absolute
                // AOEM-computed after values, NOT another debit operation.
                store
                    .module_state
                    .account_asset_balances
                    .entry(payer.clone())
                    .or_default()
                    .insert("NOV".into(), outcome.delta().payer.after);
                if payer != recipient && outcome.is_success() {
                    store
                        .module_state
                        .account_asset_balances
                        .entry(recipient.clone())
                        .or_default()
                        .insert("NOV".into(), outcome.delta().recipient.after);
                }
                let receipt = match outcome.failure() {
                    Some(TransferExecutionFailureV1::Business(error)) => {
                        build_failed_native_receipt_v1(
                            &item.request,
                            &fee,
                            &subject,
                            "native_asset".into(),
                            "transfer".into(),
                            format!("native.transfer.{error}"),
                        )
                    }
                    None => build_success_native_receipt_v1(
                        &item.request,
                        &fee,
                        &subject,
                        "native_asset",
                        "transfer",
                        vec![NovNativeExecutionLogV1 {
                            module: "native_asset".into(),
                            method: "transfer".into(),
                            event: "native_asset.transferred".into(),
                            data: serde_json::json!({"asset":"NOV", "from":payer, "to":recipient, "amount":intent.amount.to_string()}),
                        }],
                    ),
                    Some(TransferExecutionFailureV1::Fee(_)) => {
                        unreachable!("checked fee outcome")
                    }
                };
                (fee, receipt)
            }
        };
        receipt.logs.push(NovNativeExecutionLogV1 {
                module: "aoem".into(), method: "native_transfer_compute".into(), event: "aoem.native_transfer.computed".into(),
                data: serde_json::json!({"scheduler":"aoem_generic_compute_v2", "tx_hash":item.reservation.tx_hash, "output_digest":computed_digest,
                    "phase":"pre_global_fee_reduction", "authorizes_state_publication":false}),
            });
        commit_nov_native_durable_auth_reservation_v1(&mut store, &item.reservation)?;
        let journal = FeeJournalDeltaV1::between(
            &previous_journal,
            previous_journal_seq,
            &store.module_state.treasury_settlement_journal,
            store.module_state.treasury_settlement_journal_next_seq,
        )?;
        journal.apply(&mut previous_journal, &mut previous_journal_seq)?;
        let projected = fee_projection(&store)?;
        let fee_changes = projected
            .iter()
            .filter(|(key, value)| previous.get(*key) != Some(*value))
            .map(|(key, value)| {
                Ok((
                    key.clone(),
                    FeeChange {
                        before: previous
                            .get(key)
                            .context("missing captured fee prefix")?
                            .clone(),
                        after: value.clone(),
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        previous = projected;
        steps.push(Step {
            before,
            payer: balance(&store, &payer),
            recipient: balance(&store, &recipient),
            fee_changes,
            journal,
            fee: settled_fee,
            subject,
            receipt,
            nonce_after: outcome.delta().nonce_after,
        });
    }
    if steps.len() != items.len() {
        bail!("incomplete AOEM business reduction");
    }
    let digest = business_effects_digest(&steps)?;
    Ok((
        Batch {
            steps,
            recomputed,
            components: component_count,
        },
        digest,
    ))
}

/// Preserve the exact domain + JSON digest bytes without allocating a second
/// full-batch serialization beside the already owned typed results.
fn business_effects_digest(value: &(impl Serialize + ?Sized)) -> Result<[u8; 32]> {
    struct DigestWriter(sha2::Sha256);
    impl std::io::Write for DigestWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            sha2::Digest::update(&mut self.0, bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = DigestWriter(sha2::Sha256::default());
    sha2::Digest::update(
        &mut writer.0,
        b"novovm-native-transfer-business-effects-v1\0",
    );
    serde_json::to_writer(&mut writer, value).context("hash AOEM business effects")?;
    Ok(sha2::Digest::finalize(writer.0).into())
}

#[cfg(test)]
#[path = "native_transfer_business_tests.rs"]
mod tests;
