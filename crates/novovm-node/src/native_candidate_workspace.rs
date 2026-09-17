#![forbid(unsafe_code)]

//! Local input/parent staging and isolated execution. Never calls pending
//! ingress, writes the authority head, or promotes a block. Not a remote API.

#[path = "native_candidate_auth.rs"]
mod auth;
#[path = "native_candidate_execution.rs"]
mod execution;
#[cfg(test)]
pub(super) use execution::{
    corrupt_execution_output_for_test_v1, execute_with_checkpoint_v1,
    load_execution_snapshot_for_test_v1, ExecutionCheckpointV1,
};
pub use execution::{execute_v1, load_execution_v1, ExecutionInfoV1};

use super::*;
use crate::native_candidate_plan::NovNativeCandidateExecutionPlanV1;
use novovm_exec::{
    AoemAtomicGraphRequestV1, AoemAtomicGraphStepV1, AoemAtomicGraphWriteV1,
    AoemSemanticGraphStoreV1, AoemStorageProviderConfigV1,
};
use serde::{Deserialize, Serialize};

pub const MAX_WORKSPACES_V1: usize = 32;
pub const MAX_PAYLOAD_BYTES_V1: usize = 8 * 1024 * 1024;
pub const MAX_TOTAL_PAYLOAD_BYTES_V1: usize = 64 * 1024 * 1024;
const CHUNK_BYTES: usize = 512;
const SCHEMA: &str = "novovm-native-candidate-workspace/v1";
const DESCRIPTOR_BYTES: usize = 4 + 32 * 6 + 8;
const _: () = assert!(DESCRIPTOR_BYTES <= CHUNK_BYTES);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatusV1 {
    Staging,
    Ready,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceInfoV1 {
    pub schema: &'static str,
    pub workspace_id: [u8; 32],
    pub slot: usize,
    pub chain_id: u64,
    pub plan_commitment: [u8; 32],
    pub parent_block_hash: [u8; 32],
    pub parent_state_root: [u8; 32],
    pub parent_snapshot_digest: [u8; 32],
    pub payload_digest: [u8; 32],
    pub payload_bytes: usize,
    pub status: WorkspaceStatusV1,
    pub input_snapshot_verified: bool,
    pub transactions_authenticated: bool,
    pub execution_completed: bool,
    pub chain_canonical: bool,
    pub proof_sealed: bool,
    pub safe: bool,
    pub finalized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Descriptor {
    id: [u8; 32],
    plan: [u8; 32],
    payload: [u8; 32],
    parent_block: [u8; 32],
    parent_state: [u8; 32],
    parent_snapshot: [u8; 32],
    len: usize,
}

impl Descriptor {
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(DESCRIPTOR_BYTES);
        out.extend_from_slice(b"NCW1");
        for value in [
            &self.id,
            &self.plan,
            &self.payload,
            &self.parent_block,
            &self.parent_state,
            &self.parent_snapshot,
        ] {
            out.extend_from_slice(value);
        }
        out.extend_from_slice(&(self.len as u64).to_be_bytes());
        out
    }

    fn decode(bytes: &[u8], scope: &[u8; 32]) -> Result<Self> {
        if bytes.len() != DESCRIPTOR_BYTES || &bytes[..4] != b"NCW1" {
            bail!("invalid candidate workspace descriptor codec");
        }
        let hash = |index: usize| -> [u8; 32] {
            bytes[4 + index * 32..4 + (index + 1) * 32]
                .try_into()
                .expect("fixed descriptor")
        };
        let descriptor = Self {
            id: hash(0),
            plan: hash(1),
            payload: hash(2),
            parent_block: hash(3),
            parent_state: hash(4),
            parent_snapshot: hash(5),
            len: usize::try_from(u64::from_be_bytes(bytes[196..204].try_into()?))?,
        };
        if descriptor.len == 0
            || descriptor.len > MAX_PAYLOAD_BYTES_V1
            || descriptor.id != workspace_id(scope, &descriptor.plan)
        {
            bail!("invalid candidate workspace descriptor bounds or domain");
        }
        Ok(descriptor)
    }

    fn info(&self, chain_id: u64, slot: usize, status: WorkspaceStatusV1) -> WorkspaceInfoV1 {
        WorkspaceInfoV1 {
            schema: SCHEMA,
            workspace_id: self.id,
            slot,
            chain_id,
            plan_commitment: self.plan,
            parent_block_hash: self.parent_block,
            parent_state_root: self.parent_state,
            parent_snapshot_digest: self.parent_snapshot,
            payload_digest: self.payload,
            payload_bytes: self.len,
            status,
            input_snapshot_verified: status == WorkspaceStatusV1::Ready,
            transactions_authenticated: false,
            execution_completed: false,
            chain_canonical: false,
            proof_sealed: false,
            safe: false,
            finalized: false,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    schema: String,
    plan: NovNativeCandidateExecutionPlanV1,
    parent_block: NovNativeDurableBlockV1,
    parent_snapshot: NovAoemOwnedNativeStateEnvelopeV1,
}

// An unknown graph completion can still publish writes after commit() returns
// an error. Keep the *workspace* OS lock until process exit in that case. Never
// retain the authority lock. An in-memory flag alone would not fence other hosts.
struct WorkspaceLock(fs::File);

impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

fn acquire_workspace_lock(path: &Path) -> Result<WorkspaceLock> {
    // This exact physical file must not vary with the Host projection backend.
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    let started = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(WorkspaceLock(file)),
            Err(std::fs::TryLockError::WouldBlock) => {
                if started.elapsed()
                    >= Duration::from_millis(NOV_NATIVE_EXECUTION_STORE_LOCK_TIMEOUT_MS_V1)
                {
                    bail!("candidate workspace lock busy or outcome uncertain; wait for its owner to exit");
                }
                std::thread::sleep(Duration::from_millis(
                    NOV_NATIVE_EXECUTION_STORE_LOCK_POLL_MS_V1,
                ));
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
        }
    }
}

fn poisoned_locks() -> &'static Mutex<BTreeMap<PathBuf, WorkspaceLock>> {
    static LOCKS: OnceLock<Mutex<BTreeMap<PathBuf, WorkspaceLock>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn reject_poisoned_workspace(path: &Path) -> Result<()> {
    if poisoned_locks()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains_key(path)
    {
        bail!("candidate workspace graph outcome uncertain; restart the owning process before recovery");
    }
    Ok(())
}

fn allocate_slot(catalog: &[(usize, Descriptor)], payload_len: usize) -> Result<usize> {
    if payload_len == 0 || payload_len > MAX_PAYLOAD_BYTES_V1 {
        bail!("candidate workspace payload exceeds bounds");
    }
    let total = catalog
        .iter()
        .try_fold(payload_len, |sum, (_, entry)| sum.checked_add(entry.len))
        .context("candidate workspace capacity overflow")?;
    if total > MAX_TOTAL_PAYLOAD_BYTES_V1 {
        bail!("candidate workspace aggregate byte capacity exhausted");
    }
    (0..MAX_WORKSPACES_V1)
        .find(|slot| !catalog.iter().any(|(taken, _)| taken == slot))
        .context("candidate workspace slot capacity exhausted")
}

struct WorkspaceStore {
    graph: AoemSemanticGraphStoreV1,
    scope: [u8; 32],
    namespace: String,
    protocol: [u8; 32],
    chain_id: u64,
    lock_path: PathBuf,
    lock: Option<WorkspaceLock>,
}

impl WorkspaceStore {
    fn open(chain_id: u64, params: &serde_json::Value) -> Result<Self> {
        let gates = tx_ingress_aoem_ownership_gates_from_params_v1(params);
        if !gates.explicit || !(gates.production_candidate || gates.semantic_graph_v3_required) {
            bail!("candidate workspace requires explicit AOEM production ownership");
        }
        let protocol = parse_fixed_hex_32_v1(
            &verify_required_native_business_protocol_config_pin_v1()?,
            "workspace protocol pin",
        )?;
        validate_native_persistence_path_isolation_v1(params)?;
        let namespace = native_aoem_owned_state_namespace_digest_v1(params, chain_id);
        let scope = sha256_bytes_v1(&[
            b"novovm-candidate-workspace-scope-v1\0",
            &chain_id.to_be_bytes(),
            namespace.as_bytes(),
        ]);
        // Do not create an empty authority database or bootstrap genesis here.
        let db_path = native_aoem_owned_state_db_path_v1(params);
        let canonical_db_path = db_path
            .canonicalize()
            .context("candidate workspace requires an existing AOEM authority database")?;
        // Canonicalize the lock identity, not the provider input: Windows adds
        // a verbatim prefix which the bundled RocksDB provider does not accept.
        // Use the same configured database path as the authority storage API.
        let lock_path =
            canonical_db_path.join(format!("candidate-workspace-{}.lock", to_hex(&scope)));
        reject_poisoned_workspace(&lock_path)?;
        let lock = acquire_workspace_lock(&lock_path)?;
        let runtime = native_aoem_owned_runtime_config_v1()?;
        if runtime.persist_backend.trim().eq_ignore_ascii_case("none") {
            bail!("candidate workspace requires a persistent AOEM backend");
        }
        let graph = AoemSemanticGraphStoreV1::open(
            &runtime,
            &db_path,
            &AoemStorageProviderConfigV1::default(),
        )?;
        Ok(Self {
            graph,
            scope,
            namespace,
            protocol,
            chain_id,
            lock_path,
            lock: Some(lock),
        })
    }

    fn key(&self, kind: u8, suffix: &[u8]) -> Vec<u8> {
        let mut key = b"NCW1".to_vec();
        key.extend_from_slice(&self.scope);
        key.push(kind);
        key.extend_from_slice(suffix);
        key
    }

    fn slot_key(&self, slot: usize) -> Vec<u8> {
        self.key(b's', &(slot as u32).to_be_bytes())
    }

    fn chunk_key(&self, id: &[u8; 32], index: usize) -> Vec<u8> {
        let mut suffix = id.to_vec();
        suffix.extend_from_slice(&(index as u32).to_be_bytes());
        self.key(b'c', &suffix)
    }

    fn catalog(&self) -> Result<Vec<(usize, Descriptor)>> {
        let mut entries = Vec::new();
        let mut ids = HashSet::new();
        let mut total = 0usize;
        for slot in 0..MAX_WORKSPACES_V1 {
            if let Some(raw) = self.graph.get(&self.slot_key(slot))? {
                let descriptor = Descriptor::decode(&raw, &self.scope)?;
                total = total
                    .checked_add(descriptor.len)
                    .context("workspace capacity overflow")?;
                if total > MAX_TOTAL_PAYLOAD_BYTES_V1 || !ids.insert(descriptor.id) {
                    bail!("candidate workspace catalog exceeds bounds or repeats an id");
                }
                entries.push((slot, descriptor));
            }
        }
        Ok(entries)
    }

    fn marker(&self, kind: u8, slot: usize, descriptor: &Descriptor) -> Vec<u8> {
        sha256_bytes_v1(&[
            b"novovm-candidate-workspace-marker-v1\0",
            &self.scope,
            &[kind],
            &(slot as u32).to_be_bytes(),
            &descriptor.encode(),
        ])
        .to_vec()
    }

    fn has_marker(&self, kind: u8, slot: usize, descriptor: &Descriptor) -> Result<bool> {
        match self.graph.get(&self.key(kind, &descriptor.id))? {
            None => Ok(false),
            Some(raw) if raw == self.marker(kind, slot, descriptor) => Ok(true),
            Some(_) => bail!("candidate workspace lifecycle marker binding mismatch"),
        }
    }

    fn status(&self, slot: usize, descriptor: &Descriptor) -> Result<WorkspaceStatusV1> {
        // Independent, immutable tombstone wins even over a late completion.
        if self.has_marker(b'a', slot, descriptor)? {
            return Ok(WorkspaceStatusV1::Aborted);
        }
        if self.has_marker(b'r', slot, descriptor)? {
            return Ok(WorkspaceStatusV1::Ready);
        }
        Ok(WorkspaceStatusV1::Staging)
    }

    fn commit(
        &mut self,
        phase: u8,
        descriptor: &Descriptor,
        writes: Vec<AoemAtomicGraphWriteV1>,
        completion_write: AoemAtomicGraphWriteV1,
    ) -> Result<()> {
        let digest = sha256_bytes_v1(&[
            b"novovm-candidate-workspace-graph-v1\0",
            &self.scope,
            &descriptor.encode(),
            &[phase],
        ]);
        let graph_id = u64::from_be_bytes(digest[..8].try_into()?).max(1);
        let steps = writes
            .chunks(4)
            .map(|writes| AoemAtomicGraphStepV1 {
                task_kind: 1,
                task_payload: digest.to_vec(),
                writes: writes.to_vec(),
                event: None,
            })
            .collect();
        let result = self.graph.commit(AoemAtomicGraphRequestV1 {
            graph_id,
            steps,
            completion_write,
        });
        if let Err(error) = result {
            if let Some(lock) = self.lock.take() {
                poisoned_locks()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(self.lock_path.clone(), lock);
            }
            return Err(error).context(
                "candidate workspace outcome uncertain; workspace lock retained until process exit",
            );
        }
        Ok(())
    }

    fn read_payload(&self, descriptor: &Descriptor) -> Result<Payload> {
        let mut bytes = Vec::with_capacity(descriptor.len);
        for index in 0..descriptor.len.div_ceil(CHUNK_BYTES) {
            let chunk = self
                .graph
                .get(&self.chunk_key(&descriptor.id, index))?
                .context("candidate workspace snapshot chunk missing")?;
            let expected = CHUNK_BYTES.min(descriptor.len - bytes.len());
            if chunk.len() != expected {
                bail!("candidate workspace snapshot chunk length mismatch");
            }
            bytes.extend_from_slice(&chunk);
        }
        if payload_digest(&bytes) != descriptor.payload {
            bail!("candidate workspace payload digest mismatch");
        }
        let payload: Payload =
            serde_json::from_slice(&bytes).context("decode candidate workspace payload")?;
        validate_payload(&payload, self)?;
        if descriptor != &describe(&payload, &bytes, &self.scope)? {
            bail!("candidate workspace descriptor does not bind its parent and plan");
        }
        Ok(payload)
    }

    fn info(&self, slot: usize, descriptor: &Descriptor) -> Result<WorkspaceInfoV1> {
        let status = self.status(slot, descriptor)?;
        if status == WorkspaceStatusV1::Ready {
            self.read_payload(descriptor)?;
        }
        Ok(descriptor.info(self.chain_id, slot, status))
    }
}

fn workspace_id(scope: &[u8; 32], plan: &[u8; 32]) -> [u8; 32] {
    sha256_bytes_v1(&[b"novovm-candidate-workspace-id-v1\0", scope, plan])
}

fn payload_digest(bytes: &[u8]) -> [u8; 32] {
    sha256_bytes_v1(&[b"novovm-candidate-workspace-payload-v1\0", bytes])
}

fn describe(payload: &Payload, bytes: &[u8], scope: &[u8; 32]) -> Result<Descriptor> {
    if bytes.is_empty() || bytes.len() > MAX_PAYLOAD_BYTES_V1 {
        bail!("candidate workspace payload exceeds 8 MiB");
    }
    Ok(Descriptor {
        id: workspace_id(scope, &payload.plan.plan_commitment),
        plan: payload.plan.plan_commitment,
        payload: payload_digest(bytes),
        parent_block: payload.parent_block.header.block_hash,
        parent_state: payload.plan.pre_state_root,
        parent_snapshot: sha256_bytes_v1(&[
            b"novovm-candidate-workspace-parent-v1\0",
            &serde_json::to_vec(&payload.parent_snapshot)?,
        ]),
        len: bytes.len(),
    })
}

fn validate_payload(payload: &Payload, workspace: &WorkspaceStore) -> Result<()> {
    let plan = &payload.plan;
    plan.validate()?;
    crate::native_block_ledger::validate_durable_block_v1(&payload.parent_block)?;
    if payload.schema != SCHEMA
        || plan.context.chain_id != workspace.chain_id
        || plan.protocol_config_commitment != workspace.protocol
    {
        bail!("candidate workspace input chain or protocol mismatch");
    }
    let snapshot = &payload.parent_snapshot;
    validate_production_native_state_envelope_v1(
        snapshot,
        workspace.chain_id,
        &workspace.namespace,
    )?;
    let header = &payload.parent_block.header;
    let parent = plan
        .aoem_parent
        .as_ref()
        .context("candidate workspace requires an existing AOEM parent")?;
    let parent_input = NovNativePreparedAoemParentV1 {
        batch_id: snapshot.batch_result.batch_id.clone(),
        batch_result_id: snapshot.batch_result.batch_result_id.clone(),
        state_root: parse_fixed_hex_32_v1(&snapshot.state_root, "workspace parent state")?,
        state_root_codec: snapshot.state_root_codec.clone(),
        cumulative_receipt_root: parse_fixed_hex_32_v1(
            &snapshot.receipt_root,
            "workspace parent receipts",
        )?,
        receipt_root_codec: snapshot.receipt_root_codec.clone(),
        state_version: snapshot.batch_result.snapshot_metadata.state_version,
    };
    if parent != &parent_input
        || plan.pre_state_root != parent.state_root
        || header.chain_id != workspace.chain_id
        || plan.context.parent_block_hash != header.block_hash
        || header.height.checked_add(1) != Some(plan.context.block_height)
        || plan.context.slot <= header.slot
        || plan.context.timestamp_unix_ms < header.timestamp_unix_ms
        || header.aoem_batch_id != parent.batch_id
        || header.aoem_batch_result_id != parent.batch_result_id
        || header.post_state_root != parent.state_root
        || header.post_state_root_codec != parent.state_root_codec
        || header.cumulative_receipt_root != parent.cumulative_receipt_root
        || header.cumulative_receipt_root_codec != parent.receipt_root_codec
        || header.state_version != parent.state_version
        || header.aoem_expected_output_commitment != snapshot.expected_output_commitment
        || header.aoem_evidence_commitment
            != parse_fixed_hex_32_v1(
                &native_aoem_execution_evidence_commitment_v1(&snapshot.batch_result)?,
                "workspace parent evidence",
            )?
    {
        bail!("candidate workspace plan, local ledger and AOEM parent do not agree");
    }
    for (raw, expected) in plan.raw_txs.iter().zip(&plan.tx_hashes) {
        if canonical_nov_native_tx_hash_from_payload_v1(raw)? != *expected {
            bail!("candidate workspace body does not match canonical transaction hashes");
        }
    }
    Ok(())
}

fn capture_parent(
    plan: &NovNativeCandidateExecutionPlanV1,
    params: &serde_json::Value,
    workspace: &WorkspaceStore,
) -> Result<Payload> {
    let store_path = resolve_native_execution_store_path_from_params_v1(params)
        .unwrap_or_else(nov_native_execution_store_path_v1);
    let _authority_lock = acquire_nov_native_execution_store_write_lock_v1(&store_path)?;
    let ledger = NovNativeBlockLedgerV1::open_existing_read_only(
        &nov_native_block_ledger_rocksdb_path_v1(&store_path),
    )?
    .context("candidate workspace requires an existing local block ledger")?;
    let ownership = ledger
        .load_aoem_ownership()?
        .context("candidate workspace ledger has no AOEM ownership")?;
    if ownership.chain_id != workspace.chain_id
        || ownership.namespace_digest != workspace.namespace
        || ownership.protocol_config_commitment != to_hex(&workspace.protocol)
    {
        bail!("candidate workspace ledger ownership mismatch");
    }
    if ledger.load_prepared(workspace.chain_id)?.is_some() {
        bail!("candidate workspace refuses unresolved authority preparation");
    }
    let head = ledger
        .load_head(workspace.chain_id)?
        .context("candidate workspace requires a local parent block")?;
    let parent_block = ledger
        .load_by_hash(workspace.chain_id, head.block_hash)?
        .context("candidate workspace parent block is missing")?;
    // Check the existing reader's allocation metadata BEFORE it allocates.
    let raw_head = workspace
        .graph
        .get(&native_aoem_owned_state_head_key_v1(
            workspace.chain_id,
            &workspace.namespace,
        ))?
        .context("candidate workspace AOEM authority head is missing")?;
    if raw_head.len() > CHUNK_BYTES {
        bail!("candidate workspace authority head exceeds bound");
    }
    let aoem_head: NovAoemOwnedNativeStateHeadV1 = serde_json::from_slice(&raw_head)?;
    if aoem_head.envelope_len == 0
        || aoem_head.envelope_len > MAX_PAYLOAD_BYTES_V1
        || aoem_head.chunk_count != aoem_head.envelope_len.div_ceil(CHUNK_BYTES)
    {
        bail!("candidate workspace parent snapshot exceeds bounds");
    }
    let parent_snapshot = read_native_state_envelope_from_aoem_graph_store_v1(
        &workspace.graph,
        workspace.chain_id,
        &workspace.namespace,
    )?;
    let payload = Payload {
        schema: SCHEMA.to_string(),
        plan: plan.clone(),
        parent_block,
        parent_snapshot,
    };
    validate_payload(&payload, workspace)?;
    Ok(payload)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CheckpointV1 {
    Reserved,
    PartialPayload,
    PayloadWritten,
    Ready,
}

/// Stage a local plan and a complete copy of its *current locally verified*
/// parent. Exact ready replay needs no current-head match; incomplete replay does.
pub fn create_v1(
    plan: &NovNativeCandidateExecutionPlanV1,
    params: &serde_json::Value,
) -> Result<WorkspaceInfoV1> {
    create_with_checkpoint_v1(plan, params, |_| Ok(()))
}

pub(super) fn create_with_checkpoint_v1(
    plan: &NovNativeCandidateExecutionPlanV1,
    params: &serde_json::Value,
    checkpoint: impl Fn(CheckpointV1) -> Result<()>,
) -> Result<WorkspaceInfoV1> {
    plan.validate()?;
    let mut workspace = WorkspaceStore::open(plan.context.chain_id, params)?;
    if plan.protocol_config_commitment != workspace.protocol {
        bail!("candidate workspace protocol mismatch");
    }
    let id = workspace_id(&workspace.scope, &plan.plan_commitment);
    let catalog = workspace.catalog()?;
    let existing = catalog.iter().find(|(_, descriptor)| descriptor.id == id);
    if let Some((slot, descriptor)) = existing {
        match workspace.status(*slot, descriptor)? {
            WorkspaceStatusV1::Aborted => bail!("aborted candidate workspace cannot be revived"),
            WorkspaceStatusV1::Ready => {
                if workspace.read_payload(descriptor)?.plan != *plan {
                    bail!("candidate workspace replay differs from stored input");
                }
                return workspace.info(*slot, descriptor);
            }
            WorkspaceStatusV1::Staging => {}
        }
    }
    let payload = capture_parent(plan, params, &workspace)?;
    let bytes = serde_json::to_vec(&payload)?;
    let descriptor = describe(&payload, &bytes, &workspace.scope)?;
    let slot = if let Some((slot, previous)) = existing {
        if previous != &descriptor {
            bail!("candidate workspace incomplete replay changed its captured parent");
        }
        *slot
    } else {
        let slot = allocate_slot(&catalog, bytes.len())?;
        let reservation = AoemAtomicGraphWriteV1::Put {
            key: workspace.slot_key(slot),
            value: descriptor.encode(),
        };
        workspace.commit(b's', &descriptor, vec![reservation.clone()], reservation)?;
        if workspace.graph.get(&workspace.slot_key(slot))? != Some(descriptor.encode()) {
            bail!("candidate workspace reservation readback mismatch");
        }
        slot
    };
    checkpoint(CheckpointV1::Reserved)?;
    let writes: Vec<_> = bytes
        .chunks(CHUNK_BYTES)
        .enumerate()
        .map(|(index, chunk)| AoemAtomicGraphWriteV1::Put {
            key: workspace.chunk_key(&id, index),
            value: chunk.to_vec(),
        })
        .collect();
    // Stage chunks without a ready marker. Replays write exactly the same bytes.
    let reservation = AoemAtomicGraphWriteV1::Put {
        key: workspace.slot_key(slot),
        value: descriptor.encode(),
    };
    workspace.commit(b'p', &descriptor, writes[..1].to_vec(), reservation.clone())?;
    checkpoint(CheckpointV1::PartialPayload)?;
    workspace.commit(b'c', &descriptor, writes, reservation)?;
    checkpoint(CheckpointV1::PayloadWritten)?;
    workspace.read_payload(&descriptor)?;
    let ready = AoemAtomicGraphWriteV1::Put {
        key: workspace.key(b'r', &id),
        value: workspace.marker(b'r', slot, &descriptor),
    };
    // The step is an idempotent copy, never the ready marker. Only completion
    // publishes readiness after all independent input bytes are durable.
    let first_chunk = AoemAtomicGraphWriteV1::Put {
        key: workspace.chunk_key(&id, 0),
        value: bytes[..CHUNK_BYTES.min(bytes.len())].to_vec(),
    };
    workspace.commit(b'r', &descriptor, vec![first_chunk], ready)?;
    checkpoint(CheckpointV1::Ready)?;
    workspace.info(slot, &descriptor)
}

pub fn load_v1(
    chain_id: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<Option<WorkspaceInfoV1>> {
    let workspace = WorkspaceStore::open(chain_id, params)?;
    workspace
        .catalog()?
        .iter()
        .find(|(_, descriptor)| descriptor.id == id)
        .map(|(slot, descriptor)| workspace.info(*slot, descriptor))
        .transpose()
}

pub fn list_v1(chain_id: u64, params: &serde_json::Value) -> Result<Vec<WorkspaceInfoV1>> {
    let workspace = WorkspaceStore::open(chain_id, params)?;
    workspace
        .catalog()?
        .iter()
        .map(|(slot, descriptor)| workspace.info(*slot, descriptor))
        .collect()
}

pub fn abort_v1(
    chain_id: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<WorkspaceInfoV1> {
    let mut workspace = WorkspaceStore::open(chain_id, params)?;
    let (slot, descriptor) = workspace
        .catalog()?
        .into_iter()
        .find(|(_, descriptor)| descriptor.id == id)
        .context("candidate workspace to abort was not found")?;
    if !workspace.has_marker(b'a', slot, &descriptor)? {
        let abort = AoemAtomicGraphWriteV1::Put {
            key: workspace.key(b'a', &id),
            value: workspace.marker(b'a', slot, &descriptor),
        };
        let reservation = AoemAtomicGraphWriteV1::Put {
            key: workspace.slot_key(slot),
            value: descriptor.encode(),
        };
        workspace.commit(b'a', &descriptor, vec![reservation], abort)?;
    }
    let info = workspace.info(slot, &descriptor)?;
    if info.status != WorkspaceStatusV1::Aborted {
        bail!("candidate workspace abort readback mismatch");
    }
    Ok(info)
}

#[cfg(test)]
pub(super) fn corrupt_first_chunk_for_test_v1(
    chain_id: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<()> {
    let mut workspace = WorkspaceStore::open(chain_id, params)?;
    let (slot, descriptor) = workspace
        .catalog()?
        .into_iter()
        .find(|(_, d)| d.id == id)
        .context("fixture workspace missing")?;
    let key = workspace.chunk_key(&id, 0);
    let mut bytes = workspace
        .graph
        .get(&key)?
        .context("fixture chunk missing")?;
    bytes[0] ^= 0xff;
    let write = AoemAtomicGraphWriteV1::Put { key, value: bytes };
    let ready = AoemAtomicGraphWriteV1::Put {
        key: workspace.key(b'r', &id),
        value: workspace.marker(b'r', slot, &descriptor),
    };
    workspace.commit(b'x', &descriptor, vec![write], ready)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(scope: &[u8; 32], value: u8, len: usize) -> Descriptor {
        Descriptor {
            id: workspace_id(scope, &[value; 32]),
            plan: [value; 32],
            payload: [2; 32],
            parent_block: [3; 32],
            parent_state: [4; 32],
            parent_snapshot: [5; 32],
            len,
        }
    }

    #[test]
    fn candidate_workspace_descriptor_codec_and_domain_are_bounded() {
        let scope = [9; 32];
        let original = descriptor(&scope, 1, MAX_PAYLOAD_BYTES_V1);
        assert_eq!(
            Descriptor::decode(&original.encode(), &scope).unwrap(),
            original
        );
        assert!(Descriptor::decode(&original.encode(), &[8; 32]).is_err());
        for len in [0, MAX_PAYLOAD_BYTES_V1 + 1, usize::MAX] {
            assert!(Descriptor::decode(&descriptor(&scope, 1, len).encode(), &scope).is_err());
        }
        let mut raw = original.encode();
        raw.push(0);
        assert!(Descriptor::decode(&raw, &scope).is_err());
        assert!(Descriptor::decode(&[], &scope).is_err());
        assert_ne!(
            workspace_id(&scope, &[1; 32]),
            workspace_id(&scope, &[2; 32])
        );
    }

    #[test]
    fn candidate_workspace_byte_and_slot_caps_include_every_reservation() {
        let scope = [9; 32];
        let seven: Vec<_> = (0..7)
            .map(|slot| (slot, descriptor(&scope, slot as u8, MAX_PAYLOAD_BYTES_V1)))
            .collect();
        assert_eq!(allocate_slot(&seven, MAX_PAYLOAD_BYTES_V1).unwrap(), 7);
        let mut eight = seven;
        eight.push((7, descriptor(&scope, 7, MAX_PAYLOAD_BYTES_V1)));
        assert!(allocate_slot(&eight, 1).is_err());
        let full: Vec<_> = (0..MAX_WORKSPACES_V1)
            .map(|slot| (slot, descriptor(&scope, slot as u8, 1)))
            .collect();
        assert!(allocate_slot(&full, 1).is_err());
        assert!(allocate_slot(&[], MAX_PAYLOAD_BYTES_V1 + 1).is_err());
        assert!(allocate_slot(&[], 0).is_err());
        assert!(allocate_slot(&[(0, descriptor(&scope, 0, usize::MAX))], 1).is_err());
    }

    #[test]
    fn candidate_workspace_uncertain_completion_retains_physical_fence() {
        let path = std::env::temp_dir().join(format!(
            "nov-workspace-poison-{}-{}.lock",
            std::process::id(),
            now_unix_millis_v1()
        ));
        let owner = acquire_workspace_lock(&path).unwrap();
        let contender = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        assert!(matches!(
            contender.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        poisoned_locks().lock().unwrap().insert(path.clone(), owner);
        assert!(reject_poisoned_workspace(&path)
            .unwrap_err()
            .to_string()
            .contains("restart"));
        assert!(matches!(
            contender.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        // Test-only emulation of process exit; there is no production unpoison API.
        poisoned_locks().lock().unwrap().remove(&path);
        contender.try_lock().unwrap();
        contender.unlock().unwrap();
        drop(contender);
        fs::remove_file(path).unwrap();
    }
}
