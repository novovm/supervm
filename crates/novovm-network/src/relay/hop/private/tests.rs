use super::*;
use crate::{
    carrier_stream::CarrierStreamV1,
    iroh_transport::{IrohAddressV1, IrohConfigV1, IrohEndpointOwnerV1},
    novorudp::wire_budget::NativeWireBudget,
    relay::hop::{wall, HopBudget},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

use crate::{
    bound_session::peer_time::authenticate_mailbox_native_v8,
    duplex::HandshakeReplayCacheV1,
    endpoint_delivery::EndpointDeliveryResultV1,
    mailbox_rpc::{MailboxReplyV1, MailboxRequestV1, MailboxServiceV1},
    mailbox_store::MailboxStore,
    mailbox_structured::{native_requests::NativeMailboxDriver, STREAM},
    novorudp::carrier_io::NativeCarrierPump,
    product_directory::{sign_relay_record_v1, validate_relay_record_v1},
    product_rendezvous_owned::{mailbox_link_with_resources, serve_private_apfl_v8},
    product_service_lease::{IrohServiceEndpointV1, NativeCarrierMailboxEndpointV8},
    service_concurrency::ServiceConcurrencyV1,
};
use futures_util::FutureExt;
use sha2::Digest;

const SCOPE: [u8; 32] = [9; 32];

struct Circuit {
    identity: [TransportIdentity; 4],
    keys: [PrivateHopKey; 3],
    route: PrivateRoute,
}

impl Circuit {
    fn new(now: u64) -> Result<Self> {
        let identity = [1, 2, 3, 4].map(|byte| TransportIdentity::from_bytes(&[byte; 32]));
        let keys = [
            PrivateHopKey::issue(&identity[1], SCOPE, PrivateHopRole::Forwarder, now)?,
            PrivateHopKey::issue(&identity[2], SCOPE, PrivateHopRole::Forwarder, now)?,
            PrivateHopKey::issue(&identity[3], SCOPE, PrivateHopRole::Terminal, now)?,
        ];
        let verify = |i: usize| {
            PrivateHopDeclaration::verify_wire(
                &keys[i].declaration().encode()?,
                identity[i + 1].public_key(),
                SCOPE,
                now,
                &identity[0],
            )
        };
        let hops = [verify(0)?, verify(1)?, verify(2)?];
        let route = PrivateRoute::new(
            identity[0].public_key(),
            [&hops[0], &hops[1], &hops[2]],
            now,
        )?;
        Ok(Self {
            identity,
            keys,
            route,
        })
    }
}

fn predecessor(circuit: &Circuit, index: usize) -> &[u8] {
    circuit.identity[index].public_key()
}

#[test]
fn declarations_layers_replays_and_terminal_records_are_scoped() -> Result<()> {
    let now = wall()?;
    let mut circuit = Circuit::new(now)?;
    let mut wire = circuit.keys[0].declaration().encode()?;
    wire[KEY_WIRE.len() + 33] ^= 1;
    assert!(PrivateHopDeclaration::verify_wire(
        &wire,
        circuit.identity[1].public_key(),
        SCOPE,
        now,
        &circuit.identity[0],
    )
    .is_err());
    let mut wrong_declaration = circuit.keys[0].declaration().clone();
    wrong_declaration.role = PrivateHopRole::Terminal;
    assert!(wrong_declaration
        .verify(
            circuit.identity[1].public_key(),
            SCOPE,
            now,
            &circuit.identity[0]
        )
        .is_err());
    assert!(circuit.keys[0]
        .declaration()
        .verify(
            circuit.identity[2].public_key(),
            SCOPE,
            now,
            &circuit.identity[0]
        )
        .is_err());
    assert!(circuit.keys[0]
        .declaration()
        .verify(
            circuit.identity[1].public_key(),
            [1; 32],
            now,
            &circuit.identity[0]
        )
        .is_err());
    let ephemeral =
        PrivateHopKey::issue(&circuit.identity[1], SCOPE, PrivateHopRole::Forwarder, now)?;
    assert!(ephemeral
        .open(predecessor(&circuit, 0), &circuit.route.first_layer, now)
        .is_err());
    assert!(circuit.keys[1]
        .open(predecessor(&circuit, 0), &circuit.route.first_layer, now)
        .is_err());
    assert!(circuit.keys[0]
        .open(predecessor(&circuit, 1), &circuit.route.first_layer, now)
        .is_err());
    assert!(circuit.keys[0]
        .open(
            predecessor(&circuit, 0),
            &circuit.route.first_layer,
            now + SETUP_MS
        )
        .is_err());
    let OpenedPrivateHop::Forward {
        next,
        sealed: second,
    } = circuit.keys[0].open(predecessor(&circuit, 0), &circuit.route.first_layer, now)?
    else {
        anyhow::bail!("first key opened as terminal");
    };
    assert_eq!(next, circuit.identity[2].public_key());
    let first_view = circuit.keys[0].secret.open_bounded(
        &circuit.route.first_layer,
        &SCOPE,
        LAYER,
        LAYER_INFO,
        MAX_LAYER,
    )?;
    let second_view = circuit.keys[1]
        .secret
        .open_bounded(&second, &SCOPE, LAYER, LAYER_INFO, MAX_LAYER)?;
    for (view, hidden) in [
        (&first_view, &circuit.identity[3]),
        (&second_view, &circuit.identity[0]),
    ] {
        assert!(!view
            .windows(MLDSA65_PUBLIC_KEY_BYTES)
            .any(|window| window == hidden.public_key()));
    }
    assert_ne!(&first_view[..24], &second_view[..24]);
    assert!(circuit.keys[0]
        .open(predecessor(&circuit, 0), &circuit.route.first_layer, now)
        .is_err());
    assert!(circuit.keys[0]
        .secret
        .open_bounded(&second, &SCOPE, LAYER, LAYER_INFO, MAX_LAYER)
        .is_err());
    let OpenedPrivateHop::Forward { next, sealed: last } =
        circuit.keys[1].open(predecessor(&circuit, 1), &second, now)?
    else {
        anyhow::bail!("second key opened as terminal");
    };
    assert_eq!(next, circuit.identity[3].public_key());
    assert!(circuit.keys[1]
        .secret
        .open_bounded(&last, &SCOPE, LAYER, LAYER_INFO, MAX_LAYER)
        .is_err());
    let OpenedPrivateHop::Terminal(mut terminal) =
        circuit.keys[2].open(predecessor(&circuit, 2), &last, now)?
    else {
        anyhow::bail!("terminal opened as forwarder");
    };
    assert!(circuit.route.endpoint.send(b"before confirmation").is_err());
    let proof = terminal.terminal_ready()?;
    let mut altered = proof.clone();
    altered[0] ^= 1;
    assert!(circuit.route.endpoint.confirm_terminal(&altered).is_err());
    circuit.route.endpoint.confirm_terminal(&proof)?;
    assert!(circuit.route.endpoint.confirm_terminal(&proof).is_err());
    let kind = b"native handshake bytes containing a private identity";
    let ciphertext = circuit.route.endpoint.send(kind)?;
    assert!(ciphertext.windows(kind.len()).all(|window| window != kind));
    assert_eq!(terminal.receive(&ciphertext)?, kind);
    assert!(terminal.receive(&ciphertext).is_err());
    Ok(())
}

fn config() -> Result<IrohConfigV1> {
    IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)
}

fn budget() -> Result<HopBudget> {
    // One forwarder meters both links and both directions of the retained
    // object round trip, including handshakes, framing and control traffic.
    Ok(Arc::new(Mutex::new(NativeWireBudget::new(
        512 * 1024,
        Instant::now(),
    )?)))
}

async fn forwarder(
    key: PrivateHopKey,
    identity: TransportIdentity,
    previous: Vec<u8>,
    successor: Vec<u8>,
    address: oneshot::Sender<IrohAddressV1>,
    next_address: oneshot::Receiver<IrohAddressV1>,
    stop: oneshot::Receiver<()>,
) -> Result<()> {
    let owner = IrohEndpointOwnerV1::bind(config()?, || Ok(())).await?;
    let result = owner
        .with_continuous_service_v4(
            || Ok(()),
            move |scope| {
                Box::pin(async move {
                    address
                        .send(scope.address()?)
                        .map_err(|_| anyhow::anyhow!("forwarder address cancelled"))?;
                    let budget = budget()?;
                    let mut stream: CarrierStreamV1<'_> = scope.accept_inbound().await?.into();
                    assert_eq!(stream.read_frame().await?, *PRIVATE_HOP_ENTRY);
                    let (mut upstream, opened) = PrivateHopLink::accept(
                        stream,
                        &identity,
                        &key,
                        &[previous],
                        budget.clone(),
                    )
                    .await?;
                    let OpenedPrivateHop::Forward { next, sealed } = opened else {
                        anyhow::bail!("forwarder learned terminal material");
                    };
                    assert_eq!(next, successor);
                    let mut downstream = PrivateHopLink::connect(
                        scope.connect(&next_address.await?).await?.into(),
                        &identity,
                        &next,
                        &sealed,
                        Some(budget),
                    )
                    .await?;
                    tokio::select! {
                        biased;
                        stopped = stop => { stopped?; Ok(()) },
                        result = upstream.forward(&mut downstream) => result.context("private fixture forwarder"),
                    }
                })
            },
        )
        .await;
    let cleanup = owner.close().await;
    result.and(cleanup)
}

#[tokio::test]
async fn isolated_records_cross_two_authenticated_forwarders_on_real_carriers() -> Result<()> {
    roundtrip(false).await
}

#[tokio::test]
async fn private_apfl_v8_registers_stores_fetches_and_consumes_original_object() -> Result<()> {
    roundtrip(true).await
}

async fn roundtrip(apfl: bool) -> Result<()> {
    let _permit = crate::SERVICE_TEST_SERIAL.acquire().await?;
    let Circuit {
        identity: [client, first, second, terminal],
        keys: [first_key, second_key, terminal_key],
        route,
    } = Circuit::new(wall()?)?;
    let (address_first, first_in) = oneshot::channel();
    let (address_second, second_in) = oneshot::channel();
    let (address_terminal, terminal_in) = oneshot::channel();
    let (stop_first, first_stop) = oneshot::channel();
    let (stop_second, second_stop) = oneshot::channel();
    let (received, terminal_done) = oneshot::channel();
    let (stop_terminal, terminal_stop) = oneshot::channel();
    let first_run = forwarder(
        first_key,
        first.clone(),
        client.public_key().to_vec(),
        second.public_key().to_vec(),
        address_first,
        second_in,
        first_stop,
    );
    let second_run = forwarder(
        second_key,
        second.clone(),
        first.public_key().to_vec(),
        terminal.public_key().to_vec(),
        address_second,
        terminal_in,
        second_stop,
    );
    let terminal_dir = tempfile::tempdir()?;
    let terminal_owner = IrohEndpointOwnerV1::bind(config()?, || Ok(())).await?;
    let client_owner = IrohEndpointOwnerV1::bind(config()?, || Ok(())).await?;
    let terminal_client = client.public_key().to_vec();
    let node = terminal.public_key().to_vec();
    let terminal_run = terminal_owner.with_continuous_service_v4(
        || Ok(()),
        move |scope| {
            Box::pin(async move {
                address_terminal
                    .send(scope.address()?)
                    .map_err(|_| anyhow::anyhow!("terminal address cancelled"))?;
                let mut stream: CarrierStreamV1<'_> = scope.accept_inbound().await?.into();
                assert_eq!(stream.read_frame().await?, *PRIVATE_HOP_ENTRY);
                let (link, role) = PrivateHopLink::accept(
                    stream,
                    &terminal,
                    &terminal_key,
                    &[second.public_key().to_vec()],
                    budget()?,
                )
                .await?;
                let mut circuit = PrivateCircuit::accept(link, role).await?;
                if apfl {
                    let service = MailboxServiceV1::new(
                        MailboxStore::open(
                            terminal_dir.path().join("private.db"),
                            Default::default(),
                        )?,
                        terminal.clone(),
                    );
                    let endpoint = NativeCarrierMailboxEndpointV8 {
                        iroh: IrohServiceEndpointV1 {
                            instance: [39; 32],
                            address: scope.address()?,
                        },
                    };
                    let now = wall()?;
                    service.update_native_record_v8(sign_relay_record_v1(
                        &terminal,
                        "private-apfl-fixture",
                        vec![endpoint.endpoint()?],
                        now,
                        now + 90_000,
                        1,
                    )?)?;
                    let _device = service.register_endpoint_delivery_v1(
                        [71; 32],
                        [72; 32],
                        Arc::new(|request| {
                            Box::pin(async move {
                                assert_eq!(request.object, [75; 32]);
                                assert_eq!(request.ciphertext, b"device ciphertext");
                                Ok(EndpointDeliveryResultV1::Delivered {
                                    receipt_object: [76; 32],
                                    receipt_expires: request.expires,
                                    receipt: b"opaque fixture device reply".to_vec(),
                                })
                            })
                        }),
                    )?;
                    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let permit =
                        Arc::new(Arc::new(tokio::sync::Semaphore::new(1)).try_acquire_owned()?);
                    let clients = [terminal_client];
                    let result = serve_private_apfl_v8(
                        circuit,
                        &terminal,
                        &clients,
                        &service,
                        stop,
                        permit,
                        ServiceConcurrencyV1::from_host(),
                        || anyhow::bail!("V8 mailbox fixture is not a discovery request"),
                        || Ok(()),
                    );
                    return tokio::select! {
                        biased;
                        stopped = terminal_stop => { stopped?; Ok(()) },
                        outcome = result => outcome,
                    };
                }
                assert_eq!(circuit.read_frame().await?, b"original request");
                circuit.write_frame(b"original response").await?;
                assert_eq!(circuit.read_frame().await?, b"end");
                received
                    .send(())
                    .map_err(|_| anyhow::anyhow!("terminal observer cancelled"))?;
                terminal_stop.await?;
                Ok(())
            })
        },
    );
    let client_run = client_owner.with_continuous_service_v4(
        || Ok(()),
        move |scope| {
            Box::pin(async move {
                let mut circuit = PrivateCircuit::connect(
                    scope.connect(&first_in.await?).await?.into(),
                    &client,
                    route,
                    None,
                )
                .await?;
                if apfl {
                    apfl_roundtrip(circuit, &client, node).await?;
                } else {
                    circuit.write_frame(b"original request").await?;
                    assert_eq!(circuit.read_frame().await?, b"original response");
                    circuit.write_frame(b"end").await?;
                    terminal_done.await?;
                }
                stop_first.send(()).unwrap();
                stop_second.send(()).unwrap();
                stop_terminal.send(()).unwrap();
                Ok(())
            })
        },
    );
    tokio::time::timeout(Duration::from_secs(30), async {
        tokio::try_join!(first_run, second_run, terminal_run, client_run)
    })
    .await??;
    terminal_owner.close().await?;
    client_owner.close().await?;
    Ok(())
}

async fn apfl_roundtrip(
    circuit: PrivateCircuit<'_>,
    client: &TransportIdentity,
    node: Vec<u8>,
) -> Result<()> {
    let mut stream = CarrierStreamV1::from(circuit);
    let (channel, _) = authenticate_mailbox_native_v8(
        &mut stream,
        client,
        node.clone(),
        Instant::now() + Duration::from_secs(20),
        &mut HandshakeReplayCacheV1::new(8),
        &mut |_, _| Ok(()),
    )
    .await?;
    let (endpoint, mut pump) =
        NativeCarrierPump::attach_continuous_v7(&mut stream, channel, vec![STREAM])?;
    let resources = ServiceConcurrencyV1::from_host();
    let permit = resources
        .try_reserve_transport()
        .context("fixture transport admission")?;
    let mut link = mailbox_link_with_resources(endpoint, true, Some(&resources), Some(&permit))?;
    let (mut driver, mailbox) =
        NativeMailboxDriver::attach_adaptive(&mut link, 0, node.clone(), resources)?;
    let expires = i64::try_from(wall()? / 1000)? + 300;
    let (box_id, write, read, object) = ([71; 32], [72; 32], [73; 32], [74; 32]);
    let bytes: Vec<u8> = (0..40 * 1024).map(|index| (index % 251) as u8).collect();
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    let work = async {
        let run = |request| {
            mailbox.request(
                0,
                vec![request],
                Instant::now() + Duration::from_secs(5),
                Arc::new(|| Ok(())),
            )
        };
        let discovery = run(MailboxRequestV1::ServiceRecordV7).await?;
        let [MailboxReplyV1::ServiceRecordV7 { record }] = discovery.as_slice() else {
            anyhow::bail!("private APFL signed locator missing");
        };
        assert_eq!(record.relay_public_key, node);
        validate_relay_record_v1(record, wall()?, client)?;
        assert!(NativeCarrierMailboxEndpointV8::from_record(record)?.is_some());
        assert!(matches!(
            run(MailboxRequestV1::Register {
                mailbox: box_id,
                write,
                read,
                expires
            })
            .await?
            .as_slice(),
            [MailboxReplyV1::Registered]
        ));
        let stored = run(MailboxRequestV1::Put {
            mailbox: box_id,
            write,
            object,
            ciphertext: bytes.clone(),
            expires,
        })
        .await?;
        let [MailboxReplyV1::Stored(receipt)] = stored.as_slice() else {
            anyhow::bail!("private APFL custody missing");
        };
        receipt.verify(&node, box_id, object, &bytes, expires, client)?;
        let got = run(MailboxRequestV1::Fetch {
            mailbox: box_id,
            read,
        })
        .await?;
        let [MailboxReplyV1::Object {
            object: from_node,
            ciphertext,
            digest: observed,
            expires: until,
        }] = got.as_slice()
        else {
            anyhow::bail!("private APFL retained object missing");
        };
        assert_eq!(
            (from_node, ciphertext, observed, until),
            (&object, &bytes, &digest, &expires)
        );
        assert!(matches!(
            run(MailboxRequestV1::Consume {
                mailbox: box_id,
                read,
                object,
                digest
            })
            .await?
            .as_slice(),
            [MailboxReplyV1::Consumed]
        ));
        assert!(matches!(
            run(MailboxRequestV1::Fetch {
                mailbox: box_id,
                read
            })
            .await?
            .as_slice(),
            [MailboxReplyV1::Empty]
        ));
        let delivery = run(MailboxRequestV1::EndpointDeliverV1 {
            mailbox: box_id,
            write,
            object: [75; 32],
            ciphertext: b"device ciphertext".to_vec(),
            expires,
        })
        .await?;
        let [MailboxReplyV1::EndpointDeliveredV1 {
            object,
            receipt_object,
            receipt_expires,
            receipt,
        }] = delivery.as_slice()
        else {
            anyhow::bail!("private APFL device reply missing");
        };
        assert_eq!(
            (
                *object,
                *receipt_object,
                *receipt_expires,
                receipt.as_slice()
            ),
            (
                [75; 32],
                [76; 32],
                expires,
                b"opaque fixture device reply".as_slice()
            )
        );
        Ok(())
    };
    tokio::pin!(work);
    loop {
        if let Some(progress) = pump.progress().now_or_never() {
            progress?;
        }
        driver.progress(&mut link, Instant::now())?;
        let progress = link.poll(wall()?, Instant::now(), 64)?;
        for done in progress.completed {
            driver.transport_complete(&done);
        }
        while let Some(body) = link.poll_complete(STREAM, Instant::now(), 64)? {
            driver.receive(&link, body, Instant::now())?;
        }
        tokio::select! {
            biased;
            result = &mut work => return result,
            progress = pump.progress() => progress?,
            _ = tokio::time::sleep(Duration::from_millis(5)) => {}
        }
    }
}
