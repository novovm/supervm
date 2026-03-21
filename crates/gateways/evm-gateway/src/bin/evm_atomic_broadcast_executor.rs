#![forbid(unsafe_code)]

#[path = "../bincode_compat.rs"]
mod bincode_compat;

use anyhow::{bail, Context, Result};
use novovm_adapter_api::TxIR;
use serde_json::Value;
use std::io::Read;

#[derive(Debug)]
struct AtomicBroadcastExecRequest {
    intent_id: String,
    chain_id: u64,
    tx_hash: [u8; 32],
    tx_ir_bincode: Option<Vec<u8>>,
    tx_ir_format: Option<String>,
}

fn main() -> Result<()> {
    let mut body = String::new();
    std::io::stdin()
        .read_to_string(&mut body)
        .context("read executor stdin failed")?;
    if body.trim().is_empty() {
        bail!("empty executor request");
    }

    let raw: Value = serde_json::from_str(&body).context("decode executor request json failed")?;
    let req = parse_executor_request(&raw)?;
    validate_executor_request(&req)?;

    let output = serde_json::json!({
        "broadcasted": true,
        "intent_id": req.intent_id,
        "chain_id": format!("0x{:x}", req.chain_id),
        "tx_hash": format!("0x{}", to_hex(&req.tx_hash)),
        "executor": "evm_atomic_broadcast_executor",
    });
    println!(
        "{}",
        serde_json::to_string(&output).context("encode executor output json failed")?
    );
    Ok(())
}

fn parse_executor_request(raw: &Value) -> Result<AtomicBroadcastExecRequest> {
    let map = raw
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("executor request must be json object"))?;
    let intent_id = map
        .get("intent_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("intent_id is required"))?
        .to_string();
    let chain_id = map
        .get("chain_id")
        .and_then(value_to_u64)
        .ok_or_else(|| anyhow::anyhow!("chain_id is required"))?;
    let tx_hash_raw = map
        .get("tx_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("tx_hash is required"))?;
    let tx_hash =
        parse_hex32_from_string(tx_hash_raw, "tx_hash").context("decode tx_hash failed")?;
    let tx_ir_bincode = map
        .get("tx_ir_bincode")
        .and_then(Value::as_str)
        .map(|v| decode_hex_bytes(v, "tx_ir_bincode"))
        .transpose()?;
    let tx_ir_format = map
        .get("tx_ir_format")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    Ok(AtomicBroadcastExecRequest {
        intent_id,
        chain_id,
        tx_hash,
        tx_ir_bincode,
        tx_ir_format,
    })
}

fn validate_executor_request(req: &AtomicBroadcastExecRequest) -> Result<()> {
    if let Some(format) = req.tx_ir_format.as_deref() {
        if format != "bincode_v1" {
            bail!("unsupported tx_ir_format: {}", format);
        }
    }
    let Some(payload) = req.tx_ir_bincode.as_deref() else {
        return Ok(());
    };
    if payload.is_empty() {
        bail!("tx_ir_bincode is empty");
    }
    let tx = decode_tx_ir_bincode(payload)?;
    if tx.chain_id != req.chain_id {
        bail!(
            "tx_ir chain_id mismatch: expected={} actual={}",
            req.chain_id,
            tx.chain_id
        );
    }
    if tx.hash.is_empty() {
        bail!("tx_ir hash is empty");
    }
    let tx_hash = vec_to_32(&tx.hash, "tx_ir.hash")?;
    if tx_hash != req.tx_hash {
        bail!(
            "tx_ir tx_hash mismatch: expected=0x{} actual=0x{}",
            to_hex(&req.tx_hash),
            to_hex(&tx_hash)
        );
    }
    Ok(())
}

fn decode_tx_ir_bincode(payload: &[u8]) -> Result<TxIR> {
    if let Ok(tx) = crate::bincode_compat::deserialize::<TxIR>(payload) {
        return Ok(tx);
    }
    if let Ok(mut txs) = crate::bincode_compat::deserialize::<Vec<TxIR>>(payload) {
        if txs.len() == 1 {
            return Ok(txs.remove(0));
        }
        bail!("tx_ir_bincode vector payload must contain exactly one tx");
    }
    bail!("decode tx_ir_bincode failed")
}

fn value_to_u64(raw: &Value) -> Option<u64> {
    match raw {
        Value::Number(num) => num.as_u64(),
        Value::String(s) => parse_u64_hex_or_dec(s),
        _ => None,
    }
}

fn parse_u64_hex_or_dec(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u64::from_str_radix(rest, 16).ok()
    } else {
        trimmed.parse::<u64>().ok()
    }
}

fn parse_hex32_from_string(raw: &str, field: &str) -> Result<[u8; 32]> {
    let bytes = decode_hex_bytes(raw, field)?;
    vec_to_32(&bytes, field)
}

fn decode_hex_bytes(raw: &str, field: &str) -> Result<Vec<u8>> {
    let trimmed = raw.trim();
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if !hex.len().is_multiple_of(2) {
        bail!("{field} must contain an even number of hex chars");
    }
    if hex.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let high = hex_nibble(bytes[idx]).ok_or_else(|| anyhow::anyhow!("{field} is not hex"))?;
        let low =
            hex_nibble(bytes[idx + 1]).ok_or_else(|| anyhow::anyhow!("{field} is not hex"))?;
        out.push((high << 4) | low);
        idx += 2;
    }
    Ok(out)
}

fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn vec_to_32(bytes: &[u8], field: &str) -> Result<[u8; 32]> {
    if bytes.len() != 32 {
        bail!("{field} must be 32 bytes, got {}", bytes.len());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes);
    Ok(out)
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{:02x}", b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_adapter_api::SerializationFormat;

    #[test]
    fn parse_executor_request_accepts_hex_chain_id() {
        let raw = serde_json::json!({
            "intent_id": "intent-0001",
            "chain_id": "0x1",
            "tx_hash": format!("0x{}", "11".repeat(32)),
        });
        let req = parse_executor_request(&raw).expect("parse request");
        assert_eq!(req.intent_id, "intent-0001");
        assert_eq!(req.chain_id, 1);
        assert_eq!(req.tx_hash, [0x11u8; 32]);
    }

    #[test]
    fn validate_executor_request_accepts_single_tx_ir_payload() {
        let mut tx = TxIR::transfer(vec![0x11; 20], vec![0x22; 20], 1, 3, 1);
        tx.compute_hash();
        let payload = tx
            .serialize(SerializationFormat::Bincode)
            .expect("serialize bincode");
        let req = AtomicBroadcastExecRequest {
            intent_id: "intent-0002".to_string(),
            chain_id: tx.chain_id,
            tx_hash: vec_to_32(&tx.hash, "tx.hash").expect("decode tx hash"),
            tx_ir_bincode: Some(payload),
            tx_ir_format: Some("bincode_v1".to_string()),
        };
        validate_executor_request(&req).expect("validate request");
    }
}


