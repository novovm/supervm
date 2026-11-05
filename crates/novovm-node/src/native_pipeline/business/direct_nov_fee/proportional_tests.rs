use super::*;

fn policy() -> DirectNovFeePolicy {
    let mut p = super::tests::policy();
    p.policy_version = 3;
    p.amount_fee_v3 = Some(ProportionalFeeV3 {
        minimum_fee_units: 0,
        maximum_fee_units: u128::from(u64::MAX),
        rate_bps: 10,
        rounding: FeeRoundingV2::Ceil,
        service_share_bps: 6234,
        network_share_bps: 3766,
        split_remainder: SplitRemainderV2::Service,
    });
    p.reserve_share_bps = 0;
    p.fee_share_bps = 0;
    p.risk_buffer_share_bps = 0;
    p.min_risk_buffer_nov = 0;
    p.clearing_enabled = false;
    p.redeem_paused = true;
    p
}

fn request(amount: u128) -> TransferFeeRequest {
    TransferFeeRequest {
        tx_hash: [7; 32],
        payer: [1; 20].into(),
        recipient: [2; 32].into(),
        asset: "NOV".into(),
        amount,
        pay_asset: "NOV".into(),
        max_pay_amount: u128::MAX,
        slippage_bps: 0,
    }
}

#[test]
fn amount_rate_and_explicit_rounding_match_independent_integer_reference() {
    let mut p = policy();
    for rate in [1, 10, 20, 10000] {
        for rounding in [FeeRoundingV2::Floor, FeeRoundingV2::Ceil] {
            let model = p.amount_fee_v3.as_mut().unwrap();
            model.rate_bps = rate;
            model.rounding = rounding;
            for amount in [0, 1, 499, 500, 999, 1000, 1001, 100_000_000_000] {
                let product = amount * u128::from(rate);
                let expected = match rounding {
                    FeeRoundingV2::Floor => product / 10000,
                    FeeRoundingV2::Ceil => product.div_ceil(10000),
                };
                let quote = quote_transfer(&request(amount), &p, 1).unwrap().unwrap();
                assert_eq!(quote.nov_amount, expected);
                quote.validate().unwrap();
            }
        }
    }
}

#[test]
fn representable_amount_fee_does_not_fail_on_intermediate_product_overflow() {
    let mut p = policy();
    p.amount_fee_v3.as_mut().unwrap().maximum_fee_units = u128::MAX;
    for rate in [1, 10, 20, 10000] {
        p.amount_fee_v3.as_mut().unwrap().rate_bps = rate;
        let q = quote_transfer(&request(u128::MAX), &p, 1).unwrap().unwrap();
        q.validate().unwrap();
        if rate == 10000 {
            assert_eq!(q.nov_amount, u128::MAX);
        }
    }
    p.amount_fee_v3.as_mut().unwrap().rate_bps = 10;
    assert_eq!(
        quote_transfer(&request(u128::MAX), &p, 1)
            .unwrap()
            .unwrap()
            .nov_amount,
        u128::MAX / 1000 + u128::from(!u128::MAX.is_multiple_of(1000))
    );
}

#[test]
fn literal_maximum_fee_rejects_without_charging_or_monetary_mutation() {
    let p = policy();
    for cap in [0, 9] {
        let mut r = request(10_000);
        r.max_pay_amount = cap;
        let before = FeeState::default();
        let result = quote_and_settle(&r, &p, &before, 99, 1).unwrap();
        assert_eq!(result.failure.unwrap().code, FeeFailureCode::MaxPayExceeded);
        assert_eq!(result.payer_after, 99);
        assert_eq!(result.after_fee_state.accounting, before.accounting);
        assert!(result.journal.is_none());
    }
    let mut r = request(10_000);
    r.max_pay_amount = 10;
    assert_eq!(
        quote_and_settle(&r, &p, &FeeState::default(), 99, 1)
            .unwrap()
            .charged_fee(),
        10
    );
}

#[test]
fn two_explicit_budgets_conserve_fee_and_allow_zero_share_without_risk_bucket() {
    let mut p = policy();
    for network in [0, 3766, 10000] {
        p.amount_fee_v3.as_mut().unwrap().network_share_bps = network;
        p.amount_fee_v3.as_mut().unwrap().service_share_bps = 10000 - network;
        for remainder in [SplitRemainderV2::Service, SplitRemainderV2::Network] {
            p.amount_fee_v3.as_mut().unwrap().split_remainder = remainder;
            for amount in [1, 999, 1000, 1001, 100_000] {
                let e =
                    quote_and_settle(&request(amount), &p, &FeeState::default(), 1000, 1).unwrap();
                let fee = e.charged_fee();
                let a = &e.after_fee_state.accounting;
                let expected_network = match remainder {
                    SplitRemainderV2::Service => fee * u128::from(network) / 10000,
                    SplitRemainderV2::Network => fee - fee * u128::from(10000 - network) / 10000,
                };
                assert_eq!(a.reserve_bucket_nov, expected_network);
                assert_eq!(a.reserve_bucket_nov + a.fee_bucket_nov, fee);
                assert_eq!(a.risk_buffer_nov, 0);
                assert_eq!(a.treasury_reserve_nov, Some(fee));
                assert_eq!(e.payer_after + fee, 1000);
            }
        }
    }
}

#[test]
fn malformed_or_mixed_amount_policy_never_falls_back_to_legacy() {
    for change in 0..8 {
        let mut p = policy();
        match change {
            0 => p.amount_fee_v3.as_mut().unwrap().rate_bps = 0,
            1 => p.amount_fee_v3.as_mut().unwrap().rate_bps = 10001,
            2 => p.amount_fee_v3.as_mut().unwrap().network_share_bps += 1,
            3 => p.reserve_share_bps = 1,
            4 => p.policy_version = 1,
            5 => p.policy_version = 2,
            6 => p.amount_fee_v3.as_mut().unwrap().maximum_fee_units = 0,
            _ => {
                let m = p.amount_fee_v3.as_mut().unwrap();
                m.minimum_fee_units = 5;
                m.maximum_fee_units = 4;
            }
        }
        assert!(quote_transfer(&request(1), &p, 1).is_err());
    }
    for field in ["rounding", "minimum_fee_units", "maximum_fee_units"] {
        let mut missing = serde_json::to_value(policy()).unwrap();
        missing["amount_fee_v3"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(serde_json::from_value::<DirectNovFeePolicy>(missing).is_err());
    }
}

#[test]
fn changed_rate_split_basis_or_rounding_is_bound_to_quote_and_effect_contract() {
    let p = policy();
    let r = request(10000);
    let q = quote_transfer(&r, &p, 1).unwrap().unwrap();
    for change in 0..6 {
        let mut changed = p.clone();
        let m = changed.amount_fee_v3.as_mut().unwrap();
        match change {
            0 => m.rate_bps = 20,
            1 => {
                m.network_share_bps = 3000;
                m.service_share_bps = 7000;
            }
            2 => m.rounding = FeeRoundingV2::Floor,
            3 => m.split_remainder = SplitRemainderV2::Network,
            4 => m.minimum_fee_units = 1,
            _ => m.maximum_fee_units = 50,
        }
        assert!(plan_settlement(&r, &q, &changed, &FeeState::default(), 100, 1).is_err());
        assert_ne!(
            super::super::nov_transfer_batch::effect_contract(&p).unwrap(),
            super::super::nov_transfer_batch::effect_contract(&changed).unwrap()
        );
    }
    let mut forged = q.clone();
    forged.basis_amount = Some(1);
    assert!(forged.validate().is_err());
    assert!(plan_settlement(&request(10001), &q, &p, &FeeState::default(), 100, 1).is_err());
}

#[test]
fn candidate_bounds_match_nov_examples_and_exact_base_unit_edges() {
    // Attachment proposal only: these are NOT deployment defaults.
    let mut p = policy();
    let m = p.amount_fee_v3.as_mut().unwrap();
    m.rate_bps = 20;
    m.minimum_fee_units = 2_000_000;
    m.maximum_fee_units = 50_000_000_000;
    for (amount, expected) in [
        (100_000_000, 2_000_000),
        (1_000_000_000, 2_000_000),
        (5_000_000_000, 10_000_000),
        (10_000_000_000, 20_000_000),
        (100_000_000_000, 200_000_000),
        (1_000_000_000_000, 2_000_000_000),
        (10_000_000_000_000, 20_000_000_000),
        (25_000_000_000_000, 50_000_000_000),
        (100_000_000_000_000, 50_000_000_000),
        (1_000_000_000_000_000, 50_000_000_000),
        (999_999_999, 2_000_000),
        (1_000_000_001, 2_000_001),
        (24_999_999_999_500, 49_999_999_999),
        (25_000_000_000_001, 50_000_000_000),
        (u128::MAX, 50_000_000_000),
    ] {
        let q = quote_transfer(&request(amount), &p, 1).unwrap().unwrap();
        assert_eq!(q.nov_amount, expected, "amount={amount}");
        q.validate().unwrap();
    }
}

#[test]
fn bounds_are_not_additive_and_actual_clamped_fee_alone_is_distributed() {
    let mut p = policy();
    let m = p.amount_fee_v3.as_mut().unwrap();
    m.minimum_fee_units = 3;
    m.maximum_fee_units = 5;
    for (amount, fee) in [(1, 3), (4_000, 4), (100_000, 5)] {
        let r = request(amount);
        let e = quote_and_settle(&r, &p, &FeeState::default(), 100, 1).unwrap();
        assert_eq!(e.charged_fee(), fee);
        assert_eq!(e.payer_after, 100 - fee);
        assert_eq!(e.after_fee_state.accounting.settled_nov_total, fee);
        assert_eq!(
            e.after_fee_state.accounting.reserve_bucket_nov
                + e.after_fee_state.accounting.fee_bucket_nov,
            fee
        );
        let mut below = r.clone();
        below.max_pay_amount = fee - 1;
        let rejected = quote_and_settle(&below, &p, &FeeState::default(), 100, 1).unwrap();
        assert_eq!(
            rejected.failure.unwrap().code,
            FeeFailureCode::MaxPayExceeded
        );
        assert_eq!(rejected.payer_after, 100);
        assert!(rejected.journal.is_none());
        below.max_pay_amount = fee;
        assert_eq!(
            quote_and_settle(&below, &p, &FeeState::default(), 100, 1)
                .unwrap()
                .charged_fee(),
            fee
        );
    }
}

#[test]
fn new_bounds_do_not_reprice_existing_quotes_or_select_market_prices() {
    let p = policy();
    let r = request(10000);
    let original = quote_transfer(&r, &p, 1).unwrap().unwrap();
    let mut changed = p.clone();
    let m = changed.amount_fee_v3.as_mut().unwrap();
    m.maximum_fee_units = 5;
    assert_eq!(
        quote_transfer(&r, &changed, 1).unwrap().unwrap().nov_amount,
        5
    );
    assert_eq!(original.nov_amount, 10);
    original.validate().unwrap();
    assert!(plan_settlement(&r, &original, &changed, &FeeState::default(), 100, 1).is_err());
    assert_eq!(
        plan_settlement(&r, &original, &p, &FeeState::default(), 100, 1)
            .unwrap()
            .charged_fee(),
        10
    );
}

#[test]
fn amount_quote_policy_and_state_survive_original_binary_codec() {
    let p = policy();
    let e = quote_and_settle(&request(1001), &p, &FeeState::default(), 100, 1).unwrap();
    let encoded = postcard::to_allocvec(&e.after_fee_state).unwrap();
    let decoded: FeeState = postcard::from_bytes(&encoded).unwrap();
    decoded.validate().unwrap();
    assert_eq!(decoded, e.after_fee_state);
    let bytes = postcard::to_allocvec(&p).unwrap();
    assert_eq!(
        postcard::from_bytes::<DirectNovFeePolicy>(&bytes).unwrap(),
        p
    );
    assert!(postcard::from_bytes::<DirectNovFeePolicy>(&bytes[..bytes.len() - 1]).is_err());
}
