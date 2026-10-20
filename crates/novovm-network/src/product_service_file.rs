//! Bounded encrypted checkpoint journal for an installed service's single owner.
//! No keys are generated or stored here. The install owner supplies a private,
//! backup-excluded directory and a separate protected storage key. Missing or
//! torn journals are errors, never an invitation to reset signing history.
use super::*;
use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

const LEGACY_MAGIC: &[u8; 8] = b"NVLSJ001";
const MAGIC: &[u8; 8] = b"NVLSJ002";
const DOMAIN: &[u8] = b"novovm.service-journal.v1\0";
const MAX_JOURNAL: u64 = 16 * 1024 * 1024;
// Checkpoints already contain the complete sequence/revocation/trust state.
// Retain that exact state, not an expired-history-filtered reconstruction.
const COMPACT_AT: u64 = 512 * 1024;
const MAX_FRAME: usize = MAX_SERVICE_CHECKPOINT_BYTES + 24 + 16;
type ReadCheckpoint = (Zeroizing<Vec<u8>>, [u8; 32], u64, u64);

struct Journal {
    file: Option<File>,
    last: Zeroizing<Vec<u8>>,
    tail: [u8; 32],
    length: u64,
    generation: u64,
    failed: bool,
}

pub struct ServiceLeaseFileV1 {
    cipher: XChaCha20Poly1305,
    state: Mutex<Journal>,
    path: PathBuf,
    // Stable inode across replacements. Never delete this lock file, including
    // after crashes; data-file locks also exclude existing v1 writers at open.
    _owner: File,
}

fn cipher(material: Zeroizing<[u8; 32]>) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new((&*material).into())
}

fn check_file(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "journal must be a regular file"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "journal reparse point"
        );
    }
    Ok(())
}

impl ServiceLeaseFileV1 {
    pub fn create(path: &Path, material: Zeroizing<[u8; 32]>, initial: &[u8]) -> Result<Arc<Self>> {
        ensure!(
            !initial.is_empty() && initial.len() <= MAX_SERVICE_CHECKPOINT_BYTES,
            "journal state size"
        );
        let path = absolute_path(path)?;
        let owner = lock_owner(&path)?;
        let header = header(0, [0; 32]);
        let reservation = novovm_storage_budget::reserve_for_paths(
            std::slice::from_ref(&path),
            header.len() as u64,
        )?;
        let mut file = private_options().create_new(true).open(&path)?;
        lock_file(&file)?;
        file.write_all(&header)?;
        file.sync_all()?;
        sync_parent(&path)?;
        reservation.finish()?;
        let store = Arc::new(Self {
            cipher: cipher(material),
            state: Mutex::new(Journal {
                file: Some(file),
                last: Zeroizing::new(Vec::new()),
                tail: Sha256::digest(&header).into(),
                length: header.len() as u64,
                generation: 0,
                failed: false,
            }),
            path,
            _owner: owner,
        });
        store.commit(&[], initial)?;
        Ok(store)
    }

    pub fn open(path: &Path, material: Zeroizing<[u8; 32]>) -> Result<(Arc<Self>, Vec<u8>)> {
        let path = absolute_path(path)?;
        let owner = lock_owner(&path)?;
        check_file(&path)?;
        let mut file = private_options().open(&path)?;
        lock_file(&file)?;
        let cipher = cipher(material);
        let (last, tail, length, generation) = read(&mut file, &cipher)?;
        ensure!(!last.is_empty(), "journal has no enrolled state");
        let saved = last.to_vec();
        Ok((
            Arc::new(Self {
                cipher,
                state: Mutex::new(Journal {
                    file: Some(file),
                    last,
                    tail,
                    length,
                    generation,
                    failed: false,
                }),
                path,
                _owner: owner,
            }),
            saved,
        ))
    }

    fn compact(&self, state: &mut Journal) -> Result<()> {
        self.compact_with(state, |_| Ok(()))
    }

    // Hooks are private and used by real child-process crash tests. Only the
    // authoritative file is ever read on recovery; .compact is never a fallback.
    fn compact_with(&self, state: &mut Journal, hook: impl Fn(u8) -> Result<()>) -> Result<()> {
        let staged_path = sibling(&self.path, ".compact");
        if staged_path.try_exists()? {
            check_file(&staged_path)?;
            ensure!(
                std::fs::metadata(&staged_path)?.len() <= MAX_JOURNAL,
                "staged journal size"
            );
            // A crash before publication can leave only an uncommitted copy of
            // the SAME checkpoint. Caller has authenticated the real file first.
            std::fs::remove_file(&staged_path)?;
        }
        let mut staged = private_options().create_new(true).open(&staged_path)?;
        lock_file(&staged)?;
        hook(1)?;
        let generation = state
            .generation
            .checked_add(1)
            .context("journal generation exhausted")?;
        let header = header(generation, state.tail);
        staged.write_all(&header)?;
        let frame = encrypt(&self.cipher, &state.last, &Sha256::digest(&header).into())?;
        staged.write_all(&(frame.len() as u32).to_le_bytes())?;
        staged.write_all(&frame)?;
        hook(2)?;
        staged.sync_all()?;
        hook(3)?;
        let (saved, tail, length, saved_generation) = read(&mut staged, &self.cipher)?;
        ensure!(
            saved.as_slice() == state.last.as_slice() && saved_generation == generation,
            "compacted checkpoint differs"
        );
        // Windows denies replacement while these data handles are open (even
        // after unlocking). Close them under the stable .lock. Installed owners
        // additionally retain their original process lock across migration;
        // old binaries must be stopped, never run mixed-version file owners.
        drop(staged);
        drop(state.file.take().context("journal file unavailable")?);
        hook(6)?;
        publish(&staged_path, &self.path).context("publish compacted journal")?;
        hook(4)?;
        sync_parent(&self.path)?;
        hook(5)?;
        let mut published = private_options().open(&self.path)?;
        lock_file(&published)?;
        let (checked, checked_tail, checked_length, checked_generation) =
            read(&mut published, &self.cipher)?;
        ensure!(
            checked.as_slice() == saved.as_slice()
                && checked_tail == tail
                && checked_length == length
                && checked_generation == generation,
            "published checkpoint differs"
        );
        // Stable owner spans publication and acquiring the new data-file lock.
        state.file = Some(published);
        state.tail = tail;
        state.length = length;
        state.generation = generation;
        Ok(())
    }
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok(parent
        .canonicalize()?
        .join(path.file_name().context("journal filename")?))
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    name.into()
}

fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

fn lock_owner(path: &Path) -> Result<File> {
    let path = sibling(path, ".lock");
    if path.try_exists()? {
        check_file(&path)?;
    }
    let file = private_options().create(true).truncate(false).open(&path)?;
    lock_file(&file)?;
    check_file(&path)?;
    Ok(file)
}

fn lock_file(file: &File) -> Result<()> {
    #[cfg(target_os = "android")]
    rustix::fs::flock(file, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        .context("journal exclusive lock unavailable")?;
    #[cfg(not(target_os = "android"))]
    file.try_lock()
        .context("journal exclusive lock unavailable")?;
    Ok(())
}

fn header(generation: u64, predecessor: [u8; 32]) -> Vec<u8> {
    [MAGIC.as_slice(), &generation.to_le_bytes(), &predecessor].concat()
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<()> {
    File::open(path.parent().context("journal parent")?)?.sync_all()?;
    Ok(())
}
#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn publish(from: &Path, to: &Path) -> Result<()> {
    // Same-directory replacement, never cross-volume copy/delete. The flushed
    // snapshot and write-through rename precede success. Keep the network crate
    // unsafe-free; this reviewed adapter uses MoveFileExW(REPLACE|WRITE_THROUGH).
    // Installed directory ACL remains the custody boundary. No delete+rename.
    atomicwrites::replace_atomic(from, to)?;
    Ok(())
}
#[cfg(not(windows))]
fn publish(from: &Path, to: &Path) -> Result<()> {
    std::fs::rename(from, to)?;
    Ok(())
}

fn encrypt(cipher: &XChaCha20Poly1305, bytes: &[u8], tail: &[u8; 32]) -> Result<Vec<u8>> {
    let mut nonce = [0; 24];
    OsRng.fill_bytes(&mut nonce);
    let encrypted = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: bytes,
                aad: &aad(tail),
            },
        )
        .map_err(|_| anyhow::anyhow!("journal encryption failed"))?;
    Ok([nonce.as_slice(), &encrypted].concat())
}

fn aad(tail: &[u8; 32]) -> Vec<u8> {
    [DOMAIN, tail].concat()
}

fn read(file: &mut File, cipher: &XChaCha20Poly1305) -> Result<ReadCheckpoint> {
    let length = file.metadata()?.len();
    ensure!((8..=MAX_JOURNAL).contains(&length), "journal size");
    file.seek(SeekFrom::Start(0))?;
    let mut header = [0; 8];
    file.read_exact(&mut header)?;
    let mut offset = 8;
    let mut tail = [0; 32];
    let generation = if header == *LEGACY_MAGIC {
        0
    } else {
        ensure!(header == *MAGIC, "journal version");
        let mut anchor = [0; 40];
        file.read_exact(&mut anchor)?;
        tail = Sha256::digest([header.as_slice(), &anchor].concat()).into();
        offset += 40;
        u64::from_le_bytes(anchor[..8].try_into()?)
    };
    let mut last = Zeroizing::new(Vec::new());
    while offset < length {
        let mut size = [0; 4];
        file.read_exact(&mut size)?;
        let size = u32::from_le_bytes(size) as usize;
        ensure!(
            (41..=MAX_FRAME).contains(&size) && offset + 4 + size as u64 <= length,
            "journal frame size/torn write"
        );
        let mut frame = vec![0; size];
        file.read_exact(&mut frame)?;
        last = Zeroizing::new(
            cipher
                .decrypt(
                    XNonce::from_slice(&frame[..24]),
                    Payload {
                        msg: &frame[24..],
                        aad: &aad(&tail),
                    },
                )
                .map_err(|_| anyhow::anyhow!("journal authentication failed"))?,
        );
        tail = Sha256::digest(&frame).into();
        offset += 4 + size as u64;
    }
    Ok((last, tail, length, generation))
}

impl ServiceLeasePersistenceV1 for ServiceLeaseFileV1 {
    fn commit(&self, expected: &[u8], next: &[u8]) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("journal unavailable"))?;
        ensure!(!state.failed, "journal stopped after uncertain write");
        ensure!(
            !next.is_empty() && next.len() <= MAX_SERVICE_CHECKPOINT_BYTES,
            "journal state size"
        );
        // Admission happens before touching the journal. A known quota refusal
        // is not an uncertain signing-state write and must not poison the owner.
        // Include the simultaneous compact copy and the next encrypted frame.
        let compact_bytes = if state.length >= COMPACT_AT {
            48 + 4 + 24 + 16 + state.last.len() as u64
        } else {
            0
        };
        let append_bytes = if expected == next {
            0
        } else {
            4 + 24 + 16 + next.len() as u64
        };
        let reservation = novovm_storage_budget::reserve_for_paths(
            &[self.path.clone(), sibling(&self.path, ".compact")],
            compact_bytes + append_bytes,
        )?;
        let result = (|| {
            ensure!(
                !next.is_empty() && next.len() <= MAX_SERVICE_CHECKPOINT_BYTES,
                "journal state size"
            );
            check_file(&self.path)?;
            let (disk, tail, length, generation) = read(
                state.file.as_mut().context("journal file unavailable")?,
                &self.cipher,
            )?;
            ensure!(
                disk.as_slice() == expected
                    && disk.as_slice() == state.last.as_slice()
                    && tail == state.tail
                    && length == state.length
                    && generation == state.generation,
                "stale or replaced journal"
            );
            if length >= COMPACT_AT {
                self.compact(&mut state)?;
            }
            if expected == next {
                return Ok(());
            }
            let frame = encrypt(&self.cipher, next, &state.tail)?;
            let next_length = state.length + 4 + frame.len() as u64;
            ensure!(next_length <= MAX_JOURNAL, "journal budget exhausted");
            let file = state.file.as_mut().context("journal file unavailable")?;
            file.seek(SeekFrom::End(0))?;
            file.write_all(&(frame.len() as u32).to_le_bytes())?;
            file.write_all(&frame)?;
            file.sync_all()?;
            state.tail = Sha256::digest(&frame).into();
            state.length = next_length;
            state.last = Zeroizing::new(next.to_vec());
            Ok(())
        })();
        if result.is_err() {
            state.failed = true;
        }
        // Reconcile even a failed/partial write; never return a successful
        // checkpoint before its existing fsync and state checks have completed.
        let accounted = reservation.finish();
        result?;
        Ok(accounted?)
    }
}

#[cfg(test)]
#[path = "product_service_file_tests.rs"]
mod tests;
