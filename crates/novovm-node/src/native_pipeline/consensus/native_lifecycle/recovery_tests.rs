//! Unit qualification of archive continuation events and bounded ownership.
//! The original store/channel fixture supplies canonical Decision and Body
//! objects. Calling archive_delivery_completed below is deliberately a direct
//! unit stimulus, NOT evidence of a real transport ACK, restart or finality.
//! Actual sender completion and independent process recovery are separate gates.
use super::*;
use crate::native_pipeline::consensus::tests::progress_integration::{
    with_archive_lifecycle_for_test, with_published_archive_lifecycle_for_test,
};

fn require_fresh_published_archive_fixture(node: &NativeLifecycle) -> Result<()> {
    ensure!(node.progress.head().is_some_and(|head| head.height == 1));
    ensure!(node.pending_deliveries() == 0);
    ensure!(node.archives.iter().all(|state| {
        state.requested.is_none()
            && state.reading.is_none()
            && state.cached.is_none()
            && !state.decision_pending
            && state.body_request.is_none()
    }));
    Ok(())
}

fn poll_archives_until(
    node: &mut NativeLifecycle,
    pipeline: &CandidatePipeline,
    done: impl Fn(&NativeLifecycle) -> bool,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(node) {
        ensure!(Instant::now() < deadline, "archive poll fixture timed out");
        node.poll_archives(pipeline)?;
        std::thread::yield_now();
    }
    Ok(())
}

fn poll_archive_peer(
    node: &mut NativeLifecycle,
    pipeline: &CandidatePipeline,
    peer: usize,
) -> Result<()> {
    node.archive_turn = peer;
    node.poll_archives(pipeline)
}

#[test]
#[ignore = "real AOEM/four native nodes; shared verified archive read, not synthetic finality"]
fn native_archive_poll_coalesces_same_context_across_peers() -> Result<()> {
    with_published_archive_lifecycle_for_test(|node, pipeline, context| {
        require_fresh_published_archive_fixture(node)?;
        let head = node.progress.head();
        let started = node.archive_reads_started;
        let reused = node.archive_cache_reused;
        ensure!(node.archives.len() == 3);
        for peer in 0..node.archives.len() {
            node.request_archive(peer, context)?;
        }
        poll_archive_peer(node, pipeline, 0)?;
        poll_archive_peer(node, pipeline, 0)?;
        // The original read has been polled against its actual I/O owner. The
        // other two peers must retain requests, not start two more recoveries.
        poll_archive_peer(node, pipeline, 1)?;
        poll_archive_peer(node, pipeline, 2)?;
        eprintln!(
            "real archive starts={} reused={}",
            node.archive_reads_started - started,
            node.archive_cache_reused - reused
        );
        ensure!(node.archive_reads_started == started + 1);
        ensure!(node.archives[0].reading.is_some());
        ensure!(node.archives[1..]
            .iter()
            .all(|state| state.reading.is_none()));
        poll_archives_until(node, pipeline, |node| {
            node.archives.iter().all(|state| state.cached.is_some())
        })?;
        // One complete ArchiveRead traverses the original Head/Block/Outbox/
        // Candidate stages, including exactly one candidate recovery. No mock
        // read, fixture cache or fabricated head satisfies this assertion.
        ensure!(node.archive_reads_started == started + 1);
        ensure!(node.archive_cache_reused == reused + 2);
        let first = node.archives[0].cached.as_ref().expect("checked archive");
        let decision_id = first.decision.fragment_id();
        let body_id = first.body.fragment_id();
        for state in &node.archives {
            let cached = state.cached.as_ref().expect("checked archive");
            ensure!(cached.context == context && !cached.body_pending);
            ensure!(cached.decision.fragment_id() == decision_id);
            ensure!(cached.body.fragment_id() == body_id);
            ensure!(cached.charge() <= node.codec.message_bytes.saturating_mul(2));
        }
        // Direct completion stimulus checks the original continuation boundary;
        // it is NOT a claim that a fabricated transport ACK was observed.
        node.archive_delivery_completed(0, decision_id);
        ensure!(node.archives[0].cached.as_ref().unwrap().body_pending);
        ensure!(node.archives[1..].iter().all(|state| !state
            .cached
            .as_ref()
            .unwrap()
            .body_pending));
        node.archive_delivery_completed(1, body_id);
        ensure!(node.archives[0].cached.as_ref().unwrap().body_pending);
        ensure!(node.progress.head() == head);
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM/four native nodes; drain read after requester advance and serve remaining peer"]
fn native_archive_poll_drains_changed_requester_read_for_same_context_waiter() -> Result<()> {
    with_published_archive_lifecycle_for_test(|node, pipeline, context| {
        require_fresh_published_archive_fixture(node)?;
        let started = node.archive_reads_started;
        let reused = node.archive_cache_reused;
        node.request_archive(0, context)?;
        poll_archive_peer(node, pipeline, 0)?;
        poll_archive_peer(node, pipeline, 0)?;
        node.request_archive(1, context)?;
        poll_archive_peer(node, pipeline, 1)?;
        ensure!(node.archives[1].reading.is_none());
        let next = node.progress.context();
        ensure!(next.height > node.progress.head().unwrap().height);
        node.request_archive(0, next)?;
        ensure!(node.archives[0].reading.as_ref().unwrap().context == context);
        poll_archives_until(node, pipeline, |node| {
            node.archives[0].reading.is_none() && node.archives[1].cached.is_some()
        })?;
        ensure!(node.archives[0].requested == Some(next));
        ensure!(node.archives[0].cached.is_none());
        ensure!(node.archives[1].cached.as_ref().unwrap().context == context);
        ensure!(node.archive_reads_started == started + 1);
        ensure!(node.archive_cache_reused == reused + 1);
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM/four native nodes; immutable reuse keeps per-peer ACK and auxiliary budget"]
fn native_archive_poll_reuse_keeps_independent_pending_and_delivery_budget() -> Result<()> {
    with_published_archive_lifecycle_for_test(|node, pipeline, context| {
        require_fresh_published_archive_fixture(node)?;
        node.request_archive(0, context)?;
        poll_archives_until(node, pipeline, |node| node.archives[0].cached.is_some())?;
        let started = node.archive_reads_started;
        let reused = node.archive_cache_reused;
        let first = node.archives[0].cached.as_ref().unwrap();
        let decision = first.decision.fragment_id();
        let canonical_body = first.body_id;
        node.archive_delivery_completed(0, decision);
        ensure!(node.archives[0].cached.as_ref().unwrap().body_pending);
        // Fill the ORIGINAL auxiliary queue with unsent bounded requests.
        // Existing real consensus deliveries/handles remain untouched.
        for index in 0..=node.limits.messages_per_peer {
            let mut body_id = [0x95; 32];
            body_id[..8].copy_from_slice(&(index as u64).to_le_bytes());
            let prepared = node.channel.prepare(&Message::RequestBody { body_id })?;
            if !node.enqueue(1, prepared) {
                break;
            }
        }
        let bytes = node.bytes[1];
        let count = node.deliveries[1].len();
        node.request_archive(1, context)?;
        node.remember_body_request(1, canonical_body);
        poll_archive_peer(node, pipeline, 1)?;
        ensure!(node.archive_reads_started == started);
        ensure!(node.archive_cache_reused == reused + 1);
        ensure!(node.archives[1].decision_pending);
        ensure!(node.archives[1].body_request == Some(canonical_body));
        ensure!(!node.archives[1].cached.as_ref().unwrap().body_pending);
        ensure!(node.archives[0].cached.as_ref().unwrap().body_pending);
        ensure!(node.bytes[1] == bytes && node.deliveries[1].len() == count);
        poll_archive_peer(node, pipeline, 1)?;
        ensure!(node.archives[1].decision_pending);
        ensure!(node.archives[1].body_request == Some(canonical_body));
        ensure!(node.bytes[1] == bytes && node.deliveries[1].len() == count);
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM/four native nodes; same height alone cannot reuse a verified archive"]
fn native_archive_poll_reuse_requires_full_context_and_published_height() -> Result<()> {
    with_published_archive_lifecycle_for_test(|node, pipeline, context| {
        require_fresh_published_archive_fixture(node)?;
        node.request_archive(0, context)?;
        poll_archives_until(node, pipeline, |node| node.archives[0].cached.is_some())?;
        let started = node.archive_reads_started;
        let reused = node.archive_cache_reused;
        let rejected = node.rejected_messages;
        let wrong = VoteContext {
            genesis_config_commitment: [0xb7; 32],
            ..context
        };
        node.request_archive(1, wrong)?;
        poll_archive_peer(node, pipeline, 1)?;
        ensure!(node.archives[1].cached.is_none());
        ensure!(node.archive_reads_started == started + 1);
        poll_archives_until(node, pipeline, |node| node.archives[1].requested.is_none())?;
        ensure!(node.archives[1].cached.is_none());
        ensure!(node.rejected_messages == rejected + 1);
        let next = node.progress.context();
        node.request_archive(2, next)?;
        poll_archive_peer(node, pipeline, 2)?;
        ensure!(node.archives[2].reading.is_none() && node.archives[2].cached.is_none());
        node.request_archive(
            2,
            VoteContext {
                epoch: next.epoch + 1,
                ..next
            },
        )?;
        ensure!(node.archives[2].requested == Some(next));
        ensure!(node.rejected_messages == rejected + 2);
        ensure!(node.archive_cache_reused == reused);
        ensure!(node.archive_reads_started == started + 1);
        Ok(())
    })
}

fn install_cache(
    node: &mut NativeLifecycle,
    decision: &Message,
    body: &Message,
) -> Result<(VoteContext, Hash, Hash)> {
    let Message::Decision {
        proposal, body_id, ..
    } = decision
    else {
        anyhow::bail!("archive fixture did not supply a Decision")
    };
    ensure!(
        transport::candidate_body_id(body, node.codec)? == *body_id,
        "archive fixture Decision/Body identity mismatch"
    );
    let context = proposal.context;
    let body = node.channel.prepare_recovery_reply(body)?;
    let decision = node.channel.prepare_recovery_reply(decision)?;
    let ids = (context, decision.fragment_id(), body.fragment_id());
    let cached = Cached {
        context,
        body_id: *body_id,
        body,
        decision,
        body_pending: false,
    };
    ensure!(cached.charge() <= node.codec.message_bytes.saturating_mul(2));
    node.request_archive(0, context)?;
    node.archives[0].cached = Some(cached);
    Ok(ids)
}

fn pending(node: &NativeLifecycle) -> bool {
    node.archives[0]
        .cached
        .as_ref()
        .is_some_and(|cached| cached.body_pending)
}

#[test]
#[ignore = "real AOEM fixture; unit qualification, not a fabricated network ACK"]
fn native_archive_continuation_requires_exact_current_decision() -> Result<()> {
    with_archive_lifecycle_for_test(|node, decision, body| {
        let (context, decision_id, _) = install_cache(node, &decision, &body)?;
        node.archive_delivery_completed(0, [0xd3; 32]);
        ensure!(!pending(node), "unknown object armed archive body");
        node.archive_delivery_completed(1, decision_id);
        ensure!(
            !pending(node),
            "another peer's completion armed archive body"
        );
        let mut other = context;
        other.height += 1;
        node.archives[0].requested = Some(other);
        node.archive_delivery_completed(0, decision_id);
        ensure!(!pending(node), "old context completion armed archive body");
        node.archives[0].requested = Some(context);
        node.archive_delivery_completed(0, decision_id);
        ensure!(pending(node), "matching current decision did not arm body");
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM fixture; bounded archive continuation ownership"]
fn native_archive_continuation_coalesces_before_queue_handoff() -> Result<()> {
    with_archive_lifecycle_for_test(|node, decision, body| {
        let (_, decision_id, body_fragment) = install_cache(node, &decision, &body)?;
        node.archive_delivery_completed(0, decision_id);
        node.archive_delivery_completed(0, decision_id);
        ensure!(pending(node));
        node.poll_archive_body(0);
        ensure!(!pending(node));
        ensure!(node.deliveries[0].len() == 1 && node.served_bodies == 1);
        let queued = &node.deliveries[0][0];
        ensure!(queued.prepared.fragment_id() == body_fragment);
        ensure!(queued.archive && !queued.realtime && !queued.admitted);
        let bytes = node.bytes[0];
        node.poll_archive_body(0);
        ensure!(node.deliveries[0].len() == 1 && node.bytes[0] == bytes);
        ensure!(node.served_bodies == 1, "unarmed continuation resent body");
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM fixture; original auxiliary count and byte budgets"]
fn native_archive_continuation_retains_responsibility_when_auxiliary_full() -> Result<()> {
    with_archive_lifecycle_for_test(|node, decision, body| {
        let (_, decision_id, body_fragment) = install_cache(node, &decision, &body)?;
        node.limits.messages_per_peer = node.control_reserve_messages + 4;
        for index in 0u64..4 {
            let mut body_id = [0x91; 32];
            body_id[..8].copy_from_slice(&index.to_le_bytes());
            let prepared = node.channel.prepare(&Message::RequestBody { body_id })?;
            ensure!(node.enqueue(0, prepared));
        }
        let bytes = node.bytes[0];
        node.archive_delivery_completed(0, decision_id);
        node.poll_archive_body(0);
        ensure!(
            pending(node),
            "backpressure discarded archive body responsibility"
        );
        ensure!(node.deliveries[0].len() == 4 && node.bytes[0] == bytes);
        ensure!(node.served_bodies == 0);
        // Retire only UNSENT realtime requests through the original owner.
        // This changes delivery scheduling height, not the consensus journal.
        node.height += 1;
        node.retire_realtime()?;
        ensure!(node.deliveries[0].is_empty() && node.bytes[0] == 0);
        ensure!(pending(node));
        let byte_limit = node.limits.bytes_per_peer;
        let body_bytes = node.archives[0]
            .cached
            .as_ref()
            .expect("cache")
            .body
            .byte_len();
        node.limits.bytes_per_peer = node.control_reserve_bytes + body_bytes - 1;
        node.poll_archive_body(0);
        ensure!(pending(node) && node.deliveries[0].is_empty());
        ensure!(node.bytes[0] == 0 && node.served_bodies == 0);
        node.limits.bytes_per_peer = byte_limit;
        node.poll_archive_body(0);
        ensure!(!pending(node) && node.deliveries[0].len() == 1);
        ensure!(node.deliveries[0][0].prepared.fragment_id() == body_fragment);
        ensure!(node.served_bodies == 1);
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM fixture; RequestBody hints cannot overwrite archive obligation"]
fn native_archive_continuation_survives_competing_body_hint() -> Result<()> {
    with_archive_lifecycle_for_test(|node, decision, body| {
        let (_, decision_id, body_fragment) = install_cache(node, &decision, &body)?;
        let canonical = node.archives[0].cached.as_ref().expect("cache").body_id;
        node.archive_delivery_completed(0, decision_id);
        node.remember_body_request(0, canonical);
        let unrelated = [0xe7; 32];
        node.remember_body_request(0, unrelated);
        node.poll_body_response(0)?;
        ensure!(pending(node));
        ensure!(node.archives[0].body_request == Some(unrelated));
        node.poll_archive_body(0);
        ensure!(!pending(node) && node.deliveries[0].len() == 1);
        ensure!(node.deliveries[0][0].prepared.fragment_id() == body_fragment);
        ensure!(node.archives[0].body_request == Some(unrelated));
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM fixture; completion history must not suppress a new continuation"]
fn native_archive_continuation_rearms_after_previous_body_completion() -> Result<()> {
    with_archive_lifecycle_for_test(|node, decision, body| {
        let (_, decision_id, body_fragment) = install_cache(node, &decision, &body)?;
        node.archive_delivery_completed(0, decision_id);
        ensure!(pending(node));
        node.archive_delivery_completed(0, body_fragment);
        ensure!(!pending(node));
        node.archive_delivery_completed(0, [0xf1; 32]);
        ensure!(!pending(node));
        // A later qualified decision completion is a fresh opportunity. A
        // permanent body_delivered bit would incorrectly survive peer restart.
        // This tests that rule only; it does not synthesize a peer restart.
        node.archive_delivery_completed(0, decision_id);
        ensure!(
            pending(node),
            "previous body completion permanently suppressed reply"
        );
        node.poll_archive_body(0);
        ensure!(!pending(node) && node.deliveries[0].len() == 1);
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM fixture; one current archive cache and no stale continuation"]
fn native_archive_continuation_context_replacement_retires_old_cache() -> Result<()> {
    with_archive_lifecycle_for_test(|node, decision, body| {
        let (mut next, decision_id, _) = install_cache(node, &decision, &body)?;
        ensure!(node.archives[0].charge() > 0);
        node.archive_delivery_completed(0, decision_id);
        node.poll_archive_body(0);
        ensure!(node.deliveries[0].len() == 1);
        next.height += 1;
        next.parent_block_hash = [0x71; 32];
        next.parent_decision_hash = [0x72; 32];
        node.request_archive(0, next)?;
        ensure!(node.archives[0].requested == Some(next));
        ensure!(node.archives[0].cached.is_none() && node.archives[0].charge() == 0);
        ensure!(node.deliveries[0].is_empty() && node.bytes[0] == 0);
        node.archive_delivery_completed(0, decision_id);
        node.poll_archive_body(0);
        ensure!(!pending(node) && node.deliveries[0].is_empty());
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM fixture; reordered archive requests retain current ownership"]
fn native_archive_request_older_completion_cannot_replace_newer_responsibility() -> Result<()> {
    with_archive_lifecycle_for_test(|node, decision, body| {
        let (old, _, _) = install_cache(node, &decision, &body)?;
        let next = VoteContext {
            height: old.height + 1,
            parent_block_hash: [0x71; 32],
            parent_decision_hash: [0x72; 32],
            ..old
        };
        node.request_archive(0, next)?;
        ensure!(node.archives[0].cached.is_none());

        // A codec-valid body for the newer request is a delivery fixture only,
        // not an executed/authorized candidate or a manufactured final block.
        let mut next_body = body.as_ref().clone();
        let context = match &mut next_body {
            Message::Body { context, .. }
            | Message::ApflBody { context, .. }
            | Message::AssetBody { context, .. } => context,
            _ => anyhow::bail!("missing body fixture"),
        };
        context.parent_height = old.height;
        context.height = next.height;
        context.parent_block_hash = next.parent_block_hash;
        let prepared = node.channel.prepare_recovery_reply(&next_body)?;
        let fragment = prepared.fragment_id();
        ensure!(node.enqueue_kind(0, prepared, true));
        let bytes = node.bytes[0];
        let retired = node.retired_sends;

        // Simulate application delivery of an OLD request after the new one.
        // No transport completion or peer restart is fabricated by this test.
        node.request_archive(0, old)?;
        ensure!(
            node.archives[0].requested == Some(next),
            "late old request replaced current retained archive responsibility"
        );
        ensure!(node.deliveries[0].len() == 1 && node.bytes[0] == bytes);
        ensure!(node.deliveries[0][0].prepared.fragment_id() == fragment);
        ensure!(node.deliveries[0][0].archive && !node.deliveries[0][0].retiring);
        ensure!(node.retired_sends == retired);
        node.request_archive(0, next)?;
        ensure!(node.archives[0].requested == Some(next));
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM fixture; archive request ordering grants no parent authority"]
fn native_archive_request_same_height_parent_and_invalid_context_keep_original_checks() -> Result<()>
{
    with_archive_lifecycle_for_test(|node, decision, body| {
        let (old, _, _) = install_cache(node, &decision, &body)?;
        let next = VoteContext {
            height: old.height + 1,
            parent_block_hash: [0x71; 32],
            parent_decision_hash: [0x72; 32],
            ..old
        };
        node.request_archive(0, next)?;
        let other_parent = VoteContext {
            parent_block_hash: [0x73; 32],
            parent_decision_hash: [0x74; 32],
            ..next
        };
        node.request_archive(0, other_parent)?;
        // It remains only an untrusted request. Exact stored archive matching
        // is still necessary; no cached evidence or published state appears.
        ensure!(node.archives[0].requested == Some(other_parent));
        ensure!(node.archives[0].cached.is_none());
        let rejected = node.rejected_messages;
        node.request_archive(
            0,
            VoteContext {
                chain_id: next.chain_id + 1,
                ..next
            },
        )?;
        ensure!(node.rejected_messages == rejected + 1);
        ensure!(node.archives[0].requested == Some(other_parent));
        ensure!(node.progress.context() == old);
        Ok(())
    })
}
