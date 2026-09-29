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
    let received = exercise_body_network(authority, wire.clone(), raws, source, signer);
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

fn exercise_body_network(
    authority: &crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1,
    wire: Vec<u8>,
    raws: &[Vec<u8>],
    source: &str,
    signer: &ed25519_dalek::SigningKey,
) -> crate::native_candidate_body::VerifiedCandidateBodyV1 {
    use crate::native_block_seal::round_wire::{
        decode_nov_native_seal_round_wire_v1, encode_nov_native_seal_round_wire_v1,
        is_nov_native_seal_round_wire_v1, round_wire_object_hash_v1,
    };
    use crate::native_candidate_body::network::{
        CandidateBodyInboxV1 as Inbox, CandidateBodySenderV1 as Sender,
    };
    use crate::product_mainline_overlay::ProductMainlineOverlayEventV1 as Event;
    use std::time::{Duration, Instant};
    crate::native_block_seal::tests::native_seal_round_network::with_service_test_transports(
        authority.chain_id,
        |peers| {
            let (runtime, _) = peers
                .iter()
                .find(|(r, _)| r.startup().local_peer_id == source)
                .unwrap();
            let (target, _) = peers
                .iter()
                .find(|(r, _)| r.startup().local_peer_id != source)
                .unwrap();
            let mut sender = Sender::new(
                wire.clone(),
                raws,
                authority,
                4,
                source,
                &target.startup().local_peer_id,
            )
            .unwrap();
            assert!(sender.poll(target).is_err());
            let start = Instant::now();
            let mut inbox = Inbox::new(authority.clone(), 4, start).unwrap();
            let mut manifest = None;
            let received = loop {
                sender.poll(runtime).unwrap();
                runtime.drain_events(128);
                let mut complete = None;
                for event in target.drain_events(128) {
                    if let Event::Inbound(inbound) = event {
                        if is_nov_native_seal_round_wire_v1(&inbound.frame.payload) {
                            manifest = Some(inbound.clone());
                        }
                        if let Some(body) = inbox.accept(&inbound, Instant::now()).unwrap() {
                            complete = Some(body);
                        }
                    }
                }
                if let Some(body) = complete {
                    break body;
                }
                assert!(
                    start.elapsed() < Duration::from_secs(30),
                    "body transfer WSS deadline"
                );
                std::thread::sleep(Duration::from_millis(20));
            };
            assert_eq!(received.raw_txs, raws);
            let manifest = manifest.unwrap();
            let now = Instant::now();
            let mut bounded = Inbox::new(authority.clone(), 4, now).unwrap();
            bounded.accept(&manifest, now).unwrap();
            bounded
                .accept(&manifest, now + Duration::from_secs(29))
                .unwrap();
            assert_eq!(bounded.expire(now + Duration::from_secs(30)).unwrap(), 1);
            assert!(bounded.expire(now).is_err());
            let mut limited = Inbox::new(authority.clone(), 4, now).unwrap();
            for _ in 0..64 {
                limited.accept(&manifest, now).unwrap();
            }
            assert!(limited.accept(&manifest, now).is_err());
            limited
                .accept(&manifest, now + Duration::from_secs(1))
                .unwrap();
            let message =
                decode_nov_native_seal_round_wire_v1(&wire, authority, 4, source).unwrap();
            let mut limited = Inbox::new(authority.clone(), 4, now).unwrap();
            for index in 0..5 {
                let mut subject = message.proposal().unwrap().subject.clone();
                subject.post_state_root[0] ^= index + 1;
                let proposal = crate::native_block_seal::sign_modified_subject_for_body_test_v1(
                    subject,
                    &authority.validator_set,
                    signer,
                )
                .unwrap();
                let message = crate::native_block_seal::round_message::NovNativeSealRoundMessageV1::Proposal { proposal: Box::new(proposal), certificate: None };
                let mut event = manifest.clone();
                event.frame.payload =
                    encode_nov_native_seal_round_wire_v1(&message, authority, 4, source).unwrap();
                event.object_hash = round_wire_object_hash_v1(&event.frame.payload);
                let result = limited.accept(&event, now);
                assert_eq!(result.is_ok(), index < 4);
            }
            assert_eq!(limited.expire(now + Duration::from_secs(30)).unwrap(), 4);
            received
        },
    )
}
