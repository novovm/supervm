use super::*;
use novovm_protocol::{decode_nov_native_tx_wire_v1, encode_nov_native_tx_wire_v1, NovTxKindV1};

#[path = "native_seal_failover_evidence.rs"]
mod evidence;
use evidence::FailoverEvidence;

pub(super) fn exercise(
    nodes: &[Node],
    authority: &NovNativeSealEpochAuthorityV1,
    validators: &[NovNativeSealValidatorV1],
    plan: &NovNativeCandidateExecutionPlanV1,
    evidence_root: &std::path::Path,
) {
    let height = 3;
    let label = "continuous-offline-next-leader";
    let initial = authority.expected_leader(height, 0).unwrap();
    let schedule = &authority.validator_set.validators;
    let initial_position = schedule
        .iter()
        .position(|validator| validator.validator_id == initial)
        .unwrap();
    let replacement = schedule[(initial_position + 1) % schedule.len()].validator_id;
    let offline = validators
        .iter()
        .position(|validator| validator.validator_id == initial)
        .unwrap();
    let active = (0..nodes.len())
        .filter(|index| *index != offline)
        .collect::<Vec<_>>();
    let ingress = *active
        .iter()
        .find(|&&index| validators[index].validator_id != replacement)
        .unwrap();
    let parent = continuous::history(&nodes[0], height - 1);
    for node in nodes {
        assert_eq!(continuous::history(node, height - 1), parent);
        let file = node.0.join("seal.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        config["round_timeout_ms"] = 30000.into();
        fs::write(file, serde_json::to_vec(&config).unwrap()).unwrap();
    }
    let mut transaction = decode_nov_native_tx_wire_v1(&plan.raw_txs[0]).unwrap();
    let NovTxKindV1::Execute(execution) = &mut transaction.kind else {
        panic!("execute fixture")
    };
    execution.nonce = height - 1;
    sign_nov_native_tx_with_seed_v1(&mut transaction, [0xc3; 32]).unwrap();
    let raw = encode_nov_native_tx_wire_v1(&transaction).unwrap();
    let hash = canonical_nov_native_tx_hash_from_payload_v1(&raw).unwrap();
    let inject = || {
        let result = continuous::rpc(
            &nodes[ingress],
            label,
            "nov_sendRawTransaction",
            serde_json::json!([hex(&raw)]),
        );
        assert_eq!(result["result"]["status"], "queued", "{result}");
    };
    let started = Instant::now();
    run_cluster_at_height(
        nodes,
        &active,
        label,
        0,
        true,
        true,
        true,
        height,
        Some(&inject),
        true,
    );
    assert!(!nodes[offline]
        .0
        .join(format!("{label}.process.json"))
        .exists());
    let expected = continuous::history(&nodes[active[0]], height);
    assert_eq!(expected["tip"]["body"]["raw_txs"], serde_json::json!([raw]));
    assert_eq!(
        expected["tip"]["body"]["tx_hashes"],
        serde_json::json!([hash])
    );
    let elapsed = started.elapsed();
    let block_hash =
        serde_json::from_value(expected["tip"]["header"]["block_hash"].clone()).unwrap();
    let mut witnesses = Vec::new();
    for &index in &active {
        assert_eq!(continuous::history(&nodes[index], height), expected);
        let store =
            NovNativeBlockSealStoreV1::open_existing_read_only(&nodes[index].0.join("seal-db"))
                .unwrap()
                .unwrap();
        let witness =
            FailoverEvidence::read(&store, authority, height, block_hash, initial).unwrap();
        witness.check_negative_cases(authority, height, block_hash, initial);
        witnesses.push(witness);
    }
    let decisions = witnesses
        .iter()
        .map(|witness| &witness.decision)
        .collect::<Vec<_>>();
    assert!(decisions.iter().all(|decision| decision == &decisions[0]));
    assert_eq!(continuous::history(&nodes[offline], height - 1), parent);
    run_cluster_at_height(
        nodes,
        &active,
        "continuous-failover-restart",
        8,
        true,
        true,
        true,
        height,
        None,
        false,
    );
    for (&index, witness) in active.iter().zip(&witnesses) {
        assert_eq!(continuous::history(&nodes[index], height), expected);
        let store =
            NovNativeBlockSealStoreV1::open_existing_read_only(&nodes[index].0.join("seal-db"))
                .unwrap()
                .unwrap();
        assert_eq!(
            FailoverEvidence::read(&store, authority, height, block_hash, initial).unwrap(),
            *witness
        );
    }
    fs::write(
        evidence_root.join("candidate-less-failover-acceptance.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "accepted":true,"scope":"local_main_process_candidate_less_leader_failover",
            "height":height,"round":decisions[0].prepare.subject.round,"offline_round_zero_leader_index":offline,
            "active_validator_indices":active,"rpc_ingress_validator_index":ingress,
            "first_replacement_validator_id":hex(&replacement),"decisions":decisions,
            "prepared_proposer_id":hex(&witnesses[0].proposal.proposal.proposer_id),
            "verified_witnesses":witnesses,"failover_elapsed_ms":elapsed.as_millis(),
            "negative_evidence_rejected":true,"restart_preserved_witnesses":true,
            "persistent_readback":expected,"restart_preserved_finality":true,
            "manually_created_next_candidate":false,"production_ready":false,
            "physical_lan_executed":false,"power_loss_executed":false
        }))
        .unwrap(),
    )
    .unwrap();
    println!(
        "failover evidence: {}",
        evidence_root
            .join("candidate-less-failover-acceptance.json")
            .display()
    );
}
