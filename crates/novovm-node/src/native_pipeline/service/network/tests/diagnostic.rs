use super::*;
use crate::native_pipeline::consensus::{
    native_lifecycle::probe::ProbeCommand, transport::NetworkProbe,
};

#[test]
fn constrained_relay_probe_real_hop_done_does_not_replace_business_routes() -> Result<()> {
    run_constrained_relay(1)
}

#[test]
fn constrained_double_relay_probe_real_hop_done_does_not_replace_business_routes() -> Result<()> {
    run_constrained_relay(2)
}

fn run_constrained_relay(relay_count: usize) -> Result<()> {
    let count = relay_count + 2;
    let target = count - 1;
    let target_peer = target - 1;
    let mut peers = Pair::configured(false, true, count)?;
    let mut configs = peers
        .drivers
        .iter()
        .map(|d| d.config.clone())
        .collect::<Vec<_>>();
    let records = configs
        .iter()
        .map(|c| c.public_record_file.clone())
        .collect::<Vec<_>>();
    for (side, config) in configs.iter_mut().enumerate() {
        config.relay_forwarding = true;
        config.probe_control_file = Some(config.public_record_file.with_extension("nvprobe"));
        config.discovery = false;
        config.discovery_seeds.clear();
        config.peer_records = records
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != side)
            .map(|(_, p)| p.clone())
            .collect();
    }
    let observers = (0..count)
        .map(|_| Arc::new(NativeTransferDiagnosticsV1::new()))
        .collect::<Vec<_>>();
    peers.restart_drivers(configs, Some(&observers))?;
    let ids = (1..=count).map(|i| [i as u8; 32]).collect::<Vec<_>>();
    let relays = ids[1..target].to_vec();
    for (side, driver) in peers.drivers.iter().enumerate() {
        driver.diagnostic.lock().unwrap().identities = Some((
            ids[side],
            ids.iter()
                .enumerate()
                .filter(|(i, _)| *i != side)
                .map(|(_, id)| *id)
                .collect(),
        ));
        for view in driver.views.lock().unwrap().iter_mut() {
            view.fallback = false;
        }
    }
    // Establish the original business connection first. The diagnostic must
    // coexist with it, not silently promote a forced route for all traffic.
    peers.drivers[0].views.lock().unwrap()[target_peer].fallback = true;
    peers.drivers[target].views.lock().unwrap()[0].fallback = true;
    let ready_by = Instant::now() + Duration::from_secs(30);
    loop {
        peers.pump()?;
        if peers
            .drivers
            .iter()
            .all(|d| d.observation()["diagnostic_relay"]["verified_locator_count"] == count - 1)
            && peers.channels[0].session_id(target_peer).is_some()
            && peers.channels[0].session_id(target_peer) == peers.channels[target].session_id(0)
        {
            break;
        }
        ensure!(
            Instant::now() < ready_by,
            "original business connection unavailable"
        );
    }
    let before = peers
        .channels
        .iter()
        .map(|c| (0..count - 1).map(|p| c.session_id(p)).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    for config in &peers.drivers {
        let record = read_document::<PeerSignedRelayRecordV1>(&config.config.public_record_file)?;
        assert_eq!(record.endpoints.len(), 1);
        assert_eq!(record.endpoints[0].max_bytes_per_minute, 128 * 1024);
        assert_eq!(record.endpoints[0].max_sessions, 4);
    }
    let now = Instant::now();
    let wall = wall_ms()?;
    let mut probe = NetworkProbe {
        nonce: [9; 32],
        sender: ids[0],
        recipient: ids[target],
        issued_at_ms: wall,
        expires_at_ms: wall + 10000,
    };
    peers.drivers[0].diagnostic.lock().unwrap().command(
        ProbeCommand::RelayArm(probe.clone(), relays.clone()),
        wall,
        now,
    )?;
    let ready_by = Instant::now() + Duration::from_secs(30);
    loop {
        peers.pump()?;
        let report = peers.drivers[0].observation();
        if report["diagnostic_relay"]["reservation"]["ready"] == true {
            break;
        }
        ensure!(
            Instant::now() < ready_by,
            "constrained diagnostic not ready: {} relay={}",
            report,
            serde_json::to_string(&observers[1].snapshot())?
        );
    }
    probe.issued_at_ms = wall_ms()?;
    probe.expires_at_ms = probe.issued_at_ms + 10000;
    peers.drivers[0].diagnostic.lock().unwrap().command(
        ProbeCommand::RelaySend(probe.clone(), relays.clone()),
        probe.issued_at_ms,
        Instant::now(),
    )?;
    let complete_by = Instant::now() + Duration::from_secs(12);
    let event = |driver: &Driver, name: &str| {
        driver.observation()["diagnostic_relay"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["event"] == name)
            .cloned()
    };
    let (sent, received) = loop {
        peers.pump()?;
        if let (Some(sent), Some(received)) = (
            event(&peers.drivers[0], "relay_probe_object_done"),
            event(
                &peers.drivers[target],
                "relay_probe_received_integrity_verified",
            ),
        ) {
            break (sent, received);
        }
        ensure!(
            Instant::now() < complete_by,
            "diagnostic missing dual-end result: A={} D={}",
            peers.drivers[0].observation(),
            peers.drivers[target].observation()
        );
    };
    assert_eq!(sent["session"], received["session"]);
    assert_eq!(sent["object"], received["object"]);
    assert_eq!(sent["probe"], received["probe"]);
    for (side, channel) in peers.channels.iter().enumerate() {
        for (peer, expected) in before[side].iter().enumerate() {
            assert_eq!(channel.session_id(peer), *expected);
        }
        assert_eq!(
            peers.drivers[side].observation()["installed_bound_peer_routes"],
            usize::from(side == 0 || side == target)
        );
    }
    assert_eq!(peers.replaced_sends, 0);
    hop::complete_objects(
        &mut peers,
        0,
        target,
        201,
        Instant::now() + Duration::from_secs(10),
    )?;
    for relay_index in 1..target {
        let relay = observers[relay_index]
            .snapshot()
            .context("relay evidence missing")?;
        let circuit = relay
            .relay_circuits
            .iter()
            .find(|r| r.route.role == "relay")
            .context("actual relay circuit missing")?;
        assert_eq!(
            circuit.route.path,
            peers.identities[1..]
                .iter()
                .map(|id| id.peer_id())
                .collect::<Vec<_>>()
        );
        assert!(circuit.downstream_ready_us.is_some());
        assert!(circuit
            .forwarded
            .iter()
            .all(|direction| direction.packets > 0 && direction.bytes > 0));
        assert_eq!(circuit.route.cursor, relay_index - 1);
        assert_eq!(
            circuit.route.local_peer,
            peers.identities[relay_index].peer_id()
        );
        for side in [0, target] {
            let evidence = observers[side]
                .snapshot()
                .context("endpoint evidence missing")?;
            let endpoint = evidence
                .relay_circuits
                .iter()
                .find(|c| c.route.route_digest == circuit.route.route_digest)
                .context("route digest mismatch")?;
            assert_eq!(
                serde_json::to_value(endpoint.native_session)?,
                sent["session"]
            );
        }
    }
    peers.close()?;
    Ok(())
}
