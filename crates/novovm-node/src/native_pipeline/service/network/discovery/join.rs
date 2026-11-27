//! First-run consumer of the same public NOVONET locator service as validators.
use super::*;
use crate::native_pipeline::{
    consensus::{
        native::{MessageDomain, ValidatorTransportBindings},
        statement::ParentPoint,
        wire::{Validator, ValidatorSet},
    },
    service::{
        client::operator::receipt_proof::{
            transaction_hash, validate_parent, FinalityVerifier, ProofTarget,
        },
        GenesisValidator,
    },
};
use novovm_network::{
    iroh_transport::IrohControlV1, product_directory::verify_relay_record_authenticity_v1,
};
use novovm_relay::transport_identity::{
    load_transport_identity, load_transport_verifier, provision_transport_identity,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Write};
mod chain_sync;
mod read_snapshot_service;
pub use chain_sync::{
    export_activation_parent, follow_installed_state, sync_installed_chain, sync_installed_state,
    verify_installed_uca_execution,
};
pub use read_snapshot_service::{
    run_installed_public_read_snapshot, serve_installed_public_read_snapshot,
    serve_installed_read_snapshot,
};

#[cfg(test)]
mod tests;

const INSTALLED_SOURCE_BUDGET: Duration = Duration::from_secs(8);

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    chain_id: u64,
    genesis: [u8; 32],
    protocol: [u8; 32],
    library: PathBuf,
    library_sha256: String,
    nameserver: IpAddr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secondary_nameserver: Option<IpAddr>,
    relays: Vec<String>,
    /// Independently operated entry points for the original signed locator service.
    seeds: [SeedPin; 2],
    /// Release-pinned read services. Their signatures authenticate coordinates,
    /// while the validator set certifies the blocks supplied by those services.
    lookup_transport_keys: Vec<Vec<u8>>,
    /// Native query routes pinned by the same installed release as the bootstrap keys.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    native_query_peers: Vec<NativeQueryPeer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    verification: Option<VerificationPins>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeQueryPeer {
    address: SocketAddr,
    transport_key: Vec<u8>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct VerificationPins {
    validator_epoch: u64,
    validator_set_hash: [u8; 32],
    validators: Vec<GenesisValidator>,
    trusted_parent: ParentPoint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    minimum_finalized_checkpoint: Option<MinimumFinalizedCheckpoint>,
}

/// The issuer pins an independently known block. Its hash binds the state,
/// ordered receipt root and the exact execution statement.
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MinimumFinalizedCheckpoint {
    height: u64,
    block_hash: [u8; 32],
    valid_until_unix_ms: u64,
}

impl MinimumFinalizedCheckpoint {
    fn validate(self, anchor: ParentPoint) -> Result<()> {
        ensure!(
            self.height >= anchor.height
                && self.height != 0
                && self.block_hash != [0; 32]
                && self.valid_until_unix_ms != 0,
            "invalid minimum finalized checkpoint in the installed profile"
        );
        self.check(anchor)
    }

    fn check(self, point: ParentPoint) -> Result<()> {
        ensure!(
            point.height != self.height || point.block_hash == self.block_hash,
            "finalized header conflicts with the installed minimum checkpoint at height {}",
            self.height
        );
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SeedPin {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    locator: Option<PathBuf>,
    /// A release-pinned dial route. The remote still proves possession of
    /// transport_key and returns only currently valid signed locator records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    contact: Option<IrohAddressV1>,
    transport_key: Vec<u8>,
}

impl Profile {
    fn carrier_config(&self) -> Result<IrohConfigV1> {
        let config = IrohConfigV1::new(&self.relays, self.nameserver)?;
        match self.secondary_nameserver {
            Some(address) => config.with_secondary_nameserver(address),
            None => Ok(config),
        }
    }

    fn pins(&self) -> Pins {
        (self.chain_id, self.genesis, self.protocol)
    }

    fn public_read_referrals(&self) -> bool {
        self.verification.is_some() && !self.relays.is_empty()
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.chain_id != 0 && self.genesis != [0; 32] && self.protocol != [0; 32],
            "installed NOVONET chain pins missing"
        );
        ensure!(
            self.lookup_transport_keys.len() <= 28
                && (!self.lookup_transport_keys.is_empty() || self.public_read_referrals())
                && self.lookup_transport_keys.iter().all(|key| is_transport_key(key))
                && self.lookup_transport_keys.iter().collect::<BTreeSet<_>>().len()
                    == self.lookup_transport_keys.len(),
            "installed NOVONET lookup pins must be unique ML-DSA-65 keys within the discovery request budget"
        );
        ensure!(
            self.seeds[0].transport_key != self.seeds[1].transport_key
                && self
                    .seeds
                    .iter()
                    .all(|seed| is_transport_key(&seed.transport_key)),
            "installed discovery requires distinct bootstrap transport pins"
        );
        if let Some(verification) = &self.verification {
            for key in self
                .lookup_transport_keys
                .iter()
                .chain(self.seeds.iter().map(|seed| &seed.transport_key))
            {
                ensure!(
                    verification.validators.iter().all(|v| &v.public_key != key),
                    "discovery transport key overlaps a validator signing key"
                );
            }
        }
        let mut endpoint_keys = BTreeSet::new();
        for seed in &self.seeds {
            ensure!(
                seed.locator
                    .as_ref()
                    .is_some_and(|path| !path.as_os_str().is_empty())
                    || seed.contact.is_some(),
                "installed bootstrap requires a locator or pinned contact"
            );
            if let Some(contact) = &seed.contact {
                IrohServiceEndpointV1::validate_address(contact)?;
                ensure!(
                    endpoint_keys.insert(contact.endpoint_key)
                        && contact
                            .relays
                            .iter()
                            .all(|relay| self.relays.contains(relay)),
                    "installed bootstrap contacts require distinct endpoints and approved relays"
                );
            }
        }
        ensure!(
            self.native_query_peers.len() <= self.lookup_transport_keys.len()
                && self.native_query_peers.iter().all(|peer| {
                    peer.address.port() != 0
                        && !peer.address.ip().is_unspecified()
                        && !peer.address.ip().is_multicast()
                        && peer.address.ip() != IpAddr::from([255, 255, 255, 255])
                        && self.lookup_transport_keys.contains(&peer.transport_key)
                })
                && self
                    .native_query_peers
                    .iter()
                    .map(|peer| &peer.transport_key)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == self.native_query_peers.len(),
            "invalid installed native query routes"
        );
        self.carrier_config()?;
        Ok(())
    }

    fn verification_domain(&self) -> Result<(MessageDomain, ParentPoint)> {
        let pins = self
            .verification
            .as_ref()
            .context("installed proof pins missing")?;
        ensure!(
            (1..=64).contains(&pins.validators.len()),
            "installed validator set size"
        );
        let validators = ValidatorSet::new(
            self.chain_id,
            pins.validator_epoch,
            1,
            pins.validators
                .iter()
                .map(|v| Validator::new(&v.public_key, v.weight))
                .collect::<Result<Vec<_>>>()?,
        )?;
        ensure!(
            validators.hash() == pins.validator_set_hash,
            "installed validator set commitment differs"
        );
        ValidatorTransportBindings::new(
            &validators,
            pins.validators
                .iter()
                .map(|v| (v.public_key.clone(), v.transport_public_key.clone())),
        )?;
        if let Some(minimum) = pins.minimum_finalized_checkpoint {
            minimum.validate(pins.trusted_parent)?;
        }
        Ok((
            MessageDomain {
                chain_id: self.chain_id,
                genesis: self.genesis,
                protocol: self.protocol,
                validators: Arc::new(validators),
            },
            pins.trusted_parent,
        ))
    }
}

fn is_transport_key(key: &[u8]) -> bool {
    key.len() == novovm_relay::purpose_keys::VALIDATOR_PUBLIC_BYTES
        && key.iter().any(|byte| *byte != 0)
}

/// Read the installed trust profile and authenticate to available bootstrap
/// sources using a local transport-purpose identity. Advertise no device locator.
pub fn discover_installed(path: &Path) -> Result<serde_json::Value> {
    discover_installed_from_profile(path, None)
}

pub fn discover_installed_from_profile(
    path: &Path,
    profile_sha256: Option<&str>,
) -> Result<serde_json::Value> {
    let (profile, identity) = pinned_installed_profile(path, profile_sha256)?;
    let seeds = read_seeds(profile_base(path)?, &profile);
    let carrier = profile.carrier_config()?;
    let report = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(discover(carrier, identity.clone(), &profile, seeds))?;
    let read_id = novovm_network::discovery_control::public_read_locator_id_v1(profile.pins());
    let mut result = serde_json::json!({
        "chain_id": profile.chain_id,
        "transport_identity_sha256": fingerprint(identity.public_key()),
        "authenticated_discovery_sources": report.sources,
        "signed_locators_by_source": report.locators_by_source,
        "signed_locator_candidates": report.records.values().map(|record| LocatorSummary::new(record, &read_id)).collect::<Vec<_>>(),
    });
    if !report.referral_status.is_empty() {
        result["public_read_status_by_source"] = serde_json::json!(report.referral_status);
    }
    if let Some(pin) = profile_sha256 {
        result["profile_sha256"] = pin.to_ascii_lowercase().into();
    }
    Ok(result)
}

/// Discovery and proof requests authenticate with the same local identity.
/// Referrals use a relay-only connection. Proof pins come from the installed profile.
pub fn discover_verified_receipt(path: &Path, hash: &str) -> Result<serde_json::Value> {
    discover_verified_receipt_from_profile(path, hash, None)
}

/// Check a release profile pin before loading custody or opening either stream.
pub fn discover_verified_receipt_from_profile(
    path: &Path,
    hash: &str,
    profile_sha256: Option<&str>,
) -> Result<serde_json::Value> {
    discover_verified_receipt_at(path, hash, profile_sha256, ReceiptLocation::AnchorSuccessor)
}

#[cfg(test)]
mod historical_receipt_tests;
#[cfg(test)]
pub(in crate::native_pipeline::service) use historical_receipt_tests::test_slow_cached_receipt;

/// Only QueryByHash discloses the transaction hash to the authenticated archive.
#[derive(Clone, Copy)]
pub enum ReceiptLocation {
    AnchorSuccessor,
    Height(u64),
    QueryByHash,
}

pub fn discover_verified_receipt_at(
    path: &Path,
    hash: &str,
    profile_sha256: Option<&str>,
    location: ReceiptLocation,
) -> Result<serde_json::Value> {
    verified_receipt_at(path, hash, profile_sha256, location, true)
}

/// An installed-device receipt query may only use custody established on that device.
pub fn discover_existing_verified_receipt_at(
    path: &Path,
    hash: &str,
    profile_sha256: &str,
    location: ReceiptLocation,
) -> Result<serde_json::Value> {
    verified_receipt_at(path, hash, Some(profile_sha256), location, false)
}

fn verified_receipt_at(
    path: &Path,
    hash: &str,
    profile_sha256: Option<&str>,
    location: ReceiptLocation,
    provision: bool,
) -> Result<serde_json::Value> {
    let transaction = transaction_hash(hash)?;
    let (profile, domain, anchor) = installed_chain_profile(path, profile_sha256)?;
    let data = installed_state_base()?;
    let search = chain_sync::ReceiptSearch::open(&data, &profile, anchor, transaction)?;
    let base = profile_base(path)?;
    let identity = client_identity(&data, base, &profile, provision)?;
    let verifier = Arc::new(FinalityVerifier::new(&domain, identity.clone()));
    search.execute(base, &profile, &identity, &domain, location, verifier)
}

/// Validate the pinned release's chain and discovery inputs without accessing
/// device custody or allocating a transport endpoint.
pub fn check_installed_profile(path: &Path, sha256: &str) -> Result<serde_json::Value> {
    let (profile, domain, anchor) = installed_chain_profile(path, Some(sha256))?;
    validate_release_bootstraps(profile_base(path)?, &profile)?;
    Ok(serde_json::json!({
        "kind": "validated-installed-discovery-profile",
        "chain_id": domain.chain_id,
        "trusted_height": anchor.height,
    }))
}

fn validate_release_bootstraps(base: &Path, profile: &Profile) -> Result<()> {
    if profile.seeds.iter().all(|seed| seed.locator.is_none()) {
        // Profile::validate already checks both fixed contacts.
        return Ok(());
    }

    let verifier = load_transport_verifier(&base.join(&profile.library), &profile.library_sha256)?;
    let now = wall_ms()?;
    let mut endpoints = [BTreeSet::new(), BTreeSet::new()];
    for (index, (seed, keys)) in profile.seeds.iter().zip(&mut endpoints).enumerate() {
        if let Some(contact) = &seed.contact {
            keys.insert(contact.endpoint_key);
        }
        if let Some(locator) = &seed.locator {
            let record: PeerSignedRelayRecordV1 = read_document(&base.join(locator))
                .with_context(|| format!("read installed bootstrap locator {}", index + 1))?;
            ensure!(
                record.relay_public_key == seed.transport_key
                    && record.record_id == native_locator_id_v1(profile.pins()),
                "bootstrap locator signer or chain domain differs from installed pin"
            );
            verify_relay_record_authenticity_v1(&record, &verifier)
                .context("verify installed bootstrap locator signature")?;
            ensure!(
                record.issued_at_ms <= now,
                "future installed bootstrap locator"
            );
            let route = IrohServiceEndpointV1::from_record(&record)?.address;
            if record.expires_at_ms > now {
                ensure!(
                    route
                        .relays
                        .iter()
                        .all(|relay| profile.relays.contains(relay)),
                    "installed bootstrap locator uses an unapproved relay"
                );
                keys.insert(route.endpoint_key);
            } else {
                ensure!(
                    seed.contact.is_some(),
                    "expired installed bootstrap locator without fixed contact"
                );
            }
        }
    }
    ensure!(
        endpoints[0].is_disjoint(&endpoints[1]),
        "installed discovery requires distinct source identities and endpoints"
    );
    Ok(())
}

fn installed_chain_profile(
    path: &Path,
    sha256: Option<&str>,
) -> Result<(Profile, MessageDomain, ParentPoint)> {
    let profile = read_installed_profile(path, sha256)?;
    let (domain, anchor) = profile.verification_domain()?;
    validate_parent(&anchor)?;
    chain_sync::check_profile_checkpoint_deadline(&profile)?;
    Ok((profile, domain, anchor))
}

fn pinned_installed_profile(
    path: &Path,
    expected_sha256: Option<&str>,
) -> Result<(Profile, TransportIdentity)> {
    let data = installed_state_base()?;
    let profile = read_installed_profile(path, expected_sha256)?;
    let identity = local_identity(&data, profile_base(path)?, &profile)?;
    Ok((profile, identity))
}

fn read_installed_profile(path: &Path, expected_sha256: Option<&str>) -> Result<Profile> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take((MAX_CONFIG + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_CONFIG,
        "installed profile exceeds configuration budget"
    );
    if let Some(expected) = expected_sha256 {
        ensure!(
            fingerprint(&bytes).eq_ignore_ascii_case(expected),
            "installed profile SHA-256 differs from its release pin"
        );
    }
    let profile: Profile = serde_json::from_slice(&bytes)?;
    profile.validate()?;
    Ok(profile)
}

fn profile_base(path: &Path) -> Result<&Path> {
    path.parent().context("installed profile directory missing")
}

type SeedInputs = [Result<PeerSignedRelayRecordV1>; 2];

fn read_seeds(base: &Path, profile: &Profile) -> SeedInputs {
    profile.seeds.each_ref().map(|seed| {
        let path = seed
            .locator
            .as_ref()
            .context("bootstrap locator file is not configured")?;
        read_document(&base.join(path))
    })
}

fn installed_state_base() -> Result<PathBuf> {
    #[cfg(windows)]
    let data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let data = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|home| home.join(".local/state")));
    let data = data.context("user state directory unavailable")?;
    ensure!(data.is_absolute(), "absolute user state directory required");
    Ok(data)
}

fn fingerprint(key: &[u8]) -> String {
    format!("{:x}", Sha256::digest(key))
}

fn state_path(data: &Path, profile: &Profile) -> PathBuf {
    data.join("NOVOVM").join("native-discovery").join(format!(
        "{}-{}-{}",
        profile.chain_id,
        super::super::super::rpc::hex(&profile.genesis),
        super::super::super::rpc::hex(&profile.protocol),
    ))
}

fn local_identity(data: &Path, base: &Path, profile: &Profile) -> Result<TransportIdentity> {
    client_identity(data, base, profile, true)
}

fn client_identity(
    data: &Path,
    base: &Path,
    profile: &Profile,
    provision: bool,
) -> Result<TransportIdentity> {
    let root = state_path(data, profile);
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    if provision {
        builder.create(&root)?;
    }
    let key = root.join("transport.protected");
    let pin = root.join("transport.sha256");
    let library = base.join(&profile.library);
    let identity = match fs::symlink_metadata(&pin) {
        Ok(metadata) => {
            ensure!(metadata.is_file(), "invalid local NOVONET identity pin");
            load_transport_identity(&key, profile.chain_id, &library, &profile.library_sha256)
                .context("established local NOVONET custody missing or invalid")?
        }
        Err(error) if provision && error.kind() == std::io::ErrorKind::NotFound => {
            let identity = provision_transport_identity(
                &key,
                profile.chain_id,
                &library,
                &profile.library_sha256,
            )?;
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&pin)
            {
                Ok(mut file) => {
                    file.write_all(fingerprint(identity.public_key()).as_bytes())?;
                    file.sync_all()?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(error) => return Err(error.into()),
            }
            identity
        }
        Err(error) if !provision && error.kind() == std::io::ErrorKind::NotFound => {
            return Err(error).context("installed NOVONET identity pin is missing");
        }
        Err(error) => return Err(error.into()),
    };
    let mut stored = Vec::new();
    fs::File::open(pin)?.take(65).read_to_end(&mut stored)?;
    ensure!(
        stored == fingerprint(identity.public_key()).as_bytes(),
        "local NOVONET transport identity differs from first-run pin"
    );
    Ok(identity)
}

struct DiscoveryReport {
    sources: Vec<String>,
    locators_by_source: BTreeMap<String, Vec<LocatorSummary>>,
    referral_status: BTreeMap<String, &'static str>,
    conflicting_locator_keys: BTreeMap<Vec<u8>, u64>,
    records: BTreeMap<Vec<u8>, PeerSignedRelayRecordV1>,
}

impl DiscoveryReport {
    fn record_locator(&mut self, source: &str, record: PeerSignedRelayRecordV1, public_id: &str) {
        use std::collections::btree_map::Entry;

        if self
            .conflicting_locator_keys
            .contains_key(&record.relay_public_key)
        {
            return;
        }
        self.locators_by_source
            .entry(source.to_owned())
            .or_default()
            .push(LocatorSummary::new(&record, public_id));
        match self.records.entry(record.relay_public_key.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(record);
            }
            Entry::Occupied(mut entry) if entry.get().sequence < record.sequence => {
                entry.insert(record);
            }
            Entry::Occupied(entry)
                if entry.get().sequence == record.sequence && entry.get() != &record =>
            {
                self.conflicting_locator_keys
                    .insert(record.relay_public_key, record.sequence);
                entry.remove();
            }
            Entry::Occupied(_) => {}
        }
    }
}

#[derive(PartialEq, Eq, Serialize)]
struct LocatorSummary {
    transport_identity_sha256: String,
    sequence: u64,
    expires_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    purpose: Option<&'static str>,
}

impl LocatorSummary {
    fn new(record: &PeerSignedRelayRecordV1, public_read_id: &str) -> Self {
        Self {
            purpose: (record.record_id == public_read_id).then_some("public_read"),
            transport_identity_sha256: fingerprint(&record.relay_public_key),
            sequence: record.sequence,
            expires_at_ms: record.expires_at_ms,
        }
    }
}

async fn discover(
    carrier: IrohConfigV1,
    identity: TransportIdentity,
    profile: &Profile,
    seeds: SeedInputs,
) -> Result<DiscoveryReport> {
    let owner = bind_carrier(carrier).await?;
    let report = collect(&owner, identity, profile, seeds).await;
    let closed = owner.close().await;
    let report = report?;
    closed?;
    Ok(report)
}

async fn bind_carrier(carrier: IrohConfigV1) -> Result<IrohEndpointOwnerV1> {
    tokio::time::timeout(
        Duration::from_secs(8),
        IrohEndpointOwnerV1::bind(carrier, || Ok(())),
    )
    .await
    .context("NOVONET carrier start deadline")?
}

async fn collect(
    owner: &IrohEndpointOwnerV1,
    identity: TransportIdentity,
    profile: &Profile,
    seeds: SeedInputs,
) -> Result<DiscoveryReport> {
    let candidates = prepare_seed_candidates(profile, &identity, seeds)?;
    let lookups = InstalledLookups::new(profile);
    owner
        .with_operation(
            IrohControlV1::new(Instant::now() + Duration::from_secs(10), || Ok(()))?,
            |scope| {
                Box::pin(lookup_sources(
                    scope,
                    identity,
                    lookups,
                    candidates,
                    INSTALLED_SOURCE_BUDGET,
                ))
            },
        )
        .await
}

struct SeedCandidates {
    preferred: DialCandidate,
    fallback: Option<DialCandidate>,
}

impl SeedCandidates {
    fn routes(&self) -> impl Iterator<Item = &DialCandidate> {
        std::iter::once(&self.preferred).chain(&self.fallback)
    }
}

fn prepare_seed_candidates(
    profile: &Profile,
    identity: &TransportIdentity,
    seeds: SeedInputs,
) -> Result<[Result<SeedCandidates>; 2]> {
    let pins = profile.pins();
    let wall = wall_ms()?;
    let accept = |record: Result<PeerSignedRelayRecordV1>, seed: &SeedPin| {
        let installed = |route: &IrohAddressV1| DialCandidate {
            key: seed.transport_key.clone(),
            route: route.clone(),
            source: "installed_contact",
            sequence: 0,
        };
        record
            .and_then(|record| {
                ensure!(
                    record.relay_public_key == seed.transport_key,
                    "bootstrap locator signer differs from installed pin"
                );
                ensure!(
                    seed.contact.is_none() || record.expires_at_ms > wall,
                    "use the installed contact to renew an expired bootstrap locator"
                );
                let mut preferred = seed_candidate(record, pins, identity, wall)?;
                let fallback = seed.contact.as_ref().and_then(|contact| {
                    if let Some(route) = merge_installed_contact(&preferred.route, contact) {
                        preferred.route = route;
                        None
                    } else {
                        Some(installed(contact))
                    }
                });
                Ok(SeedCandidates {
                    preferred,
                    fallback,
                })
            })
            .or_else(|error| {
                let Some(contact) = &seed.contact else {
                    return Err(error);
                };
                Ok(SeedCandidates {
                    preferred: installed(contact),
                    fallback: None,
                })
            })
    };
    let [first, second] = seeds;
    let (first, second) = (
        accept(first, &profile.seeds[0]),
        accept(second, &profile.seeds[1]),
    );
    if let (Ok(first), Ok(second)) = (&first, &second) {
        ensure!(
            first.routes().all(|left| second
                .routes()
                .all(|right| left.route.endpoint_key != right.route.endpoint_key)),
            "installed discovery requires distinct source identities and endpoints"
        );
    }
    ensure!(
        profile
            .lookup_transport_keys
            .iter()
            .all(|key| key != identity.public_key())
            && profile
                .seeds
                .iter()
                .all(|seed| seed.transport_key != identity.public_key()),
        "ordinary device custody overlaps an installed service pin"
    );
    Ok([first, second])
}

/// Pair coordinates for the same endpoint when they all fit in a single dial.
/// Keep the original routes available for bounded fallback on overflow.
fn merge_installed_contact(
    route: &IrohAddressV1,
    contact: &IrohAddressV1,
) -> Option<IrohAddressV1> {
    if route.endpoint_key != contact.endpoint_key {
        return None;
    }
    Some(IrohAddressV1 {
        endpoint_key: route.endpoint_key,
        direct: merge_coordinates(&route.direct, &contact.direct, 8)?,
        relays: merge_coordinates(&route.relays, &contact.relays, 4)?,
    })
}

fn merge_coordinates<T: Clone + PartialEq>(
    signed: &[T],
    installed: &[T],
    limit: usize,
) -> Option<Vec<T>> {
    let mut merged = Vec::with_capacity(limit);
    for address in signed.iter().chain(installed) {
        if merged.contains(address) {
            continue;
        }
        if merged.len() == limit {
            return None;
        }
        merged.push(address.clone());
    }
    Some(merged)
}

fn seed_candidate(
    record: PeerSignedRelayRecordV1,
    pins: Pins,
    identity: &TransportIdentity,
    wall: u64,
) -> Result<DialCandidate> {
    let mut seed = Seed {
        path: PathBuf::new(),
        book: None,
        key: None,
        bootstrap: None,
        fallback: false,
        job: None,
        next: Instant::now(),
        quarantined: None,
    };
    Discovery::update_seed(&mut seed, &record, pins, identity, wall)?;
    seed.dial_candidate(wall)?
        .context("installed bootstrap is unavailable")
}

struct InstalledLookups {
    pins: Pins,
    fixed: Vec<Vec<u8>>,
    approved_relays: Vec<String>,
    voting_keys: BTreeSet<Vec<u8>>,
}

impl InstalledLookups {
    fn new(profile: &Profile) -> Self {
        Self {
            pins: profile.pins(),
            fixed: profile.lookup_transport_keys.clone(),
            approved_relays: if profile.public_read_referrals() {
                profile.relays.clone()
            } else {
                Vec::new()
            },
            voting_keys: profile
                .verification
                .iter()
                .flat_map(|v| &v.validators)
                .map(|v| v.public_key.clone())
                .collect(),
        }
    }

    fn accepts_public_reader(&self, record: &PeerSignedRelayRecordV1, client_key: &[u8]) -> bool {
        let peer = &record.relay_public_key;
        peer != client_key
            && !self.voting_keys.contains(peer)
            && !self.fixed.contains(peer)
            && IrohServiceEndpointV1::from_record(record).is_ok_and(|endpoint| {
                endpoint
                    .address
                    .relays
                    .iter()
                    .any(|relay| chain_sync::approved_read_relay(&self.approved_relays, relay))
            })
    }
}

async fn lookup_sources(
    scope: &IrohScopeV1,
    identity: TransportIdentity,
    plan: InstalledLookups,
    [first, second]: [Result<SeedCandidates>; 2],
    source_budget: Duration,
) -> Result<DiscoveryReport> {
    let failures = Arc::new(NetworkFailures::default());
    let pins = plan.pins;
    let sample = !plan.approved_relays.is_empty();
    let client_key = identity.public_key().to_vec();
    let lookup = |candidate: Result<SeedCandidates>, identity, lookups, failures| async move {
        let SeedCandidates {
            preferred,
            fallback,
        } = candidate?;
        let source = fingerprint(&preferred.key);
        // Give the installed fallback time within the same source deadline.
        // Only the first successful authenticated lookup supplies records.
        let until = sample
            .then(|| Instant::now() + source_budget.saturating_sub(Duration::from_millis(100)));
        let reader = || Exchange::Reader {
            sample_until: until,
        };
        let records = tokio::time::timeout(source_budget, async {
            let Some(fallback) = fallback else {
                return query(
                    scope,
                    identity,
                    preferred,
                    pins,
                    reader(),
                    lookups,
                    failures,
                )
                .await;
            };
            let mut preferred = std::pin::pin!(query(
                scope,
                identity.clone(),
                preferred,
                pins,
                reader(),
                lookups.clone(),
                failures.clone(),
            ));
            let mut fallback = std::pin::pin!(query(
                scope,
                identity,
                fallback,
                pins,
                reader(),
                lookups,
                failures,
            ));
            // Expiring the head start leaves the preferred future alive.
            let (result, remaining, finished) =
                match tokio::time::timeout(source_budget / 2, &mut preferred).await {
                    Ok(result) => (result, fallback.as_mut(), "signed locator"),
                    Err(_) => tokio::select! {
                        result = &mut preferred => (result, fallback.as_mut(), "signed locator"),
                        result = &mut fallback => (result, preferred.as_mut(), "fixed contact"),
                    },
                };
            match result {
                Ok(records) => Ok(records),
                Err(error) => remaining.await.with_context(|| {
                    format!("remaining bootstrap route after {finished} failed: {error:#}")
                }),
            }
        })
        .await
        .with_context(|| format!("bootstrap {source} exceeded its lookup deadline"))?
        .with_context(|| format!("bootstrap {source}"))?;
        Ok::<_, anyhow::Error>((source, records))
    };
    let (primary, secondary) = tokio::join!(
        lookup(
            first,
            identity.clone(),
            plan.fixed.clone(),
            failures.clone()
        ),
        lookup(second, identity, plan.fixed.clone(), failures),
    );
    let mut report = DiscoveryReport {
        sources: Vec::new(),
        locators_by_source: BTreeMap::new(),
        referral_status: BTreeMap::new(),
        conflicting_locator_keys: BTreeMap::new(),
        records: BTreeMap::new(),
    };
    let mut errors = Vec::new();
    let public_id = novovm_network::discovery_control::public_read_locator_id_v1(pins);
    let wall = wall_ms()?;
    for result in [primary, secondary] {
        let (source, records) = match result {
            Ok(found) => found,
            Err(error) => {
                errors.push(format!("{error:#}"));
                continue;
            }
        };
        report.sources.push(source.clone());
        if let Some(status) = records.referral_status {
            report.referral_status.insert(source.clone(), status);
        }
        report.locators_by_source.entry(source.clone()).or_default();
        for record in records
            .records
            .into_iter()
            .filter(|record| record.expires_at_ms > wall)
        {
            if record.record_id == public_id && !plan.accepts_public_reader(&record, &client_key) {
                continue;
            }
            report.record_locator(&source, record, &public_id);
        }
    }
    ensure!(
        !report.sources.is_empty(),
        "no authenticated NOVONET bootstrap: {}",
        errors.join("; ")
    );
    Ok(report)
}
