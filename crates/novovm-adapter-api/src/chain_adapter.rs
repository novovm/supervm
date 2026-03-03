#![forbid(unsafe_code)]

use crate::ir::{BlockIR, StateIR, TxIR};
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Chain types supported by the adapter interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChainType {
    NovoVM,
    EVM,
    Bitcoin,
    Solana,
    TRON,
    Polygon,
    BNB,
    Avalanche,
    Cardano,
    Sui,
    Aptos,
    TON,
    KOT,
    Custom,
}

impl ChainType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            ChainType::NovoVM => "novovm",
            ChainType::EVM => "evm",
            ChainType::Bitcoin => "bitcoin",
            ChainType::Solana => "solana",
            ChainType::TRON => "tron",
            ChainType::Polygon => "polygon",
            ChainType::BNB => "bnb",
            ChainType::Avalanche => "avalanche",
            ChainType::Cardano => "cardano",
            ChainType::Sui => "sui",
            ChainType::Aptos => "aptos",
            ChainType::TON => "ton",
            ChainType::KOT => "kot",
            ChainType::Custom => "custom",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "novovm" | "supervm" => ChainType::NovoVM,
            "evm" => ChainType::EVM,
            "bitcoin" => ChainType::Bitcoin,
            "solana" => ChainType::Solana,
            "tron" => ChainType::TRON,
            "polygon" => ChainType::Polygon,
            "bnb" => ChainType::BNB,
            "avalanche" => ChainType::Avalanche,
            "cardano" => ChainType::Cardano,
            "sui" => ChainType::Sui,
            "aptos" => ChainType::Aptos,
            "ton" => ChainType::TON,
            "kot" => ChainType::KOT,
            "custom" => ChainType::Custom,
            _ => anyhow::bail!("Unknown chain type: {s}"),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub chain_type: ChainType,
    pub chain_id: u64,
    pub name: String,
    pub enabled: bool,
    pub custom_config: Option<String>,
}

impl ChainConfig {
    #[must_use]
    pub fn novovm(chain_id: u64) -> Self {
        Self {
            chain_type: ChainType::NovoVM,
            chain_id,
            name: "NOVOVM".to_string(),
            enabled: true,
            custom_config: None,
        }
    }
}

/// Recommended default chain id by chain type for built-in adapter routing.
#[must_use]
pub fn default_chain_id(chain_type: ChainType) -> u64 {
    match chain_type {
        ChainType::NovoVM => 20260303,
        ChainType::EVM => 1,
        ChainType::Bitcoin => 0,
        ChainType::Solana => 101,
        ChainType::TRON => 728126428,
        ChainType::Polygon => 137,
        ChainType::BNB => 56,
        ChainType::Avalanche => 43114,
        ChainType::Cardano => 1815,
        ChainType::Sui => 784,
        ChainType::Aptos => 2,
        ChainType::TON => 607,
        ChainType::KOT => 8888,
        ChainType::Custom => 9_999_999,
    }
}

/// Factory type for creating adapters from a `ChainConfig`.
pub type AdapterFactory = fn(ChainConfig) -> Box<dyn ChainAdapter>;

/// Chain adapter interface (L1: host-facing, external-chain-facing).
///
/// Important: this crate defines the interface only. Implementations live in third-party plugins.
pub trait ChainAdapter: Send + Sync {
    fn chain_type(&self) -> ChainType;

    fn config(&self) -> &ChainConfig;

    fn initialize(&mut self) -> Result<()>;

    fn shutdown(&mut self) -> Result<()>;

    // ===== Tx =====

    fn parse_transaction(&self, raw_tx: &[u8]) -> Result<TxIR>;

    fn verify_transaction(&self, tx: &TxIR) -> Result<bool>;

    fn execute_transaction(&mut self, tx: &TxIR, state: &mut StateIR) -> Result<()>;

    fn estimate_gas(&self, tx: &TxIR) -> Result<u64>;

    // ===== Block =====

    fn parse_block(&self, raw_block: &[u8]) -> Result<BlockIR>;

    fn verify_block(&self, block: &BlockIR) -> Result<bool>;

    fn apply_block(&mut self, block: &BlockIR, state: &mut StateIR) -> Result<()>;

    // ===== State =====

    fn read_state(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;

    fn write_state(&mut self, key: &[u8], value: Vec<u8>) -> Result<()>;

    fn delete_state(&mut self, key: &[u8]) -> Result<()>;

    fn state_root(&self) -> Result<Vec<u8>>;

    // ===== Convenience =====

    fn get_balance(&self, address: &[u8]) -> Result<u128>;

    fn get_nonce(&self, address: &[u8]) -> Result<u64>;

    fn get_code(&self, _address: &[u8]) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }

    fn supports_smart_contracts(&self) -> bool {
        false
    }

    fn supports_privacy(&self) -> bool {
        false
    }

    fn version(&self) -> &str {
        "1.0.0"
    }
}
