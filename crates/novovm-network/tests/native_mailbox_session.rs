#![cfg(feature = "iroh-transport")]
//! Public SDK/session consumption, not a substitute for Android owner wiring.
use anyhow::{ensure, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    mailbox_rpc::{
        MailboxReplyV1 as Reply, MailboxRequestV1 as Request, MailboxServiceV1, MailboxWatchV1,
    },
    mailbox_store::MailboxStore,
    mailbox_structured::{native_session::*, LIMITS, STREAM},
    novorudp::{
        endpoint::{NativeEndpoint, NativeHandshakeBudget, NativePeer},
        fragments::ReassemblyLimits,
        object_link::{NativeLaneConfig, NativeObjectLink, NativeSendLimits, NativeSendPolicy},
        NovoRudpPacingProfile, NovoRudpWindowConfig,
    },
    product_directory::sign_relay_record_v1,
    product_service_lease::{NativeMailboxEndpointV7, SelfOwnedTrustV1, ServiceLeaseBookV1},
};
use std::{
    collections::VecDeque,
    net::UdpSocket,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

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
            ttl: Duration::from_secs(5),
        },
        send: NativeSendLimits {
            messages: 4,
            bytes: LIMITS.wire_bytes * 4,
            peer_messages: 4,
            peer_bytes: LIMITS.wire_bytes * 4,
        },
    }
}
fn budget() -> NativeHandshakeBudget {
    NativeHandshakeBudget {
        lifetime: Duration::from_secs(5),
        retry: Duration::from_millis(10),
    }
}
fn policy() -> NativeSendPolicy {
    NativeSendPolicy {
        window: NovoRudpWindowConfig {
            batch_size: 32,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            packet_copies: 1,
            tail_packet_copies: 1,
            ..Default::default()
        },
        pacing: NovoRudpPacingProfile {
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 32,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            no_progress_backoff: false,
        },
        ttl: Duration::from_secs(5),
    }
}
struct Fixture {
    server_link: NativeObjectLink,
    client_link: NativeObjectLink,
    server: NativeMailboxServer,
    client: NativeMailboxClient,
    service: Arc<MailboxServiceV1>,
    alive: Arc<AtomicBool>,
    jobs: tokio::task::JoinSet<Result<MailboxResponse>>,
    responses: VecDeque<MailboxResponse>,
    origin: Instant,
    received: Vec<(RequestId, Vec<Reply>)>,
    admitted: usize,
    pending_budget: usize,
    _directory: tempfile::TempDir,
}

#[tokio::test]
async fn retired_socket_generation_stops_already_admitted_mailbox_work() -> Result<()> {
    let mut f = Fixture::new().await?;
    let id = f.send(
        0,
        vec![Request::Register {
            mailbox: [1; 32],
            write: [2; 32],
            read: [3; 32],
            expires: i64::MAX,
        }],
    )?;
    let body = f.receive_request().await?;
    let work = f
        .server
        .admit(&f.server_link, body, Instant::now(), || Ok(()))?;
    // No explicit server.invalidate: the transport owner lifetime itself is
    // part of the work guard, including work retained across an await.
    drop(f.server_link);
    assert!(work.execute(&f.service, || Ok(())).await.is_err());
    assert!(matches!(
        f.service
            .request_local_v1(Request::Fetch {
                mailbox: [1; 32],
                read: [3; 32],
            })
            .await?,
        Reply::Rejected(_)
    ));
    drop(f.client_link);
    assert_eq!(f.client.expire(Instant::now()), vec![id]);
    Ok(())
}
impl Fixture {
    async fn receive_request(
        &mut self,
    ) -> Result<novovm_network::novorudp::fragments::CompletedMessage> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            ensure!(Instant::now() < deadline, "request receive timeout");
            let now = Instant::now();
            let wall = 1000 + self.origin.elapsed().as_millis() as u64;
            for done in self.client_link.poll(wall, now, 64)?.completed {
                ensure!(
                    self.client.transport_complete(&done).is_none(),
                    "request expired"
                );
            }
            let _ = self.server_link.poll(wall, now, 64)?;
            if let Some(body) = self.server_link.poll_complete(STREAM, now, 64)? {
                return Ok(body);
            }
            tokio::task::yield_now().await;
        }
    }
    async fn new() -> Result<Self> {
        Self::new_with_pressure(novovm_network::service_concurrency::ResourcePressureV1::Normal)
            .await
    }
    async fn new_with_pressure(
        pressure: novovm_network::service_concurrency::ResourcePressureV1,
    ) -> Result<Self> {
        let key = SigningKey::from_bytes(&[17; 32]);
        let pin = key.verifying_key().to_bytes();
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        let address = socket.local_addr()?;
        // Consumer obtains the actual native endpoint through the original
        // signed directory/pin owner, without an Iroh placeholder or Publisher.
        let route = NativeMailboxEndpointV7 {
            address,
            max_connections: 1,
            max_bytes_per_minute: 64 * 1024 * 1024,
        };
        let record = sign_relay_record_v1(
            &key,
            "native-mailbox",
            vec![route.endpoint()?],
            1000,
            91_000,
            1,
        )?;
        let mut directory_book = ServiceLeaseBookV1::new_self_owned(
            SelfOwnedTrustV1 {
                version: 1,
                node_pins: [pin].into(),
            },
            vec![record],
            1000,
        )?;
        let candidates = directory_book.native_mailbox_candidates_v7(1000)?;
        let selected = &candidates[0];
        directory_book.check_native_mailbox_route_v7(
            selected.public_key,
            &selected.endpoint,
            1000,
        )?;
        let mut server_link = NativeObjectLink::new(
            NativeEndpoint::listen_clients(socket, key.clone(), 1, vec![STREAM], budget())?,
            vec![capacity_lane()],
            policy(),
        )?;
        let origin = Instant::now();
        let mut client_link = NativeObjectLink::new(
            NativeEndpoint::connect_client(
                UdpSocket::bind("127.0.0.1:0")?,
                SigningKey::from_bytes(&[83; 32]),
                NativePeer {
                    key: selected.public_key,
                    address: selected.endpoint.address,
                },
                budget(),
                1000,
                origin,
            )?,
            vec![capacity_lane()],
            policy(),
        )?;
        while client_link.session_id(0).is_none() || server_link.session_id(0).is_none() {
            ensure!(
                origin.elapsed() < Duration::from_secs(5),
                "native handshake deadline"
            );
            let now = Instant::now();
            let wall = 1000 + origin.elapsed().as_millis() as u64;
            let _ = client_link.poll(wall, now, 64)?;
            let _ = server_link.poll(wall, now, 64)?;
            tokio::task::yield_now().await;
        }
        assert!(NativeMailboxClient::attach(&mut client_link, 0, [99; 32]).is_err());
        directory_book.check_continuous_native_peer_v7(selected.public_key, 1000)?;
        let client = NativeMailboxClient::attach_adaptive(
            &mut client_link,
            0,
            selected.public_key,
            capacity_controller(pressure),
        )?;
        let server = NativeMailboxServer::attach_adaptive(
            &mut server_link,
            0,
            capacity_controller(pressure),
        )?;
        assert!(NativeMailboxClient::attach(&mut client_link, 0, pin).is_err());
        assert!(NativeMailboxServer::attach(&mut server_link, 0).is_err());
        // Same authenticated peer may provide AND consume service; roles have
        // independent, direction-bound counters, not an exclusive client license.
        let reverse_client =
            NativeMailboxClient::attach(&mut server_link, 0, client_link.local_identity())?;
        let reverse_server = NativeMailboxServer::attach(&mut client_link, 0)?;
        drop((reverse_client, reverse_server)); // no implicit counter reset on drop
        let directory = tempfile::tempdir()?;
        let service = Arc::new(MailboxServiceV1::new(
            MailboxStore::open_contributor(
                directory.path().join("mailbox.db"),
                Default::default(),
                true,
            )?,
            key,
        ));
        service.set_resources_available(true).await?;
        Ok(Self {
            server_link,
            client_link,
            server,
            client,
            service,
            alive: Arc::new(AtomicBool::new(true)),
            jobs: tokio::task::JoinSet::new(),
            responses: VecDeque::new(),
            origin,
            received: Vec::new(),
            admitted: 0,
            pending_budget: 64,
            _directory: directory,
        })
    }
    fn send(&mut self, slot: usize, requests: Vec<Request>) -> Result<RequestId> {
        let id = self.client.begin(slot, requests, Instant::now())?;
        ensure!(
            self.client
                .try_send(&mut self.client_link, id, Instant::now(), || Ok(()))?,
            "fixture send capacity"
        );
        Ok(id)
    }
    async fn tick(&mut self) -> Result<()> {
        let now = Instant::now();
        self.client.progress_capacity(&mut self.client_link, now)?;
        let wall = 1000 + self.origin.elapsed().as_millis() as u64;
        for done in self.client_link.poll(wall, now, 64)?.completed {
            ensure!(
                self.client.transport_complete(&done).is_none(),
                "request transport expired"
            );
        }
        let _ = self.server_link.poll(wall, now, 64)?;
        while let Some(body) = self.server_link.poll_complete(STREAM, now, 64)? {
            assert_eq!(
                recipient_role(&body)?,
                novovm_network::novorudp::object_link::NativeApplicationRole::Service
            );
            ensure!(self.jobs.len() < self.pending_budget, "work budget");
            let work = self.server.admit(&self.server_link, body, now, || Ok(()))?;
            let service = self.service.clone();
            let alive = self.alive.clone();
            self.admitted += 1;
            self.jobs.spawn(async move {
                work.execute(&service, || {
                    ensure!(alive.load(Ordering::SeqCst), "owner revoked");
                    Ok(())
                })
                .await
            });
        }
        while let Some(result) = self.jobs.try_join_next() {
            self.responses.push_back(result??);
            ensure!(
                self.responses.len() <= self.pending_budget,
                "response budget"
            );
        }
        if let Some(response) = self.responses.front_mut() {
            if response.try_send(&mut self.server_link, now, || Ok(()))? {
                self.responses.pop_front();
            }
        }
        while let Some(body) = self.client_link.poll_complete(STREAM, now, 64)? {
            assert_eq!(
                recipient_role(&body)?,
                novovm_network::novorudp::object_link::NativeApplicationRole::Client
            );
            if self
                .client
                .receive_capacity(&self.client_link, &body, now)?
            {
                continue;
            }
            self.received.push(
                self.client
                    .receive(&self.client_link, body, now, || Ok(()))?,
            );
        }
        tokio::task::yield_now().await;
        Ok(())
    }
    async fn until(&mut self, id: RequestId) -> Result<Vec<Reply>> {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(i) = self.received.iter().position(|(seen, _)| *seen == id) {
                return Ok(self.received.remove(i).1);
            }
            ensure!(Instant::now() < deadline, "mailbox outcome timeout");
            self.tick().await?;
        }
    }
}

#[tokio::test]
async fn dropped_response_preserves_commit_but_latches_replay_and_new_session_binding() -> Result<()>
{
    use novovm_network::novorudp::fragments::CompletedMessage;
    let mut f = Fixture::new().await?;
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64
        + 600;
    let id = f.send(
        0,
        vec![
            Request::Register {
                mailbox: [1; 32],
                write: [2; 32],
                read: [3; 32],
                expires,
            },
            Request::Put {
                mailbox: [1; 32],
                write: [2; 32],
                object: [4; 32],
                ciphertext: vec![7; 64],
                expires,
            },
        ],
    )?;
    let body = f.receive_request().await?;
    let replay = || CompletedMessage {
        peer: body.peer.clone(),
        id: body.id,
        chunks: body.chunks.clone(),
    };
    let work = f
        .server
        .admit(&f.server_link, replay(), Instant::now(), || Ok(()))?;
    assert!(f
        .server
        .admit(&f.server_link, replay(), Instant::now(), || Ok(()))
        .is_err());
    let response = work.execute(&f.service, || Ok(())).await?;
    drop(response); // deliberate post-commit/pre-enqueue loss, not a store rollback
    assert!(f
        .server
        .admit(&f.server_link, replay(), Instant::now(), || Ok(()))
        .is_err());
    assert!(matches!(
        f.service
            .request_local_v1(Request::Fetch {
                mailbox: [1; 32],
                read: [3; 32]
            })
            .await?,
        Reply::Object { .. }
    ));
    assert_eq!(f.client.expire(Instant::now() + REQUEST_DEADLINE), vec![id]);
    assert!(f
        .client
        .begin(
            0,
            vec![Request::Fetch {
                mailbox: [1; 32],
                read: [3; 32]
            }],
            Instant::now()
        )
        .is_err());
    // Another slot recovers the ORIGINAL durable receipt through existing Put
    // idempotency. No global request reset, second DB or fabricated success.
    let retry = f.send(
        1,
        vec![Request::Put {
            mailbox: [1; 32],
            write: [2; 32],
            object: [4; 32],
            ciphertext: vec![7; 64],
            expires,
        }],
    )?;
    assert!(matches!(
        f.until(retry).await?.as_slice(),
        [Reply::Stored(_)]
    ));
    let mut other = Fixture::new().await?;
    assert_ne!(f.client_link.session_id(0), other.client_link.session_id(0));
    assert!(other
        .server
        .admit(&other.server_link, replay(), Instant::now(), || Ok(()))
        .is_err());
    assert!(matches!(
        other
            .service
            .request_local_v1(Request::Fetch {
                mailbox: [1; 32],
                read: [3; 32]
            })
            .await?,
        Reply::Rejected(_)
    ));
    Ok(())
}

#[tokio::test]
async fn caller_revocation_latches_work_and_cannot_be_reversed_by_retrying_response() -> Result<()>
{
    let mut f = Fixture::new().await?;
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64
        + 600;
    f.send(
        0,
        vec![Request::Register {
            mailbox: [1; 32],
            write: [2; 32],
            read: [3; 32],
            expires,
        }],
    )?;
    let body = f.receive_request().await?;
    let work = f
        .server
        .admit(&f.server_link, body, Instant::now(), || Ok(()))?;
    let mut response = work.execute(&f.service, || Ok(())).await?;
    assert!(response
        .try_send(&mut f.server_link, Instant::now(), || anyhow::bail!(
            "revoked"
        ))
        .is_err());
    assert!(response
        .try_send(&mut f.server_link, Instant::now(), || Ok(()))
        .is_err());
    assert_eq!(f.server_link.outbound_usage(STREAM)?, (0, 0));
    f.send(
        1,
        vec![Request::Fetch {
            mailbox: [1; 32],
            read: [3; 32],
        }],
    )?;
    let body = f.receive_request().await?;
    let work = f
        .server
        .admit(&f.server_link, body, Instant::now(), || Ok(()))?;
    f.server.invalidate();
    assert!(work.execute(&f.service, || Ok(())).await.is_err());
    Ok(())
}

#[tokio::test]
async fn shared_session_multiplexes_watch_and_put_and_checks_actual_storage_reply() -> Result<()> {
    let mut f = Fixture::new().await?;
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64
        + 600;
    let id = f.send(
        0,
        vec![Request::Register {
            mailbox: [1; 32],
            write: [2; 32],
            read: [3; 32],
            expires,
        }],
    )?;
    assert!(matches!(f.until(id).await?.as_slice(), [Reply::Registered]));
    let request = |after, wait_ms| Request::WatchFetchV6 {
        boxes: vec![MailboxWatchV1 {
            mailbox: [1; 32],
            read: [3; 32],
            after,
        }],
        wait_ms,
    };
    let snapshot = f.send(0, vec![request(None, 0)])?;
    let replies = f.until(snapshot).await?;
    let Reply::NoticeObjectV6 {
        notices,
        object: None,
    } = &replies[0]
    else {
        anyhow::bail!("initial watch")
    };
    let watch = f.send(1, vec![request(Some(notices[0].cursor), 10_000)])?;
    let before = Instant::now();
    while before.elapsed() < Duration::from_millis(40) {
        f.tick().await?;
    }
    assert!(f.received.is_empty());
    assert_eq!(f.jobs.len(), 1);
    let put = f.send(
        2,
        vec![Request::Put {
            mailbox: [1; 32],
            write: [2; 32],
            object: [4; 32],
            ciphertext: vec![7; 9000],
            expires,
        }],
    )?;
    assert!(matches!(f.until(put).await?.as_slice(), [Reply::Stored(_)])); // adapter independently verified signature
    let notices = f.until(watch).await?;
    assert!(
        matches!(&notices[0],Reply::NoticeObjectV6{object:Some(o),..}if o.object==[4;32]&&o.ciphertext==vec![7;9000])
    );
    ensure!(
        before.elapsed() < Duration::from_secs(5),
        "Put serialized behind 10s watch"
    );
    assert_eq!(f.client_link.session_id(0), f.server_link.session_id(0));
    let fetch = f.send(
        0,
        vec![Request::Fetch {
            mailbox: [1; 32],
            read: [3; 32],
        }],
    )?;
    assert!(matches!(
        f.until(fetch).await?.as_slice(),
        [Reply::Object { .. }]
    ));
    assert_eq!(fetch.sequence, 3);
    // A locally prepared but unsent request cannot be called completed. Expiry
    // latches only that slot; no implicit reconnect or application replay.
    let pending = f.client.begin(
        3,
        vec![Request::Fetch {
            mailbox: [1; 32],
            read: [3; 32],
        }],
        Instant::now(),
    )?;
    assert_eq!(
        f.client.expire(Instant::now() + REQUEST_DEADLINE),
        vec![pending]
    );
    assert!(f
        .client
        .begin(
            3,
            vec![Request::Fetch {
                mailbox: [1; 32],
                read: [3; 32]
            }],
            Instant::now()
        )
        .is_err());
    let good = f.send(
        0,
        vec![Request::Fetch {
            mailbox: [1; 32],
            read: [3; 32],
        }],
    )?;
    assert!(matches!(
        f.until(good).await?.as_slice(),
        [Reply::Object { .. }]
    ));
    Ok(())
}

#[tokio::test]
async fn client_invalidation_stops_enqueued_bytes_and_cannot_restart_owner() -> Result<()> {
    let mut f = Fixture::new().await?;
    let session = f.client_link.session_id(0);
    let id = f.send(
        0,
        vec![Request::Register {
            mailbox: [1; 32],
            write: [2; 32],
            read: [3; 32],
            expires: i64::MAX,
        }],
    )?;
    assert_eq!(f.client_link.outbound_usage(STREAM)?.0, 1);
    f.client.invalidate();
    let report = f.client_link.poll(1000, Instant::now(), 64)?;
    assert_eq!(report.data_sent, 0);
    assert_eq!(report.completed.len(), 1);
    assert_eq!(
        report.completed[0].state,
        novovm_network::novorudp::object_sender::ObjectSendState::Cancelled
    );
    assert_eq!(f.client_link.outbound_usage(STREAM)?, (0, 0));
    assert_eq!(f.client_link.session_id(0), session);
    assert!(f
        .client
        .try_send(&mut f.client_link, id, Instant::now(), || Ok(()))
        .is_err());
    assert!(
        NativeMailboxClient::attach(&mut f.client_link, 0, f.server_link.local_identity()).is_err()
    );
    Ok(())
}

#[tokio::test]
async fn service_revocation_cancels_queued_reply_but_preserves_real_stored_ciphertext() -> Result<()>
{
    let mut f = Fixture::new().await?;
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64
        + 600;
    let registration = f.send(
        0,
        vec![Request::Register {
            mailbox: [1; 32],
            write: [2; 32],
            read: [3; 32],
            expires,
        }],
    )?;
    assert!(matches!(
        f.until(registration).await?.as_slice(),
        [Reply::Registered]
    ));
    let id = f.send(
        0,
        vec![Request::Put {
            mailbox: [1; 32],
            write: [2; 32],
            object: [4; 32],
            ciphertext: vec![9; 9000],
            expires,
        }],
    )?;
    let body = f.receive_request().await?;
    let work = f
        .server
        .admit(&f.server_link, body, Instant::now(), || Ok(()))?;
    let mut response = work.execute(&f.service, || Ok(())).await?;
    assert!(response.try_send(&mut f.server_link, Instant::now(), || Ok(()))?);
    f.server.invalidate(); // revoke AFTER enqueue, BEFORE network send
    let report = f.server_link.poll(1000, Instant::now(), 64)?;
    assert_eq!(report.data_sent, 0);
    assert!(report
        .completed
        .iter()
        .any(|done| done.state
            == novovm_network::novorudp::object_sender::ObjectSendState::Cancelled));
    assert_eq!(f.server_link.outbound_usage(STREAM)?, (0, 0));
    assert!(response
        .try_send(&mut f.server_link, Instant::now(), || Ok(()))
        .is_err());
    assert!(!f.received.iter().any(|(received, _)| *received == id));
    // Actual existing mailbox custody remains; local cancellation is not rollback.
    assert!(matches!(f.service.request_local_v1(Request::Fetch {
        mailbox: [1; 32], read: [3; 32],
    }).await?, Reply::Object { ciphertext, .. } if ciphertext == vec![9; 9000]));
    Ok(())
}

#[tokio::test]
async fn dropped_client_stops_retained_requests_without_waiting_for_transport_ttl() -> Result<()> {
    let mut f = Fixture::new().await?;
    f.send(
        0,
        vec![Request::Fetch {
            mailbox: [1; 32],
            read: [3; 32],
        }],
    )?;
    drop(f.client);
    let report = f.client_link.poll(1000, Instant::now(), 64)?;
    assert_eq!(report.data_sent, 0);
    assert_eq!(report.completed.len(), 1);
    assert_eq!(
        report.completed[0].state,
        novovm_network::novorudp::object_sender::ObjectSendState::Cancelled
    );
    assert_eq!(f.client_link.outbound_usage(STREAM)?, (0, 0));
    Ok(())
}

#[tokio::test]
async fn response_cancel_after_enqueue_is_latched_but_other_slot_keeps_working() -> Result<()> {
    let mut f = Fixture::new().await?;
    f.send(
        0,
        vec![Request::Register {
            mailbox: [1; 32],
            write: [2; 32],
            read: [3; 32],
            expires: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs() as i64
                + 600,
        }],
    )?;
    let body = f.receive_request().await?;
    let work = f
        .server
        .admit(&f.server_link, body, Instant::now(), || Ok(()))?;
    let mut response = work.execute(&f.service, || Ok(())).await?;
    assert!(response
        .try_send(&mut f.server_link, Instant::now(), || Ok(()))
        .is_ok_and(|v| v));
    assert!(response
        .try_send(&mut f.server_link, Instant::now(), || anyhow::bail!(
            "operation cancelled"
        ))
        .is_err());
    assert!(response
        .try_send(&mut f.server_link, Instant::now(), || Ok(()))
        .is_err());
    let report = f.server_link.poll(1000, Instant::now(), 64)?;
    assert_eq!(report.data_sent, 0);
    assert!(report
        .completed
        .iter()
        .any(|done| done.state
            == novovm_network::novorudp::object_sender::ObjectSendState::Cancelled));
    let next = f.send(
        1,
        vec![Request::Fetch {
            mailbox: [1; 32],
            read: [3; 32],
        }],
    )?;
    assert!(matches!(f.until(next).await?.as_slice(), [Reply::Empty]));
    Ok(())
}

fn capacity_controller(
    pressure: novovm_network::service_concurrency::ResourcePressureV1,
) -> Arc<novovm_network::service_concurrency::ServiceConcurrencyV1> {
    use novovm_network::service_concurrency::*;
    ServiceConcurrencyV1::new(ResourceSnapshotV1 {
        cpu_parallelism: 8,
        available_memory_bytes: 16 * 1024 * 1024 * 1024,
        pressure,
    })
}
fn capacity_lane() -> NativeLaneConfig {
    let mut result = lane();
    {
        result.receive.messages = 64;
        result.receive.peer_messages = 64;
        result.receive.bytes = LIMITS.wire_bytes * 64;
        result.receive.peer_bytes = LIMITS.wire_bytes * 64;
        result.send.messages = 64;
        result.send.peer_messages = 64;
        result.send.bytes = LIMITS.wire_bytes * 64;
        result.send.peer_bytes = LIMITS.wire_bytes * 64;
    }
    result
}
#[tokio::test]
async fn adaptive_capacity_negotiates_eight_simultaneous_requests_on_one_authenticated_session(
) -> Result<()> {
    let mut f = Fixture::new().await?;
    let original = f.client_link.session_id(0);
    let deadline = Instant::now() + Duration::from_secs(5);
    while f.client.slot_capacity() < 8 {
        ensure!(Instant::now() < deadline, "capacity negotiation timeout");
        f.tick().await?;
    }
    let mut ids = Vec::new();
    for slot in 0..8 {
        ids.push(f.send(
            slot,
            vec![Request::Register {
                    mailbox: [slot as u8 + 1; 32],
                    write: [51; 32],
                    read: [52; 32],
                    expires: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_secs() as i64
                        + 600,
                }],
        )?);
    }
    // All eight sends are admitted before consuming any response.
    assert!(f.received.is_empty());
    for id in ids {
        assert!(matches!(f.until(id).await?.as_slice(), [Reply::Registered]));
    }
    assert_eq!(f.client_link.session_id(0), original);
    Ok(())
}
#[tokio::test]
async fn adaptive_capacity_negotiates_device_metadata_during_temporary_severe_pressure(
) -> Result<()> {
    let mut f =
        Fixture::new_with_pressure(novovm_network::service_concurrency::ResourcePressureV1::Severe)
            .await?;
    let original = f.client_link.session_id(0);
    let deadline = Instant::now() + Duration::from_secs(5);
    while f.client.slot_capacity() < 8 {
        ensure!(Instant::now() < deadline, "capacity negotiation timeout");
        f.tick().await?;
    }
    assert_eq!(f.client.slot_capacity(), 16);
    assert_eq!(f.client_link.session_id(0), original);
    // This negotiation allocates metadata only. native_requests capacity tests
    // separately verify live pressure blocks extras until it clears.
    Ok(())
}
#[tokio::test]
async fn adaptive_capacity_drop_replay_and_wrong_identity_do_not_reset_business_sequences(
) -> Result<()> {
    use novovm_network::novorudp::fragments::CompletedMessage;
    let mut f = Fixture::new().await?;
    f.client
        .progress_capacity(&mut f.client_link, Instant::now())?;
    let body = f.receive_request().await?;
    let copy = || CompletedMessage {
        peer: body.peer.clone(),
        id: body.id,
        chunks: body.chunks.clone(),
    };
    let mut wrong = copy();
    wrong.peer = "wrong authenticated peer".into();
    assert!(f
        .server
        .admit(&f.server_link, wrong, Instant::now(), || Ok(()))
        .is_err());
    let first = f
        .server
        .admit(&f.server_link, copy(), Instant::now(), || Ok(()))?;
    assert!(first.is_capacity_negotiation());
    drop(first.execute(&f.service, || Ok(())).await?); // Lost application response.
    assert_eq!(f.client.slot_capacity(), 4);
    let id = f.send(
        0,
        vec![Request::Register {
            mailbox: [11; 32],
            write: [12; 32],
            read: [13; 32],
            expires: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs() as i64
                + 600,
        }],
    )?;
    assert!(matches!(f.until(id).await?.as_slice(), [Reply::Registered]));
    let replay = f
        .server
        .admit(&f.server_link, copy(), Instant::now(), || Ok(()))?;
    f.responses
        .push_back(replay.execute(&f.service, || Ok(())).await?);
    let deadline = Instant::now() + Duration::from_secs(5);
    while f.client.slot_capacity() < 8 {
        ensure!(
            Instant::now() < deadline,
            "idempotent capacity reply timeout"
        );
        f.tick().await?;
    }
    let next = f.send(0, vec![Request::ProbeBulkV1])?;
    assert_eq!(next.sequence, id.sequence + 1);
    assert!(matches!(
        f.until(next).await?.as_slice(),
        [Reply::BulkReady { .. }]
    ));
    let mut other = Fixture::new().await?;
    assert!(other
        .server
        .admit(&other.server_link, copy(), Instant::now(), || Ok(()))
        .is_err());
    Ok(())
}

#[tokio::test]
async fn resource_negotiation_missing_reply_is_a_bounded_typed_failure() -> Result<()> {
    let mut f = Fixture::new().await?;
    let deadline = f
        .client
        .capacity_deadline()
        .expect("resource exchange required");
    let error = f
        .client
        .progress_capacity(&mut f.client_link, deadline)
        .unwrap_err();
    assert!(error.is::<CapacityNegotiationExpired>());
    assert!(f.client.negotiated_retained_bytes().is_none());
    assert_eq!(f.client.slot_capacity(), SLOTS);
    // No directory, durable data, business request or sequence was created.
    assert_eq!(f.admitted, 0);
    Ok(())
}
