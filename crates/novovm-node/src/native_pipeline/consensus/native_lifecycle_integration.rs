//! Fixtures supply local business bodies only: one leader input in the original
//! lifecycle test, or independently prepared follower inputs in the overlap test.
//! They do not forward protocol messages, construct votes/QCs or register results.
//! All remote body requests, native delivery and AOEM execution are node code.
use super::*;
use crate::native_pipeline::consensus::{
    bodies::{BodyLimits, CandidateBodies},
    native_lifecycle::{DeliveryLimits, NativeLifecycle},
};

#[test]
#[ignore = "real AOEM; four native sockets, exact-body retrieval and autonomous execution"]
fn native_lifecycle_single_body_three_heights_cpu() -> Result<()> {
    run_lifecycle(ExecutionBackend::CpuCallbacks)
}
#[test]
#[ignore = "real AOEM full algebraic backend; not a performance measurement"]
fn native_lifecycle_single_body_three_heights_algebraic() -> Result<()> {
    run_lifecycle(ExecutionBackend::AoemIntegerV7)
}

#[test]
#[ignore = "real AOEM; follower preparation before a real native leader proposal"]
fn native_lifecycle_follower_preparation_cpu() -> Result<()> {
    follower_preparation(ExecutionBackend::CpuCallbacks)
}

#[test]
#[ignore = "real AOEM algebraic execution; no synthetic proposal or QC"]
fn native_lifecycle_follower_preparation_algebraic() -> Result<()> {
    follower_preparation(ExecutionBackend::AoemIntegerV7)
}

fn follower_preparation(backend: ExecutionBackend) -> Result<()> {
    let Fixture {
        ids,
        set,
        mut mesh,
        mut services,
        anchor,
        drivers,
        ..
    } = fixture(backend)?;
    let context = batch_context(anchor.state_root);
    let raw = super::super::chain_integration::raw_at(1)?;
    let body = Arc::new(Message::ApflBody {
        context,
        batch: Arc::new(ApflTransferBatch::from_raw(
            &raw,
            ApflLimits {
                transactions: 8,
                transaction_bytes: 1024,
                body_bytes: 8192,
            },
        )?),
    });
    let mut reversed = raw;
    reversed.reverse();
    let alternative = Arc::new(Message::ApflBody {
        context,
        batch: Arc::new(ApflTransferBatch::from_raw(
            &reversed,
            ApflLimits {
                transactions: 8,
                transaction_bytes: 1024,
                body_bytes: 8192,
            },
        )?),
    });
    let leader = ids
        .iter()
        .position(|id| *id == set.leader(1, 0).unwrap())
        .unwrap();
    let different = (leader + 1) % ids.len();
    let mut nodes = Vec::new();
    for (i, (driver, channel)) in drivers
        .into_iter()
        .zip(std::mem::take(&mut mesh.channels))
        .enumerate()
    {
        nodes.push(NativeLifecycle::new(
            driver,
            CandidateBodies::new(
                &services.0[i],
                context,
                policy(),
                codec(),
                BodyLimits {
                    bodies: 8,
                    bytes: 256 * 1024,
                    inflight: 2,
                },
            )?,
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
    // Only followers receive local input here. Actual AOEM verification and
    // execution must finish without any leader proposal or fixture registration.
    for (i, node) in nodes.iter_mut().enumerate() {
        if i != leader {
            ensure!(node.offer(if i == different { &alternative } else { &body })?);
        }
    }
    let now = Instant::now();
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(
            Instant::now() < deadline,
            "followers did not prepare without leader"
        );
        for (i, node) in nodes.iter_mut().enumerate() {
            // Freeze protocol time only in this functional setup: slow GPU
            // initialization must not turn the no-proposal assertion into a
            // timer test. The finite wall-clock deadline still bounds execution.
            node.poll(&services.0[i], now)?;
            ensure!(!node.is_frozen());
            ensure!(
                node.progress.head().is_none(),
                "preparation published state"
            );
            ensure!(
                node.progress.step() == TimeoutStep::Propose,
                "preparation alone advanced signing phase"
            );
            // Recovery legitimately queues unsigned RequestDecision probes.
            // It must not emit any candidate/vote/decision while preparing.
            node.assert_only_initial_archive_probes_for_test()?;
        }
        if nodes
            .iter()
            .enumerate()
            .all(|(i, node)| i == leader || node.executed() == 1)
        {
            break;
        }
        std::thread::yield_now();
    }
    ensure!(nodes[leader].executed() == 0);
    ensure!(nodes[leader].offer(&body)?);
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(
            Instant::now() < deadline,
            "prepared followers failed native consensus"
        );
        for (i, node) in nodes.iter_mut().enumerate() {
            node.poll_io(1000 + now.elapsed().as_millis() as u64, Instant::now())?;
            node.poll(&services.0[i], now)?;
            ensure!(!node.is_frozen());
        }
        if nodes
            .iter()
            .all(|node| node.progress.head().is_some_and(|head| head.height == 1))
        {
            break;
        }
        std::thread::yield_now();
    }
    let head = nodes[leader].progress.head().unwrap();
    for (i, node) in nodes.iter().enumerate() {
        ensure!(
            node.progress.head() == Some(head),
            "local alternative replaced leader value"
        );
        ensure!(
            node.executed() == if i == different { 2 } else { 1 },
            "matching candidate reran or different body bypassed independent execution"
        );
        if i == different {
            ensure!(
                node.requested_bodies > 0,
                "different candidate bypassed exact-body request"
            );
        } else {
            ensure!(
                node.requested_bodies == 0,
                "matching preparation was not reused"
            );
        }
    }
    drop(nodes);
    services.shutdown()
}

fn run_lifecycle(backend: ExecutionBackend) -> Result<()> {
    let Fixture {
        paths,
        keys,
        ids,
        set,
        genesis,
        mut mesh,
        mut services,
        anchor,
        drivers,
        ..
    } = fixture(backend)?;
    let mut nodes = Vec::new();
    for (i, (driver, channel)) in drivers
        .into_iter()
        .zip(std::mem::take(&mut mesh.channels))
        .enumerate()
    {
        let bodies = CandidateBodies::new(
            &services.0[i],
            batch_context(anchor.state_root),
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
    let start = Instant::now();
    let mut final_head = None;
    for height in 1..=3 {
        let parent = nodes[0].progress.parent();
        ensure!(parent.height == height - 1, "height did not advance");
        let mut context = batch_context(parent.state_root);
        context.height = height;
        context.parent_height = parent.height;
        context.parent_block_hash = parent.block_hash;
        context.parent_receipt_root = parent.receipt_batch_commitment;
        context.parent_state_version = parent.state_version;
        context.slot = height - 1;
        context.timestamp_unix_ms += height;
        let raw = super::super::chain_integration::raw_at(height)?;
        let body = Arc::new(Message::ApflBody {
            context,
            batch: Arc::new(ApflTransferBatch::from_raw(
                &raw,
                ApflLimits {
                    transactions: 8,
                    transaction_bytes: 1024,
                    body_bytes: 8192,
                },
            )?),
        });
        let leader = ids
            .iter()
            .position(|id| *id == set.leader(height, 0).unwrap())
            .unwrap();
        let laggard = (leader + 1) % 4;
        if height == 1 {
            let mut bounded = CandidateBodies::new(
                &services.0[leader],
                context,
                policy(),
                codec(),
                BodyLimits {
                    bodies: 1,
                    bytes: 32768,
                    inflight: 1,
                },
            )?;
            ensure!(
                bounded
                    .offer(&body, false, &nodes[leader].progress)
                    .is_err(),
                "unsolicited body bypassed proposal demand"
            );
            ensure!(
                bounded.offer(&body, true, &nodes[leader].progress)?,
                "bounded first body rejected"
            );
            let mut reversed = super::super::chain_integration::raw_at(height)?;
            reversed.reverse();
            let alternative = Arc::new(Message::Body {
                context,
                raw_transactions: reversed,
            });
            let before = bounded.retained();
            ensure!(
                !bounded.offer(&alternative, true, &nodes[leader].progress)?,
                "body count limit bypassed"
            );
            ensure!(
                Arc::strong_count(&alternative) == 1 && bounded.retained() == before,
                "backpressure consumed caller input"
            );
            ensure!(
                bounded
                    .poll(&services.0[laggard], &mut nodes[leader].progress)
                    .is_err()
                    && bounded.is_frozen(),
                "foreign execution owner accepted"
            );
            let mut invalid = super::super::chain_integration::raw_at(height)?;
            *invalid[0].last_mut().unwrap() ^= 1;
            ensure!(
                nodes[leader].offer(&Arc::new(Message::Body {
                    context,
                    raw_transactions: invalid,
                }))?,
                "invalid-signature input not queued for independent validation"
            );
        }
        ensure!(nodes[leader].offer(&body)?, "local input was backpressured");
        // Same bytes do not create another execution; stale parent never enters.
        ensure!(nodes[leader].offer(&body)?, "idempotent body offer failed");
        let mut wrong = context;
        wrong.parent_state_root = [9; 32];
        ensure!(
            nodes[leader]
                .offer(&Arc::new(Message::Body {
                    context: wrong,
                    raw_transactions: raw
                }))
                .is_err(),
            "wrong parent accepted"
        );
        let deadline = Instant::now() + DEADLINE;
        let mut released = height != 1;
        loop {
            ensure!(
                Instant::now() < deadline,
                "native lifecycle timeout h{height}: {:?}",
                nodes
                    .iter()
                    .map(|n| (
                        n.progress.context().height,
                        n.progress.round(),
                        n.progress.step(),
                        n.progress.head().map(|h| h.height),
                        n.executed(),
                        n.requested_bodies,
                        n.served_bodies,
                        n.rejected_messages,
                        n.pending_deliveries()
                    ))
                    .collect::<Vec<_>>()
            );
            if !released
                && nodes
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != laggard)
                    .all(|(_, n)| n.progress.head().is_some_and(|h| h.height == height))
            {
                released = true;
            }
            for (i, node) in nodes.iter_mut().enumerate() {
                // The fourth node's socket still receives/ACKs only after its
                // application takes custody. Three honest nodes must decide
                // without waiting for its missing execution or delivery.
                node.poll_io(1000 + start.elapsed().as_millis() as u64, Instant::now())?;
                if i != laggard || released {
                    node.poll(&services.0[i], Instant::now())?;
                }
                ensure!(!node.is_frozen(), "healthy lifecycle froze");
            }
            if nodes.iter().all(|n| {
                n.progress.head().is_some_and(|h| h.height == height)
                    && n.progress.context().height == height + 1
            }) {
                break;
            }
            std::thread::yield_now();
        }
        let head = nodes[0].progress.head().unwrap();
        ensure!(
            nodes.iter().all(|n| n.progress.head() == Some(head)),
            "different published heads"
        );
        ensure!(
            nodes.iter().all(|n| n.executed() == height),
            "duplicate execution or fixture bypass"
        );
        final_head = Some(head);
    }
    ensure!(
        nodes.iter().map(|n| n.requested_bodies).sum::<u64>() >= 9,
        "missing remote exact-body requests"
    );
    ensure!(
        nodes.iter().map(|n| n.served_bodies).sum::<u64>() >= 9,
        "missing native body responses"
    );
    let head = final_head.unwrap();
    ensure!(
        nodes
            .iter()
            .map(NativeLifecycle::rejected_execution)
            .sum::<u64>()
            == 1,
        "bad signature was not rejected exactly once"
    );
    drop(nodes);
    services.shutdown()?;
    for (i, path) in paths.iter().enumerate() {
        let p = CandidatePipeline::start(pipeline_config(path)?, OpenMode::Existing)?;
        services.0.push(p);
        let p = services.0.last().unwrap();
        let journal = open_journal(
            ValidatorJournal::open(p, genesis, anchor, set.clone(), keys[i].clone())?,
            p,
        )?;
        ensure!(journal.head() == Some(head), "cold published head differs");
        let value =
            super::super::chain_integration::value(p, head.state_root, balance_key(&account(2)))
                .unwrap();
        ensure!(
            value == 350u128.to_le_bytes().to_vec(),
            "cold balance differs"
        );
        for raw in super::super::chain_integration::raw_at(3)? {
            let signed = crate::native_pipeline::ingress::authentication::authenticate_transfer_v3(
                &raw, CHAIN, 1024,
            )?;
            let key = crate::native_pipeline::business::nov_transfer_batch::nonce_key(
                &signed.nonce_identity(),
            );
            let nonce = super::super::chain_integration::value(p, head.state_root, key)?;
            ensure!(nonce == 3u64.to_le_bytes(), "cold failure nonce differs");
        }
        let mut read = ArchiveRead::new(2, head, genesis, set.clone())?;
        let deadline = Instant::now() + DEADLINE;
        let archived = loop {
            if let Some(block) = read.poll(p)? {
                break block.context("missing archived failed batch")?;
            }
            ensure!(Instant::now() < deadline, "archive deadline");
            std::thread::yield_now();
        };
        ensure!(
            archived.certificate().verify(&set)?.value() == Some(archived.point().block_hash),
            "invalid cold QC"
        );
        ensure!(
            archived.stored().receipt_bytes().len() == 2,
            "receipt count differs"
        );
        let successes = (0..2)
            .map(|i| archived.stored().receipt_view(i))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter(|r| r.success)
            .count();
        ensure!(
            successes == 1,
            "failed business transfer counted as success"
        );
    }
    services.shutdown()
}
