use super::*;
use crate::business::quoted_transfer::{
    compute_outcome, TransferFailure, TransferIntent, TransferSnapshot,
};

fn policy() -> DirectNovFeePolicy {
    DirectNovFeePolicy {
        quote_ttl_ms: 15_000,
        policy_version: 1,
        policy_source: "runtime_path".into(),
        resolution_source: "runtime_state".into(),
        reserve_share_bps: 7000,
        fee_share_bps: 2000,
        risk_buffer_share_bps: 1000,
        min_reserve_bucket_nov: 0,
        min_fee_bucket_nov: 0,
        min_risk_buffer_nov: 1000,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: 10_000,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 100,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    }
}

fn request() -> TransferFeeRequest {
    TransferFeeRequest {
        tx_hash: [0xab; 32],
        payer: Account::from([1; 20]),
        recipient: Account::from([2; 20]),
        asset: "NOV".into(),
        amount: 1,
        pay_asset: "NOV".into(),
        max_pay_amount: 0,
        slippage_bps: 0,
    }
}

fn quote(request: &TransferFeeRequest, policy: &DirectNovFeePolicy, now: u128) -> FeeQuote {
    quote_transfer(request, policy, now).unwrap().unwrap()
}

#[test]
fn quote_preserves_frozen_transfer_projection_cost_and_unsigned_cap_sentinel() {
    let p = policy();
    let mut r = request();
    // Frozen old ordered JSON projections have 78/102 bytes for amount 1 and
    // 116/140 bytes for u128::MAX (20/32-byte destination respectively).
    for (width, amount, expected_fee) in [
        (20, 1, 45),
        (32, 1, 47),
        (20, u128::MAX, 48),
        (32, u128::MAX, 49),
    ] {
        r.recipient = Account::try_from(vec![2; width]).unwrap();
        r.amount = amount;
        let q = quote(&r, &p, 16);
        assert_eq!(q.nov_amount, expected_fee);
        assert_eq!(q.quoted_pay_amount, expected_fee);
        assert_eq!(q.max_pay_amount, expected_fee);
        assert_eq!(q.quote_id, "q-abababababab-10");
        assert_eq!(q.expires_at_unix_ms, 15_016);
        q.validate().unwrap();
    }
    r = request();
    for slippage in [0, 1, 50, 9999, 10_000, u32::MAX] {
        r.slippage_bps = slippage;
        let q = quote(&r, &p, 0);
        let expected = (45 * (10_000 + u128::from(slippage.min(10_000)))).div_ceil(10_000);
        assert_eq!(q.quoted_pay_amount_with_slippage, expected);
        assert_eq!(q.max_pay_amount, expected);
        r.max_pay_amount = expected;
        assert_eq!(quote(&r, &p, 0), q);
        r.max_pay_amount = expected - 1;
        let failure = quote_transfer(&r, &p, 0).unwrap().unwrap_err();
        assert_eq!(failure.code, FeeFailureCode::MaxPayExceeded);
        assert_eq!(failure.to_string(), format!("fee.quote.max_pay_exceeded: required_with_slippage={expected} max_pay_amount={} pay_asset=NOV", expected - 1));
        r.max_pay_amount = 0;
    }
}

#[test]
fn asset_aliases_preserve_wire_projection_and_other_assets_fail_without_effects() {
    let p = policy();
    let original = request();
    let expected = quote(&original, &p, 9);
    for alias in ["NOV", "nov", " NoV ", "", " \t\n"] {
        let mut r = original.clone();
        r.asset = alias.into();
        r.pay_asset = alias.into();
        let before = r.clone();
        assert_eq!(quote(&r, &p, 9), expected);
        assert_eq!(r, before);
    }
    for field in [0, 1] {
        let mut r = original.clone();
        if field == 0 {
            r.asset = "USDT".into();
        } else {
            r.pay_asset = "USDT".into();
        }
        let state = FeeState::default();
        assert!(quote_and_settle(&r, &p, &state, 1000, 1).is_err());
        assert_eq!(state, FeeState::default());
    }
}

#[test]
fn quote_failure_does_not_refresh_day_or_erase_previous_successful_quote() {
    let mut p = policy();
    p.resolution_source = "default_fallback_partial_env".into();
    p.settlement_paused = true;
    let mut r = request();
    let mut state = FeeState::default();
    state.accounting.daily_window_day = 3;
    state.accounting.daily_nov_used = 999;
    state.diagnostics.last_quote = Some(quote(&r, &p, 7));
    let before = state.clone();
    r.max_pay_amount = 1;
    let result = quote_and_settle(&r, &p, &state, 1000, 9 * DAY_MS).unwrap();
    let mut expected = before.clone();
    expected.diagnostics.quote_max_pay_exceeded = 1;
    expected.diagnostics.last_quote_failure = result.failure.clone();
    assert_eq!(result.after_fee_state, expected);
    assert_eq!(result.payer_after, 1000);
    assert_eq!(result.charged_fee(), 0);
    assert!(result.quote.is_none() && result.journal.is_none());
    assert!(!result.clear_clearing_candidates);
    assert_eq!(state, before);
}

#[test]
fn paused_settlement_refreshes_window_and_quote_before_rejecting_and_precedes_expiry() {
    let mut p = policy();
    p.settlement_paused = true;
    p.quote_ttl_ms = 1;
    p.resolution_source = "default_fallback_invalid_env".into();
    let r = request();
    let q = quote(&r, &p, 2);
    let mut before = FeeState::default();
    before.accounting.daily_nov_used = 999;
    before.diagnostics.last_quote_failure =
        Some(failure(FeeFailureCode::MaxPayExceeded, "old failure"));
    let result = plan_settlement(&r, &q, &p, &before, 0, DAY_MS).unwrap();
    assert_eq!(
        result.failure.as_ref().unwrap().code,
        FeeFailureCode::SettlementPaused
    );
    let mut expected = before;
    expected.accounting.daily_window_day = 1;
    expected.accounting.daily_nov_used = 0;
    expected.diagnostics.last_quote = Some(q);
    expected.diagnostics.last_quote_failure = None;
    expected.diagnostics.settlement_policy_fallback = 1;
    expected.diagnostics.settlement_paused = 1;
    assert_eq!(result.after_fee_state, expected);
    assert_eq!(result.charged_fee(), 0);
    assert!(result.journal.is_none());
}

#[test]
fn quote_expiry_is_strict_and_zero_ttl_and_synthetic_extremes_preserve_old_rules() {
    let mut p = policy();
    p.quote_ttl_ms = 0;
    let r = request();
    let q = quote(&r, &p, 8);
    assert_eq!(q.expires_at_unix_ms, 9);
    let at_deadline = plan_settlement(&r, &q, &p, &FeeState::default(), 1000, 9).unwrap();
    assert_eq!(at_deadline.charged_fee(), q.nov_amount);
    let late = plan_settlement(&r, &q, &p, &FeeState::default(), 0, 10).unwrap();
    assert_eq!(
        late.failure.as_ref().unwrap().code,
        FeeFailureCode::QuoteExpired
    );
    assert_eq!(late.after_fee_state.diagnostics.clearing_quote_expired, 1);
    assert_eq!(
        late.after_fee_state
            .diagnostics
            .clearing_insufficient_user_balance,
        0
    );
    assert_eq!(
        late.after_fee_state
            .diagnostics
            .last_clearing_failure
            .as_ref()
            .unwrap()
            .unix_ms,
        10
    );
    assert!(late.journal.is_none());
    p.quote_ttl_ms = u128::MAX;
    let q = quote(&r, &p, u128::MAX);
    assert_eq!(q.expires_at_unix_ms, u128::MAX);
    let extreme = plan_settlement(&r, &q, &p, &FeeState::default(), 1000, u128::MAX).unwrap();
    assert!(extreme.failure.is_none());
    assert_eq!(
        extreme.after_fee_state.accounting.daily_window_day,
        (u128::MAX / DAY_MS) as u64
    );
}

#[test]
fn insufficient_payer_precedes_capacity_failure_and_does_not_insert_absent_treasury_leaves() {
    let p = policy();
    let r = request();
    let mut before = FeeState::default();
    before.accounting.settlements = u64::MAX;
    let result = quote_and_settle(&r, &p, &before, 44, 1).unwrap();
    let rejected = result.failure.as_ref().unwrap();
    assert_eq!(rejected.code, FeeFailureCode::InsufficientUserBalance);
    assert_eq!(rejected.to_string(), format!("fee.clearing.insufficient_user_balance: asset=NOV nov_fee_asset_debit_failed: account=0x{} requested=45 available=44", "01".repeat(20)));
    assert_eq!(result.after_fee_state.accounting, before.accounting);
    assert_eq!(
        result
            .after_fee_state
            .diagnostics
            .clearing_insufficient_user_balance,
        1
    );
    assert_eq!(
        result
            .after_fee_state
            .diagnostics
            .settlement_amount_overflow,
        0
    );
    assert_eq!(result.payer_after, 44);
    assert!(result.journal.is_none());
}

#[test]
fn every_monetary_and_sequence_overflow_is_checked_before_any_debit() {
    let p = policy();
    let r = request();
    for field in 0..9 {
        let mut before = FeeState::default();
        match field {
            0 => before.accounting.treasury_reserve_nov = Some(u128::MAX),
            1 => before.accounting.settled_nov_total = u128::MAX,
            2 => before.accounting.settled_by_asset_nov = Some(u128::MAX),
            3 => before.accounting.reserve_bucket_nov = u128::MAX,
            4 => before.accounting.fee_bucket_nov = u128::MAX,
            5 => before.accounting.risk_buffer_nov = u128::MAX,
            6 => {
                before.accounting.reserve_bucket_nov = u128::MAX - 100;
                before.accounting.fee_bucket_nov = 100;
            }
            7 => before.accounting.settlements = u64::MAX,
            8 => before.accounting.journal_next_seq = u64::MAX,
            _ => unreachable!(),
        }
        before.accounting.daily_nov_used = 78;
        let saved = before.clone();
        let result = quote_and_settle(&r, &p, &before, u128::MAX, DAY_MS).unwrap();
        assert_eq!(
            result.failure.as_ref().unwrap().code,
            FeeFailureCode::AmountOverflow,
            "field {field}"
        );
        let mut expected = before.accounting.clone();
        expected.daily_window_day = 1;
        expected.daily_nov_used = 0;
        assert_eq!(result.after_fee_state.accounting, expected, "field {field}");
        assert_eq!(
            result
                .after_fee_state
                .diagnostics
                .settlement_amount_overflow,
            1
        );
        assert_eq!(result.payer_after, u128::MAX);
        assert_eq!(result.charged_fee(), 0);
        assert!(result.journal.is_none());
        assert_eq!(before, saved);
    }
}

#[test]
fn exact_u128_and_sequence_maxima_are_valid_without_truncating_money() {
    let p = policy();
    let r = request();
    let mut before = FeeState::default();
    before.accounting.treasury_reserve_nov = Some(u128::MAX - 45);
    before.accounting.settled_nov_total = u128::MAX - 45;
    before.accounting.settled_by_asset_nov = Some(u128::MAX - 45);
    before.accounting.reserve_bucket_nov = u128::MAX - 45;
    before.accounting.settlements = u64::MAX - 1;
    before.accounting.journal_next_seq = u64::MAX - 1;
    let result = quote_and_settle(&r, &p, &before, 45, 0).unwrap();
    assert!(result.failure.is_none());
    let after = &result.after_fee_state.accounting;
    assert_eq!(after.treasury_reserve_nov, Some(u128::MAX));
    assert_eq!(after.settled_nov_total, u128::MAX);
    assert_eq!(after.settled_by_asset_nov, Some(u128::MAX));
    assert_eq!(
        after.reserve_bucket_nov + after.fee_bucket_nov + after.risk_buffer_nov,
        u128::MAX
    );
    assert_eq!(after.settlements, u64::MAX);
    assert_eq!(after.journal_next_seq, u64::MAX);
    assert_eq!(result.payer_after, 0);
    assert_eq!(result.journal.as_ref().unwrap().seq, u64::MAX);
}

#[test]
fn each_fee_rounds_separately_risk_gets_remainder_and_nov_bypasses_non_nov_gates() {
    let mut p = policy();
    p.clearing_enabled = false;
    p.clearing_require_healthy_risk_buffer = true;
    p.clearing_daily_nov_hard_limit = 1;
    p.clearing_constrained_strategy = "blocked".into();
    let r = request();
    let mut state = FeeState::default();
    state.accounting.daily_nov_used = 999;
    let mut payer = 1000;
    for sequence in 1..=2 {
        let result = quote_and_settle(&r, &p, &state, payer, 0).unwrap();
        assert!(result.failure.is_none());
        let entry = result.journal.as_ref().unwrap();
        assert_eq!(
            [
                entry.reserve_bucket_delta_nov,
                entry.fee_bucket_delta_nov,
                entry.risk_buffer_delta_nov
            ],
            [31, 9, 5]
        );
        assert_eq!(entry.policy_threshold_state, ThresholdState::Blocked);
        assert_eq!(entry.seq, sequence);
        assert_eq!(result.after_fee_state.accounting.daily_nov_used, 999);
        assert!(result.clear_clearing_candidates);
        payer = result.payer_after;
        state = result.after_fee_state;
    }
    assert_eq!(state.accounting.reserve_bucket_nov, 62); // not floor(90 * .7) = 63
    assert_eq!(state.accounting.fee_bucket_nov, 18);
    assert_eq!(state.accounting.risk_buffer_nov, 10);
    assert_eq!(payer + state.accounting.treasury_reserve_nov.unwrap(), 1000);
    assert_eq!(state.accounting.settled_nov_total, 90);
    assert_eq!(state.accounting.settled_by_asset_nov, Some(90));
    // These are accounting views of the same 90, not three new supplies.
    assert_eq!(
        state.accounting.reserve_bucket_nov
            + state.accounting.fee_bucket_nov
            + state.accounting.risk_buffer_nov,
        90
    );
}

#[test]
fn successful_fee_preserves_old_clearing_failure_and_saturating_diagnostic_counts() {
    let mut p = policy();
    p.resolution_source = "default_fallback_invalid_env".into();
    let r = request();
    let first = quote_and_settle(&r, &p, &FeeState::default(), 0, 0).unwrap();
    let mut state = first.after_fee_state;
    state.diagnostics.settlement_policy_fallback = u64::MAX;
    let old_failure = state.diagnostics.last_clearing_failure.clone();
    let success = quote_and_settle(&r, &p, &state, 1000, 1).unwrap();
    assert_eq!(
        success.after_fee_state.diagnostics.last_clearing_failure,
        old_failure
    );
    assert_eq!(
        success
            .after_fee_state
            .diagnostics
            .settlement_policy_fallback,
        u64::MAX
    );
    assert_eq!(
        success
            .after_fee_state
            .diagnostics
            .clearing_insufficient_user_balance,
        1
    );
    assert_eq!(
        success.after_fee_state.diagnostics.last_clearing_candidates,
        []
    );
    let mut capped = r;
    capped.max_pay_amount = 1;
    state.diagnostics.quote_max_pay_exceeded = u64::MAX;
    assert_eq!(
        quote_and_settle(&capped, &p, &state, 1000, 0)
            .unwrap()
            .after_fee_state
            .diagnostics
            .quote_max_pay_exceeded,
        u64::MAX
    );
}

#[test]
fn prepared_quotes_validate_every_fee_metadata_field_before_any_effects() {
    let p = policy();
    let r = request();
    let q = quote(&r, &p, 7);
    let bytes = postcard::to_allocvec(&q).unwrap();
    let decoded: FeeQuote = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(
        plan_settlement(&r, &decoded, &p, &FeeState::default(), 1000, 7)
            .unwrap()
            .charged_fee(),
        45
    );
    for field in 0..14 {
        let mut bad = decoded.clone();
        match field {
            0 => bad.quote_id.push('0'),
            1 => bad.pay_asset = "USDT".into(),
            2 => bad.nov_amount += 1,
            3 => bad.quoted_pay_amount += 1,
            4 => bad.quoted_pay_amount_with_slippage += 1,
            5 => bad.max_pay_amount += 1,
            6 => bad.slippage_bps += 1,
            7 => bad.quoted_at_unix_ms += 1,
            8 => bad.expires_at_unix_ms += 1,
            9 => bad.rate_ppm += 1,
            10 => bad.oracle_updated_at_unix_ms += 1,
            11 => bad.route.push('x'),
            12 => bad.quote_contract.push('x'),
            13 => bad.price_source.push('x'),
            _ => unreachable!(),
        }
        assert!(
            plan_settlement(&r, &bad, &p, &FeeState::default(), 1000, 7).is_err(),
            "field {field}"
        );
    }
    let mut wrong_request = r.clone();
    wrong_request.tx_hash[0] ^= 1;
    assert!(plan_settlement(&wrong_request, &q, &p, &FeeState::default(), 1000, 7).is_err());
    let mut wrong_policy = p;
    wrong_policy.quote_ttl_ms += 1;
    assert!(plan_settlement(&r, &q, &wrong_policy, &FeeState::default(), 1000, 7).is_err());
}

#[test]
fn policy_and_state_codec_are_explicit_bounded_and_do_not_invent_fallback() {
    let original = policy();
    for field in 0..8 {
        let mut p = original.clone();
        match field {
            0 => p.policy_version = 0,
            1 => p.reserve_share_bps = 0,
            2 => p.risk_buffer_share_bps += 1,
            3 => p.min_risk_buffer_nov = 0,
            4 => p.resolution_source.clear(),
            5 => p.policy_source = "x".repeat(129),
            6 => p.clearing_constrained_daily_usage_bps = 0,
            7 => p.clearing_constrained_strategy = "unknown".into(),
            _ => unreachable!(),
        }
        assert!(p.validate().is_err(), "field {field}");
        assert!(quote_and_settle(&request(), &p, &FeeState::default(), 1000, 0).is_err());
    }
    let mut p = original;
    p.policy_source = " DEFAULT ".into();
    assert_eq!(p.normalized_source(), "config_path");
    assert_eq!(p.contract_id(), "nov_treasury_policy_v1:1:config_path:7000:2000:1000:0:0:1000:0:0:1:10000:0:100:8000:daily_volume_only:0:report_only");
    let encoded_policy = postcard::to_allocvec(&p).unwrap();
    assert!(encoded_policy.len() <= 2048);
    let decoded: DirectNovFeePolicy = postcard::from_bytes(&encoded_policy).unwrap();
    assert_eq!(decoded, p);
    decoded.validate().unwrap();
    let state = quote_and_settle(&request(), &p, &FeeState::default(), 0, 9)
        .unwrap()
        .after_fee_state;
    let encoded_state = postcard::to_allocvec(&state).unwrap();
    assert!(encoded_state.len() <= 8192);
    let decoded: FeeState = postcard::from_bytes(&encoded_state).unwrap();
    assert_eq!(state, decoded);
    decoded.validate().unwrap();
    let mut bad = state;
    bad.diagnostics
        .last_clearing_failure
        .as_mut()
        .unwrap()
        .failure
        .reason = "x".repeat(513);
    assert!(bad.validate().is_err());
}

#[test]
fn journal_effects_are_per_transaction_and_do_not_accumulate_history_in_fee_state() {
    let p = policy();
    let r = request();
    let mut state = FeeState::default();
    let mut tail = std::collections::VecDeque::new();
    for index in 1..=600 {
        let effects = quote_and_settle(&r, &p, &state, 1000, index).unwrap();
        let entry = effects.journal.unwrap();
        assert_eq!(entry.seq, index as u64);
        tail.push_back(entry);
        if tail.len() > JOURNAL_RETAIN_LAST {
            tail.pop_front();
        }
        state = effects.after_fee_state;
    }
    assert_eq!(tail.len(), 512);
    assert_eq!(tail.front().unwrap().seq, 89);
    assert_eq!(state.accounting.journal_next_seq, 600);
    assert_eq!(state.accounting.settlements, 600);
    assert!(postcard::to_allocvec(&state).unwrap().len() < 1024);
}

#[test]
fn business_failure_self_transfer_and_fee_failure_compose_without_a_second_fee_debit() {
    let p = policy();
    for (amount, balance, recipient_balance, same, expected_fee, succeeds) in [
        (17, 1000, 0, false, 45, true),
        (1000, 100, 0, false, 46, false),
        (1, 1000, u128::MAX, false, 45, false),
        (7, 100, 100, true, 45, true),
        (70, 100, 100, true, 45, false),
        (0, 100, 0, false, 45, true),
        (1, 44, 0, false, 0, false),
    ] {
        let mut r = request();
        r.amount = amount;
        if same {
            r.recipient = r.payer.clone();
        }
        let q = quote(&r, &p, 0);
        let intent = TransferIntent {
            tx_hash: r.tx_hash,
            from: r.payer.clone(),
            to: r.recipient.clone(),
            nonce_identity: "authenticated signer".into(),
            nonce: 5,
            amount,
            approved_fee: q.nov_amount,
            fee_cap: q.max_pay_amount,
        };
        let snapshot = TransferSnapshot {
            payer_balance: balance,
            recipient_balance,
            next_nonce: 5,
        };
        let outcome = compute_outcome(&intent, &snapshot, None).unwrap();
        let fee = quote_and_settle(&r, &p, &FeeState::default(), balance, 0).unwrap();
        let outcome = if let Some(rejected) = &fee.failure {
            outcome.reject_fee(rejected.to_string())
        } else {
            outcome
        };
        assert_eq!(outcome.is_success(), succeeds);
        assert_eq!(outcome.delta().nonce_after, 6);
        assert_eq!(outcome.delta().fee_funding_delta, expected_fee);
        assert_eq!(fee.charged_fee(), expected_fee);
        assert_eq!(fee.journal.is_some(), expected_fee != 0);
        if !succeeds && expected_fee > 0 {
            assert!(matches!(
                outcome.failure(),
                Some(TransferFailure::Business(_))
            ));
            assert_eq!(outcome.delta().payer.after, fee.payer_after);
        }
        if succeeds && !same {
            // Apply the absolute arithmetic outcome, NOT fee.payer_after minus
            // the full arithmetic debit (which would charge the fee twice).
            assert_eq!(outcome.delta().payer.after, balance - amount - expected_fee);
        }
        if same {
            assert_eq!(
                outcome.delta().payer.after.checked_add(fee.charged_fee()),
                Some(balance)
            );
        } else {
            // Use checked deltas even when the two pre-state balances cannot
            // fit in one u128 sum (the recipient-overflow failure case).
            let debit = balance.checked_sub(outcome.delta().payer.after).unwrap();
            let credit = outcome
                .delta()
                .recipient
                .after
                .checked_sub(recipient_balance)
                .unwrap();
            assert_eq!(credit.checked_add(fee.charged_fee()), Some(debit));
        }
    }
}

#[test]
fn deterministic_ordered_settlement_matches_small_frozen_reference_across_2048_cases() {
    // Independent monetary reference: original per-tx three shares, checked
    // capacity-before-debit, NOV daily preservation. No production reducer or
    // credit_accounting helper is used to derive expected values.
    let mut random = 0x1234_5678_9abc_def0_u64;
    let mut state = FeeState::default();
    let mut expected = FeeAccounting::default();
    for index in 0..2048 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let mut p = policy();
        p.reserve_share_bps = 1 + (random % 8000) as u32;
        p.fee_share_bps = 1 + ((random >> 16) % (9999 - u64::from(p.reserve_share_bps))) as u32;
        p.risk_buffer_share_bps = 10000 - p.reserve_share_bps - p.fee_share_bps;
        p.settlement_paused = index % 13 == 0;
        let mut r = request();
        r.amount = (random % 10_000) as u128;
        r.slippage_bps = (random >> 32) as u32;
        r.max_pay_amount = if index % 17 == 0 { 1 } else { 0 };
        let now = (index / 53) as u128 * DAY_MS;
        let payer = if index % 11 == 0 { 0 } else { 10_000 };
        let result = quote_and_settle(&r, &p, &state, payer, now).unwrap();
        if r.max_pay_amount != 0 {
            assert_eq!(
                result.failure.as_ref().unwrap().code,
                FeeFailureCode::MaxPayExceeded
            );
        } else {
            if expected.daily_window_day != (now / DAY_MS) as u64 {
                expected.daily_window_day = (now / DAY_MS) as u64;
                expected.daily_nov_used = 0;
            }
            if p.settlement_paused {
                assert_eq!(
                    result.failure.as_ref().unwrap().code,
                    FeeFailureCode::SettlementPaused
                );
            } else if payer == 0 {
                assert_eq!(
                    result.failure.as_ref().unwrap().code,
                    FeeFailureCode::InsufficientUserBalance
                );
            } else {
                // 77 fixed bytes plus decimal amount length in the old projection.
                let amount = 40 + ((77 + r.amount.to_string().len()) as u128).div_ceil(16);
                let reserve = amount * u128::from(p.reserve_share_bps) / 10_000;
                let fee = amount * u128::from(p.fee_share_bps) / 10_000;
                let risk = amount - reserve - fee;
                expected.treasury_reserve_nov =
                    Some(expected.treasury_reserve_nov.unwrap_or(0) + amount);
                expected.settled_nov_total += amount;
                expected.settled_by_asset_nov =
                    Some(expected.settled_by_asset_nov.unwrap_or(0) + amount);
                expected.reserve_bucket_nov += reserve;
                expected.fee_bucket_nov += fee;
                expected.risk_buffer_nov += risk;
                expected.settlements += 1;
                expected.journal_next_seq += 1;
                assert_eq!(result.payer_after, payer - amount);
                assert_eq!(result.charged_fee(), amount);
                assert_eq!(result.journal.as_ref().unwrap().risk_buffer_delta_nov, risk);
            }
        }
        assert_eq!(result.after_fee_state.accounting, expected, "case {index}");
        result.after_fee_state.validate().unwrap();
        state = result.after_fee_state;
    }
}
