#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_protocol::NovBlockExecutionContextV1;
use rocksdb::{Options as RocksDbOptions, WriteBatch as RocksDbWriteBatch, WriteOptions, DB};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};

pub const NOV_NATIVE_BLOCK_LEDGER_SCHEMA_V1: &str = "novovm-native-block-ledger/v1";
pub const NOV_NATIVE_BLOCK_CANDIDATE_GRAPH_SCHEMA_V1: &str =
    "novovm-native-block-candidate-graph/v1";
pub const NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1: usize = 1_024;
pub const NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1: usize = 2 * 1024 * 1024;
pub const NOV_NATIVE_BLOCK_LEDGER_MAX_HYDRATE_BLOCKS_V1: usize = 4_096;
pub const NOV_NATIVE_BLOCK_CANDIDATE_GRAPH_MAX_LINKS_V1: usize = 4_096;
pub const NOV_NATIVE_BLOCK_CANDIDATE_GRAPH_MAX_TRAVERSAL_NODES_V1: usize = 4_096;

const KEY_PREFIX_V1: &str = "native_block_ledger/v1/";
const KEY_SCHEMA_V1: &[u8] = b"native_block_ledger/v1/schema";
const KEY_CANDIDATE_GRAPH_SCHEMA_V1: &[u8] =
    b"native_block_ledger/v1/extension/candidate_graph/v1/schema";
const KEY_AOEM_OWNERSHIP_V1: &[u8] = b"native_block_ledger/v1/ownership/aoem";
const AOEM_OWNERSHIP_SCHEMA_V1: &str = "novovm-native-block-ledger-aoem-ownership/v1";
const PREPARED_SCHEMA_V1: &str = "novovm-native-prepared-block/v1";
const HEADER_SCHEMA_V1: &str = "novovm-native-block-header/v1";
const BODY_SCHEMA_V1: &str = "novovm-native-block-body/v1";
const EVIDENCE_SCHEMA_V1: &str = "novovm-native-block-execution-evidence/v1";
const HEAD_SCHEMA_V1: &str = "novovm-native-block-ledger-head/v1";
const TX_LOCATION_SCHEMA_V1: &str = "novovm-native-block-tx-location/v1";
const RECEIPT_LOCATION_SCHEMA_V1: &str = "novovm-native-block-receipt-location/v1";
const EXTERNAL_ID_INDEX_SCHEMA_V1: &str = "novovm-native-block-external-id-index/v1";
const CANDIDATE_RECORD_SCHEMA_V1: &str = "novovm-native-block-candidate-record/v1";
const CANDIDATE_HEIGHT_INDEX_SCHEMA_V1: &str = "novovm-native-block-candidate-height-index/v1";
const CANDIDATE_CHILDREN_INDEX_SCHEMA_V1: &str = "novovm-native-block-candidate-children-index/v1";
const CANDIDATE_KIND_V1: &str = "local_unsealed_execution_candidate";
const CANDIDATE_SOURCE_LOCAL_AOEM_V1: &str = "local_aoem_owned_execution";
const CANDIDATE_SOURCE_OBSERVED_V1: &str = "observed_unsealed_candidate";
const CANDIDATE_STATUS_ACTIVE_V1: &str = "active_unsealed";
const CANDIDATE_STATUS_ABORTED_V1: &str = "aborted_unsealed";
const CANDIDATE_ABORT_REASON_MAX_BYTES_V1: usize = 512;
const POST_STATE_ROOT_CODEC_V1: &str = "novovm-consensus-native-state-wire/v1";
const CUMULATIVE_RECEIPT_ROOT_CODEC_V1: &str = "novovm-consensus-receipt-wire/v1";

const ORDERED_TX_ROOT_DOMAIN_V1: &[u8] = b"novovm-native-ordered-tx-root-v1\0";
const BODY_DIGEST_DOMAIN_V1: &[u8] = b"novovm-native-block-body-digest-v1\0";
const CANDIDATE_ID_DOMAIN_V1: &[u8] = b"novovm-native-block-candidate-id-v1\0";
const BLOCK_RECEIPT_ROOT_DOMAIN_V1: &[u8] = b"novovm-native-block-receipt-root-v1\0";
const BLOCK_HASH_DOMAIN_V1: &[u8] = b"novovm-native-block-hash-v1\0";
const EXTERNAL_ID_KEY_DOMAIN_V1: &[u8] = b"novovm-native-block-external-id-key-v1\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NovNativeBlockCandidateInputV1 {
    pub context: NovBlockExecutionContextV1,
    pub tx_hashes: Vec<[u8; 32]>,
    pub raw_txs: Vec<Vec<u8>>,
    pub pre_state_root: [u8; 32],
    pub aoem_parent: Option<NovNativePreparedAoemParentV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativePreparedAoemParentV1 {
    pub batch_id: String,
    pub batch_result_id: String,
    pub state_root: [u8; 32],
    pub state_root_codec: String,
    pub cumulative_receipt_root: [u8; 32],
    pub receipt_root_codec: String,
    pub state_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativePreparedBlockV1 {
    pub schema: String,
    pub candidate_id: [u8; 32],
    pub context: NovBlockExecutionContextV1,
    pub context_commitment: [u8; 32],
    pub pre_state_root: [u8; 32],
    pub ordered_tx_root: [u8; 32],
    pub body_digest: [u8; 32],
    pub body_bytes: u64,
    pub tx_hashes: Vec<[u8; 32]>,
    pub raw_txs: Vec<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aoem_parent: Option<NovNativePreparedAoemParentV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_aoem_batch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_aoem_output_commitment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NovNativeBlockCommitInputV1 {
    pub post_state_root: [u8; 32],
    pub cumulative_receipt_root: [u8; 32],
    pub per_block_receipt_commitments: Vec<[u8; 32]>,
    pub aoem_batch_id: String,
    pub aoem_batch_result_id: String,
    pub aoem_evidence_commitment: [u8; 32],
    pub state_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeBlockHeaderV1 {
    pub schema: String,
    pub candidate_kind: String,
    pub execution_context: NovBlockExecutionContextV1,
    pub chain_id: u64,
    pub height: u64,
    pub slot: u64,
    pub timestamp_unix_ms: u64,
    pub parent_block_hash: [u8; 32],
    pub block_hash: [u8; 32],
    pub candidate_id: [u8; 32],
    pub execution_context_commitment: [u8; 32],
    pub pre_state_root: [u8; 32],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aoem_parent: Option<NovNativePreparedAoemParentV1>,
    pub post_state_root: [u8; 32],
    pub post_state_root_codec: String,
    pub ordered_tx_root: [u8; 32],
    pub block_receipt_root: [u8; 32],
    pub cumulative_receipt_root: [u8; 32],
    pub cumulative_receipt_root_codec: String,
    pub body_digest: [u8; 32],
    pub body_bytes: u64,
    pub tx_count: u32,
    pub receipt_count: u32,
    pub state_version: u64,
    pub aoem_batch_id: String,
    pub aoem_batch_result_id: String,
    pub aoem_expected_output_commitment: String,
    pub aoem_evidence_commitment: [u8; 32],
    pub aoem_readback_verified: bool,
    pub canonical_local: bool,
    pub safe: bool,
    pub finalized: bool,
    pub proof_sealed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeBlockBodyV1 {
    pub schema: String,
    pub chain_id: u64,
    pub height: u64,
    pub block_hash: [u8; 32],
    pub ordered_tx_root: [u8; 32],
    pub body_digest: [u8; 32],
    pub body_bytes: u64,
    pub tx_hashes: Vec<[u8; 32]>,
    pub raw_txs: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeBlockExecutionEvidenceV1 {
    pub schema: String,
    pub chain_id: u64,
    pub height: u64,
    pub block_hash: [u8; 32],
    pub aoem_batch_id: String,
    pub aoem_batch_result_id: String,
    pub aoem_expected_output_commitment: String,
    pub aoem_evidence_commitment: [u8; 32],
    pub post_state_root: [u8; 32],
    pub cumulative_receipt_root: [u8; 32],
    pub block_receipt_root: [u8; 32],
    pub per_block_receipt_commitments: Vec<[u8; 32]>,
    pub state_version: u64,
    pub evidence_kind: String,
    pub proof_sealed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeDurableBlockV1 {
    pub header: NovNativeBlockHeaderV1,
    pub body: NovNativeBlockBodyV1,
    pub execution_evidence: NovNativeBlockExecutionEvidenceV1,
}

/// Mutable local-view metadata for an immutable, fully materialized block
/// candidate. This record deliberately lives outside the block header: abort,
/// local execution selection, and future fork-choice input must never rewrite
/// a block commitment. In v1 this graph is strictly unsealed and cannot confer
/// chain canonicality or finality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeBlockCandidateRecordV1 {
    pub schema: String,
    pub chain_id: u64,
    pub height: u64,
    pub slot: u64,
    pub timestamp_unix_ms: u64,
    pub block_hash: [u8; 32],
    pub parent_block_hash: [u8; 32],
    pub candidate_id: [u8; 32],
    pub execution_context_commitment: [u8; 32],
    pub pre_state_root: [u8; 32],
    pub post_state_root: [u8; 32],
    pub ordered_tx_root: [u8; 32],
    pub block_receipt_root: [u8; 32],
    pub cumulative_receipt_root: [u8; 32],
    pub body_digest: [u8; 32],
    pub body_bytes: u64,
    pub tx_count: u32,
    pub state_version: u64,
    pub aoem_batch_id: String,
    pub aoem_batch_result_id: String,
    pub aoem_evidence_commitment: [u8; 32],
    pub candidate_source: String,
    pub lifecycle_status: String,
    /// Monotonic lifecycle revision for future compare-and-swap transitions.
    pub revision: u64,
    pub commitment_bindings_verified: bool,
    pub parent_continuity_verified: bool,
    pub body_data_available: bool,
    /// True only when this process accepted the result through its own
    /// AOEM-owned execution/readback path. A peer's assertion is never
    /// upgraded into local AOEM verification.
    pub local_aoem_readback_verified: bool,
    /// The existing linear execution projection selected by local AOEM state.
    /// This is not network fork choice and is not chain canonicality.
    pub execution_selected_local: bool,
    pub fork_choice_selected: bool,
    pub chain_canonical: bool,
    pub proof_sealed: bool,
    pub safe: bool,
    pub finalized: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abort_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NovNativeBlockCandidateHeightIndexV1 {
    schema: String,
    chain_id: u64,
    height: u64,
    block_hashes: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NovNativeBlockCandidateChildrenIndexV1 {
    schema: String,
    chain_id: u64,
    parent_block_hash: [u8; 32],
    block_hashes: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeBlockLedgerHeadV1 {
    pub schema: String,
    pub chain_id: u64,
    pub height: u64,
    pub block_hash: [u8; 32],
    pub post_state_root: [u8; 32],
    pub cumulative_receipt_root: [u8; 32],
    pub state_version: u64,
    pub slot: u64,
    pub timestamp_unix_ms: u64,
    pub block_count: u64,
    pub cumulative_tx_count: u64,
    pub cumulative_body_bytes: u64,
    pub canonical_local: bool,
    pub safe: bool,
    pub finalized: bool,
    pub proof_sealed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeBlockTxLocationV1 {
    pub schema: String,
    pub chain_id: u64,
    pub tx_hash: [u8; 32],
    pub height: u64,
    pub block_hash: [u8; 32],
    pub tx_index: u32,
    pub canonical_local: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeBlockReceiptLocationV1 {
    pub schema: String,
    pub chain_id: u64,
    pub tx_hash: [u8; 32],
    pub height: u64,
    pub block_hash: [u8; 32],
    pub tx_index: u32,
    pub receipt_commitment: [u8; 32],
    pub canonical_local: bool,
    pub proof_sealed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NovNativeBlockExternalIdIndexV1 {
    schema: String,
    id_kind: String,
    exact_id: String,
    chain_id: u64,
    height: u64,
    block_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeBlockLedgerStatusV1 {
    pub schema: String,
    pub path: String,
    pub chain_id: u64,
    pub head: Option<NovNativeBlockLedgerHeadV1>,
    pub prepared: Option<NovNativePreparedBlockV1>,
    pub canonical_local: bool,
    pub safe: bool,
    pub finalized: bool,
    pub proof_sealed: bool,
    pub max_txs_per_block: usize,
    pub max_body_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeBlockLedgerAoemOwnershipV1 {
    pub schema: String,
    pub chain_id: u64,
    pub namespace_digest: String,
    pub protocol_config_commitment: String,
}

struct NovNativeBlockLedgerProcessEntryV1 {
    db: DB,
    write_lock: Arc<Mutex<()>>,
}

impl Deref for NovNativeBlockLedgerProcessEntryV1 {
    type Target = DB;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}

pub struct NovNativeBlockLedgerV1 {
    path: PathBuf,
    db: Arc<NovNativeBlockLedgerProcessEntryV1>,
    write_lock: Arc<Mutex<()>>,
    read_only: bool,
}

impl NovNativeBlockLedgerV1 {
    pub fn open(path: &Path) -> Result<Self> {
        let process_key = native_block_ledger_process_key_v1(path)?;
        let mut registry = native_block_ledger_process_registry_v1()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "create NOV native block ledger parent failed: {}",
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
            let db = DB::open(&options, path).with_context(|| {
                format!("open NOV native block ledger failed: {}", path.display())
            })?;
            let entry = Arc::new(NovNativeBlockLedgerProcessEntryV1 {
                db,
                write_lock: Arc::new(Mutex::new(())),
            });
            registry.insert(process_key, Arc::downgrade(&entry));
            entry
        };
        drop(registry);
        match db
            .get(KEY_SCHEMA_V1)
            .context("read NOV native block ledger schema failed")?
        {
            Some(raw) if raw.as_slice() != NOV_NATIVE_BLOCK_LEDGER_SCHEMA_V1.as_bytes() => {
                bail!(
                    "unsupported NOV native block ledger schema: {}",
                    String::from_utf8_lossy(raw.as_slice())
                );
            }
            Some(_) => {}
            None => {
                let mut batch = RocksDbWriteBatch::default();
                batch.put(KEY_SCHEMA_V1, NOV_NATIVE_BLOCK_LEDGER_SCHEMA_V1.as_bytes());
                write_sync_v1(&db, batch).context("initialize NOV native block ledger schema")?;
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
            write_lock: Arc::clone(&db.write_lock),
            db,
            read_only: false,
        })
    }

    /// Open an already initialized ledger without creating directories, a
    /// RocksDB database, or its schema. Query/RPC surfaces use this boundary so
    /// an unauthenticated read cannot materialize local persistence.
    pub fn open_existing_read_only(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let process_key = native_block_ledger_process_key_v1(path)?;
        let mut registry = native_block_ledger_process_registry_v1()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registry.retain(|_, entry| entry.strong_count() > 0);
        let db = if let Some(entry) = registry.get(process_key.as_str()).and_then(Weak::upgrade) {
            entry
        } else {
            let options = RocksDbOptions::default();
            let db = DB::open_for_read_only(&options, path, false).with_context(|| {
                format!(
                    "open existing NOV native block ledger read-only failed: {}",
                    path.display()
                )
            })?;
            Arc::new(NovNativeBlockLedgerProcessEntryV1 {
                db,
                write_lock: Arc::new(Mutex::new(())),
            })
        };
        drop(registry);
        let ledger = Self {
            path: path.to_path_buf(),
            write_lock: Arc::clone(&db.write_lock),
            db,
            read_only: true,
        };
        ledger.ensure_schema_v1()?;
        Ok(Some(ledger))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// Permanently bind this ledger to the AOEM-owned production domain. The
    /// marker is deliberately independent of runtime feature flags so a later
    /// configuration mistake cannot silently restore Host mutation.
    pub(crate) fn bind_aoem_ownership(
        &self,
        chain_id: u64,
        namespace_digest: &str,
        protocol_config_commitment: &str,
    ) -> Result<bool> {
        let requested = NovNativeBlockLedgerAoemOwnershipV1 {
            schema: AOEM_OWNERSHIP_SCHEMA_V1.to_string(),
            chain_id,
            namespace_digest: namespace_digest.to_string(),
            protocol_config_commitment: protocol_config_commitment.to_string(),
        };
        validate_aoem_ownership_v1(&requested)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        if let Some(existing) = self.load_aoem_ownership_inner_v1()? {
            if existing != requested {
                bail!(
                    "NOV native block ledger AOEM ownership binding mismatch: bound_chain={} requested_chain={}",
                    existing.chain_id,
                    requested.chain_id
                );
            }
            return Ok(false);
        }
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            KEY_AOEM_OWNERSHIP_V1,
            &requested,
            "AOEM ownership binding",
        )?;
        write_sync_v1(&self.db, batch)
            .context("persist NOV block-ledger AOEM ownership binding")?;
        if self.load_aoem_ownership_inner_v1()?.as_ref() != Some(&requested) {
            bail!("NOV native block ledger AOEM ownership binding readback mismatch");
        }
        Ok(true)
    }

    pub fn load_aoem_ownership(&self) -> Result<Option<NovNativeBlockLedgerAoemOwnershipV1>> {
        self.ensure_schema_v1()?;
        self.load_aoem_ownership_inner_v1()
    }

    pub(crate) fn prepare(
        &self,
        input: NovNativeBlockCandidateInputV1,
    ) -> Result<NovNativePreparedBlockV1> {
        let prepared = build_prepared_block_v1(input)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;

        if let Some(existing) =
            self.load_by_height_inner_v1(prepared.context.chain_id, prepared.context.block_height)?
        {
            if durable_block_matches_prepared_v1(&existing, &prepared) {
                return Ok(prepared);
            }
            bail!(
                "NOV native block height conflict at chain={} height={}",
                prepared.context.chain_id,
                prepared.context.block_height
            );
        }

        self.validate_prepared_against_head_v1(&prepared)?;
        for tx_hash in &prepared.tx_hashes {
            if let Some(location) =
                self.load_tx_location_inner_v1(prepared.context.chain_id, *tx_hash)?
            {
                bail!(
                    "NOV native transaction is already indexed: chain={} tx_hash={} height={}",
                    prepared.context.chain_id,
                    hex_v1(tx_hash),
                    location.height
                );
            }
        }

        if let Some(current) = self.load_prepared_inner_v1(prepared.context.chain_id)? {
            if prepared_core_matches_v1(&current, &prepared)
                && (prepared.expected_aoem_batch_id.is_none()
                    || current.expected_aoem_batch_id == prepared.expected_aoem_batch_id)
            {
                return Ok(current);
            }
            bail!(
                "NOV native block ledger has a different unresolved candidate: chain={} current={} requested={}",
                prepared.context.chain_id,
                hex_v1(&current.candidate_id),
                hex_v1(&prepared.candidate_id)
            );
        }

        let candidate_key = candidate_key_v1(prepared.context.chain_id, &prepared.candidate_id);
        if let Some(existing) = read_json_v1::<NovNativePreparedBlockV1>(
            &self.db,
            candidate_key.as_bytes(),
            "prepared candidate",
        )? {
            if existing != prepared {
                bail!(
                    "NOV native prepared candidate id collision: chain={} candidate={}",
                    prepared.context.chain_id,
                    hex_v1(&prepared.candidate_id)
                );
            }
        }

        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            candidate_key.as_bytes(),
            &prepared,
            "prepared candidate",
        )?;
        batch.put(
            candidate_current_key_v1(prepared.context.chain_id).as_bytes(),
            prepared.candidate_id.as_slice(),
        );
        write_sync_v1(&self.db, batch).context("persist NOV native prepared block")?;
        let readback = self
            .load_prepared_inner_v1(prepared.context.chain_id)?
            .context("NOV native prepared block readback is missing")?;
        if readback != prepared {
            bail!("NOV native prepared block readback mismatch");
        }
        Ok(prepared)
    }

    pub(crate) fn bind_expected_aoem_batch_id(
        &self,
        prepared: &NovNativePreparedBlockV1,
        aoem_batch_id: &str,
        expected_output_commitment: &str,
    ) -> Result<NovNativePreparedBlockV1> {
        validate_prepared_block_v1(prepared)?;
        validate_external_id_v1("expected AOEM batch id", aoem_batch_id)?;
        validate_hex_commitment_v1(
            "expected AOEM output commitment",
            expected_output_commitment,
        )?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        let mut stored = match self.load_prepared_inner_v1(prepared.context.chain_id)? {
            Some(stored) => stored,
            None => {
                let durable = self
                    .load_by_height_inner_v1(
                        prepared.context.chain_id,
                        prepared.context.block_height,
                    )?
                    .context("NOV native prepared block is missing before AOEM batch binding")?;
                if !durable_block_matches_prepared_v1(&durable, prepared)
                    || durable.header.aoem_batch_id != aoem_batch_id
                    || durable.header.aoem_expected_output_commitment != expected_output_commitment
                {
                    bail!(
                        "AOEM batch/output binding cannot attach to a different durable NOV native block"
                    );
                }
                let mut completed = prepared.clone();
                completed.expected_aoem_batch_id = Some(aoem_batch_id.to_string());
                completed.expected_aoem_output_commitment =
                    Some(expected_output_commitment.to_string());
                return Ok(completed);
            }
        };
        if stored.candidate_id != prepared.candidate_id
            || stored.context != prepared.context
            || stored.pre_state_root != prepared.pre_state_root
            || stored.tx_hashes != prepared.tx_hashes
            || stored.raw_txs != prepared.raw_txs
            || stored.aoem_parent != prepared.aoem_parent
        {
            bail!("AOEM batch id cannot bind to a different NOV native prepared candidate");
        }
        match stored.expected_aoem_batch_id.as_deref() {
            Some(existing) if existing != aoem_batch_id => {
                bail!("NOV native prepared candidate AOEM batch id binding conflict")
            }
            Some(_) => {}
            None => {}
        }
        match stored.expected_aoem_output_commitment.as_deref() {
            Some(existing) if existing != expected_output_commitment => {
                bail!("NOV native prepared candidate AOEM output commitment binding conflict")
            }
            Some(_) => {}
            None => {}
        }
        stored.expected_aoem_batch_id = Some(aoem_batch_id.to_string());
        stored.expected_aoem_output_commitment = Some(expected_output_commitment.to_string());
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            candidate_key_v1(stored.context.chain_id, &stored.candidate_id).as_bytes(),
            &stored,
            "prepared candidate AOEM binding",
        )?;
        write_sync_v1(&self.db, batch)
            .context("persist NOV native prepared AOEM batch id binding")?;
        let readback = self
            .load_prepared_inner_v1(stored.context.chain_id)?
            .context("NOV native prepared AOEM binding readback is missing")?;
        if readback != stored {
            bail!("NOV native prepared AOEM binding readback mismatch");
        }
        Ok(readback)
    }

    pub(crate) fn commit(
        &self,
        prepared: &NovNativePreparedBlockV1,
        input: NovNativeBlockCommitInputV1,
    ) -> Result<NovNativeDurableBlockV1> {
        validate_prepared_block_v1(prepared)?;
        let expected = build_durable_block_v1(prepared, input)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;

        if let Some(existing) =
            self.load_by_height_inner_v1(prepared.context.chain_id, prepared.context.block_height)?
        {
            if existing == expected {
                self.validate_legacy_local_candidate_parent_inner_v1(&existing)?;
                let record = candidate_record_from_block_v1(
                    &existing,
                    CANDIDATE_SOURCE_LOCAL_AOEM_V1,
                    true,
                    true,
                )?;
                let mut batch = RocksDbWriteBatch::default();
                self.stage_candidate_graph_record_v1(&mut batch, &record)?;
                write_sync_v1(&self.db, batch)
                    .context("backfill local NOV native candidate graph entry")?;
                return Ok(existing);
            }
            bail!(
                "NOV native block commit conflicts with durable height: chain={} height={}",
                prepared.context.chain_id,
                prepared.context.block_height
            );
        }

        self.validate_prepared_against_head_v1(prepared)?;
        self.validate_candidate_parent_inner_v1(&expected)?;
        let stored_prepared = self
            .load_prepared_inner_v1(prepared.context.chain_id)?
            .context("NOV native block must be durably prepared before commit")?;
        if stored_prepared != *prepared {
            bail!("NOV native block commit candidate does not match durable prepared candidate");
        }

        let prior_head = self.load_head_inner_v1(prepared.context.chain_id)?;
        if let Some(head) = prior_head.as_ref() {
            if expected.header.state_version <= head.state_version {
                bail!(
                    "NOV native block state_version must advance: previous={} requested={}",
                    head.state_version,
                    expected.header.state_version
                );
            }
        }

        self.ensure_block_hash_available_v1(&expected)?;
        self.ensure_external_id_available_v1(
            "batch",
            expected.header.aoem_batch_id.as_str(),
            &expected,
        )?;
        self.ensure_external_id_available_v1(
            "result",
            expected.header.aoem_batch_result_id.as_str(),
            &expected,
        )?;
        for tx_hash in &expected.body.tx_hashes {
            if let Some(location) =
                self.load_tx_location_inner_v1(expected.header.chain_id, *tx_hash)?
            {
                bail!(
                    "NOV native transaction index conflict: chain={} tx_hash={} existing_height={}",
                    expected.header.chain_id,
                    hex_v1(tx_hash),
                    location.height
                );
            }
        }

        let previous_block_count = prior_head.as_ref().map_or(0, |head| head.block_count);
        let previous_tx_count = prior_head
            .as_ref()
            .map_or(0, |head| head.cumulative_tx_count);
        let previous_body_bytes = prior_head
            .as_ref()
            .map_or(0, |head| head.cumulative_body_bytes);
        let block_count = previous_block_count
            .checked_add(1)
            .context("NOV native block count overflow")?;
        let cumulative_tx_count = previous_tx_count
            .checked_add(u64::from(expected.header.tx_count))
            .context("NOV native cumulative transaction count overflow")?;
        let cumulative_body_bytes = previous_body_bytes
            .checked_add(expected.header.body_bytes)
            .context("NOV native cumulative body byte count overflow")?;
        let head = NovNativeBlockLedgerHeadV1 {
            schema: HEAD_SCHEMA_V1.to_string(),
            chain_id: expected.header.chain_id,
            height: expected.header.height,
            block_hash: expected.header.block_hash,
            post_state_root: expected.header.post_state_root,
            cumulative_receipt_root: expected.header.cumulative_receipt_root,
            state_version: expected.header.state_version,
            slot: expected.header.slot,
            timestamp_unix_ms: expected.header.timestamp_unix_ms,
            block_count,
            cumulative_tx_count,
            cumulative_body_bytes,
            canonical_local: true,
            safe: false,
            finalized: false,
            proof_sealed: false,
        };

        let candidate_record =
            candidate_record_from_block_v1(&expected, CANDIDATE_SOURCE_LOCAL_AOEM_V1, true, true)?;
        let mut batch = RocksDbWriteBatch::default();
        self.stage_candidate_graph_record_v1(&mut batch, &candidate_record)?;
        put_json_v1(
            &mut batch,
            header_key_v1(expected.header.chain_id, &expected.header.block_hash).as_bytes(),
            &expected.header,
            "block header",
        )?;
        put_json_v1(
            &mut batch,
            body_key_v1(expected.header.chain_id, &expected.header.block_hash).as_bytes(),
            &expected.body,
            "block body",
        )?;
        put_json_v1(
            &mut batch,
            evidence_key_v1(expected.header.chain_id, &expected.header.block_hash).as_bytes(),
            &expected.execution_evidence,
            "block execution evidence",
        )?;
        batch.put(
            height_key_v1(expected.header.chain_id, expected.header.height).as_bytes(),
            expected.header.block_hash.as_slice(),
        );
        put_json_v1(
            &mut batch,
            head_key_v1(expected.header.chain_id).as_bytes(),
            &head,
            "block ledger head",
        )?;

        for (index, (tx_hash, receipt_commitment)) in expected
            .body
            .tx_hashes
            .iter()
            .zip(
                expected
                    .execution_evidence
                    .per_block_receipt_commitments
                    .iter(),
            )
            .enumerate()
        {
            let tx_index = u32::try_from(index).context("NOV native tx index exceeds u32")?;
            let location = NovNativeBlockTxLocationV1 {
                schema: TX_LOCATION_SCHEMA_V1.to_string(),
                chain_id: expected.header.chain_id,
                tx_hash: *tx_hash,
                height: expected.header.height,
                block_hash: expected.header.block_hash,
                tx_index,
                canonical_local: true,
            };
            put_json_v1(
                &mut batch,
                tx_key_v1(expected.header.chain_id, tx_hash).as_bytes(),
                &location,
                "transaction location",
            )?;
            let receipt = NovNativeBlockReceiptLocationV1 {
                schema: RECEIPT_LOCATION_SCHEMA_V1.to_string(),
                chain_id: expected.header.chain_id,
                tx_hash: *tx_hash,
                height: expected.header.height,
                block_hash: expected.header.block_hash,
                tx_index,
                receipt_commitment: *receipt_commitment,
                canonical_local: true,
                proof_sealed: false,
            };
            put_json_v1(
                &mut batch,
                receipt_key_v1(expected.header.chain_id, tx_hash).as_bytes(),
                &receipt,
                "receipt location",
            )?;
        }

        for (kind, id) in [
            ("batch", expected.header.aoem_batch_id.as_str()),
            ("result", expected.header.aoem_batch_result_id.as_str()),
        ] {
            let index = NovNativeBlockExternalIdIndexV1 {
                schema: EXTERNAL_ID_INDEX_SCHEMA_V1.to_string(),
                id_kind: kind.to_string(),
                exact_id: id.to_string(),
                chain_id: expected.header.chain_id,
                height: expected.header.height,
                block_hash: expected.header.block_hash,
            };
            put_json_v1(
                &mut batch,
                external_id_key_v1(expected.header.chain_id, kind, id).as_bytes(),
                &index,
                "AOEM external id index",
            )?;
        }

        batch.delete(candidate_current_key_v1(expected.header.chain_id).as_bytes());
        batch.delete(candidate_key_v1(expected.header.chain_id, &prepared.candidate_id).as_bytes());
        write_sync_v1(&self.db, batch).context("commit NOV native durable block")?;

        let readback = self
            .load_by_hash_inner_v1(expected.header.chain_id, expected.header.block_hash)?
            .context("NOV native durable block readback is missing")?;
        if readback != expected {
            bail!("NOV native durable block readback mismatch");
        }
        let candidate_readback = self
            .load_candidate_record_inner_v1(expected.header.chain_id, expected.header.block_hash)?
            .context("NOV native local candidate graph readback is missing")?;
        if candidate_readback != candidate_record {
            bail!("NOV native local candidate graph readback mismatch");
        }
        Ok(readback)
    }

    pub fn load_head(&self, chain_id: u64) -> Result<Option<NovNativeBlockLedgerHeadV1>> {
        self.ensure_schema_v1()?;
        self.load_head_verified_inner_v1(chain_id)
    }

    pub fn load_prepared(&self, chain_id: u64) -> Result<Option<NovNativePreparedBlockV1>> {
        self.ensure_schema_v1()?;
        self.load_prepared_inner_v1(chain_id)
    }

    pub fn load_by_height(
        &self,
        chain_id: u64,
        height: u64,
    ) -> Result<Option<NovNativeDurableBlockV1>> {
        self.ensure_schema_v1()?;
        self.load_by_height_inner_v1(chain_id, height)
    }

    pub fn load_by_hash(
        &self,
        chain_id: u64,
        block_hash: [u8; 32],
    ) -> Result<Option<NovNativeDurableBlockV1>> {
        self.ensure_schema_v1()?;
        self.load_by_hash_inner_v1(chain_id, block_hash)
    }

    pub fn load_tx_location(
        &self,
        chain_id: u64,
        tx_hash: [u8; 32],
    ) -> Result<Option<NovNativeBlockTxLocationV1>> {
        self.ensure_schema_v1()?;
        let location = self.load_tx_location_inner_v1(chain_id, tx_hash)?;
        if let Some(location) = location.as_ref() {
            let block = self
                .load_by_hash_inner_v1(chain_id, location.block_hash)?
                .context("NOV native transaction index points to a missing durable block")?;
            if block.header.height != location.height
                || block.body.tx_hashes.get(location.tx_index as usize) != Some(&tx_hash)
            {
                bail!("NOV native transaction index does not resolve back to its durable block");
            }
        }
        Ok(location)
    }

    pub fn load_receipt_location(
        &self,
        chain_id: u64,
        tx_hash: [u8; 32],
    ) -> Result<Option<NovNativeBlockReceiptLocationV1>> {
        self.ensure_schema_v1()?;
        let location = read_json_v1::<NovNativeBlockReceiptLocationV1>(
            &self.db,
            receipt_key_v1(chain_id, &tx_hash).as_bytes(),
            "receipt location",
        )?;
        if let Some(location) = location.as_ref() {
            validate_receipt_location_v1(location, chain_id, tx_hash)?;
            let block = self
                .load_by_hash_inner_v1(chain_id, location.block_hash)?
                .context("NOV native receipt index points to a missing durable block")?;
            if block.header.height != location.height
                || block.body.tx_hashes.get(location.tx_index as usize) != Some(&tx_hash)
                || block
                    .execution_evidence
                    .per_block_receipt_commitments
                    .get(location.tx_index as usize)
                    != Some(&location.receipt_commitment)
            {
                bail!("NOV native receipt index does not resolve back to its durable block");
            }
        }
        Ok(location)
    }

    /// Persist a fully materialized candidate observed from another node.
    /// The immutable artifact and graph indexes are stored in a sidecar
    /// keyspace. This method never changes the AOEM execution head, legacy
    /// height/transaction indexes, or any finality flag.
    // Reserved for the authenticated consensus ingress. There is deliberately
    // no public RPC mutation route in candidate_graph/v1.
    #[allow(dead_code)]
    pub(crate) fn register_observed_unsealed_candidate(
        &self,
        block: NovNativeDurableBlockV1,
    ) -> Result<NovNativeBlockCandidateRecordV1> {
        validate_durable_block_v1(&block)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;

        if let Some(existing) =
            self.load_candidate_record_inner_v1(block.header.chain_id, block.header.block_hash)?
        {
            let existing_block = self
                .load_candidate_block_for_record_inner_v1(&existing)?
                .context("NOV native candidate record points to a missing block artifact")?;
            if existing_block != block {
                bail!("NOV native candidate block hash collision or conflicting artifact");
            }
            return Ok(existing);
        }

        self.validate_candidate_parent_inner_v1(&block)?;
        if let Some(existing_artifact) = read_json_v1::<NovNativeDurableBlockV1>(
            &self.db,
            candidate_artifact_key_v1(block.header.chain_id, &block.header.block_hash).as_bytes(),
            "existing observed block candidate artifact",
        )? {
            validate_durable_block_v1(&existing_artifact)?;
            if existing_artifact != block {
                bail!("NOV native candidate artifact conflicts with an existing block hash");
            }
        }
        let record =
            candidate_record_from_block_v1(&block, CANDIDATE_SOURCE_OBSERVED_V1, false, false)?;
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            candidate_artifact_key_v1(block.header.chain_id, &block.header.block_hash).as_bytes(),
            &block,
            "observed block candidate artifact",
        )?;
        self.stage_candidate_graph_record_v1(&mut batch, &record)?;
        write_sync_v1(&self.db, batch)
            .context("persist observed NOV native block candidate graph entry")?;

        let readback = self
            .load_candidate_record_inner_v1(record.chain_id, record.block_hash)?
            .context("observed NOV native candidate graph readback is missing")?;
        if readback != record {
            bail!("observed NOV native candidate graph readback mismatch");
        }
        Ok(readback)
    }

    /// Durably abort an unselected candidate and all of its graph descendants.
    /// Artifacts and indexes remain intact for audit/restart recovery. This is
    /// not an AOEM state rollback and refuses to touch the selected execution
    /// projection.
    // Reserved for the authenticated consensus lifecycle. There is
    // deliberately no public RPC mutation route in candidate_graph/v1.
    #[allow(dead_code)]
    pub(crate) fn abort_unselected_candidate_branch(
        &self,
        chain_id: u64,
        root_block_hash: [u8; 32],
        reason: &str,
    ) -> Result<Vec<NovNativeBlockCandidateRecordV1>> {
        let reason = validate_candidate_abort_reason_v1(reason)?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;

        let mut pending = vec![root_block_hash];
        let mut visited = HashSet::new();
        let mut records = Vec::new();
        while let Some(block_hash) = pending.pop() {
            if !visited.insert(block_hash) {
                continue;
            }
            if visited.len() > NOV_NATIVE_BLOCK_CANDIDATE_GRAPH_MAX_TRAVERSAL_NODES_V1 {
                bail!("NOV native candidate abort branch exceeds the bounded traversal node limit");
            }
            let record = self
                .load_candidate_record_inner_v1(chain_id, block_hash)?
                .context("NOV native candidate abort target is missing")?;
            if record.execution_selected_local
                || record.fork_choice_selected
                || record.chain_canonical
                || record.proof_sealed
                || record.safe
                || record.finalized
            {
                bail!(
                    "NOV native selected or sealed candidate cannot use unselected abort: height={} hash={}",
                    record.height,
                    hex_v1(&record.block_hash)
                );
            }
            for child in self.load_candidate_children_inner_v1(chain_id, block_hash)? {
                pending.push(child.block_hash);
            }
            records.push(record);
        }

        records.sort_by_key(|record| (record.height, record.block_hash));
        let mut batch = RocksDbWriteBatch::default();
        let mut changed = false;
        for record in &mut records {
            if record.lifecycle_status == CANDIDATE_STATUS_ACTIVE_V1 {
                record.lifecycle_status = CANDIDATE_STATUS_ABORTED_V1.to_string();
                record.revision = record
                    .revision
                    .checked_add(1)
                    .context("NOV native candidate lifecycle revision overflow")?;
                record.abort_reason = Some(reason.clone());
                validate_candidate_record_v1(record)?;
                put_json_v1(
                    &mut batch,
                    candidate_record_key_v1(record.chain_id, &record.block_hash).as_bytes(),
                    record,
                    "aborted block candidate record",
                )?;
                changed = true;
            }
        }
        if changed {
            write_sync_v1(&self.db, batch)
                .context("persist aborted NOV native candidate branch")?;
        }
        for expected in &records {
            let actual = self
                .load_candidate_record_inner_v1(chain_id, expected.block_hash)?
                .context("aborted NOV native candidate readback is missing")?;
            if actual != *expected {
                bail!("aborted NOV native candidate readback mismatch");
            }
        }
        Ok(records)
    }

    pub fn load_candidate_record(
        &self,
        chain_id: u64,
        block_hash: [u8; 32],
    ) -> Result<Option<NovNativeBlockCandidateRecordV1>> {
        self.ensure_schema_v1()?;
        self.load_candidate_record_inner_v1(chain_id, block_hash)
    }

    pub fn load_candidate_block(
        &self,
        chain_id: u64,
        block_hash: [u8; 32],
    ) -> Result<Option<NovNativeDurableBlockV1>> {
        self.ensure_schema_v1()?;
        let Some(record) = self.load_candidate_record_inner_v1(chain_id, block_hash)? else {
            return Ok(None);
        };
        self.load_candidate_block_for_record_inner_v1(&record)
    }

    /// Load the only candidate class that seal/v1 may sign. This deliberately
    /// stays crate-private: the public seal API must derive its subject from a
    /// locally persisted AOEM result instead of trusting caller-supplied
    /// lifecycle booleans or a peer's execution claim.
    pub(crate) fn load_seal_eligible_local_candidate_v1(
        &self,
        chain_id: u64,
        block_hash: [u8; 32],
    ) -> Result<(NovNativeBlockCandidateRecordV1, NovNativeDurableBlockV1)> {
        self.ensure_schema_v1()?;
        let record = self
            .load_candidate_record_inner_v1(chain_id, block_hash)?
            .context("NOV native seal candidate is missing from the durable candidate graph")?;
        let block = self
            .load_candidate_block_for_record_inner_v1(&record)?
            .context("NOV native seal candidate artifact is missing")?;

        if record.candidate_source != CANDIDATE_SOURCE_LOCAL_AOEM_V1
            || record.lifecycle_status != CANDIDATE_STATUS_ACTIVE_V1
            || !record.commitment_bindings_verified
            || !record.parent_continuity_verified
            || !record.body_data_available
            || !record.local_aoem_readback_verified
            || !record.execution_selected_local
            || record.fork_choice_selected
            || record.chain_canonical
            || record.proof_sealed
            || record.safe
            || record.finalized
            || record.abort_reason.is_some()
        {
            bail!("NOV native seal requires an active, locally AOEM-verified unsealed candidate");
        }
        if !block.header.aoem_readback_verified
            || block.header.safe
            || block.header.finalized
            || block.header.proof_sealed
            || block.execution_evidence.proof_sealed
        {
            bail!("NOV native seal candidate artifact violates the unsealed AOEM policy");
        }
        Ok((record, block))
    }

    pub fn load_candidate_records_by_height(
        &self,
        chain_id: u64,
        height: u64,
    ) -> Result<Vec<NovNativeBlockCandidateRecordV1>> {
        self.ensure_schema_v1()?;
        self.load_candidate_records_by_height_inner_v1(chain_id, height)
    }

    pub fn load_candidate_children(
        &self,
        chain_id: u64,
        parent_block_hash: [u8; 32],
    ) -> Result<Vec<NovNativeBlockCandidateRecordV1>> {
        self.ensure_schema_v1()?;
        self.load_candidate_children_inner_v1(chain_id, parent_block_hash)
    }

    pub fn status(&self, chain_id: u64) -> Result<NovNativeBlockLedgerStatusV1> {
        self.ensure_schema_v1()?;
        let head = self.load_head_verified_inner_v1(chain_id)?;
        Ok(NovNativeBlockLedgerStatusV1 {
            schema: NOV_NATIVE_BLOCK_LEDGER_SCHEMA_V1.to_string(),
            path: self.path.display().to_string(),
            chain_id,
            canonical_local: head.is_some(),
            head,
            prepared: self.load_prepared_inner_v1(chain_id)?,
            safe: false,
            finalized: false,
            proof_sealed: false,
            max_txs_per_block: NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1,
            max_body_bytes: NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1,
        })
    }

    /// Return durable local-canonical blocks in ascending height order. This
    /// is the bounded read surface used to hydrate volatile network views.
    pub fn load_blocks_from_height(
        &self,
        chain_id: u64,
        from_height: u64,
        limit: usize,
    ) -> Result<Vec<NovNativeDurableBlockV1>> {
        self.ensure_schema_v1()?;
        if limit == 0 {
            return Ok(Vec::new());
        }
        let limit = limit.min(NOV_NATIVE_BLOCK_LEDGER_MAX_HYDRATE_BLOCKS_V1);
        let Some(head) = self.load_head_verified_inner_v1(chain_id)? else {
            return Ok(Vec::new());
        };
        let start = from_height.max(1);
        if start > head.height {
            return Ok(Vec::new());
        }
        let end = head
            .height
            .min(start.saturating_add(limit as u64).saturating_sub(1));
        let mut blocks = Vec::with_capacity(limit);
        let mut previous = if start > 1 {
            Some(
                self.load_by_height_inner_v1(chain_id, start - 1)?
                    .context("NOV native block range parent is missing")?,
            )
        } else {
            None
        };
        for height in start..=end {
            let block = self
                .load_by_height_inner_v1(chain_id, height)?
                .with_context(|| {
                    format!(
                        "NOV native block ledger has a canonical-local height gap: chain={chain_id} height={height}"
                    )
                })?;
            if let Some(parent) = previous.as_ref() {
                validate_block_continuity_v1(parent, &block)?;
            }
            previous = Some(block.clone());
            blocks.push(block);
        }
        Ok(blocks)
    }

    pub fn load_recent_blocks(
        &self,
        chain_id: u64,
        limit: usize,
    ) -> Result<Vec<NovNativeDurableBlockV1>> {
        self.ensure_schema_v1()?;
        if limit == 0 {
            return Ok(Vec::new());
        }
        let limit = limit.min(NOV_NATIVE_BLOCK_LEDGER_MAX_HYDRATE_BLOCKS_V1);
        let Some(head) = self.load_head_verified_inner_v1(chain_id)? else {
            return Ok(Vec::new());
        };
        let start = head
            .height
            .saturating_sub(limit as u64)
            .saturating_add(1)
            .max(1);
        self.load_blocks_from_height(chain_id, start, limit)
    }

    fn lock_writes_v1(&self) -> Result<MutexGuard<'_, ()>> {
        if self.read_only {
            bail!("NOV native block ledger was opened read-only");
        }
        self.write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("NOV native block ledger write lock is poisoned"))
    }

    fn ensure_schema_v1(&self) -> Result<()> {
        let raw = self
            .db
            .get(KEY_SCHEMA_V1)
            .context("read NOV native block ledger schema failed")?
            .context("NOV native block ledger schema is missing")?;
        if raw.as_slice() != NOV_NATIVE_BLOCK_LEDGER_SCHEMA_V1.as_bytes() {
            bail!(
                "unsupported NOV native block ledger schema: {}",
                String::from_utf8_lossy(raw.as_slice())
            );
        }
        Ok(())
    }

    fn load_head_inner_v1(&self, chain_id: u64) -> Result<Option<NovNativeBlockLedgerHeadV1>> {
        if chain_id == 0 {
            bail!("NOV native block ledger chain_id must be non-zero");
        }
        let head = read_json_v1::<NovNativeBlockLedgerHeadV1>(
            &self.db,
            head_key_v1(chain_id).as_bytes(),
            "block ledger head",
        )?;
        if let Some(head) = head.as_ref() {
            validate_head_v1(head, chain_id)?;
        }
        Ok(head)
    }

    fn load_aoem_ownership_inner_v1(&self) -> Result<Option<NovNativeBlockLedgerAoemOwnershipV1>> {
        let ownership = read_json_v1::<NovNativeBlockLedgerAoemOwnershipV1>(
            &self.db,
            KEY_AOEM_OWNERSHIP_V1,
            "AOEM ownership binding",
        )?;
        if let Some(ownership) = ownership.as_ref() {
            validate_aoem_ownership_v1(ownership)?;
        }
        Ok(ownership)
    }

    fn load_head_verified_inner_v1(
        &self,
        chain_id: u64,
    ) -> Result<Option<NovNativeBlockLedgerHeadV1>> {
        let Some(head) = self.load_head_inner_v1(chain_id)? else {
            return Ok(None);
        };
        let block = self
            .load_by_hash_inner_v1(chain_id, head.block_hash)?
            .context("NOV native block ledger head points to a missing durable block")?;
        if block.header.height != head.height
            || block.header.post_state_root != head.post_state_root
            || block.header.cumulative_receipt_root != head.cumulative_receipt_root
            || block.header.state_version != head.state_version
            || block.header.slot != head.slot
            || block.header.timestamp_unix_ms != head.timestamp_unix_ms
            || head.block_count != head.height
            || head.cumulative_tx_count < u64::from(block.header.tx_count)
            || head.cumulative_body_bytes < block.header.body_bytes
        {
            bail!("NOV native block ledger head binding mismatch");
        }
        Ok(Some(head))
    }

    fn load_prepared_inner_v1(&self, chain_id: u64) -> Result<Option<NovNativePreparedBlockV1>> {
        if chain_id == 0 {
            bail!("NOV native block ledger chain_id must be non-zero");
        }
        let Some(candidate_id) = self
            .db
            .get(candidate_current_key_v1(chain_id).as_bytes())
            .context("read NOV native current prepared candidate failed")?
        else {
            return Ok(None);
        };
        if candidate_id.len() != 32 {
            bail!("NOV native current prepared candidate id has an invalid length");
        }
        let mut candidate_id_bytes = [0u8; 32];
        candidate_id_bytes.copy_from_slice(candidate_id.as_slice());
        let prepared = read_json_v1::<NovNativePreparedBlockV1>(
            &self.db,
            candidate_key_v1(chain_id, &candidate_id_bytes).as_bytes(),
            "prepared candidate",
        )?
        .context("NOV native current prepared candidate payload is missing")?;
        validate_prepared_block_v1(&prepared)?;
        if prepared.context.chain_id != chain_id || prepared.candidate_id != candidate_id_bytes {
            bail!("NOV native current prepared candidate binding mismatch");
        }
        Ok(Some(prepared))
    }

    fn load_by_height_inner_v1(
        &self,
        chain_id: u64,
        height: u64,
    ) -> Result<Option<NovNativeDurableBlockV1>> {
        if chain_id == 0 || height == 0 {
            return Ok(None);
        }
        let Some(raw_hash) = self
            .db
            .get(height_key_v1(chain_id, height).as_bytes())
            .with_context(|| {
                format!(
                    "read NOV native block height index failed: chain={chain_id} height={height}"
                )
            })?
        else {
            return Ok(None);
        };
        if raw_hash.len() != 32 {
            bail!(
                "NOV native block height index hash length is invalid: chain={chain_id} height={height}"
            );
        }
        let mut block_hash = [0u8; 32];
        block_hash.copy_from_slice(raw_hash.as_slice());
        let block = self
            .load_by_hash_inner_v1(chain_id, block_hash)?
            .context("NOV native block height index points to a missing block")?;
        if block.header.height != height {
            bail!("NOV native block height index binding mismatch");
        }
        Ok(Some(block))
    }

    fn load_by_hash_inner_v1(
        &self,
        chain_id: u64,
        block_hash: [u8; 32],
    ) -> Result<Option<NovNativeDurableBlockV1>> {
        let Some(header) = read_json_v1::<NovNativeBlockHeaderV1>(
            &self.db,
            header_key_v1(chain_id, &block_hash).as_bytes(),
            "block header",
        )?
        else {
            return Ok(None);
        };
        let body = read_json_v1::<NovNativeBlockBodyV1>(
            &self.db,
            body_key_v1(chain_id, &block_hash).as_bytes(),
            "block body",
        )?
        .context("NOV native durable block body is missing")?;
        let execution_evidence = read_json_v1::<NovNativeBlockExecutionEvidenceV1>(
            &self.db,
            evidence_key_v1(chain_id, &block_hash).as_bytes(),
            "block execution evidence",
        )?
        .context("NOV native durable block execution evidence is missing")?;
        let block = NovNativeDurableBlockV1 {
            header,
            body,
            execution_evidence,
        };
        validate_durable_block_v1(&block)?;
        if block.header.chain_id != chain_id || block.header.block_hash != block_hash {
            bail!("NOV native durable block hash/domain binding mismatch");
        }
        let indexed_hash = self
            .db
            .get(height_key_v1(chain_id, block.header.height).as_bytes())
            .context("read NOV native block reverse height index failed")?
            .context("NOV native durable block reverse height index is missing")?;
        if indexed_hash.as_slice() != block_hash.as_slice() {
            bail!("NOV native durable block reverse height index mismatch");
        }
        for (index, (tx_hash, receipt_commitment)) in block
            .body
            .tx_hashes
            .iter()
            .zip(
                block
                    .execution_evidence
                    .per_block_receipt_commitments
                    .iter(),
            )
            .enumerate()
        {
            let location = self
                .load_tx_location_inner_v1(chain_id, *tx_hash)?
                .context("NOV native durable block transaction index is missing")?;
            if location.height != block.header.height
                || location.block_hash != block_hash
                || location.tx_index as usize != index
            {
                bail!("NOV native durable block transaction index binding mismatch");
            }
            let receipt = read_json_v1::<NovNativeBlockReceiptLocationV1>(
                &self.db,
                receipt_key_v1(chain_id, tx_hash).as_bytes(),
                "receipt location",
            )?
            .context("NOV native durable block receipt index is missing")?;
            validate_receipt_location_v1(&receipt, chain_id, *tx_hash)?;
            if receipt.height != block.header.height
                || receipt.block_hash != block_hash
                || receipt.tx_index as usize != index
                || receipt.receipt_commitment != *receipt_commitment
            {
                bail!("NOV native durable block receipt index binding mismatch");
            }
        }
        for (kind, id) in [
            ("batch", block.header.aoem_batch_id.as_str()),
            ("result", block.header.aoem_batch_result_id.as_str()),
        ] {
            let index = read_json_v1::<NovNativeBlockExternalIdIndexV1>(
                &self.db,
                external_id_key_v1(chain_id, kind, id).as_bytes(),
                "AOEM external id index",
            )?
            .context("NOV native durable block AOEM external id index is missing")?;
            if index.schema != EXTERNAL_ID_INDEX_SCHEMA_V1
                || index.id_kind != kind
                || index.exact_id != id
                || index.chain_id != chain_id
                || index.height != block.header.height
                || index.block_hash != block_hash
            {
                bail!("NOV native durable block AOEM external id index binding mismatch");
            }
        }
        Ok(Some(block))
    }

    fn load_tx_location_inner_v1(
        &self,
        chain_id: u64,
        tx_hash: [u8; 32],
    ) -> Result<Option<NovNativeBlockTxLocationV1>> {
        let location = read_json_v1::<NovNativeBlockTxLocationV1>(
            &self.db,
            tx_key_v1(chain_id, &tx_hash).as_bytes(),
            "transaction location",
        )?;
        if let Some(location) = location.as_ref() {
            validate_tx_location_v1(location, chain_id, tx_hash)?;
        }
        Ok(location)
    }

    fn load_candidate_record_inner_v1(
        &self,
        chain_id: u64,
        block_hash: [u8; 32],
    ) -> Result<Option<NovNativeBlockCandidateRecordV1>> {
        if chain_id == 0 {
            bail!("NOV native candidate graph chain_id must be non-zero");
        }
        if let Some(record) = read_json_v1::<NovNativeBlockCandidateRecordV1>(
            &self.db,
            candidate_record_key_v1(chain_id, &block_hash).as_bytes(),
            "block candidate record",
        )? {
            self.ensure_candidate_graph_schema_v1()?;
            validate_candidate_record_v1(&record)?;
            if record.chain_id != chain_id || record.block_hash != block_hash {
                bail!("NOV native candidate record key binding mismatch");
            }
            let block = self
                .load_candidate_block_for_record_inner_v1(&record)?
                .context("NOV native candidate record points to a missing block artifact")?;
            validate_candidate_record_against_block_v1(&record, &block)?;
            return Ok(Some(record));
        }

        // Databases written before candidate_graph/v1 remain readable without
        // an eager migration. Their selected local block is represented by a
        // deterministic synthesized record and is backfilled by repeat commit.
        let Some(block) = self.load_by_hash_inner_v1(chain_id, block_hash)? else {
            return Ok(None);
        };
        self.validate_legacy_local_candidate_parent_inner_v1(&block)?;
        Ok(Some(candidate_record_from_block_v1(
            &block,
            CANDIDATE_SOURCE_LOCAL_AOEM_V1,
            true,
            true,
        )?))
    }

    fn load_candidate_block_for_record_inner_v1(
        &self,
        record: &NovNativeBlockCandidateRecordV1,
    ) -> Result<Option<NovNativeDurableBlockV1>> {
        let block = match record.candidate_source.as_str() {
            CANDIDATE_SOURCE_LOCAL_AOEM_V1 => {
                self.load_by_hash_inner_v1(record.chain_id, record.block_hash)?
            }
            CANDIDATE_SOURCE_OBSERVED_V1 => {
                let block = read_json_v1::<NovNativeDurableBlockV1>(
                    &self.db,
                    candidate_artifact_key_v1(record.chain_id, &record.block_hash).as_bytes(),
                    "observed block candidate artifact",
                )?;
                if let Some(block) = block.as_ref() {
                    validate_durable_block_v1(block)?;
                    if block.header.chain_id != record.chain_id
                        || block.header.block_hash != record.block_hash
                    {
                        bail!("NOV native observed candidate artifact key binding mismatch");
                    }
                }
                block
            }
            _ => bail!("NOV native candidate record has an unsupported source"),
        };
        if let Some(block) = block.as_ref() {
            validate_candidate_record_against_block_v1(record, block)?;
        }
        Ok(block)
    }

    fn load_candidate_height_index_inner_v1(
        &self,
        chain_id: u64,
        height: u64,
    ) -> Result<Option<NovNativeBlockCandidateHeightIndexV1>> {
        let index = read_json_v1::<NovNativeBlockCandidateHeightIndexV1>(
            &self.db,
            candidate_height_index_key_v1(chain_id, height).as_bytes(),
            "block candidate height index",
        )?;
        if let Some(index) = index.as_ref() {
            self.ensure_candidate_graph_schema_v1()?;
            validate_candidate_height_index_v1(index, chain_id, height)?;
        }
        Ok(index)
    }

    fn load_candidate_children_index_inner_v1(
        &self,
        chain_id: u64,
        parent_block_hash: [u8; 32],
    ) -> Result<Option<NovNativeBlockCandidateChildrenIndexV1>> {
        let index = read_json_v1::<NovNativeBlockCandidateChildrenIndexV1>(
            &self.db,
            candidate_children_index_key_v1(chain_id, &parent_block_hash).as_bytes(),
            "block candidate children index",
        )?;
        if let Some(index) = index.as_ref() {
            self.ensure_candidate_graph_schema_v1()?;
            validate_candidate_children_index_v1(index, chain_id, parent_block_hash)?;
        }
        Ok(index)
    }

    fn load_candidate_records_by_height_inner_v1(
        &self,
        chain_id: u64,
        height: u64,
    ) -> Result<Vec<NovNativeBlockCandidateRecordV1>> {
        if chain_id == 0 || height == 0 {
            return Ok(Vec::new());
        }
        let mut block_hashes = self
            .load_candidate_height_index_inner_v1(chain_id, height)?
            .map_or_else(Vec::new, |index| index.block_hashes);
        if let Some(local) = self.load_by_height_inner_v1(chain_id, height)? {
            block_hashes.push(local.header.block_hash);
        }
        sort_and_validate_candidate_hashes_v1(&mut block_hashes)?;
        let mut records = Vec::with_capacity(block_hashes.len());
        for block_hash in block_hashes {
            let record = self
                .load_candidate_record_inner_v1(chain_id, block_hash)?
                .context("NOV native candidate height index points to a missing record")?;
            if record.height != height {
                bail!("NOV native candidate height index binding mismatch");
            }
            records.push(record);
        }
        Ok(records)
    }

    fn load_candidate_children_inner_v1(
        &self,
        chain_id: u64,
        parent_block_hash: [u8; 32],
    ) -> Result<Vec<NovNativeBlockCandidateRecordV1>> {
        if chain_id == 0 {
            bail!("NOV native candidate graph chain_id must be non-zero");
        }
        let mut block_hashes = self
            .load_candidate_children_index_inner_v1(chain_id, parent_block_hash)?
            .map_or_else(Vec::new, |index| index.block_hashes);

        // Compatibility fallback for a selected block written before the graph
        // extension existed.
        let next_height = if parent_block_hash == [0u8; 32] {
            Some(1)
        } else {
            self.load_candidate_record_inner_v1(chain_id, parent_block_hash)?
                .and_then(|parent| parent.height.checked_add(1))
        };
        if let Some(next_height) = next_height {
            if let Some(local_child) = self.load_by_height_inner_v1(chain_id, next_height)? {
                if local_child.header.parent_block_hash == parent_block_hash {
                    block_hashes.push(local_child.header.block_hash);
                }
            }
        }

        sort_and_validate_candidate_hashes_v1(&mut block_hashes)?;
        let mut records = Vec::with_capacity(block_hashes.len());
        for block_hash in block_hashes {
            let record = self
                .load_candidate_record_inner_v1(chain_id, block_hash)?
                .context("NOV native candidate children index points to a missing record")?;
            if record.parent_block_hash != parent_block_hash {
                bail!("NOV native candidate children index binding mismatch");
            }
            records.push(record);
        }
        Ok(records)
    }

    fn validate_candidate_parent_inner_v1(&self, block: &NovNativeDurableBlockV1) -> Result<()> {
        if block.header.height == 1 {
            if block.header.parent_block_hash != [0u8; 32] || block.header.aoem_parent.is_some() {
                bail!("NOV native candidate genesis parent binding is invalid");
            }
            if self
                .load_candidate_records_by_height_inner_v1(block.header.chain_id, 1)?
                .iter()
                .any(|record| record.block_hash != block.header.block_hash)
            {
                bail!("NOV native candidate graph rejects a competing genesis block");
            }
            return Ok(());
        }
        if block.header.parent_block_hash == [0u8; 32] {
            bail!("NOV native non-genesis candidate has a zero parent hash");
        }
        let parent_record = self
            .load_candidate_record_inner_v1(block.header.chain_id, block.header.parent_block_hash)?
            .context("NOV native candidate parent is missing")?;
        if parent_record.lifecycle_status != CANDIDATE_STATUS_ACTIVE_V1 {
            bail!("NOV native candidate cannot extend an aborted parent");
        }
        let parent = self
            .load_candidate_block_for_record_inner_v1(&parent_record)?
            .context("NOV native candidate parent artifact is missing")?;
        validate_block_continuity_v1(&parent, block)
            .context("validate NOV native candidate parent continuity")
    }

    fn validate_legacy_local_candidate_parent_inner_v1(
        &self,
        block: &NovNativeDurableBlockV1,
    ) -> Result<()> {
        if block.header.height == 1 {
            if block.header.parent_block_hash != [0u8; 32] || block.header.aoem_parent.is_some() {
                bail!("NOV native legacy genesis candidate parent binding is invalid");
            }
            return Ok(());
        }
        let parent_height = block
            .header
            .height
            .checked_sub(1)
            .context("NOV native legacy candidate parent height underflow")?;
        let parent = self
            .load_by_height_inner_v1(block.header.chain_id, parent_height)?
            .context("NOV native legacy candidate parent block is missing")?;
        validate_block_continuity_v1(&parent, block)
            .context("validate NOV native legacy candidate parent continuity")
    }

    fn stage_candidate_graph_record_v1(
        &self,
        batch: &mut RocksDbWriteBatch,
        record: &NovNativeBlockCandidateRecordV1,
    ) -> Result<()> {
        validate_candidate_record_v1(record)?;
        if let Some(raw) = self
            .db
            .get(KEY_CANDIDATE_GRAPH_SCHEMA_V1)
            .context("read existing NOV native candidate graph schema failed")?
        {
            if raw.as_slice() != NOV_NATIVE_BLOCK_CANDIDATE_GRAPH_SCHEMA_V1.as_bytes() {
                bail!(
                    "unsupported NOV native candidate graph schema: {}",
                    String::from_utf8_lossy(raw.as_slice())
                );
            }
        }
        if let Some(existing) = read_json_v1::<NovNativeBlockCandidateRecordV1>(
            &self.db,
            candidate_record_key_v1(record.chain_id, &record.block_hash).as_bytes(),
            "existing block candidate record",
        )? {
            validate_candidate_record_v1(&existing)?;
            if existing != *record
                && !(existing.candidate_source == CANDIDATE_SOURCE_OBSERVED_V1
                    && existing.lifecycle_status == CANDIDATE_STATUS_ACTIVE_V1
                    && record.candidate_source == CANDIDATE_SOURCE_LOCAL_AOEM_V1
                    && candidate_records_bind_same_artifact_v1(&existing, record))
            {
                bail!("NOV native candidate record conflicts with an existing lifecycle record");
            }
        }

        let mut height_index = self
            .load_candidate_height_index_inner_v1(record.chain_id, record.height)?
            .unwrap_or_else(|| NovNativeBlockCandidateHeightIndexV1 {
                schema: CANDIDATE_HEIGHT_INDEX_SCHEMA_V1.to_string(),
                chain_id: record.chain_id,
                height: record.height,
                block_hashes: Vec::new(),
            });
        height_index.block_hashes.push(record.block_hash);
        sort_and_validate_candidate_hashes_v1(&mut height_index.block_hashes)?;

        let mut children_index = self
            .load_candidate_children_index_inner_v1(record.chain_id, record.parent_block_hash)?
            .unwrap_or_else(|| NovNativeBlockCandidateChildrenIndexV1 {
                schema: CANDIDATE_CHILDREN_INDEX_SCHEMA_V1.to_string(),
                chain_id: record.chain_id,
                parent_block_hash: record.parent_block_hash,
                block_hashes: Vec::new(),
            });
        children_index.block_hashes.push(record.block_hash);
        sort_and_validate_candidate_hashes_v1(&mut children_index.block_hashes)?;

        put_json_v1(
            batch,
            candidate_record_key_v1(record.chain_id, &record.block_hash).as_bytes(),
            record,
            "block candidate record",
        )?;
        put_json_v1(
            batch,
            candidate_height_index_key_v1(record.chain_id, record.height).as_bytes(),
            &height_index,
            "block candidate height index",
        )?;
        put_json_v1(
            batch,
            candidate_children_index_key_v1(record.chain_id, &record.parent_block_hash).as_bytes(),
            &children_index,
            "block candidate children index",
        )?;
        batch.put(
            KEY_CANDIDATE_GRAPH_SCHEMA_V1,
            NOV_NATIVE_BLOCK_CANDIDATE_GRAPH_SCHEMA_V1.as_bytes(),
        );
        Ok(())
    }

    fn ensure_candidate_graph_schema_v1(&self) -> Result<()> {
        let raw = self
            .db
            .get(KEY_CANDIDATE_GRAPH_SCHEMA_V1)
            .context("read NOV native candidate graph schema failed")?
            .context("NOV native candidate graph schema is missing")?;
        if raw.as_slice() != NOV_NATIVE_BLOCK_CANDIDATE_GRAPH_SCHEMA_V1.as_bytes() {
            bail!(
                "unsupported NOV native candidate graph schema: {}",
                String::from_utf8_lossy(raw.as_slice())
            );
        }
        Ok(())
    }

    fn validate_prepared_against_head_v1(&self, prepared: &NovNativePreparedBlockV1) -> Result<()> {
        let head = self.load_head_verified_inner_v1(prepared.context.chain_id)?;
        match head {
            None => {
                if prepared.context.block_height != 1
                    || prepared.context.parent_block_hash != [0u8; 32]
                {
                    bail!("NOV native genesis candidate must be height 1 with a zero parent hash");
                }
            }
            Some(head) => {
                if prepared.context.block_height != head.height.saturating_add(1) {
                    bail!(
                        "NOV native block height is not contiguous: head={} requested={}",
                        head.height,
                        prepared.context.block_height
                    );
                }
                if prepared.context.parent_block_hash != head.block_hash {
                    bail!("NOV native block parent hash does not match durable head");
                }
                if prepared.pre_state_root != head.post_state_root {
                    bail!("NOV native block pre-state root does not match durable head");
                }
                let parent = self
                    .load_by_hash_inner_v1(prepared.context.chain_id, head.block_hash)?
                    .context("NOV native block ledger head block is missing")?;
                let expected_aoem_parent = NovNativePreparedAoemParentV1 {
                    batch_id: parent.header.aoem_batch_id,
                    batch_result_id: parent.header.aoem_batch_result_id,
                    state_root: parent.header.post_state_root,
                    state_root_codec: parent.header.post_state_root_codec,
                    cumulative_receipt_root: parent.header.cumulative_receipt_root,
                    receipt_root_codec: parent.header.cumulative_receipt_root_codec,
                    state_version: parent.header.state_version,
                };
                let Some(actual_aoem_parent) = prepared.aoem_parent.as_ref() else {
                    bail!("NOV native child candidate is missing its AOEM parent commitment");
                };
                if actual_aoem_parent.batch_id != expected_aoem_parent.batch_id
                    || actual_aoem_parent.batch_result_id != expected_aoem_parent.batch_result_id
                    || actual_aoem_parent.state_root != expected_aoem_parent.state_root
                    || actual_aoem_parent.state_root_codec != expected_aoem_parent.state_root_codec
                    || actual_aoem_parent.cumulative_receipt_root
                        != expected_aoem_parent.cumulative_receipt_root
                    || actual_aoem_parent.receipt_root_codec
                        != expected_aoem_parent.receipt_root_codec
                    || actual_aoem_parent.state_version != expected_aoem_parent.state_version
                {
                    bail!("NOV native child AOEM parent does not match durable head evidence");
                }
                if prepared.context.slot <= head.slot {
                    bail!(
                        "NOV native block slot must advance: head={} requested={}",
                        head.slot,
                        prepared.context.slot
                    );
                }
                if prepared.context.timestamp_unix_ms < head.timestamp_unix_ms {
                    bail!(
                        "NOV native block timestamp regressed: head={} requested={}",
                        head.timestamp_unix_ms,
                        prepared.context.timestamp_unix_ms
                    );
                }
            }
        }
        Ok(())
    }

    fn ensure_block_hash_available_v1(&self, block: &NovNativeDurableBlockV1) -> Result<()> {
        if let Some(existing) =
            self.load_by_hash_inner_v1(block.header.chain_id, block.header.block_hash)?
        {
            if existing == *block {
                return Ok(());
            }
            bail!("NOV native block hash collision or conflicting block payload");
        }
        Ok(())
    }

    fn ensure_external_id_available_v1(
        &self,
        kind: &str,
        id: &str,
        block: &NovNativeDurableBlockV1,
    ) -> Result<()> {
        let key = external_id_key_v1(block.header.chain_id, kind, id);
        let Some(index) = read_json_v1::<NovNativeBlockExternalIdIndexV1>(
            &self.db,
            key.as_bytes(),
            "AOEM external id index",
        )?
        else {
            return Ok(());
        };
        if index.schema != EXTERNAL_ID_INDEX_SCHEMA_V1
            || index.id_kind != kind
            || index.exact_id != id
            || index.chain_id != block.header.chain_id
            || index.height != block.header.height
            || index.block_hash != block.header.block_hash
        {
            bail!("NOV native AOEM {kind} id conflicts with an existing block");
        }
        Ok(())
    }
}

fn validate_aoem_ownership_v1(ownership: &NovNativeBlockLedgerAoemOwnershipV1) -> Result<()> {
    if ownership.schema != AOEM_OWNERSHIP_SCHEMA_V1 || ownership.chain_id == 0 {
        bail!("NOV native block ledger AOEM ownership binding metadata is invalid");
    }
    for (label, value) in [
        ("namespace digest", ownership.namespace_digest.as_str()),
        (
            "protocol config commitment",
            ownership.protocol_config_commitment.as_str(),
        ),
    ] {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            bail!("NOV native block ledger AOEM ownership {label} is not canonical hex");
        }
    }
    Ok(())
}

fn candidate_record_from_block_v1(
    block: &NovNativeDurableBlockV1,
    candidate_source: &str,
    local_aoem_readback_verified: bool,
    execution_selected_local: bool,
) -> Result<NovNativeBlockCandidateRecordV1> {
    validate_durable_block_v1(block)?;
    let record = NovNativeBlockCandidateRecordV1 {
        schema: CANDIDATE_RECORD_SCHEMA_V1.to_string(),
        chain_id: block.header.chain_id,
        height: block.header.height,
        slot: block.header.slot,
        timestamp_unix_ms: block.header.timestamp_unix_ms,
        block_hash: block.header.block_hash,
        parent_block_hash: block.header.parent_block_hash,
        candidate_id: block.header.candidate_id,
        execution_context_commitment: block.header.execution_context_commitment,
        pre_state_root: block.header.pre_state_root,
        post_state_root: block.header.post_state_root,
        ordered_tx_root: block.header.ordered_tx_root,
        block_receipt_root: block.header.block_receipt_root,
        cumulative_receipt_root: block.header.cumulative_receipt_root,
        body_digest: block.header.body_digest,
        body_bytes: block.header.body_bytes,
        tx_count: block.header.tx_count,
        state_version: block.header.state_version,
        aoem_batch_id: block.header.aoem_batch_id.clone(),
        aoem_batch_result_id: block.header.aoem_batch_result_id.clone(),
        aoem_evidence_commitment: block.header.aoem_evidence_commitment,
        candidate_source: candidate_source.to_string(),
        lifecycle_status: CANDIDATE_STATUS_ACTIVE_V1.to_string(),
        revision: 1,
        commitment_bindings_verified: true,
        parent_continuity_verified: true,
        body_data_available: true,
        local_aoem_readback_verified,
        execution_selected_local,
        fork_choice_selected: false,
        chain_canonical: false,
        proof_sealed: false,
        safe: false,
        finalized: false,
        abort_reason: None,
    };
    validate_candidate_record_v1(&record)?;
    validate_candidate_record_against_block_v1(&record, block)?;
    Ok(record)
}

fn validate_candidate_record_v1(record: &NovNativeBlockCandidateRecordV1) -> Result<()> {
    if record.schema != CANDIDATE_RECORD_SCHEMA_V1
        || record.chain_id == 0
        || record.height == 0
        || record.revision == 0
        || !record.commitment_bindings_verified
        || !record.parent_continuity_verified
        || !record.body_data_available
        || record.fork_choice_selected
        || record.chain_canonical
        || record.proof_sealed
        || record.safe
        || record.finalized
    {
        bail!("NOV native candidate record is invalid or falsely sealed");
    }
    match record.candidate_source.as_str() {
        CANDIDATE_SOURCE_LOCAL_AOEM_V1 => {
            if !record.local_aoem_readback_verified || !record.execution_selected_local {
                bail!("NOV native local candidate record lacks AOEM execution ownership");
            }
        }
        CANDIDATE_SOURCE_OBSERVED_V1 => {
            if record.local_aoem_readback_verified || record.execution_selected_local {
                bail!("NOV native observed candidate falsely claims local AOEM execution");
            }
        }
        _ => bail!("NOV native candidate record source is unsupported"),
    }
    match record.lifecycle_status.as_str() {
        CANDIDATE_STATUS_ACTIVE_V1 if record.abort_reason.is_none() => {}
        CANDIDATE_STATUS_ABORTED_V1
            if record.candidate_source == CANDIDATE_SOURCE_OBSERVED_V1
                && !record.execution_selected_local
                && record.abort_reason.is_some() =>
        {
            validate_candidate_abort_reason_v1(record.abort_reason.as_deref().unwrap_or_default())?;
        }
        _ => bail!("NOV native candidate lifecycle status is invalid"),
    }
    validate_external_id_v1("candidate AOEM batch id", record.aoem_batch_id.as_str())?;
    validate_hex_commitment_v1(
        "candidate AOEM batch result id",
        record.aoem_batch_result_id.as_str(),
    )?;
    Ok(())
}

fn validate_candidate_record_against_block_v1(
    record: &NovNativeBlockCandidateRecordV1,
    block: &NovNativeDurableBlockV1,
) -> Result<()> {
    validate_durable_block_v1(block)?;
    if record.chain_id != block.header.chain_id
        || record.height != block.header.height
        || record.slot != block.header.slot
        || record.timestamp_unix_ms != block.header.timestamp_unix_ms
        || record.block_hash != block.header.block_hash
        || record.parent_block_hash != block.header.parent_block_hash
        || record.candidate_id != block.header.candidate_id
        || record.execution_context_commitment != block.header.execution_context_commitment
        || record.pre_state_root != block.header.pre_state_root
        || record.post_state_root != block.header.post_state_root
        || record.ordered_tx_root != block.header.ordered_tx_root
        || record.block_receipt_root != block.header.block_receipt_root
        || record.cumulative_receipt_root != block.header.cumulative_receipt_root
        || record.body_digest != block.header.body_digest
        || record.body_bytes != block.header.body_bytes
        || record.tx_count != block.header.tx_count
        || record.state_version != block.header.state_version
        || record.aoem_batch_id != block.header.aoem_batch_id
        || record.aoem_batch_result_id != block.header.aoem_batch_result_id
        || record.aoem_evidence_commitment != block.header.aoem_evidence_commitment
    {
        bail!("NOV native candidate record/artifact commitment binding mismatch");
    }
    Ok(())
}

fn candidate_records_bind_same_artifact_v1(
    left: &NovNativeBlockCandidateRecordV1,
    right: &NovNativeBlockCandidateRecordV1,
) -> bool {
    left.chain_id == right.chain_id
        && left.height == right.height
        && left.slot == right.slot
        && left.timestamp_unix_ms == right.timestamp_unix_ms
        && left.block_hash == right.block_hash
        && left.parent_block_hash == right.parent_block_hash
        && left.candidate_id == right.candidate_id
        && left.execution_context_commitment == right.execution_context_commitment
        && left.pre_state_root == right.pre_state_root
        && left.post_state_root == right.post_state_root
        && left.ordered_tx_root == right.ordered_tx_root
        && left.block_receipt_root == right.block_receipt_root
        && left.cumulative_receipt_root == right.cumulative_receipt_root
        && left.body_digest == right.body_digest
        && left.body_bytes == right.body_bytes
        && left.tx_count == right.tx_count
        && left.state_version == right.state_version
        && left.aoem_batch_id == right.aoem_batch_id
        && left.aoem_batch_result_id == right.aoem_batch_result_id
        && left.aoem_evidence_commitment == right.aoem_evidence_commitment
}

fn validate_candidate_height_index_v1(
    index: &NovNativeBlockCandidateHeightIndexV1,
    chain_id: u64,
    height: u64,
) -> Result<()> {
    if index.schema != CANDIDATE_HEIGHT_INDEX_SCHEMA_V1
        || index.chain_id != chain_id
        || index.height != height
    {
        bail!("NOV native candidate height index metadata is invalid");
    }
    validate_stored_candidate_hashes_v1(index.block_hashes.as_slice())
}

fn validate_candidate_children_index_v1(
    index: &NovNativeBlockCandidateChildrenIndexV1,
    chain_id: u64,
    parent_block_hash: [u8; 32],
) -> Result<()> {
    if index.schema != CANDIDATE_CHILDREN_INDEX_SCHEMA_V1
        || index.chain_id != chain_id
        || index.parent_block_hash != parent_block_hash
    {
        bail!("NOV native candidate children index metadata is invalid");
    }
    validate_stored_candidate_hashes_v1(index.block_hashes.as_slice())
}

fn validate_stored_candidate_hashes_v1(block_hashes: &[[u8; 32]]) -> Result<()> {
    if block_hashes.is_empty()
        || block_hashes.len() > NOV_NATIVE_BLOCK_CANDIDATE_GRAPH_MAX_LINKS_V1
        || block_hashes.windows(2).any(|pair| pair[0] >= pair[1])
    {
        bail!("NOV native candidate graph index is empty, unsorted, duplicated, or oversized");
    }
    Ok(())
}

fn sort_and_validate_candidate_hashes_v1(block_hashes: &mut Vec<[u8; 32]>) -> Result<()> {
    block_hashes.sort_unstable();
    block_hashes.dedup();
    if block_hashes.len() > NOV_NATIVE_BLOCK_CANDIDATE_GRAPH_MAX_LINKS_V1 {
        bail!("NOV native candidate graph index exceeds the bounded link limit");
    }
    Ok(())
}

fn validate_candidate_abort_reason_v1(reason: &str) -> Result<String> {
    let reason = reason.trim();
    if reason.is_empty()
        || reason.len() > CANDIDATE_ABORT_REASON_MAX_BYTES_V1
        || reason.chars().any(char::is_control)
    {
        bail!("NOV native candidate abort reason is empty, oversized, or contains control data");
    }
    Ok(reason.to_string())
}

fn native_block_ledger_process_registry_v1(
) -> &'static Mutex<HashMap<String, Weak<NovNativeBlockLedgerProcessEntryV1>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Weak<NovNativeBlockLedgerProcessEntryV1>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn native_block_ledger_process_key_v1(path: &Path) -> Result<String> {
    let absolute = std::path::absolute(path)
        .with_context(|| format!("resolve NOV native block ledger path: {}", path.display()))?;
    let mut key = absolute.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        key.make_ascii_lowercase();
    }
    Ok(key)
}

pub fn nov_native_ordered_tx_root_v1(tx_hashes: &[[u8; 32]]) -> Result<[u8; 32]> {
    if tx_hashes.is_empty() {
        bail!("NOV native block must contain at least one transaction");
    }
    if tx_hashes.len() > NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1 {
        bail!(
            "NOV native block transaction count exceeds {}",
            NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1
        );
    }
    let mut hasher = Sha256::new();
    hasher.update(ORDERED_TX_ROOT_DOMAIN_V1);
    hasher.update((tx_hashes.len() as u64).to_be_bytes());
    for tx_hash in tx_hashes {
        hasher.update(tx_hash);
    }
    Ok(hasher.finalize().into())
}

pub fn nov_native_block_receipt_root_v1(
    tx_hashes: &[[u8; 32]],
    receipt_commitments: &[[u8; 32]],
) -> Result<[u8; 32]> {
    if tx_hashes.is_empty() || tx_hashes.len() != receipt_commitments.len() {
        bail!("NOV native block receipt commitments must match the ordered transaction count");
    }
    let mut hasher = Sha256::new();
    hasher.update(BLOCK_RECEIPT_ROOT_DOMAIN_V1);
    hasher.update((tx_hashes.len() as u64).to_be_bytes());
    for (tx_hash, receipt_commitment) in tx_hashes.iter().zip(receipt_commitments) {
        hasher.update(tx_hash);
        hasher.update(receipt_commitment);
    }
    Ok(hasher.finalize().into())
}

fn build_prepared_block_v1(
    input: NovNativeBlockCandidateInputV1,
) -> Result<NovNativePreparedBlockV1> {
    input
        .context
        .validate()
        .context("invalid NOV native block execution context")?;
    validate_body_limits_v1(input.tx_hashes.as_slice(), input.raw_txs.as_slice())?;
    let mut unique = HashSet::with_capacity(input.tx_hashes.len());
    if input.tx_hashes.iter().any(|hash| !unique.insert(*hash)) {
        bail!("NOV native block contains a duplicate transaction hash");
    }
    let body_bytes = input.raw_txs.iter().try_fold(0u64, |total, raw| {
        total
            .checked_add(raw.len() as u64)
            .context("NOV native block body byte count overflow")
    })?;
    let context_commitment = input
        .context
        .commitment()
        .context("commit NOV native block execution context")?;
    let ordered_tx_root = nov_native_ordered_tx_root_v1(input.tx_hashes.as_slice())?;
    let body_digest = body_digest_v1(input.tx_hashes.as_slice(), input.raw_txs.as_slice());
    let candidate_id = candidate_id_v1(
        &context_commitment,
        &input.pre_state_root,
        input.aoem_parent.as_ref(),
        &ordered_tx_root,
        &body_digest,
        body_bytes,
    );
    Ok(NovNativePreparedBlockV1 {
        schema: PREPARED_SCHEMA_V1.to_string(),
        candidate_id,
        context: input.context,
        context_commitment,
        pre_state_root: input.pre_state_root,
        ordered_tx_root,
        body_digest,
        body_bytes,
        tx_hashes: input.tx_hashes,
        raw_txs: input.raw_txs,
        aoem_parent: input.aoem_parent,
        expected_aoem_batch_id: None,
        expected_aoem_output_commitment: None,
    })
}

fn build_durable_block_v1(
    prepared: &NovNativePreparedBlockV1,
    input: NovNativeBlockCommitInputV1,
) -> Result<NovNativeDurableBlockV1> {
    validate_prepared_block_v1(prepared)?;
    validate_commit_input_semantics_v1(prepared, &input)?;
    let block_receipt_root = nov_native_block_receipt_root_v1(
        prepared.tx_hashes.as_slice(),
        input.per_block_receipt_commitments.as_slice(),
    )?;
    let block_hash = block_hash_v1(prepared, &input, &block_receipt_root);
    let tx_count = u32::try_from(prepared.tx_hashes.len())
        .context("NOV native block transaction count exceeds u32")?;
    let header = NovNativeBlockHeaderV1 {
        schema: HEADER_SCHEMA_V1.to_string(),
        candidate_kind: CANDIDATE_KIND_V1.to_string(),
        execution_context: prepared.context,
        chain_id: prepared.context.chain_id,
        height: prepared.context.block_height,
        slot: prepared.context.slot,
        timestamp_unix_ms: prepared.context.timestamp_unix_ms,
        parent_block_hash: prepared.context.parent_block_hash,
        block_hash,
        candidate_id: prepared.candidate_id,
        execution_context_commitment: prepared.context_commitment,
        pre_state_root: prepared.pre_state_root,
        aoem_parent: prepared.aoem_parent.clone(),
        post_state_root: input.post_state_root,
        post_state_root_codec: POST_STATE_ROOT_CODEC_V1.to_string(),
        ordered_tx_root: prepared.ordered_tx_root,
        block_receipt_root,
        cumulative_receipt_root: input.cumulative_receipt_root,
        cumulative_receipt_root_codec: CUMULATIVE_RECEIPT_ROOT_CODEC_V1.to_string(),
        body_digest: prepared.body_digest,
        body_bytes: prepared.body_bytes,
        tx_count,
        receipt_count: tx_count,
        state_version: input.state_version,
        aoem_batch_id: input.aoem_batch_id.clone(),
        aoem_batch_result_id: input.aoem_batch_result_id.clone(),
        aoem_expected_output_commitment: prepared
            .expected_aoem_output_commitment
            .clone()
            .context("NOV native prepared candidate has no AOEM output commitment binding")?,
        aoem_evidence_commitment: input.aoem_evidence_commitment,
        aoem_readback_verified: true,
        canonical_local: true,
        safe: false,
        finalized: false,
        proof_sealed: false,
    };
    let body = NovNativeBlockBodyV1 {
        schema: BODY_SCHEMA_V1.to_string(),
        chain_id: prepared.context.chain_id,
        height: prepared.context.block_height,
        block_hash,
        ordered_tx_root: prepared.ordered_tx_root,
        body_digest: prepared.body_digest,
        body_bytes: prepared.body_bytes,
        tx_hashes: prepared.tx_hashes.clone(),
        raw_txs: prepared.raw_txs.clone(),
    };
    let execution_evidence = NovNativeBlockExecutionEvidenceV1 {
        schema: EVIDENCE_SCHEMA_V1.to_string(),
        chain_id: prepared.context.chain_id,
        height: prepared.context.block_height,
        block_hash,
        aoem_batch_id: input.aoem_batch_id,
        aoem_batch_result_id: input.aoem_batch_result_id,
        aoem_expected_output_commitment: prepared
            .expected_aoem_output_commitment
            .clone()
            .context("NOV native prepared candidate has no AOEM output commitment binding")?,
        aoem_evidence_commitment: input.aoem_evidence_commitment,
        post_state_root: input.post_state_root,
        cumulative_receipt_root: input.cumulative_receipt_root,
        block_receipt_root,
        per_block_receipt_commitments: input.per_block_receipt_commitments,
        state_version: input.state_version,
        evidence_kind: "aoem_execution_commitment_not_consensus_seal".to_string(),
        proof_sealed: false,
    };
    let block = NovNativeDurableBlockV1 {
        header,
        body,
        execution_evidence,
    };
    validate_durable_block_v1(&block)?;
    Ok(block)
}

fn validate_commit_input_semantics_v1(
    prepared: &NovNativePreparedBlockV1,
    input: &NovNativeBlockCommitInputV1,
) -> Result<()> {
    let expected_aoem_batch_id = prepared
        .expected_aoem_batch_id
        .as_deref()
        .context("NOV native prepared candidate has no AOEM batch id binding")?;
    if expected_aoem_batch_id != input.aoem_batch_id {
        bail!("NOV native AOEM result batch id does not match the prepared candidate binding");
    }
    if input.post_state_root == [0u8; 32] {
        bail!("NOV native block post-state root must not be zero");
    }
    if input.cumulative_receipt_root == [0u8; 32] {
        bail!("NOV native cumulative receipt root must not be zero");
    }
    if input.aoem_evidence_commitment == [0u8; 32] {
        bail!("NOV native AOEM evidence commitment must not be zero");
    }
    validate_external_id_v1("AOEM batch id", input.aoem_batch_id.as_str())?;
    validate_hex_commitment_v1("AOEM batch result id", input.aoem_batch_result_id.as_str())?;
    if input.state_version == 0 {
        bail!("NOV native block state_version must be non-zero");
    }
    if prepared
        .aoem_parent
        .as_ref()
        .is_some_and(|parent| input.state_version <= parent.state_version)
    {
        bail!("NOV native block state_version must advance beyond its AOEM parent");
    }
    Ok(())
}

fn validate_prepared_block_v1(prepared: &NovNativePreparedBlockV1) -> Result<()> {
    if prepared.schema != PREPARED_SCHEMA_V1 {
        bail!(
            "unsupported NOV native prepared block schema: {}",
            prepared.schema
        );
    }
    prepared
        .context
        .validate()
        .context("invalid prepared NOV native block execution context")?;
    validate_body_limits_v1(prepared.tx_hashes.as_slice(), prepared.raw_txs.as_slice())?;
    let mut unique = HashSet::with_capacity(prepared.tx_hashes.len());
    if prepared.tx_hashes.iter().any(|hash| !unique.insert(*hash)) {
        bail!("prepared NOV native block contains a duplicate transaction hash");
    }
    let context_commitment = prepared
        .context
        .commitment()
        .context("recompute prepared NOV native execution context commitment")?;
    let ordered_tx_root = nov_native_ordered_tx_root_v1(prepared.tx_hashes.as_slice())?;
    let body_digest = body_digest_v1(prepared.tx_hashes.as_slice(), prepared.raw_txs.as_slice());
    let body_bytes = prepared.raw_txs.iter().try_fold(0u64, |total, raw| {
        total
            .checked_add(raw.len() as u64)
            .context("prepared NOV native block body byte count overflow")
    })?;
    let candidate_id = candidate_id_v1(
        &context_commitment,
        &prepared.pre_state_root,
        prepared.aoem_parent.as_ref(),
        &ordered_tx_root,
        &body_digest,
        body_bytes,
    );
    if prepared.context_commitment != context_commitment
        || prepared.ordered_tx_root != ordered_tx_root
        || prepared.body_digest != body_digest
        || prepared.body_bytes != body_bytes
        || prepared.candidate_id != candidate_id
    {
        bail!("prepared NOV native block commitment binding mismatch");
    }
    if let Some(batch_id) = prepared.expected_aoem_batch_id.as_deref() {
        validate_external_id_v1("expected AOEM batch id", batch_id)?;
    }
    match (
        prepared.expected_aoem_batch_id.as_deref(),
        prepared.expected_aoem_output_commitment.as_deref(),
    ) {
        (Some(_), Some(commitment)) => {
            validate_hex_commitment_v1("expected AOEM output commitment", commitment)?;
        }
        (None, None) => {}
        _ => bail!("prepared NOV native AOEM batch/output binding is incomplete"),
    }
    if let Some(parent) = prepared.aoem_parent.as_ref() {
        validate_external_id_v1("parent AOEM batch id", parent.batch_id.as_str())?;
        validate_hex_commitment_v1(
            "parent AOEM batch result id",
            parent.batch_result_id.as_str(),
        )?;
        validate_external_id_v1(
            "parent AOEM state root codec",
            parent.state_root_codec.as_str(),
        )?;
        validate_external_id_v1(
            "parent AOEM receipt root codec",
            parent.receipt_root_codec.as_str(),
        )?;
        if parent.state_root == [0u8; 32]
            || parent.cumulative_receipt_root == [0u8; 32]
            || parent.state_version == 0
            || parent.state_root_codec != POST_STATE_ROOT_CODEC_V1
            || parent.receipt_root_codec != CUMULATIVE_RECEIPT_ROOT_CODEC_V1
        {
            bail!("prepared NOV native AOEM parent commitment is invalid");
        }
        if parent.state_root != prepared.pre_state_root {
            bail!("prepared NOV native pre-state root must equal its AOEM parent state root");
        }
    }
    Ok(())
}

fn validate_durable_block_v1(block: &NovNativeDurableBlockV1) -> Result<()> {
    if block.header.schema != HEADER_SCHEMA_V1
        || block.body.schema != BODY_SCHEMA_V1
        || block.execution_evidence.schema != EVIDENCE_SCHEMA_V1
    {
        bail!("unsupported NOV native durable block component schema");
    }
    if !block.header.canonical_local
        || block.header.safe
        || block.header.finalized
        || block.header.proof_sealed
        || block.execution_evidence.proof_sealed
    {
        bail!("NOV native durable block lifecycle flags violate unsealed v1 policy");
    }
    if block.execution_evidence.evidence_kind != "aoem_execution_commitment_not_consensus_seal" {
        bail!("NOV native durable block execution evidence kind is invalid");
    }
    if block.header.height == 1 && block.header.parent_block_hash != [0u8; 32] {
        bail!("NOV native genesis candidate must use the zero block parent hash");
    }
    let prepared = NovNativePreparedBlockV1 {
        schema: PREPARED_SCHEMA_V1.to_string(),
        candidate_id: block.header.candidate_id,
        context: NovBlockExecutionContextV1 {
            chain_id: block.header.chain_id,
            block_height: block.header.height,
            parent_block_hash: block.header.parent_block_hash,
            slot: block.header.slot,
            timestamp_unix_ms: block.header.timestamp_unix_ms,
        },
        context_commitment: block.header.execution_context_commitment,
        pre_state_root: block.header.pre_state_root,
        ordered_tx_root: block.body.ordered_tx_root,
        body_digest: block.body.body_digest,
        body_bytes: block.body.body_bytes,
        tx_hashes: block.body.tx_hashes.clone(),
        raw_txs: block.body.raw_txs.clone(),
        aoem_parent: block.header.aoem_parent.clone(),
        expected_aoem_batch_id: Some(block.header.aoem_batch_id.clone()),
        expected_aoem_output_commitment: Some(block.header.aoem_expected_output_commitment.clone()),
    };
    validate_prepared_block_v1(&prepared)?;
    if block.header.candidate_kind != CANDIDATE_KIND_V1
        || block.header.post_state_root_codec != POST_STATE_ROOT_CODEC_V1
        || block.header.cumulative_receipt_root_codec != CUMULATIVE_RECEIPT_ROOT_CODEC_V1
        || block.header.execution_context
            != (NovBlockExecutionContextV1 {
                chain_id: block.header.chain_id,
                block_height: block.header.height,
                parent_block_hash: block.header.parent_block_hash,
                slot: block.header.slot,
                timestamp_unix_ms: block.header.timestamp_unix_ms,
            })
        || !block.header.aoem_readback_verified
        || block.header.chain_id != block.body.chain_id
        || block.header.chain_id != block.execution_evidence.chain_id
        || block.header.height != block.body.height
        || block.header.height != block.execution_evidence.height
        || block.header.block_hash != block.body.block_hash
        || block.header.block_hash != block.execution_evidence.block_hash
        || block.header.ordered_tx_root != block.body.ordered_tx_root
        || block.header.body_digest != block.body.body_digest
        || block.header.body_bytes != block.body.body_bytes
        || block.header.tx_count as usize != block.body.tx_hashes.len()
        || block.header.receipt_count != block.header.tx_count
        || block.header.post_state_root != block.execution_evidence.post_state_root
        || block.header.cumulative_receipt_root != block.execution_evidence.cumulative_receipt_root
        || block.header.block_receipt_root != block.execution_evidence.block_receipt_root
        || block.header.state_version != block.execution_evidence.state_version
        || block.header.aoem_batch_id != block.execution_evidence.aoem_batch_id
        || block.header.aoem_batch_result_id != block.execution_evidence.aoem_batch_result_id
        || block.header.aoem_expected_output_commitment
            != block.execution_evidence.aoem_expected_output_commitment
        || block.header.aoem_evidence_commitment
            != block.execution_evidence.aoem_evidence_commitment
    {
        bail!("NOV native durable block component binding mismatch");
    }
    let receipt_root = nov_native_block_receipt_root_v1(
        block.body.tx_hashes.as_slice(),
        block
            .execution_evidence
            .per_block_receipt_commitments
            .as_slice(),
    )?;
    if receipt_root != block.header.block_receipt_root {
        bail!("NOV native durable block receipt root mismatch");
    }
    let commit_input = NovNativeBlockCommitInputV1 {
        post_state_root: block.header.post_state_root,
        cumulative_receipt_root: block.header.cumulative_receipt_root,
        per_block_receipt_commitments: block
            .execution_evidence
            .per_block_receipt_commitments
            .clone(),
        aoem_batch_id: block.header.aoem_batch_id.clone(),
        aoem_batch_result_id: block.header.aoem_batch_result_id.clone(),
        aoem_evidence_commitment: block.header.aoem_evidence_commitment,
        state_version: block.header.state_version,
    };
    validate_commit_input_semantics_v1(&prepared, &commit_input)?;
    let block_hash = block_hash_v1(&prepared, &commit_input, &receipt_root);
    if block_hash != block.header.block_hash {
        bail!("NOV native durable block hash mismatch");
    }
    Ok(())
}

fn durable_block_matches_prepared_v1(
    block: &NovNativeDurableBlockV1,
    prepared: &NovNativePreparedBlockV1,
) -> bool {
    block.header.candidate_id == prepared.candidate_id
        && block.header.execution_context_commitment == prepared.context_commitment
        && block.header.pre_state_root == prepared.pre_state_root
        && block.body.tx_hashes == prepared.tx_hashes
        && block.body.raw_txs == prepared.raw_txs
        && block.body.ordered_tx_root == prepared.ordered_tx_root
        && block.body.body_digest == prepared.body_digest
        && block.body.body_bytes == prepared.body_bytes
}

fn prepared_core_matches_v1(
    left: &NovNativePreparedBlockV1,
    right: &NovNativePreparedBlockV1,
) -> bool {
    left.schema == right.schema
        && left.candidate_id == right.candidate_id
        && left.context == right.context
        && left.context_commitment == right.context_commitment
        && left.pre_state_root == right.pre_state_root
        && left.ordered_tx_root == right.ordered_tx_root
        && left.body_digest == right.body_digest
        && left.body_bytes == right.body_bytes
        && left.tx_hashes == right.tx_hashes
        && left.raw_txs == right.raw_txs
        && left.aoem_parent == right.aoem_parent
}

fn validate_head_v1(head: &NovNativeBlockLedgerHeadV1, chain_id: u64) -> Result<()> {
    if head.schema != HEAD_SCHEMA_V1
        || head.chain_id != chain_id
        || head.height == 0
        || head.block_count != head.height
        || !head.canonical_local
        || head.safe
        || head.finalized
        || head.proof_sealed
    {
        bail!("NOV native block ledger head is invalid or falsely sealed");
    }
    Ok(())
}

fn validate_block_continuity_v1(
    parent: &NovNativeDurableBlockV1,
    child: &NovNativeDurableBlockV1,
) -> Result<()> {
    let expected_child_height = parent
        .header
        .height
        .checked_add(1)
        .context("NOV native durable block parent height overflow")?;
    let expected_aoem_parent = NovNativePreparedAoemParentV1 {
        batch_id: parent.header.aoem_batch_id.clone(),
        batch_result_id: parent.header.aoem_batch_result_id.clone(),
        state_root: parent.header.post_state_root,
        state_root_codec: parent.header.post_state_root_codec.clone(),
        cumulative_receipt_root: parent.header.cumulative_receipt_root,
        receipt_root_codec: parent.header.cumulative_receipt_root_codec.clone(),
        state_version: parent.header.state_version,
    };
    if child.header.chain_id != parent.header.chain_id
        || child.header.height != expected_child_height
        || child.header.parent_block_hash != parent.header.block_hash
        || child.header.pre_state_root != parent.header.post_state_root
        || child.header.aoem_parent.as_ref() != Some(&expected_aoem_parent)
        || child.header.slot <= parent.header.slot
        || child.header.timestamp_unix_ms < parent.header.timestamp_unix_ms
        || child.header.state_version <= parent.header.state_version
    {
        bail!("NOV native durable block parent/state continuity mismatch");
    }
    Ok(())
}

fn validate_tx_location_v1(
    location: &NovNativeBlockTxLocationV1,
    chain_id: u64,
    tx_hash: [u8; 32],
) -> Result<()> {
    if location.schema != TX_LOCATION_SCHEMA_V1
        || location.chain_id != chain_id
        || location.tx_hash != tx_hash
        || location.height == 0
        || !location.canonical_local
    {
        bail!("NOV native transaction location binding is invalid");
    }
    Ok(())
}

fn validate_receipt_location_v1(
    location: &NovNativeBlockReceiptLocationV1,
    chain_id: u64,
    tx_hash: [u8; 32],
) -> Result<()> {
    if location.schema != RECEIPT_LOCATION_SCHEMA_V1
        || location.chain_id != chain_id
        || location.tx_hash != tx_hash
        || location.height == 0
        || !location.canonical_local
        || location.proof_sealed
    {
        bail!("NOV native receipt location binding is invalid");
    }
    Ok(())
}

fn validate_body_limits_v1(tx_hashes: &[[u8; 32]], raw_txs: &[Vec<u8>]) -> Result<()> {
    if tx_hashes.is_empty() || tx_hashes.len() != raw_txs.len() {
        bail!("NOV native block tx hashes and raw transactions must be non-empty and aligned");
    }
    if tx_hashes.len() > NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1 {
        bail!(
            "NOV native block transaction count exceeds {}",
            NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1
        );
    }
    if raw_txs.iter().any(Vec::is_empty) {
        bail!("NOV native block raw transaction must not be empty");
    }
    let body_bytes = raw_txs.iter().try_fold(0usize, |total, raw| {
        total
            .checked_add(raw.len())
            .context("NOV native block body byte count overflow")
    })?;
    if body_bytes > NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1 {
        bail!(
            "NOV native block body exceeds {} bytes",
            NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1
        );
    }
    Ok(())
}

fn validate_external_id_v1(label: &str, id: &str) -> Result<()> {
    if id.is_empty() || id.trim() != id || id.len() > 512 || !id.is_ascii() {
        bail!("{label} must be non-empty canonical ASCII and at most 512 bytes");
    }
    Ok(())
}

fn validate_hex_commitment_v1(label: &str, value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("{label} must be a canonical lowercase 32-byte hex commitment");
    }
    Ok(())
}

fn body_digest_v1(tx_hashes: &[[u8; 32]], raw_txs: &[Vec<u8>]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(BODY_DIGEST_DOMAIN_V1);
    hasher.update((tx_hashes.len() as u64).to_be_bytes());
    for (tx_hash, raw_tx) in tx_hashes.iter().zip(raw_txs) {
        hasher.update(tx_hash);
        hasher.update((raw_tx.len() as u64).to_be_bytes());
        hasher.update(raw_tx);
    }
    hasher.finalize().into()
}

fn candidate_id_v1(
    context_commitment: &[u8; 32],
    pre_state_root: &[u8; 32],
    aoem_parent: Option<&NovNativePreparedAoemParentV1>,
    ordered_tx_root: &[u8; 32],
    body_digest: &[u8; 32],
    body_bytes: u64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(CANDIDATE_ID_DOMAIN_V1);
    hasher.update(context_commitment);
    hasher.update(pre_state_root);
    match aoem_parent {
        Some(parent) => {
            hasher.update([1u8]);
            update_len_prefixed_v1(&mut hasher, parent.batch_id.as_bytes());
            update_len_prefixed_v1(&mut hasher, parent.batch_result_id.as_bytes());
            hasher.update(parent.state_root);
            update_len_prefixed_v1(&mut hasher, parent.state_root_codec.as_bytes());
            hasher.update(parent.cumulative_receipt_root);
            update_len_prefixed_v1(&mut hasher, parent.receipt_root_codec.as_bytes());
            hasher.update(parent.state_version.to_be_bytes());
        }
        None => hasher.update([0u8]),
    }
    hasher.update(ordered_tx_root);
    hasher.update(body_digest);
    hasher.update(body_bytes.to_be_bytes());
    hasher.finalize().into()
}

fn block_hash_v1(
    prepared: &NovNativePreparedBlockV1,
    input: &NovNativeBlockCommitInputV1,
    block_receipt_root: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(BLOCK_HASH_DOMAIN_V1);
    update_len_prefixed_v1(&mut hasher, CANDIDATE_KIND_V1.as_bytes());
    hasher.update(prepared.context_commitment);
    hasher.update(prepared.candidate_id);
    hasher.update(prepared.pre_state_root);
    hasher.update(input.post_state_root);
    update_len_prefixed_v1(&mut hasher, POST_STATE_ROOT_CODEC_V1.as_bytes());
    hasher.update(prepared.ordered_tx_root);
    hasher.update(block_receipt_root);
    hasher.update(input.cumulative_receipt_root);
    update_len_prefixed_v1(&mut hasher, CUMULATIVE_RECEIPT_ROOT_CODEC_V1.as_bytes());
    hasher.update(prepared.body_digest);
    hasher.update(prepared.body_bytes.to_be_bytes());
    hasher.update((prepared.tx_hashes.len() as u64).to_be_bytes());
    hasher.update([1u8]);
    hasher.update(input.state_version.to_be_bytes());
    update_len_prefixed_v1(&mut hasher, input.aoem_batch_id.as_bytes());
    update_len_prefixed_v1(&mut hasher, input.aoem_batch_result_id.as_bytes());
    update_len_prefixed_v1(
        &mut hasher,
        prepared
            .expected_aoem_output_commitment
            .as_deref()
            .unwrap_or_default()
            .as_bytes(),
    );
    hasher.update(input.aoem_evidence_commitment);
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
        .context("write synchronized NOV native block ledger batch")
}

fn read_json_v1<T: DeserializeOwned>(db: &DB, key: &[u8], label: &str) -> Result<Option<T>> {
    db.get(key)
        .with_context(|| format!("read NOV native {label} failed"))?
        .map(|raw| {
            serde_json::from_slice(raw.as_slice())
                .with_context(|| format!("decode NOV native {label} failed"))
        })
        .transpose()
}

fn put_json_v1<T: Serialize>(
    batch: &mut RocksDbWriteBatch,
    key: &[u8],
    value: &T,
    label: &str,
) -> Result<()> {
    let encoded =
        serde_json::to_vec(value).with_context(|| format!("encode NOV native {label} failed"))?;
    batch.put(key, encoded);
    Ok(())
}

fn chain_prefix_v1(chain_id: u64) -> String {
    format!("{KEY_PREFIX_V1}chain/{chain_id:020}/")
}

fn candidate_current_key_v1(chain_id: u64) -> String {
    format!("{}candidate/current", chain_prefix_v1(chain_id))
}

fn candidate_key_v1(chain_id: u64, candidate_id: &[u8; 32]) -> String {
    format!(
        "{}candidate/{}",
        chain_prefix_v1(chain_id),
        hex_v1(candidate_id)
    )
}

fn candidate_graph_prefix_v1(chain_id: u64) -> String {
    format!("{}candidate_graph/v1/", chain_prefix_v1(chain_id))
}

fn candidate_record_key_v1(chain_id: u64, block_hash: &[u8; 32]) -> String {
    format!(
        "{}record/{}",
        candidate_graph_prefix_v1(chain_id),
        hex_v1(block_hash)
    )
}

fn candidate_artifact_key_v1(chain_id: u64, block_hash: &[u8; 32]) -> String {
    format!(
        "{}artifact/{}",
        candidate_graph_prefix_v1(chain_id),
        hex_v1(block_hash)
    )
}

fn candidate_height_index_key_v1(chain_id: u64, height: u64) -> String {
    format!("{}height/{height:020}", candidate_graph_prefix_v1(chain_id))
}

fn candidate_children_index_key_v1(chain_id: u64, parent_block_hash: &[u8; 32]) -> String {
    format!(
        "{}children/{}",
        candidate_graph_prefix_v1(chain_id),
        hex_v1(parent_block_hash)
    )
}

fn head_key_v1(chain_id: u64) -> String {
    format!("{}execution_head", chain_prefix_v1(chain_id))
}

fn height_key_v1(chain_id: u64, height: u64) -> String {
    format!("{}height/{height:020}", chain_prefix_v1(chain_id))
}

fn header_key_v1(chain_id: u64, block_hash: &[u8; 32]) -> String {
    format!("{}header/{}", chain_prefix_v1(chain_id), hex_v1(block_hash))
}

fn body_key_v1(chain_id: u64, block_hash: &[u8; 32]) -> String {
    format!("{}body/{}", chain_prefix_v1(chain_id), hex_v1(block_hash))
}

fn evidence_key_v1(chain_id: u64, block_hash: &[u8; 32]) -> String {
    format!(
        "{}evidence/{}",
        chain_prefix_v1(chain_id),
        hex_v1(block_hash)
    )
}

fn tx_key_v1(chain_id: u64, tx_hash: &[u8; 32]) -> String {
    format!("{}tx/{}", chain_prefix_v1(chain_id), hex_v1(tx_hash))
}

fn receipt_key_v1(chain_id: u64, tx_hash: &[u8; 32]) -> String {
    format!("{}receipt/{}", chain_prefix_v1(chain_id), hex_v1(tx_hash))
}

fn external_id_key_v1(chain_id: u64, kind: &str, id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(EXTERNAL_ID_KEY_DOMAIN_V1);
    update_len_prefixed_v1(&mut hasher, kind.as_bytes());
    update_len_prefixed_v1(&mut hasher, id.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    format!("{}{kind}/{}", chain_prefix_v1(chain_id), hex_v1(&digest))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);

    struct TestLedgerV1 {
        path: PathBuf,
        ledger: Option<NovNativeBlockLedgerV1>,
    }

    impl TestLedgerV1 {
        fn new(label: &str) -> Self {
            let serial = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "novovm-native-block-ledger-{label}-{}-{serial}-{nanos}",
                std::process::id()
            ));
            let ledger = NovNativeBlockLedgerV1::open(path.as_path()).expect("open test ledger");
            Self {
                path,
                ledger: Some(ledger),
            }
        }

        fn ledger(&self) -> &NovNativeBlockLedgerV1 {
            self.ledger.as_ref().expect("test ledger is open")
        }

        fn reopen(&mut self) {
            self.ledger.take();
            self.ledger = Some(
                NovNativeBlockLedgerV1::open(self.path.as_path()).expect("reopen test ledger"),
            );
        }
    }

    impl Drop for TestLedgerV1 {
        fn drop(&mut self) {
            self.ledger.take();
            let _ = fs::remove_dir_all(self.path.as_path());
        }
    }

    fn context_v1(chain_id: u64, height: u64, parent: [u8; 32]) -> NovBlockExecutionContextV1 {
        NovBlockExecutionContextV1 {
            chain_id,
            block_height: height,
            parent_block_hash: parent,
            slot: height.saturating_mul(2),
            timestamp_unix_ms: 1_900_000_000_000u64.saturating_add(height.saturating_mul(2_000)),
        }
    }

    fn candidate_input_v1(
        context: NovBlockExecutionContextV1,
        pre_state_root: [u8; 32],
        tx_seed: u8,
        count: usize,
    ) -> NovNativeBlockCandidateInputV1 {
        let tx_hashes = (0..count)
            .map(|index| [tx_seed.wrapping_add(index as u8); 32])
            .collect::<Vec<_>>();
        let raw_txs = (0..count)
            .map(|index| vec![tx_seed, index as u8, 0x7f])
            .collect::<Vec<_>>();
        NovNativeBlockCandidateInputV1 {
            context,
            tx_hashes,
            raw_txs,
            pre_state_root,
            aoem_parent: None,
        }
    }

    fn with_aoem_parent_v1(
        mut input: NovNativeBlockCandidateInputV1,
        parent: &NovNativeDurableBlockV1,
    ) -> NovNativeBlockCandidateInputV1 {
        input.aoem_parent = Some(NovNativePreparedAoemParentV1 {
            batch_id: parent.header.aoem_batch_id.clone(),
            batch_result_id: parent.header.aoem_batch_result_id.clone(),
            state_root: parent.header.post_state_root,
            state_root_codec: parent.header.post_state_root_codec.clone(),
            cumulative_receipt_root: parent.header.cumulative_receipt_root,
            receipt_root_codec: parent.header.cumulative_receipt_root_codec.clone(),
            state_version: parent.header.state_version,
        });
        input
    }

    fn commit_input_v1(seed: u8, count: usize, state_version: u64) -> NovNativeBlockCommitInputV1 {
        let mut batch_result_id = [seed; 32];
        batch_result_id[24..].copy_from_slice(&state_version.to_be_bytes());
        NovNativeBlockCommitInputV1 {
            post_state_root: [seed.wrapping_add(1); 32],
            cumulative_receipt_root: [seed.wrapping_add(2); 32],
            per_block_receipt_commitments: (0..count)
                .map(|index| [seed.wrapping_add(10).wrapping_add(index as u8); 32])
                .collect(),
            aoem_batch_id: format!("batch-{seed:02x}-{state_version}"),
            aoem_batch_result_id: hex_v1(&batch_result_id),
            aoem_evidence_commitment: [seed.wrapping_add(3); 32],
            state_version,
        }
    }

    fn commit_bound_v1(
        ledger: &NovNativeBlockLedgerV1,
        prepared: &NovNativePreparedBlockV1,
        input: NovNativeBlockCommitInputV1,
    ) -> Result<NovNativeDurableBlockV1> {
        let expected_output_commitment = format!("{:064x}", input.state_version);
        let bound = ledger.bind_expected_aoem_batch_id(
            prepared,
            input.aoem_batch_id.as_str(),
            expected_output_commitment.as_str(),
        )?;
        ledger.commit(&bound, input)
    }

    fn build_observed_block_v1(
        input: NovNativeBlockCandidateInputV1,
        commit: NovNativeBlockCommitInputV1,
    ) -> NovNativeDurableBlockV1 {
        let mut prepared = build_prepared_block_v1(input).expect("build observed prepared block");
        prepared.expected_aoem_batch_id = Some(commit.aoem_batch_id.clone());
        prepared.expected_aoem_output_commitment = Some(format!("{:064x}", commit.state_version));
        build_durable_block_v1(&prepared, commit).expect("build observed durable block")
    }

    #[test]
    fn two_blocks_are_durable_indexed_and_explicitly_unsealed() {
        let test = TestLedgerV1::new("two-blocks");
        let chain_id = 71_001;
        let first_pre = [0x11; 32];
        let first = test
            .ledger()
            .prepare(candidate_input_v1(
                context_v1(chain_id, 1, [0u8; 32]),
                first_pre,
                0x21,
                2,
            ))
            .expect("prepare first block");
        let first_block = commit_bound_v1(test.ledger(), &first, commit_input_v1(0x31, 2, 2))
            .expect("commit first block");

        let second = test
            .ledger()
            .prepare(with_aoem_parent_v1(
                candidate_input_v1(
                    context_v1(chain_id, 2, first_block.header.block_hash),
                    first_block.header.post_state_root,
                    0x41,
                    1,
                ),
                &first_block,
            ))
            .expect("prepare second block");
        let second_block = commit_bound_v1(test.ledger(), &second, commit_input_v1(0x51, 1, 3))
            .expect("commit second block");

        let head = test
            .ledger()
            .load_head(chain_id)
            .expect("load head")
            .expect("head exists");
        assert_eq!(head.height, 2);
        assert_eq!(head.block_hash, second_block.header.block_hash);
        assert_eq!(head.block_count, 2);
        assert_eq!(head.cumulative_tx_count, 3);
        assert!(head.canonical_local);
        assert!(!head.safe && !head.finalized && !head.proof_sealed);

        let by_height = test
            .ledger()
            .load_by_height(chain_id, 1)
            .expect("load first by height")
            .expect("first exists");
        let by_hash = test
            .ledger()
            .load_by_hash(chain_id, first_block.header.block_hash)
            .expect("load first by hash")
            .expect("first exists");
        assert_eq!(by_height, first_block);
        assert_eq!(by_hash, first_block);
        assert!(by_hash.header.canonical_local);
        assert!(!by_hash.header.safe);
        assert!(!by_hash.header.finalized);
        assert!(!by_hash.header.proof_sealed);
        assert!(!by_hash.execution_evidence.proof_sealed);

        let tx_hash = first_block.body.tx_hashes[1];
        let location = test
            .ledger()
            .load_tx_location(chain_id, tx_hash)
            .expect("load tx location")
            .expect("tx indexed");
        assert_eq!(location.height, 1);
        assert_eq!(location.tx_index, 1);
        assert_eq!(location.block_hash, first_block.header.block_hash);
        let receipt = test
            .ledger()
            .load_receipt_location(chain_id, tx_hash)
            .expect("load receipt")
            .expect("receipt indexed");
        assert_eq!(receipt.receipt_commitment, [0x3c; 32]);
        assert!(!receipt.proof_sealed);

        let status = test.ledger().status(chain_id).expect("ledger status");
        assert!(status.prepared.is_none());
        assert!(status.canonical_local);
        assert!(!status.safe && !status.finalized && !status.proof_sealed);
        assert_eq!(status.max_txs_per_block, 1_024);
        assert_eq!(status.max_body_bytes, 2 * 1024 * 1024);
    }

    #[test]
    fn synchronized_prepare_and_commit_survive_reopen() {
        let mut test = TestLedgerV1::new("reopen");
        let chain_id = 71_002;
        let prepared = test
            .ledger()
            .prepare(candidate_input_v1(
                context_v1(chain_id, 1, [0u8; 32]),
                [0x12; 32],
                0x22,
                1,
            ))
            .expect("prepare block");
        test.reopen();
        assert_eq!(
            test.ledger()
                .load_prepared(chain_id)
                .expect("load prepared"),
            Some(prepared.clone())
        );

        let committed = commit_bound_v1(test.ledger(), &prepared, commit_input_v1(0x32, 1, 1))
            .expect("commit after reopen");
        test.reopen();
        assert!(test
            .ledger()
            .load_prepared(chain_id)
            .expect("load cleared prepared")
            .is_none());
        assert_eq!(
            test.ledger()
                .load_by_hash(chain_id, committed.header.block_hash)
                .expect("load committed after reopen"),
            Some(committed)
        );
    }

    #[test]
    fn prepare_and_commit_are_idempotent_but_conflicts_fail_closed() {
        let test = TestLedgerV1::new("idempotency");
        let chain_id = 71_003;
        let input = candidate_input_v1(context_v1(chain_id, 1, [0u8; 32]), [0x13; 32], 0x23, 1);
        let prepared = test.ledger().prepare(input.clone()).expect("prepare");
        assert_eq!(
            test.ledger().prepare(input).expect("repeat prepare"),
            prepared
        );

        let conflicting =
            candidate_input_v1(context_v1(chain_id, 1, [0u8; 32]), [0x13; 32], 0x24, 1);
        assert!(test
            .ledger()
            .prepare(conflicting)
            .expect_err("different unresolved candidate must fail")
            .to_string()
            .contains("different unresolved candidate"));

        let commit_input = commit_input_v1(0x33, 1, 1);
        let committed =
            commit_bound_v1(test.ledger(), &prepared, commit_input.clone()).expect("commit");
        assert_eq!(
            commit_bound_v1(test.ledger(), &prepared, commit_input).expect("repeat commit"),
            committed
        );

        let mut changed = commit_input_v1(0x33, 1, 1);
        changed.aoem_evidence_commitment = [0xee; 32];
        assert!(commit_bound_v1(test.ledger(), &prepared, changed)
            .expect_err("conflicting repeat commit must fail")
            .to_string()
            .contains("conflicts with durable height"));

        let replay_prepared = test
            .ledger()
            .prepare(candidate_input_v1(
                context_v1(chain_id, 1, [0u8; 32]),
                [0x13; 32],
                0x23,
                1,
            ))
            .expect("prepare exact already committed block");
        assert_eq!(replay_prepared, prepared);
    }

    #[test]
    fn continuity_and_transaction_reuse_conflicts_are_rejected() {
        let test = TestLedgerV1::new("continuity");
        let chain_id = 71_004;
        let first = test
            .ledger()
            .prepare(candidate_input_v1(
                context_v1(chain_id, 1, [0u8; 32]),
                [0x14; 32],
                0x24,
                1,
            ))
            .expect("prepare first");
        let first_block = commit_bound_v1(test.ledger(), &first, commit_input_v1(0x34, 1, 5))
            .expect("commit first");

        for (label, input) in [
            (
                "height",
                with_aoem_parent_v1(
                    candidate_input_v1(
                        context_v1(chain_id, 3, first_block.header.block_hash),
                        first_block.header.post_state_root,
                        0x44,
                        1,
                    ),
                    &first_block,
                ),
            ),
            (
                "parent",
                with_aoem_parent_v1(
                    candidate_input_v1(
                        context_v1(chain_id, 2, [0x99; 32]),
                        first_block.header.post_state_root,
                        0x44,
                        1,
                    ),
                    &first_block,
                ),
            ),
            (
                "pre-state",
                with_aoem_parent_v1(
                    candidate_input_v1(
                        context_v1(chain_id, 2, first_block.header.block_hash),
                        [0x98; 32],
                        0x44,
                        1,
                    ),
                    &first_block,
                ),
            ),
        ] {
            assert!(
                test.ledger().prepare(input).is_err(),
                "{label} conflict must fail"
            );
        }

        let reused_tx = NovNativeBlockCandidateInputV1 {
            context: context_v1(chain_id, 2, first_block.header.block_hash),
            tx_hashes: first_block.body.tx_hashes.clone(),
            raw_txs: vec![vec![0x55, 0, 0x7f]],
            pre_state_root: first_block.header.post_state_root,
            aoem_parent: Some(NovNativePreparedAoemParentV1 {
                batch_id: first_block.header.aoem_batch_id.clone(),
                batch_result_id: first_block.header.aoem_batch_result_id.clone(),
                state_root: first_block.header.post_state_root,
                state_root_codec: first_block.header.post_state_root_codec.clone(),
                cumulative_receipt_root: first_block.header.cumulative_receipt_root,
                receipt_root_codec: first_block.header.cumulative_receipt_root_codec.clone(),
                state_version: first_block.header.state_version,
            }),
        };
        assert!(test
            .ledger()
            .prepare(reused_tx)
            .expect_err("transaction reuse must fail")
            .to_string()
            .contains("already indexed"));

        let second = test
            .ledger()
            .prepare(with_aoem_parent_v1(
                candidate_input_v1(
                    context_v1(chain_id, 2, first_block.header.block_hash),
                    first_block.header.post_state_root,
                    0x45,
                    1,
                ),
                &first_block,
            ))
            .expect("prepare valid second");
        assert!(
            commit_bound_v1(test.ledger(), &second, commit_input_v1(0x35, 1, 5))
                .expect_err("state version must advance")
                .to_string()
                .contains("state_version must advance")
        );
    }

    #[test]
    fn legacy_linear_candidates_are_lazily_verified_and_backfilled() {
        let mut test = TestLedgerV1::new("legacy-candidate-backfill");
        let chain_id = 71_013;
        let first = test
            .ledger()
            .prepare(candidate_input_v1(
                context_v1(chain_id, 1, [0u8; 32]),
                [0x23; 32],
                0x33,
                1,
            ))
            .expect("prepare first");
        let first_block = commit_bound_v1(test.ledger(), &first, commit_input_v1(0x43, 1, 1))
            .expect("commit first");
        let second = test
            .ledger()
            .prepare(with_aoem_parent_v1(
                candidate_input_v1(
                    context_v1(chain_id, 2, first_block.header.block_hash),
                    first_block.header.post_state_root,
                    0x34,
                    1,
                ),
                &first_block,
            ))
            .expect("prepare second");
        let second_commit = commit_input_v1(0x44, 1, 2);
        let second_block =
            commit_bound_v1(test.ledger(), &second, second_commit.clone()).expect("commit second");

        let mut remove_graph = RocksDbWriteBatch::default();
        remove_graph.delete(KEY_CANDIDATE_GRAPH_SCHEMA_V1);
        for block in [&first_block, &second_block] {
            remove_graph
                .delete(candidate_record_key_v1(chain_id, &block.header.block_hash).as_bytes());
            remove_graph
                .delete(candidate_height_index_key_v1(chain_id, block.header.height).as_bytes());
            remove_graph.delete(
                candidate_children_index_key_v1(chain_id, &block.header.parent_block_hash)
                    .as_bytes(),
            );
        }
        write_sync_v1(&test.ledger().db, remove_graph).expect("simulate pre-graph database");
        test.reopen();

        let synthesized = test
            .ledger()
            .load_candidate_record(chain_id, second_block.header.block_hash)
            .expect("load verified legacy candidate")
            .expect("legacy candidate exists");
        assert_eq!(synthesized.candidate_source, CANDIDATE_SOURCE_LOCAL_AOEM_V1);
        assert!(synthesized.parent_continuity_verified);
        assert!(test
            .ledger()
            .db
            .get(candidate_record_key_v1(chain_id, &second_block.header.block_hash).as_bytes())
            .expect("read absent synthesized record")
            .is_none());

        let competing = build_observed_block_v1(
            with_aoem_parent_v1(
                candidate_input_v1(
                    context_v1(chain_id, 2, first_block.header.block_hash),
                    first_block.header.post_state_root,
                    0x35,
                    1,
                ),
                &first_block,
            ),
            commit_input_v1(0x45, 1, 2),
        );
        test.ledger()
            .register_observed_unsealed_candidate(competing)
            .expect("first graph write on legacy database");
        assert_eq!(
            test.ledger()
                .load_candidate_records_by_height(chain_id, 2)
                .expect("merge legacy selected and graph candidates")
                .len(),
            2
        );

        commit_bound_v1(test.ledger(), &second, second_commit)
            .expect("repeat commit backfills selected graph record");
        assert!(test
            .ledger()
            .db
            .get(candidate_record_key_v1(chain_id, &second_block.header.block_hash).as_bytes())
            .expect("read backfilled selected record")
            .is_some());
        assert_eq!(
            test.ledger()
                .load_candidate_records_by_height(chain_id, 2)
                .expect("load persisted merged height")
                .len(),
            2
        );
    }

    #[test]
    fn legacy_candidate_synthesis_fails_closed_when_its_parent_is_missing() {
        let mut test = TestLedgerV1::new("legacy-candidate-missing-parent");
        let chain_id = 71_014;
        let first = test
            .ledger()
            .prepare(candidate_input_v1(
                context_v1(chain_id, 1, [0u8; 32]),
                [0x24; 32],
                0x36,
                1,
            ))
            .expect("prepare first");
        let first_block = commit_bound_v1(test.ledger(), &first, commit_input_v1(0x46, 1, 1))
            .expect("commit first");
        let second = test
            .ledger()
            .prepare(with_aoem_parent_v1(
                candidate_input_v1(
                    context_v1(chain_id, 2, first_block.header.block_hash),
                    first_block.header.post_state_root,
                    0x37,
                    1,
                ),
                &first_block,
            ))
            .expect("prepare second");
        let second_block = commit_bound_v1(test.ledger(), &second, commit_input_v1(0x47, 1, 2))
            .expect("commit second");

        let mut corrupt_legacy = RocksDbWriteBatch::default();
        corrupt_legacy.delete(KEY_CANDIDATE_GRAPH_SCHEMA_V1);
        for block in [&first_block, &second_block] {
            corrupt_legacy
                .delete(candidate_record_key_v1(chain_id, &block.header.block_hash).as_bytes());
            corrupt_legacy
                .delete(candidate_height_index_key_v1(chain_id, block.header.height).as_bytes());
            corrupt_legacy.delete(
                candidate_children_index_key_v1(chain_id, &block.header.parent_block_hash)
                    .as_bytes(),
            );
        }
        corrupt_legacy.delete(header_key_v1(chain_id, &first_block.header.block_hash).as_bytes());
        write_sync_v1(&test.ledger().db, corrupt_legacy)
            .expect("simulate corrupt pre-graph parent");
        test.reopen();
        assert!(
            test.ledger()
                .load_candidate_record(chain_id, second_block.header.block_hash)
                .is_err(),
            "legacy synthesis must fail closed when its parent artifact is missing"
        );
    }

    #[test]
    fn competing_candidates_are_isolated_from_the_local_execution_projection() {
        let test = TestLedgerV1::new("candidate-graph");
        let chain_id = 71_010;
        let first = test
            .ledger()
            .prepare(candidate_input_v1(
                context_v1(chain_id, 1, [0u8; 32]),
                [0x20; 32],
                0x30,
                1,
            ))
            .expect("prepare first");
        let first_block = commit_bound_v1(test.ledger(), &first, commit_input_v1(0x40, 1, 1))
            .expect("commit first");

        let selected = test
            .ledger()
            .prepare(with_aoem_parent_v1(
                candidate_input_v1(
                    context_v1(chain_id, 2, first_block.header.block_hash),
                    first_block.header.post_state_root,
                    0x50,
                    1,
                ),
                &first_block,
            ))
            .expect("prepare selected child");
        let selected_block = commit_bound_v1(test.ledger(), &selected, commit_input_v1(0x60, 1, 2))
            .expect("commit selected child");

        let competing_block = build_observed_block_v1(
            with_aoem_parent_v1(
                candidate_input_v1(
                    context_v1(chain_id, 2, first_block.header.block_hash),
                    first_block.header.post_state_root,
                    0x51,
                    1,
                ),
                &first_block,
            ),
            commit_input_v1(0x61, 1, 2),
        );
        let competing_record = test
            .ledger()
            .register_observed_unsealed_candidate(competing_block.clone())
            .expect("register competing candidate");
        assert_eq!(
            competing_record.candidate_source,
            CANDIDATE_SOURCE_OBSERVED_V1
        );
        assert_eq!(
            competing_record.lifecycle_status,
            CANDIDATE_STATUS_ACTIVE_V1
        );
        assert!(!competing_record.local_aoem_readback_verified);
        assert!(!competing_record.execution_selected_local);
        assert!(!competing_record.fork_choice_selected);
        assert!(!competing_record.chain_canonical);
        assert!(!competing_record.proof_sealed);
        assert!(!competing_record.safe);
        assert!(!competing_record.finalized);

        let candidates = test
            .ledger()
            .load_candidate_records_by_height(chain_id, 2)
            .expect("load competing height");
        assert_eq!(candidates.len(), 2);
        assert!(candidates
            .windows(2)
            .all(|pair| pair[0].block_hash < pair[1].block_hash));
        assert_eq!(
            candidates
                .iter()
                .filter(|record| record.execution_selected_local)
                .count(),
            1
        );
        let children = test
            .ledger()
            .load_candidate_children(chain_id, first_block.header.block_hash)
            .expect("load candidate children");
        assert_eq!(children, candidates);

        assert_eq!(
            test.ledger()
                .load_by_height(chain_id, 2)
                .expect("load selected height"),
            Some(selected_block.clone())
        );
        assert_eq!(
            test.ledger()
                .load_head(chain_id)
                .expect("load selected head")
                .expect("selected head exists")
                .block_hash,
            selected_block.header.block_hash
        );
        assert!(test
            .ledger()
            .load_tx_location(chain_id, competing_block.body.tx_hashes[0])
            .expect("load selected transaction projection")
            .is_none());
        assert!(test
            .ledger()
            .load_receipt_location(chain_id, competing_block.body.tx_hashes[0])
            .expect("load selected receipt projection")
            .is_none());
        for (kind, id) in [
            ("batch", competing_block.header.aoem_batch_id.as_str()),
            (
                "result",
                competing_block.header.aoem_batch_result_id.as_str(),
            ),
        ] {
            assert!(test
                .ledger()
                .db
                .get(external_id_key_v1(chain_id, kind, id).as_bytes())
                .expect("read legacy AOEM external-id projection")
                .is_none());
        }
        assert_eq!(
            test.ledger()
                .load_candidate_block(chain_id, competing_block.header.block_hash)
                .expect("load competing artifact"),
            Some(competing_block.clone())
        );
        assert_eq!(
            test.ledger()
                .register_observed_unsealed_candidate(competing_block)
                .expect("repeat candidate registration"),
            competing_record
        );
    }

    #[test]
    fn candidate_abort_is_recursive_auditable_and_restart_durable() {
        let mut test = TestLedgerV1::new("candidate-abort");
        let chain_id = 71_011;
        let first = test
            .ledger()
            .prepare(candidate_input_v1(
                context_v1(chain_id, 1, [0u8; 32]),
                [0x21; 32],
                0x31,
                1,
            ))
            .expect("prepare first");
        let first_block = commit_bound_v1(test.ledger(), &first, commit_input_v1(0x41, 1, 1))
            .expect("commit first");
        let competing_parent = build_observed_block_v1(
            with_aoem_parent_v1(
                candidate_input_v1(
                    context_v1(chain_id, 2, first_block.header.block_hash),
                    first_block.header.post_state_root,
                    0x52,
                    1,
                ),
                &first_block,
            ),
            commit_input_v1(0x62, 1, 2),
        );
        test.ledger()
            .register_observed_unsealed_candidate(competing_parent.clone())
            .expect("register competing parent");
        let competing_child = build_observed_block_v1(
            with_aoem_parent_v1(
                candidate_input_v1(
                    context_v1(chain_id, 3, competing_parent.header.block_hash),
                    competing_parent.header.post_state_root,
                    0x53,
                    1,
                ),
                &competing_parent,
            ),
            commit_input_v1(0x63, 1, 3),
        );
        test.ledger()
            .register_observed_unsealed_candidate(competing_child.clone())
            .expect("register competing child");

        let aborted = test
            .ledger()
            .abort_unselected_candidate_branch(
                chain_id,
                competing_parent.header.block_hash,
                "parent lost deterministic candidate preference",
            )
            .expect("abort competing branch");
        assert_eq!(aborted.len(), 2);
        assert!(aborted.iter().all(|record| {
            record.lifecycle_status == CANDIDATE_STATUS_ABORTED_V1
                && record.revision == 2
                && record.abort_reason.as_deref()
                    == Some("parent lost deterministic candidate preference")
                && !record.chain_canonical
                && !record.proof_sealed
                && !record.safe
                && !record.finalized
        }));

        test.reopen();
        for block in [&competing_parent, &competing_child] {
            let record = test
                .ledger()
                .load_candidate_record(chain_id, block.header.block_hash)
                .expect("load aborted candidate after reopen")
                .expect("aborted candidate exists");
            assert_eq!(record.lifecycle_status, CANDIDATE_STATUS_ABORTED_V1);
            assert_eq!(record.revision, 2);
            assert_eq!(
                test.ledger()
                    .load_candidate_block(chain_id, block.header.block_hash)
                    .expect("load retained aborted artifact"),
                Some(block.clone())
            );
        }
        let repeated = test
            .ledger()
            .abort_unselected_candidate_branch(
                chain_id,
                competing_parent.header.block_hash,
                "a later reason must not rewrite the audit record",
            )
            .expect("repeat abort is idempotent");
        assert!(repeated.iter().all(|record| {
            record.revision == 2
                && record.abort_reason.as_deref()
                    == Some("parent lost deterministic candidate preference")
        }));
        let late_child = build_observed_block_v1(
            with_aoem_parent_v1(
                candidate_input_v1(
                    context_v1(chain_id, 3, competing_parent.header.block_hash),
                    competing_parent.header.post_state_root,
                    0x56,
                    1,
                ),
                &competing_parent,
            ),
            commit_input_v1(0x66, 1, 3),
        );
        assert!(test
            .ledger()
            .register_observed_unsealed_candidate(late_child)
            .expect_err("aborted parent cannot accept a new child")
            .to_string()
            .contains("aborted parent"));
        assert!(test
            .ledger()
            .abort_unselected_candidate_branch(
                chain_id,
                first_block.header.block_hash,
                "selected execution branch must reject ordinary abort",
            )
            .expect_err("selected local candidate cannot be aborted")
            .to_string()
            .contains("cannot use unselected abort"));
        assert_eq!(
            test.ledger()
                .load_head(chain_id)
                .expect("load unchanged AOEM execution head")
                .expect("head exists")
                .block_hash,
            first_block.header.block_hash
        );
    }

    #[test]
    fn invalid_or_orphaned_candidates_leave_no_graph_state() {
        let test = TestLedgerV1::new("candidate-invalid");
        let chain_id = 71_012;
        let first = test
            .ledger()
            .prepare(candidate_input_v1(
                context_v1(chain_id, 1, [0u8; 32]),
                [0x22; 32],
                0x32,
                1,
            ))
            .expect("prepare first");
        let first_block = commit_bound_v1(test.ledger(), &first, commit_input_v1(0x42, 1, 1))
            .expect("commit first");

        let competing_genesis = build_observed_block_v1(
            candidate_input_v1(context_v1(chain_id, 1, [0u8; 32]), [0x25; 32], 0x57, 1),
            commit_input_v1(0x67, 1, 1),
        );
        assert!(test
            .ledger()
            .register_observed_unsealed_candidate(competing_genesis)
            .expect_err("competing genesis must fail")
            .to_string()
            .contains("competing genesis"));
        assert_eq!(
            test.ledger()
                .load_candidate_records_by_height(chain_id, 1)
                .expect("load unique genesis")
                .len(),
            1
        );

        let orphan = build_observed_block_v1(
            with_aoem_parent_v1(
                candidate_input_v1(
                    context_v1(chain_id, 2, [0x99; 32]),
                    first_block.header.post_state_root,
                    0x54,
                    1,
                ),
                &first_block,
            ),
            commit_input_v1(0x64, 1, 2),
        );
        assert!(test
            .ledger()
            .register_observed_unsealed_candidate(orphan)
            .expect_err("orphan candidate must fail")
            .to_string()
            .contains("parent is missing"));
        assert!(test
            .ledger()
            .load_candidate_records_by_height(chain_id, 2)
            .expect("load empty candidate height")
            .is_empty());

        let mut tampered = build_observed_block_v1(
            with_aoem_parent_v1(
                candidate_input_v1(
                    context_v1(chain_id, 2, first_block.header.block_hash),
                    first_block.header.post_state_root,
                    0x55,
                    1,
                ),
                &first_block,
            ),
            commit_input_v1(0x65, 1, 2),
        );
        tampered.body.raw_txs[0][0] ^= 1;
        assert!(test
            .ledger()
            .register_observed_unsealed_candidate(tampered)
            .expect_err("tampered candidate must fail before persistence")
            .to_string()
            .contains("commitment binding mismatch"));
        assert!(test
            .ledger()
            .load_candidate_records_by_height(chain_id, 2)
            .expect("load still-empty candidate height")
            .is_empty());
        assert_eq!(
            test.ledger()
                .load_head(chain_id)
                .expect("load unchanged head")
                .expect("head exists")
                .block_hash,
            first_block.header.block_hash
        );
    }

    #[test]
    fn roots_and_hash_bind_body_receipts_and_aoem_evidence() {
        let test = TestLedgerV1::new("root-binding");
        let chain_id = 71_005;
        let mut prepared = test
            .ledger()
            .prepare(candidate_input_v1(
                context_v1(chain_id, 1, [0u8; 32]),
                [0x15; 32],
                0x25,
                2,
            ))
            .expect("prepare");
        let base_input = commit_input_v1(0x35, 2, 2);
        prepared.expected_aoem_batch_id = Some(base_input.aoem_batch_id.clone());
        prepared.expected_aoem_output_commitment = Some(format!("{:064x}", 2));
        let base = build_durable_block_v1(&prepared, base_input.clone()).expect("build block");

        let mut invalid_result_id = base_input.clone();
        invalid_result_id.aoem_batch_result_id = "result-not-a-commitment".to_string();
        assert!(build_durable_block_v1(&prepared, invalid_result_id)
            .expect_err("AOEM batch result id must retain its canonical commitment form")
            .to_string()
            .contains("canonical lowercase 32-byte hex commitment"));

        let mut receipt_changed = base_input.clone();
        receipt_changed.per_block_receipt_commitments[1][0] ^= 1;
        let receipt_variant =
            build_durable_block_v1(&prepared, receipt_changed).expect("receipt variant");
        assert_ne!(
            base.header.block_receipt_root,
            receipt_variant.header.block_receipt_root
        );
        assert_ne!(base.header.block_hash, receipt_variant.header.block_hash);

        let mut evidence_changed = base_input.clone();
        evidence_changed.aoem_evidence_commitment[0] ^= 1;
        let evidence_variant =
            build_durable_block_v1(&prepared, evidence_changed).expect("evidence variant");
        assert_ne!(base.header.block_hash, evidence_variant.header.block_hash);

        let mut expected_output_changed = prepared.clone();
        expected_output_changed.expected_aoem_output_commitment = Some("f".repeat(64));
        let expected_output_variant =
            build_durable_block_v1(&expected_output_changed, base_input.clone())
                .expect("expected output variant");
        assert_ne!(
            base.header.block_hash,
            expected_output_variant.header.block_hash
        );

        let mut cumulative_changed = base_input;
        cumulative_changed.cumulative_receipt_root[0] ^= 1;
        let cumulative_variant =
            build_durable_block_v1(&prepared, cumulative_changed).expect("cumulative variant");
        assert_ne!(base.header.block_hash, cumulative_variant.header.block_hash);
    }

    #[test]
    fn transaction_and_body_limits_are_enforced() {
        let test = TestLedgerV1::new("limits");
        let chain_id = 71_006;
        let too_many = candidate_input_v1(
            context_v1(chain_id, 1, [0u8; 32]),
            [0x16; 32],
            0x26,
            NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1 + 1,
        );
        assert!(test
            .ledger()
            .prepare(too_many)
            .expect_err("too many txs")
            .to_string()
            .contains("transaction count exceeds"));

        let too_large = NovNativeBlockCandidateInputV1 {
            context: context_v1(chain_id, 1, [0u8; 32]),
            tx_hashes: vec![[0x77; 32]],
            raw_txs: vec![vec![0u8; NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1 + 1]],
            pre_state_root: [0x16; 32],
            aoem_parent: None,
        };
        assert!(test
            .ledger()
            .prepare(too_large)
            .expect_err("body too large")
            .to_string()
            .contains("body exceeds"));
    }

    #[test]
    fn load_detects_tampered_body_and_never_upgrades_seal_flags() {
        let test = TestLedgerV1::new("tamper");
        let chain_id = 71_007;
        let prepared = test
            .ledger()
            .prepare(candidate_input_v1(
                context_v1(chain_id, 1, [0u8; 32]),
                [0x17; 32],
                0x27,
                1,
            ))
            .expect("prepare");
        let block =
            commit_bound_v1(test.ledger(), &prepared, commit_input_v1(0x37, 1, 1)).expect("commit");

        let mut body = block.body.clone();
        body.raw_txs[0][0] ^= 1;
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            body_key_v1(chain_id, &block.header.block_hash).as_bytes(),
            &body,
            "tampered body",
        )
        .expect("encode tampered body");
        write_sync_v1(&test.ledger().db, batch).expect("write tampered body");

        assert!(test
            .ledger()
            .load_by_hash(chain_id, block.header.block_hash)
            .expect_err("tampered body must fail")
            .to_string()
            .contains("commitment binding mismatch"));
        assert!(test
            .ledger()
            .status(chain_id)
            .expect_err("tampered head block must make status fail closed")
            .to_string()
            .contains("commitment binding mismatch"));
    }

    #[test]
    fn load_detects_tampered_receipt_index() {
        let test = TestLedgerV1::new("tamper-receipt-index");
        let chain_id = 71_009;
        let prepared = test
            .ledger()
            .prepare(candidate_input_v1(
                context_v1(chain_id, 1, [0u8; 32]),
                [0x19; 32],
                0x29,
                1,
            ))
            .expect("prepare");
        let block =
            commit_bound_v1(test.ledger(), &prepared, commit_input_v1(0x39, 1, 1)).expect("commit");
        let tx_hash = block.body.tx_hashes[0];
        let mut receipt = test
            .ledger()
            .load_receipt_location(chain_id, tx_hash)
            .expect("load receipt index")
            .expect("receipt index exists");
        receipt.receipt_commitment[0] ^= 1;
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            receipt_key_v1(chain_id, &tx_hash).as_bytes(),
            &receipt,
            "tampered receipt index",
        )
        .expect("encode tampered receipt index");
        write_sync_v1(&test.ledger().db, batch).expect("write tampered receipt index");

        assert!(test
            .ledger()
            .load_by_hash(chain_id, block.header.block_hash)
            .expect_err("tampered receipt index must fail")
            .to_string()
            .contains("receipt index binding mismatch"));
    }

    #[test]
    fn hydration_reads_a_bounded_ascending_durable_window() {
        let test = TestLedgerV1::new("hydrate");
        let chain_id = 71_008;
        let mut parent = [0u8; 32];
        let mut pre_state = [0x18; 32];
        let mut parent_block = None::<NovNativeDurableBlockV1>;
        for height in 1..=3 {
            let mut input = candidate_input_v1(
                context_v1(chain_id, height, parent),
                pre_state,
                0x30 + height as u8,
                1,
            );
            if let Some(previous) = parent_block.as_ref() {
                input = with_aoem_parent_v1(input, previous);
            }
            let prepared = test
                .ledger()
                .prepare(input)
                .expect("prepare hydration block");
            let block = commit_bound_v1(
                test.ledger(),
                &prepared,
                commit_input_v1(0x40 + height as u8, 1, height),
            )
            .expect("commit hydration block");
            parent = block.header.block_hash;
            pre_state = block.header.post_state_root;
            parent_block = Some(block);
        }

        let recent = test
            .ledger()
            .load_recent_blocks(chain_id, 2)
            .expect("load recent blocks");
        assert_eq!(
            recent
                .iter()
                .map(|block| block.header.height)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        let from = test
            .ledger()
            .load_blocks_from_height(chain_id, 2, 1)
            .expect("load block window");
        assert_eq!(from.len(), 1);
        assert_eq!(from[0].header.height, 2);
        assert!(test
            .ledger()
            .load_blocks_from_height(chain_id, 4, 2)
            .expect("future block window should be empty")
            .is_empty());
    }
}
