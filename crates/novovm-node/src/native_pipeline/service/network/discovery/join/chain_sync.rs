//! Durable, independently verified block-header progress for an installed client.
use super::*;
use crate::native_pipeline::{
    consensus::statement::finalized_header,
    service::client::operator::receipt_proof::{validate_parent, Download, FinalityVerifier},
};
use novovm_protocol::native_query::{Answer, Query};

const MAX_HEADERS_PER_PASS: u64 = 128;

pub fn sync_installed_chain(path: &Path) -> Result<serde_json::Value> {
    let (profile, identity, seeds) = installed_inputs(path)?;
    let (domain, anchor) = profile.verification_domain()?;
    validate_parent(&anchor)?;
    let root = state_path(&installed_state_base()?, &profile).join(format!(
        "headers-{}-{}",
        anchor.height,
        fingerprint(&serde_json::to_vec(&anchor)?),
    ));
    let verifier = FinalityVerifier::new(&domain, identity.clone());
    let mut chain = VerifiedHeaders::open(root, anchor, verifier)?;
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
                                    );
                                    match tokio::time::timeout(budget, attempt).await {
                                        Ok(Ok(peer)) => {
                                            best_advertised = best_advertised.max(Some(peer));
                                            if chain.tip.height == cap {
                                                break;
                                            }
                                        }
                                        Ok(Err(error)) => last = Some(error),
                                        Err(error) => last = Some(error.into()),
                                    }
                                }
                                if best_advertised.is_some() || chain.tip.height > reopened_height {
                                    return Ok((
                                        chain.tip,
                                        best_advertised
                                            .filter(|height| *height >= chain.tip.height),
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
    let (tip, peer) = progress;
    Ok(serde_json::json!({
        "reopened_height": reopened_height,
        "verified_height": tip.height,
        "verified_block_hash": super::super::super::super::rpc::hex(&tip.block_hash),
        "certified_state_root": super::super::super::super::rpc::hex(&tip.state_root),
        "verified_state_version": tip.state_version,
        "peer_advertised_height": peer,
        "reached_peer_advertised_height": peer == Some(tip.height),
    }))
}

async fn sync_peer(
    scope: &IrohScopeV1,
    candidate: &PeerSignedRelayRecordV1,
    identity: TransportIdentity,
    domain: MessageDomain,
    chain: &mut VerifiedHeaders,
    cap: u64,
) -> Result<u64> {
    use super::super::super::query_carrier::{connect, QuerySession};
    let (mut stream, channel) = connect(scope, candidate, identity, &domain).await?;
    let mut session = QuerySession::attach(&mut stream, channel, domain)?;
    let Answer::Status(advertised) = session.query(Query::Status).await? else {
        anyhow::bail!("chain candidate returned no status");
    };
    ensure!(
        !advertised.recovering && !advertised.projection_failed,
        "chain candidate unavailable"
    );
    ensure!(
        advertised.height >= chain.tip.height,
        "chain candidate is behind verified local height"
    );
    while chain.tip.height < advertised.height.min(cap) {
        let height = chain.tip.height + 1;
        let bytes = session.fetch(Download::header(height)).await?;
        chain.append(&bytes)?;
    }
    if chain.tip.height == advertised.height {
        ensure!(
            chain.tip.block_hash == advertised.block_hash
                && chain.tip.state_root == advertised.state_root,
            "peer advertised tip conflicts with certified header"
        );
    }
    Ok(advertised.height)
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
        let pending = final_path.with_extension(format!("{:016x}.pending", rand::random::<u64>()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&pending, &final_path)?;
        #[cfg(unix)]
        fs::File::open(&self.root)?.sync_all()?;
        self.tip = tip;
        Ok(())
    }
}
