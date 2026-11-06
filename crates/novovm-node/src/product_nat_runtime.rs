//! Network-only qualification on the original NativeEndpoint. No chain store,
//! second UDP codec, raw identity loader or signature-suite fallback.
use anyhow::{bail, ensure, Context, Result};
use novovm_network::{
    novorudp::{
        carrier::NativeRoute,
        endpoint::{NativeEndpoint, NativeHandshakeBudget, NativePeer},
        NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0,
    },
    product_overlay::{
        peer_id_from_mldsa65_public_key_v1, TransportIdentity, MLDSA65_PUBLIC_KEY_BYTES,
    },
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::{IpAddr, SocketAddr, UdpSocket},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const STREAM: u64 = 1;
const MAGIC: &[u8; 8] = b"NVNATP02";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductNatRuntimeModeV1 {
    ObservedEndpointObserver,
    NatPunchTarget,
    ObservedEndpointProbe,
    NatPunchProbe,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductNatRuntimeConfigV1 {
    /// Version 2 is the protected ML-DSA native-session contract, not old JSON UDP.
    pub version: u16,
    pub mode: ProductNatRuntimeModeV1,
    pub bind_addr: String,
    pub identity_key_path: PathBuf,
    pub chain_id: u64,
    pub library: PathBuf,
    pub library_sha256: String,
    pub peer_public_key: Vec<u8>,
    #[serde(default)]
    pub peer_addr: Option<String>,
    /// Explicitly exercise the same configured-peer source-learning policy as
    /// the native node. Never open admission or a replacement NAT protocol.
    #[serde(default)]
    pub learn_authenticated_peer_routes: bool,
    #[serde(default)]
    pub expected_peer_id: Option<String>,
    #[serde(default = "default_timeout_ms_v1")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub report_path: Option<PathBuf>,
    pub run_for_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductNatRuntimeReportV1 {
    pub accepted: bool,
    pub scope: &'static str,
    pub mode: String,
    pub bind_addr: String,
    pub local_peer_id: String,
    pub peer_id: String,
    pub observed_endpoint: Option<String>,
    pub session_id: Option<[u8; 16]>,
    pub authenticated_exchanges: usize,
    pub authenticated_requests: usize,
    pub local_replies_submitted: usize,
    pub round_trip_confirmed: bool,
    pub handshake_version: u16,
    pub signature_suite: u16,
    pub configured_peer_route_learning: bool,
    pub novorudp_inner_frame_preserved: bool,
    pub novorudp_wire_changed: bool,
    pub network_only: bool,
    pub aoem_called: bool,
    pub ledger_semantics: bool,
    pub payload_treated_opaque: bool,
    pub apfl_interpreted: bool,
}

pub fn load_product_nat_runtime_config_v1(
    path: impl AsRef<Path>,
) -> Result<ProductNatRuntimeConfigV1> {
    let path = path.as_ref().canonicalize()?;
    let mut config: ProductNatRuntimeConfigV1 = serde_json::from_slice(&fs::read(&path)?)?;
    let base = path.parent().context("NAT config directory")?;
    for item in [&mut config.identity_key_path, &mut config.library] {
        if item.is_relative() {
            *item = base.join(&*item);
        }
    }
    if let Some(report) = config.report_path.as_mut() {
        if report.is_relative() {
            *report = base.join(&*report);
        }
    }
    validate_config(&config)?;
    Ok(config)
}

fn validate_config(config: &ProductNatRuntimeConfigV1) -> Result<()> {
    ensure!(
        config.version == 2,
        "ML-DSA native NAT qualification requires config version 2"
    );
    ensure!(
        config.chain_id != 0 && config.peer_public_key.len() == MLDSA65_PUBLIC_KEY_BYTES,
        "explicit chain and ML-DSA peer pin required"
    );
    ensure!(
        (2..=30_000).contains(&config.timeout_ms) && (1..=120_000).contains(&config.run_for_ms),
        "invalid bounded native probe duration"
    );
    ensure!(
        config.run_for_ms >= config.timeout_ms,
        "run duration must cover the probe timeout"
    );
    config.bind_addr.parse::<SocketAddr>()?;
    let peer = peer_id_from_mldsa65_public_key_v1(&config.peer_public_key);
    if let Some(expected) = &config.expected_peer_id {
        ensure!(
            expected == &peer,
            "peer label differs from full public-key pin"
        );
    }
    ensure!(
        !config.learn_authenticated_peer_routes || is_server(&config.mode),
        "only a configured observer may learn authenticated peer routes"
    );
    if !is_server(&config.mode) || config.learn_authenticated_peer_routes {
        required_peer_address(config)?;
    }
    Ok(())
}

fn is_server(mode: &ProductNatRuntimeModeV1) -> bool {
    matches!(
        mode,
        ProductNatRuntimeModeV1::ObservedEndpointObserver | ProductNatRuntimeModeV1::NatPunchTarget
    )
}

fn required_peer_address(config: &ProductNatRuntimeConfigV1) -> Result<SocketAddr> {
    let address: SocketAddr = config
        .peer_addr
        .as_deref()
        .context("peer_addr required")?
        .parse()?;
    ensure!(
        address.port() != 0 && !address.ip().is_unspecified() && !address.ip().is_multicast(),
        "invalid probe peer address"
    );
    Ok(address)
}

pub fn run_product_nat_runtime_v1(
    config: ProductNatRuntimeConfigV1,
) -> Result<ProductNatRuntimeReportV1> {
    validate_config(&config)?;
    let identity = novovm_relay::transport_identity::load_transport_identity(
        &config.identity_key_path,
        config.chain_id,
        &config.library,
        &config.library_sha256,
    )?;
    run_with_identity(config, identity)
}

fn run_with_identity(
    config: ProductNatRuntimeConfigV1,
    identity: TransportIdentity,
) -> Result<ProductNatRuntimeReportV1> {
    ensure!(
        identity.public_key() != config.peer_public_key,
        "native probe self identity"
    );
    let server = is_server(&config.mode);
    let socket = UdpSocket::bind(&config.bind_addr)?;
    let budget = NativeHandshakeBudget {
        lifetime: Duration::from_millis(config.timeout_ms),
        retry: Duration::from_millis(100.min(config.timeout_ms - 1)),
    };
    let started = Instant::now();
    let mut endpoint = if config.learn_authenticated_peer_routes {
        let mut endpoint = NativeEndpoint::bind(
            socket,
            identity,
            vec![NativePeer {
                key: config.peer_public_key.clone(),
                address: required_peer_address(&config)?,
            }],
            budget,
            now_ms_v1(),
            started,
        )?;
        endpoint.enable_configured_reconnect_v1()?;
        endpoint.enable_configured_route_learning_v1()?;
        endpoint
    } else if server {
        NativeEndpoint::listen_clients(socket, identity, 4, vec![STREAM], budget)?
    } else {
        NativeEndpoint::connect_client(
            socket,
            identity,
            NativePeer {
                key: config.peer_public_key.clone(),
                address: required_peer_address(&config)?,
            },
            budget,
            now_ms_v1(),
            started,
        )?
    };
    if server && !config.learn_authenticated_peer_routes {
        endpoint.enable_client_reconnect_v1()?;
    }
    let mut nonce = [0; 16];
    OsRng.fill_bytes(&mut nonce);
    let object = u64::from_be_bytes(nonce[..8].try_into()?);
    let expected_peer = peer_id_from_mldsa65_public_key_v1(&config.peer_public_key);
    let mut report = ProductNatRuntimeReportV1 {
        accepted: false,
        scope: "novovm_native_authenticated_endpoint_v2",
        mode: if server { "observer" } else { "probe" }.into(),
        bind_addr: endpoint.local_addr()?.to_string(),
        local_peer_id: peer_id_from_mldsa65_public_key_v1(endpoint.local_identity()),
        peer_id: expected_peer.clone(),
        observed_endpoint: None,
        session_id: None,
        authenticated_exchanges: 0,
        authenticated_requests: 0,
        local_replies_submitted: 0,
        round_trip_confirmed: false,
        handshake_version: 2,
        signature_suite: 65,
        configured_peer_route_learning: config.learn_authenticated_peer_routes,
        novorudp_inner_frame_preserved: true,
        novorudp_wire_changed: false,
        network_only: true,
        aoem_called: true,
        ledger_semantics: false,
        payload_treated_opaque: true,
        apfl_interpreted: false,
    };
    let deadline = started + Duration::from_millis(config.run_for_ms);
    let mut sent = false;
    let mut pending_reply: Option<(usize, NovoRudpTransportFrameV0)> = None;
    while Instant::now() < deadline {
        let now = Instant::now();
        let polled = endpoint.poll(now_ms_v1(), now, 64)?;
        for received in polled.frames {
            if received.peer_id() != expected_peer {
                continue;
            }
            let frame = received.frame();
            if frame.stream_id != STREAM
                || frame.kind != NovoRudpTransportFrameKindV0::Data
                || !frame.payload.starts_with(MAGIC)
            {
                continue;
            }
            if server && frame.payload.len() == 25 && frame.payload[8] == 1 {
                report.authenticated_requests += 1;
                let peer = received.peer();
                let NativeRoute::Udp(source) = endpoint.peer_route(peer)? else {
                    bail!("probe is not on UDP");
                };
                let mut payload = frame.payload.clone();
                payload[8] = 2;
                encode_address(source, &mut payload);
                pending_reply = Some((
                    peer,
                    NovoRudpTransportFrameV0::new(
                        NovoRudpTransportFrameKindV0::Data,
                        frame.session_id,
                        STREAM,
                        frame.object_id,
                        frame.sequence,
                        0,
                        payload,
                    ),
                ));
                report.session_id = endpoint.session_id(peer);
                report.observed_endpoint = Some(source.to_string());
            } else if !server
                && frame.payload.get(8) == Some(&2)
                && frame.object_id == object
                && frame.payload.get(9..25) == Some(nonce.as_slice())
            {
                let observed = decode_address(&frame.payload[25..])?;
                report.observed_endpoint = Some(observed.to_string());
                report.session_id = endpoint.session_id(received.peer());
                report.authenticated_exchanges = 1;
                report.round_trip_confirmed = true;
                report.accepted = true;
                break;
            }
        }
        if let Some((peer, reply)) = &pending_reply {
            if endpoint.try_send(*peer, reply)? {
                report.authenticated_exchanges += 1;
                report.local_replies_submitted += 1;
                report.accepted = true;
                pending_reply = None;
            }
        }
        if !server && !sent {
            if let Some(session) = endpoint.session_id(0) {
                let mut payload = MAGIC.to_vec();
                payload.push(1);
                payload.extend_from_slice(&nonce);
                sent = endpoint.try_send(
                    0,
                    &NovoRudpTransportFrameV0::new(
                        NovoRudpTransportFrameKindV0::Data,
                        session,
                        STREAM,
                        object,
                        0,
                        0,
                        payload,
                    ),
                )?;
            }
        }
        if !server && report.accepted {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    if let Some(path) = &config.report_path {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    ensure!(
        report.accepted,
        "native authenticated probe did not complete; retain failure report"
    );
    Ok(report)
}

fn encode_address(address: SocketAddr, out: &mut Vec<u8>) {
    match address.ip() {
        IpAddr::V4(ip) => {
            out.push(4);
            out.extend_from_slice(&ip.octets());
        }
        IpAddr::V6(ip) => {
            out.push(6);
            out.extend_from_slice(&ip.octets());
        }
    }
    out.extend_from_slice(&address.port().to_be_bytes());
}

fn decode_address(bytes: &[u8]) -> Result<SocketAddr> {
    let ip = match bytes {
        [4, a, b, c, d, _, _] => IpAddr::V4([*a, *b, *c, *d].into()),
        value if value.len() == 19 && value[0] == 6 => {
            IpAddr::V6(<[u8; 16]>::try_from(&value[1..17])?.into())
        }
        _ => bail!("invalid authenticated observed endpoint"),
    };
    Ok(SocketAddr::new(
        ip,
        u16::from_be_bytes(bytes[bytes.len() - 2..].try_into()?),
    ))
}

fn default_timeout_ms_v1() -> u64 {
    3_000
}
fn now_ms_v1() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod replay_qualification;

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_network::product_overlay::{
        handshake_wire, HandshakeReplayCacheV1, NodeHandshakeInitiatorV1, NodeHandshakeResponderV1,
    };

    #[test]
    #[ignore = "requires explicit trusted NOVOVM_AOEM_TEST_LIBRARY; actual AOEM ML-DSA, not a component fixture"]
    fn actual_aoem_handshake_rejects_tamper_and_authenticates_both_directions() -> Result<()> {
        use novovm_exec::{pq_signature::AoemMldsaParameterSet, AoemExecFacade};
        use novovm_relay::{purpose_keys::PurposeKeyMaterialV3, transport_identity::from_mldsa};
        let library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .context("explicit trusted AOEM artifact required")?;
        let facade = AoemExecFacade::open(Path::new(&library), Default::default())?;
        let runtime = facade.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
        let identity = || -> Result<TransportIdentity> {
            let key = runtime.generate_keypair()?;
            from_mldsa(
                runtime.clone(),
                PurposeKeyMaterialV3::new(key.public_key, key.expanded_secret_key)?,
            )
        };
        let a = identity()?;
        let b = identity()?;
        ensure!(
            a.public_key() != b.public_key(),
            "independent actual purpose keys"
        );
        let initiator = NodeHandshakeInitiatorV1::start(&a, b.peer_id(), 1000, 5000)?;
        let wire = handshake_wire::encode_offer(initiator.offer())?;
        assert_eq!(wire.len(), 5529);
        let mut bad = initiator.offer().clone();
        bad.challenge[0] ^= 1;
        let mut replay = HandshakeReplayCacheV1::default();
        assert!(NodeHandshakeResponderV1::respond(&bad, &b, 1100, 5000, &mut replay).is_err());
        bad = initiator.offer().clone();
        bad.version = 1;
        assert!(NodeHandshakeResponderV1::respond(&bad, &b, 1100, 5000, &mut replay).is_err());
        let responder =
            NodeHandshakeResponderV1::respond(initiator.offer(), &b, 1100, 5000, &mut replay)?;
        assert_eq!(
            handshake_wire::encode_response(responder.response())?.len(),
            5593
        );
        let mut wrong = responder.response().clone();
        wrong.responder_identity_public_key = a.public_key().to_vec();
        drop(responder);
        let response = match handshake_wire::decode(&handshake_wire::encode_response(&wrong)?)? {
            handshake_wire::HandshakeMessage::Response(value) => value,
            _ => unreachable!(),
        };
        let initiator =
            match initiator.complete_owned(&response, 1200, &mut HandshakeReplayCacheV1::default())
            {
                Ok(_) => bail!("wrong responder public key accepted"),
                Err(failure) => failure.0,
            };
        // Obtain a fresh valid response without reusing responder replay state.
        let responder = NodeHandshakeResponderV1::respond(
            initiator.offer(),
            &b,
            1100,
            5000,
            &mut HandshakeReplayCacheV1::default(),
        )?;
        let response = responder.response().clone();
        // The earlier response has different ECDH material; use the same owner
        // that supplied this validated transcript for the actual exchange.
        let mut receiving = responder.into_channel();
        let mut sending =
            initiator.complete(&response, 1200, &mut HandshakeReplayCacheV1::default())?;
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            sending.session_id(),
            1,
            2,
            3,
            0,
            b"actual-AOEM".to_vec(),
        );
        let envelope = sending.seal_novorudp_frame(&frame)?;
        let mut corrupt = envelope.clone();
        *corrupt.ciphertext.last_mut().unwrap() ^= 1;
        assert!(receiving.open_novorudp_frame(&corrupt).is_err());
        assert_eq!(receiving.open_novorudp_frame(&envelope)?, frame);
        assert!(receiving.open_novorudp_frame(&envelope).is_err());
        let reply = receiving.seal_novorudp_frame(&frame)?;
        assert_eq!(sending.open_novorudp_frame(&reply)?, frame);
        Ok(())
    }

    #[test]
    #[ignore = "requires explicit trusted NOVOVM_AOEM_TEST_LIBRARY; actual AOEM + loopback UDP continuation/reconnect"]
    fn actual_aoem_native_udp_reconnect_rejects_previous_session_packets() -> Result<()> {
        use novovm_exec::{pq_signature::AoemMldsaParameterSet, AoemExecFacade};
        use novovm_network::novorudp::{carrier::NativeDatagramIo, endpoint::NativeRoutePeer};
        use novovm_relay::{purpose_keys::PurposeKeyMaterialV3, transport_identity::from_mldsa};
        use std::{
            io,
            sync::{Arc, Mutex},
        };

        struct CaptureIo {
            socket: UdpSocket,
            first_data: Mutex<Option<Vec<u8>>>,
            maximum_datagram: Mutex<usize>,
        }
        impl NativeDatagramIo for CaptureIo {
            fn try_receive(&self, buffer: &mut [u8]) -> io::Result<Option<(usize, NativeRoute)>> {
                self.socket.try_receive(buffer)
            }
            fn try_send(&self, bytes: &[u8], route: NativeRoute) -> io::Result<usize> {
                let count = self.socket.try_send(bytes, route)?;
                let mut maximum = self.maximum_datagram.lock().unwrap();
                *maximum = (*maximum).max(count);
                let mut first = self.first_data.lock().unwrap();
                if first.is_none() && bytes.starts_with(b"NVRLY002") {
                    *first = Some(bytes.to_vec());
                }
                Ok(count)
            }
            fn local_udp_addr(&self) -> io::Result<SocketAddr> {
                self.socket.local_addr()
            }
        }
        fn exchange(
            server: &mut NativeEndpoint,
            client: &mut NativeEndpoint,
            marker: u8,
        ) -> Result<[u8; 16]> {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut sent = false;
            let mut received = false;
            while Instant::now() < deadline {
                client.poll(now_ms_v1(), Instant::now(), 64)?;
                let result = server.poll(now_ms_v1(), Instant::now(), 64)?;
                received |= result
                    .frames
                    .iter()
                    .any(|item| item.frame().payload == [marker]);
                if let Some(session) = client.session_id(0) {
                    if !sent {
                        sent = client.try_send(
                            0,
                            &NovoRudpTransportFrameV0::new(
                                NovoRudpTransportFrameKindV0::Data,
                                session,
                                STREAM,
                                1,
                                0,
                                0,
                                vec![marker],
                            ),
                        )?;
                    }
                    if received && server.session_id(0) == Some(session) {
                        return Ok(session);
                    }
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            bail!("actual native UDP session did not exchange authenticated data")
        }
        let library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .context("explicit trusted AOEM artifact required")?;
        let facade = AoemExecFacade::open(Path::new(&library), Default::default())?;
        let runtime = facade.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
        let identity = || -> Result<TransportIdentity> {
            let pair = runtime.generate_keypair()?;
            from_mldsa(
                runtime.clone(),
                PurposeKeyMaterialV3::new(pair.public_key, pair.expanded_secret_key)?,
            )
        };
        let server_key = identity()?;
        let client_key = identity()?;
        let server_socket = UdpSocket::bind("127.0.0.1:0")?;
        let address = server_socket.local_addr()?;
        let budget = || NativeHandshakeBudget {
            lifetime: Duration::from_secs(5),
            retry: Duration::from_millis(20),
        };
        let mut server = NativeEndpoint::listen_clients(
            server_socket,
            server_key.clone(),
            1,
            vec![STREAM],
            budget(),
        )?;
        server.enable_client_reconnect_v1()?;
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        socket.set_nonblocking(true)?;
        let io = Arc::new(CaptureIo {
            socket,
            first_data: Mutex::new(None),
            maximum_datagram: Mutex::new(0),
        });
        let connect = || {
            NativeEndpoint::connect_clients_with_io(
                io.clone(),
                client_key.clone(),
                vec![NativeRoutePeer {
                    key: server_key.public_key().to_vec(),
                    route: address.into(),
                }],
                budget(),
                now_ms_v1(),
                Instant::now(),
            )
        };
        let mut client = connect()?;
        let first = exchange(&mut server, &mut client, 1)?;
        assert_eq!(server.peer_id(0)?, client_key.peer_id());
        assert_eq!(
            server.peer_route(0)?,
            NativeRoute::Udp(io.local_udp_addr()?)
        );
        let replay = io
            .first_data
            .lock()
            .unwrap()
            .clone()
            .context("encrypted data was not captured")?;
        drop(client);
        let mut client = connect()?;
        let second = exchange(&mut server, &mut client, 2)?;
        assert_ne!(first, second, "reconnect must derive a fresh session");
        assert!(
            *io.maximum_datagram.lock().unwrap() <= 1200,
            "ML-DSA handshake bypassed continuation"
        );
        io.try_send(&replay, address.into())?;
        let mut rejected = 0;
        for _ in 0..20 {
            let result = server.poll(now_ms_v1(), Instant::now(), 64)?;
            assert!(
                result.frames.is_empty(),
                "previous-session data reached the consumer"
            );
            rejected += result.rejected_datagrams;
            if rejected > 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            rejected > 0,
            "previous-session replay was not observed/rejected"
        );
        Ok(())
    }

    #[test]
    #[cfg(any(windows, target_os = "linux"))]
    #[ignore = "requires explicit trusted NOVOVM_AOEM_TEST_LIBRARY; actual protected qualification entry and reconnect"]
    fn actual_aoem_protected_probe_entry_reconnects_and_reports_observed_source() -> Result<()> {
        protected_probe_entry_fixture(false)
    }

    #[test]
    #[ignore = "trusted AOEM; actual configured source learning, wrong initial address and real socket reconnect"]
    fn actual_aoem_protected_configured_route_learning_and_reconnect() -> Result<()> {
        protected_probe_entry_fixture(true)
    }

    fn protected_probe_entry_fixture(learn_routes: bool) -> Result<()> {
        use crate::native_pipeline::{
            consensus::crypto::AoemConsensusCrypto,
            service::config::test_custody::{generate_transport_key, TestCredentialGuard},
        };
        use novovm_exec::AoemExecFacade;
        use sha2::{Digest, Sha256};

        let library = PathBuf::from(
            std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
                .context("explicit trusted AOEM artifact required")?,
        )
        .canonicalize()?;
        let sha256 = format!("{:x}", Sha256::digest(fs::read(&library)?));
        let root = std::env::temp_dir().join(format!(
            "novovm-transport-auth-{}-{}",
            std::process::id(),
            now_ms_v1()
        ));
        fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        let _credential = TestCredentialGuard::new(&root)?;
        let facade = AoemExecFacade::open(&library, Default::default())?;
        let crypto = AoemConsensusCrypto::new(&facade)?;
        let chain = 991_730;
        let (server, server_path) = generate_transport_key(&root, chain, "observer", &crypto)?;
        let (client, client_path) = generate_transport_key(&root, chain, "probe", &crypto)?;
        let reserved = UdpSocket::bind("127.0.0.1:0")?;
        let address = reserved.local_addr()?;
        drop(reserved);
        let config =
            |mode, bind: String, identity_key_path, peer_public_key| ProductNatRuntimeConfigV1 {
                version: 2,
                mode,
                bind_addr: bind,
                identity_key_path,
                chain_id: chain,
                library: library.clone(),
                library_sha256: sha256.clone(),
                peer_public_key,
                peer_addr: Some(address.to_string()),
                learn_authenticated_peer_routes: false,
                expected_peer_id: None,
                timeout_ms: 1000,
                report_path: None,
                run_for_ms: 3000,
            };
        let mut observer = config(
            ProductNatRuntimeModeV1::ObservedEndpointObserver,
            address.to_string(),
            server_path,
            client.public_key().to_vec(),
        );
        observer.report_path = Some(root.join("observer.json"));
        let _stale_candidate = if learn_routes {
            let stale = UdpSocket::bind("127.0.0.1:0")?;
            observer.peer_addr = Some(stale.local_addr()?.to_string());
            observer.learn_authenticated_peer_routes = true;
            Some(stale)
        } else {
            None
        };
        let mut probe = config(
            ProductNatRuntimeModeV1::ObservedEndpointProbe,
            "127.0.0.1:0".into(),
            client_path,
            server.public_key().to_vec(),
        );
        probe.run_for_ms = 1000;
        probe.expected_peer_id = Some(server.peer_id());
        let observer = std::thread::spawn(move || run_product_nat_runtime_v1(observer));
        let first = run_product_nat_runtime_v1(probe.clone());
        let second = run_product_nat_runtime_v1(probe);
        // Always join the original owner, including a failed probe.
        let observed = observer
            .join()
            .map_err(|_| anyhow::anyhow!("observer panicked"))??;
        let first = first?;
        let second = second?;
        assert!(first.accepted && second.accepted && observed.accepted);
        assert_eq!(observed.configured_peer_route_learning, learn_routes);
        assert!(!observed.round_trip_confirmed);
        assert_eq!(
            observed.authenticated_exchanges,
            observed.local_replies_submitted
        );
        assert!(observed.authenticated_requests >= observed.local_replies_submitted);
        assert_ne!(first.session_id, second.session_id);
        assert_eq!(observed.peer_id, client.peer_id());
        assert!(observed.authenticated_exchanges >= 2);
        for result in [first, second] {
            assert!(result.round_trip_confirmed);
            assert_eq!(result.local_replies_submitted, 0);
            assert_eq!(result.local_peer_id, client.peer_id());
            let source: SocketAddr = result
                .observed_endpoint
                .context("observed source")?
                .parse()?;
            assert!(source.ip().is_loopback() && source.port() != 0);
            assert_eq!(result.signature_suite, 65);
            assert_eq!(result.handshake_version, 2);
        }
        Ok(())
    }

    #[test]
    fn configured_route_learning_requires_observer_and_explicit_peer_candidate() -> Result<()> {
        let mut document = serde_json::json!({
            "version":2, "mode":"observed_endpoint_observer", "bind_addr":"127.0.0.1:0",
            "identity_key_path":"not-read.sealed", "chain_id":19,
            "library":"not-loaded", "library_sha256":"not-checked-before-loading",
            "peer_public_key":vec![1u8; MLDSA65_PUBLIC_KEY_BYTES],
            "timeout_ms":1000, "run_for_ms":3000,
            "learn_authenticated_peer_routes":true
        });
        let mut config: ProductNatRuntimeConfigV1 = serde_json::from_value(document.clone())?;
        assert!(validate_config(&config).is_err());
        config.peer_addr = Some("127.0.0.1:41051".into());
        validate_config(&config)?;
        config.mode = ProductNatRuntimeModeV1::ObservedEndpointProbe;
        assert!(validate_config(&config).is_err());
        config.learn_authenticated_peer_routes = false;
        validate_config(&config)?;
        document
            .as_object_mut()
            .unwrap()
            .remove("learn_authenticated_peer_routes");
        config = serde_json::from_value(document)?;
        assert!(!config.learn_authenticated_peer_routes);
        validate_config(&config)?;
        Ok(())
    }

    #[test]
    fn observed_address_encoding_rejects_truncated_or_surplus_fields() {
        for value in ["192.168.71.125:41050", "[::1]:41050"] {
            let expected = value.parse().unwrap();
            let mut bytes = Vec::new();
            encode_address(expected, &mut bytes);
            assert_eq!(decode_address(&bytes).unwrap(), expected);
            assert!(decode_address(&bytes[..bytes.len() - 1]).is_err());
            bytes.push(0);
            assert!(decode_address(&bytes).is_err());
        }
    }
}
