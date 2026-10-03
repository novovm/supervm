//! AOEM persistence for immutable incremental state nodes.
//!
//! A completion record here means only that an isolated candidate's nodes were
//! durably stored. It does NOT advance an authority head, authenticate a block,
//! or establish finality. The existing candidate/BFT protocol does not admit
//! this new root codec yet. Node keys are independent of workspace retirement.

use crate::native_state_records::{
    validate_record_blob, StagedRecordUpdate, StateRecordReader, RECORD_CHUNK_BYTES_V1,
    STATE_RECORD_CODEC_V1,
};
use crate::native_state_tree::{
    empty_root, validate_state_node_bytes, NodeHash, StagedStateUpdate, StateNodeReader,
    STATE_TREE_CODEC_V1,
};
use anyhow::{bail, Context, Result};
use novovm_exec::{
    AoemAtomicGraphRequestV1, AoemAtomicGraphStepV1, AoemAtomicGraphWriteV1,
    AoemSemanticGraphStoreV1,
};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

const DESCRIPTOR_LENGTH: usize = 4 + 6 * 32;
const _: () = assert!(DESCRIPTOR_LENGTH <= 512);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedStateRootV1 {
    record_codec: bool,
    candidate: NodeHash,
    parent_root: NodeHash,
    root: NodeHash,
    input_commitment: NodeHash,
    execution_commitment: NodeHash,
}

impl PreparedStateRootV1 {
    pub fn candidate(&self) -> NodeHash {
        self.candidate
    }

    pub fn input_commitment(&self) -> NodeHash {
        self.input_commitment
    }

    pub fn execution_commitment(&self) -> NodeHash {
        self.execution_commitment
    }

    pub fn root(&self) -> NodeHash {
        self.root
    }

    pub fn parent_root(&self) -> NodeHash {
        self.parent_root
    }

    fn encode(&self) -> Vec<u8> {
        let mut bytes = if self.record_codec { b"NSR2" } else { b"NSR1" }.to_vec();
        bytes.extend_from_slice(&codec_hash(self.record_codec));
        for hash in [
            self.candidate,
            self.parent_root,
            self.root,
            self.input_commitment,
            self.execution_commitment,
        ] {
            bytes.extend_from_slice(&hash);
        }
        bytes
    }

    fn decode(bytes: &[u8], candidate: NodeHash, record_codec: bool) -> Result<Self> {
        if bytes.len() != DESCRIPTOR_LENGTH
            || &bytes[..4] != if record_codec { b"NSR2" } else { b"NSR1" }
            || bytes[4..36] != codec_hash(record_codec)
        {
            bail!("incremental candidate root descriptor codec mismatch");
        }
        let result = Self {
            record_codec,
            candidate: bytes[36..68].try_into()?,
            parent_root: bytes[68..100].try_into()?,
            root: bytes[100..132].try_into()?,
            input_commitment: bytes[132..164].try_into()?,
            execution_commitment: bytes[164..196].try_into()?,
        };
        if result.candidate != candidate {
            bail!("incremental candidate root identity mismatch");
        }
        Ok(result)
    }
}

/// A read-only borrow of the same AOEM provider. Does not acquire/create a
/// writer lock, change state, or confer authority on a caller-provided root.
pub struct AoemStateReaderV1<'a> {
    graph: &'a AoemSemanticGraphStoreV1,
    scope: NodeHash,
}

impl<'a> AoemStateReaderV1<'a> {
    pub fn new(graph: &'a AoemSemanticGraphStoreV1, scope: NodeHash) -> Self {
        Self { graph, scope }
    }

    pub fn load_prepared(&self, candidate: NodeHash) -> Result<Option<PreparedStateRootV1>> {
        self.load_prepared_inner(candidate, false)
    }

    pub fn load_record_prepared(&self, candidate: NodeHash) -> Result<Option<PreparedStateRootV1>> {
        self.load_prepared_inner(candidate, true)
    }

    fn load_prepared_inner(
        &self,
        candidate: NodeHash,
        records: bool,
    ) -> Result<Option<PreparedStateRootV1>> {
        let Some(completed) = self.graph.get(&state_key(self.scope, b'c', candidate))? else {
            return Ok(None);
        };
        let bytes = self
            .graph
            .get(&state_key(self.scope, b'p', candidate))?
            .context("completed state root descriptor missing")?;
        if completed != completion(self.scope, &bytes) {
            bail!("incremental candidate completion mismatch");
        }
        let result = PreparedStateRootV1::decode(&bytes, candidate, records)?;
        if result.root != empty_root() {
            self.read_node(&result.root)?
                .context("incremental root node missing")?;
        }
        Ok(Some(result))
    }
}

impl StateNodeReader for AoemStateReaderV1<'_> {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        let bytes = self.graph.get(&state_key(self.scope, b'n', *hash))?;
        if let Some(bytes) = &bytes {
            validate_state_node_bytes(hash, bytes)?;
        }
        Ok(bytes)
    }
}

impl StateRecordReader for AoemStateReaderV1<'_> {
    fn read_record_chunk(&self, hash: NodeHash, index: u32) -> Result<Option<Vec<u8>>> {
        self.graph.get(&record_chunk_key(self.scope, hash, index))
    }
}

fn state_key(scope: NodeHash, kind: u8, hash: NodeHash) -> Vec<u8> {
    let mut key = b"NST1".to_vec();
    key.extend_from_slice(&scope);
    key.push(kind);
    key.extend_from_slice(&hash);
    key
}

fn record_chunk_key(scope: NodeHash, hash: NodeHash, index: u32) -> Vec<u8> {
    let mut key = state_key(scope, b'b', hash);
    key.extend_from_slice(&index.to_be_bytes());
    key
}

fn completion(scope: NodeHash, descriptor: &[u8]) -> Vec<u8> {
    let mut hash = Sha256::new();
    hash.update(b"novovm-incremental-state-complete-v1\0");
    hash.update(scope);
    hash.update(descriptor);
    hash.finalize().to_vec()
}

/// Borrows the existing AOEM authority provider, not a second state database.
/// `scope` isolates chain storage; it is deliberately absent from state roots.
pub struct AoemStateNodesV1<'a> {
    graph: &'a AoemSemanticGraphStoreV1,
    scope: NodeHash,
    // Physical, canonical database identity, not a namespace-dependent lock.
    // Held across descriptor check, graph completion and readback.
    writer_lock: Option<std::fs::File>,
}

impl<'a> AoemStateNodesV1<'a> {
    pub fn new(graph: &'a AoemSemanticGraphStoreV1, scope: NodeHash) -> Result<Self> {
        let lock_path =
            std::fs::canonicalize(graph.path())?.join("novovm-state-nodes-v1.writer.lock");
        let writer_lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        writer_lock
            .try_lock()
            .context("incremental state writer busy or outcome uncertain")?;
        Ok(Self {
            graph,
            scope,
            writer_lock: Some(writer_lock),
        })
    }

    fn key(&self, kind: u8, hash: NodeHash) -> Vec<u8> {
        state_key(self.scope, kind, hash)
    }

    fn completion(&self, descriptor: &[u8]) -> Vec<u8> {
        completion(self.scope, descriptor)
    }

    /// Read only a completed candidate, never a reservation or orphan node set.
    /// Recovery must still compare its input/execution binding and verify BFT.
    pub fn load_prepared(&self, candidate: NodeHash) -> Result<Option<PreparedStateRootV1>> {
        self.reader().load_prepared(candidate)
    }

    /// Checks descriptor completion and root availability, not a full scan of
    /// every inherited record. Reads still verify every requested chunk/hash;
    /// a missing completed blob is an error, never permission to repair it.
    pub fn load_record_prepared(&self, candidate: NodeHash) -> Result<Option<PreparedStateRootV1>> {
        self.reader().load_record_prepared(candidate)
    }

    pub fn reader(&self) -> AoemStateReaderV1<'_> {
        AoemStateReaderV1::new(self.graph, self.scope)
    }

    fn require_root(&self, root: NodeHash) -> Result<()> {
        if root != empty_root() {
            self.read_node(&root)?
                .context("incremental root node missing")?;
        }
        Ok(())
    }

    /// The update must come from a previously verified parent using the tree
    /// planner. This does not validate an arbitrary imported parent/subtree.
    /// On unknown AOEM completion, stop this candidate flow and recover first;
    /// never publish authority, discard caller locks, or blindly retry.
    pub fn persist_candidate(
        &mut self,
        candidate: NodeHash,
        input_commitment: NodeHash,
        execution_commitment: NodeHash,
        update: &StagedStateUpdate,
    ) -> Result<PreparedStateRootV1> {
        self.persist_inner(
            PreparedStateRootV1 {
                record_codec: false,
                candidate,
                parent_root: update.parent_root(),
                root: update.root(),
                input_commitment,
                execution_commitment,
            },
            update.nodes(),
            &BTreeMap::new(),
        )
    }

    /// Blobs, nodes and the prepared descriptor share one graph completion.
    /// Every staged blob chunk is checked before write and read back afterwards.
    /// No authority head is written; the outer candidate protocol owns finality.
    pub fn persist_record_candidate(
        &mut self,
        candidate: NodeHash,
        input_commitment: NodeHash,
        execution_commitment: NodeHash,
        update: &StagedRecordUpdate,
    ) -> Result<PreparedStateRootV1> {
        self.persist_inner(
            PreparedStateRootV1 {
                record_codec: true,
                candidate,
                parent_root: update.parent_root(),
                root: update.root(),
                input_commitment,
                execution_commitment,
            },
            update.nodes(),
            update.blobs(),
        )
    }

    fn persist_inner(
        &mut self,
        prepared: PreparedStateRootV1,
        nodes: &BTreeMap<NodeHash, Vec<u8>>,
        blobs: &BTreeMap<NodeHash, Vec<u8>>,
    ) -> Result<PreparedStateRootV1> {
        if self.writer_lock.is_none() {
            bail!("incremental state outcome uncertain; restart process before recovery");
        }
        let candidate = prepared.candidate;
        self.require_root(prepared.parent_root)?;
        let descriptor = prepared.encode();
        if let Some(existing) = self.graph.get(&self.key(b'p', candidate))? {
            if existing != descriptor {
                bail!("candidate identity already binds a different state update");
            }
        }
        let mut writes = Vec::with_capacity(nodes.len() + 1);
        for (hash, bytes) in blobs {
            validate_record_blob(hash, bytes)?;
            for (index, chunk) in bytes.chunks(RECORD_CHUNK_BYTES_V1).enumerate() {
                let key = record_chunk_key(self.scope, *hash, u32::try_from(index)?);
                match self.graph.get(&key)? {
                    Some(existing) if existing != chunk => {
                        bail!("immutable AOEM record chunk differs")
                    }
                    Some(_) => {}
                    None => writes.push(AoemAtomicGraphWriteV1::Put {
                        key,
                        value: chunk.to_vec(),
                    }),
                }
            }
        }
        for (hash, bytes) in nodes {
            validate_state_node_bytes(hash, bytes)?;
            match self.graph.get(&self.key(b'n', *hash))? {
                Some(existing) if existing != *bytes => {
                    bail!("immutable AOEM state node content differs");
                }
                Some(_) => {}
                None => writes.push(AoemAtomicGraphWriteV1::Put {
                    key: self.key(b'n', *hash),
                    value: bytes.clone(),
                }),
            }
        }
        if prepared.root != empty_root() && !nodes.contains_key(&prepared.root) {
            self.require_root(prepared.root)?;
        }
        if let Some(existing) = self
            .reader()
            .load_prepared_inner(candidate, prepared.record_codec)?
        {
            if existing != prepared || !writes.is_empty() {
                bail!("completed candidate state is inconsistent or missing nodes/records");
            }
            return Ok(existing);
        }
        // Always a real step, even for an empty/no-op tree update. Reservation
        // is not completion and is invisible to load_prepared until the marker.
        writes.push(AoemAtomicGraphWriteV1::Put {
            key: self.key(b'p', candidate),
            value: descriptor.clone(),
        });
        let completion = self.completion(&descriptor);
        let graph_id = u64::from_be_bytes(completion[..8].try_into()?).max(1);
        let steps = writes
            .chunks(4)
            .map(|chunk| AoemAtomicGraphStepV1 {
                task_kind: 0,
                task_payload: Vec::new(),
                writes: chunk.to_vec(),
                event: None,
            })
            .collect();
        let committed = self.graph.commit(AoemAtomicGraphRequestV1 {
            graph_id,
            steps,
            completion_write: AoemAtomicGraphWriteV1::Put {
                key: self.key(b'c', candidate),
                value: completion,
            },
        });
        if let Err(error) = committed {
            // A late writer may still publish the descriptor/marker. Do not
            // let another adapter or process reuse the identity until exit.
            if let Some(lock) = self.writer_lock.take() {
                std::mem::forget(lock);
            }
            return Err(error).context("incremental state outcome uncertain; physical writer lock retained until process exit");
        }
        for (hash, expected) in blobs {
            for (index, chunk) in expected.chunks(RECORD_CHUNK_BYTES_V1).enumerate() {
                if self
                    .read_record_chunk(*hash, u32::try_from(index)?)?
                    .as_deref()
                    != Some(chunk)
                {
                    bail!("incremental state AOEM record readback mismatch");
                }
            }
        }
        for (hash, expected) in nodes {
            if self.read_node(hash)?.as_deref() != Some(expected.as_slice()) {
                bail!("incremental state AOEM node readback mismatch");
            }
        }
        let actual = self
            .reader()
            .load_prepared_inner(candidate, prepared.record_codec)?
            .context("AOEM root completion missing")?;
        if actual != prepared {
            bail!("AOEM incremental root completion changed");
        }
        Ok(actual)
    }
}

impl StateNodeReader for AoemStateNodesV1<'_> {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        self.reader().read_node(hash)
    }
}

impl StateRecordReader for AoemStateNodesV1<'_> {
    fn read_record_chunk(&self, hash: NodeHash, index: u32) -> Result<Option<Vec<u8>>> {
        self.reader().read_record_chunk(hash, index)
    }
}

fn codec_hash(records: bool) -> NodeHash {
    Sha256::digest(
        if records {
            STATE_RECORD_CODEC_V1
        } else {
            STATE_TREE_CODEC_V1
        }
        .as_bytes(),
    )
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_state_records::{
        read_record, stage_record_update, visit_records, RecordChange, RecordOverlayV1,
        RecordScanBudget,
    };
    use crate::native_state_tree::{read_state_value, stage_state_update, StateChange};
    use novovm_exec::{AoemRuntimeConfig, AoemStorageProviderConfigV1};

    #[test]
    fn descriptor_binds_codec_parent_input_and_execution() {
        let record = PreparedStateRootV1 {
            record_codec: false,
            candidate: [1; 32],
            parent_root: [2; 32],
            root: [3; 32],
            input_commitment: [4; 32],
            execution_commitment: [5; 32],
        };
        let bytes = record.encode();
        assert!(bytes.len() <= 512);
        assert_eq!(
            PreparedStateRootV1::decode(&bytes, [1; 32], false).unwrap(),
            record
        );
        assert!(PreparedStateRootV1::decode(&bytes, [7; 32], false).is_err());
        assert!(PreparedStateRootV1::decode(&bytes, [1; 32], true).is_err());
        let mut wrong_codec = bytes.clone();
        wrong_codec[4] ^= 1;
        assert!(PreparedStateRootV1::decode(&wrong_codec, [1; 32], false).is_err());
        assert!(PreparedStateRootV1::decode(&bytes[..bytes.len() - 1], [1; 32], false).is_err());
        let record_v2 = PreparedStateRootV1 {
            record_codec: true,
            ..record
        };
        assert!(PreparedStateRootV1::decode(&record_v2.encode(), [1; 32], false).is_err());
        assert_eq!(
            PreparedStateRootV1::decode(&record_v2.encode(), [1; 32], true).unwrap(),
            record_v2
        );
    }

    #[test]
    #[ignore = "invoked in a fresh process by real_aoem_records_survive_process_restart"]
    fn record_restart_worker() {
        let path = std::env::var_os("NOVOVM_RECORD_RESTART_TEST_PATH").expect("parent test path");
        let runtime = AoemRuntimeConfig::from_env().unwrap();
        let graph = AoemSemanticGraphStoreV1::open(
            &runtime,
            std::path::Path::new(&path),
            &AoemStorageProviderConfigV1::default(),
        )
        .unwrap();
        let mut nodes = AoemStateNodesV1::new(&graph, [11; 32]).unwrap();
        let parent = nodes.load_record_prepared([1; 32]).unwrap().unwrap();
        assert_eq!(
            read_record(&nodes, parent.root(), b"large").unwrap(),
            Some(vec![7; 1200])
        );
        assert_eq!(
            read_record(&nodes, parent.root(), b"empty").unwrap(),
            Some(vec![])
        );
        assert!(nodes.load_prepared([1; 32]).is_err());
        let update = stage_record_update(
            &nodes,
            parent.root(),
            &[
                RecordChange::Put {
                    key: b"large".to_vec(),
                    value: vec![8; 1600],
                },
                RecordChange::Put {
                    key: b"child".to_vec(),
                    value: vec![9; 1],
                },
            ],
        )
        .unwrap();
        nodes
            .persist_record_candidate([2; 32], [4; 32], [5; 32], &update)
            .unwrap();
        assert_eq!(
            graph.get(b"test-authority-head").unwrap(),
            Some(b"unchanged".to_vec())
        );
    }

    #[test]
    #[ignore = "requires bundled AOEM; explicitly runs a new-process recovery worker"]
    fn real_aoem_records_survive_process_restart() {
        let runtime = AoemRuntimeConfig::from_env().unwrap();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../artifacts/incremental-state-tests")
            .join(format!("records-{}-{nonce}", std::process::id()));
        let config = AoemStorageProviderConfigV1::default();
        let initial;
        let old_root;
        {
            let graph = AoemSemanticGraphStoreV1::open(&runtime, &path, &config).unwrap();
            let sentinel = AoemAtomicGraphWriteV1::Put {
                key: b"test-authority-head".to_vec(),
                value: b"unchanged".to_vec(),
            };
            graph
                .commit(AoemAtomicGraphRequestV1 {
                    graph_id: 1,
                    steps: vec![AoemAtomicGraphStepV1 {
                        task_kind: 0,
                        task_payload: vec![],
                        event: None,
                        writes: vec![sentinel.clone()],
                    }],
                    completion_write: sentinel,
                })
                .unwrap();
            let mut nodes = AoemStateNodesV1::new(&graph, [11; 32]).unwrap();
            // The borrowed reader remains usable while the one writer is held.
            let reader = AoemStateReaderV1::new(&graph, [11; 32]);
            assert!(reader.load_record_prepared([1; 32]).unwrap().is_none());
            let mut overlay = RecordOverlayV1::new(&reader, empty_root());
            overlay
                .stage(&[RecordChange::Put {
                    key: b"large".to_vec(),
                    value: vec![7; 1200],
                }])
                .unwrap();
            overlay
                .stage(&[RecordChange::Put {
                    key: b"empty".to_vec(),
                    value: vec![],
                }])
                .unwrap();
            initial = overlay.finish();
            old_root = initial.root();
            let prepared = nodes
                .persist_record_candidate([1; 32], [2; 32], [3; 32], &initial)
                .unwrap();
            assert_eq!(prepared.root(), old_root);
            assert_eq!(
                nodes
                    .persist_record_candidate([1; 32], [2; 32], [3; 32], &initial)
                    .unwrap(),
                prepared
            );
            assert!(nodes
                .persist_record_candidate([1; 32], [9; 32], [3; 32], &initial)
                .is_err());
            assert!(nodes.load_prepared([1; 32]).is_err());
            assert_eq!(
                graph.get(b"test-authority-head").unwrap(),
                Some(b"unchanged".to_vec())
            );
        }
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native_state_storage::tests::record_restart_worker",
                "--ignored",
                "--nocapture",
            ])
            .env("NOVOVM_RECORD_RESTART_TEST_PATH", &path)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "record restart worker failed\n{}\n{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        {
            let graph = AoemSemanticGraphStoreV1::open(&runtime, &path, &config).unwrap();
            let mut nodes = AoemStateNodesV1::new(&graph, [11; 32]).unwrap();
            let next = nodes.load_record_prepared([2; 32]).unwrap().unwrap();
            assert_eq!(next.parent_root(), old_root);
            assert_eq!(
                read_record(&nodes, old_root, b"large").unwrap(),
                Some(vec![7; 1200])
            );
            assert_eq!(
                read_record(&nodes, next.root(), b"large").unwrap(),
                Some(vec![8; 1600])
            );
            let stats = visit_records(
                &nodes,
                next.root(),
                RecordScanBudget {
                    max_nodes: 5,
                    max_records: 3,
                    max_bytes: 2000,
                },
                &mut |_, _| Ok(()),
            )
            .unwrap();
            assert_eq!(stats.records, 3);
            // A completed immutable record may not be silently repaired, even
            // when replay supplies the exact original candidate update.
            let old_blob = initial
                .blobs()
                .iter()
                .find(|(_, blob)| blob.len() > 512)
                .unwrap()
                .0;
            let missing_key = record_chunk_key([11; 32], *old_blob, 1);
            let delete = AoemAtomicGraphWriteV1::Delete {
                key: missing_key.clone(),
            };
            graph
                .commit(AoemAtomicGraphRequestV1 {
                    graph_id: 999,
                    steps: vec![AoemAtomicGraphStepV1 {
                        task_kind: 0,
                        task_payload: vec![],
                        event: None,
                        writes: vec![delete.clone()],
                    }],
                    completion_write: delete,
                })
                .unwrap();
            assert!(read_record(&nodes, old_root, b"large").is_err());
            assert!(nodes
                .persist_record_candidate([1; 32], [2; 32], [3; 32], &initial)
                .is_err());
            assert!(graph.get(&missing_key).unwrap().is_none());
            assert_eq!(
                read_record(&nodes, next.root(), b"large").unwrap(),
                Some(vec![8; 1600])
            );
            assert_eq!(
                graph.get(b"test-authority-head").unwrap(),
                Some(b"unchanged".to_vec())
            );
        }
        eprintln!("AOEM record restart artifacts: {}", path.display());
    }

    #[test]
    #[ignore = "requires the bundled AOEM persistence runtime; explicitly run"]
    fn real_aoem_incremental_nodes_survive_restart_and_more_than_workspace_limit() {
        let runtime = AoemRuntimeConfig::from_env().unwrap();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Keep verification artifacts inside the active repository, never a
        // machine-specific global database or a sibling project.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../artifacts/incremental-state-tests")
            .join(format!("{}-{nonce}", std::process::id()));
        let config = AoemStorageProviderConfigV1::default();
        let scope = [9; 32];
        let mut roots = vec![empty_root()];
        {
            let graph = AoemSemanticGraphStoreV1::open(&runtime, &path, &config).unwrap();
            let mut nodes = AoemStateNodesV1::new(&graph, scope).unwrap();
            assert!(AoemStateNodesV1::new(&graph, scope).is_err());
            assert!(AoemStateNodesV1::new(&graph, [8; 32]).is_err());
            for height in 1u8..=40 {
                let changes = [StateChange::Put {
                    key: vec![height],
                    value: vec![height],
                }];
                let update = stage_state_update(&nodes, *roots.last().unwrap(), &changes).unwrap();
                let prepared = nodes
                    .persist_candidate([height; 32], [2; 32], [3; 32], &update)
                    .unwrap();
                assert_eq!(prepared.root(), update.root());
                assert_eq!(
                    nodes
                        .persist_candidate([height; 32], [2; 32], [3; 32], &update)
                        .unwrap(),
                    prepared
                );
                assert!(nodes
                    .persist_candidate([height; 32], [4; 32], [3; 32], &update)
                    .is_err());
                roots.push(prepared.root());
            }
            // An incomplete reservation must never be returned as a completed
            // root, even though its immutable tree nodes already exist.
            let pending = PreparedStateRootV1 {
                record_codec: false,
                candidate: [50; 32],
                parent_root: roots[39],
                root: roots[40],
                input_commitment: [2; 32],
                execution_commitment: [3; 32],
            };
            graph
                .commit(AoemAtomicGraphRequestV1 {
                    graph_id: 50,
                    steps: vec![AoemAtomicGraphStepV1 {
                        task_kind: 0,
                        task_payload: vec![],
                        event: None,
                        writes: vec![AoemAtomicGraphWriteV1::Put {
                            key: nodes.key(b'p', [50; 32]),
                            value: pending.encode(),
                        }],
                    }],
                    completion_write: AoemAtomicGraphWriteV1::Put {
                        key: nodes.key(b't', [50; 32]),
                        value: vec![1],
                    },
                })
                .unwrap();
            assert!(nodes.load_prepared([50; 32]).unwrap().is_none());
            drop(nodes);
            assert!(AoemStateNodesV1::new(&graph, [8; 32])
                .unwrap()
                .load_prepared([40; 32])
                .unwrap()
                .is_none());
        }
        {
            let graph = AoemSemanticGraphStoreV1::open(&runtime, &path, &config).unwrap();
            let nodes = AoemStateNodesV1::new(&graph, scope).unwrap();
            assert!(nodes.load_prepared([50; 32]).unwrap().is_none());
            for height in 1u8..=40 {
                assert_eq!(
                    nodes.load_prepared([height; 32]).unwrap().unwrap().root(),
                    roots[height as usize]
                );
                assert_eq!(
                    read_state_value(&nodes, roots[height as usize], &[height]).unwrap(),
                    Some(vec![height])
                );
                assert_eq!(
                    read_state_value(&nodes, roots[height as usize], &[height + 1]).unwrap(),
                    None
                );
            }
        }
        eprintln!("AOEM incremental root artifacts: {}", path.display());
        // Retain only this test's isolated evidence; no broad cleanup/deletion.
    }
}
