//! Explicit NVMBOX07 locator in the existing signed directory. UDP alone is
//! not a mailbox capability, reachability proof or privacy qualification.
use super::*;
use crate::novorudp::wire_budget::{MAX_BYTES_PER_MINUTE, MIN_BYTES_PER_MINUTE};
use std::net::SocketAddr;

const SUFFIX: &str = "/novonet-mailbox/v7";

/// Native transport and StoreReceipt signer use the signed record's node key.
/// No second transport identity is inferred from an address or an Iroh route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMailboxEndpointV7 {
    pub address: SocketAddr,
    pub max_connections: u16,
    /// Node-wide admitted UDP payload bytes (receive + successful send), including
    /// native framing, handshakes, ACKs and repairs; IP/link headers are excluded.
    /// Unsolicited traffic still reaches the network interface before admission.
    pub max_bytes_per_minute: u64,
}

impl NativeMailboxEndpointV7 {
    fn validate(&self) -> Result<()> {
        ensure!(
            (1..=64).contains(&self.max_connections),
            "native connection limit bounds"
        );
        ensure!(
            (MIN_BYTES_PER_MINUTE..=MAX_BYTES_PER_MINUTE).contains(&self.max_bytes_per_minute),
            "native byte budget bounds"
        );
        ensure!(
            self.address.port() != 0
                && !self.address.ip().is_unspecified()
                && !self.address.ip().is_multicast()
                && !matches!(self.address.ip(), std::net::IpAddr::V4(ip) if ip.is_broadcast())
                && !matches!(self.address, SocketAddr::V6(a) if a.scope_id() != 0 || a.flowinfo() != 0),
            "invalid native service address"
        );
        Ok(())
    }

    pub fn endpoint(&self) -> Result<RelayEndpointV1> {
        self.validate()?;
        Ok(RelayEndpointV1 {
            transport: RelayTransportV1::Udp,
            uri: format!("udp://{}{SUFFIX}", self.address),
            priority: 0,
            max_sessions: u32::from(self.max_connections),
            max_bytes_per_minute: self.max_bytes_per_minute,
        })
    }

    /// Typed parsing only. Callers must import through the original book for
    /// signature, pin, sequence, freshness and revocation validation.
    pub fn from_record(record: &PeerSignedRelayRecordV1) -> Result<Option<Self>> {
        ensure!(record.endpoints.len() <= 3, "service endpoint budget");
        let mut routes = record
            .endpoints
            .iter()
            .filter(|e| e.transport == RelayTransportV1::Udp);
        let Some(endpoint) = routes.next() else {
            return Ok(None);
        };
        ensure!(routes.next().is_none(), "duplicate native service endpoint");
        ensure!(endpoint.uri.len() <= 128, "native service URI budget");
        let address = endpoint
            .uri
            .strip_prefix("udp://")
            .and_then(|s| s.strip_suffix(SUFFIX))
            .context("native mailbox URI version")?;
        let value = Self {
            address: address.parse()?,
            max_connections: endpoint.max_sessions.try_into()?,
            max_bytes_per_minute: endpoint.max_bytes_per_minute,
        };
        value.validate()?;
        ensure!(
            value.endpoint()?.uri == endpoint.uri,
            "noncanonical native service address"
        );
        ensure!(
            endpoint.max_bytes_per_minute > 0,
            "native service byte budget"
        );
        Ok(Some(value))
    }
}

#[derive(Clone)]
pub(super) struct ServiceEndpoints {
    pub iroh: Option<IrohServiceEndpointV1>,
    pub tls: Option<TlsServiceEndpointV1>,
    pub native: Option<NativeMailboxEndpointV7>,
}
impl ServiceEndpoints {
    pub(super) fn from_record(record: &PeerSignedRelayRecordV1) -> Result<Self> {
        ensure!(
            (1..=3).contains(&record.endpoints.len()),
            "service endpoint budget"
        );
        let mut routes = record
            .endpoints
            .iter()
            .filter(|e| e.transport == RelayTransportV1::IrohRendezvousV1);
        let iroh = routes
            .next()
            .map(IrohServiceEndpointV1::parse_endpoint)
            .transpose()?;
        ensure!(routes.next().is_none(), "duplicate Iroh service endpoint");
        let tls = TlsServiceEndpointV1::from_record(record)?;
        let native = NativeMailboxEndpointV7::from_record(record)?;
        ensure!(
            record.endpoints.len()
                == usize::from(iroh.is_some())
                    + usize::from(tls.is_some())
                    + usize::from(native.is_some()),
            "unsupported service alternative"
        );
        // Preserve the legacy TLS contract; it was an alternative to Iroh.
        // Native-only nodes need no fabricated legacy carrier entry.
        ensure!(
            iroh.is_some() || native.is_some(),
            "service protocol required"
        );
        Ok(Self { iroh, tls, native })
    }
}

pub struct NativeMailboxCandidateV7 {
    pub public_key: [u8; 32],
    pub endpoint: NativeMailboxEndpointV7,
    pub record_expires_at_ms: u64,
}

impl ServiceLeaseBookV1 {
    pub fn native_mailbox_candidates_v7(
        &mut self,
        now: u64,
    ) -> Result<Vec<NativeMailboxCandidateV7>> {
        self.observe(now)?;
        ensure!(self.is_self_owned(), "native mailbox requires local trust");
        let mut candidates = Vec::new();
        for entry in self.entries.values_mut() {
            if entry.current.expires_at_ms <= now {
                continue;
            }
            let pin = entry.current.relay_public_key;
            let expires = entry.current.expires_at_ms;
            if let Some(endpoint) = &entry.decoded_current()?.routes.native {
                candidates.push(NativeMailboxCandidateV7 {
                    public_key: pin,
                    endpoint: endpoint.clone(),
                    record_expires_at_ms: expires,
                });
            }
        }
        Ok(candidates)
    }

    /// Supplement, never replace, NativeObjectLink's live session validity and
    /// authenticated peer-key binding. Ordinary locator renewal does not kill
    /// a healthy session. Removal, local revocation and owner retirement do.
    pub fn check_continuous_native_peer_v7(&mut self, pin: [u8; 32], now: u64) -> Result<()> {
        self.observe(now)?;
        ensure!(self.is_self_owned(), "native mailbox requires local trust");
        let entry = self.entries.get_mut(&pin).context("service revoked")?;
        ensure!(
            entry.decoded_current()?.routes.native.is_some(),
            "native mailbox service removed"
        );
        Ok(())
    }

    pub fn check_native_mailbox_route_v7(
        &mut self,
        pin: [u8; 32],
        endpoint: &NativeMailboxEndpointV7,
        now: u64,
    ) -> Result<()> {
        self.check_continuous_native_peer_v7(pin, now)?;
        let entry = self.entries.get_mut(&pin).context("service revoked")?;
        ensure!(
            entry.current.expires_at_ms > now,
            "native mailbox route expired"
        );
        ensure!(
            entry.decoded_current()?.routes.native.as_ref() == Some(endpoint),
            "native mailbox route changed"
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "product_service_native_tests.rs"]
mod tests;
