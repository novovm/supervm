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
            NativeApplicationRole, NativeObjectLink, NativeSendCompletion, NativeSessionValidity,
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
    id: RequestId,
    context: [u8; 32],
    requests: Vec<Request>,
    body: Arc<OutgoingMessage>,
    sent: bool,
    deadline: Instant,
}
struct ClientSlot {
    sequence: u64,
    failed: bool,
    pending: Option<Pending>,
}
pub struct NativeMailboxClient {
    binding: Binding,
    node: [u8; 32],
    slots: [ClientSlot; SLOTS],
}
impl NativeMailboxClient {
    pub fn invalidate(&mut self) {
        for slot in &mut self.slots {
            slot.failed = true;
            slot.pending = None;
        }
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
            id,
            context,
            requests,
            body,
            sent: false,
            deadline: now + REQUEST_DEADLINE,
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
            s.failed = true;
            s.pending = None;
            anyhow::bail!("mailbox deadline; outcome unknown");
        }
        if !p.sent {
            p.sent = link.try_send(self.binding.peer, STREAM, &p.body, now)?;
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
        let p = s.pending.take().unwrap();
        s.failed = true; // all decode, semantic, expiry and authority errors latch
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
                    ObjectSendState::Expired | ObjectSendState::SessionReplaced
                )
            {
                s.failed = true;
                return s.pending.take().map(|p| p.id);
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
                s.failed = true;
                expired.push(s.pending.take().unwrap().id);
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
    slots: Slots,
}
impl NativeMailboxServer {
    pub fn invalidate(&mut self) {
        if let Ok(mut slots) = self.slots.lock() {
            for slot in slots.iter_mut() {
                slot.phase = Phase::Failed;
            }
        }
    }
    pub fn attach(link: &mut NativeObjectLink, peer: usize) -> Result<Self> {
        Ok(Self {
            binding: Binding::claim(link, peer, NativeApplicationRole::Service)?,
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
        RequestBatch::decode(&bytes, context)?; // ALL rows before any durable effects
        slots[index].phase = Phase::Working;
        Ok(MailboxWork {
            bytes,
            context,
            guard: WorkGuard {
                slots: self.slots.clone(),
                slot: index,
                sequence: slots[index].sequence,
                armed: true,
                validity: self.binding.validity.clone(),
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
}
impl WorkGuard {
    fn check(&self) -> Result<()> {
        ensure!(
            self.validity.is_current(),
            "mailbox transport session retired"
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
    bytes: Zeroizing<Vec<u8>>,
    context: [u8; 32],
    guard: WorkGuard,
    peer: usize,
    session: [u8; 16],
    signer: [u8; 32],
    deadline: Instant,
}
impl MailboxWork {
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
        })
    }
}

pub struct MailboxResponse {
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
        if let Err(error) = authority().and_then(|_| {
            ensure!(
                self.guard.validity.is_current(),
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
            if !link.try_send(self.peer, STREAM, &self.body, now)? {
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
