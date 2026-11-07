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
    failures: Arc<AtomicU64>,
    lookup_turn: usize,
}
impl<'a> Discovery<'a> {
    pub(super) fn new(
        scope: &'a IrohScopeV1,
        config: &Config,
        identity: &TransportIdentity,
        keys: &[Vec<u8>],
        pins: Pins,
        failures: Arc<AtomicU64>,
    ) -> Result<Self> {
        let now = Instant::now();
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
            let result = self
                .store
                .lock()
                .map_err(|_| anyhow::anyhow!("discovery store poisoned"))?
                .put_node_locator_v1(
                    node_owner_id_v1(self.identity.public_key())?,
                    self.revision,
                    Duration::from_secs(30),
                    bytes.clone(),
                    now,
                );
            if result.is_err() {
                self.failures.fetch_add(1, Ordering::Relaxed);
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
                let _ = Self::update_seed(seed, &record, self.pins, &self.identity, wall);
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
            seed.job = Some(Box::pin(async move {
                tokio::time::timeout(
                    Duration::from_secs(8),
                    query(
                        scope,
                        identity,
                        candidate.public_key,
                        candidate.endpoint.address,
                        pins,
                        own,
                        lookups,
                    ),
                )
                .await?
            }));
        }
        let mut records = Vec::new();
        let mut store = self
            .store
            .lock()
            .map_err(|_| anyhow::anyhow!("discovery store poisoned"))?;
        for key in &self.keys {
            if let Some(record) = store.node_locator_v1(node_owner_id_v1(key)?, now)? {
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
                        self.failures.fetch_add(1, Ordering::Relaxed);
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
    let mut stream: CarrierStreamV1<'_> = scope.connect(&route).await?.into();
    stream.write_frame(DISCOVERY_ENTRY).await?;
    let mut channel = authenticate_structured_bound_session_observed(
        &mut stream,
        &identity,
        key,
        true,
        &mut HandshakeReplayCacheV1::new(8),
        &mut |_, _| Ok(()),
    )
    .await?;
    let instance = select(&mut stream, &mut channel, pins, None).await?;
    let mut client =
        DiscoveryClient::from_authenticated_node_locators_v1(&mut stream, channel, instance)?;
    let publication_revision = wall_ms()?;
    client
        .put_node_locator_v1(publication_revision, 30_000, own)
        .await?;
    let mut found = Vec::new();
    for key in lookups {
        if let Some(record) = client.get_node_locator_v1(&key).await? {
            found.push(decode_node_locator_v1(
                &record.ciphertext,
                &key,
                pins,
                wall_ms()?,
                &identity,
            )?);
        }
    }
    client.close().await?;
    Ok(found)
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
