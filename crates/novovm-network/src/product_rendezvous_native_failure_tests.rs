//! Inject failure at the actual UDP IO boundary while a real carrier session
//! continues using the same node, signer and SQLite custody.
use super::*;
use crate::{
    iroh_transport::{IrohConfigV1, IrohEndpointOwnerV1},
    mailbox_rpc::{MailboxClientSessionV2, MailboxReplyV1, MailboxRequestV1},
    mailbox_store::MailboxStore,
    novorudp::carrier::{NativeDatagramIo, NativeRoute},
    product_directory::sign_relay_record_v1,
    product_rendezvous_owned::run_self_owned_mailbox_on_endpoint_with_native_v7,
    product_service_lease::{
        request_mailbox_continuous_v4, IrohServiceEndpointV1, NativeCarrierMailboxEndpointV8,
        SelfOwnedTrustV1, ServiceLeaseBookV1, ServiceLeaseFileV1,
    },
};
use zeroize::Zeroizing;

struct FailingUdp {
    socket: UdpSocket,
    failed: Arc<AtomicBool>,
}
impl NativeDatagramIo for FailingUdp {
    fn try_receive(&self, buffer: &mut [u8]) -> std::io::Result<Option<(usize, NativeRoute)>> {
        if self.failed.load(Ordering::Acquire) {
            return Err(std::io::ErrorKind::NetworkUnreachable.into());
        }
        self.socket.try_receive(buffer)
    }
    fn try_send(&self, bytes: &[u8], route: NativeRoute) -> std::io::Result<usize> {
        self.socket.try_send(bytes, route)
    }
    fn local_udp_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}
fn config() -> Result<IrohConfigV1> {
    IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)
}

#[test]
fn only_udp_boundary_availability_failures_are_retirable() {
    let raw = anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::NetworkUnreachable));
    // An identically typed IO error from storage/authority is NOT a path error.
    assert!(!listener_io_unavailable(&raw));
    assert!(listener_io_unavailable(&mark_udp_io_failure(raw)));
    let send = NativeCarrierIoFailure(std::io::ErrorKind::AddrNotAvailable.into());
    assert!(listener_io_unavailable(&mark_udp_io_failure(send.into())));
    for kind in [
        std::io::ErrorKind::InvalidData,
        std::io::ErrorKind::PermissionDenied,
    ] {
        assert!(!listener_io_unavailable(&mark_udp_io_failure(
            std::io::Error::from(kind).into()
        )));
    }
    for message in [
        "identity rejected",
        "revoked",
        "invalid signature",
        "storage corrupted",
    ] {
        assert!(!listener_io_unavailable(&mark_udp_io_failure(
            anyhow::anyhow!(message)
        )));
    }
}

#[tokio::test]
async fn udp_runtime_failure_withdraws_locator_and_preserves_live_carrier() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let owner = IrohEndpointOwnerV1::bind(
        config()?.with_owned_service_identity(Zeroizing::new([90; 32]))?,
        || Ok(()),
    )
    .await?;
    let peer = IrohEndpointOwnerV1::bind(config()?, || Ok(())).await?;
    let stop = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicBool::new(false));
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.set_nonblocking(true)?;
    let route = NativeMailboxEndpointV7 {
        address: socket.local_addr()?,
        max_connections: 2,
        max_bytes_per_minute: 64 * 1024 * 1024,
    };
    let readiness = tokio::net::UdpSocket::from_std(socket.try_clone()?)?;
    let endpoint = NativeEndpoint::listen_clients_with_io(
        Arc::new(FailingUdp {
            socket,
            failed: failed.clone(),
        }),
        key(17),
        2,
        vec![STREAM],
        NativeHandshakeBudget {
            lifetime: REQUEST_DEADLINE,
            retry: Duration::from_millis(500),
        },
    )?;
    let listener = NativeMailboxListenerV7 {
        link: mailbox_link(endpoint, false)?,
        readiness,
        route,
        stop: stop.clone(),
    };
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open_contributor(dir.path().join("mailbox.db"), Default::default(), true)?,
        key(17),
    ));
    service.set_resources_available(true).await?;
    let journal = dir.path().join("authority.bin");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let server = run_self_owned_mailbox_on_endpoint_with_native_v7(
        &owner,
        key(17),
        stop.clone(),
        service,
        Some(listener),
        move |endpoint, until| {
            let now = now_ms()?;
            let record = sign_relay_record_v1(
                &key(17),
                "runtime-failure-fixture",
                vec![endpoint.endpoint()?],
                now,
                until,
                1,
            )?;
            let trust = SelfOwnedTrustV1 {
                version: 1,
                node_pins: [key(17).verifying_key().to_bytes()].into(),
            };
            let mut book = ServiceLeaseBookV1::new_self_owned(trust, vec![record], now)?;
            let checkpoint = book.checkpoint()?;
            let file = ServiceLeaseFileV1::create(&journal, Zeroizing::new([91; 32]), &checkpoint)?;
            book.attach_persistence(file, &checkpoint)?;
            let host = Arc::new(ServiceLeaseHostV1::new(key(17), book)?);
            host.rotate_native_carrier_mailbox_bulk_v1(true, now, until)?;
            Ok(host)
        },
        move |record| {
            tx.send(record.clone())?;
            Ok(())
        },
    );
    let client = async {
        let first = rx.recv().await.context("initial advertisement")?;
        ensure!(NativeMailboxEndpointV7::from_record(&first)?.is_some());
        let carrier = IrohServiceEndpointV1::from_record(&first)?;
        let pin = key(17).verifying_key().to_bytes();
        peer.with_continuous_service_v4(|| Ok(()), |scope| Box::pin(async move {
            let mut stream: CarrierStreamV1<'_> = scope.connect(&carrier.address).await?.into();
            request_mailbox_continuous_v4(&mut stream).await?;
            let identity = key(84);
            let mut session = MailboxClientSessionV2::begin_continuous_v4(&mut stream, &identity, pin).await?;
            let expires = (now_ms()? / 1000) as i64 + 60;
            ensure!(matches!(session.request(MailboxRequestV1::Register {
                mailbox: [1;32], write: [2;32], read: [3;32], expires,
            }).await?, MailboxReplyV1::Registered));
            let put = || MailboxRequestV1::Put {
                mailbox: [1;32], write: [2;32], object: [4;32], ciphertext: vec![9;100], expires,
            };
            let MailboxReplyV1::Stored(before) = session.request(put()).await? else { anyhow::bail!("initial Put") };
            failed.store(true, Ordering::Release);
            let next = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await?.context("withdrawal")?;
            ensure!(next.sequence > first.sequence);
            ensure!(NativeMailboxEndpointV7::from_record(&next)?.is_none());
            ensure!(NativeCarrierMailboxEndpointV8::supports_bulk_v1(&next)?);
            let surviving = IrohServiceEndpointV1::from_record(&next)?;
            ensure!(surviving.instance == carrier.instance);
            ensure!(surviving.address.endpoint_key == carrier.address.endpoint_key);
            ensure!(surviving.address.direct == carrier.address.direct);
            ensure!(surviving.address.relays == carrier.address.relays);
            // Same already-authenticated session, same persisted signed receipt.
            let MailboxReplyV1::Stored(after) = session.request(put()).await? else { anyhow::bail!("carrier stopped") };
            ensure!(before.signature == after.signature);
            ensure!(matches!(session.request(MailboxRequestV1::Fetch {
                mailbox: [1;32], read: [3;32],
            }).await?, MailboxReplyV1::Object { object, ciphertext, .. } if object == [4;32] && ciphertext == vec![9;100]));
            session.finish().await
        })).await
    };
    tokio::pin!(server);
    let result = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(15), client) => result?,
        result = &mut server => { result?; anyhow::bail!("shared owner stopped") }
    };
    stop.store(true, Ordering::Release);
    let cleanup = tokio::time::timeout(Duration::from_secs(5), &mut server).await?;
    drop(server);
    peer.close().await?;
    owner.close().await?;
    result?;
    if let Err(error) = cleanup {
        ensure!(
            error.to_string() == "service stopped by owner",
            "unexpected cleanup: {error:#}"
        );
    }
    Ok(())
}
