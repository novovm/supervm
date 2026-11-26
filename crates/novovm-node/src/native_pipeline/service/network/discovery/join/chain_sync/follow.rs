//! Installed-device sync on the original query owner. A failed route never
//! discards a certified header or a verified content-addressed state node.
use super::*;
use novovm_network::discovery_control::native_chain_query_address;
use novovm_network::novorudp::endpoint::NativePeer;
mod locators;
use locators::Locators;

const POLL: Duration = Duration::from_secs(2);
const RECOVER_NATIVE: Duration = Duration::from_secs(6);
const QUERY_BUDGET: Duration = Duration::from_secs(20);

pub fn follow_installed_state(path: &Path, profile_sha256: Option<&str>) -> Result<()> {
    let (profile, domain, anchor) = installed_chain_profile(path, profile_sha256)?;
    let identity = local_identity(&installed_state_base()?, profile_base(path)?, &profile)?;
    let chain = open_checkpoint(&profile, &identity, &domain, anchor)?;
    let cache = StateCache::open(chain.root.join("state-nodes"))?;
    let locators = Locators::open(chain.root.join("peer-locators.json"), &profile, &identity)?;
    let follower = Follower {
        base: profile_base(path)?.to_owned(),
        profile,
        identity,
        domain,
        chain,
        cache,
        locators,
        native_cursor: 0,
        snapshot: None,
        reported: None,
        retrying: false,
        waiting_finality: None,
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(follower.run())
}

struct Follower {
    base: PathBuf,
    profile: Profile,
    identity: TransportIdentity,
    domain: MessageDomain,
    chain: VerifiedHeaders,
    cache: StateCache,
    locators: Locators,
    native_cursor: usize,
    snapshot: Option<(ParentPoint, StateProgress)>,
    reported: Option<(ParentPoint, Route)>,
    retrying: bool,
    waiting_finality: Option<PendingFinality>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Carrier {
    Native,
    Iroh,
}

#[derive(Clone, PartialEq, Eq)]
struct Route {
    carrier: Carrier,
    peer: String,
    native_address: Option<SocketAddr>,
    locator_sequence: Option<u64>,
}

type Native = (QuerySession<'static>, Route);

impl Follower {
    async fn run(mut self) -> Result<()> {
        self.chain.checkpoint_validity(wall_ms()?)?;
        self.emit(
            serde_json::json!({"event":"starting", "reopened_height":self.chain.tip.height}),
        )?;
        let mut owner = None;
        let mut backoff = Duration::from_secs(1);
        let mut native = self.probe_native(None).await;
        loop {
            let failed = if let Some((mut session, route)) = native.take() {
                let failed = route.clone();
                self.run_route(&mut session, route, None).await.ok();
                Some(failed)
            } else {
                None
            };
            self.chain.checkpoint_validity(wall_ms()?)?;
            if owner.is_none() {
                let carrier = IrohConfigV1::new(&self.profile.relays, self.profile.nameserver)?;
                owner = bind_carrier(carrier).await.ok();
            }
            if let Some(active) = &owner {
                let seeds = read_seeds(&self.base, &self.profile);
                if let Ok(report) =
                    collect(active, self.identity.clone(), &self.profile, seeds).await
                {
                    self.locators.observe(report)?;
                    if let Some(discovered_native) = self.probe_native(failed.as_ref()).await {
                        native = Some(discovered_native);
                        continue;
                    }
                }
                if !self.locators.records.is_empty() {
                    let (returned, recovered_native) = active
                        .with_continuous_service_v4(
                            || Ok(()),
                            |scope| {
                                Box::pin(async move {
                                    let native = self.follow_candidates(scope).await;
                                    Ok((self, native))
                                })
                            },
                        )
                        .await?;
                    self = returned;
                    if recovered_native.is_some() {
                        native = recovered_native;
                        backoff = Duration::from_secs(1);
                        continue;
                    }
                }
                // Invalidate stale pooled routes and nudge the retained owner
                // to inspect the actual local interfaces after a failed dial.
                let refresh =
                    IrohControlV1::new(Instant::now() + Duration::from_secs(2), || Ok(()))?;
                let _ = active.network_change(refresh).await;
            }
            self.chain.checkpoint_validity(wall_ms()?)?;
            if !self.retrying {
                self.emit(serde_json::json!({"event":"retrying", "verified_height":self.chain.tip.height}))?;
                self.retrying = true;
                self.reported = None;
                backoff = Duration::from_secs(1);
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(10));
            native = self.probe_native(None).await;
        }
    }

    async fn probe_native(&mut self, failed: Option<&Route>) -> Option<Native> {
        let now = wall_ms().ok()?;
        let mut options = Vec::new();
        for (key, record) in &self.locators.records {
            if let Ok(Some(address)) = native_chain_query_address(record) {
                if record.expires_at_ms > now {
                    options.push((
                        NativePeer {
                            key: key.clone(),
                            address,
                        },
                        Some(record.sequence),
                    ));
                }
            }
        }
        for peer in &self.profile.native_query_peers {
            if self
                .locators
                .records
                .get(&peer.transport_key)
                .is_some_and(|r| native_chain_query_address(r).ok().flatten().is_some())
            {
                continue;
            }
            options.push((
                NativePeer {
                    key: peer.transport_key.clone(),
                    address: peer.address,
                },
                None,
            ));
        }
        if options.is_empty() {
            return None;
        }
        let cursor = self.native_cursor % options.len();
        options.rotate_left(cursor);
        tokio::time::timeout(Duration::from_secs(4), async {
            for (node, locator_sequence) in options {
                self.native_cursor = self.native_cursor.wrapping_add(1);
                let native_address = Some(node.address);
                let peer = fingerprint(&node.key);
                if failed
                    .is_some_and(|old| old.native_address == native_address && old.peer == peer)
                {
                    continue;
                }
                let Ok(mut session) =
                    QuerySession::connect_native(self.identity.clone(), node, self.domain.clone())
                else {
                    continue;
                };
                let result = tokio::time::timeout(
                    Duration::from_millis(1250),
                    peer_status(&mut session, self.chain.tip),
                )
                .await;
                if matches!(result, Ok(Ok(_))) {
                    return Some((
                        session,
                        Route {
                            carrier: Carrier::Native,
                            peer,
                            native_address,
                            locator_sequence,
                        },
                    ));
                }
            }
            None
        })
        .await
        .ok()
        .flatten()
    }

    async fn follow_candidates(&mut self, scope: &IrohScopeV1) -> Option<Native> {
        let candidates = self.locators.records.values().cloned().collect::<Vec<_>>();
        for candidate in candidates {
            if wall_ms().map_or(true, |now| candidate.expires_at_ms <= now) {
                continue;
            }
            let Ok(Ok((mut stream, channel))) = tokio::time::timeout(
                Duration::from_secs(8),
                connect(scope, &candidate, self.identity.clone(), &self.domain),
            )
            .await
            else {
                continue;
            };
            let Ok(mut session) = QuerySession::attach(&mut stream, channel, self.domain.clone())
            else {
                continue;
            };
            let route = Route {
                carrier: Carrier::Iroh,
                peer: fingerprint(&candidate.relay_public_key),
                native_address: None,
                locator_sequence: Some(candidate.sequence),
            };
            if let Ok(Some(recovered_native)) =
                self.run_route(&mut session, route, Some(scope)).await
            {
                return Some(recovered_native);
            }
        }
        None
    }

    async fn run_route(
        &mut self,
        session: &mut QuerySession<'_>,
        route: Route,
        scope: Option<&IrohScopeV1>,
    ) -> Result<Option<Native>> {
        let mut next_probe = Instant::now() + RECOVER_NATIVE;
        loop {
            if let Some(state) = tokio::time::timeout(QUERY_BUDGET, self.sync(session)).await?? {
                let tip = self.chain.tip;
                if self.reported.as_ref() != Some(&(tip, route.clone())) {
                    self.emit(serde_json::json!({
                        "event":"verified_state", "height":tip.height,
                        "state_root": super::super::super::super::super::rpc::hex(&tip.state_root),
                        "state_version":tip.state_version,
                        "verified_nodes":state.nodes, "verified_leaves":state.leaves,
                        "downloaded_nodes":state.downloaded,
                        "transport":match route.carrier { Carrier::Native => "native_novorudp", Carrier::Iroh => "iroh" },
                        "peer_transport_sha256":route.peer,
                        "native_peer_address":route.native_address,
                        "locator_sequence":route.locator_sequence,
                    }))?;
                    self.reported = Some((tip, route.clone()));
                    self.retrying = false;
                }
            }
            if let Some(scope) = scope {
                if Instant::now() >= next_probe {
                    let installed = read_seeds(&self.base, &self.profile);
                    if let Ok(candidates) =
                        prepare_seed_candidates(&self.profile, &self.identity, installed)
                    {
                        if let Ok(report) = lookup_sources(
                            scope,
                            self.identity.clone(),
                            self.profile.pins(),
                            candidates,
                            self.profile.lookup_transport_keys.clone(),
                            REFRESH_SOURCE_BUDGET,
                        )
                        .await
                        {
                            self.locators.observe(report)?;
                        }
                    }
                    if let Some(native) = self.probe_native(None).await {
                        return Ok(Some(native));
                    }
                    next_probe = Instant::now() + RECOVER_NATIVE;
                }
            }
            tokio::time::sleep(POLL).await;
        }
    }

    async fn sync(&mut self, session: &mut QuerySession<'_>) -> Result<Option<StateProgress>> {
        let advertised = peer_status(session, self.chain.tip).await?;
        let cap = self.chain.tip.height.saturating_add(MAX_HEADERS_PER_PASS);
        sync_headers(session, &mut self.chain, cap, &advertised).await?;
        let tip = self.chain.tip;
        if tip.height != advertised.height {
            return Ok(None);
        }
        if let Err(reason) = self.chain.state_readiness(wall_ms()?) {
            if self.waiting_finality != Some(reason) {
                self.emit(serde_json::json!({"event":"waiting_for_required_finality", "verified_height":tip.height, "reason":reason}))?;
                self.waiting_finality = Some(reason);
                self.reported = None;
            }
            return Err(reason.into());
        }
        self.waiting_finality = None;
        if let Some((verified, state)) = self.snapshot.filter(|(verified, _)| *verified == tip) {
            debug_assert_eq!(verified, tip);
            return Ok(Some(state));
        }
        let state = self.cache.sync(session, tip).await?;
        self.chain.checkpoint_validity(wall_ms()?)?;
        self.snapshot = Some((tip, state));
        Ok(Some(state))
    }

    fn emit(&self, mut event: serde_json::Value) -> Result<()> {
        event["transport_identity_sha256"] = fingerprint(self.identity.public_key()).into();
        let mut stdout = std::io::stdout().lock();
        writeln!(stdout, "{event}")?;
        stdout.flush()?;
        Ok(())
    }
}
