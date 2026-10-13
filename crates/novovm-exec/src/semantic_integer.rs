//! Existing AOEM opcode 113 typed integer outcome and optional v7 resources.
//!
//! This is a bounded wire adapter, not a Host integer interpreter. AOEM validates
//! the APFLOU01 v2 graphs and runs checked i1024 on Vulkan/SPIR-V. There is no
//! CPU fallback. The v5 API remains per-request; only the explicitly guarded v7
//! owner requests resident resources. Neither API activates a product route.

use super::{
    unwrap_aoem_state_read_value_v1, AoemExecFacade, AoemExecSession, OpsWireOp, OpsWireV1Builder,
};
use anyhow::{bail, Context, Result};
use aoem_bindings::AoemExecV2Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_ROWS: usize = 4096;
const MAX_PROGRAM_BYTES: usize = 4 * 1024 * 1024;
static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Signed two's-complement i1024, in little-endian 32-bit limbs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AoemInteger1024V1(pub [u32; 32]);

impl AoemInteger1024V1 {
    pub fn from_u128(value: u128) -> Self {
        let mut limbs = [0; 32];
        for (i, limb) in limbs[..4].iter_mut().enumerate() {
            *limb = (value >> (32 * i)) as u32;
        }
        Self(limbs)
    }

    pub fn from_i128(value: i128) -> Self {
        let mut result = Self::from_u128(value as u128);
        result.0[4..].fill(if value < 0 { u32::MAX } else { 0 });
        result
    }

    /// Explicit checked projection; never truncate a negative or wide result.
    pub fn try_to_u128(self) -> Result<u128> {
        if self.0[4..].iter().any(|limb| *limb != 0) {
            bail!("integer outcome cannot be represented as u128");
        }
        Ok(self.0[..4]
            .iter()
            .enumerate()
            .fold(0, |value, (i, limb)| value | ((*limb as u128) << (32 * i))))
    }
}

/// APFLOU01 v2 / numeric_bits=1024 asset and independent row-major inputs.
/// Business policy belongs to the caller's asset, not to this generic facade.
#[derive(Clone, Debug)]
pub struct AoemIntegerOutcomeRequestV1 {
    pub program: Vec<u8>,
    pub input_count: usize,
    pub output_count: usize,
    pub rows: Vec<Vec<AoemInteger1024V1>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AoemIntegerOutcomeKindV1 {
    Value,
    OutsideDomain,
    Refuted,
    ExecutionFailure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AoemIntegerOutcomeRowV1 {
    pub outcome: AoemIntegerOutcomeKindV1,
    pub fault: u32,
    pub component: Option<usize>,
    pub failed_instruction: Option<usize>,
    /// Empty for every non-Value outcome. Rejection is not a successful value.
    pub values: Vec<AoemInteger1024V1>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AoemIntegerOutcomeBatchV1 {
    pub rows: Vec<AoemIntegerOutcomeRowV1>,
}

#[derive(Clone, Copy, Debug)]
struct Shape {
    rows: usize,
    inputs: usize,
    outputs: usize,
    domains: usize,
    refutations: usize,
    postconditions: usize,
}

fn word(bytes: &[u8], offset: usize) -> Result<usize> {
    let value = bytes
        .get(offset..offset + 4)
        .context("truncated integer asset")?;
    Ok(u32::from_le_bytes(value.try_into()?) as usize)
}

fn validate_program(p: &[u8], inputs: usize, outputs: usize, rows: usize) -> Result<Shape> {
    if p.len() < 36 || p.len() > MAX_PROGRAM_BYTES || !p.len().is_multiple_of(4) {
        bail!("integer asset size is outside APFLOU01 bounds");
    }
    if &p[..8] != b"APFLOU01" || word(p, 8)? != 2 || word(p, 32)? != 1024 {
        bail!("integer asset must be APFLOU01 v2 checked_i1024");
    }
    let domains = word(p, 20)?;
    let refutations = word(p, 24)?;
    let postconditions = word(p, 28)?;
    if inputs > 16
        || !(1..=16).contains(&outputs)
        || inputs + outputs > 16
        || domains > 16
        || refutations > 16
        || !(1..=16).contains(&postconditions)
        || inputs != word(p, 12)?
        || outputs != word(p, 16)?
        || !(1..=MAX_ROWS).contains(&rows)
    {
        bail!("integer asset shape/count mismatch");
    }
    // Validate the bounded container, not a second implementation of AOEM's SSA
    // codec. The native codec owns graph/bank/opcode/register validity.
    let mut cursor = 36;
    for _ in 0..domains + refutations + 1 + postconditions {
        let len = word(p, cursor)?;
        cursor += 4;
        let end = cursor
            .checked_add(len)
            .context("integer graph length overflow")?;
        if len == 0 || !len.is_multiple_of(4) || end > p.len() {
            bail!("integer graph length is invalid");
        }
        cursor = end;
    }
    if cursor != p.len() {
        bail!("integer asset has trailing bytes");
    }
    Ok(Shape {
        rows,
        inputs,
        outputs,
        domains,
        refutations,
        postconditions,
    })
}

fn validate_rows(rows: &[Vec<AoemInteger1024V1>], shape: &Shape) -> Result<()> {
    if rows.len() != shape.rows || rows.iter().any(|row| row.len() != shape.inputs) {
        bail!("integer input rows are outside bounds or have incorrect width");
    }
    Ok(())
}

fn append_rows(payload: &mut Vec<u8>, rows: &[Vec<AoemInteger1024V1>]) {
    for row in rows {
        for value in row {
            for limb in value.0 {
                payload.extend_from_slice(&limb.to_le_bytes());
            }
        }
    }
}

fn encode(request: &AoemIntegerOutcomeRequestV1) -> Result<(Vec<u8>, Shape)> {
    let p = &request.program;
    let shape = validate_program(
        p,
        request.input_count,
        request.output_count,
        request.rows.len(),
    )?;
    validate_rows(&request.rows, &shape)?;
    let mut payload = Vec::with_capacity(24 + p.len() + shape.rows * shape.inputs * 128);
    payload.extend_from_slice(b"AOIP0\0\0\0");
    for value in [5, request.rows.len() as u32, p.len() as u32, 0] {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    payload.extend_from_slice(p);
    append_rows(&mut payload, &request.rows);
    Ok((payload, shape))
}

fn require_complete(report: AoemExecV2Result) -> Result<()> {
    if report.processed != 1
        || report.success != 1
        || report.failed_index != u32::MAX
        || report.total_writes != 1
    {
        bail!("integer outcome execution incomplete: {report:?}");
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRow {
    outcome: u32,
    fault: u32,
    // Value (not Option) requires these keys even when the value is null.
    component: serde_json::Value,
    failed_instruction: serde_json::Value,
    values: Vec<[u32; 32]>,
}

fn optional_index(value: &serde_json::Value, bound: usize) -> Result<Option<usize>> {
    if value.is_null() {
        return Ok(None);
    }
    let index = value
        .as_u64()
        .context("integer outcome index is not an unsigned integer")?;
    if index >= bound as u64 {
        bail!("integer outcome index is outside bounds");
    }
    Ok(Some(index as usize))
}

fn validate_outcome(
    shape: &Shape,
    tag: u32,
    fault: u32,
    component: Option<usize>,
    failed: Option<usize>,
    values: usize,
) -> Result<AoemIntegerOutcomeKindV1> {
    let body = shape.domains + shape.refutations;
    if component.is_some_and(|c| c >= body + 1 + shape.postconditions)
        || failed.is_some_and(|i| i >= 512)
    {
        bail!("integer outcome index is outside bounds");
    }
    let kind = match tag {
        0 if fault == 0 && component.is_none() && failed.is_none() && values == shape.outputs => {
            AoemIntegerOutcomeKindV1::Value
        }
        1 if fault == 0 && component.is_some_and(|c| c < shape.domains) && failed.is_none() => {
            AoemIntegerOutcomeKindV1::OutsideDomain
        }
        2 if fault == 0
            && component.is_some_and(|c| c >= shape.domains && c < body)
            && failed.is_none() =>
        {
            AoemIntegerOutcomeKindV1::Refuted
        }
        3 if (fault == 0 && component.is_some_and(|c| c > body) && failed.is_none())
            || ((2..=7).contains(&fault)
                && component.is_some_and(|c| c < shape.domains || c >= body)
                && failed.is_some()) =>
        {
            AoemIntegerOutcomeKindV1::ExecutionFailure
        }
        _ => bail!("integer outcome tag/fault/component mismatch"),
    };
    if kind != AoemIntegerOutcomeKindV1::Value && values != 0 {
        bail!("integer rejected row must not contain values");
    }
    Ok(kind)
}

fn decode_binary(
    bytes: &[u8],
    id: u64,
    sequence: u64,
    shape: &Shape,
) -> Result<AoemIntegerOutcomeBatchV1> {
    if !(1..=MAX_ROWS).contains(&shape.rows) || !(1..=16).contains(&shape.outputs) {
        bail!("integer binary shape exceeds bounds");
    }
    let stride = 16 + shape.outputs * 128;
    if !(1..=MAX_ROWS).contains(&shape.rows)
        || !(1..=16).contains(&shape.outputs)
        || bytes.len() != 40 + shape.rows * stride
        || &bytes[..8] != b"AOIR0001"
        || u64::from_le_bytes(bytes[8..16].try_into()?) != id
        || id == 0
        || u64::from_le_bytes(bytes[16..24].try_into()?) != sequence
        || sequence == 0
        || word(bytes, 24)? != shape.rows
        || word(bytes, 28)? != shape.outputs
        || word(bytes, 32)? != 32
        || word(bytes, 36)? != 0
    {
        bail!("integer binary result identity/shape/length mismatch");
    }
    let mut rows = Vec::with_capacity(shape.rows);
    for raw in bytes[40..].chunks_exact(stride) {
        let tag = word(raw, 0)? as u32;
        let fault = word(raw, 4)? as u32;
        let optional = |at| -> Result<Option<usize>> {
            let n = word(raw, at)?;
            Ok((n != u32::MAX as usize).then_some(n))
        };
        let component = optional(8)?;
        let failed_instruction = optional(12)?;
        let outcome = validate_outcome(
            shape,
            tag,
            fault,
            component,
            failed_instruction,
            if tag == 0 { shape.outputs } else { 0 },
        )?;
        let values = if outcome == AoemIntegerOutcomeKindV1::Value {
            raw[16..]
                .chunks_exact(128)
                .map(|value| {
                    let mut limbs = [0; 32];
                    for (limb, raw) in limbs.iter_mut().zip(value.chunks_exact(4)) {
                        *limb = u32::from_le_bytes(raw.try_into().unwrap());
                    }
                    AoemInteger1024V1(limbs)
                })
                .collect()
        } else {
            if raw[16..].iter().any(|b| *b != 0) {
                bail!("integer binary rejected row has nonzero padding");
            }
            Vec::new()
        };
        rows.push(AoemIntegerOutcomeRowV1 {
            outcome,
            fault,
            component,
            failed_instruction,
            values,
        });
    }
    Ok(AoemIntegerOutcomeBatchV1 { rows })
}

fn decode(mut value: serde_json::Value, shape: &Shape) -> Result<AoemIntegerOutcomeBatchV1> {
    let object = value
        .as_object()
        .context("integer outcome result is not an object")?;
    if object.len() != 6
        || value["kind"] != "compute.ai.sgm_infer_v1.integer_program.result"
        || value["version"] != 5
        || value["numeric_contract"] != "checked_i1024_le_limbs_v1"
        || value["outcome_contract"] != "goal_bound_result_v1"
        || value["backend"] != "vulkan_spirv"
    {
        bail!("integer outcome result metadata mismatch");
    }
    let serde_json::Value::Array(rows) = value["rows"].take() else {
        bail!("integer outcome rows missing");
    };
    if rows.len() != shape.rows {
        bail!("integer outcome row count mismatch");
    }
    let body = shape.domains + shape.refutations;
    let components = body + 1 + shape.postconditions;
    let mut result = Vec::with_capacity(rows.len());
    for value in rows {
        // Bound before deserializing/copying the limb arrays.
        let fields = value
            .as_object()
            .context("integer outcome row is not an object")?;
        if fields.len() != 5
            || [
                "outcome",
                "fault",
                "component",
                "failed_instruction",
                "values",
            ]
            .iter()
            .any(|key| !fields.contains_key(*key))
            || value["outcome"].as_u64().is_none()
            || value["fault"].as_u64().is_none()
        {
            bail!("integer outcome row schema mismatch");
        }
        optional_index(&value["component"], components)?;
        optional_index(&value["failed_instruction"], 512)?;
        let values = value["values"]
            .as_array()
            .context("integer outcome values missing")?;
        if values.len() > shape.outputs {
            bail!("integer outcome has too many values");
        }
        for integer in values {
            let limbs = integer
                .as_array()
                .context("integer outcome limbs missing")?;
            if limbs.len() != 32
                || limbs
                    .iter()
                    .any(|limb| limb.as_u64().is_none_or(|limb| limb > u32::MAX as u64))
            {
                bail!("integer outcome must contain exactly 32 u32 limbs");
            }
        }
        let raw: RawRow = serde_json::from_value(value)?;
        let component = optional_index(&raw.component, components)?;
        let failed_instruction = optional_index(&raw.failed_instruction, 512)?;
        let outcome = validate_outcome(
            shape,
            raw.outcome,
            raw.fault,
            component,
            failed_instruction,
            raw.values.len(),
        )?;
        result.push(AoemIntegerOutcomeRowV1 {
            outcome,
            fault: raw.fault,
            component,
            failed_instruction,
            values: raw.values.into_iter().map(AoemInteger1024V1).collect(),
        });
    }
    Ok(AoemIntegerOutcomeBatchV1 { rows: result })
}

impl AoemExecSession {
    /// Execute an already encoded generic typed-outcome asset on AOEM's Vulkan
    /// backend. No new FFI, AOEM owner, Host callback or CPU fallback is created.
    /// The caller prefix is namespaced per invocation to prevent concurrent
    /// requests reading each other's result. Failed/partial execution never
    /// reaches state readback, even if an old result exists at that key.
    pub fn execute_integer_outcome_v1(
        &self,
        prefix: &str,
        request: &AoemIntegerOutcomeRequestV1,
    ) -> Result<AoemIntegerOutcomeBatchV1> {
        validate_prefix(prefix)?;
        let (payload, shape) = encode(request)?;
        let sequence = REQUEST_SEQUENCE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| anyhow::anyhow!("integer request sequence exhausted"))?;
        let prefix = format!("{prefix}/integer-outcome/{}/{sequence}", std::process::id());
        let mut wire = OpsWireV1Builder::new();
        wire.push(OpsWireOp {
            opcode: 113,
            flags: 0,
            reserved: 0,
            key: prefix.as_bytes(),
            value: &payload,
            delta: 0,
            expect_version: None,
            plan_id: 0,
        })?;
        let report = self
            .execute_ops_wire_v1(&wire.finish().bytes)
            .context("AOEM integer outcome wire execution failed (no result read)")?;
        require_complete(report)?;
        let key = format!("{prefix}/ai/sgm_infer/integer_program/result");
        let value = unwrap_aoem_state_read_value_v1(self.state_read_json_v1(&key)?, &key)?;
        decode(value, &shape)
    }
}

const MAX_RESOURCE_HANDLES: usize = 64;

/// An opaque, context-bound lease. Dropping this token does not close it; use
/// the owner's `close`, or drop the owner to release its entire native context.
#[derive(Debug)]
pub struct AoemIntegerResourceV7 {
    owner: u64,
    id: u64,
}

/// One exclusively owned wire session, with one fixed private result key.
/// Calls are synchronous and serialized by `&mut self`; use on a computation
/// owner, never the node control loop. No timeout/cancellation is fabricated.
/// A wire/readback/schema failure poisons the whole owner, including uncertain
/// Open (whose allocated id may be unknown). Drop it for explicit recovery;
/// no replacement session, retry, old result or CPU fallback is used.
///
/// ```compile_fail
/// fn requires_send<T: Send>() {}
/// requires_send::<novovm_exec::AoemIntegerResourceSessionV7>();
/// ```
pub struct AoemIntegerResourceSessionV7 {
    inner: ResourceOwner<AoemExecSession>,
    profile_supported: bool,
    binary_supported: bool,
}

/// Local measurements only. Never include these in business or consensus commitments.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AoemIntegerResourceDiagnosticsV1 {
    pub device_name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub device_type: u32,
    pub queue_family_index: u32,
    pub timestamp_valid_bits: u32,
    pub timestamp_period_ns: f64,
    pub gpu_us: Option<u64>,
    pub dispatch_us: u64,
    pub record_us: u64,
    pub submit_us: u64,
    pub wait_us: u64,
    pub upload_us: u64,
    pub read_decode_us: u64,
    pub total_us: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct AoemIntegerResourceProfileV1 {
    pub resource_id: u64,
    pub diagnostics: AoemIntegerResourceDiagnosticsV1,
    pub encode_us: u64,
    pub exchange_us: u64,
    pub decode_us: u64,
}

fn resource_supported(raw: &serde_json::Value, wire: bool, read: bool) -> bool {
    wire && read
        && raw
            .get("compute_checked_integer_resources_v7")
            .and_then(|v| v.as_bool())
            == Some(true)
}

impl AoemExecFacade {
    /// Explicit loaded-library contract only, not proof that a GPU is present.
    /// The stable C ABI/wire symbol alone does not imply AOIP0 v7 support.
    pub fn supports_checked_integer_resources_v7(&self) -> Result<bool> {
        Ok(resource_supported(
            &self.capabilities_json()?,
            self.dynlib.supports_execute_ops_wire_v1(),
            self.dynlib.supports_state_read_v1(),
        ))
    }

    /// Opens an exclusive context only after an explicit v7 capability check.
    /// Configuration must already be frozen by the embedding application.
    pub fn create_integer_resource_session_v7(
        &self,
        prefix: &str,
    ) -> Result<AoemIntegerResourceSessionV7> {
        validate_prefix(prefix)?;
        if !self.supports_checked_integer_resources_v7()? {
            bail!("loaded AOEM library does not support checked integer resources v7; no fallback");
        }
        if std::env::var_os("AOEM_PERSISTENCE_PATH").is_some_and(|value| !value.is_empty()) {
            bail!("integer resource execution requires unset or empty AOEM_PERSISTENCE_PATH");
        }
        let owner = REQUEST_SEQUENCE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| anyhow::anyhow!("integer resource owner sequence exhausted"))?;
        Ok(AoemIntegerResourceSessionV7 {
            binary_supported: self.capabilities_json()?["compute_checked_integer_binary_result_v1"]
                == true
                && self.dynlib.supports_integer_result_take_v1(),
            profile_supported: self.capabilities_json()?
                ["compute_checked_integer_resource_profile_v1"]
                == true,
            inner: ResourceOwner {
                transport: self.create_session()?,
                owner,
                prefix: format!("{prefix}/integer-resource/{}/{owner}", std::process::id()),
                resources: BTreeMap::new(),
                last_resource_id: 0,
                poisoned: false,
            },
        })
    }
}

impl AoemIntegerResourceSessionV7 {
    pub fn supports_binary_result_v1(&self) -> bool {
        self.binary_supported
    }

    /// Same resident operator; v8 numeric result bytes, no Value tree or state read.
    pub fn run_binary_v1(
        &mut self,
        resource: &AoemIntegerResourceV7,
        rows: &[Vec<AoemInteger1024V1>],
    ) -> Result<AoemIntegerOutcomeBatchV1> {
        if !self.binary_supported {
            bail!("loaded AOEM library lacks binary integer result v1; no fallback");
        }
        self.inner.run_binary(resource, rows)
    }
    /// Opens a fixed APFLOU01 v2 i1024 asset/row shape, without running inputs.
    /// AOEM owns SSA validation, GPU allocation and bounded program sharing.
    pub fn open(
        &mut self,
        program: &[u8],
        inputs: usize,
        outputs: usize,
        rows: usize,
    ) -> Result<AoemIntegerResourceV7> {
        self.inner.open(program, inputs, outputs, rows)
    }

    pub fn run(
        &mut self,
        resource: &AoemIntegerResourceV7,
        rows: &[Vec<AoemInteger1024V1>],
    ) -> Result<AoemIntegerOutcomeBatchV1> {
        self.inner.run(resource, rows)
    }

    pub fn close(&mut self, resource: &AoemIntegerResourceV7) -> Result<()> {
        self.inner.close(resource)
    }

    /// An explicitly supported diagnostic command on the same resource and wire.
    /// Older libraries are rejected before submission; ordinary v7 stays unchanged.
    pub fn run_profiled(
        &mut self,
        resource: &AoemIntegerResourceV7,
        rows: &[Vec<AoemInteger1024V1>],
    ) -> Result<(AoemIntegerOutcomeBatchV1, AoemIntegerResourceProfileV1)> {
        if !self.profile_supported {
            bail!("loaded AOEM library does not support integer resource profile v1");
        }
        self.inner.run_profiled(resource, rows)
    }
}

fn validate_prefix(prefix: &str) -> Result<()> {
    if prefix.is_empty()
        || prefix.len() > 256
        || prefix.trim() != prefix
        || prefix.ends_with('/')
        || prefix.contains('\0')
    {
        bail!("integer outcome prefix is not canonical or exceeds 256 bytes");
    }
    Ok(())
}

fn resource_header(
    command: u32,
    id: u64,
    rows: usize,
    program_bytes: usize,
    input_bytes: usize,
) -> Vec<u8> {
    // Callers have already checked all sizes against the bounded asset/shape.
    // Reserve the optional v8 sequence without growing a complete input batch.
    // These extra bytes are not emitted by released v7 commands.
    let mut payload = Vec::with_capacity(48 + program_bytes + input_bytes);
    payload.extend_from_slice(b"AOIP0\0\0\0");
    payload.extend_from_slice(&7u32.to_le_bytes());
    payload.extend_from_slice(&command.to_le_bytes());
    payload.extend_from_slice(&id.to_le_bytes());
    for value in [
        if command == 3 { 0 } else { 5 },
        rows as u32,
        program_bytes as u32,
        input_bytes as u32,
    ] {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    payload
}

fn decode_control(
    value: serde_json::Value,
    expected_id: Option<u64>,
    shape: Option<&Shape>,
) -> Result<u64> {
    let object = value
        .as_object()
        .context("integer resource control is not an object")?;
    let id = value["resource_id"]
        .as_u64()
        .filter(|id| *id != 0)
        .context("integer resource id must be a nonzero u64")?;
    if object.len() != if shape.is_some() { 8 } else { 4 }
        || value["kind"] != "compute.ai.sgm_infer_v1.integer_resource.result"
        || value["version"] != 7
        || value["command"] != if shape.is_some() { "open" } else { "close" }
        || expected_id.is_some_and(|expected| expected != id)
    {
        bail!("integer resource control metadata mismatch");
    }
    if let Some(shape) = shape {
        if value["numeric_version"] != 5
            || value["rows"].as_u64() != Some(shape.rows as u64)
            || value["input_count"].as_u64() != Some(shape.inputs as u64)
            || value["output_count"].as_u64() != Some(shape.outputs as u64)
        {
            bail!("integer resource control shape mismatch");
        }
    }
    Ok(id)
}

fn decode_resource(
    mut value: serde_json::Value,
    id: u64,
    shape: &Shape,
) -> Result<AoemIntegerOutcomeBatchV1> {
    let object = value
        .as_object_mut()
        .context("integer resource result is not an object")?;
    if object.len() != 7
        || object
            .remove("resident_resource_id")
            .and_then(|v| v.as_u64())
            != Some(id)
    {
        bail!("integer resource result id/schema mismatch");
    }
    // Run retains numeric version 5, not resource command version 7. Do not
    // relax the original v5 decoder or accept arbitrary additional metadata.
    decode(value, shape)
}

trait ResourceTransport {
    fn exchange(&mut self, prefix: &str, payload: &[u8]) -> Result<serde_json::Value>;
    fn exchange_binary(
        &mut self,
        _prefix: &str,
        _payload: &[u8],
        _id: u64,
        _sequence: u64,
    ) -> Result<Vec<u8>> {
        bail!("binary integer result transport unsupported")
    }
}

fn resource_exchange(
    prefix: &str,
    payload: &[u8],
    execute: impl FnOnce(&[u8]) -> Result<AoemExecV2Result>,
    read: impl FnOnce(&str) -> Result<serde_json::Value>,
) -> Result<serde_json::Value> {
    let mut wire = OpsWireV1Builder::new();
    wire.push(OpsWireOp {
        opcode: 113,
        flags: 0,
        reserved: 0,
        key: prefix.as_bytes(),
        value: payload,
        delta: 0,
        expect_version: None,
        plan_id: 0,
    })?;
    let report = execute(&wire.finish().bytes)
        .context("AOEM integer resource wire execution failed (no result read)")?;
    require_complete(report).context("AOEM integer resource incomplete (no result read)")?;
    let key = format!("{prefix}/ai/sgm_infer/integer_program/result");
    unwrap_aoem_state_read_value_v1(read(&key)?, &key)
}

impl ResourceTransport for AoemExecSession {
    fn exchange_binary(
        &mut self,
        prefix: &str,
        payload: &[u8],
        id: u64,
        sequence: u64,
    ) -> Result<Vec<u8>> {
        let mut wire = OpsWireV1Builder::new();
        wire.push(OpsWireOp {
            opcode: 113,
            flags: 0,
            reserved: 0,
            key: prefix.as_bytes(),
            value: payload,
            delta: 0,
            expect_version: None,
            plan_id: 0,
        })?;
        let report = self.execute_ops_wire_v1(&wire.finish().bytes)?;
        // Binary output is transient, not a state-store write.
        if report.processed != 1
            || report.success != 1
            || report.failed_index != u32::MAX
            || report.total_writes != 0
        {
            bail!("AOEM binary integer execution incomplete; no result read");
        }
        self.integer_result_take_v1(id, sequence)
    }
    fn exchange(&mut self, prefix: &str, payload: &[u8]) -> Result<serde_json::Value> {
        resource_exchange(
            prefix,
            payload,
            |wire| self.execute_ops_wire_v1(wire),
            |key| self.state_read_json_v1(key),
        )
    }
}

struct ResourceOwner<T> {
    transport: T,
    owner: u64,
    prefix: String,
    resources: BTreeMap<u64, Shape>,
    last_resource_id: u64,
    poisoned: bool,
}

impl<T: ResourceTransport> ResourceOwner<T> {
    fn run_binary(
        &mut self,
        resource: &AoemIntegerResourceV7,
        rows: &[Vec<AoemInteger1024V1>],
    ) -> Result<AoemIntegerOutcomeBatchV1> {
        let shape = self.shape(resource)?;
        validate_rows(rows, &shape)?;
        let sequence = REQUEST_SEQUENCE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| anyhow::anyhow!("integer request sequence exhausted"))?
            + 1;
        let mut payload = resource_header(
            2,
            resource.id,
            shape.rows,
            0,
            shape.rows * shape.inputs * 128,
        );
        payload[8..12].copy_from_slice(&8u32.to_le_bytes());
        payload.extend_from_slice(&sequence.to_le_bytes());
        append_rows(&mut payload, rows);
        self.poisoned = true;
        let bytes =
            self.transport
                .exchange_binary(&self.prefix, &payload, resource.id, sequence)?;
        let output = decode_binary(&bytes, resource.id, sequence, &shape)?;
        self.poisoned = false;
        Ok(output)
    }
    fn healthy(&self) -> Result<()> {
        if self.poisoned {
            bail!("integer resource owner poisoned; drop context for explicit recovery");
        }
        Ok(())
    }

    fn shape(&self, resource: &AoemIntegerResourceV7) -> Result<Shape> {
        self.healthy()?;
        if resource.owner != self.owner {
            bail!("integer resource belongs to another owner");
        }
        self.resources
            .get(&resource.id)
            .copied()
            .context("integer resource is closed or unknown")
    }

    fn exchange(&mut self, payload: &[u8]) -> Result<serde_json::Value> {
        self.healthy()?;
        // Set before entering native code; keep set through schema validation
        // and Rust unwinding, not just through the native return code.
        self.poisoned = true;
        self.transport.exchange(&self.prefix, payload)
    }

    fn open(
        &mut self,
        program: &[u8],
        inputs: usize,
        outputs: usize,
        rows: usize,
    ) -> Result<AoemIntegerResourceV7> {
        self.healthy()?;
        let shape = validate_program(program, inputs, outputs, rows)?;
        if self.resources.len() >= MAX_RESOURCE_HANDLES {
            bail!("integer resource handle bound exceeded");
        }
        let mut payload = resource_header(1, 0, rows, program.len(), 0);
        payload.extend_from_slice(program);
        let id = decode_control(self.exchange(&payload)?, None, Some(&shape))?;
        if id <= self.last_resource_id {
            bail!("integer resource id reused by loaded library");
        }
        self.last_resource_id = id;
        self.resources.insert(id, shape);
        self.poisoned = false;
        Ok(AoemIntegerResourceV7 {
            owner: self.owner,
            id,
        })
    }

    fn run(
        &mut self,
        resource: &AoemIntegerResourceV7,
        rows: &[Vec<AoemInteger1024V1>],
    ) -> Result<AoemIntegerOutcomeBatchV1> {
        let shape = self.shape(resource)?;
        validate_rows(rows, &shape)?;
        let mut payload = resource_header(
            2,
            resource.id,
            shape.rows,
            0,
            shape.rows * shape.inputs * 128,
        );
        append_rows(&mut payload, rows);
        let output = decode_resource(self.exchange(&payload)?, resource.id, &shape)?;
        // Outside-domain/refutation/arithmetic row outcomes are valid results,
        // not an uncertain transport/device failure, and may be run again.
        self.poisoned = false;
        Ok(output)
    }

    fn close(&mut self, resource: &AoemIntegerResourceV7) -> Result<()> {
        self.shape(resource)?;
        let payload = resource_header(3, resource.id, 0, 0, 0);
        decode_control(self.exchange(&payload)?, Some(resource.id), None)?;
        self.resources.remove(&resource.id);
        self.poisoned = false;
        Ok(())
    }

    fn run_profiled(
        &mut self,
        resource: &AoemIntegerResourceV7,
        rows: &[Vec<AoemInteger1024V1>],
    ) -> Result<(AoemIntegerOutcomeBatchV1, AoemIntegerResourceProfileV1)> {
        let start = std::time::Instant::now();
        let shape = self.shape(resource)?;
        validate_rows(rows, &shape)?;
        let mut payload = resource_header(
            4,
            resource.id,
            shape.rows,
            0,
            shape.rows * shape.inputs * 128,
        );
        append_rows(&mut payload, rows);
        let encode_us = start.elapsed().as_micros() as u64;
        let start = std::time::Instant::now();
        let value = self.exchange(&payload)?;
        let exchange_us = start.elapsed().as_micros() as u64;
        let start = std::time::Instant::now();
        let (output, diagnostics) = decode_profiled(value, resource.id, &shape)?;
        let decode_us = start.elapsed().as_micros() as u64;
        self.poisoned = false;
        Ok((
            output,
            AoemIntegerResourceProfileV1 {
                resource_id: resource.id,
                diagnostics,
                encode_us,
                exchange_us,
                decode_us,
            },
        ))
    }
}

fn decode_profiled(
    mut value: serde_json::Value,
    id: u64,
    shape: &Shape,
) -> Result<(AoemIntegerOutcomeBatchV1, AoemIntegerResourceDiagnosticsV1)> {
    if value.as_object().map(|o| o.len()) != Some(5)
        || value["kind"] != "compute.ai.sgm_infer_v1.integer_resource.profiled_result"
        || value["version"] != 1
        || value["resident_resource_id"].as_u64() != Some(id)
    {
        bail!("integer profiled resource id/schema mismatch");
    }
    if value["diagnostics"].as_object().map(|o| o.len()) != Some(15) {
        bail!("integer diagnostic fields mismatch");
    }
    let diagnostics: AoemIntegerResourceDiagnosticsV1 =
        serde_json::from_value(value["diagnostics"].take())?;
    if diagnostics.device_name.is_empty()
        || diagnostics.device_name.len() > 256
        || diagnostics.timestamp_valid_bits > 64
        || !diagnostics.timestamp_period_ns.is_finite()
        || diagnostics.timestamp_period_ns < 0.0
        || (diagnostics.gpu_us.is_some()
            && (diagnostics.timestamp_valid_bits == 0 || diagnostics.timestamp_period_ns <= 0.0))
    {
        bail!("integer diagnostic device/timestamp contract mismatch");
    }
    Ok((decode(value["result"].take(), shape)?, diagnostics))
}

#[cfg(test)]
#[path = "semantic_integer_tests.rs"]
mod tests;
