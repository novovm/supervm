//! Client lifecycle v1 and pinned-validator recovery. Reuses the unchanged
//! signed handshake and AEAD frame. A signed offer alone cannot replace an
//! established connection: the peer must prove the fresh channel's secret.
use super::*;

pub(super) struct PendingReconnect {
    source: NativeRoute,
    pub(super) handshake: NativeHandshake,
    session: NativeUdpSession,
    deadline: Instant,
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
            self.peers.iter().all(|peer| peer.configured_key.is_none()
                || (peer.session.is_none() && peer.reconnect.is_none())),
            "configured peer already active"
        );
        policy.reconnect_v1 = true;
        for peer in &mut self.peers {
            if peer.configured_key.is_some() {
                // The first poll creates fresh offers with its current clocks.
                // No old credential is extended or reset to look current.
                peer.handshake = None;
            }
        }
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
        for (index, peer) in self.peers.iter_mut().enumerate() {
            let Some(key) = peer.configured_key else {
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
                key,
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
            .iter()
            .any(|p| p.reconnect.as_ref().is_some_and(|r| r.source == source))
    }

    pub(super) fn expire_reconnects(&mut self, now: Instant) {
        for peer in &mut self.peers {
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
            let peer = &mut self.peers[index];
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
        let peer = &self.peers[index];
        ensure!(
            peer.configured_key == Some(offer.initiator_identity_public_key),
            "configured reconnect identity mismatch"
        );
        if peer.session.is_some() {
            return self.prepare_reconnect(index, source, wire, wall_ms, now);
        }
        // If both endpoints are starting, retain the lower peer's offer.
        // The higher peer validates a responder transcript BEFORE replacing
        // its local attempt; invalid input cannot suppress our outgoing offer.
        let local = peer_id_from_ed25519_public_key_v1(&self.local_key);
        if local.as_str() < peer.id.as_ref() {
            ensure!(
                offer.responder_peer_id == local,
                "configured offer target mismatch"
            );
            crate::product_overlay::validate_handshake_offer_v1(&offer, wall_ms, &mut self.replay)?;
            return Ok(());
        }
        self.prepare_reconnect(index, source, wire, wall_ms, now)?;
        // Initial responders also require fresh-secret confirmation. Otherwise
        // replaying a still-signed pre-restart offer could install an orphaned
        // session and permanently suppress retries. The bounded pending attempt
        // expires normally; an unsuccessful attempt then gets a NEW offer.
        self.peers[index].handshake = None;
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
        let peer = &self.peers[index];
        let (identity, budget) = if peer.configured_key.is_some() {
            ensure!(
                self.sources.get(&source) == Some(&index),
                "configured reconnect source changed"
            );
            let policy = self
                .configured
                .as_ref()
                .context("configured policy missing")?;
            ensure!(
                policy.reconnect_v1,
                "configured reconnect policy not enabled"
            );
            (policy.identity.clone(), policy.budget.clone())
        } else {
            let open = self.open.as_ref().context("client admission disabled")?;
            ensure!(open.reconnect_v1, "native reconnect policy not enabled");
            ensure!(peer.streams.is_some(), "only open clients may reconnect");
            (open.identity.clone(), open.budget.clone())
        };
        ensure!(
            peer.session.is_some() || peer.configured_key.is_some(),
            "only established clients may reconnect"
        );
        ensure!(
            self.sources.get(&source).is_none_or(|i| *i == index),
            "source belongs to another peer"
        );
        if let Some(pending) = &mut self.peers[index].reconnect {
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
            peer_id_from_ed25519_public_key_v1(&offer.initiator_identity_public_key)
                == self.peers[index].id.as_ref(),
            "reconnect changed peer identity"
        );
        let mut handshake = NativeHandshake::start_role(
            identity,
            offer.initiator_identity_public_key,
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
        self.peers[index].reconnect = Some(PendingReconnect {
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
    ) -> Option<(NativeReceivedFrame, Option<NativeSessionChange>)> {
        let index = self
            .peers
            .iter()
            .position(|p| p.reconnect.as_ref().is_some_and(|r| r.source == source))?;
        let peer = &mut self.peers[index];
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
        let pending = peer.reconnect.take()?;
        let previous = peer.session.as_ref().map(NativeUdpSession::session_id);
        let current = pending.session.session_id();
        peer.session = Some(pending.session);
        peer.handshake = Some(pending.handshake);
        self.sources.retain(|_, i| *i != index);
        self.sources.insert(source, index);
        Some((
            NativeReceivedFrame {
                peer: index,
                identity: peer.id.clone(),
                frame,
            },
            previous.map(|previous| NativeSessionChange {
                peer: index,
                previous,
                current,
            }),
        ))
    }
}
