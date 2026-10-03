//! Incremental consensus records, separate from the lossless physical image.
//!
//! Only module_state is committed here, excluding full AOEM mirror records and
//! precisely the machine-local trace diagnostics excluded by V3. Every other
//! module field remains committed, including sequence/head and stable traces.
//! JSON objects are recursively key-sorted; number tokens are preserved exactly
//! (1 and 1.0 remain different), never converted through Value/f64.
//!
//! These roots do not publish authority. Finalization order is: apply business
//! and nonce changes; capture that business root for the semantic receipt seal;
//! apply sequence/head/trace changes; only then commit the completed receipt to
//! the separate cumulative receipt tree. Never put the final state/receipt root
//! into a receipt which contributes to that same root.

use super::native_store_records::{self as physical, RawPathChangeV1, Records};
use super::{
    NovNativeExecutionModuleStateV1, NovNativeExecutionReceiptV1, NovNativeExecutionStoreV1,
};
use crate::native_state_records::{
    read_record, RecordChange, RecordOverlayV1, StagedRecordUpdate, StateRecordReader,
};
use crate::native_state_tree::{empty_root, NodeHash, StateNodeReader};
use anyhow::{bail, Context, Result};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Mutex, OnceLock};

pub(super) const STATE_ROOT_CODEC_V1: &str = crate::native_root_codecs::RECORD_STATE_ROOT_CODEC_V1;
const TRACE_DIAGNOSTICS: [&str; 15] = [
    "concurrent_execution_enabled",
    "concurrent_execution_model",
    "recommended_threads",
    "ingress_workers",
    "host_hw_threads",
    "host_budget_threads",
    "parallelism_reason",
    "enabled",
    "required",
    "submitted",
    "processed_ops",
    "success_ops",
    "total_writes",
    "return_code_name",
    "fallback_reason",
];

struct StrictObject(BTreeMap<String, Box<RawValue>>);
impl<'de> Deserialize<'de> for StrictObject {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct ObjectVisitor;
        impl<'de> Visitor<'de> for ObjectVisitor {
            type Value = StrictObject;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON object with unique keys")
            }
            fn visit_map<M: MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Self::Value, M::Error> {
                let mut result = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, Box<RawValue>>()? {
                    if result.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom(
                            "duplicate consensus JSON object key",
                        ));
                    }
                }
                Ok(StrictObject(result))
            }
        }
        deserializer.deserialize_map(ObjectVisitor)
    }
}

pub(super) fn canonical_raw_json_v1(bytes: &[u8]) -> Result<Vec<u8>> {
    fn append(raw: &RawValue, depth: usize, out: &mut Vec<u8>) -> Result<()> {
        if depth > 128 {
            bail!("consensus record JSON exceeds depth limit");
        }
        match raw.get().as_bytes().first() {
            Some(b'{') => {
                let StrictObject(object) = serde_json::from_str(raw.get())?;
                out.push(b'{');
                for (index, (key, value)) in object.iter().enumerate() {
                    if index != 0 {
                        out.push(b',');
                    }
                    out.extend_from_slice(&serde_json::to_vec(key)?);
                    out.push(b':');
                    append(value, depth + 1, out)?;
                }
                out.push(b'}');
            }
            Some(b'[') => {
                let items: Vec<Box<RawValue>> = serde_json::from_str(raw.get())?;
                out.push(b'[');
                for (index, value) in items.iter().enumerate() {
                    if index != 0 {
                        out.push(b',');
                    }
                    append(value, depth + 1, out)?;
                }
                out.push(b']');
            }
            Some(b'"') => out.extend_from_slice(&serde_json::to_vec(&serde_json::from_str::<
                String,
            >(raw.get())?)?),
            _ => out.extend_from_slice(raw.get().as_bytes()),
        }
        Ok(())
    }
    let raw: Box<RawValue> = serde_json::from_slice(bytes)?;
    let mut result = Vec::with_capacity(bytes.len());
    append(&raw, 0, &mut result)?;
    Ok(result)
}

fn trace_without_diagnostics(raw: &[u8]) -> Result<Vec<u8>> {
    let canonical = canonical_raw_json_v1(raw)?;
    if canonical == b"null" {
        return Ok(canonical);
    }
    let StrictObject(mut trace) = serde_json::from_slice(&canonical)?;
    if let Some(meta) = trace.get_mut("aoem_semantic_ingress") {
        if meta.get() != "null" {
            let StrictObject(mut object) = serde_json::from_str(meta.get())?;
            for name in TRACE_DIAGNOSTICS {
                object.remove(name);
            }
            *meta = serde_json::value::to_raw_value(&object)?;
        }
    }
    canonical_raw_json_v1(&serde_json::to_vec(&trace)?)
}

fn is_consensus_path(path: &[String]) -> Result<bool> {
    if path.first().map(String::as_str) != Some("module_state") {
        return Ok(false);
    }
    if path.len() > 4 {
        bail!("consensus record path exceeds layout depth");
    }
    if let Some(field) = path.get(1) {
        static FIELDS: std::sync::OnceLock<std::result::Result<BTreeSet<String>, String>> =
            std::sync::OnceLock::new();
        let fields = FIELDS
            .get_or_init(|| {
                let raw = serde_json::to_vec(&NovNativeExecutionModuleStateV1::default())
                    .map_err(|error| error.to_string())?;
                let StrictObject(fields) =
                    serde_json::from_slice(&raw).map_err(|error| error.to_string())?;
                Ok(fields.into_keys().collect())
            })
            .as_ref()
            .map_err(|error| anyhow::anyhow!("consensus module field schema: {error}"))?;
        if !fields.contains(field) {
            bail!("unknown consensus module field");
        }
    }
    if path.get(1).map(String::as_str) == Some("aoem_semantic_ledger_records") {
        return Ok(false);
    }
    if path.len() >= 3 && !physical::is_object_path_v1(&path[..path.len() - 1])? {
        bail!("consensus record parent is not a structural object");
    }
    Ok(true)
}

fn projected_value(path: &[String], raw: &[u8]) -> Result<Vec<u8>> {
    if physical::is_object_path_v1(path)? {
        if raw == physical::OBJECT {
            return Ok(physical::OBJECT.to_vec());
        }
        return physical::physical_path_value_v1(path, raw);
    }
    let is_trace = path.len() == 2 && path[1] == "last_execution_trace"
        || path.len() == 3 && path[1] == "execution_traces_by_tx";
    if is_trace {
        trace_without_diagnostics(raw)
    } else {
        canonical_raw_json_v1(raw)
    }
}

/// Map one raw physical-path change to the independent consensus record tree.
/// Store-level metadata, receipt bodies and full mirrors have no state-tree entry.
pub(super) fn consensus_change_v1(change: &RawPathChangeV1) -> Result<Option<RecordChange>> {
    let path = match change {
        RawPathChangeV1::Put { path, .. } | RawPathChangeV1::Delete { path } => path,
    };
    if !is_consensus_path(path)? {
        return Ok(None);
    }
    Ok(Some(match change {
        RawPathChangeV1::Put { path, value } => RecordChange::Put {
            key: physical::key(path)?,
            value: physical::value(path, &projected_value(path, value)?)?,
        },
        RawPathChangeV1::Delete { path } => RecordChange::Delete {
            key: physical::key(path)?,
        },
    }))
}

pub(super) fn apply_consensus_changes_v1(
    overlay: &mut RecordOverlayV1<'_>,
    changes: &[RawPathChangeV1],
) -> Result<()> {
    let changes = changes
        .iter()
        .map(consensus_change_v1)
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    overlay.stage(&changes)
}

/// Cold import of all module fields, not a hot-path operation or authority grant.
pub(super) fn consensus_records_v1(state: &NovNativeExecutionModuleStateV1) -> Result<Records> {
    let mut result = Records::new();
    for (key, value) in physical::encode_module_v1(state)? {
        let (path, raw) = physical::unpack(&key, &value)?;
        let change = RawPathChangeV1::Put {
            path,
            value: raw.to_vec(),
        };
        if let Some(RecordChange::Put { key, value }) = consensus_change_v1(&change)? {
            result.insert(key, value);
        }
    }
    Ok(result)
}

fn import_records(reader: &dyn StateRecordReader, records: Records) -> Result<StagedRecordUpdate> {
    let mut overlay = RecordOverlayV1::new(reader, empty_root());
    let mut changes = Vec::new();
    let mut bytes = 0usize;
    for (key, value) in records {
        let length = 10 + key.len() + value.len();
        if changes.len() >= 128 || bytes.saturating_add(length) > 8 * 1024 * 1024 {
            overlay.stage(&changes)?;
            changes.clear();
            bytes = 0;
        }
        bytes = bytes
            .checked_add(length)
            .context("consensus import byte count overflow")?;
        changes.push(RecordChange::Put { key, value });
    }
    overlay.stage(&changes)?;
    Ok(overlay.finish())
}

pub(super) fn stage_consensus_import_v1(
    reader: &dyn StateRecordReader,
    state: &NovNativeExecutionModuleStateV1,
) -> Result<StagedRecordUpdate> {
    import_records(reader, consensus_records_v1(state)?)
}

pub(super) fn receipt_change_v1(receipt: &NovNativeExecutionReceiptV1) -> Result<RecordChange> {
    let hash = super::parse_fixed_hex_32_v1(&receipt.tx_hash, "receipt record transaction hash")?;
    if super::to_hex(&hash) != receipt.tx_hash {
        bail!("receipt record transaction hash is not canonical");
    }
    Ok(RecordChange::Put {
        key: hash.to_vec(),
        value: super::full_native_receipt_commitment_v1(receipt)?.to_vec(),
    })
}

/// Append completed receipts after the business-root/seal/final-state phases.
/// An identical existing commitment is an idempotent retry; replacement with a
/// different receipt is rejected. This does not replace transaction nonce checks.
pub(super) fn apply_receipts_v1(
    overlay: &mut RecordOverlayV1<'_>,
    receipts: &[NovNativeExecutionReceiptV1],
) -> Result<()> {
    let mut changes = Vec::with_capacity(receipts.len());
    let mut seen = BTreeSet::new();
    for receipt in receipts {
        let change = receipt_change_v1(receipt)?;
        let RecordChange::Put { key, value } = &change else {
            unreachable!();
        };
        if !seen.insert(key.clone()) {
            bail!("duplicate receipt within cumulative-tree update");
        }
        if let Some(existing) = read_record(overlay, overlay.root(), key)? {
            if existing != *value {
                bail!("cumulative receipt commitment cannot be replaced");
            }
        }
        changes.push(change);
    }
    overlay.stage(&changes)
}

pub(super) fn stage_receipt_import_v1(
    reader: &dyn StateRecordReader,
    store: &NovNativeExecutionStoreV1,
) -> Result<StagedRecordUpdate> {
    let mut records = Records::new();
    for (hash, receipt) in &store.receipts {
        if *hash != receipt.tx_hash {
            bail!("receipt map key differs from receipt transaction hash");
        }
        let RecordChange::Put { key, value } = receipt_change_v1(receipt)? else {
            unreachable!();
        };
        if records.insert(key, value).is_some() {
            bail!("duplicate receipt import");
        }
    }
    import_records(reader, records)
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

fn consensus_state_root_uncached_v1(state: &NovNativeExecutionModuleStateV1) -> Result<NodeHash> {
    Ok(stage_consensus_import_v1(&EmptyReader, state)?.root())
}

fn cumulative_receipt_root_uncached_v1(store: &NovNativeExecutionStoreV1) -> Result<NodeHash> {
    Ok(stage_receipt_import_v1(&EmptyReader, store)?.root())
}

// Pure process-local memoization only: at most 64 (input hash, root) pairs per
// domain, 128 total. No input state, nodes, snapshots, authority or trust flags
// are retained. Each lookup still serializes/hashes the actual complete typed
// input, so this saves repeated Patricia construction, NOT O(history) scanning.
const COLD_ROOT_MEMO_ENTRIES_V1: usize = 64;
const STATE_MEMO_DOMAIN_V1: &[u8] = b"novovm-native-record-cold-state-root-memo-v1\0";
const RECEIPT_MEMO_DOMAIN_V1: &[u8] = b"novovm-native-record-cold-receipt-root-memo-v1\0";

#[derive(Default)]
struct ColdRootMemoV1 {
    roots: BTreeMap<NodeHash, NodeHash>,
    insertion_order: VecDeque<NodeHash>,
}

impl ColdRootMemoV1 {
    fn insert_verified(&mut self, key: NodeHash, root: NodeHash) -> Result<()> {
        if let Some(existing) = self.roots.get(&key) {
            if *existing != root {
                bail!("cold root computation disagrees for an identical typed input");
            }
            return Ok(());
        }
        if self.roots.len() >= COLD_ROOT_MEMO_ENTRIES_V1 {
            let oldest = self
                .insertion_order
                .pop_front()
                .context("cold root memo eviction invariant failed")?;
            self.roots.remove(&oldest);
        }
        self.roots.insert(key, root);
        self.insertion_order.push_back(key);
        Ok(())
    }
}

fn cold_root_key_v1<T: serde::Serialize + ?Sized>(domain: &[u8], input: &T) -> Result<NodeHash> {
    // Do not route u128 through Value/f64, nor key this by caller-provided roots
    // or snapshot metadata. Diagnostics may cause harmless additional misses.
    let bytes = physical::typed_raw_v1(input)?;
    Ok(super::sha256_bytes_v1(&[domain, &bytes]))
}

fn memoized_cold_root_v1(
    memo: &Mutex<ColdRootMemoV1>,
    key: NodeHash,
    compute: impl FnOnce() -> Result<NodeHash>,
) -> Result<NodeHash> {
    let cached = memo
        .lock()
        .map_err(|_| anyhow::anyhow!("cold root memo lock poisoned"))?
        .roots
        .get(&key)
        .copied();
    if let Some(root) = cached {
        return Ok(root);
    }
    // Never hold the lock during serialization/tree construction. Concurrent
    // misses may duplicate pure work; an error is returned and never cached.
    let root = compute()?;
    memo.lock()
        .map_err(|_| anyhow::anyhow!("cold root memo lock poisoned"))?
        .insert_verified(key, root)?;
    Ok(root)
}

pub(super) fn consensus_state_root_v1(state: &NovNativeExecutionModuleStateV1) -> Result<NodeHash> {
    static MEMO: OnceLock<Mutex<ColdRootMemoV1>> = OnceLock::new();
    memoized_cold_root_v1(
        MEMO.get_or_init(Mutex::default),
        cold_root_key_v1(STATE_MEMO_DOMAIN_V1, state)?,
        || consensus_state_root_uncached_v1(state),
    )
}

pub(super) fn cumulative_receipt_root_v1(store: &NovNativeExecutionStoreV1) -> Result<NodeHash> {
    static MEMO: OnceLock<Mutex<ColdRootMemoV1>> = OnceLock::new();
    memoized_cold_root_v1(
        MEMO.get_or_init(Mutex::default),
        cold_root_key_v1(RECEIPT_MEMO_DOMAIN_V1, &store.receipts)?,
        || cumulative_receipt_root_uncached_v1(store),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_state_records::RECORD_CHUNK_BYTES_V1;
    use physical::{NativeRecordAccessV1, RawPathChangeV1};

    #[derive(Default)]
    struct Memory {
        nodes: BTreeMap<NodeHash, Vec<u8>>,
        chunks: BTreeMap<(NodeHash, u32), Vec<u8>>,
    }
    impl StateNodeReader for Memory {
        fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
            Ok(self.nodes.get(hash).cloned())
        }
    }
    impl StateRecordReader for Memory {
        fn read_record_chunk(&self, hash: NodeHash, index: u32) -> Result<Option<Vec<u8>>> {
            Ok(self.chunks.get(&(hash, index)).cloned())
        }
    }
    impl Memory {
        fn persist(&mut self, staged: StagedRecordUpdate) -> NodeHash {
            self.nodes.extend(staged.nodes().clone());
            for (hash, blob) in staged.blobs() {
                for (index, chunk) in blob.chunks(RECORD_CHUNK_BYTES_V1).enumerate() {
                    self.chunks.insert((*hash, index as u32), chunk.to_vec());
                }
            }
            staged.root()
        }
    }
    fn put<T: serde::Serialize>(path: &[&str], value: &T) -> RawPathChangeV1 {
        RawPathChangeV1::Put {
            path: path.iter().map(|value| (*value).into()).collect(),
            value: physical::typed_raw_v1(value).unwrap(),
        }
    }
    fn receipt(byte: u8) -> NovNativeExecutionReceiptV1 {
        serde_json::from_value(serde_json::json!({
            "tx_hash": super::super::to_hex(&[byte; 32]), "status":true,"target":"native",
            "module":"native_asset","method":"transfer", "settled_fee_nov":0,
            "paid_asset":"NOV","paid_amount":0,"logs":[],"failure_reason":null,"fee_contract":"unified"
        })).unwrap()
    }

    #[test]
    fn record_canonical_json_sorts_nested_objects_without_numeric_rounding() {
        let number = u128::MAX.to_string();
        let a = format!(
            r#" {{ "z":[{{"b":{number},"a":1.0}}], "a":-170141183460469231731687303715884105728 }} "#
        );
        let b = format!(
            r#"{{"a":-170141183460469231731687303715884105728,"z":[{{"a":1.0,"b":{number}}}]}}"#
        );
        assert_eq!(canonical_raw_json_v1(a.as_bytes()).unwrap(), b.as_bytes());
        assert_eq!(
            physical::typed_raw_v1(&u128::MAX).unwrap(),
            number.as_bytes()
        );
        assert_ne!(
            canonical_raw_json_v1(b"1").unwrap(),
            canonical_raw_json_v1(b"1.0").unwrap()
        );
        assert!(canonical_raw_json_v1(br#"{"a":1,"a":2}"#).is_err());
        assert!(canonical_raw_json_v1(br#"{"nested":[{"a":1,"a":2}]}"#).is_err());
        assert!(canonical_raw_json_v1(b"1 2").is_err());
    }

    #[test]
    fn consensus_record_field_coverage_matches_v3_without_silent_economic_omissions() {
        let state = NovNativeExecutionModuleStateV1::default();
        let mut expected = BTreeSet::from(["account_asset_balances".to_owned()]);
        for (_, shard) in super::super::native_rocksdb_module_state_shard_keys_v1() {
            let StrictObject(fields) = serde_json::from_slice(
                &super::super::native_module_state_shard_value_v1(&state, shard).unwrap(),
            )
            .unwrap();
            expected.extend(fields.into_keys());
        }
        expected.remove("aoem_semantic_ledger_records");
        let mut actual = BTreeSet::new();
        for (key, value) in consensus_records_v1(&state).unwrap() {
            let (path, _) = physical::unpack(&key, &value).unwrap();
            assert_eq!(path[0], "module_state");
            if path.len() == 2 {
                actual.insert(path[1].clone());
            }
        }
        assert_eq!(actual, expected);
        assert!(
            consensus_change_v1(&put(&["module_state", "unknown_economic_field"], &1u64)).is_err()
        );
        for path in [
            &["authority_namespace"][..],
            &["last_updated_unix_ms"][..],
            &["receipts", "tx"][..],
            &["module_state", "aoem_semantic_ledger_records", "1"][..],
        ] {
            assert!(consensus_change_v1(&put(path, &1u64)).unwrap().is_none());
        }
    }

    #[test]
    fn physical_and_consensus_single_changes_match_cold_import_with_exact_u128() {
        let mut store = NovNativeExecutionStoreV1::default();
        store
            .module_state
            .account_asset_balances
            .insert("payer".into(), BTreeMap::from([("NOV".into(), u128::MAX)]));
        store
            .module_state
            .account_asset_balances
            .insert("recipient".into(), BTreeMap::from([("USDT".into(), 9)]));
        let mut memory = Memory::default();
        let physical_root =
            memory.persist(import_records(&memory, physical::encode(&store).unwrap()).unwrap());
        let state_root =
            memory.persist(stage_consensus_import_v1(&memory, &store.module_state).unwrap());
        let mut physical_view = RecordOverlayV1::new(&memory, physical_root);
        assert_eq!(
            physical_view
                .read_path(&["module_state", "account_asset_balances", "recipient"])
                .unwrap(),
            Some(b"{}".to_vec())
        );
        assert_eq!(
            physical_view
                .read_path(&["module_state", "account_asset_balances", "missing"])
                .unwrap(),
            None
        );
        assert_eq!(
            physical::read_typed_path_v1::<u128>(
                &physical_view,
                &["module_state", "account_asset_balances", "payer", "NOV"]
            )
            .unwrap(),
            Some(u128::MAX)
        );
        let changes = [
            put(
                &["module_state", "account_asset_balances", "payer", "NOV"],
                &(u128::MAX - 1),
            ),
            put(
                &["module_state", "account_asset_balances", "recipient", "NOV"],
                &1u128,
            ),
            put(
                &["module_state", "native_auth_next_nonces", "identity"],
                &1u64,
            ),
        ];
        physical::apply_raw_path_changes_v1(&mut physical_view, &changes).unwrap();
        let mut consensus_view = RecordOverlayV1::new(&memory, state_root);
        apply_consensus_changes_v1(&mut consensus_view, &changes).unwrap();
        store
            .module_state
            .account_asset_balances
            .get_mut("payer")
            .unwrap()
            .insert("NOV".into(), u128::MAX - 1);
        store
            .module_state
            .account_asset_balances
            .get_mut("recipient")
            .unwrap()
            .insert("NOV".into(), 1);
        store
            .module_state
            .native_auth_next_nonces
            .insert("identity".into(), 1);
        assert_eq!(
            consensus_view.root(),
            consensus_state_root_v1(&store.module_state).unwrap()
        );
        assert_eq!(
            physical_view.root(),
            import_records(&memory, physical::encode(&store).unwrap())
                .unwrap()
                .root()
        );
        // New account ancestry is explicit; an empty-object marker is not a
        // replacement of the recipient's existing USDT subtree.
        physical::apply_raw_path_changes_v1(
            &mut physical_view,
            &[put(
                &["module_state", "account_asset_balances", "recipient"],
                &BTreeMap::<String, u128>::new(),
            )],
        )
        .unwrap();
        assert_eq!(
            physical::read_typed_path_v1::<u128>(
                &physical_view,
                &[
                    "module_state",
                    "account_asset_balances",
                    "recipient",
                    "USDT"
                ]
            )
            .unwrap(),
            Some(9)
        );
        let before = physical_view.root();
        assert!(physical::apply_raw_path_changes_v1(
            &mut physical_view,
            &[put(
                &["module_state", "account_asset_balances", "recipient"],
                &BTreeMap::from([("NOV", 0)])
            )]
        )
        .is_err());
        assert_eq!(physical_view.root(), before);
    }

    #[test]
    fn record_state_ignores_runtime_diagnostics_but_commits_stable_trace_and_policy() {
        let trace = super::super::NovExecutionTraceV1 {
            tx_id: "tx".into(),
            max_pay_amount: u128::MAX,
            aoem_semantic_ingress: Some(super::super::NovAoemSemanticIngressMetaV1 {
                plan_id: 7,
                wire_digest: "wire".into(),
                host_hw_threads: 64,
                recommended_threads: 16,
                required: true,
                submitted: true,
                processed_ops: 2,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut first = NovNativeExecutionModuleStateV1 {
            last_execution_trace: Some(trace.clone()),
            ..Default::default()
        };
        first.execution_traces_by_tx.insert("tx".into(), trace);
        let mut second = first.clone();
        for trace in second
            .last_execution_trace
            .iter_mut()
            .chain(second.execution_traces_by_tx.values_mut())
        {
            let meta = trace.aoem_semantic_ingress.as_mut().unwrap();
            meta.host_hw_threads = 1;
            meta.recommended_threads = 1;
            meta.required = false;
            meta.submitted = false;
            meta.processed_ops = 0;
            meta.fallback_reason = Some("local diagnostic".into());
        }
        let root = consensus_state_root_v1(&first).unwrap();
        assert_eq!(root, consensus_state_root_v1(&second).unwrap());
        second
            .last_execution_trace
            .as_mut()
            .unwrap()
            .aoem_semantic_ingress
            .as_mut()
            .unwrap()
            .plan_id = 8;
        assert_ne!(root, consensus_state_root_v1(&second).unwrap());
        first.treasury_reserve_bucket_nov = u128::MAX;
        assert_ne!(root, consensus_state_root_v1(&first).unwrap());
        let memory = Memory::default();
        let mut staged = RecordOverlayV1::new(&memory, empty_root());
        let ordinary = put(
            &["module_state", "governance_proposals", "1"],
            &serde_json::json!({"a":1,"b":{"c":2}}),
        );
        apply_consensus_changes_v1(&mut staged, &[ordinary]).unwrap();
        let ordered = staged.root();
        let reordered = RawPathChangeV1::Put {
            path: vec![
                "module_state".into(),
                "governance_proposals".into(),
                "1".into(),
            ],
            value: br#"{"b":{"c":2},"a":1}"#.to_vec(),
        };
        apply_consensus_changes_v1(&mut staged, &[reordered]).unwrap();
        assert_eq!(ordered, staged.root());
    }

    #[test]
    fn cumulative_receipts_match_cold_import_and_cannot_be_replaced() {
        let reader = EmptyReader;
        let mut tree = RecordOverlayV1::new(&reader, empty_root());
        let mut store = NovNativeExecutionStoreV1::default();
        assert_eq!(cumulative_receipt_root_v1(&store).unwrap(), empty_root());
        for index in 1..=3 {
            let mut next = receipt(index);
            next.paid_amount = u128::MAX - u128::from(index);
            apply_receipts_v1(&mut tree, std::slice::from_ref(&next)).unwrap();
            store.receipts.insert(next.tx_hash.clone(), next.clone());
            assert_eq!(tree.root(), cumulative_receipt_root_v1(&store).unwrap());
            let root = tree.root();
            apply_receipts_v1(&mut tree, std::slice::from_ref(&next)).unwrap();
            assert_eq!(tree.root(), root);
            next.status = false;
            assert!(apply_receipts_v1(&mut tree, &[next]).is_err());
            assert_eq!(tree.root(), root);
        }
        assert!(apply_receipts_v1(&mut tree, &[receipt(4), receipt(4)]).is_err());
        let mut wrong = receipt(4);
        wrong.tx_hash = format!("0x{}", wrong.tx_hash);
        assert!(receipt_change_v1(&wrong).is_err());
        assert_eq!(
            STATE_ROOT_CODEC_V1,
            crate::native_root_codecs::NativeRootCodecProfileV1::RecordTreeV1.state_root_codec()
        );
    }

    #[test]
    fn cold_root_memo_matches_uncached_and_hashes_current_typed_u128_input() {
        let mut state = NovNativeExecutionModuleStateV1 {
            treasury_reserve_bucket_nov: u128::MAX,
            ..Default::default()
        };
        let root = consensus_state_root_uncached_v1(&state).unwrap();
        let key = cold_root_key_v1(STATE_MEMO_DOMAIN_V1, &state).unwrap();
        assert_eq!(consensus_state_root_v1(&state).unwrap(), root);
        assert_eq!(consensus_state_root_v1(&state).unwrap(), root);
        state.treasury_reserve_bucket_nov -= 1;
        assert_ne!(cold_root_key_v1(STATE_MEMO_DOMAIN_V1, &state).unwrap(), key);
        let changed = consensus_state_root_uncached_v1(&state).unwrap();
        assert_ne!(changed, root);
        assert_eq!(consensus_state_root_v1(&state).unwrap(), changed);

        let mut store = NovNativeExecutionStoreV1::default();
        let mut first = receipt(41);
        first.paid_amount = u128::MAX;
        store.receipts.insert(first.tx_hash.clone(), first);
        let root = cumulative_receipt_root_uncached_v1(&store).unwrap();
        let key = cold_root_key_v1(RECEIPT_MEMO_DOMAIN_V1, &store.receipts).unwrap();
        assert_eq!(cumulative_receipt_root_v1(&store).unwrap(), root);
        assert_eq!(cumulative_receipt_root_v1(&store).unwrap(), root);
        store.receipts.values_mut().next().unwrap().paid_amount -= 1;
        assert_ne!(
            cold_root_key_v1(RECEIPT_MEMO_DOMAIN_V1, &store.receipts).unwrap(),
            key
        );
        let changed = cumulative_receipt_root_uncached_v1(&store).unwrap();
        assert_ne!(changed, root);
        assert_eq!(cumulative_receipt_root_v1(&store).unwrap(), changed);
        assert_ne!(
            cold_root_key_v1(STATE_MEMO_DOMAIN_V1, &store.receipts).unwrap(),
            cold_root_key_v1(RECEIPT_MEMO_DOMAIN_V1, &store.receipts).unwrap()
        );
        // A previously cached valid receipt body does not admit a bad map key.
        let (_, receipt) = store.receipts.pop_first().unwrap();
        store.receipts.insert("wrong-map-key".into(), receipt);
        assert!(cumulative_receipt_root_v1(&store).is_err());
        assert!(cumulative_receipt_root_v1(&store).is_err());
    }

    #[test]
    fn cold_root_memo_is_bounded_and_never_caches_errors() {
        let memo = Mutex::new(ColdRootMemoV1::default());
        let key = [0; 32];
        assert!(memoized_cold_root_v1(&memo, key, || bail!("injected tree error")).is_err());
        assert!(memo.lock().unwrap().roots.is_empty());
        assert_eq!(
            memoized_cold_root_v1(&memo, key, || Ok([1; 32])).unwrap(),
            [1; 32]
        );
        assert_eq!(
            memoized_cold_root_v1(&memo, key, || panic!("hit must not reconstruct a tree"))
                .unwrap(),
            [1; 32]
        );
        for index in 1..=COLD_ROOT_MEMO_ENTRIES_V1 {
            let key = super::super::sha256_bytes_v1(&[&index.to_be_bytes()]);
            memoized_cold_root_v1(&memo, key, || Ok(key)).unwrap();
        }
        let mut cache = memo.lock().unwrap();
        assert_eq!(cache.roots.len(), COLD_ROOT_MEMO_ENTRIES_V1);
        assert_eq!(cache.insertion_order.len(), COLD_ROOT_MEMO_ENTRIES_V1);
        assert!(!cache.roots.contains_key(&[0; 32]));
        let (&key, &root) = cache.roots.first_key_value().unwrap();
        assert!(cache
            .insert_verified(key, root.map(|byte| byte ^ 1))
            .is_err());
        assert_eq!(cache.roots[&key], root);
    }

    #[test]
    fn cold_root_memo_concurrent_hits_and_misses_are_safe() {
        let memo = Mutex::new(ColdRootMemoV1::default());
        std::thread::scope(|scope| {
            for lane in 0..8 {
                let memo = &memo;
                scope.spawn(move || {
                    for index in 0..128usize {
                        let key = super::super::sha256_bytes_v1(&[&(index + lane).to_be_bytes()]);
                        assert_eq!(memoized_cold_root_v1(memo, key, || Ok(key)).unwrap(), key);
                    }
                });
            }
        });
        let cache = memo.lock().unwrap();
        assert_eq!(cache.roots.len(), COLD_ROOT_MEMO_ENTRIES_V1);
        assert_eq!(cache.insertion_order.len(), COLD_ROOT_MEMO_ENTRIES_V1);
        assert_eq!(
            cache.insertion_order.iter().collect::<BTreeSet<_>>().len(),
            COLD_ROOT_MEMO_ENTRIES_V1
        );
    }
}
