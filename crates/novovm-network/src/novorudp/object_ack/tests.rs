use super::*;

fn ack() -> NovoRudpAckFrame {
    NovoRudpAckFrame {
        header: NovoRudpFrameHeader {
            version: 1,
            kind: NovoRudpFrameKind::Ack,
            session_id: [1; 16],
            epoch: 3,
            sequence: None,
            window_id: Some(0),
        },
        expected_total: 150,
        receiver_done: false,
        missing_count: 4,
        current_window: Some(NovoRudpRange::new(0, 63)),
        current_window_missing_ranges: vec![NovoRudpRange::new(7, 8)],
    }
}

#[test]
fn exact_object_binding_and_invalid_feedback_cannot_change_sender_state() {
    let expected = ack();
    let id = [2; 32];
    let frame = encode_object_ack(id, 7, &expected).unwrap();
    assert_eq!(
        decode_object_ack(&frame, [1; 16], 7, id, 150).unwrap(),
        expected
    );
    assert!(decode_object_ack(&frame, [9; 16], 7, id, 150).is_err());
    assert!(decode_object_ack(&frame, [1; 16], 8, id, 150).is_err());
    let mut collision = id;
    collision[31] ^= 1;
    assert!(decode_object_ack(&frame, [1; 16], 7, collision, 150).is_err());
    assert!(decode_object_ack(&frame, [1; 16], 7, id, 149).is_err());
    for end in 0..frame.payload.len() {
        let mut bad = frame.clone();
        bad.payload.truncate(end);
        assert!(decode_object_ack(&bad, [1; 16], 7, id, 150).is_err());
    }
    for offset in [48, 49, 58] {
        let mut bad = frame.clone();
        bad.payload[offset] = 255;
        assert!(decode_object_ack(&bad, [1; 16], 7, id, 150).is_err());
    }
    let mut bad = expected.clone();
    bad.receiver_done = true;
    assert!(encode_object_ack(id, 7, &bad).is_err());
    bad = expected;
    bad.current_window_missing_ranges
        .push(NovoRudpRange::new(8, 9));
    assert!(encode_object_ack(id, 7, &bad).is_err());
}

#[test]
fn hash_pending_and_transport_done_remain_distinct_and_reuse_original_selector() {
    let mut feedback = ack();
    feedback.missing_count = 0;
    feedback.current_window = None;
    feedback.current_window_missing_ranges.clear();
    feedback.header.window_id = None;
    let frame = encode_object_ack([2; 32], 7, &feedback).unwrap();
    let decoded = decode_object_ack(&frame, [1; 16], 7, [2; 32], 150).unwrap();
    let mut state = NovoRudpSenderState::new();
    assert_eq!(
        sender_repair_decision_from_ack(
            &mut state,
            &decoded,
            &NovoRudpWindowConfig::default(),
            &NovoRudpPacingProfile::default()
        ),
        NovoRudpSenderRepairDecision::WindowComplete
    );
    feedback.receiver_done = true;
    feedback.header.epoch += 1;
    let frame = encode_object_ack([2; 32], 7, &feedback).unwrap();
    let decoded = decode_object_ack(&frame, [1; 16], 7, [2; 32], 150).unwrap();
    assert_eq!(
        sender_repair_decision_from_ack(
            &mut state,
            &decoded,
            &NovoRudpWindowConfig::default(),
            &NovoRudpPacingProfile::default()
        ),
        NovoRudpSenderRepairDecision::ReceiverDone
    );
}
