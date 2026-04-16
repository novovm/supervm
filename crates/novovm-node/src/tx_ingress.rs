#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_adapter_api::{TxIR, TxType};
use novovm_exec::{
    EncodedOpsWire, ExecOpV2, OpsWireOp, OpsWireV1Builder, RawIngressCodecRegistry,
    AOEM_OPS_WIRE_V1_MAGIC, AOEM_OPS_WIRE_V1_VERSION,
};
use novovm_network::{
    eth_rlpx_transaction_hash_v1, eth_rlpx_validate_transaction_envelope_payload_v1,
    observe_network_runtime_native_pending_tx_local_ingress_with_payload_v1,
    observe_network_runtime_native_pending_tx_rejected_v1,
};
use novovm_protocol::{decode_local_tx_wire_v1 as decode_tx_wire_v1, LocalTxWireV1};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

pub const LOCAL_TX_WIRE_V1_BYTES: usize = 4 + 1 + (8 * 5) + 32;

#[derive(Debug, Clone, Copy)]
pub struct TxIngressRecord {
    pub account: u64,
    pub key: u64,
    pub value: u64,
    pub nonce: u64,
    pub fee: u64,
    pub signature: [u8; 32],
}

#[derive(Debug)]
pub struct ExecBatchBuffer {
    // Keep key/value payloads alive so ExecOpV2 raw pointers remain valid.
    _keys: Vec<[u8; 8]>,
    _values: Vec<[u8; 8]>,
    pub ops: Vec<ExecOpV2>,
}

impl ExecBatchBuffer {
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

pub type OpsWirePayload = EncodedOpsWire;

pub const LOCAL_TX_WIRE_CODEC_WRITE_U64LE_V1: &str = "local_tx_wire_v1_write_u64le_v1";
static LOCAL_TX_RECORD_CODEC_REGISTRY: OnceLock<RawIngressCodecRegistry> = OnceLock::new();

#[inline]
fn from_tx_wire_v1(wire: &LocalTxWireV1) -> TxIngressRecord {
    TxIngressRecord {
        account: wire.account,
        key: wire.key,
        value: wire.value,
        nonce: wire.nonce,
        fee: wire.fee,
        signature: wire.signature,
    }
}

pub fn encode_adapter_address(seed: u64) -> Vec<u8> {
    let mut out = vec![0u8; 20];
    out[12..20].copy_from_slice(&seed.to_be_bytes());
    out
}

pub fn tx_ingress_record_to_adapter_tx_ir(record: &TxIngressRecord, chain_id: u64) -> TxIR {
    let mut ir = TxIR {
        hash: Vec::new(),
        from: encode_adapter_address(record.account),
        to: Some(encode_adapter_address(record.key)),
        value: record.value as u128,
        gas_limit: 21_000,
        gas_price: record.fee,
        nonce: record.nonce,
        data: Vec::new(),
        signature: record.signature.to_vec(),
        chain_id,
        tx_type: TxType::Transfer,
        source_chain: None,
        target_chain: None,
    };
    ir.compute_hash();
    ir
}

pub fn tx_ingress_records_to_adapter_tx_irs(
    records: &[TxIngressRecord],
    chain_id: u64,
) -> Vec<TxIR> {
    records
        .iter()
        .map(|record| tx_ingress_record_to_adapter_tx_ir(record, chain_id))
        .collect()
}

pub fn ingest_local_eth_raw_tx_payload_v1(chain_id: u64, payload: &[u8]) -> Result<[u8; 32]> {
    if payload.is_empty() {
        bail!("eth_sendRawTransaction payload is empty");
    }
    let tx_hash = eth_rlpx_transaction_hash_v1(payload);
    if !eth_rlpx_validate_transaction_envelope_payload_v1(payload) {
        observe_network_runtime_native_pending_tx_rejected_v1(chain_id, tx_hash, None);
        bail!("eth_sendRawTransaction payload is not a valid ethereum tx envelope");
    }
    observe_network_runtime_native_pending_tx_local_ingress_with_payload_v1(
        chain_id,
        tx_hash,
        Some(payload),
    );
    Ok(tx_hash)
}

fn to_hex_prefixed_v1(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2 + 2);
    out.push_str("0x");
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn decode_hex_nibble_v1(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn decode_eth_send_raw_hex_payload_v1(raw: &str, field: &str) -> Result<Vec<u8>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("{field} is empty");
    }
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if hex.is_empty() {
        bail!("{field} is empty after 0x prefix");
    }
    if !hex.len().is_multiple_of(2) {
        bail!("{field} must be even-length hex, got len={}", hex.len());
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    for (idx, pair) in bytes.chunks_exact(2).enumerate() {
        let hi = decode_hex_nibble_v1(pair[0]).ok_or_else(|| {
            anyhow::anyhow!(
                "{field} contains invalid hex at byte={} char={}",
                idx * 2,
                pair[0] as char
            )
        })?;
        let lo = decode_hex_nibble_v1(pair[1]).ok_or_else(|| {
            anyhow::anyhow!(
                "{field} contains invalid hex at byte={} char={}",
                idx * 2 + 1,
                pair[1] as char
            )
        })?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

pub fn run_eth_send_raw_transaction_from_params_v1(
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let raw_tx = params
        .get("raw_tx")
        .and_then(|value| value.as_str())
        .or_else(|| {
            params
                .as_array()
                .and_then(|items| items.first())
                .and_then(|value| value.as_str())
        })
        .ok_or_else(|| anyhow::anyhow!("raw_tx is required for eth_sendRawTransaction"))?;
    let payload = decode_eth_send_raw_hex_payload_v1(raw_tx, "raw_tx")?;
    let chain_id = params
        .get("chain_id")
        .and_then(|value| value.as_u64())
        .or_else(|| {
            params
                .as_array()
                .and_then(|items| items.get(1))
                .and_then(|value| value.as_u64())
        })
        .unwrap_or(1);
    let tx_hash = ingest_local_eth_raw_tx_payload_v1(chain_id, payload.as_slice())?;
    Ok(serde_json::json!({
        "method": "eth_sendRawTransaction",
        "accepted": true,
        "pending_tx_local_ingress": true,
        "pending_tx_hash": to_hex_prefixed_v1(&tx_hash),
        "chain_id": chain_id,
    }))
}

fn load_tx_wire_bytes(path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read tx wire ingress file {}", path.display()))?;
    if bytes.is_empty() {
        bail!("tx wire ingress file is empty: {}", path.display());
    }
    if !bytes.len().is_multiple_of(LOCAL_TX_WIRE_V1_BYTES) {
        bail!(
            "tx wire ingress size mismatch: bytes={} not multiple of record_len={} (path={})",
            bytes.len(),
            LOCAL_TX_WIRE_V1_BYTES,
            path.display()
        );
    }
    Ok(bytes)
}

pub fn load_payload_bytes(path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read ingress file {}", path.display()))?;
    if bytes.is_empty() {
        bail!("ingress file is empty: {}", path.display());
    }
    Ok(bytes)
}

fn parse_ops_wire_v1_op_count(bytes: &[u8]) -> Result<usize> {
    const HEADER_LEN: usize = 5 + 2 + 2 + 4;
    if bytes.len() < HEADER_LEN {
        bail!(
            "ops-wire payload too short: len={} header_len={HEADER_LEN}",
            bytes.len()
        );
    }
    if &bytes[..AOEM_OPS_WIRE_V1_MAGIC.len()] != AOEM_OPS_WIRE_V1_MAGIC {
        bail!("ops-wire magic mismatch");
    }
    let mut cursor = AOEM_OPS_WIRE_V1_MAGIC.len();
    let version = u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
    cursor += 2;
    if version != AOEM_OPS_WIRE_V1_VERSION {
        bail!("ops-wire version mismatch: got={version}, expected={AOEM_OPS_WIRE_V1_VERSION}");
    }
    cursor += 2; // flags
    let count = u32::from_le_bytes([
        bytes[cursor],
        bytes[cursor + 1],
        bytes[cursor + 2],
        bytes[cursor + 3],
    ]) as usize;
    Ok(count)
}

fn encode_local_tx_wire_v1_write_u64le_v1(
    payload: &[u8],
    builder: &mut OpsWireV1Builder,
) -> Result<()> {
    if payload.is_empty() {
        bail!("tx wire payload is empty");
    }
    if !payload.len().is_multiple_of(LOCAL_TX_WIRE_V1_BYTES) {
        bail!(
            "tx wire payload size mismatch: bytes={} not multiple of record_len={}",
            payload.len(),
            LOCAL_TX_WIRE_V1_BYTES
        );
    }

    for (idx, chunk) in payload.chunks_exact(LOCAL_TX_WIRE_V1_BYTES).enumerate() {
        let wire = decode_tx_wire_v1(chunk)
            .with_context(|| format!("decode tx wire failed at record={idx}"))?;
        let key = wire.key.to_le_bytes();
        let value = wire.value.to_le_bytes();
        let plan_id = (wire.account << 32) | wire.nonce.saturating_add(1);
        builder.push(OpsWireOp {
            opcode: 2, // write
            flags: 0,
            reserved: 0,
            key: &key,
            value: &value,
            delta: 0,
            expect_version: None,
            plan_id,
        })?;
    }
    Ok(())
}

fn local_tx_record_codec_registry() -> &'static RawIngressCodecRegistry {
    LOCAL_TX_RECORD_CODEC_REGISTRY.get_or_init(|| {
        let mut registry = RawIngressCodecRegistry::new();
        registry
            .register(
                LOCAL_TX_WIRE_CODEC_WRITE_U64LE_V1,
                encode_local_tx_wire_v1_write_u64le_v1,
            )
            .expect("register local tx record codec");
        registry
    })
}

pub fn available_ingress_codecs() -> Vec<&'static str> {
    local_tx_record_codec_registry().codec_names()
}

pub fn encode_ops_wire_v1_from_payload(codec: &str, payload: &[u8]) -> Result<OpsWirePayload> {
    local_tx_record_codec_registry().encode(codec, payload)
}

pub fn load_ops_wire_v1_payload_file(path: &Path, codec: &str) -> Result<OpsWirePayload> {
    let payload = load_payload_bytes(path)?;
    encode_ops_wire_v1_from_payload(codec, &payload)
}

pub fn load_ops_wire_v1_file(path: &Path) -> Result<OpsWirePayload> {
    let bytes = load_payload_bytes(path)?;
    let op_count = parse_ops_wire_v1_op_count(&bytes)?;
    Ok(OpsWirePayload { bytes, op_count })
}

pub fn load_tx_records_from_wire_file(path: &Path) -> Result<Vec<TxIngressRecord>> {
    let bytes = load_tx_wire_bytes(path)?;

    let mut txs = Vec::with_capacity(bytes.len() / LOCAL_TX_WIRE_V1_BYTES);
    for (idx, chunk) in bytes.chunks_exact(LOCAL_TX_WIRE_V1_BYTES).enumerate() {
        let wire = decode_tx_wire_v1(chunk)
            .with_context(|| format!("decode tx wire failed at record={idx}"))?;
        txs.push(from_tx_wire_v1(&wire));
    }
    if txs.is_empty() {
        bail!(
            "tx wire ingress decoded zero transactions: {}",
            path.display()
        );
    }
    Ok(txs)
}

pub fn build_exec_batch_from_records<F>(
    records: &[TxIngressRecord],
    mut plan_id_for: F,
) -> ExecBatchBuffer
where
    F: FnMut(usize, &TxIngressRecord) -> u64,
{
    let mut keys: Vec<[u8; 8]> = records.iter().map(|rec| rec.key.to_le_bytes()).collect();
    let mut values: Vec<[u8; 8]> = records.iter().map(|rec| rec.value.to_le_bytes()).collect();
    let mut ops = Vec::with_capacity(records.len());

    for (i, ((key, value), rec)) in keys
        .iter_mut()
        .zip(values.iter_mut())
        .zip(records.iter())
        .enumerate()
    {
        ops.push(ExecOpV2 {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key_ptr: key.as_mut_ptr(),
            key_len: key.len() as u32,
            value_ptr: value.as_mut_ptr(),
            value_len: value.len() as u32,
            delta: 0,
            expect_version: u64::MAX,
            plan_id: plan_id_for(i, rec),
        });
    }

    ExecBatchBuffer {
        _keys: keys,
        _values: values,
        ops,
    }
}

pub fn load_exec_batch_from_wire_file<F>(path: &Path, mut plan_id_for: F) -> Result<ExecBatchBuffer>
where
    F: FnMut(usize, &TxIngressRecord) -> u64,
{
    let records = load_tx_records_from_wire_file(path)?;
    Ok(build_exec_batch_from_records(&records, |idx, rec| {
        plan_id_for(idx, rec)
    }))
}

pub fn build_ops_wire_v1_from_records<F>(
    records: &[TxIngressRecord],
    mut plan_id_for: F,
) -> OpsWirePayload
where
    F: FnMut(usize, &TxIngressRecord) -> u64,
{
    let mut builder = OpsWireV1Builder::new();
    for (idx, rec) in records.iter().enumerate() {
        let key = rec.key.to_le_bytes();
        let value = rec.value.to_le_bytes();
        let plan_id = plan_id_for(idx, rec);
        builder
            .push(OpsWireOp {
                opcode: 2, // write
                flags: 0,
                reserved: 0,
                key: &key,
                value: &value,
                delta: 0,
                expect_version: None,
                plan_id,
            })
            .expect("encode local tx records into ops-wire");
    }
    builder.finish()
}

pub fn load_ops_wire_v1_from_tx_wire_file(path: &Path) -> Result<OpsWirePayload> {
    let bytes = load_tx_wire_bytes(path)?;
    let tx_count = bytes.len() / LOCAL_TX_WIRE_V1_BYTES;
    if tx_count == 0 {
        bail!(
            "tx wire ingress decoded zero transactions: {}",
            path.display()
        );
    }
    encode_ops_wire_v1_from_payload(LOCAL_TX_WIRE_CODEC_WRITE_U64LE_V1, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tx_ingress_record_maps_to_adapter_tx_ir_with_fee_and_signature() {
        let record = TxIngressRecord {
            account: 7,
            key: 9,
            value: 11,
            nonce: 13,
            fee: 17,
            signature: [0xabu8; 32],
        };
        let ir = tx_ingress_record_to_adapter_tx_ir(&record, 1);
        assert_eq!(ir.chain_id, 1);
        assert_eq!(ir.tx_type, TxType::Transfer);
        assert_eq!(ir.value, 11);
        assert_eq!(ir.gas_limit, 21_000);
        assert_eq!(ir.gas_price, 17);
        assert_eq!(ir.nonce, 13);
        assert_eq!(ir.signature, vec![0xab; 32]);
        assert_eq!(ir.from.len(), 20);
        assert_eq!(ir.to.as_ref().map(Vec::len), Some(20));
        assert!(!ir.hash.is_empty());
    }

    #[test]
    fn decode_eth_send_raw_hex_payload_v1_accepts_prefixed_payload() {
        let payload = decode_eth_send_raw_hex_payload_v1("0x0102a0", "raw_tx")
            .expect("decode should succeed");
        assert_eq!(payload, vec![0x01, 0x02, 0xa0]);
    }

    #[test]
    fn run_eth_send_raw_transaction_from_params_v1_tracks_pending() {
        let chain_id = 98_877_663;
        let raw_tx_hex =
            "0x02e20180021e827530946e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e0480c0010101";
        let payload =
            decode_eth_send_raw_hex_payload_v1(raw_tx_hex, "raw_tx").expect("decode raw tx");
        let expected_hash = eth_rlpx_transaction_hash_v1(payload.as_slice());

        let out = run_eth_send_raw_transaction_from_params_v1(&serde_json::json!({
            "raw_tx": raw_tx_hex,
            "chain_id": chain_id,
        }))
        .expect("route should succeed");
        assert_eq!(out["accepted"].as_bool(), Some(true));
        assert_eq!(
            out["pending_tx_hash"].as_str(),
            Some(to_hex_prefixed_v1(&expected_hash).as_str())
        );
        assert_eq!(out["chain_id"].as_u64(), Some(chain_id));

        let pending =
            novovm_network::get_network_runtime_native_pending_tx_v1(chain_id, expected_hash)
                .expect("pending tx should exist");
        assert_eq!(
            pending.origin,
            novovm_network::NetworkRuntimeNativePendingTxOriginV1::Local
        );
    }

    #[test]
    fn ingest_local_eth_raw_tx_payload_marks_rejected_when_invalid() {
        let chain_id = 98_877_663;
        let payload = vec![0x01, 0x02, 0x03];
        let expected_hash = novovm_network::eth_rlpx_transaction_hash_v1(payload.as_slice());
        let err = ingest_local_eth_raw_tx_payload_v1(chain_id, payload.as_slice())
            .expect_err("invalid envelope should fail");
        assert!(format!("{err}").contains("not a valid ethereum tx envelope"));
        let state =
            novovm_network::get_network_runtime_native_pending_tx_v1(chain_id, expected_hash)
                .expect("invalid local tx should still be tracked as rejected");
        assert_eq!(
            state.lifecycle_stage,
            novovm_network::NetworkRuntimeNativePendingTxLifecycleStageV1::Rejected
        );
        assert_eq!(
            state.origin,
            novovm_network::NetworkRuntimeNativePendingTxOriginV1::Local
        );
        assert_eq!(state.reject_count, 1);
    }
}
