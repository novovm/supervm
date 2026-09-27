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
    fn compare(state: NovNativeExecutionStoreV1, request: NovExecutionRequestV1) {
        let mut old = state.clone();
        let mut new = state;
        let fee = unresolved_settled_fee_v1(&request);
        let subject = fallback_execution_subject_meta_v1(&request);
        let before_balances = new.module_state.account_asset_balances.clone();
        let a = legacy_deposit(&request, &fee, &subject, &mut old, 10);
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
            new.module_state.account_asset_balances, before_balances,
            "deposit is reserve accounting, not a newly invented balance transfer"
        );
    }
    #[test]
    fn treasury_deposit_transition_matches_legacy_state_receipt_and_rejections() {
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
    fn treasury_deposit_transition_preserves_argument_fallbacks_and_normalization() {
        for args in [
            vec![],
            b"invalid json".to_vec(),
            b"null".to_vec(),
            br#"{"asset":" usdt ","amount":0}"#.to_vec(),
            br#"{"asset":"","amount":"bad"}"#.to_vec(),
            br#"{"amount":-1}"#.to_vec(),
        ] {
            compare(NovNativeExecutionStoreV1::default(), request(args));
        }
    }
}
