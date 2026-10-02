//! Reviewed frame-codec migration from the isolated network implementation.
//! This module carries opaque bytes only; it is not a reliability scheduler or
//! proof of delivery, execution, consensus, or finality.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const NOVORUDP_TRANSPORT_FRAME_V0_MAGIC: &[u8; 8] = b"NOVRUDP0";
pub const NOVORUDP_TRANSPORT_FRAME_V0_VERSION: u16 = 1;
const HEADER_LEN_V0: usize = 8 + 2 + 1 + 1 + 16 + 8 + 8 + 8 + 8 + 4 + 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NovoRudpTransportFrameKindV0 {
    Data,
    Repair,
    Ack,
    Endpoint,
    Done,
}

impl NovoRudpTransportFrameKindV0 {
    const fn code(self) -> u8 {
        match self {
            Self::Data => 1,
            Self::Repair => 2,
            Self::Ack => 3,
            Self::Endpoint => 4,
            Self::Done => 5,
        }
    }

    const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Data),
            2 => Some(Self::Repair),
            3 => Some(Self::Ack),
            4 => Some(Self::Endpoint),
            5 => Some(Self::Done),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpTransportFrameV0 {
    pub kind: NovoRudpTransportFrameKindV0,
    pub session_id: [u8; 16],
    pub stream_id: u64,
    pub object_id: u64,
    pub sequence: u64,
    pub ack_epoch: u64,
    pub payload: Vec<u8>,
    pub checksum: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NovoRudpTransportFrameDecodeErrorV0 {
    TooShort { len: usize },
    BadMagic,
    UnsupportedVersion { version: u16 },
    UnknownKind { kind: u8 },
    NonzeroReserved { value: u8 },
    PayloadTooLarge { len: usize },
    LengthMismatch { expected: usize, actual: usize },
    ChecksumMismatch,
}

impl std::fmt::Display for NovoRudpTransportFrameDecodeErrorV0 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { len } => write!(f, "novorudp frame v0 too short: len={len}"),
            Self::BadMagic => write!(f, "novorudp frame v0 bad magic"),
            Self::UnsupportedVersion { version } => {
                write!(f, "novorudp frame v0 unsupported version: {version}")
            }
            Self::UnknownKind { kind } => write!(f, "novorudp frame v0 unknown kind: {kind}"),
            Self::NonzeroReserved { value } => {
                write!(f, "novorudp frame v0 nonzero reserved byte: {value}")
            }
            Self::PayloadTooLarge { len } => {
                write!(f, "novorudp frame v0 payload too large: len={len}")
            }
            Self::LengthMismatch { expected, actual } => write!(
                f,
                "novorudp frame v0 length mismatch: expected={expected} actual={actual}"
            ),
            Self::ChecksumMismatch => write!(f, "novorudp frame v0 checksum mismatch"),
        }
    }
}

impl std::error::Error for NovoRudpTransportFrameDecodeErrorV0 {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NovoRudpTransportFrameEncodeErrorV0 {
    pub payload_len: usize,
}

impl std::fmt::Display for NovoRudpTransportFrameEncodeErrorV0 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "novorudp frame v0 payload too large: len={}",
            self.payload_len
        )
    }
}

impl std::error::Error for NovoRudpTransportFrameEncodeErrorV0 {}

fn checked_frame_lengths_v0(
    payload_len: usize,
) -> Result<(u32, usize), NovoRudpTransportFrameEncodeErrorV0> {
    let error = || NovoRudpTransportFrameEncodeErrorV0 { payload_len };
    let encoded_len = u32::try_from(payload_len).map_err(|_| error())?;
    let total_len = HEADER_LEN_V0.checked_add(payload_len).ok_or_else(error)?;
    Ok((encoded_len, total_len))
}

impl NovoRudpTransportFrameV0 {
    pub fn new(
        kind: NovoRudpTransportFrameKindV0,
        session_id: [u8; 16],
        stream_id: u64,
        object_id: u64,
        sequence: u64,
        ack_epoch: u64,
        payload: Vec<u8>,
    ) -> Self {
        let checksum = novorudp_transport_frame_checksum_v0(
            kind,
            &session_id,
            stream_id,
            object_id,
            sequence,
            ack_epoch,
            payload.as_slice(),
        );
        Self {
            kind,
            session_id,
            stream_id,
            object_id,
            sequence,
            ack_epoch,
            payload,
            checksum,
        }
    }

    /// Compatibility helper for known bounded frames. Use `try_encode` at a
    /// transport boundary when the payload size can be supplied externally.
    ///
    /// # Panics
    /// Panics when the payload cannot fit the wire length or address space.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        self.try_encode()
            .expect("NOVORUDP payload must fit its wire length and address space")
    }

    /// Fallible transport boundary; never clamps a length into another frame.
    pub fn try_encode(&self) -> Result<Vec<u8>, NovoRudpTransportFrameEncodeErrorV0> {
        let (payload_len, total_len) = checked_frame_lengths_v0(self.payload.len())?;
        let checksum = novorudp_transport_frame_checksum_v0(
            self.kind,
            &self.session_id,
            self.stream_id,
            self.object_id,
            self.sequence,
            self.ack_epoch,
            self.payload.as_slice(),
        );
        let mut out = Vec::with_capacity(total_len);
        out.extend_from_slice(NOVORUDP_TRANSPORT_FRAME_V0_MAGIC);
        out.extend_from_slice(&NOVORUDP_TRANSPORT_FRAME_V0_VERSION.to_le_bytes());
        out.push(self.kind.code());
        out.push(0);
        out.extend_from_slice(&self.session_id);
        out.extend_from_slice(&self.stream_id.to_le_bytes());
        out.extend_from_slice(&self.object_id.to_le_bytes());
        out.extend_from_slice(&self.sequence.to_le_bytes());
        out.extend_from_slice(&self.ack_epoch.to_le_bytes());
        out.extend_from_slice(&payload_len.to_le_bytes());
        out.extend_from_slice(&checksum);
        out.extend_from_slice(&self.payload);
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, NovoRudpTransportFrameDecodeErrorV0> {
        if bytes.len() < HEADER_LEN_V0 {
            return Err(NovoRudpTransportFrameDecodeErrorV0::TooShort { len: bytes.len() });
        }
        if &bytes[..8] != NOVORUDP_TRANSPORT_FRAME_V0_MAGIC {
            return Err(NovoRudpTransportFrameDecodeErrorV0::BadMagic);
        }
        let version = u16::from_le_bytes([bytes[8], bytes[9]]);
        if version != NOVORUDP_TRANSPORT_FRAME_V0_VERSION {
            return Err(NovoRudpTransportFrameDecodeErrorV0::UnsupportedVersion { version });
        }
        let kind_code = bytes[10];
        let kind = NovoRudpTransportFrameKindV0::from_code(kind_code)
            .ok_or(NovoRudpTransportFrameDecodeErrorV0::UnknownKind { kind: kind_code })?;
        if bytes[11] != 0 {
            return Err(NovoRudpTransportFrameDecodeErrorV0::NonzeroReserved { value: bytes[11] });
        }
        let mut offset = 12;
        let mut session_id = [0u8; 16];
        session_id.copy_from_slice(&bytes[offset..offset + 16]);
        offset += 16;
        let stream_id = read_u64_le_v0(bytes, &mut offset);
        let object_id = read_u64_le_v0(bytes, &mut offset);
        let sequence = read_u64_le_v0(bytes, &mut offset);
        let ack_epoch = read_u64_le_v0(bytes, &mut offset);
        let payload_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;
        let mut checksum = [0u8; 32];
        checksum.copy_from_slice(&bytes[offset..offset + 32]);
        offset += 32;
        let expected = offset
            .checked_add(payload_len)
            .ok_or(NovoRudpTransportFrameDecodeErrorV0::PayloadTooLarge { len: payload_len })?;
        if expected != bytes.len() {
            return Err(NovoRudpTransportFrameDecodeErrorV0::LengthMismatch {
                expected,
                actual: bytes.len(),
            });
        }
        let payload = bytes[offset..].to_vec();
        let computed = novorudp_transport_frame_checksum_v0(
            kind,
            &session_id,
            stream_id,
            object_id,
            sequence,
            ack_epoch,
            payload.as_slice(),
        );
        if checksum != computed {
            return Err(NovoRudpTransportFrameDecodeErrorV0::ChecksumMismatch);
        }
        Ok(Self {
            kind,
            session_id,
            stream_id,
            object_id,
            sequence,
            ack_epoch,
            payload,
            checksum,
        })
    }
}

fn read_u64_le_v0(bytes: &[u8], offset: &mut usize) -> u64 {
    let value = u64::from_le_bytes([
        bytes[*offset],
        bytes[*offset + 1],
        bytes[*offset + 2],
        bytes[*offset + 3],
        bytes[*offset + 4],
        bytes[*offset + 5],
        bytes[*offset + 6],
        bytes[*offset + 7],
    ]);
    *offset += 8;
    value
}

fn novorudp_transport_frame_checksum_v0(
    kind: NovoRudpTransportFrameKindV0,
    session_id: &[u8; 16],
    stream_id: u64,
    object_id: u64,
    sequence: u64,
    ack_epoch: u64,
    payload: &[u8],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"novorudp-transport-frame-v0");
    hasher.update(NOVORUDP_TRANSPORT_FRAME_V0_MAGIC);
    hasher.update(NOVORUDP_TRANSPORT_FRAME_V0_VERSION.to_le_bytes());
    hasher.update([kind.code()]);
    hasher.update(session_id);
    hasher.update(stream_id.to_le_bytes());
    hasher.update(object_id.to_le_bytes());
    hasher.update(sequence.to_le_bytes());
    hasher.update(ack_epoch.to_le_bytes());
    hasher.update((payload.len() as u64).to_le_bytes());
    hasher.update(payload);
    hasher.finalize().into()
}

#[cfg(test)]
#[path = "novorudp_tests.rs"]
mod tests;
