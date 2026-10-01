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
    RecordChange, RecordOverlayV1, StagedRecordUpdate, StateRecordReader,
};
use crate::native_state_tree::{NodeHash, StateNodeReader};

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

/// Diff only a bounded sparse view. Changes are subsequently checked against
/// its declared access set before being merged into the candidate image.
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

struct RecordFinalizer<'a, 'r> {
    state: &'a mut RecordOverlayV1<'r>,
    receipts: &'a mut RecordOverlayV1<'r>,
    before: Option<(Records, NodeHash, u64, String)>,
}

impl TransferReceiptFinalizerV1 for RecordFinalizer<'_, '_> {
    fn begin(&mut self, store: &NovNativeExecutionStoreV1) -> Result<()> {
        if self.before.is_some() {
            bail!("record finalizer has an unfinished transfer");
        }
        self.before = Some((
            physical::encode(store)?,
            self.state.root(),
            store.module_state.aoem_semantic_ledger_sequence,
            store.module_state.aoem_semantic_ledger_head.clone(),
        ));
        Ok(())
    }

    fn finish(
        &mut self,
        store: &mut NovNativeExecutionStoreV1,
        request: &NovExecutionRequestV1,
        fee: &NovSettledFeeV1,
        subject: &NovExecutionSubjectMetaV1,
        reservation: &NovNativeDurableAuthReservationV1,
        ingress: NovAoemSemanticIngressMetaV1,
        now_ms: u128,
        mut receipt: NovNativeExecutionReceiptV1,
    ) -> Result<()> {
        let (before, before_root, previous_sequence, previous_seal) = self
            .before
            .take()
            .context("record finalizer has no ordered pre-fee state")?;
        let sequence = previous_sequence
            .checked_add(1)
            .context("semantic sequence overflow")?;
        receipt.aoem_semantic_ingress = Some(ingress);
        commit_nov_native_durable_auth_reservation_v1(store, reservation)?;
        let business = physical::encode(store)?;
        let business_changes = changes(&before, &business)?;
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
        consensus::apply_consensus_changes_v1(
            self.state,
            &changes(&business, &physical::encode(store)?)?,
        )?;
        consensus::apply_receipts_v1(self.receipts, &[receipt])?;
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
    let peak = native_transfer_dispatch::execute_with_finalizer_v1(
        sparse.working_store_mut(),
        items,
        now_ms,
        &mut RecordFinalizer {
            state: &mut state,
            receipts: &mut receipts,
            before: None,
        },
    )?;
    // Export only declared changes. Never substitute the sparse view for a
    // complete store: it deliberately omits untouched accounts/assets/history.
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
