//! Native UDP carrier for the existing authenticated NovoRUDP frame.
//! This adapter does no execution, receipt accounting or consensus admission.
//! A handshake owner supplies an already authenticated channel and selected
//! endpoint. Rebinding requires that owner; an unsolicited source cannot change it.
//! Sending never waits for an ACK. Repair/pacing remain in the original NovoRUDP
//! scheduler; repair seals a NEW envelope sequence rather than replaying ciphertext.

use super::{
    wire_budget::{try_send_datagram, NativeWireBudget},
    NovoRudpTransportFrameV0,
};
use crate::product_overlay::{binary, E2eSecureChannelV1};
use anyhow::{ensure, Result};
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::time::Instant;

pub const MAX_DATAGRAM_BYTES: usize = 1200;

pub struct NativeUdpSession {
    channel: E2eSecureChannelV1,
    remote: SocketAddr,
}

impl NativeUdpSession {
    pub fn new(channel: E2eSecureChannelV1, remote: SocketAddr) -> Result<Self> {
        ensure!(
            !remote.ip().is_unspecified() && !remote.ip().is_multicast() && remote.port() != 0,
            "invalid native peer endpoint"
        );
        Ok(Self { channel, remote })
    }
    pub fn remote_peer_id(&self) -> &str {
        self.channel.remote_peer_id()
    }
    pub fn session_id(&self) -> [u8; 16] {
        self.channel.session_id()
    }

    pub fn encode(&mut self, frame: &NovoRudpTransportFrameV0) -> Result<Vec<u8>> {
        ensure!(
            frame.session_id == self.channel.session_id(),
            "inner frame session mismatch"
        );
        // Reject oversized application input before encryption/allocation. The
        // exact envelope bound is checked by the existing binary codec as well.
        ensure!(
            frame.payload.len() <= MAX_DATAGRAM_BYTES - 96 - 16,
            "native payload too large"
        );
        binary::encode(
            &self.channel.seal_novorudp_frame(frame)?,
            MAX_DATAGRAM_BYTES,
        )
    }

    pub fn open(&mut self, source: SocketAddr, wire: &[u8]) -> Result<NovoRudpTransportFrameV0> {
        ensure!(source == self.remote, "native packet source mismatch");
        let envelope = binary::decode(wire, MAX_DATAGRAM_BYTES)?;
        let frame = self.channel.open_novorudp_frame(&envelope)?;
        ensure!(
            frame.session_id == self.channel.session_id(),
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
        socket: &UdpSocket,
        frame: &NovoRudpTransportFrameV0,
        now: Instant,
        budget: Option<&mut NativeWireBudget>,
    ) -> Result<bool> {
        let wire = self.encode(frame)?;
        try_send_datagram(socket, &wire, self.remote, now, budget)
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
