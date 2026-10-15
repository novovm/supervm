//! Native UDP carrier for the existing authenticated NovoRUDP frame.
//! This adapter does no execution, receipt accounting or consensus admission.
//! A handshake owner supplies an already authenticated channel and selected
//! endpoint. Rebinding requires that owner; an unsolicited source cannot change it.
//! Sending never waits for an ACK. Repair/pacing remain in the original NovoRUDP
//! scheduler; repair seals a NEW envelope sequence rather than replaying ciphertext.

use super::{
    carrier::{NativeDatagramIo, NativeRoute},
    wire_budget::{try_send_datagram, NativeWireBudget},
    NovoRudpTransportFrameV0,
};
use crate::product_overlay::{binary, E2eSecureChannelV1};
use anyhow::{ensure, Result};
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
        })
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
        ensure!(source == self.remote, "native packet source mismatch");
        let envelope = binary::decode(wire, MAX_DATAGRAM_BYTES)?;
        let frame = match &mut self.channel {
            NativeChannel::Udp(channel) => channel.open_novorudp_frame(&envelope)?,
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
        ensure!(
            frame.session_id == self.session_id(),
            "inner frame session mismatch"
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
    }

    pub(crate) fn try_send_limited(
        &mut self,
        io: &dyn NativeDatagramIo,
        frame: &NovoRudpTransportFrameV0,
        now: Instant,
        budget: Option<&mut NativeWireBudget>,
    ) -> Result<bool> {
        let wire = self.encode(frame)?;
        try_send_datagram(io, &wire, self.remote, now, budget)
    }

    /// At most one datagram, without an internal wait/retry loop. The caller must
    /// set nonblocking mode; a +1 sentinel rejects truncated oversized packets.
    pub fn try_receive(&mut self, socket: &UdpSocket) -> Result<Option<NovoRudpTransportFrameV0>> {
        let mut wire = [0u8; MAX_DATAGRAM_BYTES + 1];
        match socket.recv_from(&mut wire) {
            Ok((count, source)) => Ok(Some(self.open(source, &wire[..count])?)),
            Err(error) if error.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests;
