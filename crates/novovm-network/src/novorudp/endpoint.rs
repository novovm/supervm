//! Socket owner for the original native handshake and encrypted frame adapter.
//! No protocol scheduler, execution, ledger, spawned worker or hidden retry queue.
//! One bounded receive quantum precedes a fair handshake-send quantum. Rejected
//! datagrams and an absent peer never terminate another peer's live session.

use super::{
    carrier::{NativeDatagramIo, NativeRoute},
    datagram::{NativeUdpSession, MAX_DATAGRAM_BYTES},
    handshake::NativeHandshake,
    object_sender::NativeObjectSender,
    wire_budget::{NativeWireBudget, NativeWireUsage},
    NovoRudpTransportFrameV0,
};
use crate::product_overlay::{peer_id_from_ed25519_public_key_v1, HandshakeReplayCacheV1};
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::ErrorKind,
    net::{SocketAddr, UdpSocket},
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct NativePeer {
    pub key: [u8; 32],
    pub address: SocketAddr,
}

/// Node identity and actual carrier route remain independently bound.
#[derive(Clone)]
pub struct NativeRoutePeer {
    pub key: [u8; 32],
    pub route: NativeRoute,
}
impl From<NativePeer> for NativeRoutePeer {
    fn from(peer: NativePeer) -> Self {
        Self {
            key: peer.key,
            route: NativeRoute::Udp(peer.address),
        }
    }
}

#[derive(Clone)]
pub struct NativeHandshakeBudget {
    pub lifetime: Duration,
    pub retry: Duration,
}

struct Peer {
    id: Arc<str>,
    // Only bind[_shared] installs this pinned identity. An open client never
    // becomes a configured peer merely by using its key or source address.
    configured_key: Option<[u8; 32]>,
    handshake: Option<NativeHandshake>,
    session: Option<NativeUdpSession>,
    streams: Option<BTreeSet<u64>>,
    reconnect: Option<reconnect::PendingReconnect>,
    last_authenticated: Instant,
}

struct ConfiguredAdmission {
    identity: SigningKey,
    budget: NativeHandshakeBudget,
    reconnect_v1: bool,
}

struct OpenAdmission {
    identity: SigningKey,
    limit: usize,
    streams: BTreeSet<u64>,
    budget: NativeHandshakeBudget,
    reconnect_v1: bool,
    retire_clients: bool,
}

/// A fresh authenticated generation replaced the old one. Application owners
/// must retire old operations as unknown, never relabel them as new requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeSessionChange {
    pub peer: usize,
    pub previous: [u8; 16],
    pub current: [u8; 16],
}

/// An idle dynamic peer has left this owner's live table. Handles are never
/// reused, so old frames, completions and application adapters cannot alias a
/// later identity. This event does not undo any durable business operation.
#[derive(Clone, Debug)]
pub struct NativePeerRetirement {
    pub peer: usize,
    pub identity: String,
    pub previous_session: Option<[u8; 16]>,
}

/// Constructed only after source, identity, encryption and replay checks.
pub struct NativeReceivedFrame {
    peer: usize,
    identity: Arc<str>,
    frame: NovoRudpTransportFrameV0,
}
impl NativeReceivedFrame {
    pub fn peer(&self) -> usize {
        self.peer
    }
    pub fn peer_id(&self) -> &str {
        &self.identity
    }
    pub fn frame(&self) -> &NovoRudpTransportFrameV0 {
        &self.frame
    }
    pub fn into_frame(self) -> NovoRudpTransportFrameV0 {
        self.frame
    }
}

#[derive(Default)]
pub struct NativePoll {
    pub frames: Vec<NativeReceivedFrame>,
    pub rejected_datagrams: usize,
    pub receive_errors: usize,
    pub handshake_send_failures: usize,
    pub handshakes_sent: usize,
    pub session_changes: Vec<NativeSessionChange>,
}

pub struct NativeEndpoint {
    io: Arc<dyn NativeDatagramIo>,
    // Full UDP payload capacity, initialized once. Parsing sees only the exact
    // received prefix; old tail bytes are never input to authentication/decoding.
    receive_scratch: Vec<u8>,
    local_key: [u8; 32],
    peers: BTreeMap<usize, Peer>,
    next_peer: usize,
    // One fully verified admission may wait for the owner to retire idle work.
    // Its original handshake deadline and transcript remain unchanged.
    waiting_client: Option<Peer>,
    sources: BTreeMap<NativeRoute, usize>,
    replay: HandshakeReplayCacheV1,
    turn: usize,
    open: Option<OpenAdmission>,
    configured: Option<ConfiguredAdmission>,
    wire_budget: Option<NativeWireBudget>,
    traffic_started: bool,
}

impl NativeEndpoint {
    /// Adopt a Finished-verified channel on its original authenticated carrier.
    /// This consumes the existing channel; it never performs a second UDP
    /// handshake or treats a transport route as a node identity proof.
    /// `io` must belong to the exact original, still-live bound carrier. Its
    /// owner must enforce revocation, Privacy and cancellation during IO;
    /// this proof snapshot does not establish ongoing runtime validity or
    /// negotiate a native service version on the owner's behalf.
    #[cfg(any(feature = "iroh-transport", feature = "tls-transport"))]
    pub fn from_bound_channel(
        io: Arc<dyn NativeDatagramIo>,
        channel: crate::transport_binding::BoundSecureChannelV1,
        binding: &crate::transport_binding::VerifiedTransportBinding,
        streams: Vec<u64>,
    ) -> Result<Self> {
        ensure!(
            !streams.is_empty()
                && streams.len() <= 16
                && !streams.contains(&0)
                && !streams.contains(&u64::MAX),
            "invalid bound native streams"
        );
        channel.verify_transport_binding(binding)?;
        let local_key = channel.local_identity_public_key();
        let id: Arc<str> = channel.remote_peer_id().into();
        let route =
            NativeRoute::Carrier(super::carrier::VerifiedCarrierRoute::from_binding(binding));
        let session = NativeUdpSession::from_bound_channel(channel, binding)?;
        Ok(Self {
            io,
            receive_scratch: vec![0; 65536],
            local_key,
            peers: BTreeMap::from([(
                0,
                Peer {
                    id,
                    configured_key: None,
                    handshake: None,
                    session: Some(session),
                    streams: Some(streams.into_iter().collect()),
                    reconnect: None,
                    last_authenticated: Instant::now(),
                },
            )]),
            next_peer: 1,
            waiting_client: None,
            sources: BTreeMap::from([(route, 0)]),
            replay: HandshakeReplayCacheV1::new(256),
            turn: 0,
            open: None,
            configured: None,
            wire_budget: None,
            traffic_started: false,
        })
    }
    /// Explicit bounded application listener, with no pre-enrolled client or
    /// validator dependency. Empty `bind` peers still fail; this constructor
    /// installs key-possession admission before the socket can receive data.
    pub fn listen_clients(
        socket: UdpSocket,
        identity: SigningKey,
        clients: usize,
        streams: Vec<u64>,
        budget: NativeHandshakeBudget,
    ) -> Result<Self> {
        Self::listen_clients_shared(Arc::new(socket), identity, clients, streams, budget)
    }

    /// Reuse an existing socket without creating another listener or OS handle.
    /// The enclosing transport must retain one receive owner and dispatch through
    /// `poll_with_receiver`; sharing the socket does not authenticate a sender.
    pub fn listen_clients_shared(
        socket: Arc<UdpSocket>,
        identity: SigningKey,
        clients: usize,
        streams: Vec<u64>,
        budget: NativeHandshakeBudget,
    ) -> Result<Self> {
        socket.set_nonblocking(true)?;
        Self::listen_clients_with_io(socket, identity, clients, streams, budget)
    }

    /// Use the enclosing owner's bounded nonblocking IO. Carrier routes remain
    /// rejected until exporter-bound handshake/Finished support is integrated.
    pub fn listen_clients_with_io(
        io: Arc<dyn NativeDatagramIo>,
        identity: SigningKey,
        clients: usize,
        streams: Vec<u64>,
        budget: NativeHandshakeBudget,
    ) -> Result<Self> {
        let mut endpoint = Self {
            io,
            receive_scratch: vec![0; 65536],
            local_key: identity.verifying_key().to_bytes(),
            peers: BTreeMap::new(),
            next_peer: 0,
            waiting_client: None,
            sources: BTreeMap::new(),
            replay: HandshakeReplayCacheV1::new(256),
            turn: 0,
            open: None,
            configured: None,
            wire_budget: None,
            traffic_started: false,
        };
        endpoint.allow_clients(identity, clients, streams, budget)?;
        Ok(endpoint)
    }

    pub fn bind(
        socket: UdpSocket,
        identity: SigningKey,
        peers: Vec<NativePeer>,
        budget: NativeHandshakeBudget,
        wall_ms: u64,
        now: Instant,
    ) -> Result<Self> {
        Self::bind_shared(Arc::new(socket), identity, peers, budget, wall_ms, now)
    }

    /// Bind the existing native state machine to the transport's shared socket.
    /// This does not install a receiver; the enclosing owner serializes reads.
    /// Keeps the original deterministic initial role/readiness contract. Owners
    /// capable of bidirectional application traffic may explicitly opt into
    /// `enable_configured_reconnect_v1` before polling.
    pub fn bind_shared(
        socket: Arc<UdpSocket>,
        identity: SigningKey,
        peers: Vec<NativePeer>,
        budget: NativeHandshakeBudget,
        wall_ms: u64,
        now: Instant,
    ) -> Result<Self> {
        ensure!(!peers.is_empty(), "native endpoint peer bound");
        socket.set_nonblocking(true)?;
        let mut sources = BTreeMap::new();
        let mut ids = BTreeSet::new();
        let mut owned = BTreeMap::new();
        for (index, peer) in peers.into_iter().enumerate() {
            let id = peer_id_from_ed25519_public_key_v1(&peer.key);
            ensure!(
                ids.insert(id.clone())
                    && sources
                        .insert(NativeRoute::Udp(peer.address), index)
                        .is_none(),
                "duplicate native peer/endpoint"
            );
            let handshake = NativeHandshake::start(
                identity.clone(),
                peer.key,
                peer.address,
                wall_ms,
                now,
                budget.lifetime,
                budget.retry,
            )?;
            owned.insert(
                index,
                Peer {
                    id: id.into(),
                    configured_key: Some(peer.key),
                    handshake: Some(handshake),
                    session: None,
                    streams: None,
                    reconnect: None,
                    last_authenticated: Instant::now(),
                },
            );
        }
        Ok(Self {
            io: socket,
            receive_scratch: vec![0; 65536],
            local_key: identity.verifying_key().to_bytes(),
            next_peer: owned.len(),
            waiting_client: None,
            peers: owned,
            sources,
            replay: HandshakeReplayCacheV1::new(256),
            turn: 0,
            open: None,
            configured: Some(ConfiguredAdmission {
                identity,
                budget,
                reconnect_v1: false,
            }),
            wire_budget: None,
            traffic_started: false,
        })
    }

    /// Explicit open key-possession policy, not an empty allowlist. New peers
    /// get only the declared streams, never the configured validator routes.
    /// Slots are bounded and are not silently evicted/rebound to another key.
    pub fn allow_clients(
        &mut self,
        identity: SigningKey,
        additional: usize,
        streams: Vec<u64>,
        budget: NativeHandshakeBudget,
    ) -> Result<()> {
        ensure!(self.open.is_none(), "native open policy already installed");
        ensure!(
            identity.verifying_key().to_bytes() == self.local_key,
            "native client admission identity mismatch"
        );
        ensure!(
            additional > 0 && self.peers.len().checked_add(additional).is_some(),
            "native client slot bound"
        );
        ensure!(
            !streams.is_empty() && streams.len() <= 16 && !streams.contains(&0),
            "invalid client streams"
        );
        ensure!(
            !budget.retry.is_zero()
                && budget.retry < budget.lifetime
                && budget.lifetime <= Duration::from_secs(60),
            "invalid client handshake budget"
        );
        self.open = Some(OpenAdmission {
            identity,
            limit: self.peers.len() + additional,
            streams: streams.into_iter().collect(),
            budget,
            reconnect_v1: false,
            retire_clients: false,
        });
        Ok(())
    }

    /// Native SDK/application connection. The remote node key is explicitly
    /// bound; this identity need not be a validator or have a token balance.
    pub fn connect_client(
        socket: UdpSocket,
        identity: SigningKey,
        remote: NativePeer,
        budget: NativeHandshakeBudget,
        wall_ms: u64,
        now: Instant,
    ) -> Result<Self> {
        Self::connect_clients(socket, identity, vec![remote], budget, wall_ms, now)
    }

    /// A bounded set of explicitly pinned application services on one socket.
    /// Every remote uses the original client-initiated handshake role; an absent
    /// service does not serialize or change another peer's authentication.
    pub fn connect_clients(
        socket: UdpSocket,
        identity: SigningKey,
        remotes: Vec<NativePeer>,
        budget: NativeHandshakeBudget,
        wall_ms: u64,
        now: Instant,
    ) -> Result<Self> {
        socket.set_nonblocking(true)?;
        Self::connect_clients_with_io(
            Arc::new(socket),
            identity,
            remotes.into_iter().map(Into::into).collect(),
            budget,
            wall_ms,
            now,
        )
    }

    /// Same native peer lifecycle over a supplied IO owner. Only real UDP routes
    /// are currently active; authenticated carrier provenance alone cannot bind
    /// the original native handshake transcript to a reliable carrier.
    pub fn connect_clients_with_io(
        io: Arc<dyn NativeDatagramIo>,
        identity: SigningKey,
        remotes: Vec<NativeRoutePeer>,
        budget: NativeHandshakeBudget,
        wall_ms: u64,
        now: Instant,
    ) -> Result<Self> {
        ensure!(!remotes.is_empty(), "native endpoint peer bound");
        let local_key = identity.verifying_key().to_bytes();
        let mut sources = BTreeMap::new();
        let mut ids = BTreeSet::new();
        let mut peers = BTreeMap::new();
        for (index, remote) in remotes.into_iter().enumerate() {
            let id = peer_id_from_ed25519_public_key_v1(&remote.key);
            ensure!(
                ids.insert(id.clone()) && sources.insert(remote.route, index).is_none(),
                "duplicate native peer/endpoint"
            );
            let handshake = NativeHandshake::start_role(
                identity.clone(),
                remote.key,
                remote.route,
                wall_ms,
                now,
                budget.lifetime,
                budget.retry,
                Some(true),
            )?;
            peers.insert(
                index,
                Peer {
                    id: id.into(),
                    configured_key: None,
                    handshake: Some(handshake),
                    session: None,
                    streams: None,
                    reconnect: None,
                    last_authenticated: Instant::now(),
                },
            );
        }
        Ok(Self {
            io,
            receive_scratch: vec![0; 65536],
            local_key,
            next_peer: peers.len(),
            waiting_client: None,
            peers,
            sources,
            replay: HandshakeReplayCacheV1::new(256),
            turn: 0,
            open: None,
            configured: None,
            wire_budget: None,
            traffic_started: false,
        })
    }

    fn accept_client(
        &mut self,
        source: NativeRoute,
        wire: &[u8],
        wall_ms: u64,
        now: Instant,
    ) -> Result<()> {
        use crate::product_overlay::handshake_wire::{decode, HandshakeMessage};
        let open = self
            .open
            .as_ref()
            .context("native client admission disabled")?;
        let HandshakeMessage::Offer(offer) = decode(wire)? else {
            anyhow::bail!("client must initiate")
        };
        let id = peer_id_from_ed25519_public_key_v1(&offer.initiator_identity_public_key);
        if let Some(index) = self
            .peers
            .iter()
            .find_map(|(&i, p)| (p.id.as_ref() == id).then_some(i))
        {
            return self.prepare_reconnect(index, source, wire, wall_ms, now);
        }
        let full = self.peers.len() >= open.limit;
        if self
            .waiting_client
            .as_ref()
            .is_some_and(|p| p.reconnect.as_ref().is_none_or(|r| now >= r.deadline))
        {
            self.waiting_client = None;
        }
        ensure!(
            self.waiting_client.is_none() && (!full || open.retire_clients),
            "native client admission occupied"
        );
        ensure!(
            !self.sources.contains_key(&source) && !self.reconnect_source_in_use(source),
            "native source already bound"
        );
        let mut handshake = NativeHandshake::start_role(
            open.identity.clone(),
            offer.initiator_identity_public_key,
            source,
            wall_ms,
            now,
            open.budget.lifetime,
            open.budget.retry,
            Some(false),
        )?;
        // All signature/domain/time/replay checks precede retaining a peer slot.
        handshake.receive_route(source, wire, wall_ms, now, &mut self.replay)?;
        let session = handshake
            .take_session()
            .context("client handshake did not authenticate")?;
        let (handshake, session, reconnect) = if open.retire_clients {
            // A previously retired identity can arrive with a replayed signed
            // offer after the bounded replay cache rotates. Only fresh-channel
            // AEAD possession activates this new handle; pending offers expire.
            (
                None,
                None,
                Some(reconnect::PendingReconnect {
                    source,
                    handshake,
                    session,
                    deadline: now
                        .checked_add(open.budget.lifetime)
                        .context("client deadline overflow")?,
                }),
            )
        } else {
            (Some(handshake), Some(session), None)
        };
        let peer = Peer {
            id: id.into(),
            configured_key: None,
            handshake,
            session,
            streams: Some(open.streams.clone()),
            reconnect,
            last_authenticated: now,
        };
        if full {
            self.waiting_client = Some(peer);
        } else {
            self.insert_client(peer, source)?;
        }
        Ok(())
    }
    fn insert_client(&mut self, peer: Peer, source: NativeRoute) -> Result<()> {
        let index = self.next_peer;
        self.next_peer = index
            .checked_add(1)
            .context("native peer handle exhausted")?;
        if peer.session.is_some() {
            self.sources.insert(source, index);
        }
        self.peers.insert(index, peer);
        Ok(())
    }
    fn admit_waiting_client(&mut self, now: Instant) -> Result<()> {
        if self
            .waiting_client
            .as_ref()
            .is_some_and(|p| p.reconnect.as_ref().is_none_or(|r| now >= r.deadline))
        {
            self.waiting_client = None;
        }
        if self
            .open
            .as_ref()
            .is_some_and(|open| self.peers.len() < open.limit)
        {
            if let Some(peer) = self.waiting_client.take() {
                let source = peer
                    .reconnect
                    .as_ref()
                    .context("waiting client lost handshake")?
                    .source;
                self.insert_client(peer, source)?;
            }
        }
        Ok(())
    }
    pub(crate) fn needs_client_capacity(&self, now: Instant) -> bool {
        self.open.as_ref().is_some_and(|open| {
            self.peers.len() > open.limit
                || (self.peers.len() == open.limit
                    && self
                        .waiting_client
                        .as_ref()
                        .and_then(|p| p.reconnect.as_ref())
                        .is_some_and(|r| now < r.deadline))
        })
    }
    /// Configure once, before any poll/send. Reconnects and sessions cannot reset
    /// the node-wide combined ingress-admission/egress allowance.
    pub fn set_wire_budget_v1(&mut self, bytes_per_minute: u64, now: Instant) -> Result<()> {
        ensure!(
            !self.traffic_started && self.wire_budget.is_none(),
            "native wire budget already active or traffic started"
        );
        self.wire_budget = Some(NativeWireBudget::new(bytes_per_minute, now)?);
        Ok(())
    }

    pub fn wire_usage_v1(&mut self, now: Instant) -> Option<NativeWireUsage> {
        self.wire_budget.as_mut().map(|budget| budget.usage(now))
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.io.local_udp_addr()?)
    }
    pub fn accepts_clients(&self) -> bool {
        self.open.is_some()
    }
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
    /// Live handles can be sparse after dynamic client retirement.
    pub fn peer_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.peers.keys().copied()
    }
    pub(crate) fn update_client_capacity(&mut self, capacity: usize) -> Result<bool> {
        ensure!(capacity > 0, "zero native client capacity");
        let Some(open) = self.open.as_mut() else {
            return Ok(false);
        };
        open.limit = capacity;
        open.retire_clients = true;
        open.reconnect_v1 = true;
        Ok(true)
    }
    pub(crate) fn idle_client_candidates(&self, now: Instant, idle_for: Duration) -> Vec<usize> {
        let mut candidates: Vec<_> = self
            .peers
            .iter()
            .filter_map(|(&index, peer)| {
                (peer.configured_key.is_none()
                    && peer.streams.is_some()
                    && peer.reconnect.as_ref().is_none_or(|r| now >= r.deadline)
                    && now.saturating_duration_since(peer.last_authenticated) >= idle_for)
                    .then_some((peer.last_authenticated, index))
            })
            .collect();
        candidates.sort_unstable();
        candidates.into_iter().map(|(_, index)| index).collect()
    }
    pub(crate) fn retire_client(&mut self, index: usize) -> Result<NativePeerRetirement> {
        let peer = self.peers.get(&index).context("unknown native peer")?;
        ensure!(
            self.open.is_some() && peer.configured_key.is_none() && peer.streams.is_some(),
            "only dynamic clients may retire"
        );
        let retired = NativePeerRetirement {
            peer: index,
            identity: peer.id.to_string(),
            previous_session: peer.session.as_ref().map(NativeUdpSession::session_id),
        };
        self.sources.retain(|_, bound| *bound != index);
        self.peers.remove(&index);
        Ok(retired)
    }
    pub fn local_identity(&self) -> [u8; 32] {
        self.local_key
    }
    pub fn peer_id(&self, index: usize) -> Result<&str> {
        Ok(&self.peers.get(&index).context("unknown native peer")?.id)
    }
    pub fn session_id(&self, index: usize) -> Option<[u8; 16]> {
        self.peers
            .get(&index)?
            .session
            .as_ref()
            .map(NativeUdpSession::session_id)
    }
    pub fn expired_handshakes(&self, now: Instant) -> usize {
        self.peers
            .values()
            .filter(|p| p.handshake.as_ref().is_some_and(|h| h.is_expired(now)))
            .count()
    }

    pub fn poll(&mut self, wall_ms: u64, now: Instant, quantum: usize) -> Result<NativePoll> {
        let io = Arc::clone(&self.io);
        self.poll_with_route_receiver(wall_ms, now, quantum, |wire| io.try_receive(wire))
    }

    /// Poll through the enclosing transport's sole bounded datagram receiver.
    /// `Some` supplies an entire native datagram and its actual source; it is
    /// still untrusted and passes the same budget, handshake and AEAD checks.
    /// `None` consumes one quantum handled by another protocol, without adding
    /// a native rejection or native byte charge. That protocol's owner remains
    /// responsible for its own admission/accounting. `WouldBlock` ends input.
    /// The callback must not block, truncate, synthesize a source or perform
    /// native validation. A pre-existing bounded drain queue may prefetch;
    /// quantum bounds consumption here, not its underlying socket-read count.
    /// Handshake sends, reconnect expiry and generation filtering remain here.
    pub fn poll_with_receiver<F>(
        &mut self,
        wall_ms: u64,
        now: Instant,
        quantum: usize,
        mut receiver: F,
    ) -> Result<NativePoll>
    where
        F: FnMut(&mut [u8]) -> std::io::Result<Option<(usize, SocketAddr)>>,
    {
        self.poll_with_route_receiver(wall_ms, now, quantum, |wire| {
            receiver(wire)
                .map(|value| value.map(|(length, source)| (length, NativeRoute::Udp(source))))
        })
    }

    fn poll_with_route_receiver<F>(
        &mut self,
        wall_ms: u64,
        now: Instant,
        quantum: usize,
        receiver: F,
    ) -> Result<NativePoll>
    where
        F: FnMut(&mut [u8]) -> std::io::Result<Option<(usize, NativeRoute)>>,
    {
        ensure!(
            (1..=64).contains(&quantum),
            "invalid native endpoint quantum"
        );
        // Detach the scratch so the original handshake/session methods can
        // borrow this owner. Restore on EVERY Result path, including I/O errors.
        // Exclusive &mut self prevents a second safe caller from reentering.
        let mut wire = std::mem::take(&mut self.receive_scratch);
        let result = self.poll_with_scratch(wall_ms, now, quantum, receiver, &mut wire);
        self.receive_scratch = wire;
        result
    }

    fn poll_with_scratch<F>(
        &mut self,
        wall_ms: u64,
        now: Instant,
        quantum: usize,
        mut receiver: F,
        wire: &mut [u8],
    ) -> Result<NativePoll>
    where
        F: FnMut(&mut [u8]) -> std::io::Result<Option<(usize, NativeRoute)>>,
    {
        ensure!(
            wire.len() == 65536,
            "native receive scratch capacity changed"
        );
        self.traffic_started = true;
        let mut result = NativePoll::default();
        self.expire_reconnects(now);
        self.admit_waiting_client(now)?;
        self.renew_configured_handshakes(wall_ms, now)?;
        // Full UDP payload buffer ensures even oversized rejected datagrams
        // have exact receive accounting on Windows (no WSAEMSGSIZE truncation).
        // An adopted reliable carrier has no unrelated UDP ICMP notifications.
        // A reset/refusal there must reach the owner so it retires this stream.
        let udp_io = self
            .sources
            .keys()
            .all(|route| matches!(route, NativeRoute::Udp(_)));
        for _ in 0..quantum {
            let (length, source) = match receiver(wire) {
                Ok(Some(value)) => value,
                Ok(None) => continue,
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                // UDP ICMP errors can refer to a different peer's previous send.
                // Count them within this quantum; never tear down healthy peers.
                Err(error)
                    if udp_io
                        && matches!(
                            error.kind(),
                            ErrorKind::ConnectionReset
                                | ErrorKind::ConnectionRefused
                                | ErrorKind::Interrupted
                        ) =>
                {
                    result.receive_errors += 1;
                    continue;
                }
                // Windows reports an oversized UDP datagram as WSAEMSGSIZE.
                Err(error) if udp_io && cfg!(windows) && error.raw_os_error() == Some(10040) => {
                    result.rejected_datagrams += 1;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            if self
                .wire_budget
                .as_mut()
                .is_some_and(|budget| !budget.admit_receive(length, now))
                || length > wire.len()
                || length > MAX_DATAGRAM_BYTES
            {
                result.rejected_datagrams += 1;
                continue;
            }
            if wire[..length].starts_with(b"NVHSHK01") {
                if self
                    .receive_handshake(source, &wire[..length], wall_ms, now)
                    .is_err()
                {
                    result.rejected_datagrams += 1;
                }
                continue;
            }
            if let Some((frame, change)) = self.confirm_reconnect(source, &wire[..length], now) {
                result.frames.push(frame);
                result.session_changes.extend(change);
                continue;
            }
            let Some(&index) = self.sources.get(&source) else {
                result.rejected_datagrams += 1;
                continue;
            };
            let peer = self.peers.get_mut(&index).expect("known native peer");
            if let Some(session) = &mut peer.session {
                match session.open_route(source, &wire[..length]) {
                    Ok(frame)
                        if peer
                            .streams
                            .as_ref()
                            .is_none_or(|streams| streams.contains(&frame.stream_id)) =>
                    {
                        peer.last_authenticated = now;
                        result.frames.push(NativeReceivedFrame {
                            peer: index,
                            identity: peer.id.clone(),
                            frame,
                        })
                    }
                    _ => result.rejected_datagrams += 1,
                }
            } else {
                result.rejected_datagrams += 1;
            }
        }
        for _ in 0..quantum.min(self.peers.len()) {
            let index = self
                .peers
                .range(self.turn..)
                .next()
                .or_else(|| self.peers.first_key_value())
                .map(|(&i, _)| i)
                .expect("nonempty peers");
            self.turn = index.saturating_add(1);
            let peer = self.peers.get_mut(&index).expect("known native peer");
            if let Some(pending) = &mut peer.reconnect {
                match pending.handshake.try_send_limited(
                    self.io.as_ref(),
                    wall_ms,
                    now,
                    self.wire_budget.as_mut(),
                ) {
                    Ok(true) => result.handshakes_sent += 1,
                    Ok(false) => {}
                    Err(_) => result.handshake_send_failures += 1,
                }
            }
            let Some(handshake) = peer.handshake.as_mut() else {
                continue;
            };
            if handshake.is_expired(now) {
                continue;
            }
            match handshake.try_send_limited(
                self.io.as_ref(),
                wall_ms,
                now,
                self.wire_budget.as_mut(),
            ) {
                Ok(true) => result.handshakes_sent += 1,
                Ok(false) => {}
                Err(_) => result.handshake_send_failures += 1,
            }
        }
        // A quantum can contain old data BEFORE the handover. Never hand it to
        // consumers after exposing a replacement generation.
        result
            .frames
            .retain(|frame| self.session_id(frame.peer) == Some(frame.frame.session_id));
        Ok(result)
    }

    pub fn try_send(&mut self, peer: usize, frame: &NovoRudpTransportFrameV0) -> Result<bool> {
        self.traffic_started = true;
        let session = self
            .peers
            .get_mut(&peer)
            .context("unknown native peer")?
            .session
            .as_mut()
            .context("native peer not authenticated")?;
        session.try_send_limited(
            self.io.as_ref(),
            frame,
            Instant::now(),
            self.wire_budget.as_mut(),
        )
    }

    pub(crate) fn uses_deferred_writes(&self) -> bool {
        self.io.deferred_writes()
    }

    pub(crate) fn peer_uses_reliable_carrier(&self, peer: usize) -> bool {
        self.peers
            .get(&peer)
            .and_then(|peer| peer.session.as_ref())
            .is_some_and(NativeUdpSession::is_bound_reliable_carrier)
    }

    pub(crate) fn drain_object_send(
        &mut self,
        peer: usize,
        stream: u64,
        id: [u8; 32],
        now: Instant,
    ) -> Result<bool> {
        let session = self
            .peers
            .get_mut(&peer)
            .context("unknown native peer")?
            .session
            .as_mut()
            .context("native peer not authenticated")?;
        session.drain_object_send(self.io.as_ref(), stream, id, self.wire_budget.as_mut(), now)
    }

    pub(crate) fn cancel_object_send(
        &mut self,
        peer: usize,
        stream: u64,
        id: [u8; 32],
    ) -> Result<()> {
        self.cancel_retained_send(peer, stream, id, false)
    }
    pub(crate) fn cancel_feedback_send(
        &mut self,
        peer: usize,
        stream: u64,
        id: [u8; 32],
    ) -> Result<()> {
        self.cancel_retained_send(peer, stream, id, true)
    }
    fn cancel_retained_send(
        &mut self,
        peer: usize,
        stream: u64,
        id: [u8; 32],
        feedback: bool,
    ) -> Result<()> {
        let session = self
            .peers
            .get_mut(&peer)
            .context("unknown native peer")?
            .session
            .as_mut()
            .context("native peer not authenticated")?;
        session.cancel_send(
            self.io.as_ref(),
            stream,
            id,
            feedback,
            self.wire_budget.as_mut(),
            Instant::now(),
        )
    }

    pub fn poll_object(
        &mut self,
        peer: usize,
        object: &mut NativeObjectSender,
        now: Instant,
        quantum: usize,
    ) -> Result<usize> {
        self.traffic_started = true;
        let session = self
            .peers
            .get_mut(&peer)
            .context("unknown native peer")?
            .session
            .as_mut()
            .context("native peer not authenticated")?;
        object.poll_send_limited(
            self.io.as_ref(),
            session,
            now,
            quantum,
            self.wire_budget.as_mut(),
        )
    }
}

#[cfg(test)]
mod multiplex_tests;
mod reconnect;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod client_set_tests;

#[cfg(test)]
mod io_tests;
#[cfg(test)]
mod retirement_tests;
#[cfg(test)]
mod scratch_tests;
#[cfg(test)]
mod validator_reconnect_tests;
