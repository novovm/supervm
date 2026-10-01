//! Candidate document storage. A physical record root replaces its embedded
//! typed store, without changing the existing consensus roots or authority.
//! The compatibility reader still materializes and validates the full state.

use super::*;
use crate::native_state_records::{
    visit_records, RecordChange, RecordOverlayV1, RecordScanBudget, StagedRecordUpdate,
    STATE_RECORD_CODEC_V1,
};
use crate::native_state_storage::{AoemStateNodesV1, AoemStateReaderV1};
use crate::native_state_tree::empty_root;
use serde_json::value::RawValue;

const DOCUMENT_SCHEMA: &str = "novovm-candidate-record-document/v1";
const STORE_CODEC: &str = "novovm-native-store-record-layout/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoreRef {
    layout: String,
    tree_codec: String,
    root: [u8; 32],
    records: usize,
    blob_bytes: usize,
}

impl StoreRef {
    fn budget(&self) -> Result<RecordScanBudget> {
        if self.layout != STORE_CODEC
            || self.tree_codec != STATE_RECORD_CODEC_V1
            || self.records == 0
        {
            bail!("candidate record store codec/count mismatch");
        }
        Ok(RecordScanBudget {
            max_nodes: self
                .records
                .checked_mul(2)
                .context("record node count overflow")?,
            max_records: self.records,
            max_bytes: self.blob_bytes,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema: String,
    store_path: Vec<String>,
    state: StoreRef,
    inline: Box<RawValue>,
}

pub(super) struct PreparedDocument {
    pub(super) bytes: Vec<u8>,
    state: StoreRef,
    update: StagedRecordUpdate,
}

/// Replace exactly one complete store field; never traverse through a missing
/// parent, accept a non-null placeholder on decode, or pass through Value/f64.
fn replace(
    raw: &RawValue,
    path: &[&str],
    value: Box<RawValue>,
) -> Result<(Box<RawValue>, Box<RawValue>)> {
    let (field, rest) = path.split_first().context("empty document store path")?;
    let mut object: BTreeMap<String, Box<RawValue>> = serde_json::from_str(raw.get())?;
    let previous = object
        .remove(*field)
        .context("candidate document store field missing")?;
    let (replacement, removed) = if rest.is_empty() {
        (value, previous)
    } else {
        replace(&previous, rest, value)?
    };
    object.insert((*field).to_owned(), replacement);
    Ok((serde_json::value::to_raw_value(&object)?, removed))
}

pub(super) fn prepare<T: Serialize>(
    workspace: &WorkspaceStore,
    document: &T,
    store_path: &[&str],
    store: &NovNativeExecutionStoreV1,
    base: Option<(&StoreRef, &NovNativeExecutionStoreV1)>,
) -> Result<PreparedDocument> {
    let records = native_store_records::encode(store)?;
    let (parent_root, before) = if let Some((reference, parent)) = base {
        reference.budget()?;
        (reference.root, native_store_records::encode(parent)?)
    } else {
        (empty_root(), BTreeMap::new())
    };
    let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
    let mut overlay = RecordOverlayV1::new(&reader, parent_root);
    let mut changes = Vec::new();
    // Bound one staging call, not the accumulated ledger. Large individual
    // values are legal up to the record codec's independently checked cap.
    let mut stage_bytes = 0usize;
    let mut blob_bytes = 0usize;
    for (key, value) in &records {
        blob_bytes = blob_bytes
            .checked_add(10 + key.len() + value.len())
            .context("record byte count overflow")?;
        if before.get(key) == Some(value) {
            continue;
        }
        if changes.len() >= 128 || stage_bytes + key.len() + value.len() > 8 * 1024 * 1024 {
            overlay.stage(&changes)?;
            changes.clear();
            stage_bytes = 0;
        }
        stage_bytes += 10 + key.len() + value.len();
        changes.push(RecordChange::Put {
            key: key.clone(),
            value: value.clone(),
        });
    }
    for key in before.keys().filter(|key| !records.contains_key(*key)) {
        if changes.len() >= 128 {
            overlay.stage(&changes)?;
            changes.clear();
        }
        changes.push(RecordChange::Delete { key: key.clone() });
    }
    overlay.stage(&changes)?;
    let update = overlay.finish();
    let state = StoreRef {
        layout: STORE_CODEC.into(),
        tree_codec: STATE_RECORD_CODEC_V1.into(),
        root: update.root(),
        records: records.len(),
        blob_bytes,
    };
    let raw = serde_json::value::to_raw_value(document)?;
    let (inline, removed) = replace(&raw, store_path, RawValue::from_string("null".into())?)?;
    let actual: NovNativeExecutionStoreV1 = serde_json::from_str(removed.get())?;
    if actual != *store {
        bail!("candidate record document does not contain the supplied store");
    }
    let bytes = serde_json::to_vec(&Document {
        schema: DOCUMENT_SCHEMA.into(),
        store_path: store_path.iter().map(|part| (*part).to_owned()).collect(),
        state: state.clone(),
        inline,
    })?;
    Ok(PreparedDocument {
        bytes,
        state,
        update,
    })
}

pub(super) fn persist(workspace: &WorkspaceStore, prepared: &PreparedDocument) -> Result<()> {
    let commitment = sha256_bytes_v1(&[b"novovm-candidate-record-document-v1\0", &prepared.bytes]);
    let candidate = sha256_bytes_v1(&[
        b"novovm-candidate-record-prepared-v1\0",
        &prepared.update.parent_root(),
        &prepared.state.root,
        &commitment,
    ]);
    let mut storage = AoemStateNodesV1::new(&workspace.graph, workspace.scope)?;
    storage.persist_record_candidate(
        candidate,
        commitment,
        prepared.state.root,
        &prepared.update,
    )?;
    // Full typed recovery remains part of this compatibility slice. It also
    // checks every inherited blob before a candidate may publish completion.
    read_store(workspace, &prepared.state)?;
    Ok(())
}

fn read_store(
    workspace: &WorkspaceStore,
    reference: &StoreRef,
) -> Result<NovNativeExecutionStoreV1> {
    let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
    let mut records = BTreeMap::new();
    let stats = visit_records(
        &reader,
        reference.root,
        reference.budget()?,
        &mut |key, value| {
            if records.insert(key.to_vec(), value.to_vec()).is_some() {
                bail!("duplicate native store record");
            }
            Ok(())
        },
    )?;
    if stats.records != reference.records || stats.bytes != reference.blob_bytes {
        bail!("candidate store record count/bytes mismatch");
    }
    native_store_records::decode(records)
}

pub(super) fn decode<T: serde::de::DeserializeOwned>(
    workspace: &WorkspaceStore,
    bytes: &[u8],
    expected_path: &[&str],
) -> Result<(T, Option<StoreRef>)> {
    let fields: BTreeMap<String, Box<RawValue>> = serde_json::from_slice(bytes)?;
    let schema: String = serde_json::from_str(
        fields
            .get("schema")
            .context("candidate document schema missing")?
            .get(),
    )?;
    if schema != DOCUMENT_SCHEMA {
        // Existing inline images remain readable; the enclosing Payload/Output
        // schema and exact existing validation still decide their admissibility.
        return Ok((serde_json::from_slice(bytes)?, None));
    }
    let document: Document = serde_json::from_slice(bytes)?;
    if document
        .store_path
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        != expected_path
    {
        bail!("candidate document store path mismatch");
    }
    let store = read_store(workspace, &document.state)?;
    let (raw, placeholder) = replace(
        &document.inline,
        expected_path,
        serde_json::value::to_raw_value(&store)?,
    )?;
    if placeholder.get() != "null" {
        bail!("candidate document contains an ambiguous inline store");
    }
    Ok((serde_json::from_str(raw.get())?, Some(document.state)))
}

/// Determine the sole parent variant without trusting a caller-supplied path.
pub(super) fn payload_path(payload: &Payload) -> Result<[&'static str; 2]> {
    payload.parent_store()?;
    let field = if payload.finalized_parent.is_some() {
        "finalized_parent"
    } else if payload.genesis.is_some() {
        "genesis"
    } else {
        "parent_snapshot"
    };
    Ok([field, "store"])
}

pub(super) fn decode_payload(workspace: &WorkspaceStore, bytes: &[u8]) -> Result<Payload> {
    let fields: BTreeMap<String, Box<RawValue>> = serde_json::from_slice(bytes)?;
    let schema: String = serde_json::from_str(
        fields
            .get("schema")
            .context("candidate document schema missing")?
            .get(),
    )?;
    if schema != DOCUMENT_SCHEMA {
        return Ok(serde_json::from_slice(bytes)?);
    }
    let doc: Document = serde_json::from_slice(bytes)?;
    let parent = doc
        .store_path
        .first()
        .context("candidate parent path missing")?;
    if !matches!(
        parent.as_str(),
        "finalized_parent" | "genesis" | "parent_snapshot"
    ) {
        bail!("candidate document parent variant mismatch");
    }
    let (mut payload, reference): (Payload, _) = decode(workspace, bytes, &[parent, "store"])?;
    if payload_path(&payload)?[0] != parent {
        bail!("candidate document references a different parent variant");
    }
    payload.record_state = reference;
    Ok(payload)
}

#[cfg(test)]
pub(crate) fn exercise_record_document_storage_for_test(
    chain: u64,
    params: &serde_json::Value,
) -> Result<()> {
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TestDocument {
        schema: String,
        store: NovNativeExecutionStoreV1,
    }
    let workspace = WorkspaceStore::open(chain, params)?;
    let head_key = native_aoem_owned_state_head_key_v1(chain, &workspace.namespace);
    let authority_before = workspace.graph.get(&head_key)?;
    let mut store = NovNativeExecutionStoreV1 {
        authority_chain_id: Some(chain),
        authority_namespace_digest: workspace.namespace.clone(),
        ..Default::default()
    };
    store.module_state.protocol_config_commitment = to_hex(&workspace.protocol);
    store.module_state.account_asset_balances.insert(
        "test-account".into(),
        BTreeMap::from([("NOV".into(), u128::MAX), ("USDT".into(), u128::MAX - 1)]),
    );
    // Synthetic large history verifies physical storage capacity, not finalized
    // throughput. Every entry is smaller than the explicit per-record bound.
    for id in 0..13 {
        store
            .module_state
            .governance_proposals
            .insert(id, serde_json::Value::String("x".repeat(700_000)));
    }
    let inline_len = serde_json::to_vec(&store)?.len();
    assert!(inline_len > MAX_PAYLOAD_BYTES_V1);
    let initial = TestDocument {
        schema: "record-storage-test/v1".into(),
        store,
    };
    let prepared = prepare(&workspace, &initial, &["store"], &initial.store, None)?;
    assert!(prepared.bytes.len() < 2048);
    persist(&workspace, &prepared)?;
    let mut next_store = initial.store.clone();
    next_store
        .module_state
        .account_asset_balances
        .get_mut("test-account")
        .unwrap()
        .insert("NOV".into(), u128::MAX - 7);
    let next = TestDocument {
        schema: initial.schema.clone(),
        store: next_store,
    };
    let updated = prepare(
        &workspace,
        &next,
        &["store"],
        &next.store,
        Some((&prepared.state, &initial.store)),
    )?;
    assert_eq!(
        updated.update.blobs().len(),
        1,
        "one balance changed; no historical blobs rewritten"
    );
    assert!(updated.update.nodes().len() <= 257);
    let staged_bytes: usize = updated.update.blobs().values().map(Vec::len).sum();
    assert!(staged_bytes < 512);
    persist(&workspace, &updated)?;
    assert_eq!(workspace.graph.get(&head_key)?, authority_before);
    drop(workspace);
    let reopened = WorkspaceStore::open(chain, params)?;
    let (old, _): (TestDocument, _) = decode(&reopened, &prepared.bytes, &["store"])?;
    let (new, _): (TestDocument, _) = decode(&reopened, &updated.bytes, &["store"])?;
    assert_eq!(old.store, initial.store);
    assert_eq!(new.store, next.store);
    assert_eq!(
        new.store.module_state.account_asset_balances["test-account"]["USDT"],
        u128::MAX - 1
    );
    let mut bad: Document = serde_json::from_slice(&updated.bytes)?;
    bad.state.root[0] ^= 1;
    assert!(decode::<TestDocument>(&reopened, &serde_json::to_vec(&bad)?, &["store"]).is_err());
    let mut bad: Document = serde_json::from_slice(&updated.bytes)?;
    bad.state.records += 1;
    assert!(decode::<TestDocument>(&reopened, &serde_json::to_vec(&bad)?, &["store"]).is_err());
    assert!(decode::<TestDocument>(&reopened, &updated.bytes, &["different"]).is_err());
    assert_eq!(reopened.graph.get(&head_key)?, authority_before);
    eprintln!("record document storage: full_image_bytes={inline_len}, document_bytes={}, changed_blobs=1, changed_blob_bytes={staged_bytes}, changed_nodes={}, reopened=true, authority_unchanged=true", updated.bytes.len(), updated.update.nodes().len());
    Ok(())
}
