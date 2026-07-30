//! Headless WSS relay daemon for the product overlay runtime.
//!
//! TLS is a transport confidentiality layer only. Node authentication is the signed NOVOVM
//! challenge-response performed after the WebSocket upgrade; the relay never decrypts E2E frames.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::SigningKey;
use novovm_network::{
    HandshakeReplayCacheV1, NodeHandshakeResponderV1, ProductRelayRuntimeConfigV1,
    ProductRelaySessionManagerV1, ProductRelayWireMessageV1,
};
use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
use serde::{Deserialize, Serialize};
use sha1::{Digest as Sha1Digest, Sha1};
use std::{
    fs,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::runtime::{Builder as TokioRuntimeBuilder, Runtime};

const PRODUCT_RELAY_DAEMON_VERSION_V1: u16 = 1;
const PRODUCT_RELAY_WEBSOCKET_PATH_V1: &str = "/novovm";
const MAX_WEBSOCKET_FRAME_BYTES_V1: usize = 1_048_576;

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
    #[serde(default)]
    pub session_queue_capacity: Option<usize>,
    #[serde(default)]
    pub offline_queue_per_peer: Option<usize>,
    #[serde(default)]
    pub offline_queue_total: Option<usize>,
    #[serde(default)]
    pub session_ttl_ms: Option<u64>,
    #[serde(default)]
    pub rate_limit_frames: Option<u64>,
    #[serde(default)]
    pub rate_limit_window_ms: Option<u64>,
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
    pub relay_runtime: novovm_network::RelayRuntimeSnapshotV1,
}

#[derive(Debug)]
enum WebSocketFrameV1 {
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong,
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
    let manager = runtime
        .block_on(async { ProductRelaySessionManagerV1::new(relay_runtime_config_v1(&config)) })
        .context("create product relay session manager")?;
    let replay_cache = Arc::new(Mutex::new(HandshakeReplayCacheV1::default()));
    let stopping = Arc::new(AtomicBool::new(false));
    let started_at_ms = now_ms_v1();
    let mut last_report_ms = 0u64;

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
                let manager = manager.clone();
                let runtime = Arc::clone(&runtime);
                let tls_config = Arc::clone(&tls_config);
                let relay_identity = relay_identity.clone();
                let replay_cache = Arc::clone(&replay_cache);
                thread::spawn(move || {
                    if let Err(error) = serve_product_relay_connection_v1(
                        tcp,
                        tls_config,
                        relay_identity,
                        manager,
                        runtime,
                        replay_cache,
                    ) {
                        eprintln!("product relay connection closed: {error:#}");
                    }
                });
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error).context("accept product relay connection"),
        }

        let now_ms = now_ms_v1();
        if now_ms.saturating_sub(last_report_ms) >= config.report_interval_ms {
            runtime.block_on(manager.expire_stale_sessions(now_ms));
            write_product_relay_report_v1(
                &config.report_path,
                &listen_addr,
                &runtime,
                &manager,
                false,
            )?;
            last_report_ms = now_ms;
        }
    }

    manager.begin_graceful_shutdown();
    write_product_relay_report_v1(&config.report_path, &listen_addr, &runtime, &manager, true)?;
    Ok(())
}

fn serve_product_relay_connection_v1(
    tcp: TcpStream,
    tls_config: Arc<rustls::ServerConfig>,
    relay_identity: SigningKey,
    manager: ProductRelaySessionManagerV1,
    runtime: Arc<Runtime>,
    replay_cache: Arc<Mutex<HandshakeReplayCacheV1>>,
) -> Result<()> {
    // Accepted sockets inherit the listener's nonblocking mode on Windows. TLS stream I/O uses
    // bounded blocking reads so that the session loop can service its relay inbox between reads.
    tcp.set_nonblocking(false)
        .context("set product relay connection blocking mode")?;
    tcp.set_read_timeout(Some(Duration::from_millis(100)))
        .context("set product relay read timeout")?;
    tcp.set_write_timeout(Some(Duration::from_secs(5)))
        .context("set product relay write timeout")?;
    let server =
        rustls::ServerConnection::new(tls_config).context("create product relay tls connection")?;
    let mut websocket = rustls::StreamOwned::new(server, tcp);
    accept_websocket_v1(&mut websocket)?;

    let offer = match read_websocket_frame_v1(&mut websocket, true)? {
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
    write_wire_message_v1(
        &mut websocket,
        &ProductRelayWireMessageV1::HandshakeResponse(responder.response().clone()),
    )?;
    let authenticated = responder.authenticated_remote().clone();
    let (registration, mut inbox) = runtime
        .block_on(manager.register_authenticated_session(authenticated, now_ms_v1()))
        .context("register authenticated product relay session")?;

    let peer_id = registration.peer_id;
    let session_id = registration.session_id;
    let result = relay_connection_loop_v1(
        &mut websocket,
        &manager,
        &runtime,
        &peer_id,
        session_id,
        &mut inbox,
    );
    runtime.block_on(manager.disconnect(&peer_id, session_id));
    result
}

fn relay_connection_loop_v1<S: Read + Write>(
    websocket: &mut S,
    manager: &ProductRelaySessionManagerV1,
    runtime: &Runtime,
    peer_id: &str,
    session_id: [u8; 16],
    inbox: &mut novovm_network::RelaySessionInboxV1,
) -> Result<()> {
    loop {
        while let Ok(delivery) = inbox.try_recv() {
            write_wire_message_v1(websocket, &ProductRelayWireMessageV1::Delivery(delivery))?;
        }
        while let Ok(delivery) = inbox.try_recv_peer_handshake() {
            write_wire_message_v1(
                websocket,
                &ProductRelayWireMessageV1::PeerHandshakeDelivery(delivery),
            )?;
        }
        match read_websocket_frame_v1(websocket, true) {
            Ok(WebSocketFrameV1::Binary(bytes)) => {
                let message: ProductRelayWireMessageV1 =
                    serde_json::from_slice(&bytes).context("decode product relay wire message")?;
                match message {
                    ProductRelayWireMessageV1::Data(envelope) => {
                        runtime.block_on(manager.forward_opaque(
                            peer_id,
                            session_id,
                            envelope,
                            now_ms_v1(),
                        ));
                    }
                    ProductRelayWireMessageV1::PeerHandshake {
                        target_peer_id,
                        handshake,
                    } => {
                        runtime.block_on(manager.forward_peer_handshake(
                            peer_id,
                            session_id,
                            &target_peer_id,
                            handshake,
                            now_ms_v1(),
                        ));
                    }
                    ProductRelayWireMessageV1::Heartbeat => {
                        runtime.block_on(manager.heartbeat(peer_id, session_id, now_ms_v1()));
                        write_wire_message_v1(websocket, &ProductRelayWireMessageV1::HeartbeatAck)?;
                    }
                    ProductRelayWireMessageV1::Close => return Ok(()),
                    ProductRelayWireMessageV1::HandshakeOffer(_)
                    | ProductRelayWireMessageV1::HandshakeResponse(_)
                    | ProductRelayWireMessageV1::Delivery(_)
                    | ProductRelayWireMessageV1::PeerHandshakeDelivery(_)
                    | ProductRelayWireMessageV1::HeartbeatAck => {
                        bail!("invalid relay wire message after authentication")
                    }
                }
            }
            Ok(WebSocketFrameV1::Ping(payload)) => {
                write_websocket_frame_v1(websocket, 0xA, &payload)?
            }
            Ok(WebSocketFrameV1::Pong) => {
                runtime.block_on(manager.heartbeat(peer_id, session_id, now_ms_v1()));
            }
            Ok(WebSocketFrameV1::Close) => return Ok(()),
            Err(error) if is_timeout_v1(&error) => continue,
            Err(error) => return Err(error),
        }
    }
}

fn relay_runtime_config_v1(config: &ProductRelayDaemonConfigV1) -> ProductRelayRuntimeConfigV1 {
    let mut runtime = ProductRelayRuntimeConfigV1::default();
    if let Some(value) = config.session_queue_capacity {
        runtime.session_queue_capacity = value;
    }
    if let Some(value) = config.offline_queue_per_peer {
        runtime.offline_queue_per_peer = value;
    }
    if let Some(value) = config.offline_queue_total {
        runtime.offline_queue_total = value;
    }
    if let Some(value) = config.session_ttl_ms {
        runtime.session_ttl_ms = value;
    }
    if let Some(value) = config.rate_limit_frames {
        runtime.rate_limit_frames = value;
    }
    if let Some(value) = config.rate_limit_window_ms {
        runtime.rate_limit_window_ms = value;
    }
    runtime
}

fn write_product_relay_report_v1(
    report_path: &Path,
    listen_addr: &str,
    runtime: &Runtime,
    manager: &ProductRelaySessionManagerV1,
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
        daemon_version: PRODUCT_RELAY_DAEMON_VERSION_V1,
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

fn accept_websocket_v1<S: Read + Write>(stream: &mut S) -> Result<()> {
    let request = read_http_headers_v1(stream)?;
    let mut lines = request.lines();
    let request_line = lines
        .next()
        .context("missing relay WebSocket request line")?;
    let mut parts = request_line.split_whitespace();
    if parts.next() != Some("GET") || parts.next() != Some(PRODUCT_RELAY_WEBSOCKET_PATH_V1) {
        bail!("invalid relay WebSocket request line: {request_line}");
    }
    let key = lines
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("sec-websocket-key"))
                .map(|(_, value)| value.trim().to_string())
        })
        .context("missing Sec-WebSocket-Key")?;
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let accept = BASE64_STANDARD.encode(hasher.finalize());
    write!(stream, "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n")?;
    stream.flush()?;
    Ok(())
}

fn read_http_headers_v1<S: Read>(stream: &mut S) -> Result<String> {
    let mut bytes = Vec::new();
    let mut one = [0u8; 1];
    while bytes.len() < 8192 {
        stream.read_exact(&mut one)?;
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

fn read_websocket_frame_v1<S: Read>(
    stream: &mut S,
    require_masked: bool,
) -> Result<WebSocketFrameV1> {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header)?;
    if header[0] & 0x80 == 0 {
        bail!("fragmented WebSocket frames are not supported");
    }
    let opcode = header[0] & 0x0f;
    let masked = header[1] & 0x80 != 0;
    if require_masked && !masked {
        bail!("relay requires masked client WebSocket frames");
    }
    let mut len = (header[1] & 0x7f) as u64;
    if len == 126 {
        let mut extended = [0u8; 2];
        stream.read_exact(&mut extended)?;
        len = u16::from_be_bytes(extended) as u64;
    }
    if len == 127 {
        let mut extended = [0u8; 8];
        stream.read_exact(&mut extended)?;
        len = u64::from_be_bytes(extended);
    }
    if len > MAX_WEBSOCKET_FRAME_BYTES_V1 as u64 {
        bail!("relay WebSocket frame exceeds maximum size");
    }
    let mask = if masked {
        let mut mask = [0u8; 4];
        stream.read_exact(&mut mask)?;
        Some(mask)
    } else {
        None
    };
    let mut payload = vec![0u8; len as usize];
    stream.read_exact(&mut payload)?;
    if let Some(mask) = mask {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }
    }
    match opcode {
        0x2 => Ok(WebSocketFrameV1::Binary(payload)),
        0x9 => Ok(WebSocketFrameV1::Ping(payload)),
        0xA => Ok(WebSocketFrameV1::Pong),
        0x8 => Ok(WebSocketFrameV1::Close),
        _ => bail!("unsupported relay WebSocket opcode: {opcode}"),
    }
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
        peer_id_from_ed25519_public_key_v1, HandshakeReplayCacheV1, NodeHandshakeInitiatorV1,
        NodeHandshakeResponderV1, NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0,
        RelayPeerHandshakeV1,
    };
    use rustls::pki_types::{CertificateDer, ServerName};
    use std::{net::SocketAddr, time::Instant};

    type TestClientWebSocketV1 = rustls::StreamOwned<rustls::ClientConnection, TcpStream>;

    #[test]
    fn websocket_binary_and_masking_round_trip() {
        let mut wire = Vec::new();
        write_websocket_frame_v1(&mut wire, 0x2, b"opaque").unwrap();
        assert!(
            matches!(read_websocket_frame_v1(&mut wire.as_slice(), false).unwrap(), WebSocketFrameV1::Binary(bytes) if bytes == b"opaque")
        );
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
            session_queue_capacity: Some(3),
            offline_queue_per_peer: Some(4),
            offline_queue_total: Some(5),
            session_ttl_ms: Some(6),
            rate_limit_frames: Some(7),
            rate_limit_window_ms: Some(8),
        };
        let runtime = relay_runtime_config_v1(&config);
        assert_eq!(
            (
                runtime.session_queue_capacity,
                runtime.offline_queue_per_peer,
                runtime.offline_queue_total,
                runtime.session_ttl_ms,
                runtime.rate_limit_frames,
                runtime.rate_limit_window_ms
            ),
            (3, 4, 5, 6, 7, 8)
        );
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
            session_queue_capacity: Some(8),
            offline_queue_per_peer: Some(8),
            offline_queue_total: Some(16),
            session_ttl_ms: Some(5_000),
            rate_limit_frames: Some(100),
            rate_limit_window_ms: Some(1_000),
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
        assert!(report.relay_runtime.forwarded_frame_total >= 1);
        assert!(report.payload_treated_opaque);
        let _ = fs::remove_dir_all(temp);
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
