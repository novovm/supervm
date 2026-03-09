#![forbid(unsafe_code)]

use anyhow::{bail, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use novovm_adapter_api::{
    AccountRole, AccountState, BlockIR, ChainAdapter, ChainConfig, ChainType, PersonaAddress,
    PersonaType, ProtocolKind, RouteDecision, RouteRequest, SerializationFormat, StateIR, TxIR,
    TxType, UnifiedAccountError, UnifiedAccountRouter,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

const ADAPTER_UCA_ID_PREFIX: &str = "uca:adapter:";
const ADAPTER_UA_INGRESS_GUARD_ENV: &str = "NOVOVM_UNIFIED_ACCOUNT_ADAPTER_INGRESS_GUARD";
const ADAPTER_UA_AUTOPROVISION_ENV: &str = "NOVOVM_UNIFIED_ACCOUNT_ADAPTER_AUTOPROVISION";
const ADAPTER_UA_SIGNATURE_DOMAIN_ENV: &str = "NOVOVM_UNIFIED_ACCOUNT_ADAPTER_SIGNATURE_DOMAIN";
const ADAPTER_TX_SIG_DOMAIN: &[u8] = b"novovm_adapter_tx_sig_v1";

#[derive(Debug)]
pub struct NovoVmAdapter {
    config: ChainConfig,
    initialized: bool,
    state: StateIR,
    kv: HashMap<Vec<u8>, Vec<u8>>,
    unified_account_router: UnifiedAccountRouter,
}

impl NovoVmAdapter {
    #[must_use]
    pub fn new(config: ChainConfig) -> Self {
        Self {
            config,
            initialized: false,
            state: StateIR::new(),
            kv: HashMap::new(),
            unified_account_router: UnifiedAccountRouter::new(),
        }
    }

    #[must_use]
    pub fn unified_account_router(&self) -> &UnifiedAccountRouter {
        &self.unified_account_router
    }

    pub fn unified_account_router_mut(&mut self) -> &mut UnifiedAccountRouter {
        &mut self.unified_account_router
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

    fn derive_contract_address(from: &[u8], nonce: u64) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(b"novovm_contract_address_v1");
        hasher.update(from);
        hasher.update(nonce.to_le_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        digest[12..32].to_vec()
    }

    fn bool_env_default(name: &str, default: bool) -> bool {
        match std::env::var(name) {
            Ok(v) => {
                let v = v.trim();
                v == "1"
                    || v.eq_ignore_ascii_case("true")
                    || v.eq_ignore_ascii_case("on")
                    || v.eq_ignore_ascii_case("yes")
            }
            Err(_) => default,
        }
    }

    fn string_env_nonempty(name: &str) -> Option<String> {
        std::env::var(name).ok().and_then(|raw| {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }

    fn now_unix_sec() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn to_hex(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
        out
    }

    fn unified_account_guard_enabled(&self) -> bool {
        Self::bool_env_default(ADAPTER_UA_INGRESS_GUARD_ENV, true)
    }

    fn unified_account_autoprovision_enabled(&self) -> bool {
        Self::bool_env_default(ADAPTER_UA_AUTOPROVISION_ENV, true)
    }

    fn unified_account_signature_domain(&self, chain_id: u64) -> String {
        Self::string_env_nonempty(ADAPTER_UA_SIGNATURE_DOMAIN_ENV)
            .unwrap_or_else(|| format!("evm:{chain_id}"))
    }

    fn adapter_uca_id(&self, from: &[u8]) -> String {
        format!("{ADAPTER_UCA_ID_PREFIX}{}", Self::to_hex(from))
    }

    fn adapter_persona(&self, tx: &TxIR) -> PersonaAddress {
        PersonaAddress {
            persona_type: PersonaType::Evm,
            chain_id: tx.chain_id,
            external_address: tx.from.clone(),
        }
    }

    fn route_transaction_through_unified_account(&mut self, tx: &TxIR) -> Result<()> {
        if !self.unified_account_guard_enabled() {
            return Ok(());
        }

        let now = Self::now_unix_sec();
        let uca_id = self.adapter_uca_id(&tx.from);
        let persona = self.adapter_persona(tx);

        if self.unified_account_autoprovision_enabled() {
            match self
                .unified_account_router
                .create_uca(uca_id.clone(), tx.from.clone(), now)
            {
                Ok(()) | Err(UnifiedAccountError::UcaAlreadyExists { .. }) => {}
                Err(err) => {
                    bail!(
                        "unified account adapter ingress create failed (uca_id={}): {}",
                        uca_id,
                        err
                    );
                }
            }
            match self.unified_account_router.add_binding(
                &uca_id,
                AccountRole::Owner,
                persona.clone(),
                now,
            ) {
                Ok(()) | Err(UnifiedAccountError::BindingAlreadyExists) => {}
                Err(err) => {
                    bail!(
                        "unified account adapter ingress bind failed (uca_id={}, chain_id={}): {}",
                        uca_id,
                        persona.chain_id,
                        err
                    );
                }
            }
        }

        let route = self
            .unified_account_router
            .route(RouteRequest {
                uca_id: uca_id.clone(),
                persona: persona.clone(),
                role: AccountRole::Owner,
                protocol: ProtocolKind::Eth,
                signature_domain: self.unified_account_signature_domain(tx.chain_id),
                nonce: tx.nonce,
                wants_cross_chain_atomic: matches!(
                    tx.tx_type,
                    TxType::CrossChainTransfer | TxType::CrossChainCall
                ),
                tx_type4: false,
                session_expires_at: None,
                now,
            })
            .map_err(|err| {
                anyhow::anyhow!(
                    "unified account adapter ingress route rejected (uca_id={}, chain_id={}, nonce={}): {}",
                    uca_id,
                    tx.chain_id,
                    tx.nonce,
                    err
                )
            })?;

        if let RouteDecision::Adapter { chain_id } = route {
            if chain_id != tx.chain_id {
                bail!(
                    "unified account adapter ingress chain mismatch: routed={} tx_chain={}",
                    chain_id,
                    tx.chain_id
                );
            }
        }

        Ok(())
    }

    fn apply_transfer(
        tx: &TxIR,
        state: &mut StateIR,
        kv: &mut HashMap<Vec<u8>, Vec<u8>>,
    ) -> Result<()> {
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

    fn apply_contract_call(
        tx: &TxIR,
        state: &mut StateIR,
        kv: &mut HashMap<Vec<u8>, Vec<u8>>,
    ) -> Result<()> {
        let to = tx
            .to
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("contract call tx missing target address"))?;

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
        state.set_storage(to.clone(), slot.clone(), tx.hash.clone());
        kv.insert(Self::append_nonce_key(&tx.from, tx.nonce), tx.hash.clone());
        Ok(())
    }

    fn apply_contract_deploy(
        tx: &TxIR,
        state: &mut StateIR,
        kv: &mut HashMap<Vec<u8>, Vec<u8>>,
    ) -> Result<()> {
        if tx.to.is_some() {
            bail!("contract deploy tx must not set target address");
        }
        if tx.data.is_empty() {
            bail!("contract deploy tx missing init code");
        }

        let mut from_account = state
            .get_account(&tx.from)
            .cloned()
            .unwrap_or_else(Self::default_account);
        from_account.nonce = from_account.nonce.max(tx.nonce.saturating_add(1));
        state.set_account(tx.from.clone(), from_account);

        let contract_address = Self::derive_contract_address(&tx.from, tx.nonce);
        let mut contract_account = state
            .get_account(&contract_address)
            .cloned()
            .unwrap_or_else(Self::default_account);
        contract_account.balance = contract_account.balance.saturating_add(tx.value);
        let code_hash: [u8; 32] = Sha256::digest(&tx.data).into();
        contract_account.code_hash = Some(code_hash.to_vec());
        state.set_account(contract_address.clone(), contract_account);
        state.set_storage(
            contract_address.clone(),
            b"deploy:init_code_hash".to_vec(),
            code_hash.to_vec(),
        );
        kv.insert(Self::append_nonce_key(&tx.from, tx.nonce), contract_address);
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

fn tx_type_tag(tx_type: TxType) -> u8 {
    match tx_type {
        TxType::Transfer => 0,
        TxType::ContractCall => 1,
        TxType::ContractDeploy => 2,
        TxType::Privacy => 3,
        TxType::CrossShard => 4,
        TxType::CrossChainTransfer => 5,
        TxType::CrossChainCall => 6,
    }
}

fn compute_tx_ir_hash(tx: &TxIR) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(&tx.from);
    if let Some(to) = &tx.to {
        hasher.update(to);
    }
    hasher.update(tx.value.to_le_bytes());
    hasher.update(tx.nonce.to_le_bytes());
    hasher.update(&tx.data);
    hasher.finalize().to_vec()
}

pub fn tx_signing_message_v1(tx: &TxIR) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(ADAPTER_TX_SIG_DOMAIN);
    hasher.update(tx.chain_id.to_le_bytes());
    hasher.update([tx_type_tag(tx.tx_type)]);
    hasher.update(tx.nonce.to_le_bytes());
    hasher.update(tx.value.to_le_bytes());
    hasher.update(tx.gas_limit.to_le_bytes());
    hasher.update(tx.gas_price.to_le_bytes());
    hasher.update(&tx.from);
    if let Some(to) = &tx.to {
        hasher.update([1u8]);
        hasher.update(to);
    } else {
        hasher.update([0u8]);
    }
    hasher.update(&tx.data);
    hasher.update(&tx.hash);
    hasher.finalize().into()
}

pub fn address_from_pubkey_v1(pubkey: &VerifyingKey) -> Vec<u8> {
    let digest: [u8; 32] = Sha256::digest(pubkey.as_bytes()).into();
    digest[12..32].to_vec()
}

pub fn address_from_seed_v1(seed: [u8; 32]) -> Vec<u8> {
    let signing_key = SigningKey::from_bytes(&seed);
    address_from_pubkey_v1(&signing_key.verifying_key())
}

pub fn signature_payload_with_seed_v1(tx: &TxIR, seed: [u8; 32]) -> Vec<u8> {
    let signing_key = SigningKey::from_bytes(&seed);
    let verify_key = signing_key.verifying_key();
    let msg = tx_signing_message_v1(tx);
    let sig = signing_key.sign(&msg);
    let mut payload = Vec::with_capacity(32 + 64);
    payload.extend_from_slice(verify_key.as_bytes());
    payload.extend_from_slice(&sig.to_bytes());
    payload
}

fn verify_tx_signature_v1(tx: &TxIR) -> Result<bool> {
    if tx.signature.len() != 96 {
        return Ok(false);
    }
    let mut pubkey_bytes = [0u8; 32];
    pubkey_bytes.copy_from_slice(&tx.signature[..32]);
    let mut sig_bytes = [0u8; 64];
    sig_bytes.copy_from_slice(&tx.signature[32..96]);

    let verifying_key = match VerifyingKey::from_bytes(&pubkey_bytes) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    let signature = Signature::from_bytes(&sig_bytes);
    let msg = tx_signing_message_v1(tx);
    if verifying_key.verify(&msg, &signature).is_err() {
        return Ok(false);
    }

    let expected_from = address_from_pubkey_v1(&verifying_key);
    let from_matches = if tx.from.len() == 20 {
        tx.from == expected_from
    } else if tx.from.len() == 32 {
        tx.from == pubkey_bytes
    } else {
        false
    };
    Ok(from_matches)
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
        let tx_shape_ok = match tx.tx_type {
            TxType::Transfer | TxType::ContractCall => tx.to.is_some(),
            TxType::ContractDeploy => tx.to.is_none() && !tx.data.is_empty(),
            _ => false,
        };
        if !tx_shape_ok || tx.hash.is_empty() || tx.signature.is_empty() {
            return Ok(false);
        }
        if tx.hash != compute_tx_ir_hash(tx) {
            return Ok(false);
        }
        if !verify_tx_signature_v1(tx)? {
            return Ok(false);
        }
        Ok(true)
    }

    fn execute_transaction(&mut self, tx: &TxIR, state: &mut StateIR) -> Result<()> {
        self.ensure_initialized()?;
        if !self.verify_transaction(tx)? {
            bail!("transaction verification failed");
        }
        self.route_transaction_through_unified_account(tx)?;
        match tx.tx_type {
            TxType::Transfer => {
                Self::apply_transfer(tx, state, &mut self.kv)?;
                Self::apply_transfer(tx, &mut self.state, &mut self.kv)?;
            }
            TxType::ContractCall => {
                Self::apply_contract_call(tx, state, &mut self.kv)?;
                Self::apply_contract_call(tx, &mut self.state, &mut self.kv)?;
            }
            TxType::ContractDeploy => {
                Self::apply_contract_deploy(tx, state, &mut self.kv)?;
                Self::apply_contract_deploy(tx, &mut self.state, &mut self.kv)?;
            }
            _ => bail!("unsupported tx_type for native adapter: {:?}", tx.tx_type),
        }
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
        if block.number == 0 && block.transactions.is_empty() {
            return Ok(true);
        }
        for tx in &block.transactions {
            if !self.verify_transaction(tx)? {
                return Ok(false);
            }
        }
        Ok(true)
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
        Ok(self.state.get_account(address).map_or(0, |acc| acc.balance))
    }

    fn get_nonce(&self, address: &[u8]) -> Result<u64> {
        self.ensure_initialized()?;
        Ok(self.state.get_account(address).map_or(0, |acc| acc.nonce))
    }
}

#[must_use]
pub fn supports_native_chain(chain: ChainType) -> bool {
    matches!(
        chain,
        ChainType::NovoVM
            | ChainType::EVM
            | ChainType::BNB
            | ChainType::Polygon
            | ChainType::Avalanche
            | ChainType::Custom
    )
}

pub fn create_native_adapter(config: ChainConfig) -> Result<Box<dyn ChainAdapter>> {
    match config.chain_type {
        ChainType::NovoVM
        | ChainType::EVM
        | ChainType::BNB
        | ChainType::Polygon
        | ChainType::Avalanche
        | ChainType::Custom => Ok(Box::new(NovoVmAdapter::new(config))),
        other => bail!(
            "native adapter backend only supports novovm/evm/bnb/polygon/avalanche/custom currently, got {}",
            other.as_str()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_native_chain_includes_polygon_and_avalanche() {
        assert!(supports_native_chain(ChainType::NovoVM));
        assert!(supports_native_chain(ChainType::EVM));
        assert!(supports_native_chain(ChainType::BNB));
        assert!(supports_native_chain(ChainType::Polygon));
        assert!(supports_native_chain(ChainType::Avalanche));
        assert!(!supports_native_chain(ChainType::Solana));
    }

    fn sample_tx(tx_type: TxType) -> TxIR {
        let seed = [7u8; 32];
        let from = address_from_seed_v1(seed);
        let mut tx = TxIR {
            hash: Vec::new(),
            from,
            to: Some(vec![3u8; 20]),
            value: 1,
            gas_limit: 21_000,
            gas_price: 1,
            nonce: 0,
            data: Vec::new(),
            signature: Vec::new(),
            chain_id: 1,
            tx_type,
            source_chain: None,
            target_chain: None,
        };
        if tx_type == TxType::ContractDeploy {
            tx.to = None;
            tx.data = vec![0x60, 0x00, 0x60, 0x00];
        }
        if tx_type == TxType::ContractCall {
            tx.data = vec![0xaa, 0xbb];
        }
        tx.hash = compute_tx_ir_hash(&tx);
        tx.signature = signature_payload_with_seed_v1(&tx, seed);
        tx
    }

    #[test]
    fn verify_transaction_accepts_contract_call_and_deploy() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let call_tx = sample_tx(TxType::ContractCall);
        assert!(adapter.verify_transaction(&call_tx).expect("verify call"));

        let deploy_tx = sample_tx(TxType::ContractDeploy);
        assert!(adapter
            .verify_transaction(&deploy_tx)
            .expect("verify deploy"));
    }
}
