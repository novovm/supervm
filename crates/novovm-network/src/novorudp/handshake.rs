//! Nonblocking native carrier lifecycle around the existing overlay handshake.
//! One bounded attempt per configured peer. No per-object handshake, background
//! thread, registry or license. The caller retains its replay cache across retry
//! attempts. Healthy channels are owned separately and never expire with this
//! startup handshake. Reconnection is an explicit lifecycle operation.

use super::carrier::{NativeDatagramIo, NativeRoute};
use super::datagram::{NativeUdpSession, MAX_DATAGRAM_BYTES};
use super::handshake_continuation;
use super::wire_budget::{try_send_datagram, NativeWireBudget};
use crate::product_overlay::TransportIdentity;
use crate::product_overlay::{
    handshake_wire::{self, HandshakeMessage},
    *,
};
use anyhow::{ensure, Context, Result};
use std::{
    net::{SocketAddr, UdpSocket},
    time::{Duration, Instant},
};

pub struct NativeHandshake {
    identity: TransportIdentity,
    remote_peer: String,
    remote: NativeRoute,
    initiator: Option<NodeHandshakeInitiatorV1>,
    cached_offer: Option<Vec<u8>>,
    cached_response: Option<Vec<u8>>,
    outgoing: Option<Vec<u8>>,
    continuation: Option<handshake_continuation::Sender>,
    retired_continuation: Option<(handshake_continuation::Sender, Instant)>,
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
        identity: TransportIdentity,
        remote_key: Vec<u8>,
        remote: SocketAddr,
        wall_ms: u64,
        now: Instant,
        ttl: Duration,
        retry: Duration,
    ) -> Result<Self> {
        Self::start_role(identity, remote_key, remote, wall_ms, now, ttl, retry, None)
    }

    /// Client/server roles reuse the exact signed ML-DSA-65 transcript. Validator
    /// peers keep deterministic election; an application always initiates.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_role(
        identity: TransportIdentity,
        remote_key: Vec<u8>,
        remote: impl Into<NativeRoute>,
        wall_ms: u64,
        now: Instant,
        ttl: Duration,
        retry: Duration,
        initiate: Option<bool>,
    ) -> Result<Self> {
        let remote = remote.into();
        remote.validate()?;
        ensure!(
            !retry.is_zero() && retry < ttl && ttl <= Duration::from_secs(60),
            "invalid native handshake budget"
        );
        let ttl_ms = u64::try_from(ttl.as_millis())?;
        ensure!(ttl_ms > 0, "submillisecond handshake lifetime");
        ensure!(
            remote_key.len() == crate::product_overlay::MLDSA65_PUBLIC_KEY_BYTES,
            "native handshake requires an ML-DSA-65 public key"
        );
        let remote_peer = peer_id_from_mldsa65_public_key_v1(&remote_key);
        let local = identity.peer_id();
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
            continuation: None,
            retired_continuation: None,
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
        io: &dyn NativeDatagramIo,
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
        if wire.len() > MAX_DATAGRAM_BYTES {
            ensure!(
                matches!(self.remote, NativeRoute::Udp(_)),
                "continuation requires UDP"
            );
            if self.continuation.is_none() {
                self.continuation = Some(handshake_continuation::Sender::new(wire)?);
            }
            let sender = self
                .continuation
                .as_mut()
                .expect("initialized continuation");
            let datagram = sender.datagram(wire)?;
            if !try_send_datagram(io, &datagram, self.remote, now, budget)? {
                return Ok(false);
            }
            if sender.sent() {
                self.next_send = now + self.retry;
            }
            return Ok(true);
        }
        if try_send_datagram(io, wire, self.remote, now, budget)? {
            self.next_send = now + self.retry;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Observe the existing result, never run a second admission or IO decision.
    pub(crate) fn try_send_observed(
        &mut self,
        io: &dyn NativeDatagramIo,
        wall_ms: u64,
        now: Instant,
        budget: Option<&mut NativeWireBudget>,
        observer: Option<(&super::diagnostics::NativeTransferDiagnosticsV1, usize)>,
    ) -> Result<bool> {
        let Some((observer, peer)) = observer else {
            return self.try_send_limited(io, wall_ms, now, budget);
        };
        use super::diagnostics::UdpRecoveryEvent as Event;
        let before = budget.as_deref().map(NativeWireBudget::send_deferred_count);
        let mut budget = budget;
        let result = self.try_send_limited(io, wall_ms, now, budget.as_deref_mut());
        let after = budget.as_deref().map(NativeWireBudget::send_deferred_count);
        let event = match &result {
            Ok(true) => Event::DatagramSent,
            _ if now >= self.deadline => Event::AttemptDeadline,
            _ if wall_ms > self.outgoing_expiry && self.outgoing.is_some() => Event::SignedExpiry,
            Err(_) => Event::SendFailed,
            Ok(false) if now < self.next_send => Event::RetryWait,
            Ok(false) if self.outgoing.is_none() => Event::NoTranscript,
            Ok(false) if before != after => Event::BudgetDeferred,
            Ok(false) => Event::IoWouldBlock,
        };
        let os_error = result
            .as_ref()
            .err()
            .and_then(|e| e.downcast_ref::<std::io::Error>())
            .and_then(std::io::Error::raw_os_error);
        observer.udp_recovery(peer, event, None, now, os_error);
        result
    }

    pub(super) fn receive_continuation_challenge(
        &mut self,
        source: NativeRoute,
        wire: &[u8],
        now: Instant,
    ) -> Result<()> {
        ensure!(
            source == self.remote && now < self.deadline,
            "unexpected/expired continuation challenge"
        );
        let newly_accepted = match self
            .continuation
            .as_mut()
            .context("no large outgoing handshake")
            .and_then(|sender| sender.challenge(wire))
        {
            Ok(accepted) => accepted,
            Err(error) => {
                // Election can replace our Init with the peer's authenticated
                // response while the Init's cookie is still in flight. Only
                // that exact former flight, within its ORIGINAL deadline, is
                // harmless; it never resumes a sender or changes the session.
                if self
                    .retired_continuation
                    .as_mut()
                    .is_some_and(|(sender, deadline)| {
                        now < *deadline && sender.challenge(wire).is_ok()
                    })
                {
                    return Ok(());
                }
                return Err(error);
            }
        };
        // The challenge unlocks the current cached flight, not a new transcript
        // or peer; it does not extend either signed expiry or attempt deadline.
        if newly_accepted {
            self.next_send = now;
        }
        Ok(())
    }

    pub(super) fn retain_replaced_flight(&mut self, previous: &mut Self) {
        if self.remote == previous.remote && self.remote_peer == previous.remote_peer {
            // One previous flight, not a transcript/session history. A later
            // replacement overwrites the slot without extending its expiry.
            self.retired_continuation = previous
                .continuation
                .take()
                .map(|sender| (sender, previous.deadline));
        }
    }

    pub(super) fn receive_election_offer(
        &mut self,
        wire: &[u8],
        offer: &NodeHandshakeOfferV1,
        wall_ms: u64,
        replay: &mut HandshakeReplayCacheV1,
    ) -> Result<()> {
        ensure!(
            offer.responder_peer_id == self.identity.peer_id(),
            "configured offer target mismatch"
        );
        validate_handshake_offer_v1(offer, wall_ms, replay, &self.identity)?;
        // Reuse the original exact-offer retry slot. Losing election does not
        // make an already verified transcript a new reconnect on its next retry.
        self.cached_offer = Some(wire.to_vec());
        Ok(())
    }

    pub fn receive(
        &mut self,
        source: SocketAddr,
        wire: &[u8],
        wall_ms: u64,
        now: Instant,
        replay: &mut HandshakeReplayCacheV1,
    ) -> Result<()> {
        self.receive_route(NativeRoute::Udp(source), wire, wall_ms, now, replay)
    }

    pub(crate) fn receive_route(
        &mut self,
        source: NativeRoute,
        wire: &[u8],
        wall_ms: u64,
        now: Instant,
        replay: &mut HandshakeReplayCacheV1,
    ) -> Result<()> {
        ensure!(source == self.remote, "native handshake source mismatch");
        ensure!(now < self.deadline, "native handshake attempt expired");
        match handshake_wire::decode(wire)? {
            HandshakeMessage::Offer(offer) => {
                if self.cached_offer.as_deref() == Some(wire) {
                    ensure!(
                        wall_ms <= offer.expires_at_ms && wall_ms <= self.outgoing_expiry,
                        "expired offer cannot retry a live attempt"
                    );
                    // Only the responder retries its authenticated response;
                    // a losing offer cannot accelerate the elected initiator.
                    if self.ready && self.outgoing.is_some() {
                        self.next_send = now;
                    }
                    return Ok(());
                }
                ensure!(
                    self.initiator.is_none(),
                    "unexpected simultaneous native offer"
                );
                ensure!(
                    offer.initiator_peer_id == self.remote_peer,
                    "unexpected native identity"
                );
                ensure!(
                    !self.ready,
                    "different/expired offer cannot replace live session"
                );
                let responder = NodeHandshakeResponderV1::respond(
                    &offer,
                    &self.identity,
                    wall_ms,
                    self.ttl_ms,
                    replay,
                )?;
                self.outgoing_expiry = responder.response().expires_at_ms;
                self.outgoing = Some(handshake_wire::encode_response(responder.response())?);
                self.continuation = None;
                self.session = Some(NativeUdpSession::new_route(
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
                        self.session = Some(NativeUdpSession::new_route(channel, self.remote)?);
                        self.cached_response = Some(wire.to_vec());
                        self.ready = true;
                        self.outgoing = None;
                        // Retain this one flight's bounded metadata so a delayed
                        // Init reply still checks its original commitment and
                        // deadline. No transcript remains queued for sending.
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
    #[cfg(test)]
    pub(super) fn outgoing_wire_for_test(&self) -> Option<&[u8]> {
        self.outgoing.as_deref()
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
