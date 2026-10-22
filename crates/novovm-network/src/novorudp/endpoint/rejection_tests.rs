//! Fixed rejection counters at the real native session/endpoint boundary.
//! Matching a source or an envelope field is not authenticated attribution.
use super::*;
use crate::{novorudp::NovoRudpTransportFrameKindV0, product_overlay::binary};

fn pair() -> Result<(NativeEndpoint, NativeEndpoint)> {
    let left = UdpSocket::bind("127.0.0.1:0")?;
    let right = UdpSocket::bind("127.0.0.1:0")?;
    let addresses = [left.local_addr()?, right.local_addr()?];
    let keys = [71, 72].map(|value| SigningKey::from_bytes(&[value; 32]));
    let start = Instant::now();
    let budget = || NativeHandshakeBudget {
        lifetime: Duration::from_secs(5),
        retry: Duration::from_millis(5),
    };
    let mut a = NativeEndpoint::bind(
        left,
        keys[0].clone(),
        vec![NativePeer {
            key: keys[1].verifying_key().to_bytes(),
            address: addresses[1],
        }],
        budget(),
        1000,
        start,
    )?;
    let mut b = NativeEndpoint::bind(
        right,
        keys[1].clone(),
        vec![NativePeer {
            key: keys[0].verifying_key().to_bytes(),
            address: addresses[0],
        }],
        budget(),
        1000,
        start,
    )?;
    while a.session_id(0).is_none() || b.session_id(0).is_none() {
        ensure!(
            start.elapsed() < Duration::from_secs(5),
            "fixture handshake timeout"
        );
        a.poll(1000, Instant::now(), 8)?;
        b.poll(1000, Instant::now(), 8)?;
        std::thread::yield_now();
    }
    Ok((a, b))
}

fn encoded(endpoint: &mut NativeEndpoint, stream: u64) -> Result<Vec<u8>> {
    let frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        endpoint.session_id(0).unwrap(),
        stream,
        1,
        0,
        0,
        vec![9; 20],
    );
    endpoint
        .peers
        .get_mut(&0)
        .unwrap()
        .session
        .as_mut()
        .unwrap()
        .encode(&frame)
}

fn poll_one(endpoint: &mut NativeEndpoint, wire: &[u8], source: SocketAddr) -> Result<NativePoll> {
    let mut incoming = Some(wire);
    endpoint.poll_with_receiver(1000, Instant::now(), 8, |buffer| {
        let Some(wire) = incoming.take() else {
            return Err(ErrorKind::WouldBlock.into());
        };
        buffer[..wire.len()].copy_from_slice(wire);
        Ok(Some((wire.len(), source)))
    })
}

fn rejected(report: NativePoll, reason: NativeRejectReasonV1) {
    let mut expected = NativeRejectCountsV1::default();
    expected.record(reason);
    assert_eq!(report.rejected_datagrams, 1);
    assert_eq!(report.rejection_reasons.total(), 1);
    assert_eq!(report.rejection_reasons.counts, expected.counts);
    assert!(report.frames.is_empty());
}

#[test]
fn secure_rejection_categories_preserve_authentication_and_replay_checks() -> Result<()> {
    let (mut a, mut b) = pair()?;
    let source = a.local_addr()?;
    for reason in [
        NativeRejectReasonV1::SecureRoute,
        NativeRejectReasonV1::SecureNonce,
        NativeRejectReasonV1::SecureAuthentication,
    ] {
        let good = encoded(&mut a, 17)?;
        let mut envelope = binary::decode(&good, MAX_DATAGRAM_BYTES)?;
        match reason {
            NativeRejectReasonV1::SecureRoute => envelope.session_id[0] ^= 1,
            NativeRejectReasonV1::SecureNonce => envelope.nonce[0] ^= 1,
            NativeRejectReasonV1::SecureAuthentication => envelope.ciphertext[0] ^= 1,
            _ => unreachable!(),
        }
        rejected(
            poll_one(
                &mut b,
                &binary::encode(&envelope, MAX_DATAGRAM_BYTES)?,
                source,
            )?,
            reason,
        );
        let accepted = poll_one(&mut b, &good, source)?;
        assert_eq!(
            accepted.frames.len(),
            1,
            "rejection must not consume valid replay state"
        );
        assert_eq!(accepted.rejection_reasons.total(), 0);
        rejected(
            poll_one(&mut b, &good, source)?,
            NativeRejectReasonV1::SecureReplay,
        );
    }
    Ok(())
}

#[test]
fn endpoint_unknown_decode_and_authenticated_stream_rejections_are_distinct() -> Result<()> {
    let (mut a, mut b) = pair()?;
    let source = a.local_addr()?;
    let unrelated = UdpSocket::bind("127.0.0.1:0")?;
    let wire = encoded(&mut a, 17)?;
    rejected(
        poll_one(&mut b, &wire, unrelated.local_addr()?)?,
        NativeRejectReasonV1::UnknownSource,
    );
    rejected(
        poll_one(&mut b, b"invalid-envelope", source)?,
        NativeRejectReasonV1::WireOrFrame,
    );
    b.peers.get_mut(&0).unwrap().streams = Some(BTreeSet::from([18]));
    rejected(
        poll_one(&mut b, &wire, source)?,
        NativeRejectReasonV1::StreamDenied,
    );
    // Stream denial occurs after successful authentication, so replaying that
    // envelope remains a secure replay rejection, not another stream denial.
    rejected(
        poll_one(&mut b, &wire, source)?,
        NativeRejectReasonV1::SecureReplay,
    );
    Ok(())
}

#[test]
fn pre_session_and_handshake_rejections_do_not_claim_authentication() -> Result<()> {
    let (a, mut b) = pair()?;
    let source = a.local_addr()?;
    b.peers.get_mut(&0).unwrap().session = None;
    rejected(
        poll_one(&mut b, b"not-a-session", source)?,
        NativeRejectReasonV1::NoSession,
    );
    rejected(
        poll_one(&mut b, b"NVHSHK01-invalid", source)?,
        NativeRejectReasonV1::Handshake,
    );
    assert!(b.session_id(0).is_none());
    Ok(())
}

#[test]
fn wire_budget_rejection_keeps_original_precedence_and_byte_charge() -> Result<()> {
    let mut endpoint = NativeEndpoint::listen_clients(
        UdpSocket::bind("127.0.0.1:0")?,
        SigningKey::from_bytes(&[73; 32]),
        1,
        vec![17],
        NativeHandshakeBudget {
            lifetime: Duration::from_secs(5),
            retry: Duration::from_millis(5),
        },
    )?;
    let source = UdpSocket::bind("127.0.0.1:0")?;
    let now = Instant::now();
    endpoint.set_wire_budget_v1(2400, now)?;
    let oversize = vec![0; 2500];
    rejected(
        poll_one(&mut endpoint, &oversize, source.local_addr()?)?,
        NativeRejectReasonV1::WireBudget,
    );
    let usage = endpoint.wire_usage_v1(now).unwrap();
    assert_eq!(usage.received_bytes, 2500);
    assert_eq!(usage.receive_dropped, 1);
    assert_eq!(usage.admitted_receive_bytes, 0);
    // Without a budget, the same packet reaches the original shape boundary.
    let (_, mut unmetered) = pair()?;
    rejected(
        poll_one(&mut unmetered, &oversize, source.local_addr()?)?,
        NativeRejectReasonV1::Oversize,
    );
    Ok(())
}
