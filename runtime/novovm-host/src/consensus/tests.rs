//! Same-process, four-independent-database integration. Real signed NOV batches
//! use the resident AOEM pipeline; signed consensus messages are transported as
//! encoded bytes by this fixture, not by a real network. Decision archival is
//! deliberately NOT canonical-head publication, execution proof or mainnet TPS.

use super::statement::{BlockStatement, ParentPoint};
use super::wire::{
    self, Context as ConsensusContext, Hash, Phase, Quorum, Validator, ValidatorSet,
};
use super::{DurableMessage, JournalOpening, TimeoutStep, ValidatorJournal};
use crate::business::direct_nov_fee::{DirectNovFeePolicy, FeeState};
use crate::business::nov_transfer_batch::{
    balance_key, effect_contract, fee_record_changes, program_id, receipt_codec, SEMANTIC_VERSION,
};
use crate::business::quoted_transfer::Account;
use crate::execution::plan::{BatchContext, PlanBudget};
use crate::ingress::batch::AuthenticationBudget;
use crate::ingress::wire::{encode_transfer_v3, signing_message, FeePolicy, TransferV3};
use crate::persistence::io::IoBudget;
use crate::persistence::metadata::{MetaKey, MetadataSnapshot};
use crate::persistence::{CandidateStore, OpenMode, PacketBudget, StorageDomain, StoreConfig};
use crate::pipeline::{
    BatchRequest, CandidatePipeline, DurableCandidate, PipelineConfig, PipelineTicket, Submission,
};
use crate::state::frontier::CaptureBudget;
use crate::state::tree::{empty_root, stage_state_update, NodeHash, StateChange, StateNodeReader};
use anyhow::{bail, ensure, Context, Result};
use ed25519_dalek::{Signer, SigningKey};
use novovm_aoem::StorageConfig;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CHAIN: u64 = 292;
const GENESIS: Hash = [0x71; 32];
const PROTOCOL: Hash = [0x72; 32];
const DEADLINE: Duration = Duration::from_secs(60);

#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);
impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

// Join all started services even if an assertion fails, so a failed fixture
// does not silently leave I/O or compute owners alive for subsequent tests.
#[derive(Default)]
struct Services(Vec<CandidatePipeline>);
impl Services {
    fn shutdown(&mut self) -> Result<()> {
        let mut error = None;
        for service in self.0.drain(..) {
            if let Err(failure) = service.shutdown() {
                error.get_or_insert(failure);
            }
        }
        match error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}
impl Drop for Services {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn library() -> Result<PathBuf> {
    fs::canonicalize(PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .context("explicit real NOVOVM_AOEM_TEST_LIBRARY required")?,
    ))
    .context("resolve real AOEM test library")
}

fn store_config(directory: &Path) -> Result<StoreConfig> {
    Ok(StoreConfig {
        library: library()?,
        database: directory.join("provider.rocksdb"),
        domain: StorageDomain {
            chain_id: CHAIN,
            genesis_config_commitment: GENESIS,
            protocol_commitment: PROTOCOL,
        },
        storage: StorageConfig::default(),
        packet_budget: PacketBudget::default(),
    })
}

fn pipeline_config(directory: &Path) -> Result<PipelineConfig> {
    Ok(PipelineConfig {
        store: store_config(directory)?,
        workers: 4,
        max_batches: 2,
        max_retained_bytes: 256 * 1024 * 1024,
        authentication: AuthenticationBudget {
            transactions: 8,
            transaction_bytes: 1024,
            body_bytes: 8192,
        },
        plan: PlanBudget {
            transactions: 8,
            transaction_bytes: 1024,
            body_bytes: 8192,
            access_keys: 128,
        },
        capture: CaptureBudget {
            keys: 128,
            nodes: 4096,
            bytes: 2 * 1024 * 1024,
        },
        compute_timeout: Duration::from_secs(30),
        io: IoBudget {
            requests: 1,
            ..IoBudget::default()
        },
        capture_edge_quantum: 64,
    })
}

fn policy() -> DirectNovFeePolicy {
    DirectNovFeePolicy {
        quote_ttl_ms: 15000,
        policy_version: 1,
        policy_source: "default".into(),
        resolution_source: "runtime_state".into(),
        reserve_share_bps: 7000,
        fee_share_bps: 2000,
        risk_buffer_share_bps: 1000,
        min_reserve_bucket_nov: 0,
        min_fee_bucket_nov: 0,
        min_risk_buffer_nov: 1,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: 1_000_000,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 500,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    }
}

fn account(seed: u8) -> Account {
    let public = SigningKey::from_bytes(&[seed; 32])
        .verifying_key()
        .to_bytes();
    Account::try_from(Sha256::digest(public)[12..].to_vec()).unwrap()
}

fn raw() -> Result<Vec<Vec<u8>>> {
    [(1, 100), (3, 50)]
        .into_iter()
        .map(|(seed, amount)| {
            let mut tx = TransferV3 {
                chain_id: CHAIN,
                from: account(seed).as_bytes().to_vec(),
                to: account(2).as_bytes().to_vec(),
                asset: "NOV".into(),
                amount,
                nonce: 0,
                fee_policy: FeePolicy {
                    pay_asset: "NOV".into(),
                    max_pay_amount: 0,
                    slippage_bps: 0,
                },
                signature: Vec::new(),
            };
            let signer = SigningKey::from_bytes(&[seed; 32]);
            let signature = signer.sign(&signing_message(&tx)?);
            tx.signature = signer.verifying_key().to_bytes().to_vec();
            tx.signature.extend_from_slice(&signature.to_bytes());
            encode_transfer_v3(&tx)
        })
        .collect()
}

fn batch_context(root: Hash) -> BatchContext {
    BatchContext {
        chain_id: CHAIN,
        genesis_config_commitment: GENESIS,
        protocol_commitment: PROTOCOL,
        business_program: program_id(),
        semantic_version: SEMANTIC_VERSION,
        effect_contract: effect_contract(&policy()).unwrap(),
        parent_block_hash: [0; 32],
        parent_height: 0,
        parent_state_root: root,
        parent_receipt_root: empty_root(),
        parent_state_version: 0,
        receipt_codec: receipt_codec(),
        height: 1,
        slot: 0,
        timestamp_unix_ms: 172_800_500,
    }
}

fn initialize(directory: &Path) -> Result<Hash> {
    fs::create_dir_all(directory)?;
    let mut changes = fee_record_changes(&policy(), &FeeState::default())?;
    for seed in [1, 3] {
        changes.push(StateChange::Put {
            key: balance_key(&account(seed)),
            value: 1_000_000u128.to_le_bytes().to_vec(),
        });
    }
    let update = stage_state_update(&Memory::default(), empty_root(), &changes)?;
    let store = CandidateStore::open(store_config(directory)?, OpenMode::CreateNew)?;
    store.install_unpublished_state(&update)?;
    Ok(update.root())
}

fn admit(pipeline: &CandidatePipeline, root: Hash) -> Result<PipelineTicket> {
    let deadline = Instant::now() + DEADLINE;
    let mut request = BatchRequest::new(raw()?, batch_context(root), policy())?;
    loop {
        ensure!(
            Instant::now() < deadline,
            "pipeline admission deadline exhausted"
        );
        match pipeline.try_submit(request)? {
            Submission::Accepted(ticket) => return Ok(ticket),
            Submission::Backpressured(returned) => request = returned,
        }
        std::thread::yield_now();
    }
}

fn open_journal(
    mut opening: JournalOpening,
    pipeline: &CandidatePipeline,
) -> Result<ValidatorJournal> {
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(
            Instant::now() < deadline,
            "journal opening deadline exhausted"
        );
        if let Some(journal) = opening.poll(pipeline)? {
            return Ok(journal);
        }
        std::thread::yield_now();
    }
}

fn message_bytes(message: Option<&DurableMessage>) -> Result<Option<Vec<u8>>> {
    Ok(match message {
        None => None,
        Some(DurableMessage::Proposal(proposal)) => {
            Some([vec![1], wire::encode_proposal(proposal)?].concat())
        }
        Some(DurableMessage::Vote(vote)) => Some([vec![2], wire::encode_vote(vote)?].concat()),
        Some(DurableMessage::Decision {
            proposal,
            certificate,
        }) => Some(
            [
                vec![3],
                wire::encode_proposal(proposal)?,
                wire::encode_quorum(certificate)?,
            ]
            .concat(),
        ),
    })
}

fn complete(
    journal: &mut ValidatorJournal,
    pipeline: &CandidatePipeline,
) -> Result<DurableMessage> {
    ensure!(
        journal.is_pending(),
        "fixture did not stage a signing transition"
    );
    let before = message_bytes(journal.last_durable_message())?;
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(
            Instant::now() < deadline,
            "signing transition deadline exhausted"
        );
        match journal.poll(pipeline)? {
            None => ensure!(
                message_bytes(journal.last_durable_message())? == before,
                "pending signature escaped before durable poll completion"
            ),
            Some(message) => {
                let message = message.context("fixture expected signed durable message")?;
                ensure!(
                    !journal.is_pending()
                        && message_bytes(Some(&message))?
                            == message_bytes(journal.last_durable_message())?,
                    "durable outbox reply differs from retained message"
                );
                return Ok(message);
            }
        }
        std::thread::yield_now();
    }
}

fn expect_poll_error(
    journal: &mut ValidatorJournal,
    pipeline: &CandidatePipeline,
    reason: &str,
) -> Result<()> {
    let before = message_bytes(journal.last_durable_message())?;
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(
            Instant::now() < deadline,
            "negative signing poll deadline exhausted"
        );
        match journal.poll(pipeline) {
            Err(error) => {
                ensure!(
                    format!("{error:#}").contains(reason),
                    "wrong rejection: {error:#}"
                );
                ensure!(
                    journal.is_frozen() && message_bytes(journal.last_durable_message())? == before,
                    "failed transition released a signature or remained usable"
                );
                return Ok(());
            }
            Ok(Some(_)) => bail!("invalid signing transition completed"),
            Ok(None) => ensure!(
                message_bytes(journal.last_durable_message())? == before,
                "invalid signature escaped while pending"
            ),
        }
        std::thread::yield_now();
    }
}

fn read_metadata(pipeline: &CandidatePipeline, keys: Vec<MetaKey>) -> Result<MetadataSnapshot> {
    let deadline = Instant::now() + DEADLINE;
    let mut ticket = loop {
        ensure!(
            Instant::now() < deadline,
            "metadata read admission deadline exhausted"
        );
        if let Some(ticket) = pipeline.try_read_consensus_metadata(keys.clone())? {
            break ticket;
        }
        std::thread::yield_now();
    };
    loop {
        ensure!(
            Instant::now() < deadline,
            "metadata read completion deadline exhausted"
        );
        if let Some(snapshot) = ticket.try_take()? {
            return Ok(snapshot);
        }
        std::thread::yield_now();
    }
}

fn validator_key(index: usize) -> SigningKey {
    SigningKey::from_bytes(&[101 + index as u8; 32])
}

fn wire_vote(
    message: DurableMessage,
    set: &ValidatorSet,
    phase: Phase,
    value: Hash,
) -> Result<wire::Vote> {
    let DurableMessage::Vote(vote) = message else {
        bail!("expected a durable vote")
    };
    let bytes = wire::encode_vote(&vote)?;
    let vote = wire::decode_vote(&bytes)?;
    vote.verify(set)?;
    ensure!(
        vote.round == 0 && vote.phase == phase && vote.value == Some(value),
        "unexpected durable vote subject"
    );
    Ok(vote)
}

#[test]
#[ignore = "requires explicit real AOEM library; same-process durable consensus integration, not network/canonical finality"]
fn real_four_validator_journals_persist_before_emit_recover_and_reject_stale() -> Result<()> {
    let _ = library()?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/consensus-tests")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    let paths: Vec<_> = (0..4)
        .map(|index| directory.join(format!("validator-{index}")))
        .collect();
    let members = (0..4)
        .map(|index| Validator::new(validator_key(index).verifying_key().to_bytes(), 1))
        .collect::<Result<Vec<_>>>()?;
    let set = Arc::new(ValidatorSet::new(CHAIN, 1, 1, members)?);
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
    let mut services = Services::default();
    let mut tickets = Vec::new();
    let mut parent_root = None;
    for path in &paths {
        let root = initialize(path)?;
        ensure!(
            parent_root.is_none_or(|expected| expected == root),
            "independent genesis trees differ"
        );
        parent_root = Some(root);
        let pipeline = CandidatePipeline::start(pipeline_config(path)?, OpenMode::Existing)?;
        tickets.push(Some(admit(&pipeline, root)?));
        services.0.push(pipeline);
    }
    let root = parent_root.unwrap();
    let parent = ParentPoint {
        height: 0,
        block_hash: [0; 32],
        state_root: root,
        receipt_batch_commitment: empty_root(),
        state_version: 0,
        decision_hash: [0; 32],
    };
    let mut candidates: Vec<Option<DurableCandidate>> = vec![None; 4];
    let deadline = Instant::now() + DEADLINE;
    while tickets.iter().any(Option::is_some) {
        ensure!(
            Instant::now() < deadline,
            "four real candidate executions did not finish"
        );
        for (index, ticket) in tickets.iter_mut().enumerate() {
            let Some(current) = ticket.as_mut() else {
                continue;
            };
            if let Some(done) = current.try_take()? {
                ensure!(
                    done.observation.peak_callbacks >= 1,
                    "no real AOEM business callback"
                );
                ensure!(
                    !done.persisted.already_present
                        && done.persisted.candidate_id == done.packet.candidate_id()
                        && done.persisted.state_root == done.packet.state_root()
                        && done.persisted.statement_commitment
                            == done.packet.statement_commitment()
                        && done.persisted.document_digest == done.packet.document_digest(),
                    "durable candidate binding differs"
                );
                candidates[index] = Some(done.candidate().clone());
                ticket.take();
            }
        }
        std::thread::yield_now();
    }
    let candidates: Vec<_> = candidates.into_iter().map(Option::unwrap).collect();
    let statements: Vec<_> = candidates
        .iter()
        .map(|candidate| BlockStatement::from_executed(candidate.packet(), context, &set, &parent))
        .collect::<Result<_>>()?;
    let value = statements[0].hash();
    for (candidate, statement) in candidates.iter().zip(&statements) {
        let packet = candidate.packet();
        ensure!(
            statement.hash() == value
                && statement.matches_packet(packet)
                && statement.state_version() == 2
                && packet.records() == candidates[0].packet().records(),
            "same signed input/parent did not produce exact common statement/content"
        );
    }
    let mut journals = Vec::new();
    for (index, pipeline) in services.0.iter().enumerate() {
        journals.push(open_journal(
            ValidatorJournal::open(pipeline, context, parent, set.clone(), validator_key(index))?,
            pipeline,
        )?);
    }
    let leader_id = set.leader(1, 0)?;
    let leader = (0..4)
        .find(|&index| {
            Validator::new(validator_key(index).verifying_key().to_bytes(), 1)
                .unwrap()
                .id()
                == leader_id
        })
        .unwrap();
    let stale_index = (leader + 1) % 4;
    let mut stale = open_journal(
        ValidatorJournal::open(
            &services.0[stale_index],
            context,
            parent,
            set.clone(),
            validator_key(stale_index),
        )?,
        &services.0[stale_index],
    )?;
    journals[leader].propose(&candidates[leader], None)?;
    ensure!(
        journals[leader].last_durable_message().is_none(),
        "proposal escaped before durable poll"
    );
    let DurableMessage::Proposal(proposal) = complete(&mut journals[leader], &services.0[leader])?
    else {
        bail!("missing durable proposal")
    };
    let proposal_bytes = wire::encode_proposal(&proposal)?;
    let proposal = wire::decode_proposal(&proposal_bytes)?.verify(&set)?;
    ensure!(
        proposal.proposal().value == value && proposal.proposal().proposer_id == leader_id,
        "leader proposed wrong statement"
    );
    let mut prevotes = Vec::new();
    for (index, journal) in journals.iter_mut().enumerate() {
        let before = message_bytes(journal.last_durable_message())?;
        journal.accept_proposal(&proposal, &candidates[index], None)?;
        ensure!(
            message_bytes(journal.last_durable_message())? == before,
            "prevote escaped synchronously"
        );
        prevotes.push(wire_vote(
            complete(journal, &services.0[index])?,
            &set,
            Phase::Prevote,
            value,
        )?);
    }
    // Both sessions opened the same exact absent signing snapshot. The first
    // has now durably voted for the value; the stale nil vote must lose CAS.
    stale.timeout(0, TimeoutStep::Propose)?;
    ensure!(
        stale.last_durable_message().is_none(),
        "stale vote escaped before CAS"
    );
    expect_poll_error(
        &mut stale,
        &services.0[stale_index],
        "durable signing state changed",
    )?;
    ensure!(
        stale.timeout(0, TimeoutStep::Propose).is_err(),
        "frozen stale session reused"
    );
    let prevote_bytes = wire::encode_quorum(&Quorum::from_votes(
        &set,
        prevotes.into_iter().take(3).collect(),
    )?)?;
    let prevote_qc = wire::decode_quorum(&prevote_bytes)?.verify(&set)?;
    let mut precommits = Vec::new();
    for (index, journal) in journals.iter_mut().enumerate() {
        journal.observe_prevotes(&prevote_qc, Some((&proposal, &candidates[index])))?;
        precommits.push(wire_vote(
            complete(journal, &services.0[index])?,
            &set,
            Phase::Precommit,
            value,
        )?);
    }
    let decision_bytes = wire::encode_quorum(&Quorum::from_votes(
        &set,
        precommits.into_iter().take(3).collect(),
    )?)?;
    let decision = wire::decode_quorum(&decision_bytes)?.verify(&set)?;
    let mut archived = Vec::new();
    for (index, journal) in journals.iter_mut().enumerate() {
        journal.observe_decision(&proposal, &candidates[index], &decision)?;
        ensure!(
            journal.decided().is_none(),
            "decision adopted before durable poll"
        );
        let DurableMessage::Decision {
            proposal: saved_proposal,
            certificate,
        } = complete(journal, &services.0[index])?
        else {
            bail!("missing durable decision")
        };
        ensure!(
            wire::encode_proposal(&saved_proposal)? == proposal_bytes
                && wire::encode_quorum(&certificate)? == decision_bytes
                && journal.decided() == Some(value),
            "decision bytes/state differ"
        );
        archived.push(message_bytes(journal.last_durable_message())?.unwrap());
    }
    drop(stale);
    drop(journals);
    services.shutdown()?;
    for (index, path) in paths.iter().enumerate() {
        let pipeline = CandidatePipeline::start(pipeline_config(path)?, OpenMode::Existing)?;
        services.0.push(pipeline);
        let pipeline = &services.0[index];
        let mut journal = open_journal(
            ValidatorJournal::open(pipeline, context, parent, set.clone(), validator_key(index))?,
            pipeline,
        )?;
        ensure!(
            journal.decided() == Some(value)
                && message_bytes(journal.last_durable_message())? == Some(archived[index].clone()),
            "reopened journal lost terminal decision or exact certificate bytes"
        );
        // Precommit is the saved step. Without the terminal decision check,
        // this exact local event would legally advance and allow more votes.
        let error = journal
            .timeout(0, TimeoutStep::Precommit)
            .err()
            .context("recovered decision permitted another round")?;
        ensure!(
            format!("{error:#}").contains("already decided"),
            "wrong terminal rejection: {error:#}"
        );
        // Same height/block hashes, but an altered state parent must not open
        // an existing signer snapshot as if it were the trusted original.
        if index == 0 {
            let changed_parent = ParentPoint {
                state_root: [0xf1; 32],
                ..parent
            };
            ensure!(
                BlockStatement::from_executed(
                    candidates[index].packet(),
                    context,
                    &set,
                    &changed_parent
                )
                .is_err(),
                "changed execution parent accepted"
            );
            let opening = ValidatorJournal::open(
                pipeline,
                context,
                changed_parent,
                set.clone(),
                validator_key(index),
            )?;
            let error = match open_journal(opening, pipeline) {
                Err(error) => error,
                Ok(_) => bail!("changed parent reopened journal"),
            };
            ensure!(
                format!("{error:#}").contains("identity/parent mismatch"),
                "wrong changed-parent rejection: {error:#}"
            );
        }
    }
    services.shutdown()?;
    // A real durable capsule from another resident owner is not local signing
    // authority. Rejection must not copy/repair its candidate into this DB.
    let missing = directory.join("missing-local-candidate");
    ensure!(
        initialize(&missing)? == root,
        "missing-marker fixture parent differs"
    );
    let pipeline = CandidatePipeline::start(pipeline_config(&missing)?, OpenMode::Existing)?;
    services.0.push(pipeline);
    let pipeline = &services.0[0];
    let mut journal = open_journal(
        ValidatorJournal::open(
            pipeline,
            context,
            parent,
            set.clone(),
            validator_key(leader),
        )?,
        pipeline,
    )?;
    ensure!(
        journal.propose(&candidates[leader], None).is_err(),
        "foreign owner capsule accepted"
    );
    ensure!(
        !journal.is_pending() && journal.last_durable_message().is_none(),
        "foreign candidate released or staged proposal"
    );
    drop(journal);
    services.shutdown()?;
    let store = CandidateStore::open(store_config(&missing)?, OpenMode::Existing)?;
    ensure!(
        store
            .recover(candidates[leader].packet().candidate_id())?
            .is_none(),
        "signing repaired a missing local candidate"
    );
    println!("real AOEM four independent stores: exact statement, durable votes/decision, reopen terminal, stale CAS and foreign-owner/changed-parent rejection PASS; same process, no canonical head/network/TPS claim; artifacts {}", directory.display());
    Ok(())
}

#[test]
#[ignore = "requires explicit real AOEM library; deterministic lost durable-ACK recovery, not crash/power-loss evidence"]
fn real_lost_signing_ack_recovers_exact_vote_without_second_prevote() -> Result<()> {
    let _ = library()?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/consensus-tests")
        .join(format!(
            "lost-ack-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    let root = initialize(&directory)?;
    let members = (0..4)
        .map(|index| Validator::new(validator_key(index).verifying_key().to_bytes(), 1))
        .collect::<Result<Vec<_>>>()?;
    let set = Arc::new(ValidatorSet::new(CHAIN, 1, 1, members)?);
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
    let validator = Validator::new(validator_key(0).verifying_key().to_bytes(), 1)?.id();
    let mut services = Services::default();
    services.0.push(CandidatePipeline::start(
        pipeline_config(&directory)?,
        OpenMode::Existing,
    )?);
    let pipeline = &services.0[0];
    let mut journal = open_journal(
        ValidatorJournal::open(pipeline, context, parent, set.clone(), validator_key(0))?,
        pipeline,
    )?;
    journal.timeout(0, TimeoutStep::Propose)?;
    ensure!(
        journal.is_pending() && journal.last_durable_message().is_none(),
        "nil vote escaped before submission"
    );
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(
            Instant::now() < deadline,
            "lost-ACK metadata submission deadline exhausted"
        );
        // Narrow cfg(test) hook executes the real enqueue path but NEVER polls
        // the result. A normal poll can win the worker race and adopt the ACK
        // immediately, which would not constitute this failure scenario.
        if journal.submit_pending_without_receiving_for_test(pipeline)? {
            break;
        }
        std::thread::yield_now();
    }
    ensure!(
        journal.is_pending() && journal.last_durable_message().is_none(),
        "test hook consumed/adopted durable ACK"
    );
    drop(journal); // Drop the accepted command's reply without cancelling it.
    let keys = vec![
        MetaKey::ConsensusState(validator),
        MetaKey::ConsensusOutbox {
            validator,
            sequence: 1,
        },
    ];
    // The same unique I/O owner processes this read after the accepted CAS.
    // Both real values must exist; merely queueing the write is not a PASS.
    let saved = read_metadata(pipeline, keys.clone())?;
    ensure!(
        saved.values.len() == 2 && saved.values.iter().all(Option::is_some),
        "accepted signing write did not survive lost ACK"
    );
    services.shutdown()?;
    services.0.push(CandidatePipeline::start(
        pipeline_config(&directory)?,
        OpenMode::Existing,
    )?);
    let pipeline = &services.0[0];
    let mut recovered = open_journal(
        ValidatorJournal::open(pipeline, context, parent, set.clone(), validator_key(0))?,
        pipeline,
    )?;
    ensure!(
        read_metadata(pipeline, keys.clone())? == saved,
        "reopen repaired or replaced lost-ACK records"
    );
    let Some(DurableMessage::Vote(vote)) = recovered.last_durable_message() else {
        bail!("lost-ACK vote was not recovered")
    };
    vote.verify(&set)?;
    let expected = wire::Vote::sign(context, 0, Phase::Prevote, None, &set, &validator_key(0))?;
    ensure!(
        wire::encode_vote(vote)? == wire::encode_vote(&expected)?
            && recovered.round() == 0
            && recovered.decided().is_none(),
        "recovered exact nil prevote/round changed"
    );
    let error = recovered
        .timeout(0, TimeoutStep::Propose)
        .err()
        .context("lost-ACK recovery allowed second prevote")?;
    ensure!(
        format!("{error:#}").contains("stale or mismatched local timeout"),
        "wrong duplicate-prevote rejection: {error:#}"
    );
    ensure!(
        !recovered.is_pending(),
        "rejected second prevote staged work"
    );
    recovered.timeout(0, TimeoutStep::Prevote)?;
    let DurableMessage::Vote(next) = complete(&mut recovered, pipeline)? else {
        bail!("missing subsequent nil precommit")
    };
    ensure!(
        next.phase == Phase::Precommit && next.value.is_none() && next.round == 0,
        "recovered signer failed legal next phase"
    );
    let after = read_metadata(pipeline, keys)?;
    ensure!(
        after.values[0] != saved.values[0] && after.values[1] == saved.values[1],
        "legal next phase overwrote original append-only vote or failed to advance state"
    );
    drop(recovered);
    services.shutdown()?;
    println!("real AOEM lost signing ACK: accepted write survives dropped journal, exact nil prevote recovers, duplicate rejected and next phase durable; no crash/power-loss claim; artifacts {}", directory.display());
    Ok(())
}
