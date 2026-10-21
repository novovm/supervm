//! Optional TCP listener borrowed by the existing self-owned node owner.
//! No independent store, identity authority, maintenance loop or task spawn.
use super::*;
use crate::{
    product_service_lease::{serve_self_owned_node_selected_v8, TlsServiceEndpointV1},
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
    ) -> Result<Self> {
        // Existing signed/native peer envelope; live resource limits are supplied
        // by the same node owner's shared adaptive gate, not by this maximum.
        Self::bind_with_capacity(listen, advertised, seed, stop, 64).await
    }
    pub async fn bind_with_capacity(
        listen: SocketAddr,
        mut advertised: SocketAddr,
        seed: Zeroizing<[u8; 32]>,
        stop: Arc<AtomicBool>,
        max_connections: u16,
    ) -> Result<Self> {
        let scope_stop = stop.clone();
        let scope = TlsScopeV1::new_owned_continuous_v1(seed, move || {
            ensure!(
                !scope_stop.load(Ordering::Acquire),
                "service stopped by owner"
            );
            Ok(())
        })?
        .with_connection_limit(usize::from(max_connections))?;
        let listener = TcpListener::bind(listen).await?;
        if advertised.port() == 0 {
            advertised.set_port(listener.local_addr()?.port());
        }
        let route = TlsServiceEndpointV1 {
            address: advertised,
            endpoint_key: scope.endpoint_key(),
            max_connections,
            native_mailbox_v8: false,
        };
        route.endpoint()?;
        Ok(Self {
            listener,
            scope,
            route,
            stop,
        })
    }

    /// Opt in before publication; legacy listeners keep their exact old record.
    /// The host's native capability must also be present and remains revocable.
    pub fn with_native_mailbox_v8(mut self) -> Self {
        self.route.native_mailbox_v8 = true;
        self
    }

    pub fn route(&self) -> &TlsServiceEndpointV1 {
        &self.route
    }

    pub(super) async fn serve(
        &self,
        key: &SigningKey,
        clients: &Option<Vec<[u8; 32]>>,
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
