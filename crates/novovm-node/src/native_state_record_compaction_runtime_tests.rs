//! Explicit real-provider coverage for opt-in staged-content compaction.
//! This exercises immutable records, not candidate authority or BFT finality.

use super::{read_record, stage_record_update, RecordChange, RecordOverlayV1, StagedRecordUpdate};
use crate::native_state_records::{StateRecordReader, RECORD_CHUNK_BYTES_V1};
use crate::native_state_storage::{AoemStateNodesV1, AoemStateReaderV1};
use crate::native_state_tree::{empty_root, NodeHash};
use anyhow::{Context, Result};
use novovm_exec::{
    AoemAtomicGraphRequestV1, AoemAtomicGraphStepV1, AoemAtomicGraphWriteV1, AoemRuntimeConfig,
    AoemSemanticGraphStoreV1, AoemStorageProviderConfigV1,
};

const SCOPE: NodeHash = [0xc7; 32];
const PARENT: NodeHash = [1; 32];
const CHILD: NodeHash = [2; 32];
const INPUT: NodeHash = [3; 32];
const EXECUTION: NodeHash = [4; 32];
const VALUE_BYTES: usize = RECORD_CHUNK_BYTES_V1 * 3 + 37;
const CHILD_BYTE: u8 = 24;
const WORKER_PATH_ENV: &str = "NOVOVM_RECORD_COMPACTION_RESTART_TEST_PATH";
const WORKER: &str =
    "native_state_records::compaction_runtime_tests::record_compaction_restart_worker_v1";

fn put(key: &[u8], value: Vec<u8>) -> RecordChange {
    RecordChange::Put {
        key: key.to_vec(),
        value,
    }
}

fn child_update(
    reader: &dyn StateRecordReader,
    parent: NodeHash,
    compact: bool,
) -> Result<StagedRecordUpdate> {
    let mut overlay = RecordOverlayV1::new(reader, parent);
    for revision in 1..=CHILD_BYTE {
        overlay.stage(&[
            put(b"mutable", vec![revision; VALUE_BYTES]),
            put(b"transient", vec![revision; 600]),
        ])?;
    }
    overlay.stage(&[
        RecordChange::Delete {
            key: b"transient".to_vec(),
        },
        RecordChange::Delete {
            key: b"removed".to_vec(),
        },
        put(b"new", b"final-value".to_vec()),
    ])?;
    if compact {
        overlay.finish_compacted()
    } else {
        Ok(overlay.finish())
    }
}

// Test-only physical addressing for exact fault injection. Production access
// still uses the public record/storage APIs. Match the existing NST1 codec;
// every key is confined to this test's scope in its unique temporary database.
fn physical_key(kind: u8, hash: NodeHash) -> Vec<u8> {
    let mut key = b"NST1".to_vec();
    key.extend_from_slice(&SCOPE);
    key.push(kind);
    key.extend_from_slice(&hash);
    key
}

fn chunk_key(hash: NodeHash, index: u32) -> Vec<u8> {
    let mut key = physical_key(b'b', hash);
    key.extend_from_slice(&index.to_be_bytes());
    key
}

fn chunk_count(update: &StagedRecordUpdate) -> usize {
    update
        .blobs()
        .values()
        .map(|bytes| bytes.len().div_ceil(RECORD_CHUNK_BYTES_V1))
        .sum()
}

fn assert_values(reader: &dyn StateRecordReader, parent: NodeHash, child: NodeHash) {
    assert_eq!(
        read_record(reader, parent, b"mutable").unwrap(),
        Some(vec![7; VALUE_BYTES])
    );
    assert_eq!(
        read_record(reader, child, b"mutable").unwrap(),
        Some(vec![CHILD_BYTE; VALUE_BYTES])
    );
    for root in [parent, child] {
        assert_eq!(
            read_record(reader, root, b"stable").unwrap(),
            Some(vec![11; 31])
        );
        assert_eq!(read_record(reader, root, b"transient").unwrap(), None);
    }
    assert_eq!(
        read_record(reader, parent, b"removed").unwrap(),
        Some(vec![9; 15])
    );
    assert_eq!(read_record(reader, child, b"removed").unwrap(), None);
    assert_eq!(read_record(reader, parent, b"new").unwrap(), None);
    assert_eq!(
        read_record(reader, child, b"new").unwrap(),
        Some(b"final-value".to_vec())
    );
}

fn missing_chunk(update: &StagedRecordUpdate) -> (Vec<u8>, Vec<u8>) {
    let (hash, blob) = update
        .blobs()
        .iter()
        .find(|(_, bytes)| bytes.len() > 2 * RECORD_CHUNK_BYTES_V1)
        .expect("the surviving mutable record must be multi-chunk");
    (
        chunk_key(*hash, 1),
        blob[RECORD_CHUNK_BYTES_V1..2 * RECORD_CHUNK_BYTES_V1].to_vec(),
    )
}

#[test]
#[ignore = "only invoked with an explicit path by the real compaction restart test"]
fn record_compaction_restart_worker_v1() -> Result<()> {
    // Missing environment is a failure, never a silent pass for --include-ignored.
    let path = std::env::var_os(WORKER_PATH_ENV).context("parent compaction test path required")?;
    let runtime = AoemRuntimeConfig::from_env()?;
    let graph = AoemSemanticGraphStoreV1::open(
        &runtime,
        std::path::Path::new(&path),
        &AoemStorageProviderConfigV1::default(),
    )?;
    let mut storage = AoemStateNodesV1::new(&graph, SCOPE)?;
    let parent = storage
        .load_record_prepared(PARENT)?
        .context("parent completion missing")?;
    let child = storage
        .load_record_prepared(CHILD)?
        .context("child completion missing")?;
    assert_eq!(child.parent_root(), parent.root());
    assert_eq!(child.input_commitment(), INPUT);
    assert_eq!(child.execution_commitment(), EXECUTION);
    assert_values(&storage, parent.root(), child.root());

    let reader = AoemStateReaderV1::new(&graph, SCOPE);
    let compacted = child_update(&reader, parent.root(), true)?;
    let full = child_update(&reader, parent.root(), false)?;
    assert_eq!(compacted.root(), child.root());
    assert_eq!(full.root(), child.root());
    assert_eq!(
        storage.persist_record_candidate(CHILD, INPUT, EXECUTION, &compacted)?,
        child,
        "same-version compact replay must be exactly idempotent after restart"
    );

    let descriptor_key = physical_key(b'p', CHILD);
    let completion_key = physical_key(b'c', CHILD);
    let descriptor = graph.get(&descriptor_key)?.context("descriptor missing")?;
    let completion = graph.get(&completion_key)?.context("completion missing")?;
    let mut omitted_node = None;
    for hash in full
        .nodes()
        .keys()
        .filter(|hash| !compacted.nodes().contains_key(*hash))
    {
        let key = physical_key(b'n', *hash);
        if graph.get(&key)?.is_none() {
            omitted_node = Some(key);
            break;
        }
    }
    let omitted_node = omitted_node.context("expected an unpublished intermediate node")?;
    // A legacy unpruned replay may demand intermediate content that was never
    // published. Do not relax the completed-content invariant to repair it.
    let full_replay = storage.persist_record_candidate(CHILD, INPUT, EXECUTION, &full);
    assert!(full_replay.is_err());
    assert!(format!("{:#}", full_replay.unwrap_err()).contains("completed candidate state"));
    assert_eq!(
        graph.get(&omitted_node)?,
        None,
        "full replay must not backfill an omitted intermediate node"
    );
    assert_eq!(graph.get(&descriptor_key)?, Some(descriptor.clone()));
    assert_eq!(graph.get(&completion_key)?, Some(completion.clone()));
    assert_values(&storage, parent.root(), child.root());

    let (key, expected_chunk) = missing_chunk(&compacted);
    assert_eq!(graph.get(&key)?, Some(expected_chunk));
    let deletion = AoemAtomicGraphWriteV1::Delete { key: key.clone() };
    graph.commit(AoemAtomicGraphRequestV1 {
        graph_id: 0xc701,
        steps: vec![AoemAtomicGraphStepV1 {
            task_kind: 0,
            task_payload: Vec::new(),
            writes: vec![deletion.clone()],
            event: None,
        }],
        completion_write: deletion,
    })?;
    assert!(read_record(&storage, child.root(), b"mutable").is_err());
    let rejected = storage.persist_record_candidate(CHILD, INPUT, EXECUTION, &compacted);
    assert!(rejected.is_err());
    assert!(format!("{:#}", rejected.unwrap_err()).contains("completed candidate state"));
    assert_eq!(
        graph.get(&key)?,
        None,
        "completed replay must not repair the missing chunk"
    );
    assert_eq!(graph.get(&descriptor_key)?, Some(descriptor));
    assert_eq!(graph.get(&completion_key)?, Some(completion));
    assert_eq!(
        read_record(&storage, parent.root(), b"mutable")?,
        Some(vec![7; VALUE_BYTES]),
        "child corruption and compaction must not delete the durable parent"
    );
    Ok(())
}

#[test]
#[ignore = "requires bundled AOEM; runs an independent process and preserves fault artifacts"]
fn real_aoem_compacted_records_survive_restart_and_reject_missing_completed_content() -> Result<()>
{
    let runtime = AoemRuntimeConfig::from_env()?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts/incremental-state-tests")
        .join(format!("record-compaction-{}-{nonce}", std::process::id()));
    let config = AoemStorageProviderConfigV1::default();
    {
        let graph = AoemSemanticGraphStoreV1::open(&runtime, &path, &config)?;
        let mut storage = AoemStateNodesV1::new(&graph, SCOPE)?;
        let initial = stage_record_update(
            &storage,
            empty_root(),
            &[
                put(b"stable", vec![11; 31]),
                put(b"mutable", vec![7; VALUE_BYTES]),
                put(b"removed", vec![9; 15]),
            ],
        )?;
        let parent = storage.persist_record_candidate(PARENT, [5; 32], [6; 32], &initial)?;
        let reader = AoemStateReaderV1::new(&graph, SCOPE);
        let full = child_update(&reader, parent.root(), false)?;
        let compacted = child_update(&reader, parent.root(), true)?;
        assert_eq!(full.parent_root(), compacted.parent_root());
        assert_eq!(full.root(), compacted.root());
        assert!(compacted.nodes().len() < full.nodes().len());
        assert!(compacted.blobs().len() < full.blobs().len());
        assert!(chunk_count(&compacted) < chunk_count(&full));

        let mut omitted = Vec::new();
        for hash in full
            .nodes()
            .keys()
            .filter(|hash| !compacted.nodes().contains_key(*hash))
        {
            let key = physical_key(b'n', *hash);
            if graph.get(&key)?.is_none() {
                omitted.push(key);
            }
        }
        let omitted_nodes = omitted.len();
        for (hash, bytes) in full
            .blobs()
            .iter()
            .filter(|(hash, _)| !compacted.blobs().contains_key(*hash))
        {
            for index in 0..bytes.len().div_ceil(RECORD_CHUNK_BYTES_V1) {
                let key = chunk_key(*hash, u32::try_from(index)?);
                if graph.get(&key)?.is_none() {
                    omitted.push(key);
                }
            }
        }
        let omitted_chunks = omitted.len() - omitted_nodes;
        assert!(omitted_nodes > 0 && omitted_chunks > 0);
        let child = storage.persist_record_candidate(CHILD, INPUT, EXECUTION, &compacted)?;
        assert_eq!(child.root(), full.root());
        assert_values(&storage, parent.root(), child.root());
        for key in &omitted {
            assert_eq!(
                graph.get(key)?,
                None,
                "unreachable staged content must not be written"
            );
        }
        eprintln!(
            "real AOEM compaction: staged_nodes={}->{} staged_blobs={}->{} staged_chunks={}->{} omitted_new_nodes={} omitted_new_chunks={} roots_equal=true",
            full.nodes().len(), compacted.nodes().len(), full.blobs().len(), compacted.blobs().len(),
            chunk_count(&full), chunk_count(&compacted), omitted_nodes, omitted_chunks
        );
    }
    let child = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", WORKER, "--ignored", "--nocapture"])
        .env(WORKER_PATH_ENV, &path)
        .output()?;
    assert!(
        child.status.success(),
        "compaction restart worker failed\n{}\n{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    {
        // A second reopen confirms that the rejection was not an in-memory
        // illusion and that the missing completed chunk remains missing.
        let graph = AoemSemanticGraphStoreV1::open(&runtime, &path, &config)?;
        let mut storage = AoemStateNodesV1::new(&graph, SCOPE)?;
        let parent = storage
            .load_record_prepared(PARENT)?
            .context("parent completion missing")?;
        let child = storage
            .load_record_prepared(CHILD)?
            .context("child completion missing")?;
        let reader = AoemStateReaderV1::new(&graph, SCOPE);
        let compacted = child_update(&reader, parent.root(), true)?;
        let (key, _) = missing_chunk(&compacted);
        assert_eq!(graph.get(&key)?, None);
        assert!(read_record(&storage, child.root(), b"mutable").is_err());
        assert!(storage
            .persist_record_candidate(CHILD, INPUT, EXECUTION, &compacted)
            .is_err());
        assert_eq!(graph.get(&key)?, None);
        assert_eq!(
            read_record(&storage, parent.root(), b"mutable")?,
            Some(vec![7; VALUE_BYTES])
        );
    }
    eprintln!(
        "AOEM compaction restart/fault artifacts: {}",
        path.display()
    );
    Ok(())
}
