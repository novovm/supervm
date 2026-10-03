//! Existing AOEM opcode 113 / AOIP0 v5 typed integer outcome surface.
//!
//! This is a bounded wire adapter, not a Host integer interpreter. AOEM validates
//! the APFLOU01 v2 graphs and runs checked i1024 on Vulkan/SPIR-V. There is no
//! CPU fallback. Reusing the execution session does not imply that this SDK's
//! per-request GPU device/program allocations are resident across calls.

use super::{unwrap_aoem_state_read_value_v1, AoemExecSession, OpsWireOp, OpsWireV1Builder};
use anyhow::{bail, Context, Result};
use aoem_bindings::AoemExecV2Result;
use serde::Deserialize;
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

struct Shape {
    rows: usize,
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

fn encode(request: &AoemIntegerOutcomeRequestV1) -> Result<(Vec<u8>, Shape)> {
    let p = &request.program;
    if p.len() < 36 || p.len() > MAX_PROGRAM_BYTES || !p.len().is_multiple_of(4) {
        bail!("integer asset size is outside APFLOU01 bounds");
    }
    if &p[..8] != b"APFLOU01" || word(p, 8)? != 2 || word(p, 32)? != 1024 {
        bail!("integer asset must be APFLOU01 v2 checked_i1024");
    }
    let inputs = word(p, 12)?;
    let outputs = word(p, 16)?;
    let domains = word(p, 20)?;
    let refutations = word(p, 24)?;
    let postconditions = word(p, 28)?;
    if inputs > 16
        || !(1..=16).contains(&outputs)
        || inputs + outputs > 16
        || domains > 16
        || refutations > 16
        || !(1..=16).contains(&postconditions)
        || inputs != request.input_count
        || outputs != request.output_count
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
    if request.rows.is_empty()
        || request.rows.len() > MAX_ROWS
        || request.rows.iter().any(|row| row.len() != inputs)
    {
        bail!("integer input rows are outside bounds or have incorrect width");
    }
    let mut payload = Vec::with_capacity(24 + p.len() + request.rows.len() * inputs * 128);
    payload.extend_from_slice(b"AOIP0\0\0\0");
    for value in [5, request.rows.len() as u32, p.len() as u32, 0] {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    payload.extend_from_slice(p);
    for row in &request.rows {
        for value in row {
            for limb in value.0 {
                payload.extend_from_slice(&limb.to_le_bytes());
            }
        }
    }
    Ok((
        payload,
        Shape {
            rows: request.rows.len(),
            outputs,
            domains,
            refutations,
            postconditions,
        },
    ))
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

fn decode(value: serde_json::Value, shape: &Shape) -> Result<AoemIntegerOutcomeBatchV1> {
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
    let rows = value["rows"]
        .as_array()
        .context("integer outcome rows missing")?;
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
        let raw: RawRow = serde_json::from_value(value.clone())?;
        let component = optional_index(&raw.component, components)?;
        let failed_instruction = optional_index(&raw.failed_instruction, 512)?;
        let outcome = match raw.outcome {
            0 if raw.fault == 0
                && component.is_none()
                && failed_instruction.is_none()
                && raw.values.len() == shape.outputs =>
            {
                AoemIntegerOutcomeKindV1::Value
            }
            1 if raw.fault == 0
                && component.is_some_and(|c| c < shape.domains)
                && failed_instruction.is_none() =>
            {
                AoemIntegerOutcomeKindV1::OutsideDomain
            }
            2 if raw.fault == 0
                && component.is_some_and(|c| c >= shape.domains && c < body)
                && failed_instruction.is_none() =>
            {
                AoemIntegerOutcomeKindV1::Refuted
            }
            3 if (raw.fault == 0
                && component.is_some_and(|c| c > body)
                && failed_instruction.is_none())
                || ((2..=7).contains(&raw.fault)
                    && component.is_some_and(|c| c < shape.domains || c >= body)
                    && failed_instruction.is_some()) =>
            {
                AoemIntegerOutcomeKindV1::ExecutionFailure
            }
            _ => bail!("integer outcome tag/fault/component mismatch"),
        };
        if outcome != AoemIntegerOutcomeKindV1::Value && !raw.values.is_empty() {
            bail!("integer rejected row must not contain values");
        }
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
        if prefix.is_empty()
            || prefix.len() > 256
            || prefix.trim() != prefix
            || prefix.ends_with('/')
            || prefix.contains('\0')
        {
            bail!("integer outcome prefix is not canonical or exceeds 256 bytes");
        }
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

#[cfg(test)]
#[path = "semantic_integer_tests.rs"]
mod tests;
