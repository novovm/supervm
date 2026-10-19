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
    slots: [Slot; SLOTS],
    next: u64,
    closed: Option<NativeRequestErrorKind>,
}
struct Shared {
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
impl NativeMailboxHandle {
    /// Only after validating the node-signed bulk-v1 locator for this session.
    pub fn enable_verified_bulk_v1(&self) {
        self.shared.bulk.store(true, Ordering::Release);
    }
    pub fn supports_bulk_v1(&self) -> bool {
        self.shared.bulk.load(Ordering::Acquire)
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
        match self.enqueue(slot, requests, deadline, authority) {
            Ok(future) => future,
            Err(error) => NativeRequestFuture::error(error),
        }
    }
    fn enqueue(
        &self,
        index: usize,
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
        let ticket = {
            let mut state = self.shared.lock()?;
            if let Some(kind) = state.closed {
                return Err(NativeRequestError::new(kind));
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
            state.next = state
                .next
                .checked_add(1)
                .ok_or_else(|| NativeRequestError::new(NativeRequestErrorKind::Stopped))?;
            let ticket = state.next;
            state.slots[index].entry = Some(Entry {
                ticket,
                cancelled,
                phase: Phase::Preparing,
            });
            ticket
        };
        let reservation = Reservation {
            shared: self.shared.clone(),
            slot: index,
            ticket,
        };
        // Existing codec validates all lengths/shapes before building columns.
        // Only four reserved callers may prepare; no ciphertext is cloned into
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
    active: [Option<Active>; SLOTS],
}
impl NativeMailboxDriver {
    pub fn attach(
        link: &mut NativeObjectLink,
        peer: usize,
        node: [u8; 32],
    ) -> Result<(Self, NativeMailboxHandle)> {
        let client = NativeMailboxClient::attach(link, peer, node)
            .map_err(|error| NativeRequestError::caused(NativeRequestErrorKind::Session, error))?;
        let shared = Arc::new(Shared {
            bulk: AtomicBool::new(false),
            state: Mutex::new(State {
                slots: std::array::from_fn(|_| Slot::default()),
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
                active: std::array::from_fn(|_| None),
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
                    // Pending::Drop also cancels a successfully completed transport.
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
        for index in 0..SLOTS {
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
