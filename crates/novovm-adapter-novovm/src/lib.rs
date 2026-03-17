#![forbid(unsafe_code)]

use anyhow::{anyhow, bail, Result};
use aoem_bindings::{
    ed25519_verify_batch_v1_auto, ed25519_verify_v1_auto, AoemEd25519VerifyItemRef,
};
use dashmap::DashSet;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use novovm_adapter_api::{
    AccountRole, AccountState, BlockIR, ChainAdapter, ChainConfig, ChainType, PersonaAddress,
    PersonaType, ProtocolKind, RouteDecision, RouteRequest, SerializationFormat, StateIR, TxIR,
    TxType, UnifiedAccountError, UnifiedAccountRouter,
};
use novovm_adapter_evm_core::{
    recover_raw_evm_tx_sender_m0, translate_raw_evm_tx_fields_m0, tx_ir_from_raw_fields_m0,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use web30_core::privacy::verify_ring_signature;
use web30_core::types::{
    Address as Web30Address, RingSignature as Web30RingSignature,
    StealthAddress as Web30StealthAddress,
};

const ADAPTER_UCA_ID_PREFIX: &str = "uca:adapter:";
const ADAPTER_UA_INGRESS_GUARD_ENV: &str = "NOVOVM_UNIFIED_ACCOUNT_ADAPTER_INGRESS_GUARD";
const ADAPTER_UA_AUTOPROVISION_ENV: &str = "NOVOVM_UNIFIED_ACCOUNT_ADAPTER_AUTOPROVISION";
const ADAPTER_UA_SIGNATURE_DOMAIN_ENV: &str = "NOVOVM_UNIFIED_ACCOUNT_ADAPTER_SIGNATURE_DOMAIN";
const ADAPTER_TX_SIG_DOMAIN: &[u8] = b"novovm_adapter_tx_sig_v1";
const TX_SIG_VERIFY_PARALLEL_MIN_BATCH: usize = 128;
const TX_SIG_VERIFY_PARALLEL_MIN_CHUNK: usize = 64;

#[derive(Debug)]
pub struct NovoVmAdapter {
    config: ChainConfig,
    initialized: bool,
    state: StateIR,
    kv: HashMap<Vec<u8>, Vec<u8>>,
    state_root_cache: Mutex<StateRootCache>,
    verified_tx_cache: DashSet<Vec<u8>>,
    unified_account_router: UnifiedAccountRouter,
}

#[derive(Debug)]
struct StateRootCache {
    root: Vec<u8>,
    dirty: bool,
}

impl NovoVmAdapter {
    #[must_use]
    pub fn new(config: ChainConfig) -> Self {
        let state = StateIR::new();
        let kv = HashMap::new();
        let state_root = Self::compute_state_root(&state, &kv);
        Self {
            config,
            initialized: false,
            state,
            kv,
            state_root_cache: Mutex::new(StateRootCache {
                root: state_root,
                dirty: false,
            }),
            verified_tx_cache: DashSet::new(),
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

    fn tx_shape_ok(tx: &TxIR) -> bool {
        match tx.tx_type {
            TxType::Transfer | TxType::ContractCall => tx.to.is_some(),
            TxType::ContractDeploy => tx.to.is_none() && !tx.data.is_empty(),
            TxType::Privacy => tx.to.is_none() && !tx.data.is_empty(),
            _ => false,
        }
    }

    fn tx_from_matches_pubkey_bytes(tx: &TxIR, pubkey_bytes: &[u8; 32]) -> bool {
        let expected_from = address_from_pubkey_bytes_v1(pubkey_bytes);
        if tx.from.len() == 20 {
            tx.from == expected_from
        } else if tx.from.len() == 32 {
            tx.from == *pubkey_bytes
        } else {
            false
        }
    }

    fn supports_evm_raw_signature_path(&self) -> bool {
        matches!(
            self.config.chain_type,
            ChainType::EVM
                | ChainType::BNB
                | ChainType::Polygon
                | ChainType::Avalanche
                | ChainType::Custom
        )
    }

    pub fn verify_transactions_batch(&self, txs: &[TxIR]) -> Result<Vec<bool>> {
        self.ensure_initialized()?;
        if txs.is_empty() {
            return Ok(Vec::new());
        }

        let mut verify_results = vec![false; txs.len()];
        let mut batch_indices = Vec::new();
        let mut batch_pubkeys = Vec::new();
        let mut batch_signatures = Vec::new();
        let mut batch_messages = Vec::new();

        for (idx, tx) in txs.iter().enumerate() {
            if tx.chain_id != self.config.chain_id {
                continue;
            }
            if !Self::tx_shape_ok(tx) || tx.hash.is_empty() || tx.signature.is_empty() {
                continue;
            }
            if tx.hash != compute_tx_ir_hash(tx) {
                continue;
            }

            match tx.tx_type {
                TxType::Privacy => {
                    if Self::decode_privacy_stealth_address(tx).is_err() {
                        continue;
                    }
                    let signature = match Self::decode_privacy_signature(tx) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let key_image_key = Self::privacy_key_image_key(&signature.key_image);
                    if self.kv.contains_key(&key_image_key) {
                        continue;
                    }
                    verify_results[idx] = Self::verify_privacy_tx_signature_v1(tx)?;
                }
                _ => {
                    // For native NOVOVM signatures, keep the fast ed25519 batch path.
                    // For EVM-family chains, fallback to raw-EVM signature recovery path,
                    // which mirrors geth's "decode+recover sender" basic validation stage.
                    if tx.signature.len() != 96 {
                        if self.supports_evm_raw_signature_path() {
                            verify_results[idx] = verify_evm_raw_tx_signature_v1(tx)?;
                        }
                        continue;
                    }
                    let mut pubkey_bytes = [0u8; 32];
                    pubkey_bytes.copy_from_slice(&tx.signature[..32]);
                    let mut sig_bytes = [0u8; 64];
                    sig_bytes.copy_from_slice(&tx.signature[32..96]);
                    batch_indices.push(idx);
                    batch_pubkeys.push(pubkey_bytes);
                    batch_signatures.push(sig_bytes);
                    batch_messages.push(tx_signing_message_v1(tx));
                }
            }
        }

        if !batch_indices.is_empty() {
            let mut batch_items = Vec::with_capacity(batch_indices.len());
            for slot in 0..batch_indices.len() {
                batch_items.push(AoemEd25519VerifyItemRef {
                    pubkey: &batch_pubkeys[slot],
                    message: &batch_messages[slot],
                    signature: &batch_signatures[slot],
                });
            }

            if let Some(batch_crypto_results) = ed25519_verify_batch_v1_auto(&batch_items)? {
                for (slot, verified) in batch_crypto_results.into_iter().enumerate() {
                    if !verified {
                        continue;
                    }
                    let idx = batch_indices[slot];
                    verify_results[idx] =
                        Self::tx_from_matches_pubkey_bytes(&txs[idx], &batch_pubkeys[slot]);
                }
            } else {
                let total = batch_indices.len();
                let worker_count = std::thread::available_parallelism()
                    .map(|v| v.get())
                    .unwrap_or(1)
                    .min((total / TX_SIG_VERIFY_PARALLEL_MIN_CHUNK).max(1));
                if total >= TX_SIG_VERIFY_PARALLEL_MIN_BATCH && worker_count > 1 {
                    let chunk_size = total
                        .div_ceil(worker_count)
                        .max(TX_SIG_VERIFY_PARALLEL_MIN_CHUNK);
                    let batch_indices_ref = &batch_indices;
                    let txs_ref = txs;
                    let fallback_results = std::thread::scope(|scope| -> Result<Vec<bool>> {
                        let mut jobs = Vec::with_capacity(total.div_ceil(chunk_size));
                        for start in (0..batch_indices_ref.len()).step_by(chunk_size) {
                            let end = (start + chunk_size).min(batch_indices_ref.len());
                            jobs.push(scope.spawn(move || -> Result<(usize, Vec<bool>)> {
                                let mut chunk_results = Vec::with_capacity(end - start);
                                for idx in &batch_indices_ref[start..end] {
                                    let verified = verify_tx_signature_v1(&txs_ref[*idx])?;
                                    chunk_results.push(verified);
                                }
                                Ok((start, chunk_results))
                            }));
                        }

                        let mut merged = vec![false; batch_indices_ref.len()];
                        for job in jobs {
                            let (start, chunk_results) = job.join().map_err(|_| {
                                anyhow!("parallel verify_transaction thread panicked")
                            })??;
                            let end = start + chunk_results.len();
                            merged[start..end].copy_from_slice(&chunk_results);
                        }
                        Ok(merged)
                    })?;
                    for (slot, verified) in fallback_results.into_iter().enumerate() {
                        if verified {
                            let idx = batch_indices[slot];
                            verify_results[idx] = true;
                        }
                    }
                } else {
                    for idx in batch_indices {
                        verify_results[idx] = verify_tx_signature_v1(&txs[idx])?;
                    }
                }
            }
        }

        for (idx, verified) in verify_results.iter().copied().enumerate() {
            if verified {
                self.verified_tx_cache.insert(txs[idx].hash.clone());
            }
        }
        Ok(verify_results)
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

    fn privacy_key_image_key(key_image: &[u8; 32]) -> Vec<u8> {
        let mut key = Vec::with_capacity(18 + key_image.len());
        key.extend_from_slice(b"privacy:key_image:");
        key.extend_from_slice(key_image);
        key
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

    fn decode_privacy_signature(tx: &TxIR) -> Result<Web30RingSignature> {
        serde_json::from_slice(&tx.signature)
            .map_err(|e| anyhow::anyhow!("privacy tx ring signature decode failed: {e}"))
    }

    fn decode_privacy_stealth_address(tx: &TxIR) -> Result<Web30StealthAddress> {
        bincode::deserialize(&tx.data)
            .map_err(|e| anyhow::anyhow!("privacy tx stealth address decode failed: {e}"))
    }

    fn privacy_member_matches_tx_from(member: &Web30Address, from: &[u8]) -> bool {
        if from.len() == 32 {
            member.as_bytes().as_slice() == from
        } else if from.len() == 20 {
            &member.as_bytes()[12..32] == from
        } else {
            false
        }
    }

    fn verify_privacy_tx_signature_v1(tx: &TxIR) -> Result<bool> {
        let signature = match Self::decode_privacy_signature(tx) {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };
        if !signature
            .ring_members
            .iter()
            .any(|member| Self::privacy_member_matches_tx_from(member, &tx.from))
        {
            return Ok(false);
        }
        let message = tx_signing_message_v1(tx);
        verify_ring_signature(&signature, &message, tx.value)
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

    fn apply_privacy_transfer(
        tx: &TxIR,
        state: &mut StateIR,
        kv: &mut HashMap<Vec<u8>, Vec<u8>>,
        record_key_image: bool,
    ) -> Result<()> {
        let stealth_address = Self::decode_privacy_stealth_address(tx)?;
        let ring_signature = Self::decode_privacy_signature(tx)?;
        let key_image_key = Self::privacy_key_image_key(&ring_signature.key_image);
        if record_key_image && kv.contains_key(&key_image_key) {
            bail!("privacy tx key image already spent");
        }

        let mut from_account = state
            .get_account(&tx.from)
            .cloned()
            .unwrap_or_else(Self::default_account);
        if from_account.balance < tx.value {
            bail!("privacy tx insufficient balance");
        }
        from_account.balance -= tx.value;
        from_account.nonce = from_account.nonce.max(tx.nonce.saturating_add(1));
        state.set_account(tx.from.clone(), from_account);

        let recipient = stealth_address.spend_key.to_vec();
        let mut to_account = state
            .get_account(&recipient)
            .cloned()
            .unwrap_or_else(Self::default_account);
        to_account.balance = to_account.balance.saturating_add(tx.value);
        state.set_account(recipient.clone(), to_account);

        state.set_storage(
            recipient.clone(),
            b"privacy:view_key".to_vec(),
            stealth_address.view_key.to_vec(),
        );
        state.set_storage(
            recipient.clone(),
            b"privacy:key_image".to_vec(),
            ring_signature.key_image.to_vec(),
        );
        state.set_storage(
            tx.from.clone(),
            tx.nonce.to_le_bytes().to_vec(),
            tx.hash.clone(),
        );

        if record_key_image {
            kv.insert(key_image_key, tx.hash.clone());
        }
        kv.insert(Self::append_nonce_key(&tx.from, tx.nonce), tx.hash.clone());
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

    fn mark_state_root_dirty_and_snapshot_last(&self) -> Vec<u8> {
        if let Ok(mut cache) = self.state_root_cache.lock() {
            cache.dirty = true;
            return cache.root.clone();
        }
        vec![0u8; 32]
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

fn address_from_pubkey_bytes_v1(pubkey_bytes: &[u8; 32]) -> Vec<u8> {
    let digest: [u8; 32] = Sha256::digest(pubkey_bytes).into();
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

pub fn privacy_stealth_payload_v1(stealth_address: &Web30StealthAddress) -> Result<Vec<u8>> {
    bincode::serialize(stealth_address)
        .map_err(|e| anyhow::anyhow!("encode privacy stealth address failed: {e}"))
}

pub fn privacy_signature_payload_with_secret_v1(
    tx: &TxIR,
    ring_members: &[Web30Address],
    signer_index: usize,
    private_key: [u8; 32],
) -> Result<Vec<u8>> {
    let message = tx_signing_message_v1(tx);
    let signature = web30_core::privacy::generate_ring_signature_with_amount(
        &private_key,
        ring_members,
        &message,
        tx.value,
        signer_index,
    )?;
    serde_json::to_vec(&signature)
        .map_err(|e| anyhow::anyhow!("encode privacy ring signature payload failed: {e}"))
}

#[derive(Debug, Clone)]
pub struct PrivacyTxEnvelopeV1 {
    pub from: Vec<u8>,
    pub stealth_address: Web30StealthAddress,
    pub value: u128,
    pub nonce: u64,
    pub chain_id: u64,
    pub gas_limit: u64,
    pub gas_price: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct PrivacyTxSignerV1<'a> {
    pub ring_members: &'a [Web30Address],
    pub signer_index: usize,
    pub private_key: [u8; 32],
}

#[derive(Debug, Clone)]
pub struct PrivacyTxRawEnvelopeV1 {
    pub from: Vec<u8>,
    pub stealth_view_key: [u8; 32],
    pub stealth_spend_key: [u8; 32],
    pub value: u128,
    pub nonce: u64,
    pub chain_id: u64,
    pub gas_limit: u64,
    pub gas_price: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct PrivacyTxRawSignerV1<'a> {
    pub ring_members: &'a [[u8; 32]],
    pub signer_index: usize,
    pub private_key: [u8; 32],
}

pub fn build_privacy_tx_ir_unsigned_v1(envelope: &PrivacyTxEnvelopeV1) -> Result<TxIR> {
    let data = privacy_stealth_payload_v1(&envelope.stealth_address)?;
    let mut tx = TxIR {
        hash: Vec::new(),
        from: envelope.from.clone(),
        to: None,
        value: envelope.value,
        gas_limit: envelope.gas_limit,
        gas_price: envelope.gas_price,
        nonce: envelope.nonce,
        data,
        signature: Vec::new(),
        chain_id: envelope.chain_id,
        tx_type: TxType::Privacy,
        source_chain: None,
        target_chain: None,
    };
    tx.hash = compute_tx_ir_hash(&tx);
    Ok(tx)
}

pub fn build_privacy_tx_ir_signed_v1(
    envelope: &PrivacyTxEnvelopeV1,
    signer: PrivacyTxSignerV1<'_>,
) -> Result<TxIR> {
    let mut tx = build_privacy_tx_ir_unsigned_v1(envelope)?;
    tx.signature = privacy_signature_payload_with_secret_v1(
        &tx,
        signer.ring_members,
        signer.signer_index,
        signer.private_key,
    )?;
    Ok(tx)
}

pub fn build_privacy_tx_ir_signed_from_raw_v1(
    envelope: &PrivacyTxRawEnvelopeV1,
    signer: PrivacyTxRawSignerV1<'_>,
) -> Result<TxIR> {
    let envelope = PrivacyTxEnvelopeV1 {
        from: envelope.from.clone(),
        stealth_address: Web30StealthAddress {
            view_key: envelope.stealth_view_key,
            spend_key: envelope.stealth_spend_key,
        },
        value: envelope.value,
        nonce: envelope.nonce,
        chain_id: envelope.chain_id,
        gas_limit: envelope.gas_limit,
        gas_price: envelope.gas_price,
    };
    let ring_member_addrs: Vec<Web30Address> = signer
        .ring_members
        .iter()
        .copied()
        .map(Web30Address::from_bytes)
        .collect();
    build_privacy_tx_ir_signed_v1(
        &envelope,
        PrivacyTxSignerV1 {
            ring_members: &ring_member_addrs,
            signer_index: signer.signer_index,
            private_key: signer.private_key,
        },
    )
}

fn verify_tx_signature_v1(tx: &TxIR) -> Result<bool> {
    if tx.signature.len() != 96 {
        return Ok(false);
    }
    let mut pubkey_bytes = [0u8; 32];
    pubkey_bytes.copy_from_slice(&tx.signature[..32]);
    let mut sig_bytes = [0u8; 64];
    sig_bytes.copy_from_slice(&tx.signature[32..96]);

    let msg = tx_signing_message_v1(tx);
    let signature_ok = if let Some(valid) = ed25519_verify_v1_auto(&pubkey_bytes, &msg, &sig_bytes)?
    {
        valid
    } else {
        let verifying_key = match VerifyingKey::from_bytes(&pubkey_bytes) {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };
        let signature = Signature::from_bytes(&sig_bytes);
        verifying_key.verify(&msg, &signature).is_ok()
    };
    if !signature_ok {
        return Ok(false);
    }

    let expected_from = address_from_pubkey_bytes_v1(&pubkey_bytes);
    let from_matches = if tx.from.len() == 20 {
        tx.from == expected_from
    } else if tx.from.len() == 32 {
        tx.from == pubkey_bytes
    } else {
        false
    };
    Ok(from_matches)
}

fn verify_evm_raw_tx_signature_v1(tx: &TxIR) -> Result<bool> {
    let Some(recovered_from) = recover_raw_evm_tx_sender_m0(&tx.signature)? else {
        return Ok(false);
    };
    let parsed_fields = match translate_raw_evm_tx_fields_m0(&tx.signature) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    let canonical =
        tx_ir_from_raw_fields_m0(&parsed_fields, &tx.signature, recovered_from, tx.chain_id);
    Ok(tx.hash == canonical.hash)
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
        self.verified_tx_cache.clear();
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
        if !Self::tx_shape_ok(tx) || tx.hash.is_empty() || tx.signature.is_empty() {
            return Ok(false);
        }
        if tx.hash != compute_tx_ir_hash(tx) {
            return Ok(false);
        }
        let signature_ok = match tx.tx_type {
            TxType::Privacy => {
                if Self::decode_privacy_stealth_address(tx).is_err() {
                    return Ok(false);
                }
                let signature = match Self::decode_privacy_signature(tx) {
                    Ok(v) => v,
                    Err(_) => return Ok(false),
                };
                let key_image_key = Self::privacy_key_image_key(&signature.key_image);
                if self.kv.contains_key(&key_image_key) {
                    return Ok(false);
                }
                Self::verify_privacy_tx_signature_v1(tx)?
            }
            _ => {
                if tx.signature.len() == 96 {
                    verify_tx_signature_v1(tx)?
                } else if self.supports_evm_raw_signature_path() {
                    verify_evm_raw_tx_signature_v1(tx)?
                } else {
                    false
                }
            }
        };
        if !signature_ok {
            return Ok(false);
        }
        self.verified_tx_cache.insert(tx.hash.clone());
        Ok(true)
    }

    fn execute_transaction(&mut self, tx: &TxIR, state: &mut StateIR) -> Result<()> {
        self.ensure_initialized()?;
        let already_verified = if tx.hash.is_empty() {
            false
        } else {
            self.verified_tx_cache.remove(&tx.hash).is_some()
        };
        if !already_verified && !self.verify_transaction(tx)? {
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
            TxType::Privacy => {
                Self::apply_privacy_transfer(tx, state, &mut self.kv, false)?;
                Self::apply_privacy_transfer(tx, &mut self.state, &mut self.kv, true)?;
            }
            _ => bail!("unsupported tx_type for native adapter: {:?}", tx.tx_type),
        }
        let last_root = self.mark_state_root_dirty_and_snapshot_last();
        state.state_root = last_root.clone();
        self.state.state_root = last_root;
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
        Ok(self
            .verify_transactions_batch(&block.transactions)?
            .into_iter()
            .all(|v| v))
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
        let _ = self.mark_state_root_dirty_and_snapshot_last();
        Ok(())
    }

    fn delete_state(&mut self, key: &[u8]) -> Result<()> {
        self.ensure_initialized()?;
        self.kv.remove(key);
        let _ = self.mark_state_root_dirty_and_snapshot_last();
        Ok(())
    }

    fn state_root(&self) -> Result<Vec<u8>> {
        self.ensure_initialized()?;
        {
            let cache = self
                .state_root_cache
                .lock()
                .map_err(|_| anyhow!("state_root cache mutex poisoned"))?;
            if !cache.dirty {
                return Ok(cache.root.clone());
            }
        }

        let root = Self::compute_state_root(&self.state, &self.kv);
        let mut cache = self
            .state_root_cache
            .lock()
            .map_err(|_| anyhow!("state_root cache mutex poisoned"))?;
        cache.root = root.clone();
        cache.dirty = false;
        Ok(root)
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
    use novovm_adapter_evm_core::{
        recover_raw_evm_tx_sender_m0, translate_raw_evm_tx_fields_m0, tx_ir_from_raw_fields_m0,
    };
    use web30_core::privacy::generate_ring_keypair;

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

    fn sample_privacy_tx_with_invalid_signature() -> TxIR {
        let envelope = PrivacyTxEnvelopeV1 {
            from: vec![7u8; 32],
            stealth_address: Web30StealthAddress {
                view_key: [9u8; 32],
                spend_key: [8u8; 32],
            },
            value: 3,
            nonce: 0,
            chain_id: 1,
            gas_limit: 21_000,
            gas_price: 1,
        };
        let mut tx = build_privacy_tx_ir_unsigned_v1(&envelope).expect("build unsigned privacy tx");
        tx.signature = br#"{"bad":"ring"}"#.to_vec();
        tx
    }

    fn decode_hex_bytes(hex: &str) -> Vec<u8> {
        let normalized = hex.trim_start_matches("0x");
        assert_eq!(normalized.len() % 2, 0, "hex length must be even");
        let mut out = Vec::with_capacity(normalized.len() / 2);
        let bytes = normalized.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            let hi = (bytes[i] as char).to_digit(16).expect("hex hi digit");
            let lo = (bytes[i + 1] as char).to_digit(16).expect("hex lo digit");
            out.push(((hi << 4) | lo) as u8);
            i += 2;
        }
        out
    }

    #[test]
    fn build_privacy_tx_unsigned_sets_expected_shape() {
        let envelope = PrivacyTxEnvelopeV1 {
            from: vec![0x11u8; 32],
            stealth_address: Web30StealthAddress {
                view_key: [0x31u8; 32],
                spend_key: [0x52u8; 32],
            },
            value: 9,
            nonce: 2,
            chain_id: 20260303,
            gas_limit: 90_000,
            gas_price: 3,
        };
        let tx = build_privacy_tx_ir_unsigned_v1(&envelope).expect("build unsigned privacy tx");
        assert_eq!(tx.tx_type, TxType::Privacy);
        assert!(tx.to.is_none());
        assert_eq!(tx.value, 9);
        assert_eq!(tx.nonce, 2);
        assert_eq!(tx.chain_id, 20260303);
        assert_eq!(tx.gas_limit, 90_000);
        assert_eq!(tx.gas_price, 3);
        assert!(!tx.hash.is_empty());
        let decoded: Web30StealthAddress =
            bincode::deserialize(&tx.data).expect("decode stealth payload");
        assert_eq!(decoded.view_key, envelope.stealth_address.view_key);
        assert_eq!(decoded.spend_key, envelope.stealth_address.spend_key);
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

    #[test]
    fn verify_transaction_accepts_raw_evm_signature_payload_when_sender_matches() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        // Sample EIP-155 legacy signed tx payload (same fixture style as gateway tests).
        let raw_tx = decode_hex_bytes("0xf86c018502540be40082520894aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa88016345785d8a00008026a0b7e8e4a6c7d58a47f6d29b6cb16f1c7f5c8a7f7ec5b9fa7a1d8c19f6d8f2b87a02a6d2f8c8f42d8f6d8909f94b6f6a6a4d9f7f1c7b6a5d4e3f2c1b0a99887766");
        let Some(sender) = recover_raw_evm_tx_sender_m0(&raw_tx)
            .expect("raw sender recovery path should not error")
        else {
            return;
        };
        let fields = translate_raw_evm_tx_fields_m0(&raw_tx).expect("decode raw tx fields");
        let tx = tx_ir_from_raw_fields_m0(&fields, &raw_tx, sender, 1);
        assert!(
            adapter.verify_transaction(&tx).expect("verify raw tx"),
            "raw evm signature payload should verify via sender recovery"
        );
    }

    #[test]
    fn verify_transaction_rejects_malformed_privacy_signature() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::NovoVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let tx = sample_privacy_tx_with_invalid_signature();
        assert!(!adapter.verify_transaction(&tx).expect("verify privacy"));
    }

    #[test]
    fn execute_transaction_rejects_replayed_privacy_key_image_when_aoem_available() {
        let Some(_) = NovoVmAdapter::string_env_nonempty("NOVOVM_AOEM_DLL")
            .or_else(|| NovoVmAdapter::string_env_nonempty("AOEM_DLL"))
            .or_else(|| NovoVmAdapter::string_env_nonempty("AOEM_FFI_DLL"))
        else {
            return;
        };

        let (decoy_pub, _decoy_secret) = match generate_ring_keypair() {
            Ok(v) => v,
            Err(_) => return,
        };
        let (real_pub, real_secret) = match generate_ring_keypair() {
            Ok(v) => v,
            Err(_) => return,
        };

        let ring_members = vec![
            Web30Address::from_bytes(decoy_pub),
            Web30Address::from_bytes(real_pub),
        ];
        let envelope = PrivacyTxEnvelopeV1 {
            from: real_pub.to_vec(),
            stealth_address: Web30StealthAddress {
                view_key: [5u8; 32],
                spend_key: [6u8; 32],
            },
            value: 7,
            nonce: 0,
            chain_id: 1,
            gas_limit: 21_000,
            gas_price: 1,
        };
        let tx = build_privacy_tx_ir_signed_v1(
            &envelope,
            PrivacyTxSignerV1 {
                ring_members: &ring_members,
                signer_index: 1,
                private_key: real_secret,
            },
        )
        .expect("privacy sign");

        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::NovoVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let funded = AccountState {
            balance: 100,
            nonce: 0,
            code_hash: None,
            storage_root: vec![0u8; 32],
        };
        adapter.state.set_account(tx.from.clone(), funded.clone());
        let mut runtime_state = StateIR::new();
        runtime_state.set_account(tx.from.clone(), funded);

        adapter
            .execute_transaction(&tx, &mut runtime_state)
            .expect("first privacy tx must pass");
        assert!(
            adapter
                .execute_transaction(&tx, &mut runtime_state)
                .is_err(),
            "replayed privacy tx must fail due to spent key image"
        );
    }

    #[test]
    fn build_privacy_tx_from_raw_signs_and_marks_privacy_type() {
        let Some(_) = NovoVmAdapter::string_env_nonempty("NOVOVM_AOEM_DLL")
            .or_else(|| NovoVmAdapter::string_env_nonempty("AOEM_DLL"))
            .or_else(|| NovoVmAdapter::string_env_nonempty("AOEM_FFI_DLL"))
        else {
            return;
        };
        let (decoy_pub, _decoy_secret) = match generate_ring_keypair() {
            Ok(v) => v,
            Err(_) => return,
        };
        let (real_pub, real_secret) = match generate_ring_keypair() {
            Ok(v) => v,
            Err(_) => return,
        };
        let ring_members = [decoy_pub, real_pub];
        let tx = build_privacy_tx_ir_signed_from_raw_v1(
            &PrivacyTxRawEnvelopeV1 {
                from: real_pub.to_vec(),
                stealth_view_key: [0x11u8; 32],
                stealth_spend_key: [0x22u8; 32],
                value: 9,
                nonce: 3,
                chain_id: 20260303,
                gas_limit: 90_000,
                gas_price: 7,
            },
            PrivacyTxRawSignerV1 {
                ring_members: &ring_members,
                signer_index: 1,
                private_key: real_secret,
            },
        )
        .expect("build privacy tx from raw");
        assert_eq!(tx.tx_type, TxType::Privacy);
        assert!(tx.to.is_none());
        assert!(!tx.signature.is_empty());
    }
}
