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

fn accept_prepared(
    state: &mut Transactions,
    channel: &HostChannel,
    config: &super::super::super::channel::ChannelConfig,
    sequence: u64,
    now: Instant,
) -> Result<(TransactionsScope, Hash)> {
    accept_prepared_selected(state, channel, config, sequence, now, None)
}

fn accept_prepared_selected(
    state: &mut Transactions,
    channel: &HostChannel,
    config: &super::super::super::channel::ChannelConfig,
    sequence: u64,
    now: Instant,
    requested: Option<&[bool]>,
) -> Result<(TransactionsScope, Hash)> {
    use crate::native_pipeline::consensus::channel::tests::prepared;
    let mask = state
        .recipient_mask_requested(requested)
        .context("no available requested recipient")?;
    let queued: Vec<_> = mask
        .iter()
        .enumerate()
        .map(|(peer, selected)| *selected && !state.earlier_pending(sequence, peer))
        .collect();
    let scope = TransactionsScope {
        chain_id: config.chain_id,
        genesis: config.genesis,
        protocol: config.protocol,
        epoch: config.validators.epoch(),
        validator_set_hash: config.validators.hash(),
        session: state.session,
        sequence,
    };
    let ready = prepared(
        channel,
        config,
        Arc::new(Message::Transactions {
            scope,
            raw_transactions: vec![sequence.to_le_bytes().to_vec()],
        }),
    );
    let fragment = ready.prepared.fragment_id();
    let requested_count = requested.map_or(state.peers.len(), |selected| {
        selected.iter().filter(|selected| **selected).count()
    });
    state.prepared_admission(sequence, mask, requested_count, now);
    let entry = state.outbox.back_mut().unwrap();
    entry.prepared = Some(ready.prepared);
    entry.queued = queued;
    Ok((scope, fragment))
}

fn release_completed(state: &mut Transactions) {
    // The same completion predicate used by poll_transactions; this pure
    // fixture has no native/network owner or retirement-thread claim.
    state
        .outbox
        .retain(|entry| entry.pending.iter().any(|pending| *pending));
    state.sync_usage();
}

fn assert_bounded(state: &Transactions) {
    assert!(state.outbox.len() <= state.capacity());
    assert!(state.outbox.len() * state.charge <= state.budget.bytes);
    for peer in 0..state.peers.len() {
        assert!(state.pending_for_peer(peer) <= state.peer_capacity());
    }
    assert_eq!(
        state.stats_snapshot().outbound_retained_bytes,
        state.outbox.len() * state.charge
    );
}

#[test]
fn one_silent_peer_cannot_pin_global_outbox_across_many_healthy_batches_without_ttl() -> Result<()>
{
    use crate::native_pipeline::consensus::channel::tests::{config, unstarted};
    let config = config(0);
    let channel = unstarted(&config);
    let mut state = Transactions::new(&channel, config.peers.clone());
    // Exercise the actual 4-validator shape: three remote reservations share
    // the unchanged eight-body ceiling; this extra ID is a flow-only fixture.
    state.peers.push("silent-fixture-peer".into());
    let now = Instant::now();
    let silent = 2;
    let healthy = state.peers[..2].to_vec();
    assert_eq!((state.capacity(), state.peer_capacity()), (8, 2));
    let iterations = state.capacity() * 16;
    for sequence in 1..=iterations as u64 {
        let (scope, fragment) = accept_prepared(&mut state, &channel, &config, sequence, now)?;
        for peer in &healthy {
            assert!(state.return_credit(peer, &scope, fragment, now)?);
        }
        release_completed(&mut state);
        assert_eq!(state.pending_for_peer(silent), (sequence as usize).min(2));
        assert_eq!(state.outbox.len(), (sequence as usize).min(2));
        assert_bounded(&state);
        assert!(state.outbox.iter().all(|entry| entry.created == now));
    }
    assert_eq!(state.stats.outbound_batches_accepted, iterations as u64);
    assert_eq!(
        state.stats.outbound_recipient_skips,
        (iterations - 2) as u64
    );
    assert_eq!(
        state.stats.outbound_recipient_reservations,
        (2 * iterations + 2) as u64
    );
    assert_eq!(state.stats.expired_batches, 0);
    assert_eq!(
        state.stats.peer_credits_returned, 0,
        "pure helper is not a receive/controller ACK observation"
    );
    assert_eq!(
        state.stats_snapshot().outbound_peer_pending[&state.peers[silent]],
        2
    );
    Ok(())
}

#[test]
fn all_recipients_full_refuse_new_obligations_without_changing_existing_slots_or_counters(
) -> Result<()> {
    use crate::native_pipeline::consensus::channel::tests::{config, unstarted};
    let config = config(0);
    let channel = unstarted(&config);
    let mut state = Transactions::new(&channel, config.peers.clone());
    let now = Instant::now();
    for sequence in 1..=state.peer_capacity() as u64 {
        accept_prepared(&mut state, &channel, &config, sequence, now)?;
    }
    assert!(
        state.outbox.len() < state.capacity(),
        "peer quota must apply before global full"
    );
    let before = serde_json::to_value(&state.stats)?;
    let tokens: Vec<_> = state.outbox.iter().map(|entry| entry.token).collect();
    for _ in 0..32 {
        assert!(state.recipient_mask().is_none());
    }
    assert_eq!(serde_json::to_value(&state.stats)?, before);
    assert_eq!(
        state
            .outbox
            .iter()
            .map(|entry| entry.token)
            .collect::<Vec<_>>(),
        tokens
    );
    assert_bounded(&state);
    Ok(())
}

#[test]
fn real_credit_reopens_only_its_peer_and_wrong_duplicate_or_late_credit_cannot_reopen_it(
) -> Result<()> {
    use crate::native_pipeline::consensus::channel::tests::{config, unstarted};
    let config = config(0);
    let channel = unstarted(&config);
    let mut state = Transactions::new(&channel, config.peers.clone());
    state.budget.messages = 2; // Tighten, never enlarge the existing budget.
    let now = Instant::now();
    let peers = state.peers.clone();
    let (first, fragment) = accept_prepared(&mut state, &channel, &config, 1, now)?;
    assert!(state.recipient_mask().is_none());
    assert!(!state.return_credit(&peers[0], &first, [0; 32], now)?);
    let mut wrong = first;
    wrong.session[0] ^= 1;
    assert!(!state.return_credit(&peers[0], &wrong, fragment, now)?);
    assert!(!state.return_credit(&peers[0], &first, fragment, now + state.ttl)?);
    assert!(state.recipient_mask().is_none());
    assert!(state.return_credit(&peers[0], &first, fragment, now)?);
    assert_eq!(state.recipient_mask(), Some(vec![true, false]));
    let (second, second_fragment) = accept_prepared(&mut state, &channel, &config, 2, now)?;
    assert!(state.recipient_mask().is_none());
    assert!(!state.return_credit(&peers[0], &first, fragment, now)?);
    assert!(
        !state.return_credit(&peers[1], &second, second_fragment, now)?,
        "skipped recipient had no obligation to release"
    );
    assert!(state.recipient_mask().is_none());
    assert!(state.return_credit(&peers[0], &second, second_fragment, now)?);
    release_completed(&mut state);
    assert_eq!(state.recipient_mask(), Some(vec![true, false]));
    assert!(state.return_credit(&peers[1], &first, fragment, now)?);
    release_completed(&mut state);
    assert_eq!(state.recipient_mask(), Some(vec![true, true]));
    assert_bounded(&state);
    Ok(())
}

#[test]
fn byte_ceiling_and_owner_pending_preparations_are_charged_before_any_peer_send() {
    let mut state = state();
    state.budget.bytes = state.charge * 4;
    let now = Instant::now();
    assert_eq!((state.capacity(), state.peer_capacity()), (4, 2));
    for token in 1..=2 {
        let recipients = state.recipient_mask().unwrap();
        state.prepared_admission(token, recipients, state.peers.len(), now);
        assert_bounded(&state);
    }
    assert!(state.recipient_mask().is_none());
    assert!(state
        .outbox
        .iter()
        .all(|entry| !entry.expired(now + state.ttl, state.ttl)));
    assert_eq!(
        state.stats_snapshot().outbound_retained_bytes,
        2 * state.charge
    );
}

#[test]
fn detailed_peer_usage_is_query_only_and_does_not_materialize_in_the_poll_counters() -> Result<()> {
    use crate::native_pipeline::consensus::channel::tests::{config, unstarted};
    let config = config(0);
    let channel = unstarted(&config);
    let mut state = Transactions::new(&channel, config.peers.clone());
    let now = Instant::now();
    let (scope, fragment) = accept_prepared(&mut state, &channel, &config, 1, now)?;
    let first_peer = state.peers[0].clone();
    assert!(state.return_credit(&first_peer, &scope, fragment, now)?);
    for _ in 0..128 {
        state.sync_usage();
        assert!(state.stats.outbound_peer_pending.is_empty());
        assert_eq!(state.stats.outbound_retained_bytes, 0);
        assert_eq!(state.stats.outbound_capacity, 0);
        assert_eq!(state.stats.outbound_peer_capacity, 0);
    }
    let snapshot = state.stats_snapshot();
    assert_eq!(snapshot.outbound_batches_accepted, 1);
    assert_eq!(snapshot.outbound_pending, 1);
    assert_eq!(snapshot.outbound_retained_bytes, state.charge);
    assert_eq!(snapshot.outbound_capacity, state.capacity());
    assert_eq!(snapshot.outbound_peer_capacity, state.peer_capacity());
    assert_eq!(snapshot.outbound_peer_pending[&first_peer], 0);
    assert_eq!(snapshot.outbound_peer_pending[&state.peers[1]], 1);
    assert!(state.stats.outbound_peer_pending.is_empty());

    let ttl = state.ttl;
    state.outbox.retain(|entry| !entry.expired(now + ttl, ttl));
    state.sync_usage();
    let after_expiry = state.stats_snapshot();
    assert_eq!(after_expiry.outbound_batches_accepted, 1);
    assert_eq!(after_expiry.outbound_pending, 0);
    assert_eq!(after_expiry.outbound_retained_bytes, 0);
    assert!(after_expiry.outbound_peer_pending.values().all(|n| *n == 0));
    assert_eq!(
        snapshot.outbound_pending, 1,
        "returned snapshot is detached"
    );
    assert!(state.stats.outbound_peer_pending.is_empty());
    Ok(())
}

#[test]
fn targeted_repair_waits_for_requested_credit_and_never_resends_other_peers() -> Result<()> {
    use crate::native_pipeline::consensus::channel::tests::{config, unstarted};
    let config = config(0);
    let channel = unstarted(&config);
    let mut state = Transactions::new(&channel, config.peers.clone());
    state.budget.messages = 2;
    let now = Instant::now();
    let peers = state.peers.clone();
    let (first, first_fragment) = accept_prepared(&mut state, &channel, &config, 1, now)?;
    assert!(state.return_credit(&peers[1], &first, first_fragment, now)?);
    assert_eq!(state.recipient_mask(), Some(vec![false, true]));

    let requested = [true, false];
    let before = serde_json::to_value(state.stats_snapshot())?;
    for _ in 0..64 {
        assert!(state.recipient_mask_requested(Some(&requested)).is_none());
    }
    assert_eq!(serde_json::to_value(state.stats_snapshot())?, before);
    assert_eq!(state.outbox.len(), 1);
    assert_eq!(state.outbox[0].token, 1);

    assert!(state.return_credit(&peers[0], &first, first_fragment, now)?);
    release_completed(&mut state);
    assert_eq!(
        state.recipient_mask_requested(Some(&requested)),
        Some(vec![true, false])
    );
    let (repair, repair_fragment) =
        accept_prepared_selected(&mut state, &channel, &config, 2, now, Some(&requested))?;
    assert_eq!(state.outbox.back().unwrap().pending, vec![true, false]);
    assert_eq!(state.outbox.back().unwrap().queued, vec![true, false]);
    assert_eq!(state.stats.outbound_recipient_reservations, 3);
    assert_eq!(
        state.stats.outbound_recipient_skips, 0,
        "unrequested peer must not count as a skipped reservation"
    );
    assert!(!state.return_credit(&peers[1], &repair, repair_fragment, now)?);
    assert!(state.return_credit(&peers[0], &repair, repair_fragment, now)?);
    assert!(!state.return_credit(&peers[0], &repair, repair_fragment, now)?);
    release_completed(&mut state);
    assert_bounded(&state);
    Ok(())
}

#[test]
fn targeted_reservations_intersect_requested_peers_and_the_unchanged_global_budget() {
    let mut state = state();
    let now = Instant::now();
    assert!(state
        .recipient_mask_requested(Some(&[false, false]))
        .is_none());
    let quota = state.peer_capacity();
    for token in 0..quota {
        let mask = state
            .recipient_mask_requested(Some(&[true, false]))
            .unwrap();
        assert_eq!(mask, vec![true, false]);
        state.prepared_admission(token as u64, mask, 1, now);
    }
    assert_eq!(
        state.recipient_mask_requested(Some(&[true, true])),
        Some(vec![false, true])
    );
    for token in quota..state.capacity() {
        let mask = state
            .recipient_mask_requested(Some(&[false, true]))
            .unwrap();
        state.prepared_admission(token as u64, mask, 1, now);
    }
    assert_eq!(state.outbox.len(), state.capacity());
    assert!(state
        .recipient_mask_requested(Some(&[true, true]))
        .is_none());
    assert_eq!(state.stats.outbound_recipient_skips, 0);
    assert_bounded(&state);
}

#[test]
fn ttl_retires_only_prepared_obligations_and_late_credit_cannot_release_replacement() -> Result<()>
{
    use crate::native_pipeline::consensus::channel::tests::{config, unstarted};
    let config = config(0);
    let channel = unstarted(&config);
    let mut state = Transactions::new(&channel, config.peers.clone());
    state.budget.messages = 2;
    let now = Instant::now();
    let (old, fragment) = accept_prepared(&mut state, &channel, &config, 1, now)?;
    assert!(state.recipient_mask().is_none());
    let expired = now + state.ttl;
    let ttl = state.ttl;
    assert!(!state.outbox[0].expired(expired - Duration::from_nanos(1), ttl));
    state.outbox.retain(|entry| !entry.expired(expired, ttl));
    state.sync_usage();
    assert_eq!(state.recipient_mask(), Some(vec![true, true]));
    accept_prepared(&mut state, &channel, &config, 2, expired)?;
    let peer = state.peers[0].clone();
    assert!(!state.return_credit(&peer, &old, fragment, expired)?);
    assert!(state.recipient_mask().is_none());
    assert_bounded(&state);
    Ok(())
}
