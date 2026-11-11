//! Real reactor/QUIC/Finished/service-selection IO. Component crypto identities,
//! not AOEM qualification or physical four-host discovery/multihop acceptance.
use super::*;
use crate::native_pipeline::consensus::native::{
    tests::{codec, separated_material, split_bindings, split_lanes},
    NativeChannel,
};
use crate::native_pipeline::consensus::{native::ReceiveEvent, transport::Message};
use novovm_network::{
    novorudp::{
        endpoint::NativePeer, object_link::NativeSendPolicy, object_sender::ObjectSendState,
    },
    UdpTransport,
};
use std::net::UdpSocket;
mod diagnostic;
mod hop;

#[test]
fn network_failure_observation_retains_first_last_without_blocking_or_unbounded_history() {
    let failures = NetworkFailures::default();
    let error = anyhow::anyhow!("{}", "错".repeat(2048));
    failures.record("discovery_lookup", None, &error);
    for peer in 0..100 {
        failures.record(
            "object_outbound",
            Some(peer),
            &anyhow::anyhow!("connection refused"),
        );
    }
    let report = failures.observation();
    assert_eq!(failures.count.load(Ordering::Relaxed), 101);
    assert_eq!(report["first"]["stage"], "discovery_lookup");
    assert_eq!(
        report["first"]["error"].as_str().unwrap().chars().count(),
        512
    );
    assert_eq!(report["last"]["ordinal"], 101);
    assert_eq!(report["last"]["peer"], 99);
    let guard = failures.details.lock().unwrap();
    failures.record("inbound_session", None, &anyhow::anyhow!("rejected"));
    assert_eq!(failures.count.load(Ordering::Relaxed), 102);
    assert_eq!(failures.observation()["available"], false);
    drop(guard);
    assert_eq!(failures.observation()["omitted"], 1);
    assert_eq!(failures.observation()["last"]["ordinal"], 101);

    // Added stage context must not turn a terminal rejection into a retryable IO error.
    let rejection =
        anyhow::Error::new(ServiceSelectionMismatch).context("discovery domain selection");
    assert!(terminal_session_rejection(&rejection));
    assert!(!terminal_session_rejection(
        &anyhow::anyhow!("connection refused").context("discovery carrier connect")
    ));
}

struct Pair {
    drivers: Vec<Driver>,
    channels: Vec<NativeChannel>,
    _unused_udp: Vec<UdpSocket>,
    identities: Vec<TransportIdentity>,
    pins: (u64, [u8; 32], [u8; 32]),
    replaced_sends: usize,
}
impl Pair {
    fn new(wrong_domain: bool) -> Result<Self> {
        Self::configured(wrong_domain, false, 2)
    }
    fn configured(wrong_domain: bool, discovery: bool, count: usize) -> Result<Self> {
        let root =
            std::env::temp_dir().join(format!("novonet-node-owner-{:016x}", rand::random::<u64>()));
        std::fs::create_dir(&root)?;
        let (signing, identities, domain, _) = separated_material()?;
        let bindings = split_bindings(&domain, &signing, &identities)?;
        let unused = (0..count)
            .map(|_| UdpSocket::bind("127.0.0.1:0"))
            .collect::<std::io::Result<Vec<_>>>()?;
        let mut drivers = Vec::new();
        let mut channels = Vec::new();
        for side in 0..count {
            let config = Config {
                probe_control_file: None,
                relays: Vec::new(),
                nameserver: "127.0.0.1".parse()?,
                bind_address: "127.0.0.1:0".parse()?,
                public_record_file: root.join(format!("{side}.json")),
                carrier_identity_file: None,
                relay_forwarding: false,
                peer_records: if discovery {
                    Vec::new()
                } else {
                    vec![root.join(format!("{}.json", 1 - side))]
                },
                discovery,
                discovery_seeds: if discovery && side != 0 {
                    vec![root.join("0.json")]
                } else {
                    Vec::new()
                },
                fallback_after_ms: 100,
                retry_ms: 100,
                udp_recovery_ms: 1000,
            };
            drivers.push(Driver::with_material(
                config,
                identities[side].clone(),
                (0..count)
                    .filter(|other| *other != side)
                    .map(|other| identities[other].public_key().to_vec())
                    .collect(),
                (
                    domain.chain_id + u64::from(wrong_domain && side == 1),
                    domain.genesis,
                    domain.protocol,
                ),
                None,
            )?);
            let transport = UdpTransport::bind_for_chain(
                novovm_protocol::NodeId(side as u64 + 1),
                "127.0.0.1:0",
                domain.chain_id,
            )?;
            channels.push(NativeChannel::from_transport(
                transport,
                identities[side].clone(),
                bindings.clone(),
                (0..count)
                    .filter(|other| *other != side)
                    .map(|other| {
                        Ok(NativePeer {
                            key: identities[other].public_key().to_vec(),
                            address: unused[other].local_addr()?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
                1,
                domain.clone(),
                codec(),
                split_lanes(),
                NativeSendPolicy::reliable_carrier_v1(Duration::from_secs(10)),
            )?);
        }
        Ok(Self {
            drivers,
            channels,
            _unused_udp: unused,
            identities: identities[..count].to_vec(),
            pins: (domain.chain_id, domain.genesis, domain.protocol),
            replaced_sends: 0,
        })
    }
    fn pump(&mut self) -> Result<()> {
        for (driver, channel) in self.drivers.iter_mut().zip(&mut self.channels) {
            if driver.closed || driver.finished {
                continue;
            }
            driver.poll_network()?;
            if let Ok(connection) = driver.connected.try_recv() {
                let report = channel.replace_bound_carrier_v1(
                    connection.peer,
                    connection.endpoint,
                    Instant::now(),
                )?;
                for completed in report.completed {
                    ensure!(
                        completed.state == ObjectSendState::SessionReplaced,
                        "a handoff must not invent delivered work"
                    );
                    self.replaced_sends += 1;
                }
                driver.views.lock().unwrap()[connection.peer] = PeerView {
                    session: channel.session_id(connection.peer),
                    bound: true,
                    fallback: false,
                };
                connection
                    .accepted
                    .send(true)
                    .map_err(|_| anyhow::anyhow!("native owner did not retain handoff"))?;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
        Ok(())
    }
    fn allow_fallback(&self) {
        for driver in &self.drivers {
            for view in driver.views.lock().unwrap().iter_mut() {
                view.fallback = true;
            }
        }
    }
    fn restart_drivers(
        &mut self,
        configs: Vec<Config>,
        observers: Option<&[Arc<NativeTransferDiagnosticsV1>]>,
    ) -> Result<()> {
        self.close()?;
        self.drivers = configs
            .into_iter()
            .enumerate()
            .map(|(side, config)| {
                Driver::with_material(
                    config,
                    self.identities[side].clone(),
                    self.identities
                        .iter()
                        .enumerate()
                        .filter(|(other, _)| *other != side)
                        .map(|(_, key)| key.public_key().to_vec())
                        .collect(),
                    self.pins,
                    observers.map(|observers| observers[side].clone()),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        self.allow_fallback();
        Ok(())
    }
    fn cold_channels(&mut self) -> Result<()> {
        let (signing, transport, domain, _) = separated_material()?;
        let bindings = split_bindings(&domain, &signing, &transport)?;
        self.channels.clear();
        for side in 0..self.identities.len() {
            self.channels.push(NativeChannel::from_transport(
                UdpTransport::bind_for_chain(
                    novovm_protocol::NodeId(side as u64 + 1),
                    "127.0.0.1:0",
                    domain.chain_id,
                )?,
                self.identities[side].clone(),
                bindings.clone(),
                self.identities
                    .iter()
                    .enumerate()
                    .filter(|(other, _)| *other != side)
                    .map(|(other, key)| {
                        Ok(NativePeer {
                            key: key.public_key().to_vec(),
                            address: self._unused_udp[other].local_addr()?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
                1,
                domain.clone(),
                codec(),
                split_lanes(),
                NativeSendPolicy::reliable_carrier_v1(Duration::from_secs(10)),
            )?);
        }
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        for driver in &mut self.drivers {
            driver.close()?;
        }
        Ok(())
    }
    fn deliver(&mut self, marker: u8, deadline: Instant) -> Result<()> {
        let messages = [
            Message::RequestBody {
                body_id: [marker; 32],
            },
            Message::RequestBody {
                body_id: [marker + 1; 32],
            },
        ];
        for (side, channel) in self.channels.iter_mut().enumerate() {
            ensure!(channel.peer_uses_bound_carrier(0)?);
            let prepared = channel.prepare(&messages[side])?;
            ensure!(channel.try_send(0, &prepared, Instant::now())?);
        }
        let mut received = [false; 2];
        let mut completed = [false; 2];
        while received != [true; 2] || completed != [true; 2] {
            ensure!(
                Instant::now() < deadline,
                "fallback original binary messages did not complete"
            );
            self.pump()?;
            for (side, channel) in self.channels.iter_mut().enumerate() {
                let report = channel.poll_io(wall_ms()?, Instant::now(), 64)?;
                ensure!(
                    report.rejected == 0 && report.receive_errors == 0 && report.send_failures == 0
                );
                for done in report.completed {
                    ensure!(!completed[side] && done.state == ObjectSendState::Delivered);
                    completed[side] = true;
                }
                if let Some(event) = channel.poll_receive(Instant::now(), 64)? {
                    let ReceiveEvent::Message(message) = event else {
                        anyhow::bail!("valid binary fallback message rejected");
                    };
                    ensure!(!received[side] && message.message == messages[1 - side]);
                    received[side] = true;
                }
            }
        }
        Ok(())
    }
}

#[test]
fn cold_private_only_checkpoint_keeps_authenticated_seed_as_dial_hint() -> Result<()> {
    let mut peers = Pair::configured(false, true, 2)?;
    let root = peers.drivers[0]
        .config
        .public_record_file
        .parent()
        .unwrap()
        .to_owned();
    let _credential = super::super::config::test_custody::TestCredentialGuard::new(&root)?;
    let mut configs = peers
        .drivers
        .iter()
        .map(|d| d.config.clone())
        .collect::<Vec<_>>();
    for (side, config) in configs.iter_mut().enumerate() {
        config.carrier_identity_file = Some(root.join(format!("cold-hint-{side}.sealed")));
        config.bind_address = UdpSocket::bind("127.0.0.1:0")?.local_addr()?;
        config.discovery_seeds = if side == 1 {
            vec![root.join("frozen-bootstrap.json")]
        } else {
            Vec::new()
        };
    }
    peers.restart_drivers(configs.clone(), None)?;
    let ready = Instant::now() + Duration::from_secs(15);
    while configs
        .iter()
        .any(|config| !config.public_record_file.exists())
    {
        ensure!(Instant::now() < ready, "fixture locator not published");
        peers.pump()?;
    }
    let original = read_document::<PeerSignedRelayRecordV1>(&configs[0].public_record_file)?;
    publish(&root.join("frozen-bootstrap.json"), &original)?;
    peers
        .close()
        .context("close fully published cold fixture")?;
    // Bind, but never service, the unreachable address. No external traffic or
    // substitute transport: the real Carrier must hit its original deadline.
    let blackhole = UdpSocket::bind("127.0.0.1:0")?;
    let mut endpoint = IrohServiceEndpointV1::from_record(&original)?;
    endpoint.address.direct = vec![blackhole.local_addr()?];
    endpoint.address.relays.clear();
    let now = wall_ms()?;
    let cached = sign_relay_record_v1(
        &peers.identities[0],
        &original.record_id,
        vec![endpoint.endpoint()?],
        now,
        now + 120_000,
        original.sequence + 1,
    )?;
    let cache = configs[1]
        .public_record_file
        .with_extension("discovery-cache.json");
    publish(&cache, &vec![cached.clone()])?;
    // The live publisher has advanced, but the cold client cannot learn its
    // usable address until it actually performs the original authenticated RPC.
    let live = sign_relay_record_v1(
        &peers.identities[0],
        &original.record_id,
        original.endpoints.clone(),
        now,
        now + 120_000,
        cached.sequence + 1,
    )?;
    publish(&configs[0].public_record_file, &live)?;
    peers.cold_channels()?;
    let observers = (0..2)
        .map(|_| Arc::new(NativeTransferDiagnosticsV1::new()))
        .collect::<Vec<_>>();
    peers.restart_drivers(configs.clone(), Some(&observers))?;
    let deadline = Instant::now() + Duration::from_secs(35);
    while peers.channels.iter().any(|c| c.session_id(0).is_none()) {
        ensure!(
            Instant::now() < deadline,
            "cold discovery discarded the reachable frozen seed"
        );
        peers.pump()?;
    }
    let learned = read_document::<Vec<PeerSignedRelayRecordV1>>(&cache)?;
    let client = peers.drivers[1].failures.carrier.report();
    let events = client["events"].as_array().unwrap();
    ensure!(events
        .iter()
        .any(|e| e["phase"] == "candidate" && e["source"] == "configured_seed"));
    ensure!(events.iter().any(|e| e["outcome"] == "deadline"));
    ensure!(events.iter().any(|e| e["phase"] == "selected_path"
        && e["kind"] == "direct"
        && e["selected_remote"].is_string()));
    let server = peers.drivers[0].failures.carrier.report();
    ensure!(server["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["direction"] == "inbound" && e["phase"] == "Stream" && e["outcome"] == "Ready"));
    ensure!(learned
        .iter()
        .any(|r| r.relay_public_key == original.relay_public_key && r.sequence > cached.sequence));
    ensure!(
        read_document::<PeerSignedRelayRecordV1>(&root.join("frozen-bootstrap.json"))? == original
    );
    let message = Message::RequestBody { body_id: [182; 32] };
    let prepared = peers.channels[1].prepare(&message)?;
    ensure!(peers.channels[1].try_send(0, &prepared, Instant::now())?);
    let mut received = false;
    let mut done = false;
    while !received || !done {
        ensure!(
            Instant::now() < deadline,
            "cold seed object did not reach DONE"
        );
        peers.pump()?;
        for side in 0..2 {
            let report = peers.channels[side].poll_io(wall_ms()?, Instant::now(), 64)?;
            ensure!(
                report.rejected == 0 && report.receive_errors == 0 && report.send_failures == 0
            );
            for completion in report.completed {
                ensure!(side == 1 && !done && completion.state == ObjectSendState::Delivered);
                done = true;
            }
            if let Some(event) = peers.channels[side].poll_receive(Instant::now(), 64)? {
                let ReceiveEvent::Message(event) = event else {
                    anyhow::bail!("cold seed object rejected")
                };
                ensure!(side == 0 && !received && event.message == message);
                received = true;
            }
        }
    }
    peers.close().context("close cold seed after object DONE")
}

#[test]
fn all_native_owners_cold_reopen_from_frozen_bootstraps_and_changed_seed_coordinates() -> Result<()>
{
    let mut peers = Pair::configured(false, true, 3)?;
    let root = peers.drivers[0]
        .config
        .public_record_file
        .parent()
        .unwrap()
        .to_owned();
    let _credential = super::super::config::test_custody::TestCredentialGuard::new(&root)?;
    let mut configs = peers
        .drivers
        .iter()
        .map(|d| d.config.clone())
        .collect::<Vec<_>>();
    for (side, config) in configs.iter_mut().enumerate() {
        config.carrier_identity_file = Some(root.join(format!("carrier-{side}.sealed")));
        config.bind_address = UdpSocket::bind("127.0.0.1:0")?.local_addr()?;
        config.discovery_seeds = match side {
            0 => vec![root.join("bootstrap-1.json")],
            1 => vec![root.join("bootstrap-0.json")],
            _ => vec![root.join("bootstrap-0.json"), root.join("bootstrap-1.json")],
        };
    }
    peers.restart_drivers(configs.clone(), None)?;
    let ready = Instant::now() + Duration::from_secs(15);
    while peers
        .drivers
        .iter()
        .any(|d| !d.config.public_record_file.exists())
    {
        ensure!(Instant::now() < ready, "cold fixture locator not published");
        peers.pump()?;
    }
    for (side, config) in configs.iter().enumerate().take(2) {
        std::fs::copy(
            &config.public_record_file,
            root.join(format!("bootstrap-{side}.json")),
        )?;
    }
    let frozen = std::fs::read(root.join("bootstrap-0.json"))?;
    let initial = read_document::<PeerSignedRelayRecordV1>(&configs[0].public_record_file)?;
    let deadline = Instant::now() + Duration::from_secs(60);
    while peers
        .channels
        .iter()
        .any(|c| (0..2).any(|p| c.session_id(p).is_none()))
    {
        ensure!(
            Instant::now() < deadline,
            "initial independent discovery not ready"
        );
        peers.pump()?;
    }
    peers.drivers[0].close()?;
    configs[0].bind_address = UdpSocket::bind("127.0.0.1:0")?.local_addr()?;
    peers.drivers[0] = Driver::with_material(
        configs[0].clone(),
        peers.identities[0].clone(),
        peers.identities[1..]
            .iter()
            .map(|k| k.public_key().to_vec())
            .collect(),
        peers.pins,
        None,
    )?;
    peers.allow_fallback();
    let cache = configs[2]
        .public_record_file
        .with_extension("discovery-cache.json");
    loop {
        ensure!(
            Instant::now() < deadline,
            "relocated seed was not learned through the other seed"
        );
        peers.pump()?;
        let learned = read_document::<Vec<PeerSignedRelayRecordV1>>(&cache).unwrap_or_default();
        if learned.iter().any(|r| {
            r.relay_public_key == peers.identities[0].public_key()
                && IrohServiceEndpointV1::from_record(r)
                    .is_ok_and(|e| e.address.direct.contains(&configs[0].bind_address))
        }) {
            break;
        }
    }
    let changed = read_document::<PeerSignedRelayRecordV1>(&configs[0].public_record_file)?;
    ensure!(
        IrohServiceEndpointV1::from_record(&changed)?
            .address
            .endpoint_key
            == IrohServiceEndpointV1::from_record(&initial)?
                .address
                .endpoint_key
    );
    ensure!(
        std::fs::read(root.join("bootstrap-0.json"))? == frozen,
        "test must not refresh bootstrap copies by filesystem"
    );
    peers.close()?;
    peers.cold_channels()?;
    peers.restart_drivers(configs, None)?;
    let cold = Instant::now() + Duration::from_secs(40);
    while peers
        .channels
        .iter()
        .any(|c| (0..2).any(|p| c.session_id(p).is_none()))
    {
        ensure!(
            Instant::now() < cold,
            "all owners did not cold recover from frozen sources"
        );
        peers.pump()?;
    }
    let message = Message::RequestBody { body_id: [181; 32] };
    let prepared = peers.channels[1].prepare(&message)?;
    ensure!(peers.channels[1].try_send(1, &prepared, Instant::now())?);
    let mut received = false;
    let mut done = false;
    while !received || !done {
        ensure!(
            Instant::now() < cold,
            "cold recovered object did not actually complete"
        );
        peers.pump()?;
        for side in 0..3 {
            let report = peers.channels[side].poll_io(wall_ms()?, Instant::now(), 64)?;
            ensure!(
                report.rejected == 0 && report.receive_errors == 0 && report.send_failures == 0
            );
            for completion in report.completed {
                ensure!(side == 1 && !done && completion.state == ObjectSendState::Delivered);
                done = true;
            }
            if let Some(event) = peers.channels[side].poll_receive(Instant::now(), 64)? {
                let ReceiveEvent::Message(event) = event else {
                    anyhow::bail!("cold recovered object rejected")
                };
                ensure!(side == 2 && !received && event.message == message);
                received = true;
            }
        }
    }
    peers.close()
}

#[test]
fn native_owner_discovers_peers_through_shared_rpc_then_delivers_original_objects() -> Result<()> {
    let mut peers = Pair::configured(false, true, 3)?;
    // No peer files for ANY node; two nodes know only the first seed's locator.
    // They must learn each other via actual authenticated RPC, not shared files.
    ensure!(peers
        .drivers
        .iter()
        .all(|d| d.config.peer_records.is_empty()));
    peers.allow_fallback();
    let deadline = Instant::now() + Duration::from_secs(35);
    while peers
        .channels
        .iter()
        .any(|c| (0..2).any(|p| c.session_id(p).is_none()))
    {
        ensure!(
            Instant::now() < deadline,
            "network discovery did not establish all original peer routes"
        );
        peers.pump()?;
    }
    // Node 1 <-> node 2 are NOT a seed pair. Actual original object delivery
    // and DONE are required, not a directory hit or connection observation.
    let deliver = |peers: &mut Pair, marker, seed_offline: bool| -> Result<()> {
        let message = Message::RequestBody {
            body_id: [marker; 32],
        };
        let prepared = peers.channels[1].prepare(&message)?;
        ensure!(peers.channels[1].try_send(1, &prepared, Instant::now())?);
        let mut received = false;
        let mut delivered = false;
        let mut disconnected_io = 0;
        while !received || !delivered {
            ensure!(
                Instant::now() < deadline,
                "discovered peer object did not complete"
            );
            peers.pump()?;
            for side in 1..3 {
                let report = peers.channels[side].poll_io(wall_ms()?, Instant::now(), 64)?;
                ensure!(report.rejected == 0);
                if seed_offline {
                    // poll_io covers ALL original peers, including the seed
                    // deliberately stopped below. Retain its failure count;
                    // only actual receipt/DONE establishes the OTHER route.
                    disconnected_io += report.receive_errors + report.send_failures;
                } else {
                    ensure!(report.receive_errors == 0 && report.send_failures == 0);
                }
                for done in report.completed {
                    ensure!(side == 1 && !delivered && done.state == ObjectSendState::Delivered);
                    delivered = true;
                }
                if let Some(event) = peers.channels[side].poll_receive(Instant::now(), 64)? {
                    let ReceiveEvent::Message(incoming) = event else {
                        anyhow::bail!("discovered object rejected")
                    };
                    ensure!(side == 2 && !received && incoming.message == message);
                    received = true;
                }
            }
        }
        if seed_offline {
            ensure!(disconnected_io > 0, "stopped seed IO was not observed");
        }
        Ok(())
    };
    deliver(&mut peers, 171, false)?;
    // Discovery is metadata, not a runtime dependency of a healthy session.
    peers.drivers[0].close()?;
    let keep = (
        peers.channels[1].session_id(1),
        peers.channels[2].session_id(1),
    );
    for _ in 0..10 {
        for side in 1..3 {
            peers.drivers[side].poll_network()?;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    ensure!(
        keep == (
            peers.channels[1].session_id(1),
            peers.channels[2].session_id(1)
        )
    );
    deliver(&mut peers, 172, true)?;
    peers.close()
}

#[test]
fn retained_tick_reactor_reauthenticates_after_peer_owner_restart_without_reopening_native_owner(
) -> Result<()> {
    let mut pair = Pair::new(false)?;
    pair.allow_fallback();
    let deadline = Instant::now() + Duration::from_secs(20);
    while pair
        .channels
        .iter()
        .any(|channel| channel.session_id(0).is_none())
    {
        ensure!(Instant::now() < deadline, "initial fallback session absent");
        pair.pump()?;
    }
    pair.deliver(81, deadline)?;
    let previous = pair.channels[0].session_id(0);
    pair.drivers[1].close()?;
    let pending = pair.channels[0].prepare(&Message::RequestBody { body_id: [91; 32] })?;
    ensure!(pair.channels[0].try_send(0, &pending, Instant::now())?);
    let mut failed_writes = 0;
    while failed_writes == 0 {
        ensure!(
            Instant::now() < deadline,
            "closed carrier not observed with pending work"
        );
        pair.drivers[0].poll_network()?;
        let report = pair.channels[0].poll_io(wall_ms()?, Instant::now(), 64)?;
        failed_writes += report.send_failures;
        ensure!(
            report.completed.is_empty(),
            "disconnection cannot claim business delivery"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let replacement = Driver::with_material(
        pair.drivers[1].config.clone(),
        pair.identities[1].clone(),
        vec![pair.identities[0].public_key().to_vec()],
        pair.pins,
        None,
    )?;
    pair.drivers[1] = replacement;
    pair.drivers[1].views.lock().unwrap()[0] = PeerView {
        fallback: false,
        bound: true,
        session: pair.channels[1].session_id(0),
    };
    while pair
        .channels
        .iter()
        .any(|channel| channel.session_id(0) == previous)
    {
        ensure!(
            Instant::now() < deadline,
            "native owners did not adopt fresh Finished after peer restart"
        );
        pair.pump()?;
    }
    ensure!(pair.channels[0].session_id(0) == pair.channels[1].session_id(0));
    ensure!(
        pair.replaced_sends == 1,
        "original pending send must retire exactly once as unknown"
    );
    pair.deliver(83, deadline)?;
    pair.close()
}

#[test]
fn retained_tick_reactor_uses_backup_only_when_requested_and_delivers_original_binary_messages(
) -> Result<()> {
    let mut pair = Pair::new(false)?;
    let quiet = Instant::now() + Duration::from_millis(1200);
    while Instant::now() < quiet {
        pair.pump()?;
    }
    ensure!(
        pair.channels
            .iter()
            .all(|channel| channel.session_id(0).is_none()),
        "backup replaced default UDP without a failure request"
    );
    pair.allow_fallback();
    let deadline = Instant::now() + Duration::from_secs(15);
    while pair
        .channels
        .iter()
        .any(|channel| channel.session_id(0).is_none())
    {
        ensure!(
            Instant::now() < deadline,
            "real owner did not authenticate fallback: {:?}",
            pair.drivers
                .iter()
                .map(Driver::observation)
                .collect::<Vec<_>>()
        );
        pair.pump()?;
    }
    let messages = [
        Message::RequestBody { body_id: [77; 32] },
        Message::RequestBody { body_id: [78; 32] },
    ];
    for (side, channel) in pair.channels.iter_mut().enumerate() {
        ensure!(channel.peer_uses_bound_carrier(0)?);
        let prepared = channel.prepare(&messages[side])?;
        ensure!(channel.try_send(0, &prepared, Instant::now())?);
    }
    let mut received = [false; 2];
    let mut completed = [false; 2];
    while received != [true; 2] || completed != [true; 2] {
        ensure!(
            Instant::now() < deadline,
            "fallback original binary messages did not complete"
        );
        pair.pump()?;
        for (side, channel) in pair.channels.iter_mut().enumerate() {
            let report = channel.poll_io(wall_ms()?, Instant::now(), 64)?;
            ensure!(
                report.rejected == 0 && report.receive_errors == 0 && report.send_failures == 0
            );
            for done in report.completed {
                ensure!(!completed[side] && done.state == ObjectSendState::Delivered);
                completed[side] = true;
            }
            if let Some(event) = channel.poll_receive(Instant::now(), 64)? {
                let ReceiveEvent::Message(message) = event else {
                    anyhow::bail!("valid binary fallback message rejected");
                };
                ensure!(!received[side] && message.message == messages[1 - side]);
                received[side] = true;
            }
        }
    }
    pair.close()
}

#[test]
fn retained_tick_reactor_rejects_wrong_chain_selection_before_native_handoff() -> Result<()> {
    let mut pair = Pair::new(true)?;
    let ready = Instant::now() + Duration::from_secs(10);
    while pair
        .drivers
        .iter()
        .any(|driver| !driver.config.public_record_file.exists())
    {
        ensure!(
            Instant::now() < ready,
            "native locator publication not ready"
        );
        pair.pump()?;
    }
    let initiator = usize::from(pair.identities[1].public_key() < pair.identities[0].public_key());
    let responder = 1 - initiator;
    let path = &pair.drivers[responder].config.public_record_file;
    let record: PeerSignedRelayRecordV1 = read_document(path)?;
    let pins = (pair.pins.0 + initiator as u64, pair.pins.1, pair.pins.2);
    // A peer can sign a plausible locator for our domain while actually
    // running another chain. Exercise the ENCRYPTED selection, not just the
    // cheap directory-domain filter introduced by network discovery.
    let record = sign_relay_record_v1(
        &pair.identities[responder],
        novovm_network::discovery_control::native_locator_id_v1(pins),
        record.endpoints,
        record.issued_at_ms,
        record.expires_at_ms,
        record.sequence + 1,
    )?;
    publish(path, &record)?;
    pair.allow_fallback();
    let deadline = Instant::now() + Duration::from_secs(15);
    while pair
        .drivers
        .iter()
        .all(|driver| driver.failures.count.load(Ordering::Relaxed) == 0)
    {
        ensure!(
            Instant::now() < deadline,
            "wrong-chain test never reached a real failed handshake"
        );
        pair.pump()?;
    }
    ensure!(pair
        .channels
        .iter()
        .all(|channel| channel.session_id(0).is_none()));
    pair.close()
}
