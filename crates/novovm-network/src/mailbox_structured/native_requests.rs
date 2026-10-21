//! Bounded async request handles over ONE existing native mailbox session.
//! The application owner alone polls the link, dispatches events and owns this
//! driver. There is no socket, runtime, background task, reconnect or retry here.
use super::{
    encode_requests,
    native_session::{NativeMailboxClient, ReplyRequestV7, RequestId, REQUEST_DEADLINE, SLOTS},
    Reply, Request,
};
use crate::novorudp::{
    fragments::CompletedMessage,
    object_link::{NativeObjectLink, NativeSendCancellation, NativeSendCompletion},
    object_sender::ObjectSendState,
};
use std::{
    cell::Cell,
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll},
    time::Instant,
};
use tokio::sync::{oneshot, Notify};

pub type NativeRequestAuthority = Arc<dyn Fn() -> anyhow::Result<()> + Send + Sync>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeRequestErrorKind {
    Capacity,
    Cancelled,
    Deadline,
    Authority,
    Session,
    Protocol,
    Transport,
    Stopped,
}
/// Temporary exhaustion of the shared local payload budget, not a bad request
/// index, occupied dedicated lane, authority failure or protocol error.
#[derive(Debug)]
pub struct NativeRequestResourcesUnavailableV1;
impl fmt::Display for NativeRequestResourcesUnavailableV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("native request local payload budget unavailable")
    }
}
impl std::error::Error for NativeRequestResourcesUnavailableV1 {}
#[derive(Debug)]
pub struct NativeRequestError {
    pub kind: NativeRequestErrorKind,
    cause: Option<anyhow::Error>,
}
impl NativeRequestError {
    fn new(kind: NativeRequestErrorKind) -> Self {
        Self { kind, cause: None }
    }
    fn caused(kind: NativeRequestErrorKind, cause: anyhow::Error) -> Self {
        Self {
            kind,
            cause: Some(cause),
        }
    }
}
impl fmt::Display for NativeRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Keep wire/body/contact data out of the public error presentation.
        write!(f, "native mailbox request {:?}", self.kind)
    }
}
impl std::error::Error for NativeRequestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_ref().map(|e| e.as_ref())
    }
}
type Result<T> = std::result::Result<T, NativeRequestError>;
type Outcome = Result<Vec<Reply>>;
struct Queued {
    requests: Vec<Request>,
    deadline: Instant,
    authority: NativeRequestAuthority,
    reply: oneshot::Sender<Outcome>,
}
enum Phase {
    Preparing,
    Queued(Queued),
    Active,
    Complete,
}
struct Entry {
    retained_bytes: usize,
    _body_permit: crate::service_concurrency::ServiceRequestBodyPermitV1,
    _reply_permit: Option<crate::service_concurrency::ServiceRequestBodyPermitV1>,
    ticket: u64,
    cancelled: NativeSendCancellation,
    phase: Phase,
}
#[derive(Default)]
struct Slot {
    entry: Option<Entry>,
    failed: Option<NativeRequestErrorKind>,
}
struct State {
    slots: Vec<Slot>,
    next: u64,
    closed: Option<NativeRequestErrorKind>,
}
struct Shared {
    controller: Arc<crate::service_concurrency::ServiceConcurrencyV1>,
    retained_limit: std::sync::atomic::AtomicUsize,
    bulk: AtomicBool,
    state: Mutex<State>,
    wake: Arc<Notify>,
}
impl Shared {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>> {
        self.state
            .lock()
            .map_err(|_| NativeRequestError::new(NativeRequestErrorKind::Stopped))
    }
    fn retain_reply(&self, index: usize, ticket: u64, wire_bytes: usize) -> Result<()> {
        let mut state = self.lock()?;
        let entry = state
            .slots
            .get_mut(index)
            .and_then(|s| s.entry.as_mut())
            .filter(|e| e.ticket == ticket && matches!(e.phase, Phase::Active))
            .ok_or_else(|| NativeRequestError::new(NativeRequestErrorKind::Protocol))?;
        if entry._reply_permit.is_some() {
            return Err(NativeRequestError::new(NativeRequestErrorKind::Protocol));
        }
        // Flattening and decoding coexist briefly; the decoded result then stays
        // in the oneshot until polled/dropped. Existing transport owns wire chunks.
        let bytes = wire_bytes.saturating_mul(3).saturating_add(4096);
        let permit = self
            .controller
            .try_reserve_request_body(bytes)
            .ok_or_else(|| {
                NativeRequestError::caused(
                    NativeRequestErrorKind::Capacity,
                    NativeRequestResourcesUnavailableV1.into(),
                )
            })?;
        entry.retained_bytes = entry.retained_bytes.saturating_add(bytes);
        entry._reply_permit = Some(permit);
        Ok(())
    }
}
struct Reservation {
    shared: Arc<Shared>,
    slot: usize,
    ticket: u64,
}
impl Reservation {
    fn release(self, cancel: bool) {
        if let Ok(mut state) = self.shared.state.lock() {
            let slot = &mut state.slots[self.slot];
            if let Some(entry) = slot
                .entry
                .as_mut()
                .filter(|entry| entry.ticket == self.ticket)
            {
                if matches!(entry.phase, Phase::Active) {
                    // Owner must retire the original active request before this
                    // slot can be considered free. No second request queues here.
                    if cancel {
                        entry.cancelled.cancel();
                    }
                } else {
                    slot.entry = None;
                }
            }
        }
        self.shared.wake.notify_one();
    }
}

/// Admission is synchronous; constructing many unpolled futures cannot create
/// an unbounded queue of request bodies. A completed result keeps its slot until
/// consumed/dropped, bounding unpolled reply buffers as well.
pub struct NativeRequestFuture {
    receiver: Option<oneshot::Receiver<Outcome>>,
    immediate: Option<Outcome>,
    reservation: Option<Reservation>,
    deadline: Option<Pin<Box<tokio::time::Sleep>>>,
}
impl NativeRequestFuture {
    fn error(error: NativeRequestError) -> Self {
        Self {
            receiver: None,
            immediate: Some(Err(error)),
            reservation: None,
            deadline: None,
        }
    }
}
impl Future for NativeRequestFuture {
    type Output = Outcome;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if let Some(result) = this.immediate.take() {
            return Poll::Ready(result);
        }
        let receiver = this
            .receiver
            .as_mut()
            .expect("request future polled after completion");
        if let Poll::Ready(result) = Pin::new(receiver).poll(cx) {
            this.receiver = None;
            this.deadline = None;
            if let Some(reservation) = this.reservation.take() {
                reservation.release(false);
            }
            return Poll::Ready(result.unwrap_or_else(|_| {
                Err(NativeRequestError::new(NativeRequestErrorKind::Stopped))
            }));
        }
        if this
            .deadline
            .as_mut()
            .is_some_and(|deadline| deadline.as_mut().poll(cx).is_ready())
        {
            this.receiver = None;
            this.deadline = None;
            if let Some(reservation) = this.reservation.take() {
                reservation.release(true);
            }
            return Poll::Ready(Err(NativeRequestError::new(
                NativeRequestErrorKind::Deadline,
            )));
        }
        Poll::Pending
    }
}
impl Drop for NativeRequestFuture {
    fn drop(&mut self) {
        if let Some(reservation) = self.reservation.take() {
            reservation.release(true);
        }
    }
}

#[derive(Clone)]
pub struct NativeMailboxHandle {
    shared: Arc<Shared>,
}
fn queued_request_bytes(requests: &[Request], allocated_slots: usize) -> usize {
    // Account the owned request plus its bounded codec copy, not a hypothetical
    // maximum Fetch/Watch response. Response transport has its own owner lease.
    requests.iter().fold(
        4096usize.saturating_add(allocated_slots.saturating_mul(std::mem::size_of::<Request>())),
        |total, request| {
            let dynamic = match request {
                Request::Put { ciphertext, .. }
                | Request::PutBulkV1 { ciphertext, .. }
                | Request::EndpointDeliverV1 { ciphertext, .. } => ciphertext.capacity(),
                Request::Watch { boxes, .. }
                | Request::WatchFetchV6 { boxes, .. }
                | Request::WatchFetchBulkV1 { boxes, .. } => boxes
                    .capacity()
                    .saturating_mul(std::mem::size_of::<crate::mailbox_rpc::MailboxWatchV1>()),
                _ => 0,
            };
            total
                .saturating_add(1024)
                .saturating_add(dynamic.saturating_mul(3))
        },
    )
}
impl NativeMailboxHandle {
    /// Only after validating the node-signed bulk-v1 locator for this session.
    pub fn enable_verified_bulk_v1(&self) {
        self.shared.bulk.store(true, Ordering::Release);
    }
    pub fn supports_bulk_v1(&self) -> bool {
        self.shared.bulk.load(Ordering::Acquire)
    }
    pub fn has_negotiated_capacity(&self) -> bool {
        self.shared.retained_limit.load(Ordering::Acquire) != 0
    }

    pub fn is_stopped(&self) -> bool {
        self.shared
            .state
            .lock()
            .map_or(true, |state| state.closed.is_some())
    }
    pub fn request(
        &self,
        slot: usize,
        requests: Vec<Request>,
        deadline: Instant,
        authority: NativeRequestAuthority,
    ) -> NativeRequestFuture {
        match self.enqueue(Some(slot), requests, deadline, authority) {
            Ok(future) => future,
            Err(error) => NativeRequestFuture::error(error),
        }
    }
    /// Choose an extra negotiated slot atomically, reserving 0..4 for existing
    /// explicit notification/control/send lanes. Until capacity is negotiated, existing lanes
    /// with explicit slot ownership may continue using request(index, ...).
    pub fn request_auto(
        &self,
        requests: Vec<Request>,
        deadline: Instant,
        authority: NativeRequestAuthority,
    ) -> NativeRequestFuture {
        match self.enqueue(None, requests, deadline, authority) {
            Ok(future) => future,
            Err(error) => NativeRequestFuture::error(error),
        }
    }
    pub fn slot_capacity(&self) -> usize {
        self.shared.state.lock().map_or(SLOTS, |s| s.slots.len())
    }
    fn enqueue(
        &self,
        requested_index: Option<usize>,
        requests: Vec<Request>,
        deadline: Instant,
        authority: NativeRequestAuthority,
    ) -> Result<NativeRequestFuture> {
        let now = Instant::now();
        let deadline = deadline.min(now + REQUEST_DEADLINE);
        if now >= deadline {
            return Err(NativeRequestError::new(NativeRequestErrorKind::Deadline));
        }
        let cancelled = NativeSendCancellation::default();
        let retained_bytes = queued_request_bytes(&requests, requests.capacity());
        let (ticket, index) = {
            let mut state = self.shared.lock()?;
            if let Some(kind) = state.closed {
                return Err(NativeRequestError::new(kind));
            }
            let negotiated = state.slots.len();
            let live_limit = self
                .shared
                .controller
                .snapshot()
                .work_limit
                .max(SLOTS)
                .min(negotiated);
            let index = requested_index
                .or_else(|| {
                    (SLOTS..live_limit).find(|&i| {
                        state.slots[i].entry.is_none() && state.slots[i].failed.is_none()
                    })
                })
                .ok_or_else(|| {
                    self.shared.controller.note_demand();
                    NativeRequestError::new(NativeRequestErrorKind::Capacity)
                })?;
            if index >= SLOTS {
                let occupied = state.slots.iter().filter(|s| s.entry.is_some()).count();
                let bytes = state
                    .slots
                    .iter()
                    .filter_map(|s| s.entry.as_ref())
                    .fold(0usize, |n, e| n.saturating_add(e.retained_bytes));
                let resource_bytes = self.shared.controller.snapshot().retained_bytes;
                let limit = resource_bytes.min(self.shared.retained_limit.load(Ordering::Acquire));
                if index >= live_limit
                    || occupied >= live_limit
                    || retained_bytes > limit.saturating_sub(bytes)
                {
                    self.shared.controller.note_demand();
                    return Err(NativeRequestError::new(NativeRequestErrorKind::Capacity));
                }
            }
            let slot = state
                .slots
                .get(index)
                .ok_or_else(|| NativeRequestError::new(NativeRequestErrorKind::Capacity))?;
            if let Some(kind) = slot.failed {
                return Err(NativeRequestError::new(kind));
            }
            if slot.entry.is_some() {
                return Err(NativeRequestError::new(NativeRequestErrorKind::Capacity));
            }
            let body_permit = self
                .shared
                .controller
                .try_reserve_request_body(retained_bytes)
                .ok_or_else(|| {
                    NativeRequestError::caused(
                        NativeRequestErrorKind::Capacity,
                        NativeRequestResourcesUnavailableV1.into(),
                    )
                })?;
            state.next = state
                .next
                .checked_add(1)
                .ok_or_else(|| NativeRequestError::new(NativeRequestErrorKind::Stopped))?;
            let ticket = state.next;
            state.slots[index].entry = Some(Entry {
                retained_bytes,
                _body_permit: body_permit,
                _reply_permit: None,
                ticket,
                cancelled,
                phase: Phase::Preparing,
            });
            (ticket, index)
        };
        let reservation = Reservation {
            shared: self.shared.clone(),
            slot: index,
            ticket,
        };
        // Existing codec validates all lengths/shapes before building columns.
        // Only admitted reserved callers may prepare; no ciphertext is cloned into
        // any unbounded pending-future collection. The real context is set at begin.
        if let Err(error) = encode_requests([1; 32], &requests) {
            reservation.release(false);
            return Err(NativeRequestError::caused(
                NativeRequestErrorKind::Protocol,
                error,
            ));
        }
        let (reply, receiver) = oneshot::channel();
        {
            let mut state = match self.shared.lock() {
                Ok(state) => state,
                Err(error) => {
                    reservation.release(false);
                    return Err(error);
                }
            };
            if let Some(kind) = state.closed {
                drop(state);
                reservation.release(false);
                return Err(NativeRequestError::new(kind));
            }
            let Some(entry) = state.slots[index]
                .entry
                .as_mut()
                .filter(|e| e.ticket == ticket)
            else {
                drop(state);
                reservation.release(false);
                return Err(NativeRequestError::new(NativeRequestErrorKind::Stopped));
            };
            entry.phase = Phase::Queued(Queued {
                requests,
                deadline,
                authority,
                reply,
            });
        }
        self.shared.wake.notify_one();
        Ok(NativeRequestFuture {
            receiver: Some(receiver),
            immediate: None,
            reservation: Some(reservation),
            deadline: Some(Box::pin(tokio::time::sleep_until(deadline.into()))),
        })
    }
}
struct Active {
    ticket: u64,
    id: RequestId,
    cancelled: NativeSendCancellation,
    deadline: Instant,
    authority: NativeRequestAuthority,
    reply: oneshot::Sender<Outcome>,
}

pub struct NativeMailboxDriver {
    client: NativeMailboxClient,
    shared: Arc<Shared>,
    active: Vec<Option<Active>>,
}
impl NativeMailboxDriver {
    pub fn attach(
        link: &mut NativeObjectLink,
        peer: usize,
        node: [u8; 32],
    ) -> Result<(Self, NativeMailboxHandle)> {
        Self::attach_adaptive(
            link,
            peer,
            node,
            crate::service_concurrency::ServiceConcurrencyV1::from_host(),
        )
    }
    pub fn attach_adaptive(
        link: &mut NativeObjectLink,
        peer: usize,
        node: [u8; 32],
        controller: Arc<crate::service_concurrency::ServiceConcurrencyV1>,
    ) -> Result<(Self, NativeMailboxHandle)> {
        let client = NativeMailboxClient::attach_adaptive(link, peer, node, controller.clone())
            .map_err(|error| NativeRequestError::caused(NativeRequestErrorKind::Session, error))?;
        let shared = Arc::new(Shared {
            controller,
            retained_limit: std::sync::atomic::AtomicUsize::new(0),
            bulk: AtomicBool::new(false),
            state: Mutex::new(State {
                slots: (0..SLOTS).map(|_| Slot::default()).collect(),
                next: 0,
                closed: None,
            }),
            wake: Arc::new(Notify::new()),
        });
        let handle = NativeMailboxHandle {
            shared: shared.clone(),
        };
        Ok((
            Self {
                client,
                shared,
                active: (0..SLOTS).map(|_| None).collect(),
            },
            handle,
        ))
    }
    pub fn wake(&self) -> Arc<Notify> {
        self.shared.wake.clone()
    }
    /// The original link owner can include this in its existing timer selection.
    pub fn next_deadline(&self) -> Option<Instant> {
        let mut earliest = self.active.iter().flatten().map(|a| a.deadline).min();
        if let Some(deadline) = self.client.capacity_deadline() {
            earliest = Some(earliest.map_or(deadline, |d| d.min(deadline)));
        }
        if let Ok(state) = self.shared.state.lock() {
            for slot in &state.slots {
                if let Some(Entry {
                    phase: Phase::Queued(q),
                    ..
                }) = &slot.entry
                {
                    earliest = Some(earliest.map_or(q.deadline, |d| d.min(q.deadline)));
                }
            }
        }
        earliest
    }
    fn complete(&mut self, index: usize, outcome: Outcome, failed: Option<NativeRequestErrorKind>) {
        if let Some(active) = self.active[index].take() {
            if let Ok(mut state) = self.shared.state.lock() {
                let slot = &mut state.slots[index];
                if slot
                    .entry
                    .as_ref()
                    .is_some_and(|e| e.ticket == active.ticket)
                {
                    if let Some(kind) = failed {
                        slot.failed = Some(kind);
                    }
                    // Successful replies leave bounded transport cleanup alive.
                    // Only a dropped receiver permits early reservation release.
                    if active.reply.is_closed() {
                        slot.entry = None;
                    } else if let Some(entry) = &mut slot.entry {
                        entry.phase = Phase::Complete;
                    }
                }
            }
            let _ = active.reply.send(outcome);
        }
    }
    fn fail(&mut self, index: usize, kind: NativeRequestErrorKind) {
        self.fail_with_error(index, NativeRequestError::new(kind));
    }
    fn fail_with_error(&mut self, index: usize, error: NativeRequestError) {
        if let Some(active) = &self.active[index] {
            self.client.cancel(active.id);
        }
        let kind = error.kind;
        self.complete(index, Err(error), Some(kind));
    }
    fn check_session(&mut self, link: &NativeObjectLink) -> Result<()> {
        if let Some(kind) = self.shared.lock()?.closed {
            return Err(NativeRequestError::new(kind));
        }
        if let Err(error) = self.client.check_session(link) {
            self.invalidate(NativeRequestErrorKind::Session);
            return Err(NativeRequestError::caused(
                NativeRequestErrorKind::Session,
                error,
            ));
        }
        Ok(())
    }
    /// Bounded synchronous owner work; caller still polls the SAME link itself.
    /// Request-specific failures complete that future, not the other three slots.
    pub fn progress(&mut self, link: &mut NativeObjectLink, now: Instant) -> Result<()> {
        self.check_session(link)?;
        self.client.progress_capacity(link, now).map_err(|e| {
            let kind = if e.is::<super::native_session::CapacityNegotiationExpired>() {
                NativeRequestErrorKind::Deadline
            } else {
                NativeRequestErrorKind::Transport
            };
            NativeRequestError::caused(kind, e)
        })?;
        for index in 0..self.active.len() {
            if let Some(active) = &self.active[index] {
                if now >= active.deadline {
                    self.fail(index, NativeRequestErrorKind::Deadline);
                    continue;
                }
                if active.cancelled.is_cancelled() {
                    self.fail(index, NativeRequestErrorKind::Cancelled);
                    continue;
                }
            } else {
                let queued = {
                    let mut state = self.shared.lock()?;
                    let Some(entry) = &mut state.slots[index].entry else {
                        continue;
                    };
                    if !matches!(entry.phase, Phase::Queued(_)) {
                        continue;
                    }
                    let Phase::Queued(queued) = std::mem::replace(&mut entry.phase, Phase::Active)
                    else {
                        unreachable!()
                    };
                    (entry.ticket, entry.cancelled.clone(), queued)
                };
                let (ticket, cancelled, queued) = queued;
                if now >= queued.deadline || cancelled.is_cancelled() {
                    let kind = if now >= queued.deadline {
                        NativeRequestErrorKind::Deadline
                    } else {
                        NativeRequestErrorKind::Cancelled
                    };
                    if let Ok(mut state) = self.shared.state.lock() {
                        let slot = &mut state.slots[index];
                        if slot.entry.as_ref().is_some_and(|e| e.ticket == ticket) {
                            if queued.reply.is_closed() {
                                slot.entry = None;
                            } else if let Some(entry) = &mut slot.entry {
                                entry.phase = Phase::Complete;
                            }
                        }
                    }
                    let _ = queued.reply.send(Err(NativeRequestError::new(kind)));
                    continue; // No begin, no consumed sequence, no failed native slot.
                }
                if let Err(error) = (queued.authority)() {
                    if let Ok(mut state) = self.shared.state.lock() {
                        let slot = &mut state.slots[index];
                        slot.failed = Some(NativeRequestErrorKind::Authority);
                        if let Some(entry) = &mut slot.entry {
                            entry.phase = Phase::Complete;
                        }
                    }
                    let _ = queued.reply.send(Err(NativeRequestError::caused(
                        NativeRequestErrorKind::Authority,
                        error,
                    )));
                    continue;
                }
                // The policy callback may have taken time, or the caller may
                // have dropped its future concurrently. Do not begin in either
                // case, and do not consume a request sequence.
                let begin_at = Instant::now().max(now);
                if begin_at >= queued.deadline || cancelled.is_cancelled() {
                    let kind = if begin_at >= queued.deadline {
                        NativeRequestErrorKind::Deadline
                    } else {
                        NativeRequestErrorKind::Cancelled
                    };
                    if let Ok(mut state) = self.shared.state.lock() {
                        let slot = &mut state.slots[index];
                        if slot.entry.as_ref().is_some_and(|e| e.ticket == ticket) {
                            if queued.reply.is_closed() {
                                slot.entry = None;
                            } else if let Some(entry) = &mut slot.entry {
                                entry.phase = Phase::Complete;
                            }
                        }
                    }
                    let _ = queued.reply.send(Err(NativeRequestError::new(kind)));
                    continue;
                }
                if self.shared.bulk.load(Ordering::Acquire) {
                    self.client.enable_verified_bulk_v1();
                }
                match self.client.begin_until_cancellable(
                    index,
                    queued.requests,
                    begin_at,
                    queued.deadline,
                    &cancelled,
                ) {
                    Ok(id) => {
                        self.active[index] = Some(Active {
                            ticket,
                            id,
                            cancelled,
                            deadline: queued.deadline,
                            authority: queued.authority,
                            reply: queued.reply,
                        })
                    }
                    Err(error) => {
                        if let Ok(mut state) = self.shared.state.lock() {
                            state.slots[index].failed = Some(NativeRequestErrorKind::Protocol);
                            if let Some(entry) = &mut state.slots[index].entry {
                                entry.phase = Phase::Complete;
                            }
                        }
                        let _ = queued.reply.send(Err(NativeRequestError::caused(
                            NativeRequestErrorKind::Protocol,
                            error,
                        )));
                        continue;
                    }
                }
            }
            let active = self.active[index].as_ref().expect("admitted request");
            if let Err(error) = (active.authority)() {
                self.fail_with_error(
                    index,
                    NativeRequestError::caused(NativeRequestErrorKind::Authority, error),
                );
                continue;
            }
            // A policy callback can take time, or the future can be dropped on
            // another thread. The original deadline/cancellation still apply.
            let send_at = Instant::now().max(now);
            if send_at >= active.deadline {
                self.fail(index, NativeRequestErrorKind::Deadline);
                continue;
            }
            if active.cancelled.is_cancelled() {
                self.fail(index, NativeRequestErrorKind::Cancelled);
                continue;
            }
            if let Err(error) = self.client.try_send(link, active.id, send_at, || Ok(())) {
                self.fail_with_error(
                    index,
                    NativeRequestError::caused(NativeRequestErrorKind::Transport, error),
                );
            }
        }
        Ok(())
    }
    pub fn receive(
        &mut self,
        link: &NativeObjectLink,
        body: CompletedMessage,
        now: Instant,
    ) -> Result<()> {
        match self.receive_inner(link, body, now) {
            Err(error)
                if matches!(
                    error.kind,
                    NativeRequestErrorKind::Deadline | NativeRequestErrorKind::Cancelled
                ) =>
            {
                Ok(())
            }
            Err(error) => {
                // Latch the real terminal cause before the owner drops this driver.
                // The waiting callers must not see a security error as Stopped.
                self.invalidate(error.kind);
                Err(error)
            }
            result => result,
        }
    }
    fn receive_inner(
        &mut self,
        link: &NativeObjectLink,
        body: CompletedMessage,
        now: Instant,
    ) -> Result<()> {
        self.check_session(link)?;
        if self
            .client
            .receive_capacity(link, &body, now)
            .map_err(|e| {
                let kind = if e.is::<super::native_session::CapacityNegotiationExpired>() {
                    NativeRequestErrorKind::Deadline
                } else {
                    NativeRequestErrorKind::Protocol
                };
                NativeRequestError::caused(kind, e)
            })?
        {
            let size = self.client.slot_capacity();
            self.active.resize_with(size, || None);
            self.shared.lock()?.slots.resize_with(size, Slot::default);
            if let Some(bytes) = self.client.negotiated_retained_bytes() {
                self.shared.retained_limit.store(bytes, Ordering::Release);
            }
            self.shared.wake.notify_one();
            return Ok(());
        }
        let id = match self
            .client
            .reply_request(link, &body)
            .map_err(|e| NativeRequestError::caused(NativeRequestErrorKind::Protocol, e))?
        {
            ReplyRequestV7::Retired(_) => return Ok(()),
            ReplyRequestV7::Pending(id) => id,
        };
        let active = self.active[id.slot]
            .as_ref()
            .filter(|a| a.id == id)
            .ok_or_else(|| NativeRequestError::new(NativeRequestErrorKind::Protocol))?;
        if now >= active.deadline || active.cancelled.is_cancelled() {
            let kind = if now >= active.deadline {
                NativeRequestErrorKind::Deadline
            } else {
                NativeRequestErrorKind::Cancelled
            };
            self.fail(id.slot, kind);
            // Even a canceled context must have a well-formed authenticated reply.
            self.client
                .reply_request(link, &body)
                .map_err(|e| NativeRequestError::caused(NativeRequestErrorKind::Protocol, e))?;
            return Ok(());
        }
        let authority = active.authority.clone();
        let ticket = active.ticket;
        let deadline = active.deadline;
        let cancelled = active.cancelled.clone();
        if let Err(error) = authority() {
            self.fail(id.slot, NativeRequestErrorKind::Authority);
            return Err(NativeRequestError::caused(
                NativeRequestErrorKind::Authority,
                error,
            ));
        }
        let receive_at = Instant::now().max(now);
        if receive_at >= deadline || cancelled.is_cancelled() {
            let kind = if receive_at >= deadline {
                NativeRequestErrorKind::Deadline
            } else {
                NativeRequestErrorKind::Cancelled
            };
            self.fail(id.slot, kind);
            return Ok(());
        }
        let wire_bytes = body
            .chunks
            .iter()
            .fold(0usize, |n, c| n.saturating_add(c.len()));
        if let Err(error) = self.shared.retain_reply(id.slot, ticket, wire_bytes) {
            // Retire this request before reporting local resource pressure. Other
            // healthy lanes are not invalidated, and no unbudgeted reply is decoded.
            self.fail_with_error(id.slot, error);
            return Ok(());
        }
        let calls = Cell::new(0usize);
        let denied = Cell::new(None);
        let result = self.client.receive(link, body, receive_at, || {
            let previous = calls.replace(calls.get() + 1);
            if previous == 0 {
                return Ok(());
            } // Precheck was already explicit above.
            if let Err(error) = authority() {
                denied.set(Some(NativeRequestErrorKind::Authority));
                return Err(error);
            }
            if Instant::now().max(receive_at) >= deadline {
                denied.set(Some(NativeRequestErrorKind::Deadline));
                anyhow::bail!("mailbox response deadline");
            }
            if cancelled.is_cancelled() {
                denied.set(Some(NativeRequestErrorKind::Cancelled));
                anyhow::bail!("mailbox response cancelled");
            }
            Ok(())
        });
        match result {
            Ok((received, replies)) => {
                debug_assert_eq!(received, id);
                // receive has checked the original request, authenticated session,
                // current authority, deadline and exact supported bulk bound.
                // Publish before waking callers so every cloned lane sees the
                // negotiated capability, including UDP without a carrier locator.
                if replies
                    .iter()
                    .any(|reply| matches!(reply, Reply::BulkReady { .. }))
                {
                    self.shared.bulk.store(true, Ordering::Release);
                }
                self.complete(id.slot, Ok(replies), None);
                Ok(())
            }
            Err(error) => {
                let kind = denied.get().unwrap_or(NativeRequestErrorKind::Protocol);
                self.fail(id.slot, kind);
                Err(NativeRequestError::caused(kind, error))
            }
        }
    }
    pub fn transport_complete(&mut self, event: &NativeSendCompletion) {
        if let Some(id) = self.client.transport_complete(event) {
            let kind = match event.state {
                ObjectSendState::Expired => NativeRequestErrorKind::Deadline,
                ObjectSendState::SessionReplaced => NativeRequestErrorKind::Session,
                ObjectSendState::Cancelled => NativeRequestErrorKind::Cancelled,
                _ => NativeRequestErrorKind::Transport,
            };
            if self.active[id.slot].as_ref().is_some_and(|a| a.id == id) {
                self.complete(id.slot, Err(NativeRequestError::new(kind)), Some(kind));
            }
        }
    }
    pub fn invalidate(&mut self, kind: NativeRequestErrorKind) {
        self.client.invalidate();
        if let Ok(mut state) = self.shared.state.lock() {
            // Preserve the first terminal classification: Drop must not turn a
            // protocol/security rejection into an ordinary Stopped result.
            let kind = *state.closed.get_or_insert(kind);
            for slot in &mut state.slots {
                slot.failed = Some(kind);
                if let Some(entry) = slot.entry.take() {
                    entry.cancelled.cancel();
                    if let Phase::Queued(queued) = entry.phase {
                        let _ = queued.reply.send(Err(NativeRequestError::new(kind)));
                    }
                }
            }
        }
        for active in &mut self.active {
            if let Some(active) = active.take() {
                active.cancelled.cancel();
                let _ = active.reply.send(Err(NativeRequestError::new(kind)));
            }
        }
        self.shared.wake.notify_one();
    }
}
impl Drop for NativeMailboxDriver {
    fn drop(&mut self) {
        self.invalidate(NativeRequestErrorKind::Stopped);
    }
}

#[cfg(test)]
mod capacity_tests {
    use super::*;
    use crate::service_concurrency::*;

    #[tokio::test]
    async fn owned_business_lanes_share_payload_memory_across_sessions_and_leave_control_progress()
    {
        let controller = ServiceConcurrencyV1::new(ResourceSnapshotV1 {
            cpu_parallelism: 8,
            available_memory_bytes: 8 * 1024 * 1024,
            pressure: ResourcePressureV1::Normal,
        });
        let make = || NativeMailboxHandle {
            shared: Arc::new(Shared {
                controller: controller.clone(),
                retained_limit: std::sync::atomic::AtomicUsize::new(0),
                bulk: AtomicBool::new(false),
                state: Mutex::new(State {
                    slots: (0..SLOTS).map(|_| Slot::default()).collect(),
                    next: 0,
                    closed: None,
                }),
                wake: Arc::new(Notify::new()),
            }),
        };
        let first = make();
        let second = make();
        let authority: NativeRequestAuthority = Arc::new(|| Ok(()));
        let payload = || {
            vec![Request::Put {
                mailbox: [1; 32],
                write: [2; 32],
                object: [3; 32],
                ciphertext: vec![4; 32 * 1024],
                expires: 1_000_000,
            }]
        };
        let pending: Vec<_> = (0..3)
            .map(|slot| {
                first.request(
                    slot,
                    payload(),
                    Instant::now() + REQUEST_DEADLINE,
                    authority.clone(),
                )
            })
            .collect();
        assert_eq!(
            first
                .shared
                .state
                .lock()
                .unwrap()
                .slots
                .iter()
                .filter(|s| s.entry.is_some())
                .count(),
            3
        );
        let refused = second
            .request(
                0,
                payload(),
                Instant::now() + REQUEST_DEADLINE,
                authority.clone(),
            )
            .await;
        let refused = refused.err().unwrap();
        assert_eq!(refused.kind, NativeRequestErrorKind::Capacity);
        assert!(std::error::Error::source(&refused)
            .unwrap()
            .is::<NativeRequestResourcesUnavailableV1>());
        let busy = first
            .request(
                0,
                vec![Request::ProbeBulkV1],
                Instant::now() + REQUEST_DEADLINE,
                authority.clone(),
            )
            .await
            .err()
            .unwrap();
        assert_eq!(busy.kind, NativeRequestErrorKind::Capacity);
        assert!(std::error::Error::source(&busy).is_none());
        let watch = first.request(
            3,
            vec![Request::Watch {
                boxes: vec![crate::mailbox_rpc::MailboxWatchV1 {
                    mailbox: [1; 32],
                    read: [2; 32],
                    after: None,
                }],
                wait_ms: 1,
            }],
            Instant::now() + REQUEST_DEADLINE,
            authority,
        );
        assert!(first.shared.state.lock().unwrap().slots[3].entry.is_some());
        let snapshot = controller.snapshot();
        assert!(
            snapshot.reserved_bytes > 0 && snapshot.reserved_bytes <= snapshot.retained_bytes / 2
        );
        assert_eq!(snapshot.active_work, 0);
        let local = controller
            .try_acquire_work(snapshot.retained_bytes / 4)
            .expect("local receipt still progresses");
        drop((pending, watch, local));
        assert_eq!(controller.snapshot().reserved_bytes, 0);
        assert_eq!(controller.snapshot().completed_work, 0);

        // Exercise the actual oneshot/future ownership boundary for a large
        // reply: completing a request must not release its unpolled payload.
        for consume in [false, true] {
            let future = first.request(
                0,
                vec![Request::Fetch {
                    mailbox: [1; 32],
                    read: [2; 32],
                }],
                Instant::now() + REQUEST_DEADLINE,
                Arc::new(|| Ok(())),
            );
            let (ticket, queued) = {
                let mut state = first.shared.state.lock().unwrap();
                let entry = state.slots[0].entry.as_mut().unwrap();
                let Phase::Queued(queued) = std::mem::replace(&mut entry.phase, Phase::Active)
                else {
                    panic!("expected queued request");
                };
                (entry.ticket, queued)
            };
            let before = controller.snapshot().reserved_bytes;
            first.shared.retain_reply(0, ticket, 64 * 1024).unwrap();
            let retained = controller.snapshot().reserved_bytes;
            assert!(retained > before + 64 * 1024);
            assert!(first.shared.retain_reply(0, ticket, 64 * 1024).is_err());
            assert_eq!(controller.snapshot().reserved_bytes, retained);
            first.shared.state.lock().unwrap().slots[0]
                .entry
                .as_mut()
                .unwrap()
                .phase = Phase::Complete;
            assert!(queued
                .reply
                .send(Ok(vec![Reply::Object {
                    object: [3; 32],
                    ciphertext: vec![4; 64 * 1024],
                    digest: [5; 32],
                    expires: 1_000_000,
                }]))
                .is_ok());
            assert_eq!(controller.snapshot().reserved_bytes, retained);
            if consume {
                let replies = future.await.unwrap();
                assert!(
                    matches!(&replies[0], Reply::Object { ciphertext, .. } if ciphertext.len() == 64 * 1024)
                );
            } else {
                drop(future);
            }
            assert_eq!(controller.snapshot().reserved_bytes, 0);
            assert_eq!(controller.snapshot().completed_work, 0);
        }
    }

    #[tokio::test]
    async fn capacity_negotiated_under_pressure_recovers_extra_admission_without_reconnect() {
        let resources = ResourceSnapshotV1 {
            cpu_parallelism: 16,
            available_memory_bytes: 16 * 1024 * 1024 * 1024,
            pressure: ResourcePressureV1::Severe,
        };
        let controller = ServiceConcurrencyV1::new(resources);
        controller.update_resources(resources); // Platform sample clamps live target to one.
        let count = super::super::native_session::adaptive_request_slot_ceiling_v1(&controller);
        assert_eq!(count, 32);
        let shared = Arc::new(Shared {
            controller: controller.clone(),
            retained_limit: std::sync::atomic::AtomicUsize::new(
                super::super::native_session::adaptive_retained_bytes_v1(&controller),
            ),
            bulk: AtomicBool::new(false),
            state: Mutex::new(State {
                slots: (0..count).map(|_| Slot::default()).collect(),
                next: 0,
                closed: None,
            }),
            wake: Arc::new(Notify::new()),
        });
        let handle = NativeMailboxHandle { shared };
        let authority: NativeRequestAuthority = Arc::new(|| Ok(()));
        assert_eq!(
            handle
                .request_auto(
                    vec![Request::ProbeBulkV1],
                    Instant::now() + REQUEST_DEADLINE,
                    authority.clone()
                )
                .await
                .err()
                .unwrap()
                .kind,
            NativeRequestErrorKind::Capacity
        );
        controller.update_resources(ResourceSnapshotV1 {
            pressure: ResourcePressureV1::Normal,
            ..resources
        });
        assert_eq!(handle.slot_capacity(), count);
        let pending: Vec<_> = (0..8)
            .map(|_| {
                handle.request_auto(
                    vec![Request::ProbeBulkV1],
                    Instant::now() + REQUEST_DEADLINE,
                    authority.clone(),
                )
            })
            .collect();
        let state = handle.shared.state.lock().unwrap();
        assert_eq!(state.slots.iter().filter(|s| s.entry.is_some()).count(), 8);
        assert!(state.slots[..SLOTS].iter().all(|s| s.entry.is_none()));
        drop(state);
        drop(pending);
        assert!(!handle.is_stopped());
    }

    #[tokio::test]
    async fn pressure_shrinks_extra_admission_without_dropping_inflight_or_stealing_owned_lanes() {
        let controller = ServiceConcurrencyV1::new(ResourceSnapshotV1 {
            cpu_parallelism: 16,
            available_memory_bytes: 16 * 1024 * 1024 * 1024,
            pressure: ResourcePressureV1::Normal,
        });
        let shared = Arc::new(Shared {
            controller: controller.clone(),
            retained_limit: std::sync::atomic::AtomicUsize::new(1024 * 1024 * 1024),
            bulk: AtomicBool::new(false),
            state: Mutex::new(State {
                slots: (0..32).map(|_| Slot::default()).collect(),
                next: 0,
                closed: None,
            }),
            wake: Arc::new(Notify::new()),
        });
        let handle = NativeMailboxHandle {
            shared: shared.clone(),
        };
        let authority: NativeRequestAuthority = Arc::new(|| Ok(()));
        let mut pending = Vec::new();
        for _ in 0..8 {
            pending.push(handle.request_auto(
                vec![Request::ProbeBulkV1],
                Instant::now() + REQUEST_DEADLINE,
                authority.clone(),
            ));
        }
        {
            let state = shared.state.lock().unwrap();
            assert!(state.slots[..SLOTS].iter().all(|s| s.entry.is_none()));
            assert_eq!(state.slots.iter().filter(|s| s.entry.is_some()).count(), 8);
        }
        controller.update_resources(ResourceSnapshotV1 {
            cpu_parallelism: 1,
            available_memory_bytes: 8 * 1024 * 1024,
            pressure: ResourcePressureV1::Severe,
        });
        let result = handle
            .request_auto(
                vec![Request::ProbeBulkV1],
                Instant::now() + REQUEST_DEADLINE,
                authority,
            )
            .await;
        assert_eq!(result.err().unwrap().kind, NativeRequestErrorKind::Capacity);
        assert_eq!(
            shared
                .state
                .lock()
                .unwrap()
                .slots
                .iter()
                .filter(|s| s.entry.is_some())
                .count(),
            8
        );
        assert!(!handle.is_stopped());
        drop(pending);
        assert!(shared
            .state
            .lock()
            .unwrap()
            .slots
            .iter()
            .all(|s| s.entry.is_none()));
    }
}
