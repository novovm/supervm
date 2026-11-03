//! Optional TCP listener borrowed by the existing self-owned node owner.
//! No independent store, identity authority, maintenance loop or task spawn.
use super::*;
use crate::{
    product_service_lease::{serve_self_owned_node_selected_v8, TlsServiceEndpointV1},
    service_concurrency::ServiceConcurrencyV1,
    tls_transport::TlsScopeV1,
};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use zeroize::Zeroizing;

pub struct TlsMailboxListenerV1 {
    listener: TcpListener,
    scope: TlsScopeV1,
    route: TlsServiceEndpointV1,
    stop: Arc<AtomicBool>,
    concurrency: Option<Arc<ServiceConcurrencyV1>>,
    connection_ceiling: u16,
}

impl TlsMailboxListenerV1 {
    /// The platform owns reachability/firewall configuration and protects this
    /// purpose-specific transport seed. Binding does not claim public reachability.
    pub async fn bind(
        listen: SocketAddr,
        advertised: SocketAddr,
        seed: Zeroizing<[u8; 32]>,
        stop: Arc<AtomicBool>,
    ) -> Result<Self> {
        Self::bind_with_capacity(listen, advertised, seed, stop, 4).await
    }
    pub async fn bind_adaptive(
        listen: SocketAddr,
        advertised: SocketAddr,
        seed: Zeroizing<[u8; 32]>,
        stop: Arc<AtomicBool>,
        concurrency: Arc<ServiceConcurrencyV1>,
    ) -> Result<Self> {
        Self::bind_with_resources(listen, advertised, seed, stop, u16::MAX, Some(concurrency)).await
    }
    pub async fn bind_adaptive_with_capacity(
        listen: SocketAddr,
        advertised: SocketAddr,
        seed: Zeroizing<[u8; 32]>,
        stop: Arc<AtomicBool>,
        max_connections: Option<u16>,
        concurrency: Arc<ServiceConcurrencyV1>,
    ) -> Result<Self> {
        Self::bind_with_resources(
            listen,
            advertised,
            seed,
            stop,
            max_connections.unwrap_or(u16::MAX),
            Some(concurrency),
        )
        .await
    }
    pub async fn bind_with_capacity(
        listen: SocketAddr,
        advertised: SocketAddr,
        seed: Zeroizing<[u8; 32]>,
        stop: Arc<AtomicBool>,
        max_connections: u16,
    ) -> Result<Self> {
        Self::bind_with_resources(listen, advertised, seed, stop, max_connections, None).await
    }
    async fn bind_with_resources(
        listen: SocketAddr,
        mut advertised: SocketAddr,
        seed: Zeroizing<[u8; 32]>,
        stop: Arc<AtomicBool>,
        max_connections: u16,
        concurrency: Option<Arc<ServiceConcurrencyV1>>,
    ) -> Result<Self> {
        let scope_stop = stop.clone();
        let mut scope = TlsScopeV1::new_owned_continuous_v1(seed, move || {
            ensure!(
                !scope_stop.load(Ordering::Acquire),
                "service stopped by owner"
            );
            Ok(())
        })?
        .with_connection_limit(usize::from(max_connections))?;
        if let Some(controller) = &concurrency {
            scope = scope
                .with_service_concurrency_limit(controller.clone(), usize::from(max_connections));
        }
        let listener = TcpListener::bind(listen).await?;
        if advertised.port() == 0 {
            advertised.set_port(listener.local_addr()?.port());
        }
        let route = TlsServiceEndpointV1 {
            address: advertised,
            endpoint_key: scope.endpoint_key(),
            max_connections: concurrency.as_ref().map_or(max_connections, |c| {
                advertised_capacity(c).min(max_connections)
            }),
            native_mailbox_v8: false,
        };
        route.endpoint()?;
        Ok(Self {
            listener,
            scope,
            route,
            stop,
            concurrency,
            connection_ceiling: max_connections,
        })
    }

    /// Select the native mailbox capability before publication. The host's
    /// matching capability must also be present and remains revocable.
    pub fn with_native_mailbox_v8(mut self) -> Self {
        self.route.native_mailbox_v8 = true;
        self
    }

    pub fn route(&self) -> &TlsServiceEndpointV1 {
        &self.route
    }

    pub(super) fn concurrency(&self) -> Option<Arc<ServiceConcurrencyV1>> {
        self.concurrency.clone()
    }

    pub(super) fn admission_ceiling(&self) -> usize {
        usize::from(self.connection_ceiling)
    }

    /// Capacity is an observation, not a reservation or a permanent ceiling.
    /// Re-sign this snapshot through the existing node declaration lifecycle.
    pub fn current_route(&self) -> TlsServiceEndpointV1 {
        let mut route = self.route.clone();
        if let Some(controller) = &self.concurrency {
            route.max_connections = advertised_capacity(controller).min(self.connection_ceiling);
        }
        route
    }

    pub(super) async fn serve(
        &self,
        key: &TransportIdentity,
        clients: &Option<Vec<Vec<u8>>>,
        host: &ServiceLeaseHostV1,
        service: &crate::mailbox_rpc::MailboxServiceV1,
        slots: Arc<AdaptiveAdmissionV1>,
    ) -> Result<()> {
        let policy = clients.as_deref().map_or(
            ServicePeerPolicyV1::AnyAuthenticated,
            ServicePeerPolicyV1::Pinned,
        );
        let mut active = FuturesUnordered::new();
        loop {
            tokio::select! {
                biased;
                Some(_) = active.next(), if !active.is_empty() => {}
                accepted = self.listener.accept() => {
                    let (tcp, _) = accepted?;
                    // One shared node-wide connection budget with QUIC. Do not
                    // queue unbounded unauthenticated TLS handshakes in memory.
                    let Ok(permit) = slots.clone().try_acquire_owned() else { drop(tcp); continue };
                    tcp.set_nodelay(true)?;
                    let concurrency = slots.controller();
                    active.push(async move {
                        let permit = Arc::new(permit);
                        let check = || host.check_continuous_tls_peer_v1(&self.route, now_ms()?);
                        check()?;
                        let mut stream = CarrierStreamV1::from(self.scope.accept(tcp, None).await?);
                        let admitted_at = Instant::now();
                        let (kind, _, clock): (_, Option<()>, _) = tokio::time::timeout(
                            Duration::from_secs(3),
                            serve_self_owned_node_selected_v8(&mut stream,
                                || anyhow::bail!("TLS listener requires explicit mailbox selector"), check,
                                || anyhow::bail!("TLS listener does not advertise NVMBOX07"),
                                || host.check_continuous_native_tls_v8(&self.route, now_ms()?)),
                        ).await??;
                        if kind == NodeServiceV1::MailboxNativeV8 {
                            return native::serve_carrier(
                                &mut stream, key, policy, service, self.stop.clone(), permit, concurrency,
                                || host.check_continuous_native_tls_v8(&self.route, now_ms()?),
                                native::NativeCarrierHandshake::SessionClockV8 {
                                    received: clock, deadline: admitted_at + Duration::from_secs(30),
                                },
                            ).await;
                        }
                        ensure!(kind == NodeServiceV1::MailboxPrefetch, "TLS listener requires NVMBOX06");
                        let _permit = permit;
                        crate::mailbox_rpc::serve_mailbox_prefetch_with_policy_v6(
                            stream, key, policy, service, check,
                        ).await
                    });
                }
            }
        }
    }
}

fn advertised_capacity(controller: &ServiceConcurrencyV1) -> u16 {
    controller.snapshot().inbound_connections() as u16
}

#[cfg(test)]
mod resource_tests {
    use super::*;
    use crate::service_concurrency::{ResourcePressureV1, ResourceSnapshotV1};

    #[tokio::test]
    async fn explicit_tls_capacity_bounds_same_adaptive_controller() -> Result<()> {
        let _serial = crate::tls_transport::TLS_TEST_SERIAL.acquire().await?;
        let resources = ResourceSnapshotV1 {
            cpu_parallelism: 64,
            available_memory_bytes: 16 * 1024 * 1024 * 1024,
            pressure: ResourcePressureV1::Normal,
        };
        let controller = ServiceConcurrencyV1::new(resources);
        let listener = TlsMailboxListenerV1::bind_adaptive_with_capacity(
            "127.0.0.1:0".parse()?,
            "127.0.0.1:0".parse()?,
            Zeroizing::new([39; 32]),
            Arc::new(AtomicBool::new(false)),
            Some(8),
            controller.clone(),
        )
        .await?;
        assert!(Arc::ptr_eq(&listener.concurrency().unwrap(), &controller));
        assert_eq!(listener.route().max_connections, 8);
        assert_eq!(listener.admission_ceiling(), 8);
        controller.update_resources(ResourceSnapshotV1 {
            pressure: ResourcePressureV1::Severe,
            ..resources
        });
        assert_eq!(listener.current_route().max_connections, 2);
        controller.update_resources(resources);
        assert_eq!(listener.current_route().max_connections, 8);
        let omitted = TlsMailboxListenerV1::bind_adaptive_with_capacity(
            "127.0.0.1:0".parse()?,
            "127.0.0.1:0".parse()?,
            Zeroizing::new([40; 32]),
            Arc::new(AtomicBool::new(false)),
            None,
            controller,
        )
        .await?;
        assert!(omitted.current_route().max_connections > 64);
        assert_eq!(omitted.admission_ceiling(), usize::from(u16::MAX));
        Ok(())
    }

    #[tokio::test]
    async fn tls_route_capacity_tracks_resources_without_changing_identity() -> Result<()> {
        let _serial = crate::tls_transport::TLS_TEST_SERIAL.acquire().await?;
        let resources = ResourceSnapshotV1 {
            cpu_parallelism: 64,
            available_memory_bytes: 16 * 1024 * 1024 * 1024,
            pressure: ResourcePressureV1::Normal,
        };
        let controller = ServiceConcurrencyV1::new(resources);
        let listener = TlsMailboxListenerV1::bind_adaptive(
            "127.0.0.1:0".parse()?,
            "127.0.0.1:0".parse()?,
            Zeroizing::new([38; 32]),
            Arc::new(AtomicBool::new(false)),
            controller.clone(),
        )
        .await?;
        let initial = listener.current_route();
        assert!(initial.max_connections > 64);
        assert_eq!(listener.admission_ceiling(), usize::from(u16::MAX));
        controller.update_resources(ResourceSnapshotV1 {
            pressure: ResourcePressureV1::Severe,
            ..resources
        });
        let reduced = listener.current_route();
        assert_eq!(reduced.max_connections, 2);
        assert_eq!(reduced.endpoint_key, initial.endpoint_key);
        assert_eq!(reduced.address, initial.address);
        controller.update_resources(resources);
        assert_eq!(listener.current_route(), initial);
        assert_eq!(controller.snapshot().active_work, 0);
        Ok(())
    }
}
