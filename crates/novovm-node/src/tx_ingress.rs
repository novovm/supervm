#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_exec::{
    EncodedOpsWire, ExecOpV2, OpsWireOp, OpsWireV1Builder, RawIngressCodecRegistry,
    AOEM_OPS_WIRE_V1_MAGIC, AOEM_OPS_WIRE_V1_VERSION,
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
}

#[derive(Debug)]
pub struct ExecBatchBuffer {
    _keys: Vec<[u8; 8]>,
    _values: Vec<[u8; 8]>,
    pub ops: Vec<ExecOpV2>,
}

impl ExecBatchBuffer {
    pub fn len(&self) -> usize {
        self.ops.len()
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
    }
}

fn load_tx_wire_bytes(path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read tx wire ingress file {}", path.display()))?;
    if bytes.is_empty() {
        bail!("tx wire ingress file is empty: {}", path.display());
    }
    if bytes.len() % LOCAL_TX_WIRE_V1_BYTES != 0 {
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
    if payload.len() % LOCAL_TX_WIRE_V1_BYTES != 0 {
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
    let mut keys = vec![[0u8; 8]; records.len()];
    let mut values = vec![[0u8; 8]; records.len()];
    let mut ops = Vec::with_capacity(records.len());

    for (i, rec) in records.iter().enumerate() {
        keys[i] = rec.key.to_le_bytes();
        values[i] = rec.value.to_le_bytes();
        ops.push(ExecOpV2 {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key_ptr: keys[i].as_mut_ptr(),
            key_len: keys[i].len() as u32,
            value_ptr: values[i].as_mut_ptr(),
            value_len: values[i].len() as u32,
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
    let bytes = load_tx_wire_bytes(path)?;

    let tx_count = bytes.len() / LOCAL_TX_WIRE_V1_BYTES;
    let mut keys = vec![[0u8; 8]; tx_count];
    let mut values = vec![[0u8; 8]; tx_count];
    let mut ops = Vec::with_capacity(tx_count);

    for (idx, chunk) in bytes.chunks_exact(LOCAL_TX_WIRE_V1_BYTES).enumerate() {
        let wire = decode_tx_wire_v1(chunk)
            .with_context(|| format!("decode tx wire failed at record={idx}"))?;
        let rec = from_tx_wire_v1(&wire);
        keys[idx] = rec.key.to_le_bytes();
        values[idx] = rec.value.to_le_bytes();
        ops.push(ExecOpV2 {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key_ptr: keys[idx].as_mut_ptr(),
            key_len: keys[idx].len() as u32,
            value_ptr: values[idx].as_mut_ptr(),
            value_len: values[idx].len() as u32,
            delta: 0,
            expect_version: u64::MAX,
            plan_id: plan_id_for(idx, &rec),
        });
    }
    if ops.is_empty() {
        bail!(
            "tx wire ingress decoded zero transactions: {}",
            path.display()
        );
    }

    Ok(ExecBatchBuffer {
        _keys: keys,
        _values: values,
        ops,
    })
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
