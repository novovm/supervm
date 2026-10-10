//! Optional reliable carrier. Neither direct QUIC nor a single relay is anonymous.
//! The caller must authorize this scope BEFORE bind, DNS, or discovery. No N0
//! presets, address publication, port mapping, or implicit public DNS are enabled.
//! NOVOVM application identity and E2E authentication remain a separate layer.

pub use crate::duplex::{decode_iroh_envelope_v1, encode_iroh_envelope_v1};
use crate::transport_binding::{
    VerifiedTransportBinding, BOUND_TRANSPORT_ALPN_V1, BOUND_TRANSPORT_EXPORTER_CONTEXT_V1,
    BOUND_TRANSPORT_EXPORTER_LABEL_V1,
};
use anyhow::{bail, ensure, Context, Result};
use iroh::{
    dns::{DnsResolver, NameserverConfig},
    endpoint::{
        presets, Connection, PortmapperConfig, QuicTransportConfig, RecvStream, SendStream,
    },
    Endpoint, EndpointAddr, EndpointId, NetReportConfig, RelayMode, RelayUrl, TransportAddr,
};
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, Instant},
};

pub const IROH_ALPN_V1: &[u8] = BOUND_TRANSPORT_ALPN_V1;
pub const IROH_MAX_FRAME_V1: usize = 16 * 1024;
/// Adapter-owned active connections, including outbound attempts and inbound
/// handshakes. This is NOT iroh's internal pending-Incoming queue limit.
pub const IROH_MAX_CONNECTIONS_V1: usize = 4;
const CHECK_INTERVAL: Duration = Duration::from_millis(20);
const INCOMING_SETUP_TIMEOUT: Duration = Duration::from_secs(3);
static ENDPOINT_LIMIT: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();

/// Fast local authorization/cancellation check. Never do RPC or blocking IO here.
#[derive(Clone)]
pub struct IrohControlV1 {
    check: Arc<dyn Fn() -> Result<()> + Send + Sync>,
    deadline: Instant,
}

impl IrohControlV1 {
    pub fn new(
        deadline: Instant,
        check: impl Fn() -> Result<()> + Send + Sync + 'static,
    ) -> Result<Self> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        ensure!(
            !remaining.is_zero() && remaining <= Duration::from_secs(120),
            "carrier scope requires a remaining lease of at most 120 seconds"
        );
        Ok(Self {
            check: Arc::new(check),
            deadline,
        })
    }

    fn check(&self) -> Result<()> {
        (self.check)()?;
        ensure!(Instant::now() < self.deadline, "carrier scope expired");
        Ok(())
    }

    async fn revoked(&self) -> anyhow::Error {
        loop {
            if let Err(error) = self.check() {
                return error;
            }
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    }
}

/// Explicit operator configuration, not an untrusted peer's relay list.
pub struct IrohConfigV1 {
    relays: Vec<RelayUrl>,
    nameserver: IpAddr,
    bind_address: Option<SocketAddr>,
}

impl IrohConfigV1 {
    pub fn new(relays: &[String], nameserver: IpAddr) -> Result<Self> {
        ensure!(relays.len() <= 4, "too many relay services");
        ensure!(
            !nameserver.is_unspecified() && !nameserver.is_multicast(),
            "invalid DNS server"
        );
        let mut parsed = Vec::new();
        for value in relays {
            ensure!(value.len() <= 512, "relay URL too long");
            let relay: RelayUrl = value.parse()?;
            ensure!(
                relay.scheme() == "https"
                    && relay.username().is_empty()
                    && relay.password().is_none()
                    && relay.query().is_none()
                    && relay.fragment().is_none()
                    && relay.path() == "/"
                    && relay.port_or_known_default() == Some(443),
                "explicit HTTPS relay on port 443 required"
            );
            ensure!(!parsed.contains(&relay), "duplicate relay");
            parsed.push(relay);
        }
        Ok(Self {
            relays: parsed,
            nameserver,
            bind_address: None,
        })
    }

    /// Optional interface binding, primarily for deterministic local tests.
    pub fn with_bind_address(mut self, address: SocketAddr) -> Result<Self> {
        ensure!(!address.ip().is_multicast(), "invalid bind address");
        self.bind_address = Some(address);
        Ok(self)
    }
}

/// Public addressing is sensitive metadata; exchange only inside an authorized
/// control channel. This object alone is NOT a signed identity/discovery ticket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrohAddressV1 {
    pub endpoint_key: [u8; 32],
    pub direct: Vec<SocketAddr>,
    pub relays: Vec<String>,
}

pub struct IrohScopeV1 {
    endpoint: Endpoint,
    control: IrohControlV1,
    relays: Vec<RelayUrl>,
    connections: tokio::sync::Semaphore,
    accept_consumer: tokio::sync::Mutex<()>,
}

/// The operation and all borrowed streams end before endpoint.close + drop.
/// Revocation interrupts the WHOLE operation, including third-party background
/// punching/relay work; returning or failing also releases the endpoint.
/// The lease is bounded, and callers must not silently renew a revoked scope.
pub async fn with_iroh_scope_v1<T, F>(
    config: IrohConfigV1,
    control: IrohControlV1,
    operation: F,
) -> Result<T>
where
    F: for<'a> FnOnce(&'a IrohScopeV1) -> Pin<Box<dyn Future<Output = Result<T>> + 'a>>,
{
    control.check()?;
    // Includes any graceful cleanup still running after its caller left. Never
    // open unbounded endpoints or accumulate a queue of waiting scopes.
    let permit = ENDPOINT_LIMIT
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(4)))
        .clone()
        .try_acquire_owned()
        .context("carrier endpoint limit")?;
    let resolver = DnsResolver::builder()
        .add_nameserver_config(NameserverConfig::udp(config.nameserver))
        .disable_fallback()
        .build();
    let mut report = NetReportConfig::default();
    report.captive_portal_check = false;
    let transport = QuicTransportConfig::builder()
        .max_concurrent_bidi_streams(1u32.into())
        .max_concurrent_uni_streams(0u32.into())
        .stream_receive_window(65536u32.into())
        .receive_window(65536u32.into())
        .send_window(128 * 1024)
        .max_remote_nat_traversal_addresses(8)
        .max_idle_timeout(Some(Duration::from_secs(15).try_into()?))
        .keep_alive_interval(Duration::from_secs(5))
        .datagram_receive_buffer_size(None)
        .build();
    let mut builder = Endpoint::builder(presets::Minimal)
        .alpns(vec![IROH_ALPN_V1.to_vec()])
        .clear_address_lookup()
        .relay_mode(if config.relays.is_empty() {
            RelayMode::Disabled
        } else {
            RelayMode::custom(config.relays.clone())
        })
        .dns_resolver(resolver)
        .portmapper_config(PortmapperConfig::Disabled)
        .net_report_config(report)
        .transport_config(transport);
    // Iroh generates a fresh independent transport key. No wallet/chat seed is
    // accepted by this API, so key reuse cannot occur through configuration.
    if let Some(address) = config.bind_address {
        builder = builder.clear_ip_transports().bind_addr(address)?;
    }
    let endpoint = tokio::select! {
        biased;
        error = control.revoked() => return Err(error),
        result = builder.bind() => result?,
    };
    let scope = IrohScopeV1 {
        endpoint,
        control,
        relays: config.relays,
        connections: tokio::sync::Semaphore::new(IROH_MAX_CONNECTIONS_V1),
        accept_consumer: tokio::sync::Mutex::new(()),
    };
    let result = tokio::select! {
        biased;
        error = scope.control.revoked() => Err(error),
        result = operation(&scope) => result,
    };
    // Include post-operation authorization: a late success cannot revive a scope.
    let result = scope.control.check().and(result);
    if result.is_err() {
        // Drop without starting close invokes iroh's abort path, cancelling all
        // actors. Do not cancel an already-started close: in 1.3.0 that leaves
        // is_closing set and prevents Drop from aborting the remaining tasks.
        drop(scope);
        tokio::task::yield_now().await;
        drop(permit);
        return result;
    }
    let final_control = scope.control.clone();
    let cleanup = tokio::spawn(async move {
        scope.endpoint.close().await;
        drop(scope);
        drop(permit);
    });
    // Timing out this JoinHandle detaches it, not its future: it keeps driving
    // close and holds the global permit. At most four such owners can exist.
    tokio::time::timeout(Duration::from_secs(5), cleanup)
        .await
        .context("carrier graceful cleanup still running")??;
    final_control.check().and(result)
}

impl IrohScopeV1 {
    pub fn address(&self) -> Result<IrohAddressV1> {
        self.control.check()?;
        let address = self.endpoint.addr();
        Ok(IrohAddressV1 {
            endpoint_key: *address.id.as_bytes(),
            direct: address.ip_addrs().take(8).copied().collect(),
            relays: address
                .relay_urls()
                .take(4)
                .map(ToString::to_string)
                .collect(),
        })
    }

    pub async fn online(&self) -> Result<()> {
        self.control.check()?;
        ensure!(
            !self.relays.is_empty(),
            "online requires a configured relay"
        );
        self.endpoint.online().await;
        self.control.check()
    }

    fn peer_address(&self, peer: &IrohAddressV1) -> Result<EndpointAddr> {
        self.control.check()?;
        ensure!(
            peer.direct.len() <= 8 && peer.relays.len() <= 4,
            "peer candidate limit"
        );
        let id = EndpointId::from_bytes(&peer.endpoint_key)?;
        ensure!(id != self.endpoint.id(), "self connection rejected");
        let mut addresses = Vec::new();
        for address in &peer.direct {
            ensure!(
                address.port() != 0
                    && !address.ip().is_unspecified()
                    && !address.ip().is_multicast(),
                "invalid candidate"
            );
            addresses.push(TransportAddr::Ip(*address));
        }
        for relay in &peer.relays {
            ensure!(relay.len() <= 512, "relay candidate too long");
            let relay: RelayUrl = relay.parse()?;
            ensure!(
                self.relays.contains(&relay),
                "peer relay not locally approved"
            );
            addresses.push(TransportAddr::Relay(relay));
        }
        ensure!(!addresses.is_empty(), "no approved peer candidate");
        Ok(EndpointAddr::from_parts(id, addresses))
    }

    pub async fn connect(&self, peer: &IrohAddressV1) -> Result<IrohStreamV1<'_>> {
        let address = self.peer_address(peer)?;
        // Reserve before dialing: outbound work shares the same active budget
        // as inbound TLS handshakes and streams, without a waiting queue.
        let admission = self
            .connections
            .try_acquire()
            .context("carrier connection limit")?;
        let connection = self.endpoint.connect(address, IROH_ALPN_V1).await?;
        let pending = PendingConnection::new(connection);
        tokio::time::timeout(
            INCOMING_SETUP_TIMEOUT,
            self.stream(pending, peer.endpoint_key, true, admission),
        )
        .await
        .context("carrier stream setup deadline")?
    }

    /// Accept a known TLS endpoint. Unknown endpoints are closed, never treated
    /// as an invitation to change the expected application identity.
    pub async fn accept(&self, expected_endpoint: [u8; 32]) -> Result<IrohStreamV1<'_>> {
        self.control.check()?;
        EndpointId::from_bytes(&expected_endpoint)?;
        self.accept_inner(Some(expected_endpoint)).await
    }

    /// Accept a TLS-authenticated temporary endpoint whose key is not known in
    /// advance. This grants NO NOVOVM/application identity or rendezvous access.
    /// The caller must verify an independently authorized application pin and
    /// both Finished records using this stream's binding before using its data.
    ///
    /// Only one accept future may consume a scope's incoming queue. Active
    /// admission is shared with connect(); excess Incoming objects are refused
    /// before starting TLS. iroh 1.3 does not expose a Builder setter for its
    /// endpoint-wide pending-Incoming queue, whose library limit still applies.
    /// The three-second budget starts when an Incoming is dequeued, and covers
    /// TLS plus opening the first bidirectional stream, not application auth.
    pub async fn accept_inbound(&self) -> Result<IrohStreamV1<'_>> {
        self.accept_inner(None).await
    }

    async fn accept_inner(&self, expected: Option<[u8; 32]>) -> Result<IrohStreamV1<'_>> {
        self.control.check()?;
        let _consumer = self
            .accept_consumer
            .try_lock()
            .context("carrier accept already pending")?;
        // The same consumer guard covers both accept APIs and is released on
        // cancellation. Concurrent callers must not steal and reject peers.
        for _ in 0..8 {
            self.control.check()?;
            let incoming = self.endpoint.accept().await.context("endpoint closed")?;
            self.control.check()?;
            let admission = match self.connections.try_acquire() {
                Ok(admission) => admission,
                Err(error) => {
                    incoming.refuse();
                    return Err(error).context("carrier connection limit");
                }
            };
            let deadline = tokio::time::Instant::now() + INCOMING_SETUP_TIMEOUT;
            let connection = match tokio::time::timeout_at(deadline, incoming).await {
                Ok(Ok(connection)) => connection,
                _ => continue,
            };
            let remote = *connection.remote_id().as_bytes();
            let pending = PendingConnection::new(connection);
            if expected.is_some_and(|expected| expected != remote) {
                // PendingConnection also closes on timeout/cancellation before
                // the stream becomes an owned, admitted IrohStreamV1.
                pending
                    .connection
                    .as_ref()
                    .context("missing pending connection")?
                    .close(1u32.into(), b"unexpected endpoint");
                continue;
            }
            return tokio::time::timeout_at(
                deadline,
                self.stream(pending, remote, false, admission),
            )
            .await
            .context("incoming stream setup deadline")?;
        }
        bail!("incoming peer budget exhausted")
    }

    async fn stream<'a>(
        &'a self,
        mut pending: PendingConnection,
        expected: [u8; 32],
        initiator: bool,
        admission: tokio::sync::SemaphorePermit<'a>,
    ) -> Result<IrohStreamV1<'a>> {
        let connection = pending
            .connection
            .as_ref()
            .context("missing pending connection")?;
        self.control.check()?;
        ensure!(
            connection.remote_id().as_bytes() == &expected && connection.alpn() == IROH_ALPN_V1,
            "transport peer/ALPN mismatch"
        );
        let (send, recv) = if initiator {
            connection.open_bi().await?
        } else {
            connection.accept_bi().await?
        };
        self.control.check()?;
        Ok(IrohStreamV1 {
            scope: self,
            connection: pending
                .connection
                .take()
                .context("missing pending connection")?,
            send,
            recv,
            failed: AtomicBool::new(false),
            initiator,
            _admission: admission,
        })
    }
}

/// Owns an established TLS connection before its stream is returned. In
/// particular, cancelling accept_bi/open_bi must not leave an unadmitted live
/// connection in the scope. Constructed before polling the setup future so an
/// already-expired timeout closes it too.
struct PendingConnection {
    connection: Option<Connection>,
}

impl PendingConnection {
    fn new(connection: Connection) -> Self {
        Self {
            connection: Some(connection),
        }
    }
}

impl Drop for PendingConnection {
    fn drop(&mut self) {
        if let Some(connection) = &self.connection {
            connection.close(1u32.into(), b"connection setup cancelled");
        }
    }
}

pub struct IrohStreamV1<'a> {
    scope: &'a IrohScopeV1,
    connection: Connection,
    send: SendStream,
    recv: RecvStream,
    failed: AtomicBool,
    initiator: bool,
    _admission: tokio::sync::SemaphorePermit<'a>,
}

/// A borrowed framed writer. The parent stream and its authorized scope remain
/// alive until both halves are released. Dropping an in-progress write closes
/// the entire connection; restarting a partly-written frame is never allowed.
pub struct IrohWriteHalfV1<'a> {
    scope: &'a IrohScopeV1,
    connection: &'a Connection,
    send: &'a mut SendStream,
    failed: &'a AtomicBool,
}

/// A borrowed framed reader, independent of the writer's mutable borrow. Keep
/// one read future alive across application-level waits; cancelling it poisons
/// both directions rather than forgetting a partially consumed frame.
pub struct IrohReadHalfV1<'a> {
    scope: &'a IrohScopeV1,
    connection: &'a Connection,
    recv: &'a mut RecvStream,
    failed: &'a AtomicBool,
}

struct FrameIoGuard<'a> {
    connection: &'a Connection,
    failed: &'a AtomicBool,
    complete: bool,
}
impl Drop for FrameIoGuard<'_> {
    fn drop(&mut self) {
        if !self.complete {
            // This is a permanent, shared terminal latch, never an IO-busy bit.
            // A successful concurrent operation must not clear this failure.
            self.failed.store(true, Ordering::Release);
            self.connection
                .close(1u32.into(), b"frame operation cancelled");
        }
    }
}

fn check_frame_io(scope: &IrohScopeV1, connection: &Connection, failed: &AtomicBool) -> Result<()> {
    ensure!(!failed.load(Ordering::Acquire), "carrier stream failed");
    if let Err(error) = scope.control.check() {
        failed.store(true, Ordering::Release);
        connection.close(1u32.into(), b"frame authorization revoked");
        return Err(error);
    }
    // The other half can fail while the local authority callback is running.
    ensure!(!failed.load(Ordering::Acquire), "carrier stream failed");
    Ok(())
}

async fn write_frame_io(
    scope: &IrohScopeV1,
    connection: &Connection,
    failed: &AtomicBool,
    send: &mut SendStream,
    bytes: &[u8],
) -> Result<()> {
    check_frame_io(scope, connection, failed)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= IROH_MAX_FRAME_V1,
        "carrier frame limit"
    );
    let mut guard = FrameIoGuard {
        connection,
        failed,
        complete: false,
    };
    send.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
    send.write_all(bytes).await?;
    check_frame_io(scope, connection, failed)?;
    guard.complete = true;
    Ok(())
}

async fn read_frame_io(
    scope: &IrohScopeV1,
    connection: &Connection,
    failed: &AtomicBool,
    recv: &mut RecvStream,
) -> Result<Vec<u8>> {
    check_frame_io(scope, connection, failed)?;
    let mut guard = FrameIoGuard {
        connection,
        failed,
        complete: false,
    };
    let mut length = [0u8; 4];
    recv.read_exact(&mut length).await?;
    let length = u32::from_be_bytes(length) as usize;
    ensure!(
        length > 0 && length <= IROH_MAX_FRAME_V1,
        "carrier frame limit"
    );
    let mut bytes = vec![0; length];
    recv.read_exact(&mut bytes).await?;
    check_frame_io(scope, connection, failed)?;
    guard.complete = true;
    Ok(bytes)
}

impl IrohWriteHalfV1<'_> {
    pub async fn write_frame(&mut self, bytes: &[u8]) -> Result<()> {
        write_frame_io(self.scope, self.connection, self.failed, self.send, bytes).await
    }
}

impl IrohReadHalfV1<'_> {
    pub async fn read_frame(&mut self) -> Result<Vec<u8>> {
        read_frame_io(self.scope, self.connection, self.failed, self.recv).await
    }
}

impl IrohStreamV1<'_> {
    fn check(&self) -> Result<()> {
        check_frame_io(self.scope, &self.connection, &self.failed)
    }

    /// Split only the framed IO borrows, without transferring the connection or
    /// extending its scope. Once both halves are dropped, the original stream
    /// can be used again (including `finish`) if neither direction has failed.
    /// A half's failed/cancelled operation permanently invalidates both halves
    /// and the parent. Ordinary application waits must not cancel half-frames.
    pub fn split_io(&mut self) -> Result<(IrohWriteHalfV1<'_>, IrohReadHalfV1<'_>)> {
        self.check()?;
        Ok((
            IrohWriteHalfV1 {
                scope: self.scope,
                connection: &self.connection,
                send: &mut self.send,
                failed: &self.failed,
            },
            IrohReadHalfV1 {
                scope: self.scope,
                connection: &self.connection,
                recv: &mut self.recv,
                failed: &self.failed,
            },
        ))
    }

    /// The identity proved by TLS, not a NOVOVM identity or an application pin.
    /// Never promote this temporary transport key into a trusted contact.
    pub fn remote_endpoint_key(&self) -> Result<[u8; 32]> {
        self.check()?;
        Ok(*self.connection.remote_id().as_bytes())
    }

    /// Material is only for the NOVOVM signed connection binding, never a wallet
    /// key or printable diagnostic. Both ends derive it from completed TLS.
    pub(crate) fn binding_material(&self) -> Result<([u8; 32], [u8; 32], [u8; 32])> {
        self.check()?;
        let local = *self.scope.endpoint.id().as_bytes();
        let remote = *self.connection.remote_id().as_bytes();
        let mut material = [0u8; 32];
        self.connection
            .export_keying_material(
                &mut material,
                BOUND_TRANSPORT_EXPORTER_LABEL_V1,
                BOUND_TRANSPORT_EXPORTER_CONTEXT_V1,
            )
            .map_err(|_| anyhow::anyhow!("TLS binding export failed"))?;
        Ok(if self.initiator {
            (local, remote, material)
        } else {
            (remote, local, material)
        })
    }

    pub fn binding(&self) -> Result<VerifiedTransportBinding> {
        let (initiator, responder, mut exporter) = self.binding_material()?;
        let (local, remote) = if self.initiator {
            (initiator, responder)
        } else {
            (responder, initiator)
        };
        let binding = VerifiedTransportBinding::from_authenticated_transport(
            local,
            remote,
            exporter,
            self.connection.alpn(),
            self.initiator,
        );
        zeroize::Zeroize::zeroize(&mut exporter);
        Ok(binding?)
    }

    pub fn selected_path(&self) -> Result<&'static str> {
        self.check()?;
        for path in self.connection.paths().iter() {
            if path.is_selected() {
                return Ok(if path.is_ip() {
                    "direct"
                } else if path.is_relay() {
                    "relay"
                } else {
                    "other"
                });
            }
        }
        Ok("unknown")
    }

    pub async fn write_frame(&mut self, bytes: &[u8]) -> Result<()> {
        write_frame_io(
            self.scope,
            &self.connection,
            &self.failed,
            &mut self.send,
            bytes,
        )
        .await
    }

    pub async fn read_frame(&mut self) -> Result<Vec<u8>> {
        read_frame_io(self.scope, &self.connection, &self.failed, &mut self.recv).await
    }

    /// Flush both directions before closing QUIC. Transport completion is NOT a
    /// recipient's application receipt; the application must verify that first.
    pub async fn finish(&mut self) -> Result<()> {
        self.check()?;
        let mut guard = FrameIoGuard {
            connection: &self.connection,
            failed: &self.failed,
            complete: false,
        };
        self.send.finish()?;
        let mut trailing = [0u8; 1];
        ensure!(
            self.recv.read(&mut trailing).await?.is_none(),
            "unexpected trailing stream data"
        );
        ensure!(self.send.stopped().await?.is_none(), "peer stopped stream");
        self.check()?;
        guard.complete = true;
        Ok(())
    }
}

impl Drop for IrohStreamV1<'_> {
    fn drop(&mut self) {
        self.connection.close(0u32.into(), b"scope finished");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;
    static SERIAL: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

    fn local_config() -> IrohConfigV1 {
        IrohConfigV1::new(&[], "127.0.0.1".parse().unwrap())
            .unwrap()
            .with_bind_address("127.0.0.1:0".parse().unwrap())
            .unwrap()
    }
    fn control() -> IrohControlV1 {
        IrohControlV1::new(Instant::now() + Duration::from_secs(8), || Ok(())).unwrap()
    }

    #[test]
    fn unapproved_relay_syntax_is_rejected() {
        for url in [
            "http://example.org/",
            "https://user@example.org/",
            "https://example.org/?token=x",
            "https://example.org:8443/",
            "https://example.org/old-wss-path",
        ] {
            assert!(IrohConfigV1::new(&[url.to_owned()], "127.0.0.1".parse().unwrap()).is_err());
        }
        assert!(IrohControlV1::new(Instant::now() + Duration::from_secs(121), || Ok(())).is_err());
    }

    #[tokio::test]
    async fn denied_scope_never_enters_network_operation() {
        let _permit = SERIAL.acquire().await.unwrap();
        let entered = Arc::new(AtomicBool::new(false));
        let operation_entered = entered.clone();
        let control = IrohControlV1::new(Instant::now() + Duration::from_secs(1), || {
            bail!("privacy required")
        })
        .unwrap();
        let result = with_iroh_scope_v1(local_config(), control, move |_scope| {
            Box::pin(async move {
                operation_entered.store(true, Ordering::Release);
                Ok(())
            })
        })
        .await;
        assert!(result.unwrap_err().to_string().contains("privacy required"));
        assert!(!entered.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn revoke_pending_operation_releases_its_udp_port() {
        let _permit = SERIAL.acquire().await.unwrap();
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let address = socket.local_addr().unwrap();
        drop(socket);
        let cancelled = Arc::new(AtomicBool::new(false));
        let probe_cancel = cancelled.clone();
        let gate = IrohControlV1::new(Instant::now() + Duration::from_secs(5), move || {
            ensure!(!probe_cancel.load(Ordering::Acquire), "scope revoked");
            Ok(())
        })
        .unwrap();
        let (ready, started) = oneshot::channel();
        let operation = with_iroh_scope_v1(
            local_config().with_bind_address(address).unwrap(),
            gate,
            move |_scope| {
                Box::pin(async move {
                    ready.send(()).unwrap();
                    std::future::pending::<Result<()>>().await
                })
            },
        );
        let trigger = async {
            started.await.unwrap();
            assert!(std::net::UdpSocket::bind(address).is_err());
            cancelled.store(true, Ordering::Release);
        };
        let (result, ()) = tokio::join!(operation, trigger);
        assert!(result.unwrap_err().to_string().contains("scope revoked"));
        let release_deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if std::net::UdpSocket::bind(address).is_ok() {
                break;
            }
            assert!(
                Instant::now() < release_deadline,
                "UDP socket retained after abort"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn reliable_frames_and_tls_binding_match_on_real_quic() {
        let _permit = SERIAL.acquire().await.unwrap();
        let (a_tx, a_rx) = oneshot::channel();
        let (b_tx, b_rx) = oneshot::channel();
        let server = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                a_tx.send(scope.address()?).unwrap();
                let peer: IrohAddressV1 = b_rx.await?;
                let mut stream = scope.accept(peer.endpoint_key).await?;
                let binding = stream.binding_material()?;
                for i in 0..8u8 {
                    let frame = stream.read_frame().await?;
                    ensure!(frame == vec![i; 13000], "large reliable frame differs");
                    stream.write_frame(&frame).await?;
                }
                let path = stream.selected_path()?;
                stream.finish().await?;
                Ok((binding, path))
            })
        });
        let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                b_tx.send(scope.address()?).unwrap();
                let mut stream = scope.connect(&a_rx.await?).await?;
                let binding = stream.binding_material()?;
                for i in 0..8u8 {
                    let frame = vec![i; 13000];
                    stream.write_frame(&frame).await?;
                    ensure!(stream.read_frame().await? == frame, "reliable echo differs");
                }
                let path = stream.selected_path()?;
                stream.finish().await?;
                Ok((binding, path))
            })
        });
        let (a, b) = tokio::join!(server, client);
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_eq!(a, b);
        assert_eq!(a.1, "direct");
        assert_ne!(a.0 .0, a.0 .1);
    }

    #[tokio::test]
    async fn inbound_unknown_endpoint_has_the_same_tls_binding_without_a_client_bootstrap() {
        let _permit = SERIAL.acquire().await.unwrap();
        let (address_tx, address_rx) = oneshot::channel();
        let server = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                address_tx.send(scope.address()?).unwrap();
                // No client endpoint key or address is supplied to the listener.
                let mut stream = scope.accept_inbound().await?;
                let remote = stream.remote_endpoint_key()?;
                let binding = stream.binding_material()?;
                let _proof = stream.binding()?;
                // Transport-only test bytes; no application identity or access
                // is granted by these bytes or by accept_inbound().
                ensure!(
                    stream.read_frame().await? == b"transport probe",
                    "wrong probe"
                );
                stream.write_frame(b"transport reply").await?;
                stream.finish().await?;
                Ok((remote, binding))
            })
        });
        let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                let own_endpoint = scope.address()?.endpoint_key;
                let mut stream = scope.connect(&address_rx.await?).await?;
                let binding = stream.binding_material()?;
                stream.write_frame(b"transport probe").await?;
                ensure!(
                    stream.read_frame().await? == b"transport reply",
                    "wrong reply"
                );
                stream.finish().await?;
                Ok((own_endpoint, binding))
            })
        });
        let (server, client) = tokio::join!(server, client);
        assert_eq!(server.unwrap(), client.unwrap());
    }

    #[tokio::test]
    async fn concurrent_known_and_unknown_accepts_fail_without_consuming_each_others_peers() {
        let _permit = SERIAL.acquire().await.unwrap();
        with_iroh_scope_v1(local_config(), control(), |scope| {
            Box::pin(async move {
                let expected = *iroh::SecretKey::generate().public().as_bytes();
                let mut first = Box::pin(scope.accept_inbound());
                // Poll the first consumer into its actual endpoint wait.
                tokio::select! {
                    biased;
                    _ = &mut first => bail!("unexpected incoming connection"),
                    _ = async {} => {},
                }
                let error = scope
                    .accept(expected)
                    .await
                    .err()
                    .context("second accept succeeded")?;
                ensure!(
                    error.to_string().contains("accept already pending"),
                    "wrong admission error"
                );
                drop(first);
                // Cancelling the first waiter must release the same guard used
                // by both APIs, with no reserved connection left behind.
                let mut next = Box::pin(scope.accept(expected));
                tokio::select! {
                    biased;
                    _ = &mut next => bail!("consumer remained blocked after cancellation"),
                    _ = async {} => {},
                }
                let error = scope
                    .accept_inbound()
                    .await
                    .err()
                    .context("third accept succeeded")?;
                ensure!(
                    error.to_string().contains("accept already pending"),
                    "wrong admission error"
                );
                drop(next);
                ensure!(
                    scope.accept_consumer.try_lock().is_ok(),
                    "consumer guard leaked"
                );
                ensure!(
                    scope.connections.available_permits() == IROH_MAX_CONNECTIONS_V1,
                    "connection permit leaked"
                );
                Ok(())
            })
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn known_endpoint_accept_still_closes_a_different_tls_peer() {
        let _permit = SERIAL.acquire().await.unwrap();
        let (address_tx, address_rx) = oneshot::channel();
        let (rejected_tx, rejected_rx) = oneshot::channel();
        let server = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                address_tx.send(scope.address()?).unwrap();
                let expected = *iroh::SecretKey::generate().public().as_bytes();
                let mut accepting = Box::pin(scope.accept(expected));
                tokio::select! {
                    _ = &mut accepting => bail!("wrong endpoint ended the known-peer wait"),
                    result = rejected_rx => result?,
                }
                drop(accepting);
                ensure!(
                    scope.connections.available_permits() == IROH_MAX_CONNECTIONS_V1,
                    "rejected peer retained a permit"
                );
                Ok(())
            })
        });
        let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                let reason = match scope.connect(&address_rx.await?).await {
                    Ok(stream) => stream.connection.closed().await.to_string(),
                    Err(error) => format!("{error:#}"),
                };
                ensure!(
                    reason.contains("unexpected endpoint"),
                    "peer was not rejected for its endpoint pin: {reason}"
                );
                rejected_tx.send(()).unwrap();
                Ok(())
            })
        });
        let (server, client) = tokio::join!(server, client);
        server.unwrap();
        client.unwrap();
    }

    #[tokio::test]
    async fn active_admission_bounds_inbound_and_outbound_and_releases_on_drop() {
        let _permit = SERIAL.acquire().await.unwrap();
        let (address_tx, address_rx) = oneshot::channel();
        let (extra_address_tx, extra_address_rx) = oneshot::channel();
        let (client_address_tx, client_address_rx) = oneshot::channel();
        let (full_tx, full_rx) = oneshot::channel();
        let (finish_tx, finish_rx) = oneshot::channel();
        let server = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                let address = scope.address()?;
                address_tx.send(address.clone()).unwrap();
                extra_address_tx.send(address).unwrap();
                let client_address = client_address_rx.await?;
                let mut streams = Vec::new();
                for _ in 0..IROH_MAX_CONNECTIONS_V1 {
                    let mut stream = scope.accept_inbound().await?;
                    ensure!(
                        stream.read_frame().await? == b"hold",
                        "missing held connection"
                    );
                    streams.push(stream);
                }
                ensure!(
                    scope.connections.available_permits() == 0,
                    "active cap not reserved"
                );
                let error = scope
                    .connect(&client_address)
                    .await
                    .err()
                    .context("inbound sessions did not consume outbound capacity")?;
                ensure!(
                    error.to_string().contains("connection limit"),
                    "inbound and outbound admission are not shared"
                );
                full_tx.send(()).unwrap();
                let error = scope
                    .accept_inbound()
                    .await
                    .err()
                    .context("excess inbound accepted")?;
                ensure!(
                    error.to_string().contains("connection limit"),
                    "wrong capacity error"
                );
                finish_tx.send(()).unwrap();
                for mut stream in streams {
                    stream.finish().await?;
                }
                ensure!(
                    scope.connections.available_permits() == IROH_MAX_CONNECTIONS_V1,
                    "inbound capacity retained after drop"
                );
                Ok(())
            })
        });
        let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                let address = address_rx.await?;
                client_address_tx.send(scope.address()?).unwrap();
                let mut streams = Vec::new();
                for _ in 0..IROH_MAX_CONNECTIONS_V1 {
                    let mut stream = scope.connect(&address).await?;
                    stream.write_frame(b"hold").await?;
                    streams.push(stream);
                }
                let error = scope
                    .connect(&address)
                    .await
                    .err()
                    .context("excess outbound started")?;
                ensure!(
                    error.to_string().contains("connection limit"),
                    "wrong capacity error"
                );
                finish_rx.await?;
                for mut stream in streams {
                    stream.finish().await?;
                }
                ensure!(
                    scope.connections.available_permits() == IROH_MAX_CONNECTIONS_V1,
                    "outbound capacity retained after drop"
                );
                Ok(())
            })
        });
        let extra = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                let address = extra_address_rx.await?;
                full_rx.await?;
                ensure!(
                    scope.connect(&address).await.is_err(),
                    "full server admitted an extra connection"
                );
                ensure!(
                    scope.connections.available_permits() == IROH_MAX_CONNECTIONS_V1,
                    "failed dial retained a permit"
                );
                Ok(())
            })
        });
        let (server, client, extra) = tokio::join!(server, client, extra);
        server.unwrap();
        client.unwrap();
        extra.unwrap();
    }

    #[tokio::test]
    async fn inbound_peer_that_never_opens_a_stream_is_closed_at_setup_deadline() {
        let _permit = SERIAL.acquire().await.unwrap();
        let (address_tx, address_rx) = oneshot::channel();
        let server = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                address_tx.send(scope.address()?).unwrap();
                let error = scope
                    .accept_inbound()
                    .await
                    .err()
                    .context("peer without stream was accepted")?;
                ensure!(
                    error.to_string().contains("incoming stream setup deadline"),
                    "wrong deadline error"
                );
                ensure!(
                    scope.connections.available_permits() == IROH_MAX_CONNECTIONS_V1,
                    "timed out setup retained a permit"
                );
                ensure!(
                    scope.accept_consumer.try_lock().is_ok(),
                    "timed out setup retained the consumer"
                );
                Ok(())
            })
        });
        let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                // Deliberately bypass only the client stream-opening helper in
                // this local test: complete real TLS but open no QUIC stream.
                let peer = scope.peer_address(&address_rx.await?)?;
                let connection = scope.endpoint.connect(peer, IROH_ALPN_V1).await?;
                let reason = connection.closed().await.to_string();
                ensure!(
                    reason.contains("connection setup cancelled"),
                    "setup cancellation did not close TLS: {reason}"
                );
                Ok(())
            })
        });
        let (server, client) = tokio::join!(server, client);
        server.unwrap();
        client.unwrap();
    }

    #[tokio::test]
    async fn peer_cannot_supply_an_unapproved_relay() {
        let _permit = SERIAL.acquire().await.unwrap();
        with_iroh_scope_v1(local_config(), control(), |scope| {
            Box::pin(async move {
                let peer = IrohAddressV1 {
                    endpoint_key: *iroh::SecretKey::generate().public().as_bytes(),
                    direct: vec![],
                    relays: vec!["https://unapproved.example/".into()],
                };
                assert!(scope
                    .peer_address(&peer)
                    .unwrap_err()
                    .to_string()
                    .contains("not locally approved"));
                Ok(())
            })
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn cancelling_a_partial_frame_prevents_stream_reuse() {
        let _permit = SERIAL.acquire().await.unwrap();
        let (a_tx, a_rx) = oneshot::channel();
        let (b_tx, b_rx) = oneshot::channel();
        let server = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                a_tx.send(scope.address()?).unwrap();
                let peer: IrohAddressV1 = b_rx.await?;
                let mut stream = scope.accept(peer.endpoint_key).await?;
                assert!(
                    tokio::time::timeout(Duration::from_millis(70), stream.read_frame())
                        .await
                        .is_err()
                );
                assert!(stream
                    .read_frame()
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("stream failed"));
                Ok(())
            })
        });
        let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                b_tx.send(scope.address()?).unwrap();
                let mut stream = scope.connect(&a_rx.await?).await?;
                stream.send.write_all(&100u32.to_be_bytes()).await?;
                assert!(stream.read_frame().await.is_err());
                Ok(())
            })
        });
        let (a, b) = tokio::join!(server, client);
        a.unwrap();
        b.unwrap();
    }

    async fn exchange_split_frames(stream: &mut IrohStreamV1<'_>, base: u8) -> Result<()> {
        {
            let (mut writer, mut reader) = stream.split_io()?;
            let send = async {
                // More than either QUIC window: both directions must keep
                // receiving while the independent writers apply backpressure.
                for sequence in 0..24u8 {
                    writer
                        .write_frame(&vec![base + sequence; IROH_MAX_FRAME_V1])
                        .await?;
                }
                Ok::<_, anyhow::Error>(())
            };
            let receive = async {
                let remote_base = if base == 0 { 128 } else { 0 };
                for sequence in 0..24u8 {
                    ensure!(
                        reader.read_frame().await?
                            == vec![remote_base + sequence; IROH_MAX_FRAME_V1],
                        "duplex frame order/content differs"
                    );
                }
                Ok::<_, anyhow::Error>(())
            };
            tokio::try_join!(send, receive)?;
        }
        // The borrowed halves do not take ownership or make finishing depend
        // on a background task retaining an endpoint clone.
        stream.finish().await
    }

    #[tokio::test]
    async fn split_real_quic_exchanges_large_frames_simultaneously_and_finishes() {
        let _permit = SERIAL.acquire().await.unwrap();
        let (a_tx, a_rx) = oneshot::channel();
        let (b_tx, b_rx) = oneshot::channel();
        let server = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                a_tx.send(scope.address()?).unwrap();
                let peer: IrohAddressV1 = b_rx.await?;
                let mut stream = scope.accept(peer.endpoint_key).await?;
                exchange_split_frames(&mut stream, 128).await
            })
        });
        let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                b_tx.send(scope.address()?).unwrap();
                let mut stream = scope.connect(&a_rx.await?).await?;
                exchange_split_frames(&mut stream, 0).await
            })
        });
        let (a, b) = tokio::join!(server, client);
        a.unwrap();
        b.unwrap();
    }

    #[tokio::test]
    async fn cancelling_split_partial_read_permanently_invalidates_writer_and_parent() {
        let _permit = SERIAL.acquire().await.unwrap();
        let (a_tx, a_rx) = oneshot::channel();
        let (b_tx, b_rx) = oneshot::channel();
        let server = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                a_tx.send(scope.address()?).unwrap();
                let peer: IrohAddressV1 = b_rx.await?;
                let mut stream = scope.accept(peer.endpoint_key).await?;
                ensure!(stream.read_frame().await? == b"ready", "missing ready");
                {
                    let (mut writer, mut reader) = stream.split_io()?;
                    // A successful concurrent write must not reset the latch
                    // set when this incomplete read is subsequently dropped.
                    let (reading, writing) = tokio::join!(
                        tokio::time::timeout(Duration::from_millis(100), reader.read_frame()),
                        writer.write_frame(b"read pending"),
                    );
                    writing?;
                    assert!(reading.is_err());
                    assert!(writer
                        .write_frame(b"must not send")
                        .await
                        .unwrap_err()
                        .to_string()
                        .contains("stream failed"));
                    assert!(reader
                        .read_frame()
                        .await
                        .unwrap_err()
                        .to_string()
                        .contains("stream failed"));
                }
                assert!(stream.split_io().is_err());
                assert!(stream
                    .finish()
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("stream failed"));
                Ok(())
            })
        });
        let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                b_tx.send(scope.address()?).unwrap();
                let mut stream = scope.connect(&a_rx.await?).await?;
                stream.write_frame(b"ready").await?;
                // Deliberately send a valid prefix and only part of its body.
                stream.send.write_all(&100u32.to_be_bytes()).await?;
                stream.send.write_all(b"partial").await?;
                ensure!(
                    stream.read_frame().await? == b"read pending",
                    "missing frame"
                );
                assert!(stream.read_frame().await.is_err());
                Ok(())
            })
        });
        let (a, b) = tokio::join!(server, client);
        a.unwrap();
        b.unwrap();
    }

    #[tokio::test]
    async fn split_frame_bounds_reject_without_io_and_revocation_is_permanent() {
        let _permit = SERIAL.acquire().await.unwrap();
        let (a_tx, a_rx) = oneshot::channel();
        let (b_tx, b_rx) = oneshot::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let gate_cancelled = cancelled.clone();
        let gate = IrohControlV1::new(Instant::now() + Duration::from_secs(8), move || {
            ensure!(!gate_cancelled.load(Ordering::Acquire), "test gate revoked");
            Ok(())
        })
        .unwrap();
        let server = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                a_tx.send(scope.address()?).unwrap();
                let peer: IrohAddressV1 = b_rx.await?;
                let mut stream = scope.accept(peer.endpoint_key).await?;
                ensure!(
                    stream.read_frame().await? == b"legal",
                    "bounds polluted wire"
                );
                stream.write_frame(b"legal received").await?;
                assert!(stream.read_frame().await.is_err());
                Ok(())
            })
        });
        let client = with_iroh_scope_v1(local_config(), gate, move |scope| {
            Box::pin(async move {
                b_tx.send(scope.address()?).unwrap();
                let mut stream = scope.connect(&a_rx.await?).await?;
                {
                    let (mut writer, mut reader) = stream.split_io()?;
                    let oversized = vec![0; IROH_MAX_FRAME_V1 + 1];
                    for frame in [&[][..], oversized.as_slice()] {
                        assert!(writer
                            .write_frame(frame)
                            .await
                            .unwrap_err()
                            .to_string()
                            .contains("frame limit"));
                    }
                    writer.write_frame(b"legal").await?;
                    ensure!(
                        reader.read_frame().await? == b"legal received",
                        "missing ack"
                    );
                    // These operations return Ready without yielding; the outer
                    // scope watchdog cannot preempt these local assertions.
                    cancelled.store(true, Ordering::Release);
                    let denied = writer.write_frame(b"forbidden").await;
                    cancelled.store(false, Ordering::Release);
                    assert!(denied
                        .unwrap_err()
                        .to_string()
                        .contains("test gate revoked"));
                    assert!(reader
                        .read_frame()
                        .await
                        .unwrap_err()
                        .to_string()
                        .contains("stream failed"));
                    assert!(writer
                        .write_frame(b"still forbidden")
                        .await
                        .unwrap_err()
                        .to_string()
                        .contains("stream failed"));
                }
                assert!(stream.split_io().is_err());
                Ok(())
            })
        });
        let (a, b) = tokio::join!(server, client);
        a.unwrap();
        b.unwrap();
    }
}
