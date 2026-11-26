//! Short public read leases on the existing authenticated discovery RPC.
use super::super::super::{query, DialCandidate, Exchange, Pins, PublicReadPublication};
use super::super::{
    fingerprint, prepare_seed_candidates, read_seeds, wall_ms, Profile, SeedCandidates,
};
use crate::native_pipeline::service::network::{publication_endpoint, NetworkFailures};
use anyhow::{ensure, Result};
use novovm_network::{
    discovery_control::{encode_public_read_locator_v1, public_read_locator_id_v1},
    iroh_transport::{approved_read_relay, IrohScopeV1},
    product_overlay::TransportIdentity,
    product_service_lease::IrohServiceEndpointV1,
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

// Match the resident publisher's signed lifetime. NVNETDS1 still keeps each
// directory entry for at most 30 seconds while new readers can use the record
// they already authenticated until the signed deadline.
const SIGNED_LEASE: Duration = Duration::from_secs(120);
const RENEW: Duration = Duration::from_secs(15);
const RETRY: Duration = Duration::from_secs(2);

pub(super) struct Publication {
    sources: [Vec<DialCandidate>; 2],
    relays: Vec<String>,
    checkpoint_expires: Option<u64>,
}

impl Publication {
    pub(super) fn new(
        profile: &Profile,
        base: &Path,
        identity: &TransportIdentity,
    ) -> Result<Self> {
        let sources = prepare_seed_candidates(profile, identity, read_seeds(base, profile))?
            .map(|candidate| eligible_routes(candidate.ok(), &profile.relays));
        ensure!(
            sources.iter().any(|routes| !routes.is_empty()),
            "public read serving requires an authenticated bootstrap over an approved relay"
        );
        Ok(Self {
            sources,
            relays: profile.relays.clone(),
            checkpoint_expires: profile
                .verification
                .as_ref()
                .and_then(|verification| verification.minimum_finalized_checkpoint)
                .map(|pin| pin.valid_until_unix_ms),
        })
    }

    pub(super) async fn run(
        &self,
        scope: &IrohScopeV1,
        identity: &TransportIdentity,
        pins: Pins,
        height: u64,
        deadline: Instant,
    ) -> Result<()> {
        let instance = rand::random();
        let failures = Arc::new(NetworkFailures::default());
        let mut acknowledgements = false;
        loop {
            let turn = Instant::now();
            let now = wall_ms()?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            let ttl = u64::try_from(remaining.min(SIGNED_LEASE).as_millis())?;
            let expires = now
                .checked_add(ttl)
                .unwrap_or(u64::MAX)
                .min(self.checkpoint_expires.unwrap_or(u64::MAX));
            if expires <= now {
                break;
            }
            // Publication never includes private direct routes or disconnected relays.
            let endpoint = publication_endpoint(
                IrohServiceEndpointV1 {
                    instance,
                    address: scope.connected_relay_address()?,
                },
                Some(&self.relays),
                None,
            )?;
            let mut accepted = 0;
            if let Some(endpoint) = endpoint {
                let record = super::sign_relay_record_v1(
                    identity,
                    public_read_locator_id_v1(pins),
                    vec![endpoint],
                    now,
                    expires,
                    now,
                )?;
                let lease = PublicReadPublication {
                    bytes: encode_public_read_locator_v1(&record, pins)?,
                    sequence: record.sequence,
                    expires_at_ms: expires,
                };
                let first = publish_to_source(
                    scope,
                    identity,
                    &self.sources[0],
                    pins,
                    lease.clone(),
                    failures.clone(),
                );
                let second = publish_to_source(
                    scope,
                    identity,
                    &self.sources[1],
                    pins,
                    lease,
                    failures.clone(),
                );
                let wall_limit = turn + Duration::from_millis(expires - now);
                let limit = deadline.min(wall_limit);
                if let Ok((first, second)) =
                    tokio::time::timeout_at(limit.into(), async { tokio::join!(first, second) })
                        .await
                {
                    accepted = usize::from(first) + usize::from(second);
                }
                if accepted != 0 {
                    acknowledgements = true;
                    println!(
                        "{}",
                        serde_json::json!({
                            "event":"public_read_lease_published", "height":height,
                            "transport_identity_sha256":fingerprint(identity.public_key()),
                            "sequence":record.sequence, "expires_at_ms":expires,
                            "acknowledged_sources":accepted, "scope":"relay_only",
                        })
                    );
                }
            }
            let wait = if accepted == 0 { RETRY } else { RENEW };
            let stop = deadline.min(
                turn + Duration::from_millis(
                    self.checkpoint_expires
                        .map_or(u64::MAX, |pin| pin.saturating_sub(now))
                        .min(wait.as_millis() as u64),
                ),
            );
            tokio::time::sleep_until(stop.into()).await;
        }
        ensure!(
            acknowledgements,
            "no authenticated bootstrap acknowledged a public read lease"
        );
        Ok(())
    }
}

fn eligible_routes(candidate: Option<SeedCandidates>, relays: &[String]) -> Vec<DialCandidate> {
    candidate
        .into_iter()
        .flat_map(|c| [Some(c.preferred), c.fallback])
        .flatten()
        .filter_map(|mut candidate| {
            candidate.route.direct.clear();
            candidate
                .route
                .relays
                .retain(|relay| approved_read_relay(relays, relay));
            (!candidate.route.relays.is_empty()).then_some(candidate)
        })
        .collect()
}

async fn publish_to_source(
    scope: &IrohScopeV1,
    identity: &TransportIdentity,
    routes: &[DialCandidate],
    pins: Pins,
    lease: PublicReadPublication,
    failures: Arc<NetworkFailures>,
) -> bool {
    for candidate in routes {
        if query(
            scope,
            identity.clone(),
            candidate.clone(),
            pins,
            Exchange::PublicReader(lease.clone()),
            Vec::new(),
            failures.clone(),
        )
        .await
        .is_ok()
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_pipeline::consensus::native::tests::separated_material;
    use crate::native_pipeline::service::network::{
        discovery::{
            join::{lookup_sources, InstalledLookups},
            serve,
        },
        DISCOVERY_ENTRY,
    };
    use novovm_network::{
        carrier_stream::CarrierStreamV1,
        iroh_transport::{IrohConfigV1, IrohEndpointOwnerV1},
        opaque_rendezvous::RendezvousStoreV1,
        product_directory::sign_relay_record_v1,
    };
    use std::sync::Mutex;

    #[test]
    fn public_read_lease_reaches_zero_pin_reader_over_authenticated_discovery() -> Result<()> {
        // This test only dials local discovery sockets. The signed public relay
        // address is checked by the reader policy but is never dialed.
        let relay = "https://relay.invalid/".to_owned();
        let (_, identities, domain, _) = separated_material()?;
        let pins = (domain.chain_id, domain.genesis, domain.protocol);
        let provider = identities[0].clone();
        let bootstrap = identities[1].clone();
        let consumer = identities[2].clone();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let local = || {
                    IrohConfigV1::new(&[], "127.0.0.1".parse()?)?
                        .with_bind_address("127.0.0.1:0".parse()?)
                };
                let boot = IrohEndpointOwnerV1::bind(local()?, || Ok(())).await?;
                let provider_owner = IrohEndpointOwnerV1::bind(local()?, || Ok(())).await?;
                let consumer_owner = IrohEndpointOwnerV1::bind(local()?, || Ok(())).await?;
                let (ready, contact) = tokio::sync::oneshot::channel();
                let bootstrap_key = bootstrap.public_key().to_vec();
                let service = boot.with_continuous_service_v4(
                    || Ok(()),
                    |boot_scope| {
                        Box::pin(async move {
                            ready
                                .send(boot_scope.address()?)
                                .map_err(|_| anyhow::anyhow!("contact reader stopped"))?;
                            let store = Arc::new(Mutex::new(RendezvousStoreV1::new(
                                [91; 32],
                                Instant::now() + Duration::from_secs(40),
                            )?));
                            for _ in 0..2 {
                                let mut stream: CarrierStreamV1<'_> =
                                    boot_scope.accept_inbound().await?.into();
                                ensure!(
                                    stream.read_frame().await? == DISCOVERY_ENTRY,
                                    "expected native discovery entry"
                                );
                                serve(stream, bootstrap.clone(), pins, store.clone()).await?;
                            }
                            Ok(())
                        })
                    },
                );
                let clients = async {
                    let candidate = DialCandidate {
                        key: bootstrap_key.clone(),
                        route: contact.await?,
                        source: "isolated_bootstrap",
                        sequence: 0,
                    };
                    let provider_key = provider.public_key().to_vec();
                    let publish_route = candidate.clone();
                    let publish_relay = relay.clone();
                    let signed = provider_owner
                        .with_continuous_service_v4(
                            || Ok(()),
                            |scope| {
                                Box::pin(async move {
                                    let mut address = scope.address()?;
                                    address.relays = vec![publish_relay.clone()];
                                    let endpoint = publication_endpoint(
                                        IrohServiceEndpointV1 {
                                            instance: [92; 32],
                                            address,
                                        },
                                        Some(std::slice::from_ref(&publish_relay)),
                                        None,
                                    )?
                                    .expect("approved route");
                                    let now = wall_ms()?;
                                    let signed = sign_relay_record_v1(
                                        &provider,
                                        public_read_locator_id_v1(pins),
                                        vec![endpoint],
                                        now,
                                        now + 30_000,
                                        now,
                                    )?;
                                    let lease = PublicReadPublication {
                                        bytes: encode_public_read_locator_v1(&signed, pins)?,
                                        sequence: signed.sequence,
                                        expires_at_ms: signed.expires_at_ms,
                                    };
                                    ensure!(
                                        publish_to_source(
                                            scope,
                                            &provider,
                                            std::slice::from_ref(&publish_route),
                                            pins,
                                            lease,
                                            Arc::new(NetworkFailures::default())
                                        )
                                        .await,
                                        "publication not acknowledged"
                                    );
                                    Ok(signed)
                                })
                            },
                        )
                        .await?;
                    let report = consumer_owner
                        .with_continuous_service_v4(
                            || Ok(()),
                            |scope| {
                                Box::pin(async move {
                                    let plan = InstalledLookups {
                                        pins,
                                        fixed: Vec::new(),
                                        approved_relays: vec![relay],
                                        voting_keys: Default::default(),
                                    };
                                    lookup_sources(
                                        scope,
                                        consumer,
                                        plan,
                                        [
                                            Ok(SeedCandidates {
                                                preferred: candidate,
                                                fallback: None,
                                            }),
                                            Err(anyhow::anyhow!("independent standby absent")),
                                        ],
                                        Duration::from_secs(6),
                                    )
                                    .await
                                })
                            },
                        )
                        .await?;
                    assert_eq!(report.records.get(&provider_key), Some(&signed));
                    assert_eq!(report.sources, [fingerprint(&bootstrap_key)]);
                    assert!(IrohServiceEndpointV1::from_record(&signed)?
                        .address
                        .direct
                        .is_empty());
                    Ok::<_, anyhow::Error>(())
                };
                let result = tokio::time::timeout(Duration::from_secs(20), async {
                    tokio::try_join!(service, clients)
                })
                .await;
                consumer_owner.close().await?;
                provider_owner.close().await?;
                boot.close().await?;
                result??;
                Ok(())
            })
    }

    #[test]
    #[ignore = "requires a previously approved relay profile and an isolated test domain"]
    fn approved_relay_bootstrap_fixture() -> Result<()> {
        use crate::native_pipeline::service::network::discovery::join::read_installed_profile;
        use novovm_relay::transport_identity::ephemeral_transport_identity;
        use std::{fs, path::PathBuf};

        let stage = PathBuf::from(
            std::env::var_os("NOVOVM_PUBLIC_READ_BOOTSTRAP_STAGE")
                .ok_or_else(|| anyhow::anyhow!("missing isolated bootstrap stage"))?,
        );
        let relay_path = stage.join("approved-relay.json");
        let relay_pin = "f7f759b55d876ace73f152c9571792926e4d27be5f1d024c778e48e85f44a4fc";
        let relay_bytes = fs::read(relay_path)?;
        ensure!(
            fingerprint(&relay_bytes) == relay_pin,
            "relay profile pin mismatch"
        );
        let approved: serde_json::Value = serde_json::from_slice(&relay_bytes)?;
        let relays: Vec<String> = serde_json::from_value(approved["relays"].clone())?;
        let nameserver = serde_json::from_value(approved["nameserver"].clone())?;
        let profile_path = stage.join("test-domain.json");
        let profile_sha = std::env::var("NOVOVM_PUBLIC_READ_TEST_DOMAIN_SHA256")?;
        let profile = read_installed_profile(&profile_path, Some(&profile_sha))?;
        ensure!(
            profile.pins().0 == 7251564206,
            "expected isolated height-42 test domain"
        );
        let pins = profile.pins();
        let (identity, _) =
            ephemeral_transport_identity(&profile.library, &profile.library_sha256)?;
        let config = IrohConfigV1::new(&relays, nameserver)?.with_relay_only()?;
        tokio::runtime::Builder::new_current_thread()
            .enable_all().build()?.block_on(async move {
                let owner = IrohEndpointOwnerV1::bind(config, || Ok(())).await?;
                let result = owner.with_relay_only_service_v1(async |scope| {
                    let start = Instant::now();
                    let address = loop {
                        let address = scope.connected_relay_address()?;
                        if !address.relays.is_empty() {
                            break address;
                        }
                        ensure!(start.elapsed() < Duration::from_secs(25), "approved relay did not connect");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    };
                    let until = Instant::now() + Duration::from_secs(110);
                    let store = Arc::new(Mutex::new(RendezvousStoreV1::new(rand::random(), until)?));
                    fs::write(stage.join("ready.tmp"), serde_json::to_vec(&serde_json::json!({
                        "contact": address, "transport_key": identity.public_key(),
                        "transport_sha256": fingerprint(identity.public_key()),
                    }))?)?;
                    fs::rename(stage.join("ready.tmp"), stage.join("ready.json"))?;
                    let serve_streams = async {
                        loop {
                            let mut stream: CarrierStreamV1<'_> = scope.accept_inbound().await?.into();
                            if matches!(tokio::time::timeout(Duration::from_secs(4), stream.read_frame()).await,
                                Ok(Ok(entry)) if entry == DISCOVERY_ENTRY) {
                                let _ = serve(stream, identity.clone(), pins, store.clone()).await;
                            }
                        }
                        #[allow(unreachable_code)]
                        Ok::<_, anyhow::Error>(())
                    };
                    let stop = async {
                        while !stage.join("stop").is_file() {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    };
                    tokio::select! {
                        _ = tokio::time::sleep_until(until.into()) => Ok(()),
                        _ = stop => Ok(()),
                        result = serve_streams => result,
                    }
                }).await;
                let closed = owner.close().await;
                result.and(closed)
            })
    }
}
