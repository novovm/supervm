//! Self-owned lifecycle for the EXISTING rendezvous V2 offer and authenticated
//! RPC. Stable listener; rotating opaque stores and resource-bound mailbox sessions.
use crate::service_concurrency::{AdaptiveAdmissionV1, AdmissionKind};
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
        serve_self_owned_node_selected_v8, IrohServiceEndpointV1, NativeCarrierMailboxEndpointV7,
        NativeCarrierMailboxEndpointV8, NativeMailboxEndpointV7, NodeServiceV1, ServiceLeaseHostV1,
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

fn check_shared_controller(
    owner: Option<&Arc<crate::service_concurrency::ServiceConcurrencyV1>>,
    listener: Option<&Arc<crate::service_concurrency::ServiceConcurrencyV1>>,
) -> Result<()> {
    if let Some(listener) = listener {
        ensure!(
            owner.is_some_and(|owner| Arc::ptr_eq(owner, listener)),
            "adaptive listener requires the original owner's resource controller"
        );
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn adaptive_listener_rejects_missing_or_different_owner_controller() {
    use crate::service_concurrency::{
        ResourcePressureV1, ResourceSnapshotV1, ServiceConcurrencyV1,
    };
    let resources = ResourceSnapshotV1 {
        cpu_parallelism: 8,
        available_memory_bytes: 1024 * 1024 * 1024,
        pressure: ResourcePressureV1::Normal,
    };
    let a = ServiceConcurrencyV1::new(resources);
    let b = ServiceConcurrencyV1::new(resources);
    assert!(check_shared_controller(None, Some(&a)).is_err());
    assert!(check_shared_controller(Some(&a), Some(&b)).is_err());
    assert!(check_shared_controller(Some(&a), Some(&a)).is_ok());
    assert!(check_shared_controller(Some(&a), None).is_ok());
}
#[path = "product_rendezvous_native.rs"]
mod native;
pub use native::{NativeMailboxListenerV7, NativeMailboxResourcesUnavailableV1};
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
    let node = Node::new(
        key,
        clients,
        stop,
        mailbox,
        if carrier.service_concurrency().is_some() {
            usize::from(u16::MAX)
        } else {
            IROH_MAX_CONNECTIONS_V1
        },
    )?;
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
    let mut node = Node::new(key, clients, stop, Some(mailbox), tls.admission_ceiling())?;
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
/// A resource-managed endpoint shares the device's current service budget.
/// Explicit fixed scopes retain their caller-selected admission policy.
pub async fn run_self_owned_node_on_endpoint_v1(
    endpoint: &IrohEndpointOwnerV1,
    key: SigningKey,
    clients: BTreeSet<[u8; 32]>,
    stop: Arc<AtomicBool>,
    mailbox: Option<Arc<crate::mailbox_rpc::MailboxServiceV1>>,
    activate: impl FnOnce(&IrohServiceEndpointV1, u64) -> Result<Arc<ServiceLeaseHostV1>> + 'static,
    publish: impl Fn(&PeerSignedRelayRecordV1) -> Result<()> + 'static,
) -> Result<()> {
    let node = Node::new(
        key,
        clients,
        stop,
        mailbox,
        if endpoint.service_concurrency().is_some() {
            usize::from(u16::MAX)
        } else {
            2
        },
    )?;
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
    run_self_owned_mailbox_on_endpoint_with_native_v7(
        endpoint, key, stop, mailbox, None, activate, publish,
    )
    .await
}

/// The same permissionless L4 dispatcher with an optional already-bound native
/// UDP listener. Both carriers use the original signer, signed locator and
/// custody service; the platform still owns endpoint lifetime and shutdown.
/// Native admission shares the inbound-work budget rather than reserving
/// another independent budget. This does not publish an Iroh NAT mapping as a
/// native UDP route: the caller must supply the native socket's actual address.
#[allow(clippy::too_many_arguments)]
pub async fn run_self_owned_mailbox_on_endpoint_with_native_v7(
    endpoint: &IrohEndpointOwnerV1,
    key: SigningKey,
    stop: Arc<AtomicBool>,
    mailbox: Arc<crate::mailbox_rpc::MailboxServiceV1>,
    native: Option<NativeMailboxListenerV7>,
    activate: impl FnOnce(&IrohServiceEndpointV1, u64) -> Result<Arc<ServiceLeaseHostV1>> + 'static,
    publish: impl Fn(&PeerSignedRelayRecordV1) -> Result<()> + 'static,
) -> Result<()> {
    ensure!(
        mailbox.identity_public_key() == key.verifying_key().to_bytes(),
        "mailbox node signer mismatch"
    );
    let mut max_connections = if endpoint.service_concurrency().is_some() {
        usize::from(u16::MAX)
    } else {
        2
    };
    if let Some(listener) = &native {
        ensure!(
            listener.uses_stop(&stop),
            "native listener requires original owner stop"
        );
        ensure!(
            listener.identity() == key.verifying_key().to_bytes(),
            "native node signer mismatch"
        );
        max_connections = max_connections.min(listener.admission_ceiling());
    }
    Node {
        key,
        clients: None,
        stop,
        mailbox: Some(mailbox),
        max_connections,
        native,
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
    let node = native_node(key, clients, stop, mailbox, native)?;
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

/// Platform composition of the existing listeners, sharing one Node, signing
/// book, mailbox and admission budget. TLS is retained during native takeover.
#[cfg(feature = "tls-transport")]
#[allow(clippy::too_many_arguments)]
pub async fn run_self_owned_node_with_native_tls_v8(
    carrier: IrohConfigV1,
    key: SigningKey,
    clients: BTreeSet<[u8; 32]>,
    stop: Arc<AtomicBool>,
    mailbox: Arc<crate::mailbox_rpc::MailboxServiceV1>,
    native: NativeMailboxListenerV7,
    tls: Option<TlsMailboxListenerV1>,
    activate: impl FnOnce(&IrohServiceEndpointV1, u64) -> Result<Arc<ServiceLeaseHostV1>> + 'static,
    publish: impl Fn(&PeerSignedRelayRecordV1) -> Result<()> + 'static,
) -> Result<()> {
    let mut node = native_node(key, clients, stop, mailbox, native)?;
    if let Some(tls) = tls {
        // Neither configured resource ceiling is silently raised by composing
        // listeners. Active connections share the same semaphore in Node::serve.
        node.max_connections = node.max_connections.min(tls.admission_ceiling());
        node.tls = Some(tls);
    }
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

fn native_node(
    key: SigningKey,
    clients: BTreeSet<[u8; 32]>,
    stop: Arc<AtomicBool>,
    mailbox: Arc<crate::mailbox_rpc::MailboxServiceV1>,
    native: NativeMailboxListenerV7,
) -> Result<Node> {
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
        native.admission_ceiling(),
    )?;
    node.native = Some(native);
    Ok(node)
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
    let mut route = listener.current_route();
    let admission_ceiling = listener.admission_ceiling();
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
    let concurrency = listener.concurrency();
    let serving = listener.serve(
        &host,
        &mailbox,
        AdaptiveAdmissionV1::new(
            usize::from(route.max_connections),
            concurrency.clone(),
            AdmissionKind::Inbound,
            admission_ceiling,
        ),
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
                if let Some(c) = &concurrency { c.refresh_host(); }
                let now = now_ms()?;
                host.check_continuous_native_peer_v7(now)?;
                if Instant::now() >= rotate_at {
                    if let Some(c) = &concurrency {
                        let snapshot = c.snapshot();
                        route.max_connections = snapshot.inbound_connections() as u16;
                        if snapshot.wire_bytes_per_minute != 0 {
                            route.max_bytes_per_minute = snapshot.wire_bytes_per_minute;
                        }
                    }
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
        let concurrency = scope.service_concurrency();
        check_shared_controller(
            concurrency.as_ref(),
            native.as_ref().and_then(|n| n.concurrency()).as_ref(),
        )?;
        #[cfg(feature = "tls-transport")]
        check_shared_controller(
            concurrency.as_ref(),
            tls.as_ref().and_then(|t| t.concurrency()).as_ref(),
        )?;
        #[cfg(feature = "tls-transport")]
        let has_other_listener = native.is_some() || tls.is_some();
        #[cfg(not(feature = "tls-transport"))]
        let has_other_listener = native.is_some();
        // Relay unavailability is not an owner failure. A local candidate can
        // already accept authenticated LAN peers; the same endpoint maintains
        // its relay connection in parallel. No fabricated/empty route is signed.
        let mut endpoint = IrohServiceEndpointV1 {
            instance: rand::random(),
            address: scope.available_address(has_other_listener).await?,
        };
        let expires_ms = now_ms()?
            .checked_add(90_000)
            .ok_or_else(|| anyhow::anyhow!("service clock exhausted"))?;
        let host = activate(&endpoint, expires_ms)?;
        let native_adaptive = native.as_ref().is_some_and(|n| n.concurrency().is_some());
        let mut native_route = native.as_ref().map(NativeMailboxListenerV7::current_route);
        #[cfg(feature = "tls-transport")]
        let tls_route = tls.as_ref().map(TlsMailboxListenerV1::current_route);
        #[cfg(not(feature = "tls-transport"))]
        let tls_route = None;
        let first = if let Some(c) = &concurrency {
            host.rotate_self_owned_resources_v1(
                &endpoint,
                tls_route.as_ref(),
                native_route.as_ref(),
                c.snapshot().inbound_connections().min(max_connections) as u16,
                now_ms()?,
                expires_ms,
            )?
        } else {
            host.rotate_self_owned_with_tls_v1(
                &endpoint,
                tls_route.as_ref(),
                now_ms()?,
                expires_ms,
            )?
        };
        // Reconcile the advertised native route with an actually bound listener.
        // Do not leave a previously published UDP route live after disabling it.
        let first = if concurrency.is_none()
            && (native.is_some() || NativeMailboxEndpointV7::from_record(&first)?.is_some())
        {
            host.rotate_native_mailbox_v7(
                native.as_ref().map(NativeMailboxListenerV7::route),
                now_ms()?,
                expires_ms,
            )?
        } else {
            first
        };
        ensure!(
            mailbox.is_some()
                || (NativeCarrierMailboxEndpointV7::from_record(&first)?.is_none()
                    && NativeCarrierMailboxEndpointV8::from_record(&first)?.is_none()),
            "native mailbox capability without custody service"
        );
        if let Some(service) = &mailbox {
            if NativeCarrierMailboxEndpointV8::from_record(&first)?.is_some() {
                service.update_native_record_v8(first.clone())?;
            } else if NativeMailboxEndpointV7::from_record(&first)?.is_some()
                || NativeCarrierMailboxEndpointV7::from_record(&first)?.is_some()
            {
                // Publish the exact already-journaled signed record through the
                // original native locator operation, not a second directory.
                service.update_native_record_v7(first.clone())?;
            }
        }
        publish(&first)?;
        let slots = AdaptiveAdmissionV1::new(
            max_connections,
            concurrency.clone(),
            AdmissionKind::Inbound,
            max_connections,
        );
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
        let mut native_running = true;
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
                    if let Some(c) = &concurrency { c.refresh_host(); }
                    let now = Instant::now();
                    if now >= rotate_at {
                        // The listener survives network changes. Re-signing its
                        // startup snapshot would preserve stale NAT/interface
                        // candidates forever. Read the SAME endpoint here; the
                        // host below still rejects a changed transport identity.
                        let address = scope.address()?;
                        if address.direct.is_empty() && address.relays.is_empty() {
                            // Keep the listener, signer and durable custody alive
                            // while no interface is usable. Do not re-sign stale
                            // addresses or extend old locator/request lifetimes.
                            host.check_route(endpoint.address.endpoint_key, endpoint.instance, now_ms()?)?;
                            continue;
                        }
                        endpoint.address = address;
                        endpoint.instance = rand::random();
                        let expiry = now_ms()?.checked_add(90_000).ok_or_else(|| anyhow::anyhow!("service clock exhausted"))?;
                        let next = Arc::new(Mutex::new(RendezvousStoreV1::new(endpoint.instance, now + Duration::from_secs(90))?));
                        // One lock couples the durable signed instance and its
                        // store. Existing streams retain only their old bounded
                        // store and lease; its tombstones are never transplanted.
                        let mut current = generation.lock().map_err(|_| anyhow::anyhow!("generation unavailable"))?;
                        let record = if let Some(c) = &concurrency {
                            let snapshot = c.snapshot();
                            let capacity = snapshot.inbound_connections().min(max_connections) as u16;
                            if native_adaptive {
                                if let Some(route) = &mut native_route {
                                    route.max_connections = capacity;
                                    if snapshot.wire_bytes_per_minute != 0 {
                                        route.max_bytes_per_minute = snapshot.wire_bytes_per_minute;
                                    }
                                }
                            }
                            #[cfg(feature = "tls-transport")]
                            let tls_route = tls.as_ref().map(TlsMailboxListenerV1::current_route);
                            #[cfg(not(feature = "tls-transport"))]
                            let tls_route = None;
                            host.rotate_self_owned_resources_v1(&endpoint, tls_route.as_ref(),
                                native_route.as_ref(), capacity, now_ms()?, expiry)?
                        } else {
                            host.rotate_self_owned(&endpoint, now_ms()?, expiry)?
                        };
                        *current = Generation { store: next, expires_ms: expiry };
                        if let Some(service) = &mailbox {
                            if NativeCarrierMailboxEndpointV8::from_record(&record)?.is_some() {
                                service.update_native_record_v8(record.clone())?;
                            } else if NativeMailboxEndpointV7::from_record(&record)?.is_some()
                                || NativeCarrierMailboxEndpointV7::from_record(&record)?.is_some()
                            {
                                service.update_native_record_v7(record.clone())?;
                            }
                        }
                        publish(&record)?;
                        rotate_at = now + Duration::from_secs(60);
                    }
                    host.check_route(endpoint.address.endpoint_key, endpoint.instance, now_ms()?)?;
                }
                result = &mut sweeping => { result?; unreachable!("mailbox maintenance is continuous"); }
                result = &mut native_serving, if native_running => {
                    match result {
                        Err(error) if native::listener_io_unavailable(&error) => {
                            // A failed local UDP path must not terminate healthy
                            // carrier sessions or custody. Withdraw it durably
                            // before publishing the surviving service. The
                            // original platform owner may bind a fresh listener
                            // on its next network generation; no retry owner here.
                            native_running = false;
                            native_route = None;
                            let current = generation.lock().map_err(|_| anyhow::anyhow!("generation unavailable"))?;
                            let record = host.rotate_native_mailbox_v7(None, now_ms()?, current.expires_ms)?;
                            if let Some(service) = &mailbox {
                                if NativeCarrierMailboxEndpointV8::from_record(&record)?.is_some() {
                                    service.update_native_record_v8(record.clone())?;
                                } else if NativeCarrierMailboxEndpointV7::from_record(&record)?.is_some() {
                                    service.update_native_record_v7(record.clone())?;
                                }
                            }
                            publish(&record)?;
                        }
                        result => { result?; break; }
                    }
                }
                result = &mut extra_serving => { result?; anyhow::bail!("TLS mailbox listener ended"); }
                Some(_) = active.next(), if !active.is_empty() => {}
                accepted = &mut incoming, if active.len() < slots.limit() => {
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
                        let native_stop = stop.clone();
                        let concurrency = concurrency.clone();
                        active.push(async move {
                            let permit = Arc::new(permit);
                            let mut stream: CarrierStreamV1<'_> = stream.into();
                            loop {
                                let admitted_at = Instant::now();
                                    let (kind, selected, clock) = tokio::time::timeout(Duration::from_secs(3), serve_self_owned_node_selected_v8(&mut stream, || {
                                        let current = generation.lock().map_err(|_| anyhow::anyhow!("generation unavailable"))?;
                                        let offer = host.offer_until(now_ms()?, current.expires_ms)?;
                                        let until = offer.lease.as_ref().map_or(0, |lease| lease.expires_at_ms);
                                        Ok((offer, (current.store.clone(), until)))
                                    }, || host.check_continuous_peer_v4(transport, now_ms()?),
                                    || host.check_continuous_native_carrier_v7(transport, now_ms()?),
                                    || host.check_continuous_native_carrier_v8(transport, now_ms()?))).await??;
                                if matches!(kind, NodeServiceV1::MailboxNative | NodeServiceV1::MailboxNativeV8) {
                                    let mailbox = mailbox.as_ref().ok_or_else(||anyhow::anyhow!("mailbox contribution disabled"))?;
                                    let handshake = if kind == NodeServiceV1::MailboxNativeV8 {
                                        native::NativeCarrierHandshake::SessionClockV8 { received: clock, deadline: admitted_at + Duration::from_secs(30) }
                                    } else { native::NativeCarrierHandshake::UtcV7 };
                                    return native::serve_carrier(
                                        &mut stream, key, mailbox_policy, mailbox, native_stop, permit, concurrency,
                                        || if kind == NodeServiceV1::MailboxNativeV8 {
                                            host.check_continuous_native_carrier_v8(transport, now_ms()?)
                                        } else { host.check_continuous_native_carrier_v7(transport, now_ms()?) }, handshake,
                                    ).await;
                                }
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
                                        NodeServiceV1::MailboxNative | NodeServiceV1::MailboxNativeV8 | NodeServiceV1::MailboxContinuous | NodeServiceV1::MailboxMultiplex | NodeServiceV1::MailboxPrefetch => anyhow::bail!("continuous service in legacy dispatch"),
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
