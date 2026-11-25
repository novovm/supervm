//! Opaque off-chain application RPC on an owner's existing authenticated link.
//! No socket, identity, encryption, discovery or transport retry is created here.
use crate::{
    novorudp::{
        fragments::{CompletedMessage, FragmentProfile, OutgoingMessage, ReassemblyLimits},
        object_link::{
            NativeApplicationRole, NativeLaneConfig, NativeObjectLink, NativeSendCancellation,
            NativeSendLimits, NativeSessionValidity,
        },
    },
    service_concurrency::{ServiceConcurrencyV1, ServiceWorkPermitV1},
};
use anyhow::{ensure, Context, Result};
use futures_util::FutureExt;
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

pub const MAX_WIRE_BYTES: usize = 1_100_000;
pub const MAX_JOBS: usize = 4;
pub const DEADLINE: Duration = Duration::from_secs(35);
const HEADER: usize = 32;
const REQUEST: &[u8; 8] = b"NVAPP01Q";
const RESPONSE: &[u8; 8] = b"NVAPP01S";
const REJECTED: &[u8; 8] = b"NVAPP01E";

#[derive(Clone)]
pub struct AuthenticatedApplicationPeerV1 {
    id: String,
    session: [u8; 16],
    validity: NativeSessionValidity,
}
impl AuthenticatedApplicationPeerV1 {
    pub fn session_id(&self) -> [u8; 16] {
        self.session
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn is_current(&self) -> bool {
        self.validity.is_current()
    }
}
pub type ApplicationFutureV1 = Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send>>;
pub trait NativeApplicationHandlerV1: Send + Sync {
    fn request(&self, peer: AuthenticatedApplicationPeerV1, input: Vec<u8>) -> ApplicationFutureV1;
    fn closed(&self) {}
}
#[derive(Clone)]
pub struct NativeApplicationRouteV1 {
    pub stream: u64,
    pub domain: [u8; 32],
    pub handler: Arc<dyn NativeApplicationHandlerV1>,
}
/// Partitions the existing transport reservation; never adds another allowance.
pub fn lane(stream: u64, domain: [u8; 32], bytes: usize) -> Result<NativeLaneConfig> {
    ensure!(
        bytes >= MAX_WIRE_BYTES,
        "native application transport budget"
    );
    let messages = (bytes / MAX_WIRE_BYTES).min(MAX_JOBS);
    Ok(NativeLaneConfig {
        stream,
        domain,
        receive: ReassemblyLimits {
            max_message_bytes: MAX_WIRE_BYTES,
            messages,
            bytes,
            peer_messages: 1,
            peer_bytes: MAX_WIRE_BYTES,
            ttl: DEADLINE,
        },
        send: NativeSendLimits {
            messages,
            bytes,
            peer_messages: 1,
            peer_bytes: MAX_WIRE_BYTES,
        },
    })
}
fn encode(
    kind: &[u8; 8],
    session: [u8; 16],
    sequence: u64,
    payload: &[u8],
    domain: [u8; 32],
) -> Result<Arc<OutgoingMessage>> {
    ensure!(
        payload.len() <= MAX_WIRE_BYTES - HEADER,
        "native application payload bound"
    );
    let mut bytes = Vec::with_capacity(HEADER + payload.len());
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(&session);
    bytes.extend_from_slice(&sequence.to_le_bytes());
    bytes.extend_from_slice(payload);
    Ok(Arc::new(OutgoingMessage::with_profile(
        domain,
        bytes,
        MAX_WIRE_BYTES,
        FragmentProfile::NativeUdpV2,
    )?))
}
fn decode(body: CompletedMessage, session: [u8; 16], sequence: u64) -> Result<([u8; 8], Vec<u8>)> {
    let size = body.chunks.iter().try_fold(0usize, |sum, c| {
        sum.checked_add(c.len())
            .context("application size overflow")
    })?;
    ensure!(
        (HEADER..=MAX_WIRE_BYTES).contains(&size),
        "native application frame bound"
    );
    let bytes: Vec<u8> = body.chunks.into_iter().flatten().collect();
    ensure!(
        bytes[8..24] == session && bytes[24..32] == sequence.to_le_bytes(),
        "native application session/sequence mismatch"
    );
    Ok((bytes[..8].try_into()?, bytes[HEADER..].to_vec()))
}
struct Binding {
    peer: AuthenticatedApplicationPeerV1,
    session: [u8; 16],
    next: u64,
    busy: bool,
}
impl Binding {
    fn attach(
        link: &mut NativeObjectLink,
        peer: usize,
        stream: u64,
        domain: [u8; 32],
        role: NativeApplicationRole,
    ) -> Result<Self> {
        let session = link.claim_application_session(peer, stream, domain, role)?;
        Ok(Self {
            peer: AuthenticatedApplicationPeerV1 {
                id: link.peer_id(peer)?.to_owned(),
                session,
                validity: link.session_validity(peer)?,
            },
            session,
            next: 0,
            busy: false,
        })
    }
}
struct Job {
    peer: usize,
    sequence: u64,
    session: [u8; 16],
    validity: NativeSessionValidity,
    deadline: Instant,
    future: Option<ApplicationFutureV1>,
    reply: Option<Arc<OutgoingMessage>>,
    permit: Option<ServiceWorkPermitV1>,
}
pub struct NativeApplicationDispatcherV1 {
    route: NativeApplicationRouteV1,
    bindings: BTreeMap<usize, Binding>,
    jobs: Vec<Job>,
}
impl Drop for NativeApplicationDispatcherV1 {
    fn drop(&mut self) {
        self.route.handler.closed();
    }
}
impl NativeApplicationDispatcherV1 {
    pub fn new(route: NativeApplicationRouteV1) -> Self {
        Self {
            route,
            bindings: BTreeMap::new(),
            jobs: Vec::new(),
        }
    }
    pub fn protected_peers(&self) -> impl Iterator<Item = usize> + '_ {
        self.jobs.iter().map(|job| job.peer)
    }
    pub fn busy(&self) -> bool {
        !self.jobs.is_empty()
    }
    /// Bounded cooperative work, polled by the original socket owner. No await
    /// on application execution can block handshake/ACK/mailbox progress.
    pub fn poll(
        &mut self,
        link: &mut NativeObjectLink,
        now: Instant,
        controller: Option<&Arc<ServiceConcurrencyV1>>,
    ) -> Result<bool> {
        self.bindings.retain(|_, b| b.peer.is_current());
        let mut progress = false;
        let mut index = 0;
        while index < self.jobs.len() {
            let job = &mut self.jobs[index];
            if !job.validity.is_current() || now >= job.deadline {
                // Keep a live binding busy after an ambiguous timeout. Only a
                // newly authenticated session can reclaim it; never replay.
                self.jobs.swap_remove(index);
                continue;
            }
            if let Some(future) = &mut job.future {
                if let Some(result) = future.as_mut().now_or_never() {
                    let payload = result.ok().filter(|v| v.len() <= MAX_WIRE_BYTES - HEADER);
                    job.reply = Some(encode(
                        if payload.is_some() {
                            RESPONSE
                        } else {
                            REJECTED
                        },
                        job.session,
                        job.sequence,
                        payload.as_deref().unwrap_or_default(),
                        self.route.domain,
                    )?);
                    job.future = None;
                    if let Some(permit) = &mut job.permit {
                        permit.completed();
                    }
                    progress = true;
                }
            }
            if let Some(reply) = &job.reply {
                if link.try_send_until(job.peer, self.route.stream, reply, now, job.deadline)? {
                    if let Some(binding) = self.bindings.get_mut(&job.peer) {
                        binding.busy = false;
                    }
                    self.jobs.swap_remove(index);
                    progress = true;
                    continue;
                }
            }
            index += 1;
        }
        if self.jobs.len() >= MAX_JOBS {
            return Ok(progress);
        }
        if link.pending_messages(self.route.stream)? == 0 {
            return Ok(progress);
        }
        let permit = controller.map(|c| c.try_acquire_ready_work(MAX_WIRE_BYTES * 2));
        if permit.as_ref().is_some_and(Option::is_none) {
            return Ok(progress);
        }
        let (body, hashing) = link.poll_complete_with_progress(self.route.stream, now, 32)?;
        progress |= hashing;
        let Some(body) = body else {
            return Ok(progress);
        };
        let peer = link
            .peer_indices()
            .find(|&p| link.peer_id(p).is_ok_and(|id| id == body.peer))
            .context("unknown application peer")?;
        if let std::collections::btree_map::Entry::Vacant(entry) = self.bindings.entry(peer) {
            entry.insert(Binding::attach(
                link,
                peer,
                self.route.stream,
                self.route.domain,
                NativeApplicationRole::Service,
            )?);
        }
        let binding = self
            .bindings
            .get_mut(&peer)
            .expect("attached application peer");
        if binding.busy {
            return Ok(true);
        }
        let Ok((kind, payload)) = decode(body, binding.session, binding.next) else {
            binding.busy = true;
            return Ok(true);
        };
        if &kind != REQUEST {
            binding.busy = true;
            return Ok(true);
        }
        let sequence = binding.next;
        binding.next = binding
            .next
            .checked_add(1)
            .context("application sequence exhausted")?;
        binding.busy = true;
        self.jobs.push(Job {
            peer,
            sequence,
            session: binding.session,
            validity: binding.peer.validity.clone(),
            deadline: now + DEADLINE,
            future: Some(self.route.handler.request(binding.peer.clone(), payload)),
            reply: None,
            permit: permit.flatten(),
        });
        Ok(true)
    }
}
struct ClientPending {
    sequence: u64,
    body: Arc<OutgoingMessage>,
    deadline: Instant,
    sent: bool,
    cancel: NativeSendCancellation,
}
impl Drop for ClientPending {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
/// This adapter borrows B's existing link. Its owner still performs network IO.
pub struct NativeApplicationClientV1 {
    binding: Binding,
    peer: usize,
    stream: u64,
    domain: [u8; 32],
    pending: Option<ClientPending>,
    failed: bool,
}
impl NativeApplicationClientV1 {
    pub fn attach(
        link: &mut NativeObjectLink,
        peer: usize,
        stream: u64,
        domain: [u8; 32],
    ) -> Result<Self> {
        Ok(Self {
            binding: Binding::attach(link, peer, stream, domain, NativeApplicationRole::Client)?,
            peer,
            stream,
            domain,
            pending: None,
            failed: false,
        })
    }
    pub fn begin(&mut self, payload: &[u8], now: Instant) -> Result<()> {
        ensure!(
            !self.failed && self.binding.peer.is_current() && self.pending.is_none(),
            "application client busy or requires recovery"
        );
        let sequence = self.binding.next;
        let body = encode(
            REQUEST,
            self.binding.session,
            sequence,
            payload,
            self.domain,
        )?;
        self.binding.next = sequence
            .checked_add(1)
            .context("application sequence exhausted")?;
        self.pending = Some(ClientPending {
            sequence,
            body,
            deadline: now + DEADLINE,
            sent: false,
            cancel: NativeSendCancellation::default(),
        });
        Ok(())
    }
    /// Multi-peer owners call progress, demultiplex their completed lane by
    /// authenticated peer, then call receive on the matching adapter.
    pub fn progress(&mut self, link: &mut NativeObjectLink, now: Instant) -> Result<()> {
        let result = self.progress_inner(link, now);
        if result.is_err() {
            self.failed = true;
            self.pending.take();
        }
        result
    }
    fn progress_inner(&mut self, link: &mut NativeObjectLink, now: Instant) -> Result<()> {
        ensure!(
            !self.failed && self.binding.peer.is_current(),
            "application session retired; inspect original task"
        );
        let Some(pending) = &mut self.pending else {
            return Ok(());
        };
        ensure!(
            now < pending.deadline,
            "application deadline; outcome unknown, no replay"
        );
        if !pending.sent {
            pending.sent = link.try_send_cancellable_until(
                self.peer,
                self.stream,
                &pending.body,
                now,
                pending.deadline,
                &pending.cancel,
            )?;
        }
        Ok(())
    }
    /// Convenience only for a single-peer link; never consumes another peer's response.
    pub fn poll(&mut self, link: &mut NativeObjectLink, now: Instant) -> Result<Option<Vec<u8>>> {
        ensure!(
            link.peer_count() == 1,
            "multi-peer owner must demultiplex application responses"
        );
        self.progress(link, now)?;
        if self.pending.is_none() {
            return Ok(None);
        }
        let Some(body) = link.poll_complete(self.stream, now, 32)? else {
            return Ok(None);
        };
        self.receive(link, body, now).map(Some)
    }
    pub fn receive(
        &mut self,
        link: &NativeObjectLink,
        body: CompletedMessage,
        now: Instant,
    ) -> Result<Vec<u8>> {
        let result = self.receive_inner(link, body, now);
        if result.is_err() {
            self.failed = true;
            self.pending.take();
        }
        result
    }
    fn receive_inner(
        &mut self,
        link: &NativeObjectLink,
        body: CompletedMessage,
        now: Instant,
    ) -> Result<Vec<u8>> {
        ensure!(
            !self.failed
                && self.binding.peer.is_current()
                && link.session_id(self.peer) == Some(self.binding.session)
                && link.peer_id(self.peer)? == self.binding.peer.id,
            "application response session retired"
        );
        let pending = self
            .pending
            .as_ref()
            .context("unsolicited application response")?;
        ensure!(
            pending.sent && now < pending.deadline,
            "late or unsent application response"
        );
        ensure!(
            body.peer == self.binding.peer.id,
            "application response peer mismatch"
        );
        let (kind, payload) = decode(body, self.binding.session, pending.sequence)?;
        ensure!(
            &kind == RESPONSE,
            "application request rejected; inspect original task, no replay"
        );
        self.pending.take();
        Ok(payload)
    }
}

#[cfg(test)]
#[path = "native_application_tests.rs"]
mod tests;
