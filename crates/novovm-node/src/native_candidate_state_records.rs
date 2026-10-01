//! Candidate document storage. V1 preserves the original physical-only image;
//! V2 additionally binds independently projected consensus state/receipt roots.
//! Both use the same AOEM database and the existing authority publication path.
//! The compatibility reader still materializes and validates the full state.

use super::*;
use crate::native_state_records::{
    visit_records, RecordChange, RecordOverlayV1, RecordScanBudget, StagedRecordUpdate,
    StateRecordReader, RECORD_CHUNK_BYTES_V1, STATE_RECORD_CODEC_V1,
};
use crate::native_state_storage::{AoemStateNodesV1, AoemStateReaderV1};
use crate::native_state_tree::{empty_root, validate_state_node_bytes, NodeHash, StateNodeReader};
use serde_json::value::RawValue;

const DOCUMENT_SCHEMA: &str = "novovm-candidate-record-document/v1";
const RECORD_DOCUMENT_SCHEMA: &str = "novovm-candidate-record-document/v2";
const STORE_CODEC: &str = "novovm-native-store-record-layout/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RootLink {
    parent_root: NodeHash,
    root: NodeHash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RootBundle {
    schema: String,
    state_codec: String,
    receipt_codec: String,
    physical: RootLink,
    state: RootLink,
    receipts: RootLink,
}

impl RootBundle {
    fn validate(&self, physical_root: NodeHash) -> Result<()> {
        use crate::native_root_codecs::NativeRootCodecProfileV1;
        if self.schema != "novovm-candidate-record-root-bundle/v1"
            || self.physical.root != physical_root
            || NativeRootCodecProfileV1::from_root_codecs(&self.state_codec, &self.receipt_codec)?
                != NativeRootCodecProfileV1::RecordTreeV1
        {
            bail!("candidate record root bundle codec/physical binding mismatch");
        }
        Ok(())
    }

    fn commitment(&self) -> Result<NodeHash> {
        Ok(sha256_bytes_v1(&[
            b"novovm-candidate-record-root-bundle-v1\0",
            &serde_json::to_vec(self)?,
        ]))
    }

    fn links(&self) -> [(&'static [u8], &RootLink); 3] {
        [
            (b"physical", &self.physical),
            (b"state", &self.state),
            (b"receipts", &self.receipts),
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoreRef {
    layout: String,
    tree_codec: String,
    root: [u8; 32],
    records: usize,
    blob_bytes: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bundle: Option<RootBundle>,
}

impl StoreRef {
    /// Structurally checked roots, not an authority grant. The reference must
    /// come from a document whose markers, projection and parent were verified.
    #[allow(clippy::type_complexity)]
    pub(super) fn rooted_parts(
        &self,
    ) -> Result<Option<(NodeHash, NodeHash, NodeHash, usize, usize)>> {
        self.budget()?;
        self.bundle
            .as_ref()
            .map(|bundle| {
                bundle.validate(self.root)?;
                Ok((
                    self.root,
                    bundle.state.root,
                    bundle.receipts.root,
                    self.records,
                    self.blob_bytes,
                ))
            })
            .transpose()
    }

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
    consensus_updates: Option<(StagedRecordUpdate, StagedRecordUpdate)>,
}

pub(super) struct RecordTreeUpdatesV1 {
    pub(super) physical: StagedRecordUpdate,
    pub(super) state: StagedRecordUpdate,
    pub(super) receipts: StagedRecordUpdate,
    pub(super) records: usize,
    pub(super) blob_bytes: usize,
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
        bundle: None,
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
        consensus_updates: None,
    })
}

/// Serialize a new three-root document. This is a cold verification boundary,
/// not permission to trust a caller-provided root or a second authority head.
pub(super) fn prepare_record_profile<T: Serialize>(
    workspace: &WorkspaceStore,
    document: &T,
    store_path: &[&str],
    store: &NovNativeExecutionStoreV1,
    base: Option<(&StoreRef, &NovNativeExecutionStoreV1)>,
    updates: Option<RecordTreeUpdatesV1>,
) -> Result<PreparedDocument> {
    let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
    let mut parents = [empty_root(); 3];
    if let Some((reference, parent)) = base {
        if read_store(workspace, reference)? != *parent {
            bail!("candidate record base does not match its complete typed state");
        }
        parents[0] = reference.root;
        if let Some((_, state, receipts, _, _)) = reference.rooted_parts()? {
            parents[1] = state;
            parents[2] = receipts;
        }
    }
    let updates = match updates {
        Some(updates) => updates,
        None => {
            // Preserve the original physical diff implementation and its exact
            // encoding. The resulting v1 document itself is never persisted here.
            let physical = prepare(workspace, document, store_path, store, base)?;
            let (state, receipts) = if let Some((reference, parent)) =
                base.filter(|(reference, _)| reference.bundle.is_some())
            {
                if store == parent {
                    (
                        RecordOverlayV1::new(&reader, parents[1]).finish(),
                        RecordOverlayV1::new(&reader, parents[2]).finish(),
                    )
                } else {
                    reference.rooted_parts()?;
                    (
                        cold_diff(
                            &reader,
                            parents[1],
                            &native_record_commitment::consensus_records_v1(&parent.module_state)?,
                            &native_record_commitment::consensus_records_v1(&store.module_state)?,
                        )?,
                        cold_diff(
                            &reader,
                            parents[2],
                            &receipt_records(parent)?,
                            &receipt_records(store)?,
                        )?,
                    )
                }
            } else {
                (
                    native_record_commitment::stage_consensus_import_v1(
                        &reader,
                        &store.module_state,
                    )?,
                    native_record_commitment::stage_receipt_import_v1(&reader, store)?,
                )
            };
            RecordTreeUpdatesV1 {
                physical: physical.update,
                state,
                receipts,
                records: physical.state.records,
                blob_bytes: physical.state.blob_bytes,
            }
        }
    };
    for (update, parent) in [&updates.physical, &updates.state, &updates.receipts]
        .into_iter()
        .zip(parents)
    {
        if update.parent_root() != parent {
            bail!("candidate three-root update differs from the captured parent roots");
        }
    }
    let link = |update: &StagedRecordUpdate| RootLink {
        parent_root: update.parent_root(),
        root: update.root(),
    };
    let profile = crate::native_root_codecs::NativeRootCodecProfileV1::RecordTreeV1;
    let state = StoreRef {
        layout: STORE_CODEC.into(),
        tree_codec: STATE_RECORD_CODEC_V1.into(),
        root: updates.physical.root(),
        records: updates.records,
        blob_bytes: updates.blob_bytes,
        bundle: Some(RootBundle {
            schema: "novovm-candidate-record-root-bundle/v1".into(),
            state_codec: profile.state_root_codec().into(),
            receipt_codec: profile.receipt_root_codec().into(),
            physical: link(&updates.physical),
            state: link(&updates.state),
            receipts: link(&updates.receipts),
        }),
    };
    // Do not re-encode the complete physical store on the incremental path.
    // Traverse staged+inherited records instead, retaining exact u128 tokens.
    let staged = StagedReader {
        base: &reader,
        update: &updates.physical,
    };
    if read_store_from(&staged, &state)? != *store {
        bail!("candidate physical update differs from supplied typed state");
    }
    validate_projection(&state, store)?;
    let raw = serde_json::value::to_raw_value(document)?;
    let (inline, removed) = replace(&raw, store_path, RawValue::from_string("null".into())?)?;
    let actual: NovNativeExecutionStoreV1 = serde_json::from_str(removed.get())?;
    if actual != *store {
        bail!("candidate record document does not contain the supplied store");
    }
    let bytes = serde_json::to_vec(&Document {
        schema: RECORD_DOCUMENT_SCHEMA.into(),
        store_path: store_path.iter().map(|part| (*part).to_owned()).collect(),
        state: state.clone(),
        inline,
    })?;
    Ok(PreparedDocument {
        bytes,
        state,
        update: updates.physical,
        consensus_updates: Some((updates.state, updates.receipts)),
    })
}

fn receipt_records(store: &NovNativeExecutionStoreV1) -> Result<BTreeMap<Vec<u8>, Vec<u8>>> {
    let mut result = BTreeMap::new();
    for (hash, receipt) in &store.receipts {
        if hash != &receipt.tx_hash {
            bail!("candidate receipt map key differs from its transaction hash");
        }
        let RecordChange::Put { key, value } =
            native_record_commitment::receipt_change_v1(receipt)?
        else {
            unreachable!();
        };
        if result.insert(key, value).is_some() {
            bail!("duplicate candidate cumulative receipt");
        }
    }
    Ok(result)
}

fn cold_diff(
    reader: &dyn StateRecordReader,
    parent: NodeHash,
    before: &BTreeMap<Vec<u8>, Vec<u8>>,
    after: &BTreeMap<Vec<u8>, Vec<u8>>,
) -> Result<StagedRecordUpdate> {
    let mut overlay = RecordOverlayV1::new(reader, parent);
    // A one-record staging budget also accepts a legal maximal-size record.
    for (key, value) in after {
        if before.get(key) != Some(value) {
            overlay.stage(&[RecordChange::Put {
                key: key.clone(),
                value: value.clone(),
            }])?;
        }
    }
    for key in before.keys().filter(|key| !after.contains_key(*key)) {
        overlay.stage(&[RecordChange::Delete { key: key.clone() }])?;
    }
    Ok(overlay.finish())
}

struct StagedReader<'a> {
    base: &'a dyn StateRecordReader,
    update: &'a StagedRecordUpdate,
}

impl StateNodeReader for StagedReader<'_> {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        if let Some(bytes) = self.update.nodes().get(hash) {
            validate_state_node_bytes(hash, bytes)?;
            return Ok(Some(bytes.clone()));
        }
        self.base.read_node(hash)
    }
}

impl StateRecordReader for StagedReader<'_> {
    fn read_record_chunk(&self, hash: NodeHash, index: u32) -> Result<Option<Vec<u8>>> {
        if let Some(bytes) = self.update.blobs().get(&hash) {
            return Ok(bytes
                .chunks(RECORD_CHUNK_BYTES_V1)
                .nth(index as usize)
                .map(<[u8]>::to_vec));
        }
        self.base.read_record_chunk(hash, index)
    }
}

fn record_document_commitment(bytes: &[u8]) -> NodeHash {
    sha256_bytes_v1(&[b"novovm-candidate-record-document-v2\0", bytes])
}

fn prepared_role_id(commitment: NodeHash, role: &[u8]) -> NodeHash {
    sha256_bytes_v1(&[b"novovm-candidate-record-prepared-v2\0", role, &commitment])
}

fn validate_projection(reference: &StoreRef, store: &NovNativeExecutionStoreV1) -> Result<()> {
    let bundle = reference
        .bundle
        .as_ref()
        .context("candidate record root bundle missing")?;
    bundle.validate(reference.root)?;
    if bundle.state.root != native_record_commitment::consensus_state_root_v1(&store.module_state)?
        || bundle.receipts.root != native_record_commitment::cumulative_receipt_root_v1(store)?
    {
        bail!("candidate record roots differ from the physical state's consensus projection");
    }
    Ok(())
}

fn validate_prepared_bundle(
    workspace: &WorkspaceStore,
    bytes: &[u8],
    reference: &StoreRef,
) -> Result<()> {
    let bundle = reference
        .bundle
        .as_ref()
        .context("candidate record root bundle missing")?;
    bundle.validate(reference.root)?;
    let commitment = record_document_commitment(bytes);
    let execution = bundle.commitment()?;
    let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
    for (role, link) in bundle.links() {
        let prepared = reader
            .load_record_prepared(prepared_role_id(commitment, role))?
            .context("candidate record tree completion missing")?;
        if prepared.input_commitment() != commitment
            || prepared.execution_commitment() != execution
            || prepared.parent_root() != link.parent_root
            || prepared.root() != link.root
        {
            bail!("candidate record tree descriptor differs from bound document roots");
        }
    }
    Ok(())
}

pub(super) fn persist(workspace: &WorkspaceStore, prepared: &PreparedDocument) -> Result<()> {
    if let Some(bundle) = &prepared.state.bundle {
        bundle.validate(prepared.state.root)?;
        let (state, receipts) = prepared
            .consensus_updates
            .as_ref()
            .context("candidate record document lacks consensus updates")?;
        let commitment = record_document_commitment(&prepared.bytes);
        let execution = bundle.commitment()?;
        let mut storage = AoemStateNodesV1::new(&workspace.graph, workspace.scope)?;
        for ((role, link), update) in
            bundle
                .links()
                .into_iter()
                .zip([&prepared.update, state, receipts])
        {
            if update.parent_root() != link.parent_root || update.root() != link.root {
                bail!("candidate staged root differs from its bound document");
            }
            storage.persist_record_candidate(
                prepared_role_id(commitment, role),
                commitment,
                execution,
                update,
            )?;
        }
        validate_prepared_bundle(workspace, &prepared.bytes, &prepared.state)?;
        read_store(workspace, &prepared.state)?;
        return Ok(());
    }
    if prepared.consensus_updates.is_some() {
        bail!("legacy candidate document has unbound consensus updates");
    }
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
    let store = read_store_from(&reader, reference)?;
    if let Some(bundle) = &reference.bundle {
        validate_projection(reference, &store)?;
        // A root node or prepared marker alone does not prove that inherited
        // leaves remain available. Keep full recovery checks at this boundary.
        for link in [&bundle.state, &bundle.receipts] {
            visit_records(&reader, link.root, reference.budget()?, &mut |_, _| Ok(()))?;
        }
    }
    Ok(store)
}

fn read_store_from(
    reader: &dyn StateRecordReader,
    reference: &StoreRef,
) -> Result<NovNativeExecutionStoreV1> {
    let mut records = BTreeMap::new();
    let stats = visit_records(
        reader,
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
    if schema != DOCUMENT_SCHEMA && schema != RECORD_DOCUMENT_SCHEMA {
        // Existing inline images remain readable; the enclosing Payload/Output
        // schema and exact existing validation still decide their admissibility.
        return Ok((serde_json::from_slice(bytes)?, None));
    }
    let document: Document = serde_json::from_slice(bytes)?;
    if schema == RECORD_DOCUMENT_SCHEMA {
        validate_prepared_bundle(workspace, bytes, &document.state)?;
    } else if document.state.bundle.is_some() {
        bail!("legacy candidate document cannot contain a record-profile root bundle");
    }
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
    if schema != DOCUMENT_SCHEMA && schema != RECORD_DOCUMENT_SCHEMA {
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

#[cfg(test)]
pub(crate) fn exercise_record_profile_document_storage_for_test(
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
        "record-profile-account".into(),
        BTreeMap::from([("NOV".into(), u128::MAX), ("USDT".into(), 99)]),
    );
    let initial = TestDocument {
        schema: "root-bundle-test/v1".into(),
        store,
    };
    let legacy = prepare(&workspace, &initial, &["store"], &initial.store, None)?;
    persist(&workspace, &legacy)?;
    assert!(legacy.state.rooted_parts()?.is_none());
    let initial =
        prepare_record_profile(&workspace, &initial, &["store"], &initial.store, None, None)?;
    assert!(decode::<TestDocument>(&workspace, &initial.bytes, &["store"]).is_err());
    persist(&workspace, &initial)?;
    let (document, reference): (TestDocument, _) = decode(&workspace, &initial.bytes, &["store"])?;
    let reference = reference.unwrap();
    let (physical_root, state_root, receipt_root, records, blob_bytes) =
        reference.rooted_parts()?.unwrap();
    assert_eq!(receipt_root, empty_root());
    assert_eq!(physical_root, legacy.state.root);
    assert_ne!(physical_root, state_root);
    let noop = prepare_record_profile(
        &workspace,
        &document,
        &["store"],
        &document.store,
        Some((&reference, &document.store)),
        None,
    )?;
    assert!(noop.update.nodes().is_empty() && noop.update.blobs().is_empty());
    let (state, receipts) = noop.consensus_updates.as_ref().unwrap();
    assert!(state.nodes().is_empty() && receipts.nodes().is_empty());
    persist(&workspace, &noop)?;
    let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
    let make_updates = |wrong_parent: bool| -> Result<RecordTreeUpdatesV1> {
        let mut physical = RecordOverlayV1::new(&reader, physical_root);
        let mut state = RecordOverlayV1::new(&reader, state_root);
        let changes = [native_store_records::RawPathChangeV1::Put {
            path: [
                "module_state",
                "account_asset_balances",
                "record-profile-account",
                "NOV",
            ]
            .map(str::to_owned)
            .to_vec(),
            value: native_store_records::typed_raw_v1(&(u128::MAX - 7))?,
        }];
        let delta = native_store_records::apply_raw_changes_to_overlay_v1(&mut physical, &changes)?;
        native_record_commitment::apply_consensus_changes_v1(&mut state, &changes)?;
        let (records, blob_bytes) = delta.checked_apply(records, blob_bytes)?;
        Ok(RecordTreeUpdatesV1 {
            physical: if wrong_parent {
                RecordOverlayV1::new(&reader, state_root).finish()
            } else {
                physical.finish()
            },
            state: state.finish(),
            receipts: RecordOverlayV1::new(&reader, receipt_root).finish(),
            records,
            blob_bytes,
        })
    };
    let mut next = TestDocument {
        schema: document.schema.clone(),
        store: document.store.clone(),
    };
    next.store
        .module_state
        .account_asset_balances
        .get_mut("record-profile-account")
        .unwrap()
        .insert("NOV".into(), u128::MAX - 7);
    assert!(prepare_record_profile(
        &workspace,
        &next,
        &["store"],
        &next.store,
        Some((&reference, &document.store)),
        Some(make_updates(true)?),
    )
    .is_err());
    let prepared = prepare_record_profile(
        &workspace,
        &next,
        &["store"],
        &next.store,
        Some((&reference, &document.store)),
        Some(make_updates(false)?),
    )?;
    assert_eq!(prepared.update.blobs().len(), 1);
    persist(&workspace, &prepared)?;
    let cold = prepare_record_profile(
        &workspace,
        &next,
        &["store"],
        &next.store,
        Some((&reference, &document.store)),
        None,
    )?;
    assert_eq!(
        prepared.bytes, cold.bytes,
        "incremental and cold document bytes agree"
    );
    assert_eq!(workspace.graph.get(&head_key)?, authority_before);
    drop(workspace);
    let reopened = WorkspaceStore::open(chain, params)?;
    let (recovered, _): (TestDocument, _) = decode(&reopened, &prepared.bytes, &["store"])?;
    assert_eq!(recovered.store, next.store);
    assert_eq!(
        recovered.store.module_state.account_asset_balances["record-profile-account"]["USDT"],
        99
    );
    for fault in 0..6 {
        let mut bad: Document = serde_json::from_slice(&prepared.bytes)?;
        match fault {
            0 => bad.state.bundle = None,
            1 => bad.schema = DOCUMENT_SCHEMA.into(),
            2 => bad.state.bundle.as_mut().unwrap().state.root[0] ^= 1,
            3 => bad.state.bundle.as_mut().unwrap().physical.parent_root[0] ^= 1,
            4 => {
                bad.state.bundle.as_mut().unwrap().state_codec =
                    crate::native_root_codecs::LEGACY_STATE_ROOT_CODEC_V3.into()
            }
            _ => bad.state.blob_bytes += 1,
        }
        assert!(decode::<TestDocument>(&reopened, &serde_json::to_vec(&bad)?, &["store"]).is_err());
    }
    // A completed document must not recreate a missing role completion marker.
    let id = prepared_role_id(record_document_commitment(&prepared.bytes), b"receipts");
    let marker = [b"NST1".as_slice(), &reopened.scope, b"c", &id].concat();
    let delete = AoemAtomicGraphWriteV1::Delete {
        key: marker.clone(),
    };
    reopened
        .graph
        .commit(novovm_exec::AoemAtomicGraphRequestV1 {
            graph_id: u64::from_be_bytes(id[..8].try_into()?)
                .max(1)
                .wrapping_add(1),
            steps: vec![novovm_exec::AoemAtomicGraphStepV1 {
                task_kind: 0,
                task_payload: vec![],
                writes: vec![delete.clone()],
                event: None,
            }],
            completion_write: delete,
        })?;
    assert!(decode::<TestDocument>(&reopened, &prepared.bytes, &["store"]).is_err());
    assert!(reopened.graph.get(&marker)?.is_none());
    assert_eq!(reopened.graph.get(&head_key)?, authority_before);
    Ok(())
}

#[cfg(test)]
mod root_bundle_tests {
    use super::*;

    #[test]
    fn record_bundle_legacy_reference_preserves_exact_json_bytes() {
        let reference = StoreRef {
            layout: STORE_CODEC.into(),
            tree_codec: STATE_RECORD_CODEC_V1.into(),
            root: [1; 32],
            records: 3,
            blob_bytes: 100,
            bundle: None,
        };
        let expected = format!(
            "{{\"layout\":\"{STORE_CODEC}\",\"tree_codec\":\"{STATE_RECORD_CODEC_V1}\",\"root\":{},\"records\":3,\"blob_bytes\":100}}",
            serde_json::to_string(&[1u8; 32]).unwrap(),
        );
        assert_eq!(serde_json::to_string(&reference).unwrap(), expected);
        assert!(reference.rooted_parts().unwrap().is_none());
    }

    #[test]
    fn record_bundle_role_and_document_bindings_are_distinct() {
        let first = record_document_commitment(b"first");
        let second = record_document_commitment(b"second");
        assert_ne!(first, second);
        let mut ids = std::collections::BTreeSet::new();
        for doc in [first, second] {
            for role in [b"physical".as_slice(), b"state", b"receipts"] {
                assert!(ids.insert(prepared_role_id(doc, role)));
            }
        }
    }
}
