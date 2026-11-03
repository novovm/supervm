//! Binding-material unit fixtures only, not a real TLS/Iroh network test.
//! No raw route constructor is exposed to SDK consumers.
use super::*;
use crate::{
    novorudp::{
        endpoint::{NativeEndpoint, NativeHandshakeBudget, NativeRoutePeer},
        handshake::NativeHandshake,
    },
    transport_binding::BOUND_TRANSPORT_ALPN_V1,
};
use anyhow::Result;
use ed25519_dalek::SigningKey;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn binding(exporter: u8, remote: u8) -> Result<VerifiedTransportBinding> {
    Ok(VerifiedTransportBinding::from_authenticated_transport(
        SigningKey::from_bytes(&[11; 32]).verifying_key().to_bytes(),
        SigningKey::from_bytes(&[remote; 32])
            .verifying_key()
            .to_bytes(),
        [exporter; 32],
        BOUND_TRANSPORT_ALPN_V1,
        true,
    )?)
}
#[test]
fn authenticated_route_provenance_does_not_authorize_bare_native_handshake() -> Result<()> {
    let first = VerifiedCarrierRoute::from_binding(&binding(31, 12)?);
    let next_connection = VerifiedCarrierRoute::from_binding(&binding(32, 12)?);
    let other_peer = VerifiedCarrierRoute::from_binding(&binding(31, 13)?);
    assert_eq!(
        first.peer_key(),
        SigningKey::from_bytes(&[12; 32]).verifying_key().to_bytes()
    );
    assert_ne!(
        first.connection_binding(),
        next_connection.connection_binding()
    );
    assert_ne!(first, other_peer);
    let route = NativeRoute::Carrier(first);
    assert!(route.validate().is_err());
    assert_ne!(route, NativeRoute::Udp("127.0.0.1:443".parse()?));
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    socket.set_nonblocking(true)?;
    assert_eq!(
        socket.try_send(&[1], route).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(NativeEndpoint::connect_clients_with_io(
        socket,
        crate::test_identity::key(21),
        vec![NativeRoutePeer {
            key: crate::test_identity::key(22).public_key().to_vec(),
            route
        }],
        NativeHandshakeBudget {
            lifetime: Duration::from_secs(2),
            retry: Duration::from_millis(10)
        },
        1000,
        Instant::now()
    )
    .is_err());
    assert!(NativeHandshake::start_role(
        crate::test_identity::key(21),
        crate::test_identity::key(22).public_key().to_vec(),
        route,
        1000,
        Instant::now(),
        Duration::from_secs(2),
        Duration::from_millis(10),
        Some(true)
    )
    .is_err());
    Ok(())
}
