//! Shared bounded durable ciphertext store for NOVONET L3/L4.
//! Moved from NOVOCHAT P0; this is the sole implementation. Platform owns the private directory.
//! Each mailbox belongs to one recipient device. Read capabilities never leave that device.
//! No wallet keys, UCA identifiers, plaintext indexing, or chain/RPC dependency.
//! Store schema 3 deletes object metadata with consumed/expired ciphertext. A later
//! authorized Put is new custody; end-device journals own message deduplication.
use rand::RngCore;
use rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};
use sha2::{Digest, Sha256};
use std::path::Path;
use subtle::ConstantTimeEq;
use thiserror::Error;

pub const MAX_TTL: i64 = 7 * 24 * 3600;
const REPLAY_WINDOW: i64 = 300;
pub type Id = [u8; 32];
// Deliberately not Debug/Serialize: capabilities are secrets.
#[derive(zeroize::ZeroizeOnDrop)]
pub struct Capability([u8; 32]);
impl Capability {
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, StoreError> {
        if bytes == [0; 32] {
            return Err(StoreError::Invalid);
        }
        Ok(Self(bytes))
    }
    /// Only for an authenticated encrypted carrier or platform protected storage.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
impl MailboxGrant {
    pub fn generate() -> Result<Self, StoreError> {
        Ok(Self {
            id: random()?,
            write: Capability(random()?),
            read: Capability(random()?),
        })
    }
}
pub struct MailboxGrant {
    pub id: Id,
    pub write: Capability,
    pub read: Capability,
}

#[derive(Clone, Copy)]
pub struct Limits {
    pub object_bytes: usize,
    pub mailbox_bytes: usize,
    pub mailbox_objects: usize,
    pub total_bytes: usize,
    pub total_objects: usize,
    pub mailboxes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            object_bytes: 64 * 1024,
            mailbox_bytes: 2 * 1024 * 1024,
            mailbox_objects: 256,
            total_bytes: 64 * 1024 * 1024,
            total_objects: 8192,
            mailboxes: 256,
        }
    }
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("storage error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("random source unavailable")]
    Random,
    #[error("invalid capability or expired mailbox")]
    Unauthorized,
    #[error("invalid identifier, retention or ciphertext size")]
    Invalid,
    #[error("resource quota exceeded")]
    Quota,
    #[error("object ID reused with a different commitment")]
    Conflict,
    #[error("object was already consumed")]
    Consumed,
    #[error("clock moved backwards")]
    ClockRollback,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PutResult {
    Stored,
    Duplicate,
}
pub struct StoredEnvelope {
    pub id: Id,
    pub ciphertext: Vec<u8>,
    pub digest: Id,
    pub expires: i64,
}

/// Private per-mailbox commit position. Generation prevents deletion/recreation
/// from making an old position current. Neither value is a delivery receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailboxCursorV1 {
    pub generation: Id,
    pub sequence: u64,
}

/// Local owner status only, never a peer-declared durability/reward proof.
/// Turning off new obligations does not delete committed objects. Serving
/// existing objects still requires the platform's live execution/path authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContributionStatusV1 {
    pub enabled: bool,
    pub resources_available: bool,
    pub retained_objects: u64,
    pub retained_bytes: u64,
    pub retain_until: Option<i64>,
}

pub struct MailboxStore {
    db: Connection,
    limits: Limits,
    resources_available: bool,
}
impl MailboxStore {
    pub fn open(path: impl AsRef<Path>, limits: Limits) -> Result<Self, StoreError> {
        Self::open_inner(path.as_ref(), limits, true, true)
    }

    /// Same store/schema for an opportunistic contributor. Existing durable
    /// choice wins over the install default. Execution/resource readiness is
    /// deliberately unavailable on every open until the local platform checks it.
    pub fn open_contributor(
        path: impl AsRef<Path>,
        limits: Limits,
        initially_enabled: bool,
    ) -> Result<Self, StoreError> {
        Self::open_inner(path.as_ref(), limits, initially_enabled, false)
    }

    fn open_inner(
        path: &Path,
        limits: Limits,
        initially_enabled: bool,
        resources_available: bool,
    ) -> Result<Self, StoreError> {
        if limits.object_bytes == 0
            || limits.object_bytes > 65536
            || limits.mailbox_bytes == 0
            || limits.mailbox_bytes > limits.total_bytes
            || limits.total_bytes > 64 * 1024 * 1024
            || limits.mailbox_objects == 0
            || limits.mailbox_objects > limits.total_objects
            || limits.total_objects > 8192
            || limits.mailboxes == 0
            || limits.mailboxes > 256
        {
            return Err(StoreError::Invalid);
        }
        let exists = path.try_exists().map_err(|_| StoreError::Invalid)?;
        if exists {
            let metadata = std::fs::symlink_metadata(path).map_err(|_| StoreError::Invalid)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() == 0 {
                return Err(StoreError::Invalid);
            }
        }
        let flags = if exists {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
        };
        let mut db = Connection::open_with_flags(path, flags)?;
        db.busy_timeout(std::time::Duration::from_secs(2))?;
        if exists {
            let tables:i64=db.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('clock_guard','mailboxes','envelopes')",[],|r|r.get(0))?;
            // Missing/corrupt history is never treated as a new enrollment.
            if tables != 3 {
                return Err(StoreError::Invalid);
            }
            let clocks: i64 =
                db.query_row("SELECT count(*) FROM clock_guard WHERE id=1", [], |r| {
                    r.get(0)
                })?;
            if clocks != 1 {
                return Err(StoreError::Invalid);
            }
        }
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=DELETE;
            PRAGMA synchronous=EXTRA; PRAGMA secure_delete=ON; PRAGMA trusted_schema=OFF;
            PRAGMA temp_store=MEMORY; PRAGMA mmap_size=0; PRAGMA cache_size=-2048;
            CREATE TABLE IF NOT EXISTS clock_guard (id INTEGER PRIMARY KEY CHECK(id=1), high_water INTEGER NOT NULL);
            INSERT OR IGNORE INTO clock_guard VALUES(1,0);
            CREATE TABLE IF NOT EXISTS mailboxes (
              id BLOB PRIMARY KEY CHECK(length(id)=32), writer BLOB NOT NULL, reader BLOB NOT NULL, expires INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS envelopes (
              mailbox BLOB NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
              id BLOB NOT NULL CHECK(length(id)=32), payload BLOB NOT NULL,
              digest BLOB NOT NULL, expires INTEGER NOT NULL, consumed INTEGER NOT NULL DEFAULT 0,
              PRIMARY KEY(mailbox,id));")?;
        // Enforce bytes, not an assumed page size. The rollback journal can use
        // another bounded copy; neither is a promise of physical flash erasure.
        let page_size: i64 = db.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        let pages = 128 * 1024 * 1024 / page_size;
        let actual: i64 =
            db.query_row(&format!("PRAGMA max_page_count={pages}"), [], |r| r.get(0))?;
        let synchronous: i64 = db.query_row("PRAGMA synchronous", [], |r| r.get(0))?;
        if actual > pages || synchronous != 3 {
            return Err(StoreError::Invalid);
        }
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 0 {
            // Atomic migration of the existing database; no message, capability,
            // receipt, expiry or clock guard is changed.
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch(
                "CREATE TABLE mailbox_revisions (
                mailbox BLOB PRIMARY KEY REFERENCES mailboxes(id) ON DELETE CASCADE,
                generation BLOB NOT NULL CHECK(length(generation)=32),
                revision INTEGER NOT NULL CHECK(revision>=0));
                INSERT INTO mailbox_revisions SELECT id,randomblob(32),0 FROM mailboxes;
                PRAGMA user_version=1;",
            )?;
            tx.commit()?;
        } else if version != 1 && version != 2 && version != 3 {
            return Err(StoreError::Invalid);
        }
        if version <= 1 {
            // Version 0's preceding revision migration is independently safe.
            // A crash before this transaction leaves version 1; a retry neither
            // deletes content nor resets any established contribution choice.
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch(
                "CREATE TABLE mailbox_contribution (
                 id INTEGER PRIMARY KEY CHECK(id=1),
                 enabled INTEGER NOT NULL CHECK(enabled IN (0,1)));",
            )?;
            tx.execute(
                "INSERT INTO mailbox_contribution VALUES(1,?1)",
                [initially_enabled],
            )?;
            tx.execute_batch("PRAGMA user_version=2;")?;
            tx.commit()?;
        }
        let valid: bool = db.query_row(
            "SELECT count(*)=1 AND coalesce(min(id=1 AND enabled IN (0,1)),0)
             FROM mailbox_contribution",
            [],
            |r| r.get(0),
        )?;
        if !valid {
            return Err(StoreError::Invalid);
        }
        let missing: i64 = db.query_row(
            "SELECT count(*) FROM mailboxes m LEFT JOIN
            mailbox_revisions r ON m.id=r.mailbox WHERE r.mailbox IS NULL
            OR length(r.generation)!=32 OR r.revision<0",
            [],
            |r| r.get(0),
        )?;
        if missing != 0 {
            return Err(StoreError::Invalid);
        }
        if version < 3 {
            // Retention contract v3: consumed objects leave no per-object
            // evidence at the intermediary. Preserve live custody, capability
            // revocations, notification positions and the clock guard. Older
            // readers reject version 3 rather than restoring tombstone semantics.
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch(
                "DELETE FROM envelopes WHERE consumed=1;
                 PRAGMA user_version=3;",
            )?;
            tx.commit()?;
        }
        Ok(Self {
            db,
            limits,
            resources_available,
        })
    }
    fn begin(&mut self, now: i64) -> Result<Transaction<'_>, StoreError> {
        if !(0..=i64::MAX - MAX_TTL - REPLAY_WINDOW).contains(&now) {
            return Err(StoreError::Invalid);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let last: i64 = tx.query_row("SELECT high_water FROM clock_guard WHERE id=1", [], |r| {
            r.get(0)
        })?;
        if now < last {
            return Err(StoreError::ClockRollback);
        }
        tx.execute("UPDATE clock_guard SET high_water=?1 WHERE id=1", [now])?;
        // Expired ciphertext and its object metadata have no further custody
        // purpose. Only capability revocation retains its bounded replay window.
        tx.execute("DELETE FROM envelopes WHERE expires<=?1", [now])?;
        tx.execute(
            "DELETE FROM mailboxes WHERE expires<=?1",
            [now - REPLAY_WINDOW],
        )?;
        Ok(tx)
    }
    pub fn sweep(&mut self, now: i64) -> Result<(), StoreError> {
        self.begin(now)?.commit()?;
        Ok(())
    }

    /// Owner-only durable choice. Serialize through the SAME store owner as
    /// Put: successful return is the barrier after which no new obligation is
    /// accepted while disabled. An already committed Put remains committed.
    pub fn set_contribution_enabled(&mut self, enabled: bool, now: i64) -> Result<(), StoreError> {
        let tx = self.begin(now)?;
        if tx.execute(
            "UPDATE mailbox_contribution SET enabled=?1 WHERE id=1",
            [enabled],
        )? != 1
        {
            return Err(StoreError::Invalid);
        }
        tx.commit()?;
        Ok(())
    }

    /// Transient local resource readiness, never a peer assertion or a grant
    /// to run in the background. Changing it does not alter retained ciphertext.
    pub fn set_resources_available(&mut self, available: bool) {
        self.resources_available = available;
    }

    pub fn contribution_status(&mut self, now: i64) -> Result<ContributionStatusV1, StoreError> {
        let resources_available = self.resources_available;
        let tx = self.begin(now)?;
        let enabled = contribution_enabled(&tx)?;
        let (retained_objects, retained_bytes, retain_until): (i64, i64, Option<i64>) = tx
            .query_row(
                "SELECT count(*),coalesce(sum(length(payload)),0),max(expires)
             FROM envelopes WHERE consumed=0 AND expires>?1",
                [now],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
        tx.commit()?;
        Ok(ContributionStatusV1 {
            enabled,
            resources_available,
            retained_objects: retained_objects
                .try_into()
                .map_err(|_| StoreError::Invalid)?,
            retained_bytes: retained_bytes.try_into().map_err(|_| StoreError::Invalid)?,
            retain_until,
        })
    }

    pub fn create(&mut self, now: i64, ttl: i64) -> Result<MailboxGrant, StoreError> {
        if !(60..=MAX_TTL).contains(&ttl) {
            return Err(StoreError::Invalid);
        }
        let grant = MailboxGrant::generate()?;
        self.register(
            &grant,
            now,
            now.checked_add(ttl).ok_or(StoreError::Invalid)?,
        )?;
        Ok(grant)
    }
    /// Client generates and durably retains the grant before registration, so a
    /// lost response can retry exactly the same mailbox. No public account ID.
    pub fn register(
        &mut self,
        grant: &MailboxGrant,
        now: i64,
        expires: i64,
    ) -> Result<(), StoreError> {
        let ttl = expires.checked_sub(now).ok_or(StoreError::Invalid)?;
        if !(1..=MAX_TTL).contains(&ttl)
            || grant.id == [0; 32]
            || bool::from(grant.write.0.ct_eq(&grant.read.0))
        {
            return Err(StoreError::Invalid);
        }
        let limit = self.limits.mailboxes;
        let resources_available = self.resources_available;
        let tx = self.begin(now)?;
        let existing: Option<(Vec<u8>, Vec<u8>, i64)> = tx
            .query_row(
                "SELECT writer,reader,expires FROM mailboxes WHERE id=?1",
                [&grant.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((writer, reader, expiry)) = existing {
            if expiry != expires
                || !bool::from(writer.ct_eq(&capability_hash(&grant.write, b"write")))
                || !bool::from(reader.ct_eq(&capability_hash(&grant.read, b"read")))
            {
                return Err(StoreError::Conflict);
            }
            tx.commit()?;
            return Ok(());
        }
        admit_new_obligation(&tx, resources_available)?;
        let count =
            tx.query_row("SELECT count(*) FROM mailboxes", [], |r| r.get::<_, u32>(0))? as usize;
        if count >= limit {
            return Err(StoreError::Quota);
        }
        tx.execute(
            "INSERT INTO mailboxes VALUES(?1,?2,?3,?4)",
            params![
                &grant.id,
                capability_hash(&grant.write, b"write"),
                capability_hash(&grant.read, b"read"),
                expires
            ],
        )?;
        tx.execute(
            "INSERT INTO mailbox_revisions VALUES(?1,?2,0)",
            params![&grant.id, random()?],
        )?;
        tx.commit()?;
        Ok(())
    }
    /// Extends the mailbox lease, never any envelope expiry.
    pub fn renew(
        &mut self,
        id: &Id,
        read: &Capability,
        now: i64,
        ttl: i64,
    ) -> Result<(), StoreError> {
        if !(60..=MAX_TTL).contains(&ttl) {
            return Err(StoreError::Invalid);
        }
        let resources_available = self.resources_available;
        let tx = self.begin(now)?;
        let previous = authorize(&tx, id, read, false, now)?;
        if now + ttl > previous {
            admit_new_obligation(&tx, resources_available)?;
        }
        tx.execute(
            "UPDATE mailboxes SET expires=max(expires,?2) WHERE id=?1",
            params![id, now + ttl],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn put(
        &mut self,
        mailbox: &Id,
        write: &Capability,
        object: &Id,
        ciphertext: &[u8],
        expires: i64,
        now: i64,
    ) -> Result<PutResult, StoreError> {
        let limits = self.limits;
        let resources_available = self.resources_available;
        if *object == [0; 32]
            || ciphertext.is_empty()
            || ciphertext.len() > limits.object_bytes
            || expires <= now
            || expires.checked_sub(now).is_none_or(|ttl| ttl > MAX_TTL)
        {
            return Err(StoreError::Invalid);
        }
        let digest = Sha256::digest(ciphertext).to_vec();
        let tx = self.begin(now)?;
        let lease = authorize(&tx, mailbox, write, true, now)?;
        if expires > lease {
            return Err(StoreError::Invalid);
        }
        let existing: Option<(Vec<u8>, i64)> = tx
            .query_row(
                "SELECT digest,expires FROM envelopes WHERE mailbox=?1 AND id=?2",
                params![mailbox, object],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((old_digest, old_expiry)) = existing {
            if old_digest != digest || old_expiry != expires {
                return Err(StoreError::Conflict);
            }
            tx.commit()?;
            return Ok(PutResult::Duplicate);
        }
        admit_new_obligation(&tx, resources_available)?;
        let (box_count, box_bytes): (usize, usize) = tx.query_row(
            "SELECT count(*),coalesce(sum(length(payload)),0) FROM envelopes WHERE mailbox=?1",
            [mailbox],
            |r| Ok((r.get::<_, u32>(0)? as usize, r.get::<_, u32>(1)? as usize)),
        )?;
        let (total_count, total_bytes): (usize, usize) = tx.query_row(
            "SELECT count(*),coalesce(sum(length(payload)),0) FROM envelopes",
            [],
            |r| Ok((r.get::<_, u32>(0)? as usize, r.get::<_, u32>(1)? as usize)),
        )?;
        if box_count >= limits.mailbox_objects
            || total_count >= limits.total_objects
            || ciphertext.len() > limits.mailbox_bytes.saturating_sub(box_bytes)
            || ciphertext.len() > limits.total_bytes.saturating_sub(total_bytes)
        {
            return Err(StoreError::Quota);
        }
        tx.execute(
            "INSERT INTO envelopes(mailbox,id,payload,digest,expires) VALUES(?1,?2,?3,?4,?5)",
            params![mailbox, object, ciphertext, digest, expires],
        )?;
        if tx.execute(
            "UPDATE mailbox_revisions SET revision=revision+1
            WHERE mailbox=?1 AND revision<9223372036854775807",
            [mailbox],
        )? != 1
        {
            return Err(StoreError::Invalid);
        }
        tx.commit()?;
        Ok(PutResult::Stored)
    }
    /// Snapshot AFTER notification observation is installed. Put increments the
    /// cursor in the same transaction as its ciphertext. Fetch stays authoritative
    /// across lost notifications and node restarts; cursor never consumes data.
    pub fn observe(
        &mut self,
        id: &Id,
        read: &Capability,
        now: i64,
    ) -> Result<(MailboxCursorV1, bool), StoreError> {
        let tx = self.begin(now)?;
        authorize(&tx, id, read, false, now)?;
        let (generation, sequence): (Id, i64) = tx.query_row(
            "SELECT generation,revision FROM mailbox_revisions WHERE mailbox=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let pending: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM envelopes
            WHERE mailbox=?1 AND consumed=0 AND expires>?2)",
            params![id, now],
            |r| r.get(0),
        )?;
        tx.commit()?;
        Ok((
            MailboxCursorV1 {
                generation,
                sequence: sequence.try_into().map_err(|_| StoreError::Invalid)?,
            },
            pending,
        ))
    }
    /// Non-destructive fetch. The device acknowledges only after authenticated local commit.
    pub fn fetch(
        &mut self,
        id: &Id,
        read: &Capability,
        now: i64,
    ) -> Result<Vec<StoredEnvelope>, StoreError> {
        self.fetch_bounded(id, read, now, 32)
    }
    /// One retained object for a wire response; avoid allocating 32 payloads.
    pub fn fetch_one(
        &mut self,
        id: &Id,
        read: &Capability,
        now: i64,
    ) -> Result<Option<StoredEnvelope>, StoreError> {
        Ok(self.fetch_bounded(id, read, now, 1)?.into_iter().next())
    }
    fn fetch_bounded(
        &mut self,
        id: &Id,
        read: &Capability,
        now: i64,
        limit: u8,
    ) -> Result<Vec<StoredEnvelope>, StoreError> {
        let tx = self.begin(now)?;
        authorize(&tx, id, read, false, now)?;
        let rows = {
            // Seconds-resolution expiries often tie for a burst of controls.
            // Do not let their random object IDs reorder that burst. rowid is
            // only a local arrival tie-break, never identity, replay protection
            // or a persistent/wire cursor (maintenance may renumber it).
            // Recipients still handle out-of-order delivery across replicas.
            let mut statement = tx.prepare("SELECT id,payload,digest,expires FROM envelopes WHERE mailbox=?1 AND consumed=0 AND expires>?2 ORDER BY expires,rowid LIMIT ?3")?;
            let result = statement
                .query_map(params![id, now, limit], |r| {
                    Ok(StoredEnvelope {
                        id: r.get(0)?,
                        ciphertext: r.get(1)?,
                        digest: r.get(2)?,
                        expires: r.get(3)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            result
        };
        tx.commit()?;
        Ok(rows)
    }
    /// Read capability revokes the mailbox immediately without resurrectable
    /// ID reuse inside the replay window. Other device boxes are unaffected.
    pub fn revoke(&mut self, id: &Id, read: &Capability, now: i64) -> Result<(), StoreError> {
        let tx = self.begin(now)?;
        authorize(&tx, id, read, false, now)?;
        tx.execute(
            "UPDATE mailboxes SET expires=?2 WHERE id=?1",
            params![id, now],
        )?;
        tx.execute("DELETE FROM envelopes WHERE mailbox=?1", [id])?;
        tx.commit()?;
        Ok(())
    }
    /// Remove this object's ciphertext AND metadata after recipient durable commit.
    /// Success means absent, not proof of delivery. A caller losing this deletion
    /// ACK may repeat it; retained end-device receipts establish delivery instead.
    pub fn consume(
        &mut self,
        mailbox: &Id,
        read: &Capability,
        object: &Id,
        digest: &Id,
        now: i64,
    ) -> Result<(), StoreError> {
        let tx = self.begin(now)?;
        authorize(&tx, mailbox, read, false, now)?;
        let current: Option<Vec<u8>> = tx
            .query_row(
                "SELECT digest FROM envelopes WHERE mailbox=?1 AND id=?2",
                params![mailbox, object],
                |r| r.get(0),
            )
            .optional()?;
        // Absence is an idempotent deletion result, never delivery evidence.
        // Check read authority even when absent. A delayed Consume cannot erase
        // a different object currently using this ID.
        if current
            .as_ref()
            .is_some_and(|value| !bool::from(value.ct_eq(digest)))
        {
            return Err(StoreError::Conflict);
        }
        tx.execute(
            "DELETE FROM envelopes WHERE mailbox=?1 AND id=?2",
            params![mailbox, object],
        )?;
        tx.commit()?;
        Ok(())
    }
}
fn contribution_enabled(tx: &Transaction<'_>) -> Result<bool, StoreError> {
    let enabled: i64 = tx.query_row(
        "SELECT enabled FROM mailbox_contribution WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    match enabled {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(StoreError::Invalid),
    }
}
fn admit_new_obligation(tx: &Transaction<'_>, resources_available: bool) -> Result<(), StoreError> {
    // Existing Quota response expresses zero available new capacity; no new
    // wire protocol or user-identifying contribution metadata is introduced.
    if !contribution_enabled(tx)? || !resources_available {
        return Err(StoreError::Quota);
    }
    Ok(())
}
fn authorize(
    tx: &Transaction<'_>,
    id: &Id,
    token: &Capability,
    writer: bool,
    now: i64,
) -> Result<i64, StoreError> {
    let row: Option<(Vec<u8>, Vec<u8>, i64)> = tx
        .query_row(
            "SELECT writer,reader,expires FROM mailboxes WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((write, read, expiry)) = row else {
        return Err(StoreError::Unauthorized);
    };
    let expected = if writer { write } else { read };
    let actual = capability_hash(token, if writer { b"write" } else { b"read" });
    if expiry <= now || !bool::from(expected.ct_eq(&actual)) {
        return Err(StoreError::Unauthorized);
    }
    Ok(expiry)
}
fn capability_hash(token: &Capability, role: &[u8]) -> Vec<u8> {
    let mut hash = Sha256::new();
    hash.update(b"novochat-mailbox-capability-v1");
    hash.update(role);
    hash.update(token.0);
    hash.finalize().to_vec()
}
fn random() -> Result<Id, StoreError> {
    let mut id = [0; 32];
    rand::rngs::OsRng
        .try_fill_bytes(&mut id)
        .map_err(|_| StoreError::Random)?;
    Ok(id)
}
