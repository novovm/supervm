//! One L3/L4 protocol over the existing Finished-bound carrier. No chat IDs.
//! Owner admits the service path; credentials are never sent before peer authentication.
use crate::{
    bound_session::{
        authenticate_allowed_service_client, authenticate_bound_session,
        authenticate_service_client_observed, ServicePeerPolicyV1,
    },
    carrier_stream::CarrierStreamV1,
    duplex::{HandshakeReplayCacheV1, NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0},
    iroh_transport::{decode_iroh_envelope_v1, encode_iroh_envelope_v1},
    mailbox_receipt::StoreReceiptV1,
    mailbox_store::{
        Capability, ContributionStatusV1, MailboxCursorV1, MailboxGrant, MailboxStore, StoreError,
    },
    product_directory::{validate_relay_record_v1, PeerSignedRelayRecordV1},
    product_service_lease::{NativeCarrierMailboxEndpointV7, NativeMailboxEndpointV7},
    transport_binding::BoundSecureChannelV1,
};
use anyhow::{ensure, Result};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use zeroize::{Zeroize, Zeroizing};

#[path = "mailbox_rpc_metrics.rs"]
mod metrics;
pub use metrics::MailboxTimingsV1;

pub const MAX_SESSION_REQUESTS_V2: u16 = 64;

#[cfg(test)]
#[path = "mailbox_consumed_evidence_tests.rs"]
mod consumed_evidence_tests;
#[cfg(test)]
#[path = "mailbox_local_tests.rs"]
mod local_tests;

const STREAM: u64 = 0x4e564d424f5831;
const CHUNK: usize = 8192;
const MAX_WIRE: usize = 300_000; // 64 KiB ciphertext in bounded JSON byte-array representation.

// No Debug. These capability-bearing values must never enter logs/status.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MailboxRequestV1 {
    /// NVMBOX07 APFL only; has no legacy JSON wire representation.
    #[serde(skip)]
    ServiceRecordV7,
    /// Explicit opt-in on NVMBOX02; never a downgrade on an older service.
    BeginSession,
    /// Explicit, Finished-protected NVMBOX04 negotiation. Legacy rejects it.
    BeginContinuousV4,
    BeginContinuousV6,
    EndSession,
    /// Bounded notification wait; each position is private to an authorized box.
    /// The caller snapshots before Fetch, then watches from that position.
    Watch {
        boxes: Vec<MailboxWatchV1>,
        wait_ms: u32,
    },
    /// V6 only: return at most one retained ciphertext with the notification.
    WatchFetchV6 {
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
            Self::ServiceRecordV7
            | Self::BeginSession
            | Self::BeginContinuousV4
            | Self::BeginContinuousV6
            | Self::EndSession
            | Self::Watch { .. }
            | Self::WatchFetchV6 { .. } => {} // each item zeroizes its own read capability
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
/// Encrypted object only. Query index binds it to an already validated read capability.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailboxPrefetchV6 {
    pub box_index: usize,
    pub object: [u8; 32],
    pub ciphertext: Vec<u8>,
    pub digest: [u8; 32],
    pub expires: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MailboxReplyV1 {
    /// Original signed directory value, never re-signed by the RPC adapter.
    #[serde(skip)]
    ServiceRecordV7 {
        record: PeerSignedRelayRecordV1,
    },
    SessionReady,
    ContinuousReadyV4,
    ContinuousReadyV6,
    SessionEnded,
    Notice(Vec<MailboxNoticeV1>),
    NoticeObjectV6 {
        notices: Vec<MailboxNoticeV1>,
        object: Option<MailboxPrefetchV6>,
    },
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
    // Shared across V4 connections, so reconnecting cannot reset the budget.
    request_budget: Mutex<RequestBudgetV4>,
    timings: metrics::Timings,
    native_record_v7: Mutex<Option<PeerSignedRelayRecordV1>>,
}

struct RequestBudgetV4 {
    tokens: u32,
    updated: std::time::Instant,
    burst: u16,
    interval: std::time::Duration,
}
impl RequestBudgetV4 {
    fn take(&mut self) -> Option<std::time::Duration> {
        use std::time::Duration;
        let elapsed = self.updated.elapsed();
        let refill = elapsed.as_nanos() / self.interval.as_nanos();
        if refill > 0 {
            self.tokens = (u128::from(self.tokens) + refill).min(u128::from(self.burst)) as u32;
            let remainder = elapsed.as_nanos() % self.interval.as_nanos();
            self.updated += elapsed - Duration::from_nanos(remainder as u64);
        }
        if self.tokens > 0 {
            self.tokens -= 1;
            None
        } else {
            Some(self.interval.saturating_sub(self.updated.elapsed()))
        }
    }
}
impl MailboxServiceV1 {
    pub(crate) fn identity_public_key(&self) -> [u8; 32] {
        self.signer.verifying_key().to_bytes()
    }
    pub fn new(store: MailboxStore, signer: SigningKey) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            signer,
            slots: Arc::new(tokio::sync::Semaphore::new(1)),
            changed: tokio::sync::watch::channel(0).0,
            timings: metrics::Timings::default(),
            native_record_v7: Mutex::new(None),
            request_budget: Mutex::new(RequestBudgetV4 {
                tokens: 64,
                updated: std::time::Instant::now(),
                burst: 64,
                interval: std::time::Duration::from_millis(500),
            }),
        }
    }

    /// Local owner publication only. The directory journal remains authoritative;
    /// this is a single current snapshot for authenticated native clients.
    pub fn update_native_record_v7(&self, record: PeerSignedRelayRecordV1) -> Result<()> {
        self.update_native_record(record, false)
    }

    /// Same signed locator and durable owner; V8 is an explicit carrier
    /// capability, not permission to reinterpret an expired V7 credential.
    pub fn update_native_record_v8(&self, record: PeerSignedRelayRecordV1) -> Result<()> {
        self.update_native_record(record, true)
    }

    fn update_native_record(&self, record: PeerSignedRelayRecordV1, v8: bool) -> Result<()> {
        crate::mailbox_structured::locator::validate(&record)?;
        ensure!(
            record.relay_public_key == self.identity_public_key(),
            "mailbox locator node mismatch"
        );
        validate_relay_record_v1(&record, crate::product_rendezvous_daemon::now_ms()?)?;
        // Parse the complete canonical service inventory before checking either
        // native capability. A UDP entry cannot hide a malformed carrier/TLS
        // alternative, and a legacy Iroh address never implies NVMBOX07.
        let carrier = if v8 {
            crate::product_service_lease::NativeCarrierMailboxEndpointV8::from_record(&record)?
                .is_some()
        } else {
            NativeCarrierMailboxEndpointV7::from_record(&record)?.is_some()
        };
        ensure!(
            carrier || (!v8 && NativeMailboxEndpointV7::from_record(&record)?.is_some()),
            "mailbox native locator missing"
        );
        let mut current = self
            .native_record_v7
            .lock()
            .map_err(|_| anyhow::anyhow!("mailbox locator unavailable"))?;
        if let Some(previous) = current.as_ref() {
            ensure!(
                record.sequence >= previous.sequence
                    && record.issued_at_ms >= previous.issued_at_ms,
                "mailbox locator rollback"
            );
            ensure!(
                record.sequence != previous.sequence || record == *previous,
                "mailbox locator sequence conflict"
            );
        }
        *current = Some(record);
        Ok(())
    }

    /// Local provider configuration, chosen before sharing this service owner.
    /// It cannot be changed by RPC or reset by reconnecting. Storage/byte/frame
    /// limits remain independent; these are request units, not NOV rewards.
    pub fn with_continuous_request_rate_v4(mut self, burst: u16, per_second: u16) -> Result<Self> {
        ensure!(
            (1..=256).contains(&burst) && (1..=128).contains(&per_second),
            "mailbox local rate bounds"
        );
        *self
            .request_budget
            .get_mut()
            .map_err(|_| anyhow::anyhow!("mailbox request budget unavailable"))? =
            RequestBudgetV4 {
                tokens: u32::from(burst),
                burst,
                interval: std::time::Duration::from_nanos(
                    1_000_000_000u64.div_ceil(u64::from(per_second)),
                ),
                updated: std::time::Instant::now(),
            };
        Ok(self)
    }

    async fn admit_request_v4(&self) -> Result<()> {
        let sample = self.timings.admission.start();
        loop {
            let wait = self
                .request_budget
                .lock()
                .map_err(|_| anyhow::anyhow!("mailbox request budget unavailable"))?
                .take();
            match wait {
                None => {
                    sample.complete(false);
                    return Ok(());
                }
                Some(wait) => tokio::time::sleep(wait).await,
            }
        }
    }

    /// Local-only diagnostics, not a remote RPC or a service receipt. No
    /// identifiers or request values are retained; snapshots never grant work.
    pub fn timing_snapshot_v1(&self) -> MailboxTimingsV1 {
        self.timings.snapshot()
    }

    /// In-process capability client of the SAME mailbox owner. A phone may
    /// receive its own stored ciphertext without dialing itself through a relay.
    /// This is not a privileged database reader: capabilities, quotas, TTL,
    /// revocation, durable notifications and the node-wide request budget are
    /// identical to remote operations. The embedding owner must check its live
    /// identity/contact policy before and after awaiting this method.
    /// Transport negotiation belongs to authenticated network streams only.
    pub async fn request_local_v1(&self, request: MailboxRequestV1) -> Result<MailboxReplyV1> {
        if matches!(
            &request,
            MailboxRequestV1::BeginSession
                | MailboxRequestV1::BeginContinuousV4
                | MailboxRequestV1::BeginContinuousV6
                | MailboxRequestV1::EndSession
        ) {
            return Ok(MailboxReplyV1::Rejected(MailboxRejectionV1::Invalid));
        }
        self.admit_request_v4().await?;
        self.execute(request).await
    }

    /// Local platform control, not remotely callable. Uses the same fair store
    /// queue as RPC commits: once this completes, later new obligations are
    /// refused. Prior committed objects/read capabilities remain intact.
    pub async fn set_contribution_enabled(&self, enabled: bool) -> Result<()> {
        self.owner_update(move |store| Ok(store.set_contribution_enabled(enabled, now()?)?))
            .await
    }

    /// Local foreground/power/network/thermal decision. This controls NEW
    /// storage obligations only; the existing transport authority separately
    /// controls whether any service may execute. Never accepts peer claims.
    pub async fn set_resources_available(&self, available: bool) -> Result<()> {
        self.owner_update(move |store| {
            store.set_resources_available(available);
            Ok(())
        })
        .await
    }

    pub async fn contribution_status(&self) -> Result<ContributionStatusV1> {
        self.owner_update(|store| Ok(store.contribution_status(now()?)?))
            .await
    }

    async fn owner_update<T: Send + 'static>(
        &self,
        update: impl FnOnce(&mut MailboxStore) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let permit = self.slots.clone().acquire_owned().await?;
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut store = store
                .lock()
                .map_err(|_| anyhow::anyhow!("mailbox store unavailable"))?;
            update(&mut store)
        })
        .await?
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
        let counter = match &request {
            MailboxRequestV1::Put { .. } => &self.timings.put,
            MailboxRequestV1::Fetch { .. } => &self.timings.fetch,
            MailboxRequestV1::Consume { .. } => &self.timings.consume,
            MailboxRequestV1::Watch { .. } | MailboxRequestV1::WatchFetchV6 { .. } => {
                &self.timings.watch
            }
            _ => &self.timings.other,
        };
        let sample = counter.start();
        let reply = self.execute_inner(request).await?;
        sample.complete(matches!(&reply, MailboxReplyV1::Rejected(_)));
        Ok(reply)
    }

    async fn execute_inner(&self, request: MailboxRequestV1) -> Result<MailboxReplyV1> {
        if matches!(&request, MailboxRequestV1::ServiceRecordV7) {
            let current = self
                .native_record_v7
                .lock()
                .map_err(|_| anyhow::anyhow!("mailbox locator unavailable"))?;
            let now = crate::product_rendezvous_daemon::now_ms()?;
            return Ok(match current.as_ref() {
                Some(record) if record.issued_at_ms <= now && now < record.expires_at_ms => {
                    MailboxReplyV1::ServiceRecordV7 {
                        record: record.clone(),
                    }
                }
                _ => MailboxReplyV1::Rejected(MailboxRejectionV1::Invalid),
            });
        }
        if let MailboxRequestV1::Watch { boxes, wait_ms } = &request {
            return self.watch(boxes, *wait_ms).await;
        }
        if let MailboxRequestV1::WatchFetchV6 { boxes, wait_ms } = &request {
            return self.watch_inner(boxes, *wait_ms, true).await;
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
                    MailboxRequestV1::ServiceRecordV7
                    | MailboxRequestV1::BeginSession
                    | MailboxRequestV1::BeginContinuousV4
                    | MailboxRequestV1::BeginContinuousV6
                    | MailboxRequestV1::EndSession => Err(StoreError::Invalid),
                    MailboxRequestV1::Watch { .. } | MailboxRequestV1::WatchFetchV6 { .. } => {
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
                        let capability = Capability::from_bytes(*write)?;
                        // A retried Put after deletion is new real custody,
                        // subject to the same resource bounds and original TTL.
                        // Never sign an unverifiable historical storage claim.
                        store.put(mailbox, &capability, object, ciphertext, *expires, now)?;
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
        self.watch_inner(boxes, wait_ms, false).await
    }
    async fn watch_inner(
        &self,
        boxes: &[MailboxWatchV1],
        wait_ms: u32,
        prefetch: bool,
    ) -> Result<MailboxReplyV1> {
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
                let notices = queries
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
                    .collect::<std::result::Result<Vec<_>, StoreError>>()?;
                // Validate ALL requested capabilities/cursors before fetching anything.
                // Same exclusive owner; no writer can slip between observation and fetch.
                let mut object = None;
                if prefetch {
                    if let Some(index) = notices.iter().position(|n| n.pending) {
                        let q = &queries[index];
                        if let Some(row) =
                            store.fetch_one(&q.mailbox, &Capability::from_bytes(q.read)?, now)?
                        {
                            object = Some(MailboxPrefetchV6 {
                                box_index: index,
                                object: row.id,
                                ciphertext: row.ciphertext,
                                digest: row.digest,
                                expires: row.expires,
                            });
                        }
                    }
                }
                Ok::<_, StoreError>((notices, object))
            })
            .await?;
            let (notices, object) = match snapshots {
                Ok(snapshot) => snapshot,
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
                return Ok(if prefetch {
                    MailboxReplyV1::NoticeObjectV6 { notices, object }
                } else {
                    MailboxReplyV1::Notice(notices)
                });
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
    receive_with_first(stream, channel, received, None).await
}
async fn receive_with_first<T: serde::de::DeserializeOwned>(
    stream: &mut CarrierStreamV1<'_>,
    channel: &mut BoundSecureChannelV1,
    received: &mut u64,
    mut first: Option<Vec<u8>>,
) -> Result<T> {
    let mut body = Zeroizing::new(Vec::new());
    let mut total = 0;
    for chunk in 0..MAX_WIRE.div_ceil(CHUNK) {
        let sequence = received
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("mailbox sequence exhausted"))?;
        let bytes = match first.take() {
            Some(bytes) => bytes,
            None => stream.read_frame().await?,
        };
        let envelope = decode_iroh_envelope_v1(&bytes)?;
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
    ensure!(
        !matches!(
            &request,
            MailboxRequestV1::WatchFetchV6 { .. } | MailboxRequestV1::BeginContinuousV6
        ),
        "mailbox V6 not negotiated"
    );
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
    continuous: bool,
    prefetch: bool,
}
impl<'stream, 'scope> MailboxClientSessionV2<'stream, 'scope> {
    pub async fn begin(
        stream: &'stream mut CarrierStreamV1<'scope>,
        identity: &SigningKey,
        pinned_node: [u8; 32],
    ) -> Result<Self> {
        Self::begin_inner(stream, identity, pinned_node, false, false).await
    }

    /// Requires a V4 selector and an owner-controlled continuous carrier scope.
    /// Local peer trust/revocation and privacy remain the owner's live checks.
    pub async fn begin_continuous_v4(
        stream: &'stream mut CarrierStreamV1<'scope>,
        identity: &SigningKey,
        pinned_node: [u8; 32],
    ) -> Result<Self> {
        Self::begin_inner(stream, identity, pinned_node, true, false).await
    }

    /// Explicit encrypted V6 negotiation on NVMBOX06. No old-version fallback.
    pub async fn begin_continuous_v6(
        stream: &'stream mut CarrierStreamV1<'scope>,
        identity: &SigningKey,
        pinned_node: [u8; 32],
    ) -> Result<Self> {
        Self::begin_inner(stream, identity, pinned_node, true, true).await
    }
    async fn begin_inner(
        stream: &'stream mut CarrierStreamV1<'scope>,
        identity: &SigningKey,
        pinned_node: [u8; 32],
        continuous: bool,
        prefetch: bool,
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
            continuous: false,
            prefetch,
        };
        let reply = session
            .exchange(if prefetch {
                MailboxRequestV1::BeginContinuousV6
            } else if continuous {
                MailboxRequestV1::BeginContinuousV4
            } else {
                MailboxRequestV1::BeginSession
            })
            .await?;
        ensure!(
            matches!(
                (&reply, continuous, prefetch),
                (MailboxReplyV1::SessionReady, false, false)
                    | (MailboxReplyV1::ContinuousReadyV4, true, false)
                    | (MailboxReplyV1::ContinuousReadyV6, true, true)
            ),
            "mailbox session negotiation rejected"
        );
        if continuous {
            session.stream.admit_continuous_service_v4()?;
            session.continuous = true;
            if prefetch {
                session.stream.complete_service_request_v6()?;
            }
        }
        Ok(session)
    }
    pub fn remaining_requests(&self) -> u16 {
        // Continuous V4 uses the node-wide rate budget, not a lifetime cap.
        // Kept for existing callers; they must opt into V4 explicitly.
        if self.continuous {
            return u16::MAX;
        }
        MAX_SESSION_REQUESTS_V2.saturating_sub(self.requests)
    }
    /// Passive observation of this session's current carrier, not its initial
    /// admission path. No network probe, identity, liveness or delivery proof.
    /// RTT is QUIC's smoothed estimate, not mailbox request completion time.
    /// Unsupported/missing RTT remains None; it must not rank as zero latency.
    pub fn carrier_observation_v1(&self) -> Result<(&'static str, Option<std::time::Duration>)> {
        ensure!(!self.poisoned, "mailbox session unusable");
        let path = self.stream.selected_path()?;
        #[cfg(feature = "iroh-transport")]
        if let Some((selected, rtt)) = self.stream.selected_rtt() {
            // Read kind and RTT together: migration can occur after the live
            // authority check above. Never pair a new RTT with the old kind.
            return Ok((selected, Some(rtt)));
        }
        Ok((path, None))
    }
    pub async fn request(&mut self, request: MailboxRequestV1) -> Result<MailboxReplyV1> {
        ensure!(
            self.prefetch || !matches!(&request, MailboxRequestV1::WatchFetchV6 { .. }),
            "mailbox V6 not negotiated"
        );
        ensure!(
            !matches!(
                &request,
                MailboxRequestV1::BeginSession
                    | MailboxRequestV1::BeginContinuousV4
                    | MailboxRequestV1::BeginContinuousV6
                    | MailboxRequestV1::EndSession
            ),
            "mailbox session control reserved"
        );
        ensure!(
            self.continuous || self.requests < MAX_SESSION_REQUESTS_V2,
            "mailbox session request budget"
        );
        if !self.continuous {
            self.requests += 1;
        }
        let reply = self.exchange(request).await?;
        if matches!(
            reply,
            MailboxReplyV1::SessionReady
                | MailboxReplyV1::ContinuousReadyV4
                | MailboxReplyV1::ContinuousReadyV6
                | MailboxReplyV1::SessionEnded
        ) {
            self.poisoned = true;
            anyhow::bail!("unexpected mailbox session control");
        }
        Ok(reply)
    }
    async fn exchange(&mut self, request: MailboxRequestV1) -> Result<MailboxReplyV1> {
        ensure!(!self.poisoned, "mailbox session unusable");
        self.poisoned = true; // Also remains true if this future is cancelled.
        if self.continuous {
            self.stream.begin_service_request_v4()?;
        }
        self.stream.selected_path()?;
        let ending = matches!(&request, MailboxRequestV1::EndSession);
        send(self.stream, &mut self.channel, &request, &mut self.sent).await?;
        let reply = receive(self.stream, &mut self.channel, &mut self.received).await?;
        self.stream.selected_path()?;
        if self.continuous && self.prefetch && !ending {
            self.stream.complete_service_request_v6()?;
        }
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
    serve_session(
        stream,
        identity,
        ServicePeerPolicyV1::Pinned(allowed),
        service,
        authority,
        SessionMode::Legacy,
    )
    .await
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
            ServicePeerPolicyV1::Pinned(allowed),
            service,
            authority,
            SessionMode::Window(deadline),
        ),
    )
    .await
    .map_err(|_| anyhow::anyhow!("mailbox window expired"))?
}

/// Node-owned resource service, independent of rendezvous declaration epochs.
/// All waits are supervised for revocation, including storage and idle reads.
/// The same identity handshake, operations, receipts and storage are reused.
pub async fn serve_mailbox_continuous_v4(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    allowed: &[[u8; 32]],
    service: &MailboxServiceV1,
    authority: impl Fn() -> Result<()>,
) -> Result<()> {
    serve_mailbox_continuous_with_policy_v4(
        stream,
        identity,
        ServicePeerPolicyV1::Pinned(allowed),
        service,
        authority,
    )
    .await
}

/// The same NVMBOX04 handshake/session with explicit provider admission policy.
/// Open admission does not bypass the store's capabilities or global budgets.
pub async fn serve_mailbox_continuous_with_policy_v4(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    policy: ServicePeerPolicyV1<'_>,
    service: &MailboxServiceV1,
    authority: impl Fn() -> Result<()>,
) -> Result<()> {
    serve_continuous(
        stream,
        identity,
        policy,
        service,
        authority,
        SessionMode::Continuous,
    )
    .await
}

/// One continuous NVMBOX06 lane on an owner-controlled carrier, including TLS.
/// The caller must select V6 explicitly. This reuses the same bound handshake,
/// capabilities, durable cursor and request budget as multiplex V6; it does
/// not turn TLS into a multiplexed connection or grant another lane's identity.
pub async fn serve_mailbox_continuous_with_policy_v6(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    policy: ServicePeerPolicyV1<'_>,
    service: &MailboxServiceV1,
    authority: impl Fn() -> Result<()>,
) -> Result<()> {
    serve_continuous(
        stream,
        identity,
        policy,
        service,
        authority,
        SessionMode::Prefetch,
    )
    .await
}

async fn serve_continuous(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    policy: ServicePeerPolicyV1<'_>,
    service: &MailboxServiceV1,
    authority: impl Fn() -> Result<()>,
    mode: SessionMode,
) -> Result<()> {
    let revoked = async {
        loop {
            if let Err(error) = authority() {
                return error;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    };
    tokio::select! {
        biased;
        error = revoked => Err(error),
        result = serve_session(stream, identity, policy, service, &authority, mode) => result,
    }
}

enum SessionMode {
    Legacy,
    Window(std::time::Instant),
    Continuous,
    Prefetch,
}

/// One admitted connection, bounded independent streams. Each lane still
/// authenticates its own allowed client, Finished and AEAD sequence. The same
/// service owns storage/rate budgets across ALL connections and lanes.
pub async fn serve_mailbox_multiplex_v5(
    first: CarrierStreamV1<'_>,
    identity: &SigningKey,
    allowed: &[[u8; 32]],
    service: &MailboxServiceV1,
    authority: impl Fn() -> Result<()>,
) -> Result<()> {
    serve_mailbox_multiplex_with_policy_v5(
        first,
        identity,
        ServicePeerPolicyV1::Pinned(allowed),
        service,
        authority,
    )
    .await
}

/// Original multiplex protocol; each lane proves its own key possession once.
/// All lanes and identities share the SAME node resource accounting.
pub async fn serve_mailbox_multiplex_with_policy_v5(
    first: CarrierStreamV1<'_>,
    identity: &SigningKey,
    policy: ServicePeerPolicyV1<'_>,
    service: &MailboxServiceV1,
    authority: impl Fn() -> Result<()>,
) -> Result<()> {
    serve_multiplex(first, identity, policy, service, authority, false).await
}
pub async fn serve_mailbox_multiplex_with_policy_v6(
    first: CarrierStreamV1<'_>,
    identity: &SigningKey,
    policy: ServicePeerPolicyV1<'_>,
    service: &MailboxServiceV1,
    authority: impl Fn() -> Result<()>,
) -> Result<()> {
    serve_multiplex(first, identity, policy, service, authority, true).await
}

/// Dispatch an already selected NVMBOX06 service using the actual carrier.
/// TLS is one stream per connection; QUIC retains the existing bounded lanes.
/// Both use the same executor, store and node-wide accounting. Selecting a
/// carrier never selects a weaker peer policy or bypasses V6 authentication.
pub async fn serve_mailbox_prefetch_with_policy_v6(
    mut first: CarrierStreamV1<'_>,
    identity: &SigningKey,
    policy: ServicePeerPolicyV1<'_>,
    service: &MailboxServiceV1,
    authority: impl Fn() -> Result<()>,
) -> Result<()> {
    if first.supports_multiplex_v1() {
        serve_mailbox_multiplex_with_policy_v6(first, identity, policy, service, authority).await
    } else {
        serve_mailbox_continuous_with_policy_v6(&mut first, identity, policy, service, authority)
            .await
    }
}
async fn serve_multiplex(
    mut first: CarrierStreamV1<'_>,
    identity: &SigningKey,
    policy: ServicePeerPolicyV1<'_>,
    service: &MailboxServiceV1,
    authority: impl Fn() -> Result<()>,
    prefetch: bool,
) -> Result<()> {
    use futures_util::{stream::FuturesUnordered, StreamExt};
    use std::time::Duration;
    authority()?;
    let connection = first.multiplex_v1()?;
    let lane = |mut stream, selected: bool| {
        let authority = &authority;
        async move {
            authority()?;
            if !selected {
                tokio::time::timeout(
                    Duration::from_secs(3),
                    crate::product_service_lease::accept_mailbox_multiplex_lane(
                        &mut stream,
                        prefetch,
                    ),
                )
                .await??;
            }
            serve_session(
                &mut stream,
                identity,
                policy,
                service,
                authority,
                if prefetch {
                    SessionMode::Prefetch
                } else {
                    SessionMode::Continuous
                },
            )
            .await
        }
    };
    let mut active = FuturesUnordered::new();
    active.push(lane(first, true));
    let mut incoming = Box::pin(connection.accept());
    let mut check = tokio::time::interval(Duration::from_millis(20));
    check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            _ = check.tick() => authority()?,
            Some(result) = active.next() => {
                result?; // Security/IO failures remain visible; never silently retry.
                if active.is_empty() { return Ok(()); }
            }
            result = &mut incoming,
                if active.len() < crate::iroh_transport::IROH_MAX_MULTIPLEX_STREAMS_V1 => {
                let stream = result?;
                incoming = Box::pin(connection.accept());
                active.push(lane(stream, false));
            }
        }
    }
}

async fn serve_session(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    policy: ServicePeerPolicyV1<'_>,
    service: &MailboxServiceV1,
    authority: impl Fn() -> Result<()>,
    mode: SessionMode,
) -> Result<()> {
    ensure!(
        identity.verifying_key() == service.signer.verifying_key(),
        "mailbox service signer mismatch"
    );
    authority()?;
    let (mut channel, _) = authenticate_service_client_observed(
        stream,
        identity,
        policy,
        &mut HandshakeReplayCacheV1::new(8),
        &mut |_, _| Ok(()),
    )
    .await?;
    let (mut sent, mut received) = (0, 0);
    let first: MailboxRequestV1 = receive(stream, &mut channel, &mut received).await?;
    authority()?;
    stream.selected_path()?;
    if let SessionMode::Window(deadline) = mode {
        ensure!(
            matches!(&first, MailboxRequestV1::BeginSession),
            "mailbox window requires session"
        );
        // Only the successful, identity-bound service handshake can transition
        // the listener's short unauthenticated admission into a service window.
        stream.admit_service_window(deadline)?;
    }
    let prefetch = matches!(mode, SessionMode::Prefetch);
    let continuous = prefetch || matches!(mode, SessionMode::Continuous);
    if continuous {
        ensure!(
            matches!(
                (&first, prefetch),
                (MailboxRequestV1::BeginContinuousV4, false)
                    | (MailboxRequestV1::BeginContinuousV6, true)
            ),
            "mailbox V4 negotiation required"
        );
        stream.admit_continuous_service_v4()?;
    } else {
        ensure!(
            !matches!(
                &first,
                MailboxRequestV1::BeginContinuousV4
                    | MailboxRequestV1::BeginContinuousV6
                    | MailboxRequestV1::WatchFetchV6 { .. }
            ),
            "mailbox V4 not selected"
        );
    }
    if !continuous && !matches!(&first, MailboxRequestV1::BeginSession) {
        let reply = service.execute(first).await?;
        authority()?;
        stream.selected_path()?;
        send(stream, &mut channel, &reply, &mut sent).await?;
    } else {
        send(
            stream,
            &mut channel,
            &if prefetch {
                MailboxReplyV1::ContinuousReadyV6
            } else if continuous {
                MailboxReplyV1::ContinuousReadyV4
            } else {
                MailboxReplyV1::SessionReady
            },
            &mut sent,
        )
        .await?;
        let mut index = 0;
        loop {
            // V6 separates idle from request IO; all remaining chunks, quota
            // waiting, durable work and reply share the first-byte deadline.
            // V1-V5 retain their original bounded lifecycle.
            let first = if prefetch {
                stream.complete_service_request_v6()?;
                Some(stream.read_idle_service_frame_v6().await?)
            } else {
                None
            };
            let deadline = if prefetch {
                stream.service_request_deadline_v6()?
            } else {
                std::time::Instant::now() + std::time::Duration::from_secs(30)
            };
            let done = tokio::time::timeout_at(deadline.into(), async {
                if continuous {
                    if !prefetch {
                        stream.begin_service_request_v4()?;
                    }
                    service.admit_request_v4().await?;
                }
                let request: MailboxRequestV1 =
                    receive_with_first(stream, &mut channel, &mut received, first).await?;
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
                    continuous || index < MAX_SESSION_REQUESTS_V2,
                    "mailbox session request budget"
                );
                ensure!(
                    !matches!(
                        &request,
                        MailboxRequestV1::BeginSession
                            | MailboxRequestV1::BeginContinuousV4
                            | MailboxRequestV1::BeginContinuousV6
                    ),
                    "mailbox session already active"
                );
                ensure!(
                    prefetch || !matches!(&request, MailboxRequestV1::WatchFetchV6 { .. }),
                    "mailbox V6 not negotiated"
                );
                let reply = service.execute(request).await?;
                authority()?;
                stream.selected_path()?;
                service.timings.carrier_rtt(stream.selected_rtt());
                let writing = service.timings.response_enqueue.start();
                send(stream, &mut channel, &reply, &mut sent).await?;
                writing.complete(false);
                Ok(false)
            })
            .await
            .map_err(|_| anyhow::anyhow!("mailbox request deadline"))??;
            if done {
                break;
            }
            if !continuous {
                index += 1;
            }
        }
    }
    authority()?;
    stream.finish().await
}

#[cfg(test)]
mod notification_tests {
    use super::*;
    #[tokio::test]
    async fn continuous_replay_is_rejected_with_original_counters() -> Result<()> {
        use crate::iroh_transport::{with_iroh_owned_listener_v1, IrohEndpointOwnerV1};
        fn key(n: u8) -> SigningKey {
            SigningKey::from_bytes(&[n; 32])
        }
        fn config() -> crate::iroh_transport::IrohConfigV1 {
            crate::iroh_transport::IrohConfigV1::new(&[], "127.0.0.1".parse().unwrap())
                .unwrap()
                .with_bind_address("127.0.0.1:0".parse().unwrap())
                .unwrap()
        }
        let dir = tempfile::tempdir()?;
        let service = MailboxServiceV1::new(
            MailboxStore::open(dir.path().join("continuous.db"), Default::default())?,
            key(1),
        );
        let (tx, rx) = tokio::sync::oneshot::channel();
        let server = with_iroh_owned_listener_v1(
            config().with_owned_service_identity(Zeroizing::new([90; 32]))?,
            || Ok(()),
            move |scope| {
                Box::pin(async move {
                    tx.send(scope.address()?)
                        .map_err(|_| anyhow::anyhow!("fixture address"))?;
                    let mut stream = scope.accept_inbound().await?.into();
                    let result = serve_mailbox_continuous_v4(
                        &mut stream,
                        &key(1),
                        &[key(2).verifying_key().to_bytes()],
                        &service,
                        || Ok(()),
                    )
                    .await;
                    drop(stream);
                    ensure!(result.unwrap_err().to_string().contains("mailbox sequence"));
                    Ok(())
                })
            },
        );
        let owner = IrohEndpointOwnerV1::bind(config(), || Ok(())).await?;
        let client = owner.with_continuous_service_v4(
            || Ok(()),
            move |scope| {
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
                    let bytes = serde_json::to_vec(&MailboxRequestV1::BeginContinuousV4)?;
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
                        MailboxReplyV1::ContinuousReadyV4
                    ));
                    stream.admit_continuous_service_v4()?;
                    stream.write_frame(&encoded).await?;
                    ensure!(stream.read_frame().await.is_err());
                    Ok(())
                })
            },
        );
        let (a, b) = tokio::join!(server, client);
        a?;
        b?;
        owner.close().await?;
        Ok(())
    }
    #[test]
    fn continuous_budget_is_bounded_and_refills_without_lifetime_reset() {
        let mut budget = RequestBudgetV4 {
            tokens: 64,
            updated: std::time::Instant::now(),
            burst: 64,
            interval: std::time::Duration::from_millis(500),
        };
        for _ in 0..64 {
            assert!(budget.take().is_none());
        }
        assert!(budget.take().is_some());
        budget.updated -= std::time::Duration::from_millis(1000);
        assert!(budget.take().is_none());
        assert!(budget.take().is_none());
        assert!(budget.take().is_some());
        budget.updated -= std::time::Duration::from_secs(120);
        for _ in 0..64 {
            assert!(budget.take().is_none());
        }
        assert!(budget.take().is_some());
    }
    #[tokio::test(flavor = "current_thread")]
    async fn contribution_stop_orders_with_real_commits_and_preserves_lost_ack_recovery(
    ) -> Result<()> {
        for stop_first in [true, false] {
            let dir = tempfile::tempdir()?;
            let mut store = MailboxStore::open_contributor(
                dir.path().join("contribution.db"),
                Default::default(),
                true,
            )?;
            store.set_resources_available(true);
            let expiry = now()? + 600;
            let grant = store.create(now()?, 900)?;
            let service = MailboxServiceV1::new(store, SigningKey::from_bytes(&[1; 32]));
            let put = || MailboxRequestV1::Put {
                mailbox: grant.id,
                write: *grant.write.as_bytes(),
                object: [3; 32],
                ciphertext: vec![4; 8],
                expires: expiry,
            };
            // Force both possible orders on the real fair commit queue. No sleeps
            // or assumed worker scheduling determine which operation wins.
            let held = service.slots.clone().acquire_owned().await?;
            let stopping = service.set_contribution_enabled(false);
            let writing = service.execute(put());
            tokio::pin!(stopping, writing);
            if stop_first {
                assert!(futures_util::poll!(&mut stopping).is_pending());
                assert!(futures_util::poll!(&mut writing).is_pending());
            } else {
                assert!(futures_util::poll!(&mut writing).is_pending());
                assert!(futures_util::poll!(&mut stopping).is_pending());
            }
            drop(held);
            let (stopped, written) =
                tokio::time::timeout(std::time::Duration::from_secs(3), async {
                    tokio::join!(&mut stopping, &mut writing)
                })
                .await?;
            stopped?;
            let written = written?;
            if stop_first {
                assert!(matches!(
                    written,
                    MailboxReplyV1::Rejected(MailboxRejectionV1::Quota)
                ));
            } else {
                let MailboxReplyV1::Stored(receipt) = written else {
                    anyhow::bail!("committed Put lost")
                };
                receipt.verify(
                    SigningKey::from_bytes(&[1; 32]).verifying_key().to_bytes(),
                    grant.id,
                    [3; 32],
                    &[4; 8],
                    expiry,
                )?;
                // Owner stop cannot invalidate a committed receipt, even if its
                // first response was lost. The original commitment is replayed.
                assert!(matches!(
                    service.execute(put()).await?,
                    MailboxReplyV1::Stored(_)
                ));
            }
            let state = service.contribution_status().await?;
            assert!(!state.enabled);
            assert_eq!(state.retained_objects, u64::from(!stop_first));
            assert_eq!(state.retained_bytes, if stop_first { 0 } else { 8 });
            assert!(matches!(
                service
                    .execute(MailboxRequestV1::Put {
                        mailbox: grant.id,
                        write: *grant.write.as_bytes(),
                        object: [5; 32],
                        ciphertext: vec![6; 8],
                        expires: expiry,
                    })
                    .await?,
                MailboxReplyV1::Rejected(MailboxRejectionV1::Quota)
            ));
            assert_eq!(service.slots.available_permits(), 1);
        }
        Ok(())
    }

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
        assert!(matches!(
            service
                .execute(MailboxRequestV1::Fetch {
                    mailbox: [3; 32],
                    read: [6; 32],
                })
                .await?,
            MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
        ));
        // Timings include the real queue wait above and count a capability
        // rejection separately; they never replace the actual storage result.
        let measured = serde_json::to_value(service.timing_snapshot_v1())?;
        assert_eq!(measured["other"]["completed"], 1);
        assert!(measured["other"]["max_us"].as_u64().unwrap() >= 20_000);
        assert_eq!(measured["fetch"]["completed"], 2);
        assert_eq!(measured["fetch"]["rejected"], 1);
        assert_eq!(measured["fetch"]["interrupted"], 0);
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

#[cfg(test)]
mod prefetch_tests {
    use super::*;
    #[tokio::test]
    async fn bounded_prefetch_checks_every_capability_and_retains_object_until_consume(
    ) -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("prefetch.db");
        let mut store = MailboxStore::open(&path, Default::default())?;
        let a = store.create(now()?, 600)?;
        let b = store.create(now()?, 600)?;
        let expires = now()? + 300;
        let data = vec![255; 64 * 1024];
        store.put(&a.id, &a.write, &[7; 32], &data, expires, now()?)?;
        store.put(&b.id, &b.write, &[8; 32], &data, expires, now()?)?;
        let service = MailboxServiceV1::new(store, SigningKey::from_bytes(&[1; 32]));
        let mut queries = vec![
            MailboxWatchV1 {
                mailbox: a.id,
                read: *a.read.as_bytes(),
                after: None,
            },
            MailboxWatchV1 {
                mailbox: b.id,
                read: [99; 32],
                after: None,
            },
        ];
        assert!(matches!(
            service.watch_inner(&queries, 0, true).await?,
            MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
        ));
        queries[1].read = *b.read.as_bytes();
        let reply = service.watch_inner(&queries, 0, true).await?;
        assert!(serde_json::to_vec(&reply)?.len() <= MAX_WIRE);
        let digest = match reply {
            MailboxReplyV1::NoticeObjectV6 {
                notices,
                object: Some(row),
            } => {
                assert_eq!(notices.len(), 2);
                assert!(notices.iter().all(|n| n.pending));
                assert_eq!(row.box_index, 0);
                assert_eq!(row.object, [7; 32]);
                assert_eq!(row.ciphertext, data);
                assert_eq!(row.expires, expires);
                row.digest
            }
            _ => anyhow::bail!("prefetch missing"),
        };
        // No Consume: dropped notifications and node restart must retain the original.
        drop(service);
        let mut store = MailboxStore::open(&path, Default::default())?;
        let row = store.fetch_one(&a.id, &a.read, now()?)?.unwrap();
        assert_eq!(row.id, [7; 32]);
        assert_eq!(row.expires, expires);
        store.consume(&a.id, &a.read, &row.id, &digest, now()?)?;
        assert!(store.fetch_one(&a.id, &a.read, now()?)?.is_none());
        assert!(store.fetch_one(&b.id, &b.read, now()?)?.is_some());
        store.revoke(&b.id, &b.read, now()?)?;
        let service = MailboxServiceV1::new(store, SigningKey::from_bytes(&[1; 32]));
        assert!(matches!(
            service.watch_inner(&queries, 0, true).await?,
            MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
        ));
        Ok(())
    }
}

#[cfg(test)]
#[path = "mailbox_idle_tests.rs"]
mod idle_tests;

#[cfg(all(test, feature = "tls-transport"))]
#[path = "mailbox_tls_tests.rs"]
mod tls_tests;
