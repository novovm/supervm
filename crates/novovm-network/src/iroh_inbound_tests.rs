// Included in iroh_transport::tests. These loopback fixtures exercise the
// existing accept path; observations do not alter authority or retry policy.

#[derive(Default)]
struct InboundDiagnosticCapture {
    events: std::sync::Mutex<Vec<IrohInboundObservationV1>>,
    changed: tokio::sync::Notify,
}

impl InboundDiagnosticCapture {
    fn has(&self, phase: IrohInboundPhaseV1, outcome: IrohInboundOutcomeV1) -> bool {
        self.events
            .lock()
            .unwrap()
            .iter()
            .any(|event| event.phase == phase && event.outcome == outcome)
    }

    async fn wait(&self, phase: IrohInboundPhaseV1, outcome: IrohInboundOutcomeV1) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let notified = self.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self.has(phase, outcome) {
                    return;
                }
                notified.await;
            }
        })
        .await
        .context("inbound observation was not emitted")?;
        Ok(())
    }

    fn assert_terminal(&self, phase: IrohInboundPhaseV1, outcome: IrohInboundOutcomeV1) {
        use IrohInboundOutcomeV1::{Ready, Started};
        let events = self.events.lock().unwrap();
        let terminal: Vec<_> = events
            .iter()
            .filter(|event| {
                event.outcome != Started
                    && !(event.phase == IrohInboundPhaseV1::Tls && event.outcome == Ready)
            })
            .collect();
        assert_eq!(
            terminal.len(),
            1,
            "one attempt must have one terminal observation"
        );
        assert_eq!(terminal[0].phase, phase);
        assert_eq!(terminal[0].outcome, outcome);
        assert!(events
            .iter()
            .all(|event| event.attempt == terminal[0].attempt));
        assert!(events
            .windows(2)
            .all(|pair| pair[0].elapsed_ms <= pair[1].elapsed_ms));
    }
}

fn inbound_diagnostic_config() -> (IrohConfigV1, Arc<InboundDiagnosticCapture>) {
    let capture = Arc::new(InboundDiagnosticCapture::default());
    let observe = capture.clone();
    let config = local_config().with_inbound_observer(move |event| {
        observe.events.lock().unwrap().push(event);
        observe.changed.notify_one();
    });
    (config, capture)
}

fn inbound_assert_released(scope: &IrohScopeV1) {
    assert_eq!(
        scope.connections.available_permits(),
        IROH_MAX_CONNECTIONS_V1
    );
    assert!(scope.accept_consumer.try_lock().is_ok());
}

#[tokio::test(flavor = "current_thread")]
async fn inbound_diagnostic_stream_deadline_has_real_post_tls_close() {
    let _permit = SERIAL.acquire().await.unwrap();
    let (config, capture) = inbound_diagnostic_config();
    let server_capture = capture.clone();
    let (address_tx, address_rx) = oneshot::channel();
    let (closed_tx, closed_rx) = oneshot::channel();
    let server = with_iroh_scope_v1(config, control(), move |scope| {
        Box::pin(async move {
            address_tx.send(scope.address()?).unwrap();
            let error = scope
                .accept_inbound()
                .await
                .err()
                .context("empty peer accepted")?;
            ensure!(error.to_string().contains("incoming stream setup deadline"));
            server_capture
                .assert_terminal(IrohInboundPhaseV1::Stream, IrohInboundOutcomeV1::Deadline);
            assert!(server_capture.has(IrohInboundPhaseV1::Tls, IrohInboundOutcomeV1::Ready));
            inbound_assert_released(scope);
            // Keep the endpoint alive until the peer observes this setup close,
            // so whole-scope shutdown cannot replace the cause being tested.
            closed_rx.await?;
            Ok(())
        })
    });
    let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
        Box::pin(async move {
            let peer = scope.peer_address(&address_rx.await?)?;
            let connection = scope.endpoint.connect(peer, IROH_ALPN_V1).await?;
            // TLS has completed; no stream is ever opened by this fixture.
            let error = connection.closed().await;
            ensure!(
                matches!(&error, iroh::endpoint::ConnectionError::ApplicationClosed(close)
                    if close.error_code == 1u32.into()),
                "expected post-TLS application close, got {error:?}"
            );
            assert!(iroh_protocol_diagnostic_v1(&error.into()).is_none());
            closed_tx.send(()).unwrap();
            Ok(())
        })
    });
    let (server, client) = tokio::join!(server, client);
    server.unwrap();
    client.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn inbound_diagnostic_cancelled_stream_setup_releases_real_connection() {
    let _permit = SERIAL.acquire().await.unwrap();
    let (config, capture) = inbound_diagnostic_config();
    let (address_tx, address_rx) = oneshot::channel();
    let (closed_tx, closed_rx) = oneshot::channel();
    let server = with_iroh_scope_v1(config, control(), move |scope| {
        Box::pin(async move {
            address_tx.send(scope.address()?).unwrap();
            let mut accepting = Box::pin(scope.accept_inbound());
            tokio::select! {
                result = &mut accepting => {
                    let _ = result;
                    bail!("stream setup ended before cancellation");
                }
                result = capture.wait(IrohInboundPhaseV1::Stream, IrohInboundOutcomeV1::Started) => result?,
            }
            drop(accepting);
            capture.assert_terminal(IrohInboundPhaseV1::Stream, IrohInboundOutcomeV1::Cancelled);
            inbound_assert_released(scope);
            closed_rx.await?;
            Ok(())
        })
    });
    let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
        Box::pin(async move {
            let peer = scope.peer_address(&address_rx.await?)?;
            let connection = scope.endpoint.connect(peer, IROH_ALPN_V1).await?;
            let error = connection.closed().await;
            ensure!(
                matches!(&error, iroh::endpoint::ConnectionError::ApplicationClosed(close)
                    if close.error_code == 1u32.into()),
                "cancelled accept did not close its connection: {error:?}"
            );
            closed_tx.send(()).unwrap();
            Ok(())
        })
    });
    let (server, client) = tokio::join!(server, client);
    server.unwrap();
    client.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn inbound_diagnostic_wrong_pin_is_not_a_setup_deadline() {
    let _permit = SERIAL.acquire().await.unwrap();
    let (config, capture) = inbound_diagnostic_config();
    let (address_tx, address_rx) = oneshot::channel();
    let (closed_tx, closed_rx) = oneshot::channel();
    let server = with_iroh_scope_v1(config, control(), move |scope| {
        Box::pin(async move {
            address_tx.send(scope.address()?).unwrap();
            let wrong_pin = *iroh::SecretKey::generate().public().as_bytes();
            let mut accepting = Box::pin(scope.accept(wrong_pin));
            tokio::select! {
                result = &mut accepting => {
                    let _ = result;
                    bail!("wrong peer completed the pinned accept");
                }
                result = capture.wait(IrohInboundPhaseV1::Tls, IrohInboundOutcomeV1::PeerRejected) => result?,
            }
            drop(accepting);
            capture.assert_terminal(IrohInboundPhaseV1::Tls, IrohInboundOutcomeV1::PeerRejected);
            inbound_assert_released(scope);
            closed_rx.await?;
            Ok(())
        })
    });
    let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
        Box::pin(async move {
            let peer = scope.peer_address(&address_rx.await?)?;
            let error: anyhow::Error = match scope.endpoint.connect(peer, IROH_ALPN_V1).await {
                Ok(connection) => connection.closed().await.into(),
                Err(error) => error.into(),
            };
            // A pin rejection can reach a peer before or after its TLS future
            // completes. Assert the real close category without pretending all
            // application closes are transport APPLICATION_ERROR (12).
            ensure!(
                iroh_connection_failure_kind_v1(&error)
                    == Some(IrohConnectionFailureKindV1::RemoteApplicationClose)
                    || iroh_protocol_diagnostic_v1(&error)
                        == Some(IrohProtocolDiagnosticV1 {
                            origin: 3,
                            code: 12
                        }),
                "unexpected peer rejection: {error:#}"
            );
            assert!(!error.is::<IrohConnectUnavailableV1>());
            closed_tx.send(()).unwrap();
            Ok(())
        })
    });
    let (server, client) = tokio::join!(server, client);
    server.unwrap();
    client.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn inbound_diagnostic_wrong_alpn_reports_real_tls_failure() {
    let _permit = SERIAL.acquire().await.unwrap();
    let (config, capture) = inbound_diagnostic_config();
    let (address_tx, address_rx) = oneshot::channel();
    let (rejected_tx, rejected_rx) = oneshot::channel();
    let (client_done_tx, client_done_rx) = oneshot::channel();
    let server = with_iroh_scope_v1(config, control(), move |scope| {
        Box::pin(async move {
            address_tx.send(scope.address()?).unwrap();
            let mut accepting = Box::pin(scope.accept_inbound());
            tokio::select! {
                result = &mut accepting => {
                    let _ = result;
                    bail!("wrong ALPN completed the inbound accept");
                }
                result = capture.wait(IrohInboundPhaseV1::Tls, IrohInboundOutcomeV1::Failed) => result?,
            }
            drop(accepting);
            capture.assert_terminal(IrohInboundPhaseV1::Tls, IrohInboundOutcomeV1::Failed);
            assert!(!capture.has(IrohInboundPhaseV1::Stream, IrohInboundOutcomeV1::Started));
            inbound_assert_released(scope);
            rejected_tx.send(()).unwrap();
            // Hold the server endpoint until the client has dropped its dial;
            // scope shutdown must not create the failure being asserted.
            client_done_rx.await?;
            Ok(())
        })
    });
    let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
        Box::pin(async move {
            let peer = scope.peer_address(&address_rx.await?)?;
            let mut connecting =
                Box::pin(scope.endpoint.connect(peer, b"novovm-wrong-alpn-fixture"));
            let mut rejected_rx = rejected_rx;
            tokio::select! {
                biased;
                result = &mut connecting => {
                    ensure!(result.is_err(), "server accepted an unsupported ALPN");
                    (&mut rejected_rx).await?;
                }
                result = &mut rejected_rx => {
                    result?;
                    // The actual server handshake has already failed and never
                    // opened a stream. Iroh may retain a pending outgoing path;
                    // do not wait for its unrelated dial timeout to prove the
                    // server observation. The biased branch still rejects an
                    // unexpectedly ready successful connection first.
                }
            }
            drop(connecting);
            client_done_tx.send(()).unwrap();
            Ok(())
        })
    });
    let (server, client) = tokio::join!(server, client);
    server.unwrap();
    client.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn inbound_diagnostic_pre_handshake_close_is_remote_transport_code_twelve() {
    let _permit = SERIAL.acquire().await.unwrap();
    let (address_tx, address_rx) = oneshot::channel();
    let (closed_tx, closed_rx) = oneshot::channel();
    let server = with_iroh_scope_v1(local_config(), control(), move |scope| {
        Box::pin(async move {
            address_tx.send(scope.address()?).unwrap();
            let incoming = scope
                .endpoint
                .accept()
                .await
                .context("missing incoming peer")?;
            // TEST ONLY: obtain a raw pre-handshake handle and close it without
            // yielding or opening a stream. No 0-RTT business data is accepted;
            // production remains on its fully authenticated existing path.
            let early = incoming.accept()?.into_0rtt();
            early.close(1u32.into(), b"fixture closes before TLS completion");
            closed_rx.await?;
            drop(early);
            Ok(())
        })
    });
    let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
        Box::pin(async move {
            let error = scope
                .connect(&address_rx.await?)
                .await
                .err()
                .context("pre-handshake close unexpectedly connected")?;
            assert_eq!(
                iroh_protocol_diagnostic_v1(&error),
                Some(IrohProtocolDiagnosticV1 {
                    origin: 3,
                    code: 12
                })
            );
            assert_eq!(
                iroh_connection_failure_kind_v1(&error),
                Some(IrohConnectionFailureKindV1::Protocol)
            );
            assert!(!error.is::<IrohConnectUnavailableV1>());
            closed_tx.send(()).unwrap();
            Ok(())
        })
    });
    let (server, client) = tokio::join!(server, client);
    server.unwrap();
    client.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn inbound_diagnostic_peer_close_during_stream_setup_is_failed_not_deadline() {
    let _permit = SERIAL.acquire().await.unwrap();
    let (config, capture) = inbound_diagnostic_config();
    let client_capture = capture.clone();
    let (address_tx, address_rx) = oneshot::channel();
    let (observed_tx, observed_rx) = oneshot::channel();
    let server = with_iroh_scope_v1(config, control(), move |scope| {
        Box::pin(async move {
            address_tx.send(scope.address()?).unwrap();
            let error = scope
                .accept_inbound()
                .await
                .err()
                .context("closed peer accepted")?;
            capture.assert_terminal(IrohInboundPhaseV1::Stream, IrohInboundOutcomeV1::Failed);
            ensure!(
                matches!(error.downcast_ref::<iroh::endpoint::ConnectionError>(),
                    Some(iroh::endpoint::ConnectionError::ApplicationClosed(close))
                        if close.error_code == 7u32.into()),
                "original peer close was replaced: {error:#}"
            );
            assert!(iroh_protocol_diagnostic_v1(&error).is_none());
            assert!(!error.is::<IrohConnectUnavailableV1>());
            inbound_assert_released(scope);
            observed_tx.send(()).unwrap();
            Ok(())
        })
    });
    let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
        Box::pin(async move {
            let peer = scope.peer_address(&address_rx.await?)?;
            let connection = scope.endpoint.connect(peer, IROH_ALPN_V1).await?;
            client_capture
                .wait(IrohInboundPhaseV1::Stream, IrohInboundOutcomeV1::Started)
                .await?;
            connection.close(7u32.into(), b"fixture closes before first stream");
            observed_rx.await?;
            Ok(())
        })
    });
    let (server, client) = tokio::join!(server, client);
    server.unwrap();
    client.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn inbound_diagnostic_persistent_owner_keeps_callback_across_listener_operations() {
    let _permit = SERIAL.acquire().await.unwrap();
    let (config, capture) = inbound_diagnostic_config();
    let server = IrohEndpointOwnerV1::bind(
        config
            .with_owned_service_identity(zeroize::Zeroizing::new([43; 32]))
            .unwrap(),
        || Ok(()),
    )
    .await
    .unwrap();
    let client = IrohEndpointOwnerV1::bind(local_config(), || Ok(()))
        .await
        .unwrap();
    let before = [server.snapshot(), client.snapshot()];
    let ports = [
        server.endpoint.bound_sockets()[0],
        client.endpoint.bound_sockets()[0],
    ];

    for round in 0u8..2 {
        let (address_tx, address_rx) = oneshot::channel();
        // Exercise the same stable service-owner path used by an L4 host, not
        // just a temporary scope. Each listener scope still admits fresh peers.
        let listener = server.with_owned_listener_v1(
            || Ok(()),
            move |scope| {
                Box::pin(async move {
                    address_tx.send(scope.address()?).unwrap();
                    let mut stream = scope.accept_inbound().await?;
                    ensure!(stream.read_frame().await? == [round, 0x4d], "request");
                    stream.write_frame(&[round, 0x42]).await?;
                    stream.finish().await?;
                    drop(stream);
                    inbound_assert_released(scope);
                    Ok(())
                })
            },
        );
        let connector = client.with_operation(control(), move |scope| {
            Box::pin(async move {
                let mut stream = scope.connect(&address_rx.await?).await?;
                stream.write_frame(&[round, 0x4d]).await?;
                ensure!(stream.read_frame().await? == [round, 0x42], "response");
                stream.finish().await
            })
        });
        let (accepted, connected) = tokio::time::timeout(Duration::from_secs(8), async {
            tokio::join!(listener, connector)
        })
        .await
        .expect("persistent owner exchange timed out");
        accepted.unwrap();
        connected.unwrap();

        for (index, owner) in [&server, &client].into_iter().enumerate() {
            assert_eq!(owner.snapshot().instance, before[index].instance);
            assert_eq!(
                owner.snapshot().binds_completed,
                before[index].binds_completed
            );
            assert_eq!(owner.endpoint.bound_sockets()[0], ports[index]);
            assert_eq!(owner.snapshot().active_operations, 0);
            assert_eq!(owner.snapshot().operations_started, u64::from(round) + 1);
            assert_eq!(
                owner.connections.available_permits(),
                IROH_MAX_CONNECTIONS_V1
            );
            assert!(owner.accept_consumer.try_lock().is_ok());
            assert!(!owner.snapshot().closing);
        }
    }

    {
        use IrohInboundOutcomeV1::{Ready, Started};
        use IrohInboundPhaseV1::{Stream, Tls};
        let events = capture.events.lock().unwrap();
        assert_eq!(
            events.len(),
            8,
            "both scopes must reach the retained callback"
        );
        assert_ne!(events[0].attempt, events[4].attempt);
        for attempt in events.chunks_exact(4) {
            assert!(attempt[0].attempt > 0);
            assert!(attempt
                .iter()
                .all(|event| event.attempt == attempt[0].attempt));
            assert!(attempt
                .windows(2)
                .all(|pair| pair[0].elapsed_ms <= pair[1].elapsed_ms));
            assert_eq!(
                attempt
                    .iter()
                    .map(|event| (event.phase, event.outcome))
                    .collect::<Vec<_>>(),
                [
                    (Tls, Started),
                    (Tls, Ready),
                    (Stream, Started),
                    (Stream, Ready)
                ]
            );
        }
    }
    let (server_closed, client_closed) = tokio::join!(server.close(), client.close());
    server_closed.unwrap();
    client_closed.unwrap();
    for port in ports {
        assert!(std::net::UdpSocket::bind(port).is_ok());
    }
}
