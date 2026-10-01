// Included in native_candidate_auth::record_tests. The reference intentionally
// retains the old append/whole-prefix-authenticate/pop algorithm.

fn selection_raw(tx: &NovNativeTxWireV1) -> Vec<u8> {
    novovm_protocol::encode_nov_native_tx_wire_v1(tx).unwrap()
}

fn selection_exact_sign(tx: &mut NovNativeTxWireV1, seed: [u8; 32]) {
    tx.signature.clear();
    let ir = nov_native_tx_to_adapter_tx_ir_v1(tx).unwrap();
    tx.signature = novovm_adapter_novovm::signature_payload_with_seed_v1(&ir, seed);
}

fn selection_alias(nonce: u64, seed: [u8; 32], width: usize, uppercase: bool) -> NovNativeTxWireV1 {
    let signed = transaction(nonce, seed, 1);
    let caller = if width == 32 {
        signed.signature[..32].to_vec()
    } else {
        assert_eq!(width, 20);
        novovm_adapter_novovm::address_from_seed_v1(seed)
    };
    let account = if uppercase {
        format!(" \t0X{}\n", to_hex(&caller).to_ascii_uppercase())
    } else {
        to_hex(&caller)
    };
    let mut tx = NovNativeTxWireV1 {
        chain_id: CHAIN,
        kind: NovTxKindV1::Execute(novovm_protocol::NovExecuteTxV1 {
            caller,
            account_id: Some(account.clone()),
            fee_owner_account_id: Some(account.clone()),
            nonce_owner_account_id: Some(account),
            target: NovExecutionTargetV1::NativeModule("treasury".into()),
            method: "deposit_reserve".into(),
            args: br#"{"asset":"NOV","amount":1}"#.to_vec(),
            execution_mode: NovExecutionModeV1::Standard,
            execution_policy: NovExecutionPolicyV1::Standard,
            privacy_mode: NovPrivacyModeV1::Public,
            verification_mode: NovVerificationModeV1::Standard,
            fee_policy: NovFeePolicyV1 {
                pay_asset: "NOV".into(),
                max_pay_amount: 1000,
                slippage_bps: 0,
            },
            gas_like_limit: Some(90_000),
            nonce,
        }),
        signature: Vec::new(),
    };
    selection_exact_sign(&mut tx, seed);
    tx
}

fn selection_prefix_reference(
    parent: &NovNativeExecutionStoreV1,
    ordered: &[Vec<u8>],
    params: &serde_json::Value,
    limit: usize,
) -> Vec<Vec<u8>> {
    let template = plan(&[transaction(0, [0x51; 32], 1)]);
    let mut selected = Vec::new();
    for raw in ordered {
        if selected.len() == limit {
            break;
        }
        selected.push(raw.clone());
        let accepted = (|| -> Result<()> {
            let hashes = selected
                .iter()
                .map(|raw| canonical_nov_native_tx_hash_from_payload_v1(raw))
                .collect::<Result<Vec<_>>>()?;
            let candidate = NovNativeCandidateExecutionPlanV1::new(
                template.context,
                template.protocol_config_commitment,
                template.pre_state_root,
                template.aoem_parent.clone(),
                hashes,
                selected.clone(),
            )?;
            authenticate_plan(&candidate, parent, params)?;
            Ok(())
        })();
        if accepted.is_err() {
            selected.pop();
        }
    }
    selected
}

fn selection_nonce_reads(reader: &Reader) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for parts in reader.reads.borrow().iter() {
        if parts.len() == 3 && parts[0] == "module_state" && parts[1] == "native_auth_next_nonces" {
            *counts.entry(parts[2].clone()).or_insert(0) += 1;
        }
    }
    counts
}

#[test]
fn native_selection_matches_prefix_alias_nonce_gap_stale_and_replays() {
    let params = serde_json::json!({"chain_id": CHAIN});
    let a = [0x51; 32];
    let b = [0x52; 32];
    let txs = [
        selection_alias(0, a, 20, false), // stale at the parent
        selection_alias(1, a, 32, true),
        selection_alias(1, a, 20, false), // same signer and nonce, another alias
        selection_alias(2, a, 20, true),
        selection_alias(4, a, 32, false), // gap: no speculative nonce advance
        transaction(0, b, 1),
        transaction(1, b, 2),
        transaction(3, b, 3), // another gap
    ];
    let identity = reservation(&txs[0]).identity_key;
    for tx in &txs[..5] {
        assert_eq!(reservation(tx).identity_key, identity);
    }
    let mut parent = store();
    parent
        .module_state
        .native_auth_next_nonces
        .insert(identity, 1);
    let mut ordered: Vec<_> = txs.iter().map(selection_raw).collect();
    ordered.push(ordered[1].clone()); // repeated exact signed intent
    let reader = Reader::new(&parent);
    let before = reader.values.clone();
    let selected =
        select_record_transactions(CHAIN, PROTOCOL, &reader, ordered.clone(), &params, 16).unwrap();
    assert_eq!(
        selected,
        select_transactions(CHAIN, PROTOCOL, &parent, ordered.clone(), &params, 16).unwrap()
    );
    assert_eq!(
        selected,
        selection_prefix_reference(&parent, &ordered, &params, 16)
    );
    assert_eq!(selected, [1, 3, 5, 6].map(|index| ordered[index].clone()));
    assert_eq!(selection_nonce_reads(&reader).len(), 2);
    assert!(selection_nonce_reads(&reader)
        .values()
        .all(|reads| *reads == 1));
    assert_eq!(reader.values, before);

    // Parent reservation/receipt replays remain exclusions even if a corrupt
    // fixture nonce floor would otherwise admit the transaction.
    for replay_path in [
        path(&[
            "module_state",
            "native_auth_nonce_reservations",
            &reservation(&txs[1]).ledger_key,
        ]),
        path(&["receipts", &reservation(&txs[1]).tx_hash]),
    ] {
        let mut reader = Reader::new(&parent);
        reader.values.insert(replay_path, b"null".to_vec());
        let selected =
            select_record_transactions(CHAIN, PROTOCOL, &reader, ordered.clone(), &params, 16)
                .unwrap();
        assert!(!selected.contains(&ordered[1]));
    }
}

#[test]
fn native_selection_distinct_signer_reads_are_linear_and_limits_match_prefix() {
    let parent = store();
    let params = serde_json::json!({"chain_id": CHAIN});
    let mut entries = Vec::new();
    for seed in 0x51..0x59 {
        for nonce in 0..8 {
            let tx = transaction(nonce, [seed; 32], 1);
            let signed = reservation(&tx);
            entries.push((signed.identity_key, nonce, selection_raw(&tx)));
        }
    }
    entries.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    let ordered: Vec<_> = entries.into_iter().map(|(_, _, raw)| raw).collect();
    for limit in [1, 16, 17, 32, 1024] {
        let reader = Reader::new(&parent);
        let selected =
            select_record_transactions(CHAIN, PROTOCOL, &reader, ordered.clone(), &params, limit)
                .unwrap();
        assert_eq!(
            selected,
            selection_prefix_reference(&parent, &ordered, &params, limit)
        );
        assert_eq!(selected.len(), limit.min(ordered.len()));
        let counts = selection_nonce_reads(&reader);
        assert_eq!(counts.len(), selected.len().div_ceil(8));
        assert!(counts.values().all(|reads| *reads == 1));
        assert_eq!(
            reader.reads.borrow().len(),
            8 + counts.len() + 2 * selected.len()
        );
    }
    for limit in [0, 1025, usize::MAX] {
        assert!(select_record_transactions(
            CHAIN,
            PROTOCOL,
            &Reader::new(&parent),
            ordered.clone(),
            &params,
            limit
        )
        .is_err());
    }
}

#[test]
fn native_selection_byte_limit_skips_without_consuming_nonce() {
    let seed = [0x51; 32];
    let params = serde_json::json!({"chain_id": CHAIN});
    let parent = store();
    let large = |nonce| {
        let mut tx = selection_alias(nonce, seed, 20, false);
        let NovTxKindV1::Execute(execute) = &mut tx.kind else {
            unreachable!()
        };
        execute.args =
            serde_json::to_vec(&serde_json::json!({"padding": "x".repeat(63_000)})).unwrap();
        selection_exact_sign(&mut tx, seed);
        selection_raw(&tx)
    };
    let max_bytes = crate::native_block_ledger::NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1;
    let size = large(0).len();
    assert!(size < crate::tx_ingress::fresh_pool::MAX_RAW_BYTES);
    let fit = max_bytes / size;
    assert!((17..64).contains(&fit));
    let mut ordered: Vec<_> = (0..=fit).map(|nonce| large(nonce as u64)).collect();
    assert!(ordered.iter().map(Vec::len).sum::<usize>() > max_bytes);
    let small = selection_raw(&selection_alias(fit as u64, seed, 20, false));
    let next = selection_raw(&selection_alias(fit as u64 + 1, seed, 32, true));
    ordered.extend([small.clone(), next.clone()]);
    let reader = Reader::new(&parent);
    let selected =
        select_record_transactions(CHAIN, PROTOCOL, &reader, ordered.clone(), &params, 1024)
            .unwrap();
    assert_eq!(
        selected,
        selection_prefix_reference(&parent, &ordered, &params, 1024)
    );
    assert_eq!(selected.len(), fit + 2);
    assert_eq!(&selected[fit..], &[small, next]);
    assert!(selected.iter().map(Vec::len).sum::<usize>() <= max_bytes);
    assert_eq!(
        selection_nonce_reads(&reader)
            .values()
            .copied()
            .collect::<Vec<_>>(),
        [1]
    );
}

#[test]
fn native_selection_invalid_raw_signature_domain_and_nonce_exhaustion() {
    let params = serde_json::json!({"chain_id": CHAIN});
    let mut parent = store();
    let good = transaction(0, [0x51; 32], 1);
    let mut bad_signature = good.clone();
    *bad_signature.signature.last_mut().unwrap() ^= 1;
    let mut wrong_chain = transaction(0, [0x52; 32], 1);
    wrong_chain.chain_id += 1;
    sign_nov_native_tx_with_seed_v1(&mut wrong_chain, [0x52; 32]).unwrap();
    let ordered = vec![
        vec![],
        vec![0xff],
        selection_raw(&bad_signature),
        selection_raw(&wrong_chain),
        selection_raw(&good),
    ];
    let reader = Reader::new(&parent);
    assert_eq!(
        select_record_transactions(CHAIN, PROTOCOL, &reader, ordered.clone(), &params, 16).unwrap(),
        [selection_raw(&good)]
    );
    assert_eq!(
        selection_prefix_reference(&parent, &ordered, &params, 16),
        [selection_raw(&good)]
    );
    assert_eq!(selection_nonce_reads(&reader).len(), 1);
    assert_eq!(reader.reads.borrow().len(), 8 + 3);

    let last = transaction(u64::MAX - 1, [0x53; 32], 1);
    let exhausted = transaction(u64::MAX, [0x53; 32], 1);
    let identity = reservation(&last).identity_key;
    let ordered = vec![selection_raw(&last), selection_raw(&exhausted)];
    for floor in [u64::MAX - 1, u64::MAX] {
        parent
            .module_state
            .native_auth_next_nonces
            .insert(identity.clone(), floor);
        let reader = Reader::new(&parent);
        let selected =
            select_record_transactions(CHAIN, PROTOCOL, &reader, ordered.clone(), &params, 16)
                .unwrap();
        assert_eq!(
            selected,
            selection_prefix_reference(&parent, &ordered, &params, 16)
        );
        assert_eq!(selected.len(), usize::from(floor == u64::MAX - 1));
        assert_eq!(selection_nonce_reads(&reader)[&identity], 1);
    }
}

#[test]
fn native_selection_parent_domain_and_late_read_fault_abort_not_partial_success() {
    let parent = store();
    let params = serde_json::json!({"chain_id": CHAIN});
    let first = transaction(0, [0x51; 32], 1);
    let second = transaction(0, [0x52; 32], 1);
    let second_reservation = reservation(&second);
    let first_identity = reservation(&first).identity_key;
    let ordered = vec![selection_raw(&first), selection_raw(&second)];
    for failed in [
        path(&[
            "module_state",
            "native_auth_nonce_reservations",
            &second_reservation.ledger_key,
        ]),
        path(&["receipts", &second_reservation.tx_hash]),
        path(&[
            "module_state",
            "native_auth_next_nonces",
            &second_reservation.identity_key,
        ]),
    ] {
        let mut reader = Reader::new(&parent);
        reader.failed_path = Some(failed);
        let before = reader.values.clone();
        let error =
            select_record_transactions(CHAIN, PROTOCOL, &reader, ordered.clone(), &params, 16)
                .unwrap_err();
        assert!(format!("{error:#}").contains("injected missing or corrupt parent blob"));
        assert_eq!(reader.values, before);
        assert_eq!(selection_nonce_reads(&reader)[&first_identity], 1);
    }
    for (field, value) in [
        (path(&["authority_chain_id"]), b"1".to_vec()),
        (
            path(&["module_state", "protocol_config_commitment"]),
            b"\"wrong\"".to_vec(),
        ),
        (
            path(&["module_state", "native_auth_nonce_identity_scheme"]),
            b"\"legacy\"".to_vec(),
        ),
    ] {
        let mut reader = Reader::new(&parent);
        reader.values.insert(field, value);
        assert!(
            select_record_transactions(CHAIN, PROTOCOL, &reader, ordered.clone(), &params, 16)
                .is_err()
        );
    }
    let mut reader = Reader::new(&parent);
    reader.values.remove(&path(&["receipts"]));
    assert!(
        select_record_transactions(CHAIN, PROTOCOL, &reader, ordered.clone(), &params, 16).is_err()
    );
    let mut reader = Reader::new(&parent);
    reader.values.insert(
        path(&[
            "module_state",
            "native_auth_next_nonces",
            &second_reservation.identity_key,
        ]),
        b"null".to_vec(),
    );
    let error =
        select_record_transactions(CHAIN, PROTOCOL, &reader, ordered, &params, 16).unwrap_err();
    assert!(format!("{error:#}").contains("next nonce must be a u64"));
}

#[test]
fn native_selection_does_not_mutate_durable_pool_on_success_or_read_failure() {
    use crate::tx_ingress::fresh_pool::{FreshTransactionPool, PendingTransaction};
    let params = serde_json::json!({"chain_id": CHAIN});
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts/audit/native-selection");
    std::fs::create_dir_all(&root).unwrap();
    let pool_path = root.join(format!(
        "pool-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut pool = FreshTransactionPool::open(&pool_path, CHAIN, [0x71; 32], &params).unwrap();
    for nonce in (0..6).rev() {
        let raw = selection_raw(&transaction(nonce, [0x51; 32], 1));
        assert!(pool
            .insert(PendingTransaction::authenticate(raw, CHAIN, &params).unwrap())
            .unwrap());
    }
    let snapshot = |pool: &FreshTransactionPool| {
        pool.ordered()
            .into_iter()
            .map(|entry| (entry.hash, entry.raw, entry.identity, entry.nonce))
            .collect::<Vec<_>>()
    };
    let before = snapshot(&pool);
    let ordered: Vec<_> = pool.ordered().into_iter().map(|entry| entry.raw).collect();
    let parent = store();
    assert_eq!(
        select_record_transactions(
            CHAIN,
            PROTOCOL,
            &Reader::new(&parent),
            ordered.clone(),
            &params,
            3
        )
        .unwrap(),
        ordered[..3]
    );
    assert_eq!(snapshot(&pool), before);
    let mut reader = Reader::new(&parent);
    reader.failed_path = Some(path(&[
        "receipts",
        &reservation(&transaction(4, [0x51; 32], 1)).tx_hash,
    ]));
    assert!(select_record_transactions(CHAIN, PROTOCOL, &reader, ordered, &params, 16).is_err());
    assert_eq!(snapshot(&pool), before);
    drop(pool);
    let reopened = FreshTransactionPool::open(&pool_path, CHAIN, [0x71; 32], &params).unwrap();
    assert_eq!(snapshot(&reopened), before);
}
