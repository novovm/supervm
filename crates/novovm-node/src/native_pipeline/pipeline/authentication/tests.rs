//! Channel tests prove ownership/bounds, not native execution. The ignored
//! real-library test uses the resident pipeline and reopens its actual store.

use super::*;
use crate::native_pipeline::business::direct_nov_fee::FeeState;
use crate::native_pipeline::business::nov_transfer_batch::{
    balance_key, fee_record_changes, NovTransferPlan,
};
use crate::native_pipeline::ingress::batch::authenticate_batch_for_proof;
use crate::native_pipeline::persistence::CandidateStore;
use crate::native_pipeline::pipeline::compute::tests::{account, context, domain, policy, signed};
use crate::native_pipeline::pipeline::tests::{config, inert_pipeline, request};
use crate::native_pipeline::state::tree::{
    empty_root, read_state_value, stage_state_update, StateChange, StateNodeReader,
};
use std::collections::BTreeMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn input() -> AuthenticationRequest {
    AuthenticationRequest::new(vec![signed(1, 10), signed(2, 20)], policy()).unwrap()
}

#[test]
fn apfl_authentication_reserves_expanded_body_and_returns_same_shared_source() {
    use crate::native_pipeline::ingress::apfl::{ApflLimits, ApflTransferBatch};
    let raw = vec![signed(1, 10), signed(2, 20)];
    let limits = ApflLimits {
        transactions: 16,
        transaction_bytes: 4096,
        body_bytes: 65_536,
    };
    let batch = Arc::new(ApflTransferBatch::from_raw(&raw, limits).unwrap());
    let request = AuthenticationRequest::from_apfl(batch.clone(), policy()).unwrap();
    let mut config = cfg();
    assert_eq!(request.body_bytes, raw.iter().map(Vec::len).sum::<usize>());
    assert_eq!(
        request.reservation(&config).unwrap(),
        input().reservation(&config).unwrap()
    );
    config.max_batches = 1;
    let (pipeline, _) = inert_pipeline(config);
    let AuthenticationSubmission::Backpressured(returned) =
        pipeline.try_authenticate_owned(request).unwrap()
    else {
        panic!("structured authentication bypassed ordinary reservation")
    };
    let BatchSource::Apfl(retained) = returned.request.raw_transactions else {
        panic!("expanded structured input")
    };
    assert!(Arc::ptr_eq(&batch, &retained));
    assert_eq!(usage(&pipeline), (0, 0, 0));
}

fn cfg() -> PipelineConfig {
    let mut config = config("unused-authentication-channel-fixture".into());
    config.max_batches = 3;
    config.max_retained_bytes = 512 * 1024 * 1024;
    config
}

fn usage(pipeline: &CandidatePipeline) -> (usize, usize, usize) {
    let current = pipeline.usage.lock().unwrap();
    (current.batches, current.bytes, current.background)
}

fn admitted(pipeline: &CandidatePipeline) -> AuthenticationTicket {
    let AuthenticationSubmission::Accepted(ticket) =
        pipeline.try_authenticate_owned(input()).unwrap()
    else {
        panic!("authentication was not admitted");
    };
    ticket
}

fn next_auth(receiver: &mpsc::Receiver<DriverMessage>) -> AuthenticationCommand {
    let DriverMessage::Authenticate(command) = receiver.try_recv().unwrap() else {
        panic!("wrong driver command");
    };
    command
}

// Same private constructor inputs as the real driver, but using the proof-side
// Ed25519 verifier for these channel-only tests. No native callback is claimed.
fn complete_authentication(pipeline: &CandidatePipeline, command: AuthenticationCommand) {
    let AuthenticationCommand {
        request,
        reply,
        permit,
    } = command;
    let checked = authenticate_batch_for_proof(
        domain().chain_id,
        request.request.raw_transactions.into_raw().unwrap(),
        pipeline.config.authentication,
    )
    .unwrap();
    let body =
        NovTransferBody::prepare(checked, request.request.policy, pipeline.config.plan).unwrap();
    assert!(reply
        .send(Ok(AuthenticatedBody {
            body: Box::new(body),
            owner: pipeline.identity.clone(),
            permit
        }))
        .is_ok());
}

#[test]
fn optional_authentication_reserves_current_slot_and_worst_case_bytes() {
    let mut config = cfg();
    config.max_batches = 1;
    let (single, receiver) = inert_pipeline(config.clone());
    assert!(matches!(
        single.try_authenticate_owned(input()).unwrap(),
        AuthenticationSubmission::Backpressured(_)
    ));
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert_eq!(usage(&single), (0, 0, 0));

    config.max_batches = 2;
    let bytes = input().reservation(&config).unwrap();
    let reserve = BatchRequest::retained_reservation(
        config.authentication.body_bytes.min(config.plan.body_bytes),
        &config,
    )
    .unwrap();
    config.max_retained_bytes = bytes + reserve - 1;
    let (short, _) = inert_pipeline(config.clone());
    assert!(matches!(
        short.try_authenticate_owned(input()).unwrap(),
        AuthenticationSubmission::Backpressured(_)
    ));
    config.max_retained_bytes += 1;
    let (pipeline, receiver) = inert_pipeline(config);
    let ticket = admitted(&pipeline);
    let command = next_auth(&receiver);
    assert_eq!(usage(&pipeline), (1, bytes, 1));
    let Submission::Accepted(normal) = pipeline.try_submit_owned(request(empty_root())).unwrap()
    else {
        panic!("authentication consumed the reserved current slot");
    };
    let ordinary = receiver.try_recv().unwrap();
    assert_eq!(usage(&pipeline).0, 2);
    drop((ticket, command, normal, ordinary));
    assert_eq!(usage(&pipeline), (0, 0, 0));
}

#[test]
fn authentication_and_existing_successor_share_one_background_permit() {
    let (pipeline, receiver) = inert_pipeline(cfg());
    let ticket = admitted(&pipeline);
    let command = next_auth(&receiver);
    assert!(matches!(
        pipeline
            .try_submit_background_owned(request(empty_root()))
            .unwrap(),
        Submission::Backpressured(_)
    ));
    drop((ticket, command));
    let Submission::Accepted(background) = pipeline
        .try_submit_background_owned(request(empty_root()))
        .unwrap()
    else {
        panic!("background permit not returned");
    };
    let command = receiver.try_recv().unwrap();
    assert!(matches!(
        pipeline.try_authenticate_owned(input()).unwrap(),
        AuthenticationSubmission::Backpressured(_)
    ));
    drop((background, command));
    assert_eq!(usage(&pipeline), (0, 0, 0));
}

#[test]
fn dropped_authentication_ticket_and_unconsumed_error_retain_live_job_budget() {
    let (pipeline, receiver) = inert_pipeline(cfg());
    let ticket = admitted(&pipeline);
    drop(ticket);
    assert_eq!(usage(&pipeline).0, 1);
    let command = next_auth(&receiver);
    assert_eq!(usage(&pipeline).2, 1);
    drop(command);
    assert_eq!(usage(&pipeline), (0, 0, 0));

    let mut ticket = admitted(&pipeline);
    assert!(ticket.try_take().unwrap().is_none());
    let command = next_auth(&receiver);
    assert!(command
        .reply
        .send(Err(anyhow::anyhow!("ordinary signature rejection")))
        .is_ok());
    drop(command);
    assert_eq!(usage(&pipeline).0, 1);
    assert!(matches!(
        pipeline.try_authenticate_owned(input()).unwrap(),
        AuthenticationSubmission::Backpressured(_)
    ));
    assert!(ticket.try_take().is_err());
    assert!(ticket.try_take().is_err());
    assert_eq!(usage(&pipeline), (0, 0, 0));
}

#[test]
fn completed_reply_body_bind_and_backpressure_keep_the_original_permit() {
    let (pipeline, receiver) = inert_pipeline(cfg());
    let mut ticket = admitted(&pipeline);
    let expected = usage(&pipeline);
    complete_authentication(&pipeline, next_auth(&receiver));
    assert_eq!(usage(&pipeline), expected); // Completed, not consumed.
    let body = ticket.try_take().unwrap().unwrap();
    assert!(ticket.try_take().is_err());
    assert_eq!(usage(&pipeline), expected); // Owned token still retained.
    let permit = Arc::as_ptr(&body.permit);
    let request = body.bind(context(empty_root()));
    assert_eq!(usage(&pipeline), expected);
    // Fill the one-slot inert coordinator channel, not the logical quota.
    let Submission::Accepted(normal) = pipeline
        .try_submit_owned(super::super::tests::request(empty_root()))
        .unwrap()
    else {
        panic!("ordinary request not accepted");
    };
    let with_normal = usage(&pipeline);
    let AuthenticatedSubmission::Backpressured(returned) =
        pipeline.try_submit_authenticated_owned(request).unwrap()
    else {
        panic!("full coordinator channel accepted a bound request");
    };
    assert_eq!(Arc::as_ptr(&returned.body.permit), permit);
    assert_eq!(usage(&pipeline), with_normal); // No second reservation.
    drop((normal, receiver.try_recv().unwrap()));
    assert_eq!(usage(&pipeline), expected);
    let AuthenticatedSubmission::Accepted(mut bound) =
        pipeline.try_submit_authenticated_owned(returned).unwrap()
    else {
        panic!("bound request retry failed");
    };
    let DriverMessage::Bind(command) = receiver.try_recv().unwrap() else {
        panic!("wrong bound command");
    };
    assert_eq!(Arc::as_ptr(&command.request.body.permit), permit);
    assert!(!command.background);
    assert_eq!(usage(&pipeline), expected);
    assert!(command
        .reply
        .send(Err(anyhow::anyhow!("test ends before state capture")))
        .is_ok());
    drop(command);
    assert_eq!(usage(&pipeline), expected);
    assert!(bound.try_take().is_err());
    assert_eq!(usage(&pipeline), (0, 0, 0));
}

#[test]
fn background_bind_keeps_exact_permit_at_full_quota_and_does_not_take_current_slot() {
    for lose_ticket in [false, true] {
        let mut config = cfg();
        config.max_batches = 2;
        let bytes = input().reservation(&config).unwrap();
        let ordinary_reserve = BatchRequest::retained_reservation(
            config.authentication.body_bytes.min(config.plan.body_bytes),
            &config,
        )
        .unwrap();
        config.max_retained_bytes = bytes + ordinary_reserve;
        let (pipeline, receiver) = inert_pipeline(config);
        let mut auth = admitted(&pipeline);
        complete_authentication(&pipeline, next_auth(&receiver));
        let body = auth.try_take().unwrap().unwrap();
        let permit = Arc::as_ptr(&body.permit);
        let body_pointer = body.body.as_ref() as *const _;
        let baseline = (1, bytes, 1);
        assert_eq!(usage(&pipeline), baseline);
        let bound_request = body.bind(context(empty_root()));
        let context_pointer = bound_request.context.as_ref() as *const _;

        // The normal request owns the remaining logical slot AND fills the
        // one-entry inert command queue. Failure must return the exact token.
        let Submission::Accepted(ordinary) =
            pipeline.try_submit_owned(request(empty_root())).unwrap()
        else {
            panic!("reserved ordinary slot was unavailable");
        };
        let full = usage(&pipeline);
        assert_eq!(full.0, 2);
        assert_eq!(full.2, 1);
        let AuthenticatedSubmission::Backpressured(returned) = pipeline
            .try_submit_authenticated_background_owned(bound_request)
            .unwrap()
        else {
            panic!("full command queue accepted background bind");
        };
        assert_eq!(Arc::as_ptr(&returned.body.permit), permit);
        assert_eq!(returned.body.body.as_ref() as *const _, body_pointer);
        assert_eq!(returned.context.as_ref() as *const _, context_pointer);
        assert_eq!(usage(&pipeline), full);
        let ordinary_command = receiver.try_recv().unwrap();

        // No queue occupancy remains, but BOTH quota slots are still owned.
        // Also hold the quota lock: trying reserve() again must not be needed.
        let quota_lock = pipeline.usage.lock().unwrap();
        let submission = pipeline.try_submit_authenticated_background_owned(returned);
        drop(quota_lock);
        let AuthenticatedSubmission::Accepted(bound) = submission.unwrap() else {
            panic!("bind attempted to reserve a second permit");
        };
        assert_eq!(Arc::as_ptr(bound.permit.as_ref().unwrap()), permit);
        assert_eq!(usage(&pipeline), full);
        let DriverMessage::Bind(command) = receiver.try_recv().unwrap() else {
            panic!("wrong background driver command");
        };
        assert!(command.background);
        assert!(command.request.body.permit.background);
        assert_eq!(Arc::as_ptr(&command.request.body.permit), permit);
        assert_eq!(command.request.body.body.as_ref() as *const _, body_pointer);
        assert_eq!(
            command.request.context.as_ref() as *const _,
            context_pointer
        );
        drop((ordinary, ordinary_command));
        assert_eq!(usage(&pipeline), baseline);

        assert!(matches!(
            pipeline.try_authenticate_owned(input()).unwrap(),
            AuthenticationSubmission::Backpressured(_)
        ));
        assert!(matches!(
            pipeline
                .try_submit_background_owned(request(empty_root()))
                .unwrap(),
            Submission::Backpressured(_)
        ));
        let Submission::Accepted(current) =
            pipeline.try_submit_owned(request(empty_root())).unwrap()
        else {
            panic!("background bind consumed the reserved current slot");
        };
        drop((current, receiver.try_recv().unwrap()));
        assert_eq!(usage(&pipeline), baseline);

        if lose_ticket {
            drop(bound);
            assert_eq!(usage(&pipeline), baseline); // Live owner command remains.
            assert!(command
                .reply
                .send(Err(anyhow::anyhow!("owner drained after lost ticket")))
                .is_err());
            drop(command);
        } else {
            let mut bound = bound;
            assert!(command
                .reply
                .send(Err(anyhow::anyhow!("owner-side bind rejection")))
                .is_ok());
            drop(command);
            assert_eq!(usage(&pipeline), baseline); // Unconsumed terminal reply.
            assert!(matches!(
                pipeline.try_authenticate_owned(input()).unwrap(),
                AuthenticationSubmission::Backpressured(_)
            ));
            assert!(bound.try_take().is_err());
            assert!(bound.try_take().is_err());
        }
        assert_eq!(usage(&pipeline), (0, 0, 0));
    }
}

#[test]
fn wrong_owner_and_domain_return_same_body_without_releasing_or_substituting_it() {
    let (mut pipeline, receiver) = inert_pipeline(cfg());
    let (foreign, foreign_receiver) = inert_pipeline(cfg());
    let mut ticket = admitted(&pipeline);
    complete_authentication(&pipeline, next_auth(&receiver));
    let body = ticket.try_take().unwrap().unwrap();
    let permit = Arc::as_ptr(&body.permit);
    let expected = usage(&pipeline);
    let rejected = foreign
        .try_submit_authenticated_owned(body.bind(context(empty_root())))
        .err()
        .unwrap();
    assert!(rejected.error.to_string().contains("another pipeline"));
    assert_eq!(Arc::as_ptr(&rejected.request.body.permit), permit);
    assert_eq!(usage(&pipeline), expected);
    assert_eq!(usage(&foreign), (0, 0, 0));
    assert!(matches!(
        foreign_receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    drop(rejected);
    assert_eq!(usage(&pipeline), (0, 0, 0));

    for mutate in [
        (|context: &mut BatchContext| context.chain_id += 1) as fn(&mut BatchContext),
        |context| context.genesis_config_commitment[0] ^= 1,
        |context| context.protocol_commitment[0] ^= 1,
    ] {
        let mut ticket = admitted(&pipeline);
        complete_authentication(&pipeline, next_auth(&receiver));
        let body = ticket.try_take().unwrap().unwrap();
        let expected = usage(&pipeline);
        let mut wrong = context(empty_root());
        mutate(&mut wrong);
        let rejected = pipeline
            .try_submit_authenticated_owned(body.bind(wrong))
            .err()
            .unwrap();
        assert!(rejected.error.to_string().contains("domain mismatch"));
        assert_eq!(usage(&pipeline), expected);
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        drop(rejected);
        assert_eq!(usage(&pipeline), (0, 0, 0));
    }
    let mut ticket = admitted(&pipeline);
    complete_authentication(&pipeline, next_auth(&receiver));
    let body = ticket.try_take().unwrap().unwrap();
    let expected = usage(&pipeline);
    pipeline.sender.take();
    let rejected = pipeline
        .try_submit_authenticated_owned(body.bind(context(empty_root())))
        .err()
        .unwrap();
    assert!(rejected.error.to_string().contains("pipeline closed"));
    assert_eq!(usage(&pipeline), expected);
    drop(rejected);
    assert_eq!(usage(&pipeline), (0, 0, 0));
}

#[test]
fn unaccepted_authentication_keeps_exact_allocation_on_busy_full_or_invalid_input() {
    let (pipeline, receiver) = inert_pipeline(cfg());
    let mut request = input();
    let owner = request.request.as_ref() as *const _;
    let bytes = request.request.raw_transactions[0].as_ptr();
    let lock = pipeline.usage.lock().unwrap();
    let AuthenticationSubmission::Backpressured(returned) =
        pipeline.try_authenticate_owned(request).unwrap()
    else {
        panic!("locked admission did not backpressure");
    };
    assert_eq!(returned.request.as_ref() as *const _, owner);
    assert_eq!(returned.request.raw_transactions[0].as_ptr(), bytes);
    drop(lock);
    request = returned;
    let Submission::Accepted(normal) = pipeline
        .try_submit_owned(super::super::tests::request(empty_root()))
        .unwrap()
    else {
        panic!("could not fill fixture channel");
    };
    let before = usage(&pipeline);
    let AuthenticationSubmission::Backpressured(mut request) =
        pipeline.try_authenticate_owned(request).unwrap()
    else {
        panic!("full channel did not backpressure");
    };
    assert_eq!(request.request.as_ref() as *const _, owner);
    assert_eq!(request.request.raw_transactions[0].as_ptr(), bytes);
    assert_eq!(usage(&pipeline), before);
    request.body_bytes = usize::MAX;
    let rejected = pipeline.try_authenticate_owned(request).err().unwrap();
    assert!(rejected.error.to_string().contains("input budget"));
    assert_eq!(rejected.request.request.as_ref() as *const _, owner);
    assert_eq!(usage(&pipeline), before);
    drop((normal, receiver.try_recv().unwrap()));
    assert_eq!(usage(&pipeline), (0, 0, 0));
}

#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);

impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

fn wait_some<T>(mut poll: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        ensure!(
            Instant::now() < deadline,
            "bounded authentication fixture completion expired"
        );
        if let Some(value) = poll()? {
            return Ok(value);
        }
        thread::yield_now();
    }
}

fn real_authenticate(pipeline: &CandidatePipeline, raw: Vec<Vec<u8>>) -> Result<AuthenticatedBody> {
    let mut request = AuthenticationRequest::new(raw, policy())?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        ensure!(
            Instant::now() < deadline,
            "bounded authentication fixture admission expired"
        );
        match pipeline
            .try_authenticate_owned(request)
            .map_err(|rejected| rejected.error)?
        {
            AuthenticationSubmission::Accepted(mut ticket) => {
                return wait_some(|| ticket.try_take())
            }
            AuthenticationSubmission::Backpressured(returned) => request = returned,
        }
        thread::yield_now();
    }
}

fn real_bind(
    pipeline: &CandidatePipeline,
    mut request: AuthenticatedRequest,
) -> Result<DurableBatch> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        ensure!(
            Instant::now() < deadline,
            "bounded binding fixture admission expired"
        );
        match pipeline
            .try_submit_authenticated_owned(request)
            .map_err(|rejected| rejected.error)?
        {
            AuthenticatedSubmission::Accepted(mut ticket) => {
                return wait_some(|| ticket.try_take())
            }
            AuthenticatedSubmission::Backpressured(returned) => request = returned,
        }
        thread::yield_now();
    }
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; real two-stage local pipeline and database reopen, not finality/TPS"]
fn real_parent_independent_authentication_rebinds_only_through_fresh_capture_and_recovers(
) -> Result<()> {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/pipeline-authentication-tests")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    std::fs::create_dir_all(&directory)?;
    let mut config = cfg();
    config.store.database = directory.join("state.rocksdb");
    config.store.library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .context("explicit trusted AOEM test library required")?
        .into();
    let raw = vec![signed(1, 10), signed(2, 20)];
    let store = CandidateStore::open(config.store.clone(), OpenMode::CreateNew)?;
    let mut parents = Vec::new();
    for balance in [0u128, 10_000] {
        let mut changes = fee_record_changes(&policy(), &FeeState::default())?;
        for seed in [1, 2] {
            changes.push(StateChange::Put {
                key: balance_key(&account(seed)),
                value: balance.to_le_bytes().to_vec(),
            });
        }
        let update = stage_state_update(&Memory::default(), empty_root(), &changes)?;
        store.install_unpublished_state(&update)?;
        parents.push((Memory(update.nodes().clone()), update.root()));
    }
    drop(store);
    let pipeline = CandidatePipeline::start(config.clone(), OpenMode::Existing)?;
    let mut bad = raw.clone();
    *bad[0].last_mut().unwrap() ^= 1;
    assert!(real_authenticate(&pipeline, bad).is_err());
    // A valid signature body can still fail program binding; this does not
    // poison the resident owner or grant permission to query another domain.
    let body = real_authenticate(&pipeline, raw.clone())?;
    let mut wrong = context(parents[0].1);
    wrong.business_program[0] ^= 1;
    assert!(real_bind(&pipeline, body.bind(wrong)).is_err());

    let mut references = Vec::new();
    for (memory, root) in &parents {
        let body = real_authenticate(&pipeline, raw.clone())?;
        assert_eq!(usage(&pipeline).0, 1);
        assert_eq!(usage(&pipeline).2, 1);
        // Context is deliberately selected only AFTER actual authentication.
        let context = context(*root);
        let expected = NovTransferPlan::compile(
            authenticate_batch_for_proof(domain().chain_id, raw.clone(), config.authentication)?,
            context,
            policy(),
            config.plan,
        )?
        .capture(memory, config.capture)?
        .execute_for_proof()?;
        let mut projected = Memory(memory.0.clone());
        projected
            .0
            .extend(expected.effects().update().nodes().clone());
        let expected_values = expected
            .effects()
            .plan()
            .declared_access()
            .iter()
            .map(|access| {
                Ok((
                    access.key.clone(),
                    read_state_value(&projected, expected.effects().update().root(), &access.key)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let expected = PreparedCandidate::from_executed(expected, config.store.packet_budget)?;
        let mut absence =
            wait_some(|| pipeline.try_recover_consensus_candidate(expected.candidate_id()))?;
        assert!(
            wait_some(|| absence.try_take())?.is_none(),
            "authentication persisted candidate content"
        );
        let mut query = wait_some(|| pipeline.try_read_value(*root, balance_key(&account(1))))?;
        assert_eq!(
            wait_some(|| query.try_take())?,
            read_state_value(memory, *root, &balance_key(&account(1)))?
        );
        let result = real_bind(&pipeline, body.bind(context))?;
        assert_eq!(result.packet.records(), expected.records());
        assert_eq!(result.persisted.candidate_id, expected.candidate_id());
        assert_eq!(result.persisted.state_root, expected.state_root());
        assert_eq!(
            result.persisted.statement_commitment,
            expected.statement_commitment()
        );
        assert!(result.observation.peak_callbacks > 0);
        references.push((expected, expected_values));
    }
    assert_ne!(references[0].0.state_root(), references[1].0.state_root());
    // An unbound completed token is retained content, not active work or an I/O
    // handle. Administrative drain must not wait for its external owner.
    let held_body = real_authenticate(&pipeline, raw.clone())?;
    let retained = pipeline.usage.clone();
    pipeline.shutdown()?;
    assert_eq!(retained.lock().unwrap().batches, 1);
    let reopened = CandidatePipeline::start(config.clone(), OpenMode::Existing)?;
    let old_permit = Arc::as_ptr(&held_body.permit);
    let rejected = reopened
        .try_submit_authenticated_owned(held_body.bind(context(parents[0].1)))
        .err()
        .context("closed owner's token was accepted by a new pipeline")?;
    assert!(rejected.error.to_string().contains("another pipeline"));
    assert_eq!(Arc::as_ptr(&rejected.request.body.permit), old_permit);
    assert_eq!(usage(&reopened), (0, 0, 0));
    drop(rejected);
    assert_eq!(retained.lock().unwrap().batches, 0);
    assert_eq!(retained.lock().unwrap().bytes, 0);
    reopened.shutdown()?;
    let store = CandidateStore::open(config.store, OpenMode::Existing)?;
    for (expected, expected_values) in &references {
        let recovered = store
            .recover(expected.candidate_id())?
            .context("bound candidate missing after reopen")?;
        assert!(expected.matches(&recovered));
        assert_eq!(recovered.raw_transactions(), raw);
        for (key, value) in expected_values {
            assert_eq!(
                &read_state_value(&store, expected.state_root(), key)?,
                value,
                "exact economic projection changed after database reopen"
            );
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; real background bind/retained reply/cold records, not finality or native preemption"]
fn real_background_bind_keeps_original_permit_through_shutdown_and_unconsumed_success() -> Result<()>
{
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/pipeline-background-bind-tests")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    std::fs::create_dir_all(&directory)?;
    let mut config = cfg();
    config.max_batches = 2;
    config.store.database = directory.join("state.rocksdb");
    config.store.library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .context("explicit trusted AOEM test library required")?
        .into();
    let raw = vec![signed(1, 10), signed(2, 20)];
    let mut changes = fee_record_changes(&policy(), &FeeState::default())?;
    for seed in [1, 2] {
        changes.push(StateChange::Put {
            key: balance_key(&account(seed)),
            value: 10_000u128.to_le_bytes().to_vec(),
        });
    }
    let update = stage_state_update(&Memory::default(), empty_root(), &changes)?;
    let memory = Memory(update.nodes().clone());
    let store = CandidateStore::open(config.store.clone(), OpenMode::CreateNew)?;
    store.install_unpublished_state(&update)?;
    drop(store);
    let context = context(update.root());
    let expected = NovTransferPlan::compile(
        authenticate_batch_for_proof(domain().chain_id, raw.clone(), config.authentication)?,
        context,
        policy(),
        config.plan,
    )?
    .capture(&memory, config.capture)?
    .execute_for_proof()?;
    let mut projected = Memory(memory.0.clone());
    projected
        .0
        .extend(expected.effects().update().nodes().clone());
    let expected_values = expected
        .effects()
        .plan()
        .declared_access()
        .iter()
        .map(|access| {
            Ok((
                access.key.clone(),
                read_state_value(&projected, expected.effects().update().root(), &access.key)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let expected = PreparedCandidate::from_executed(expected, config.store.packet_budget)?;

    let pipeline = CandidatePipeline::start(config.clone(), OpenMode::Existing)?;
    let body = real_authenticate(&pipeline, raw.clone())?;
    let original_permit = Arc::as_ptr(&body.permit);
    let reserved = usage(&pipeline);
    assert_eq!(reserved.0, 1);
    assert_eq!(reserved.2, 1);
    let AuthenticatedSubmission::Accepted(mut ticket) = pipeline
        .try_submit_authenticated_background_owned(body.bind(context))
        .map_err(|rejected| rejected.error)?
    else {
        anyhow::bail!("completed authentication could not reuse its background reservation");
    };
    assert_eq!(
        Arc::as_ptr(ticket.permit.as_ref().unwrap()),
        original_permit
    );
    assert_eq!(usage(&pipeline), reserved);
    assert!(matches!(
        pipeline.try_authenticate_owned(input()).unwrap(),
        AuthenticationSubmission::Backpressured(_)
    ));
    let retained = pipeline.usage.clone();
    // This drains the REAL driver/native/I/O owners. A successful shutdown
    // proves the terminal reply exists without polling/consuming that reply.
    pipeline.shutdown()?;
    {
        let usage = retained.lock().unwrap();
        assert_eq!((usage.batches, usage.bytes, usage.background), reserved);
    }
    let result = ticket
        .try_take()?
        .context("drained background job has no terminal reply")?;
    assert!(ticket.try_take().is_err());
    {
        let usage = retained.lock().unwrap();
        assert_eq!((usage.batches, usage.bytes, usage.background), (0, 0, 0));
    }
    assert_eq!(result.packet.records(), expected.records());
    assert_eq!(result.persisted.candidate_id, expected.candidate_id());
    assert_eq!(result.persisted.state_root, expected.state_root());
    assert_eq!(
        result.persisted.statement_commitment,
        expected.statement_commitment()
    );
    assert!(result.observation.peak_callbacks > 0);
    let store = CandidateStore::open(config.store, OpenMode::Existing)?;
    let recovered = store
        .recover(expected.candidate_id())?
        .context("background candidate missing after reopen")?;
    assert!(expected.matches(&recovered));
    assert_eq!(recovered.raw_transactions(), raw);
    for (key, value) in expected_values {
        assert_eq!(
            read_state_value(&store, expected.state_root(), &key)?,
            value,
            "background bind changed the cold economic projection"
        );
    }
    Ok(())
}
