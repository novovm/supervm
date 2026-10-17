//! Real localhost QUIC checks for the explicit V2 entry. These tests do not
//! alter either device's system clock or establish trusted UTC.
use super::*;
use crate::bound_session::session_clock::{
    authenticate_structured_bound_session_v2_observed,
    authenticate_structured_service_client_v2_observed,
};
use crate::transport_binding::TransportBindingErrorV1;
use std::sync::Arc;

#[derive(Clone, Copy)]
enum Case {
    Native { open: bool },
    Mixed { server_v2: bool },
    InitialReadStall,
    FinalObserverDeadline,
    FinalObserverRevocation,
}

fn assert_local_deadline(result: Result<BoundSecureChannelV1, BoundSessionError>) {
    let error = result.err().expect("expired handshake exposed a channel");
    let BoundSessionError::Local(cause) = error else {
        panic!("local deadline was misclassified: {error}");
    };
    assert!(matches!(
        cause.downcast_ref::<TransportBindingErrorV1>(),
        Some(TransportBindingErrorV1::HandshakeDeadline)
    ));
}

// Use the original pump, endpoint and object link after Finished. The test's
// small object traverses real encrypted QUIC in each direction, with native ACKs.
async fn exchange_native(
    stream: &mut CarrierStreamV1<'_>,
    channel: BoundSecureChannelV1,
    side: u8,
    done: Arc<tokio::sync::Barrier>,
) -> Result<()> {
    use crate::novorudp::{
        carrier_io::NativeCarrierPump,
        fragments::{FragmentProfile, OutgoingMessage, ReassemblyLimits},
        object_link::{NativeLaneConfig, NativeObjectLink, NativeSendLimits, NativeSendPolicy},
        object_sender::ObjectSendState,
        NovoRudpPacingProfile, NovoRudpWindowConfig,
    };
    let (endpoint, mut pump) = NativeCarrierPump::attach(stream, channel, vec![7])?;
    let mut link = NativeObjectLink::new(
        endpoint,
        vec![NativeLaneConfig {
            stream: 7,
            domain: [7; 32],
            receive: ReassemblyLimits {
                max_message_bytes: 4096,
                messages: 2,
                bytes: 8192,
                peer_messages: 2,
                peer_bytes: 8192,
                ttl: Duration::from_secs(2),
            },
            send: NativeSendLimits {
                messages: 2,
                bytes: 8192,
                peer_messages: 2,
                peer_bytes: 8192,
            },
        }],
        NativeSendPolicy {
            window: NovoRudpWindowConfig::default(),
            pacing: NovoRudpPacingProfile::default(),
            ttl: Duration::from_secs(2),
        },
    )?;
    let body = Arc::new(OutgoingMessage::with_profile(
        [7; 32],
        vec![side; 1024],
        4096,
        FragmentProfile::NativeUdpV2,
    )?);
    ensure!(link.try_send(0, 7, &body, Instant::now())?);
    let mut sent = false;
    let mut received = false;
    let mut barrier = None;
    let started = Instant::now();
    loop {
        ensure!(
            started.elapsed() < Duration::from_secs(2),
            "V2 native IO stalled"
        );
        let report = link.poll(0, Instant::now(), 16)?;
        ensure!(report.rejected == 0 && report.send_failures == 0 && report.receive_errors == 0);
        for completion in report.completed {
            ensure!(completion.state == ObjectSendState::Delivered);
            ensure!(!sent, "duplicate native send completion");
            sent = true;
        }
        if let Some(body) = link.poll_complete(7, Instant::now(), 16)? {
            ensure!(!received && body.chunks.concat() == vec![3 - side; 1024]);
            received = true;
        }
        if sent && received && barrier.is_none() {
            barrier = Some(Box::pin(done.wait()));
        }
        tokio::select! {
            biased;
            _ = async {
                match &mut barrier {
                    Some(wait) => { wait.await; }
                    None => std::future::pending::<()>().await,
                }
            } => return Ok(()),
            result = pump.progress() => result?,
            _ = tokio::time::sleep(Duration::from_millis(1)) => {}
        }
    }
}

async fn run_case(case: Case) {
    let _permit = SERIAL.acquire().await.unwrap();
    let (server_tx, server_rx) = oneshot::channel();
    let (client_tx, client_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let (observed_tx, observed_rx) = oneshot::channel();
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let server_barrier = barrier.clone();
    let server = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            server_tx.send(scope.address()?).unwrap();
            let peer: IrohAddressV1 = client_rx.await?;
            let original_deadline = Instant::now() + Duration::from_secs(3);
            let mut stream: CarrierStreamV1<'_> = scope.accept(peer.endpoint_key).await?.into();
            let mut replay = HandshakeReplayCacheV1::new(8);
            match case {
                Case::Native { open } => {
                    let channel = if open {
                        let (channel, claimed) =
                            authenticate_structured_service_client_v2_observed(
                                &mut stream,
                                &key(2),
                                ServicePeerPolicyV1::AnyAuthenticated,
                                original_deadline,
                                &mut replay,
                                &mut |_, _| Ok(()),
                            )
                            .await?;
                        ensure!(claimed == public(1));
                        channel
                    } else {
                        authenticate_structured_bound_session_v2_observed(
                            &mut stream,
                            &key(2),
                            public(1),
                            false,
                            original_deadline,
                            &mut replay,
                            &mut |_, _| Ok(()),
                        )
                        .await?
                    };
                    exchange_native(&mut stream, channel, 2, server_barrier).await?;
                }
                Case::Mixed { server_v2 } => {
                    let mut stages = Vec::new();
                    let mut observer = |stage, _| {
                        stages.push(stage);
                        Ok(())
                    };
                    let result = if server_v2 {
                        authenticate_structured_bound_session_v2_observed(
                            &mut stream,
                            &key(2),
                            public(1),
                            false,
                            original_deadline,
                            &mut replay,
                            &mut observer,
                        )
                        .await
                    } else {
                        authenticate_structured_bound_session_observed(
                            &mut stream,
                            &key(2),
                            public(1),
                            false,
                            &mut replay,
                            &mut observer,
                        )
                        .await
                    };
                    ensure!(matches!(result, Err(BoundSessionError::Authentication(_))));
                    ensure!(stages.is_empty(), "mixed schemas entered authentication");
                }
                Case::InitialReadStall => {
                    // QUIC accepts a bi-stream only after the first bytes. This
                    // fixture prelude forces that setup, then deliberately sends
                    // no identity offer. The deadline precedes the prelude IO.
                    let deadline = Instant::now() + Duration::from_millis(200);
                    ensure!(stream.read_frame().await? == b"fixture stream setup");
                    stream.write_frame(b"fixture ready").await?;
                    let mut stages = Vec::new();
                    let result = authenticate_structured_service_client_v2_observed(
                        &mut stream,
                        &key(2),
                        ServicePeerPolicyV1::AnyAuthenticated,
                        deadline,
                        &mut replay,
                        &mut |stage, _| {
                            stages.push(stage);
                            Ok(())
                        },
                    )
                    .await
                    .map(|(channel, _)| channel);
                    assert_local_deadline(result);
                    ensure!(stages.is_empty());
                }
                Case::FinalObserverDeadline | Case::FinalObserverRevocation => {
                    let deadline = Instant::now() + Duration::from_millis(250);
                    let mut reached_final = false;
                    let result = authenticate_structured_service_client_v2_observed(
                        &mut stream,
                        &key(2),
                        ServicePeerPolicyV1::AnyAuthenticated,
                        deadline,
                        &mut replay,
                        &mut |stage, _| {
                            if stage == BoundSessionStage::FinishedVerified {
                                reached_final = true;
                                if matches!(case, Case::FinalObserverDeadline) {
                                    // A synchronous observer cannot be preempted
                                    // by timeout_at; the return boundary must recheck.
                                    std::thread::sleep(
                                        deadline.saturating_duration_since(Instant::now())
                                            + Duration::from_millis(2),
                                    );
                                } else {
                                    anyhow::bail!("fixture owner revoked");
                                }
                            }
                            Ok(())
                        },
                    )
                    .await
                    .map(|(channel, _)| channel);
                    ensure!(reached_final, "fixture did not reach the final observer");
                    if matches!(case, Case::FinalObserverDeadline) {
                        assert_local_deadline(result);
                    } else {
                        let error = result.err().expect("revoked observer exposed a channel");
                        ensure!(matches!(error, BoundSessionError::Local(_)));
                        ensure!(format!("{error:#}").contains("fixture owner revoked"));
                    }
                }
            }
            let _ = observed_tx.send(());
            drop(stream);
            // Wait for the counterpart to observe completion/rejection before
            // closing its scope, avoiding a connection-close race in assertions.
            done_rx.await?;
            Ok(())
        })
    });
    let client = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            client_tx.send(scope.address()?).unwrap();
            let original_deadline = Instant::now() + Duration::from_secs(3);
            let mut stream: CarrierStreamV1<'_> = scope.connect(&server_rx.await?).await?.into();
            if matches!(case, Case::InitialReadStall) {
                stream.write_frame(b"fixture stream setup").await?;
                ensure!(stream.read_frame().await? == b"fixture ready");
                ensure!(
                    stream.read_frame().await.is_err(),
                    "stalled read returned an offer response"
                );
            } else {
                let result = if matches!(case, Case::Mixed { server_v2: true }) {
                    authenticate_structured_bound_session_observed(
                        &mut stream,
                        &key(1),
                        public(2),
                        true,
                        &mut HandshakeReplayCacheV1::new(8),
                        &mut |_, _| Ok(()),
                    )
                    .await
                } else {
                    authenticate_structured_bound_session_v2_observed(
                        &mut stream,
                        &key(1),
                        public(2),
                        true,
                        original_deadline,
                        &mut HandshakeReplayCacheV1::new(8),
                        &mut |_, _| Ok(()),
                    )
                    .await
                };
                if matches!(case, Case::Native { .. }) {
                    exchange_native(&mut stream, result?, 1, barrier).await?;
                } else if matches!(case, Case::Mixed { .. }) {
                    ensure!(result.is_err(), "mixed-version peer exposed a channel");
                }
                // With a failing final observer the peer may have completed its
                // own Finished. That does not authorize the rejecting owner.
            }
            if matches!(
                case,
                Case::FinalObserverDeadline | Case::FinalObserverRevocation
            ) {
                // Keep our stream alive until the server has exercised its
                // return boundary; client scope teardown must not replace the
                // deliberately injected observer failure with transport EOF.
                observed_rx.await?;
            }
            drop(stream);
            let _ = done_tx.send(());
            Ok(())
        })
    });
    // Dropping either fixture on timeout closes its original scoped resources.
    let (server, client) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(server, client)
    })
    .await
    .expect("V2 QUIC fixture stalled");
    server.unwrap();
    client.unwrap();
}

#[tokio::test]
async fn v2_pinned_quic_finished_adopts_original_native_pump() {
    run_case(Case::Native { open: false }).await;
}

#[tokio::test]
async fn v2_open_quic_finished_adopts_original_native_pump() {
    run_case(Case::Native { open: true }).await;
}

#[tokio::test]
async fn v2_quic_rejects_v1_offer_without_fallback() {
    run_case(Case::Mixed { server_v2: true }).await;
}

#[tokio::test]
async fn v1_quic_rejects_v2_offer_without_fallback() {
    run_case(Case::Mixed { server_v2: false }).await;
}

#[tokio::test]
async fn v2_responder_first_offer_read_obeys_original_deadline() {
    run_case(Case::InitialReadStall).await;
}

#[tokio::test]
async fn v2_final_observer_crossing_deadline_cannot_expose_channel() {
    run_case(Case::FinalObserverDeadline).await;
}

#[tokio::test]
async fn v2_final_observer_revocation_cannot_expose_channel() {
    run_case(Case::FinalObserverRevocation).await;
}
