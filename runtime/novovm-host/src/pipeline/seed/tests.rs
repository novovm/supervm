//! Cache accounting tests do not fabricate a persistence ACK. The native gate
//! additionally requires real second-height seed hits and cold economic parity.

use super::*;
use crate::business::direct_nov_fee::FeeState;
use crate::business::nov_transfer_batch::{
    balance_key, fee_record_changes, nonce_key, NovTransferPlan,
};
use crate::ingress::authentication::authenticate_transfer_v3;
use crate::ingress::batch::authenticate_batch_for_proof;
use crate::ingress::wire::{decode_transfer_v3, encode_transfer_v3, signing_message};
use crate::persistence::CandidateStore;
use crate::pipeline::compute::tests::{account, context, domain, policy, signed};
use crate::pipeline::tests::{config, inert_pipeline};
use crate::state::tree::{
    empty_root, read_state_value, stage_state_update, StateChange, StateNodeReader,
};
use ed25519_dalek::{Signer, SigningKey};
use std::collections::BTreeMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);

impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

fn initial_state() -> Result<crate::state::tree::StagedStateUpdate> {
    let mut changes = fee_record_changes(&policy(), &FeeState::default())?;
    for seed in [1, 2] {
        changes.push(StateChange::Put {
            key: balance_key(&account(seed)),
            value: 10_000u128.to_le_bytes().to_vec(),
        });
    }
    stage_state_update(&Memory::default(), empty_root(), &changes)
}

fn raw_at(nonce: u64) -> Result<Vec<Vec<u8>>> {
    [(1, 10), (2, 20)]
        .into_iter()
        .map(|(seed, amount)| {
            let mut tx = decode_transfer_v3(&signed(seed, amount + u128::from(nonce)), 4096)?;
            tx.nonce = nonce;
            let signer = SigningKey::from_bytes(&[seed; 32]);
            let signature = signer.sign(&signing_message(&tx)?);
            tx.signature = signer.verifying_key().to_bytes().to_vec();
            tx.signature.extend_from_slice(&signature.to_bytes());
            encode_transfer_v3(&tx)
        })
        .collect()
}

fn executed(
    config: &PipelineConfig,
    memory: &Memory,
    context: BatchContext,
    raw: Vec<Vec<u8>>,
) -> Result<crate::business::nov_transfer_batch::ExecutedNovBatch> {
    NovTransferPlan::compile(
        authenticate_batch_for_proof(domain().chain_id, raw, config.authentication)?,
        context,
        policy(),
        config.plan,
    )?
    .capture(memory, config.capture)?
    .execute_for_proof()
}

fn seed_fixture() -> Result<(PipelineConfig, PreparedCandidate, PostStateSeed)> {
    let config = config("unused-seed-accounting-fixture".into());
    let initial = initial_state()?;
    let memory = Memory(initial.nodes().clone());
    let output = executed(&config, &memory, context(initial.root()), raw_at(0)?)?;
    let (packet, seed) = PreparedCandidate::from_executed_with_seed(
        output,
        config.store.packet_budget,
        config.capture,
    )?;
    Ok((
        config,
        packet,
        seed.context("small fixture seed exceeded original capture bounds")?,
    ))
}

fn child_context(parent: &PreparedCandidate) -> BatchContext {
    let mut child = *parent.context();
    child.parent_height = parent.context().height;
    // A nonzero fixture block identity, not a consensus-certified block hash.
    child.parent_block_hash = parent.candidate_id();
    child.parent_state_root = parent.state_root();
    child.parent_receipt_root = parent.receipt_batch_commitment();
    child.parent_state_version += u64::try_from(parent.transaction_count()).unwrap();
    child.height += 1;
    child.slot += 1;
    child.timestamp_unix_ms += 1;
    child
}

fn maximum_reservation(config: &PipelineConfig) -> Result<usize> {
    BatchRequest::retained_reservation(
        config.authentication.body_bytes.min(config.plan.body_bytes),
        config,
    )
}

fn usage(usage: &Arc<Mutex<Usage>>) -> (usize, usize, usize) {
    let usage = usage.lock().unwrap();
    (usage.batches, usage.bytes, usage.background)
}

#[test]
fn seed_byte_lease_inclusive_bound_preserves_live_batch_accounting() -> Result<()> {
    let (mut config, packet, seed) = seed_fixture()?;
    let reserve = maximum_reservation(&config)?;
    let bytes = seed.retained_bytes();
    let occupied = reserve + 17;
    config.max_batches = 2;
    config.max_retained_bytes = bytes + (occupied + reserve).max(2 * reserve);
    let accounting = Arc::new(Mutex::new(Usage {
        batches: 1,
        bytes: occupied,
        background: 1,
    }));
    let cache = SeedCache::try_install(seed, &packet, &config, &accounting)?
        .context("inclusive byte boundary unexpectedly refused cache")?;
    assert_eq!(usage(&accounting), (1, occupied + bytes, 1));
    drop(cache);
    assert_eq!(usage(&accounting), (1, occupied, 1));

    let (_, _, seed) = seed_fixture()?;
    config.max_retained_bytes -= 1;
    assert!(SeedCache::try_install(seed, &packet, &config, &accounting)?.is_none());
    assert_eq!(usage(&accounting), (1, occupied, 1));
    Ok(())
}

#[test]
fn seed_leaves_maximum_background_and_ordinary_admission_without_using_slots() -> Result<()> {
    let (mut config, packet, seed) = seed_fixture()?;
    let bytes = seed.retained_bytes();
    let reserve = maximum_reservation(&config)?;
    config.max_batches = 2;
    config.max_retained_bytes = 2 * reserve + bytes;
    let (pipeline, receiver) = inert_pipeline(config);
    let cache = SeedCache::try_install(seed, &packet, &pipeline.config, &pipeline.usage)?
        .context("exact two-batch net space refused cache")?;
    assert_eq!(usage(&pipeline.usage), (0, bytes, 0));
    let maximum_request = || {
        // Admission-only raw bytes, deliberately not claimed to be signed.
        BatchRequest::new(vec![vec![7; 4096]; 16], child_context(&packet), policy())
    };
    let Submission::Accepted(background) = pipeline
        .try_submit_background_owned(maximum_request()?)
        .map_err(|rejected| rejected.error)?
    else {
        anyhow::bail!("cache prevented maximum background admission");
    };
    let background_command = receiver.try_recv()?;
    let Submission::Accepted(normal) = pipeline
        .try_submit_owned(maximum_request()?)
        .map_err(|rejected| rejected.error)?
    else {
        anyhow::bail!("cache consumed the reserved ordinary admission");
    };
    let normal_command = receiver.try_recv()?;
    assert_eq!(usage(&pipeline.usage), (2, 2 * reserve + bytes, 1));
    drop((background, background_command, normal, normal_command));
    assert_eq!(usage(&pipeline.usage), (0, bytes, 0));
    drop(cache);
    assert_eq!(usage(&pipeline.usage), (0, 0, 0));

    let (_, _, seed) = seed_fixture()?;
    let mut short = pipeline.config.clone();
    short.max_retained_bytes -= 1;
    assert!(SeedCache::try_install(seed, &packet, &short, &pipeline.usage)?.is_none());
    assert_eq!(usage(&pipeline.usage), (0, 0, 0));
    Ok(())
}

#[test]
fn seed_wrong_context_never_supplies_bytes_or_parent_authority() -> Result<()> {
    let (mut config, packet, seed) = seed_fixture()?;
    config.max_retained_bytes = 256 * 1024 * 1024;
    let accounting = Arc::new(Mutex::new(Usage::default()));
    let cache = SeedCache::try_install(seed, &packet, &config, &accounting)?
        .context("empty fixture accounting should admit cache")?;
    let child = child_context(&packet);
    assert_eq!(packet.transaction_count(), 2);
    assert!(cache.for_context(&child).is_some());
    let mut block_count_version = child;
    block_count_version.parent_state_version = packet.context().parent_state_version + 1;
    assert!(
        cache.for_context(&block_count_version).is_none(),
        "state version counts actual transactions, not one per block"
    );
    let mutations: &[fn(&mut BatchContext)] = &[
        |c| c.chain_id += 1,
        |c| c.genesis_config_commitment[0] ^= 1,
        |c| c.protocol_commitment[0] ^= 1,
        |c| c.business_program[0] ^= 1,
        |c| c.semantic_version += 1,
        |c| c.effect_contract[0] ^= 1,
        |c| c.receipt_codec[0] ^= 1,
        |c| c.parent_height += 1,
        |c| c.height += 1,
        |c| c.parent_state_version += 1,
        |c| c.parent_state_root[0] ^= 1,
        |c| c.parent_receipt_root[0] ^= 1,
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let mut wrong = child;
        mutate(&mut wrong);
        assert!(
            cache.for_context(&wrong).is_none(),
            "wrong context field {index}"
        );
    }
    let mut other_claim = child;
    other_claim.parent_block_hash[0] ^= 1;
    other_claim.slot += 1;
    other_claim.timestamp_unix_ms += 1;
    assert!(cache.for_context(&other_claim).is_some());
    // These bytes do not certify the changed block/round/time. The caller's
    // independently authorized context and new capture still govern execution.
    drop(cache);
    assert_eq!(usage(&accounting), (0, 0, 0));
    Ok(())
}

#[test]
fn seed_refuses_wrong_postroot_exceeded_bounds_and_busy_accounting() -> Result<()> {
    let (config, packet, seed) = seed_fixture()?;
    let accounting = Arc::new(Mutex::new(Usage::default()));
    let held = accounting.lock().unwrap();
    assert!(SeedCache::try_install(seed, &packet, &config, &accounting)?.is_none());
    drop(held);
    assert_eq!(usage(&accounting), (0, 0, 0));

    let (_, _, seed) = seed_fixture()?;
    let mut short = config.clone();
    short.capture.bytes = seed.retained_bytes() - 1;
    assert!(SeedCache::try_install(seed, &packet, &short, &accounting).is_err());
    let (_, _, seed) = seed_fixture()?;
    short = config.clone();
    short.capture.nodes = seed.node_count() - 1;
    assert!(SeedCache::try_install(seed, &packet, &short, &accounting).is_err());

    let initial = initial_state()?;
    let memory = Memory(initial.nodes().clone());
    let other = executed(
        &config,
        &memory,
        context(initial.root()),
        vec![signed(1, 11)],
    )?;
    let other = PreparedCandidate::from_executed(other, config.store.packet_budget)?;
    let (_, _, seed) = seed_fixture()?;
    assert!(SeedCache::try_install(seed, &other, &config, &accounting).is_err());
    assert_eq!(usage(&accounting), (0, 0, 0));
    Ok(())
}

#[test]
fn optional_seed_declines_unrepresentable_maximum_and_poisoned_accounting() -> Result<()> {
    let (mut config, packet, seed) = seed_fixture()?;
    config.authentication.body_bytes = usize::MAX;
    config.plan.body_bytes = usize::MAX;
    let accounting = Arc::new(Mutex::new(Usage::default()));
    assert!(SeedCache::try_install(seed, &packet, &config, &accounting)?.is_none());
    assert_eq!(usage(&accounting), (0, 0, 0));
    let (config, packet, seed) = seed_fixture()?;
    let poison = accounting.clone();
    assert!(thread::spawn(move || {
        let _guard = poison.lock().unwrap();
        panic!("test-only poisoned cache accounting");
    })
    .join()
    .is_err());
    assert!(SeedCache::try_install(seed, &packet, &config, &accounting)?.is_none());
    let retained = accounting.lock().err().expect("poisoned lock").into_inner();
    assert_eq!(
        (retained.batches, retained.bytes, retained.background),
        (0, 0, 0)
    );
    Ok(())
}

fn wait_some<T>(mut poll: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        ensure!(
            Instant::now() < deadline,
            "bounded seed fixture completion expired"
        );
        if let Some(value) = poll()? {
            return Ok(value);
        }
        thread::yield_now();
    }
}

fn submit(pipeline: &CandidatePipeline, mut request: BatchRequest) -> Result<DurableBatch> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        ensure!(
            Instant::now() < deadline,
            "bounded seed fixture admission expired"
        );
        match pipeline
            .try_submit_owned(request)
            .map_err(|rejected| rejected.error)?
        {
            Submission::Accepted(mut ticket) => return wait_some(|| ticket.try_take()),
            Submission::Backpressured(returned) => request = returned,
        }
        thread::yield_now();
    }
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; real two-height seed hits, fallback and cold economics, not finality/TPS"]
fn real_two_height_seed_matches_full_capture_and_reopens_with_original_budgets() -> Result<()> {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/pipeline-seed-tests")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    std::fs::create_dir_all(&directory)?;
    let mut observations = Vec::new();
    let mut previous_records = None;
    for (label, mib, unrepresentable_max, expect_seed) in [
        ("128", 128, false, false),
        ("256", 256, false, true),
        ("max-body", 256, true, false),
    ] {
        // Existing pipeline test budget and existing controller byte budget.
        // Do not tune packet/capture/IO bounds to force a cache hit.
        let mut config = config(directory.join(format!("state-{label}.rocksdb")));
        config.max_batches = 2;
        config.max_retained_bytes = mib * 1024 * 1024;
        if unrepresentable_max {
            config.authentication.body_bytes = usize::MAX;
            config.plan.body_bytes = usize::MAX;
        }
        config.store.library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .context("explicit trusted AOEM test library required")?
            .into();
        let initial = initial_state()?;
        let mut memory = Memory(initial.nodes().clone());
        let store = CandidateStore::open(config.store.clone(), OpenMode::CreateNew)?;
        store.install_unpublished_state(&initial)?;
        drop(store);
        let pipeline = CandidatePipeline::start(config.clone(), OpenMode::Existing)?;
        let accounting = pipeline.usage.clone();
        let mut invalid = raw_at(0)?;
        *invalid[0].last_mut().unwrap() ^= 1;
        assert!(submit(
            &pipeline,
            BatchRequest::new(invalid, context(initial.root()), policy())?
        )
        .is_err());
        wait_some(|| Ok((usage(&accounting) == (0, 0, 0)).then_some(())))?;

        let mut next = context(initial.root());
        let mut references = Vec::new();
        for nonce in 0..2 {
            let raw = raw_at(nonce)?;
            // This oracle captures afresh from the complete immutable parent,
            // never consuming the pipeline seed or native output as its result.
            let expected = executed(&config, &memory, next, raw.clone())?;
            memory.0.extend(expected.effects().update().nodes().clone());
            let expected_values = expected
                .effects()
                .plan()
                .declared_access()
                .iter()
                .map(|access| {
                    Ok((
                        access.key.clone(),
                        read_state_value(&memory, expected.effects().update().root(), &access.key)?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            let expected = PreparedCandidate::from_executed(expected, config.store.packet_budget)?;
            let result = submit(&pipeline, BatchRequest::new(raw.clone(), next, policy())?)?;
            assert_eq!(result.packet.records(), expected.records());
            assert_eq!(result.persisted.candidate_id, expected.candidate_id());
            assert_eq!(result.persisted.state_root, expected.state_root());
            assert_eq!(
                result.persisted.statement_commitment,
                expected.statement_commitment()
            );
            assert_eq!(result.persisted.document_digest, expected.document_digest());
            assert!(result.observation.peak_callbacks > 0);
            if nonce == 0 {
                assert_eq!(result.capture.seed_nodes, 0);
                assert!(result.capture.storage_requests > 0);
            } else {
                assert_eq!(
                    result.capture.seed_nodes > 0,
                    expect_seed,
                    "second-height seed path was not exercised under {mib} MiB"
                );
                observations.push(result.capture);
            }
            next = child_context(&result.packet);
            references.push((expected, expected_values, raw));
        }
        let records = references
            .iter()
            .map(|(packet, _, _)| packet.records().clone())
            .collect::<Vec<_>>();
        if let Some(previous) = &previous_records {
            assert_eq!(&records, previous, "cache changed candidate byte identity");
        }
        previous_records = Some(records);
        wait_some(|| {
            let (batches, bytes, background) = usage(&accounting);
            Ok((batches == 0 && background == 0 && (bytes > 0) == expect_seed).then_some(()))
        })?;
        pipeline.shutdown()?;
        assert_eq!(
            usage(&accounting),
            (0, 0, 0),
            "shutdown retained cache bytes"
        );
        let store = CandidateStore::open(config.store, OpenMode::Existing)?;
        for (expected, values, raw) in &references {
            let recovered = store
                .recover(expected.candidate_id())?
                .context("candidate missing after cold reopen")?;
            assert!(expected.matches(&recovered));
            assert_eq!(recovered.raw_transactions(), *raw);
            for (key, value) in values {
                assert_eq!(
                    &read_state_value(&store, expected.state_root(), key)?,
                    value,
                    "cold exact economic projection changed"
                );
            }
        }
        let root = references.last().unwrap().0.state_root();
        for (seed, balance) in [(1, 9889u128), (2, 9869), (9, 62)] {
            assert_eq!(
                read_state_value(&store, root, &balance_key(&account(seed)))?,
                Some(balance.to_le_bytes().to_vec())
            );
        }
        for seed in [1, 2] {
            let identity = authenticate_transfer_v3(&signed(seed, 10), domain().chain_id, 4096)?
                .nonce_identity();
            assert_eq!(
                read_state_value(&store, root, &nonce_key(&identity))?,
                Some(2u64.to_le_bytes().to_vec())
            );
        }
    }
    assert!(
        observations[1].storage_nodes < observations[0].storage_nodes,
        "seed hits did not reduce actual second-height storage-node requests"
    );
    Ok(())
}
