use std::collections::HashMap;

use ed25519_dalek::{SigningKey, VerifyingKey};
use novovm_consensus::{
    BFTConfig, BFTEngine, BFTError, Epoch, HotStuffProtocol, NodeId, SlashMode, SlashPolicy,
    ValidatorSet, Vote,
};
use rand::rngs::OsRng;

fn generate_keys(count: usize) -> (Vec<SigningKey>, HashMap<NodeId, VerifyingKey>) {
    let signing_keys: Vec<_> = (0..count)
        .map(|_| SigningKey::generate(&mut OsRng))
        .collect();
    let public_keys: HashMap<_, _> = signing_keys
        .iter()
        .enumerate()
        .map(|(i, sk)| (i as NodeId, sk.verifying_key()))
        .collect();
    (signing_keys, public_keys)
}

fn invalid_signature_case() -> bool {
    let run = || -> Result<bool, String> {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);
        let (signing_keys, public_keys) = generate_keys(4);
        let mut leader_engine = BFTEngine::new(
            BFTConfig::default(),
            0,
            signing_keys[0].clone(),
            validator_set,
            public_keys,
        )
        .map_err(|e| e.to_string())?;

        leader_engine.start_epoch().map_err(|e| e.to_string())?;
        leader_engine.add_batch(1, 100).map_err(|e| e.to_string())?;
        leader_engine.add_batch(2, 150).map_err(|e| e.to_string())?;

        let mut batch_results = HashMap::new();
        batch_results.insert(1, [1u8; 32]);
        batch_results.insert(2, [2u8; 32]);
        let proposal = leader_engine
            .propose_epoch(&batch_results)
            .map_err(|e| e.to_string())?;
        let proposal_hash = proposal.hash();

        let mut qc_opt = None;
        for node_id in 0..4 {
            let vote = Vote::new(
                node_id as NodeId,
                proposal_hash,
                proposal.height,
                &signing_keys[node_id],
            );
            if let Some(qc) = leader_engine
                .collect_vote(vote)
                .map_err(|e| e.to_string())?
            {
                qc_opt = Some(qc);
                break;
            }
        }

        let mut qc = qc_opt.ok_or_else(|| "failed to build quorum certificate".to_string())?;
        let first_vote = qc
            .votes
            .get_mut(0)
            .ok_or_else(|| "qc has no vote to tamper".to_string())?;
        let first_sig_byte = first_vote
            .signature
            .get_mut(0)
            .ok_or_else(|| "vote signature is empty".to_string())?;
        *first_sig_byte ^= 0x01;

        let got_invalid_signature = match leader_engine.commit_qc(qc) {
            Err(BFTError::InvalidSignature(_)) => true,
            Err(_) => false,
            Ok(_) => false,
        };
        Ok(got_invalid_signature)
    };

    run().unwrap_or_else(|err| {
        eprintln!("consensus_negative_case=invalid_signature error={}", err);
        false
    })
}

fn duplicate_vote_case() -> bool {
    let run = || -> Result<bool, String> {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);
        let (signing_keys, _) = generate_keys(4);
        let mut leader_protocol =
            HotStuffProtocol::new(validator_set, 0).map_err(|e| e.to_string())?;

        let mut epoch = Epoch::new(0, 0, 0);
        epoch.add_batch(1, 10);
        let proposal = leader_protocol.propose(&epoch).map_err(|e| e.to_string())?;

        let vote = Vote::new(1, proposal.hash(), proposal.height, &signing_keys[1]);
        let _ = leader_protocol
            .collect_vote(vote.clone())
            .map_err(|e| e.to_string())?;

        let got_duplicate_vote = match leader_protocol.collect_vote(vote) {
            Err(BFTError::DuplicateVote(1)) => true,
            Err(_) => false,
            Ok(_) => false,
        };
        Ok(got_duplicate_vote)
    };

    run().unwrap_or_else(|err| {
        eprintln!("consensus_negative_case=duplicate_vote error={}", err);
        false
    })
}

fn wrong_epoch_case() -> bool {
    let run = || -> Result<bool, String> {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);
        let (signing_keys, _) = generate_keys(4);
        let mut leader_protocol =
            HotStuffProtocol::new(validator_set.clone(), 0).map_err(|e| e.to_string())?;

        let mut epoch = Epoch::new(0, 0, 0);
        epoch.add_batch(1, 10);
        let proposal = leader_protocol.propose(&epoch).map_err(|e| e.to_string())?;
        let leader_state = leader_protocol.get_state();

        let mut voter_protocol =
            HotStuffProtocol::new(validator_set, 1).map_err(|e| e.to_string())?;
        voter_protocol.sync_state(leader_state);

        let mut wrong_epoch_proposal = proposal.clone();
        wrong_epoch_proposal.epoch_id = proposal.epoch_id.saturating_add(1);

        let got_epoch_mismatch = match voter_protocol.vote(&wrong_epoch_proposal, &signing_keys[1])
        {
            Err(BFTError::InvalidProposal(msg)) => msg.contains("Epoch mismatch"),
            Err(_) => false,
            Ok(_) => false,
        };
        Ok(got_epoch_mismatch)
    };

    run().unwrap_or_else(|err| {
        eprintln!("consensus_negative_case=wrong_epoch error={}", err);
        false
    })
}

fn weighted_quorum_case() -> bool {
    let run = || -> Result<bool, String> {
        // total weight=10, quorum=7
        let validator_set =
            ValidatorSet::new_weighted(vec![(0, 6), (1, 4)]).map_err(|e| e.to_string())?;
        let (signing_keys, _) = generate_keys(2);
        let mut leader_protocol =
            HotStuffProtocol::new(validator_set.clone(), 0).map_err(|e| e.to_string())?;

        let mut epoch = Epoch::new(0, 0, 0);
        epoch.add_batch(1, 10);
        let proposal = leader_protocol.propose(&epoch).map_err(|e| e.to_string())?;
        let leader_state = leader_protocol.get_state();

        let mut voter0 =
            HotStuffProtocol::new(validator_set.clone(), 0).map_err(|e| e.to_string())?;
        voter0.sync_state(leader_state.clone());
        let vote0 = voter0
            .vote(&proposal, &signing_keys[0])
            .map_err(|e| e.to_string())?;
        let first = leader_protocol
            .collect_vote(vote0)
            .map_err(|e| e.to_string())?;

        let mut voter1 = HotStuffProtocol::new(validator_set, 1).map_err(|e| e.to_string())?;
        voter1.sync_state(leader_state);
        let vote1 = voter1
            .vote(&proposal, &signing_keys[1])
            .map_err(|e| e.to_string())?;
        let second = leader_protocol
            .collect_vote(vote1)
            .map_err(|e| e.to_string())?;

        Ok(first.is_none() && second.is_some())
    };

    run().unwrap_or_else(|err| {
        eprintln!("consensus_negative_case=weighted_quorum error={}", err);
        false
    })
}

fn equivocation_case() -> bool {
    let run = || -> Result<bool, String> {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, _) = generate_keys(3);
        let mut leader_protocol =
            HotStuffProtocol::new(validator_set.clone(), 0).map_err(|e| e.to_string())?;

        let mut epoch = Epoch::new(0, 0, 0);
        epoch.add_batch(1, 10);
        let proposal = leader_protocol.propose(&epoch).map_err(|e| e.to_string())?;
        let leader_state = leader_protocol.get_state();

        let mut voter = HotStuffProtocol::new(validator_set, 1).map_err(|e| e.to_string())?;
        voter.sync_state(leader_state);
        let first_vote = voter
            .vote(&proposal, &signing_keys[1])
            .map_err(|e| e.to_string())?;
        let _ = leader_protocol
            .collect_vote(first_vote)
            .map_err(|e| e.to_string())?;

        let mut other_hash = proposal.hash();
        other_hash[0] ^= 0xAA;
        let conflicting_vote = Vote::new(1, other_hash, proposal.height, &signing_keys[1]);
        let equivocation_detected = matches!(
            leader_protocol.collect_vote(conflicting_vote),
            Err(BFTError::EquivocationDetected { voter: 1, .. })
        );
        let has_slash_evidence = !leader_protocol.slash_evidences().is_empty();
        Ok(equivocation_detected && has_slash_evidence)
    };

    run().unwrap_or_else(|err| {
        eprintln!("consensus_negative_case=equivocation error={}", err);
        false
    })
}

fn slash_execution_case() -> bool {
    let run = || -> Result<bool, String> {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, _) = generate_keys(3);
        let mut leader_protocol =
            HotStuffProtocol::new(validator_set.clone(), 0).map_err(|e| e.to_string())?;

        let mut epoch = Epoch::new(0, 0, 0);
        epoch.add_batch(1, 10);
        let proposal = leader_protocol.propose(&epoch).map_err(|e| e.to_string())?;
        let leader_state = leader_protocol.get_state();

        let mut voter1 =
            HotStuffProtocol::new(validator_set.clone(), 1).map_err(|e| e.to_string())?;
        voter1.sync_state(leader_state.clone());
        let vote1 = voter1
            .vote(&proposal, &signing_keys[1])
            .map_err(|e| e.to_string())?;
        let _ = leader_protocol
            .collect_vote(vote1)
            .map_err(|e| e.to_string())?;

        // conflicting vote -> slash execution
        let mut other_hash = proposal.hash();
        other_hash[0] ^= 0x44;
        let conflicting_vote = Vote::new(1, other_hash, proposal.height, &signing_keys[1]);
        let _ = leader_protocol.collect_vote(conflicting_vote).unwrap_err();

        let slashed = leader_protocol.is_validator_jailed(1);
        let executed = leader_protocol
            .slash_executions()
            .iter()
            .any(|x| x.voter_id == 1 && x.jailed && x.weight_before > x.weight_after);
        let quorum_recomputed = leader_protocol.active_quorum_size() == 2;

        // slashed validator should be rejected
        let mut voter1_again =
            HotStuffProtocol::new(validator_set.clone(), 1).map_err(|e| e.to_string())?;
        voter1_again.sync_state(leader_protocol.get_state());
        let vote1_again = voter1_again
            .vote(&proposal, &signing_keys[1])
            .map_err(|e| e.to_string())?;
        let slashed_rejected = matches!(
            leader_protocol.collect_vote(vote1_again),
            Err(BFTError::SlashedValidator(1))
        );

        Ok(slashed && executed && quorum_recomputed && slashed_rejected)
    };

    run().unwrap_or_else(|err| {
        eprintln!("consensus_negative_case=slash_execution error={}", err);
        false
    })
}

fn slash_policy_threshold_case() -> bool {
    let run = || -> Result<bool, String> {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, _) = generate_keys(3);
        let mut leader_protocol =
            HotStuffProtocol::new(validator_set.clone(), 0).map_err(|e| e.to_string())?;
        leader_protocol
            .set_slash_policy(SlashPolicy {
                mode: SlashMode::Enforce,
                equivocation_threshold: 2,
                min_active_validators: 2,
                cooldown_epochs: 0,
            })
            .map_err(|e| e.to_string())?;

        let mut epoch = Epoch::new(0, 0, 0);
        epoch.add_batch(1, 10);
        let proposal = leader_protocol.propose(&epoch).map_err(|e| e.to_string())?;
        let leader_state = leader_protocol.get_state();

        let mut voter1 =
            HotStuffProtocol::new(validator_set.clone(), 1).map_err(|e| e.to_string())?;
        voter1.sync_state(leader_state);
        let vote1 = voter1
            .vote(&proposal, &signing_keys[1])
            .map_err(|e| e.to_string())?;
        let _ = leader_protocol
            .collect_vote(vote1)
            .map_err(|e| e.to_string())?;

        let mut hash_a = proposal.hash();
        hash_a[0] ^= 0x19;
        let first_equivocation = matches!(
            leader_protocol.collect_vote(Vote::new(1, hash_a, proposal.height, &signing_keys[1])),
            Err(BFTError::EquivocationDetected { voter: 1, .. })
        );
        let first_not_jailed = !leader_protocol.is_validator_jailed(1)
            && leader_protocol
                .slash_executions()
                .last()
                .map(|e| !e.jailed && e.evidence_count == 1 && e.threshold == 2)
                .unwrap_or(false);

        let mut hash_b = proposal.hash();
        hash_b[0] ^= 0x27;
        let second_equivocation = matches!(
            leader_protocol.collect_vote(Vote::new(1, hash_b, proposal.height, &signing_keys[1])),
            Err(BFTError::EquivocationDetected { voter: 1, .. })
        );
        let second_jailed = leader_protocol.is_validator_jailed(1)
            && leader_protocol
                .slash_executions()
                .last()
                .map(|e| e.jailed && e.evidence_count >= 2 && e.policy_mode == "enforce")
                .unwrap_or(false);

        Ok(first_equivocation && first_not_jailed && second_equivocation && second_jailed)
    };

    run().unwrap_or_else(|err| {
        eprintln!(
            "consensus_negative_case=slash_policy_threshold error={}",
            err
        );
        false
    })
}

fn slash_policy_observe_only_case() -> bool {
    let run = || -> Result<bool, String> {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, _) = generate_keys(3);
        let mut leader_protocol =
            HotStuffProtocol::new(validator_set.clone(), 0).map_err(|e| e.to_string())?;
        leader_protocol
            .set_slash_policy(SlashPolicy {
                mode: SlashMode::ObserveOnly,
                equivocation_threshold: 1,
                min_active_validators: 2,
                cooldown_epochs: 0,
            })
            .map_err(|e| e.to_string())?;

        let mut epoch = Epoch::new(0, 0, 0);
        epoch.add_batch(1, 10);
        let proposal = leader_protocol.propose(&epoch).map_err(|e| e.to_string())?;
        let leader_state = leader_protocol.get_state();

        let mut voter1 = HotStuffProtocol::new(validator_set, 1).map_err(|e| e.to_string())?;
        voter1.sync_state(leader_state);
        let vote1 = voter1
            .vote(&proposal, &signing_keys[1])
            .map_err(|e| e.to_string())?;
        let _ = leader_protocol
            .collect_vote(vote1)
            .map_err(|e| e.to_string())?;

        let mut hash_a = proposal.hash();
        hash_a[0] ^= 0x31;
        let observed = matches!(
            leader_protocol.collect_vote(Vote::new(1, hash_a, proposal.height, &signing_keys[1])),
            Err(BFTError::EquivocationDetected { voter: 1, .. })
        );
        let not_jailed = !leader_protocol.is_validator_jailed(1);
        let exec_observe = leader_protocol
            .slash_executions()
            .last()
            .map(|e| !e.jailed && e.policy_mode == "observe_only" && e.evidence_count == 1)
            .unwrap_or(false);
        Ok(observed && not_jailed && exec_observe)
    };

    run().unwrap_or_else(|err| {
        eprintln!(
            "consensus_negative_case=slash_policy_observe_only error={}",
            err
        );
        false
    })
}

#[derive(Debug, Clone, Copy)]
struct UnjailCooldownSignal {
    pass: bool,
    jailed: bool,
    until: u64,
    unjailed: bool,
    at: u64,
    premature_rejected: bool,
}

fn unjail_cooldown_case() -> UnjailCooldownSignal {
    let run = || -> Result<UnjailCooldownSignal, String> {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, _) = generate_keys(3);
        let mut leader_protocol =
            HotStuffProtocol::new(validator_set, 2).map_err(|e| e.to_string())?;
        leader_protocol
            .set_slash_policy(SlashPolicy {
                mode: SlashMode::Enforce,
                equivocation_threshold: 1,
                min_active_validators: 2,
                cooldown_epochs: 2,
            })
            .map_err(|e| e.to_string())?;
        let _ = leader_protocol
            .trigger_view_change()
            .map_err(|e| e.to_string())?;
        let _ = leader_protocol
            .trigger_view_change()
            .map_err(|e| e.to_string())?;
        if leader_protocol.current_leader() != 2 {
            return Err("failed to rotate leader to node-2 for cooldown probe".to_string());
        }

        let mut epoch0 = Epoch::new(0, 0, 0);
        epoch0.add_batch(1, 10);
        let proposal0 = leader_protocol
            .propose(&epoch0)
            .map_err(|e| e.to_string())?;

        let vote1 = Vote::new(1, proposal0.hash(), proposal0.height, &signing_keys[1]);
        let _ = leader_protocol
            .collect_vote(vote1)
            .map_err(|e| e.to_string())?;
        let mut conflicting_hash = proposal0.hash();
        conflicting_hash[0] ^= 0x5A;
        let _ = leader_protocol
            .collect_vote(Vote::new(
                1,
                conflicting_hash,
                proposal0.height,
                &signing_keys[1],
            ))
            .unwrap_err();

        let jailed = leader_protocol.is_validator_jailed(1);
        let until = leader_protocol.validator_jailed_until_epoch(1).unwrap_or(0);

        let _ = leader_protocol
            .collect_vote(Vote::new(
                2,
                proposal0.hash(),
                proposal0.height,
                &signing_keys[2],
            ))
            .map_err(|e| e.to_string())?;
        let qc0 = leader_protocol
            .collect_vote(Vote::new(
                0,
                proposal0.hash(),
                proposal0.height,
                &signing_keys[0],
            ))
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "qc0 not formed".to_string())?;
        leader_protocol
            .pre_commit(&qc0)
            .map_err(|e| e.to_string())?;
        leader_protocol.commit().map_err(|e| e.to_string())?;

        let mut epoch1 = Epoch::new(1, 1, 0);
        epoch1.add_batch(1, 11);
        let proposal1 = leader_protocol
            .propose(&epoch1)
            .map_err(|e| e.to_string())?;
        let premature_rejected = matches!(
            leader_protocol.collect_vote(Vote::new(
                1,
                proposal1.hash(),
                proposal1.height,
                &signing_keys[1]
            )),
            Err(BFTError::SlashedValidator(1))
        );

        let _ = leader_protocol
            .collect_vote(Vote::new(
                2,
                proposal1.hash(),
                proposal1.height,
                &signing_keys[2],
            ))
            .map_err(|e| e.to_string())?;
        let qc1 = leader_protocol
            .collect_vote(Vote::new(
                0,
                proposal1.hash(),
                proposal1.height,
                &signing_keys[0],
            ))
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "qc1 not formed".to_string())?;
        leader_protocol
            .pre_commit(&qc1)
            .map_err(|e| e.to_string())?;
        leader_protocol.commit().map_err(|e| e.to_string())?;

        let at = leader_protocol.current_height();
        let unjailed = !leader_protocol.is_validator_jailed(1);
        if !unjailed {
            return Ok(UnjailCooldownSignal {
                pass: false,
                jailed,
                until,
                unjailed: false,
                at,
                premature_rejected,
            });
        }

        let mut epoch2 = Epoch::new(2, 2, 0);
        epoch2.add_batch(1, 12);
        let proposal2 = leader_protocol
            .propose(&epoch2)
            .map_err(|e| e.to_string())?;
        let recovered_vote_accepted = leader_protocol
            .collect_vote(Vote::new(
                1,
                proposal2.hash(),
                proposal2.height,
                &signing_keys[1],
            ))
            .map(|_| true)
            .unwrap_or(false);

        Ok(UnjailCooldownSignal {
            pass: jailed
                && until == 2
                && premature_rejected
                && unjailed
                && at == 2
                && recovered_vote_accepted,
            jailed,
            until,
            unjailed,
            at,
            premature_rejected,
        })
    };

    run().unwrap_or_else(|err| {
        eprintln!("consensus_negative_case=unjail_cooldown error={}", err);
        UnjailCooldownSignal {
            pass: false,
            jailed: false,
            until: 0,
            unjailed: false,
            at: 0,
            premature_rejected: false,
        }
    })
}

fn view_change_case() -> bool {
    let run = || -> Result<bool, String> {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).map_err(|e| e.to_string())?;
        let leader0 = protocol.current_leader();
        let view0 = protocol.current_view();

        let next = protocol.trigger_view_change().map_err(|e| e.to_string())?;
        let leader1 = protocol.current_leader();
        let view1 = protocol.current_view();
        let phase_ok = protocol.current_phase() == novovm_consensus::Phase::Propose;

        Ok(leader0 == 0 && view0 == 0 && next == 1 && leader1 == 1 && view1 == 1 && phase_ok)
    };

    run().unwrap_or_else(|err| {
        eprintln!("consensus_negative_case=view_change error={}", err);
        false
    })
}

fn fork_choice_case() -> bool {
    let run = || -> Result<bool, String> {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);
        let (signing_keys, public_keys) = generate_keys(4);
        let engine = BFTEngine::new(
            BFTConfig::default(),
            0,
            signing_keys[0].clone(),
            validator_set.clone(),
            public_keys,
        )
        .map_err(|e| e.to_string())?;

        let mut qc_low = novovm_consensus::QuorumCertificate::new([1u8; 32], 10);
        for i in 0..3usize {
            let vote = Vote::new(i as NodeId, [1u8; 32], 10, &signing_keys[i]);
            qc_low.add_vote(vote, 1);
        }

        let mut qc_high = novovm_consensus::QuorumCertificate::new([2u8; 32], 11);
        for i in 0..3usize {
            let vote = Vote::new(i as NodeId, [2u8; 32], 11, &signing_keys[i]);
            qc_high.add_vote(vote, 1);
        }

        let best = engine
            .select_best_qc(&[qc_low.clone(), qc_high.clone()])
            .map_err(|e| e.to_string())?;
        if best.height != 11 {
            return Ok(false);
        }

        let mut qc_invalid = qc_high.clone();
        qc_invalid.total_weight = 99;
        let fallback = engine
            .select_best_qc(&[qc_low.clone(), qc_invalid])
            .map_err(|e| e.to_string())?;
        Ok(fallback.height == qc_low.height)
    };

    run().unwrap_or_else(|err| {
        eprintln!("consensus_negative_case=fork_choice error={}", err);
        false
    })
}

fn main() {
    let invalid_signature = invalid_signature_case();
    let duplicate_vote = duplicate_vote_case();
    let wrong_epoch = wrong_epoch_case();
    let weighted_quorum = weighted_quorum_case();
    let equivocation = equivocation_case();
    let slash_execution = slash_execution_case();
    let slash_threshold = slash_policy_threshold_case();
    let slash_observe_only = slash_policy_observe_only_case();
    let unjail_cooldown = unjail_cooldown_case();
    let view_change = view_change_case();
    let fork_choice = fork_choice_case();
    let pass = invalid_signature
        && duplicate_vote
        && wrong_epoch
        && weighted_quorum
        && equivocation
        && slash_execution
        && slash_threshold
        && slash_observe_only
        && unjail_cooldown.pass
        && view_change
        && fork_choice;

    println!(
        "consensus_negative_out: invalid_signature={} duplicate_vote={} wrong_epoch={} pass={}",
        invalid_signature, duplicate_vote, wrong_epoch, pass
    );
    println!(
        "consensus_negative_ext: weighted_quorum={} equivocation={} slash_execution={} slash_threshold={} slash_observe_only={} unjail_cooldown={} view_change={} fork_choice={}",
        weighted_quorum,
        equivocation,
        slash_execution,
        slash_threshold,
        slash_observe_only,
        unjail_cooldown.pass,
        view_change,
        fork_choice
    );
    println!(
        "unjail_out: jailed={} until={} premature_rejected={} unjailed={} at={}",
        unjail_cooldown.jailed,
        unjail_cooldown.until,
        unjail_cooldown.premature_rejected,
        unjail_cooldown.unjailed,
        unjail_cooldown.at
    );
}
