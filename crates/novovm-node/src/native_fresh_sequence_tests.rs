// Reuse the same persisted signers and publication APIs across successive heights.
fn exercise_fresh_sequence(
    path: &Path,
    params: &serde_json::Value,
    compiled: &crate::tx_ingress::fresh_genesis::CompiledFreshGenesisV1,
    mut parent: [u8; 32],
    mut candidate: [u8; 32],
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
            candidate = workspace::create_from_finalized_genesis_v1(&plan, parent, pin, params)
                .unwrap()
                .workspace_id;
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
        if height == 4 {
            exercise_fresh_successor_relay(path, params, chain, parent, candidate, pin, &proof);
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
