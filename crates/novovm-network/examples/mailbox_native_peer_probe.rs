//! Bounded remote probe using the existing native SDK drivers from
//! novovm-relay's rendezvous_dual_owner_tests and native owner tests.
//! No server, production identity, relay, new codec or retry scheduler.
//! Run with --features iroh-transport,tls-transport.
//! Arguments: RECORD EXPECTED_PIN_HEX udp|tls [CLIENTS ROUNDS PLAINTEXT_BYTES].
//! Load mode is UDP only; preparation/window checks are excluded from load rates.
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
            native_session::{
                adaptive_request_slot_ceiling_v1, adaptive_retained_bytes_v1, domain,
                NativeMailboxClient, RequestId, REQUEST_DEADLINE,
            },
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
        service_concurrency::ServiceConcurrencyV1,
        tls_transport::TlsScopeV1,
    };
    use std::{
        io::Read,
        net::UdpSocket,
        time::{Duration, Instant},
    };
    use zeroize::Zeroizing;

    fn lane(controller: &ServiceConcurrencyV1) -> NativeLaneConfig {
        // Diagnostic working set only; the product controller is unchanged.
        let messages = adaptive_request_slot_ceiling_v1(controller).min(8);
        let bytes = adaptive_retained_bytes_v1(controller).min(LIMITS.wire_bytes * 4);
        NativeLaneConfig {
            stream: STREAM,
            domain: domain(),
            receive: ReassemblyLimits {
                max_message_bytes: LIMITS.wire_bytes,
                messages,
                bytes,
                peer_messages: messages,
                peer_bytes: bytes,
                ttl: REQUEST_DEADLINE,
            },
            send: NativeSendLimits {
                messages,
                bytes,
                peer_messages: messages,
                peer_bytes: bytes,
            },
        }
    }

    // The SDK owns authentication, fragmentation, pacing, codecs and receipts.
    // This bounded diagnostic only polls its existing synchronous driver.
    struct Client<'a> {
        link: NativeObjectLink,
        session: NativeMailboxClient,
        pump: Option<NativeCarrierPump<'a>>,
        // Fixed aggregate counters: rejected, receive errors, send failures,
        // accepted/duplicate fragments, feedback sent, data sent.
        transport_counts: [u64; 7],
    }
    impl Client<'_> {
        fn poll_transport(&mut self, at: Instant) -> Result<()> {
            // Incoming bodies remain in the link until poll_complete below (or
            // the next progress call); only outbound completions transfer here.
            let report = self.link.poll(now_ms()?, at, 64)?;
            for (total, count) in self.transport_counts.iter_mut().zip([
                report.rejected,
                report.receive_errors,
                report.send_failures,
                report.accepted_fragments,
                report.duplicate_fragments,
                report.feedback_sent,
                report.data_sent,
            ]) {
                *total = total.saturating_add(count as u64);
            }
            for done in report.completed {
                ensure!(
                    self.session.transport_complete(&done).is_none(),
                    "native probe transport failed"
                );
            }
            Ok(())
        }

        async fn progress(&mut self, at: Instant) -> Result<Vec<(RequestId, Vec<Reply>)>> {
            self.session.progress_capacity(&mut self.link, at)?;
            self.poll_transport(at)?;
            let mut replies = Vec::new();
            let mut consumed = false;
            for _ in 0..8 {
                let Some(body) = self.link.poll_complete(STREAM, at, 64)? else {
                    break;
                };
                consumed = true;
                if !self.session.receive_capacity(&self.link, &body, at)? {
                    replies.push(self.session.receive(&self.link, body, at, || Ok(()))?);
                }
            }
            if consumed {
                // Whole-object verification queues DONE in poll_complete, after
                // the first transport poll. Drive that feedback before returning
                // the final grant/preflight reply and parking this ready client.
                // This is transport progress, not another business request.
                self.poll_transport(Instant::now())?;
            }
            if let Some(pump) = &mut self.pump {
                tokio::select! {
                    result = pump.progress() => result?,
                    _ = tokio::time::sleep(Duration::from_millis(1)) => {},
                }
            } else {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            Ok(replies)
        }

        async fn negotiate(&mut self) -> Result<()> {
            let deadline = Instant::now() + Duration::from_secs(10);
            while self.session.negotiated_retained_bytes().is_none() {
                ensure!(Instant::now() < deadline, "native probe capacity deadline");
                ensure!(
                    self.progress(Instant::now()).await?.is_empty(),
                    "unsolicited probe reply"
                );
            }
            Ok(())
        }

        async fn request(&mut self, request: Request) -> Result<Reply> {
            self.negotiate().await?;
            let transport_before = self.transport_counts;
            let started = Instant::now();
            let deadline = started + Duration::from_secs(10);
            let id = self
                .session
                .begin_until(0, vec![request], Instant::now(), deadline)?;
            let diagnostics = std::sync::Arc::new(
                novovm_network::novorudp::diagnostics::NativeTransferDiagnosticsV1::new(),
            );
            let request_tag = self.session.request_transfer_tag_v1(id);
            self.link.set_diagnostics_v1(diagnostics.clone());
            let outcome = async {
                loop {
                    let at = Instant::now();
                    if at >= deadline {
                        // Read local aggregate counters once on failure only.
                        // They do not identify the current object or prove
                        // remote receipt, execution or delivery.
                        let (outbound_messages, outbound_bytes) =
                            self.link.outbound_usage(STREAM)?;
                        let pending_inbound_messages = self.link.pending_messages(STREAM)?;
                        let reserved_inbound_bytes = self.link.reserved_bytes(STREAM)?;
                        bail!(
                            "native probe request deadline: elapsed_ms={}, outbound_messages={}, outbound_bytes={}, pending_inbound_messages={}, reserved_inbound_bytes={}",
                            at.saturating_duration_since(started).as_millis(),
                            outbound_messages,
                            outbound_bytes,
                            pending_inbound_messages,
                            reserved_inbound_bytes
                        );
                    }
                    self.session.try_send(&mut self.link, id, at, || Ok(()))?;
                    for (got, mut replies) in self.progress(at).await? {
                        ensure!(got == id && replies.len() == 1, "probe reply binding");
                        return Ok(replies.remove(0));
                    }
                }
            }
            .await;
            if outcome.is_err() {
                let delta: [u64; 7] = std::array::from_fn(|i| {
                    self.transport_counts[i].saturating_sub(transport_before[i])
                });
                // Failure-only, bounded authenticated transfer evidence. Rejections
                // remain window-scoped/unattributed even when one source matches.
                let evidence = serde_json::json!({"phase":"native_request_evidence", "request_tag":request_tag,
                    "request_elapsed_ms":started.elapsed().as_millis(), "clock_scope":"each side has its own monotonic origin; same-poll ties unordered",
                    "evidence":diagnostics.snapshot()});
                println!("{}", evidence);
                self.session.cancel(id);
                self.session.invalidate();
                return outcome.with_context(|| {
                    format!(
                        "native probe request poll deltas: rejected={}, receive_errors={}, send_failures={}, accepted_fragments={}, duplicate_fragments={}, feedback_sent={}, data_sent={}",
                        delta[0], delta[1], delta[2], delta[3], delta[4], delta[5], delta[6]
                    )
                });
            }
            outcome
        }

        async fn window_preflight(&mut self, connection: usize) -> Result<()> {
            self.negotiate().await?;
            let drain_deadline = Instant::now() + Duration::from_secs(10);
            while self.link.outbound_usage(STREAM)?.0 != 0 {
                ensure!(
                    Instant::now() < drain_deadline,
                    "capacity transport drain deadline"
                );
                ensure!(
                    self.progress(Instant::now()).await?.is_empty(),
                    "unsolicited preflight reply"
                );
            }
            let slots = self.session.slot_capacity();
            let target = slots.min(8);
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut pending = Vec::new();
            let mut sent = Vec::new();
            let mut peak = 0;
            let mut verified = 0;
            let mut rejections = std::collections::BTreeMap::<String, usize>::new();
            let outcome: Result<()> = async {
                for slot in 0..target {
                    pending.push(self.session.begin_until(
                        slot,
                        vec![Request::ServiceRecordV7],
                        Instant::now(),
                        deadline,
                    )?);
                }
                while !pending.is_empty() {
                    let at = Instant::now();
                    ensure!(at < deadline, "native probe window deadline");
                    for &id in &pending {
                        if self.session.try_send(&mut self.link, id, at, || Ok(()))?
                            && !sent.contains(&id)
                        {
                            sent.push(id);
                        }
                    }
                    peak = peak.max(sent.len());
                    for (id, mut replies) in self.progress(at).await? {
                        ensure!(
                            pending.contains(&id) && sent.contains(&id) && replies.len() == 1,
                            "window reply binding"
                        );
                        pending.retain(|p| *p != id);
                        sent.retain(|p| *p != id);
                        match replies.remove(0) {
                            Reply::ServiceRecordV7 { .. } => verified += 1,
                            Reply::Rejected(reason) => {
                                *rejections.entry(format!("{reason:?}")).or_default() += 1
                            }
                            _ => bail!("window metadata response mismatch"),
                        }
                    }
                }
                ensure!(
                    rejections.is_empty(),
                    "window requests rejected: {rejections:?}"
                );
                Ok(())
            }
            .await;
            println!(
                "{}",
                serde_json::json!({
                    "phase":"native_window_preflight", "connection":connection,
                    "negotiated_slots":slots, "negotiated_retained_bytes":self.session.negotiated_retained_bytes(),
                    "requested_metadata":target, "peak_sent_without_business_reply":peak,
                    "verified_metadata_replies":verified, "rejections":rejections,
                    "eight_request_window_verified":target == 8 && peak >= 8 && verified == 8 && outcome.is_ok(),
                    "error":outcome.as_ref().err().map(ToString::to_string),
                    "server_simultaneous_execution_proven":false
                })
            );
            if outcome.is_err() {
                for id in pending {
                    self.session.cancel(id);
                }
                self.session.invalidate();
            }
            outcome
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
        let controller = ServiceConcurrencyV1::from_host();
        let mut link = NativeObjectLink::new(
            endpoint,
            vec![lane(&controller)],
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
        let session = NativeMailboxClient::attach_adaptive(
            &mut link,
            0,
            record.relay_public_key,
            controller,
        )?;
        Ok(Client {
            link,
            session,
            pump: None,
            transport_counts: [0; 7],
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
        let controller = ServiceConcurrencyV1::from_host();
        let mut link = NativeObjectLink::new(
            endpoint,
            vec![lane(&controller)],
            NativeSendPolicy::reliable_carrier_v1(REQUEST_DEADLINE),
        )?;
        let session = NativeMailboxClient::attach_adaptive(&mut link, 0, pin, controller)?;
        Ok(Client {
            link,
            session,
            pump: Some(pump),
            transport_counts: [0; 7],
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
        stats: &mut LoadStats,
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
                .await
                .with_context(|| format!("load setup before round 1/{rounds} stage Register"))?,
            |r| matches!(r, Reply::Registered),
            "load Register",
        )
        .with_context(|| format!("load setup before round 1/{rounds} stage Register"))?;
        let key = Zeroizing::new(rand::random::<[u8; 32]>());
        let cipher = ChaCha20Poly1305::new_from_slice(key.as_ref()).unwrap();
        let plain = Zeroizing::new(vec![0x5a; size]);
        for round in 1..=rounds {
            let nonce: [u8; 12] = rand::random();
            let ciphertext = cipher
                .encrypt(Nonce::from_slice(&nonce), plain.as_slice())
                .map_err(|_| {
                    anyhow::anyhow!("load round {round}/{rounds} stage Put encryption failed")
                })?;
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
                .await
                .with_context(|| format!("load round {round}/{rounds} stage Put request"))?
            {
                Reply::Stored(value) => value,
                Reply::Rejected(reason) => {
                    bail!("load round {round}/{rounds} stage Put rejected: {reason:?}")
                }
                _ => bail!("load round {round}/{rounds} stage Put receipt missing"),
            };
            receipt
                .verify(pin, mailbox, object, &ciphertext, expires)
                .with_context(|| format!("load round {round}/{rounds} stage Put receipt verify"))?;
            stats.stored_plaintext_bytes += size;
            let digest = match client
                .request(Request::Fetch {
                    mailbox,
                    read: caps.0,
                })
                .await
                .with_context(|| format!("load round {round}/{rounds} stage Fetch request"))?
            {
                Reply::Object {
                    object: got,
                    ciphertext: bytes,
                    digest,
                    expires: ttl,
                } => {
                    ensure!(
                        got == object && bytes == ciphertext && ttl == expires,
                        "load round {round}/{rounds} stage Fetch object mismatch"
                    );
                    let decrypted = Zeroizing::new(
                        cipher
                            .decrypt(Nonce::from_slice(&nonce), bytes.as_slice())
                            .map_err(|_| {
                                anyhow::anyhow!(
                                    "load round {round}/{rounds} stage Fetch authentication failed"
                                )
                            })?,
                    );
                    ensure!(
                        *decrypted == *plain,
                        "load round {round}/{rounds} stage Fetch plaintext mismatch"
                    );
                    stats.fetched_plaintext_bytes += size;
                    digest
                }
                Reply::Rejected(reason) => {
                    bail!("load round {round}/{rounds} stage Fetch rejected: {reason:?}")
                }
                _ => bail!("load round {round}/{rounds} stage Fetch missing object"),
            };
            require(
                client
                    .request(Request::Consume {
                        mailbox,
                        read: caps.0,
                        object,
                        digest,
                    })
                    .await
                    .with_context(|| {
                        format!("load round {round}/{rounds} stage Consume request")
                    })?,
                |r| matches!(r, Reply::Consumed),
                "load Consume",
            )
            .with_context(|| format!("load round {round}/{rounds} stage Consume"))?;
            stats.cycles_ms.push(start.elapsed().as_millis());
        }
        Ok(())
    }

    #[derive(Default)]
    struct LoadStats {
        cycles_ms: Vec<u128>,
        stored_plaintext_bytes: usize,
        fetched_plaintext_bytes: usize,
    }
    // Nearest-rank empirical percentiles; a small sample remains a small sample.
    fn percentile(sorted: &[u128], percent: usize) -> Option<u128> {
        if sorted.is_empty() {
            return None;
        }
        let rank = (sorted.len() * percent).div_ceil(100).max(1);
        sorted.get(rank - 1).copied()
    }

    async fn load(
        record: &PeerSignedRelayRecordV1,
        pin: [u8; 32],
        clients: usize,
        rounds: usize,
        size: usize,
    ) -> Result<()> {
        use futures_util::{stream::FuturesUnordered, StreamExt};
        let preparation = Instant::now();
        let mut preparing = FuturesUnordered::new();
        for connection in 0..clients {
            preparing.push(async move {
                let seed = Zeroizing::new(rand::random::<[u8; 32]>());
                let mut client = udp(record, SigningKey::from_bytes(&seed))
                    .await
                    .with_context(|| format!("connection {connection} handshake"))?;
                client
                    .negotiate()
                    .await
                    .with_context(|| format!("connection {connection} resource negotiation"))?;
                println!(
                    "{}",
                    serde_json::json!({
                        "phase":"native_capacity_negotiated", "connection":connection,
                        "negotiated_slots":client.session.slot_capacity(),
                        "negotiated_retained_bytes":client.session.negotiated_retained_bytes()
                    })
                );
                Ok::<_, anyhow::Error>((connection, client))
            });
        }
        let mut ready = Vec::new();
        let mut failures = Vec::new();
        while let Some(result) = preparing.next().await {
            match result {
                Ok(client) => ready.push(client),
                Err(error) => failures.push(format!("preparation: {error:#}")),
            }
        }
        // One session proves the logical request window; do not turn this into
        // CLIENTS * 8 artificial metadata traffic before measuring payload work.
        if let Some((connection, mut client)) = ready.pop() {
            match client.window_preflight(connection).await {
                Ok(()) => ready.push((connection, client)),
                Err(error) => {
                    failures.push(format!("preflight connection {connection}: {error:#}"))
                }
            }
        }
        let preparation_errors = failures.len();
        let preparation_seconds = preparation.elapsed().as_secs_f64();
        let start = Instant::now();
        let mut pending = FuturesUnordered::new();
        for (connection, mut client) in ready {
            pending.push(async move {
                let mut stats = LoadStats::default();
                let result = load_exchange(&mut client, pin, rounds, size, &mut stats)
                    .await
                    .with_context(|| format!("connection {connection}"));
                (stats, result)
            });
        }
        let mut completed_clients = 0;
        let mut total = LoadStats::default();
        while let Some((mut stats, result)) = pending.next().await {
            total.cycles_ms.append(&mut stats.cycles_ms);
            total.stored_plaintext_bytes += stats.stored_plaintext_bytes;
            total.fetched_plaintext_bytes += stats.fetched_plaintext_bytes;
            match result {
                Ok(()) => completed_clients += 1,
                Err(error) => failures.push(format!("load: {error:#}")),
            }
        }
        let seconds = start.elapsed().as_secs_f64();
        total.cycles_ms.sort_unstable();
        println!(
            "{}",
            serde_json::json!({
                "mode":"bounded_service_load", "carrier":"native-udp",
                "offered_clients":clients, "completed_clients":completed_clients, "rounds_per_client":rounds,
                "payload_bytes":size, "verified_put_fetch_consume":total.cycles_ms.len(),
                "preparation_seconds":preparation_seconds, "elapsed_seconds":seconds,
                "objects_per_second":total.cycles_ms.len() as f64 / seconds,
                "verified_stored_plaintext_bytes":total.stored_plaintext_bytes,
                "verified_fetched_plaintext_bytes":total.fetched_plaintext_bytes,
                "put_plaintext_bytes_per_second":total.stored_plaintext_bytes as f64 / seconds,
                "fetch_verified_plaintext_bytes_per_second":total.fetched_plaintext_bytes as f64 / seconds,
                "put_plus_fetch_plaintext_bytes_per_second":(total.stored_plaintext_bytes + total.fetched_plaintext_bytes) as f64 / seconds,
                "p50_ms":percentile(&total.cycles_ms, 50), "p95_ms":percentile(&total.cycles_ms, 95),
                "p99_ms":percentile(&total.cycles_ms, 99), "max_ms":total.cycles_ms.last(),
                "percentile_method":"nearest_rank_successful_put_fetch_consume_cycles",
                "preparation_errors":preparation_errors, "load_errors":failures.len() - preparation_errors,
                "error_count":failures.len(), "failures":failures,
                "device_delivery_receipt":false, "android_ui_acceptance":false
            })
        );
        ensure!(
            completed_clients == clients,
            "load retains failed clients; not a full PASS"
        );
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn nearest_rank_percentiles_keep_small_sample_tail_and_empty_unknown() {
            assert_eq!(percentile(&[], 95), None);
            assert_eq!(percentile(&[3, 5, 8, 59], 50), Some(5));
            assert_eq!(percentile(&[3, 5, 8, 59], 95), Some(59));
            assert_eq!(percentile(&[3, 5, 8, 59], 99), Some(59));
            let hundred: Vec<_> = (1..=100).collect();
            assert_eq!(percentile(&hundred, 95), Some(95));
            assert_eq!(percentile(&hundred, 99), Some(99));
        }
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
                    && (1..=60).contains(&rounds)
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
                client.window_preflight(0).await?;
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
                client.window_preflight(0).await?;
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
