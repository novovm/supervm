//! Real AOEM journal integration for quorum-qualified local timers. The fixture
//! supplies remote signed messages in-process; it is not a network acceptance.

use super::*;
use crate::native_pipeline::consensus::collector::{CollectorLimits, VoteCollector};
use crate::native_pipeline::consensus::pacemaker::{Pacemaker, TimeoutPolicy, TimerAction};

fn timers() -> Result<Pacemaker> {
    Pacemaker::new(TimeoutPolicy {
        propose: Duration::from_millis(10),
        prevote: Duration::from_millis(10),
        precommit: Duration::from_millis(10),
        round_increment: Duration::from_millis(1),
    })
}

fn collector(
    context: ConsensusContext,
    set: &Arc<ValidatorSet>,
    round: u64,
) -> Result<VoteCollector> {
    VoteCollector::new(
        context,
        Arc::clone(set),
        round,
        CollectorLimits {
            max_retained_rounds: 8,
            max_future_round_span: 8,
            max_votes: 64,
        },
    )
}

fn remote_vote(
    context: ConsensusContext,
    set: &ValidatorSet,
    signer: usize,
    round: u64,
    phase: Phase,
    value: Option<Hash>,
) -> Result<wire::Vote> {
    let vote = novovm_consensus::round_bft::test_vectors::sign_vote(
        context,
        round,
        phase,
        value,
        set,
        &validator_key(signer),
    )?;
    wire::decode_vote(&wire::encode_vote(&vote)?)
}

fn expect_vote(
    message: DurableMessage,
    set: &ValidatorSet,
    round: u64,
    phase: Phase,
    value: Option<Hash>,
) -> Result<wire::Vote> {
    let DurableMessage::Vote(vote) = message else {
        bail!("expected actual durable journal vote")
    };
    vote.verify(set)?;
    ensure!(
        vote.round == round && vote.phase == phase && vote.value == value,
        "wrong durable timer vote"
    );
    Ok(vote)
}

fn state_only(journal: &mut ValidatorJournal, pipeline: &CandidatePipeline) -> Result<()> {
    let old_round = journal.round();
    let old_step = journal.step();
    let old_message = message_bytes(journal.last_durable_message())?;
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(
            Instant::now() < deadline,
            "pacemaker state-only ACK timed out"
        );
        match journal.poll(pipeline)? {
            Some(message) => {
                ensure!(
                    message.is_none() && !journal.is_pending(),
                    "round advancement invented a vote"
                );
                return Ok(());
            }
            None => ensure!(
                journal.round() == old_round
                    && journal.step() == old_step
                    && message_bytes(journal.last_durable_message())? == old_message,
                "unacknowledged state-only transition became visible"
            ),
        }
        std::thread::yield_now();
    }
}

fn submit_without_ack(journal: &mut ValidatorJournal, pipeline: &CandidatePipeline) -> Result<()> {
    let old_round = journal.round();
    let old_step = journal.step();
    let old_message = message_bytes(journal.last_durable_message())?;
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(
            Instant::now() < deadline,
            "pacemaker no-ACK submission timed out"
        );
        if journal.enqueue_pending(pipeline)? {
            break;
        }
        std::thread::yield_now();
    }
    // The real enqueue path was used, without consuming its ACK. The sole I/O
    // request remains held here; completion/readback is checked by the next
    // normal journal poll, not a second read that would need another permit.
    ensure!(
        journal.is_pending()
            && journal.round() == old_round
            && journal.step() == old_step
            && message_bytes(journal.last_durable_message())? == old_message,
        "enqueued but unacknowledged transition released authority/signature"
    );
    Ok(())
}

fn execute(pipeline: &CandidatePipeline, root: Hash, slot: u64) -> Result<DurableCandidate> {
    let mut context = batch_context(root);
    context.slot = slot;
    context.timestamp_unix_ms += slot;
    let mut request = BatchRequest::new(raw()?, context, policy())?;
    let deadline = Instant::now() + DEADLINE;
    let mut ticket = loop {
        ensure!(
            Instant::now() < deadline,
            "pacemaker candidate admission timed out"
        );
        match pipeline.try_submit(request)? {
            Submission::Accepted(ticket) => break ticket,
            Submission::Backpressured(returned) => request = returned,
        }
        std::thread::yield_now();
    };
    loop {
        ensure!(
            Instant::now() < deadline,
            "pacemaker real execution timed out"
        );
        if let Some(done) = ticket.try_take()? {
            ensure!(
                done.observation.peak_callbacks > 0 && !done.persisted.already_present,
                "pacemaker fixture did not execute new AOEM work"
            );
            return Ok(done.candidate().clone());
        }
        std::thread::yield_now();
    }
}

fn leader_index(set: &ValidatorSet, round: u64) -> Result<usize> {
    let leader = set.leader(1, round)?;
    (0..4)
        .find(|index| {
            Validator::new(validator_key(*index).verifying_key().to_bytes(), 1)
                .is_ok_and(|validator| validator.id() == leader)
        })
        .context("fixture leader missing")
}

#[test]
#[ignore = "requires explicit real AOEM library; local durable timer/catch-up integration, not network finality"]
fn real_aoem_pacemaker_quorum_timers_ack_and_locked_catchup_recovery() -> Result<()> {
    let _ = library()?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/pacemaker-integration")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    let set = Arc::new(ValidatorSet::new(
        CHAIN,
        1,
        1,
        (0..4)
            .map(|index| Validator::new(validator_key(index).verifying_key().to_bytes(), 1))
            .collect::<Result<Vec<_>>>()?,
    )?);
    let context = ConsensusContext {
        chain_id: CHAIN,
        genesis_config_commitment: GENESIS,
        protocol_commitment: PROTOCOL,
        epoch: 1,
        validator_set_hash: set.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    };
    let local = leader_index(&set, 1)?;
    let remote: Vec<_> = (0..4).filter(|index| *index != local).collect();
    let mut services = Services::default();
    let timer_path = directory.join("phase-timers");
    let root = initialize(&timer_path)?;
    let parent = ParentPoint {
        height: 0,
        block_hash: [0; 32],
        state_root: root,
        receipt_batch_commitment: empty_root(),
        state_version: 0,
        decision_hash: [0; 32],
    };
    services.0.push(CandidatePipeline::start(
        pipeline_config(&timer_path)?,
        OpenMode::Existing,
    )?);
    {
        let pipeline = &services.0[0];
        let mut journal = open_journal(
            ValidatorJournal::open(
                pipeline,
                context,
                parent,
                Arc::clone(&set),
                validator_key(local),
            )?,
            pipeline,
        )?;
        let mut votes = collector(context, &set, 0)?;
        let mut pacemaker = timers()?;
        let start = Instant::now();
        assert_eq!(
            pacemaker.poll(&mut journal, &votes, start)?,
            TimerAction::Idle
        );
        assert_eq!(
            pacemaker.poll(&mut journal, &votes, start + Duration::from_millis(10))?,
            TimerAction::Timeout(TimeoutStep::Propose)
        );
        expect_vote(
            complete(&mut journal, pipeline)?,
            &set,
            0,
            Phase::Prevote,
            None,
        )?;
        for (signer, value) in remote.iter().zip([Some([31; 32]), Some([32; 32])]) {
            votes.insert(&remote_vote(
                context,
                &set,
                *signer,
                0,
                Phase::Prevote,
                value,
            )?)?;
        }
        // A huge amount of local time alone is NOT a prevote timeout license.
        assert_eq!(
            pacemaker.poll(&mut journal, &votes, start + Duration::from_secs(100))?,
            TimerAction::Idle
        );
        assert_eq!(journal.step(), TimeoutStep::Prevote);
        votes.insert(&remote_vote(
            context,
            &set,
            remote[2],
            0,
            Phase::Prevote,
            None,
        )?)?;
        let armed = start + Duration::from_millis(100_001);
        assert_eq!(
            pacemaker.poll(&mut journal, &votes, armed)?,
            TimerAction::Idle
        );
        for value in [Some([31; 32]), Some([32; 32]), None] {
            assert!(votes.quorum(0, Phase::Prevote, value)?.is_none());
        }
        assert_eq!(
            pacemaker.poll(&mut journal, &votes, armed + Duration::from_millis(9))?,
            TimerAction::Idle
        );
        let before = message_bytes(journal.last_durable_message())?;
        assert_eq!(
            pacemaker.poll(&mut journal, &votes, armed + Duration::from_millis(10))?,
            TimerAction::Timeout(TimeoutStep::Prevote)
        );
        assert_eq!(journal.step(), TimeoutStep::Prevote);
        assert_eq!(message_bytes(journal.last_durable_message())?, before);
        assert_eq!(
            pacemaker.poll(&mut journal, &votes, armed + Duration::from_millis(11))?,
            TimerAction::Pending
        );
        submit_without_ack(&mut journal, pipeline)?;
        expect_vote(
            complete(&mut journal, pipeline)?,
            &set,
            0,
            Phase::Precommit,
            None,
        )?;
        for (signer, value) in remote.iter().zip([Some([31; 32]), Some([32; 32]), None]) {
            votes.insert(&remote_vote(
                context,
                &set,
                *signer,
                0,
                Phase::Precommit,
                value,
            )?)?;
        }
        let round_armed = armed + Duration::from_millis(20);
        assert_eq!(
            pacemaker.poll(&mut journal, &votes, round_armed)?,
            TimerAction::Idle
        );
        assert_eq!(
            pacemaker.poll(
                &mut journal,
                &votes,
                round_armed + Duration::from_millis(10)
            )?,
            TimerAction::Timeout(TimeoutStep::Precommit)
        );
        assert_eq!(journal.round(), 0);
        submit_without_ack(&mut journal, pipeline)?;
        state_only(&mut journal, pipeline)?;
        assert_eq!(journal.round(), 1);
        assert!(journal.last_durable_message().is_none());
    }
    services.shutdown()?;

    // A second real database starts in Propose with no local proposal/body.
    // Three actual remote precommits can expire its round wait without making
    // up an intermediate local prevote/precommit.
    let locked_path = directory.join("locked-catchup");
    ensure!(
        initialize(&locked_path)? == root,
        "fixture genesis roots disagree"
    );
    services.0.push(CandidatePipeline::start(
        pipeline_config(&locked_path)?,
        OpenMode::Existing,
    )?);
    let witness;
    let locked_value;
    {
        let pipeline = &services.0[0];
        let mut journal = open_journal(
            ValidatorJournal::open(
                pipeline,
                context,
                parent,
                Arc::clone(&set),
                validator_key(local),
            )?,
            pipeline,
        )?;
        let mut votes = collector(context, &set, 0)?;
        for (signer, value) in remote.iter().zip([Some([31; 32]), Some([32; 32]), None]) {
            votes.insert(&remote_vote(
                context,
                &set,
                *signer,
                0,
                Phase::Precommit,
                value,
            )?)?;
        }
        let mut pacemaker = timers()?;
        let start = Instant::now();
        assert_eq!(
            pacemaker.poll(&mut journal, &votes, start)?,
            TimerAction::Idle
        );
        assert_eq!(journal.step(), TimeoutStep::Propose);
        assert_eq!(
            pacemaker.poll(&mut journal, &votes, start + Duration::from_millis(10))?,
            TimerAction::Timeout(TimeoutStep::Precommit)
        );
        assert_eq!(journal.step(), TimeoutStep::Propose);
        assert!(journal.last_durable_message().is_none());
        submit_without_ack(&mut journal, pipeline)?;
        state_only(&mut journal, pipeline)?;
        assert_eq!(journal.round(), 1);
        assert!(journal.last_durable_message().is_none());
        votes.advance_round(1)?;
        let candidate = execute(pipeline, root, 0)?;
        journal.propose(&candidate, None)?;
        let DurableMessage::Proposal(proposal) = complete(&mut journal, pipeline)? else {
            bail!("local leader did not durably propose")
        };
        locked_value = proposal.value;
        let proposal = proposal.verify(&set)?;
        journal.accept_proposal(&proposal, &candidate, None)?;
        let local_prevote = expect_vote(
            complete(&mut journal, pipeline)?,
            &set,
            1,
            Phase::Prevote,
            Some(locked_value),
        )?;
        votes.insert(&local_prevote)?;
        for signer in remote.iter().take(2) {
            votes.insert(&remote_vote(
                context,
                &set,
                *signer,
                1,
                Phase::Prevote,
                Some(locked_value),
            )?)?;
        }
        let qc = votes
            .quorum(1, Phase::Prevote, Some(locked_value))?
            .context("missing real three-vote QC")?;
        journal.observe_prevotes(&qc, Some((&proposal, &candidate)))?;
        expect_vote(
            complete(&mut journal, pipeline)?,
            &set,
            1,
            Phase::Precommit,
            Some(locked_value),
        )?;
        witness = wire::encode_quorum(
            journal
                .valid_certificate()
                .context("valid witness missing")?,
        )?;

        let wrong = ConsensusContext {
            protocol_commitment: [0xfe; 32],
            ..context
        };
        let mut foreign_votes = collector(wrong, &set, 1)?;
        for signer in remote.iter().take(2) {
            foreign_votes.insert(&remote_vote(wrong, &set, *signer, 4, Phase::Prevote, None)?)?;
        }
        let foreign = foreign_votes
            .catch_up()
            .context("foreign fixture lacks evidence")?;
        assert!(journal.catch_up(&foreign).is_err());
        assert!(!journal.is_pending());
        assert_eq!(journal.round(), 1);
        assert_eq!(
            wire::encode_quorum(journal.valid_certificate().unwrap())?,
            witness
        );

        votes.insert(&remote_vote(
            context,
            &set,
            remote[0],
            4,
            Phase::Prevote,
            None,
        )?)?;
        votes.insert(&remote_vote(
            context,
            &set,
            remote[0],
            4,
            Phase::Precommit,
            None,
        )?)?;
        assert!(votes.catch_up().is_none());
        votes.insert(&remote_vote(
            context,
            &set,
            remote[1],
            4,
            Phase::Precommit,
            Some([33; 32]),
        )?)?;
        let before = message_bytes(journal.last_durable_message())?;
        assert_eq!(
            pacemaker.poll(&mut journal, &votes, start + Duration::from_millis(20))?,
            TimerAction::CatchUp(4)
        );
        assert_eq!(journal.round(), 1);
        assert_eq!(message_bytes(journal.last_durable_message())?, before);
        submit_without_ack(&mut journal, pipeline)?;
        state_only(&mut journal, pipeline)?;
        assert_eq!(journal.round(), 4);
        assert_eq!(journal.step(), TimeoutStep::Propose);
        assert!(journal.last_durable_message().is_none());
        assert_eq!(
            wire::encode_quorum(journal.valid_certificate().unwrap())?,
            witness
        );
    }
    services.shutdown()?;
    services.0.push(CandidatePipeline::start(
        pipeline_config(&locked_path)?,
        OpenMode::Existing,
    )?);
    {
        let pipeline = &services.0[0];
        let mut journal = open_journal(
            ValidatorJournal::open(
                pipeline,
                context,
                parent,
                Arc::clone(&set),
                validator_key(local),
            )?,
            pipeline,
        )?;
        assert_eq!(journal.round(), 4);
        assert_eq!(journal.step(), TimeoutStep::Propose);
        assert!(journal.last_durable_message().is_none());
        assert_eq!(
            wire::encode_quorum(journal.valid_certificate().unwrap())?,
            witness
        );
        // Prove the LOCK, not merely the valid certificate, survived recovery:
        // a different genuinely executed value without a valid-round proof must
        // lead our real journal to nil, never a conflicting value prevote.
        let other_candidate = execute(pipeline, root, 1)?;
        let other_value =
            BlockStatement::from_executed(other_candidate.packet(), context, &set, &parent)?.hash();
        assert_ne!(other_value, locked_value);
        let proposer = leader_index(&set, 4)?;
        assert_ne!(proposer, local);
        let remote_proposal = novovm_consensus::round_bft::test_vectors::sign_proposal(
            context,
            4,
            other_value,
            None,
            &set,
            &validator_key(proposer),
        )?
        .verify(&set)?;
        journal.accept_proposal(&remote_proposal, &other_candidate, None)?;
        expect_vote(
            complete(&mut journal, pipeline)?,
            &set,
            4,
            Phase::Prevote,
            None,
        )?;
        assert_eq!(
            wire::encode_quorum(journal.valid_certificate().unwrap())?,
            witness
        );
    }
    services.shutdown()?;
    println!("real AOEM local timers: 2/4 cannot timeout prevote, mixed 3/4 only arms waits, persisted ACK gate, Propose-stage round expiry without fake votes, same-round f+1 catch-up and recovered lock PASS; not network/finality/TPS; artifacts {}", directory.display());
    Ok(())
}
