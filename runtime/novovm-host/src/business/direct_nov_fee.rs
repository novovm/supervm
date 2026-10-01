#![forbid(unsafe_code)]

//! Pure NOV-direct quotation and ordered settlement, migrated from
//! `legacy/supervm-20261002/crates/novovm-node/src/tx_ingress.rs` at `01afa749`:
//! `build_execution_fee_quote_v1`, `check_direct_nov_fee_capacity_v1`, and the
//! direct branch of `settle_fee_quote_into_treasury_v1`; Transfer projection is
//! from `native_transfer_dispatch.rs::fee_request_v1` at the same baseline.
//!
//! This is a new typed record layout, NOT the old Store/receipt codec. Policy is
//! explicit and must be independently bound to the authenticated batch/parent.
//! No environment, clock, database, nonce reservation, mint, burn, or publication
//! is accessed here. Business failure still pays a successfully settled fee;
//! composing balances/nonce/receipts and repairing dependent speculation belongs
//! to the batch compiler. Returned effects are tentative, not durable evidence.

use super::quoted_transfer::Account;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

pub const JOURNAL_RETAIN_LAST: usize = 512;
const BPS: u128 = 10_000;
const RATE_PPM: u128 = 1_000_000;
const DAY_MS: u128 = 86_400_000;
const MAX_POLICY_TEXT: usize = 128;
const MAX_REASON_TEXT: usize = 512;

/// Complete resolved inputs to NOV settlement and its policy attribution. No
/// defaults or environment fallback occur here. `resolution_source` preserves
/// the old fallback diagnostic; `policy_source` names the selected policy path.
/// Non-NOV gate fields affect threshold *metadata*, never block direct NOV.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectNovFeePolicy {
    pub quote_ttl_ms: u128,
    pub policy_version: u32,
    pub policy_source: String,
    pub resolution_source: String,
    pub reserve_share_bps: u32,
    pub fee_share_bps: u32,
    pub risk_buffer_share_bps: u32,
    pub min_reserve_bucket_nov: u128,
    pub min_fee_bucket_nov: u128,
    pub min_risk_buffer_nov: u128,
    pub settlement_paused: bool,
    pub redeem_paused: bool,
    pub clearing_enabled: bool,
    pub clearing_daily_nov_hard_limit: u128,
    pub clearing_require_healthy_risk_buffer: bool,
    pub clearing_constrained_max_slippage_bps: u32,
    pub clearing_constrained_daily_usage_bps: u32,
    pub clearing_constrained_strategy: String,
    pub mapped_asset_auto_heal_rollback_enabled: bool,
    pub mapped_asset_reorg_response_policy: String,
}

impl DirectNovFeePolicy {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.policy_version > 0,
            "fee policy version must be explicit"
        );
        ensure!(
            self.reserve_share_bps > 0 && self.fee_share_bps > 0 && self.risk_buffer_share_bps > 0,
            "fee shares must all be positive"
        );
        ensure!(
            u64::from(self.reserve_share_bps)
                + u64::from(self.fee_share_bps)
                + u64::from(self.risk_buffer_share_bps)
                == BPS as u64,
            "fee shares must sum to 10000"
        );
        ensure!(
            self.min_risk_buffer_nov > 0,
            "resolved risk-buffer minimum must be positive"
        );
        ensure!(
            (1..=BPS as u32).contains(&self.clearing_constrained_daily_usage_bps),
            "resolved constrained daily usage out of range"
        );
        for text in [
            &self.policy_source,
            &self.resolution_source,
            &self.clearing_constrained_strategy,
            &self.mapped_asset_reorg_response_policy,
        ] {
            ensure!(
                text.len() <= MAX_POLICY_TEXT,
                "fee policy text exceeds bound"
            );
        }
        ensure!(
            !self.resolution_source.is_empty(),
            "fee policy resolution source missing"
        );
        ensure!(
            matches!(
                self.clearing_constrained_strategy.as_str(),
                "daily_volume_only" | "treasury_direct_only" | "blocked"
            ),
            "unresolved constrained strategy"
        );
        ensure!(
            matches!(
                self.mapped_asset_reorg_response_policy.as_str(),
                "report_only" | "freeze_only" | "freeze_and_rollback"
            ),
            "unresolved reorg policy"
        );
        Ok(())
    }

    pub fn normalized_source(&self) -> String {
        let source = self.policy_source.trim().to_ascii_lowercase();
        if source.is_empty() || source == "default" {
            "config_path".into()
        } else {
            source
        }
    }

    /// Legacy attribution string, NOT a cryptographic policy commitment. The
    /// compiler must commit the complete snapshot, including TTL/provenance.
    pub fn contract_id(&self) -> String {
        format!(
            "nov_treasury_policy_v1:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
            self.policy_version,
            self.normalized_source(),
            self.reserve_share_bps,
            self.fee_share_bps,
            self.risk_buffer_share_bps,
            self.min_reserve_bucket_nov,
            self.min_fee_bucket_nov,
            self.min_risk_buffer_nov,
            u8::from(self.settlement_paused),
            u8::from(self.redeem_paused),
            u8::from(self.clearing_enabled),
            self.clearing_daily_nov_hard_limit,
            u8::from(self.clearing_require_healthy_risk_buffer),
            self.clearing_constrained_max_slippage_bps,
            self.clearing_constrained_daily_usage_bps,
            self.clearing_constrained_strategy,
            u8::from(self.mapped_asset_auto_heal_rollback_enabled),
            self.mapped_asset_reorg_response_policy
        )
    }
}

/// Caller binds this projection to the exact authenticated wire. Normalization
/// retains legacy empty/whitespace/case NOV aliases without rewriting the wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferFeeRequest {
    pub tx_hash: [u8; 32],
    pub payer: Account,
    pub recipient: Account,
    pub asset: String,
    pub amount: u128,
    pub pay_asset: String,
    pub max_pay_amount: u128,
    pub slippage_bps: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeeQuote {
    pub quote_id: String,
    pub pay_asset: String,
    pub nov_amount: u128,
    pub quoted_pay_amount: u128,
    pub quoted_pay_amount_with_slippage: u128,
    pub max_pay_amount: u128,
    pub slippage_bps: u32,
    pub quoted_at_unix_ms: u128,
    pub expires_at_unix_ms: u128,
    pub rate_ppm: u128,
    pub oracle_updated_at_unix_ms: u128,
    pub route: String,
    pub quote_contract: String,
    pub price_source: String,
}

/// Optional map leaves distinguish absence from a present zero. Treasury totals
/// and bucket sums are overlapping accounting views, not independent assets.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeeAccounting {
    pub treasury_reserve_nov: Option<u128>,
    pub settled_nov_total: u128,
    pub settled_by_asset_nov: Option<u128>,
    pub reserve_bucket_nov: u128,
    pub fee_bucket_nov: u128,
    pub risk_buffer_nov: u128,
    pub settlements: u64,
    pub journal_next_seq: u64,
    pub daily_window_day: u64,
    pub daily_nov_used: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeeFailureCode {
    MaxPayExceeded,
    SettlementPaused,
    QuoteExpired,
    InsufficientUserBalance,
    AmountOverflow,
}

impl FeeFailureCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MaxPayExceeded => "fee.quote.max_pay_exceeded",
            Self::SettlementPaused => "fee.settlement.settlement_paused",
            Self::QuoteExpired => "fee.clearing.quote_expired",
            Self::InsufficientUserBalance => "fee.clearing.insufficient_user_balance",
            Self::AmountOverflow => "fee.settlement.amount_overflow",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeeFailure {
    pub code: FeeFailureCode,
    pub reason: String,
}

impl std::fmt::Display for FeeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for FeeFailure {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClearingFailure {
    pub failure: FeeFailure,
    pub unix_ms: u128,
}

/// Only direct-NOV keys, with the old saturating diagnostic counters. Monetary
/// counters (`settlements`/`journal_next_seq`) are checked, never saturating.
/// The direct-only layout has no foreign-route candidate list; successful
/// settlement emits an explicit clear effect for that projection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeeDiagnostics {
    pub quote_max_pay_exceeded: u64,
    pub settlement_policy_fallback: u64,
    pub settlement_paused: u64,
    pub settlement_amount_overflow: u64,
    pub clearing_quote_expired: u64,
    pub clearing_insufficient_user_balance: u64,
    pub last_quote: Option<FeeQuote>,
    pub last_quote_failure: Option<FeeFailure>,
    pub last_clearing_failure: Option<ClearingFailure>,
    /// This new direct-only profile cannot import foreign-route candidates.
    /// The explicit empty array rejects nonempty lists during decoding instead
    /// of silently discarding their content. No route discovery occurs here.
    pub last_clearing_candidates: [(); 0],
}

/// Versioned by the compiler's explicit record codec, not by an implicit old
/// Store decoder. Journal history is separate immutable per-sequence records;
/// each successful effect carries one append and the retain-last-512 rule.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeeState {
    pub accounting: FeeAccounting,
    pub diagnostics: FeeDiagnostics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThresholdState {
    Healthy,
    Constrained,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeeJournalEntry {
    pub seq: u64,
    pub unix_ms: u128,
    pub tx_hash: [u8; 32],
    pub payer: Account,
    pub source_amount: u128,
    pub settled_nov: u128,
    pub reserve_bucket_delta_nov: u128,
    pub fee_bucket_delta_nov: u128,
    pub risk_buffer_delta_nov: u128,
    pub policy_version: u32,
    pub policy_source: String,
    pub policy_contract_id: String,
    pub policy_threshold_state: ThresholdState,
    pub policy_constrained_strategy: String,
}

/// Only fee money and observations, not the amount transfer or nonce transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeeEffects {
    pub after_fee_state: FeeState,
    pub payer_after: u128,
    pub quote: Option<FeeQuote>,
    pub failure: Option<FeeFailure>,
    pub journal: Option<FeeJournalEntry>,
    pub clear_clearing_candidates: bool,
}

impl FeeEffects {
    pub fn charged_fee(&self) -> u128 {
        self.journal.as_ref().map_or(0, |entry| entry.settled_nov)
    }
}

impl FeeQuote {
    /// Structural validation for persisted diagnostic records. Settlement also
    /// re-quotes the exact request/policy and compares all fields; this alone
    /// does not turn a decoded quote into authority to charge a payer.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.pay_asset == "NOV"
                && self.rate_ppm == RATE_PPM
                && self.route == "direct_nov"
                && self.price_source == "direct_nov"
                && self.quote_contract == "novovm-exec-fee-quote/v1",
            "invalid direct NOV quote profile"
        );
        ensure!(
            (40..=104).contains(&self.nov_amount)
                && self.quoted_pay_amount == self.nov_amount
                && self.slippage_bps <= BPS as u32,
            "invalid NOV quote arithmetic"
        );
        let inclusive = (self.nov_amount * (BPS + u128::from(self.slippage_bps))).div_ceil(BPS);
        ensure!(
            self.quoted_pay_amount_with_slippage == inclusive && self.max_pay_amount >= inclusive,
            "invalid NOV quote cap"
        );
        ensure!(
            self.oracle_updated_at_unix_ms == self.quoted_at_unix_ms
                && self.expires_at_unix_ms >= self.quoted_at_unix_ms,
            "invalid NOV quote times"
        );
        ensure!(self.quote_id.len() <= 48, "fee quote id exceeds bound");
        let parts: Vec<_> = self.quote_id.split('-').collect();
        ensure!(
            parts.len() == 3
                && parts[0] == "q"
                && parts[1].len() == 12
                && parts[1]
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                && parts[2] == format!("{:x}", self.quoted_at_unix_ms),
            "invalid NOV quote id"
        );
        Ok(())
    }
}

impl FeeState {
    /// Call after bounded canonical decoding. No growing maps or history are
    /// accepted. Overflowing accounting is not repaired here: the next fee is
    /// rejected by monetary preflight with the original diagnostic effects.
    pub fn validate(&self) -> Result<()> {
        if let Some(quote) = &self.diagnostics.last_quote {
            quote.validate()?;
        }
        if let Some(failure) = &self.diagnostics.last_quote_failure {
            ensure!(
                failure.code == FeeFailureCode::MaxPayExceeded,
                "wrong quote diagnostic category"
            );
            validate_failure(failure)?;
        }
        if let Some(clearing) = &self.diagnostics.last_clearing_failure {
            ensure!(
                matches!(
                    clearing.failure.code,
                    FeeFailureCode::QuoteExpired | FeeFailureCode::InsufficientUserBalance
                ),
                "wrong clearing diagnostic category"
            );
            validate_failure(&clearing.failure)?;
        }
        Ok(())
    }
}

fn validate_failure(failure: &FeeFailure) -> Result<()> {
    ensure!(
        failure.reason.len() <= MAX_REASON_TEXT
            && failure
                .reason
                .starts_with(&format!("{}: ", failure.code.as_str())),
        "invalid bounded fee failure"
    );
    Ok(())
}

fn is_nov(asset: &str) -> bool {
    let asset = asset.trim();
    asset.is_empty() || asset.eq_ignore_ascii_case("NOV")
}

fn validate_request(request: &TransferFeeRequest) -> Result<()> {
    ensure!(
        is_nov(&request.asset) && is_nov(&request.pay_asset),
        "direct NOV transfer requires NOV asset and NOV fee payment"
    );
    ensure!(
        matches!(request.payer.as_bytes().len(), 20 | 32)
            && matches!(request.recipient.as_bytes().len(), 20 | 32),
        "invalid transfer account width"
    );
    Ok(())
}

fn hex(bytes: &[u8], prefixed: bool) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(bytes.len() * 2 + usize::from(prefixed) * 2);
    if prefixed {
        result.push_str("0x");
    }
    for byte in bytes {
        write!(&mut result, "{byte:02x}").expect("String write cannot fail");
    }
    result
}

fn failure(code: FeeFailureCode, detail: impl std::fmt::Display) -> FeeFailure {
    FeeFailure {
        code,
        reason: format!("{}: {detail}", code.as_str()),
    }
}

/// Pure Transfer quote: preserves the original compact JSON argument projection,
/// native-module/method/gas costs, NOV 1:1 rate, cap sentinel and slippage rounding.
/// An economic cap refusal is the inner Err, not an infrastructure/config error.
pub fn quote_transfer(
    request: &TransferFeeRequest,
    policy: &DirectNovFeePolicy,
    now: u128,
) -> Result<std::result::Result<FeeQuote, FeeFailure>> {
    policy.validate()?;
    validate_request(request)?;
    // Every interpolated value is fixed NOV, canonical hexadecimal, or decimal
    // digits, so this is byte-identical to the old ordered serde JSON struct.
    let arguments = format!(
        "{{\"asset\":\"NOV\",\"to\":\"{}\",\"amount\":\"{}\"}}",
        hex(request.recipient.as_bytes(), true),
        request.amount
    );
    let nov_amount = 40 + ((arguments.len() as u128).div_ceil(16)).min(64);
    let slippage_bps = request.slippage_bps.min(BPS as u32);
    // nov_amount <= 104; this multiplication and ceiling cannot overflow.
    let with_slippage = (nov_amount * (BPS + u128::from(slippage_bps))).div_ceil(BPS);
    let max_pay_amount = if request.max_pay_amount == 0 {
        with_slippage
    } else {
        request.max_pay_amount
    };
    if with_slippage > max_pay_amount {
        return Ok(Err(failure(
            FeeFailureCode::MaxPayExceeded,
            format!(
            "required_with_slippage={with_slippage} max_pay_amount={max_pay_amount} pay_asset=NOV"),
        )));
    }
    Ok(Ok(FeeQuote {
        quote_id: format!("q-{}-{now:x}", hex(&request.tx_hash[..6], false)),
        pay_asset: "NOV".into(),
        nov_amount,
        quoted_pay_amount: nov_amount,
        quoted_pay_amount_with_slippage: with_slippage,
        max_pay_amount,
        slippage_bps,
        quoted_at_unix_ms: now,
        expires_at_unix_ms: now.saturating_add(policy.quote_ttl_ms.max(1)),
        rate_ppm: RATE_PPM,
        oracle_updated_at_unix_ms: now,
        route: "direct_nov".into(),
        quote_contract: "novovm-exec-fee-quote/v1".into(),
        price_source: "direct_nov".into(),
    }))
}

fn threshold(policy: &DirectNovFeePolicy, state: &FeeAccounting) -> ThresholdState {
    let risk_healthy = state.risk_buffer_nov >= policy.min_risk_buffer_nov;
    let hard_limit = policy.clearing_daily_nov_hard_limit;
    let reached = hard_limit > 0 && state.daily_nov_used >= hard_limit;
    if !policy.clearing_enabled
        || (policy.clearing_require_healthy_risk_buffer && !risk_healthy)
        || reached
    {
        return ThresholdState::Blocked;
    }
    // Saturation here intentionally preserves old diagnostic threshold rules;
    // this is not a monetary addition or permission to reject direct NOV.
    let near = hard_limit > 0
        && state.daily_nov_used.saturating_mul(BPS)
            >= hard_limit.saturating_mul(u128::from(policy.clearing_constrained_daily_usage_bps));
    if state.reserve_bucket_nov < policy.min_reserve_bucket_nov
        || state.fee_bucket_nov < policy.min_fee_bucket_nov
        || near
    {
        ThresholdState::Constrained
    } else {
        ThresholdState::Healthy
    }
}

fn credit_accounting(
    before: &FeeAccounting,
    amount: u128,
    policy: &DirectNovFeePolicy,
) -> Option<(FeeAccounting, [u128; 3])> {
    let reserve = amount.checked_mul(u128::from(policy.reserve_share_bps))? / BPS;
    let fee = amount.checked_mul(u128::from(policy.fee_share_bps))? / BPS;
    let risk = amount.checked_sub(reserve)?.checked_sub(fee)?;
    let mut after = before.clone();
    after.treasury_reserve_nov = Some(
        before
            .treasury_reserve_nov
            .unwrap_or(0)
            .checked_add(amount)?,
    );
    after.settled_nov_total = before.settled_nov_total.checked_add(amount)?;
    after.settled_by_asset_nov = Some(
        before
            .settled_by_asset_nov
            .unwrap_or(0)
            .checked_add(amount)?,
    );
    after.reserve_bucket_nov = before.reserve_bucket_nov.checked_add(reserve)?;
    after.fee_bucket_nov = before.fee_bucket_nov.checked_add(fee)?;
    after.risk_buffer_nov = before.risk_buffer_nov.checked_add(risk)?;
    after
        .reserve_bucket_nov
        .checked_add(after.fee_bucket_nov)?
        .checked_add(after.risk_buffer_nov)?;
    after.settlements = before.settlements.checked_add(1)?;
    after.journal_next_seq = before.journal_next_seq.checked_add(1)?;
    Some((after, [reserve, fee, risk]))
}

fn reject_settlement(mut effects: FeeEffects, rejected: FeeFailure, now: u128) -> FeeEffects {
    let diagnostics = &mut effects.after_fee_state.diagnostics;
    match rejected.code {
        FeeFailureCode::SettlementPaused => {
            diagnostics.settlement_paused = diagnostics.settlement_paused.saturating_add(1)
        }
        FeeFailureCode::AmountOverflow => {
            diagnostics.settlement_amount_overflow =
                diagnostics.settlement_amount_overflow.saturating_add(1)
        }
        FeeFailureCode::QuoteExpired => {
            diagnostics.clearing_quote_expired =
                diagnostics.clearing_quote_expired.saturating_add(1);
            diagnostics.last_clearing_failure = Some(ClearingFailure {
                failure: rejected.clone(),
                unix_ms: now,
            });
        }
        FeeFailureCode::InsufficientUserBalance => {
            diagnostics.clearing_insufficient_user_balance = diagnostics
                .clearing_insufficient_user_balance
                .saturating_add(1);
            diagnostics.last_clearing_failure = Some(ClearingFailure {
                failure: rejected.clone(),
                unix_ms: now,
            });
        }
        FeeFailureCode::MaxPayExceeded => unreachable!("quotation does not enter settlement"),
    }
    effects.failure = Some(rejected);
    effects
}

/// Recompute the required quote from the supplied request and explicit policy,
/// record quote observations, then settle at `now`. The old quote contains only
/// a short hash prefix and fee metadata, NOT a full transaction commitment; the
/// compiler must bind the complete authenticated request independently. Delayed
/// settlement cannot replace quoted fee amounts or expiry with arbitrary values.
/// Invalid metadata/config is outer Err with no effects. Legitimate rejection
/// returns effects containing day/diagnostic changes and unchanged money.
pub fn plan_settlement(
    request: &TransferFeeRequest,
    quote: &FeeQuote,
    policy: &DirectNovFeePolicy,
    before: &FeeState,
    payer_before: u128,
    now: u128,
) -> Result<FeeEffects> {
    before.validate()?;
    quote.validate()?;
    let expected = quote_transfer(request, policy, quote.quoted_at_unix_ms)?;
    ensure!(
        expected.as_ref().ok() == Some(quote),
        "prepared fee quote/request/policy mismatch"
    );
    let mut effects = FeeEffects {
        after_fee_state: before.clone(),
        payer_after: payer_before,
        quote: Some(quote.clone()),
        failure: None,
        journal: None,
        clear_clearing_candidates: false,
    };
    effects.after_fee_state.diagnostics.last_quote = Some(quote.clone());
    effects.after_fee_state.diagnostics.last_quote_failure = None;
    // Deliberately retain old u128-day -> u64 conversion and saturated quote
    // expiry. Real block time has a narrower domain; do not silently change old
    // arithmetic at synthetic extremes as part of this local migration.
    let day = (now / DAY_MS) as u64;
    let accounting = &mut effects.after_fee_state.accounting;
    if accounting.daily_window_day != day {
        accounting.daily_window_day = day;
        accounting.daily_nov_used = 0;
    }
    let threshold_state = threshold(policy, accounting);
    if policy.resolution_source.starts_with("default_fallback") {
        let count = &mut effects
            .after_fee_state
            .diagnostics
            .settlement_policy_fallback;
        *count = count.saturating_add(1);
    }
    if policy.settlement_paused {
        return Ok(reject_settlement(
            effects,
            failure(
                FeeFailureCode::SettlementPaused,
                "treasury settlement is paused",
            ),
            now,
        ));
    }
    if now > quote.expires_at_unix_ms {
        return Ok(reject_settlement(
            effects,
            failure(
                FeeFailureCode::QuoteExpired,
                format!(
                    "asset=NOV quote_id={} now={now} expires_at={}",
                    quote.quote_id, quote.expires_at_unix_ms
                ),
            ),
            now,
        ));
    }
    let Some(payer_after) = payer_before.checked_sub(quote.nov_amount) else {
        return Ok(reject_settlement(effects, failure(FeeFailureCode::InsufficientUserBalance, format!(
            "asset=NOV nov_fee_asset_debit_failed: account={} requested={} available={payer_before}",
            hex(request.payer.as_bytes(), true), quote.nov_amount)), now));
    };
    let Some((accounting, [reserve, fee, risk])) = credit_accounting(
        &effects.after_fee_state.accounting,
        quote.nov_amount,
        policy,
    ) else {
        return Ok(reject_settlement(
            effects,
            failure(
                FeeFailureCode::AmountOverflow,
                "direct NOV fee settlement monetary capacity exceeded",
            ),
            now,
        ));
    };
    effects.journal = Some(FeeJournalEntry {
        seq: accounting.journal_next_seq,
        unix_ms: now,
        tx_hash: request.tx_hash,
        payer: request.payer.clone(),
        source_amount: quote.nov_amount,
        settled_nov: quote.nov_amount,
        reserve_bucket_delta_nov: reserve,
        fee_bucket_delta_nov: fee,
        risk_buffer_delta_nov: risk,
        policy_version: policy.policy_version,
        policy_source: policy.normalized_source(),
        policy_contract_id: policy.contract_id(),
        policy_threshold_state: threshold_state,
        policy_constrained_strategy: policy.clearing_constrained_strategy.clone(),
    });
    effects.after_fee_state.accounting = accounting;
    effects.after_fee_state.diagnostics.last_clearing_candidates = [];
    effects.payer_after = payer_after;
    effects.clear_clearing_candidates = true;
    Ok(effects)
}

/// Quote and settle at the same explicit time, matching the normal ordered
/// Transfer execution path. A failed quote does not enter settlement: no day
/// refresh, fallback counter, treasury change, or previous-quote replacement.
pub fn quote_and_settle(
    request: &TransferFeeRequest,
    policy: &DirectNovFeePolicy,
    before: &FeeState,
    payer_before: u128,
    now: u128,
) -> Result<FeeEffects> {
    before.validate()?;
    match quote_transfer(request, policy, now)? {
        Ok(quote) => plan_settlement(request, &quote, policy, before, payer_before, now),
        Err(rejected) => {
            let mut after = before.clone();
            after.diagnostics.quote_max_pay_exceeded =
                after.diagnostics.quote_max_pay_exceeded.saturating_add(1);
            after.diagnostics.last_quote_failure = Some(rejected.clone());
            Ok(FeeEffects {
                after_fee_state: after,
                payer_after: payer_before,
                quote: None,
                failure: Some(rejected),
                journal: None,
                clear_clearing_candidates: false,
            })
        }
    }
}

#[cfg(test)]
mod tests;
