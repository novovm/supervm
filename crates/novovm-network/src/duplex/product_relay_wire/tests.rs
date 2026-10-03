use super::*;
use crate::duplex::product_overlay::{NodeHandshakeOfferV1, NodeHandshakeResponseV1};
use crate::duplex::product_relay::{
    RelayForwardDispositionV1, RelayForwardOutcomeV1, RelayPeerHandshakeDeliveryV1,
    RelayPeerHandshakeV1,
};

fn envelope() -> SecureNovoRudpEnvelopeV1 {
    SecureNovoRudpEnvelopeV1 {
        version: 0x1234,
        session_id: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        sender_peer_id: "A".into(),
        recipient_peer_id: "β".into(),
        sequence: 0x0102030405060708,
        nonce: [16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27],
        ciphertext: vec![0, 255, 127],
    }
}

fn delivery(envelope: SecureNovoRudpEnvelopeV1) -> OpaqueRelayDeliveryV1 {
    OpaqueRelayDeliveryV1 {
        source_peer_id: "S".into(),
        target_peer_id: "T".into(),
        received_at_ms: 0xf0e0d0c0b0a09080,
        envelope,
    }
}

fn hex(bytes: &str) -> Vec<u8> {
    bytes
        .split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

fn golden_data() -> Vec<u8> {
    hex("4e 56 52 4c 59 30 30 32 01
        12 34 00 01 02 03 04 05 06 07 08 09 0a 0b 0c 0d 0e 0f
        00 00 00 01 41 00 00 00 02 ce b2
        01 02 03 04 05 06 07 08 10 11 12 13 14 15 16 17 18 19 1a 1b
        00 00 00 03 00 ff 7f")
}

fn golden_delivery() -> Vec<u8> {
    hex("4e 56 52 4c 59 30 30 32 02
        00 00 00 01 53 00 00 00 01 54 f0 e0 d0 c0 b0 a0 90 80
        12 34 00 01 02 03 04 05 06 07 08 09 0a 0b 0c 0d 0e 0f
        00 00 00 01 41 00 00 00 02 ce b2
        01 02 03 04 05 06 07 08 10 11 12 13 14 15 16 17 18 19 1a 1b
        00 00 00 03 00 ff 7f")
}

fn assert_round_trip(message: &ProductRelayWireMessageV1) -> Vec<u8> {
    let wire = encode_message_v2(message).unwrap();
    let decoded = decode_message_v2(&wire).unwrap();
    match (message, &decoded) {
        (ProductRelayWireMessageV1::Data(expected), ProductRelayWireMessageV1::Data(actual)) => {
            assert_eq!(actual, expected);
        }
        (
            ProductRelayWireMessageV1::Delivery(expected),
            ProductRelayWireMessageV1::Delivery(actual),
        ) => assert_eq!(actual, expected),
        _ => assert_eq!(
            serde_json::to_value(message).unwrap(),
            serde_json::to_value(decoded).unwrap()
        ),
    }
    wire
}

#[test]
fn binary_v2_fixed_golden_bytes_pin_field_order_endianness_and_ciphertext() {
    assert_eq!(
        PRODUCT_RELAY_WEBSOCKET_SUBPROTOCOL_V2,
        "novovm.relay.binary.v2"
    );
    let data = ProductRelayWireMessageV1::Data(envelope());
    assert_eq!(assert_round_trip(&data), golden_data());
    let delivered = ProductRelayWireMessageV1::Delivery(delivery(envelope()));
    assert_eq!(assert_round_trip(&delivered), golden_delivery());
    assert_eq!(golden_data().len(), 65);
    assert_eq!(golden_delivery().len(), 83);
}

#[test]
fn every_field_round_trips_without_adding_version_route_or_identifier_policy() {
    for maximum in [false, true] {
        let envelope = SecureNovoRudpEnvelopeV1 {
            version: if maximum { u16::MAX } else { 0 },
            session_id: [if maximum { 255 } else { 0 }; 16],
            sender_peer_id: "\0\"\\\n🧬".into(),
            recipient_peer_id: String::new(),
            sequence: if maximum { u64::MAX } else { 0 },
            nonce: [if maximum { 255 } else { 0 }; 12],
            ciphertext: if maximum {
                (0..=255).collect()
            } else {
                Vec::new()
            },
        };
        assert_round_trip(&ProductRelayWireMessageV1::Data(envelope.clone()));
        assert_round_trip(&ProductRelayWireMessageV1::Delivery(
            OpaqueRelayDeliveryV1 {
                source_peer_id: String::new(),
                target_peer_id: "different from envelope recipient\0\t世界".into(),
                received_at_ms: if maximum { u64::MAX } else { 0 },
                envelope,
            },
        ));
    }
}

#[test]
fn every_binary_truncation_and_wrong_magic_tag_or_suffix_is_rejected() {
    for wire in [golden_data(), golden_delivery()] {
        for length in 0..wire.len() {
            assert!(decode_message_v2(&wire[..length]).is_err(), "cut {length}");
        }
        for position in 0..MAGIC.len() {
            let mut malformed = wire.clone();
            malformed[position] ^= 1;
            assert!(decode_message_v2(&malformed).is_err());
        }
        for tag in [0, 3, 255] {
            let mut malformed = wire.clone();
            malformed[MAGIC.len()] = tag;
            assert!(decode_message_v2(&malformed).is_err());
        }
        let mut trailing = wire.clone();
        trailing.push(0);
        assert!(decode_message_v2(&trailing).is_err());
    }
}

#[test]
fn every_variable_field_rejects_bad_lengths_and_each_identifier_rejects_bad_utf8() {
    for (wire, lengths, strings) in [
        (golden_data(), vec![27, 32, 58], vec![31, 36]),
        (
            golden_delivery(),
            vec![9, 14, 45, 50, 76],
            vec![13, 18, 49, 54],
        ),
    ] {
        for position in lengths {
            for invalid in [u32::MAX, PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 as u32 + 1] {
                let mut malformed = wire.clone();
                malformed[position..position + 4].copy_from_slice(&invalid.to_be_bytes());
                assert!(decode_message_v2(&malformed).is_err());
            }
        }
        for position in strings {
            let mut malformed = wire.clone();
            malformed[position] = 255;
            assert!(decode_message_v2(&malformed).is_err());
        }
    }
}

#[test]
fn binary_preflight_borrows_until_all_lengths_utf8_and_tail_are_valid() {
    let wire = golden_delivery();
    let view = BinaryMessage::preflight(&wire).unwrap();
    assert_eq!(view.envelope.ciphertext.as_ptr(), wire[80..].as_ptr());
    assert_eq!(view.envelope.sender_peer_id.as_ptr(), wire[49..].as_ptr());
    assert_eq!(
        view.delivery.as_ref().unwrap().source_peer_id.as_ptr(),
        wire[13..].as_ptr()
    );
    let mut tail = wire.clone();
    tail.push(0);
    assert!(BinaryMessage::preflight(&tail).is_err());
    // Checked additions reject overflow without needing a huge allocation.
    assert!(WireLength(usize::MAX).fixed(1).is_err());
}

#[test]
fn exact_wire_limit_is_inclusive_and_ciphertext_is_not_json_expanded() {
    for delivered in [false, true] {
        let mut body = envelope();
        body.ciphertext.clear();
        let wrap = |body| {
            if delivered {
                ProductRelayWireMessageV1::Delivery(delivery(body))
            } else {
                ProductRelayWireMessageV1::Data(body)
            }
        };
        let fixed = encode_message_v2(&wrap(body.clone())).unwrap().len();
        body.ciphertext = vec![255; 196_720];
        let wire = assert_round_trip(&wrap(body.clone()));
        assert_eq!(wire.len(), 196_720 + fixed);
        assert!(fixed < 100);
        assert_eq!(&wire[wire.len() - 196_720..], &body.ciphertext);

        body.ciphertext
            .resize(PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 - fixed, 0);
        let mut wire = assert_round_trip(&wrap(body.clone()));
        assert_eq!(wire.len(), PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1);
        body.ciphertext.push(0);
        assert!(encode_message_v2(&wrap(body)).is_err());
        wire.push(0);
        assert!(decode_message_v2(&wire).is_err());
    }
}

fn offer() -> NodeHandshakeOfferV1 {
    NodeHandshakeOfferV1 {
        version: 17,
        session_id: [1; 16],
        initiator_peer_id: "initiator\0世界".into(),
        responder_peer_id: "responder".into(),
        initiator_identity_public_key: [2; 32],
        initiator_ephemeral_public_key: vec![3, 4],
        challenge: [5; 32],
        issued_at_ms: 6,
        expires_at_ms: u64::MAX,
        signature: vec![7, 8],
    }
}

fn response() -> NodeHandshakeResponseV1 {
    NodeHandshakeResponseV1 {
        version: 19,
        session_id: [9; 16],
        initiator_peer_id: "initiator".into(),
        responder_peer_id: "responder\nβ".into(),
        responder_identity_public_key: [10; 32],
        responder_ephemeral_public_key: vec![11, 12],
        challenge: [13; 32],
        response_nonce: [14; 32],
        offer_hash: [15; 32],
        issued_at_ms: 16,
        expires_at_ms: u64::MAX,
        signature: vec![17, 18],
    }
}

#[test]
fn every_control_variant_keeps_its_existing_json_bytes_and_fields() {
    let messages = vec![
        ProductRelayWireMessageV1::HandshakeOffer(offer()),
        ProductRelayWireMessageV1::HandshakeResponse(response()),
        ProductRelayWireMessageV1::DeliveryWindowV1 { max_unconsumed: 15 },
        ProductRelayWireMessageV1::DeliveryConsumedV1 { through: u64::MAX },
        ProductRelayWireMessageV1::PeerHandshake {
            target_peer_id: "target".into(),
            handshake: RelayPeerHandshakeV1::Offer(offer()),
        },
        ProductRelayWireMessageV1::PeerHandshakeDelivery(RelayPeerHandshakeDeliveryV1 {
            source_peer_id: "source".into(),
            target_peer_id: "target".into(),
            received_at_ms: u64::MAX,
            handshake: RelayPeerHandshakeV1::Response(response()),
        }),
        ProductRelayWireMessageV1::Heartbeat,
        ProductRelayWireMessageV1::HeartbeatAck,
        ProductRelayWireMessageV1::ForwardOutcome(RelayForwardOutcomeV1 {
            disposition: RelayForwardDispositionV1::QueuedTargetOffline,
            source_peer_id: "source".into(),
            target_peer_id: "target".into(),
            forwarded: false,
            queued: true,
            payload_treated_opaque: true,
            envelope_session_id: Some([255; 16]),
            envelope_sequence: Some(u64::MAX),
            admitted_wire_bytes: usize::MAX,
        }),
        ProductRelayWireMessageV1::Close,
    ];
    for message in messages {
        assert_eq!(
            assert_round_trip(&message),
            serde_json::to_vec(&message).unwrap()
        );
    }
    // JSON whitespace, escaped discriminants and body-before-kind remain
    // accepted where the existing enum accepts them.
    for wire in [
        br#" {"kind":"heart\u0062eat"} "#.as_slice(),
        br#"{"body":{"through":18446744073709551615},"kind":"delivery_consumed_v1"}"#,
    ] {
        assert_eq!(
            serde_json::to_value(decode_message_v2(wire).unwrap()).unwrap(),
            serde_json::to_value(
                serde_json::from_slice::<ProductRelayWireMessageV1>(wire).unwrap()
            )
            .unwrap()
        );
    }
}

#[test]
fn old_json_data_and_delivery_are_rejected_including_escaped_and_reordered_kind() {
    for message in [
        ProductRelayWireMessageV1::Data(envelope()),
        ProductRelayWireMessageV1::Delivery(delivery(envelope())),
    ] {
        let old = serde_json::to_vec(&message).unwrap();
        assert!(serde_json::from_slice::<ProductRelayWireMessageV1>(&old).is_ok());
        assert!(decode_message_v2(&old).is_err());
        let mut value = serde_json::to_value(&message).unwrap();
        let object = value.as_object_mut().unwrap();
        let kind = object.remove("kind").unwrap();
        let body = object.remove("body").unwrap();
        let reordered = format!("{{\"body\":{body},\"kind\":{kind}}}");
        assert!(decode_message_v2(reordered.as_bytes()).is_err());
    }
    assert!(decode_message_v2(br#"{"kind":"d\u0061ta","body":{}}"#).is_err());
    assert!(decode_message_v2(br#"{"kind":"deliv\u0065ry","body":{}}"#).is_err());
}

#[test]
fn control_json_serialization_and_deserialization_keep_the_existing_wire_bound() {
    let wrap = |target_peer_id: String| ProductRelayWireMessageV1::PeerHandshake {
        target_peer_id,
        handshake: RelayPeerHandshakeV1::Offer(offer()),
    };
    let overhead = encode_message_v2(&wrap(String::new())).unwrap().len();
    let length = PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 - overhead;
    let mut wire = assert_round_trip(&wrap("x".repeat(length)));
    assert_eq!(wire.len(), PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1);
    assert!(encode_message_v2(&wrap("x".repeat(length + 1))).is_err());
    // Escaping must count actual JSON bytes, not the UTF-8 source string size.
    assert!(encode_message_v2(&wrap("\0".repeat(length / 5))).is_err());
    wire.push(b' ');
    assert!(decode_message_v2(&wire).is_err());

    let mut output = BoundedJson(Vec::new());
    let chunk = [7; 4096];
    for _ in 0..PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 / chunk.len() {
        output.write_all(&chunk).unwrap();
        assert!(output.0.capacity() <= PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1);
    }
    assert_eq!(output.0.len(), PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1);
    assert!(output.write_all(&[1]).is_err());
    assert_eq!(output.0.len(), PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1);
    assert!(output.0.iter().all(|byte| *byte == 7));
    assert_eq!(output.write(&[]).unwrap(), 0);
}
