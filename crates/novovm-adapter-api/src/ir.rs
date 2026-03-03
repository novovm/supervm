#![forbid(unsafe_code)]

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[cfg(feature = "serde_json")]
use serde_json;

/// Transaction IR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxIR {
    pub hash: Vec<u8>,
    pub from: Vec<u8>,
    pub to: Option<Vec<u8>>,
    pub value: u128,
    pub gas_limit: u64,
    pub gas_price: u64,
    pub nonce: u64,
    pub data: Vec<u8>,
    pub signature: Vec<u8>,
    pub chain_id: u64,
    pub tx_type: TxType,

    // Phase 5.3 (optional cross-chain hints)
    pub source_chain: Option<u64>,
    pub target_chain: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxType {
    Transfer,
    ContractCall,
    ContractDeploy,
    Privacy,
    CrossShard,
    CrossChainTransfer,
    CrossChainCall,
}

/// Block IR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockIR {
    pub hash: Vec<u8>,
    pub parent_hash: Vec<u8>,
    pub number: u64,
    pub timestamp: u64,
    pub transactions: Vec<TxIR>,
    pub state_root: Vec<u8>,
    pub transactions_root: Vec<u8>,
    pub receipts_root: Vec<u8>,
    pub miner: Vec<u8>,
    pub difficulty: u64,
    pub gas_used: u64,
    pub gas_limit: u64,
}

/// State IR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateIR {
    pub accounts: std::collections::HashMap<Vec<u8>, AccountState>,
    pub storage: std::collections::HashMap<Vec<u8>, std::collections::HashMap<Vec<u8>, Vec<u8>>>,
    pub state_root: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountState {
    pub balance: u128,
    pub nonce: u64,
    pub code_hash: Option<Vec<u8>>,
    pub storage_root: Vec<u8>,
}

impl StateIR {
    #[must_use]
    pub fn new() -> Self {
        Self {
            accounts: std::collections::HashMap::new(),
            storage: std::collections::HashMap::new(),
            state_root: vec![],
        }
    }

    pub fn get_account(&self, address: &[u8]) -> Option<&AccountState> {
        self.accounts.get(address)
    }

    pub fn set_account(&mut self, address: Vec<u8>, account: AccountState) {
        self.accounts.insert(address, account);
    }

    pub fn get_storage(&self, address: &[u8], key: &[u8]) -> Option<&Vec<u8>> {
        self.storage.get(address)?.get(key)
    }

    pub fn set_storage(&mut self, address: Vec<u8>, key: Vec<u8>, value: Vec<u8>) {
        self.storage.entry(address).or_default().insert(key, value);
    }
}

impl Default for StateIR {
    fn default() -> Self {
        Self::new()
    }
}

impl TxIR {
    #[must_use]
    pub fn transfer(from: Vec<u8>, to: Vec<u8>, value: u128, nonce: u64, chain_id: u64) -> Self {
        Self {
            hash: vec![],
            from,
            to: Some(to),
            value,
            gas_limit: 21_000,
            gas_price: 1,
            nonce,
            data: vec![],
            signature: vec![],
            chain_id,
            tx_type: TxType::Transfer,
            source_chain: None,
            target_chain: None,
        }
    }

    pub fn compute_hash(&mut self) {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&self.from);
        if let Some(to) = &self.to {
            hasher.update(to);
        }
        hasher.update(self.value.to_le_bytes());
        hasher.update(self.nonce.to_le_bytes());
        hasher.update(&self.data);
        self.hash = hasher.finalize().to_vec();
    }

    #[must_use]
    pub fn cross_chain_transfer(
        from: Vec<u8>,
        to: Vec<u8>,
        value: u128,
        nonce: u64,
        source_chain: u64,
        target_chain: u64,
    ) -> Self {
        Self {
            hash: vec![],
            from,
            to: Some(to),
            value,
            gas_limit: 100_000,
            gas_price: 1,
            nonce,
            data: vec![],
            signature: vec![],
            chain_id: source_chain,
            tx_type: TxType::CrossChainTransfer,
            source_chain: Some(source_chain),
            target_chain: Some(target_chain),
        }
    }
}

impl BlockIR {
    #[must_use]
    pub fn genesis(chain_id: u64) -> Self {
        let _ = chain_id; // reserved for future extension
        Self {
            hash: vec![0; 32],
            parent_hash: vec![0; 32],
            number: 0,
            timestamp: 0,
            transactions: vec![],
            state_root: vec![0; 32],
            transactions_root: vec![0; 32],
            receipts_root: vec![0; 32],
            miner: vec![0; 20],
            difficulty: 0,
            gas_used: 0,
            gas_limit: 10_000_000,
        }
    }

    pub fn serialize(&self, format: SerializationFormat) -> Result<Vec<u8>> {
        match format {
            SerializationFormat::Json => {
                #[cfg(feature = "serde_json")]
                {
                    Ok(serde_json::to_vec(self)?)
                }
                #[cfg(not(feature = "serde_json"))]
                bail!("JSON serialization requires 'serde_json' feature")
            }
            SerializationFormat::Bincode => Ok(bincode::serialize(self)?),
        }
    }

    pub fn deserialize(data: &[u8], format: SerializationFormat) -> Result<Self> {
        match format {
            SerializationFormat::Json => {
                #[cfg(feature = "serde_json")]
                {
                    Ok(serde_json::from_slice(data)?)
                }
                #[cfg(not(feature = "serde_json"))]
                bail!("JSON deserialization requires 'serde_json' feature")
            }
            SerializationFormat::Bincode => Ok(bincode::deserialize(data)?),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializationFormat {
    Json,
    Bincode,
}

impl TxIR {
    pub fn serialize(&self, format: SerializationFormat) -> Result<Vec<u8>> {
        match format {
            SerializationFormat::Json => {
                #[cfg(feature = "serde_json")]
                {
                    Ok(serde_json::to_vec(self)?)
                }
                #[cfg(not(feature = "serde_json"))]
                bail!("JSON serialization requires 'serde_json' feature")
            }
            SerializationFormat::Bincode => Ok(bincode::serialize(self)?),
        }
    }

    pub fn deserialize(data: &[u8], format: SerializationFormat) -> Result<Self> {
        match format {
            SerializationFormat::Json => {
                #[cfg(feature = "serde_json")]
                {
                    Ok(serde_json::from_slice(data)?)
                }
                #[cfg(not(feature = "serde_json"))]
                bail!("JSON deserialization requires 'serde_json' feature")
            }
            SerializationFormat::Bincode => Ok(bincode::deserialize(data)?),
        }
    }
}

impl StateIR {
    pub fn serialize(&self, format: SerializationFormat) -> Result<Vec<u8>> {
        match format {
            SerializationFormat::Json => {
                #[cfg(feature = "serde_json")]
                {
                    Ok(serde_json::to_vec(self)?)
                }
                #[cfg(not(feature = "serde_json"))]
                bail!("JSON serialization requires 'serde_json' feature")
            }
            SerializationFormat::Bincode => Ok(bincode::serialize(self)?),
        }
    }

    pub fn deserialize(data: &[u8], format: SerializationFormat) -> Result<Self> {
        match format {
            SerializationFormat::Json => {
                #[cfg(feature = "serde_json")]
                {
                    Ok(serde_json::from_slice(data)?)
                }
                #[cfg(not(feature = "serde_json"))]
                bail!("JSON deserialization requires 'serde_json' feature")
            }
            SerializationFormat::Bincode => Ok(bincode::deserialize(data)?),
        }
    }
}
