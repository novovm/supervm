//! Real local Iroh IO and the original native binary/evidence consumer.
//! Component signing fixtures, not AOEM/runtime or physical-network acceptance.
use super::*;
use novovm_network::{
    bound_session::authenticate_structured_bound_session_observed,
    carrier_stream::CarrierStreamV1,
    duplex::HandshakeReplayCacheV1,
    iroh_transport::{with_iroh_scope_v1, IrohAddressV1, IrohConfigV1, IrohControlV1},
    novorudp::carrier_io::NativeCarrierPump,
};
use std::{
    future::{poll_fn, Future},
    task::Poll,
};
use tokio::sync::oneshot;

fn config() -> Result<IrohConfigV1> {
    IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)
}
fn control() -> Result<IrohControlV1> {
    IrohControlV1::new(Instant::now() + Duration::from_secs(20), || Ok(()))
}

async fn exchange(
    mut stream: CarrierStreamV1<'_>,
    side: usize,
    original: Option<NativeChannel>,
    done: oneshot::Sender<()>,
    mut peer_done: oneshot::Receiver<()>,
) -> Result<()> {
    let (signing, identities, domain, _) = separated_material()?;
    let bindings = split_bindings(&domain, &signing, &identities)?;
    let channel = authenticate_structured_bound_session_observed(
        &mut stream,
        &identities[side],
        identities[1 - side].public_key().to_vec(),
        side == 0,
        &mut HandshakeReplayCacheV1::new(8),
        &mut |_, _| Ok(()),
    )
    .await?;
    let (endpoint, mut pump) =
        NativeCarrierPump::attach(&mut stream, channel, NativeChannel::carrier_streams())?;
    let endpoint = NativeEndpoint::join_bound_carriers_v1(vec![endpoint])?;
    let mut native = if let Some(mut native) = original {
        let old = native
            .session_id(0)
            .context("original UDP not authenticated")?;
        let report = native.replace_bound_carrier_v1(0, endpoint, Instant::now())?;
        ensure!(report.session_changes.len() == 1 && report.completed.is_empty());
        ensure!(native.session_id(0).is_some_and(|session| session != old));
        ensure!(native.applications, "fallback lost original client ingress");
        ensure!(matches!(native.link, ChannelLink::Original(_)));
        native
    } else {
        NativeChannel::from_bound_carriers(
            endpoint,
            identities[side].public_key(),
            domain,
            bindings,
            codec(),
            split_lanes(),
            NativeSendPolicy::reliable_carrier_v1(Duration::from_secs(10)),
        )?
    };
    let request = transport::Message::RequestBody {
        body_id: [21 + side as u8; 32],
    };
    let prepared = native.prepare(&request)?;
    ensure!(
        native.try_send(0, &prepared, Instant::now())?,
        "initial send not admitted"
    );
    let mut done = Some(done);
    let mut received = false;
    let mut delivered = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if received && delivered {
            if let Some(done) = done.take() {
                let _ = done.send(());
            }
            match peer_done.try_recv() {
                Ok(()) => break,
                Err(oneshot::error::TryRecvError::Closed) => {
                    anyhow::bail!("peer exited before completion")
                }
                Err(oneshot::error::TryRecvError::Empty) => {}
            }
        }
        ensure!(Instant::now() < deadline, "native carrier message deadline");
        // The pump retains partially read/written futures, not this poll branch.
        poll_fn(|cx| match std::pin::pin!(pump.progress()).poll(cx) {
            Poll::Ready(result) => Poll::Ready(result),
            Poll::Pending => Poll::Ready(Ok(())),
        })
        .await?;
        let now = Instant::now();
        let wall = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
        let report = native.poll_io(wall, now, 64)?;
        ensure!(
            report.rejected + report.receive_errors + report.send_failures == 0,
            "valid carrier traffic rejected"
        );
        for complete in report.completed {
            ensure!(
                complete.state == ObjectSendState::Delivered,
                "native carrier send expired"
            );
            delivered = true;
        }
        if let Some(event) = native.poll_receive(now, 64)? {
            match event {
                ReceiveEvent::Message(message) => {
                    ensure!(
                        !received
                            && message.message
                                == transport::Message::RequestBody {
                                    body_id: [22 - side as u8; 32]
                                },
                        "changed or duplicate native message"
                    );
                    ensure!(
                        message.peer == identities[1 - side].peer_id(),
                        "wrong authenticated peer"
                    );
                    received = true;
                }
                ReceiveEvent::Rejected { error, .. } => {
                    anyhow::bail!("native message rejected: {error}")
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    Ok(())
}

#[test]
fn original_native_channel_consumes_authenticated_iroh_carrier() -> Result<()> {
    real_carrier_exchange(false)
}

#[test]
fn original_udp_channel_adopts_real_iroh_fallback_without_losing_client_ingress() -> Result<()> {
    real_carrier_exchange(true)
}

fn authenticated_udp_pair() -> Result<Vec<NativeChannel>> {
    let (signing, identities, domain, _) = separated_material()?;
    let bindings = split_bindings(&domain, &signing, &identities)?;
    let transports = (0..2)
        .map(|side| {
            UdpTransport::bind_for_chain(
                novovm_protocol::NodeId(side + 1),
                "127.0.0.1:0",
                domain.chain_id,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let addresses = transports
        .iter()
        .map(UdpTransport::local_addr)
        .collect::<Result<Vec<_>, _>>()?;
    let mut channels = transports
        .into_iter()
        .enumerate()
        .map(|(side, transport)| {
            NativeChannel::from_transport(
                transport,
                identities[side].clone(),
                bindings.clone(),
                vec![NativePeer {
                    key: identities[1 - side].public_key().to_vec(),
                    address: addresses[1 - side],
                }],
                1,
                domain.clone(),
                codec(),
                split_lanes(),
                NativeSendPolicy::reliable_carrier_v1(Duration::from_secs(10)),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let request = transport::Message::RequestBody { body_id: [43; 32] };
    let prepared = channels
        .iter()
        .map(|channel| channel.prepare(&request))
        .collect::<Result<Vec<_>>>()?;
    let mut sent = [false; 2];
    let mut received = [false; 2];
    let mut completed = 0;
    let deadline = Instant::now() + Duration::from_secs(10);
    while completed != 2 || !received.into_iter().all(|value| value) {
        ensure!(
            Instant::now() < deadline,
            "original UDP authentication/roundtrip deadline"
        );
        let wall = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
        for (side, channel) in channels.iter_mut().enumerate() {
            if !sent[side] {
                sent[side] = channel.try_send(0, &prepared[side], Instant::now())?;
            }
            let report = channel.poll_io(wall, Instant::now(), 64)?;
            for done in report.completed {
                ensure!(done.state == ObjectSendState::Delivered);
                completed += 1;
            }
            if let Some(event) = channel.poll_receive(Instant::now(), 64)? {
                let ReceiveEvent::Message(message) = event else {
                    anyhow::bail!("UDP probe rejected");
                };
                ensure!(!received[side] && message.message == request);
                received[side] = true;
            }
        }
        std::thread::yield_now();
    }
    Ok(channels)
}

fn real_carrier_exchange(replace_udp: bool) -> Result<()> {
    let mut original = if replace_udp {
        authenticated_udp_pair()?
    } else {
        Vec::new()
    };
    let original_b = original.pop();
    let original_a = original.pop();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server_tx, server_rx) = oneshot::channel();
            let (client_tx, client_rx) = oneshot::channel();
            let (done_a, wait_a) = oneshot::channel();
            let (done_b, wait_b) = oneshot::channel();
            let server = with_iroh_scope_v1(config()?, control()?, move |scope| {
                Box::pin(async move {
                    server_tx
                        .send(scope.address()?)
                        .map_err(|_| anyhow::anyhow!("client absent"))?;
                    let peer: IrohAddressV1 = client_rx.await?;
                    exchange(
                        scope.accept(peer.endpoint_key).await?.into(),
                        1,
                        original_b,
                        done_b,
                        wait_a,
                    )
                    .await
                })
            });
            let client = with_iroh_scope_v1(config()?, control()?, move |scope| {
                Box::pin(async move {
                    let peer = server_rx.await?;
                    client_tx
                        .send(scope.address()?)
                        .map_err(|_| anyhow::anyhow!("server absent"))?;
                    exchange(
                        scope.connect(&peer).await?.into(),
                        0,
                        original_a,
                        done_a,
                        wait_b,
                    )
                    .await
                })
            });
            let (server, client) = tokio::join!(server, client);
            server?;
            client
        })
}
