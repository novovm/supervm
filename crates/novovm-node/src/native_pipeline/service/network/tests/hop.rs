use super::*;

#[test]
fn actual_two_relay_native_objects_use_both_physical_hops_and_complete_concurrent_work(
) -> Result<()> {
    run_relay_objects(2)
}

#[test]
fn actual_single_relay_fixed_probe_objects_done_and_reauthenticate() -> Result<()> {
    run_relay_objects(1)
}

fn run_relay_objects(relay_count: usize) -> Result<()> {
    use novovm_network::novorudp::carrier::NativeRoute;
    let count = relay_count + 2;
    let mut peers = Pair::configured(false, true, count)?;
    let root = peers.drivers[0]
        .config
        .public_record_file
        .parent()
        .unwrap()
        .to_owned();
    let source = (0..count)
        .min_by_key(|i| peers.identities[*i].public_key())
        .unwrap();
    let target = (0..count)
        .max_by_key(|i| peers.identities[*i].public_key())
        .unwrap();
    let relays = (0..count)
        .filter(|i| *i != source && *i != target)
        .collect::<Vec<_>>();
    let peer_index = |side: usize, other: usize| if other < side { other } else { other - 1 };
    let mut configs = peers
        .drivers
        .iter()
        .map(|d| d.config.clone())
        .collect::<Vec<_>>();
    for (side, config) in configs.iter_mut().enumerate() {
        config.discovery = false;
        config.discovery_seeds.clear();
        config.relay_forwarding = true;
        config.peer_records = (0..count)
            .filter(|other| *other != side)
            .map(|other| root.join(format!("leg-{side}-{other}.json")))
            .collect();
    }
    let observers = (0..count)
        .map(|_| Arc::new(NativeTransferDiagnosticsV1::new()))
        .collect::<Vec<_>>();
    peers.restart_drivers(configs.clone(), Some(&observers))?;
    for driver in &peers.drivers {
        for view in driver.views.lock().unwrap().iter_mut() {
            view.fallback = false;
        }
    }
    peers.drivers[source].views.lock().unwrap()[peer_index(source, target)].fallback = true;
    let publish_by = Instant::now() + Duration::from_secs(15);
    while configs.iter().any(|c| !c.public_record_file.exists()) {
        ensure!(
            Instant::now() < publish_by,
            "relay fixture locator not ready"
        );
        peers.pump()?;
    }
    let original = configs
        .iter()
        .map(|c| read_document::<PeerSignedRelayRecordV1>(&c.public_record_file))
        .collect::<Result<Vec<_>>>()?;
    let unreachable = UdpSocket::bind("127.0.0.1:0")?;
    let identities = peers.identities.clone();
    let publish_legs = |revision| -> Result<()> {
        for side in 0..count {
            for other in (0..count).filter(|other| *other != side) {
                let mut endpoint = IrohServiceEndpointV1::from_record(&original[other])?;
                // Real source-specific stale coordinates force A -> R1 -> R2 -> D.
                // No fake success, mocked IO, altered authentication or test-only
                // route shortcut. Each failed dial uses the actual carrier owner.
                if (side == source && (other == target || (relay_count == 2 && other == relays[1])))
                    || (relay_count == 2 && side == relays[0] && other == target)
                {
                    endpoint.address.direct = vec![unreachable.local_addr()?];
                    endpoint.address.relays.clear();
                }
                let wall = wall_ms()?;
                let record = sign_relay_record_v1(
                    &identities[other],
                    original[other].record_id.clone(),
                    vec![endpoint.endpoint()?],
                    wall,
                    wall + 120_000,
                    original[other].sequence + revision,
                )?;
                publish(&root.join(format!("leg-{side}-{other}.json")), &record)?;
            }
        }
        Ok(())
    };
    publish_legs(1)?;
    let deadline = Instant::now() + Duration::from_secs(110);
    let from = peer_index(source, target);
    let to = peer_index(target, source);
    while peers.channels[source].session_id(from).is_none()
        || peers.channels[target].session_id(to).is_none()
    {
        ensure!(
            Instant::now() < deadline,
            "actual two-relay circuit did not authenticate: failures={:?}",
            peers
                .drivers
                .iter()
                .map(|d| d.failures.load(Ordering::Relaxed))
                .collect::<Vec<_>>()
        );
        peers.pump()?;
    }
    let NativeRoute::Carrier(first) = peers.channels[source].peer_route(from)? else {
        anyhow::bail!("source did not adopt actual carrier")
    };
    let NativeRoute::Carrier(last) = peers.channels[target].peer_route(to)? else {
        anyhow::bail!("target did not adopt actual carrier")
    };
    ensure!(
        first.peer_key()
            == IrohServiceEndpointV1::from_record(&original[relays[0]])?
                .address
                .endpoint_key
    );
    ensure!(
        last.peer_key()
            == IrohServiceEndpointV1::from_record(&original[*relays.last().unwrap()])?
                .address
                .endpoint_key
    );
    ensure!(peers.channels[source].session_id(from) == peers.channels[target].session_id(to));
    complete_objects(&mut peers, source, target, 191, deadline)?;
    // SAME actual IO as the existing delivery regression, not a simulated
    // route report. Every relay independently observed completed writes.
    let source_snapshot = observers[source]
        .snapshot()
        .context("source observation contended")?;
    let session = peers.channels[source]
        .session_id(from)
        .context("source session")?;
    let source_circuit = source_snapshot
        .relay_circuits
        .iter()
        .find(|r| r.native_session == Some(session))
        .context("no authenticated source circuit")?;
    ensure!(
        source_circuit.route.role == "origin" && source_circuit.route.path.len() == relay_count + 1
    );
    let target_snapshot = observers[target]
        .snapshot()
        .context("target observation contended")?;
    let target_circuit = target_snapshot
        .relay_circuits
        .iter()
        .find(|r| r.route.route_digest == source_circuit.route.route_digest)
        .context("no matching final circuit")?;
    ensure!(
        target_circuit.route.role == "target" && target_circuit.native_session == Some(session)
    );
    for (cursor, side) in relays.iter().enumerate() {
        let snapshot = observers[*side]
            .snapshot()
            .context("relay observation contended")?;
        let circuit = snapshot
            .relay_circuits
            .iter()
            .find(|r| r.route.route_digest == source_circuit.route.route_digest)
            .context("actual intermediate relay evidence missing")?;
        ensure!(circuit.route.role == "relay" && circuit.route.cursor == cursor);
        let next = if cursor + 1 < relays.len() {
            relays[cursor + 1]
        } else {
            target
        };
        ensure!(
            circuit
                .downstream
                .context("relay actual next carrier missing")?
                .peer_key()
                == IrohServiceEndpointV1::from_record(&original[next])?
                    .address
                    .endpoint_key
        );
        ensure!(
            circuit.route.origin == source_circuit.route.origin
                && circuit.route.path == source_circuit.route.path
        );
        ensure!(
            circuit.native_session.is_none(),
            "relay must not acquire E2E authority"
        );
        ensure!(
            circuit.downstream_ready_us.is_some()
                && circuit
                    .forwarded
                    .iter()
                    .all(|d| d.packets > 0 && d.bytes > 0)
        );
        ensure!(circuit.failed_writes == [0; 2]);
    }
    let old_session = peers.channels[source].session_id(from);
    // Deliberately separate recovery with available provider capacity from
    // exhaustion (covered by hop_budget tests). Retain REAL rolling accounting,
    // and also prove continuous circuits survive an idle interval beyond 30s.
    // No production lease, frame bound or provider allowance is raised.
    let idle_until = Instant::now() + Duration::from_secs(61);
    while Instant::now() < idle_until {
        peers.pump()?;
        ensure!(peers.channels[source].session_id(from) == old_session);
        ensure!(peers.channels[target].session_id(to) == old_session);
    }
    publish_legs(2)?;
    // End the actual forwarding owner and both of its actual streams. Keep the
    // native endpoints and receipt owners; a new planned route is not success.
    let middle = relays[0];
    peers.drivers[middle].close()?;
    peers.drivers[middle] = Driver::with_material(
        configs[middle].clone(),
        peers.identities[middle].clone(),
        peers
            .identities
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != middle)
            .map(|(_, k)| k.public_key().to_vec())
            .collect(),
        peers.pins,
        Some(observers[middle].clone()),
    )?;
    peers.drivers[source].views.lock().unwrap()[from].fallback = true;
    let restart = Instant::now() + Duration::from_secs(110);
    loop {
        ensure!(
            Instant::now() < restart,
            "actual relay owner did not republish"
        );
        peers.pump()?;
        let record = read_document::<PeerSignedRelayRecordV1>(&configs[middle].public_record_file)?;
        if record.sequence > original[middle].sequence
            && IrohServiceEndpointV1::from_record(&record)?
                .address
                .endpoint_key
                != IrohServiceEndpointV1::from_record(&original[middle])?
                    .address
                    .endpoint_key
        {
            for side in (0..count).filter(|side| *side != middle) {
                publish(&root.join(format!("leg-{side}-{middle}.json")), &record)?;
            }
            break;
        }
    }
    while peers.channels[source].session_id(from) == old_session
        || peers.channels[target].session_id(to) == old_session
        || peers.channels[source].session_id(from).is_none()
        || peers.channels[target].session_id(to).is_none()
    {
        ensure!(
            Instant::now() < restart,
            "relay reconnect did not reauthenticate"
        );
        peers.pump()?;
    }
    ensure!(peers.channels[source].session_id(from) == peers.channels[target].session_id(to));
    complete_objects(&mut peers, source, target, 193, restart)?;
    let snapshot = observers[source]
        .snapshot()
        .context("reconnect evidence contended")?;
    let renewed = snapshot
        .relay_circuits
        .iter()
        .find(|r| r.native_session == peers.channels[source].session_id(from))
        .context("renewed native session not bound to an observed circuit")?;
    ensure!(renewed.route.route_digest != source_circuit.route.route_digest);
    let closed = snapshot
        .relay_circuits
        .iter()
        .find(|r| r.route.route_digest == source_circuit.route.route_digest)
        .context("old circuit evidence was lost")?;
    ensure!(closed.stream_dropped_us.is_some());
    peers.close()
}

fn complete_objects(
    peers: &mut Pair,
    source: usize,
    target: usize,
    marker: u8,
    deadline: Instant,
) -> Result<()> {
    let from = if target < source { target } else { target - 1 };
    let to = if source < target { source } else { source - 1 };
    let sender = peers.channels[target].validator_for_peer(to)?;
    let recipient = peers.channels[source].validator_for_peer(from)?;
    let issued_at_ms = wall_ms()?;
    let messages = [
        Message::RequestBody {
            body_id: [marker; 32],
        },
        Message::RequestBody {
            body_id: [marker + 1; 32],
        },
        Message::NetworkProbe(crate::native_pipeline::consensus::transport::NetworkProbe {
            nonce: [marker + 2; 32],
            sender,
            recipient,
            issued_at_ms,
            expires_at_ms: issued_at_ms + 10_000,
        }),
        Message::NetworkProbe(crate::native_pipeline::consensus::transport::NetworkProbe {
            nonce: [marker + 3; 32],
            sender,
            recipient,
            issued_at_ms,
            expires_at_ms: issued_at_ms + 10_000,
        }),
    ];
    for message in &messages {
        let prepared = peers.channels[source].prepare(message)?;
        ensure!(peers.channels[source].try_send(from, &prepared, Instant::now())?);
    }
    let mut received = BTreeSet::new();
    let mut completed = BTreeSet::new();
    while received.len() != messages.len() || completed.len() != messages.len() {
        ensure!(
            Instant::now() < deadline,
            "relayed concurrent objects did not actually receive/DONE"
        );
        peers.pump()?;
        for side in [source, target] {
            let report = peers.channels[side].poll_io(wall_ms()?, Instant::now(), 64)?;
            ensure!(
                report.rejected == 0 && report.receive_errors == 0 && report.send_failures == 0
            );
            for done in report.completed {
                ensure!(
                    side == source
                        && done.state == ObjectSendState::Delivered
                        && completed.insert(done.body.id())
                );
            }
            if let Some(event) = peers.channels[side].poll_receive(Instant::now(), 64)? {
                let ReceiveEvent::Message(event) = event else {
                    anyhow::bail!("relayed object rejected")
                };
                ensure!(
                    side == target
                        && messages.contains(&event.message)
                        && received.insert(event.fragment_id)
                );
                if let Message::NetworkProbe(probe) = &event.message {
                    probe.validate_time(wall_ms()?)?;
                }
            }
        }
    }
    Ok(())
}
