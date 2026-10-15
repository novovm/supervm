//! Self-owned lifecycle for the EXISTING rendezvous V2 offer and authenticated
//! RPC. Stable listener; rotating opaque stores and resource-bound mailbox sessions.
use crate::{
    bound_session::ServicePeerPolicyV1,
    carrier_stream::CarrierStreamV1,
    iroh_transport::{
        with_iroh_owned_listener_v1, IrohConfigV1, IrohEndpointOwnerV1, IrohScopeV1,
        IROH_MAX_CONNECTIONS_V1,
    },
    opaque_rendezvous::RendezvousStoreV1,
    product_directory::PeerSignedRelayRecordV1,
    product_rendezvous_daemon::now_ms,
    product_service_lease::{
        serve_self_owned_node_selected_v4, IrohServiceEndpointV1, NativeMailboxEndpointV7,
        NodeServiceV1, ServiceLeaseHostV1,
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
#[path = "product_rendezvous_native.rs"]
mod native;
pub use native::NativeMailboxListenerV7;
#[cfg(feature = "tls-transport")]
#[path = "product_rendezvous_tls.rs"]
mod tls;
#[cfg(feature = "tls-transport")]
pub use tls::TlsMailboxListenerV1;
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
    let node = Node::new(key, clients, stop, mailbox, IROH_MAX_CONNECTIONS_V1)?;
    let revoked = node.stop.clone();
    with_iroh_owned_listener_v1(
        carrier,
        move || {
            ensure!(!revoked.load(Ordering::Acquire), "service stopped by owner");
            Ok(())
        },
        move |scope| Box::pin(node.serve(scope, activate, publish)),
    )
    .await
}

/// Optional TLS listener on the SAME node owner, signed locator, persistent
/// mailbox and connection budget. The platform must provide a reachable route.
#[cfg(feature = "tls-transport")]
#[allow(clippy::too_many_arguments)]
pub async fn run_self_owned_node_with_tls_v1(
    carrier: IrohConfigV1,
    key: SigningKey,
    clients: BTreeSet<[u8; 32]>,
    stop: Arc<AtomicBool>,
    mailbox: Arc<crate::mailbox_rpc::MailboxServiceV1>,
    tls: TlsMailboxListenerV1,
    activate: impl FnOnce(&IrohServiceEndpointV1, u64) -> Result<Arc<ServiceLeaseHostV1>> + 'static,
    publish: impl Fn(&PeerSignedRelayRecordV1) -> Result<()> + 'static,
) -> Result<()> {
    let mut node = Node::new(
        key,
        clients,
        stop,
        Some(mailbox),
        usize::from(tls.route().max_connections),
    )?;
    node.tls = Some(tls);
    let revoked = node.stop.clone();
    with_iroh_owned_listener_v1(
        carrier,
        move || {
            ensure!(!revoked.load(Ordering::Acquire), "service stopped by owner");
            Ok(())
        },
        move |scope| Box::pin(node.serve(scope, activate, publish)),
    )
    .await
}

/// The identical node dispatcher on an endpoint already held by the platform.
/// No bind, runtime, scheduler or detached cleanup is created here. The caller
/// must keep the endpoint owner alive, observe service termination, and close
/// that owner after all borrowers have ended. Endpoint ownership is not a
/// recipient capability or service identity; the original verifiers still run.
/// At most two inbound connections are serviced, leaving two of the shared
/// endpoint's four slots available for the device's own outbound work.
pub async fn run_self_owned_node_on_endpoint_v1(
    endpoint: &IrohEndpointOwnerV1,
    key: SigningKey,
    clients: BTreeSet<[u8; 32]>,
    stop: Arc<AtomicBool>,
    mailbox: Option<Arc<crate::mailbox_rpc::MailboxServiceV1>>,
    activate: impl FnOnce(&IrohServiceEndpointV1, u64) -> Result<Arc<ServiceLeaseHostV1>> + 'static,
    publish: impl Fn(&PeerSignedRelayRecordV1) -> Result<()> + 'static,
) -> Result<()> {
    let node = Node::new(key, clients, stop, mailbox, 2)?;
    node.on_endpoint(endpoint, activate, publish).await
}

/// Permissionless mailbox on the original node dispatcher and platform owner.
/// Unknown valid peer keys are accepted; mailbox capabilities, total resource
/// budgets and owner revocation still apply. Legacy rendezvous/mailbox services
/// are NOT silently opened by this opt-in: use their existing pinned entrypoint.
pub async fn run_self_owned_mailbox_on_endpoint_v1(
    endpoint: &IrohEndpointOwnerV1,
    key: SigningKey,
    stop: Arc<AtomicBool>,
    mailbox: Arc<crate::mailbox_rpc::MailboxServiceV1>,
    activate: impl FnOnce(&IrohServiceEndpointV1, u64) -> Result<Arc<ServiceLeaseHostV1>> + 'static,
    publish: impl Fn(&PeerSignedRelayRecordV1) -> Result<()> + 'static,
) -> Result<()> {
    Node {
        key,
        clients: None,
        stop,
        mailbox: Some(mailbox),
        max_connections: 2,
        native: None,
        #[cfg(feature = "tls-transport")]
        tls: None,
    }
    .on_endpoint(endpoint, activate, publish)
    .await
}

/// Original self-owned node dispatcher with an explicitly bound native mailbox.
/// Reuses the same custody service and node signing book as the carrier listeners.
/// Binding a UDP address alone does not claim NAT traversal or public reachability.
#[allow(clippy::too_many_arguments)]
pub async fn run_self_owned_node_with_native_v7(
    carrier: IrohConfigV1,
    key: SigningKey,
    clients: BTreeSet<[u8; 32]>,
    stop: Arc<AtomicBool>,
    mailbox: Arc<crate::mailbox_rpc::MailboxServiceV1>,
    native: NativeMailboxListenerV7,
    activate: impl FnOnce(&IrohServiceEndpointV1, u64) -> Result<Arc<ServiceLeaseHostV1>> + 'static,
    publish: impl Fn(&PeerSignedRelayRecordV1) -> Result<()> + 'static,
) -> Result<()> {
    ensure!(
        native.uses_stop(&stop),
        "native listener requires original owner stop"
    );
    ensure!(
        native.identity() == key.verifying_key().to_bytes(),
        "native node signer mismatch"
    );
    let mut node = Node::new(
        key,
        clients,
        stop,
        Some(mailbox),
        usize::from(native.route().max_connections),
    )?;
    node.native = Some(native);
    let revoked = node.stop.clone();
    with_iroh_owned_listener_v1(
        carrier,
        move || {
            ensure!(!revoked.load(Ordering::Acquire), "service stopped by owner");
            Ok(())
        },
        move |scope| Box::pin(node.serve(scope, activate, publish)),
    )
    .await
}

/// Native-only activation of the SAME self-owned node: no dummy Iroh endpoint,
/// Publisher, new mailbox store or JSON compatibility entrance is constructed.
/// The platform supplies its existing protected signer and durable history.
pub async fn run_self_owned_native_mailbox_v7(
    listener: NativeMailboxListenerV7,
    key: SigningKey,
    stop: Arc<AtomicBool>,
    mailbox: Arc<crate::mailbox_rpc::MailboxServiceV1>,
    activate: impl FnOnce(&NativeMailboxEndpointV7, u64) -> Result<Arc<ServiceLeaseHostV1>>,
    publish: impl Fn(&PeerSignedRelayRecordV1) -> Result<()>,
) -> Result<()> {
    ensure!(!stop.load(Ordering::Acquire), "service stopped by owner");
    ensure!(
        listener.uses_stop(&stop),
        "native listener requires original owner stop"
    );
    ensure!(
        listener.identity() == key.verifying_key().to_bytes()
            && mailbox.identity_public_key() == key.verifying_key().to_bytes(),
        "native node signer mismatch"
    );
    let route = listener.route().clone();
    let expiry = now_ms()?
        .checked_add(90_000)
        .ok_or_else(|| anyhow::anyhow!("service clock exhausted"))?;
    let host = activate(&route, expiry)?;
    let record = host.rotate_native_mailbox_v7(Some(&route), now_ms()?, expiry)?;
    ensure!(
        record.relay_public_key == key.verifying_key().to_bytes(),
        "native host signer mismatch"
    );
    mailbox.update_native_record_v7(record.clone())?;
    publish(&record)?;
    let serving = listener.serve(
        &host,
        &mailbox,
        Arc::new(tokio::sync::Semaphore::new(usize::from(
            route.max_connections,
        ))),
    );
    tokio::pin!(serving);
    let sweeping = async {
        loop {
            mailbox.sweep().await?;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };
    tokio::pin!(sweeping);
    let mut maintenance = tokio::time::interval(Duration::from_millis(200));
    maintenance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut rotate_at = Instant::now() + Duration::from_secs(60);
    loop {
        tokio::select! {
            result = &mut serving => return result,
            result = &mut sweeping => return result,
            _ = maintenance.tick() => {
                if stop.load(Ordering::Acquire) { return Ok(()); }
                let now = now_ms()?;
                host.check_continuous_native_peer_v7(now)?;
                if Instant::now() >= rotate_at {
                    let expiry = now.checked_add(90_000).ok_or_else(|| anyhow::anyhow!("service clock exhausted"))?;
                    let record = host.rotate_native_mailbox_v7(Some(&route), now, expiry)?;
                    mailbox.update_native_record_v7(record.clone())?;
                    publish(&record)?;
                    rotate_at = Instant::now() + Duration::from_secs(60);
                }
            }
        }
    }
}

struct Node {
    key: SigningKey,
    clients: Option<Vec<[u8; 32]>>,
    stop: Arc<AtomicBool>,
    mailbox: Option<Arc<crate::mailbox_rpc::MailboxServiceV1>>,
    max_connections: usize,
    native: Option<NativeMailboxListenerV7>,
    #[cfg(feature = "tls-transport")]
    tls: Option<TlsMailboxListenerV1>,
}

impl Node {
    async fn on_endpoint(
        self,
        endpoint: &IrohEndpointOwnerV1,
        activate: impl FnOnce(&IrohServiceEndpointV1, u64) -> Result<Arc<ServiceLeaseHostV1>> + 'static,
        publish: impl Fn(&PeerSignedRelayRecordV1) -> Result<()> + 'static,
    ) -> Result<()> {
        let revoked = self.stop.clone();
        endpoint
            .with_owned_listener_v1(
                move || {
                    ensure!(!revoked.load(Ordering::Acquire), "service stopped by owner");
                    Ok(())
                },
                move |scope| Box::pin(self.serve(scope, activate, publish)),
            )
            .await
    }

    fn new(
        key: SigningKey,
        clients: BTreeSet<[u8; 32]>,
        stop: Arc<AtomicBool>,
        mailbox: Option<Arc<crate::mailbox_rpc::MailboxServiceV1>>,
        max_connections: usize,
    ) -> Result<Self> {
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
        Ok(Self {
            key,
            clients: Some(clients.into_iter().collect()),
            stop,
            mailbox,
            max_connections,
            native: None,
            #[cfg(feature = "tls-transport")]
            tls: None,
        })
    }

    async fn serve(
        self,
        scope: &IrohScopeV1,
        activate: impl FnOnce(&IrohServiceEndpointV1, u64) -> Result<Arc<ServiceLeaseHostV1>>,
        publish: impl Fn(&PeerSignedRelayRecordV1) -> Result<()>,
    ) -> Result<()> {
        let Self {
            key,
            clients,
            stop,
            mailbox,
            max_connections,
            native,
            #[cfg(feature = "tls-transport")]
            tls,
        } = self;
        #[cfg(feature = "tls-transport")]
        let has_tls_listener = tls.is_some();
        #[cfg(not(feature = "tls-transport"))]
        let has_tls_listener = false;
        // A bound TCP listener must not wait for a separate relay registration.
        // The signed QUIC locator is a candidate, never proof that it is online.
        if scope.uses_relays() && !has_tls_listener && native.is_none() {
            tokio::time::timeout(Duration::from_secs(20), scope.online()).await??;
        }
        let mut endpoint = IrohServiceEndpointV1 {
            instance: rand::random(),
            address: scope.address()?,
        };
        let expires_ms = now_ms()?
            .checked_add(90_000)
            .ok_or_else(|| anyhow::anyhow!("service clock exhausted"))?;
        let host = activate(&endpoint, expires_ms)?;
        #[cfg(feature = "tls-transport")]
        let first = host.rotate_self_owned_with_tls_v1(
            &endpoint,
            tls.as_ref().map(TlsMailboxListenerV1::route),
            now_ms()?,
            expires_ms,
        )?;
        #[cfg(not(feature = "tls-transport"))]
        let first = host.rotate_self_owned_with_tls_v1(&endpoint, None, now_ms()?, expires_ms)?;
        // Reconcile the advertised native route with an actually bound listener.
        // Do not leave a previously published UDP route live after disabling it.
        let first = if native.is_some() || NativeMailboxEndpointV7::from_record(&first)?.is_some() {
            host.rotate_native_mailbox_v7(
                native.as_ref().map(NativeMailboxListenerV7::route),
                now_ms()?,
                expires_ms,
            )?
        } else {
            first
        };
        publish(&first)?;
        let slots = Arc::new(tokio::sync::Semaphore::new(max_connections));
        #[cfg(feature = "tls-transport")]
        let extra_serving = async {
            if let Some(tls) = &tls {
                let service = mailbox
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("TLS mailbox missing"))?;
                return tls
                    .serve(&key, &clients, &host, service, slots.clone())
                    .await;
            }
            std::future::pending::<Result<()>>().await
        };
        #[cfg(not(feature = "tls-transport"))]
        let extra_serving = std::future::pending::<Result<()>>();
        tokio::pin!(extra_serving);
        let native_serving = async {
            if let Some(native) = native {
                let service = mailbox
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("native mailbox missing"))?;
                return native.serve(&host, service, slots.clone()).await;
            }
            std::future::pending::<Result<()>>().await
        };
        tokio::pin!(native_serving);
        let generation = Arc::new(Mutex::new(Generation {
            store: Arc::new(Mutex::new(RendezvousStoreV1::new(
                endpoint.instance,
                Instant::now() + Duration::from_secs(90),
            )?)),
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
                        // The listener survives network changes. Re-signing its
                        // startup snapshot would preserve stale NAT/interface
                        // candidates forever. Read the SAME endpoint here; the
                        // host below still rejects a changed transport identity.
                        endpoint.address = scope.address()?;
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
                result = &mut native_serving => { result?; break; }
                result = &mut extra_serving => { result?; anyhow::bail!("TLS mailbox listener ended"); }
                Some(_) = active.next(), if !active.is_empty() => {}
                accepted = &mut incoming, if active.len() < max_connections => {
                    incoming = Box::pin(scope.accept_inbound());
                    if let Ok(stream) = accepted {
                        let Ok(permit) = slots.clone().try_acquire_owned() else { drop(stream); continue };
                        let host = host.clone();
                        let generation = generation.clone();
                        let key = &key;
                        let clients = &clients;
                        let mailbox_policy = clients.as_deref().map_or(
                            ServicePeerPolicyV1::AnyAuthenticated, ServicePeerPolicyV1::Pinned,
                        );
                        let mailbox=mailbox.clone();
                        let transport = endpoint.address.endpoint_key;
                        active.push(async move {
                            let _permit = permit;
                            let mut stream: CarrierStreamV1<'_> = stream.into();
                            loop {
                                let admitted_at = Instant::now();
                                    let (kind, selected) = tokio::time::timeout(Duration::from_secs(3), serve_self_owned_node_selected_v4(&mut stream, || {
                                        let current = generation.lock().map_err(|_| anyhow::anyhow!("generation unavailable"))?;
                                        let offer = host.offer_until(now_ms()?, current.expires_ms)?;
                                        let until = offer.lease.as_ref().map_or(0, |lease| lease.expires_at_ms);
                                        Ok((offer, (current.store.clone(), until)))
                                    }, || host.check_continuous_peer_v4(transport, now_ms()?))).await??;
                                if kind == NodeServiceV1::MailboxPrefetch {
                                    let mailbox = mailbox.as_ref().ok_or_else(||anyhow::anyhow!("mailbox contribution disabled"))?;
                                    return crate::mailbox_rpc::serve_mailbox_prefetch_with_policy_v6(
                                        stream, key, mailbox_policy, mailbox,
                                        || host.check_continuous_peer_v4(transport, now_ms()?),
                                    ).await;
                                }
                                if kind == NodeServiceV1::MailboxMultiplex {
                                    let mailbox = mailbox.as_ref().ok_or_else(||anyhow::anyhow!("mailbox contribution disabled"))?;
                                    return crate::mailbox_rpc::serve_mailbox_multiplex_with_policy_v5(
                                        stream, key, mailbox_policy, mailbox,
                                        || host.check_continuous_peer_v4(transport, now_ms()?),
                                    ).await;
                                }
                                if kind == NodeServiceV1::MailboxContinuous {
                                    let mailbox = mailbox.as_ref().ok_or_else(||anyhow::anyhow!("mailbox contribution disabled"))?;
                                    crate::mailbox_rpc::serve_mailbox_continuous_with_policy_v4(
                                        &mut stream, key, mailbox_policy, mailbox,
                                        || host.check_continuous_peer_v4(transport, now_ms()?),
                                    ).await?;
                                    break;
                                }
                                let clients = clients.as_deref().ok_or_else(|| anyhow::anyhow!("open mailbox requires NVMBOX04 or NVMBOX05"))?;
                                let (store, offer_until) = selected.ok_or_else(||anyhow::anyhow!("missing legacy service offer"))?;
                                let legacy_until = admitted_at + Duration::from_secs(30);
                                // The signed offer remains the authority. Its remaining wall
                                // interval is anchored once to monotonic time, before Finished.
                                // Neither requests nor node generation rotation can reset it.
                                let window_until = Instant::now() + Duration::from_millis(
                                    offer_until.saturating_sub(now_ms()?).min(120_000));
                                let until = if kind == NodeServiceV1::MailboxWindow { window_until } else { legacy_until };
                                let again = tokio::time::timeout_at(until.into(), async {
                                    match kind {
                                        NodeServiceV1::MailboxContinuous | NodeServiceV1::MailboxMultiplex | NodeServiceV1::MailboxPrefetch => anyhow::bail!("continuous service in legacy dispatch"),
                                        NodeServiceV1::Rendezvous => { crate::discovery_control::serve_rendezvous(&mut stream,key,clients,store).await?; },
                                        NodeServiceV1::Mailbox | NodeServiceV1::MailboxRealtime | NodeServiceV1::MailboxWindow => {
                                            let mailbox=mailbox.as_ref().ok_or_else(||anyhow::anyhow!("mailbox contribution disabled"))?;
                                            let check = || {
                                                    ensure!(now_ms()? < offer_until, "mailbox offered lease expired");
                                                    Ok(())
                                            };
                                            if kind == NodeServiceV1::MailboxWindow {
                                                crate::mailbox_rpc::serve_mailbox_window_v3(&mut stream,key,clients,mailbox,check,window_until).await?;
                                            } else if kind == NodeServiceV1::MailboxRealtime {
                                                crate::mailbox_rpc::serve_mailbox_session_v2(&mut stream,key,clients,mailbox,check).await?;
                                            } else {
                                                crate::mailbox_rpc::serve_mailbox_v1(&mut stream,key,clients,mailbox).await?;
                                            }
                                        }
                                    }
                                    Ok::<_,anyhow::Error>(matches!(kind,NodeServiceV1::MailboxRealtime | NodeServiceV1::MailboxWindow))
                                }).await??;
                                if !again { break; }
                                // Transport continuity only. The next stream receives a new
                                // short admission budget, signed offer and Finished authentication.
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
    }
}
