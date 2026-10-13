//! Self-owned lifecycle for the EXISTING rendezvous V2 offer and authenticated
//! RPC. Stable listener; bounded rotating opaque stores and per-stream leases.
use crate::{
    carrier_stream::CarrierStreamV1,
    iroh_transport::{with_iroh_owned_listener_v1, IrohConfigV1},
    opaque_rendezvous::RendezvousStoreV1,
    product_directory::PeerSignedRelayRecordV1,
    product_rendezvous_daemon::now_ms,
    product_service_lease::{
        serve_node_offer_selected_v1, IrohServiceEndpointV1, NodeServiceV1, ServiceLeaseHostV1,
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
    run_self_owned_node_v1(carrier, key, clients, stop, None, activate, publish).await
}

/// Same stable listener/identity for L3 or L4. Platform owner decides resource
/// admission and supplies the shared persistent store; no chat server or epoch DB.
pub async fn run_self_owned_node_v1(
    carrier: IrohConfigV1,
    key: SigningKey,
    clients: BTreeSet<[u8; 32]>,
    stop: Arc<AtomicBool>,
    mailbox: Option<Arc<crate::mailbox_rpc::MailboxServiceV1>>,
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
        // Poll store maintenance alongside client futures. Awaiting sweep in
        // the timer branch can deadlock the fair storage semaphore: a queued
        // client is assigned its permit but cannot run until this loop polls it.
        // One borrowed future, no detached worker, overlapping sweeps or new owner.
        let sweep_service = mailbox.clone();
        let sweeping = async move {
            if let Some(service) = sweep_service {
                loop {
                    service.sweep().await?;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
            std::future::pending::<Result<()>>().await
        };
        tokio::pin!(sweeping);
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
                result = &mut sweeping => { result?; unreachable!("mailbox maintenance is continuous"); }
                Some(_) = active.next(), if !active.is_empty() => {}
                accepted = &mut incoming, if active.len() < 4 => {
                    incoming = Box::pin(scope.accept_inbound());
                    if let Ok(stream) = accepted {
                        let host = host.clone();
                        let generation = generation.clone();
                        let key = &key;
                        let clients = &clients;
                        let mailbox=mailbox.clone();
                        active.push(async move {
                            let mut stream: CarrierStreamV1<'_> = stream.into();
                            loop {
                                let again = tokio::time::timeout(Duration::from_secs(30), async {
                                    let (kind, (store, offer_until)) = tokio::time::timeout(Duration::from_secs(3), serve_node_offer_selected_v1(&mut stream, || {
                                        let current = generation.lock().map_err(|_| anyhow::anyhow!("generation unavailable"))?;
                                        let offer = host.offer_until(now_ms()?, current.expires_ms)?;
                                        let until = offer.lease.as_ref().map_or(0, |lease| lease.expires_at_ms);
                                        Ok((offer, (current.store.clone(), until)))
                                    })).await??;
                                    match kind {
                                        NodeServiceV1::Rendezvous => { crate::discovery_control::serve_rendezvous(&mut stream,key,clients,store).await?; },
                                        NodeServiceV1::Mailbox | NodeServiceV1::MailboxRealtime => {
                                            let mailbox=mailbox.as_ref().ok_or_else(||anyhow::anyhow!("mailbox contribution disabled"))?;
                                            if kind == NodeServiceV1::MailboxRealtime {
                                                crate::mailbox_rpc::serve_mailbox_session_v2(&mut stream,key,clients,mailbox,|| {
                                                    ensure!(now_ms()? < offer_until, "mailbox offered lease expired");
                                                    Ok(())
                                                }).await?;
                                            } else {
                                                crate::mailbox_rpc::serve_mailbox_v1(&mut stream,key,clients,mailbox).await?;
                                            }
                                        }
                                    }
                                    Ok::<_,anyhow::Error>(kind == NodeServiceV1::MailboxRealtime)
                                }).await??;
                                if !again { break; }
                                // Transport continuity only. The next stream receives a new
                                // 30s IO budget, signed offer and Finished authentication.
                                stream = stream.next_inbound().await?;
                            }
                            Ok::<_,anyhow::Error>(())
                        });
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
