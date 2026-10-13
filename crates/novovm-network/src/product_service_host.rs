//! One signer and one durable authorization book. Endorsed and explicitly
//! locally pinned trust are separate policies, sharing the same signed offers.
use super::*;

const REQUEST_V2: &[u8; 8] = b"NVLSRQ02";
const MAILBOX_REQUEST_V1: &[u8; 8] = b"NVMBOX01";
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
        self.offer_until(now, u64::MAX)
    }

    /// Bound each online lease to the selected store's unchanged deadline.
    pub fn offer_until(&self, now: u64, store_expires_ms: u64) -> Result<ServiceLeaseOfferV2> {
        let mut book = self
            .book
            .lock()
            .map_err(|_| anyhow::anyhow!("host unavailable"))?;
        book.observe(now)?;
        // Do not serve expired source sets, even when a lease was once valid.
        if let ServiceTrustV1::Endorsed(policy) = &book.policy {
            validated_sources(&book.sources, policy, now)?;
        }
        let pin = self.key.verifying_key().to_bytes();
        let lease = match book.entries.get(&pin) {
            Some(entry) => {
                ensure!(entry.expires > now, "host authority expired");
                let record = directory::sign_relay_record_v1(
                    &self.key,
                    entry.grant.record_id.clone(),
                    entry.grant.endpoints.clone(),
                    now,
                    now.saturating_add(self.lease_ms)
                        .min(entry.expires)
                        .min(store_expires_ms),
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

    /// Local service owner starts a fresh bounded store on the SAME transport.
    /// This is never available to the Publisher-endorsed policy.
    pub fn rotate_self_owned(
        &self,
        endpoint: &IrohServiceEndpointV1,
        now: u64,
        expires: u64,
    ) -> Result<PeerSignedRelayRecordV1> {
        let mut book = self
            .book
            .lock()
            .map_err(|_| anyhow::anyhow!("host unavailable"))?;
        ensure!(book.is_self_owned(), "self-owned policy required");
        let pin = self.key.verifying_key().to_bytes();
        let entry = book.entries.get(&pin).context("service revoked")?;
        ensure!(
            IrohServiceEndpointV1::from_record(&entry.current)?
                .address
                .endpoint_key
                == endpoint.address.endpoint_key,
            "self-owned transport changed"
        );
        let record = directory::sign_relay_record_v1(
            &self.key,
            entry.grant.record_id.clone(),
            vec![endpoint.endpoint()?],
            now,
            expires,
            entry
                .current
                .sequence
                .checked_add(1)
                .context("host sequence exhausted")?,
        )?;
        book.replace_self_owned_records(vec![record.clone()], now)?;
        Ok(record)
    }
}

impl ServiceLeaseBookV1 {
    /// A separately approved mirror can deliver fresh signed sources after all
    /// service routes expire. It cannot introduce source IDs, replace local
    /// priorities, omit an admitted publisher, or mint a service lease.
    pub fn accept_bootstrap_sources(
        &mut self,
        updates: Vec<BootstrapSourceV1>,
        now: u64,
    ) -> Result<()> {
        ensure!(
            updates.len() == self.sources.len(),
            "incomplete bootstrap refresh"
        );
        validated_sources(&updates, self.policy.endorsed()?, now)?;
        let mut sources = self.sources();
        for update in updates {
            let source = sources
                .iter_mut()
                .find(|old| old.manifest.manifest_id == update.manifest.manifest_id)
                .context("unapproved bootstrap source identity")?;
            source.manifest = update.manifest;
        }
        self.replace_sources(sources, now)
    }

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
        self.accept_offer_sources(&offer, pin, endpoint, instance, now)?;
        let record = offer.lease.context("service no longer authorized")?;
        let expiry = record.expires_at_ms;
        self.accept_renewal(record, pin, endpoint, instance, now)?;
        Ok(expiry)
    }

    fn accept_offer_sources(
        &mut self,
        offer: &ServiceLeaseOfferV2,
        pin: [u8; 32],
        endpoint: [u8; 32],
        instance: [u8; 32],
        now: u64,
    ) -> Result<()> {
        ensure!(offer.version == 2, "service offer version");
        encode(offer)?;
        if matches!(self.policy, ServiceTrustV1::SelfOwned(_)) {
            ensure!(
                offer.sources.is_empty(),
                "self-owned offer cannot import endorsements"
            );
            return self.check_bootstrap_route(pin, endpoint, instance, now);
        }
        // A peer only refreshes source IDs the install owner already admitted.
        // Missing sources are not revocations: retain independent publishers.
        validated_sources(&offer.sources, self.policy.endorsed()?, now)?;
        let mut sources = self.sources();
        for update in &offer.sources {
            let source = sources
                .iter_mut()
                .find(|old| old.manifest.manifest_id == update.manifest.manifest_id)
                .context("unapproved online source identity")?;
            // Source kind and local priority are not covered by the manifest
            // signature. Keep these local admission attributes unchanged.
            source.manifest = update.manifest.clone();
        }
        self.replace_sources(sources, now)?;
        self.check_route(pin, endpoint, instance, now)
    }
}

/// Bounded activation wait, NOT a clock correction or an expiry grace period.
/// Sources/revocations are validated and persisted at the caller's real time
/// first. Only a cryptographically checked, pinned online renewal may wait.
/// No discovery/application IO is authorized until the ORIGINAL validator
/// accepts it at real local time; the original carrier/operation guard remains
/// active. The book mutex is never held across an await.
pub async fn accept_service_offer_with_activation_v2(
    book: &Mutex<ServiceLeaseBookV1>,
    offer: ServiceLeaseOfferV2,
    pin: [u8; 32],
    expected: &IrohServiceEndpointV1,
    now: impl Fn() -> Result<u64>,
    check: impl Fn() -> Result<()>,
) -> Result<u64> {
    use std::time::{Duration, Instant};
    const WAIT: Duration = Duration::from_millis(2000);
    let until = Instant::now() + WAIT;
    check()?;
    let record = {
        let mut owner = book
            .lock()
            .map_err(|_| anyhow::anyhow!("lease owner unavailable"))?;
        // Sampling inside the owner lock prevents contention from looking like
        // a clock rollback. Never substitute a peer's timestamp for this sample.
        let current = now()?;
        owner.accept_offer_sources(
            &offer,
            pin,
            expected.address.endpoint_key,
            expected.instance,
            current,
        )?;
        let record = offer.lease.context("service no longer authorized")?;
        if record.issued_at_ms <= current {
            let expiry = record.expires_at_ms;
            owner.accept_renewal(
                record,
                pin,
                expected.address.endpoint_key,
                expected.instance,
                current,
            )?;
            return Ok(expiry);
        }
        // This verification at issued_at only checks the candidate signature,
        // shape and lifetime. It does not publish it, advance the book clock,
        // confer authority, or qualify the candidate for application traffic.
        let address = verify(&record, record.issued_at_ms, MAX_SERVICE_LEASE_MS)?;
        ensure!(
            record.relay_public_key == pin
                && address.address.endpoint_key == expected.address.endpoint_key
                && address.instance == expected.instance,
            "renewal identity differs"
        );
        let entry = owner.entries.get(&pin).context("service revoked")?;
        ensure!(
            record.expires_at_ms <= entry.expires,
            "renewal exceeds authority"
        );
        newer(&entry.current, &record)?;
        ensure!(
            record.issued_at_ms - current <= WAIT.as_millis() as u64,
            "service lease activation wait exceeded"
        );
        record
    };
    loop {
        check()?;
        ensure!(
            Instant::now() < until,
            "service lease activation wait exceeded"
        );
        {
            let mut owner = book
                .lock()
                .map_err(|_| anyhow::anyhow!("lease owner unavailable"))?;
            let current = now()?;
            owner.check_bootstrap_route(
                pin,
                expected.address.endpoint_key,
                expected.instance,
                current,
            )?;
            if current >= record.issued_at_ms {
                let expiry = record.expires_at_ms;
                owner.accept_renewal(
                    record,
                    pin,
                    expected.address.endpoint_key,
                    expected.instance,
                    current,
                )?;
                return Ok(expiry);
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
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
    request_offer(stream, REQUEST_V2).await
}

/// Explicit mailbox selection on the same service endpoint; old servers reject
/// this prelude. Never probe or fall back to the rendezvous payload protocol.
pub async fn request_mailbox_offer_v1(
    stream: &mut CarrierStreamV1<'_>,
) -> Result<ServiceLeaseOfferV2> {
    request_offer(stream, MAILBOX_REQUEST_V1).await
}
async fn request_offer(
    stream: &mut CarrierStreamV1<'_>,
    request: &[u8; 8],
) -> Result<ServiceLeaseOfferV2> {
    stream
        .write_frame(request)
        .await
        .context(ServiceOfferStepV2::Write)?;
    let bytes = stream
        .read_frame()
        .await
        .context(ServiceOfferStepV2::Read)?;
    (|| {
        ensure!(
            bytes.len() > 8 && bytes.len() <= MAX_OFFER_BYTES && bytes.starts_with(RESPONSE_V2),
            "service offer version/size"
        );
        let offer: ServiceLeaseOfferV2 = serde_json::from_slice(&bytes[8..])?;
        ensure!(offer.version == 2, "service offer version");
        Ok(offer)
    })()
    .context(ServiceOfferStepV2::Decode)
}

/// An anyhow context preserves the original sealed carrier error for the
/// existing retry predicate. This phase is diagnostic only, never authority.
#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum ServiceOfferStepV2 {
    #[error("service offer request write")]
    Write,
    #[error("service offer response read")]
    Read,
    #[error("service offer response decode")]
    Decode,
}

pub async fn serve_service_offer_v2(
    stream: &mut CarrierStreamV1<'_>,
    host: &ServiceLeaseHostV1,
    now: impl FnOnce() -> Result<u64>,
) -> Result<()> {
    serve_service_offer_selected_v2(stream, || Ok((host.offer(now()?)?, ()))).await
}

/// Select the store and its offer together AFTER receiving the V2 request.
/// A generation switch cannot pair a new lease with an older store instance.
pub async fn serve_service_offer_selected_v2<T>(
    stream: &mut CarrierStreamV1<'_>,
    select: impl FnOnce() -> Result<(ServiceLeaseOfferV2, T)>,
) -> Result<T> {
    ensure!(
        stream.read_frame().await?.as_slice() == REQUEST_V2,
        "service offer request version"
    );
    let (offer, selected) = select()?;
    stream.write_frame(&encode(&offer)?).await?;
    Ok(selected)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NodeServiceV1 {
    Rendezvous,
    Mailbox,
}

/// The bounded service selector only chooses a protocol, never trust or privacy.
/// Both paths still require the unchanged offer verification and Finished binding.
pub async fn serve_node_offer_selected_v1<T>(
    stream: &mut CarrierStreamV1<'_>,
    select: impl FnOnce() -> Result<(ServiceLeaseOfferV2, T)>,
) -> Result<(NodeServiceV1, T)> {
    let request = stream.read_frame().await?;
    let kind = if request.as_slice() == REQUEST_V2 {
        NodeServiceV1::Rendezvous
    } else if request.as_slice() == MAILBOX_REQUEST_V1 {
        NodeServiceV1::Mailbox
    } else {
        anyhow::bail!("unknown node service version");
    };
    let (offer, selected) = select()?;
    stream.write_frame(&encode(&offer)?).await?;
    Ok((kind, selected))
}

/// Source-only distributor using the existing V2 prelude. Its owner supplies a
/// bounded, audience-specific bundle; it has no signing key and grants no path
/// qualification. Consumers MUST validate/persist with their original book.
pub async fn serve_bootstrap_sources_v2(
    stream: &mut CarrierStreamV1<'_>,
    sources: Vec<BootstrapSourceV1>,
) -> Result<()> {
    ensure!((1..=4).contains(&sources.len()), "bootstrap source budget");
    let bytes = encode(&ServiceLeaseOfferV2 {
        version: 2,
        sources,
        lease: None,
    })?;
    ensure!(
        stream.read_frame().await?.as_slice() == REQUEST_V2,
        "bootstrap request version"
    );
    stream.write_frame(&bytes).await
}
