//! Real local AOEM journal recovery. Quorum inputs here are explicitly signed
//! fixtures, not a network/finality test; the controller crash test covers that.
use super::*;
use crate::consensus::journal::ReplayEvidence;
use novovm_aoem::{StorageSession, StorageWrite};

fn state_only(journal: &mut ValidatorJournal, pipeline: &CandidatePipeline) -> Result<()> {
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(Instant::now() < deadline, "state-only transition timed out");
        if let Some(message) = journal.poll(pipeline)? {
            ensure!(message.is_none(), "state-only update signed a second vote");
            return Ok(());
        }
        std::thread::yield_now();
    }
}

fn quorum(
    set: &ValidatorSet,
    context: ConsensusContext,
    round: u64,
    value: Hash,
) -> Result<wire::VerifiedQuorum> {
    Quorum::from_votes(
        set,
        (0..3)
            .map(|i| {
                wire::Vote::sign(
                    context,
                    round,
                    Phase::Prevote,
                    Some(value),
                    set,
                    &validator_key(i),
                )
            })
            .collect::<Result<Vec<_>>>()?,
    )?
    .verify(set)
}

#[test]
#[ignore = "requires real AOEM; exact undecided journal replay and corruption, not network finality"]
fn real_late_quorum_reopen_preserves_both_votes_old_proposal_proof_and_locked_candidate(
) -> Result<()> {
    let _ = library()?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/consensus-tests")
        .join(format!(
            "late-quorum-replay-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    let root = initialize(&directory)?;
    let set = Arc::new(ValidatorSet::new(
        CHAIN,
        1,
        1,
        (0..4)
            .map(|i| Validator::new(validator_key(i).verifying_key().to_bytes(), 1))
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
    let parent = ParentPoint {
        height: 0,
        block_hash: [0; 32],
        state_root: root,
        receipt_batch_commitment: empty_root(),
        state_version: 0,
        decision_hash: [0; 32],
    };
    let leader = |round| -> Result<usize> {
        let id = set.leader(1, round)?;
        (0..4)
            .find(|i| {
                Validator::new(validator_key(*i).verifying_key().to_bytes(), 1)
                    .is_ok_and(|v| v.id() == id)
            })
            .context("fixture leader missing")
    };
    let index = leader(1)?;
    let validator = Validator::new(validator_key(index).verifying_key().to_bytes(), 1)?.id();
    let mut services = Services(vec![CandidatePipeline::start(
        pipeline_config(&directory)?,
        OpenMode::Existing,
    )?]);
    let pipeline = &services.0[0];
    let mut execution = admit(pipeline, root)?;
    let deadline = Instant::now() + DEADLINE;
    let candidate = loop {
        ensure!(
            Instant::now() < deadline,
            "replay candidate execution timed out"
        );
        if let Some(done) = execution.try_take()? {
            break done.candidate().clone();
        }
        std::thread::yield_now();
    };
    let value = BlockStatement::from_executed(candidate.packet(), context, &set, &parent)?.hash();
    let mut journal = open_journal(
        ValidatorJournal::open(pipeline, context, parent, set.clone(), validator_key(index))?,
        pipeline,
    )?;
    let p0 = wire::Proposal::sign(context, 0, value, None, &set, &validator_key(leader(0)?))?
        .verify(&set)?;
    journal.accept_proposal(&p0, &candidate, None)?;
    complete(&mut journal, pipeline)?;
    let qc0 = quorum(&set, context, 0, value)?;
    journal.observe_prevotes(&qc0, Some((&p0, &candidate)))?;
    complete(&mut journal, pipeline)?;
    journal.round_wait_elapsed(0)?;
    state_only(&mut journal, pipeline)?;
    journal.propose(&candidate, Some(&qc0))?;
    let DurableMessage::Proposal(p1) = complete(&mut journal, pipeline)? else {
        bail!("missing original proposal")
    };
    let original_proposal = wire::encode_proposal(&p1)?;
    let p1 = p1.verify(&set)?;
    journal.accept_proposal(&p1, &candidate, Some(&qc0))?;
    let prevote = complete(&mut journal, pipeline)?;
    journal.timeout(1, TimeoutStep::Prevote)?;
    let precommit = complete(&mut journal, pipeline)?;
    let DurableMessage::Vote(nil) = &precommit else {
        bail!("missing nil precommit")
    };
    ensure!(
        nil.value.is_none() && nil.phase == Phase::Precommit,
        "timeout changed vote subject"
    );
    let qc1 = quorum(&set, context, 1, value)?;
    journal.observe_prevotes(&qc1, Some((&p1, &candidate)))?;
    state_only(&mut journal, pipeline)?;
    ensure!(
        journal.last_durable_message().is_none(),
        "late QC was not state-only"
    );
    let mut keys = vec![MetaKey::ConsensusState(validator)];
    keys.extend((1..=7).map(|sequence| MetaKey::ConsensusOutbox {
        validator,
        sequence,
    }));
    let before = read_metadata(pipeline, keys.clone())?;
    ensure!(
        before.values.iter().all(Option::is_some),
        "fixture did not persist all seven transitions"
    );
    drop(journal);
    services.shutdown()?;
    services.0.push(CandidatePipeline::start(
        pipeline_config(&directory)?,
        OpenMode::Existing,
    )?);
    let pipeline = &services.0[0];
    let mut recovered = open_journal(
        ValidatorJournal::open(pipeline, context, parent, set.clone(), validator_key(index))?,
        pipeline,
    )?;
    let replay = recovered.take_replay_records();
    ensure!(
        replay.len() == 5,
        "role references lost or duplicated an original event"
    );
    ensure!(
        recovered.take_replay_records().is_empty(),
        "recovery records are not consumed"
    );
    let mut proposals = 0;
    let mut replay_votes = Vec::new();
    let mut certified_rounds = Vec::new();
    for record in replay {
        if let Some(DurableMessage::Vote(vote)) = &record.message {
            if vote.round == 1 {
                replay_votes.push(message_bytes(record.message.as_ref())?.unwrap());
            }
        }
        match record.evidence {
            ReplayEvidence::Proposal {
                proposal,
                justification,
                candidate: locator,
            } => {
                ensure!(
                    wire::encode_proposal(proposal.proposal())? == original_proposal,
                    "old signed proposal changed"
                );
                let justification = justification.context("lost original proposal QC")?;
                ensure!(
                    wire::encode_quorum(justification.quorum())?
                        == wire::encode_quorum(qc0.quorum())?,
                    "latest QC replaced original valid_round proof"
                );
                ensure!(
                    locator.value == value
                        && locator.candidate_id == candidate.packet().candidate_id()
                        && locator.document_digest == candidate.packet().document_digest(),
                    "candidate locator differs from original execution"
                );
                proposals += 1;
            }
            ReplayEvidence::Certified { certificate, .. } => {
                certified_rounds.push(certificate.round())
            }
            ReplayEvidence::None => {}
        }
    }
    ensure!(
        proposals == 2 && certified_rounds == [0, 1],
        "lost distinct original-proposal and locked/valid evidence"
    );
    ensure!(
        replay_votes
            == [
                message_bytes(Some(&prevote))?.unwrap(),
                message_bytes(Some(&precommit))?.unwrap()
            ],
        "replay does not contain exact two original current-round votes"
    );
    ensure!(
        recovered
            .valid_certificate()
            .context("lost latest valid QC")?
            .verify(&set)?
            .round()
            == 1,
        "latest valid checkpoint changed"
    );
    ensure!(
        recovered
            .observe_prevotes(&qc1, Some((&p1, &candidate)))
            .is_err(),
        "cold recovery accepted a foreign owner capability"
    );
    // A stale capability alone cannot test anti-double-signing: obtain a REAL
    // execution on the reopened owner, then reach the restored round guard.
    let mut execution = admit(pipeline, root)?;
    let deadline = Instant::now() + DEADLINE;
    let current_candidate = loop {
        ensure!(Instant::now() < deadline, "cold re-execution timed out");
        if let Some(done) = execution.try_take()? {
            break done.candidate().clone();
        }
        std::thread::yield_now();
    };
    ensure!(
        current_candidate.packet().candidate_id() == candidate.packet().candidate_id()
            && current_candidate.packet().document_digest() == candidate.packet().document_digest(),
        "cold re-execution changed the durable candidate"
    );
    let error = recovered
        .observe_prevotes(&qc1, Some((&p1, &current_candidate)))
        .expect_err("restored current-round QC authorized a second precommit");
    ensure!(
        error
            .to_string()
            .contains("current-round prevote quorum already applied")
            && !recovered.is_pending(),
        "duplicate QC was rejected for an unrelated reason: {error:#}"
    );
    ensure!(
        read_metadata(pipeline, keys.clone())? == before,
        "read-only recovery signed or repaired metadata"
    );
    drop(recovered);
    services.shutdown()?;
    // Delete the OLD proposal event, not the latest state-only outbox. Startup
    // must follow the durable reference and refuse, rather than reset the lock.
    let config = store_config(&directory)?;
    let missing = {
        let store = CandidateStore::open(store_config(&directory)?, OpenMode::Existing)?;
        store.scoped_key(
            &[
                b"m/consensus/v1/outbox/".as_slice(),
                &validator,
                &4u64.to_be_bytes(),
            ]
            .concat(),
        )
    };
    {
        let mut storage = StorageSession::open(&config.library, &config.database, config.storage)?;
        ensure!(
            storage.get(&missing)?.is_some(),
            "historical event to damage is absent"
        );
        storage.atomic_write_batch(&[StorageWrite::Delete { key: missing }])?;
    }
    services.0.push(CandidatePipeline::start(
        pipeline_config(&directory)?,
        OpenMode::Existing,
    )?);
    let pipeline = &services.0[0];
    let opening = ValidatorJournal::open(pipeline, context, parent, set, validator_key(index))?;
    ensure!(
        open_journal(opening, pipeline).is_err(),
        "missing historical replay event reopened as usable signer"
    );
    let after = read_metadata(pipeline, keys)?;
    ensure!(
        after.values[0] == before.values[0] && after.values[4].is_none(),
        "failed recovery repaired snapshot or missing event"
    );
    services.shutdown()?;
    eprintln!(
        "late-QC exact original replay and missing historical event rejection: {}",
        directory.display()
    );
    Ok(())
}
