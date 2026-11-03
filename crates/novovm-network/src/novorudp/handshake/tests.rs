use super::*;
use crate::novorudp::{NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0};
use std::io::ErrorKind;

fn keys() -> (TransportIdentity, TransportIdentity) {
    let a = crate::test_identity::key(31);
    let b = crate::test_identity::key(47);
    if peer_id_from_mldsa65_public_key_v1(a.public_key())
        < peer_id_from_mldsa65_public_key_v1(b.public_key())
    {
        (a, b)
    } else {
        (b, a)
    }
}

fn receive(socket: &UdpSocket) -> Result<(Vec<u8>, SocketAddr)> {
    let until = Instant::now() + Duration::from_secs(2);
    let mut buffer = [0; MAX_DATAGRAM_BYTES + 1];
    loop {
        match socket.recv_from(&mut buffer) {
            Ok((n, source)) => return Ok((buffer[..n].to_vec(), source)),
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                ensure!(Instant::now() < until, "UDP test receive timeout");
                std::thread::yield_now();
            }
            Err(e) => return Err(e.into()),
        }
    }
}

fn receive_handshake(
    sender: &mut NativeHandshake,
    source_socket: &UdpSocket,
    destination: &UdpSocket,
    wall_ms: u64,
    now: Instant,
) -> Result<(Vec<u8>, SocketAddr)> {
    let mut receiver = handshake_continuation::Receiver::new(now);
    for _ in 0..32 {
        let (wire, source) = receive(destination)?;
        match receiver.receive(source, &wire, now)? {
            handshake_continuation::Received::Reply(reply) => {
                destination.send_to(&reply, source)?;
                let (challenge, source) = receive(source_socket)?;
                sender.receive_continuation_challenge(source.into(), &challenge, now)?;
            }
            handshake_continuation::Received::Pending => {}
            handshake_continuation::Received::Complete(wire) => return Ok((wire, source)),
            handshake_continuation::Received::Challenge => anyhow::bail!("unexpected challenge"),
        }
        ensure!(
            sender.try_send(source_socket, wall_ms, now)?,
            "handshake continuation stalled"
        );
    }
    anyhow::bail!("handshake continuation exceeded test packet bound")
}

#[test]
fn lost_response_retries_same_native_handshake_and_live_channel_survives_expiry() -> Result<()> {
    let (ka, kb) = keys();
    let a = UdpSocket::bind("127.0.0.1:0")?;
    let b = UdpSocket::bind("127.0.0.1:0")?;
    a.set_nonblocking(true)?;
    b.set_nonblocking(true)?;
    let now = Instant::now();
    let mut left = NativeHandshake::start(
        ka.clone(),
        kb.public_key().to_vec(),
        b.local_addr()?,
        1000,
        now,
        Duration::from_secs(5),
        Duration::from_millis(10),
    )?;
    let mut right = NativeHandshake::start(
        kb.clone(),
        ka.public_key().to_vec(),
        a.local_addr()?,
        1000,
        now,
        Duration::from_secs(5),
        Duration::from_millis(10),
    )?;
    let mut left_cache = HandshakeReplayCacheV1::default();
    let mut right_cache = HandshakeReplayCacheV1::default();
    assert!(left.try_send(&a, 1000, now)?);
    assert!(!right.try_send(&b, 1000, now)?);
    let (offer, source) = receive_handshake(&mut left, &a, &b, 1000, now)?;
    assert!(right
        .receive("127.0.0.1:1".parse()?, &offer, 1000, now, &mut right_cache)
        .is_err());
    right.receive(source, &offer, 1000, now, &mut right_cache)?;
    let mut right_channel = right.take_session().context("responder channel missing")?;
    assert!(right.try_send(&b, 1000, now)?);
    let (lost_response, _) = receive_handshake(&mut right, &b, &a, 1000, now)?;
    let later = now + Duration::from_millis(10);
    assert!(left.try_send(&a, 1010, later)?);
    let (retry, source) = receive_handshake(&mut left, &a, &b, 1010, later)?;
    assert_eq!(retry, offer);
    right.receive(source, &retry, 1010, later, &mut right_cache)?;
    assert!(
        right.take_session().is_none(),
        "retry recreated encryption sequence state"
    );
    assert!(right.try_send(&b, 1010, later)?);
    let (response, source) = receive_handshake(&mut right, &b, &a, 1010, later)?;
    assert_eq!(response, lost_response);
    let mut forged = response.clone();
    *forged.last_mut().unwrap() ^= 1;
    assert!(left
        .receive(source, &forged, 1010, later, &mut left_cache)
        .is_err());
    assert!(!left.is_ready());
    left.receive(source, &response, 1010, later, &mut left_cache)?;
    let mut left_channel = left.take_session().context("initiator channel missing")?;
    assert!(left.take_session().is_none());
    assert!(left.is_ready() && right.is_ready());
    // Responder retries until it knows delivery; an exact duplicate is not a
    // new handshake and cannot reset the extracted channel's replay sequence.
    left.receive(source, &response, 1011, later, &mut left_cache)?;
    assert!(left.take_session().is_none());
    assert!(left
        .receive(source, &forged, 1011, later, &mut left_cache)
        .is_err());
    // A second offer cannot replace this session, even with a valid signature.
    let fresh = NodeHandshakeInitiatorV1::start(
        &ka,
        peer_id_from_mldsa65_public_key_v1(kb.public_key()),
        1011,
        5000,
    )?;
    assert!(right
        .receive(
            a.local_addr()?,
            &handshake_wire::encode_offer(fresh.offer())?,
            1011,
            later,
            &mut right_cache
        )
        .is_err());
    let expired = now + Duration::from_secs(6);
    assert!(!left.try_send(&a, 7000, expired)?);
    assert!(!right.try_send(&b, 7000, expired)?);
    assert!(!left.is_expired(expired) && !right.is_expired(expired));
    let frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        left_channel.session_id(),
        1,
        2,
        3,
        0,
        vec![7; 32],
    );
    assert!(left_channel.try_send(&a, &frame)?);
    let (wire, source) = receive(&b)?;
    assert_eq!(right_channel.open(source, &wire)?, frame);
    // Explicit new attempt still shares replay history, not a fresh verifier.
    let mut another = NativeHandshake::start(
        kb,
        ka.public_key().to_vec(),
        a.local_addr()?,
        1012,
        later,
        Duration::from_secs(5),
        Duration::from_millis(10),
    )?;
    assert!(another
        .receive(a.local_addr()?, &offer, 1012, later, &mut right_cache)
        .is_err());
    Ok(())
}

#[test]
fn pending_handshake_has_bounded_lifetime_and_never_accepts_unknown_identity() -> Result<()> {
    let (a, b) = keys();
    let now = Instant::now();
    let address = "127.0.0.1:1234".parse()?;
    let mut left = NativeHandshake::start(
        a.clone(),
        b.public_key().to_vec(),
        address,
        1000,
        now,
        Duration::from_secs(1),
        Duration::from_millis(10),
    )?;
    assert!(left.is_expired(now + Duration::from_secs(1)));
    assert!(left.take_session().is_none());
    let unknown = crate::test_identity::key(88);
    let offer = NodeHandshakeInitiatorV1::start(
        &unknown,
        peer_id_from_mldsa65_public_key_v1(a.public_key()),
        1000,
        1000,
    )?;
    assert!(left
        .receive(
            address,
            &handshake_wire::encode_offer(offer.offer())?,
            1000,
            now,
            &mut HandshakeReplayCacheV1::default()
        )
        .is_err());
    assert!(!left.is_ready());
    assert!(
        NativeHandshake::start(
            a.clone(),
            vec![47; 32],
            address,
            1000,
            now,
            Duration::from_secs(1),
            Duration::from_millis(10),
        )
        .is_err(),
        "legacy remote key must not be inferred as ML-DSA-65"
    );
    assert!(NativeHandshake::start(
        a.clone(),
        a.public_key().to_vec(),
        address,
        1000,
        now,
        Duration::from_secs(1),
        Duration::from_millis(10)
    )
    .is_err());
    Ok(())
}
