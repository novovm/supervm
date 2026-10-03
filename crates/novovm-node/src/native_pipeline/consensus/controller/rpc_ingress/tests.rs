use super::*;

fn scope() -> TransactionsScope {
    TransactionsScope {
        chain_id: 292,
        genesis: [1; 32],
        protocol: [2; 32],
        epoch: 1,
        validator_set_hash: [3; 32],
        session: [4; 32],
        sequence: 1,
    }
}
fn state() -> Transactions {
    Transactions {
        session: [8; 32],
        sequence: 0,
        peers: vec!["a".into(), "b".into()],
        outbox: VecDeque::new(),
        credits: VecDeque::new(),
        inbox: BTreeMap::new(),
        replay: BTreeMap::new(),
        receive_turn: 0,
        budget: QueueBudget {
            messages: 8,
            bytes: 800,
        },
        charge: 100,
        ttl: Duration::from_secs(2),
        stats: TransactionsStats::default(),
    }
}

#[test]
fn replay_hint_rejects_duplicate_and_changed_body_but_new_process_can_restart_sequence(
) -> Result<()> {
    let mut state = state();
    let now = Instant::now();
    let mut scope = scope();
    assert!(state.check_replay("a", &scope, [5; 32], now)?);
    state.remember("a".into(), &scope, [5; 32], now);
    assert!(!state.check_replay("a", &scope, [5; 32], now)?);
    assert!(!state.check_replay("a", &scope, [6; 32], now)?);
    assert_eq!(state.stats.duplicate_batches, 1);
    assert_eq!(state.stats.conflicting_sequences, 1);
    scope.session = [9; 32];
    assert!(state.check_replay("a", &scope, [6; 32], now)?);
    assert_eq!(
        state.stats.inbound_batches, 0,
        "replay check is not pool/receipt admission"
    );
    Ok(())
}

#[test]
fn replay_is_per_peer_finite_and_expires_without_becoming_durable_authority() -> Result<()> {
    let mut state = state();
    let now = Instant::now();
    let mut scope = scope();
    assert!(state
        .check_replay("not-configured", &scope, [5; 32], now)
        .is_err());
    assert!(state.replay.is_empty());
    for sequence in 1..=128 {
        scope.sequence = sequence;
        state.remember("a".into(), &scope, [5; 32], now);
    }
    assert_eq!(state.replay["a"].len(), REPLAY_HINTS_PER_PEER);
    assert!(state.check_replay("b", &scope, [5; 32], now)?);
    assert!(state.check_replay("a", &scope, [5; 32], now + state.ttl)?);
    assert!(state.replay["a"].is_empty());
    assert_eq!(state.capacity(), 8);
    assert_eq!(state.peer_capacity(), 4);
    state.budget.bytes = 600;
    assert_eq!(
        state.peer_capacity(),
        3,
        "bytes as well as count constrain each peer"
    );
    Ok(())
}

#[test]
fn every_pinned_domain_field_is_checked_without_height_or_parent_authority() -> Result<()> {
    let scope = scope();
    let context = Context {
        chain_id: scope.chain_id,
        genesis_config_commitment: scope.genesis,
        protocol_commitment: scope.protocol,
        epoch: scope.epoch,
        validator_set_hash: scope.validator_set_hash,
        height: 99,
        parent_block_hash: [6; 32],
        parent_decision_hash: [7; 32],
    };
    check_scope(&scope, context)?;
    for field in 0..5 {
        let mut wrong = scope;
        match field {
            0 => wrong.chain_id += 1,
            1 => wrong.genesis[0] ^= 1,
            2 => wrong.protocol[0] ^= 1,
            3 => wrong.epoch += 1,
            _ => wrong.validator_set_hash[0] ^= 1,
        }
        assert!(check_scope(&wrong, context).is_err());
    }
    Ok(())
}

#[test]
fn credit_matches_exact_peer_scope_and_bytes_once_without_authorizing_any_finality() -> Result<()> {
    credit_case(false)
}

#[test]
fn apfl_credit_matches_exact_peer_scope_and_bytes_once_without_authorizing_any_finality(
) -> Result<()> {
    credit_case(true)
}

fn credit_case(apfl: bool) -> Result<()> {
    use crate::native_pipeline::consensus::channel::tests::{config, prepared, unstarted};
    let config = config(0);
    let channel = unstarted(&config);
    let mut state = Transactions::new(&channel, config.peers.clone());
    let now = Instant::now();
    let scope = TransactionsScope {
        chain_id: config.chain_id,
        genesis: config.genesis,
        protocol: config.protocol,
        epoch: config.validators.epoch(),
        validator_set_hash: config.validators.hash(),
        session: state.session,
        sequence: 1,
    };
    let message = if apfl {
        use crate::native_pipeline::ingress::{
            apfl::ApflLimits,
            wire::{encode_transfer_v3, FeePolicy, TransferV3},
        };
        // Structurally canonical but NOT signed: flow credit must never confer
        // authentication, even for valid structural columns.
        let raw = encode_transfer_v3(&TransferV3 {
            chain_id: config.chain_id,
            from: vec![1; 20],
            to: vec![2; 20],
            asset: "NOV".into(),
            amount: 1,
            nonce: 0,
            fee_policy: FeePolicy {
                pay_asset: "NOV".into(),
                max_pay_amount: 0,
                slippage_bps: 0,
            },
            signature: vec![0; 96],
        })?;
        Arc::new(Message::ApflTransactions {
            scope,
            batch: Arc::new(ApflTransferBatch::from_raw(
                &[raw],
                ApflLimits {
                    transactions: config.codec.transactions,
                    transaction_bytes: config.codec.transaction_bytes,
                    body_bytes: config.codec.body_bytes,
                },
            )?),
        })
    } else {
        Arc::new(Message::Transactions {
            scope,
            raw_transactions: vec![vec![1, 2, 3]],
        })
    };
    let alternate_fragment = if let Message::ApflTransactions { scope, batch } = message.as_ref() {
        Some(
            prepared(
                &channel,
                &config,
                Arc::new(Message::Transactions {
                    scope: *scope,
                    raw_transactions: (0..batch.len())
                        .map(|i| batch.canonical_raw(i))
                        .collect::<Result<_>>()?,
                }),
            )
            .prepared
            .fragment_id(),
        )
    } else {
        None
    };
    let ready = prepared(&channel, &config, message);
    assert!(matches!(ready.evidence.as_ref(), VerifiedEvidence::None));
    assert!(
        ready.body.is_none() && ready.early.is_none(),
        "raw gossip created executable request"
    );
    let fragment = ready.prepared.fragment_id();
    state.outbox.push_back(PendingSend {
        token: 1,
        prepared: Some(ready.prepared),
        created: now,
        next_peer: 0,
        pending: vec![true, true],
        queued: vec![true, false],
    });
    let first = state.peers[0].clone();
    let second = state.peers[1].clone();
    if let Some(alternate) = alternate_fragment {
        assert_ne!(
            alternate, fragment,
            "raw and structural transport fragments must differ"
        );
        assert!(
            !state.return_credit(&first, &scope, alternate, now)?,
            "equivalent semantic transactions released a different transport credit"
        );
    }
    assert!(
        !state.return_credit(&second, &scope, fragment, now)?,
        "unsent peer released window"
    );
    assert!(state
        .return_credit("unconfigured", &scope, fragment, now)
        .is_err());
    assert!(!state.return_credit(&first, &scope, [8; 32], now)?);
    for field in 0..7 {
        let mut wrong = scope;
        match field {
            0 => wrong.session[0] ^= 1,
            1 => wrong.sequence += 1,
            2 => wrong.chain_id += 1,
            3 => wrong.genesis[0] ^= 1,
            4 => wrong.protocol[0] ^= 1,
            5 => wrong.epoch += 1,
            _ => wrong.validator_set_hash[0] ^= 1,
        }
        assert!(!state.return_credit(&first, &wrong, fragment, now)?);
    }
    assert!(state.return_credit(&first, &scope, fragment, now)?);
    assert!(
        !state.return_credit(&first, &scope, fragment, now)?,
        "duplicate released another window"
    );
    assert_eq!(state.outbox[0].pending, vec![false, true]);
    assert_eq!(state.stats.inbound_batches, 0);
    // Reusing exact old bytes after TTL still cannot accept a late flow ACK.
    state.outbox[0].pending[0] = true;
    assert!(!state.return_credit(&first, &scope, fragment, now + state.ttl)?);
    Ok(())
}

#[test]
fn blocked_receiver_window_does_not_stop_healthy_peer_or_resend_taken_peer() {
    let mut state = state();
    let now = Instant::now();
    state.outbox.push_back(PendingSend {
        token: 1,
        prepared: None,
        created: now,
        next_peer: 0,
        pending: vec![true, false],
        queued: vec![true, true],
    });
    assert!(
        state.earlier_pending(2, 0),
        "blocked peer gained a second unconsumed batch"
    );
    assert!(
        !state.earlier_pending(2, 1),
        "blocked peer pinned a healthy receiver"
    );
    assert!(
        !state.outbox[0].pending[1],
        "returned credit reactivated old transmission"
    );
    state.outbox.pop_front();
    assert!(
        !state.earlier_pending(2, 0),
        "retired TTL slot permanently blocked the peer"
    );
}
