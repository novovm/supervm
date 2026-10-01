mod receipt_wire_tests {
    use super::*;

    #[test]
    fn mesh_ack_round_robin_serves_later_peers_under_continuous_first_peer_backlog() {
        let mut pending = [1usize, 4, 4];
        let mut next_peer = 0;
        let mut selected = Vec::new();
        for _ in 0..12 {
            let peer = next_mesh_ack_peer_v1(3, &mut next_peer, |index| pending[index] > 0)
                .expect("pending ACK peer");
            selected.push(peer);
            pending[peer] -= 1;
            // The first peer is never empty, even immediately after a send.
            if peer == 0 {
                pending[0] += 1;
            }
        }
        assert_eq!(selected, [0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2]);
        assert_eq!(pending, [1, 0, 0]);
    }

    #[test]
    fn mesh_ack_round_robin_skips_unready_and_retains_rejected_peer_for_later_turn() {
        let pending = [1usize, 1, 1];
        let active = [true, false, true];
        let mut next_peer = 0;
        let mut selected = Vec::new();
        for _ in 0..6 {
            let peer = next_mesh_ack_peer_v1(3, &mut next_peer, |index| {
                active[index] && pending[index] > 0
            })
            .unwrap();
            selected.push(peer);
            // Model a retained item (e.g. target-local admission failure), not
            // a successful pop. It must neither be lost nor monopolize attempts.
        }
        assert_eq!(selected, [0, 2, 0, 2, 0, 2]);
        assert_eq!(pending, [1, 1, 1]);
        next_peer = 1;
        let result = mesh_send_when_inbound_drained_v1(true, || {
            Ok(next_mesh_ack_peer_v1(3, &mut next_peer, |_| true))
        })
        .unwrap();
        assert_eq!(result, None);
        assert_eq!(next_peer, 1, "buffered inbound must defer the ACK turn");
        assert_eq!(next_mesh_ack_peer_v1(3, &mut next_peer, |_| false), None);
        assert_eq!(next_peer, 1, "no ready ACK must not consume a turn");
        assert_eq!(next_mesh_ack_peer_v1(3, &mut next_peer, |_| true), Some(1));
        assert_eq!(
            next_mesh_ack_peer_v1(0, &mut next_peer, |_| panic!("no peers")),
            None
        );
    }

    fn identities() -> (SigningKey, SigningKey, String, String) {
        let sender = SigningKey::from_bytes(&[0x61; 32]);
        let recipient = SigningKey::from_bytes(&[0x62; 32]);
        let sender_id = peer_id_from_ed25519_public_key_v1(&sender.verifying_key().to_bytes());
        let recipient_id =
            peer_id_from_ed25519_public_key_v1(&recipient.verifying_key().to_bytes());
        (sender, recipient, sender_id, recipient_id)
    }

    fn request(
        class: ProductMainlineOverlayPayloadClassV1,
        disposition: ProductMainlineOverlayRecipientAckDispositionV1,
    ) -> ProductMainlineOverlayRecipientAckRequestV1 {
        let (_, _, sender, recipient) = identities();
        let object_hash = [0x63; 32];
        let payload_sha256 = [0x64; 32];
        ProductMainlineOverlayRecipientAckRequestV1 {
            payload_class: class,
            object_hash,
            payload_sha256,
            delivery_id: product_delivery_id_v1(
                7,
                class.label(),
                object_hash,
                payload_sha256,
                &sender,
                &recipient,
            ),
            original_sender_peer_id: sender,
            recipient_peer_id: recipient,
            accepted_at_ms: 1_234,
            disposition,
        }
    }

    // The pre-existing v1 byte layout written independently of the production
    // encoder. Intentionally has no validation so signed invalid combinations
    // can reach the decoder, rather than failing only in the encoder fixture.
    fn signing_bytes_unchecked(ack: &ProductMainlineOverlayRecipientAckV1) -> Vec<u8> {
        let domain = b"novovm-product-mainline-recipient-ack/v1";
        let mut bytes = b"NOVACK01".to_vec();
        bytes.extend_from_slice(&(domain.len() as u16).to_be_bytes());
        bytes.extend_from_slice(domain);
        bytes.extend_from_slice(&ack.version.to_be_bytes());
        bytes.extend_from_slice(&ack.chain_id.to_be_bytes());
        bytes.push(match ack.payload_class {
            ProductMainlineOverlayPayloadClassV1::NativeTransaction => 1,
            ProductMainlineOverlayPayloadClassV1::NativeSeal => 2,
        });
        bytes.push(match ack.disposition {
            ProductMainlineOverlayRecipientAckDispositionV1::JournalPersisted => 1,
            ProductMainlineOverlayRecipientAckDispositionV1::PendingTransactionPersisted => 2,
        });
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&ack.object_hash);
        bytes.extend_from_slice(&ack.payload_sha256);
        bytes.extend_from_slice(&ack.delivery_id);
        bytes.extend_from_slice(&ack.accepted_at_ms.to_be_bytes());
        bytes.extend_from_slice(&(ack.original_sender_peer_id.len() as u16).to_be_bytes());
        bytes.extend_from_slice(ack.original_sender_peer_id.as_bytes());
        bytes.extend_from_slice(&(ack.recipient_peer_id.len() as u16).to_be_bytes());
        bytes.extend_from_slice(ack.recipient_peer_id.as_bytes());
        bytes
    }

    fn wire_unchecked(ack: &ProductMainlineOverlayRecipientAckV1) -> Vec<u8> {
        let mut wire = signing_bytes_unchecked(ack);
        wire.extend_from_slice(&ack.signature);
        wire
    }

    #[test]
    fn legacy_code_one_keeps_its_original_bytes_signature_and_route_verification() {
        let (_, recipient, sender_id, recipient_id) = identities();
        for class in [
            ProductMainlineOverlayPayloadClassV1::NativeTransaction,
            ProductMainlineOverlayPayloadClassV1::NativeSeal,
        ] {
            let ack = build_recipient_ack_v1(
                &recipient,
                7,
                &request(
                    class,
                    ProductMainlineOverlayRecipientAckDispositionV1::JournalPersisted,
                ),
            )
            .unwrap();
            assert_eq!(ack.version, 1);
            assert_eq!(ack.disposition.code(), 1);
            let old_signature = recipient.sign(&signing_bytes_unchecked(&ack)).to_bytes();
            assert_eq!(ack.signature, old_signature);
            let old_wire = wire_unchecked(&ack);
            assert_eq!(encode_recipient_ack_v1(&ack).unwrap(), old_wire);
            assert_eq!(
                decode_and_verify_recipient_ack_v1(&old_wire, 7, &sender_id, &recipient_id)
                    .unwrap(),
                ack
            );
            ack.verify_route(7, &sender_id, &recipient_id).unwrap();
        }
    }

    #[test]
    fn pending_transaction_code_two_round_trips_but_cannot_relabel_a_legacy_signature() {
        let (_, recipient, sender_id, recipient_id) = identities();
        let ack = build_recipient_ack_v1(
            &recipient,
            7,
            &request(
                ProductMainlineOverlayPayloadClassV1::NativeTransaction,
                ProductMainlineOverlayRecipientAckDispositionV1::PendingTransactionPersisted,
            ),
        )
        .unwrap();
        assert_eq!(ack.disposition.code(), 2);
        let encoded = encode_recipient_ack_v1(&ack).unwrap();
        assert_eq!(encoded, wire_unchecked(&ack));
        assert_eq!(
            decode_and_verify_recipient_ack_v1(&encoded, 7, &sender_id, &recipient_id).unwrap(),
            ack
        );
        ack.verify_route(7, &sender_id, &recipient_id).unwrap();

        let mut old = build_recipient_ack_v1(
            &recipient,
            7,
            &request(
                ProductMainlineOverlayPayloadClassV1::NativeTransaction,
                ProductMainlineOverlayRecipientAckDispositionV1::JournalPersisted,
            ),
        )
        .unwrap();
        old.disposition =
            ProductMainlineOverlayRecipientAckDispositionV1::PendingTransactionPersisted;
        let error = old.verify_route(7, &sender_id, &recipient_id).unwrap_err();
        assert!(error.to_string().contains("signature"));
        let error =
            decode_and_verify_recipient_ack_v1(&wire_unchecked(&old), 7, &sender_id, &recipient_id)
                .unwrap_err();
        assert!(error.to_string().contains("signature"));
    }

    #[test]
    fn pending_receipt_rejects_wrong_routes_and_tampered_delivery_binding() {
        let (_, recipient, sender_id, recipient_id) = identities();
        let ack = build_recipient_ack_v1(
            &recipient,
            7,
            &request(
                ProductMainlineOverlayPayloadClassV1::NativeTransaction,
                ProductMainlineOverlayRecipientAckDispositionV1::PendingTransactionPersisted,
            ),
        )
        .unwrap();
        let other_id = peer_id_from_ed25519_public_key_v1(
            &SigningKey::from_bytes(&[0x65; 32])
                .verifying_key()
                .to_bytes(),
        );
        let wire = encode_recipient_ack_v1(&ack).unwrap();
        for (chain, sender, recipient) in [
            (8, sender_id.as_str(), recipient_id.as_str()),
            (7, other_id.as_str(), recipient_id.as_str()),
            (7, sender_id.as_str(), other_id.as_str()),
        ] {
            assert!(ack.verify_route(chain, sender, recipient).is_err());
            assert!(decode_and_verify_recipient_ack_v1(&wire, chain, sender, recipient).is_err());
        }
        let mutations: [fn(&mut ProductMainlineOverlayRecipientAckV1); 8] = [
            |bad| bad.chain_id = 8,
            |bad| bad.original_sender_peer_id = bad.recipient_peer_id.clone(),
            |bad| bad.recipient_peer_id = bad.original_sender_peer_id.clone(),
            |bad| bad.object_hash[0] ^= 1,
            |bad| bad.payload_sha256[0] ^= 1,
            |bad| bad.delivery_id[0] ^= 1,
            |bad| bad.signature[0] ^= 1,
            |bad| bad.accepted_at_ms = 0,
        ];
        for (index, mutate) in mutations.into_iter().enumerate() {
            let mut bad = ack.clone();
            mutate(&mut bad);
            assert!(
                bad.verify_route(7, &sender_id, &recipient_id).is_err(),
                "typed mutation {index}"
            );
            assert!(
                decode_and_verify_recipient_ack_v1(
                    &wire_unchecked(&bad),
                    7,
                    &sender_id,
                    &recipient_id
                )
                .is_err(),
                "wire mutation {index}"
            );
        }
        // Zero timestamp must fail even with an otherwise correct signature.
        let mut zero_time = ack;
        zero_time.accepted_at_ms = 0;
        zero_time.signature = recipient
            .sign(&signing_bytes_unchecked(&zero_time))
            .to_bytes();
        assert!(encode_recipient_ack_v1(&zero_time).is_err());
        assert!(decode_and_verify_recipient_ack_v1(
            &wire_unchecked(&zero_time),
            7,
            &sender_id,
            &recipient_id,
        )
        .is_err());
    }

    #[test]
    fn native_seal_cannot_claim_pending_transaction_persistence_even_when_signed() {
        let (_, recipient, sender_id, recipient_id) = identities();
        let invalid_request = request(
            ProductMainlineOverlayPayloadClassV1::NativeSeal,
            ProductMainlineOverlayRecipientAckDispositionV1::PendingTransactionPersisted,
        );
        assert!(build_recipient_ack_v1(&recipient, 7, &invalid_request).is_err());
        let mut invalid = build_recipient_ack_v1(
            &recipient,
            7,
            &request(
                ProductMainlineOverlayPayloadClassV1::NativeSeal,
                ProductMainlineOverlayRecipientAckDispositionV1::JournalPersisted,
            ),
        )
        .unwrap();
        invalid.disposition =
            ProductMainlineOverlayRecipientAckDispositionV1::PendingTransactionPersisted;
        invalid.signature = recipient
            .sign(&signing_bytes_unchecked(&invalid))
            .to_bytes();
        assert!(encode_recipient_ack_v1(&invalid).is_err());
        assert!(invalid.verify_route(7, &sender_id, &recipient_id).is_err());
        let error = decode_and_verify_recipient_ack_v1(
            &wire_unchecked(&invalid),
            7,
            &sender_id,
            &recipient_id,
        )
        .unwrap_err();
        assert!(
            !error.to_string().contains("signature"),
            "must reject semantics, not our valid signature"
        );
    }
}
