use super::*;
use crate::novorudp::endpoint::{NativeEndpoint, NativeHandshakeBudget, NativePeer};
use crate::novorudp::{NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0};

#[test]
fn native_endpoint_first_contact_handles_clock_skew_and_jumps_without_os_sync() -> Result<()> {
    for reversed in [false, true] {
        let server_key = crate::test_identity::key(81);
        let client_key = crate::test_identity::key(82);
        let budget = || NativeHandshakeBudget {
            lifetime: Duration::from_secs(2),
            retry: Duration::from_millis(10),
        };
        let mut server = NativeEndpoint::listen_clients(
            UdpSocket::bind("127.0.0.1:0")?,
            server_key.clone(),
            1,
            vec![17],
            budget(),
        )?;
        let start = Instant::now();
        let mut client = NativeEndpoint::connect_client(
            UdpSocket::bind("127.0.0.1:0")?,
            client_key,
            NativePeer {
                key: server_key.public_key().to_vec(),
                address: server.local_addr()?,
            },
            budget(),
            if reversed { 0 } else { u64::MAX - 100 },
            start,
        )?;
        let mut tick = 0;
        while client.session_id(0).is_none() || server.session_id(0).is_none() {
            ensure!(
                start.elapsed() < Duration::from_secs(2),
                "native skew bootstrap stalled"
            );
            let wall = if (tick % 2 == 0) ^ reversed {
                0
            } else {
                u64::MAX - 100
            };
            client.poll(wall, Instant::now(), 64)?;
            server.poll(u64::MAX - wall, Instant::now(), 64)?;
            tick += 1;
        }
        assert_eq!(client.session_id(0), server.session_id(0));
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            client.session_id(0).unwrap(),
            17,
            1,
            0,
            0,
            vec![5; 16],
        );
        assert!(client.try_send(0, &frame)?);
        let mut received = false;
        while !received {
            ensure!(
                start.elapsed() < Duration::from_secs(2),
                "post-Finished frame missing"
            );
            let report = server.poll(0, Instant::now(), 64)?;
            for got in report.frames {
                assert_eq!(got.frame(), &frame);
                received = true;
            }
        }
    }
    Ok(())
}

// The state machine is real; only delivery of signed offer/response is direct
// to isolate liveness, replay and cancellation. Finished uses actual UDP sends.
fn pending_pair() -> Result<(
    NativeHandshake,
    NativeHandshake,
    UdpSocket,
    UdpSocket,
    Instant,
    Vec<u8>,
)> {
    let a = UdpSocket::bind("127.0.0.1:0")?;
    let b = UdpSocket::bind("127.0.0.1:0")?;
    a.set_nonblocking(true)?;
    b.set_nonblocking(true)?;
    let ka = crate::test_identity::key(83);
    let kb = crate::test_identity::key(84);
    let now = Instant::now();
    let mut left = NativeHandshake::start_role(
        ka.clone(),
        kb.public_key().to_vec(),
        b.local_addr()?,
        1,
        now,
        Duration::from_secs(2),
        Duration::from_millis(10),
        Some(true),
    )?;
    let mut right = NativeHandshake::start_role(
        kb,
        ka.public_key().to_vec(),
        a.local_addr()?,
        999_999,
        now,
        Duration::from_secs(2),
        Duration::from_millis(10),
        Some(false),
    )?;
    let offer = left.outgoing.clone().unwrap();
    right.receive(
        a.local_addr()?,
        &offer,
        999_999,
        now,
        &mut HandshakeReplayCacheV1::default(),
    )?;
    let response = right.outgoing.clone().unwrap();
    left.receive(
        b.local_addr()?,
        &response,
        1,
        now,
        &mut HandshakeReplayCacheV1::default(),
    )?;
    Ok((left, right, a, b, now, offer))
}

#[test]
fn native_finished_loss_tamper_replay_and_duplicate_flood_preserve_deadline() -> Result<()> {
    let (mut left, mut right, a, b, now, offer) = pending_pair()?;
    let mut cache = HandshakeReplayCacheV1::default();
    assert!(left.take_session().is_none() && right.take_session().is_none());
    let deadline = right.deadline;
    let retry = right.next_send;
    for _ in 0..1000 {
        right.receive(a.local_addr()?, &offer, 0, now, &mut cache)?;
    }
    assert_eq!(right.deadline, deadline);
    assert_eq!(right.next_send, retry);
    assert!(left.try_send(&a, 0, now)?);
    let (lost, _) = super::tests::receive(&b)?;
    assert!(left.try_send(&a, u64::MAX, now + Duration::from_millis(10))?);
    let (finished, source) = super::tests::receive(&b)?;
    assert_eq!(lost, finished, "retries do not reseal/reset sequence");
    let mut bad = finished.clone();
    *bad.last_mut().unwrap() ^= 1;
    assert!(right.receive(source, &bad, 0, now, &mut cache).is_err());
    assert!(right.take_session().is_none());
    right.receive(source, &finished, 0, now, &mut cache)?;
    assert!(
        right.take_session().is_none(),
        "generation is not successful send"
    );
    assert!(right.try_send(&b, u64::MAX, now)?);
    let (lost_reply, _) = super::tests::receive(&a)?;
    assert!(left.take_session().is_none());
    assert!(right.try_send(&b, 0, now + Duration::from_millis(10))?);
    let (reply, source) = super::tests::receive(&a)?;
    assert_eq!(lost_reply, reply, "responder Finished retries unchanged");
    left.receive(
        source,
        &reply,
        0,
        now + Duration::from_millis(10),
        &mut cache,
    )?;
    assert!(left.take_session().is_some() && right.take_session().is_some());
    right.receive(a.local_addr()?, &finished, 0, now, &mut cache)?;
    assert!(
        right.take_session().is_none(),
        "duplicate cannot recreate a channel"
    );

    // Evicted replay history still cannot activate an orphaned responder:
    // replayed Offer yields a fresh response secret, old Finished is unusable.
    let mut fresh = NativeHandshake::start_role(
        right.identity.clone(),
        left.identity.public_key().to_vec(),
        a.local_addr()?,
        0,
        now,
        Duration::from_secs(2),
        Duration::from_millis(10),
        Some(false),
    )?;
    fresh.receive(
        a.local_addr()?,
        &offer,
        0,
        now,
        &mut HandshakeReplayCacheV1::default(),
    )?;
    assert!(fresh
        .receive(a.local_addr()?, &finished, 0, now, &mut cache)
        .is_err());
    assert!(fresh.take_session().is_none());
    assert!(fresh
        .receive(a.local_addr()?, &finished, 0, deadline, &mut cache)
        .is_err());
    assert!(fresh.is_expired(deadline));
    Ok(())
}

#[test]
fn native_session_profile_rejects_utc_downgrade_and_unsent_finished() -> Result<()> {
    let (mut left, mut right, a, b, now, _) = pending_pair()?;
    let old =
        NodeHandshakeInitiatorV1::start(&left.identity, right.identity.peer_id(), 1000, 2000)?;
    let mut fresh = NativeHandshake::start_role(
        right.identity.clone(),
        left.identity.public_key().to_vec(),
        a.local_addr()?,
        1000,
        now,
        Duration::from_secs(2),
        Duration::from_millis(10),
        Some(false),
    )?;
    assert!(fresh
        .receive(
            a.local_addr()?,
            &handshake_wire::encode_offer(old.offer())?,
            1000,
            now,
            &mut HandshakeReplayCacheV1::default()
        )
        .is_err());
    // Accepting the new wire shape must not reinterpret a legacy signature.
    let mut relabeled = old.offer().clone();
    relabeled.version = NATIVE_SESSION_VERSION;
    relabeled.issued_at_ms = 0;
    relabeled.expires_at_ms = 2000;
    assert!(fresh
        .receive(
            a.local_addr()?,
            &handshake_wire::encode_offer(&relabeled)?,
            1000,
            now,
            &mut HandshakeReplayCacheV1::default()
        )
        .is_err());
    let finished = left.outgoing.clone().unwrap();
    right.receive(
        a.local_addr()?,
        &finished,
        1000,
        now,
        &mut HandshakeReplayCacheV1::default(),
    )?;
    let reply = right.outgoing.clone().unwrap();
    left.receive(
        b.local_addr()?,
        &reply,
        1000,
        now,
        &mut HandshakeReplayCacheV1::default(),
    )?;
    assert!(left.take_session().is_none() && right.take_session().is_none());
    let deadline = now + Duration::from_secs(2);
    assert!(left.try_send(&a, 0, deadline).is_err());
    assert!(right.try_send(&b, 0, deadline).is_err());
    assert!(left.take_session().is_none() && right.take_session().is_none());
    Ok(())
}
