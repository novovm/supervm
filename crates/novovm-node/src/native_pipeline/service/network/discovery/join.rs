//! First-run consumer of the same public NOVONET locator service as validators.
use super::*;
use crate::native_pipeline::{
    consensus::{
        native::{MessageDomain, ValidatorTransportBindings},
        statement::ParentPoint,
        wire::{Validator, ValidatorSet},
    },
    service::{
        client::operator::receipt_proof::{transaction_hash, validate_parent, ProofTarget},
        GenesisValidator,
    },
};
use novovm_network::iroh_transport::IrohControlV1;
use novovm_relay::transport_identity::{load_transport_identity, provision_transport_identity};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Write};
mod chain_sync;
pub use chain_sync::{
    follow_installed_state, sync_installed_chain, sync_installed_state,
    verify_installed_uca_execution,
};

#[cfg(test)]
mod tests;

const INITIAL_SOURCE_BUDGET: Duration = Duration::from_secs(8);
const REFRESH_SOURCE_BUDGET: Duration = Duration::from_secs(5);

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    chain_id: u64,
    genesis: [u8; 32],
    protocol: [u8; 32],
    library: PathBuf,
    library_sha256: String,
    nameserver: IpAddr,
    relays: Vec<String>,
    /// Independently operated entry points for the original signed locator service.
    seeds: [SeedPin; 2],
    /// Transport identity pins from the installed chain profile.
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
    fn pins(&self) -> Pins {
        (self.chain_id, self.genesis, self.protocol)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.chain_id != 0 && self.genesis != [0; 32] && self.protocol != [0; 32],
            "installed NOVONET chain pins missing"
        );
        ensure!(
            (1..=28).contains(&self.lookup_transport_keys.len())
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
        ensure!(
            self.lookup_transport_keys.iter().all(|key| pins
                .validators
                .iter()
                .any(|v| &v.transport_public_key == key)),
            "lookup key is outside the installed chain transport bindings"
        );
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
    let (profile, identity, seeds) = installed_inputs(path)?;
    let carrier = IrohConfigV1::new(&profile.relays, profile.nameserver)?;
    let report = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(discover(carrier, identity.clone(), &profile, seeds))?;
    Ok(serde_json::json!({
        "chain_id": profile.chain_id,
        "transport_identity_sha256": fingerprint(identity.public_key()),
        "authenticated_discovery_sources": report.sources,
        "signed_locator_candidates": report.records.values().map(|record| serde_json::json!({
            "transport_identity_sha256": fingerprint(&record.relay_public_key),
            "sequence": record.sequence,
            "expires_at_ms": record.expires_at_ms,
        })).collect::<Vec<_>>(),
    }))
}

/// The discovery and native proof streams use the same owned carrier and local
/// identity. All proof pins come from the installed profile.
pub fn discover_verified_receipt(path: &Path, hash: &str) -> Result<serde_json::Value> {
    discover_verified_receipt_from_profile(path, hash, None)
}

/// Check a release profile pin before loading custody or opening either stream.
pub fn discover_verified_receipt_from_profile(
    path: &Path,
    hash: &str,
    profile_sha256: Option<&str>,
) -> Result<serde_json::Value> {
    let transaction = transaction_hash(hash)?;
    let (profile, identity) = pinned_installed_profile(path, profile_sha256)?;
    let seeds = read_seeds(profile_base(path)?, &profile);
    let (domain, parent) = profile.verification_domain()?;
    let target = ProofTarget::new(&domain, identity.clone(), parent, transaction)?;
    let carrier = IrohConfigV1::new(&profile.relays, profile.nameserver)?;
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let owner = bind_carrier(carrier).await?;
        let result = async {
            let report = collect(&owner, identity.clone(), &profile, seeds).await?;
            let result = owner.with_continuous_service_v4(|| Ok(()), |scope| Box::pin(async move {
                let deadline = Instant::now() + Duration::from_secs(30);
                let mut last = None;
                for candidate in report.records.into_values() {
                    if candidate.expires_at_ms <= wall_ms()? || Instant::now() >= deadline { continue }
                    let budget = Duration::from_secs(10).min(deadline.saturating_duration_since(Instant::now()));
                    let proof = super::super::query_carrier::read_receipt(scope, &candidate, identity.clone(), domain.clone(), &target);
                    match tokio::time::timeout(budget, proof).await {
                        Ok(Ok(mut verified)) => {
                            verified["authenticated_discovery_sources"] = serde_json::json!(report.sources);
                            verified["serving_transport_sha256"] = serde_json::json!(fingerprint(&candidate.relay_public_key));
                            verified["transport_identity_sha256"] = serde_json::json!(fingerprint(identity.public_key()));
                            return Ok(verified);
                        }
                        Ok(Err(error)) => last = Some(error),
                        Err(error) => last = Some(error.into()),
                    }
                }
                Err(last.unwrap_or_else(|| anyhow::anyhow!("no usable authenticated proof candidate")))
            })).await;
            result
        }.await;
        let closed = owner.close().await;
        let proof = result?;
        closed?;
        Ok(proof)
    })
}

/// Validate the pinned release's chain and discovery inputs without accessing
/// device custody or allocating a transport endpoint.
pub fn check_installed_profile(path: &Path, sha256: &str) -> Result<serde_json::Value> {
    let (_, domain, anchor) = installed_chain_profile(path, Some(sha256))?;
    Ok(serde_json::json!({
        "kind": "validated-installed-discovery-profile",
        "chain_id": domain.chain_id,
        "trusted_height": anchor.height,
    }))
}

fn installed_chain_profile(
    path: &Path,
    sha256: Option<&str>,
) -> Result<(Profile, MessageDomain, ParentPoint)> {
    let profile = read_installed_profile(path, sha256)?;
    let (domain, anchor) = profile.verification_domain()?;
    validate_parent(&anchor)?;
    IrohConfigV1::new(&profile.relays, profile.nameserver)?;
    Ok((profile, domain, anchor))
}

fn installed_profile(path: &Path) -> Result<(Profile, TransportIdentity)> {
    pinned_installed_profile(path, None)
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

fn installed_inputs(path: &Path) -> Result<(Profile, TransportIdentity, SeedInputs)> {
    let (profile, identity) = installed_profile(path)?;
    let seeds = read_seeds(profile_base(path)?, &profile);
    Ok((profile, identity, seeds))
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
    let root = state_path(data, profile);
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&root)?;
    let key = root.join("transport.protected");
    let pin = root.join("transport.sha256");
    let library = base.join(&profile.library);
    let identity = match fs::symlink_metadata(&pin) {
        Ok(metadata) => {
            ensure!(metadata.is_file(), "invalid local NOVONET identity pin");
            load_transport_identity(&key, profile.chain_id, &library, &profile.library_sha256)
                .context("established local NOVONET custody missing or invalid")?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
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
    records: BTreeMap<Vec<u8>, PeerSignedRelayRecordV1>,
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
    let pins = profile.pins();
    let lookups = profile.lookup_transport_keys.clone();
    owner
        .with_operation(
            IrohControlV1::new(Instant::now() + Duration::from_secs(10), || Ok(()))?,
            |scope| {
                Box::pin(lookup_sources(
                    scope,
                    identity,
                    pins,
                    candidates,
                    lookups,
                    INITIAL_SOURCE_BUDGET,
                ))
            },
        )
        .await
}

fn prepare_seed_candidates(
    profile: &Profile,
    identity: &TransportIdentity,
    seeds: SeedInputs,
) -> Result<[Result<DialCandidate>; 2]> {
    let pins = profile.pins();
    let wall = wall_ms()?;
    let accept = |record: Result<PeerSignedRelayRecordV1>, seed: &SeedPin| {
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
                let mut candidate = seed_candidate(record, pins, identity, wall)?;
                if let Some(contact) = &seed.contact {
                    add_installed_contact(&mut candidate.route, contact);
                }
                Ok(candidate)
            })
            .or_else(|error| {
                let Some(contact) = &seed.contact else {
                    return Err(error);
                };
                Ok(DialCandidate {
                    key: seed.transport_key.clone(),
                    route: contact.clone(),
                    source: "installed_contact",
                    sequence: 0,
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
            first.route.endpoint_key != second.route.endpoint_key,
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

/// Pair coordinates only for the same Iroh endpoint, before its one NOVOVM
/// authentication. The installed hints stay local to this dial.
fn add_installed_contact(route: &mut IrohAddressV1, contact: &IrohAddressV1) {
    if route.endpoint_key != contact.endpoint_key {
        return;
    }
    append_coordinates(&mut route.direct, &contact.direct, 8);
    append_coordinates(&mut route.relays, &contact.relays, 4);
}

fn append_coordinates<T: Clone + PartialEq>(route: &mut Vec<T>, installed: &[T], limit: usize) {
    for address in installed {
        if route.len() >= limit {
            break;
        }
        if !route.contains(address) {
            route.push(address.clone());
        }
    }
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

async fn lookup_sources(
    scope: &IrohScopeV1,
    identity: TransportIdentity,
    pins: Pins,
    [first, second]: [Result<DialCandidate>; 2],
    lookups: Vec<Vec<u8>>,
    source_budget: Duration,
) -> Result<DiscoveryReport> {
    let failures = Arc::new(NetworkFailures::default());
    let lookup = |candidate, identity, lookups, failures| async move {
        let candidate: DialCandidate = candidate?;
        let source = fingerprint(&candidate.key);
        // A stalled source must not cancel a completed result from another source.
        let records = tokio::time::timeout(
            source_budget,
            query(scope, identity, candidate, pins, None, lookups, failures),
        )
        .await
        .with_context(|| format!("bootstrap {source} exceeded its lookup deadline"))?
        .with_context(|| format!("bootstrap {source}"))?;
        Ok::<_, anyhow::Error>((source, records))
    };
    let (primary, secondary) = tokio::join!(
        lookup(first, identity.clone(), lookups.clone(), failures.clone()),
        lookup(second, identity, lookups, failures),
    );
    let mut report = DiscoveryReport {
        sources: Vec::new(),
        records: BTreeMap::new(),
    };
    let mut errors = Vec::new();
    let wall = wall_ms()?;
    for result in [primary, secondary] {
        let (source, records) = match result {
            Ok(found) => found,
            Err(error) => {
                errors.push(format!("{error:#}"));
                continue;
            }
        };
        report.sources.push(source);
        for record in records
            .into_iter()
            .filter(|record| record.expires_at_ms > wall)
        {
            use std::collections::btree_map::Entry;
            match report.records.entry(record.relay_public_key.clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(record);
                }
                Entry::Occupied(mut entry) if entry.get().sequence < record.sequence => {
                    entry.insert(record);
                }
                Entry::Occupied(entry) => ensure!(
                    entry.get().sequence != record.sequence || entry.get() == &record,
                    "conflicting signed discovery records at the same sequence"
                ),
            }
        }
    }
    ensure!(
        !report.sources.is_empty(),
        "no authenticated NOVONET bootstrap: {}",
        errors.join("; ")
    );
    Ok(report)
}
