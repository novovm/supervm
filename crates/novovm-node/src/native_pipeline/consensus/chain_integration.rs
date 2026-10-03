//! Actual resident AOEM execution and four separate local ledgers. The fixture
//! transports encoded messages in-process; elapsed test time is NOT block time.
use super::*;
use crate::native_pipeline::business::nov_transfer_batch::nonce_key;
use crate::native_pipeline::consensus::chain::ChainRecord;
use crate::native_pipeline::consensus::{transport, ArchiveBlock, ArchiveRead};
use crate::native_pipeline::ingress::authentication::authenticate_transfer_v3;
use crate::native_pipeline::persistence::packet::marker_key;
use novovm_exec::resident::{StorageSession, StorageWrite};

fn raw_at(height: u64) -> Result<Vec<Vec<u8>>> {
    [(1, if height == 2 { 2_000_000 } else { 100 }), (3, 50)]
        .into_iter()
        .map(|(seed, amount)| {
            let signer = SigningKey::from_bytes(&[seed; 32]);
            let mut tx = TransferV3 {
                chain_id: CHAIN,
                from: account(seed).as_bytes().to_vec(),
                to: account(2).as_bytes().to_vec(),
                asset: "NOV".into(),
                amount,
                nonce: height - 1,
                fee_policy: FeePolicy {
                    pay_asset: "NOV".into(),
                    max_pay_amount: 0,
                    slippage_bps: 0,
                },
                signature: Vec::new(),
            };
            let signature = signer.sign(&signing_message(&tx)?);
            tx.signature = signer.verifying_key().to_bytes().to_vec();
            tx.signature.extend_from_slice(&signature.to_bytes());
            encode_transfer_v3(&tx)
        })
        .collect()
}

fn execute_height(
    services: &Services,
    context: ConsensusContext,
    parent: ParentPoint,
) -> Result<Vec<DurableCandidate>> {
    let mut execution = batch_context(parent.state_root);
    execution.height = context.height;
    execution.parent_height = parent.height;
    execution.parent_block_hash = parent.block_hash;
    execution.parent_state_root = parent.state_root;
    execution.parent_receipt_root = parent.receipt_batch_commitment;
    execution.parent_state_version = parent.state_version;
    execution.slot = context.height - 1;
    execution.timestamp_unix_ms += context.height;
    let mut tickets = Vec::new();
    for pipeline in &services.0 {
        let request = BatchRequest::new(raw_at(context.height)?, execution, policy())?;
        let Submission::Accepted(ticket) = pipeline.try_submit(request)? else {
            bail!("empty resident pipeline rejected fixture admission");
        };
        tickets.push(Some(ticket));
    }
    let mut candidates = vec![None; tickets.len()];
    let deadline = Instant::now() + DEADLINE;
    while tickets.iter().any(Option::is_some) {
        ensure!(
            Instant::now() < deadline,
            "consecutive candidate execution timed out"
        );
        for (index, ticket) in tickets.iter_mut().enumerate() {
            let Some(pending) = ticket.as_mut() else {
                continue;
            };
            if let Some(done) = pending.try_take()? {
                ensure!(
                    done.observation.peak_callbacks > 0 && !done.persisted.already_present,
                    "height did not execute new real AOEM work"
                );
                candidates[index] = Some(done.candidate().clone());
                ticket.take();
            }
        }
        std::thread::yield_now();
    }
    let candidates: Vec<_> = candidates.into_iter().map(Option::unwrap).collect();
    for candidate in &candidates {
        ensure!(
            candidate.packet().records() == candidates[0].packet().records(),
            "independent execution at height {} disagrees",
            context.height
        );
    }
    Ok(candidates)
}

struct ReplayExpected {
    records: Vec<ChainRecord>,
    proposal: Vec<u8>,
    certificates: Vec<Vec<u8>>,
}

fn finish_archive_read(
    read: &mut ArchiveRead,
    pipeline: &CandidatePipeline,
) -> Result<Option<ArchiveBlock>> {
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(Instant::now() < deadline, "archive replay timed out");
        if let Some(done) = read.poll(pipeline)? {
            return Ok(done);
        }
        std::thread::yield_now();
    }
}

fn check_archive_replay(
    pipeline: &CandidatePipeline,
    context: ConsensusContext,
    ceiling: ParentPoint,
    set: Arc<ValidatorSet>,
    expected: &ReplayExpected,
    local: usize,
) -> Result<()> {
    let record = &expected.records[local];
    let mut read = ArchiveRead::new(record.point().height, ceiling, context, set)?;
    let block =
        finish_archive_read(&mut read, pipeline)?.context("decided historical block absent")?;
    ensure!(
        read.poll(pipeline).is_err(),
        "finished archive read restarted"
    );
    ensure!(
        block.context() == record.context()
            && block.parent() == record.parent()
            && block.point() == record.point()
            && block.stored().candidate_id() == record.candidate_id()
            && block.stored().state_root() == record.point().state_root
            && block.stored().receipt_batch_commitment() == record.point().receipt_batch_commitment
            && wire::encode_proposal(block.proposal())? == expected.proposal
            && wire::encode_quorum(block.certificate())? == expected.certificates[local],
        "archive replay changed local decided content or exact QC subset"
    );
    let mut expected_context = batch_context(record.parent().state_root);
    expected_context.height = record.point().height;
    expected_context.parent_height = record.parent().height;
    expected_context.parent_block_hash = record.parent().block_hash;
    expected_context.parent_receipt_root = record.parent().receipt_batch_commitment;
    expected_context.parent_state_version = record.parent().state_version;
    expected_context.slot = record.point().height - 1;
    expected_context.timestamp_unix_ms += record.point().height;
    let expected_raw = raw_at(record.point().height)?;
    ensure!(
        block.stored().context() == &expected_context
            && block.stored().raw_transactions() == expected_raw,
        "archive replay changed original body or exact execution parent"
    );
    // Full-body work deliberately happens AFTER ArchiveRead::poll, in this
    // fixture's assembly role. No transport work is added to the real reader.
    let limits = transport::DecodeLimits {
        transactions: 8,
        transaction_bytes: 1024,
        body_bytes: 8192,
        message_bytes: 16_384,
    };
    ensure!(
        transport::body_id(
            block.stored().context(),
            block.stored().raw_transactions(),
            limits,
        )? == transport::body_id(&expected_context, &expected_raw, limits)?,
        "archive replay changed network body identity"
    );
    Ok(())
}

fn state_only(journal: &mut ValidatorJournal, pipeline: &CandidatePipeline) -> Result<()> {
    let old_context = journal.context();
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(Instant::now() < deadline, "height advancement timed out");
        match journal.poll(pipeline)? {
            None => ensure!(
                journal.context() == old_context,
                "new signing height escaped before durable completion"
            ),
            Some(message) => {
                ensure!(
                    message.is_none() && !journal.is_pending(),
                    "advance emitted a vote"
                );
                return Ok(());
            }
        }
        std::thread::yield_now();
    }
}

fn submit_without_ack(journal: &mut ValidatorJournal, pipeline: &CandidatePipeline) -> Result<()> {
    let deadline = Instant::now() + DEADLINE;
    let old_context = journal.context();
    let old_head = journal.head();
    let old_decision = journal.decided();
    let old_message = message_bytes(journal.last_durable_message())?;
    loop {
        ensure!(Instant::now() < deadline, "lost-ACK enqueue timed out");
        if journal.enqueue_pending(pipeline)? {
            break;
        }
        std::thread::yield_now();
    }
    ensure!(
        journal.is_pending()
            && journal.context() == old_context
            && journal.head() == old_head
            && journal.decided() == old_decision
            && message_bytes(journal.last_durable_message())? == old_message,
        "unacknowledged write changed caller-visible authority"
    );
    Ok(())
}

fn value(pipeline: &CandidatePipeline, root: Hash, key: Vec<u8>) -> Result<Vec<u8>> {
    let deadline = Instant::now() + DEADLINE;
    let mut ticket = loop {
        ensure!(Instant::now() < deadline, "query admission timed out");
        if let Some(ticket) = pipeline.try_read_value(root, key.clone())? {
            break ticket;
        }
        std::thread::yield_now();
    };
    loop {
        ensure!(Instant::now() < deadline, "query completion timed out");
        if let Some(reply) = ticket.try_take()? {
            return reply.context("expected state value missing");
        }
        std::thread::yield_now();
    }
}

fn reject_partial_rollback(
    path: &Path,
    genesis: ConsensusContext,
    anchor: ParentPoint,
    set: Arc<ValidatorSet>,
    old: MetadataSnapshot,
) -> Result<()> {
    let validator = Validator::new(validator_key(3).verifying_key().to_bytes(), 1)?.id();
    let keys = {
        let store = CandidateStore::open(store_config(path)?, OpenMode::Existing)?;
        vec![
            store.scoped_key(b"m/consensus/v1/chain/head"),
            store.scoped_key(&[b"m/consensus/v1/state/".as_slice(), &validator].concat()),
        ]
    };
    let correct = {
        let config = store_config(path)?;
        let mut storage = StorageSession::open(&config.library, &config.database, config.storage)?;
        let correct = storage.multi_get(&keys)?;
        let writes = keys
            .iter()
            .zip(&old.values)
            .map(|(key, value)| StorageWrite::Put {
                key: key.clone(),
                value: value.clone().unwrap(),
            })
            .collect::<Vec<_>>();
        storage.atomic_write_batch(&writes)?;
        correct
    };
    let mut services = Services::default();
    services.0.push(CandidatePipeline::start(
        pipeline_config(path)?,
        OpenMode::Existing,
    )?);
    let pipeline = &services.0[0];
    let result = open_journal(
        ValidatorJournal::open(pipeline, genesis, anchor, set, validator_key(3))?,
        pipeline,
    );
    let error = match result {
        Ok(_) => bail!("partial rollback reopened signer despite retained decided successor"),
        Err(error) => error,
    };
    ensure!(
        format!("{error:#}").contains("decided successor exists beyond declared head"),
        "wrong partial rollback rejection: {error:#}"
    );
    let read = read_metadata(
        pipeline,
        vec![MetaKey::ChainHead, MetaKey::ConsensusState(validator)],
    )?;
    ensure!(read == old, "failed recovery repaired metadata");
    services.shutdown()?;
    // Restore only our deliberate injection in this throwaway fixture, so the
    // independent missing-prefix case below still begins from a valid ledger.
    let config = store_config(path)?;
    let mut storage = StorageSession::open(&config.library, &config.database, config.storage)?;
    storage.atomic_write_batch(
        &keys
            .into_iter()
            .zip(correct)
            .map(|(key, value)| StorageWrite::Put {
                key,
                value: value.unwrap(),
            })
            .collect::<Vec<_>>(),
    )?;
    Ok(())
}

#[test]
#[ignore = "requires real AOEM; four local stores, not network or production finality"]
fn real_three_heights_different_qc_subsets_lost_ack_recovery_and_corruption_rejection() -> Result<()>
{
    let _ = library()?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/consensus-tests")
        .join(format!(
            "chain-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    let paths: Vec<_> = (0..4)
        .map(|i| directory.join(format!("validator-{i}")))
        .collect();
    let set = Arc::new(ValidatorSet::new(
        CHAIN,
        1,
        1,
        (0..4)
            .map(|i| Validator::new(validator_key(i).verifying_key().to_bytes(), 1))
            .collect::<Result<_>>()?,
    )?);
    let genesis = ConsensusContext {
        chain_id: CHAIN,
        genesis_config_commitment: GENESIS,
        protocol_commitment: PROTOCOL,
        epoch: 1,
        validator_set_hash: set.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    };
    let mut services = Services::default();
    let mut roots = Vec::new();
    for path in &paths {
        roots.push(initialize(path)?);
        services.0.push(CandidatePipeline::start(
            pipeline_config(path)?,
            OpenMode::Existing,
        )?);
    }
    ensure!(
        roots.iter().all(|r| *r == roots[0]),
        "genesis roots disagree"
    );
    let anchor = ParentPoint {
        height: 0,
        block_hash: [0; 32],
        state_root: roots[0],
        receipt_batch_commitment: empty_root(),
        state_version: 0,
        decision_hash: [0; 32],
    };
    let mut journals = Vec::new();
    for (i, pipeline) in services.0.iter().enumerate() {
        journals.push(Some(open_journal(
            ValidatorJournal::open(pipeline, genesis, anchor, set.clone(), validator_key(i))?,
            pipeline,
        )?));
    }
    // Different signer key on the SAME DB: its own absent snapshot never changes.
    // Only the head guard can reject this staged vote after validator 0 publishes.
    let mut stale = open_journal(
        ValidatorJournal::open(
            &services.0[0],
            genesis,
            anchor,
            set.clone(),
            validator_key(1),
        )?,
        &services.0[0],
    )?;
    stale.timeout(0, TimeoutStep::Propose)?;
    let mut previous_candidates: Option<Vec<DurableCandidate>> = None;
    let mut expected_head = None;
    let mut previous_revisions = [0u64; 4];
    let mut rollback_snapshot = None;
    let mut records = Vec::new();
    let mut replay_expected = Vec::new();
    for height in 1..=3 {
        let context = journals[0].as_ref().unwrap().context();
        let parent = journals[0].as_ref().unwrap().parent();
        ensure!(context.height == height, "wrong successor height");
        for journal in journals.iter().flatten() {
            ensure!(
                journal.context() == context
                    && journal.parent() == parent
                    && journal.head() == expected_head
                    && journal.decided().is_none(),
                "nodes derived different next-height authority"
            );
            ensure!(
                ValidatorJournal::open(
                    &services.0[0],
                    context,
                    parent,
                    set.clone(),
                    validator_key(0)
                )
                .is_err()
                    || height == 1,
                "arbitrary checkpoint bypassed genesis-anchored recovery"
            );
        }
        if let Some(old) = &previous_candidates {
            for (i, journal) in journals.iter_mut().enumerate() {
                let journal = journal.as_mut().unwrap();
                ensure!(
                    journal.propose(&old[i], None).is_err() && !journal.is_pending(),
                    "old-height execution acquired new-height signing permission"
                );
            }
        }
        let candidates = execute_height(&services, context, parent)?;
        let block_hash =
            BlockStatement::from_executed(candidates[0].packet(), context, &set, &parent)?.hash();
        let leader = (0..4)
            .find(|&i| {
                Validator::new(validator_key(i).verifying_key().to_bytes(), 1)
                    .unwrap()
                    .id()
                    == set.leader(height, 0).unwrap()
            })
            .unwrap();
        let leader_journal = journals[leader].as_mut().unwrap();
        leader_journal.propose(&candidates[leader], None)?;
        let DurableMessage::Proposal(proposal) = complete(leader_journal, &services.0[leader])?
        else {
            bail!("missing proposal");
        };
        let proposal = wire::decode_proposal(&wire::encode_proposal(&proposal)?)?.verify(&set)?;
        let mut prevotes = Vec::new();
        for (i, journal) in journals.iter_mut().enumerate() {
            let journal = journal.as_mut().unwrap();
            journal.accept_proposal(&proposal, &candidates[i], None)?;
            prevotes.push(wire_vote(
                complete(journal, &services.0[i])?,
                &set,
                Phase::Prevote,
                block_hash,
            )?);
        }
        let prevote_qc = Quorum::from_votes(&set, prevotes[..3].to_vec())?.verify(&set)?;
        let mut precommits = Vec::new();
        for (i, journal) in journals.iter_mut().enumerate() {
            let journal = journal.as_mut().unwrap();
            journal.observe_prevotes(&prevote_qc, Some((&proposal, &candidates[i])))?;
            precommits.push(wire_vote(
                complete(journal, &services.0[i])?,
                &set,
                Phase::Precommit,
                block_hash,
            )?);
        }
        // Every node uses a DIFFERENT legal 3/4 subset for the SAME value.
        let mut certificate_bytes = Vec::new();
        records.clear();
        for i in 0..4 {
            let quorum = Quorum::from_votes(
                &set,
                precommits
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, v)| v.clone())
                    .collect(),
            )?;
            let bytes = wire::encode_quorum(&quorum)?;
            ensure!(
                !certificate_bytes.contains(&bytes),
                "fixture reused QC subset"
            );
            certificate_bytes.push(bytes.clone());
            let verified = wire::decode_quorum(&bytes)?.verify(&set)?;
            let journal = journals[i].as_mut().unwrap();
            journal.observe_decision(&proposal, &candidates[i], &verified)?;
            ensure!(
                journal.head() == expected_head && journal.decided().is_none(),
                "head was published before durable ACK"
            );
            if height == 2 && i == 0 {
                submit_without_ack(journal, &services.0[i])?;
                journals[i].take();
                // FIFO read barrier: observe storage, never infer commit from enqueue.
                let read = read_metadata(
                    &services.0[i],
                    vec![MetaKey::ChainHead, MetaKey::ChainBlock { height }],
                )?;
                ensure!(
                    read.values.iter().all(Option::is_some),
                    "lost-ACK decision not stored"
                );
                journals[i] = Some(open_journal(
                    ValidatorJournal::open(
                        &services.0[i],
                        genesis,
                        anchor,
                        set.clone(),
                        validator_key(i),
                    )?,
                    &services.0[i],
                )?);
            } else {
                complete(journal, &services.0[i])?;
            }
            let journal = journals[i].as_ref().unwrap();
            let DurableMessage::Decision { certificate, .. } =
                journal.last_durable_message().unwrap()
            else {
                bail!("recovered decision missing");
            };
            ensure!(
                wire::encode_quorum(certificate)? == bytes && journal.decided() == Some(block_hash),
                "decision or exact outbox lost"
            );
            let read = read_metadata(&services.0[i], vec![MetaKey::ChainBlock { height }])?;
            let record = ChainRecord::decode(read.values[0].as_deref().unwrap())?;
            let MetaKey::ConsensusOutbox { sequence, .. } = record.outbox_key() else {
                unreachable!()
            };
            ensure!(
                sequence > previous_revisions[i]
                    && record.point() == journal.head().unwrap()
                    && record.point().block_hash == block_hash
                    && record.point().state_version == height * 2,
                "head record or global signer revision mismatch"
            );
            previous_revisions[i] = sequence;
            records.push(record);
        }
        let head = journals[0].as_ref().unwrap().head().unwrap();
        ensure!(
            journals.iter().flatten().all(|j| j.head() == Some(head)),
            "different certificate subsets produced different chain identity"
        );
        ensure!(
            records[0].head_bytes()? != records[1].head_bytes()?,
            "fixture failed to exercise node-local proof locators"
        );
        replay_expected.push(ReplayExpected {
            records: records.clone(),
            proposal: wire::encode_proposal(proposal.proposal())?,
            certificates: certificate_bytes,
        });
        if height == 1 {
            expect_poll_error(&mut stale, &services.0[0], "durable signing state changed")?;
        }
        if height < 3 {
            for (i, slot) in journals.iter_mut().enumerate() {
                let journal = slot.as_mut().unwrap();
                journal.advance_height()?;
                ensure!(
                    journal.context() == context,
                    "advance changed height before ACK"
                );
                if height == 2 && i == 1 {
                    submit_without_ack(journal, &services.0[i])?;
                    slot.take();
                    let _ = read_metadata(&services.0[i], vec![MetaKey::ChainHead])?;
                    *slot = Some(open_journal(
                        ValidatorJournal::open(
                            &services.0[i],
                            genesis,
                            anchor,
                            set.clone(),
                            validator_key(i),
                        )?,
                        &services.0[i],
                    )?);
                } else {
                    state_only(journal, &services.0[i])?;
                }
                let journal = slot.as_ref().unwrap();
                ensure!(
                    journal.parent() == head
                        && journal.context().height == height + 1
                        && journal.decided().is_none()
                        && journal.round() == 0
                        && journal.last_durable_message().is_none(),
                    "successor reset/recovery failed"
                );
                if height == 2 && i == 3 {
                    let validator =
                        Validator::new(validator_key(i).verifying_key().to_bytes(), 1)?.id();
                    rollback_snapshot = Some(read_metadata(
                        &services.0[i],
                        vec![MetaKey::ChainHead, MetaKey::ConsensusState(validator)],
                    )?);
                }
            }
        }
        expected_head = Some(head);
        previous_candidates = Some(candidates);
    }
    let final_head = expected_head.unwrap();
    let current = journals[0].as_ref().unwrap();
    ensure!(
        ArchiveRead::new(4, final_head, current.context(), set.clone()).is_err(),
        "archive read accepted height beyond local decided head"
    );
    let before_replay = read_metadata(
        &services.0[0],
        vec![MetaKey::ChainHead, records[0].outbox_key()],
    )?;
    for expected in [&replay_expected[0], &replay_expected[2]] {
        check_archive_replay(
            &services.0[0],
            current.context(),
            final_head,
            set.clone(),
            expected,
            0,
        )?;
    }
    ensure!(
        read_metadata(
            &services.0[0],
            vec![MetaKey::ChainHead, records[0].outbox_key()],
        )?
        .values
            == before_replay.values,
        "historical replay mutated local head or decision outbox"
    );
    let exact_messages = journals
        .iter()
        .flatten()
        .map(|j| message_bytes(j.last_durable_message()))
        .collect::<Result<Vec<_>>>()?;
    drop(stale);
    drop(journals);
    services.shutdown()?;
    // No re-execution: reopen the persisted prefix and read the finalized root.
    for (i, path) in paths.iter().enumerate() {
        services.0.push(CandidatePipeline::start(
            pipeline_config(path)?,
            OpenMode::Existing,
        )?);
        let pipeline = &services.0[i];
        let journal = open_journal(
            ValidatorJournal::open(pipeline, genesis, anchor, set.clone(), validator_key(i))?,
            pipeline,
        )?;
        ensure!(
            journal.head() == Some(final_head)
                && journal.context().height == 3
                && journal.decided() == Some(final_head.block_hash)
                && message_bytes(journal.last_durable_message())? == exact_messages[i],
            "cold prefix recovery lost head or signing outbox"
        );
        // Reopened resident session: replay reconstructs only read-only stored
        // content, never a DurableCandidate from the retired session.
        for expected in [&replay_expected[0], &replay_expected[2]] {
            check_archive_replay(
                pipeline,
                journal.context(),
                journal.head().unwrap(),
                set.clone(),
                expected,
                i,
            )?;
        }
        ensure!(
            u128::from_le_bytes(
                value(pipeline, final_head.state_root, balance_key(&account(2)))?
                    .try_into()
                    .unwrap()
            ) == 350,
            "failed transfer changed recipient balance"
        );
        for raw in raw_at(3)? {
            let signed = authenticate_transfer_v3(&raw, CHAIN, 1024)?;
            ensure!(
                u64::from_le_bytes(
                    value(
                        pipeline,
                        final_head.state_root,
                        nonce_key(&signed.nonce_identity())
                    )?
                    .try_into()
                    .unwrap()
                ) == 3,
                "business failure did not consume exactly one nonce"
            );
        }
    }
    services.shutdown()?;
    reject_partial_rollback(
        &paths[3],
        genesis,
        anchor,
        set.clone(),
        rollback_snapshot.unwrap(),
    )?;
    // Deliberately damage only the throwaway test DBs, each a different hole.
    // No repair, reset, history skipping or automatic replay is allowed.
    for (i, path) in paths.iter().enumerate() {
        let relative = match i {
            0 => b"m/consensus/v1/chain/head".to_vec(),
            1 => match records[i].outbox_key() {
                MetaKey::ConsensusOutbox {
                    validator,
                    sequence,
                } => [
                    b"m/consensus/v1/outbox/".as_slice(),
                    &validator,
                    &sequence.to_be_bytes(),
                ]
                .concat(),
                _ => unreachable!(),
            },
            2 => marker_key(records[i].candidate_id()),
            _ => [
                b"m/consensus/v1/chain/block/".as_slice(),
                &2u64.to_be_bytes(),
            ]
            .concat(),
        };
        let key = {
            let store = CandidateStore::open(store_config(path)?, OpenMode::Existing)?;
            store.scoped_key(&relative)
        };
        {
            let config = store_config(path)?;
            let mut storage =
                StorageSession::open(&config.library, &config.database, config.storage)?;
            ensure!(
                storage.get(&key)?.is_some(),
                "damage fixture key did not exist"
            );
            storage.atomic_write_batch(&[StorageWrite::Delete { key }])?;
        }
        let pipeline = CandidatePipeline::start(pipeline_config(path)?, OpenMode::Existing)?;
        services.0.push(pipeline);
        let pipeline = &services.0[i];
        let result = open_journal(
            ValidatorJournal::open(pipeline, genesis, anchor, set.clone(), validator_key(i))?,
            pipeline,
        );
        let error = match result {
            Ok(_) => bail!("damaged chain reopened"),
            Err(e) => e,
        };
        let expected = [
            "missing chain head",
            "decided chain outbox missing",
            "decided chain candidate missing",
            "decided chain block missing",
        ][i];
        ensure!(
            format!("{error:#}").contains(expected),
            "wrong damage rejection: {error:#}"
        );
        // Reuse the existing deliberately damaged stores without repairing or
        // weakening the journal's cold-recovery rejections above. Only absence
        // of the requested ChainBlock is a replay miss, never missing evidence
        // for a block that is present.
        let mut replay = ArchiveRead::new(
            if i == 3 { 2 } else { 3 },
            final_head,
            records[i].context(),
            set.clone(),
        )?;
        let replayed = finish_archive_read(&mut replay, pipeline);
        if i == 3 {
            ensure!(
                replayed?.is_none(),
                "missing archive block was not distinct from pending"
            );
        } else {
            let error = match replayed {
                Ok(_) => bail!("damaged archived evidence was reported as readable or missing"),
                Err(error) => error,
            };
            let expected = [
                "archive durable head missing",
                "archive decision outbox missing",
                "archive decided candidate missing",
            ][i];
            ensure!(
                format!("{error:#}").contains(expected),
                "wrong archive damage rejection: {error:#}"
            );
        }
        ensure!(
            replay.poll(pipeline).is_err(),
            "terminal archive read retried"
        );
    }
    services.shutdown()?;
    println!("real AOEM four stores x three consecutive blocks: six distinct transactions / 24 executions, different QC subsets, global signer log, stale-head guard, lost decision/advance ACK, cold recovery, historical/latest archive replay before and after reopen, balance/nonces, partial rollback and four corruption rejections PASS; same process, no network/TPS/proof claim; {}", directory.display());
    Ok(())
}
