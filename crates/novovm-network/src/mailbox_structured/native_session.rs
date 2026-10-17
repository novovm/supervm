//! NVMBOX07 session adapter on the existing NativeObjectLink. The embedding
//! socket owner keeps polling the SAME link while mailbox work awaits storage
//! or Watch. No socket, worker, store or identity is created here.
//!
//! Four bounded logical request slots prevent an idle Watch from owning all
//! request progress. Every slot has its own monotonic sequence, bound to the
//! authenticated transport session. Errors/cancellation latch the slot; never
//! reset counters or automatically retry an unknown business outcome.
use super::*;
use crate::{
    novorudp::{
        fragments::{CompletedMessage, FragmentProfile, OutgoingMessage},
        object_link::{
            NativeApplicationRole, NativeObjectLink, NativeSendCancellation, NativeSendCompletion,
            NativeSessionValidity,
        },
        object_sender::ObjectSendState,
    },
    product_overlay::peer_id_from_ed25519_public_key_v1,
};
use sha2::{Digest, Sha256};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub const SLOTS: usize = 4;
pub const REQUEST_DEADLINE: Duration = Duration::from_secs(30);
pub fn domain() -> [u8; 32] {
    Sha256::digest(b"novonet.mailbox.apfl.v7\0").into()
}
/// Routing hint only, not authentication or validation. Dispatch a completed
/// object from this lane to the indicated session adapter, which validates the
/// entire schema/context. This permits the same L4 peer to be client AND server.
pub fn recipient_role(body: &CompletedMessage) -> Result<NativeApplicationRole> {
    let prefix: Vec<u8> = body.chunks.iter().flatten().take(24).copied().collect();
    ensure!(prefix.len() == 24, "short mailbox routing header");
    match u64::from_le_bytes(fixed(&prefix[16..24])?) {
        REQUEST_SCHEMA => Ok(NativeApplicationRole::Service),
        REPLY_SCHEMA => Ok(NativeApplicationRole::Client),
        _ => anyhow::bail!("unknown mailbox routing schema"),
    }
}

struct Binding {
    peer: usize,
    id: String,
    session: [u8; 16],
    local: [u8; 32],
    role: NativeApplicationRole,
    validity: NativeSessionValidity,
}
impl Binding {
    fn claim(
        link: &mut NativeObjectLink,
        peer: usize,
        role: NativeApplicationRole,
    ) -> Result<Self> {
        let id = link.peer_id(peer)?.to_owned();
        let session = link.claim_application_session(peer, STREAM, domain(), role)?;
        Ok(Self {
            peer,
            id,
            session,
            local: link.local_identity(),
            role,
            validity: link.session_validity(peer)?,
        })
    }
    fn check(&self, link: &NativeObjectLink) -> Result<()> {
        ensure!(
            self.validity.is_current()
                && link.local_identity() == self.local
                && link.session_id(self.peer) == Some(self.session)
                && link.peer_id(self.peer)? == self.id,
            "native mailbox session changed"
        );
        Ok(())
    }
    fn context(&self, slot: usize, sequence: u64) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"novonet.mailbox.request.v7\0");
        hash.update(domain());
        hash.update(self.session);
        let local = peer_id_from_ed25519_public_key_v1(&self.local);
        let parties = match self.role {
            NativeApplicationRole::Client => [&local, &self.id],
            NativeApplicationRole::Service => [&self.id, &local],
        };
        for party in parties {
            hash.update((party.len() as u32).to_le_bytes());
            hash.update(party.as_bytes());
        }
        hash.update((slot as u32).to_le_bytes());
        hash.update(sequence.to_le_bytes());
        hash.finalize().into()
    }
}
fn flatten(body: &CompletedMessage) -> Result<Zeroizing<Vec<u8>>> {
    let mut length = 0usize;
    for chunk in &body.chunks {
        length = length
            .checked_add(chunk.len())
            .filter(|n| *n <= LIMITS.wire_bytes)
            .context("native mailbox wire bound")?;
    }
    let mut out = Zeroizing::new(Vec::with_capacity(length));
    for chunk in &body.chunks {
        out.extend_from_slice(chunk);
    }
    Ok(out)
}
fn outgoing(bytes: Zeroizing<Vec<u8>>) -> Result<Arc<OutgoingMessage>> {
    // The existing transport retains ordinary byte buffers. Their secure erase
    // lifecycle remains a separate integration requirement; do not claim it here.
    Ok(Arc::new(OutgoingMessage::with_profile(
        domain(),
        bytes.to_vec(),
        LIMITS.wire_bytes,
        FragmentProfile::NativeUdpV2,
    )?))
}
fn context(bytes: &[u8]) -> Result<[u8; 32]> {
    fixed(bytes.get(24..56).context("short mailbox batch")?)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestId {
    pub slot: usize,
    pub sequence: u64,
}
struct Pending {
    cancellation: NativeSendCancellation,
    id: RequestId,
    context: [u8; 32],
    requests: Vec<Request>,
    body: Arc<OutgoingMessage>,
    sent: bool,
    deadline: Instant,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}
struct RetiredRequest {
    id: RequestId,
    context: [u8; 32],
    rows: usize,
}
/// A late reply for a terminated slot is distinct from an unsolicited frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyRequestV7 {
    Pending(RequestId),
    Retired(RequestId),
}
struct ClientSlot {
    retired: Option<RetiredRequest>,
    sequence: u64,
    failed: bool,
    pending: Option<Pending>,
}
impl ClientSlot {
    fn fail_pending(&mut self) -> Option<Pending> {
        self.failed = true;
        let pending = self.pending.take()?;
        self.retired = Some(RetiredRequest {
            id: pending.id,
            context: pending.context,
            rows: pending.requests.len(),
        });
        Some(pending)
    }
}
pub struct NativeMailboxClient {
    binding: Binding,
    node: [u8; 32],
    slots: [ClientSlot; SLOTS],
}
impl NativeMailboxClient {
    pub fn invalidate(&mut self) {
        for slot in &mut self.slots {
            slot.fail_pending();
        }
    }
    pub fn check_session(&self, link: &NativeObjectLink) -> Result<()> {
        self.binding.check(link)
    }
    /// Cancel exactly this request; no sequence reset or remote rollback.
    pub fn cancel(&mut self, id: RequestId) -> bool {
        let Some(slot) = self.slots.get_mut(id.slot) else {
            return false;
        };
        if !slot.pending.as_ref().is_some_and(|p| p.id == id) {
            return false;
        }
        slot.fail_pending();
        true
    }
    /// Inspect only the authenticated peer/session and a fixed-size context
    /// prefix on the hot path. Terminated replies additionally undergo full
    /// schema validation before being classified as safe to discard.
    pub fn reply_request(
        &self,
        link: &NativeObjectLink,
        body: &CompletedMessage,
    ) -> Result<ReplyRequestV7> {
        self.binding.check(link)?;
        ensure!(body.peer == self.binding.id, "mailbox reply peer");
        let mut prefix = [0u8; 56];
        let mut source = body.chunks.iter().flatten();
        for byte in &mut prefix {
            *byte = *source.next().context("short mailbox reply")?;
        }
        let context = context(&prefix)?;
        for slot in &self.slots {
            if let Some(p) = slot.pending.as_ref().filter(|p| p.context == context) {
                return Ok(ReplyRequestV7::Pending(p.id));
            }
            if let Some(p) = slot.retired.as_ref().filter(|p| p.context == context) {
                let bytes = flatten(body)?;
                decode_replies(&bytes, p.context, p.rows)?;
                return Ok(ReplyRequestV7::Retired(p.id));
            }
        }
        anyhow::bail!("unsolicited/old mailbox reply")
    }
    /// Call only once after the existing pinned peer handshake is established.
    /// No handshake per batch; node must be that same peer, not a receipt key
    /// supplied by an unauthenticated response.
    pub fn attach(link: &mut NativeObjectLink, peer: usize, node: [u8; 32]) -> Result<Self> {
        ensure!(
            link.peer_id(peer)? == peer_id_from_ed25519_public_key_v1(&node),
            "mailbox node pin mismatch"
        );
        Ok(Self {
            binding: Binding::claim(link, peer, NativeApplicationRole::Client)?,
            node,
            slots: std::array::from_fn(|_| ClientSlot {
                retired: None,
                sequence: 1,
                failed: false,
                pending: None,
            }),
        })
    }
    /// Bounded local preparation, NOT service receipt. Caller keeps retryable
    /// business inputs/IDs until a verified outcome; there is no automatic replay.
    pub fn begin(
        &mut self,
        slot: usize,
        requests: Vec<Request>,
        now: Instant,
    ) -> Result<RequestId> {
        self.begin_until(slot, requests, now, now + REQUEST_DEADLINE)
    }
    /// Caller deadline starts before enqueueing; preparation never restarts it.
    pub fn begin_until(
        &mut self,
        slot: usize,
        requests: Vec<Request>,
        now: Instant,
        deadline: Instant,
    ) -> Result<RequestId> {
        self.begin_until_cancellable(
            slot,
            requests,
            now,
            deadline,
            &NativeSendCancellation::default(),
        )
    }
    /// Shared owner cancellation reaches retained link sends even if the future
    /// is dropped between driver progress and the next link polling quantum.
    pub(crate) fn begin_until_cancellable(
        &mut self,
        slot: usize,
        requests: Vec<Request>,
        now: Instant,
        deadline: Instant,
        cancellation: &NativeSendCancellation,
    ) -> Result<RequestId> {
        let deadline = deadline.min(now + REQUEST_DEADLINE);
        ensure!(now < deadline, "mailbox deadline before preparation");
        let s = self.slots.get_mut(slot).context("mailbox slot")?;
        ensure!(
            !s.failed && s.pending.is_none(),
            "mailbox slot busy/unusable"
        );
        let id = RequestId {
            slot,
            sequence: s.sequence,
        };
        let context = self.binding.context(slot, id.sequence);
        let body = outgoing(encode_requests(context, &requests)?)?;
        s.pending = Some(Pending {
            cancellation: cancellation.clone(),
            id,
            context,
            requests,
            body,
            sent: false,
            deadline,
        });
        Ok(id)
    }
    pub fn try_send(
        &mut self,
        link: &mut NativeObjectLink,
        id: RequestId,
        now: Instant,
        authority: impl Fn() -> Result<()>,
    ) -> Result<bool> {
        if let Err(error) = self.binding.check(link).and_then(|_| authority()) {
            self.invalidate();
            return Err(error);
        }
        let s = self.slots.get_mut(id.slot).context("mailbox slot")?;
        let p = s
            .pending
            .as_mut()
            .filter(|p| p.id == id)
            .context("mailbox pending identity")?;
        if now >= p.deadline {
            s.fail_pending();
            anyhow::bail!("mailbox deadline; outcome unknown");
        }
        if !p.sent {
            p.sent = link.try_send_cancellable_until(
                self.binding.peer,
                STREAM,
                &p.body,
                now,
                p.deadline,
                &p.cancellation,
            )?;
        }
        Ok(p.sent)
    }
    /// Consume only a completion from this link's NVMBOX07 lane. A transport
    /// DONE never calls this method and cannot turn into business success.
    pub fn receive(
        &mut self,
        link: &NativeObjectLink,
        body: CompletedMessage,
        now: Instant,
        authority: impl Fn() -> Result<()>,
    ) -> Result<(RequestId, Vec<Reply>)> {
        if let Err(error) = self.binding.check(link).and_then(|_| authority()) {
            self.invalidate();
            return Err(error);
        }
        ensure!(body.peer == self.binding.id, "mailbox reply peer");
        let bytes = flatten(&body)?;
        let context = context(&bytes)?;
        let index = self
            .slots
            .iter()
            .position(|s| s.pending.as_ref().is_some_and(|p| p.context == context))
            .context("unsolicited/old mailbox reply")?;
        let s = &mut self.slots[index];
        let p = s.fail_pending().unwrap(); // all decode, semantic, expiry and authority errors latch
        ensure!(p.sent && now < p.deadline, "mailbox late/unsent reply");
        let replies = decode_replies(&bytes, p.context, p.requests.len())?;
        for (request, reply) in p.requests.iter().zip(&replies) {
            verify_reply(self.node, request, reply)?;
        }
        authority()?;
        s.sequence = s
            .sequence
            .checked_add(1)
            .context("mailbox sequence exhausted")?;
        s.failed = false;
        s.retired = None;
        Ok((p.id, replies))
    }
    /// Feed the existing link's completion events. Returns an unknown-outcome
    /// request on expiry/replacement; Delivered releases no business request slot.
    pub fn transport_complete(&mut self, event: &NativeSendCompletion) -> Option<RequestId> {
        if event.peer != self.binding.peer || event.stream != STREAM {
            return None;
        }
        for s in &mut self.slots {
            if s.pending
                .as_ref()
                .is_some_and(|p| Arc::ptr_eq(&p.body, &event.body))
                && matches!(
                    event.state,
                    ObjectSendState::Expired
                        | ObjectSendState::SessionReplaced
                        | ObjectSendState::Cancelled
                )
            {
                return s.fail_pending().map(|p| p.id);
            }
        }
        None
    }
    pub fn expire(&mut self, now: Instant) -> Vec<RequestId> {
        let mut expired = Vec::new();
        for s in &mut self.slots {
            if s.pending
                .as_ref()
                .is_some_and(|p| now >= p.deadline || !self.binding.validity.is_current())
            {
                expired.push(s.fail_pending().unwrap().id);
            }
        }
        expired
    }
}
fn verify_reply(node: [u8; 32], request: &Request, reply: &Reply) -> Result<()> {
    if matches!(reply, Reply::Rejected(_)) {
        return Ok(());
    }
    match (request, reply) {
        (Request::ServiceRecordV7, Reply::ServiceRecordV7 { record }) => {
            ensure!(
                record.relay_public_key == node,
                "mailbox locator node binding"
            );
            crate::product_directory::validate_relay_record_v1(
                record,
                crate::product_rendezvous_daemon::now_ms()?,
            )?;
            let carrier =
                crate::product_service_lease::NativeCarrierMailboxEndpointV7::from_record(record)?;
            ensure!(
                carrier.is_some()
                    || crate::product_service_lease::NativeMailboxEndpointV7::from_record(record)?
                        .is_some(),
                "mailbox native locator missing"
            );
            Ok(())
        }
        (Request::Register { .. }, Reply::Registered)
        | (Request::Consume { .. }, Reply::Consumed)
        | (Request::Renew { .. }, Reply::Renewed)
        | (Request::Revoke { .. }, Reply::Revoked)
        | (Request::Fetch { .. }, Reply::Empty) => Ok(()),
        (
            Request::Put {
                mailbox,
                object,
                ciphertext,
                expires,
                ..
            },
            Reply::Stored(r),
        ) => r.verify(node, *mailbox, *object, ciphertext, *expires),
        (
            Request::Fetch { .. },
            Reply::Object {
                ciphertext, digest, ..
            },
        ) => {
            ensure!(
                <[u8; 32]>::from(Sha256::digest(ciphertext)) == *digest,
                "mailbox object digest"
            );
            Ok(())
        }
        (Request::Watch { boxes, .. }, Reply::Notice(notices)) => {
            ensure!(boxes.len() == notices.len(), "mailbox notice shape");
            Ok(())
        }
        (Request::WatchFetchV6 { boxes, .. }, Reply::NoticeObjectV6 { notices, object }) => {
            ensure!(boxes.len() == notices.len(), "mailbox notice shape");
            if let Some(o) = object {
                ensure!(
                    o.box_index < boxes.len()
                        && <[u8; 32]>::from(Sha256::digest(&o.ciphertext)) == o.digest,
                    "mailbox prefetch binding"
                );
            }
            Ok(())
        }
        _ => anyhow::bail!("mailbox response operation mismatch"),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Ready,
    Working,
    Failed,
}
struct ServerSlot {
    sequence: u64,
    phase: Phase,
}
type Slots = Arc<Mutex<[ServerSlot; SLOTS]>>;
pub struct NativeMailboxServer {
    binding: Binding,
    cancellation: NativeSendCancellation,
    slots: Slots,
}
impl NativeMailboxServer {
    pub fn invalidate(&mut self) {
        self.cancellation.cancel();
        if let Ok(mut slots) = self.slots.lock() {
            for slot in slots.iter_mut() {
                slot.phase = Phase::Failed;
            }
        }
    }
    pub fn attach(link: &mut NativeObjectLink, peer: usize) -> Result<Self> {
        Ok(Self {
            binding: Binding::claim(link, peer, NativeApplicationRole::Service)?,
            cancellation: NativeSendCancellation::default(),
            slots: Arc::new(Mutex::new(std::array::from_fn(|_| ServerSlot {
                sequence: 1,
                phase: Phase::Ready,
            }))),
        })
    }
    /// Owner must hold at most one adapter per authenticated peer and retain its
    /// node-wide link/output budgets. Returned work is Send and does not borrow
    /// the link: poll network and other slots while it awaits the original owner.
    pub fn admit(
        &mut self,
        link: &NativeObjectLink,
        body: CompletedMessage,
        now: Instant,
        authority: impl Fn() -> Result<()>,
    ) -> Result<MailboxWork> {
        if let Err(error) = self.binding.check(link).and_then(|_| authority()) {
            self.invalidate();
            return Err(error);
        }
        ensure!(
            !self.cancellation.is_cancelled(),
            "mailbox service owner cancelled"
        );
        ensure!(body.peer == self.binding.id, "mailbox request peer");
        let bytes = flatten(&body)?;
        let context = context(&bytes)?;
        let mut slots = self
            .slots
            .lock()
            .map_err(|_| anyhow::anyhow!("mailbox owner poisoned"))?;
        let index = slots
            .iter()
            .enumerate()
            .position(|(i, s)| {
                s.phase == Phase::Ready && self.binding.context(i, s.sequence) == context
            })
            .context("mailbox replay/busy/unknown request")?;
        let batch = RequestBatch::decode(&bytes, context)?; // ALL rows before any durable effects
        let waiting_watch = batch.has_waiting_watch()?;
        let rows = batch.len();
        slots[index].phase = Phase::Working;
        Ok(MailboxWork {
            waiting_watch,
            rows,
            bytes,
            context,
            guard: WorkGuard {
                slots: self.slots.clone(),
                slot: index,
                sequence: slots[index].sequence,
                armed: true,
                validity: self.binding.validity.clone(),
                cancellation: self.cancellation.clone(),
            },
            peer: self.binding.peer,
            session: self.binding.session,
            signer: self.binding.local,
            deadline: now + REQUEST_DEADLINE,
        })
    }
}
impl Drop for NativeMailboxServer {
    fn drop(&mut self) {
        self.invalidate();
    }
}
struct WorkGuard {
    slots: Slots,
    slot: usize,
    sequence: u64,
    armed: bool,
    validity: NativeSessionValidity,
    cancellation: NativeSendCancellation,
}
impl WorkGuard {
    fn check(&self) -> Result<()> {
        ensure!(
            self.validity.is_current() && !self.cancellation.is_cancelled(),
            "mailbox transport session retired/cancelled"
        );
        let slots = self
            .slots
            .lock()
            .map_err(|_| anyhow::anyhow!("mailbox owner poisoned"))?;
        ensure!(
            slots[self.slot].sequence == self.sequence && slots[self.slot].phase == Phase::Working,
            "mailbox work no longer authorized"
        );
        Ok(())
    }
    fn fail(&mut self) {
        if let Ok(mut slots) = self.slots.lock() {
            let s = &mut slots[self.slot];
            if s.sequence == self.sequence {
                s.phase = Phase::Failed;
            }
        }
    }
}
impl Drop for WorkGuard {
    fn drop(&mut self) {
        if self.armed {
            if let Ok(mut slots) = self.slots.lock() {
                let s = &mut slots[self.slot];
                if s.sequence == self.sequence {
                    s.phase = Phase::Failed;
                }
            }
        }
    }
}
pub struct MailboxWork {
    waiting_watch: bool,
    rows: usize,
    bytes: Zeroizing<Vec<u8>>,
    context: [u8; 32],
    guard: WorkGuard,
    peer: usize,
    session: [u8; 16],
    signer: [u8; 32],
    deadline: Instant,
}
impl MailboxWork {
    pub fn has_waiting_watch(&self) -> bool {
        self.waiting_watch
    }

    /// Bounded overload reply on the SAME authenticated request slot. No row is
    /// executed, and the slot advances only when the response is enqueued by
    /// try_send. This is never a storage or delivery acknowledgement.
    pub fn reject_quota(
        self,
        service: &MailboxServiceV1,
        authority: impl Fn() -> Result<()>,
    ) -> Result<MailboxResponse> {
        ensure!(Instant::now() < self.deadline, "mailbox rejection expired");
        ensure!(
            service.identity_public_key() == self.signer,
            "mailbox signer differs from authenticated node"
        );
        self.guard.check()?;
        authority()?;
        let replies = (0..self.rows)
            .map(|_| Reply::Rejected(MailboxRejectionV1::Quota))
            .collect::<Vec<_>>();
        let bytes = encode_replies(self.context, &replies)?;
        Ok(MailboxResponse {
            body: outgoing(bytes)?,
            guard: self.guard,
            peer: self.peer,
            session: self.session,
            deadline: self.deadline,
            sent: false,
            failed: false,
        })
    }

    pub async fn execute(
        self,
        service: &MailboxServiceV1,
        authority: impl Fn() -> Result<()>,
    ) -> Result<MailboxResponse> {
        ensure!(
            Instant::now() < self.deadline,
            "mailbox work expired before execution"
        );
        ensure!(
            service.identity_public_key() == self.signer,
            "mailbox signer differs from authenticated node"
        );
        let bytes = tokio::time::timeout_at(
            self.deadline.into(),
            execute_local_batch(service, &self.bytes, self.context, || {
                self.guard.check()?;
                authority()
            }),
        )
        .await
        .context("mailbox deadline; outcome unknown")??;
        Ok(MailboxResponse {
            body: outgoing(bytes)?,
            guard: self.guard,
            peer: self.peer,
            session: self.session,
            deadline: self.deadline,
            sent: false,
            failed: false,
        })
    }
}

pub struct MailboxResponse {
    failed: bool,
    body: Arc<OutgoingMessage>,
    guard: WorkGuard,
    peer: usize,
    session: [u8; 16],
    deadline: Instant,
    sent: bool,
}

impl MailboxResponse {
    /// false retains THIS handle for retry; do not drop/rebuild it. Successful
    /// enqueue advances this slot only after owner work completed. Later sender
    /// expiry remains an unknown client outcome, not storage rollback.
    pub fn try_send(
        &mut self,
        link: &mut NativeObjectLink,
        now: Instant,
        authority: impl Fn() -> Result<()>,
    ) -> Result<bool> {
        ensure!(!self.failed, "mailbox response cancelled");
        if let Err(error) = authority().and_then(|_| {
            ensure!(
                self.guard.validity.is_current() && !self.guard.cancellation.is_cancelled(),
                "mailbox transport session retired"
            );
            ensure!(
                link.session_id(self.peer) == Some(self.session),
                "mailbox response session changed"
            );
            ensure!(
                now < self.deadline,
                "mailbox response deadline; outcome unknown"
            );
            Ok(())
        }) {
            self.failed = true;
            link.cancel_send(self.peer, STREAM, &self.body);
            self.guard.fail();
            return Err(error);
        }
        if !self.sent {
            let mut slots = self
                .guard
                .slots
                .lock()
                .map_err(|_| anyhow::anyhow!("mailbox owner poisoned"))?;
            let s = &mut slots[self.guard.slot];
            ensure!(
                s.phase == Phase::Working && s.sequence == self.guard.sequence,
                "mailbox response owner mismatch"
            );
            let next = s
                .sequence
                .checked_add(1)
                .context("mailbox sequence exhausted")?;
            if !link.try_send_cancellable_until(
                self.peer,
                STREAM,
                &self.body,
                now,
                self.deadline,
                &self.guard.cancellation,
            )? {
                return Ok(false);
            }
            s.sequence = next;
            s.phase = Phase::Ready;
            self.guard.armed = false;
            self.sent = true;
        }
        Ok(self.sent)
    }
}

#[cfg(test)]
mod tests;
