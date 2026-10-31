//! Isolated preparation experiment; not a second product encryption path.
use super::*;
use std::{hint::black_box, time::Instant};

struct PreparedSeal<'a> {
    channel: &'a mut E2eSecureChannelV1,
    cipher: ChaCha20Poly1305,
    aad: Vec<u8>,
}

// Local unsent benchmark fixtures only. Both variants must use the exact same
// key/session/sequence/payload bytes, not separately randomized handshakes.
fn copy_benchmark_fixture(source: &E2eSecureChannelV1) -> E2eSecureChannelV1 {
    E2eSecureChannelV1 {
        session_id: source.session_id,
        local_peer_id: source.local_peer_id.clone(),
        remote_peer_id: source.remote_peer_id.clone(),
        outbound_key: source.outbound_key,
        inbound_key: source.inbound_key,
        outbound_nonce_prefix: source.outbound_nonce_prefix,
        inbound_nonce_prefix: source.inbound_nonce_prefix,
        outbound_sequence: source.outbound_sequence,
        inbound_replay: source.inbound_replay.clone(),
    }
}

impl<'a> PreparedSeal<'a> {
    fn new(channel: &'a mut E2eSecureChannelV1) -> Self {
        let cipher = ChaCha20Poly1305::new((&channel.outbound_key).into());
        let aad = secure_frame_aad_v1(&SecureNovoRudpEnvelopeV1 {
            version: PRODUCT_OVERLAY_PROTOCOL_VERSION_V1,
            session_id: channel.session_id,
            sender_peer_id: channel.local_peer_id.clone(),
            recipient_peer_id: channel.remote_peer_id.clone(),
            sequence: 0,
            nonce: [0; 12],
            ciphertext: Vec::new(),
        });
        Self {
            channel,
            cipher,
            aad,
        }
    }

    fn seal(
        &mut self,
        frame: &NovoRudpTransportFrameV0,
    ) -> Result<SecureNovoRudpEnvelopeV1, ProductOverlayErrorV1> {
        let _cost = io_span(IoPhase::SecureSeal);
        let sequence = self.channel.outbound_sequence;
        self.channel.outbound_sequence = sequence
            .checked_add(1)
            .ok_or(ProductOverlayErrorV1::SecureFrameSequenceExhausted)?;
        let nonce = secure_frame_nonce_v1(self.channel.outbound_nonce_prefix, sequence);
        let suffix = self.aad.len() - 20;
        self.aad[suffix..suffix + 8].copy_from_slice(&sequence.to_be_bytes());
        self.aad[suffix + 8..].copy_from_slice(&nonce);
        let mut envelope = SecureNovoRudpEnvelopeV1 {
            version: PRODUCT_OVERLAY_PROTOCOL_VERSION_V1,
            session_id: self.channel.session_id,
            sender_peer_id: self.channel.local_peer_id.clone(),
            recipient_peer_id: self.channel.remote_peer_id.clone(),
            sequence,
            nonce,
            ciphertext: {
                let _cost = io_span(IoPhase::FrameEncode);
                frame.encode_for_aead()
            },
        };
        let _cost = io_span(IoPhase::SealAead);
        self.cipher
            .encrypt_in_place(
                Nonce::from_slice(&nonce),
                &self.aad,
                &mut envelope.ciphertext,
            )
            .map_err(|_| ProductOverlayErrorV1::SecureFrameAuthenticationFailed)?;
        Ok(envelope)
    }
}

#[test]
fn isolated_preparation_keeps_exact_ciphertext_nonce_replay_and_sequence_exhaustion() {
    let (mut sender, mut receiver) = channels();
    let mut prepared = PreparedSeal::new(&mut sender);
    for (index, length) in [0, 1, 64, 767, 1088].into_iter().enumerate() {
        let mut input = frame(index as u64);
        input.payload = vec![index as u8; length];
        let sealed = prepared.seal(&input).unwrap();
        assert_eq!(prepared.aad, secure_frame_aad_v1(&sealed));
        let expected = ChaCha20Poly1305::new((&prepared.channel.outbound_key).into())
            .encrypt(
                Nonce::from_slice(&sealed.nonce),
                Payload {
                    msg: &input.encode(),
                    aad: &secure_frame_aad_v1(&sealed),
                },
            )
            .unwrap();
        assert_eq!(sealed.ciphertext, expected);
        let mut corrupted = sealed.clone();
        *corrupted.ciphertext.last_mut().unwrap() ^= 1;
        assert!(matches!(
            receiver.open_novorudp_frame_owned(corrupted),
            Err(ProductOverlayErrorV1::SecureFrameAuthenticationFailed)
        ));
        assert_eq!(
            receiver
                .open_novorudp_frame_owned(sealed.clone())
                .unwrap()
                .encode(),
            input.encode()
        );
        assert!(matches!(
            receiver.open_novorudp_frame_owned(sealed),
            Err(ProductOverlayErrorV1::SecureFrameReplay)
        ));
    }
    prepared.channel.outbound_sequence = u64::MAX;
    assert!(matches!(
        prepared.seal(&frame(99)),
        Err(ProductOverlayErrorV1::SecureFrameSequenceExhausted)
    ));
    assert_eq!(prepared.channel.outbound_sequence, u64::MAX);
}

#[test]
#[ignore = "isolated original vs bounded preparation experiment; run release alone, diagnostics off"]
fn benchmark_original_and_prepared_aead_batches() {
    assert!(!crate::novorudp::diagnostics::io_cost_enabled());
    for (batch, payload) in [(32, 64), (1, 768), (8, 768), (32, 768)] {
        let (fixture, _) = channels();
        let mut samples = Vec::new();
        for repetition in 0..5 {
            for prepared in [false, true, false] {
                let mut sender = copy_benchmark_fixture(&fixture);
                let frames = (0..batch)
                    .map(|i| {
                        let mut input = frame(i as u64);
                        input.payload = vec![i as u8; payload];
                        input
                    })
                    .collect::<Vec<_>>();
                let cpu_before = novovm_udp_batch::read_process_cpu_time(std::process::id()).ok();
                let started = Instant::now();
                let mut latencies = Vec::with_capacity(65_536 / batch);
                for _ in 0..65_536 / batch {
                    let batch_started = Instant::now();
                    if prepared {
                        let mut sealer = PreparedSeal::new(&mut sender);
                        for input in &frames {
                            black_box(sealer.seal(black_box(input)).unwrap());
                        }
                    } else {
                        for input in &frames {
                            black_box(sender.seal_novorudp_frame(black_box(input)).unwrap());
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
                samples.push(serde_json::json!({
                    "repetition":repetition,"prepared":prepared,"packets":65_536,"wall_us":wall_us,
                    "cpu_ns":cpu_ns,"batch_p50_ns":latencies[(latencies.len()-1)/2],
                    "batch_p95_ns":latencies[(latencies.len()*95).div_ceil(100)-1],
                    "batch_p99_ns":latencies[(latencies.len()*99).div_ceil(100)-1]
                }));
            }
        }
        eprintln!(
            "AEAD_PREPARATION {}",
            serde_json::json!({
                "scope":"isolated seal including preparation, framing, encryption and output allocation; not transport or TPS; batch scratch is dropped each turn",
                "batch":batch,"payload_bytes":payload,"samples":samples
            })
        );
    }
}
