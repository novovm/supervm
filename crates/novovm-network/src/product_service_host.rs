//! One signer and one durable authorization book. Source signing belongs to the
//! independently trusted publisher; this host cannot extend its own authority.
use super::*;

const REQUEST_V2: &[u8; 8] = b"NVLSRQ02";
const RESPONSE_V2: &[u8; 8] = b"NVLSRS02";
const MAX_OFFER_BYTES: usize = 16 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceLeaseOfferV2 {
    pub version: u8,
    pub sources: Vec<BootstrapSourceV1>,
    pub lease: Option<PeerSignedRelayRecordV1>,
}

pub struct ServiceLeaseHostV1 {
    key: SigningKey,
    book: Mutex<ServiceLeaseBookV1>,
    lease_ms: u64,
}

impl ServiceLeaseHostV1 {
    /// Fast local supervision; does not renew or perform file/network IO.
    pub fn check_route(&self, endpoint: [u8; 32], instance: [u8; 32], now: u64) -> Result<()> {
        self.book
            .lock()
            .map_err(|_| anyhow::anyhow!("host unavailable"))?
            .check_route(self.key.verifying_key().to_bytes(), endpoint, instance, now)
    }
    /// Only an already durable book is accepted. The caller must explicitly
    /// enroll or restore it with the install owner's unchanged trust policy.
    pub fn new(key: SigningKey, book: ServiceLeaseBookV1) -> Result<Self> {
        ensure!(
            book.persistence.is_some() && !book.failed,
            "durable host required"
        );
        // A revoked host may reopen solely to publish the signed revocation.
        let pin = key.verifying_key().to_bytes();
        ensure!(
            book.entries.contains_key(&pin) || book.tombstones.contains(&pin),
            "host not enrolled"
        );
        Ok(Self {
            key,
            book: Mutex::new(book),
            lease_ms: MAX_SERVICE_LEASE_MS,
        })
    }

    /// A local operator can shorten online leases, never exceed protocol limits
    /// or extend the independently signed bootstrap authorization.
    pub fn with_lease_limit(mut self, lease_ms: u64) -> Result<Self> {
        ensure!(
            (1..=MAX_SERVICE_LEASE_MS).contains(&lease_ms),
            "host lease limit"
        );
        self.lease_ms = lease_ms;
        Ok(self)
    }

    /// This receives externally signed sources. It has no publisher private
    /// key and cannot manufacture an authorized source, rotate roots or revive
    /// a revoked node. The same durable CAS covers imported grants and leases.
    pub fn replace_sources(&self, sources: Vec<BootstrapSourceV1>, now: u64) -> Result<()> {
        self.book
            .lock()
            .map_err(|_| anyhow::anyhow!("host unavailable"))?
            .replace_sources(sources, now)
    }

    pub fn offer(&self, now: u64) -> Result<ServiceLeaseOfferV2> {
        let mut book = self
            .book
            .lock()
            .map_err(|_| anyhow::anyhow!("host unavailable"))?;
        book.observe(now)?;
        // Do not serve expired source sets, even when a lease was once valid.
        validated_sources(&book.sources, &book.policy, now)?;
        let pin = self.key.verifying_key().to_bytes();
        let lease = match book.entries.get(&pin) {
            Some(entry) => {
                ensure!(entry.expires > now, "host authority expired");
                let record = directory::sign_relay_record_v1(
                    &self.key,
                    entry.grant.record_id.clone(),
                    entry.grant.endpoints.clone(),
                    now,
                    now.saturating_add(self.lease_ms).min(entry.expires),
                    entry
                        .current
                        .sequence
                        .checked_add(1)
                        .context("host sequence exhausted")?,
                )?;
                Some(record)
            }
            None => None,
        };
        let offer = ServiceLeaseOfferV2 {
            version: 2,
            sources: book.sources(),
            lease,
        };
        // Check response bounds BEFORE consuming a sequence or writing disk.
        encode(&offer)?;
        if let Some(record) = &offer.lease {
            let endpoint = IrohServiceEndpointV1::from_record(record)?;
            book.accept_renewal(
                record.clone(),
                pin,
                endpoint.address.endpoint_key,
                endpoint.instance,
                now,
            )?;
        }
        Ok(offer)
    }
}

impl ServiceLeaseBookV1 {
    /// Persist trustworthy revocations even if the accompanying lease is absent
    /// or invalid. A bad lease never authorizes discovery or application IO.
    /// The caller pins the carrier and checks its own path policy before dialing.
    pub fn accept_offer(
        &mut self,
        offer: ServiceLeaseOfferV2,
        pin: [u8; 32],
        endpoint: [u8; 32],
        instance: [u8; 32],
        now: u64,
    ) -> Result<u64> {
        ensure!(offer.version == 2, "service offer version");
        encode(&offer)?;
        // A peer only refreshes source IDs the install owner already admitted.
        // Missing sources are not revocations: retain independent publishers.
        validated_sources(&offer.sources, &self.policy, now)?;
        let mut sources = self.sources();
        for update in offer.sources {
            let source = sources
                .iter_mut()
                .find(|old| old.manifest.manifest_id == update.manifest.manifest_id)
                .context("unapproved online source identity")?;
            // Source kind and local priority are not covered by the manifest
            // signature. Keep these local admission attributes unchanged.
            source.manifest = update.manifest;
        }
        self.replace_sources(sources, now)?;
        self.check_route(pin, endpoint, instance, now)?;
        let record = offer.lease.context("service no longer authorized")?;
        let expiry = record.expires_at_ms;
        self.accept_renewal(record, pin, endpoint, instance, now)?;
        Ok(expiry)
    }
}

fn encode(offer: &ServiceLeaseOfferV2) -> Result<Vec<u8>> {
    let mut bytes = RESPONSE_V2.to_vec();
    bytes.extend_from_slice(&serde_json::to_vec(offer)?);
    ensure!(bytes.len() <= MAX_OFFER_BYTES, "service offer budget");
    Ok(bytes)
}

/// Explicit V2, with no V1 probing/fallback. This precedes the existing
/// Finished-bound service handshake; it does not replace that authentication.
pub async fn request_service_offer_v2(
    stream: &mut CarrierStreamV1<'_>,
) -> Result<ServiceLeaseOfferV2> {
    stream.write_frame(REQUEST_V2).await?;
    let bytes = stream.read_frame().await?;
    ensure!(
        bytes.len() > 8 && bytes.len() <= MAX_OFFER_BYTES && bytes.starts_with(RESPONSE_V2),
        "service offer version/size"
    );
    let offer: ServiceLeaseOfferV2 = serde_json::from_slice(&bytes[8..])?;
    ensure!(offer.version == 2, "service offer version");
    Ok(offer)
}

pub async fn serve_service_offer_v2(
    stream: &mut CarrierStreamV1<'_>,
    host: &ServiceLeaseHostV1,
    now: impl FnOnce() -> Result<u64>,
) -> Result<()> {
    ensure!(
        stream.read_frame().await?.as_slice() == REQUEST_V2,
        "service offer request version"
    );
    let offer = host.offer(now()?)?;
    stream.write_frame(&encode(&offer)?).await
}
