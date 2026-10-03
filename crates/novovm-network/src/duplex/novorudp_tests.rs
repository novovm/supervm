use super::*;

fn frame() -> NovoRudpTransportFrameV0 {
    NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        [0x11; 16],
        1,
        2,
        3,
        4,
        b"opaque".to_vec(),
    )
}

#[test]
fn transport_frame_v0_roundtrips_all_transport_kinds_without_business_envelope() {
    for (index, kind) in [
        NovoRudpTransportFrameKindV0::Data,
        NovoRudpTransportFrameKindV0::Repair,
        NovoRudpTransportFrameKindV0::Ack,
        NovoRudpTransportFrameKindV0::Endpoint,
        NovoRudpTransportFrameKindV0::Done,
    ]
    .into_iter()
    .enumerate()
    {
        let original = NovoRudpTransportFrameV0::new(
            kind,
            [index as u8; 16],
            7,
            100 + index as u64,
            200 + index as u64,
            300 + index as u64,
            format!("opaque-payload-{index}").into_bytes(),
        );
        let encoded = original.try_encode().unwrap();
        assert_eq!(&encoded[..8], NOVORUDP_TRANSPORT_FRAME_V0_MAGIC);
        assert_eq!(encoded, original.encode());
        assert_eq!(
            NovoRudpTransportFrameV0::decode(&encoded).unwrap(),
            original
        );
    }
}

#[test]
fn transport_frame_v0_rejects_payload_and_authenticated_header_tamper() {
    let original = frame().encode();
    for offset in [12, 28, 36, 44, 52, 64, HEADER_LEN_V0] {
        let mut encoded = original.clone();
        encoded[offset] ^= 1;
        assert_eq!(
            NovoRudpTransportFrameV0::decode(&encoded),
            Err(NovoRudpTransportFrameDecodeErrorV0::ChecksumMismatch)
        );
    }
}

#[test]
fn transport_frame_v0_rejects_noncanonical_reserved_byte() {
    let mut encoded = frame().encode();
    encoded[11] = 1;
    // The legacy checksum omits this byte. Explicit validation must reject it.
    assert_eq!(
        NovoRudpTransportFrameV0::decode(&encoded),
        Err(NovoRudpTransportFrameDecodeErrorV0::NonzeroReserved { value: 1 })
    );
}

#[test]
fn transport_frame_v0_rejects_bad_tags_truncation_and_trailing_bytes() {
    let original = frame().encode();
    for len in 0..HEADER_LEN_V0 {
        assert!(matches!(
            NovoRudpTransportFrameV0::decode(&original[..len]),
            Err(NovoRudpTransportFrameDecodeErrorV0::TooShort { .. })
        ));
    }
    for (offset, error) in [
        (0, NovoRudpTransportFrameDecodeErrorV0::BadMagic),
        (
            8,
            NovoRudpTransportFrameDecodeErrorV0::UnsupportedVersion { version: 0 },
        ),
        (
            10,
            NovoRudpTransportFrameDecodeErrorV0::UnknownKind { kind: 0 },
        ),
    ] {
        let mut encoded = original.clone();
        encoded[offset] = 0;
        assert_eq!(NovoRudpTransportFrameV0::decode(&encoded), Err(error));
    }
    for encoded in [
        original[..original.len() - 1].to_vec(),
        [original.clone(), vec![0]].concat(),
    ] {
        assert!(matches!(
            NovoRudpTransportFrameV0::decode(&encoded),
            Err(NovoRudpTransportFrameDecodeErrorV0::LengthMismatch { .. })
        ));
    }
    let mut declared_huge = original;
    declared_huge[60..64].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(NovoRudpTransportFrameV0::decode(&declared_huge).is_err());
}

#[test]
fn transport_frame_v0_checked_length_never_clamps_or_wraps() {
    assert_eq!(checked_frame_lengths_v0(0), Ok((0, HEADER_LEN_V0)));
    assert!(checked_frame_lengths_v0(usize::MAX).is_err());
    #[cfg(target_pointer_width = "64")]
    {
        assert_eq!(
            checked_frame_lengths_v0(u32::MAX as usize),
            Ok((u32::MAX, HEADER_LEN_V0 + u32::MAX as usize))
        );
        assert!(checked_frame_lengths_v0(u32::MAX as usize + 1).is_err());
    }
}

#[test]
fn transport_frame_v0_empty_payload_roundtrips_and_encoding_refreshes_checksum() {
    let mut original = frame();
    original.payload.clear();
    original.checksum = [0xff; 32];
    let encoded = original.try_encode().unwrap();
    assert_eq!(encoded.len(), HEADER_LEN_V0);
    let decoded = NovoRudpTransportFrameV0::decode(&encoded).unwrap();
    assert!(decoded.payload.is_empty());
    assert_ne!(decoded.checksum, original.checksum);
}
