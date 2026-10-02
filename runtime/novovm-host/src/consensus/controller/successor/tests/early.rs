//! Real AOEM/journal/assembly owners, with deliberately controlled delivery of
//! their replies. These are not native-preemption or four-node network tests.
use super::*;
use crate::consensus::transport::EarlyBodyScope;

fn announcement(scope: EarlyBodyScope, extra: u128) -> Result<Arc<Message>> {
    Ok(Arc::new(Message::EarlyBody {
        scope,
        raw_transactions: raw(scope.target_height, extra)?,
    }))
}

fn start_early(controller: &mut Controller, message: &Arc<Message>) -> Result<u64> {
    flush(controller)?;
    ensure!(
        controller.try_submit_early_body(message, 1, 172_800_502)?,
        "early input not admitted"
    );
    controller
        .preparing
        .values()
        .filter_map(|p| match &p.purpose {
            Purpose::EarlyBody(generation) => Some(*generation),
            _ => None,
        })
        .max()
        .context("local early preparation has no generation")
}

fn prepare_early(controller: &mut Controller, message: &Arc<Message>) -> Result<Hash> {
    let generation = start_early(controller, message)?;
    pump_owner(controller, |c| !c.preparing.contains_key(&generation))?;
    controller
        .fixed
        .iter()
        .find_map(|f| f.prepared.early_id())
        .context("local owner-prepared early identity absent")
}

fn step_early(controller: &mut Controller, pipeline: &CandidatePipeline) -> Result<()> {
    flush(controller)?;
    controller.flush_preparations()?;
    match controller.channel.try_recv()? {
        Some(ChannelEvent::Prepared { token, result }) => controller.prepared(token, result)?,
        Some(ChannelEvent::Received(received)) => {
            controller.receive(received.peer, received.ready)?
        }
        None => (),
    }
    flush(controller)?;
    controller.poll_early(pipeline)
}

fn until_early(
    controller: &mut Controller,
    pipeline: &CandidatePipeline,
    done: impl Fn(&Controller) -> bool,
) -> Result<()> {
    let deadline = Instant::now() + DEADLINE;
    while !done(controller) {
        ensure!(
            Instant::now() < deadline,
            "early transition timed out: {:?}",
            controller.stats()
        );
        step_early(controller, pipeline)?;
        std::thread::yield_now();
    }
    Ok(())
}

fn authenticated_input(controller: &Controller, id: Hash, current: bool) -> Result<()> {
    let body = if current {
        controller.bodies.get(&id)
    } else {
        controller.successor.as_ref().and_then(|s| s.body.as_ref())
    }
    .context("canonical body missing")?;
    ensure!(
        body.prepared.body_id() == Some(id)
            && body.authenticated.is_some()
            && body.request.is_none()
            && body.candidate.is_none()
            && body.early_origin.is_some(),
        "canonical body did not retain exactly the authenticated input"
    );
    Ok(())
}

fn economic_oracle(
    pipeline: &CandidatePipeline,
    parent: ParentPoint,
    child: &DurableCandidate,
) -> Result<()> {
    let mut ordinary = submit(
        pipeline,
        BatchRequest::new(raw(2, 0)?, execution(parent), policy())?,
    )?;
    let deadline = Instant::now() + DEADLINE;
    let ordinary = loop {
        ensure!(
            Instant::now() < deadline,
            "ordinary economic oracle timed out"
        );
        if let Some(batch) = ordinary.try_take()? {
            break batch;
        }
        std::thread::yield_now();
    };
    ensure!(
        ordinary.observation.peak_callbacks > 0
            && ordinary.packet.records() == child.packet().records(),
        "early and old full execution differ"
    );
    // max_pay_amount=0 selects the automatic quote cap, NOT a zero fee.
    // Amounts 100/50 to a 20-byte account have 80/79 argument bytes, hence
    // each transfer costs 40 + ceil(bytes/16) = 45 in addition to its amount.
    for (seed, amount) in [(1, 999_710u128), (3, 999_810), (2, 300)] {
        let bytes = read(
            pipeline,
            child.packet().state_root(),
            balance_key(&account(seed)),
        )?;
        let actual = u128::from_le_bytes(
            bytes
                .as_slice()
                .try_into()
                .context("invalid balance width")?,
        );
        ensure!(
            actual == amount,
            "two-height balance projection differs: seed={seed} actual={actual} expected={amount}"
        );
    }
    for raw in raw(2, 0)? {
        let authenticated = authenticate_transfer_v3(&raw, CHAIN, 1024)?;
        ensure!(
            read(
                pipeline,
                child.packet().state_root(),
                nonce_key(&authenticated.nonce_identity())
            )? == 2u64.to_le_bytes(),
            "two-height nonce differs"
        );
    }
    // Full immutable record comparison above also preserves the nonzero fee
    // pages and receipts; four successful transfers settle 4 * 45 = 180.
    Ok(())
}

fn current_proposal_gate(
    controller: &mut Controller,
    pipeline: &CandidatePipeline,
    parent: ParentPoint,
) -> Result<()> {
    controller.drive_consensus()?;
    ensure!(
        controller.is_pending() && controller.head() == Some(parent),
        "current proposal skipped journal ACK"
    );
    acknowledge(controller, pipeline)?;
    let Some(DurableMessage::Proposal(proposal)) = controller.journal.last_durable_message() else {
        bail!("authenticated current input did not propose after ACK");
    };
    ensure!(
        proposal.context == controller.context()
            && proposal.context.height == 2
            && controller.head() == Some(parent),
        "proposal has wrong context or changed finalized head"
    );
    Ok(())
}

#[test]
#[ignore = "requires explicit real AOEM; authentication before any parent, sticky binding, future signing gate and full execution oracle"]
fn real_auth_before_parent_rejects_wrong_source_and_sticky_rebinding_then_closes() -> Result<()> {
    Fixture::run("early-auth-first", |controller, pipeline| {
        let scope = controller
            .early_body_scope()?
            .context("fixture not next proposer")?;
        let message = announcement(scope, 0)?;
        let wrong_peer = controller.config.peers.values().next().unwrap().clone();
        let ready = owner_ready(controller, message.clone())?;
        controller.keep_early(wrong_peer.clone(), ready, None)?;
        flush(controller)?;
        let mut wrong_scope = scope;
        wrong_scope.source_round += 1;
        let ready = owner_ready(controller, announcement(wrong_scope, 0)?)?;
        controller.keep_early(controller.local_peer.clone(), ready, None)?;
        flush(controller)?;
        // A binding alone cannot create work, even before there is any parent.
        controller.receive_early_binding(
            &controller.local_peer.clone(),
            scope,
            [7; 32],
            execution(controller.parent()),
        )?;
        controller.poll_early(pipeline)?;
        ensure!(
            controller.early.is_none()
                && controller.successor.is_none()
                && controller.bodies.is_empty()
                && controller.stats.early_authentication_started == 0,
            "wrong source/scope or missing announcement created work"
        );

        let id = prepare_early(controller, &message)?;
        until_early(controller, pipeline, |c| {
            c.stats.early_authentication_completed == 1
        })?;
        ensure!(
            controller.stats.early_authentication_started == 1
                && controller
                    .stats
                    .early_authentication_completed_before_parent
                    == 1
                && controller.stats.executed_batches == 0
                && controller.bodies.is_empty()
                && controller.successor.is_none()
                && controller.head().is_none(),
            "authentication was not really completed before any local parent"
        );
        controller.drive_consensus()?;
        ensure!(
            !controller.is_pending() && controller.journal.last_durable_message().is_none(),
            "authentication alone granted signing authority"
        );
        let parent = executed_parent(controller, pipeline, 0, controller.local_peer.clone())?;
        let parent_point = point(controller, &parent)?;
        let exact = execution(parent_point);
        let local = controller.local_peer.clone();
        let rejected = controller.stats.rejected_messages;
        controller.receive_early_binding(&wrong_peer, scope, id, exact)?;
        controller.receive_early_binding(&local, wrong_scope, id, exact)?;
        let mut wrong_id = id;
        wrong_id[0] ^= 1;
        controller.receive_early_binding(&local, scope, wrong_id, exact)?;
        controller.receive_early_binding(&local, scope, id, exact)?;
        let mut changed = exact;
        changed.timestamp_unix_ms += 1;
        controller.receive_early_binding(&local, scope, id, changed)?;
        changed = exact;
        changed.parent_state_root[0] ^= 1;
        controller.receive_early_binding(&local, scope, id, changed)?;
        ensure!(
            controller.stats.rejected_messages == rejected + 5,
            "invalid/sticky binding was not rejected"
        );
        until_early(controller, pipeline, |c| c.stats.early_bind_reused == 1)?;
        let body_id = controller
            .successor
            .as_ref()
            .and_then(|s| s.body.as_ref())
            .and_then(|b| b.prepared.body_id())
            .context("future canonical body absent")?;
        authenticated_input(controller, body_id, false)?;
        flush(controller)?;
        let deadline = Instant::now() + DEADLINE;
        loop {
            ensure!(
                Instant::now() < deadline,
                "authenticated background execution timed out"
            );
            controller.poll_successor(pipeline)?;
            if controller
                .successor
                .as_ref()
                .is_some_and(|s| s.body.as_ref().is_some_and(|b| b.candidate.is_some()))
            {
                break;
            }
            flush(controller)?;
            std::thread::yield_now();
        }
        let body = controller
            .successor
            .as_ref()
            .unwrap()
            .body
            .as_ref()
            .unwrap();
        ensure!(
            body.authenticated.is_none() && body.request.is_none(),
            "authenticated body was not consumed"
        );
        let child = body.candidate.as_ref().unwrap().1.clone();
        ensure!(
            child.packet().context() == &exact
                && controller.stats.early_authentication_started == 1
                && controller.stats.successor_completed_before_parent == 1,
            "binding changed or authentication repeated"
        );
        controller.drive_consensus()?;
        ensure!(
            controller.head().is_none()
                && !controller.is_pending()
                && controller.journal.last_durable_message().is_none()
                && controller.journal.propose(&child, None).is_err(),
            "future completion obtained signing/head authority"
        );
        decide_and_advance(controller, pipeline, &parent)?;
        ensure!(
            controller
                .bodies
                .get(&body_id)
                .is_some_and(|b| b.candidate.is_some()),
            "exact parent failed to promote early result"
        );
        current_proposal_gate(controller, pipeline, parent_point)?;
        economic_oracle(pipeline, parent_point, &child)
    })
}

#[test]
#[ignore = "requires explicit real AOEM; parent ACK crosses retained authentication ticket or canonical owner preparation"]
fn real_parent_ack_during_auth_or_canonical_delivery_keeps_one_authenticated_execution(
) -> Result<()> {
    for canonical_pending in [false, true] {
        Fixture::run(
            if canonical_pending {
                "early-ack-canonical"
            } else {
                "early-ack-auth"
            },
            |controller, pipeline| {
                let scope = controller
                    .early_body_scope()?
                    .context("fixture not next proposer")?;
                prepare_early(controller, &announcement(scope, 0)?)?;
                if canonical_pending {
                    until_early(controller, pipeline, |c| {
                        c.stats.early_authentication_completed == 1
                    })?;
                } else {
                    flush(controller)?;
                    controller.poll_early(pipeline)?;
                    ensure!(
                        controller.early.as_ref().is_some_and(EarlyWork::has_ticket)
                            && controller.stats.early_authentication_started == 1
                            && controller.stats.early_authentication_completed == 0,
                        "authentication reply was consumed too soon"
                    );
                }
                let parent =
                    executed_parent(controller, pipeline, 0, controller.local_peer.clone())?;
                let parent_point = point(controller, &parent)?;
                if canonical_pending {
                    flush(controller)?;
                    controller.poll_early(pipeline)?;
                    ensure!(
                        controller
                            .preparing
                            .values()
                            .any(|p| matches!(p.purpose, Purpose::BindEarly(_)))
                            && controller.stats.early_bind_reused == 0,
                        "canonical owner preparation was not held"
                    );
                }
                // Only reply delivery is delayed. The AOEM callback is not blocked
                // or timed, and may already have finished before this parent ACK.
                decide_and_advance(controller, pipeline, &parent)?;
                ensure!(
                    controller.early.is_some() && controller.stats.early_bind_reused == 0,
                    "parent ACK discarded or prematurely installed pending early input"
                );
                until_early(controller, pipeline, |c| c.stats.early_bind_reused == 1)?;
                let id = controller
                    .bodies
                    .iter()
                    .find_map(|(id, b)| b.early_origin.is_some().then_some(*id))
                    .context("promoted canonical body absent")?;
                authenticated_input(controller, id, true)?;
                flush(controller)?;
                controller.submit_executions(pipeline)?;
                ensure!(
                    controller.bodies[&id].authenticated.is_none()
                        && controller.bodies[&id].request.is_none()
                        && controller.inflight.iter().any(|work| work.id == id),
                    "current path did not consume authenticated input once"
                );
                let child = wait_candidate(controller, id)?;
                ensure!(
                    controller.stats.early_authentication_started == 1
                        && controller.stats.early_authentication_completed == 1
                        && controller
                            .stats
                            .early_authentication_completed_before_parent
                            == u64::from(canonical_pending)
                        && controller.stats.early_bind_reused == 1
                        && controller.stats.successor_started == 0
                        && controller.stats.executed_batches == 2
                        && controller.head() == Some(parent_point),
                    "ACK race repeated auth/execution or bypassed current parent"
                );
                current_proposal_gate(controller, pipeline, parent_point)?;
                economic_oracle(pipeline, parent_point, &child)
            },
        )?;
    }
    Ok(())
}

#[test]
#[ignore = "requires explicit real AOEM; round drains accepted authentication and stale owner generation cannot replace new work"]
fn real_round_retirement_drains_auth_and_late_generation_does_not_revive() -> Result<()> {
    Fixture::run("early-round-generation", |controller, pipeline| {
        let scope = controller
            .early_body_scope()?
            .context("fixture not next proposer")?;
        prepare_early(controller, &announcement(scope, 0)?)?;
        flush(controller)?;
        controller.poll_early(pipeline)?;
        ensure!(
            controller.early.as_ref().is_some_and(EarlyWork::has_ticket),
            "no actual authentication ticket"
        );
        controller.journal.round_wait_elapsed(0)?;
        acknowledge(controller, pipeline)?;
        ensure!(
            controller.early.is_none()
                && controller.early_drain.is_some()
                && controller.stats.early_discarded == 1,
            "round retirement lost accepted ticket"
        );
        until_early(controller, pipeline, |c| c.early_drain.is_none())?;
        ensure!(
            controller.stats.early_authentication_completed == 0
                && controller.stats.stale_results >= 1
                && controller.bodies.is_empty()
                && controller.successor.is_none(),
            "late auth revived old input"
        );
        flush(controller)?;
        let second_scope = controller
            .early_body_scope()?
            .context("new round did not release optional scope")?;
        let old_generation = start_early(controller, &announcement(second_scope, 0)?)?;
        // Leave this real assembly reply unread across a second round change.
        controller.journal.round_wait_elapsed(1)?;
        acknowledge(controller, pipeline)?;
        let third_scope = controller
            .early_body_scope()?
            .context("second new round not available")?;
        let new_generation = start_early(controller, &announcement(third_scope, 0)?)?;
        ensure!(
            new_generation != old_generation,
            "local owner generation reused"
        );
        let stale = controller.stats.stale_results;
        pump_owner(controller, |c| {
            !c.preparing.contains_key(&old_generation) && !c.preparing.contains_key(&new_generation)
        })?;
        ensure!(
            controller.stats.stale_results > stale && controller.has_early_target(2),
            "old owner reply erased or revived over new generation"
        );
        until_early(controller, pipeline, |c| {
            c.stats.early_authentication_completed == 1
        })?;
        ensure!(
            controller.stats.early_authentication_started == 2
                && controller
                    .stats
                    .early_authentication_completed_before_parent
                    == 1
                && controller.stats.executed_batches == 0
                && controller.head().is_none()
                && controller
                    .fixed
                    .iter()
                    .all(|f| !matches!(f.prepared.message().as_ref(),
                Message::EarlyBody { scope, .. } if *scope != third_scope)),
            "stale generation reauthenticated, executed or retained old raw cache"
        );
        Ok(())
    })
}

#[test]
#[ignore = "requires explicit real AOEM; one-slot exclusion, bad-signature tombstone, old full fallback and actual cold reopen"]
fn real_early_static_gate_and_failed_scope_tombstone_preserve_full_fallback_and_cold_state(
) -> Result<()> {
    let mut fixture = Fixture::new("early-tombstone-fallback")?;
    let controller = fixture.controller.as_mut().unwrap();
    let pipeline = fixture.pipeline.as_ref().unwrap();
    let scope = controller
        .early_body_scope()?
        .context("fixture not next proposer")?;
    let good = announcement(scope, 0)?;
    let original_limit = controller.config.limits.max_inflight;
    controller.config.limits.max_inflight = 1;
    validate_config(&controller.config, &controller.journal)?;
    ensure!(
        controller.early_body_scope()?.is_none()
            && !controller.try_submit_early_body(&good, 1, 172_800_502)?,
        "single current slot admitted optional early work"
    );
    let ready = owner_ready(controller, good.clone())?;
    controller.keep_early(controller.local_peer.clone(), ready, None)?;
    ensure!(
        controller.early.is_none(),
        "remote path bypassed the one-slot gate"
    );
    controller.config.limits.max_inflight = original_limit;
    flush(controller)?;
    let mut bad = raw(2, 0)?;
    *bad[0].last_mut().unwrap() ^= 1;
    prepare_early(
        controller,
        &Arc::new(Message::EarlyBody {
            scope,
            raw_transactions: bad,
        }),
    )?;
    until_early(controller, pipeline, |c| c.early.is_none())?;
    ensure!(
        controller.stats.early_authentication_started == 1
            && controller.stats.early_authentication_completed == 0
            && controller.stats.early_discarded == 1,
        "bad signature did not fail actual authentication"
    );
    flush(controller)?;
    ensure!(
        controller.early_body_scope()?.is_none()
            && !controller.try_submit_early_body(&good, 1, 172_800_502)?,
        "retired source/round reauthenticated new raw bytes"
    );
    let ready = owner_ready(controller, good)?;
    controller.keep_early(controller.local_peer.clone(), ready, None)?;
    flush(controller)?;
    controller.poll_early(pipeline)?;
    ensure!(
        controller.early.is_none() && controller.stats.early_authentication_started == 1,
        "same-scope incoming announcement bypassed tombstone"
    );
    let parent = executed_parent(controller, pipeline, 0, controller.local_peer.clone())?;
    let parent_point = decide_and_advance(controller, pipeline, &parent)?;
    let ready = owner_ready(controller, body(parent_point, 0)?)?;
    let id = ready
        .prepared
        .body_id()
        .context("full fallback identity absent")?;
    controller.keep_body(controller.local_peer.clone(), ready, Some(0))?;
    ensure!(
        controller.bodies[&id].request.is_some() && controller.bodies[&id].authenticated.is_none(),
        "old full fallback invented an authenticated token"
    );
    flush(controller)?;
    controller.submit_executions(pipeline)?;
    let child = wait_candidate(controller, id)?;
    economic_oracle(pipeline, parent_point, &child)?;
    ensure!(
        controller.stats.early_bind_reused == 0 && controller.stats.executed_batches == 2,
        "failed early scope suppressed or duplicated old full execution"
    );
    current_proposal_gate(controller, pipeline, parent_point)?;
    let store_config = fixture.pipeline_config.store.clone();
    fixture.shutdown()?;
    let store = CandidateStore::open(store_config, OpenMode::Existing)?;
    let cold = store
        .recover(child.packet().candidate_id())?
        .context("full fallback missing after actual DB reopen")?;
    ensure!(
        child.packet().matches(&cold) && cold.raw_transactions() == raw(2, 0)?,
        "cold fallback records differ"
    );
    for (seed, amount) in [(1, 999_710u128), (3, 999_810), (2, 300)] {
        let bytes = crate::state::tree::read_state_value(
            &store,
            child.packet().state_root(),
            &balance_key(&account(seed)),
        )?
        .context("cold fallback balance absent")?;
        let actual = u128::from_le_bytes(
            bytes
                .as_slice()
                .try_into()
                .context("invalid cold balance width")?,
        );
        ensure!(
            actual == amount,
            "cold fallback balance differs: seed={seed} actual={actual} expected={amount}"
        );
    }
    for raw in raw(2, 0)? {
        let authenticated = authenticate_transfer_v3(&raw, CHAIN, 1024)?;
        ensure!(
            crate::state::tree::read_state_value(
                &store,
                child.packet().state_root(),
                &nonce_key(&authenticated.nonce_identity())
            )? == Some(2u64.to_le_bytes().to_vec()),
            "cold fallback nonce differs"
        );
    }
    Ok(())
}
