// Real original node/exec/AOEM execution, with explicitly test-only signatures.
// This does not run a durable round-BFT signer, network or publication path.
mod round_bft_candidate_tests {
    use super::*;
    use crate::native_round_bft::{
        load_executed_genesis_candidate_v1, verify_genesis_decision_v1, ExecutedGenesisCandidateV1,
    };
    use crate::tx_ingress::fresh_genesis::{
        publication::{publish_v1, verify_persisted_v1},
        FreshGenesisConfigV1, GenesisAllocationV1, GenesisValidatorV1, GENESIS_SCHEMA_RECORD_V2,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use novovm_consensus::round_bft::wire::{self, Phase, Proposal, Quorum, Validator, Vote};
    use sha2::{Digest, Sha256};

    // Deliberately not a production signer API: fixture signatures are over the
    // public, canonical unsigned wire fields. No journal permission is claimed.
    fn evidence(candidate: &ExecutedGenesisCandidateV1) -> (Proposal, Quorum) {
        let context = candidate.context();
        let value = candidate.value();
        let set = candidate.validators();
        let keys: Vec<_> = (1..=4)
            .map(|seed| SigningKey::from_bytes(&[seed; 32]))
            .collect();
        let id = |key: &SigningKey| {
            Validator::new(key.verifying_key().to_bytes(), 1)
                .unwrap()
                .id()
        };
        let leader = set.leader(context.height, 2).unwrap();
        let key = keys.iter().find(|key| id(key) == leader).unwrap();
        let mut proposal = Proposal {
            context,
            round: 2,
            value,
            valid_round: None,
            proposer_id: leader,
            signature: vec![0; 64],
        };
        let bytes = wire::encode_proposal(&proposal).unwrap();
        proposal.signature = sign_wire_fixture(key, b"novovm-round-bft/v1/proposal\0", &bytes);
        let votes = keys
            .iter()
            .take(3)
            .map(|key| {
                let mut vote = Vote {
                    context,
                    round: 2,
                    phase: Phase::Precommit,
                    value: Some(value),
                    validator_id: id(key),
                    signature: vec![0; 64],
                };
                let bytes = wire::encode_vote(&vote).unwrap();
                vote.signature = sign_wire_fixture(key, b"novovm-round-bft/v1/vote\0", &bytes);
                vote
            })
            .collect();
        (proposal, Quorum::from_votes(set, votes).unwrap())
    }

    fn sign_wire_fixture(key: &SigningKey, domain: &[u8], bytes: &[u8]) -> Vec<u8> {
        // NOVRBFT1 + u16 version + kind = 11 bytes; final 64 bytes are signature.
        let mut hash = Sha256::new();
        hash.update(domain);
        hash.update(&bytes[11..bytes.len() - 64]);
        key.sign(&hash.finalize()).to_bytes().to_vec()
    }

    fn config() -> FreshGenesisConfigV1 {
        FreshGenesisConfigV1 {
            schema: GENESIS_SCHEMA_RECORD_V2.into(),
            chain_id: 98_919_729,
            timestamp_unix_ms: 1_900_000_000_790,
            protocol_config_commitment: parse_fixed_hex_32_v1(
                &native_business_protocol_config_commitment_v1().unwrap(),
                "protocol",
            )
            .unwrap(),
            allocations: vec![GenesisAllocationV1 {
                account: novovm_adapter_novovm::address_from_seed_v1([0x79; 32])
                    .try_into()
                    .unwrap(),
                nov: "1000".into(),
            }],
            total_initial_nov: "1000".into(),
            validators: (1..=4)
                .map(|seed| GenesisValidatorV1 {
                    public_key: SigningKey::from_bytes(&[seed; 32])
                        .verifying_key()
                        .to_bytes(),
                    weight: 1,
                })
                .collect(),
        }
    }

    #[test]
    fn round_bft_candidate_real_aoem_binding_is_read_only_and_namespace_independent() {
        transfer_candidate_on_runtime_stack(|| {
            let _guard = PLAN_RUNTIME_TEST_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let config = config();
            let compiled = config.compile().unwrap();
            let chain = config.chain_id;
            let pin = compiled.config_commitment();
            let mut previous = None;
            for _ in 0..2 {
                with_plan_runtime(|path, params| {
                    let namespace = parse_fixed_hex_32_v1(
                        &native_aoem_owned_state_namespace_digest_v1(params, chain),
                        "namespace",
                    )
                    .unwrap();
                    let ledger = nov_native_block_ledger_rocksdb_path_v1(path);
                    NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(
                        &ledger, &config, pin, namespace,
                    )
                    .unwrap();
                    publish_v1(chain, pin, params).unwrap();
                    let head_key = native_aoem_owned_state_head_key_v1(chain, &to_hex(&namespace));
                    let authority = candidate_workspace_graph(params).get(&head_key).unwrap();
                    let genesis =
                        serde_json::to_vec(&verify_persisted_v1(chain, pin, params).unwrap())
                            .unwrap();
                    let host_before = load_nov_native_execution_store_v1(path).unwrap();
                    let unchanged = || {
                        assert_eq!(
                            candidate_workspace_graph(params).get(&head_key).unwrap(),
                            authority
                        );
                        assert_eq!(
                            serde_json::to_vec(&verify_persisted_v1(chain, pin, params).unwrap())
                                .unwrap(),
                            genesis
                        );
                        assert_eq!(
                            load_nov_native_execution_store_v1(path).unwrap(),
                            host_before
                        );
                        assert!(
                            NovNativeBlockLedgerV1::load_fresh_genesis_published_block_v1(
                                &ledger, pin, namespace,
                            )
                            .unwrap()
                            .is_none()
                        );
                    };
                    let plan_for = |amount| {
                        make_plan(
                            NovBlockExecutionContextV1 {
                                chain_id: chain,
                                block_height: 1,
                                parent_block_hash: [0; 32],
                                slot: 1,
                                timestamp_unix_ms: config.timestamp_unix_ms,
                            },
                            compiled.state_root(),
                            None,
                            vec![transfer_candidate_raw(
                                chain, 0, [0x79; 32], [0x7a; 32], amount,
                            )],
                        )
                    };
                    let id = workspace::create_from_genesis_v1(&plan_for(10), pin, params)
                        .unwrap()
                        .workspace_id;
                    assert!(load_executed_genesis_candidate_v1(&config, id, params).is_err());
                    let result = workspace::execute_v1(chain, id, params).unwrap();
                    assert_candidate_workspace_execution_complete(&result);
                    let candidate =
                        load_executed_genesis_candidate_v1(&config, id, params).unwrap();
                    assert_eq!(candidate.workspace_id(), id);
                    assert_eq!(candidate.block().header.height, 1);
                    assert!(!candidate.block().header.finalized);
                    assert_ne!(
                        candidate.profile_commitment(),
                        config.protocol_config_commitment
                    );
                    assert_ne!(
                        candidate.validators().hash(),
                        compiled.validator_set().validator_set_hash
                    );
                    // Captured from this fixed, real-AOEM input when introducing
                    // the v1 statement. A codec change must not silently change
                    // this profile's signed bytes, even on a different platform.
                    assert_eq!(
                        to_hex(&candidate.value()),
                        "d7ef25d92de551d5d856a23595a8347d3db132fa87db88c6d3ea70e99fef8860"
                    );
                    let (proposal, certificate) = evidence(&candidate);
                    let verified =
                        verify_genesis_decision_v1(&config, id, params, &proposal, &certificate)
                            .unwrap();
                    assert_eq!(verified.candidate().value(), candidate.value());
                    assert_eq!(verified.decision().value(), candidate.value());
                    let fingerprint = (
                        candidate.context(),
                        candidate.value(),
                        candidate.block().clone(),
                    );
                    if let Some((old_namespace, old_id, old_fingerprint)) = &previous {
                        assert_ne!(*old_namespace, namespace);
                        assert_ne!(*old_id, id);
                        assert_eq!(
                            old_fingerprint, &fingerprint,
                            "local storage identity must not alter a consensus value"
                        );
                    }
                    previous = Some((namespace, id, fingerprint));
                    unchanged();

                    let mut few = certificate.clone();
                    few.votes.truncate(2);
                    assert!(
                        verify_genesis_decision_v1(&config, id, params, &proposal, &few).is_err()
                    );
                    let mut invalid = proposal.clone();
                    invalid.signature[0] ^= 1;
                    assert!(verify_genesis_decision_v1(
                        &config,
                        id,
                        params,
                        &invalid,
                        &certificate
                    )
                    .is_err());
                    let mut other_config = config.clone();
                    other_config.timestamp_unix_ms += 1;
                    assert!(verify_genesis_decision_v1(
                        &other_config,
                        id,
                        params,
                        &proposal,
                        &certificate
                    )
                    .is_err());
                    let other = workspace::create_from_genesis_v1(&plan_for(11), pin, params)
                        .unwrap()
                        .workspace_id;
                    workspace::execute_v1(chain, other, params).unwrap();
                    assert!(verify_genesis_decision_v1(
                        &config,
                        other,
                        params,
                        &proposal,
                        &certificate
                    )
                    .is_err());
                    unchanged();

                    // A previously returned checked wrapper cannot hide later
                    // storage damage: the real node entry re-reads all evidence.
                    workspace::corrupt_execution_output_for_test_v1(chain, id, params).unwrap();
                    assert!(verify_genesis_decision_v1(
                        &config,
                        id,
                        params,
                        &proposal,
                        &certificate
                    )
                    .is_err());
                    unchanged();
                });
            }
        });
    }
}
