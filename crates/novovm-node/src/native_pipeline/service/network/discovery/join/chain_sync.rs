//! Durable, independently verified block-header progress for an installed client.
use super::super::super::query_carrier::{connect_route, query_deadline, QuerySession};
use super::*;
use crate::native_pipeline::{
    consensus::statement::finalized_header,
    service::client::operator::receipt_proof::{validate_parent, Download, FinalityVerifier},
};
use novovm_protocol::native_query::{Answer, Query};
mod follow;
#[cfg(test)]
pub(super) use follow::check_locator_role_transition;
mod read_route;
mod receipt;
pub(super) use novovm_network::iroh_transport::approved_read_relay;
pub(super) use read_route::order_installed_candidates;
use read_route::{boxed_read, ReadRoute};
pub(super) use receipt::ReceiptSearch;
pub(crate) mod read_snapshot;
mod state;
pub use follow::follow_installed_state;
use state::{StateCache, StateProgress};

const MAX_HEADERS_PER_PASS: u64 = 128;

pub fn sync_installed_chain(path: &Path) -> Result<serde_json::Value> {
    sync_installed(path, false)
}

pub fn sync_installed_state(path: &Path) -> Result<serde_json::Value> {
    sync_installed(path, true)
}

pub fn verify_installed_uca_execution(
    path: &Path,
    profile_sha256: &str,
    archive: &Path,
) -> Result<serde_json::Value> {
    use crate::native_pipeline::proof::uca_replay::{read_bundle, ReplayBundle};
    use novovm_relay::transport_identity::load_transport_verifier;
    let profile = read_installed_profile(path, Some(profile_sha256))?;
    let (domain, anchor) = profile.verification_domain()?;
    validate_parent(&anchor)?;
    let minimum = minimum_checkpoint(&profile);
    checkpoint_validity(minimum, wall_ms()?)?;
    let bytes = read_bundle(archive)?;
    let bundle = ReplayBundle::decode(&bytes)?;
    let parent_height = bundle
        .height()
        .checked_sub(1)
        .context("UCA replay requires a successor block")?;
    let library = profile_base(path)?.join(&profile.library);
    let verifier = FinalityVerifier::new(
        &domain,
        load_transport_verifier(&library, &profile.library_sha256)?,
    );
    let root = checkpoint_root(&profile, anchor)?;
    // The writer publishes immutable headers by rename, so readers can verify
    // a committed prefix while follow-state holds the writer lock.
    let history = cached_header_paths(&root)?;
    let mut parent = anchor;
    for (height, path) in history.range(..=parent_height) {
        parent = verify_cached_header(&verifier, parent, *height, path, minimum)?;
    }
    ensure!(
        parent.height == parent_height,
        "certified parent height {parent_height} is not present in the installed checkpoint history"
    );
    let header = verifier.verify_header(bundle.header(), &parent)?;
    let point = header.point();
    if let Some(minimum) = minimum {
        minimum.check(point)?;
    }
    if let Some(path) = history.get(&point.height) {
        ensure!(
            point == verify_cached_header(&verifier, parent, point.height, path, minimum)?,
            "UCA replay archive conflicts with the certified local chain"
        );
    }
    if let Some(checkpoint) = minimum.filter(|checkpoint| checkpoint.height > point.height) {
        let mut descendant = point;
        for (&height, path) in history.range((point.height + 1)..=checkpoint.height) {
            descendant = verify_cached_header(&verifier, descendant, height, path, minimum)?;
        }
        ensure!(
            descendant.height == checkpoint.height,
            "UCA replay requires certified headers through the installed minimum checkpoint at height {}; run sync-chain first",
            checkpoint.height
        );
    }
    bundle.verify(&header, &library, &profile.library_sha256)?;
    checkpoint_validity(minimum, wall_ms()?)?;
    use crate::native_pipeline::service::rpc::hex;
    Ok(serde_json::json!({
        "verification":"independent_uca_execution_replay",
        "height":point.height, "parent_height":parent.height,
        "block_hash":hex(&point.block_hash), "state_root":hex(&point.state_root),
        "receipt_batch_commitment":hex(&point.receipt_batch_commitment), "state_version":point.state_version,
    }))
}

/// Re-verify the locally cached original prefix before exporting a checkpoint.
pub fn export_activation_parent(
    path: &Path,
    profile_sha256: &str,
    height: u64,
    output: &Path,
) -> Result<serde_json::Value> {
    use novovm_relay::transport_identity::load_transport_verifier;
    use sha2::{Digest, Sha256};
    use std::io::Write;

    let profile = read_installed_profile(path, Some(profile_sha256))?;
    let (domain, anchor) = profile.verification_domain()?;
    validate_parent(&anchor)?;
    ensure!(
        height >= anchor.height,
        "activation parent precedes the installed trust anchor"
    );
    let minimum = profile
        .verification
        .as_ref()
        .and_then(|pins| pins.minimum_finalized_checkpoint);
    checkpoint_validity(minimum, wall_ms()?)?;
    let mut point = anchor;
    if height > anchor.height {
        let library = profile_base(path)?.join(&profile.library);
        let verifier = FinalityVerifier::new(
            &domain,
            load_transport_verifier(&library, &profile.library_sha256)?,
        );
        let root = checkpoint_root(&profile, anchor)?;
        for (height, header) in cached_header_paths(&root)?.range(..=height) {
            point = verify_cached_header(&verifier, point, *height, header, minimum)?;
        }
    }
    ensure!(
        point.height == height,
        "activation parent missing from the verified local prefix"
    );
    checkpoint_validity(minimum, wall_ms()?)?;
    let bytes = serde_json::to_vec(&point)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(serde_json::json!({
        "kind":"pinned_native_chain_checkpoint",
        "height":point.height,
        "sha256":format!("{:x}", Sha256::digest(&bytes)),
    }))
}

fn sync_installed(path: &Path, with_state: bool) -> Result<serde_json::Value> {
    let profile = read_installed_profile(path, None)?;
    let (domain, anchor) = profile.verification_domain()?;
    validate_parent(&anchor)?;
    if with_state {
        check_profile_checkpoint_deadline(&profile)?;
    }
    let base = profile_base(path)?;
    let identity = local_identity(&installed_state_base()?, base, &profile)?;
    let seeds = read_seeds(base, &profile);
    let mut chain = open_checkpoint(&profile, &identity, &domain, anchor)?;
    if with_state {
        chain.checkpoint_validity(wall_ms()?)?;
    }
    let mut cache = with_state
        .then(|| StateCache::open(chain.root.join("state-nodes")))
        .transpose()?;
    let reopened_height = chain.tip.height;
    let carrier = IrohConfigV1::new(&profile.relays, profile.nameserver)?;
    let progress = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let owner = bind_carrier(carrier).await?;
            let result = async {
                let report = collect(&owner, identity.clone(), &profile, seeds).await?;
                owner
                    .with_continuous_service_v4(
                        || Ok(()),
                        |scope| {
                            Box::pin(async move {
                                let deadline = Instant::now() + Duration::from_secs(30);
                                let cap = reopened_height.saturating_add(MAX_HEADERS_PER_PASS);
                                let mut last = None;
                                let mut best_advertised = None;
                                let candidates = order_installed_candidates(
                                    report.records.into_values(),
                                    &profile,
                                );
                                for candidate in candidates {
                                    if candidate.expires_at_ms <= wall_ms()?
                                        || Instant::now() >= deadline
                                    {
                                        continue;
                                    }
                                    let budget = Duration::from_secs(12)
                                        .min(deadline.saturating_duration_since(Instant::now()));
                                    let route = match ReadRoute::new(&candidate, &profile) {
                                        Ok(route) => route,
                                        Err(error) => {
                                            last = Some(error);
                                            continue;
                                        }
                                    };
                                    let attempt = route
                                        .run(scope, Some(budget), async |scope, address| {
                                            sync_peer(
                                                scope,
                                                &address,
                                                &candidate,
                                                identity.clone(),
                                                domain.clone(),
                                                &mut chain,
                                                cap,
                                                cache.as_mut(),
                                            )
                                            .await
                                        })
                                        .await;
                                    match attempt {
                                        Ok((peer, Some(state))) => {
                                            return Ok((chain.tip, Some(peer), Some(state)));
                                        }
                                        Ok((peer, None)) => {
                                            best_advertised = best_advertised.max(Some(peer));
                                            if chain.tip.height == cap {
                                                break;
                                            }
                                        }
                                        Err(error) => last = Some(error),
                                    }
                                }
                                if !with_state
                                    && (best_advertised.is_some()
                                        || chain.tip.height > reopened_height)
                                {
                                    return Ok((
                                        chain.tip,
                                        best_advertised
                                            .filter(|height| *height >= chain.tip.height),
                                        None,
                                    ));
                                }
                                Err(last.unwrap_or_else(|| {
                                    anyhow::anyhow!("no usable authenticated chain candidate")
                                }))
                            })
                        },
                    )
                    .await
            }
            .await;
            let closed = owner.close().await;
            let progress = result?;
            closed?;
            Ok::<_, anyhow::Error>(progress)
        })?;
    let (tip, peer, state) = progress;
    let mut report = serde_json::json!({
        "reopened_height": reopened_height,
        "verified_height": tip.height,
        "verified_block_hash": super::super::super::super::rpc::hex(&tip.block_hash),
        "certified_state_root": super::super::super::super::rpc::hex(&tip.state_root),
        "verified_state_version": tip.state_version,
        "peer_advertised_height": peer,
        "reached_peer_advertised_height": peer == Some(tip.height),
    });
    if let Some(state) = state {
        report["verified_state_nodes"] = state.nodes.into();
        report["verified_state_leaves"] = state.leaves.into();
        report["downloaded_state_nodes"] = state.downloaded.into();
    }
    Ok(report)
}

fn open_checkpoint(
    profile: &Profile,
    identity: &TransportIdentity,
    domain: &MessageDomain,
    anchor: ParentPoint,
) -> Result<VerifiedHeaders> {
    VerifiedHeaders::open(
        checkpoint_root(profile, anchor)?,
        anchor,
        FinalityVerifier::new(domain, identity.clone()),
        profile
            .verification
            .as_ref()
            .context("installed verification pins missing")?,
    )
}

fn checkpoint_root(profile: &Profile, anchor: ParentPoint) -> Result<PathBuf> {
    checkpoint_root_in(&installed_state_base()?, profile, anchor)
}

fn checkpoint_root_in(data: &Path, profile: &Profile, anchor: ParentPoint) -> Result<PathBuf> {
    Ok(state_path(data, profile).join(format!(
        "headers-{}-{}",
        anchor.height,
        fingerprint(&serde_json::to_vec(&anchor)?),
    )))
}

async fn sync_peer(
    scope: &IrohScopeV1,
    route: &IrohAddressV1,
    candidate: &PeerSignedRelayRecordV1,
    identity: TransportIdentity,
    domain: MessageDomain,
    chain: &mut VerifiedHeaders,
    cap: u64,
    cache: Option<&mut StateCache>,
) -> Result<(u64, Option<StateProgress>)> {
    let (mut stream, channel) =
        connect_route(scope, route, &candidate.relay_public_key, identity, &domain).await?;
    let mut session = QuerySession::attach(&mut stream, channel, domain)?;
    let advertised = peer_status(&mut session, chain.tip).await?;
    sync_headers(&mut session, chain, cap, &advertised).await?;
    let state = if let Some(cache) = cache {
        chain.state_readiness(wall_ms()?)?;
        ensure!(
            chain.tip.height == advertised.height,
            "pass verified headers only; rerun to continue state synchronization"
        );
        let progress = cache.sync(&mut session, chain.tip).await?;
        chain.state_readiness(wall_ms()?)?;
        Some(progress)
    } else {
        None
    };
    Ok((advertised.height, state))
}

async fn peer_status(
    session: &mut QuerySession<'_>,
    tip: ParentPoint,
) -> Result<novovm_protocol::native_query::Status> {
    let Answer::Status(advertised) = session.query(Query::Status).await? else {
        anyhow::bail!("chain candidate returned no status");
    };
    ensure!(
        !advertised.recovering && !advertised.projection_failed,
        "chain candidate unavailable"
    );
    ensure!(
        advertised.height >= tip.height,
        "chain candidate is behind verified local height"
    );
    verify_same_height(tip, &advertised)?;
    Ok(advertised)
}

async fn sync_headers(
    session: &mut QuerySession<'_>,
    chain: &mut VerifiedHeaders,
    cap: u64,
    advertised: &novovm_protocol::native_query::Status,
) -> Result<()> {
    while chain.tip.height < advertised.height.min(cap) {
        let height = chain.tip.height + 1;
        let bytes = session.fetch(Download::header(height)).await?;
        chain.append(&bytes)?;
    }
    verify_same_height(chain.tip, advertised)
}

fn verify_same_height(
    tip: ParentPoint,
    advertised: &novovm_protocol::native_query::Status,
) -> Result<()> {
    if tip.height == advertised.height {
        ensure!(
            tip.block_hash == advertised.block_hash && tip.state_root == advertised.state_root,
            "peer advertised tip conflicts with certified header"
        );
    }
    Ok(())
}

struct VerifiedHeaders {
    root: PathBuf,
    _lock: fs::File,
    verifier: FinalityVerifier,
    tip: ParentPoint,
    minimum: Option<MinimumFinalizedCheckpoint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PendingFinality {
    Checkpoint {
        required_height: u64,
        verified_height: u64,
    },
    ExpiredCheckpoint {
        valid_until_unix_ms: u64,
    },
}

impl std::fmt::Display for PendingFinality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Checkpoint { required_height, verified_height } => write!(f,
                "waiting for installed checkpoint at height {required_height}, verified height {verified_height}"),
            Self::ExpiredCheckpoint { valid_until_unix_ms } => write!(f,
                "installed checkpoint policy expired at UTC timestamp {valid_until_unix_ms}"),
        }
    }
}
impl std::error::Error for PendingFinality {}

impl VerifiedHeaders {
    fn open(
        root: PathBuf,
        anchor: ParentPoint,
        verifier: FinalityVerifier,
        pins: &VerificationPins,
    ) -> Result<Self> {
        fs::create_dir_all(&root)?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("writer.lock"))?;
        lock.try_lock()
            .context("another installed client owns the header checkpoint")?;
        let mut tip = anchor;
        for (height, path) in cached_header_paths(&root)? {
            tip = verify_cached_header(
                &verifier,
                tip,
                height,
                &path,
                pins.minimum_finalized_checkpoint,
            )?;
        }
        Ok(Self {
            root,
            _lock: lock,
            verifier,
            tip,
            minimum: pins.minimum_finalized_checkpoint,
        })
    }

    fn append(&mut self, bytes: &[u8]) -> Result<()> {
        let header = self.verifier.verify_header(bytes, &self.tip)?;
        let tip = header.point();
        if let Some(minimum) = self.minimum {
            minimum.check(tip)?;
        }
        let final_path = self.root.join(format!("{:020}.proof", tip.height));
        ensure!(
            !final_path.try_exists()?,
            "header checkpoint already exists"
        );
        store_checkpoint(&final_path, bytes)?;
        self.tip = tip;
        Ok(())
    }

    fn state_readiness(&self, now: u64) -> std::result::Result<(), PendingFinality> {
        self.checkpoint_validity(now)?;
        if let Some(minimum) = self
            .minimum
            .filter(|minimum| self.tip.height < minimum.height)
        {
            return Err(PendingFinality::Checkpoint {
                required_height: minimum.height,
                verified_height: self.tip.height,
            });
        }
        Ok(())
    }

    fn checkpoint_validity(&self, now: u64) -> std::result::Result<(), PendingFinality> {
        checkpoint_validity(self.minimum, now)
    }
}

fn minimum_checkpoint(profile: &Profile) -> Option<MinimumFinalizedCheckpoint> {
    profile.verification.as_ref()?.minimum_finalized_checkpoint
}

pub(super) fn check_profile_checkpoint_deadline(profile: &Profile) -> Result<()> {
    checkpoint_validity(minimum_checkpoint(profile), wall_ms()?)?;
    Ok(())
}

fn checkpoint_validity(
    minimum: Option<MinimumFinalizedCheckpoint>,
    now: u64,
) -> std::result::Result<(), PendingFinality> {
    if let Some(minimum) = minimum.filter(|minimum| now >= minimum.valid_until_unix_ms) {
        return Err(PendingFinality::ExpiredCheckpoint {
            valid_until_unix_ms: minimum.valid_until_unix_ms,
        });
    }
    Ok(())
}

fn cached_header_paths(root: &Path) -> Result<BTreeMap<u64, PathBuf>> {
    let mut paths = BTreeMap::new();
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(paths),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|suffix| suffix.to_str()) != Some("proof") {
            continue;
        }
        ensure!(
            entry.file_type()?.is_file(),
            "invalid header checkpoint entry"
        );
        let height = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .context("invalid header checkpoint name")?
            .parse()?;
        ensure!(
            paths.insert(height, path).is_none(),
            "duplicate header checkpoint height"
        );
    }
    Ok(paths)
}

fn verify_cached_header(
    verifier: &FinalityVerifier,
    parent: ParentPoint,
    height: u64,
    path: &Path,
    minimum: Option<MinimumFinalizedCheckpoint>,
) -> Result<ParentPoint> {
    ensure!(
        parent.height.checked_add(1) == Some(height),
        "header checkpoint has a height gap"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take((finalized_header::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    let point = verifier
        .verify_header(&bytes, &parent)
        .with_context(|| format!("verifying stored header at height {height}"))?
        .point();
    if let Some(minimum) = minimum {
        minimum.check(point)?;
    }
    Ok(point)
}

fn store_checkpoint(path: &Path, bytes: &[u8]) -> Result<()> {
    let pending = path.with_extension(format!("{:016x}.pending", rand::random::<u64>()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&pending, path)?;
    #[cfg(unix)]
    fs::File::open(path.parent().context("checkpoint directory missing")?)?.sync_all()?;
    Ok(())
}
