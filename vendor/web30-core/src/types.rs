//! WEB30 核心数据类型

use serde::{Deserialize, Serialize};

/// 地址类型（32 字节）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address(pub [u8; 32]);

impl Address {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn zero() -> Self {
        Self([0u8; 32])
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 使用简单的十六进制编码
        write!(f, "0x")?;
        for byte in &self.0[..8] {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

/// 隐身地址（用于隐私转账）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthAddress {
    pub view_key: [u8; 32],
    pub spend_key: [u8; 32],
}

/// 转账收据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferReceipt {
    pub tx_hash: [u8; 32],
    pub from: Address,
    pub to: Address,
    pub amount: u128,
    pub timestamp: u64,
    pub gas_used: u64,
}

/// 跨链收据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossChainReceipt {
    pub swap_id: [u8; 32],
    pub from_chain: ChainId,
    pub to_chain: ChainId,
    pub from_address: Address,
    pub to_address: Address,
    pub amount: u128,
    pub status: CrossChainStatus,
}

pub type ChainId = u64;
pub type ProposalId = u64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CrossChainStatus {
    Pending,
    Confirmed,
    Failed,
}

/// 代币元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub icon_uri: String,
    pub description: String,
    pub website: String,
    pub social: SocialLinks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialLinks {
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub discord: Option<String>,
}

/// 环签名（隐私转账）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RingSignature {
    pub ring_members: Vec<Address>,
    pub key_image: [u8; 32],
    pub c: Vec<[u8; 32]>,
    pub r: Vec<[u8; 32]>,
}

/// 治理提案
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: ProposalId,
    pub proposer: Address,
    pub title: String,
    pub description: String,
    pub actions: Vec<ProposalAction>,
    pub start_time: u64,
    pub end_time: u64,
    pub votes_for: u128,
    pub votes_against: u128,
    pub status: ProposalStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProposalAction {
    UpdateMetadata(TokenMetadata),
    Mint { to: Address, amount: u128 },
    Burn { amount: u128 },
    Freeze { account: Address },
    Unfreeze { account: Address },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProposalStatus {
    Pending,
    Active,
    Succeeded,
    Failed,
    Executed,
}
