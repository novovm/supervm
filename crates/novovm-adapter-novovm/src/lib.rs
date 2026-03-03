#![forbid(unsafe_code)]

use anyhow::{bail, Result};
use novovm_adapter_api::{
    AccountState, BlockIR, ChainAdapter, ChainConfig, ChainType, SerializationFormat, StateIR, TxIR,
    TxType,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug)]
pub struct NovoVmAdapter {
    config: ChainConfig,
    initialized: bool,
    state: StateIR,
    kv: HashMap<Vec<u8>, Vec<u8>>,
}

impl NovoVmAdapter {
    #[must_use]
    pub fn new(config: ChainConfig) -> Self {
        Self {
            config,
            initialized: false,
            state: StateIR::new(),
            kv: HashMap::new(),
        }
    }

    fn ensure_initialized(&self) -> Result<()> {
        if !self.initialized {
            bail!("adapter is not initialized");
        }
        Ok(())
    }

    fn default_account() -> AccountState {
        AccountState {
            balance: 0,
            nonce: 0,
            code_hash: None,
            storage_root: vec![0u8; 32],
        }
    }

    fn append_nonce_key(address: &[u8], nonce: u64) -> Vec<u8> {
        let mut key = Vec::with_capacity(address.len() + 8);
        key.extend_from_slice(address);
        key.extend_from_slice(&nonce.to_le_bytes());
        key
    }

    fn apply_transfer(tx: &TxIR, state: &mut StateIR, kv: &mut HashMap<Vec<u8>, Vec<u8>>) -> Result<()> {
        let to = tx
            .to
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("transfer tx missing target address"))?;

        let mut from_account = state
            .get_account(&tx.from)
            .cloned()
            .unwrap_or_else(Self::default_account);
        from_account.nonce = from_account.nonce.max(tx.nonce.saturating_add(1));
        state.set_account(tx.from.clone(), from_account);

        let mut to_account = state
            .get_account(to)
            .cloned()
            .unwrap_or_else(Self::default_account);
        to_account.balance = to_account.balance.saturating_add(tx.value);
        state.set_account(to.clone(), to_account);

        let slot = tx.nonce.to_le_bytes().to_vec();
        state.set_storage(tx.from.clone(), slot, tx.hash.clone());

        let state_key = Self::append_nonce_key(&tx.from, tx.nonce);
        kv.insert(state_key, tx.hash.clone());
        Ok(())
    }

    fn compute_state_root(state: &StateIR, kv: &HashMap<Vec<u8>, Vec<u8>>) -> Vec<u8> {
        let mut hasher = Sha256::new();

        let mut owners = state.accounts.keys().cloned().collect::<Vec<_>>();
        owners.sort();
        for owner in owners {
            if let Some(acc) = state.accounts.get(&owner) {
                hasher.update(&owner);
                hasher.update(acc.balance.to_le_bytes());
                hasher.update(acc.nonce.to_le_bytes());
            }
        }

        let mut storage_owners = state.storage.keys().cloned().collect::<Vec<_>>();
        storage_owners.sort();
        for owner in storage_owners {
            hasher.update(&owner);
            if let Some(slots) = state.storage.get(&owner) {
                let mut keys = slots.keys().cloned().collect::<Vec<_>>();
                keys.sort();
                for key in keys {
                    hasher.update(&key);
                    if let Some(value) = slots.get(&key) {
                        hasher.update(value);
                    }
                }
            }
        }

        let mut kv_keys = kv.keys().cloned().collect::<Vec<_>>();
        kv_keys.sort();
        for key in kv_keys {
            hasher.update(&key);
            if let Some(value) = kv.get(&key) {
                hasher.update(value);
            }
        }

        hasher.finalize().to_vec()
    }
}

impl ChainAdapter for NovoVmAdapter {
    fn chain_type(&self) -> ChainType {
        self.config.chain_type
    }

    fn config(&self) -> &ChainConfig {
        &self.config
    }

    fn initialize(&mut self) -> Result<()> {
        if !self.config.enabled {
            bail!("adapter config is disabled");
        }
        self.initialized = true;
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        self.ensure_initialized()?;
        self.initialized = false;
        Ok(())
    }

    fn parse_transaction(&self, raw_tx: &[u8]) -> Result<TxIR> {
        self.ensure_initialized()?;
        TxIR::deserialize(raw_tx, SerializationFormat::Bincode)
    }

    fn verify_transaction(&self, tx: &TxIR) -> Result<bool> {
        self.ensure_initialized()?;
        if tx.chain_id != self.config.chain_id {
            return Ok(false);
        }
        if tx.tx_type != TxType::Transfer {
            return Ok(false);
        }
        if tx.to.is_none() || tx.hash.is_empty() || tx.signature.is_empty() {
            return Ok(false);
        }
        Ok(true)
    }

    fn execute_transaction(&mut self, tx: &TxIR, state: &mut StateIR) -> Result<()> {
        self.ensure_initialized()?;
        if !self.verify_transaction(tx)? {
            bail!("transaction verification failed");
        }
        Self::apply_transfer(tx, state, &mut self.kv)?;
        Self::apply_transfer(tx, &mut self.state, &mut self.kv)?;
        let root = Self::compute_state_root(state, &self.kv);
        state.state_root = root.clone();
        self.state.state_root = root;
        Ok(())
    }

    fn estimate_gas(&self, tx: &TxIR) -> Result<u64> {
        self.ensure_initialized()?;
        Ok(tx.gas_limit.max(21_000))
    }

    fn parse_block(&self, raw_block: &[u8]) -> Result<BlockIR> {
        self.ensure_initialized()?;
        BlockIR::deserialize(raw_block, SerializationFormat::Bincode)
    }

    fn verify_block(&self, block: &BlockIR) -> Result<bool> {
        self.ensure_initialized()?;
        if block.number == 0 {
            return Ok(true);
        }
        Ok(block
            .transactions
            .iter()
            .all(|tx| tx.chain_id == self.config.chain_id && tx.tx_type == TxType::Transfer))
    }

    fn apply_block(&mut self, block: &BlockIR, state: &mut StateIR) -> Result<()> {
        self.ensure_initialized()?;
        if !self.verify_block(block)? {
            bail!("block verification failed");
        }
        for tx in &block.transactions {
            self.execute_transaction(tx, state)?;
        }
        Ok(())
    }

    fn read_state(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.ensure_initialized()?;
        Ok(self.kv.get(key).cloned())
    }

    fn write_state(&mut self, key: &[u8], value: Vec<u8>) -> Result<()> {
        self.ensure_initialized()?;
        self.kv.insert(key.to_vec(), value);
        Ok(())
    }

    fn delete_state(&mut self, key: &[u8]) -> Result<()> {
        self.ensure_initialized()?;
        self.kv.remove(key);
        Ok(())
    }

    fn state_root(&self) -> Result<Vec<u8>> {
        self.ensure_initialized()?;
        Ok(Self::compute_state_root(&self.state, &self.kv))
    }

    fn get_balance(&self, address: &[u8]) -> Result<u128> {
        self.ensure_initialized()?;
        Ok(self
            .state
            .get_account(address)
            .map_or(0, |acc| acc.balance))
    }

    fn get_nonce(&self, address: &[u8]) -> Result<u64> {
        self.ensure_initialized()?;
        Ok(self.state.get_account(address).map_or(0, |acc| acc.nonce))
    }
}

#[must_use]
pub fn supports_native_chain(chain: ChainType) -> bool {
    matches!(chain, ChainType::NovoVM | ChainType::Custom)
}

pub fn create_native_adapter(config: ChainConfig) -> Result<Box<dyn ChainAdapter>> {
    match config.chain_type {
        ChainType::NovoVM | ChainType::Custom => Ok(Box::new(NovoVmAdapter::new(config))),
        other => bail!(
            "native adapter backend only supports novovm/custom currently, got {}",
            other.as_str()
        ),
    }
}
