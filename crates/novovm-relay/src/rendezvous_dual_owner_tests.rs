// Included inside the shared platform owner tests to reuse its protected
// Fixture and actual UDP Client. These are loopback owner tests, not Android or
// public NAT/relay independence evidence.
mod dual_owner_tests {
    use super::*;
    use novovm_network::{
        bound_session::peer_time::authenticate_mailbox_native_v8,
        carrier_stream::CarrierStreamV1,
        duplex::HandshakeReplayCacheV1,
        iroh_transport::IrohEndpointOwnerV1,
        novorudp::carrier_io::NativeCarrierPump,
        product_service_lease::{IrohServiceEndpointV1, NativeCarrierMailboxEndpointV8},
    };

    // Same bounded SDK fixture driver as mailbox_native_carrier.rs; no alternate
    // wire codec, storage or production scheduler is implemented here.
    struct CarrierClient<'a> {
        pump: NativeCarrierPump<'a>,
        link: NativeObjectLink,
        session: NativeMailboxClient,
    }
    impl<'a> CarrierClient<'a> {
        async fn attach(stream: &'a mut CarrierStreamV1<'_>, pin: [u8; 32]) -> Result<Self> {
            let (channel, observed) = authenticate_mailbox_native_v8(
                stream,
                &SigningKey::from_bytes(&[41; 32]),
                pin,
                Instant::now() + Duration::from_secs(5),
                &mut HandshakeReplayCacheV1::new(8),
                &mut |_, _| Ok(()),
            )
            .await?;
            ensure!(observed.peer_identity_public_key == pin);
            let (endpoint, pump) =
                NativeCarrierPump::attach_continuous_v7(stream, channel, vec![STREAM])?;
            let limits = novovm_network::mailbox_structured::BULK_LIMITS;
            let mut link = NativeObjectLink::new(
                endpoint,
                vec![NativeLaneConfig {
                    stream: STREAM,
                    domain: domain(),
                    receive: ReassemblyLimits {
                        max_message_bytes: limits.wire_bytes,
                        messages: 4,
                        bytes: limits.wire_bytes * 4,
                        peer_messages: 4,
                        peer_bytes: limits.wire_bytes * 4,
                        ttl: REQUEST_DEADLINE,
                    },
                    send: NativeSendLimits {
                        messages: 4,
                        bytes: limits.wire_bytes * 4,
                        peer_messages: 4,
                        peer_bytes: limits.wire_bytes * 4,
                    },
                }],
                NativeSendPolicy::reliable_carrier_v1(REQUEST_DEADLINE),
            )?;
            let session = NativeMailboxClient::attach(&mut link, 0, pin)?;
            Ok(Self {
                pump,
                link,
                session,
            })
        }

        async fn request(&mut self, request: Request) -> Result<Reply> {
            let id = self.session.begin(0, vec![request], Instant::now())?;
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                let at = Instant::now();
                ensure!(at < deadline, "dual owner carrier request deadline");
                self.session.progress_capacity(&mut self.link, at)?;
                if self.session.negotiated_retained_bytes().is_some() {
                    self.session.try_send(&mut self.link, id, at, || Ok(()))?;
                }
                for done in self.link.poll(now_ms()?, at, 64)?.completed {
                    ensure!(
                        self.session.transport_complete(&done).is_none(),
                        "carrier transport failed before business reply"
                    );
                }
                if let Some(body) = self.link.poll_complete(STREAM, at, 64)? {
                    if !self.session.receive_capacity(&self.link, &body, at)? {
                        let (got, mut replies) =
                            self.session.receive(&self.link, body, at, || Ok(()))?;
                        ensure!(got == id && replies.len() == 1);
                        return Ok(replies.remove(0));
                    }
                }
                tokio::select! {
                    result = self.pump.progress() => result?,
                    _ = tokio::time::sleep(Duration::from_millis(1)) => {},
                }
            }
        }
    }

    fn put(expires: i64) -> Request {
        Request::Put {
            mailbox: [1; 32],
            write: [2; 32],
            object: [4; 32],
            ciphertext: vec![8; 256],
            expires,
        }
    }
    fn fetch() -> Request {
        Request::Fetch {
            mailbox: [1; 32],
            read: [3; 32],
        }
    }
    fn check_object(reply: Reply, expires: i64) -> Result<()> {
        let Reply::Object {
            object,
            ciphertext,
            expires: actual,
            ..
        } = reply
        else {
            anyhow::bail!("dual owner retained custody missing")
        };
        ensure!(object == [4; 32] && ciphertext == vec![8; 256] && actual == expires);
        Ok(())
    }

    async fn dual_exchange(
        fixture: &Fixture,
        prior: Option<&StoreReceiptV1>,
        expires: i64,
    ) -> Result<(StoreReceiptV1, u64)> {
        let owner = fixture.open()?;
        let pin = owner.policy.node_pin;
        let direct_only = matches!(owner.policy.carrier_mode, Some(CarrierMode::DirectOnly));
        #[cfg(feature = "tls-transport")]
        let tls_enabled = owner.policy.tls_listener.is_some();
        let root = fixture.0.root.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let serving = owner.run(stop.clone());
        tokio::pin!(serving);
        let work = async {
            let deadline = Instant::now() + Duration::from_secs(8);
            let record: PeerSignedRelayRecordV1 = loop {
                ensure!(Instant::now() < deadline, "dual owner publication deadline");
                if let (Ok(bytes), Ok(status)) = (
                    std::fs::read(root.join("self-owned-record.json")),
                    std::fs::read(root.join("status.json")),
                ) {
                    let status: serde_json::Value = serde_json::from_slice(&status)?;
                    if status["phase"] == "serving" {
                        break serde_json::from_slice(&bytes)?;
                    }
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            };
            ensure!(record.relay_public_key == pin);
            ensure!(NativeMailboxEndpointV7::from_record(&record)?.is_some());
            let carrier = NativeCarrierMailboxEndpointV8::from_record(&record)?
                .context("dual owner lost V8 entry")?;
            ensure!(NativeCarrierMailboxEndpointV8::supports_bulk_v1(&record)?);
            let route = IrohServiceEndpointV1::from_record(&record)?;
            if direct_only {
                ensure!(
                    route.address.relays.is_empty(),
                    "direct-only advertised a relay"
                );
                let status: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(root.join("status.json"))?)?;
                ensure!(status["relayServicesEnabled"] == false);
            }
            let mut book =
                ServiceLeaseBookV1::new_self_owned(trust(pin), vec![record.clone()], now_ms()?)?;
            book.check_native_carrier_route_v8(pin, &carrier, now_ms()?)?;
            book.check_continuous_peer_v4(pin, route.address.endpoint_key, now_ms()?)?;
            let mut udp = Client::connect(&record).await?;
            if prior.is_some() {
                // Read before any Put on either carrier: cannot manufacture
                // recovery by re-uploading the object after a restart.
                check_object(udp.request(fetch()).await?, expires)?;
            } else {
                ensure!(matches!(
                    udp.request(Request::Register {
                        mailbox: [1; 32],
                        write: [2; 32],
                        read: [3; 32],
                        expires,
                    })
                    .await?,
                    Reply::Registered
                ));
            }
            let Reply::Stored(receipt) = udp.request(put(expires)).await? else {
                anyhow::bail!("UDP did not issue durable StoreReceipt")
            };
            receipt.verify(pin, [1; 32], [4; 32], &vec![8; 256], expires)?;
            if let Some(previous) = prior {
                ensure!(receipt.signature == previous.signature);
            }

            let endpoint = IrohEndpointOwnerV1::bind(
                IrohConfigV1::new(&[], "127.0.0.1".parse()?)?
                    .with_bind_address("127.0.0.1:0".parse()?)?,
                || Ok(()),
            )
            .await?;
            let expected = receipt.signature.clone();
            let result = endpoint
                .with_continuous_service_v4(
                    || Ok(()),
                    move |scope| {
                        Box::pin(async move {
                            let mut stream: CarrierStreamV1<'_> =
                                scope.connect(&route.address).await?.into();
                            let mut client = CarrierClient::attach(&mut stream, pin).await?;
                            check_object(client.request(fetch()).await?, expires)?;
                            let Reply::Stored(same) = client.request(put(expires)).await? else {
                                anyhow::bail!("V8 did not return the original custody receipt")
                            };
                            same.verify(pin, [1; 32], [4; 32], &vec![8; 256], expires)?;
                            ensure!(
                                same.signature == expected,
                                "carriers do not share receipt history"
                            );
                            // Prove reverse visibility too, with a second mailbox so Fetch
                            // cannot return the first object's row and hide a missing Put.
                            ensure!(matches!(
                                client
                                    .request(Request::Register {
                                        mailbox: [11; 32],
                                        write: [12; 32],
                                        read: [13; 32],
                                        expires,
                                    })
                                    .await?,
                                Reply::Registered
                            ));
                            let Reply::Stored(other) = client
                                .request(Request::Put {
                                    mailbox: [11; 32],
                                    write: [12; 32],
                                    object: [14; 32],
                                    ciphertext: vec![18; 256],
                                    expires,
                                })
                                .await?
                            else {
                                anyhow::bail!("V8 Put failed")
                            };
                            other.verify(pin, [11; 32], [14; 32], &vec![18; 256], expires)?;
                            Ok(())
                        })
                    },
                )
                .await;
            let closed = endpoint.close().await;
            result?;
            closed?;
            #[cfg(feature = "tls-transport")]
            if tls_enabled {
                use novovm_network::{
                    product_service_lease::TlsServiceEndpointV1, tls_transport::TlsScopeV1,
                };
                let tls = TlsServiceEndpointV1::from_record(&record)?
                    .context("dual owner silently dropped configured TLS entry")?;
                ensure!(tls.native_mailbox_v8);
                book.check_tls_route_v1(pin, &tls, now_ms()?)?;
                book.check_continuous_native_tls_v8(pin, &tls, now_ms()?)?;
                let scope = TlsScopeV1::new_continuous_v4(|| Ok(()))?;
                let tcp = tokio::net::TcpStream::connect(tls.address).await?;
                tcp.set_nodelay(true)?;
                let mut stream = CarrierStreamV1::from(scope.connect(tcp, tls.endpoint_key).await?);
                let mut client = CarrierClient::attach(&mut stream, pin).await?;
                check_object(client.request(fetch()).await?, expires)?;
                let Reply::Stored(tls_receipt) = client.request(put(expires)).await? else {
                    anyhow::bail!("configured TLS entry did not serve native custody")
                };
                tls_receipt.verify(pin, [1; 32], [4; 32], &vec![8; 256], expires)?;
                ensure!(tls_receipt.signature == receipt.signature);
            }
            let Reply::Object {
                object,
                ciphertext,
                expires: actual,
                ..
            } = udp
                .request(Request::Fetch {
                    mailbox: [11; 32],
                    read: [13; 32],
                })
                .await?
            else {
                anyhow::bail!("UDP could not read V8 custody")
            };
            ensure!(object == [14; 32] && ciphertext == vec![18; 256] && actual == expires);
            Ok::<_, anyhow::Error>((receipt, record.sequence))
        };
        tokio::pin!(work);
        let outcome = tokio::select! {
            biased;
            result = &mut serving => { result?; anyhow::bail!("dual owner ended before exchange") },
            result = &mut work => result,
        };
        stop.store(true, Ordering::Release);
        let stopped = tokio::time::timeout(Duration::from_secs(8), &mut serving).await;
        // Drain the original owner first, but never replace an exchange failure
        // with the final Iroh scope check after our explicit stop above.
        let outcome = outcome?;
        if let Err(error) = stopped? {
            ensure!(
                FailureEvidence::from_error(&error).stage == FailureStage::TransportRun
                    && error.root_cause().to_string() == "service stopped by owner",
                "unexpected dual owner stop: {error:#}"
            );
        }
        Ok(outcome)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn dual_owner_udp_v8_share_custody_identity_and_restart_history() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.provision()?.set_native_carrier_mailbox_v8(true)?;
        fixture.open()?.set_native_listener(Some(settings()))?;
        let keys = std::fs::read(fixture.0.root.join(KEYS_FILE))?;
        let pin = fixture.open()?.policy.node_pin;
        let expires = (now_ms()? / 1000) as i64 + 600;
        let (first, sequence) = dual_exchange(&fixture, None, expires).await?;
        let (second, next) = dual_exchange(&fixture, Some(&first), expires).await?;
        ensure!(next > sequence && second.signature == first.signature);
        ensure!(fixture.open()?.policy.node_pin == pin);
        ensure!(std::fs::read(fixture.0.root.join(KEYS_FILE))? == keys);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn dual_owner_occupied_udp_fails_before_publication_without_losing_identity() -> Result<()>
    {
        let fixture = Fixture::new()?;
        fixture.provision()?.set_native_carrier_mailbox_v8(true)?;
        let occupied = UdpSocket::bind("127.0.0.1:0")?;
        let mut listener = settings();
        listener.listen = occupied.local_addr()?;
        listener.advertised = listener.listen;
        fixture.open()?.set_native_listener(Some(listener))?;
        let keys = std::fs::read(fixture.0.root.join(KEYS_FILE))?;
        let journal = std::fs::read(fixture.0.root.join("service.journal"))?;
        let result = fixture.open()?.run(Arc::new(AtomicBool::new(false))).await;
        let error = result.expect_err("occupied UDP must not pretend to serve both carriers");
        ensure!(error.downcast_ref::<FailureStage>() == Some(&FailureStage::NativeBind));
        ensure!(
            error
                .downcast_ref::<std::io::Error>()
                .context("typed bind failure")?
                .kind()
                == std::io::ErrorKind::AddrInUse
        );
        let status: serde_json::Value =
            serde_json::from_slice(&std::fs::read(fixture.0.root.join("status.json"))?)?;
        ensure!(status["phase"] == "failed_closed");
        ensure!(!fixture.0.root.join("self-owned-record.json").exists());
        ensure!(std::fs::read(fixture.0.root.join(KEYS_FILE))? == keys);
        ensure!(std::fs::read(fixture.0.root.join("service.journal"))? == journal);
        drop(occupied);
        drop(fixture.open()?); // Failure released the existing exclusive owner lock.
        Ok(())
    }

    #[cfg(feature = "tls-transport")]
    #[tokio::test(flavor = "current_thread")]
    async fn dual_owner_keeps_configured_tls_live_with_udp_v8_after_restart() -> Result<()> {
        use novovm_network::product_service_lease::TlsServiceEndpointV1;
        let fixture = Fixture::new()?;
        fixture.provision()?.set_native_carrier_mailbox_v8(true)?;
        fixture.open()?.set_tls_listener(Some(tls_settings(true)))?;
        fixture.open()?.set_native_listener(Some(settings()))?;
        let keys = std::fs::read(fixture.0.root.join(KEYS_FILE))?;
        let expires = (now_ms()? / 1000) as i64 + 600;
        let (first, sequence) = dual_exchange(&fixture, None, expires).await?;
        let first_record: PeerSignedRelayRecordV1 = serde_json::from_slice(&std::fs::read(
            fixture.0.root.join("self-owned-record.json"),
        )?)?;
        let tls_key = TlsServiceEndpointV1::from_record(&first_record)?
            .unwrap()
            .endpoint_key;
        let (second, next) = dual_exchange(&fixture, Some(&first), expires).await?;
        let second_record: PeerSignedRelayRecordV1 = serde_json::from_slice(&std::fs::read(
            fixture.0.root.join("self-owned-record.json"),
        )?)?;
        ensure!(
            TlsServiceEndpointV1::from_record(&second_record)?
                .unwrap()
                .endpoint_key
                == tls_key
        );
        ensure!(next > sequence && second.signature == first.signature);
        ensure!(std::fs::read(fixture.0.root.join(KEYS_FILE))? == keys);
        Ok(())
    }

    #[cfg(feature = "tls-transport")]
    #[tokio::test(flavor = "current_thread")]
    async fn direct_only_owner_keeps_udp_tls_custody_after_restart() -> Result<()> {
        let mut fixture = Fixture::new()?;
        // An unusable saved preference must not become an effective route.
        // This checks publication and real local I/O, not external packet capture.
        fixture.0.relays = vec!["https://unused.example.invalid/".into()];
        fixture
            .provision()?
            .set_carrier_mode(CarrierMode::DirectOnly)?;
        fixture.open()?.set_native_carrier_mailbox_v8(true)?;
        fixture.open()?.set_tls_listener(Some(tls_settings(true)))?;
        fixture.open()?.set_native_listener(Some(settings()))?;
        let keys = std::fs::read(fixture.0.root.join(KEYS_FILE))?;
        let expires = (now_ms()? / 1000) as i64 + 600;
        let (receipt, sequence) = dual_exchange(&fixture, None, expires).await?;
        let (recovered, next) = dual_exchange(&fixture, Some(&receipt), expires).await?;
        ensure!(next > sequence && receipt.signature == recovered.signature);
        ensure!(std::fs::read(fixture.0.root.join(KEYS_FILE))? == keys);
        let owner = fixture.open()?;
        ensure!(owner.policy.config.relays == fixture.0.relays);
        ensure!(effective_relays(&owner.policy).is_empty());
        Ok(())
    }
}
