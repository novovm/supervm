//! Typed alternative inside the existing signed node record. Parsing is not
//! signature verification or permission to dial an untrusted address.
use super::*;
use std::net::SocketAddr;

const PREFIX: &str = "tls-service-v1://";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsServiceEndpointV1 {
    pub address: SocketAddr,
    pub endpoint_key: [u8; 32],
    #[serde(
        default = "default_connections",
        skip_serializing_if = "is_default_connections"
    )]
    pub max_connections: u16,
}
fn default_connections() -> u16 {
    4
}
fn is_default_connections(value: &u16) -> bool {
    *value == 4
}

impl TlsServiceEndpointV1 {
    fn validate(&self) -> Result<()> {
        strict_key(&self.endpoint_key)?;
        ensure!(
            (1..=64).contains(&self.max_connections),
            "TLS connection limit bounds"
        );
        ensure!(
            self.address.port() != 0
                && !self.address.ip().is_unspecified()
                && !self.address.ip().is_multicast()
                && !matches!(self.address.ip(), std::net::IpAddr::V4(ip) if ip.is_broadcast()),
            "invalid TLS service address"
        );
        Ok(())
    }

    pub fn endpoint(&self) -> Result<RelayEndpointV1> {
        self.validate()?;
        Ok(RelayEndpointV1 {
            transport: RelayTransportV1::TlsTcpV1,
            uri: format!(
                "{PREFIX}{}",
                URL_SAFE_NO_PAD.encode(serde_json::to_vec(self)?)
            ),
            priority: 0,
            max_sessions: u32::from(self.max_connections),
            max_bytes_per_minute: 65536,
        })
    }

    pub fn from_record(record: &PeerSignedRelayRecordV1) -> Result<Option<Self>> {
        ensure!(record.endpoints.len() <= 2, "service endpoint budget");
        let mut routes = record
            .endpoints
            .iter()
            .filter(|e| e.transport == RelayTransportV1::TlsTcpV1);
        let Some(endpoint) = routes.next() else {
            return Ok(None);
        };
        ensure!(routes.next().is_none(), "duplicate TLS service endpoint");
        ensure!(endpoint.uri.len() <= 1024, "TLS service URI budget");
        let encoded = endpoint
            .uri
            .strip_prefix(PREFIX)
            .context("TLS service URI version")?;
        let value: Self = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded)?)?;
        ensure!(
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value)?) == encoded,
            "noncanonical TLS service address"
        );
        value.validate()?;
        ensure!(
            endpoint.max_sessions == u32::from(value.max_connections),
            "TLS capacity declaration differs"
        );
        Ok(Some(value))
    }
}

impl ServiceLeaseBookV1 {
    /// Live session supervision under the existing durable self-owned policy.
    /// Locator expiry does not terminate a healthy session. Key changes,
    /// removal, revocation and owner retirement still invalidate this route.
    /// TLS Finished and the bound service handshake are separately mandatory.
    pub fn check_continuous_tls_peer_v1(
        &mut self,
        pin: [u8; 32],
        endpoint: &TlsServiceEndpointV1,
        now: u64,
    ) -> Result<()> {
        self.observe(now)?;
        ensure!(self.is_self_owned(), "continuous peer requires local trust");
        let entry = self.entries.get_mut(&pin).context("service revoked")?;
        let current = entry
            .decoded_current()?
            .tls
            .as_ref()
            .context("TLS service removed")?;
        // A live connection is bound to its key, not a mutable advertised IP.
        ensure!(
            current.endpoint_key == endpoint.endpoint_key,
            "self-owned TLS transport changed"
        );
        Ok(())
    }

    /// A new dial requires the exact signed address from a fresh record. This
    /// does not turn an old locator into live TLS proof or approve new nodes.
    pub fn check_tls_route_v1(
        &mut self,
        pin: [u8; 32],
        endpoint: &TlsServiceEndpointV1,
        now: u64,
    ) -> Result<()> {
        self.check_continuous_tls_peer_v1(pin, endpoint, now)?;
        let entry = self.entries.get_mut(&pin).context("service revoked")?;
        ensure!(
            entry.current.expires_at_ms > now,
            "TLS service route expired"
        );
        ensure!(
            entry.decoded_current()?.tls.as_ref() == Some(endpoint),
            "TLS service route changed"
        );
        Ok(())
    }
}
