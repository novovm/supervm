//! Reuses the real four-block AOEM/finality fixture; no synthetic valid QC.
use super::*;

fn counted<T>(action: impl FnOnce() -> T) -> (T, (usize, usize, usize)) {
    let before = successor_promotion::HISTORY_PARENT_CHECKS.with(std::cell::Cell::get);
    let current_before = successor_promotion::HISTORY_CURRENT_CHECKS.with(std::cell::Cell::get);
    let result = action();
    let after = successor_promotion::HISTORY_PARENT_CHECKS.with(std::cell::Cell::get);
    let current_after = successor_promotion::HISTORY_CURRENT_CHECKS.with(std::cell::Cell::get);
    (
        result,
        (
            after.0 - before.0,
            after.1 - before.1,
            current_after - current_before,
        ),
    )
}

impl NovNativeBlockLedgerV1 {
    pub(crate) fn assert_fresh_history_reuse_for_test_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
    ) {
        let ledger = Self::open_inner_v1(path, true).unwrap();
        let (config, history) = {
            let _guard = ledger.write_lock.lock().unwrap();
            let config = load_verified(&ledger, genesis, namespace).unwrap();
            (config, archives(&ledger).unwrap())
        };
        assert!(history.len() >= 3, "requires the real four-block fixture");
        let validation = FreshGenesisValidationV1::new(&config).unwrap();
        let last_height = history.last().unwrap().0;
        let expected = history.last().unwrap().1.proof.clone();
        {
            let _guard = ledger.write_lock.lock().unwrap();
            let (keys, counts) = counted(|| validated_keys(&ledger, &validation, namespace));
            assert!(!keys.unwrap().is_empty());
            // The new traversal checks genesis once, reuses the preceding
            // verified record N times, and still checks every current witness.
            assert_eq!(counts, (1, history.len(), history.len()));
            let (cold, counts) = counted(|| -> Result<()> {
                for (_, intent) in &history {
                    intent.validate_with_validation(&ledger, &validation, namespace)?;
                }
                Ok(())
            });
            cold.unwrap();
            assert_eq!(counts, (history.len(), 0, history.len()));
        }
        let read = || Self::load_fresh_finality_by_height_v1(path, genesis, namespace, last_height);
        // Independent public loads do not reuse the previous call's context.
        for _ in 0..2 {
            let (result, counts) = counted(read);
            assert_eq!(result.unwrap().as_ref(), Some(&expected));
            // One cold validation of the active intent plus this history's
            // genesis seed; every archived current witness remains mandatory.
            assert_eq!(counts, (2, history.len(), history.len() + 1));
        }

        // Exercise the actual node runtime lease against real signed history.
        // Stable reads reuse this exact DB revision; every mutation below must
        // invalidate it, including an old QC or an unrelated historical index.
        let _runtime = Self::retain_existing_fresh_session_v1(path).unwrap();
        assert_eq!(read().unwrap().as_ref(), Some(&expected));
        let (stable, counts) = counted(read);
        assert_eq!(stable.unwrap().as_ref(), Some(&expected));
        assert_eq!(
            counts,
            (0, 0, 0),
            "unchanged history is not replayed per call"
        );

        let mut faults = Vec::new();
        for (height, prepare_vote) in [(2, false), (last_height, true)] {
            let mut intent = read_archive(&ledger, height).unwrap().unwrap();
            let crate::native_block_seal::round_message::NovNativeSealRoundMessageV1::DecisionCertificateV3 { decision, .. } = &mut intent.proof.witness else {
                panic!("fixture must contain a complete decision");
            };
            if prepare_vote {
                decision.prepare.votes[0].signature[0] ^= 1;
            } else {
                decision.votes[0].signature[0] ^= 1;
            }
            faults.push((
                archive_key(height),
                serde_json::to_vec(&intent).unwrap(),
                "signature",
            ));
        }
        // This old parent is re-read on the next independent load; a retained
        // previous successful load must not hide its record/pin corruption.
        let parent = successors::record_at(&ledger, 2).unwrap();
        let record_key =
            candidate_record_key_v1(config.chain_id, &parent.block.header.block_hash).into_bytes();
        let mut record: NovNativeBlockCandidateRecordV1 =
            serde_json::from_slice(&ledger.db.get(&record_key).unwrap().unwrap()).unwrap();
        record
            .isolated_execution_binding
            .as_mut()
            .unwrap()
            .workspace_id[0] ^= 1;
        faults.push((record_key, serde_json::to_vec(&record).unwrap(), ""));
        faults.push((
            height_key_v1(config.chain_id, 2).into_bytes(),
            vec![0x55; 32],
            "historical published index",
        ));
        for field in ["genesis", "namespace", "parent_workspace"] {
            let mut intent =
                serde_json::to_value(read_archive(&ledger, 3).unwrap().unwrap()).unwrap();
            intent[field] = serde_json::to_value([0x55u8; 32]).unwrap();
            faults.push((archive_key(3), serde_json::to_vec(&intent).unwrap(), ""));
        }
        for (key, corrupt, expected_error) in faults {
            assert_eq!(read().unwrap().as_ref(), Some(&expected));
            let original = ledger.db.get(&key).unwrap().unwrap();
            {
                let _guard = ledger.write_lock.lock().unwrap();
                ledger.db.put(&key, &corrupt).unwrap();
            }
            let rejected = read();
            // Restore the fixture before asserting: this is an explicit test
            // repair, not any production recovery or silent acceptance path.
            {
                let _guard = ledger.write_lock.lock().unwrap();
                assert_eq!(ledger.db.get(&key).unwrap().as_ref(), Some(&corrupt));
                ledger.db.put(&key, &original).unwrap();
            }
            let error = rejected.expect_err("a new verification accepted corrupted history");
            assert!(
                format!("{error:#}").contains(expected_error),
                "wrong rejection: {error:#}"
            );
            assert_eq!(read().unwrap().as_ref(), Some(&expected));
        }
    }
}
