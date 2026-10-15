use super::*;
use crate::novorudp::NovoRudpTransportFrameKindV0;

fn listener() -> Result<NativeEndpoint> {
    NativeEndpoint::listen_clients(
        UdpSocket::bind("127.0.0.1:0")?,
        SigningKey::from_bytes(&[93; 32]),
        1,
        vec![17],
        NativeHandshakeBudget {
            lifetime: Duration::from_secs(3),
            retry: Duration::from_millis(10),
        },
    )
}

#[test]
fn scratch_survives_wouldblock_error_and_invalid_length_without_reallocation() -> Result<()> {
    let mut endpoint = listener()?;
    let allocation = endpoint.receive_scratch.as_ptr() as usize;
    let source = endpoint.local_addr()?;
    let now = Instant::now();
    for _ in 0..16 {
        endpoint.poll_with_receiver(1000, now, 64, |wire| {
            assert_eq!(wire.as_ptr() as usize, allocation);
            wire[65535] = 0xa5;
            Err(ErrorKind::WouldBlock.into())
        })?;
        assert_eq!(endpoint.receive_scratch.as_ptr() as usize, allocation);
        assert_eq!(endpoint.receive_scratch[65535], 0xa5);
    }
    assert!(endpoint
        .poll_with_receiver(1000, now, 64, |_| Err(ErrorKind::PermissionDenied.into()))
        .is_err());
    assert_eq!(endpoint.receive_scratch.as_ptr() as usize, allocation);
    assert_eq!(endpoint.receive_scratch.len(), 65536);
    assert!(endpoint
        .poll_with_receiver(1000, now, 0, |_| unreachable!())
        .is_err());
    let report =
        endpoint.poll_with_receiver(1000, now, 1, |wire| Ok(Some((wire.len() + 1, source))))?;
    assert_eq!(report.rejected_datagrams, 1);
    assert!(report.frames.is_empty());
    endpoint.poll_with_receiver(1000, now, 64, |_| Err(ErrorKind::WouldBlock.into()))?;
    assert_eq!(endpoint.receive_scratch.as_ptr() as usize, allocation);
    assert_eq!(endpoint.peer_count(), 0);
    Ok(())
}

#[test]
fn scratch_long_then_short_authenticated_datagrams_use_only_received_prefix() -> Result<()> {
    let mut endpoint = listener()?;
    let now = Instant::now();
    let mut client = NativeEndpoint::connect_client(
        UdpSocket::bind("127.0.0.1:0")?,
        SigningKey::from_bytes(&[94; 32]),
        NativePeer {
            key: SigningKey::from_bytes(&[93; 32]).verifying_key().to_bytes(),
            address: endpoint.local_addr()?,
        },
        NativeHandshakeBudget {
            lifetime: Duration::from_secs(3),
            retry: Duration::from_millis(10),
        },
        1000,
        now,
    )?;
    while endpoint.session_id(0).is_none() || client.session_id(0).is_none() {
        ensure!(now.elapsed() < Duration::from_secs(3), "handshake deadline");
        client.poll(1000, Instant::now(), 64)?;
        endpoint.poll(1000, Instant::now(), 64)?;
    }
    let allocation = endpoint.receive_scratch.as_ptr() as usize;
    let source = client.local_addr()?;
    let mut old_length = 0;
    for (sequence, payload) in [vec![0xd3; 800], vec![0x17; 3]].into_iter().enumerate() {
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            client.session_id(0).unwrap(),
            17,
            2,
            sequence as u64,
            0,
            payload,
        );
        let encrypted = client.peers[0].session.as_mut().unwrap().encode(&frame)?;
        if old_length != 0 {
            assert!(encrypted.len() < old_length);
        }
        let report = endpoint.poll_with_receiver(1000, Instant::now(), 1, |wire| {
            wire[..encrypted.len()].copy_from_slice(&encrypted);
            Ok(Some((encrypted.len(), source)))
        })?;
        assert_eq!(report.rejected_datagrams, 0);
        assert_eq!(report.frames.len(), 1);
        assert_eq!(report.frames[0].frame(), &frame);
        assert_eq!(endpoint.receive_scratch.as_ptr() as usize, allocation);
        old_length = encrypted.len();
    }
    Ok(())
}
