use super::*;
use crate::consensus::wire::{Proposal, Quorum, Validator, ValidatorSet, Vote};
use ed25519_dalek::SigningKey;

const A: Hash = [31; 32];
const B: Hash = [32; 32];

// These hashes are deliberately fixture values, not execution proofs. The
// tests exercise the safety kernel with real signed evidence. They do not
// exercise a durable signer, network, AOEM execution or complete pacemaker.
struct Fixture {
    keys: Vec<SigningKey>,
    set: ValidatorSet,
    context: Context,
}

impl Fixture {
    fn new() -> Self {
        let keys: Vec<_> = (1u8..=4)
            .map(|seed| SigningKey::from_bytes(&[seed; 32]))
            .collect();
        let validators = keys
            .iter()
            .map(|key| Validator::new(key.verifying_key().to_bytes(), 1).unwrap())
            .collect();
        let set = ValidatorSet::new(7, 1, 1, validators).unwrap();
        let context = Context {
            chain_id: 7,
            genesis_config_commitment: [3; 32],
            protocol_commitment: [4; 32],
            epoch: 1,
            validator_set_hash: set.hash(),
            height: 1,
            parent_block_hash: [0; 32],
            parent_decision_hash: [0; 32],
        };
        Self { keys, set, context }
    }

    fn proposal_at(
        &self,
        context: Context,
        round: u64,
        value: Hash,
        valid_round: Option<u64>,
    ) -> VerifiedProposal {
        let leader = self.set.leader(context.height, round).unwrap();
        let key = self
            .keys
            .iter()
            .find(|key| {
                Validator::new(key.verifying_key().to_bytes(), 1)
                    .unwrap()
                    .id()
                    == leader
            })
            .unwrap();
        Proposal::sign(context, round, value, valid_round, &self.set, key)
            .unwrap()
            .verify(&self.set)
            .unwrap()
    }

    fn proposal(&self, round: u64, value: Hash, valid_round: Option<u64>) -> VerifiedProposal {
        self.proposal_at(self.context, round, value, valid_round)
    }

    fn quorum_at(
        &self,
        context: Context,
        round: u64,
        phase: Phase,
        value: Option<Hash>,
    ) -> VerifiedQuorum {
        let votes = self
            .keys
            .iter()
            .take(3)
            .map(|key| Vote::sign(context, round, phase, value, &self.set, key).unwrap())
            .collect();
        Quorum::from_votes(&self.set, votes)
            .unwrap()
            .verify(&self.set)
            .unwrap()
    }

    fn quorum(&self, round: u64, phase: Phase, value: Option<Hash>) -> VerifiedQuorum {
        self.quorum_at(self.context, round, phase, value)
    }

    fn intents_quorum(&self, intents: &[VoteIntent]) -> VerifiedQuorum {
        assert_eq!(intents.len(), 3);
        let votes = intents
            .iter()
            .zip(&self.keys)
            .map(|(intent, key)| {
                Vote::sign(
                    intent.context,
                    intent.round,
                    intent.phase,
                    intent.value,
                    &self.set,
                    key,
                )
                .unwrap()
            })
            .collect();
        Quorum::from_votes(&self.set, votes)
            .unwrap()
            .verify(&self.set)
            .unwrap()
    }

    fn initial(&self) -> State {
        State::new(self.context).unwrap()
    }

    fn locked(&self, round: u64, value: Hash) -> State {
        let state = advance_to(self.initial(), round);
        let proposal = self.proposal(round, value, None);
        let state = state
            .prepare_proposal(&proposal, value, None)
            .unwrap()
            .into_parts()
            .0;
        let quorum = self.quorum(round, Phase::Prevote, Some(value));
        state
            .prepare_prevote_quorum(&quorum, Some((&proposal, value)))
            .unwrap()
            .into_parts()
            .0
    }
}

fn timeout_event(state: &State) -> LocalTimeout {
    LocalTimeout {
        context: *state.context(),
        round: state.round(),
        step: state.step(),
    }
}

fn timeout(state: &State) -> PreparedStep {
    state.prepare_timeout(timeout_event(state)).unwrap()
}

fn advance_to(mut state: State, round: u64) -> State {
    while state.round() < round {
        state = timeout(&state).into_parts().0;
    }
    assert_eq!(state.round(), round);
    assert_eq!(state.step(), Step::Propose);
    state
}

fn assert_vote(step: &PreparedStep, phase: Phase, value: Option<Hash>) {
    let intent = step.intent().expect("one prepared vote intent");
    assert_eq!(intent.phase, phase);
    assert_eq!(intent.value, value);
    assert_eq!(intent.round, step.next().round());
    assert_eq!(intent.context, *step.next().context());
    assert!(step.decision().is_none());
}

#[test]
fn authenticated_round_change_preserves_lock_valid_and_never_invents_votes() {
    let fixture = Fixture::new();
    let original = fixture.locked(2, A);
    let step = original.prepare_round_change(6).unwrap();
    assert_eq!(original.round(), 2);
    assert_eq!(step.next().round(), 6);
    assert_eq!(step.next().step(), Step::Propose);
    assert_eq!(step.next().locked(), original.locked());
    assert_eq!(step.next().valid(), original.valid());
    assert!(step.intent().is_none() && step.decision().is_none());
    assert!(original.prepare_round_change(2).is_err());
    assert!(original.prepare_round_change(1).is_err());
    let proposal = fixture.proposal(6, B, None);
    assert_vote(
        &step.next().prepare_proposal(&proposal, B, None).unwrap(),
        Phase::Prevote,
        None,
    );
    let encoded = step.next().encode().unwrap();
    assert_eq!(
        State::restore(&encoded, &fixture.context).unwrap(),
        *step.next()
    );
}

#[test]
fn no_quorum_a_a_b_split_does_not_lock_and_can_converge_next_round() {
    let fixture = Fixture::new();
    let mut states = Vec::new();
    for value in [A, A, B] {
        // One test leader equivocates. These first votes do not form any QC.
        let proposal = fixture.proposal(0, value, None);
        let step = fixture
            .initial()
            .prepare_proposal(&proposal, value, None)
            .unwrap();
        assert_vote(&step, Phase::Prevote, Some(value));
        assert_eq!(step.next().locked(), None);
        assert_eq!(step.next().valid(), None);
        let state = step.into_parts().0;
        let step = timeout(&state);
        assert_vote(&step, Phase::Precommit, None);
        let state = step.into_parts().0;
        states.push(advance_to(state, 1));
    }
    let proposal = fixture.proposal(1, B, None);
    let mut prevotes = Vec::new();
    for state in &mut states {
        let (next, intent, decision) = state
            .prepare_proposal(&proposal, B, None)
            .unwrap()
            .into_parts();
        assert!(decision.is_none());
        assert_eq!(next.locked(), None);
        prevotes.push(intent.unwrap());
        *state = next;
    }
    let prevote_qc = fixture.intents_quorum(&prevotes);
    let mut precommits = Vec::new();
    for state in &mut states {
        let (next, intent, decision) = state
            .prepare_prevote_quorum(&prevote_qc, Some((&proposal, B)))
            .unwrap()
            .into_parts();
        assert!(decision.is_none());
        assert_eq!(next.locked(), Some((1, B)));
        assert_eq!(next.valid(), Some((1, B)));
        precommits.push(intent.unwrap());
        *state = next;
    }
    let decision_qc = fixture.intents_quorum(&precommits);
    for state in states {
        let step = state.prepare_decision(&proposal, B, &decision_qc).unwrap();
        assert_eq!(step.decision(), Some(B));
        assert!(step.intent().is_none());
        assert_eq!(step.next().decided(), Some(B));
    }
}

#[test]
fn preparing_transition_does_not_mutate_or_grant_durable_signing_authority() {
    let fixture = Fixture::new();
    let state = fixture.initial();
    let before = state.encode().unwrap();
    let proposal = fixture.proposal(0, A, None);
    let step = state.prepare_proposal(&proposal, A, None).unwrap();
    assert_eq!(state.encode().unwrap(), before);
    assert_eq!(step.next().step(), Step::Prevote);
    // Pure preparation may be repeated from the same snapshot. The outer
    // expected-state CAS/outbox, not this immutable object, prevents signing
    // twice from a cloned old snapshot.
    let repeat = state.prepare_proposal(&proposal, A, None).unwrap();
    assert_eq!(step.intent(), repeat.intent());
    assert!(step.next().prepare_proposal(&proposal, A, None).is_err());
}

#[test]
fn locked_value_rejects_unjustified_change_but_accepts_same_value() {
    let fixture = Fixture::new();
    let state = advance_to(fixture.locked(0, A), 1);
    let different = fixture.proposal(1, B, None);
    let step = state.prepare_proposal(&different, B, None).unwrap();
    assert_vote(&step, Phase::Prevote, None);
    assert_eq!(step.next().locked(), Some((0, A)));
    let same = fixture.proposal(1, A, None);
    let step = state.prepare_proposal(&same, A, None).unwrap();
    assert_vote(&step, Phase::Prevote, Some(A));
    assert_eq!(step.next().locked(), Some((0, A)));
}

#[test]
fn old_justification_cannot_override_newer_lock_or_invent_equal_round_value() {
    let fixture = Fixture::new();
    let state = advance_to(fixture.locked(1, A), 2);
    let older = fixture.proposal(2, B, Some(0));
    let older_qc = fixture.quorum(0, Phase::Prevote, Some(B));
    let step = state.prepare_proposal(&older, B, Some(&older_qc)).unwrap();
    assert_vote(&step, Phase::Prevote, None);
    assert_eq!(step.next().locked(), Some((1, A)));
    // The fixture deliberately signs contradictory same-round certificates;
    // this is outside the <1/3 Byzantine assumption, but must fail closed.
    let conflict = fixture.proposal(2, B, Some(1));
    let conflict_qc = fixture.quorum(1, Phase::Prevote, Some(B));
    assert!(state
        .prepare_proposal(&conflict, B, Some(&conflict_qc))
        .is_err());
}

#[test]
fn newer_verified_prevote_quorum_allows_lock_migration_only_when_current_qc_arrives() {
    let fixture = Fixture::new();
    let state = advance_to(fixture.locked(0, A), 2);
    let proof = fixture.quorum(1, Phase::Prevote, Some(B));
    let proposal = fixture.proposal(2, B, Some(1));
    let step = state.prepare_proposal(&proposal, B, Some(&proof)).unwrap();
    assert_vote(&step, Phase::Prevote, Some(B));
    assert_eq!(step.next().locked(), Some((0, A)));
    assert_eq!(step.next().valid(), Some((0, A)));
    let state = step.into_parts().0;
    let current_qc = fixture.quorum(2, Phase::Prevote, Some(B));
    let step = state
        .prepare_prevote_quorum(&current_qc, Some((&proposal, B)))
        .unwrap();
    assert_vote(&step, Phase::Precommit, Some(B));
    assert_eq!(step.next().locked(), Some((2, B)));
    assert_eq!(step.next().valid(), Some((2, B)));
}

#[test]
fn justification_requires_exact_domain_round_phase_value_and_presence() {
    let fixture = Fixture::new();
    let state = advance_to(fixture.initial(), 2);
    let proposal = fixture.proposal(2, B, Some(1));
    assert!(state.prepare_proposal(&proposal, B, None).is_err());
    for proof in [
        fixture.quorum(0, Phase::Prevote, Some(B)),
        fixture.quorum(1, Phase::Precommit, Some(B)),
        fixture.quorum(1, Phase::Prevote, Some(A)),
        fixture.quorum(1, Phase::Prevote, None),
        fixture.quorum_at(
            Context {
                protocol_commitment: [99; 32],
                ..fixture.context
            },
            1,
            Phase::Prevote,
            Some(B),
        ),
    ] {
        assert!(state.prepare_proposal(&proposal, B, Some(&proof)).is_err());
    }
    let proof = fixture.quorum(1, Phase::Prevote, Some(B));
    let no_claim = fixture.proposal(2, B, None);
    assert!(state.prepare_proposal(&no_claim, B, Some(&proof)).is_err());
    assert!(state.prepare_proposal(&proposal, B, Some(&proof)).is_ok());
    let leader = fixture.set.leader(1, 2).unwrap();
    let key = fixture
        .keys
        .iter()
        .find(|key| {
            Validator::new(key.verifying_key().to_bytes(), 1)
                .unwrap()
                .id()
                == leader
        })
        .unwrap();
    assert!(Proposal::sign(fixture.context, 2, B, Some(2), &fixture.set, key).is_err());
    assert!(Proposal::sign(fixture.context, 2, B, Some(3), &fixture.set, key).is_err());
}

#[test]
fn nil_quorum_and_all_timeouts_preserve_existing_lock() {
    let fixture = Fixture::new();
    let state = advance_to(fixture.locked(0, A), 1);
    let step = timeout(&state);
    assert_vote(&step, Phase::Prevote, None);
    let state = step.into_parts().0;
    let nil = fixture.quorum(1, Phase::Prevote, None);
    let step = state.prepare_prevote_quorum(&nil, None).unwrap();
    assert_vote(&step, Phase::Precommit, None);
    assert_eq!(step.next().locked(), Some((0, A)));
    let state = step.into_parts().0;
    assert!(state.prepare_prevote_quorum(&nil, None).is_err());
    let next = timeout(&state);
    assert!(next.intent().is_none());
    assert_eq!(next.next().round(), 2);
    assert_eq!(next.next().locked(), Some((0, A)));
    let mut state = next.into_parts().0;
    for _ in 0..3 {
        state = timeout(&state).into_parts().0;
        assert_eq!(state.locked(), Some((0, A)));
        assert_eq!(state.valid(), Some((0, A)));
    }
}

#[test]
fn late_current_quorum_after_nil_precommit_updates_valid_without_second_vote() {
    let fixture = Fixture::new();
    let state = advance_to(fixture.locked(0, A), 1);
    let state = timeout(&state).into_parts().0;
    let state = timeout(&state).into_parts().0;
    assert_eq!(state.step(), Step::Precommit);
    let proposal = fixture.proposal(1, B, None);
    let proof = fixture.quorum(1, Phase::Prevote, Some(B));
    let step = state
        .prepare_prevote_quorum(&proof, Some((&proposal, B)))
        .unwrap();
    assert!(step.intent().is_none());
    assert_eq!(step.next().locked(), Some((0, A)));
    assert_eq!(step.next().valid(), Some((1, B)));
    let state = step.into_parts().0;
    assert!(state
        .prepare_prevote_quorum(&proof, Some((&proposal, B)))
        .is_err());
    let state = advance_to(state, 2);
    let proposal = fixture.proposal(2, B, Some(1));
    let step = state.prepare_proposal(&proposal, B, Some(&proof)).unwrap();
    assert_vote(&step, Phase::Prevote, Some(B));
    assert_eq!(step.next().locked(), Some((0, A)));
}

#[test]
fn current_quorum_can_be_processed_after_equivocating_proposal_but_not_without_execution() {
    let fixture = Fixture::new();
    let first = fixture.proposal(0, A, None);
    let state = fixture
        .initial()
        .prepare_proposal(&first, A, None)
        .unwrap()
        .into_parts()
        .0;
    let different = fixture.proposal(0, B, None);
    let proof = fixture.quorum(0, Phase::Prevote, Some(B));
    assert!(state.prepare_prevote_quorum(&proof, None).is_err());
    assert!(state
        .prepare_prevote_quorum(&proof, Some((&different, A)))
        .is_err());
    assert!(state
        .prepare_prevote_quorum(&proof, Some((&first, A)))
        .is_err());
    let step = state
        .prepare_prevote_quorum(&proof, Some((&different, B)))
        .unwrap();
    assert_vote(&step, Phase::Precommit, Some(B));
    assert_eq!(step.next().locked(), Some((0, B)));
}

#[test]
fn duplicate_and_old_round_quorums_cannot_create_more_votes() {
    let fixture = Fixture::new();
    let proposal = fixture.proposal(0, A, None);
    let proof = fixture.quorum(0, Phase::Prevote, Some(A));
    let state = fixture.locked(0, A);
    assert!(state
        .prepare_prevote_quorum(&proof, Some((&proposal, A)))
        .is_err());
    let conflicting = fixture.proposal(0, B, None);
    let conflicting_qc = fixture.quorum(0, Phase::Prevote, Some(B));
    assert!(state
        .prepare_prevote_quorum(&conflicting_qc, Some((&conflicting, B)))
        .is_err());
    let state = advance_to(state, 1);
    assert!(state.prepare_proposal(&proposal, A, None).is_err());
    let state = timeout(&state).into_parts().0;
    assert!(state
        .prepare_prevote_quorum(&proof, Some((&proposal, A)))
        .is_err());
    let nil_precommit = fixture.quorum(1, Phase::Precommit, None);
    assert!(state.prepare_prevote_quorum(&nil_precommit, None).is_err());
}

#[test]
fn local_timeout_requires_exact_context_round_step_and_checked_increment() {
    let fixture = Fixture::new();
    let state = fixture.initial();
    for event in [
        LocalTimeout {
            context: fixture.context,
            round: 1,
            step: Step::Propose,
        },
        LocalTimeout {
            context: fixture.context,
            round: 0,
            step: Step::Prevote,
        },
        LocalTimeout {
            context: Context {
                genesis_config_commitment: [99; 32],
                ..fixture.context
            },
            round: 0,
            step: Step::Propose,
        },
    ] {
        assert!(state.prepare_timeout(event).is_err());
    }
    let state = timeout(&state).into_parts().0;
    assert!(state
        .prepare_timeout(LocalTimeout {
            context: fixture.context,
            round: 0,
            step: Step::Propose
        })
        .is_err());
    let state = timeout(&state).into_parts().0;
    let step = timeout(&state);
    assert_eq!(step.next().round(), 1);
    assert_eq!(step.next().step(), Step::Propose);
    assert!(step.intent().is_none());
    let mut exhausted = fixture.initial();
    exhausted.round = u64::MAX;
    exhausted.step = Step::Precommit;
    assert!(exhausted
        .prepare_timeout(timeout_event(&exhausted))
        .is_err());
}

#[test]
fn cross_context_evidence_is_rejected_even_when_signed_by_same_set() {
    let fixture = Fixture::new();
    let state = fixture.initial();
    let altered = [
        Context {
            genesis_config_commitment: [51; 32],
            ..fixture.context
        },
        Context {
            protocol_commitment: [52; 32],
            ..fixture.context
        },
        Context {
            height: 2,
            parent_block_hash: [53; 32],
            parent_decision_hash: [54; 32],
            ..fixture.context
        },
    ];
    let prevote = timeout(&state).into_parts().0;
    for context in altered {
        let proposal = fixture.proposal_at(context, 0, A, None);
        let quorum = fixture.quorum_at(context, 0, Phase::Prevote, Some(A));
        assert!(state.prepare_proposal(&proposal, A, None).is_err());
        assert!(prevote
            .prepare_prevote_quorum(&quorum, Some((&proposal, A)))
            .is_err());
        let decision = fixture.quorum_at(context, 0, Phase::Precommit, Some(A));
        assert!(state.prepare_decision(&proposal, A, &decision).is_err());
    }
}

#[test]
fn proposal_local_value_mismatch_does_not_change_state() {
    let fixture = Fixture::new();
    let state = fixture.initial();
    let proposal = fixture.proposal(0, A, None);
    let bytes = state.encode().unwrap();
    assert!(state.prepare_proposal(&proposal, B, None).is_err());
    assert!(state.prepare_proposal(&proposal, [0; 32], None).is_err());
    assert_eq!(state.encode().unwrap(), bytes);
    assert_eq!(state.step(), Step::Propose);
}

#[test]
fn only_non_nil_precommit_certificate_and_exact_proposal_can_decide() {
    let fixture = Fixture::new();
    let state = fixture.initial();
    let proposal = fixture.proposal(0, A, None);
    for proof in [
        fixture.quorum(0, Phase::Prevote, Some(A)),
        fixture.quorum(0, Phase::Precommit, None),
        fixture.quorum(0, Phase::Precommit, Some(B)),
        fixture.quorum(1, Phase::Precommit, Some(A)),
    ] {
        assert!(state.prepare_decision(&proposal, A, &proof).is_err());
    }
    let proof = fixture.quorum(0, Phase::Precommit, Some(A));
    assert!(state.prepare_decision(&proposal, B, &proof).is_err());
    let step = state.prepare_decision(&proposal, A, &proof).unwrap();
    assert_eq!(step.decision(), Some(A));
    assert!(step.intent().is_none());
    assert_eq!(state.decided(), None);
}

#[test]
fn decision_can_arrive_for_older_round_without_old_vote_and_is_terminal() {
    let fixture = Fixture::new();
    let state = advance_to(fixture.initial(), 3);
    let proposal = fixture.proposal(0, A, None);
    let proof = fixture.quorum(0, Phase::Precommit, Some(A));
    let step = state.prepare_decision(&proposal, A, &proof).unwrap();
    assert!(step.intent().is_none());
    assert_eq!(step.decision(), Some(A));
    let state = step.into_parts().0;
    assert_eq!(state.round(), 3);
    assert_eq!(state.decided(), Some(A));
    assert!(state.prepare_decision(&proposal, A, &proof).is_err());
    assert!(state.prepare_timeout(timeout_event(&state)).is_err());
    let current = fixture.proposal(3, B, None);
    assert!(state.prepare_proposal(&current, B, None).is_err());
    let current_qc = fixture.quorum(3, Phase::Prevote, Some(B));
    assert!(state
        .prepare_prevote_quorum(&current_qc, Some((&current, B)))
        .is_err());
    assert_eq!(
        State::restore(&state.encode().unwrap(), &fixture.context).unwrap(),
        state
    );
}

fn snapshot(state: &State) -> Snapshot {
    Snapshot {
        context: state.context,
        round: state.round,
        step: state.step,
        locked: state.locked,
        valid: state.valid,
        decided: state.decided,
    }
}

fn encode_unchecked(snapshot: Snapshot) -> Vec<u8> {
    let mut bytes = SNAPSHOT_PREFIX.to_vec();
    bytes.extend_from_slice(&postcard::to_allocvec(&snapshot).unwrap());
    bytes
}

#[test]
fn snapshot_roundtrip_preserves_locks_validity_steps_and_is_canonical_bounded() {
    let fixture = Fixture::new();
    let initial = fixture.initial();
    let prevote = timeout(&initial).into_parts().0;
    let locked = fixture.locked(0, A);
    let later = advance_to(locked.clone(), 3);
    for state in [initial, prevote, locked, later] {
        let bytes = state.encode().unwrap();
        assert!(bytes.len() <= MAX_SNAPSHOT_BYTES);
        assert_eq!(State::restore(&bytes, &fixture.context).unwrap(), state);
        assert_eq!(
            State::restore(&bytes, &fixture.context)
                .unwrap()
                .encode()
                .unwrap(),
            bytes
        );
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(State::restore(&trailing, &fixture.context).is_err());
        assert!(State::restore(&bytes[..bytes.len() - 1], &fixture.context).is_err());
        let mut prefix = bytes.clone();
        prefix[0] ^= 1;
        assert!(State::restore(&prefix, &fixture.context).is_err());
        assert!(State::restore(
            &bytes,
            &Context {
                protocol_commitment: [88; 32],
                ..fixture.context
            }
        )
        .is_err());
    }
    assert!(State::restore(&[], &fixture.context).is_err());
    assert!(State::restore(&vec![0; MAX_SNAPSHOT_BYTES + 1], &fixture.context).is_err());
    // Non-minimal varint for first scalar chain_id=7. Even if the underlying
    // postcard decoder accepts it, canonical re-encoding must reject it.
    let mut bytes = fixture.initial().encode().unwrap();
    assert_eq!(bytes[SNAPSHOT_PREFIX.len()], 7);
    bytes.splice(SNAPSHOT_PREFIX.len()..SNAPSHOT_PREFIX.len() + 1, [0x87, 0]);
    assert!(State::restore(&bytes, &fixture.context).is_err());
}

#[test]
fn snapshot_rejects_invalid_checkpoint_pairs_and_early_locks() {
    let fixture = Fixture::new();
    let locked = fixture.locked(1, A);
    let mut invalid = Vec::new();
    let mut item = snapshot(&locked);
    item.valid = None;
    invalid.push(item);
    let mut item = snapshot(&locked);
    item.locked.as_mut().unwrap().round = 2;
    invalid.push(item);
    let mut item = snapshot(&locked);
    item.valid.as_mut().unwrap().round = 2;
    invalid.push(item);
    let mut item = snapshot(&locked);
    item.valid.as_mut().unwrap().round = 0;
    invalid.push(item);
    let mut item = snapshot(&locked);
    item.valid.as_mut().unwrap().value = B;
    invalid.push(item);
    let mut item = snapshot(&locked);
    item.locked.as_mut().unwrap().value = [0; 32];
    invalid.push(item);
    let mut item = snapshot(&locked);
    item.valid.as_mut().unwrap().value = [0; 32];
    invalid.push(item);
    let mut item = snapshot(&locked);
    item.step = Step::Propose;
    invalid.push(item);
    let mut item = snapshot(&locked);
    item.step = Step::Prevote;
    invalid.push(item);
    let mut item = snapshot(&locked);
    item.decided = Some([0; 32]);
    invalid.push(item);
    for item in invalid {
        assert!(State::restore(&encode_unchecked(item), &fixture.context).is_err());
    }
}

#[test]
fn new_and_restore_reject_invalid_context_domain_and_parent_convention() {
    let fixture = Fixture::new();
    let contexts = [
        Context {
            chain_id: 0,
            ..fixture.context
        },
        Context {
            epoch: 0,
            ..fixture.context
        },
        Context {
            height: 0,
            ..fixture.context
        },
        Context {
            genesis_config_commitment: [0; 32],
            ..fixture.context
        },
        Context {
            protocol_commitment: [0; 32],
            ..fixture.context
        },
        Context {
            validator_set_hash: [0; 32],
            ..fixture.context
        },
        Context {
            parent_block_hash: [1; 32],
            ..fixture.context
        },
        Context {
            parent_decision_hash: [1; 32],
            ..fixture.context
        },
        Context {
            height: 2,
            ..fixture.context
        },
        Context {
            height: 2,
            parent_block_hash: [1; 32],
            ..fixture.context
        },
        Context {
            height: 2,
            parent_decision_hash: [1; 32],
            ..fixture.context
        },
    ];
    for context in contexts {
        assert!(State::new(context).is_err());
        let mut item = snapshot(&fixture.initial());
        item.context = context;
        assert!(State::restore(&encode_unchecked(item), &context).is_err());
    }
    let successor = Context {
        height: 2,
        parent_block_hash: [1; 32],
        parent_decision_hash: [2; 32],
        ..fixture.context
    };
    assert!(State::new(successor).is_ok());
}
