//! Local output-delta witness. Parent authority and transaction semantics are
//! verified by the caller; this module proves exact three-tree transitions.
//! It never scans a tree or grants finality from a prepared marker.
use super::*;
use crate::native_state_records::read_record;
use native_store_records::RawPathChangeV1;

pub(super) const SCHEMA: &str = "novovm-candidate-record-document/v3";
// Each encoded {path,op} entry uses at least this many bytes even for [].
const MIN_PATH_WIRE_BYTES: usize = 21;
const MAX_PATHS: usize = MAX_PAYLOAD_BYTES_V1 / MIN_PATH_WIRE_BYTES;
const STAGE_PATHS: usize = 128;
const STAGE_BYTES: usize = 8 * 1024 * 1024;

fn within_writer_budget(values: usize, paths: usize, count: usize) -> bool {
    values <= MAX_TOTAL_PAYLOAD_BYTES_V1 && paths <= MAX_PAYLOAD_BYTES_V1 && count <= MAX_PATHS
}

fn add_reconstructed_bytes(total: usize, next: usize) -> Result<usize> {
    let total = total
        .checked_add(next)
        .context("delta value budget overflow")?;
    if total > MAX_TOTAL_PAYLOAD_BYTES_V1 {
        bail!("delta reconstructed values exceed the 64 MiB local budget");
    }
    Ok(total)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Put,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PathChange {
    path: Vec<String>,
    op: Operation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Witness {
    paths: Vec<PathChange>,
}

fn parts(change: &RawPathChangeV1) -> (&[String], Operation) {
    match change {
        RawPathChangeV1::Put { path, .. } => (path, Operation::Put),
        RawPathChangeV1::Delete { path } => (path, Operation::Delete),
    }
}

/// Only a writer may decline the optional V3 encoding before reservation.
/// Invalid ordering/paths remain errors, never an implicit legacy fallback.
pub(super) fn writer_budget_allows(changes: &[RawPathChangeV1]) -> Result<bool> {
    let mut bytes = 0usize;
    let mut path_bytes = 0usize;
    let mut previous: Option<&[String]> = None;
    if changes.is_empty() {
        bail!("delta witness cannot be empty");
    }
    for change in changes {
        let (path, op) = parts(change);
        if path.len() > 4 || previous.is_some_and(|old| old >= path) {
            bail!("delta witness paths must be unique, sorted and within layout depth");
        }
        path_bytes = path_bytes
            .checked_add(
                serde_json::to_vec(&PathChange {
                    path: path.to_vec(),
                    op,
                })?
                .len(),
            )
            .context("delta path budget overflow")?;
        previous = Some(path);
        if let RawPathChangeV1::Put { value, .. } = change {
            if value.len() > crate::native_state_records::MAX_RECORD_VALUE_BYTES_V1 {
                bail!("delta value exceeds the existing per-record budget");
            }
            bytes = bytes
                .checked_add(value.len())
                .context("delta value budget overflow")?;
        }
    }
    Ok(within_writer_budget(bytes, path_bytes, changes.len()))
}

impl Witness {
    pub(super) fn from_changes(changes: &[RawPathChangeV1]) -> Result<Self> {
        let result = Self {
            paths: changes
                .iter()
                .map(|change| {
                    let (path, op) = parts(change);
                    PathChange {
                        path: path.to_vec(),
                        op,
                    }
                })
                .collect(),
        };
        result.validate()?;
        Ok(result)
    }

    pub(super) fn validate(&self) -> Result<()> {
        if self.paths.is_empty() || self.paths.len() > MAX_PATHS {
            bail!("delta witness path count exceeds document budget");
        }
        let mut previous: Option<&[String]> = None;
        let mut path_bytes = 0usize;
        for entry in &self.paths {
            if entry.path.len() > 4 || previous.is_some_and(|old| old >= entry.path.as_slice()) {
                bail!("delta witness paths must be unique, sorted and within layout depth");
            }
            path_bytes = path_bytes
                .checked_add(serde_json::to_vec(entry)?.len())
                .context("delta path budget overflow")?;
            if path_bytes > MAX_PAYLOAD_BYTES_V1 {
                bail!("delta witness paths exceed document byte budget");
            }
            previous = Some(&entry.path);
        }
        Ok(())
    }
}

fn read_raw(
    reader: &dyn StateRecordReader,
    root: NodeHash,
    path: &[String],
) -> Result<Option<Vec<u8>>> {
    let key = native_store_records::key(path)?;
    let Some(value) = read_record(reader, root, &key)? else {
        return Ok(None);
    };
    let (stored, raw) = native_store_records::unpack(&key, &value)?;
    if stored != path {
        bail!("delta point-read path differs from its authenticated record");
    }
    if native_store_records::is_object_path_v1(path)? {
        if raw != native_store_records::OBJECT {
            bail!("delta structural record marker mismatch");
        }
        Ok(Some(b"{}".to_vec()))
    } else {
        let token: Box<RawValue> = serde_json::from_slice(raw)?;
        if token.get().as_bytes() != raw {
            bail!("delta record JSON has noncanonical outer whitespace");
        }
        Ok(Some(raw.to_vec()))
    }
}

/// Reconstruct only named post-state records; missing nodes/chunks are errors
/// from the authenticated record reader, not proof of a missing account.
fn reconstruct(
    parent_reader: &dyn StateRecordReader,
    post_reader: &dyn StateRecordReader,
    parent_root: NodeHash,
    post_root: NodeHash,
    witness: &Witness,
) -> Result<Vec<RawPathChangeV1>> {
    witness.validate()?;
    let mut changes = Vec::with_capacity(witness.paths.len());
    let mut bytes = 0usize;
    for entry in &witness.paths {
        let before = read_raw(parent_reader, parent_root, &entry.path)?;
        let after = read_raw(post_reader, post_root, &entry.path)?;
        if before == after {
            bail!("delta witness contains a redundant or nonexistent change");
        }
        let change = match (entry.op, after) {
            (Operation::Put, Some(value)) => {
                bytes = add_reconstructed_bytes(bytes, value.len())?;
                RawPathChangeV1::Put {
                    path: entry.path.clone(),
                    value,
                }
            }
            (Operation::Delete, None) => RawPathChangeV1::Delete {
                path: entry.path.clone(),
            },
            _ => bail!("delta operation disagrees with authenticated post-state presence"),
        };
        changes.push(change);
    }
    Ok(changes)
}

pub(super) fn verify(
    parent_reader: &dyn StateRecordReader,
    post_reader: &dyn StateRecordReader,
    parent: &StoreRef,
    output: &StoreRef,
    witness: &Witness,
) -> Result<Vec<RawPathChangeV1>> {
    let (physical_root, state_root, receipt_root, records, blob_bytes) = parent
        .rooted_parts()?
        .context("delta witness requires a verified three-root parent")?;
    output
        .rooted_parts()?
        .context("delta output root bundle missing")?;
    let bundle = output
        .bundle
        .as_ref()
        .context("delta output bundle missing")?;
    if [
        bundle.physical.parent_root,
        bundle.state.parent_root,
        bundle.receipts.parent_root,
    ] != [physical_root, state_root, receipt_root]
    {
        bail!("delta witness parent roots differ from the verified parent");
    }
    let changes = reconstruct(
        parent_reader,
        post_reader,
        physical_root,
        output.root,
        witness,
    )?;
    let mut physical = RecordOverlayV1::new(parent_reader, physical_root);
    let mut state = RecordOverlayV1::new(parent_reader, state_root);
    let mut receipts = RecordOverlayV1::new(parent_reader, receipt_root);
    let mut totals = (records, blob_bytes);
    let mut start = 0;
    while start < changes.len() {
        let mut end = start;
        let mut bytes = 0usize;
        while end < changes.len() && end - start < STAGE_PATHS {
            let (path, _) = parts(&changes[end]);
            let length = serde_json::to_vec(path)?.len()
                + match &changes[end] {
                    RawPathChangeV1::Put { value, .. } => value.len(),
                    RawPathChangeV1::Delete { .. } => 0,
                };
            if end > start && bytes.saturating_add(length) > STAGE_BYTES {
                break;
            }
            bytes = bytes
                .checked_add(length)
                .context("delta stage size overflow")?;
            end += 1;
        }
        let batch = &changes[start..end];
        totals = native_store_records::apply_raw_changes_to_overlay_v1(&mut physical, batch)?
            .checked_apply(totals.0, totals.1)?;
        native_record_commitment::apply_consensus_changes_v1(&mut state, batch)?;
        for change in batch {
            let (path, _) = parts(change);
            if path.first().map(String::as_str) != Some("receipts") {
                continue;
            }
            let RawPathChangeV1::Put { value, .. } = change else {
                bail!("delta cumulative receipts cannot be deleted");
            };
            if path.len() != 2 {
                bail!("delta receipts require a single canonical transaction key");
            }
            if read_raw(parent_reader, physical_root, path)?.is_some() {
                bail!("delta cannot replace an existing physical receipt");
            }
            let receipt: NovNativeExecutionReceiptV1 = serde_json::from_slice(value)?;
            if receipt.tx_hash != path[1] {
                bail!("delta receipt value differs from its transaction path");
            }
            let RecordChange::Put { key, value } =
                native_record_commitment::receipt_change_v1(&receipt)?
            else {
                unreachable!();
            };
            if read_record(parent_reader, receipt_root, &key)?.is_some() {
                bail!("delta cannot replace an existing cumulative receipt");
            }
            receipts.stage(&[RecordChange::Put { key, value }])?;
        }
        start = end;
    }
    if physical.root() != bundle.physical.root
        || state.root() != bundle.state.root
        || receipts.root() != bundle.receipts.root
        || totals != (output.records, output.blob_bytes)
    {
        bail!("delta witness does not reproduce exact output roots and record statistics");
    }
    Ok(changes)
}

pub(super) fn document_commitment(bytes: &[u8]) -> NodeHash {
    sha256_bytes_v1(&[b"novovm-candidate-record-document-v3\0", bytes])
}

pub(super) fn prepared_id(commitment: NodeHash, role: &[u8]) -> NodeHash {
    sha256_bytes_v1(&[b"novovm-candidate-record-prepared-v3\0", role, &commitment])
}

pub(in super::super) struct VerifiedDeltaDocument<T> {
    pub(in super::super) inline: T,
    pub(in super::super) state: StoreRef,
    pub(in super::super) changes: Vec<RawPathChangeV1>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_state_records::{stage_record_update, RECORD_CHUNK_BYTES_V1};

    #[derive(Default, Clone)]
    struct Memory {
        nodes: BTreeMap<NodeHash, Vec<u8>>,
        blobs: BTreeMap<NodeHash, Vec<u8>>,
    }
    impl StateNodeReader for Memory {
        fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
            Ok(self.nodes.get(hash).cloned())
        }
    }
    impl StateRecordReader for Memory {
        fn read_record_chunk(&self, hash: NodeHash, index: u32) -> Result<Option<Vec<u8>>> {
            Ok(self
                .blobs
                .get(&hash)
                .and_then(|bytes| bytes.chunks(RECORD_CHUNK_BYTES_V1).nth(index as usize))
                .map(<[u8]>::to_vec))
        }
    }
    impl Memory {
        fn absorb(&mut self, update: &StagedRecordUpdate) {
            self.nodes.extend(update.nodes().clone());
            self.blobs.extend(update.blobs().clone());
        }
    }
    fn reference(
        memory: &mut Memory,
        store: &NovNativeExecutionStoreV1,
        parent: Option<&StoreRef>,
    ) -> StoreRef {
        let records = native_store_records::encode(store).unwrap();
        let count = records.len();
        let blob_bytes = records
            .iter()
            .map(|(key, value)| 10 + key.len() + value.len())
            .sum();
        let physical = stage_record_update(
            memory,
            empty_root(),
            &records
                .into_iter()
                .map(|(key, value)| RecordChange::Put { key, value })
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let state =
            native_record_commitment::stage_consensus_import_v1(memory, &store.module_state)
                .unwrap();
        let receipts = native_record_commitment::stage_receipt_import_v1(memory, store).unwrap();
        for update in [&physical, &state, &receipts] {
            memory.absorb(update);
        }
        let parents = parent
            .map(|reference| {
                let (p, s, r, _, _) = reference.rooted_parts().unwrap().unwrap();
                [p, s, r]
            })
            .unwrap_or([empty_root(); 3]);
        let profile = crate::native_root_codecs::NativeRootCodecProfileV1::RecordTreeV1;
        StoreRef {
            layout: STORE_CODEC.into(),
            tree_codec: STATE_RECORD_CODEC_V1.into(),
            root: physical.root(),
            records: count,
            blob_bytes,
            bundle: Some(RootBundle {
                schema: "novovm-candidate-record-root-bundle/v1".into(),
                state_codec: profile.state_root_codec().into(),
                receipt_codec: profile.receipt_root_codec().into(),
                physical: RootLink {
                    parent_root: parents[0],
                    root: physical.root(),
                },
                state: RootLink {
                    parent_root: parents[1],
                    root: state.root(),
                },
                receipts: RootLink {
                    parent_root: parents[2],
                    root: receipts.root(),
                },
            }),
        }
    }
    fn receipt(byte: u8) -> NovNativeExecutionReceiptV1 {
        serde_json::from_value(serde_json::json!({
            "tx_hash":to_hex(&[byte;32]),"status":true,"target":"native","module":"native_asset",
            "method":"transfer","settled_fee_nov":0,"paid_asset":"NOV","paid_amount":0,
            "logs":[],"failure_reason":null,"fee_contract":"unified"
        }))
        .unwrap()
    }
    fn put<T: Serialize>(path: &[&str], value: &T) -> RawPathChangeV1 {
        RawPathChangeV1::Put {
            path: path.iter().map(|part| (*part).to_owned()).collect(),
            value: native_store_records::typed_raw_v1(value).unwrap(),
        }
    }
    fn fixture() -> (Memory, StoreRef, NovNativeExecutionStoreV1) {
        let mut store = NovNativeExecutionStoreV1::default();
        store.module_state.account_asset_balances.insert(
            "alice".into(),
            BTreeMap::from([("NOV".into(), u128::MAX), ("USDT".into(), 55)]),
        );
        let old = receipt(1);
        store.receipts.insert(old.tx_hash.clone(), old);
        let mut memory = Memory::default();
        let parent = reference(&mut memory, &store, None);
        (memory, parent, store)
    }
    fn transition(store: &mut NovNativeExecutionStoreV1) -> Vec<RawPathChangeV1> {
        store
            .module_state
            .account_asset_balances
            .get_mut("alice")
            .unwrap()
            .insert("NOV".into(), u128::MAX - 7);
        store
            .module_state
            .native_auth_next_nonces
            .insert("alice".into(), 1);
        let new = receipt(2);
        store.receipts.insert(new.tx_hash.clone(), new.clone());
        vec![
            put(
                &["module_state", "account_asset_balances", "alice", "NOV"],
                &(u128::MAX - 7),
            ),
            put(&["module_state", "native_auth_next_nonces", "alice"], &1u64),
            put(&["receipts", &new.tx_hash], &new),
        ]
    }

    #[test]
    fn record_delta_point_replay_matches_roots_and_rejects_bad_parent_stats_and_changes() {
        let (mut memory, parent, mut store) = fixture();
        let changes = transition(&mut store);
        let output = reference(&mut memory, &store, Some(&parent));
        let witness = Witness::from_changes(&changes).unwrap();
        assert_eq!(
            verify(&memory, &memory, &parent, &output, &witness).unwrap(),
            changes
        );
        let mut bad = output.clone();
        bad.bundle.as_mut().unwrap().state.parent_root[0] ^= 1;
        assert!(verify(&memory, &memory, &parent, &bad, &witness).is_err());
        for fault in 0..2 {
            let mut bad = output.clone();
            if fault == 0 {
                bad.records += 1;
            } else {
                bad.blob_bytes += 1;
            }
            assert!(verify(&memory, &memory, &parent, &bad, &witness).is_err());
        }
        let mut missing = witness.clone();
        missing.paths.remove(0);
        assert!(verify(&memory, &memory, &parent, &output, &missing).is_err());
        let mut extra = changes.clone();
        extra.push(put(
            &["module_state", "account_asset_balances", "alice", "USDT"],
            &55u128,
        ));
        extra.sort_by(|a, b| parts(a).0.cmp(parts(b).0));
        assert!(verify(
            &memory,
            &memory,
            &parent,
            &output,
            &Witness::from_changes(&extra).unwrap()
        )
        .unwrap_err()
        .to_string()
        .contains("redundant"));
        let mut wrong_op = witness.clone();
        wrong_op.paths[0].op = Operation::Delete;
        assert!(verify(&memory, &memory, &parent, &output, &wrong_op).is_err());
        let key = native_store_records::key(parts(&changes[0]).0).unwrap();
        let blob = *memory
            .blobs
            .iter()
            .find(|(_, bytes)| {
                let len = usize::from(u16::from_be_bytes(bytes[4..6].try_into().unwrap()));
                bytes.get(10..10 + len) == Some(key.as_slice())
            })
            .unwrap()
            .0;
        memory.blobs.remove(&blob);
        assert!(verify(&memory, &memory, &parent, &output, &witness)
            .unwrap_err()
            .to_string()
            .contains("chunk missing"));
    }

    #[test]
    fn record_delta_rejects_receipt_replacement_and_deletion() {
        for deletion in [false, true] {
            let (mut memory, parent, mut store) = fixture();
            let hash = to_hex(&[1; 32]);
            let change = if deletion {
                store.receipts.remove(&hash);
                RawPathChangeV1::Delete {
                    path: vec!["receipts".into(), hash.clone()],
                }
            } else {
                let receipt = store.receipts.get_mut(&hash).unwrap();
                receipt.paid_amount = 1;
                put(&["receipts", &hash], receipt)
            };
            let output = reference(&mut memory, &store, Some(&parent));
            let witness = Witness::from_changes(&[change]).unwrap();
            let error = verify(&memory, &memory, &parent, &output, &witness).unwrap_err();
            assert!(
                error.to_string().contains(if deletion {
                    "cannot be deleted"
                } else {
                    "cannot replace"
                }),
                "{error:#}"
            );
        }
    }

    #[test]
    fn record_delta_witness_order_domains_and_budgets_are_closed() {
        let mut changes = vec![
            put(&["module_state", "a"], &1),
            put(&["module_state", "b"], &2),
        ];
        assert!(Witness::from_changes(&changes).is_ok());
        changes.reverse();
        assert!(Witness::from_changes(&changes).is_err());
        changes[1] = changes[0].clone();
        assert!(Witness::from_changes(&changes).is_err());
        assert!(Witness::from_changes(&[]).is_err());
        assert!(Witness::from_changes(&[put(&["a", "b", "c", "d", "e"], &1)]).is_err());
        let changes = (0..4097)
            .map(|index| {
                put(
                    &[
                        "module_state",
                        "native_auth_next_nonces",
                        &format!("{index:06}"),
                    ],
                    &1u64,
                )
            })
            .collect::<Vec<_>>();
        assert!(
            writer_budget_allows(&changes).unwrap(),
            "4096 is only one-stage limit"
        );
        assert!(Witness::from_changes(&changes).is_ok());
        assert!(within_writer_budget(
            MAX_TOTAL_PAYLOAD_BYTES_V1,
            MAX_PAYLOAD_BYTES_V1,
            MAX_PATHS
        ));
        assert!(!within_writer_budget(MAX_TOTAL_PAYLOAD_BYTES_V1 + 1, 1, 1));
        assert!(!within_writer_budget(1, MAX_PAYLOAD_BYTES_V1 + 1, 1));
        assert!(add_reconstructed_bytes(MAX_TOTAL_PAYLOAD_BYTES_V1, 1).is_err());
        assert!(add_reconstructed_bytes(usize::MAX, 1).is_err());
        assert_ne!(
            document_commitment(b"same"),
            record_document_commitment(b"same")
        );
        assert_ne!(
            prepared_id([1; 32], b"state"),
            prepared_role_id([1; 32], b"state")
        );
    }
}
