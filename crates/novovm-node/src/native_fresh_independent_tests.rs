// Independent storage executions, not shared AOEM readback. Consensus signers
// are local fixtures; this is not physical multi-machine consensus acceptance.
#[test]
fn candidate_workspace_execution_fresh_genesis_independent_storage_parity() {
    use crate::native_block_ledger::{
        NovNativeBlockLedgerV1 as Ledger, NovNativeFreshFinalityProofV1 as Proof,
    };
    use crate::native_block_seal::commit_v3::NovNativeSealDecisionCertificateV3 as Decision;
    use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
    use crate::native_block_seal::{
        NovNativeBlockSealStoreV1 as Seal, NovNativeSealLocalProposalRequestV1 as Request,
        NovNativeSealQuorumCertificateV1 as Qc,
    };
    use crate::native_block_seal_overlay::{
        NovNativeSealEpochAuthorityV1 as Authority,
        NovNativeSealValidatorTransportBindingV1 as Binding,
    };
    use crate::tx_ingress::fresh_genesis::{
        FreshGenesisConfigV1, GenesisAllocationV1, GenesisValidatorV1, GENESIS_SCHEMA_V1,
    };
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let chain = 98_917_412;
    let mut runs = Vec::new();
    for _ in 0..2 {
        runs.push(with_plan_runtime(|path, params| {
            let config = FreshGenesisConfigV1 {
                schema: GENESIS_SCHEMA_V1.into(),
                chain_id: chain,
                timestamp_unix_ms: 1900000000000,
                protocol_config_commitment: parse_fixed_hex_32_v1(
                    &native_business_protocol_config_commitment_v1().unwrap(),
                    "protocol",
                )
                .unwrap(),
                allocations: vec![GenesisAllocationV1 {
                    account: novovm_adapter_novovm::address_from_seed_v1([0xc3; 32])
                        .try_into()
                        .unwrap(),
                    nov: "1000".into(),
                }],
                total_initial_nov: "1000".into(),
                validators: (1..=4)
                    .map(|seed| GenesisValidatorV1 {
                        public_key: ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
                            .verifying_key()
                            .to_bytes(),
                        weight: 1,
                    })
                    .collect(),
            };
            let compiled = config.compile().unwrap();
            let pin = compiled.config_commitment();
            let set = compiled.validator_set();
            let namespace = parse_fixed_hex_32_v1(
                &native_aoem_owned_state_namespace_digest_v1(params, chain),
                "namespace",
            )
            .unwrap();
            let authority = Authority::derive_operator_pinned_fresh_genesis_epoch(
                &config,
                pin,
                set.validators
                    .iter()
                    .map(|v| Binding {
                        validator_id: v.validator_id,
                        transport_peer_id: novovm_network::peer_id_from_ed25519_public_key_v1(
                            &v.public_key,
                        ),
                    })
                    .collect(),
            )
            .unwrap();
            let mut keys = (1..=4)
                .map(|seed| ed25519_dalek::SigningKey::from_bytes(&[seed; 32]))
                .collect::<Vec<_>>();
            keys.sort_by_key(|key| {
                crate::native_block_seal::NovNativeSealValidatorV1::new(
                    key.verifying_key().to_bytes(),
                    1,
                )
                .unwrap()
                .validator_id
            });
            let ledger = nov_native_block_ledger_rocksdb_path_v1(path);
            let db_path = native_aoem_owned_state_db_path_v1(params);
            assert!(!db_path.exists());
            crate::tx_ingress::fresh_genesis::publication::initialize_v1(&config, pin, params)
                .unwrap();
            let mut parent = None;
            let mut blocks = Vec::new();
            let mut proofs = Vec::new();
            let mut ids = Vec::new();
            for height in 1..=3 {
                let raw = candidate_workspace_execution_raw(
                    chain,
                    height - 1,
                    [0xc3; 32],
                    10,
                    "deposit_reserve",
                );
                let context = NovBlockExecutionContextV1 {
                    chain_id: chain,
                    block_height: height,
                    parent_block_hash: blocks.last().map_or(
                        [0; 32],
                        |block: &crate::native_block_ledger::NovNativeDurableBlockV1| {
                            block.header.block_hash
                        },
                    ),
                    slot: height,
                    timestamp_unix_ms: config.timestamp_unix_ms + height * 250,
                };
                let input = if let Some(parent_id) = parent {
                    let image =
                        workspace::load_finalized_genesis_parent_v1(chain, parent_id, pin, params)
                            .unwrap();
                    let plan = image.successor_plan(context, vec![raw], params).unwrap();
                    workspace::create_from_finalized_genesis_v1(&plan, parent_id, pin, params)
                        .unwrap()
                } else {
                    let plan = make_plan(context, compiled.state_root(), None, vec![raw]);
                    workspace::create_from_genesis_v1(&plan, pin, params).unwrap()
                };
                let id = input.workspace_id;
                let execution = workspace::execute_v1(chain, id, params).unwrap();
                assert_candidate_workspace_execution_complete(&execution);
                assert!(execution
                    .batch_result
                    .per_tx_receipts
                    .iter()
                    .all(|r| r.status_ok));
                if let Some(parent_id) = parent {
                    workspace::register_finalized_successor_v1(chain, parent_id, id, pin, params)
                        .unwrap();
                } else {
                    workspace::register_genesis_block_candidate_v1(chain, id, pin, params).unwrap();
                }
                let artifact = workspace::load_block_artifact_v1(chain, id, params)
                    .unwrap()
                    .unwrap();
                let seal_paths = (0..4)
                    .map(|index| path.with_extension(format!("independent-seal-{index}")))
                    .collect::<Vec<_>>();
                let stores = seal_paths
                    .iter()
                    .map(|path| Seal::open(path).unwrap())
                    .collect::<Vec<_>>();
                let sign = |view: &Ledger| -> Result<Proof> {
                    let leader = authority.expected_leader(height, 0)?;
                    let index = set
                        .validators
                        .iter()
                        .position(|v| v.validator_id == leader)
                        .unwrap();
                    let proposal = stores[index].sign_local_proposal(
                        view,
                        &Request {
                            chain_id: chain,
                            block_hash: artifact.block().header.block_hash,
                            round: 0,
                            justify_qc_hash: None,
                        },
                        set,
                        &keys[index],
                    )?;
                    let votes = (0..3)
                        .map(|i| stores[i].sign_local_vote(view, &proposal, set, &keys[i]))
                        .collect::<Result<Vec<_>>>()?;
                    let qc = Qc::from_votes(proposal.subject.clone(), set, votes)?;
                    let votes = (0..3)
                        .map(|i| {
                            stores[i].persist_local_verified_qc(view, &qc, set)?;
                            stores[i].sign_local_decision_vote_v3(view, &qc, set, &keys[i])
                        })
                        .collect::<Result<Vec<_>>>()?;
                    let decision = Decision::from_votes(qc, set, votes)?;
                    stores[0]
                        .persist_local_verified_decision_certificate_v3(view, &decision, set)?;
                    Ok(Proof {
                        authority: authority.clone(),
                        witness: Message::DecisionCertificateV3 {
                            proposal: Box::new(proposal),
                            decision: Box::new(decision),
                            certificate: None,
                        },
                    })
                };
                let proof = if let Some(parent_id) = parent {
                    workspace::with_verified_finalized_successor_v1(
                        chain, parent_id, id, pin, params, sign,
                    )
                } else {
                    workspace::with_verified_genesis_block_candidate_v1(
                        chain, id, pin, params, sign,
                    )
                }
                .unwrap();
                drop(stores);
                if let Some(parent_id) = parent {
                    assert!(
                        workspace::resume_successor_promotion_v1(
                            chain, parent_id, id, pin, &proof, &ledger, params
                        )
                        .unwrap()
                        .finalized
                    );
                } else {
                    workspace::resume_genesis_promotion_v1(
                        chain,
                        id,
                        pin,
                        &seal_paths[0],
                        &ledger,
                        params,
                    )
                    .unwrap();
                    assert!(
                        workspace::finalize_genesis_promotion_v1(chain, id, pin, &proof, params)
                            .unwrap()
                            .finalized
                    );
                }
                reset_native_aoem_semantic_ingress_session_v1();
                let recovered =
                    workspace::load_finalized_genesis_parent_v1(chain, id, pin, params).unwrap();
                assert_eq!(recovered.block(), artifact.block());
                assert_eq!(
                    Ledger::load_fresh_finality_by_height_v1(&ledger, pin, namespace, height)
                        .unwrap()
                        .as_ref(),
                    Some(&proof)
                );
                blocks.push(artifact.block().clone());
                proofs.push(proof);
                ids.push(id);
                parent = Some(id);
            }
            (db_path, namespace, ids, blocks, proofs)
        }));
    }
    assert_ne!(runs[0].0, runs[1].0, "must not reuse an AOEM database");
    assert_ne!(
        runs[0].1, runs[1].1,
        "fixture namespaces must be independent"
    );
    assert_ne!(
        runs[0].2, runs[1].2,
        "local workspace IDs must not be imported"
    );
    assert_eq!(
        runs[0].3, runs[1].3,
        "complete durable blocks must agree across storage domains"
    );
    assert_eq!(
        runs[0].4, runs[1].4,
        "V3 decisions must bind identical shared consensus subjects"
    );
}
