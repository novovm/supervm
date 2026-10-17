//! Optional TCP listener borrowed by the existing self-owned node owner.
//! No independent store, identity authority, maintenance loop or task spawn.
use super::*;
use crate::{
    product_service_lease::{serve_self_owned_node_selected_v4, TlsServiceEndpointV1},
    tls_transport::TlsScopeV1,
};
use std::net::SocketAddr;
use tokio::{net::TcpListener, sync::Semaphore};
use zeroize::Zeroizing;

pub struct TlsMailboxListenerV1 {
    listener: TcpListener,
    scope: TlsScopeV1,
    route: TlsServiceEndpointV1,
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
    pub async fn bind_with_capacity(
        listen: SocketAddr,
        mut advertised: SocketAddr,
        seed: Zeroizing<[u8; 32]>,
        stop: Arc<AtomicBool>,
        max_connections: u16,
    ) -> Result<Self> {
        let scope = TlsScopeV1::new_owned_continuous_v1(seed, move || {
            ensure!(!stop.load(Ordering::Acquire), "service stopped by owner");
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
        };
        route.endpoint()?;
        Ok(Self {
            listener,
            scope,
            route,
        })
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
        slots: Arc<Semaphore>,
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
                    active.push(async move {
                        let _permit = permit;
                        let check = || host.check_continuous_tls_peer_v1(&self.route, now_ms()?);
                        check()?;
                        let mut stream = CarrierStreamV1::from(self.scope.accept(tcp, None).await?);
                        let (kind, _): (_, Option<()>) = tokio::time::timeout(
                            Duration::from_secs(3),
                            serve_self_owned_node_selected_v4(&mut stream,
                                || anyhow::bail!("TLS listener requires NVMBOX06"), check),
                        ).await??;
                        ensure!(kind == NodeServiceV1::MailboxPrefetch, "TLS listener requires NVMBOX06");
                        crate::mailbox_rpc::serve_mailbox_prefetch_with_policy_v6(
                            stream, key, policy, service, check,
                        ).await
                    });
                }
            }
        }
    }
}
