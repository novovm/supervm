mod native_treasury_transition_tests {
    use super::*;
    // Frozen pre-extraction branch: test oracle only, not a production route.
    fn legacy_deposit(
        request: &NovExecutionRequestV1,
        settled_fee: &NovSettledFeeV1,
        subject_meta: &NovExecutionSubjectMetaV1,
        store: &mut NovNativeExecutionStoreV1,
        now_ms: u128,
    ) -> NovNativeExecutionReceiptV1 {
        let args_json = decode_execute_args_json_v1(&request.args)
            .unwrap_or_else(|| fallback_execute_args_value_v1(&request.args));
        let asset = args_json
            .get("asset")
            .and_then(|value| value.as_str())
            .map(normalize_asset_symbol_v1)
            .unwrap_or_else(|| normalize_asset_symbol_v1(request.fee_pay_asset.as_str()));
        let amount = args_json
            .get("amount")
            .and_then(parse_u128_from_json_value_v1)
            .unwrap_or_else(|| request.fee_max_pay_amount.max(1));
        if asset != "NOV" {
            if let Some(reason) =
                reserve_proof_block_reason_for_asset_v1(store, asset.as_str(), now_ms)
            {
                increment_settlement_failure_v1(store, "reserve_proof_not_active");
                return build_failed_native_receipt_v1(
                    request,
                    settled_fee,
                    subject_meta,
                    "treasury".to_string(),
                    "deposit_reserve".to_string(),
                    fee_settlement_reason_v1("reserve_proof_not_active", reason.as_str()),
                );
            }
            let current_reserve = store
                .module_state
                .treasury_reserves
                .get(asset.as_str())
                .copied()
                .unwrap_or(0);
            let projected_reserve_after = current_reserve.saturating_add(amount);
            if let Some(reason) = reserve_proof_capacity_block_reason_v1(
                store,
                asset.as_str(),
                projected_reserve_after,
                now_ms,
            ) {
                increment_settlement_failure_v1(store, "reserve_proof_capacity_exceeded");
                return build_failed_native_receipt_v1(
                    request,
                    settled_fee,
                    subject_meta,
                    "treasury".to_string(),
                    "deposit_reserve".to_string(),
                    fee_settlement_reason_v1("reserve_proof_capacity_exceeded", reason.as_str()),
                );
            }
        }
        let reserve_entry = store
            .module_state
            .treasury_reserves
            .entry(asset.clone())
            .or_insert(0);
        *reserve_entry = reserve_entry.saturating_add(amount);
        let log = NovNativeExecutionLogV1 {
            module: "treasury".to_string(),
            method: "deposit_reserve".to_string(),
            event: "treasury.reserve_deposited".to_string(),
            data: serde_json::json!({
                "asset": asset,
                "amount": amount,
                "reserve_after": *reserve_entry,
                "fee_route": settled_fee.route,
            }),
        };
        build_success_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            "treasury",
            "deposit_reserve",
            vec![log],
        )
    }
    fn request(args: Vec<u8>) -> NovExecutionRequestV1 {
        NovExecutionRequestV1 {
            tx_hash: [7; 32],
            chain_id: 1,
            caller: vec![7; 20],
            target: NovExecutionRequestTargetV1::NativeModule("treasury".into()),
            method: "deposit_reserve".into(),
            args,
            fee_pay_asset: "NOV".into(),
            fee_max_pay_amount: 0,
            fee_slippage_bps: 0,
            gas_like_limit: None,
            nonce: 0,
        }
    }
    fn compare(mut state: NovNativeExecutionStoreV1, request: NovExecutionRequestV1) {
        let args: serde_json::Value = serde_json::from_slice(&request.args).unwrap();
        let asset = normalize_asset_symbol_v1(args["asset"].as_str().unwrap());
        let amount = parse_u128_from_json_value_v1(&args["amount"]).unwrap();
        let account = fallback_execution_subject_meta_v1(&request).account_id;
        credit_native_account_asset_balance_v1(&mut state, &account, &asset, amount);
        let mut old = state.clone();
        let mut new = state;
        let fee = unresolved_settled_fee_v1(&request);
        let subject = fallback_execution_subject_meta_v1(&request);
        if amount == 0 {
            let before = serde_json::to_vec(&new).unwrap();
            let receipt = dispatch_native_module_execute_v1(&request, &fee, &subject, &mut new, 10);
            assert!(!receipt.status);
            assert_eq!(serde_json::to_vec(&new).unwrap(), before);
            return;
        }
        let mut a = legacy_deposit(&request, &fee, &subject, &mut old, 10);
        // V2 deliberately changes conservation and receipt semantics. The
        // frozen V1 oracle only supplies unchanged reserve proof checks.
        if a.status {
            debit_native_account_asset_balance_v1(&mut old, &account, &asset, amount).unwrap();
            let data = a.logs[0].data.as_object_mut().unwrap();
            data.insert(
                "funding_source".into(),
                serde_json::json!("native_account_balance"),
            );
            data.insert("source_account".into(), serde_json::json!(account));
            data.insert("source_balance_after".into(), serde_json::json!(0));
        }
        let b = dispatch_native_module_execute_v1(&request, &fee, &subject, &mut new, 10);
        assert_eq!(
            serde_json::to_vec(&a).unwrap(),
            serde_json::to_vec(&b).unwrap()
        );
        assert_eq!(
            serde_json::to_vec(&old).unwrap(),
            serde_json::to_vec(&new).unwrap()
        );
        assert_eq!(
            native_semantic_ledger_state_digest_v1(&old.module_state),
            native_semantic_ledger_state_digest_v1(&new.module_state)
        );
        assert_eq!(
            full_native_receipt_commitment_v1(&a).unwrap(),
            full_native_receipt_commitment_v1(&b).unwrap()
        );
        assert_eq!(
            native_account_asset_balance_v1(&new, &account, &asset),
            if b.status { 0 } else { amount }
        );
    }
    #[test]
    fn treasury_deposit_transition_preserves_proof_checks_with_v2_accounting() {
        for asset in ["NOV", "USDT"] {
            for status in [
                "active",
                " VALID ",
                "review",
                "under_review",
                "disabled",
                "revoked",
                "expired",
                "unknown",
                "",
            ] {
                for expiry in [0, 9, 10, 11] {
                    for (current, amount) in [
                        (0, 0),
                        (0, 7),
                        (7, 1),
                        (u64::MAX as u128 - 1, 1),
                        (u64::MAX as u128, 0),
                    ] {
                        for capacity in [None, Some(7), Some(u64::MAX as u128)] {
                            let mut state = NovNativeExecutionStoreV1::default();
                            state
                                .module_state
                                .treasury_reserves
                                .insert(asset.into(), current);
                            if let Some(capacity) = capacity {
                                let proof: NovTreasuryReserveProofV1 = serde_json::from_value(serde_json::json!({
                                    "asset": asset, "reserve_amount": 0, "proof_type": "fixture",
                                    "proof_digest": "fixture-digest", "proof_source": "fixture-source",
                                    "proof_reference": "fixture-reference", "status": status
                                })).unwrap();
                                state.module_state.treasury_reserve_proofs.insert(
                                    asset.into(),
                                    NovTreasuryReserveProofV1 {
                                        reserve_amount: capacity,
                                        expires_at_unix_ms: expiry,
                                        ..proof
                                    },
                                );
                            }
                            let args = serde_json::to_vec(
                                &serde_json::json!({"asset": asset, "amount": amount.to_string()}),
                            )
                            .unwrap();
                            compare(state, request(args));
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn treasury_deposit_transition_rejects_unencodable_amounts_before_reserve_mutation() {
        for asset in ["NOV", "USDT"] {
            for (current, amount) in [
                (0, u64::MAX as u128 + 1),
                (u64::MAX as u128, 1),
                (1, u64::MAX as u128),
                (0, u128::MAX),
            ] {
                let mut state = NovNativeExecutionStoreV1::default();
                state
                    .module_state
                    .treasury_reserves
                    .insert(asset.into(), current);
                let mut expected = state.clone();
                increment_settlement_failure_v1(&mut expected, "reserve_encoding_limit_exceeded");
                let req = request(
                    serde_json::to_vec(&serde_json::json!({
                        "asset": asset, "amount": amount.to_string()
                    }))
                    .unwrap(),
                );
                let receipt = dispatch_native_module_execute_v1(
                    &req,
                    &unresolved_settled_fee_v1(&req),
                    &fallback_execution_subject_meta_v1(&req),
                    &mut state,
                    10,
                );
                assert!(!receipt.status);
                assert!(receipt.logs.is_empty());
                assert!(receipt
                    .failure_reason
                    .as_deref()
                    .unwrap()
                    .contains("reserve_encoding_limit_exceeded"));
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&expected).unwrap()
                );
                assert_eq!(
                    native_semantic_ledger_state_digest_v1(&state.module_state),
                    native_semantic_ledger_state_digest_v1(&expected.module_state)
                );
                full_native_receipt_commitment_v1(&receipt).unwrap();
                let raw =
                    native_module_state_shard_value_v1(&state.module_state, "treasury").unwrap();
                let mut recovered = NovNativeExecutionModuleStateV1::default();
                native_apply_module_state_shard_v1(&mut recovered, "treasury", &raw).unwrap();
                assert_eq!(
                    native_module_state_shard_value_v1(&recovered, "treasury").unwrap(),
                    raw
                );
            }
        }
    }

    #[test]
    fn treasury_deposit_transition_requires_callers_own_funds_and_conserves_total() {
        for asset in ["NOV", "USDT"] {
            let mut state = NovNativeExecutionStoreV1::default();
            let req = request(
                serde_json::to_vec(&serde_json::json!({
                    "asset": asset, "amount": 7,
                    "source_account": "victim", "proof_digest": "forged"
                }))
                .unwrap(),
            );
            let subject = fallback_execution_subject_meta_v1(&req);
            credit_native_account_asset_balance_v1(&mut state, "victim", asset, 100);
            let execute = |state: &mut NovNativeExecutionStoreV1| {
                dispatch_native_module_execute_v1(
                    &req,
                    &unresolved_settled_fee_v1(&req),
                    &subject,
                    state,
                    10,
                )
            };
            let denied = execute(&mut state);
            assert!(!denied.status);
            assert!(denied
                .failure_reason
                .unwrap()
                .contains("reserve_deposit_insufficient_balance"));
            assert!(!state.module_state.treasury_reserves.contains_key(asset));
            assert!(!state
                .module_state
                .account_asset_balances
                .contains_key(&subject.account_id));
            assert_eq!(
                native_account_asset_balance_v1(&state, "victim", asset),
                100
            );
            credit_native_account_asset_balance_v1(&mut state, &subject.account_id, asset, 7);
            let receipt = execute(&mut state);
            assert!(receipt.status);
            assert_eq!(
                native_account_asset_balance_v1(&state, &subject.account_id, asset),
                0
            );
            assert_eq!(state.module_state.treasury_reserves[asset], 7);
            assert_eq!(
                receipt.logs[0].data["funding_source"],
                "native_account_balance"
            );
            assert_eq!(
                native_account_asset_balance_v1(&state, "victim", asset),
                100
            );
            // Even a direct repeated dispatch cannot manufacture new reserves.
            // Durable nonce replay protection is a separate ingress obligation.
            assert!(!execute(&mut state).status);
            assert_eq!(state.module_state.treasury_reserves[asset], 7);
        }
    }

    #[test]
    fn treasury_deposit_transition_manual_attestation_is_not_funding() {
        let mut state = NovNativeExecutionStoreV1::default();
        let mut req = request(
            serde_json::to_vec(&serde_json::json!({
                "asset": "USDT", "reserve_amount": 100, "proof_digest": "manual-report"
            }))
            .unwrap(),
        );
        req.target = NovExecutionRequestTargetV1::NativeModule("governance".into());
        req.method = "set_reserve_proof".into();
        let subject = fallback_execution_subject_meta_v1(&req);
        with_env_override_v1(NOV_NATIVE_GOVERNANCE_ENABLED_ENV, "true", || {
            with_env_override_v1(
                NOV_NATIVE_GOVERNANCE_ALLOWLIST_ENV,
                &to_hex(&req.caller),
                || {
                    let receipt = dispatch_native_module_execute_v1(
                        &req,
                        &unresolved_settled_fee_v1(&req),
                        &subject,
                        &mut state,
                        10,
                    );
                    assert!(receipt.status);
                    assert_eq!(receipt.logs[0].data["automated_verification"], false);
                    assert!(state.module_state.treasury_reserves.is_empty());
                    assert!(state.module_state.account_asset_balances.is_empty());
                },
            );
        });
        req.target = NovExecutionRequestTargetV1::NativeModule("treasury".into());
        req.method = "deposit_reserve".into();
        req.args = br#"{"asset":"USDT","amount":7}"#.to_vec();
        let receipt = dispatch_native_module_execute_v1(
            &req,
            &unresolved_settled_fee_v1(&req),
            &subject,
            &mut state,
            10,
        );
        assert!(!receipt.status);
        assert!(receipt
            .failure_reason
            .unwrap()
            .contains("reserve_deposit_insufficient_balance"));
        assert!(state.module_state.treasury_reserves.is_empty());
        assert!(state.module_state.account_asset_balances.is_empty());
    }

    #[test]
    fn treasury_deposit_transition_persists_debit_and_reserve_together() {
        with_test_native_execution_store_path_v1(|path| {
            let mut req = request(br#"{"asset":"USDT","amount":7}"#.to_vec());
            req.fee_max_pay_amount = 10_000;
            let account = fallback_execution_subject_meta_v1(&req).account_id;
            let mut seed = NovNativeExecutionStoreV1::default();
            credit_native_account_asset_balance_v1(&mut seed, &account, "USDT", 10);
            save_nov_native_execution_store_v1(path.as_path(), &seed).unwrap();
            let receipt =
                dispatch_and_persist_nov_execution_request_with_store_path_v1(path.as_path(), &req)
                    .unwrap();
            assert!(receipt.status, "{:?}", receipt.failure_reason);
            let recovered = load_nov_native_execution_store_v1(path.as_path()).unwrap();
            assert_eq!(
                native_account_asset_balance_v1(&recovered, &account, "USDT"),
                3
            );
            assert_eq!(recovered.module_state.treasury_reserves["USDT"], 7);
            // Internal dispatcher bypasses authenticated nonce admission.
            // This verifies storage read-back, not replay via that API.
            assert!(recovered.receipts[&receipt.tx_hash].status);
            assert_eq!(
                recovered,
                load_nov_native_execution_store_v1(path.as_path()).unwrap()
            );
        });
    }

    #[test]
    fn treasury_deposit_transition_rejects_legacy_argument_fallbacks() {
        for args in [
            vec![],
            b"invalid json".to_vec(),
            b"null".to_vec(),
            br#"{"asset":" usdt ","amount":0}"#.to_vec(),
            br#"{"asset":"","amount":"bad"}"#.to_vec(),
            br#"{"amount":-1}"#.to_vec(),
            br#"{"asset":"NOV","amount":18446744073709551616}"#.to_vec(),
            br#"{"asset":"NOV","amount":1.5}"#.to_vec(),
        ] {
            let mut state = NovNativeExecutionStoreV1::default();
            let req = request(args);
            let before = serde_json::to_vec(&state).unwrap();
            let receipt = dispatch_native_module_execute_v1(
                &req,
                &unresolved_settled_fee_v1(&req),
                &fallback_execution_subject_meta_v1(&req),
                &mut state,
                10,
            );
            assert!(!receipt.status);
            assert!(receipt
                .failure_reason
                .unwrap()
                .contains("reserve_deposit_args_invalid"));
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}
