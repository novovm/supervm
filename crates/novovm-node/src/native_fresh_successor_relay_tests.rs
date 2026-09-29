#[allow(clippy::too_many_arguments)]
fn exercise_fresh_successor_relay(
    path: &Path,
    params: &serde_json::Value,
    chain: u64,
    parent: [u8; 32],
    id: [u8; 32],
    pin: [u8; 32],
    proof: &crate::native_block_ledger::NovNativeFreshFinalityProofV1,
) {
    use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
    use crate::native_block_seal::round_wire::decode_nov_native_seal_round_wire_v1 as decode;
    use crate::native_block_seal::tests::native_seal_round_network::with_service_test_transports;
    use crate::native_block_seal::{
        service::FreshGenesisPublicationDriverV1 as Driver,
        service_config::NovNativeSealServiceConfigV1 as Config, NovNativeBlockSealStoreV1 as Seal,
    };
    use crate::product_mainline_overlay::{
        ProductMainlineOverlayEventV1 as Event, ProductMainlineOverlayPayloadClassV1 as Class,
    };
    use std::time::{Duration, Instant};
    let ledger = nov_native_block_ledger_rocksdb_path_v1(path);
    let Message::DecisionCertificateV3 { decision, .. } = &proof.witness else {
        panic!("full witness required");
    };
    let height = decision.prepare.subject.height;
    with_service_test_transports(chain, |peers| {
        let validator = &proof.authority.validator_set.validators[0];
        let (sender, (runtime, key)) = peers
            .iter()
            .enumerate()
            .find(|(_, (_, key))| key.verifying_key().to_bytes() == validator.public_key)
            .unwrap();
        let root = path.with_extension("successor-relay");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("authority.json"),
            serde_json::to_vec(&proof.authority).unwrap(),
        )
        .unwrap();
        fs::write(root.join("signer.hex"), to_hex(&key.to_bytes())).unwrap();
        let seal_path = path.with_extension("genesis-seal-0");
        let config_path = root.join("service.json");
        let config = serde_json::json!({
            "schema":"novovm-native-seal-service/v1", "enabled":true, "decision_v3_enabled":true,
            "fresh_genesis_config_commitment":to_hex(&pin), "finalized_parent_workspace_id":to_hex(&parent),
            "isolated_workspace_id":to_hex(&id), "chain_id":chain, "height":height,
            "block_hash":to_hex(&decision.prepare.subject.block_hash),
            "authority_path":"authority.json", "signer_key_path":"signer.hex", "seal_store_path":seal_path,
            "round_timeout_ms":300000, "poll_interval_ms":100, "ingress_per_source_per_second":16, "ingress_per_poll":32,
        });
        fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
        let load = || Config::load(&config_path, chain).unwrap();
        assert!(Driver::open(&load(), &root, params, runtime, Instant::now()).is_err());
        let seal = Seal::open(&seal_path).unwrap();
        let before = seal
            .load_pending_outbox(chain, validator.validator_id, 128)
            .unwrap();
        let mut relay = Driver::open(&load(), &ledger, params, runtime, Instant::now())
            .unwrap()
            .unwrap();
        assert_eq!(relay.status_json()["height"], height);
        assert_eq!(relay.status_json()["finalized"], true);
        assert_eq!(relay.status_json()["signing_enabled"], false);
        let started = Instant::now();
        let mut received = std::collections::BTreeSet::new();
        while received.len() < 6 {
            relay.poll(runtime, Instant::now()).unwrap();
            for (index, (peer, _)) in peers.iter().enumerate() {
                if index == sender {
                    continue;
                }
                for event in peer.drain_events(128) {
                    if let Event::Inbound(inbound) = event {
                        if inbound.payload_class != Class::NativeSeal {
                            continue;
                        }
                        let message = decode(
                            &inbound.frame.payload,
                            &proof.authority,
                            height,
                            &inbound.source_peer_id,
                        )
                        .unwrap();
                        match message {
                            Message::DecisionCertificateV3 { decision: got, .. } => {
                                assert_eq!(*got, **decision);
                                received.insert((index, "decision"));
                            }
                            Message::QuorumCertificate { qc, .. } => {
                                assert_eq!(*qc, decision.prepare);
                                received.insert((index, "prepare"));
                            }
                            _ => panic!("relay emitted a new vote or proposal"),
                        }
                    }
                }
            }
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "successor relay delivery deadline: {received:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        drop(relay);
        let mut relay = Driver::open(&load(), &ledger, params, runtime, Instant::now())
            .unwrap()
            .unwrap();
        relay.poll(runtime, Instant::now()).unwrap();
        assert_eq!(
            seal.load_pending_outbox(chain, validator.validator_id, 128)
                .unwrap(),
            before
        );
        let pin_key = b"native_block_ledger/v1/successor/finalized-intent";
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        let original = db.get(pin_key).unwrap().unwrap();
        db.delete(pin_key).unwrap();
        drop(db);
        assert!(relay.poll(runtime, Instant::now()).is_err());
        assert_eq!(relay.status_json()["halted"], true);
        assert_eq!(relay.status_json()["finalized"], false);
        assert!(Driver::open(&load(), &ledger, params, runtime, Instant::now()).is_err());
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        assert!(db.get(pin_key).unwrap().is_none());
        db.put(pin_key, original).unwrap(); // Explicit fixture restoration.
        drop(db);
        assert!(relay.poll(runtime, Instant::now()).is_err());
        assert!(
            Driver::open(&load(), &ledger, params, runtime, Instant::now())
                .unwrap()
                .is_some()
        );
    });
}
