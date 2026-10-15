//! Nonblocking datagram boundary for the original native state machine.
//! This owns no queue, task or retry policy. A reliable-stream adapter must keep
//! datagram framing and bounded partial-write state in its existing IO owner.
#[cfg(any(feature = "iroh-transport", feature = "tls-transport"))]
use crate::transport_binding::VerifiedTransportBinding;
use std::{
    io,
    net::{SocketAddr, UdpSocket},
};

/// Actual transport provenance, never an address synthesized from a peer key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NativeRoute {
    Udp(SocketAddr),
    Carrier(VerifiedCarrierRoute),
}
impl From<SocketAddr> for NativeRoute {
    fn from(address: SocketAddr) -> Self {
        Self::Udp(address)
    }
}
impl NativeRoute {
    pub(crate) fn validate(self) -> anyhow::Result<()> {
        let Self::Udp(address) = self else {
            anyhow::bail!("native carrier requires exporter-bound handshake and Finished");
        };
        anyhow::ensure!(
            !address.ip().is_unspecified() && !address.ip().is_multicast() && address.port() != 0,
            "invalid native peer endpoint"
        );
        Ok(())
    }
}

/// Derived only from the authenticated carrier's real exporter and peer key.
/// The exporter includes the carrier's connection/stream generation. This is
/// routing provenance, not a replacement for the signed native handshake or
/// proof that its transcript is cryptographically bound to this carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VerifiedCarrierRoute {
    peer_key: [u8; 32],
    binding: [u8; 32],
}
impl VerifiedCarrierRoute {
    #[cfg(any(feature = "iroh-transport", feature = "tls-transport"))]
    pub fn from_binding(binding: &VerifiedTransportBinding) -> Self {
        let (peer_key, binding) = binding.native_route_material();
        Self { peer_key, binding }
    }
    pub fn peer_key(&self) -> [u8; 32] {
        self.peer_key
    }
    pub fn connection_binding(&self) -> [u8; 32] {
        self.binding
    }
}

/// The one enclosing IO owner supplies complete native datagrams and their
/// actual provenance. No implementation may block, truncate, invent a source,
/// return success for an incomplete write, or buffer unbounded work. `None`
/// consumes one poll turn belonging to another protocol; WouldBlock ends input.
/// Sending must return the complete datagram length only once accepted by the
/// real carrier. WouldBlock must not consume it; partial writes are errors and
/// cannot advance native object/handshake progress. Stream framing and any
/// partial failure must be handled by the adapter before accepting more work.
/// This interface does not authorize a carrier or attest its privacy properties.
pub trait NativeDatagramIo: Send + Sync {
    fn try_receive(&self, buffer: &mut [u8]) -> io::Result<Option<(usize, NativeRoute)>>;
    fn try_send(&self, bytes: &[u8], route: NativeRoute) -> io::Result<usize>;
    fn local_udp_addr(&self) -> io::Result<SocketAddr> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "carrier has no UDP socket address",
        ))
    }
}
impl NativeDatagramIo for UdpSocket {
    fn try_receive(&self, buffer: &mut [u8]) -> io::Result<Option<(usize, NativeRoute)>> {
        self.recv_from(buffer)
            .map(|(size, source)| Some((size, NativeRoute::Udp(source))))
    }
    fn try_send(&self, bytes: &[u8], route: NativeRoute) -> io::Result<usize> {
        match route {
            NativeRoute::Udp(address) => self.send_to(bytes, address),
            NativeRoute::Carrier(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "authenticated carrier route cannot use UDP send",
            )),
        }
    }
    fn local_udp_addr(&self) -> io::Result<SocketAddr> {
        self.local_addr()
    }
}

#[cfg(all(test, any(feature = "iroh-transport", feature = "tls-transport")))]
mod tests;
