//! Short public read leases on the existing authenticated discovery RPC.
use super::super::super::{query, DialCandidate, Exchange, Pins, PublicReadPublication};
use super::super::{
    fingerprint, prepare_seed_candidates, read_seeds, seed_candidate, wall_ms, Profile,
    SeedCandidates,
};
use crate::native_pipeline::service::network::{publication_endpoint, NetworkFailures};
use anyhow::{ensure, Result};
use novovm_network::{
    discovery_control::{encode_public_read_locator_v1, public_read_locator_id_v1},
    iroh_transport::{approved_read_relay, IrohScopeV1},
    product_directory::PeerSignedRelayRecordV1,
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

pub(super) struct Publication<'a> {
    base: &'a Path,
    profile: &'a Profile,
    resident: bool,
    checkpoint_expires: Option<u64>,
}

impl<'a> Publication<'a> {
    pub(super) fn new(
        profile: &'a Profile,
        base: &'a Path,
        identity: &TransportIdentity,
        resident: bool,
    ) -> Result<Self> {
        let sources = prepare_seed_candidates(profile, identity, read_seeds(base, profile))?;
        ensure!(
            resident
                || sources.into_iter().any(|candidate| !eligible_routes(
                    candidate.ok(),
                    &profile.relays
                )
                .is_empty()),
            "public read serving requires an authenticated bootstrap over an approved relay"
        );
        Ok(Self {
            base,
            profile,
            resident,
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
        deadline: Option<Instant>,
    ) -> Result<()> {
        let instance = rand::random();
        let failures = Arc::new(NetworkFailures::default());
        let mut acknowledgements = false;
        let keys = self
            .profile
            .seeds
            .each_ref()
            .map(|seed| seed.transport_key.clone());
        let mut learned: [Option<PeerSignedRelayRecordV1>; 2] = [None, None];
        loop {
            let turn = Instant::now();
            let now = wall_ms()?;
            let remaining =
                deadline.map_or(SIGNED_LEASE, |end| end.saturating_duration_since(turn));
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
                Some(&self.profile.relays),
                None,
            )?;
            let mut accepted = 0;
            let mut seeds = read_seeds(self.base, self.profile);
            for (index, seed) in seeds.iter_mut().enumerate() {
                if let Some(record) = learned[index]
                    .as_ref()
                    .filter(|record| record.expires_at_ms > now)
                {
                    let current_disk_record = seed.as_ref().is_ok_and(|disk| {
                        disk.sequence >= record.sequence
                            && disk.relay_public_key == keys[index]
                            && disk.expires_at_ms > now
                            && seed_candidate(disk.clone(), pins, identity, now).is_ok()
                    });
                    if !current_disk_record {
                        *seed = Ok(record.clone());
                    }
                }
            }
            let sources = prepare_seed_candidates(self.profile, identity, seeds);
            if let (Some(endpoint), Ok(sources)) = (endpoint, sources) {
                let sources =
                    sources.map(|candidate| eligible_routes(candidate.ok(), &self.profile.relays));
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
                    &sources[0],
                    pins,
                    lease.clone(),
                    self.resident.then_some(keys[1].as_slice()),
                    failures.clone(),
                );
                let second = publish_to_source(
                    scope,
                    identity,
                    &sources[1],
                    pins,
                    lease,
                    self.resident.then_some(keys[0].as_slice()),
                    failures.clone(),
                );
                let wall_limit = turn + Duration::from_millis(expires - now);
                let limit = deadline.map_or(wall_limit, |end| end.min(wall_limit));
                let (first, second) = tokio::join!(
                    tokio::time::timeout_at(limit.into(), first),
                    tokio::time::timeout_at(limit.into(), second),
                );
                let mut confirmed = Vec::new();
                for (index, confirmation) in [second.ok().flatten(), first.ok().flatten()]
                    .into_iter()
                    .enumerate()
                {
                    if let Some((source, records)) = confirmation {
                        confirmed.push(source);
                        if let Some(record) = records.into_iter().find(|record| {
                            record.relay_public_key == keys[index] && record.expires_at_ms > now
                        }) {
                            if learned[index]
                                .as_ref()
                                .is_none_or(|old| record.sequence > old.sequence)
                            {
                                learned[index] = Some(record);
                            }
                        }
                    }
                }
                accepted = confirmed.len();
                if accepted != 0 {
                    acknowledgements = true;
                    println!(
                        "{}",
                        serde_json::json!({
                            "event":"public_read_lease_published", "height":height,
                            "transport_identity_sha256":fingerprint(identity.public_key()),
                            "sequence":record.sequence, "expires_at_ms":expires,
                            "acknowledged_sources":accepted,
                            "acknowledged_source_sha256":confirmed,
                            "scope":"relay_only",
                        })
                    );
                }
            }
            let wait = if accepted == 0 { RETRY } else { RENEW };
            let retry = turn
                + Duration::from_millis(
                    self.checkpoint_expires
                        .map_or(u64::MAX, |pin| pin.saturating_sub(now))
                        .min(wait.as_millis() as u64),
                );
            let stop = deadline.map_or(retry, |end| end.min(retry));
            tokio::time::sleep_until(stop.into()).await;
        }
        ensure!(
            self.resident || acknowledgements,
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
    lookup: Option<&[u8]>,
    failures: Arc<NetworkFailures>,
) -> Option<(String, Vec<PeerSignedRelayRecordV1>)> {
    for candidate in routes {
        let request = |keys| {
            query(
                scope,
                identity.clone(),
                candidate.clone(),
                pins,
                Exchange::PublicReader(lease.clone()),
                keys,
                failures.clone(),
            )
        };
        let result = request(lookup.map(<[u8]>::to_vec).into_iter().collect()).await;
        let result = match result {
            Err(_) if lookup.is_some() => request(Vec::new()).await,
            value => value,
        };
        if let Ok(found) = result {
            return Some((fingerprint(&candidate.key), found.records));
        }
    }
    None
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
                                            None,
                                            Arc::new(NetworkFailures::default())
                                        )
                                        .await
                                        .is_some(),
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
        let restart = std::env::var_os("NOVOVM_PUBLIC_READ_BOOTSTRAP_RESTART_TEST").is_some();
        let fixed =
            std::env::var("NOVOVM_PUBLIC_READ_BOOTSTRAP_FIXED_ENDPOINT").as_deref() == Ok("1");
        ensure!(
            !fixed || restart,
            "fixed fixture requires protected bootstrap identity"
        );
        let identity = if restart {
            novovm_relay::transport_identity::provision_transport_identity(
                &stage.join("bootstrap.protected"),
                pins.0,
                &profile.library,
                &profile.library_sha256,
            )?
        } else {
            ephemeral_transport_identity(&profile.library, &profile.library_sha256)?.0
        };
        let mut config = IrohConfigV1::new(&relays, nameserver)?.with_relay_only()?;
        let fixed_pin = stage.join("fixed-endpoint.pin");
        if fixed {
            use novovm_relay::purpose_keys::carrier;
            let custody = stage.join("fixed-carrier.protected");
            if !custody.try_exists()? {
                ensure!(
                    !fixed_pin.try_exists()?,
                    "fixture carrier custody missing after publication"
                );
                carrier::create(&custody, pins.0, identity.public_key())?;
            }
            config = config.with_owned_service_identity(carrier::read(
                &custody,
                pins.0,
                identity.public_key(),
            )?)?;
        }
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
                    if fixed {
                        let bytes = address.endpoint_key;
                        if fixed_pin.try_exists()? {
                            ensure!(fs::read(&fixed_pin)? == bytes, "fixture carrier endpoint changed");
                        } else {
                            fs::write(stage.join("fixed-endpoint.tmp"), bytes)?;
                            fs::rename(stage.join("fixed-endpoint.tmp"), &fixed_pin)?;
                        }
                    }
                    let now = Instant::now();
                    let until = now + Duration::from_secs(if restart { 360 } else { 110 });
                    let mut rotate_at = now + Duration::from_secs(100);
                    let store = Arc::new(Mutex::new(RendezvousStoreV1::new(
                        rand::random(), now + Duration::from_secs(110),
                    )?));
                    fs::write(stage.join("ready.tmp"), serde_json::to_vec(&serde_json::json!({
                        "contact": address, "transport_key": identity.public_key(),
                        "transport_sha256": fingerprint(identity.public_key()),
                    }))?)?;
                    if restart {
                        let now = wall_ms()?;
                        let record = sign_relay_record_v1(
                            &identity,
                            novovm_network::discovery_control::native_locator_id_v1(pins),
                            vec![IrohServiceEndpointV1 { instance: rand::random(), address }.endpoint()?],
                            now,
                            now + 120_000,
                            now,
                        )?;
                        fs::write(stage.join("locator.tmp"), serde_json::to_vec(&record)?)?;
                        fs::rename(stage.join("locator.tmp"), stage.join("locator.json"))?;
                        let introducer = stage.join("introducer.json");
                        if introducer.is_file() {
                            use super::super::super::super::LocalLocators;
                            use novovm_network::discovery_control::encode_node_locator_v1;
                            let contact: serde_json::Value = serde_json::from_slice(&fs::read(introducer)?)?;
                            let candidate = DialCandidate {
                                key: serde_json::from_value(contact["transport_key"].clone())?,
                                route: serde_json::from_value(contact["contact"].clone())?,
                                source: "installed_contact",
                                sequence: 0,
                            };
                            let ordinary = encode_node_locator_v1(&record, pins)?;
                            query(
                                scope, identity.clone(), candidate, pins,
                                Exchange::Publisher(LocalLocators { ordinary, read_service: None }),
                                Vec::new(), Arc::new(NetworkFailures::default()),
                            ).await?;
                            println!("isolated restarted bootstrap authenticated its rotated locator to the primary");
                        }
                    }
                    fs::rename(stage.join("ready.tmp"), stage.join("ready.json"))?;
                    let serve_streams = async {
                        loop {
                            let mut stream: CarrierStreamV1<'_> = tokio::select! {
                                _ = tokio::time::sleep_until(rotate_at.into()), if restart => {
                                    let now = Instant::now();
                                    *store.lock().map_err(|_| anyhow::anyhow!("fixture store poisoned"))? =
                                        RendezvousStoreV1::new(rand::random(), now + Duration::from_secs(110))?;
                                    rotate_at = now + Duration::from_secs(100);
                                    continue;
                                }
                                stream = scope.accept_inbound() => stream?.into(),
                            };
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
