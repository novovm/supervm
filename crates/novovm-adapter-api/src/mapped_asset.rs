#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

fn default_phase4_mode_shadow_v1() -> String {
    "shadow".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappedAssetSourceChain {
    Ethereum,
    Other(String),
}

impl MappedAssetSourceChain {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Ethereum => "ethereum",
            Self::Other(other) => other.as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappedLockProofFormat {
    EthereumLockEventV1,
}

impl MappedLockProofFormat {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EthereumLockEventV1 => "ethereum_lock_event_v1",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappedAssetLockProof {
    pub lock_id: [u8; 32],
    pub source_chain: MappedAssetSourceChain,
    pub source_asset_symbol: String,
    pub source_tx_hash: Vec<u8>,
    pub source_lock_ref: Vec<u8>,
    pub external_owner_ref: Vec<u8>,
    pub target_account_id: String,
    pub amount: u128,
    pub proof_payload: Vec<u8>,
    pub proof_format: MappedLockProofFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappedAssetStatus {
    Registered,
    Active,
    BurnPending,
    Frozen,
    Released,
    Rejected,
}

impl MappedAssetStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Registered => "registered",
            Self::Active => "active",
            Self::BurnPending => "burn_pending",
            Self::Frozen => "frozen",
            Self::Released => "released",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappedAssetRecord {
    pub mapping_id: [u8; 32],
    pub lock_id: [u8; 32],
    pub source_chain: MappedAssetSourceChain,
    pub source_asset_symbol: String,
    pub source_tx_hash: Vec<u8>,
    pub source_lock_ref: Vec<u8>,
    #[serde(default)]
    pub source_chain_id: Option<u64>,
    #[serde(default)]
    pub source_block_number: Option<u64>,
    #[serde(default)]
    pub source_block_hash: Vec<u8>,
    #[serde(default)]
    pub source_receipts_root: Vec<u8>,
    #[serde(default)]
    pub source_finalized_block_number: Option<u64>,
    #[serde(default)]
    pub source_log_index: Option<u64>,
    #[serde(default)]
    pub source_receipt_index: Option<u64>,
    #[serde(default)]
    pub source_receipt_log_index: Option<u64>,
    pub external_owner_ref: Vec<u8>,
    pub target_asset_symbol: String,
    pub target_account_id: String,
    pub amount: u128,
    #[serde(default = "default_phase4_mode_shadow_v1")]
    pub phase4_mode: String,
    pub status: MappedAssetStatus,
    pub created_at: u64,
    pub updated_at: u64,
    pub audit_ref: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappedAssetOperationKind {
    RegisterLock,
    BurnMapped,
    FreezeMapped,
    ReleaseSource,
}

impl MappedAssetOperationKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RegisterLock => "register_lock",
            Self::BurnMapped => "burn_mapped",
            Self::FreezeMapped => "freeze_mapped",
            Self::ReleaseSource => "release_source",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappedAssetOperation {
    pub op_id: [u8; 32],
    pub mapping_id: [u8; 32],
    pub kind: MappedAssetOperationKind,
    pub account_id: String,
    pub amount: u128,
    pub created_at: u64,
}
