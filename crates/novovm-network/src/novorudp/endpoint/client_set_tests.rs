//! Real loopback UDP exercises the original peer state machines, not NAT or
//! mobile acceptance. There is still one client socket and one receive owner.
use super::*;
use crate::novorudp::NovoRudpTransportFrameKindV0;

fn budget() -> NativeHandshakeBudget {
    NativeHandshakeBudget {
        lifetime: Duration::from_secs(3),
        retry: Duration::from_millis(20),
    }
}
fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

#[test]
fn connect_clients_rejects_empty_oversize_and_duplicate_bindings() -> Result<()> {
    let socket = || UdpSocket::bind("127.0.0.1:0");
    let remote = NativePeer {
        key: key(1).verifying_key().to_bytes(),
        address: "127.0.0.1:12001".parse()?,
    };
    let connect = |peers| {
        NativeEndpoint::connect_clients(socket()?, key(250), peers, budget(), 1000, Instant::now())
    };
    assert!(connect(vec![]).is_err());
    assert!(connect(vec![remote.clone(); 65]).is_err());
    let mut repeated_key = remote.clone();
    repeated_key.address.set_port(12002);
    assert!(connect(vec![remote.clone(), repeated_key]).is_err());
    let mut repeated_address = remote.clone();
    repeated_address.key = key(2).verifying_key().to_bytes();
    assert!(connect(vec![remote, repeated_address]).is_err());
    let maximum = (1..=64)
        .map(|seed| NativePeer {
            key: key(seed).verifying_key().to_bytes(),
            address: SocketAddr::from(([127, 0, 0, 1], 12000 + u16::from(seed))),
        })
        .collect();
    assert_eq!(connect(maximum)?.peer_count(), 64);
    Ok(())
}

#[test]
fn connect_clients_one_socket_authenticates_live_services_around_silent_peer() -> Result<()> {
    // Exercise BOTH lexical key orders: open services always need a client
    // initiator, regardless of the role selection used by validator bind().
    let mut keys: Vec<_> = (21..=28).map(key).collect();
    keys.sort_by_key(|key| key.verifying_key().to_bytes());
    let client_key = keys[4].clone();
    let mut servers = Vec::new();
    for identity in [keys[0].clone(), keys[7].clone()] {
        servers.push(NativeEndpoint::listen_clients(
            UdpSocket::bind("127.0.0.1:0")?,
            identity,
            1,
            vec![17],
            budget(),
        )?);
    }
    let silent = UdpSocket::bind("127.0.0.1:0")?;
    let peers = vec![
        NativePeer {
            key: servers[0].local_key,
            address: servers[0].local_addr()?,
        },
        NativePeer {
            key: key(240).verifying_key().to_bytes(),
            address: silent.local_addr()?,
        },
        NativePeer {
            key: servers[1].local_key,
            address: servers[1].local_addr()?,
        },
    ];
    let mut client = NativeEndpoint::connect_clients(
        UdpSocket::bind("127.0.0.1:0")?,
        client_key.clone(),
        peers,
        budget(),
        1000,
        Instant::now(),
    )?;
    let source = client.local_addr()?;
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        ensure!(Instant::now() < until, "live service handshake blocked");
        // A one-item send quantum must fairly visit the third peer even while
        // the second endpoint never replies.
        client.poll(1000, Instant::now(), 1)?;
        for server in &mut servers {
            server.poll(1000, Instant::now(), 1)?;
        }
        if [0, 2].iter().all(|i| client.session_id(*i).is_some())
            && servers.iter().all(|server| server.session_id(0).is_some())
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(client.session_id(1).is_none());
    assert_eq!(client.local_addr()?, source);
    for (ordinal, server) in servers.iter_mut().enumerate() {
        assert_eq!(server.sources.get(&NativeRoute::Udp(source)), Some(&0));
        assert_eq!(
            server.peer_id(0)?,
            peer_id_from_ed25519_public_key_v1(&client_key.verifying_key().to_bytes())
        );
        let index = ordinal * 2;
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            client.session_id(index).unwrap(),
            17,
            1,
            1,
            0,
            vec![ordinal as u8; 8],
        );
        assert!(client.try_send(index, &frame)?);
    }
    let mut received = [false; 2];
    let until = Instant::now() + Duration::from_secs(3);
    while received.iter().any(|v| !*v) {
        ensure!(Instant::now() < until, "service data stalled");
        for (ordinal, server) in servers.iter_mut().enumerate() {
            for frame in server.poll(1000, Instant::now(), 1)?.frames {
                assert_eq!(frame.peer(), 0);
                assert_eq!(frame.frame().payload, vec![ordinal as u8; 8]);
                received[ordinal] = true;
                assert!(server.try_send(0, frame.frame())?);
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut returned = BTreeSet::new();
    while returned.len() < 2 {
        ensure!(Instant::now() < until, "client multiplex receive stalled");
        for frame in client.poll(1000, Instant::now(), 1)?.frames {
            assert!([0, 2].contains(&frame.peer()));
            assert_eq!(frame.frame().payload, vec![(frame.peer() / 2) as u8; 8]);
            returned.insert(frame.peer());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(client.peer_count(), 3);
    assert!(client.session_id(1).is_none());
    Ok(())
}
