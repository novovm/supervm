use anyhow::{anyhow, bail, Result};
use std::collections::BTreeMap;

pub const AOEM_OPS_WIRE_V1_MAGIC: &[u8; 5] = b"AOV2\0";
pub const AOEM_OPS_WIRE_V1_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug)]
pub struct OpsWireOp<'a> {
    pub opcode: u8,
    pub flags: u8,
    pub reserved: u16,
    pub key: &'a [u8],
    pub value: &'a [u8],
    pub delta: i64,
    pub expect_version: Option<u64>,
    pub plan_id: u64,
}

#[derive(Clone, Debug)]
pub struct EncodedOpsWire {
    pub bytes: Vec<u8>,
    pub op_count: usize,
}

pub struct OpsWireV1Builder {
    bytes: Vec<u8>,
    op_count: usize,
}

impl OpsWireV1Builder {
    pub fn new() -> Self {
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(AOEM_OPS_WIRE_V1_MAGIC);
        write_u16(&mut bytes, AOEM_OPS_WIRE_V1_VERSION);
        write_u16(&mut bytes, 0); // flags reserved
        write_u32(&mut bytes, 0); // op_count placeholder
        Self { bytes, op_count: 0 }
    }

    pub fn push(&mut self, op: OpsWireOp<'_>) -> Result<()> {
        if op.key.len() > u32::MAX as usize {
            bail!("ops-wire key too large: {}", op.key.len());
        }
        if op.value.len() > u32::MAX as usize {
            bail!("ops-wire value too large: {}", op.value.len());
        }
        if self.op_count >= u32::MAX as usize {
            bail!("ops-wire op count overflow");
        }
        self.bytes.push(op.opcode);
        self.bytes.push(op.flags);
        write_u16(&mut self.bytes, op.reserved);
        write_u32(&mut self.bytes, op.key.len() as u32);
        write_u32(&mut self.bytes, op.value.len() as u32);
        write_i64(&mut self.bytes, op.delta);
        write_u64(
            &mut self.bytes,
            op.expect_version.unwrap_or(u64::MAX),
        );
        write_u64(&mut self.bytes, op.plan_id);
        self.bytes.extend_from_slice(op.key);
        self.bytes.extend_from_slice(op.value);
        self.op_count += 1;
        Ok(())
    }

    pub fn finish(mut self) -> EncodedOpsWire {
        let count_off = AOEM_OPS_WIRE_V1_MAGIC.len() + 2 + 2;
        let cnt = (self.op_count as u32).to_le_bytes();
        self.bytes[count_off..count_off + 4].copy_from_slice(&cnt);
        EncodedOpsWire {
            bytes: self.bytes,
            op_count: self.op_count,
        }
    }
}

impl Default for OpsWireV1Builder {
    fn default() -> Self {
        Self::new()
    }
}

pub type CodecEncodeFn<T> = fn(&T, &mut OpsWireV1Builder) -> Result<()>;
pub type RawCodecEncodeFn = fn(&[u8], &mut OpsWireV1Builder) -> Result<()>;

pub struct IngressCodecRegistry<T> {
    codecs: BTreeMap<&'static str, CodecEncodeFn<T>>,
}

impl<T> IngressCodecRegistry<T> {
    pub fn new() -> Self {
        Self {
            codecs: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, name: &'static str, encoder: CodecEncodeFn<T>) -> Result<()> {
        if name.trim().is_empty() {
            bail!("codec name must not be empty");
        }
        if self.codecs.contains_key(name) {
            bail!("codec already registered: {name}");
        }
        self.codecs.insert(name, encoder);
        Ok(())
    }

    pub fn encode(&self, name: &str, input: &T) -> Result<EncodedOpsWire> {
        let encoder = self
            .codecs
            .get(name)
            .ok_or_else(|| anyhow!("codec not registered: {name}"))?;
        let mut builder = OpsWireV1Builder::new();
        encoder(input, &mut builder)?;
        Ok(builder.finish())
    }

    pub fn codec_names(&self) -> Vec<&'static str> {
        self.codecs.keys().copied().collect()
    }
}

impl<T> Default for IngressCodecRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RawIngressCodecRegistry {
    codecs: BTreeMap<&'static str, RawCodecEncodeFn>,
}

impl RawIngressCodecRegistry {
    pub fn new() -> Self {
        Self {
            codecs: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, name: &'static str, encoder: RawCodecEncodeFn) -> Result<()> {
        if name.trim().is_empty() {
            bail!("codec name must not be empty");
        }
        if self.codecs.contains_key(name) {
            bail!("codec already registered: {name}");
        }
        self.codecs.insert(name, encoder);
        Ok(())
    }

    pub fn encode(&self, name: &str, payload: &[u8]) -> Result<EncodedOpsWire> {
        let encoder = self
            .codecs
            .get(name)
            .ok_or_else(|| anyhow!("codec not registered: {name}"))?;
        let mut builder = OpsWireV1Builder::new();
        encoder(payload, &mut builder)?;
        Ok(builder.finish())
    }

    pub fn codec_names(&self) -> Vec<&'static str> {
        self.codecs.keys().copied().collect()
    }
}

impl Default for RawIngressCodecRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn write_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn write_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn write_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn write_i64(out: &mut Vec<u8>, v: i64) {
    out.extend_from_slice(&v.to_le_bytes());
}
