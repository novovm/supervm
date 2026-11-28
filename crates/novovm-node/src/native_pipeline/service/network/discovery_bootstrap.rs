//! A non-voting role of the native discovery service.
use super::discovery::{serve, Pins};
use super::{
    publication_endpoint, wall_ms, CarrierStreamV1, IrohConfigV1, IrohEndpointOwnerV1, IrohScopeV1,
    ServiceConcurrencyV1, CARRIER_CONNECTION_CEILING, DISCOVERY_ENTRY,
};
use anyhow::{ensure, Context, Result};
use novovm_network::{
    discovery_control::{encode_node_locator_v1, native_locator_id_v1, node_owner_id_v1},
    opaque_rendezvous::RendezvousStoreV1,
    product_directory::{sign_relay_record_v1, validate_relay_record_v1, PeerSignedRelayRecordV1},
    product_overlay::TransportIdentity,
    product_service_lease::IrohServiceEndpointV1,
};
#[cfg(target_os = "linux")]
use novovm_relay::linux_owner_files as owner_files;
#[cfg(windows)]
use novovm_relay::windows_owner_files as owner_files;
use novovm_relay::{purpose_keys::carrier, transport_identity::provision_transport_identity};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    future::{poll_fn, Future},
    io::{Read, Write},
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    task::Poll,
    time::{Duration, Instant},
};

const STORE_TTL: Duration = Duration::from_secs(120);
const STORE_ROTATE: Duration = Duration::from_secs(100);
const LOCATOR_TTL_MS: u64 = 120_000;
const REFRESH: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    chain_id: u64,
    genesis: [u8; 32],
    protocol: [u8; 32],
    library: PathBuf,
    library_sha256: String,
    state_directory: PathBuf,
    nameserver: IpAddr,
    #[serde(default)]
    relays: Vec<String>,
    bind: SocketAddr,
    advertised_direct: SocketAddr,
}

pub fn run_discovery_bootstrap(path: &Path, sha256: &str) -> Result<()> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(16_385).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 16_384 && format!("{:x}", Sha256::digest(&bytes)) == sha256,
        "bootstrap configuration size or SHA-256 mismatch"
    );
    let config: Config = serde_json::from_slice(&bytes)?;
    ensure!(
        config.chain_id > 0 && config.genesis != [0; 32] && config.protocol != [0; 32],
        "explicit native chain domain required"
    );
    let advertised = config.advertised_direct;
    ensure!(
        config.bind.is_ipv4()
            && advertised.is_ipv4()
            && config.bind.port() != 0
            && advertised.port() == config.bind.port()
            && !advertised.ip().is_unspecified()
            && !advertised.ip().is_multicast()
            && advertised.ip().is_loopback() == config.bind.ip().is_loopback(),
        "fixed IPv4 bind and advertised UDP coordinates required"
    );
    ensure!(
        config.library.is_absolute() && config.state_directory.is_absolute(),
        "bootstrap state and AOEM paths must be absolute"
    );
    let state = config.state_directory;
    #[cfg(target_os = "linux")]
    owner_files::check_private_root(&state)?;
    #[cfg(windows)]
    {
        owner_files::check_path(&state)?;
        ensure!(
            fs::metadata(&state)?.is_dir(),
            "private bootstrap state directory required"
        );
    }
    let lock_path = state.join("bootstrap.lock");
    if !lock_path.try_exists()? {
        match owner_files::create(&lock_path, b"") {
            Ok(()) => {}
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::AlreadyExists) => {}
            Err(error) => return Err(error),
        }
    }
    #[cfg(target_os = "linux")]
    owner_files::check_private_file(&lock_path)?;
    #[cfg(windows)]
    owner_files::check_path(&lock_path)?;
    let lock = OpenOptions::new().read(true).write(true).open(lock_path)?;
    lock.try_lock()
        .context("another bootstrap owns this state")?;
    let key = state.join("transport.protected");
    let seed = state.join("carrier.protected");
    let locator = state.join("locator.json");
    let prior = locator.try_exists()?;
    ensure!(
        !prior || (key.try_exists()? && seed.try_exists()?),
        "bootstrap identity missing after publication"
    );
    let identity = provision_transport_identity(
        &key,
        config.chain_id,
        &config.library,
        &config.library_sha256,
    )?;
    if !seed.try_exists()? {
        carrier::create(&seed, config.chain_id, identity.public_key())?;
    }
    let carrier = IrohConfigV1::new(&config.relays, config.nameserver)?
        .with_bind_address(config.bind)?
        .with_service_concurrency(ServiceConcurrencyV1::from_host())
        .with_connection_ceiling(CARRIER_CONNECTION_CEILING)?
        .with_owned_service_identity(carrier::read(
            &seed,
            config.chain_id,
            identity.public_key(),
        )?)?;
    let pins = (config.chain_id, config.genesis, config.protocol);
    let previous = if prior {
        let old: PeerSignedRelayRecordV1 =
            serde_json::from_slice(&owner_files::read_bounded(&locator, 64 * 1024)?)?;
        ensure!(
            old.record_id == native_locator_id_v1(pins)
                && old.relay_public_key == identity.public_key(),
            "stored bootstrap locator identity or domain changed"
        );
        validate_relay_record_v1(&old, old.issued_at_ms, &identity)?;
        let endpoint = IrohServiceEndpointV1::from_record(&old)?;
        Some((old.sequence, endpoint.address.endpoint_key))
    } else {
        None
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            #[cfg(target_os = "linux")]
            use tokio::signal::unix::{signal, SignalKind};
            #[cfg(target_os = "linux")]
            let mut terminate = signal(SignalKind::terminate())?;
            #[cfg(target_os = "linux")]
            let mut interrupt = signal(SignalKind::interrupt())?;
            #[cfg(windows)]
            let mut terminate = tokio::signal::windows::ctrl_break()?;
            #[cfg(windows)]
            let mut interrupt = tokio::signal::windows::ctrl_c()?;
            let owner = tokio::time::timeout(
                Duration::from_secs(8),
                IrohEndpointOwnerV1::bind(carrier, || Ok(())),
            )
            .await??;
            let service = owner.with_continuous_service_v4(
                || Ok(()),
                |scope| Box::pin(run(scope, identity, pins, advertised, locator, previous)),
            );
            let result = tokio::select! {
                result = service => result,
                _ = terminate.recv() => Ok(()),
                _ = interrupt.recv() => Ok(()),
            };
            let closed = owner.close().await;
            result.and(closed)
        })
}

async fn run(
    scope: &IrohScopeV1,
    identity: TransportIdentity,
    pins: Pins,
    advertised: SocketAddr,
    locator: PathBuf,
    previous: Option<(u64, [u8; 32])>,
) -> Result<()> {
    let mut sequence = previous.map_or(0, |(sequence, _)| sequence);
    type Session<'a> = Pin<Box<dyn Future<Output = ()> + 'a>>;
    let mut sessions: Vec<Session<'_>> = Vec::new();
    let now = Instant::now();
    let mut store = new_store(now)?;
    let mut rotate_at = now + STORE_ROTATE;
    let mut next = now;
    let mut ready = false;
    loop {
        let now = Instant::now();
        if now >= rotate_at {
            store = new_store(now)?;
            rotate_at = now + STORE_ROTATE;
            next = now;
        }
        if now >= next {
            let mut address = scope.address()?;
            ensure!(
                previous.is_none_or(|(_, key)| key == address.endpoint_key),
                "persisted bootstrap Iroh endpoint identity changed"
            );
            address.direct = vec![advertised];
            address.relays = scope.connected_relay_address()?.relays;
            let instance = store
                .lock()
                .map_err(|_| anyhow::anyhow!("bootstrap store poisoned"))?
                .instance();
            let endpoint =
                publication_endpoint(IrohServiceEndpointV1 { instance, address }, None, None)?
                    .context("bootstrap endpoint unavailable")?;
            let wall = wall_ms()?;
            sequence = wall.max(
                sequence
                    .checked_add(1)
                    .context("locator sequence exhausted")?,
            );
            let record = sign_relay_record_v1(
                &identity,
                native_locator_id_v1(pins),
                vec![endpoint],
                wall,
                wall.checked_add(LOCATOR_TTL_MS)
                    .context("locator expiry overflow")?,
                sequence,
            )?;
            let value = encode_node_locator_v1(&record, pins)?;
            owner_files::atomic(&locator, &serde_json::to_vec(&record)?)?;
            store
                .lock()
                .map_err(|_| anyhow::anyhow!("bootstrap store poisoned"))?
                .put_node_locator_v1(
                    node_owner_id_v1(identity.public_key())?,
                    sequence,
                    Duration::from_secs(30),
                    value,
                    Instant::now(),
                )?;
            if !ready {
                writeln!(
                    std::io::stdout().lock(),
                    "{}",
                    serde_json::json!({
                        "event": "discovery_bootstrap_ready",
                        "chain_id": pins.0,
                        "announced_direct": advertised,
                        "transport_identity_sha256": format!("{:x}", Sha256::digest(identity.public_key())),
                    })
                )?;
                ready = true;
            }
            next = now + REFRESH;
        }
        tokio::select! {
            _ = tokio::time::sleep_until(next.min(rotate_at).into()) => {}
            _ = poll_fn(|cx| {
                let before = sessions.len();
                sessions.retain_mut(|session| session.as_mut().poll(cx).is_pending());
                if before == sessions.len() { Poll::Pending } else { Poll::Ready(()) }
            }) => {}
            incoming = scope.accept_inbound(), if sessions.len() < scope.connection_capacity() => {
                let Ok(stream) = incoming else {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    continue;
                };
                let identity = identity.clone();
                let store = Arc::clone(&store);
                sessions.push(Box::pin(async move {
                    let mut stream: CarrierStreamV1<'_> = stream.into();
                    if matches!(
                        tokio::time::timeout(Duration::from_secs(3), stream.read_frame()).await,
                        Ok(Ok(entry)) if entry == DISCOVERY_ENTRY
                    ) {
                        let _ = serve(stream, identity, pins, store).await;
                    }
                }));
            }
        }
    }
}

fn new_store(now: Instant) -> Result<Arc<Mutex<RendezvousStoreV1>>> {
    Ok(Arc::new(Mutex::new(RendezvousStoreV1::new(
        rand::random(),
        now + STORE_TTL,
    )?)))
}
