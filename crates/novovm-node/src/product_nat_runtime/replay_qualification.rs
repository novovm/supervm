//! Explicit cross-host diagnostic; the receiving process remains the frozen
//! product NAT binary. This driver neither decides receiver acceptance nor
//! substitutes a missing receive trace with a successful local send.
use super::*;
use novovm_network::novorudp::{carrier::NativeDatagramIo, endpoint::NativeRoutePeer};
use sha2::{Digest, Sha256};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

struct CaptureIo {
    socket: UdpSocket,
    armed: AtomicBool,
    captured: Mutex<Option<Vec<u8>>>,
}

impl NativeDatagramIo for CaptureIo {
    fn try_receive(&self, buffer: &mut [u8]) -> io::Result<Option<(usize, NativeRoute)>> {
        self.socket.try_receive(buffer)
    }

    fn try_send(&self, bytes: &[u8], route: NativeRoute) -> io::Result<usize> {
        let count = self.socket.try_send(bytes, route)?;
        if count == bytes.len() && self.armed.load(Ordering::Relaxed) {
            let mut capture = self.captured.lock().unwrap();
            assert!(capture.is_none(), "one application send per capture");
            *capture = Some(bytes.to_vec());
        }
        Ok(count)
    }

    fn local_udp_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}

#[derive(Serialize)]
struct Exchange {
    session_id: [u8; 16],
    observed_endpoint: String,
    wire_file: String,
    wire_sha256: String,
    wire_bytes: usize,
    submitted_ms: u64,
    reply_ms: u64,
}

fn exchange(
    endpoint: &mut NativeEndpoint,
    io: &CaptureIo,
    config: &ProductNatRuntimeConfigV1,
    output: &Path,
    name: &str,
) -> Result<Exchange> {
    let mut nonce = [0; 16];
    OsRng.fill_bytes(&mut nonce);
    let object = u64::from_be_bytes(nonce[..8].try_into()?);
    let expected_peer = peer_id_from_mldsa65_public_key_v1(&config.peer_public_key);
    let deadline = Instant::now() + Duration::from_millis(config.timeout_ms);
    let mut submission = None;
    while Instant::now() < deadline {
        let polled = endpoint.poll(now_ms_v1(), Instant::now(), 64)?;
        for received in polled.frames {
            let frame = received.frame();
            if received.peer_id() == expected_peer
                && frame.kind == NovoRudpTransportFrameKindV0::Data
                && frame.stream_id == STREAM
                && frame.object_id == object
                && frame.payload.starts_with(MAGIC)
                && frame.payload.get(8) == Some(&2)
                && frame.payload.get(9..25) == Some(nonce.as_slice())
            {
                let (session, submitted_ms) = submission.context("reply before request")?;
                ensure!(
                    frame.session_id == session,
                    "reply belongs to another session"
                );
                let wire = io
                    .captured
                    .lock()
                    .unwrap()
                    .take()
                    .context("missing capture")?;
                ensure!(
                    wire.starts_with(b"NVRLY002"),
                    "not an encrypted native frame"
                );
                let wire_file = format!("{name}.wire.bin");
                fs::write(output.join(&wire_file), &wire)?;
                let result = Exchange {
                    session_id: session,
                    observed_endpoint: decode_address(&frame.payload[25..])?.to_string(),
                    wire_file,
                    wire_sha256: format!("{:x}", Sha256::digest(&wire)),
                    wire_bytes: wire.len(),
                    submitted_ms,
                    reply_ms: now_ms_v1(),
                };
                fs::write(
                    output.join(format!("{name}.json")),
                    serde_json::to_vec_pretty(&result)?,
                )?;
                return Ok(result);
            }
        }
        if submission.is_none() {
            if let Some(session) = endpoint.session_id(0) {
                let mut payload = MAGIC.to_vec();
                payload.push(1);
                payload.extend_from_slice(&nonce);
                let frame = NovoRudpTransportFrameV0::new(
                    NovoRudpTransportFrameKindV0::Data,
                    session,
                    STREAM,
                    object,
                    0,
                    0,
                    payload,
                );
                // Arm only this original application send, never handshake poll.
                io.armed.store(true, Ordering::Relaxed);
                let sent = endpoint.try_send(0, &frame);
                io.armed.store(false, Ordering::Relaxed);
                if sent? {
                    submission = Some((session, now_ms_v1()));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    bail!("{name}: authenticated reply timed out; retain diagnostic output")
}

#[test]
#[ignore = "explicit cross-host replay config/output required; fixed receiver and receive trace must be reconciled separately"]
fn real_network_previous_session_wire_replay() -> Result<()> {
    let config = load_product_nat_runtime_config_v1(
        std::env::var_os("NOVOVM_REPLAY_CONFIG")
            .context("explicit original probe config required")?,
    )?;
    ensure!(
        !is_server(&config.mode),
        "replay driver requires probe config"
    );
    let output = PathBuf::from(
        std::env::var_os("NOVOVM_REPLAY_OUTPUT").context("new diagnostic directory required")?,
    );
    fs::create_dir(&output).context("output must be new; preserve previous evidence")?;
    let identity = novovm_relay::transport_identity::load_transport_identity(
        &config.identity_key_path,
        config.chain_id,
        &config.library,
        &config.library_sha256,
    )?;
    let local_peer = identity.peer_id();
    let address = required_peer_address(&config)?;
    let socket = UdpSocket::bind(&config.bind_addr)?;
    socket.set_nonblocking(true)?;
    let io = Arc::new(CaptureIo {
        socket,
        armed: AtomicBool::new(false),
        captured: Mutex::new(None),
    });
    let connect = || {
        NativeEndpoint::connect_clients_with_io(
            io.clone(),
            identity.clone(),
            vec![NativeRoutePeer {
                key: config.peer_public_key.clone(),
                route: address.into(),
            }],
            NativeHandshakeBudget {
                lifetime: Duration::from_millis(config.timeout_ms),
                retry: Duration::from_millis(100.min(config.timeout_ms - 1)),
            },
            now_ms_v1(),
            Instant::now(),
        )
    };
    let mut first = connect()?;
    let before = exchange(&mut first, &io, &config, &output, "session1-request")?;
    let old_wire = fs::read(output.join(&before.wire_file))?;
    drop(first);
    let mut second = connect()?;
    // An authenticated reply proves the responder installed session2; the
    // initiator merely receiving a handshake Response does not prove that.
    let confirmation = exchange(&mut second, &io, &config, &output, "session2-confirm")?;
    ensure!(
        before.session_id != confirmation.session_id,
        "session was reused"
    );
    ensure!(
        before.observed_endpoint == confirmation.observed_endpoint,
        "source mapping changed"
    );
    let route = second.peer_route(0)?;
    ensure!(
        route == NativeRoute::Udp(address),
        "unexpected replay destination"
    );
    let replay_ms = now_ms_v1();
    let count = io.try_send(&old_wire, route)?;
    ensure!(count == old_wire.len(), "replay not fully submitted");
    fs::write(
        output.join("replay-submission.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "submitted_ms": replay_ms, "wire_bytes": count,
            "wire_sha256": before.wire_sha256, "source": io.local_udp_addr()?.to_string(),
            "destination": address.to_string(), "receiver_arrival_proven": false
        }))?,
    )?;
    let after = exchange(&mut second, &io, &config, &output, "session2-after-replay")?;
    ensure!(
        after.session_id == confirmation.session_id,
        "post-replay session changed"
    );
    ensure!(
        after.observed_endpoint == before.observed_endpoint,
        "post-replay source changed"
    );
    fs::write(
        output.join("sender-result.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "scope": "test_driver_original_native_endpoint_fixed_receiver",
            "local_peer_id": local_peer,
            "peer_id": second.peer_id(0)?,
            "before": before, "confirmed_new_session": confirmation, "after": after,
            "expected_receiver_authenticated_requests": 3,
            "sender_round_trips_passed": true,
            "receiver_reconciliation_pending": true,
            "network_only": true, "chain_started": false
        }))?,
    )?;
    Ok(())
}
