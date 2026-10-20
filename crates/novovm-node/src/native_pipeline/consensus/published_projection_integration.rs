//! Real AOEM and journals, bounded in-process transport of actual signed
//! messages. This verifies publication/view boundaries, not network capacity.
use super::*;

fn execute(
    pipeline: &CandidatePipeline,
    context: BatchContext,
    raw: &[Vec<u8>],
) -> Result<DurableCandidate> {
    let batch = Arc::new(ApflTransferBatch::from_raw(
        raw,
        ApflLimits {
            transactions: 8,
            transaction_bytes: 1024,
            body_bytes: 8192,
        },
    )?);
    let Submission::Accepted(mut ticket) =
        pipeline.try_submit(BatchRequest::from_apfl(batch, context, policy())?)?
    else {
        bail!("empty fixture pipeline backpressured")
    };
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(Instant::now() < deadline, "projection candidate timeout");
        if let Some(done) = ticket.try_take()? {
            return Ok(done.candidate().clone());
        }
        std::thread::yield_now();
    }
}

fn point(
    candidate: &DurableCandidate,
    context: ConsensusContext,
    set: &ValidatorSet,
    parent: ParentPoint,
) -> Result<ParentPoint> {
    let statement = BlockStatement::from_executed(candidate.packet(), context, set, &parent)?;
    Ok(ParentPoint {
        height: context.height,
        block_hash: statement.hash(),
        state_root: candidate.packet().state_root(),
        receipt_batch_commitment: candidate.packet().receipt_batch_commitment(),
        state_version: statement.state_version(),
        decision_hash: crate::native_pipeline::consensus::chain::decision_id(
            &context,
            statement.hash(),
        ),
    })
}

fn archive(
    pipeline: &CandidatePipeline,
    height: u64,
    head: ParentPoint,
    context: ConsensusContext,
    set: Arc<ValidatorSet>,
) -> Result<crate::native_pipeline::consensus::ArchiveBlock> {
    let mut read = ArchiveRead::new(height, head, context, set)?;
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(Instant::now() < deadline, "projection archive timeout");
        if let Some(done) = read.poll(pipeline)? {
            return done.context("archive missing");
        }
        std::thread::yield_now();
    }
}

#[test]
#[ignore = "real AOEM; ACK-bound warm projection, bounded fallback and cold recovery, not TPS"]
fn published_projection_requires_local_ack_exact_head_and_matches_cold_receipts() -> Result<()> {
    let Fixture {
        paths,
        keys,
        ids,
        set,
        genesis,
        anchor,
        mut services,
        mut drivers,
        ..
    } = fixture(ExecutionBackend::CpuCallbacks)?;
    // One candidate slot has no room for the disposable duplicate Arc charge.
    // Consensus must still finish and use the unchanged archive fallback.
    drivers[3] = RoundProgress::new(
        open_journal(
            ValidatorJournal::open(
                &services.0[3],
                genesis,
                anchor,
                set.clone(),
                keys[3].clone(),
            )?,
            &services.0[3],
        )?,
        &services.0[3],
        set.clone(),
        ProgressLimits {
            candidates: 1,
            candidate_bytes: 32 * 1024 * 1024,
            votes: CollectorLimits {
                max_retained_rounds: 4,
                max_future_round_span: 32,
                max_votes: 44,
            },
        },
        TimeoutPolicy {
            propose: Duration::from_secs(5),
            prevote: Duration::from_secs(5),
            precommit: Duration::from_secs(5),
            round_increment: Duration::from_secs(1),
        },
    )?;
    let mut previous = None;
    let mut final_projection = Vec::new();
    for height in 1..=2 {
        let parent = drivers[0].parent();
        let context = drivers[0].context();
        let mut execution = batch_context(parent.state_root);
        execution.height = height;
        execution.parent_height = parent.height;
        execution.parent_block_hash = parent.block_hash;
        execution.parent_state_root = parent.state_root;
        execution.parent_receipt_root = parent.receipt_batch_commitment;
        execution.parent_state_version = parent.state_version;
        execution.slot = height - 1;
        execution.timestamp_unix_ms += height;
        let raw = super::super::chain_integration::raw_at(height)?;
        let id = transport::body_id(&execution, &raw, codec())?;
        let mut candidates = Vec::new();
        for pipeline in &services.0 {
            candidates.push(execute(pipeline, execution, &raw)?);
        }
        let expected = point(&candidates[0], context, &set, parent)?;
        // An immutable/durable candidate is not a published candidate.
        for driver in &drivers {
            assert!(driver.published_packet(expected).is_none());
        }
        assert!(drivers[0]
            .register_candidate(id, &candidates[1], true)
            .is_err());
        for i in 0..4 {
            assert!(drivers[i].register_candidate(id, &candidates[i], true)?);
        }
        let mut competing_raw = raw.clone();
        competing_raw.reverse();
        let other_id = transport::body_id(&execution, &competing_raw, codec())?;
        let competitor = execute(&services.0[0], execution, &competing_raw)?;
        let competing_point = point(&competitor, context, &set, parent)?;
        assert_ne!(competing_point, expected);
        assert!(drivers[0].register_candidate(other_id, &competitor, false)?);
        assert!(drivers[0].published_packet(competing_point).is_none());
        // Hold one non-proposer's control polling until a genuine peer Decision
        // arrives. Three active drivers can decide; receiving the certificate
        // still cannot publish on the held owner's behalf.
        let leader = ids
            .iter()
            .position(|id| *id == set.leader(height, 0).unwrap())
            .unwrap();
        let held = (leader + 1) % 4;
        let mut released = false;
        let deadline = Instant::now() + DEADLINE;
        loop {
            ensure!(Instant::now() < deadline, "projection consensus deadline");
            let mut messages = Vec::new();
            for (i, driver) in drivers.iter_mut().enumerate() {
                if i == held && !released {
                    continue;
                }
                driver.poll(&services.0[i], Instant::now())?;
                if driver.head() != Some(expected) {
                    assert!(driver.published_packet(expected).is_none());
                }
                if let Some(message) = driver.take_message() {
                    messages.push((i, message));
                }
            }
            for (sender, message) in messages {
                for (i, driver) in drivers.iter_mut().enumerate() {
                    if i == sender || driver.context().height != height {
                        continue;
                    }
                    let before = driver.head();
                    driver.receive(ids[sender], &message)?;
                    assert_eq!(driver.head(), before);
                    if before != Some(expected) {
                        // Includes a genuine remote Decision: receipt alone cannot populate the slot.
                        assert!(driver.published_packet(expected).is_none());
                    }
                    if i == held && !released && matches!(message, Message::Decision { .. }) {
                        assert_eq!(before, previous);
                        released = true;
                    }
                }
            }
            if drivers.iter().all(|d| d.context().height == height + 1) {
                break;
            }
            std::thread::yield_now();
        }
        assert!(released, "held peer never received genuine decision");
        for i in 0..4 {
            assert_eq!(drivers[i].head(), Some(expected));
            let cold = archive(
                &services.0[i],
                height,
                expected,
                drivers[i].context(),
                set.clone(),
            )?;
            if i == 3 {
                assert!(drivers[i].published_packet(expected).is_none());
            } else {
                let hot = drivers[i]
                    .published_packet(expected)
                    .context("ACK view missing")?;
                assert_eq!(hot.candidate_id(), candidates[i].packet().candidate_id());
                for (index, row) in hot.receipt_projection().iter().enumerate() {
                    let archived = cold.stored().receipt_view(index)?;
                    assert_eq!(row.tx_hash, archived.tx_hash);
                    assert_eq!(row.signer_identity, archived.signer_identity);
                    assert_eq!(row.nonce_after, archived.nonce_after);
                    assert_eq!(row.success, archived.success);
                    assert_eq!(row.charged_fee, archived.charged_fee.parse::<u128>()?);
                }
                if i == 0 {
                    final_projection = hot.receipt_projection().to_vec();
                }
            }
            assert!(drivers[i].published_packet(competing_point).is_none());
            if let Some(old) = previous {
                assert!(drivers[i].published_packet(old).is_none());
            }
            for field in 0..6 {
                let mut wrong = expected;
                match field {
                    0 => wrong.height += 1,
                    1 => wrong.block_hash[0] ^= 1,
                    2 => wrong.state_root[0] ^= 1,
                    3 => wrong.receipt_batch_commitment[0] ^= 1,
                    4 => wrong.state_version += 1,
                    _ => wrong.decision_hash[0] ^= 1,
                }
                assert!(drivers[i].published_packet(wrong).is_none());
            }
        }
        previous = Some(expected);
    }
    assert!(final_projection.iter().any(|r| !r.success));
    let head = previous.unwrap();
    drop(drivers);
    services.shutdown()?;
    services.0.push(CandidatePipeline::start(
        pipeline_config(&paths[0])?,
        OpenMode::Existing,
    )?);
    let pipeline = &services.0[0];
    let reopened = progress(
        open_journal(
            ValidatorJournal::open(pipeline, genesis, anchor, set.clone(), keys[0].clone())?,
            pipeline,
        )?,
        pipeline,
        set.clone(),
    )?;
    assert_eq!(reopened.head(), Some(head));
    assert!(reopened.published_packet(head).is_none());
    let cold = archive(pipeline, head.height, head, reopened.context(), set)?;
    for (index, hot) in final_projection.iter().enumerate() {
        let row = cold.stored().receipt_view(index)?;
        assert_eq!(row.tx_hash, hot.tx_hash);
        assert_eq!(row.success, hot.success);
        assert_eq!(row.nonce_after, hot.nonce_after);
        assert_eq!(row.charged_fee.parse::<u128>()?, hot.charged_fee);
    }
    drop(reopened);
    services.shutdown()?;
    Ok(())
}
