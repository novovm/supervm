//! Self-owned lifecycle for the EXISTING rendezvous V2 offer and authenticated
//! RPC. Stable listener; bounded rotating opaque stores and per-stream leases.
use crate::{
    carrier_stream::CarrierStreamV1,
    iroh_transport::{with_iroh_owned_listener_v1, IrohConfigV1},
    opaque_rendezvous::RendezvousStoreV1,
    product_directory::PeerSignedRelayRecordV1,
    product_rendezvous_daemon::now_ms,
    product_service_lease::{
        serve_service_offer_selected_v2, IrohServiceEndpointV1, ServiceLeaseHostV1,
    },
};
use anyhow::{ensure, Result};
use ed25519_dalek::SigningKey;
use futures_util::{stream::FuturesUnordered, StreamExt};
use std::{
    collections::BTreeSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

type Store = Arc<Mutex<RendezvousStoreV1>>;
struct Generation {
    store: Store,
    expires_ms: u64,
}

/// The platform owner supplies protected identity, durable history and an
/// explicit stop handle. No publisher, registry, CA or chain RPC is consulted.
pub async fn run_self_owned_rendezvous_v1(
    carrier: IrohConfigV1,
    key: SigningKey,
    clients: BTreeSet<[u8; 32]>,
    stop: Arc<AtomicBool>,
    activate: impl FnOnce(&IrohServiceEndpointV1, u64) -> Result<Arc<ServiceLeaseHostV1>> + 'static,
    publish: impl Fn(&PeerSignedRelayRecordV1) -> Result<()> + 'static,
) -> Result<()> {
    ensure!(
        (1..=16).contains(&clients.len()) && !clients.contains(&key.verifying_key().to_bytes()),
        "local client resource policy"
    );
    for pin in &clients {
        ensure!(
            !ed25519_dalek::VerifyingKey::from_bytes(pin)?.is_weak(),
            "weak client key"
        );
    }
    let clients: Vec<_> = clients.into_iter().collect();
    let revoked = stop.clone();
    let result = with_iroh_owned_listener_v1(carrier, move || {
        ensure!(!revoked.load(Ordering::Acquire), "service stopped by owner"); Ok(())
    }, move |scope| Box::pin(async move {
        if scope.uses_relays() { tokio::time::timeout(Duration::from_secs(20), scope.online()).await??; }
        let mut endpoint = IrohServiceEndpointV1 { instance: rand::random(), address: scope.address()? };
        let expires_ms = now_ms()?.checked_add(90_000).ok_or_else(|| anyhow::anyhow!("service clock exhausted"))?;
        let host = activate(&endpoint, expires_ms)?;
        let first = host.rotate_self_owned(&endpoint, now_ms()?, expires_ms)?;
        publish(&first)?;
        let generation = Arc::new(Mutex::new(Generation {
            store: Arc::new(Mutex::new(RendezvousStoreV1::new(endpoint.instance, Instant::now() + Duration::from_secs(90))?)),
            expires_ms,
        }));
        let mut rotate_at = Instant::now() + Duration::from_secs(60);
        let mut active = FuturesUnordered::new();
        let mut incoming = Box::pin(scope.accept_inbound());
        let mut maintenance = tokio::time::interval(Duration::from_millis(200));
        maintenance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = maintenance.tick() => {
                    if stop.load(Ordering::Acquire) { break; }
                    let now = Instant::now();
                    if now >= rotate_at {
                        endpoint.instance = rand::random();
                        let expiry = now_ms()?.checked_add(90_000).ok_or_else(|| anyhow::anyhow!("service clock exhausted"))?;
                        let next = Arc::new(Mutex::new(RendezvousStoreV1::new(endpoint.instance, now + Duration::from_secs(90))?));
                        // One lock couples the durable signed instance and its
                        // store. Existing streams retain only their old bounded
                        // store and lease; its tombstones are never transplanted.
                        let mut current = generation.lock().map_err(|_| anyhow::anyhow!("generation unavailable"))?;
                        let record = host.rotate_self_owned(&endpoint, now_ms()?, expiry)?;
                        *current = Generation { store: next, expires_ms: expiry };
                        publish(&record)?;
                        rotate_at = now + Duration::from_secs(60);
                    }
                    host.check_route(endpoint.address.endpoint_key, endpoint.instance, now_ms()?)?;
                }
                Some(_) = active.next(), if !active.is_empty() => {}
                accepted = &mut incoming, if active.len() < 4 => {
                    incoming = Box::pin(scope.accept_inbound());
                    if let Ok(stream) = accepted {
                        let host = host.clone();
                        let generation = generation.clone();
                        let key = &key;
                        let clients = &clients;
                        active.push(tokio::time::timeout(Duration::from_secs(30), async move {
                            let mut stream: CarrierStreamV1<'_> = stream.into();
                            let store = tokio::time::timeout(Duration::from_secs(3), serve_service_offer_selected_v2(&mut stream, || {
                                let current = generation.lock().map_err(|_| anyhow::anyhow!("generation unavailable"))?;
                                Ok((host.offer_until(now_ms()?, current.expires_ms)?, current.store.clone()))
                            })).await??;
                            crate::discovery_control::serve_rendezvous(&mut stream, key, clients, store).await
                        }));
                    }
                }
            }
        }
        drop(active);
        Ok(())
    })).await;
    // Local explicit stop is normal. Other errors remain observable to the
    // owner, which must not silently restart corrupt or revoked histories.
    result
}
