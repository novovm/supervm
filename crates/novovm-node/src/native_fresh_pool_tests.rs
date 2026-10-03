mod fresh_pool_tests {
    use super::*;
    use crate::tx_ingress::fresh_pool::{FreshTransactionPool, PendingTransaction};

    fn path(label: &str) -> PathBuf {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/audit/fresh-pool");
        fs::create_dir_all(&parent).unwrap();
        parent.join(format!(
            "{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn raw(chain: u64, nonce: u64, amount: u64) -> Vec<u8> {
        let mut transaction = NovNativeTxWireV1 {
            chain_id: chain,
            kind: NovTxKindV1::Transfer(novovm_protocol::NovTransferTxV1 {
                from: Vec::new(),
                to: novovm_adapter_novovm::address_from_seed_v1([0xa5; 32]),
                asset: "NOV".into(),
                amount: u128::from(amount),
                nonce,
                fee_policy: NovFeePolicyV1 {
                    pay_asset: "NOV".into(),
                    max_pay_amount: 1_000,
                    slippage_bps: 0,
                },
            }),
            signature: Vec::new(),
        };
        sign_nov_native_tx_with_seed_v1(&mut transaction, [0xa4; 32]).unwrap();
        encode_native_auth_test_tx_v1(&transaction)
    }

    fn execute_raw(chain: u64) -> Vec<u8> {
        encode_native_auth_test_tx_v1(&build_signed_native_auth_test_tx_v1(
            chain,
            0,
            [0xa4; 32],
            "pool-user",
            1,
        ))
    }

    fn isolated_case(name: &str, permit: Option<&str>) -> bool {
        super::restoration_host_guard::isolated_named_case(
            &format!("tx_ingress::tests::fresh_pool_tests::{name}"),
            permit,
            true,
        )
    }

    fn assert_execute_capability_rejection(error: anyhow::Error) {
        assert!(
            error.to_string().starts_with(
                "legacy Host execution is disabled: fresh transaction pool Host Execute"
            ),
            "wrong rejection: {error:#}"
        );
    }

    #[test]
    fn unsupported_execute_is_rejected_before_admission_and_transfer_still_works() {
        if !isolated_case(
            "unsupported_execute_is_rejected_before_admission_and_transfer_still_works",
            None,
        ) {
            return;
        }
        let path = path("unsupported-execute");
        let chain = 891042;
        let params = serde_json::json!({
            "chain_id":chain,
            "allow_legacy_host_execution":true,
            "NOVOVM_ALLOW_LEGACY_HOST_EXECUTION":"1",
            "aoem_owned_gate_config":{"production_candidate":true}
        });
        let mut pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        assert_execute_capability_rejection(
            PendingTransaction::authenticate(execute_raw(chain), chain, &params)
                .err()
                .expect("RPC fields cannot grant a Host execution capability"),
        );
        assert!(pool.is_empty());
        assert_eq!(pool.admission_sync_commits_for_test(), 0);
        let transfer = PendingTransaction::authenticate(raw(chain, 0, 1), chain, &params).unwrap();
        let hash = transfer.hash;
        assert!(pool.insert(transfer).unwrap());
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.admission_sync_commits_for_test(), 1);
        drop(pool);
        let pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        assert_eq!(pool.len(), 1);
        assert!(pool.contains(&hash));
        drop(pool);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn explicit_comparison_execute_still_requires_signature_and_chain() {
        if !isolated_case(
            "explicit_comparison_execute_still_requires_signature_and_chain",
            Some("1"),
        ) {
            return;
        }
        let path = path("comparison-execute");
        let chain = 891043;
        let params = serde_json::json!({"chain_id":chain});
        let signed = execute_raw(chain);
        assert!(PendingTransaction::authenticate(signed.clone(), chain + 1, &params).is_err());
        let mut bad_signature = signed.clone();
        *bad_signature.last_mut().unwrap() ^= 1;
        assert!(PendingTransaction::authenticate(bad_signature, chain, &params).is_err());
        let entry = PendingTransaction::authenticate(signed.clone(), chain, &params).unwrap();
        let hash = entry.hash;
        let mut pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        assert!(pool.insert(entry).unwrap());
        drop(pool);
        let pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.get(&hash).unwrap().raw, signed);
        drop(pool);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn old_execute_pool_recovery_fails_closed_without_deleting_records() {
        if !isolated_case(
            "old_execute_pool_recovery_fails_closed_without_deleting_records",
            None,
        ) {
            return;
        }
        // Only this exact test runs in the isolated child. Produce the old
        // record with explicit permission, then simulate a default-denied
        // restart without changing another test's process environment.
        let path = path("old-execute-recovery");
        let chain = 891044;
        let params = serde_json::json!({"chain_id":chain});
        let signed = execute_raw(chain);
        std::env::set_var(NOV_NATIVE_ALLOW_LEGACY_HOST_EXECUTION_ENV, "1");
        let entry = PendingTransaction::authenticate(signed.clone(), chain, &params).unwrap();
        let hash = entry.hash;
        let mut pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        assert!(pool.insert(entry).unwrap());
        drop(pool);
        std::env::remove_var(NOV_NATIVE_ALLOW_LEGACY_HOST_EXECUTION_ENV);
        assert_execute_capability_rejection(
            FreshTransactionPool::open(&path, chain, [7; 32], &params)
                .err()
                .expect("unsupported persisted Execute must not be silently dropped"),
        );
        let db = rocksdb::DB::open_default(&path).unwrap();
        let mut key = vec![b't'];
        key.extend_from_slice(&hash);
        assert_eq!(db.get(key).unwrap().unwrap(), signed);
        assert_eq!(
            db.get(b"identity").unwrap().unwrap(),
            serde_json::to_vec(&("novovm-fresh-transaction-pool/v1", chain, [7u8; 32])).unwrap()
        );
        assert_eq!(db.iterator(rocksdb::IteratorMode::Start).count(), 2);
        drop(db);
        std::env::set_var(NOV_NATIVE_ALLOW_LEGACY_HOST_EXECUTION_ENV, "1");
        let pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.get(&hash).unwrap().raw, signed);
        drop(pool);
        std::env::remove_var(NOV_NATIVE_ALLOW_LEGACY_HOST_EXECUTION_ENV);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn durable_admission_restart_binding_dedup_and_nonce_conflict() {
        let path = path("restart");
        let chain = 891035;
        let params = serde_json::json!({"chain_id":chain});
        let mut pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        let entry = PendingTransaction::authenticate(raw(chain, 0, 1), chain, &params).unwrap();
        let hash = entry.hash;
        assert!(pool.insert(entry.clone()).unwrap());
        assert!(pool.insert(entry).unwrap());
        assert!(!pool
            .insert(PendingTransaction::authenticate(raw(chain, 0, 2), chain, &params).unwrap())
            .unwrap());
        assert_eq!(pool.len(), 1);
        assert!(FreshTransactionPool::open(&path, chain, [7; 32], &params).is_err());
        drop(pool);
        assert!(FreshTransactionPool::open(&path, chain, [8; 32], &params).is_err());
        let pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        assert_eq!(pool.len(), 1);
        assert!(pool.contains(&hash));
        drop(pool);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn invalid_signatures_domains_bounds_and_corrupt_recovery_fail_closed() {
        let path = path("corrupt");
        let chain = 891036;
        let params = serde_json::json!({"chain_id":chain});
        let mut signed = raw(chain, 0, 1);
        assert!(PendingTransaction::authenticate(signed.clone(), chain + 1, &params).is_err());
        assert!(PendingTransaction::authenticate(
            vec![0; fresh_pool::MAX_RAW_BYTES + 1],
            chain,
            &params
        )
        .is_err());
        *signed.last_mut().unwrap() ^= 1;
        assert!(PendingTransaction::authenticate(signed.clone(), chain, &params).is_err());
        let pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        drop(pool);
        let db = rocksdb::DB::open_default(&path).unwrap();
        db.put(
            [b't'].into_iter().chain([0u8; 32]).collect::<Vec<_>>(),
            signed,
        )
        .unwrap();
        drop(db);
        assert!(FreshTransactionPool::open(&path, chain, [7; 32], &params).is_err());
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn signer_capacity_survives_restart_and_order_is_nonce_sorted() {
        let path = path("capacity");
        let chain = 891037;
        let params = serde_json::json!({"chain_id":chain});
        let mut pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        for nonce in (0..64).rev() {
            assert!(pool
                .insert(
                    PendingTransaction::authenticate(raw(chain, nonce, 1), chain, &params).unwrap()
                )
                .unwrap());
        }
        assert!(!pool
            .insert(PendingTransaction::authenticate(raw(chain, 64, 1), chain, &params).unwrap())
            .unwrap());
        assert_eq!(
            pool.ordered()
                .iter()
                .map(|entry| entry.nonce)
                .collect::<Vec<_>>(),
            (0..64).collect::<Vec<_>>()
        );
        for (borrowed, owned) in pool.ordered_refs().into_iter().zip(pool.ordered()) {
            assert_eq!(borrowed.hash, owned.hash);
            assert_eq!(borrowed.raw, owned.raw);
            assert_eq!(borrowed.identity, owned.identity);
            assert_eq!(borrowed.nonce, owned.nonce);
        }
        drop(pool);
        let pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        assert_eq!(pool.len(), 64);
        drop(pool);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn missing_identity_in_empty_existing_store_is_not_reinitialized() {
        let path = path("identity");
        let params = serde_json::json!({"chain_id":891038});
        drop(FreshTransactionPool::open(&path, 891038, [7; 32], &params).unwrap());
        let db = rocksdb::DB::open_default(&path).unwrap();
        db.delete(b"identity").unwrap();
        drop(db);
        assert!(FreshTransactionPool::open(&path, 891038, [7; 32], &params).is_err());
        let db = rocksdb::DB::open_default(&path).unwrap();
        assert!(db.get(b"identity").unwrap().is_none());
        drop(db);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn batch_admission_commits_once_preserves_duplicates_conflicts_and_restart() {
        let path = path("batch-order");
        let chain = 891039;
        let params = serde_json::json!({"chain_id":chain});
        let mut pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        let first = PendingTransaction::authenticate(raw(chain, 0, 1), chain, &params).unwrap();
        let next = PendingTransaction::authenticate(raw(chain, 1, 1), chain, &params).unwrap();
        let conflict = PendingTransaction::authenticate(raw(chain, 0, 2), chain, &params).unwrap();
        // Deliberately impossible authenticated hash binding: the pool must
        // still compare exact raw bytes rather than acknowledge a hash alone.
        let mut collision = next.clone();
        collision.hash = first.hash;
        let admission = pool
            .insert_live_batch_with_retention(
                vec![
                    first.clone(),
                    first.clone(),
                    conflict.clone(),
                    next.clone(),
                    collision,
                ],
                None,
                &params,
            )
            .unwrap();
        assert_eq!(admission.retained, [true, true, false, true, false]);
        assert_eq!(admission.rejected, 2);
        assert_eq!(pool.admission_sync_commits_for_test(), 1);
        assert_eq!(pool.len(), 2);
        assert_eq!(pool.get(&first.hash).unwrap().raw, first.raw);
        assert_eq!(pool.get(&next.hash).unwrap().raw, next.raw);
        assert!(!pool.contains(&conflict.hash));
        assert_eq!(
            pool.insert_batch(vec![first.clone(), next.clone()])
                .unwrap(),
            [true, true]
        );
        assert_eq!(
            pool.admission_sync_commits_for_test(),
            1,
            "duplicates must not create another sync write"
        );
        drop(pool);
        let pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        assert_eq!(pool.len(), 2);
        assert_eq!(pool.get(&first.hash).unwrap().raw, first.raw);
        assert_eq!(pool.get(&next.hash).unwrap().raw, next.raw);
        drop(pool);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn batch_admission_signer_capacity_rejects_in_order_without_partial_writes() {
        let path = path("batch-capacity");
        let chain = 891040;
        let params = serde_json::json!({"chain_id":chain});
        let mut pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        let entries = (0..65)
            .map(|nonce| {
                PendingTransaction::authenticate(raw(chain, nonce, 1), chain, &params).unwrap()
            })
            .collect();
        let retained = pool.insert_batch(entries).unwrap();
        assert_eq!(&retained[..64], &[true; 64]);
        assert!(!retained[64]);
        assert_eq!(pool.len(), 64);
        assert_eq!(pool.admission_sync_commits_for_test(), 1);
        drop(pool);
        let pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        assert_eq!(pool.len(), 64);
        assert_eq!(
            pool.ordered()
                .iter()
                .map(|entry| entry.nonce)
                .collect::<Vec<_>>(),
            (0..64).collect::<Vec<_>>()
        );
        drop(pool);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn batch_admission_write_failure_publishes_no_memory_or_durable_prefix() {
        let path = path("batch-write-failure");
        let chain = 891041;
        let params = serde_json::json!({"chain_id":chain});
        let mut pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        let old = PendingTransaction::authenticate(raw(chain, 0, 1), chain, &params).unwrap();
        assert!(pool.insert(old.clone()).unwrap());
        let entries: Vec<_> = (1..4)
            .map(|nonce| {
                PendingTransaction::authenticate(raw(chain, nonce, 1), chain, &params).unwrap()
            })
            .collect();
        pool.fail_next_admission_write_for_test();
        let error = pool
            .insert_live_batch_with_retention(entries.clone(), None, &params)
            .err()
            .unwrap();
        assert!(error
            .to_string()
            .contains("injected transaction admission write failure"));
        assert_eq!(pool.admission_sync_commits_for_test(), 1);
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.get(&old.hash).unwrap().raw, old.raw);
        assert!(entries.iter().all(|entry| !pool.contains(&entry.hash)));
        drop(pool);
        let mut pool = FreshTransactionPool::open(&path, chain, [7; 32], &params).unwrap();
        assert_eq!(pool.len(), 1);
        assert!(entries.iter().all(|entry| !pool.contains(&entry.hash)));
        assert_eq!(pool.insert_batch(entries).unwrap(), [true; 3]);
        assert_eq!(pool.admission_sync_commits_for_test(), 1);
        assert_eq!(pool.len(), 4);
        drop(pool);
        fs::remove_dir_all(path).unwrap();
    }
}
