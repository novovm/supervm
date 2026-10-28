//! Deterministic delivery-purpose ownership, using original stores and native
//! handles. Scheduling height changes below exercise retirement only; they do
//! not advance consensus, publish a block or fabricate transport completion.
use super::*;
use crate::native_pipeline::consensus::tests::progress_integration::{
    with_archive_lifecycle_for_test, with_delivery_lifecycle_for_test,
    with_delivery_live_body_for_test, with_delivery_peer_lifecycle_for_test,
};
use crate::native_pipeline::consensus::transport;

fn prepared(node: &NativeLifecycle, message: &Message) -> Result<PreparedMessage> {
    node.channel.prepare(message)
}

fn request(node: &NativeLifecycle, index: u64) -> Result<PreparedMessage> {
    let mut body_id = [0x91; 32];
    body_id[..8].copy_from_slice(&index.to_le_bytes());
    node.channel.prepare(&Message::RequestBody { body_id })
}

#[test]
fn native_control_reserve_tracks_signed_evidence_not_expanded_body_capacity() -> Result<()> {
    let small = DecodeLimits {
        transactions: 1024,
        transaction_bytes: 8192,
        body_bytes: 384 * 1024,
        message_bytes: 512 * 1024,
    };
    let large = DecodeLimits {
        body_bytes: 8 * 1024 * 1024,
        message_bytes: 8 * 1024 * 1024 + 128 * 1024,
        ..small
    };
    let send = NativeSendLimits {
        messages: 32,
        bytes: 16 * 1024 * 1024,
        peer_messages: 4,
        peer_bytes: 2 * 1024 * 1024,
    };
    let control = transport::durable_control_bound(4, large)?;
    let reserve = control_reservation(4, large, &send)?;
    assert_eq!(reserve, (8, 8 * control));
    assert_eq!(reserve, control_reservation(4, small, &send)?);
    assert!(reserve.1 < small.message_bytes);
    assert!(control_reservation(0, large, &send).is_err());
    Ok(())
}

#[test]
fn native_control_reserve_keeps_current_controls_and_exact_bounded_drain() -> Result<()> {
    let codec = DecodeLimits {
        transactions: 1024,
        transaction_bytes: 8192,
        body_bytes: 8 * 1024 * 1024,
        message_bytes: 8 * 1024 * 1024 + 128 * 1024,
    };
    let control = transport::durable_control_bound(4, codec)?;
    for (messages, peer_messages, bytes, peer_bytes, slots, drain) in [
        (32, 4, 16 * control, 8 * control, 4, 4 * control),
        (2, 4, 16 * control, 8 * control, 2, 2 * control),
        (32, 4, 16 * control, control + 1, 4, control + 1),
        (32, 4, control + 1, 16 * control, 4, control + 1),
    ] {
        let send = NativeSendLimits {
            messages,
            bytes,
            peer_messages,
            peer_bytes,
        };
        assert_eq!(
            control_reservation(4, codec, &send)?,
            (4 + slots, 4 * control + drain)
        );
    }
    Ok(())
}

#[test]
#[ignore = "real AOEM; original delivery ownership and retirement"]
fn native_archive_delivery_unsent_takeover_survives_realtime_retirement() -> Result<()> {
    with_delivery_lifecycle_for_test(|node, message| {
        let wire = prepared(node, &message)?;
        ensure!(node.enqueue(0, wire.clone()));
        let bytes = node.bytes[0];
        ensure!(node.enqueue_kind(0, wire.clone(), true));
        ensure!(node.deliveries[0].len() == 1 && node.bytes[0] == bytes);
        node.height += 1;
        node.retire_realtime()?;
        ensure!(
            node.deliveries[0].len() == 1 && node.bytes[0] == bytes,
            "deduplicated archive responsibility was retired with unsent realtime bytes"
        );
        ensure!(node.deliveries[0][0].archive && !node.deliveries[0][0].retiring);
        ensure!(node.deliveries[0][0].prepared.fragment_id() == wire.fragment_id());
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM and native admitted handle; no fabricated ACK"]
fn native_archive_delivery_inflight_takeover_retains_original_handle() -> Result<()> {
    with_delivery_lifecycle_for_test(|node, message| {
        let wire = prepared(node, &message)?;
        ensure!(node.enqueue(0, wire.clone()));
        node.poll_io(1000, Instant::now())?;
        ensure!(node.deliveries[0][0].admitted, "no real native send handle");
        let bytes = node.bytes[0];
        ensure!(node.enqueue_kind(0, wire, true));
        node.height += 1;
        node.retire_realtime()?;
        ensure!(
            node.deliveries[0].len() == 1
                && node.deliveries[0][0].admitted
                && !node.deliveries[0][0].retiring
                && node.bytes[0] == bytes,
            "archive takeover cancelled or released the original in-flight handle"
        );
        Ok(())
    })
}

#[test]
#[ignore = "real native bounded retirement drain; no resurrection of old handle"]
fn native_archive_delivery_retiring_object_refuses_takeover_until_returned() -> Result<()> {
    with_delivery_lifecycle_for_test(|node, message| {
        let wire = prepared(node, &message)?;
        ensure!(node.enqueue(0, wire.clone()));
        node.poll_io(1000, Instant::now())?;
        ensure!(node.deliveries[0][0].admitted);
        node.height += 1;
        node.retire_realtime()?;
        ensure!(node.deliveries[0][0].retiring);
        let bytes = node.bytes[0];
        ensure!(
            !node.enqueue_kind(0, wire.clone(), true),
            "retiring object falsely accepted archive responsibility"
        );
        ensure!(node.bytes[0] == bytes && node.deliveries[0][0].retiring);
        // Existing archive callers clear their pending marker only on true.
        // The receiver is deliberately not pumped. Advance the fixture clock
        // past the ORIGINAL sender's 30-second deadline, without extending or
        // replacing it; only the real returned completion releases ownership.
        node.poll_io(32_000, Instant::now() + Duration::from_secs(31))?;
        ensure!(
            node.deliveries[0].is_empty(),
            "original handle did not return"
        );
        ensure!(node.bytes[0] == 0);
        ensure!(node.enqueue_kind(0, wire, true));
        ensure!(node.deliveries[0].len() == 1 && !node.deliveries[0][0].admitted);
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM; reclassification preserves reserved control capacity"]
fn native_archive_delivery_promotion_respects_auxiliary_message_budget() -> Result<()> {
    with_delivery_lifecycle_for_test(|node, message| {
        node.limits.messages_per_peer = node.control_reserve_messages + 4;
        let wire = prepared(node, &message)?;
        ensure!(node.enqueue(0, wire.clone()));
        for i in 0..4 {
            ensure!(node.enqueue(0, request(node, i)?));
        }
        let bytes = node.bytes[0];
        ensure!(
            !node.enqueue_kind(0, wire, true),
            "archive promotion bypassed the full auxiliary message budget"
        );
        ensure!(node.deliveries[0].len() == 5 && node.bytes[0] == bytes);
        ensure!(
            !node.deliveries[0][0].archive,
            "failed takeover mutated purpose"
        );
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM; reclassification preserves reserved control byte budget"]
fn native_archive_delivery_promotion_respects_auxiliary_byte_budget() -> Result<()> {
    with_delivery_lifecycle_for_test(|node, message| {
        node.limits.messages_per_peer = 4096;
        node.limits.bytes_per_peer = node.control_reserve_bytes + node.codec.message_bytes;
        let wire = prepared(node, &message)?;
        ensure!(node.enqueue(0, wire.clone()));
        let auxiliary_limit = node.codec.message_bytes;
        let mut auxiliary_bytes = 0;
        let mut index = 0;
        while auxiliary_limit - auxiliary_bytes >= wire.byte_len() {
            let query = request(node, index)?;
            ensure!(query.byte_len() <= auxiliary_limit - auxiliary_bytes);
            auxiliary_bytes += query.byte_len();
            ensure!(node.enqueue(0, query));
            index += 1;
        }
        let count = node.deliveries[0].len();
        let bytes = node.bytes[0];
        ensure!(
            !node.enqueue_kind(0, wire, true),
            "archive promotion bypassed the full auxiliary byte budget"
        );
        ensure!(node.deliveries[0].len() == count && node.bytes[0] == bytes);
        ensure!(!node.deliveries[0][0].archive);
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM; realtime duplicate cannot erase archive or double-charge"]
fn native_archive_delivery_realtime_duplicate_preserves_archive_and_bytes() -> Result<()> {
    with_delivery_lifecycle_for_test(|node, message| {
        let wire = prepared(node, &message)?;
        ensure!(node.enqueue_kind(0, wire.clone(), true));
        let bytes = node.bytes[0];
        ensure!(node.enqueue(0, wire));
        ensure!(node.deliveries[0].len() == 1 && node.bytes[0] == bytes);
        node.height += 1;
        node.retire_realtime()?;
        ensure!(node.deliveries[0].len() == 1 && node.deliveries[0][0].archive);
        ensure!(node.bytes[0] == bytes);
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM; retiring archive purpose preserves current realtime obligation"]
fn native_archive_delivery_new_request_preserves_current_realtime_obligation() -> Result<()> {
    with_delivery_lifecycle_for_test(|node, message| {
        let wire = prepared(node, &message)?;
        let context = node.progress.context();
        node.request_archive(0, context)?;
        ensure!(node.enqueue_kind(0, wire.clone(), true));
        ensure!(node.enqueue(0, wire));
        let bytes = node.bytes[0];
        let mut next = context;
        next.height += 1;
        next.parent_block_hash = [0x64; 32];
        next.parent_decision_hash = [0x65; 32];
        node.request_archive(0, next)?;
        ensure!(
            node.deliveries[0].len() == 1 && node.bytes[0] == bytes,
            "archive request change erased still-current realtime responsibility"
        );
        ensure!(!node.deliveries[0][0].archive && !node.deliveries[0][0].retiring);
        node.height += 1;
        node.retire_realtime()?;
        ensure!(node.deliveries[0].is_empty() && node.bytes[0] == 0);
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM; requested live body retains its response obligation"]
fn native_archive_delivery_requested_live_body_survives_advance_and_first_archive_request(
) -> Result<()> {
    with_delivery_live_body_for_test(|node, pipeline, body| {
        ensure!(node.offer(&body)?);
        let body_id = super::super::transport::candidate_body_id(&body, node.codec)?;
        let wire = node.channel.prepare(&body)?;
        let reply = node.channel.prepare_recovery_reply(&body)?;
        ensure!(wire.fragment_id() != reply.fragment_id());
        let context = node.progress.context();
        ensure!(node.enqueue(0, wire.clone()));
        let bytes = node.bytes[0];
        node.remember_body_request(0, body_id);
        node.archive_turn = 0;
        node.poll_archives(pipeline)?;
        ensure!(
            node.served_bodies == 1,
            "live response was not served by original owner"
        );
        ensure!(node.deliveries[0].len() == 2 && node.bytes[0] == bytes + reply.byte_len());
        ensure!(
            node.deliveries[0][1].archive,
            "requested live reply lost response purpose"
        );
        node.height += 1;
        node.retire_realtime()?;
        ensure!(
            node.deliveries[0].len() == 1,
            "local advance erased requested live body"
        );
        node.request_archive(0, context)?;
        ensure!(
            node.deliveries[0].len() == 1
                && node.deliveries[0][0].archive
                && !node.deliveries[0][0].retiring
                && node.deliveries[0][0].prepared.fragment_id() == reply.fragment_id()
                && node.bytes[0] == reply.byte_len(),
            "first same-height RequestDecision cancelled requested live-body reply"
        );
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM; same-height archive context refresh preserves reply"]
fn native_archive_delivery_same_height_context_refresh_preserves_archive_only_reply() -> Result<()>
{
    with_delivery_live_body_for_test(|node, _, body| {
        let mut context = node.progress.context();
        context.height = 2;
        context.parent_block_hash = [0x74; 32];
        context.parent_decision_hash = [0x75; 32];
        let Message::ApflBody {
            context: execution,
            batch,
        } = body.as_ref()
        else {
            anyhow::bail!("expected original APFL fixture body");
        };
        let mut execution = *execution;
        execution.height = 2;
        execution.parent_height = 1;
        execution.parent_block_hash = context.parent_block_hash;
        let body = Message::ApflBody {
            context: execution,
            batch: batch.clone(),
        };
        let wire = node.channel.prepare(&body)?;
        node.request_archive(0, context)?;
        ensure!(node.enqueue_kind(0, wire.clone(), true));
        let bytes = node.bytes[0];
        context.parent_block_hash = [0x76; 32];
        context.parent_decision_hash = [0x77; 32];
        node.request_archive(0, context)?;
        ensure!(
            node.deliveries[0].len() == 1
                && node.deliveries[0][0].archive
                && !node.deliveries[0][0].retiring
                && node.deliveries[0][0].prepared.fragment_id() == wire.fragment_id()
                && node.bytes[0] == bytes,
            "same-height context refresh erased retained archive-only reply"
        );
        Ok(())
    })
}

#[test]
#[ignore = "real native handle; auxiliary charge persists until bounded drain"]
fn native_archive_delivery_cancel_drain_keeps_auxiliary_budget_charge() -> Result<()> {
    with_archive_lifecycle_for_test(|node, decision, _| {
        node.limits.messages_per_peer = node.control_reserve_messages + 4;
        let context = node.progress.context();
        node.request_archive(0, context)?;
        let wire = prepared(node, &decision)?;
        ensure!(node.enqueue_kind(0, wire.clone(), true));
        node.poll_io(1000, Instant::now())?;
        ensure!(
            node.deliveries[0][0].admitted,
            "missing actual native handle"
        );
        for i in 0..3 {
            ensure!(node.enqueue(0, request(node, i)?));
        }
        let bytes = node.bytes[0];
        let mut next = context;
        next.height += 1;
        next.parent_block_hash = [0x84; 32];
        next.parent_decision_hash = [0x85; 32];
        node.request_archive(0, next)?;
        ensure!(
            node.deliveries[0][0].retiring && !node.deliveries[0][0].archive,
            "archive-only obligation did not enter original retirement drain"
        );
        let extra = request(node, 100)?;
        ensure!(
            !node.enqueue(0, extra.clone()),
            "clearing archive purpose prematurely released auxiliary capacity"
        );
        ensure!(node.bytes[0] == bytes && node.deliveries[0].len() == 4);
        node.poll_io(32_000, Instant::now() + Duration::from_secs(31))?;
        ensure!(
            !node.deliveries[0].iter().any(|delivery| delivery.retiring),
            "actual retired handle failed to return at its original deadline"
        );
        ensure!(node.bytes[0] == bytes - wire.byte_len());
        ensure!(node.enqueue(0, extra));
        ensure!(node.deliveries[0].len() == 4);
        Ok(())
    })
}

#[test]
#[ignore = "real native sender; retirement must not strand remote partial objects"]
fn native_realtime_retirement_admitted_handle_keeps_original_deadline() -> Result<()> {
    with_delivery_lifecycle_for_test(|node, message| {
        let wire = prepared(node, &message)?;
        ensure!(
            wire.byte_len() > novovm_network::novorudp::fragments::UDP_CHUNK_BYTES,
            "ML-DSA proposal fixture must require multiple frames"
        );
        let start = Instant::now();
        ensure!(node.enqueue(0, wire.clone()));
        node.poll_io(1000, start)?;
        ensure!(node.deliveries[0][0].admitted);
        // Send through the actual native owner. The remote is intentionally not
        // pumped, so no real DONE can arrive before the original sender TTL.
        node.poll_io(1001, start + Duration::from_millis(1))?;
        let bytes = node.bytes[0];
        node.height += 1;
        node.retire_realtime()?;
        ensure!(node.deliveries[0][0].retiring && node.deliveries[0][0].admitted);
        node.poll_io(1002, start + Duration::from_millis(2))?;
        ensure!(
            node.deliveries[0].len() == 1
                && node.deliveries[0][0].admitted
                && node.deliveries[0][0].retiring
                && node.deliveries[0][0].prepared.fragment_id() == wire.fragment_id()
                && node.bytes[0] == bytes,
            "retirement locally cancelled the in-flight sender before DONE/its original deadline"
        );
        ensure!(
            !node.enqueue(0, wire.clone()),
            "retirement resurrected realtime work"
        );
        ensure!(
            !node.enqueue_kind(0, wire, true),
            "retirement falsely accepted archive work"
        );
        node.poll_io(30_999, start + Duration::from_millis(29_999))?;
        ensure!(node.deliveries[0].len() == 1 && node.bytes[0] == bytes);
        node.poll_io(31_000, start + Duration::from_secs(30))?;
        ensure!(node.deliveries[0].is_empty() && node.bytes[0] == 0);
        node.poll_io(32_000, start + Duration::from_secs(31))?;
        ensure!(
            node.deliveries[0].is_empty() && node.bytes[0] == 0,
            "retired expired sender was retried beyond its original deadline"
        );
        Ok(())
    })
}

#[test]
#[ignore = "real lifecycle fixture; unsent superseded work releases immediately"]
fn native_realtime_retirement_unadmitted_work_releases_immediately() -> Result<()> {
    with_delivery_lifecycle_for_test(|node, message| {
        let wire = prepared(node, &message)?;
        ensure!(node.enqueue(0, wire));
        ensure!(!node.deliveries[0][0].admitted && node.bytes[0] > 0);
        node.height += 1;
        node.retire_realtime()?;
        ensure!(node.deliveries[0].is_empty() && node.bytes[0] == 0);
        Ok(())
    })
}

#[test]
#[ignore = "real UDP partial ML-DSA object; original send must drain instead of orphaning"]
fn native_realtime_retirement_partial_remote_object_drains_to_real_done() -> Result<()> {
    with_delivery_peer_lifecycle_for_test(|node, remote, message| {
        let wire = prepared(node, &message)?;
        ensure!(
            wire.byte_len() > novovm_network::novorudp::fragments::UDP_CHUNK_BYTES,
            "fixture must span more than one UDP frame"
        );
        let start = Instant::now();
        ensure!(node.enqueue(0, wire.clone()));
        node.poll_io(1000, start)?;
        ensure!(node.deliveries[0][0].admitted);
        // Exactly one ORIGINAL frame reaches the peer before local retirement.
        // The ordinary sender continues to own all remaining frames and TTL.
        let sent = node.channel.poll_io(1001, start, 1)?;
        ensure!(sent.data_sent == 1 && sent.completed.is_empty());
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            ensure!(Instant::now() < deadline, "first fragment did not arrive");
            let report = remote.channel.poll_io(1001, Instant::now(), 1)?;
            ensure!(report.rejected == 0 && report.backpressured == 0);
            ensure!(report.accepted_fragments <= 1);
            if report.accepted_fragments == 1 {
                break;
            }
            std::thread::yield_now();
        }
        ensure!(
            remote
                .channel
                .poll_receive_lane(0, Instant::now(), 64)?
                .is_none(),
            "one fragment unexpectedly completed the multi-frame fixture"
        );
        node.height += 1;
        node.retire_realtime()?;
        let mut received = false;
        while !received || !node.deliveries[0].is_empty() {
            ensure!(
                Instant::now() < deadline,
                "retirement orphaned the remote partial object instead of completing its original bounded send"
            );
            node.poll_io(1002 + start.elapsed().as_millis() as u64, Instant::now())?;
            let report = remote.channel.poll_io(1002, Instant::now(), 64)?;
            ensure!(report.rejected == 0 && report.backpressured == 0);
            if let Some(event) = remote.channel.poll_receive_lane(0, Instant::now(), 64)? {
                match event {
                    ReceiveEvent::Message(value) => {
                        ensure!(!received, "duplicate application delivery");
                        ensure!(value.fragment_id == wire.fragment_id());
                        ensure!(value.message == message, "drain changed signed proposal");
                        received = true;
                    }
                    ReceiveEvent::Rejected { error, .. } => {
                        anyhow::bail!("valid drained proposal rejected: {error}")
                    }
                }
            }
            std::thread::yield_now();
        }
        ensure!(node.bytes[0] == 0 && node.expired_sends == 0);
        ensure!(start.elapsed() < Duration::from_secs(30));
        Ok(())
    })
}

/// Use the original deterministic component key seeds. These are independent
/// signature-valid scheduling fixtures, never journal-issued publication.
fn fixture_control_votes(node: &NativeLifecycle, height: u64) -> Result<Vec<PreparedMessage>> {
    use crate::native_pipeline::consensus::wire::Phase;
    use novovm_consensus::round_bft::test_vectors::{self, FixtureSigningKey};

    let mut context = node.progress.context();
    context.height = height;
    if height > 1 {
        context.parent_block_hash = [0xc1; 32];
        context.parent_decision_hash = [0xc2; 32];
    }
    (101..105)
        .map(|seed| {
            let key = FixtureSigningKey::from_bytes(&[seed; 32]);
            let vote = test_vectors::sign_vote(
                context,
                0,
                Phase::Prevote,
                Some([0xc3; 32]),
                &node.validators,
                &key,
            )?;
            prepared(node, &Message::Vote(vote))
        })
        .collect()
}

#[test]
#[ignore = "real native handles; current control capacity coexists with bounded old drain"]
fn native_realtime_retirement_auxiliary_full_preserves_four_current_controls() -> Result<()> {
    with_delivery_lifecycle_for_test(|node, _| {
        ensure!(
            node.limits.messages_per_peer == 32,
            "retain actual fixture host budget"
        );
        let control =
            transport::durable_control_bound(node.validators.members().len(), node.codec)?;
        ensure!(node.control_reserve_messages == 8);
        ensure!(
            node.control_reserve_bytes == 8 * control,
            "body capacity was incorrectly reserved as control evidence"
        );
        let old = fixture_control_votes(node, 1)?;
        let start = Instant::now();
        for (index, wire) in old.iter().enumerate() {
            ensure!(node.enqueue(0, wire.clone()));
            node.poll_io(
                1000 + index as u64,
                start + Duration::from_millis(index as u64),
            )?;
        }
        ensure!(
            node.deliveries[0].len() == 4
                && node.deliveries[0].iter().all(|delivery| delivery.admitted),
            "fixture did not occupy the original four per-peer control sender slots"
        );
        let old_bytes = node.bytes[0];
        node.height = 2;
        node.retire_realtime()?;
        ensure!(
            node.deliveries[0].len() == 4
                && node.deliveries[0]
                    .iter()
                    .all(|delivery| delivery.admitted && delivery.retiring)
                && node.bytes[0] == old_bytes,
            "retirement prematurely released original send ownership or charge"
        );

        let mut auxiliary = 0usize;
        let mut auxiliary_bytes = 0usize;
        loop {
            ensure!(auxiliary < 32, "auxiliary admission escaped the host bound");
            let wire = request(node, auxiliary as u64)?;
            let bytes = wire.byte_len();
            if !node.enqueue(0, wire) {
                break;
            }
            auxiliary += 1;
            auxiliary_bytes += bytes;
        }
        ensure!(
            auxiliary > 0,
            "reserving control capacity starved all auxiliary work"
        );
        let current = fixture_control_votes(node, 2)?;
        let mut current_bytes = 0usize;
        for (index, wire) in current.iter().enumerate() {
            ensure!(
                node.enqueue(0, wire.clone()),
                "full auxiliary traffic plus bounded old drain blocked current control {index}"
            );
            current_bytes += wire.byte_len();
        }
        ensure!(auxiliary == 24 && node.deliveries[0].len() == 32);
        ensure!(node.bytes[0] == old_bytes + auxiliary_bytes + current_bytes);
        ensure!(node.bytes[0] <= node.limits.bytes_per_peer);
        ensure!(
            node.deliveries[0]
                .iter()
                .filter(|delivery| delivery.retiring)
                .count()
                == 4,
            "capacity was obtained by clearing original draining charges"
        );
        ensure!(!node.enqueue(0, request(node, 100)?));
        ensure!(node.bytes[0] == old_bytes + auxiliary_bytes + current_bytes);
        Ok(())
    })
}
