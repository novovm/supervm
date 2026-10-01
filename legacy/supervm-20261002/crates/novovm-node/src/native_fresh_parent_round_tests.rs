use crate::native_block_seal::newview::NovNativeSealNewViewCertificateV1 as ParentNewView;
use crate::native_block_seal::timeout::{
    NovNativeSealRoundTimerV1 as ParentTimer, NovNativeSealTimeoutCertificateV1 as ParentTimeout,
};
use crate::native_block_seal::NovNativeBlockSealStoreV1 as ParentSeal;

fn with_parent_round_fixture(
    test: impl FnOnce(&Path, &serde_json::Value, IndependentFreshRun) + Send + 'static,
) {
    with_parent_round_fixture_at(3, test);
}

fn with_parent_round_fixture_at(
    height: u64,
    test: impl FnOnce(&Path, &serde_json::Value, IndependentFreshRun) + Send + 'static,
) {
    std::thread::Builder::new()
        .name("candidate-less-round-test".into())
        .stack_size(crate::native_block_seal::service::FRESH_CHAIN_LIFECYCLE_STACK_BYTES_V1)
        .spawn(move || {
            let _guard = PLAN_RUNTIME_TEST_LOCK
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            with_plan_runtime(|path, params| {
                let fixture = independent_fresh_storage_run_through(path, params, None, height);
                test(path, params, fixture);
            });
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn candidate_less_round_first_finalized_parent_authorizes_only_next_height() {
    with_parent_round_fixture_at(1, |path, params, fixture| {
        let authority = &fixture.4[0].authority;
        let chain = authority.chain_id;
        let parent_id = fixture.2[0];
        let artifact = workspace::load_block_artifact_v1(chain, parent_id, params)
            .unwrap()
            .unwrap();
        let pin = artifact
            .fresh_genesis_identity()
            .unwrap()
            .config_commitment();
        workspace::with_verified_finalized_parent_round_v1(chain, parent_id, pin, params, |view| {
            authority.validate_against_ledger(view)?;
            let store = ParentSeal::open(&path.with_extension("parent-round-first"))?;
            let set = &authority.validator_set;
            assert_eq!(view.fresh_round_height_v1()?, Some(2));
            assert!(view.load_candidate_records_by_height(chain, 2)?.is_empty());
            assert!(store.start_round_tracking(view, set, 1).is_err());
            assert!(store.start_round_tracking(view, set, 3).is_err());
            store.start_round_tracking(view, set, 2)?;
            store
                .sign_local_timeout(
                    view,
                    set,
                    2,
                    0,
                    &ed25519_dalek::SigningKey::from_bytes(&[1; 32]),
                )?
                .verify(set)?;
            Ok(())
        })
        .unwrap();
    });
}

#[test]
fn candidate_less_round_corrupt_safety_state_refuses_resigning() {
    with_parent_round_fixture(|path, params, fixture| {
        let authority = &fixture.4.last().unwrap().authority;
        let chain = authority.chain_id;
        let set = &authority.validator_set;
        let parent_id = *fixture.2.last().unwrap();
        let artifact = workspace::load_block_artifact_v1(chain, parent_id, params)
            .unwrap()
            .unwrap();
        let pin = artifact
            .fresh_genesis_identity()
            .unwrap()
            .config_commitment();
        let height = artifact.block().header.height + 1;
        let key = ed25519_dalek::SigningKey::from_bytes(&[1; 32]);
        let signer = crate::native_block_seal::NovNativeSealValidatorV1::new(
            key.verifying_key().to_bytes(),
            1,
        )
        .unwrap()
        .validator_id;
        for fault in ["missing_watermark", "bad_vote", "bad_round"] {
            let store_path = path.with_extension(format!("parent-round-{fault}"));
            workspace::with_verified_finalized_parent_round_v1(
                chain,
                parent_id,
                pin,
                params,
                |view| {
                    let store = ParentSeal::open(&store_path)?;
                    store.start_round_tracking(view, set, height)?;
                    store.sign_local_timeout(view, set, height, 0, &key)?;
                    Ok(())
                },
            )
            .unwrap();
            let snapshot = || {
                let database = rocksdb::DB::open_default(&store_path).unwrap();
                database
                    .iterator(rocksdb::IteratorMode::Start)
                    .map(|entry| {
                        let (key, value) = entry.unwrap();
                        (key.to_vec(), value.to_vec())
                    })
                    .collect::<Vec<_>>()
            };
            {
                let database = rocksdb::DB::open_default(&store_path).unwrap();
                let prefix = format!(
                    "native_block_seal/v1/timeout/{}/{}/{}/",
                    chain,
                    set.epoch,
                    to_hex(&signer)
                );
                let slot = match fault {
                    "missing_watermark" => format!("{prefix}watermark"),
                    "bad_vote" => format!("{prefix}vote/{height}/0"),
                    _ => format!(
                        "native_block_seal/v1/round-state/{}/{}/{height}",
                        chain, set.epoch
                    ),
                };
                assert!(database.get(slot.as_bytes()).unwrap().is_some());
                if fault == "missing_watermark" {
                    database.delete(slot.as_bytes()).unwrap();
                } else {
                    database
                        .put(slot.as_bytes(), b"invalid durable safety record")
                        .unwrap();
                }
                database.flush().unwrap();
            }
            let damaged = snapshot();
            workspace::with_verified_finalized_parent_round_v1(
                chain,
                parent_id,
                pin,
                params,
                |view| {
                    let store = ParentSeal::open(&store_path)?;
                    if fault == "bad_round" {
                        assert!(store.start_round_tracking(view, set, height).is_err());
                    } else {
                        assert!(store
                            .load_local_timeout(view, set, height, 0, signer)
                            .is_err());
                    }
                    assert!(store
                        .sign_local_timeout(view, set, height, 0, &key)
                        .is_err());
                    assert!(store.load_pending_outbox(chain, signer, 128)?.is_empty());
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(snapshot(), damaged);
        }
    });
}

#[test]
fn candidate_less_round_quorum_restart_and_verified_candidate_handoff() {
    with_parent_round_fixture(|path, params, fixture| {
        let authority = &fixture.4.last().unwrap().authority;
        let chain = authority.chain_id;
        let set = &authority.validator_set;
        let parent_id = *fixture.2.last().unwrap();
        let pin = workspace::load_block_artifact_v1(chain, parent_id, params)
            .unwrap()
            .unwrap()
            .fresh_genesis_identity()
            .unwrap()
            .config_commitment();
        let parent =
            workspace::load_finalized_genesis_parent_v1(chain, parent_id, pin, params).unwrap();
        let height = parent.block().header.height + 1;
        let initial = authority.scheduled_leader_v1(height, 0).unwrap();
        let keys = (1..=4)
            .map(|seed| ed25519_dalek::SigningKey::from_bytes(&[seed; 32]))
            .filter(|key| {
                crate::native_block_seal::NovNativeSealValidatorV1::new(
                    key.verifying_key().to_bytes(),
                    1,
                )
                .unwrap()
                .validator_id
                    != initial
            })
            .collect::<Vec<_>>();
        assert_eq!(keys.len(), 3);
        let before = workspace::list_v1(chain, params).unwrap();
        let paths = (0..keys.len())
            .map(|index| path.with_extension(format!("parent-round-{index}")))
            .collect::<Vec<_>>();
        let started = std::time::Instant::now();
        let interval = std::time::Duration::from_secs(2);
        let (votes, observations, certificate) =
            workspace::with_verified_finalized_parent_round_v1(
                chain,
                parent_id,
                pin,
                params,
                |view| {
                    authority.validate_against_ledger(view)?;
                    assert_eq!(view.fresh_round_height_v1()?, Some(height));
                    assert_eq!(view.fresh_successor_height_v1()?, None);
                    assert!(view
                        .load_candidate_records_by_height(chain, height)?
                        .is_empty());
                    let stores = paths
                        .iter()
                        .map(|path| ParentSeal::open(path))
                        .collect::<Result<Vec<_>>>()?;
                    let mut votes = Vec::new();
                    for (store, key) in stores.iter().zip(&keys) {
                        let state = store.start_round_tracking(view, set, height)?;
                        assert_eq!(state.current.round, 0);
                        let timer = ParentTimer::new(&state, started, interval)?;
                        assert!(timer
                            .poll(
                                started + interval - std::time::Duration::from_millis(1),
                                store,
                                view,
                                set,
                                key
                            )?
                            .is_none());
                        assert!(store
                            .sign_local_new_view(view, authority, height, 1, key)
                            .is_err());
                        votes.push(
                            timer
                                .poll(started + interval, store, view, set, key)?
                                .unwrap(),
                        );
                        assert!(store
                            .load_pending_outbox(chain, votes.last().unwrap().validator_id, 128)?
                            .is_empty());
                    }
                    let context = votes[0].context.clone();
                    let short = ParentTimeout {
                        context: context.clone(),
                        votes: votes[..2].to_vec(),
                    };
                    assert!(stores[0].advance_round_tracking(view, set, &short).is_err());
                    let duplicate = ParentTimeout {
                        context: context.clone(),
                        votes: vec![votes[0].clone(); 3],
                    };
                    assert!(stores[0]
                        .advance_round_tracking(view, set, &duplicate)
                        .is_err());
                    let mut altered = ParentTimeout {
                        context: context.clone(),
                        votes: votes.clone(),
                    };
                    altered.votes[0].signature[0] ^= 1;
                    assert!(stores[0]
                        .advance_round_tracking(view, set, &altered)
                        .is_err());
                    assert_eq!(
                        stores[0]
                            .load_round_tracking(view, set, height)?
                            .unwrap()
                            .current
                            .round,
                        0
                    );
                    let certificate = ParentTimeout {
                        context,
                        votes: votes.clone(),
                    };
                    let mut observations = Vec::new();
                    for (store, key) in stores.iter().zip(&keys) {
                        let state = store.advance_round_tracking(view, set, &certificate)?;
                        assert_eq!(state.current.round, 1);
                        assert!(store.sign_local_timeout(view, set, height, 0, key).is_err());
                        observations
                            .push(store.sign_local_new_view(view, authority, height, 1, key)?);
                        assert!(observations.last().unwrap().highest_qc.is_none());
                    }
                    let new_view = ParentNewView {
                        schema: "novovm-native-seal-new-view-certificate/v1".into(),
                        authority_commitment: authority.authority_commitment,
                        context: observations[0].context.clone(),
                        previous_timeout: certificate.clone(),
                        observations: observations.clone(),
                    };
                    assert!(new_view.verify(&new_view.context, authority)?.is_none());
                    let replacement = authority.scheduled_leader_v1(height, 1)?;
                    assert_ne!(replacement, initial);
                    assert!(observations
                        .iter()
                        .any(|observation| observation.validator_id == replacement));
                    Ok((votes, observations, certificate))
                },
            )
            .unwrap();
        workspace::with_verified_finalized_parent_round_v1(chain, parent_id, pin, params, |view| {
            for (index, path) in paths.iter().enumerate() {
                let store = ParentSeal::open(path)?;
                let state = store.start_round_tracking(view, set, height)?;
                assert_eq!(state.current.round, 1);
                assert_eq!(state.previous_timeout, Some(certificate.clone()));
                assert_eq!(
                    store.load_local_timeout(view, set, height, 0, votes[index].validator_id)?,
                    Some(votes[index].clone())
                );
                assert_eq!(
                    store.load_local_new_view(
                        view,
                        authority,
                        height,
                        1,
                        votes[index].validator_id
                    )?,
                    Some(observations[index].clone())
                );
                assert_eq!(
                    store.sign_local_new_view(view, authority, height, 1, &keys[index])?,
                    observations[index]
                );
                let timer = ParentTimer::new(
                    &state,
                    started + std::time::Duration::from_secs(60),
                    interval,
                )?;
                assert!(timer
                    .poll(
                        started + std::time::Duration::from_secs(60),
                        &store,
                        view,
                        set,
                        &keys[index]
                    )?
                    .is_none());
                assert!(store
                    .load_pending_outbox(chain, votes[index].validator_id, 128)?
                    .is_empty());
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(workspace::list_v1(chain, params).unwrap(), before);
        let after =
            workspace::load_finalized_genesis_parent_v1(chain, parent_id, pin, params).unwrap();
        assert_eq!(after.block(), parent.block());
        assert_eq!(
            serde_json::to_value(after.state()).unwrap(),
            serde_json::to_value(parent.state()).unwrap()
        );
        assert_eq!(after.finality_proof(), parent.finality_proof());
        let plan = parent
            .successor_plan(
                NovBlockExecutionContextV1 {
                    chain_id: chain,
                    block_height: height,
                    parent_block_hash: parent.block().header.block_hash,
                    slot: parent.block().header.slot + 1,
                    timestamp_unix_ms: parent.block().header.timestamp_unix_ms + 250,
                },
                vec![candidate_workspace_execution_raw(
                    chain,
                    height - 1,
                    [0xc3; 32],
                    10,
                    "deposit_reserve",
                )],
                params,
            )
            .unwrap();
        let child =
            workspace::create_from_finalized_genesis_v1(&plan, parent_id, pin, params).unwrap();
        assert_candidate_workspace_execution_complete(
            &workspace::execute_v1(chain, child.workspace_id, params).unwrap(),
        );
        workspace::register_finalized_successor_v1(
            chain,
            parent_id,
            child.workspace_id,
            pin,
            params,
        )
        .unwrap();
        let artifact = workspace::load_block_artifact_v1(chain, child.workspace_id, params)
            .unwrap()
            .unwrap();
        let new_view = ParentNewView {
            schema: "novovm-native-seal-new-view-certificate/v1".into(),
            authority_commitment: authority.authority_commitment,
            context: observations[0].context.clone(),
            previous_timeout: certificate,
            observations,
        };
        let proof = workspace::with_verified_finalized_successor_v1(chain, parent_id, child.workspace_id, pin, params, |view| {
            let stores = paths.iter().map(|path| ParentSeal::open(path)).collect::<Result<Vec<_>>>()?;
            let request = crate::native_block_seal::NovNativeSealLocalProposalRequestV1 {
                chain_id: chain,
                block_hash: artifact.block().header.block_hash,
                round: 1,
                justify_qc_hash: None,
            };
            let replacement = authority.scheduled_leader_v1(height, 1)?;
            let leader = votes.iter().position(|vote| vote.validator_id == replacement).unwrap();
            assert!(stores[leader].sign_local_proposal(view, &request, set, &keys[leader]).is_err());
            for store in &stores {
                assert_eq!(store.load_round_tracking(view, set, height)?.unwrap().current.round, 1);
                store.admit_local_new_view_candidate(view, authority, &new_view, &request)?;
            }
            let proposal = stores[leader].sign_local_proposal(view, &request, set, &keys[leader])?;
            let prepare_votes = stores.iter().zip(&keys).map(|(store, key)| store.sign_local_vote(view, &proposal, set, key)).collect::<Result<Vec<_>>>()?;
            let qc = crate::native_block_seal::NovNativeSealQuorumCertificateV1::from_votes(proposal.subject.clone(), set, prepare_votes)?;
            let decisions = stores.iter().zip(&keys).map(|(store, key)| {
                store.persist_local_verified_qc(view, &qc, set)?;
                store.sign_local_decision_vote_v3(view, &qc, set, key)
            }).collect::<Result<Vec<_>>>()?;
            let decision = crate::native_block_seal::commit_v3::NovNativeSealDecisionCertificateV3::from_votes(qc, set, decisions)?;
            stores[leader].persist_local_verified_decision_certificate_v3(view, &decision, set)?;
            Ok(crate::native_block_ledger::NovNativeFreshFinalityProofV1 {
                authority: authority.clone(),
                witness: crate::native_block_seal::round_message::NovNativeSealRoundMessageV1::DecisionCertificateV3 {
                    proposal: Box::new(proposal),
                    decision: Box::new(decision),
                    certificate: Some(Box::new(new_view)),
                },
            })
        }).unwrap();
        assert_eq!(
            workspace::load_finalized_genesis_parent_v1(chain, parent_id, pin, params)
                .unwrap()
                .block(),
            parent.block()
        );
        let ledger = nov_native_block_ledger_rocksdb_path_v1(path);
        let publication = workspace::resume_successor_promotion_v1(
            chain,
            parent_id,
            child.workspace_id,
            pin,
            &proof,
            &ledger,
            params,
        )
        .unwrap();
        assert!(publication.finalized);
        assert_eq!(
            workspace::load_finalized_genesis_parent_v1(chain, child.workspace_id, pin, params)
                .unwrap()
                .block(),
            artifact.block()
        );
        assert!(workspace::with_verified_finalized_parent_round_v1(
            chain,
            parent_id,
            pin,
            params,
            |_| Ok(())
        )
        .is_err());
    });
}

#[test]
fn candidate_less_round_rejects_wrong_domain_height_and_block_signatures() {
    with_parent_round_fixture(|path, params, fixture| {
        let authority = &fixture.4.last().unwrap().authority;
        let chain = authority.chain_id;
        let set = &authority.validator_set;
        let parent_id = *fixture.2.last().unwrap();
        let artifact = workspace::load_block_artifact_v1(chain, parent_id, params)
            .unwrap()
            .unwrap();
        let pin = artifact
            .fresh_genesis_identity()
            .unwrap()
            .config_commitment();
        let height = artifact.block().header.height + 1;
        let key = ed25519_dalek::SigningKey::from_bytes(&[1; 32]);
        let store = ParentSeal::open(&path.with_extension("parent-round-guards")).unwrap();
        let signer = crate::native_block_seal::NovNativeSealValidatorV1::new(
            key.verifying_key().to_bytes(),
            1,
        )
        .unwrap()
        .validator_id;
        workspace::with_verified_finalized_parent_round_v1(chain, parent_id, pin, params, |view| {
            for wrong in [1, height - 1, height + 1, u64::MAX] {
                assert!(store.start_round_tracking(view, set, wrong).is_err());
                assert!(store.sign_local_timeout(view, set, wrong, 0, &key).is_err());
            }
            let mut foreign = set.clone();
            foreign.chain_id += 1;
            assert!(store.start_round_tracking(view, &foreign, height).is_err());
            store.start_round_tracking(view, set, height)?;
            assert!(store
                .sign_local_timeout(
                    view,
                    set,
                    height,
                    0,
                    &ed25519_dalek::SigningKey::from_bytes(&[99; 32])
                )
                .is_err());
            for hash in [artifact.block().header.block_hash, [0xa5; 32]] {
                let error = store
                    .prepare_local_subject(view, chain, hash, set, 0, None)
                    .unwrap_err();
                assert!(error
                    .to_string()
                    .contains("cannot authorize candidate signatures"));
                assert!(store
                    .sign_local_proposal(
                        view,
                        &crate::native_block_seal::NovNativeSealLocalProposalRequestV1 {
                            chain_id: chain,
                            block_hash: hash,
                            round: 0,
                            justify_qc_hash: None,
                        },
                        set,
                        &key
                    )
                    .is_err());
            }
            let proposal = fixture.4.last().unwrap().witness.proposal().unwrap();
            assert!(store.sign_local_vote(view, proposal, set, &key).is_err());
            assert!(store.load_pending_outbox(chain, signer, 128)?.is_empty());
            assert!(store
                .load_local_timeout(view, set, height, 0, signer)?
                .is_none());
            Ok(())
        })
        .unwrap();
        for (bad_chain, bad_parent, bad_pin) in [
            (chain + 1, parent_id, pin),
            (chain, fixture.2[1], pin),
            (chain, parent_id, [0xa5; 32]),
        ] {
            assert!(workspace::with_verified_finalized_parent_round_v1(
                bad_chain,
                bad_parent,
                bad_pin,
                params,
                |_| -> Result<()> {
                    panic!("invalid domain reached parent round callback");
                }
            )
            .is_err());
        }
        workspace::corrupt_execution_output_for_test_v1(chain, parent_id, params).unwrap();
        assert!(workspace::with_verified_finalized_parent_round_v1(
            chain,
            parent_id,
            pin,
            params,
            |_| Ok(())
        )
        .is_err());
    });
}
