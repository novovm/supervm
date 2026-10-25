use super::*;
use crate::native_pipeline::consensus::wire::Validator;
use novovm_consensus::round_bft::test_vectors::FixtureSigningKey;

fn authority() -> (ParentPoint, ConsensusContext, Arc<ValidatorSet>) {
    let key = FixtureSigningKey::from_bytes(&[19; 32]);
    let member = Validator::new(key.verifying_key().to_bytes(), 1).unwrap();
    let validators = Arc::new(ValidatorSet::new_for_test(71, 1, 1, vec![member]).unwrap());
    let ceiling = ParentPoint {
        height: 8,
        block_hash: [1; 32],
        state_root: [2; 32],
        receipt_batch_commitment: [3; 32],
        state_version: 8,
        decision_hash: [4; 32],
    };
    let context = ConsensusContext {
        chain_id: 71,
        genesis_config_commitment: [5; 32],
        protocol_commitment: [6; 32],
        epoch: 1,
        validator_set_hash: validators.hash(),
        height: 9,
        parent_block_hash: ceiling.block_hash,
        parent_decision_hash: ceiling.decision_hash,
    };
    (ceiling, context, validators)
}

fn locator(index: u8) -> Locator {
    Locator {
        hash: [index; 32],
        height: u64::from(index) + 1,
    }
}

fn query(index: u8, complete: bool) -> Query {
    Query {
        locator: locator(index),
        // Queue-only tests never call poll; production pending queries always
        // own a validated ArchiveRead, not synthetic execution evidence.
        read: None,
        result: complete.then(|| Ok(json!({"lookup_complete":true}))),
    }
}

#[test]
fn receipt_locator_preserves_hash_only_and_accepts_exact_positive_height() -> Result<()> {
    let hash = hex(&[7; 32]);
    assert_eq!(parse_lookup(&json!({"params":[hash]}))?, ([7; 32], None));
    for height in [1, 65_537, u64::MAX] {
        assert_eq!(
            parse_lookup(&json!({"params":[hash,height]}))?,
            ([7; 32], Some(height))
        );
    }
    Ok(())
}

#[test]
fn receipt_locator_rejects_malformed_or_ambiguous_requests() {
    let hash = hex(&[7; 32]);
    for params in [
        json!(null),
        json!({"tx_hash":hash,"block_height":1}),
        json!([]),
        json!([hash, 1, 2]),
        json!([hash, 0]),
        json!([hash, -1]),
        json!([hash, "1"]),
        json!([hash, 1.0]),
        json!([hash, null]),
        json!([hash, true]),
        json!(["0x01", 1]),
        json!([format!("{hash}00"), 1]),
        json!([format!("0x{}", "z".repeat(64)), 1]),
    ] {
        assert!(parse_lookup(&json!({"params":params})).is_err());
    }
}

#[test]
fn historical_queries_deduplicate_exact_locator_and_retain_completed_batch_results() -> Result<()> {
    let mut history = ReceiptHistory::default();
    history.queries.push_back(query(0, false));
    for _ in 0..32 {
        let result = history.existing(locator(0)).unwrap()?;
        assert_eq!(result["lookup_complete"], false);
        assert_eq!(result["finalized"], false);
        assert_eq!(history.len(), 1);
    }
    assert!(history
        .existing(Locator {
            height: 2,
            ..locator(0)
        })
        .is_none());
    history.queries[0].result = Some(Ok(json!({"lookup_complete":true})));
    for _ in 0..32 {
        assert_eq!(
            history.existing(locator(0)).unwrap()?["lookup_complete"],
            true
        );
        assert_eq!(history.len(), 1);
    }
    Ok(())
}

#[test]
fn historical_query_checks_local_ceiling_before_allocation_or_ready_eviction() -> Result<()> {
    let (ceiling, context, validators) = authority();
    let mut history = ReceiptHistory {
        queries: (0..MAX_QUERIES as u8).map(|i| query(i, true)).collect(),
    };
    for height in [0, ceiling.height + 1, u64::MAX] {
        assert!(history
            .lookup([90; 32], height, ceiling, context, validators.clone())
            .is_err());
        assert_eq!(history.len(), MAX_QUERIES);
        assert_eq!(history.queries.front().unwrap().locator, locator(0));
    }
    let mut invalid_context = context;
    invalid_context.epoch += 1;
    assert!(history
        .lookup([90; 32], 1, ceiling, invalid_context, validators.clone())
        .is_err());
    assert_eq!(history.len(), MAX_QUERIES);
    assert_eq!(history.queries.front().unwrap().locator, locator(0));
    let result = history.lookup([90; 32], 1, ceiling, context, validators.clone())?;
    assert_eq!(result["state"], "query_pending");
    assert_eq!(history.len(), MAX_QUERIES);
    assert!(history.queries.back().unwrap().read.is_some());
    for _ in 0..32 {
        assert_eq!(
            history.lookup([90; 32], 1, ceiling, context, validators.clone())?,
            result
        );
        assert_eq!(history.len(), MAX_QUERIES);
    }
    Ok(())
}

#[test]
fn historical_backpressure_never_discards_pending_and_reclaims_abandoned_results() {
    let mut history = ReceiptHistory {
        queries: (0..MAX_QUERIES as u8).map(|i| query(i, false)).collect(),
    };
    assert!(history.reserve_slot().is_err());
    assert_eq!(history.len(), MAX_QUERIES);
    history.queries[3].result = Some(Ok(json!({"lookup_complete":true})));
    history.reserve_slot().unwrap();
    assert_eq!(history.len(), MAX_QUERIES - 1);
    assert!(!history.queries.iter().any(|q| q.locator == locator(3)));
    assert!(history.queries.iter().all(|q| q.result.is_none()));
    history.queries.push_back(query(8, true));
    for next in 9..32 {
        history.reserve_slot().unwrap();
        history.queries.push_back(query(next, true));
        assert_eq!(history.len(), MAX_QUERIES);
    }
    assert_eq!(history.queries.back().unwrap().locator, locator(31));
}

#[test]
fn historical_failures_are_cached_errors_never_unknown_or_finalized_and_can_be_evicted() {
    let mut history = ReceiptHistory::default();
    let mut failed = query(0, false);
    failed.result = Some(Err("archive decision outbox missing".into()));
    history.queries.push_back(failed);
    let error = history.existing(locator(0)).unwrap().unwrap_err();
    assert_eq!(error.to_string(), "archive decision outbox missing");
    assert_eq!(history.len(), 1);
    for index in 1..MAX_QUERIES as u8 {
        history.queries.push_back(query(index, false));
    }
    history.reserve_slot().unwrap();
    assert_eq!(history.len(), MAX_QUERIES - 1);
    assert!(history.existing(locator(0)).is_none());
    assert!(completed_receipt(locator(0), None)
        .unwrap_err()
        .to_string()
        .contains("decided block missing"));
}

fn pending_history(count: u8) -> Result<ReceiptHistory> {
    let (ceiling, context, validators) = authority();
    let mut history = ReceiptHistory::default();
    for index in 0..count {
        let locator = locator(index);
        history.lookup(
            locator.hash,
            locator.height,
            ceiling,
            context,
            validators.clone(),
        )?;
    }
    Ok(history)
}

#[test]
fn historical_poll_has_one_attempt_per_turn_and_round_robin_survives_ready_eviction() -> Result<()>
{
    let mut history = pending_history(MAX_QUERIES as u8)?;
    for turn in 0..24 {
        let mut calls = 0;
        history.poll_one(|query| {
            calls += 1;
            assert_eq!(query.locator, locator((turn % MAX_QUERIES) as u8));
        });
        assert_eq!(calls, 1);
    }
    // Completing query 0 moves it behind the pending queue; query 1 is next.
    history.poll_one(|query| {
        assert_eq!(query.locator, locator(0));
        query.read = None;
        query.result = Some(Ok(json!({"lookup_complete":true})));
    });
    history.reserve_slot()?;
    assert_eq!(history.len(), MAX_QUERIES - 1);
    assert!(!history.queries.iter().any(|q| q.locator == locator(0)));
    for turn in 1..MAX_QUERIES {
        history.poll_one(|query| {
            assert_eq!(query.locator, locator(turn as u8));
            query.read = None;
            query.result = Some(Ok(json!({"lookup_complete":true})));
        });
    }
    // All finished slots are skipped without polling or restarting them.
    history.poll_one(|_| panic!("ready query must never be polled"));
    Ok(())
}

#[test]
fn historical_single_metadata_slot_completion_leaves_next_control_turn_free() -> Result<()> {
    let mut history = pending_history(MAX_QUERIES as u8)?;
    // Model the existing one-slot metadata budget: submitting retains the slot
    // through worker completion until that same query consumes its ticket. The
    // narrow helper is the exact scheduling boundary used by production poll.
    let mut occupied_by = None;
    let mut released_last_turn = false;
    let mut control_opportunities = 0;
    let mut completions = 0;
    for _ in 0..80 {
        if released_last_turn {
            assert!(
                occupied_by.is_none(),
                "historical query reacquired freed slot"
            );
            control_opportunities += 1;
        }
        released_last_turn = false;
        let mut attempts = 0;
        history.poll_one(|query| {
            attempts += 1;
            match occupied_by {
                Some(owner) if owner == query.locator => {
                    occupied_by = None;
                    released_last_turn = true;
                    completions += 1;
                }
                None => occupied_by = Some(query.locator),
                Some(_) => (), // Another retained ticket imposes backpressure.
            }
        });
        assert_eq!(attempts, 1);
    }
    assert!(completions >= MAX_QUERIES);
    assert!(control_opportunities >= MAX_QUERIES);
    Ok(())
}
