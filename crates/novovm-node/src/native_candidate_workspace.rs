#![forbid(unsafe_code)]

//! Local input/parent staging and isolated execution. Never calls pending
//! ingress, writes the authority head, or promotes a block. Not a remote API.

#[path = "native_candidate_auth.rs"]
mod auth;
pub(crate) use auth::{require_new_successor_execution, CandidateInputRejected};
#[path = "native_candidate_execution.rs"]
mod execution;
#[path = "native_candidate_finalized_parent.rs"]
mod finalized_parent;
#[path = "native_candidate_input_capture.rs"]
mod input_capture;
#[path = "native_candidate_live_parent.rs"]
mod live_parent;
pub(crate) use live_parent::{load_finalized_parent_view_v1, FinalizedParentViewV1};
#[cfg(test)]
#[path = "native_candidate_finalized_query_tests.rs"]
mod finalized_query_tests;
#[cfg(test)]
pub(super) use finalized_query_tests::exercise_finalized_record_queries_for_test_v1;
#[cfg(test)]
#[path = "native_candidate_publication_verify_tests.rs"]
mod publication_verify_tests;
#[cfg(test)]
pub(crate) use publication_verify_tests::exercise_publication_verify_corruption_for_test_v1;
#[path = "native_candidate_rooted_parent.rs"]
mod rooted_parent;
#[path = "native_candidate_state_records.rs"]
mod state_records;
pub(crate) use execution::{
    capture_execution_from_finalized_v1, finish_execution_v1, ExecutionJobV1, ExecutionStartV1,
    PreparedExecutionV1,
};
pub use execution::{
    complete_genesis_promotion_v1, complete_successor_ledger_v1, execute_v1,
    finalize_genesis_promotion_v1, finalize_successor_v1, load_block_artifact_v1,
    load_execution_v1, load_finalized_genesis_parent_v1, load_latest_finalized_parent_v1,
    prepare_genesis_promotion_v1, prepare_successor_promotion_v1, publish_genesis_promotion_v1,
    publish_successor_authority_v1, register_block_candidate_v1, register_finalized_successor_v1,
    register_genesis_block_candidate_v1, resume_genesis_promotion_v1,
    resume_successor_promotion_v1, retire_old_workspaces_v1, verify_genesis_promotion_v1,
    verify_successor_authority_v1, with_verified_block_candidate_v1,
    with_verified_finalized_parent_round_v1, with_verified_finalized_successor_v1,
    with_verified_genesis_block_candidate_v1, ExecutionInfoV1, FinalizedGenesisParentV1,
    FreshSuccessorPublicationV1, GenesisPromotionPublicationV1, IsolatedBlockArtifactV1,
    WorkspaceRetirementV1,
};
#[cfg(test)]
pub(super) use execution::{
    complete_successor_with_checkpoint_v1, complete_with_checkpoint_v1,
    corrupt_execution_output_for_test_v1, execute_with_checkpoint_v1,
    exercise_light_first_compute_for_test_v1, exercise_light_output_recovery_for_test_v1,
    exercise_publication_readback_for_test_v1, finalize_successor_with_checkpoint_v1,
    load_execution_snapshot_for_test_v1, load_typed_execution_snapshot_for_test_v1,
    publish_successor_with_checkpoint_v1, publish_with_checkpoint_v1, retire_with_checkpoint_v1,
    ExecutionCheckpointV1, PromotionCheckpointV1, RetirementCheckpointV1,
};
pub(crate) use execution::{load_startup_artifact_v1, load_startup_successor_v1};
pub use finalized_parent::create_from_finalized_genesis_v1;
use finalized_parent::FinalizedParentSnapshot;
#[cfg(test)]
pub(super) use state_records::assert_materialization_allowed_for_test;
#[cfg(test)]
pub(super) use state_records::without_materialization_for_test;
#[cfg(test)]
#[path = "native_candidate_live_parent_tests.rs"]
mod live_parent_tests;
#[cfg(test)]
pub(super) use live_parent_tests::assert_live_parent_descriptor_binding_for_test_v1;
#[cfg(test)]
pub(super) use live_parent_tests::exercise_live_parent_admission_for_test_v1;
#[cfg(test)]
pub(super) use live_parent_tests::exercise_live_successor_signing_scope_for_test_v1;
#[cfg(test)]
pub(crate) use state_records::exercise_record_document_storage_for_test;
#[cfg(test)]
pub(crate) use state_records::exercise_record_profile_document_storage_for_test;

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
const LIGHT_SCHEMA: &str = "novovm-native-candidate-workspace/v2";
const DESCRIPTOR_BYTES: usize = 4 + 32 * 6 + 8;
const _: () = assert!(DESCRIPTOR_BYTES <= CHUNK_BYTES);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatusV1 {
    Staging,
    Ready,
    Aborted,
    Retiring,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DescriptorVersion {
    Ncw1,
    Ncw2,
}

impl DescriptorVersion {
    fn payload_digest(self, bytes: &[u8]) -> [u8; 32] {
        match self {
            Self::Ncw1 => payload_digest(bytes),
            Self::Ncw2 => sha256_bytes_v1(&[b"novovm-candidate-workspace-payload-v2\0", bytes]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Descriptor {
    version: DescriptorVersion,
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
        out.extend_from_slice(match self.version {
            DescriptorVersion::Ncw1 => b"NCW1",
            DescriptorVersion::Ncw2 => b"NCW2",
        });
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
        if bytes.len() != DESCRIPTOR_BYTES {
            bail!("invalid candidate workspace descriptor codec");
        }
        let version = match &bytes[..4] {
            b"NCW1" => DescriptorVersion::Ncw1,
            b"NCW2" => DescriptorVersion::Ncw2,
            _ => bail!("invalid candidate workspace descriptor codec"),
        };
        let hash = |index: usize| -> [u8; 32] {
            bytes[4 + index * 32..4 + (index + 1) * 32]
                .try_into()
                .expect("fixed descriptor")
        };
        let descriptor = Self {
            version,
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
            schema: match self.version {
                DescriptorVersion::Ncw1 => SCHEMA,
                DescriptorVersion::Ncw2 => LIGHT_SCHEMA,
            },
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
    parent_block: Option<NovNativeDurableBlockV1>,
    parent_snapshot: Option<NovAoemOwnedNativeStateEnvelopeV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    genesis: Option<fresh_genesis::publication::GenesisSnapshotV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    finalized_parent: Option<FinalizedParentSnapshot>,
    #[serde(skip)]
    record_state: Option<state_records::StoreRef>,
}

/// A proof-bound input without a fabricated historical Store. The parent
/// source document and the outer reference are validated independently.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LightPayload {
    schema: String,
    plan: NovNativeCandidateExecutionPlanV1,
    finalized_parent: rooted_parent::RootedParentSnapshot,
    #[serde(skip)]
    record_state: Option<state_records::StoreRef>,
}

enum VerifiedInput {
    Cold(Box<Payload>),
    Light(Box<LightPayload>),
}

impl Payload {
    fn root_codec_profile(&self) -> Result<crate::native_root_codecs::NativeRootCodecProfileV1> {
        if let Some(parent) = &self.finalized_parent {
            return parent.config.root_codec_profile();
        }
        if let Some(genesis) = &self.genesis {
            return genesis.config.root_codec_profile();
        }
        let parent = self
            .parent_snapshot
            .as_ref()
            .context("candidate root profile has no parent")?;
        crate::native_root_codecs::NativeRootCodecProfileV1::from_root_codecs(
            &parent.state_root_codec,
            &parent.receipt_root_codec,
        )
    }

    fn parent_store(&self) -> Result<&NovNativeExecutionStoreV1> {
        if let Some(parent) = &self.finalized_parent {
            if self.parent_block.is_some()
                || self.parent_snapshot.is_some()
                || self.genesis.is_some()
            {
                bail!("finalized parent cannot coexist with another parent variant");
            }
            return Ok(&parent.store);
        }
        match (&self.parent_block, &self.parent_snapshot, &self.genesis) {
            (Some(_), Some(snapshot), None) => Ok(&snapshot.store),
            (None, None, Some(snapshot)) => Ok(&snapshot.store),
            _ => bail!("candidate parent must be exactly one executed or fresh genesis state"),
        }
    }
}

// An unknown graph completion can still publish writes after commit() returns
// an error. Keep the *workspace* OS lock until process exit in that case. Never
// retain the authority lock for isolated execution. Authority publication has
// its own additional authority-lock retention boundary.
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
    params: serde_json::Value,
    lock_path: PathBuf,
    lock: Option<WorkspaceLock>,
    runtime: novovm_exec::AoemRuntimeConfig,
}

impl WorkspaceStore {
    fn open(chain_id: u64, params: &serde_json::Value) -> Result<Self> {
        Self::open_mode(chain_id, params, true)
    }

    /// Immutable candidate computation only. The caller cannot publish control
    /// metadata through commit() without acquiring the original namespace lock.
    /// Graph ownership is supplied by the explicit storage-owner thread scope;
    /// no Rc-backed handle or OS lock is moved between threads.
    fn open_computation(chain_id: u64, params: &serde_json::Value) -> Result<Self> {
        Self::open_mode(chain_id, params, false)
    }

    fn open_mode(chain_id: u64, params: &serde_json::Value, lock_required: bool) -> Result<Self> {
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
        let lock = lock_required
            .then(|| acquire_workspace_lock(&lock_path))
            .transpose()?;
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
            params: params.clone(),
            lock_path,
            lock,
            runtime,
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
        if self.graph.get(&self.key(b'g', &descriptor.id))?.is_some() {
            return Ok(WorkspaceStatusV1::Retiring);
        }
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
        if self.lock.is_none() {
            bail!("candidate computation view cannot commit workspace metadata without its lock");
        }
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

    fn read_input(&self, descriptor: &Descriptor) -> Result<VerifiedInput> {
        self.read_input_with_parent_archive(descriptor, None)
    }

    /// Same-call reuse for NCW2 only. Cold inputs and any further ancestor
    /// reads retain their original validation and do not inherit this archive.
    fn read_input_with_parent_archive(
        &self,
        descriptor: &Descriptor,
        verified_parent_archive: Option<&crate::native_block_ledger::FinalizedRecordArchiveV1>,
    ) -> Result<VerifiedInput> {
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
        if descriptor.version.payload_digest(&bytes) != descriptor.payload {
            bail!("candidate workspace payload digest mismatch");
        }
        match descriptor.version {
            DescriptorVersion::Ncw1 => {
                let payload = state_records::decode_payload(self, &bytes)
                    .context("decode candidate workspace payload")?;
                validate_payload(&payload, self)?;
                if descriptor != &describe(&payload, &bytes, &self.scope)? {
                    bail!("candidate workspace descriptor does not bind its parent and plan");
                }
                Ok(VerifiedInput::Cold(Box::new(payload)))
            }
            DescriptorVersion::Ncw2 => {
                let document = state_records::decode_metadata::<LightPayload>(
                    self,
                    &bytes,
                    &["finalized_parent", "store"],
                )?;
                let mut payload = document.inline;
                payload.record_state = Some(document.state);
                validate_light_payload_with_archive(&payload, self, verified_parent_archive)?;
                let reference = payload
                    .record_state
                    .as_ref()
                    .context("NCW2 parent reference missing")?;
                if descriptor != &describe_light(&payload, &bytes, reference, &self.scope)? {
                    bail!("candidate NCW2 descriptor does not bind its parent and plan");
                }
                Ok(VerifiedInput::Light(Box::new(payload)))
            }
        }
    }

    /// Explicit cold compatibility/export boundary, never used to implement
    /// light input validation. The reconstructed Store is complete and verified.
    fn read_payload(&self, descriptor: &Descriptor) -> Result<Payload> {
        match self.read_input(descriptor)? {
            VerifiedInput::Cold(payload) => Ok(*payload),
            VerifiedInput::Light(payload) => {
                let parent = payload
                    .finalized_parent
                    .materialize_cold(self, &self.params)?;
                let payload = Payload {
                    schema: SCHEMA.into(),
                    plan: payload.plan,
                    parent_block: None,
                    parent_snapshot: None,
                    genesis: None,
                    finalized_parent: Some(parent),
                    record_state: payload.record_state,
                };
                validate_payload(&payload, self)?;
                Ok(payload)
            }
        }
    }

    fn info(&self, slot: usize, descriptor: &Descriptor) -> Result<WorkspaceInfoV1> {
        let status = self.status(slot, descriptor)?;
        if status == WorkspaceStatusV1::Ready {
            self.read_input(descriptor)?;
        }
        Ok(descriptor.info(self.chain_id, slot, status))
    }

    /// An owned NCW2 input need not have been staged yet. Materialize only its
    /// already published parent source, never try to read a new candidate slot.
    fn cold_payload_from_verified(&self, verified: &VerifiedInput) -> Result<Payload> {
        let VerifiedInput::Light(light) = verified else {
            bail!("owned cold input is already directly available");
        };
        let payload = Payload {
            schema: SCHEMA.into(),
            plan: light.plan.clone(),
            parent_block: None,
            parent_snapshot: None,
            genesis: None,
            finalized_parent: Some(
                light
                    .finalized_parent
                    .materialize_cold(self, &self.params)?,
            ),
            record_state: light.record_state.clone(),
        };
        validate_payload(&payload, self)?;
        Ok(payload)
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
        version: DescriptorVersion::Ncw1,
        id: workspace_id(scope, &payload.plan.plan_commitment),
        plan: payload.plan.plan_commitment,
        payload: payload_digest(bytes),
        parent_block: payload.plan.context.parent_block_hash,
        parent_state: payload.plan.pre_state_root,
        parent_snapshot: sha256_bytes_v1(&[
            b"novovm-candidate-workspace-parent-v1\0",
            &if let Some(parent) = &payload.finalized_parent {
                serde_json::to_vec(parent)?
            } else if let Some(genesis) = &payload.genesis {
                serde_json::to_vec(genesis)?
            } else {
                serde_json::to_vec(&payload.parent_snapshot)?
            },
        ]),
        len: bytes.len(),
    })
}

fn describe_light(
    payload: &LightPayload,
    bytes: &[u8],
    reference: &state_records::StoreRef,
    scope: &[u8; 32],
) -> Result<Descriptor> {
    if bytes.is_empty() || bytes.len() > MAX_PAYLOAD_BYTES_V1 {
        bail!("candidate workspace payload exceeds 8 MiB");
    }
    Ok(Descriptor {
        version: DescriptorVersion::Ncw2,
        id: workspace_id(scope, &payload.plan.plan_commitment),
        plan: payload.plan.plan_commitment,
        payload: DescriptorVersion::Ncw2.payload_digest(bytes),
        parent_block: payload.plan.context.parent_block_hash,
        parent_state: payload.plan.pre_state_root,
        parent_snapshot: sha256_bytes_v1(&[
            b"novovm-candidate-workspace-rooted-parent-v2\0",
            &serde_json::to_vec(&(&payload.finalized_parent, reference))?,
        ]),
        len: bytes.len(),
    })
}

fn plan_contains_only_transfers(plan: &NovNativeCandidateExecutionPlanV1) -> Result<bool> {
    for raw in &plan.raw_txs {
        if !matches!(
            decode_nov_native_tx_wire_v1(raw)?.kind,
            NovTxKindV1::Transfer(_)
        ) {
            return Ok(false);
        }
    }
    Ok(!plan.raw_txs.is_empty())
}

fn validate_light_payload(payload: &LightPayload, workspace: &WorkspaceStore) -> Result<()> {
    validate_light_payload_with_archive(payload, workspace, None)
}

fn validate_light_payload_with_archive(
    payload: &LightPayload,
    workspace: &WorkspaceStore,
    verified_parent_archive: Option<&crate::native_block_ledger::FinalizedRecordArchiveV1>,
) -> Result<()> {
    let plan = &payload.plan;
    plan.validate()?;
    if payload.schema != LIGHT_SCHEMA
        || plan.context.chain_id != workspace.chain_id
        || plan.protocol_config_commitment != workspace.protocol
        || !plan_contains_only_transfers(plan)?
    {
        bail!("candidate NCW2 input schema, domain or transaction kind mismatch");
    }
    for (raw, expected) in plan.raw_txs.iter().zip(&plan.tx_hashes) {
        if canonical_nov_native_tx_hash_from_payload_v1(raw)? != *expected {
            bail!("candidate workspace body does not match canonical transaction hashes");
        }
    }
    let reference = payload
        .record_state
        .as_ref()
        .context("NCW2 parent reference missing")?;
    match verified_parent_archive {
        Some(archive) => payload.finalized_parent.validate_with_archive(
            workspace,
            plan,
            reference,
            &workspace.params,
            Some(archive),
        ),
        None => payload
            .finalized_parent
            .validate(workspace, plan, reference, &workspace.params),
    }
}

fn validate_payload(payload: &Payload, workspace: &WorkspaceStore) -> Result<()> {
    let plan = &payload.plan;
    plan.validate()?;
    payload.parent_store()?; // exclusive parent variants, never synthesized metadata
    if payload.schema != SCHEMA
        || plan.context.chain_id != workspace.chain_id
        || plan.protocol_config_commitment != workspace.protocol
    {
        bail!("candidate workspace input chain or protocol mismatch");
    }
    for (raw, expected) in plan.raw_txs.iter().zip(&plan.tx_hashes) {
        if canonical_nov_native_tx_hash_from_payload_v1(raw)? != *expected {
            bail!("candidate workspace body does not match canonical transaction hashes");
        }
    }
    if let Some(parent) = &payload.finalized_parent {
        return parent.validate(plan, workspace);
    }
    if let Some(genesis) = &payload.genesis {
        genesis.validate()?;
        if genesis.chain_id != workspace.chain_id
            || genesis.namespace_digest != workspace.namespace
            || genesis.config.protocol_config_commitment != workspace.protocol
            || genesis.config.compile()?.state_root() != plan.pre_state_root
            || plan.context.block_height != 1
            || plan.context.parent_block_hash != [0; 32]
            || plan.aoem_parent.is_some()
            || plan.context.timestamp_unix_ms < genesis.config.timestamp_unix_ms
        {
            bail!("candidate first-block plan disagrees with fresh genesis");
        }
        return Ok(());
    }
    let block = payload
        .parent_block
        .as_ref()
        .context("candidate parent block missing")?;
    crate::native_block_ledger::validate_durable_block_v1(block)?;
    let snapshot = payload
        .parent_snapshot
        .as_ref()
        .context("candidate parent snapshot missing")?;
    validate_production_native_state_envelope_v1(
        snapshot,
        workspace.chain_id,
        &workspace.namespace,
    )?;
    let header = &block.header;
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
    capture_parent_locked(plan, &store_path, workspace)
}

// Caller must hold the authority OS lock, after the workspace OS lock.
fn capture_parent_locked(
    plan: &NovNativeCandidateExecutionPlanV1,
    store_path: &Path,
    workspace: &WorkspaceStore,
) -> Result<Payload> {
    let ledger = NovNativeBlockLedgerV1::open_existing_read_only(
        &nov_native_block_ledger_rocksdb_path_v1(store_path),
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
        parent_block: Some(parent_block),
        parent_snapshot: Some(parent_snapshot),
        genesis: None,
        finalized_parent: None,
        record_state: None,
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

/// Stage the first candidate from a pinned, live AOEM genesis image. This does
/// not register it for signing, activate a ledger or publish execution output.
pub fn create_from_genesis_v1(
    plan: &NovNativeCandidateExecutionPlanV1,
    expected_genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<WorkspaceInfoV1> {
    create_inner_v1(plan, params, Some(expected_genesis), |_| Ok(()))
}

pub(super) fn create_with_checkpoint_v1(
    plan: &NovNativeCandidateExecutionPlanV1,
    params: &serde_json::Value,
    checkpoint: impl Fn(CheckpointV1) -> Result<()>,
) -> Result<WorkspaceInfoV1> {
    create_inner_v1(plan, params, None, checkpoint)
}

fn create_inner_v1(
    plan: &NovNativeCandidateExecutionPlanV1,
    params: &serde_json::Value,
    expected_genesis: Option<[u8; 32]>,
    checkpoint: impl Fn(CheckpointV1) -> Result<()>,
) -> Result<WorkspaceInfoV1> {
    plan.validate()?;
    let mut workspace = WorkspaceStore::open(plan.context.chain_id, params)?;
    if plan.protocol_config_commitment != workspace.protocol {
        bail!("candidate workspace protocol mismatch");
    }
    let id = workspace_id(&workspace.scope, &plan.plan_commitment);
    if workspace.graph.get(&workspace.key(b'g', &id))?.is_some() {
        bail!("retired candidate workspace cannot be revived");
    }
    let catalog = workspace.catalog()?;
    let existing = catalog.iter().find(|(_, descriptor)| descriptor.id == id);
    if let Some((slot, descriptor)) = existing {
        match workspace.status(*slot, descriptor)? {
            WorkspaceStatusV1::Aborted | WorkspaceStatusV1::Retiring => {
                bail!("closed candidate workspace cannot be revived")
            }
            WorkspaceStatusV1::Ready => {
                let payload = workspace.read_payload(descriptor)?;
                if payload.plan != *plan
                    || payload.finalized_parent.is_some()
                    || payload.genesis.as_ref().map(|g| g.commitment()) != expected_genesis
                {
                    bail!("candidate workspace replay differs from stored input");
                }
                return workspace.info(*slot, descriptor);
            }
            WorkspaceStatusV1::Staging => {}
        }
    }
    let payload = if let Some(pin) = expected_genesis {
        let store_path = resolve_native_execution_store_path_from_params_v1(params)
            .context("genesis candidate requires explicit native store path")?;
        let _authority_lock = acquire_nov_native_execution_store_write_lock_v1(&store_path)?;
        let manifest = NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(
            &nov_native_block_ledger_rocksdb_path_v1(&store_path),
            pin,
            parse_fixed_hex_32_v1(&workspace.namespace, "genesis candidate namespace")?,
        )?
        .context("genesis candidate requires a complete reserved manifest")?;
        if manifest.compile()?.config_commitment() != pin {
            bail!("genesis candidate manifest pin mismatch");
        }
        let genesis = fresh_genesis::publication::read_snapshot_v1(
            &workspace.graph,
            workspace.chain_id,
            &workspace.namespace,
            pin,
        )?;
        let payload = Payload {
            schema: SCHEMA.to_owned(),
            plan: plan.clone(),
            parent_block: None,
            parent_snapshot: None,
            genesis: Some(genesis),
            finalized_parent: None,
            record_state: None,
        };
        validate_payload(&payload, &workspace)?;
        payload
    } else {
        capture_parent(plan, params, &workspace)?
    };
    stage_payload(&mut workspace, &payload, checkpoint)
}

fn stage_payload(
    workspace: &mut WorkspaceStore,
    payload: &Payload,
    checkpoint: impl Fn(CheckpointV1) -> Result<()>,
) -> Result<WorkspaceInfoV1> {
    let parent_store = payload.parent_store()?;
    let mut prepared = if payload.root_codec_profile()?
        == crate::native_root_codecs::NativeRootCodecProfileV1::RecordTreeV1
    {
        state_records::prepare_record_profile(
            workspace,
            payload,
            &state_records::payload_path(payload)?,
            parent_store,
            payload
                .record_state
                .as_ref()
                .map(|reference| (reference, parent_store)),
            None,
        )?
    } else {
        state_records::prepare(
            workspace,
            payload,
            &state_records::payload_path(payload)?,
            parent_store,
            payload
                .record_state
                .as_ref()
                .map(|reference| (reference, parent_store)),
        )?
    };
    let mut descriptor = describe(payload, &prepared.bytes, &workspace.scope)?;
    let id = descriptor.id;
    if workspace.graph.get(&workspace.key(b'g', &id))?.is_some() {
        bail!("retired candidate workspace cannot be revived");
    }
    let catalog = workspace.catalog()?;
    let existing = catalog.iter().find(|(_, previous)| previous.id == id);
    if existing.is_some_and(|(_, previous)| *previous != descriptor) {
        let old = state_records::prepare(
            workspace,
            payload,
            &state_records::payload_path(payload)?,
            parent_store,
            payload
                .record_state
                .as_ref()
                .map(|reference| (reference, parent_store)),
        )?;
        let old_descriptor = describe(payload, &old.bytes, &workspace.scope)?;
        if existing.is_some_and(|(_, previous)| *previous == old_descriptor) {
            prepared = old;
            descriptor = old_descriptor;
        }
    }
    // An existing reservation pins its exact physical encoding. An upgrade may
    // read/replay an old inline image, but must not replace it with a record
    // document under the same workspace id or relax the captured-parent check.
    let inline =
        if let Some((_, previous)) = existing.filter(|(_, previous)| *previous != descriptor) {
            let bytes = serde_json::to_vec(payload)?;
            let legacy = describe(payload, &bytes, &workspace.scope)?;
            if *previous != legacy {
                bail!("candidate workspace replay changed its captured parent");
            }
            descriptor = legacy;
            Some(bytes)
        } else {
            None
        };
    let bytes = inline.as_deref().unwrap_or(&prepared.bytes);
    stage_payload_bytes(
        workspace,
        descriptor,
        bytes,
        |workspace| {
            if inline.is_none() {
                state_records::persist(workspace, &prepared)?;
            }
            Ok(())
        },
        checkpoint,
    )
}

/// Only an oversized, never-reserved input may decline NCW2 before writing.
/// Invalid provenance, mismatching replays and all storage errors stay closed.
fn stage_light_payload(
    workspace: &mut WorkspaceStore,
    payload: &LightPayload,
    checkpoint: impl Fn(CheckpointV1) -> Result<()>,
) -> Result<Option<WorkspaceInfoV1>> {
    validate_light_payload(payload, workspace)?;
    let id = workspace_id(&workspace.scope, &payload.plan.plan_commitment);
    if workspace.graph.get(&workspace.key(b'g', &id))?.is_some() {
        bail!("retired candidate workspace cannot be revived");
    }
    let already_reserved = workspace.catalog()?.iter().any(|(_, input)| input.id == id);
    let prepared = match state_records::prepare_reference(
        workspace,
        payload,
        &["finalized_parent", "store"],
        payload
            .record_state
            .as_ref()
            .context("NCW2 parent reference missing")?,
    ) {
        Ok(prepared) => prepared,
        Err(error) if !already_reserved && error.is::<state_records::ReferenceInputTooLarge>() => {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let descriptor = describe_light(payload, &prepared.bytes, prepared.state(), &workspace.scope)?;
    stage_payload_bytes(
        workspace,
        descriptor,
        &prepared.bytes,
        |workspace| state_records::persist_reference(workspace, &prepared),
        checkpoint,
    )
    .map(Some)
}

fn stage_payload_bytes(
    workspace: &mut WorkspaceStore,
    descriptor: Descriptor,
    bytes: &[u8],
    persist_records: impl FnOnce(&WorkspaceStore) -> Result<()>,
    checkpoint: impl Fn(CheckpointV1) -> Result<()>,
) -> Result<WorkspaceInfoV1> {
    if bytes.is_empty()
        || bytes.len() > MAX_PAYLOAD_BYTES_V1
        || descriptor.len != bytes.len()
        || descriptor.version.payload_digest(bytes) != descriptor.payload
    {
        bail!("candidate workspace staged bytes disagree with descriptor bounds or digest");
    }
    let id = descriptor.id;
    if workspace.graph.get(&workspace.key(b'g', &id))?.is_some() {
        bail!("retired candidate workspace cannot be revived");
    }
    let catalog = workspace.catalog()?;
    let existing = catalog.iter().find(|(_, previous)| previous.id == id);
    if let Some((slot, previous)) = existing {
        if previous != &descriptor {
            bail!("candidate workspace replay changed its captured parent");
        }
        match workspace.status(*slot, previous)? {
            WorkspaceStatusV1::Aborted | WorkspaceStatusV1::Retiring => {
                bail!("closed candidate workspace cannot be revived")
            }
            WorkspaceStatusV1::Ready => return workspace.info(*slot, previous),
            WorkspaceStatusV1::Staging => {}
        }
    }
    let slot = if let Some((slot, _)) = existing {
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
    persist_records(workspace)?;
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
    workspace.read_input(&descriptor)?;
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

#[cfg(test)]
pub(crate) fn assert_execution_unpublished_for_test_v1(
    chain: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<()> {
    let workspace = WorkspaceStore::open(chain, params)?;
    let mut first_chunk = id.to_vec();
    first_chunk.extend_from_slice(&0u32.to_be_bytes());
    for key in [
        workspace.key(b'v', &id),
        workspace.key(b'e', &id),
        workspace.key(b'o', &first_chunk),
    ] {
        if workspace.graph.get(&key)?.is_some() {
            bail!("candidate unexpectedly has an output reservation, chunk or completion");
        }
    }
    Ok(())
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
    if workspace.status(slot, &descriptor)? == WorkspaceStatusV1::Retiring {
        bail!("retiring workspace requires retirement recovery, not abort");
    }
    if workspace.status(slot, &descriptor)? == WorkspaceStatusV1::Ready {
        let payload = match workspace.read_payload(&descriptor) {
            Ok(payload) => Some(payload),
            Err(error) => {
                // Legacy isolated inputs can be discarded even when damaged.
                // Never infer legacy from unreadable bytes: the existing ledger
                // must explicitly pass the non-genesis schema/evidence fence.
                let native_path = resolve_native_execution_store_path_from_params_v1(params)
                    .context("damaged candidate abort requires explicit native path")?;
                NovNativeBlockLedgerV1::open_existing_read_only(
                    &nov_native_block_ledger_rocksdb_path_v1(&native_path),
                )
                .with_context(|| format!("cannot establish legacy abort scope: {error}"))?
                .context("damaged candidate abort requires an existing legacy ledger")?;
                None
            }
        };
        if let Some(parent) = payload
            .as_ref()
            .and_then(|payload| payload.finalized_parent.as_ref())
        {
            let native_path = resolve_native_execution_store_path_from_params_v1(params)
                .context("successor abort requires explicit native path")?;
            NovNativeBlockLedgerV1::refuse_pending_successor_promotion_v1(
                &nov_native_block_ledger_rocksdb_path_v1(&native_path),
                parent.config.compile()?.config_commitment(),
                parse_fixed_hex_32_v1(&workspace.namespace, "successor abort namespace")?,
            )?;
        }
        if let Some(genesis) = payload.and_then(|payload| payload.genesis) {
            let native_path = resolve_native_execution_store_path_from_params_v1(params)
                .context("fresh candidate abort requires explicit native store path")?;
            NovNativeBlockLedgerV1::refuse_pending_fresh_promotion_v1(
                &nov_native_block_ledger_rocksdb_path_v1(&native_path),
                genesis.config.compile()?.config_commitment(),
                parse_fixed_hex_32_v1(&workspace.namespace, "abort namespace")?,
            )?;
        }
    }
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
pub(super) fn publication_evidence_key_for_test_v1(
    chain: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<Vec<u8>> {
    Ok(WorkspaceStore::open(chain, params)?.key(b'h', &id))
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

/// Write the pre-record-document format to exercise real upgrade recovery.
#[cfg(test)]
pub(super) fn seed_legacy_inline_input_for_test_v1(
    plan: &NovNativeCandidateExecutionPlanV1,
    params: &serde_json::Value,
    stage: CheckpointV1,
    corrupt_descriptor: bool,
) -> Result<WorkspaceInfoV1> {
    let mut workspace = WorkspaceStore::open(plan.context.chain_id, params)?;
    let payload = capture_parent(plan, params, &workspace)?;
    let bytes = serde_json::to_vec(&payload)?;
    let mut descriptor = describe(&payload, &bytes, &workspace.scope)?;
    let catalog = workspace.catalog()?;
    if catalog.iter().any(|(_, entry)| entry.id == descriptor.id) {
        bail!("legacy inline fixture requires a new workspace");
    }
    let slot = allocate_slot(&catalog, bytes.len())?;
    if corrupt_descriptor {
        descriptor.payload[0] ^= 1;
    }
    let reservation = AoemAtomicGraphWriteV1::Put {
        key: workspace.slot_key(slot),
        value: descriptor.encode(),
    };
    workspace.commit(
        b's',
        &descriptor,
        vec![reservation.clone()],
        reservation.clone(),
    )?;
    let writes: Vec<_> = bytes
        .chunks(CHUNK_BYTES)
        .enumerate()
        .map(|(index, chunk)| AoemAtomicGraphWriteV1::Put {
            key: workspace.chunk_key(&descriptor.id, index),
            value: chunk.to_vec(),
        })
        .collect();
    if stage == CheckpointV1::PartialPayload {
        workspace.commit(b'p', &descriptor, writes[..1].to_vec(), reservation)?;
    } else if matches!(stage, CheckpointV1::PayloadWritten | CheckpointV1::Ready) {
        workspace.commit(b'c', &descriptor, writes, reservation)?;
        if stage == CheckpointV1::Ready {
            let first = AoemAtomicGraphWriteV1::Put {
                key: workspace.chunk_key(&descriptor.id, 0),
                value: bytes[..CHUNK_BYTES.min(bytes.len())].to_vec(),
            };
            let ready = AoemAtomicGraphWriteV1::Put {
                key: workspace.key(b'r', &descriptor.id),
                value: workspace.marker(b'r', slot, &descriptor),
            };
            workspace.commit(b'r', &descriptor, vec![first], ready)?;
        }
    } else if stage != CheckpointV1::Reserved {
        bail!("unsupported legacy inline input fixture stage");
    }
    let original = workspace.info(slot, &descriptor)?;
    if stage == CheckpointV1::Ready {
        // The finalized-parent creator calls stage_payload directly; ordinary
        // create_v1 has an earlier ready fast path. Exercise both entry shapes.
        let replay = stage_payload(&mut workspace, &payload, |_| {
            bail!("ready legacy replay must not write or reach checkpoints")
        })?;
        if workspace.graph.get(&workspace.slot_key(slot))? != Some(descriptor.encode())
            || replay.workspace_id != original.workspace_id
        {
            bail!("ready legacy replay changed its reservation");
        }
    }
    Ok(original)
}

#[cfg(test)]
pub(crate) use execution::{
    assert_delta_output_point_read_for_test_v1, assert_light_input_output_point_read_for_test_v1,
    seed_legacy_inline_output_for_test_v1, seed_legacy_record_output_for_test_v1,
    seed_previous_record_output_for_test_v1,
};

/// Four isolated input-recovery fixtures reuse the same verified light parent.
/// No execution output, registration or authority mutation is performed here.
#[cfg(test)]
pub(crate) fn exercise_light_input_recovery_for_test_v1(
    chain: u64,
    source_id: [u8; 32],
    params: &serde_json::Value,
) -> Result<()> {
    state_records::without_materialization_for_test(|| {
        let (source_slot, source_descriptor, source_bytes, reference, authority) = {
            let workspace = WorkspaceStore::open(chain, params)?;
            let (slot, descriptor) = workspace
                .catalog()?
                .into_iter()
                .find(|(_, input)| input.id == source_id)
                .context("NCW2 recovery fixture source input missing")?;
            if workspace.status(slot, &descriptor)? != WorkspaceStatusV1::Ready {
                bail!("NCW2 recovery fixture source must be ready");
            }
            let VerifiedInput::Light(payload) = workspace.read_input(&descriptor)? else {
                bail!("NCW2 recovery fixture requires a light source input");
            };
            let authority = workspace.graph.get(&native_aoem_owned_state_head_key_v1(
                chain,
                &workspace.namespace,
            ))?;
            (
                slot,
                descriptor,
                serde_json::to_vec(&payload)?,
                payload.record_state,
                authority,
            )
        };
        for (index, stop) in [
            CheckpointV1::Reserved,
            CheckpointV1::PartialPayload,
            CheckpointV1::PayloadWritten,
            CheckpointV1::Ready,
        ]
        .into_iter()
        .enumerate()
        {
            let mut payload: LightPayload = serde_json::from_slice(&source_bytes)?;
            payload.record_state = reference.clone();
            let mut context = payload.plan.context;
            context.slot = context
                .slot
                .checked_add(100 + u64::try_from(index)?)
                .context("NCW2 recovery fixture slot overflow")?;
            payload.plan = NovNativeCandidateExecutionPlanV1::new(
                context,
                payload.plan.protocol_config_commitment,
                payload.plan.pre_state_root,
                payload.plan.aoem_parent.clone(),
                payload.plan.tx_hashes.clone(),
                payload.plan.raw_txs.clone(),
            )?;
            let (slot, descriptor) = {
                let mut workspace = WorkspaceStore::open(chain, params)?;
                let prepared = state_records::prepare_reference(
                    &workspace,
                    &payload,
                    &["finalized_parent", "store"],
                    payload
                        .record_state
                        .as_ref()
                        .context("NCW2 fixture reference missing")?,
                )?;
                let expected = describe_light(
                    &payload,
                    &prepared.bytes,
                    prepared.state(),
                    &workspace.scope,
                )?;
                if workspace
                    .catalog()?
                    .iter()
                    .any(|(_, input)| input.id == expected.id)
                {
                    bail!("NCW2 recovery fixture requires an unused input context");
                }
                let reached = std::cell::Cell::new(false);
                let failure = match stage_light_payload(&mut workspace, &payload, |point| {
                    if point == stop {
                        reached.set(true);
                        bail!("NCW2 fixture interrupted input stage");
                    }
                    Ok(())
                }) {
                    Err(error) => error,
                    Ok(_) => bail!("NCW2 fixture failed to interrupt its chosen stage"),
                };
                if !reached.get()
                    || !format!("{failure:#}").contains("NCW2 fixture interrupted input stage")
                {
                    bail!("NCW2 fixture failed before its chosen stage: {failure:#}");
                }
                let (slot, actual) = workspace
                    .catalog()?
                    .into_iter()
                    .find(|(_, input)| input.id == expected.id)
                    .context("NCW2 interrupted reservation missing")?;
                let status = if stop == CheckpointV1::Ready {
                    WorkspaceStatusV1::Ready
                } else {
                    WorkspaceStatusV1::Staging
                };
                if actual != expected
                    || actual.version != DescriptorVersion::Ncw2
                    || workspace.status(slot, &actual)? != status
                {
                    bail!("NCW2 interrupted stage changed its reservation or readiness");
                }
                (slot, actual)
            };
            // A new handle recovers from durable bytes, not an in-memory payload.
            let mut workspace = WorkspaceStore::open(chain, params)?;
            if workspace.graph.get(&workspace.slot_key(slot))? != Some(descriptor.encode()) {
                bail!("NCW2 reopened reservation differs from its original bytes");
            }
            let checkpoints = std::cell::RefCell::new(Vec::new());
            let info = stage_light_payload(&mut workspace, &payload, |point| {
                checkpoints.borrow_mut().push(point);
                Ok(())
            })?
            .context("NCW2 reserved fixture cannot fall back to a cold input")?;
            let expected = if stop == CheckpointV1::Ready {
                Vec::new()
            } else {
                vec![
                    CheckpointV1::Reserved,
                    CheckpointV1::PartialPayload,
                    CheckpointV1::PayloadWritten,
                    CheckpointV1::Ready,
                ]
            };
            if *checkpoints.borrow() != expected
                || info.schema != LIGHT_SCHEMA
                || info.workspace_id != descriptor.id
                || info.payload_digest != descriptor.payload
                || info.parent_snapshot_digest != descriptor.parent_snapshot
                || info.status != WorkspaceStatusV1::Ready
                || workspace.graph.get(&workspace.slot_key(slot))? != Some(descriptor.encode())
                || !matches!(workspace.read_input(&descriptor)?, VerifiedInput::Light(_))
                || workspace
                    .graph
                    .get(&workspace.key(b'v', &descriptor.id))?
                    .is_some()
                || workspace.graph.get(&native_aoem_owned_state_head_key_v1(
                    chain,
                    &workspace.namespace,
                ))? != authority
            {
                bail!("NCW2 recovery changed its reserved bytes, output, authority or completion order");
            }
        }
        let mut workspace = WorkspaceStore::open(chain, params)?;
        let mut wrong_parent: LightPayload = serde_json::from_slice(&source_bytes)?;
        wrong_parent.record_state = reference;
        wrong_parent.finalized_parent.promotion_commitment[0] ^= 1;
        let wrong_reached_stage = std::cell::Cell::new(false);
        if stage_light_payload(&mut workspace, &wrong_parent, |_| {
            wrong_reached_stage.set(true);
            bail!("changed NCW2 parent must fail before staging")
        })
        .is_ok()
            || wrong_reached_stage.get()
            || workspace.graph.get(&workspace.slot_key(source_slot))?
                != Some(source_descriptor.encode())
            || workspace.status(source_slot, &source_descriptor)? != WorkspaceStatusV1::Ready
            || !matches!(
                workspace.read_input(&source_descriptor)?,
                VerifiedInput::Light(_)
            )
            || workspace.graph.get(&native_aoem_owned_state_head_key_v1(
                chain,
                &workspace.namespace,
            ))? != authority
        {
            bail!("NCW2 same-plan changed-parent replay altered the verified source");
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(scope: &[u8; 32], value: u8, len: usize) -> Descriptor {
        Descriptor {
            version: DescriptorVersion::Ncw1,
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
    fn candidate_workspace_ncw2_descriptor_keeps_ncw1_bytes_and_separates_domains() {
        let scope = [9; 32];
        let original = descriptor(&scope, 1, 513);
        // Fixed pre-NCW2 layout: no version byte was inserted into its fields.
        let mut legacy = b"NCW1".to_vec();
        for field in [original.id, [1; 32], [2; 32], [3; 32], [4; 32], [5; 32]] {
            legacy.extend_from_slice(&field);
        }
        legacy.extend_from_slice(&513u64.to_be_bytes());
        assert_eq!(legacy.len(), 204);
        assert_eq!(original.encode(), legacy);
        assert_eq!(Descriptor::decode(&legacy, &scope).unwrap(), original);
        let bytes = b"the exact same parent document";
        assert_eq!(
            DescriptorVersion::Ncw1.payload_digest(bytes),
            sha256_bytes_v1(&[b"novovm-candidate-workspace-payload-v1\0", bytes,])
        );
        assert_ne!(
            DescriptorVersion::Ncw1.payload_digest(bytes),
            DescriptorVersion::Ncw2.payload_digest(bytes)
        );
        let mut light = original.clone();
        light.version = DescriptorVersion::Ncw2;
        assert_eq!(&light.encode()[..4], b"NCW2");
        assert_eq!(&light.encode()[4..], &legacy[4..]);
        assert_eq!(Descriptor::decode(&light.encode(), &scope).unwrap(), light);
        assert_eq!(
            original.info(1, 0, WorkspaceStatusV1::Staging).schema,
            SCHEMA
        );
        assert_eq!(
            light.info(1, 0, WorkspaceStatusV1::Staging).schema,
            LIGHT_SCHEMA
        );
        for magic in [b"NCW0", b"NCW3", b"NCW\0"] {
            let mut unknown = legacy.clone();
            unknown[..4].copy_from_slice(magic);
            assert!(Descriptor::decode(&unknown, &scope).is_err());
        }
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
