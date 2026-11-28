//! Installed-device sync on the original query owner. A failed route never
//! discards a certified header or a verified content-addressed state node.
use super::super::read_snapshot_service::{installed_public_read_stop, FollowedRead};
use super::*;
use novovm_network::discovery_control::native_chain_query_address;
use novovm_network::iroh_transport::IrohPathObserverV1;
use novovm_network::novorudp::endpoint::NativePeer;
use std::{collections::VecDeque, num::NonZeroUsize};
use tokio::sync::watch;
mod locators;
mod native_round;
#[cfg(test)]
mod successor_tests;
#[cfg(test)]
pub(in super::super) use locators::check_locator_role_transition;
use locators::Locators;
use native_round::NativeRound;

const POLL: Duration = Duration::from_secs(2);
const RECOVER_NATIVE: Duration = Duration::from_secs(6);
const IROH_SUCCESSOR_BUDGET: Duration = Duration::from_secs(10);
const FOLLOW_STACK_BYTES: usize = 8 * 1024 * 1024;

pub fn follow_installed_state(path: &Path, profile_sha256: Option<&str>) -> Result<()> {
    follow_on_stack(path, profile_sha256, None)
}

pub fn follow_and_serve_installed_state(
    path: &Path,
    profile_sha256: &str,
    max_state_nodes: NonZeroUsize,
    stop_file: Option<&Path>,
) -> Result<()> {
    follow_on_stack(
        path,
        Some(profile_sha256),
        Some(FollowedRead {
            profile_path: path,
            profile_pin: profile_sha256,
            max_state_nodes,
            stop_file,
        }),
    )
}

fn follow_on_stack(
    path: &Path,
    profile_sha256: Option<&str>,
    public: Option<FollowedRead<'_>>,
) -> Result<()> {
    // Discovery refresh runs while the state-query carrier is still on the
    // call stack. Reserve the worker stack explicitly, including on Windows.
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("novovm-follow-state".into())
            .stack_size(FOLLOW_STACK_BYTES)
            .spawn_scoped(scope, || follow_on_worker(path, profile_sha256, public))
            .context("start native state follower")?
            .join()
            .map_err(|_| anyhow::anyhow!("native state follower panicked"))?
    })
}

fn follow_on_worker(
    path: &Path,
    profile_sha256: Option<&str>,
    public: Option<FollowedRead<'_>>,
) -> Result<()> {
    let (profile, domain, anchor) = installed_chain_profile(path, profile_sha256)?;
    if let Some(public) = &public {
        if public.stopped_before_start(&profile)? {
            return Ok(());
        }
    }
    let data = installed_state_base()?;
    let identity = local_identity(&data, profile_base(path)?, &profile)?;
    let chain = open_checkpoint(&profile, &identity, &domain, anchor)?;
    let cache = StateCache::open(chain.root.join("state-nodes"))?;
    let locators = Locators::open(chain.root.join("peer-locators.json"), &profile, &identity)?;
    let (height, published) =
        watch::channel((chain.tip.height > anchor.height).then_some(chain.tip.height));
    let mut follower = Follower {
        base: profile_base(path)?.to_owned(),
        profile: Arc::new(profile),
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
        publication_height: None,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let Some(public) = public else {
        return runtime.block_on(boxed_read(|| follower.run()));
    };
    let identity = follower.identity.clone();
    let profile = follower.profile.clone();
    follower.publication_height = Some(height);
    let (shutdown, stop) = watch::channel(false);
    runtime.block_on(async {
        let publisher = boxed_read(|| public.run(identity, &profile, &data, published, stop));
        tokio::pin!(publisher);
        let result = tokio::select! {
            result = boxed_read(|| follower.run()) => result,
            result = installed_public_read_stop(public.stop_file) => result,
            result = &mut publisher => return result,
        };
        shutdown.send_replace(true);
        let stopped = publisher.await;
        result.and(stopped)
    })
}

struct Follower {
    base: PathBuf,
    profile: Arc<Profile>,
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
    publication_height: Option<watch::Sender<Option<u64>>>,
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

enum RouteChange {
    Native(Native),
    CertifiedIroh(PeerSignedRelayRecordV1),
}

impl Follower {
    async fn run(mut self) -> Result<()> {
        self.chain.checkpoint_validity(wall_ms()?)?;
        self.emit(
            serde_json::json!({"event":"starting", "reopened_height":self.chain.tip.height}),
        )?;
        let mut owner = None;
        let mut backoff = Duration::from_secs(1);
        let mut native_round = NativeRound::default();
        let mut native = self.probe_native(&native_round, false).await;
        loop {
            if let Some((mut session, route)) = native.take() {
                let failed_peer = route.peer.clone();
                let failed_address = route.native_address;
                boxed_read(|| self.run_route(&mut session, route, None, None))
                    .await
                    .ok();
                if let Some(address) = failed_address {
                    native_round.record_failure(failed_peer, address);
                }
            }
            self.chain.checkpoint_validity(wall_ms()?)?;
            if owner.is_none() {
                let carrier = self.profile.carrier_config()?;
                owner = bind_carrier(carrier).await.ok();
            }
            if let Some(active) = &owner {
                let seeds = read_seeds(&self.base, &self.profile);
                if let Ok(report) =
                    collect(active, self.identity.clone(), &self.profile, seeds).await
                {
                    self.observe_locators(report)?;
                    if let Some(discovered_native) = self.probe_native(&native_round, false).await {
                        native = Some(discovered_native);
                        continue;
                    }
                }
                if self
                    .locators
                    .candidates(&self.profile, wall_ms()?)
                    .next()
                    .is_some()
                {
                    let (returned, recovered_native) = active
                        .with_continuous_service_v4(
                            || Ok(()),
                            |scope| {
                                Box::pin(async move {
                                    let native = boxed_read(|| self.follow_candidates(scope)).await;
                                    Ok((self, native))
                                })
                            },
                        )
                        .await?;
                    self = returned;
                    if recovered_native.is_some() {
                        native_round = NativeRound::default();
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
            native_round = NativeRound::default();
            native = self.probe_native(&native_round, false).await;
        }
    }

    async fn probe_native(
        &mut self,
        round: &NativeRound,
        replacing_healthy_iroh: bool,
    ) -> Option<Native> {
        let now = wall_ms().ok()?;
        let mut options = Vec::new();
        for record in self.locators.fixed(&self.profile) {
            if let Ok(Some(address)) = native_chain_query_address(record) {
                if record.expires_at_ms > now {
                    options.push((
                        NativePeer {
                            key: record.relay_public_key.clone(),
                            address,
                        },
                        Some(record.sequence),
                    ));
                }
            }
        }
        for peer in &self.profile.native_query_peers {
            // Deduplicate the same socket. A still-valid locator for a
            // different socket leaves the installed address eligible.
            if self.locators.fixed(&self.profile).any(|record| {
                record.relay_public_key == peer.transport_key
                    && record.expires_at_ms > now
                    && native_chain_query_address(record).ok().flatten() == Some(peer.address)
            }) {
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
                if round.excludes(&peer, node.address) {
                    continue;
                }
                let Ok(mut session) =
                    QuerySession::connect_native(self.identity.clone(), node, self.domain.clone())
                else {
                    continue;
                };
                let budget = if replacing_healthy_iroh {
                    Duration::from_secs(2)
                } else {
                    Duration::from_millis(1250)
                };
                let result = tokio::time::timeout(budget, async {
                    let advertised = peer_status(&mut session, self.chain.tip).await?;
                    if replacing_healthy_iroh {
                        self.prove_native_recovery(&mut session, &advertised)
                            .await?;
                    }
                    Ok::<(), anyhow::Error>(())
                })
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

    async fn prove_native_recovery(
        &self,
        session: &mut QuerySession<'_>,
        advertised: &novovm_protocol::native_query::Status,
    ) -> Result<()> {
        let tip = self.chain.tip;
        self.chain.checkpoint_validity(wall_ms()?)?;
        if advertised.height == tip.height {
            // The local cache cannot establish that this authenticated route
            // serves any state. Request the committed root over that route.
            state::verify_remote_root(session, tip.height, tip.state_root).await?;
        } else {
            self.probe_successor(session, advertised).await?;
        }
        self.chain.checkpoint_validity(wall_ms()?)?;
        Ok(())
    }

    async fn probe_successor(
        &self,
        session: &mut QuerySession<'_>,
        advertised: &novovm_protocol::native_query::Status,
    ) -> Result<Vec<u8>> {
        let tip = self.chain.tip;
        ensure!(
            advertised.height > tip.height,
            "no successor on candidate route"
        );
        let next = tip
            .height
            .checked_add(1)
            .context("chain height exhausted")?;
        let proof = session.fetch(Download::header(next)).await?;
        let point = self.chain.verifier.verify_header(&proof, &tip)?.point();
        if let Some(minimum) = self.chain.minimum {
            minimum.check(point)?;
        }
        verify_same_height(point, advertised)?;
        // A peer can relay a valid certificate without serving any state.
        // Ask for the root it serves at its advertised tip, since snapshots
        // may serve only their newest height. For tips more than one block
        // ahead this is an availability check, not a finality decision.
        let certified_empty = point.height == advertised.height
            && point.state_root == crate::native_pipeline::state::tree::empty_root();
        if !certified_empty {
            state::verify_remote_root(session, advertised.height, advertised.state_root).await?;
        }
        Ok(proof)
    }

    async fn follow_candidates(&mut self, scope: &IrohScopeV1) -> Option<Native> {
        let mut candidates = VecDeque::from(order_installed_candidates(
            self.locators
                .candidates(&self.profile, wall_ms().ok()?)
                .cloned(),
            &self.profile,
        ));
        while let Some(candidate) = candidates.pop_front() {
            // An earlier session may have refreshed or withdrawn these coordinates.
            if !self
                .locators
                .candidates(&self.profile, wall_ms().ok()?)
                .any(|record| record == &candidate)
            {
                continue;
            }
            let Ok(route) = ReadRoute::new(&candidate, &self.profile) else {
                continue;
            };
            let result = route
                .run(scope, None, async |dial_scope, address| {
                    let (mut stream, channel) = tokio::time::timeout(
                        Duration::from_secs(8),
                        connect_route(
                            dial_scope,
                            &address,
                            &candidate.relay_public_key,
                            self.identity.clone(),
                            &self.domain,
                        ),
                    )
                    .await??;
                    let path = stream.iroh_path_observer();
                    let mut session =
                        QuerySession::attach(&mut stream, channel, self.domain.clone())?;
                    let route = Route {
                        carrier: Carrier::Iroh,
                        peer: fingerprint(&candidate.relay_public_key),
                        native_address: None,
                        locator_sequence: Some(candidate.sequence),
                    };
                    boxed_read(|| self.run_route(&mut session, route, Some(scope), path)).await
                })
                .await;
            match result {
                Ok(RouteChange::Native(native)) => return Some(native),
                Ok(RouteChange::CertifiedIroh(preferred)) => {
                    candidates
                        .retain(|record| record.relay_public_key != preferred.relay_public_key);
                    candidates.push_front(preferred);
                }
                Err(_) => {}
            }
        }
        None
    }

    async fn run_route(
        &mut self,
        session: &mut QuerySession<'_>,
        route: Route,
        scope: Option<&IrohScopeV1>,
        path: Option<IrohPathObserverV1>,
    ) -> Result<RouteChange> {
        let mut next_probe = Instant::now() + RECOVER_NATIVE;
        let mut alternate_cursor = 0;
        let mut observed_path = None;
        let mut route_checked = false;
        loop {
            let progress = self.sync(session).await?;
            if let Some(state) = progress {
                let tip = self.chain.tip;
                let selected = path.as_ref().and_then(IrohPathObserverV1::sample);
                let path_changed = selected != observed_path;
                let mut confirmed_path = None;
                if !route_checked || path_changed {
                    // A warm cache and an authenticated status do not show that
                    // this session actually serves the committed state.
                    let has_root =
                        tip.state_root != crate::native_pipeline::state::tree::empty_root();
                    if has_root {
                        state::verify_remote_root(session, tip.height, tip.state_root).await?;
                        self.chain.checkpoint_validity(wall_ms()?)?;
                    }
                    route_checked = true;
                    if selected == path.as_ref().and_then(IrohPathObserverV1::sample) {
                        confirmed_path = selected;
                    }
                    observed_path = None;
                }
                if self.reported.as_ref() != Some(&(tip, route.clone())) {
                    self.emit(serde_json::json!({
                        "event":"verified_state", "height":tip.height,
                        "block_hash": super::super::super::super::super::rpc::hex(&tip.block_hash),
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
                if let Some(selected) = confirmed_path {
                    self.emit(serde_json::json!({
                        "event": "verified_carrier_path",
                        "height": tip.height,
                        "peer_transport_sha256": route.peer,
                        "locator_sequence": route.locator_sequence,
                        "path": selected.path,
                    }))?;
                    observed_path = Some(selected);
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
                            InstalledLookups::new(&self.profile),
                            candidates,
                            INSTALLED_SOURCE_BUDGET,
                        )
                        .await
                        {
                            self.observe_locators(report)?;
                        }
                    }
                    // Finish verifying the current advertised tip before leaving a usable route.
                    if progress.is_some() {
                        if let Some(candidate) = boxed_read(|| {
                            self.probe_iroh_successor(scope, &route, &mut alternate_cursor)
                        })
                        .await?
                        {
                            self.emit(serde_json::json!({
                                "event": "certified_route_advance",
                                "height": self.chain.tip.height,
                                "from_peer_transport_sha256": route.peer,
                                "peer_transport_sha256": fingerprint(&candidate.relay_public_key),
                                "locator_sequence": candidate.sequence,
                            }))?;
                            return Ok(RouteChange::CertifiedIroh(candidate));
                        }
                        if let Some(native) = self.probe_native(&NativeRound::default(), true).await
                        {
                            return Ok(RouteChange::Native(native));
                        }
                    }
                    next_probe = Instant::now() + RECOVER_NATIVE;
                }
            }
            tokio::time::sleep(POLL).await;
        }
    }

    async fn probe_iroh_successor(
        &mut self,
        scope: &IrohScopeV1,
        current: &Route,
        cursor: &mut usize,
    ) -> Result<Option<PeerSignedRelayRecordV1>> {
        let candidates = self
            .locators
            .candidates(&self.profile, wall_ms()?)
            .filter(|record| fingerprint(&record.relay_public_key) != current.peer)
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Ok(None);
        }
        let candidate = candidates[*cursor % candidates.len()].clone();
        *cursor = cursor.wrapping_add(1);
        let Ok(route) = ReadRoute::new(&candidate, &self.profile) else {
            return Ok(None);
        };
        let proof = route
            .run(
                scope,
                Some(IROH_SUCCESSOR_BUDGET),
                async |dial_scope, address| {
                    let (mut stream, channel) = connect_route(
                        dial_scope,
                        &address,
                        &candidate.relay_public_key,
                        self.identity.clone(),
                        &self.domain,
                    )
                    .await?;
                    let mut session =
                        QuerySession::attach(&mut stream, channel, self.domain.clone())?;
                    let advertised = peer_status(&mut session, self.chain.tip).await?;
                    self.probe_successor(&mut session, &advertised).await
                },
            )
            .await;
        let Ok(proof) = proof else {
            return Ok(None);
        };
        // A failed availability probe leaves both the current route and the
        // persisted header checkpoint unchanged.
        self.chain.checkpoint_validity(wall_ms()?)?;
        if self.chain.append(&proof).is_err() {
            return Ok(None);
        }
        Ok(Some(candidate))
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
        if let Some(height) = &self.publication_height {
            height.send_replace(Some(tip.height));
        }
        Ok(Some(state))
    }

    fn observe_locators(&mut self, report: DiscoveryReport) -> Result<()> {
        for accepted in self
            .locators
            .observe(report, &self.profile, self.identity.public_key())?
        {
            self.emit(serde_json::json!({
                "event":"verified_locator_accepted",
                "peer_transport_sha256":accepted.peer_transport_sha256,
                "locator_sequence":accepted.sequence,
                "locator_issued_at_ms":accepted.issued_at_ms,
                "locator_expires_at_ms":accepted.expires_at_ms,
                "locator_returned_by":accepted.returned_by,
            }))?;
        }
        Ok(())
    }

    fn emit(&self, mut event: serde_json::Value) -> Result<()> {
        event["transport_identity_sha256"] = fingerprint(self.identity.public_key()).into();
        event["utc_unix_ms"] = wall_ms()?.into();
        let mut stdout = std::io::stdout().lock();
        writeln!(stdout, "{event}")?;
        stdout.flush()?;
        Ok(())
    }
}
