#![forbid(unsafe_code)]

//! Read-only export and portable framing of legacy nonce checkpoint evidence.
//! Inputs must be stopped-node copies. No AOEM runtime, Host recovery loader,
//! writable ledger opens, migration activation, or persistence defaults are invoked.
//! Shared ledger reads may reuse a process-local handle; this wrapper is read-only.
//! RocksDB source decoding assumes trusted coherent copies, not hostile DB files.

use super::native_nonce_checkpoint::{
    verify_nonce_checkpoint_v1, NonceMigrationCheckpointReportV1, NonceMigrationCheckpointV1,
    MAX_CHECKPOINT_BLOCKS_V1,
};
use super::native_nonce_migration::{
    MAX_HISTORY_BYTES_V1, MAX_HISTORY_TRANSACTIONS_V1, MAX_SNAPSHOT_BYTES_V1,
};
use super::*;
use crate::native_block_ledger::NovNativeBlockLedgerHeadV1;
use std::io::{Read, Write};

const MAGIC_V1: &[u8; 8] = b"NVNCPK1\0";
pub const MAX_CHECKPOINT_BUNDLE_BYTES_V1: usize = 96 * 1024 * 1024;
const MAX_HEAD_JSON_BYTES_V1: usize = 8 * 1024;
const MAX_BLOCK_JSON_BYTES_V1: usize = 16 * 1024 * 1024;

#[derive(Debug, serde::Serialize)]
pub struct NonceMigrationSourceInspectionV1 {
    pub schema: &'static str,
    pub checkpoint: NonceMigrationCheckpointV1,
    pub head: NovNativeBlockLedgerHeadV1,
    pub observed_only: bool,
    pub independent_provenance_verified: bool,
    pub execution_replayed: bool,
    pub aoem_evidence_verified: bool,
    pub qc_verified: bool,
    pub activation_ready: bool,
    pub import_performed: bool,
}

fn read_regular_file_bounded(path: &Path, max: usize) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("read offline input metadata: {}", path.display()))?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > max as u64 {
        bail!("offline input must be a nonempty regular file within {max} bytes");
    }
    let file = fs::File::open(path)
        .with_context(|| format!("open offline input read-only: {}", path.display()))?;
    if !file.metadata()?.is_file() {
        bail!("offline input changed to a non-regular file");
    }
    let mut bytes = Vec::new();
    file.take(max as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > max {
        bail!("offline input changed or exceeded {max} bytes while reading");
    }
    Ok(bytes)
}

fn inspect_source(
    snapshot_path: &Path,
    ledger_path: &Path,
    chain_id: u64,
) -> Result<(
    Vec<u8>,
    NovNativeBlockLedgerV1,
    NonceMigrationSourceInspectionV1,
)> {
    if chain_id == 0 {
        bail!("offline checkpoint chain must be nonzero");
    }
    let snapshot = read_regular_file_bounded(snapshot_path, MAX_SNAPSHOT_BYTES_V1)?;
    let store: NovNativeExecutionStoreV1 =
        serde_json::from_slice(&snapshot).context("decode copied legacy Host JSON snapshot")?;
    if store.schema != NOV_NATIVE_EXECUTION_STORE_SCHEMA_V1
        || store.authority_chain_id != Some(chain_id)
        || !store
            .module_state
            .native_auth_nonce_identity_scheme
            .is_empty()
    {
        bail!("offline source requires an explicitly bound, complete legacy Host snapshot");
    }
    let ledger = NovNativeBlockLedgerV1::open_existing_read_only(ledger_path)?
        .context("offline source ledger is missing; no database was created")?;
    if ledger.load_prepared(chain_id)?.is_some() {
        bail!("offline source has an unfinished prepared block; complete recovery before copying");
    }
    let ownership = ledger
        .load_aoem_ownership()?
        .context("offline ledger ownership is missing")?;
    if ownership.chain_id != chain_id
        || ownership.namespace_digest != store.authority_namespace_digest
        || ownership.protocol_config_commitment != store.module_state.protocol_config_commitment
    {
        bail!("offline snapshot and ledger ownership domains differ");
    }
    let head = ledger
        .load_head(chain_id)?
        .context("offline source ledger has no durable head")?;
    if head.height as u128 > MAX_CHECKPOINT_BLOCKS_V1 as u128
        || head.cumulative_tx_count as u128 > MAX_HISTORY_TRANSACTIONS_V1 as u128
        || head.cumulative_body_bytes as u128 > MAX_HISTORY_BYTES_V1 as u128
    {
        bail!("offline ledger head exceeds checkpoint export limits");
    }
    let checkpoint = NonceMigrationCheckpointV1 {
        chain_id,
        namespace_digest: ownership.namespace_digest,
        legacy_protocol_config_commitment: ownership.protocol_config_commitment,
        tip_block_hash: to_hex(&head.block_hash),
        snapshot_digest: to_hex(&sha256_bytes_v1(&[
            b"novovm-native-nonce-migration-source-snapshot-v1\0",
            &snapshot,
        ])),
    };
    Ok((
        snapshot,
        ledger,
        NonceMigrationSourceInspectionV1 {
            schema: "novovm-native-nonce-source-inspection/v1",
            checkpoint,
            head,
            observed_only: true,
            independent_provenance_verified: false,
            execution_replayed: false,
            aoem_evidence_verified: false,
            qc_verified: false,
            activation_ready: false,
            import_performed: false,
        },
    ))
}

/// Observe fingerprints of explicit copied inputs. Inspection is NOT provenance
/// verification: do not trust these values merely because this tool printed them.
pub fn inspect_nonce_checkpoint_source_v1(
    snapshot_path: &Path,
    ledger_path: &Path,
    chain_id: u64,
) -> Result<NonceMigrationSourceInspectionV1> {
    Ok(inspect_source(snapshot_path, ledger_path, chain_id)?.2)
}

struct BoundedBytes {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for BoundedBytes {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other(
                "checkpoint encoding byte bound exceeded",
            ));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn encode_json_bounded<T: serde::Serialize>(value: &T, max: usize) -> Result<Vec<u8>> {
    let mut writer = BoundedBytes {
        bytes: Vec::new(),
        limit: max,
    };
    serde_json::to_writer(&mut writer, value).context("encode bounded checkpoint JSON frame")?;
    Ok(writer.bytes)
}

fn append_frame(output: &mut BoundedBytes, payload: &[u8]) -> Result<()> {
    let len = u32::try_from(payload.len()).context("checkpoint frame length overflow")?;
    output.write_all(&len.to_le_bytes())?;
    output.write_all(payload)?;
    Ok(())
}

pub(super) fn encode_bundle(
    snapshot: &[u8],
    head: &NovNativeBlockLedgerHeadV1,
    blocks: &[NovNativeDurableBlockV1],
) -> Result<Vec<u8>> {
    let mut out = BoundedBytes {
        bytes: Vec::new(),
        limit: MAX_CHECKPOINT_BUNDLE_BYTES_V1,
    };
    out.write_all(MAGIC_V1)?;
    append_frame(&mut out, snapshot)?;
    append_frame(
        &mut out,
        &encode_json_bounded(head, MAX_HEAD_JSON_BYTES_V1)?,
    )?;
    out.write_all(&u32::try_from(blocks.len())?.to_le_bytes())?;
    for block in blocks {
        append_frame(
            &mut out,
            &encode_json_bounded(block, MAX_BLOCK_JSON_BYTES_V1)?,
        )?;
    }
    Ok(out.bytes)
}

/// Export only after full local-history validation. Does not create an output
/// file or mutate source ledger entries/snapshot bytes. This is not a claim
/// about RocksDB diagnostic files. The CLI publishes bytes using create_new.
pub fn export_nonce_checkpoint_bundle_v1(
    snapshot_path: &Path,
    ledger_path: &Path,
    checkpoint: &NonceMigrationCheckpointV1,
) -> Result<Vec<u8>> {
    let (snapshot, ledger, observed) =
        inspect_source(snapshot_path, ledger_path, checkpoint.chain_id)?;
    if observed.checkpoint != *checkpoint {
        bail!("offline source does not match the independently supplied checkpoint anchors");
    }
    let mut blocks = Vec::new();
    let mut raw_bytes = 0usize;
    let mut txs = 0usize;
    let mut encoded_bytes = MAGIC_V1.len() + snapshot.len() + MAX_HEAD_JSON_BYTES_V1 + 12;
    for height in 1..=observed.head.height {
        let block = ledger
            .load_by_height(checkpoint.chain_id, height)?
            .context("offline checkpoint history has a height gap")?;
        txs = txs
            .checked_add(block.body.raw_txs.len())
            .context("checkpoint tx count overflow")?;
        for raw in &block.body.raw_txs {
            raw_bytes = raw_bytes
                .checked_add(raw.len())
                .context("checkpoint raw byte overflow")?;
        }
        let frame = encode_json_bounded(&block, MAX_BLOCK_JSON_BYTES_V1)?;
        encoded_bytes = encoded_bytes
            .checked_add(frame.len() + 4)
            .context("checkpoint encoding overflow")?;
        if raw_bytes > MAX_HISTORY_BYTES_V1
            || txs > MAX_HISTORY_TRANSACTIONS_V1
            || encoded_bytes > MAX_CHECKPOINT_BUNDLE_BYTES_V1
        {
            bail!("offline history exceeds aggregate checkpoint bounds");
        }
        blocks.push(block);
    }
    if ledger.load_head(checkpoint.chain_id)?.as_ref() != Some(&observed.head)
        || ledger.load_prepared(checkpoint.chain_id)?.is_some()
    {
        bail!("offline source changed during export; use a coherent stopped-node copy");
    }
    let ownership = ledger
        .load_aoem_ownership()?
        .context("offline source ownership disappeared")?;
    if ownership.chain_id != checkpoint.chain_id
        || ownership.namespace_digest != checkpoint.namespace_digest
        || ownership.protocol_config_commitment != checkpoint.legacy_protocol_config_commitment
    {
        bail!("offline source ownership changed during export");
    }
    verify_nonce_checkpoint_v1(&snapshot, &observed.head, &blocks, checkpoint)?;
    encode_bundle(&snapshot, &observed.head, &blocks)
}

struct Frames<'a> {
    remaining: &'a [u8],
}
impl<'a> Frames<'a> {
    fn len(&mut self) -> Result<usize> {
        let bytes = self
            .remaining
            .get(..4)
            .context("truncated checkpoint frame length")?;
        let len = u32::from_le_bytes(bytes.try_into()?) as usize;
        self.remaining = &self.remaining[4..];
        Ok(len)
    }
    fn frame(&mut self, max: usize) -> Result<&'a [u8]> {
        let len = self.len()?;
        if len == 0 || len > max {
            bail!("checkpoint frame exceeds its nonempty byte limit");
        }
        let bytes = self
            .remaining
            .get(..len)
            .context("truncated checkpoint frame payload")?;
        self.remaining = &self.remaining[len..];
        Ok(bytes)
    }
}

pub fn read_nonce_checkpoint_bundle_v1(path: &Path) -> Result<Vec<u8>> {
    read_regular_file_bounded(path, MAX_CHECKPOINT_BUNDLE_BYTES_V1)
}

/// Bundle digest is transport integrity, not independent proof of chain trust.
pub fn checkpoint_bundle_digest_v1(bytes: &[u8]) -> String {
    to_hex(&sha256_bytes_v1(&[
        b"novovm-native-nonce-checkpoint-bundle-v1\0",
        bytes,
    ]))
}

pub fn verify_nonce_checkpoint_bundle_v1(
    bytes: &[u8],
    checkpoint: &NonceMigrationCheckpointV1,
) -> Result<NonceMigrationCheckpointReportV1> {
    Ok(verified_nonce_checkpoint_inputs_v1(bytes, checkpoint)?.2)
}

/// Preserve exact source bytes for a separate, non-authoritative upgrade proposal.
/// Callers cannot obtain this tuple without the complete checkpoint validation.
pub(super) fn verified_nonce_checkpoint_inputs_v1<'a>(
    bytes: &'a [u8],
    checkpoint: &NonceMigrationCheckpointV1,
) -> Result<(
    &'a [u8],
    NovNativeBlockLedgerHeadV1,
    NonceMigrationCheckpointReportV1,
)> {
    if bytes.len() > MAX_CHECKPOINT_BUNDLE_BYTES_V1 || !bytes.starts_with(MAGIC_V1) {
        bail!("invalid or oversized native nonce checkpoint bundle");
    }
    let mut frames = Frames {
        remaining: &bytes[MAGIC_V1.len()..],
    };
    let snapshot = frames.frame(MAX_SNAPSHOT_BYTES_V1)?;
    let head: NovNativeBlockLedgerHeadV1 =
        serde_json::from_slice(frames.frame(MAX_HEAD_JSON_BYTES_V1)?)?;
    let count = frames.len()?;
    if count == 0 || count > MAX_CHECKPOINT_BLOCKS_V1 {
        bail!("checkpoint block count exceeds bounds");
    }
    let mut blocks = Vec::with_capacity(count);
    let mut raw_bytes = 0usize;
    let mut txs = 0usize;
    for _ in 0..count {
        let block: NovNativeDurableBlockV1 =
            serde_json::from_slice(frames.frame(MAX_BLOCK_JSON_BYTES_V1)?)?;
        txs = txs
            .checked_add(block.body.raw_txs.len())
            .context("checkpoint tx count overflow")?;
        for raw in &block.body.raw_txs {
            raw_bytes = raw_bytes
                .checked_add(raw.len())
                .context("checkpoint raw byte overflow")?;
        }
        if raw_bytes > MAX_HISTORY_BYTES_V1 || txs > MAX_HISTORY_TRANSACTIONS_V1 {
            bail!("decoded checkpoint history exceeds aggregate bounds");
        }
        blocks.push(block);
    }
    if !frames.remaining.is_empty() {
        bail!("checkpoint bundle has trailing bytes");
    }
    let report = verify_nonce_checkpoint_v1(snapshot, &head, &blocks, checkpoint)?;
    Ok((snapshot, head, report))
}

#[cfg(test)]
mod tests {
    use super::super::native_nonce_checkpoint::test_fixture_v1;
    use super::*;
    use crate::native_block_ledger::{
        NovNativeBlockCandidateInputV1, NovNativePreparedAoemParentV1,
    };

    fn records(path: &Path) -> Vec<(String, String)> {
        let db =
            rocksdb::DB::open_for_read_only(&rocksdb::Options::default(), path, false).unwrap();
        db.iterator(rocksdb::IteratorMode::Start)
            .map(|entry| {
                let (key, value) = entry.unwrap();
                (to_hex(&key), to_hex(&value))
            })
            .collect()
    }

    #[test]
    fn native_nonce_bundle_export_is_deterministic_and_preserves_source() {
        let (snapshot, head, blocks, checkpoint, ledger_path) = test_fixture_v1();
        let snapshot_path = ledger_path.with_extension("snapshot.json");
        fs::write(&snapshot_path, &snapshot).unwrap();
        let before = records(&ledger_path);
        let inspection =
            inspect_nonce_checkpoint_source_v1(&snapshot_path, &ledger_path, checkpoint.chain_id)
                .unwrap();
        assert_eq!(inspection.checkpoint, checkpoint);
        assert!(inspection.observed_only);
        assert!(!inspection.independent_provenance_verified);
        assert!(!inspection.execution_replayed);
        assert!(!inspection.aoem_evidence_verified);
        assert!(!inspection.qc_verified);
        assert!(!inspection.activation_ready);
        assert!(!inspection.import_performed);
        let bundle =
            export_nonce_checkpoint_bundle_v1(&snapshot_path, &ledger_path, &checkpoint).unwrap();
        assert_eq!(bundle, encode_bundle(&snapshot, &head, &blocks).unwrap());
        assert_eq!(
            bundle,
            export_nonce_checkpoint_bundle_v1(&snapshot_path, &ledger_path, &checkpoint,).unwrap()
        );
        let report = verify_nonce_checkpoint_bundle_v1(&bundle, &checkpoint).unwrap();
        assert_eq!(
            report,
            verify_nonce_checkpoint_v1(&snapshot, &head, &blocks, &checkpoint).unwrap()
        );
        assert_eq!(before, records(&ledger_path));
        assert_eq!(snapshot, fs::read(&snapshot_path).unwrap());
    }

    #[test]
    fn native_nonce_bundle_rejects_bad_magic_truncation_and_trailing_bytes() {
        let (snapshot, head, blocks, checkpoint, _) = test_fixture_v1();
        let bundle = encode_bundle(&snapshot, &head, &blocks).unwrap();
        for length in [0, 7, 8, 9, 11, 12, 12 + snapshot.len(), bundle.len() - 1] {
            assert!(verify_nonce_checkpoint_bundle_v1(&bundle[..length], &checkpoint).is_err());
        }
        let mut changed = bundle.clone();
        changed[0] ^= 1;
        assert!(verify_nonce_checkpoint_bundle_v1(&changed, &checkpoint).is_err());
        changed = bundle.clone();
        changed.push(0);
        assert!(verify_nonce_checkpoint_bundle_v1(&changed, &checkpoint).is_err());
        assert_ne!(
            checkpoint_bundle_digest_v1(&changed),
            checkpoint_bundle_digest_v1(&bundle)
        );
    }

    #[test]
    fn native_nonce_bundle_rejects_invalid_frame_lengths_counts_and_pins() {
        let (snapshot, head, blocks, checkpoint, _) = test_fixture_v1();
        let bundle = encode_bundle(&snapshot, &head, &blocks).unwrap();
        let head_start = 12 + snapshot.len();
        let head_size =
            u32::from_le_bytes(bundle[head_start..head_start + 4].try_into().unwrap()) as usize;
        let count_start = head_start + 4 + head_size;
        for (offset, invalid) in [
            (8, 0),
            (8, u32::MAX),
            (head_start, MAX_HEAD_JSON_BYTES_V1 as u32 + 1),
            (count_start, 0),
            (count_start, MAX_CHECKPOINT_BLOCKS_V1 as u32 + 1),
            (count_start + 4, MAX_BLOCK_JSON_BYTES_V1 as u32 + 1),
        ] {
            let mut changed = bundle.clone();
            changed[offset..offset + 4].copy_from_slice(&invalid.to_le_bytes());
            assert!(verify_nonce_checkpoint_bundle_v1(&changed, &checkpoint).is_err());
        }
        let mut wrong = checkpoint.clone();
        wrong.tip_block_hash = "ef".repeat(32);
        assert!(verify_nonce_checkpoint_bundle_v1(&bundle, &wrong).is_err());
    }

    #[test]
    fn native_nonce_bundle_missing_ledger_is_not_created_and_wrong_pin_is_rejected() {
        let (snapshot, _, _, checkpoint, ledger_path) = test_fixture_v1();
        let snapshot_path = ledger_path.with_extension("snapshot.json");
        fs::write(&snapshot_path, snapshot).unwrap();
        let missing = ledger_path.with_extension("missing-ledger");
        assert!(!missing.exists());
        assert!(
            inspect_nonce_checkpoint_source_v1(&snapshot_path, &missing, checkpoint.chain_id)
                .is_err()
        );
        assert!(!missing.exists());
        let mut wrong = checkpoint.clone();
        wrong.snapshot_digest = "ef".repeat(32);
        assert!(export_nonce_checkpoint_bundle_v1(&snapshot_path, &ledger_path, &wrong).is_err());
    }

    #[test]
    fn native_nonce_bundle_refuses_unfinished_prepared_block() {
        let (snapshot, _, blocks, checkpoint, ledger_path) = test_fixture_v1();
        let snapshot_path = ledger_path.with_extension("snapshot.json");
        fs::write(&snapshot_path, snapshot).unwrap();
        let tail = &blocks.last().unwrap().header;
        let ledger = NovNativeBlockLedgerV1::open(&ledger_path).unwrap();
        ledger
            .prepare(NovNativeBlockCandidateInputV1 {
                context: novovm_protocol::NovBlockExecutionContextV1 {
                    chain_id: checkpoint.chain_id,
                    block_height: tail.height + 1,
                    parent_block_hash: tail.block_hash,
                    slot: tail.slot + 1,
                    timestamp_unix_ms: tail.timestamp_unix_ms + 1,
                },
                tx_hashes: vec![[0xf1; 32]],
                raw_txs: blocks[0].body.raw_txs.clone(),
                pre_state_root: tail.post_state_root,
                aoem_parent: Some(NovNativePreparedAoemParentV1 {
                    batch_id: tail.aoem_batch_id.clone(),
                    batch_result_id: tail.aoem_batch_result_id.clone(),
                    state_root: tail.post_state_root,
                    state_root_codec: tail.post_state_root_codec.clone(),
                    cumulative_receipt_root: tail.cumulative_receipt_root,
                    receipt_root_codec: tail.cumulative_receipt_root_codec.clone(),
                    state_version: tail.state_version,
                }),
            })
            .unwrap();
        drop(ledger);
        assert!(inspect_nonce_checkpoint_source_v1(
            &snapshot_path,
            &ledger_path,
            checkpoint.chain_id
        )
        .is_err());
        assert!(
            export_nonce_checkpoint_bundle_v1(&snapshot_path, &ledger_path, &checkpoint).is_err()
        );
    }

    #[test]
    fn native_nonce_bundle_file_and_writer_limits_fail_closed() {
        let (_, _, _, _, path) = test_fixture_v1();
        assert!(read_regular_file_bounded(&path, 8).is_err());
        let file = path.with_extension("bounded-input");
        fs::write(&file, []).unwrap();
        assert!(read_regular_file_bounded(&file, 8).is_err());
        fs::write(&file, [0u8; 9]).unwrap();
        assert!(read_regular_file_bounded(&file, 8).is_err());
        fs::write(&file, [1u8; 8]).unwrap();
        assert_eq!(read_regular_file_bounded(&file, 8).unwrap(), vec![1u8; 8]);
        let mut writer = BoundedBytes {
            bytes: Vec::new(),
            limit: 3,
        };
        writer.write_all(&[1, 2, 3]).unwrap();
        assert!(writer.write_all(&[4]).is_err());
        assert_eq!(writer.bytes, vec![1, 2, 3]);
        assert!(encode_json_bounded(&vec![0u8; 8], 8).is_err());
    }

    #[test]
    fn native_nonce_bundle_committed_cli_fixture_matches_validated_source() {
        // Synthetic internally consistent claims, NOT an AOEM execution/QC fixture.
        let (snapshot, _, _, checkpoint, ledger_path) = test_fixture_v1();
        let fixture = serde_json::json!({
            "snapshot_json": String::from_utf8(snapshot).unwrap(),
            "checkpoint": checkpoint,
            "records": records(&ledger_path),
        });
        let committed: serde_json::Value = serde_json::from_str(include_str!(
            "../../novovmctl/tests/fixtures/native_nonce_checkpoint_v1.json"
        ))
        .unwrap();
        assert_eq!(fixture, committed);
    }
}
