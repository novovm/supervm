//! Adapt the shared discovery RPC/store to this SAME native network owner.
//! Public node coordinates are separate from private chat/contact capabilities.
use super::*;
use novovm_network::{
    discovery_control::{
        decode_node_locator_v1, encode_node_locator_v1, native_locator_id_v1, node_owner_id_v1,
        serve_authenticated_node_locators_v1, DiscoveryClient,
    },
    opaque_rendezvous::RendezvousStoreV1,
    transport_binding::BoundSecureChannelV1,
};

type Pins = (u64, [u8; 32], [u8; 32]);
type Lookup<'a> = Pin<Box<dyn Future<Output = Result<Vec<PeerSignedRelayRecordV1>>> + 'a>>;
const TAG: &[u8; 8] = b"NVDISC01";
struct Seed<'a> {
    path: PathBuf,
    book: Option<ServiceLeaseBookV1>,
    key: Option<Vec<u8>>,
    job: Option<Lookup<'a>>,
    next: Instant,
    quarantined: Option<u64>,
}
pub(super) struct Discovery<'a> {
    scope: &'a IrohScopeV1,
    identity: TransportIdentity,
    keys: Vec<Vec<u8>>,
    pins: Pins,
    pub(super) store: Arc<Mutex<RendezvousStoreV1>>,
    rotate_at: Instant,
    local: Option<Vec<u8>>,
    publish_at: Instant,
    revision: u64,
    seeds: Vec<Seed<'a>>,
    failures: Arc<NetworkFailures>,
    lookup_turn: usize,
    cache_path: PathBuf,
    cached: Vec<PeerSignedRelayRecordV1>,
}
impl<'a> Discovery<'a> {
    pub(super) fn new(
        scope: &'a IrohScopeV1,
        config: &Config,
        identity: &TransportIdentity,
        keys: &[Vec<u8>],
        pins: Pins,
        failures: Arc<NetworkFailures>,
    ) -> Result<Self> {
        let now = Instant::now();
        let cache_path = config
            .public_record_file
            .with_extension("discovery-cache.json");
        let cached: Vec<PeerSignedRelayRecordV1> = if cache_path.try_exists()? {
            read_document(&cache_path)?
        } else {
            Vec::new()
        };
        ensure!(
            cached.len() <= 2
                && cached
                    .iter()
                    .map(|r| &r.relay_public_key)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == cached.len(),
            "bounded unique discovery bootstrap checkpoint"
        );
        Ok(Self {
            scope,
            identity: identity.clone(),
            keys: keys.to_vec(),
            pins,
            store: Arc::new(Mutex::new(RendezvousStoreV1::new(
                rand::random(),
                now + Duration::from_secs(120),
            )?)),
            rotate_at: now + Duration::from_secs(100),
            local: None,
            publish_at: now,
            revision: 0,
            seeds: config
                .discovery_seeds
                .iter()
                .map(|path| Seed {
                    path: path.clone(),
                    book: None,
                    key: None,
                    job: None,
                    next: now,
                    quarantined: None,
                })
                .collect(),
            failures,
            lookup_turn: 0,
            cache_path,
            cached,
        })
    }
    pub(super) fn instance(&self) -> Result<[u8; 32]> {
        Ok(self
            .store
            .lock()
            .map_err(|_| anyhow::anyhow!("discovery store poisoned"))?
            .instance())
    }
    pub(super) fn rotate_if_due(&mut self, now: Instant) -> Result<bool> {
        if now < self.rotate_at {
            return Ok(false);
        }
        // Old RPCs keep their original Arc/expiry. Existing object sessions are
        // NOT coupled to an ephemeral discovery instance or its publication TTL.
        self.store = Arc::new(Mutex::new(RendezvousStoreV1::new(
            rand::random(),
            now + Duration::from_secs(120),
        )?));
        self.rotate_at = now + Duration::from_secs(100);
        self.local = None;
        self.publish_at = now;
        self.revision = 0;
        Ok(true)
    }
    pub(super) fn set_local(&mut self, record: &PeerSignedRelayRecordV1) -> Result<()> {
        self.local = Some(encode_node_locator_v1(record, self.pins)?);
        self.publish_at = Instant::now();
        Ok(())
    }
    fn update_seed(
        seed: &mut Seed<'a>,
        record: &PeerSignedRelayRecordV1,
        pins: Pins,
        identity: &TransportIdentity,
        wall: u64,
    ) -> Result<()> {
        ensure!(
            record.record_id == native_locator_id_v1(pins),
            "discovery seed domain differs"
        );
        if let Some(book) = &mut seed.book {
            ensure!(
                seed.key.as_deref() == Some(record.relay_public_key.as_slice()),
                "discovery seed key changed"
            );
            book.replace_self_owned_records(vec![record.clone()], wall)?;
        } else {
            // The existing self-owned policy may use a historical coordinate
            // ONLY for a new pinned handshake, never as a renewed data lease.
            ensure!(record.issued_at_ms <= wall, "future discovery seed");
            seed.book = Some(ServiceLeaseBookV1::new_self_owned(
                SelfOwnedTrustV1 {
                    version: 2,
                    node_pins: BTreeSet::from([record.relay_public_key.clone()]),
                },
                vec![record.clone()],
                record.issued_at_ms,
                identity.clone(),
            )?);
            seed.key = Some(record.relay_public_key.clone());
        }
        if seed.quarantined.is_some_and(|old| record.sequence > old) {
            seed.quarantined = None;
        }
        Ok(())
    }
    pub(super) fn update_bootstraps(&mut self, records: &[PeerSignedRelayRecordV1], wall: u64) {
        for seed in &mut self.seeds {
            for record in records {
                if seed.key.as_deref() == Some(record.relay_public_key.as_slice()) {
                    let _ = Self::update_seed(seed, record, self.pins, &self.identity, wall);
                }
            }
        }
    }
    pub(super) fn persist_bootstraps(&mut self) -> Result<()> {
        let mut checkpoint = self
            .cached
            .iter()
            .map(|r| (r.relay_public_key.clone(), r.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut active = BTreeSet::new();
        for seed in &self.seeds {
            if let Some(book) = &seed.book {
                for record in book.self_owned_records()? {
                    active.insert(record.relay_public_key.clone());
                    if let Some(old) = checkpoint.get(&record.relay_public_key) {
                        ensure!(
                            record.sequence != old.sequence || record == *old,
                            "conflicting signed bootstrap checkpoint"
                        );
                        if record.sequence < old.sequence {
                            continue;
                        }
                    }
                    checkpoint.insert(record.relay_public_key.clone(), record);
                }
            }
        }
        if checkpoint.len() > 2 {
            checkpoint.retain(|key, _| active.contains(key));
        }
        let records = checkpoint.into_values().collect::<Vec<_>>();
        if !records.is_empty() && records != self.cached {
            publish(&self.cache_path, &records)?;
            self.cached = records;
        }
        Ok(())
    }
    pub(super) fn maintain(
        &mut self,
        now: Instant,
        wall: u64,
    ) -> Result<Vec<PeerSignedRelayRecordV1>> {
        let Some(bytes) = self.local.clone() else {
            return Ok(Vec::new());
        };
        if now >= self.publish_at {
            self.revision = self
                .revision
                .checked_add(1)
                .context("discovery revision exhausted")?;
            let result = {
                let mut store = self
                    .store
                    .lock()
                    .map_err(|_| anyhow::anyhow!("discovery store poisoned"))?;
                // The tick may predate a rotation or an intervening RPC. Use
                // admission time under this SAME store's lock, not tick time.
                store.put_node_locator_v1(
                    node_owner_id_v1(self.identity.public_key())?,
                    self.revision,
                    Duration::from_secs(30),
                    bytes.clone(),
                    Instant::now(),
                )
            };
            if let Err(error) = result {
                self.failures
                    .record("discovery_local_publish", None, &error.into());
            }
            self.publish_at = now + Duration::from_secs(10);
        }
        for seed in &mut self.seeds {
            if seed.job.is_some() || now < seed.next {
                continue;
            }
            seed.next = now + Duration::from_secs(10);
            if let Ok(record) = read_document::<PeerSignedRelayRecordV1>(&seed.path) {
                // A stale disk copy must not roll back a newer network record.
                if seed.book.is_none() {
                    if record.issued_at_ms > wall {
                        continue;
                    }
                    if Self::update_seed(
                        seed,
                        &record,
                        self.pins,
                        &self.identity,
                        record.issued_at_ms,
                    )
                    .is_ok()
                    {
                        if let Some(cached) = self.cached.iter().find(|r| {
                            r.relay_public_key == record.relay_public_key
                                && r.sequence >= record.sequence
                        }) {
                            // Retain the independently configured root pin. A
                            // checkpoint supplies coordinates, never new trust.
                            if cached.issued_at_ms > wall
                                || Self::update_seed(
                                    seed,
                                    cached,
                                    self.pins,
                                    &self.identity,
                                    cached.issued_at_ms,
                                )
                                .is_err()
                            {
                                seed.book = None;
                                seed.key = None;
                                self.failures.record(
                                    "discovery_checkpoint",
                                    None,
                                    &anyhow::anyhow!("cached bootstrap is future-dated or invalid"),
                                );
                            }
                        }
                    }
                } else {
                    let _ = Self::update_seed(seed, &record, self.pins, &self.identity, wall);
                }
            }
            if seed.quarantined.is_some() {
                continue;
            }
            let Some(book) = &mut seed.book else { continue };
            let Some(candidate) = book.bootstrap_candidates(wall)?.into_iter().next() else {
                continue;
            };
            let identity = self.identity.clone();
            let pins = self.pins;
            let own = bytes.clone();
            let scope = self.scope;
            let count = self.keys.len().min(28);
            let lookups = (0..count)
                .map(|i| self.keys[(self.lookup_turn + i) % self.keys.len()].clone())
                .collect();
            self.lookup_turn = (self.lookup_turn + count) % self.keys.len();
            seed.job = Some(Box::pin(query(
                scope,
                identity,
                candidate.public_key,
                candidate.endpoint.address,
                pins,
                own,
                lookups,
            )));
        }
        let mut records = Vec::new();
        let mut store = self
            .store
            .lock()
            .map_err(|_| anyhow::anyhow!("discovery store poisoned"))?;
        let observed = Instant::now();
        for key in &self.keys {
            if let Some(record) = store.node_locator_v1(node_owner_id_v1(key)?, observed)? {
                if let Ok(record) =
                    decode_node_locator_v1(&record.ciphertext, key, self.pins, wall, &self.identity)
                {
                    records.push(record);
                }
            }
        }
        Ok(records)
    }
    pub(super) fn poll(&mut self, cx: &mut std::task::Context<'_>) -> Vec<PeerSignedRelayRecordV1> {
        let mut records = Vec::new();
        for seed in &mut self.seeds {
            let Some(job) = &mut seed.job else { continue };
            if let Poll::Ready(result) = job.as_mut().poll(cx) {
                match result {
                    Ok(found) => records.extend(found),
                    Err(error) => {
                        self.failures.record("discovery_lookup", None, &error);
                        if terminal_session_rejection(&error) {
                            seed.quarantined = seed
                                .book
                                .as_ref()
                                .and_then(|b| b.self_owned_records().ok())
                                .and_then(|r| r.first().map(|r| r.sequence));
                        }
                    }
                }
                seed.job = None;
            }
        }
        records
    }
}
async fn query(
    scope: &IrohScopeV1,
    identity: TransportIdentity,
    key: Vec<u8>,
    route: IrohAddressV1,
    pins: Pins,
    own: Vec<u8>,
    lookups: Vec<Vec<u8>>,
) -> Result<Vec<PeerSignedRelayRecordV1>> {
    let stage = std::cell::Cell::new("carrier connect");
    // Keep the original single eight-second deadline, including every RPC.
    // The phase survives cancellation so timeout does not erase the actual boundary.
    tokio::time::timeout(Duration::from_secs(8), async {
        let mut stream: CarrierStreamV1<'_> = scope
            .connect(&route)
            .await
            .context("discovery carrier connect")?
            .into();
        stage.set("entry write");
        stream
            .write_frame(DISCOVERY_ENTRY)
            .await
            .context("discovery entry write")?;
        stage.set("bound authentication");
        let mut channel = authenticate_structured_bound_session_observed(
            &mut stream,
            &identity,
            key,
            true,
            &mut HandshakeReplayCacheV1::new(8),
            &mut |_, _| Ok(()),
        )
        .await
        .context("discovery bound authentication")?;
        stage.set("domain selection");
        let instance = select(&mut stream, &mut channel, pins, None)
            .await
            .context("discovery domain selection")?;
        let mut client =
            DiscoveryClient::from_authenticated_node_locators_v1(&mut stream, channel, instance)?;
        let publication_revision = wall_ms()?;
        stage.set("remote publication");
        client
            .put_node_locator_v1(publication_revision, 30_000, own)
            .await
            .context("discovery remote publication")?;
        let mut found = Vec::new();
        stage.set("remote lookup");
        for key in lookups {
            if let Some(record) = client
                .get_node_locator_v1(&key)
                .await
                .context("discovery remote lookup")?
            {
                found.push(decode_node_locator_v1(
                    &record.ciphertext,
                    &key,
                    pins,
                    wall_ms()?,
                    &identity,
                )?);
            }
        }
        stage.set("close");
        client.close().await.context("discovery close")?;
        Ok(found)
    })
    .await
    .with_context(|| format!("discovery deadline at {}", stage.get()))?
}

pub(super) async fn serve(
    mut stream: CarrierStreamV1<'_>,
    identity: TransportIdentity,
    pins: Pins,
    store: Arc<Mutex<RendezvousStoreV1>>,
) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(8), async {
        let (mut channel, _) = authenticate_structured_service_client_observed(
            &mut stream,
            &identity,
            ServicePeerPolicyV1::AnyAuthenticated,
            &mut HandshakeReplayCacheV1::new(8),
            &mut |_, _| Ok(()),
        )
        .await?;
        let instance = store
            .lock()
            .map_err(|_| anyhow::anyhow!("discovery store poisoned"))?
            .instance();
        select(&mut stream, &mut channel, pins, Some(instance)).await?;
        serve_authenticated_node_locators_v1(&mut stream, channel, store).await?;
        Ok(())
    })
    .await?
}
async fn select(
    stream: &mut CarrierStreamV1<'_>,
    channel: &mut BoundSecureChannelV1,
    pins: Pins,
    instance: Option<[u8; 32]>,
) -> Result<[u8; 32]> {
    let mut prefix = TAG.to_vec();
    prefix.extend_from_slice(&pins.0.to_be_bytes());
    prefix.extend_from_slice(&pins.1);
    prefix.extend_from_slice(&pins.2);
    let session = channel.session_id();
    let frame = |payload| {
        Frame::new(
            Kind::Data,
            session,
            0,
            u64::from_be_bytes(*TAG),
            0,
            0,
            payload,
        )
    };
    if let Some(instance) = instance {
        let mut payload = prefix.clone();
        payload.extend_from_slice(&instance);
        stream
            .write_frame(&encode_iroh_envelope_v1(
                &channel.seal_novorudp_frame(&frame(payload))?,
            )?)
            .await?;
    }
    let wire = stream.read_frame().await?;
    let decoded = channel
        .open_novorudp_frame(&decode_iroh_envelope_v1(&wire).context(ServiceSelectionMismatch)?)
        .context(ServiceSelectionMismatch)?;
    ensure!(
        decoded == frame(decoded.payload.clone())
            && decoded.payload.len() == prefix.len() + 32
            && decoded.payload.starts_with(&prefix),
        ServiceSelectionMismatch
    );
    let remote: [u8; 32] = decoded.payload[prefix.len()..].try_into()?;
    ensure!(
        remote != [0; 32] && instance.is_none_or(|expected| expected == remote),
        ServiceSelectionMismatch
    );
    if instance.is_none() {
        stream
            .write_frame(&encode_iroh_envelope_v1(
                &channel.seal_novorudp_frame(&decoded)?,
            )?)
            .await?;
    }
    Ok(remote)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_pipeline::consensus::native::tests::separated_material;

    #[test]
    fn rotated_discovery_store_uses_time_sampled_after_its_lock() -> Result<()> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let owner = IrohEndpointOwnerV1::bind(
                    IrohConfigV1::new(&[], "127.0.0.1".parse()?)?
                        .with_bind_address("127.0.0.1:0".parse()?)?,
                    || Ok(()),
                )
                .await?;
                let result = owner
                    .with_continuous_service_v4(
                        || Ok(()),
                        |scope| {
                            Box::pin(async move {
                                let (_, keys, domain, _) = separated_material()?;
                                let pins = (domain.chain_id, domain.genesis, domain.protocol);
                                let config = Config {
                                    probe_control_file: None,
                                    relays: Vec::new(),
                                    nameserver: "127.0.0.1".parse()?,
                                    bind_address: "127.0.0.1:0".parse()?,
                                    public_record_file: std::env::temp_dir().join(format!(
                                        "novonet-clock-{:016x}.json",
                                        rand::random::<u64>()
                                    )),
                                    carrier_identity_file: None,
                                    peer_records: Vec::new(),
                                    discovery: true,
                                    discovery_seeds: Vec::new(),
                                    relay_forwarding: false,
                                    fallback_after_ms: 1000,
                                    retry_ms: 1000,
                                    udp_recovery_ms: 5000,
                                };
                                let mut discovery = Discovery::new(
                                    scope,
                                    &config,
                                    &keys[0],
                                    &[keys[1].public_key().to_vec()],
                                    pins,
                                    Arc::new(NetworkFailures::default()),
                                )?;
                                for sequence in 1..=3 {
                                    // Reproduce the real 100-second rotation without a
                                    // sleep or changing production rotation/lifetime.
                                    let tick = Instant::now() - Duration::from_millis(1);
                                    discovery.rotate_at = tick;
                                    let previous = discovery.store.clone();
                                    ensure!(discovery.rotate_if_due(tick)?);
                                    ensure!(
                                        previous.lock().unwrap().instance()
                                            != discovery.instance()?
                                    );
                                    let wall = wall_ms()?;
                                    let record = sign_relay_record_v1(
                                        &keys[0],
                                        native_locator_id_v1(pins),
                                        vec![IrohServiceEndpointV1 {
                                            instance: discovery.instance()?,
                                            address: scope.address()?,
                                        }
                                        .endpoint()?],
                                        wall,
                                        wall + 120_000,
                                        sequence,
                                    )?;
                                    discovery.set_local(&record)?;
                                    discovery.publish_at = tick;
                                    ensure!(discovery.maintain(tick, wall)?.is_empty());
                                    ensure!(discovery
                                        .store
                                        .lock()
                                        .unwrap()
                                        .node_locator_v1(
                                            node_owner_id_v1(keys[0].public_key())?,
                                            Instant::now()
                                        )?
                                        .is_some());
                                    // Old in-flight RPCs retain their original store;
                                    // rotation must not invalidate it or reset its TTL.
                                    previous.lock().unwrap().stats(Instant::now())?;
                                    // Also cover a sampled tick preceding an RPC store
                                    // observation, without weakening ClockRegression.
                                    discovery.maintain(tick, wall)?;
                                }
                                ensure!(discovery.failures.count.load(Ordering::Relaxed) == 0);
                                Ok(())
                            })
                        },
                    )
                    .await;
                owner.close().await?;
                result
            })
    }

    #[test]
    fn bootstrap_rejects_bad_signature_domain_future_key_change_and_rollback() -> Result<()> {
        let (_, keys, domain, _) = separated_material()?;
        let pins = (domain.chain_id, domain.genesis, domain.protocol);
        let endpoint = IrohServiceEndpointV1 {
            instance: [1; 32],
            address: IrohAddressV1 {
                endpoint_key: ed25519_dalek::SigningKey::from_bytes(&[17; 32])
                    .verifying_key()
                    .to_bytes(),
                direct: vec!["127.0.0.1:12345".parse()?],
                relays: Vec::new(),
            },
        }
        .endpoint()?;
        let now = wall_ms()?;
        let record = sign_relay_record_v1(
            &keys[0],
            native_locator_id_v1(pins),
            vec![endpoint],
            now,
            now + 120_000,
            2,
        )?;
        let empty = || Seed {
            path: PathBuf::new(),
            book: None,
            key: None,
            job: None,
            next: Instant::now(),
            quarantined: None,
        };
        let mut seed = empty();
        Discovery::update_seed(&mut seed, &record, pins, &keys[1], now)?;
        // The original signed record and sequence owner, not another parser or
        // a checkpoint's alleged identity, remains authoritative.
        let mut tampered = record.clone();
        tampered.endpoints[0].uri.push('x');
        ensure!(Discovery::update_seed(&mut empty(), &tampered, pins, &keys[1], now).is_err());
        ensure!(Discovery::update_seed(
            &mut empty(),
            &record,
            (pins.0 + 1, pins.1, pins.2),
            &keys[1],
            now
        )
        .is_err());
        ensure!(Discovery::update_seed(&mut empty(), &record, pins, &keys[1], now - 1).is_err());
        let other = sign_relay_record_v1(
            &keys[2],
            &record.record_id,
            record.endpoints.clone(),
            now,
            now + 120_000,
            3,
        )?;
        ensure!(Discovery::update_seed(&mut seed, &other, pins, &keys[1], now).is_err());
        let older = sign_relay_record_v1(
            &keys[0],
            &record.record_id,
            record.endpoints.clone(),
            now,
            now + 120_000,
            1,
        )?;
        ensure!(Discovery::update_seed(&mut seed, &older, pins, &keys[1], now).is_err());
        ensure!(seed.book.as_ref().unwrap().self_owned_records()?[0] == record);
        // Expired coordinates may locate a pinned fresh handshake, never become
        // an active data lease. This is the original bootstrap owner contract.
        let mut stale = empty();
        Discovery::update_seed(&mut stale, &record, pins, &keys[1], now + 120_001)?;
        ensure!(!stale
            .book
            .as_mut()
            .unwrap()
            .bootstrap_candidates(now + 120_001)?
            .is_empty());
        Ok(())
    }
}
