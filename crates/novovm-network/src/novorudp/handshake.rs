//! Nonblocking native carrier lifecycle around the existing overlay handshake.
//! One bounded attempt per configured peer. No per-object handshake, background
//! thread, registry or license. The caller retains its replay cache across retry
//! attempts. Healthy channels are owned separately and never expire with this
//! startup handshake. Reconnection is an explicit lifecycle operation.

use super::datagram::{NativeUdpSession, MAX_DATAGRAM_BYTES};
use super::wire_budget::{try_send_datagram, NativeWireBudget};
use crate::product_overlay::{
    handshake_wire::{self, HandshakeMessage},
    *,
};
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use std::{
    net::{SocketAddr, UdpSocket},
    time::{Duration, Instant},
};

pub struct NativeHandshake {
    identity: SigningKey,
    remote_peer: String,
    remote: SocketAddr,
    initiator: Option<NodeHandshakeInitiatorV1>,
    cached_offer: Option<Vec<u8>>,
    cached_response: Option<Vec<u8>>,
    outgoing: Option<Vec<u8>>,
    next_send: Instant,
    deadline: Instant,
    ttl_ms: u64,
    retry: Duration,
    outgoing_expiry: u64,
    ready: bool,
    session: Option<NativeUdpSession>,
}

impl NativeHandshake {
    pub fn start(
        identity: SigningKey,
        remote_key: [u8; 32],
        remote: SocketAddr,
        wall_ms: u64,
        now: Instant,
        ttl: Duration,
        retry: Duration,
    ) -> Result<Self> {
        Self::start_role(identity, remote_key, remote, wall_ms, now, ttl, retry, None)
    }

    /// Client/server roles reuse the exact signed V1 transcript. Validator
    /// peers keep deterministic election; an application always initiates.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_role(
        identity: SigningKey,
        remote_key: [u8; 32],
        remote: SocketAddr,
        wall_ms: u64,
        now: Instant,
        ttl: Duration,
        retry: Duration,
        initiate: Option<bool>,
    ) -> Result<Self> {
        ensure!(
            !remote.ip().is_unspecified() && !remote.ip().is_multicast() && remote.port() != 0,
            "invalid native endpoint"
        );
        ensure!(
            !retry.is_zero() && retry < ttl && ttl <= Duration::from_secs(60),
            "invalid native handshake budget"
        );
        let ttl_ms = u64::try_from(ttl.as_millis())?;
        ensure!(ttl_ms > 0, "submillisecond handshake lifetime");
        let remote_peer = peer_id_from_ed25519_public_key_v1(&remote_key);
        let local = peer_id_from_ed25519_public_key_v1(&identity.verifying_key().to_bytes());
        ensure!(local != remote_peer, "native self handshake");
        // One deterministic initiator, not two competing sessions per peer.
        let initiator = if initiate.unwrap_or(local < remote_peer) {
            Some(NodeHandshakeInitiatorV1::start(
                &identity,
                remote_peer.clone(),
                wall_ms,
                ttl_ms,
            )?)
        } else {
            None
        };
        let outgoing = initiator
            .as_ref()
            .map(|i| handshake_wire::encode_offer(i.offer()))
            .transpose()?;
        let outgoing_expiry = initiator
            .as_ref()
            .map(|i| i.offer().expires_at_ms)
            .unwrap_or(0);
        Ok(Self {
            identity,
            remote_peer,
            remote,
            initiator,
            cached_offer: None,
            cached_response: None,
            outgoing,
            next_send: now,
            deadline: now
                .checked_add(ttl)
                .context("handshake deadline overflow")?,
            ttl_ms,
            retry,
            outgoing_expiry,
            ready: false,
            session: None,
        })
    }

    /// Poll a bounded retry without waiting for a response. A full socket does
    /// not advance the retry timer and does not pretend the packet was sent.
    pub fn try_send(&mut self, socket: &UdpSocket, wall_ms: u64, now: Instant) -> Result<bool> {
        self.try_send_limited(socket, wall_ms, now, None)
    }

    pub(crate) fn try_send_limited(
        &mut self,
        socket: &UdpSocket,
        wall_ms: u64,
        now: Instant,
        budget: Option<&mut NativeWireBudget>,
    ) -> Result<bool> {
        if now >= self.deadline || wall_ms > self.outgoing_expiry {
            ensure!(
                self.ready || self.outgoing.is_none(),
                "native handshake expired"
            );
            return Ok(false);
        }
        if now < self.next_send {
            return Ok(false);
        }
        let Some(wire) = &self.outgoing else {
            return Ok(false);
        };
        ensure!(
            wire.len() <= MAX_DATAGRAM_BYTES,
            "handshake datagram too large"
        );
        if try_send_datagram(socket, wire, self.remote, now, budget)? {
            self.next_send = now + self.retry;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn receive(
        &mut self,
        source: SocketAddr,
        wire: &[u8],
        wall_ms: u64,
        now: Instant,
        replay: &mut HandshakeReplayCacheV1,
    ) -> Result<()> {
        ensure!(source == self.remote, "native handshake source mismatch");
        ensure!(now < self.deadline, "native handshake attempt expired");
        match handshake_wire::decode(wire)? {
            HandshakeMessage::Offer(offer) => {
                ensure!(
                    self.initiator.is_none(),
                    "unexpected simultaneous native offer"
                );
                ensure!(
                    offer.initiator_peer_id == self.remote_peer,
                    "unexpected native identity"
                );
                if self.ready {
                    ensure!(
                        self.cached_offer.as_deref() == Some(wire)
                            && wall_ms <= offer.expires_at_ms
                            && wall_ms <= self.outgoing_expiry,
                        "different/expired offer cannot replace live session"
                    );
                    // Response loss: retry the SAME authenticated transcript.
                    // Do not bypass the replay cache to create another channel.
                    self.next_send = now;
                    return Ok(());
                }
                let responder = NodeHandshakeResponderV1::respond(
                    &offer,
                    &self.identity,
                    wall_ms,
                    self.ttl_ms,
                    replay,
                )?;
                self.outgoing_expiry = responder.response().expires_at_ms;
                self.outgoing = Some(handshake_wire::encode_response(responder.response())?);
                self.session = Some(NativeUdpSession::new(
                    responder.into_channel(),
                    self.remote,
                )?);
                self.cached_offer = Some(wire.to_vec());
                self.next_send = now;
                self.ready = true;
            }
            HandshakeMessage::Response(response) => {
                ensure!(
                    response.responder_peer_id == self.remote_peer,
                    "unexpected responder identity"
                );
                if self.ready {
                    ensure!(
                        self.cached_response.as_deref() == Some(wire)
                            && wall_ms <= response.expires_at_ms,
                        "different/expired response cannot replace live session"
                    );
                    return Ok(());
                }
                let initiator = self.initiator.take().context("no pending native offer")?;
                match initiator.complete_owned(&response, wall_ms, replay) {
                    Ok(channel) => {
                        self.session = Some(NativeUdpSession::new(channel, self.remote)?);
                        self.cached_response = Some(wire.to_vec());
                        self.ready = true;
                        self.outgoing = None;
                    }
                    Err(failure) => {
                        let (initiator, error) = *failure;
                        self.initiator = Some(initiator);
                        return Err(error.into());
                    }
                }
            }
        }
        Ok(())
    }
    pub fn take_session(&mut self) -> Option<NativeUdpSession> {
        self.session.take()
    }
    pub fn is_ready(&self) -> bool {
        self.ready
    }
    pub fn is_expired(&self, now: Instant) -> bool {
        !self.ready && now >= self.deadline
    }
}

#[cfg(test)]
mod tests;
