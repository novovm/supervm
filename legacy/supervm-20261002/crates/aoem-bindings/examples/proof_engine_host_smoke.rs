use anyhow::{bail, Context, Result};
use aoem_bindings::{default_host_dll_path, AoemDyn};
use std::path::PathBuf;

const FIXED_PROFILE_RESIDENT_PROOF_V1_ID: u32 = 1;

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

fn build_proof_payload(request_id: &str, output_prefix: &str) -> Result<Vec<u8>> {
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
    push_u32(&mut payload, FIXED_PROFILE_RESIDENT_PROOF_V1_ID);
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

fn build_proof_wire(request_id: &str, output_prefix: &str) -> Result<Vec<u8>> {
    let payload = build_proof_payload(request_id, output_prefix)?;
    let mut wire = Vec::new();
    append_wire_op(&mut wire, 98, output_prefix, &payload)?;
    Ok(wire)
}

fn state_contains(dynlib: &AoemDyn, key: &str, needles: &[&str]) -> Result<()> {
    let response = dynlib.state_read_json_v1(key)?;
    let text = response.to_string();
    let found = response
        .get("value")
        .and_then(|value| value.get("found"))
        .or_else(|| response.get("found"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !found {
        bail!("state key not found: {key}; response={text}");
    }
    for needle in needles {
        if !text.contains(needle) {
            bail!("state key {key} missing {needle}; response={text}");
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
    let wire = build_proof_wire(request_id, output_prefix)?;
    let result = handle.execute_ops_wire_v1(&wire)?;
    if result.processed != 1 || result.success != 1 || result.total_writes != 5 {
        bail!(
            "unexpected proof execution result: processed={} success={} writes={}",
            result.processed,
            result.success,
            result.total_writes
        );
    }

    state_contains(
        &dynlib,
        &proof_key,
        &["compute.zk.resident_proof_v1", "real_input_used"],
    )?;
    state_contains(
        &dynlib,
        &status_key,
        &["compute.zk.resident_proof_v1.status", "proof_verified"],
    )?;
    state_contains(
        &dynlib,
        &metadata_key,
        &["input_source", "runtime_canon_unchanged"],
    )?;
    state_contains(
        &dynlib,
        &public_outputs_key,
        &[
            "compute.zk.resident_proof_v1.public_outputs",
            "real_input_used",
        ],
    )?;
    state_contains(
        &dynlib,
        &verify_status_key,
        &["compute.zk.resident_proof_v1.verify_status", "accepted"],
    )?;

    println!(
        "SUPERVM_AOEM_PROOF_ENGINE_HOST_SMOKE|profile=fixed_profile_v1|proof=ok|verify=ok|state_read=ok|metadata=ok|failures=0"
    );
    Ok(())
}
