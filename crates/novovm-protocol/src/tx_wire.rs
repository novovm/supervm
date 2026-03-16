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

    fn fuzz_env_u64(name: &str, default: u64) -> u64 {
        std::env::var(name)
            .ok()
            .and_then(|raw| raw.trim().parse::<u64>().ok())
            .unwrap_or(default)
    }

    fn fuzz_env_usize(name: &str, default: usize) -> usize {
        std::env::var(name)
            .ok()
            .and_then(|raw| raw.trim().parse::<usize>().ok())
            .unwrap_or(default)
    }

    fn fuzz_next(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        *state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn mutate_bytes(state: &mut u64, base: &[u8]) -> Vec<u8> {
        let mut out = base.to_vec();
        match fuzz_next(state) % 4 {
            0 => {
                if !out.is_empty() {
                    let idx = (fuzz_next(state) as usize) % out.len();
                    out[idx] ^= (fuzz_next(state) & 0xff) as u8;
                }
            }
            1 => {
                let flips = ((fuzz_next(state) % 4) + 1) as usize;
                for _ in 0..flips {
                    if out.is_empty() {
                        break;
                    }
                    let idx = (fuzz_next(state) as usize) % out.len();
                    out[idx] = (fuzz_next(state) & 0xff) as u8;
                }
            }
            2 => {
                if !out.is_empty() {
                    let keep = (fuzz_next(state) as usize) % out.len();
                    out.truncate(keep);
                }
            }
            _ => {
                let append = ((fuzz_next(state) % 8) + 1) as usize;
                for _ in 0..append {
                    out.push((fuzz_next(state) & 0xff) as u8);
                }
            }
        }
        out
    }

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

    #[test]
    fn fuzz_min_tx_wire_decode_seeded_no_panic() {
        let seed = fuzz_env_u64("NOVOVM_FUZZ_MIN_SEED", 20260313);
        let iterations = fuzz_env_usize("NOVOVM_FUZZ_MIN_TX_ITERS", 5000);
        let mut state = seed.max(1);

        let valid = LocalTxWireV1 {
            account: 42,
            key: 7,
            value: 9001,
            nonce: 3,
            fee: 1,
            signature: [0xabu8; 32],
        };
        let valid_wire = encode_local_tx_wire_v1(&valid);
        let expected_len = 4usize + 1 + (8 * 5) + 32;

        let mut short_wire = valid_wire.clone();
        short_wire.truncate(expected_len.saturating_sub(3));
        let mut bad_magic = valid_wire.clone();
        bad_magic[0] ^= 0xff;
        let mut bad_version = valid_wire.clone();
        bad_version[4] = bad_version[4].wrapping_add(1);

        let corpus = [
            valid_wire,
            short_wire,
            bad_magic,
            bad_version,
            Vec::new(),
            b"NTX1".to_vec(),
            vec![0u8; expected_len],
            vec![0xffu8; expected_len],
        ];

        for _ in 0..iterations {
            let idx = (fuzz_next(&mut state) as usize) % corpus.len();
            let sample = mutate_bytes(&mut state, &corpus[idx]);
            let _ = decode_local_tx_wire_v1(&sample);
        }

        println!(
            "fuzz_min_tx_wire: seed={} iterations={} corpus={} expected_len={}",
            seed,
            iterations,
            corpus.len(),
            expected_len
        );
    }
}
