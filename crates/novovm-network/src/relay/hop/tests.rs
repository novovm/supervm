use super::*;
use crate::iroh_transport::{with_iroh_scope_v1, IrohConfigV1, IrohControlV1};
use std::time::Duration;
use tokio::sync::oneshot;

fn key(seed: u8) -> TransportIdentity {
    TransportIdentity::from_bytes(&[seed; 32])
}
fn route() -> HopRoute {
    HopRoute::new(
        key(1).public_key().to_vec(),
        vec![key(2).public_key().to_vec()],
        [9; 32],
    )
    .unwrap()
}

#[test]
fn hop_route_canonical_bounds_loops_expiry_and_payload_binding() -> Result<()> {
    let route = route();
    route.validate(route.expires - 30_000 - PRODUCT_OVERLAY_CLOCK_SKEW_MS_V1)?;
    ensure!(route
        .validate(route.expires - 30_001 - PRODUCT_OVERLAY_CLOCK_SKEW_MS_V1)
        .is_err());
    ensure!(route.validate(route.expires).is_err());
    let encoded = route.encode()?;
    ensure!(HopRoute::decode(&encoded)?.digest() == route.digest());
    for i in 0..encoded.len() {
        ensure!(HopRoute::decode(&encoded[..i]).is_err());
    }
    let mut trailing = encoded.clone();
    trailing.push(0);
    ensure!(HopRoute::decode(&trailing).is_err());
    for range in [56..64, 64..65, 65..66] {
        let mut changed = encoded.clone();
        changed[range].fill(255);
        ensure!(HopRoute::decode(&changed).is_err());
    }
    ensure!(HopRoute::new(
        key(1).public_key().to_vec(),
        vec![key(1).public_key().to_vec()],
        [9; 32]
    )
    .is_err());
    ensure!(HopRoute::new(
        key(1).public_key().to_vec(),
        vec![key(2).public_key().to_vec(); 4],
        [9; 32]
    )
    .is_err());
    let two = HopRoute::new(
        key(1).public_key().to_vec(),
        vec![key(2).public_key().to_vec(), key(3).public_key().to_vec()],
        [9; 32],
    )?;
    ensure!(two.forwarded()?.digest() == two.digest());
    ensure!(route.forwarded().is_err());
    let mut payload = DATA.to_vec();
    payload.extend_from_slice(&route.digest());
    payload.extend_from_slice(b"opaque");
    ensure!(unwrap_payload(payload.clone(), true, route.digest())? == b"opaque");
    payload[8] ^= 1;
    ensure!(unwrap_payload(payload, true, route.digest()).is_err());
    Ok(())
}

#[test]
fn hop_budget_is_shared_and_exhaustion_never_means_delivery() -> Result<()> {
    let budget = Arc::new(Mutex::new(NativeWireBudget::new(2400, Instant::now())?));
    let another = budget.clone();
    charge_receive(Some(&budget), 2000)?;
    drop(HeldSend::reserve(Some(&budget), 100)?);
    ensure!(charge_receive(Some(&another), 401).is_err());
    let usage = budget.lock().unwrap().usage(Instant::now());
    ensure!(
        usage.admitted_receive_bytes == 2000
            && usage.receive_dropped == 1
            && usage.sent_bytes == 0
            && usage.uncertain_send_bytes == 100
            && usage.reserved_send_bytes == 0
    );
    let failure = HeldSend::reserve(Some(&budget), 401)
        .err()
        .context("over-budget send accepted")?;
    ensure!(failure.to_string().contains("requested=401"));
    ensure!(failure.to_string().contains("limit: 2400"));
    ensure!(budget.lock().unwrap().usage(Instant::now()).used == 2100);
    Ok(())
}

async fn actual_rejection(case: u8) -> Result<()> {
    let _permit = crate::SERVICE_TEST_SERIAL.acquire().await?;
    let config = || {
        IrohConfigV1::new(&[], "127.0.0.1".parse().unwrap())
            .unwrap()
            .with_bind_address("127.0.0.1:0".parse().unwrap())
            .unwrap()
    };
    let control =
        || IrohControlV1::new(Instant::now() + Duration::from_secs(20), || Ok(())).unwrap();
    let (address_tx, address_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let server = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            address_tx.send(scope.address()?).unwrap();
            let mut stream: CarrierStreamV1<'_> = scope.accept_inbound().await?.into();
            ensure!(stream.read_frame().await? == ENTRY);
            let budget = Arc::new(Mutex::new(NativeWireBudget::new(65536, Instant::now())?));
            if case < 4 {
                let error = match HopStream::accept(stream, &key(2), [9; 32], budget).await {
                    Ok(_) => anyhow::bail!("invalid actual authenticated OPEN accepted"),
                    Err(error) => error,
                };
                ensure!(
                    error.is::<RelayProtocolRejected>(),
                    "wrong failure: {error:#}"
                );
            } else {
                let mut hop = HopStream::accept(stream, &key(2), [9; 32], budget).await?;
                hop.confirm().await?;
                let error = if case == 4 {
                    ensure!(hop.read().await? == b"opaque");
                    hop.read().await.unwrap_err()
                } else {
                    match hop
                        .authenticate_native(&key(2), key(1).public_key(), false)
                        .await
                    {
                        Ok(_) => anyhow::bail!("wrong end-to-end native origin/signature accepted"),
                        Err(error) => error,
                    }
                };
                ensure!(
                    error.is::<RelayProtocolRejected>(),
                    "wrong replay failure: {error:#}"
                );
            }
            done_tx.send(()).unwrap();
            Ok(())
        })
    });
    let client = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            let mut stream: CarrierStreamV1<'_> = scope.connect(&address_rx.await?).await?.into();
            stream.write_frame(ENTRY).await?;
            let mut channel = authenticate_structured_bound_session_observed(
                &mut stream,
                &key(1),
                key(2).public_key().to_vec(),
                true,
                &mut HandshakeReplayCacheV1::new(8),
                &mut |_, _| Ok(()),
            )
            .await?;
            let mut route = route();
            match case {
                0 => route.context = [8; 32],
                1 => route.origin = key(3).public_key().to_vec(),
                2 => route.path[0] = key(3).public_key().to_vec(),
                _ => {}
            }
            let mut payload = route.encode()?;
            if case == 3 {
                payload[56..64].fill(0);
            }
            let session = channel.session_id();
            stream
                .write_frame(&encode_iroh_envelope_v1(
                    &channel.seal_novorudp_frame(&frame(session, payload))?,
                )?)
                .await?;
            if case >= 4 {
                let response = decode_iroh_envelope_v1(&stream.read_frame().await?)?;
                let confirmed = channel.open_novorudp_frame(&response)?;
                ensure!(confirmed.payload.starts_with(READY));
                let mut payload = DATA.to_vec();
                payload.extend_from_slice(&route.digest());
                let inner = if case == 4 {
                    b"opaque".to_vec()
                } else {
                    use crate::product_overlay::{
                        handshake_wire, peer_id_from_mldsa65_public_key_v1,
                        NodeHandshakeInitiatorV1,
                    };
                    let signer = if case == 5 { key(3) } else { key(1) };
                    let pending = NodeHandshakeInitiatorV1::start(
                        &signer,
                        peer_id_from_mldsa65_public_key_v1(key(2).public_key()),
                        wall()?,
                        30_000,
                    )?;
                    let mut offer = pending.offer().clone();
                    if case == 6 {
                        offer.signature[0] ^= 1;
                    }
                    handshake_wire::encode_offer(&offer)?
                };
                payload.extend_from_slice(&inner);
                let wire = encode_iroh_envelope_v1(
                    &channel.seal_novorudp_frame(&frame(session, payload))?,
                )?;
                if case == 4 {
                    stream.write_frame(&wire).await?;
                }
                stream.write_frame(&wire).await?;
            }
            done_rx.await?;
            Ok(())
        })
    });
    let (server, client) = tokio::join!(server, client);
    server?;
    client?;
    Ok(())
}

#[tokio::test]
async fn actual_hop_rejects_wrong_context_sender_target_expiry_replay_and_native_authority(
) -> Result<()> {
    for case in 0..=6 {
        actual_rejection(case).await?;
    }
    Ok(())
}
