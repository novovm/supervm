//! Authenticated objects on the original UDP socket and drain queue. This is an
//! ownership adapter, not another listener, scheduler or execution authority.
use super::{NetworkError, NodeId, ProtocolMessage, UdpIngressV1, UdpTransport};
use crate::novorudp::{
    endpoint::{NativeEndpoint, NativeHandshakeBudget, NativePeer},
    fragments::{CompletedMessage, OutgoingMessage},
    object_link::{NativeLaneConfig, NativeLinkPoll, NativeObjectLink, NativeSendPolicy},
};
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use std::{collections::BTreeSet, io, net::SocketAddr, sync::Arc, time::Instant};

pub struct NativeUdpIngressConfigV1 {
    pub identity: SigningKey,
    pub peers: Vec<NativePeer>,
    /// Explicit key-possession slots; zero does not turn an empty peer list open.
    pub client_slots: usize,
    /// Explicit application-only subset of configured lanes. An authenticated
    /// client must never inherit validator lanes merely because they exist.
    pub client_streams: Vec<u64>,
    /// Historical compatibility observer only. Native product ownership sets
    /// this false: plain Protocol must not mutate old pending/sync state.
    pub observe_legacy_protocol: bool,
    pub handshake: NativeHandshakeBudget,
    pub lanes: Vec<NativeLaneConfig>,
    pub send: NativeSendPolicy,
    pub bytes_per_minute: Option<u64>,
}

#[must_use]
pub struct NativeUdpPollV1 {
    pub native: NativeLinkPoll,
    /// Decoded legacy messages retain their original validation/observer path.
    /// Presence here is NOT a new authentication or transaction-admission grant.
    pub protocol: Vec<ProtocolMessage>,
    pub rejected_protocol: usize,
}

pub struct AuthenticatedUdpIngressV1 {
    transport: UdpTransport,
    link: NativeObjectLink,
    accepts_clients: bool,
    observe_legacy_protocol: bool,
}

impl UdpTransport {
    /// Transfer receive authority once. All pre-existing clones lose their
    /// receive API. Dropping the owner does not silently reactivate raw ingress
    /// or discard replay state while allowing another reader to continue.
    pub fn into_authenticated_objects_v1(
        self,
        config: NativeUdpIngressConfigV1,
        wall_ms: u64,
        now: Instant,
    ) -> Result<AuthenticatedUdpIngressV1> {
        let mut claimed = self
            .receive_owner
            .lock()
            .map_err(|_| anyhow::anyhow!("udp receive owner lock poisoned"))?;
        ensure!(!*claimed, "udp receive authority already transferred");
        // Exact native byte accounting must include oversize rejected packets.
        // A truncated UDP read cannot be repaired after the datagram is gone.
        ensure!(
            self.recv_buf
                .lock()
                .map_err(|_| anyhow::anyhow!("UDP receive buffer poisoned"))?
                .len()
                >= 65_536,
            "authenticated UDP ingress requires a full datagram receive buffer"
        );
        let NativeUdpIngressConfigV1 {
            identity,
            peers,
            client_slots,
            client_streams,
            observe_legacy_protocol,
            handshake,
            lanes,
            send,
            bytes_per_minute,
        } = config;
        let lane_streams: BTreeSet<_> = lanes.iter().map(|lane| lane.stream).collect();
        ensure!(lane_streams.len() == lanes.len(), "duplicate native lanes");
        let allowed: BTreeSet<_> = client_streams.iter().copied().collect();
        ensure!(
            allowed.len() == client_streams.len()
                && allowed
                    .iter()
                    .all(|stream| *stream != 0 && lane_streams.contains(stream)),
            "invalid or duplicate native client streams"
        );
        ensure!(
            (client_slots > 0 && !client_streams.is_empty())
                || (client_slots == 0 && client_streams.is_empty()),
            "native client slots require an explicit stream policy"
        );
        let client_budget = NativeHandshakeBudget {
            lifetime: handshake.lifetime,
            retry: handshake.retry,
        };
        let mut endpoint = if peers.is_empty() {
            NativeEndpoint::listen_clients_shared(
                Arc::clone(&self.socket),
                identity,
                client_slots,
                client_streams,
                handshake,
            )?
        } else {
            let mut endpoint = NativeEndpoint::bind_shared(
                Arc::clone(&self.socket),
                identity.clone(),
                peers,
                handshake,
                wall_ms,
                now,
            )?;
            if client_slots > 0 {
                endpoint.allow_clients(identity, client_slots, client_streams, client_budget)?;
            }
            endpoint
        };
        if client_slots > 0 {
            endpoint.enable_client_reconnect_v1()?;
        }
        if let Some(bytes) = bytes_per_minute {
            endpoint.set_wire_budget_v1(bytes, now)?;
        }
        let link = NativeObjectLink::new(endpoint, lanes, send)?;
        *claimed = true;
        drop(claimed);
        Ok(AuthenticatedUdpIngressV1 {
            transport: self,
            link,
            accepts_clients: client_slots > 0,
            observe_legacy_protocol,
        })
    }
}

impl AuthenticatedUdpIngressV1 {
    pub fn accepts_clients(&self) -> bool {
        self.accepts_clients
    }
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.transport.local_addr()?)
    }
    pub fn session_id(&self, peer: usize) -> Option<[u8; 16]> {
        self.link.session_id(peer)
    }
    pub fn peer_id(&self, peer: usize) -> Result<&str> {
        self.link.peer_id(peer)
    }
    pub fn peer_count(&self) -> usize {
        self.link.peer_count()
    }
    pub fn send_protocol(&self, peer: NodeId, message: &ProtocolMessage) -> Result<()> {
        // Compatibility sends retain main's original transaction stamping and
        // signing contract; the adapter does not introduce an unsigned bypass.
        Ok(super::Transport::send(
            &self.transport,
            peer,
            message.clone(),
        )?)
    }
    pub fn try_send(
        &mut self,
        peer: usize,
        stream: u64,
        body: &Arc<OutgoingMessage>,
        now: Instant,
    ) -> Result<bool> {
        self.link.try_send(peer, stream, body, now)
    }
    /// Preserve the existing object's deadline and ownership; this does not
    /// create another sender, extend retries or grant publication authority.
    pub fn try_send_until(
        &mut self,
        peer: usize,
        stream: u64,
        body: &Arc<OutgoingMessage>,
        now: Instant,
        deadline: Instant,
    ) -> Result<bool> {
        self.link.try_send_until(peer, stream, body, now, deadline)
    }
    /// Consume at most `quantum` queued datagrams. Invalid packets consume one
    /// turn, never drain until valid and never terminate healthy peers' progress.
    /// Native handshake/AEAD packets cannot fall back to bincode on any failure.
    pub fn poll(&mut self, wall_ms: u64, now: Instant, quantum: usize) -> Result<NativeUdpPollV1> {
        let transport = &self.transport;
        let claimed = transport
            .receive_owner
            .lock()
            .map_err(|_| anyhow::anyhow!("udp receive owner lock poisoned"))?;
        ensure!(*claimed, "authenticated UDP receive authority missing");
        let mut protocol = Vec::new();
        let mut rejected_protocol = 0;
        let native = self
            .link
            .poll_with_receiver(wall_ms, now, quantum, |buffer| {
                let Some((packet, source)) = transport.receive_packet_v1()? else {
                    return Err(io::ErrorKind::WouldBlock.into());
                };
                // Reserve whole wire families, including invalid versions. The
                // existing endpoint applies its own strict codec/authentication.
                if packet.starts_with(b"NVHSHK") || packet.starts_with(b"NVRLY") {
                    if packet.len() > buffer.len() {
                        return Err(io::Error::other("UDP datagram exceeds receive buffer"));
                    }
                    buffer[..packet.len()].copy_from_slice(&packet);
                    return Ok(Some((packet.len(), source)));
                }
                // Native-only mode rejects before the legacy decoder can learn
                // peers, write pending work, advance observations or send sync.
                if !self.observe_legacy_protocol {
                    rejected_protocol += 1;
                    return Ok(None);
                }
                match transport.decode_ingress_packet_v1(Some((packet, source)), false) {
                    Ok(Some(UdpIngressV1::Protocol(message))) => protocol.push(message),
                    Ok(None) => {}
                    Ok(Some(UdpIngressV1::UntrustedNovoRudpFrame { .. })) => {
                        unreachable!("raw native ingress is disabled")
                    }
                    Err(NetworkError::Decode(_)) => rejected_protocol += 1,
                    Err(error) => return Err(io::Error::other(error.to_string())),
                }
                Ok(None)
            })?;
        Ok(NativeUdpPollV1 {
            native,
            protocol,
            rejected_protocol,
        })
    }
    /// The caller must have a free bounded output slot. Transport completeness
    /// authenticates the peer and object, not wallet signatures or execution.
    pub fn poll_complete(
        &mut self,
        stream: u64,
        now: Instant,
        quantum: usize,
    ) -> Result<Option<CompletedMessage>> {
        self.link
            .poll_complete(stream, now, quantum)
            .context("native object completion")
    }
}

#[cfg(test)]
#[path = "native_ingress/tests.rs"]
mod tests;
