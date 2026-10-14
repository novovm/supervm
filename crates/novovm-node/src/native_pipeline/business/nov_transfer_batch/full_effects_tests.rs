use super::*;

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
