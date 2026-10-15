//! Real AOEM/native-lifecycle successor regression. Only original transaction
//! bodies and polling schedules are supplied by this fixture. It never signs a
//! validator message, creates a QC, registers a completion or advances a journal.
//! Four sockets/stores in one process are not four-machine/TPS acceptance.
use super::*;
use crate::native_pipeline::consensus::{
    bodies::{BodyLimits, CandidateBodies},
    native_lifecycle::{DeliveryLimits, NativeLifecycle, SuccessorSelection},
};

struct NativeFixture {
    nodes: Vec<NativeLifecycle>,
    base: Fixture,
    started: Instant,
}

impl NativeFixture {
    fn new(inflight: usize) -> Result<Self> {
        let mut base = fixture(ExecutionBackend::CpuCallbacks)?;
        let mut nodes = Vec::new();
        for (i, (driver, channel)) in std::mem::take(&mut base.drivers)
            .into_iter()
            .zip(std::mem::take(&mut base.mesh.channels))
            .enumerate()
        {
            let bodies = CandidateBodies::new(
                &base.services.0[i],
                batch_context(base.anchor.state_root),
                policy(),
                codec(),
                BodyLimits {
                    bodies: 8,
                    bytes: 64 * 1024 * 1024,
                    inflight,
                },
            )?;
            nodes.push(NativeLifecycle::new(
                driver,
                bodies,
                channel,
                &base.set,
                DeliveryLimits {
                    messages_per_peer: 32,
                    bytes_per_peer: 512 * 1024,
                },
                policy(),
                codec(),
            )?);
        }
        Ok(Self {
            nodes,
            base,
            started: Instant::now(),
        })
    }

    fn leader(&self, height: u64) -> Result<usize> {
        let id = self.base.set.leader(height, 0)?;
        self.base
            .ids
            .iter()
            .position(|v| *v == id)
            .context("missing leader")
    }

    fn until(
        &mut self,
        active: &[usize],
        label: &str,
        ready: impl Fn(&[NativeLifecycle]) -> bool,
    ) -> Result<()> {
        let deadline = Instant::now() + DEADLINE;
        while !ready(&self.nodes) {
            ensure!(
                Instant::now() < deadline,
                "{label} timed out: {:?}",
                self.nodes
                    .iter()
                    .map(|n| (
                        n.progress.context().height,
                        n.progress.round(),
                        n.progress.head().map(|h| h.height),
                        n.executed(),
                        n.successor_metrics(),
                        n.is_frozen(),
                    ))
                    .collect::<Vec<_>>()
            );
            let now = Instant::now();
            let wall = 1000 + self.started.elapsed().as_millis() as u64;
            for (i, node) in self.nodes.iter_mut().enumerate() {
                // A paused consensus participant still services the SAME native
                // socket. Two active validators cannot form the four-node QC.
                node.poll_io(wall, now)?;
                if active.contains(&i) {
                    node.poll(&self.base.services.0[i], now)?;
                }
                ensure!(!node.is_frozen(), "native lifecycle froze at node {i}");
            }
            std::thread::yield_now();
        }
        Ok(())
    }

    fn await_height(&mut self, height: u64) -> Result<ParentPoint> {
        self.until(&[0, 1, 2, 3], "four-node durable height", |nodes| {
            nodes.iter().all(|n| {
                n.progress.head().is_some_and(|h| h.height == height)
                    && n.progress.context().height == height + 1
            })
        })?;
        let head = self.nodes[0].progress.head().context("missing head")?;
        ensure!(
            self.nodes.iter().all(|n| n.progress.head() == Some(head)),
            "different durable native heads"
        );
        Ok(head)
    }

    fn offer_current(&mut self, height: u64) -> Result<()> {
        let leader = self.leader(height)?;
        let body = body(self.nodes[leader].progress.parent(), height)?;
        ensure!(
            self.nodes[leader].offer(&body)?,
            "current body backpressured"
        );
        Ok(())
    }
}

fn body(parent: ParentPoint, height: u64) -> Result<Arc<Message>> {
    let mut context = batch_context(parent.state_root);
    context.height = height;
    context.parent_height = parent.height;
    context.parent_block_hash = parent.block_hash;
    context.parent_receipt_root = parent.receipt_batch_commitment;
    context.parent_state_version = parent.state_version;
    context.slot = height - 1;
    context.timestamp_unix_ms += height;
    Ok(Arc::new(Message::ApflBody {
        context,
        batch: Arc::new(ApflTransferBatch::from_raw(
            &super::super::chain_integration::raw_at(height)?,
            ApflLimits {
                transactions: 8,
                transaction_bytes: 1024,
                body_bytes: 8192,
            },
        )?),
    }))
}

fn body_id(message: &Message) -> Result<Hash> {
    let Message::ApflBody { context, batch } = message else {
        bail!("fixture must retain APFL input");
    };
    transport::apfl_body_id(context, batch, codec())
}

fn reject_altered_pins(
    node: &mut NativeLifecycle,
    pipeline: &CandidatePipeline,
    message: &Arc<Message>,
    selected: &SuccessorSelection,
) -> Result<()> {
    for field in 0..8 {
        let mut wrong = selected.clone();
        match field {
            0 => wrong.parent.block_hash[0] ^= 1,
            1 => wrong.parent.state_root[0] ^= 1,
            2 => wrong.parent.receipt_batch_commitment[0] ^= 1,
            3 => wrong.parent.state_version += 1,
            4 => wrong.parent.decision_hash[0] ^= 1,
            5 => wrong.parent.height += 1,
            6 => wrong.source_round += 1,
            _ => wrong.source.genesis_config_commitment[0] ^= 1,
        }
        ensure!(
            node.offer_successor(message, &wrong, pipeline).is_err(),
            "changed exact parent/source pin {field} accepted"
        );
        ensure!(!node.is_frozen(), "bad local hint froze the lifecycle");
    }
    ensure!(node.successor_metrics() == [0; 5], "bad pins consumed work");
    Ok(())
}

#[test]
#[ignore = "real AOEM; native two-node hold, exact parent ACK, four-store publication and cold failure nonce"]
fn native_successor_early_completion_waits_for_parent_ack_and_recovers() -> Result<()> {
    let mut f = NativeFixture::new(2)?;
    let first = f.leader(1)?;
    let next = f.leader(2)?;
    ensure!(
        first != next,
        "fixture requires different consecutive proposers"
    );
    f.offer_current(1)?;
    f.until(
        &[first, next],
        "real parent execution at next proposer",
        |nodes| nodes[next].executed() == 1,
    )?;
    let selection = f.nodes[next]
        .successor_selection()?
        .context("missing real successor basis")?;
    ensure!(
        selection.parent.height == 1,
        "wrong successor parent height"
    );
    let child = body(selection.parent, 2)?;
    let child_id = body_id(&child)?;
    reject_altered_pins(
        &mut f.nodes[next],
        &f.base.services.0[next],
        &child,
        &selection,
    )?;
    ensure!(
        f.nodes[next].offer_successor(&child, &selection, &f.base.services.0[next])?,
        "successor not admitted"
    );
    ensure!(
        f.nodes[next].offer_successor(&child, &selection, &f.base.services.0[next])?,
        "identical optional input was not idempotent"
    );
    f.until(&[first, next], "successor before parent ACK", |nodes| {
        nodes[next].successor_metrics()[2] == 1
    })?;
    ensure!(
        f.nodes[next].successor_metrics() == [1, 1, 1, 0, 0],
        "duplicate or premature successor promotion"
    );
    ensure!(
        f.nodes
            .iter()
            .all(|n| n.progress.head().is_none() && n.progress.context().height == 1)
            && !f.nodes[next].progress.retains_body(child_id),
        "future completion acquired authority before parent QC/ACK"
    );
    // Inspect the actual persisted journal, not just in-memory metrics. Opening
    // this reader does not generate votes/proposals or advance its context.
    let journal = open_journal(
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
        journal.head().is_none() && journal.context().height == 1,
        "speculation wrote durable authority"
    );
    drop(journal);

    // Releasing honest participants is the only authorization for real quorum
    // progression. The fixture supplies no h2 body again and no certificate.
    let head = f.await_height(2)?;
    ensure!(
        f.nodes[next].successor_metrics() == [1, 1, 1, 1, 0],
        "exact ACK did not promote once"
    );
    ensure!(
        f.nodes.iter().all(|n| n.executed() == 2),
        "successor re-executed or remote execution bypassed"
    );
    ensure!(
        f.nodes.iter().all(|n| n.progress.parent() == head),
        "publication did not share the durable parent"
    );
    f.offer_current(3)?;
    let final_head = f.await_height(3)?;
    ensure!(
        f.nodes.iter().all(|n| n.executed() == 3),
        "post-failure successor did not execute once"
    );
    drop(f.nodes);
    f.base.services.shutdown()?;
    for (i, path) in f.base.paths.iter().enumerate() {
        f.base.services.0.push(CandidatePipeline::start(
            pipeline_config(path)?,
            OpenMode::Existing,
        )?);
        let p = f.base.services.0.last().unwrap();
        let journal = open_journal(
            ValidatorJournal::open(
                p,
                f.base.genesis,
                f.base.anchor,
                f.base.set.clone(),
                f.base.keys[i].clone(),
            )?,
            p,
        )?;
        ensure!(
            journal.head() == Some(final_head),
            "cold successor chain head mismatch"
        );
        ensure!(
            super::super::chain_integration::value(
                p,
                final_head.state_root,
                balance_key(&account(2))
            )? == 350u128.to_le_bytes(),
            "cold success/failure balance mismatch"
        );
        for raw in super::super::chain_integration::raw_at(3)? {
            let signed = crate::native_pipeline::ingress::authentication::authenticate_transfer_v3(
                &raw, CHAIN, 1024,
            )?;
            let key = crate::native_pipeline::business::nov_transfer_batch::nonce_key(
                &signed.nonce_identity(),
            );
            ensure!(
                super::super::chain_integration::value(p, final_head.state_root, key)?
                    == 3u64.to_le_bytes(),
                "failed transfer nonce not preserved through successor/restart"
            );
        }
        let mut archived = ArchiveRead::new(2, final_head, f.base.genesis, f.base.set.clone())?;
        let deadline = Instant::now() + DEADLINE;
        let archived = loop {
            ensure!(Instant::now() < deadline, "cold successor archive timeout");
            if let Some(result) = archived.poll(p)? {
                break result.context("missing successor archive")?;
            }
            std::thread::yield_now();
        };
        ensure!(archived.point() == head, "archived successor point changed");
        ensure!(
            archived.certificate().verify(&f.base.set)?.value() == Some(head.block_hash),
            "cold successor QC invalid"
        );
        ensure!(
            archived.stored().receipt_bytes().len() == 2,
            "successor receipt count mismatch"
        );
        ensure!(
            !archived.stored().receipt_view(0)?.success
                && archived.stored().receipt_view(1)?.success,
            "successor business failure/order changed"
        );
    }
    f.base.services.shutdown()
}

#[test]
#[ignore = "real AOEM; one body inflight disables optional work without blocking current native publication"]
fn native_successor_single_inflight_keeps_current_height_progress() -> Result<()> {
    let mut f = NativeFixture::new(1)?;
    let first = f.leader(1)?;
    let next = f.leader(2)?;
    ensure!(
        first != next,
        "fixture requires distinct consecutive leaders"
    );
    f.offer_current(1)?;
    f.until(&[first, next], "single-slot parent execution", |nodes| {
        nodes[next].executed() == 1
    })?;
    ensure!(
        f.nodes[next].successor_selection()?.is_none()
            && !f.nodes[next].has_successor_target(2)
            && f.nodes[next].successor_metrics() == [0; 5],
        "impossible optional work retained a single-capacity target"
    );
    f.await_height(1)?;
    f.offer_current(2)?;
    f.await_height(2)?;
    ensure!(
        f.nodes
            .iter()
            .all(|n| n.executed() == 2 && n.successor_metrics() == [0; 5]),
        "single-slot fallback bypassed or duplicated current execution"
    );
    drop(f.nodes);
    f.base.services.shutdown()
}

#[path = "native_successor_drain_integration.rs"]
mod drain;
