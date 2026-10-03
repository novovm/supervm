#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use thiserror::Error;

pub const LOCAL_TX_WIRE_V1_CODEC: &str = "novovm_local_tx_wire_v1";
pub const NOV_NATIVE_TX_WIRE_V1_CODEC: &str = "novovm_native_tx_wire_v1_postcard";
pub const NOV_NATIVE_TX_WIRE_V2_CODEC: &str = "novovm_native_tx_wire_v2_postcard";
pub const NOV_NATIVE_TX_WIRE_V3_CODEC: &str = "novovm_native_tx_wire_v3_postcard";
const LOCAL_TX_WIRE_MAGIC: &[u8; 4] = b"NTX1";
const LOCAL_TX_WIRE_VERSION: u8 = 1;
const NOV_NATIVE_TX_WIRE_MAGIC: &[u8; 4] = b"NNX1";
const NOV_NATIVE_TX_WIRE_VERSION_V1: u8 = 1;
const NOV_NATIVE_TX_WIRE_VERSION_V2: u8 = 2;
const NOV_NATIVE_TX_WIRE_VERSION_V3: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalTxWireV1 {
    pub account: u64,
    pub key: u64,
    pub value: u64,
    pub nonce: u64,
    pub fee: u64,
    pub signature: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovFeePolicyV1 {
    pub pay_asset: String,
    pub max_pay_amount: u128,
    pub slippage_bps: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NovExecutionTargetV1 {
    NativeModule(String),
    WasmApp(String),
    Plugin(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NovExecutionModeV1 {
    Standard,
    HighPriority,
    Batch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NovPrivacyModeV1 {
    Public,
    Private,
    Confidential,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NovVerificationModeV1 {
    Standard,
    Auditable,
    MandatoryZk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum NovExecutionPolicyV1 {
    #[default]
    Standard,
    PqRequired,
    PrivacyRequired,
}

impl NovExecutionPolicyV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::PqRequired => "pq_required",
            Self::PrivacyRequired => "privacy_required",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NovGovernanceProposalTypeV1 {
    Parameter,
    Treasury,
    Protocol,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovTransferTxV1 {
    pub from: Vec<u8>,
    pub to: Vec<u8>,
    pub asset: String,
    pub amount: u128,
    pub nonce: u64,
    pub fee_policy: NovFeePolicyV1,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovExecuteTxV1 {
    pub caller: Vec<u8>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub fee_owner_account_id: Option<String>,
    #[serde(default)]
    pub nonce_owner_account_id: Option<String>,
    pub target: NovExecutionTargetV1,
    pub method: String,
    pub args: Vec<u8>,
    pub execution_mode: NovExecutionModeV1,
    #[serde(default)]
    pub execution_policy: NovExecutionPolicyV1,
    pub privacy_mode: NovPrivacyModeV1,
    pub verification_mode: NovVerificationModeV1,
    pub fee_policy: NovFeePolicyV1,
    pub gas_like_limit: Option<u64>,
    pub nonce: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovGovernanceTxV1 {
    pub proposer: Vec<u8>,
    pub proposal_type: NovGovernanceProposalTypeV1,
    pub payload: Vec<u8>,
    pub nonce: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NovTxKindV1 {
    Transfer(NovTransferTxV1),
    Execute(NovExecuteTxV1),
    Governance(NovGovernanceTxV1),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovNativeTxWireV1 {
    pub chain_id: u64,
    pub kind: NovTxKindV1,
    /// Native authentication payload.
    ///
    /// Version 3 encodes `ed25519 public key (32) || signature (64)`. The
    /// decoder still accepts a version-1 wire and exposes its legacy 32-byte
    /// value here so ingress can reject it with an authentication-specific
    /// error instead of a generic codec failure.
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct NovNativeTxWireLegacyV1 {
    chain_id: u64,
    kind: NovTxKindV1,
    signature: [u8; 32],
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

#[derive(Debug, Error)]
pub enum NativeTxWireError {
    #[error("wire length mismatch: expected >= {expected}, got {got}")]
    LengthMismatch { expected: usize, got: usize },
    #[error("wire magic mismatch")]
    MagicMismatch,
    #[error("wire version mismatch: expected {expected}, got {got}")]
    VersionMismatch { expected: u8, got: u8 },
    #[error("wire encode failed: {0}")]
    EncodeFailed(String),
    #[error("wire decode failed: {0}")]
    DecodeFailed(String),
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

pub fn encode_nov_native_tx_wire_v1(tx: &NovNativeTxWireV1) -> Result<Vec<u8>, NativeTxWireError> {
    let payload = postcard::to_allocvec(tx)
        .map_err(|err| NativeTxWireError::EncodeFailed(err.to_string()))?;
    let mut out = Vec::with_capacity(4 + 1 + payload.len());
    out.extend_from_slice(NOV_NATIVE_TX_WIRE_MAGIC);
    out.push(NOV_NATIVE_TX_WIRE_VERSION_V3);
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Returns the domain-separated identity of every signed native transaction
/// field. The authentication payload is deliberately cleared before encoding,
/// so the commitment is stable before and after signing while still binding
/// the complete version-3 wire intent.
pub fn native_tx_unsigned_commitment_v3(
    tx: &NovNativeTxWireV1,
) -> Result<[u8; 32], NativeTxWireError> {
    let mut unsigned = tx.clone();
    unsigned.signature.clear();
    let wire = encode_nov_native_tx_wire_v1(&unsigned)?;
    let mut hasher = Sha256::new();
    hasher.update(b"novovm-native-tx-unsigned-commitment-v3");
    hasher.update((wire.len() as u64).to_le_bytes());
    hasher.update(wire.as_slice());
    Ok(hasher.finalize().into())
}

pub fn decode_nov_native_tx_wire_v1(bytes: &[u8]) -> Result<NovNativeTxWireV1, NativeTxWireError> {
    let header_len = 4 + 1;
    if bytes.len() < header_len {
        return Err(NativeTxWireError::LengthMismatch {
            expected: header_len,
            got: bytes.len(),
        });
    }
    if &bytes[..4] != NOV_NATIVE_TX_WIRE_MAGIC {
        return Err(NativeTxWireError::MagicMismatch);
    }
    match bytes[4] {
        NOV_NATIVE_TX_WIRE_VERSION_V3 => postcard::from_bytes(&bytes[header_len..])
            .map_err(|err| NativeTxWireError::DecodeFailed(err.to_string())),
        NOV_NATIVE_TX_WIRE_VERSION_V2 => Err(NativeTxWireError::VersionMismatch {
            expected: NOV_NATIVE_TX_WIRE_VERSION_V3,
            got: NOV_NATIVE_TX_WIRE_VERSION_V2,
        }),
        NOV_NATIVE_TX_WIRE_VERSION_V1 => {
            let legacy: NovNativeTxWireLegacyV1 = postcard::from_bytes(&bytes[header_len..])
                .map_err(|err| NativeTxWireError::DecodeFailed(err.to_string()))?;
            Ok(NovNativeTxWireV1 {
                chain_id: legacy.chain_id,
                kind: legacy.kind,
                signature: legacy.signature.to_vec(),
            })
        }
        got => Err(NativeTxWireError::VersionMismatch {
            expected: NOV_NATIVE_TX_WIRE_VERSION_V3,
            got,
        }),
    }
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

    #[test]
    fn nov_native_tx_wire_roundtrip_transfer_execute_governance() {
        let transfer = NovNativeTxWireV1 {
            chain_id: 1,
            kind: NovTxKindV1::Transfer(NovTransferTxV1 {
                from: vec![0x11; 20],
                to: vec![0x22; 20],
                asset: "NOV".to_string(),
                amount: 100,
                nonce: 1,
                fee_policy: NovFeePolicyV1 {
                    pay_asset: "NOV".to_string(),
                    max_pay_amount: 2,
                    slippage_bps: 50,
                },
            }),
            signature: vec![0xabu8; 96],
        };
        let transfer_wire = encode_nov_native_tx_wire_v1(&transfer).expect("encode transfer");
        let transfer_decoded =
            decode_nov_native_tx_wire_v1(&transfer_wire).expect("decode transfer");
        assert_eq!(transfer_decoded, transfer);

        let execute = NovNativeTxWireV1 {
            chain_id: 777,
            kind: NovTxKindV1::Execute(NovExecuteTxV1 {
                caller: vec![0x33; 20],
                account_id: Some("acct-roundtrip".to_string()),
                fee_owner_account_id: Some("acct-fee".to_string()),
                nonce_owner_account_id: Some("acct-nonce".to_string()),
                target: NovExecutionTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: vec![1, 2, 3],
                execution_mode: NovExecutionModeV1::Batch,
                execution_policy: NovExecutionPolicyV1::PqRequired,
                privacy_mode: NovPrivacyModeV1::Confidential,
                verification_mode: NovVerificationModeV1::MandatoryZk,
                fee_policy: NovFeePolicyV1 {
                    pay_asset: "USDT".to_string(),
                    max_pay_amount: 999,
                    slippage_bps: 120,
                },
                gas_like_limit: Some(300_000),
                nonce: 9,
            }),
            signature: vec![0x55; 96],
        };
        let execute_wire = encode_nov_native_tx_wire_v1(&execute).expect("encode execute");
        let execute_decoded = decode_nov_native_tx_wire_v1(&execute_wire).expect("decode execute");
        assert_eq!(execute_decoded, execute);

        let execute_commitment =
            native_tx_unsigned_commitment_v3(&execute).expect("commit execute intent");
        let mut signature_only = execute.clone();
        signature_only.signature = vec![0x99; 96];
        assert_eq!(
            native_tx_unsigned_commitment_v3(&signature_only).expect("commit re-signed intent"),
            execute_commitment,
            "authentication bytes are not part of the unsigned intent"
        );

        let mut execution_mode_changed = execute.clone();
        let NovTxKindV1::Execute(item) = &mut execution_mode_changed.kind else {
            unreachable!("execute fixture")
        };
        item.execution_mode = NovExecutionModeV1::HighPriority;
        assert_ne!(
            native_tx_unsigned_commitment_v3(&execution_mode_changed)
                .expect("commit execution-mode mutation"),
            execute_commitment
        );

        let mut privacy_changed = execute.clone();
        let NovTxKindV1::Execute(item) = &mut privacy_changed.kind else {
            unreachable!("execute fixture")
        };
        item.privacy_mode = NovPrivacyModeV1::Private;
        assert_ne!(
            native_tx_unsigned_commitment_v3(&privacy_changed).expect("commit privacy mutation"),
            execute_commitment
        );

        let mut verification_changed = execute.clone();
        let NovTxKindV1::Execute(item) = &mut verification_changed.kind else {
            unreachable!("execute fixture")
        };
        item.verification_mode = NovVerificationModeV1::Auditable;
        assert_ne!(
            native_tx_unsigned_commitment_v3(&verification_changed)
                .expect("commit verification mutation"),
            execute_commitment
        );

        let mut fee_changed = execute.clone();
        let NovTxKindV1::Execute(item) = &mut fee_changed.kind else {
            unreachable!("execute fixture")
        };
        item.fee_policy.max_pay_amount += 1;
        assert_ne!(
            native_tx_unsigned_commitment_v3(&fee_changed).expect("commit fee mutation"),
            execute_commitment
        );

        let governance = NovNativeTxWireV1 {
            chain_id: 3,
            kind: NovTxKindV1::Governance(NovGovernanceTxV1 {
                proposer: vec![0x44; 20],
                proposal_type: NovGovernanceProposalTypeV1::Parameter,
                payload: b"{\"set\":\"m2_limit\"}".to_vec(),
                nonce: 19,
            }),
            signature: vec![0x66; 96],
        };
        let governance_wire = encode_nov_native_tx_wire_v1(&governance).expect("encode governance");
        let governance_decoded =
            decode_nov_native_tx_wire_v1(&governance_wire).expect("decode governance");
        assert_eq!(governance_decoded, governance);

        let governance_commitment =
            native_tx_unsigned_commitment_v3(&governance).expect("commit governance intent");
        let mut governance_type_changed = governance.clone();
        let NovTxKindV1::Governance(item) = &mut governance_type_changed.kind else {
            unreachable!("governance fixture")
        };
        item.proposal_type = NovGovernanceProposalTypeV1::Treasury;
        assert_ne!(
            native_tx_unsigned_commitment_v3(&governance_type_changed)
                .expect("commit governance-type mutation"),
            governance_commitment
        );
    }

    #[test]
    fn nov_native_tx_wire_decodes_legacy_auth_for_explicit_ingress_rejection() {
        let legacy = NovNativeTxWireLegacyV1 {
            chain_id: 77,
            kind: NovTxKindV1::Transfer(NovTransferTxV1 {
                from: vec![0x11; 20],
                to: vec![0x22; 20],
                asset: "NOV".to_string(),
                amount: 1,
                nonce: 1,
                fee_policy: NovFeePolicyV1 {
                    pay_asset: "NOV".to_string(),
                    max_pay_amount: 1,
                    slippage_bps: 0,
                },
            }),
            signature: [0x44; 32],
        };
        let payload = postcard::to_allocvec(&legacy).expect("encode legacy native tx");
        let mut wire = Vec::with_capacity(5 + payload.len());
        wire.extend_from_slice(NOV_NATIVE_TX_WIRE_MAGIC);
        wire.push(NOV_NATIVE_TX_WIRE_VERSION_V1);
        wire.extend_from_slice(&payload);

        let decoded = decode_nov_native_tx_wire_v1(&wire).expect("decode legacy native tx");
        assert_eq!(decoded.chain_id, legacy.chain_id);
        assert_eq!(decoded.kind, legacy.kind);
        assert_eq!(decoded.signature, legacy.signature);
    }
}
