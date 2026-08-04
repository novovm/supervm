//! Node-side WSS relay client for the product relay protocol.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::SigningKey;
use novovm_network::{
    HandshakeReplayCacheV1, NodeHandshakeInitiatorV1, OpaqueRelayDeliveryV1,
    ProductRelayWireMessageV1, RelayForwardDispositionV1, RelayForwardOutcomeV1,
    RelayPeerHandshakeDeliveryV1, RelayPeerHandshakeV1, SecureNovoRudpEnvelopeV1,
    PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1,
};
use rand::{rngs::OsRng, RngCore};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{pem::PemObject, CertificateDer, ServerName, UnixTime},
    DigitallySignedStruct, SignatureScheme,
};
use serde::{Deserialize, Serialize};
use sha1::{Digest as Sha1Digest, Sha1};
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

const MAX_WEBSOCKET_CONTROL_FRAME_BYTES_V1: usize = 125;
const PRODUCT_RELAY_FRAME_DEADLINE_MS_V1: u64 = 10_000;
const PRODUCT_RELAY_PROTOCOL_ITEM_DEADLINE_MS_V1: u64 = 10_000;
const PRODUCT_RELAY_MAX_CONTROL_FRAMES_PER_PROTOCOL_ITEM_V1: usize = 64;
pub(crate) const PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_EVENTS_V1: usize = 64;
pub(crate) const PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_BYTES_V1: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductRelayTlsTrustV1 {
    NativeWebPki,
    ExplicitCa {
        certificate_path: PathBuf,
    },
    /// Test-only transport encryption for a relay resolved to loopback. The post-upgrade signed
    /// node handshake does not bind subsequent relay wire messages to that identity.
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

#[derive(Debug)]
enum ProductRelayClientProtocolItemV1 {
    Event {
        event: Box<ProductRelayClientEventV1>,
        wire_bytes: usize,
    },
    ForwardOutcome(RelayForwardOutcomeV1),
}

#[derive(Debug)]
struct ProductRelayPendingEventV1 {
    event: ProductRelayClientEventV1,
    wire_bytes: usize,
}

pub struct ProductRelayClientV1 {
    stream: rustls::StreamOwned<rustls::ClientConnection, ProductRelayDeadlineTcpStreamV1>,
    session: ProductRelayClientSessionV1,
    read_buffer: Vec<u8>,
    read_buffer_offset: usize,
    pending_events: VecDeque<ProductRelayPendingEventV1>,
    pending_event_bytes: usize,
}

#[derive(Debug)]
struct ProductRelayDeadlineTcpStreamV1 {
    inner: TcpStream,
    handshake_deadline: Option<Instant>,
    frame_deadline: Option<Instant>,
}

impl ProductRelayDeadlineTcpStreamV1 {
    fn new(inner: TcpStream, handshake_deadline: Instant) -> Self {
        Self {
            inner,
            handshake_deadline: Some(handshake_deadline),
            frame_deadline: None,
        }
    }

    fn finish_handshake_v1(
        &mut self,
        read_timeout: Duration,
        write_timeout: Duration,
    ) -> io::Result<()> {
        self.check_io_deadlines_v1()?;
        self.inner.set_read_timeout(Some(read_timeout))?;
        self.inner.set_write_timeout(Some(write_timeout))?;
        self.handshake_deadline = None;
        self.frame_deadline = None;
        Ok(())
    }

    fn finish_frame_v1(&mut self, buffered_next_frame: bool) -> io::Result<()> {
        self.check_io_deadlines_v1()?;
        self.frame_deadline = if buffered_next_frame {
            Some(self.new_frame_deadline_v1()?)
        } else {
            None
        };
        Ok(())
    }

    fn ensure_frame_deadline_v1(&mut self) -> io::Result<()> {
        if self.handshake_deadline.is_none() && self.frame_deadline.is_none() {
            self.frame_deadline = Some(self.new_frame_deadline_v1()?);
        }
        self.check_io_deadlines_v1()
    }

    fn ensure_frame_deadline_until_v1(&mut self, operation_deadline: Instant) -> io::Result<()> {
        if self.handshake_deadline.is_none() {
            let frame_deadline = match self.frame_deadline {
                Some(frame_deadline) => frame_deadline.min(operation_deadline),
                None => self.new_frame_deadline_v1()?.min(operation_deadline),
            };
            self.frame_deadline = Some(frame_deadline);
        }
        self.check_io_deadlines_v1()
    }

    fn begin_authenticated_write_v1(&mut self) -> io::Result<()> {
        if self.handshake_deadline.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "product relay client authenticated write began during handshake",
            ));
        }
        self.frame_deadline = Some(self.new_frame_deadline_v1()?);
        self.check_io_deadlines_v1()
    }

    fn finish_authenticated_write_v1(&mut self) -> io::Result<()> {
        self.check_io_deadlines_v1()?;
        self.frame_deadline = None;
        Ok(())
    }

    fn new_frame_deadline_v1(&self) -> io::Result<Instant> {
        Instant::now()
            .checked_add(Duration::from_millis(PRODUCT_RELAY_FRAME_DEADLINE_MS_V1))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "product relay client frame deadline overflow",
                )
            })
    }

    fn check_io_deadlines_v1(&self) -> io::Result<()> {
        if self
            .handshake_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "product relay client absolute handshake deadline exceeded",
            ));
        }
        if self
            .frame_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "product relay client absolute frame deadline exceeded",
            ));
        }
        Ok(())
    }
}

impl Read for ProductRelayDeadlineTcpStreamV1 {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.check_io_deadlines_v1()?;
        let result = self.inner.read(output);
        if result.as_ref().is_ok_and(|read| *read > 0) {
            self.ensure_frame_deadline_v1()?;
        }
        self.check_io_deadlines_v1()?;
        result
    }
}

impl Write for ProductRelayDeadlineTcpStreamV1 {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        self.check_io_deadlines_v1()?;
        let result = self.inner.write(input);
        self.check_io_deadlines_v1()?;
        result
    }

    fn flush(&mut self) -> io::Result<()> {
        self.check_io_deadlines_v1()?;
        let result = self.inner.flush();
        self.check_io_deadlines_v1()?;
        result
    }
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
        let tls_config = build_tls_config_v1(&config.tls_trust, endpoint.socket_addr.ip())?;
        let connect_timeout = Duration::from_millis(config.connect_timeout_ms.max(1));
        let read_timeout = Duration::from_millis(config.read_timeout_ms.max(1));
        let handshake_deadline = Instant::now()
            .checked_add(connect_timeout)
            .context("product relay client handshake deadline overflow")?;
        let tcp = TcpStream::connect_timeout(&endpoint.socket_addr, connect_timeout)
            .with_context(|| format!("connect relay endpoint: {}", config.endpoint))?;
        let handshake_io_timeout = read_timeout.min(connect_timeout);
        tcp.set_read_timeout(Some(handshake_io_timeout))?;
        tcp.set_write_timeout(Some(connect_timeout))?;
        let server_name = ServerName::try_from(endpoint.host.clone())
            .context("relay endpoint must use a DNS hostname")?;
        let connection = rustls::ClientConnection::new(tls_config, server_name)
            .context("create relay TLS client")?;
        let mut stream = rustls::StreamOwned::new(
            connection,
            ProductRelayDeadlineTcpStreamV1::new(tcp, handshake_deadline),
        );
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
        stream
            .sock
            .finish_handshake_v1(read_timeout, connect_timeout)
            .context("finish product relay client handshake deadline")?;
        Ok(Self {
            stream,
            session: ProductRelayClientSessionV1 {
                relay_peer_id: config.expected_relay_peer_id.clone(),
                endpoint: config.endpoint.clone(),
                websocket_path: endpoint.path,
                node_identity_challenge_response_verified: true,
                tls_is_novorudp_identity_root: false,
            },
            read_buffer: Vec::new(),
            read_buffer_offset: 0,
            pending_events: VecDeque::new(),
            pending_event_bytes: 0,
        })
    }

    #[must_use]
    pub fn session(&self) -> &ProductRelayClientSessionV1 {
        &self.session
    }

    pub fn send_envelope(&mut self, envelope: SecureNovoRudpEnvelopeV1) -> Result<()> {
        let outcome = self.send_envelope_with_outcome_v1(envelope)?;
        ensure_forward_outcome_accepted_v1(&outcome)
    }

    pub fn send_envelope_with_outcome_v1(
        &mut self,
        envelope: SecureNovoRudpEnvelopeV1,
    ) -> Result<RelayForwardOutcomeV1> {
        let source_peer_id = envelope.sender_peer_id.clone();
        let target_peer_id = envelope.recipient_peer_id.clone();
        let envelope_session_id = envelope.session_id;
        let envelope_sequence = envelope.sequence;
        let admitted_wire_bytes =
            self.write_authenticated_wire_v1(&ProductRelayWireMessageV1::Data(envelope))?;
        self.wait_for_forward_outcome_v1(
            &source_peer_id,
            &target_peer_id,
            Some(envelope_session_id),
            Some(envelope_sequence),
            admitted_wire_bytes,
        )
    }

    pub fn send_peer_handshake(
        &mut self,
        target_peer_id: impl Into<String>,
        handshake: RelayPeerHandshakeV1,
    ) -> Result<()> {
        let outcome = self.send_peer_handshake_with_outcome_v1(target_peer_id, handshake)?;
        ensure_forward_outcome_accepted_v1(&outcome)
    }

    pub fn send_peer_handshake_with_outcome_v1(
        &mut self,
        target_peer_id: impl Into<String>,
        handshake: RelayPeerHandshakeV1,
    ) -> Result<RelayForwardOutcomeV1> {
        let target_peer_id = target_peer_id.into();
        let source_peer_id = match &handshake {
            RelayPeerHandshakeV1::Offer(offer) => offer.initiator_peer_id.clone(),
            RelayPeerHandshakeV1::Response(response) => response.responder_peer_id.clone(),
        };
        let admitted_wire_bytes =
            self.write_authenticated_wire_v1(&ProductRelayWireMessageV1::PeerHandshake {
                target_peer_id: target_peer_id.clone(),
                handshake,
            })?;
        self.wait_for_forward_outcome_v1(
            &source_peer_id,
            &target_peer_id,
            None,
            None,
            admitted_wire_bytes,
        )
    }

    pub fn heartbeat(&mut self) -> Result<()> {
        self.write_authenticated_wire_v1(&ProductRelayWireMessageV1::Heartbeat)
            .map(|_| ())
    }

    pub fn recv_event(&mut self) -> Result<ProductRelayClientEventV1> {
        if let Some(event) =
            pop_pending_relay_event_v1(&mut self.pending_events, &mut self.pending_event_bytes)
        {
            return Ok(event);
        }
        match self.read_protocol_item_v1()? {
            ProductRelayClientProtocolItemV1::Event { event, .. } => Ok(*event),
            ProductRelayClientProtocolItemV1::ForwardOutcome(_) => {
                bail!("relay returned an unsolicited forward outcome")
            }
        }
    }

    pub fn close(mut self) -> Result<()> {
        self.write_authenticated_wire_v1(&ProductRelayWireMessageV1::Close)
            .map(|_| ())
    }

    fn wait_for_forward_outcome_v1(
        &mut self,
        expected_source_peer_id: &str,
        expected_target_peer_id: &str,
        expected_envelope_session_id: Option<[u8; 16]>,
        expected_envelope_sequence: Option<u64>,
        expected_admitted_wire_bytes: usize,
    ) -> Result<RelayForwardOutcomeV1> {
        let outcome_deadline = Instant::now()
            .checked_add(Duration::from_millis(
                PRODUCT_RELAY_PROTOCOL_ITEM_DEADLINE_MS_V1,
            ))
            .context("product relay forward-outcome deadline overflow")?;
        loop {
            match self.read_protocol_item_until_v1(outcome_deadline)? {
                ProductRelayClientProtocolItemV1::Event { event, .. }
                    if matches!(event.as_ref(), ProductRelayClientEventV1::Closed) =>
                {
                    bail!("relay closed before returning a forward outcome")
                }
                ProductRelayClientProtocolItemV1::Event { event, wire_bytes } => {
                    push_pending_relay_event_v1(
                        &mut self.pending_events,
                        &mut self.pending_event_bytes,
                        *event,
                        wire_bytes,
                    )?;
                }
                ProductRelayClientProtocolItemV1::ForwardOutcome(outcome) => {
                    if outcome.source_peer_id != expected_source_peer_id
                        || outcome.target_peer_id != expected_target_peer_id
                        || outcome.envelope_session_id != expected_envelope_session_id
                        || outcome.envelope_sequence != expected_envelope_sequence
                        || outcome.admitted_wire_bytes != expected_admitted_wire_bytes
                        || !outcome.payload_treated_opaque
                    {
                        bail!("relay forward outcome correlation mismatch");
                    }
                    let flags_match_disposition = match outcome.disposition {
                        RelayForwardDispositionV1::Forwarded => {
                            outcome.forwarded && !outcome.queued
                        }
                        RelayForwardDispositionV1::QueuedTargetOffline
                        | RelayForwardDispositionV1::QueuedBackpressure => {
                            !outcome.forwarded && outcome.queued
                        }
                        _ => !outcome.forwarded && !outcome.queued,
                    };
                    if !flags_match_disposition {
                        bail!("relay forward outcome flags contradict its disposition");
                    }
                    return Ok(outcome);
                }
            }
        }
    }

    fn read_protocol_item_v1(&mut self) -> Result<ProductRelayClientProtocolItemV1> {
        let protocol_item_deadline = Instant::now()
            .checked_add(Duration::from_millis(
                PRODUCT_RELAY_PROTOCOL_ITEM_DEADLINE_MS_V1,
            ))
            .context("product relay protocol-item deadline overflow")?;
        self.read_protocol_item_until_v1(protocol_item_deadline)
    }

    fn read_protocol_item_until_v1(
        &mut self,
        protocol_item_deadline: Instant,
    ) -> Result<ProductRelayClientProtocolItemV1> {
        let mut control_frame_count = 0usize;
        loop {
            ensure_protocol_item_progress_v1(protocol_item_deadline, control_frame_count)?;
            match read_buffered_frame_v1(
                &mut self.stream,
                &mut self.read_buffer,
                &mut self.read_buffer_offset,
                protocol_item_deadline,
            )? {
                RelayClientFrameV1::Binary(bytes) => {
                    let wire_bytes = bytes.len();
                    let message = serde_json::from_slice(&bytes).context("decode relay event")?;
                    return match message {
                        ProductRelayWireMessageV1::Delivery(delivery) => {
                            Ok(ProductRelayClientProtocolItemV1::Event {
                                event: Box::new(ProductRelayClientEventV1::Delivery(delivery)),
                                wire_bytes,
                            })
                        }
                        ProductRelayWireMessageV1::PeerHandshakeDelivery(delivery) => {
                            Ok(ProductRelayClientProtocolItemV1::Event {
                                event: Box::new(ProductRelayClientEventV1::PeerHandshake(delivery)),
                                wire_bytes,
                            })
                        }
                        ProductRelayWireMessageV1::HeartbeatAck => {
                            Ok(ProductRelayClientProtocolItemV1::Event {
                                event: Box::new(ProductRelayClientEventV1::HeartbeatAck),
                                wire_bytes,
                            })
                        }
                        ProductRelayWireMessageV1::ForwardOutcome(outcome) => {
                            Ok(ProductRelayClientProtocolItemV1::ForwardOutcome(outcome))
                        }
                        _ => bail!("unexpected relay event"),
                    };
                }
                RelayClientFrameV1::Ping(payload) => {
                    control_frame_count = control_frame_count.saturating_add(1);
                    ensure_protocol_item_progress_v1(protocol_item_deadline, control_frame_count)?;
                    self.write_authenticated_frame_v1(0xA, &payload)?;
                }
                RelayClientFrameV1::Pong => {
                    control_frame_count = control_frame_count.saturating_add(1);
                    ensure_protocol_item_progress_v1(protocol_item_deadline, control_frame_count)?;
                }
                RelayClientFrameV1::Close => {
                    return Ok(ProductRelayClientProtocolItemV1::Event {
                        event: Box::new(ProductRelayClientEventV1::Closed),
                        wire_bytes: 0,
                    })
                }
            }
        }
    }

    fn write_authenticated_wire_v1(
        &mut self,
        message: &ProductRelayWireMessageV1,
    ) -> Result<usize> {
        self.stream.sock.begin_authenticated_write_v1()?;
        let write_result = write_wire_v1(&mut self.stream, message);
        let finish_result = self.stream.sock.finish_authenticated_write_v1();
        let written = write_result?;
        finish_result?;
        Ok(written)
    }

    fn write_authenticated_frame_v1(&mut self, opcode: u8, payload: &[u8]) -> Result<()> {
        self.stream.sock.begin_authenticated_write_v1()?;
        let write_result = write_masked_frame_v1(&mut self.stream, opcode, payload);
        let finish_result = self.stream.sock.finish_authenticated_write_v1();
        write_result?;
        finish_result?;
        Ok(())
    }
}

fn ensure_protocol_item_progress_v1(deadline: Instant, control_frame_count: usize) -> Result<()> {
    if Instant::now() >= deadline {
        bail!("product relay protocol-item absolute deadline exceeded");
    }
    if control_frame_count > PRODUCT_RELAY_MAX_CONTROL_FRAMES_PER_PROTOCOL_ITEM_V1 {
        bail!("product relay protocol-item control-frame budget exceeded");
    }
    Ok(())
}

fn ensure_forward_outcome_accepted_v1(outcome: &RelayForwardOutcomeV1) -> Result<()> {
    match outcome.disposition {
        RelayForwardDispositionV1::Forwarded
        | RelayForwardDispositionV1::QueuedTargetOffline
        | RelayForwardDispositionV1::QueuedBackpressure => Ok(()),
        disposition => bail!("relay rejected forward admission: {disposition:?}"),
    }
}

fn push_pending_relay_event_v1(
    pending_events: &mut VecDeque<ProductRelayPendingEventV1>,
    pending_event_bytes: &mut usize,
    event: ProductRelayClientEventV1,
    wire_bytes: usize,
) -> Result<()> {
    let next_bytes = pending_event_bytes
        .checked_add(wire_bytes)
        .context("relay pending event byte accounting overflow")?;
    if pending_events.len() >= PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_EVENTS_V1
        || next_bytes > PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_BYTES_V1
    {
        bail!(
            "relay event buffer reached its bounded count or byte capacity while awaiting a forward outcome"
        );
    }
    pending_events.push_back(ProductRelayPendingEventV1 { event, wire_bytes });
    *pending_event_bytes = next_bytes;
    Ok(())
}

fn pop_pending_relay_event_v1(
    pending_events: &mut VecDeque<ProductRelayPendingEventV1>,
    pending_event_bytes: &mut usize,
) -> Option<ProductRelayClientEventV1> {
    let pending = pending_events.pop_front()?;
    *pending_event_bytes = (*pending_event_bytes).saturating_sub(pending.wire_bytes);
    Some(pending.event)
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

fn build_tls_config_v1(
    trust: &ProductRelayTlsTrustV1,
    resolved_ip: std::net::IpAddr,
) -> Result<Arc<rustls::ClientConfig>> {
    validate_tls_trust_endpoint_v1(trust, resolved_ip)?;
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

fn validate_tls_trust_endpoint_v1(
    trust: &ProductRelayTlsTrustV1,
    resolved_ip: std::net::IpAddr,
) -> Result<()> {
    if matches!(trust, ProductRelayTlsTrustV1::NodeKeyBoundEncrypted) && !resolved_ip.is_loopback()
    {
        bail!(
            "node_key_bound_encrypted is restricted to loopback relay endpoints; non-loopback relay {resolved_ip} must use native_web_pki or explicit_ca"
        );
    }
    Ok(())
}

fn tls_crypto_provider_v1() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

fn websocket_upgrade_v1<S: Read + Write>(stream: &mut S, endpoint: &RelayEndpointV1) -> Result<()> {
    let key = fresh_websocket_key_v1();
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

fn fresh_websocket_key_v1() -> String {
    let mut nonce = [0u8; 16];
    OsRng.fill_bytes(&mut nonce);
    BASE64_STANDARD.encode(nonce)
}

fn write_wire_v1<S: Write>(stream: &mut S, message: &ProductRelayWireMessageV1) -> Result<usize> {
    let payload = serde_json::to_vec(message)?;
    write_masked_frame_v1(stream, 0x2, &payload)?;
    Ok(payload.len())
}

fn write_masked_frame_v1<S: Write>(stream: &mut S, opcode: u8, payload: &[u8]) -> Result<()> {
    validate_websocket_payload_size_v1(opcode, payload.len())?;
    let mut mask = [0u8; 4];
    OsRng.fill_bytes(&mut mask);
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

fn read_buffered_frame_v1(
    stream: &mut rustls::StreamOwned<rustls::ClientConnection, ProductRelayDeadlineTcpStreamV1>,
    read_buffer: &mut Vec<u8>,
    read_buffer_offset: &mut usize,
    operation_deadline: Instant,
) -> Result<RelayClientFrameV1> {
    loop {
        stream
            .sock
            .ensure_frame_deadline_until_v1(operation_deadline)?;
        let unread = read_buffer
            .get(*read_buffer_offset..)
            .context("relay read buffer cursor is out of bounds")?;
        if let Some((frame, consumed)) = decode_buffered_frame_v1(unread)? {
            *read_buffer_offset = read_buffer_offset
                .checked_add(consumed)
                .context("relay read buffer cursor overflow")?;
            let buffered_next_frame = *read_buffer_offset < read_buffer.len();
            if !buffered_next_frame {
                read_buffer.clear();
                *read_buffer_offset = 0;
            } else if *read_buffer_offset >= 16 * 1024
                && *read_buffer_offset >= read_buffer.len().saturating_div(2)
            {
                read_buffer.drain(..*read_buffer_offset);
                *read_buffer_offset = 0;
            }
            stream.sock.finish_frame_v1(buffered_next_frame)?;
            return Ok(frame);
        }
        let mut chunk = [0u8; 16 * 1024];
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            bail!("relay WebSocket stream closed");
        }
        read_buffer.extend_from_slice(&chunk[..read]);
        stream.sock.ensure_frame_deadline_v1()?;
    }
}

fn decode_buffered_frame_v1(bytes: &[u8]) -> Result<Option<(RelayClientFrameV1, usize)>> {
    if bytes.len() < 2 {
        return Ok(None);
    }
    if bytes[0] & 0x80 == 0 {
        bail!("fragmented relay WebSocket frames are unsupported");
    }
    if bytes[0] & 0x70 != 0 {
        bail!("relay WebSocket RSV bits are unsupported");
    }
    let opcode = bytes[0] & 0x0f;
    let masked = bytes[1] & 0x80 != 0;
    if masked {
        bail!("relay server WebSocket frames must not be masked");
    }
    let length_tag = bytes[1] & 0x7f;
    let mut cursor = 2usize;
    let length = match length_tag {
        126 => {
            if bytes.len() < cursor + 2 {
                return Ok(None);
            }
            let length = u16::from_be_bytes(bytes[cursor..cursor + 2].try_into().unwrap()) as u64;
            cursor += 2;
            length
        }
        127 => {
            if bytes.len() < cursor + 8 {
                return Ok(None);
            }
            let length = u64::from_be_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
            cursor += 8;
            length
        }
        length => u64::from(length),
    };
    if length > PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 as u64 {
        bail!("relay WebSocket frame too large");
    }
    validate_websocket_payload_size_v1(opcode, length as usize)?;
    let mask = if masked {
        if bytes.len() < cursor + 4 {
            return Ok(None);
        }
        let mask: [u8; 4] = bytes[cursor..cursor + 4].try_into().unwrap();
        cursor += 4;
        Some(mask)
    } else {
        None
    };
    let payload_len = usize::try_from(length).context("relay frame length exceeds usize")?;
    let frame_len = cursor
        .checked_add(payload_len)
        .context("relay frame length overflow")?;
    if bytes.len() < frame_len {
        return Ok(None);
    }
    let mut payload = bytes[cursor..frame_len].to_vec();
    if let Some(mask) = mask {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }
    }
    let frame = match opcode {
        0x2 => RelayClientFrameV1::Binary(payload),
        0x9 => RelayClientFrameV1::Ping(payload),
        0xA => RelayClientFrameV1::Pong,
        0x8 => RelayClientFrameV1::Close,
        _ => bail!("unsupported relay WebSocket opcode"),
    };
    Ok(Some((frame, frame_len)))
}

fn read_frame_v1<S: Read>(stream: &mut S) -> Result<RelayClientFrameV1> {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header)?;
    if header[0] & 0x80 == 0 {
        bail!("fragmented relay WebSocket frames are unsupported");
    }
    if header[0] & 0x70 != 0 {
        bail!("relay WebSocket RSV bits are unsupported");
    }
    let opcode = header[0] & 0x0f;
    let masked = header[1] & 0x80 != 0;
    if masked {
        bail!("relay server WebSocket frames must not be masked");
    }
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
    if length > PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 as u64 {
        bail!("relay WebSocket frame too large");
    }
    validate_websocket_payload_size_v1(opcode, length as usize)?;
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

fn validate_websocket_payload_size_v1(opcode: u8, payload_len: usize) -> Result<()> {
    if payload_len > PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 {
        bail!("relay WebSocket frame too large");
    }
    if opcode & 0x08 != 0 && payload_len > MAX_WEBSOCKET_CONTROL_FRAME_BYTES_V1 {
        bail!("relay WebSocket control frame exceeds 125 bytes");
    }
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
    fn client_websocket_bounds_writes_and_rejects_masked_server_frames() {
        let first_key = fresh_websocket_key_v1();
        let second_key = fresh_websocket_key_v1();
        assert_ne!(first_key, second_key);
        assert_eq!(BASE64_STANDARD.decode(first_key).unwrap().len(), 16);

        let mut wire = Vec::new();
        let oversized = vec![0u8; PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 + 1];
        assert!(write_masked_frame_v1(&mut wire, 0x2, &oversized)
            .unwrap_err()
            .to_string()
            .contains("too large"));
        assert!(write_masked_frame_v1(
            &mut wire,
            0x9,
            &[0u8; MAX_WEBSOCKET_CONTROL_FRAME_BYTES_V1 + 1],
        )
        .unwrap_err()
        .to_string()
        .contains("control frame"));

        let mut masked_server_frame = Vec::new();
        write_masked_frame_v1(&mut masked_server_frame, 0x2, b"server").unwrap();
        assert!(decode_buffered_frame_v1(&masked_server_frame)
            .unwrap_err()
            .to_string()
            .contains("must not be masked"));
    }

    #[test]
    fn node_key_bound_tls_is_loopback_only_before_connect() {
        let identity = SigningKey::from_bytes(&[140; 32]);
        let config = ProductRelayClientConfigV1 {
            endpoint: "wss://192.0.2.1:443/novovm".into(),
            expected_relay_peer_id: "novovm-ed25519:test-relay".into(),
            connect_timeout_ms: 60_000,
            read_timeout_ms: 1_000,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
        };
        let error = ProductRelayClientV1::connect(&identity, &config)
            .err()
            .expect("non-loopback node-key-only TLS must fail before TCP connect");
        let message = error.to_string();
        assert!(message.contains("restricted to loopback"));
        assert!(message.contains("native_web_pki"));
        assert!(message.contains("explicit_ca"));

        let trust = ProductRelayTlsTrustV1::NodeKeyBoundEncrypted;
        assert!(build_tls_config_v1(&trust, "127.0.0.1".parse().unwrap()).is_ok());
        assert!(build_tls_config_v1(&trust, "::1".parse().unwrap()).is_ok());
        assert!(validate_tls_trust_endpoint_v1(
            &ProductRelayTlsTrustV1::NativeWebPki,
            "192.0.2.1".parse().unwrap(),
        )
        .is_ok());
        assert!(validate_tls_trust_endpoint_v1(
            &ProductRelayTlsTrustV1::ExplicitCa {
                certificate_path: "unused-in-validation.pem".into(),
            },
            "192.0.2.1".parse().unwrap(),
        )
        .is_ok());
    }

    #[test]
    fn client_handshake_deadline_is_not_reset_by_byte_progress() {
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
        let mut guarded =
            ProductRelayDeadlineTcpStreamV1::new(tcp, Instant::now() + Duration::from_millis(35));
        let mut bytes = [0u8; 8];
        let error = guarded.read_exact(&mut bytes).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(error.to_string().contains("absolute handshake deadline"));
        drop(guarded);
        server.join().unwrap();
    }

    #[test]
    fn protocol_item_control_frames_and_wall_clock_are_bounded() {
        let future = Instant::now() + Duration::from_secs(1);
        ensure_protocol_item_progress_v1(
            future,
            PRODUCT_RELAY_MAX_CONTROL_FRAMES_PER_PROTOCOL_ITEM_V1,
        )
        .unwrap();
        assert!(ensure_protocol_item_progress_v1(
            future,
            PRODUCT_RELAY_MAX_CONTROL_FRAMES_PER_PROTOCOL_ITEM_V1 + 1,
        )
        .unwrap_err()
        .to_string()
        .contains("control-frame budget"));
        assert!(ensure_protocol_item_progress_v1(Instant::now(), 0)
            .unwrap_err()
            .to_string()
            .contains("absolute deadline"));
    }

    #[test]
    fn forward_wait_event_buffer_enforces_count_and_wire_byte_caps() {
        let mut pending = VecDeque::new();
        let mut pending_bytes = 0usize;
        for _ in 0..PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_EVENTS_V1 {
            push_pending_relay_event_v1(
                &mut pending,
                &mut pending_bytes,
                ProductRelayClientEventV1::HeartbeatAck,
                1,
            )
            .unwrap();
        }
        assert!(push_pending_relay_event_v1(
            &mut pending,
            &mut pending_bytes,
            ProductRelayClientEventV1::HeartbeatAck,
            1,
        )
        .is_err());
        while pop_pending_relay_event_v1(&mut pending, &mut pending_bytes).is_some() {}
        assert_eq!(pending_bytes, 0);

        assert!(push_pending_relay_event_v1(
            &mut pending,
            &mut pending_bytes,
            ProductRelayClientEventV1::HeartbeatAck,
            PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_BYTES_V1 + 1,
        )
        .is_err());
        push_pending_relay_event_v1(
            &mut pending,
            &mut pending_bytes,
            ProductRelayClientEventV1::HeartbeatAck,
            PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_BYTES_V1,
        )
        .unwrap();
        assert!(push_pending_relay_event_v1(
            &mut pending,
            &mut pending_bytes,
            ProductRelayClientEventV1::HeartbeatAck,
            1,
        )
        .is_err());
        assert_eq!(
            pending_bytes,
            PRODUCT_RELAY_CLIENT_FORWARD_OUTCOME_PENDING_BYTES_V1
        );
    }

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
                    run_for_ms: Some(3_000),
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
        let reverse_frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            [145; 16],
            5,
            6,
            7,
            8,
            b"event buffered while awaiting relay acceptance".to_vec(),
        );
        client_b
            .send_envelope(channel_b.seal_novorudp_frame(&reverse_frame).unwrap())
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
        let reverse_delivery = match client_a.recv_event().unwrap() {
            ProductRelayClientEventV1::Delivery(delivery) => delivery,
            other => panic!("unexpected buffered client A delivery event: {other:?}"),
        };
        assert_eq!(
            channel_a
                .open_novorudp_frame(&reverse_delivery.envelope)
                .unwrap(),
            reverse_frame
        );
        let mut replacement_a = ProductRelayClientV1::connect(&node_a, &config).unwrap();
        let _ = client_a.heartbeat();
        assert!(matches!(
            client_a.recv_event(),
            Err(_) | Ok(ProductRelayClientEventV1::Closed)
        ));
        replacement_a.heartbeat().unwrap();
        assert_eq!(
            replacement_a.recv_event().unwrap(),
            ProductRelayClientEventV1::HeartbeatAck
        );
        drop(replacement_a);
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

    #[test]
    fn buffered_decoder_preserves_partial_websocket_frame_across_reads() {
        let payload = vec![0x5a; 300];
        let mut encoded = vec![0x82, 126];
        encoded.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        encoded.extend_from_slice(&payload);

        assert!(decode_buffered_frame_v1(&encoded[..1]).unwrap().is_none());
        assert!(decode_buffered_frame_v1(&encoded[..3]).unwrap().is_none());
        assert!(decode_buffered_frame_v1(&encoded[..100]).unwrap().is_none());
        let (frame, consumed) = decode_buffered_frame_v1(&encoded).unwrap().unwrap();
        assert_eq!(consumed, encoded.len());
        match frame {
            RelayClientFrameV1::Binary(decoded) => assert_eq!(decoded, payload),
            other => panic!("unexpected buffered frame: {other:?}"),
        }
    }

    fn hex_v1(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
