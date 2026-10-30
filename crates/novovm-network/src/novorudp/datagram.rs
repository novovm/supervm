//! Native UDP carrier for the existing authenticated NovoRUDP frame.
//! This adapter does no execution, receipt accounting or consensus admission.
//! A handshake owner supplies an already authenticated channel and selected
//! endpoint. Rebinding requires that owner; an unsolicited source cannot change it.
//! Sending never waits for an ACK. Repair/pacing remain in the original NovoRUDP
//! scheduler; repair seals a NEW envelope sequence rather than replaying ciphertext.

use super::{
    carrier::{NativeCarrierIoFailure, NativeDatagramIo, NativeRoute},
    diagnostics::NativeRejectReasonV1,
    wire_budget::{try_send_datagram, NativeSendReservation, NativeWireBudget},
    NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0,
};
use crate::product_overlay::{binary, E2eSecureChannelV1};
use anyhow::{ensure, Context, Result};
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::time::Instant;

pub const MAX_DATAGRAM_BYTES: usize = 1200;

enum NativeChannel {
    Udp(E2eSecureChannelV1),
    #[cfg(any(feature = "iroh-transport", feature = "tls-transport"))]
    Bound(Box<crate::transport_binding::BoundSecureChannelV1>),
}

pub struct NativeUdpSession {
    channel: NativeChannel,
    remote: NativeRoute,
    pending: Option<PendingWrite>,
}
struct PendingWrite {
    frame: NovoRudpTransportFrameV0,
    wire: Vec<u8>,
    reservation: Option<NativeSendReservation>,
}

impl NativeUdpSession {
    pub fn new(channel: E2eSecureChannelV1, remote: SocketAddr) -> Result<Self> {
        Self::new_route(channel, NativeRoute::Udp(remote))
    }
    pub(crate) fn new_route(channel: E2eSecureChannelV1, remote: NativeRoute) -> Result<Self> {
        remote.validate()?;
        Ok(Self {
            channel: NativeChannel::Udp(channel),
            remote,
            pending: None,
        })
    }
    #[cfg(any(feature = "iroh-transport", feature = "tls-transport"))]
    pub(crate) fn from_bound_channel(
        channel: crate::transport_binding::BoundSecureChannelV1,
        binding: &crate::transport_binding::VerifiedTransportBinding,
    ) -> Result<Self> {
        channel.verify_transport_binding(binding)?;
        Ok(Self {
            channel: NativeChannel::Bound(Box::new(channel)),
            remote: NativeRoute::Carrier(super::carrier::VerifiedCarrierRoute::from_binding(
                binding,
            )),
            pending: None,
        })
    }
    pub(crate) fn is_bound_reliable_carrier(&self) -> bool {
        #[cfg(any(feature = "iroh-transport", feature = "tls-transport"))]
        {
            matches!(self.channel, NativeChannel::Bound(_))
                && matches!(self.remote, NativeRoute::Carrier(_))
        }
        #[cfg(not(any(feature = "iroh-transport", feature = "tls-transport")))]
        false
    }

    pub fn remote_peer_id(&self) -> &str {
        match &self.channel {
            NativeChannel::Udp(channel) => channel.remote_peer_id(),
            #[cfg(any(feature = "iroh-transport", feature = "tls-transport"))]
            NativeChannel::Bound(channel) => channel.remote_peer_id(),
        }
    }
    pub fn session_id(&self) -> [u8; 16] {
        match &self.channel {
            NativeChannel::Udp(channel) => channel.session_id(),
            #[cfg(any(feature = "iroh-transport", feature = "tls-transport"))]
            NativeChannel::Bound(channel) => channel.session_id(),
        }
    }

    pub fn encode(&mut self, frame: &NovoRudpTransportFrameV0) -> Result<Vec<u8>> {
        ensure!(
            frame.session_id == self.session_id(),
            "inner frame session mismatch"
        );
        // Reject oversized application input before encryption/allocation. The
        // exact envelope bound is checked by the existing binary codec as well.
        ensure!(
            frame.payload.len() <= MAX_DATAGRAM_BYTES - 96 - 16,
            "native payload too large"
        );
        let envelope = match &mut self.channel {
            NativeChannel::Udp(channel) => channel.seal_novorudp_frame(frame)?,
            #[cfg(any(feature = "iroh-transport", feature = "tls-transport"))]
            NativeChannel::Bound(channel) => {
                let envelope = channel.seal_novorudp_frame(frame)?;
                crate::product_overlay::SecureNovoRudpEnvelopeV1 {
                    version: envelope.version,
                    session_id: envelope.session_id,
                    sender_peer_id: envelope.sender_peer_id,
                    recipient_peer_id: envelope.recipient_peer_id,
                    sequence: envelope.sequence,
                    nonce: envelope.nonce,
                    ciphertext: envelope.ciphertext,
                }
            }
        };
        let _cost = super::diagnostics::io_span(super::diagnostics::IoPhase::EnvelopeEncode);
        binary::encode(&envelope, MAX_DATAGRAM_BYTES)
    }

    pub fn open(&mut self, source: SocketAddr, wire: &[u8]) -> Result<NovoRudpTransportFrameV0> {
        self.open_route(NativeRoute::Udp(source), wire)
    }

    pub(crate) fn open_route(
        &mut self,
        source: NativeRoute,
        wire: &[u8],
    ) -> Result<NovoRudpTransportFrameV0> {
        self.open_decoded(source, wire.len(), || {
            binary::decode(wire, MAX_DATAGRAM_BYTES)
        })
    }

    pub(crate) fn open_route_owned(
        &mut self,
        source: NativeRoute,
        wire: Vec<u8>,
    ) -> Result<NovoRudpTransportFrameV0> {
        self.open_decoded(source, wire.len(), || {
            binary::decode_owned(wire, MAX_DATAGRAM_BYTES)
        })
    }

    fn open_decoded(
        &mut self,
        source: NativeRoute,
        wire_bytes: usize,
        decode: impl FnOnce() -> Result<crate::product_overlay::SecureNovoRudpEnvelopeV1>,
    ) -> Result<NovoRudpTransportFrameV0> {
        if source != self.remote {
            return Err(anyhow::anyhow!("native packet source mismatch")
                .context(NativeRejectReasonV1::SecureRoute));
        }
        let envelope = {
            let _cost = super::diagnostics::io_span(super::diagnostics::IoPhase::EnvelopeDecode);
            decode().context(NativeRejectReasonV1::WireOrFrame)?
        };
        let frame = match &mut self.channel {
            NativeChannel::Udp(channel) => channel.open_novorudp_frame_owned(envelope)?,
            #[cfg(any(feature = "iroh-transport", feature = "tls-transport"))]
            NativeChannel::Bound(channel) => {
                channel.open_novorudp_frame(&crate::duplex::SecureNovoRudpEnvelopeV1 {
                    version: envelope.version,
                    session_id: envelope.session_id,
                    sender_peer_id: envelope.sender_peer_id,
                    recipient_peer_id: envelope.recipient_peer_id,
                    sequence: envelope.sequence,
                    nonce: envelope.nonce,
                    ciphertext: envelope.ciphertext,
                })?
            }
        };
        if frame.session_id != self.session_id() {
            return Err(anyhow::anyhow!("inner frame session mismatch")
                .context(NativeRejectReasonV1::SecureRoute));
        }
        super::diagnostics::frame_traffic(
            super::diagnostics::Traffic::AuthenticatedReceived,
            &frame,
            wire_bytes,
        );
        Ok(frame)
    }

    /// Caller owns nonblocking socket and fair polling budget. WouldBlock keeps
    /// the original object/frame at the caller; no acceptance is manufactured.
    pub fn try_send(
        &mut self,
        socket: &UdpSocket,
        frame: &NovoRudpTransportFrameV0,
    ) -> Result<bool> {
        self.try_send_limited(socket, frame, Instant::now(), None)
            .map(|sent| sent.is_some())
    }

    pub(crate) fn try_send_limited(
        &mut self,
        io: &dyn NativeDatagramIo,
        frame: &NovoRudpTransportFrameV0,
        now: Instant,
        mut budget: Option<&mut NativeWireBudget>,
    ) -> Result<Option<usize>> {
        if io.deferred_writes() {
            if self.pending.is_none() {
                let wire = self.encode(frame)?;
                let reservation = match budget.as_deref_mut() {
                    Some(budget) => match budget.reserve_send(wire.len(), now)? {
                        Some(reservation) => Some(reservation),
                        None => return Ok(None),
                    },
                    None => None,
                };
                self.pending = Some(PendingWrite {
                    frame: frame.clone(),
                    wire,
                    reservation,
                });
            }
            let pending = self.pending.as_ref().expect("retained write");
            if pending.frame != *frame {
                return Ok(None);
            }
            let result = io.try_send(&pending.wire, self.remote);
            if matches!(&result, Err(error) if error.kind() == ErrorKind::WouldBlock) {
                return Ok(None);
            }
            let pending = self.pending.take().expect("retained write");
            match result {
                Ok(count) if count == pending.wire.len() => {
                    finish_reservation(pending.reservation, budget, count, false, now)?;
                    super::diagnostics::frame_traffic(
                        super::diagnostics::Traffic::SealedSent,
                        frame,
                        count,
                    );
                    return Ok(Some(count));
                }
                Ok(count) => {
                    let known = if count < pending.wire.len() { count } else { 0 };
                    finish_reservation(pending.reservation, budget, known, true, now)?;
                    return Err(NativeCarrierIoFailure(std::io::Error::new(
                        ErrorKind::InvalidData,
                        "incomplete native carrier write",
                    ))
                    .into());
                }
                Err(error) => {
                    finish_reservation(pending.reservation, budget, 0, true, now)?;
                    return Err(NativeCarrierIoFailure(error).into());
                }
            }
        }
        let wire = self.encode(frame)?;
        if try_send_datagram(io, &wire, self.remote, now, budget)? {
            super::diagnostics::frame_traffic(
                super::diagnostics::Traffic::SealedSent,
                frame,
                wire.len(),
            );
            Ok(Some(wire.len()))
        } else {
            Ok(None)
        }
    }

    /// A verified complete object can still have one repair frame in flight.
    /// Finish that exact retained write instead of aborting a partial stream.
    /// WouldBlock keeps its reservation and bytes; no new frame is admitted.
    pub(crate) fn drain_object_send(
        &mut self,
        io: &dyn NativeDatagramIo,
        stream: u64,
        id: [u8; 32],
        budget: Option<&mut NativeWireBudget>,
        now: Instant,
    ) -> Result<bool> {
        let Some(pending) = self.pending.as_ref() else {
            return Ok(true);
        };
        let frame = &pending.frame;
        if frame.stream_id != stream
            || !matches!(
                frame.kind,
                NovoRudpTransportFrameKindV0::Data | NovoRudpTransportFrameKindV0::Repair
            )
            || frame.payload.get(40..72) != Some(id.as_slice())
        {
            return Ok(true);
        }
        let frame = frame.clone();
        self.try_send_limited(io, &frame, now, budget)
            .map(|sent| sent.is_some())
    }

    pub(crate) fn cancel_send(
        &mut self,
        io: &dyn NativeDatagramIo,
        stream: u64,
        id: [u8; 32],
        feedback: bool,
        budget: Option<&mut NativeWireBudget>,
        now: Instant,
    ) -> Result<()> {
        let Some(pending) = self.pending.as_ref() else {
            return Ok(());
        };
        let frame = &pending.frame;
        let matches = frame.stream_id == stream
            && if feedback {
                frame.kind == NovoRudpTransportFrameKindV0::Ack
                    && frame.payload.get(8..40) == Some(id.as_slice())
            } else {
                matches!(
                    frame.kind,
                    NovoRudpTransportFrameKindV0::Data | NovoRudpTransportFrameKindV0::Repair
                ) && frame.payload.get(40..72) == Some(id.as_slice())
            };
        if !matches {
            return Ok(());
        }
        let pending = self.pending.take().expect("matched pending write");
        match io.cancel_send(&pending.wire, self.remote) {
            Ok(count) if count == 0 || count == pending.wire.len() => {
                if count != 0 {
                    super::diagnostics::frame_traffic(
                        super::diagnostics::Traffic::SealedSent,
                        &pending.frame,
                        count,
                    );
                }
                finish_reservation(pending.reservation, budget, count, false, now)
            }
            result => {
                let known = match &result {
                    Ok(count) if *count < pending.wire.len() => *count,
                    _ => 0,
                };
                finish_reservation(pending.reservation, budget, known, true, now)?;
                let error = result.err().unwrap_or_else(|| {
                    std::io::Error::new(ErrorKind::InvalidData, "partial carrier cancellation")
                });
                Err(NativeCarrierIoFailure(error).into())
            }
        }
    }

    /// At most one datagram, without an internal wait/retry loop. The caller must
    /// set nonblocking mode; a +1 sentinel rejects truncated oversized packets.
    pub fn try_receive(&mut self, socket: &UdpSocket) -> Result<Option<NovoRudpTransportFrameV0>> {
        let mut wire = [0u8; MAX_DATAGRAM_BYTES + 1];
        match socket.recv_from(&mut wire) {
            Ok((count, source)) => {
                super::diagnostics::traffic(super::diagnostics::Traffic::WireReceived, 0, 3, count);
                Ok(Some(self.open(source, &wire[..count])?))
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

fn finish_reservation(
    reservation: Option<NativeSendReservation>,
    budget: Option<&mut NativeWireBudget>,
    actual: usize,
    uncertain: bool,
    now: Instant,
) -> Result<()> {
    if let Some(reservation) = reservation {
        let budget =
            budget.ok_or_else(|| anyhow::anyhow!("native write reservation lost its budget"))?;
        budget.finish_send(reservation, actual, uncertain, now)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
