#![forbid(unsafe_code)]

use thiserror::Error;

pub const LOCAL_TX_WIRE_V1_CODEC: &str = "novovm_local_tx_wire_v1";
const LOCAL_TX_WIRE_MAGIC: &[u8; 4] = b"NTX1";
const LOCAL_TX_WIRE_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalTxWireV1 {
    pub account: u64,
    pub key: u64,
    pub value: u64,
    pub nonce: u64,
    pub fee: u64,
    pub signature: [u8; 32],
}

#[derive(Debug, Error)]
pub enum TxWireError {
    #[error("wire length mismatch: expected {expected}, got {got}")]
    LengthMismatch { expected: usize, got: usize },
    #[error("wire magic mismatch")]
    MagicMismatch,
    #[error("wire version mismatch: expected {expected}, got {got}")]
    VersionMismatch { expected: u8, got: u8 },
}

pub fn encode_local_tx_wire_v1(tx: &LocalTxWireV1) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 1 + (8 * 5) + 32);
    out.extend_from_slice(LOCAL_TX_WIRE_MAGIC);
    out.push(LOCAL_TX_WIRE_VERSION);
    out.extend_from_slice(&tx.account.to_le_bytes());
    out.extend_from_slice(&tx.key.to_le_bytes());
    out.extend_from_slice(&tx.value.to_le_bytes());
    out.extend_from_slice(&tx.nonce.to_le_bytes());
    out.extend_from_slice(&tx.fee.to_le_bytes());
    out.extend_from_slice(&tx.signature);
    out
}

pub fn decode_local_tx_wire_v1(bytes: &[u8]) -> Result<LocalTxWireV1, TxWireError> {
    let expected_len = 4 + 1 + (8 * 5) + 32;
    if bytes.len() != expected_len {
        return Err(TxWireError::LengthMismatch {
            expected: expected_len,
            got: bytes.len(),
        });
    }
    if &bytes[0..4] != LOCAL_TX_WIRE_MAGIC {
        return Err(TxWireError::MagicMismatch);
    }
    if bytes[4] != LOCAL_TX_WIRE_VERSION {
        return Err(TxWireError::VersionMismatch {
            expected: LOCAL_TX_WIRE_VERSION,
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

    let account = read_u64(bytes, &mut off);
    let key = read_u64(bytes, &mut off);
    let value = read_u64(bytes, &mut off);
    let nonce = read_u64(bytes, &mut off);
    let fee = read_u64(bytes, &mut off);
    let mut signature = [0u8; 32];
    signature.copy_from_slice(&bytes[off..(off + 32)]);

    Ok(LocalTxWireV1 {
        account,
        key,
        value,
        nonce,
        fee,
        signature,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tx_wire_roundtrip() {
        let tx = LocalTxWireV1 {
            account: 1000,
            key: 11,
            value: 22,
            nonce: 3,
            fee: 1,
            signature: [9u8; 32],
        };
        let wire = encode_local_tx_wire_v1(&tx);
        let decoded = decode_local_tx_wire_v1(&wire).unwrap();
        assert_eq!(decoded, tx);
    }

    #[test]
    fn tx_wire_rejects_bad_magic() {
        let tx = LocalTxWireV1 {
            account: 1,
            key: 2,
            value: 3,
            nonce: 4,
            fee: 5,
            signature: [0u8; 32],
        };
        let mut wire = encode_local_tx_wire_v1(&tx);
        wire[0] = b'X';
        let err = decode_local_tx_wire_v1(&wire).unwrap_err().to_string();
        assert!(err.contains("magic mismatch"));
    }
}
