//! Bounded, process-local opaque rendezvous records. No network or file IO.
//!
//! The caller authenticates `owner`, authorizes access before calling, supplies
//! fresh unguessable 32-byte slot capabilities and a fresh service instance, and
//! schedules expiry checks or drops the store. A record is opaque ciphertext:
//! neither ownership nor a receipt proves the enclosed address/identity valid.
//! No record, slot capability, owner, digest or instance is exposed by Debug.
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use thiserror::Error;

pub const RENDEZVOUS_MAX_CIPHERTEXT_BYTES_V1: usize = 4096;
pub const RENDEZVOUS_MAX_LIVE_RECORDS_V1: usize = 16;
pub const RENDEZVOUS_MAX_LIVE_BYTES_V1: usize = 64 * 1024;
pub const RENDEZVOUS_MAX_OWNER_SLOTS_V1: usize = 8;
pub const RENDEZVOUS_MAX_RETAINED_SLOTS_V1: usize = 64;
pub const RENDEZVOUS_MAX_TTL_V1: Duration = Duration::from_secs(30);
pub const RENDEZVOUS_MAX_INSTANCE_TTL_V1: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum RendezvousErrorV1 {
    #[error("invalid rendezvous instance or lifetime")]
    InvalidInstance,
    #[error("invalid rendezvous owner or slot capability")]
    InvalidCapability,
    #[error("invalid rendezvous revision, TTL or ciphertext length")]
    InvalidRecord,
    #[error("rendezvous service has expired")]
    Expired,
    #[error("rendezvous monotonic clock moved backwards")]
    ClockRegression,
    #[error("rendezvous slot belongs to another authenticated owner")]
    OwnerMismatch,
    #[error("rendezvous revision is stale")]
    StaleRevision,
    #[error("rendezvous revision has conflicting ciphertext")]
    Conflict,
    #[error("rendezvous revision was consumed or expired")]
    RetiredRevision,
    #[error("rendezvous owner slot budget exhausted")]
    OwnerQuota,
    #[error("rendezvous retained slot budget exhausted")]
    RetainedQuota,
    #[error("rendezvous live record or byte budget exhausted")]
    LiveQuota,
    #[error("rendezvous bounded allocation unavailable")]
    Allocation,
    #[error("rendezvous counter exhausted")]
    CounterExhausted,
}

#[derive(Clone, PartialEq, Eq)]
pub struct RendezvousReceiptV1 {
    pub revision: u64,
    pub digest: [u8; 32],
    /// Truncated milliseconds until the fixed original expiry. A positive
    /// submillisecond remainder is reported as zero, never as an extension.
    pub remaining_ttl_ms: u64,
}

#[derive(Clone, PartialEq, Eq)]
pub struct RendezvousRecordV1 {
    pub revision: u64,
    pub digest: [u8; 32],
    pub ciphertext: Vec<u8>,
    pub remaining_ttl_ms: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RendezvousStatsV1 {
    pub live_records: usize,
    pub live_bytes: usize,
    /// Includes consumed/expired slots, whose owner and revision remain bound.
    pub retained_slots: usize,
}

struct Entry {
    // Hash the capability with an instance/domain binding instead of retaining
    // the bearer secret. Guessable slots are not made safe by this hash.
    slot_hash: [u8; 32],
    owner: [u8; 32],
    revision: u64,
    digest: [u8; 32],
    expires_at: Instant,
    ciphertext: Option<Box<[u8]>>,
}

/// At most 64 metadata entries and 64 KiB of live payloads are retained. Input
/// Vec capacity is not trusted; accepted bytes are copied into bounded storage.
/// Returned record copies belong to the caller, which must bound its own IO.
pub struct RendezvousStoreV1 {
    instance: [u8; 32],
    expires_at: Instant,
    last_now: Instant,
    terminal: Option<RendezvousErrorV1>,
    entries: Vec<Entry>,
}

impl RendezvousStoreV1 {
    pub fn new(instance: [u8; 32], expires_at: Instant) -> Result<Self, RendezvousErrorV1> {
        let now = Instant::now();
        if instance == [0; 32]
            || expires_at <= now
            || expires_at.duration_since(now) > RENDEZVOUS_MAX_INSTANCE_TTL_V1
        {
            return Err(RendezvousErrorV1::InvalidInstance);
        }
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(RENDEZVOUS_MAX_RETAINED_SLOTS_V1)
            .map_err(|_| RendezvousErrorV1::Allocation)?;
        Ok(Self {
            instance,
            expires_at,
            last_now: now,
            terminal: None,
            entries,
        })
    }

    pub fn instance(&self) -> [u8; 32] {
        self.instance
    }

    /// First write binds an authenticated owner for the entire instance.
    /// Higher revisions may replace a live/retired record in the same slot.
    /// Repeating a live revision with identical bytes returns its ORIGINAL
    /// expiry; a consumed/expired revision can never be resurrected.
    pub fn put(
        &mut self,
        owner: [u8; 32],
        slot: [u8; 32],
        revision: u64,
        ttl: Duration,
        ciphertext: Vec<u8>,
        now: Instant,
    ) -> Result<RendezvousReceiptV1, RendezvousErrorV1> {
        self.observe(now)?;
        if owner == [0; 32] {
            return Err(RendezvousErrorV1::InvalidCapability);
        }
        let slot_hash = self.slot_hash(slot)?;
        if revision == 0
            || ttl.is_zero()
            || ttl > RENDEZVOUS_MAX_TTL_V1
            || ciphertext.is_empty()
            || ciphertext.len() > RENDEZVOUS_MAX_CIPHERTEXT_BYTES_V1
        {
            return Err(RendezvousErrorV1::InvalidRecord);
        }
        let digest: [u8; 32] = Sha256::digest(&ciphertext).into();
        let existing = self
            .entries
            .iter()
            .position(|entry| entry.slot_hash == slot_hash);
        if let Some(index) = existing {
            let entry = &self.entries[index];
            if entry.owner != owner {
                return Err(RendezvousErrorV1::OwnerMismatch);
            }
            if revision < entry.revision {
                return Err(RendezvousErrorV1::StaleRevision);
            }
            if revision == entry.revision {
                if digest != entry.digest {
                    return Err(RendezvousErrorV1::Conflict);
                }
                let bytes = entry
                    .ciphertext
                    .as_ref()
                    .ok_or(RendezvousErrorV1::RetiredRevision)?;
                if bytes.as_ref() != ciphertext.as_slice() {
                    return Err(RendezvousErrorV1::Conflict);
                }
                return Ok(receipt(entry, now));
            }
            let minimum = entry
                .revision
                .checked_add(1)
                .ok_or(RendezvousErrorV1::CounterExhausted)?;
            if revision < minimum {
                return Err(RendezvousErrorV1::StaleRevision);
            }
        } else {
            if self.entries.len() >= RENDEZVOUS_MAX_RETAINED_SLOTS_V1 {
                return Err(RendezvousErrorV1::RetainedQuota);
            }
            if self
                .entries
                .iter()
                .filter(|entry| entry.owner == owner)
                .count()
                >= RENDEZVOUS_MAX_OWNER_SLOTS_V1
            {
                return Err(RendezvousErrorV1::OwnerQuota);
            }
        }
        let usage = self.usage()?;
        let old_size =
            existing.and_then(|index| self.entries[index].ciphertext.as_ref().map(|v| v.len()));
        let live_records = usage
            .live_records
            .checked_sub(usize::from(old_size.is_some()))
            .and_then(|value| value.checked_add(1))
            .ok_or(RendezvousErrorV1::CounterExhausted)?;
        let live_bytes = usage
            .live_bytes
            .checked_sub(old_size.unwrap_or(0))
            .and_then(|value| value.checked_add(ciphertext.len()))
            .ok_or(RendezvousErrorV1::CounterExhausted)?;
        if live_records > RENDEZVOUS_MAX_LIVE_RECORDS_V1
            || live_bytes > RENDEZVOUS_MAX_LIVE_BYTES_V1
        {
            return Err(RendezvousErrorV1::LiveQuota);
        }
        let expires_at = now
            .checked_add(ttl)
            .ok_or(RendezvousErrorV1::InvalidRecord)?
            .min(self.expires_at);
        // All ownership, revision, quota, time and allocation checks precede
        // a new binding/replacement. Failed writes cannot consume a slot.
        let payload = copy_bounded(&ciphertext)?.into_boxed_slice();
        let entry = Entry {
            slot_hash,
            owner,
            revision,
            digest,
            expires_at,
            ciphertext: Some(payload),
        };
        let result = receipt(&entry, now);
        match existing {
            Some(index) => self.entries[index] = entry,
            None => self.entries.push(entry),
        }
        Ok(result)
    }

    /// The unguessable slot capability authorizes this bounded copy. Retrieval
    /// does not consume the record; use an exact conditional acknowledgement.
    pub fn get(
        &mut self,
        slot: [u8; 32],
        now: Instant,
    ) -> Result<Option<RendezvousRecordV1>, RendezvousErrorV1> {
        self.observe(now)?;
        let slot_hash = self.slot_hash(slot)?;
        let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.slot_hash == slot_hash)
        else {
            return Ok(None);
        };
        let Some(ciphertext) = &entry.ciphertext else {
            return Ok(None);
        };
        Ok(Some(RendezvousRecordV1 {
            revision: entry.revision,
            digest: entry.digest,
            ciphertext: copy_bounded(ciphertext)?,
            remaining_ttl_ms: remaining_ms(entry.expires_at, now),
        }))
    }

    /// A stale acknowledgement cannot delete a replacement revision. The first
    /// exact acknowledgement consumes it; repeats/misses return false.
    pub fn ack(
        &mut self,
        slot: [u8; 32],
        revision: u64,
        digest: [u8; 32],
        now: Instant,
    ) -> Result<bool, RendezvousErrorV1> {
        self.observe(now)?;
        let slot_hash = self.slot_hash(slot)?;
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.slot_hash == slot_hash)
        else {
            return Ok(false);
        };
        if entry.revision != revision || entry.digest != digest || entry.ciphertext.is_none() {
            return Ok(false);
        }
        entry.ciphertext = None;
        Ok(true)
    }

    pub fn stats(&mut self, now: Instant) -> Result<RendezvousStatsV1, RendezvousErrorV1> {
        self.observe(now)?;
        self.usage()
    }

    fn slot_hash(&self, slot: [u8; 32]) -> Result<[u8; 32], RendezvousErrorV1> {
        if slot == [0; 32] {
            return Err(RendezvousErrorV1::InvalidCapability);
        }
        let mut hasher = Sha256::new();
        hasher.update(b"novovm.opaque-rendezvous.slot.v1\0");
        hasher.update(self.instance);
        hasher.update(slot);
        Ok(hasher.finalize().into())
    }

    fn observe(&mut self, now: Instant) -> Result<(), RendezvousErrorV1> {
        if let Some(error) = self.terminal {
            return Err(error);
        }
        let failure = if now < self.last_now {
            Some(RendezvousErrorV1::ClockRegression)
        } else if now >= self.expires_at {
            Some(RendezvousErrorV1::Expired)
        } else {
            None
        };
        if let Some(error) = failure {
            self.terminal = Some(error);
            for entry in &mut self.entries {
                entry.ciphertext = None;
            }
            return Err(error);
        }
        self.last_now = now;
        for entry in &mut self.entries {
            if now >= entry.expires_at {
                entry.ciphertext = None;
            }
        }
        Ok(())
    }

    fn usage(&self) -> Result<RendezvousStatsV1, RendezvousErrorV1> {
        let mut stats = RendezvousStatsV1 {
            retained_slots: self.entries.len(),
            ..RendezvousStatsV1::default()
        };
        for entry in &self.entries {
            if let Some(ciphertext) = &entry.ciphertext {
                stats.live_records = stats
                    .live_records
                    .checked_add(1)
                    .ok_or(RendezvousErrorV1::CounterExhausted)?;
                stats.live_bytes = stats
                    .live_bytes
                    .checked_add(ciphertext.len())
                    .ok_or(RendezvousErrorV1::CounterExhausted)?;
            }
        }
        Ok(stats)
    }
}

fn copy_bounded(bytes: &[u8]) -> Result<Vec<u8>, RendezvousErrorV1> {
    if bytes.len() > RENDEZVOUS_MAX_CIPHERTEXT_BYTES_V1 {
        return Err(RendezvousErrorV1::InvalidRecord);
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(bytes.len())
        .map_err(|_| RendezvousErrorV1::Allocation)?;
    result.extend_from_slice(bytes);
    Ok(result)
}

fn remaining_ms(expires_at: Instant, now: Instant) -> u64 {
    // Stored deadlines are at most 30 seconds ahead; this conversion is bounded.
    expires_at.saturating_duration_since(now).as_millis() as u64
}

fn receipt(entry: &Entry, now: Instant) -> RendezvousReceiptV1 {
    RendezvousReceiptV1 {
        revision: entry.revision,
        digest: entry.digest,
        remaining_ttl_ms: remaining_ms(entry.expires_at, now),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (RendezvousStoreV1, Instant) {
        let store =
            RendezvousStoreV1::new([1; 32], Instant::now() + RENDEZVOUS_MAX_INSTANCE_TTL_V1)
                .unwrap();
        (store, Instant::now())
    }

    fn put(
        store: &mut RendezvousStoreV1,
        slot: u8,
        revision: u64,
        now: Instant,
    ) -> RendezvousReceiptV1 {
        store
            .put(
                [2; 32],
                [slot; 32],
                revision,
                Duration::from_secs(30),
                vec![3; 32],
                now,
            )
            .unwrap()
    }

    #[test]
    fn instance_and_input_bounds() {
        let now = Instant::now();
        assert!(matches!(
            RendezvousStoreV1::new([0; 32], now + Duration::from_secs(60)),
            Err(RendezvousErrorV1::InvalidInstance)
        ));
        assert!(matches!(
            RendezvousStoreV1::new([1; 32], now),
            Err(RendezvousErrorV1::InvalidInstance)
        ));
        assert!(matches!(
            RendezvousStoreV1::new([1; 32], now + Duration::from_secs(121)),
            Err(RendezvousErrorV1::InvalidInstance)
        ));
        let (mut store, now) = fixture();
        assert_eq!(store.instance(), [1; 32]);
        for ttl in [Duration::ZERO, Duration::from_secs(31)] {
            assert!(matches!(
                store.put([2; 32], [3; 32], 1, ttl, vec![4], now),
                Err(RendezvousErrorV1::InvalidRecord)
            ));
        }
        for bytes in [Vec::new(), vec![1; 4097]] {
            assert!(matches!(
                store.put([2; 32], [3; 32], 1, Duration::from_secs(1), bytes, now),
                Err(RendezvousErrorV1::InvalidRecord)
            ));
        }
        assert!(matches!(
            store.put([2; 32], [3; 32], 0, Duration::from_secs(1), vec![4], now),
            Err(RendezvousErrorV1::InvalidRecord)
        ));
        assert!(matches!(
            store.put([0; 32], [3; 32], 1, Duration::from_secs(1), vec![4], now),
            Err(RendezvousErrorV1::InvalidCapability)
        ));
        assert!(matches!(
            store.get([0; 32], now),
            Err(RendezvousErrorV1::InvalidCapability)
        ));
        assert_eq!(store.stats(now).unwrap(), RendezvousStatsV1::default());
    }

    #[test]
    fn retry_returns_original_expiry_and_exact_payload() {
        let (mut store, now) = fixture();
        let first = put(&mut store, 3, 1, now);
        let later = now + Duration::from_secs(11);
        let retry = put(&mut store, 3, 1, later);
        assert_eq!(first.remaining_ttl_ms, 30_000);
        assert_eq!(retry.remaining_ttl_ms, 19_000);
        assert_eq!(retry.digest, first.digest);
        let record = store.get([3; 32], later).unwrap().unwrap();
        assert_eq!(record.ciphertext, vec![3; 32]);
        assert_eq!(record.remaining_ttl_ms, 19_000);
        assert!(store
            .get([3; 32], now + Duration::from_secs(30))
            .unwrap()
            .is_none());
        assert!(matches!(
            store.put(
                [2; 32],
                [3; 32],
                1,
                Duration::from_secs(30),
                vec![3; 32],
                now + Duration::from_secs(30)
            ),
            Err(RendezvousErrorV1::RetiredRevision)
        ));
    }

    #[test]
    fn ownership_survives_ack_and_expiry() {
        let (mut store, now) = fixture();
        let receipt = put(&mut store, 3, 1, now);
        assert!(store.ack([3; 32], 1, receipt.digest, now).unwrap());
        for time in [now, now + Duration::from_secs(31)] {
            assert!(matches!(
                store.put([4; 32], [3; 32], 2, Duration::from_secs(30), vec![5], time),
                Err(RendezvousErrorV1::OwnerMismatch)
            ));
        }
        let later = now + Duration::from_secs(31);
        assert!(matches!(
            store.put(
                [2; 32],
                [3; 32],
                1,
                Duration::from_secs(30),
                vec![3; 32],
                later
            ),
            Err(RendezvousErrorV1::RetiredRevision)
        ));
        assert_eq!(put(&mut store, 3, 2, later).revision, 2);
    }

    #[test]
    fn conflicts_and_old_revisions_never_replace() {
        let (mut store, now) = fixture();
        let receipt = put(&mut store, 3, 2, now);
        assert!(matches!(
            store.put(
                [2; 32],
                [3; 32],
                1,
                Duration::from_secs(30),
                vec![3; 32],
                now
            ),
            Err(RendezvousErrorV1::StaleRevision)
        ));
        assert!(matches!(
            store.put(
                [2; 32],
                [3; 32],
                2,
                Duration::from_secs(30),
                vec![4; 32],
                now
            ),
            Err(RendezvousErrorV1::Conflict)
        ));
        let record = store.get([3; 32], now).unwrap().unwrap();
        assert_eq!(record.digest, receipt.digest);
        assert_eq!(record.ciphertext, vec![3; 32]);
    }

    #[test]
    fn acknowledgements_are_conditional_and_cannot_delete_replacements() {
        let (mut store, now) = fixture();
        let old = put(&mut store, 3, 1, now);
        let new = put(&mut store, 3, 2, now);
        assert!(!store.ack([3; 32], 1, old.digest, now).unwrap());
        assert!(!store.ack([3; 32], 2, [0; 32], now).unwrap());
        assert!(!store.ack([4; 32], 2, new.digest, now).unwrap());
        assert!(store.get([3; 32], now).unwrap().is_some());
        assert!(store.ack([3; 32], 2, new.digest, now).unwrap());
        assert!(!store.ack([3; 32], 2, new.digest, now).unwrap());
        assert!(store.get([3; 32], now).unwrap().is_none());
        assert!(matches!(
            store.put(
                [2; 32],
                [3; 32],
                2,
                Duration::from_secs(30),
                vec![3; 32],
                now
            ),
            Err(RendezvousErrorV1::RetiredRevision)
        ));
        assert_eq!(
            store.stats(now).unwrap(),
            RendezvousStatsV1 {
                live_records: 0,
                live_bytes: 0,
                retained_slots: 1
            }
        );
    }

    #[test]
    fn expiry_is_exclusive_and_millisecond_rounding_never_extends_it() {
        let (mut store, now) = fixture();
        let receipt = store
            .put([2; 32], [3; 32], 1, Duration::from_nanos(1), vec![1], now)
            .unwrap();
        assert_eq!(receipt.remaining_ttl_ms, 0);
        assert!(store.get([3; 32], now).unwrap().is_some());
        let expired = now + Duration::from_nanos(1);
        assert!(store.get([3; 32], expired).unwrap().is_none());
        assert!(!store.ack([3; 32], 1, receipt.digest, expired).unwrap());
    }

    #[test]
    fn owner_quota_includes_retired_slots() {
        let (mut store, now) = fixture();
        for slot in 1..=8 {
            let receipt = put(&mut store, slot, 1, now);
            assert!(store.ack([slot; 32], 1, receipt.digest, now).unwrap());
        }
        assert!(matches!(
            store.put([2; 32], [9; 32], 1, Duration::from_secs(30), vec![3], now),
            Err(RendezvousErrorV1::OwnerQuota)
        ));
        assert_eq!(put(&mut store, 1, 2, now).revision, 2);
        assert_eq!(store.stats(now).unwrap().retained_slots, 8);
    }

    #[test]
    fn live_full_budget_allows_replacement_but_no_seventeenth_record() {
        let (mut store, now) = fixture();
        for slot in 1..=16u8 {
            let owner = [1 + (slot - 1) / 8; 32];
            store
                .put(
                    owner,
                    [slot; 32],
                    1,
                    Duration::from_secs(30),
                    vec![slot; 4096],
                    now,
                )
                .unwrap();
        }
        assert_eq!(
            store.stats(now).unwrap(),
            RendezvousStatsV1 {
                live_records: 16,
                live_bytes: 65_536,
                retained_slots: 16
            }
        );
        assert!(matches!(
            store.put([3; 32], [17; 32], 1, Duration::from_secs(30), vec![1], now),
            Err(RendezvousErrorV1::LiveQuota)
        ));
        store
            .put(
                [1; 32],
                [1; 32],
                2,
                Duration::from_secs(30),
                vec![2; 4096],
                now,
            )
            .unwrap();
        let saved = store.get([2; 32], now).unwrap().unwrap();
        store.ack([2; 32], 1, saved.digest, now).unwrap();
        store
            .put(
                [3; 32],
                [17; 32],
                1,
                Duration::from_secs(30),
                vec![7; 4096],
                now,
            )
            .unwrap();
        assert_eq!(
            store.stats(now).unwrap(),
            RendezvousStatsV1 {
                live_records: 16,
                live_bytes: 65_536,
                retained_slots: 17
            }
        );
    }

    #[test]
    fn tombstone_budget_never_evicts_to_allow_old_writes() {
        let (mut store, now) = fixture();
        for slot in 1..=64u8 {
            let owner = [1 + (slot - 1) / 8; 32];
            let receipt = store
                .put(
                    owner,
                    [slot; 32],
                    1,
                    Duration::from_secs(30),
                    vec![slot],
                    now,
                )
                .unwrap();
            assert!(store.ack([slot; 32], 1, receipt.digest, now).unwrap());
        }
        assert_eq!(
            store.stats(now).unwrap(),
            RendezvousStatsV1 {
                live_records: 0,
                live_bytes: 0,
                retained_slots: 64
            }
        );
        assert!(matches!(
            store.put([9; 32], [65; 32], 1, Duration::from_secs(30), vec![1], now),
            Err(RendezvousErrorV1::RetainedQuota)
        ));
        assert!(matches!(
            store.put([1; 32], [1; 32], 1, Duration::from_secs(30), vec![1], now),
            Err(RendezvousErrorV1::RetiredRevision)
        ));
        store
            .put([1; 32], [1; 32], 2, Duration::from_secs(30), vec![1], now)
            .unwrap();
        assert_eq!(store.stats(now).unwrap().retained_slots, 64);
    }

    #[test]
    fn expired_payloads_release_live_budget_but_keep_high_water() {
        let (mut store, now) = fixture();
        for slot in 1..=16u8 {
            store
                .put(
                    [1 + (slot - 1) / 8; 32],
                    [slot; 32],
                    1,
                    Duration::from_secs(1),
                    vec![slot; 4096],
                    now,
                )
                .unwrap();
        }
        let later = now + Duration::from_secs(1);
        store
            .put(
                [3; 32],
                [17; 32],
                1,
                Duration::from_secs(30),
                vec![3],
                later,
            )
            .unwrap();
        assert_eq!(
            store.stats(later).unwrap(),
            RendezvousStatsV1 {
                live_records: 1,
                live_bytes: 1,
                retained_slots: 17
            }
        );
        assert!(matches!(
            store.put(
                [1; 32],
                [1; 32],
                1,
                Duration::from_secs(30),
                vec![1; 4096],
                later
            ),
            Err(RendezvousErrorV1::RetiredRevision)
        ));
    }

    #[test]
    fn service_expiry_is_final_and_bounds_record_expiry() {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut store = RendezvousStoreV1::new([1; 32], deadline).unwrap();
        let now = Instant::now();
        let receipt = put(&mut store, 3, 1, now);
        assert!(receipt.remaining_ttl_ms <= 10_000);
        assert!(matches!(
            store.get([3; 32], deadline),
            Err(RendezvousErrorV1::Expired)
        ));
        assert!(matches!(store.stats(now), Err(RendezvousErrorV1::Expired)));
        assert!(matches!(
            store.ack([3; 32], 1, receipt.digest, deadline),
            Err(RendezvousErrorV1::Expired)
        ));
    }

    #[test]
    fn clock_regression_fails_closed_permanently() {
        let (mut store, now) = fixture();
        put(&mut store, 3, 1, now);
        store.stats(now + Duration::from_secs(2)).unwrap();
        assert!(matches!(
            store.get([3; 32], now),
            Err(RendezvousErrorV1::ClockRegression)
        ));
        assert!(matches!(
            store.put(
                [2; 32],
                [3; 32],
                2,
                Duration::from_secs(30),
                vec![3],
                now + Duration::from_secs(3)
            ),
            Err(RendezvousErrorV1::ClockRegression)
        ));
    }

    #[test]
    fn maximum_revision_does_not_wrap_or_restore_an_older_value() {
        let (mut store, now) = fixture();
        put(&mut store, 3, u64::MAX, now);
        assert_eq!(put(&mut store, 3, u64::MAX, now).revision, u64::MAX);
        assert!(matches!(
            store.put(
                [2; 32],
                [3; 32],
                1,
                Duration::from_secs(30),
                vec![3; 32],
                now
            ),
            Err(RendezvousErrorV1::StaleRevision)
        ));
        assert!(matches!(
            store.put(
                [2; 32],
                [3; 32],
                0,
                Duration::from_secs(30),
                vec![3; 32],
                now
            ),
            Err(RendezvousErrorV1::InvalidRecord)
        ));
    }

    #[test]
    fn retained_payload_does_not_inherit_input_vec_capacity() {
        let (mut store, now) = fixture();
        let mut input = Vec::with_capacity(1024 * 1024);
        input.push(7);
        store
            .put([2; 32], [3; 32], 1, Duration::from_secs(30), input, now)
            .unwrap();
        assert_eq!(store.entries[0].ciphertext.as_ref().unwrap().len(), 1);
        assert_eq!(store.stats(now).unwrap().live_bytes, 1);
    }
}
