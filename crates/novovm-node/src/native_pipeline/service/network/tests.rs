//! Real reactor/QUIC/Finished/service-selection IO. Component crypto identities,
//! not AOEM qualification, four-host discovery or native multihop acceptance.
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
        let root =
            std::env::temp_dir().join(format!("novonet-node-owner-{:016x}", rand::random::<u64>()));
        std::fs::create_dir(&root)?;
        let (signing, identities, domain, _) = separated_material()?;
        let bindings = split_bindings(&domain, &signing, &identities)?;
        let unused = (0..2)
            .map(|_| UdpSocket::bind("127.0.0.1:0"))
            .collect::<std::io::Result<Vec<_>>>()?;
        let mut drivers = Vec::new();
        let mut channels = Vec::new();
        for side in 0..2 {
            let config = Config {
                relays: Vec::new(),
                nameserver: "127.0.0.1".parse()?,
                bind_address: "127.0.0.1:0".parse()?,
                public_record_file: root.join(format!("{side}.json")),
                peer_records: vec![root.join(format!("{}.json", 1 - side))],
                fallback_after_ms: 100,
                retry_ms: 100,
                udp_recovery_ms: 1000,
            };
            drivers.push(Driver::with_material(
                config,
                identities[side].clone(),
                vec![identities[1 - side].public_key().to_vec()],
                (
                    domain.chain_id + u64::from(wrong_domain && side == 1),
                    domain.genesis,
                    domain.protocol,
                ),
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
                vec![NativePeer {
                    key: identities[1 - side].public_key().to_vec(),
                    address: unused[side].local_addr()?,
                }],
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
            identities: identities[..2].to_vec(),
            pins: (domain.chain_id, domain.genesis, domain.protocol),
            replaced_sends: 0,
        })
    }
    fn pump(&mut self) -> Result<()> {
        for (driver, channel) in self.drivers.iter_mut().zip(&mut self.channels) {
            driver.poll_network()?;
            if let Ok(connection) = driver.connected.try_recv() {
                let report =
                    channel.replace_bound_carrier_v1(0, connection.endpoint, Instant::now())?;
                for completed in report.completed {
                    ensure!(
                        completed.state == ObjectSendState::SessionReplaced,
                        "a handoff must not invent delivered work"
                    );
                    self.replaced_sends += 1;
                }
                driver.views.lock().unwrap()[0] = PeerView {
                    session: channel.session_id(0),
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
            driver.views.lock().unwrap()[0].fallback = true;
        }
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
    pair.allow_fallback();
    let deadline = Instant::now() + Duration::from_secs(15);
    while pair
        .drivers
        .iter()
        .all(|driver| driver.failures.load(Ordering::Relaxed) == 0)
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
