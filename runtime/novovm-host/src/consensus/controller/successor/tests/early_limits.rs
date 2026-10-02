//! Real AOEM/controller regressions. Only owner reply consumption and control
//! arrival are scheduled; no native callback is blocked or fabricated here.
use super::*;
use crate::consensus::transport::EarlyBodyScope;

fn admit_without_authentication(controller: &mut Controller) -> Result<Arc<Message>> {
    let scope = controller
        .early_body_scope()?
        .context("early scope unavailable")?;
    let message = Arc::new(Message::EarlyBody {
        scope,
        raw_transactions: raw(scope.target_height, 0)?,
    });
    flush(controller)?;
    ensure!(
        controller.try_submit_early_body(
            &message,
            scope.source.height,
            172_800_500 + scope.target_height
        )?,
        "real owner did not admit early input"
    );
    pump_owner(controller, |c| {
        c.fixed.iter().any(|f| {
        matches!(f.prepared.message().as_ref(), Message::EarlyBody { scope: found, .. } if *found == scope)
    })
    })?;
    ensure!(
        controller.has_early_target(scope.target_height)
            && controller.stats.early_authentication_started == 0,
        "fixture must consume only assembly, not a native authentication reply"
    );
    Ok(message)
}

fn current_candidate(controller: &Controller) -> Option<DurableCandidate> {
    controller
        .bodies
        .values()
        .filter_map(|body| body.candidate.as_ref())
        .map(|(_, candidate)| candidate)
        .find(|candidate| controller.matches_context(candidate.packet().context()))
        .cloned()
}

fn execute_full_current(
    controller: &mut Controller,
    pipeline: &CandidatePipeline,
) -> Result<DurableCandidate> {
    let message = body(controller.parent(), 0)?;
    let now = Instant::now();
    let deadline = now + DEADLINE;
    flush(controller)?;
    while !controller.try_submit_body(&message)? {
        ensure!(
            Instant::now() < deadline,
            "ordinary Full admission stalled after early refusal"
        );
        controller.poll(pipeline, now)?;
        std::thread::yield_now();
    }
    loop {
        ensure!(
            Instant::now() < deadline,
            "ordinary Full execution stalled after early refusal: {:?}",
            controller.stats()
        );
        controller.poll(pipeline, now)?;
        if let Some(candidate) = current_candidate(controller) {
            return Ok(candidate);
        }
        std::thread::yield_now();
    }
}

fn economic_oracle(
    controller: &mut Controller,
    pipeline: &CandidatePipeline,
    parent: ParentPoint,
    child: &DurableCandidate,
) -> Result<()> {
    if controller.journal.is_pending() {
        acknowledge(controller, pipeline)?;
    }
    flush(controller)?;
    let mut ticket = submit(
        pipeline,
        BatchRequest::new(raw(2, 0)?, execution(parent), policy())?,
    )?;
    let deadline = Instant::now() + DEADLINE;
    let oracle = loop {
        ensure!(
            Instant::now() < deadline,
            "ordinary economic oracle timed out"
        );
        if let Some(batch) = ticket.try_take()? {
            break batch;
        }
        std::thread::yield_now();
    };
    ensure!(
        oracle.observation.peak_callbacks > 0
            && oracle.packet.records() == child.packet().records(),
        "early/fallback child differs from genuine ordinary AOEM execution"
    );
    ensure!(
        read(
            pipeline,
            child.packet().state_root(),
            balance_key(&account(2))
        )? == 300u128.to_le_bytes(),
        "actual child recipient differs after two heights"
    );
    for raw in raw(2, 0)? {
        let tx = authenticate_transfer_v3(&raw, CHAIN, 1024)?;
        ensure!(
            read(
                pipeline,
                child.packet().state_root(),
                nonce_key(&tx.nonce_identity())
            )? == 2u64.to_le_bytes(),
            "actual child nonce differs after two heights"
        );
    }
    ensure!(
        controller.head() == Some(parent)
            && controller.context().height == 2
            && controller.round() == 0,
        "fixture escaped by changing parent/round or publishing the child"
    );
    Ok(())
}

#[test]
#[ignore = "requires real AOEM; promoted early input must authenticate/bind/execute despite continuous verified control ingress"]
fn real_promoted_early_body_progresses_under_continuous_control_ready() -> Result<()> {
    Fixture::run("early-warm-control", |controller, pipeline| {
        let _early = admit_without_authentication(controller)?;
        let parent = executed_parent(controller, pipeline, 0, controller.local_peer.clone())?;
        let parent_point = decide_and_advance(controller, pipeline, &parent)?;
        ensure!(
            controller.has_early_target(2)
                && controller.stats.early_authentication_started == 0
                && controller.is_local_leader()?
                && controller.bodies.is_empty(),
            "fixture did not retain an unexecuted early target across real parent ACKs"
        );
        let (&validator, peer) = controller
            .config
            .peers
            .iter()
            .next()
            .context("remote route absent")?;
        let peer = peer.clone();
        let vote = Vote::sign(
            controller.context(),
            0,
            Phase::Prevote,
            None,
            &controller.config.validators,
            &key(index_for(&controller.config.validators, validator)),
        )?;
        let control = owner_ready(controller, Arc::new(Message::Vote(vote)))?;
        // A fixed protocol clock rules out a timeout/round change as a false
        // escape. The independent real wall-clock bound and AOEM remain live.
        let now = Instant::now();
        let deadline = now + DEADLINE;
        let mut supplied = 0usize;
        let child = loop {
            ensure!(
                Instant::now() < deadline,
                "fresh control retirement starved current early execution: {:?}",
                controller.stats()
            );
            supplied += usize::from(controller.regression_warm_control(&peer, &control)?);
            controller.poll(pipeline, now)?;
            if let Some(candidate) = current_candidate(controller) {
                break candidate;
            }
            std::thread::yield_now();
        };
        ensure!(
            supplied >= 2
                && controller.stats.received_votes >= 1
                && controller.stats.early_authentication_started == 1
                && controller.stats.early_authentication_completed == 1
                && controller.stats.early_bind_reused == 1
                && controller.stats.execution_failures == 0,
            "control fixture bypassed genuine early work or failed to deliver verified controls"
        );
        controller.retire(Retirement::Ready(control));
        economic_oracle(controller, pipeline, parent_point, &child)
    })
}

#[test]
#[ignore = "requires real AOEM; legal one-inflight controller refuses early work without blocking normal current execution"]
fn real_single_inflight_controller_refuses_early_and_full_current_still_executes() -> Result<()> {
    Fixture::run("early-one-controller-slot", |controller, pipeline| {
        controller.config.limits.max_inflight = 1;
        validate_config(&controller.config, &controller.journal)?;
        let scope = EarlyBodyScope {
            source: controller.context(),
            source_round: controller.round(),
            target_height: 2,
        };
        let message = Arc::new(Message::EarlyBody {
            scope,
            raw_transactions: raw(2, 0)?,
        });
        ensure!(
            controller.early_body_scope()?.is_none()
                && !controller.try_submit_early_body(&message, 1, 172_800_502)?,
            "one-inflight controller admitted impossible optional local work"
        );
        // Actual owner decoding/preparation also cannot smuggle the optional
        // input around the same limit through the received-body entry point.
        let ready = owner_ready(controller, message)?;
        controller.receive(controller.local_peer.clone(), ready)?;
        flush(controller)?;
        ensure!(
            controller.early.is_none()
                && !controller.has_early_target(2)
                && controller.stats.early_authentication_started == 0,
            "received early input retained an impossible target"
        );
        let parent = executed_parent(controller, pipeline, 0, controller.local_peer.clone())?;
        let parent_point = decide_and_advance(controller, pipeline, &parent)?;
        let child = execute_full_current(controller, pipeline)?;
        economic_oracle(controller, pipeline, parent_point, &child)
    })
}

fn reopen_single_batch(fixture: &mut Fixture) -> Result<()> {
    let controller = fixture.controller.as_ref().unwrap();
    let context = controller.context();
    let parent = controller.parent();
    let config = ControllerConfig {
        validators: controller.config.validators.clone(),
        local_validator: controller.config.local_validator,
        peers: controller.config.peers.clone(),
        execution: controller.config.execution,
        collector: controller.config.collector,
        timeouts: controller.config.timeouts,
        limits: controller.config.limits,
        retransmit: controller.config.retransmit,
    };
    let local = index_for(&config.validators, config.local_validator);
    fixture.shutdown()?;
    fixture.pipeline_config.max_batches = 1;
    fixture.pipeline = Some(CandidatePipeline::start(
        fixture.pipeline_config.clone(),
        OpenMode::Existing,
    )?);
    let pipeline = fixture.pipeline.as_ref().unwrap();
    let journal = open(
        ValidatorJournal::open(
            pipeline,
            context,
            parent,
            config.validators.clone(),
            key(local),
        )?,
        pipeline,
    )?;
    let channel = channel(&config, local)?;
    fixture.controller = Some(Controller::new(config, journal, channel)?);
    Ok(())
}

#[test]
#[ignore = "requires real AOEM; genuinely reopened one-batch pipeline retires admitted early input and preserves Full fallback"]
fn real_single_batch_pipeline_releases_early_target_then_full_current_executes() -> Result<()> {
    let mut fixture = Fixture::new("early-one-pipeline-slot")?;
    reopen_single_batch(&mut fixture)?;
    let result = (|| -> Result<()> {
        let controller = fixture.controller.as_mut().unwrap();
        let pipeline = fixture.pipeline.as_ref().unwrap();
        let message = admit_without_authentication(controller)?;
        controller.poll_early(pipeline)?;
        flush(controller)?;
        ensure!(
            controller.early.is_none()
                && controller.early_drain.is_none()
                && !controller.has_early_target(2)
                && controller.stats.early_authentication_started == 0
                && controller.fixed.iter().all(|f| !matches!(
                    f.prepared.message().as_ref(),
                    Message::EarlyBody { .. } | Message::BindBody { .. }
                )),
            "impossible background capacity retained the target, permit or raw cache"
        );
        // Same-generation replay must not repeatedly reinstall the failed
        // optional slot, even though its valid original transactions are reusable.
        let ready = owner_ready(controller, message)?;
        controller.receive(controller.local_peer.clone(), ready)?;
        controller.poll_early(pipeline)?;
        flush(controller)?;
        ensure!(
            controller.early.is_none() && !controller.has_early_target(2),
            "same-generation replay revived a retired impossible slot"
        );
        let parent = executed_parent(controller, pipeline, 0, controller.local_peer.clone())?;
        let parent_point = decide_and_advance(controller, pipeline, &parent)?;
        let child = execute_full_current(controller, pipeline)?;
        economic_oracle(controller, pipeline, parent_point, &child)
    })();
    let shutdown = fixture.shutdown();
    result?;
    shutdown
}

#[test]
#[ignore = "requires real AOEM; current-body pressure retires early/Bind caches but preserves the accepted successor ticket for drain"]
fn real_current_body_preempts_early_successor_caches_without_losing_native_ticket() -> Result<()> {
    Fixture::run("early-successor-budget", |controller, pipeline| {
        let message = admit_without_authentication(controller)?;
        let parent = executed_parent(controller, pipeline, 0, controller.local_peer.clone())?;
        let deadline = Instant::now() + DEADLINE;
        loop {
            ensure!(
                Instant::now() < deadline,
                "early body did not enter actual successor execution: {:?}",
                controller.stats()
            );
            flush(controller)?;
            controller.poll_early(pipeline)?;
            controller.flush_preparations()?;
            match controller.channel.try_recv()? {
                Some(ChannelEvent::Prepared { token, result }) => {
                    controller.prepared(token, result)?
                }
                Some(ChannelEvent::Received(received)) => {
                    controller.receive(received.peer, received.ready)?
                }
                None => {}
            }
            flush(controller)?;
            controller.poll_successor(pipeline)?;
            if controller
                .successor
                .as_ref()
                .is_some_and(|s| s.ticket.is_some())
            {
                break;
            }
            std::thread::yield_now();
        }
        // Only reply consumption is held here; the real native job may have
        // finished already. Its accepted, unconsumed ticket must still count.
        pump_owner(controller, |c| {
            c.fixed
                .iter()
                .any(|f| matches!(f.prepared.message().as_ref(), Message::BindBody { .. }))
        })?;
        let successor = controller
            .successor
            .as_ref()
            .context("successor vanished")?;
        let future_id = successor
            .body
            .as_ref()
            .unwrap()
            .prepared
            .body_id()
            .context("future body identity absent")?;
        ensure!(
            successor.ticket.is_some()
                && successor.body.as_ref().unwrap().early_origin.is_some()
                && controller.early.is_none()
                && controller.stats.early_bind_reused == 1,
            "fixture did not transfer the original authenticated body to a native successor ticket"
        );
        let early_fragments: Vec<_> = controller
            .fixed
            .iter()
            .filter(|f| {
                matches!(
                    f.prepared.message().as_ref(),
                    Message::EarlyBody { .. } | Message::BindBody { .. }
                )
            })
            .map(|f| f.prepared.fragment_id())
            .collect();
        ensure!(
            early_fragments.len() == 2,
            "fixture lacks both actual Early and Bind retransmission caches"
        );
        let reserved = controller.config.validators.members().len() * 2
            + controller.config.limits.max_inflight
            + 2;
        let charge = controller.channel.preparation_charge();
        controller.config.limits.max_body_bytes = reserved * charge;
        validate_config(&controller.config, &controller.journal)?;
        let retained = controller.retained_bodies().len();
        ensure!(
            retained == 3,
            "expected parent, early raw and canonical successor allocations"
        );
        for extra in retained..reserved {
            let ready = owner_ready(controller, body(controller.parent(), extra as u128 + 2)?)?;
            ensure!(
                controller.cache(ready.prepared.clone(), None),
                "legal fixed-body fill refused"
            );
            controller.retire(Retirement::Ready(ready));
            flush(controller)?;
        }
        ensure!(
            controller.body_bytes() == controller.config.limits.max_body_bytes,
            "fixture did not fill the existing legal byte ceiling"
        );
        let ready = owner_ready(controller, body(controller.parent(), 1)?)?;
        let current_id = ready
            .prepared
            .body_id()
            .context("current body identity absent")?;
        let source = controller.config.peers.values().next().unwrap().clone();
        controller.keep_body(source.clone(), ready, None)?;
        ensure!(
            controller.bodies.contains_key(&current_id)
                && controller.successor.is_none()
                && controller.successor_drain.is_some()
                && controller.inflight_count() == 1
                && !controller.bodies.contains_key(&future_id)
                && controller
                    .fixed
                    .iter()
                    .all(|f| !early_fragments.contains(&f.prepared.fragment_id()))
                && controller.body_bytes() <= controller.config.limits.max_body_bytes,
            "preemption left early payload/Bind cache or dropped accepted native work"
        );
        let started = controller.stats.early_authentication_started;
        let replay = owner_ready(controller, message)?;
        controller.receive(controller.local_peer.clone(), replay)?;
        ensure!(
            controller.early.is_none() && controller.stats.early_authentication_started == started,
            "retired scope restarted authentication before its abandoned work drained"
        );
        drain(controller)?;
        ensure!(
            controller.bodies[&current_id].request.is_some()
                && controller.bodies[&current_id].candidate.is_none()
                && !controller.bodies.contains_key(&future_id),
            "late successor completion attached to unrelated current input"
        );
        let current = executed_parent(controller, pipeline, 1, source)?;
        ensure!(
            current.packet().state_root() != parent.packet().state_root()
                && read(
                    pipeline,
                    current.packet().state_root(),
                    balance_key(&account(2))
                )? == 151u128.to_le_bytes()
                && controller.head().is_none(),
            "ordinary competing current input failed real execution after optional preemption"
        );
        Ok(())
    })
}
