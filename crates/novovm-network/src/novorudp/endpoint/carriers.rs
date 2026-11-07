//! Compose already authenticated carrier sessions under the SAME object owner.
//! No new wire format, receive queue, connection, worker or authentication path.
use super::*;
use std::{
    io,
    sync::atomic::{AtomicUsize, Ordering},
    sync::RwLock,
};

impl NativeEndpoint {
    /// Join fresh endpoints returned by the original carrier pumps, before
    /// constructing one NativeObjectLink. All lanes then retain their original
    /// GLOBAL budgets, rather than multiplying them by the number of carriers.
    /// Pumps and their authenticated stream lifetimes remain with the caller.
    pub fn join_bound_carriers_v1(endpoints: Vec<Self>) -> Result<Self> {
        ensure!(!endpoints.is_empty(), "no bound native carriers");
        let mut identities = BTreeSet::new();
        let mut routes = BTreeSet::new();
        for endpoint in &endpoints {
            endpoint.validate_bound_carriers_v1(&[])?;
            ensure!(
                endpoint.peers.len() == 1,
                "join requires individual carrier endpoints"
            );
            ensure!(
                endpoint.local_key == endpoints[0].local_key,
                "different local carrier identities"
            );
            let peer = endpoint
                .peers
                .values()
                .next()
                .expect("validated carrier peer");
            ensure!(
                identities.insert(peer.id.clone()),
                "duplicate bound native peer"
            );
            ensure!(
                routes.insert(endpoint.peer_route(0)?),
                "duplicate bound carrier route"
            );
            ensure!(
                endpoint.wire_budget.is_none(),
                "set aggregate wire budget after carrier join"
            );
        }
        let mut endpoints = endpoints.into_iter();
        let mut combined = endpoints.next().expect("nonempty carrier endpoints");
        let mut io = vec![(combined.peer_route(0)?, combined.io.clone())];
        for mut endpoint in endpoints {
            let index = combined.next_peer;
            combined.next_peer = index.checked_add(1).context("native peer index overflow")?;
            let route = endpoint.peer_route(0)?;
            let peer = endpoint.peers.remove(&0).expect("validated carrier peer");
            combined.bound_keys.insert(
                index,
                endpoint
                    .bound_keys
                    .remove(&0)
                    .context("carrier identity proof missing")?,
            );
            combined.peers.insert(index, peer);
            combined.sources.insert(route, index);
            io.push((route, endpoint.io));
        }
        if io.len() > 1 {
            let carriers = Arc::new(BoundCarrierSet::new(io));
            combined.io = carriers.clone();
            combined.bound_carriers = Some(carriers);
        }
        Ok(combined)
    }

    /// Check capability before a consumer adopts this endpoint. Merely having
    /// a carrier address never passes: each session must have verified Finished
    /// on that exact exporter and must admit every requested native stream.
    pub fn validate_bound_carriers_v1(&self, streams: &[u64]) -> Result<()> {
        ensure!(
            !self.traffic_started
                && self.bound_carriers.is_some()
                && self.open.is_none()
                && self.configured.is_none()
                && !self.peers.is_empty()
                && self.sources.len() == self.peers.len(),
            "native consumer requires fresh, exclusively bound carrier sessions"
        );
        for (&index, peer) in &self.peers {
            ensure!(
                self.peer_uses_reliable_carrier(index)
                    && self.bound_keys.get(&index).is_some_and(|key| key.len()
                        == crate::product_overlay::MLDSA65_PUBLIC_KEY_BYTES
                        && peer_id_from_mldsa65_public_key_v1(key) == peer.id.as_ref())
                    && matches!(self.peer_route(index)?, NativeRoute::Carrier(_))
                    && peer
                        .streams
                        .as_ref()
                        .is_some_and(|allowed| streams.iter().all(|s| allowed.contains(s))),
                "carrier session does not admit required native streams"
            );
        }
        Ok(())
    }

    // Only NativeObjectLink exposes replacement: it MUST consume the generation
    // change immediately so queued objects cannot cross the authentication boundary.
    pub(crate) fn replace_bound_carrier_v1(
        &mut self,
        index: usize,
        mut replacement: Self,
        now: Instant,
    ) -> Result<Option<NativeSessionChange>> {
        replacement.validate_bound_carriers_v1(&[])?;
        ensure!(
            replacement.peers.len() == 1 && replacement.wire_budget.is_none(),
            "replacement requires one fresh carrier without an independent budget"
        );
        ensure!(
            self.local_key == replacement.local_key,
            "replacement changed local identity"
        );
        let previous = self.session_id(index);
        let old = self
            .peers
            .get(&index)
            .context("unknown native carrier peer")?;
        let fresh = replacement
            .peers
            .get(&0)
            .context("missing replacement peer")?;
        let fresh_key = replacement
            .bound_keys
            .get(&0)
            .context("missing bound identity proof")?;
        let pinned_key = old
            .configured_key
            .as_ref()
            .or_else(|| self.bound_keys.get(&index))
            .context("carrier replacement requires a pinned peer")?;
        ensure!(
            old.id == fresh.id
                && pinned_key == fresh_key
                && old
                    .streams
                    .as_ref()
                    .is_none_or(|streams| Some(streams) == fresh.streams.as_ref()),
            "replacement changed peer identity or stream contract"
        );
        ensure!(
            old.configured_key.is_none()
                || self.configured.as_ref().is_some_and(|p| p.reconnect_v1),
            "configured carrier recovery requires explicit reconnect policy"
        );
        let current = replacement
            .session_id(0)
            .context("replacement has no session")?;
        let route = replacement.peer_route(0)?;
        let old_route = self.sources.iter().find_map(|(route, peer)| {
            (*peer == index && matches!(route, NativeRoute::Carrier(_))).then_some(*route)
        });
        ensure!(
            previous != Some(current)
                && old_route != Some(route)
                && !self.sources.contains_key(&route)
                && !self.reconnect_source_in_use(route),
            "replacement reused session or carrier route"
        );
        let configured_key = old.configured_key.clone();
        self.retire_peer_pending_send(index, now)?;
        // Never wrap/replace the UDP IO: its receive authority and synchronous
        // batch send capability remain exactly where they were established.
        let carriers = self
            .bound_carriers
            .get_or_insert_with(|| Arc::new(BoundCarrierSet::new(Vec::new())));
        carriers.install(old_route, route, replacement.io)?;
        let mut fresh = replacement.peers.remove(&0).expect("validated replacement");
        fresh.configured_key = configured_key;
        self.bound_keys.insert(index, fresh_key.clone());
        self.peers.insert(index, fresh);
        if let Some(old_route) = old_route {
            self.sources.remove(&old_route);
        }
        // Keep at most the original configured UDP source as a pinned alias.
        // It grants no old session: AEAD still checks the active carrier route.
        self.sources.insert(route, index);
        Ok(previous.map(|previous| NativeSessionChange {
            peer: index,
            previous,
            current,
        }))
    }

    /// Retry the original configured UDP address while the carrier remains
    /// active. The response still proves the full original signed transcript;
    /// a responder changes route only after its ordinary AEAD confirmation.
    pub fn request_udp_recovery_v1(
        &mut self,
        index: usize,
        wall_ms: u64,
        now: Instant,
    ) -> Result<()> {
        let policy = self
            .configured
            .as_ref()
            .context("not a configured UDP owner")?;
        ensure!(
            policy.reconnect_v1 && self.peer_uses_reliable_carrier(index),
            "UDP recovery requires an active configured carrier"
        );
        let route = self
            .sources
            .iter()
            .find_map(|(route, peer)| {
                (*peer == index && matches!(route, NativeRoute::Udp(_))).then_some(*route)
            })
            .context("configured UDP route missing")?;
        let peer = self.peers.get_mut(&index).context("unknown native peer")?;
        // An existing bounded attempt retains its original deadline. Repeated
        // owner maintenance requests neither restart it nor kill the fallback.
        if peer.reconnect.is_some() {
            return Ok(());
        }
        // Both owners may retry together. Elect one initiator exactly as the
        // original configured handshake; the other keeps its carrier until
        // prepare/confirm_reconnect proves the new UDP secret.
        if peer_id_from_mldsa65_public_key_v1(&self.local_key).as_str() > peer.id.as_ref() {
            return Ok(());
        }
        if peer.handshake.as_ref().is_some_and(|h| !h.is_expired(now)) {
            return Ok(());
        }
        peer.handshake = Some(NativeHandshake::start_role(
            policy.identity.clone(),
            peer.configured_key
                .clone()
                .context("not a configured peer")?,
            route,
            wall_ms,
            now,
            policy.budget.lifetime,
            policy.budget.retry,
            Some(true),
        )?);
        Ok(())
    }

    pub(super) fn promote_udp_recoveries(
        &mut self,
        result: &mut NativePoll,
        now: Instant,
    ) -> Result<()> {
        if self.configured.is_none() || self.bound_carriers.is_none() {
            return Ok(());
        }
        let mut fresh_sessions = Vec::new();
        for (&index, peer) in &mut self.peers {
            if peer
                .session
                .as_ref()
                .is_none_or(|session| !session.is_bound_reliable_carrier())
            {
                continue;
            }
            if let Some(fresh) = peer
                .handshake
                .as_mut()
                .and_then(NativeHandshake::take_session)
            {
                fresh_sessions.push((index, fresh));
            }
        }
        for (index, fresh) in fresh_sessions {
            let route = self
                .sources
                .iter()
                .find_map(|(route, bound)| {
                    (*bound == index && matches!(route, NativeRoute::Udp(_))).then_some(*route)
                })
                .context("authenticated UDP recovery lost configured route")?;
            let previous = self.session_id(index).context("active carrier missing")?;
            let current = fresh.session_id();
            ensure!(previous != current, "UDP recovery reused session");
            self.retire_peer_pending_send(index, now)?;
            let peer = self
                .peers
                .get_mut(&index)
                .context("configured peer missing")?;
            peer.session = Some(fresh);
            peer.reconnect = None;
            peer.last_authenticated = now;
            self.sources.retain(|_, bound| *bound != index);
            self.sources.insert(route, index);
            result.session_changes.push(NativeSessionChange {
                peer: index,
                previous,
                current,
            });
        }
        if self.bound_keys.keys().all(|index| {
            self.peers.get(index).is_some_and(|peer| {
                peer.session
                    .as_ref()
                    .is_some_and(NativeUdpSession::is_bound_reliable_carrier)
            })
        }) {
            return Ok(());
        }
        // Passive original UDP reconnect confirmation also arrives through this
        // owner. Detach its old stream before another receive poll can fail it.
        let routes = self
            .sources
            .keys()
            .filter(|route| matches!(route, NativeRoute::Carrier(_)))
            .copied()
            .collect();
        self.bound_carriers
            .as_ref()
            .expect("checked carrier set")
            .retain_routes(&routes)?;
        self.bound_keys.retain(|index, _| {
            self.peers.get(index).is_some_and(|peer| {
                peer.session
                    .as_ref()
                    .is_some_and(NativeUdpSession::is_bound_reliable_carrier)
            })
        });
        Ok(())
    }

    pub(super) fn retire_peer_pending_send(&mut self, index: usize, now: Instant) -> Result<()> {
        let io = self.peer_io(index)?;
        let Some(session) = self
            .peers
            .get_mut(&index)
            .context("unknown native peer")?
            .session
            .as_mut()
        else {
            return Ok(());
        };
        match session.retire_pending_send(io.as_ref(), self.wire_budget.as_mut(), now) {
            // A forced authenticated handover retires this old stream anyway.
            // The original cancellation already resolved its reservation as
            // uncertain; it is not a successful object or actual byte count.
            Err(error)
                if error
                    .downcast_ref::<super::super::carrier::NativeCarrierIoFailure>()
                    .is_some() =>
            {
                Ok(())
            }
            result => result,
        }
    }
}

pub(super) struct BoundCarrierSet {
    io: RwLock<Vec<(NativeRoute, Arc<dyn NativeDatagramIo>)>>,
    turn: AtomicUsize,
    ingress_turn: AtomicUsize,
}
impl BoundCarrierSet {
    pub(super) fn new(io: Vec<(NativeRoute, Arc<dyn NativeDatagramIo>)>) -> Self {
        Self {
            io: RwLock::new(io),
            turn: AtomicUsize::new(0),
            ingress_turn: AtomicUsize::new(0),
        }
    }
    fn install(
        &self,
        old: Option<NativeRoute>,
        route: NativeRoute,
        io: Arc<dyn NativeDatagramIo>,
    ) -> Result<()> {
        let mut routes = self
            .io
            .write()
            .map_err(|_| anyhow::anyhow!("bound carrier owner poisoned"))?;
        ensure!(
            !routes.iter().any(|(expected, _)| *expected == route),
            "duplicate carrier IO"
        );
        if let Some(old) = old {
            let entry = routes
                .iter_mut()
                .find(|(expected, _)| *expected == old)
                .context("old carrier IO missing")?;
            *entry = (route, io);
        } else {
            routes.push((route, io));
        }
        Ok(())
    }
    fn retain_routes(&self, active: &BTreeSet<NativeRoute>) -> Result<()> {
        self.io
            .write()
            .map_err(|_| anyhow::anyhow!("bound carrier owner poisoned"))?
            .retain(|(route, _)| active.contains(route));
        Ok(())
    }
    pub(super) fn for_route(&self, route: NativeRoute) -> io::Result<Arc<dyn NativeDatagramIo>> {
        self.io
            .read()
            .map_err(|_| io::Error::other("bound carrier owner poisoned"))?
            .iter()
            .find(|(expected, _)| *expected == route)
            .map(|(_, io)| io.clone())
            .ok_or_else(|| io::ErrorKind::InvalidInput.into())
    }
    pub(super) fn receive_with<F>(
        &self,
        buffer: &mut [u8],
        receiver: &mut F,
    ) -> io::Result<Option<ReceivedDatagram>>
    where
        F: FnMut(&mut [u8]) -> io::Result<Option<ReceivedDatagram>>,
    {
        let carrier_first = self
            .ingress_turn
            .fetch_add(1, Ordering::Relaxed)
            .is_multiple_of(2);
        let carrier = |buffer: &mut [u8]| {
            self.try_receive(buffer)
                .map(|packet| packet.map(|(len, source)| ReceivedDatagram::Borrowed(len, source)))
                .map_err(|error| match error.kind() {
                    io::ErrorKind::WouldBlock => error,
                    // The owning pump reports its actual stream failure to the
                    // reconnect caller. Count a bounded receive error here without
                    // taking healthy UDP peers/clients down with that one stream.
                    io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionRefused
                    | io::ErrorKind::BrokenPipe => {
                        io::Error::new(io::ErrorKind::Interrupted, error)
                    }
                    _ => io::Error::other(error),
                })
        };
        let first = if carrier_first {
            carrier(buffer)
        } else {
            receiver(buffer)
        };
        match first {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if carrier_first {
                    receiver(buffer)
                } else {
                    carrier(buffer)
                }
            }
            value => value,
        }
    }
}
impl NativeDatagramIo for BoundCarrierSet {
    fn try_receive(&self, buffer: &mut [u8]) -> io::Result<Option<(usize, NativeRoute)>> {
        let routes = self
            .io
            .read()
            .map_err(|_| io::Error::other("bound carrier owner poisoned"))?;
        if routes.is_empty() {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let start = self.turn.fetch_add(1, Ordering::Relaxed) % routes.len();
        // One pass over the caller's admitted set, with no internal polling loop
        // or buffering. An idle first carrier cannot starve another live peer.
        for offset in 0..routes.len() {
            let (expected, io) = &routes[(start + offset) % routes.len()];
            match io.try_receive(buffer) {
                Ok(Some((_, source))) if source != *expected => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "bound carrier source changed",
                    ));
                }
                Ok(Some(value)) => return Ok(Some(value)),
                Ok(None) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
        }
        Err(io::ErrorKind::WouldBlock.into())
    }
    fn try_send(&self, bytes: &[u8], route: NativeRoute) -> io::Result<usize> {
        self.for_route(route)?.try_send(bytes, route)
    }
    fn cancel_send(&self, bytes: &[u8], route: NativeRoute) -> io::Result<usize> {
        self.for_route(route)?.cancel_send(bytes, route)
    }
    fn deferred_writes(&self) -> bool {
        self.io.read().map_or(true, |routes| {
            routes.iter().any(|(_, io)| io.deferred_writes())
        })
    }
}
