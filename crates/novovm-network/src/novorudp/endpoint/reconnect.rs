//! Explicit client lifecycle v1. Reuses the unchanged signed handshake and
//! AEAD frame. A signed offer alone cannot replace an established connection:
//! the new source must prove possession of the fresh channel's secret.
use super::*;

pub(super) struct PendingReconnect {
    source: NativeRoute,
    pub(super) handshake: NativeHandshake,
    session: NativeUdpSession,
    deadline: Instant,
}

impl NativeEndpoint {
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
            if peer
                .handshake
                .receive_route(source, wire, wall_ms, now, &mut self.replay)
                .is_ok()
            {
                if peer.session.is_none() {
                    peer.session = peer.handshake.take_session();
                }
                return Ok(());
            }
        }
        self.accept_client(source, wire, wall_ms, now)
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
        let open = self.open.as_ref().context("client admission disabled")?;
        ensure!(open.reconnect_v1, "native reconnect policy not enabled");
        ensure!(
            self.peers[index].streams.is_some() && self.peers[index].session.is_some(),
            "only established open clients may reconnect"
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
                .checked_add(open.budget.lifetime)
                .context("reconnect deadline overflow")?,
        });
        Ok(())
    }

    pub(super) fn confirm_reconnect(
        &mut self,
        source: NativeRoute,
        wire: &[u8],
    ) -> Option<(NativeReceivedFrame, NativeSessionChange)> {
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
        if !peer.streams.as_ref()?.contains(&frame.stream_id) {
            return None;
        }
        let pending = peer.reconnect.take()?;
        let previous = peer.session.as_ref()?.session_id();
        let current = pending.session.session_id();
        peer.session = Some(pending.session);
        peer.handshake = pending.handshake;
        self.sources.retain(|_, i| *i != index);
        self.sources.insert(source, index);
        Some((
            NativeReceivedFrame {
                peer: index,
                identity: peer.id.clone(),
                frame,
            },
            NativeSessionChange {
                peer: index,
                previous,
                current,
            },
        ))
    }
}
