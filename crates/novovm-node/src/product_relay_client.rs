//! Node-side WSS relay client for the product relay protocol.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::SigningKey;
use novovm_network::{
    HandshakeReplayCacheV1, NodeHandshakeInitiatorV1, OpaqueRelayDeliveryV1,
    ProductRelayWireMessageV1, RelayPeerHandshakeDeliveryV1, RelayPeerHandshakeV1,
    SecureNovoRudpEnvelopeV1,
};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{pem::PemObject, CertificateDer, ServerName, UnixTime},
    DigitallySignedStruct, SignatureScheme,
};
use serde::{Deserialize, Serialize};
use sha1::{Digest as Sha1Digest, Sha1};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

const MAX_WEBSOCKET_FRAME_BYTES_V1: usize = 1_048_576;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductRelayTlsTrustV1 {
    NativeWebPki,
    ExplicitCa {
        certificate_path: PathBuf,
    },
    /// The post-upgrade signed node handshake, not TLS PKI, authenticates the relay.
    NodeKeyBoundEncrypted,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProductRelayClientConfigV1 {
    pub endpoint: String,
    pub expected_relay_peer_id: String,
    #[serde(default = "default_connect_timeout_ms_v1")]
    pub connect_timeout_ms: u64,
    #[serde(default = "default_read_timeout_ms_v1")]
    pub read_timeout_ms: u64,
    #[serde(default = "default_tls_trust_v1")]
    pub tls_trust: ProductRelayTlsTrustV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductRelayClientSessionV1 {
    pub relay_peer_id: String,
    pub endpoint: String,
    pub websocket_path: String,
    pub node_identity_challenge_response_verified: bool,
    pub tls_is_novorudp_identity_root: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductRelayClientEventV1 {
    Delivery(OpaqueRelayDeliveryV1),
    PeerHandshake(RelayPeerHandshakeDeliveryV1),
    HeartbeatAck,
    Closed,
}

pub struct ProductRelayClientV1 {
    stream: rustls::StreamOwned<rustls::ClientConnection, TcpStream>,
    session: ProductRelayClientSessionV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductRelayReconnectPolicyV1 {
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for ProductRelayReconnectPolicyV1 {
    fn default() -> Self {
        Self {
            base_delay_ms: 500,
            max_delay_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductRelayReconnectStateV1 {
    pub consecutive_failure_count: u32,
    pub next_delay_ms: u64,
    pub last_connect_succeeded: bool,
}

pub struct ProductRelayConnectorV1 {
    identity: SigningKey,
    config: ProductRelayClientConfigV1,
    policy: ProductRelayReconnectPolicyV1,
    state: ProductRelayReconnectStateV1,
}

impl ProductRelayClientV1 {
    pub fn connect(identity: &SigningKey, config: &ProductRelayClientConfigV1) -> Result<Self> {
        if config.expected_relay_peer_id.is_empty() {
            bail!("expected_relay_peer_id is required");
        }
        let endpoint = parse_endpoint_v1(&config.endpoint)?;
        let tcp = TcpStream::connect_timeout(
            &endpoint.socket_addr,
            Duration::from_millis(config.connect_timeout_ms.max(1)),
        )
        .with_context(|| format!("connect relay endpoint: {}", config.endpoint))?;
        tcp.set_read_timeout(Some(Duration::from_millis(config.read_timeout_ms.max(1))))?;
        tcp.set_write_timeout(Some(Duration::from_millis(
            config.connect_timeout_ms.max(1),
        )))?;
        let tls_config = build_tls_config_v1(&config.tls_trust)?;
        let server_name = ServerName::try_from(endpoint.host.clone())
            .context("relay endpoint must use a DNS hostname")?;
        let connection = rustls::ClientConnection::new(tls_config, server_name)
            .context("create relay TLS client")?;
        let mut stream = rustls::StreamOwned::new(connection, tcp);
        websocket_upgrade_v1(&mut stream, &endpoint)?;
        let initiator = NodeHandshakeInitiatorV1::start(
            identity,
            config.expected_relay_peer_id.clone(),
            now_ms_v1(),
            30_000,
        )?;
        write_wire_v1(
            &mut stream,
            &ProductRelayWireMessageV1::HandshakeOffer(initiator.offer().clone()),
        )?;
        let response = match read_frame_v1(&mut stream)? {
            RelayClientFrameV1::Binary(bytes) => {
                match serde_json::from_slice(&bytes).context("decode relay handshake response")? {
                    ProductRelayWireMessageV1::HandshakeResponse(response) => response,
                    _ => bail!("relay did not return a handshake response"),
                }
            }
            _ => bail!("relay handshake response was not a binary frame"),
        };
        let mut replay = HandshakeReplayCacheV1::default();
        initiator
            .complete(&response, now_ms_v1(), &mut replay)
            .context("relay node identity challenge-response failed")?;
        Ok(Self {
            stream,
            session: ProductRelayClientSessionV1 {
                relay_peer_id: config.expected_relay_peer_id.clone(),
                endpoint: config.endpoint.clone(),
                websocket_path: endpoint.path,
                node_identity_challenge_response_verified: true,
                tls_is_novorudp_identity_root: false,
            },
        })
    }

    #[must_use]
    pub fn session(&self) -> &ProductRelayClientSessionV1 {
        &self.session
    }

    pub fn send_envelope(&mut self, envelope: SecureNovoRudpEnvelopeV1) -> Result<()> {
        write_wire_v1(&mut self.stream, &ProductRelayWireMessageV1::Data(envelope))
    }

    pub fn send_peer_handshake(
        &mut self,
        target_peer_id: impl Into<String>,
        handshake: RelayPeerHandshakeV1,
    ) -> Result<()> {
        write_wire_v1(
            &mut self.stream,
            &ProductRelayWireMessageV1::PeerHandshake {
                target_peer_id: target_peer_id.into(),
                handshake,
            },
        )
    }

    pub fn heartbeat(&mut self) -> Result<()> {
        write_wire_v1(&mut self.stream, &ProductRelayWireMessageV1::Heartbeat)
    }

    pub fn recv_event(&mut self) -> Result<ProductRelayClientEventV1> {
        loop {
            match read_frame_v1(&mut self.stream)? {
                RelayClientFrameV1::Binary(bytes) => {
                    match serde_json::from_slice(&bytes).context("decode relay event")? {
                        ProductRelayWireMessageV1::Delivery(delivery) => {
                            return Ok(ProductRelayClientEventV1::Delivery(delivery))
                        }
                        ProductRelayWireMessageV1::PeerHandshakeDelivery(delivery) => {
                            return Ok(ProductRelayClientEventV1::PeerHandshake(delivery))
                        }
                        ProductRelayWireMessageV1::HeartbeatAck => {
                            return Ok(ProductRelayClientEventV1::HeartbeatAck)
                        }
                        _ => bail!("unexpected relay event"),
                    }
                }
                RelayClientFrameV1::Ping(payload) => {
                    write_masked_frame_v1(&mut self.stream, 0xA, &payload)?
                }
                RelayClientFrameV1::Pong => {}
                RelayClientFrameV1::Close => return Ok(ProductRelayClientEventV1::Closed),
            }
        }
    }

    pub fn close(mut self) -> Result<()> {
        write_wire_v1(&mut self.stream, &ProductRelayWireMessageV1::Close)
    }
}

impl ProductRelayConnectorV1 {
    pub fn new(
        identity: SigningKey,
        config: ProductRelayClientConfigV1,
        policy: ProductRelayReconnectPolicyV1,
    ) -> Result<Self> {
        if policy.base_delay_ms == 0 || policy.max_delay_ms < policy.base_delay_ms {
            bail!("invalid product relay reconnect policy");
        }
        Ok(Self {
            identity,
            config,
            state: ProductRelayReconnectStateV1 {
                consecutive_failure_count: 0,
                next_delay_ms: 0,
                last_connect_succeeded: false,
            },
            policy,
        })
    }

    #[must_use]
    pub fn state(&self) -> &ProductRelayReconnectStateV1 {
        &self.state
    }

    pub fn connect(&mut self) -> Result<ProductRelayClientV1> {
        match ProductRelayClientV1::connect(&self.identity, &self.config) {
            Ok(client) => {
                self.state.consecutive_failure_count = 0;
                self.state.next_delay_ms = 0;
                self.state.last_connect_succeeded = true;
                Ok(client)
            }
            Err(error) => {
                self.state.consecutive_failure_count =
                    self.state.consecutive_failure_count.saturating_add(1);
                let exponent = self
                    .state
                    .consecutive_failure_count
                    .saturating_sub(1)
                    .min(16);
                self.state.next_delay_ms = self
                    .policy
                    .base_delay_ms
                    .saturating_mul(1u64 << exponent)
                    .min(self.policy.max_delay_ms);
                self.state.last_connect_succeeded = false;
                Err(error)
            }
        }
    }
}

#[derive(Debug)]
struct RelayEndpointV1 {
    host: String,
    socket_addr: SocketAddr,
    path: String,
}

#[derive(Debug)]
enum RelayClientFrameV1 {
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong,
    Close,
}

#[derive(Debug)]
struct NodeKeyBoundVerifierV1;

impl ServerCertVerifier for NodeKeyBoundVerifierV1 {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::aws_lc_rs::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn parse_endpoint_v1(endpoint: &str) -> Result<RelayEndpointV1> {
    let without_scheme = endpoint
        .strip_prefix("wss://")
        .context("relay endpoint must start with wss://")?;
    let mut split = without_scheme.splitn(2, '/');
    let authority = split.next().context("relay endpoint missing authority")?;
    let (host, port) = authority
        .rsplit_once(':')
        .context("relay endpoint must contain host:port")?;
    let port = port.parse::<u16>().context("parse relay endpoint port")?;
    let socket_addr = (host, port)
        .to_socket_addrs()
        .context("resolve relay endpoint")?
        .next()
        .context("relay endpoint has no address")?;
    Ok(RelayEndpointV1 {
        host: host.into(),
        socket_addr,
        path: format!("/{}", split.next().unwrap_or("novovm")),
    })
}

fn build_tls_config_v1(trust: &ProductRelayTlsTrustV1) -> Result<Arc<rustls::ClientConfig>> {
    let mut roots = rustls::RootCertStore::empty();
    match trust {
        ProductRelayTlsTrustV1::NativeWebPki => {
            let native = rustls_native_certs::load_native_certs();
            if !native.errors.is_empty() {
                bail!("load native TLS roots: {:?}", native.errors);
            }
            for certificate in native.certs {
                roots.add(certificate).context("add native TLS root")?;
            }
            Ok(Arc::new(
                rustls::ClientConfig::builder_with_provider(tls_crypto_provider_v1())
                    .with_safe_default_protocol_versions()
                    .context("select native relay TLS protocol versions")?
                    .with_root_certificates(roots)
                    .with_no_client_auth(),
            ))
        }
        ProductRelayTlsTrustV1::ExplicitCa { certificate_path } => {
            let bytes = std::fs::read(certificate_path).with_context(|| {
                format!("read explicit relay CA: {}", certificate_path.display())
            })?;
            let certificates = CertificateDer::pem_slice_iter(bytes.as_slice())
                .collect::<std::result::Result<Vec<_>, _>>()
                .context("parse explicit relay CA")?;
            for certificate in certificates {
                roots.add(certificate).context("add explicit relay CA")?;
            }
            Ok(Arc::new(
                rustls::ClientConfig::builder_with_provider(tls_crypto_provider_v1())
                    .with_safe_default_protocol_versions()
                    .context("select explicit relay TLS protocol versions")?
                    .with_root_certificates(roots)
                    .with_no_client_auth(),
            ))
        }
        ProductRelayTlsTrustV1::NodeKeyBoundEncrypted => Ok(Arc::new(
            rustls::ClientConfig::builder_with_provider(tls_crypto_provider_v1())
                .with_safe_default_protocol_versions()
                .context("select node-key relay TLS protocol versions")?
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NodeKeyBoundVerifierV1))
                .with_no_client_auth(),
        )),
    }
}

fn tls_crypto_provider_v1() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

fn websocket_upgrade_v1<S: Read + Write>(stream: &mut S, endpoint: &RelayEndpointV1) -> Result<()> {
    let key = BASE64_STANDARD.encode([17u8; 16]);
    write!(
        stream,
        "GET {} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {}\r\nSec-WebSocket-Version: 13\r\n\r\n",
        endpoint.path, endpoint.host, key
    )?;
    stream.flush()?;
    let response = read_http_headers_v1(stream)?;
    if !response.starts_with("HTTP/1.1 101") {
        bail!("relay rejected WebSocket upgrade");
    }
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let expected = BASE64_STANDARD.encode(hasher.finalize());
    if !response.contains(&format!("Sec-WebSocket-Accept: {expected}")) {
        bail!("relay WebSocket accept mismatch");
    }
    Ok(())
}

fn write_wire_v1<S: Write>(stream: &mut S, message: &ProductRelayWireMessageV1) -> Result<()> {
    write_masked_frame_v1(stream, 0x2, &serde_json::to_vec(message)?)
}

fn write_masked_frame_v1<S: Write>(stream: &mut S, opcode: u8, payload: &[u8]) -> Result<()> {
    let mask = [0x13, 0x37, 0x39, 0x41];
    let mut frame = Vec::with_capacity(payload.len() + 14);
    frame.push(0x80 | (opcode & 0x0f));
    match payload.len() {
        length if length <= 125 => frame.push(0x80 | length as u8),
        length if length <= u16::MAX as usize => {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(length as u16).to_be_bytes());
        }
        length => {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(length as u64).to_be_bytes());
        }
    }
    frame.extend_from_slice(&mask);
    frame.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4]),
    );
    stream.write_all(&frame)?;
    stream.flush()?;
    Ok(())
}

fn read_frame_v1<S: Read>(stream: &mut S) -> Result<RelayClientFrameV1> {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header)?;
    if header[0] & 0x80 == 0 {
        bail!("fragmented relay WebSocket frames are unsupported");
    }
    let opcode = header[0] & 0x0f;
    let masked = header[1] & 0x80 != 0;
    let mut length = (header[1] & 0x7f) as u64;
    if length == 126 {
        let mut extended = [0u8; 2];
        stream.read_exact(&mut extended)?;
        length = u16::from_be_bytes(extended) as u64;
    }
    if length == 127 {
        let mut extended = [0u8; 8];
        stream.read_exact(&mut extended)?;
        length = u64::from_be_bytes(extended);
    }
    if length > MAX_WEBSOCKET_FRAME_BYTES_V1 as u64 {
        bail!("relay WebSocket frame too large");
    }
    let mask = if masked {
        let mut mask = [0u8; 4];
        stream.read_exact(&mut mask)?;
        Some(mask)
    } else {
        None
    };
    let mut payload = vec![0u8; length as usize];
    stream.read_exact(&mut payload)?;
    if let Some(mask) = mask {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }
    }
    match opcode {
        0x2 => Ok(RelayClientFrameV1::Binary(payload)),
        0x9 => Ok(RelayClientFrameV1::Ping(payload)),
        0xA => Ok(RelayClientFrameV1::Pong),
        0x8 => Ok(RelayClientFrameV1::Close),
        _ => bail!("unsupported relay WebSocket opcode"),
    }
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
    bail!("relay WebSocket headers too large")
}

fn now_ms_v1() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn default_connect_timeout_ms_v1() -> u64 {
    10_000
}
fn default_read_timeout_ms_v1() -> u64 {
    30_000
}
fn default_tls_trust_v1() -> ProductRelayTlsTrustV1 {
    ProductRelayTlsTrustV1::NativeWebPki
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_relay_daemon::{run_product_relay_daemon_v1, ProductRelayDaemonConfigV1};
    use novovm_network::{
        peer_id_from_ed25519_public_key_v1, NodeHandshakeResponderV1, NovoRudpTransportFrameKindV0,
        NovoRudpTransportFrameV0,
    };
    use std::{fs, net::TcpListener, thread};

    #[test]
    fn formal_client_establishes_peer_e2e_session_through_relay() {
        let root =
            std::env::temp_dir().join(format!("novovm-product-relay-client-{}", now_ms_v1()));
        fs::create_dir_all(&root).unwrap();
        let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let certificate_path = root.join("relay-cert.pem");
        let key_path = root.join("relay-key.pem");
        let identity_path = root.join("relay-identity.hex");
        fs::write(&certificate_path, certificate.serialize_pem().unwrap()).unwrap();
        fs::write(&key_path, certificate.serialize_private_key_pem()).unwrap();
        fs::write(&identity_path, hex_v1(&[141; 32])).unwrap();
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let daemon = thread::spawn({
            let root = root.clone();
            move || {
                run_product_relay_daemon_v1(ProductRelayDaemonConfigV1 {
                    bind_addr: format!("127.0.0.1:{port}"),
                    tls_cert_path: certificate_path,
                    tls_key_path: key_path,
                    relay_identity_key_path: identity_path,
                    report_path: root.join("relay-report.json"),
                    report_interval_ms: 20,
                    run_for_ms: Some(1_800),
                    session_queue_capacity: Some(8),
                    offline_queue_per_peer: Some(8),
                    offline_queue_total: Some(16),
                    session_ttl_ms: Some(5_000),
                    rate_limit_frames: Some(100),
                    rate_limit_window_ms: Some(1_000),
                })
            }
        });
        let relay_identity = SigningKey::from_bytes(&[141; 32]);
        let config = ProductRelayClientConfigV1 {
            endpoint: format!("wss://127.0.0.1:{port}/novovm"),
            expected_relay_peer_id: peer_id_from_ed25519_public_key_v1(
                &relay_identity.verifying_key().to_bytes(),
            ),
            connect_timeout_ms: 2_000,
            read_timeout_ms: 2_000,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
        };
        let node_a = SigningKey::from_bytes(&[142; 32]);
        let node_b = SigningKey::from_bytes(&[143; 32]);
        let mut client_a = connect_retry_v1(&node_a, &config);
        let mut client_b = ProductRelayClientV1::connect(&node_b, &config).unwrap();
        assert!(client_a.session().node_identity_challenge_response_verified);
        let node_a_peer_id = peer_id_from_ed25519_public_key_v1(&node_a.verifying_key().to_bytes());
        let node_b_peer_id = peer_id_from_ed25519_public_key_v1(&node_b.verifying_key().to_bytes());
        let peer_initiator =
            NodeHandshakeInitiatorV1::start(&node_a, node_b_peer_id.clone(), now_ms_v1(), 5_000)
                .unwrap();
        client_a
            .send_peer_handshake(
                node_b_peer_id.clone(),
                RelayPeerHandshakeV1::Offer(peer_initiator.offer().clone()),
            )
            .unwrap();
        let relay_offer = match client_b.recv_event().unwrap() {
            ProductRelayClientEventV1::PeerHandshake(delivery) => delivery,
            other => panic!("unexpected client B event: {other:?}"),
        };
        let RelayPeerHandshakeV1::Offer(offer) = relay_offer.handshake else {
            panic!("expected peer offer");
        };
        let mut replay = HandshakeReplayCacheV1::default();
        let responder =
            NodeHandshakeResponderV1::respond(&offer, &node_b, now_ms_v1(), 5_000, &mut replay)
                .unwrap();
        let response = responder.response().clone();
        let mut channel_b = responder.into_channel();
        client_b
            .send_peer_handshake(node_a_peer_id, RelayPeerHandshakeV1::Response(response))
            .unwrap();
        let relay_response = match client_a.recv_event().unwrap() {
            ProductRelayClientEventV1::PeerHandshake(delivery) => delivery,
            other => panic!("unexpected client A event: {other:?}"),
        };
        let RelayPeerHandshakeV1::Response(response) = relay_response.handshake else {
            panic!("expected peer response");
        };
        let mut response_replay = HandshakeReplayCacheV1::default();
        let mut channel_a = peer_initiator
            .complete(&response, now_ms_v1(), &mut response_replay)
            .unwrap();
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            [144; 16],
            1,
            2,
            3,
            4,
            b"formal relay client".to_vec(),
        );
        client_a
            .send_envelope(channel_a.seal_novorudp_frame(&frame).unwrap())
            .unwrap();
        let delivery = match client_b.recv_event().unwrap() {
            ProductRelayClientEventV1::Delivery(delivery) => delivery,
            other => panic!("unexpected client B delivery event: {other:?}"),
        };
        assert_eq!(
            channel_b.open_novorudp_frame(&delivery.envelope).unwrap(),
            frame
        );
        drop(client_a);
        drop(client_b);
        daemon.join().unwrap().unwrap();
        let _ = fs::remove_dir_all(root);
    }

    fn connect_retry_v1(
        identity: &SigningKey,
        config: &ProductRelayClientConfigV1,
    ) -> ProductRelayClientV1 {
        for _ in 0..50 {
            if let Ok(client) = ProductRelayClientV1::connect(identity, config) {
                return client;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("relay client could not connect");
    }

    #[test]
    fn reconnect_state_is_bounded_after_failures() {
        let identity = SigningKey::from_bytes(&[145; 32]);
        let config = ProductRelayClientConfigV1 {
            endpoint: "wss://127.0.0.1:9/novovm".into(),
            expected_relay_peer_id: "novovm-ed25519:unreachable".into(),
            connect_timeout_ms: 1,
            read_timeout_ms: 1,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
        };
        let mut connector = ProductRelayConnectorV1::new(
            identity,
            config,
            ProductRelayReconnectPolicyV1 {
                base_delay_ms: 100,
                max_delay_ms: 250,
            },
        )
        .unwrap();
        assert!(connector.connect().is_err());
        assert_eq!(connector.state().next_delay_ms, 100);
        assert!(connector.connect().is_err());
        assert_eq!(connector.state().next_delay_ms, 200);
        assert!(connector.connect().is_err());
        assert_eq!(connector.state().next_delay_ms, 250);
    }

    fn hex_v1(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
