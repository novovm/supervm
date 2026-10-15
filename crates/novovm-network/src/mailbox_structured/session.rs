//! NVMBOX07 request ownership on the existing native object link. No socket,
//! spawned task, retry loop or second mailbox. The application/node owner keeps
//! ONE client/server state per authenticated session and drives independent jobs.
//! A Watch job must not block polling or unrelated Put/Fetch jobs.
//!
//! Received objects must come from that link's NVMBOX07 poll_complete. Transport
//! completion is not custody. Unknown outcomes retain original business IDs;
//! retry is a NEW request sequence, never a reset of this live session state.
use super::{decode_replies, encode_requests, execute_local_batch, RequestBatch, LIMITS};
use crate::{
    mailbox_rpc::{MailboxReplyV1, MailboxRequestV1, MailboxServiceV1},
    novorudp::{
        fragments::{CompletedMessage, FragmentProfile, OutgoingMessage},
        object_link::NativeObjectLink,
    },
    product_overlay::{peer_id_from_ed25519_public_key_v1, SlidingReplayWindowV1},
};
use anyhow::{ensure, Context, Result};
use novovm_protocol::apfl_batch;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc, time::Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use zeroize::Zeroizing;

const WINDOW: u64 = 128;
const MAX_PENDING: usize = 32;

/// Fragment domain for the explicitly versioned NVMBOX07 stream.
pub fn object_domain() -> [u8; 32] {
    Sha256::digest(b"novovm/NVMBOX07/object/v1\0").into()
}

#[derive(Clone)]
pub struct SessionBinding {
    peer: usize,
    identity: String,
    local_key: [u8; 32],
    session: [u8; 16],
    request_scope: [u8; 32],
}
impl SessionBinding {
    fn capture(link: &NativeObjectLink, peer: usize, client: bool) -> Result<Self> {
        ensure!(
            link.lane_domain(super::STREAM)? == object_domain(),
            "native mailbox lane domain"
        );
        let session = link.session_id(peer).context("native session not ready")?;
        let identity = link.peer_id(peer)?.to_owned();
        let local_key = link.local_public_key();
        let local = peer_id_from_ed25519_public_key_v1(&local_key);
        let (sender, receiver) = if client {
            (&local, &identity)
        } else {
            (&identity, &local)
        };
        let mut hash = Sha256::new();
        hash.update(b"novovm/NVMBOX07/request-scope/v1\0");
        hash.update(session);
        for id in [sender, receiver] {
            hash.update((id.len() as u64).to_le_bytes());
            hash.update(id.as_bytes());
        }
        Ok(Self {
            peer,
            identity,
            local_key,
            session,
            request_scope: hash.finalize().into(),
        })
    }

    /// Call from the SAME owner, along with its live revocation/privacy/resource
    /// checks. This checks transport identity, not UCA ownership or entitlement.
    pub fn check(&self, link: &NativeObjectLink) -> Result<()> {
        ensure!(
            link.local_public_key() == self.local_key
                && link.session_id(self.peer) == Some(self.session)
                && link.peer_id(self.peer)? == self.identity,
            "native mailbox session changed"
        );
        Ok(())
    }

    fn context(&self, sequence: u64) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"novovm/NVMBOX07/request/v1\0");
        hash.update(self.request_scope);
        hash.update(sequence.to_le_bytes());
        let digest = hash.finalize();
        let mut context = [0; 32];
        context[..8].copy_from_slice(&sequence.to_le_bytes());
        context[8..].copy_from_slice(&digest[..24]);
        context
    }

    fn receive(
        &self,
        link: &NativeObjectLink,
        body: CompletedMessage,
    ) -> Result<Zeroizing<Vec<u8>>> {
        // Wipe the completed original chunks as well as the flattened adapter
        // buffer. Lower transport buffers remain subject to their own lifecycle.
        let chunks = Zeroizing::new(body.chunks);
        self.check(link)?;
        ensure!(
            body.peer == self.identity,
            "native mailbox response/request peer"
        );
        let size = chunks
            .iter()
            .try_fold(0usize, |n, c| n.checked_add(c.len()))
            .context("native mailbox size overflow")?;
        ensure!(size <= LIMITS.wire_bytes, "native mailbox object budget");
        Ok(Zeroizing::new(chunks.concat()))
    }
}

fn wire_context(bytes: &[u8], schema: u64) -> Result<(u64, [u8; 32])> {
    let view = apfl_batch::decode(bytes, schema, LIMITS).map_err(anyhow::Error::msg)?;
    let sequence = u64::from_le_bytes(view.context[..8].try_into()?);
    ensure!(sequence != 0, "zero native mailbox request sequence");
    Ok((sequence, view.context))
}
fn object(bytes: Zeroizing<Vec<u8>>) -> Result<Arc<OutgoingMessage>> {
    Ok(Arc::new(OutgoingMessage::with_profile(
        object_domain(),
        bytes.to_vec(),
        LIMITS.wire_bytes,
        FragmentProfile::NativeUdpV2,
    )?))
}

struct Pending {
    context: [u8; 32],
    object: [u8; 32],
    count: usize,
    deadline: Instant,
}
pub struct PreparedRequest {
    pub sequence: u64,
    pub body: Arc<OutgoingMessage>,
}

pub struct NativeMailboxClient {
    binding: SessionBinding,
    next: u64,
    pending: BTreeMap<u64, Pending>,
}
impl NativeMailboxClient {
    pub fn new(link: &NativeObjectLink, peer: usize) -> Result<Self> {
        Ok(Self {
            binding: SessionBinding::capture(link, peer, true)?,
            next: 1,
            pending: BTreeMap::new(),
        })
    }
    /// Prepare once, then keep the same body when try_send is backpressured.
    /// No handshake, chain query, periodic service license or transport rebuild.
    pub fn prepare(
        &mut self,
        link: &NativeObjectLink,
        requests: &[MailboxRequestV1],
        now: Instant,
        deadline: Instant,
    ) -> Result<PreparedRequest> {
        self.binding.check(link)?;
        ensure!(now < deadline, "native mailbox request deadline");
        ensure!(
            self.pending.len() < MAX_PENDING,
            "native mailbox pending budget"
        );
        if let Some((&oldest, _)) = self.pending.first_key_value() {
            ensure!(
                self.next - oldest < WINDOW,
                "native mailbox outstanding sequence window"
            );
        }
        let next = self
            .next
            .checked_add(1)
            .context("native mailbox sequence exhausted")?;
        let context = self.binding.context(self.next);
        let body = object(encode_requests(context, requests)?)?;
        let sequence = self.next;
        self.pending.insert(
            sequence,
            Pending {
                context,
                object: body.id(),
                count: requests.len(),
                deadline,
            },
        );
        self.next = next;
        Ok(PreparedRequest { sequence, body })
    }

    /// Preserve the business deadline inside the original transport sender.
    /// A false result is backpressure; retain the same prepared request.
    pub fn try_send(
        &self,
        link: &mut NativeObjectLink,
        prepared: &PreparedRequest,
        now: Instant,
        authority: impl Fn(&SessionBinding) -> Result<()>,
    ) -> Result<bool> {
        self.binding.check(link)?;
        authority(&self.binding)?;
        let pending = self
            .pending
            .get(&prepared.sequence)
            .context("unknown native mailbox request")?;
        ensure!(
            prepared.body.id() == pending.object,
            "native mailbox prepared object changed"
        );
        link.try_send_until(
            self.binding.peer,
            super::STREAM,
            &prepared.body,
            now,
            pending.deadline,
        )
    }

    pub fn receive(
        &mut self,
        link: &NativeObjectLink,
        body: CompletedMessage,
        now: Instant,
    ) -> Result<(u64, Vec<MailboxReplyV1>)> {
        let bytes = self.binding.receive(link, body)?;
        let (sequence, context) = wire_context(&bytes, super::REPLY_SCHEMA)?;
        let pending = self
            .pending
            .get(&sequence)
            .context("unknown/completed native mailbox response")?;
        ensure!(now < pending.deadline, "native mailbox response deadline");
        ensure!(
            context == pending.context,
            "native mailbox response context"
        );
        let replies = decode_replies(&bytes, context, pending.count)?;
        self.pending.remove(&sequence);
        // Caller verifies Stored against the requested object/provider, and
        // separately verifies DeviceReceipt. Parsing is not a delivery claim.
        Ok((sequence, replies))
    }

    /// Owner cancellation/deadline => UNKNOWN outcome, never rollback/Delivered.
    /// Returns whether this sequence was outstanding; does not reset numbering.
    pub fn abandon(&mut self, sequence: u64) -> bool {
        self.pending.remove(&sequence).is_some()
    }
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

/// Share ONE budget across all peers of a node, in addition to transport byte
/// quotas and the original store quotas. It is local resource protection.
#[derive(Clone)]
pub struct NativeMailboxBudget(Arc<Semaphore>);
impl NativeMailboxBudget {
    pub fn new(operations: usize) -> Result<Self> {
        ensure!(
            (1..=64).contains(&operations),
            "native mailbox active budget"
        );
        Ok(Self(Arc::new(Semaphore::new(operations))))
    }
}

pub struct NativeMailboxServer {
    binding: SessionBinding,
    replay: SlidingReplayWindowV1,
    budget: NativeMailboxBudget,
}
impl NativeMailboxServer {
    pub fn new(link: &NativeObjectLink, peer: usize, budget: NativeMailboxBudget) -> Result<Self> {
        Ok(Self {
            binding: SessionBinding::capture(link, peer, false)?,
            replay: SlidingReplayWindowV1::new(WINDOW as u32),
            budget,
        })
    }
    /// Dispatch from the existing owner; no await and no store mutation here.
    /// Keep this state until the native session ends, including after job errors.
    pub fn accept(
        &mut self,
        link: &NativeObjectLink,
        body: CompletedMessage,
        now: Instant,
        deadline: Instant,
    ) -> Result<NativeMailboxJob> {
        let bytes = self.binding.receive(link, body)?;
        ensure!(now < deadline, "native mailbox operation deadline");
        let (sequence, context) = wire_context(&bytes, super::REQUEST_SCHEMA)?;
        ensure!(
            context == self.binding.context(sequence),
            "native mailbox request context"
        );
        RequestBatch::decode(&bytes, context)?;
        let permit = self
            .budget
            .0
            .clone()
            .try_acquire_owned()
            .context("native mailbox active operation budget")?;
        ensure!(
            self.replay.mark_authenticated(sequence),
            "native mailbox request replay"
        );
        Ok(NativeMailboxJob {
            binding: self.binding.clone(),
            bytes,
            context,
            deadline,
            _permit: permit,
        })
    }
}

/// Owned by the existing caller's bounded operation set. Drop releases capacity
/// on cancellation; replay is NOT forgotten, and committed custody is retained.
pub struct NativeMailboxJob {
    binding: SessionBinding,
    bytes: Zeroizing<Vec<u8>>,
    context: [u8; 32],
    deadline: Instant,
    _permit: OwnedSemaphorePermit,
}
impl NativeMailboxJob {
    /// Authority must inspect the current owner's session, revocation and path
    /// policy, not a captured boolean. No lock on NativeObjectLink across await.
    pub async fn execute(
        self,
        service: &MailboxServiceV1,
        authority: impl Fn(&SessionBinding) -> Result<()>,
    ) -> Result<NativeMailboxResponse> {
        let encoded = tokio::time::timeout_at(
            self.deadline.into(),
            execute_local_batch(service, &self.bytes, self.context, || {
                ensure!(
                    Instant::now() < self.deadline,
                    "native mailbox operation deadline"
                );
                authority(&self.binding)
            }),
        )
        .await
        .context("native mailbox operation deadline; outcome may be committed")??;
        Ok(NativeMailboxResponse {
            binding: self.binding.clone(),
            deadline: self.deadline,
            body: object(encoded)?,
            _permit: self._permit,
        })
    }
}

pub struct NativeMailboxResponse {
    binding: SessionBinding,
    deadline: Instant,
    body: Arc<OutgoingMessage>,
    _permit: OwnedSemaphorePermit,
}

#[cfg(test)]
mod tests;
impl NativeMailboxResponse {
    /// Original immutable reply survives backpressure; the owner retains it
    /// within its byte/operation budget. Recheck live policy before each call.
    pub fn try_send(
        &self,
        link: &mut NativeObjectLink,
        now: Instant,
        authority: impl Fn(&SessionBinding) -> Result<()>,
    ) -> Result<bool> {
        ensure!(now < self.deadline, "native mailbox reply deadline");
        self.binding.check(link)?;
        authority(&self.binding)?;
        link.try_send_until(
            self.binding.peer,
            super::STREAM,
            &self.body,
            now,
            self.deadline,
        )
    }
}
