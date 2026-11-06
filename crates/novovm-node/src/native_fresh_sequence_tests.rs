include!("native_successor_publication_bundle_tests.rs");

fn with_record_signing_guard<T>(
    record_profile: bool,
    action: impl FnOnce() -> Result<T>,
) -> Result<T> {
    if record_profile {
        workspace::without_materialization_for_test(action)
    } else {
        action()
    }
}

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
    let record_profile = compiled.root_codec_profile()
        == crate::native_root_codecs::NativeRootCodecProfileV1::RecordTreeV1;
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
                    vec![transfer_candidate_raw(
                        chain, height, [0xc3; 32], [0xd4; 32], 10,
                    )],
                    params,
                )
                .unwrap();
            if compiled.root_codec_profile()
                == crate::native_root_codecs::NativeRootCodecProfileV1::RecordTreeV1
            {
                // The third block is a finalized NCW2 Transfer, unlike the
                // second block's legacy Execute input. Earlier workspaces have
                // already been retired; descriptor validation must stay rooted.
                workspace::assert_live_parent_descriptor_binding_for_test_v1(
                    &plan, parent, pin, params,
                )
                .unwrap();
            }
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
            for wrong_id in [[9; 32], parent] {
                // Both an absent ID and a real workspace at the wrong height
                // must fail ancestry validation before anything is staged.
                let mut wrong_ancestor = load();
                wrong_ancestor.finalized_parent_workspace_id = Some(wrong_id);
                let error = with_record_signing_guard(record_profile, || {
                    wrong_ancestor.prepare_fresh_successor(
                        context.slot,
                        context.timestamp_unix_ms,
                        plan.raw_txs.clone(),
                        params,
                    )
                })
                .err()
                .expect("wrong predecessor must fail before candidate staging");
                let diagnostic = format!("{error:#}");
                // Self-parenting is already rejected by config.validate;
                // an absent but otherwise well-formed ID reaches lineage.
                let expected = if wrong_id == parent {
                    "explicit finalized parent for successor heights"
                } else {
                    "predecessor"
                };
                assert!(diagnostic.contains(expected), "{diagnostic}");
                assert!(!diagnostic.contains("unexpected full candidate store materialization"));
                assert_eq!(workspace::list_v1(chain, params).unwrap(), before);
            }
            assert!(load()
                .prepare_fresh_successor(
                    context.slot,
                    context.timestamp_unix_ms,
                    vec![vec![0; 32]],
                    params
                )
                .is_err());
            assert_eq!(workspace::list_v1(chain, params).unwrap(), before);
            let next = with_record_signing_guard(record_profile, || {
                load().prepare_fresh_successor(
                    context.slot,
                    context.timestamp_unix_ms,
                    plan.raw_txs.clone(),
                    params,
                )
            })
            .unwrap();
            candidate = next.isolated_workspace_id.unwrap();
            assert_eq!(next.height, height);
            assert_eq!(next.finalized_parent_workspace_id, Some(parent));
            assert_eq!(next.seal_store_path, load().seal_store_path);
            assert_eq!(next.local_validator_id, load().local_validator_id);
            assert_eq!(next.authority, load().authority);
            let retry = with_record_signing_guard(record_profile, || {
                load().prepare_fresh_successor(
                    context.slot,
                    context.timestamp_unix_ms,
                    plan.raw_txs.clone(),
                    params,
                )
            })
            .unwrap();
            assert_eq!(retry.isolated_workspace_id, Some(candidate));
            assert_eq!(retry.block_hash, next.block_hash);
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
        if compiled.root_codec_profile()
            == crate::native_root_codecs::NativeRootCodecProfileV1::RecordTreeV1
        {
            workspace::assert_light_input_output_point_read_for_test_v1(chain, candidate, params)
                .unwrap();
        }
        if height == 3 {
            with_record_signing_guard(record_profile, || {
                let error = workspace::with_verified_finalized_successor_v1(
                    chain,
                    parent,
                    candidate,
                    pin,
                    params,
                    |_| -> Result<()> {
                        panic!("unregistered third block reached signing callback")
                    },
                )
                .unwrap_err();
                assert!(!format!("{error:#}")
                    .contains("unexpected full candidate store materialization"));
                Ok(())
            })
            .unwrap();
        }
        with_record_signing_guard(record_profile, || {
            workspace::register_finalized_successor_v1(chain, parent, candidate, pin, params)
        })
        .unwrap();
        if record_profile && height == 3 {
            workspace::exercise_live_successor_signing_scope_for_test_v1(
                chain, parent, candidate, pin, params,
            )
            .unwrap();
        }
        if height == 3 {
            prepare_offline_candidate_config(path, params, compiled, candidate, Some(parent));
        }
        let artifact = workspace::load_block_artifact_v1(chain, candidate, params)
            .unwrap()
            .unwrap();
        let authority = parent_image.finality_proof().authority.clone();
        with_record_signing_guard(record_profile, || {
            workspace::with_verified_finalized_parent_round_v1(chain, parent, pin, params, |view| {
                authority.validate_against_ledger(view)?;
                assert_eq!(view.fresh_round_height_v1()?, Some(height));
                let store =
                    Seal::open(&path.with_extension(format!("rooted-parent-round-{height}")))?;
                store.start_round_tracking(view, set, height)?;
                store
                    .sign_local_timeout(view, set, height, 0, &keys[0])?
                    .verify(set)?;
                Ok(())
            })
        })
        .unwrap();
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
        let proposal = with_record_signing_guard(record_profile, || {
            workspace::with_verified_finalized_successor_v1(
                chain,
                parent,
                candidate,
                pin,
                params,
                |view| {
                    stores[leader_index].sign_local_proposal(
                        view,
                        &request,
                        set,
                        &keys[leader_index],
                    )
                },
            )
        })
        .unwrap();
        assert_eq!(
            proposal.subject,
            parent_image.successor_seal_subject(&artifact, 0).unwrap()
        );
        if height == 4 {
            exercise_received_successor_body(
                path,
                params,
                &authority,
                &proposal,
                &artifact.block().body.raw_txs,
                &keys[leader_index],
                candidate,
            );
        }
        let votes = (0..3)
            .map(|i| {
                with_record_signing_guard(record_profile, || {
                    workspace::with_verified_finalized_successor_v1(
                        chain,
                        parent,
                        candidate,
                        pin,
                        params,
                        |view| stores[i].sign_local_vote(view, &proposal, set, &keys[i]),
                    )
                })
                .unwrap()
            })
            .collect::<Vec<_>>();
        if record_profile && height == 4 {
            let catalog_before = workspace::list_v1(chain, params).unwrap();
            let outbox_before = stores[0]
                .load_pending_outbox(chain, set.validators[0].validator_id, 128)
                .unwrap();
            assert!(workspace::reclaim_unregistered_workspaces_v1(
                chain,
                parent,
                pin,
                &[candidate, parent],
                params,
            )
            .unwrap()
            .is_empty());
            assert_eq!(workspace::list_v1(chain, params).unwrap(), catalog_before);
            assert_eq!(
                workspace::load_block_artifact_v1(chain, candidate, params)
                    .unwrap()
                    .unwrap(),
                artifact
            );
            assert_eq!(
                stores[0]
                    .load_pending_outbox(chain, set.validators[0].validator_id, 128)
                    .unwrap(),
                outbox_before
            );
        }
        assert!(Qc::from_votes(proposal.subject.clone(), set, votes[..2].to_vec()).is_err());
        let qc = Qc::from_votes(proposal.subject.clone(), set, votes).unwrap();
        let decisions = (0..3)
            .map(|i| {
                with_record_signing_guard(record_profile, || {
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
                })
                .unwrap()
            })
            .collect::<Vec<_>>();
        assert!(Certificate::from_votes(qc.clone(), set, decisions[..2].to_vec()).is_err());
        let decision = Certificate::from_votes(qc, set, decisions).unwrap();
        with_record_signing_guard(record_profile, || {
            workspace::with_verified_finalized_successor_v1(
                chain,
                parent,
                candidate,
                pin,
                params,
                |view| {
                    stores[0].persist_local_verified_decision_certificate_v3(view, &decision, set)
                },
            )
        })
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
        with_record_signing_guard(record_profile, || {
            workspace::prepare_successor_promotion_v1(chain, parent, candidate, pin, &proof, params)
        })
        .unwrap();
        if record_profile && height == 3 {
            exercise_successor_publication_bundle(
                &ledger,
                pin,
                namespace,
                parent,
                &artifact,
                SuccessorBundleStage::Prepared,
                params,
            );
        }
        // Preparing publication pins the decision; it must never authorize new
        // signing or parent-round callbacks, even though retrying it is allowed.
        with_record_signing_guard(record_profile, || {
            let signer = workspace::with_verified_finalized_successor_v1(
                chain,
                parent,
                candidate,
                pin,
                params,
                |_| -> Result<()> { panic!("pending promotion reached successor signer") },
            )
            .unwrap_err();
            let round = workspace::with_verified_finalized_parent_round_v1(
                chain,
                parent,
                pin,
                params,
                |_| -> Result<()> { panic!("pending promotion reached parent-round signer") },
            )
            .unwrap_err();
            for error in [signer, round] {
                assert!(!format!("{error:#}")
                    .contains("unexpected full candidate store materialization"));
            }
            Ok(())
        })
        .unwrap();
        assert!(workspace::load_finalized_genesis_parent_v1(chain, parent, pin, params).is_err());
        let interrupted = with_record_signing_guard(record_profile, || {
            workspace::complete_successor_with_checkpoint_v1(
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
                },
            )
        })
        .unwrap_err();
        assert!(format!("{interrupted:#}").contains("simulated response loss"));
        if record_profile && height == 3 {
            exercise_successor_publication_bundle(
                &ledger,
                pin,
                namespace,
                parent,
                &artifact,
                SuccessorBundleStage::Published,
                params,
            );
        }
        assert!(
            Ledger::load_fresh_finality_by_height_v1(&ledger, pin, namespace, height)
                .unwrap()
                .is_none()
        );
        if height == 4 {
            let mut configured =
                crate::native_block_seal::service_config::NovNativeSealServiceConfigV1::load(
                    &path.with_extension("fresh-service-3").join("service.json"),
                    chain,
                )
                .unwrap();
            configured.follow_finalized_tip = true;
            assert!(configured
                .clone()
                .resolve_finalized_startup(params)
                .is_err());
        }
        let report = with_record_signing_guard(record_profile, || {
            workspace::resume_successor_promotion_v1(
                chain, parent, candidate, pin, &proof, &ledger, params,
            )
        })
        .unwrap();
        assert!(report.finalized && report.ledger_publication_completed);
        if record_profile && height == 3 {
            exercise_successor_publication_bundle(
                &ledger,
                pin,
                namespace,
                parent,
                &artifact,
                SuccessorBundleStage::Finalized,
                params,
            );
        }
        with_record_signing_guard(record_profile, || {
            let error = workspace::with_verified_finalized_parent_round_v1(
                chain,
                parent,
                pin,
                params,
                |_| -> Result<()> { panic!("stale parent reached parent-round signer") },
            )
            .unwrap_err();
            assert!(
                !format!("{error:#}").contains("unexpected full candidate store materialization")
            );
            Ok(())
        })
        .unwrap();
        assert_eq!(
            report,
            with_record_signing_guard(record_profile, || workspace::resume_successor_promotion_v1(
                chain, parent, candidate, pin, &proof, &ledger, params
            ))
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
            // The same signed Transfer now crosses prepare QC, decision QC,
            // publication interruption, original workspace reopen and final recovery.
            // Immutable records must also survive earlier workspace retirement.
            let raw = &current.block().body.raw_txs[0];
            let fee = transfer_candidate_fee(raw);
            let payer = transfer_candidate_account([0xc3; 32]);
            let recipient = transfer_candidate_account([0xd4; 32]);
            let receipt = &current.state().receipts[&to_hex(&current.block().body.tx_hashes[0])];
            assert!(receipt.status);
            assert_eq!(receipt.settled_fee_nov, fee);
            assert_transfer_candidate_compute_logs(receipt, 1);
            assert_eq!(
                native_account_asset_balance_v1(current.state(), &payer, "NOV"),
                native_account_asset_balance_v1(parent_image.state(), &payer, "NOV") - fee - 10,
            );
            assert_eq!(
                native_account_asset_balance_v1(current.state(), &recipient, "NOV"),
                native_account_asset_balance_v1(parent_image.state(), &recipient, "NOV") + 10,
            );
        }
        if height == 3 {
            workspace::corrupt_first_chunk_for_test_v1(chain, candidate, params).unwrap();
            assert!(workspace::abort_v1(chain, candidate, params).is_err());
            workspace::corrupt_first_chunk_for_test_v1(chain, candidate, params).unwrap();
            workspace::load_finalized_genesis_parent_v1(chain, candidate, pin, params).unwrap();
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
            let original_path = path.with_extension("fresh-service-3").join("service.json");
            let follow_path = original_path.with_extension("follow.json");
            let mut json: serde_json::Value =
                serde_json::from_slice(&fs::read(&original_path).unwrap()).unwrap();
            json["follow_finalized_tip"] = serde_json::json!(true);
            fs::write(&follow_path, serde_json::to_vec(&json).unwrap()).unwrap();
            let follow = || {
                crate::native_block_seal::service_config::NovNativeSealServiceConfigV1::load(
                    &follow_path,
                    chain,
                )
                .unwrap()
            };
            let fixed =
                crate::native_block_seal::service_config::NovNativeSealServiceConfigV1::load(
                    &original_path,
                    chain,
                )
                .unwrap()
                .resolve_finalized_startup(params)
                .unwrap();
            assert_eq!(fixed.height, 3);
            let mut wrong_anchor = follow();
            wrong_anchor.block_hash[0] ^= 1;
            assert!(wrong_anchor.resolve_finalized_startup(params).is_err());
            let resolved = follow().resolve_finalized_startup(params).unwrap();
            assert_eq!(resolved.height, 4);
            assert_eq!(resolved.isolated_workspace_id, Some(candidate));
            assert_eq!(resolved.finalized_parent_workspace_id, Some(parent));
            assert_eq!(resolved.local_validator_id, follow().local_validator_id);
            assert_eq!(
                resolved.resolve_finalized_startup(params).unwrap().height,
                4
            );
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

            workspace::corrupt_execution_output_for_test_v1(chain, parent, params).unwrap();
            assert!(follow().resolve_finalized_startup(params).is_err());
            let before = workspace::list_v1(chain, params).unwrap();
            if compiled.root_codec_profile()
                == crate::native_root_codecs::NativeRootCodecProfileV1::RecordTreeV1
            {
                workspace::without_materialization_for_test(|| {
                    let error =
                        workspace::load_finalized_parent_view_v1(chain, candidate, pin, params)
                            .err()
                            .context("live view accepted a corrupt retained predecessor")?;
                    assert!(!format!("{error:#}")
                        .contains("unexpected full candidate store materialization"));
                    let error = workspace::with_verified_finalized_parent_round_v1(
                        chain,
                        candidate,
                        pin,
                        params,
                        |_| -> Result<()> {
                            panic!("corrupt predecessor reached parent-round signer")
                        },
                    )
                    .unwrap_err();
                    assert!(!format!("{error:#}")
                        .contains("unexpected full candidate store materialization"));
                    Ok(())
                })
                .unwrap();
            }
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
            if compiled.root_codec_profile()
                == crate::native_root_codecs::NativeRootCodecProfileV1::RecordTreeV1
            {
                // Height 3's immediate parent (height 2) is retired. Its
                // embedded source document and publication archive must still
                // support verified reads without the retired parent's chunks.
                let retired_parent =
                    Ledger::load_fresh_finalized_execution_v1(&ledger, pin, namespace, 2)
                        .unwrap()
                        .0
                        .workspace_id;
                assert!(workspace::load_v1(chain, retired_parent, params)
                    .unwrap()
                    .is_none());
                workspace::assert_light_input_output_point_read_for_test_v1(chain, parent, params)
                    .unwrap();
            }
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
    // Both root profiles reach this real, fully finalized four-block history.
    // Exercise same-traversal parent reuse and independent-load corruption
    // rejection before the existing explicit missing-archive recovery fixture.
    Ledger::assert_fresh_history_reuse_for_test_v1(&ledger, pin, namespace);
    let read_counted_finality = |height| {
        crate::tx_ingress::fresh_genesis::count_genesis_compilations_for_test_v1(|| {
            Ledger::count_fresh_ledger_verifications_for_test_v1(|| {
                Ledger::load_fresh_finality_by_height_v1(&ledger, pin, namespace, height)
            })
        })
    };
    // This is a nonempty four-block finalized history for both root profiles.
    // Each independent read recompiles once; reuse is confined to that read,
    // never a cache of configuration validation or ledger/QC authority.
    for _ in 0..2 {
        let ((loaded, verified_loads), compilations) = read_counted_finality(4);
        assert_eq!(loaded.unwrap().as_ref(), history.last());
        assert_eq!(verified_loads, 1);
        assert_eq!(compilations, 1);
    }
    let key = b"native_block_ledger/v1/successor/finalized/0000000000000002";
    let db = rocksdb::DB::open_default(&ledger).unwrap();
    let original = db.get(key).unwrap().unwrap();
    db.delete(key).unwrap();
    drop(db);
    let ((rejected, verified_loads), compilations) = read_counted_finality(4);
    assert!(rejected.is_err());
    assert_eq!(verified_loads, 1);
    assert_eq!(compilations, 1);
    assert!(workspace::load_finalized_genesis_parent_v1(chain, candidate, pin, params).is_err());
    assert!(workspace::retire_old_workspaces_v1(chain, candidate, pin, params).is_err());
    let db = rocksdb::DB::open_default(&ledger).unwrap();
    assert!(db.get(key).unwrap().is_none());
    db.put(key, original).unwrap(); // Explicit test fixture restoration, never recovery repair.
    drop(db);
    let ((restored, verified_loads), compilations) = read_counted_finality(2);
    assert_eq!(restored.unwrap().as_ref(), Some(&history[1]));
    assert_eq!(verified_loads, 1);
    assert_eq!(compilations, 1);
}

fn exercise_received_successor_body(
    path: &Path,
    params: &serde_json::Value,
    authority: &crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1,
    proposal: &crate::native_block_seal::NovNativeSealProposalV1,
    raws: &[Vec<u8>],
    signer: &ed25519_dalek::SigningKey,
    expected_id: [u8; 32],
) {
    use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
    use crate::native_block_seal::round_wire::encode_nov_native_seal_round_wire_v1 as encode;
    use crate::native_candidate_body::CandidateBodyAssemblerV1 as Body;
    let source = authority.transport_peer_id(proposal.proposer_id).unwrap();
    let encode_proposal = |p: crate::native_block_seal::NovNativeSealProposalV1| {
        encode(
            &Message::Proposal {
                proposal: Box::new(p),
                certificate: None,
            },
            authority,
            4,
            source,
        )
        .unwrap()
    };
    let wire = encode_proposal(proposal.clone());
    let fresh = || Body::new(&wire, authority, 4, source).unwrap();
    assert!(Body::new(&wire, authority, 5, source).is_err());
    assert!(Body::new(&wire, authority, 4, "unbound-source").is_err());
    let packets = fresh().encode_chunks(raws).unwrap();
    let mut receiver = fresh();
    assert!(receiver.push("unbound-source", &packets[0]).is_err());
    let mut damaged = packets[0].clone();
    damaged[8] ^= 1;
    assert!(receiver.push(source, &damaged).is_err());
    damaged = packets[0].clone();
    *damaged.last_mut().unwrap() ^= 1;
    assert!(fresh().push(source, &damaged).is_err());
    let received = receiver.push(source, &packets[0]).unwrap().unwrap();
    assert!(receiver.push(source, &packets[0]).is_err());
    let load = || {
        crate::native_block_seal::service_config::NovNativeSealServiceConfigV1::load(
            &path.with_extension("fresh-service-3").join("service.json"),
            authority.chain_id,
        )
        .unwrap()
    };
    assert_eq!(
        load()
            .prepare_received_successor(received, params)
            .unwrap()
            .isolated_workspace_id,
        Some(expected_id)
    );
    // Even a correctly signed false output claim cannot grant local authority.
    let mut false_subject = proposal.subject.clone();
    false_subject.post_state_root[0] ^= 1;
    let false_proposal = crate::native_block_seal::sign_modified_subject_for_body_test_v1(
        false_subject,
        &authority.validator_set,
        signer,
    )
    .unwrap();
    let wire = encode_proposal(false_proposal);
    let mut receiver = Body::new(&wire, authority, 4, source).unwrap();
    let packet = receiver.encode_chunks(raws).unwrap().remove(0);
    let received = receiver.push(source, &packet).unwrap().unwrap();
    assert!(load().prepare_received_successor(received, params).is_err());
    // Large signed input claim tests the transport only: repeated transactions
    // are deliberately not admitted to execution by this fixture.
    let many = vec![raws[0].clone(); 1024];
    let hashes = many
        .iter()
        .map(|raw| canonical_nov_native_tx_hash_from_payload_v1(raw).unwrap())
        .collect::<Vec<_>>();
    let mut subject = proposal.subject.clone();
    subject.tx_count = 1024;
    subject.receipt_count = 1024;
    subject.body_bytes = many.iter().map(|raw| raw.len() as u64).sum();
    subject.body_digest = crate::native_block_ledger::body_digest_v1(&hashes, &many);
    subject.ordered_tx_root =
        crate::native_block_ledger::nov_native_ordered_tx_root_v1(&hashes).unwrap();
    let wire = encode_proposal(
        crate::native_block_seal::sign_modified_subject_for_body_test_v1(
            subject,
            &authority.validator_set,
            signer,
        )
        .unwrap(),
    );
    let mut receiver = Body::new(&wire, authority, 4, source).unwrap();
    let packets = receiver.encode_chunks(&many).unwrap();
    assert!(packets.len() > 1);
    let last = packets.last().unwrap();
    assert!(receiver.push(source, last).unwrap().is_none());
    assert!(receiver.push(source, last).unwrap().is_none());
    let mut conflicting = last.clone();
    *conflicting.last_mut().unwrap() ^= 1;
    let mut conflict = Body::new(&wire, authority, 4, source).unwrap();
    conflict.push(source, last).unwrap();
    assert!(conflict.push(source, &conflicting).is_err());
    assert!(conflict.push(source, &packets[0]).is_err());
    let mut completed = None;
    for packet in packets[..packets.len() - 1].iter().rev() {
        completed = receiver.push(source, packet).unwrap().or(completed);
    }
    assert_eq!(completed.unwrap().raw_txs, many);
}
