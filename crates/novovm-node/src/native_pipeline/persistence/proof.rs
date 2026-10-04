//! Retained TEST-ONLY immutable attachment codec from `387bf0b7`.
//! The backend-specific product service and its I/O lane have been retired.
//! This regression asset is not a second product proof/storage entry point.
//! Bytes here are opaque and never confer execution, signing or finality rights.
//! Future reuse must verify the unified semantic proof contract before writing
//! and after reading. This codec only binds location and exact bytes.

use super::store::BULK_KEYS;
use super::CandidateStore;
use crate::native_pipeline::state::tree::NodeHash;
use anyhow::{ensure, Context, Result};
use novovm_exec::resident::StorageWrite;
use sha2::{Digest, Sha256};

pub(crate) const MAX_PROOF_BLOB_BYTES: usize = 16 * 1024 * 1024 + 4096;
const CHUNK_BYTES: usize = 64 * 1024;
const MAX_CHUNKS: usize = MAX_PROOF_BLOB_BYTES.div_ceil(CHUNK_BYTES);
const MAGIC: &[u8; 8] = b"NVPRST01";
const MARKER_BYTES: usize = 8 + 32 + 32 + 8 + 4 + 32;
const PREFIX: &[u8] = b"p/execution/v1/";

pub(crate) fn validate_identity(candidate: NodeHash, image: [u32; 8]) -> Result<()> {
    ensure!(candidate != [0; 32], "proof candidate is not configured");
    ensure!(image != [0; 8], "proof image is not configured");
    Ok(())
}

pub(crate) fn validate_blob(blob: &[u8]) -> Result<()> {
    ensure!(
        !blob.is_empty() && blob.len() <= MAX_PROOF_BLOB_BYTES,
        "proof attachment size exceeds bound"
    );
    Ok(())
}

fn image_bytes(image: [u32; 8]) -> [u8; 32] {
    let mut bytes = [0; 32];
    for (word, out) in image.iter().zip(bytes.chunks_exact_mut(4)) {
        out.copy_from_slice(&word.to_be_bytes());
    }
    bytes
}

fn key_prefix(candidate: NodeHash, image: [u32; 8]) -> Vec<u8> {
    [PREFIX, candidate.as_slice(), &image_bytes(image)].concat()
}

fn marker_key(candidate: NodeHash, image: [u32; 8]) -> Vec<u8> {
    [key_prefix(candidate, image).as_slice(), b"/complete"].concat()
}

fn chunk_key(candidate: NodeHash, image: [u32; 8], index: usize) -> Vec<u8> {
    [
        key_prefix(candidate, image).as_slice(),
        b"/data/",
        &(index as u32).to_be_bytes(),
    ]
    .concat()
}

struct Marker {
    len: usize,
    chunks: usize,
    digest: NodeHash,
}

impl Marker {
    fn from_blob(blob: &[u8]) -> Result<Self> {
        validate_blob(blob)?;
        Ok(Self {
            len: blob.len(),
            chunks: blob.len().div_ceil(CHUNK_BYTES),
            digest: Sha256::digest(blob).into(),
        })
    }

    fn encode(&self, candidate: NodeHash, image: [u32; 8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(MARKER_BYTES);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&candidate);
        out.extend_from_slice(&image_bytes(image));
        out.extend_from_slice(&(self.len as u64).to_be_bytes());
        out.extend_from_slice(&(self.chunks as u32).to_be_bytes());
        out.extend_from_slice(&self.digest);
        out
    }

    fn decode(bytes: &[u8], candidate: NodeHash, image: [u32; 8]) -> Result<Self> {
        ensure!(
            bytes.len() == MARKER_BYTES && &bytes[..8] == MAGIC,
            "proof attachment marker format mismatch"
        );
        ensure!(
            bytes[8..40] == candidate && bytes[40..72] == image_bytes(image),
            "proof attachment marker identity mismatch"
        );
        let len = usize::try_from(u64::from_be_bytes(bytes[72..80].try_into()?))?;
        let chunks = usize::try_from(u32::from_be_bytes(bytes[80..84].try_into()?))?;
        ensure!(
            (1..=MAX_PROOF_BLOB_BYTES).contains(&len) && chunks == len.div_ceil(CHUNK_BYTES),
            "proof attachment marker length/chunks exceed bound"
        );
        Ok(Self {
            len,
            chunks,
            digest: bytes[84..116].try_into()?,
        })
    }
}

fn read_exact(
    read: &mut impl FnMut(&[Vec<u8>]) -> Result<Vec<Option<Vec<u8>>>>,
    keys: &[Vec<u8>],
) -> Result<Vec<Option<Vec<u8>>>> {
    ensure!(
        !keys.is_empty() && keys.len() <= BULK_KEYS,
        "proof attachment read exceeds key bound"
    );
    let values = read(keys)?;
    ensure!(
        values.len() == keys.len(),
        "proof attachment read count mismatch"
    );
    Ok(values)
}

fn read_blob(
    candidate: NodeHash,
    image: [u32; 8],
    mut read: impl FnMut(&[Vec<u8>]) -> Result<Vec<Option<Vec<u8>>>>,
) -> Result<Option<Vec<u8>>> {
    validate_identity(candidate, image)?;
    let raw = read_exact(&mut read, &[marker_key(candidate, image)])?
        .pop()
        .flatten();
    let marker = raw
        .as_deref()
        .map(|bytes| Marker::decode(bytes, candidate, image))
        .transpose()?;
    let mut blob = Vec::with_capacity(marker.as_ref().map_or(0, |marker| marker.len));
    // Scan the complete bounded chunk namespace plus a sentinel, not just the
    // first expected chunk. Absent marker + orphan chunks is corruption, not a
    // retry that may overwrite partial content. No prefix scan or repair occurs.
    for start in (0..=MAX_CHUNKS).step_by(BULK_KEYS) {
        let end = (start + BULK_KEYS).min(MAX_CHUNKS + 1);
        let keys: Vec<_> = (start..end)
            .map(|index| chunk_key(candidate, image, index))
            .collect();
        for (index, value) in (start..end).zip(read_exact(&mut read, &keys)?) {
            if let Some(marker) = &marker {
                if index < marker.chunks {
                    let bytes = value.context("proof attachment chunk missing")?;
                    let expected = CHUNK_BYTES.min(marker.len - index * CHUNK_BYTES);
                    ensure!(
                        bytes.len() == expected,
                        "proof attachment chunk length mismatch"
                    );
                    blob.extend_from_slice(&bytes);
                    continue;
                }
            }
            ensure!(value.is_none(), "proof attachment has orphan/tail chunk");
        }
    }
    match marker {
        Some(marker) => {
            ensure!(
                blob.len() == marker.len
                    && <NodeHash>::from(Sha256::digest(&blob)) == marker.digest,
                "proof attachment digest mismatch"
            );
            Ok(Some(blob))
        }
        None => Ok(None),
    }
}

impl CandidateStore {
    /// Read-back bytes only. The caller must independently verify the pinned
    /// receipt and published candidate; this does not recover a capability.
    pub(crate) fn read_proof(
        &self,
        candidate: NodeHash,
        image: [u32; 8],
    ) -> Result<Option<Vec<u8>>> {
        read_blob(candidate, image, |keys| self.read_relative(keys))
    }

    /// Exact immutable replay returns true; a new atomic attachment returns
    /// false. A different blob never replaces even another valid proof. Chunk
    /// and total native limits remain enforced by the original storage adapter.
    /// The synchronous WAL write/readback is bounded I/O, not a hard latency SLA.
    pub(crate) fn write_proof(
        &self,
        candidate: NodeHash,
        image: [u32; 8],
        blob: &[u8],
    ) -> Result<bool> {
        self.writable()?;
        validate_identity(candidate, image)?;
        let marker = Marker::from_blob(blob)?;
        if let Some(existing) = self.read_proof(candidate, image)? {
            ensure!(existing == blob, "immutable proof attachment conflict");
            return Ok(true);
        }
        let mut writes: Vec<_> = blob
            .chunks(CHUNK_BYTES)
            .enumerate()
            .map(|(index, bytes)| StorageWrite::Put {
                key: self.scoped_key(&chunk_key(candidate, image, index)),
                value: bytes.to_vec(),
            })
            .collect();
        writes.push(StorageWrite::Put {
            key: self.scoped_key(&marker_key(candidate, image)),
            value: marker.encode(candidate, image),
        });
        // One owner operation and one native WAL-synchronous atomic batch for
        // ALL chunks plus marker. Native unknown writes poison that session.
        self.storage.borrow_mut().atomic_write_batch(&writes)?;
        match self.read_proof(candidate, image) {
            Ok(Some(actual)) if actual == blob => Ok(false),
            Ok(_) => {
                self.write_frozen.set(true);
                anyhow::bail!("proof attachment readback mismatch; recovery required")
            }
            Err(error) => {
                self.write_frozen.set(true);
                Err(error.context("proof attachment readback failed; recovery required"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const CANDIDATE: NodeHash = [3; 32];
    const IMAGE: [u32; 8] = [0x01020304; 8];

    fn records(blob: &[u8]) -> BTreeMap<Vec<u8>, Vec<u8>> {
        let mut records = BTreeMap::new();
        for (index, bytes) in blob.chunks(CHUNK_BYTES).enumerate() {
            records.insert(chunk_key(CANDIDATE, IMAGE, index), bytes.to_vec());
        }
        records.insert(
            marker_key(CANDIDATE, IMAGE),
            Marker::from_blob(blob).unwrap().encode(CANDIDATE, IMAGE),
        );
        records
    }

    fn load(records: &BTreeMap<Vec<u8>, Vec<u8>>) -> Result<Option<Vec<u8>>> {
        read_blob(CANDIDATE, IMAGE, |keys| {
            assert!(keys.len() <= BULK_KEYS);
            Ok(keys.iter().map(|key| records.get(key).cloned()).collect())
        })
    }

    #[test]
    fn proof_storage_marker_binds_version_candidate_image_and_resource_bounds() {
        let marker = Marker::from_blob(&vec![5; CHUNK_BYTES + 1]).unwrap();
        let encoded = marker.encode(CANDIDATE, IMAGE);
        assert_eq!(encoded.len(), MARKER_BYTES);
        assert_eq!(&encoded[40..44], &[1, 2, 3, 4]);
        let decoded = Marker::decode(&encoded, CANDIDATE, IMAGE).unwrap();
        assert_eq!(decoded.len, CHUNK_BYTES + 1);
        assert_eq!(decoded.chunks, 2);
        for index in [0, 8, 40, 72, 80] {
            let mut bad = encoded.clone();
            bad[index] ^= 1;
            assert!(Marker::decode(&bad, CANDIDATE, IMAGE).is_err());
        }
        assert!(Marker::decode(&encoded[..MARKER_BYTES - 1], CANDIDATE, IMAGE).is_err());
        assert!(Marker::decode(&[encoded, vec![0]].concat(), CANDIDATE, IMAGE).is_err());
        assert!(Marker::from_blob(&[]).is_err());
        assert!(Marker::from_blob(&vec![0; MAX_PROOF_BLOB_BYTES + 1]).is_err());
        assert!(validate_identity([0; 32], IMAGE).is_err());
        assert!(validate_identity(CANDIDATE, [0; 8]).is_err());
    }

    #[test]
    fn proof_storage_chunk_roundtrip_empty_and_maximum_preserve_exact_bytes() {
        assert_eq!(load(&BTreeMap::new()).unwrap(), None);
        for len in [1, CHUNK_BYTES, CHUNK_BYTES + 1, MAX_PROOF_BLOB_BYTES] {
            let blob = vec![9; len];
            assert_eq!(load(&records(&blob)).unwrap(), Some(blob));
        }
        let first = chunk_key(CANDIDATE, IMAGE, 1);
        assert!(first.starts_with(PREFIX));
        assert_ne!(first, marker_key(CANDIDATE, IMAGE));
        assert_ne!(first, chunk_key([4; 32], IMAGE, 1));
        assert_ne!(first, chunk_key(CANDIDATE, [4; 8], 1));
        assert_ne!(first, chunk_key(CANDIDATE, IMAGE, 2));
        assert!(![b'n', b'd', b'c', b'm'].contains(&first[0]));
    }

    #[test]
    fn proof_storage_missing_corrupt_orphan_and_late_tail_never_return_success() {
        let blob = vec![8; CHUNK_BYTES + 7];
        let original = records(&blob);
        let mut bad = original.clone();
        bad.remove(&chunk_key(CANDIDATE, IMAGE, 0));
        assert!(load(&bad).is_err());
        let mut bad = original.clone();
        bad.get_mut(&chunk_key(CANDIDATE, IMAGE, 0)).unwrap()[0] ^= 1;
        assert!(load(&bad).is_err());
        let mut bad = original.clone();
        bad.get_mut(&chunk_key(CANDIDATE, IMAGE, 1))
            .unwrap()
            .push(0);
        assert!(load(&bad).is_err());
        let mut bad = original.clone();
        bad.remove(&marker_key(CANDIDATE, IMAGE));
        assert!(load(&bad).is_err());
        let mut bad = original;
        bad.insert(chunk_key(CANDIDATE, IMAGE, MAX_CHUNKS), vec![1]);
        assert!(load(&bad).is_err());
        assert!(read_blob(CANDIDATE, IMAGE, |_| Ok(Vec::new())).is_err());
    }

    #[test]
    #[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; real opaque attachment storage, not cryptographic proof"]
    fn real_proof_storage_multichunk_reopen_immutable_and_corruption_rejection() -> Result<()> {
        use crate::native_pipeline::persistence::{
            OpenMode, PacketBudget, StorageDomain, StoreConfig,
        };
        use novovm_exec::resident::StorageConfig;
        use std::time::{SystemTime, UNIX_EPOCH};

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/resident-proof-storage-tests");
        std::fs::create_dir_all(&root)?;
        let database = root.join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        let config = StoreConfig {
            library: std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
                .context("explicit trusted NOVOVM_AOEM_TEST_LIBRARY is required")?
                .into(),
            database,
            domain: StorageDomain {
                chain_id: 9,
                genesis_config_commitment: [2; 32],
                protocol_commitment: [3; 32],
            },
            storage: StorageConfig::default(),
            packet_budget: PacketBudget::default(),
        };
        // Deliberately opaque storage fixture: no receipt, executed candidate,
        // proof validity or published head is fabricated by this test.
        let blob = vec![0x51; 2 * 1024 * 1024 + 7];
        {
            let store = CandidateStore::open(config.clone(), OpenMode::CreateNew)?;
            assert!(store.read_proof(CANDIDATE, IMAGE)?.is_none());
            assert!(!store.write_proof(CANDIDATE, IMAGE, &blob)?);
            assert!(store.write_proof(CANDIDATE, IMAGE, &blob)?);
            assert_eq!(store.read_proof(CANDIDATE, IMAGE)?, Some(blob.clone()));
            assert!(store.write_proof(CANDIDATE, IMAGE, b"different").is_err());
            assert!(!store.is_write_frozen());
            assert!(store.read_proof([4; 32], IMAGE)?.is_none());
            assert!(store.read_proof(CANDIDATE, [4; 8])?.is_none());
            assert_eq!(
                store
                    .read_metadata(&[super::super::metadata::MetaKey::ChainHead])?
                    .values,
                vec![None]
            );
        }
        let store = CandidateStore::open(config, OpenMode::Existing)?;
        assert_eq!(store.read_proof(CANDIDATE, IMAGE)?, Some(blob));
        // Fixture-only corruption, never production repair or retry.
        store
            .storage
            .borrow_mut()
            .atomic_write_batch(&[StorageWrite::Delete {
                key: store.scoped_key(&chunk_key(CANDIDATE, IMAGE, 0)),
            }])?;
        assert!(store.read_proof(CANDIDATE, IMAGE).is_err());
        assert!(store.write_proof(CANDIDATE, IMAGE, b"replacement").is_err());
        Ok(())
    }
}
