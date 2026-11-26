//! Durable, independently verified block-header progress for an installed client.
use super::super::super::query_carrier::{connect, QuerySession};
use super::*;
use crate::native_pipeline::{
    consensus::statement::finalized_header,
    service::client::operator::receipt_proof::{validate_parent, Download, FinalityVerifier},
};
use novovm_protocol::native_query::{Answer, Query};
mod follow;
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

fn sync_installed(path: &Path, with_state: bool) -> Result<serde_json::Value> {
    let (profile, identity, seeds) = installed_inputs(path)?;
    let (domain, anchor) = profile.verification_domain()?;
    validate_parent(&anchor)?;
    let mut chain = open_checkpoint(&profile, &identity, &domain, anchor)?;
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
                                let mut candidates =
                                    report.records.into_values().collect::<Vec<_>>();
                                use rand::seq::SliceRandom;
                                candidates.shuffle(&mut rand::thread_rng());
                                for candidate in candidates {
                                    if candidate.expires_at_ms <= wall_ms()?
                                        || Instant::now() >= deadline
                                    {
                                        continue;
                                    }
                                    let budget = Duration::from_secs(12)
                                        .min(deadline.saturating_duration_since(Instant::now()));
                                    let attempt = sync_peer(
                                        scope,
                                        &candidate,
                                        identity.clone(),
                                        domain.clone(),
                                        &mut chain,
                                        cap,
                                        cache.as_mut(),
                                    );
                                    match tokio::time::timeout(budget, attempt).await {
                                        Ok(Ok((peer, Some(state)))) => {
                                            return Ok((chain.tip, Some(peer), Some(state)));
                                        }
                                        Ok(Ok((peer, None))) => {
                                            best_advertised = best_advertised.max(Some(peer));
                                            if chain.tip.height == cap {
                                                break;
                                            }
                                        }
                                        Ok(Err(error)) => last = Some(error),
                                        Err(error) => last = Some(error.into()),
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
    let root = state_path(&installed_state_base()?, profile).join(format!(
        "headers-{}-{}",
        anchor.height,
        fingerprint(&serde_json::to_vec(&anchor)?),
    ));
    VerifiedHeaders::open(
        root,
        anchor,
        FinalityVerifier::new(domain, identity.clone()),
    )
}

async fn sync_peer(
    scope: &IrohScopeV1,
    candidate: &PeerSignedRelayRecordV1,
    identity: TransportIdentity,
    domain: MessageDomain,
    chain: &mut VerifiedHeaders,
    cap: u64,
    cache: Option<&mut StateCache>,
) -> Result<(u64, Option<StateProgress>)> {
    let (mut stream, channel) = connect(scope, candidate, identity, &domain).await?;
    let mut session = QuerySession::attach(&mut stream, channel, domain)?;
    let advertised = peer_status(&mut session, chain.tip).await?;
    sync_headers(&mut session, chain, cap, &advertised).await?;
    let state = if let Some(cache) = cache {
        ensure!(
            chain.tip.height == advertised.height,
            "pass verified headers only; rerun to continue state synchronization"
        );
        Some(cache.sync(&mut session, chain.tip).await?)
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
}

impl VerifiedHeaders {
    fn open(root: PathBuf, anchor: ParentPoint, verifier: FinalityVerifier) -> Result<Self> {
        fs::create_dir_all(&root)?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("writer.lock"))?;
        lock.try_lock()
            .context("another installed client owns the header checkpoint")?;
        let mut paths = BTreeMap::new();
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|suffix| suffix.to_str()) != Some("proof") {
                continue;
            }
            ensure!(
                entry.file_type()?.is_file(),
                "invalid header checkpoint entry"
            );
            let height: u64 = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .context("invalid header checkpoint name")?
                .parse()?;
            ensure!(
                paths.insert(height, path).is_none(),
                "duplicate header checkpoint height"
            );
        }
        let mut tip = anchor;
        for (height, path) in paths {
            ensure!(
                tip.height.checked_add(1) == Some(height),
                "header checkpoint has a height gap"
            );
            let mut bytes = Vec::new();
            fs::File::open(path)?
                .take((finalized_header::MAX_BYTES + 1) as u64)
                .read_to_end(&mut bytes)?;
            tip = verifier
                .verify_header(&bytes, &tip)
                .with_context(|| format!("verifying stored header at height {height}"))?;
        }
        Ok(Self {
            root,
            _lock: lock,
            verifier,
            tip,
        })
    }

    fn append(&mut self, bytes: &[u8]) -> Result<()> {
        let tip = self.verifier.verify_header(bytes, &self.tip)?;
        let final_path = self.root.join(format!("{:020}.proof", tip.height));
        ensure!(
            !final_path.try_exists()?,
            "header checkpoint already exists"
        );
        store_checkpoint(&final_path, bytes)?;
        self.tip = tip;
        Ok(())
    }
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
