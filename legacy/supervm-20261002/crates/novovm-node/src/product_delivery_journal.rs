#![forbid(unsafe_code)]

//! Host-owned durable delivery obligations for the NOVOVM Product Overlay.
//!
//! This store deliberately knows nothing about AOEM or NOV business execution.
//! It persists opaque, per-recipient transport obligations and recipient-side
//! admission records so the node can recover them after a process restart.

use anyhow::{bail, Context, Result};
use rocksdb::{
    Direction as RocksDbDirection, IteratorMode as RocksDbIteratorMode, Options as RocksDbOptions,
    WriteBatch as RocksDbWriteBatch, WriteOptions, DB,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};

pub const PRODUCT_DELIVERY_JOURNAL_SCHEMA_V1: &str = "novovm-product-delivery-journal/v1";
pub const PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1: &str = "native_transaction";
pub const PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_SEAL_V1: &str = "native_seal";
pub const PRODUCT_DELIVERY_JOURNAL_MAX_SCAN_V1: usize = 1_000_000;

const KEY_PREFIX_V1: &str = "product_delivery_journal/v1/";
const KEY_SCHEMA_V1: &[u8] = b"product_delivery_journal/v1/schema";
const KEY_SCOPE_V1: &[u8] = b"product_delivery_journal/v1/scope";
const KEY_USAGE_V1: &[u8] = b"product_delivery_journal/v1/usage";
const SCOPE_SCHEMA_V1: &str = "novovm-product-delivery-journal-scope/v1";
const USAGE_SCHEMA_V1: &str = "novovm-product-delivery-journal-usage/v1";
const OUTBOUND_SCHEMA_V1: &str = "novovm-product-delivery-outbound/v1";
const OUTBOUND_FANOUT_SCHEMA_V1: &str = "novovm-product-delivery-fanout/v1";
const INBOUND_SCHEMA_V1: &str = "novovm-product-delivery-inbound/v1";
const TOMBSTONE_SCHEMA_V1: &str = "novovm-product-delivery-tombstone/v1";

const DELIVERY_ID_DOMAIN_V1: &[u8] = b"novovm-product-delivery-id-v1\0";
const FANOUT_ID_DOMAIN_V1: &[u8] = b"novovm-product-delivery-fanout-id-v1\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductDeliveryJournalConfigV1 {
    pub path: PathBuf,
    /// Maximum logical primary records. Derived RocksDB indexes are excluded.
    pub max_entries: usize,
    /// Maximum retained opaque payload bytes across active inbound/outbound records.
    pub max_bytes: usize,
    pub obligation_ttl_ms: u64,
    pub terminal_retention_ms: u64,
    pub retry_interval_ms: u64,
}

impl ProductDeliveryJournalConfigV1 {
    fn validate(&self) -> Result<()> {
        if self.path.as_os_str().is_empty() {
            bail!("product delivery journal path must not be empty");
        }
        if self.max_entries == 0
            || self.max_entries > PRODUCT_DELIVERY_JOURNAL_MAX_SCAN_V1
            || self.max_bytes == 0
            || self.obligation_ttl_ms == 0
            || self.terminal_retention_ms == 0
            || self.retry_interval_ms == 0
        {
            bail!("product delivery journal limits and durations must be positive and bounded");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductDeliveryJournalScopeV1 {
    pub chain_id: u64,
    pub local_peer_id: String,
}

impl ProductDeliveryJournalScopeV1 {
    fn validate(&self) -> Result<()> {
        if self.chain_id == 0 || !valid_peer_id_v1(self.local_peer_id.as_str()) {
            bail!("product delivery journal scope is invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProductDeliveryJournalScopeBindingV1 {
    schema: String,
    chain_id: u64,
    local_peer_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductDeliveryJournalUsageV1 {
    pub entries: u64,
    pub payload_bytes: u64,
    pub next_inbound_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProductDeliveryJournalUsageRecordV1 {
    schema: String,
    entries: u64,
    payload_bytes: u64,
    next_inbound_sequence: u64,
}

impl ProductDeliveryJournalUsageRecordV1 {
    fn empty() -> Self {
        Self {
            schema: USAGE_SCHEMA_V1.to_string(),
            entries: 0,
            payload_bytes: 0,
            next_inbound_sequence: 1,
        }
    }

    fn public(&self) -> ProductDeliveryJournalUsageV1 {
        ProductDeliveryJournalUsageV1 {
            entries: self.entries,
            payload_bytes: self.payload_bytes,
            next_inbound_sequence: self.next_inbound_sequence,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductDeliveryOutboundStateV1 {
    Pending,
    RelayAdmitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductDeliveryFanoutStateV1 {
    Active,
    Completed,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductDeliveryInboundStateV1 {
    Prepared,
    Accepted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductDeliveryTerminalStateV1 {
    OutboundRecipientAcked,
    OutboundExpired,
    InboundCompleted,
    InboundPreparedExpired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductDeliveryOutboundRecordV1 {
    pub schema: String,
    pub revision: u64,
    pub delivery_id: [u8; 32],
    pub fanout_id: [u8; 32],
    pub chain_id: u64,
    pub payload_class: String,
    pub object_hash: [u8; 32],
    pub payload_sha256: [u8; 32],
    pub original_sender_peer_id: String,
    pub recipient_peer_id: String,
    pub payload: Vec<u8>,
    pub created_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub state: ProductDeliveryOutboundStateV1,
    pub attempt_count: u64,
    pub last_attempt_at_unix_ms: Option<u64>,
    pub next_attempt_at_unix_ms: u64,
    pub relay_admitted_at_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductDeliveryOutboundFanoutV1 {
    pub schema: String,
    pub revision: u64,
    pub fanout_id: [u8; 32],
    pub chain_id: u64,
    pub payload_class: String,
    pub object_hash: [u8; 32],
    pub payload_sha256: [u8; 32],
    pub original_sender_peer_id: String,
    pub recipient_peer_ids: Vec<String>,
    pub delivery_ids: Vec<[u8; 32]>,
    pub created_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub state: ProductDeliveryFanoutStateV1,
    pub all_acked_at_unix_ms: Option<u64>,
    pub completion_claimed_at_unix_ms: Option<u64>,
    pub completion_observed: bool,
    pub completion_observed_at_unix_ms: Option<u64>,
    pub retain_until_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductDeliveryInboundRecordV1 {
    pub schema: String,
    pub revision: u64,
    pub delivery_id: [u8; 32],
    pub chain_id: u64,
    pub payload_class: String,
    pub object_hash: [u8; 32],
    pub payload_sha256: [u8; 32],
    pub original_sender_peer_id: String,
    pub recipient_peer_id: String,
    pub payload: Vec<u8>,
    pub prepared_sequence: u64,
    pub prepared_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub state: ProductDeliveryInboundStateV1,
    pub accepted_at_unix_ms: Option<u64>,
    pub ack_pending: bool,
    pub last_ack_emitted_at_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductDeliveryTombstoneV1 {
    pub schema: String,
    pub revision: u64,
    pub delivery_id: [u8; 32],
    pub fanout_id: Option<[u8; 32]>,
    pub chain_id: u64,
    pub payload_class: String,
    pub object_hash: [u8; 32],
    pub payload_sha256: [u8; 32],
    pub original_sender_peer_id: String,
    pub recipient_peer_id: String,
    pub payload_len: u64,
    pub prepared_sequence: Option<u64>,
    pub terminal_state: ProductDeliveryTerminalStateV1,
    pub terminal_at_unix_ms: u64,
    pub retain_until_unix_ms: u64,
    /// Successful outbound tombstones are retained until their fanout completion
    /// side effect has been durably marked observed.
    pub completion_observed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductDeliveryPrepareDispositionV1 {
    Inserted,
    ExistingActive,
    ExistingRecipientAcked,
    ExistingExpired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductDeliveryPreparedRecipientV1 {
    pub delivery_id: [u8; 32],
    pub recipient_peer_id: String,
    pub disposition: ProductDeliveryPrepareDispositionV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductDeliveryFanoutPrepareResultV1 {
    pub fanout: ProductDeliveryOutboundFanoutV1,
    pub recipients: Vec<ProductDeliveryPreparedRecipientV1>,
    pub inserted_count: usize,
    pub cleanup: ProductDeliveryCleanupSummaryV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductDeliveryRecipientAckV1 {
    pub delivery_id: [u8; 32],
    pub chain_id: u64,
    pub payload_class: String,
    pub object_hash: [u8; 32],
    pub payload_sha256: [u8; 32],
    pub original_sender_peer_id: String,
    pub recipient_peer_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductDeliveryRecipientAckResultV1 {
    pub tombstone: ProductDeliveryTombstoneV1,
    pub duplicate: bool,
    pub late_after_expiry: bool,
    pub fanout_all_acked: bool,
    pub cleanup: ProductDeliveryCleanupSummaryV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductDeliveryInboundPrepareDispositionV1 {
    Inserted,
    ExistingPrepared,
    ExistingAccepted,
    ExistingCompleted,
    ExistingExpired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductDeliveryInboundPrepareResultV1 {
    pub delivery_id: [u8; 32],
    pub disposition: ProductDeliveryInboundPrepareDispositionV1,
    pub record: Option<ProductDeliveryInboundRecordV1>,
    pub tombstone: Option<ProductDeliveryTombstoneV1>,
    pub cleanup: ProductDeliveryCleanupSummaryV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductDeliveryCompletionClaimV1 {
    pub fanout: ProductDeliveryOutboundFanoutV1,
    pub newly_claimed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProductDeliveryCleanupSummaryV1 {
    pub outbound_expired: usize,
    pub inbound_prepared_expired: usize,
    pub fanouts_expired: usize,
    pub tombstones_removed: usize,
    pub fanouts_removed: usize,
}

struct ProductDeliveryJournalProcessEntryV1 {
    db: DB,
    write_lock: Arc<Mutex<()>>,
}

impl Deref for ProductDeliveryJournalProcessEntryV1 {
    type Target = DB;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}

pub struct ProductDeliveryJournalV1 {
    config: ProductDeliveryJournalConfigV1,
    scope: ProductDeliveryJournalScopeV1,
    db: Arc<ProductDeliveryJournalProcessEntryV1>,
    write_lock: Arc<Mutex<()>>,
}

impl ProductDeliveryJournalV1 {
    pub fn open(
        config: ProductDeliveryJournalConfigV1,
        scope: ProductDeliveryJournalScopeV1,
        now_unix_ms: u64,
    ) -> Result<Self> {
        config.validate()?;
        scope.validate()?;
        let process_key = product_delivery_journal_process_key_v1(config.path.as_path())?;
        let mut registry = product_delivery_journal_process_registry_v1()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(parent) = config.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "create product delivery journal parent failed: {}",
                        parent.display()
                    )
                })?;
            }
        }
        registry.retain(|_, entry| entry.strong_count() > 0);
        let db = if let Some(entry) = registry.get(process_key.as_str()).and_then(Weak::upgrade) {
            entry
        } else {
            let mut options = RocksDbOptions::default();
            options.create_if_missing(true);
            let db = DB::open(&options, config.path.as_path()).with_context(|| {
                format!(
                    "open product delivery journal failed: {}",
                    config.path.display()
                )
            })?;
            let entry = Arc::new(ProductDeliveryJournalProcessEntryV1 {
                db,
                write_lock: Arc::new(Mutex::new(())),
            });
            registry.insert(process_key, Arc::downgrade(&entry));
            entry
        };
        drop(registry);

        let journal = Self {
            config,
            scope,
            write_lock: Arc::clone(&db.write_lock),
            db,
        };
        journal.initialize_and_reconcile_v1()?;
        journal.cleanup_expired(now_unix_ms)?;
        Ok(journal)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        self.config.path.as_path()
    }

    #[must_use]
    pub fn scope(&self) -> &ProductDeliveryJournalScopeV1 {
        &self.scope
    }

    pub fn usage(&self) -> Result<ProductDeliveryJournalUsageV1> {
        self.ensure_schema_and_scope_v1()?;
        Ok(self.load_usage_inner_v1()?.public())
    }

    fn initialize_and_reconcile_v1(&self) -> Result<()> {
        let _guard = self.lock_writes_v1();
        match self
            .db
            .get(KEY_SCHEMA_V1)
            .context("read product delivery journal schema")?
        {
            Some(raw) if raw.as_slice() != PRODUCT_DELIVERY_JOURNAL_SCHEMA_V1.as_bytes() => {
                bail!(
                    "unsupported product delivery journal schema: {}",
                    String::from_utf8_lossy(raw.as_slice())
                );
            }
            Some(_) => {}
            None => {
                let mut batch = RocksDbWriteBatch::default();
                batch.put(KEY_SCHEMA_V1, PRODUCT_DELIVERY_JOURNAL_SCHEMA_V1.as_bytes());
                write_sync_v1(&self.db, batch)
                    .context("initialize product delivery journal schema")?;
            }
        }

        let requested_binding = ProductDeliveryJournalScopeBindingV1 {
            schema: SCOPE_SCHEMA_V1.to_string(),
            chain_id: self.scope.chain_id,
            local_peer_id: self.scope.local_peer_id.clone(),
        };
        match read_json_v1::<ProductDeliveryJournalScopeBindingV1>(
            &self.db,
            KEY_SCOPE_V1,
            "scope binding",
        )? {
            Some(existing) if existing != requested_binding => {
                bail!(
                    "product delivery journal scope mismatch: stored_chain={} stored_peer={} requested_chain={} requested_peer={}",
                    existing.chain_id,
                    existing.local_peer_id,
                    requested_binding.chain_id,
                    requested_binding.local_peer_id
                );
            }
            Some(_) => {}
            None => {
                let mut batch = RocksDbWriteBatch::default();
                put_json_v1(
                    &mut batch,
                    KEY_SCOPE_V1,
                    &requested_binding,
                    "scope binding",
                )?;
                write_sync_v1(&self.db, batch)
                    .context("persist product delivery journal scope binding")?;
            }
        }

        let recomputed = self.recompute_usage_inner_v1()?;
        let stored =
            read_json_v1::<ProductDeliveryJournalUsageRecordV1>(&self.db, KEY_USAGE_V1, "usage")?;
        if stored.as_ref() != Some(&recomputed) {
            let mut batch = RocksDbWriteBatch::default();
            put_json_v1(&mut batch, KEY_USAGE_V1, &recomputed, "usage")?;
            write_sync_v1(&self.db, batch).context("reconcile product delivery journal usage")?;
        }
        Ok(())
    }

    fn ensure_schema_and_scope_v1(&self) -> Result<()> {
        let schema = self
            .db
            .get(KEY_SCHEMA_V1)
            .context("read product delivery journal schema")?
            .context("product delivery journal schema is missing")?;
        if schema.as_slice() != PRODUCT_DELIVERY_JOURNAL_SCHEMA_V1.as_bytes() {
            bail!("product delivery journal schema changed while open");
        }
        let binding = read_json_v1::<ProductDeliveryJournalScopeBindingV1>(
            &self.db,
            KEY_SCOPE_V1,
            "scope binding",
        )?
        .context("product delivery journal scope binding is missing")?;
        if binding.schema != SCOPE_SCHEMA_V1
            || binding.chain_id != self.scope.chain_id
            || binding.local_peer_id != self.scope.local_peer_id
        {
            bail!("product delivery journal scope binding changed while open");
        }
        Ok(())
    }

    fn lock_writes_v1(&self) -> MutexGuard<'_, ()> {
        self.write_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn validate_payload_class_v1(payload_class: &str) -> Result<()> {
    if !matches!(
        payload_class,
        PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1
            | PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_SEAL_V1
    ) {
        bail!("unsupported product delivery payload class: {payload_class}");
    }
    Ok(())
}

fn validate_payload_binding_v1(
    chain_id: u64,
    payload_class: &str,
    object_hash: [u8; 32],
    payload: &[u8],
    original_sender_peer_id: &str,
) -> Result<()> {
    validate_payload_class_v1(payload_class)?;
    if chain_id == 0
        || object_hash == [0u8; 32]
        || payload.is_empty()
        || !valid_peer_id_v1(original_sender_peer_id)
    {
        bail!("product delivery payload binding is invalid");
    }
    Ok(())
}

fn canonical_recipient_set_v1(
    recipient_peer_ids: &[String],
    original_sender_peer_id: &str,
    max_entries: usize,
) -> Result<Vec<String>> {
    if recipient_peer_ids.is_empty() || recipient_peer_ids.len() > max_entries {
        bail!("product delivery recipient set is empty or oversized");
    }
    let mut recipients = recipient_peer_ids.to_vec();
    for recipient in &recipients {
        if !valid_peer_id_v1(recipient) || recipient == original_sender_peer_id {
            bail!("product delivery recipient identity is invalid");
        }
    }
    recipients.sort();
    if recipients.windows(2).any(|pair| pair[0] == pair[1]) {
        bail!("product delivery recipient set contains duplicates");
    }
    Ok(recipients)
}

fn validate_outbound_record_v1(record: &ProductDeliveryOutboundRecordV1) -> Result<()> {
    validate_payload_binding_v1(
        record.chain_id,
        record.payload_class.as_str(),
        record.object_hash,
        record.payload.as_slice(),
        record.original_sender_peer_id.as_str(),
    )?;
    if record.schema != OUTBOUND_SCHEMA_V1
        || record.revision == 0
        || record.fanout_id == [0u8; 32]
        || !valid_peer_id_v1(record.recipient_peer_id.as_str())
        || record.recipient_peer_id == record.original_sender_peer_id
        || record.payload_sha256 != product_delivery_payload_sha256_v1(record.payload.as_slice())
        || record.delivery_id
            != product_delivery_id_v1(
                record.chain_id,
                record.payload_class.as_str(),
                record.object_hash,
                record.payload_sha256,
                record.original_sender_peer_id.as_str(),
                record.recipient_peer_id.as_str(),
            )
        || record.expires_at_unix_ms <= record.created_at_unix_ms
        || record.next_attempt_at_unix_ms < record.created_at_unix_ms
        || record
            .last_attempt_at_unix_ms
            .is_some_and(|value| value < record.created_at_unix_ms)
        || record
            .relay_admitted_at_unix_ms
            .is_some_and(|value| value < record.created_at_unix_ms)
    {
        bail!("product delivery outbound record is invalid");
    }
    Ok(())
}

fn validate_outbound_fanout_v1(fanout: &ProductDeliveryOutboundFanoutV1) -> Result<()> {
    validate_payload_class_v1(fanout.payload_class.as_str())?;
    if fanout.schema != OUTBOUND_FANOUT_SCHEMA_V1
        || fanout.revision == 0
        || fanout.chain_id == 0
        || fanout.object_hash == [0u8; 32]
        || fanout.payload_sha256 == [0u8; 32]
        || !valid_peer_id_v1(fanout.original_sender_peer_id.as_str())
        || fanout.recipient_peer_ids.is_empty()
        || fanout.recipient_peer_ids.len() != fanout.delivery_ids.len()
        || fanout
            .recipient_peer_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || fanout.expires_at_unix_ms <= fanout.created_at_unix_ms
    {
        bail!("product delivery outbound fanout is invalid");
    }
    let expected_fanout_id = product_delivery_fanout_id_v1(
        fanout.chain_id,
        fanout.payload_class.as_str(),
        fanout.object_hash,
        fanout.payload_sha256,
        fanout.original_sender_peer_id.as_str(),
        fanout.recipient_peer_ids.as_slice(),
    );
    if fanout.fanout_id != expected_fanout_id {
        bail!("product delivery outbound fanout id binding is invalid");
    }
    for (recipient, delivery_id) in fanout
        .recipient_peer_ids
        .iter()
        .zip(fanout.delivery_ids.iter())
    {
        if !valid_peer_id_v1(recipient)
            || recipient == &fanout.original_sender_peer_id
            || *delivery_id
                != product_delivery_id_v1(
                    fanout.chain_id,
                    fanout.payload_class.as_str(),
                    fanout.object_hash,
                    fanout.payload_sha256,
                    fanout.original_sender_peer_id.as_str(),
                    recipient,
                )
        {
            bail!("product delivery outbound fanout recipient binding is invalid");
        }
    }
    match fanout.state {
        ProductDeliveryFanoutStateV1::Active => {
            if fanout.completion_observed
                || fanout.completion_observed_at_unix_ms.is_some()
                || fanout.retain_until_unix_ms.is_some()
            {
                bail!("active product delivery fanout has terminal metadata");
            }
        }
        ProductDeliveryFanoutStateV1::Completed => {
            if fanout.all_acked_at_unix_ms.is_none()
                || fanout.completion_claimed_at_unix_ms.is_none()
                || !fanout.completion_observed
                || fanout.completion_observed_at_unix_ms.is_none()
                || fanout.retain_until_unix_ms.is_none()
            {
                bail!("completed product delivery fanout metadata is incomplete");
            }
        }
        ProductDeliveryFanoutStateV1::Expired => {
            if !fanout.completion_observed
                || fanout.completion_observed_at_unix_ms.is_none()
                || fanout.retain_until_unix_ms.is_none()
            {
                bail!("expired product delivery fanout metadata is incomplete");
            }
        }
    }
    Ok(())
}

fn validate_inbound_record_v1(record: &ProductDeliveryInboundRecordV1) -> Result<()> {
    validate_payload_binding_v1(
        record.chain_id,
        record.payload_class.as_str(),
        record.object_hash,
        record.payload.as_slice(),
        record.original_sender_peer_id.as_str(),
    )?;
    if record.schema != INBOUND_SCHEMA_V1
        || record.revision == 0
        || record.prepared_sequence == 0
        || !valid_peer_id_v1(record.recipient_peer_id.as_str())
        || record.recipient_peer_id == record.original_sender_peer_id
        || record.payload_sha256 != product_delivery_payload_sha256_v1(record.payload.as_slice())
        || record.delivery_id
            != product_delivery_id_v1(
                record.chain_id,
                record.payload_class.as_str(),
                record.object_hash,
                record.payload_sha256,
                record.original_sender_peer_id.as_str(),
                record.recipient_peer_id.as_str(),
            )
        || record.expires_at_unix_ms <= record.prepared_at_unix_ms
    {
        bail!("product delivery inbound record is invalid");
    }
    match record.state {
        ProductDeliveryInboundStateV1::Prepared => {
            if record.accepted_at_unix_ms.is_some()
                || record.ack_pending
                || record.last_ack_emitted_at_unix_ms.is_some()
            {
                bail!("prepared inbound delivery has accepted metadata");
            }
        }
        ProductDeliveryInboundStateV1::Accepted => {
            if record.accepted_at_unix_ms.is_none() {
                bail!("accepted inbound delivery is missing its acceptance time");
            }
        }
    }
    Ok(())
}

fn validate_tombstone_v1(tombstone: &ProductDeliveryTombstoneV1) -> Result<()> {
    validate_payload_class_v1(tombstone.payload_class.as_str())?;
    if tombstone.schema != TOMBSTONE_SCHEMA_V1
        || tombstone.revision == 0
        || tombstone.chain_id == 0
        || tombstone.object_hash == [0u8; 32]
        || tombstone.payload_sha256 == [0u8; 32]
        || !valid_peer_id_v1(tombstone.original_sender_peer_id.as_str())
        || !valid_peer_id_v1(tombstone.recipient_peer_id.as_str())
        || tombstone.original_sender_peer_id == tombstone.recipient_peer_id
        || tombstone.payload_len == 0
        || tombstone.retain_until_unix_ms <= tombstone.terminal_at_unix_ms
        || tombstone.delivery_id
            != product_delivery_id_v1(
                tombstone.chain_id,
                tombstone.payload_class.as_str(),
                tombstone.object_hash,
                tombstone.payload_sha256,
                tombstone.original_sender_peer_id.as_str(),
                tombstone.recipient_peer_id.as_str(),
            )
    {
        bail!("product delivery tombstone is invalid");
    }
    match tombstone.terminal_state {
        ProductDeliveryTerminalStateV1::OutboundRecipientAcked
        | ProductDeliveryTerminalStateV1::OutboundExpired => {
            if tombstone.fanout_id.is_none() || tombstone.prepared_sequence.is_some() {
                bail!("outbound product delivery tombstone metadata is invalid");
            }
        }
        ProductDeliveryTerminalStateV1::InboundCompleted
        | ProductDeliveryTerminalStateV1::InboundPreparedExpired => {
            if tombstone.fanout_id.is_some() || tombstone.prepared_sequence.is_none() {
                bail!("inbound product delivery tombstone metadata is invalid");
            }
        }
    }
    Ok(())
}

fn validate_recipient_ack_v1(ack: &ProductDeliveryRecipientAckV1) -> Result<()> {
    validate_payload_class_v1(ack.payload_class.as_str())?;
    if ack.chain_id == 0
        || ack.object_hash == [0u8; 32]
        || ack.payload_sha256 == [0u8; 32]
        || !valid_peer_id_v1(ack.original_sender_peer_id.as_str())
        || !valid_peer_id_v1(ack.recipient_peer_id.as_str())
        || ack.original_sender_peer_id == ack.recipient_peer_id
        || ack.delivery_id
            != product_delivery_id_v1(
                ack.chain_id,
                ack.payload_class.as_str(),
                ack.object_hash,
                ack.payload_sha256,
                ack.original_sender_peer_id.as_str(),
                ack.recipient_peer_id.as_str(),
            )
    {
        bail!("product delivery recipient ACK binding is invalid");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn ensure_same_outbound_binding_v1(
    existing: &ProductDeliveryOutboundRecordV1,
    fanout_id: [u8; 32],
    chain_id: u64,
    payload_class: &str,
    object_hash: [u8; 32],
    payload_sha256: [u8; 32],
    original_sender_peer_id: &str,
    recipient_peer_id: &str,
    payload: &[u8],
) -> Result<()> {
    if existing.fanout_id != fanout_id
        || existing.chain_id != chain_id
        || existing.payload_class != payload_class
        || existing.object_hash != object_hash
        || existing.payload_sha256 != payload_sha256
        || existing.original_sender_peer_id != original_sender_peer_id
        || existing.recipient_peer_id != recipient_peer_id
        || existing.payload != payload
    {
        bail!("product delivery outbound id collision or conflicting fanout");
    }
    Ok(())
}

fn ensure_same_fanout_binding_v1(
    existing: &ProductDeliveryOutboundFanoutV1,
    requested: &ProductDeliveryOutboundFanoutV1,
) -> Result<()> {
    if existing.fanout_id != requested.fanout_id
        || existing.chain_id != requested.chain_id
        || existing.payload_class != requested.payload_class
        || existing.object_hash != requested.object_hash
        || existing.payload_sha256 != requested.payload_sha256
        || existing.original_sender_peer_id != requested.original_sender_peer_id
        || existing.recipient_peer_ids != requested.recipient_peer_ids
        || existing.delivery_ids != requested.delivery_ids
    {
        bail!("product delivery fanout id collision or conflicting target set");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn ensure_same_inbound_binding_v1(
    existing: &ProductDeliveryInboundRecordV1,
    chain_id: u64,
    payload_class: &str,
    object_hash: [u8; 32],
    payload_sha256: [u8; 32],
    original_sender_peer_id: &str,
    recipient_peer_id: &str,
    payload: &[u8],
) -> Result<()> {
    if existing.chain_id != chain_id
        || existing.payload_class != payload_class
        || existing.object_hash != object_hash
        || existing.payload_sha256 != payload_sha256
        || existing.original_sender_peer_id != original_sender_peer_id
        || existing.recipient_peer_id != recipient_peer_id
        || existing.payload != payload
    {
        bail!("product delivery inbound replay equivocation");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn ensure_same_tombstone_binding_v1(
    existing: &ProductDeliveryTombstoneV1,
    fanout_id: Option<[u8; 32]>,
    chain_id: u64,
    payload_class: &str,
    object_hash: [u8; 32],
    payload_sha256: [u8; 32],
    original_sender_peer_id: &str,
    recipient_peer_id: &str,
    payload_len: usize,
) -> Result<()> {
    if existing.fanout_id != fanout_id
        || existing.chain_id != chain_id
        || existing.payload_class != payload_class
        || existing.object_hash != object_hash
        || existing.payload_sha256 != payload_sha256
        || existing.original_sender_peer_id != original_sender_peer_id
        || existing.recipient_peer_id != recipient_peer_id
        || existing.payload_len != u64::try_from(payload_len).context("payload len exceeds u64")?
    {
        bail!("product delivery tombstone id collision or replay equivocation");
    }
    Ok(())
}

fn ensure_ack_matches_outbound_v1(
    ack: &ProductDeliveryRecipientAckV1,
    record: &ProductDeliveryOutboundRecordV1,
) -> Result<()> {
    if ack.delivery_id != record.delivery_id
        || ack.chain_id != record.chain_id
        || ack.payload_class != record.payload_class
        || ack.object_hash != record.object_hash
        || ack.payload_sha256 != record.payload_sha256
        || ack.original_sender_peer_id != record.original_sender_peer_id
        || ack.recipient_peer_id != record.recipient_peer_id
    {
        bail!("recipient ACK does not match its outbound delivery");
    }
    Ok(())
}

fn ensure_ack_matches_tombstone_v1(
    ack: &ProductDeliveryRecipientAckV1,
    tombstone: &ProductDeliveryTombstoneV1,
) -> Result<()> {
    if ack.delivery_id != tombstone.delivery_id
        || ack.chain_id != tombstone.chain_id
        || ack.payload_class != tombstone.payload_class
        || ack.object_hash != tombstone.object_hash
        || ack.payload_sha256 != tombstone.payload_sha256
        || ack.original_sender_peer_id != tombstone.original_sender_peer_id
        || ack.recipient_peer_id != tombstone.recipient_peer_id
    {
        bail!("recipient ACK does not match its delivery tombstone");
    }
    Ok(())
}

fn tombstone_from_outbound_v1(
    record: &ProductDeliveryOutboundRecordV1,
    terminal_state: ProductDeliveryTerminalStateV1,
    terminal_at_unix_ms: u64,
    retain_until_unix_ms: u64,
) -> Result<ProductDeliveryTombstoneV1> {
    if !matches!(
        terminal_state,
        ProductDeliveryTerminalStateV1::OutboundRecipientAcked
            | ProductDeliveryTerminalStateV1::OutboundExpired
    ) {
        bail!("invalid outbound product delivery terminal state");
    }
    let tombstone = ProductDeliveryTombstoneV1 {
        schema: TOMBSTONE_SCHEMA_V1.to_string(),
        revision: record.revision.saturating_add(1),
        delivery_id: record.delivery_id,
        fanout_id: Some(record.fanout_id),
        chain_id: record.chain_id,
        payload_class: record.payload_class.clone(),
        object_hash: record.object_hash,
        payload_sha256: record.payload_sha256,
        original_sender_peer_id: record.original_sender_peer_id.clone(),
        recipient_peer_id: record.recipient_peer_id.clone(),
        payload_len: u64::try_from(record.payload.len()).context("payload len exceeds u64")?,
        prepared_sequence: None,
        terminal_state,
        terminal_at_unix_ms,
        retain_until_unix_ms,
        completion_observed: terminal_state
            != ProductDeliveryTerminalStateV1::OutboundRecipientAcked,
    };
    validate_tombstone_v1(&tombstone)?;
    Ok(tombstone)
}

fn tombstone_from_inbound_v1(
    record: &ProductDeliveryInboundRecordV1,
    terminal_state: ProductDeliveryTerminalStateV1,
    terminal_at_unix_ms: u64,
    retain_until_unix_ms: u64,
) -> Result<ProductDeliveryTombstoneV1> {
    if !matches!(
        terminal_state,
        ProductDeliveryTerminalStateV1::InboundCompleted
            | ProductDeliveryTerminalStateV1::InboundPreparedExpired
    ) {
        bail!("invalid inbound product delivery terminal state");
    }
    let tombstone = ProductDeliveryTombstoneV1 {
        schema: TOMBSTONE_SCHEMA_V1.to_string(),
        revision: record.revision.saturating_add(1),
        delivery_id: record.delivery_id,
        fanout_id: None,
        chain_id: record.chain_id,
        payload_class: record.payload_class.clone(),
        object_hash: record.object_hash,
        payload_sha256: record.payload_sha256,
        original_sender_peer_id: record.original_sender_peer_id.clone(),
        recipient_peer_id: record.recipient_peer_id.clone(),
        payload_len: u64::try_from(record.payload.len()).context("payload len exceeds u64")?,
        prepared_sequence: Some(record.prepared_sequence),
        terminal_state,
        terminal_at_unix_ms,
        retain_until_unix_ms,
        completion_observed: true,
    };
    validate_tombstone_v1(&tombstone)?;
    Ok(tombstone)
}

fn product_delivery_fanout_id_v1(
    chain_id: u64,
    payload_class: &str,
    object_hash: [u8; 32],
    payload_sha256: [u8; 32],
    original_sender_peer_id: &str,
    recipient_peer_ids: &[String],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(FANOUT_ID_DOMAIN_V1);
    update_len_prefixed_v1(&mut hasher, chain_id.to_be_bytes().as_slice());
    update_len_prefixed_v1(&mut hasher, payload_class.as_bytes());
    update_len_prefixed_v1(&mut hasher, object_hash.as_slice());
    update_len_prefixed_v1(&mut hasher, payload_sha256.as_slice());
    update_len_prefixed_v1(&mut hasher, original_sender_peer_id.as_bytes());
    hasher.update((recipient_peer_ids.len() as u64).to_be_bytes());
    for recipient in recipient_peer_ids {
        update_len_prefixed_v1(&mut hasher, recipient.as_bytes());
    }
    hasher.finalize().into()
}

fn valid_peer_id_v1(peer_id: &str) -> bool {
    !peer_id.is_empty()
        && peer_id.len() <= 256
        && peer_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.'))
}

fn validate_scan_limit_v1(limit: usize, configured_max: usize) -> Result<()> {
    if limit == 0 || limit > configured_max || limit > PRODUCT_DELIVERY_JOURNAL_MAX_SCAN_V1 {
        bail!("product delivery journal scan limit is invalid");
    }
    Ok(())
}

fn checked_scan_increment_v1(scanned: usize) -> Result<usize> {
    let scanned = scanned
        .checked_add(1)
        .context("product delivery journal scan count overflow")?;
    if scanned > PRODUCT_DELIVERY_JOURNAL_MAX_SCAN_V1 {
        bail!("product delivery journal scan exceeds its fail-closed bound");
    }
    Ok(scanned)
}

fn hash_parts_v1(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        update_len_prefixed_v1(&mut hasher, part);
    }
    hasher.finalize().into()
}

fn update_len_prefixed_v1(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn write_sync_v1(db: &DB, batch: RocksDbWriteBatch) -> Result<()> {
    let mut options = WriteOptions::default();
    options.set_sync(true);
    db.write_opt(batch, &options)
        .context("write synchronized product delivery journal batch")
}

fn read_json_v1<T: DeserializeOwned>(db: &DB, key: &[u8], label: &str) -> Result<Option<T>> {
    db.get(key)
        .with_context(|| format!("read product delivery journal {label}"))?
        .map(|raw| {
            serde_json::from_slice(raw.as_slice())
                .with_context(|| format!("decode product delivery journal {label}"))
        })
        .transpose()
}

fn put_json_v1<T: Serialize>(
    batch: &mut RocksDbWriteBatch,
    key: &[u8],
    value: &T,
    label: &str,
) -> Result<()> {
    let encoded = serde_json::to_vec(value)
        .with_context(|| format!("encode product delivery journal {label}"))?;
    batch.put(key, encoded);
    Ok(())
}

fn product_delivery_journal_process_registry_v1(
) -> &'static Mutex<HashMap<String, Weak<ProductDeliveryJournalProcessEntryV1>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Weak<ProductDeliveryJournalProcessEntryV1>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn product_delivery_journal_process_key_v1(path: &Path) -> Result<String> {
    let absolute = std::path::absolute(path)
        .with_context(|| format!("resolve product delivery journal path: {}", path.display()))?;
    let mut key = absolute.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        key.make_ascii_lowercase();
    }
    Ok(key)
}

fn hex_v1(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex_32_v1(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 {
        bail!("product delivery index id is not canonical 32-byte hex");
    }
    let mut out = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = decode_hex_nibble_v1(pair[0]).context("invalid product delivery index hex")?;
        let low = decode_hex_nibble_v1(pair[1]).context("invalid product delivery index hex")?;
        out[index] = (high << 4) | low;
    }
    Ok(out)
}

fn decode_hex_nibble_v1(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn parse_timed_delivery_index_key_v1(
    key: &[u8],
    prefix: &[u8],
    label: &str,
) -> Result<(u64, [u8; 32])> {
    let suffix = key
        .strip_prefix(prefix)
        .with_context(|| format!("{label} index prefix mismatch"))?;
    let suffix =
        std::str::from_utf8(suffix).with_context(|| format!("{label} index is not UTF-8"))?;
    let (time, id) = suffix
        .split_once('/')
        .with_context(|| format!("{label} index shape is invalid"))?;
    if time.len() != 20 {
        bail!("{label} index time is not zero-padded u64");
    }
    let time = time
        .parse::<u64>()
        .with_context(|| format!("{label} index time is invalid"))?;
    Ok((time, decode_hex_32_v1(id)?))
}

fn outbound_prefix_v1() -> String {
    format!("{KEY_PREFIX_V1}outbound/record/")
}

fn outbound_key_v1(delivery_id: &[u8; 32]) -> String {
    format!("{}{}", outbound_prefix_v1(), hex_v1(delivery_id))
}

fn outbound_due_prefix_v1() -> String {
    format!("{KEY_PREFIX_V1}outbound/due/")
}

fn outbound_due_key_v1(next_attempt_at: u64, delivery_id: &[u8; 32]) -> String {
    format!(
        "{}{next_attempt_at:020}/{}",
        outbound_due_prefix_v1(),
        hex_v1(delivery_id)
    )
}

fn outbound_expiry_prefix_v1() -> String {
    format!("{KEY_PREFIX_V1}outbound/expiry/")
}

fn outbound_expiry_key_v1(expires_at: u64, delivery_id: &[u8; 32]) -> String {
    format!(
        "{}{expires_at:020}/{}",
        outbound_expiry_prefix_v1(),
        hex_v1(delivery_id)
    )
}

fn outbound_fanout_prefix_v1() -> String {
    format!("{KEY_PREFIX_V1}outbound/fanout/")
}

fn outbound_fanout_key_v1(fanout_id: &[u8; 32]) -> String {
    format!("{}{}", outbound_fanout_prefix_v1(), hex_v1(fanout_id))
}

fn outbound_fanout_expiry_prefix_v1() -> String {
    format!("{KEY_PREFIX_V1}outbound/fanout_expiry/")
}

fn outbound_fanout_expiry_key_v1(expires_at: u64, fanout_id: &[u8; 32]) -> String {
    format!(
        "{}{expires_at:020}/{}",
        outbound_fanout_expiry_prefix_v1(),
        hex_v1(fanout_id)
    )
}

fn outbound_fanout_retention_prefix_v1() -> String {
    format!("{KEY_PREFIX_V1}outbound/fanout_retention/")
}

fn outbound_fanout_retention_key_v1(retain_until: u64, fanout_id: &[u8; 32]) -> String {
    format!(
        "{}{retain_until:020}/{}",
        outbound_fanout_retention_prefix_v1(),
        hex_v1(fanout_id)
    )
}

fn inbound_prefix_v1() -> String {
    format!("{KEY_PREFIX_V1}inbound/record/")
}

fn inbound_key_v1(delivery_id: &[u8; 32]) -> String {
    format!("{}{}", inbound_prefix_v1(), hex_v1(delivery_id))
}

fn inbound_expiry_prefix_v1() -> String {
    format!("{KEY_PREFIX_V1}inbound/expiry/")
}

fn inbound_expiry_key_v1(expires_at: u64, delivery_id: &[u8; 32]) -> String {
    format!(
        "{}{expires_at:020}/{}",
        inbound_expiry_prefix_v1(),
        hex_v1(delivery_id)
    )
}

fn tombstone_prefix_v1() -> String {
    format!("{KEY_PREFIX_V1}terminal/tombstone/")
}

fn tombstone_key_v1(delivery_id: &[u8; 32]) -> String {
    format!("{}{}", tombstone_prefix_v1(), hex_v1(delivery_id))
}

fn tombstone_retention_prefix_v1() -> String {
    format!("{KEY_PREFIX_V1}terminal/retention/")
}

fn tombstone_retention_key_v1(retain_until: u64, delivery_id: &[u8; 32]) -> String {
    format!(
        "{}{retain_until:020}/{}",
        tombstone_retention_prefix_v1(),
        hex_v1(delivery_id)
    )
}

impl ProductDeliveryJournalV1 {
    fn load_usage_inner_v1(&self) -> Result<ProductDeliveryJournalUsageRecordV1> {
        let usage =
            read_json_v1::<ProductDeliveryJournalUsageRecordV1>(&self.db, KEY_USAGE_V1, "usage")?
                .context("product delivery journal usage is missing")?;
        if usage.schema != USAGE_SCHEMA_V1 || usage.next_inbound_sequence == 0 {
            bail!("product delivery journal usage record is invalid");
        }
        Ok(usage)
    }

    fn recompute_usage_inner_v1(&self) -> Result<ProductDeliveryJournalUsageRecordV1> {
        let mut usage = ProductDeliveryJournalUsageRecordV1::empty();
        let mut max_inbound_sequence = 0u64;
        for (prefix, kind) in [
            (outbound_fanout_prefix_v1(), "outbound_fanout"),
            (outbound_prefix_v1(), "outbound"),
            (inbound_prefix_v1(), "inbound"),
            (tombstone_prefix_v1(), "tombstone"),
        ] {
            let mut scanned = 0usize;
            for item in self.db.iterator(RocksDbIteratorMode::From(
                prefix.as_bytes(),
                RocksDbDirection::Forward,
            )) {
                let (key, raw) = item.context("iterate product delivery usage records")?;
                if !key.starts_with(prefix.as_bytes()) {
                    break;
                }
                scanned = checked_scan_increment_v1(scanned)?;
                usage.entries = usage
                    .entries
                    .checked_add(1)
                    .context("product delivery usage entries overflow")?;
                match kind {
                    "outbound_fanout" => {
                        let value: ProductDeliveryOutboundFanoutV1 =
                            serde_json::from_slice(raw.as_ref())
                                .context("decode outbound fanout during usage recovery")?;
                        validate_outbound_fanout_v1(&value)?;
                    }
                    "outbound" => {
                        let value: ProductDeliveryOutboundRecordV1 =
                            serde_json::from_slice(raw.as_ref())
                                .context("decode outbound record during usage recovery")?;
                        validate_outbound_record_v1(&value)?;
                        usage.payload_bytes = usage
                            .payload_bytes
                            .checked_add(
                                u64::try_from(value.payload.len())
                                    .context("outbound payload len exceeds u64")?,
                            )
                            .context("product delivery usage bytes overflow")?;
                    }
                    "inbound" => {
                        let value: ProductDeliveryInboundRecordV1 =
                            serde_json::from_slice(raw.as_ref())
                                .context("decode inbound record during usage recovery")?;
                        validate_inbound_record_v1(&value)?;
                        usage.payload_bytes = usage
                            .payload_bytes
                            .checked_add(
                                u64::try_from(value.payload.len())
                                    .context("inbound payload len exceeds u64")?,
                            )
                            .context("product delivery usage bytes overflow")?;
                        max_inbound_sequence = max_inbound_sequence.max(value.prepared_sequence);
                    }
                    "tombstone" => {
                        let value: ProductDeliveryTombstoneV1 =
                            serde_json::from_slice(raw.as_ref())
                                .context("decode tombstone during usage recovery")?;
                        validate_tombstone_v1(&value)?;
                        if let Some(sequence) = value.prepared_sequence {
                            max_inbound_sequence = max_inbound_sequence.max(sequence);
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
        usage.next_inbound_sequence = max_inbound_sequence
            .checked_add(1)
            .context("product delivery inbound sequence overflow")?
            .max(1);
        Ok(usage)
    }

    fn ensure_capacity_v1(
        &self,
        usage: &ProductDeliveryJournalUsageRecordV1,
        additional_entries: usize,
        additional_payload_bytes: usize,
    ) -> Result<()> {
        let requested_entries = usage
            .entries
            .checked_add(
                u64::try_from(additional_entries).context("additional entries exceed u64")?,
            )
            .context("product delivery entry capacity overflow")?;
        let requested_bytes = usage
            .payload_bytes
            .checked_add(
                u64::try_from(additional_payload_bytes)
                    .context("additional payload bytes exceed u64")?,
            )
            .context("product delivery byte capacity overflow")?;
        if requested_entries
            > u64::try_from(self.config.max_entries).context("max entries exceed u64")?
            || requested_bytes
                > u64::try_from(self.config.max_bytes).context("max bytes exceed u64")?
        {
            bail!(
                "product delivery journal capacity exceeded: entries={requested_entries}/{} payload_bytes={requested_bytes}/{}",
                self.config.max_entries,
                self.config.max_bytes
            );
        }
        Ok(())
    }

    fn load_outbound_inner_v1(
        &self,
        delivery_id: [u8; 32],
    ) -> Result<Option<ProductDeliveryOutboundRecordV1>> {
        read_json_v1(
            &self.db,
            outbound_key_v1(&delivery_id).as_bytes(),
            "outbound record",
        )
    }

    fn load_outbound_fanout_inner_v1(
        &self,
        fanout_id: [u8; 32],
    ) -> Result<Option<ProductDeliveryOutboundFanoutV1>> {
        read_json_v1(
            &self.db,
            outbound_fanout_key_v1(&fanout_id).as_bytes(),
            "outbound fanout",
        )
    }

    fn load_inbound_inner_v1(
        &self,
        delivery_id: [u8; 32],
    ) -> Result<Option<ProductDeliveryInboundRecordV1>> {
        read_json_v1(
            &self.db,
            inbound_key_v1(&delivery_id).as_bytes(),
            "inbound record",
        )
    }

    fn load_tombstone_inner_v1(
        &self,
        delivery_id: [u8; 32],
    ) -> Result<Option<ProductDeliveryTombstoneV1>> {
        read_json_v1(
            &self.db,
            tombstone_key_v1(&delivery_id).as_bytes(),
            "tombstone",
        )
    }

    fn readback_outbound_v1(
        &self,
        expected: &ProductDeliveryOutboundRecordV1,
    ) -> Result<ProductDeliveryOutboundRecordV1> {
        let readback = self
            .load_outbound_inner_v1(expected.delivery_id)?
            .context("outbound delivery readback is missing")?;
        if &readback != expected {
            bail!("outbound delivery readback mismatch");
        }
        Ok(readback)
    }

    fn readback_inbound_v1(
        &self,
        expected: &ProductDeliveryInboundRecordV1,
    ) -> Result<ProductDeliveryInboundRecordV1> {
        let readback = self
            .load_inbound_inner_v1(expected.delivery_id)?
            .context("inbound delivery readback is missing")?;
        if &readback != expected {
            bail!("inbound delivery readback mismatch");
        }
        Ok(readback)
    }

    fn fanout_all_acked_inner_v1(
        &self,
        fanout_id: [u8; 32],
        acknowledged_override: Option<[u8; 32]>,
    ) -> Result<bool> {
        let fanout = self
            .load_outbound_fanout_inner_v1(fanout_id)?
            .context("product delivery fanout is missing")?;
        validate_outbound_fanout_v1(&fanout)?;
        for delivery_id in &fanout.delivery_ids {
            if acknowledged_override == Some(*delivery_id) {
                continue;
            }
            let Some(tombstone) = self.load_tombstone_inner_v1(*delivery_id)? else {
                return Ok(false);
            };
            validate_tombstone_v1(&tombstone)?;
            if tombstone.fanout_id != Some(fanout_id)
                || tombstone.terminal_state
                    != ProductDeliveryTerminalStateV1::OutboundRecipientAcked
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn collect_expired_outbound_v1(
        &self,
        now_unix_ms: u64,
    ) -> Result<Vec<ProductDeliveryOutboundRecordV1>> {
        let prefix = outbound_expiry_prefix_v1();
        let mut records = Vec::new();
        let mut scanned = 0usize;
        for item in self.db.iterator(RocksDbIteratorMode::From(
            prefix.as_bytes(),
            RocksDbDirection::Forward,
        )) {
            let (key, _) = item.context("iterate outbound delivery expiry index")?;
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }
            scanned = checked_scan_increment_v1(scanned)?;
            let (expires_at, delivery_id) = parse_timed_delivery_index_key_v1(
                key.as_ref(),
                prefix.as_bytes(),
                "outbound expiry",
            )?;
            if expires_at > now_unix_ms {
                break;
            }
            let record = self
                .load_outbound_inner_v1(delivery_id)?
                .context("outbound expiry index points to a missing record")?;
            validate_outbound_record_v1(&record)?;
            if record.expires_at_unix_ms != expires_at {
                bail!("outbound expiry index does not match its record");
            }
            records.push(record);
        }
        Ok(records)
    }

    fn collect_expired_inbound_v1(
        &self,
        now_unix_ms: u64,
    ) -> Result<Vec<ProductDeliveryInboundRecordV1>> {
        let prefix = inbound_expiry_prefix_v1();
        let mut records = Vec::new();
        let mut scanned = 0usize;
        for item in self.db.iterator(RocksDbIteratorMode::From(
            prefix.as_bytes(),
            RocksDbDirection::Forward,
        )) {
            let (key, _) = item.context("iterate inbound delivery expiry index")?;
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }
            scanned = checked_scan_increment_v1(scanned)?;
            let (expires_at, delivery_id) = parse_timed_delivery_index_key_v1(
                key.as_ref(),
                prefix.as_bytes(),
                "inbound expiry",
            )?;
            if expires_at > now_unix_ms {
                break;
            }
            let record = self
                .load_inbound_inner_v1(delivery_id)?
                .context("inbound expiry index points to a missing record")?;
            validate_inbound_record_v1(&record)?;
            if record.expires_at_unix_ms != expires_at
                || record.state != ProductDeliveryInboundStateV1::Prepared
            {
                bail!("inbound expiry index does not match a prepared record");
            }
            records.push(record);
        }
        Ok(records)
    }

    fn expire_terminal_fanouts_v1(&self, now_unix_ms: u64) -> Result<usize> {
        let prefix = outbound_fanout_expiry_prefix_v1();
        let mut expired = Vec::new();
        let mut scanned = 0usize;
        for item in self.db.iterator(RocksDbIteratorMode::From(
            prefix.as_bytes(),
            RocksDbDirection::Forward,
        )) {
            let (key, _) = item.context("iterate outbound fanout expiry index")?;
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }
            scanned = checked_scan_increment_v1(scanned)?;
            let (expires_at, fanout_id) = parse_timed_delivery_index_key_v1(
                key.as_ref(),
                prefix.as_bytes(),
                "outbound fanout expiry",
            )?;
            if expires_at > now_unix_ms {
                break;
            }
            let mut fanout = self
                .load_outbound_fanout_inner_v1(fanout_id)?
                .context("fanout expiry index points to a missing record")?;
            validate_outbound_fanout_v1(&fanout)?;
            if fanout.expires_at_unix_ms != expires_at {
                bail!("outbound fanout expiry index does not match its record");
            }
            if self.fanout_all_acked_inner_v1(fanout_id, None)? {
                continue;
            }
            let mut terminal_tombstones = Vec::with_capacity(fanout.delivery_ids.len());
            for delivery_id in &fanout.delivery_ids {
                let Some(tombstone) = self.load_tombstone_inner_v1(*delivery_id)? else {
                    terminal_tombstones.clear();
                    break;
                };
                validate_tombstone_v1(&tombstone)?;
                if tombstone.delivery_id != *delivery_id
                    || tombstone.fanout_id != Some(fanout_id)
                    || !matches!(
                        tombstone.terminal_state,
                        ProductDeliveryTerminalStateV1::OutboundRecipientAcked
                            | ProductDeliveryTerminalStateV1::OutboundExpired
                    )
                {
                    bail!("expired fanout contains an invalid terminal delivery binding");
                }
                terminal_tombstones.push(tombstone);
            }
            if terminal_tombstones.len() != fanout.delivery_ids.len() {
                continue;
            }
            let retain_until = now_unix_ms
                .checked_add(self.config.terminal_retention_ms)
                .context("expired fanout retention overflow")?;
            fanout.revision = fanout
                .revision
                .checked_add(1)
                .context("outbound fanout revision overflow")?;
            fanout.state = ProductDeliveryFanoutStateV1::Expired;
            fanout.completion_observed = true;
            fanout.completion_observed_at_unix_ms = Some(now_unix_ms);
            fanout.retain_until_unix_ms = Some(retain_until);
            let mut acknowledged_tombstones = Vec::new();
            for mut tombstone in terminal_tombstones {
                if tombstone.terminal_state
                    != ProductDeliveryTerminalStateV1::OutboundRecipientAcked
                {
                    continue;
                }
                let old_retain_until = tombstone.retain_until_unix_ms;
                tombstone.revision = tombstone
                    .revision
                    .checked_add(1)
                    .context("delivery tombstone revision overflow")?;
                tombstone.completion_observed = true;
                tombstone.retain_until_unix_ms = retain_until;
                validate_tombstone_v1(&tombstone)?;
                acknowledged_tombstones.push((old_retain_until, tombstone));
            }
            expired.push((fanout, acknowledged_tombstones));
        }
        if expired.is_empty() {
            return Ok(0);
        }
        let mut batch = RocksDbWriteBatch::default();
        for (fanout, acknowledged_tombstones) in &expired {
            batch.delete(
                outbound_fanout_expiry_key_v1(fanout.expires_at_unix_ms, &fanout.fanout_id)
                    .as_bytes(),
            );
            batch.put(
                outbound_fanout_retention_key_v1(
                    fanout.retain_until_unix_ms.unwrap_or_default(),
                    &fanout.fanout_id,
                )
                .as_bytes(),
                [],
            );
            put_json_v1(
                &mut batch,
                outbound_fanout_key_v1(&fanout.fanout_id).as_bytes(),
                fanout,
                "expired outbound fanout",
            )?;
            for (old_retain_until, tombstone) in acknowledged_tombstones {
                batch.delete(
                    tombstone_retention_key_v1(*old_retain_until, &tombstone.delivery_id)
                        .as_bytes(),
                );
                batch.put(
                    tombstone_retention_key_v1(
                        tombstone.retain_until_unix_ms,
                        &tombstone.delivery_id,
                    )
                    .as_bytes(),
                    [],
                );
                put_json_v1(
                    &mut batch,
                    tombstone_key_v1(&tombstone.delivery_id).as_bytes(),
                    tombstone,
                    "expired fanout acknowledged tombstone",
                )?;
            }
        }
        write_sync_v1(&self.db, batch).context("persist expired outbound fanouts")?;
        Ok(expired.len())
    }

    fn remove_expired_terminal_records_v1(&self, now_unix_ms: u64) -> Result<(usize, usize)> {
        let mut usage = self.load_usage_inner_v1()?;
        let tombstone_prefix = tombstone_retention_prefix_v1();
        let mut tombstones = Vec::new();
        let mut scanned = 0usize;
        for item in self.db.iterator(RocksDbIteratorMode::From(
            tombstone_prefix.as_bytes(),
            RocksDbDirection::Forward,
        )) {
            let (key, _) = item.context("iterate product delivery tombstone retention index")?;
            if !key.starts_with(tombstone_prefix.as_bytes()) {
                break;
            }
            scanned = checked_scan_increment_v1(scanned)?;
            let (retain_until, delivery_id) = parse_timed_delivery_index_key_v1(
                key.as_ref(),
                tombstone_prefix.as_bytes(),
                "tombstone retention",
            )?;
            if retain_until > now_unix_ms {
                break;
            }
            let tombstone = self
                .load_tombstone_inner_v1(delivery_id)?
                .context("tombstone retention index points to a missing record")?;
            validate_tombstone_v1(&tombstone)?;
            if tombstone.retain_until_unix_ms != retain_until {
                bail!("tombstone retention index does not match its record");
            }
            if tombstone.terminal_state == ProductDeliveryTerminalStateV1::OutboundRecipientAcked
                && !tombstone.completion_observed
            {
                continue;
            }
            tombstones.push(tombstone);
        }

        let fanout_prefix = outbound_fanout_retention_prefix_v1();
        let mut fanouts = Vec::new();
        scanned = 0;
        for item in self.db.iterator(RocksDbIteratorMode::From(
            fanout_prefix.as_bytes(),
            RocksDbDirection::Forward,
        )) {
            let (key, _) = item.context("iterate product delivery fanout retention index")?;
            if !key.starts_with(fanout_prefix.as_bytes()) {
                break;
            }
            scanned = checked_scan_increment_v1(scanned)?;
            let (retain_until, fanout_id) = parse_timed_delivery_index_key_v1(
                key.as_ref(),
                fanout_prefix.as_bytes(),
                "fanout retention",
            )?;
            if retain_until > now_unix_ms {
                break;
            }
            let fanout = self
                .load_outbound_fanout_inner_v1(fanout_id)?
                .context("fanout retention index points to a missing record")?;
            validate_outbound_fanout_v1(&fanout)?;
            if fanout.retain_until_unix_ms != Some(retain_until) || !fanout.completion_observed {
                bail!("fanout retention index does not match a terminal fanout");
            }
            fanouts.push(fanout);
        }

        if tombstones.is_empty() && fanouts.is_empty() {
            return Ok((0, 0));
        }
        let removed = tombstones
            .len()
            .checked_add(fanouts.len())
            .context("terminal cleanup count overflow")?;
        usage.entries = usage
            .entries
            .checked_sub(u64::try_from(removed).context("cleanup count exceeds u64")?)
            .context("product delivery usage entry count underflow")?;
        let mut batch = RocksDbWriteBatch::default();
        for tombstone in &tombstones {
            batch.delete(tombstone_key_v1(&tombstone.delivery_id).as_bytes());
            batch.delete(
                tombstone_retention_key_v1(tombstone.retain_until_unix_ms, &tombstone.delivery_id)
                    .as_bytes(),
            );
        }
        for fanout in &fanouts {
            batch.delete(outbound_fanout_key_v1(&fanout.fanout_id).as_bytes());
            batch.delete(
                outbound_fanout_retention_key_v1(
                    fanout.retain_until_unix_ms.unwrap_or_default(),
                    &fanout.fanout_id,
                )
                .as_bytes(),
            );
        }
        put_json_v1(&mut batch, KEY_USAGE_V1, &usage, "usage")?;
        write_sync_v1(&self.db, batch).context("remove retained product delivery terminals")?;
        Ok((tombstones.len(), fanouts.len()))
    }
}

/// Stable per-recipient identity. Session sequence numbers are deliberately absent.
#[must_use]
pub fn product_delivery_id_v1(
    chain_id: u64,
    payload_class: &str,
    object_hash: [u8; 32],
    payload_sha256: [u8; 32],
    original_sender_peer_id: &str,
    recipient_peer_id: &str,
) -> [u8; 32] {
    hash_parts_v1(
        DELIVERY_ID_DOMAIN_V1,
        &[
            chain_id.to_be_bytes().as_slice(),
            payload_class.as_bytes(),
            object_hash.as_slice(),
            payload_sha256.as_slice(),
            original_sender_peer_id.as_bytes(),
            recipient_peer_id.as_bytes(),
        ],
    )
}

#[must_use]
pub fn product_delivery_payload_sha256_v1(payload: &[u8]) -> [u8; 32] {
    Sha256::digest(payload).into()
}

impl ProductDeliveryJournalV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_outbound_fanout(
        &self,
        payload_class: &str,
        object_hash: [u8; 32],
        payload: &[u8],
        original_sender_peer_id: &str,
        recipient_peer_ids: &[String],
        now_unix_ms: u64,
    ) -> Result<ProductDeliveryFanoutPrepareResultV1> {
        validate_payload_binding_v1(
            self.scope.chain_id,
            payload_class,
            object_hash,
            payload,
            original_sender_peer_id,
        )?;
        if original_sender_peer_id != self.scope.local_peer_id {
            bail!("outbound product delivery sender is not the journal owner");
        }
        let recipients = canonical_recipient_set_v1(
            recipient_peer_ids,
            original_sender_peer_id,
            self.config.max_entries,
        )?;
        let payload_sha256 = product_delivery_payload_sha256_v1(payload);
        let fanout_id = product_delivery_fanout_id_v1(
            self.scope.chain_id,
            payload_class,
            object_hash,
            payload_sha256,
            original_sender_peer_id,
            recipients.as_slice(),
        );
        let delivery_ids = recipients
            .iter()
            .map(|recipient| {
                product_delivery_id_v1(
                    self.scope.chain_id,
                    payload_class,
                    object_hash,
                    payload_sha256,
                    original_sender_peer_id,
                    recipient,
                )
            })
            .collect::<Vec<_>>();
        let expires_at_unix_ms = now_unix_ms
            .checked_add(self.config.obligation_ttl_ms)
            .context("product delivery outbound expiry overflow")?;
        let requested_fanout = ProductDeliveryOutboundFanoutV1 {
            schema: OUTBOUND_FANOUT_SCHEMA_V1.to_string(),
            revision: 1,
            fanout_id,
            chain_id: self.scope.chain_id,
            payload_class: payload_class.to_string(),
            object_hash,
            payload_sha256,
            original_sender_peer_id: original_sender_peer_id.to_string(),
            recipient_peer_ids: recipients.clone(),
            delivery_ids: delivery_ids.clone(),
            created_at_unix_ms: now_unix_ms,
            expires_at_unix_ms,
            state: ProductDeliveryFanoutStateV1::Active,
            all_acked_at_unix_ms: None,
            completion_claimed_at_unix_ms: None,
            completion_observed: false,
            completion_observed_at_unix_ms: None,
            retain_until_unix_ms: None,
        };
        validate_outbound_fanout_v1(&requested_fanout)?;

        let cleanup = self.cleanup_expired(now_unix_ms)?;
        let _guard = self.lock_writes_v1();
        self.ensure_schema_and_scope_v1()?;
        let mut usage = self.load_usage_inner_v1()?;
        let existing_fanout = self.load_outbound_fanout_inner_v1(fanout_id)?;
        if let Some(existing) = existing_fanout.as_ref() {
            validate_outbound_fanout_v1(existing)?;
            ensure_same_fanout_binding_v1(existing, &requested_fanout)?;
        }

        let mut prepared = Vec::with_capacity(recipients.len());
        let mut inserted = Vec::new();
        for (recipient_peer_id, delivery_id) in recipients.iter().zip(delivery_ids.iter()) {
            if let Some(existing) = self.load_outbound_inner_v1(*delivery_id)? {
                validate_outbound_record_v1(&existing)?;
                ensure_same_outbound_binding_v1(
                    &existing,
                    fanout_id,
                    self.scope.chain_id,
                    payload_class,
                    object_hash,
                    payload_sha256,
                    original_sender_peer_id,
                    recipient_peer_id,
                    payload,
                )?;
                prepared.push(ProductDeliveryPreparedRecipientV1 {
                    delivery_id: *delivery_id,
                    recipient_peer_id: recipient_peer_id.clone(),
                    disposition: ProductDeliveryPrepareDispositionV1::ExistingActive,
                });
                continue;
            }
            if let Some(tombstone) = self.load_tombstone_inner_v1(*delivery_id)? {
                validate_tombstone_v1(&tombstone)?;
                ensure_same_tombstone_binding_v1(
                    &tombstone,
                    Some(fanout_id),
                    self.scope.chain_id,
                    payload_class,
                    object_hash,
                    payload_sha256,
                    original_sender_peer_id,
                    recipient_peer_id,
                    payload.len(),
                )?;
                let disposition = match tombstone.terminal_state {
                    ProductDeliveryTerminalStateV1::OutboundRecipientAcked => {
                        ProductDeliveryPrepareDispositionV1::ExistingRecipientAcked
                    }
                    ProductDeliveryTerminalStateV1::OutboundExpired => {
                        ProductDeliveryPrepareDispositionV1::ExistingExpired
                    }
                    ProductDeliveryTerminalStateV1::InboundCompleted
                    | ProductDeliveryTerminalStateV1::InboundPreparedExpired => {
                        bail!("outbound delivery id collides with an inbound tombstone")
                    }
                };
                prepared.push(ProductDeliveryPreparedRecipientV1 {
                    delivery_id: *delivery_id,
                    recipient_peer_id: recipient_peer_id.clone(),
                    disposition,
                });
                continue;
            }

            let record = ProductDeliveryOutboundRecordV1 {
                schema: OUTBOUND_SCHEMA_V1.to_string(),
                revision: 1,
                delivery_id: *delivery_id,
                fanout_id,
                chain_id: self.scope.chain_id,
                payload_class: payload_class.to_string(),
                object_hash,
                payload_sha256,
                original_sender_peer_id: original_sender_peer_id.to_string(),
                recipient_peer_id: recipient_peer_id.clone(),
                payload: payload.to_vec(),
                created_at_unix_ms: now_unix_ms,
                expires_at_unix_ms,
                state: ProductDeliveryOutboundStateV1::Pending,
                attempt_count: 0,
                last_attempt_at_unix_ms: None,
                next_attempt_at_unix_ms: now_unix_ms,
                relay_admitted_at_unix_ms: None,
            };
            validate_outbound_record_v1(&record)?;
            prepared.push(ProductDeliveryPreparedRecipientV1 {
                delivery_id: *delivery_id,
                recipient_peer_id: recipient_peer_id.clone(),
                disposition: ProductDeliveryPrepareDispositionV1::Inserted,
            });
            inserted.push(record);
        }

        let new_fanout_count = usize::from(existing_fanout.is_none());
        let new_entry_count = new_fanout_count
            .checked_add(inserted.len())
            .context("product delivery fanout entry count overflow")?;
        let new_payload_bytes = payload
            .len()
            .checked_mul(inserted.len())
            .context("product delivery fanout byte count overflow")?;
        self.ensure_capacity_v1(&usage, new_entry_count, new_payload_bytes)?;

        if new_entry_count > 0 {
            let mut batch = RocksDbWriteBatch::default();
            if existing_fanout.is_none() {
                put_json_v1(
                    &mut batch,
                    outbound_fanout_key_v1(&fanout_id).as_bytes(),
                    &requested_fanout,
                    "outbound fanout",
                )?;
                batch.put(
                    outbound_fanout_expiry_key_v1(expires_at_unix_ms, &fanout_id).as_bytes(),
                    [],
                );
            }
            for record in &inserted {
                put_json_v1(
                    &mut batch,
                    outbound_key_v1(&record.delivery_id).as_bytes(),
                    record,
                    "outbound record",
                )?;
                batch.put(
                    outbound_due_key_v1(record.next_attempt_at_unix_ms, &record.delivery_id)
                        .as_bytes(),
                    [],
                );
                batch.put(
                    outbound_expiry_key_v1(record.expires_at_unix_ms, &record.delivery_id)
                        .as_bytes(),
                    [],
                );
            }
            usage.entries = usage
                .entries
                .checked_add(u64::try_from(new_entry_count).context("entry count exceeds u64")?)
                .context("product delivery usage entry count overflow")?;
            usage.payload_bytes = usage
                .payload_bytes
                .checked_add(u64::try_from(new_payload_bytes).context("payload bytes exceed u64")?)
                .context("product delivery usage payload bytes overflow")?;
            put_json_v1(&mut batch, KEY_USAGE_V1, &usage, "usage")?;
            write_sync_v1(&self.db, batch).context("persist product delivery outbound fanout")?;
        }

        let fanout = self
            .load_outbound_fanout_inner_v1(fanout_id)?
            .context("product delivery outbound fanout readback is missing")?;
        validate_outbound_fanout_v1(&fanout)?;
        Ok(ProductDeliveryFanoutPrepareResultV1 {
            fanout,
            recipients: prepared,
            inserted_count: inserted.len(),
            cleanup,
        })
    }

    pub fn load_due_outbound(
        &self,
        now_unix_ms: u64,
        limit: usize,
    ) -> Result<Vec<ProductDeliveryOutboundRecordV1>> {
        self.ensure_schema_and_scope_v1()?;
        validate_scan_limit_v1(limit, self.config.max_entries)?;
        let prefix = outbound_due_prefix_v1();
        let mut records = Vec::new();
        let mut scanned = 0usize;
        for item in self.db.iterator(RocksDbIteratorMode::From(
            prefix.as_bytes(),
            RocksDbDirection::Forward,
        )) {
            let (key, _) = item.context("iterate product delivery outbound due index")?;
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }
            scanned = checked_scan_increment_v1(scanned)?;
            let (due_at, delivery_id) =
                parse_timed_delivery_index_key_v1(key.as_ref(), prefix.as_bytes(), "outbound due")?;
            if due_at > now_unix_ms {
                break;
            }
            let record = self
                .load_outbound_inner_v1(delivery_id)?
                .context("outbound due index points to a missing record")?;
            validate_outbound_record_v1(&record)?;
            if record.next_attempt_at_unix_ms != due_at {
                bail!("outbound due index does not match its record");
            }
            records.push(record);
            if records.len() == limit {
                break;
            }
        }
        Ok(records)
    }

    pub fn mark_outbound_attempt(
        &self,
        delivery_id: [u8; 32],
        now_unix_ms: u64,
    ) -> Result<ProductDeliveryOutboundRecordV1> {
        let _guard = self.lock_writes_v1();
        self.ensure_schema_and_scope_v1()?;
        let mut record = self
            .load_outbound_inner_v1(delivery_id)?
            .context("cannot mark an unknown outbound delivery attempt")?;
        validate_outbound_record_v1(&record)?;
        if now_unix_ms >= record.expires_at_unix_ms {
            bail!("cannot mark an attempt for an expired outbound delivery");
        }
        let old_due = record.next_attempt_at_unix_ms;
        record.revision = record
            .revision
            .checked_add(1)
            .context("outbound delivery revision overflow")?;
        record.attempt_count = record
            .attempt_count
            .checked_add(1)
            .context("outbound delivery attempt count overflow")?;
        record.last_attempt_at_unix_ms = Some(now_unix_ms);
        record.next_attempt_at_unix_ms = now_unix_ms
            .checked_add(self.config.retry_interval_ms)
            .context("outbound delivery retry time overflow")?;
        let mut batch = RocksDbWriteBatch::default();
        batch.delete(outbound_due_key_v1(old_due, &delivery_id).as_bytes());
        batch.put(
            outbound_due_key_v1(record.next_attempt_at_unix_ms, &delivery_id).as_bytes(),
            [],
        );
        put_json_v1(
            &mut batch,
            outbound_key_v1(&delivery_id).as_bytes(),
            &record,
            "outbound attempt",
        )?;
        write_sync_v1(&self.db, batch).context("persist outbound delivery attempt")?;
        self.readback_outbound_v1(&record)
    }

    pub fn mark_outbound_relay_admitted(
        &self,
        delivery_id: [u8; 32],
        now_unix_ms: u64,
    ) -> Result<ProductDeliveryOutboundRecordV1> {
        let _guard = self.lock_writes_v1();
        self.ensure_schema_and_scope_v1()?;
        let mut record = self
            .load_outbound_inner_v1(delivery_id)?
            .context("cannot relay-admit an unknown outbound delivery")?;
        validate_outbound_record_v1(&record)?;
        if now_unix_ms >= record.expires_at_unix_ms {
            bail!("cannot relay-admit an expired outbound delivery");
        }
        record.revision = record
            .revision
            .checked_add(1)
            .context("outbound delivery revision overflow")?;
        record.state = ProductDeliveryOutboundStateV1::RelayAdmitted;
        record.relay_admitted_at_unix_ms = Some(now_unix_ms);
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            outbound_key_v1(&delivery_id).as_bytes(),
            &record,
            "relay-admitted outbound delivery",
        )?;
        write_sync_v1(&self.db, batch).context("persist outbound relay admission")?;
        self.readback_outbound_v1(&record)
    }

    pub fn mark_outbound_recipient_ack(
        &self,
        ack: &ProductDeliveryRecipientAckV1,
        now_unix_ms: u64,
    ) -> Result<ProductDeliveryRecipientAckResultV1> {
        validate_recipient_ack_v1(ack)?;
        if ack.chain_id != self.scope.chain_id
            || ack.original_sender_peer_id != self.scope.local_peer_id
        {
            bail!("recipient ACK does not belong to this product delivery journal scope");
        }
        // Expiry owns the exact boundary. Materialize every obligation whose
        // `expires_at_unix_ms <= now_unix_ms` before an ACK may transition an
        // active record, so a late ACK can never race cleanup into all-acked.
        let cleanup = self.cleanup_expired(now_unix_ms)?;
        let _guard = self.lock_writes_v1();
        self.ensure_schema_and_scope_v1()?;
        if let Some(tombstone) = self.load_tombstone_inner_v1(ack.delivery_id)? {
            validate_tombstone_v1(&tombstone)?;
            ensure_ack_matches_tombstone_v1(ack, &tombstone)?;
            let late_after_expiry =
                tombstone.terminal_state == ProductDeliveryTerminalStateV1::OutboundExpired;
            if !matches!(
                tombstone.terminal_state,
                ProductDeliveryTerminalStateV1::OutboundRecipientAcked
                    | ProductDeliveryTerminalStateV1::OutboundExpired
            ) {
                bail!("recipient ACK collides with an inbound tombstone");
            }
            let fanout_all_acked = match tombstone.fanout_id {
                Some(fanout_id) if !late_after_expiry => {
                    self.fanout_all_acked_inner_v1(fanout_id, None)?
                }
                _ => false,
            };
            return Ok(ProductDeliveryRecipientAckResultV1 {
                tombstone,
                duplicate: true,
                late_after_expiry,
                fanout_all_acked,
                cleanup,
            });
        }
        let record = self
            .load_outbound_inner_v1(ack.delivery_id)?
            .context("recipient ACK refers to an unknown outbound delivery")?;
        validate_outbound_record_v1(&record)?;
        ensure_ack_matches_outbound_v1(ack, &record)?;
        let mut usage = self.load_usage_inner_v1()?;
        usage.payload_bytes = usage
            .payload_bytes
            .checked_sub(u64::try_from(record.payload.len()).context("payload len exceeds u64")?)
            .context("product delivery payload usage underflow")?;
        let retain_until_unix_ms = now_unix_ms
            .checked_add(self.config.terminal_retention_ms)
            .context("recipient ACK tombstone retention overflow")?;
        let mut tombstone = tombstone_from_outbound_v1(
            &record,
            ProductDeliveryTerminalStateV1::OutboundRecipientAcked,
            now_unix_ms,
            retain_until_unix_ms,
        )?;
        tombstone.completion_observed = false;
        let mut fanout = self
            .load_outbound_fanout_inner_v1(record.fanout_id)?
            .context("outbound ACK record is missing its fanout")?;
        validate_outbound_fanout_v1(&fanout)?;
        let fanout_all_acked =
            self.fanout_all_acked_inner_v1(record.fanout_id, Some(record.delivery_id))?;
        if fanout_all_acked && fanout.all_acked_at_unix_ms.is_none() {
            fanout.revision = fanout
                .revision
                .checked_add(1)
                .context("outbound fanout revision overflow")?;
            fanout.all_acked_at_unix_ms = Some(now_unix_ms);
        }

        let mut batch = RocksDbWriteBatch::default();
        batch.delete(outbound_key_v1(&record.delivery_id).as_bytes());
        batch.delete(
            outbound_due_key_v1(record.next_attempt_at_unix_ms, &record.delivery_id).as_bytes(),
        );
        batch.delete(
            outbound_expiry_key_v1(record.expires_at_unix_ms, &record.delivery_id).as_bytes(),
        );
        put_json_v1(
            &mut batch,
            tombstone_key_v1(&record.delivery_id).as_bytes(),
            &tombstone,
            "recipient ACK tombstone",
        )?;
        batch.put(
            tombstone_retention_key_v1(retain_until_unix_ms, &record.delivery_id).as_bytes(),
            [],
        );
        if fanout_all_acked {
            put_json_v1(
                &mut batch,
                outbound_fanout_key_v1(&fanout.fanout_id).as_bytes(),
                &fanout,
                "all-acked outbound fanout",
            )?;
        }
        put_json_v1(&mut batch, KEY_USAGE_V1, &usage, "usage")?;
        write_sync_v1(&self.db, batch).context("persist recipient ACK transition")?;
        let readback = self
            .load_tombstone_inner_v1(record.delivery_id)?
            .context("recipient ACK tombstone readback is missing")?;
        if readback != tombstone {
            bail!("recipient ACK tombstone readback mismatch");
        }
        Ok(ProductDeliveryRecipientAckResultV1 {
            tombstone: readback,
            duplicate: false,
            late_after_expiry: false,
            fanout_all_acked,
            cleanup,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn outbound_all_acked(
        &self,
        payload_class: &str,
        object_hash: [u8; 32],
        payload_sha256: [u8; 32],
        original_sender_peer_id: &str,
        recipient_peer_ids: &[String],
    ) -> Result<bool> {
        validate_payload_class_v1(payload_class)?;
        let recipients = canonical_recipient_set_v1(
            recipient_peer_ids,
            original_sender_peer_id,
            self.config.max_entries,
        )?;
        let fanout_id = product_delivery_fanout_id_v1(
            self.scope.chain_id,
            payload_class,
            object_hash,
            payload_sha256,
            original_sender_peer_id,
            recipients.as_slice(),
        );
        let Some(fanout) = self.load_outbound_fanout_inner_v1(fanout_id)? else {
            return Ok(false);
        };
        validate_outbound_fanout_v1(&fanout)?;
        if fanout.recipient_peer_ids != recipients {
            bail!("outbound all-acked query fanout binding mismatch");
        }
        self.fanout_all_acked_inner_v1(fanout_id, None)
    }

    pub fn claim_outbound_completion(
        &self,
        fanout_id: [u8; 32],
        now_unix_ms: u64,
    ) -> Result<Option<ProductDeliveryCompletionClaimV1>> {
        let _guard = self.lock_writes_v1();
        self.ensure_schema_and_scope_v1()?;
        let mut fanout = self
            .load_outbound_fanout_inner_v1(fanout_id)?
            .context("cannot claim an unknown outbound fanout completion")?;
        validate_outbound_fanout_v1(&fanout)?;
        if fanout.completion_observed {
            return Ok(None);
        }
        if !self.fanout_all_acked_inner_v1(fanout_id, None)? {
            bail!("cannot claim outbound fanout completion before every recipient ACK");
        }
        let newly_claimed = fanout.completion_claimed_at_unix_ms.is_none();
        if newly_claimed {
            fanout.revision = fanout
                .revision
                .checked_add(1)
                .context("outbound fanout revision overflow")?;
            fanout.all_acked_at_unix_ms.get_or_insert(now_unix_ms);
            fanout.completion_claimed_at_unix_ms = Some(now_unix_ms);
            let mut batch = RocksDbWriteBatch::default();
            put_json_v1(
                &mut batch,
                outbound_fanout_key_v1(&fanout_id).as_bytes(),
                &fanout,
                "claimed outbound fanout completion",
            )?;
            write_sync_v1(&self.db, batch).context("persist outbound fanout completion claim")?;
        }
        Ok(Some(ProductDeliveryCompletionClaimV1 {
            fanout,
            newly_claimed,
        }))
    }

    pub fn mark_outbound_completion_observed(
        &self,
        fanout_id: [u8; 32],
        now_unix_ms: u64,
    ) -> Result<bool> {
        let _guard = self.lock_writes_v1();
        self.ensure_schema_and_scope_v1()?;
        let mut fanout = self
            .load_outbound_fanout_inner_v1(fanout_id)?
            .context("cannot observe an unknown outbound fanout completion")?;
        validate_outbound_fanout_v1(&fanout)?;
        if fanout.completion_observed {
            return Ok(false);
        }
        if fanout.completion_claimed_at_unix_ms.is_none()
            || !self.fanout_all_acked_inner_v1(fanout_id, None)?
        {
            bail!("outbound completion must be claimed after all ACKs before observation");
        }
        let retain_until_unix_ms = now_unix_ms
            .checked_add(self.config.terminal_retention_ms)
            .context("outbound completion retention overflow")?;
        fanout.revision = fanout
            .revision
            .checked_add(1)
            .context("outbound fanout revision overflow")?;
        fanout.state = ProductDeliveryFanoutStateV1::Completed;
        fanout.completion_observed = true;
        fanout.completion_observed_at_unix_ms = Some(now_unix_ms);
        fanout.retain_until_unix_ms = Some(retain_until_unix_ms);

        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            outbound_fanout_key_v1(&fanout_id).as_bytes(),
            &fanout,
            "observed outbound fanout completion",
        )?;
        batch.delete(
            outbound_fanout_expiry_key_v1(fanout.expires_at_unix_ms, &fanout_id).as_bytes(),
        );
        batch.put(
            outbound_fanout_retention_key_v1(retain_until_unix_ms, &fanout_id).as_bytes(),
            [],
        );
        for delivery_id in &fanout.delivery_ids {
            let mut tombstone = self
                .load_tombstone_inner_v1(*delivery_id)?
                .context("completed fanout is missing a recipient ACK tombstone")?;
            if tombstone.terminal_state != ProductDeliveryTerminalStateV1::OutboundRecipientAcked {
                bail!("completed fanout contains a non-ACK terminal delivery");
            }
            let old_retain = tombstone.retain_until_unix_ms;
            tombstone.revision = tombstone
                .revision
                .checked_add(1)
                .context("delivery tombstone revision overflow")?;
            tombstone.completion_observed = true;
            tombstone.retain_until_unix_ms = retain_until_unix_ms;
            batch.delete(tombstone_retention_key_v1(old_retain, delivery_id).as_bytes());
            batch.put(
                tombstone_retention_key_v1(retain_until_unix_ms, delivery_id).as_bytes(),
                [],
            );
            put_json_v1(
                &mut batch,
                tombstone_key_v1(delivery_id).as_bytes(),
                &tombstone,
                "completion-observed delivery tombstone",
            )?;
        }
        write_sync_v1(&self.db, batch).context("persist outbound completion observation")?;
        Ok(true)
    }

    pub fn load_unobserved_completed_fanouts(
        &self,
        limit: usize,
    ) -> Result<Vec<ProductDeliveryOutboundFanoutV1>> {
        self.ensure_schema_and_scope_v1()?;
        validate_scan_limit_v1(limit, self.config.max_entries)?;
        let prefix = outbound_fanout_prefix_v1();
        let mut out = Vec::new();
        let mut scanned = 0usize;
        for item in self.db.iterator(RocksDbIteratorMode::From(
            prefix.as_bytes(),
            RocksDbDirection::Forward,
        )) {
            let (key, raw) = item.context("iterate product delivery fanouts")?;
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }
            scanned = checked_scan_increment_v1(scanned)?;
            let fanout: ProductDeliveryOutboundFanoutV1 = serde_json::from_slice(raw.as_ref())
                .context("decode product delivery outbound fanout")?;
            validate_outbound_fanout_v1(&fanout)?;
            if !fanout.completion_observed
                && self.fanout_all_acked_inner_v1(fanout.fanout_id, None)?
            {
                out.push(fanout);
                if out.len() == limit {
                    break;
                }
            }
        }
        out.sort_by_key(|fanout| (fanout.created_at_unix_ms, fanout.fanout_id));
        Ok(out)
    }

    pub fn load_outbound(
        &self,
        delivery_id: [u8; 32],
    ) -> Result<Option<ProductDeliveryOutboundRecordV1>> {
        self.ensure_schema_and_scope_v1()?;
        let record = self.load_outbound_inner_v1(delivery_id)?;
        if let Some(record) = record.as_ref() {
            validate_outbound_record_v1(record)?;
        }
        Ok(record)
    }

    pub fn load_tombstone(
        &self,
        delivery_id: [u8; 32],
    ) -> Result<Option<ProductDeliveryTombstoneV1>> {
        self.ensure_schema_and_scope_v1()?;
        let tombstone = self.load_tombstone_inner_v1(delivery_id)?;
        if let Some(tombstone) = tombstone.as_ref() {
            validate_tombstone_v1(tombstone)?;
        }
        Ok(tombstone)
    }
}

impl ProductDeliveryJournalV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_inbound(
        &self,
        delivery_id: [u8; 32],
        payload_class: &str,
        object_hash: [u8; 32],
        payload: &[u8],
        original_sender_peer_id: &str,
        now_unix_ms: u64,
    ) -> Result<ProductDeliveryInboundPrepareResultV1> {
        validate_payload_binding_v1(
            self.scope.chain_id,
            payload_class,
            object_hash,
            payload,
            original_sender_peer_id,
        )?;
        if original_sender_peer_id == self.scope.local_peer_id {
            bail!("inbound product delivery sender must differ from the journal owner");
        }
        let payload_sha256 = product_delivery_payload_sha256_v1(payload);
        let expected_delivery_id = product_delivery_id_v1(
            self.scope.chain_id,
            payload_class,
            object_hash,
            payload_sha256,
            original_sender_peer_id,
            self.scope.local_peer_id.as_str(),
        );
        if delivery_id != expected_delivery_id {
            bail!("inbound product delivery id equivocation or payload binding mismatch");
        }
        let cleanup = self.cleanup_expired(now_unix_ms)?;
        let _guard = self.lock_writes_v1();
        self.ensure_schema_and_scope_v1()?;

        if let Some(mut existing) = self.load_inbound_inner_v1(delivery_id)? {
            validate_inbound_record_v1(&existing)?;
            ensure_same_inbound_binding_v1(
                &existing,
                self.scope.chain_id,
                payload_class,
                object_hash,
                payload_sha256,
                original_sender_peer_id,
                self.scope.local_peer_id.as_str(),
                payload,
            )?;
            let disposition = match existing.state {
                ProductDeliveryInboundStateV1::Prepared => {
                    ProductDeliveryInboundPrepareDispositionV1::ExistingPrepared
                }
                ProductDeliveryInboundStateV1::Accepted => {
                    if !existing.ack_pending {
                        existing.revision = existing
                            .revision
                            .checked_add(1)
                            .context("inbound delivery revision overflow")?;
                        existing.ack_pending = true;
                        let mut batch = RocksDbWriteBatch::default();
                        put_json_v1(
                            &mut batch,
                            inbound_key_v1(&delivery_id).as_bytes(),
                            &existing,
                            "replayed accepted inbound delivery",
                        )?;
                        write_sync_v1(&self.db, batch)
                            .context("persist replayed inbound ACK intent")?;
                    }
                    ProductDeliveryInboundPrepareDispositionV1::ExistingAccepted
                }
            };
            return Ok(ProductDeliveryInboundPrepareResultV1 {
                delivery_id,
                disposition,
                record: Some(existing),
                tombstone: None,
                cleanup,
            });
        }
        if let Some(tombstone) = self.load_tombstone_inner_v1(delivery_id)? {
            validate_tombstone_v1(&tombstone)?;
            ensure_same_tombstone_binding_v1(
                &tombstone,
                None,
                self.scope.chain_id,
                payload_class,
                object_hash,
                payload_sha256,
                original_sender_peer_id,
                self.scope.local_peer_id.as_str(),
                payload.len(),
            )?;
            let disposition = match tombstone.terminal_state {
                ProductDeliveryTerminalStateV1::InboundCompleted => {
                    ProductDeliveryInboundPrepareDispositionV1::ExistingCompleted
                }
                ProductDeliveryTerminalStateV1::InboundPreparedExpired => {
                    ProductDeliveryInboundPrepareDispositionV1::ExistingExpired
                }
                ProductDeliveryTerminalStateV1::OutboundRecipientAcked
                | ProductDeliveryTerminalStateV1::OutboundExpired => {
                    bail!("inbound delivery id collides with an outbound tombstone")
                }
            };
            return Ok(ProductDeliveryInboundPrepareResultV1 {
                delivery_id,
                disposition,
                record: None,
                tombstone: Some(tombstone),
                cleanup,
            });
        }

        let mut usage = self.load_usage_inner_v1()?;
        self.ensure_capacity_v1(&usage, 1, payload.len())?;
        let prepared_sequence = usage.next_inbound_sequence;
        usage.next_inbound_sequence = usage
            .next_inbound_sequence
            .checked_add(1)
            .context("inbound delivery sequence overflow")?;
        usage.entries = usage
            .entries
            .checked_add(1)
            .context("product delivery usage entry count overflow")?;
        usage.payload_bytes = usage
            .payload_bytes
            .checked_add(u64::try_from(payload.len()).context("payload len exceeds u64")?)
            .context("product delivery usage payload bytes overflow")?;
        let expires_at_unix_ms = now_unix_ms
            .checked_add(self.config.obligation_ttl_ms)
            .context("inbound delivery expiry overflow")?;
        let record = ProductDeliveryInboundRecordV1 {
            schema: INBOUND_SCHEMA_V1.to_string(),
            revision: 1,
            delivery_id,
            chain_id: self.scope.chain_id,
            payload_class: payload_class.to_string(),
            object_hash,
            payload_sha256,
            original_sender_peer_id: original_sender_peer_id.to_string(),
            recipient_peer_id: self.scope.local_peer_id.clone(),
            payload: payload.to_vec(),
            prepared_sequence,
            prepared_at_unix_ms: now_unix_ms,
            expires_at_unix_ms,
            state: ProductDeliveryInboundStateV1::Prepared,
            accepted_at_unix_ms: None,
            ack_pending: false,
            last_ack_emitted_at_unix_ms: None,
        };
        validate_inbound_record_v1(&record)?;
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            inbound_key_v1(&delivery_id).as_bytes(),
            &record,
            "prepared inbound delivery",
        )?;
        batch.put(
            inbound_expiry_key_v1(expires_at_unix_ms, &delivery_id).as_bytes(),
            [],
        );
        put_json_v1(&mut batch, KEY_USAGE_V1, &usage, "usage")?;
        write_sync_v1(&self.db, batch).context("persist prepared inbound delivery")?;
        let readback = self
            .load_inbound_inner_v1(delivery_id)?
            .context("prepared inbound delivery readback is missing")?;
        if readback != record {
            bail!("prepared inbound delivery readback mismatch");
        }
        Ok(ProductDeliveryInboundPrepareResultV1 {
            delivery_id,
            disposition: ProductDeliveryInboundPrepareDispositionV1::Inserted,
            record: Some(readback),
            tombstone: None,
            cleanup,
        })
    }

    pub fn accept_inbound(
        &self,
        delivery_id: [u8; 32],
        now_unix_ms: u64,
    ) -> Result<ProductDeliveryInboundRecordV1> {
        let _guard = self.lock_writes_v1();
        self.ensure_schema_and_scope_v1()?;
        let mut record = self
            .load_inbound_inner_v1(delivery_id)?
            .context("cannot accept an unknown inbound delivery")?;
        validate_inbound_record_v1(&record)?;
        if record.state == ProductDeliveryInboundStateV1::Accepted {
            if record.ack_pending {
                return Ok(record);
            }
            record.revision = record
                .revision
                .checked_add(1)
                .context("inbound delivery revision overflow")?;
            record.ack_pending = true;
        } else {
            if now_unix_ms >= record.expires_at_unix_ms {
                bail!("cannot accept an expired inbound prepared delivery");
            }
            record.revision = record
                .revision
                .checked_add(1)
                .context("inbound delivery revision overflow")?;
            record.state = ProductDeliveryInboundStateV1::Accepted;
            record.accepted_at_unix_ms = Some(now_unix_ms);
            record.ack_pending = true;
        }
        let mut batch = RocksDbWriteBatch::default();
        batch.delete(inbound_expiry_key_v1(record.expires_at_unix_ms, &delivery_id).as_bytes());
        put_json_v1(
            &mut batch,
            inbound_key_v1(&delivery_id).as_bytes(),
            &record,
            "accepted inbound delivery",
        )?;
        write_sync_v1(&self.db, batch).context("persist accepted inbound delivery")?;
        self.readback_inbound_v1(&record)
    }

    pub fn mark_inbound_ack_emitted(
        &self,
        delivery_id: [u8; 32],
        now_unix_ms: u64,
    ) -> Result<ProductDeliveryInboundRecordV1> {
        let _guard = self.lock_writes_v1();
        self.ensure_schema_and_scope_v1()?;
        let mut record = self
            .load_inbound_inner_v1(delivery_id)?
            .context("cannot mark ACK for an unknown inbound delivery")?;
        validate_inbound_record_v1(&record)?;
        if record.state != ProductDeliveryInboundStateV1::Accepted {
            bail!("cannot emit a recipient ACK before durable inbound acceptance");
        }
        if !record.ack_pending {
            return Ok(record);
        }
        record.revision = record
            .revision
            .checked_add(1)
            .context("inbound delivery revision overflow")?;
        record.ack_pending = false;
        record.last_ack_emitted_at_unix_ms = Some(now_unix_ms);
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            inbound_key_v1(&delivery_id).as_bytes(),
            &record,
            "inbound ACK emission",
        )?;
        write_sync_v1(&self.db, batch).context("persist inbound ACK emission")?;
        self.readback_inbound_v1(&record)
    }

    /// Release an accepted inbound payload only after its external durable owner
    /// has completed. The journal does not interpret what that owner is.
    pub fn complete_inbound(
        &self,
        delivery_id: [u8; 32],
        now_unix_ms: u64,
    ) -> Result<ProductDeliveryTombstoneV1> {
        let _guard = self.lock_writes_v1();
        self.ensure_schema_and_scope_v1()?;
        if let Some(tombstone) = self.load_tombstone_inner_v1(delivery_id)? {
            if tombstone.terminal_state == ProductDeliveryTerminalStateV1::InboundCompleted {
                return Ok(tombstone);
            }
            bail!("cannot complete an inbound delivery with a different terminal state");
        }
        let record = self
            .load_inbound_inner_v1(delivery_id)?
            .context("cannot complete an unknown inbound delivery")?;
        validate_inbound_record_v1(&record)?;
        if record.state != ProductDeliveryInboundStateV1::Accepted {
            bail!("cannot complete an inbound delivery before acceptance");
        }
        if record.ack_pending {
            bail!("cannot complete an inbound delivery before recipient ACK relay admission");
        }
        let mut usage = self.load_usage_inner_v1()?;
        usage.payload_bytes = usage
            .payload_bytes
            .checked_sub(u64::try_from(record.payload.len()).context("payload len exceeds u64")?)
            .context("product delivery payload usage underflow")?;
        let retain_until_unix_ms = now_unix_ms
            .checked_add(self.config.terminal_retention_ms)
            .context("inbound tombstone retention overflow")?;
        let tombstone = tombstone_from_inbound_v1(
            &record,
            ProductDeliveryTerminalStateV1::InboundCompleted,
            now_unix_ms,
            retain_until_unix_ms,
        )?;
        let mut batch = RocksDbWriteBatch::default();
        batch.delete(inbound_key_v1(&delivery_id).as_bytes());
        batch.delete(inbound_expiry_key_v1(record.expires_at_unix_ms, &delivery_id).as_bytes());
        put_json_v1(
            &mut batch,
            tombstone_key_v1(&delivery_id).as_bytes(),
            &tombstone,
            "completed inbound tombstone",
        )?;
        batch.put(
            tombstone_retention_key_v1(retain_until_unix_ms, &delivery_id).as_bytes(),
            [],
        );
        put_json_v1(&mut batch, KEY_USAGE_V1, &usage, "usage")?;
        write_sync_v1(&self.db, batch).context("persist completed inbound delivery")?;
        Ok(tombstone)
    }

    pub fn load_inbound_recovery(
        &self,
        limit: usize,
    ) -> Result<Vec<ProductDeliveryInboundRecordV1>> {
        self.ensure_schema_and_scope_v1()?;
        validate_scan_limit_v1(limit, self.config.max_entries)?;
        let prefix = inbound_prefix_v1();
        let mut records = Vec::new();
        let mut scanned = 0usize;
        for item in self.db.iterator(RocksDbIteratorMode::From(
            prefix.as_bytes(),
            RocksDbDirection::Forward,
        )) {
            let (key, raw) = item.context("iterate product delivery inbound records")?;
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }
            scanned = checked_scan_increment_v1(scanned)?;
            let record: ProductDeliveryInboundRecordV1 = serde_json::from_slice(raw.as_ref())
                .context("decode product delivery inbound recovery record")?;
            validate_inbound_record_v1(&record)?;
            records.push(record);
            if records.len() == limit {
                break;
            }
        }
        records.sort_by_key(|record| (record.prepared_sequence, record.delivery_id));
        Ok(records)
    }

    pub fn load_inbound(
        &self,
        delivery_id: [u8; 32],
    ) -> Result<Option<ProductDeliveryInboundRecordV1>> {
        self.ensure_schema_and_scope_v1()?;
        let record = self.load_inbound_inner_v1(delivery_id)?;
        if let Some(record) = record.as_ref() {
            validate_inbound_record_v1(record)?;
        }
        Ok(record)
    }

    pub fn cleanup_expired(&self, now_unix_ms: u64) -> Result<ProductDeliveryCleanupSummaryV1> {
        let _guard = self.lock_writes_v1();
        self.ensure_schema_and_scope_v1()?;
        let mut summary = ProductDeliveryCleanupSummaryV1::default();
        let mut usage = self.load_usage_inner_v1()?;

        let outbound_expired = self.collect_expired_outbound_v1(now_unix_ms)?;
        let inbound_expired = self.collect_expired_inbound_v1(now_unix_ms)?;
        if !outbound_expired.is_empty() || !inbound_expired.is_empty() {
            let mut batch = RocksDbWriteBatch::default();
            for record in &outbound_expired {
                let retain_until_unix_ms = now_unix_ms
                    .checked_add(self.config.terminal_retention_ms)
                    .context("outbound expiry tombstone retention overflow")?;
                let tombstone = tombstone_from_outbound_v1(
                    record,
                    ProductDeliveryTerminalStateV1::OutboundExpired,
                    now_unix_ms,
                    retain_until_unix_ms,
                )?;
                batch.delete(outbound_key_v1(&record.delivery_id).as_bytes());
                batch.delete(
                    outbound_due_key_v1(record.next_attempt_at_unix_ms, &record.delivery_id)
                        .as_bytes(),
                );
                batch.delete(
                    outbound_expiry_key_v1(record.expires_at_unix_ms, &record.delivery_id)
                        .as_bytes(),
                );
                put_json_v1(
                    &mut batch,
                    tombstone_key_v1(&record.delivery_id).as_bytes(),
                    &tombstone,
                    "expired outbound tombstone",
                )?;
                batch.put(
                    tombstone_retention_key_v1(retain_until_unix_ms, &record.delivery_id)
                        .as_bytes(),
                    [],
                );
                usage.payload_bytes = usage
                    .payload_bytes
                    .checked_sub(
                        u64::try_from(record.payload.len()).context("payload len exceeds u64")?,
                    )
                    .context("product delivery payload usage underflow")?;
            }
            for record in &inbound_expired {
                let retain_until_unix_ms = now_unix_ms
                    .checked_add(self.config.terminal_retention_ms)
                    .context("inbound expiry tombstone retention overflow")?;
                let tombstone = tombstone_from_inbound_v1(
                    record,
                    ProductDeliveryTerminalStateV1::InboundPreparedExpired,
                    now_unix_ms,
                    retain_until_unix_ms,
                )?;
                batch.delete(inbound_key_v1(&record.delivery_id).as_bytes());
                batch.delete(
                    inbound_expiry_key_v1(record.expires_at_unix_ms, &record.delivery_id)
                        .as_bytes(),
                );
                put_json_v1(
                    &mut batch,
                    tombstone_key_v1(&record.delivery_id).as_bytes(),
                    &tombstone,
                    "expired inbound tombstone",
                )?;
                batch.put(
                    tombstone_retention_key_v1(retain_until_unix_ms, &record.delivery_id)
                        .as_bytes(),
                    [],
                );
                usage.payload_bytes = usage
                    .payload_bytes
                    .checked_sub(
                        u64::try_from(record.payload.len()).context("payload len exceeds u64")?,
                    )
                    .context("product delivery payload usage underflow")?;
            }
            put_json_v1(&mut batch, KEY_USAGE_V1, &usage, "usage")?;
            write_sync_v1(&self.db, batch).context("persist expired product deliveries")?;
        }
        summary.outbound_expired = outbound_expired.len();
        summary.inbound_prepared_expired = inbound_expired.len();

        summary.fanouts_expired = self.expire_terminal_fanouts_v1(now_unix_ms)?;
        let (tombstones_removed, fanouts_removed) =
            self.remove_expired_terminal_records_v1(now_unix_ms)?;
        summary.tombstones_removed = tombstones_removed;
        summary.fanouts_removed = fanouts_removed;
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_PATH_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    struct TestJournalDir(PathBuf);

    impl TestJournalDir {
        fn new(label: &str) -> Self {
            let sequence = TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            Self(std::env::temp_dir().join(format!(
                "novovm-product-delivery-journal-{label}-{}-{sequence}",
                std::process::id()
            )))
        }
    }

    impl Drop for TestJournalDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn config(path: PathBuf) -> ProductDeliveryJournalConfigV1 {
        ProductDeliveryJournalConfigV1 {
            path,
            max_entries: 64,
            max_bytes: 64 * 1024,
            obligation_ttl_ms: 10_000,
            terminal_retention_ms: 10_000,
            retry_interval_ms: 100,
        }
    }

    fn scope(peer: &str) -> ProductDeliveryJournalScopeV1 {
        ProductDeliveryJournalScopeV1 {
            chain_id: 77,
            local_peer_id: peer.to_string(),
        }
    }

    fn recipient_ack(
        prepared: &ProductDeliveryFanoutPrepareResultV1,
        recipient_peer_id: &str,
    ) -> ProductDeliveryRecipientAckV1 {
        let recipient = prepared
            .recipients
            .iter()
            .find(|recipient| recipient.recipient_peer_id == recipient_peer_id)
            .expect("prepared recipient exists");
        ProductDeliveryRecipientAckV1 {
            delivery_id: recipient.delivery_id,
            chain_id: prepared.fanout.chain_id,
            payload_class: prepared.fanout.payload_class.clone(),
            object_hash: prepared.fanout.object_hash,
            payload_sha256: prepared.fanout.payload_sha256,
            original_sender_peer_id: prepared.fanout.original_sender_peer_id.clone(),
            recipient_peer_id: recipient_peer_id.to_string(),
        }
    }

    #[test]
    fn reopen_recovers_outbound_and_inbound_obligations() -> Result<()> {
        let test_dir = TestJournalDir::new("reopen");
        let journal_config = config(test_dir.0.clone());
        let journal_scope = scope("peer-a");
        let journal =
            ProductDeliveryJournalV1::open(journal_config.clone(), journal_scope.clone(), 100)?;

        let outbound = journal.prepare_outbound_fanout(
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x11; 32],
            b"outbound-payload",
            "peer-a",
            &["peer-b".to_string()],
            100,
        )?;
        let inbound_payload = b"inbound-payload";
        let inbound_delivery_id = product_delivery_id_v1(
            77,
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_SEAL_V1,
            [0x22; 32],
            product_delivery_payload_sha256_v1(inbound_payload),
            "peer-c",
            "peer-a",
        );
        journal.prepare_inbound(
            inbound_delivery_id,
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_SEAL_V1,
            [0x22; 32],
            inbound_payload,
            "peer-c",
            100,
        )?;
        journal.accept_inbound(inbound_delivery_id, 101)?;
        drop(journal);

        let reopened = ProductDeliveryJournalV1::open(journal_config, journal_scope, 102)?;
        let due = reopened.load_due_outbound(102, 10)?;
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].delivery_id, outbound.recipients[0].delivery_id);
        let recovered = reopened.load_inbound_recovery(10)?;
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].delivery_id, inbound_delivery_id);
        assert_eq!(recovered[0].state, ProductDeliveryInboundStateV1::Accepted);
        assert!(recovered[0].ack_pending);
        Ok(())
    }

    #[test]
    fn fanout_prepare_is_atomic_at_entry_and_byte_capacity() -> Result<()> {
        let entry_dir = TestJournalDir::new("entry-capacity");
        let mut entry_config = config(entry_dir.0.clone());
        entry_config.max_entries = 2;
        let entry_journal = ProductDeliveryJournalV1::open(entry_config, scope("peer-a"), 100)?;
        let error = entry_journal
            .prepare_outbound_fanout(
                PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
                [0x31; 32],
                b"payload",
                "peer-a",
                &["peer-b".to_string(), "peer-c".to_string()],
                100,
            )
            .expect_err("fanout plus two recipients exceeds two entries");
        assert!(error.to_string().contains("capacity exceeded"));
        assert_eq!(entry_journal.usage()?.entries, 0);
        assert!(entry_journal.load_due_outbound(100, 2)?.is_empty());

        let byte_dir = TestJournalDir::new("byte-capacity");
        let mut byte_config = config(byte_dir.0.clone());
        byte_config.max_bytes = 3;
        let byte_journal = ProductDeliveryJournalV1::open(byte_config, scope("peer-a"), 100)?;
        byte_journal
            .prepare_outbound_fanout(
                PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
                [0x32; 32],
                b"ab",
                "peer-a",
                &["peer-b".to_string(), "peer-c".to_string()],
                100,
            )
            .expect_err("two retained payload copies exceed byte capacity");
        assert_eq!(byte_journal.usage()?.entries, 0);
        assert_eq!(byte_journal.usage()?.payload_bytes, 0);
        Ok(())
    }

    #[test]
    fn recipient_acks_are_per_peer_and_completion_recovers() -> Result<()> {
        let test_dir = TestJournalDir::new("recipient-acks");
        let journal_config = config(test_dir.0.clone());
        let journal_scope = scope("peer-a");
        let journal =
            ProductDeliveryJournalV1::open(journal_config.clone(), journal_scope.clone(), 100)?;
        let recipients = vec!["peer-b".to_string(), "peer-c".to_string()];
        let prepared = journal.prepare_outbound_fanout(
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_SEAL_V1,
            [0x41; 32],
            b"sealed-payload",
            "peer-a",
            recipients.as_slice(),
            100,
        )?;

        let first =
            journal.mark_outbound_recipient_ack(&recipient_ack(&prepared, "peer-b"), 110)?;
        assert!(!first.fanout_all_acked);
        assert!(!journal.outbound_all_acked(
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_SEAL_V1,
            [0x41; 32],
            prepared.fanout.payload_sha256,
            "peer-a",
            recipients.as_slice(),
        )?);
        let due = journal.load_due_outbound(110, 10)?;
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].recipient_peer_id, "peer-c");

        let second =
            journal.mark_outbound_recipient_ack(&recipient_ack(&prepared, "peer-c"), 111)?;
        assert!(second.fanout_all_acked);
        assert!(journal.outbound_all_acked(
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_SEAL_V1,
            [0x41; 32],
            prepared.fanout.payload_sha256,
            "peer-a",
            recipients.as_slice(),
        )?);
        assert!(journal.load_due_outbound(111, 10)?.is_empty());
        assert_eq!(journal.load_unobserved_completed_fanouts(10)?.len(), 1);
        let claim = journal
            .claim_outbound_completion(prepared.fanout.fanout_id, 112)?
            .expect("all-acked completion is claimable");
        assert!(claim.newly_claimed);
        drop(journal);

        let reopened = ProductDeliveryJournalV1::open(journal_config, journal_scope, 113)?;
        let recovered_claim = reopened
            .claim_outbound_completion(prepared.fanout.fanout_id, 113)?
            .expect("unobserved completion remains claimable after restart");
        assert!(!recovered_claim.newly_claimed);
        assert!(reopened.mark_outbound_completion_observed(prepared.fanout.fanout_id, 114)?);
        assert!(reopened.load_unobserved_completed_fanouts(10)?.is_empty());
        Ok(())
    }

    #[test]
    fn recipient_ack_at_exact_expiry_is_late_and_cannot_complete_fanout() -> Result<()> {
        let test_dir = TestJournalDir::new("ack-at-expiry");
        let mut journal_config = config(test_dir.0.clone());
        journal_config.obligation_ttl_ms = 10;
        journal_config.terminal_retention_ms = 5;
        let journal = ProductDeliveryJournalV1::open(journal_config, scope("peer-a"), 100)?;
        let recipients = vec!["peer-b".to_string()];
        let prepared = journal.prepare_outbound_fanout(
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x49; 32],
            b"expiry-boundary-payload",
            "peer-a",
            recipients.as_slice(),
            100,
        )?;

        let result =
            journal.mark_outbound_recipient_ack(&recipient_ack(&prepared, "peer-b"), 110)?;
        assert_eq!(result.cleanup.outbound_expired, 1);
        assert_eq!(result.cleanup.fanouts_expired, 1);
        assert!(result.duplicate);
        assert!(result.late_after_expiry);
        assert!(!result.fanout_all_acked);
        assert_eq!(
            result.tombstone.terminal_state,
            ProductDeliveryTerminalStateV1::OutboundExpired
        );
        assert!(journal
            .load_outbound(prepared.recipients[0].delivery_id)?
            .is_none());
        assert!(!journal.outbound_all_acked(
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x49; 32],
            prepared.fanout.payload_sha256,
            "peer-a",
            recipients.as_slice(),
        )?);
        let fanout = journal
            .load_outbound_fanout_inner_v1(prepared.fanout.fanout_id)?
            .context("expired fanout remains retained")?;
        assert_eq!(fanout.state, ProductDeliveryFanoutStateV1::Expired);
        assert!(fanout.completion_observed);
        assert!(journal.load_unobserved_completed_fanouts(10)?.is_empty());
        assert!(journal
            .claim_outbound_completion(prepared.fanout.fanout_id, 110)?
            .is_none());
        Ok(())
    }

    #[test]
    fn prepare_results_report_their_internal_expiry_cleanup() -> Result<()> {
        let test_dir = TestJournalDir::new("prepare-cleanup-summary");
        let mut journal_config = config(test_dir.0.clone());
        journal_config.obligation_ttl_ms = 10;
        journal_config.terminal_retention_ms = 100;
        let journal = ProductDeliveryJournalV1::open(journal_config, scope("peer-a"), 100)?;
        let first = journal.prepare_outbound_fanout(
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x4a; 32],
            b"first-outbound",
            "peer-a",
            &["peer-b".to_string()],
            100,
        )?;
        assert_eq!(first.cleanup, ProductDeliveryCleanupSummaryV1::default());

        let inbound_payload = b"cleanup-trigger-inbound";
        let inbound_delivery_id = product_delivery_id_v1(
            77,
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x4b; 32],
            product_delivery_payload_sha256_v1(inbound_payload),
            "peer-c",
            "peer-a",
        );
        let inbound = journal.prepare_inbound(
            inbound_delivery_id,
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x4b; 32],
            inbound_payload,
            "peer-c",
            110,
        )?;
        assert_eq!(inbound.cleanup.outbound_expired, 1);
        assert_eq!(inbound.cleanup.fanouts_expired, 1);

        journal.prepare_outbound_fanout(
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x4c; 32],
            b"second-outbound",
            "peer-a",
            &["peer-b".to_string()],
            111,
        )?;
        let outbound = journal.prepare_outbound_fanout(
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x4d; 32],
            b"third-outbound",
            "peer-a",
            &["peer-b".to_string()],
            121,
        )?;
        assert_eq!(outbound.cleanup.outbound_expired, 1);
        assert_eq!(outbound.cleanup.inbound_prepared_expired, 1);
        assert_eq!(outbound.cleanup.fanouts_expired, 1);
        Ok(())
    }

    #[test]
    fn inbound_completion_requires_recipient_ack_relay_admission() -> Result<()> {
        let test_dir = TestJournalDir::new("complete-after-ack-admission");
        let journal =
            ProductDeliveryJournalV1::open(config(test_dir.0.clone()), scope("peer-b"), 100)?;
        let payload = b"accepted-inbound";
        let delivery_id = product_delivery_id_v1(
            77,
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x4e; 32],
            product_delivery_payload_sha256_v1(payload),
            "peer-a",
            "peer-b",
        );
        journal.prepare_inbound(
            delivery_id,
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x4e; 32],
            payload,
            "peer-a",
            100,
        )?;
        journal.accept_inbound(delivery_id, 101)?;
        let error = journal
            .complete_inbound(delivery_id, 102)
            .expect_err("ACK-pending inbound payload must remain durable");
        assert!(error.to_string().contains("ACK relay admission"));
        assert!(journal
            .load_inbound(delivery_id)?
            .is_some_and(|record| record.ack_pending));

        journal.mark_inbound_ack_emitted(delivery_id, 103)?;
        let tombstone = journal.complete_inbound(delivery_id, 104)?;
        assert_eq!(
            tombstone.terminal_state,
            ProductDeliveryTerminalStateV1::InboundCompleted
        );
        assert!(journal.load_inbound(delivery_id)?.is_none());
        Ok(())
    }

    #[test]
    fn prepared_inbound_duplicate_is_idempotent_and_equivocation_fails_closed() -> Result<()> {
        let test_dir = TestJournalDir::new("inbound-duplicate");
        let journal =
            ProductDeliveryJournalV1::open(config(test_dir.0.clone()), scope("peer-b"), 100)?;
        let payload = b"exact-payload";
        let delivery_id = product_delivery_id_v1(
            77,
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x51; 32],
            product_delivery_payload_sha256_v1(payload),
            "peer-a",
            "peer-b",
        );

        let inserted = journal.prepare_inbound(
            delivery_id,
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x51; 32],
            payload,
            "peer-a",
            100,
        )?;
        assert_eq!(
            inserted.disposition,
            ProductDeliveryInboundPrepareDispositionV1::Inserted
        );
        let duplicate = journal.prepare_inbound(
            delivery_id,
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x51; 32],
            payload,
            "peer-a",
            101,
        )?;
        assert_eq!(
            duplicate.disposition,
            ProductDeliveryInboundPrepareDispositionV1::ExistingPrepared
        );
        journal.accept_inbound(delivery_id, 102)?;
        journal.mark_inbound_ack_emitted(delivery_id, 103)?;
        let accepted_duplicate = journal.prepare_inbound(
            delivery_id,
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x51; 32],
            payload,
            "peer-a",
            104,
        )?;
        assert_eq!(
            accepted_duplicate.disposition,
            ProductDeliveryInboundPrepareDispositionV1::ExistingAccepted
        );
        assert!(
            accepted_duplicate
                .record
                .expect("accepted record")
                .ack_pending
        );

        let error = journal
            .prepare_inbound(
                delivery_id,
                PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
                [0x51; 32],
                b"different-payload",
                "peer-a",
                105,
            )
            .expect_err("same delivery id cannot bind a different payload digest");
        assert!(error.to_string().contains("binding mismatch"));
        Ok(())
    }

    #[test]
    fn partial_ack_fanout_expiry_releases_capacity_after_restart() -> Result<()> {
        let test_dir = TestJournalDir::new("partial-ack-expiry");
        let mut journal_config = config(test_dir.0.clone());
        journal_config.max_entries = 3;
        journal_config.obligation_ttl_ms = 10;
        journal_config.terminal_retention_ms = 5;
        let journal_scope = scope("peer-a");
        let journal =
            ProductDeliveryJournalV1::open(journal_config.clone(), journal_scope.clone(), 100)?;
        let recipients = vec!["peer-b".to_string(), "peer-c".to_string()];
        let prepared = journal.prepare_outbound_fanout(
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x61; 32],
            b"first-payload",
            "peer-a",
            recipients.as_slice(),
            100,
        )?;
        let acknowledged_delivery_id = prepared
            .recipients
            .iter()
            .find(|recipient| recipient.recipient_peer_id == "peer-b")
            .expect("peer-b delivery exists")
            .delivery_id;
        let unacknowledged_delivery_id = prepared
            .recipients
            .iter()
            .find(|recipient| recipient.recipient_peer_id == "peer-c")
            .expect("peer-c delivery exists")
            .delivery_id;
        journal.mark_outbound_recipient_ack(&recipient_ack(&prepared, "peer-b"), 101)?;
        let relay_error = journal
            .mark_outbound_relay_admitted(unacknowledged_delivery_id, 110)
            .expect_err("an expired obligation cannot become relay-admitted");
        assert!(relay_error.to_string().contains("expired"));
        assert_eq!(journal.usage()?.entries, 3);
        drop(journal);

        let reopened = ProductDeliveryJournalV1::open(journal_config, journal_scope, 110)?;
        let acknowledged_tombstone = reopened
            .load_tombstone(acknowledged_delivery_id)?
            .context("acknowledged tombstone must survive fanout expiry retention")?;
        assert!(acknowledged_tombstone.completion_observed);
        assert_eq!(acknowledged_tombstone.retain_until_unix_ms, 115);
        assert_eq!(reopened.usage()?.entries, 3);
        assert_eq!(reopened.usage()?.payload_bytes, 0);

        let cleanup = reopened.cleanup_expired(115)?;
        assert_eq!(cleanup.tombstones_removed, 2);
        assert_eq!(cleanup.fanouts_removed, 1);
        assert_eq!(reopened.usage()?.entries, 0);
        let replacement = reopened.prepare_outbound_fanout(
            PRODUCT_DELIVERY_PAYLOAD_CLASS_NATIVE_TRANSACTION_V1,
            [0x62; 32],
            b"next-payload",
            "peer-a",
            recipients.as_slice(),
            116,
        )?;
        assert_eq!(replacement.inserted_count, 2);
        assert_eq!(reopened.usage()?.entries, 3);
        Ok(())
    }

    #[test]
    fn reopening_with_a_different_scope_is_rejected() -> Result<()> {
        let test_dir = TestJournalDir::new("scope-mismatch");
        let journal_config = config(test_dir.0.clone());
        let journal = ProductDeliveryJournalV1::open(journal_config.clone(), scope("peer-a"), 100)?;
        drop(journal);
        let error = ProductDeliveryJournalV1::open(journal_config, scope("peer-b"), 101)
            .err()
            .context("different local peer scope must be rejected")?;
        assert!(error.to_string().contains("scope mismatch"));
        Ok(())
    }
}
