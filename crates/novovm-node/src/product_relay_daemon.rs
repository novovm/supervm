//! Headless WSS relay daemon for the product overlay runtime.
//!
//! TLS is a transport confidentiality layer only. Node authentication is the signed NOVOVM
//! challenge-response performed after the WebSocket upgrade; the relay never decrypts E2E frames.

use crate::product_relay_client::{
    PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_BYTES_V1,
    PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_EVENTS_V1,
};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::SigningKey;
use novovm_network::{
    HandshakeReplayCacheV1, NodeHandshakeResponderV1, ProductRelayRuntimeConfigV1,
    ProductRelaySessionManagerV1, ProductRelayWireMessageV1,
    PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1,
};
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
const MAX_DATA_DELIVERIES_PER_CONNECTION_TICK_V1: usize = 4;
const MAX_PEER_HANDSHAKE_DELIVERIES_PER_CONNECTION_TICK_V1: usize = 4;
const _: () = assert!(
    MAX_DATA_DELIVERIES_PER_CONNECTION_TICK_V1
        + MAX_PEER_HANDSHAKE_DELIVERIES_PER_CONNECTION_TICK_V1
        < PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_EVENTS_V1
);
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
    pub relay_runtime: novovm_network::RelayRuntimeSnapshotV1,
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
    let stopping = Arc::new(AtomicBool::new(false));
    let started_at_ms = now_ms_v1();
    let mut last_report_ms = 0u64;
    let mut last_maintenance_ms = 0u64;
    let mut connection_workers = Vec::new();

    loop {
        if config
            .run_for_ms
            .is_some_and(|duration| now_ms_v1().saturating_sub(started_at_ms) >= duration)
        {
            stopping.store(true, Ordering::Release);
        }
        if stopping.load(Ordering::Acquire) {
            break;
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
        if now_ms.saturating_sub(last_maintenance_ms) >= PRODUCT_RELAY_MAINTENANCE_INTERVAL_MS_V1 {
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

    manager.begin_graceful_shutdown();
    for worker in connection_workers {
        if worker.join().is_err() {
            eprintln!("product relay connection worker panicked during shutdown");
        }
    }
    write_product_relay_report_v1(
        &config.report_path,
        &listen_addr,
        &runtime,
        &manager,
        &admission,
        true,
    )?;
    Ok(())
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
    // Accepted sockets inherit the listener's nonblocking mode on Windows. TLS stream I/O uses
    // bounded blocking reads so that the session loop can service its relay inbox between reads.
    tcp.set_nonblocking(false)
        .context("set product relay connection blocking mode")?;
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
    let _ = handshake_finished.send(());
    if let Err(error) = io_deadline.clear_v1() {
        runtime.block_on(manager.disconnect(&peer_id, session_id));
        return Err(error).context("finish product relay lower-stream handshake deadline");
    }
    let result = relay_connection_loop_v1(
        &mut websocket,
        ProductRelayConnectionLoopV1 {
            manager: &manager,
            runtime: &runtime,
            peer_id: &peer_id,
            session_id,
            inbox: &mut inbox,
            stopping: &stopping,
            io_deadline: Some(&io_deadline),
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
    inbox: &'a mut novovm_network::RelaySessionInboxV1,
    stopping: &'a AtomicBool,
    io_deadline: Option<&'a ProductRelayDaemonIoDeadlineV1>,
}

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
    } = context;
    let mut prefer_control_delivery = false;
    while !stopping.load(Ordering::Acquire) {
        if !runtime.block_on(manager.is_current_session(peer_id, session_id, now_ms_v1())) {
            bail!("product relay session was replaced, expired, or revoked");
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
                let message: ProductRelayWireMessageV1 = match serde_json::from_slice(&bytes) {
                    Ok(message) => message,
                    Err(error) => {
                        runtime.block_on(manager.reject_admitted_wire_v1(admission));
                        return Err(error).context("decode product relay wire message");
                    }
                };
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
                    ProductRelayWireMessageV1::Close => return Ok(()),
                    ProductRelayWireMessageV1::HandshakeOffer(_)
                    | ProductRelayWireMessageV1::HandshakeResponse(_)
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
                    peer_id,
                    session_id,
                    inbox,
                    &mut prefer_control_delivery,
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
                    peer_id,
                    session_id,
                    inbox,
                    &mut prefer_control_delivery,
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
                    peer_id,
                    session_id,
                    inbox,
                    &mut prefer_control_delivery,
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
                    MAX_DATA_DELIVERIES_PER_CONNECTION_TICK_V1,
                    || stopping.load(Ordering::Acquire),
                    || inbox.try_recv().ok(),
                    |delivery| {
                        if !runtime.block_on(manager.is_current_session(
                            peer_id,
                            session_id,
                            now_ms_v1(),
                        )) {
                            bail!("product relay session was replaced before data delivery");
                        }
                        write_wire_message_v1(
                            websocket,
                            &ProductRelayWireMessageV1::Delivery(delivery),
                        )
                    },
                )?;
                drain_bounded_relay_inbox_v1(
                    MAX_PEER_HANDSHAKE_DELIVERIES_PER_CONNECTION_TICK_V1,
                    || stopping.load(Ordering::Acquire),
                    || inbox.try_recv_peer_handshake().ok(),
                    |delivery| {
                        if !runtime.block_on(manager.is_current_session(
                            peer_id,
                            session_id,
                            now_ms_v1(),
                        )) {
                            bail!("product relay session was replaced before control delivery");
                        }
                        write_wire_message_v1(
                            websocket,
                            &ProductRelayWireMessageV1::PeerHandshakeDelivery(delivery),
                        )
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
    disposition: novovm_network::RelayForwardDispositionV1,
) -> bool {
    matches!(
        disposition,
        novovm_network::RelayForwardDispositionV1::RejectedSourceSessionMissing
            | novovm_network::RelayForwardDispositionV1::RejectedStaleSourceSession
            | novovm_network::RelayForwardDispositionV1::RejectedSourceSessionExpired
            | novovm_network::RelayForwardDispositionV1::RejectedRouteMismatch
            | novovm_network::RelayForwardDispositionV1::RejectedShuttingDown
    )
}

fn service_one_relay_inbox_v1<S: Write>(
    websocket: &mut S,
    manager: &ProductRelaySessionManagerV1,
    runtime: &Runtime,
    peer_id: &str,
    session_id: [u8; 16],
    inbox: &mut novovm_network::RelaySessionInboxV1,
    prefer_control: &mut bool,
) -> Result<bool> {
    runtime.block_on(manager.drain_queued_for_session(peer_id, session_id, now_ms_v1()));
    enum RelayInboxItemV1 {
        Data(novovm_network::OpaqueRelayDeliveryV1),
        Control(novovm_network::RelayPeerHandshakeDeliveryV1),
    }
    let item = if *prefer_control {
        inbox
            .try_recv_peer_handshake()
            .map(RelayInboxItemV1::Control)
            .or_else(|_| inbox.try_recv().map(RelayInboxItemV1::Data))
    } else {
        inbox.try_recv().map(RelayInboxItemV1::Data).or_else(|_| {
            inbox
                .try_recv_peer_handshake()
                .map(RelayInboxItemV1::Control)
        })
    };
    let Ok(item) = item else {
        return Ok(false);
    };
    if !runtime.block_on(manager.is_current_session(peer_id, session_id, now_ms_v1())) {
        bail!("product relay session was replaced before queued delivery");
    }
    match item {
        RelayInboxItemV1::Data(delivery) => {
            write_wire_message_v1(websocket, &ProductRelayWireMessageV1::Delivery(delivery))?;
            *prefer_control = true;
        }
        RelayInboxItemV1::Control(delivery) => {
            write_wire_message_v1(
                websocket,
                &ProductRelayWireMessageV1::PeerHandshakeDelivery(delivery),
            )?;
            *prefer_control = false;
        }
    }
    Ok(true)
}

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
}

struct ProductRelayDaemonDeadlineTcpStreamV1 {
    inner: TcpStream,
    deadline: ProductRelayDaemonIoDeadlineV1,
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
        state.deadline = Some(deadline);
        state.lower_read_progressed = false;
        drop(state);
        self.check_v1()
    }

    fn begin_if_idle_v1(&self, deadline: Instant) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("product relay daemon I/O deadline lock poisoned"))?;
        if state.deadline.is_none() {
            state.deadline = Some(deadline);
            state.lower_read_progressed = false;
        }
        drop(state);
        self.check_v1()
    }

    fn clear_v1(&self) -> io::Result<()> {
        self.check_v1()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("product relay daemon I/O deadline lock poisoned"))?;
        state.deadline = None;
        state.lower_read_progressed = false;
        Ok(())
    }

    fn preserve_partial_read_deadline_v1(&self) -> io::Result<bool> {
        if self.stopping.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "product relay daemon is stopping",
            ));
        }
        let state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("product relay daemon I/O deadline lock poisoned"))?;
        if state
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "product relay daemon partial lower-stream deadline expired",
            ));
        }
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
        if self.stopping.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "product relay daemon is stopping",
            ));
        }
        let deadline = self
            .state
            .lock()
            .map_err(|_| io::Error::other("product relay daemon I/O deadline lock poisoned"))?
            .deadline;
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "product relay daemon absolute lower-stream I/O deadline exceeded",
            ));
        }
        Ok(())
    }
}

impl Read for ProductRelayDaemonDeadlineTcpStreamV1 {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.deadline.check_v1()?;
        let result = self.inner.read(output);
        if let Ok(read) = result {
            self.deadline.record_lower_read_v1(read)?;
        } else {
            self.deadline.check_v1()?;
        }
        result
    }
}

impl Write for ProductRelayDaemonDeadlineTcpStreamV1 {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        self.deadline.check_v1()?;
        let result = self.inner.write(input);
        self.deadline.check_v1()?;
        result
    }

    fn flush(&mut self) -> io::Result<()> {
        self.deadline.check_v1()?;
        let result = self.inner.flush();
        self.deadline.check_v1()?;
        result
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
    write!(stream, "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n")?;
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
        } else if name.eq_ignore_ascii_case("sec-websocket-key")
            && key.replace(value.to_string()).is_some()
        {
            bail!("duplicate Sec-WebSocket-Key");
        }
    }
    if !host_present || !upgrade_websocket || !connection_upgrade || !version_13 {
        bail!("relay WebSocket upgrade headers are incomplete or invalid");
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
    write_websocket_frame_v1(stream, 0x2, &serde_json::to_vec(message)?)
}

fn write_websocket_frame_v1<S: Write>(stream: &mut S, opcode: u8, payload: &[u8]) -> Result<()> {
    validate_websocket_payload_size_v1(
        opcode,
        payload.len(),
        PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1,
    )?;
    let mut header = Vec::with_capacity(14 + payload.len());
    header.push(0x80 | (opcode & 0x0f));
    match payload.len() {
        len if len <= 125 => header.push(len as u8),
        len if len <= u16::MAX as usize => {
            header.push(126);
            header.extend_from_slice(&(len as u16).to_be_bytes());
        }
        len => {
            header.push(127);
            header.extend_from_slice(&(len as u64).to_be_bytes());
        }
    }
    header.extend_from_slice(payload);
    stream.write_all(&header)?;
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
    }
    if len == 127 {
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
mod tests {
    use super::*;
    use novovm_network::{
        peer_id_from_ed25519_public_key_v1, AuthenticatedPeerV1, E2eSecureChannelV1,
        HandshakeReplayCacheV1, NodeHandshakeInitiatorV1, NodeHandshakeResponderV1,
        NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0, RelayPeerHandshakeV1,
    };
    use rustls::pki_types::{CertificateDer, ServerName};
    use std::{cell::Cell, collections::VecDeque, io::Cursor, net::SocketAddr, time::Instant};

    type TestClientWebSocketV1 = rustls::StreamOwned<rustls::ClientConnection, TcpStream>;

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
    fn websocket_upgrade_requires_rfc6455_headers_and_fresh_key_shape() {
        let valid = "GET /novovm HTTP/1.1\r\nHost: relay.example\r\nUpgrade: websocket\r\nConnection: keep-alive, Upgrade\r\nSec-WebSocket-Key: AAECAwQFBgcICQoLDA0ODw==\r\nSec-WebSocket-Version: 13\r\n\r\n";
        assert_eq!(
            validate_websocket_upgrade_request_v1(valid).unwrap(),
            "AAECAwQFBgcICQoLDA0ODw=="
        );
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
            inner: tcp,
            deadline,
        };
        let mut bytes = [0u8; 8];
        let error = guarded.read_exact(&mut bytes).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
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
            let message: ProductRelayWireMessageV1 = serde_json::from_slice(&bytes).unwrap();
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
            let message: ProductRelayWireMessageV1 = serde_json::from_slice(&bytes).unwrap();
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
        let temp = std::env::temp_dir().join(format!("novovm-product-relay-test-{}", now_ms_v1()));
        fs::create_dir_all(&temp).unwrap();
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
                Ok(WebSocketFrameV1::Binary(bytes)) => {
                    match serde_json::from_slice::<ProductRelayWireMessageV1>(&bytes).unwrap() {
                        ProductRelayWireMessageV1::PeerHandshakeDelivery(delivery) => {
                            break delivery
                        }
                        _ => continue,
                    }
                }
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
                Ok(WebSocketFrameV1::Binary(bytes)) => {
                    match serde_json::from_slice::<ProductRelayWireMessageV1>(&bytes).unwrap() {
                        ProductRelayWireMessageV1::PeerHandshakeDelivery(delivery) => {
                            break delivery
                        }
                        _ => continue,
                    }
                }
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
                Ok(WebSocketFrameV1::Binary(bytes)) => {
                    match serde_json::from_slice::<ProductRelayWireMessageV1>(&bytes).unwrap() {
                        ProductRelayWireMessageV1::Delivery(delivery) => break delivery,
                        _ => continue,
                    }
                }
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
        let _ = fs::remove_dir_all(temp);
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
        write!(stream, "GET /novovm HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n").unwrap();
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
        let payload = serde_json::to_vec(message)?;
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
