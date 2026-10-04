use anyhow::{bail, Context, Result};
use aoem_bindings::{default_host_dll_path, AoemDyn};
use serde_json::{json, Value};
use std::path::PathBuf;

const FIXED_PROFILE_RESIDENT_PROOF_V1_ID: u32 = 1;
const RETIRED_PRIVATE_PROFILE_ID: u32 = 3;
const ENVELOPE_VERIFICATION_SCOPE: &str = "envelope_integrity_only_not_zk";
const PROOF_OUTPUT_SUFFIXES: [&str; 5] = [
    "bytes",
    "status",
    "metadata",
    "public_outputs",
    "verify_status",
];

fn push_u8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn append_wire_op(out: &mut Vec<u8>, opcode: u8, key: &str, value: &[u8]) -> Result<()> {
    let key_len = u32::try_from(key.len()).context("wire key too large")?;
    let value_len = u32::try_from(value.len()).context("wire value too large")?;
    out.extend_from_slice(b"AOV2\0");
    push_u16(out, 1);
    push_u16(out, 0);
    push_u32(out, 1);
    push_u8(out, opcode);
    push_u8(out, 0);
    push_u16(out, 0);
    push_u32(out, key_len);
    push_u32(out, value_len);
    push_i64(out, 0);
    push_u64(out, u64::MAX);
    push_u64(out, 0);
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(value);
    Ok(())
}

fn build_proof_payload(request_id: &str, output_prefix: &str, profile_id: u32) -> Result<Vec<u8>> {
    let public_input = b"supervm:public:proof-engine:v1";
    let witness = b"supervm:witness:proof-engine:v1:\x11\x22\x33\x44\x55\x66\x77\x88";
    let request_id_len = u16::try_from(request_id.len()).context("request id too large")?;
    let output_prefix_len =
        u16::try_from(output_prefix.len()).context("output prefix too large")?;
    let public_input_len = u32::try_from(public_input.len()).context("public input too large")?;
    let witness_len = u32::try_from(witness.len()).context("witness too large")?;

    let mut payload = Vec::new();
    payload.extend_from_slice(b"AOFP\0");
    push_u16(&mut payload, 2);
    push_u16(&mut payload, 1 | 2 | 4 | 8);
    push_u8(&mut payload, 4);
    payload.extend_from_slice(&[0, 0, 0]);
    push_u16(&mut payload, request_id_len);
    push_u16(&mut payload, output_prefix_len);
    push_u32(&mut payload, profile_id);
    push_u32(&mut payload, 0xA0E0_5051);
    push_u32(&mut payload, 0xA0E0_9EED);
    push_u32(&mut payload, 256);
    push_u32(&mut payload, 1);
    push_u32(&mut payload, 2);
    push_u32(&mut payload, 16);
    push_u32(&mut payload, public_input_len);
    push_u32(&mut payload, witness_len);
    payload.extend_from_slice(request_id.as_bytes());
    payload.extend_from_slice(output_prefix.as_bytes());
    payload.extend_from_slice(public_input);
    payload.extend_from_slice(witness);
    Ok(payload)
}

fn build_proof_wire(request_id: &str, output_prefix: &str, profile_id: u32) -> Result<Vec<u8>> {
    let payload = build_proof_payload(request_id, output_prefix, profile_id)?;
    let mut wire = Vec::new();
    append_wire_op(&mut wire, 98, output_prefix, &payload)?;
    Ok(wire)
}

fn read_state_value(dynlib: &AoemDyn, key: &str) -> Result<Option<Value>> {
    let response = dynlib.state_read_json_v1(key)?;
    if response.get("status").and_then(Value::as_str) != Some("ok")
        || response.get("status_code").and_then(Value::as_i64) != Some(0)
    {
        bail!("state read did not succeed: {key}; response={response}");
    }
    let entry = response
        .get("value")
        .and_then(Value::as_object)
        .with_context(|| format!("missing state entry: {key}; response={response}"))?;
    if entry.get("key").and_then(Value::as_str) != Some(key) {
        bail!("state key mismatch: {key}; response={response}");
    }
    match entry.get("found").and_then(Value::as_bool) {
        Some(false) => Ok(None),
        Some(true) => {
            let value = entry
                .get("value")
                .filter(|value| value.is_object())
                .with_context(|| format!("missing state object: {key}; response={response}"))?;
            Ok(Some(value.clone()))
        }
        None => bail!("missing boolean state found field: {key}; response={response}"),
    }
}

fn require_fields(value: &Value, key: &str, fields: &[(&str, Value)]) -> Result<()> {
    for (field, expected) in fields {
        if value.get(*field) != Some(expected) {
            bail!("state key {key} expected {field}={expected}; value={value}");
        }
    }
    Ok(())
}

fn require_state_fields(dynlib: &AoemDyn, key: &str, fields: &[(&str, Value)]) -> Result<Value> {
    let value =
        read_state_value(dynlib, key)?.with_context(|| format!("state key not found: {key}"))?;
    require_fields(&value, key, fields)?;
    Ok(value)
}

fn require_outputs_absent(dynlib: &AoemDyn, output_prefix: &str) -> Result<()> {
    for suffix in PROOF_OUTPUT_SUFFIXES {
        let key = format!("{output_prefix}/zk/proof/{suffix}");
        if read_state_value(dynlib, &key)?.is_some() {
            bail!("retired profile unexpectedly has an output: {key}");
        }
    }
    Ok(())
}

fn parse_dll_arg() -> PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--dll" || arg == "--library" {
            if let Some(path) = args.next() {
                return PathBuf::from(path);
            }
        } else if !arg.starts_with("--") {
            return PathBuf::from(arg);
        }
    }
    default_host_dll_path()
}

fn main() -> Result<()> {
    let dll_path = parse_dll_arg();
    let dynlib = unsafe { AoemDyn::load(&dll_path) }
        .with_context(|| format!("failed to load AOEM library: {}", dll_path.display()))?;
    if !dynlib.supports_proof_engine_v1() {
        bail!("loaded AOEM library does not support proof engine wire/state_read path");
    }

    let request_id = "supervm-proof-engine-host-smoke";
    let output_prefix = "aoem.compute.output/supervm-proof-engine-host-smoke";
    let proof_key = format!("{output_prefix}/zk/proof/bytes");
    let status_key = format!("{output_prefix}/zk/proof/status");
    let metadata_key = format!("{output_prefix}/zk/proof/metadata");
    let public_outputs_key = format!("{output_prefix}/zk/proof/public_outputs");
    let verify_status_key = format!("{output_prefix}/zk/proof/verify_status");

    let handle = dynlib.create_handle()?;
    let retired_request_id = "supervm-proof-engine-host-smoke-retired-profile3";
    let retired_prefix = "aoem.compute.output/supervm-proof-engine-host-smoke-retired-profile3";
    require_outputs_absent(&dynlib, retired_prefix)?;
    let retired_wire = build_proof_wire(
        retired_request_id,
        retired_prefix,
        RETIRED_PRIVATE_PROFILE_ID,
    )?;
    let retired_error = match handle.execute_ops_wire_v1(&retired_wire) {
        Ok(_) => bail!("retired private profile 3 was accepted"),
        Err(error) => error.to_string(),
    };
    if !retired_error.contains("rc=-4,")
        || !retired_error.contains(
            "zk_merkle_membership_v1 retired: no independent cryptographic relation verifier",
        )
    {
        bail!("unexpected retired profile failure: {retired_error}");
    }
    // The binding returns only the error on failure. Check actual output absence;
    // do not infer an inaccessible total_writes counter from that error.
    require_outputs_absent(&dynlib, retired_prefix)?;

    let wire = build_proof_wire(
        request_id,
        output_prefix,
        FIXED_PROFILE_RESIDENT_PROOF_V1_ID,
    )?;
    let result = handle.execute_ops_wire_v1(&wire)?;
    if result.processed != 1 || result.success != 1 || result.total_writes != 5 {
        bail!(
            "unexpected proof execution result: processed={} success={} writes={}",
            result.processed,
            result.success,
            result.total_writes
        );
    }

    require_state_fields(
        &dynlib,
        &proof_key,
        &[
            ("kind", json!("compute.zk.resident_proof_v1")),
            ("request_id", json!(request_id)),
            ("profile_id", json!(FIXED_PROFILE_RESIDENT_PROOF_V1_ID)),
            ("real_input_used", json!(true)),
            ("verification_scope", json!(ENVELOPE_VERIFICATION_SCOPE)),
            ("envelope_integrity_verified", json!(true)),
            ("cryptographic_proof_verified", json!(false)),
            ("fixed_profile_verifier_accepted", json!(false)),
        ],
    )?;
    require_state_fields(
        &dynlib,
        &status_key,
        &[
            ("kind", json!("compute.zk.resident_proof_v1.status")),
            ("request_id", json!(request_id)),
            ("status", json!("ok")),
            ("proof_verified", json!(false)),
            ("verification_scope", json!(ENVELOPE_VERIFICATION_SCOPE)),
            ("envelope_integrity_verified", json!(true)),
            ("cryptographic_proof_verified", json!(false)),
        ],
    )?;
    let metadata = require_state_fields(
        &dynlib,
        &metadata_key,
        &[
            ("kind", json!("compute.zk.resident_proof_v1.metadata")),
            ("request_id", json!(request_id)),
            ("runtime_canon_unchanged", json!(true)),
        ],
    )?;
    require_fields(
        metadata
            .get("payload")
            .context("missing metadata payload")?,
        &metadata_key,
        &[
            ("input_source", json!("payload_v2_real_input")),
            ("profile_id", json!(FIXED_PROFILE_RESIDENT_PROOF_V1_ID)),
        ],
    )?;
    require_state_fields(
        &dynlib,
        &public_outputs_key,
        &[
            ("kind", json!("compute.zk.resident_proof_v1.public_outputs")),
            ("request_id", json!(request_id)),
            ("profile_id", json!(FIXED_PROFILE_RESIDENT_PROOF_V1_ID)),
            ("real_input_used", json!(true)),
        ],
    )?;
    require_state_fields(
        &dynlib,
        &verify_status_key,
        &[
            ("kind", json!("compute.zk.resident_proof_v1.verify_status")),
            ("request_id", json!(request_id)),
            ("accepted", json!(false)),
            ("verification_scope", json!(ENVELOPE_VERIFICATION_SCOPE)),
            ("envelope_integrity_verified", json!(true)),
            ("cryptographic_proof_verified", json!(false)),
        ],
    )?;

    println!(
        "SUPERVM_AOEM_PROOF_ENGINE_HOST_SMOKE|profile=fixed_profile_v1|scope=not_zk|envelope_integrity_verified=true|cryptographic_proof_verified=false|proof_verified=false|accepted=false|retired_profile3=rejected|retired_profile3_outputs=absent|state_read=ok|metadata=ok|failures=0"
    );
    Ok(())
}
