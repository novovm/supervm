use super::*;
use crate::{
    bound_session::session_clock::{
        authenticate_structured_bound_session_v2_observed,
        authenticate_structured_service_client_v2_observed,
    },
    iroh_transport::{with_iroh_scope_v1, IrohConfigV1, IrohControlV1},
    product_service_lease::{serve_self_owned_node_selected_v8, NodeServiceV1},
    transport_binding::{
        session_clock::{SessionClockInitiatorV2, SessionClockResponderV2},
        VerifiedTransportBinding, BOUND_TRANSPORT_ALPN_V1,
    },
};
use anyhow::Context;
use std::time::Duration;
use tokio::sync::oneshot;

fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
fn public(n: u8) -> [u8; 32] {
    key(n).verifying_key().to_bytes()
}
fn binding(initiator: bool) -> VerifiedTransportBinding {
    VerifiedTransportBinding::from_authenticated_transport(
        public(if initiator { 3 } else { 4 }),
        public(if initiator { 4 } else { 3 }),
        [7; 32],
        BOUND_TRANSPORT_ALPN_V1,
        initiator,
    )
    .unwrap()
}
fn pair() -> (BoundSecureChannelV1, BoundSecureChannelV1) {
    let deadline = SessionClockDeadlineV2::new(Instant::now() + Duration::from_secs(3)).unwrap();
    let a = SessionClockInitiatorV2::start(&key(1), public(2), binding(true), deadline).unwrap();
    let b = SessionClockResponderV2::respond(
        a.offer(),
        &key(2),
        public(1),
        binding(false),
        deadline,
        &mut HandshakeReplayCacheV1::new(8),
    )
    .unwrap();
    let mut a = a
        .complete(b.response(), &mut HandshakeReplayCacheV1::new(8))
        .unwrap();
    let mut b = b.into_pending();
    let af = a.make_finished().unwrap();
    let bf = b.make_finished().unwrap();
    a.verify_finished(&bf).unwrap();
    b.verify_finished(&af).unwrap();
    (a.into_channel().unwrap(), b.into_channel().unwrap())
}

#[test]
fn protected_sample_preserves_sequences_and_rejects_replay() -> Result<()> {
    let (mut a, mut b) = pair();
    let wire = encode_sample(&mut b, Some((1000, 1001, 1_000_000)))?;
    assert_eq!(open_sample(&mut a, &wire)?, Some((1000, 1001, 1_000_000)));
    assert!(open_sample(&mut a, &wire).is_err());
    // The original channel continues; server consumed 1, client still starts 1.
    let body = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        a.session_id(),
        7,
        2,
        1,
        0,
        vec![8],
    );
    let outgoing = a.seal_novorudp_frame(&body)?;
    assert_eq!(outgoing.sequence, 1);
    assert_eq!(b.open_novorudp_frame(&outgoing)?, body);
    let outgoing = b.seal_novorudp_frame(&body)?;
    assert_eq!(outgoing.sequence, 2);
    assert_eq!(a.open_novorudp_frame(&outgoing)?, body);
    Ok(())
}

#[test]
fn protected_sample_rejects_changed_mac_and_different_session() -> Result<()> {
    let (mut a, mut b) = pair();
    let wire = encode_sample(&mut b, None)?;
    let mut altered = binary::decode(&wire, WIRE_LIMIT)?;
    altered.ciphertext[0] ^= 1;
    assert!(open_sample(&mut a, &binary::encode(&altered, WIRE_LIMIT)?).is_err());
    let (mut other, _) = pair();
    assert!(open_sample(&mut other, &wire).is_err());
    Ok(())
}

#[test]
fn valid_aead_cannot_hide_wrong_route_or_schema() -> Result<()> {
    for wrong in 0..5 {
        let (mut a, mut b) = pair();
        let mut body = payload(&b, None)?;
        if wrong == 4 {
            body[0] ^= 1;
        }
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            b.session_id(),
            if wrong == 0 { STREAM + 1 } else { STREAM },
            if wrong == 1 { 2 } else { 1 },
            if wrong == 2 { 2 } else { 1 },
            u64::from(wrong == 3),
            body,
        );
        let sealed = b.seal_novorudp_frame(&frame)?;
        let wire = binary::encode(
            &crate::product_overlay::SecureNovoRudpEnvelopeV1 {
                version: sealed.version,
                session_id: sealed.session_id,
                sender_peer_id: sealed.sender_peer_id,
                recipient_peer_id: sealed.recipient_peer_id,
                sequence: sealed.sequence,
                nonce: sealed.nonce,
                ciphertext: sealed.ciphertext,
            },
            WIRE_LIMIT,
        )?;
        assert!(open_sample(&mut a, &wire).is_err(), "case {wrong}");
    }
    Ok(())
}

#[test]
fn sample_payload_context_and_unavailable_are_canonical() -> Result<()> {
    let (a, b) = pair();
    assert_eq!(decode_payload(&a, &payload(&b, None)?)?, None);
    let (other, _) = pair();
    assert!(decode_payload(&other, &payload(&b, None)?).is_err());
    let tag = [0];
    let nonzero = 1u64.to_le_bytes();
    let zero = 0u64.to_le_bytes();
    let fields: [&[u8]; 4] = [&tag, &nonzero, &zero, &zero];
    let columns: Vec<_> = fields
        .iter()
        .map(|f| Column::collect(std::iter::once(*f)))
        .collect();
    let wire =
        apfl_batch::encode(SCHEMA, context(&a), 1, &columns, LIMITS).map_err(anyhow::Error::msg)?;
    assert!(decode_payload(&a, &wire).is_err());
    Ok(())
}

#[derive(Clone, Copy)]
enum Case {
    Offset(i64),
    PeerUnavailable,
    LocalUnavailable,
    InvalidNumeric,
    SampleStall,
    SelectorStall,
    Revoked,
}

fn config() -> IrohConfigV1 {
    IrohConfigV1::new(&[], "127.0.0.1".parse().unwrap())
        .unwrap()
        .with_bind_address("127.0.0.1:0".parse().unwrap())
        .unwrap()
}
fn control() -> IrohControlV1 {
    IrohControlV1::new(Instant::now() + Duration::from_secs(8), || Ok(())).unwrap()
}
fn assert_deadline<T>(result: Result<T, BoundSessionError>) {
    let err = result.err().expect("deadline exposed success");
    assert!(
        matches!(err, BoundSessionError::Local(ref e)
        if matches!(e.downcast_ref::<TransportBindingErrorV1>(), Some(TransportBindingErrorV1::HandshakeDeadline))),
        "{err}"
    );
}
async fn run(case: Case) -> Result<()> {
    let _permit = crate::SERVICE_TEST_SERIAL.acquire().await?;
    let base = ClockAnchorV1::from_parts(1_000_000_000, Instant::now());
    let (addr_tx, addr_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let (ready_tx, ready_rx) = oneshot::channel();
    let server = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            addr_tx.send(scope.address()?).unwrap();
            let mut stream: CarrierStreamV1<'_> = scope.accept_inbound().await?.into();
            let deadline = Instant::now() + Duration::from_secs(3);
            if matches!(case, Case::SelectorStall) {
                ensure!(stream.read_frame().await? == SELECTOR);
                done_rx.await?;
                return Ok(());
            }
            let (kind, _, clock) = serve_self_owned_node_selected_v8::<()>(
                &mut stream,
                || anyhow::bail!("legacy dispatch"),
                || anyhow::bail!("legacy dispatch"),
                || anyhow::bail!("legacy dispatch"),
                || Ok(()),
            )
            .await?;
            ensure!(kind == NodeServiceV1::MailboxNativeV8);
            let mut channel = authenticate_structured_service_client_v2_observed(
                &mut stream,
                &key(2),
                ServicePeerPolicyV1::AnyAuthenticated,
                deadline,
                &mut HandshakeReplayCacheV1::new(8),
                &mut |_, _| Ok(()),
            )
            .await?
            .0;
            if matches!(case, Case::SampleStall) {
                ready_tx.send(()).unwrap();
                done_rx.await?;
                return Ok(());
            }
            if matches!(case, Case::Revoked) {
                let error = send_peer_time_v8(&mut stream, &mut channel, clock, deadline, || {
                    anyhow::bail!("fixture owner revoked")
                })
                .await
                .err()
                .expect("revoked owner sent time");
                ensure!(matches!(error, BoundSessionError::Local(_)));
                drop(stream);
                done_rx.await?;
                return Ok(());
            }
            if matches!(case, Case::InvalidNumeric) {
                // Authenticated peer supplies impossible residence, not forged identity.
                let wire = encode_sample(&mut channel, Some((1, 0, u64::MAX)))?;
                stream.write_frame(&wire).await?;
            } else {
                // Platform-injected clock offsets, without changing Windows time.
                let clock = if matches!(case, Case::PeerUnavailable) {
                    None
                } else {
                    let at = clock
                        .context("fixture clock capture unavailable")?
                        .instant();
                    let offset = if let Case::Offset(offset) = case {
                        offset
                    } else {
                        0
                    };
                    Some(ClockAnchorV1::from_parts(
                        (base.at(at)? as i64 + offset) as u64,
                        at,
                    ))
                };
                send_peer_time_v8(&mut stream, &mut channel, clock, deadline, || Ok(())).await?;
            }
            // Subsequent real encrypted business traffic uses the exact same keys/counters.
            let incoming = read_business(&stream.read_frame().await?)?;
            ensure!(incoming.sequence == 1);
            let body = channel.open_novorudp_frame(&incoming)?;
            let outgoing = channel.seal_novorudp_frame(&body)?;
            ensure!(outgoing.sequence == 2);
            stream.write_frame(&write_business(&outgoing)?).await?;
            done_rx.await?;
            Ok(())
        })
    });
    let client = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            let mut stream: CarrierStreamV1<'_> = scope.connect(&addr_rx.await?).await?.into();
            let deadline = Instant::now()
                + if matches!(case, Case::SelectorStall) {
                    Duration::from_millis(150)
                } else {
                    Duration::from_secs(3)
                };
            let at = Instant::now();
            let clock = if matches!(case, Case::LocalUnavailable) {
                None
            } else {
                Some(ClockAnchorV1::from_parts(base.at(at)?, at))
            };
            let pending = request_with_anchor(&mut stream, deadline, clock).await;
            if matches!(case, Case::SelectorStall) {
                assert_deadline(pending);
                done_tx.send(()).unwrap();
                return Ok(());
            }
            let mut pending = pending?;
            let mut channel = authenticate_structured_bound_session_v2_observed(
                &mut stream,
                &key(1),
                public(2),
                true,
                pending.deadline(),
                &mut HandshakeReplayCacheV1::new(8),
                &mut |_, _| Ok(()),
            )
            .await?;
            if matches!(case, Case::SampleStall) {
                ready_rx.await?;
                // Shorten the fixture's remaining original budget; never extend it.
                pending.deadline =
                    SessionClockDeadlineV2::new(Instant::now() + Duration::from_millis(150))?;
            }
            let sample = receive_peer_time_v8(&mut stream, &mut channel, pending, public(2)).await;
            if matches!(case, Case::SampleStall) {
                assert_deadline(sample);
                done_tx.send(()).unwrap();
                return Ok(());
            }
            if matches!(case, Case::Revoked) {
                ensure!(sample.is_err());
                done_tx.send(()).unwrap();
                return Ok(());
            }
            let sample = sample?;
            ensure!(sample.peer_identity_public_key == public(2));
            match (case, sample.observation) {
                (Case::Offset(offset), PeerTimeObservationV1::Candidate(sample)) => {
                    let (lo, hi) = sample.offset_bounds_ms();
                    ensure!(
                        lo <= i128::from(offset) && hi >= i128::from(offset),
                        "missing offset {offset}: {lo}..{hi}"
                    );
                }
                (Case::PeerUnavailable, PeerTimeObservationV1::PeerClockUnavailable)
                | (Case::LocalUnavailable, PeerTimeObservationV1::LocalClockUnavailable)
                | (Case::InvalidNumeric, PeerTimeObservationV1::InvalidMeasurement) => {}
                _ => anyhow::bail!("incorrect observation outcome"),
            }
            let frame = NovoRudpTransportFrameV0::new(
                NovoRudpTransportFrameKindV0::Data,
                channel.session_id(),
                7,
                9,
                1,
                0,
                b"same channel after clock candidate".to_vec(),
            );
            let wire = channel.seal_novorudp_frame(&frame)?;
            ensure!(wire.sequence == 1);
            stream.write_frame(&write_business(&wire)?).await?;
            let echoed = read_business(&stream.read_frame().await?)?;
            ensure!(echoed.sequence == 2 && channel.open_novorudp_frame(&echoed)? == frame);
            done_tx.send(()).unwrap();
            Ok(())
        })
    });
    let (a, b) = tokio::time::timeout(Duration::from_secs(6), async {
        tokio::join!(server, client)
    })
    .await?;
    a?;
    b?;
    Ok(())
}
#[tokio::test]
async fn large_positive_and_negative_clock_offsets_do_not_block_real_quic_identity() -> Result<()> {
    run(Case::Offset(36 * 3_600_000)).await?;
    run(Case::Offset(-36 * 3_600_000)).await
}
#[tokio::test]
async fn unavailable_or_invalid_clock_candidate_does_not_destroy_authenticated_session(
) -> Result<()> {
    for case in [
        Case::PeerUnavailable,
        Case::LocalUnavailable,
        Case::InvalidNumeric,
    ] {
        run(case).await?;
    }
    Ok(())
}
#[tokio::test]
async fn selector_and_time_reply_stalls_obey_original_operation_deadline() -> Result<()> {
    run(Case::SelectorStall).await?;
    run(Case::SampleStall).await
}
#[tokio::test]
async fn revoked_owner_cannot_send_time_or_continue_business() -> Result<()> {
    run(Case::Revoked).await
}

fn write_business(e: &SecureNovoRudpEnvelopeV1) -> Result<Vec<u8>> {
    binary::encode(
        &crate::product_overlay::SecureNovoRudpEnvelopeV1 {
            version: e.version,
            session_id: e.session_id,
            sender_peer_id: e.sender_peer_id.clone(),
            recipient_peer_id: e.recipient_peer_id.clone(),
            sequence: e.sequence,
            nonce: e.nonce,
            ciphertext: e.ciphertext.clone(),
        },
        1200,
    )
}
fn read_business(wire: &[u8]) -> Result<SecureNovoRudpEnvelopeV1> {
    let e = binary::decode(wire, 1200)?;
    Ok(SecureNovoRudpEnvelopeV1 {
        version: e.version,
        session_id: e.session_id,
        sender_peer_id: e.sender_peer_id,
        recipient_peer_id: e.recipient_peer_id,
        sequence: e.sequence,
        nonce: e.nonce,
        ciphertext: e.ciphertext,
    })
}
