#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Canonical codec name for a NOV block execution context.
pub const NOV_BLOCK_EXECUTION_CONTEXT_V1_CODEC: &str = "novovm_block_execution_context_v1";

/// The encoded context is fixed-width so its commitment never depends on a
/// serializer implementation or map ordering.
pub const NOV_BLOCK_EXECUTION_CONTEXT_V1_WIRE_LEN: usize = 4 + 1 + 8 + 8 + 32 + 8 + 8;

const NOV_BLOCK_EXECUTION_CONTEXT_V1_MAGIC: &[u8; 4] = b"NBX1";
const NOV_BLOCK_EXECUTION_CONTEXT_V1_VERSION: u8 = 1;
const NOV_BLOCK_EXECUTION_CONTEXT_V1_COMMITMENT_DOMAIN: &[u8] =
    b"novovm-block-execution-context-commitment-v1\0";

/// Deterministic inputs supplied by the NOV host to every transaction in one
/// candidate block.
///
/// This is a NOV protocol type. It intentionally contains no AOEM-specific
/// data and no consensus vote/QC fields: AOEM consumes the host-selected
/// context as opaque execution input, while consensus later seals the block
/// candidate hash that commits to this context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovBlockExecutionContextV1 {
    pub chain_id: u64,
    pub block_height: u64,
    pub parent_block_hash: [u8; 32],
    pub slot: u64,
    pub timestamp_unix_ms: u64,
}

impl NovBlockExecutionContextV1 {
    /// Validate intrinsic v1 invariants.
    ///
    /// Height 1 uses the zero block-parent hash. Every later executable block
    /// must identify a concrete parent. Parent/child height, slot and timestamp
    /// continuity is ledger state and must additionally be checked by the
    /// block-candidate builder.
    pub fn validate(&self) -> Result<(), NovBlockExecutionContextError> {
        if self.chain_id == 0 {
            return Err(NovBlockExecutionContextError::ZeroChainId);
        }
        if self.block_height == 0 {
            return Err(NovBlockExecutionContextError::ZeroBlockHeight);
        }
        if self.block_height == 1 && self.parent_block_hash != [0u8; 32] {
            return Err(NovBlockExecutionContextError::UnexpectedGenesisParentBlockHash);
        }
        if self.block_height > 1 && self.parent_block_hash == [0u8; 32] {
            return Err(NovBlockExecutionContextError::MissingParentBlockHash {
                block_height: self.block_height,
            });
        }
        Ok(())
    }

    /// Encode with the canonical fixed-width v1 codec.
    pub fn encode(&self) -> Result<Vec<u8>, NovBlockExecutionContextError> {
        encode_nov_block_execution_context_v1(self)
    }

    /// Return the domain-separated SHA-256 commitment to the canonical wire.
    pub fn commitment(&self) -> Result<[u8; 32], NovBlockExecutionContextError> {
        nov_block_execution_context_commitment_v1(self)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum NovBlockExecutionContextError {
    #[error("NOV block execution context chain_id must be non-zero")]
    ZeroChainId,
    #[error("NOV block execution context block_height must be non-zero")]
    ZeroBlockHeight,
    #[error("NOV block execution context at height 1 must use the zero parent block hash")]
    UnexpectedGenesisParentBlockHash,
    #[error(
        "NOV block execution context at height {block_height} must have a non-zero parent block hash"
    )]
    MissingParentBlockHash { block_height: u64 },
    #[error("wire length mismatch: expected {expected}, got {got}")]
    LengthMismatch { expected: usize, got: usize },
    #[error("wire magic mismatch")]
    MagicMismatch,
    #[error("wire version mismatch: expected {expected}, got {got}")]
    VersionMismatch { expected: u8, got: u8 },
}

/// Canonical wire layout, in order:
///
/// `NBX1 || version:u8 || chain_id:u64le || block_height:u64le ||
/// parent_block_hash:[u8;32] || slot:u64le || timestamp_unix_ms:u64le`.
pub fn encode_nov_block_execution_context_v1(
    context: &NovBlockExecutionContextV1,
) -> Result<Vec<u8>, NovBlockExecutionContextError> {
    context.validate()?;
    let mut out = Vec::with_capacity(NOV_BLOCK_EXECUTION_CONTEXT_V1_WIRE_LEN);
    out.extend_from_slice(NOV_BLOCK_EXECUTION_CONTEXT_V1_MAGIC);
    out.push(NOV_BLOCK_EXECUTION_CONTEXT_V1_VERSION);
    out.extend_from_slice(&context.chain_id.to_le_bytes());
    out.extend_from_slice(&context.block_height.to_le_bytes());
    out.extend_from_slice(&context.parent_block_hash);
    out.extend_from_slice(&context.slot.to_le_bytes());
    out.extend_from_slice(&context.timestamp_unix_ms.to_le_bytes());
    debug_assert_eq!(out.len(), NOV_BLOCK_EXECUTION_CONTEXT_V1_WIRE_LEN);
    Ok(out)
}

pub fn decode_nov_block_execution_context_v1(
    bytes: &[u8],
) -> Result<NovBlockExecutionContextV1, NovBlockExecutionContextError> {
    if bytes.len() != NOV_BLOCK_EXECUTION_CONTEXT_V1_WIRE_LEN {
        return Err(NovBlockExecutionContextError::LengthMismatch {
            expected: NOV_BLOCK_EXECUTION_CONTEXT_V1_WIRE_LEN,
            got: bytes.len(),
        });
    }
    if &bytes[0..4] != NOV_BLOCK_EXECUTION_CONTEXT_V1_MAGIC {
        return Err(NovBlockExecutionContextError::MagicMismatch);
    }
    if bytes[4] != NOV_BLOCK_EXECUTION_CONTEXT_V1_VERSION {
        return Err(NovBlockExecutionContextError::VersionMismatch {
            expected: NOV_BLOCK_EXECUTION_CONTEXT_V1_VERSION,
            got: bytes[4],
        });
    }

    let mut offset = 5usize;
    let chain_id = read_u64_le_v1(bytes, &mut offset);
    let block_height = read_u64_le_v1(bytes, &mut offset);
    let mut parent_block_hash = [0u8; 32];
    parent_block_hash.copy_from_slice(&bytes[offset..offset + 32]);
    offset += 32;
    let slot = read_u64_le_v1(bytes, &mut offset);
    let timestamp_unix_ms = read_u64_le_v1(bytes, &mut offset);
    debug_assert_eq!(offset, NOV_BLOCK_EXECUTION_CONTEXT_V1_WIRE_LEN);

    let context = NovBlockExecutionContextV1 {
        chain_id,
        block_height,
        parent_block_hash,
        slot,
        timestamp_unix_ms,
    };
    context.validate()?;
    Ok(context)
}

/// Hash the canonical context with an explicit, NUL-terminated v1 domain.
pub fn nov_block_execution_context_commitment_v1(
    context: &NovBlockExecutionContextV1,
) -> Result<[u8; 32], NovBlockExecutionContextError> {
    let wire = encode_nov_block_execution_context_v1(context)?;
    let mut hasher = Sha256::new();
    hasher.update(NOV_BLOCK_EXECUTION_CONTEXT_V1_COMMITMENT_DOMAIN);
    hasher.update(wire);
    Ok(hasher.finalize().into())
}

fn read_u64_le_v1(bytes: &[u8], offset: &mut usize) -> u64 {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&bytes[*offset..*offset + 8]);
    *offset += 8;
    u64::from_le_bytes(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_context() -> NovBlockExecutionContextV1 {
        NovBlockExecutionContextV1 {
            chain_id: 20_260_303,
            block_height: 42,
            parent_block_hash: [0xabu8; 32],
            slot: 47,
            timestamp_unix_ms: 1_800_000_094_000,
        }
    }

    #[test]
    fn block_execution_context_wire_roundtrip_and_layout_are_stable() {
        let context = example_context();
        let wire = encode_nov_block_execution_context_v1(&context).expect("encode context");

        assert_eq!(wire.len(), NOV_BLOCK_EXECUTION_CONTEXT_V1_WIRE_LEN);
        assert_eq!(&wire[0..4], b"NBX1");
        assert_eq!(wire[4], 1);
        assert_eq!(&wire[5..13], &context.chain_id.to_le_bytes());
        assert_eq!(&wire[13..21], &context.block_height.to_le_bytes());
        assert_eq!(&wire[21..53], &context.parent_block_hash);
        assert_eq!(&wire[53..61], &context.slot.to_le_bytes());
        assert_eq!(&wire[61..69], &context.timestamp_unix_ms.to_le_bytes());
        assert_eq!(
            decode_nov_block_execution_context_v1(&wire).expect("decode context"),
            context
        );
    }

    #[test]
    fn block_execution_context_commitment_has_a_golden_vector() {
        let commitment = example_context().commitment().expect("commit context");
        assert_eq!(
            commitment,
            [
                0xa1, 0x89, 0xaa, 0x85, 0x9f, 0xc4, 0x76, 0x83, 0x7b, 0x70, 0xb9, 0x70, 0xe8, 0x83,
                0x64, 0x68, 0x7c, 0x3c, 0xdf, 0xe4, 0x5d, 0x3b, 0x9f, 0xef, 0x07, 0xf4, 0xa6, 0xf9,
                0x49, 0x70, 0x90, 0x44,
            ]
        );
    }

    #[test]
    fn block_execution_context_commitment_binds_every_field() {
        let baseline = example_context();
        let baseline_commitment = baseline.commitment().expect("baseline commitment");
        let variants = [
            NovBlockExecutionContextV1 {
                chain_id: baseline.chain_id + 1,
                ..baseline
            },
            NovBlockExecutionContextV1 {
                block_height: baseline.block_height + 1,
                ..baseline
            },
            NovBlockExecutionContextV1 {
                parent_block_hash: [0xcdu8; 32],
                ..baseline
            },
            NovBlockExecutionContextV1 {
                slot: baseline.slot + 1,
                ..baseline
            },
            NovBlockExecutionContextV1 {
                timestamp_unix_ms: baseline.timestamp_unix_ms + 1,
                ..baseline
            },
        ];

        for variant in variants {
            assert_ne!(
                variant.commitment().expect("variant commitment"),
                baseline_commitment
            );
        }
    }

    #[test]
    fn block_execution_context_validation_is_fail_closed() {
        let baseline = example_context();
        assert_eq!(
            NovBlockExecutionContextV1 {
                chain_id: 0,
                ..baseline
            }
            .validate(),
            Err(NovBlockExecutionContextError::ZeroChainId)
        );
        assert_eq!(
            NovBlockExecutionContextV1 {
                block_height: 0,
                ..baseline
            }
            .validate(),
            Err(NovBlockExecutionContextError::ZeroBlockHeight)
        );
        assert_eq!(
            NovBlockExecutionContextV1 {
                block_height: 2,
                parent_block_hash: [0u8; 32],
                ..baseline
            }
            .validate(),
            Err(NovBlockExecutionContextError::MissingParentBlockHash { block_height: 2 })
        );

        NovBlockExecutionContextV1 {
            block_height: 1,
            parent_block_hash: [0u8; 32],
            ..baseline
        }
        .validate()
        .expect("height-one bootstrap context");
        assert_eq!(
            NovBlockExecutionContextV1 {
                block_height: 1,
                parent_block_hash: [0x11; 32],
                ..baseline
            }
            .validate(),
            Err(NovBlockExecutionContextError::UnexpectedGenesisParentBlockHash)
        );
    }

    #[test]
    fn block_execution_context_decode_rejects_noncanonical_wire() {
        let wire = example_context().encode().expect("encode context");

        let short = &wire[..wire.len() - 1];
        assert!(matches!(
            decode_nov_block_execution_context_v1(short),
            Err(NovBlockExecutionContextError::LengthMismatch { .. })
        ));

        let mut bad_magic = wire.clone();
        bad_magic[0] ^= 0xff;
        assert_eq!(
            decode_nov_block_execution_context_v1(&bad_magic),
            Err(NovBlockExecutionContextError::MagicMismatch)
        );

        let mut bad_version = wire;
        bad_version[4] = 2;
        assert_eq!(
            decode_nov_block_execution_context_v1(&bad_version),
            Err(NovBlockExecutionContextError::VersionMismatch {
                expected: 1,
                got: 2,
            })
        );
    }
}
