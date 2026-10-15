//! Admission and renewal for NOVOVM's opaque rendezvous service. Reuses the
//! product directory's signed manifests/records; no chat or account identifier.
//! Trust anchors are supplied separately. Optional durable checkpoints retain
//! authorization history; encryption and external freshness remain platform-owned.
use crate::{
    carrier_stream::CarrierStreamV1,
    iroh_transport::IrohAddressV1,
    product_directory::{
        self as directory, BootstrapSourceV1, BootstrapTrustPolicyV1, PeerSignedRelayRecordV1,
        RelayEndpointV1, RelayTransportV1,
    },
};
use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

const PREFIX: &str = "iroh-rendezvous-v1://";
const REQUEST: &[u8; 8] = b"NVLSRQ01";
const RESPONSE: &[u8; 8] = b"NVLSRS01";
pub const MAX_SERVICE_LEASE_MS: u64 = 120_000;
pub const MAX_BOOTSTRAP_LEASE_MS: u64 = 600_000;
pub const MAX_SERVICE_LEASE_BYTES: usize = 8192;

#[path = "product_service_tls.rs"]
mod tls_endpoint;
pub use tls_endpoint::TlsServiceEndpointV1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrohServiceEndpointV1 {
    pub instance: [u8; 32],
    pub address: IrohAddressV1,
}
impl IrohServiceEndpointV1 {
    fn validate(&self) -> Result<()> {
        strict_key(&self.address.endpoint_key)?;
        ensure!(self.instance != [0; 32], "empty service instance");
        ensure!(
            self.address.direct.len() <= 8 && self.address.relays.len() <= 4,
            "service address budget"
        );
        ensure!(
            !self.address.direct.is_empty() || !self.address.relays.is_empty(),
            "empty service route"
        );
        ensure!(
            self.address
                .direct
                .iter()
                .all(|a| a.port() != 0 && !a.ip().is_unspecified() && !a.ip().is_multicast()),
            "invalid service address"
        );
        // Parse using the same carrier configuration, without binding a socket.
        crate::iroh_transport::IrohConfigV1::new(&self.address.relays, "127.0.0.1".parse()?)?;
        Ok(())
    }
    pub fn endpoint(&self) -> Result<RelayEndpointV1> {
        self.validate()?;
        Ok(RelayEndpointV1 {
            transport: RelayTransportV1::IrohRendezvousV1,
            uri: format!(
                "{PREFIX}{}",
                URL_SAFE_NO_PAD.encode(serde_json::to_vec(self)?)
            ),
            priority: 0,
            max_sessions: 4,
            max_bytes_per_minute: 65536,
        })
    }
    pub fn from_record(record: &PeerSignedRelayRecordV1) -> Result<Self> {
        ensure!(
            (1..=2).contains(&record.endpoints.len()),
            "service endpoint budget"
        );
        let mut routes = record
            .endpoints
            .iter()
            .filter(|e| e.transport == RelayTransportV1::IrohRendezvousV1);
        let endpoint = routes.next().context("Iroh service endpoint required")?;
        ensure!(routes.next().is_none(), "duplicate Iroh service endpoint");
        // Explicit extension: one optional raw-key TLS alternative. Unknown,
        // duplicate or malformed alternatives reject the whole signed record.
        let tls = TlsServiceEndpointV1::from_record(record)?;
        ensure!(
            record.endpoints.len() == 1 + usize::from(tls.is_some()),
            "unsupported service alternative"
        );
        ensure!(
            endpoint.transport == RelayTransportV1::IrohRendezvousV1 && endpoint.uri.len() <= 4096,
            "different service transport"
        );
        let encoded = endpoint
            .uri
            .strip_prefix(PREFIX)
            .context("service URI version")?;
        let bytes = URL_SAFE_NO_PAD.decode(encoded)?;
        let value: Self = serde_json::from_slice(&bytes)?;
        ensure!(
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value)?) == encoded,
            "noncanonical service address"
        );
        value.validate()?;
        Ok(value)
    }
}
fn strict_key(bytes: &[u8; 32]) -> Result<()> {
    let key = VerifyingKey::from_bytes(bytes)?;
    ensure!(
        !key.is_weak()
            && key.to_edwards().is_torsion_free()
            && key.to_edwards().compress().to_bytes() == *bytes,
        "invalid service key"
    );
    Ok(())
}
fn verify(
    record: &PeerSignedRelayRecordV1,
    now: u64,
    max_ttl: u64,
) -> Result<IrohServiceEndpointV1> {
    ensure!(
        serde_json::to_vec(record)?.len() <= MAX_SERVICE_LEASE_BYTES
            && !record.record_id.is_empty()
            && record.record_id.len() <= 128
            && record.sequence > 0,
        "service record bounds"
    );
    ensure!(
        record
            .expires_at_ms
            .checked_sub(record.issued_at_ms)
            .is_some_and(|ttl| ttl > 0 && ttl <= max_ttl),
        "service record lifetime"
    );
    strict_key(&record.relay_public_key)?;
    directory::validate_relay_record_v1(record, now)?;
    // The existing signing bytes are shared; enforce canonical, strong-key
    // verification here without changing historical directory consumers.
    VerifyingKey::from_bytes(&record.relay_public_key)?.verify_strict(
        &directory::relay_record_signing_bytes_v1(record),
        &Signature::from_slice(&record.signature)?,
    )?;
    IrohServiceEndpointV1::from_record(record)
}
fn newer(previous: &PeerSignedRelayRecordV1, next: &PeerSignedRelayRecordV1) -> Result<()> {
    ensure!(
        next.sequence >= previous.sequence && next.issued_at_ms >= previous.issued_at_ms,
        "service lease rollback"
    );
    ensure!(
        next.sequence != previous.sequence || next == previous,
        "service lease equivocation"
    );
    Ok(())
}
#[derive(Clone)]
struct Entry {
    grant: PeerSignedRelayRecordV1,
    current: PeerSignedRelayRecordV1,
    expires: u64,
    decoded: Option<DecodedCurrent>,
}
#[derive(Clone)]
struct DecodedCurrent {
    record: PeerSignedRelayRecordV1,
    iroh: IrohServiceEndpointV1,
    tls: Option<TlsServiceEndpointV1>,
}
impl Entry {
    // Entries only come from verified imports/restores. Compare the WHOLE
    // record, not just its sequence: no mutation/rotation path can leave a
    // stale key or address in use. Derived data is never checkpointed and
    // cannot replace this owner's revocation, clock or persistence checks.
    fn decoded_current(&mut self) -> Result<&DecodedCurrent> {
        if self
            .decoded
            .as_ref()
            .is_none_or(|d| d.record != self.current)
        {
            let iroh = IrohServiceEndpointV1::from_record(&self.current)?;
            let tls = TlsServiceEndpointV1::from_record(&self.current)?;
            self.decoded = Some(DecodedCurrent {
                record: self.current.clone(),
                iroh,
                tls,
            });
        }
        Ok(self.decoded.as_ref().expect("decoded current record"))
    }
}
/// The caller must hold this under its existing Application/native owner lock.
/// No network IO, wallet lookup, automatic trust-on-first-use or privacy claim.
pub struct ServiceLeaseBookV1 {
    policy: ServiceTrustV1,
    sources: Vec<BootstrapSourceV1>,
    entries: BTreeMap<[u8; 32], Entry>,
    tombstones: BTreeSet<[u8; 32]>,
    clock: u64,
    persistence: Option<Arc<dyn ServiceLeasePersistenceV1>>,
    persisted: Vec<u8>,
    failed: bool,
    prior_endorsement: Option<String>,
}

#[derive(Clone)]
enum ServiceTrustV1 {
    Endorsed(BootstrapTrustPolicyV1),
    SelfOwned(SelfOwnedTrustV1),
}
impl ServiceTrustV1 {
    fn endorsed(&self) -> Result<&BootstrapTrustPolicyV1> {
        match self {
            Self::Endorsed(policy) => Ok(policy),
            Self::SelfOwned(_) => anyhow::bail!("self-owned trust is not publisher authority"),
        }
    }
}
impl ServiceLeaseBookV1 {
    pub fn new(
        policy: BootstrapTrustPolicyV1,
        sources: Vec<BootstrapSourceV1>,
        now: u64,
    ) -> Result<Self> {
        ensure!(
            !policy.allowed_signer_peer_ids.is_empty()
                && policy.allowed_signer_peer_ids.len() <= 8
                && policy.minimum_valid_signatures > 0
                && policy.minimum_valid_signatures <= policy.allowed_signer_peer_ids.len(),
            "bootstrap trust bounds"
        );
        let entries = validated_sources(&sources, &policy, now)?;
        Ok(Self {
            policy: ServiceTrustV1::Endorsed(policy),
            sources,
            entries,
            tombstones: BTreeSet::new(),
            clock: now,
            persistence: None,
            persisted: Vec::new(),
            failed: false,
            prior_endorsement: None,
        })
    }
    pub fn sources(&self) -> Vec<BootstrapSourceV1> {
        self.sources.clone()
    }
    fn observe(&mut self, now: u64) -> Result<()> {
        ensure!(!self.failed, "service lease persistence unavailable");
        ensure!(now >= self.clock, "service lease clock rollback");
        self.clock = now;
        Ok(())
    }
    /// A newer authenticated source set can revoke nodes, but cannot resurrect
    /// revoked identities or replace anchors in this process. Failed imports
    /// leave the previous state intact. No unbounded tombstone eviction.
    pub fn replace_sources(&mut self, sources: Vec<BootstrapSourceV1>, now: u64) -> Result<()> {
        self.observe(now)?;
        let mut entries = validated_sources(&sources, self.policy.endorsed()?, now)?;
        for source in &sources {
            if let Some(old) = self
                .sources
                .iter()
                .find(|old| old.manifest.manifest_id == source.manifest.manifest_id)
            {
                ensure!(
                    source.manifest.issued_at_ms >= old.manifest.issued_at_ms,
                    "bootstrap rollback"
                );
                ensure!(
                    source.manifest.issued_at_ms != old.manifest.issued_at_ms
                        || source.manifest == old.manifest,
                    "bootstrap equivocation"
                );
            } else {
                anyhow::bail!("source identity cannot change during renewal");
            }
        }
        let mut tombstones = self.tombstones.clone();
        for (pin, previous) in &self.entries {
            if let Some(next) = entries.get_mut(pin) {
                newer(&previous.grant, &next.grant)?;
                if next.grant == previous.grant {
                    next.current = previous.current.clone();
                } else {
                    if next.grant.issued_at_ms < previous.current.issued_at_ms {
                        // A separately publisher-authorized epoch can be prepared
                        // while the old epoch still issues online leases. Its
                        // reserved sequence must exceed ALL consumed old leases;
                        // only a fresh instance AND carrier may use this ordering.
                        // Grant/source time checks above and the local clock are
                        // unchanged. Online renewal still uses strict `newer`.
                        let old = IrohServiceEndpointV1::from_record(&previous.current)?;
                        let fresh = IrohServiceEndpointV1::from_record(&next.grant)?;
                        ensure!(
                            next.grant.sequence > previous.current.sequence
                                && fresh.instance != old.instance
                                && fresh.address.endpoint_key != old.address.endpoint_key,
                            "service lease rollback"
                        );
                    } else {
                        newer(&previous.current, &next.grant)?;
                    }
                }
            } else {
                tombstones.insert(*pin);
            }
        }
        ensure!(
            tombstones.len() <= 32 && entries.keys().all(|pin| !tombstones.contains(pin)),
            "revoked service identity"
        );
        self.publish(sources, entries, tombstones)
    }
    /// Locators for a NEW connection and lease handshake only. Unlike routes(),
    /// these are the original publisher-authorized signed records, not a
    /// previous connection's shorter online lease. No record is re-signed or
    /// given a later expiry. Callers must check_route throughout dialing and
    /// accept a fresh service offer before service RPC; this grants no right
    /// to continue an old stream beyond its own online lease.
    pub fn bootstrap_routes(&mut self, now: u64) -> Result<Vec<PeerSignedRelayRecordV1>> {
        self.observe(now)?;
        Ok(self
            .entries
            .values()
            .filter(|entry| entry.expires > now && entry.grant.expires_at_ms > now)
            .map(|entry| entry.grant.clone())
            .collect())
    }

    pub fn routes(&mut self, now: u64) -> Result<Vec<PeerSignedRelayRecordV1>> {
        self.observe(now)?;
        Ok(self
            .entries
            .values()
            .filter(|entry| entry.expires > now)
            .filter_map(|entry| {
                if entry.current.expires_at_ms > now {
                    Some(entry.current.clone())
                } else if entry.grant.expires_at_ms > now {
                    Some(entry.grant.clone())
                } else {
                    None
                }
            })
            .collect())
    }
    pub fn check_route(
        &mut self,
        pin: [u8; 32],
        endpoint: [u8; 32],
        instance: [u8; 32],
        now: u64,
    ) -> Result<()> {
        self.observe(now)?;
        let entry = self.entries.get(&pin).context("service revoked")?;
        ensure!(entry.expires > now, "bootstrap authorization expired");
        let self_owned = matches!(self.policy, ServiceTrustV1::SelfOwned(_));
        let route = if self_owned || entry.current.expires_at_ms > now {
            &entry.current
        } else {
            &entry.grant
        };
        // A locally pinned self-owned locator only authorizes the fresh offer
        // handshake. Callers must separately enforce the returned online lease.
        ensure!(
            self_owned || route.expires_at_ms > now,
            "service route expired"
        );
        let address = IrohServiceEndpointV1::from_record(route)?;
        ensure!(
            address.address.endpoint_key == endpoint && address.instance == instance,
            "service route changed"
        );
        Ok(())
    }
    /// A signed renewal is confined to the already admitted node and this
    /// exact carrier endpoint. It never renews its bootstrap authority.
    pub fn accept_renewal(
        &mut self,
        record: PeerSignedRelayRecordV1,
        expected_pin: [u8; 32],
        expected_endpoint: [u8; 32],
        instance: [u8; 32],
        now: u64,
    ) -> Result<()> {
        self.observe(now)?;
        let address = verify(&record, now, MAX_SERVICE_LEASE_MS)?;
        ensure!(
            record.relay_public_key == expected_pin
                && address.address.endpoint_key == expected_endpoint
                && address.instance == instance,
            "renewal identity differs"
        );
        let entry = self.entries.get(&expected_pin).context("service revoked")?;
        ensure!(
            entry.expires > now && record.expires_at_ms <= entry.expires,
            "renewal exceeds authority"
        );
        newer(&entry.current, &record)?;
        if self.is_self_owned() {
            ensure!(
                IrohServiceEndpointV1::from_record(&entry.current)?
                    .address
                    .endpoint_key
                    == expected_endpoint,
                "self-owned renewal changed transport"
            );
        }
        let mut entries = self.entries.clone();
        let entry = entries.get_mut(&expected_pin).expect("checked entry");
        if self.is_self_owned() {
            entry.grant = record.clone();
        }
        entry.current = record;
        self.publish(self.sources.clone(), entries, self.tombstones.clone())
    }
}
fn validated_sources(
    sources: &[BootstrapSourceV1],
    policy: &BootstrapTrustPolicyV1,
    now: u64,
) -> Result<BTreeMap<[u8; 32], Entry>> {
    ensure!(
        !sources.is_empty() && sources.len() <= 4,
        "bootstrap source budget"
    );
    let mut ids = BTreeSet::new();
    let mut entries = BTreeMap::<[u8; 32], Entry>::new();
    for source in sources {
        let manifest = &source.manifest;
        ensure!(
            !manifest.manifest_id.is_empty()
                && manifest.manifest_id.len() <= 128
                && ids.insert(&manifest.manifest_id)
                && manifest.signatures.len() <= 8
                && (1..=2).contains(&manifest.candidate_limit)
                && manifest.relay_records.len() <= 2
                && manifest
                    .expires_at_ms
                    .checked_sub(manifest.issued_at_ms)
                    .is_some_and(|ttl| ttl > 0 && ttl <= MAX_BOOTSTRAP_LEASE_MS),
            "bootstrap disclosure/lifetime bounds"
        );
        ensure!(
            directory::validate_bootstrap_manifest_v1(manifest, policy, now).accepted,
            "untrusted bootstrap source"
        );
        for signature in &manifest.signatures {
            strict_key(&signature.signer_public_key)?;
            if policy
                .allowed_signer_peer_ids
                .contains(&signature.signer_peer_id)
            {
                VerifyingKey::from_bytes(&signature.signer_public_key)?.verify_strict(
                    &directory::bootstrap_manifest_signing_bytes_v1(manifest),
                    &Signature::from_slice(&signature.signature)?,
                )?;
            }
        }
        for record in &manifest.relay_records {
            verify(record, now, MAX_BOOTSTRAP_LEASE_MS)?;
            let expires = record.expires_at_ms.min(manifest.expires_at_ms);
            match entries.get_mut(&record.relay_public_key) {
                Some(entry) => {
                    ensure!(entry.grant == *record, "conflicting bootstrap node records");
                    entry.expires = entry.expires.max(expires);
                }
                None => {
                    entries.insert(
                        record.relay_public_key,
                        Entry {
                            grant: record.clone(),
                            current: record.clone(),
                            expires,
                            decoded: None,
                        },
                    );
                }
            }
        }
    }
    ensure!(entries.len() <= 2, "aggregate candidate disclosure budget");
    Ok(entries)
}

pub struct ServiceLeaseIssuerV1 {
    key: SigningKey,
    original: PeerSignedRelayRecordV1,
    state: Mutex<(u64, u64)>,
}
impl ServiceLeaseIssuerV1 {
    /// The install owner supplies a signed record and non-reused sequence floor.
    /// Restart/rotation requires a new signed source, never a counter reset.
    pub fn new(key: SigningKey, original: PeerSignedRelayRecordV1, now: u64) -> Result<Self> {
        verify(&original, now, MAX_BOOTSTRAP_LEASE_MS)?;
        ensure!(
            original.relay_public_key == key.verifying_key().to_bytes(),
            "issuer key differs"
        );
        let state = Mutex::new((original.sequence, now));
        Ok(Self {
            key,
            original,
            state,
        })
    }
    pub fn issue(&self, now: u64) -> Result<PeerSignedRelayRecordV1> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("issuer unavailable"))?;
        ensure!(
            now >= state.1 && now < self.original.expires_at_ms,
            "issuer expired or clock rollback"
        );
        state.0 = state.0.checked_add(1).context("lease sequence exhausted")?;
        state.1 = now;
        Ok(directory::sign_relay_record_v1(
            &self.key,
            self.original.record_id.clone(),
            self.original.endpoints.clone(),
            now,
            now.saturating_add(MAX_SERVICE_LEASE_MS)
                .min(self.original.expires_at_ms),
            state.0,
        )?)
    }
}
/// Explicit prelude for this service only. Old peers fail on the magic; there
/// is no fallback into an old handshake. The enclosing carrier owns timeout,
/// authentication endpoint pin, cancellation and frame allocation bounds.
pub async fn request_service_lease_v1(
    stream: &mut CarrierStreamV1<'_>,
) -> Result<PeerSignedRelayRecordV1> {
    stream.write_frame(REQUEST).await?;
    let response = stream.read_frame().await?;
    ensure!(
        response.len() > RESPONSE.len()
            && response.len() <= MAX_SERVICE_LEASE_BYTES
            && response.starts_with(RESPONSE),
        "service lease response version/size"
    );
    Ok(serde_json::from_slice(&response[RESPONSE.len()..])?)
}
pub async fn serve_service_lease_v1(
    stream: &mut CarrierStreamV1<'_>,
    issuer: &ServiceLeaseIssuerV1,
    now: impl FnOnce() -> Result<u64>,
) -> Result<()> {
    ensure!(
        stream.read_frame().await?.as_slice() == REQUEST,
        "service lease request version"
    );
    let record = issuer.issue(now()?)?;
    let mut bytes = RESPONSE.to_vec();
    bytes.extend_from_slice(&serde_json::to_vec(&record)?);
    ensure!(
        bytes.len() <= MAX_SERVICE_LEASE_BYTES,
        "service lease response budget"
    );
    stream.write_frame(&bytes).await
}
#[cfg(test)]
#[path = "product_service_lease_tests.rs"]
mod tests;

#[path = "product_service_checkpoint.rs"]
mod checkpoint;
pub use checkpoint::{ServiceLeasePersistenceV1, MAX_SERVICE_CHECKPOINT_BYTES};

#[path = "product_service_host.rs"]
mod host;
pub(crate) use host::accept_mailbox_multiplex_lane;
pub use host::{
    accept_service_offer_with_activation_v2, request_mailbox_continuous_v4,
    request_mailbox_continuous_v6, request_mailbox_multiplex_lane_v5,
    request_mailbox_multiplex_lane_v6, request_mailbox_multiplex_v5, request_mailbox_multiplex_v6,
    request_service_offer_v2, serve_bootstrap_sources_v2, serve_self_owned_node_selected_v4,
    serve_service_offer_selected_v2, serve_service_offer_v2, ServiceLeaseHostV1,
    ServiceLeaseOfferV2, ServiceOfferStepV2,
};
pub use host::{
    request_mailbox_offer_v1, request_mailbox_offer_v2, request_mailbox_offer_v3,
    serve_node_offer_selected_v1, NodeServiceV1,
};
#[path = "product_service_file.rs"]
mod file;
pub use file::ServiceLeaseFileV1;

#[path = "product_service_self_owned.rs"]
mod self_owned;
pub use self_owned::{SelfOwnedTrustV1, ServiceBootstrapCandidateV1};
