//! Client lifecycle v1 and pinned-validator recovery. Reuses the unchanged
//! signed handshake and AEAD frame. A signed offer alone cannot replace an
//! established connection: the peer must prove the fresh channel's secret.
use super::*;

pub(super) struct PendingReconnect {
    pub(super) source: NativeRoute,
    pub(super) handshake: NativeHandshake,
    pub(super) session: NativeUdpSession,
    pub(super) deadline: Instant,
}

impl NativeEndpoint {
    /// Explicit pinned-peer lifecycle v1, not open-client admission. Install
    /// before any poll/send. Failed attempts get fresh transcripts; either peer
    /// may initiate and responders require ordinary AEAD application traffic.
    /// Thus receive-only adapters retain their original readiness semantics
    /// unless their owner deliberately adopts this bidirectional lifecycle.
    pub fn enable_configured_reconnect_v1(&mut self) -> Result<()> {
        ensure!(
            !self.traffic_started,
            "configured recovery requires an unstarted endpoint"
        );
        let policy = self
            .configured
            .as_mut()
            .context("configured peers missing")?;
        ensure!(!policy.reconnect_v1, "configured recovery already enabled");
        ensure!(
            self.peers.values().all(|peer| peer.configured_key.is_none()
                || (peer.session.is_none() && peer.reconnect.is_none())),
            "configured peer already active"
        );
        policy.reconnect_v1 = true;
        for peer in self.peers.values_mut() {
            if peer.configured_key.is_some() {
                // The first poll creates fresh offers with its current clocks.
                // No old credential is extended or reset to look current.
                peer.handshake = None;
            }
        }
        Ok(())
    }

    /// Learn a configured identity's actual UDP source only after the original
    /// signed handshake and fresh-secret AEAD confirmation. This is an explicit
    /// routing policy, not open admission or permission to replace a pinned key.
    /// The old source/session remains authoritative until confirmation.
    pub fn enable_configured_route_learning_v1(&mut self) -> Result<()> {
        ensure!(
            !self.traffic_started,
            "configured route learning requires an unstarted endpoint"
        );
        let policy = self
            .configured
            .as_mut()
            .context("configured peers missing")?;
        ensure!(
            policy.reconnect_v1,
            "configured route learning requires configured recovery"
        );
        ensure!(
            !policy.route_learning_v1,
            "configured route learning already enabled"
        );
        policy.route_learning_v1 = true;
        Ok(())
    }

    /// Opt in on an explicit open-client listener. Configured validator peers
    /// remain pinned. This is a lifecycle policy, not a wire/credential rewrite.
    /// One pending attempt per existing client, bounded by the original slots.
    pub fn enable_client_reconnect_v1(&mut self) -> Result<()> {
        self.open
            .as_mut()
            .context("client admission disabled")?
            .reconnect_v1 = true;
        Ok(())
    }

    /// Renew one bounded attempt, not its expired credentials. This never
    /// touches a healthy session, the replay cache, identity, route or budget.
    /// A late/restarted peer in either identity order can initiate a new offer.
    pub(super) fn renew_configured_handshakes(&mut self, wall_ms: u64, now: Instant) -> Result<()> {
        let Some(policy) = &self.configured else {
            return Ok(());
        };
        if !policy.reconnect_v1 {
            return Ok(());
        }
        for (&index, peer) in &mut self.peers {
            let Some(key) = &peer.configured_key else {
                continue;
            };
            if peer.session.is_some()
                || peer.reconnect.is_some()
                || peer.handshake.as_ref().is_some_and(|h| !h.is_expired(now))
            {
                continue;
            }
            let route = *self
                .sources
                .iter()
                .find_map(|(route, bound)| (*bound == index).then_some(route))
                .context("configured peer lost its route")?;
            peer.handshake = Some(NativeHandshake::start_role(
                policy.identity.clone(),
                key.clone(),
                route,
                wall_ms,
                now,
                policy.budget.lifetime,
                policy.budget.retry,
                Some(true),
            )?);
        }
        Ok(())
    }

    pub(super) fn reconnect_source_in_use(&self, source: NativeRoute) -> bool {
        self.peers
            .values()
            .chain(self.waiting_clients.iter())
            .any(|p| p.reconnect.as_ref().is_some_and(|r| r.source == source))
    }

    pub(super) fn expire_reconnects(&mut self, now: Instant) {
        for peer in self.peers.values_mut() {
            if peer.reconnect.as_ref().is_some_and(|r| now >= r.deadline) {
                peer.reconnect = None;
            }
        }
    }

    pub(super) fn receive_handshake(
        &mut self,
        source: NativeRoute,
        wire: &[u8],
        wall_ms: u64,
        now: Instant,
    ) -> Result<()> {
        if let Some(&index) = self.sources.get(&source) {
            let peer = self.peers.get_mut(&index).context("unknown native peer")?;
            // Old transcript retries keep their original replay semantics.
            if let Some(handshake) = peer.handshake.as_mut() {
                if handshake
                    .receive_route(source, wire, wall_ms, now, &mut self.replay)
                    .is_ok()
                {
                    if peer.session.is_none() {
                        peer.session = handshake.take_session();
                    }
                    return Ok(());
                }
            }
            if peer.configured_key.is_some()
                && self
                    .configured
                    .as_ref()
                    .is_some_and(|policy| policy.reconnect_v1)
            {
                return self.receive_configured_handshake(index, source, wire, wall_ms, now);
            }
        }
        if !self.sources.contains_key(&source)
            && matches!(source, NativeRoute::Udp(_))
            && self
                .configured
                .as_ref()
                .is_some_and(|policy| policy.route_learning_v1)
        {
            use crate::product_overlay::handshake_wire::{decode, HandshakeMessage};
            if let HandshakeMessage::Offer(offer) = decode(wire)? {
                if let Some(index) = self.peers.iter().find_map(|(&index, peer)| {
                    (peer.configured_key.as_deref()
                        == Some(offer.initiator_identity_public_key.as_slice()))
                    .then_some(index)
                }) {
                    // A NAT-only initiator may be either side of the key-order
                    // election. Its unknown source cannot receive our original
                    // configured offer; authenticate its responder flight instead.
                    // Existing cookie assembly and one pending slot per peer are
                    // retained. A signed offer alone does not install this route.
                    return self.prepare_reconnect(index, source, wire, wall_ms, now);
                }
            }
        }
        self.accept_client(source, wire, wall_ms, now)
    }

    fn receive_configured_handshake(
        &mut self,
        index: usize,
        source: NativeRoute,
        wire: &[u8],
        wall_ms: u64,
        now: Instant,
    ) -> Result<()> {
        use crate::product_overlay::handshake_wire::{decode, HandshakeMessage};
        let HandshakeMessage::Offer(offer) = decode(wire)? else {
            anyhow::bail!("no matching configured handshake response")
        };
        let peer = &self.peers[&index];
        ensure!(
            peer.configured_key.as_deref() == Some(offer.initiator_identity_public_key.as_slice()),
            "configured reconnect identity mismatch"
        );
        if peer.session.is_some() {
            return self.prepare_reconnect(index, source, wire, wall_ms, now);
        }
        // If both endpoints are starting, retain the lower peer's offer.
        // The higher peer validates a responder transcript BEFORE replacing
        // its local attempt; invalid input cannot suppress our outgoing offer.
        let local = peer_id_from_mldsa65_public_key_v1(&self.local_key);
        if local.as_str() < peer.id.as_ref() {
            return self
                .peers
                .get_mut(&index)
                .context("configured peer missing")?
                .handshake
                .as_mut()
                .context("configured attempt missing")?
                .receive_election_offer(wire, &offer, wall_ms, &mut self.replay);
        }
        self.prepare_reconnect(index, source, wire, wall_ms, now)?;
        // Initial responders also require fresh-secret confirmation. Otherwise
        // replaying a still-signed pre-restart offer could install an orphaned
        // session and permanently suppress retries. The bounded pending attempt
        // expires normally; an unsuccessful attempt then gets a NEW offer.
        // Keep the existing bounded initiator flight until confirmation. With
        // a continued transcript, abandoning its last parts would leave the
        // remote source's assembly occupied and block this response. Election
        // still selects the lower peer's offer; only fresh AEAD confirmation
        // installs the pending responder and replaces this initiator state.
        Ok(())
    }

    pub(super) fn prepare_reconnect(
        &mut self,
        index: usize,
        source: NativeRoute,
        wire: &[u8],
        wall_ms: u64,
        now: Instant,
    ) -> Result<()> {
        use crate::product_overlay::handshake_wire::{decode, HandshakeMessage};
        self.expire_reconnects(now);
        let peer = &self.peers[&index];
        let (identity, budget) = if peer.configured_key.is_some() {
            let policy = self
                .configured
                .as_ref()
                .context("configured policy missing")?;
            ensure!(
                policy.reconnect_v1,
                "configured reconnect policy not enabled"
            );
            ensure!(
                self.sources.get(&source) == Some(&index)
                    || (policy.route_learning_v1 && matches!(source, NativeRoute::Udp(_))),
                "configured reconnect source changed"
            );
            (policy.identity.clone(), policy.budget.clone())
        } else {
            let open = self.open.as_ref().context("client admission disabled")?;
            ensure!(open.reconnect_v1, "native reconnect policy not enabled");
            ensure!(peer.streams.is_some(), "only open clients may reconnect");
            (open.identity.clone(), open.budget.clone())
        };
        ensure!(
            peer.session.is_some()
                || peer.configured_key.is_some()
                || self
                    .open
                    .as_ref()
                    .is_some_and(|open| open.retire_clients && peer.streams.is_some()),
            "only established clients may reconnect"
        );
        ensure!(
            self.sources.get(&source).is_none_or(|i| *i == index),
            "source belongs to another peer"
        );
        if let Some(pending) = &mut self
            .peers
            .get_mut(&index)
            .context("unknown native peer")?
            .reconnect
        {
            // Only the identical transcript can retry; no deadline/slot reset.
            return pending
                .handshake
                .receive_route(source, wire, wall_ms, now, &mut self.replay);
        }
        ensure!(
            !self.reconnect_source_in_use(source),
            "source has a pending handshake"
        );
        let HandshakeMessage::Offer(offer) = decode(wire)? else {
            anyhow::bail!("client must initiate")
        };
        ensure!(
            self.peers[&index]
                .configured_key
                .as_deref()
                .is_none_or(|key| { key == offer.initiator_identity_public_key.as_slice() }),
            "configured reconnect identity mismatch"
        );
        ensure!(
            peer_id_from_mldsa65_public_key_v1(&offer.initiator_identity_public_key)
                == self
                    .peers
                    .get_mut(&index)
                    .context("unknown native peer")?
                    .id
                    .as_ref(),
            "reconnect changed peer identity"
        );
        let mut handshake = NativeHandshake::start_role(
            identity,
            offer.initiator_identity_public_key.clone(),
            source,
            wall_ms,
            now,
            budget.lifetime,
            budget.retry,
            Some(false),
        )?;
        handshake.receive_route(source, wire, wall_ms, now, &mut self.replay)?;
        let session = handshake
            .take_session()
            .context("reconnect did not authenticate")?;
        ensure!(
            Some(session.session_id()) != self.session_id(index),
            "reconnect reused a session"
        );
        self.peers
            .get_mut(&index)
            .context("unknown native peer")?
            .reconnect = Some(PendingReconnect {
            source,
            handshake,
            session,
            deadline: now
                .checked_add(budget.lifetime)
                .context("reconnect deadline overflow")?,
        });
        Ok(())
    }

    pub(super) fn confirm_reconnect(
        &mut self,
        source: NativeRoute,
        wire: &[u8],
        now: Instant,
    ) -> Option<(NativeReceivedFrame, Option<NativeSessionChange>)> {
        let index = self.peers.iter().find_map(|(&i, p)| {
            p.reconnect
                .as_ref()
                .is_some_and(|r| r.source == source && now < r.deadline)
                .then_some(i)
        })?;
        if self
            .sources
            .get(&source)
            .is_some_and(|bound| *bound != index)
        {
            return None;
        }
        let peer = self.peers.get_mut(&index)?;
        let frame = peer
            .reconnect
            .as_mut()?
            .session
            .open_route(source, wire)
            .ok()?;
        // Authenticated but forbidden streams do not activate a connection.
        if peer
            .streams
            .as_ref()
            .is_some_and(|streams| !streams.contains(&frame.stream_id))
        {
            return None;
        }
        #[cfg(any(feature = "iroh-transport", feature = "tls-transport"))]
        if self.peer_uses_reliable_carrier(index) {
            self.retire_peer_pending_send(index, now).ok()?;
        }
        let peer = self.peers.get_mut(&index)?;
        let mut pending = peer.reconnect.take()?;
        let previous = peer.session.as_ref().map(NativeUdpSession::session_id);
        let current = pending.session.session_id();
        peer.last_authenticated = now;
        peer.session = Some(pending.session);
        if let Some(previous) = peer.handshake.as_mut() {
            pending.handshake.retain_replaced_flight(previous);
        }
        peer.handshake = Some(pending.handshake);
        self.sources.retain(|_, i| *i != index);
        self.sources.insert(source, index);
        Some((
            NativeReceivedFrame {
                peer: index,
                identity: peer.id.clone(),
                frame,
                wire_bytes: wire.len(),
            },
            previous.map(|previous| NativeSessionChange {
                peer: index,
                previous,
                current,
            }),
        ))
    }
}
