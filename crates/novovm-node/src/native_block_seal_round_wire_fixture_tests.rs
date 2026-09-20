// Reuse the controlled driver's independent stores to exercise every message
// variant with real signatures and nonzero-round nested certificates.
#[test]
fn native_seal_round_wire_roundtrips_all_six_authenticated_message_variants() {
    use crate::native_block_seal::round_wire::{
        decode_nov_native_seal_round_wire_v1, encode_nov_native_seal_round_wire_v1,
        round_wire_object_hash_v1, NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1,
    };
    let mut cluster = DriverCluster::new(960_701);
    let active = cluster.without_initial_leader();
    let now = cluster.started + DRIVER_INTERVAL;
    let mut seen = std::collections::BTreeSet::new();
    let mut oversized_certificate_checked = false;
    let mut malformed_frames_checked = false;
    for _ in 0..12 {
        let messages = cluster.poll(&active, now);
        for (sender, message) in &messages {
            let source = cluster.source(*sender);
            let wire =
                encode_nov_native_seal_round_wire_v1(message, &cluster.authority, 1, &source)
                    .unwrap();
            assert!(wire.len() <= NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1);
            assert_eq!(
                decode_nov_native_seal_round_wire_v1(&wire, &cluster.authority, 1, &source,)
                    .unwrap(),
                *message
            );
            assert_eq!(
                wire,
                encode_nov_native_seal_round_wire_v1(message, &cluster.authority, 1, &source,)
                    .unwrap()
            );
            assert_ne!(round_wire_object_hash_v1(&wire), [0; 32]);
            assert!(
                decode_nov_native_seal_round_wire_v1(&wire, &cluster.authority, 2, &source,)
                    .is_err()
            );
            assert!(decode_nov_native_seal_round_wire_v1(
                &wire,
                &cluster.authority,
                1,
                "untrusted-peer",
            )
            .is_err());
            assert!(
                crate::native_block_seal_overlay::decode_nov_native_seal_overlay_wire_v1(
                    &wire,
                    &cluster.authority,
                )
                .is_err(),
                "old round-zero codec must not admit new protocol"
            );
            if !malformed_frames_checked {
                assert_malformed_round_frames_rejected(&wire, &cluster.authority, &source);
                malformed_frames_checked = true;
            }
            if !oversized_certificate_checked
                && matches!(
                    message,
                    RoundMessage::Proposal {
                        certificate: Some(_),
                        ..
                    }
                )
            {
                let mut oversized = message.clone();
                if let RoundMessage::Proposal {
                    certificate: Some(certificate),
                    ..
                } = &mut oversized
                {
                    certificate.observations[0].signature = vec![0; crate::native_block_seal::round_message::NOV_NATIVE_SEAL_ROUND_MAX_NVC_BYTES_V1];
                }
                assert!(postcard::to_allocvec(&oversized).unwrap().len() <= crate::native_block_seal::round_wire::NOV_NATIVE_SEAL_ROUND_MAX_PAYLOAD_BYTES_V1);
                let error = oversized
                    .validate_authenticated(&cluster.authority, 1, &source)
                    .unwrap_err();
                assert!(
                    error
                        .to_string()
                        .contains("certificate exceeds its admission budget"),
                    "budget must reject before the deliberately invalid signature: {error:#}"
                );
                let recipient = (*sender + 1) % 4;
                let peer = &mut cluster.peers[recipient];
                let before = peer.driver.status();
                assert!(peer
                    .driver
                    .ingest_authenticated(peer.node.ledger(), peer.node.store(), &source, oversized)
                    .is_err());
                assert_eq!(
                    peer.driver.status(),
                    before,
                    "remote oversized evidence must not halt or advance the owner"
                );
                oversized_certificate_checked = true;
            }
            let kind = match message {
                RoundMessage::Timeout(_) => 0,
                RoundMessage::TimeoutCertificate(_) => 1,
                RoundMessage::NewView { .. } => 2,
                RoundMessage::Proposal { .. } => 3,
                RoundMessage::Vote { .. } => 4,
                RoundMessage::QuorumCertificate { .. } => 5,
            };
            seen.insert(kind);
            if matches!(kind, 0 | 2 | 3 | 4) {
                let other = cluster.source((*sender + 1) % 4);
                assert!(
                    decode_nov_native_seal_round_wire_v1(&wire, &cluster.authority, 1, &other,)
                        .is_err(),
                    "direct signatures must bind the authenticated sender"
                );
            }
        }
        cluster.deliver(&active, &messages, false);
        if seen.len() == 6 {
            break;
        }
    }
    assert_eq!(seen.len(), 6);
    assert!(oversized_certificate_checked);
    assert!(malformed_frames_checked);
    cluster.assert_prepared(&active, 1);
    cluster.assert_unfinalized();
}

fn assert_malformed_round_frames_rejected(
    wire: &[u8],
    authority: &NovNativeSealEpochAuthorityV1,
    source: &str,
) {
    use crate::native_block_seal::round_wire::{
        decode_nov_native_seal_round_wire_v1, NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1,
    };
    use sha2::{Digest, Sha256};
    let reseal = |frame: &mut [u8]| {
        let end = frame.len() - 32;
        let mut hash = Sha256::new();
        hash.update(b"novovm-native-seal-round-wire-checksum-v1\0");
        hash.update(&frame[..end]);
        frame[end..].copy_from_slice(&hash.finalize());
    };
    // Recomputed checksums cannot authorize altered header-domain fields or
    // an otherwise valid kind/round that does not match the embedded object.
    for index in [0, 8, 10, 11, 12, 20, 28, 60, 68] {
        let mut malformed = wire.to_vec();
        malformed[index] ^= 1;
        reseal(&mut malformed);
        assert!(
            decode_nov_native_seal_round_wire_v1(&malformed, authority, 1, source).is_err(),
            "header byte {index} was accepted"
        );
    }
    for declared_len in [0u32, u32::MAX] {
        let mut malformed = wire.to_vec();
        malformed[76..80].copy_from_slice(&declared_len.to_be_bytes());
        reseal(&mut malformed);
        assert!(decode_nov_native_seal_round_wire_v1(&malformed, authority, 1, source).is_err());
    }
    let mut corrupt_checksum = wire.to_vec();
    *corrupt_checksum.last_mut().unwrap() ^= 1;
    assert!(decode_nov_native_seal_round_wire_v1(&corrupt_checksum, authority, 1, source).is_err());
    let mut trailing_payload = wire[..wire.len() - 32].to_vec();
    trailing_payload.push(0);
    let payload_len = (trailing_payload.len() - 80) as u32;
    trailing_payload[76..80].copy_from_slice(&payload_len.to_be_bytes());
    trailing_payload.extend_from_slice(&[0; 32]);
    reseal(&mut trailing_payload);
    assert!(decode_nov_native_seal_round_wire_v1(&trailing_payload, authority, 1, source).is_err());
    let mut nonminimal_tag = wire.to_vec();
    nonminimal_tag[80] |= 0x80;
    nonminimal_tag.insert(81, 0);
    let payload_len = (nonminimal_tag.len() - 112) as u32;
    nonminimal_tag[76..80].copy_from_slice(&payload_len.to_be_bytes());
    reseal(&mut nonminimal_tag);
    assert!(decode_nov_native_seal_round_wire_v1(&nonminimal_tag, authority, 1, source).is_err());
    assert!(decode_nov_native_seal_round_wire_v1(
        &vec![0; NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1 + 1],
        authority,
        1,
        source
    )
    .is_err());
}
