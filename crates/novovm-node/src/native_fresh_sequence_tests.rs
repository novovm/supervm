// Reuse the same persisted signers and publication APIs across successive heights.
fn exercise_fresh_sequence(
    path: &Path,
    params: &serde_json::Value,
    compiled: &crate::tx_ingress::fresh_genesis::CompiledFreshGenesisV1,
    mut parent: [u8; 32],
    mut candidate: [u8; 32],
    first_plan: &crate::native_candidate_plan::NovNativeCandidateExecutionPlanV1,
) {
    use crate::native_block_ledger::NovNativeBlockLedgerV1 as Ledger;
    use crate::native_block_seal::commit_v3::NovNativeSealDecisionCertificateV3 as Certificate;
    use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
    use crate::native_block_seal::{
        NovNativeBlockSealStoreV1 as Seal, NovNativeSealLocalProposalRequestV1 as Request,
        NovNativeSealQuorumCertificateV1 as Qc,
    };
    let set = compiled.validator_set();
    let chain = set.chain_id;
    let pin = compiled.config_commitment();
    let namespace = parse_fixed_hex_32_v1(
        &native_aoem_owned_state_namespace_digest_v1(params, chain),
        "namespace",
    )
    .unwrap();
    let ledger = nov_native_block_ledger_rocksdb_path_v1(path);
    let first = Ledger::load_fresh_finalized_execution_v1(&ledger, pin, namespace, 1)
        .unwrap()
        .0
        .workspace_id;
    let first_slot = workspace::load_v1(chain, first, params)
        .unwrap()
        .unwrap()
        .slot;
    let mut history = (1..=2)
        .map(|height| {
            Ledger::load_fresh_finality_by_height_v1(&ledger, pin, namespace, height)
                .unwrap()
                .unwrap()
        })
        .collect::<Vec<_>>();
    let mut keys = (1..=4)
        .map(|seed| ed25519_dalek::SigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    keys.sort_by_key(|key| {
        crate::native_block_seal::NovNativeSealValidatorV1::new(key.verifying_key().to_bytes(), 1)
            .unwrap()
            .validator_id
    });
    for height in 3..=4 {
        let parent_image =
            workspace::load_finalized_genesis_parent_v1(chain, parent, pin, params).unwrap();
        if height > 3 {
            let previous = &parent_image.block().header;
            let context = novovm_protocol::NovBlockExecutionContextV1 {
                chain_id: chain,
                block_height: height,
                parent_block_hash: previous.block_hash,
                slot: previous.slot + 1,
                timestamp_unix_ms: previous.timestamp_unix_ms + 1,
            };
            let plan = parent_image
                .successor_plan(
                    context,
                    vec![candidate_workspace_execution_raw(
                        chain,
                        height,
                        [0xc3; 32],
                        10,
                        "deposit_reserve",
                    )],
                    params,
                )
                .unwrap();
            use crate::native_block_seal::service_config::NovNativeSealServiceConfigV1 as Config;
            let config_path = path.with_extension("fresh-service-3").join("service.json");
            let load = || Config::load(&config_path, chain).unwrap();
            let before = workspace::list_v1(chain, params).unwrap();
            let mut wrong_parent = load();
            wrong_parent.block_hash[0] ^= 1;
            assert!(wrong_parent
                .prepare_fresh_successor(
                    context.slot,
                    context.timestamp_unix_ms,
                    plan.raw_txs.clone(),
                    params
                )
                .is_err());
            let mut wrong_ancestor = load();
            wrong_ancestor.finalized_parent_workspace_id = Some([9; 32]);
            assert!(wrong_ancestor
                .prepare_fresh_successor(
                    context.slot,
                    context.timestamp_unix_ms,
                    plan.raw_txs.clone(),
                    params
                )
                .is_err());
            assert!(load()
                .prepare_fresh_successor(
                    context.slot,
                    context.timestamp_unix_ms,
                    vec![vec![0; 32]],
                    params
                )
                .is_err());
            assert_eq!(workspace::list_v1(chain, params).unwrap(), before);
            let next = load()
                .prepare_fresh_successor(
                    context.slot,
                    context.timestamp_unix_ms,
                    plan.raw_txs.clone(),
                    params,
                )
                .unwrap();
            candidate = next.isolated_workspace_id.unwrap();
            assert_eq!(next.height, height);
            assert_eq!(next.finalized_parent_workspace_id, Some(parent));
            assert_eq!(next.seal_store_path, load().seal_store_path);
            assert_eq!(next.local_validator_id, load().local_validator_id);
            assert_eq!(next.authority, load().authority);
            let retry = load()
                .prepare_fresh_successor(
                    context.slot,
                    context.timestamp_unix_ms,
                    plan.raw_txs.clone(),
                    params,
                )
                .unwrap();
            assert_eq!(retry.isolated_workspace_id, Some(candidate));
            assert_eq!(retry.block_hash, next.block_hash);
            crate::native_block_seal::tests::native_seal_round_network::with_service_test_transports(chain, |peers| {
                let (runtime, _) = peers.iter().find(|(_, key)|
                    key.verifying_key() == next.signer.verifying_key()).unwrap();
                let mut service = crate::native_block_seal::service::NovNativeSealServiceV1::open_configured(
                    next, &ledger, params, runtime, std::time::Instant::now()).unwrap();
                service.poll(runtime, std::time::Instant::now()).unwrap();
                assert_eq!(service.status_json()["height"], height);
                assert_eq!(service.status_json()["finalized"], false);
                assert!(!service.halted());
            });
            assert!(retry
                .prepare_fresh_successor(
                    context.slot + 1,
                    context.timestamp_unix_ms + 1,
                    plan.raw_txs.clone(),
                    params
                )
                .is_err()); // Unconfirmed child cannot parent another block.
            assert_eq!(
                workspace::load_v1(chain, candidate, params)
                    .unwrap()
                    .unwrap()
                    .slot,
                first_slot
            );
            workspace::retire_old_workspaces_v1(chain, parent, pin, params).unwrap();
            assert!(workspace::load_v1(chain, candidate, params)
                .unwrap()
                .is_some());
            assert!(
                workspace::execute_v1(chain, candidate, params)
                    .unwrap()
                    .execution_completed
            );
        }
        workspace::register_finalized_successor_v1(chain, parent, candidate, pin, params).unwrap();
        if height == 3 {
            exercise_fresh_candidate_service(path, params, compiled, candidate, Some(parent));
        }
        let artifact = workspace::load_block_artifact_v1(chain, candidate, params)
            .unwrap()
            .unwrap();
        let authority = parent_image.finality_proof().authority.clone();
        let leader = authority.expected_leader(height, 0).unwrap();
        let leader_index = set
            .validators
            .iter()
            .position(|v| v.validator_id == leader)
            .unwrap();
        let stores = (0..4)
            .map(|index| Seal::open(&path.with_extension(format!("genesis-seal-{index}"))).unwrap())
            .collect::<Vec<_>>();
        let request = Request {
            chain_id: chain,
            block_hash: artifact.block().header.block_hash,
            round: 0,
            justify_qc_hash: None,
        };
        let proposal = workspace::with_verified_finalized_successor_v1(
            chain,
            parent,
            candidate,
            pin,
            params,
            |view| {
                stores[leader_index].sign_local_proposal(view, &request, set, &keys[leader_index])
            },
        )
        .unwrap();
        let votes = (0..3)
            .map(|i| {
                workspace::with_verified_finalized_successor_v1(
                    chain,
                    parent,
                    candidate,
                    pin,
                    params,
                    |view| stores[i].sign_local_vote(view, &proposal, set, &keys[i]),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        assert!(Qc::from_votes(proposal.subject.clone(), set, votes[..2].to_vec()).is_err());
        let qc = Qc::from_votes(proposal.subject.clone(), set, votes).unwrap();
        let decisions = (0..3)
            .map(|i| {
                workspace::with_verified_finalized_successor_v1(
                    chain,
                    parent,
                    candidate,
                    pin,
                    params,
                    |view| {
                        stores[i].persist_local_verified_qc(view, &qc, set)?;
                        stores[i].sign_local_decision_vote_v3(view, &qc, set, &keys[i])
                    },
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        assert!(Certificate::from_votes(qc.clone(), set, decisions[..2].to_vec()).is_err());
        let decision = Certificate::from_votes(qc, set, decisions).unwrap();
        workspace::with_verified_finalized_successor_v1(
            chain,
            parent,
            candidate,
            pin,
            params,
            |view| stores[0].persist_local_verified_decision_certificate_v3(view, &decision, set),
        )
        .unwrap();
        drop(stores);
        let proof = crate::native_block_ledger::NovNativeFreshFinalityProofV1 {
            authority,
            witness: Message::DecisionCertificateV3 {
                proposal: Box::new(proposal),
                decision: Box::new(decision),
                certificate: None,
            },
        };
        assert!(
            Ledger::load_fresh_finality_by_height_v1(&ledger, pin, namespace, height)
                .unwrap()
                .is_none()
        );
        workspace::prepare_successor_promotion_v1(chain, parent, candidate, pin, &proof, params)
            .unwrap();
        assert!(workspace::load_finalized_genesis_parent_v1(chain, parent, pin, params).is_err());
        assert!(workspace::complete_successor_with_checkpoint_v1(
            chain,
            parent,
            candidate,
            pin,
            params,
            |point| {
                if point == workspace::PromotionCheckpointV1::AfterLedgerCommit {
                    anyhow::bail!("simulated response loss");
                }
                Ok(())
            }
        )
        .is_err());
        assert!(
            Ledger::load_fresh_finality_by_height_v1(&ledger, pin, namespace, height)
                .unwrap()
                .is_none()
        );
        let report = workspace::resume_successor_promotion_v1(
            chain, parent, candidate, pin, &proof, &ledger, params,
        )
        .unwrap();
        assert!(report.finalized && report.ledger_publication_completed);
        assert_eq!(
            report,
            workspace::resume_successor_promotion_v1(
                chain, parent, candidate, pin, &proof, &ledger, params
            )
            .unwrap()
        );
        history.push(proof.clone());
        for (index, expected) in history.iter().enumerate() {
            assert_eq!(
                Ledger::load_fresh_finality_by_height_v1(&ledger, pin, namespace, index as u64 + 1)
                    .unwrap()
                    .as_ref(),
                Some(expected)
            );
        }
        let current =
            workspace::load_finalized_genesis_parent_v1(chain, candidate, pin, params).unwrap();
        assert_eq!(current.block().header.height, height);
        assert_eq!(current.block().header.state_version, height + 1);
        assert_eq!(current.state().receipts.len(), height as usize + 1);
        if height == 3 {
            let before = workspace::list_v1(chain, params).unwrap();
            assert!(workspace::retire_old_workspaces_v1(chain, parent, pin, params).is_err());
            assert_eq!(workspace::list_v1(chain, params).unwrap(), before);
            for stop in [
                workspace::RetirementCheckpointV1::IntentPersisted,
                workspace::RetirementCheckpointV1::PartialReclaim,
            ] {
                let error =
                    workspace::retire_with_checkpoint_v1(chain, candidate, pin, params, |point| {
                        if point == stop {
                            anyhow::bail!("retirement interruption");
                        }
                        Ok(())
                    })
                    .unwrap_err();
                assert!(error.to_string().contains("retirement interruption"));
                assert_eq!(
                    workspace::load_v1(chain, first, params)
                        .unwrap()
                        .unwrap()
                        .status,
                    workspace::WorkspaceStatusV1::Retiring
                );
                assert!(workspace::execute_v1(chain, first, params).is_err());
                workspace::load_finalized_genesis_parent_v1(chain, candidate, pin, params).unwrap();
            }
            let reclaimed =
                workspace::retire_old_workspaces_v1(chain, candidate, pin, params).unwrap();
            assert!(reclaimed.retired_workspaces.contains(&first));
            assert!(reclaimed.snapshot_bytes_unreferenced > 0);
            assert!(workspace::load_v1(chain, first, params).unwrap().is_none());
            assert!(workspace::create_from_genesis_v1(first_plan, pin, params)
                .unwrap_err()
                .to_string()
                .contains("retired"));
        }
        if height == 4 {
            let old_config =
                crate::native_block_seal::service_config::NovNativeSealServiceConfigV1::load(
                    &path.with_extension("fresh-service-3").join("service.json"),
                    chain,
                )
                .unwrap();
            let before = workspace::list_v1(chain, params).unwrap();
            assert!(old_config
                .prepare_fresh_successor(
                    current.block().header.slot + 1,
                    current.block().header.timestamp_unix_ms + 1,
                    vec![candidate_workspace_execution_raw(
                        chain,
                        5,
                        [0xc3; 32],
                        10,
                        "deposit_reserve"
                    )],
                    params
                )
                .is_err());
            assert_eq!(workspace::list_v1(chain, params).unwrap(), before);
            exercise_fresh_successor_relay(path, params, chain, parent, candidate, pin, &proof);
            workspace::corrupt_execution_output_for_test_v1(chain, parent, params).unwrap();
            let before = workspace::list_v1(chain, params).unwrap();
            assert!(workspace::retire_old_workspaces_v1(chain, candidate, pin, params).is_err());
            assert_eq!(workspace::list_v1(chain, params).unwrap(), before);
            workspace::corrupt_execution_output_for_test_v1(chain, parent, params).unwrap();
            let error =
                workspace::retire_with_checkpoint_v1(chain, candidate, pin, params, |point| {
                    if point == workspace::RetirementCheckpointV1::SlotReleased {
                        anyhow::bail!("slot released response loss");
                    }
                    Ok(())
                })
                .unwrap_err();
            assert!(error.to_string().contains("slot released response loss"));
            workspace::retire_old_workspaces_v1(chain, candidate, pin, params).unwrap();
            for protected in [parent, candidate] {
                assert!(workspace::load_block_artifact_v1(chain, protected, params)
                    .unwrap()
                    .is_some());
            }
            workspace::load_finalized_genesis_parent_v1(chain, candidate, pin, params).unwrap();
            for (index, expected) in history.iter().enumerate() {
                assert_eq!(
                    Ledger::load_fresh_finality_by_height_v1(
                        &ledger,
                        pin,
                        namespace,
                        index as u64 + 1
                    )
                    .unwrap()
                    .as_ref(),
                    Some(expected)
                );
            }
        }
        parent = candidate;
    }
    let key = b"native_block_ledger/v1/successor/finalized/0000000000000002";
    let db = rocksdb::DB::open_default(&ledger).unwrap();
    let original = db.get(key).unwrap().unwrap();
    db.delete(key).unwrap();
    drop(db);
    assert!(Ledger::load_fresh_finality_by_height_v1(&ledger, pin, namespace, 4).is_err());
    assert!(workspace::load_finalized_genesis_parent_v1(chain, candidate, pin, params).is_err());
    assert!(workspace::retire_old_workspaces_v1(chain, candidate, pin, params).is_err());
    let db = rocksdb::DB::open_default(&ledger).unwrap();
    assert!(db.get(key).unwrap().is_none());
    db.put(key, original).unwrap(); // Explicit test fixture restoration, never recovery repair.
    drop(db);
    assert_eq!(
        Ledger::load_fresh_finality_by_height_v1(&ledger, pin, namespace, 2)
            .unwrap()
            .as_ref(),
        Some(&history[1])
    );
}
