//! Source: `runtime/novovm-host/src/consensus/collector/tests.rs` at `2da3583c`.
//! Migrated rules/tests; no production signing or publication is activated here.
//!
use super::*;
use crate::round_bft::wire::Validator;
use ed25519_dalek::SigningKey;

const A: Hash = [31; 32];
const B: Hash = [32; 32];

struct Fixture {
    keys: Vec<SigningKey>,
    set: Arc<ValidatorSet>,
    context: Context,
}

impl Fixture {
    fn new(weights: &[u64]) -> Self {
        let keys: Vec<_> = (1..=weights.len())
            .map(|seed| SigningKey::from_bytes(&[seed as u8; 32]))
            .collect();
        let members = keys
            .iter()
            .zip(weights)
            .map(|(key, weight)| Validator::new(key.verifying_key().to_bytes(), *weight).unwrap())
            .collect();
        let set = Arc::new(ValidatorSet::new(7, 1, 1, members).unwrap());
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

    fn collector(&self) -> VoteCollector {
        self.with_limits(CollectorLimits {
            max_retained_rounds: 8,
            max_future_round_span: 8,
            max_votes: self.keys.len() * 2 * 8,
        })
    }

    fn with_limits(&self, limits: CollectorLimits) -> VoteCollector {
        VoteCollector::new(self.context, Arc::clone(&self.set), 0, limits).unwrap()
    }

    fn vote(&self, signer: usize, round: u64, phase: Phase, value: Option<Hash>) -> Vote {
        Vote::sign(
            self.context,
            round,
            phase,
            value,
            &self.set,
            &self.keys[signer],
        )
        .unwrap()
    }

    fn successor(&self) -> Context {
        Context {
            height: 2,
            parent_block_hash: [8; 32],
            parent_decision_hash: [9; 32],
            ..self.context
        }
    }
}

fn rejected(result: Result<VoteInsert>, fragment: &str) {
    assert!(result.unwrap_err().to_string().contains(fragment));
}

#[test]
fn two_of_four_do_not_certify_three_same_value_do() {
    let f = Fixture::new(&[1; 4]);
    let mut collector = f.collector();
    for i in 0..2 {
        collector
            .insert(&f.vote(i, 0, Phase::Prevote, Some(A)))
            .unwrap();
    }
    assert_eq!(collector.phase_weight(0, Phase::Prevote), 2);
    assert!(!collector.timeout_eligible(0, Phase::Prevote));
    assert!(collector
        .quorum(0, Phase::Prevote, Some(A))
        .unwrap()
        .is_none());
    collector
        .insert(&f.vote(2, 0, Phase::Prevote, Some(A)))
        .unwrap();
    let qc = collector
        .quorum(0, Phase::Prevote, Some(A))
        .unwrap()
        .unwrap();
    assert_eq!(qc.signed_weight(), 3);
    assert_eq!(qc.context(), &f.context);
    assert_eq!(qc.round(), 0);
    assert_eq!(qc.phase(), Phase::Prevote);
    assert_eq!(qc.value(), Some(A));
    qc.quorum().verify(&f.set).unwrap();
    assert!(collector.timeout_eligible(0, Phase::Prevote));
}

#[test]
fn mixed_values_only_qualify_phase_timeout_and_never_a_block_quorum() {
    let f = Fixture::new(&[1; 4]);
    let mut collector = f.collector();
    for (i, value) in [Some(A), Some(B), None].into_iter().enumerate() {
        collector
            .insert(&f.vote(i, 0, Phase::Prevote, value))
            .unwrap();
    }
    assert!(collector.timeout_eligible(0, Phase::Prevote));
    for value in [Some(A), Some(B), None] {
        assert!(collector
            .quorum(0, Phase::Prevote, value)
            .unwrap()
            .is_none());
    }
    for i in 0..3 {
        collector
            .insert(&f.vote(i, 0, Phase::Precommit, None))
            .unwrap();
    }
    let nil_qc = collector
        .quorum(0, Phase::Precommit, None)
        .unwrap()
        .unwrap();
    assert_eq!(nil_qc.value(), None);
    assert!(collector.timeout_eligible(0, Phase::Precommit));
    assert!(collector
        .quorum(0, Phase::Precommit, Some(A))
        .unwrap()
        .is_none());
}

#[test]
fn duplicate_and_equivocation_never_add_weight_or_overwrite_the_first_vote() {
    let f = Fixture::new(&[1; 4]);
    let mut collector = f.collector();
    let first = f.vote(0, 0, Phase::Prevote, Some(A));
    assert_eq!(collector.insert(&first).unwrap(), VoteInsert::Inserted);
    assert_eq!(collector.insert(&first).unwrap(), VoteInsert::Duplicate);
    for value in [Some(B), None] {
        rejected(
            collector.insert(&f.vote(0, 0, Phase::Prevote, value)),
            "equivocation",
        );
    }
    assert_eq!(collector.vote_count(), 1);
    assert_eq!(collector.phase_weight(0, Phase::Prevote), 1);
    for i in 1..3 {
        collector
            .insert(&f.vote(i, 0, Phase::Prevote, Some(A)))
            .unwrap();
    }
    let qc = collector
        .quorum(0, Phase::Prevote, Some(A))
        .unwrap()
        .unwrap();
    assert!(qc.quorum().votes.contains(&first));
}

#[test]
fn phases_and_rounds_never_combine_into_quorum_or_timeout_weight() {
    let f = Fixture::new(&[1; 4]);
    let mut collector = f.collector();
    for i in 0..2 {
        collector
            .insert(&f.vote(i, 0, Phase::Prevote, Some(A)))
            .unwrap();
        collector
            .insert(&f.vote(i + 2, 0, Phase::Precommit, Some(A)))
            .unwrap();
        collector
            .insert(&f.vote(i + 2, 1, Phase::Prevote, Some(A)))
            .unwrap();
    }
    for (round, phase) in [
        (0, Phase::Prevote),
        (0, Phase::Precommit),
        (1, Phase::Prevote),
    ] {
        assert_eq!(collector.phase_weight(round, phase), 2);
        assert!(collector.quorum(round, phase, Some(A)).unwrap().is_none());
        assert!(!collector.timeout_eligible(round, phase));
    }
}

#[test]
fn weighted_thresholds_are_strict_and_count_weight_not_signer_count() {
    let f = Fixture::new(&[3, 3, 2, 1]);
    let mut collector = f.collector();
    for i in [0, 1] {
        collector
            .insert(&f.vote(i, 0, Phase::Prevote, Some(A)))
            .unwrap();
    }
    assert_eq!(f.set.total_weight(), 9);
    assert_eq!(collector.phase_weight(0, Phase::Prevote), 6);
    assert!(!collector.timeout_eligible(0, Phase::Prevote));
    assert!(collector
        .quorum(0, Phase::Prevote, Some(A))
        .unwrap()
        .is_none());
    collector
        .insert(&f.vote(3, 0, Phase::Prevote, Some(A)))
        .unwrap();
    assert_eq!(
        collector
            .quorum(0, Phase::Prevote, Some(A))
            .unwrap()
            .unwrap()
            .signed_weight(),
        7
    );
    collector
        .insert(&f.vote(0, 3, Phase::Precommit, None))
        .unwrap();
    assert!(collector.catch_up().is_none()); // Exactly one-third is insufficient.
    collector
        .insert(&f.vote(3, 3, Phase::Prevote, Some(B)))
        .unwrap();
    assert_eq!(collector.catch_up().unwrap().signed_weight(), 4);
}

#[test]
fn high_round_catch_up_deduplicates_phases_and_rejects_cross_round_fragments() {
    let f = Fixture::new(&[1; 4]);
    let mut collector = f.collector();
    collector
        .insert(&f.vote(0, 4, Phase::Prevote, Some(A)))
        .unwrap();
    collector
        .insert(&f.vote(0, 4, Phase::Precommit, None))
        .unwrap();
    assert!(collector.catch_up().is_none());
    collector
        .insert(&f.vote(1, 5, Phase::Prevote, Some(B)))
        .unwrap();
    assert!(collector.catch_up().is_none());
    collector
        .insert(&f.vote(1, 4, Phase::Precommit, None))
        .unwrap();
    let evidence = collector.catch_up().unwrap();
    assert_eq!(evidence.context(), &f.context);
    assert_eq!(evidence.round(), 4);
    assert_eq!(evidence.signed_weight(), 2);
    assert_eq!(evidence.votes().len(), 2);
    assert!(evidence.votes().iter().all(|vote| vote.round == 4));
    collector
        .insert(&f.vote(2, 5, Phase::Prevote, None))
        .unwrap();
    assert_eq!(collector.catch_up().unwrap().round(), 5);
    assert_eq!(collector.current_round(), 0); // A recommendation changes no state.
    collector.advance_round(5).unwrap();
    assert!(collector.catch_up().is_none());
}

#[test]
fn unequal_weight_catch_up_may_have_one_heavy_signer_but_never_two_light_phases() {
    let f = Fixture::new(&[4, 2, 2, 1]);
    let mut collector = f.collector();
    collector
        .insert(&f.vote(3, 1, Phase::Prevote, None))
        .unwrap();
    collector
        .insert(&f.vote(3, 1, Phase::Precommit, None))
        .unwrap();
    assert!(collector.catch_up().is_none());
    collector
        .insert(&f.vote(0, 2, Phase::Prevote, Some(A)))
        .unwrap();
    let evidence = collector.catch_up().unwrap();
    assert_eq!(evidence.round(), 2);
    assert_eq!(evidence.signed_weight(), 4);
    assert_eq!(evidence.votes().len(), 1);
}

#[test]
fn forged_signatures_and_verified_votes_from_another_set_are_not_trusted() {
    let f = Fixture::new(&[1; 4]);
    let other = Fixture::new(&[1, 1, 1, 2]);
    let mut collector = f.collector();
    let verified_elsewhere = other
        .vote(0, 0, Phase::Prevote, Some(A))
        .verify(&other.set)
        .unwrap();
    rejected(
        collector.insert(verified_elsewhere.vote()),
        "context mismatch",
    );
    let mut relabeled = verified_elsewhere.vote().clone();
    relabeled.context = f.context;
    rejected(
        collector.insert(&relabeled),
        "invalid round-bft vote signature",
    );
    let good = f.vote(0, 0, Phase::Prevote, Some(A));
    collector.insert(&good).unwrap();
    let mut fake_duplicate = good.clone();
    fake_duplicate.signature[0] ^= 1;
    rejected(
        collector.insert(&fake_duplicate),
        "invalid round-bft vote signature",
    );
    fake_duplicate.signature.clear();
    rejected(collector.insert(&fake_duplicate), "exactly 64 bytes");
    assert_eq!(collector.vote_count(), 1);
}

#[test]
fn every_context_field_is_pinned_and_context_advancement_rejects_old_votes() {
    let f = Fixture::new(&[1; 4]);
    let mut collector = f.collector();
    let old = f.vote(0, 0, Phase::Prevote, Some(A));
    collector.insert(&old).unwrap();
    for mutate in 0..8 {
        let mut forged = old.clone();
        match mutate {
            0 => forged.context.chain_id += 1,
            1 => forged.context.genesis_config_commitment[0] ^= 1,
            2 => forged.context.protocol_commitment[0] ^= 1,
            3 => forged.context.epoch += 1,
            4 => forged.context.validator_set_hash[0] ^= 1,
            5 => forged.context.height += 1,
            6 => forged.context.parent_block_hash[0] ^= 1,
            _ => forged.context.parent_decision_hash[0] ^= 1,
        }
        rejected(collector.insert(&forged), "context mismatch");
    }
    let mut wrong_genesis = f.successor();
    wrong_genesis.genesis_config_commitment[0] ^= 1;
    assert!(collector.reset_context(wrong_genesis, 0).is_err());
    assert_eq!(collector.vote_count(), 1);
    collector.reset_context(f.successor(), 0).unwrap();
    assert_eq!(collector.vote_count(), 0);
    assert_eq!(collector.retained_round_count(), 0);
    rejected(collector.insert(&old), "context mismatch");
    assert!(collector.reset_context(f.context, 0).is_err());
    let new_vote = Vote::sign(
        f.successor(),
        0,
        Phase::Prevote,
        Some(A),
        &f.set,
        &f.keys[0],
    )
    .unwrap();
    collector.insert(&new_vote).unwrap();
    assert_eq!(collector.vote_count(), 1);
}

#[test]
fn budgets_fail_explicitly_without_overwrite_and_retirement_is_explicit() {
    let f = Fixture::new(&[1; 4]);
    let mut collector = f.with_limits(CollectorLimits {
        max_retained_rounds: 2,
        max_future_round_span: 2,
        max_votes: 22,
    });
    let first = f.vote(0, 0, Phase::Prevote, Some(A));
    collector.insert(&first).unwrap();
    collector
        .insert(&f.vote(1, 0, Phase::Prevote, Some(A)))
        .unwrap();
    collector.advance_round(1).unwrap();
    rejected(
        collector.insert(&f.vote(2, 0, Phase::Prevote, Some(A))),
        "vote budget",
    );
    assert_eq!(collector.insert(&first).unwrap(), VoteInsert::Duplicate);
    rejected(
        collector.insert(&f.vote(0, 0, Phase::Prevote, Some(B))),
        "equivocation",
    );
    collector
        .insert(&f.vote(0, 2, Phase::Prevote, None))
        .unwrap();
    rejected(
        collector.insert(&f.vote(0, 4, Phase::Prevote, None)),
        "future-round span",
    );
    assert_eq!(collector.vote_count(), 3);
    collector.retire_before(1).unwrap();
    assert_eq!(collector.vote_count(), 1);
    rejected(collector.insert(&first), "retired");
    assert!(collector.retire_before(0).is_err());
    assert!(collector.advance_round(0).is_err());
    for phase in [Phase::Prevote, Phase::Precommit] {
        for signer in 0..4 {
            collector
                .insert(&f.vote(signer, 1, phase, Some(A)))
                .unwrap();
        }
    }
    assert!(collector.advance_round(2).is_err());
    assert_eq!(collector.current_round(), 1);
    assert_eq!(collector.vote_count(), 9);
    collector.advance_round_and_retire(2, 2).unwrap();
    assert_eq!(collector.current_round(), 2);
    assert_eq!(collector.phase_weight(2, Phase::Prevote), 1);
    assert_eq!(collector.future_tip_count(), 0);
    assert_eq!(collector.vote_count(), 1);
}

#[test]
fn future_phase_threshold_is_not_current_timer_eligibility() {
    let f = Fixture::new(&[1; 4]);
    let mut collector = f.collector();
    for i in 0..3 {
        collector
            .insert(&f.vote(i, 1, Phase::Precommit, None))
            .unwrap();
    }
    assert_eq!(collector.phase_weight(1, Phase::Precommit), 3);
    assert!(!collector.timeout_eligible(1, Phase::Precommit));
    collector.advance_round(1).unwrap();
    assert!(collector.timeout_eligible(1, Phase::Precommit));
    collector.advance_round(2).unwrap();
    assert!(!collector.timeout_eligible(1, Phase::Precommit));
    assert!(collector
        .quorum(1, Phase::Precommit, None)
        .unwrap()
        .is_some());
}

#[test]
fn zero_overflowing_or_impossible_budgets_and_invalid_context_are_rejected() {
    let f = Fixture::new(&[1; 4]);
    for limits in [
        CollectorLimits {
            max_retained_rounds: 0,
            max_future_round_span: 1,
            max_votes: 1,
        },
        CollectorLimits {
            max_retained_rounds: 1,
            max_future_round_span: 1,
            max_votes: 0,
        },
        CollectorLimits {
            max_retained_rounds: 1,
            max_future_round_span: 1,
            max_votes: 9,
        },
        CollectorLimits {
            max_retained_rounds: usize::MAX,
            max_future_round_span: 1,
            max_votes: 1,
        },
    ] {
        assert!(VoteCollector::new(f.context, Arc::clone(&f.set), 0, limits).is_err());
    }
    let limits = CollectorLimits {
        max_retained_rounds: 1,
        max_future_round_span: 0,
        max_votes: 20,
    };
    let mut bad_context = f.context;
    bad_context.parent_decision_hash = [1; 32];
    assert!(VoteCollector::new(bad_context, Arc::clone(&f.set), 0, limits).is_err());
    let mut collector = VoteCollector::new(f.context, Arc::clone(&f.set), 0, limits).unwrap();
    rejected(
        collector.insert(&f.vote(0, 1, Phase::Prevote, None)),
        "future-round span",
    );
}

#[test]
fn maximum_round_does_not_overflow_future_admission_or_weight_arithmetic() {
    let f = Fixture::new(&[u64::MAX - 3, 1, 1, 1]);
    let limits = CollectorLimits {
        max_retained_rounds: 2,
        max_future_round_span: 8,
        max_votes: 28,
    };
    let mut collector =
        VoteCollector::new(f.context, Arc::clone(&f.set), u64::MAX - 1, limits).unwrap();
    collector
        .insert(&f.vote(0, u64::MAX, Phase::Prevote, Some(A)))
        .unwrap();
    assert_eq!(collector.catch_up().unwrap().round(), u64::MAX);
    collector.advance_round(u64::MAX).unwrap();
    assert!(collector.catch_up().is_none());
    for i in 1..4 {
        collector
            .insert(&f.vote(i, u64::MAX, Phase::Prevote, Some(A)))
            .unwrap();
    }
    assert_eq!(collector.phase_weight(u64::MAX, Phase::Prevote), u64::MAX);
    assert!(collector.timeout_eligible(u64::MAX, Phase::Prevote));
    assert_eq!(
        collector
            .quorum(u64::MAX, Phase::Prevote, Some(A))
            .unwrap()
            .unwrap()
            .signed_weight(),
        u64::MAX
    );
}

#[test]
fn malicious_future_fragments_cannot_starve_the_current_round_slots_or_votes() {
    let f = Fixture::new(&[1; 4]);
    let mut collector = f.with_limits(CollectorLimits {
        max_retained_rounds: 1,
        max_future_round_span: 100,
        max_votes: 20,
    });
    for round in 1..=100 {
        for phase in [Phase::Prevote, Phase::Precommit] {
            collector.insert(&f.vote(0, round, phase, None)).unwrap();
        }
    }
    assert_eq!(collector.retained_round_count(), 0);
    assert_eq!(collector.future_tip_count(), 2);
    rejected(
        collector.insert(&f.vote(0, 99, Phase::Prevote, None)),
        "stale future tip",
    );
    for phase in [Phase::Prevote, Phase::Precommit] {
        for signer in 0..4 {
            collector
                .insert(&f.vote(signer, 0, phase, Some(A)))
                .unwrap();
        }
        assert!(collector.quorum(0, phase, Some(A)).unwrap().is_some());
    }
    assert_eq!(collector.vote_count(), 10);
    assert!(collector.catch_up().is_none());
}

#[test]
fn advancing_a_full_current_round_requires_explicit_retirement_and_reserves_the_next() {
    let f = Fixture::new(&[1; 4]);
    let mut collector = f.with_limits(CollectorLimits {
        max_retained_rounds: 1,
        max_future_round_span: 8,
        max_votes: 20,
    });
    for phase in [Phase::Prevote, Phase::Precommit] {
        for signer in 0..4 {
            collector
                .insert(&f.vote(signer, 0, phase, Some(A)))
                .unwrap();
        }
    }
    assert!(collector.advance_round(1).is_err());
    assert_eq!(collector.current_round(), 0);
    assert_eq!(collector.vote_count(), 8);
    let preserved_qc = collector
        .quorum(0, Phase::Prevote, Some(A))
        .unwrap()
        .unwrap();
    collector.advance_round_and_retire(1, 1).unwrap();
    assert_eq!(collector.current_round(), 1);
    assert_eq!(collector.lowest_retained_round(), 1);
    assert_eq!(collector.vote_count(), 0);
    preserved_qc.quorum().verify(&f.set).unwrap();
    rejected(
        collector.insert(&f.vote(0, 0, Phase::Prevote, Some(A))),
        "retired",
    );
    for phase in [Phase::Prevote, Phase::Precommit] {
        for signer in 0..4 {
            collector
                .insert(&f.vote(signer, 1, phase, Some(A)))
                .unwrap();
        }
    }
    assert_eq!(collector.vote_count(), 8);
}

#[test]
fn one_validator_future_bucket_poison_cannot_block_honest_exact_round_catchup() {
    let f = Fixture::new(&[1; 4]);
    let mut collector = f.with_limits(CollectorLimits {
        max_retained_rounds: 3,
        max_future_round_span: 8,
        max_votes: 24,
    });
    collector
        .insert(&f.vote(0, 1, Phase::Prevote, None))
        .unwrap();
    collector
        .insert(&f.vote(0, 3, Phase::Precommit, None))
        .unwrap();
    for signer in [1, 2] {
        collector
            .insert(&f.vote(signer, 2, Phase::Prevote, None))
            .expect("one validator consumed other validators' future admission");
    }
    assert_eq!(collector.catch_up().unwrap().round(), 2);
}

#[test]
fn future_tip_updates_cannot_revoke_an_already_formed_exact_round_witness() {
    let f = Fixture::new(&[1; 4]);
    let mut collector = f.with_limits(CollectorLimits {
        max_retained_rounds: 1,
        max_future_round_span: 8,
        max_votes: 20,
    });
    collector
        .insert(&f.vote(0, 2, Phase::Prevote, None))
        .unwrap();
    collector
        .insert(&f.vote(1, 2, Phase::Prevote, None))
        .unwrap();
    let old = collector.catch_up().unwrap();
    assert_eq!(old.round(), 2);
    collector
        .insert(&f.vote(0, 3, Phase::Prevote, None))
        .unwrap();
    assert_eq!(collector.catch_up().unwrap().round(), 2);
    rejected(
        collector.insert(&f.vote(0, 2, Phase::Prevote, Some(A))),
        "equivocation",
    );
    collector
        .insert(&f.vote(1, 3, Phase::Prevote, None))
        .unwrap();
    assert_eq!(collector.catch_up().unwrap().round(), 3);
    collector
        .insert(&f.vote(0, 4, Phase::Prevote, None))
        .unwrap();
    collector
        .insert(&f.vote(0, 4, Phase::Precommit, None))
        .unwrap();
    assert_eq!(collector.catch_up().unwrap().round(), 3);
    assert_eq!(old.votes().len(), 2);
    collector.advance_round_and_retire(3, 3).unwrap();
    assert_eq!(collector.phase_weight(3, Phase::Prevote), 2);
    assert_eq!(collector.future_tip_count(), 2);
    assert!(collector.catch_up().is_none());
    collector
        .insert(&f.vote(1, 4, Phase::Prevote, None))
        .unwrap();
    assert_eq!(collector.catch_up().unwrap().round(), 4);
    assert!(collector.vote_count() <= 20);
}

#[test]
fn failed_promotion_preserves_future_tips_and_pinned_witness_atomically() {
    let f = Fixture::new(&[1; 4]);
    let mut collector = f.with_limits(CollectorLimits {
        max_retained_rounds: 1,
        max_future_round_span: 8,
        max_votes: 20,
    });
    for phase in [Phase::Prevote, Phase::Precommit] {
        for signer in 0..4 {
            collector
                .insert(&f.vote(signer, 0, phase, Some(A)))
                .unwrap();
        }
    }
    collector
        .insert(&f.vote(0, 2, Phase::Prevote, None))
        .unwrap();
    collector
        .insert(&f.vote(1, 2, Phase::Prevote, None))
        .unwrap();
    let old = collector.catch_up().unwrap();
    assert!(collector.advance_round(1).is_err());
    assert_eq!(collector.current_round(), 0);
    assert_eq!(collector.vote_count(), 12);
    assert_eq!(collector.future_tip_count(), 2);
    assert_eq!(collector.catch_up().unwrap().votes(), old.votes());
    collector.advance_round_and_retire(2, 2).unwrap();
    assert_eq!(collector.vote_count(), 2);
    assert_eq!(collector.phase_weight(2, Phase::Prevote), 2);
    assert_eq!(collector.future_tip_count(), 0);
    assert!(collector.catch_up().is_none());
}

#[test]
fn future_reservation_rejects_small_configuration_and_does_not_consume_history_buckets() {
    let f = Fixture::new(&[1; 4]);
    for budget in [8, 16, 19, 21] {
        assert!(VoteCollector::new(
            f.context,
            Arc::clone(&f.set),
            0,
            CollectorLimits {
                max_retained_rounds: 1,
                max_future_round_span: 100,
                max_votes: budget
            }
        )
        .is_err());
    }
    let mut collector = f.with_limits(CollectorLimits {
        max_retained_rounds: 1,
        max_future_round_span: 100,
        max_votes: 20,
    });
    for signer in 0..4 {
        for (phase, offset) in [(Phase::Prevote, 0), (Phase::Precommit, 1)] {
            collector
                .insert(&f.vote(signer, 1 + 2 * signer as u64 + offset, phase, None))
                .unwrap();
        }
    }
    assert_eq!(collector.future_tip_count(), 8);
    assert_eq!(collector.retained_round_count(), 0);
    assert!(collector.catch_up().is_none());
}
