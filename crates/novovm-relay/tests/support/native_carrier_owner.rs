//! Actual Windows CLI, original DPAPI owner and persistent mailbox. The bounded
//! client drives the existing SDK over loopback QUIC; this is not mobile or
//! autonomous-relay acceptance evidence.
use super::*;
use novovm_network::{
    bound_session::authenticate_structured_bound_session_observed,
    carrier_stream::CarrierStreamV1,
    duplex::HandshakeReplayCacheV1,
    iroh_transport::IrohEndpointOwnerV1,
    mailbox_receipt::StoreReceiptV1,
    mailbox_rpc::{MailboxClientSessionV2, MailboxReplyV1 as Reply, MailboxRequestV1 as Request},
    mailbox_structured::{native_session::*, LIMITS, STREAM},
    novorudp::{
        carrier_io::NativeCarrierPump,
        fragments::ReassemblyLimits,
        object_link::{NativeLaneConfig, NativeObjectLink, NativeSendLimits, NativeSendPolicy},
        NovoRudpPacingProfile, NovoRudpWindowConfig,
    },
};

async fn configure(f: &Fixture, value: &str, success: bool) -> Result<()> {
    use std::os::windows::process::CommandExt;
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_novovm-rendezvous-service"))
            .arg("--set-self-owned-native-carrier")
            .arg(f.0.join("owned.json"))
            .arg(value)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .creation_flags(0x08000000)
            .spawn()?,
    );
    process
        .finish(success)
        .await
        .context("native carrier CLI selection")
}

type PreservedFiles = Vec<(&'static str, Option<Vec<u8>>)>;

fn snapshot(f: &Fixture) -> Result<PreservedFiles> {
    [
        "owned.json",
        "keys.dpapi",
        "service.journal",
        "mailbox.sqlite",
        "mailbox.sqlite-wal",
    ]
    .into_iter()
    .map(|name| {
        let path = f.0.join(name);
        Ok((
            name,
            if path.try_exists()? {
                Some(std::fs::read(path)?)
            } else {
                None
            },
        ))
    })
    .collect()
}

fn unchanged(f: &Fixture, before: &[(&str, Option<Vec<u8>>)]) -> Result<()> {
    let after = snapshot(f)?;
    for ((name, old), (_, new)) in before.iter().zip(after.iter()) {
        ensure!(old == new, "selection altered {name}");
    }
    Ok(())
}

fn put(expires: i64, object: u8) -> Request {
    Request::Put {
        mailbox: [81; 32],
        write: [82; 32],
        object: [object; 32],
        ciphertext: vec![object + 1; 9000],
        expires,
    }
}
fn fetch() -> Request {
    Request::Fetch {
        mailbox: [81; 32],
        read: [83; 32],
    }
}
fn book(record: &PeerSignedRelayRecordV1) -> Result<ServiceLeaseBookV1> {
    ServiceLeaseBookV1::new_self_owned(
        SelfOwnedTrustV1 {
            version: 1,
            node_pins: [record.relay_public_key].into(),
        },
        vec![record.clone()],
        now_ms()?,
    )
}
fn client_config() -> Result<IrohConfigV1> {
    IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)
}

async fn store_before_activation(
    record: PeerSignedRelayRecordV1,
    expires: i64,
) -> Result<StoreReceiptV1> {
    book(&record)?;
    let owner = IrohEndpointOwnerV1::bind(client_config()?, || Ok(())).await?;
    let result = tokio::time::timeout(
        Duration::from_secs(12),
        owner.with_continuous_service_v4(
            || Ok(()),
            move |scope| {
                Box::pin(async move {
                    let route = IrohServiceEndpointV1::from_record(&record)?;
                    let mut stream = scope
                        .connect(&route.address)
                        .await
                        .context("legacy custody connect")?
                        .into();
                    // The Iroh node dispatches NVMBOX06 as a multiplex root. Select
                    // that same binding context before Finished and retain its
                    // owner until the authenticated operation lane has finished.
                    let _group = request_mailbox_multiplex_v6(&mut stream)
                        .await
                        .context("legacy multiplex selection")?;
                    let mut client = MailboxClientSessionV2::begin_continuous_v6(
                        &mut stream,
                        &key(73),
                        record.relay_public_key,
                    )
                    .await
                    .context("legacy custody authentication")?;
                    ensure!(matches!(
                        client
                            .request(Request::Register {
                                mailbox: [81; 32],
                                write: [82; 32],
                                read: [83; 32],
                                expires,
                            })
                            .await
                            .context("legacy custody register")?,
                        Reply::Registered
                    ));
                    let Reply::Stored(receipt) = client
                        .request(put(expires, 84))
                        .await
                        .context("legacy custody Put")?
                    else {
                        anyhow::bail!("original mailbox did not acknowledge custody")
                    };
                    receipt.verify(
                        record.relay_public_key,
                        [81; 32],
                        [84; 32],
                        &vec![85; 9000],
                        expires,
                    )?;
                    client.finish().await.context("legacy custody finish")?;
                    Ok(receipt)
                })
            },
        ),
    )
    .await;
    let closed = owner.close().await;
    closed.context("legacy fixture endpoint cleanup")?;
    result.context("legacy fixture deadline")?
}

// Same bounded SDK driver as network/tests/mailbox_native_carrier.rs. All
// service execution, signatures and disk writes occur in the real executable.
struct Client<'a> {
    pump: NativeCarrierPump<'a>,
    link: NativeObjectLink,
    session: NativeMailboxClient,
}
impl<'a> Client<'a> {
    async fn attach(stream: &'a mut CarrierStreamV1<'_>, node: [u8; 32]) -> Result<Self> {
        request_mailbox_native_v7(stream)
            .await
            .context("native carrier selection")?;
        let channel = authenticate_structured_bound_session_observed(
            stream,
            &key(73),
            node,
            true,
            &mut HandshakeReplayCacheV1::new(8),
            &mut |_, _| Ok(()),
        )
        .await
        .context("native structured authentication")?;
        let (endpoint, pump) =
            NativeCarrierPump::attach_continuous_v7(stream, channel, vec![STREAM])?;
        let lane = NativeLaneConfig {
            stream: STREAM,
            domain: domain(),
            receive: ReassemblyLimits {
                max_message_bytes: LIMITS.wire_bytes,
                messages: SLOTS,
                bytes: LIMITS.wire_bytes * SLOTS,
                peer_messages: SLOTS,
                peer_bytes: LIMITS.wire_bytes * SLOTS,
                ttl: REQUEST_DEADLINE,
            },
            send: NativeSendLimits {
                messages: SLOTS,
                bytes: LIMITS.wire_bytes * SLOTS,
                peer_messages: SLOTS,
                peer_bytes: LIMITS.wire_bytes * SLOTS,
            },
        };
        let mut link = NativeObjectLink::new(
            endpoint,
            vec![lane],
            NativeSendPolicy {
                window: NovoRudpWindowConfig::default(),
                pacing: NovoRudpPacingProfile::default(),
                ttl: REQUEST_DEADLINE,
            },
        )?;
        let session = NativeMailboxClient::attach(&mut link, 0, node)?;
        Ok(Self {
            pump,
            link,
            session,
        })
    }
    async fn tick(&mut self) -> Result<Option<(RequestId, Vec<Reply>)>> {
        let now = Instant::now();
        self.session.progress_capacity(&mut self.link, now)?;
        let report = self.link.poll(now_ms()?, now, 64)?;
        ensure!(report.session_changes.is_empty(), "native session replaced");
        for done in report.completed {
            ensure!(
                self.session.transport_complete(&done).is_none(),
                "native transport failed"
            );
        }
        if let Some(body) = self.link.poll_complete(STREAM, now, 64)? {
            if !self.session.receive_capacity(&self.link, &body, now)? {
                return Ok(Some(
                    self.session.receive(&self.link, body, now, || Ok(()))?,
                ));
            }
        }
        tokio::select! {
            result = self.pump.progress() => result?,
            _ = tokio::time::sleep(Duration::from_millis(1)) => {},
        }
        Ok(None)
    }
    async fn request(&mut self, slot: usize, request: Request) -> Result<Reply> {
        let id = self.session.begin(slot, vec![request], Instant::now())?;
        let end = Instant::now() + Duration::from_secs(8);
        loop {
            ensure!(Instant::now() < end, "native request deadline");
            if self.session.negotiated_retained_bytes().is_some() {
                self.session
                    .try_send(&mut self.link, id, Instant::now(), || Ok(()))?;
            }
            if let Some((seen, mut replies)) = self.tick().await? {
                ensure!(seen == id && replies.len() == 1, "unexpected native reply");
                return Ok(replies.remove(0));
            }
        }
    }
    async fn record(
        &mut self,
        node: [u8; 32],
        history: &mut ServiceLeaseBookV1,
    ) -> Result<PeerSignedRelayRecordV1> {
        let Reply::ServiceRecordV7 { record } = self.request(3, Request::ServiceRecordV7).await?
        else {
            anyhow::bail!("missing native signed record")
        };
        history.accept_native_record_v7(node, record.clone(), now_ms()?)?;
        let route = NativeCarrierMailboxEndpointV7::from_record(&record)?
            .context("native marker missing")?;
        history.check_native_carrier_route_v7(node, &route, now_ms()?)?;
        Ok(record)
    }
}

async fn native_custody(
    record: PeerSignedRelayRecordV1,
    expires: i64,
    original: StoreReceiptV1,
    cross_epoch: bool,
) -> Result<(u64, StoreReceiptV1)> {
    let mut history = book(&record)?;
    let node = record.relay_public_key;
    let route =
        NativeCarrierMailboxEndpointV7::from_record(&record)?.context("native carrier absent")?;
    history.check_native_carrier_route_v7(node, &route, now_ms()?)?;
    let owner = IrohEndpointOwnerV1::bind(client_config()?, || Ok(())).await?;
    let result = tokio::time::timeout(
        Duration::from_secs(90),
        owner.with_continuous_service_v4(
            || Ok(()),
            move |scope| {
                Box::pin(async move {
                    let mut stream = scope.connect(&route.iroh.address).await?.into();
                    let mut client = Client::attach(&mut stream, node).await?;
                    let first = client.record(node, &mut history).await?;
                    let original_session = client.link.session_id(0);
                    let mut last = first.clone();
                    if cross_epoch {
                        let until = Instant::now() + Duration::from_secs(62);
                        while Instant::now() < until {
                            ensure!(
                                client.tick().await?.is_none(),
                                "unsolicited native business reply"
                            );
                        }
                        last = client.record(node, &mut history).await?;
                        ensure!(
                            last.sequence > first.sequence,
                            "normal node rotation did not advance"
                        );
                        let next = NativeCarrierMailboxEndpointV7::from_record(&last)?
                            .context("rotation lost marker")?;
                        let previous =
                            NativeCarrierMailboxEndpointV7::from_record(&first)?.unwrap();
                        ensure!(
                            next.iroh.instance != previous.iroh.instance
                                && next.iroh.address.endpoint_key
                                    == previous.iroh.address.endpoint_key
                        );
                        ensure!(
                            client.link.session_id(0) == original_session,
                            "rotation replaced session"
                        );
                    }
                    let Reply::Object {
                        object,
                        ciphertext,
                        digest,
                        expires: actual,
                    } = client.request(0, fetch()).await?
                    else {
                        anyhow::bail!("native migration/restart lost existing ciphertext")
                    };
                    ensure!(
                        object == original.object
                            && ciphertext == vec![original.object[0] + 1; 9000]
                            && actual == expires
                            && digest == original.digest,
                        "custody changed"
                    );
                    let Reply::Stored(replayed) =
                        client.request(0, put(expires, object[0])).await?
                    else {
                        anyhow::bail!("duplicate native Put did not retain receipt")
                    };
                    replayed.verify(node, [81; 32], object, &ciphertext, expires)?;
                    ensure!(
                        replayed.signature == original.signature,
                        "original durable receipt changed"
                    );
                    {
                        ensure!(matches!(
                            client
                                .request(
                                    0,
                                    Request::Consume {
                                        mailbox: [81; 32],
                                        read: [83; 32],
                                        object,
                                        digest,
                                    }
                                )
                                .await?,
                            Reply::Consumed
                        ));
                        ensure!(matches!(client.request(0, fetch()).await?, Reply::Empty));
                    }
                    let next_receipt = if cross_epoch {
                        // A genuinely new NVMBOX07 write must survive the next
                        // process exit, not just a replay of a legacy Stored reply.
                        let Reply::Stored(stored) = client.request(0, put(expires, 86)).await?
                        else {
                            anyhow::bail!("native Put did not commit new custody")
                        };
                        stored.verify(node, [81; 32], [86; 32], &vec![87; 9000], expires)?;
                        stored
                    } else {
                        replayed
                    };
                    Ok((last.sequence, next_receipt))
                })
            },
        ),
    )
    .await;
    let closed = owner.close().await;
    closed.context("native fixture endpoint cleanup")?;
    result.context("native fixture deadline")?
}

#[tokio::test(flavor = "current_thread")]
async fn windows_native_carrier_cli_preserves_identity_custody_and_rotation() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(140), async {
        let f = Fixture::new()?;
        f.owned_config()?;
        let initial = f.launch_owned("--provision-self-owned")?;
        let first = f
            .owned_record(0)
            .await
            .context("initial provision publication")?;
        ensure!(NativeCarrierMailboxEndpointV7::from_record(&first)?.is_none());
        let transport = IrohServiceEndpointV1::from_record(&first)?
            .address
            .endpoint_key;
        let expires = (now_ms()? / 1000) as i64 + 600;
        let receipt = store_before_activation(first.clone(), expires)
            .await
            .context("pre-activation custody")?;
        let policy_path = f.0.join("self-owned-policy.dpapi");
        let policy = std::fs::read(&policy_path)?;
        configure(&f, "on", false)
            .await
            .context("reject running owner mutation")?;
        ensure!(
            std::fs::read(&policy_path)? == policy,
            "live selection altered policy"
        );
        drop(initial);
        let before = snapshot(&f)?;
        configure(&f, "invalid", false)
            .await
            .context("reject invalid selection")?;
        unchanged(&f, &before)?;
        ensure!(
            std::fs::read(&policy_path)? == policy,
            "invalid selection altered policy"
        );
        configure(&f, "on", true)
            .await
            .context("enable native carrier")?;
        unchanged(&f, &before)?;
        let keys = std::fs::read(f.0.join("keys.dpapi"))?;
        let running = f.launch_owned("--foreground")?;
        let second = f
            .owned_record(first.sequence)
            .await
            .context("enabled carrier publication")?;
        ensure!(second.relay_public_key == first.relay_public_key);
        ensure!(
            IrohServiceEndpointV1::from_record(&second)?
                .address
                .endpoint_key
                == transport
        );
        ensure!(
            NativeMailboxEndpointV7::from_record(&second)?.is_none(),
            "Carrier activation enabled UDP listener"
        );
        let (latest, native_receipt) = native_custody(second, expires, receipt, true)
            .await
            .context("native custody across live rotation")?;
        let published = f
            .owned_record(latest - 1)
            .await
            .context("rotated disk publication")?;
        validate_relay_record_v1(&published, now_ms()?)?;
        ensure!(NativeCarrierMailboxEndpointV7::from_record(&published)?.is_some());
        drop(running); // Abrupt real process exit after confirmed durable custody.
        let restarted = f.launch_owned("--foreground")?;
        let third = f
            .owned_record(published.sequence)
            .await
            .context("restarted carrier publication")?;
        ensure!(third.relay_public_key == first.relay_public_key);
        ensure!(
            IrohServiceEndpointV1::from_record(&third)?
                .address
                .endpoint_key
                == transport
        );
        let (last, _) = native_custody(third, expires, native_receipt, false)
            .await
            .context("native custody after process restart")?;
        drop(restarted);
        let before = snapshot(&f)?;
        configure(&f, "off", true)
            .await
            .context("disable native carrier")?;
        unchanged(&f, &before)?;
        let _restored = f.launch_owned("--foreground")?;
        let fourth = f
            .owned_record(last)
            .await
            .context("disabled carrier publication")?;
        validate_relay_record_v1(&fourth, now_ms()?)?;
        ensure!(fourth.relay_public_key == first.relay_public_key);
        ensure!(
            IrohServiceEndpointV1::from_record(&fourth)?
                .address
                .endpoint_key
                == transport
        );
        ensure!(NativeCarrierMailboxEndpointV7::from_record(&fourth)?.is_none());
        ensure!(std::fs::read(f.0.join("keys.dpapi"))? == keys);
        Ok(())
    })
    .await?
}
