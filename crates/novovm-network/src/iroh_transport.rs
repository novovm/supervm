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
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, Instant},
};

pub const IROH_ALPN_V1: &[u8] = BOUND_TRANSPORT_ALPN_V1;
pub const IROH_MAX_FRAME_V1: usize = 16 * 1024;
/// Adapter-owned active connections, including outbound attempts and inbound
/// handshakes. This is NOT iroh's internal pending-Incoming queue limit.
pub const IROH_MAX_CONNECTIONS_V1: usize = 4;

static FORCED_DRAINS: AtomicU64 = AtomicU64::new(0);
static BINDS_COMPLETED: AtomicU64 = AtomicU64::new(0);

/// Redacted process-wide diagnostics only; never an application receipt or
/// authority to retry. Counts completed local shutdowns with timed-out remote drain.
pub fn iroh_forced_drain_count_v1() -> u64 {
    FORCED_DRAINS.load(Ordering::Relaxed)
}

/// A failure returned by an actual QUIC stream IO operation, or an explicit
/// peer STOP. Only this carrier can construct the marker. It does not classify
/// frame violations, local authorization failures or an already-failed stream
/// as transport failures, and does not itself authorize retries or downgrade.
#[derive(Debug, thiserror::Error)]
#[error("carrier stream IO failed: {0:#}")]
pub struct IrohStreamIoFailureV1(#[source] anyhow::Error);

/// Only this adapter's finite frame timer can construct this marker. The old
/// stream is closed and unusable. It is neither a remote rejection nor a new
/// permission; a product owner must reauthorize a NEW operation to continue.
#[derive(Debug, thiserror::Error)]
#[error("carrier frame operation reached its original deadline")]
pub struct IrohFrameDeadlineV1(());

/// Only this carrier's original finite scope deadline constructs this marker,
/// after the caller's current authority check has succeeded. It terminates the
/// old scope; it grants neither a new lease nor permission to bypass identity.
#[derive(Debug, thiserror::Error)]
#[error("carrier scope expired")]
pub struct IrohScopeDeadlineV1(());

/// Recognize this carrier's own scope timer, never arbitrary error text or
/// nested authentication/local failures. A new operation needs new checks.
pub fn is_iroh_scope_deadline_v1(error: &anyhow::Error) -> bool {
    error.is::<IrohScopeDeadlineV1>()
}

/// Only a timeout/reset during an actual outbound handshake. Local policy,
/// capacity, TLS authentication and protocol errors never carry this marker.
#[derive(Debug, thiserror::Error)]
#[error("carrier connect unavailable: {0:#}")]
pub struct IrohConnectUnavailableV1(#[source] anyhow::Error);

/// Fixed, non-sensitive diagnostic categories. These are NOT retry permission.
/// In particular a remote close, even code zero, does not authorize a redial.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrohConnectionFailureKindV1 {
    Timeout,
    Reset,
    RemoteGracefulClose,
    RemoteApplicationClose,
    Protocol,
    Authentication,
    LocalClosed,
    LocalRejected,
    ResourceExhausted,
    OtherConnect,
}

fn connection_failure_kind(error: &iroh::endpoint::ConnectionError) -> IrohConnectionFailureKindV1 {
    use iroh::endpoint::ConnectionError;
    use IrohConnectionFailureKindV1::*;
    match error {
        ConnectionError::TimedOut => Timeout,
        ConnectionError::Reset => Reset,
        ConnectionError::ApplicationClosed(close) if close.error_code == 0u32.into() => {
            RemoteGracefulClose
        }
        ConnectionError::ApplicationClosed(_) => RemoteApplicationClose,
        ConnectionError::VersionMismatch
        | ConnectionError::TransportError(_)
        | ConnectionError::ConnectionClosed(_) => Protocol,
        ConnectionError::LocallyClosed => LocalClosed,
        ConnectionError::CidsExhausted => ResourceExhausted,
    }
}

/// Inspect typed carrier errors without exporting peer reason strings, endpoint
/// addresses or identifiers. Unknown/local policy errors remain unclassified.
pub fn iroh_connection_failure_kind_v1(
    error: &anyhow::Error,
) -> Option<IrohConnectionFailureKindV1> {
    use iroh::endpoint::{ConnectError, ConnectingError, ConnectionError};
    if let Some(error) = error.downcast_ref::<ConnectError>() {
        return Some(match error {
            ConnectError::Connection { source, .. } => connection_failure_kind(source),
            ConnectError::Connecting { source, .. } => match source {
                ConnectingError::ConnectionError { source, .. } => connection_failure_kind(source),
                ConnectingError::HandshakeFailure { .. } => {
                    IrohConnectionFailureKindV1::Authentication
                }
                ConnectingError::LocallyRejected { .. } => {
                    IrohConnectionFailureKindV1::LocalRejected
                }
                _ => IrohConnectionFailureKindV1::OtherConnect,
            },
            _ => IrohConnectionFailureKindV1::OtherConnect,
        });
    }
    error
        .downcast_ref::<ConnectionError>()
        .map(connection_failure_kind)
}

fn connect_unavailable(error: &iroh::endpoint::ConnectError) -> bool {
    use iroh::endpoint::{ConnectError, ConnectingError, ConnectionError};
    matches!(
        error,
        ConnectError::Connection {
            source: ConnectionError::TimedOut | ConnectionError::Reset,
            ..
        } | ConnectError::Connecting {
            source: ConnectingError::ConnectionError {
                source: ConnectionError::TimedOut | ConnectionError::Reset,
                ..
            },
            ..
        }
    )
}

/// Protocol-only diagnostics. Origin 1 = version mismatch, 2 = locally
/// detected transport violation, 3 = peer transport close. Standard QUIC/TLS
/// codes only; unknown extensions collapse to 65535. Never expose reasons,
/// frame bodies, identities or addresses, and NEVER use this to grant retries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct IrohProtocolDiagnosticV1 {
    pub origin: u8,
    pub code: u16,
}

pub fn iroh_protocol_diagnostic_v1(error: &anyhow::Error) -> Option<IrohProtocolDiagnosticV1> {
    use iroh::endpoint::{ConnectError, ConnectingError, ConnectionError};
    fn classify(error: &ConnectionError) -> Option<IrohProtocolDiagnosticV1> {
        let (origin, code) = match error {
            ConnectionError::VersionMismatch => (1, 0),
            ConnectionError::TransportError(error) => (2, u64::from(error.code)),
            ConnectionError::ConnectionClosed(error) => (3, u64::from(error.error_code)),
            _ => return None,
        };
        Some(IrohProtocolDiagnosticV1 {
            origin,
            code: if code <= 17 || (256..=511).contains(&code) {
                code as u16
            } else {
                65535
            },
        })
    }
    for cause in error.chain() {
        if let Some(error) = cause.downcast_ref::<ConnectionError>() {
            if let Some(value) = classify(error) {
                return Some(value);
            }
        }
        if let Some(error) = cause.downcast_ref::<ConnectError>() {
            let connection = match error {
                ConnectError::Connection { source, .. } => Some(source),
                ConnectError::Connecting {
                    source: ConnectingError::ConnectionError { source, .. },
                    ..
                } => Some(source),
                _ => None,
            };
            if let Some(value) = connection.and_then(classify) {
                return Some(value);
            }
        }
    }
    None
}

const CHECK_INTERVAL: Duration = Duration::from_millis(20);
const INCOMING_SETUP_TIMEOUT: Duration = Duration::from_secs(3);
static ENDPOINT_LIMIT: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();

/// Fast local authorization/cancellation check. Never do RPC or blocking IO here.
#[derive(Clone)]
pub struct IrohControlV1 {
    check: Arc<dyn Fn() -> Result<()> + Send + Sync>,
    deadline: Option<Instant>,
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
            deadline: Some(deadline),
        })
    }

    fn check(&self) -> Result<()> {
        (self.check)()?;
        if self.deadline.is_some_and(|end| Instant::now() >= end) {
            return Err(IrohScopeDeadlineV1(()).into());
        }
        Ok(())
    }

    fn io_deadline(&self) -> Result<Instant> {
        self.deadline.context("stream requires a finite IO budget")
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
    relay_only: bool,
    service_identity: Option<iroh::SecretKey>,
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
            relay_only: false,
            service_identity: None,
        })
    }

    /// An explicitly provisioned distribution service may retain its independent
    /// carrier identity across bounded scopes. The platform owns protected key
    /// custody and permission expiry. Never pass a wallet, chat or issuer seed.
    /// Ordinary client/rendezvous scopes keep the default fresh identity.
    pub fn with_owned_service_identity(
        mut self,
        seed: zeroize::Zeroizing<[u8; 32]>,
    ) -> Result<Self> {
        ensure!(*seed != [0; 32], "empty service carrier identity");
        self.service_identity = Some(iroh::SecretKey::from_bytes(&seed));
        Ok(self)
    }

    /// Disable direct IP transports and accept only locally approved relay
    /// candidates. This is a routing constraint, NOT an anonymity guarantee.
    ///
    /// In iroh 1.3 this also disables QUIC address-discovery probes because no
    /// IP transport exists. HTTPS relay probes and relay connections remain;
    /// the explicitly configured DNS resolver can still send UDP queries.
    pub fn with_relay_only(mut self) -> Result<Self> {
        ensure!(
            !self.relays.is_empty(),
            "relay-only requires an approved relay"
        );
        ensure!(
            self.bind_address.is_none(),
            "relay-only cannot bind an IP transport"
        );
        self.relay_only = true;
        Ok(self)
    }

    /// Optional interface binding, primarily for deterministic local tests.
    pub fn with_bind_address(mut self, address: SocketAddr) -> Result<Self> {
        ensure!(!self.relay_only, "relay-only cannot bind an IP transport");
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
    relay_only: bool,
    connections: Arc<tokio::sync::Semaphore>,
    accept_consumer: Arc<tokio::sync::Mutex<()>>,
    listener_budget: Option<Duration>,
}

/// Process-local diagnostics, not identifiers, receipts or permission to retry.
/// `instance` is assigned after a real endpoint bind. `binds_completed` includes
/// temporary scopes too, so a caller can detect accidental additional binds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IrohEndpointSnapshotV1 {
    pub instance: u64,
    pub binds_completed: u64,
    pub operations_started: u64,
    pub active_operations: usize,
    pub closing: bool,
    pub closed: bool,
}

struct EndpointOwnerState {
    check: Arc<dyn Fn() -> Result<()> + Send + Sync>,
    closing: AtomicBool,
    closed: AtomicBool,
    active: AtomicUsize,
    started: AtomicU64,
}

impl EndpointOwnerState {
    fn check(&self) -> Result<()> {
        ensure!(
            !self.closing.load(Ordering::SeqCst),
            "carrier owner stopped"
        );
        if let Err(error) = (self.check)() {
            // An owner's revocation is terminal. A later successful callback
            // must not resurrect its endpoint or any of its operation scopes.
            self.closing.store(true, Ordering::SeqCst);
            return Err(error);
        }
        ensure!(
            !self.closing.load(Ordering::SeqCst),
            "carrier owner stopped"
        );
        Ok(())
    }
}

struct EndpointOperationGuard(Arc<EndpointOwnerState>);

impl Drop for EndpointOperationGuard {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A platform-owned transport endpoint, retained on ONE live Tokio runtime.
/// The platform must keep that runtime alive until `close()` completes. This
/// type does not create a runtime, renew authorization, or cache QUIC sessions.
/// Each operation requires a new finite control and each connect creates a new
/// authenticated connection. Its retained transport key can be linkable and is
/// not an anonymous route; callers must apply their policy before binding.
///
/// The owner-wide admission and accept mutex apply across nested scopes. There
/// is one accept consumer at a time: products must sequence control/data accepts
/// rather than treating an authenticated peer key as an application demux tag.
pub struct IrohEndpointOwnerV1 {
    endpoint: Endpoint,
    runtime: tokio::runtime::Id,
    instance: u64,
    state: Arc<EndpointOwnerState>,
    relays: Vec<RelayUrl>,
    nameserver: IpAddr,
    bind_address: Option<SocketAddr>,
    relay_only: bool,
    service_identity: Option<EndpointId>,
    connections: Arc<tokio::sync::Semaphore>,
    accept_consumer: Arc<tokio::sync::Mutex<()>>,
    close_gate: tokio::sync::Mutex<()>,
    close_started: AtomicBool,
    permit: std::sync::Mutex<Option<tokio::sync::OwnedSemaphorePermit>>,
}

impl IrohEndpointOwnerV1 {
    /// `check` grants only local endpoint ownership, never peer/application
    /// trust. It must be fast and non-blocking, and failure permanently stops
    /// admission. The platform is responsible for observing its policy while
    /// idle and calling `close`; active operations also poll it themselves.
    pub async fn bind(
        config: IrohConfigV1,
        check: impl Fn() -> Result<()> + Send + Sync + 'static,
    ) -> Result<Self> {
        let runtime = tokio::runtime::Handle::try_current()
            .context("carrier owner requires a live runtime")?
            .id();
        let state = Arc::new(EndpointOwnerState {
            check: Arc::new(check),
            closing: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            active: AtomicUsize::new(0),
            started: AtomicU64::new(0),
        });
        let bound_state = state.clone();
        let control = IrohControlV1 {
            check: Arc::new(move || bound_state.check()),
            deadline: None,
        };
        let (endpoint, permit, instance) = bind_carrier_endpoint(&config, &control).await?;
        if let Err(error) = control.check() {
            // Revocation may race successful bind. Do not publish/cache an
            // owner; retain endpoint admission through bounded inline cleanup.
            let cleanup = tokio::time::timeout(
                Duration::from_secs(5),
                endpoint.close_with_drain_timeout(Duration::from_secs(4)),
            )
            .await;
            return match cleanup {
                Ok(Some(drained)) => {
                    if !drained {
                        FORCED_DRAINS.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(error)
                }
                _ => Err(error.context("revoked carrier bind cleanup did not complete")),
            };
        }
        Ok(Self {
            endpoint,
            runtime,
            instance,
            state,
            service_identity: config
                .service_identity
                .as_ref()
                .map(iroh::SecretKey::public),
            relays: config.relays,
            nameserver: config.nameserver,
            bind_address: config.bind_address,
            relay_only: config.relay_only,
            connections: Arc::new(tokio::sync::Semaphore::new(IROH_MAX_CONNECTIONS_V1)),
            accept_consumer: Arc::new(tokio::sync::Mutex::new(())),
            close_gate: tokio::sync::Mutex::new(()),
            close_started: AtomicBool::new(false),
            permit: std::sync::Mutex::new(Some(permit)),
        })
    }

    fn check_runtime(&self) -> Result<()> {
        ensure!(
            tokio::runtime::Handle::try_current()
                .context("carrier owner requires its live runtime")?
                .id()
                == self.runtime,
            "carrier owner runtime changed"
        );
        Ok(())
    }

    /// Exact local routing policy compatibility (relay ordering is irrelevant).
    /// A product must check this against its newly approved candidate config;
    /// retaining an endpoint never expands the candidate's relay/DNS policy.
    pub fn matches_config(&self, config: &IrohConfigV1) -> bool {
        self.nameserver == config.nameserver
            && self.bind_address == config.bind_address
            && self.relay_only == config.relay_only
            && self.service_identity
                == config
                    .service_identity
                    .as_ref()
                    .map(iroh::SecretKey::public)
            && self.relays.len() == config.relays.len()
            && self
                .relays
                .iter()
                .all(|relay| config.relays.contains(relay))
    }

    pub fn snapshot(&self) -> IrohEndpointSnapshotV1 {
        IrohEndpointSnapshotV1 {
            instance: self.instance,
            binds_completed: BINDS_COMPLETED.load(Ordering::Relaxed),
            operations_started: self.state.started.load(Ordering::Relaxed),
            active_operations: self.state.active.load(Ordering::SeqCst),
            closing: self.state.closing.load(Ordering::SeqCst),
            closed: self.state.closed.load(Ordering::Acquire),
        }
    }

    /// Borrow a fresh finite operation. The original deadline is unchanged;
    /// ending/cancelling it drops only its streams and connections. It cannot
    /// return a borrowed stream or prolong a previous operation's authority.
    pub async fn with_operation<T, F>(&self, control: IrohControlV1, operation: F) -> Result<T>
    where
        F: for<'a> FnOnce(&'a IrohScopeV1) -> Pin<Box<dyn Future<Output = Result<T>> + 'a>>,
    {
        self.check_runtime()?;
        control.io_deadline()?;
        control.check()?;
        self.state.check()?;
        self.state.active.fetch_add(1, Ordering::SeqCst);
        let _active = EndpointOperationGuard(self.state.clone());
        // Paired with close's SeqCst closing/admission reads. If close won the
        // race before admission, no operation is permitted to enter network IO.
        self.state.check()?;
        self.state.started.fetch_add(1, Ordering::Relaxed);
        let owner_state = self.state.clone();
        let operation_control = control.clone();
        let scope = IrohScopeV1 {
            endpoint: self.endpoint.clone(),
            control: IrohControlV1 {
                check: Arc::new(move || {
                    owner_state.check()?;
                    operation_control.check()
                }),
                deadline: control.deadline,
            },
            relays: self.relays.clone(),
            relay_only: self.relay_only,
            connections: self.connections.clone(),
            accept_consumer: self.accept_consumer.clone(),
            listener_budget: None,
        };
        let result = tokio::select! {
            biased;
            error = scope.control.revoked() => Err(error),
            result = operation(&scope) => result,
        };
        // Preserve the operation's first failure (including authentication
        // rejection). A later cancellation must not reclassify it as a fresh
        // operation boundary; only a successful result needs the final check.
        result.and_then(|value| scope.control.check().map(|()| value))
    }

    /// Forward a platform-observed network change on this owner's runtime.
    /// This is a finite, independently authorized maintenance operation: it
    /// neither extends an old scope nor establishes peer/application trust.
    /// Success means Iroh accepted a request to inspect the current interfaces,
    /// not that a relay is online or that the asynchronous inspection finished.
    /// The endpoint identity and owner stay intact; Iroh may update its existing
    /// transports after discovering an actual interface/route change.
    pub async fn network_change(&self, control: IrohControlV1) -> Result<()> {
        self.with_operation(control, |scope| {
            Box::pin(async move {
                scope.endpoint.network_change().await;
                scope.control.check()
            })
        })
        .await
    }

    /// Stop admission, await all borrowed operations, then join QUIC/actor and
    /// OS socket closure on the owning runtime. Cleanup has its own bounded
    /// budget and grants no message or authorization success. A cancelled or
    /// timed-out close is fail-closed, but is NOT reported as completed.
    pub async fn close(&self) -> Result<()> {
        self.check_runtime()?;
        self.state.closing.store(true, Ordering::SeqCst);
        tokio::time::timeout(Duration::from_secs(5), async {
            let _close = self.close_gate.lock().await;
            if self.state.closed.load(Ordering::Acquire) {
                return Ok(());
            }
            ensure!(
                !self.close_started.load(Ordering::Acquire),
                "carrier owner close did not complete"
            );
            while self.state.active.load(Ordering::SeqCst) != 0 {
                tokio::time::sleep(CHECK_INTERVAL).await;
            }
            self.close_started.store(true, Ordering::Release);
            let drained = self
                .endpoint
                .close_with_drain_timeout(Duration::from_secs(4))
                .await
                .context("carrier endpoint close already started")?;
            if !drained {
                FORCED_DRAINS.fetch_add(1, Ordering::Relaxed);
            }
            self.state.closed.store(true, Ordering::Release);
            self.permit
                .lock()
                .map_err(|_| anyhow::anyhow!("carrier endpoint admission poisoned"))?
                .take();
            Ok(())
        })
        .await
        .context("carrier owner cleanup still running")?
    }
}

impl Drop for IrohEndpointOwnerV1 {
    fn drop(&mut self) {
        self.state.closing.store(true, Ordering::SeqCst);
        // Endpoint Drop aborts its tasks. Only explicit close proves that the
        // operating-system resources have been released; never mark closed here.
    }
}

/// Platform-owned, inbound-only service listener. Its fixed operator permission
/// is distinct from short client tasks and signed service/source leases. There
/// is no renewal method. Stop/revocation is polled throughout bind, serving and
/// cleanup. Each admitted stream gets a separate, non-renewable 3-second budget.
/// This does not grant application identity, source trust or Privacy eligibility.
pub async fn with_iroh_listener_v1<T, F>(
    config: IrohConfigV1,
    until: Instant,
    check: impl Fn() -> Result<()> + Send + Sync + 'static,
    operation: F,
) -> Result<T>
where
    F: for<'a> FnOnce(&'a IrohScopeV1) -> Pin<Box<dyn Future<Output = Result<T>> + 'a>> + 'static,
{
    ensure!(
        config.service_identity.is_some(),
        "listener requires protected service identity"
    );
    let remaining = until.saturating_duration_since(Instant::now());
    ensure!(
        !remaining.is_zero() && remaining <= Duration::from_secs(86400),
        "listener permission bound"
    );
    // Only local cleanup may use this reserve: the operation is stopped at
    // `until`, and the original operator check still applies during cleanup.
    let control = IrohControlV1 {
        check: Arc::new(check),
        deadline: Some(until + Duration::from_secs(6)),
    };
    with_iroh_scope_inner(config, control, Some(Duration::from_secs(3)), move |scope| {
        Box::pin(async move {
            tokio::select! {
                biased;
                result = operation(scope) => result,
                _ = tokio::time::sleep_until(until.into()) => bail!("listener permission ended"),
            }
        })
    })
    .await
}

/// Stable inbound-only node listener under an explicit LOCAL owner policy.
/// It confers no peer trust or service authority. Each accepted stream still
/// gets a finite 30-second budget; callers verify fresh signed service leases.
/// The owner check is polled during binding, serving and resource cleanup.
pub async fn with_iroh_owned_listener_v1<T, F>(
    config: IrohConfigV1,
    check: impl Fn() -> Result<()> + Send + Sync + 'static,
    operation: F,
) -> Result<T>
where
    F: for<'a> FnOnce(&'a IrohScopeV1) -> Pin<Box<dyn Future<Output = Result<T>> + 'a>> + 'static,
{
    ensure!(
        config.service_identity.is_some(),
        "listener requires protected service identity"
    );
    with_iroh_scope_inner(
        config,
        IrohControlV1 {
            check: Arc::new(check),
            deadline: None,
        },
        Some(Duration::from_secs(30)),
        operation,
    )
    .await
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
    with_iroh_scope_inner(config, control, None, operation).await
}

async fn bind_carrier_endpoint(
    config: &IrohConfigV1,
    control: &IrohControlV1,
) -> Result<(Endpoint, tokio::sync::OwnedSemaphorePermit, u64)> {
    control.check()?;
    // A permit covers the operation and its inline local cleanup. No detached
    // endpoint closer may outlive the runtime that owns the operation.
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
    // Only explicit platform-owned distribution services use a retained key.
    // Default clients and rendezvous epochs still generate fresh identities.
    if let Some(key) = &config.service_identity {
        builder = builder.secret_key(key.clone());
    }
    if config.relay_only {
        builder = builder.clear_ip_transports();
    } else if let Some(address) = config.bind_address {
        builder = builder.clear_ip_transports().bind_addr(address)?;
    }
    let endpoint = tokio::select! {
        biased;
        error = control.revoked() => return Err(error),
        result = builder.bind() => result?,
    };
    let instance = BINDS_COMPLETED.fetch_add(1, Ordering::Relaxed) + 1;
    Ok((endpoint, permit, instance))
}

async fn with_iroh_scope_inner<T, F>(
    config: IrohConfigV1,
    control: IrohControlV1,
    listener_budget: Option<Duration>,
    operation: F,
) -> Result<T>
where
    F: for<'a> FnOnce(&'a IrohScopeV1) -> Pin<Box<dyn Future<Output = Result<T>> + 'a>>,
{
    let (endpoint, permit, _) = bind_carrier_endpoint(&config, &control).await?;
    let scope = IrohScopeV1 {
        endpoint,
        control,
        relays: config.relays,
        relay_only: config.relay_only,
        connections: Arc::new(tokio::sync::Semaphore::new(IROH_MAX_CONNECTIONS_V1)),
        accept_consumer: Arc::new(tokio::sync::Mutex::new(())),
        listener_budget,
    };
    let result = tokio::select! {
        biased;
        error = scope.control.revoked() => Err(error),
        result = operation(&scope) => result,
    };
    // A late success cannot revive a scope, but a late revocation must not
    // replace an operation's already-returned authentication or protocol error.
    let result = result.and_then(|value| scope.control.check().map(|()| value));
    if result.is_err() {
        // Failed operations never gain success from endpoint cleanup.
        drop(scope);
        tokio::task::yield_now().await;
        drop(permit);
        return result;
    }
    let final_control = scope.control.clone();
    let cleanup = tokio::select! {
        biased;
        error = final_control.revoked() => Err(error),
        closed = tokio::time::timeout(Duration::from_secs(5),
            scope.endpoint.close_with_drain_timeout(Duration::from_secs(4))) => {
            closed.context("carrier graceful cleanup still running")
                .and_then(|value| value.context("carrier endpoint close already started"))
        }
    };
    if matches!(cleanup, Ok(false)) {
        FORCED_DRAINS.fetch_add(1, Ordering::Relaxed);
    }
    drop(scope);
    tokio::task::yield_now().await;
    drop(permit);
    cleanup?;
    final_control.check().and(result)
}

impl IrohScopeV1 {
    pub(crate) fn check_authority(&self) -> Result<()> {
        self.control.check()
    }

    pub fn address(&self) -> Result<IrohAddressV1> {
        self.control.check()?;
        let address = self.endpoint.addr();
        Ok(IrohAddressV1 {
            endpoint_key: *address.id.as_bytes(),
            direct: if self.relay_only {
                Vec::new()
            } else {
                address.ip_addrs().take(8).copied().collect()
            },
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

    pub fn uses_relays(&self) -> bool {
        !self.relays.is_empty()
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
            if !self.relay_only {
                addresses.push(TransportAddr::Ip(*address));
            }
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
        ensure!(
            !self.relay_only || !addresses.is_empty(),
            "relay-only scope requires an approved relay candidate"
        );
        ensure!(!addresses.is_empty(), "no approved peer candidate");
        Ok(EndpointAddr::from_parts(id, addresses))
    }

    pub async fn connect(&self, peer: &IrohAddressV1) -> Result<IrohStreamV1<'_>> {
        ensure!(
            self.listener_budget.is_none(),
            "service listener cannot initiate connections"
        );
        let address = self.peer_address(peer)?;
        // Reserve before dialing: outbound work shares the same active budget
        // as inbound TLS handshakes and streams, without a waiting queue.
        let admission = self
            .connections
            .try_acquire()
            .context("carrier connection limit")?;
        let connection = self
            .endpoint
            .connect(address, IROH_ALPN_V1)
            .await
            .map_err(|error| {
                if connect_unavailable(&error) {
                    anyhow::Error::new(IrohConnectUnavailableV1(error.into()))
                } else {
                    error.into()
                }
            })?;
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
            io_control: IrohControlV1 {
                check: self.control.check.clone(),
                deadline: if let Some(budget) = self.listener_budget {
                    let until = Instant::now() + budget;
                    Some(self.control.deadline.map_or(until, |end| end.min(until)))
                } else {
                    self.control.deadline
                },
            },
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
    io_control: IrohControlV1,
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
    control: &'a IrohControlV1,
    connection: &'a Connection,
    send: &'a mut SendStream,
    failed: &'a AtomicBool,
}

/// A borrowed framed reader, independent of the writer's mutable borrow. Keep
/// one read future alive across application-level waits; cancelling it poisons
/// both directions rather than forgetting a partially consumed frame.
pub struct IrohReadHalfV1<'a> {
    control: &'a IrohControlV1,
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

fn check_frame_io(
    control: &IrohControlV1,
    connection: &Connection,
    failed: &AtomicBool,
) -> Result<()> {
    ensure!(!failed.load(Ordering::Acquire), "carrier stream failed");
    if let Err(error) = control.check() {
        failed.store(true, Ordering::Release);
        connection.close(1u32.into(), b"frame authorization revoked");
        return Err(error);
    }
    // The other half can fail while the local authority callback is running.
    ensure!(!failed.load(Ordering::Acquire), "carrier stream failed");
    Ok(())
}

async fn write_frame_io(
    control: &IrohControlV1,
    connection: &Connection,
    failed: &AtomicBool,
    send: &mut SendStream,
    bytes: &[u8],
) -> Result<()> {
    check_frame_io(control, connection, failed)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= IROH_MAX_FRAME_V1,
        "carrier frame limit"
    );
    let mut guard = FrameIoGuard {
        connection,
        failed,
        complete: false,
    };
    tokio::time::timeout_at(control.io_deadline()?.into(), async {
        send.write_all(&(bytes.len() as u32).to_be_bytes())
            .await
            .map_err(|error| IrohStreamIoFailureV1(error.into()))?;
        send.write_all(bytes)
            .await
            .map_err(|error| IrohStreamIoFailureV1(error.into()))
    })
    .await
    .map_err(|_| IrohFrameDeadlineV1(()))??;
    check_frame_io(control, connection, failed)?;
    guard.complete = true;
    Ok(())
}

async fn read_frame_io(
    control: &IrohControlV1,
    connection: &Connection,
    failed: &AtomicBool,
    recv: &mut RecvStream,
) -> Result<Vec<u8>> {
    check_frame_io(control, connection, failed)?;
    let mut guard = FrameIoGuard {
        connection,
        failed,
        complete: false,
    };
    let mut length = [0u8; 4];
    tokio::time::timeout_at(control.io_deadline()?.into(), recv.read_exact(&mut length))
        .await
        .map_err(|_| IrohFrameDeadlineV1(()))?
        .map_err(|error| IrohStreamIoFailureV1(error.into()))?;
    let length = u32::from_be_bytes(length) as usize;
    ensure!(
        length > 0 && length <= IROH_MAX_FRAME_V1,
        "carrier frame limit"
    );
    let mut bytes = vec![0; length];
    tokio::time::timeout_at(control.io_deadline()?.into(), recv.read_exact(&mut bytes))
        .await
        .map_err(|_| IrohFrameDeadlineV1(()))?
        .map_err(|error| IrohStreamIoFailureV1(error.into()))?;
    check_frame_io(control, connection, failed)?;
    guard.complete = true;
    Ok(bytes)
}

async fn confirm_send_completion(send: &mut SendStream) -> Result<()> {
    ensure!(
        send.stopped()
            .await
            .map_err(|error| IrohStreamIoFailureV1(error.into()))?
            .is_none(),
        IrohStreamIoFailureV1(anyhow::anyhow!("peer stopped stream"))
    );
    Ok(())
}

impl IrohWriteHalfV1<'_> {
    pub async fn write_frame(&mut self, bytes: &[u8]) -> Result<()> {
        write_frame_io(self.control, self.connection, self.failed, self.send, bytes).await
    }
}

impl IrohReadHalfV1<'_> {
    pub async fn read_frame(&mut self) -> Result<Vec<u8>> {
        read_frame_io(self.control, self.connection, self.failed, self.recv).await
    }
}

impl IrohStreamV1<'_> {
    fn check(&self) -> Result<()> {
        check_frame_io(&self.io_control, &self.connection, &self.failed)
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
                control: &self.io_control,
                connection: &self.connection,
                send: &mut self.send,
                failed: &self.failed,
            },
            IrohReadHalfV1 {
                control: &self.io_control,
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
            &self.io_control,
            &self.connection,
            &self.failed,
            &mut self.send,
            bytes,
        )
        .await
    }

    pub async fn read_frame(&mut self) -> Result<Vec<u8>> {
        read_frame_io(
            &self.io_control,
            &self.connection,
            &self.failed,
            &mut self.recv,
        )
        .await
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
        tokio::time::timeout_at(self.io_control.io_deadline()?.into(), async {
            ensure!(
                self.recv
                    .read(&mut trailing)
                    .await
                    .map_err(|error| IrohStreamIoFailureV1(error.into()))?
                    .is_none(),
                "unexpected trailing stream data"
            );
            confirm_send_completion(&mut self.send).await
        })
        .await
        .map_err(|_| IrohFrameDeadlineV1(()))??;
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

    #[test]
    fn fixed_connection_diagnostics_do_not_grant_retries_or_match_text() {
        use iroh::endpoint::ConnectionError;
        for (error, expected) in [
            (
                ConnectionError::TimedOut,
                IrohConnectionFailureKindV1::Timeout,
            ),
            (ConnectionError::Reset, IrohConnectionFailureKindV1::Reset),
            (
                ConnectionError::VersionMismatch,
                IrohConnectionFailureKindV1::Protocol,
            ),
            (
                ConnectionError::LocallyClosed,
                IrohConnectionFailureKindV1::LocalClosed,
            ),
            (
                ConnectionError::CidsExhausted,
                IrohConnectionFailureKindV1::ResourceExhausted,
            ),
        ] {
            let error = anyhow::Error::new(error).context("outer operation");
            assert_eq!(iroh_connection_failure_kind_v1(&error), Some(expected));
            assert!(!error.is::<IrohConnectUnavailableV1>());
        }
        for message in ["timed out", "closed by peer", "authentication failed"] {
            assert_eq!(
                iroh_connection_failure_kind_v1(&anyhow::anyhow!(message)),
                None
            );
        }
    }

    #[test]
    fn protocol_diagnostics_keep_only_fixed_codes_and_never_mark_retry() {
        use iroh::endpoint::{ConnectionError, TransportError, TransportErrorCode, VarInt};
        for (code, expected) in [(0, 0), (10, 10), (256, 256), (511, 511), (99999, 65535)] {
            let error = ConnectionError::TransportError(TransportError::new(
                TransportErrorCode::from(VarInt::from_u32(code)),
                "private peer reason must not be retained".into(),
            ));
            let error = anyhow::Error::new(error).context("local operation");
            let diagnostic = iroh_protocol_diagnostic_v1(&error).unwrap();
            assert_eq!(
                diagnostic,
                IrohProtocolDiagnosticV1 {
                    origin: 2,
                    code: expected
                }
            );
            assert!(!error.is::<IrohStreamIoFailureV1>());
            assert!(!error.is::<IrohConnectUnavailableV1>());
            assert!(!serde_json::to_string(&diagnostic)
                .unwrap()
                .contains("private"));
        }
        assert_eq!(
            iroh_protocol_diagnostic_v1(&ConnectionError::VersionMismatch.into()),
            Some(IrohProtocolDiagnosticV1 { origin: 1, code: 0 })
        );
        assert!(iroh_protocol_diagnostic_v1(&anyhow::anyhow!(
            "transport error: protocol violation"
        ))
        .is_none());
    }

    fn local_config() -> IrohConfigV1 {
        IrohConfigV1::new(&[], "127.0.0.1".parse().unwrap())
            .unwrap()
            .with_bind_address("127.0.0.1:0".parse().unwrap())
            .unwrap()
    }
    fn control() -> IrohControlV1 {
        IrohControlV1::new(Instant::now() + Duration::from_secs(8), || Ok(())).unwrap()
    }

    #[tokio::test]
    async fn finite_scope_deadline_is_typed_without_reclassifying_security_errors() {
        use crate::{
            bound_session::BoundSessionError, discovery_control::is_service_deadline_failure,
        };
        let _permit = SERIAL.acquire().await.unwrap();
        let owner = IrohEndpointOwnerV1::bind(local_config(), || Ok(()))
            .await
            .unwrap();
        let gate =
            IrohControlV1::new(Instant::now() + Duration::from_millis(80), || Ok(())).unwrap();
        let result = owner
            .with_operation(gate.clone(), |scope| {
                Box::pin(async move { scope.accept_inbound().await.map(|_| ()) })
            })
            .await;
        let error = result.unwrap_err();
        assert!(is_iroh_scope_deadline_v1(&error));
        assert!(is_service_deadline_failure(&error));
        assert!(!crate::carrier_stream::is_carrier_io_failure(&error));
        assert_eq!(owner.snapshot().active_operations, 0);
        assert!(!owner.snapshot().closed);
        // Every marker below comes from the same actually elapsed control.
        assert!(is_service_deadline_failure(
            &BoundSessionError::Transport(gate.check().unwrap_err()).into()
        ));
        for error in [
            BoundSessionError::Authentication(gate.check().unwrap_err()),
            BoundSessionError::Local(gate.check().unwrap_err()),
            BoundSessionError::Transport(
                BoundSessionError::Authentication(gate.check().unwrap_err()).into(),
            ),
            BoundSessionError::Transport(
                BoundSessionError::Local(gate.check().unwrap_err()).into(),
            ),
        ] {
            let error = error.into();
            assert!(!is_service_deadline_failure(&error));
            assert!(!is_iroh_scope_deadline_v1(&error));
        }
        assert!(!is_service_deadline_failure(&anyhow::anyhow!(
            "carrier scope expired"
        )));
        #[derive(Debug, thiserror::Error)]
        #[error("fixture local failure")]
        struct LocalFailure(#[source] anyhow::Error);
        assert!(!is_service_deadline_failure(
            &LocalFailure(gate.check().unwrap_err()).into()
        ));
        owner.close().await.unwrap();
    }

    #[tokio::test]
    async fn expired_scope_preserves_callback_rejection_before_local_timer() {
        let _permit = SERIAL.acquire().await.unwrap();
        let owner = IrohEndpointOwnerV1::bind(local_config(), || Ok(()))
            .await
            .unwrap();
        let deadline = Instant::now() + Duration::from_millis(30);
        let gate = IrohControlV1::new(deadline, || {
            Err(
                crate::bound_session::BoundSessionError::Authentication(anyhow::anyhow!(
                    "fixture revoked identity"
                ))
                .into(),
            )
        })
        .unwrap();
        tokio::time::sleep_until(deadline.into()).await;
        let error = owner
            .with_operation::<(), _>(gate, |_| {
                Box::pin(async { panic!("revoked scope entered operation") })
            })
            .await
            .unwrap_err();
        assert!(matches!(
            error.downcast_ref::<crate::bound_session::BoundSessionError>(),
            Some(crate::bound_session::BoundSessionError::Authentication(_))
        ));
        assert!(!is_iroh_scope_deadline_v1(&error));
        assert!(!crate::discovery_control::is_service_deadline_failure(
            &error
        ));
        assert_eq!(owner.snapshot().operations_started, 0);
        owner.close().await.unwrap();
    }

    #[derive(Debug, thiserror::Error)]
    #[error("fixture operation authority revoked")]
    struct OperationRevoked;

    fn revocable_control(revoked: Arc<AtomicBool>) -> IrohControlV1 {
        IrohControlV1::new(Instant::now() + Duration::from_secs(8), move || {
            if revoked.load(Ordering::Acquire) {
                return Err(OperationRevoked.into());
            }
            Ok(())
        })
        .unwrap()
    }

    // Set revocation in the same poll that returns the operation's result:
    // this deterministically exercises the final check, not a timer race.
    fn finish_and_revoke(revoked: &AtomicBool, reject: bool) -> Result<()> {
        revoked.store(true, Ordering::Release);
        if reject {
            Err(
                crate::bound_session::BoundSessionError::Authentication(anyhow::anyhow!(
                    "fixture authentication rejection"
                ))
                .into(),
            )
        } else {
            Ok(())
        }
    }

    fn assert_final_result(error: &anyhow::Error, reject: bool) {
        if reject {
            assert!(matches!(
                error.downcast_ref::<crate::bound_session::BoundSessionError>(),
                Some(crate::bound_session::BoundSessionError::Authentication(_))
            ));
            assert!(!error.is::<OperationRevoked>());
        } else {
            assert!(error.is::<OperationRevoked>(), "late success was accepted");
        }
    }

    #[tokio::test]
    async fn persistent_operation_preserves_first_error_and_checks_late_success() {
        let _permit = SERIAL.acquire().await.unwrap();
        let owner = IrohEndpointOwnerV1::bind(local_config(), || Ok(()))
            .await
            .unwrap();
        let port = owner.endpoint.bound_sockets()[0];
        for reject in [true, false] {
            let revoked = Arc::new(AtomicBool::new(false));
            let gate = revocable_control(revoked.clone());
            let result = owner
                .with_operation(gate, move |_| {
                    Box::pin(async move { finish_and_revoke(&revoked, reject) })
                })
                .await;
            assert_final_result(&result.unwrap_err(), reject);
            assert_eq!(owner.snapshot().active_operations, 0);
            assert!(!owner.snapshot().closed);
        }
        owner.close().await.unwrap();
        assert!(owner.snapshot().closed);
        assert!(std::net::UdpSocket::bind(port).is_ok());
    }

    #[tokio::test]
    async fn temporary_operation_preserves_first_error_and_checks_late_success() {
        let _permit = SERIAL.acquire().await.unwrap();
        for reject in [true, false] {
            let revoked = Arc::new(AtomicBool::new(false));
            let gate = revocable_control(revoked.clone());
            let result = with_iroh_scope_v1(local_config(), gate, move |_| {
                Box::pin(async move { finish_and_revoke(&revoked, reject) })
            })
            .await;
            assert_final_result(&result.unwrap_err(), reject);
            assert_eq!(ENDPOINT_LIMIT.get().unwrap().available_permits(), 4);
        }
    }

    #[tokio::test]
    async fn persistent_network_change_preserves_socket_and_active_stream() {
        let _permit = SERIAL.acquire().await.unwrap();
        let server = Arc::new(
            IrohEndpointOwnerV1::bind(local_config(), || Ok(()))
                .await
                .unwrap(),
        );
        let client = Arc::new(
            IrohEndpointOwnerV1::bind(local_config(), || Ok(()))
                .await
                .unwrap(),
        );
        let before = [server.snapshot(), client.snapshot()];
        let ports = [
            server.endpoint.bound_sockets()[0],
            client.endpoint.bound_sockets()[0],
        ];
        let (server_tx, server_rx) = oneshot::channel();
        let (client_tx, client_rx) = oneshot::channel();
        let listener = server.with_operation(control(), |scope| {
            Box::pin(async move {
                server_tx.send(scope.address()?).unwrap();
                let peer: IrohAddressV1 = client_rx.await?;
                let mut stream = scope.accept(peer.endpoint_key).await?;
                ensure!(stream.read_frame().await? == b"before", "before fixture");
                stream.write_frame(b"ready").await?;
                ensure!(stream.read_frame().await? == b"after", "after fixture");
                stream.write_frame(b"still open").await?;
                stream.finish().await
            })
        });
        let changed = client.clone();
        let connector = client.with_operation(control(), |scope| {
            Box::pin(async move {
                client_tx.send(scope.address()?).unwrap();
                let peer = server_rx.await?;
                let mut stream = scope.connect(&peer).await?;
                let binding = stream.binding_material()?;
                stream.write_frame(b"before").await?;
                ensure!(stream.read_frame().await? == b"ready", "ready fixture");
                changed.network_change(control()).await?;
                // Real local QUIC traffic after the notification. This does not
                // simulate an OS interface change or prove mobile relay recovery.
                stream.write_frame(b"after").await?;
                ensure!(
                    stream.read_frame().await? == b"still open",
                    "notification closed stream"
                );
                assert_eq!(stream.binding_material()?, binding);
                stream.finish().await
            })
        });
        let (a, b) = tokio::join!(listener, connector);
        a.unwrap();
        b.unwrap();
        for (index, owner) in [&server, &client].into_iter().enumerate() {
            assert_eq!(owner.snapshot().instance, before[index].instance);
            assert_eq!(
                owner.snapshot().binds_completed,
                before[index].binds_completed
            );
            assert_eq!(owner.endpoint.bound_sockets()[0], ports[index]);
            assert_eq!(owner.snapshot().active_operations, 0);
            assert!(!owner.snapshot().closing);
        }
        assert_eq!(server.snapshot().operations_started, 1);
        assert_eq!(client.snapshot().operations_started, 2);
        let (a, b) = tokio::join!(server.close(), client.close());
        a.unwrap();
        b.unwrap();
        for port in ports {
            assert!(std::net::UdpSocket::bind(port).is_ok());
        }
    }

    #[tokio::test]
    async fn persistent_network_change_rejects_expired_operation_and_revoked_owner() {
        let _permit = SERIAL.acquire().await.unwrap();
        let revoked = Arc::new(AtomicBool::new(false));
        let check = revoked.clone();
        let owner = IrohEndpointOwnerV1::bind(local_config(), move || {
            ensure!(!check.load(Ordering::Acquire), "revoked fixture owner");
            Ok(())
        })
        .await
        .unwrap();
        let until = Instant::now() + Duration::from_millis(5);
        let expired = IrohControlV1::new(until, || Ok(())).unwrap();
        tokio::time::sleep_until(until.into()).await;
        assert!(owner
            .network_change(expired)
            .await
            .unwrap_err()
            .to_string()
            .contains("scope expired"));
        let denied = IrohControlV1::new(Instant::now() + Duration::from_secs(1), || {
            bail!("revoked fixture operation")
        })
        .unwrap();
        assert!(owner.network_change(denied).await.is_err());
        assert_eq!(owner.snapshot().operations_started, 0);
        assert!(!owner.snapshot().closing);
        owner.network_change(control()).await.unwrap();
        revoked.store(true, Ordering::Release);
        assert!(owner.network_change(control()).await.is_err());
        revoked.store(false, Ordering::Release);
        assert!(owner.network_change(control()).await.is_err());
        assert_eq!(owner.snapshot().operations_started, 1);
        assert_eq!(owner.snapshot().active_operations, 0);
        assert!(owner.snapshot().closing);
        owner.close().await.unwrap();
        assert!(owner.network_change(control()).await.is_err());
    }

    #[tokio::test]
    async fn persistent_endpoint_nested_connections_keep_control_alive_and_bind_independently() {
        let _permit = SERIAL.acquire().await.unwrap();
        let server = Arc::new(
            IrohEndpointOwnerV1::bind(local_config(), || Ok(()))
                .await
                .unwrap(),
        );
        let client = Arc::new(
            IrohEndpointOwnerV1::bind(local_config(), || Ok(()))
                .await
                .unwrap(),
        );
        let bound = BINDS_COMPLETED.load(Ordering::Relaxed);
        let server_instance = server.snapshot().instance;
        let client_instance = client.snapshot().instance;
        let ports = [
            server.endpoint.bound_sockets()[0],
            client.endpoint.bound_sockets()[0],
        ];
        // Repeat with newly authorized scopes on exactly the same two endpoints.
        // Payloads are transport-only fixtures, not application delivery claims.
        for round in 1..=2 {
            let (server_tx, server_rx) = oneshot::channel();
            let (client_tx, client_rx) = oneshot::channel();
            let nested_server = server.clone();
            let server_task = server.with_operation(control(), move |scope| {
                Box::pin(async move {
                    server_tx.send(scope.address()?).unwrap();
                    let peer: IrohAddressV1 = client_rx.await?;
                    let mut control_stream = scope.accept(peer.endpoint_key).await?;
                    let control_binding = control_stream.binding_material()?;
                    ensure!(
                        control_stream.read_frame().await? == b"control",
                        "control fixture"
                    );
                    control_stream.write_frame(b"control ready").await?;
                    let nested_state = nested_server.clone();
                    let data_binding = nested_server
                        .with_operation(control(), move |data_scope| {
                            Box::pin(async move {
                                ensure!(
                                    nested_state.snapshot().active_operations == 2,
                                    "nested admission"
                                );
                                let mut data = data_scope.accept(peer.endpoint_key).await?;
                                let binding = data.binding_material()?;
                                ensure!(
                                    data_scope.connections.available_permits() == 2,
                                    "shared connection cap"
                                );
                                ensure!(data.read_frame().await? == b"data", "data fixture");
                                data.write_frame(b"data done").await?;
                                data.finish().await?;
                                drop(data);
                                Ok(binding)
                            })
                        })
                        .await?;
                    ensure!(
                        scope.connections.available_permits() == 3,
                        "data permit leaked"
                    );
                    ensure!(
                        control_stream.read_frame().await? == b"after data",
                        "control lost after data finish"
                    );
                    control_stream.write_frame(b"still alive").await?;
                    control_stream.finish().await?;
                    Ok((control_binding, data_binding))
                })
            });
            let nested_client = client.clone();
            let client_task = client.with_operation(control(), move |scope| {
                Box::pin(async move {
                    client_tx.send(scope.address()?).unwrap();
                    let peer = server_rx.await?;
                    let mut control_stream = scope.connect(&peer).await?;
                    let control_binding = control_stream.binding_material()?;
                    control_stream.write_frame(b"control").await?;
                    ensure!(
                        control_stream.read_frame().await? == b"control ready",
                        "control response"
                    );
                    let data_binding = nested_client
                        .with_operation(control(), move |data_scope| {
                            Box::pin(async move {
                                let mut data = data_scope.connect(&peer).await?;
                                let binding = data.binding_material()?;
                                data.write_frame(b"data").await?;
                                ensure!(data.read_frame().await? == b"data done", "data response");
                                data.finish().await?;
                                drop(data);
                                Ok(binding)
                            })
                        })
                        .await?;
                    control_stream.write_frame(b"after data").await?;
                    ensure!(
                        control_stream.read_frame().await? == b"still alive",
                        "control closed by nested scope"
                    );
                    control_stream.finish().await?;
                    Ok((control_binding, data_binding))
                })
            });
            let (a, b) = tokio::join!(server_task, client_task);
            let (a, b) = (a.unwrap(), b.unwrap());
            assert_eq!(a, b, "both ends must derive the real connection binding");
            assert_eq!(a.0 .0, a.1 .0, "same initiator endpoint");
            assert_eq!(a.0 .1, a.1 .1, "same responder endpoint");
            assert_ne!(a.0 .2, a.1 .2, "data must use an independent TLS exporter");
            for owner in [&server, &client] {
                assert_eq!(owner.snapshot().active_operations, 0);
                assert_eq!(owner.snapshot().operations_started, round * 2);
                assert_eq!(owner.snapshot().binds_completed, bound);
                assert!(!owner.snapshot().closed);
            }
            assert_eq!(server.snapshot().instance, server_instance);
            assert_eq!(client.snapshot().instance, client_instance);
            assert_eq!(server.endpoint.bound_sockets()[0], ports[0]);
            assert_eq!(client.endpoint.bound_sockets()[0], ports[1]);
        }
        let (a, b) = tokio::join!(server.close(), client.close());
        a.unwrap();
        b.unwrap();
        for (owner, port) in [(&server, ports[0]), (&client, ports[1])] {
            assert!(owner.snapshot().closed);
            assert!(
                std::net::UdpSocket::bind(port).is_ok(),
                "owner close returned before UDP release"
            );
            assert!(owner
                .with_operation(control(), |_| Box::pin(async { Ok(()) }))
                .await
                .is_err());
        }
        assert_eq!(ENDPOINT_LIMIT.get().unwrap().available_permits(), 4);
    }

    #[tokio::test]
    async fn persistent_endpoint_deadline_cancels_real_dial_without_reviving_old_control() {
        let _permit = SERIAL.acquire().await.unwrap();
        let owner = IrohEndpointOwnerV1::bind(local_config(), || Ok(()))
            .await
            .unwrap();
        let sink = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let peer = IrohAddressV1 {
            endpoint_key: *iroh::SecretKey::generate().public().as_bytes(),
            direct: vec![sink.local_addr().unwrap()],
            relays: vec![],
        };
        let old =
            IrohControlV1::new(Instant::now() + Duration::from_millis(400), || Ok(())).unwrap();
        let dial = owner.with_operation(old.clone(), move |scope| {
            Box::pin(async move {
                let _stream = scope.connect(&peer).await?;
                Ok(())
            })
        });
        let traffic = async {
            let mut bytes = [0; 2048];
            assert!(
                tokio::time::timeout(Duration::from_millis(300), sink.recv(&mut bytes))
                    .await
                    .unwrap()
                    .unwrap()
                    > 0
            );
        };
        let (result, ()) = tokio::join!(dial, traffic);
        assert!(result.unwrap_err().to_string().contains("scope expired"));
        assert_eq!(owner.snapshot().active_operations, 0);
        assert_eq!(
            owner.connections.available_permits(),
            IROH_MAX_CONNECTIONS_V1
        );
        assert!(!owner.snapshot().closing);
        let old_result: Result<()> = owner
            .with_operation(old, |_| {
                Box::pin(async { panic!("expired control entered") })
            })
            .await;
        assert!(old_result.is_err());
        assert_eq!(owner.snapshot().operations_started, 1);
        owner
            .with_operation(control(), |scope| {
                Box::pin(async move {
                    scope.address()?;
                    Ok(())
                })
            })
            .await
            .unwrap();
        assert_eq!(owner.snapshot().operations_started, 2);
        owner.close().await.unwrap();
    }

    #[tokio::test]
    async fn persistent_endpoint_shares_accept_consumer_across_operations() {
        let _permit = SERIAL.acquire().await.unwrap();
        let owner = Arc::new(
            IrohEndpointOwnerV1::bind(local_config(), || Ok(()))
                .await
                .unwrap(),
        );
        let nested = owner.clone();
        owner
            .with_operation(control(), move |scope| {
                Box::pin(async move {
                    let mut first = Box::pin(scope.accept_inbound());
                    assert!(futures_util::poll!(first.as_mut()).is_pending());
                    let result = nested
                        .with_operation(control(), |scope| {
                            Box::pin(async move {
                                let _stream = scope.accept_inbound().await?;
                                Ok(())
                            })
                        })
                        .await;
                    ensure!(
                        result
                            .unwrap_err()
                            .to_string()
                            .contains("accept already pending"),
                        "scopes did not share accept lock"
                    );
                    drop(first);
                    let mut next = Box::pin(scope.accept_inbound());
                    ensure!(
                        futures_util::poll!(next.as_mut()).is_pending(),
                        "cancelled accept retained lock"
                    );
                    drop(next);
                    Ok(())
                })
            })
            .await
            .unwrap();
        assert_eq!(owner.snapshot().active_operations, 0);
        assert_eq!(
            owner.connections.available_permits(),
            IROH_MAX_CONNECTIONS_V1
        );
        owner.close().await.unwrap();
    }

    #[tokio::test]
    async fn persistent_endpoint_close_interrupts_active_scope_before_os_barrier() {
        let _permit = SERIAL.acquire().await.unwrap();
        let owner = IrohEndpointOwnerV1::bind(local_config(), || Ok(()))
            .await
            .unwrap();
        let port = owner.endpoint.bound_sockets()[0];
        let dropped = Arc::new(AtomicBool::new(false));
        struct DropMarker(Arc<AtomicBool>);
        impl Drop for DropMarker {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let marker = DropMarker(dropped.clone());
        let (ready, started) = oneshot::channel();
        let operation = owner.with_operation(control(), move |scope| {
            Box::pin(async move {
                let _marker = marker;
                ready.send(()).unwrap();
                let _stream = scope.accept_inbound().await?;
                Ok(())
            })
        });
        let close = async {
            started.await.unwrap();
            owner.close().await
        };
        let (operation, close) = tokio::join!(operation, close);
        assert!(operation.is_err());
        close.unwrap();
        assert!(dropped.load(Ordering::Acquire));
        assert_eq!(owner.snapshot().active_operations, 0);
        assert!(owner.snapshot().closed);
        assert!(std::net::UdpSocket::bind(port).is_ok());
        owner.close().await.unwrap();
    }

    #[tokio::test]
    async fn persistent_endpoint_owner_revocation_is_terminal_and_config_is_exact() {
        let _permit = SERIAL.acquire().await.unwrap();
        let binds = BINDS_COMPLETED.load(Ordering::Relaxed);
        assert!(
            IrohEndpointOwnerV1::bind(local_config(), || bail!("privacy required"))
                .await
                .is_err()
        );
        assert_eq!(BINDS_COMPLETED.load(Ordering::Relaxed), binds);
        let revoked = Arc::new(AtomicBool::new(false));
        let check_revoked = revoked.clone();
        let owner = IrohEndpointOwnerV1::bind(local_config(), move || {
            ensure!(!check_revoked.load(Ordering::Acquire), "owner revoked");
            Ok(())
        })
        .await
        .unwrap();
        assert!(owner.matches_config(&local_config()));
        let mut wrong = local_config();
        wrong.nameserver = "127.0.0.2".parse().unwrap();
        assert!(!owner.matches_config(&wrong));
        let mut wrong = local_config();
        wrong.bind_address = None;
        assert!(!owner.matches_config(&wrong));
        let mut wrong = local_config();
        wrong.relays.push("https://127.0.0.1/".parse().unwrap());
        assert!(!owner.matches_config(&wrong));
        let mut wrong = local_config();
        wrong.relay_only = true;
        assert!(!owner.matches_config(&wrong));
        let wrong = local_config()
            .with_owned_service_identity(zeroize::Zeroizing::new([17; 32]))
            .unwrap();
        assert!(!owner.matches_config(&wrong));
        revoked.store(true, Ordering::Release);
        assert!(owner
            .with_operation::<(), _>(control(), |_| Box::pin(async {
                panic!("revoked owner entered")
            }))
            .await
            .is_err());
        revoked.store(false, Ordering::Release);
        assert!(owner
            .with_operation::<(), _>(control(), |_| Box::pin(async {
                panic!("revived owner entered")
            }))
            .await
            .is_err());
        assert!(owner.snapshot().closing);
        assert_eq!(owner.snapshot().operations_started, 0);
        owner.close().await.unwrap();
    }

    #[tokio::test]
    async fn persistent_endpoint_revocation_at_bind_completion_releases_the_actual_socket() {
        let _permit = SERIAL.acquire().await.unwrap();
        let probe = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = probe.local_addr().unwrap();
        drop(probe);
        let before = BINDS_COMPLETED.load(Ordering::Relaxed);
        let result =
            IrohEndpointOwnerV1::bind(local_config().with_bind_address(port).unwrap(), move || {
                ensure!(
                    BINDS_COMPLETED.load(Ordering::Relaxed) == before,
                    "owner revoked during bind"
                );
                Ok(())
            })
            .await;
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("revoked during bind"));
        assert_eq!(BINDS_COMPLETED.load(Ordering::Relaxed), before + 1);
        assert!(
            std::net::UdpSocket::bind(port).is_ok(),
            "revoked bind returned before socket cleanup"
        );
        assert_eq!(ENDPOINT_LIMIT.get().unwrap().available_permits(), 4);
    }

    #[test]
    fn persistent_endpoint_rejects_a_different_runtime_without_closing_the_owner() {
        let first = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let second = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _permit = first.block_on(SERIAL.acquire()).unwrap();
        let owner = first
            .block_on(IrohEndpointOwnerV1::bind(local_config(), || Ok(())))
            .unwrap();
        let error = second
            .block_on(owner.with_operation::<(), _>(control(), |_| {
                Box::pin(async { panic!("wrong runtime entered") })
            }))
            .unwrap_err();
        assert!(error.to_string().contains("runtime changed"));
        assert!(second
            .block_on(owner.network_change(control()))
            .unwrap_err()
            .to_string()
            .contains("runtime changed"));
        assert!(second
            .block_on(owner.close())
            .unwrap_err()
            .to_string()
            .contains("runtime changed"));
        assert!(!owner.snapshot().closing);
        first.block_on(owner.close()).unwrap();
    }

    #[test]
    fn persistent_endpoint_cancelled_close_does_not_claim_barrier_or_release_admission() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(1)
            .build()
            .unwrap();
        runtime.block_on(async {
            let _permit = SERIAL.acquire().await.unwrap();
            let owner = IrohEndpointOwnerV1::bind(local_config(), || Ok(()))
                .await
                .unwrap();
            let port = owner.endpoint.bound_sockets()[0];
            let (release, held) = std::sync::mpsc::channel::<()>();
            let (ready, started) = oneshot::channel();
            let blocker = tokio::task::spawn_blocking(move || {
                let _ = ready.send(());
                let _ = held.recv();
            });
            started.await.unwrap();
            let mut close = Box::pin(owner.close());
            assert!(tokio::time::timeout(Duration::from_millis(150), &mut close)
                .await
                .is_err());
            assert!(!owner.snapshot().closed);
            assert_eq!(ENDPOINT_LIMIT.get().unwrap().available_permits(), 3);
            drop(close);
            assert!(
                owner.close().await.is_err(),
                "cancelled close became a completion proof"
            );
            assert!(!owner.snapshot().closed);
            assert_eq!(ENDPOINT_LIMIT.get().unwrap().available_permits(), 3);
            release.send(()).unwrap();
            blocker.await.unwrap();
            let end = Instant::now() + Duration::from_secs(1);
            while std::net::UdpSocket::bind(port).is_err() {
                assert!(
                    Instant::now() < end,
                    "cancelled owner retained socket after worker resumed"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            drop(owner);
            assert_eq!(ENDPOINT_LIMIT.get().unwrap().available_permits(), 4);
        });
    }

    fn local_relay_only_config() -> IrohConfigV1 {
        // A literal loopback URL: these tests require neither a relay server nor
        // public DNS. They test fail-closed routing, not successful relay IO.
        IrohConfigV1::new(
            &["https://127.0.0.1/".to_owned()],
            "127.0.0.1".parse().unwrap(),
        )
        .unwrap()
        .with_relay_only()
        .unwrap()
    }

    #[test]
    fn relay_only_requires_relays_and_rejects_ip_binding_in_both_orders() {
        let dns = "127.0.0.1".parse().unwrap();
        let bind = "127.0.0.1:0".parse().unwrap();
        assert!(!IrohConfigV1::new(&[], dns).unwrap().relay_only);
        assert!(IrohConfigV1::new(&[], dns)
            .unwrap()
            .with_relay_only()
            .is_err());
        assert!(IrohConfigV1::new(&["https://127.0.0.1/".to_owned()], dns)
            .unwrap()
            .with_bind_address(bind)
            .unwrap()
            .with_relay_only()
            .is_err());
        assert!(local_relay_only_config().with_bind_address(bind).is_err());
        assert!(local_relay_only_config().with_relay_only().is_ok());
    }

    #[tokio::test]
    async fn relay_only_has_no_bound_ip_socket_and_filters_direct_candidates_before_dial() {
        let _permit = SERIAL.acquire().await.unwrap();
        with_iroh_scope_v1(local_relay_only_config(), control(), |scope| {
            Box::pin(async move {
                // Inspect the actual endpoint, not just the exported address:
                // hiding addresses would not disable direct transports.
                assert!(scope.endpoint.bound_sockets().is_empty());
                assert!(scope.endpoint.addr().ip_addrs().next().is_none());
                assert!(scope.address()?.direct.is_empty());
                let mut peer = IrohAddressV1 {
                    endpoint_key: *iroh::SecretKey::generate().public().as_bytes(),
                    direct: vec!["127.0.0.1:9".parse().unwrap()],
                    relays: vec![],
                };
                let error =
                    match tokio::time::timeout(Duration::from_millis(200), scope.connect(&peer))
                        .await
                        .expect("direct-only input must fail before network IO")
                    {
                        Ok(_) => panic!("relay-only scope accepted a direct-only peer"),
                        Err(error) => error,
                    };
                assert!(error.to_string().contains("approved relay candidate"));
                assert_eq!(
                    scope.connections.available_permits(),
                    IROH_MAX_CONNECTIONS_V1
                );
                peer.relays.push("https://127.0.0.1/".to_owned());
                let filtered = scope.peer_address(&peer)?;
                assert!(filtered.ip_addrs().next().is_none());
                assert_eq!(filtered.relay_urls().count(), 1);
                peer.relays.push("https://127.0.0.2/".to_owned());
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
    async fn listener_requires_owned_identity_and_keeps_client_scope_bound() {
        assert!(IrohControlV1::new(Instant::now() + Duration::from_secs(121), || Ok(())).is_err());
        let result: Result<()> = with_iroh_listener_v1(
            local_config(),
            Instant::now() + Duration::from_secs(130),
            || Ok(()),
            |_| Box::pin(async { panic!("unowned listener bound") }),
        )
        .await;
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("protected service identity"));
        let result = with_iroh_listener_v1(
            local_config()
                .with_owned_service_identity(zeroize::Zeroizing::new([17; 32]))
                .unwrap(),
            Instant::now() + Duration::from_secs(86401),
            || Ok(()),
            |_| Box::pin(async { Ok(()) }),
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("permission bound"));
    }

    #[tokio::test]
    async fn listener_cannot_dial_or_extend_an_admitted_stream() {
        let _permit = SERIAL.acquire().await.unwrap();
        let (tx, rx) = oneshot::channel();
        let server = with_iroh_listener_v1(
            local_config()
                .with_owned_service_identity(zeroize::Zeroizing::new([17; 32]))
                .unwrap(),
            Instant::now() + Duration::from_secs(130),
            || Ok(()),
            move |scope| {
                Box::pin(async move {
                    let address = scope.address()?;
                    ensure!(scope.connect(&address).await.is_err(), "listener dialed");
                    tx.send(address).unwrap();
                    let mut stream = scope.accept_inbound().await?;
                    ensure!(stream.read_frame().await? == b"hello", "payload");
                    let started = Instant::now();
                    let elapsed = stream.read_frame().await.unwrap_err();
                    ensure!(
                        elapsed.is::<IrohFrameDeadlineV1>(),
                        "owned frame timeout lost its type: {elapsed:#}"
                    );
                    ensure!(
                        crate::discovery_control::is_service_deadline_failure(&elapsed),
                        "own timer not recognized"
                    );
                    ensure!(
                        !crate::carrier_stream::is_carrier_io_failure(&elapsed),
                        "local budget became remote IO"
                    );
                    use crate::bound_session::BoundSessionError;
                    ensure!(
                        crate::discovery_control::is_service_deadline_failure(
                            &BoundSessionError::Transport(IrohFrameDeadlineV1(()).into()).into()
                        ),
                        "transport timer lost classification"
                    );
                    ensure!(
                        !crate::discovery_control::is_service_deadline_failure(
                            &BoundSessionError::Local(IrohFrameDeadlineV1(()).into()).into()
                        ),
                        "local authority rejection became a retry"
                    );
                    ensure!(
                        !crate::discovery_control::is_service_deadline_failure(&anyhow::anyhow!(
                            "carrier frame operation reached its original deadline"
                        )),
                        "text spoofed owned timer"
                    );
                    ensure!(
                        !crate::discovery_control::is_service_deadline_failure(
                            &BoundSessionError::Authentication(elapsed).into()
                        ),
                        "authentication rejection became a retry"
                    );
                    ensure!(
                        started.elapsed() < Duration::from_secs(4),
                        "unbounded listener read"
                    );
                    ensure!(
                        stream.write_frame(b"late").await.is_err(),
                        "listener revived old stream"
                    );
                    // A new connection has a NEW short budget, same endpoint.
                    drop(stream);
                    let mut stream = scope.accept_inbound().await?;
                    ensure!(stream.read_frame().await? == b"next", "new request");
                    stream.write_frame(b"ok").await?;
                    stream.finish().await
                })
            },
        );
        let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                let peer = rx.await?;
                let mut stream = scope.connect(&peer).await?;
                stream.write_frame(b"hello").await?;
                ensure!(stream.read_frame().await.is_err(), "late response accepted");
                drop(stream);
                let mut stream = scope.connect(&peer).await?;
                stream.write_frame(b"next").await?;
                ensure!(stream.read_frame().await? == b"ok", "fresh response");
                stream.finish().await
            })
        });
        let (a, b) = tokio::join!(server, client);
        a.unwrap();
        b.unwrap();
    }

    #[tokio::test]
    async fn relay_only_online_wait_is_revoked_and_releases_its_endpoint_admission() {
        let _permit = SERIAL.acquire().await.unwrap();
        let revoked = Arc::new(AtomicBool::new(false));
        let check_revoked = revoked.clone();
        let gate = IrohControlV1::new(Instant::now() + Duration::from_secs(5), move || {
            ensure!(
                !check_revoked.load(Ordering::Acquire),
                "relay scope revoked"
            );
            Ok(())
        })
        .unwrap();
        let dropped = Arc::new(AtomicBool::new(false));
        struct DropMarker(Arc<AtomicBool>);
        impl Drop for DropMarker {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let marker = DropMarker(dropped.clone());
        let (ready, started) = oneshot::channel();
        let operation = with_iroh_scope_v1(local_relay_only_config(), gate, move |scope| {
            Box::pin(async move {
                let _marker = marker;
                assert!(scope.endpoint.bound_sockets().is_empty());
                ready.send(()).unwrap();
                scope.online().await
            })
        });
        let trigger = async {
            started.await.unwrap();
            // Give the real online future a poll; no server is required on the
            // loopback relay. Revocation must interrupt the entire owner.
            tokio::task::yield_now().await;
            revoked.store(true, Ordering::Release);
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(operation, trigger)
        })
        .await
        .expect("relay-only online wait ignored revocation");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("relay scope revoked"));
        assert!(dropped.load(Ordering::Acquire));
        assert_eq!(ENDPOINT_LIMIT.get().unwrap().available_permits(), 4);
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
    async fn cancelled_endpoint_close_releases_socket_before_runtime_exit() {
        let _permit = SERIAL.acquire().await.unwrap();
        let bind = || async {
            Endpoint::builder(presets::Minimal)
                .alpns(vec![IROH_ALPN_V1.to_vec()])
                .clear_address_lookup()
                .relay_mode(RelayMode::Disabled)
                .clear_ip_transports()
                .bind_addr("127.0.0.1:0".parse::<SocketAddr>().unwrap())
                .unwrap()
                .bind()
                .await
                .unwrap()
        };
        let client = bind().await;
        let server = bind().await;
        let port = client.bound_sockets()[0];
        let accepted = async { server.accept().await.unwrap().await.unwrap() };
        let (connection, incoming) =
            tokio::join!(client.connect(server.addr(), IROH_ALPN_V1), accepted);
        let connection = connection.unwrap();
        let mut close = Box::pin(client.close());
        assert!(futures_util::poll!(close.as_mut()).is_pending());
        // The close has started, but its owner is cancelled while QUIC drains.
        drop(close);
        assert!(client.is_closed(), "cancelled close left a live endpoint");
        drop(connection);
        drop(client);
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if std::net::UdpSocket::bind(port).is_ok() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "cancelled close retained UDP socket"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        drop(incoming);
        server.close().await;
    }

    #[tokio::test]
    async fn bounded_drain_joins_local_drivers_without_waiting_for_unreachable_peer() {
        let _permit = SERIAL.acquire().await.unwrap();
        let sink = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Endpoint::builder(presets::Minimal)
            .alpns(vec![IROH_ALPN_V1.to_vec()])
            .clear_address_lookup()
            .relay_mode(RelayMode::Disabled)
            .clear_ip_transports()
            .bind_addr("127.0.0.1:0".parse::<SocketAddr>().unwrap())
            .unwrap()
            .transport_config(
                QuicTransportConfig::builder()
                    .initial_rtt(Duration::from_secs(3))
                    .build(),
            )
            .bind()
            .await
            .unwrap();
        let port = endpoint.bound_sockets()[0];
        let peer = EndpointAddr::new(iroh::SecretKey::from_bytes(&[7; 32]).public())
            .with_addrs([TransportAddr::Ip(sink.local_addr().unwrap())]);
        let mut connecting = Box::pin(endpoint.connect(peer, IROH_ALPN_V1));
        let mut packet = [0u8; 2048];
        tokio::select! {
            _ = &mut connecting => panic!("blackhole unexpectedly connected"),
            packet = tokio::time::timeout(Duration::from_secs(2), sink.recv(&mut packet)) => {
                assert!(packet.unwrap().unwrap() > 0);
            }
        }
        // Actual unacknowledged QUIC traffic, no mock close future. The remote
        // drain exceeds this budget; local drivers must nevertheless be joined.
        let started = Instant::now();
        let drained = tokio::time::timeout(
            Duration::from_secs(2),
            endpoint.close_with_drain_timeout(Duration::from_millis(100)),
        )
        .await
        .unwrap();
        assert_eq!(drained, Some(false));
        assert!(endpoint.is_closed());
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(
            std::net::UdpSocket::bind(port).is_ok(),
            "completed close retained UDP socket while endpoint handles remain"
        );
        drop(connecting);
        drop(endpoint);
    }

    #[test]
    fn completed_close_waits_for_operating_system_socket_release() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(1)
            .build()
            .unwrap();
        runtime.block_on(async {
            let _permit = SERIAL.acquire().await.unwrap();
            let endpoint = Endpoint::builder(presets::Minimal)
                .alpns(vec![IROH_ALPN_V1.to_vec()])
                .clear_address_lookup()
                .relay_mode(RelayMode::Disabled)
                .clear_ip_transports()
                .bind_addr("127.0.0.1:0".parse::<SocketAddr>().unwrap())
                .unwrap()
                .bind()
                .await
                .unwrap();
            let port = endpoint.bound_sockets()[0];

            // netwatch closes the OS socket on Tokio's blocking pool. Hold its
            // sole worker to distinguish a joined close from a detached Drop.
            // Dropping the sender also releases the worker on assertion panic.
            let (release, held) = std::sync::mpsc::channel::<()>();
            let (ready, started) = oneshot::channel();
            let blocker = tokio::task::spawn_blocking(move || {
                let _ = ready.send(());
                let _ = held.recv();
            });
            started.await.unwrap();
            let mut close = Box::pin(endpoint.close_with_drain_timeout(Duration::from_millis(100)));
            assert!(
                tokio::time::timeout(Duration::from_millis(250), &mut close)
                    .await
                    .is_err(),
                "close reported local completion before its OS close could run"
            );
            assert!(std::net::UdpSocket::bind(port).is_err());
            drop(release);
            blocker.await.unwrap();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(2), close)
                    .await
                    .unwrap(),
                Some(true)
            );
            assert!(endpoint.is_closed());
            // No Drop, sleep or bind retry may be needed after Some(...).
            assert!(std::net::UdpSocket::bind(port).is_ok());
        });
    }

    #[tokio::test]
    async fn draining_endpoint_refuses_new_handshake_before_any_offer() {
        let _permit = SERIAL.acquire().await.unwrap();
        let sink = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Endpoint::builder(presets::Minimal)
            .alpns(vec![IROH_ALPN_V1.to_vec()])
            .clear_address_lookup()
            .relay_mode(RelayMode::Disabled)
            .clear_ip_transports()
            .bind_addr("127.0.0.1:0".parse::<SocketAddr>().unwrap())
            .unwrap()
            .transport_config(
                QuicTransportConfig::builder()
                    .initial_rtt(Duration::from_secs(3))
                    .build(),
            )
            .bind()
            .await
            .unwrap();
        let address = endpoint.addr();
        let peer = EndpointAddr::new(iroh::SecretKey::from_bytes(&[7; 32]).public())
            .with_addrs([TransportAddr::Ip(sink.local_addr().unwrap())]);
        let mut pending = Box::pin(endpoint.connect(peer, IROH_ALPN_V1));
        let mut packet = [0u8; 2048];
        tokio::select! {
            _ = &mut pending => panic!("blackhole unexpectedly connected"),
            result = tokio::time::timeout(Duration::from_secs(2), sink.recv(&mut packet)) => {
                assert!(result.unwrap().unwrap() > 0);
            }
        }
        // Hold a real unacknowledged QUIC connection in drain. A new connection
        // to this same approved identity reaches noq's closed-endpoint refusal,
        // before TLS/ALPN or any NOVOVM offer exchange can complete.
        let mut closing = Box::pin(endpoint.close_with_drain_timeout(Duration::from_secs(2)));
        assert!(futures_util::poll!(closing.as_mut()).is_pending());
        let probe = async {
            with_iroh_scope_v1(local_config(), control(), move |scope| {
                Box::pin(async move {
                    let peer = IrohAddressV1 {
                        endpoint_key: *address.id.as_bytes(),
                        direct: address.ip_addrs().copied().collect(),
                        relays: vec![],
                    };
                    let error = match scope.connect(&peer).await {
                        Ok(_) => panic!("closing endpoint accepted a new connection"),
                        Err(error) => error,
                    };
                    assert_eq!(
                        iroh_protocol_diagnostic_v1(&error),
                        Some(IrohProtocolDiagnosticV1 { origin: 3, code: 2 })
                    );
                    assert!(!error.is::<IrohConnectUnavailableV1>());
                    Ok(())
                })
            })
            .await
            .unwrap();
        };
        let (closed, ()) = tokio::join!(closing, probe);
        assert_eq!(closed, Some(false));
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

    #[derive(Clone, Copy)]
    enum StreamFaultFixture {
        Disconnect,
        OversizedLength,
        TrailingData,
    }

    async fn real_stream_fault(fault: StreamFaultFixture) -> Result<anyhow::Error> {
        let (a_tx, a_rx) = oneshot::channel();
        let (b_tx, b_rx) = oneshot::channel();
        let (done_tx, done_rx) = oneshot::channel();
        let server = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                a_tx.send(scope.address()?).unwrap();
                let peer: IrohAddressV1 = b_rx.await?;
                let mut stream = scope.accept(peer.endpoint_key).await?;
                ensure!(stream.read_frame().await? == b"ready", "fixture not ready");
                stream.write_frame(b"accepted").await?;
                let error = match fault {
                    StreamFaultFixture::TrailingData => stream.finish().await.unwrap_err(),
                    _ => stream.read_frame().await.unwrap_err(),
                };
                done_tx.send(()).unwrap();
                Ok(error)
            })
        });
        let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                b_tx.send(scope.address()?).unwrap();
                let mut stream = scope.connect(&a_rx.await?).await?;
                stream.write_frame(b"ready").await?;
                ensure!(
                    stream.read_frame().await? == b"accepted",
                    "fixture not accepted"
                );
                match fault {
                    StreamFaultFixture::Disconnect => {
                        stream.connection.close(42u32.into(), b"fixture disconnect");
                    }
                    StreamFaultFixture::OversizedLength => {
                        // Bypass the local frame encoder to exercise a real
                        // malicious wire prefix before receiver allocation.
                        stream
                            .send
                            .write_all(&((IROH_MAX_FRAME_V1 + 1) as u32).to_be_bytes())
                            .await?;
                    }
                    StreamFaultFixture::TrailingData => {
                        stream.send.write_all(b"unexpected").await?;
                    }
                }
                // Keep both endpoint owners alive until the actual read or
                // finish error is classified; fixture drop must not mask it.
                done_rx.await?;
                Ok(())
            })
        });
        let (server, client) = tokio::join!(server, client);
        client?;
        server
    }

    #[tokio::test]
    async fn real_quic_disconnect_marks_only_actual_stream_io_failure() -> Result<()> {
        let _permit = SERIAL.acquire().await?;
        let error = real_stream_fault(StreamFaultFixture::Disconnect).await?;
        let marker = error
            .downcast_ref::<IrohStreamIoFailureV1>()
            .context("actual peer disconnect did not retain stream IO classification")?;
        assert!(
            std::error::Error::source(marker).is_some(),
            "lost underlying QUIC failure"
        );
        Ok(())
    }

    #[tokio::test]
    async fn real_quic_oversized_frame_and_trailing_bytes_are_not_io_failures() -> Result<()> {
        let _permit = SERIAL.acquire().await?;
        for (fault, expected) in [
            (StreamFaultFixture::OversizedLength, "carrier frame limit"),
            (
                StreamFaultFixture::TrailingData,
                "unexpected trailing stream data",
            ),
        ] {
            let error = real_stream_fault(fault).await?;
            assert!(
                !error.is::<IrohStreamIoFailureV1>(),
                "wire violation became transport failure"
            );
            assert_eq!(error.to_string(), expected);
        }
        Ok(())
    }

    #[tokio::test]
    async fn real_quic_peer_stop_is_explicit_stream_interruption() -> Result<()> {
        let _permit = SERIAL.acquire().await?;
        let (a_tx, a_rx) = oneshot::channel();
        let (b_tx, b_rx) = oneshot::channel();
        let (done_tx, done_rx) = oneshot::channel();
        let server = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                a_tx.send(scope.address()?).unwrap();
                let peer: IrohAddressV1 = b_rx.await?;
                let mut stream = scope.accept(peer.endpoint_key).await?;
                ensure!(stream.read_frame().await? == b"ready");
                stream.write_frame(b"accepted").await?;
                // Exercise the same real stopped() check used by finish. Do
                // not send local FIN here: its transport ACK could race with
                // STOP and legitimately complete before the peer interrupts.
                let error = confirm_send_completion(&mut stream.send).await.unwrap_err();
                assert!(
                    error.is::<IrohStreamIoFailureV1>(),
                    "peer STOP was not classified: {error:#}"
                );
                assert!(error.to_string().contains("peer stopped stream"));
                done_tx.send(()).unwrap();
                Ok(())
            })
        });
        let client = with_iroh_scope_v1(local_config(), control(), move |scope| {
            Box::pin(async move {
                b_tx.send(scope.address()?).unwrap();
                let mut stream = scope.connect(&a_rx.await?).await?;
                stream.write_frame(b"ready").await?;
                ensure!(stream.read_frame().await? == b"accepted");
                stream.recv.stop(7u32.into())?;
                done_rx.await?;
                Ok(())
            })
        });
        let (server, client) = tokio::join!(server, client);
        server?;
        client?;
        Ok(())
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
                        let error = writer.write_frame(frame).await.unwrap_err();
                        assert!(!error.is::<IrohStreamIoFailureV1>());
                        assert!(error.to_string().contains("frame limit"));
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
                    let denied = denied.unwrap_err();
                    assert!(!denied.is::<IrohStreamIoFailureV1>());
                    assert!(denied.to_string().contains("test gate revoked"));
                    let failed_read = reader.read_frame().await.unwrap_err();
                    assert!(!failed_read.is::<IrohStreamIoFailureV1>());
                    assert!(failed_read.to_string().contains("stream failed"));
                    let failed_write = writer.write_frame(b"still forbidden").await.unwrap_err();
                    assert!(!failed_write.is::<IrohStreamIoFailureV1>());
                    assert!(failed_write.to_string().contains("stream failed"));
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
