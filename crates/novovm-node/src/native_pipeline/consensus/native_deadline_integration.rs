//! Deadline ordering regression, not a performance measurement. Real AOEM
//! execution and durable leader/follower journals; only the already-received
//! control-slot timing and the local monotonic deadline are fixture-controlled.
use super::*;
use crate::native_pipeline::consensus::{
    bodies::{BodyLimits, CandidateBodies},
    native_lifecycle::{DeliveryLimits, NativeLifecycle},
};

#[test]
#[ignore = "real AOEM; native ready proposal must precede same-poll timeout"]
fn native_lifecycle_ready_proposal_wins_deadline_without_disabling_timeout() -> Result<()> {
    deadline_ready_proposal(false)
}

#[test]
#[ignore = "real AOEM; queued socket proposal behind rejected control must precede timeout"]
fn native_lifecycle_second_ready_control_wins_deadline_without_disabling_timeout() -> Result<()> {
    deadline_ready_proposal(true)
}

fn deadline_ready_proposal(backlog: bool) -> Result<()> {
    let Fixture {
        ids,
        set,
        mut mesh,
        services,
        anchor,
        mut drivers,
        ..
    } = fixture(ExecutionBackend::CpuCallbacks)?;
    let leader = ids
        .iter()
        .position(|id| *id == set.leader(1, 0).unwrap())
        .context("fixture leader missing")?;
    let ready = (leader + 1) % 4;
    let absent = (leader + 2) % 4;
    let context = batch_context(anchor.state_root);
    let body_id = transport::body_id(&context, &raw()?, codec())?;
    for (i, driver) in drivers.iter_mut().enumerate() {
        let mut ticket = admit(&services.0[i], anchor.state_root)?;
        let deadline = Instant::now() + DEADLINE;
        let candidate = loop {
            ensure!(
                Instant::now() < deadline,
                "deadline fixture execution timeout"
            );
            if let Some(done) = ticket.try_take()? {
                break done.candidate().clone();
            }
            std::thread::yield_now();
        };
        ensure!(
            driver.register_candidate(body_id, &candidate, i == leader)?,
            "deadline fixture candidate budget"
        );
    }
    // Obtain an actual persisted proposal, never fixture-sign an honest vote.
    let start = Instant::now();
    let deadline = Instant::now() + DEADLINE;
    let proposal = loop {
        ensure!(Instant::now() < deadline, "durable leader proposal timeout");
        drivers[leader].poll(&services.0[leader], start)?;
        if let Some(message) = drivers[leader].take_message() {
            ensure!(
                matches!(message, Message::Proposal { .. }),
                "expected proposal"
            );
            break message;
        }
        std::thread::yield_now();
    };
    let value = match &proposal {
        Message::Proposal { proposal, .. } => proposal.value,
        _ => unreachable!(),
    };
    for i in [ready, absent] {
        drivers[i].poll(&services.0[i], start)?;
        ensure!(
            drivers[i].step() == TimeoutStep::Propose,
            "premature timeout"
        );
    }
    if backlog {
        // The SECOND message uses the original socket/reassembly owner, not
        // another fixture queue. Leave it unconsumed until the deadline poll.
        let prepared = mesh.channels[leader].prepare(&proposal)?;
        ensure!(
            prepared.byte_len() <= 1024,
            "fixture proposal must be one fragment"
        );
        let peer = (0..ids.len())
            .filter(|i| *i != leader)
            .position(|i| i == ready)
            .context("fixture receiver missing")?;
        ensure!(
            mesh.channels[leader].try_send(peer, &prepared, Instant::now())?,
            "fixture socket send backpressure"
        );
        let deadline = Instant::now() + DEADLINE;
        let mut received = false;
        while !received {
            ensure!(Instant::now() < deadline, "socket proposal arrival timeout");
            for (i, channel) in mesh.channels.iter_mut().enumerate() {
                let report = channel.poll_io(
                    2000 + start.elapsed().as_millis() as u64,
                    Instant::now(),
                    64,
                )?;
                ensure!(
                    report.rejected == 0 && report.backpressured == 0,
                    "fixture socket rejection"
                );
                if i == ready && report.accepted_fragments > 0 {
                    received = true;
                }
            }
            std::thread::yield_now();
        }
    }
    let mut nodes = Vec::new();
    for (i, (driver, channel)) in drivers
        .into_iter()
        .zip(std::mem::take(&mut mesh.channels))
        .enumerate()
    {
        let bodies = CandidateBodies::new(
            &services.0[i],
            context,
            policy(),
            codec(),
            BodyLimits {
                bodies: 8,
                bytes: 256 * 1024,
                inflight: 2,
            },
        )?;
        nodes.push(NativeLifecycle::new(
            driver,
            bodies,
            channel,
            &set,
            DeliveryLimits {
                messages_per_peer: 32,
                bytes_per_peer: 512 * 1024,
            },
            policy(),
            codec(),
        )?);
    }
    // This is the existing bounded retained-control slot, after transport
    // custody but before consensus admission. Production still verifies it.
    let mut first = proposal;
    if backlog {
        let Message::Proposal { proposal, .. } = &mut first else {
            unreachable!()
        };
        proposal.signature[0] ^= 1;
    }
    nodes[ready].retain_control_for_deadline_test(ids[leader], first)?;
    let expired = start + Duration::from_secs(5);
    for (i, expected) in [(ready, Some(value)), (absent, None)] {
        nodes[i].poll(&services.0[i], expired)?;
        if i == ready && backlog {
            ensure!(
                nodes[i].rejected_messages == 1,
                "bad first signature was not rejected"
            );
        }
        // Poll the same journal to obtain its real ACK without letting the
        // lifecycle fan out/take the message before this assertion sees it.
        let deadline = Instant::now() + DEADLINE;
        let vote = loop {
            ensure!(Instant::now() < deadline, "deadline vote ACK timeout");
            nodes[i].progress.poll(&services.0[i], expired)?;
            if let Some(message) = nodes[i].progress.take_message() {
                let Message::Vote(vote) = message else {
                    bail!("expected first durable prevote")
                };
                break vote;
            }
            std::thread::yield_now();
        };
        vote.verify(&set)?;
        ensure!(
            vote.round == 0 && vote.phase == Phase::Prevote && vote.value == expected,
            "ready proposal lost to same-poll deadline or absent-input timeout disabled: node={i}, actual={:?}, expected={expected:?}",
            vote.value
        );
        ensure!(
            nodes[i].progress.head().is_none(),
            "single vote granted finality"
        );
    }
    Ok(())
}
