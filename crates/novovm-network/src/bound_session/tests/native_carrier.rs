//! Actual localhost QUIC, structured handshake and the original native envelope.
//! Also exercises the original NativeObjectLink through scoped async carrier IO.
use super::*;
use crate::novorudp::{
    carrier::{NativeRoute, VerifiedCarrierRoute},
    datagram::{NativeUdpSession, MAX_DATAGRAM_BYTES},
};

async fn native_roundtrip(open: bool) {
    let _permit = SERIAL.acquire().await.unwrap();
    let (server_tx, server_rx) = oneshot::channel();
    let (client_tx, client_rx) = oneshot::channel();
    let server = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            server_tx.send(scope.address()?).unwrap();
            let peer: IrohAddressV1 = client_rx.await?;
            let mut stream: CarrierStreamV1<'_> = scope.accept(peer.endpoint_key).await?.into();
            let binding = stream.binding()?;
            let mut replay = HandshakeReplayCacheV1::new(8);
            let channel = if open {
                authenticate_structured_service_client_observed(
                    &mut stream,
                    &key(2),
                    ServicePeerPolicyV1::AnyAuthenticated,
                    &mut replay,
                    &mut |_, _| Ok(()),
                )
                .await?
                .0
            } else {
                authenticate_structured_bound_session_observed(
                    &mut stream,
                    &key(2),
                    public(1),
                    false,
                    &mut replay,
                    &mut |_, _| Ok(()),
                )
                .await?
            };
            let mut native = NativeUdpSession::from_bound_channel(channel, &binding)?;
            let route = NativeRoute::Carrier(VerifiedCarrierRoute::from_binding(&binding));
            let session = native.session_id();
            for sequence in 0..8 {
                let wire = stream.read_frame().await?;
                ensure!(wire.starts_with(b"NVRLY002") && wire.len() <= MAX_DATAGRAM_BYTES);
                let envelope = crate::product_overlay::binary::decode(&wire, MAX_DATAGRAM_BYTES)?;
                // Finished consumes sequence zero. Adoption must not reset it.
                ensure!(envelope.sequence == sequence + 1);
                let frame = native.open_route(route, &wire)?;
                ensure!(frame == payload(session, sequence));
                ensure!(native.open_route(route, &wire).is_err(), "replay accepted");
                stream.write_frame(&native.encode(&frame)?).await?;
            }
            stream.finish().await?;
            Ok(session)
        })
    });
    let client = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            client_tx.send(scope.address()?).unwrap();
            let mut stream: CarrierStreamV1<'_> = scope.connect(&server_rx.await?).await?.into();
            let binding = stream.binding()?;
            let channel = authenticate_structured_bound_session_observed(
                &mut stream,
                &key(1),
                public(2),
                true,
                &mut HandshakeReplayCacheV1::new(8),
                &mut |_, _| Ok(()),
            )
            .await?;
            let mut native = NativeUdpSession::from_bound_channel(channel, &binding)?;
            let route = NativeRoute::Carrier(VerifiedCarrierRoute::from_binding(&binding));
            let session = native.session_id();
            for sequence in 0..8 {
                let frame = payload(session, sequence);
                let wire = native.encode(&frame)?;
                ensure!(wire.starts_with(b"NVRLY002") && wire.len() <= MAX_DATAGRAM_BYTES);
                stream.write_frame(&wire).await?;
                let echo = stream.read_frame().await?;
                ensure!(native.open_route(route, &echo)? == frame);
            }
            stream.finish().await?;
            Ok(session)
        })
    });
    let (server, client) = tokio::join!(server, client);
    assert_eq!(server.unwrap(), client.unwrap());
}

fn payload(session: [u8; 16], sequence: u64) -> NovoRudpTransportFrameV0 {
    NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        session,
        11,
        sequence,
        sequence,
        0,
        vec![(sequence + 1) as u8; 848],
    )
}

#[tokio::test]
async fn structured_native_quic_reuses_completed_pinned_session() {
    native_roundtrip(false).await;
}

#[tokio::test]
async fn structured_native_quic_reuses_completed_open_service_session() {
    native_roundtrip(true).await;
}

// An independently signed, carrier-bound offer can fail before any business
// request when the initiator's UTC is ahead of the responder's clock. Keep the
// original policy and error visible; a closed carrier is not evidence that
// authentication succeeded or that the network is unavailable.
#[tokio::test]
async fn structured_future_clock_is_rejected_before_authentication_completes() {
    use crate::{
        iroh_transport::IrohStreamIoFailureV1, transport_binding::TransportBindingErrorV1,
    };
    let _permit = SERIAL.acquire().await.unwrap();
    let (server_tx, server_rx) = oneshot::channel();
    let (client_tx, client_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let server = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            server_tx.send(scope.address()?).unwrap();
            let peer: IrohAddressV1 = client_rx.await?;
            let mut stream: CarrierStreamV1<'_> = scope.accept(peer.endpoint_key).await?.into();
            let mut stages = Vec::new();
            let result = authenticate_structured_service_client_observed(
                &mut stream,
                &key(2),
                ServicePeerPolicyV1::AnyAuthenticated,
                &mut HandshakeReplayCacheV1::new(8),
                &mut |stage, _| {
                    stages.push(stage);
                    Ok(())
                },
            )
            .await;
            let error = result.err().expect("future-clock offer must be rejected");
            assert!(matches!(error, BoundSessionError::Authentication(_)));
            let BoundSessionError::Authentication(cause) = error else {
                unreachable!()
            };
            assert!(matches!(
                cause.downcast_ref::<TransportBindingErrorV1>(),
                Some(TransportBindingErrorV1::Expired)
            ));
            assert!(
                stages.is_empty(),
                "future offer must not produce a completed authentication step"
            );
            drop(stream); // Same original stream teardown as a rejected node operation.
            done_rx.await?;
            Ok(())
        })
    });
    let client = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            client_tx.send(scope.address()?).unwrap();
            let mut stream: CarrierStreamV1<'_> = scope.connect(&server_rx.await?).await?.into();
            // Eight seconds leaves margin for local fixture scheduling while
            // remaining within the original 30-second signed handshake TTL.
            let start = BoundHandshakeInitiatorV1::start(
                &key(1),
                public(2),
                stream.binding()?,
                now_ms()? + 8_000,
                BOUND_HANDSHAKE_MAX_TTL_MS_V1,
            )?;
            stream
                .write_frame(&crate::transport_binding::structured::encode_offer(
                    start.offer(),
                )?)
                .await?;
            let error = stream
                .read_frame()
                .await
                .expect_err("rejected offer cannot return a response");
            // The reader can observe stream EOF before the QUIC close frame;
            // do not equate a particular client close code with the server's
            // verified authentication reason. No response/channel was exposed.
            assert!(error.is::<IrohStreamIoFailureV1>(), "{error:#}");
            let _ = done_tx.send(());
            Ok(())
        })
    });
    let (server, client) = tokio::join!(server, client);
    server.unwrap();
    client.unwrap();
}

// Multiple native lanes send simultaneously in both directions. Cancellation
// of a select branch must preserve the pump's partially polled frame futures.
async fn exchange_objects(
    stream: &mut CarrierStreamV1<'_>,
    channel: BoundSecureChannelV1,
    value: u8,
    done: std::sync::Arc<tokio::sync::Barrier>,
    continuous: bool,
) -> Result<()> {
    use crate::novorudp::{
        carrier_io::NativeCarrierPump,
        fragments::{FragmentProfile, OutgoingMessage, ReassemblyLimits},
        object_link::{NativeLaneConfig, NativeObjectLink, NativeSendLimits, NativeSendPolicy},
        object_sender::ObjectSendState,
        NovoRudpPacingProfile, NovoRudpWindowConfig,
    };
    use std::sync::Arc;
    let (mut endpoint, mut pump) = if continuous {
        NativeCarrierPump::attach_continuous_v7(stream, channel, vec![1, 2])?
    } else {
        ensure!(
            stream.split_native_continuous_v7().is_err(),
            "finite scope upgraded"
        );
        NativeCarrierPump::attach(stream, channel, vec![1, 2])?
    };
    endpoint.set_wire_budget_v1(1_000_000, Instant::now())?;
    let lanes = [1, 2]
        .into_iter()
        .map(|stream| NativeLaneConfig {
            stream,
            domain: [stream as u8; 32],
            receive: ReassemblyLimits {
                max_message_bytes: 8192,
                messages: 2,
                bytes: 16384,
                peer_messages: 2,
                peer_bytes: 16384,
                ttl: Duration::from_secs(5),
            },
            send: NativeSendLimits {
                messages: 2,
                bytes: 16384,
                peer_messages: 2,
                peer_bytes: 16384,
            },
        })
        .collect();
    let policy = NativeSendPolicy {
        window: NovoRudpWindowConfig {
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 16,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            no_progress_backoff: false,
            ..Default::default()
        },
        pacing: NovoRudpPacingProfile {
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 16,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            no_progress_backoff: false,
        },
        ttl: Duration::from_secs(5),
    };
    let mut link = NativeObjectLink::new(endpoint, lanes, policy)?;
    // The second wave proves the same scoped owner can continue after earlier
    // native objects and their transport ACKs have completed.
    for wave in 0..2u8 {
        if continuous && wave == 1 {
            // Actual time beyond the old initial30s window; retain idle reader
            // across progress/select cancellation without extending a request.
            let idle = tokio::time::sleep(Duration::from_secs(31));
            tokio::pin!(idle);
            loop {
                tokio::select! {
                    biased;
                    _ = &mut idle => break,
                    result = pump.progress() => result?,
                }
            }
        }
        let start = Instant::now();
        for lane in [1, 2] {
            let body = Arc::new(OutgoingMessage::with_profile(
                [lane as u8; 32],
                vec![value + wave; 3500 + lane as usize],
                8192,
                FragmentProfile::NativeUdpV2,
            )?);
            ensure!(link.try_send(0, lane, &body, Instant::now())?);
        }
        let mut sent = 0;
        let mut received = std::collections::BTreeSet::new();
        let mut ready = None;
        loop {
            ensure!(start.elapsed() < Duration::from_secs(5),
                "native object carrier stalled: value={value} wave={wave} sent={sent} received={received:?}");
            let report = link.poll(
                1000 + start.elapsed().as_millis() as u64,
                Instant::now(),
                16,
            )?;
            ensure!(
                report.rejected == 0 && report.send_failures == 0 && report.receive_errors == 0
            );
            for completion in report.completed {
                ensure!(completion.state == ObjectSendState::Delivered);
                sent += 1;
            }
            for lane in [1, 2] {
                if let Some(body) = link.poll_complete(lane, Instant::now(), 16)? {
                    ensure!(received.insert(lane), "duplicate object presentation");
                    ensure!(body.chunks.concat() == vec![3 - value + wave; 3500 + lane as usize]);
                }
            }
            if sent == 2 && received.len() == 2 && ready.is_none() {
                ready = Some(Box::pin(done.wait()));
            }
            tokio::select! {
                biased;
                _ = async { match &mut ready {
                    Some(wait) => { wait.await; },
                    None => std::future::pending::<()>().await,
                }} => break,
                result = pump.progress() => result?,
                _ = tokio::time::sleep(Duration::from_millis(1)) => {},
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn structured_native_object_link_duplex_two_waves_on_one_carrier() {
    let _permit = SERIAL.acquire().await.unwrap();
    let (server_tx, server_rx) = oneshot::channel();
    let (client_tx, client_rx) = oneshot::channel();
    let done = std::sync::Arc::new(tokio::sync::Barrier::new(2));
    let server_done = done.clone();
    let server = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            server_tx.send(scope.address()?).unwrap();
            let peer: IrohAddressV1 = client_rx.await?;
            let mut stream: CarrierStreamV1<'_> = scope.accept(peer.endpoint_key).await?.into();
            let channel = authenticate_structured_service_client_observed(
                &mut stream,
                &key(2),
                ServicePeerPolicyV1::AnyAuthenticated,
                &mut HandshakeReplayCacheV1::new(8),
                &mut |_, _| Ok(()),
            )
            .await?
            .0;
            exchange_objects(&mut stream, channel, 2, server_done, false).await
        })
    });
    let client = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            client_tx.send(scope.address()?).unwrap();
            let mut stream: CarrierStreamV1<'_> = scope.connect(&server_rx.await?).await?.into();
            let channel = authenticate_structured_bound_session_observed(
                &mut stream,
                &key(1),
                public(2),
                true,
                &mut HandshakeReplayCacheV1::new(8),
                &mut |_, _| Ok(()),
            )
            .await?;
            exchange_objects(&mut stream, channel, 1, done, false).await
        })
    });
    let (server, client) = tokio::join!(server, client);
    server.unwrap();
    client.unwrap();
}

#[tokio::test]
async fn structured_native_continuous_object_link_survives_idle_initial_deadline() -> Result<()> {
    use crate::iroh_transport::IrohEndpointOwnerV1;
    let _permit = SERIAL.acquire().await?;
    let (server_tx, server_rx) = oneshot::channel();
    let (client_tx, client_rx) = oneshot::channel();
    let done = std::sync::Arc::new(tokio::sync::Barrier::new(2));
    let server_done = done.clone();
    let server_owner = IrohEndpointOwnerV1::bind(config(), || Ok(())).await?;
    let client_owner = IrohEndpointOwnerV1::bind(config(), || Ok(())).await?;
    let server = server_owner.with_continuous_service_v4(
        || Ok(()),
        move |scope| {
            Box::pin(async move {
                server_tx.send(scope.address()?).unwrap();
                let peer: IrohAddressV1 = client_rx.await?;
                let mut stream: CarrierStreamV1<'_> = scope.accept(peer.endpoint_key).await?.into();
                let channel = authenticate_structured_service_client_observed(
                    &mut stream,
                    &key(2),
                    ServicePeerPolicyV1::AnyAuthenticated,
                    &mut HandshakeReplayCacheV1::new(8),
                    &mut |_, _| Ok(()),
                )
                .await?
                .0;
                exchange_objects(&mut stream, channel, 2, server_done, true).await
            })
        },
    );
    let client = client_owner.with_continuous_service_v4(
        || Ok(()),
        move |scope| {
            Box::pin(async move {
                client_tx.send(scope.address()?).unwrap();
                let mut stream: CarrierStreamV1<'_> =
                    scope.connect(&server_rx.await?).await?.into();
                let channel = authenticate_structured_bound_session_observed(
                    &mut stream,
                    &key(1),
                    public(2),
                    true,
                    &mut HandshakeReplayCacheV1::new(8),
                    &mut |_, _| Ok(()),
                )
                .await?;
                exchange_objects(&mut stream, channel, 1, done, true).await
            })
        },
    );
    let result = tokio::time::timeout(Duration::from_secs(40), async {
        tokio::join!(server, client)
    })
    .await;
    server_owner.close().await?;
    client_owner.close().await?;
    let (server, client) = result?;
    server?;
    client?;
    Ok(())
}
