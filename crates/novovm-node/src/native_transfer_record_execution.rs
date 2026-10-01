#![forbid(unsafe_code)]

//! Record-profile transfer execution. The working view contains only declared
//! account/nonce/fee records and bounded diagnostic windows. The finalizer
//! never computes a legacy whole-state hash or clones a historical store.
//!
//! `execute_segment_v1` is the compatibility bridge for the current candidate
//! envelope: it imports/exports the complete image once per Transfer segment.
//! It is NOT yet a claim of history-independent candidate loading/validation.

use super::native_record_commitment as consensus;
use super::native_store_records::{
    self as physical, NativeRecordAccessV1, RawPathChangeV1, Records,
};
use super::native_transfer_dispatch::{Item, TransferReceiptFinalizerV1};
use super::native_transfer_state_access::TransferAccessV1;
use super::*;
use crate::native_state_records::{
    read_record, visit_records, RecordChange, RecordOverlayV1, RecordScanBudget,
    RecordStatsDeltaV1, StagedRecordUpdate, StateRecordReader,
};
use crate::native_state_tree::{NodeHash, StateNodeReader};
use std::cell::RefCell;

#[path = "native_transfer_record_effects.rs"]
mod effects;
use effects::TransferRecordEffectsV1;

/// Bounded, execution-local immutable content cache. No roots, verification
/// permissions or negative lookups survive this call. Traversals still verify
/// node/blob hashes; shared paths need not repeat AOEM database reads.
pub(super) struct ExecutionReader<'a> {
    inner: &'a dyn StateRecordReader,
    nodes: RefCell<BTreeMap<NodeHash, Vec<u8>>>,
    chunks: RefCell<BTreeMap<(NodeHash, u32), Vec<u8>>>,
}

impl<'a> ExecutionReader<'a> {
    pub(super) fn new(inner: &'a dyn StateRecordReader) -> Self {
        Self {
            inner,
            nodes: RefCell::new(BTreeMap::new()),
            chunks: RefCell::new(BTreeMap::new()),
        }
    }
}

impl StateNodeReader for ExecutionReader<'_> {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        if let Some(value) = self.nodes.borrow().get(hash) {
            return Ok(Some(value.clone()));
        }
        let value = self.inner.read_node(hash)?;
        if let Some(bytes) = &value {
            crate::native_state_tree::validate_state_node_bytes(hash, bytes)?;
            if self.nodes.borrow().len() < 16_384 {
                self.nodes.borrow_mut().insert(*hash, bytes.clone());
            }
        }
        Ok(value)
    }
}
impl StateRecordReader for ExecutionReader<'_> {
    fn read_record_chunk(&self, hash: NodeHash, index: u32) -> Result<Option<Vec<u8>>> {
        let key = (hash, index);
        if let Some(value) = self.chunks.borrow().get(&key) {
            return Ok(Some(value.clone()));
        }
        let value = self.inner.read_record_chunk(hash, index)?;
        if let Some(bytes) = &value {
            if bytes.len() > crate::native_state_records::RECORD_CHUNK_BYTES_V1 {
                bail!("rooted transfer inherited record chunk exceeds codec bound");
            }
            if self.chunks.borrow().len() < 4_096 {
                self.chunks.borrow_mut().insert(key, bytes.clone());
            }
        }
        Ok(value)
    }
}

struct EmptyReader;
impl StateNodeReader for EmptyReader {
    fn read_node(&self, _: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}
impl StateRecordReader for EmptyReader {
    fn read_record_chunk(&self, _: NodeHash, _: u32) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

struct ImportedReader<'a>(&'a StagedRecordUpdate);
impl StateNodeReader for ImportedReader<'_> {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.nodes().get(hash).cloned())
    }
}
impl StateRecordReader for ImportedReader<'_> {
    fn read_record_chunk(&self, hash: NodeHash, index: u32) -> Result<Option<Vec<u8>>> {
        let Some(blob) = self.0.blobs().get(&hash) else {
            return Ok(None);
        };
        let start = usize::try_from(index)?
            .checked_mul(512)
            .context("record chunk overflow")?;
        Ok(blob
            .get(start..)
            .map(|tail| tail[..tail.len().min(512)].to_vec()))
    }
}

struct EncodedAccess<'a>(&'a Records);
impl NativeRecordAccessV1 for EncodedAccess<'_> {
    fn read_path(&self, path: &[&str]) -> Result<Option<Vec<u8>>> {
        let path: Vec<_> = path.iter().map(|part| (*part).to_owned()).collect();
        let key = physical::key(&path)?;
        let Some(value) = self.0.get(&key) else {
            return Ok(None);
        };
        let (stored_path, raw) = physical::unpack(&key, value)?;
        if stored_path != path {
            bail!("record lookup path mismatch");
        }
        Ok(Some(if raw == physical::OBJECT {
            b"{}".to_vec()
        } else {
            raw.to_vec()
        }))
    }
}

/// A single authenticated physical view plus its independently committed state
/// and receipt roots. Every requested consensus-bearing value is cross-checked;
/// a hash-correct physical tree alone is not a business-state authority.
pub(super) struct RootedAccess<'a, 'r> {
    pub(super) physical: &'a RecordOverlayV1<'r>,
    pub(super) state: &'a RecordOverlayV1<'r>,
    pub(super) receipts: &'a RecordOverlayV1<'r>,
}

impl NativeRecordAccessV1 for RootedAccess<'_, '_> {
    fn read_path(&self, path: &[&str]) -> Result<Option<Vec<u8>>> {
        let raw = self.physical.read_path(path)?;
        let owned: Vec<_> = path.iter().map(|part| (*part).to_owned()).collect();
        let change = match &raw {
            Some(value) => RawPathChangeV1::Put {
                path: owned,
                value: value.clone(),
            },
            None => RawPathChangeV1::Delete { path: owned },
        };
        if let Some(change) = consensus::consensus_change_v1(&change)? {
            let (key, expected) = match change {
                RecordChange::Put { key, value } => (key, Some(value)),
                RecordChange::Delete { key } => (key, None),
            };
            if read_record(self.state, self.state.root(), &key)? != expected {
                bail!("rooted transfer physical/state read mismatch");
            }
        }
        if let ["receipts", hash] = path {
            let key = parse_fixed_hex_32_v1(hash, "rooted transfer receipt key")?;
            let expected = raw
                .as_ref()
                .map(|value| -> Result<Vec<u8>> {
                    let receipt: NovNativeExecutionReceiptV1 = serde_json::from_slice(value)?;
                    let RecordChange::Put { key: actual, value } =
                        consensus::receipt_change_v1(&receipt)?
                    else {
                        bail!("receipt projection must be a put");
                    };
                    if actual != key {
                        bail!("rooted transfer physical receipt key mismatch");
                    }
                    Ok(value)
                })
                .transpose()?;
            if read_record(self.receipts, self.receipts.root(), &key)? != expected {
                bail!("rooted transfer physical/receipt read mismatch");
            }
        }
        Ok(raw)
    }
}

/// Isolated updates, never authority. The physical statistics count live NRB1
/// blob bytes, not disk writes. Only the caller's existing output protocol may
/// publish their completion and subsequently seek BFT promotion.
pub(super) struct RootedTransferUpdateV1 {
    pub physical: StagedRecordUpdate,
    pub state: StagedRecordUpdate,
    pub receipts: StagedRecordUpdate,
    pub stats: RecordStatsDeltaV1,
    pub peak_inflight: usize,
    pub changes: Vec<RawPathChangeV1>,
}

/// The caller has verified all three parent roots and authenticated the entire
/// candidate. This function neither imports nor scans historical state. Only
/// declared account/nonce/fee/window records are loaded and changed.
pub(super) fn execute_rooted_segment_v1(
    reader: &dyn StateRecordReader,
    physical_root: NodeHash,
    state_root: NodeHash,
    receipt_root: NodeHash,
    items: &[Item<'_>],
    now_ms: u128,
) -> Result<RootedTransferUpdateV1> {
    let reader = ExecutionReader::new(reader);
    let mut physical = RecordOverlayV1::new(&reader, physical_root);
    let mut state = RecordOverlayV1::new(&reader, state_root);
    let mut receipts = RecordOverlayV1::new(&reader, receipt_root);
    let batch: Vec<_> = items
        .iter()
        .map(|item| (item.transaction, item.reservation))
        .collect();
    let mut sparse = TransferAccessV1::for_batch(&batch)?.load(&RootedAccess {
        physical: &physical,
        state: &state,
        receipts: &receipts,
    })?;
    let mut effects = TransferRecordEffectsV1::new(sparse.captured_records_v1());
    let peak_inflight = native_transfer_dispatch::execute_with_finalizer_v1(
        sparse.working_store_mut(),
        items,
        now_ms,
        &mut RecordFinalizer {
            state: &mut state,
            receipts: &mut receipts,
            before: None,
            effects: EffectSource::Typed(&mut effects),
            #[cfg(test)]
            encodings_at_begin: 0,
        },
    )?;
    // Unique sorted paths put new parent markers before children, including
    // across staging calls. Bound each call, not the whole candidate's writes.
    let changes = sparse.changed_records()?;
    if changes != effects.net_changes() {
        bail!("typed transfer effects differ from independent final sparse patch");
    }
    let mut stats = RecordStatsDeltaV1::default();
    let mut offset = 0;
    while offset < changes.len() {
        let mut end = offset;
        let mut bytes = 0usize;
        while end < changes.len() && end - offset < 128 {
            let cost = match &changes[end] {
                RawPathChangeV1::Put { value, .. } => value.len().saturating_add(512),
                RawPathChangeV1::Delete { .. } => 512,
            };
            if end != offset && bytes.saturating_add(cost) > 8 * 1024 * 1024 {
                break;
            }
            bytes = bytes
                .checked_add(cost)
                .context("record patch byte overflow")?;
            end += 1;
        }
        let delta =
            physical::apply_raw_changes_to_overlay_v1(&mut physical, &changes[offset..end])?;
        stats.records = stats
            .records
            .checked_add(delta.records)
            .context("record count delta overflow")?;
        stats.blob_bytes = stats
            .blob_bytes
            .checked_add(delta.blob_bytes)
            .context("record byte delta overflow")?;
        offset = end;
    }
    Ok(RootedTransferUpdateV1 {
        // Only the final physical/receipt roots are published. Intermediate
        // state roots, in contrast, are bound into individual receipts, so
        // retain their staged content. Neither operation deletes old roots.
        physical: physical.finish_compacted()?,
        state: state.finish(),
        receipts: receipts.finish_compacted()?,
        stats,
        peak_inflight,
        changes,
    })
}

pub(super) struct UpdatedReaderV1<'a> {
    pub reader: &'a dyn StateRecordReader,
    pub update: &'a StagedRecordUpdate,
}
impl StateNodeReader for UpdatedReaderV1<'_> {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        match self.update.nodes().get(hash) {
            Some(bytes) => Ok(Some(bytes.clone())),
            None => self.reader.read_node(hash),
        }
    }
}
impl StateRecordReader for UpdatedReaderV1<'_> {
    fn read_record_chunk(&self, hash: NodeHash, index: u32) -> Result<Option<Vec<u8>>> {
        let Some(blob) = self.update.blobs().get(&hash) else {
            return self.reader.read_record_chunk(hash, index);
        };
        let start = usize::try_from(index)?
            .checked_mul(512)
            .context("rooted transfer record chunk overflow")?;
        Ok(blob
            .get(start..)
            .map(|tail| tail[..tail.len().min(512)].to_vec()))
    }
}

/// Explicit cold bridge for current output validation and mixed Execute
/// compatibility. It is not called inside the rooted transaction executor.
pub(super) fn materialize_update_v1(
    reader: &dyn StateRecordReader,
    update: &StagedRecordUpdate,
    records: usize,
    blob_bytes: usize,
) -> Result<NovNativeExecutionStoreV1> {
    #[cfg(test)]
    super::candidate_workspace::assert_materialization_allowed_for_test()?;
    let reader = UpdatedReaderV1 { reader, update };
    let mut values = BTreeMap::new();
    let stats = visit_records(
        &reader,
        update.root(),
        RecordScanBudget {
            max_nodes: records.checked_mul(2).context("record count overflow")?,
            max_records: records,
            max_bytes: blob_bytes,
        },
        &mut |key, value| {
            if values.insert(key.to_vec(), value.to_vec()).is_some() {
                bail!("duplicate rooted transfer output record");
            }
            Ok(())
        },
    )?;
    if stats.records != records || stats.bytes != blob_bytes {
        bail!("rooted transfer output statistics mismatch");
    }
    physical::decode(values)
}

/// Diff only a bounded sparse view. Changes are subsequently checked against
/// its declared access set before being merged into the candidate image.
#[cfg(test)]
fn changes(before: &Records, after: &Records) -> Result<Vec<RawPathChangeV1>> {
    let mut result = Vec::new();
    for (key, value) in after {
        if before.get(key) != Some(value) {
            let (path, raw) = physical::unpack(key, value)?;
            result.push(RawPathChangeV1::Put {
                path,
                value: if raw == physical::OBJECT {
                    b"{}".to_vec()
                } else {
                    raw.to_vec()
                },
            });
        }
    }
    for (key, value) in before {
        if !after.contains_key(key) {
            let (path, _) = physical::unpack(key, value)?;
            result.push(RawPathChangeV1::Delete { path });
        }
    }
    Ok(result)
}

/// Canonical, uniquely keyed patch digest. Full values are committed by hash,
/// not converted to JSON Value (which cannot faithfully carry every u128).
fn delta_commitment(changes: &[RawPathChangeV1]) -> Result<(usize, String)> {
    use sha2::{Digest, Sha256};
    let mut ordered = BTreeMap::new();
    for change in changes {
        if let Some(change) = consensus::consensus_change_v1(change)? {
            let (key, value) = match change {
                RecordChange::Put { key, value } => (key, Some(value)),
                RecordChange::Delete { key } => (key, None),
            };
            if ordered.insert(key, value).is_some() {
                bail!("duplicate semantic patch key");
            }
        }
    }
    let count = ordered.len();
    let mut hash = Sha256::new();
    hash.update(b"novovm-native-record-semantic-delta-v1\0");
    hash.update((count as u64).to_be_bytes());
    for (key, value) in ordered {
        hash.update((key.len() as u64).to_be_bytes());
        hash.update(key);
        match value {
            Some(value) => {
                hash.update([1]);
                hash.update((value.len() as u64).to_be_bytes());
                hash.update(value);
            }
            None => hash.update([0]),
        }
    }
    Ok((count, to_hex(&hash.finalize())))
}

/// The full-view algorithm remains an independent delta oracle in tests only.
/// Production can only construct the bounded typed effect source.
enum EffectSource<'a> {
    Typed(&'a mut TransferRecordEffectsV1),
    #[cfg(test)]
    FullEncodeOracle(Option<Records>),
}

impl EffectSource<'_> {
    fn begin(&mut self, _store: &NovNativeExecutionStoreV1) -> Result<()> {
        #[cfg(test)]
        if let Self::FullEncodeOracle(before) = self {
            *before = Some(physical::encode(_store)?);
        }
        Ok(())
    }

    fn business(
        &mut self,
        store: &NovNativeExecutionStoreV1,
        transaction: &NovNativeTxWireV1,
        reservation: &NovNativeDurableAuthReservationV1,
    ) -> Result<Vec<RawPathChangeV1>> {
        match self {
            Self::Typed(effects) => effects.business(store, transaction, reservation),
            #[cfg(test)]
            Self::FullEncodeOracle(before) => {
                let after = physical::encode(store)?;
                let delta = changes(before.as_ref().context("oracle before missing")?, &after)?;
                *before = Some(after);
                Ok(delta)
            }
        }
    }

    fn finalized(
        &mut self,
        store: &NovNativeExecutionStoreV1,
        tx_hash: &str,
        sequence: u64,
        evicted: Option<&str>,
    ) -> Result<Vec<RawPathChangeV1>> {
        match self {
            Self::Typed(effects) => effects.finalized(store, tx_hash, sequence, evicted),
            #[cfg(test)]
            Self::FullEncodeOracle(before) => changes(
                &before.take().context("oracle business missing")?,
                &physical::encode(store)?,
            ),
        }
    }
}

struct RecordFinalizer<'a, 'r> {
    state: &'a mut RecordOverlayV1<'r>,
    receipts: &'a mut RecordOverlayV1<'r>,
    before: Option<(NodeHash, u64, String)>,
    effects: EffectSource<'a>,
    #[cfg(test)]
    encodings_at_begin: usize,
}

impl TransferReceiptFinalizerV1 for RecordFinalizer<'_, '_> {
    fn begin(&mut self, store: &NovNativeExecutionStoreV1) -> Result<()> {
        if self.before.is_some() {
            bail!("record finalizer has an unfinished transfer");
        }
        #[cfg(test)]
        {
            self.encodings_at_begin = physical::full_store_encodings_for_test_v1();
        }
        self.effects.begin(store)?;
        self.before = Some((
            self.state.root(),
            store.module_state.aoem_semantic_ledger_sequence,
            store.module_state.aoem_semantic_ledger_head.clone(),
        ));
        Ok(())
    }

    fn finish(
        &mut self,
        store: &mut NovNativeExecutionStoreV1,
        transaction: &NovNativeTxWireV1,
        request: &NovExecutionRequestV1,
        fee: &NovSettledFeeV1,
        subject: &NovExecutionSubjectMetaV1,
        reservation: &NovNativeDurableAuthReservationV1,
        ingress: NovAoemSemanticIngressMetaV1,
        now_ms: u128,
        mut receipt: NovNativeExecutionReceiptV1,
    ) -> Result<()> {
        let (before_root, previous_sequence, previous_seal) = self
            .before
            .take()
            .context("record finalizer has no ordered pre-fee state")?;
        let sequence = previous_sequence
            .checked_add(1)
            .context("semantic sequence overflow")?;
        receipt.aoem_semantic_ingress = Some(ingress);
        commit_nov_native_durable_auth_reservation_v1(store, reservation)?;
        let business_changes = self.effects.business(store, transaction, reservation)?;
        let (delta_count, delta_digest) = delta_commitment(&business_changes)?;
        consensus::apply_consensus_changes_v1(self.state, &business_changes)?;
        let business_root = self.state.root();
        let state_before = to_hex(&before_root);
        let state_after = to_hex(&business_root);
        let meta = receipt
            .aoem_semantic_ingress
            .as_mut()
            .context("record transfer ingress missing")?;
        let seal = to_hex(&sha256_bytes_v1(&[
            b"novovm-native-record-semantic-ledger-seal-v1\0",
            consensus::STATE_ROOT_CODEC_V1.as_bytes(),
            receipt.tx_hash.as_bytes(),
            meta.semantic_entry.as_bytes(),
            &meta.plan_id.to_le_bytes(),
            meta.wire_digest.as_bytes(),
            delta_digest.as_bytes(),
            &before_root,
            &business_root,
            previous_seal.as_bytes(),
            &sequence.to_le_bytes(),
        ]));
        meta.semantic_delta_count = delta_count;
        meta.semantic_delta_digest = delta_digest.clone();
        meta.semantic_state_before_digest = state_before.clone();
        meta.semantic_state_after_digest = state_after.clone();
        meta.semantic_ledger_sequence = sequence;
        meta.semantic_ledger_prev_seal = previous_seal.clone();
        meta.semantic_ledger_commit_seal = seal.clone();
        receipt.logs.push(NovNativeExecutionLogV1 {
            module: "aoem".into(),
            method: "semantic_record_commit".into(),
            event: "aoem.native_asset.semantic_record_commit".into(),
            data: serde_json::json!({
                "codec": "novovm-native-record-semantic-ledger-seal/v1",
                "state_root_codec": consensus::STATE_ROOT_CODEC_V1,
                "delta_count": delta_count, "delta_digest": delta_digest,
                "state_before_digest": state_before, "state_after_digest": state_after,
                "ledger_sequence": sequence, "prev_seal": previous_seal, "commit_seal": seal,
            }),
        });
        store.module_state.aoem_semantic_ledger_sequence = sequence;
        store.module_state.aoem_semantic_ledger_head = seal;
        receipt.aoem_semantic_commit = build_native_receipt_aoem_semantic_commit_v1(&receipt);
        if store
            .receipts
            .insert(receipt.tx_hash.clone(), receipt.clone())
            .is_some()
        {
            bail!("record transfer cannot replace a receipt");
        }
        let trace = build_execution_trace_v1(request, fee, &receipt, subject, store, now_ms);
        // Replacing an existing trace reorders it but does not evict another.
        // The loader checked the bound and uniqueness, so at most one eviction
        // is possible when appending this receipt's new trace.
        let trace_key = normalize_tx_hash_hex_v1(&trace.tx_id);
        let order = &store.module_state.execution_trace_order;
        let evicted =
            if order.len() == NOV_EXECUTION_TRACE_MAX_ENTRIES_V1 && !order.contains(&trace_key) {
                order.first().cloned()
            } else {
                None
            };
        persist_execution_trace_v1(store, trace);
        store.last_updated_unix_ms = now_ms;
        let mirror = build_native_aoem_semantic_ledger_mirror_record_v1(&receipt, now_ms)
            .context("record semantic mirror missing")?;
        if store
            .module_state
            .aoem_semantic_ledger_records
            .insert(sequence, mirror)
            .is_some()
        {
            bail!("record semantic sequence already exists");
        }
        let finalized =
            self.effects
                .finalized(store, &receipt.tx_hash, sequence, evicted.as_deref())?;
        consensus::apply_consensus_changes_v1(self.state, &finalized)?;
        consensus::apply_receipts_v1(self.receipts, &[receipt])?;
        #[cfg(test)]
        {
            let expected = match self.effects {
                EffectSource::Typed(_) => 0,
                EffectSource::FullEncodeOracle(_) => 3,
            };
            assert_eq!(
                physical::full_store_encodings_for_test_v1() - self.encodings_at_begin,
                expected,
                "per-transaction full sparse Store encoding count"
            );
        }
        Ok(())
    }
}

/// Real candidate adapter; isolated execution only. A caller must authenticate
/// the entire candidate first and discard it on any error. No authority writes.
pub(super) fn execute_segment_v1(
    store: &mut NovNativeExecutionStoreV1,
    items: &[Item<'_>],
    now_ms: u128,
) -> Result<usize> {
    let mut records = physical::encode(store)?;
    let batch: Vec<_> = items
        .iter()
        .map(|item| (item.transaction, item.reservation))
        .collect();
    let mut sparse = TransferAccessV1::for_batch(&batch)?.load(&EncodedAccess(&records))?;
    let state_import = consensus::stage_consensus_import_v1(&EmptyReader, &store.module_state)?;
    let receipt_import = consensus::stage_receipt_import_v1(&EmptyReader, store)?;
    let state_reader = ImportedReader(&state_import);
    let receipt_reader = ImportedReader(&receipt_import);
    let mut state = RecordOverlayV1::new(&state_reader, state_import.root());
    let mut receipts = RecordOverlayV1::new(&receipt_reader, receipt_import.root());
    let mut effects = TransferRecordEffectsV1::new(sparse.captured_records_v1());
    let peak = native_transfer_dispatch::execute_with_finalizer_v1(
        sparse.working_store_mut(),
        items,
        now_ms,
        &mut RecordFinalizer {
            state: &mut state,
            receipts: &mut receipts,
            before: None,
            effects: EffectSource::Typed(&mut effects),
            #[cfg(test)]
            encodings_at_begin: 0,
        },
    )?;
    // Export only declared changes. Never substitute the sparse view for a
    // complete store: it deliberately omits untouched accounts/assets/history.
    let changes = sparse.changed_records()?;
    if changes != effects.net_changes() {
        bail!("typed transfer effects differ from independent final sparse patch");
    }
    for change in changes {
        match change {
            RawPathChangeV1::Put { path, value } => {
                records.insert(
                    physical::key(&path)?,
                    physical::value(&path, &physical::physical_path_value_v1(&path, &value)?)?,
                );
            }
            RawPathChangeV1::Delete { path } => {
                records.remove(&physical::key(&path)?);
            }
        }
    }
    let complete = physical::decode(records)?;
    // Transitional independent cold validation. Remove only when rooted
    // envelopes carry and verify these same roots, not to hide discrepancies.
    if consensus::consensus_state_root_v1(&complete.module_state)? != state.root()
        || consensus::cumulative_receipt_root_v1(&complete)? != receipts.root()
    {
        bail!("incremental candidate root differs from complete record projection");
    }
    *store = complete;
    Ok(peak)
}

/// Independent pre-optimization projection oracle: three full sparse encodes
/// per transaction, rather than the typed journal. Only tests can call this.
/// Business computation still runs on AOEM; no fake Host execution is used.
#[cfg(test)]
pub(super) fn execute_segment_fullencode_oracle_for_test_v1(
    store: &mut NovNativeExecutionStoreV1,
    items: &[Item<'_>],
    now_ms: u128,
) -> Result<usize> {
    let mut records = physical::encode(store)?;
    let batch: Vec<_> = items
        .iter()
        .map(|item| (item.transaction, item.reservation))
        .collect();
    let mut sparse = TransferAccessV1::for_batch(&batch)?.load(&EncodedAccess(&records))?;
    let state_import = consensus::stage_consensus_import_v1(&EmptyReader, &store.module_state)?;
    let receipt_import = consensus::stage_receipt_import_v1(&EmptyReader, store)?;
    let state_reader = ImportedReader(&state_import);
    let receipt_reader = ImportedReader(&receipt_import);
    let mut state = RecordOverlayV1::new(&state_reader, state_import.root());
    let mut receipts = RecordOverlayV1::new(&receipt_reader, receipt_import.root());
    let peak = native_transfer_dispatch::execute_with_finalizer_v1(
        sparse.working_store_mut(),
        items,
        now_ms,
        &mut RecordFinalizer {
            state: &mut state,
            receipts: &mut receipts,
            before: None,
            effects: EffectSource::FullEncodeOracle(None),
            encodings_at_begin: 0,
        },
    )?;
    for change in sparse.changed_records()? {
        match change {
            RawPathChangeV1::Put { path, value } => {
                records.insert(
                    physical::key(&path)?,
                    physical::value(&path, &physical::physical_path_value_v1(&path, &value)?)?,
                );
            }
            RawPathChangeV1::Delete { path } => {
                records.remove(&physical::key(&path)?);
            }
        }
    }
    let complete = physical::decode(records)?;
    if consensus::consensus_state_root_v1(&complete.module_state)? != state.root()
        || consensus::cumulative_receipt_root_v1(&complete)? != receipts.root()
    {
        bail!("fullencode oracle root differs from complete projection");
    }
    *store = complete;
    Ok(peak)
}
