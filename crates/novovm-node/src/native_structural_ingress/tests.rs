use super::*;
use crate::native_pipeline::ingress::authentication::authenticate_transfer_v3;
use crate::native_pipeline::ingress::wire::{
    encode_transfer_v3, signing_message, FeePolicy, TransferV3,
};
use ed25519_dalek::{Signer, SigningKey};

fn limits() -> StructuralReceiveLimits {
    StructuralReceiveLimits {
        max_total_frames: 16,
        max_buffered_frames: 4,
        max_buffered_bytes: 32,
        max_payload_bytes: 8,
        bulk_size: 2,
    }
}

fn frame(sequence: u64, payload: &[u8]) -> NovoRudpTransportFrameV0 {
    NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        [7; 16],
        1,
        sequence,
        sequence,
        0,
        payload.to_vec(),
    )
}

#[test]
fn ordered_bulk_preserves_original_allocations_and_repair_identity() {
    let mut rx = NativeStructuralReceiver::new([7; 16], 1, 3, limits()).unwrap();
    let input = frame(1, b"second");
    let allocation = input.payload.as_ptr();
    assert_eq!(rx.accept(input).unwrap(), ReceiveDisposition::Accepted);
    assert!(rx.take_ready_bulk().is_none());
    let mut repair = frame(1, b"second");
    repair.kind = NovoRudpTransportFrameKindV0::Repair;
    repair.ack_epoch = 15;
    assert_eq!(rx.accept(repair).unwrap(), ReceiveDisposition::Duplicate);
    rx.accept(frame(2, b"tail")).unwrap();
    assert!(rx.take_ready_bulk().is_none());
    rx.accept(frame(0, b"first")).unwrap();
    let batch = rx.take_ready_bulk().unwrap();
    assert_eq!(
        batch.iter().map(|(seq, _)| *seq).collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(batch[1].1.as_ptr(), allocation);
    assert_eq!(rx.take_ready_bulk().unwrap(), vec![(2, b"tail".to_vec())]);
    assert!(rx.take_ready_bulk().is_none());
    assert_eq!(rx.buffered_bytes(), 0);
    assert!(rx.is_complete());
    assert_eq!(
        rx.accept(frame(1, b"second")).unwrap(),
        ReceiveDisposition::Duplicate
    );
    assert!(rx.accept(frame(1, b"forged")).is_err());
    let mut other_object = frame(1, b"second");
    other_object.object_id = 0;
    assert!(rx.accept(other_object).is_err());
}

#[test]
fn rejects_out_of_range_conflicts_and_wrong_session_without_mutation() {
    let mut rx = NativeStructuralReceiver::new([7; 16], 1, 2, limits()).unwrap();
    let mut foreign = frame(0, b"x");
    foreign.session_id = [8; 16];
    assert_eq!(
        rx.accept(foreign).unwrap(),
        ReceiveDisposition::IgnoredSession
    );
    let mut foreign = frame(0, b"x");
    foreign.stream_id = 2;
    assert_eq!(
        rx.accept(foreign).unwrap(),
        ReceiveDisposition::IgnoredSession
    );
    let mut control = frame(u64::MAX, b"x");
    control.kind = NovoRudpTransportFrameKindV0::Done;
    assert_eq!(
        rx.accept(control).unwrap(),
        ReceiveDisposition::IgnoredControl
    );
    assert!(rx.accept(frame(2, b"x")).is_err());
    assert!(rx.accept(frame(0, b"123456789")).is_err());
    assert_eq!(rx.received_count(), 0);
    rx.accept(frame(0, b"original")).unwrap();
    assert!(rx.accept(frame(0, b"mutated")).is_err());
    assert_eq!(rx.buffered_payloads()[&0], b"original");
    assert_eq!(rx.received_count(), 1);
    assert!(!rx.is_complete());
}

#[test]
fn future_frames_cannot_occupy_repair_capacity_and_backpressure_returns_input() {
    let mut rx = NativeStructuralReceiver::new([7; 16], 1, 6, limits()).unwrap();
    rx.accept(frame(4, b"12345678")).unwrap();
    rx.accept(frame(5, b"12345678")).unwrap();
    let retry = frame(3, b"12345678");
    let retry = match rx.accept(retry.clone()).unwrap() {
        ReceiveDisposition::Backpressured(returned) => {
            assert_eq!(returned, retry);
            returned
        }
        outcome => panic!("expected backpressure, got {outcome:?}"),
    };
    assert!(!rx.has_received(3));
    rx.accept(frame(1, b"12345678")).unwrap();
    rx.accept(frame(0, b"12345678")).unwrap();
    assert_eq!(rx.buffered_bytes(), 32);
    assert_eq!(rx.take_ready_bulk().unwrap().len(), 2);
    assert_eq!(rx.accept(retry).unwrap(), ReceiveDisposition::Accepted);
    rx.accept(frame(2, b"12345678")).unwrap();
    assert_eq!(rx.take_ready_bulk().unwrap()[0].0, 2);
    assert_eq!(rx.take_ready_bulk().unwrap()[0].0, 4);
    assert!(rx.is_complete());
}

#[test]
fn frontier_count_tracks_only_accepted_members_across_out_of_order_bulk_boundaries() {
    fn assert_count(rx: &NativeStructuralReceiver, expected: usize) {
        let end = (rx.next_sequence + rx.limits.bulk_size as u64).min(rx.expected_total);
        let recomputed = (rx.next_sequence..end)
            .filter(|sequence| !rx.buffered.contains_key(sequence))
            .count();
        assert_eq!(rx.frontier_missing, recomputed);
        assert_eq!(rx.frontier_missing, expected);
    }

    let mut rx = NativeStructuralReceiver::new([7; 16], 1, 7, limits()).unwrap();
    assert_count(&rx, 2);
    for sequence in [5, 4] {
        assert_eq!(
            rx.accept(frame(sequence, b"x")).unwrap(),
            ReceiveDisposition::Accepted
        );
        assert_count(&rx, 2);
    }
    assert!(matches!(
        rx.accept(frame(3, b"x")).unwrap(),
        ReceiveDisposition::Backpressured(_)
    ));
    assert_count(&rx, 2);
    let mut foreign = frame(0, b"x");
    foreign.stream_id = 2;
    assert_eq!(
        rx.accept(foreign).unwrap(),
        ReceiveDisposition::IgnoredSession
    );
    assert_count(&rx, 2);
    assert!(rx.accept(frame(7, b"x")).is_err());
    assert_count(&rx, 2);
    assert!(rx.take_ready_bulk().is_none());
    assert_count(&rx, 2);

    assert_eq!(
        rx.accept(frame(1, b"x")).unwrap(),
        ReceiveDisposition::Accepted
    );
    assert_count(&rx, 1);
    let mut duplicate = frame(1, b"x");
    duplicate.kind = NovoRudpTransportFrameKindV0::Repair;
    assert_eq!(rx.accept(duplicate).unwrap(), ReceiveDisposition::Duplicate);
    assert_count(&rx, 1);
    assert!(rx.accept(frame(1, b"altered")).is_err());
    assert_count(&rx, 1);
    assert_eq!(
        rx.accept(frame(0, b"x")).unwrap(),
        ReceiveDisposition::Accepted
    );
    assert_count(&rx, 0);
    assert_eq!(
        rx.accept(frame(0, b"x")).unwrap(),
        ReceiveDisposition::Duplicate
    );
    assert_count(&rx, 0);
    assert!(matches!(
        rx.accept(frame(6, b"x")).unwrap(),
        ReceiveDisposition::Backpressured(_)
    ));
    assert_count(&rx, 0);

    assert_eq!(rx.take_ready_bulk().unwrap()[0].0, 0);
    assert_count(&rx, 2);
    assert!(rx.accept(frame(2, b"123456789")).is_err());
    assert_count(&rx, 2);
    for (sequence, missing) in [(3, 1), (2, 0)] {
        assert_eq!(
            rx.accept(frame(sequence, b"x")).unwrap(),
            ReceiveDisposition::Accepted
        );
        assert_count(&rx, missing);
    }
    assert_eq!(rx.take_ready_bulk().unwrap()[0].0, 2);
    // Both members arrived before either preceding bulk was consumed.
    assert_count(&rx, 0);
    assert_eq!(rx.take_ready_bulk().unwrap()[0].0, 4);
    assert_count(&rx, 1);
    assert_eq!(
        rx.accept(frame(4, b"x")).unwrap(),
        ReceiveDisposition::Duplicate
    );
    assert_count(&rx, 1);
    assert_eq!(
        rx.accept(frame(6, b"x")).unwrap(),
        ReceiveDisposition::Accepted
    );
    assert_count(&rx, 0);
    assert_eq!(rx.take_ready_bulk().unwrap(), vec![(6, b"x".to_vec())]);
    assert_count(&rx, 0);
    assert!(rx.take_ready_bulk().is_none());
    assert_count(&rx, 0);
    assert!(rx.is_complete());
}

#[test]
fn small_bulk_then_large_repairs_cannot_stall_at_the_next_frontier() {
    // Previously accepted, but two 1-byte current payloads plus two 7-byte
    // future payloads exhausted this budget. Draining the current bulk left
    // only 2 bytes, so neither next-bulk 8-byte repair could be admitted.
    let mut undersized = limits();
    undersized.max_buffered_bytes = 16;
    assert!(NativeStructuralReceiver::new([7; 16], 1, 6, undersized).is_err());

    let mut overflow = limits();
    overflow.max_payload_bytes = usize::MAX;
    overflow.max_buffered_bytes = usize::MAX;
    assert!(NativeStructuralReceiver::new([7; 16], 1, 6, overflow).is_err());

    let mut rx = NativeStructuralReceiver::new([7; 16], 1, 6, limits()).unwrap();
    for (sequence, bytes) in [
        (0, b"a".as_slice()),
        (1, b"b"),
        (4, b"1234567"),
        (5, b"7654321"),
    ] {
        assert_eq!(
            rx.accept(frame(sequence, bytes)).unwrap(),
            ReceiveDisposition::Accepted
        );
    }
    assert_eq!(rx.buffered_bytes(), 16);
    assert_eq!(
        rx.take_ready_bulk()
            .unwrap()
            .iter()
            .map(|(n, _)| *n)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(rx.buffered_bytes(), 14);
    for sequence in [3, 2] {
        let mut repair = frame(sequence, b"12345678");
        repair.kind = NovoRudpTransportFrameKindV0::Repair;
        assert_eq!(rx.accept(repair).unwrap(), ReceiveDisposition::Accepted);
    }
    assert_eq!(rx.buffered_bytes(), 30);
    assert_eq!(
        rx.take_ready_bulk()
            .unwrap()
            .iter()
            .map(|(n, _)| *n)
            .collect::<Vec<_>>(),
        [2, 3]
    );
    assert_eq!(
        rx.take_ready_bulk()
            .unwrap()
            .iter()
            .map(|(n, _)| *n)
            .collect::<Vec<_>>(),
        [4, 5]
    );
    assert_eq!(rx.buffered_bytes(), 0);
    assert!(rx.is_complete());
}

#[test]
fn default_bulk_is_128_and_session_fingerprint_budget_is_hard_bounded() {
    let mut rx =
        NativeStructuralReceiver::new([7; 16], 1, 129, StructuralReceiveLimits::default()).unwrap();
    for sequence in (1..129).rev() {
        rx.accept(frame(sequence, b"x")).unwrap();
    }
    assert!(rx.take_ready_bulk().is_none());
    rx.accept(frame(0, b"x")).unwrap();
    assert_eq!(rx.take_ready_bulk().unwrap().len(), 128);
    assert_eq!(rx.take_ready_bulk().unwrap().len(), 1);
    assert_eq!(rx.received_count(), 129);
    let oversized = StructuralReceiveLimits {
        max_total_frames: MAX_STRUCTURAL_SESSION_FRAMES + 1,
        ..StructuralReceiveLimits::default()
    };
    assert!(NativeStructuralReceiver::new([7; 16], 1, 1, oversized).is_err());
    let mut too_small = limits();
    too_small.max_buffered_bytes = 15;
    assert!(NativeStructuralReceiver::new([7; 16], 1, 3, too_small).is_err());
}

fn apfl_limits() -> ApflLimits {
    ApflLimits {
        transactions: 8,
        transaction_bytes: 4096,
        body_bytes: 8 * 4096,
    }
}

fn signed(nonce: u64) -> TransferV3 {
    let key = SigningKey::from_bytes(&[9; 32]);
    let mut tx = TransferV3 {
        chain_id: 71,
        from: key.verifying_key().to_bytes().to_vec(),
        to: vec![3; 32],
        asset: "NOV".into(),
        amount: 100 + nonce as u128,
        nonce,
        fee_policy: FeePolicy {
            pay_asset: "NOV".into(),
            max_pay_amount: 100,
            slippage_bps: 0,
        },
        signature: key.verifying_key().to_bytes().to_vec(),
    };
    tx.signature
        .extend_from_slice(&key.sign(&signing_message(&tx).unwrap()).to_bytes());
    tx
}

#[test]
fn real_structured_v3_authentication_preserves_views_and_original_authority() {
    let raw: Vec<_> = (0..3)
        .map(|n| encode_transfer_v3(&signed(n)).unwrap())
        .collect();
    let payload = ApflTransferBatch::from_raw(&raw, apfl_limits())
        .unwrap()
        .encode()
        .unwrap();
    let checked = authenticate_structured_v3(&payload, 3, 71, apfl_limits()).unwrap();
    for (transaction, raw) in checked.iter().zip(&raw) {
        let original = authenticate_transfer_v3(raw, 71, 4096).unwrap();
        assert!(transaction.is_apfl_view());
        assert_eq!(transaction.transfer(), original.transfer());
        assert_eq!(transaction.tx_hash(), original.tx_hash());
        assert_eq!(transaction.nonce_identity(), original.nonce_identity());
    }
    assert!(authenticate_structured_v3(&payload, 2, 71, apfl_limits()).is_err());
    assert!(authenticate_structured_v3(&payload, 3, 72, apfl_limits()).is_err());
    assert!(authenticate_structured_v3(b"NOVRUDP-APFL-NTX-V0", 1, 71, apfl_limits()).is_err());
    let mut forged = signed(1);
    forged.amount += 1;
    let raw = vec![raw[0].clone(), encode_transfer_v3(&forged).unwrap()];
    let forged = ApflTransferBatch::from_raw(&raw, apfl_limits())
        .unwrap()
        .encode()
        .unwrap();
    assert!(authenticate_structured_v3(&forged, 2, 71, apfl_limits()).is_err());
}
