//! Bounded fault injection at the original lifecycle boundary. Two controlled
//! peers sign their own higher-round votes; the real collector/journal checks
//! them and durably changes round. No QC, parent ACK or execution is fabricated.
//! This checks accepted-ticket retirement, NOT complete network round recovery.
use super::*;
use crate::native_pipeline::{
    consensus::wire::Phase,
    pipeline::{AuthenticationRequest, AuthenticationSubmission},
};

#[test]
#[ignore = "real AOEM; accepted auth ticket retired after verified durable round change"]
fn native_successor_auth_ticket_drains_after_source_round_changes() -> Result<()> {
    round_change_drains(false)
}

#[test]
#[ignore = "real AOEM; accepted execution ticket retired after verified durable round change"]
fn native_successor_execution_ticket_drains_after_source_round_changes() -> Result<()> {
    round_change_drains(true)
}

fn round_change_drains(execution_started: bool) -> Result<()> {
    let mut f = NativeFixture::new(2)?;
    let first = f.leader(1)?;
    let next = f.leader(2)?;
    ensure!(first != next, "fixture needs different leaders");
    f.offer_current(1)?;
    f.until(&[first, next], "real parent at next proposer", |nodes| {
        nodes[next].executed() == 1
    })?;
    let selection = f.nodes[next]
        .successor_selection()?
        .context("missing real successor basis")?;
    let child = body(selection.parent, 2)?;
    let child_id = body_id(&child)?;
    ensure!(
        f.nodes[next].offer_successor(&child, &selection, &f.base.services.0[next])?,
        "successor not admitted"
    );
    let counter = usize::from(execution_started);
    f.until(&[first, next], "accepted successor ticket", |nodes| {
        nodes[next].successor_metrics()[counter] == 1
    })?;
    let expected = if execution_started {
        [1, 1, 0, 0, 0]
    } else {
        [1, 0, 0, 0, 0]
    };
    ensure!(
        f.nodes[next].successor_metrics() == expected,
        "fixture consumed the ticket before changing source round"
    );

    // Stop assembly polling so this SAME accepted ticket remains owned. Its
    // worker may finish asynchronously; either way, the result must be drained
    // without reattaching it once its real source round becomes stale.
    let next_round = selection.source_round + 1;
    for peer in (0..4).filter(|i| *i != next).take(2) {
        let vote = novovm_consensus::round_bft::test_vectors::sign_vote(
            selection.source,
            next_round,
            Phase::Prevote,
            None,
            &f.base.set,
            &f.base.keys[peer],
        )?;
        f.nodes[next]
            .progress
            .receive(f.base.ids[peer], &Message::Vote(vote))?;
    }
    let deadline = Instant::now() + DEADLINE;
    let mut held_outbox = Vec::new();
    while f.nodes[next].progress.round() != next_round {
        ensure!(Instant::now() < deadline, "durable catch-up timeout");
        // Simulated delayed delivery, not a fabricated ACK or cleared journal.
        if let Some(message) = f.nodes[next].progress.take_message() {
            held_outbox.push(message);
        }
        f.nodes[next]
            .progress
            .poll(&f.base.services.0[next], Instant::now())?;
        std::thread::yield_now();
    }
    ensure!(
        f.nodes[next].progress.context() == selection.source
            && f.nodes[next].progress.head().is_none(),
        "round catch-up unexpectedly published a parent"
    );

    // The real pipeline has max_batches=2, and parent-independent auth reserves
    // one whole foreground batch. A second accepted background auth therefore
    // proves the retired owner released its original permit; target=false alone
    // would not establish that the accepted ticket had actually been drained.
    let Message::ApflBody { batch, .. } = child.as_ref() else {
        unreachable!()
    };
    let mut probe = Some(AuthenticationRequest::from_apfl(batch.clone(), policy())?);
    let deadline = Instant::now() + DEADLINE;
    let mut probe_ticket = loop {
        ensure!(
            Instant::now() < deadline,
            "retired successor permit not released"
        );
        f.nodes[next].poll(&f.base.services.0[next], Instant::now())?;
        match f.base.services.0[next].try_authenticate_owned(probe.take().unwrap()) {
            Ok(AuthenticationSubmission::Accepted(ticket)) => break ticket,
            Ok(AuthenticationSubmission::Backpressured(request)) => probe = Some(request),
            Err(rejected) => return Err(rejected.error),
        }
        std::thread::yield_now();
    };
    loop {
        ensure!(Instant::now() < deadline, "probe auth completion timeout");
        if let Some(authenticated) = probe_ticket.try_take()? {
            drop(authenticated);
            break;
        }
        std::thread::yield_now();
    }
    let expected = if execution_started {
        [1, 1, 0, 0, 1]
    } else {
        [1, 0, 0, 0, 1]
    };
    ensure!(
        f.nodes[next].successor_metrics() == expected
            && !f.nodes[next].has_successor_target(2)
            && !f.nodes[next].progress.retains_body(child_id)
            && f.nodes[next].progress.head().is_none(),
        "stale completion retained future authority"
    );
    for _ in 0..32 {
        f.nodes[next].poll(&f.base.services.0[next], Instant::now())?;
        ensure!(
            f.nodes[next].successor_metrics() == expected,
            "empty polls refreshed successor progress or revived stale work"
        );
    }
    let reopened = open_journal(
        ValidatorJournal::open(
            &f.base.services.0[next],
            f.base.genesis,
            f.base.anchor,
            f.base.set.clone(),
            f.base.keys[next].clone(),
        )?,
        &f.base.services.0[next],
    )?;
    ensure!(
        reopened.round() == next_round && reopened.head().is_none(),
        "source round transition was not durable"
    );
    drop(reopened);
    drop(held_outbox);
    drop(f.nodes);
    f.base.services.shutdown()
}
