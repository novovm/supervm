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
#[path = "native_candidate_record_delta.rs"]
mod delta;
pub(super) use delta::VerifiedDeltaDocument;
#[cfg(test)]
#[path = "native_candidate_prepared_delta_tests.rs"]
mod prepared_delta_tests;
#[cfg(test)]
#[path = "native_candidate_reference_tests.rs"]
mod reference_tests;

const DOCUMENT_SCHEMA: &str = "novovm-candidate-record-document/v1";
const RECORD_DOCUMENT_SCHEMA: &str = "novovm-candidate-record-document/v2";
const STORE_CODEC: &str = "novovm-native-store-record-layout/v1";

#[cfg(test)]
std::thread_local! {
    static MATERIALIZATION_FORBIDDEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(super) fn without_materialization_for_test<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            MATERIALIZATION_FORBIDDEN.with(|flag| flag.set(self.0));
        }
    }
    let _restore = Restore(MATERIALIZATION_FORBIDDEN.with(|flag| flag.replace(true)));
    f()
}

#[cfg(test)]
pub(in super::super) fn assert_materialization_allowed_for_test() -> Result<()> {
    if MATERIALIZATION_FORBIDDEN.with(std::cell::Cell::get) {
        bail!("unexpected full candidate store materialization");
    }
    Ok(())
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    witness: Option<delta::Witness>,
}

pub(super) struct PreparedDocument {
    pub(super) bytes: Vec<u8>,
    state: StoreRef,
    update: StagedRecordUpdate,
    consensus_updates: Option<(StagedRecordUpdate, StagedRecordUpdate)>,
}

/// A checked V3 output prepared without a complete historical Store. The
/// original updates stay with the caller for exact legacy-reservation recovery.
pub(super) struct PreparedDeltaDocument {
    pub(super) bytes: Vec<u8>,
    state: StoreRef,
    parent: StoreRef,
    commitment: NodeHash,
}

impl PreparedDeltaDocument {
    pub(super) fn state(&self) -> &StoreRef {
        &self.state
    }
}

/// Only preparation may return this typed capacity condition. Bad roots,
/// missing blobs and malformed witnesses are not reasons to choose a cold path.
#[derive(Debug)]
pub(super) enum DeltaOutputTooLarge {
    Document,
    Witness,
}

impl std::fmt::Display for DeltaOutputTooLarge {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Document => "delta output exceeds the existing 8 MiB document budget",
            Self::Witness => "delta output exceeds the existing 8 MiB path or 64 MiB replay budget",
        })
    }
}

impl std::error::Error for DeltaOutputTooLarge {}

/// A metadata-only identity document. Its constructor does not authenticate the
/// supplied reference: the caller must already have a ledger/QC-bound source.
/// Keep this separate from PreparedDocument so the cold persistence boundary
/// cannot be accidentally weakened for normal candidate outputs.
pub(super) struct PreparedReferenceDocument {
    pub(super) bytes: Vec<u8>,
    state: StoreRef,
    updates: [StagedRecordUpdate; 3],
}

impl PreparedReferenceDocument {
    pub(super) fn state(&self) -> &StoreRef {
        &self.state
    }
}

/// Local document/marker binding only, NOT a verified parent or authority
/// capability. No physical-to-consensus projection, QC or historical data
/// availability claim is made by this value.
pub(super) struct BoundRecordMetadata<T> {
    pub(super) inline: T,
    pub(super) state: StoreRef,
}

pub(super) struct RecordTreeUpdatesV1 {
    pub(super) physical: StagedRecordUpdate,
    pub(super) state: StagedRecordUpdate,
    pub(super) receipts: StagedRecordUpdate,
    pub(super) records: usize,
    pub(super) blob_bytes: usize,
    pub(super) changes: Option<Vec<native_store_records::RawPathChangeV1>>,
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

/// Only this pre-reservation size refusal may select an explicit cold input.
#[derive(Debug)]
pub(super) struct ReferenceInputTooLarge;

impl std::fmt::Display for ReferenceInputTooLarge {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("reference input document exceeds existing 8 MiB budget")
    }
}

impl std::error::Error for ReferenceInputTooLarge {}

/// Bind an already-authorized three-root parent into a new local input document
/// without importing, cloning or scanning its store. The caller must preserve
/// the original source-document provenance in the enclosing NCW2 input.
pub(super) fn prepare_reference<T: Serialize>(
    workspace: &WorkspaceStore,
    inline_with_null_store: &T,
    store_path: &[&str],
    verified_parent_ref: &StoreRef,
) -> Result<PreparedReferenceDocument> {
    verified_parent_ref
        .rooted_parts()?
        .context("reference input requires a three-root parent")?;
    let mut state = verified_parent_ref.clone();
    let bundle = state
        .bundle
        .as_mut()
        .context("reference input bundle missing")?;
    for link in [
        &mut bundle.physical,
        &mut bundle.state,
        &mut bundle.receipts,
    ] {
        link.parent_root = link.root;
    }
    let raw = serde_json::value::to_raw_value(inline_with_null_store)?;
    let (inline, placeholder) = replace(&raw, store_path, RawValue::from_string("null".into())?)?;
    if placeholder.get() != "null" {
        bail!("reference input must contain exactly a null store placeholder");
    }
    let bytes = serde_json::to_vec(&Document {
        schema: RECORD_DOCUMENT_SCHEMA.into(),
        store_path: store_path.iter().map(|part| (*part).to_owned()).collect(),
        state: state.clone(),
        inline,
        witness: None,
    })?;
    if bytes.len() > MAX_PAYLOAD_BYTES_V1 {
        return Err(ReferenceInputTooLarge.into());
    }
    let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
    let bundle = state
        .bundle
        .as_ref()
        .context("reference input bundle missing")?;
    let updates = [
        bundle.physical.root,
        bundle.state.root,
        bundle.receipts.root,
    ]
    .map(|root| RecordOverlayV1::new(&reader, root).finish());
    Ok(PreparedReferenceDocument {
        bytes,
        state,
        updates,
    })
}

/// Writes identity-update descriptors/completions only, in the existing AOEM
/// database and scope. No new state/receipt records or authority head are made.
pub(super) fn persist_reference(
    workspace: &WorkspaceStore,
    prepared: &PreparedReferenceDocument,
) -> Result<()> {
    if prepared.bytes.len() > MAX_PAYLOAD_BYTES_V1 {
        bail!("reference input document exceeds existing 8 MiB budget");
    }
    let document: Document = serde_json::from_slice(&prepared.bytes)?;
    let expected_path = document
        .store_path
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let document = parse_metadata_document(&prepared.bytes, &expected_path, false)?;
    if serde_json::to_vec(&document.state)? != serde_json::to_vec(&prepared.state)? {
        bail!("reference input bytes differ from the prepared root bundle");
    }
    let bundle = document
        .state
        .bundle
        .as_ref()
        .context("reference input bundle missing")?;
    let commitment = record_document_commitment(&prepared.bytes);
    let execution = bundle.commitment()?;
    let mut storage = AoemStateNodesV1::new(&workspace.graph, workspace.scope)?;
    for ((role, link), update) in bundle.links().into_iter().zip(&prepared.updates) {
        if update.parent_root() != link.root
            || update.root() != link.root
            || !update.nodes().is_empty()
            || !update.blobs().is_empty()
        {
            bail!("reference input requires three exact identity updates");
        }
        storage.persist_record_candidate(
            prepared_role_id(commitment, role),
            commitment,
            execution,
            update,
        )?;
    }
    validate_prepared_bundle(workspace, &prepared.bytes, &document.state)
}

fn parse_metadata_document(
    bytes: &[u8],
    expected_path: &[&str],
    published_output: bool,
) -> Result<Document> {
    if bytes.len() > MAX_PAYLOAD_BYTES_V1 {
        bail!("record metadata document exceeds existing 8 MiB budget");
    }
    let document: Document = serde_json::from_slice(bytes)?;
    match document.schema.as_str() {
        RECORD_DOCUMENT_SCHEMA if document.witness.is_none() => {}
        delta::SCHEMA if published_output => document
            .witness
            .as_ref()
            .context("published delta metadata witness missing")?
            .validate()?,
        _ => bail!("unsupported record metadata document version or witness"),
    }
    if document
        .store_path
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        != expected_path
    {
        bail!("record metadata document store path mismatch");
    }
    let (_, placeholder) = replace(
        &document.inline,
        expected_path,
        RawValue::from_string("null".into())?,
    )?;
    if placeholder.get() != "null" {
        bail!("record metadata document contains an ambiguous inline store");
    }
    document
        .state
        .rooted_parts()?
        .context("record metadata document needs three roots")?;
    if !published_output {
        let bundle = document
            .state
            .bundle
            .as_ref()
            .context("reference input bundle missing")?;
        if bundle
            .links()
            .iter()
            .any(|(_, link)| link.parent_root != link.root)
        {
            bail!("reference input metadata must bind identity root links");
        }
    }
    Ok(document)
}

/// NCW2 input metadata only. Rejects inline/V1/V3 and non-identity documents.
/// The enclosing NCW2 validator still authenticates its source, chain and QC.
pub(super) fn decode_metadata<T: serde::de::DeserializeOwned>(
    workspace: &WorkspaceStore,
    bytes: &[u8],
    expected_path: &[&str],
) -> Result<BoundRecordMetadata<T>> {
    let document = parse_metadata_document(bytes, expected_path, false)?;
    validate_prepared_bundle(workspace, bytes, &document.state)?;
    Ok(BoundRecordMetadata {
        inline: serde_json::from_str(document.inline.get())?,
        state: document.state,
    })
}

/// Metadata from an original ledger-anchored published output. This verifies
/// document/role-marker binding and, for V3, witness structure only. It does NOT
/// replay deltas, authorize its parent roots, validate execution or authenticate
/// publication. The caller must check the exact original bytes against the
/// ledger's committed output digest and verify the block, QC and authority.
pub(super) fn decode_published_output_metadata<T: serde::de::DeserializeOwned>(
    workspace: &WorkspaceStore,
    bytes: &[u8],
    expected_path: &[&str],
) -> Result<BoundRecordMetadata<T>> {
    let document = parse_metadata_document(bytes, expected_path, true)?;
    validate_prepared_bundle(workspace, bytes, &document.state)?;
    Ok(BoundRecordMetadata {
        inline: serde_json::from_str(document.inline.get())?,
        state: document.state,
    })
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
        witness: None,
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
                changes: None,
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
    let witness = if let Some(changes) = &updates.changes {
        if delta::writer_budget_allows(changes)? {
            let witness = delta::Witness::from_changes(changes)?;
            let parent = base
                .context("delta output requires a captured three-root parent")?
                .0;
            let cached_parent = native_transfer_record_execution::ExecutionReader::new(&reader);
            let cached_post = native_transfer_record_execution::ExecutionReader::new(&staged);
            let reconstructed =
                delta::verify(&cached_parent, &cached_post, parent, &state, &witness)?;
            if reconstructed != *changes {
                bail!("delta supplied values differ from their authenticated output records");
            }
            Some(witness)
        } else {
            // No V3 reservation exists yet. Large legitimate updates retain the
            // cold V2 representation, never a V3 reader downgrade on corruption.
            None
        }
    } else {
        None
    };
    let mut document = Document {
        schema: if witness.is_some() {
            delta::SCHEMA
        } else {
            RECORD_DOCUMENT_SCHEMA
        }
        .into(),
        store_path: store_path.iter().map(|part| (*part).to_owned()).collect(),
        state: state.clone(),
        inline,
        witness,
    };
    let mut bytes = serde_json::to_vec(&document)?;
    if document.witness.is_some() && bytes.len() > MAX_PAYLOAD_BYTES_V1 {
        document.schema = RECORD_DOCUMENT_SCHEMA.into();
        document.witness = None;
        bytes = serde_json::to_vec(&document)?;
    }
    Ok(PreparedDocument {
        bytes,
        state,
        update: updates.physical,
        consensus_updates: Some((updates.state, updates.receipts)),
    })
}

fn delta_reference(parent: &StoreRef, updates: &RecordTreeUpdatesV1) -> Result<StoreRef> {
    let (physical, state, receipts, _, _) = parent
        .rooted_parts()?
        .context("delta preparation requires a verified three-root parent")?;
    for (update, root) in [&updates.physical, &updates.state, &updates.receipts]
        .into_iter()
        .zip([physical, state, receipts])
    {
        if update.parent_root() != root {
            bail!("delta update parent differs from the verified input roots");
        }
    }
    let link = |update: &StagedRecordUpdate| RootLink {
        parent_root: update.parent_root(),
        root: update.root(),
    };
    let profile = crate::native_root_codecs::NativeRootCodecProfileV1::RecordTreeV1;
    let reference = StoreRef {
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
    reference.rooted_parts()?;
    Ok(reference)
}

fn verify_delta_projection_points(
    state_reader: &dyn StateRecordReader,
    receipt_reader: &dyn StateRecordReader,
    reference: &StoreRef,
    changes: &[native_store_records::RawPathChangeV1],
) -> Result<()> {
    use crate::native_state_records::read_record;
    use native_store_records::RawPathChangeV1;
    let (_, state_root, receipt_root, _, _) = reference
        .rooted_parts()?
        .context("delta projection has no root bundle")?;
    let check = |reader: &dyn StateRecordReader, root, change: RecordChange| -> Result<()> {
        let (key, expected) = match change {
            RecordChange::Put { key, value } => (key, Some(value)),
            RecordChange::Delete { key } => (key, None),
        };
        if read_record(reader, root, &key)? != expected {
            bail!("delta projected point differs from the authenticated physical change");
        }
        Ok(())
    };
    for change in changes {
        if let Some(projected) = native_record_commitment::consensus_change_v1(change)? {
            check(state_reader, state_root, projected)?;
        }
        if let RawPathChangeV1::Put { path, value } = change {
            if path.first().map(String::as_str) == Some("receipts") {
                let receipt: NovNativeExecutionReceiptV1 = serde_json::from_slice(value)?;
                check(
                    receipt_reader,
                    receipt_root,
                    native_record_commitment::receipt_change_v1(&receipt)?,
                )?;
            }
        }
    }
    Ok(())
}

fn verify_staged_delta(
    workspace: &WorkspaceStore,
    parent: &StoreRef,
    reference: &StoreRef,
    updates: &RecordTreeUpdatesV1,
    witness: &delta::Witness,
) -> Result<()> {
    if serde_json::to_vec(&delta_reference(parent, updates)?)? != serde_json::to_vec(reference)? {
        bail!("delta prepared roots/statistics differ from the supplied updates");
    }
    let changes = updates
        .changes
        .as_ref()
        .context("delta preparation requires exact changed paths")?;
    let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
    let cached = native_transfer_record_execution::ExecutionReader::new(&reader);
    let physical = StagedReader {
        base: &cached,
        update: &updates.physical,
    };
    let cached_physical = native_transfer_record_execution::ExecutionReader::new(&physical);
    let reconstructed = delta::verify(&cached, &cached_physical, parent, reference, witness)?;
    if reconstructed != *changes {
        bail!("delta supplied values differ from their authenticated output records");
    }
    verify_delta_projection_points(
        &StagedReader {
            base: &cached,
            update: &updates.state,
        },
        &StagedReader {
            base: &cached,
            update: &updates.receipts,
        },
        reference,
        changes,
    )
}

/// Prepare only a V3 output from verified roots and exact, sorted net changes.
/// No complete Store is read, cloned or encoded. Business authorization and
/// execution-result metadata remain the enclosing candidate validator's job.
pub(super) fn prepare_delta<T: Serialize>(
    workspace: &WorkspaceStore,
    inline_with_null_store: &T,
    store_path: &[&str],
    verified_parent: &StoreRef,
    updates: &RecordTreeUpdatesV1,
) -> Result<PreparedDeltaDocument> {
    let state = delta_reference(verified_parent, updates)?;
    let raw = serde_json::value::to_raw_value(inline_with_null_store)?;
    // Use the identical object ordering as the old full-store V3 writer.
    let (inline, placeholder) = replace(&raw, store_path, RawValue::from_string("null".into())?)?;
    if placeholder.get() != "null" {
        bail!("delta preparation requires exactly a null store placeholder");
    }
    let changes = updates
        .changes
        .as_ref()
        .context("delta preparation requires exact changed paths")?;
    if !delta::writer_budget_allows(changes)? {
        return Err(DeltaOutputTooLarge::Witness.into());
    }
    let witness = delta::Witness::from_changes(changes)?;
    verify_staged_delta(workspace, verified_parent, &state, updates, &witness)?;
    let bytes = serde_json::to_vec(&Document {
        schema: delta::SCHEMA.into(),
        store_path: store_path.iter().map(|part| (*part).to_owned()).collect(),
        state: state.clone(),
        inline,
        witness: Some(witness),
    })?;
    if bytes.len() > MAX_PAYLOAD_BYTES_V1 {
        return Err(DeltaOutputTooLarge::Document.into());
    }
    Ok(PreparedDeltaDocument {
        commitment: delta::document_commitment(&bytes),
        bytes,
        state,
        parent: verified_parent.clone(),
    })
}

/// Persist only a previously checked delta. Ordinary persist remains a cold
/// boundary. All supplied updates are borrowed so an existing legacy output
/// reservation can still be recovered without executing business a second time.
pub(super) fn persist_delta(
    workspace: &WorkspaceStore,
    prepared: &PreparedDeltaDocument,
    updates: &RecordTreeUpdatesV1,
) -> Result<()> {
    if prepared.bytes.len() > MAX_PAYLOAD_BYTES_V1
        || delta::document_commitment(&prepared.bytes) != prepared.commitment
    {
        bail!("prepared delta document bytes changed after verification");
    }
    let document: Document = serde_json::from_slice(&prepared.bytes)?;
    if document.schema != delta::SCHEMA
        || serde_json::to_vec(&document.state)? != serde_json::to_vec(&prepared.state)?
    {
        bail!("prepared delta document root or schema binding changed");
    }
    let witness = document
        .witness
        .as_ref()
        .context("prepared delta witness missing")?;
    verify_staged_delta(
        workspace,
        &prepared.parent,
        &prepared.state,
        updates,
        witness,
    )?;
    let bundle = prepared
        .state
        .bundle
        .as_ref()
        .context("prepared delta root bundle missing")?;
    let execution = bundle.commitment()?;
    let mut storage = AoemStateNodesV1::new(&workspace.graph, workspace.scope)?;
    for ((role, _), update) in
        bundle
            .links()
            .into_iter()
            .zip([&updates.physical, &updates.state, &updates.receipts])
    {
        storage.persist_record_candidate(
            delta::prepared_id(prepared.commitment, role),
            prepared.commitment,
            execution,
            update,
        )?;
    }
    validate_prepared_bundle(workspace, &prepared.bytes, &prepared.state)?;
    // Use a fresh reader: staged or cached pre-write values cannot serve as
    // readback evidence. Check only named changes, never scan historical trees.
    let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
    let cached = native_transfer_record_execution::ExecutionReader::new(&reader);
    let changes = delta::verify(&cached, &cached, &prepared.parent, &prepared.state, witness)?;
    verify_delta_projection_points(&cached, &cached, &prepared.state, &changes)
}

/// Reproduce an already-reserved V2 byte image before considering older
/// physical-only/inline encodings. This never publishes or repairs a document.
pub(super) fn without_delta(mut prepared: PreparedDocument) -> Result<PreparedDocument> {
    let mut document: Document = serde_json::from_slice(&prepared.bytes)?;
    if document.schema == delta::SCHEMA {
        document
            .witness
            .as_ref()
            .context("delta document witness missing")?
            .validate()?;
        document.schema = RECORD_DOCUMENT_SCHEMA.into();
        document.witness = None;
        prepared.bytes = serde_json::to_vec(&document)?;
    } else if document.schema != DOCUMENT_SCHEMA && document.schema != RECORD_DOCUMENT_SCHEMA {
        bail!("unsupported prepared document version");
    }
    Ok(prepared)
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

#[derive(Deserialize)]
struct DocumentVersion {
    schema: String,
}

pub(super) fn is_delta_document(bytes: &[u8]) -> Result<bool> {
    let version: DocumentVersion = serde_json::from_slice(bytes)?;
    Ok(version.schema == delta::SCHEMA)
}

fn bound_document_commitment(bytes: &[u8]) -> Result<(NodeHash, bool)> {
    let version: DocumentVersion = serde_json::from_slice(bytes)?;
    match version.schema.as_str() {
        RECORD_DOCUMENT_SCHEMA => Ok((record_document_commitment(bytes), false)),
        delta::SCHEMA => Ok((delta::document_commitment(bytes), true)),
        _ => bail!("unsupported root-bound document version"),
    }
}

fn bound_prepared_id(commitment: NodeHash, role: &[u8], is_delta: bool) -> NodeHash {
    if is_delta {
        delta::prepared_id(commitment, role)
    } else {
        prepared_role_id(commitment, role)
    }
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
    let (commitment, is_delta) = bound_document_commitment(bytes)?;
    let execution = bundle.commitment()?;
    let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
    for (role, link) in bundle.links() {
        let prepared = reader
            .load_record_prepared(bound_prepared_id(commitment, role, is_delta))?
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
        let (commitment, is_delta) = bound_document_commitment(&prepared.bytes)?;
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
                bound_prepared_id(commitment, role, is_delta),
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
    #[cfg(test)]
    assert_materialization_allowed_for_test()?;
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
    if schema != DOCUMENT_SCHEMA && schema != RECORD_DOCUMENT_SCHEMA && schema != delta::SCHEMA {
        // Existing inline images remain readable; the enclosing Payload/Output
        // schema and exact existing validation still decide their admissibility.
        return Ok((serde_json::from_slice(bytes)?, None));
    }
    let document: Document = serde_json::from_slice(bytes)?;
    if schema == delta::SCHEMA {
        document
            .witness
            .as_ref()
            .context("delta document witness missing")?
            .validate()?;
    } else if document.witness.is_some() {
        bail!("legacy document cannot contain a delta witness");
    }
    if schema == RECORD_DOCUMENT_SCHEMA || schema == delta::SCHEMA {
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

/// Verify a V3 output from an independently verified parent without scanning or
/// materializing the historical store. Caller still validates exact authorized
/// changes, signatures, receipt semantics, execution metadata and QC linkage.
pub(super) fn decode_delta<T: serde::de::DeserializeOwned>(
    workspace: &WorkspaceStore,
    bytes: &[u8],
    expected_path: &[&str],
    verified_parent: &StoreRef,
) -> Result<Option<VerifiedDeltaDocument<T>>> {
    if bytes.len() > MAX_PAYLOAD_BYTES_V1 {
        bail!("delta document exceeds existing 8 MiB budget");
    }
    if !is_delta_document(bytes)? {
        return Ok(None);
    }
    let document: Document = serde_json::from_slice(bytes)?;
    if document
        .store_path
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        != expected_path
    {
        bail!("delta document store path mismatch");
    }
    let (_, placeholder) = replace(
        &document.inline,
        expected_path,
        RawValue::from_string("null".into())?,
    )?;
    if placeholder.get() != "null" {
        bail!("delta document contains an ambiguous inline store");
    }
    validate_prepared_bundle(workspace, bytes, &document.state)?;
    let witness = document
        .witness
        .as_ref()
        .context("delta document witness missing")?;
    let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
    let cached = native_transfer_record_execution::ExecutionReader::new(&reader);
    let changes = delta::verify(&cached, &cached, verified_parent, &document.state, witness)?;
    Ok(Some(VerifiedDeltaDocument {
        inline: serde_json::from_str(document.inline.get())?,
        state: document.state,
        changes,
    }))
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
    if is_delta_document(bytes)? {
        bail!("delta documents are output-only and require a verified parent");
    }
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
            changes: None,
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
    let changes = vec![native_store_records::RawPathChangeV1::Put {
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
    let mut delta_updates = make_updates(false)?;
    delta_updates.changes = Some(changes.clone());
    let with_delta = prepare_record_profile(
        &workspace,
        &next,
        &["store"],
        &next.store,
        Some((&reference, &document.store)),
        Some(delta_updates),
    )?;
    assert!(is_delta_document(&with_delta.bytes)?);
    assert!(!is_delta_document(&cold.bytes)?);
    let delta_bytes = with_delta.bytes.clone();
    persist(&workspace, &with_delta)?;
    assert_eq!(
        without_delta(with_delta)?.bytes,
        cold.bytes,
        "V3 fallback must exactly reproduce an already-reserved V2 image"
    );
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct LightDocument {
        schema: String,
        store: Option<NovNativeExecutionStoreV1>,
    }
    let verify_delta = |workspace: &WorkspaceStore| -> Result<()> {
        without_materialization_for_test(|| {
            let recovered =
                decode_delta::<LightDocument>(workspace, &delta_bytes, &["store"], &reference)?
                    .context("expected a delta document")?;
            assert_eq!(recovered.inline.schema, next.schema);
            assert!(recovered.inline.store.is_none());
            assert_eq!(
                recovered.state.rooted_parts()?,
                prepared.state.rooted_parts()?
            );
            assert_eq!(recovered.changes, changes);
            assert!(
                decode_delta::<LightDocument>(workspace, &cold.bytes, &["store"], &reference,)?
                    .is_none()
            );
            Ok(())
        })
    };
    verify_delta(&workspace)?;
    assert!(
        decode_delta::<LightDocument>(&workspace, &delta_bytes, &["wrong_store"], &reference,)
            .is_err()
    );
    let mut bad_parent = reference.clone();
    bad_parent.bundle.as_mut().unwrap().state.root[0] ^= 1;
    assert!(
        decode_delta::<LightDocument>(&workspace, &delta_bytes, &["store"], &bad_parent,).is_err()
    );
    assert!(decode_payload(&workspace, &delta_bytes)
        .err()
        .context("delta input document unexpectedly accepted")?
        .to_string()
        .contains("output-only"));
    let mut bad: Document = serde_json::from_slice(&delta_bytes)?;
    bad.inline = RawValue::from_string("{\"schema\":\"tampered\",\"store\":null}".into())?;
    assert!(decode_delta::<LightDocument>(
        &workspace,
        &serde_json::to_vec(&bad)?,
        &["store"],
        &reference,
    )
    .is_err());
    // Crossing the optional witness document budget selects the original V2
    // bytes before a reservation exists. V3 readers never silently downgrade.
    let large = TestDocument {
        schema: "x".repeat(MAX_PAYLOAD_BYTES_V1 - cold.bytes.len() + next.schema.len()),
        store: next.store.clone(),
    };
    let large_cold = prepare_record_profile(
        &workspace,
        &large,
        &["store"],
        &large.store,
        Some((&reference, &document.store)),
        None,
    )?;
    assert_eq!(large_cold.bytes.len(), MAX_PAYLOAD_BYTES_V1);
    let mut large_updates = make_updates(false)?;
    large_updates.changes = Some(changes.clone());
    let large_fallback = prepare_record_profile(
        &workspace,
        &large,
        &["store"],
        &large.store,
        Some((&reference, &document.store)),
        Some(large_updates),
    )?;
    assert!(!is_delta_document(&large_fallback.bytes)?);
    assert_eq!(large_fallback.bytes, large_cold.bytes);
    drop(large);
    let prepared_delta_bytes =
        prepared_delta_tests::exercise(&workspace, &reference, &document.store)?;
    let reference_bytes = reference_tests::exercise_metadata_reference_storage(
        &workspace,
        &cold.bytes,
        &delta_bytes,
        &legacy.bytes,
        &legacy.update,
    )?;
    assert_eq!(workspace.graph.get(&head_key)?, authority_before);
    drop(workspace);
    let reopened = WorkspaceStore::open(chain, params)?;
    prepared_delta_tests::verify_after_reopen(&reopened, &prepared_delta_bytes, &reference)?;
    reference_tests::verify_metadata_reference_after_reopen(&reopened, &reference_bytes)?;
    verify_delta(&reopened)?;
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
    // A matching V2 receipt marker is not a V3 marker, even for identical roots.
    let delta_id = delta::prepared_id(delta::document_commitment(&delta_bytes), b"receipts");
    let delta_marker = [b"NST1".as_slice(), &reopened.scope, b"c", &delta_id].concat();
    let delete = AoemAtomicGraphWriteV1::Delete {
        key: delta_marker.clone(),
    };
    reopened
        .graph
        .commit(novovm_exec::AoemAtomicGraphRequestV1 {
            graph_id: u64::from_be_bytes(delta_id[..8].try_into()?)
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
    assert!(
        decode_delta::<LightDocument>(&reopened, &delta_bytes, &["store"], &reference,).is_err()
    );
    assert!(decode::<TestDocument>(&reopened, &delta_bytes, &["store"]).is_err());
    assert!(reopened.graph.get(&delta_marker)?.is_none());
    assert!(decode::<TestDocument>(&reopened, &prepared.bytes, &["store"]).is_ok());
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
