use super::{aead_preparation::copy_benchmark_fixture, *};
use std::{hint::black_box, time::Instant};

#[test]
fn direct_wire_matches_existing_envelope_and_mutable_frame_encoding_in_both_directions() {
    let (mut alice, mut bob) = channels();
    let mut reference_alice = copy_benchmark_fixture(&alice);
    let mut reference_bob = copy_benchmark_fixture(&bob);
    for index in 0..64 {
        for direction in 0..2 {
            let (sender, receiver, reference) = if direction == 0 {
                (&mut alice, &mut bob, &mut reference_alice)
            } else {
                (&mut bob, &mut alice, &mut reference_bob)
            };
            let mut input = frame(index);
            input.session_id = sender.session_id();
            input.payload = vec![index as u8; [0, 1, 64, 768, 848, 871][index as usize % 6]];
            if index % 2 != 0 {
                input.kind = NovoRudpTransportFrameKindV0::Repair;
            }
            let wire = sender.seal_novorudp_wire(&input, 1200).unwrap();
            let envelope = reference.seal_novorudp_frame(&input).unwrap();
            assert_eq!(wire, binary::encode(&envelope, 1200).unwrap());
            assert_eq!(binary::decode(&wire, 1200).unwrap(), envelope);
            let decoded = receiver
                .open_novorudp_frame_owned(binary::decode_owned(wire.clone(), 1200).unwrap())
                .unwrap();
            assert_eq!(decoded.encode(), input.encode());
            assert!(matches!(
                receiver.open_novorudp_frame(&envelope),
                Err(ProductOverlayErrorV1::SecureFrameReplay)
            ));
            assert_eq!(sender.outbound_sequence, reference.outbound_sequence);
        }
    }
}

#[test]
fn direct_wire_rejection_keeps_nonce_progression_and_sequence_exhaustion() {
    let (mut sender, _) = channels();
    let mut reference = copy_benchmark_fixture(&sender);
    let input = frame(1);
    let envelope = reference.seal_novorudp_frame(&input).unwrap();
    let wire_len = binary::encode(&envelope, 1200).unwrap().len();
    assert!(binary::encode(&envelope, wire_len - 1).is_err());
    assert!(sender.seal_novorudp_wire(&input, wire_len - 1).is_err());
    assert_eq!(sender.outbound_sequence, reference.outbound_sequence);
    assert_eq!(
        sender.seal_novorudp_wire(&input, wire_len).unwrap(),
        binary::encode(&reference.seal_novorudp_frame(&input).unwrap(), wire_len).unwrap()
    );
    sender.outbound_sequence = u64::MAX;
    let error = sender.seal_novorudp_wire(&input, 1200).unwrap_err();
    assert!(matches!(
        error.downcast_ref::<ProductOverlayErrorV1>(),
        Some(ProductOverlayErrorV1::SecureFrameSequenceExhausted)
    ));
    assert_eq!(sender.outbound_sequence, u64::MAX);
}

#[test]
#[ignore = "same-key final-wire allocation comparison; release only, diagnostics off, run alone"]
fn benchmark_existing_envelope_and_direct_wire() {
    assert!(!crate::novorudp::diagnostics::io_cost_enabled());
    for batch in [1, 8, 16, 32] {
        let (fixture, _) = channels();
        let frames = (0..batch)
            .map(|index| {
                let mut input = frame(index as u64);
                input.payload = vec![index as u8; if index + 1 == batch { 64 } else { 848 }];
                input
            })
            .collect::<Vec<_>>();
        let mut samples = Vec::new();
        for repetition in 0..5 {
            for direct in [false, true, false] {
                let mut sender = copy_benchmark_fixture(&fixture);
                let cpu_before = novovm_udp_batch::read_process_cpu_time(std::process::id()).ok();
                let started = Instant::now();
                let mut latencies = Vec::with_capacity(65_536 / batch);
                for _ in 0..65_536 / batch {
                    let batch_started = Instant::now();
                    for input in &frames {
                        let wire = if direct {
                            sender.seal_novorudp_wire(black_box(input), 1200).unwrap()
                        } else {
                            binary::encode(
                                &sender.seal_novorudp_frame(black_box(input)).unwrap(),
                                1200,
                            )
                            .unwrap()
                        };
                        black_box(wire);
                    }
                    latencies.push(batch_started.elapsed().as_nanos());
                }
                let wall_us = started.elapsed().as_micros();
                let cpu_ns = cpu_before.and_then(|before| {
                    novovm_udp_batch::read_process_cpu_time(std::process::id())
                        .ok()?
                        .checked_sub(before)?
                        .checked_total_ns()
                });
                latencies.sort_unstable();
                samples.push(serde_json::json!({"direct":direct,"repetition":repetition,"wall_us":wall_us,"cpu_ns":cpu_ns,
                    "batch_p50_ns":latencies[(latencies.len()-1)/2],"batch_p95_ns":latencies[(latencies.len()*95).div_ceil(100)-1],
                    "batch_p99_ns":latencies[(latencies.len()*99).div_ceil(100)-1]}));
            }
        }
        eprintln!(
            "NATIVE_SEND_WIRE {}",
            serde_json::json!({"scope":"real existing seal+codec vs real direct final-wire method; same key/session/plaintext/nonce sequence, 65536 calls; no OS sends or delivery/TPS claims; frames/checksums supplied before clock", "batch":batch,"full_payload":848,"tail_payload":64,"samples":samples})
        );
    }
}
