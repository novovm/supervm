//! Actual localhost QUIC, structured handshake and the original native envelope.
//! This does not exercise the not-yet-wired asynchronous NativeObjectLink IO pump.
use super::*;
use crate::novorudp::{
    carrier::{NativeRoute, VerifiedCarrierRoute},
    datagram::{NativeUdpSession, MAX_DATAGRAM_BYTES},
};

async fn native_roundtrip(open: bool) {
    let _permit = SERIAL.acquire().await.unwrap();
    let (server_tx, server_rx) = oneshot::channel();
    let (client_tx, client_rx) = oneshot::channel();
    let server = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            server_tx.send(scope.address()?).unwrap();
            let peer: IrohAddressV1 = client_rx.await?;
            let mut stream: CarrierStreamV1<'_> = scope.accept(peer.endpoint_key).await?.into();
            let binding = stream.binding()?;
            let mut replay = HandshakeReplayCacheV1::new(8);
            let channel = if open {
                authenticate_structured_service_client_observed(
                    &mut stream,
                    &key(2),
                    ServicePeerPolicyV1::AnyAuthenticated,
                    &mut replay,
                    &mut |_, _| Ok(()),
                )
                .await?
                .0
            } else {
                authenticate_structured_bound_session_observed(
                    &mut stream,
                    &key(2),
                    public(1),
                    false,
                    &mut replay,
                    &mut |_, _| Ok(()),
                )
                .await?
            };
            let mut native = NativeUdpSession::from_bound_channel(channel, &binding)?;
            let route = NativeRoute::Carrier(VerifiedCarrierRoute::from_binding(&binding));
            let session = native.session_id();
            for sequence in 0..8 {
                let wire = stream.read_frame().await?;
                ensure!(wire.starts_with(b"NVRLY002") && wire.len() <= MAX_DATAGRAM_BYTES);
                let envelope = crate::product_overlay::binary::decode(&wire, MAX_DATAGRAM_BYTES)?;
                // Finished consumes sequence zero. Adoption must not reset it.
                ensure!(envelope.sequence == sequence + 1);
                let frame = native.open_route(route, &wire)?;
                ensure!(frame == payload(session, sequence));
                ensure!(native.open_route(route, &wire).is_err(), "replay accepted");
                stream.write_frame(&native.encode(&frame)?).await?;
            }
            stream.finish().await?;
            Ok(session)
        })
    });
    let client = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            client_tx.send(scope.address()?).unwrap();
            let mut stream: CarrierStreamV1<'_> = scope.connect(&server_rx.await?).await?.into();
            let binding = stream.binding()?;
            let channel = authenticate_structured_bound_session_observed(
                &mut stream,
                &key(1),
                public(2),
                true,
                &mut HandshakeReplayCacheV1::new(8),
                &mut |_, _| Ok(()),
            )
            .await?;
            let mut native = NativeUdpSession::from_bound_channel(channel, &binding)?;
            let route = NativeRoute::Carrier(VerifiedCarrierRoute::from_binding(&binding));
            let session = native.session_id();
            for sequence in 0..8 {
                let frame = payload(session, sequence);
                let wire = native.encode(&frame)?;
                ensure!(wire.starts_with(b"NVRLY002") && wire.len() <= MAX_DATAGRAM_BYTES);
                stream.write_frame(&wire).await?;
                let echo = stream.read_frame().await?;
                ensure!(native.open_route(route, &echo)? == frame);
            }
            stream.finish().await?;
            Ok(session)
        })
    });
    let (server, client) = tokio::join!(server, client);
    assert_eq!(server.unwrap(), client.unwrap());
}

fn payload(session: [u8; 16], sequence: u64) -> NovoRudpTransportFrameV0 {
    NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        session,
        11,
        sequence,
        sequence,
        0,
        vec![(sequence + 1) as u8; 848],
    )
}

#[tokio::test]
async fn structured_native_quic_reuses_completed_pinned_session() {
    native_roundtrip(false).await;
}

#[tokio::test]
async fn structured_native_quic_reuses_completed_open_service_session() {
    native_roundtrip(true).await;
}
