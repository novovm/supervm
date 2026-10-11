//! Bounded encrypted append journal for an installed service's single owner.
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
    path::Path,
};
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"NVLSJ001";
const DOMAIN: &[u8] = b"novovm.service-journal.v1\0";
// Intentionally finite. Rotation/compaction needs a separate crash-safe owner
// operation; never truncate old authorization history to keep a service alive.
const MAX_JOURNAL: u64 = 16 * 1024 * 1024;
const MAX_FRAME: usize = MAX_SERVICE_CHECKPOINT_BYTES + 24 + 16;

struct Journal {
    file: File,
    last: Zeroizing<Vec<u8>>,
    tail: [u8; 32],
    length: u64,
    failed: bool,
}

pub struct ServiceLeaseFileV1 {
    cipher: XChaCha20Poly1305,
    state: Mutex<Journal>,
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
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.try_lock()
            .map_err(|_| anyhow::anyhow!("journal already owned"))?;
        file.write_all(MAGIC)?;
        file.sync_all()?;
        let store = Arc::new(Self {
            cipher: cipher(material),
            state: Mutex::new(Journal {
                file,
                last: Zeroizing::new(Vec::new()),
                tail: [0; 32],
                length: 8,
                failed: false,
            }),
        });
        store.commit(&[], initial)?;
        Ok(store)
    }

    pub fn open(path: &Path, material: Zeroizing<[u8; 32]>) -> Result<(Arc<Self>, Vec<u8>)> {
        check_file(path)?;
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        file.try_lock()
            .map_err(|_| anyhow::anyhow!("journal already owned"))?;
        let cipher = cipher(material);
        let (last, tail, length) = read(&mut file, &cipher)?;
        ensure!(!last.is_empty(), "journal has no enrolled state");
        let saved = last.to_vec();
        Ok((
            Arc::new(Self {
                cipher,
                state: Mutex::new(Journal {
                    file,
                    last,
                    tail,
                    length,
                    failed: false,
                }),
            }),
            saved,
        ))
    }
}

fn aad(tail: &[u8; 32]) -> Vec<u8> {
    [DOMAIN, tail].concat()
}

fn read(
    file: &mut File,
    cipher: &XChaCha20Poly1305,
) -> Result<(Zeroizing<Vec<u8>>, [u8; 32], u64)> {
    let length = file.metadata()?.len();
    ensure!((8..=MAX_JOURNAL).contains(&length), "journal size");
    file.seek(SeekFrom::Start(0))?;
    let mut header = [0; 8];
    file.read_exact(&mut header)?;
    ensure!(header == *MAGIC, "journal version");
    let mut offset = 8;
    let mut tail = [0; 32];
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
    Ok((last, tail, length))
}

impl ServiceLeasePersistenceV1 for ServiceLeaseFileV1 {
    fn commit(&self, expected: &[u8], next: &[u8]) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("journal unavailable"))?;
        ensure!(!state.failed, "journal stopped after uncertain write");
        let result = (|| {
            ensure!(
                !next.is_empty() && next.len() <= MAX_SERVICE_CHECKPOINT_BYTES,
                "journal state size"
            );
            let (disk, tail, length) = read(&mut state.file, &self.cipher)?;
            ensure!(
                disk.as_slice() == expected
                    && disk.as_slice() == state.last.as_slice()
                    && tail == state.tail
                    && length == state.length,
                "stale or replaced journal"
            );
            if expected == next {
                return Ok(());
            }
            let mut nonce = [0; 24];
            OsRng.fill_bytes(&mut nonce);
            let encrypted = self
                .cipher
                .encrypt(
                    XNonce::from_slice(&nonce),
                    Payload {
                        msg: next,
                        aad: &aad(&tail),
                    },
                )
                .map_err(|_| anyhow::anyhow!("journal encryption failed"))?;
            let frame = [nonce.as_slice(), &encrypted].concat();
            let next_length = length + 4 + frame.len() as u64;
            ensure!(next_length <= MAX_JOURNAL, "journal budget exhausted");
            state.file.seek(SeekFrom::End(0))?;
            state.file.write_all(&(frame.len() as u32).to_le_bytes())?;
            state.file.write_all(&frame)?;
            state.file.sync_all()?;
            state.tail = Sha256::digest(&frame).into();
            state.length = next_length;
            state.last = Zeroizing::new(next.to_vec());
            Ok(())
        })();
        if result.is_err() {
            state.failed = true;
        }
        result
    }
}
