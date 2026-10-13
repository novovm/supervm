//! Shared bounded durable ciphertext store for NOVONET L3/L4.
//! Moved from NOVOCHAT P0; this is the sole implementation. Platform owns the private directory.
//! Each mailbox belongs to one recipient device. Read capabilities never leave that device.
//! No wallet keys, UCA identifiers, plaintext indexing, or chain/RPC dependency.
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

pub struct MailboxStore {
    db: Connection,
    limits: Limits,
}
impl MailboxStore {
    pub fn open(path: impl AsRef<Path>, limits: Limits) -> Result<Self, StoreError> {
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
        let path = path.as_ref();
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
        let db = Connection::open_with_flags(path, flags)?;
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
        Ok(Self { db, limits })
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
        // Preserve bounded tombstones after expiry, then remove the device mailbox.
        tx.execute(
            "UPDATE envelopes SET payload=x'',consumed=1 WHERE consumed=0 AND expires<=?1",
            [now],
        )?;
        tx.execute(
            "DELETE FROM envelopes WHERE expires<=?1",
            [now - REPLAY_WINDOW],
        )?;
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
        let tx = self.begin(now)?;
        authorize(&tx, id, read, false, now)?;
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
        let existing: Option<(Vec<u8>, i64, bool)> = tx
            .query_row(
                "SELECT digest,expires,consumed FROM envelopes WHERE mailbox=?1 AND id=?2",
                params![mailbox, object],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((old_digest, old_expiry, consumed)) = existing {
            if consumed {
                return Err(StoreError::Consumed);
            }
            if old_digest != digest || old_expiry != expires {
                return Err(StoreError::Conflict);
            }
            tx.commit()?;
            return Ok(PutResult::Duplicate);
        }
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
        tx.commit()?;
        Ok(PutResult::Stored)
    }
    /// Non-destructive fetch. The device acknowledges only after authenticated local commit.
    pub fn fetch(
        &mut self,
        id: &Id,
        read: &Capability,
        now: i64,
    ) -> Result<Vec<StoredEnvelope>, StoreError> {
        let tx = self.begin(now)?;
        authorize(&tx, id, read, false, now)?;
        let rows = {
            let mut statement = tx.prepare("SELECT id,payload,digest,expires FROM envelopes WHERE mailbox=?1 AND consumed=0 AND expires>?2 ORDER BY expires,id LIMIT 32")?;
            let result = statement
                .query_map(params![id, now], |r| {
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
        tx.execute(
            "UPDATE envelopes SET payload=x'',consumed=1 WHERE mailbox=?1",
            [id],
        )?;
        tx.commit()?;
        Ok(())
    }
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
        let current: Vec<u8> = tx.query_row(
            "SELECT digest FROM envelopes WHERE mailbox=?1 AND id=?2",
            params![mailbox, object],
            |r| r.get(0),
        )?;
        if !bool::from(current.ct_eq(digest)) {
            return Err(StoreError::Conflict);
        }
        tx.execute(
            "UPDATE envelopes SET payload=x'',consumed=1 WHERE mailbox=?1 AND id=?2",
            params![mailbox, object],
        )?;
        tx.commit()?;
        Ok(())
    }
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
