use super::*;
use novovm_protocol::native_query::{QueryError, ReceiptState};

#[derive(Clone)]
pub(in super::super) struct ReceiptSearch {
    anchor: ParentPoint,
    minimum: Option<MinimumFinalizedCheckpoint>,
    root: PathBuf,
    pub(super) transaction: [u8; 32],
}

#[derive(Clone)]
pub(in super::super) struct PreparedReceipt {
    search: ReceiptSearch,
    height: u64,
    end: u64,
    // The cached prefix and its intersection with the requested receipt.
    prefix: ParentPoint,
    receipt_parent: Option<ParentPoint>,
    cached_receipt: Option<ParentPoint>,
}

impl ReceiptSearch {
    pub(in super::super) fn open(
        data: &Path,
        profile: &Profile,
        anchor: ParentPoint,
        transaction: [u8; 32],
    ) -> Result<Self> {
        ensure!(anchor.height < u64::MAX, "trusted parent height exhausted");
        Ok(Self {
            anchor,
            minimum: minimum_checkpoint(profile),
            root: checkpoint_root_in(data, profile, anchor)?,
            transaction,
        })
    }

    fn header_path(&self, height: u64) -> Result<Option<PathBuf>> {
        let path = self.root.join(format!("{height:020}.proof"));
        match fs::symlink_metadata(&path) {
            Ok(info) => {
                ensure!(
                    info.is_file(),
                    "invalid local header checkpoint at height {height}"
                );
                Ok(Some(path))
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    fn end_height(&self, height: u64) -> Result<u64> {
        ensure!(
            height > self.anchor.height,
            "receipt height must follow the trusted parent"
        );
        let end = height.max(self.minimum.map_or(0, |c| c.height));
        // The installed writer publishes a contiguous prefix. A missing path at
        // this boundary already means too much remote work, even if the peer
        // claimed u64::MAX. File existence alone grants no certified height.
        let needed = end.saturating_sub(MAX_HEADERS_PER_PASS + 1);
        ensure!(needed <= self.anchor.height || self.header_path(needed)?.is_some(),
            "receipt height is beyond the remote header budget; run sync-chain or follow-state before this lookup");
        Ok(end)
    }

    /// Runs before network proof budgets. Memory use is independent of history
    /// length: only retain the requested parent, requested point and prefix tip.
    pub(in super::super) fn prepare(
        &self,
        height: u64,
        verifier: &FinalityVerifier,
    ) -> Result<PreparedReceipt> {
        checkpoint_validity(self.minimum, wall_ms()?)?;
        let end = self.end_height(height)?;
        let mut prefix = self.anchor;
        let mut receipt_parent = None;
        let mut cached_receipt = None;
        while prefix.height < end {
            let next = prefix.height + 1;
            let Some(path) = self.header_path(next)? else {
                break;
            };
            let point = verify_cached_header(verifier, prefix, next, &path, self.minimum)?;
            if next == height {
                receipt_parent = Some(prefix);
                cached_receipt = Some(point);
            }
            prefix = point;
        }
        let missing = end - prefix.height - u64::from(height > prefix.height);
        ensure!(missing <= MAX_HEADERS_PER_PASS,
            "receipt needs {missing} uncached headers; run sync-chain or follow-state before this lookup");
        checkpoint_validity(self.minimum, wall_ms()?)?;
        Ok(PreparedReceipt {
            search: self.clone(),
            height,
            end,
            prefix,
            receipt_parent,
            cached_receipt,
        })
    }
}

impl PreparedReceipt {
    pub(super) async fn verify(
        &self,
        session: &mut QuerySession<'_>,
        domain: &MessageDomain,
        identity: TransportIdentity,
    ) -> Result<serde_json::Value> {
        let search = &self.search;
        checkpoint_validity(search.minimum, wall_ms()?)?;
        let verifier = FinalityVerifier::new(domain, identity.clone());
        let mut parent = self.receipt_parent.unwrap_or(self.prefix);
        if parent.height < self.height - 1 {
            parent = self
                .extend(session, &verifier, parent, self.height - 1)
                .await?;
        }
        let target = ProofTarget::new(domain, identity, parent, search.transaction)?;
        let (point, mut report) =
            target.verify_point(&session.fetch(Download::new(self.height)).await?)?;
        if let Some(expected) = self.cached_receipt {
            ensure!(
                expected == point,
                "receipt proof conflicts with certified local header at height {}",
                self.height
            );
        } else if let Some(path) = search.header_path(self.height)? {
            ensure!(
                verify_cached_header(&verifier, parent, self.height, &path, search.minimum)?
                    == point,
                "receipt proof conflicts with concurrently published header"
            );
        }
        if let Some(checkpoint) = search.minimum {
            checkpoint.check(point)?;
        }
        // A certified prefix extending past the receipt was already verified
        // through this exact point; otherwise build the missing suffix from it.
        let suffix = if self.cached_receipt.is_some() {
            self.prefix
        } else {
            point
        };
        self.extend(session, &verifier, suffix, self.end).await?;
        checkpoint_validity(search.minimum, wall_ms()?)?;
        report["verified_parent_height"] = parent.height.into();
        report["verified_parent_state_root"] =
            super::super::super::super::super::rpc::hex(&parent.state_root).into();
        Ok(report)
    }

    async fn extend(
        &self,
        session: &mut QuerySession<'_>,
        verifier: &FinalityVerifier,
        mut parent: ParentPoint,
        end: u64,
    ) -> Result<ParentPoint> {
        while parent.height < end {
            let height = parent.height + 1;
            parent = if let Some(path) = self.search.header_path(height)? {
                verify_cached_header(verifier, parent, height, &path, self.search.minimum)?
            } else {
                let bytes = session.fetch(Download::header(height)).await?;
                let point = verifier
                    .verify_header(&bytes, &parent)
                    .with_context(|| format!("verifying archive header at height {height}"))?
                    .point();
                if let Some(checkpoint) = self.search.minimum {
                    checkpoint.check(point)?;
                }
                point
            };
        }
        Ok(parent)
    }
}

pub(super) async fn lookup_height(
    session: &mut QuerySession<'_>,
    transaction: [u8; 32],
) -> Result<u64> {
    match session.query(Query::Receipt { hash: transaction }).await? {
        Answer::Receipt {
            state: ReceiptState::Finalized { height, .. },
            indexed_height,
        } if height > 0 && height <= indexed_height => Ok(height),
        Answer::Error(QueryError::Unavailable) => {
            anyhow::bail!("finalized receipt height unavailable")
        }
        Answer::Receipt { .. } => anyhow::bail!(
            "archive has no valid finalized height for this transaction; supply --height if known"
        ),
        _ => anyhow::bail!(
            "authenticated archive did not supply a receipt height; supply --height if known"
        ),
    }
}
impl ReceiptSearch {
    pub(in super::super) fn execute(
        &self,
        base: &Path,
        profile: &Profile,
        identity: &TransportIdentity,
        domain: &MessageDomain,
        location: ReceiptLocation,
        cache_verifier: Arc<FinalityVerifier>,
    ) -> Result<serde_json::Value> {
        let height = match location {
            ReceiptLocation::AnchorSuccessor => Some(self.anchor.height + 1),
            ReceiptLocation::Height(h) => Some(h),
            ReceiptLocation::QueryByHash => None,
        };
        let prepared = height
            .map(|h| self.prepare(h, &cache_verifier))
            .transpose()?;
        let carrier = IrohConfigV1::new(&profile.relays, profile.nameserver)?;
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let owner = bind_carrier(carrier).await?;
                let outcome = self
                    .discover(
                        prepared,
                        &owner,
                        base,
                        profile,
                        identity,
                        domain,
                        cache_verifier,
                    )
                    .await;
                let closed = owner.close().await;
                let proof = outcome?;
                closed?;
                Ok(proof)
            })
    }

    async fn discover(
        &self,
        fixed_height: Option<PreparedReceipt>,
        owner: &IrohEndpointOwnerV1,
        base: &Path,
        profile: &Profile,
        identity: &TransportIdentity,
        domain: &MessageDomain,
        cache_verifier: Arc<FinalityVerifier>,
    ) -> Result<serde_json::Value> {
        let mut remaining = Duration::from_secs(30);
        let per_read = Duration::from_secs(10);
        let mut prepared = BTreeMap::new();
        let mut hinted = BTreeMap::new();
        let mut errors = Vec::new();
        // Local authentication of a long cached prefix may outlast an earlier
        // locator. Rediscover once to obtain a fresh lease for the proof read.
        for _ in 0..2 {
            checkpoint_validity(self.minimum, wall_ms()?)?;
            let report =
                collect(owner, identity.clone(), profile, read_seeds(base, profile)).await?;
            let mut refresh = false;
            for candidate in order_installed_candidates(report.records.into_values(), profile) {
                if remaining.is_zero() {
                    break;
                }
                if candidate.expires_at_ms <= wall_ms()? {
                    refresh = true;
                    continue;
                }
                let result: Result<_> = async {
                    let plan = if let Some(plan) = &fixed_height {
                        plan.clone()
                    } else {
                        let key = &candidate.relay_public_key;
                        let height = if let Some(h) = hinted.get(key) {
                            *h
                        } else {
                            let started = Instant::now();
                            let transaction = self.transaction;
                            let result = read_route::query_archive(
                                owner,
                                &candidate,
                                identity.clone(),
                                domain.clone(),
                                profile,
                                per_read.min(remaining),
                                async move |session| lookup_height(session, transaction).await,
                            )
                            .await;
                            remaining = remaining.saturating_sub(started.elapsed());
                            let h = result?;
                            hinted.insert(key.clone(), h);
                            h
                        };
                        if let std::collections::btree_map::Entry::Vacant(entry) =
                            prepared.entry(height)
                        {
                            let search = self.clone();
                            let verifier = cache_verifier.clone();
                            let plan = tokio::task::spawn_blocking(move || {
                                search.prepare(height, &verifier)
                            })
                            .await??;
                            entry.insert(plan);
                        }
                        prepared[&height].clone()
                    };
                    if candidate.expires_at_ms <= wall_ms()? {
                        refresh = true;
                        anyhow::bail!("read locator expired during local header verification");
                    }
                    ensure!(
                        !remaining.is_zero(),
                        "receipt network query budget exhausted"
                    );
                    checkpoint_validity(self.minimum, wall_ms()?)?;
                    let started = Instant::now();
                    let verify_domain = domain.clone();
                    let verifier = identity.clone();
                    let result = read_route::query_archive(
                        owner,
                        &candidate,
                        identity.clone(),
                        domain.clone(),
                        profile,
                        per_read.min(remaining),
                        async move |session| plan.verify(session, &verify_domain, verifier).await,
                    )
                    .await;
                    remaining = remaining.saturating_sub(started.elapsed());
                    result
                }
                .await;
                match result {
                    Ok(mut verified) => {
                        checkpoint_validity(self.minimum, wall_ms()?)?;
                        verified["authenticated_discovery_sources"] =
                            serde_json::json!(report.sources);
                        verified["serving_transport_sha256"] =
                            fingerprint(&candidate.relay_public_key).into();
                        verified["transport_identity_sha256"] =
                            fingerprint(identity.public_key()).into();
                        return Ok(verified);
                    }
                    Err(error) => errors.push(format!("{error:#}")),
                }
            }
            if !refresh || remaining.is_zero() {
                break;
            }
        }
        ensure!(
            !errors.is_empty(),
            "no usable authenticated proof candidate"
        );
        anyhow::bail!(
            "no verified receipt from authenticated archives: {}",
            errors.join("; ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn historical_receipt_bounds_count_only_missing_non_target_headers() {
        let mut search = ReceiptSearch {
            anchor: ParentPoint {
                height: 7,
                block_hash: [1; 32],
                state_root: [2; 32],
                receipt_batch_commitment: [3; 32],
                state_version: 7,
                decision_hash: [4; 32],
            },
            minimum: None,
            root: std::env::temp_dir().join(format!(
                "novovm-receipt-missing-{:x}",
                rand::random::<u64>()
            )),
            transaction: [5; 32],
        };
        let max = search.anchor.height + MAX_HEADERS_PER_PASS + 1;
        assert!(search.end_height(7).is_err());
        assert!(search.end_height(u64::MAX).is_err());
        assert_eq!(search.end_height(max).unwrap(), max);
        assert!(search.end_height(max + 1).is_err());
        search.minimum = Some(MinimumFinalizedCheckpoint {
            height: max + 1,
            block_hash: [6; 32],
            valid_until_unix_ms: u64::MAX,
        });
        assert!(search.end_height(8).is_err());
    }
}
