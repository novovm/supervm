//! Locally migrated from legacy/supervm-20261002/crates/novovm-node/src/product_relay_daemon.rs.
//! Transport only: relay admission is not consensus finality or execution validity.
//!
//! Headless WSS relay daemon for the product overlay runtime.
//!
//! TLS is a transport confidentiality layer only. Node authentication is the signed NOVOVM
//! challenge-response performed after the WebSocket upgrade; the relay never decrypts E2E frames.

use crate::product_relay_client::{
    PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_BYTES_V1,
    PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_EVENTS_V1,
};
use crate::product_relay_wire::{
    decode_message_v2, encode_message_v2, PRODUCT_RELAY_WEBSOCKET_SUBPROTOCOL_V2,
};
use crate::{
    HandshakeReplayCacheV1, NodeHandshakeResponderV1, ProductRelayRuntimeConfigV1,
    ProductRelaySessionManagerV1, ProductRelayWireMessageV1,
    PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1,
};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::SigningKey;
use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
use serde::{Deserialize, Serialize};
use sha1::{Digest as Sha1Digest, Sha1};
use std::{
    cell::Cell,
    fs,
    io::{self, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::runtime::{Builder as TokioRuntimeBuilder, Runtime};

mod connection;
mod pump;

pub(crate) const PRODUCT_RELAY_DAEMON_VERSION_V2: u16 = 2;
const PRODUCT_RELAY_WEBSOCKET_PATH_V1: &str = "/novovm";
// Keep physical admission above the default authenticated-session ceiling so authenticated
// sessions alone cannot consume every physical connection slot. This headroom is not a
// reservation: unauthenticated slow connections can still consume it.
const DEFAULT_MAX_CONNECTIONS_V1: usize = 512;
const DEFAULT_HANDSHAKE_TIMEOUT_MS_V1: u64 = 5_000;
const MAX_HANDSHAKE_WIRE_MESSAGE_BYTES_V1: usize = 16 * 1024;
const MAX_WEBSOCKET_CONTROL_FRAME_BYTES_V1: usize = 125;
const PRODUCT_RELAY_FRAME_DEADLINE_MS_V1: u64 = 10_000;
const PRODUCT_RELAY_MAINTENANCE_INTERVAL_MS_V1: u64 = 1_000;
// Per-turn fairness is separate from the cumulative connection delivery
// window below. Neither a wake nor a timeout replenishes unconsumed credit.
const MAX_DATA_DELIVERIES_PER_CONNECTION_TICK_V1: usize =
    PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_BYTES_V1 / PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1
        - MAX_PEER_HANDSHAKE_DELIVERIES_PER_CONNECTION_TICK_V1
        - 1;
const MAX_PEER_HANDSHAKE_DELIVERIES_PER_CONNECTION_TICK_V1: usize = 4;
const _: () = assert!(
    MAX_DATA_DELIVERIES_PER_CONNECTION_TICK_V1
        + MAX_PEER_HANDSHAKE_DELIVERIES_PER_CONNECTION_TICK_V1
        < PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_EVENTS_V1
);
const _: () = assert!(
    crate::product_relay::PRODUCT_RELAY_DELIVERY_WINDOW_V1 as usize
        * PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1
        < PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_BYTES_V1
);
const _: () = assert!(
    crate::product_relay::PRODUCT_RELAY_DELIVERY_WINDOW_V1 as usize + 1
        < PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_EVENTS_V1
);

#[derive(Default)]
struct RelayDeliveryWindowV1 {
    sent: u64,
    consumed: u64,
}

impl RelayDeliveryWindowV1 {
    fn available(&self) -> usize {
        (crate::product_relay::PRODUCT_RELAY_DELIVERY_WINDOW_V1 - (self.sent - self.consumed))
            as usize
    }

    fn acknowledge(&mut self, through: u64) -> Result<()> {
        if through < self.consumed || through > self.sent {
            bail!("invalid relay delivery consumption watermark");
        }
        self.consumed = through;
        Ok(())
    }

    fn sent(&mut self) -> Result<()> {
        if self.available() == 0 {
            bail!("relay delivery window exhausted");
        }
        self.sent = self
            .sent
            .checked_add(1)
            .context("relay delivery counter overflow")?;
        Ok(())
    }
}
const _: () = assert!(
    (MAX_DATA_DELIVERIES_PER_CONNECTION_TICK_V1
        + MAX_PEER_HANDSHAKE_DELIVERIES_PER_CONNECTION_TICK_V1)
        * PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1
        < PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_BYTES_V1
);

#[derive(Debug, Clone, Deserialize)]
pub struct ProductRelayDaemonConfigV1 {
    pub bind_addr: String,
    pub tls_cert_path: PathBuf,
    pub tls_key_path: PathBuf,
    pub relay_identity_key_path: PathBuf,
    pub report_path: PathBuf,
    #[serde(default = "default_report_interval_ms_v1")]
    pub report_interval_ms: u64,
    /// A bounded duration is useful for deterministic smoke runs. Omit for a long-lived daemon.
    #[serde(default)]
    pub run_for_ms: Option<u64>,
    /// Hard physical-socket admission bound, including TLS/WebSocket pre-authentication work.
    #[serde(default)]
    pub max_connections: Option<usize>,
    /// Absolute TLS + WebSocket + signed-node-handshake wall-clock budget.
    #[serde(default)]
    pub handshake_timeout_ms: Option<u64>,
    #[serde(default)]
    pub max_sessions: Option<usize>,
    #[serde(default)]
    pub max_tracked_sources: Option<usize>,
    #[serde(default)]
    pub session_queue_capacity: Option<usize>,
    #[serde(default)]
    pub session_queue_bytes: Option<usize>,
    #[serde(default)]
    pub active_queue_total: Option<usize>,
    #[serde(default)]
    pub active_queue_bytes_total: Option<usize>,
    #[serde(default)]
    pub offline_queue_per_peer: Option<usize>,
    #[serde(default)]
    pub offline_queue_bytes_per_peer: Option<usize>,
    #[serde(default)]
    pub offline_queue_per_source: Option<usize>,
    #[serde(default)]
    pub offline_queue_bytes_per_source: Option<usize>,
    #[serde(default)]
    pub offline_queue_total: Option<usize>,
    #[serde(default)]
    pub offline_queue_bytes_total: Option<usize>,
    #[serde(default)]
    pub offline_queue_ttl_ms: Option<u64>,
    #[serde(default)]
    pub session_ttl_ms: Option<u64>,
    #[serde(default)]
    pub rate_limit_frames: Option<u64>,
    #[serde(default)]
    pub max_frames_per_window: Option<u64>,
    #[serde(default)]
    pub rate_limit_window_ms: Option<u64>,
    #[serde(default)]
    pub source_bytes_per_minute: Option<u64>,
    #[serde(default)]
    pub max_bytes_per_minute: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductRelayDaemonReportV1 {
    pub accepted: bool,
    pub scope: String,
    pub daemon_version: u16,
    pub listen_addr: String,
    pub websocket_path: String,
    pub transport: String,
    pub report_updated_at_ms: u64,
    pub graceful_shutdown: bool,
    pub tls_transport_enabled: bool,
    pub ca_trust_required_for_novovm_identity: bool,
    pub node_identity_challenge_response_required: bool,
    pub payload_treated_opaque: bool,
    pub relay_is_trusted_authority: bool,
    pub business_semantics_interpreted_by_relay: bool,
    pub novorudp_wire_changed: bool,
    #[serde(default)]
    pub max_connection_count: usize,
    #[serde(default)]
    pub active_connection_count: usize,
    #[serde(default)]
    pub rejected_connection_total: u64,
    pub relay_runtime: crate::RelayRuntimeSnapshotV1,
}

#[derive(Debug)]
struct ProductRelayConnectionAdmissionV1 {
    max_connections: usize,
    active_connections: AtomicUsize,
    rejected_connections: AtomicU64,
}

#[derive(Debug)]
struct ProductRelayConnectionPermitV1 {
    admission: Arc<ProductRelayConnectionAdmissionV1>,
}

#[derive(Clone)]
struct ProductRelayConnectionContextV1 {
    tls_config: Arc<rustls::ServerConfig>,
    relay_identity: SigningKey,
    manager: ProductRelaySessionManagerV1,
    runtime: Arc<Runtime>,
    replay_cache: Arc<Mutex<HandshakeReplayCacheV1>>,
    stopping: Arc<AtomicBool>,
    handshake_timeout_ms: u64,
}

impl ProductRelayConnectionAdmissionV1 {
    fn new(max_connections: usize) -> Result<Arc<Self>> {
        if max_connections == 0 {
            bail!("max_connections must be positive");
        }
        Ok(Arc::new(Self {
            max_connections,
            active_connections: AtomicUsize::new(0),
            rejected_connections: AtomicU64::new(0),
        }))
    }

    fn try_acquire(self: &Arc<Self>) -> Option<ProductRelayConnectionPermitV1> {
        let acquired = self
            .active_connections
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < self.max_connections).then_some(active.saturating_add(1))
            })
            .is_ok();
        if acquired {
            Some(ProductRelayConnectionPermitV1 {
                admission: Arc::clone(self),
            })
        } else {
            self.rejected_connections.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}

impl Drop for ProductRelayConnectionPermitV1 {
    fn drop(&mut self) {
        self.admission
            .active_connections
            .fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Debug)]
enum WebSocketFrameV1 {
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close,
}

pub fn load_product_relay_daemon_config_v1(
    path: impl AsRef<Path>,
) -> Result<ProductRelayDaemonConfigV1> {
    let path = path.as_ref();
    let bytes =
        fs::read(path).with_context(|| format!("read relay daemon config: {}", path.display()))?;
    let config = serde_json::from_slice(&bytes)
        .with_context(|| format!("decode relay daemon config: {}", path.display()))?;
    Ok(config)
}

pub fn run_product_relay_daemon_v1(config: ProductRelayDaemonConfigV1) -> Result<()> {
    run_product_relay_daemon_with_shutdown_v1(config, Arc::new(AtomicBool::new(false)))
}

/// Run until the configured duration elapses or the caller requests shutdown.
///
/// The signal is shared with all connection workers. A caller that spawns this
/// function owns its thread and must signal shutdown and join it before dropping
/// the surrounding service or test scope. Do not reset the signal while running.
pub fn run_product_relay_daemon_with_shutdown_v1(
    config: ProductRelayDaemonConfigV1,
    stopping: Arc<AtomicBool>,
) -> Result<()> {
    if config.report_interval_ms == 0 {
        bail!("report_interval_ms must be positive");
    }
    let handshake_timeout_ms = config
        .handshake_timeout_ms
        .unwrap_or(DEFAULT_HANDSHAKE_TIMEOUT_MS_V1);
    if handshake_timeout_ms == 0 {
        bail!("handshake_timeout_ms must be positive");
    }
    let relay_runtime_config = relay_runtime_config_v1(&config);
    let admission = ProductRelayConnectionAdmissionV1::new(resolve_max_connections_v1(
        config.max_connections,
        relay_runtime_config.max_sessions,
    ))?;
    let relay_identity = load_ed25519_signing_key_v1(&config.relay_identity_key_path)?;
    let tls_config = Arc::new(build_server_tls_config_v1(
        &config.tls_cert_path,
        &config.tls_key_path,
    )?);
    let listener = TcpListener::bind(&config.bind_addr)
        .with_context(|| format!("bind product relay: {}", config.bind_addr))?;
    listener
        .set_nonblocking(true)
        .context("set product relay listener nonblocking")?;
    let listen_addr = listener
        .local_addr()
        .context("read product relay listen addr")?
        .to_string();

    let runtime = Arc::new(
        TokioRuntimeBuilder::new_multi_thread()
            .enable_all()
            .build()
            .context("create product relay async runtime")?,
    );
    validate_connection_session_headroom_v1(
        admission.max_connections,
        relay_runtime_config.max_sessions,
    )?;
    let manager = runtime
        .block_on(async { ProductRelaySessionManagerV1::new(relay_runtime_config) })
        .context("create product relay session manager")?;
    let replay_cache = Arc::new(Mutex::new(HandshakeReplayCacheV1::default()));
    let started_at = Instant::now();
    let mut last_report_ms = 0u64;
    let mut last_maintenance_ms = 0u64;
    let mut connection_workers = Vec::new();

    // Capture all loop errors so report, accept, or spawn failures cannot detach
    // already-running connection workers or leave their Tokio runtime alive.
    let run_result = (|| -> Result<&'static str> {
        loop {
            if stopping.load(Ordering::Acquire) {
                return Ok("external_request");
            }
            if config
                .run_for_ms
                .is_some_and(|duration| started_at.elapsed() >= Duration::from_millis(duration))
            {
                return Ok("run_duration_elapsed");
            }
            match listener.accept() {
                Ok((tcp, _)) => {
                    if let Some(permit) = admission.try_acquire() {
                        let connection_context = ProductRelayConnectionContextV1 {
                            tls_config: Arc::clone(&tls_config),
                            relay_identity: relay_identity.clone(),
                            manager: manager.clone(),
                            runtime: Arc::clone(&runtime),
                            replay_cache: Arc::clone(&replay_cache),
                            stopping: Arc::clone(&stopping),
                            handshake_timeout_ms,
                        };
                        let worker = thread::Builder::new()
                            .name("novovm-product-relay-connection".into())
                            .spawn(move || {
                                let _permit = permit;
                                if let Err(error) =
                                    serve_product_relay_connection_v1(tcp, connection_context)
                                {
                                    eprintln!("product relay connection closed: {error:#}");
                                }
                            })
                            .context("spawn product relay connection worker")?;
                        connection_workers.push(worker);
                    } else {
                        drop(tcp);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error).context("accept product relay connection"),
            }

            reap_finished_connection_workers_v1(&mut connection_workers);

            let now_ms = now_ms_v1();
            if now_ms.saturating_sub(last_maintenance_ms)
                >= PRODUCT_RELAY_MAINTENANCE_INTERVAL_MS_V1
            {
                runtime.block_on(manager.expire_stale_sessions(now_ms));
                last_maintenance_ms = now_ms;
            }
            if now_ms.saturating_sub(last_report_ms) >= config.report_interval_ms {
                write_product_relay_report_v1(
                    &config.report_path,
                    &listen_addr,
                    &runtime,
                    &manager,
                    &admission,
                    false,
                )?;
                last_report_ms = now_ms;
            }
        }
    })();

    stopping.store(true, Ordering::Release);
    drop(listener);
    manager.begin_graceful_shutdown();
    for worker in connection_workers {
        if worker.join().is_err() {
            eprintln!("product relay connection worker panicked during shutdown");
        }
    }
    eprintln!(
        "product relay stopped: reason={} elapsed_ms={}",
        run_result.as_ref().copied().unwrap_or("error"),
        started_at.elapsed().as_millis(),
    );
    if let Err(error) = &run_result {
        // This loop propagates only accept/spawn/report errors, never key or
        // payload material. Preserve the cause even if an owner drops the result.
        eprintln!("product relay run failed: {error:#}");
    }
    let shutdown_report = write_product_relay_report_v1(
        &config.report_path,
        &listen_addr,
        &runtime,
        &manager,
        &admission,
        true,
    );
    match run_result {
        Ok(_) => shutdown_report,
        Err(error) => {
            if let Err(report_error) = shutdown_report {
                eprintln!("product relay shutdown report failed: {report_error:#}");
            }
            Err(error)
        }
    }
}

fn validate_connection_session_headroom_v1(
    max_connections: usize,
    max_sessions: usize,
) -> Result<()> {
    if max_connections <= max_sessions {
        bail!(
            "max_connections ({max_connections}) must exceed max_sessions ({max_sessions}) so authenticated sessions alone cannot consume every physical connection slot"
        );
    }
    Ok(())
}

fn resolve_max_connections_v1(
    explicit_max_connections: Option<usize>,
    max_sessions: usize,
) -> usize {
    explicit_max_connections
        .unwrap_or_else(|| DEFAULT_MAX_CONNECTIONS_V1.max(max_sessions.saturating_add(1)))
}

fn reap_finished_connection_workers_v1(workers: &mut Vec<thread::JoinHandle<()>>) {
    let mut index = 0usize;
    while index < workers.len() {
        if workers[index].is_finished() {
            let worker = workers.swap_remove(index);
            if worker.join().is_err() {
                eprintln!("product relay connection worker panicked");
            }
        } else {
            index = index.saturating_add(1);
        }
    }
}

fn serve_product_relay_connection_v1(
    tcp: TcpStream,
    context: ProductRelayConnectionContextV1,
) -> Result<()> {
    let ProductRelayConnectionContextV1 {
        tls_config,
        relay_identity,
        manager,
        runtime,
        replay_cache,
        stopping,
        handshake_timeout_ms,
    } = context;
    let handshake_deadline = Instant::now()
        .checked_add(Duration::from_millis(handshake_timeout_ms))
        .context("product relay handshake deadline overflow")?;
    let deadline_socket = tcp
        .try_clone()
        .context("clone product relay socket for absolute handshake deadline")?;
    let (handshake_finished, handshake_deadline_cancelled) = tokio::sync::oneshot::channel();
    runtime.spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(handshake_timeout_ms)) => {
                let _ = deadline_socket.shutdown(Shutdown::Both);
            }
            _ = handshake_deadline_cancelled => {}
        }
    });
    // The adapter uses safe readiness waits on Windows, keeping the same idle
    // budget without continuing a Winsock connection after SO_RCVTIMEO expiry.
    let mut tcp = crate::product_relay_io::ProductRelaySocketV1::new(tcp)
        .context("prepare product relay connection I/O")?;
    tcp.set_read_timeout(Some(Duration::from_millis(100)))
        .context("set product relay read timeout")?;
    tcp.set_write_timeout(Some(Duration::from_millis(100)))
        .context("set product relay write timeout")?;
    let io_deadline = ProductRelayDaemonIoDeadlineV1::new(Arc::clone(&stopping));
    io_deadline
        .begin_v1(handshake_deadline)
        .context("start product relay lower-stream handshake deadline")?;
    let server =
        rustls::ServerConnection::new(tls_config).context("create product relay tls connection")?;
    let mut websocket = rustls::StreamOwned::new(
        server,
        ProductRelayDaemonDeadlineTcpStreamV1 {
            inner: tcp,
            deadline: io_deadline.clone(),
            inbound_tls_records: pump::InboundTlsRecords::default(),
            #[cfg(test)]
            test_writes: ProductRelayDaemonTestWritesV1::default(),
        },
    );
    accept_websocket_until_v1(&mut websocket, handshake_deadline, &stopping)?;

    let offer = match read_websocket_frame_until_v1(
        &mut websocket,
        true,
        MAX_HANDSHAKE_WIRE_MESSAGE_BYTES_V1,
        handshake_deadline,
        &stopping,
    )? {
        WebSocketFrameV1::Binary(bytes) => {
            match serde_json::from_slice(&bytes).context("decode relay handshake offer")? {
                ProductRelayWireMessageV1::HandshakeOffer(offer) => offer,
                _ => bail!("first product relay message must be a handshake offer"),
            }
        }
        _ => bail!("first product relay WebSocket frame must be binary"),
    };
    let responder = {
        let mut replay_cache = replay_cache
            .lock()
            .map_err(|_| anyhow::anyhow!("relay handshake replay cache poisoned"))?;
        NodeHandshakeResponderV1::respond(
            &offer,
            &relay_identity,
            now_ms_v1(),
            30_000,
            &mut replay_cache,
        )
        .context("verify product relay node handshake")?
    };
    let authenticated = responder.authenticated_remote().clone();
    let (registration, mut inbox) = runtime
        .block_on(manager.register_authenticated_session(authenticated, now_ms_v1()))
        .context("register authenticated product relay session")?;

    let peer_id = registration.peer_id;
    let session_id = registration.session_id;
    if let Err(error) = write_wire_message_v1(
        &mut websocket,
        &ProductRelayWireMessageV1::HandshakeResponse(responder.response().clone()),
    ) {
        runtime.block_on(manager.disconnect(&peer_id, session_id));
        return Err(error).context("write admitted product relay handshake response");
    }
    // Flow-control v1 is mandatory before queued deliveries. It is distinct
    // from the signed node identity handshake and from all chain protocols.
    // No legacy fallback, new timeout, or unbounded pre-negotiation egress.
    let flow_negotiation = (|| -> Result<()> {
        write_wire_message_v1(
            &mut websocket,
            &ProductRelayWireMessageV1::DeliveryWindowV1 {
                max_unconsumed: crate::product_relay::PRODUCT_RELAY_DELIVERY_WINDOW_V1,
            },
        )?;
        let WebSocketFrameV1::Binary(bytes) = read_websocket_frame_until_v1(
            &mut websocket,
            true,
            MAX_HANDSHAKE_WIRE_MESSAGE_BYTES_V1,
            handshake_deadline,
            &stopping,
        )?
        else {
            bail!("relay delivery-window confirmation must be binary");
        };
        let admission = runtime
            .block_on(manager.admit_authenticated_wire_v1(
                &peer_id,
                session_id,
                bytes.len(),
                now_ms_v1(),
            ))
            .map_err(|reason| anyhow::anyhow!("relay window confirmation rejected: {reason:?}"))?;
        if !matches!(
            serde_json::from_slice(&bytes),
            Ok(ProductRelayWireMessageV1::DeliveryConsumedV1 { through: 0 })
        ) {
            runtime.block_on(manager.reject_admitted_wire_v1(admission));
            bail!("relay delivery-window v1 confirmation required");
        }
        if !runtime.block_on(manager.heartbeat_admitted_v1(admission, now_ms_v1())) {
            bail!("relay delivery-window confirmation session expired");
        }
        Ok(())
    })();
    if let Err(error) = flow_negotiation {
        runtime.block_on(manager.disconnect(&peer_id, session_id));
        return Err(error);
    }
    websocket.sock.inner.enable_duplex_read_ahead();
    let _ = handshake_finished.send(());
    // The incremental owner inherits any already-buffered input's original
    // deadline before retiring the completed handshake operation.
    let result = relay_incremental_connection_loop_v1(
        websocket,
        ProductRelayConnectionLoopV1 {
            manager: &manager,
            runtime: &runtime,
            peer_id: &peer_id,
            session_id,
            inbox: &mut inbox,
            stopping: &stopping,
            #[cfg(test)]
            io_deadline: None,
            #[cfg(test)]
            read_waker: None,
        },
    );
    runtime.block_on(manager.disconnect(&peer_id, session_id));
    result
}

struct ProductRelayConnectionLoopV1<'a> {
    manager: &'a ProductRelaySessionManagerV1,
    runtime: &'a Runtime,
    peer_id: &'a str,
    session_id: [u8; 16],
    inbox: &'a mut crate::RelaySessionInboxV1,
    stopping: &'a AtomicBool,
    #[cfg(test)]
    io_deadline: Option<&'a ProductRelayDaemonIoDeadlineV1>,
    #[cfg(test)]
    read_waker: Option<&'a std::task::Waker>,
}

fn relay_incremental_connection_loop_v1(
    websocket: rustls::StreamOwned<rustls::ServerConnection, ProductRelayDaemonDeadlineTcpStreamV1>,
    context: ProductRelayConnectionLoopV1<'_>,
) -> Result<()> {
    connection::run(websocket, context)
}

// Retained as a test-only synchronous reference for the migrated fault gates.
// Production authenticated connections use the incremental owner above.
#[cfg(test)]
fn relay_connection_loop_v1<S: Read + Write>(
    websocket: &mut S,
    context: ProductRelayConnectionLoopV1<'_>,
) -> Result<()> {
    let ProductRelayConnectionLoopV1 {
        manager,
        runtime,
        peer_id,
        session_id,
        inbox,
        stopping,
        io_deadline,
        read_waker,
    } = context;
    let mut prefer_control_delivery = false;
    let mut delivery_window = RelayDeliveryWindowV1::default();
    while !stopping.load(Ordering::Acquire) {
        if !runtime.block_on(manager.is_current_session(peer_id, session_id, now_ms_v1())) {
            bail!("product relay session was replaced, expired, or revoked");
        }
        if delivery_window.available() > 0 {
            if let Some(waker) = read_waker {
                inbox.arm_delivery_wake(waker);
            }
        }
        let frame_deadline = Instant::now()
            .checked_add(Duration::from_millis(PRODUCT_RELAY_FRAME_DEADLINE_MS_V1))
            .context("product relay frame deadline overflow")?;
        if let Some(io_deadline) = io_deadline {
            io_deadline
                .begin_if_idle_v1(frame_deadline)
                .context("start product relay lower-stream frame deadline")?;
        }
        let mut preserve_lower_deadline = false;
        match read_authenticated_websocket_frame_until_v1(
            websocket,
            true,
            PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1,
            frame_deadline,
            stopping,
        ) {
            Ok(WebSocketFrameV1::Binary(bytes)) => {
                let wire_bytes = bytes.len();
                let admission = match runtime.block_on(manager.admit_authenticated_wire_v1(
                    peer_id,
                    session_id,
                    wire_bytes,
                    now_ms_v1(),
                )) {
                    Ok(admission) => admission,
                    Err(disposition) => {
                        bail!("product relay rejected raw authenticated wire admission: {disposition:?}")
                    }
                };
                let message: ProductRelayWireMessageV1 = match decode_message_v2(&bytes) {
                    Ok(message) => message,
                    Err(error) => {
                        runtime.block_on(manager.reject_admitted_wire_v1(admission));
                        return Err(error).context("decode product relay wire message");
                    }
                };
                // Dispatch owns its decoded fields; the admitted byte count is
                // already bound. Do not retain a second input JSON allocation
                // while admission prepares the immutable egress wire image.
                drop(bytes);
                match message {
                    ProductRelayWireMessageV1::Data(envelope) => {
                        let outcome = runtime.block_on(manager.forward_opaque_admitted_v1(
                            admission,
                            envelope,
                            now_ms_v1(),
                        ));
                        let disposition = outcome.disposition;
                        write_wire_message_v1(
                            websocket,
                            &ProductRelayWireMessageV1::ForwardOutcome(outcome),
                        )?;
                        if relay_forward_disposition_requires_close_v1(disposition) {
                            bail!("product relay rejected data forward: {disposition:?}");
                        }
                    }
                    ProductRelayWireMessageV1::PeerHandshake {
                        target_peer_id,
                        handshake,
                    } => {
                        let outcome = runtime.block_on(manager.forward_peer_handshake_admitted_v1(
                            admission,
                            &target_peer_id,
                            handshake,
                            now_ms_v1(),
                        ));
                        let disposition = outcome.disposition;
                        write_wire_message_v1(
                            websocket,
                            &ProductRelayWireMessageV1::ForwardOutcome(outcome),
                        )?;
                        if relay_forward_disposition_requires_close_v1(disposition) {
                            bail!("product relay rejected peer-handshake forward: {disposition:?}");
                        }
                    }
                    ProductRelayWireMessageV1::Heartbeat => {
                        if !runtime.block_on(manager.heartbeat_admitted_v1(admission, now_ms_v1()))
                        {
                            bail!("product relay rejected heartbeat budget or stale session");
                        }
                        write_wire_message_v1(websocket, &ProductRelayWireMessageV1::HeartbeatAck)?;
                    }
                    ProductRelayWireMessageV1::DeliveryConsumedV1 { through } => {
                        // Count the control wire against the same admission
                        // budgets and refresh only THIS current session.
                        if let Err(error) = delivery_window.acknowledge(through) {
                            runtime.block_on(manager.reject_admitted_wire_v1(admission));
                            return Err(error);
                        }
                        if !runtime.block_on(manager.heartbeat_admitted_v1(admission, now_ms_v1()))
                        {
                            bail!("relay rejected delivery consumption session");
                        }
                    }
                    ProductRelayWireMessageV1::Close => return Ok(()),
                    ProductRelayWireMessageV1::HandshakeOffer(_)
                    | ProductRelayWireMessageV1::HandshakeResponse(_)
                    | ProductRelayWireMessageV1::DeliveryWindowV1 { .. }
                    | ProductRelayWireMessageV1::Delivery(_)
                    | ProductRelayWireMessageV1::PeerHandshakeDelivery(_)
                    | ProductRelayWireMessageV1::HeartbeatAck
                    | ProductRelayWireMessageV1::ForwardOutcome(_) => {
                        runtime.block_on(manager.reject_admitted_wire_v1(admission));
                        bail!("invalid relay wire message after authentication")
                    }
                }
                service_one_relay_inbox_v1(
                    websocket,
                    manager,
                    runtime,
                    (peer_id, session_id),
                    inbox,
                    &mut prefer_control_delivery,
                    &mut delivery_window,
                )?;
            }
            Ok(WebSocketFrameV1::Ping(payload)) => {
                if !runtime.block_on(manager.ping_with_wire_bytes(
                    peer_id,
                    session_id,
                    payload.len(),
                    now_ms_v1(),
                )) {
                    bail!("product relay rejected ping budget or stale session");
                }
                write_websocket_frame_v1(websocket, 0xA, &payload)?;
                service_one_relay_inbox_v1(
                    websocket,
                    manager,
                    runtime,
                    (peer_id, session_id),
                    inbox,
                    &mut prefer_control_delivery,
                    &mut delivery_window,
                )?;
            }
            Ok(WebSocketFrameV1::Pong(payload)) => {
                if !runtime.block_on(manager.ping_with_wire_bytes(
                    peer_id,
                    session_id,
                    payload.len(),
                    now_ms_v1(),
                )) {
                    bail!("product relay rejected pong budget or stale session");
                }
                service_one_relay_inbox_v1(
                    websocket,
                    manager,
                    runtime,
                    (peer_id, session_id),
                    inbox,
                    &mut prefer_control_delivery,
                    &mut delivery_window,
                )?;
            }
            Ok(WebSocketFrameV1::Close) => return Ok(()),
            Err(error) if is_timeout_v1(&error) => {
                if let Some(io_deadline) = io_deadline {
                    preserve_lower_deadline = io_deadline
                        .preserve_partial_read_deadline_v1()
                        .context("check product relay partial lower-stream deadline")?;
                }
                if stopping.load(Ordering::Acquire) {
                    continue;
                }
                runtime.block_on(manager.drain_queued_for_session(
                    peer_id,
                    session_id,
                    now_ms_v1(),
                ));
                drain_bounded_relay_inbox_v1(
                    MAX_DATA_DELIVERIES_PER_CONNECTION_TICK_V1.min(delivery_window.available()),
                    || stopping.load(Ordering::Acquire),
                    || inbox.try_recv_encoded().ok(),
                    |delivery| {
                        write_encoded_relay_delivery_v1(
                            websocket,
                            manager,
                            runtime,
                            (peer_id, session_id),
                            delivery,
                        )?;
                        delivery_window.sent()
                    },
                )?;
                drain_bounded_relay_inbox_v1(
                    MAX_PEER_HANDSHAKE_DELIVERIES_PER_CONNECTION_TICK_V1
                        .min(delivery_window.available()),
                    || stopping.load(Ordering::Acquire),
                    || inbox.try_recv_peer_handshake_encoded().ok(),
                    |delivery| {
                        write_encoded_relay_delivery_v1(
                            websocket,
                            manager,
                            runtime,
                            (peer_id, session_id),
                            delivery,
                        )?;
                        delivery_window.sent()
                    },
                )?;
            }
            Err(error) => return Err(error),
        }
        if let Some(io_deadline) = io_deadline {
            // A TLS-record slow drip may time out before producing WebSocket plaintext. Preserve
            // its original lower-stream deadline across outer idle ticks; otherwise begin a fresh
            // bounded operation on the next loop.
            if !preserve_lower_deadline {
                io_deadline.clear_v1()?;
            }
        }
    }
    Ok(())
}

fn relay_forward_disposition_requires_close_v1(
    disposition: crate::RelayForwardDispositionV1,
) -> bool {
    matches!(
        disposition,
        crate::RelayForwardDispositionV1::RejectedSourceSessionMissing
            | crate::RelayForwardDispositionV1::RejectedStaleSourceSession
            | crate::RelayForwardDispositionV1::RejectedSourceSessionExpired
            | crate::RelayForwardDispositionV1::RejectedRouteMismatch
            | crate::RelayForwardDispositionV1::RejectedShuttingDown
    )
}

#[cfg(test)]
fn service_one_relay_inbox_v1<S: Write>(
    websocket: &mut S,
    manager: &ProductRelaySessionManagerV1,
    runtime: &Runtime,
    session: (&str, [u8; 16]),
    inbox: &mut crate::RelaySessionInboxV1,
    prefer_control: &mut bool,
    delivery_window: &mut RelayDeliveryWindowV1,
) -> Result<bool> {
    let (peer_id, session_id) = session;
    if delivery_window.available() == 0 {
        return Ok(false);
    }
    runtime.block_on(manager.drain_queued_for_session(peer_id, session_id, now_ms_v1()));
    let item = if *prefer_control {
        inbox
            .try_recv_peer_handshake_encoded()
            .map(|delivery| (delivery, false))
            .or_else(|_| inbox.try_recv_encoded().map(|delivery| (delivery, true)))
    } else {
        inbox
            .try_recv_encoded()
            .map(|delivery| (delivery, true))
            .or_else(|_| {
                inbox
                    .try_recv_peer_handshake_encoded()
                    .map(|delivery| (delivery, false))
            })
    };
    let Ok((delivery, next_prefer_control)) = item else {
        return Ok(false);
    };
    write_encoded_relay_delivery_v1(websocket, manager, runtime, session, delivery)?;
    *prefer_control = next_prefer_control;
    delivery_window.sent()?;
    Ok(true)
}

#[cfg(test)]
fn write_encoded_relay_delivery_v1<S: Write>(
    websocket: &mut S,
    manager: &ProductRelaySessionManagerV1,
    runtime: &Runtime,
    session: (&str, [u8; 16]),
    delivery: crate::product_relay::RelayEncodedDeliveryV1,
) -> Result<()> {
    let (peer_id, session_id) = session;
    if !runtime.block_on(manager.is_current_session(peer_id, session_id, now_ms_v1())) {
        bail!("product relay session was replaced before queued delivery");
    }
    // The original session/global quota guard stays alive through payload
    // writing AND flush, including terminal failures. Never decode/re-encode
    // this immutable, admission-bound wire image on the egress path.
    write_websocket_frame_v1(websocket, 0x2, delivery.wire_bytes())
}

#[cfg(test)]
fn drain_bounded_relay_inbox_v1<T>(
    limit: usize,
    mut should_stop: impl FnMut() -> bool,
    mut try_recv: impl FnMut() -> Option<T>,
    mut deliver: impl FnMut(T) -> Result<()>,
) -> Result<usize> {
    let mut drained = 0usize;
    while drained < limit {
        if should_stop() {
            break;
        }
        let Some(item) = try_recv() else {
            break;
        };
        deliver(item)?;
        drained = drained.saturating_add(1);
    }
    Ok(drained)
}

fn relay_runtime_config_v1(config: &ProductRelayDaemonConfigV1) -> ProductRelayRuntimeConfigV1 {
    let mut runtime = ProductRelayRuntimeConfigV1::default();
    if let Some(value) = config.max_sessions {
        runtime.max_sessions = value;
    }
    if let Some(value) = config.max_tracked_sources {
        runtime.max_tracked_sources = value;
    }
    if let Some(value) = config.session_queue_capacity {
        runtime.session_queue_capacity = value;
    }
    if let Some(value) = config.session_queue_bytes {
        runtime.session_queue_bytes = value;
    }
    if let Some(value) = config.active_queue_total {
        runtime.active_queue_total = value;
    }
    if let Some(value) = config.active_queue_bytes_total {
        runtime.active_queue_bytes_total = value;
    }
    if let Some(value) = config.offline_queue_per_peer {
        runtime.offline_queue_per_peer = value;
    }
    if let Some(value) = config.offline_queue_bytes_per_peer {
        runtime.offline_queue_bytes_per_peer = value;
    }
    if let Some(value) = config.offline_queue_per_source {
        runtime.offline_queue_per_source = value;
    }
    if let Some(value) = config.offline_queue_bytes_per_source {
        runtime.offline_queue_bytes_per_source = value;
    }
    if let Some(value) = config.offline_queue_total {
        runtime.offline_queue_total = value;
    }
    if let Some(value) = config.offline_queue_bytes_total {
        runtime.offline_queue_bytes_total = value;
    }
    if let Some(value) = config.offline_queue_ttl_ms {
        runtime.offline_queue_ttl_ms = value;
    }
    if let Some(value) = config.session_ttl_ms {
        runtime.session_ttl_ms = value;
    }
    if let Some(value) = config.rate_limit_frames {
        runtime.rate_limit_frames = value;
    }
    if let Some(value) = config.max_frames_per_window {
        runtime.max_frames_per_window = value;
    }
    if let Some(value) = config.rate_limit_window_ms {
        runtime.rate_limit_window_ms = value;
    }
    if let Some(value) = config.source_bytes_per_minute {
        runtime.source_bytes_per_minute = value;
    }
    if let Some(value) = config.max_bytes_per_minute {
        runtime.max_bytes_per_minute = value;
    }
    if config.max_tracked_sources.is_none() {
        runtime.max_tracked_sources = runtime.max_tracked_sources.max(runtime.max_sessions);
    }
    if config.offline_queue_per_source.is_none() {
        runtime.offline_queue_per_source = runtime
            .offline_queue_per_source
            .min(runtime.offline_queue_total);
    }
    if config.offline_queue_bytes_per_source.is_none() {
        runtime.offline_queue_bytes_per_source = runtime
            .offline_queue_bytes_per_source
            .min(runtime.offline_queue_bytes_total);
    }
    if config.max_frames_per_window.is_none() {
        runtime.max_frames_per_window =
            runtime.max_frames_per_window.max(runtime.rate_limit_frames);
    }
    runtime
}

fn write_product_relay_report_v1(
    report_path: &Path,
    listen_addr: &str,
    runtime: &Runtime,
    manager: &ProductRelaySessionManagerV1,
    admission: &ProductRelayConnectionAdmissionV1,
    graceful_shutdown: bool,
) -> Result<()> {
    let relay_runtime = if graceful_shutdown {
        runtime.block_on(manager.finish_graceful_shutdown())
    } else {
        runtime.block_on(manager.snapshot())
    };
    let report = ProductRelayDaemonReportV1 {
        accepted: true,
        scope: "novovm_product_relay_daemon_v1".into(),
        daemon_version: PRODUCT_RELAY_DAEMON_VERSION_V2,
        listen_addr: listen_addr.to_string(),
        websocket_path: PRODUCT_RELAY_WEBSOCKET_PATH_V1.into(),
        transport: "wss".into(),
        report_updated_at_ms: now_ms_v1(),
        graceful_shutdown,
        tls_transport_enabled: true,
        ca_trust_required_for_novovm_identity: false,
        node_identity_challenge_response_required: true,
        payload_treated_opaque: true,
        relay_is_trusted_authority: false,
        business_semantics_interpreted_by_relay: false,
        novorudp_wire_changed: false,
        max_connection_count: admission.max_connections,
        active_connection_count: admission.active_connections.load(Ordering::Acquire),
        rejected_connection_total: admission.rejected_connections.load(Ordering::Acquire),
        relay_runtime,
    };
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create relay report directory: {}", parent.display()))?;
    }
    let temporary_path = report_path.with_extension("json.tmp");
    fs::write(&temporary_path, serde_json::to_vec_pretty(&report)?)
        .with_context(|| format!("write relay report: {}", temporary_path.display()))?;
    fs::rename(&temporary_path, report_path)
        .with_context(|| format!("persist relay report: {}", report_path.display()))?;
    Ok(())
}

fn build_server_tls_config_v1(cert_path: &Path, key_path: &Path) -> Result<rustls::ServerConfig> {
    let cert_bytes = fs::read(cert_path)
        .with_context(|| format!("read relay tls certificate: {}", cert_path.display()))?;
    let certificates = CertificateDer::pem_slice_iter(cert_bytes.as_slice())
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("parse relay tls certificates")?;
    if certificates.is_empty() {
        bail!("relay TLS certificate file contains no certificate");
    }
    let key_bytes = fs::read(key_path)
        .with_context(|| format!("read relay tls key: {}", key_path.display()))?;
    let private_key = load_private_key_v1(&key_bytes)?;
    rustls::ServerConfig::builder_with_provider(tls_crypto_provider_v1())
        .with_safe_default_protocol_versions()
        .context("select product relay TLS protocol versions")?
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
        .context("build product relay tls config")
}

fn tls_crypto_provider_v1() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

fn load_private_key_v1(bytes: &[u8]) -> Result<PrivateKeyDer<'static>> {
    PrivateKeyDer::from_pem_slice(bytes)
        .context("parse relay TLS key")
        .context("relay TLS key file contains no supported key")
}

fn load_ed25519_signing_key_v1(path: &Path) -> Result<SigningKey> {
    let encoded = fs::read_to_string(path)
        .with_context(|| format!("read relay identity key: {}", path.display()))?;
    let encoded = encoded.trim();
    if encoded.len() != 64 {
        bail!("relay identity key must be exactly 32 bytes encoded as 64 hexadecimal characters");
    }
    let mut secret = [0u8; 32];
    for (index, output) in secret.iter_mut().enumerate() {
        *output = u8::from_str_radix(&encoded[index * 2..index * 2 + 2], 16)
            .context("decode relay identity key hex")?;
    }
    Ok(SigningKey::from_bytes(&secret))
}

struct ProductRelayReadDeadlineV1<'a> {
    deadline: Instant,
    stopping: &'a AtomicBool,
    scope: &'static str,
    return_idle_timeout: bool,
    frame_started: Cell<bool>,
}

#[derive(Clone)]
struct ProductRelayDaemonIoDeadlineV1 {
    state: Arc<Mutex<ProductRelayDaemonIoDeadlineStateV1>>,
    stopping: Arc<AtomicBool>,
}

#[derive(Debug, Default)]
struct ProductRelayDaemonIoDeadlineStateV1 {
    deadline: Option<Instant>,
    lower_read_progressed: bool,
    buffered_read_deadline: Option<Instant>,
    terminal_error: Option<String>,
}

struct ProductRelayDaemonDeadlineTcpStreamV1 {
    inner: crate::product_relay_io::ProductRelaySocketV1,
    deadline: ProductRelayDaemonIoDeadlineV1,
    // Record boundaries start at the first handshake byte; transferring a
    // live TLS owner must not renew a half-record already in rustls buffers.
    inbound_tls_records: pump::InboundTlsRecords,
    #[cfg(test)]
    test_writes: ProductRelayDaemonTestWritesV1,
}

#[cfg(test)]
#[derive(Default)]
struct ProductRelayDaemonTestWritesV1 {
    fault: Option<ProductRelayDaemonTestWriteFaultV1>,
    read_fault: Option<ProductRelayDaemonTestReadFaultV1>,
    capture: bool,
    calls: usize,
    reads: usize,
    flushes: usize,
    attempted: Vec<Vec<u8>>,
    sent: Vec<Vec<u8>>,
}

#[cfg(test)]
#[derive(Clone, Copy)]
enum ProductRelayDaemonTestWriteFaultV1 {
    ZeroProgressTimeout,
    TimeoutAfterProgress(usize),
    ExpireAfterProgress(usize),
}

#[cfg(test)]
enum ProductRelayDaemonTestReadFaultV1 {
    ExpireAfterProgress,
    ConnectionReset,
}

impl ProductRelayDaemonIoDeadlineV1 {
    fn new(stopping: Arc<AtomicBool>) -> Self {
        Self {
            state: Arc::new(Mutex::new(ProductRelayDaemonIoDeadlineStateV1::default())),
            stopping,
        }
    }

    fn begin_v1(&self, deadline: Instant) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("product relay daemon I/O deadline lock poisoned"))?;
        self.check_state_v1(&mut state)?;
        state.deadline = Some(deadline);
        state.lower_read_progressed = false;
        self.check_state_v1(&mut state)
    }

    #[cfg(test)]
    fn begin_if_idle_v1(&self, deadline: Instant) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("product relay daemon I/O deadline lock poisoned"))?;
        self.check_state_v1(&mut state)?;
        if state.deadline.is_none() {
            state.deadline = Some(deadline);
            state.lower_read_progressed = false;
        }
        self.check_state_v1(&mut state)
    }

    #[cfg(test)]
    fn clear_v1(&self) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("product relay daemon I/O deadline lock poisoned"))?;
        self.check_state_v1(&mut state)?;
        // This boundary finishes a WebSocket frame (or an entirely idle
        // read). Pending ciphertext for the NEXT frame keeps its own oldest
        // timestamp, not the completed frame's now-obsolete deadline.
        state.deadline = state.buffered_read_deadline;
        state.lower_read_progressed = state.buffered_read_deadline.is_some();
        Ok(())
    }

    fn observe_read_ahead_v1(&self, started: Option<Instant>) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("product relay daemon I/O deadline lock poisoned"))?;
        self.check_state_v1(&mut state)?;
        state.buffered_read_deadline = None;
        if let Some(started) = started {
            let deadline = started
                .checked_add(Duration::from_millis(PRODUCT_RELAY_FRAME_DEADLINE_MS_V1))
                .ok_or_else(|| io::Error::other("relay read-ahead deadline overflow"))?;
            state.buffered_read_deadline = Some(deadline);
            state.deadline = Some(state.deadline.map_or(deadline, |old| old.min(deadline)));
            state.lower_read_progressed = true;
        }
        self.check_state_v1(&mut state)
    }

    #[cfg(test)]
    fn preserve_partial_read_deadline_v1(&self) -> io::Result<bool> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("product relay daemon I/O deadline lock poisoned"))?;
        self.check_state_v1(&mut state).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("product relay daemon partial lower-stream deadline: {error}"),
            )
        })?;
        Ok(state.lower_read_progressed)
    }

    fn record_lower_read_v1(&self, read: usize) -> io::Result<()> {
        if read > 0 {
            let mut state = self
                .state
                .lock()
                .map_err(|_| io::Error::other("product relay daemon I/O deadline lock poisoned"))?;
            state.lower_read_progressed = true;
        }
        self.check_v1()
    }

    fn check_v1(&self) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("product relay daemon I/O deadline lock poisoned"))?;
        self.check_state_v1(&mut state)
    }

    fn check_state_v1(&self, state: &mut ProductRelayDaemonIoDeadlineStateV1) -> io::Result<()> {
        if let Some(reason) = &state.terminal_error {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                format!("product relay daemon terminal lower-stream I/O: {reason}"),
            ));
        }
        if self.stopping.load(Ordering::Acquire) {
            state.terminal_error = Some("product relay daemon is stopping".into());
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "product relay daemon is stopping",
            ));
        }
        if state
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            state.terminal_error =
                Some("product relay daemon absolute lower-stream I/O deadline exceeded".into());
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "product relay daemon absolute lower-stream I/O deadline exceeded",
            ));
        }
        Ok(())
    }

    fn fail_v1(&self, error: impl std::fmt::Display) -> io::Error {
        let reason = error.to_string();
        if let Ok(mut state) = self.state.lock() {
            state.terminal_error.get_or_insert(reason.clone());
        }
        io::Error::new(
            io::ErrorKind::BrokenPipe,
            format!("product relay daemon terminal lower-stream I/O: {reason}"),
        )
    }

    fn bounded_timeout_v1(&self, configured: Option<Duration>) -> io::Result<Option<Duration>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("product relay daemon I/O deadline lock poisoned"))?;
        self.check_state_v1(&mut state)?;
        match state.deadline {
            Some(deadline) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    state.terminal_error = Some(
                        "product relay daemon absolute lower-stream I/O deadline exceeded".into(),
                    );
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "product relay daemon absolute lower-stream I/O deadline exceeded",
                    ));
                }
                Ok(Some(
                    configured.map_or(remaining, |budget| budget.min(remaining)),
                ))
            }
            None => Ok(configured),
        }
    }

    fn finish_progress_v1(
        &self,
        result: io::Result<usize>,
        maintenance: io::Result<()>,
    ) -> io::Result<usize> {
        match maintenance {
            Ok(()) => result,
            Err(error) => {
                let terminal = self.fail_v1(error);
                match result {
                    // Rustls must consume bytes already transferred even when
                    // the next operation is forbidden by a terminal error.
                    Ok(count) if count > 0 => Ok(count),
                    _ => Err(terminal),
                }
            }
        }
    }
}

impl Read for ProductRelayDaemonDeadlineTcpStreamV1 {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.deadline.check_v1()?;
        let read_started = self
            .inner
            .read_ahead_started_at()
            .unwrap_or_else(Instant::now);
        let original = self
            .inner
            .read_timeout()
            .map_err(|error| self.deadline.fail_v1(error))?;
        let bounded = self.deadline.bounded_timeout_v1(original)?;
        self.inner
            .set_read_timeout(bounded)
            .map_err(|error| self.deadline.fail_v1(error))?;
        #[cfg(test)]
        {
            self.test_writes.reads += 1;
        }
        #[cfg(not(test))]
        let result = self.inner.read(output);
        #[cfg(test)]
        let result = self.test_read_v1(output);
        let restored = self.inner.set_read_timeout(original).and_then(|()| {
            self.deadline
                .observe_read_ahead_v1(self.inner.read_ahead_started_at())
        });
        let checked = match &result {
            Ok(read) => self
                .inbound_tls_records
                .observe(&output[..*read], read_started)
                .and_then(|()| self.deadline.record_lower_read_v1(*read)),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                self.deadline.check_v1()
            }
            Err(error) => Err(self.deadline.fail_v1(error)),
        };
        self.deadline
            .finish_progress_v1(result, restored.and(checked))
    }
}

impl Write for ProductRelayDaemonDeadlineTcpStreamV1 {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        self.deadline.check_v1()?;
        let original = self
            .inner
            .write_timeout()
            .map_err(|error| self.deadline.fail_v1(error))?;
        let bounded = self.deadline.bounded_timeout_v1(original)?;
        self.inner
            .set_write_timeout(bounded)
            .map_err(|error| self.deadline.fail_v1(error))?;
        #[cfg(not(test))]
        let result = self.inner.write(input);
        #[cfg(test)]
        let result = self.test_write_v1(input);
        let restored = self.inner.set_write_timeout(original).and_then(|()| {
            self.deadline
                .observe_read_ahead_v1(self.inner.read_ahead_started_at())
        });
        let checked = match &result {
            Ok(_) => self.deadline.check_v1(),
            Err(error) => Err(self.deadline.fail_v1(error)),
        };
        self.deadline
            .finish_progress_v1(result, restored.and(checked))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.deadline.check_v1()?;
        #[cfg(test)]
        {
            self.test_writes.flushes += 1;
        }
        if let Err(error) = self.inner.flush() {
            return Err(self.deadline.fail_v1(error));
        }
        self.deadline.check_v1()
    }
}

fn accept_websocket_until_v1<S: Read + Write>(
    stream: &mut S,
    deadline: Instant,
    stopping: &AtomicBool,
) -> Result<()> {
    let guard = ProductRelayReadDeadlineV1 {
        deadline,
        stopping,
        scope: "handshake",
        return_idle_timeout: false,
        frame_started: Cell::new(false),
    };
    let request = read_http_headers_with_guard_v1(stream, Some(&guard))?;
    ensure_read_deadline_v1(Some(&guard))?;
    let key = validate_websocket_upgrade_request_v1(&request)?;
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let accept = BASE64_STANDARD.encode(hasher.finalize());
    write!(stream, "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\nSec-WebSocket-Protocol: {PRODUCT_RELAY_WEBSOCKET_SUBPROTOCOL_V2}\r\n\r\n")?;
    stream.flush()?;
    ensure_read_deadline_v1(Some(&guard))?;
    Ok(())
}

fn validate_websocket_upgrade_request_v1(request: &str) -> Result<String> {
    let mut lines = request.lines();
    let request_line = lines
        .next()
        .context("missing relay WebSocket request line")?;
    let mut parts = request_line.split_whitespace();
    if parts.next() != Some("GET")
        || parts.next() != Some(PRODUCT_RELAY_WEBSOCKET_PATH_V1)
        || parts.next() != Some("HTTP/1.1")
        || parts.next().is_some()
    {
        bail!("invalid relay WebSocket request line: {request_line}");
    }
    let mut host_present = false;
    let mut upgrade_websocket = false;
    let mut connection_upgrade = false;
    let mut version_13 = false;
    let mut key = None;
    let mut subprotocol = None;
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line
            .split_once(':')
            .with_context(|| format!("malformed relay WebSocket header: {line}"))?;
        let value = value.trim();
        if name.eq_ignore_ascii_case("host") {
            host_present |= !value.is_empty();
        } else if name.eq_ignore_ascii_case("upgrade") {
            upgrade_websocket |= value.eq_ignore_ascii_case("websocket");
        } else if name.eq_ignore_ascii_case("connection") {
            connection_upgrade |= value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("upgrade"));
        } else if name.eq_ignore_ascii_case("sec-websocket-version") {
            version_13 |= value == "13";
        } else if name.eq_ignore_ascii_case("sec-websocket-protocol") {
            if subprotocol.replace(value).is_some() {
                bail!("duplicate Sec-WebSocket-Protocol");
            }
        } else if name.eq_ignore_ascii_case("sec-websocket-key")
            && key.replace(value.to_string()).is_some()
        {
            bail!("duplicate Sec-WebSocket-Key");
        }
    }
    if !host_present || !upgrade_websocket || !connection_upgrade || !version_13 {
        bail!("relay WebSocket upgrade headers are incomplete or invalid");
    }
    // One codec for the complete manager lifetime, including offline queues.
    // Reject old/mixed carriers before identity registration can replace a
    // session or transfer any queued delivery to a different decoder.
    if subprotocol != Some(PRODUCT_RELAY_WEBSOCKET_SUBPROTOCOL_V2) {
        bail!("relay requires the binary v2 WebSocket subprotocol");
    }
    let key = key.context("missing Sec-WebSocket-Key")?;
    let decoded = BASE64_STANDARD
        .decode(key.as_bytes())
        .context("decode Sec-WebSocket-Key")?;
    if decoded.len() != 16 {
        bail!("Sec-WebSocket-Key must decode to exactly 16 bytes");
    }
    Ok(key)
}

#[cfg(test)]
fn read_http_headers_v1<S: Read>(stream: &mut S) -> Result<String> {
    read_http_headers_with_guard_v1(stream, None)
}

fn read_http_headers_with_guard_v1<S: Read>(
    stream: &mut S,
    guard: Option<&ProductRelayReadDeadlineV1<'_>>,
) -> Result<String> {
    let mut bytes = Vec::new();
    let mut one = [0u8; 1];
    while bytes.len() < 8192 {
        read_exact_with_guard_v1(stream, &mut one, guard)?;
        bytes.push(one[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            return String::from_utf8(bytes).context("relay WebSocket headers are not UTF-8");
        }
    }
    bail!("relay WebSocket headers exceed 8192 bytes")
}

fn write_wire_message_v1<S: Write>(
    stream: &mut S,
    message: &ProductRelayWireMessageV1,
) -> Result<()> {
    write_websocket_frame_v1(stream, 0x2, &encode_message_v2(message)?)
}

fn write_websocket_frame_v1<S: Write>(stream: &mut S, opcode: u8, payload: &[u8]) -> Result<()> {
    validate_websocket_payload_size_v1(
        opcode,
        payload.len(),
        PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1,
    )?;
    let mut header = [0u8; 10];
    header[0] = 0x80 | (opcode & 0x0f);
    let header_len = match payload.len() {
        len if len <= 125 => {
            header[1] = len as u8;
            2
        }
        len if len <= u16::MAX as usize => {
            header[1] = 126;
            header[2..4].copy_from_slice(&(len as u16).to_be_bytes());
            4
        }
        len => {
            header[1] = 127;
            header[2..10].copy_from_slice(&(len as u64).to_be_bytes());
            10
        }
    };
    stream.write_all(&header[..header_len])?;
    stream.write_all(payload)?;
    stream.flush()?;
    Ok(())
}

#[cfg(test)]
fn read_websocket_frame_v1<S: Read>(
    stream: &mut S,
    require_masked: bool,
) -> Result<WebSocketFrameV1> {
    read_websocket_frame_with_guard_v1(
        stream,
        require_masked,
        PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1,
        None,
    )
}

fn read_websocket_frame_until_v1<S: Read>(
    stream: &mut S,
    require_masked: bool,
    max_payload_bytes: usize,
    deadline: Instant,
    stopping: &AtomicBool,
) -> Result<WebSocketFrameV1> {
    let guard = ProductRelayReadDeadlineV1 {
        deadline,
        stopping,
        scope: "handshake",
        return_idle_timeout: false,
        frame_started: Cell::new(false),
    };
    read_websocket_frame_with_guard_v1(stream, require_masked, max_payload_bytes, Some(&guard))
}

#[cfg(test)]
fn read_authenticated_websocket_frame_until_v1<S: Read>(
    stream: &mut S,
    require_masked: bool,
    max_payload_bytes: usize,
    deadline: Instant,
    stopping: &AtomicBool,
) -> Result<WebSocketFrameV1> {
    let guard = ProductRelayReadDeadlineV1 {
        deadline,
        stopping,
        scope: "frame",
        return_idle_timeout: true,
        frame_started: Cell::new(false),
    };
    read_websocket_frame_with_guard_v1(stream, require_masked, max_payload_bytes, Some(&guard))
}

fn read_websocket_frame_with_guard_v1<S: Read>(
    stream: &mut S,
    require_masked: bool,
    max_payload_bytes: usize,
    guard: Option<&ProductRelayReadDeadlineV1<'_>>,
) -> Result<WebSocketFrameV1> {
    let mut header = [0u8; 2];
    read_exact_with_guard_v1(stream, &mut header, guard)?;
    if header[0] & 0x80 == 0 {
        bail!("fragmented WebSocket frames are not supported");
    }
    if header[0] & 0x70 != 0 {
        bail!("relay WebSocket RSV bits are unsupported");
    }
    let opcode = header[0] & 0x0f;
    let masked = header[1] & 0x80 != 0;
    if require_masked && !masked {
        bail!("relay requires masked client WebSocket frames");
    }
    let mut len = (header[1] & 0x7f) as u64;
    if len == 126 {
        let mut extended = [0u8; 2];
        read_exact_with_guard_v1(stream, &mut extended, guard)?;
        len = u16::from_be_bytes(extended) as u64;
    } else if len == 127 {
        let mut extended = [0u8; 8];
        read_exact_with_guard_v1(stream, &mut extended, guard)?;
        len = u64::from_be_bytes(extended);
    }
    if len > max_payload_bytes as u64 {
        bail!("relay WebSocket frame exceeds maximum size");
    }
    validate_websocket_payload_size_v1(opcode, len as usize, max_payload_bytes)?;
    let mask = if masked {
        let mut mask = [0u8; 4];
        read_exact_with_guard_v1(stream, &mut mask, guard)?;
        Some(mask)
    } else {
        None
    };
    let mut payload = vec![0u8; len as usize];
    read_exact_with_guard_v1(stream, &mut payload, guard)?;
    if let Some(mask) = mask {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }
    }
    match opcode {
        0x2 => Ok(WebSocketFrameV1::Binary(payload)),
        0x9 => Ok(WebSocketFrameV1::Ping(payload)),
        0xA => Ok(WebSocketFrameV1::Pong(payload)),
        0x8 => Ok(WebSocketFrameV1::Close),
        _ => bail!("unsupported relay WebSocket opcode: {opcode}"),
    }
}

fn validate_websocket_payload_size_v1(
    opcode: u8,
    payload_len: usize,
    max_payload_bytes: usize,
) -> Result<()> {
    if payload_len > max_payload_bytes {
        bail!("relay WebSocket frame exceeds maximum size");
    }
    if opcode & 0x08 != 0 && payload_len > MAX_WEBSOCKET_CONTROL_FRAME_BYTES_V1 {
        bail!("relay WebSocket control frame exceeds 125 bytes");
    }
    Ok(())
}

fn ensure_read_deadline_v1(guard: Option<&ProductRelayReadDeadlineV1<'_>>) -> Result<()> {
    if let Some(guard) = guard {
        if guard.stopping.load(Ordering::Acquire) {
            bail!("product relay daemon is stopping");
        }
        if Instant::now() >= guard.deadline {
            bail!("product relay absolute {} deadline exceeded", guard.scope);
        }
    }
    Ok(())
}

fn read_exact_with_guard_v1<S: Read>(
    stream: &mut S,
    bytes: &mut [u8],
    guard: Option<&ProductRelayReadDeadlineV1<'_>>,
) -> Result<()> {
    let mut offset = 0usize;
    while offset < bytes.len() {
        ensure_read_deadline_v1(guard)?;
        match stream.read(&mut bytes[offset..]) {
            Ok(0) => bail!("relay WebSocket stream closed during read"),
            Ok(read) => {
                offset = offset.saturating_add(read);
                if let Some(guard) = guard {
                    guard.frame_started.set(true);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) && guard.is_none()
                    && offset > 0 =>
            {
                bail!("partial relay WebSocket frame read timed out")
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) && guard.is_some_and(|guard| {
                    !guard.return_idle_timeout || guard.frame_started.get()
                }) =>
            {
                continue;
            }
            Err(error) => return Err(error.into()),
        }
    }
    ensure_read_deadline_v1(guard)
}

#[cfg(test)]
fn is_timeout_v1(error: &anyhow::Error) -> bool {
    error.downcast_ref::<io::Error>().is_some_and(|io| {
        matches!(
            io.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        )
    })
}

fn now_ms_v1() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn default_report_interval_ms_v1() -> u64 {
    5_000
}

#[cfg(test)]
pub(crate) fn product_relay_test_fixture_dir_v1(scope: &str) -> PathBuf {
    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
    assert!(scope
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'));
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let mut artifacts = repository.clone();
    for component in ["target", "runtime-rebuild", "network-relay"] {
        artifacts.push(component);
        if !artifacts.try_exists().unwrap() {
            match fs::create_dir(&artifacts) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create relay fixture parent: {error}"),
            }
        }
        artifacts = artifacts.canonicalize().unwrap();
        assert!(artifacts.starts_with(&repository));
    }
    let root = artifacts.join(format!(
        "{scope}-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir(&root).unwrap();
    // Retain bounded test artifacts for diagnostics; never recursively remove a
    // computed pathname from a migrated network fixture.
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        peer_id_from_ed25519_public_key_v1, AuthenticatedPeerV1, E2eSecureChannelV1,
        HandshakeReplayCacheV1, NodeHandshakeInitiatorV1, NodeHandshakeResponderV1,
        NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0, RelayPeerHandshakeV1,
    };
    use rustls::pki_types::{CertificateDer, ServerName};
    use std::{cell::Cell, collections::VecDeque, io::Cursor, net::SocketAddr, time::Instant};

    type TestClientWebSocketV1 = rustls::StreamOwned<rustls::ClientConnection, TcpStream>;

    include!("product_relay_daemon_io_tests.rs");
    include!("product_relay_daemon_encoded_tests.rs");
    include!("product_relay_daemon_duplex_tests.rs");

    struct TestControlledRelayDaemonV1 {
        report_path: PathBuf,
        stopping: Arc<AtomicBool>,
        worker: Option<thread::JoinHandle<Result<()>>>,
        spawned_at: Instant,
    }

    impl TestControlledRelayDaemonV1 {
        fn start(run_for_ms: Option<u64>) -> Self {
            let root = product_relay_test_fixture_dir_v1("controlled-daemon");
            let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
            let certificate_path = root.join("relay-cert.pem");
            let key_path = root.join("relay-key.pem");
            let identity_path = root.join("relay-identity.hex");
            let report_path = root.join("reports/relay.json");
            fs::write(&certificate_path, certificate.serialize_pem().unwrap()).unwrap();
            fs::write(&key_path, certificate.serialize_private_key_pem()).unwrap();
            fs::write(&identity_path, hex_encode_v1(&[71; 32])).unwrap();
            let config: ProductRelayDaemonConfigV1 = serde_json::from_value(serde_json::json!({
                "bind_addr": "127.0.0.1:0",
                "tls_cert_path": certificate_path,
                "tls_key_path": key_path,
                "relay_identity_key_path": identity_path,
                "report_path": report_path,
                "report_interval_ms": 10,
                "run_for_ms": run_for_ms,
                "max_connections": 4,
                "max_sessions": 2,
                // Stopping an idle handshake must not wait for this deadline.
                "handshake_timeout_ms": 60_000,
            }))
            .unwrap();
            let stopping = Arc::new(AtomicBool::new(false));
            let worker_stopping = Arc::clone(&stopping);
            let spawned_at = Instant::now();
            let worker = thread::spawn(move || {
                run_product_relay_daemon_with_shutdown_v1(config, worker_stopping)
            });
            Self {
                report_path,
                stopping,
                worker: Some(worker),
                spawned_at,
            }
        }

        fn report(&self) -> Option<ProductRelayDaemonReportV1> {
            let bytes = fs::read(&self.report_path).ok()?;
            serde_json::from_slice(&bytes).ok()
        }

        fn wait_for_report(
            &self,
            predicate: impl Fn(&ProductRelayDaemonReportV1) -> bool,
        ) -> ProductRelayDaemonReportV1 {
            let started_at = Instant::now();
            loop {
                if let Some(report) = self.report().filter(|report| predicate(report)) {
                    return report;
                }
                assert!(
                    started_at.elapsed() < Duration::from_secs(5),
                    "controlled relay report predicate exceeded its deadline",
                );
                thread::sleep(Duration::from_millis(5));
            }
        }

        fn join_within(&mut self, budget: Duration) -> Result<()> {
            let started_at = Instant::now();
            while !self.worker.as_ref().unwrap().is_finished() {
                assert!(
                    started_at.elapsed() < budget,
                    "controlled relay shutdown exceeded its deadline",
                );
                thread::sleep(Duration::from_millis(5));
            }
            self.worker
                .take()
                .unwrap()
                .join()
                .expect("controlled relay thread panicked")
        }

        fn stop_and_join(&mut self) -> Result<()> {
            self.stopping.store(true, Ordering::Release);
            self.join_within(Duration::from_secs(5))
        }
    }

    impl Drop for TestControlledRelayDaemonV1 {
        fn drop(&mut self) {
            // Also own cleanup on a failed assertion. Dropping a JoinHandle
            // alone would detach an otherwise unbounded daemon.
            self.stopping.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    #[test]
    fn daemon_external_shutdown_without_duration_or_clients_is_joined() {
        let mut daemon = TestControlledRelayDaemonV1::start(None);
        let running = daemon.wait_for_report(|report| !report.graceful_shutdown);
        assert_eq!(running.active_connection_count, 0);
        assert!(!daemon.stopping.load(Ordering::Acquire));
        daemon.stop_and_join().unwrap();
        let stopped = daemon.report().unwrap();
        assert!(stopped.graceful_shutdown);
        assert_eq!(stopped.active_connection_count, 0);
        assert!(daemon.stopping.load(Ordering::Acquire));
    }

    #[test]
    fn daemon_external_shutdown_interrupts_idle_handshake_and_joins_worker() {
        let mut daemon = TestControlledRelayDaemonV1::start(None);
        let running = daemon.wait_for_report(|report| !report.graceful_shutdown);
        let idle_connection = TcpStream::connect(&running.listen_addr).unwrap();
        daemon.wait_for_report(|report| report.active_connection_count == 1);
        daemon.stop_and_join().unwrap();
        let stopped = daemon.report().unwrap();
        assert!(stopped.graceful_shutdown);
        assert_eq!(stopped.active_connection_count, 0);
        drop(idle_connection);
    }

    #[test]
    fn daemon_configured_duration_still_stops_without_external_request() {
        let duration = Duration::from_millis(150);
        let mut daemon = TestControlledRelayDaemonV1::start(Some(duration.as_millis() as u64));
        daemon.join_within(Duration::from_secs(5)).unwrap();
        assert!(daemon.spawned_at.elapsed() >= duration);
        assert!(daemon.stopping.load(Ordering::Acquire));
        let stopped = daemon.report().unwrap();
        assert!(stopped.graceful_shutdown);
        assert_eq!(stopped.active_connection_count, 0);
    }

    #[test]
    fn daemon_report_failure_stops_and_joins_existing_idle_connection() {
        let mut daemon = TestControlledRelayDaemonV1::start(None);
        let running = daemon.wait_for_report(|report| !report.graceful_shutdown);
        let mut idle_connection = TcpStream::connect(&running.listen_addr).unwrap();
        daemon.wait_for_report(|report| report.active_connection_count == 1);
        assert!(!daemon.stopping.load(Ordering::Acquire));

        // Atomically occupy the report's temporary pathname with a directory.
        // Retry only if a report write currently owns the path; once successful,
        // the next report must fail even though connection workers are active.
        let temporary_report = daemon.report_path.with_extension("json.tmp");
        let started_at = Instant::now();
        loop {
            match fs::create_dir(&temporary_report) {
                Ok(()) => break,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    assert!(started_at.elapsed() < Duration::from_secs(5));
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("install controlled relay report fault: {error}"),
            }
        }
        let error = daemon.join_within(Duration::from_secs(5)).unwrap_err();
        assert!(error.to_string().contains("write relay report"));
        assert!(daemon.stopping.load(Ordering::Acquire));

        // Even with the client still open and the 60-second handshake deadline
        // outstanding, the connection worker must have closed its socket.
        idle_connection
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut byte = [0u8; 1];
        match idle_connection.read(&mut byte) {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
                ) => {}
            other => panic!("relay error left its idle connection alive: {other:?}"),
        }
    }

    struct ScriptedWebSocketV1 {
        reads: Cursor<Vec<u8>>,
        writes: Vec<u8>,
    }

    impl Read for ScriptedWebSocketV1 {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.reads.read(bytes)
        }
    }

    impl Write for ScriptedWebSocketV1 {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn websocket_binary_and_masking_round_trip() {
        let mut wire = Vec::new();
        write_websocket_frame_v1(&mut wire, 0x2, b"opaque").unwrap();
        assert!(
            matches!(read_websocket_frame_v1(&mut wire.as_slice(), false).unwrap(), WebSocketFrameV1::Binary(bytes) if bytes == b"opaque")
        );
    }

    #[test]
    fn daemon_websocket_length_boundaries_preserve_payload_and_next_frame() {
        for length in [125usize, 126, 127, 128, 65_535, 65_536] {
            for masked in [false, true] {
                let payload = vec![0x5a; length];
                let mut encoded = Vec::new();
                write_websocket_frame_v1(&mut encoded, 0x2, &payload).unwrap();
                if masked {
                    let header_len = match encoded[1] {
                        126 => 4,
                        127 => 10,
                        _ => 2,
                    };
                    let mask = [0x13, 0x37, 0x39, 0x41];
                    encoded[1] |= 0x80;
                    encoded.truncate(header_len);
                    encoded.extend_from_slice(&mask);
                    encoded.extend(
                        payload
                            .iter()
                            .enumerate()
                            .map(|(index, byte)| byte ^ mask[index % mask.len()]),
                    );
                }
                encoded.extend_from_slice(&[0x82, 1, 0x42]);
                let mut stream = encoded.as_slice();
                assert!(
                    matches!(
                        read_websocket_frame_v1(&mut stream, masked).unwrap(),
                        WebSocketFrameV1::Binary(decoded) if decoded == payload
                    ),
                    "daemon decoder length {length}, masked {masked}"
                );
                assert_eq!(stream, &[0x82, 1, 0x42]);
            }
        }
    }

    #[test]
    fn websocket_upgrade_requires_rfc6455_headers_and_fresh_key_shape() {
        let valid = "GET /novovm HTTP/1.1\r\nHost: relay.example\r\nUpgrade: websocket\r\nConnection: keep-alive, Upgrade\r\nSec-WebSocket-Key: AAECAwQFBgcICQoLDA0ODw==\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: novovm.relay.binary.v2\r\n\r\n";
        assert_eq!(
            validate_websocket_upgrade_request_v1(valid).unwrap(),
            "AAECAwQFBgcICQoLDA0ODw=="
        );
        for replacement in [
            "",
            "Sec-WebSocket-Protocol: novovm.relay.binary.v1\r\n",
            "Sec-WebSocket-Protocol: novovm.relay.binary.v2, legacy\r\n",
            "Sec-WebSocket-Protocol: novovm.relay.binary.v2\r\nSec-WebSocket-Protocol: novovm.relay.binary.v2\r\n",
        ] {
            assert!(validate_websocket_upgrade_request_v1(&valid.replace(
                "Sec-WebSocket-Protocol: novovm.relay.binary.v2\r\n", replacement
            )).is_err());
        }
        assert!(validate_websocket_upgrade_request_v1(
            &valid.replace("Upgrade: websocket\r\n", "")
        )
        .unwrap_err()
        .to_string()
        .contains("incomplete or invalid"));
        assert!(validate_websocket_upgrade_request_v1(
            &valid.replace("Sec-WebSocket-Version: 13", "Sec-WebSocket-Version: 12")
        )
        .is_err());
        assert!(validate_websocket_upgrade_request_v1(
            &valid.replace("AAECAwQFBgcICQoLDA0ODw==", "AAECAw==")
        )
        .unwrap_err()
        .to_string()
        .contains("exactly 16 bytes"));
    }

    #[test]
    fn carrier_negotiation_rejects_legacy_before_reading_identity_or_writing_success() {
        let base = "GET /novovm HTTP/1.1\r\nHost: relay.example\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: AAECAwQFBgcICQoLDA0ODw==\r\nSec-WebSocket-Version: 13\r\n";
        for protocol in [
            "",
            "Sec-WebSocket-Protocol: legacy\r\n",
            "Sec-WebSocket-Protocol: novovm.relay.binary.v2, legacy\r\n",
            "Sec-WebSocket-Protocol: novovm.relay.binary.v2\r\nSec-WebSocket-Protocol: novovm.relay.binary.v2\r\n",
        ] {
            let request = format!("{base}{protocol}\r\n");
            let header_length = request.len();
            let mut bytes = request.into_bytes();
            bytes.extend_from_slice(b"identity-offer-must-not-be-consumed");
            let mut stream = ScriptedWebSocketV1 { reads: Cursor::new(bytes), writes: Vec::new() };
            let result = accept_websocket_until_v1(&mut stream, Instant::now() + Duration::from_secs(1), &AtomicBool::new(false));
            assert!(result.is_err());
            assert_eq!(stream.reads.position() as usize, header_length);
            assert!(stream.writes.is_empty());
        }
        let mut stream = ScriptedWebSocketV1 {
            reads: Cursor::new(format!("{base}Sec-WebSocket-Protocol: {PRODUCT_RELAY_WEBSOCKET_SUBPROTOCOL_V2}\r\n\r\n").into_bytes()),
            writes: Vec::new(),
        };
        accept_websocket_until_v1(
            &mut stream,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false),
        )
        .unwrap();
        let response = String::from_utf8(stream.writes).unwrap();
        assert_eq!(response.matches("Sec-WebSocket-Protocol:").count(), 1);
        assert!(response.contains(&format!(
            "Sec-WebSocket-Protocol: {PRODUCT_RELAY_WEBSOCKET_SUBPROTOCOL_V2}\r\n"
        )));
    }

    #[test]
    fn websocket_write_bounds_data_and_control_before_io() {
        let mut wire = Vec::new();
        let oversized = vec![0u8; PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 + 1];
        assert!(write_websocket_frame_v1(&mut wire, 0x2, &oversized)
            .unwrap_err()
            .to_string()
            .contains("maximum size"));
        assert!(write_websocket_frame_v1(
            &mut wire,
            0x9,
            &[0u8; MAX_WEBSOCKET_CONTROL_FRAME_BYTES_V1 + 1],
        )
        .unwrap_err()
        .to_string()
        .contains("control frame"));
        write_websocket_frame_v1(&mut wire, 0xA, &[0u8; MAX_WEBSOCKET_CONTROL_FRAME_BYTES_V1])
            .unwrap();
    }

    #[test]
    fn daemon_lower_stream_deadline_is_not_reset_by_tls_record_progress() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            for byte in 0u8..8 {
                if socket.write_all(&[byte]).is_err() {
                    break;
                }
                let _ = socket.flush();
                thread::sleep(Duration::from_millis(15));
            }
        });
        let tcp = TcpStream::connect(address).unwrap();
        tcp.set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let deadline = ProductRelayDaemonIoDeadlineV1::new(Arc::new(AtomicBool::new(false)));
        deadline
            .begin_v1(Instant::now() + Duration::from_millis(35))
            .unwrap();
        let mut guarded = ProductRelayDaemonDeadlineTcpStreamV1 {
            inner: crate::product_relay_io::ProductRelaySocketV1::new(tcp).unwrap(),
            deadline,
            inbound_tls_records: super::pump::InboundTlsRecords::default(),
            test_writes: ProductRelayDaemonTestWritesV1::default(),
        };
        let mut bytes = [0u8; 8];
        let error = guarded.read_exact(&mut bytes).unwrap_err();
        assert!(
            error.kind() == io::ErrorKind::TimedOut
                || (error.kind() == io::ErrorKind::BrokenPipe
                    && error.to_string().contains("terminal lower-stream"))
        );
        assert!(error.to_string().contains("absolute lower-stream"));
        drop(guarded);
        server.join().unwrap();
    }

    #[test]
    fn expired_partial_lower_stream_deadline_closes_instead_of_hot_looping() {
        let deadline = ProductRelayDaemonIoDeadlineV1::new(Arc::new(AtomicBool::new(false)));
        {
            let mut state = deadline.state.lock().unwrap();
            state.deadline = Some(Instant::now() - Duration::from_millis(1));
            state.lower_read_progressed = true;
        }
        let error = deadline.preserve_partial_read_deadline_v1().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(error.to_string().contains("partial lower-stream"));
    }

    #[test]
    fn idle_connection_tick_bounds_inbox_delivery_after_request_timeout() {
        let mut data = (0..100).collect::<VecDeque<_>>();
        let mut control = (0..100).collect::<VecDeque<_>>();
        let mut delivered_data = Vec::new();
        let mut delivered_control = Vec::new();

        let data_count = drain_bounded_relay_inbox_v1(
            MAX_DATA_DELIVERIES_PER_CONNECTION_TICK_V1,
            || false,
            || data.pop_front(),
            |item| {
                delivered_data.push(item);
                Ok(())
            },
        )
        .unwrap();
        let control_count = drain_bounded_relay_inbox_v1(
            MAX_PEER_HANDSHAKE_DELIVERIES_PER_CONNECTION_TICK_V1,
            || false,
            || control.pop_front(),
            |item| {
                delivered_control.push(item);
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(data_count, MAX_DATA_DELIVERIES_PER_CONNECTION_TICK_V1);
        assert_eq!(
            control_count,
            MAX_PEER_HANDSHAKE_DELIVERIES_PER_CONNECTION_TICK_V1
        );
        assert_eq!(delivered_data.len(), data_count);
        assert_eq!(delivered_control.len(), control_count);
        assert_eq!(data.len(), 100 - data_count);
        assert_eq!(control.len(), 100 - control_count);

        let stop = Cell::new(false);
        let mut shutdown_queue = VecDeque::from([1, 2, 3]);
        let shutdown_count = drain_bounded_relay_inbox_v1(
            MAX_DATA_DELIVERIES_PER_CONNECTION_TICK_V1,
            || stop.get(),
            || shutdown_queue.pop_front(),
            |_| {
                stop.set(true);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(shutdown_count, 1);
        assert_eq!(shutdown_queue, VecDeque::from([2, 3]));
    }

    #[test]
    fn completed_frame_can_advance_to_the_remaining_prefetch_timestamp() {
        let now = Instant::now();
        let deadline = ProductRelayDaemonIoDeadlineV1::new(Arc::new(AtomicBool::new(false)));
        deadline
            .observe_read_ahead_v1(Some(now - Duration::from_secs(8)))
            .unwrap();
        deadline
            .observe_read_ahead_v1(Some(now - Duration::from_secs(1)))
            .unwrap();
        assert_eq!(
            deadline.state.lock().unwrap().deadline,
            Some(now + Duration::from_secs(2))
        );
        // Advancing the buffered-byte timestamp cannot extend an unfinished
        // frame. Only finishing that frame permits the next frame's budget.
        deadline.clear_v1().unwrap();
        assert_eq!(
            deadline.state.lock().unwrap().deadline,
            Some(now + Duration::from_secs(9))
        );
    }

    #[test]
    fn prefetched_tls_bytes_keep_their_original_deadline_across_outer_clear() {
        let now = Instant::now();
        let deadline = ProductRelayDaemonIoDeadlineV1::new(Arc::new(AtomicBool::new(false)));
        deadline.begin_v1(now + Duration::from_secs(10)).unwrap();
        deadline
            .observe_read_ahead_v1(Some(now - Duration::from_secs(8)))
            .unwrap();
        let original = deadline.state.lock().unwrap().deadline;
        assert_eq!(original, Some(now + Duration::from_secs(2)));
        deadline.clear_v1().unwrap();
        deadline
            .begin_if_idle_v1(now + Duration::from_secs(10))
            .unwrap();
        deadline.observe_read_ahead_v1(Some(now)).unwrap();
        assert_eq!(deadline.state.lock().unwrap().deadline, original);
        // Consuming the raw buffer does not end a partially decoded TLS/WS
        // frame; its lower-progress deadline is retained until frame finish.
        deadline.observe_read_ahead_v1(None).unwrap();
        assert!(deadline.preserve_partial_read_deadline_v1().unwrap());
        assert_eq!(deadline.state.lock().unwrap().deadline, original);
        deadline.clear_v1().unwrap();
        assert_eq!(deadline.state.lock().unwrap().deadline, None);
        assert!(deadline
            .observe_read_ahead_v1(Some(now - Duration::from_secs(11)))
            .is_err());
        assert!(deadline.clear_v1().is_err());
    }

    #[test]
    fn delivery_window_is_cumulative_not_refilled_by_idle_or_duplicate_credit() {
        let mut window = RelayDeliveryWindowV1::default();
        assert_eq!(window.available(), 15);
        for _ in 0..15 {
            window.sent().unwrap();
        }
        assert_eq!(window.available(), 0);
        assert!(window.sent().is_err());
        window.acknowledge(0).unwrap();
        assert_eq!(window.available(), 0);
        window.acknowledge(7).unwrap();
        assert_eq!(window.available(), 7);
        for _ in 0..7 {
            window.sent().unwrap();
        }
        window.acknowledge(7).unwrap();
        assert_eq!(window.available(), 0);
        assert!(window.acknowledge(6).is_err());
        assert!(window.acknowledge(23).is_err());
        assert_eq!(window.available(), 0);
        let mut overflow = RelayDeliveryWindowV1 {
            sent: u64::MAX,
            consumed: u64::MAX,
        };
        assert!(overflow.sent().is_err());
        assert_eq!(overflow.sent, u64::MAX);
    }

    #[test]
    #[ignore = "real read-wake deadline regression; run release with --include-ignored --test-threads=1"]
    fn real_tcp_inbox_notification_interrupts_five_second_idle_read() {
        // This isolates queue -> authenticated connection scheduling over REAL
        // TCP/Mio, not TLS performance. The manager identity handshake and
        // opaque payload use real signatures/E2E. The disconnected-waker
        // control must time out at the reader before the unchanged 5s idle.
        struct ReadStartedSocket {
            inner: crate::product_relay_io::ProductRelaySocketV1,
            started: Option<std::sync::mpsc::Sender<()>>,
        }
        impl Read for ReadStartedSocket {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                if let Some(started) = self.started.take() {
                    let _ = started.send(());
                }
                self.inner.read(out)
            }
        }
        impl Write for ReadStartedSocket {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.inner.write(bytes)
            }
            fn flush(&mut self) -> io::Result<()> {
                self.inner.flush()
            }
        }

        for connect_waker in [false, true] {
            let runtime = TokioRuntimeBuilder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let manager =
                ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1::default()).unwrap();
            let relay = SigningKey::from_bytes(&[164; 32]);
            let source = SigningKey::from_bytes(&[165; 32]);
            let target = SigningKey::from_bytes(&[166; 32]);
            let now = now_ms_v1();
            let (source_registration, _source_inbox) = runtime
                .block_on(manager.register_authenticated_session(
                    authenticate_test_peer_v1(&source, &relay, now),
                    now,
                ))
                .unwrap();
            let (target_registration, mut inbox) = runtime
                .block_on(manager.register_authenticated_session(
                    authenticate_test_peer_v1(&target, &relay, now),
                    now,
                ))
                .unwrap();
            let (mut sender, mut receiver) = test_peer_channels_v1(&source, &target, now);
            let expected = test_data_frame_v1(0x2345);
            let envelope = sender.seal_novorudp_frame(&expected).unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (server, _) = listener.accept().unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut server = crate::product_relay_io::ProductRelaySocketV1::new(server).unwrap();
            server
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            server
                .set_write_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let waker = server.read_waker();
            let (started, read_started) = std::sync::mpsc::channel();
            let stopping = Arc::new(AtomicBool::new(false));
            let worker = thread::spawn({
                let manager = manager.clone();
                let stopping = Arc::clone(&stopping);
                move || {
                    let runtime = TokioRuntimeBuilder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    let mut socket = ReadStartedSocket {
                        inner: server,
                        started: Some(started),
                    };
                    relay_connection_loop_v1(
                        &mut socket,
                        ProductRelayConnectionLoopV1 {
                            manager: &manager,
                            runtime: &runtime,
                            peer_id: &target_registration.peer_id,
                            session_id: target_registration.session_id,
                            inbox: &mut inbox,
                            stopping: &stopping,
                            io_deadline: None,
                            read_waker: connect_waker.then_some(&waker),
                        },
                    )
                }
            });
            // No assertion may detach the worker: even a timeout or setup
            // error below shuts down both TCP directions and joins first.
            let observation = (|| -> Result<(crate::RelayForwardOutcomeV1, Result<WebSocketFrameV1>, Duration)> {
                read_started.recv_timeout(Duration::from_secs(5)).context("relay did not enter its socket read")?;
                let admitted = runtime.block_on(manager.forward_opaque(
                    &source_registration.peer_id,
                    source_registration.session_id,
                    envelope,
                    now_ms_v1(),
                ));
                let start = Instant::now();
                let received = read_websocket_frame_v1(&mut client, false);
                Ok((admitted, received, start.elapsed()))
            })();
            stopping.store(true, Ordering::Release);
            let _ = client.shutdown(Shutdown::Both);
            drop(client);
            let _terminated = worker.join().expect("relay connection worker panicked");
            let (admitted, received, elapsed) = observation.unwrap();
            assert!(admitted.forwarded && !admitted.queued);
            if connect_waker {
                assert!(
                    elapsed < Duration::from_secs(1),
                    "queue wake waited for idle timeout: {elapsed:?}"
                );
                let WebSocketFrameV1::Binary(bytes) = received.unwrap() else {
                    panic!("queue wake did not produce a binary delivery");
                };
                let ProductRelayWireMessageV1::Delivery(delivery) =
                    decode_message_v2(&bytes).unwrap()
                else {
                    panic!("queue wake produced a non-delivery message");
                };
                assert_eq!(
                    receiver.open_novorudp_frame(&delivery.envelope).unwrap(),
                    expected
                );
            } else {
                let error = match received {
                    Err(error) => error,
                    Ok(_) => panic!("unwired control unexpectedly bypassed the 5s read"),
                };
                assert!(
                    is_timeout_v1(&error),
                    "unwired control failed for a non-idle reason: {error:#}"
                );
            }
        }
    }

    #[test]
    fn delayed_request_cannot_receive_unbounded_egress_before_its_outcome() {
        // The read script models an arbitrarily delayed request while the
        // authenticated target already has >16 MiB of REAL opaque ciphertext.
        // Many idle/wake turns must not each grant a fresh delivery window.
        struct DelayedRequest {
            chunks: VecDeque<Option<Vec<u8>>>,
            current: Cursor<Vec<u8>>,
            writes: Vec<u8>,
        }
        impl Read for DelayedRequest {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                if self.current.position() == self.current.get_ref().len() as u64 {
                    match self.chunks.pop_front() {
                        Some(Some(bytes)) => self.current = Cursor::new(bytes),
                        Some(None) => return Err(io::ErrorKind::WouldBlock.into()),
                        None => return Ok(0),
                    }
                }
                self.current.read(out)
            }
        }
        impl Write for DelayedRequest {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.writes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let runtime = TokioRuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1 {
            session_queue_bytes: 64 * 1024 * 1024,
            ..ProductRelayRuntimeConfigV1::default()
        })
        .unwrap();
        let relay = SigningKey::from_bytes(&[161; 32]);
        let a = SigningKey::from_bytes(&[162; 32]);
        let b = SigningKey::from_bytes(&[163; 32]);
        let now = now_ms_v1();
        let (ra, mut ia) =
            runtime
                .block_on(manager.register_authenticated_session(
                    authenticate_test_peer_v1(&a, &relay, now),
                    now,
                ))
                .unwrap();
        let (rb, _ib) =
            runtime
                .block_on(manager.register_authenticated_session(
                    authenticate_test_peer_v1(&b, &relay, now),
                    now,
                ))
                .unwrap();
        let (mut ca, mut cb) = test_peer_channels_v1(&a, &b, now);
        // Compact ciphertext no longer gains ~3.6x JSON array expansion.
        // Keep the original >16 MiB pressure premise with real input bytes.
        const QUEUED_DELIVERIES: usize = 90;
        for seq in 0..QUEUED_DELIVERIES {
            let mut frame = test_data_frame_v1(seq as u64);
            frame.payload = vec![seq as u8; 200_000];
            let envelope = cb.seal_novorudp_frame(&frame).unwrap();
            assert!(
                runtime
                    .block_on(manager.forward_opaque(&rb.peer_id, rb.session_id, envelope, now))
                    .forwarded
            );
        }
        assert!(
            runtime.block_on(manager.snapshot()).active_queued_bytes
                > PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_BYTES_V1
        );
        let wire = |message: ProductRelayWireMessageV1| {
            let mut bytes = Vec::new();
            write_masked_wire_message_v1(&mut bytes, &message).unwrap();
            Some(bytes)
        };
        let mut chunks = VecDeque::from([None, None, None, None]);
        chunks.push_back(wire(ProductRelayWireMessageV1::Data(
            ca.seal_novorudp_frame(&test_data_frame_v1(100)).unwrap(),
        )));
        chunks.push_back(wire(ProductRelayWireMessageV1::DeliveryConsumedV1 {
            through: 7,
        }));
        chunks.extend([None, None, None]);
        chunks.push_back(wire(ProductRelayWireMessageV1::DeliveryConsumedV1 {
            through: 7,
        }));
        chunks.extend([None, None]);
        chunks.push_back(wire(ProductRelayWireMessageV1::Close));
        let mut socket = DelayedRequest {
            chunks,
            current: Cursor::new(Vec::new()),
            writes: Vec::new(),
        };
        relay_connection_loop_v1(
            &mut socket,
            ProductRelayConnectionLoopV1 {
                manager: &manager,
                runtime: &runtime,
                peer_id: &ra.peer_id,
                session_id: ra.session_id,
                inbox: &mut ia,
                stopping: &AtomicBool::new(false),
                io_deadline: None,
                read_waker: None,
            },
        )
        .unwrap();
        let mut writes = socket.writes.as_slice();
        let mut delivered = 0;
        let mut outcome_count = 0;
        while !writes.is_empty() {
            let WebSocketFrameV1::Binary(bytes) =
                read_websocket_frame_v1(&mut writes, false).unwrap()
            else {
                panic!("unexpected frame");
            };
            match decode_message_v2(&bytes).unwrap() {
                ProductRelayWireMessageV1::Delivery(delivery) => {
                    let frame = ca.open_novorudp_frame(&delivery.envelope).unwrap();
                    assert_eq!(frame.payload, vec![delivered as u8; 200_000]);
                    delivered += 1;
                }
                ProductRelayWireMessageV1::ForwardOutcome(outcome) => {
                    assert!(outcome.forwarded);
                    assert_eq!(
                        delivered, 15,
                        "unconsumed egress exceeded cumulative window"
                    );
                    outcome_count += 1;
                }
                other => panic!("unexpected output: {other:?}"),
            }
        }
        assert_eq!((delivered, outcome_count), (22, 1));
        assert_eq!(
            runtime
                .block_on(manager.snapshot())
                .active_queued_frame_count,
            QUEUED_DELIVERIES - 22 + 1
        );
    }

    #[test]
    fn sender_outcomes_precede_one_fair_egress_item_per_request() {
        let runtime = TokioRuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let manager =
            ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1::default()).unwrap();
        let relay_identity = SigningKey::from_bytes(&[151; 32]);
        let node_a = SigningKey::from_bytes(&[152; 32]);
        let node_b = SigningKey::from_bytes(&[153; 32]);
        let now = now_ms_v1();
        let authenticated_a = authenticate_test_peer_v1(&node_a, &relay_identity, now);
        let authenticated_b = authenticate_test_peer_v1(&node_b, &relay_identity, now);
        let (registration_a, mut inbox_a) = runtime
            .block_on(manager.register_authenticated_session(authenticated_a, now))
            .unwrap();
        let (registration_b, _inbox_b) = runtime
            .block_on(manager.register_authenticated_session(authenticated_b, now))
            .unwrap();
        let (mut channel_a, mut channel_b) = test_peer_channels_v1(&node_a, &node_b, now);

        for sequence in 0..40 {
            let reverse = channel_b
                .seal_novorudp_frame(&test_data_frame_v1(sequence))
                .unwrap();
            let outcome = runtime.block_on(manager.forward_opaque(
                &registration_b.peer_id,
                registration_b.session_id,
                reverse,
                now,
            ));
            assert!(outcome.forwarded);
        }

        let mut scripted_reads = Vec::new();
        for sequence in 100..102 {
            let outbound = channel_a
                .seal_novorudp_frame(&test_data_frame_v1(sequence))
                .unwrap();
            write_masked_wire_message_v1(
                &mut scripted_reads,
                &ProductRelayWireMessageV1::Data(outbound),
            )
            .unwrap();
        }
        write_masked_wire_message_v1(&mut scripted_reads, &ProductRelayWireMessageV1::Close)
            .unwrap();
        let mut websocket = ScriptedWebSocketV1 {
            reads: Cursor::new(scripted_reads),
            writes: Vec::new(),
        };
        let stopping = AtomicBool::new(false);

        relay_connection_loop_v1(
            &mut websocket,
            ProductRelayConnectionLoopV1 {
                manager: &manager,
                runtime: &runtime,
                peer_id: &registration_a.peer_id,
                session_id: registration_a.session_id,
                inbox: &mut inbox_a,
                stopping: &stopping,
                io_deadline: None,
                read_waker: None,
            },
        )
        .unwrap();

        let mut writes = websocket.writes.as_slice();
        for _ in 0..2 {
            let WebSocketFrameV1::Binary(bytes) =
                read_websocket_frame_v1(&mut writes, false).unwrap()
            else {
                panic!("sender request did not receive a binary forward outcome");
            };
            let message: ProductRelayWireMessageV1 = decode_message_v2(&bytes).unwrap();
            assert!(matches!(
                message,
                ProductRelayWireMessageV1::ForwardOutcome(outcome)
                    if outcome.forwarded && !outcome.queued
            ));
            let WebSocketFrameV1::Binary(bytes) =
                read_websocket_frame_v1(&mut writes, false).unwrap()
            else {
                panic!("bounded fair egress did not follow the forward outcome");
            };
            let message: ProductRelayWireMessageV1 = decode_message_v2(&bytes).unwrap();
            assert!(matches!(message, ProductRelayWireMessageV1::Delivery(_)));
        }
        assert!(writes.is_empty());
        assert!(inbox_a.try_recv().is_ok());
    }

    #[test]
    fn physical_connection_admission_is_bounded_and_recoverable() {
        assert!(validate_connection_session_headroom_v1(2, 2).is_err());
        validate_connection_session_headroom_v1(3, 2).unwrap();
        let admission = ProductRelayConnectionAdmissionV1::new(2).unwrap();
        let first = admission.try_acquire().expect("first permit");
        let second = admission.try_acquire().expect("second permit");
        assert!(admission.try_acquire().is_none());
        assert_eq!(admission.active_connections.load(Ordering::Acquire), 2);
        assert_eq!(admission.rejected_connections.load(Ordering::Acquire), 1);
        drop(first);
        let replacement = admission.try_acquire().expect("recovered permit");
        assert_eq!(admission.active_connections.load(Ordering::Acquire), 2);
        drop(replacement);
        drop(second);
        assert_eq!(admission.active_connections.load(Ordering::Acquire), 0);
    }

    #[test]
    fn omitted_connection_limit_preserves_legacy_large_session_configs() {
        assert_eq!(resolve_max_connections_v1(None, 2_048), 2_049);
        assert_eq!(resolve_max_connections_v1(Some(700), 2_048), 700);
        assert!(validate_connection_session_headroom_v1(700, 2_048).is_err());
    }

    #[test]
    fn absolute_handshake_deadline_is_not_reset_by_byte_progress() {
        struct SlowProgressReaderV1 {
            bytes: Cursor<Vec<u8>>,
        }

        impl Read for SlowProgressReaderV1 {
            fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
                thread::sleep(Duration::from_millis(2));
                let read_len = output.len().min(1);
                self.bytes.read(&mut output[..read_len])
            }
        }

        let stopping = AtomicBool::new(false);
        let guard = ProductRelayReadDeadlineV1 {
            deadline: Instant::now() + Duration::from_millis(10),
            stopping: &stopping,
            scope: "handshake",
            return_idle_timeout: false,
            frame_started: Cell::new(false),
        };
        let mut reader = SlowProgressReaderV1 {
            bytes: Cursor::new(
                b"GET /novovm HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\n\r\n".to_vec(),
            ),
        };
        let error = read_http_headers_with_guard_v1(&mut reader, Some(&guard)).unwrap_err();
        assert!(error.to_string().contains("absolute handshake deadline"));
    }

    #[test]
    fn relay_runtime_config_applies_explicit_limits() {
        let config = ProductRelayDaemonConfigV1 {
            bind_addr: "127.0.0.1:443".into(),
            tls_cert_path: "cert.pem".into(),
            tls_key_path: "key.pem".into(),
            relay_identity_key_path: "identity.key".into(),
            report_path: "report.json".into(),
            report_interval_ms: 1_000,
            run_for_ms: None,
            max_connections: Some(19),
            handshake_timeout_ms: Some(20),
            max_sessions: Some(2),
            max_tracked_sources: Some(18),
            session_queue_capacity: Some(3),
            session_queue_bytes: Some(30),
            active_queue_total: Some(6),
            active_queue_bytes_total: Some(60),
            offline_queue_per_peer: Some(4),
            offline_queue_bytes_per_peer: Some(40),
            offline_queue_per_source: Some(5),
            offline_queue_bytes_per_source: Some(50),
            offline_queue_total: Some(5),
            offline_queue_bytes_total: Some(60),
            offline_queue_ttl_ms: Some(9),
            session_ttl_ms: Some(6),
            rate_limit_frames: Some(7),
            max_frames_per_window: Some(70),
            rate_limit_window_ms: Some(8),
            source_bytes_per_minute: Some(70),
            max_bytes_per_minute: Some(80),
        };
        let runtime = relay_runtime_config_v1(&config);
        assert_eq!(
            (
                runtime.max_sessions,
                runtime.max_tracked_sources,
                runtime.session_queue_capacity,
                runtime.session_queue_bytes,
                runtime.active_queue_total,
                runtime.active_queue_bytes_total,
                runtime.offline_queue_per_peer,
                runtime.offline_queue_bytes_per_peer,
                runtime.offline_queue_per_source,
                runtime.offline_queue_bytes_per_source,
                runtime.offline_queue_total,
            ),
            (2, 18, 3, 30, 6, 60, 4, 40, 5, 50, 5)
        );
        assert_eq!(
            (
                runtime.offline_queue_bytes_total,
                runtime.offline_queue_ttl_ms,
                runtime.session_ttl_ms,
                runtime.rate_limit_frames,
                runtime.max_frames_per_window,
                runtime.rate_limit_window_ms,
                runtime.source_bytes_per_minute,
                runtime.max_bytes_per_minute,
            ),
            (60, 9, 6, 7, 70, 8, 70, 80)
        );

        let mut partial_legacy_config = config;
        partial_legacy_config.max_sessions = Some(2_048);
        partial_legacy_config.max_tracked_sources = None;
        partial_legacy_config.offline_queue_per_source = None;
        partial_legacy_config.offline_queue_bytes_per_source = None;
        partial_legacy_config.rate_limit_frames = Some(100_000);
        partial_legacy_config.max_frames_per_window = None;
        let compatible = relay_runtime_config_v1(&partial_legacy_config);
        assert_eq!(compatible.max_tracked_sources, 2_048);
        assert_eq!(compatible.offline_queue_per_source, 5);
        assert_eq!(compatible.offline_queue_bytes_per_source, 60);
        assert_eq!(compatible.max_frames_per_window, 100_000);
    }

    #[test]
    fn daemon_authenticates_nodes_and_forwards_only_opaque_e2e_ciphertext() {
        let temp = product_relay_test_fixture_dir_v1("opaque-daemon");
        let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let certificate_path = temp.join("relay-cert.pem");
        let key_path = temp.join("relay-key.pem");
        let identity_path = temp.join("relay-identity.hex");
        let report_path = temp.join("reports/relay.json");
        fs::write(&certificate_path, certificate.serialize_pem().unwrap()).unwrap();
        fs::write(&key_path, certificate.serialize_private_key_pem()).unwrap();
        fs::write(&identity_path, hex_encode_v1(&[21; 32])).unwrap();
        let certificate_der = certificate.serialize_der().unwrap();
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let config = ProductRelayDaemonConfigV1 {
            bind_addr: format!("127.0.0.1:{port}"),
            tls_cert_path: certificate_path,
            tls_key_path: key_path,
            relay_identity_key_path: identity_path,
            report_path: report_path.clone(),
            report_interval_ms: 25,
            run_for_ms: Some(1_500),
            max_connections: Some(8),
            handshake_timeout_ms: Some(1_000),
            max_sessions: Some(4),
            max_tracked_sources: Some(16),
            session_queue_capacity: Some(8),
            session_queue_bytes: Some(1024 * 1024),
            active_queue_total: Some(32),
            active_queue_bytes_total: Some(4 * 1024 * 1024),
            offline_queue_per_peer: Some(8),
            offline_queue_bytes_per_peer: Some(1024 * 1024),
            offline_queue_per_source: Some(16),
            offline_queue_bytes_per_source: Some(2 * 1024 * 1024),
            offline_queue_total: Some(16),
            offline_queue_bytes_total: Some(2 * 1024 * 1024),
            offline_queue_ttl_ms: Some(5_000),
            session_ttl_ms: Some(5_000),
            rate_limit_frames: Some(100),
            max_frames_per_window: Some(1_000),
            rate_limit_window_ms: Some(1_000),
            source_bytes_per_minute: Some(16 * 1024 * 1024),
            max_bytes_per_minute: Some(32 * 1024 * 1024),
        };
        let daemon = thread::spawn(move || run_product_relay_daemon_v1(config));
        let client_config = test_client_tls_config_v1(certificate_der);
        let address: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let relay_identity = SigningKey::from_bytes(&[21; 32]);
        let relay_peer_id =
            peer_id_from_ed25519_public_key_v1(&relay_identity.verifying_key().to_bytes());
        let node_a = SigningKey::from_bytes(&[22; 32]);
        let node_b = SigningKey::from_bytes(&[23; 32]);
        let now = now_ms_v1();

        let mut client_b = connect_and_register_test_client_v1(
            address,
            Arc::clone(&client_config),
            &node_b,
            &relay_peer_id,
            now,
        );
        let mut client_a = connect_and_register_test_client_v1(
            address,
            Arc::clone(&client_config),
            &node_a,
            &relay_peer_id,
            now,
        );

        let node_a_peer_id = peer_id_from_ed25519_public_key_v1(&node_a.verifying_key().to_bytes());
        let node_b_peer_id = peer_id_from_ed25519_public_key_v1(&node_b.verifying_key().to_bytes());
        let peer_initiator =
            NodeHandshakeInitiatorV1::start(&node_a, node_b_peer_id.clone(), now_ms_v1(), 5_000)
                .unwrap();
        write_masked_wire_message_v1(
            &mut client_a,
            &ProductRelayWireMessageV1::PeerHandshake {
                target_peer_id: node_b_peer_id.clone(),
                handshake: RelayPeerHandshakeV1::Offer(peer_initiator.offer().clone()),
            },
        )
        .unwrap();
        let relay_offer = loop {
            match read_websocket_frame_v1(&mut client_b, false) {
                Ok(WebSocketFrameV1::Binary(bytes)) => match decode_message_v2(&bytes).unwrap() {
                    ProductRelayWireMessageV1::PeerHandshakeDelivery(delivery) => break delivery,
                    _ => continue,
                },
                Err(error) if is_timeout_v1(&error) => continue,
                other => panic!("unexpected relay handshake offer result: {other:?}"),
            }
        };
        let RelayPeerHandshakeV1::Offer(relayed_offer) = relay_offer.handshake else {
            panic!("relay did not forward a peer handshake offer");
        };
        let mut peer_replay = HandshakeReplayCacheV1::default();
        let peer_responder = NodeHandshakeResponderV1::respond(
            &relayed_offer,
            &node_b,
            now_ms_v1(),
            5_000,
            &mut peer_replay,
        )
        .unwrap();
        let peer_response = peer_responder.response().clone();
        let mut node_b_channel = peer_responder.into_channel();
        write_masked_wire_message_v1(
            &mut client_b,
            &ProductRelayWireMessageV1::PeerHandshake {
                target_peer_id: node_a_peer_id,
                handshake: RelayPeerHandshakeV1::Response(peer_response.clone()),
            },
        )
        .unwrap();
        let relayed_response = loop {
            match read_websocket_frame_v1(&mut client_a, false) {
                Ok(WebSocketFrameV1::Binary(bytes)) => match decode_message_v2(&bytes).unwrap() {
                    ProductRelayWireMessageV1::PeerHandshakeDelivery(delivery) => break delivery,
                    _ => continue,
                },
                Err(error) if is_timeout_v1(&error) => continue,
                other => panic!("unexpected relay handshake response result: {other:?}"),
            }
        };
        let RelayPeerHandshakeV1::Response(peer_response) = relayed_response.handshake else {
            panic!("relay did not forward a peer handshake response");
        };
        let mut initiator_replay = HandshakeReplayCacheV1::default();
        let mut node_a_channel = peer_initiator
            .complete(&peer_response, now_ms_v1(), &mut initiator_replay)
            .unwrap();
        let inner = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            [24; 16],
            25,
            26,
            27,
            28,
            b"opaque product relay test".to_vec(),
        );
        let envelope = node_a_channel.seal_novorudp_frame(&inner).unwrap();
        let expected_ciphertext = envelope.ciphertext.clone();
        write_masked_wire_message_v1(&mut client_a, &ProductRelayWireMessageV1::Data(envelope))
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(3);
        let received = loop {
            if Instant::now() > deadline {
                panic!("relay delivery deadline exceeded");
            }
            match read_websocket_frame_v1(&mut client_b, false) {
                Ok(WebSocketFrameV1::Binary(bytes)) => match decode_message_v2(&bytes).unwrap() {
                    ProductRelayWireMessageV1::Delivery(delivery) => break delivery,
                    _ => continue,
                },
                Err(error) if is_timeout_v1(&error) => continue,
                other => panic!("unexpected relay delivery result: {other:?}"),
            }
        };
        assert_eq!(received.envelope.ciphertext, expected_ciphertext);
        let decoded = node_b_channel
            .open_novorudp_frame(&received.envelope)
            .unwrap();
        assert_eq!(decoded.payload, inner.payload);
        drop(client_a);
        drop(client_b);
        daemon.join().unwrap().unwrap();
        let report: ProductRelayDaemonReportV1 =
            serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
        assert!(report.graceful_shutdown);
        assert_eq!(report.daemon_version, PRODUCT_RELAY_DAEMON_VERSION_V2);
        assert!(report.relay_runtime.forwarded_frame_total >= 1);
        assert!(report.payload_treated_opaque);
    }

    fn authenticate_test_peer_v1(
        node_identity: &SigningKey,
        relay_identity: &SigningKey,
        now_ms: u64,
    ) -> AuthenticatedPeerV1 {
        let relay_peer_id =
            peer_id_from_ed25519_public_key_v1(&relay_identity.verifying_key().to_bytes());
        let initiator =
            NodeHandshakeInitiatorV1::start(node_identity, relay_peer_id, now_ms, 5_000).unwrap();
        let mut replay = HandshakeReplayCacheV1::default();
        NodeHandshakeResponderV1::respond(
            initiator.offer(),
            relay_identity,
            now_ms.saturating_add(1),
            5_000,
            &mut replay,
        )
        .unwrap()
        .authenticated_remote()
        .clone()
    }

    fn test_peer_channels_v1(
        initiator_identity: &SigningKey,
        responder_identity: &SigningKey,
        now_ms: u64,
    ) -> (E2eSecureChannelV1, E2eSecureChannelV1) {
        let responder_peer_id =
            peer_id_from_ed25519_public_key_v1(&responder_identity.verifying_key().to_bytes());
        let initiator =
            NodeHandshakeInitiatorV1::start(initiator_identity, responder_peer_id, now_ms, 5_000)
                .unwrap();
        let mut responder_replay = HandshakeReplayCacheV1::default();
        let responder = NodeHandshakeResponderV1::respond(
            initiator.offer(),
            responder_identity,
            now_ms.saturating_add(1),
            5_000,
            &mut responder_replay,
        )
        .unwrap();
        let response = responder.response().clone();
        let responder_channel = responder.into_channel();
        let mut initiator_replay = HandshakeReplayCacheV1::default();
        let initiator_channel = initiator
            .complete(&response, now_ms.saturating_add(2), &mut initiator_replay)
            .unwrap();
        (initiator_channel, responder_channel)
    }

    fn test_data_frame_v1(sequence: u64) -> NovoRudpTransportFrameV0 {
        NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            [0x72; 16],
            1,
            2,
            sequence,
            3,
            format!("relay-opaque-{sequence}").into_bytes(),
        )
    }

    fn connect_and_register_test_client_v1(
        address: SocketAddr,
        client_config: Arc<rustls::ClientConfig>,
        identity: &SigningKey,
        relay_peer_id: &str,
        now_ms: u64,
    ) -> TestClientWebSocketV1 {
        let mut stream = loop {
            match TcpStream::connect(address) {
                Ok(tcp) => break connect_test_websocket_v1(tcp, client_config.clone()),
                Err(_) => thread::sleep(Duration::from_millis(20)),
            }
        };
        let initiator =
            NodeHandshakeInitiatorV1::start(identity, relay_peer_id, now_ms, 30_000).unwrap();
        write_masked_wire_message_v1(
            &mut stream,
            &ProductRelayWireMessageV1::HandshakeOffer(initiator.offer().clone()),
        )
        .unwrap();
        let response = match read_websocket_frame_v1(&mut stream, false).unwrap() {
            WebSocketFrameV1::Binary(bytes) => match serde_json::from_slice(&bytes).unwrap() {
                ProductRelayWireMessageV1::HandshakeResponse(response) => response,
                other => panic!("unexpected relay handshake response: {other:?}"),
            },
            other => panic!("unexpected relay handshake frame: {other:?}"),
        };
        let mut replay = HandshakeReplayCacheV1::default();
        initiator
            .complete(&response, now_ms_v1(), &mut replay)
            .unwrap();
        let WebSocketFrameV1::Binary(bytes) = read_websocket_frame_v1(&mut stream, false).unwrap()
        else {
            panic!("missing delivery window");
        };
        assert!(matches!(
            serde_json::from_slice(&bytes).unwrap(),
            ProductRelayWireMessageV1::DeliveryWindowV1 { max_unconsumed: 15 }
        ));
        write_masked_wire_message_v1(
            &mut stream,
            &ProductRelayWireMessageV1::DeliveryConsumedV1 { through: 0 },
        )
        .unwrap();
        stream
    }

    fn connect_test_websocket_v1(
        tcp: TcpStream,
        client_config: Arc<rustls::ClientConfig>,
    ) -> TestClientWebSocketV1 {
        tcp.set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        tcp.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
        let server_name = ServerName::try_from("localhost").unwrap();
        let connection = rustls::ClientConnection::new(client_config, server_name).unwrap();
        let mut stream = rustls::StreamOwned::new(connection, tcp);
        let key = BASE64_STANDARD.encode([9u8; 16]);
        write!(stream, "GET /novovm HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: {PRODUCT_RELAY_WEBSOCKET_SUBPROTOCOL_V2}\r\n\r\n").unwrap();
        stream.flush().unwrap();
        let response = read_http_headers_v1(&mut stream).unwrap();
        assert!(response.starts_with("HTTP/1.1 101"));
        stream
    }

    fn test_client_tls_config_v1(certificate_der: Vec<u8>) -> Arc<rustls::ClientConfig> {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(CertificateDer::from(certificate_der)).unwrap();
        Arc::new(
            rustls::ClientConfig::builder_with_provider(tls_crypto_provider_v1())
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        )
    }

    fn write_masked_wire_message_v1<S: Write>(
        stream: &mut S,
        message: &ProductRelayWireMessageV1,
    ) -> Result<()> {
        let payload = encode_message_v2(message)?;
        let mask = [0x13, 0x37, 0x39, 0x41];
        let mut header = Vec::with_capacity(payload.len() + 14);
        header.push(0x82);
        match payload.len() {
            len if len <= 125 => header.push(0x80 | len as u8),
            len if len <= u16::MAX as usize => {
                header.push(0x80 | 126);
                header.extend_from_slice(&(len as u16).to_be_bytes());
            }
            len => {
                header.push(0x80 | 127);
                header.extend_from_slice(&(len as u64).to_be_bytes());
            }
        }
        header.extend_from_slice(&mask);
        header.extend(
            payload
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ mask[index % 4]),
        );
        stream.write_all(&header)?;
        stream.flush()?;
        Ok(())
    }

    fn hex_encode_v1(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
