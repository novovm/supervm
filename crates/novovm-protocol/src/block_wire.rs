#![forbid(unsafe_code)]

use crate::block_binding::ConsensusPluginBindingV1;
use thiserror::Error;

pub const BLOCK_HEADER_WIRE_V1_CODEC: &str = "novovm_block_header_wire_v1";
const BLOCK_HEADER_WIRE_MAGIC: &[u8; 4] = b"NBH1";
const BLOCK_HEADER_WIRE_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockHeaderWireV1 {
    pub height: u64,
    pub epoch_id: u64,
    pub parent_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub tx_count: u64,
    pub batch_count: u32,
    pub consensus_binding: ConsensusPluginBindingV1,
}

#[derive(Debug, Error)]
pub enum BlockWireError {
    #[error("wire length mismatch: expected {expected}, got {got}")]
    LengthMismatch { expected: usize, got: usize },
    #[error("wire magic mismatch")]
    MagicMismatch,
    #[error("wire version mismatch: expected {expected}, got {got}")]
    VersionMismatch { expected: u8, got: u8 },
}

pub fn encode_block_header_wire_v1(header: &BlockHeaderWireV1) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 1 + 8 + 8 + 32 + 32 + 8 + 4 + 1 + 32);
    out.extend_from_slice(BLOCK_HEADER_WIRE_MAGIC);
    out.push(BLOCK_HEADER_WIRE_VERSION);
    out.extend_from_slice(&header.height.to_le_bytes());
    out.extend_from_slice(&header.epoch_id.to_le_bytes());
    out.extend_from_slice(&header.parent_hash);
    out.extend_from_slice(&header.state_root);
    out.extend_from_slice(&header.tx_count.to_le_bytes());
    out.extend_from_slice(&header.batch_count.to_le_bytes());
    out.push(header.consensus_binding.plugin_class_code);
    out.extend_from_slice(&header.consensus_binding.adapter_hash);
    out
}

pub fn decode_block_header_wire_v1(bytes: &[u8]) -> Result<BlockHeaderWireV1, BlockWireError> {
    let expected_len = 4 + 1 + 8 + 8 + 32 + 32 + 8 + 4 + 1 + 32;
    if bytes.len() != expected_len {
        return Err(BlockWireError::LengthMismatch {
            expected: expected_len,
            got: bytes.len(),
        });
    }
    if &bytes[0..4] != BLOCK_HEADER_WIRE_MAGIC {
        return Err(BlockWireError::MagicMismatch);
    }
    if bytes[4] != BLOCK_HEADER_WIRE_VERSION {
        return Err(BlockWireError::VersionMismatch {
            expected: BLOCK_HEADER_WIRE_VERSION,
            got: bytes[4],
        });
    }

    let mut off = 5usize;
    let read_u64 = |buf: &[u8], offset: &mut usize| -> u64 {
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&buf[*offset..(*offset + 8)]);
        *offset += 8;
        u64::from_le_bytes(arr)
    };
    let read_u32 = |buf: &[u8], offset: &mut usize| -> u32 {
        let mut arr = [0u8; 4];
        arr.copy_from_slice(&buf[*offset..(*offset + 4)]);
        *offset += 4;
        u32::from_le_bytes(arr)
    };
    let read_hash32 = |buf: &[u8], offset: &mut usize| -> [u8; 32] {
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&buf[*offset..(*offset + 32)]);
        *offset += 32;
        arr
    };

    let height = read_u64(bytes, &mut off);
    let epoch_id = read_u64(bytes, &mut off);
    let parent_hash = read_hash32(bytes, &mut off);
    let state_root = read_hash32(bytes, &mut off);
    let tx_count = read_u64(bytes, &mut off);
    let batch_count = read_u32(bytes, &mut off);
    let plugin_class_code = bytes[off];
    off += 1;
    let adapter_hash = read_hash32(bytes, &mut off);

    Ok(BlockHeaderWireV1 {
        height,
        epoch_id,
        parent_hash,
        state_root,
        tx_count,
        batch_count,
        consensus_binding: ConsensusPluginBindingV1 {
            plugin_class_code,
            adapter_hash,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_header_wire_roundtrip() {
        let header = BlockHeaderWireV1 {
            height: 7,
            epoch_id: 11,
            parent_hash: [1u8; 32],
            state_root: [2u8; 32],
            tx_count: 9,
            batch_count: 3,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: 1,
                adapter_hash: [3u8; 32],
            },
        };
        let wire = encode_block_header_wire_v1(&header);
        let decoded = decode_block_header_wire_v1(&wire).unwrap();
        assert_eq!(decoded, header);
    }

    #[test]
    fn block_header_wire_rejects_bad_magic() {
        let header = BlockHeaderWireV1 {
            height: 1,
            epoch_id: 2,
            parent_hash: [0u8; 32],
            state_root: [0u8; 32],
            tx_count: 3,
            batch_count: 1,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: 1,
                adapter_hash: [0u8; 32],
            },
        };
        let mut wire = encode_block_header_wire_v1(&header);
        wire[0] = b'X';
        let err = decode_block_header_wire_v1(&wire).unwrap_err().to_string();
        assert!(err.contains("magic mismatch"));
    }
}
