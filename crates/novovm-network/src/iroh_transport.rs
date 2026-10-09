//! Optional reliable carrier. Neither direct QUIC nor a single relay is anonymous.
//! The caller must authorize this scope BEFORE bind, DNS, or discovery. No N0
//! presets, address publication, port mapping, or implicit public DNS are enabled.
//! NOVOVM application identity and E2E authentication remain a separate layer.

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
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

pub const IROH_ALPN_V1: &[u8] = BOUND_TRANSPORT_ALPN_V1;
pub const IROH_MAX_FRAME_V1: usize = 16 * 1024;
const CHECK_INTERVAL: Duration = Duration::from_millis(20);
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
        let connection = self.endpoint.connect(address, IROH_ALPN_V1).await?;
        self.stream(connection, peer.endpoint_key, true).await
    }

    pub async fn accept(&self, expected_endpoint: [u8; 32]) -> Result<IrohStreamV1<'_>> {
        self.control.check()?;
        EndpointId::from_bytes(&expected_endpoint)?;
        // Bounded unwanted peers. Only one handshake is actively accepted at a
        // time. Library pending-incoming limits still apply before accept.
        for _ in 0..8 {
            let incoming = self.endpoint.accept().await.context("endpoint closed")?;
            let connection = match tokio::time::timeout(Duration::from_secs(3), incoming).await {
                Ok(Ok(connection)) => connection,
                _ => continue,
            };
            if connection.remote_id().as_bytes() != &expected_endpoint {
                connection.close(1u32.into(), b"unexpected endpoint");
                continue;
            }
            return self.stream(connection, expected_endpoint, false).await;
        }
        bail!("incoming peer budget exhausted")
    }

    async fn stream(
        &self,
        connection: Connection,
        expected: [u8; 32],
        initiator: bool,
    ) -> Result<IrohStreamV1<'_>> {
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
            connection,
            send,
            recv,
            failed: false,
            initiator,
        })
    }
}

pub struct IrohStreamV1<'a> {
    scope: &'a IrohScopeV1,
    connection: Connection,
    send: SendStream,
    recv: RecvStream,
    failed: bool,
    initiator: bool,
}

struct FrameIoGuard {
    connection: Connection,
    complete: bool,
}
impl Drop for FrameIoGuard {
    fn drop(&mut self) {
        if !self.complete {
            self.connection
                .close(1u32.into(), b"frame operation cancelled");
        }
    }
}

impl IrohStreamV1<'_> {
    fn check(&self) -> Result<()> {
        ensure!(!self.failed, "carrier stream failed");
        self.scope.control.check()
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
        self.check()?;
        ensure!(
            !bytes.is_empty() && bytes.len() <= IROH_MAX_FRAME_V1,
            "carrier frame limit"
        );
        self.failed = true; // Dropping a partly-completed future poisons this stream.
        let mut guard = FrameIoGuard {
            connection: self.connection.clone(),
            complete: false,
        };
        let result: Result<()> = async {
            self.send
                .write_all(&(bytes.len() as u32).to_be_bytes())
                .await?;
            self.send.write_all(bytes).await?;
            self.scope.control.check()
        }
        .await;
        if result.is_err() {
            self.connection.close(1u32.into(), b"write failed");
        } else {
            self.failed = false;
        }
        guard.complete = result.is_ok();
        result
    }

    pub async fn read_frame(&mut self) -> Result<Vec<u8>> {
        self.check()?;
        self.failed = true;
        let mut guard = FrameIoGuard {
            connection: self.connection.clone(),
            complete: false,
        };
        let result: Result<Vec<u8>> = async {
            let mut length = [0u8; 4];
            self.recv.read_exact(&mut length).await?;
            let length = u32::from_be_bytes(length) as usize;
            ensure!(
                length > 0 && length <= IROH_MAX_FRAME_V1,
                "carrier frame limit"
            );
            let mut bytes = vec![0; length];
            self.recv.read_exact(&mut bytes).await?;
            self.scope.control.check()?;
            Ok(bytes)
        }
        .await;
        if result.is_err() {
            self.connection.close(1u32.into(), b"read failed");
        } else {
            self.failed = false;
        }
        guard.complete = result.is_ok();
        result
    }

    /// Flush both directions before closing QUIC. Transport completion is NOT a
    /// recipient's application receipt; the application must verify that first.
    pub async fn finish(&mut self) -> Result<()> {
        self.check()?;
        self.failed = true;
        let mut guard = FrameIoGuard {
            connection: self.connection.clone(),
            complete: false,
        };
        let result: Result<()> = async {
            self.send.finish()?;
            let mut trailing = [0u8; 1];
            ensure!(
                self.recv.read(&mut trailing).await?.is_none(),
                "unexpected trailing stream data"
            );
            ensure!(self.send.stopped().await?.is_none(), "peer stopped stream");
            self.scope.control.check()
        }
        .await;
        if result.is_err() {
            self.connection.close(1u32.into(), b"finish failed");
        } else {
            self.failed = false;
        }
        guard.complete = result.is_ok();
        result
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
    use std::sync::atomic::{AtomicBool, Ordering};
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
}
