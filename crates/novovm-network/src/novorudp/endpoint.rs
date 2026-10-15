//! Socket owner for the original native handshake and encrypted frame adapter.
//! No protocol scheduler, execution, ledger, spawned worker or hidden retry queue.
//! One bounded receive quantum precedes a fair handshake-send quantum. Rejected
//! datagrams and an absent peer never terminate another peer's live session.

use super::{
    datagram::{NativeUdpSession, MAX_DATAGRAM_BYTES},
    handshake::NativeHandshake,
    object_sender::NativeObjectSender,
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

pub struct NativeHandshakeBudget {
    pub lifetime: Duration,
    pub retry: Duration,
}

struct Peer {
    id: Arc<str>,
    handshake: NativeHandshake,
    session: Option<NativeUdpSession>,
    streams: Option<BTreeSet<u64>>,
}

struct OpenAdmission {
    identity: SigningKey,
    limit: usize,
    streams: BTreeSet<u64>,
    budget: NativeHandshakeBudget,
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
}

pub struct NativeEndpoint {
    socket: UdpSocket,
    local_key: [u8; 32],
    peers: Vec<Peer>,
    sources: BTreeMap<SocketAddr, usize>,
    replay: HandshakeReplayCacheV1,
    turn: usize,
    open: Option<OpenAdmission>,
}

impl NativeEndpoint {
    pub fn local_public_key(&self) -> [u8; 32] {
        self.local_key
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
        socket.set_nonblocking(true)?;
        let mut endpoint = Self {
            socket,
            local_key: identity.verifying_key().to_bytes(),
            peers: Vec::new(),
            sources: BTreeMap::new(),
            replay: HandshakeReplayCacheV1::new(256),
            turn: 0,
            open: None,
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
        ensure!(
            (1..=64).contains(&peers.len()),
            "native endpoint peer bound"
        );
        socket.set_nonblocking(true)?;
        let mut sources = BTreeMap::new();
        let mut ids = BTreeSet::new();
        let mut owned = Vec::new();
        for (index, peer) in peers.into_iter().enumerate() {
            let id = peer_id_from_ed25519_public_key_v1(&peer.key);
            ensure!(
                ids.insert(id.clone()) && sources.insert(peer.address, index).is_none(),
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
            owned.push(Peer {
                id: id.into(),
                handshake,
                session: None,
                streams: None,
            });
        }
        Ok(Self {
            socket,
            local_key: identity.verifying_key().to_bytes(),
            peers: owned,
            sources,
            replay: HandshakeReplayCacheV1::new(256),
            turn: 0,
            open: None,
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
            additional > 0 && additional <= 64usize.saturating_sub(self.peers.len()),
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
        let local_key = identity.verifying_key().to_bytes();
        let handshake = NativeHandshake::start_role(
            identity,
            remote.key,
            remote.address,
            wall_ms,
            now,
            budget.lifetime,
            budget.retry,
            Some(true),
        )?;
        socket.set_nonblocking(true)?;
        let id = peer_id_from_ed25519_public_key_v1(&remote.key);
        Ok(Self {
            socket,
            local_key,
            peers: vec![Peer {
                id: id.into(),
                handshake,
                session: None,
                streams: None,
            }],
            sources: BTreeMap::from([(remote.address, 0)]),
            replay: HandshakeReplayCacheV1::new(256),
            turn: 0,
            open: None,
        })
    }

    fn accept_client(
        &mut self,
        source: SocketAddr,
        wire: &[u8],
        wall_ms: u64,
        now: Instant,
    ) -> Result<()> {
        use crate::product_overlay::handshake_wire::{decode, HandshakeMessage};
        let open = self
            .open
            .as_ref()
            .context("native client admission disabled")?;
        ensure!(self.peers.len() < open.limit, "native client slots full");
        let HandshakeMessage::Offer(offer) = decode(wire)? else {
            anyhow::bail!("client must initiate")
        };
        let id = peer_id_from_ed25519_public_key_v1(&offer.initiator_identity_public_key);
        ensure!(
            !self.peers.iter().any(|peer| peer.id.as_ref() == id),
            "native identity already bound to an endpoint"
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
        handshake.receive(source, wire, wall_ms, now, &mut self.replay)?;
        let session = handshake
            .take_session()
            .context("client handshake did not authenticate")?;
        self.sources.insert(source, self.peers.len());
        self.peers.push(Peer {
            id: id.into(),
            handshake,
            session: Some(session),
            streams: Some(open.streams.clone()),
        });
        Ok(())
    }
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.socket.local_addr()?)
    }
    pub fn accepts_clients(&self) -> bool {
        self.open.is_some()
    }
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
    pub fn peer_id(&self, index: usize) -> Result<&str> {
        Ok(&self.peers.get(index).context("unknown native peer")?.id)
    }
    pub fn session_id(&self, index: usize) -> Option<[u8; 16]> {
        self.peers
            .get(index)?
            .session
            .as_ref()
            .map(NativeUdpSession::session_id)
    }
    pub fn expired_handshakes(&self, now: Instant) -> usize {
        self.peers
            .iter()
            .filter(|p| p.handshake.is_expired(now))
            .count()
    }

    pub fn poll(&mut self, wall_ms: u64, now: Instant, quantum: usize) -> Result<NativePoll> {
        ensure!(
            (1..=64).contains(&quantum),
            "invalid native endpoint quantum"
        );
        let mut result = NativePoll::default();
        let mut wire = [0; MAX_DATAGRAM_BYTES + 1];
        for _ in 0..quantum {
            let (length, source) = match self.socket.recv_from(&mut wire) {
                Ok(value) => value,
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                // UDP ICMP errors can refer to a different peer's previous send.
                // Count them within this quantum; never tear down healthy peers.
                Err(error)
                    if matches!(
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
                Err(error) if cfg!(windows) && error.raw_os_error() == Some(10040) => {
                    result.rejected_datagrams += 1;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let Some(&index) = self.sources.get(&source) else {
                if self
                    .accept_client(source, &wire[..length], wall_ms, now)
                    .is_err()
                {
                    result.rejected_datagrams += 1;
                }
                continue;
            };
            let peer = &mut self.peers[index];
            if wire[..length].starts_with(b"NVHSHK01") {
                if peer
                    .handshake
                    .receive(source, &wire[..length], wall_ms, now, &mut self.replay)
                    .is_err()
                {
                    result.rejected_datagrams += 1;
                } else if peer.session.is_none() {
                    peer.session = peer.handshake.take_session();
                }
            } else if let Some(session) = &mut peer.session {
                match session.open(source, &wire[..length]) {
                    Ok(frame)
                        if peer
                            .streams
                            .as_ref()
                            .is_none_or(|streams| streams.contains(&frame.stream_id)) =>
                    {
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
            let index = self.turn % self.peers.len();
            self.turn = (index + 1) % self.peers.len();
            let peer = &mut self.peers[index];
            if peer.handshake.is_expired(now) {
                continue;
            }
            match peer.handshake.try_send(&self.socket, wall_ms, now) {
                Ok(true) => result.handshakes_sent += 1,
                Ok(false) => {}
                Err(_) => result.handshake_send_failures += 1,
            }
        }
        Ok(result)
    }

    pub fn try_send(&mut self, peer: usize, frame: &NovoRudpTransportFrameV0) -> Result<bool> {
        let session = self
            .peers
            .get_mut(peer)
            .context("unknown native peer")?
            .session
            .as_mut()
            .context("native peer not authenticated")?;
        session.try_send(&self.socket, frame)
    }

    pub fn poll_object(
        &mut self,
        peer: usize,
        object: &mut NativeObjectSender,
        now: Instant,
        quantum: usize,
    ) -> Result<usize> {
        let session = self
            .peers
            .get_mut(peer)
            .context("unknown native peer")?
            .session
            .as_mut()
            .context("native peer not authenticated")?;
        object.poll_send(&self.socket, session, now, quantum)
    }
}

#[cfg(test)]
mod tests;
