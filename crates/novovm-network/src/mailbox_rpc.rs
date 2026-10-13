//! One L3/L4 protocol over the existing Finished-bound carrier. No chat IDs.
//! Owner admits the service path; credentials are never sent before peer authentication.
use crate::{
    bound_session::{authenticate_allowed_service_client, authenticate_bound_session},
    carrier_stream::CarrierStreamV1,
    duplex::{HandshakeReplayCacheV1, NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0},
    iroh_transport::{decode_iroh_envelope_v1, encode_iroh_envelope_v1},
    mailbox_receipt::StoreReceiptV1,
    mailbox_store::{Capability, MailboxCursorV1, MailboxGrant, MailboxStore, StoreError},
    transport_binding::BoundSecureChannelV1,
};
use anyhow::{ensure, Result};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use zeroize::{Zeroize, Zeroizing};

pub const MAX_SESSION_REQUESTS_V2: u16 = 64;

const STREAM: u64 = 0x4e564d424f5831;
const CHUNK: usize = 8192;
const MAX_WIRE: usize = 300_000; // 64 KiB ciphertext in bounded JSON byte-array representation.

// No Debug. These capability-bearing values must never enter logs/status.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MailboxRequestV1 {
    /// Explicit opt-in on NVMBOX02; never a downgrade on an older service.
    BeginSession,
    EndSession,
    /// Bounded notification wait; each position is private to an authorized box.
    /// The caller snapshots before Fetch, then watches from that position.
    Watch {
        boxes: Vec<MailboxWatchV1>,
        wait_ms: u32,
    },
    Register {
        mailbox: [u8; 32],
        write: [u8; 32],
        read: [u8; 32],
        expires: i64,
    },
    Put {
        mailbox: [u8; 32],
        write: [u8; 32],
        object: [u8; 32],
        ciphertext: Vec<u8>,
        expires: i64,
    },
    Fetch {
        mailbox: [u8; 32],
        read: [u8; 32],
    },
    Consume {
        mailbox: [u8; 32],
        read: [u8; 32],
        object: [u8; 32],
        digest: [u8; 32],
    },
    Renew {
        mailbox: [u8; 32],
        read: [u8; 32],
        ttl: i64,
    },
    Revoke {
        mailbox: [u8; 32],
        read: [u8; 32],
    },
}
impl Drop for MailboxRequestV1 {
    fn drop(&mut self) {
        match self {
            Self::BeginSession | Self::EndSession | Self::Watch { .. } => {} // each item zeroizes its own read capability
            Self::Register { write, read, .. } => {
                write.zeroize();
                read.zeroize();
            }
            Self::Put { write, .. } => write.zeroize(),
            Self::Fetch { read, .. }
            | Self::Consume { read, .. }
            | Self::Renew { read, .. }
            | Self::Revoke { read, .. } => read.zeroize(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailboxWatchV1 {
    pub mailbox: [u8; 32],
    pub read: [u8; 32],
    pub after: Option<MailboxCursorV1>,
}
impl Drop for MailboxWatchV1 {
    fn drop(&mut self) {
        self.read.zeroize();
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailboxNoticeV1 {
    pub cursor: MailboxCursorV1,
    pub pending: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MailboxReplyV1 {
    SessionReady,
    SessionEnded,
    Notice(Vec<MailboxNoticeV1>),
    Registered,
    Stored(StoreReceiptV1),
    Object {
        object: [u8; 32],
        ciphertext: Vec<u8>,
        digest: [u8; 32],
        expires: i64,
    },
    Empty,
    Consumed,
    Renewed,
    Revoked,
    Rejected(MailboxRejectionV1),
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MailboxRejectionV1 {
    Unauthorized,
    Invalid,
    Quota,
    Conflict,
    Consumed,
    Storage,
}

/// One bounded store owner, shared across L3/L4 runtimes. Blocking durable work
/// runs off the socket executor; cancellation cannot manufacture/undo a commit.
pub struct MailboxServiceV1 {
    store: Arc<Mutex<MailboxStore>>,
    signer: SigningKey,
    slots: Arc<tokio::sync::Semaphore>,
    changed: tokio::sync::watch::Sender<u64>,
}
impl MailboxServiceV1 {
    pub fn new(store: MailboxStore, signer: SigningKey) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            signer,
            slots: Arc::new(tokio::sync::Semaphore::new(1)),
            changed: tokio::sync::watch::channel(0).0,
        }
    }
    pub async fn sweep(&self) -> Result<()> {
        let permit = self.slots.clone().acquire_owned().await?;
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            store
                .lock()
                .map_err(|_| anyhow::anyhow!("mailbox store unavailable"))?
                .sweep(now()?)?;
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }
    async fn execute(&self, request: MailboxRequestV1) -> Result<MailboxReplyV1> {
        if let MailboxRequestV1::Watch { boxes, wait_ms } = &request {
            return self.watch(boxes, *wait_ms).await;
        }
        let permit = self.slots.clone().acquire_owned().await?;
        let store = self.store.clone();
        let signer = self.signer.clone();
        let changed = self.changed.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut store = store
                .lock()
                .map_err(|_| anyhow::anyhow!("mailbox store unavailable"))?;
            let result = (|| -> std::result::Result<MailboxReplyV1, StoreError> {
                let now = now().map_err(|_| StoreError::Invalid)?;
                match &request {
                    MailboxRequestV1::BeginSession | MailboxRequestV1::EndSession => {
                        Err(StoreError::Invalid)
                    }
                    MailboxRequestV1::Watch { .. } => {
                        unreachable!("watch dispatched before store worker")
                    }
                    MailboxRequestV1::Register {
                        mailbox,
                        write,
                        read,
                        expires,
                    } => {
                        let grant = MailboxGrant {
                            id: *mailbox,
                            write: Capability::from_bytes(*write)?,
                            read: Capability::from_bytes(*read)?,
                        };
                        store.register(&grant, now, *expires)?;
                        Ok(MailboxReplyV1::Registered)
                    }
                    MailboxRequestV1::Put {
                        mailbox,
                        write,
                        object,
                        ciphertext,
                        expires,
                    } => {
                        store.put(
                            mailbox,
                            &Capability::from_bytes(*write)?,
                            object,
                            ciphertext,
                            *expires,
                            now,
                        )?;
                        Ok(MailboxReplyV1::Stored(StoreReceiptV1::committed(
                            &signer, *mailbox, *object, ciphertext, *expires,
                        )))
                    }
                    MailboxRequestV1::Fetch { mailbox, read } => {
                        // Non-destructive; consumer only removes after its own durable commit.
                        match store
                            .fetch(mailbox, &Capability::from_bytes(*read)?, now)?
                            .into_iter()
                            .next()
                        {
                            Some(row) => Ok(MailboxReplyV1::Object {
                                object: row.id,
                                ciphertext: row.ciphertext,
                                digest: row.digest,
                                expires: row.expires,
                            }),
                            None => Ok(MailboxReplyV1::Empty),
                        }
                    }
                    MailboxRequestV1::Consume {
                        mailbox,
                        read,
                        object,
                        digest,
                    } => {
                        store.consume(
                            mailbox,
                            &Capability::from_bytes(*read)?,
                            object,
                            digest,
                            now,
                        )?;
                        Ok(MailboxReplyV1::Consumed)
                    }
                    MailboxRequestV1::Renew { mailbox, read, ttl } => {
                        store.renew(mailbox, &Capability::from_bytes(*read)?, now, *ttl)?;
                        Ok(MailboxReplyV1::Renewed)
                    }
                    MailboxRequestV1::Revoke { mailbox, read } => {
                        store.revoke(mailbox, &Capability::from_bytes(*read)?, now)?;
                        Ok(MailboxReplyV1::Revoked)
                    }
                }
            })();
            // Inside the blocking commit worker: even cancellation after commit
            // cannot drop the wakeup. Private durable positions remain authority.
            if result.is_ok()
                && matches!(
                    &request,
                    MailboxRequestV1::Put { .. }
                        | MailboxRequestV1::Revoke { .. }
                        | MailboxRequestV1::Register { .. }
                )
            {
                changed.send_modify(|value| *value = value.wrapping_add(1));
            }
            Ok(match result {
                Ok(reply) => reply,
                Err(error) => MailboxReplyV1::Rejected(match error {
                    StoreError::Unauthorized => MailboxRejectionV1::Unauthorized,
                    StoreError::Invalid | StoreError::Random => MailboxRejectionV1::Invalid,
                    StoreError::Quota => MailboxRejectionV1::Quota,
                    StoreError::Conflict => MailboxRejectionV1::Conflict,
                    StoreError::Consumed => MailboxRejectionV1::Consumed,
                    StoreError::Database(_) | StoreError::ClockRollback => {
                        MailboxRejectionV1::Storage
                    }
                }),
            })
        })
        .await?
    }
    async fn watch(&self, boxes: &[MailboxWatchV1], wait_ms: u32) -> Result<MailboxReplyV1> {
        if boxes.is_empty()
            || boxes.len() > 4
            || wait_ms > 10_000
            || boxes
                .iter()
                .enumerate()
                .any(|(i, b)| boxes[..i].iter().any(|x| x.mailbox == b.mailbox))
        {
            return Ok(MailboxReplyV1::Rejected(MailboxRejectionV1::Invalid));
        }
        // Subscribe BEFORE observing durable state. A commit during/between
        // snapshot and await changes this receiver; no Fetch/subscribe gap.
        let mut events = self.changed.subscribe();
        let until = tokio::time::Instant::now() + std::time::Duration::from_millis(wait_ms.into());
        loop {
            let permit = self.slots.clone().acquire_owned().await?;
            let store = self.store.clone();
            let queries = boxes.to_vec();
            let snapshots = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let mut store = store.lock().map_err(|_| StoreError::Invalid)?;
                let now = now().map_err(|_| StoreError::Invalid)?;
                queries
                    .iter()
                    .map(|query| {
                        let (cursor, pending) = store.observe(
                            &query.mailbox,
                            &Capability::from_bytes(query.read)?,
                            now,
                        )?;
                        if query.after.is_some_and(|old| {
                            old.generation == cursor.generation && old.sequence > cursor.sequence
                        }) {
                            return Err(StoreError::Conflict);
                        }
                        Ok(MailboxNoticeV1 { cursor, pending })
                    })
                    .collect::<std::result::Result<Vec<_>, StoreError>>()
            })
            .await?;
            let notices = match snapshots {
                Ok(notices) => notices,
                Err(StoreError::Unauthorized) => {
                    return Ok(MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized))
                }
                Err(StoreError::Conflict) => {
                    return Ok(MailboxReplyV1::Rejected(MailboxRejectionV1::Conflict))
                }
                Err(StoreError::Invalid | StoreError::Random) => {
                    return Ok(MailboxReplyV1::Rejected(MailboxRejectionV1::Invalid))
                }
                Err(_) => return Ok(MailboxReplyV1::Rejected(MailboxRejectionV1::Storage)),
            };
            if tokio::time::Instant::now() >= until
                || notices
                    .iter()
                    .zip(boxes)
                    .any(|(n, b)| n.pending || b.after != Some(n.cursor))
            {
                return Ok(MailboxReplyV1::Notice(notices));
            }
            tokio::select! {
                result=events.changed() => { result?; }
                _=tokio::time::sleep_until(until) => {}
                // Recheck expiry/revocation even without a Put. Never let a
                // subscription turn a read capability into permanent authority.
                _=tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
            }
        }
    }
}
fn now() -> Result<i64> {
    Ok((crate::product_rendezvous_daemon::now_ms()? / 1000).try_into()?)
}

async fn send<T: Serialize>(
    stream: &mut CarrierStreamV1<'_>,
    channel: &mut BoundSecureChannelV1,
    value: &T,
    sent: &mut u64,
) -> Result<()> {
    let bytes = Zeroizing::new(serde_json::to_vec(value)?);
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_WIRE,
        "mailbox wire budget"
    );
    for chunk in bytes.chunks(CHUNK) {
        let mut body = Zeroizing::new(Vec::with_capacity(chunk.len() + 4));
        body.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        body.extend_from_slice(chunk);
        let sequence = sent
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("mailbox sequence exhausted"))?;
        *sent = sequence;
        let mut frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            channel.session_id(),
            STREAM,
            sequence,
            sequence,
            0,
            body.to_vec(),
        );
        let sealed = channel.seal_novorudp_frame(&frame);
        frame.payload.zeroize();
        stream
            .write_frame(&encode_iroh_envelope_v1(&sealed?)?)
            .await?;
    }
    Ok(())
}
async fn receive<T: serde::de::DeserializeOwned>(
    stream: &mut CarrierStreamV1<'_>,
    channel: &mut BoundSecureChannelV1,
    received: &mut u64,
) -> Result<T> {
    let mut body = Zeroizing::new(Vec::new());
    let mut total = 0;
    for chunk in 0..MAX_WIRE.div_ceil(CHUNK) {
        let sequence = received
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("mailbox sequence exhausted"))?;
        let envelope = decode_iroh_envelope_v1(&stream.read_frame().await?)?;
        ensure!(envelope.sequence == sequence, "mailbox sequence");
        let mut frame = channel.open_novorudp_frame(&envelope)?;
        let result = (|| {
            ensure!(
                frame.kind == NovoRudpTransportFrameKindV0::Data
                    && frame.session_id == channel.session_id()
                    && frame.stream_id == STREAM
                    && frame.object_id == sequence
                    && frame.sequence == sequence
                    && frame.ack_epoch == 0,
                "mailbox route"
            );
            ensure!(
                frame.payload.len() > 4 && frame.payload.len() <= CHUNK + 4,
                "mailbox chunk"
            );
            let length = u32::from_be_bytes(frame.payload[..4].try_into()?) as usize;
            ensure!(length > 0 && length <= MAX_WIRE, "mailbox size");
            if chunk == 0 {
                total = length;
            }
            ensure!(
                length == total && frame.payload.len() - 4 == (total - body.len()).min(CHUNK),
                "mailbox framing"
            );
            body.extend_from_slice(&frame.payload[4..]);
            Ok::<_, anyhow::Error>(())
        })();
        frame.payload.zeroize();
        result?;
        *received = sequence;
        if body.len() == total {
            return Ok(serde_json::from_slice(&body)?);
        }
    }
    anyhow::bail!("mailbox frame budget")
}

/// One request on an already authorized service stream. Must be the same stream
/// as its verified signed service offer. Errors drop it, never downgrade/retry.
pub async fn request_mailbox_v1(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    pinned_node: [u8; 32],
    request: MailboxRequestV1,
) -> Result<MailboxReplyV1> {
    let mut channel = authenticate_bound_session(
        stream,
        identity,
        pinned_node,
        true,
        &mut HandshakeReplayCacheV1::new(8),
    )
    .await?;
    send(stream, &mut channel, &request, &mut 0).await?;
    let reply = receive(stream, &mut channel, &mut 0).await?;
    stream.finish().await?;
    Ok(reply)
}
pub async fn serve_mailbox_v1(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    allowed: &[[u8; 32]],
    service: &MailboxServiceV1,
) -> Result<()> {
    ensure!(
        identity.verifying_key() == service.signer.verifying_key(),
        "mailbox service signer mismatch"
    );
    let (mut channel, _) = authenticate_allowed_service_client(
        stream,
        identity,
        allowed,
        &mut HandshakeReplayCacheV1::new(8),
    )
    .await?;
    let request = receive(stream, &mut channel, &mut 0).await?;
    stream.selected_path()?;
    let reply = service.execute(request).await?;
    stream.selected_path()?;
    send(stream, &mut channel, &reply, &mut 0).await?;
    stream.finish().await?;
    Ok(())
}

/// Multiple sequential operations within ONE fresh Finished-bound stream.
/// The caller retains the original scoped authority/deadline. Sequence and
/// AEAD nonce state are continuous; cancellation/error poisons the session.
pub struct MailboxClientSessionV2<'stream, 'scope> {
    stream: &'stream mut CarrierStreamV1<'scope>,
    channel: BoundSecureChannelV1,
    sent: u64,
    received: u64,
    requests: u16,
    poisoned: bool,
}
impl<'stream, 'scope> MailboxClientSessionV2<'stream, 'scope> {
    pub async fn begin(
        stream: &'stream mut CarrierStreamV1<'scope>,
        identity: &SigningKey,
        pinned_node: [u8; 32],
    ) -> Result<Self> {
        let channel = authenticate_bound_session(
            stream,
            identity,
            pinned_node,
            true,
            &mut HandshakeReplayCacheV1::new(8),
        )
        .await?;
        let mut session = Self {
            stream,
            channel,
            sent: 0,
            received: 0,
            requests: 0,
            poisoned: false,
        };
        ensure!(
            matches!(
                session.exchange(MailboxRequestV1::BeginSession).await?,
                MailboxReplyV1::SessionReady
            ),
            "mailbox session negotiation rejected"
        );
        Ok(session)
    }
    pub fn remaining_requests(&self) -> u16 {
        MAX_SESSION_REQUESTS_V2.saturating_sub(self.requests)
    }
    pub async fn request(&mut self, request: MailboxRequestV1) -> Result<MailboxReplyV1> {
        ensure!(
            !matches!(
                &request,
                MailboxRequestV1::BeginSession | MailboxRequestV1::EndSession
            ),
            "mailbox session control reserved"
        );
        ensure!(
            self.requests < MAX_SESSION_REQUESTS_V2,
            "mailbox session request budget"
        );
        self.requests += 1;
        let reply = self.exchange(request).await?;
        if matches!(
            reply,
            MailboxReplyV1::SessionReady | MailboxReplyV1::SessionEnded
        ) {
            self.poisoned = true;
            anyhow::bail!("unexpected mailbox session control");
        }
        Ok(reply)
    }
    async fn exchange(&mut self, request: MailboxRequestV1) -> Result<MailboxReplyV1> {
        ensure!(!self.poisoned, "mailbox session unusable");
        self.poisoned = true; // Also remains true if this future is cancelled.
        self.stream.selected_path()?;
        send(self.stream, &mut self.channel, &request, &mut self.sent).await?;
        let reply = receive(self.stream, &mut self.channel, &mut self.received).await?;
        self.stream.selected_path()?;
        self.poisoned = false;
        Ok(reply)
    }
    pub async fn finish(mut self) -> Result<()> {
        ensure!(
            matches!(
                self.exchange(MailboxRequestV1::EndSession).await?,
                MailboxReplyV1::SessionEnded
            ),
            "mailbox session end rejected"
        );
        self.stream.finish().await
    }
}

/// NVMBOX02 accepts an explicit bounded session or an existing single-request
/// client. No implicit fallback: BeginSession is rejected on NVMBOX01.
/// The host supplies the unchanged signed-offer authority, checked per request.
pub async fn serve_mailbox_session_v2(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    allowed: &[[u8; 32]],
    service: &MailboxServiceV1,
    authority: impl Fn() -> Result<()>,
) -> Result<()> {
    serve_session(stream, identity, allowed, service, authority, None).await
}

/// Explicit NVMBOX03 admission. The host derives this immutable monotonic
/// deadline from its signed offer BEFORE authentication. No lease renewal,
/// request-budget reset or replay-state reset occurs within the session.
pub async fn serve_mailbox_window_v3(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    allowed: &[[u8; 32]],
    service: &MailboxServiceV1,
    authority: impl Fn() -> Result<()>,
    deadline: std::time::Instant,
) -> Result<()> {
    ensure!(
        deadline > std::time::Instant::now(),
        "mailbox window expired"
    );
    tokio::time::timeout_at(
        deadline.into(),
        serve_session(
            stream,
            identity,
            allowed,
            service,
            authority,
            Some(deadline),
        ),
    )
    .await
    .map_err(|_| anyhow::anyhow!("mailbox window expired"))?
}

async fn serve_session(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    allowed: &[[u8; 32]],
    service: &MailboxServiceV1,
    authority: impl Fn() -> Result<()>,
    window: Option<std::time::Instant>,
) -> Result<()> {
    ensure!(
        identity.verifying_key() == service.signer.verifying_key(),
        "mailbox service signer mismatch"
    );
    authority()?;
    let (mut channel, _) = authenticate_allowed_service_client(
        stream,
        identity,
        allowed,
        &mut HandshakeReplayCacheV1::new(8),
    )
    .await?;
    let (mut sent, mut received) = (0, 0);
    let first: MailboxRequestV1 = receive(stream, &mut channel, &mut received).await?;
    authority()?;
    stream.selected_path()?;
    if let Some(deadline) = window {
        ensure!(
            matches!(&first, MailboxRequestV1::BeginSession),
            "mailbox window requires session"
        );
        // Only the successful, identity-bound service handshake can transition
        // the listener's short unauthenticated admission into a service window.
        stream.admit_service_window(deadline)?;
    }
    if !matches!(&first, MailboxRequestV1::BeginSession) {
        let reply = service.execute(first).await?;
        authority()?;
        stream.selected_path()?;
        send(stream, &mut channel, &reply, &mut sent).await?;
    } else {
        send(
            stream,
            &mut channel,
            &MailboxReplyV1::SessionReady,
            &mut sent,
        )
        .await?;
        for index in 0..=MAX_SESSION_REQUESTS_V2 {
            // Each request keeps the old 30s maximum IO budget, including its
            // bounded Watch and durable work. Activity cannot slide the window.
            let done = tokio::time::timeout(std::time::Duration::from_secs(30), async {
                let request: MailboxRequestV1 =
                    receive(stream, &mut channel, &mut received).await?;
                authority()?;
                stream.selected_path()?;
                if matches!(&request, MailboxRequestV1::EndSession) {
                    send(
                        stream,
                        &mut channel,
                        &MailboxReplyV1::SessionEnded,
                        &mut sent,
                    )
                    .await?;
                    return Ok::<_, anyhow::Error>(true);
                }
                ensure!(
                    index < MAX_SESSION_REQUESTS_V2,
                    "mailbox session request budget"
                );
                ensure!(
                    !matches!(&request, MailboxRequestV1::BeginSession),
                    "mailbox session already active"
                );
                let reply = service.execute(request).await?;
                authority()?;
                stream.selected_path()?;
                send(stream, &mut channel, &reply, &mut sent).await?;
                Ok(false)
            })
            .await
            .map_err(|_| anyhow::anyhow!("mailbox request deadline"))??;
            if done {
                break;
            }
        }
    }
    authority()?;
    stream.finish().await
}

#[cfg(test)]
mod notification_tests {
    use super::*;
    #[tokio::test(flavor = "current_thread")]
    async fn maintenance_must_poll_queued_clients_while_waiting_for_store() -> Result<()> {
        // Deterministic reproduction of the old node loop's inline sweep await.
        // Actual service admission and SQLite workers, with a held first permit
        // forcing the otherwise timing-dependent client-before-maintenance order.
        let dir = tempfile::tempdir()?;
        let service = MailboxServiceV1::new(
            MailboxStore::open(dir.path().join("contention.db"), Default::default())?,
            SigningKey::from_bytes(&[2; 32]),
        );
        let held = service.slots.clone().acquire_owned().await?;
        let request = service.execute(MailboxRequestV1::Register {
            mailbox: [3; 32],
            write: [4; 32],
            read: [5; 32],
            expires: now()? + 600,
        });
        tokio::pin!(request);
        assert!(futures_util::poll!(&mut request).is_pending());
        let sweeping = service.sweep();
        tokio::pin!(sweeping);
        assert!(futures_util::poll!(&mut sweeping).is_pending());
        drop(held);
        // Releasing the worker alone cannot resolve an inline sweep await.
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut sweeping)
                .await
                .is_err()
        );
        // The node's existing select loop must keep polling both operations.
        let (reply, swept) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            tokio::join!(&mut request, &mut sweeping)
        })
        .await?;
        assert!(matches!(reply?, MailboxReplyV1::Registered));
        swept?;
        assert_eq!(service.slots.available_permits(), 1);
        // A subsequent request and cleanup still use the same live service.
        assert!(matches!(
            service
                .execute(MailboxRequestV1::Fetch {
                    mailbox: [3; 32],
                    read: [5; 32]
                })
                .await?,
            MailboxReplyV1::Empty
        ));
        service.sweep().await?;
        Ok(())
    }
    #[tokio::test]
    async fn commit_before_and_during_watch_is_caught_and_bad_cursor_or_key_rejected() -> Result<()>
    {
        let dir = tempfile::tempdir()?;
        let mut store = MailboxStore::open(dir.path().join("watch.db"), Default::default())?;
        let now = now()?;
        let grant = store.create(now, 600)?;
        let service = MailboxServiceV1::new(store, SigningKey::from_bytes(&[1; 32]));
        let initial = vec![MailboxWatchV1 {
            mailbox: grant.id,
            read: *grant.read.as_bytes(),
            after: None,
        }];
        let MailboxReplyV1::Notice(first) = service.watch(&initial, 0).await? else {
            panic!("snapshot");
        };
        assert!(!first[0].pending);
        let mut query = initial.clone();
        query[0].after = Some(first[0].cursor);
        let put = |id| MailboxRequestV1::Put {
            mailbox: grant.id,
            write: *grant.write.as_bytes(),
            object: [id; 32],
            ciphertext: vec![id; 8],
            expires: now + 300,
        };
        // Put exactly after Empty, before installing the next watch.
        assert!(matches!(
            service.execute(put(2)).await?,
            MailboxReplyV1::Stored(_)
        ));
        let MailboxReplyV1::Notice(notice) = service.watch(&query, 10_000).await? else {
            panic!("lost gap commit");
        };
        assert_eq!(notice[0].cursor.sequence, 1);
        assert!(notice[0].pending);
        service
            .execute(MailboxRequestV1::Consume {
                mailbox: grant.id,
                read: *grant.read.as_bytes(),
                object: [2; 32],
                digest: sha2::Sha256::digest([2; 8]).into(),
            })
            .await?;
        query[0].after = Some(notice[0].cursor);
        let start = std::time::Instant::now();
        let waiter = service.watch(&query, 10_000);
        let writer = async {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            service.execute(put(3)).await
        };
        let (notice, stored) = tokio::join!(waiter, writer);
        stored?;
        let MailboxReplyV1::Notice(notice) = notice? else {
            panic!("no live notification");
        };
        assert_eq!(notice[0].cursor.sequence, 2);
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
        query[0].after = Some(MailboxCursorV1 {
            sequence: 3,
            ..notice[0].cursor
        });
        assert!(matches!(
            service.watch(&query, 0).await?,
            MailboxReplyV1::Rejected(MailboxRejectionV1::Conflict)
        ));
        query[0].read = [9; 32];
        assert!(matches!(
            service.watch(&query, 0).await?,
            MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
        ));
        Ok(())
    }
    #[tokio::test]
    async fn idle_watch_obeys_revocation_and_cancellation_releases_store() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut store = MailboxStore::open(dir.path().join("watch.db"), Default::default())?;
        let grant = store.create(now()?, 600)?;
        let (cursor, _) = store.observe(&grant.id, &grant.read, now()?)?;
        let service = MailboxServiceV1::new(store, SigningKey::from_bytes(&[1; 32]));
        let query = vec![MailboxWatchV1 {
            mailbox: grant.id,
            read: *grant.read.as_bytes(),
            after: Some(cursor),
        }];
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(30),
            service.watch(&query, 10_000)
        )
        .await
        .is_err());
        let waiter = service.watch(&query, 10_000);
        let revoker = async {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            service
                .execute(MailboxRequestV1::Revoke {
                    mailbox: grant.id,
                    read: *grant.read.as_bytes(),
                })
                .await
        };
        let (reply, revoked) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            tokio::join!(waiter, revoker)
        })
        .await?;
        assert!(matches!(revoked?, MailboxReplyV1::Revoked));
        assert!(matches!(
            reply?,
            MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
        ));
        Ok(())
    }
    use sha2::Digest;
}

#[cfg(test)]
mod session_failure_tests {
    use super::*;
    use crate::iroh_transport::{with_iroh_scope_v1, IrohConfigV1, IrohControlV1};
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::{Duration, Instant},
    };
    fn key(n: u8) -> SigningKey {
        SigningKey::from_bytes(&[n; 32])
    }
    fn config() -> IrohConfigV1 {
        IrohConfigV1::new(&[], "127.0.0.1".parse().unwrap())
            .unwrap()
            .with_bind_address("127.0.0.1:0".parse().unwrap())
            .unwrap()
    }
    fn control() -> IrohControlV1 {
        IrohControlV1::new(Instant::now() + Duration::from_secs(10), || Ok(())).unwrap()
    }
    #[tokio::test]
    async fn cancelled_wait_or_revoked_authority_cannot_reuse_session() -> Result<()> {
        for cancel in [true, false] {
            let dir = tempfile::tempdir()?;
            let service = MailboxServiceV1::new(
                MailboxStore::open(dir.path().join("session.db"), Default::default())?,
                key(1),
            );
            let grant = MailboxGrant::generate()?;
            service
                .execute(MailboxRequestV1::Register {
                    mailbox: grant.id,
                    read: *grant.read.as_bytes(),
                    write: *grant.write.as_bytes(),
                    expires: now()? + 300,
                })
                .await?;
            let cursor = match service
                .execute(MailboxRequestV1::Watch {
                    boxes: vec![MailboxWatchV1 {
                        mailbox: grant.id,
                        read: *grant.read.as_bytes(),
                        after: None,
                    }],
                    wait_ms: 0,
                })
                .await?
            {
                MailboxReplyV1::Notice(n) => n[0].cursor,
                _ => anyhow::bail!("fixture notice"),
            };
            let active = Arc::new(AtomicBool::new(true));
            let host = active.clone();
            let (tx, rx) = tokio::sync::oneshot::channel();
            let server = with_iroh_scope_v1(config(), control(), move |scope| {
                Box::pin(async move {
                    tx.send(scope.address()?)
                        .map_err(|_| anyhow::anyhow!("fixture address"))?;
                    let mut stream = scope.accept_inbound().await?.into();
                    let result = serve_mailbox_session_v2(
                        &mut stream,
                        &key(1),
                        &[key(2).verifying_key().to_bytes()],
                        &service,
                        || {
                            ensure!(host.load(Ordering::Acquire), "fixture revoked");
                            Ok(())
                        },
                    )
                    .await;
                    drop(stream);
                    Ok::<_, anyhow::Error>(result)
                })
            });
            let client = with_iroh_scope_v1(config(), control(), move |scope| {
                Box::pin(async move {
                    let mut stream = scope.connect(&rx.await?).await?.into();
                    let mut session = MailboxClientSessionV2::begin(
                        &mut stream,
                        &key(2),
                        key(1).verifying_key().to_bytes(),
                    )
                    .await?;
                    if cancel {
                        assert!(tokio::time::timeout(
                            Duration::from_millis(80),
                            session.request(MailboxRequestV1::Watch {
                                boxes: vec![MailboxWatchV1 {
                                    mailbox: grant.id,
                                    read: *grant.read.as_bytes(),
                                    after: Some(cursor)
                                }],
                                wait_ms: 500,
                            })
                        )
                        .await
                        .is_err());
                    } else {
                        active.store(false, Ordering::Release);
                        assert!(session
                            .request(MailboxRequestV1::Fetch {
                                mailbox: grant.id,
                                read: *grant.read.as_bytes()
                            })
                            .await
                            .is_err());
                    }
                    let error = session
                        .request(MailboxRequestV1::Fetch {
                            mailbox: grant.id,
                            read: *grant.read.as_bytes(),
                        })
                        .await
                        .err()
                        .unwrap();
                    ensure!(error.to_string() == "mailbox session unusable");
                    assert!(session.finish().await.is_err());
                    Ok::<_, anyhow::Error>(())
                })
            });
            let (server, client) = tokio::join!(server, client);
            client?;
            assert!(server?.is_err());
        }
        Ok(())
    }
    #[tokio::test]
    async fn repeated_envelope_after_negotiation_is_rejected_before_request() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let service = MailboxServiceV1::new(
            MailboxStore::open(dir.path().join("session.db"), Default::default())?,
            key(1),
        );
        let (tx, rx) = tokio::sync::oneshot::channel();
        let server = with_iroh_scope_v1(config(), control(), move |scope| {
            Box::pin(async move {
                tx.send(scope.address()?)
                    .map_err(|_| anyhow::anyhow!("fixture address"))?;
                let mut stream = scope.accept_inbound().await?.into();
                let result = serve_mailbox_session_v2(
                    &mut stream,
                    &key(1),
                    &[key(2).verifying_key().to_bytes()],
                    &service,
                    || Ok(()),
                )
                .await;
                drop(stream);
                Ok::<_, anyhow::Error>(result)
            })
        });
        let client = with_iroh_scope_v1(config(), control(), move |scope| {
            Box::pin(async move {
                let mut stream: CarrierStreamV1<'_> = scope.connect(&rx.await?).await?.into();
                let mut channel = authenticate_bound_session(
                    &mut stream,
                    &key(2),
                    key(1).verifying_key().to_bytes(),
                    true,
                    &mut HandshakeReplayCacheV1::new(8),
                )
                .await?;
                let bytes = serde_json::to_vec(&MailboxRequestV1::BeginSession)?;
                let body = [(bytes.len() as u32).to_be_bytes().as_slice(), &bytes].concat();
                let frame = NovoRudpTransportFrameV0::new(
                    NovoRudpTransportFrameKindV0::Data,
                    channel.session_id(),
                    STREAM,
                    1,
                    1,
                    0,
                    body,
                );
                let encoded = encode_iroh_envelope_v1(&channel.seal_novorudp_frame(&frame)?)?;
                stream.write_frame(&encoded).await?;
                ensure!(matches!(
                    receive::<MailboxReplyV1>(&mut stream, &mut channel, &mut 0).await?,
                    MailboxReplyV1::SessionReady
                ));
                stream.write_frame(&encoded).await?;
                assert!(stream.read_frame().await.is_err());
                Ok::<_, anyhow::Error>(())
            })
        });
        let (server, client) = tokio::join!(server, client);
        client?;
        ensure!(server?
            .unwrap_err()
            .to_string()
            .contains("mailbox sequence"));
        Ok(())
    }
}
