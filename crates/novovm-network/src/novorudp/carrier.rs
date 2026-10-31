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
    /// Synchronous UDP-only batching. Return the number of complete datagrams
    /// accepted from the prefix; an error means none were accepted. This never
    /// authorizes a larger poll quantum or an asynchronous buffered success.
    fn send_batch_limit(&self) -> usize {
        1
    }
    fn try_send_batch(&self, _datagrams: &[&[u8]], _route: NativeRoute) -> io::Result<usize> {
        Err(io::ErrorKind::Unsupported.into())
    }
    /// A scoped reliable carrier retains exactly one write until real completion.
    /// The original session reserves its budget and keeps the exact sealed frame.
    fn deferred_writes(&self) -> bool {
        false
    }
    /// Cancel the exact retained write. Zero means it never started; a full count
    /// means it already completed. Partial/uncertain cancellation retires the carrier.
    fn cancel_send(&self, _bytes: &[u8], _route: NativeRoute) -> io::Result<usize> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn local_udp_addr(&self) -> io::Result<SocketAddr> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "carrier has no UDP socket address",
        ))
    }
}

/// Only the original transport can prove that its private socket has no other
/// writer before transferring ownership. Shared public socket entrypoints do
/// not opt into a socket-wide segmentation setting.
pub(crate) fn owned_udp_io(
    socket: std::sync::Arc<UdpSocket>,
    exclusive: bool,
) -> std::sync::Arc<dyn NativeDatagramIo> {
    #[cfg(windows)]
    if exclusive && novovm_udp_batch::set_udp_send_segment_size(&socket, 0).is_ok() {
        return std::sync::Arc::new(SegmentedUdpIo {
            socket,
            segment_size: std::sync::Mutex::new(0),
        });
    }
    #[cfg(not(windows))]
    let _ = exclusive;
    socket
}

#[cfg(windows)]
struct SegmentedUdpIo {
    socket: std::sync::Arc<UdpSocket>,
    // Serialize the socket-wide setting and its synchronous send. No queue or
    // worker is introduced; all original ACK/handshake/single writes use this.
    segment_size: std::sync::Mutex<usize>,
}

#[cfg(windows)]
impl NativeDatagramIo for SegmentedUdpIo {
    fn try_receive(&self, buffer: &mut [u8]) -> io::Result<Option<(usize, NativeRoute)>> {
        self.socket.try_receive(buffer)
    }
    fn try_send(&self, bytes: &[u8], route: NativeRoute) -> io::Result<usize> {
        let mut size = self
            .segment_size
            .lock()
            .map_err(|_| io::Error::other("UDP send owner poisoned"))?;
        if *size != 0 {
            novovm_udp_batch::set_udp_send_segment_size(&self.socket, 0)?;
            *size = 0;
        }
        self.socket.try_send(bytes, route)
    }
    fn send_batch_limit(&self) -> usize {
        32
    }
    fn try_send_batch(&self, datagrams: &[&[u8]], route: NativeRoute) -> io::Result<usize> {
        let NativeRoute::Udp(address) = route else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        let Some(first) = datagrams.first() else {
            return Ok(0);
        };
        let mut size = self
            .segment_size
            .lock()
            .map_err(|_| io::Error::other("UDP send owner poisoned"))?;
        if *size != first.len() {
            novovm_udp_batch::set_udp_send_segment_size(&self.socket, first.len())?;
            *size = first.len();
        }
        novovm_udp_batch::send_udp_segmented_batch(&self.socket, address, datagrams, *size)
    }
    fn local_udp_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}

/// Terminal reliable-carrier failure, never reclassified as unrelated UDP noise.
#[derive(Debug, thiserror::Error)]
#[error("native carrier IO failed: {0}")]
pub struct NativeCarrierIoFailure(pub(crate) io::Error);
impl NativeDatagramIo for UdpSocket {
    fn try_receive(&self, buffer: &mut [u8]) -> io::Result<Option<(usize, NativeRoute)>> {
        self.recv_from(buffer).map(|(size, source)| {
            super::diagnostics::traffic(super::diagnostics::Traffic::WireReceived, 0, 3, size);
            Some((size, NativeRoute::Udp(source)))
        })
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
