//! Native mailbox transport borrowed by the existing node owner. No task spawn,
//! runtime, new store or parallel service protocol. Watch work is polled beside
//! packet progress, so it cannot hold the transport receive loop hostage.
use crate::service_concurrency::{
    AdaptiveAdmissionV1, ServiceConcurrencyV1, ServiceTransportPermitV1, ServiceWorkPermitV1,
};
use crate::{
    bound_session::{authenticate_structured_service_client_observed, ServicePeerPolicyV1},
    bound_session::{
        peer_time::send_peer_time_v8,
        session_clock::authenticate_structured_service_client_v2_observed,
    },
    carrier_stream::CarrierStreamV1,
    duplex::HandshakeReplayCacheV1,
    mailbox_rpc::MailboxServiceV1,
    mailbox_structured::{
        native_session::{domain, MailboxResponse, NativeMailboxServer, REQUEST_DEADLINE, SLOTS},
        BULK_LIMITS as LIMITS, STREAM,
    },
    network_time::ClockAnchorV1,
    novorudp::{
        carrier::NativeCarrierIoFailure,
        carrier_io::NativeCarrierPump,
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
use tokio::sync::OwnedSemaphorePermit;

const WORK_LIMIT: usize = 8;
const WAITING_WATCH_LIMIT: usize = 2;
const QUANTUM: usize = 32;
type WorkResult = (u64, std::result::Result<Result<MailboxResponse>, Aborted>);
struct Job {
    peer: usize,
    waiting_watch: bool,
    abort: AbortHandle,
    permit: Arc<OwnedSemaphorePermit>,
    work_permit: Option<ServiceWorkPermitV1>,
}
struct Pending {
    peer: usize,
    response: MailboxResponse,
    permit: Option<Arc<OwnedSemaphorePermit>>,
    work_permit: Option<ServiceWorkPermitV1>,
}

enum Readiness<'a> {
    Udp(tokio::net::UdpSocket),
    Carrier(NativeCarrierPump<'a>),
}

/// Local memory observation only. It carries no peer text and cannot relax
/// identity, permissions, protocol validity or storage guarantees.
#[derive(Debug, thiserror::Error)]
#[error("native mailbox resources temporarily unavailable")]
pub struct NativeMailboxResourcesUnavailableV1;

fn require_mailbox_frame_budget(bytes: usize) -> Result<()> {
    if bytes < LIMITS.wire_bytes {
        return Err(NativeMailboxResourcesUnavailableV1.into());
    }
    Ok(())
}

/// Applied only at the UDP IO boundary, never to authority or storage errors.
#[derive(Debug, thiserror::Error)]
#[error("native UDP listener unavailable: {0:?}")]
struct ListenerIoUnavailable(std::io::ErrorKind);

fn mark_udp_io_failure(error: anyhow::Error) -> anyhow::Error {
    let kind = error
        .downcast_ref::<std::io::Error>()
        .map(std::io::Error::kind)
        .or_else(|| {
            error
                .downcast_ref::<NativeCarrierIoFailure>()
                .map(|e| e.0.kind())
        });
    match kind {
        Some(
            kind @ (std::io::ErrorKind::NetworkUnreachable
            | std::io::ErrorKind::HostUnreachable
            | std::io::ErrorKind::NetworkDown
            | std::io::ErrorKind::AddrNotAvailable
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::TimedOut),
        ) => error.context(ListenerIoUnavailable(kind)),
        _ => error,
    }
}

pub(super) fn listener_io_unavailable(error: &anyhow::Error) -> bool {
    error.downcast_ref::<ListenerIoUnavailable>().is_some()
}

impl Readiness<'_> {
    fn link_error(&self, error: anyhow::Error) -> anyhow::Error {
        match self {
            Self::Udp(_) => mark_udp_io_failure(error),
            Self::Carrier(_) => error,
        }
    }

    fn poll_carrier(&mut self) -> Result<()> {
        if let Self::Carrier(pump) = self {
            // Also poll during the bounded fast path: queued replies and busy
            // Watch work must not defer carrier IO until the next idle wait.
            if let Some(result) = pump.progress().now_or_never() {
                result?;
            }
        }
        Ok(())
    }

    async fn ready(&mut self, peek: &mut [u8]) -> Result<()> {
        match self {
            Self::Udp(readiness) => {
                // Windows may surface stale UDP ICMP errors on this clone;
                // NativeEndpoint classifies/consumes the transport condition.
                if let Err(error) = readiness.peek_from(peek).await {
                    match error.kind() {
                        std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionRefused
                        | std::io::ErrorKind::Interrupted => {}
                        _ => return Err(mark_udp_io_failure(error.into())),
                    }
                }
                Ok(())
            }
            Self::Carrier(pump) => pump.progress().await,
        }
    }
}

enum Admission {
    Udp(Arc<AdaptiveAdmissionV1>),
    Carrier(Arc<OwnedSemaphorePermit>),
}
impl Admission {
    fn acquire(&self) -> Option<Arc<OwnedSemaphorePermit>> {
        match self {
            Self::Udp(slots) => slots.clone().try_acquire_owned().ok().map(Arc::new),
            // The existing node selector already reserved this connection.
            Self::Carrier(permit) => Some(permit.clone()),
        }
    }
}

#[cfg(test)]
fn mailbox_link(endpoint: NativeEndpoint, reliable_carrier: bool) -> Result<NativeObjectLink> {
    mailbox_link_with_resources(endpoint, reliable_carrier, None, None)
}
fn mailbox_link_with_resources(
    endpoint: NativeEndpoint,
    reliable_carrier: bool,
    concurrency: Option<&Arc<ServiceConcurrencyV1>>,
    transport: Option<&ServiceTransportPermitV1>,
) -> Result<NativeObjectLink> {
    let work_limit = concurrency
        .map_or(WORK_LIMIT, |c| crate::mailbox_structured::native_session::adaptive_request_slot_ceiling_v1(c))
        .min(4096);
    let peer_limit = if concurrency.is_some() {
        work_limit
    } else {
        SLOTS
    };
    let retained_bytes = concurrency.map_or(work_limit.saturating_mul(LIMITS.wire_bytes), |c| {
        c.snapshot()
            .retained_bytes
            .max(LIMITS.wire_bytes)
            .min(work_limit.saturating_mul(LIMITS.wire_bytes))
    });
    let retained_bytes = transport.map_or(retained_bytes, |p| {
        retained_bytes.min(p.bytes_per_direction())
    });
    if concurrency.is_some() {
        require_mailbox_frame_budget(retained_bytes)?;
    }
    let peer_bytes = retained_bytes.min(peer_limit.saturating_mul(LIMITS.wire_bytes));
    NativeObjectLink::new(
        endpoint,
        vec![NativeLaneConfig {
            stream: STREAM,
            domain: domain(),
            receive: ReassemblyLimits {
                max_message_bytes: LIMITS.wire_bytes,
                messages: work_limit,
                bytes: retained_bytes,
                peer_messages: peer_limit,
                peer_bytes,
                ttl: REQUEST_DEADLINE,
            },
            send: NativeSendLimits {
                messages: work_limit,
                bytes: retained_bytes,
                peer_messages: peer_limit,
                peer_bytes,
            },
        }],
        if reliable_carrier {
            NativeSendPolicy::reliable_carrier_v1(REQUEST_DEADLINE)
        } else {
            NativeSendPolicy {
                window: NovoRudpWindowConfig::default(),
                pacing: NovoRudpPacingProfile::default(),
                ttl: REQUEST_DEADLINE,
            }
        },
    )
}

#[derive(Clone, Copy)]
pub(super) enum NativeCarrierHandshake {
    UtcV7,
    SessionClockV8 {
        received: Option<ClockAnchorV1>,
        deadline: Instant,
    },
}

/// The owner has explicitly selected NVMBOX07/08 and reserved its existing slot.
/// Reuse the carrier scope's resource bounds and the mailbox/lane quotas; the
/// UDP listener's signed wire budget is not a global cross-carrier rate limit.
// One common owner/pump for both explicit handshake versions.
#[allow(clippy::too_many_arguments)]
pub(super) async fn serve_carrier(
    stream: &mut CarrierStreamV1<'_>,
    key: &SigningKey,
    policy: ServicePeerPolicyV1<'_>,
    service: &MailboxServiceV1,
    stop: Arc<AtomicBool>,
    permit: Arc<OwnedSemaphorePermit>,
    concurrency: Option<Arc<ServiceConcurrencyV1>>,
    authority: impl Fn() -> Result<()> + Copy + Sync,
    handshake: NativeCarrierHandshake,
) -> Result<()> {
    ensure!(
        key.verifying_key().to_bytes() == service.identity_public_key(),
        "native carrier and mailbox signer differ"
    );
    let check = || {
        ensure!(!stop.load(Ordering::Acquire), "service stopped by owner");
        authority()
    };
    check()?;
    let (mut channel, _) = match handshake {
        NativeCarrierHandshake::UtcV7 => {
            authenticate_structured_service_client_observed(
                stream,
                key,
                policy,
                &mut HandshakeReplayCacheV1::new(8),
                &mut |_, _| check(),
            )
            .await?
        }
        NativeCarrierHandshake::SessionClockV8 { deadline, .. } => {
            authenticate_structured_service_client_v2_observed(
                stream,
                key,
                policy,
                deadline,
                &mut HandshakeReplayCacheV1::new(8),
                &mut |_, _| check(),
            )
            .await?
        }
    };
    if let NativeCarrierHandshake::SessionClockV8 { received, deadline } = handshake {
        send_peer_time_v8(stream, &mut channel, received, deadline, check).await?;
    }
    check()?;
    // The Finished channel is bound to this exact carrier; do not establish a
    // second UDP handshake or substitute another peer's authentication result.
    let (endpoint, pump) = NativeCarrierPump::attach_continuous_v7(stream, channel, vec![STREAM])?;
    let transport = concurrency
        .as_ref()
        .map(|c| {
            c.try_reserve_transport()
                .context(NativeMailboxResourcesUnavailableV1)
        })
        .transpose()?;
    serve_link(
        mailbox_link_with_resources(endpoint, true, concurrency.as_ref(), transport.as_ref())?,
        Readiness::Carrier(pump),
        service,
        &stop,
        Admission::Carrier(permit),
        concurrency,
        check,
    )
    .await
}

pub struct NativeMailboxListenerV7 {
    link: NativeObjectLink,
    readiness: tokio::net::UdpSocket,
    route: NativeMailboxEndpointV7,
    stop: Arc<AtomicBool>,
    concurrency: Option<Arc<ServiceConcurrencyV1>>,
    transport: Option<ServiceTransportPermitV1>,
}
impl NativeMailboxListenerV7 {
    /// The caller advertises its actually reachable address. Binding does not
    /// establish NAT reachability. The original SDK presently has a cumulative
    /// identity capacity (<=64), not an evicting public-server peer table.
    pub fn bind(
        listen: SocketAddr,
        advertised: SocketAddr,
        key: SigningKey,
        stop: Arc<AtomicBool>,
        max_connections: u16,
    ) -> Result<Self> {
        Self::bind_with_wire_budget(
            listen,
            advertised,
            key,
            stop,
            max_connections,
            64 * 1024 * 1024,
        )
    }

    /// One explicit node resource budget is both signed and enforced by the
    /// original socket owner. It is not a permission to join the network.
    pub fn bind_with_wire_budget(
        listen: SocketAddr,
        advertised: SocketAddr,
        key: SigningKey,
        stop: Arc<AtomicBool>,
        max_connections: u16,
        max_bytes_per_minute: u64,
    ) -> Result<Self> {
        Self::bind_with_resources(
            listen,
            advertised,
            key,
            stop,
            max_connections,
            max_bytes_per_minute,
            None,
        )
    }
    /// Existing native peer-table envelope is advertised; new work is admitted
    /// from the platform's shared resource observation without another owner.
    pub fn bind_adaptive(
        listen: SocketAddr,
        advertised: SocketAddr,
        key: SigningKey,
        stop: Arc<AtomicBool>,
        max_bytes_per_minute: u64,
        concurrency: Arc<ServiceConcurrencyV1>,
    ) -> Result<Self> {
        Self::bind_with_resources(
            listen,
            advertised,
            key,
            stop,
            64,
            max_bytes_per_minute,
            Some(concurrency),
        )
    }
    fn bind_with_resources(
        listen: SocketAddr,
        mut advertised: SocketAddr,
        key: SigningKey,
        stop: Arc<AtomicBool>,
        max_connections: u16,
        max_bytes_per_minute: u64,
        concurrency: Option<Arc<ServiceConcurrencyV1>>,
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
            max_bytes_per_minute,
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
        endpoint.set_wire_budget_v1(max_bytes_per_minute, Instant::now())?;
        endpoint.enable_client_reconnect_v1()?;
        let transport = concurrency
            .as_ref()
            .map(|c| {
                c.try_reserve_transport()
                    .context(NativeMailboxResourcesUnavailableV1)
            })
            .transpose()?;
        let link =
            mailbox_link_with_resources(endpoint, false, concurrency.as_ref(), transport.as_ref())?;
        Ok(Self {
            link,
            readiness,
            route,
            stop,
            concurrency,
            transport,
        })
    }

    pub(super) fn concurrency(&self) -> Option<Arc<ServiceConcurrencyV1>> {
        self.concurrency.clone()
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
        slots: Arc<AdaptiveAdmissionV1>,
    ) -> Result<()> {
        let Self {
            link,
            readiness,
            stop,
            transport: _transport,
            ..
        } = self;
        let authority = || {
            ensure!(!stop.load(Ordering::Acquire), "service stopped by owner");
            host.check_continuous_native_peer_v7(now_ms()?)
        };
        let concurrency = slots.controller().or_else(|| self.concurrency.clone());
        serve_link(
            link,
            Readiness::Udp(readiness),
            service,
            &stop,
            Admission::Udp(slots),
            concurrency,
            authority,
        )
        .await
    }
}

async fn serve_link(
    mut link: NativeObjectLink,
    mut readiness: Readiness<'_>,
    service: &MailboxServiceV1,
    stop: &AtomicBool,
    admission: Admission,
    concurrency: Option<Arc<ServiceConcurrencyV1>>,
    authority: impl Fn() -> Result<()> + Copy,
) -> Result<()> {
    ensure!(
        link.local_identity() == service.identity_public_key(),
        "native listener and mailbox signer differ"
    );
    let mut servers = BTreeMap::<usize, NativeMailboxServer>::new();
    let mut jobs = BTreeMap::<u64, Job>::new();
    let mut active: FuturesUnordered<LocalBoxFuture<'_, WorkResult>> = FuturesUnordered::new();
    let mut pending = Vec::<Pending>::new();
    let mut next_job = 0u64;
    let mut outbound = 0usize;
    // Retain each response with its reservation until that response completes.
    // An unrelated continuously busy connection must not retain finished work.
    let mut sending = Vec::<(MailboxResponse, ServiceWorkPermitV1)>::new();
    // Full datagram buffer avoids WSAEMSGSIZE on Windows when peeking.
    let mut peek = vec![0u8; 65_536];
    loop {
        // Network input cannot starve already admitted disk/Watch work or
        // the enclosing Application owner during a continuous packet flood.
        tokio::task::consume_budget().await;
        if let Some(Some((id, result))) = active.next().now_or_never() {
            if let Some(mut job) = jobs.remove(&id) {
                if let Ok(Ok(response)) = result {
                    if let Some(p) = &mut job.work_permit {
                        p.completed();
                    }
                    pending.push(Pending {
                        peer: job.peer,
                        response,
                        permit: Some(job.permit),
                        work_permit: job.work_permit,
                    });
                }
            }
        }
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        authority()?;
        readiness.poll_carrier()?;
        let now = Instant::now();
        let report = link
            .poll(now_ms()?, now, QUANTUM)
            .map_err(|error| readiness.link_error(error))?;
        outbound = outbound.saturating_sub(report.completed.len());
        for completed in &report.completed {
            sending.retain(|(response, _permit)| !response.transport_complete(completed));
        }
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
        // Consuming a completed deferred write frees the carrier's sole slot.
        // Give the existing loop another turn to enqueue data/ACKs; otherwise
        // an inbound-only service sleeps its idle 200 ms after each frame.
        // No-progress/pacing/blocked writes still use the bounded wait below.
        let mut progress = report.data_sent > 0 || report.feedback_sent > 0;
        let mut index = 0;
        while index < pending.len() {
            match pending[index].response.try_send(&mut link, now, authority) {
                Ok(false) => index += 1,
                Ok(true) => {
                    let mut completed = pending.swap_remove(index);
                    if let Some(permit) = completed.work_permit.take() {
                        sending.push((completed.response, permit));
                    }
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
        let limits = concurrency.as_ref().map(|c| c.snapshot());
        let work_limit = limits.map_or(WORK_LIMIT, |s| {
            s.work_limit.saturating_add(s.waiting_watch_limit)
        });
        let waiting_watch_limit = limits.map_or(WAITING_WATCH_LIMIT, |s| s.waiting_watch_limit);
        if active.len() + pending.len() >= work_limit {
            if let Some(c) = &concurrency {
                c.note_demand();
            }
        }
        if active.len() + pending.len() < work_limit {
            if let Some(body) = link.poll_complete(STREAM, now, QUANTUM)? {
                progress = true;
                let peer = (0..link.peer_count())
                    .find(|&p| link.peer_id(p).is_ok_and(|id| id == body.peer))
                    .context("native completed unknown peer")?;
                // Always admit/validate before returning an overload reply.
                // Transport completion alone must not strand a valid request
                // when a different carrier currently owns the shared permits.
                if let std::collections::btree_map::Entry::Vacant(entry) = servers.entry(peer) {
                    let attached = if let Some(controller) = &concurrency {
                        NativeMailboxServer::attach_adaptive(&mut link, peer, controller.clone())
                    } else {
                        NativeMailboxServer::attach(&mut link, peer)
                    };
                    if let Ok(server) = attached {
                        entry.insert(server);
                    }
                }
                if let Some(server) = servers.get_mut(&peer) {
                    if let Ok(work) = server.admit(&link, body, now, authority) {
                        let waiting_watch = work.has_waiting_watch();
                        let watch_saturated = waiting_watch
                            && (jobs.values().filter(|job| job.waiting_watch).count()
                                >= waiting_watch_limit
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
                            .or_else(|| admission.acquire());
                        let work_permit = concurrency.as_ref().and_then(|c| {
                            let bytes = work.retained_bytes_estimate();
                            if waiting_watch {
                                c.try_acquire_watch(bytes)
                            } else {
                                c.try_acquire_work(bytes)
                            }
                        });
                        if watch_saturated
                            || permit.is_none()
                            || (concurrency.is_some() && work_permit.is_none())
                        {
                            // The resource-derived Watch budget, one per peer,
                            // leaves ordinary Put/Fetch/Consume work available.
                            if let Ok(response) = work.reject_quota(service, authority) {
                                pending.push(Pending {
                                    peer,
                                    response,
                                    permit: None,
                                    work_permit: None,
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
                                    work_permit,
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
                if let Some(mut job) = jobs.remove(&id) {
                    if let Ok(Ok(response)) = result {
                        if let Some(p) = &mut job.work_permit { p.completed(); }
                        pending.push(Pending { peer: job.peer, response, permit: Some(job.permit), work_permit: job.work_permit });
                    }
                }
            }
            readable = readiness.ready(&mut peek) => { readable?; }
            _ = tokio::time::sleep(maintenance) => {}
        }
    }
}

#[cfg(test)]
#[path = "product_rendezvous_native_failure_tests.rs"]
mod failure_tests;

#[cfg(test)]
mod resource_boundary_tests {
    use super::*;
    #[test]
    fn insufficient_link_memory_is_typed_resource_deferral() {
        let error = require_mailbox_frame_budget(LIMITS.wire_bytes - 1).unwrap_err();
        assert!(error.is::<NativeMailboxResourcesUnavailableV1>());
        require_mailbox_frame_budget(LIMITS.wire_bytes).unwrap();
    }
}
