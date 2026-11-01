//! Original seal/open contract versus session-local AAD preparation.
use super::*;
use std::{hint::black_box, time::Instant};

// Local, unsent fixtures only: never clone a live session's sending state.
pub(super) fn copy_benchmark_fixture(source: &E2eSecureChannelV1) -> E2eSecureChannelV1 {
    E2eSecureChannelV1 {
        session_id: source.session_id,
        local_peer_id: source.local_peer_id.clone(),
        remote_peer_id: source.remote_peer_id.clone(),
        outbound_key: source.outbound_key,
        inbound_key: source.inbound_key,
        outbound_nonce_prefix: source.outbound_nonce_prefix,
        inbound_nonce_prefix: source.inbound_nonce_prefix,
        outbound_sequence: source.outbound_sequence,
        outbound_aad: source.outbound_aad.clone(),
        inbound_aad: source.inbound_aad.clone(),
        inbound_replay: source.inbound_replay.clone(),
    }
}

// Frozen 8c778ab9's uncached preparation, kept ONLY as a local benchmark and
// independent wire-contract oracle. Product calls always use the real methods.
fn uncached_seal(
    channel: &mut E2eSecureChannelV1,
    frame: &NovoRudpTransportFrameV0,
) -> Result<SecureNovoRudpEnvelopeV1, ProductOverlayErrorV1> {
    let sequence = channel.outbound_sequence;
    channel.outbound_sequence = sequence
        .checked_add(1)
        .ok_or(ProductOverlayErrorV1::SecureFrameSequenceExhausted)?;
    let nonce = secure_frame_nonce_v1(channel.outbound_nonce_prefix, sequence);
    let mut envelope = SecureNovoRudpEnvelopeV1 {
        version: PRODUCT_OVERLAY_PROTOCOL_VERSION_V1,
        session_id: channel.session_id,
        sender_peer_id: channel.local_peer_id.clone(),
        recipient_peer_id: channel.remote_peer_id.clone(),
        sequence,
        nonce,
        ciphertext: frame.encode_for_aead(),
    };
    ChaCha20Poly1305::new((&channel.outbound_key).into())
        .encrypt_in_place(
            Nonce::from_slice(&nonce),
            &secure_frame_aad_v1(&envelope),
            &mut envelope.ciphertext,
        )
        .map_err(|_| ProductOverlayErrorV1::SecureFrameAuthenticationFailed)?;
    Ok(envelope)
}

fn uncached_open(
    channel: &mut E2eSecureChannelV1,
    mut envelope: SecureNovoRudpEnvelopeV1,
) -> Result<NovoRudpTransportFrameV0, ProductOverlayErrorV1> {
    if envelope.version != PRODUCT_OVERLAY_PROTOCOL_VERSION_V1 {
        return Err(ProductOverlayErrorV1::UnsupportedVersion(envelope.version));
    }
    if envelope.session_id != channel.session_id
        || envelope.sender_peer_id != channel.remote_peer_id
        || envelope.recipient_peer_id != channel.local_peer_id
    {
        return Err(ProductOverlayErrorV1::SecureFrameRouteMismatch);
    }
    if envelope.nonce != secure_frame_nonce_v1(channel.inbound_nonce_prefix, envelope.sequence) {
        return Err(ProductOverlayErrorV1::SecureFrameNonceMismatch);
    }
    if !channel.inbound_replay.would_accept(envelope.sequence) {
        return Err(ProductOverlayErrorV1::SecureFrameReplay);
    }
    let aad = secure_frame_aad_v1(&envelope);
    ChaCha20Poly1305::new((&channel.inbound_key).into())
        .decrypt_in_place(
            Nonce::from_slice(&envelope.nonce),
            &aad,
            &mut envelope.ciphertext,
        )
        .map_err(|_| ProductOverlayErrorV1::SecureFrameAuthenticationFailed)?;
    if !channel.inbound_replay.mark_authenticated(envelope.sequence) {
        return Err(ProductOverlayErrorV1::SecureFrameReplay);
    }
    Ok(NovoRudpTransportFrameV0::decode(&envelope.ciphertext)?)
}

#[test]
fn session_aad_matches_uncached_bytes_in_both_directions_without_buffer_growth() {
    let (mut alice, mut bob) = channels();
    let pointers = [
        alice.outbound_aad.as_ptr(),
        alice.inbound_aad.as_ptr(),
        bob.outbound_aad.as_ptr(),
        bob.inbound_aad.as_ptr(),
    ];
    let capacities = [
        alice.outbound_aad.capacity(),
        alice.inbound_aad.capacity(),
        bob.outbound_aad.capacity(),
        bob.inbound_aad.capacity(),
    ];
    let mut oracle_alice = copy_benchmark_fixture(&alice);
    let mut oracle_bob = copy_benchmark_fixture(&bob);
    for index in 0..256 {
        for direction in 0..2 {
            let (sender, receiver, oracle) = if direction == 0 {
                (&mut alice, &mut bob, &mut oracle_alice)
            } else {
                (&mut bob, &mut alice, &mut oracle_bob)
            };
            let mut input = frame(index);
            input.payload = vec![index as u8; [0, 1, 64, 768, 1088, 7][index as usize % 6]];
            let envelope = sender.seal_novorudp_frame(&input).unwrap();
            assert_eq!(envelope, uncached_seal(oracle, &input).unwrap());
            assert_eq!(sender.outbound_aad, secure_frame_aad_v1(&envelope));
            let decoded = if index % 2 == 0 {
                receiver
                    .open_novorudp_frame_owned(envelope.clone())
                    .unwrap()
            } else {
                receiver.open_novorudp_frame(&envelope).unwrap()
            };
            assert_eq!(decoded.encode(), input.encode());
            assert_eq!(receiver.inbound_aad, secure_frame_aad_v1(&envelope));
        }
    }
    assert_eq!(
        pointers,
        [
            alice.outbound_aad.as_ptr(),
            alice.inbound_aad.as_ptr(),
            bob.outbound_aad.as_ptr(),
            bob.inbound_aad.as_ptr()
        ]
    );
    assert_eq!(
        capacities,
        [
            alice.outbound_aad.capacity(),
            alice.inbound_aad.capacity(),
            bob.outbound_aad.capacity(),
            bob.inbound_aad.capacity()
        ]
    );
    alice.outbound_sequence = u64::MAX;
    let previous = alice.outbound_aad.clone();
    assert!(matches!(
        alice.seal_novorudp_frame(&frame(0)),
        Err(ProductOverlayErrorV1::SecureFrameSequenceExhausted)
    ));
    assert_eq!(alice.outbound_sequence, u64::MAX);
    assert_eq!(alice.outbound_aad, previous);
}

#[test]
fn rejected_dynamic_aad_and_other_sessions_do_not_poison_replay_or_preparation() {
    let (mut alice, mut bob) = channels();
    let (_, mut other_bob) = channels();
    let first = alice.seal_novorudp_frame(&frame(0)).unwrap();
    assert!(matches!(
        other_bob.open_novorudp_frame(&first),
        Err(ProductOverlayErrorV1::SecureFrameRouteMismatch)
    ));
    let second = alice.seal_novorudp_frame(&frame(1)).unwrap();
    let mut tampered = second.clone();
    tampered.sequence = 100;
    tampered.nonce = secure_frame_nonce_v1(bob.inbound_nonce_prefix, 100);
    assert!(matches!(
        bob.open_novorudp_frame_owned(tampered),
        Err(ProductOverlayErrorV1::SecureFrameAuthenticationFailed)
    ));
    assert!(bob.inbound_replay.would_accept(100));
    assert_eq!(
        bob.open_novorudp_frame_owned(second.clone())
            .unwrap()
            .encode(),
        frame(1).encode()
    );
    assert_eq!(
        bob.open_novorudp_frame(&first).unwrap().encode(),
        frame(0).encode()
    );
    assert!(matches!(
        bob.open_novorudp_frame_owned(second),
        Err(ProductOverlayErrorV1::SecureFrameReplay)
    ));
    let mut wrong_direction = first;
    std::mem::swap(
        &mut wrong_direction.sender_peer_id,
        &mut wrong_direction.recipient_peer_id,
    );
    assert!(matches!(
        bob.open_novorudp_frame(&wrong_direction),
        Err(ProductOverlayErrorV1::SecureFrameRouteMismatch)
    ));
}

#[test]
fn concurrent_channels_keep_directional_scratch_independent() {
    std::thread::scope(|scope| {
        for task in 0..4 {
            scope.spawn(move || {
                let (mut sender, mut receiver) = channels();
                for index in 0..128 {
                    let mut input = frame(index);
                    input.payload = vec![task; 768];
                    let envelope = sender.seal_novorudp_frame(&input).unwrap();
                    assert_eq!(
                        receiver
                            .open_novorudp_frame_owned(envelope)
                            .unwrap()
                            .encode(),
                        input.encode()
                    );
                }
            });
        }
    });
}

#[test]
#[ignore = "isolated uncached reference vs real session seal/open; run release alone, diagnostics off"]
fn benchmark_session_aad_reuse_seal_and_open() {
    assert!(!crate::novorudp::diagnostics::io_cost_enabled());
    for batch in [1, 8, 16, 32] {
        for mixed in [false, true] {
            let (sender_fixture, receiver_fixture) = channels();
            let frames = (0..batch)
                .map(|i| {
                    let mut input = frame(i as u64);
                    let length = if mixed {
                        [64, 1088, 768, 1][i % 4]
                    } else {
                        768
                    };
                    input.payload = vec![i as u8; length];
                    input
                })
                .collect::<Vec<_>>();
            let mut producer = copy_benchmark_fixture(&sender_fixture);
            let encrypted = (0..65_536)
                .map(|i| producer.seal_novorudp_frame(&frames[i % batch]).unwrap())
                .collect::<Vec<_>>();
            for operation in ["seal", "open"] {
                let mut samples = Vec::new();
                for repetition in 0..3 {
                    for cached in [false, true, false] {
                        let mut sender = copy_benchmark_fixture(&sender_fixture);
                        let mut receiver = copy_benchmark_fixture(&receiver_fixture);
                        // Owned inputs are prepared outside both open clocks.
                        let mut inputs = encrypted.clone().into_iter();
                        let cpu_before =
                            novovm_udp_batch::read_process_cpu_time(std::process::id()).ok();
                        let started = Instant::now();
                        let mut latencies = Vec::with_capacity(65_536 / batch);
                        for _ in 0..65_536 / batch {
                            let batch_started = Instant::now();
                            for input in &frames {
                                if operation == "seal" {
                                    black_box(
                                        if cached {
                                            sender.seal_novorudp_frame(black_box(input))
                                        } else {
                                            uncached_seal(&mut sender, black_box(input))
                                        }
                                        .unwrap(),
                                    );
                                } else {
                                    let envelope = black_box(inputs.next().unwrap());
                                    black_box(
                                        if cached {
                                            receiver.open_novorudp_frame_owned(envelope)
                                        } else {
                                            uncached_open(&mut receiver, envelope)
                                        }
                                        .unwrap(),
                                    );
                                }
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
                        samples.push(serde_json::json!({"repetition":repetition,"cached":cached,"wall_us":wall_us,"cpu_ns":cpu_ns,
                            "batch_p50_ns":latencies[(latencies.len()-1)/2],"batch_p95_ns":latencies[(latencies.len()*95).div_ceil(100)-1],"batch_p99_ns":latencies[(latencies.len()*99).div_ceil(100)-1]}));
                    }
                }
                eprintln!(
                    "AEAD_SESSION_AAD {}",
                    serde_json::json!({"scope":"isolated 65536 real seal/open calls vs frozen uncached test reference, same keys/session/inputs; not TPS; no batch-fill wait; input cloning before clocks; frame/output allocation included", "operation":operation,"batch":batch,"mixed_lengths":mixed,"samples":samples})
                );
            }
        }
    }
}
