use super::{AoemExecFacade, AoemExecSession, AoemRuntimeConfig};
use anyhow::{bail, Context, Result};
use aoem_bindings::{
    AoemAtomicWriteRecordV1, AoemAtomicWriteSetV1, AoemGraphCallbacksV3, AoemGraphCompletionV2,
    AoemGraphSubmitOptionsV3, AoemStateEventV2, AoemTaskDescriptorV2, AoemTaskStepOutputV3,
    AOEM_ATOMIC_WRITE_DELETE_V1, AOEM_ATOMIC_WRITE_PUT_V1, AOEM_ERROR_INVALID_ARGUMENT,
    AOEM_ERROR_STATE_WRITE_FAILED, AOEM_SEMANTIC_GRAPH_ABI_V2, AOEM_STATUS_OK,
    AOEM_STEP_HAS_ATOMIC_WRITE_SET, AOEM_STEP_HAS_EVENT,
};
use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

const STORAGE_REQUEST_MAGIC_V1: &[u8; 4] = b"AOSQ";
const STORAGE_RESPONSE_MAGIC_V1: &[u8; 4] = b"AOSR";
const STORAGE_WIRE_VERSION_V1: u16 = 1;
const STORAGE_OP_OPEN_V1: u16 = 1;
const STORAGE_OP_GET_V1: u16 = 3;
const DEFAULT_COMPLETION_TIMEOUT: Duration = Duration::from_secs(30);
const CANCEL_COMPLETION_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_ATOMIC_WRITES_PER_SET_V1: usize = 4;
const MAX_ATOMIC_WRITE_KEY_BYTES_V1: usize = 96;
const MAX_ATOMIC_WRITE_VALUE_BYTES_V1: usize = 512;
const MAX_TASK_PAYLOAD_BYTES_V1: usize = 88;
const MAX_EVENT_PAYLOAD_BYTES_V1: usize = 216;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AoemStorageProviderConfigV1 {
    pub max_open_files: u32,
    pub write_buffer_bytes: u64,
    pub block_cache_bytes: u64,
    pub max_background_jobs: u32,
    pub sync_every: u32,
    pub compression: bool,
    pub writer_queue_capacity: u32,
    pub writer_max_batch_sets: u32,
}

impl Default for AoemStorageProviderConfigV1 {
    fn default() -> Self {
        Self {
            max_open_files: 256,
            write_buffer_bytes: 16 * 1024 * 1024,
            block_cache_bytes: 32 * 1024 * 1024,
            max_background_jobs: 4,
            sync_every: 1,
            compression: true,
            writer_queue_capacity: 1024,
            writer_max_batch_sets: 64,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AoemAtomicGraphWriteV1 {
    Put { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AoemAtomicGraphEventV1 {
    pub kind: u16,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AoemAtomicGraphStepV1 {
    pub task_kind: u16,
    pub task_payload: Vec<u8>,
    pub writes: Vec<AoemAtomicGraphWriteV1>,
    pub event: Option<AoemAtomicGraphEventV1>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AoemAtomicGraphRequestV1 {
    pub graph_id: u64,
    pub steps: Vec<AoemAtomicGraphStepV1>,
    pub completion_write: AoemAtomicGraphWriteV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AoemAtomicGraphCommitReportV1 {
    pub graph_id: u64,
    pub processed: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub would_block_retries: u64,
    pub peak_queued_tasks: u64,
    pub durable_event_count: u64,
}

pub struct AoemSemanticGraphStoreV1 {
    session: AoemExecSession,
    database_id: u64,
    path: PathBuf,
}

impl AoemSemanticGraphStoreV1 {
    pub fn open(
        runtime: &AoemRuntimeConfig,
        path: &Path,
        config: &AoemStorageProviderConfigV1,
    ) -> Result<Self> {
        validate_storage_config(config)?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "create AOEM semantic graph storage parent failed: {}",
                        parent.display()
                    )
                })?;
            }
        }
        let facade = AoemExecFacade::open_with_runtime(runtime)
            .context("open AOEM semantic graph V3 runtime failed")?;
        let capability = facade
            .capability_contract()
            .context("read AOEM semantic graph V3 capability contract failed")?;
        if !capability.semantic_graph_v3_ready {
            bail!(
                "AOEM semantic graph V3 host boundary is not ready: semantic_graph_v3={} domain_agnostic={} opaque_task_payload={} host_business_policy_owner={:?} atomic_step_commit={} durable_completion_boundary={}",
                capability.semantic_graph_v3,
                capability.semantic_graph_v3_domain_agnostic,
                capability.semantic_graph_v3_opaque_task_payload,
                capability.semantic_graph_v3_host_business_policy_owner,
                capability.semantic_graph_v3_atomic_step_commit,
                capability.semantic_graph_v3_durable_completion_boundary
            );
        }
        let session = facade
            .create_session()
            .context("create AOEM semantic graph V3 session failed")?;
        if !session.supports_semantic_graph_v3() {
            bail!("AOEM semantic graph V3 symbols are unavailable");
        }
        let request = encode_storage_open_request(path, config)?;
        let response = session
            .storage_provider_wire_v1(request.as_slice())
            .context("open AOEM storage provider failed")?;
        let payload = decode_storage_response(STORAGE_OP_OPEN_V1, response.as_slice())?;
        let database_id = decode_single_u64(payload, "AOEM storage provider database id")?;
        if database_id == 0 {
            bail!("AOEM storage provider returned a zero database id");
        }
        session
            .bind_semantic_atomic_writer_v1(
                database_id,
                config.writer_queue_capacity,
                config.writer_max_batch_sets,
            )
            .context("bind AOEM semantic graph atomic writer failed")?;
        Ok(Self {
            session,
            database_id,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        if key.is_empty() {
            bail!("AOEM storage provider key must not be empty");
        }
        let request = encode_storage_get_request(self.database_id, key)?;
        let response = self
            .session
            .storage_provider_wire_v1(request.as_slice())
            .context("read AOEM storage provider failed")?;
        let payload = decode_storage_response(STORAGE_OP_GET_V1, response.as_slice())?;
        decode_single_value(payload)
    }

    pub fn commit(
        &self,
        request: AoemAtomicGraphRequestV1,
    ) -> Result<AoemAtomicGraphCommitReportV1> {
        let prepared = PreparedGraphV1::new(request)?;
        let seeds = prepared.seeds.clone();
        let options = AoemGraphSubmitOptionsV3 {
            // AOEM's generic semantic-graph contract reserves one queue slot
            // beyond a runnable task and therefore admits a minimum of two.
            // Keep this domain-neutral adapter valid for legitimate one-step
            // graphs instead of forcing callers to invent a dummy task.
            max_queued_tasks: seeds.len().max(2).try_into().unwrap_or(u32::MAX),
            event_capacity: prepared.event_count.max(1).try_into().unwrap_or(u32::MAX),
            initial_event_sequence: 0,
            ..AoemGraphSubmitOptionsV3::default()
        };
        let (completion_tx, completion_rx) = mpsc::channel();
        let context = Arc::new(GraphCallbackContextV1 {
            graph_id: prepared.graph_id,
            step_write_sets: prepared.step_write_sets,
            step_events: prepared.step_events,
            completion_write_set: prepared.completion_write_set,
            durable_event_count: AtomicU64::new(0),
            completion_tx: Mutex::new(Some(completion_tx)),
        });
        let user_data = Arc::as_ptr(&context).cast_mut().cast::<c_void>();
        let callbacks = AoemGraphCallbacksV3 {
            execute: Some(execute_graph_step_v1),
            retain_context: Some(retain_graph_context_v1),
            release_context: Some(release_graph_context_v1),
            state_event: Some(deliver_graph_event_v1),
            completion_write: Some(materialize_graph_completion_write_v1),
            completion: Some(complete_graph_v1),
            user_data,
        };
        let submit_status = unsafe {
            self.session
                .submit_semantic_graph_v3(seeds.as_slice(), &options, &callbacks)
        }
        .context("submit AOEM semantic graph V3 failed")?;
        if submit_status != AOEM_STATUS_OK {
            bail!("AOEM semantic graph V3 admission returned status {submit_status}");
        }

        let completion = match completion_rx.recv_timeout(DEFAULT_COMPLETION_TIMEOUT) {
            Ok(completion) => completion,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = self.session.cancel_semantic_graph_v2(prepared.graph_id);
                match completion_rx.recv_timeout(CANCEL_COMPLETION_TIMEOUT) {
                    Ok(completion) => completion,
                    Err(error) => {
                        let graph_id = prepared.graph_id;
                        let _leaked_context = Arc::into_raw(context);
                        bail!(
                            "AOEM semantic graph V3 did not complete after cancellation: graph_id={graph_id}, error={error}"
                        );
                    }
                }
            }
            Err(error) => {
                bail!(
                    "AOEM semantic graph V3 completion channel closed: graph_id={}, error={error}",
                    prepared.graph_id
                );
            }
        };
        let durable_event_count = context.durable_event_count.load(Ordering::Acquire);
        if completion.status != AOEM_STATUS_OK {
            bail!(
                "AOEM semantic graph V3 completion failed: graph_id={}, status={}, processed={}, succeeded={}, failed={}",
                completion.graph_id,
                completion.status,
                completion.processed,
                completion.succeeded,
                completion.failed
            );
        }
        if completion.graph_id != prepared.graph_id
            || completion.processed != seeds.len() as u64
            || completion.succeeded != seeds.len() as u64
            || completion.failed != 0
        {
            bail!(
                "AOEM semantic graph V3 completion counters mismatch: graph_id={}, expected_graph_id={}, processed={}, expected_processed={}, succeeded={}, failed={}",
                completion.graph_id,
                prepared.graph_id,
                completion.processed,
                seeds.len(),
                completion.succeeded,
                completion.failed
            );
        }
        if durable_event_count != prepared.event_count as u64 {
            bail!(
                "AOEM semantic graph V3 durable event count mismatch: delivered={durable_event_count}, expected={}",
                prepared.event_count
            );
        }
        Ok(AoemAtomicGraphCommitReportV1 {
            graph_id: completion.graph_id,
            processed: completion.processed,
            succeeded: completion.succeeded,
            failed: completion.failed,
            would_block_retries: completion.would_block_retries,
            peak_queued_tasks: completion.peak_queued_tasks,
            durable_event_count,
        })
    }
}

struct PreparedGraphV1 {
    graph_id: u64,
    seeds: Vec<AoemTaskDescriptorV2>,
    step_write_sets: Vec<AoemAtomicWriteSetV1>,
    step_events: Vec<Option<AoemStateEventV2>>,
    completion_write_set: AoemAtomicWriteSetV1,
    event_count: usize,
}

impl PreparedGraphV1 {
    fn new(request: AoemAtomicGraphRequestV1) -> Result<Self> {
        if request.graph_id == 0 {
            bail!("AOEM semantic graph id must be non-zero");
        }
        if request.steps.is_empty() {
            bail!("AOEM semantic graph requires at least one step");
        }
        if request.steps.len() > u32::MAX as usize {
            bail!("AOEM semantic graph step count exceeds u32");
        }
        let step_count = request.steps.len();
        let mut seeds = Vec::with_capacity(step_count);
        let mut step_write_sets = Vec::with_capacity(step_count);
        let mut step_events = Vec::with_capacity(step_count);
        let mut next_event_sequence = 0u64;
        for (index, step) in request.steps.into_iter().enumerate() {
            if step.task_payload.len() > MAX_TASK_PAYLOAD_BYTES_V1 {
                bail!(
                    "AOEM semantic graph task payload exceeds {} bytes at step {index}",
                    MAX_TASK_PAYLOAD_BYTES_V1
                );
            }
            let task_id = index as u64 + 1;
            let mut descriptor = AoemTaskDescriptorV2 {
                task_kind: step.task_kind,
                payload_len: step.task_payload.len() as u16,
                graph_id: request.graph_id,
                task_id,
                context_handle: task_id,
                sequence: index as u64,
                ..AoemTaskDescriptorV2::default()
            };
            descriptor.payload[..step.task_payload.len()]
                .copy_from_slice(step.task_payload.as_slice());
            let write_set =
                encode_atomic_write_set(request.graph_id, index as u64, step.writes.as_slice())?;
            let event = step
                .event
                .map(|event| {
                    if event.payload.len() > MAX_EVENT_PAYLOAD_BYTES_V1 {
                        bail!(
                            "AOEM semantic graph event payload exceeds {} bytes at step {index}",
                            MAX_EVENT_PAYLOAD_BYTES_V1
                        );
                    }
                    let mut encoded = AoemStateEventV2 {
                        event_kind: event.kind,
                        payload_len: event.payload.len() as u16,
                        graph_id: request.graph_id,
                        task_id,
                        context_handle: task_id,
                        sequence: next_event_sequence,
                        ..AoemStateEventV2::default()
                    };
                    encoded.payload[..event.payload.len()]
                        .copy_from_slice(event.payload.as_slice());
                    next_event_sequence = next_event_sequence.saturating_add(1);
                    Ok(encoded)
                })
                .transpose()?;
            seeds.push(descriptor);
            step_write_sets.push(write_set);
            step_events.push(event);
        }
        let completion_sequence = step_count as u64;
        let completion_write_set = encode_atomic_write_set(
            request.graph_id,
            completion_sequence,
            std::slice::from_ref(&request.completion_write),
        )?;
        Ok(Self {
            graph_id: request.graph_id,
            seeds,
            step_write_sets,
            step_events,
            completion_write_set,
            event_count: next_event_sequence as usize,
        })
    }
}

struct GraphCallbackContextV1 {
    graph_id: u64,
    step_write_sets: Vec<AoemAtomicWriteSetV1>,
    step_events: Vec<Option<AoemStateEventV2>>,
    completion_write_set: AoemAtomicWriteSetV1,
    durable_event_count: AtomicU64,
    completion_tx: Mutex<Option<mpsc::Sender<AoemGraphCompletionV2>>>,
}

unsafe extern "C-unwind" fn execute_graph_step_v1(
    descriptor: *const AoemTaskDescriptorV2,
    output: *mut AoemTaskStepOutputV3,
    user_data: *mut c_void,
) -> i32 {
    let Some(context) = callback_context(user_data) else {
        return AOEM_ERROR_INVALID_ARGUMENT;
    };
    let Some(descriptor) = descriptor.as_ref() else {
        return AOEM_ERROR_INVALID_ARGUMENT;
    };
    let Some(output) = output.as_mut() else {
        return AOEM_ERROR_INVALID_ARGUMENT;
    };
    if descriptor.abi_version != AOEM_SEMANTIC_GRAPH_ABI_V2
        || descriptor.graph_id != context.graph_id
        || descriptor.context_handle == 0
    {
        return AOEM_ERROR_INVALID_ARGUMENT;
    }
    let index = descriptor.context_handle.saturating_sub(1) as usize;
    let Some(write_set) = context.step_write_sets.get(index) else {
        return AOEM_ERROR_INVALID_ARGUMENT;
    };
    *output = AoemTaskStepOutputV3::default();
    output.flags = AOEM_STEP_HAS_ATOMIC_WRITE_SET;
    output.atomic_write_set = *write_set;
    if let Some(event) = context.step_events.get(index).and_then(Option::as_ref) {
        output.flags |= AOEM_STEP_HAS_EVENT;
        output.event = *event;
    }
    AOEM_STATUS_OK
}

unsafe extern "C-unwind" fn retain_graph_context_v1(
    context_handle: u64,
    user_data: *mut c_void,
) -> i32 {
    validate_context_handle(context_handle, user_data)
}

unsafe extern "C-unwind" fn release_graph_context_v1(
    context_handle: u64,
    user_data: *mut c_void,
) -> i32 {
    validate_context_handle(context_handle, user_data)
}

unsafe extern "C-unwind" fn deliver_graph_event_v1(
    event: *const AoemStateEventV2,
    user_data: *mut c_void,
) -> i32 {
    let Some(context) = callback_context(user_data) else {
        return AOEM_ERROR_INVALID_ARGUMENT;
    };
    let Some(event) = event.as_ref() else {
        return AOEM_ERROR_INVALID_ARGUMENT;
    };
    if event.abi_version != AOEM_SEMANTIC_GRAPH_ABI_V2 || event.graph_id != context.graph_id {
        return AOEM_ERROR_INVALID_ARGUMENT;
    }
    context.durable_event_count.fetch_add(1, Ordering::AcqRel);
    AOEM_STATUS_OK
}

unsafe extern "C-unwind" fn materialize_graph_completion_write_v1(
    completion: *const AoemGraphCompletionV2,
    output: *mut AoemAtomicWriteSetV1,
    user_data: *mut c_void,
) -> i32 {
    let Some(context) = callback_context(user_data) else {
        return AOEM_ERROR_INVALID_ARGUMENT;
    };
    let Some(completion) = completion.as_ref() else {
        return AOEM_ERROR_INVALID_ARGUMENT;
    };
    let Some(output) = output.as_mut() else {
        return AOEM_ERROR_INVALID_ARGUMENT;
    };
    if completion.graph_id != context.graph_id || completion.status != AOEM_STATUS_OK {
        return AOEM_ERROR_STATE_WRITE_FAILED;
    }
    *output = context.completion_write_set;
    AOEM_STATUS_OK
}

unsafe extern "C-unwind" fn complete_graph_v1(
    completion: *const AoemGraphCompletionV2,
    user_data: *mut c_void,
) {
    let raw_context = user_data.cast::<GraphCallbackContextV1>();
    if raw_context.is_null() {
        return;
    }
    Arc::increment_strong_count(raw_context);
    let context = Arc::from_raw(raw_context);
    let Some(completion) = completion.as_ref() else {
        return;
    };
    let mut sender = context
        .completion_tx
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(sender) = sender.take() {
        let _ = sender.send(*completion);
    }
}

unsafe fn callback_context<'a>(user_data: *mut c_void) -> Option<&'a GraphCallbackContextV1> {
    user_data.cast::<GraphCallbackContextV1>().as_ref()
}

unsafe fn validate_context_handle(context_handle: u64, user_data: *mut c_void) -> i32 {
    let Some(context) = callback_context(user_data) else {
        return AOEM_ERROR_INVALID_ARGUMENT;
    };
    if context_handle == 0 || context_handle as usize > context.step_write_sets.len() {
        AOEM_ERROR_INVALID_ARGUMENT
    } else {
        AOEM_STATUS_OK
    }
}

fn validate_storage_config(config: &AoemStorageProviderConfigV1) -> Result<()> {
    if config.max_open_files == 0
        || config.write_buffer_bytes == 0
        || config.block_cache_bytes == 0
        || config.max_background_jobs == 0
        || config.sync_every == 0
        || config.writer_queue_capacity == 0
        || config.writer_max_batch_sets == 0
    {
        bail!("AOEM storage provider configuration values must be non-zero");
    }
    Ok(())
}

fn encode_atomic_write_set(
    graph_id: u64,
    sequence: u64,
    writes: &[AoemAtomicGraphWriteV1],
) -> Result<AoemAtomicWriteSetV1> {
    if writes.is_empty() || writes.len() > MAX_ATOMIC_WRITES_PER_SET_V1 {
        bail!(
            "AOEM atomic write set must contain 1..={} writes",
            MAX_ATOMIC_WRITES_PER_SET_V1
        );
    }
    let mut set = AoemAtomicWriteSetV1 {
        write_count: writes.len() as u16,
        stream_id: graph_id,
        sequence,
        ..AoemAtomicWriteSetV1::default()
    };
    for (index, write) in writes.iter().enumerate() {
        let (kind, key, value) = match write {
            AoemAtomicGraphWriteV1::Put { key, value } => {
                (AOEM_ATOMIC_WRITE_PUT_V1, key.as_slice(), value.as_slice())
            }
            AoemAtomicGraphWriteV1::Delete { key } => {
                (AOEM_ATOMIC_WRITE_DELETE_V1, key.as_slice(), &[][..])
            }
        };
        if key.is_empty() || key.len() > MAX_ATOMIC_WRITE_KEY_BYTES_V1 {
            bail!(
                "AOEM atomic write key must contain 1..={} bytes",
                MAX_ATOMIC_WRITE_KEY_BYTES_V1
            );
        }
        if value.len() > MAX_ATOMIC_WRITE_VALUE_BYTES_V1 {
            bail!(
                "AOEM atomic write value exceeds {} bytes",
                MAX_ATOMIC_WRITE_VALUE_BYTES_V1
            );
        }
        let mut record = AoemAtomicWriteRecordV1 {
            kind,
            key_len: key.len() as u16,
            value_len: value.len() as u16,
            ..AoemAtomicWriteRecordV1::default()
        };
        record.key[..key.len()].copy_from_slice(key);
        record.value[..value.len()].copy_from_slice(value);
        set.writes[index] = record;
    }
    Ok(set)
}

fn encode_storage_open_request(
    path: &Path,
    config: &AoemStorageProviderConfigV1,
) -> Result<Vec<u8>> {
    let path = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("AOEM storage provider path must be valid UTF-8"))?;
    if path.trim().is_empty() {
        bail!("AOEM storage provider path must not be empty");
    }
    let mut payload = Vec::new();
    push_bytes(&mut payload, path.as_bytes())?;
    payload.extend_from_slice(&config.max_open_files.to_le_bytes());
    payload.extend_from_slice(&config.write_buffer_bytes.to_le_bytes());
    payload.extend_from_slice(&config.block_cache_bytes.to_le_bytes());
    payload.extend_from_slice(&config.max_background_jobs.to_le_bytes());
    payload.extend_from_slice(&config.sync_every.to_le_bytes());
    payload.push(u8::from(config.compression));
    encode_storage_request(STORAGE_OP_OPEN_V1, payload)
}

fn encode_storage_get_request(database_id: u64, key: &[u8]) -> Result<Vec<u8>> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&database_id.to_le_bytes());
    payload.extend_from_slice(&0u64.to_le_bytes());
    push_bytes(&mut payload, key)?;
    encode_storage_request(STORAGE_OP_GET_V1, payload)
}

fn encode_storage_request(opcode: u16, payload: Vec<u8>) -> Result<Vec<u8>> {
    let payload_len =
        u32::try_from(payload.len()).context("AOEM storage request payload exceeds u32")?;
    let mut request = Vec::with_capacity(12 + payload.len());
    request.extend_from_slice(STORAGE_REQUEST_MAGIC_V1);
    request.extend_from_slice(&STORAGE_WIRE_VERSION_V1.to_le_bytes());
    request.extend_from_slice(&opcode.to_le_bytes());
    request.extend_from_slice(&payload_len.to_le_bytes());
    request.extend_from_slice(payload.as_slice());
    Ok(request)
}

fn decode_storage_response(expected_opcode: u16, response: &[u8]) -> Result<&[u8]> {
    if response.len() < 16 || response.get(..4) != Some(STORAGE_RESPONSE_MAGIC_V1.as_slice()) {
        bail!("AOEM storage provider response header is invalid");
    }
    let version = read_u16(response, 4)?;
    let opcode = read_u16(response, 6)?;
    let status = read_i32(response, 8)?;
    let payload_len = read_u32(response, 12)? as usize;
    if version != STORAGE_WIRE_VERSION_V1 || opcode != expected_opcode {
        bail!(
            "AOEM storage provider response contract mismatch: version={version}, opcode={opcode}, expected_opcode={expected_opcode}"
        );
    }
    if payload_len != response.len().saturating_sub(16) {
        bail!("AOEM storage provider response payload length mismatch");
    }
    let payload = &response[16..];
    if status != AOEM_STATUS_OK {
        let detail = String::from_utf8_lossy(payload);
        bail!("AOEM storage provider operation failed: status={status}, detail={detail}");
    }
    Ok(payload)
}

fn decode_single_value(payload: &[u8]) -> Result<Option<Vec<u8>>> {
    if payload.len() < 9 {
        bail!("AOEM storage provider get response is truncated");
    }
    if read_u32(payload, 0)? != 1 {
        bail!("AOEM storage provider get response count is not one");
    }
    let found = payload[4];
    let value_len = read_u32(payload, 5)? as usize;
    if payload.len() != 9usize.saturating_add(value_len) {
        bail!("AOEM storage provider get value length mismatch");
    }
    match found {
        0 if value_len == 0 => Ok(None),
        1 => Ok(Some(payload[9..].to_vec())),
        _ => bail!("AOEM storage provider get response found flag is invalid"),
    }
}

fn decode_single_u64(payload: &[u8], label: &str) -> Result<u64> {
    if payload.len() != 8 {
        bail!("{label} response length must be 8 bytes");
    }
    Ok(u64::from_le_bytes(
        payload.try_into().expect("fixed length"),
    ))
}

fn push_bytes(output: &mut Vec<u8>, value: &[u8]) -> Result<()> {
    let len = u32::try_from(value.len()).context("AOEM storage wire byte field exceeds u32")?;
    output.extend_from_slice(&len.to_le_bytes());
    output.extend_from_slice(value);
    Ok(())
}

fn read_u16(input: &[u8], offset: usize) -> Result<u16> {
    let bytes = input
        .get(offset..offset.saturating_add(2))
        .context("AOEM storage response is truncated")?;
    Ok(u16::from_le_bytes(bytes.try_into().expect("fixed length")))
}

fn read_u32(input: &[u8], offset: usize) -> Result<u32> {
    let bytes = input
        .get(offset..offset.saturating_add(4))
        .context("AOEM storage response is truncated")?;
    Ok(u32::from_le_bytes(bytes.try_into().expect("fixed length")))
}

fn read_i32(input: &[u8], offset: usize) -> Result<i32> {
    let bytes = input
        .get(offset..offset.saturating_add(4))
        .context("AOEM storage response is truncated")?;
    Ok(i32::from_le_bytes(bytes.try_into().expect("fixed length")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_wire_open_and_get_codecs_match_public_contract() {
        let request = encode_storage_open_request(
            Path::new("artifacts/test-provider.rocksdb"),
            &AoemStorageProviderConfigV1::default(),
        )
        .expect("encode open");
        assert_eq!(&request[..4], b"AOSQ");
        assert_eq!(read_u16(&request, 4).expect("version"), 1);
        assert_eq!(read_u16(&request, 6).expect("opcode"), 1);

        let mut get_payload = Vec::new();
        get_payload.extend_from_slice(&1u32.to_le_bytes());
        get_payload.push(1);
        get_payload.extend_from_slice(&3u32.to_le_bytes());
        get_payload.extend_from_slice(b"abc");
        assert_eq!(
            decode_single_value(get_payload.as_slice()).expect("decode get"),
            Some(b"abc".to_vec())
        );
    }

    #[test]
    fn graph_lowering_is_domain_neutral_and_bounded() {
        let prepared = PreparedGraphV1::new(AoemAtomicGraphRequestV1 {
            graph_id: 9,
            steps: vec![AoemAtomicGraphStepV1 {
                task_kind: 7,
                task_payload: b"opaque".to_vec(),
                writes: vec![AoemAtomicGraphWriteV1::Put {
                    key: b"k".to_vec(),
                    value: b"v".to_vec(),
                }],
                event: Some(AoemAtomicGraphEventV1 {
                    kind: 8,
                    payload: b"durable".to_vec(),
                }),
            }],
            completion_write: AoemAtomicGraphWriteV1::Put {
                key: b"head".to_vec(),
                value: b"done".to_vec(),
            },
        })
        .expect("prepare graph");
        assert_eq!(prepared.graph_id, 9);
        assert_eq!(prepared.seeds.len(), 1);
        assert_eq!(prepared.event_count, 1);
        assert_eq!(prepared.step_write_sets[0].stream_id, 9);
        assert_eq!(prepared.completion_write_set.sequence, 1);
    }

    #[test]
    fn graph_lowering_rejects_oversized_host_values() {
        let result = PreparedGraphV1::new(AoemAtomicGraphRequestV1 {
            graph_id: 1,
            steps: vec![AoemAtomicGraphStepV1 {
                task_kind: 1,
                task_payload: Vec::new(),
                writes: vec![AoemAtomicGraphWriteV1::Put {
                    key: b"k".to_vec(),
                    value: vec![0; MAX_ATOMIC_WRITE_VALUE_BYTES_V1 + 1],
                }],
                event: None,
            }],
            completion_write: AoemAtomicGraphWriteV1::Put {
                key: b"head".to_vec(),
                value: b"done".to_vec(),
            },
        });
        assert!(result.is_err());
    }
}
