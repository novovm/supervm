//! First-run consumer of the same public NOVONET locator service as validators.
use super::*;
use crate::native_pipeline::{
    consensus::{
        native::{MessageDomain, ValidatorTransportBindings},
        statement::ParentPoint,
        wire::{Validator, ValidatorSet},
    },
    service::{
        client::operator::receipt_proof::{transaction_hash, ProofTarget},
        GenesisValidator,
    },
};
use novovm_network::iroh_transport::IrohControlV1;
use novovm_relay::transport_identity::{load_transport_identity, provision_transport_identity};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Write};

#[cfg(test)]
mod tests;

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
    /// Independently distributed public signed bootstrap locator files.
    seeds: [SeedPin; 2],
    /// Transport identity pins from the installed chain profile.
    lookup_transport_keys: Vec<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    verification: Option<VerificationPins>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct VerificationPins {
    validator_epoch: u64,
    validator_set_hash: [u8; 32],
    validators: Vec<GenesisValidator>,
    trusted_parent: ParentPoint,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SeedPin {
    locator: PathBuf,
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
        Ok(())
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
    let transaction = transaction_hash(hash)?;
    let (profile, identity, seeds) = installed_inputs(path)?;
    let pins = profile
        .verification
        .as_ref()
        .context("installed proof pins missing")?;
    ensure!(
        (1..=64).contains(&pins.validators.len()),
        "installed validator set size"
    );
    let validators = ValidatorSet::new(
        profile.chain_id,
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
        profile.lookup_transport_keys.iter().all(|key| pins
            .validators
            .iter()
            .any(|v| &v.transport_public_key == key)),
        "lookup key is outside the installed chain transport bindings"
    );
    let domain = MessageDomain {
        chain_id: profile.chain_id,
        genesis: profile.genesis,
        protocol: profile.protocol,
        validators: Arc::new(validators),
    };
    let target = ProofTarget::new(&domain, identity.clone(), pins.trusted_parent, transaction)?;
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

fn installed_inputs(
    path: &Path,
) -> Result<(Profile, TransportIdentity, [PeerSignedRelayRecordV1; 2])> {
    let data = installed_state_base()?;
    let profile: Profile = read_document(path)?;
    profile.validate()?;
    let base = path
        .parent()
        .context("installed profile directory missing")?;
    let seeds = [
        read_document(&base.join(&profile.seeds[0].locator))?,
        read_document(&base.join(&profile.seeds[1].locator))?,
    ];
    let identity = local_identity(&data, base, &profile)?;
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

fn local_identity(data: &Path, base: &Path, profile: &Profile) -> Result<TransportIdentity> {
    let root = data.join("NOVOVM").join("native-discovery").join(format!(
        "{}-{}-{}",
        profile.chain_id,
        super::super::super::rpc::hex(&profile.genesis),
        super::super::super::rpc::hex(&profile.protocol),
    ));
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
    seeds: [PeerSignedRelayRecordV1; 2],
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
    seeds: [PeerSignedRelayRecordV1; 2],
) -> Result<DiscoveryReport> {
    let pins = profile.pins();
    let wall = wall_ms()?;
    let [first, second] = seeds;
    ensure!(
        first.relay_public_key == profile.seeds[0].transport_key
            && second.relay_public_key == profile.seeds[1].transport_key,
        "bootstrap locator signer differs from installed pin"
    );
    let first = seed_candidate(first, pins, &identity, wall);
    let second = seed_candidate(second, pins, &identity, wall);
    let (first, second) = (first?, second?);
    ensure!(
        first.route.endpoint_key != second.route.endpoint_key,
        "installed discovery requires distinct source identities and endpoints"
    );
    ensure!(
        profile
            .lookup_transport_keys
            .iter()
            .all(|key| key != identity.public_key())
            && first.key != identity.public_key()
            && second.key != identity.public_key(),
        "ordinary device custody overlaps an installed service pin"
    );
    let lookups = profile.lookup_transport_keys.clone();
    owner
        .with_operation(
            IrohControlV1::new(Instant::now() + Duration::from_secs(10), || Ok(()))?,
            |scope| {
                Box::pin(lookup_sources(
                    scope,
                    identity,
                    pins,
                    [first, second],
                    lookups,
                ))
            },
        )
        .await
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
    [first, second]: [DialCandidate; 2],
    lookups: Vec<Vec<u8>>,
) -> Result<DiscoveryReport> {
    let sources = [fingerprint(&first.key), fingerprint(&second.key)];
    let failures = Arc::new(NetworkFailures::default());
    let (primary, secondary) = tokio::join!(
        query(
            scope,
            identity.clone(),
            first,
            pins,
            None,
            lookups.clone(),
            failures.clone()
        ),
        query(scope, identity, second, pins, None, lookups, failures),
    );
    let mut report = DiscoveryReport {
        sources: Vec::new(),
        records: BTreeMap::new(),
    };
    let mut errors = Vec::new();
    let wall = wall_ms()?;
    for (source, result) in sources.into_iter().zip([primary, secondary]) {
        let records = match result {
            Ok(records) => records,
            Err(error) => {
                errors.push(format!("{source}: {error:#}"));
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
