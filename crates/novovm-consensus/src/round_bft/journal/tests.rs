//! Pure ACK-boundary fixtures, not AOEM or business-execution evidence. Value
//! candidates deliberately remain unavailable here; real candidate/chain I/O
//! and process recovery are tested by the product adapter's existing suites.
use super::*;
use backend::{JournalDomain, RecoveredChain};
use metadata::MetadataSnapshot;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

type Reply<T> = Rc<RefCell<Option<Result<T>>>>;
struct Ticket<T>(Reply<T>);
impl<T> JournalTicket<T> for Ticket<T> {
    fn try_take(&mut self) -> Result<Option<T>> {
        self.0.borrow_mut().take().transpose()
    }
}
struct NoRecord;
impl JournalRecord for NoRecord {
    fn parent(&self) -> ParentPoint {
        unreachable!("nil-only fixture")
    }
    fn context(&self) -> ConsensusContext {
        unreachable!("nil-only fixture")
    }
    fn point(&self) -> ParentPoint {
        unreachable!("nil-only fixture")
    }
    fn encode(&self) -> Result<Vec<u8>> {
        anyhow::bail!("nil-only fixture")
    }
    fn head_bytes(&self) -> Result<Vec<u8>> {
        anyhow::bail!("nil-only fixture")
    }
}
impl JournalStatement for NoRecord {
    fn hash(&self) -> Hash {
        unreachable!("no executed candidates in this fixture")
    }
}

struct Backend {
    owner: Arc<()>,
    domain: JournalDomain,
    values: RefCell<BTreeMap<MetaKey, Vec<u8>>>,
    queued: RefCell<VecDeque<(MetaTransition, Reply<MetaOutcome>)>>,
    accept: Cell<bool>,
}
impl JournalBackend for Backend {
    type Candidate = ();
    type Statement = NoRecord;
    type Record = NoRecord;
    type Recovery = ();
    type ReadTicket = Ticket<MetadataSnapshot>;
    type WriteTicket = Ticket<MetaOutcome>;

    fn owner_identity(&self) -> Arc<()> {
        self.owner.clone()
    }
    fn storage_domain(&self) -> JournalDomain {
        self.domain
    }
    fn checked_statement(
        _: &(),
        _: &Arc<()>,
        _: ConsensusContext,
        _: &ValidatorSet,
        _: &ParentPoint,
    ) -> Result<NoRecord> {
        anyhow::bail!("fixture has no durable candidate")
    }
    fn candidate_locator(_: &(), _: Hash) -> CandidateLocator {
        unreachable!("fixture has no durable candidate")
    }
    fn new_record(
        _: &NoRecord,
        _: &(),
        _: ParentPoint,
        _: Hash,
        _: u64,
        _: &[u8],
    ) -> Result<NoRecord> {
        anyhow::bail!("fixture has no chain publication")
    }
    fn begin_recovery(
        _: ConsensusContext,
        _: ParentPoint,
        _: Arc<ValidatorSet>,
        head: Option<Vec<u8>>,
    ) -> Result<()> {
        ensure!(head.is_none(), "fixture cannot authenticate a chain prefix");
        Ok(())
    }
    fn poll_recovery(&self, _: &mut ()) -> Result<Option<RecoveredChain<NoRecord>>> {
        Ok(Some(RecoveredChain {
            record: None,
            head_bytes: None,
        }))
    }
    fn try_read_consensus_metadata(&self, keys: Vec<MetaKey>) -> Result<Option<Self::ReadTicket>> {
        Ok(Some(Ticket(Rc::new(RefCell::new(Some(Ok(
            MetadataSnapshot {
                values: keys
                    .iter()
                    .map(|key| self.values.borrow().get(key).cloned())
                    .collect(),
            },
        )))))))
    }
    fn try_apply_consensus_metadata(
        &self,
        transition: MetaTransition,
    ) -> Result<Option<Self::WriteTicket>> {
        if !self.accept.get() {
            return Ok(None);
        }
        let reply = Rc::new(RefCell::new(None));
        self.queued
            .borrow_mut()
            .push_back((transition, reply.clone()));
        Ok(Some(Ticket(reply)))
    }
}

impl Backend {
    /// Model a single atomic conditional owner operation. Unknown completion
    /// deliberately happens AFTER the write, so reopening must recover it.
    fn complete(&self, unknown_completion: bool) {
        let (transition, reply) = self.queued.borrow_mut().pop_front().unwrap();
        let mut values = self.values.borrow_mut();
        let guards_match = transition
            .guards()
            .iter()
            .all(|guard| values.get(&guard.key) == guard.expected.as_ref());
        let present = transition
            .changes()
            .iter()
            .all(|change| values.get(&change.key) == Some(&change.value));
        let expected = transition
            .changes()
            .iter()
            .all(|change| values.get(&change.key) == change.expected.as_ref());
        let outcome = if !guards_match || (!present && !expected) {
            MetaOutcome::Conflict
        } else if present {
            MetaOutcome::AlreadyPresent
        } else {
            for change in transition.changes() {
                values.insert(change.key.clone(), change.value.clone());
            }
            MetaOutcome::Applied
        };
        *reply.borrow_mut() = Some(if unknown_completion {
            Err(anyhow::anyhow!("unknown owner write completion"))
        } else {
            Ok(outcome)
        });
    }
}

fn fixture() -> (
    Backend,
    ConsensusContext,
    ParentPoint,
    Arc<ValidatorSet>,
    SigningKey,
) {
    let key = SigningKey::from_bytes(&[7; 32]);
    let member = Validator::new(key.verifying_key().to_bytes(), 1).unwrap();
    let set = Arc::new(ValidatorSet::new(71, 1, 1, vec![member]).unwrap());
    let context = ConsensusContext {
        chain_id: 71,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        epoch: 1,
        validator_set_hash: set.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    };
    let parent = ParentPoint {
        height: 0,
        block_hash: [0; 32],
        state_root: [3; 32],
        receipt_batch_commitment: [4; 32],
        state_version: 0,
        decision_hash: [0; 32],
    };
    let backend = Backend {
        owner: Arc::new(()),
        domain: JournalDomain {
            chain_id: 71,
            genesis_config_commitment: [1; 32],
            protocol_commitment: [2; 32],
        },
        values: RefCell::default(),
        queued: RefCell::default(),
        accept: Cell::new(true),
    };
    (backend, context, parent, set, key)
}

fn open(
    backend: &Backend,
    context: ConsensusContext,
    parent: ParentPoint,
    set: &Arc<ValidatorSet>,
    key: &SigningKey,
) -> Result<ValidatorJournal<Backend>> {
    let mut opening = ValidatorJournal::open(backend, context, parent, set.clone(), key.clone())?;
    for _ in 0..8 {
        if let Some(journal) = opening.poll(backend)? {
            return Ok(journal);
        }
    }
    anyhow::bail!("bounded fixture opening did not complete")
}

#[test]
fn pending_signature_is_released_only_after_its_exact_ack_and_once() {
    let (backend, context, parent, set, key) = fixture();
    let mut journal = open(&backend, context, parent, &set, &key).unwrap();
    journal.timeout(0, TimeoutStep::Propose).unwrap();
    assert_eq!(journal.step(), TimeoutStep::Propose);
    assert!(journal.last_durable_message().is_none());
    assert!(journal.timeout(0, TimeoutStep::Propose).is_err());
    assert!(journal.enqueue_pending(&backend).unwrap());
    assert!(journal.enqueue_pending(&backend).unwrap());
    assert_eq!(
        backend.queued.borrow().len(),
        1,
        "one exact in-flight ticket"
    );
    assert!(journal.poll(&backend).unwrap().is_none());
    {
        let queued = backend.queued.borrow();
        let transition = &queued.front().unwrap().0;
        assert_eq!(transition.changes().len(), 2);
        assert!(matches!(
            transition.changes()[0].key,
            MetaKey::ConsensusState(_)
        ));
        assert!(matches!(
            transition.changes()[1].key,
            MetaKey::ConsensusOutbox { sequence: 1, .. }
        ));
        assert_eq!(transition.guards().len(), 1);
        assert_eq!(transition.guards()[0].key, MetaKey::ChainHead);
        assert!(transition.guards()[0].expected.is_none());
    }
    backend.complete(false);
    assert!(
        journal.last_durable_message().is_none(),
        "completion is not an ACK until consumed"
    );
    let Some(Some(DurableMessage::Vote(vote))) = journal.poll(&backend).unwrap() else {
        panic!("missing durable nil vote")
    };
    vote.verify(&set).unwrap();
    assert_eq!(vote.value, None);
    assert_eq!(journal.step(), TimeoutStep::Prevote);
    assert!(journal.poll(&backend).unwrap().is_none());
}

#[test]
fn backpressure_does_not_adopt_or_emit_and_wrong_owner_cannot_submit() {
    let (backend, context, parent, set, key) = fixture();
    let mut journal = open(&backend, context, parent, &set, &key).unwrap();
    journal.timeout(0, TimeoutStep::Propose).unwrap();
    let (other, ..) = fixture();
    assert!(journal.enqueue_pending(&other).is_err());
    assert!(other.queued.borrow().is_empty());
    backend.accept.set(false);
    assert!(!journal.enqueue_pending(&backend).unwrap());
    assert!(journal.poll(&backend).unwrap().is_none());
    assert_eq!(journal.step(), TimeoutStep::Propose);
    assert!(journal.last_durable_message().is_none());
    backend.accept.set(true);
    assert!(journal.poll(&backend).unwrap().is_none());
    backend.complete(false);
    assert!(journal.poll(&backend).unwrap().unwrap().is_some());
}

#[test]
fn stale_expected_state_freezes_without_releasing_a_signature() {
    let (backend, context, parent, set, key) = fixture();
    let mut journal = open(&backend, context, parent, &set, &key).unwrap();
    journal.timeout(0, TimeoutStep::Propose).unwrap();
    journal.enqueue_pending(&backend).unwrap();
    backend
        .values
        .borrow_mut()
        .insert(MetaKey::ConsensusState(journal.local_validator()), vec![99]);
    backend.complete(false);
    assert!(journal.poll(&backend).is_err());
    assert!(journal.is_frozen());
    assert!(!journal.is_pending());
    assert!(journal.last_durable_message().is_none());
    assert!(journal.timeout(0, TimeoutStep::Propose).is_err());
    assert!(journal.enqueue_pending(&backend).is_err());
}

#[test]
fn lost_ack_reopen_recovers_exact_signed_outbox_without_resigning() {
    let (backend, context, parent, set, key) = fixture();
    let mut journal = open(&backend, context, parent, &set, &key).unwrap();
    journal.timeout(0, TimeoutStep::Propose).unwrap();
    journal.enqueue_pending(&backend).unwrap();
    backend.complete(false);
    let before = backend.values.borrow().clone();
    drop(journal); // The durable result never escaped the old journal.
    let mut recovered = open(&backend, context, parent, &set, &key).unwrap();
    assert_eq!(recovered.step(), TimeoutStep::Prevote);
    let Some(DurableMessage::Vote(vote)) = recovered.last_durable_message() else {
        panic!("lost durable message")
    };
    vote.verify(&set).unwrap();
    assert_eq!(vote.round, 0);
    assert_eq!(vote.phase, wire::Phase::Prevote);
    assert_eq!(vote.value, None);
    assert_eq!(recovered.take_replay_records().len(), 1);
    assert!(recovered.timeout(0, TimeoutStep::Propose).is_err());
    assert!(recovered.poll(&backend).unwrap().is_none());
    assert_eq!(*backend.values.borrow(), before);
}

#[test]
fn unknown_completion_freezes_even_when_write_really_happened() {
    let (backend, context, parent, set, key) = fixture();
    let mut journal = open(&backend, context, parent, &set, &key).unwrap();
    journal.timeout(0, TimeoutStep::Propose).unwrap();
    journal.enqueue_pending(&backend).unwrap();
    backend.complete(true);
    assert!(journal.poll(&backend).is_err());
    assert!(journal.is_frozen());
    assert!(journal.last_durable_message().is_none());
    assert!(journal.poll(&backend).is_err());
    // Explicit cold recovery, not clearing a frozen journal in place.
    let recovered = open(&backend, context, parent, &set, &key).unwrap();
    assert_eq!(recovered.step(), TimeoutStep::Prevote);
    assert!(recovered.last_durable_message().is_some());
}

#[test]
fn opening_rejects_missing_or_corrupt_outbox_and_foreign_domain() {
    let (backend, context, parent, set, key) = fixture();
    let mut journal = open(&backend, context, parent, &set, &key).unwrap();
    journal.timeout(0, TimeoutStep::Propose).unwrap();
    journal.enqueue_pending(&backend).unwrap();
    backend.complete(false);
    let key_out = MetaKey::ConsensusOutbox {
        validator: journal.local_validator(),
        sequence: 1,
    };
    let original = backend.values.borrow_mut().remove(&key_out).unwrap();
    assert!(open(&backend, context, parent, &set, &key).is_err());
    let mut corrupt = original;
    corrupt[10] ^= 1;
    backend.values.borrow_mut().insert(key_out, corrupt);
    assert!(open(&backend, context, parent, &set, &key).is_err());
    let mut wrong = context;
    wrong.genesis_config_commitment = [9; 32];
    assert!(open(&backend, wrong, parent, &set, &key).is_err());
}

#[test]
fn exact_already_present_transition_is_replayed_but_stale_head_is_not() {
    let (backend, context, parent, set, key) = fixture();
    let mut first = open(&backend, context, parent, &set, &key).unwrap();
    let mut second = open(&backend, context, parent, &set, &key).unwrap();
    for journal in [&mut first, &mut second] {
        journal.timeout(0, TimeoutStep::Propose).unwrap();
        journal.enqueue_pending(&backend).unwrap();
    }
    backend.complete(false);
    backend.complete(false); // Same bytes: AlreadyPresent, not another vote.
    let first = first.poll(&backend).unwrap().unwrap().unwrap();
    let second = second.poll(&backend).unwrap().unwrap().unwrap();
    let (DurableMessage::Vote(first), DurableMessage::Vote(second)) = (first, second) else {
        panic!("expected votes")
    };
    assert_eq!(first, second);
    assert_eq!(backend.values.borrow().len(), 2);

    // Byte-identical signer changes must still obey the exact head guard.
    let (backend, context, parent, set, key) = fixture();
    let mut journal = open(&backend, context, parent, &set, &key).unwrap();
    journal.timeout(0, TimeoutStep::Propose).unwrap();
    journal.enqueue_pending(&backend).unwrap();
    backend
        .values
        .borrow_mut()
        .insert(MetaKey::ChainHead, vec![1]);
    backend.complete(false);
    assert!(journal.poll(&backend).is_err());
    assert!(journal.is_frozen());
    assert!(journal.last_durable_message().is_none());
}
