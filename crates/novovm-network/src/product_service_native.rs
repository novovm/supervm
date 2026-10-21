//! Explicit NVMBOX07 locator in the existing signed directory. UDP alone is
//! not a mailbox capability, reachability proof or privacy qualification.
use super::*;
use crate::novorudp::wire_budget::{MAX_BYTES_PER_MINUTE, MIN_BYTES_PER_MINUTE};
use std::net::SocketAddr;

pub(super) const SUFFIX: &str = "/novonet-mailbox/v7";
pub(super) const SUFFIX_BULK_V1: &str = "/novonet-mailbox/v8/bulk-v1";
pub(super) const SUFFIX_V8: &str = "/novonet-mailbox/v8";

/// Explicit NVMBOX07 capability on the existing Iroh carrier. The marker is
/// part of the original node-signed URI; an ordinary Iroh address is not proof
/// that the node accepts the native codec. Old service parsers reject this
/// marker, so publication requires coordinated deployment.
#[derive(Clone, Debug)]
pub struct NativeCarrierMailboxEndpointV7 {
    pub iroh: IrohServiceEndpointV1,
}

impl NativeCarrierMailboxEndpointV7 {
    pub fn endpoint(&self) -> Result<RelayEndpointV1> {
        let mut endpoint = self.iroh.endpoint()?;
        endpoint.uri.push_str(SUFFIX);
        ensure!(endpoint.uri.len() <= 4096, "native carrier URI budget");
        Ok(endpoint)
    }

    /// Typed parsing only; signature, trust, replay and revocation checks still
    /// belong to ServiceLeaseBookV1. Never infers NVMBOX07 from legacy Iroh/UDP.
    pub fn from_record(record: &PeerSignedRelayRecordV1) -> Result<Option<Self>> {
        Ok(ServiceEndpoints::from_record(record)?.native_carrier)
    }
}

/// Explicit NVMBOX08 capability on the same physical carrier. Selection uses
/// session-clock V2 authentication; it does not qualify a peer clock as UTC or
/// grant mailbox resource capabilities. V7 never infers this version.
#[derive(Clone, Debug)]
pub struct NativeCarrierMailboxEndpointV8 {
    pub iroh: IrohServiceEndpointV1,
}

impl NativeCarrierMailboxEndpointV8 {
    pub fn endpoint_bulk_v1(&self) -> Result<RelayEndpointV1> {
        let mut endpoint = self.iroh.endpoint()?;
        endpoint.uri.push_str(SUFFIX_BULK_V1);
        ensure!(endpoint.uri.len() <= 4096, "native bulk URI budget");
        Ok(endpoint)
    }
    /// Call only after original signed-record validation; old V8 is not bulk.
    pub fn from_record_supports_bulk_v1(record: &PeerSignedRelayRecordV1) -> Result<bool> {
        Self::supports_bulk_v1(record)
    }
    pub fn supports_bulk_v1(record: &PeerSignedRelayRecordV1) -> Result<bool> {
        Ok(Self::from_record(record)?.is_some()
            && record.endpoints.iter().any(|e| {
                e.transport == RelayTransportV1::IrohRendezvousV1 && e.uri.ends_with(SUFFIX_BULK_V1)
            }))
    }

    pub fn endpoint(&self) -> Result<RelayEndpointV1> {
        let mut endpoint = self.iroh.endpoint()?;
        endpoint.uri.push_str(SUFFIX_V8);
        ensure!(endpoint.uri.len() <= 4096, "native carrier URI budget");
        Ok(endpoint)
    }

    pub fn from_record(record: &PeerSignedRelayRecordV1) -> Result<Option<Self>> {
        Ok(ServiceEndpoints::from_record(record)?.native_carrier_v8)
    }
}

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
        ensure!(self.max_connections > 0, "native connection limit bounds");
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
    pub native_carrier: Option<NativeCarrierMailboxEndpointV7>,
    pub native_carrier_v8: Option<NativeCarrierMailboxEndpointV8>,
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
        let carrier = routes.next();
        let iroh = carrier
            .map(IrohServiceEndpointV1::parse_endpoint)
            .transpose()?;
        ensure!(routes.next().is_none(), "duplicate Iroh service endpoint");
        // One physical Iroh route carries an explicit codec capability; it is
        // not a fourth endpoint or an alias for the separate native UDP route.
        let native_carrier = match (&iroh, carrier) {
            (Some(iroh), Some(endpoint)) if endpoint.uri.ends_with(SUFFIX) => {
                Some(NativeCarrierMailboxEndpointV7 { iroh: iroh.clone() })
            }
            _ => None,
        };
        let native_carrier_v8 = match (&iroh, carrier) {
            (Some(iroh), Some(endpoint))
                if endpoint.uri.ends_with(SUFFIX_V8) || endpoint.uri.ends_with(SUFFIX_BULK_V1) =>
            {
                Some(NativeCarrierMailboxEndpointV8 { iroh: iroh.clone() })
            }
            _ => None,
        };
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
        Ok(Self {
            iroh,
            tls,
            native,
            native_carrier,
            native_carrier_v8,
        })
    }
}

pub struct NativeMailboxCandidateV7 {
    pub public_key: [u8; 32],
    pub endpoint: NativeMailboxEndpointV7,
    pub record_expires_at_ms: u64,
}

pub struct NativeCarrierMailboxCandidateV7 {
    pub public_key: [u8; 32],
    pub endpoint: NativeCarrierMailboxEndpointV7,
    pub record_expires_at_ms: u64,
}

pub struct NativeCarrierMailboxCandidateV8 {
    pub public_key: [u8; 32],
    pub endpoint: NativeCarrierMailboxEndpointV8,
    pub record_expires_at_ms: u64,
}

/// Historical signed address usable ONLY to obtain a fresh record from the
/// locally pinned node. It carries no permission for mailbox business requests.
#[derive(Clone)]
pub struct NativeMailboxBootstrapV7 {
    pub public_key: [u8; 32],
    pub endpoint: NativeMailboxEndpointV7,
}

impl ServiceLeaseBookV1 {
    /// Fresh signed candidates only. The actual carrier Finished and bound
    /// native handshake are required separately before any mailbox request.
    pub fn native_carrier_mailbox_candidates_v7(
        &mut self,
        now: u64,
    ) -> Result<Vec<NativeCarrierMailboxCandidateV7>> {
        self.observe(now)?;
        ensure!(self.is_self_owned(), "native carrier requires local trust");
        let mut candidates = Vec::new();
        for entry in self.entries.values_mut() {
            if entry.current.expires_at_ms <= now {
                continue;
            }
            let public_key = entry.current.relay_public_key;
            let record_expires_at_ms = entry.current.expires_at_ms;
            if let Some(endpoint) = &entry.decoded_current()?.routes.native_carrier {
                candidates.push(NativeCarrierMailboxCandidateV7 {
                    public_key,
                    endpoint: endpoint.clone(),
                    record_expires_at_ms,
                });
            }
        }
        Ok(candidates)
    }

    /// Supervise an already authenticated NVMBOX07 session on this exact
    /// transport identity. Ordinary locator expiry/instance rotation does not
    /// end it; capability removal, key change, revocation and retirement do.
    /// This does not admit a new dial or replace live native session checks.
    pub fn check_continuous_native_carrier_v7(
        &mut self,
        pin: [u8; 32],
        transport: [u8; 32],
        now: u64,
    ) -> Result<()> {
        self.observe(now)?;
        ensure!(self.is_self_owned(), "native carrier requires local trust");
        let entry = self.entries.get_mut(&pin).context("service revoked")?;
        let current = entry
            .decoded_current()?
            .routes
            .native_carrier
            .as_ref()
            .context("native carrier mailbox service removed")?;
        ensure!(
            current.iroh.address.endpoint_key == transport,
            "native carrier transport changed"
        );
        Ok(())
    }

    /// New business connections require the exact fresh, signed locator. An
    /// expired locator or old instance is not authority for a new NVMBOX07 dial.
    pub fn check_native_carrier_route_v7(
        &mut self,
        pin: [u8; 32],
        endpoint: &NativeCarrierMailboxEndpointV7,
        now: u64,
    ) -> Result<()> {
        self.check_continuous_native_carrier_v7(pin, endpoint.iroh.address.endpoint_key, now)?;
        let entry = self.entries.get_mut(&pin).context("service revoked")?;
        ensure!(
            entry.current.expires_at_ms > now,
            "native carrier route expired"
        );
        let current = entry
            .decoded_current()?
            .routes
            .native_carrier
            .as_ref()
            .context("native carrier mailbox service removed")?;
        ensure!(
            current.endpoint()?.uri == endpoint.endpoint()?.uri,
            "native carrier route changed"
        );
        Ok(())
    }

    /// Fresh signed candidates only. The actual carrier Finished and bound
    /// native handshake are required separately before any mailbox request.
    pub fn native_carrier_mailbox_candidates_v8(
        &mut self,
        now: u64,
    ) -> Result<Vec<NativeCarrierMailboxCandidateV8>> {
        self.observe(now)?;
        ensure!(self.is_self_owned(), "native carrier requires local trust");
        let mut candidates = Vec::new();
        for entry in self.entries.values_mut() {
            if entry.current.expires_at_ms <= now {
                continue;
            }
            let public_key = entry.current.relay_public_key;
            let record_expires_at_ms = entry.current.expires_at_ms;
            if let Some(endpoint) = &entry.decoded_current()?.routes.native_carrier_v8 {
                candidates.push(NativeCarrierMailboxCandidateV8 {
                    public_key,
                    endpoint: endpoint.clone(),
                    record_expires_at_ms,
                });
            }
        }
        Ok(candidates)
    }

    /// Supervise an already authenticated NVMBOX08 session on this exact
    /// transport identity. Ordinary locator expiry/instance rotation does not
    /// end it; capability removal, key change, revocation and retirement do.
    /// This does not admit a new dial or replace live native session checks.
    pub fn check_continuous_native_carrier_v8(
        &mut self,
        pin: [u8; 32],
        transport: [u8; 32],
        now: u64,
    ) -> Result<()> {
        self.observe(now)?;
        ensure!(self.is_self_owned(), "native carrier requires local trust");
        let entry = self.entries.get_mut(&pin).context("service revoked")?;
        let current = entry
            .decoded_current()?
            .routes
            .native_carrier_v8
            .as_ref()
            .context("native carrier mailbox service removed")?;
        ensure!(
            current.iroh.address.endpoint_key == transport,
            "native carrier transport changed"
        );
        Ok(())
    }

    /// New business connections require the exact fresh, signed locator. An
    /// expired locator or old instance is not authority for a new NVMBOX08 dial.
    pub fn check_native_carrier_route_v8(
        &mut self,
        pin: [u8; 32],
        endpoint: &NativeCarrierMailboxEndpointV8,
        now: u64,
    ) -> Result<()> {
        self.check_continuous_native_carrier_v8(pin, endpoint.iroh.address.endpoint_key, now)?;
        let entry = self.entries.get_mut(&pin).context("service revoked")?;
        ensure!(
            entry.current.expires_at_ms > now,
            "native carrier route expired"
        );
        let current = entry
            .decoded_current()?
            .routes
            .native_carrier_v8
            .as_ref()
            .context("native carrier mailbox service removed")?;
        ensure!(
            current.endpoint()?.uri == endpoint.endpoint()?.uri,
            "native carrier route changed"
        );
        Ok(())
    }

    pub fn native_mailbox_bootstrap_v7(
        &mut self,
        now: u64,
    ) -> Result<Vec<NativeMailboxBootstrapV7>> {
        self.observe(now)?;
        ensure!(
            self.is_self_owned(),
            "native bootstrap requires local trust"
        );
        let mut out = Vec::new();
        for entry in self.entries.values_mut() {
            let public_key = entry.current.relay_public_key;
            if let Some(endpoint) = &entry.decoded_current()?.routes.native {
                out.push(NativeMailboxBootstrapV7 {
                    public_key,
                    endpoint: endpoint.clone(),
                });
            }
        }
        Ok(out)
    }

    pub fn check_native_bootstrap_route_v7(
        &mut self,
        pin: [u8; 32],
        endpoint: &NativeMailboxEndpointV7,
        now: u64,
    ) -> Result<()> {
        self.check_continuous_native_peer_v7(pin, now)?;
        ensure!(
            self.entries
                .get_mut(&pin)
                .context("service revoked")?
                .decoded_current()?
                .routes
                .native
                .as_ref()
                == Some(endpoint),
            "native bootstrap route changed"
        );
        Ok(())
    }

    /// Inspect authentic metadata without installing it or granting a route.
    /// Replay, equivocation, revocation and version failures are checked BEFORE
    /// exposing a temporal state; callers cannot turn them into a delayed retry.
    pub fn inspect_native_record_v7(
        &mut self,
        expected_pin: [u8; 32],
        record: &PeerSignedRelayRecordV1,
        now: u64,
    ) -> Result<directory::RelayRecordTimeV1> {
        self.inspect_native_record(expected_pin, record, now, false)
    }
    pub fn inspect_native_record_v8(
        &mut self,
        expected_pin: [u8; 32],
        record: &PeerSignedRelayRecordV1,
        now: u64,
    ) -> Result<directory::RelayRecordTimeV1> {
        self.inspect_native_record(expected_pin, record, now, true)
    }
    fn inspect_native_record(
        &mut self,
        expected_pin: [u8; 32],
        record: &PeerSignedRelayRecordV1,
        now: u64,
        v8: bool,
    ) -> Result<directory::RelayRecordTimeV1> {
        self.observe(now)?;
        ensure!(self.is_self_owned(), "native refresh requires local trust");
        ensure!(
            record.relay_public_key == expected_pin,
            "native refresh wrong node"
        );
        let decoded = verify_service_record_authenticity(record, MAX_BOOTSTRAP_LEASE_MS)?;
        ensure!(
            if v8 {
                decoded.native_carrier_v8.is_some()
            } else {
                decoded.native.is_some() || decoded.native_carrier.is_some()
            },
            "native service missing"
        );
        let entry = self
            .entries
            .get(&expected_pin)
            .context("untrusted or revoked self-owned node")?;
        newer(&entry.current, record)?;
        Ok(directory::relay_record_time_v1(record, now))
    }

    /// Commit only a currently valid update to the original pin. Metadata
    /// inspection/waiting never changes stored routes, grants or message TTLs.
    pub fn accept_native_record_v7(
        &mut self,
        expected_pin: [u8; 32],
        record: PeerSignedRelayRecordV1,
        now: u64,
    ) -> Result<()> {
        self.accept_native_record(expected_pin, record, now, false)
    }
    pub fn accept_native_record_v8(
        &mut self,
        expected_pin: [u8; 32],
        record: PeerSignedRelayRecordV1,
        now: u64,
    ) -> Result<()> {
        self.accept_native_record(expected_pin, record, now, true)
    }
    fn accept_native_record(
        &mut self,
        expected_pin: [u8; 32],
        record: PeerSignedRelayRecordV1,
        now: u64,
        v8: bool,
    ) -> Result<()> {
        match self.inspect_native_record(expected_pin, &record, now, v8)? {
            directory::RelayRecordTimeV1::NotYetValid => {
                return Err(directory::ProductDirectoryErrorV1::NotYetValid.into())
            }
            directory::RelayRecordTimeV1::Expired => {
                return Err(directory::ProductDirectoryErrorV1::Expired.into())
            }
            directory::RelayRecordTimeV1::Valid => {}
        }
        let mut entries = self.entries.clone();
        let entry = entries
            .get_mut(&expected_pin)
            .context("untrusted or revoked self-owned node")?;
        if entry.current == record {
            return Ok(());
        }
        entry.grant = record.clone();
        entry.current = record;
        self.publish(Vec::new(), entries, self.tombstones.clone())
    }

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
