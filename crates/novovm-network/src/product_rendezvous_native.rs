//! Native mailbox socket borrowed by the existing node owner. No task spawn,
//! runtime, new store or parallel service protocol. Watch work is polled beside
//! packet progress, so it cannot hold the UDP receive loop hostage.
use crate::{
    mailbox_rpc::MailboxServiceV1,
    mailbox_structured::{
        native_session::{domain, MailboxResponse, NativeMailboxServer, REQUEST_DEADLINE, SLOTS},
        LIMITS, STREAM,
    },
    novorudp::{
        endpoint::{NativeEndpoint, NativeHandshakeBudget},
        fragments::ReassemblyLimits,
        object_link::{NativeLaneConfig, NativeObjectLink, NativeSendLimits, NativeSendPolicy},
        NovoRudpPacingProfile, NovoRudpWindowConfig,
    },
    product_rendezvous_daemon::now_ms,
    product_service_lease::{NativeMailboxEndpointV7, ServiceLeaseHostV1},
};
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use futures_util::{
    future::{AbortHandle, Abortable, Aborted, LocalBoxFuture},
    stream::FuturesUnordered,
    FutureExt, StreamExt,
};
use std::{
    collections::BTreeMap,
    net::{SocketAddr, UdpSocket},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const WORK_LIMIT: usize = 8;
const WAITING_WATCH_LIMIT: usize = 2;
const QUANTUM: usize = 32;
type WorkResult = (u64, std::result::Result<Result<MailboxResponse>, Aborted>);
struct Job {
    peer: usize,
    waiting_watch: bool,
    abort: AbortHandle,
    permit: Arc<OwnedSemaphorePermit>,
}
struct Pending {
    peer: usize,
    response: MailboxResponse,
    permit: Option<Arc<OwnedSemaphorePermit>>,
}

pub struct NativeMailboxListenerV7 {
    link: NativeObjectLink,
    readiness: tokio::net::UdpSocket,
    route: NativeMailboxEndpointV7,
    stop: Arc<AtomicBool>,
}
impl NativeMailboxListenerV7 {
    /// The caller advertises its actually reachable address. Binding does not
    /// establish NAT reachability. The original SDK presently has a cumulative
    /// identity capacity (<=64), not an evicting public-server peer table.
    pub fn bind(
        listen: SocketAddr,
        mut advertised: SocketAddr,
        key: SigningKey,
        stop: Arc<AtomicBool>,
        max_connections: u16,
    ) -> Result<Self> {
        ensure!(
            (1..=64).contains(&max_connections),
            "native connection bounds"
        );
        let socket = UdpSocket::bind(listen)?;
        socket.set_nonblocking(true)?;
        if advertised.port() == 0 {
            advertised.set_port(socket.local_addr()?.port());
        }
        let route = NativeMailboxEndpointV7 {
            address: advertised,
            max_connections,
        };
        route.endpoint()?;
        // The clone ONLY observes readiness through peek. NativeEndpoint remains
        // the sole receiver/sender and owns all datagram/session verification.
        let readiness = tokio::net::UdpSocket::from_std(socket.try_clone()?)?;
        let mut endpoint = NativeEndpoint::listen_clients(
            socket,
            key,
            usize::from(max_connections),
            vec![STREAM],
            NativeHandshakeBudget {
                lifetime: REQUEST_DEADLINE,
                retry: Duration::from_millis(500),
            },
        )?;
        endpoint.enable_client_reconnect_v1()?;
        let link = NativeObjectLink::new(
            endpoint,
            vec![NativeLaneConfig {
                stream: STREAM,
                domain: domain(),
                receive: ReassemblyLimits {
                    max_message_bytes: LIMITS.wire_bytes,
                    messages: WORK_LIMIT,
                    bytes: WORK_LIMIT * LIMITS.wire_bytes,
                    peer_messages: SLOTS,
                    peer_bytes: SLOTS * LIMITS.wire_bytes,
                    ttl: REQUEST_DEADLINE,
                },
                send: NativeSendLimits {
                    messages: WORK_LIMIT,
                    bytes: WORK_LIMIT * LIMITS.wire_bytes,
                    peer_messages: SLOTS,
                    peer_bytes: SLOTS * LIMITS.wire_bytes,
                },
            }],
            NativeSendPolicy {
                window: NovoRudpWindowConfig::default(),
                pacing: NovoRudpPacingProfile::default(),
                ttl: REQUEST_DEADLINE,
            },
        )?;
        Ok(Self {
            link,
            readiness,
            route,
            stop,
        })
    }

    pub(super) fn uses_stop(&self, stop: &Arc<AtomicBool>) -> bool {
        Arc::ptr_eq(&self.stop, stop)
    }

    pub fn identity(&self) -> [u8; 32] {
        self.link.local_identity()
    }

    pub fn route(&self) -> &NativeMailboxEndpointV7 {
        &self.route
    }

    pub(super) async fn serve(
        self,
        host: &ServiceLeaseHostV1,
        service: &MailboxServiceV1,
        slots: Arc<Semaphore>,
    ) -> Result<()> {
        let Self {
            mut link,
            readiness,
            stop,
            ..
        } = self;
        ensure!(
            link.local_identity() == service.identity_public_key(),
            "native listener and mailbox signer differ"
        );
        let authority = || {
            ensure!(!stop.load(Ordering::Acquire), "service stopped by owner");
            host.check_continuous_native_peer_v7(now_ms()?)
        };
        let mut servers = BTreeMap::<usize, NativeMailboxServer>::new();
        let mut jobs = BTreeMap::<u64, Job>::new();
        let mut active: FuturesUnordered<LocalBoxFuture<'_, WorkResult>> = FuturesUnordered::new();
        let mut pending = Vec::<Pending>::new();
        let mut next_job = 0u64;
        let mut outbound = 0usize;
        // Full datagram buffer avoids WSAEMSGSIZE on Windows when peeking.
        let mut peek = vec![0u8; 65_536];
        loop {
            // Network input cannot starve already admitted disk/Watch work or
            // the enclosing Application owner during a continuous packet flood.
            tokio::task::consume_budget().await;
            if let Some(Some((id, result))) = active.next().now_or_never() {
                if let Some(job) = jobs.remove(&id) {
                    if let Ok(Ok(response)) = result {
                        pending.push(Pending {
                            peer: job.peer,
                            response,
                            permit: Some(job.permit),
                        });
                    }
                }
            }
            if stop.load(Ordering::Acquire) {
                return Ok(());
            }
            authority()?;
            let now = Instant::now();
            let report = link.poll(now_ms()?, now, QUANTUM)?;
            outbound = outbound.saturating_sub(report.completed.len());
            // Retire application bindings BEFORE accepting completed new-session
            // objects. An old committed operation has an unknown remote outcome;
            // cancellation must never undo its persistent custody obligation.
            for changed in report.session_changes {
                servers.remove(&changed.peer);
                for job in jobs.values().filter(|job| job.peer == changed.peer) {
                    job.abort.abort();
                }
                pending.retain(|item| item.peer != changed.peer);
            }
            let mut progress = false;
            let mut index = 0;
            while index < pending.len() {
                match pending[index].response.try_send(&mut link, now, authority) {
                    Ok(false) => index += 1,
                    Ok(true) => {
                        pending.swap_remove(index);
                        outbound += 1;
                        progress = true;
                    }
                    Err(_) => {
                        pending.swap_remove(index);
                    }
                }
            }
            // Never take an assembled object without capacity for its work AND
            // eventual response. Native receive/send retention has separate bounds.
            if active.len() + pending.len() < WORK_LIMIT {
                if let Some(body) = link.poll_complete(STREAM, now, QUANTUM)? {
                    progress = true;
                    let peer = (0..link.peer_count())
                        .find(|&p| link.peer_id(p).is_ok_and(|id| id == body.peer))
                        .context("native completed unknown peer")?;
                    // Always admit/validate before returning an overload reply.
                    // Transport completion alone must not strand a valid request
                    // when a different carrier currently owns the shared permits.
                    if let std::collections::btree_map::Entry::Vacant(entry) = servers.entry(peer) {
                        if let Ok(server) = NativeMailboxServer::attach(&mut link, peer) {
                            entry.insert(server);
                        }
                    }
                    if let Some(server) = servers.get_mut(&peer) {
                        if let Ok(work) = server.admit(&link, body, now, authority) {
                            let waiting_watch = work.has_waiting_watch();
                            let watch_saturated = waiting_watch
                                && (jobs.values().filter(|job| job.waiting_watch).count()
                                    >= WAITING_WATCH_LIMIT
                                    || jobs
                                        .values()
                                        .any(|job| job.peer == peer && job.waiting_watch));
                            let permit = jobs
                                .values()
                                .find(|job| job.peer == peer)
                                .map(|job| job.permit.clone())
                                .or_else(|| {
                                    pending
                                        .iter()
                                        .find(|item| item.peer == peer && item.permit.is_some())
                                        .and_then(|item| item.permit.clone())
                                })
                                .or_else(|| slots.clone().try_acquire_owned().ok().map(Arc::new));
                            if watch_saturated || permit.is_none() {
                                // At most two waiting watches, one per peer. Other
                                // slots remain available for Put/Fetch/Consume.
                                if let Ok(response) = work.reject_quota(service, authority) {
                                    pending.push(Pending {
                                        peer,
                                        response,
                                        permit: None,
                                    });
                                }
                            } else if let Some(permit) = permit {
                                next_job = next_job
                                    .checked_add(1)
                                    .context("native job sequence exhausted")?;
                                let id = next_job;
                                let (abort, registration) = AbortHandle::new_pair();
                                jobs.insert(
                                    id,
                                    Job {
                                        peer,
                                        waiting_watch,
                                        abort,
                                        permit,
                                    },
                                );
                                active.push(
                                    async move {
                                        (
                                            id,
                                            Abortable::new(
                                                work.execute(service, authority),
                                                registration,
                                            )
                                            .await,
                                        )
                                    }
                                    .boxed_local(),
                                );
                            }
                        }
                    }
                }
            }
            // Flush newly queued responses/ACKs immediately. This bounded progress
            // path is not a retry spin; idle work goes through socket/timer wakeup.
            if progress {
                continue;
            }
            let maintenance = if outbound == 0 {
                Duration::from_millis(200)
            } else {
                Duration::from_millis(10)
            };
            tokio::select! {
                biased;
                Some((id, result)) = active.next(), if !active.is_empty() => {
                    if let Some(job) = jobs.remove(&id) {
                        if let Ok(Ok(response)) = result {
                            pending.push(Pending { peer: job.peer, response, permit: Some(job.permit) });
                        }
                    }
                }
                readable = readiness.peek_from(&mut peek) => {
                    // Windows may surface stale UDP ICMP errors on this clone;
                    // NativeEndpoint classifies/consumes the transport condition.
                    if let Err(error) = readable {
                        match error.kind() {
                            std::io::ErrorKind::ConnectionReset |
                            std::io::ErrorKind::ConnectionRefused |
                            std::io::ErrorKind::Interrupted => {},
                            _ => return Err(error.into()),
                        }
                    }
                }
                _ = tokio::time::sleep(maintenance) => {}
            }
        }
    }
}
