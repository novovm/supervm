//! Bounded remote probe using the existing native SDK drivers from
//! novovm-relay's rendezvous_dual_owner_tests and native owner tests.
//! No server, production identity, relay, new codec or retry scheduler.
//! Run with --features iroh-transport,tls-transport.
//! Arguments: RECORD EXPECTED_PIN_HEX udp|tls.
//! Only random probe ciphertext/capabilities are created, with a five-minute TTL.
//! This verifies service custody, NOT Android delivery or a DeviceReceipt.
#[cfg(not(all(feature = "iroh-transport", feature = "tls-transport")))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("mailbox_native_peer_probe requires --features iroh-transport,tls-transport")
}

#[cfg(all(feature = "iroh-transport", feature = "tls-transport"))]
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    probe::run().await
}

#[cfg(all(feature = "iroh-transport", feature = "tls-transport"))]
mod probe {
    use anyhow::{bail, ensure, Context, Result};
    use chacha20poly1305::{
        aead::{Aead, KeyInit},
        ChaCha20Poly1305, Nonce,
    };
    use ed25519_dalek::SigningKey;
    use novovm_network::{
        bound_session::peer_time::authenticate_mailbox_native_v8,
        carrier_stream::CarrierStreamV1,
        duplex::HandshakeReplayCacheV1,
        mailbox_rpc::{MailboxRejectionV1, MailboxReplyV1 as Reply, MailboxRequestV1 as Request},
        mailbox_store::MailboxGrant,
        mailbox_structured::{
            native_session::{domain, NativeMailboxClient, REQUEST_DEADLINE},
            LIMITS, STREAM,
        },
        novorudp::{
            carrier_io::NativeCarrierPump,
            endpoint::{NativeEndpoint, NativeHandshakeBudget, NativePeer},
            fragments::ReassemblyLimits,
            object_link::{NativeLaneConfig, NativeObjectLink, NativeSendLimits, NativeSendPolicy},
            NovoRudpPacingProfile, NovoRudpWindowConfig,
        },
        product_directory::{validate_relay_record_v1, PeerSignedRelayRecordV1},
        product_rendezvous_daemon::now_ms,
        product_service_lease::{
            NativeMailboxEndpointV7, SelfOwnedTrustV1, ServiceLeaseBookV1, TlsServiceEndpointV1,
        },
        tls_transport::TlsScopeV1,
    };
    use std::{
        io::Read,
        net::UdpSocket,
        time::{Duration, Instant},
    };
    use zeroize::Zeroizing;

    fn lane() -> NativeLaneConfig {
        NativeLaneConfig {
            stream: STREAM,
            domain: domain(),
            receive: ReassemblyLimits {
                max_message_bytes: LIMITS.wire_bytes,
                messages: 4,
                bytes: LIMITS.wire_bytes * 4,
                peer_messages: 4,
                peer_bytes: LIMITS.wire_bytes * 4,
                ttl: REQUEST_DEADLINE,
            },
            send: NativeSendLimits {
                messages: 4,
                bytes: LIMITS.wire_bytes * 4,
                peer_messages: 4,
                peer_bytes: LIMITS.wire_bytes * 4,
            },
        }
    }

    // The SDK owns authentication, fragmentation, pacing, codecs and receipts.
    // This bounded diagnostic only polls its existing synchronous driver.
    struct Client<'a> {
        link: NativeObjectLink,
        session: NativeMailboxClient,
        pump: Option<NativeCarrierPump<'a>>,
    }
    impl Client<'_> {
        async fn request(&mut self, request: Request) -> Result<Reply> {
            let id = self.session.begin(0, vec![request], Instant::now())?;
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut result = None;
            loop {
                let at = Instant::now();
                ensure!(at < deadline, "native probe request deadline");
                self.session.progress_capacity(&mut self.link, at)?;
                if result.is_none() {
                    self.session.try_send(&mut self.link, id, at, || Ok(()))?;
                }
                for done in self.link.poll(now_ms()?, at, 64)?.completed {
                    ensure!(
                        self.session.transport_complete(&done).is_none(),
                        "native probe transport failed"
                    );
                }
                if let Some(body) = self.link.poll_complete(STREAM, at, 64)? {
                    if !self.session.receive_capacity(&self.link, &body, at)? {
                        let (got, mut replies) =
                            self.session.receive(&self.link, body, at, || Ok(()))?;
                        ensure!(got == id && replies.len() == 1, "probe reply binding");
                        result = Some(replies.remove(0));
                    }
                }
                if self.session.negotiated_retained_bytes().is_some() {
                    if let Some(reply) = result.take() {
                        return Ok(reply);
                    }
                }
                if let Some(pump) = &mut self.pump {
                    tokio::select! {
                        result = pump.progress() => result?,
                        _ = tokio::time::sleep(Duration::from_millis(1)) => {},
                    }
                } else {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }
        }
    }

    async fn udp(record: &PeerSignedRelayRecordV1, key: SigningKey) -> Result<Client<'static>> {
        let route = NativeMailboxEndpointV7::from_record(record)?.context("UDP not advertised")?;
        let endpoint = NativeEndpoint::connect_client(
            UdpSocket::bind(if route.address.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            })?,
            key,
            NativePeer {
                key: record.relay_public_key,
                address: route.address,
            },
            NativeHandshakeBudget {
                lifetime: Duration::from_secs(10),
                retry: Duration::from_millis(500),
            },
            now_ms()?,
            Instant::now(),
        )?;
        let mut link = NativeObjectLink::new(
            endpoint,
            vec![lane()],
            NativeSendPolicy {
                window: NovoRudpWindowConfig::default(),
                pacing: NovoRudpPacingProfile::default(),
                ttl: REQUEST_DEADLINE,
            },
        )?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while link.session_id(0).is_none() {
            ensure!(
                Instant::now() < deadline,
                "native probe UDP handshake deadline"
            );
            let _ = link.poll(now_ms()?, Instant::now(), 64)?;
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let session = NativeMailboxClient::attach(&mut link, 0, record.relay_public_key)?;
        Ok(Client {
            link,
            session,
            pump: None,
        })
    }

    async fn tls<'a>(
        stream: &'a mut CarrierStreamV1<'_>,
        key: &SigningKey,
        pin: [u8; 32],
    ) -> Result<Client<'a>> {
        let (channel, observed) = authenticate_mailbox_native_v8(
            stream,
            key,
            pin,
            Instant::now() + Duration::from_secs(10),
            &mut HandshakeReplayCacheV1::new(8),
            &mut |_, _| Ok(()),
        )
        .await?;
        ensure!(
            observed.peer_identity_public_key == pin,
            "native probe peer mismatch"
        );
        let (endpoint, pump) =
            NativeCarrierPump::attach_continuous_v7(stream, channel, vec![STREAM])?;
        let mut link = NativeObjectLink::new(
            endpoint,
            vec![lane()],
            NativeSendPolicy::reliable_carrier_v1(REQUEST_DEADLINE),
        )?;
        let session = NativeMailboxClient::attach(&mut link, 0, pin)?;
        Ok(Client {
            link,
            session,
            pump: Some(pump),
        })
    }

    fn require(reply: Reply, expected: fn(&Reply) -> bool, operation: &str) -> Result<()> {
        if let Reply::Rejected(reason) = reply {
            bail!("{operation} rejected: {reason:?}; node policy was not changed");
        }
        ensure!(expected(&reply), "{operation} unexpected response");
        Ok(())
    }

    async fn exchange(client: &mut Client<'_>, pin: [u8; 32]) -> Result<()> {
        let grant = MailboxGrant::generate()?;
        let mailbox = grant.id;
        let caps = Zeroizing::new((*grant.read.as_bytes(), *grant.write.as_bytes()));
        let object: [u8; 32] = rand::random();
        let plain = Zeroizing::new(rand::random::<[u8; 32]>());
        let key = Zeroizing::new(rand::random::<[u8; 32]>());
        let cipher = ChaCha20Poly1305::new_from_slice(key.as_ref()).unwrap();
        let nonce: [u8; 12] = rand::random();
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plain.as_ref())
            .map_err(|_| anyhow::anyhow!("probe encryption failed"))?;
        let expires = (now_ms()? / 1000) as i64 + 300;
        require(
            client
                .request(Request::Register {
                    mailbox,
                    read: caps.0,
                    write: caps.1,
                    expires,
                })
                .await?,
            |r| matches!(r, Reply::Registered),
            "Register",
        )?;
        let operation = Instant::now();
        let put = || Request::Put {
            mailbox,
            write: caps.1,
            object,
            ciphertext: ciphertext.clone(),
            expires,
        };
        let receipt = match client.request(put()).await? {
            Reply::Stored(receipt) => receipt,
            Reply::Rejected(reason) => {
                bail!("Put rejected: {reason:?}; node policy was not changed")
            }
            _ => bail!("Put missing StoreReceipt"),
        };
        receipt.verify(pin, mailbox, object, &ciphertext, expires)?;
        println!(
            "{}",
            serde_json::json!({
                "phase": "store_receipt_verified", "elapsed_ms": operation.elapsed().as_millis()
            })
        );
        let duplicate = match client.request(put()).await? {
            Reply::Stored(receipt) => receipt,
            _ => bail!("duplicate Put missing StoreReceipt"),
        };
        duplicate.verify(pin, mailbox, object, &ciphertext, expires)?;
        ensure!(
            duplicate.signature == receipt.signature,
            "duplicate changed custody receipt"
        );
        ensure!(
            matches!(
                client
                    .request(Request::Fetch {
                        mailbox,
                        read: rand::random()
                    })
                    .await?,
                Reply::Rejected(MailboxRejectionV1::Unauthorized)
            ),
            "wrong-capability Fetch was not rejected"
        );
        let (received, digest) = match client
            .request(Request::Fetch {
                mailbox,
                read: caps.0,
            })
            .await?
        {
            Reply::Object {
                object: actual,
                ciphertext: bytes,
                digest,
                expires: ttl,
            } => {
                ensure!(
                    actual == object && bytes == ciphertext && ttl == expires,
                    "stored object or original TTL changed"
                );
                (bytes, digest)
            }
            Reply::Rejected(reason) => {
                bail!("Fetch rejected: {reason:?}; node policy was not changed")
            }
            _ => bail!("Fetch missing original object"),
        };
        let decrypted = Zeroizing::new(
            cipher
                .decrypt(Nonce::from_slice(&nonce), received.as_slice())
                .map_err(|_| anyhow::anyhow!("probe decryption failed"))?,
        );
        ensure!(
            decrypted.as_slice() == plain.as_ref(),
            "probe plaintext differs"
        );
        require(
            client
                .request(Request::Consume {
                    mailbox,
                    read: caps.0,
                    object,
                    digest,
                })
                .await?,
            |r| matches!(r, Reply::Consumed),
            "Consume",
        )?;
        require(
            client
                .request(Request::Fetch {
                    mailbox,
                    read: caps.0,
                })
                .await?,
            |r| matches!(r, Reply::Empty),
            "post-Consume Fetch",
        )?;
        println!(
            "{}",
            serde_json::json!({"phase":"fetch_verified_consumed", "original_ttl":true})
        );
        Ok(())
    }

    // Explicit bounded load mode: real authenticated service operations and
    // verified receipts, never application messages or production TPS claims.
    async fn load_exchange(
        client: &mut Client<'_>,
        pin: [u8; 32],
        rounds: usize,
        size: usize,
        durations: &mut Vec<u128>,
    ) -> Result<()> {
        let grant = MailboxGrant::generate()?;
        let mailbox = grant.id;
        let caps = Zeroizing::new((*grant.read.as_bytes(), *grant.write.as_bytes()));
        let expires = (now_ms()? / 1000) as i64 + 300;
        require(
            client
                .request(Request::Register {
                    mailbox,
                    read: caps.0,
                    write: caps.1,
                    expires,
                })
                .await?,
            |r| matches!(r, Reply::Registered),
            "load Register",
        )?;
        let key = Zeroizing::new(rand::random::<[u8; 32]>());
        let cipher = ChaCha20Poly1305::new_from_slice(key.as_ref()).unwrap();
        let plain = Zeroizing::new(vec![0x5a; size]);
        for _ in 0..rounds {
            let nonce: [u8; 12] = rand::random();
            let ciphertext = cipher
                .encrypt(Nonce::from_slice(&nonce), plain.as_slice())
                .map_err(|_| anyhow::anyhow!("load encryption failed"))?;
            let object: [u8; 32] = rand::random();
            let start = Instant::now();
            let receipt = match client
                .request(Request::Put {
                    mailbox,
                    write: caps.1,
                    object,
                    ciphertext: ciphertext.clone(),
                    expires,
                })
                .await?
            {
                Reply::Stored(value) => value,
                Reply::Rejected(reason) => bail!("load Put rejected: {reason:?}"),
                _ => bail!("load Put receipt missing"),
            };
            receipt.verify(pin, mailbox, object, &ciphertext, expires)?;
            let digest = match client
                .request(Request::Fetch {
                    mailbox,
                    read: caps.0,
                })
                .await?
            {
                Reply::Object {
                    object: got,
                    ciphertext: bytes,
                    digest,
                    expires: ttl,
                } => {
                    ensure!(
                        got == object && bytes == ciphertext && ttl == expires,
                        "load object mismatch"
                    );
                    let decrypted = Zeroizing::new(
                        cipher
                            .decrypt(Nonce::from_slice(&nonce), bytes.as_slice())
                            .map_err(|_| anyhow::anyhow!("load authentication failed"))?,
                    );
                    ensure!(*decrypted == *plain, "load plaintext mismatch");
                    digest
                }
                _ => bail!("load Fetch missing object"),
            };
            require(
                client
                    .request(Request::Consume {
                        mailbox,
                        read: caps.0,
                        object,
                        digest,
                    })
                    .await?,
                |r| matches!(r, Reply::Consumed),
                "load Consume",
            )?;
            durations.push(start.elapsed().as_millis());
        }
        Ok(())
    }

    async fn load(
        record: &PeerSignedRelayRecordV1,
        pin: [u8; 32],
        clients: usize,
        rounds: usize,
        size: usize,
    ) -> Result<()> {
        use futures_util::{stream::FuturesUnordered, StreamExt};
        let start = Instant::now();
        let mut pending = FuturesUnordered::new();
        for _ in 0..clients {
            pending.push(async {
                let mut times = Vec::new();
                let result = async {
                    let seed = Zeroizing::new(rand::random::<[u8; 32]>());
                    let mut client = udp(record, SigningKey::from_bytes(&seed)).await?;
                    load_exchange(&mut client, pin, rounds, size, &mut times).await
                }
                .await;
                (times, result)
            });
        }
        let mut completed_clients = 0;
        let mut samples = Vec::new();
        let mut failures = Vec::new();
        while let Some((mut times, result)) = pending.next().await {
            samples.append(&mut times);
            match result {
                Ok(()) => {
                    completed_clients += 1;
                }
                Err(error) => failures.push(error.to_string()),
            }
        }
        let seconds = start.elapsed().as_secs_f64();
        samples.sort_unstable();
        println!(
            "{}",
            serde_json::json!({"mode":"bounded_service_load", "carrier":"native-udp",
            "offered_clients":clients,"completed_clients":completed_clients,"rounds_per_client":rounds,
            "payload_bytes":size,"verified_put_fetch_consume":samples.len(),"elapsed_seconds":seconds,
            "objects_per_second":samples.len() as f64 / seconds,
            "p50_ms":samples.get(samples.len()/2),"max_ms":samples.last(),"failures":failures,
            "device_delivery_receipt":false,"android_ui_acceptance":false})
        );
        ensure!(
            completed_clients == clients,
            "load retains failed clients; not a full PASS"
        );
        Ok(())
    }

    pub async fn run() -> Result<()> {
        let args: Vec<_> = std::env::args().collect();
        ensure!(
            args.len() == 4 || args.len() == 7,
            "usage: mailbox_native_peer_probe RECORD EXPECTED_PIN_HEX udp|tls [CLIENTS ROUNDS PLAINTEXT_BYTES]"
        );
        ensure!(
            matches!(args[3].as_str(), "udp" | "tls"),
            "carrier must be udp or tls"
        );
        ensure!(
            args[2].len() == 64 && args[2].is_ascii(),
            "pin must be 32-byte hexadecimal"
        );
        let mut pin = [0; 32];
        for (i, byte) in pin.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&args[2][i * 2..i * 2 + 2], 16).context("invalid pin")?;
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&args[1])?
            .take(16_385)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 16_384, "record too large");
        let record: PeerSignedRelayRecordV1 = serde_json::from_slice(&bytes)?;
        ensure!(
            record.relay_public_key == pin,
            "record differs from approved pin"
        );
        validate_relay_record_v1(&record, now_ms()?)?;
        if args.len() == 7 {
            ensure!(args[3] == "udp", "bounded load uses native UDP only");
            let clients: usize = args[4].parse()?;
            let rounds: usize = args[5].parse()?;
            let size: usize = args[6].parse()?;
            ensure!(
                (1..=32).contains(&clients)
                    && (1..=16).contains(&rounds)
                    && (1..=32768).contains(&size),
                "bounded diagnostic limits"
            );
            return tokio::time::timeout(
                Duration::from_secs(90),
                load(&record, pin, clients, rounds, size),
            )
            .await?;
        }
        let mut book = ServiceLeaseBookV1::new_self_owned(
            SelfOwnedTrustV1 {
                version: 1,
                node_pins: [pin].into(),
            },
            vec![record.clone()],
            now_ms()?,
        )?;
        let seed = Zeroizing::new(rand::random::<[u8; 32]>());
        let key = SigningKey::from_bytes(&seed);
        let start = Instant::now();
        tokio::time::timeout(Duration::from_secs(90), async {
            if args[3] == "udp" {
                let route = NativeMailboxEndpointV7::from_record(&record)?.context("UDP not advertised")?;
                book.check_native_mailbox_route_v7(pin, &route, now_ms()?)?;
                let mut client = udp(&record, key).await?;
                println!("{}", serde_json::json!({"phase":"authenticated","carrier":"udp","elapsed_ms":start.elapsed().as_millis()}));
                exchange(&mut client, pin).await?;
            } else {
                let route = TlsServiceEndpointV1::from_record(&record)?.context("TLS not advertised")?;
                ensure!(route.native_mailbox_v8, "native TLS V8 not advertised");
                book.check_tls_route_v1(pin, &route, now_ms()?)?;
                book.check_continuous_native_tls_v8(pin, &route, now_ms()?)?;
                let scope = TlsScopeV1::new_continuous_v4(|| Ok(()))?;
                let tcp = tokio::net::TcpStream::connect(route.address).await?;
                tcp.set_nodelay(true)?;
                let mut stream = CarrierStreamV1::from(scope.connect(tcp, route.endpoint_key).await?);
                let mut client = tls(&mut stream, &key, pin).await?;
                println!("{}", serde_json::json!({"phase":"authenticated","carrier":"tls-v8","elapsed_ms":start.elapsed().as_millis()}));
                exchange(&mut client, pin).await?;
            }
            Ok::<_, anyhow::Error>(())
        }).await.context("bounded native probe expired")??;
        println!(
            "{}",
            serde_json::json!({
                "result":"PASS", "carrier":args[3], "device_delivery_receipt":false,
                "android_acceptance":false, "relay_services_used":false, "billing_exercised":false
            })
        );
        Ok(())
    }
}
