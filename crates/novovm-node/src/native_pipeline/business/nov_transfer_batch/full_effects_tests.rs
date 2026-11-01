use super::*;

// One complete candidate, not a throughput/finality substitute. Run this same
// test in one or four independent processes to isolate shared-device pressure.
#[test]
#[ignore = "explicit AOEM library and Vulkan; isolated complete-plan contention diagnosis"]
fn resident_complete_plan_contention_cost() -> Result<()> {
    let library = std::path::PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY").context("AOEM library required")?,
    );
    let plan = compile(&prepared(1024, false), 172_800_123)?;
    let projection = ExportProjection::new(&plan.exports);
    let initialization = std::time::Instant::now();
    let mut resources = super::super::integer::ResidentTransferResources::new(&library, 4, 1024)?;
    let initialization_us = initialization.elapsed().as_micros();
    let first = std::time::Instant::now();
    let expected = resources.run_effects(&plan.nodes, &projection.unique)?;
    println!(
        "complete_plan_first_use pid={} resource_initialization_us={initialization_us} first_execution_us={}",
        std::process::id(), first.elapsed().as_micros()
    );
    if let Some(directory) = std::env::var_os("NOVOVM_PLAN_DIAGNOSTIC_BARRIER") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::write(
            directory.join(format!("ready-{}", std::process::id())),
            b"ready",
        )?;
        let started = std::time::Instant::now();
        while !directory.join("start").exists() {
            ensure!(
                started.elapsed().as_secs() < 60,
                "diagnostic barrier timeout"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    for iteration in 0..16 {
        let start = std::time::Instant::now();
        let output = resources.run_effects(&plan.nodes, &projection.unique)?;
        let elapsed_us = start.elapsed().as_micros();
        assert_eq!(output, expected, "complete effective state/receipt exports");
        println!(
            "complete_plan_cost pid={} iteration={iteration} elapsed_us={elapsed_us}",
            std::process::id()
        );
    }
    Ok(())
}

#[test]
fn expression_sharing_matches_reference_keys_for_all_program_shapes() {
    // Reference the old key algorithm, including hits and prefix-shaped inputs.
    // Emitted guards/inputs/fallbacks must stay exact.
    let mut old_symbols = BindingSymbols::default();
    let mut old = HashMap::new();
    let mut builder = Builder::default();
    let mut expected = Vec::<AoemIntegerPlanNodeV1>::new();
    let shapes = std::iter::once((10, 5)).chain(super::super::effect_assets::SHAPES);
    for (program, (arity, outputs)) in shapes.enumerate() {
        for seed in [0u128, 1, u128::MAX, 0, 1, u128::MAX] {
            let guard = lit(seed % 2);
            let inputs: Vec<_> = (0..arity)
                .map(|i| lit(seed.wrapping_add(i as u128)))
                .collect();
            let key = (
                program,
                old_symbols.key(&guard),
                inputs
                    .iter()
                    .map(|v| old_symbols.key(v))
                    .collect::<Vec<_>>(),
                outputs,
            );
            let row = *old.entry(key).or_insert_with(|| {
                let row = expected.len() as u32;
                expected.push(AoemIntegerPlanNodeV1 {
                    program,
                    guard,
                    inputs: inputs.clone(),
                    otherwise: vec![lit(0); outputs],
                });
                row
            });
            assert_eq!(
                builder.call(program, &guard, inputs, outputs),
                (0..outputs)
                    .map(|column| V::Output {
                        row,
                        column: column as u32
                    })
                    .collect::<Vec<_>>()
            );
        }
    }
    assert_eq!(format!("{:?}", builder.nodes), format!("{:?}", expected));
    // Different input arities must never alias.
    let a = builder.call(1, &lit(1), vec![V::Output { row: 0, column: 0 }], 1);
    let b = builder.call(1, &lit(1), vec![V::Output { row: 0, column: 0 }; 2], 1);
    assert_ne!(a, b);
}

// Same complete graph/exports, only the existing resource's dispatch tile changes.
// This is an isolated scheduling experiment, NOT finalized transaction TPS.
#[test]
#[ignore = "requires trusted AOEM and Vulkan; complete-plan tile cost experiment"]
fn resident_complete_plan_tile_cost_and_output_equivalence() -> Result<()> {
    let library = std::path::PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .context("explicit trusted AOEM library required")?,
    );
    for diverse in [false, true] {
        let plan = compile(&prepared(1024, diverse), 172_800_123)?;
        let projection = ExportProjection::new(&plan.exports);
        let mut reference = None;
        // Reverse the second sweep to retain ordering/thermal effects rather
        // than select the fastest sample. Resource creation is outside timing.
        for (sweep, tiles) in [[32, 64, 128, 256, 512, 1024], [1024, 512, 256, 128, 64, 32]]
            .into_iter()
            .enumerate()
        {
            for tile in tiles {
                let mut resources =
                    super::super::integer::ResidentTransferResources::with_test_tile(
                        &library, tile,
                    )?;
                let warm = resources.run_effects(&plan.nodes, &projection.unique)?;
                if let Some(expected) = &reference {
                    assert_eq!(&warm, expected);
                } else {
                    reference = Some(warm);
                }
                let start = std::time::Instant::now();
                let output = resources.run_effects(&plan.nodes, &projection.unique)?;
                let elapsed_us = start.elapsed().as_micros();
                assert_eq!(&output, reference.as_ref().unwrap());
                println!(
                    "tile_cost={}",
                    serde_json::json!({"diverse":diverse,"sweep":sweep,"tile":tile,
                    "elapsed_us":elapsed_us,"unique_exports":projection.unique.len(),
                    "static_upper_bound":work(&plan,[tile,tile,1,1,tile,1,tile,tile])})
                );
            }
        }
    }
    Ok(())
}

#[test]
fn compiler_symbols_are_compact_exact_full_width_and_candidate_local() {
    let mut symbols = BindingSymbols::default();
    let zero_key = symbols.key(&lit(0));
    let one_key = symbols.key(&lit(1));
    assert!(zero_key != one_key);
    assert!(
        symbols.literals.is_empty(),
        "identities bypass literal hashing"
    );
    assert!(zero_key == symbols.key(&lit(0)));
    assert!(one_key == symbols.key(&lit(1)));
    let mut wide_symbols = BindingSymbols::default();
    for low in [0, 1] {
        for limb in 1..32 {
            let mut wide = AoemInteger1024V1::from_u128(low);
            wide.0[limb] = 1;
            let key = wide_symbols.key(&V::Literal(wide));
            assert!(key != zero_key && key != one_key);
        }
    }
    let low = lit(7);
    let first = symbols.key(&low);
    assert!(matches!(first, BindingKey::Literal(0)));
    assert!(first == symbols.key(&low));
    for limb in 1..32 {
        let mut wide = AoemInteger1024V1::from_u128(7);
        wide.0[limb] = 9;
        let wide = V::Literal(wide);
        let key = symbols.key(&wide);
        assert!(key != first);
        assert!(key == symbols.key(&wide));
    }
    assert_eq!(symbols.literals.len(), 32);
    assert!(first != symbols.key(&V::Output { row: 0, column: 0 }));
    assert!(
        symbols.key(&V::Output { row: 0, column: 1 })
            != symbols.key(&V::Output { row: 1, column: 0 })
    );
    assert_eq!(
        symbols.literals.len(),
        32,
        "references do not intern values"
    );
    assert!(std::mem::size_of::<BindingKey>() <= 12);
    assert!(BindingSymbols::default().literals.is_empty());
}

#[test]
fn export_projection_preserves_width_reference_identity_order_and_multiplicity() {
    let mut high = AoemInteger1024V1::from_u128(7);
    high.0[31] = 1;
    let exports = vec![
        lit(7),
        V::Output { row: 0, column: 1 },
        lit(7),
        V::Literal(high),
        V::Output { row: 1, column: 1 },
        V::Output { row: 0, column: 0 },
        V::Output { row: 0, column: 1 },
    ];
    let projection = ExportProjection::new(&exports);
    assert_eq!(
        projection.unique,
        vec![exports[0], exports[1], exports[3], exports[4], exports[5]]
    );
    assert_eq!(projection.slots, vec![0, 1, 0, 2, 3, 4, 1]);
    // Synthetic values test projection only; high bits are validated by the
    // existing AOEM decoder/try_to_u128, never truncated by this projection.
    assert_eq!(
        projection.expand(&[7, 11, 13, 17, 19]).unwrap(),
        vec![7, 11, 7, 13, 17, 19, 11]
    );
    assert!(projection.expand(&[7]).is_err());
    assert!(projection.expand(&[7, 11, 13, 17, 19, 23]).is_err());
    let again = ExportProjection::new(&exports);
    assert_eq!(projection.unique, again.unique);
    assert_eq!(projection.slots, again.slots);
    assert!(ExportProjection::new(&[]).expand(&[]).unwrap().is_empty());
    let invalid = ExportProjection {
        unique: vec![lit(1)],
        slots: vec![1],
    };
    assert!(invalid.expand(&[1]).is_err());
}

#[test]
fn pure_sharing_binds_program_guard_full_width_and_dependency_identity() {
    let mut b = Builder::default();
    let a = b.call(1, &lit(1), vec![lit(7), lit(9), lit(0), lit(0)], 1);
    assert_eq!(
        a,
        b.call(1, &lit(1), vec![lit(7), lit(9), lit(0), lit(0)], 1)
    );
    assert_eq!(b.nodes.len(), 1);
    assert_ne!(
        a,
        b.call(1, &lit(0), vec![lit(7), lit(9), lit(0), lit(0)], 1)
    );
    assert_ne!(a, b.call(4, &lit(1), vec![lit(7), lit(9), lit(0)], 1));
    assert_ne!(
        a,
        b.call(1, &lit(1), vec![lit(7), lit(9), lit(0), lit(0)], 2)
    );
    let mut high = AoemInteger1024V1::from_u128(7);
    high.0[31] = 1;
    assert_ne!(
        a,
        b.call(
            1,
            &lit(1),
            vec![V::Literal(high), lit(9), lit(0), lit(0)],
            1
        )
    );
    let reference = V::Output { row: 0, column: 0 };
    let r = b.call(1, &lit(1), vec![reference, lit(1), lit(0), lit(0)], 1);
    for different in [
        V::Output { row: 1, column: 0 },
        V::Output { row: 0, column: 1 },
    ] {
        assert_ne!(
            r,
            b.call(1, &lit(1), vec![different, lit(1), lit(0), lit(0)], 1)
        );
    }
    let sum = b.sum(
        &[a[0], lit(0), lit(0), lit(0)],
        &[a[0], lit(0), lit(0), lit(0)],
    );
    assert_ne!(
        sum[0], a[0],
        "sharing a value must not erase its multiplicity"
    );
    assert!(
        Builder::default().nodes.is_empty(),
        "no cross-candidate cache"
    );
}

// Compiler work fixture only; real authentication/economics/restart are tested
// by body_tests and the original-node RPC load. Never count these as signed TPS.
fn prepared(count: usize, diverse: bool) -> Prepared {
    let recipient = Account::try_from(vec![90; 20]).unwrap();
    let mut p = Prepared {
        policy: super::super::body_tests::policy(),
        requests: Vec::new(),
        identities: Vec::new(),
        tx_nonces: Vec::new(),
        balances: BTreeMap::new(),
        nonces: BTreeMap::new(),
        fees: FeeState::default(),
        components: Vec::new(),
        credit_only: BTreeSet::from([recipient.clone()]),
        spend: None,
    };
    p.balances.insert(recipient.clone(), Some(0));
    for i in 0..count {
        let mut key = [0; 32];
        key[..8].copy_from_slice(&(i as u64).to_le_bytes());
        let mut account = vec![0; 20];
        account[..8].copy_from_slice(&(i as u64).to_le_bytes());
        let payer = Account::try_from(account).unwrap();
        p.balances.insert(
            payer.clone(),
            Some(if diverse { i as u128 * 77 } else { 100_000 }),
        );
        p.nonces.insert(key, 0);
        p.identities.push(key);
        p.tx_nonces.push(0);
        p.components.push(vec![i]);
        p.requests.push(TransferFeeRequest {
            tx_hash: key,
            payer,
            recipient: recipient.clone(),
            asset: "NOV".into(),
            amount: if diverse { i as u128 + 1 } else { 10 },
            pay_asset: "NOV".into(),
            max_pay_amount: if diverse && i % 7 == 0 { 1 } else { 0 },
            slippage_bps: 0,
        });
    }
    p
}

fn unshared(p: &Prepared) -> Compiled {
    compile_with_builder(
        p,
        172_800_123,
        Builder {
            unshared: true,
            ..Builder::default()
        },
    )
    .unwrap()
}

// Mirrors only the static level/group shape in AOEM's existing scheduler.
// This is an upper bound: false guards can remove active rows at execution.
fn work(plan: &Compiled, capacity: [usize; 8]) -> serde_json::Value {
    let mut levels = Vec::<usize>::new();
    let mut groups = BTreeMap::<(usize, usize), usize>::new();
    let mut programs = [0usize; 8];
    for node in &plan.nodes {
        let level = std::iter::once(&node.guard)
            .chain(&node.inputs)
            .chain(&node.otherwise)
            .filter_map(|v| match v {
                V::Literal(_) => None,
                V::Output { row, .. } => Some(levels[*row as usize] + 1),
            })
            .max()
            .unwrap_or(0);
        levels.push(level);
        *groups.entry((level, node.program)).or_default() += 1;
        programs[node.program] += 1;
    }
    let dispatches: usize = groups
        .iter()
        .map(|((_, p), n)| n.div_ceil(capacity[*p]))
        .sum();
    let padded_rows: usize = groups
        .iter()
        .map(|((_, p), n)| n.div_ceil(capacity[*p]) * capacity[*p])
        .sum();
    serde_json::json!({"nodes":plan.nodes.len(),"exports":plan.exports.len(),"nodes_by_program":programs,
        "levels":levels.iter().max().map_or(0, |n| n+1),"dispatch_upper_bound":dispatches,
        "padded_rows_upper_bound":padded_rows,"resource_rows":capacity})
}

#[test]
fn complete_plan_work_shares_values_not_transaction_effects() {
    for diverse in [false, true] {
        let p = prepared(1024, diverse);
        let old = unshared(&p);
        let new = compile(&p, 172_800_123).unwrap();
        assert_eq!(old.exports.len(), new.exports.len());
        assert!(new.nodes.len() < old.nodes.len());
        let old_shapes = [1024, 1024, 1024, 1024, 1024, 1, 1024, 1024];
        let shapes = super::super::integer::composition_resource_rows(1024);
        println!(
            "plan_work={}",
            serde_json::json!({"diverse":diverse,
            "unshared_original_tiles":work(&old, old_shapes),
            "shared_original_tiles":work(&new, old_shapes),
            "shared_bounded_tiles":work(&new, shapes)})
        );
        // Random HashMap seeds cannot affect emitted node or export order.
        let again = compile(&p, 172_800_123).unwrap();
        assert_eq!(format!("{:?}", new.nodes), format!("{:?}", again.nodes));
        assert_eq!(new.exports, again.exports);
    }
}

#[test]
#[ignore = "requires trusted AOEM and Vulkan; shared/unshared complete plans"]
fn resident_shared_and_unshared_complete_plans_match_every_export() -> Result<()> {
    let library = std::path::PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .ok_or_else(|| anyhow::anyhow!("AOEM library required"))?,
    );
    let mut resources = super::super::integer::ResidentTransferResources::new(&library, 4, 1024)?;
    for diverse in [false, true] {
        let p = prepared(1024, diverse);
        let old = unshared(&p);
        let new = compile(&p, 172_800_123)?;
        let before = resources.run_effects(&old.nodes, &old.exports)?;
        let after = resources.run_effects(&new.nodes, &new.exports)?;
        assert_eq!(
            before, after,
            "all effective state/receipt/diagnostic outputs; diverse={diverse}"
        );
        assert_eq!(new.exports.len(), 1024 * ROW + 21);
        let projection = ExportProjection::new(&new.exports);
        assert!(projection.unique.len() < new.exports.len());
        let compact = resources.run_effects(&new.nodes, &projection.unique)?;
        assert_eq!(
            after,
            projection.expand(&compact)?,
            "every logical effect, including repetitions"
        );
        println!(
            "export_projection={}",
            serde_json::json!({
                "diverse":diverse,"logical_exports":new.exports.len(),
                "unique_exports":projection.unique.len(),
                "value_payload_bytes_before":new.exports.len()*128,
                "value_payload_bytes_after":projection.unique.len()*128,
                "nodes_unchanged":new.nodes.len()
            })
        );
    }
    Ok(())
}

fn spend_prepared(count: usize, remaining: u128) -> Prepared {
    use super::super::uca_effects::{AuthorityOption, SpendEntry, SpendGrant, SpendPlan};
    use novovm_adapter_api::uca_spend_policy::SuppliedAssetSpendGrantV2;
    use novovm_protocol::uca_spend::AssetSpendIntentV3;
    let mut p = prepared(count, false);
    let payer = p.requests[0].payer.clone();
    let identity = p.identities[0];
    p.credit_only.clear();
    for i in 0..count {
        p.requests[i].payer = payer.clone();
        p.requests[i].recipient = payer.clone();
        p.requests[i].max_pay_amount = 1000;
        p.identities[i] = identity;
        p.tx_nonces[i] = i as u64;
    }
    let grant_id = [17; 32];
    let intent = AssetSpendIntentV3 {
        authority: novovm_protocol::uca_spend::AssetSpendAuthorityV3::DelegatedGrant,
        chain_id: 7,
        genesis: [8; 32],
        payer: payer.as_bytes().to_vec(),
        authorization_epoch: 1,
        spend_key_ref: [9; 32],
        recipient: payer.as_bytes().to_vec(),
        asset: 1,
        amount: 10,
        max_fee: 1000,
        nonce: 0,
        not_before_height: 1,
        expires_at_height: 100,
        authority_update: None,
        recipient_creation: None,
    };
    p.spend = Some(SpendPlan {
        entries: (0..count)
            .map(|_| SpendEntry {
                grant_id,
                reserve: 1010,
                static_failure: None,
                intent: intent.clone(),
                creates_recipient: false,
                authority_options: vec![AuthorityOption {
                    version: 0,
                    failure: None,
                    absent_failure: Some(
                        novovm_adapter_api::uca_spend_policy::SpendPolicyError::StateUnavailable,
                    ),
                    limit: 1010,
                }],
            })
            .collect(),
        grants: BTreeMap::from([(
            grant_id,
            SpendGrant {
                intent,
                record: Some(SuppliedAssetSpendGrantV2 {
                    account: payer.as_bytes().to_vec(),
                    asset: 1,
                    key_ref: [9; 32],
                    authorization_epoch: 1,
                    permits_nov_transfer: true,
                    revoked: false,
                    not_before_height: 1,
                    expires_at_height: 100,
                    max_debit_per_intent: 1010,
                    remaining_debit_budget: remaining,
                }),
            },
        )]),
    });
    p
}

#[test]
fn controller_complete_plan_needs_no_grant_or_synthetic_budget() -> Result<()> {
    let mut p = spend_prepared(2, 10_000);
    let spend = p.spend.as_mut().unwrap();
    spend.grants.clear();
    for entry in &mut spend.entries {
        entry.intent.authority = novovm_protocol::uca_spend::AssetSpendAuthorityV3::Controller;
        entry.authority_options[0].limit = 0;
    }
    let plan = compile(&p, 172_800_123)?;
    let tail = p.requests.len() * ROW + 21;
    for index in 0..p.requests.len() {
        assert_eq!(plan.exports[tail + index * 2 + 1], lit(0));
    }
    Ok(())
}

#[test]
#[ignore = "requires trusted AOEM and Vulkan; complete UCA authorization/budget effects"]
fn resident_uca_authorization_guards_nonce_budget_and_diagnostics() -> Result<()> {
    use novovm_adapter_api::uca_spend_policy::SpendPolicyError;
    let library = std::path::PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY").context("AOEM library required")?,
    );
    let mut resources = super::super::integer::ResidentTransferResources::new(&library, 4, 16)?;
    let now = 172_800_123;
    let mut p = spend_prepared(2, 10_000);
    p.spend.as_mut().unwrap().entries[0].static_failure = Some(SpendPolicyError::Revoked);
    p.tx_nonces[1] = 0; // Rejected row must not consume the nonce.
    let plan = compile(&p, now)?;
    let result = resources.run_effects(&plan.nodes, &plan.exports)?;
    let tail = 2 * ROW + 21;
    let fee = plan.quotes[1].intent_terms().0;
    assert_eq!(
        result[tail],
        super::super::uca_effects::authorization_code(Some(SpendPolicyError::Revoked))
    );
    assert_eq!((result[5], result[6], result[9], result[10]), (0, 0, 0, 0));
    assert_eq!(result[tail + 1], 10_000);
    assert_eq!(result[tail + 2], 0);
    assert_eq!(result[ROW + 5], 1);
    assert_eq!(result[ROW + 6], fee);
    assert_eq!(
        result[tail + 3],
        10_000 - 10 - fee,
        "successful self-transfer consumes amount plus actual fee"
    );

    // Both rows separately fit the original allowance; only the effective
    // prefix reveals the second exceeds it. Unused max_fee is released.
    let p = spend_prepared(2, 1010 + 10 + fee - 1);
    let plan = compile(&p, now)?;
    let result = resources.run_effects(&plan.nodes, &plan.exports)?;
    assert_eq!(result[tail], 0);
    assert_eq!(result[tail + 1], 1009);
    assert_eq!(result[tail + 2], 21);
    assert_eq!(result[tail + 3], 1009);
    assert_eq!(result[ROW + 5], 1);
    assert_eq!(result[ROW + 6], 0);

    let mut p = spend_prepared(1, 1010);
    p.balances.insert(p.requests[0].payer.clone(), Some(fee));
    let plan = compile(&p, now)?;
    let result = resources.run_effects(&plan.nodes, &plan.exports)?;
    assert_eq!(result[ROW + 21], 0);
    assert_eq!(result[10], 0, "insufficient amount is a business failure");
    assert_eq!(result[5], 1);
    assert_eq!(result[6], fee);
    assert_eq!(
        result[ROW + 22],
        1010 - fee,
        "business failure consumes actual fee only"
    );

    // Unauthorized rows never observe quotes or refresh the fee window. A
    // nonzero sentinel tells projection to retain the previous failure detail.
    let mut p = spend_prepared(1, 0);
    p.fees.accounting.daily_window_day = 1;
    p.fees.accounting.daily_nov_used = 77;
    p.tx_nonces[0] = 9; // Nonce failure precedes the insufficient allowance.
    let plan = compile(&p, now)?;
    let result = resources.run_effects(&plan.nodes, &plan.exports)?;
    assert_eq!(result[ROW + 21], 20);
    assert_eq!(result[ROW + 9], 1);
    assert_eq!(result[ROW + 10], 77);
    assert_eq!(result[ROW + 19], 0);
    assert_eq!(result[ROW + 20], 2);
    assert_eq!(&result[ROW + 11..ROW + 17], &[0; 6]);

    // A signed cap of zero is strict, not the old V3 automatic-quote sentinel.
    let mut p = spend_prepared(1, 1010);
    p.requests[0].max_pay_amount = 0;
    let spend = p.spend.as_mut().unwrap();
    spend.entries[0].reserve = 10;
    spend.grants.get_mut(&[17; 32]).unwrap().intent.max_fee = 0;
    let plan = compile(&p, now)?;
    assert!(
        plan.quotes[0].quotation().is_err(),
        "strict zero cap must reject a positive fee"
    );
    let result = resources.run_effects(&plan.nodes, &plan.exports)?;
    assert_eq!(result[ROW + 21], 0, "valid authority, fee rejection");
    assert_eq!((result[5], result[6], result[9], result[10]), (1, 0, 0, 0));
    assert_eq!(result[ROW + 22], 1010);

    // All rejected rows preserve pre-existing diagnostics, including both a
    // previous failed quote and a past fee window. Some quotes here are valid,
    // others invalid; neither kind becomes an observed fee event.
    let mut p = spend_prepared(2, 1010);
    p.fees.accounting.daily_window_day = 1;
    p.fees.accounting.daily_nov_used = 77;
    let d = &mut p.fees.diagnostics;
    d.quote_max_pay_exceeded = 11;
    d.settlement_paused = 12;
    d.settlement_amount_overflow = 13;
    d.clearing_quote_expired = 14;
    d.clearing_insufficient_user_balance = 15;
    d.settlement_policy_fallback = 16;
    d.last_quote_failure = Some(FeeFailure {
        code: FeeFailureCode::MaxPayExceeded,
        reason: "prior quote failure retained".into(),
    });
    p.requests[1].max_pay_amount = 0;
    for entry in &mut p.spend.as_mut().unwrap().entries {
        entry.static_failure = Some(SpendPolicyError::Revoked);
    }
    let plan = compile(&p, now)?;
    let result = resources.run_effects(&plan.nodes, &plan.exports)?;
    let f = &result[2 * ROW..2 * ROW + 21];
    assert_eq!((f[9], f[10]), (1, 77));
    assert_eq!(&f[11..17], &[11, 12, 13, 14, 15, 16]);
    assert_eq!(
        (f[17], f[19], f[20]),
        (0, 0, 3),
        "no clearing or quote observation; preserve prior failure"
    );
    for i in 0..2 {
        let row = &result[i * ROW..(i + 1) * ROW];
        assert_eq!(&row[3..6], &row[..3]);
        assert_eq!((row[6], row[9], row[10]), (0, 0, 0));
        assert_eq!(result[2 * ROW + 21 + i * 2 + 1], 1010);
    }

    // Different purpose grants share the same UCA nonce. A new grant cannot
    // reuse nonce zero; that refusal also leaves its own independent budget.
    let mut p = spend_prepared(3, 10_000);
    let mut other = spend_prepared(1, 20_000)
        .spend
        .unwrap()
        .grants
        .remove(&[17; 32])
        .unwrap();
    other.intent.spend_key_ref = [10; 32];
    other.record.as_mut().unwrap().key_ref = [10; 32];
    let spend = p.spend.as_mut().unwrap();
    spend.grants.insert([18; 32], other);
    spend.entries[1].grant_id = [18; 32];
    spend.entries[2].grant_id = [18; 32];
    p.tx_nonces = vec![0, 0, 1];
    let plan = compile(&p, now)?;
    let result = resources.run_effects(&plan.nodes, &plan.exports)?;
    let tail = 3 * ROW + 21;
    assert_eq!(
        (result[tail], result[tail + 2], result[tail + 4]),
        (0, 20, 0)
    );
    assert_eq!((result[5], result[ROW + 5], result[2 * ROW + 5]), (1, 1, 2));
    assert_eq!(result[tail + 1], 10_000 - 10 - fee);
    assert_eq!(result[tail + 3], 20_000);
    assert_eq!(result[tail + 5], 20_000 - 10 - fee);
    Ok(())
}
