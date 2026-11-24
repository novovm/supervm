//! Periodic endpoint observations share the listener socket and packet budget.
use super::*;
use crate::endpoint_observation::{
    is_public_observed_endpoint_v2, EndpointObservationProbeV2, ObservationSourcesV2,
    ENDPOINT_OBSERVATION_STREAM,
};
use std::sync::Mutex;

const INTERVAL: Duration = Duration::from_secs(20);
const ATTEMPT: Duration = Duration::from_secs(3);
const SAMPLE_LIFETIME: Duration = Duration::from_secs(80);

#[derive(Clone, Copy)]
pub(crate) struct Mapping {
    pub address: SocketAddr,
    pub expires_ms: u64,
    expires: Instant,
}

#[derive(Clone, Default)]
pub(crate) struct Observation(Arc<Mutex<Option<Mapping>>>);

impl Observation {
    pub fn current(&self, wall_ms: u64, now: Instant) -> Option<Mapping> {
        self.0
            .lock()
            .ok()
            .and_then(|mapping| *mapping)
            .filter(|mapping| wall_ms < mapping.expires_ms && now < mapping.expires)
    }

    fn store(&self, mapping: Option<Mapping>) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = mapping;
        }
    }
}

pub(super) struct Refresh {
    source: ObservationSourcesV2,
    local: SocketAddr,
    state: Observation,
    next: Instant,
    round: Option<Round>,
}

struct Round {
    peers: [NativePeer; 2],
    first: Option<SocketAddr>,
    attempt: Attempt,
}

struct Attempt {
    peer: usize,
    deadline: Instant,
    probe: EndpointObservationProbeV2,
    sent: Option<[u8; 16]>,
}

impl NativeEndpoint {
    pub(crate) fn observe_listener_mapping(
        &mut self,
        source: ObservationSourcesV2,
        now: Instant,
    ) -> Result<Observation> {
        ensure!(
            self.open.is_some() && !self.traffic_started && self.observer_refresh.is_none(),
            "observation requires the original idle listener"
        );
        let state = Observation::default();
        self.observer_refresh = Some(Refresh {
            source,
            local: self.local_addr()?,
            state: state.clone(),
            next: now,
            round: None,
        });
        Ok(state)
    }

    fn start_observation_peer(
        &mut self,
        peer: &NativePeer,
        wall_ms: u64,
        now: Instant,
    ) -> Result<Attempt> {
        let address = NativeRoute::Udp(peer.address);
        let id = peer_id_from_mldsa65_public_key_v1(&peer.key);
        ensure!(
            self.observation_peer.is_none()
                && !self.sources.contains_key(&address)
                && !self.reconnect_source_in_use(address)
                && !self
                    .peers
                    .values()
                    .chain(self.waiting_clients.iter())
                    .any(|peer| peer.id.as_ref() == id),
            "observation cannot replace an existing peer"
        );
        let identity = self
            .open
            .as_ref()
            .context("observation listener ended")?
            .identity
            .clone();
        let handshake = NativeHandshake::start_role(
            identity,
            peer.key.clone(),
            address,
            wall_ms,
            now,
            ATTEMPT,
            Duration::from_millis(350),
            Some(true),
        )?;
        let handle = self.next_peer;
        self.insert_client(
            Peer {
                id: id.into(),
                configured_key: None,
                observation_only: true,
                handshake: Some(handshake),
                session: None,
                streams: Some(BTreeSet::from([ENDPOINT_OBSERVATION_STREAM])),
                reconnect: None,
                last_authenticated: now,
            },
            address,
        )?;
        self.observation_peer = Some(handle);
        Ok(Attempt {
            peer: handle,
            deadline: now + ATTEMPT,
            probe: EndpointObservationProbeV2::default(),
            sent: None,
        })
    }

    fn end_observation_peer(&mut self, peer: usize) {
        if self.observation_peer == Some(peer) {
            self.sources.retain(|_, handle| *handle != peer);
            self.peers.remove(&peer);
            self.observation_peer = None;
        }
    }
}

fn eligible_sources(source: &ObservationSourcesV2, local: SocketAddr) -> Option<[NativePeer; 2]> {
    let peers: [NativePeer; 2] = source().try_into().ok()?;
    (peers[0].key != peers[1].key
        && peers[0].address.ip() != peers[1].address.ip()
        && peers.iter().all(|p| {
            p.key.len() == crate::product_overlay::MLDSA65_PUBLIC_KEY_BYTES
                && p.address.is_ipv4() == local.is_ipv4()
        }))
    .then_some(peers)
}

impl Refresh {
    pub(super) fn poll(
        &mut self,
        endpoint: &mut NativeEndpoint,
        frames: &mut Vec<NativeReceivedFrame>,
        wall_ms: u64,
        now: Instant,
    ) {
        if self.round.is_none() {
            if now < self.next {
                return;
            }
            self.next = now + INTERVAL;
            let Some(peers) = eligible_sources(&self.source, self.local) else {
                self.state.store(None);
                return;
            };
            let Ok(attempt) = endpoint.start_observation_peer(&peers[0], wall_ms, now) else {
                return;
            };
            self.round = Some(Round {
                peers,
                first: None,
                attempt,
            });
            return;
        }

        let mut round = self.round.take().expect("active observation round");
        let attempt = &mut round.attempt;
        let mut observed = None;
        frames.retain(|frame| {
            if frame.peer() != attempt.peer {
                return true;
            }
            if now < attempt.deadline {
                if let Some(session) = attempt.sent {
                    observed = attempt
                        .probe
                        .observed(frame, attempt.peer, session)
                        .or(observed);
                }
            }
            false
        });
        if let Some(address) = observed {
            endpoint.end_observation_peer(attempt.peer);
            if let Some(first) = round.first {
                let approved = eligible_sources(&self.source, self.local).is_some_and(|approved| {
                    approved
                        .iter()
                        .zip(&round.peers)
                        .all(|(a, b)| a.key == b.key && a.address == b.address)
                });
                self.state.store(
                    (approved
                        && first == address
                        && address.is_ipv4() == self.local.is_ipv4()
                        && is_public_observed_endpoint_v2(address))
                    .then_some(Mapping {
                        address,
                        expires_ms: wall_ms.saturating_add(SAMPLE_LIFETIME.as_millis() as u64),
                        expires: now + SAMPLE_LIFETIME,
                    }),
                );
            } else if let Ok(attempt) =
                endpoint.start_observation_peer(&round.peers[1], wall_ms, now)
            {
                round.first = Some(address);
                round.attempt = attempt;
                self.round = Some(round);
            }
        } else if now >= attempt.deadline {
            endpoint.end_observation_peer(attempt.peer);
        } else {
            if attempt.sent.is_none() {
                if let Some(session) = endpoint.session_id(attempt.peer) {
                    if endpoint
                        .try_send(attempt.peer, &attempt.probe.request(session))
                        .unwrap_or(false)
                    {
                        attempt.sent = Some(session);
                    }
                }
            }
            self.round = Some(round);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        endpoint_observation::observation_reply_v2,
        novorudp::{NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0},
        test_identity::key,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn periodic_observation_uses_one_listener_and_preserves_a_live_business_session() -> Result<()>
    {
        const BUSINESS: u64 = 7;
        let local_socket = UdpSocket::bind("127.0.0.1:0")?;
        let local_addr = local_socket.local_addr()?;
        let handset = key(101);
        let budget = NativeHandshakeBudget {
            lifetime: ATTEMPT,
            retry: Duration::from_millis(25),
        };
        let mut listener = NativeEndpoint::listen_clients(
            local_socket,
            handset.clone(),
            1,
            vec![BUSINESS],
            budget.clone(),
        )?;
        listener.enable_endpoint_observer_v2()?;
        let mut observers = Vec::new();
        let mut source = Vec::new();
        for (seed, ip) in [(102, "127.0.0.1:0"), (103, "127.0.0.2:0")] {
            let socket = UdpSocket::bind(ip)?;
            let identity = key(seed);
            source.push(NativePeer {
                key: identity.public_key().to_vec(),
                address: socket.local_addr()?,
            });
            let mut observer = NativeEndpoint::listen_clients(
                socket,
                identity,
                1,
                vec![ENDPOINT_OBSERVATION_STREAM],
                budget.clone(),
            )?;
            observer.enable_client_reconnect_v1()?;
            observers.push(observer);
        }
        let start = Instant::now();
        let wall = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let state = listener.observe_listener_mapping(Arc::new(move || source.clone()), start)?;
        let mut business = NativeEndpoint::connect_client(
            UdpSocket::bind("127.0.0.1:0")?,
            key(104),
            NativePeer {
                key: handset.public_key().to_vec(),
                address: local_addr,
            },
            budget,
            wall,
            start,
        )?;
        let mut prior_session = None;
        for round in 0..2 {
            let clock = Instant::now();
            let mut sampled = [0; 2];
            let mut sent = false;
            let mut received = false;
            while clock.elapsed() < 3 * ATTEMPT {
                let elapsed = start.elapsed() + INTERVAL * round;
                let now = start + elapsed;
                let wall_ms = wall + elapsed.as_millis() as u64;
                business.poll(wall_ms, now, 16)?;
                if let Some(session) = business.session_id(0) {
                    if let Some(saved) = prior_session {
                        assert_eq!(session, saved);
                    } else {
                        prior_session = Some(session);
                    }
                    if !sent {
                        sent = business.try_send(
                            0,
                            &NovoRudpTransportFrameV0::new(
                                NovoRudpTransportFrameKindV0::Data,
                                session,
                                BUSINESS,
                                round as u64,
                                0,
                                0,
                                vec![round as u8],
                            ),
                        )?;
                    }
                }
                let report = listener.poll(wall_ms, now, 16)?;
                assert!(listener.peer_count() <= 1);
                for frame in report.frames {
                    assert_eq!(frame.frame().stream_id, BUSINESS);
                    assert_eq!(frame.frame().payload, vec![round as u8]);
                    received = true;
                }
                for (index, observer) in observers.iter_mut().enumerate() {
                    for frame in observer.poll(wall_ms, now, 16)?.frames {
                        let (observed_addr, reply) = observation_reply_v2(observer, &frame)
                            .context("authenticated listener observation")?;
                        assert_eq!(observed_addr, local_addr);
                        if observer.try_send(frame.peer(), &reply)? {
                            sampled[index] += 1;
                        }
                    }
                }
                if received && sampled == [1, 1] && listener.observation_peer.is_none() {
                    assert_eq!(listener.local_addr()?, local_addr);
                    assert!(
                        state.current(wall_ms, now).is_none(),
                        "loopback may not be public"
                    );
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(received, "observation interrupted business");
            assert_eq!(sampled, [1, 1], "both pinned observers queried once");
            assert!(listener.observation_peer.is_none());
        }
        Ok(())
    }

    #[test]
    fn mapping_expires_on_either_clock() {
        let state = Observation::default();
        let now = Instant::now();
        state.store(Some(Mapping {
            address: "8.8.8.8:42".parse().unwrap(),
            expires_ms: 2000,
            expires: now + Duration::from_secs(1),
        }));
        assert!(state.current(1999, now).is_some());
        assert!(state.current(2000, now).is_none());
        assert!(state.current(1000, now + Duration::from_secs(1)).is_none());
    }
}
