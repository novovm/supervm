#![forbid(unsafe_code)]

mod bincode_compat;

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
    derive_create_contract_address_m0, estimate_intrinsic_gas_with_envelope_extras_for_chain_m0,
    recover_raw_evm_tx_sender_m0, resolve_evm_profile, translate_raw_evm_tx_fields_m0,
    tx_ir_from_raw_fields_m0, validate_tx_semantics_m0,
};
use novovm_exec::{
    aoem_failure_recoverability_from_class_v1, classify_failure_from_anchor_v1,
    is_eip7610_rejected_account_v1, reconstruct_tx_execution_artifact_v1, AoemCanonicalTxTypeV1,
    AoemEventLogV1, AoemExecutionReconstructionInputV1, AoemExecutionReconstructionSourcesV1,
    AoemFailureClassSourceV1, AoemFailureClassV1, AoemFailureRecoverabilityV1, AoemFieldSourceV1,
    AoemReceiptDerivationRulesV1, AoemTxExecutionArtifactV1,
    AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1, EIP7610_REJECTION_CONTRACT_V1,
};
use novovm_protocol::{EvmBlockAccessListV1, EvmConstructionBlockAccessListV1};
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
const ADAPTER_UA_SUBJECT_ACCOUNT_KEY_PREFIX: &[u8] = b"ua:subject:v1:";
const ADAPTER_UA_SUBJECT_NONCE_KEY_PREFIX: &[u8] = b"ua:nonce:v1:";
const TX_SIG_VERIFY_PARALLEL_MIN_BATCH: usize = 128;
const TX_SIG_VERIFY_PARALLEL_MIN_CHUNK: usize = 64;
const AOEM_TX_ARTIFACT_KEY_PREFIX_V1: &[u8] = b"aoem:tx_artifact:v1:";
const AOEM_TX_STATE_ROOT_KEY_PREFIX_V1: &[u8] = b"aoem:tx_state_root:v1:";
const AOEM_TX_ANCHOR_KEY_PREFIX_V1: &[u8] = b"aoem:tx_anchor:v1:";
const ADAPTER_EVM_RUNTIME_CODE_STORAGE_KEY: &[u8] = b"evm:runtime_code_v1";
const ADAPTER_RUNTIME_LOG_REBUILD_ENV: &str = "NOVOVM_ADAPTER_RUNTIME_LOG_REBUILD";
const ADAPTER_RUNTIME_LOG_REBUILD_REQUIRED_ENV: &str =
    "NOVOVM_ADAPTER_RUNTIME_LOG_REBUILD_REQUIRED";

#[derive(Debug)]
pub struct NovoVmAdapter {
    config: ChainConfig,
    initialized: bool,
    state: StateIR,
    kv: HashMap<Vec<u8>, Vec<u8>>,
    state_root_cache: Mutex<StateRootCache>,
    verified_tx_cache: DashSet<Vec<u8>>,
    unified_account_router: UnifiedAccountRouter,
    execution_current_tx_hash: Vec<u8>,
    execution_current_tx_index: u32,
    execution_current_log_index: u32,
    execution_current_logs: Vec<AoemEventLogV1>,
    execution_tx_ordinal: u32,
}

#[derive(Debug)]
struct StateRootCache {
    root: Vec<u8>,
    dirty: bool,
}

#[derive(Clone, Copy)]
struct ExecutionReconstructionContextV1<'a> {
    status_ok: bool,
    final_state_root: [u8; 32],
    tx_index: u32,
    raw_execution_logs: &'a [AoemEventLogV1],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NovoVmObservedBlockAccessListV1 {
    pub block_access_list: Option<EvmBlockAccessListV1>,
    pub block_access_list_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NovoVmTxExecutionOutcomeV1 {
    pub artifact: AoemTxExecutionArtifactV1,
    pub observed_block_access_list: NovoVmObservedBlockAccessListV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AdapterExecutionSubjectV1 {
    account_id: String,
    fee_owner_account_id: String,
    nonce_owner_account_id: String,
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
            execution_current_tx_hash: Vec::new(),
            execution_current_tx_index: 0,
            execution_current_log_index: 0,
            execution_current_logs: Vec::new(),
            execution_tx_ordinal: 0,
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
                            verify_results[idx] =
                                verify_evm_raw_tx_signature_v1(self.config.chain_type, tx)?;
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

    fn normalize_account_hint_v1(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    fn append_subject_nonce_key(account_id: &str, nonce: u64) -> Vec<u8> {
        let mut key =
            Vec::with_capacity(ADAPTER_UA_SUBJECT_NONCE_KEY_PREFIX.len() + account_id.len() + 8);
        key.extend_from_slice(ADAPTER_UA_SUBJECT_NONCE_KEY_PREFIX);
        key.extend_from_slice(account_id.as_bytes());
        key.extend_from_slice(&nonce.to_le_bytes());
        key
    }

    fn subject_state_key(account_id: &str) -> Vec<u8> {
        let mut key =
            Vec::with_capacity(ADAPTER_UA_SUBJECT_ACCOUNT_KEY_PREFIX.len() + account_id.len());
        key.extend_from_slice(ADAPTER_UA_SUBJECT_ACCOUNT_KEY_PREFIX);
        key.extend_from_slice(account_id.as_bytes());
        key
    }

    fn fallback_adapter_account_id(from: &[u8]) -> String {
        format!("{ADAPTER_UCA_ID_PREFIX}{}", Self::to_hex(from))
    }

    fn subject_account_id_from_address_v1(&self, address: &[u8]) -> String {
        let persona = PersonaAddress {
            persona_type: PersonaType::Evm,
            chain_id: self.config.chain_id,
            external_address: address.to_vec(),
        };
        self.unified_account_router
            .resolve_binding_owner(&persona)
            .map(str::to_owned)
            .unwrap_or_else(|| Self::fallback_adapter_account_id(address))
    }

    fn subject_from_tx_v1(tx: &TxIR) -> AdapterExecutionSubjectV1 {
        let account_id = tx
            .account_id
            .as_deref()
            .and_then(Self::normalize_account_hint_v1)
            .unwrap_or_else(|| Self::fallback_adapter_account_id(&tx.from));
        let fee_owner_account_id = tx
            .fee_owner_account_id
            .as_deref()
            .and_then(Self::normalize_account_hint_v1)
            .unwrap_or_else(|| account_id.clone());
        let nonce_owner_account_id = tx
            .nonce_owner_account_id
            .as_deref()
            .and_then(Self::normalize_account_hint_v1)
            .unwrap_or_else(|| account_id.clone());
        AdapterExecutionSubjectV1 {
            account_id,
            fee_owner_account_id,
            nonce_owner_account_id,
        }
    }

    fn bump_subject_nonce_owner_v1(
        tx: &TxIR,
        state: &mut StateIR,
        kv: &mut HashMap<Vec<u8>, Vec<u8>>,
    ) {
        let subject = Self::subject_from_tx_v1(tx);
        let nonce_owner_key = Self::subject_state_key(subject.nonce_owner_account_id.as_str());
        let mut nonce_owner_account = state
            .get_account(&nonce_owner_key)
            .cloned()
            .unwrap_or_else(Self::default_account);
        nonce_owner_account.nonce = nonce_owner_account.nonce.max(tx.nonce.saturating_add(1));
        state.set_account(nonce_owner_key.clone(), nonce_owner_account);
        state.set_storage(
            nonce_owner_key,
            tx.nonce.to_le_bytes().to_vec(),
            Self::tx_hash_or_compute(tx),
        );
        kv.insert(
            Self::append_subject_nonce_key(subject.nonce_owner_account_id.as_str(), tx.nonce),
            Self::tx_hash_or_compute(tx),
        );
    }

    fn derive_contract_address(from: &[u8], nonce: u64) -> Vec<u8> {
        derive_create_contract_address_m0(from, nonce)
    }

    fn tx_hash_or_compute(tx: &TxIR) -> Vec<u8> {
        if tx.hash.is_empty() {
            compute_tx_ir_hash(tx)
        } else {
            tx.hash.clone()
        }
    }

    fn aoem_tx_artifact_key(tx_hash: &[u8]) -> Vec<u8> {
        let mut key = Vec::with_capacity(AOEM_TX_ARTIFACT_KEY_PREFIX_V1.len() + tx_hash.len());
        key.extend_from_slice(AOEM_TX_ARTIFACT_KEY_PREFIX_V1);
        key.extend_from_slice(tx_hash);
        key
    }

    fn aoem_tx_state_root_key(tx_hash: &[u8]) -> Vec<u8> {
        let mut key = Vec::with_capacity(AOEM_TX_STATE_ROOT_KEY_PREFIX_V1.len() + tx_hash.len());
        key.extend_from_slice(AOEM_TX_STATE_ROOT_KEY_PREFIX_V1);
        key.extend_from_slice(tx_hash);
        key
    }

    fn aoem_tx_anchor_key(tx_hash: &[u8]) -> Vec<u8> {
        let mut key = Vec::with_capacity(AOEM_TX_ANCHOR_KEY_PREFIX_V1.len() + tx_hash.len());
        key.extend_from_slice(AOEM_TX_ANCHOR_KEY_PREFIX_V1);
        key.extend_from_slice(tx_hash);
        key
    }

    fn tx_execution_status_ok(artifact: Option<&AoemTxExecutionArtifactV1>) -> bool {
        artifact.map(|item| item.status_ok).unwrap_or(true)
    }

    fn deploy_rejected_by_eip7610_v1(
        tx: &TxIR,
        artifact: Option<&AoemTxExecutionArtifactV1>,
    ) -> bool {
        if tx.tx_type != TxType::ContractDeploy {
            return false;
        }
        let contract_address = artifact
            .and_then(|item| item.contract_address.clone())
            .unwrap_or_else(|| Self::derive_contract_address(&tx.from, tx.nonce));
        is_eip7610_rejected_account_v1(tx.chain_id, &contract_address)
    }

    fn deploy_contract_address_v1(
        tx: &TxIR,
        artifact: Option<&AoemTxExecutionArtifactV1>,
    ) -> Vec<u8> {
        artifact
            .and_then(|item| item.contract_address.clone())
            .unwrap_or_else(|| Self::derive_contract_address(&tx.from, tx.nonce))
    }

    fn deploy_target_has_creation_collision_v1(
        tx: &TxIR,
        state: &StateIR,
        artifact: Option<&AoemTxExecutionArtifactV1>,
    ) -> bool {
        if tx.tx_type != TxType::ContractDeploy {
            return false;
        }
        let contract_address = Self::deploy_contract_address_v1(tx, artifact);
        let account_collision = state
            .get_account(&contract_address)
            .is_some_and(|account| account.nonce != 0 || account.code_hash.is_some());
        let storage_collision = state
            .storage
            .get(&contract_address)
            .is_some_and(|slots| !slots.is_empty());
        account_collision || storage_collision
    }

    fn target_account_has_runtime_code_v1(tx: &TxIR, state: &StateIR) -> bool {
        let Some(to) = tx.to.as_deref() else {
            return false;
        };
        state
            .get_account(to)
            .is_some_and(|account| account.code_hash.is_some())
            || Self::read_runtime_code_for_address(state, to)
                .is_some_and(|runtime_code| !runtime_code.is_empty())
    }

    fn effective_execution_tx_v1(tx: &TxIR, state: &StateIR) -> TxIR {
        let mut execution_tx = tx.clone();
        if tx.tx_type == TxType::Transfer && Self::target_account_has_runtime_code_v1(tx, state) {
            execution_tx.tx_type = TxType::ContractCall;
        }
        execution_tx
    }

    fn effective_tx_execution_status_ok(
        tx: &TxIR,
        state: &StateIR,
        artifact: Option<&AoemTxExecutionArtifactV1>,
    ) -> bool {
        Self::tx_execution_status_ok(artifact)
            && !Self::deploy_rejected_by_eip7610_v1(tx, artifact)
            && !Self::deploy_target_has_creation_collision_v1(tx, state, artifact)
    }

    fn tx_intrinsic_gas_v1(tx: &TxIR) -> u64 {
        const TX_INTRINSIC_BASE_GAS_V1: u64 = 21_000;
        const TX_DATA_ZERO_GAS_V1: u64 = 4;
        const TX_DATA_NONZERO_GAS_V1: u64 = 16;
        let data_gas = tx.data.iter().fold(0u64, |acc, byte| {
            let weight = if *byte == 0 {
                TX_DATA_ZERO_GAS_V1
            } else {
                TX_DATA_NONZERO_GAS_V1
            };
            acc.saturating_add(weight)
        });
        TX_INTRINSIC_BASE_GAS_V1.saturating_add(data_gas)
    }

    fn tx_intrinsic_gas_with_envelope_extras_v1(tx: &TxIR) -> u64 {
        let Ok(fields) = translate_raw_evm_tx_fields_m0(&tx.signature) else {
            return Self::tx_intrinsic_gas_v1(tx);
        };
        let access_list_address_count = fields.access_list_address_count.unwrap_or(0);
        let access_list_storage_key_count = fields.access_list_storage_key_count.unwrap_or(0);
        let blob_hash_count = fields.blob_hash_count.unwrap_or(0);
        estimate_intrinsic_gas_with_envelope_extras_for_chain_m0(
            tx.chain_id,
            tx,
            access_list_address_count,
            access_list_storage_key_count,
            blob_hash_count,
        )
    }

    fn host_execution_gas_estimate_v1(tx: &TxIR, status_ok: bool) -> u64 {
        if !status_ok {
            return Self::host_failed_execution_gas_estimate_v1(
                tx,
                AoemFailureClassV1::ExecutionFailed,
            );
        }
        let intrinsic = Self::tx_intrinsic_gas_with_envelope_extras_v1(tx);
        let execution = match tx.tx_type {
            TxType::Transfer => 0,
            TxType::ContractCall => 32_000,
            TxType::ContractDeploy => {
                // Host-side fallback baseline until AOEM tx-level gas is available everywhere.
                32_000u64.saturating_add((tx.data.len() as u64).saturating_mul(8))
            }
            TxType::Privacy => 25_000,
            TxType::CrossShard | TxType::CrossChainTransfer | TxType::CrossChainCall => 40_000,
        };
        intrinsic
            .saturating_add(execution)
            .min(tx.gas_limit.max(intrinsic))
    }

    fn host_failed_execution_gas_estimate_v1(tx: &TxIR, failure_class: AoemFailureClassV1) -> u64 {
        let intrinsic = Self::tx_intrinsic_gas_with_envelope_extras_v1(tx);
        if matches!(failure_class, AoemFailureClassV1::OutOfGas) {
            return tx.gas_limit.max(intrinsic);
        }
        let execution = match tx.tx_type {
            TxType::Transfer => 0,
            TxType::ContractCall => match failure_class {
                AoemFailureClassV1::Revert => 14_000,
                AoemFailureClassV1::Invalid => 4_000,
                AoemFailureClassV1::ExecutionFailed => 14_000,
                AoemFailureClassV1::OutOfGas => unreachable!("handled above"),
            },
            TxType::ContractDeploy => match failure_class {
                AoemFailureClassV1::Revert | AoemFailureClassV1::ExecutionFailed => {
                    20_000u64.saturating_add((tx.data.len() as u64).saturating_mul(4))
                }
                AoemFailureClassV1::Invalid => {
                    8_000u64.saturating_add((tx.data.len() as u64).saturating_mul(2))
                }
                AoemFailureClassV1::OutOfGas => unreachable!("handled above"),
            },
            TxType::Privacy => 25_000,
            TxType::CrossShard | TxType::CrossChainTransfer | TxType::CrossChainCall => 40_000,
        };
        intrinsic
            .saturating_add(execution)
            .min(tx.gas_limit.max(intrinsic))
    }

    fn should_derive_failed_artifact_gas_v1(artifact: &AoemTxExecutionArtifactV1) -> bool {
        if artifact.status_ok {
            return false;
        }
        if artifact.gas_used != 0 || artifact.cumulative_gas_used != 0 {
            return false;
        }
        artifact
            .revert_data
            .as_ref()
            .is_some_and(|value| !value.is_empty())
            || artifact
                .anchor
                .as_ref()
                .is_some_and(|anchor| anchor.return_code != 0)
    }

    fn resolved_execution_gas_fields_v1(
        tx: &TxIR,
        artifact: Option<&AoemTxExecutionArtifactV1>,
        status_ok: bool,
        failed_class: Option<AoemFailureClassV1>,
    ) -> (u64, u64, AoemFieldSourceV1) {
        let Some(artifact) = artifact else {
            let fallback = Self::host_execution_gas_estimate_v1(tx, status_ok);
            return (fallback, fallback, AoemFieldSourceV1::HostDerived);
        };
        if Self::should_derive_failed_artifact_gas_v1(artifact) {
            let fallback = if status_ok {
                Self::host_execution_gas_estimate_v1(tx, true)
            } else {
                let failure_class = failed_class.unwrap_or(AoemFailureClassV1::ExecutionFailed);
                Self::host_failed_execution_gas_estimate_v1(tx, failure_class)
            };
            return (fallback, fallback, AoemFieldSourceV1::HostDerived);
        }
        let gas_used = artifact.gas_used;
        let cumulative_gas_used = if artifact.cumulative_gas_used > 0 {
            artifact.cumulative_gas_used
        } else {
            gas_used
        };
        (gas_used, cumulative_gas_used, AoemFieldSourceV1::AoemRaw)
    }

    fn execution_fee_wei_v1(
        tx: &TxIR,
        artifact: Option<&AoemTxExecutionArtifactV1>,
        status_ok: bool,
    ) -> u128 {
        let failed_class = if status_ok {
            None
        } else if Self::deploy_rejected_by_eip7610_v1(tx, artifact) {
            Some(AoemFailureClassV1::Invalid)
        } else {
            Some(Self::classify_failed_tx_details_v1(tx, artifact).0)
        };
        let (gas_used, _, _) =
            Self::resolved_execution_gas_fields_v1(tx, artifact, status_ok, failed_class);
        let effective_gas_price = artifact
            .and_then(|item| item.effective_gas_price)
            .unwrap_or(tx.gas_price);
        (gas_used as u128).saturating_mul(effective_gas_price as u128)
    }

    fn debit_tracked_sender_balance_v1(
        tx: &TxIR,
        state: &mut StateIR,
        artifact: Option<&AoemTxExecutionArtifactV1>,
        status_ok: bool,
    ) -> Result<()> {
        let Some(mut from_account) = state.get_account(&tx.from).cloned() else {
            return Ok(());
        };
        let value_debit = if status_ok { tx.value } else { 0 };
        let fee_debit = Self::execution_fee_wei_v1(tx, artifact, status_ok);
        let total_debit = value_debit.saturating_add(fee_debit);
        if from_account.balance < total_debit {
            bail!(
                "evm tx insufficient balance: balance={} required={} value={} fee={}",
                from_account.balance,
                total_debit,
                value_debit,
                fee_debit
            );
        }
        from_account.balance -= total_debit;
        state.set_account(tx.from.clone(), from_account);
        Ok(())
    }

    fn classify_failed_tx_details_v1(
        tx: &TxIR,
        artifact: Option<&AoemTxExecutionArtifactV1>,
    ) -> (
        AoemFailureClassV1,
        AoemFailureClassSourceV1,
        AoemFailureRecoverabilityV1,
    ) {
        let Some(artifact) = artifact else {
            return (
                AoemFailureClassV1::ExecutionFailed,
                AoemFailureClassSourceV1::HeuristicNoArtifact,
                aoem_failure_recoverability_from_class_v1(AoemFailureClassV1::ExecutionFailed),
            );
        };
        if let Some(anchor) = artifact.anchor.as_ref() {
            if let Some(mapped) = classify_failure_from_anchor_v1(anchor) {
                return (mapped.class, mapped.source, mapped.recoverability);
            }
        }
        if artifact
            .revert_data
            .as_ref()
            .is_some_and(|value| !value.is_empty())
        {
            return (
                AoemFailureClassV1::Revert,
                AoemFailureClassSourceV1::HeuristicRevertData,
                aoem_failure_recoverability_from_class_v1(AoemFailureClassV1::Revert),
            );
        }
        if tx.gas_limit > 0 && artifact.gas_used >= tx.gas_limit {
            return (
                AoemFailureClassV1::OutOfGas,
                AoemFailureClassSourceV1::HeuristicGasUsedGeLimit,
                aoem_failure_recoverability_from_class_v1(AoemFailureClassV1::OutOfGas),
            );
        }
        (
            AoemFailureClassV1::ExecutionFailed,
            AoemFailureClassSourceV1::HeuristicDefault,
            aoem_failure_recoverability_from_class_v1(AoemFailureClassV1::ExecutionFailed),
        )
    }

    fn classify_failed_tx_v1(
        tx: &TxIR,
        artifact: Option<&AoemTxExecutionArtifactV1>,
    ) -> (&'static str, &'static str, &'static str) {
        let (class, source, recoverability) = Self::classify_failed_tx_details_v1(tx, artifact);
        (class.as_str(), source.as_str(), recoverability.as_str())
    }

    fn persist_failure_classification_v1(
        tx: &TxIR,
        state: &mut StateIR,
        artifact: Option<&AoemTxExecutionArtifactV1>,
        failure_class: &str,
        failure_class_source: &str,
        failure_recoverability: &str,
    ) {
        state.set_storage(
            tx.from.clone(),
            b"aoem:last_failure_class".to_vec(),
            failure_class.as_bytes().to_vec(),
        );
        state.set_storage(
            tx.from.clone(),
            b"aoem:last_failure_class_source".to_vec(),
            failure_class_source.as_bytes().to_vec(),
        );
        state.set_storage(
            tx.from.clone(),
            b"aoem:last_failure_recoverability".to_vec(),
            failure_recoverability.as_bytes().to_vec(),
        );
        state.set_storage(
            tx.from.clone(),
            b"aoem:last_failure_classification_contract".to_vec(),
            AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1.as_bytes().to_vec(),
        );
        if let Some(anchor) = artifact.and_then(|item| item.anchor.as_ref()) {
            state.set_storage(
                tx.from.clone(),
                b"aoem:last_failure_code".to_vec(),
                anchor.return_code.to_le_bytes().to_vec(),
            );
            state.set_storage(
                tx.from.clone(),
                b"aoem:last_failure_code_name".to_vec(),
                anchor.return_code_name.as_bytes().to_vec(),
            );
        }
    }

    fn record_execution_artifact(
        tx: &TxIR,
        state: &mut StateIR,
        kv: &mut HashMap<Vec<u8>, Vec<u8>>,
        artifact: &AoemTxExecutionArtifactV1,
    ) -> Result<()> {
        let tx_hash = Self::tx_hash_or_compute(tx);
        let artifact_bytes = crate::bincode_compat::serialize(artifact)
            .map_err(|e| anyhow!("encode aoem tx execution artifact failed: {e}"))?;
        kv.insert(Self::aoem_tx_artifact_key(&tx_hash), artifact_bytes);
        kv.insert(
            Self::aoem_tx_state_root_key(&tx_hash),
            artifact.state_root.to_vec(),
        );
        if let Some(anchor) = artifact.anchor.as_ref() {
            let anchor_bytes = crate::bincode_compat::serialize(anchor)
                .map_err(|e| anyhow!("encode aoem tx execution anchor failed: {e}"))?;
            kv.insert(Self::aoem_tx_anchor_key(&tx_hash), anchor_bytes.clone());
            state.set_storage(tx.from.clone(), b"aoem:last_anchor".to_vec(), anchor_bytes);
        }
        if let Some(receipt_type) = artifact.receipt_type {
            state.set_storage(
                tx.from.clone(),
                b"aoem:last_receipt_type".to_vec(),
                vec![receipt_type],
            );
        }
        if let Some(effective_gas_price) = artifact.effective_gas_price {
            state.set_storage(
                tx.from.clone(),
                b"aoem:last_effective_gas_price".to_vec(),
                effective_gas_price.to_le_bytes().to_vec(),
            );
        }
        if !artifact.log_bloom.is_empty() {
            state.set_storage(
                tx.from.clone(),
                b"aoem:last_log_bloom".to_vec(),
                artifact.log_bloom.clone(),
            );
        }
        if !artifact.event_logs.is_empty() {
            let event_logs = crate::bincode_compat::serialize(&artifact.event_logs)
                .map_err(|e| anyhow!("encode aoem tx event logs failed: {e}"))?;
            state.set_storage(
                tx.from.clone(),
                b"aoem:last_event_logs".to_vec(),
                event_logs,
            );
        }
        if let Some(runtime_code_hash) = artifact.runtime_code_hash.as_ref() {
            state.set_storage(
                tx.from.clone(),
                b"aoem:last_runtime_code_hash".to_vec(),
                runtime_code_hash.clone(),
            );
        }
        if let Some(revert_data) = artifact.revert_data.as_ref() {
            state.set_storage(
                tx.from.clone(),
                b"aoem:last_revert_data".to_vec(),
                revert_data.clone(),
            );
        }
        state.set_storage(
            tx.from.clone(),
            b"aoem:last_artifact_state_root".to_vec(),
            artifact.state_root.to_vec(),
        );
        Ok(())
    }

    fn commit_canonical_state_root(&self, root: &[u8]) {
        if let Ok(mut cache) = self.state_root_cache.lock() {
            cache.root = root.to_vec();
            cache.dirty = false;
        }
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

    fn runtime_code_storage_key() -> &'static [u8] {
        ADAPTER_EVM_RUNTIME_CODE_STORAGE_KEY
    }

    fn canonical_tx_type(tx_type: TxType) -> AoemCanonicalTxTypeV1 {
        match tx_type {
            TxType::Transfer => AoemCanonicalTxTypeV1::Transfer,
            TxType::ContractCall => AoemCanonicalTxTypeV1::ContractCall,
            TxType::ContractDeploy => AoemCanonicalTxTypeV1::ContractDeploy,
            TxType::Privacy => AoemCanonicalTxTypeV1::Privacy,
            TxType::CrossShard => AoemCanonicalTxTypeV1::CrossShard,
            TxType::CrossChainTransfer => AoemCanonicalTxTypeV1::CrossChainTransfer,
            TxType::CrossChainCall => AoemCanonicalTxTypeV1::CrossChainCall,
        }
    }

    fn read_runtime_code_for_address(state: &StateIR, address: &[u8]) -> Option<Vec<u8>> {
        state
            .get_storage(address, Self::runtime_code_storage_key())
            .cloned()
            .or_else(|| state.get_storage(address, b"deploy:runtime_code").cloned())
    }

    fn read_runtime_code_hash_for_address(state: &StateIR, address: &[u8]) -> Option<Vec<u8>> {
        state
            .get_storage(address, b"deploy:runtime_code_hash")
            .cloned()
            .or_else(|| {
                state
                    .get_storage(address, b"deploy:runtime_code_hash_hint")
                    .cloned()
            })
            .or_else(|| {
                state
                    .get_account(address)
                    .and_then(|account| account.code_hash.clone())
            })
    }

    fn to_evm_address20_v1(address: &[u8]) -> Option<[u8; 20]> {
        if address.len() != 20 {
            return None;
        }
        let mut out = [0u8; 20];
        out.copy_from_slice(address);
        Some(out)
    }

    fn u128_to_be32_v1(value: u128) -> [u8; 32] {
        let mut out = [0u8; 32];
        out[16..32].copy_from_slice(&value.to_be_bytes());
        out
    }

    fn bytes_to_bal_word32_v1(domain: &[u8], raw: &[u8]) -> [u8; 32] {
        if raw.len() == 32 {
            let mut out = [0u8; 32];
            out.copy_from_slice(raw);
            return out;
        }
        if raw.len() < 32 {
            let mut out = [0u8; 32];
            out[(32 - raw.len())..].copy_from_slice(raw);
            return out;
        }

        let mut hasher = Sha256::new();
        hasher.update(domain);
        hasher.update(raw);
        hasher.finalize().into()
    }

    fn block_access_index_from_tx_index_v1(tx_index: u32) -> u32 {
        tx_index.saturating_add(1)
    }

    fn add_observed_storage_write_if_present_v1(
        out: &mut EvmConstructionBlockAccessListV1,
        state: &StateIR,
        block_access_index: u32,
        address: [u8; 20],
        owner: &[u8],
        slot_key: &[u8],
    ) -> bool {
        if let Some(post_value) = state.get_storage(owner, slot_key) {
            out.storage_write(
                block_access_index,
                address,
                Self::bytes_to_bal_word32_v1(b"novovm-bal-slot-v1", slot_key),
                Self::bytes_to_bal_word32_v1(b"novovm-bal-storage-value-v1", post_value),
            );
            true
        } else {
            false
        }
    }

    fn build_observed_block_access_list_for_tx_v1(
        tx: &TxIR,
        state: &StateIR,
        resolved_artifact: &AoemTxExecutionArtifactV1,
        tx_index: u32,
    ) -> NovoVmObservedBlockAccessListV1 {
        let mut out = EvmConstructionBlockAccessListV1::new();
        let block_access_index = Self::block_access_index_from_tx_index_v1(tx_index);
        let from_evm = Self::to_evm_address20_v1(&tx.from);
        let mut block_access_list_complete = false;

        if let Some(from) = from_evm {
            out.account_read(from);
            out.nonce_change(block_access_index, from, tx.nonce.saturating_add(1));
            if let Some(post_balance) = state.get_account(&tx.from).map(|account| account.balance) {
                out.balance_change(
                    block_access_index,
                    from,
                    Self::u128_to_be32_v1(post_balance),
                );
            }
        }
        for entry in &tx.evm_access_list {
            if let Some(address) = Self::to_evm_address20_v1(&entry.address) {
                out.account_read(address);
                for storage_key in &entry.storage_keys {
                    if storage_key.len() == 32 {
                        let mut slot = [0u8; 32];
                        slot.copy_from_slice(storage_key);
                        out.storage_read(address, slot);
                    }
                }
            }
        }

        match tx.tx_type {
            TxType::Transfer | TxType::ContractCall => {
                if let Some(to_bytes) = tx.to.as_deref() {
                    if let Some(to) = Self::to_evm_address20_v1(to_bytes) {
                        out.account_read(to);
                        if resolved_artifact.status_ok && tx.value > 0 {
                            if let Some(post_balance) =
                                state.get_account(to_bytes).map(|account| account.balance)
                            {
                                out.balance_change(
                                    block_access_index,
                                    to,
                                    Self::u128_to_be32_v1(post_balance),
                                );
                            }
                        }
                        block_access_list_complete = tx.tx_type == TxType::Transfer
                            && resolved_artifact.status_ok
                            && from_evm.is_some();
                        if tx.tx_type == TxType::ContractCall && resolved_artifact.status_ok {
                            let slot_key = tx.nonce.to_le_bytes();
                            if Self::add_observed_storage_write_if_present_v1(
                                &mut out,
                                state,
                                block_access_index,
                                to,
                                to_bytes,
                                &slot_key,
                            ) {
                                block_access_list_complete = from_evm.is_some();
                            }
                        }
                    }
                }
            }
            TxType::ContractDeploy => {
                if let Some(contract_bytes) = resolved_artifact.contract_address.as_deref() {
                    if let Some(contract) = Self::to_evm_address20_v1(contract_bytes) {
                        out.account_read(contract);
                        if resolved_artifact.status_ok {
                            let mut deploy_complete = from_evm.is_some();
                            if tx.value > 0 {
                                if let Some(post_balance) = state
                                    .get_account(contract_bytes)
                                    .map(|account| account.balance)
                                {
                                    out.balance_change(
                                        block_access_index,
                                        contract,
                                        Self::u128_to_be32_v1(post_balance),
                                    );
                                } else {
                                    deploy_complete = false;
                                }
                            }
                            if let Some(runtime_code) =
                                resolved_artifact.runtime_code.clone().or_else(|| {
                                    Self::read_runtime_code_for_address(state, contract_bytes)
                                })
                            {
                                out.code_change(block_access_index, contract, runtime_code);
                            } else {
                                deploy_complete = false;
                            }
                            let init_hash_observed = Self::add_observed_storage_write_if_present_v1(
                                &mut out,
                                state,
                                block_access_index,
                                contract,
                                contract_bytes,
                                b"deploy:init_code_hash",
                            );
                            let runtime_hash_observed =
                                Self::add_observed_storage_write_if_present_v1(
                                    &mut out,
                                    state,
                                    block_access_index,
                                    contract,
                                    contract_bytes,
                                    b"deploy:runtime_code_hash",
                                );
                            let _ = Self::add_observed_storage_write_if_present_v1(
                                &mut out,
                                state,
                                block_access_index,
                                contract,
                                contract_bytes,
                                b"deploy:runtime_code_hash_hint",
                            );
                            let _ = Self::add_observed_storage_write_if_present_v1(
                                &mut out,
                                state,
                                block_access_index,
                                contract,
                                contract_bytes,
                                b"deploy:runtime_code",
                            );
                            let _ = Self::add_observed_storage_write_if_present_v1(
                                &mut out,
                                state,
                                block_access_index,
                                contract,
                                contract_bytes,
                                Self::runtime_code_storage_key(),
                            );
                            block_access_list_complete =
                                deploy_complete && init_hash_observed && runtime_hash_observed;
                        }
                    }
                }
            }
            TxType::Privacy
            | TxType::CrossShard
            | TxType::CrossChainTransfer
            | TxType::CrossChainCall => {}
        }

        NovoVmObservedBlockAccessListV1 {
            block_access_list: if out.is_empty() {
                None
            } else {
                Some(out.to_access_list())
            },
            block_access_list_complete,
        }
    }

    fn execution_reconstruction_rules(
        &self,
        rebuild_logs_from_runtime_code_when_missing: bool,
    ) -> AoemReceiptDerivationRulesV1 {
        AoemReceiptDerivationRulesV1 {
            derive_runtime_code_hash_when_missing: true,
            derive_log_bloom_from_logs_when_missing: true,
            rebuild_logs_from_runtime_code_when_missing,
            rebuild_logs_requires_status_ok: true,
            rebuild_logs_requires_runtime_code: true,
            rebuild_logs_requires_call_data: false,
            deploy_runtime_code_fallback_to_init_code: true,
            derive_anchor_log_when_logs_empty: true,
        }
    }

    fn build_execution_reconstruction_input(
        &self,
        tx: &TxIR,
        state: &StateIR,
        artifact: Option<&AoemTxExecutionArtifactV1>,
        context: ExecutionReconstructionContextV1<'_>,
    ) -> AoemExecutionReconstructionInputV1 {
        let status_ok = context.status_ok;
        let tx_hash = Self::tx_hash_or_compute(tx);
        let contract_address_from_artifact =
            artifact.and_then(|item| item.contract_address.clone());
        let contract_address = if status_ok {
            contract_address_from_artifact.clone().or_else(|| {
                if tx.tx_type == TxType::ContractDeploy {
                    Some(Self::derive_contract_address(&tx.from, tx.nonce))
                } else {
                    None
                }
            })
        } else {
            None
        };

        let runtime_owner = match tx.tx_type {
            TxType::ContractCall => tx.to.clone(),
            TxType::ContractDeploy => contract_address.clone(),
            _ => None,
        };
        let runtime_code_from_artifact = artifact.and_then(|item| item.runtime_code.clone());
        let runtime_code = runtime_code_from_artifact.clone().or_else(|| {
            runtime_owner
                .as_ref()
                .and_then(|owner| Self::read_runtime_code_for_address(state, owner.as_slice()))
        });
        let runtime_code_hash_from_artifact =
            artifact.and_then(|item| item.runtime_code_hash.clone());
        let runtime_code_hash = runtime_code_hash_from_artifact.clone().or_else(|| {
            runtime_owner
                .as_ref()
                .and_then(|owner| Self::read_runtime_code_hash_for_address(state, owner.as_slice()))
        });

        let raw_event_logs = artifact
            .map(|item| item.event_logs.clone())
            .filter(|logs| !logs.is_empty())
            .unwrap_or_else(|| context.raw_execution_logs.to_vec());
        let raw_log_bloom = artifact.map(|item| item.log_bloom.clone());
        let failed_class = if status_ok {
            None
        } else {
            Some(Self::classify_failed_tx_details_v1(tx, artifact).0)
        };
        let raw_revert_data = if status_ok {
            artifact.and_then(|item| item.revert_data.clone())
        } else {
            match failed_class {
                Some(AoemFailureClassV1::Revert) => {
                    artifact.and_then(|item| item.revert_data.clone())
                }
                _ => None,
            }
        };

        let status_source = if artifact.is_some() {
            AoemFieldSourceV1::AoemRaw
        } else {
            AoemFieldSourceV1::HostDerived
        };
        let (resolved_gas_used, resolved_cumulative_gas_used, gas_used_source) =
            Self::resolved_execution_gas_fields_v1(tx, artifact, status_ok, failed_class);
        let state_root_source = if artifact.is_some() {
            AoemFieldSourceV1::AoemRaw
        } else {
            AoemFieldSourceV1::HostDerived
        };
        let contract_address_source = if contract_address_from_artifact.is_some() {
            AoemFieldSourceV1::AoemRaw
        } else if contract_address.is_some() {
            AoemFieldSourceV1::HostDerived
        } else {
            AoemFieldSourceV1::Missing
        };
        let runtime_code_source = if runtime_code_from_artifact.is_some() {
            AoemFieldSourceV1::AoemRaw
        } else if runtime_code.is_some() {
            AoemFieldSourceV1::HostState
        } else {
            AoemFieldSourceV1::Missing
        };
        let runtime_code_hash_source = if runtime_code_hash_from_artifact.is_some() {
            AoemFieldSourceV1::AoemRaw
        } else if runtime_code_hash.is_some() {
            AoemFieldSourceV1::HostState
        } else {
            AoemFieldSourceV1::Missing
        };
        let event_logs_source = if artifact
            .map(|item| !item.event_logs.is_empty())
            .unwrap_or(false)
        {
            AoemFieldSourceV1::AoemRaw
        } else if !context.raw_execution_logs.is_empty() {
            AoemFieldSourceV1::HostState
        } else {
            AoemFieldSourceV1::Missing
        };
        let log_bloom_source = if artifact
            .map(|item| !item.log_bloom.is_empty())
            .unwrap_or(false)
        {
            AoemFieldSourceV1::AoemRaw
        } else {
            AoemFieldSourceV1::Missing
        };
        let revert_data_source = if raw_revert_data.is_some() {
            AoemFieldSourceV1::AoemRaw
        } else {
            AoemFieldSourceV1::Missing
        };
        let log_emitter = tx
            .to
            .clone()
            .or_else(|| contract_address_from_artifact.clone())
            .or_else(|| contract_address.clone())
            .or_else(|| Some(tx.from.clone()));

        AoemExecutionReconstructionInputV1 {
            tx_index: context.tx_index,
            tx_hash,
            tx_type: Self::canonical_tx_type(tx.tx_type),
            from: tx.from.clone(),
            to: tx.to.clone(),
            nonce: tx.nonce,
            gas_limit: tx.gas_limit,
            gas_used: Some(resolved_gas_used),
            cumulative_gas_used: Some(resolved_cumulative_gas_used),
            gas_price: artifact
                .and_then(|item| item.effective_gas_price)
                .or(Some(tx.gas_price)),
            receipt_type: artifact.and_then(|item| item.receipt_type),
            status_ok,
            state_root: context.final_state_root,
            contract_address,
            call_data: tx.data.clone(),
            init_code: if tx.tx_type == TxType::ContractDeploy {
                Some(tx.data.clone())
            } else {
                None
            },
            runtime_code,
            runtime_code_hash,
            revert_data: raw_revert_data,
            raw_event_logs,
            raw_log_bloom,
            anchor: artifact.and_then(|item| item.anchor.clone()),
            log_emitter,
            sources: AoemExecutionReconstructionSourcesV1 {
                status_source,
                gas_used_source,
                state_root_source,
                contract_address_source,
                runtime_code_source,
                runtime_code_hash_source,
                event_logs_source,
                log_bloom_source,
                revert_data_source,
            },
        }
    }

    fn runtime_log_rebuild_enabled(&self) -> bool {
        Self::bool_env_default(ADAPTER_RUNTIME_LOG_REBUILD_ENV, true)
    }

    fn runtime_log_rebuild_required(&self) -> bool {
        Self::bool_env_default(ADAPTER_RUNTIME_LOG_REBUILD_REQUIRED_ENV, false)
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

    fn adapter_persona(&self, tx: &TxIR) -> PersonaAddress {
        PersonaAddress {
            persona_type: PersonaType::Evm,
            chain_id: tx.chain_id,
            external_address: tx.from.clone(),
        }
    }

    fn begin_execution_receipt_context(&mut self, tx: &TxIR) {
        self.execution_current_tx_hash = Self::tx_hash_or_compute(tx);
        self.execution_current_tx_index = self.execution_tx_ordinal;
        self.execution_current_log_index = 0;
        self.execution_current_logs.clear();
        self.execution_tx_ordinal = self.execution_tx_ordinal.saturating_add(1);
    }

    fn reset_execution_receipt_context(&mut self) {
        self.execution_current_tx_hash.clear();
        self.execution_current_tx_index = 0;
        self.execution_current_log_index = 0;
        self.execution_current_logs.clear();
    }

    fn route_transaction_through_unified_account(
        &mut self,
        tx: &TxIR,
    ) -> Result<AdapterExecutionSubjectV1> {
        let subject = Self::subject_from_tx_v1(tx);
        if !self.unified_account_guard_enabled() {
            return Ok(subject);
        }

        let now = Self::now_unix_sec();
        let persona = self.adapter_persona(tx);

        if self.unified_account_autoprovision_enabled() {
            match self.unified_account_router.create_uca(
                subject.account_id.clone(),
                tx.from.clone(),
                now,
            ) {
                Ok(()) | Err(UnifiedAccountError::UcaAlreadyExists { .. }) => {}
                Err(err) => {
                    bail!(
                        "unified account adapter ingress create failed (uca_id={}): {}",
                        subject.account_id,
                        err
                    );
                }
            }
            match self.unified_account_router.add_binding(
                &subject.account_id,
                AccountRole::Owner,
                persona.clone(),
                now,
            ) {
                Ok(()) | Err(UnifiedAccountError::BindingAlreadyExists) => {}
                Err(err) => {
                    bail!(
                        "unified account adapter ingress bind failed (uca_id={}, chain_id={}): {}",
                        subject.account_id,
                        persona.chain_id,
                        err
                    );
                }
            }
        }

        let route = self
            .unified_account_router
            .route(RouteRequest {
                uca_id: subject.account_id.clone(),
                persona: persona.clone(),
                role: AccountRole::Owner,
                protocol: ProtocolKind::Eth,
                signature_domain: self.unified_account_signature_domain(tx.chain_id),
                nonce: tx.nonce,
                kyc_attestation_provided: false,
                kyc_verified: false,
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
                    subject.account_id,
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

        Ok(subject)
    }

    fn decode_privacy_signature(tx: &TxIR) -> Result<Web30RingSignature> {
        serde_json::from_slice(&tx.signature)
            .map_err(|e| anyhow::anyhow!("privacy tx ring signature decode failed: {e}"))
    }

    fn decode_privacy_stealth_address(tx: &TxIR) -> Result<Web30StealthAddress> {
        crate::bincode_compat::deserialize(&tx.data)
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
        artifact: Option<&AoemTxExecutionArtifactV1>,
    ) -> Result<()> {
        let to = tx
            .to
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("transfer tx missing target address"))?;
        Self::debit_tracked_sender_balance_v1(
            tx,
            state,
            artifact,
            Self::tx_execution_status_ok(artifact),
        )?;
        Self::bump_subject_nonce_owner_v1(tx, state, kv);

        if Self::tx_execution_status_ok(artifact) {
            let mut to_account = state
                .get_account(to)
                .cloned()
                .unwrap_or_else(Self::default_account);
            to_account.balance = to_account.balance.saturating_add(tx.value);
            state.set_account(to.clone(), to_account);
        } else {
            let (failure_class, failure_class_source, failure_recoverability) =
                Self::classify_failed_tx_v1(tx, artifact);
            state.set_storage(
                tx.from.clone(),
                b"aoem:last_transfer_failure".to_vec(),
                Self::tx_hash_or_compute(tx),
            );
            Self::persist_failure_classification_v1(
                tx,
                state,
                artifact,
                failure_class,
                failure_class_source,
                failure_recoverability,
            );
        }
        if let Some(artifact) = artifact {
            Self::record_execution_artifact(tx, state, kv, artifact)?;
        }
        Ok(())
    }

    fn apply_contract_call(
        tx: &TxIR,
        state: &mut StateIR,
        kv: &mut HashMap<Vec<u8>, Vec<u8>>,
        artifact: Option<&AoemTxExecutionArtifactV1>,
    ) -> Result<()> {
        let to = tx
            .to
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("contract call tx missing target address"))?;

        Self::debit_tracked_sender_balance_v1(
            tx,
            state,
            artifact,
            Self::tx_execution_status_ok(artifact),
        )?;
        Self::bump_subject_nonce_owner_v1(tx, state, kv);

        if Self::tx_execution_status_ok(artifact) {
            let mut to_account = state
                .get_account(to)
                .cloned()
                .unwrap_or_else(Self::default_account);
            to_account.balance = to_account.balance.saturating_add(tx.value);
            state.set_account(to.clone(), to_account);

            let slot = tx.nonce.to_le_bytes().to_vec();
            state.set_storage(to.clone(), slot, Self::tx_hash_or_compute(tx));
        } else {
            let (failure_class, failure_class_source, failure_recoverability) =
                Self::classify_failed_tx_v1(tx, artifact);
            state.set_storage(
                tx.from.clone(),
                b"aoem:last_contract_call_failure".to_vec(),
                Self::tx_hash_or_compute(tx),
            );
            Self::persist_failure_classification_v1(
                tx,
                state,
                artifact,
                failure_class,
                failure_class_source,
                failure_recoverability,
            );
        }
        if let Some(artifact) = artifact {
            Self::record_execution_artifact(tx, state, kv, artifact)?;
        }
        Ok(())
    }

    fn apply_contract_deploy(
        tx: &TxIR,
        state: &mut StateIR,
        kv: &mut HashMap<Vec<u8>, Vec<u8>>,
        artifact: Option<&AoemTxExecutionArtifactV1>,
    ) -> Result<()> {
        if tx.to.is_some() {
            bail!("contract deploy tx must not set target address");
        }
        if tx.data.is_empty() {
            bail!("contract deploy tx missing init code");
        }

        let contract_address = Self::deploy_contract_address_v1(tx, artifact);
        let eip7610_rejected = is_eip7610_rejected_account_v1(tx.chain_id, &contract_address);
        let creation_collision = Self::deploy_target_has_creation_collision_v1(tx, state, artifact);
        let status_ok =
            Self::tx_execution_status_ok(artifact) && !eip7610_rejected && !creation_collision;
        Self::debit_tracked_sender_balance_v1(tx, state, artifact, status_ok)?;
        Self::bump_subject_nonce_owner_v1(tx, state, kv);
        if status_ok {
            let mut contract_account = state
                .get_account(&contract_address)
                .cloned()
                .unwrap_or_else(Self::default_account);
            contract_account.balance = contract_account.balance.saturating_add(tx.value);
            contract_account.nonce = contract_account.nonce.max(1);
            let fallback_init_code_hash: [u8; 32] = Sha256::digest(&tx.data).into();
            let runtime_code_hash = artifact
                .and_then(|item| item.runtime_code_hash.clone())
                .unwrap_or_else(|| fallback_init_code_hash.to_vec());
            contract_account.code_hash = Some(runtime_code_hash.clone());
            state.set_account(contract_address.clone(), contract_account);
            state.set_storage(
                contract_address.clone(),
                b"deploy:init_code_hash".to_vec(),
                fallback_init_code_hash.to_vec(),
            );
            state.set_storage(
                contract_address.clone(),
                b"deploy:runtime_code_hash".to_vec(),
                runtime_code_hash.clone(),
            );
            if artifact
                .and_then(|item| item.runtime_code.as_ref())
                .is_none()
            {
                state.set_storage(
                    contract_address.clone(),
                    b"deploy:runtime_code_hash_hint".to_vec(),
                    runtime_code_hash.clone(),
                );
            }
            if let Some(runtime_code) = artifact.and_then(|item| item.runtime_code.as_ref()) {
                state.set_storage(
                    contract_address.clone(),
                    b"deploy:runtime_code".to_vec(),
                    runtime_code.clone(),
                );
                state.set_storage(
                    contract_address.clone(),
                    Self::runtime_code_storage_key().to_vec(),
                    runtime_code.clone(),
                );
            }
        } else {
            state.set_storage(
                tx.from.clone(),
                b"deploy:last_failed_contract_address".to_vec(),
                contract_address.clone(),
            );
            if eip7610_rejected {
                state.set_storage(
                    tx.from.clone(),
                    b"deploy:last_reject_reason".to_vec(),
                    b"eip7610_rejected_account".to_vec(),
                );
                state.set_storage(
                    tx.from.clone(),
                    b"deploy:last_reject_contract".to_vec(),
                    EIP7610_REJECTION_CONTRACT_V1.as_bytes().to_vec(),
                );
            } else if creation_collision {
                state.set_storage(
                    tx.from.clone(),
                    b"deploy:last_reject_reason".to_vec(),
                    b"create_collision_existing_account".to_vec(),
                );
                state.set_storage(
                    tx.from.clone(),
                    b"deploy:last_reject_contract".to_vec(),
                    contract_address.clone(),
                );
            }
            let (failure_class, failure_class_source, failure_recoverability) = if eip7610_rejected
            {
                (
                    AoemFailureClassV1::Invalid.as_str(),
                    AoemFailureClassSourceV1::HeuristicDefault.as_str(),
                    aoem_failure_recoverability_from_class_v1(AoemFailureClassV1::Invalid).as_str(),
                )
            } else if creation_collision {
                (
                    AoemFailureClassV1::ExecutionFailed.as_str(),
                    AoemFailureClassSourceV1::HeuristicDefault.as_str(),
                    aoem_failure_recoverability_from_class_v1(AoemFailureClassV1::ExecutionFailed)
                        .as_str(),
                )
            } else {
                Self::classify_failed_tx_v1(tx, artifact)
            };
            state.set_storage(
                tx.from.clone(),
                b"aoem:last_contract_deploy_failure".to_vec(),
                Self::tx_hash_or_compute(tx),
            );
            Self::persist_failure_classification_v1(
                tx,
                state,
                artifact,
                failure_class,
                failure_class_source,
                failure_recoverability,
            );
        }
        if let Some(artifact) = artifact {
            Self::record_execution_artifact(tx, state, kv, artifact)?;
        }
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
        state.set_account(tx.from.clone(), from_account);
        Self::bump_subject_nonce_owner_v1(tx, state, kv);

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
        if record_key_image {
            kv.insert(key_image_key, tx.hash.clone());
        }
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
                match acc.code_hash.as_deref() {
                    Some(code_hash) => {
                        hasher.update([1u8]);
                        hasher.update(code_hash);
                    }
                    None => hasher.update([0u8]),
                }
                hasher.update(acc.storage_root.as_slice());
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

    pub fn execute_transaction_with_observed_metadata_v1(
        &mut self,
        tx: &TxIR,
        state: &mut StateIR,
        artifact: Option<&AoemTxExecutionArtifactV1>,
    ) -> Result<NovoVmTxExecutionOutcomeV1> {
        self.ensure_initialized()?;
        self.begin_execution_receipt_context(tx);
        let already_verified = if tx.hash.is_empty() {
            false
        } else {
            self.verified_tx_cache.remove(&tx.hash).is_some()
        };
        if !already_verified && !self.verify_transaction(tx)? {
            self.reset_execution_receipt_context();
            bail!("transaction verification failed");
        }
        let tx_hash = Self::tx_hash_or_compute(tx);
        if let Some(artifact) = artifact {
            if !artifact.tx_hash.is_empty() && artifact.tx_hash != tx_hash {
                self.reset_execution_receipt_context();
                bail!("aoem tx execution artifact hash mismatch");
            }
        }
        if let Err(err) = self.route_transaction_through_unified_account(tx) {
            self.reset_execution_receipt_context();
            return Err(err);
        }
        if self.state.get_account(&tx.from).is_none() {
            if let Some(account) = state.get_account(&tx.from).cloned() {
                self.state.set_account(tx.from.clone(), account);
            }
        }
        if let Some(to) = tx.to.as_ref() {
            if self.state.get_account(to).is_none() {
                if let Some(account) = state.get_account(to).cloned() {
                    self.state.set_account(to.clone(), account);
                }
            }
            if !self.state.storage.contains_key(to) {
                if let Some(storage) = state.storage.get(to).cloned() {
                    self.state.storage.insert(to.clone(), storage);
                }
            }
        }
        let execution_tx = Self::effective_execution_tx_v1(tx, state);
        if execution_tx.tx_type == TxType::ContractDeploy {
            let contract_address = Self::deploy_contract_address_v1(&execution_tx, artifact);
            if self.state.get_account(&contract_address).is_none() {
                if let Some(account) = state.get_account(&contract_address).cloned() {
                    self.state.set_account(contract_address.clone(), account);
                }
            }
            if !self.state.storage.contains_key(&contract_address) {
                if let Some(storage) = state.storage.get(&contract_address).cloned() {
                    self.state.storage.insert(contract_address, storage);
                }
            }
        }
        let status_ok = Self::effective_tx_execution_status_ok(&execution_tx, state, artifact);
        match execution_tx.tx_type {
            TxType::Transfer => {
                Self::apply_transfer(&execution_tx, state, &mut self.kv, artifact)?;
                Self::apply_transfer(&execution_tx, &mut self.state, &mut self.kv, artifact)?;
            }
            TxType::ContractCall => {
                Self::apply_contract_call(&execution_tx, state, &mut self.kv, artifact)?;
                Self::apply_contract_call(&execution_tx, &mut self.state, &mut self.kv, artifact)?;
            }
            TxType::ContractDeploy => {
                Self::apply_contract_deploy(&execution_tx, state, &mut self.kv, artifact)?;
                Self::apply_contract_deploy(
                    &execution_tx,
                    &mut self.state,
                    &mut self.kv,
                    artifact,
                )?;
            }
            TxType::Privacy => {
                Self::apply_privacy_transfer(&execution_tx, state, &mut self.kv, false)?;
                Self::apply_privacy_transfer(&execution_tx, &mut self.state, &mut self.kv, true)?;
            }
            _ => {
                self.reset_execution_receipt_context();
                bail!(
                    "unsupported tx_type for native adapter: {:?}",
                    execution_tx.tx_type
                )
            }
        }

        let final_state_root = if let Some(artifact) = artifact {
            let root = artifact.state_root.to_vec();
            self.commit_canonical_state_root(root.as_slice());
            state.state_root = root.clone();
            self.state.state_root = root;
            normalize_root32(state.state_root.as_slice())
        } else {
            let last_root = self.mark_state_root_dirty_and_snapshot_last();
            state.state_root = last_root.clone();
            self.state.state_root = last_root;
            normalize_root32(state.state_root.as_slice())
        };

        let raw_execution_logs = std::mem::take(&mut self.execution_current_logs);
        let reconstruction_context = ExecutionReconstructionContextV1 {
            status_ok,
            final_state_root,
            tx_index: self.execution_current_tx_index,
            raw_execution_logs: raw_execution_logs.as_slice(),
        };
        let reconstruction_input = self.build_execution_reconstruction_input(
            &execution_tx,
            state,
            artifact,
            reconstruction_context,
        );
        let reconstruction_rules =
            self.execution_reconstruction_rules(self.runtime_log_rebuild_enabled());
        let resolved_artifact = match reconstruct_tx_execution_artifact_v1(
            &reconstruction_input,
            &reconstruction_rules,
        ) {
            Ok(artifact) => artifact,
            Err(primary_err)
                if reconstruction_rules.rebuild_logs_from_runtime_code_when_missing
                    && !self.runtime_log_rebuild_required() =>
            {
                let fallback_rules = AoemReceiptDerivationRulesV1 {
                    rebuild_logs_from_runtime_code_when_missing: false,
                    ..reconstruction_rules.clone()
                };
                reconstruct_tx_execution_artifact_v1(&reconstruction_input, &fallback_rules)
                    .map_err(|fallback_err| {
                        anyhow!(
                            "execution artifact reconstruction failed: primary={primary_err}; fallback={fallback_err}"
                        )
                    })?
            }
            Err(err) => {
                self.reset_execution_receipt_context();
                return Err(err);
            }
        };

        Self::record_execution_artifact(&execution_tx, state, &mut self.kv, &resolved_artifact)?;
        Self::record_execution_artifact(
            &execution_tx,
            &mut self.state,
            &mut self.kv,
            &resolved_artifact,
        )?;
        let observed_block_access_list = Self::build_observed_block_access_list_for_tx_v1(
            &execution_tx,
            state,
            &resolved_artifact,
            self.execution_current_tx_index,
        );
        self.reset_execution_receipt_context();
        Ok(NovoVmTxExecutionOutcomeV1 {
            artifact: resolved_artifact,
            observed_block_access_list,
        })
    }

    pub fn execute_transaction_with_artifact(
        &mut self,
        tx: &TxIR,
        state: &mut StateIR,
        artifact: Option<&AoemTxExecutionArtifactV1>,
    ) -> Result<AoemTxExecutionArtifactV1> {
        Ok(self
            .execute_transaction_with_observed_metadata_v1(tx, state, artifact)?
            .artifact)
    }
}

fn normalize_root32(root: &[u8]) -> [u8; 32] {
    if root.len() == 32 {
        let mut out = [0u8; 32];
        out.copy_from_slice(root);
        return out;
    }
    let mut hasher = Sha256::new();
    hasher.update(root);
    hasher.finalize().into()
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
    crate::bincode_compat::serialize(stealth_address)
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
        account_id: None,
        fee_owner_account_id: None,
        nonce_owner_account_id: None,
        to: None,
        value: envelope.value,
        gas_limit: envelope.gas_limit,
        gas_price: envelope.gas_price,
        nonce: envelope.nonce,
        data,
        signature: Vec::new(),
        chain_id: envelope.chain_id,
        tx_type: TxType::Privacy,
        execution_policy: Default::default(),
        evm_access_list: Vec::new(),
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

fn verify_evm_raw_tx_signature_v1(chain_type: ChainType, tx: &TxIR) -> Result<bool> {
    let Some(recovered_from) = recover_raw_evm_tx_sender_m0(&tx.signature)? else {
        return Ok(false);
    };
    let parsed_fields = match translate_raw_evm_tx_fields_m0(&tx.signature) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    let canonical =
        tx_ir_from_raw_fields_m0(&parsed_fields, &tx.signature, recovered_from, tx.chain_id);
    if tx.hash != canonical.hash {
        return Ok(false);
    }
    if tx.evm_access_list != canonical.evm_access_list {
        return Ok(false);
    }
    let profile = match resolve_evm_profile(chain_type, tx.chain_id) {
        Ok(profile) => profile,
        Err(_) => return Ok(false),
    };
    if validate_tx_semantics_m0(&profile, &canonical).is_err() {
        return Ok(false);
    }
    Ok(true)
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
                    verify_evm_raw_tx_signature_v1(self.config.chain_type, tx)?
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
        self.execute_transaction_with_artifact(tx, state, None)
            .map(|_| ())
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
        let account_id = self.subject_account_id_from_address_v1(address);
        let subject_key = Self::subject_state_key(&account_id);
        Ok(self.state.get_account(&subject_key).map_or_else(
            || self.state.get_account(address).map_or(0, |acc| acc.nonce),
            |acc| acc.nonce,
        ))
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
    use novovm_adapter_api::EvmAccessListEntryV1;
    use novovm_adapter_evm_core::{
        active_precompile_set_m0, apply_eip3529_gas_refund_m0, derive_create2_contract_address_m0,
        estimate_access_list_execution_warm_savings_m0, estimate_calldata_floor_gas_m0,
        estimate_eip2929_storage_read_gas_m0, estimate_eip2929_storage_read_sequence_gas_m0,
        estimate_eip3529_sstore_transition_gas_m0, estimate_intrinsic_gas_m0,
        estimate_intrinsic_gas_with_access_list_rules_m0,
        estimate_intrinsic_gas_with_envelope_extras_m0, recover_raw_evm_tx_sender_m0,
        resolve_evm_profile, translate_raw_evm_tx_fields_m0, tx_ir_from_raw_fields_m0,
        validate_tx_semantics_m0, EvmStorageAccessKeyM0,
    };
    use novovm_exec::{
        AoemTxExecutionAnchorV1, AoemTxExecutionArtifactV1, AOEM_LOG_BLOOM_BYTES_V1,
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
            account_id: None,
            fee_owner_account_id: None,
            nonce_owner_account_id: None,
            to: Some(vec![3u8; 20]),
            value: 1,
            gas_limit: 21_000,
            gas_price: 1,
            nonce: 0,
            data: Vec::new(),
            signature: Vec::new(),
            chain_id: 1,
            tx_type,
            execution_policy: Default::default(),
            evm_access_list: Vec::new(),
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

    fn resign_tx(mut tx: TxIR) -> TxIR {
        let seed = [7u8; 32];
        tx.hash = compute_tx_ir_hash(&tx);
        tx.signature = signature_payload_with_seed_v1(&tx, seed);
        tx
    }

    fn fund_sender_for_test(state: &mut StateIR, tx: &TxIR, balance: u128) {
        state.set_account(
            tx.from.clone(),
            AccountState {
                balance,
                nonce: tx.nonce,
                code_hash: None,
                storage_root: vec![0u8; 32],
            },
        );
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

    fn sample_aoem_artifact(
        tx: &TxIR,
        status_ok: bool,
        state_root: [u8; 32],
        contract_address: Option<Vec<u8>>,
    ) -> AoemTxExecutionArtifactV1 {
        let event_logs = if status_ok {
            vec![AoemEventLogV1 {
                emitter: tx.to.clone().unwrap_or_else(|| tx.from.clone()),
                topics: vec![[0xabu8; 32]],
                data: vec![0x01, 0x02, 0x03],
                log_index: 0,
            }]
        } else {
            Vec::new()
        };
        AoemTxExecutionArtifactV1 {
            tx_index: 0,
            tx_hash: NovoVmAdapter::tx_hash_or_compute(tx),
            status_ok,
            gas_used: if status_ok { tx.gas_limit } else { 0 },
            cumulative_gas_used: if status_ok { tx.gas_limit } else { 0 },
            state_root,
            contract_address,
            receipt_type: Some(2),
            effective_gas_price: Some(tx.gas_price),
            runtime_code: if status_ok && tx.tx_type == TxType::ContractDeploy {
                Some(vec![0x60, 0x00, 0x60, 0x01])
            } else {
                None
            },
            runtime_code_hash: if status_ok && tx.tx_type == TxType::ContractDeploy {
                Some(vec![0x77; 32])
            } else {
                None
            },
            event_logs,
            log_bloom: if status_ok {
                vec![0x55; 256]
            } else {
                vec![0u8; 256]
            },
            revert_data: if status_ok {
                None
            } else {
                Some(vec![0xde, 0xad])
            },
            anchor: Some(AoemTxExecutionAnchorV1 {
                op_index: Some(0),
                processed_ops: 1,
                success_ops: u32::from(status_ok),
                failed_index: if status_ok { None } else { Some(0) },
                total_writes: 1,
                elapsed_us: 7,
                return_code: 0,
                return_code_name: "ok".to_string(),
            }),
        }
    }

    fn sample_eip7610_rejected_mainnet_address_v1() -> Vec<u8> {
        vec![
            0x02, 0x82, 0x0e, 0x4b, 0xee, 0x48, 0x8c, 0x40, 0xf7, 0x45, 0x5f, 0xdc, 0xa5, 0x31,
            0x25, 0x56, 0x51, 0x48, 0x70, 0x8f,
        ]
    }

    fn runtime_code_emit_single_log(topic0: [u8; 32], data_word: [u8; 32]) -> Vec<u8> {
        let mut code = Vec::new();
        code.push(0x7f);
        code.extend_from_slice(&data_word);
        code.push(0x60);
        code.push(0x00);
        code.push(0x52);
        code.push(0x7f);
        code.extend_from_slice(&topic0);
        code.push(0x60);
        code.push(0x20);
        code.push(0x60);
        code.push(0x00);
        code.push(0xa1);
        code.push(0x00);
        code
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

    fn decode_hex_u128(hex: &str) -> u128 {
        let normalized = hex.trim_start_matches("0x");
        if normalized.is_empty() {
            return 0;
        }
        u128::from_str_radix(normalized, 16).expect("hex u128")
    }

    fn json_str<'a>(value: &'a serde_json::Value, field: &str) -> &'a str {
        value
            .as_str()
            .unwrap_or_else(|| panic!("{field} must be a JSON string"))
    }

    fn json_hex_u128(value: &serde_json::Value, field: &str) -> u128 {
        decode_hex_u128(json_str(value, field))
    }

    fn official_eip1559_sender_balance_state_fixture_v1() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../tests/fixtures/ethereum-official-state-subset/senderBalance.json"
        ))
        .expect("decode official EIP-1559 sender balance state fixture")
    }

    fn official_storage_costs_warm_cold_state_fixture_v1() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../tests/fixtures/ethereum-official-state-subset/storageCosts-warm-cold.json"
        ))
        .expect("decode official storage costs warm/cold state fixture")
    }

    fn official_sstore_refund_cap_state_fixture_v1() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../tests/fixtures/ethereum-official-state-subset/sstore-refund-cap.json"
        ))
        .expect("decode official SSTORE refund cap state fixture")
    }

    fn official_failure_account_state_fixture_v1() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../tests/fixtures/ethereum-official-state-subset/failure-account.json"
        ))
        .expect("decode official failure/account state fixture")
    }

    fn official_create_account_state_fixture_v1() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../tests/fixtures/ethereum-official-state-subset/create-account.json"
        ))
        .expect("decode official CREATE/account state fixture")
    }

    fn official_staticcall_precompile_return_state_fixture_v1() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../tests/fixtures/ethereum-official-state-subset/staticcall-precompile-return.json"
        ))
        .expect("decode official STATICCALL/precompile/return-data state fixture")
    }

    fn official_log_receipt_state_fixture_v1() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../tests/fixtures/ethereum-official-state-subset/log-receipt.json"
        ))
        .expect("decode official LOG/receipt state fixture")
    }

    fn first_official_state_fixture_case_v1(fixture: &serde_json::Value) -> &serde_json::Value {
        fixture
            .as_object()
            .expect("official state fixture object")
            .values()
            .next()
            .expect("official state fixture case")
    }

    fn test_rlp_encode_len(prefix_small: u8, prefix_long: u8, len: usize) -> Vec<u8> {
        if len <= 55 {
            return vec![prefix_small + (len as u8)];
        }
        let mut len_bytes = Vec::new();
        let mut n = len;
        while n > 0 {
            len_bytes.push((n & 0xff) as u8);
            n >>= 8;
        }
        len_bytes.reverse();
        let mut out = vec![prefix_long + (len_bytes.len() as u8)];
        out.extend_from_slice(&len_bytes);
        out
    }

    fn test_rlp_encode_bytes(bytes: &[u8]) -> Vec<u8> {
        if bytes.len() == 1 && bytes[0] < 0x80 {
            return vec![bytes[0]];
        }
        let mut out = test_rlp_encode_len(0x80, 0xb7, bytes.len());
        out.extend_from_slice(bytes);
        out
    }

    fn test_rlp_encode_u64(v: u64) -> Vec<u8> {
        if v == 0 {
            return test_rlp_encode_bytes(&[]);
        }
        let bytes = v.to_be_bytes();
        let first_non_zero = bytes
            .iter()
            .position(|b| *b != 0)
            .unwrap_or(bytes.len() - 1);
        test_rlp_encode_bytes(&bytes[first_non_zero..])
    }

    fn test_rlp_encode_u128(v: u128) -> Vec<u8> {
        if v == 0 {
            return test_rlp_encode_bytes(&[]);
        }
        let bytes = v.to_be_bytes();
        let first_non_zero = bytes
            .iter()
            .position(|b| *b != 0)
            .unwrap_or(bytes.len() - 1);
        test_rlp_encode_bytes(&bytes[first_non_zero..])
    }

    fn test_rlp_encode_list(items: &[Vec<u8>]) -> Vec<u8> {
        let payload_len: usize = items.iter().map(Vec::len).sum();
        let mut out = test_rlp_encode_len(0xc0, 0xf7, payload_len);
        for item in items {
            out.extend_from_slice(item);
        }
        out
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum EquivalenceGasDiffClassV1 {
        Exact,
        OverEstimated,
        UnderEstimated,
        MissingTarget,
    }

    fn classify_gas_diff_v1(actual: u64, target: Option<u64>) -> EquivalenceGasDiffClassV1 {
        match target {
            Some(expected) if actual == expected => EquivalenceGasDiffClassV1::Exact,
            Some(expected) if actual > expected => EquivalenceGasDiffClassV1::OverEstimated,
            Some(_) => EquivalenceGasDiffClassV1::UnderEstimated,
            None => EquivalenceGasDiffClassV1::MissingTarget,
        }
    }

    #[derive(Debug, Clone)]
    struct EquivalenceCaseV1 {
        name: &'static str,
        tx: TxIR,
        artifact: Option<AoemTxExecutionArtifactV1>,
        expected_execution_ok: bool,
        expected_status_ok: Option<bool>,
        expected_revert_data: Option<bool>,
        expected_contract_address: Option<bool>,
        expected_logs_present: Option<bool>,
        expected_failure_class: Option<&'static str>,
        expected_failure_class_source: Option<&'static str>,
        expected_failure_recoverability: Option<&'static str>,
        expected_failure_contract: Option<&'static str>,
        target_gas_used: Option<u64>,
        expected_cumulative_gas_used: Option<u64>,
        expected_gas_diff_class: Option<EquivalenceGasDiffClassV1>,
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
            crate::bincode_compat::deserialize(&tx.data).expect("decode stealth payload");
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
    fn verify_raw_evm_signature_path_rejects_non_evm_chain_profile() {
        let raw_tx = decode_hex_bytes("0xf86c018502540be40082520894aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa88016345785d8a00008026a0b7e8e4a6c7d58a47f6d29b6cb16f1c7f5c8a7f7ec5b9fa7a1d8c19f6d8f2b87a02a6d2f8c8f42d8f6d8909f94b6f6a6a4d9f7f1c7b6a5d4e3f2c1b0a99887766");
        let Some(sender) = recover_raw_evm_tx_sender_m0(&raw_tx)
            .expect("raw sender recovery path should not error")
        else {
            return;
        };
        let fields = translate_raw_evm_tx_fields_m0(&raw_tx).expect("decode raw tx fields");
        let tx = tx_ir_from_raw_fields_m0(&fields, &raw_tx, sender, 1);
        assert!(
            !verify_evm_raw_tx_signature_v1(ChainType::NovoVM, &tx).expect("verify path"),
            "raw evm signature path must reject non-EVM chain profile"
        );
    }

    #[test]
    fn tx_intrinsic_gas_includes_type1_access_list_extras_v1() {
        let to = vec![0x55u8; 20];
        let access_list = test_rlp_encode_list(&[
            test_rlp_encode_list(&[
                test_rlp_encode_bytes(&[0x10; 20]),
                test_rlp_encode_list(&[
                    test_rlp_encode_bytes(&[0x01; 32]),
                    test_rlp_encode_bytes(&[0x02; 32]),
                ]),
            ]),
            test_rlp_encode_list(&[
                test_rlp_encode_bytes(&[0x20; 20]),
                test_rlp_encode_list(&[test_rlp_encode_bytes(&[0x03; 32])]),
            ]),
        ]);
        let payload = test_rlp_encode_list(&[
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(8),
            test_rlp_encode_u64(2),
            test_rlp_encode_u64(30_500),
            test_rlp_encode_bytes(&to),
            test_rlp_encode_u128(3),
            test_rlp_encode_bytes(&[0xaa]),
            access_list,
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(1),
        ]);
        let mut raw = vec![0x01u8];
        raw.extend_from_slice(&payload);
        let fields = translate_raw_evm_tx_fields_m0(&raw).expect("type1 raw decode");
        let tx = tx_ir_from_raw_fields_m0(&fields, &raw, vec![0x77; 20], 1);
        let intrinsic = NovoVmAdapter::tx_intrinsic_gas_with_envelope_extras_v1(&tx);
        let expected = estimate_intrinsic_gas_with_envelope_extras_m0(&tx, 2, 3, 0);
        assert_eq!(
            intrinsic, expected,
            "type1 intrinsic extras must match core estimator"
        );
        assert!(
            intrinsic > NovoVmAdapter::tx_intrinsic_gas_v1(&tx),
            "type1 intrinsic should exceed plain intrinsic when access list is present"
        );
    }

    #[test]
    fn execute_raw_type1_access_list_emits_declared_storage_reads_v1() {
        let to = vec![0x55u8; 20];
        let access_address = vec![0x10u8; 20];
        let access_slot = vec![0x01u8; 32];
        let access_list = test_rlp_encode_list(&[test_rlp_encode_list(&[
            test_rlp_encode_bytes(&access_address),
            test_rlp_encode_list(&[test_rlp_encode_bytes(&access_slot)]),
        ])]);
        let payload = test_rlp_encode_list(&[
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(0),
            test_rlp_encode_u64(2),
            test_rlp_encode_u64(100_000),
            test_rlp_encode_bytes(&to),
            test_rlp_encode_u128(3),
            test_rlp_encode_bytes(&[0xaa]),
            access_list,
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(1),
        ]);
        let mut raw = vec![0x01u8];
        raw.extend_from_slice(&payload);
        let fields = translate_raw_evm_tx_fields_m0(&raw).expect("type1 raw decode");
        let tx = tx_ir_from_raw_fields_m0(&fields, &raw, vec![0x77; 20], 1);
        assert_eq!(tx.evm_access_list.len(), 1);

        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");
        adapter.verified_tx_cache.insert(tx.hash.clone());
        let mut runtime_state = StateIR::new();
        let outcome = adapter
            .execute_transaction_with_observed_metadata_v1(&tx, &mut runtime_state, None)
            .expect("execute raw type1 access list");
        let block_access_list = outcome
            .observed_block_access_list
            .block_access_list
            .expect("observed block access list");
        let access_entry = block_access_list
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == access_address.as_slice())
            .expect("declared access-list account read");
        assert_eq!(access_entry.storage_reads, vec![[0x01u8; 32]]);
    }

    #[test]
    fn evm_execution_spec_access_list_warm_storage_smoke_v1() {
        let mut tx = sample_tx(TxType::ContractCall);
        tx.gas_limit = 120_000;
        let target = tx.to.clone().expect("target");
        let mut target20 = [0u8; 20];
        target20.copy_from_slice(&target);
        let declared_read_slot = [0x44u8; 32];
        tx.evm_access_list = vec![EvmAccessListEntryV1 {
            address: target.clone(),
            storage_keys: vec![declared_read_slot.to_vec()],
        }];
        let tx = resign_tx(tx);

        let cold_read = estimate_eip2929_storage_read_gas_m0(1, 0);
        let warm_read = estimate_eip2929_storage_read_gas_m0(1, 1);
        assert_eq!(cold_read - warm_read, 2_000);
        assert_eq!(estimate_access_list_execution_warm_savings_m0(1, 1), 4_500);
        let declared_access_key = EvmStorageAccessKeyM0 {
            address: target20,
            slot: declared_read_slot,
        };
        let cold_access_key = EvmStorageAccessKeyM0 {
            address: target20,
            slot: [0x45u8; 32],
        };
        assert_eq!(
            estimate_eip2929_storage_read_sequence_gas_m0(
                &[declared_access_key],
                &[declared_access_key, declared_access_key, cold_access_key]
            ),
            2 * 100 + 2_100
        );

        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "access-list-warm-storage-smoke".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");
        let mut runtime_state = StateIR::new();
        fund_sender_for_test(&mut runtime_state, &tx, 1_000_000);

        let outcome = adapter
            .execute_transaction_with_observed_metadata_v1(&tx, &mut runtime_state, None)
            .expect("execute access-list warm storage call");
        assert!(outcome.artifact.status_ok);
        assert!(
            outcome
                .observed_block_access_list
                .block_access_list_complete
        );
        let block_access_list = outcome
            .observed_block_access_list
            .block_access_list
            .expect("observed block access list");
        let target_entry = block_access_list
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == target.as_slice())
            .expect("target entry");
        assert!(
            target_entry.storage_reads.contains(&declared_read_slot),
            "access-list declared storage key must remain observable as a warm read"
        );

        let write_slot =
            NovoVmAdapter::bytes_to_bal_word32_v1(b"novovm-bal-slot-v1", &tx.nonce.to_le_bytes());
        assert!(
            target_entry
                .storage_changes
                .iter()
                .any(|change| change.slot == write_slot),
            "contract call SSTORE slot must remain observable as a write"
        );
        assert!(
            target_entry
                .storage_changes
                .iter()
                .all(|change| change.slot != declared_read_slot),
            "declared warm read must not be collapsed into the SSTORE write slot"
        );
    }

    #[test]
    fn evm_execution_spec_sload_warm_cold_fee_debit_v1() {
        let mut tx = sample_tx(TxType::ContractCall);
        tx.value = 0;
        tx.gas_limit = 120_000;
        tx.gas_price = 5;
        let target = tx.to.clone().expect("target");
        let mut target20 = [0u8; 20];
        target20.copy_from_slice(&target);
        let warm_slot = [0x46u8; 32];
        let cold_slot = [0x47u8; 32];
        tx.evm_access_list = vec![EvmAccessListEntryV1 {
            address: target.clone(),
            storage_keys: vec![warm_slot.to_vec()],
        }];
        tx = resign_tx(tx);

        let warm_key = EvmStorageAccessKeyM0 {
            address: target20,
            slot: warm_slot,
        };
        let cold_key = EvmStorageAccessKeyM0 {
            address: target20,
            slot: cold_slot,
        };
        let sload_sequence_gas = estimate_eip2929_storage_read_sequence_gas_m0(
            &[warm_key],
            &[warm_key, warm_key, cold_key, cold_key],
        );
        let all_cold_sequence_gas = estimate_eip2929_storage_read_sequence_gas_m0(
            &[],
            &[warm_key, warm_key, cold_key, cold_key],
        );
        let count_only_cold_gas = estimate_eip2929_storage_read_gas_m0(4, 0);
        assert_eq!(sload_sequence_gas, 2_400);
        assert_eq!(all_cold_sequence_gas, 4_400);
        assert_eq!(count_only_cold_gas, 8_400);

        let intrinsic_with_access_list =
            estimate_intrinsic_gas_with_access_list_rules_m0(&tx, 1, 1, false);
        let expected_gas_used = intrinsic_with_access_list.saturating_add(sload_sequence_gas);
        let all_cold_gas_used = intrinsic_with_access_list.saturating_add(all_cold_sequence_gas);
        let count_only_cold_gas_used =
            intrinsic_with_access_list.saturating_add(count_only_cold_gas);
        assert!(expected_gas_used < all_cold_gas_used);
        assert!(all_cold_gas_used < count_only_cold_gas_used);

        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "sload-warm-cold-fee-debit".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");
        let mut runtime_state = StateIR::new();
        let initial_balance = 1_000_000u128;
        fund_sender_for_test(&mut runtime_state, &tx, initial_balance);

        let mut artifact = sample_aoem_artifact(&tx, true, [0x76; 32], None);
        artifact.gas_used = expected_gas_used;
        artifact.cumulative_gas_used = expected_gas_used;
        artifact.effective_gas_price = Some(tx.gas_price);
        let expected_fee = (expected_gas_used as u128) * (tx.gas_price as u128);
        let all_cold_fee = (all_cold_gas_used as u128) * (tx.gas_price as u128);
        let count_only_cold_fee = (count_only_cold_gas_used as u128) * (tx.gas_price as u128);

        let outcome = adapter
            .execute_transaction_with_observed_metadata_v1(&tx, &mut runtime_state, Some(&artifact))
            .expect("execute SLOAD warm/cold fee debit call");

        assert!(outcome.artifact.status_ok);
        assert_eq!(outcome.artifact.gas_used, expected_gas_used);
        assert_eq!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(initial_balance - expected_fee)
        );
        assert_ne!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(initial_balance - all_cold_fee),
            "fee debit must not treat an access-list warm SLOAD as cold"
        );
        assert_ne!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(initial_balance - count_only_cold_fee),
            "fee debit must not use count-only cold SLOAD gas"
        );
        let block_access_list = outcome
            .observed_block_access_list
            .block_access_list
            .expect("SLOAD warm/cold observed BAL");
        let sender_entry = block_access_list
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == tx.from.as_slice())
            .expect("sender BAL entry");
        assert_eq!(
            sender_entry.balance_changes[0].post_balance,
            NovoVmAdapter::u128_to_be32_v1(initial_balance - expected_fee)
        );
        let target_entry = block_access_list
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == target.as_slice())
            .expect("target BAL entry");
        assert!(
            target_entry.storage_reads.contains(&warm_slot),
            "access-list warm slot must stay observable as a storage read"
        );
    }

    #[test]
    fn tx_intrinsic_gas_includes_type3_blob_extras_v1() {
        let key = "NOVOVM_EVM_ENABLE_TYPE3_WRITE_CHAIN_1";
        let captured = std::env::var(key).ok();
        std::env::set_var(key, "1");

        let to = vec![0x44u8; 20];
        let payload = test_rlp_encode_list(&[
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(9),
            test_rlp_encode_u64(2),
            test_rlp_encode_u64(30),
            test_rlp_encode_u64(50_000),
            test_rlp_encode_bytes(&to),
            test_rlp_encode_u128(4),
            test_rlp_encode_bytes(&[]),
            test_rlp_encode_list(&[]),
            test_rlp_encode_u64(7),
            test_rlp_encode_list(&[
                test_rlp_encode_bytes(&[0x11; 32]),
                test_rlp_encode_bytes(&[0x22; 32]),
            ]),
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(1),
        ]);
        let mut raw = vec![0x03u8];
        raw.extend_from_slice(&payload);

        let fields = translate_raw_evm_tx_fields_m0(&raw).expect("type3 raw decode");
        let tx = tx_ir_from_raw_fields_m0(&fields, &raw, vec![0x88; 20], 1);
        let intrinsic = NovoVmAdapter::tx_intrinsic_gas_with_envelope_extras_v1(&tx);
        let expected = estimate_intrinsic_gas_with_envelope_extras_m0(&tx, 0, 0, 2);
        assert_eq!(
            intrinsic, expected,
            "type3 intrinsic extras must include blob gas"
        );
        assert!(
            intrinsic > NovoVmAdapter::tx_intrinsic_gas_v1(&tx),
            "type3 intrinsic should exceed plain intrinsic when blob hashes are present"
        );

        if let Some(value) = captured {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn typed_type2_semantics_reject_priority_fee_above_max_fee_v1() {
        let to = vec![0x44u8; 20];
        let payload = test_rlp_encode_list(&[
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(0),
            test_rlp_encode_u64(6),
            test_rlp_encode_u64(5),
            test_rlp_encode_u64(21_000),
            test_rlp_encode_bytes(&to),
            test_rlp_encode_u128(4),
            test_rlp_encode_bytes(&[]),
            test_rlp_encode_list(&[]),
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(1),
        ]);
        let mut raw = vec![0x02u8];
        raw.extend_from_slice(&payload);

        let fields = translate_raw_evm_tx_fields_m0(&raw).expect("decode type2 raw tx");
        let tx = tx_ir_from_raw_fields_m0(&fields, &raw, vec![0x88; 20], 1);
        let profile = resolve_evm_profile(ChainType::EVM, 1).expect("resolve evm profile");
        let err = validate_tx_semantics_m0(&profile, &tx)
            .expect_err("must reject priority fee above max fee");
        assert!(err
            .to_string()
            .contains("max_priority_fee_per_gas exceeds max_fee_per_gas"));
    }

    #[test]
    fn typed_type2_semantics_reject_intrinsic_gas_too_low_v1() {
        let to = vec![0x44u8; 20];
        let payload = test_rlp_encode_list(&[
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(0),
            test_rlp_encode_u64(2),
            test_rlp_encode_u64(30),
            test_rlp_encode_u64(21_000),
            test_rlp_encode_bytes(&to),
            test_rlp_encode_u128(4),
            test_rlp_encode_bytes(&[0xaa]),
            test_rlp_encode_list(&[]),
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(1),
        ]);
        let mut raw = vec![0x02u8];
        raw.extend_from_slice(&payload);

        let fields = translate_raw_evm_tx_fields_m0(&raw).expect("decode type2 raw tx");
        let tx = tx_ir_from_raw_fields_m0(&fields, &raw, vec![0x88; 20], 1);
        let profile = resolve_evm_profile(ChainType::EVM, 1).expect("resolve evm profile");
        let err =
            validate_tx_semantics_m0(&profile, &tx).expect_err("must reject intrinsic gas low");
        assert!(err.to_string().contains("intrinsic gas too low"));
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

    #[test]
    fn execute_transaction_with_artifact_prefers_contract_deploy_address_and_root() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let tx = sample_tx(TxType::ContractDeploy);
        assert!(adapter.verify_transaction(&tx).expect("verify deploy"));
        let mut runtime_state = StateIR::new();
        let contract_address = vec![0x44; 20];
        let artifact = sample_aoem_artifact(&tx, true, [0x33; 32], Some(contract_address.clone()));

        adapter
            .execute_transaction_with_artifact(&tx, &mut runtime_state, Some(&artifact))
            .expect("execute with artifact");

        let deployed = runtime_state
            .get_account(&contract_address)
            .cloned()
            .expect("deployed account");
        assert_eq!(deployed.code_hash, Some(vec![0x77; 32]));
        assert_eq!(runtime_state.state_root, [0x33; 32].to_vec());
        assert_eq!(
            adapter.state_root().expect("state root"),
            [0x33; 32].to_vec()
        );
        let stored = adapter
            .read_state(&NovoVmAdapter::aoem_tx_artifact_key(&artifact.tx_hash))
            .expect("read artifact key");
        assert!(stored.is_some());
        assert_eq!(
            runtime_state.get_storage(&contract_address, b"deploy:runtime_code"),
            Some(&vec![0x60, 0x00, 0x60, 0x01])
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"aoem:last_log_bloom"),
            Some(&vec![0x55; 256])
        );
    }

    #[test]
    fn execute_transaction_with_observed_metadata_emits_complete_transfer_evm_bal() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let tx = sample_tx(TxType::Transfer);
        let mut runtime_state = StateIR::new();
        let outcome = adapter
            .execute_transaction_with_observed_metadata_v1(&tx, &mut runtime_state, None)
            .expect("execute with observed metadata");

        assert!(outcome.artifact.status_ok);
        assert!(
            outcome
                .observed_block_access_list
                .block_access_list_complete
        );
        let block_access_list = outcome
            .observed_block_access_list
            .block_access_list
            .expect("observed block access list");
        assert_eq!(block_access_list.0.len(), 2);
        let sender_entry = block_access_list
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == tx.from.as_slice())
            .expect("sender entry");
        assert_eq!(sender_entry.nonce_changes.len(), 1);
        assert_eq!(sender_entry.nonce_changes[0].post_nonce, 1);
        let recipient = tx.to.clone().expect("recipient");
        let recipient_entry = block_access_list
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == recipient.as_slice())
            .expect("recipient entry");
        assert_eq!(recipient_entry.balance_changes.len(), 1);
    }

    #[test]
    fn execute_transfer_debits_tracked_sender_value_and_fee_v1() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let mut tx = sample_tx(TxType::Transfer);
        tx.value = 9;
        tx.gas_price = 3;
        tx = resign_tx(tx);
        let mut runtime_state = StateIR::new();
        let initial_balance = 100_000u128;
        fund_sender_for_test(&mut runtime_state, &tx, initial_balance);
        let expected_fee = NovoVmAdapter::execution_fee_wei_v1(&tx, None, true);
        let expected_sender_balance = initial_balance - tx.value - expected_fee;

        let outcome = adapter
            .execute_transaction_with_observed_metadata_v1(&tx, &mut runtime_state, None)
            .expect("execute funded transfer");

        assert_eq!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(expected_sender_balance)
        );
        assert_eq!(
            adapter
                .get_balance(&tx.from)
                .expect("adapter sender balance"),
            expected_sender_balance
        );
        assert_eq!(
            runtime_state
                .get_account(tx.to.as_ref().expect("recipient"))
                .map(|acc| acc.balance),
            Some(tx.value)
        );
        let block_access_list = outcome
            .observed_block_access_list
            .block_access_list
            .expect("observed block access list");
        let sender_entry = block_access_list
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == tx.from.as_slice())
            .expect("sender entry");
        assert_eq!(sender_entry.balance_changes.len(), 1);
        assert_eq!(
            sender_entry.balance_changes[0].post_balance,
            NovoVmAdapter::u128_to_be32_v1(expected_sender_balance)
        );
    }

    #[test]
    fn execute_failed_call_debits_fee_without_value_transfer_v1() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let mut tx = sample_tx(TxType::ContractCall);
        tx.value = 11;
        tx.gas_limit = 120_000;
        tx.gas_price = 2;
        tx = resign_tx(tx);
        let target = tx.to.clone().expect("call target");
        let mut runtime_state = StateIR::new();
        let initial_balance = 100_000u128;
        fund_sender_for_test(&mut runtime_state, &tx, initial_balance);
        runtime_state.set_account(
            target.clone(),
            AccountState {
                balance: 5,
                nonce: 0,
                code_hash: None,
                storage_root: vec![0u8; 32],
            },
        );

        let mut artifact = sample_aoem_artifact(&tx, false, [0x55; 32], None);
        artifact.gas_used = 25_000;
        artifact.cumulative_gas_used = 25_000;
        artifact.effective_gas_price = Some(tx.gas_price);
        let expected_fee = NovoVmAdapter::execution_fee_wei_v1(&tx, Some(&artifact), false);
        let expected_sender_balance = initial_balance - expected_fee;

        let resolved = adapter
            .execute_transaction_with_artifact(&tx, &mut runtime_state, Some(&artifact))
            .expect("execute failed call artifact");

        assert!(!resolved.status_ok);
        assert_eq!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(expected_sender_balance)
        );
        assert_eq!(
            runtime_state.get_account(&target).map(|acc| acc.balance),
            Some(5),
            "failed call must not transfer value"
        );
    }

    #[test]
    fn execute_success_call_debits_refunded_sstore_gas_used_v1() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let mut tx = sample_tx(TxType::ContractCall);
        tx.value = 0;
        tx.gas_limit = 120_000;
        tx.gas_price = 3;
        tx = resign_tx(tx);
        let target = tx.to.clone().expect("call target");
        let mut runtime_state = StateIR::new();
        let initial_balance = 100_000u128;
        fund_sender_for_test(&mut runtime_state, &tx, initial_balance);
        runtime_state.set_storage(target.clone(), b"clear-me".to_vec(), vec![0x01]);

        let transition =
            estimate_eip3529_sstore_transition_gas_m0([0x01u8; 32], [0x01u8; 32], [0u8; 32], false);
        assert_eq!(transition.gas_cost, 5_000);
        assert_eq!(transition.refund_delta, 4_800);
        let pre_refund_gas_used =
            estimate_intrinsic_gas_m0(&tx).saturating_add(transition.gas_cost);
        let refund_counter = transition.refund_delta.max(0) as u64;
        let refunded_gas_used = apply_eip3529_gas_refund_m0(pre_refund_gas_used, refund_counter);
        assert_eq!(refunded_gas_used, pre_refund_gas_used.saturating_sub(4_800));

        let mut artifact = sample_aoem_artifact(&tx, true, [0x56; 32], None);
        artifact.gas_used = refunded_gas_used;
        artifact.cumulative_gas_used = refunded_gas_used;
        artifact.effective_gas_price = Some(tx.gas_price);
        let expected_fee = (refunded_gas_used as u128) * (tx.gas_price as u128);
        let unrefunded_fee = (pre_refund_gas_used as u128) * (tx.gas_price as u128);

        let resolved = adapter
            .execute_transaction_with_artifact(&tx, &mut runtime_state, Some(&artifact))
            .expect("execute refunded gas call artifact");

        assert!(resolved.status_ok);
        assert_eq!(resolved.gas_used, refunded_gas_used);
        assert_eq!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(initial_balance - expected_fee)
        );
        assert!(
            expected_fee < unrefunded_fee,
            "SSTORE clear refund must reduce charged gas before fee debit"
        );
    }

    #[test]
    fn evm_execution_spec_sstore_refund_cap_fee_debit_v1() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "sstore-refund-cap-fee-debit".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let mut tx = sample_tx(TxType::ContractCall);
        tx.value = 0;
        tx.gas_limit = 120_000;
        tx.gas_price = 4;
        tx = resign_tx(tx);
        let mut runtime_state = StateIR::new();
        let initial_balance = 1_000_000u128;
        fund_sender_for_test(&mut runtime_state, &tx, initial_balance);

        let pre_refund_gas_used = 24_000u64;
        let refund_counter = 19_200u64;
        let capped_refunded_gas_used =
            apply_eip3529_gas_refund_m0(pre_refund_gas_used, refund_counter);
        let uncapped_refunded_gas_used = pre_refund_gas_used.saturating_sub(refund_counter);
        assert_eq!(capped_refunded_gas_used, 19_200);
        assert_eq!(uncapped_refunded_gas_used, 4_800);
        assert!(
            capped_refunded_gas_used > uncapped_refunded_gas_used,
            "EIP-3529 refund cap must prevent over-refund"
        );

        let mut artifact = sample_aoem_artifact(&tx, true, [0x75; 32], None);
        artifact.gas_used = capped_refunded_gas_used;
        artifact.cumulative_gas_used = capped_refunded_gas_used;
        artifact.effective_gas_price = Some(tx.gas_price);
        let expected_capped_fee = (capped_refunded_gas_used as u128) * (tx.gas_price as u128);
        let uncapped_fee = (uncapped_refunded_gas_used as u128) * (tx.gas_price as u128);
        assert_eq!(expected_capped_fee, 76_800);
        assert_eq!(uncapped_fee, 19_200);

        let outcome = adapter
            .execute_transaction_with_observed_metadata_v1(&tx, &mut runtime_state, Some(&artifact))
            .expect("execute capped refund call artifact");

        assert!(outcome.artifact.status_ok);
        assert_eq!(outcome.artifact.gas_used, capped_refunded_gas_used);
        assert_eq!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(initial_balance - expected_capped_fee)
        );
        assert_ne!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(initial_balance - uncapped_fee),
            "fee debit must not use uncapped refund"
        );
        let block_access_list = outcome
            .observed_block_access_list
            .block_access_list
            .expect("capped refund observed BAL");
        let sender_entry = block_access_list
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == tx.from.as_slice())
            .expect("sender BAL entry");
        assert_eq!(
            sender_entry.balance_changes[0].post_balance,
            NovoVmAdapter::u128_to_be32_v1(initial_balance - expected_capped_fee)
        );
    }

    #[test]
    fn evm_execution_spec_account_balance_value_fee_invariants_v1() {
        let mut call_adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "account-balance-call".to_string(),
            enabled: true,
            custom_config: None,
        });
        call_adapter.initialize().expect("init call adapter");

        let mut call_tx = sample_tx(TxType::ContractCall);
        call_tx.value = 13;
        call_tx.gas_limit = 120_000;
        call_tx.gas_price = 4;
        call_tx = resign_tx(call_tx);
        let call_target = call_tx.to.clone().expect("call target");
        let mut call_state = StateIR::new();
        let initial_call_sender_balance = 1_000_000u128;
        let initial_call_target_balance = 7u128;
        fund_sender_for_test(&mut call_state, &call_tx, initial_call_sender_balance);
        call_state.set_account(
            call_target.clone(),
            AccountState {
                balance: initial_call_target_balance,
                nonce: 0,
                code_hash: None,
                storage_root: vec![0u8; 32],
            },
        );
        let mut call_artifact = sample_aoem_artifact(&call_tx, true, [0x71; 32], None);
        call_artifact.gas_used = 26_000;
        call_artifact.cumulative_gas_used = 26_000;
        call_artifact.effective_gas_price = Some(call_tx.gas_price);
        let expected_call_fee =
            NovoVmAdapter::execution_fee_wei_v1(&call_tx, Some(&call_artifact), true);
        let expected_call_sender_balance =
            initial_call_sender_balance - call_tx.value - expected_call_fee;
        let expected_call_target_balance = initial_call_target_balance + call_tx.value;

        let call_outcome = call_adapter
            .execute_transaction_with_observed_metadata_v1(
                &call_tx,
                &mut call_state,
                Some(&call_artifact),
            )
            .expect("execute successful call with value");

        assert!(call_outcome.artifact.status_ok);
        assert_eq!(
            call_state.get_account(&call_tx.from).map(|acc| acc.balance),
            Some(expected_call_sender_balance)
        );
        assert_eq!(
            call_state.get_account(&call_target).map(|acc| acc.balance),
            Some(expected_call_target_balance)
        );
        let call_bal = call_outcome
            .observed_block_access_list
            .block_access_list
            .expect("successful call observed BAL");
        let call_sender_entry = call_bal
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == call_tx.from.as_slice())
            .expect("successful call sender BAL entry");
        assert_eq!(
            call_sender_entry.balance_changes[0].post_balance,
            NovoVmAdapter::u128_to_be32_v1(expected_call_sender_balance)
        );
        let call_target_entry = call_bal
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == call_target.as_slice())
            .expect("successful call target BAL entry");
        assert_eq!(
            call_target_entry.balance_changes[0].post_balance,
            NovoVmAdapter::u128_to_be32_v1(expected_call_target_balance)
        );

        let mut deploy_adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "account-balance-create".to_string(),
            enabled: true,
            custom_config: None,
        });
        deploy_adapter.initialize().expect("init deploy adapter");

        let mut deploy_tx = sample_tx(TxType::ContractDeploy);
        deploy_tx.value = 19;
        deploy_tx.gas_limit = 120_000;
        deploy_tx.gas_price = 5;
        deploy_tx = resign_tx(deploy_tx);
        let deploy_contract = vec![0x72u8; 20];
        let mut deploy_state = StateIR::new();
        let initial_deploy_sender_balance = 1_000_000u128;
        fund_sender_for_test(&mut deploy_state, &deploy_tx, initial_deploy_sender_balance);
        let mut deploy_artifact =
            sample_aoem_artifact(&deploy_tx, true, [0x72; 32], Some(deploy_contract.clone()));
        deploy_artifact.gas_used = 41_000;
        deploy_artifact.cumulative_gas_used = 41_000;
        deploy_artifact.effective_gas_price = Some(deploy_tx.gas_price);
        deploy_artifact.runtime_code = Some(vec![0x60, 0x00, 0x60, 0x01]);
        deploy_artifact.runtime_code_hash = Some(vec![0x77; 32]);
        let expected_deploy_fee =
            NovoVmAdapter::execution_fee_wei_v1(&deploy_tx, Some(&deploy_artifact), true);
        let expected_deploy_sender_balance =
            initial_deploy_sender_balance - deploy_tx.value - expected_deploy_fee;

        let deploy_outcome = deploy_adapter
            .execute_transaction_with_observed_metadata_v1(
                &deploy_tx,
                &mut deploy_state,
                Some(&deploy_artifact),
            )
            .expect("execute successful create with value");

        assert!(deploy_outcome.artifact.status_ok);
        assert_eq!(
            deploy_state
                .get_account(&deploy_tx.from)
                .map(|acc| acc.balance),
            Some(expected_deploy_sender_balance)
        );
        let deployed_account = deploy_state
            .get_account(&deploy_contract)
            .expect("deployed contract account");
        assert_eq!(deployed_account.balance, deploy_tx.value);
        assert_eq!(deployed_account.code_hash, Some(vec![0x77; 32]));
        let deploy_bal = deploy_outcome
            .observed_block_access_list
            .block_access_list
            .expect("successful create observed BAL");
        let deploy_sender_entry = deploy_bal
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == deploy_tx.from.as_slice())
            .expect("successful create sender BAL entry");
        assert_eq!(
            deploy_sender_entry.balance_changes[0].post_balance,
            NovoVmAdapter::u128_to_be32_v1(expected_deploy_sender_balance)
        );
        let deploy_contract_entry = deploy_bal
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == deploy_contract.as_slice())
            .expect("successful create contract BAL entry");
        assert_eq!(
            deploy_contract_entry.balance_changes[0].post_balance,
            NovoVmAdapter::u128_to_be32_v1(deploy_tx.value)
        );
        assert_eq!(deploy_contract_entry.code_changes.len(), 1);

        let mut failed_deploy_adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "account-balance-failed-create".to_string(),
            enabled: true,
            custom_config: None,
        });
        failed_deploy_adapter
            .initialize()
            .expect("init failed deploy adapter");

        let mut failed_deploy_tx = sample_tx(TxType::ContractDeploy);
        failed_deploy_tx.value = 23;
        failed_deploy_tx.gas_limit = 120_000;
        failed_deploy_tx.gas_price = 6;
        failed_deploy_tx = resign_tx(failed_deploy_tx);
        let failed_contract = vec![0x73u8; 20];
        let mut failed_deploy_state = StateIR::new();
        let initial_failed_deploy_sender_balance = 1_000_000u128;
        fund_sender_for_test(
            &mut failed_deploy_state,
            &failed_deploy_tx,
            initial_failed_deploy_sender_balance,
        );
        let mut failed_deploy_artifact = sample_aoem_artifact(
            &failed_deploy_tx,
            false,
            [0x73; 32],
            Some(failed_contract.clone()),
        );
        failed_deploy_artifact.revert_data = None;
        failed_deploy_artifact.gas_used = 39_000;
        failed_deploy_artifact.cumulative_gas_used = 39_000;
        failed_deploy_artifact.effective_gas_price = Some(failed_deploy_tx.gas_price);
        if let Some(anchor) = failed_deploy_artifact.anchor.as_mut() {
            anchor.return_code = 13;
            anchor.return_code_name = "out_of_gas".to_string();
        }
        let expected_failed_deploy_fee = NovoVmAdapter::execution_fee_wei_v1(
            &failed_deploy_tx,
            Some(&failed_deploy_artifact),
            false,
        );
        let expected_failed_deploy_sender_balance =
            initial_failed_deploy_sender_balance - expected_failed_deploy_fee;

        let failed_deploy_outcome = failed_deploy_adapter
            .execute_transaction_with_observed_metadata_v1(
                &failed_deploy_tx,
                &mut failed_deploy_state,
                Some(&failed_deploy_artifact),
            )
            .expect("execute failed create with value");

        assert!(!failed_deploy_outcome.artifact.status_ok);
        assert_eq!(
            failed_deploy_state
                .get_account(&failed_deploy_tx.from)
                .map(|acc| acc.balance),
            Some(expected_failed_deploy_sender_balance)
        );
        assert!(
            failed_deploy_state.get_account(&failed_contract).is_none(),
            "failed CREATE must not transfer value into a contract account"
        );
        assert_eq!(
            failed_deploy_state.get_storage(
                &failed_deploy_tx.from,
                b"deploy:last_failed_contract_address"
            ),
            Some(&failed_contract)
        );
        let failed_deploy_bal = failed_deploy_outcome
            .observed_block_access_list
            .block_access_list
            .expect("failed create observed BAL");
        let failed_deploy_sender_entry = failed_deploy_bal
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == failed_deploy_tx.from.as_slice())
            .expect("failed create sender BAL entry");
        assert_eq!(
            failed_deploy_sender_entry.balance_changes[0].post_balance,
            NovoVmAdapter::u128_to_be32_v1(expected_failed_deploy_sender_balance)
        );
        assert!(
            failed_deploy_bal
                .0
                .iter()
                .all(|entry| entry.address.as_slice() != failed_contract.as_slice()),
            "failed CREATE must not emit a contract BAL account entry"
        );
    }

    #[test]
    fn evm_execution_spec_effective_gas_price_fee_debit_v1() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "effective-gas-price-fee-debit".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let mut tx = sample_tx(TxType::Transfer);
        tx.value = 5;
        tx.gas_limit = 80_000;
        tx.gas_price = 9;
        tx = resign_tx(tx);
        let recipient = tx.to.clone().expect("transfer recipient");
        let mut runtime_state = StateIR::new();
        let initial_sender_balance = 1_000_000u128;
        fund_sender_for_test(&mut runtime_state, &tx, initial_sender_balance);

        let mut artifact = sample_aoem_artifact(&tx, true, [0x74; 32], None);
        artifact.gas_used = 21_000;
        artifact.cumulative_gas_used = 21_000;
        artifact.effective_gas_price = Some(3);
        let expected_effective_fee =
            NovoVmAdapter::execution_fee_wei_v1(&tx, Some(&artifact), true);
        let max_fee_cap_debit = (artifact.gas_used as u128) * (tx.gas_price as u128);
        assert_eq!(expected_effective_fee, 63_000);
        assert_ne!(
            expected_effective_fee, max_fee_cap_debit,
            "EIP-1559 settlement must not charge the max_fee cap when effectiveGasPrice is lower"
        );
        let expected_sender_balance = initial_sender_balance - tx.value - expected_effective_fee;

        let outcome = adapter
            .execute_transaction_with_observed_metadata_v1(&tx, &mut runtime_state, Some(&artifact))
            .expect("execute effective gas price settlement tx");

        assert!(outcome.artifact.status_ok);
        assert_eq!(outcome.artifact.effective_gas_price, Some(3));
        assert_eq!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(expected_sender_balance)
        );
        assert_ne!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(initial_sender_balance - tx.value - max_fee_cap_debit)
        );
        assert_eq!(
            runtime_state.get_account(&recipient).map(|acc| acc.balance),
            Some(tx.value)
        );

        let block_access_list = outcome
            .observed_block_access_list
            .block_access_list
            .expect("effective gas price observed BAL");
        let sender_entry = block_access_list
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == tx.from.as_slice())
            .expect("sender BAL entry");
        assert_eq!(
            sender_entry.balance_changes[0].post_balance,
            NovoVmAdapter::u128_to_be32_v1(expected_sender_balance)
        );
    }

    #[test]
    fn official_state_fixture_eip1559_sender_balance_fee_debit_v1() {
        let fixture = official_eip1559_sender_balance_state_fixture_v1();
        let case = first_official_state_fixture_case_v1(&fixture);
        assert_eq!(case["_info"]["fixture-format"].as_str(), Some("state_test"));
        assert_eq!(case["_info"]["repo"].as_str(), Some("ethereum/tests"));
        assert_eq!(
            case["_info"]["source"].as_str(),
            Some("src/GeneralStateTestsFiller/stEIP1559/senderBalanceFiller.yml")
        );

        let env = &case["env"];
        let transaction = &case["transaction"];
        let post = &case["post"]["Cancun"][0];
        let sender = json_str(&transaction["sender"], "transaction.sender");
        let to = json_str(&transaction["to"], "transaction.to");
        let coinbase = json_str(&env["currentCoinbase"], "env.currentCoinbase");

        let pre_sender_balance =
            json_hex_u128(&case["pre"][sender]["balance"], "pre.sender.balance");
        let post_sender_balance =
            json_hex_u128(&post["state"][sender]["balance"], "post.sender.balance");
        let post_coinbase_balance =
            json_hex_u128(&post["state"][coinbase]["balance"], "post.coinbase.balance");
        let execution_visible_sender_balance =
            json_hex_u128(&post["state"][to]["storage"]["0x00"], "post.to.storage.0");
        let pre_to_balance = json_hex_u128(&case["pre"][to]["balance"], "pre.to.balance");
        let post_to_balance = json_hex_u128(&post["state"][to]["balance"], "post.to.balance");
        let gas_limit = json_hex_u128(&transaction["gasLimit"][0], "transaction.gasLimit[0]");
        let max_fee_per_gas =
            json_hex_u128(&transaction["maxFeePerGas"], "transaction.maxFeePerGas");
        let max_priority_fee_per_gas = json_hex_u128(
            &transaction["maxPriorityFeePerGas"],
            "transaction.maxPriorityFeePerGas",
        );
        let base_fee = json_hex_u128(&env["currentBaseFee"], "env.currentBaseFee");
        let effective_gas_price = max_fee_per_gas.min(base_fee + max_priority_fee_per_gas);

        assert_eq!(effective_gas_price, 111);
        assert_eq!(
            execution_visible_sender_balance,
            pre_sender_balance - gas_limit * effective_gas_price,
            "official state fixture records sender BALANCE after effective-price gas precharge"
        );
        assert_ne!(
            execution_visible_sender_balance,
            pre_sender_balance - gas_limit * max_fee_per_gas,
            "official state fixture must not use maxFeePerGas for execution-visible balance"
        );

        let final_fee = pre_sender_balance - post_sender_balance;
        assert_eq!(final_fee % effective_gas_price, 0);
        let gas_used = final_fee / effective_gas_price;
        assert_eq!(gas_used, 43_205);
        assert_eq!(
            post_coinbase_balance,
            gas_used * max_priority_fee_per_gas,
            "coinbase receives priority fee; base fee is burned"
        );

        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: json_hex_u128(&case["config"]["chainid"], "config.chainid") as u64,
            name: "official-eip1559-sender-balance".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let raw_tx = decode_hex_bytes(json_str(&post["txbytes"], "post.txbytes"));
        let recovered_sender = recover_raw_evm_tx_sender_m0(&raw_tx)
            .expect("official fixture raw sender recovery")
            .expect("official fixture recovered sender");
        assert_eq!(recovered_sender, decode_hex_bytes(sender));
        let fields = translate_raw_evm_tx_fields_m0(&raw_tx).expect("official raw tx decode");
        let tx = tx_ir_from_raw_fields_m0(
            &fields,
            &raw_tx,
            recovered_sender,
            json_hex_u128(&case["config"]["chainid"], "config.chainid") as u64,
        );
        assert_eq!(tx.from, decode_hex_bytes(sender));
        assert_eq!(tx.to, Some(decode_hex_bytes(to)));
        assert_eq!(
            tx.value,
            json_hex_u128(&transaction["value"][0], "transaction.value[0]")
        );
        assert_eq!(tx.gas_limit, gas_limit as u64);
        assert_eq!(tx.gas_price, max_fee_per_gas as u64);
        assert_eq!(
            tx.nonce,
            json_hex_u128(&transaction["nonce"], "transaction.nonce") as u64
        );
        assert_eq!(
            tx.data,
            decode_hex_bytes(json_str(&transaction["data"][0], "transaction.data[0]"))
        );
        assert!(
            adapter
                .verify_transaction(&tx)
                .expect("verify official raw tx"),
            "official state fixture txbytes must verify through the raw EVM path"
        );

        let mut runtime_state = StateIR::new();
        runtime_state.set_account(
            tx.from.clone(),
            AccountState {
                balance: pre_sender_balance,
                nonce: tx.nonce,
                code_hash: None,
                storage_root: vec![0u8; 32],
            },
        );
        runtime_state.set_account(
            tx.to.clone().expect("call target"),
            AccountState {
                balance: pre_to_balance,
                nonce: json_hex_u128(&case["pre"][to]["nonce"], "pre.to.nonce") as u64,
                code_hash: Some(decode_hex_bytes(json_str(
                    &case["pre"][to]["code"],
                    "pre.to.code",
                ))),
                storage_root: vec![0u8; 32],
            },
        );

        let mut artifact = sample_aoem_artifact(&tx, true, [0x82; 32], None);
        artifact.gas_used = gas_used as u64;
        artifact.cumulative_gas_used = gas_used as u64;
        artifact.effective_gas_price = Some(effective_gas_price as u64);
        artifact.event_logs.clear();
        artifact.log_bloom = vec![0u8; 256];

        let outcome = adapter
            .execute_transaction_with_observed_metadata_v1(&tx, &mut runtime_state, Some(&artifact))
            .expect("execute official EIP-1559 sender balance fixture projection");

        assert!(outcome.artifact.status_ok);
        assert_eq!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(post_sender_balance)
        );
        assert_ne!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(pre_sender_balance - gas_used * max_fee_per_gas),
            "SUPERVM adapter must not settle this official fixture using maxFeePerGas"
        );
        assert_eq!(
            runtime_state
                .get_account(tx.to.as_ref().expect("call target"))
                .map(|acc| acc.balance),
            Some(post_to_balance)
        );

        let bal = outcome
            .observed_block_access_list
            .block_access_list
            .expect("official fixture observed BAL");
        let sender_entry = bal
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == tx.from.as_slice())
            .expect("sender BAL entry");
        assert_eq!(
            sender_entry.balance_changes[0].post_balance,
            NovoVmAdapter::u128_to_be32_v1(post_sender_balance)
        );
    }

    #[test]
    fn official_state_fixture_sload_warm_cold_fee_debit_v1() {
        let fixture = official_storage_costs_warm_cold_state_fixture_v1();
        assert_eq!(fixture["source"]["repo"].as_str(), Some("ethereum/tests"));
        assert_eq!(
            fixture["source"]["fixtureFormat"].as_str(),
            Some("state_test")
        );
        assert_eq!(
            fixture["source"]["source"].as_str(),
            Some("src/GeneralStateTestsFiller/stEIP2930/storageCostsFiller.yml")
        );

        let transaction = &fixture["transaction"];
        let sender = json_str(&transaction["sender"], "transaction.sender");
        let to = json_str(&transaction["to"], "transaction.to");
        let pre_sender_balance = json_hex_u128(
            &transaction["preSenderBalance"],
            "transaction.preSenderBalance",
        );
        let pre_to_balance =
            json_hex_u128(&transaction["preToBalance"], "transaction.preToBalance");
        let post_to_balance =
            json_hex_u128(&transaction["postToBalance"], "transaction.postToBalance");
        let gas_price = json_hex_u128(&transaction["gasPrice"], "transaction.gasPrice");
        let gas_limit = json_hex_u128(&transaction["gasLimit"], "transaction.gasLimit");
        let value = json_hex_u128(&transaction["value"], "transaction.value");
        assert_eq!(post_to_balance, pre_to_balance + value);

        let mut warm_gas_used = None;
        let mut cold_gas_used = None;
        let cases = fixture["cases"].as_array().expect("warm/cold cases");
        assert_eq!(cases.len(), 2);

        for case in cases {
            let label = json_str(&case["label"], "case.label");
            let raw_tx = decode_hex_bytes(json_str(&case["txbytes"], "case.txbytes"));
            let recovered_sender = recover_raw_evm_tx_sender_m0(&raw_tx)
                .expect("official storage costs raw sender recovery")
                .expect("official storage costs recovered sender");
            assert_eq!(recovered_sender, decode_hex_bytes(sender));
            let fields = translate_raw_evm_tx_fields_m0(&raw_tx).expect("official type1 decode");
            let tx = tx_ir_from_raw_fields_m0(&fields, &raw_tx, recovered_sender, 1);

            assert_eq!(tx.tx_type, TxType::ContractCall);
            assert_eq!(tx.from, decode_hex_bytes(sender));
            assert_eq!(tx.to, Some(decode_hex_bytes(to)));
            assert_eq!(tx.value, value);
            assert_eq!(tx.gas_limit, gas_limit as u64);
            assert_eq!(tx.gas_price, gas_price as u64);
            assert_eq!(
                tx.data,
                decode_hex_bytes(json_str(&case["data"], "case.data"))
            );
            assert_eq!(tx.evm_access_list.len(), 1);
            assert_eq!(
                tx.evm_access_list[0].address,
                decode_hex_bytes(json_str(
                    &case["accessList"][0]["address"],
                    "accessList.address"
                ))
            );
            assert_eq!(
                tx.evm_access_list[0].storage_keys[0],
                decode_hex_bytes(json_str(
                    &case["accessList"][0]["storageKeys"][0],
                    "accessList.storageKeys[0]"
                ))
            );

            let mut adapter = NovoVmAdapter::new(ChainConfig {
                chain_type: ChainType::EVM,
                chain_id: 1,
                name: format!("official-storage-costs-{label}"),
                enabled: true,
                custom_config: None,
            });
            adapter.initialize().expect("init");
            assert!(
                adapter
                    .verify_transaction(&tx)
                    .expect("verify official storage costs raw tx"),
                "official storageCosts txbytes must verify through the raw EVM path"
            );

            let post_sender_balance =
                json_hex_u128(&case["postSenderBalance"], "case.postSenderBalance");
            let charged_fee = pre_sender_balance
                .saturating_sub(post_sender_balance)
                .saturating_sub(value);
            assert_eq!(charged_fee % gas_price, 0);
            let gas_used = charged_fee / gas_price;
            assert_eq!(
                gas_used,
                case["gasUsed"].as_u64().expect("case.gasUsed") as u128
            );

            let mut runtime_state = StateIR::new();
            runtime_state.set_account(
                tx.from.clone(),
                AccountState {
                    balance: pre_sender_balance,
                    nonce: tx.nonce,
                    code_hash: None,
                    storage_root: vec![0u8; 32],
                },
            );
            runtime_state.set_account(
                tx.to.clone().expect("call target"),
                AccountState {
                    balance: pre_to_balance,
                    nonce: 0,
                    code_hash: Some(vec![0xcc; 32]),
                    storage_root: vec![0u8; 32],
                },
            );

            let mut artifact = sample_aoem_artifact(&tx, true, [0x83; 32], None);
            artifact.gas_used = gas_used as u64;
            artifact.cumulative_gas_used = gas_used as u64;
            artifact.effective_gas_price = Some(gas_price as u64);
            artifact.event_logs.clear();
            artifact.log_bloom = vec![0u8; 256];

            let outcome = adapter
                .execute_transaction_with_observed_metadata_v1(
                    &tx,
                    &mut runtime_state,
                    Some(&artifact),
                )
                .expect("execute official storageCosts fixture projection");

            assert!(outcome.artifact.status_ok);
            assert_eq!(
                runtime_state.get_account(&tx.from).map(|acc| acc.balance),
                Some(post_sender_balance)
            );
            assert_eq!(
                runtime_state
                    .get_account(tx.to.as_ref().expect("call target"))
                    .map(|acc| acc.balance),
                Some(post_to_balance)
            );

            let bal = outcome
                .observed_block_access_list
                .block_access_list
                .expect("official storage costs observed BAL");
            let sender_entry = bal
                .0
                .iter()
                .find(|entry| entry.address.as_slice() == tx.from.as_slice())
                .expect("sender BAL entry");
            assert_eq!(
                sender_entry.balance_changes[0].post_balance,
                NovoVmAdapter::u128_to_be32_v1(post_sender_balance)
            );

            match label {
                "declaredKeyRead" => warm_gas_used = Some(gas_used),
                "undeclaredKeyRead" => cold_gas_used = Some(gas_used),
                _ => panic!("unexpected storageCosts label {label}"),
            }
        }

        let warm_gas_used = warm_gas_used.expect("declaredKeyRead gas used");
        let cold_gas_used = cold_gas_used.expect("undeclaredKeyRead gas used");
        assert_eq!(
            cold_gas_used - warm_gas_used,
            2_000,
            "official storageCosts fixture must preserve EIP-2929 SLOAD warm/cold delta"
        );
    }

    #[test]
    fn official_state_fixture_sstore_refund_cap_fee_debit_v1() {
        let fixture = official_sstore_refund_cap_state_fixture_v1();
        assert_eq!(fixture["source"]["repo"].as_str(), Some("ethereum/tests"));
        assert_eq!(
            fixture["source"]["fixtureFormat"].as_str(),
            Some("state_test")
        );
        assert_eq!(fixture["source"]["fork"].as_str(), Some("Cancun"));

        let cases = fixture["cases"].as_array().expect("refund cap cases");
        assert_eq!(cases.len(), 7);

        let mut refund_max_gas_used = None;
        let mut refund_50_cap_gas_used = None;
        let mut refund_sstore_gas_used = None;
        let mut sstore_gas_used = None;
        let mut contract_clear_gas_used = None;
        let mut internal_clear_gas_used = None;
        let mut combined_clear_gas_used = None;

        for case in cases {
            let label = json_str(&case["label"], "case.label");
            assert!(
                json_str(&case["fixtureKey"], "case.fixtureKey").contains("-fork_[Cancun-Prague]"),
                "official refund case must be the Cancun/Prague projection"
            );
            assert!(
                json_str(&case["postHash"], "case.postHash").starts_with("0x"),
                "official refund case must carry post hash"
            );

            let sender = json_str(&case["sender"], "case.sender");
            let to = json_str(&case["to"], "case.to");
            let pre_sender_balance =
                json_hex_u128(&case["preSenderBalance"], "case.preSenderBalance");
            let pre_sender_nonce =
                json_hex_u128(&case["preSenderNonce"], "case.preSenderNonce") as u64;
            let pre_to_balance = json_hex_u128(&case["preToBalance"], "case.preToBalance");
            let pre_to_nonce = json_hex_u128(&case["preToNonce"], "case.preToNonce") as u64;
            let post_sender_balance =
                json_hex_u128(&case["postSenderBalance"], "case.postSenderBalance");
            let post_to_balance = json_hex_u128(&case["postToBalance"], "case.postToBalance");
            let gas_price = json_hex_u128(&case["gasPrice"], "case.gasPrice");
            let gas_limit = json_hex_u128(&case["gasLimit"], "case.gasLimit");
            let value = json_hex_u128(&case["value"], "case.value");
            let nonce = json_hex_u128(&case["nonce"], "case.nonce") as u64;
            assert_eq!(nonce, pre_sender_nonce);

            let raw_tx = decode_hex_bytes(json_str(&case["txbytes"], "case.txbytes"));
            let recovered_sender = recover_raw_evm_tx_sender_m0(&raw_tx)
                .expect("official refund raw sender recovery")
                .expect("official refund recovered sender");
            assert_eq!(recovered_sender, decode_hex_bytes(sender));
            let fields =
                translate_raw_evm_tx_fields_m0(&raw_tx).expect("official refund tx decode");
            let tx = tx_ir_from_raw_fields_m0(&fields, &raw_tx, recovered_sender, 1);

            assert_eq!(tx.from, decode_hex_bytes(sender));
            assert_eq!(tx.to, Some(decode_hex_bytes(to)));
            assert_eq!(tx.value, value);
            assert_eq!(tx.gas_limit, gas_limit as u64);
            assert_eq!(tx.gas_price, gas_price as u64);
            assert_eq!(tx.nonce, nonce);
            assert_eq!(
                tx.data,
                decode_hex_bytes(json_str(&case["data"], "case.data"))
            );
            if raw_tx.first() == Some(&0x01) {
                assert!(
                    tx.evm_access_list.is_empty(),
                    "official refund typed tx subset uses empty access lists"
                );
            }

            let mut adapter = NovoVmAdapter::new(ChainConfig {
                chain_type: ChainType::EVM,
                chain_id: 1,
                name: format!("official-sstore-refund-{label}"),
                enabled: true,
                custom_config: None,
            });
            adapter.initialize().expect("init");
            assert!(
                adapter
                    .verify_transaction(&tx)
                    .expect("verify official refund raw tx"),
                "official SSTORE/refund txbytes must verify through the raw EVM path"
            );
            for historical_nonce in 0..pre_sender_nonce {
                let mut historical_tx = tx.clone();
                historical_tx.nonce = historical_nonce;
                adapter
                    .route_transaction_through_unified_account(&historical_tx)
                    .expect("prime official refund pre-state nonce");
            }

            let charged_fee = pre_sender_balance
                .saturating_sub(post_sender_balance)
                .saturating_sub(value);
            assert_eq!(charged_fee % gas_price, 0);
            let gas_used = charged_fee / gas_price;
            let expected_gas_used = case["gasUsed"].as_u64().expect("case.gasUsed");
            assert_eq!(gas_used, expected_gas_used as u128);

            let mut runtime_state = StateIR::new();
            runtime_state.set_account(
                tx.from.clone(),
                AccountState {
                    balance: pre_sender_balance,
                    nonce: pre_sender_nonce,
                    code_hash: None,
                    storage_root: vec![0u8; 32],
                },
            );
            runtime_state.set_account(
                tx.to.clone().expect("call target"),
                AccountState {
                    balance: pre_to_balance,
                    nonce: pre_to_nonce,
                    code_hash: Some(vec![0xcc; 32]),
                    storage_root: vec![0u8; 32],
                },
            );

            let mut artifact = sample_aoem_artifact(&tx, true, [0x85; 32], None);
            artifact.gas_used = expected_gas_used;
            artifact.cumulative_gas_used = expected_gas_used;
            artifact.effective_gas_price = Some(gas_price as u64);
            artifact.event_logs.clear();
            artifact.log_bloom = vec![0u8; 256];

            let outcome = adapter
                .execute_transaction_with_observed_metadata_v1(
                    &tx,
                    &mut runtime_state,
                    Some(&artifact),
                )
                .expect("execute official SSTORE refund fixture projection");

            assert!(outcome.artifact.status_ok);
            assert_eq!(outcome.artifact.gas_used, expected_gas_used);
            assert_eq!(
                runtime_state.get_account(&tx.from).map(|acc| acc.balance),
                Some(post_sender_balance)
            );
            if post_to_balance == pre_to_balance + value {
                assert_eq!(
                    runtime_state
                        .get_account(tx.to.as_ref().expect("call target"))
                        .map(|acc| acc.balance),
                    Some(post_to_balance)
                );
            }

            let bal = outcome
                .observed_block_access_list
                .block_access_list
                .expect("official SSTORE refund observed BAL");
            let sender_entry = bal
                .0
                .iter()
                .find(|entry| entry.address.as_slice() == tx.from.as_slice())
                .expect("sender BAL entry");
            assert_eq!(
                sender_entry.balance_changes[0].post_balance,
                NovoVmAdapter::u128_to_be32_v1(post_sender_balance)
            );

            let post_storage = case["postTargetStorage"]
                .as_object()
                .expect("case.postTargetStorage");
            match label {
                "refundMax" => {
                    refund_max_gas_used = Some(expected_gas_used);
                    assert!(post_storage.is_empty());
                }
                "refund50percentCap" => {
                    refund_50_cap_gas_used = Some(expected_gas_used);
                    assert_eq!(
                        json_str(&case["postTargetStorage"]["0x0b"], "refund cap slot 0x0b"),
                        "0x0de0b6b3a7640000"
                    );
                }
                "refundSSTORE" => {
                    refund_sstore_gas_used = Some(expected_gas_used);
                    assert!(post_storage.is_empty());
                }
                "sstoreGas" => {
                    sstore_gas_used = Some(expected_gas_used);
                    assert_eq!(
                        json_str(&case["postTargetStorage"]["0x1006"], "sstore slot 0x1006"),
                        "0x5654"
                    );
                    assert_eq!(
                        json_str(&case["postTargetStorage"]["0x1007"], "sstore slot 0x1007"),
                        "0x0898"
                    );
                    assert_eq!(
                        json_str(&case["postTargetStorage"]["0x1008"], "sstore slot 0x1008"),
                        "0x4e20"
                    );
                }
                "ContractStoreClearsSuccess" => {
                    contract_clear_gas_used = Some(expected_gas_used);
                    assert!(post_storage.is_empty());
                }
                "InternalCallStoreClearsSuccess" => {
                    internal_clear_gas_used = Some(expected_gas_used);
                    assert!(post_storage.is_empty());
                }
                "StoreClearsAndInternalCallStoreClearsSuccess" => {
                    combined_clear_gas_used = Some(expected_gas_used);
                    assert_eq!(
                        json_str(&case["postTargetStorage"]["0x04"], "store clear slot 0x04"),
                        "0x0c"
                    );
                }
                _ => panic!("unexpected official SSTORE/refund label {label}"),
            }
        }

        let refund_max_gas_used = refund_max_gas_used.expect("refundMax gas used");
        let refund_50_cap_gas_used = refund_50_cap_gas_used.expect("refund50percentCap gas used");
        let refund_sstore_gas_used = refund_sstore_gas_used.expect("refundSSTORE gas used");
        let sstore_gas_used = sstore_gas_used.expect("sstoreGas gas used");
        let contract_clear_gas_used =
            contract_clear_gas_used.expect("ContractStoreClearsSuccess gas used");
        let internal_clear_gas_used =
            internal_clear_gas_used.expect("InternalCallStoreClearsSuccess gas used");
        let combined_clear_gas_used =
            combined_clear_gas_used.expect("StoreClearsAndInternalCallStoreClearsSuccess gas used");

        assert_eq!(refund_max_gas_used, 48_842);
        assert_eq!(refund_50_cap_gas_used, 76_336);
        assert_eq!(refund_sstore_gas_used, 21_210);
        assert_eq!(sstore_gas_used, 225_910);
        assert_eq!(contract_clear_gas_used, 56_848);
        assert_eq!(internal_clear_gas_used, 64_305);
        assert_eq!(combined_clear_gas_used, 80_324);
        assert!(
            refund_max_gas_used > refund_sstore_gas_used,
            "official refundMax must charge more gas than the compact refundSSTORE clear path"
        );
        assert!(
            combined_clear_gas_used > internal_clear_gas_used
                && internal_clear_gas_used > contract_clear_gas_used,
            "official store-clear success cases must preserve the expected gas ordering"
        );
    }

    #[test]
    fn official_state_fixture_failure_account_fee_debit_v1() {
        let fixture = official_failure_account_state_fixture_v1();
        assert_eq!(fixture["source"]["repo"].as_str(), Some("ethereum/tests"));
        assert_eq!(
            fixture["source"]["fixtureFormat"].as_str(),
            Some("state_test")
        );
        assert_eq!(fixture["source"]["fork"].as_str(), Some("Cancun"));

        let cases = fixture["cases"].as_array().expect("failure/account cases");
        assert_eq!(cases.len(), 7);

        let mut top_level_revert_seen = false;
        let mut deploy_revert_seen = false;
        let mut no_value_transfer_cases = 0usize;
        let mut storage_no_commit_seen = false;
        let mut out_of_gas_cases = 0usize;

        for case in cases {
            let label = json_str(&case["label"], "case.label");
            assert!(
                json_str(&case["fixtureKey"], "case.fixtureKey").contains("-fork_[Cancun-Prague]"),
                "official failure/account case must be the Cancun/Prague projection"
            );
            assert!(
                json_str(&case["postHash"], "case.postHash").starts_with("0x"),
                "official failure/account case must carry post hash"
            );

            let sender = json_str(&case["sender"], "case.sender");
            let expected_to = case["to"].as_str().map(decode_hex_bytes);
            let pre_sender_balance =
                json_hex_u128(&case["preSenderBalance"], "case.preSenderBalance");
            let pre_sender_nonce =
                json_hex_u128(&case["preSenderNonce"], "case.preSenderNonce") as u64;
            let post_sender_balance =
                json_hex_u128(&case["postSenderBalance"], "case.postSenderBalance");
            let gas_price = json_hex_u128(&case["gasPrice"], "case.gasPrice");
            let gas_limit = json_hex_u128(&case["gasLimit"], "case.gasLimit");
            let value = json_hex_u128(&case["value"], "case.value");
            let nonce = json_hex_u128(&case["nonce"], "case.nonce") as u64;
            assert_eq!(nonce, pre_sender_nonce);

            let raw_tx = decode_hex_bytes(json_str(&case["txbytes"], "case.txbytes"));
            let recovered_sender = recover_raw_evm_tx_sender_m0(&raw_tx)
                .expect("official failure/account raw sender recovery")
                .expect("official failure/account recovered sender");
            assert_eq!(recovered_sender, decode_hex_bytes(sender));
            let fields =
                translate_raw_evm_tx_fields_m0(&raw_tx).expect("official failure tx decode");
            let tx = tx_ir_from_raw_fields_m0(&fields, &raw_tx, recovered_sender, 1);

            assert_eq!(tx.from, decode_hex_bytes(sender));
            assert_eq!(tx.to, expected_to);
            assert_eq!(tx.value, value);
            assert_eq!(tx.gas_limit, gas_limit as u64);
            assert_eq!(tx.gas_price, gas_price as u64);
            assert_eq!(tx.nonce, nonce);
            assert_eq!(
                tx.data,
                decode_hex_bytes(json_str(&case["data"], "case.data"))
            );

            let mut adapter = NovoVmAdapter::new(ChainConfig {
                chain_type: ChainType::EVM,
                chain_id: 1,
                name: format!("official-failure-account-{label}"),
                enabled: true,
                custom_config: None,
            });
            adapter.initialize().expect("init");
            assert!(
                adapter
                    .verify_transaction(&tx)
                    .expect("verify official failure/account raw tx"),
                "official failure/account txbytes must verify through the raw EVM path"
            );
            for historical_nonce in 0..pre_sender_nonce {
                let mut historical_tx = tx.clone();
                historical_tx.nonce = historical_nonce;
                adapter
                    .route_transaction_through_unified_account(&historical_tx)
                    .expect("prime official failure/account pre-state nonce");
            }

            let charged_fee = pre_sender_balance.saturating_sub(post_sender_balance);
            assert_eq!(charged_fee % gas_price, 0);
            let gas_used = charged_fee / gas_price;
            let expected_gas_used = case["gasUsed"].as_u64().expect("case.gasUsed");
            assert_eq!(gas_used, expected_gas_used as u128);

            let mut runtime_state = StateIR::new();
            runtime_state.set_account(
                tx.from.clone(),
                AccountState {
                    balance: pre_sender_balance,
                    nonce: pre_sender_nonce,
                    code_hash: None,
                    storage_root: vec![0u8; 32],
                },
            );

            if let Some(to) = tx.to.as_ref() {
                let pre_to_balance = json_hex_u128(&case["preToBalance"], "case.preToBalance");
                let pre_to_nonce = json_hex_u128(&case["preToNonce"], "case.preToNonce") as u64;
                runtime_state.set_account(
                    to.clone(),
                    AccountState {
                        balance: pre_to_balance,
                        nonce: pre_to_nonce,
                        code_hash: Some(vec![0xcc; 32]),
                        storage_root: vec![0u8; 32],
                    },
                );
                for (slot, value) in case["preTargetStorage"]
                    .as_object()
                    .expect("case.preTargetStorage")
                {
                    runtime_state.set_storage(
                        to.clone(),
                        decode_hex_bytes(slot),
                        decode_hex_bytes(json_str(value, "preTargetStorage.value")),
                    );
                }
            }

            let deploy_address = if tx.tx_type == TxType::ContractDeploy {
                Some(derive_create_contract_address_m0(&tx.from, tx.nonce))
            } else {
                None
            };
            let mut artifact = sample_aoem_artifact(&tx, false, [0x86; 32], deploy_address.clone());
            artifact.gas_used = expected_gas_used;
            artifact.cumulative_gas_used = expected_gas_used;
            artifact.effective_gas_price = Some(gas_price as u64);
            artifact.event_logs.clear();
            artifact.log_bloom = vec![0u8; 256];
            let failure_class = json_str(&case["failureClass"], "case.failureClass");
            if failure_class == "revert" {
                artifact.revert_data = Some(vec![0xfd]);
            } else {
                artifact.revert_data = None;
            }
            if let Some(anchor) = artifact.anchor.as_mut() {
                anchor.return_code = match failure_class {
                    "revert" => 0,
                    "out_of_gas" => 13,
                    "invalid" => 14,
                    _ => 2001,
                };
                anchor.return_code_name = failure_class.to_string();
            }

            let outcome = adapter
                .execute_transaction_with_observed_metadata_v1(
                    &tx,
                    &mut runtime_state,
                    Some(&artifact),
                )
                .expect("execute official failure/account fixture projection");

            assert!(!outcome.artifact.status_ok);
            assert_eq!(outcome.artifact.gas_used, expected_gas_used);
            assert!(outcome.artifact.event_logs.is_empty());
            assert!(outcome.artifact.log_bloom.iter().all(|byte| *byte == 0));
            assert_eq!(
                runtime_state.get_account(&tx.from).map(|acc| acc.balance),
                Some(post_sender_balance)
            );
            assert_eq!(
                runtime_state.get_storage(&tx.from, b"aoem:last_failure_class"),
                Some(&failure_class.as_bytes().to_vec())
            );

            if let Some(to) = tx.to.as_ref() {
                let post_to_balance = json_hex_u128(&case["postToBalance"], "case.postToBalance");
                assert_eq!(
                    runtime_state.get_account(to).map(|acc| acc.balance),
                    Some(post_to_balance)
                );
                if value > 0 && !case["valueTransferred"].as_bool().unwrap_or(false) {
                    no_value_transfer_cases += 1;
                    assert_eq!(
                        runtime_state.get_account(to).map(|acc| acc.balance),
                        Some(json_hex_u128(&case["preToBalance"], "case.preToBalance"))
                    );
                }
                for (slot, value) in case["postTargetStorage"]
                    .as_object()
                    .expect("case.postTargetStorage")
                {
                    assert_eq!(
                        runtime_state.get_storage(to, &decode_hex_bytes(slot)),
                        Some(&decode_hex_bytes(json_str(
                            value,
                            "postTargetStorage.value"
                        )))
                    );
                    storage_no_commit_seen = true;
                }
                assert!(
                    runtime_state
                        .get_storage(to, &tx.nonce.to_le_bytes().to_vec())
                        .is_none(),
                    "failed official case must not add adapter synthetic target storage"
                );
            } else {
                let deploy_address = deploy_address.as_ref().expect("deploy address");
                assert!(
                    runtime_state.get_account(deploy_address).is_none(),
                    "failed official deploy must not create a contract account"
                );
            }

            let bal = outcome
                .observed_block_access_list
                .block_access_list
                .expect("official failure/account observed BAL");
            let sender_entry = bal
                .0
                .iter()
                .find(|entry| entry.address.as_slice() == tx.from.as_slice())
                .expect("sender BAL entry");
            assert_eq!(
                sender_entry.balance_changes[0].post_balance,
                NovoVmAdapter::u128_to_be32_v1(post_sender_balance)
            );
            if let Some(deploy_address) = deploy_address.as_ref() {
                assert!(
                    bal.0
                        .iter()
                        .all(|entry| entry.address.as_slice() != deploy_address.as_slice()),
                    "failed official deploy must not emit a contract BAL entry"
                );
            }

            match label {
                "topLevelRevertOpcode" => top_level_revert_seen = true,
                "deployInitRevertOpcode" => deploy_revert_seen = true,
                _ => {}
            }
            if failure_class == "out_of_gas" {
                out_of_gas_cases += 1;
            }
        }

        assert!(top_level_revert_seen);
        assert!(deploy_revert_seen);
        assert_eq!(no_value_transfer_cases, 4);
        assert!(storage_no_commit_seen);
        assert_eq!(out_of_gas_cases, 4);
    }

    #[test]
    fn official_state_fixture_create_account_grouped_projection_v1() {
        let fixture = official_create_account_state_fixture_v1();
        assert_eq!(fixture["source"]["repo"].as_str(), Some("ethereum/tests"));
        assert_eq!(
            fixture["source"]["fixtureFormat"].as_str(),
            Some("state_test")
        );
        assert_eq!(fixture["source"]["fork"].as_str(), Some("Cancun"));

        let cases = fixture["cases"].as_array().expect("CREATE/account cases");
        assert_eq!(cases.len(), 9);

        let mut deploy_success_cases = 0usize;
        let mut deploy_collision_cases = 0usize;
        let mut internal_projection_cases = 0usize;
        let mut create2_projection_cases = 0usize;

        for case in cases {
            let label = json_str(&case["label"], "case.label");
            let mode = json_str(&case["mode"], "case.mode");
            assert!(
                json_str(&case["fixtureKey"], "case.fixtureKey").contains("-fork_[Cancun-Prague]"),
                "official CREATE/account case must be the Cancun/Prague projection"
            );
            assert!(
                json_str(&case["postHash"], "case.postHash").starts_with("0x"),
                "official CREATE/account case must carry post hash"
            );

            let sender = json_str(&case["sender"], "case.sender");
            let expected_to = case["to"].as_str().map(decode_hex_bytes);
            let pre_sender_balance =
                json_hex_u128(&case["preSenderBalance"], "case.preSenderBalance");
            let pre_sender_nonce =
                json_hex_u128(&case["preSenderNonce"], "case.preSenderNonce") as u64;
            let post_sender_balance =
                json_hex_u128(&case["postSenderBalance"], "case.postSenderBalance");
            let gas_price = json_hex_u128(&case["gasPrice"], "case.gasPrice");
            let gas_limit = json_hex_u128(&case["gasLimit"], "case.gasLimit");
            let value = json_hex_u128(&case["value"], "case.value");
            let nonce = json_hex_u128(&case["nonce"], "case.nonce") as u64;
            assert_eq!(nonce, pre_sender_nonce);

            let raw_tx = decode_hex_bytes(json_str(&case["txbytes"], "case.txbytes"));
            let recovered_sender = recover_raw_evm_tx_sender_m0(&raw_tx)
                .expect("official CREATE/account raw sender recovery")
                .expect("official CREATE/account recovered sender");
            assert_eq!(recovered_sender, decode_hex_bytes(sender));
            let fields =
                translate_raw_evm_tx_fields_m0(&raw_tx).expect("official CREATE tx decode");
            let tx = tx_ir_from_raw_fields_m0(&fields, &raw_tx, recovered_sender, 1);

            assert_eq!(tx.from, decode_hex_bytes(sender));
            assert_eq!(tx.to, expected_to);
            assert_eq!(tx.value, value);
            assert_eq!(tx.gas_limit, gas_limit as u64);
            assert_eq!(tx.gas_price, gas_price as u64);
            assert_eq!(tx.nonce, nonce);
            assert_eq!(
                tx.data,
                decode_hex_bytes(json_str(&case["data"], "case.data"))
            );
            if mode.starts_with("topLevelDeploy") {
                assert_eq!(tx.tx_type, TxType::ContractDeploy);
            }

            let mut adapter = NovoVmAdapter::new(ChainConfig {
                chain_type: ChainType::EVM,
                chain_id: 1,
                name: format!("official-create-account-{label}"),
                enabled: true,
                custom_config: None,
            });
            adapter.initialize().expect("init");
            assert!(
                adapter
                    .verify_transaction(&tx)
                    .expect("verify official CREATE/account raw tx"),
                "official CREATE/account txbytes must verify through the raw EVM path"
            );
            for historical_nonce in 0..pre_sender_nonce {
                let mut historical_tx = tx.clone();
                historical_tx.nonce = historical_nonce;
                adapter
                    .route_transaction_through_unified_account(&historical_tx)
                    .expect("prime official CREATE/account pre-state nonce");
            }

            let mut value_transferred = false;
            if mode == "topLevelDeploySuccess" {
                value_transferred = true;
            } else if case["preToBalance"].is_string() && case["postToBalance"].is_string() {
                let pre_to_balance = json_hex_u128(&case["preToBalance"], "case.preToBalance");
                let post_to_balance = json_hex_u128(&case["postToBalance"], "case.postToBalance");
                value_transferred = post_to_balance == pre_to_balance + value;
            }
            let charged_fee = pre_sender_balance
                .saturating_sub(post_sender_balance)
                .saturating_sub(if value_transferred { value } else { 0 });
            assert_eq!(charged_fee % gas_price, 0);
            let gas_used = charged_fee / gas_price;
            let expected_gas_used = case["gasUsed"].as_u64().expect("case.gasUsed");
            assert_eq!(gas_used, expected_gas_used as u128);

            let mut runtime_state = StateIR::new();
            runtime_state.set_account(
                tx.from.clone(),
                AccountState {
                    balance: pre_sender_balance,
                    nonce: pre_sender_nonce,
                    code_hash: None,
                    storage_root: vec![0u8; 32],
                },
            );
            if let Some(to) = tx.to.as_ref() {
                runtime_state.set_account(
                    to.clone(),
                    AccountState {
                        balance: json_hex_u128(&case["preToBalance"], "case.preToBalance"),
                        nonce: json_hex_u128(&case["preToNonce"], "case.preToNonce") as u64,
                        code_hash: Some(vec![0xcc; 32]),
                        storage_root: vec![0u8; 32],
                    },
                );
            }

            let contract_address = case["contractAddress"].as_str().map(decode_hex_bytes);
            if mode == "topLevelDeployCollision" {
                let contract_address = contract_address.clone().expect("collision contract");
                let pre_code = decode_hex_bytes(json_str(&case["preContractCode"], "pre code"));
                runtime_state.set_account(
                    contract_address.clone(),
                    AccountState {
                        balance: json_hex_u128(
                            &case["preContractBalance"],
                            "case.preContractBalance",
                        ),
                        nonce: json_hex_u128(&case["preContractNonce"], "case.preContractNonce")
                            as u64,
                        code_hash: if pre_code.is_empty() {
                            None
                        } else {
                            Some(Sha256::digest(&pre_code).to_vec())
                        },
                        storage_root: vec![0u8; 32],
                    },
                );
                if !pre_code.is_empty() {
                    runtime_state.set_storage(
                        contract_address,
                        NovoVmAdapter::runtime_code_storage_key().to_vec(),
                        pre_code,
                    );
                }
            }

            let mut artifact = sample_aoem_artifact(
                &tx,
                true,
                [0x87; 32],
                contract_address.clone().or_else(|| {
                    if tx.tx_type == TxType::ContractDeploy {
                        Some(derive_create_contract_address_m0(&tx.from, tx.nonce))
                    } else {
                        None
                    }
                }),
            );
            artifact.gas_used = expected_gas_used;
            artifact.cumulative_gas_used = expected_gas_used;
            artifact.effective_gas_price = Some(gas_price as u64);
            artifact.event_logs.clear();
            artifact.log_bloom = vec![0u8; 256];
            if mode == "topLevelDeploySuccess" {
                let runtime_code =
                    decode_hex_bytes(json_str(&case["postContractCode"], "postContractCode"));
                artifact.runtime_code = Some(runtime_code.clone());
                artifact.runtime_code_hash = Some(Sha256::digest(&runtime_code).to_vec());
            }

            let outcome = adapter
                .execute_transaction_with_observed_metadata_v1(
                    &tx,
                    &mut runtime_state,
                    Some(&artifact),
                )
                .expect("execute official CREATE/account fixture projection");

            assert_eq!(
                runtime_state.get_account(&tx.from).map(|acc| acc.balance),
                Some(post_sender_balance)
            );
            let bal = outcome
                .observed_block_access_list
                .block_access_list
                .expect("official CREATE/account observed BAL");
            let sender_entry = bal
                .0
                .iter()
                .find(|entry| entry.address.as_slice() == tx.from.as_slice())
                .expect("sender BAL entry");
            assert_eq!(
                sender_entry.balance_changes[0].post_balance,
                NovoVmAdapter::u128_to_be32_v1(post_sender_balance)
            );

            match mode {
                "topLevelDeploySuccess" => {
                    deploy_success_cases += 1;
                    assert!(outcome.artifact.status_ok);
                    let contract_address = contract_address.expect("success contract");
                    assert_eq!(
                        derive_create_contract_address_m0(&tx.from, tx.nonce),
                        contract_address
                    );
                    assert_eq!(
                        outcome.artifact.contract_address,
                        Some(contract_address.clone())
                    );
                    let contract = runtime_state
                        .get_account(&contract_address)
                        .expect("official CREATE success contract account");
                    assert_eq!(
                        contract.balance,
                        json_hex_u128(&case["postContractBalance"], "postContractBalance")
                    );
                    assert_eq!(
                        contract.nonce,
                        json_hex_u128(&case["postContractNonce"], "postContractNonce") as u64
                    );
                    assert_eq!(
                        runtime_state.get_storage(
                            &contract_address,
                            NovoVmAdapter::runtime_code_storage_key()
                        ),
                        Some(&decode_hex_bytes(json_str(
                            &case["postContractCode"],
                            "postContractCode"
                        )))
                    );
                    assert!(
                        bal.0
                            .iter()
                            .any(|entry| entry.address.as_slice() == contract_address.as_slice()),
                        "official CREATE success must emit a contract BAL entry"
                    );
                }
                "topLevelDeployCollision" => {
                    deploy_collision_cases += 1;
                    assert!(!outcome.artifact.status_ok);
                    assert_eq!(outcome.artifact.contract_address, None);
                    let contract_address = contract_address.expect("collision contract");
                    let contract = runtime_state
                        .get_account(&contract_address)
                        .expect("official CREATE collision target remains");
                    assert_eq!(
                        contract.balance,
                        json_hex_u128(&case["postContractBalance"], "postContractBalance")
                    );
                    assert_eq!(
                        contract.nonce,
                        json_hex_u128(&case["postContractNonce"], "postContractNonce") as u64
                    );
                    assert_eq!(
                        runtime_state.get_storage(&tx.from, b"deploy:last_reject_reason"),
                        Some(&b"create_collision_existing_account".to_vec())
                    );
                    assert!(
                        bal.0
                            .iter()
                            .all(|entry| entry.address.as_slice() != contract_address.as_slice()),
                        "official CREATE collision must not emit a contract BAL entry"
                    );
                }
                "internalCreateProjection" => {
                    internal_projection_cases += 1;
                    if label.contains("create2") {
                        create2_projection_cases += 1;
                    }
                    assert!(outcome.artifact.status_ok);
                    if let Some(to) = tx.to.as_ref() {
                        assert_eq!(
                            runtime_state.get_account(to).map(|acc| acc.balance),
                            Some(json_hex_u128(&case["postToBalance"], "postToBalance"))
                        );
                    }
                    assert!(
                        case["internalCreatedAddresses"]
                            .as_array()
                            .expect("internalCreatedAddresses")
                            .iter()
                            .all(|addr| runtime_state
                                .get_account(&decode_hex_bytes(json_str(
                                    addr,
                                    "internalCreatedAddress"
                                )))
                                .is_none()),
                        "internal CREATE/CREATE2 accounts remain host/AOEM responsibility"
                    );
                }
                _ => panic!("unexpected official CREATE/account mode {mode}"),
            }
        }

        assert_eq!(deploy_success_cases, 2);
        assert_eq!(deploy_collision_cases, 2);
        assert_eq!(internal_projection_cases, 5);
        assert_eq!(create2_projection_cases, 2);
    }

    #[test]
    fn official_state_fixture_staticcall_precompile_return_grouped_projection_v1() {
        let fixture = official_staticcall_precompile_return_state_fixture_v1();
        assert_eq!(fixture["source"]["repo"].as_str(), Some("ethereum/tests"));
        assert_eq!(
            fixture["source"]["fixtureFormat"].as_str(),
            Some("state_test")
        );
        assert_eq!(fixture["source"]["fork"].as_str(), Some("Cancun"));

        let profile = resolve_evm_profile(ChainType::EVM, 1).expect("eth profile");
        let precompiles = active_precompile_set_m0(&profile);
        for required in ["ecrecover", "sha256", "ripemd160", "identity"] {
            assert!(
                precompiles.contains(&required),
                "ethereum precompile set must include {required}"
            );
        }

        let cases = fixture["cases"]
            .as_array()
            .expect("STATICCALL/precompile/return cases");
        assert_eq!(cases.len(), 9);

        let empty_logs_hash = "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347";
        let mut static_precompile_cases = 0usize;
        let mut return_projection_cases = 0usize;
        let mut nested_account_fact_cases = 0usize;
        let mut direct_precompile_suite_gas = None;
        let mut called_precompile_suite_gas = None;
        let mut sha256_gas = None;
        let mut identity_gas = None;
        let mut ripemd160_gas = None;
        let mut ecrecover_gas = None;
        let mut return_test_gas = None;
        let mut call_to_return_gas = None;
        let mut call_output_partial_gas = None;

        for case in cases {
            let label = json_str(&case["label"], "case.label");
            let mode = json_str(&case["mode"], "case.mode");
            assert!(
                json_str(&case["fixtureKey"], "case.fixtureKey").contains("-fork_[Cancun-Prague]"),
                "official STATICCALL/precompile/return case must be the Cancun/Prague projection"
            );
            assert!(
                json_str(&case["postHash"], "case.postHash").starts_with("0x"),
                "official STATICCALL/precompile/return case must carry post hash"
            );
            assert_eq!(
                json_str(&case["logsHash"], "case.logsHash"),
                empty_logs_hash
            );

            let sender = json_str(&case["sender"], "case.sender");
            let to = json_str(&case["to"], "case.to");
            let pre_sender_balance =
                json_hex_u128(&case["preSenderBalance"], "case.preSenderBalance");
            let pre_sender_nonce =
                json_hex_u128(&case["preSenderNonce"], "case.preSenderNonce") as u64;
            let post_sender_balance =
                json_hex_u128(&case["postSenderBalance"], "case.postSenderBalance");
            let pre_to_balance = json_hex_u128(&case["preToBalance"], "case.preToBalance");
            let pre_to_nonce = json_hex_u128(&case["preToNonce"], "case.preToNonce") as u64;
            let post_to_balance = json_hex_u128(&case["postToBalance"], "case.postToBalance");
            let post_to_nonce = json_hex_u128(&case["postToNonce"], "case.postToNonce") as u64;
            let gas_price = json_hex_u128(&case["gasPrice"], "case.gasPrice");
            let gas_limit = json_hex_u128(&case["gasLimit"], "case.gasLimit");
            let value = json_hex_u128(&case["value"], "case.value");
            let nonce = json_hex_u128(&case["nonce"], "case.nonce") as u64;
            assert_eq!(nonce, pre_sender_nonce);

            let raw_tx = decode_hex_bytes(json_str(&case["txbytes"], "case.txbytes"));
            let recovered_sender = recover_raw_evm_tx_sender_m0(&raw_tx)
                .expect("official STATICCALL/precompile/return raw sender recovery")
                .expect("official STATICCALL/precompile/return recovered sender");
            assert_eq!(recovered_sender, decode_hex_bytes(sender));
            let fields = translate_raw_evm_tx_fields_m0(&raw_tx)
                .expect("official STATICCALL/precompile/return tx decode");
            let tx = tx_ir_from_raw_fields_m0(&fields, &raw_tx, recovered_sender, 1);

            assert_eq!(
                tx.tx_type,
                TxType::Transfer,
                "raw empty-calldata tx classification is state-agnostic before execution"
            );
            assert_eq!(tx.from, decode_hex_bytes(sender));
            assert_eq!(tx.to, Some(decode_hex_bytes(to)));
            assert_eq!(tx.value, value);
            assert_eq!(tx.gas_limit, gas_limit as u64);
            assert_eq!(tx.gas_price, gas_price as u64);
            assert_eq!(tx.nonce, nonce);
            assert_eq!(
                tx.data.len(),
                case["dataLen"].as_u64().expect("case.dataLen") as usize
            );
            assert_eq!(
                Sha256::digest(&tx.data).to_vec(),
                decode_hex_bytes(json_str(&case["dataSha256"], "case.dataSha256"))
            );

            let mut adapter = NovoVmAdapter::new(ChainConfig {
                chain_type: ChainType::EVM,
                chain_id: 1,
                name: format!("official-staticcall-precompile-return-{label}"),
                enabled: true,
                custom_config: None,
            });
            adapter.initialize().expect("init");
            assert!(
                adapter
                    .verify_transaction(&tx)
                    .expect("verify official STATICCALL/precompile/return raw tx"),
                "official STATICCALL/precompile/return txbytes must verify through raw EVM path"
            );
            for historical_nonce in 0..pre_sender_nonce {
                let mut historical_tx = tx.clone();
                historical_tx.nonce = historical_nonce;
                adapter
                    .route_transaction_through_unified_account(&historical_tx)
                    .expect("prime official STATICCALL/precompile/return pre-state nonce");
            }

            let value_transferred = post_to_balance == pre_to_balance + value;
            let charged_fee = pre_sender_balance
                .saturating_sub(post_sender_balance)
                .saturating_sub(if value_transferred { value } else { 0 });
            assert_eq!(charged_fee % gas_price, 0);
            let gas_used = charged_fee / gas_price;
            let expected_gas_used = case["gasUsed"].as_u64().expect("case.gasUsed");
            assert_eq!(gas_used, expected_gas_used as u128);

            let mut runtime_state = StateIR::new();
            runtime_state.set_account(
                tx.from.clone(),
                AccountState {
                    balance: pre_sender_balance,
                    nonce: pre_sender_nonce,
                    code_hash: None,
                    storage_root: vec![0u8; 32],
                },
            );
            runtime_state.set_account(
                tx.to.clone().expect("call target"),
                AccountState {
                    balance: pre_to_balance,
                    nonce: pre_to_nonce,
                    code_hash: Some(vec![0xcc; 32]),
                    storage_root: vec![0u8; 32],
                },
            );
            assert_eq!(
                NovoVmAdapter::effective_execution_tx_v1(&tx, &runtime_state).tx_type,
                TxType::ContractCall,
                "target code in pre-state must promote empty-calldata tx to contract call execution"
            );

            let mut artifact = sample_aoem_artifact(&tx, true, [0x88; 32], None);
            artifact.gas_used = expected_gas_used;
            artifact.cumulative_gas_used = expected_gas_used;
            artifact.effective_gas_price = Some(gas_price as u64);
            artifact.receipt_type = None;
            artifact.event_logs.clear();
            artifact.log_bloom = vec![0u8; AOEM_LOG_BLOOM_BYTES_V1];
            artifact.anchor = None;

            let outcome = adapter
                .execute_transaction_with_observed_metadata_v1(
                    &tx,
                    &mut runtime_state,
                    Some(&artifact),
                )
                .expect("execute official STATICCALL/precompile/return fixture projection");

            assert!(outcome.artifact.status_ok);
            assert_eq!(outcome.artifact.gas_used, expected_gas_used);
            assert!(outcome.artifact.event_logs.is_empty());
            assert!(outcome.artifact.log_bloom.iter().all(|byte| *byte == 0));
            assert_eq!(
                runtime_state.get_storage(&tx.from, b"aoem:last_log_bloom"),
                Some(&vec![0u8; AOEM_LOG_BLOOM_BYTES_V1])
            );
            assert_eq!(
                runtime_state.get_account(&tx.from).map(|acc| acc.balance),
                Some(post_sender_balance)
            );
            assert_eq!(
                runtime_state
                    .get_account(tx.to.as_ref().expect("call target"))
                    .map(|acc| acc.balance),
                Some(post_to_balance)
            );
            assert_eq!(
                runtime_state
                    .get_account(tx.to.as_ref().expect("call target"))
                    .map(|acc| acc.nonce),
                Some(pre_to_nonce),
                "adapter must not claim opcode-level target nonce projection"
            );
            assert_eq!(
                post_to_nonce, pre_to_nonce,
                "selected official STATICCALL/precompile/return subset keeps target nonce stable"
            );

            let post_storage_facts = case["postStorageFacts"]
                .as_object()
                .expect("case.postStorageFacts");
            assert!(
                !post_storage_facts.is_empty(),
                "official STATICCALL/precompile/return case must pin at least one storage fact"
            );
            for (slot, value) in post_storage_facts {
                assert!(slot.starts_with("0x"));
                assert!(json_str(value, "postStorageFacts.value").starts_with("0x"));
            }
            if case["nestedAccountFacts"].is_object() {
                nested_account_fact_cases += 1;
                let nested = &case["nestedAccountFacts"];
                assert!(json_str(&nested["address"], "nested.address").starts_with("0x"));
                assert_eq!(
                    json_hex_u128(&nested["postBalance"], "nested.postBalance"),
                    0
                );
                assert_eq!(json_hex_u128(&nested["postNonce"], "nested.postNonce"), 0);
                assert!(
                    nested["postStorageFacts"]
                        .as_object()
                        .expect("nested.postStorageFacts")
                        .len()
                        >= 5
                );
                assert!(
                    runtime_state
                        .get_account(&decode_hex_bytes(json_str(
                            &nested["address"],
                            "nested.address"
                        )))
                        .is_none(),
                    "nested STATICCALL/precompile account state remains host/AOEM responsibility"
                );
            }

            let bal = outcome
                .observed_block_access_list
                .block_access_list
                .expect("official STATICCALL/precompile/return observed BAL");
            let sender_entry = bal
                .0
                .iter()
                .find(|entry| entry.address.as_slice() == tx.from.as_slice())
                .expect("sender BAL entry");
            assert_eq!(
                sender_entry.balance_changes[0].post_balance,
                NovoVmAdapter::u128_to_be32_v1(post_sender_balance)
            );
            if value_transferred && value > 0 {
                let to_entry = bal
                    .0
                    .iter()
                    .find(|entry| {
                        entry.address.as_slice() == tx.to.as_ref().expect("call target").as_slice()
                    })
                    .expect("value-transfer target BAL entry");
                assert_eq!(
                    to_entry.balance_changes[0].post_balance,
                    NovoVmAdapter::u128_to_be32_v1(post_to_balance)
                );
            }

            match mode {
                "staticPrecompileProjection" => static_precompile_cases += 1,
                "returnDataProjection" => return_projection_cases += 1,
                _ => panic!("unexpected official STATICCALL/precompile/return mode {mode}"),
            }
            match label {
                "staticcallPrecompileFromTransaction" => {
                    direct_precompile_suite_gas = Some(expected_gas_used);
                    assert_eq!(
                        json_str(&case["postStorageFacts"]["0x05"], "sha256 suite slot"),
                        "0x73f5062fb68ed2a1ec82ff8c73f9251bb9cf53a623bc93527e16bc5ae29dad74"
                    );
                }
                "staticcallPrecompileFromCalledContract" => {
                    called_precompile_suite_gas = Some(expected_gas_used);
                }
                "staticCallSha256" => {
                    sha256_gas = Some(expected_gas_used);
                    assert_eq!(
                        json_str(&case["postStorageFacts"]["0x00"], "sha256 output"),
                        json_str(&case["dataSha256"], "sha256 empty input")
                    );
                }
                "staticCallIdentity" => {
                    identity_gas = Some(expected_gas_used);
                    assert_eq!(
                        json_str(&case["postStorageFacts"]["0x00"], "identity output"),
                        "0xf34578907f"
                    );
                }
                "staticCallRipemd160" => {
                    ripemd160_gas = Some(expected_gas_used);
                    assert_eq!(
                        json_str(&case["postStorageFacts"]["0x00"], "ripemd160 output"),
                        "0x9c1185a5c5e9fc54612808977ee8f548b2258d31"
                    );
                }
                "staticCallEcrecover" => {
                    ecrecover_gas = Some(expected_gas_used);
                    assert_eq!(
                        json_str(&case["postStorageFacts"]["0x00"], "ecrecover output"),
                        sender
                    );
                }
                "staticReturnTest2" => return_test_gas = Some(expected_gas_used),
                "staticCallToReturn1" => call_to_return_gas = Some(expected_gas_used),
                "staticCallOutput3partial" => {
                    call_output_partial_gas = Some(expected_gas_used);
                    assert_eq!(
                        json_str(&case["postStorageFacts"]["0x00"], "partial output").len(),
                        66
                    );
                }
                _ => panic!("unexpected official STATICCALL/precompile/return label {label}"),
            }
        }

        assert_eq!(static_precompile_cases, 6);
        assert_eq!(return_projection_cases, 3);
        assert_eq!(nested_account_fact_cases, 1);
        let direct_precompile_suite_gas =
            direct_precompile_suite_gas.expect("direct precompile suite gas");
        let called_precompile_suite_gas =
            called_precompile_suite_gas.expect("called precompile suite gas");
        let sha256_gas = sha256_gas.expect("sha256 gas");
        let identity_gas = identity_gas.expect("identity gas");
        let ripemd160_gas = ripemd160_gas.expect("ripemd160 gas");
        let ecrecover_gas = ecrecover_gas.expect("ecrecover gas");
        let return_test_gas = return_test_gas.expect("return test gas");
        let call_to_return_gas = call_to_return_gas.expect("call-to-return gas");
        let call_output_partial_gas = call_output_partial_gas.expect("partial output gas");

        assert!(called_precompile_suite_gas > direct_precompile_suite_gas);
        assert!(direct_precompile_suite_gas > ecrecover_gas);
        assert!(ecrecover_gas > ripemd160_gas);
        assert!(ripemd160_gas > sha256_gas);
        assert!(sha256_gas > identity_gas);
        assert!(call_output_partial_gas > return_test_gas);
        assert!(return_test_gas > call_to_return_gas);
    }

    #[test]
    fn official_state_fixture_log_receipt_grouped_projection_v1() {
        let fixture = official_log_receipt_state_fixture_v1();
        assert_eq!(fixture["source"]["repo"].as_str(), Some("ethereum/tests"));
        assert_eq!(
            fixture["source"]["fixtureFormat"].as_str(),
            Some("state_test")
        );
        assert_eq!(fixture["source"]["fork"].as_str(), Some("Cancun"));

        let empty_logs_hash = json_str(&fixture["emptyLogsHash"], "emptyLogsHash");
        let cases = fixture["cases"].as_array().expect("LOG/receipt cases");
        assert_eq!(cases.len(), 4);

        let mut log_success_cases = 0usize;
        let mut no_log_cases = 0usize;
        let mut empty_mem_cases = 0usize;
        let mut non_empty_mem_cases = 0usize;
        let mut topic_count_sum = 0usize;
        let mut empty_topic_gas = Vec::new();
        let mut non_empty_topic_gas = Vec::new();
        let mut non_empty_logs_hashes = std::collections::HashSet::new();

        for case in cases {
            let label = json_str(&case["label"], "case.label");
            let mode = json_str(&case["mode"], "case.mode");
            assert!(
                json_str(&case["fixtureKey"], "case.fixtureKey").contains("-fork_[Cancun-Prague]"),
                "official LOG/receipt case must be the Cancun/Prague projection"
            );
            assert!(
                json_str(&case["postHash"], "case.postHash").starts_with("0x"),
                "official LOG/receipt case must carry post hash"
            );
            let logs_hash = json_str(&case["logsHash"], "case.logsHash");
            let logs_expected = case["logsExpected"]
                .as_bool()
                .expect("case.logsExpected bool");
            assert_eq!(logs_expected, logs_hash != empty_logs_hash);
            if logs_expected {
                assert!(non_empty_logs_hashes.insert(logs_hash.to_string()));
            }

            let sender = json_str(&case["sender"], "case.sender");
            let to = json_str(&case["to"], "case.to");
            let pre_sender_balance =
                json_hex_u128(&case["preSenderBalance"], "case.preSenderBalance");
            let pre_sender_nonce =
                json_hex_u128(&case["preSenderNonce"], "case.preSenderNonce") as u64;
            let post_sender_balance =
                json_hex_u128(&case["postSenderBalance"], "case.postSenderBalance");
            let pre_to_balance = json_hex_u128(&case["preToBalance"], "case.preToBalance");
            let pre_to_nonce = json_hex_u128(&case["preToNonce"], "case.preToNonce") as u64;
            let post_to_balance = json_hex_u128(&case["postToBalance"], "case.postToBalance");
            let post_to_nonce = json_hex_u128(&case["postToNonce"], "case.postToNonce") as u64;
            let gas_price = json_hex_u128(&case["gasPrice"], "case.gasPrice");
            let gas_limit = json_hex_u128(&case["gasLimit"], "case.gasLimit");
            let value = json_hex_u128(&case["value"], "case.value");
            let nonce = json_hex_u128(&case["nonce"], "case.nonce") as u64;
            assert_eq!(nonce, pre_sender_nonce);

            let raw_tx = decode_hex_bytes(json_str(&case["txbytes"], "case.txbytes"));
            let recovered_sender = recover_raw_evm_tx_sender_m0(&raw_tx)
                .expect("official LOG/receipt raw sender recovery")
                .expect("official LOG/receipt recovered sender");
            assert_eq!(recovered_sender, decode_hex_bytes(sender));
            let fields =
                translate_raw_evm_tx_fields_m0(&raw_tx).expect("official LOG/receipt tx decode");
            let tx = tx_ir_from_raw_fields_m0(&fields, &raw_tx, recovered_sender, 1);

            assert_eq!(
                tx.tx_type,
                TxType::Transfer,
                "raw empty-calldata LOG fixture tx is state-agnostic before execution"
            );
            assert_eq!(tx.from, decode_hex_bytes(sender));
            assert_eq!(tx.to, Some(decode_hex_bytes(to)));
            assert_eq!(tx.value, value);
            assert_eq!(tx.gas_limit, gas_limit as u64);
            assert_eq!(tx.gas_price, gas_price as u64);
            assert_eq!(tx.nonce, nonce);
            assert_eq!(
                tx.data,
                decode_hex_bytes(json_str(&case["data"], "case.data"))
            );

            let mut adapter = NovoVmAdapter::new(ChainConfig {
                chain_type: ChainType::EVM,
                chain_id: 1,
                name: format!("official-log-receipt-{label}"),
                enabled: true,
                custom_config: None,
            });
            adapter.initialize().expect("init");
            assert!(
                adapter
                    .verify_transaction(&tx)
                    .expect("verify official LOG/receipt raw tx"),
                "official LOG/receipt txbytes must verify through raw EVM path"
            );
            for historical_nonce in 0..pre_sender_nonce {
                let mut historical_tx = tx.clone();
                historical_tx.nonce = historical_nonce;
                adapter
                    .route_transaction_through_unified_account(&historical_tx)
                    .expect("prime official LOG/receipt pre-state nonce");
            }

            let value_transferred = post_to_balance == pre_to_balance + value;
            let charged_fee = pre_sender_balance
                .saturating_sub(post_sender_balance)
                .saturating_sub(if value_transferred { value } else { 0 });
            assert_eq!(charged_fee % gas_price, 0);
            let gas_used = charged_fee / gas_price;
            let expected_gas_used = case["gasUsed"].as_u64().expect("case.gasUsed");
            assert_eq!(gas_used, expected_gas_used as u128);

            let mut runtime_state = StateIR::new();
            runtime_state.set_account(
                tx.from.clone(),
                AccountState {
                    balance: pre_sender_balance,
                    nonce: pre_sender_nonce,
                    code_hash: None,
                    storage_root: vec![0u8; 32],
                },
            );
            runtime_state.set_account(
                tx.to.clone().expect("call target"),
                AccountState {
                    balance: pre_to_balance,
                    nonce: pre_to_nonce,
                    code_hash: Some(vec![0xcc; 32]),
                    storage_root: vec![0u8; 32],
                },
            );
            assert_eq!(
                NovoVmAdapter::effective_execution_tx_v1(&tx, &runtime_state).tx_type,
                TxType::ContractCall,
                "target code in pre-state must promote LOG fixture tx to contract call execution"
            );

            let mut artifact = sample_aoem_artifact(&tx, true, [0x89; 32], None);
            artifact.gas_used = expected_gas_used;
            artifact.cumulative_gas_used = expected_gas_used;
            artifact.effective_gas_price = Some(gas_price as u64);
            artifact.receipt_type = None;
            artifact.anchor = None;
            if logs_expected {
                let topic_count =
                    case["logTopicCount"].as_u64().expect("case.logTopicCount") as usize;
                topic_count_sum += topic_count;
                let topics = (0..topic_count)
                    .map(|idx| {
                        let mut topic = [0u8; 32];
                        topic[0] = topic_count as u8;
                        topic[31] = idx as u8;
                        topic
                    })
                    .collect::<Vec<_>>();
                let data = if json_str(&case["logDataKind"], "case.logDataKind") == "nonEmpty" {
                    vec![0x4c, 0x4f, 0x47]
                } else {
                    Vec::new()
                };
                artifact.event_logs = vec![AoemEventLogV1 {
                    emitter: tx.to.clone().expect("log emitter"),
                    topics,
                    data,
                    log_index: 0,
                }];
                artifact.log_bloom = vec![0x44; AOEM_LOG_BLOOM_BYTES_V1];
            } else {
                artifact.event_logs.clear();
                artifact.log_bloom = vec![0u8; AOEM_LOG_BLOOM_BYTES_V1];
            }

            let outcome = adapter
                .execute_transaction_with_observed_metadata_v1(
                    &tx,
                    &mut runtime_state,
                    Some(&artifact),
                )
                .expect("execute official LOG/receipt fixture projection");

            assert!(outcome.artifact.status_ok);
            assert_eq!(outcome.artifact.gas_used, expected_gas_used);
            assert_eq!(
                runtime_state.get_account(&tx.from).map(|acc| acc.balance),
                Some(post_sender_balance)
            );
            assert_eq!(
                runtime_state
                    .get_account(tx.to.as_ref().expect("call target"))
                    .map(|acc| acc.balance),
                Some(post_to_balance)
            );
            assert_eq!(
                runtime_state
                    .get_account(tx.to.as_ref().expect("call target"))
                    .map(|acc| acc.nonce),
                Some(post_to_nonce)
            );

            if logs_expected {
                log_success_cases += 1;
                assert_eq!(outcome.artifact.event_logs.len(), 1);
                let event_log = &outcome.artifact.event_logs[0];
                assert_eq!(
                    event_log.topics.len(),
                    case["logTopicCount"].as_u64().expect("case.logTopicCount") as usize
                );
                let non_empty_data =
                    json_str(&case["logDataKind"], "case.logDataKind") == "nonEmpty";
                assert_eq!(!event_log.data.is_empty(), non_empty_data);
                assert!(outcome.artifact.log_bloom.iter().any(|byte| *byte != 0));
                assert!(
                    runtime_state
                        .get_storage(&tx.from, b"aoem:last_event_logs")
                        .is_some(),
                    "AOEM log projection must be persisted for non-empty official logs hash cases"
                );
            } else {
                no_log_cases += 1;
                assert!(outcome.artifact.event_logs.is_empty());
                assert!(outcome.artifact.log_bloom.iter().all(|byte| *byte == 0));
                assert!(runtime_state
                    .get_storage(&tx.from, b"aoem:last_event_logs")
                    .is_none());
            }
            assert_eq!(
                runtime_state.get_storage(&tx.from, b"aoem:last_log_bloom"),
                Some(&outcome.artifact.log_bloom)
            );

            let post_storage_facts = case["postStorageFacts"]
                .as_object()
                .expect("case.postStorageFacts");
            match mode {
                "logSuccessProjection" => {
                    assert!(post_storage_facts.is_empty());
                    if json_str(&case["logDataKind"], "case.logDataKind") == "empty" {
                        empty_mem_cases += 1;
                        empty_topic_gas.push((
                            case["logTopicCount"].as_u64().expect("topic count"),
                            expected_gas_used,
                        ));
                    } else {
                        non_empty_mem_cases += 1;
                        non_empty_topic_gas.push((
                            case["logTopicCount"].as_u64().expect("topic count"),
                            expected_gas_used,
                        ));
                    }
                }
                "oogNoLogProjection" => {
                    assert!(post_storage_facts.is_empty());
                }
                _ => panic!("unexpected official LOG/receipt mode {mode}"),
            }

            let bal = outcome
                .observed_block_access_list
                .block_access_list
                .expect("official LOG/receipt observed BAL");
            let sender_entry = bal
                .0
                .iter()
                .find(|entry| entry.address.as_slice() == tx.from.as_slice())
                .expect("sender BAL entry");
            assert_eq!(
                sender_entry.balance_changes[0].post_balance,
                NovoVmAdapter::u128_to_be32_v1(post_sender_balance)
            );
            if value_transferred && value > 0 {
                let to_entry = bal
                    .0
                    .iter()
                    .find(|entry| {
                        entry.address.as_slice() == tx.to.as_ref().expect("call target").as_slice()
                    })
                    .expect("value-transfer target BAL entry");
                assert_eq!(
                    to_entry.balance_changes[0].post_balance,
                    NovoVmAdapter::u128_to_be32_v1(post_to_balance)
                );
            }
        }

        assert_eq!(log_success_cases, 4);
        assert_eq!(no_log_cases, 0);
        assert_eq!(empty_mem_cases, 0);
        assert_eq!(non_empty_mem_cases, 4);
        assert_eq!(topic_count_sum, 6);
        assert_eq!(non_empty_logs_hashes.len(), 4);
        empty_topic_gas.sort_by_key(|(topic_count, _)| *topic_count);
        non_empty_topic_gas.sort_by_key(|(topic_count, _)| *topic_count);
        assert!(empty_topic_gas.is_empty());
        assert_eq!(non_empty_topic_gas.len(), 4);
        for window in non_empty_topic_gas.windows(2) {
            assert_eq!(window[1].1 - window[0].1, 478);
        }
    }

    #[test]
    fn execute_tracked_sender_rejects_insufficient_value_fee_v1() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let tx = sample_tx(TxType::Transfer);
        let mut runtime_state = StateIR::new();
        fund_sender_for_test(&mut runtime_state, &tx, 1);

        let err = adapter
            .execute_transaction_with_artifact(&tx, &mut runtime_state, None)
            .expect_err("insufficient sender balance must reject before nonce bump");

        assert!(err.to_string().contains("evm tx insufficient balance"));
        assert_eq!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(1)
        );
        assert_eq!(adapter.get_nonce(&tx.from).expect("nonce"), 0);
    }

    #[test]
    fn execute_transaction_with_observed_metadata_emits_complete_contract_call_evm_bal() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let tx = sample_tx(TxType::ContractCall);
        let mut runtime_state = StateIR::new();
        let outcome = adapter
            .execute_transaction_with_observed_metadata_v1(&tx, &mut runtime_state, None)
            .expect("execute with observed metadata");

        assert!(outcome.artifact.status_ok);
        assert!(
            outcome
                .observed_block_access_list
                .block_access_list_complete
        );
        let block_access_list = outcome
            .observed_block_access_list
            .block_access_list
            .expect("observed block access list");
        let target = tx.to.clone().expect("target");
        let target_entry = block_access_list
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == target.as_slice())
            .expect("target entry");
        assert_eq!(target_entry.storage_changes.len(), 1);
        assert_eq!(
            target_entry.storage_changes[0].slot,
            NovoVmAdapter::bytes_to_bal_word32_v1(b"novovm-bal-slot-v1", &tx.nonce.to_le_bytes())
        );
        assert_eq!(
            target_entry.storage_changes[0].slot_changes[0].post_value,
            NovoVmAdapter::bytes_to_bal_word32_v1(b"novovm-bal-storage-value-v1", &tx.hash)
        );
    }

    #[test]
    fn evm_execution_spec_create_address_derivation_matches_geth_v1() {
        let from = decode_hex_bytes("970e8128ab834e8eac17ab8e3812f010678cf791");
        assert_eq!(
            NovoVmAdapter::derive_contract_address(&from, 0),
            decode_hex_bytes("333c3310824b7c685133f2bedb2ca4b8b4df633d")
        );
        assert_eq!(
            NovoVmAdapter::derive_contract_address(&from, 1),
            decode_hex_bytes("8bda78331c916a08481428e4b07c96d3e916d165")
        );
        assert_eq!(
            NovoVmAdapter::derive_contract_address(&from, 2),
            decode_hex_bytes("c9ddedf451bc62ce88bf9292afb13df35b670699")
        );
    }

    #[test]
    fn execute_transaction_with_observed_metadata_emits_complete_contract_deploy_evm_bal() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let tx = sample_tx(TxType::ContractDeploy);
        let expected_contract = NovoVmAdapter::derive_contract_address(&tx.from, tx.nonce);
        let mut runtime_state = StateIR::new();
        let outcome = adapter
            .execute_transaction_with_observed_metadata_v1(&tx, &mut runtime_state, None)
            .expect("execute with observed metadata");

        assert!(outcome.artifact.status_ok);
        assert!(
            outcome
                .observed_block_access_list
                .block_access_list_complete
        );
        let contract = outcome
            .artifact
            .contract_address
            .clone()
            .expect("contract address");
        assert_eq!(contract, expected_contract);
        assert!(runtime_state.get_account(&expected_contract).is_some());
        let block_access_list = outcome
            .observed_block_access_list
            .block_access_list
            .expect("observed block access list");
        let contract_entry = block_access_list
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == contract.as_slice())
            .expect("contract entry");
        assert_eq!(contract_entry.balance_changes.len(), 1);
        assert_eq!(contract_entry.code_changes.len(), 1);
        assert!(
            contract_entry.storage_changes.len() >= 2,
            "deploy init/runtime code hash storage changes must be observed"
        );
        let slots = contract_entry
            .storage_changes
            .iter()
            .map(|item| item.slot)
            .collect::<Vec<_>>();
        assert!(slots.contains(&NovoVmAdapter::bytes_to_bal_word32_v1(
            b"novovm-bal-slot-v1",
            b"deploy:init_code_hash"
        )));
        assert!(slots.contains(&NovoVmAdapter::bytes_to_bal_word32_v1(
            b"novovm-bal-slot-v1",
            b"deploy:runtime_code_hash"
        )));
    }

    #[test]
    fn evm_adapter_balance_fee_access_storage_surface_smoke_v1() {
        execute_transfer_debits_tracked_sender_value_and_fee_v1();
        execute_failed_call_debits_fee_without_value_transfer_v1();
        execute_success_call_debits_refunded_sstore_gas_used_v1();
        evm_execution_spec_sstore_refund_cap_fee_debit_v1();
        evm_execution_spec_account_balance_value_fee_invariants_v1();
        evm_execution_spec_effective_gas_price_fee_debit_v1();
        official_state_fixture_eip1559_sender_balance_fee_debit_v1();
        official_state_fixture_sload_warm_cold_fee_debit_v1();
        official_state_fixture_sstore_refund_cap_fee_debit_v1();
        official_state_fixture_failure_account_fee_debit_v1();
        official_state_fixture_create_account_grouped_projection_v1();
        official_state_fixture_staticcall_precompile_return_grouped_projection_v1();
        official_state_fixture_log_receipt_grouped_projection_v1();
        execute_tracked_sender_rejects_insufficient_value_fee_v1();
        tx_intrinsic_gas_includes_type1_access_list_extras_v1();
        execute_raw_type1_access_list_emits_declared_storage_reads_v1();
        evm_execution_spec_access_list_warm_storage_smoke_v1();
        evm_execution_spec_sload_warm_cold_fee_debit_v1();
        evm_execution_spec_create_address_derivation_matches_geth_v1();
        execute_transaction_with_observed_metadata_emits_complete_contract_call_evm_bal();
        execute_transaction_with_observed_metadata_emits_complete_contract_deploy_evm_bal();
        evm_execution_spec_create_existing_account_collision_invariants_v1();
        evm_execution_spec_create2_artifact_collision_invariants_v1();
    }

    #[test]
    fn execute_transaction_with_failed_call_artifact_records_failure_without_value_transfer() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let tx = sample_tx(TxType::ContractCall);
        assert!(adapter.verify_transaction(&tx).expect("verify call"));
        let mut runtime_state = StateIR::new();
        let initial_balance = 100_000u128;
        runtime_state.set_account(
            tx.from.clone(),
            AccountState {
                balance: initial_balance,
                nonce: 0,
                code_hash: None,
                storage_root: vec![0u8; 32],
            },
        );
        adapter.state.set_account(
            tx.from.clone(),
            AccountState {
                balance: initial_balance,
                nonce: 0,
                code_hash: None,
                storage_root: vec![0u8; 32],
            },
        );
        let artifact = sample_aoem_artifact(&tx, false, [0x55; 32], None);
        let expected_fee = NovoVmAdapter::execution_fee_wei_v1(&tx, Some(&artifact), false);

        adapter
            .execute_transaction_with_artifact(&tx, &mut runtime_state, Some(&artifact))
            .expect("execute failed call artifact");

        let target = tx.to.clone().expect("call target");
        assert_eq!(
            runtime_state
                .get_account(&target)
                .map(|acc| acc.balance)
                .unwrap_or(0),
            0
        );
        assert_eq!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(initial_balance - expected_fee)
        );
        assert_eq!(
            runtime_state
                .get_account(&NovoVmAdapter::subject_state_key(
                    NovoVmAdapter::fallback_adapter_account_id(&tx.from).as_str(),
                ))
                .map(|acc| acc.nonce)
                .unwrap_or(0),
            1
        );
        assert_eq!(
            adapter.state_root().expect("state root"),
            [0x55; 32].to_vec()
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"aoem:last_revert_data"),
            Some(&vec![0xde, 0xad])
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"aoem:last_failure_class"),
            Some(&b"revert".to_vec())
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"aoem:last_failure_class_source"),
            Some(&b"heuristic_revert_data".to_vec())
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"aoem:last_failure_recoverability"),
            Some(&b"recoverable".to_vec())
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"aoem:last_failure_classification_contract"),
            Some(&AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1.as_bytes().to_vec())
        );
    }

    #[test]
    fn execute_transaction_with_failed_deploy_artifact_records_failure_contract() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let tx = sample_tx(TxType::ContractDeploy);
        assert!(adapter.verify_transaction(&tx).expect("verify deploy"));
        let mut runtime_state = StateIR::new();
        let mut artifact = sample_aoem_artifact(&tx, false, [0x66; 32], None);
        artifact.revert_data = None;
        artifact.gas_used = tx.gas_limit;
        artifact.cumulative_gas_used = tx.gas_limit;
        if let Some(anchor) = artifact.anchor.as_mut() {
            anchor.return_code = 13;
            anchor.return_code_name = "out_of_gas".to_string();
        }

        adapter
            .execute_transaction_with_artifact(&tx, &mut runtime_state, Some(&artifact))
            .expect("execute failed deploy artifact");

        assert_eq!(
            runtime_state.get_storage(&tx.from, b"aoem:last_contract_deploy_failure"),
            Some(&NovoVmAdapter::tx_hash_or_compute(&tx))
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"aoem:last_failure_class"),
            Some(&b"out_of_gas".to_vec())
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"aoem:last_failure_class_source"),
            Some(&b"anchor_return_code".to_vec())
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"aoem:last_failure_recoverability"),
            Some(&b"recoverable".to_vec())
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"aoem:last_failure_classification_contract"),
            Some(&AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1.as_bytes().to_vec())
        );
    }

    #[test]
    fn evm_execution_spec_create_call_failure_state_invariants_v1() {
        evm_execution_spec_create_existing_account_collision_invariants_v1();
        evm_execution_spec_create2_artifact_collision_invariants_v1();

        let mut call_adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "call-failure-invariants".to_string(),
            enabled: true,
            custom_config: None,
        });
        call_adapter.initialize().expect("init call adapter");

        let mut call_tx = sample_tx(TxType::ContractCall);
        call_tx.value = 17;
        call_tx.gas_limit = 120_000;
        call_tx = resign_tx(call_tx);
        let call_target = call_tx.to.clone().expect("call target");
        let mut call_state = StateIR::new();
        fund_sender_for_test(&mut call_state, &call_tx, 1_000_000);
        call_state.set_account(
            call_target.clone(),
            AccountState {
                balance: 5,
                nonce: 0,
                code_hash: None,
                storage_root: vec![0u8; 32],
            },
        );
        call_state.set_storage(call_target.clone(), b"existing".to_vec(), vec![0x7a]);

        let mut call_artifact = sample_aoem_artifact(&call_tx, false, [0x61; 32], None);
        call_artifact.revert_data = None;
        if let Some(anchor) = call_artifact.anchor.as_mut() {
            anchor.return_code = 2001;
            anchor.return_code_name = "engine_exec_failed".to_string();
        }

        let call_outcome = call_adapter
            .execute_transaction_with_observed_metadata_v1(
                &call_tx,
                &mut call_state,
                Some(&call_artifact),
            )
            .expect("execute failed call");
        assert!(!call_outcome.artifact.status_ok);
        assert!(call_outcome.artifact.revert_data.is_none());
        assert!(call_outcome.artifact.event_logs.is_empty());
        assert!(call_outcome
            .artifact
            .log_bloom
            .iter()
            .all(|byte| *byte == 0));
        assert_eq!(
            call_state.get_account(&call_target).map(|acc| acc.balance),
            Some(5),
            "failed CALL must not transfer value"
        );
        assert_eq!(
            call_state.get_storage(&call_target, b"existing"),
            Some(&vec![0x7a]),
            "failed CALL must preserve existing target storage"
        );
        assert_eq!(
            call_state.get_storage(&call_target, &call_tx.nonce.to_le_bytes()),
            None,
            "failed CALL must not commit call storage writes"
        );
        assert_eq!(
            call_state.get_storage(&call_tx.from, b"aoem:last_failure_class"),
            Some(&b"execution_failed".to_vec())
        );
        let call_bal = call_outcome
            .observed_block_access_list
            .block_access_list
            .expect("failed call observed BAL");
        let call_target_entry = call_bal
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == call_target.as_slice())
            .expect("failed call target account read");
        assert!(call_target_entry.storage_changes.is_empty());

        let mut deploy_adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "create-failure-invariants".to_string(),
            enabled: true,
            custom_config: None,
        });
        deploy_adapter.initialize().expect("init deploy adapter");

        let mut deploy_tx = sample_tx(TxType::ContractDeploy);
        deploy_tx.value = 19;
        deploy_tx.gas_limit = 120_000;
        deploy_tx = resign_tx(deploy_tx);
        let failed_contract = vec![0x66u8; 20];
        let mut deploy_state = StateIR::new();
        fund_sender_for_test(&mut deploy_state, &deploy_tx, 1_000_000);
        let mut deploy_artifact =
            sample_aoem_artifact(&deploy_tx, false, [0x62; 32], Some(failed_contract.clone()));
        deploy_artifact.revert_data = None;
        deploy_artifact.runtime_code = Some(vec![0x60, 0x00, 0x60, 0x01]);
        deploy_artifact.runtime_code_hash = Some(vec![0x99; 32]);
        if let Some(anchor) = deploy_artifact.anchor.as_mut() {
            anchor.return_code = 13;
            anchor.return_code_name = "out_of_gas".to_string();
        }

        let deploy_outcome = deploy_adapter
            .execute_transaction_with_observed_metadata_v1(
                &deploy_tx,
                &mut deploy_state,
                Some(&deploy_artifact),
            )
            .expect("execute failed create");
        assert!(!deploy_outcome.artifact.status_ok);
        assert!(deploy_outcome.artifact.contract_address.is_none());
        assert!(deploy_outcome.artifact.runtime_code.is_none());
        assert!(deploy_outcome.artifact.runtime_code_hash.is_none());
        assert!(
            deploy_state.get_account(&failed_contract).is_none(),
            "failed CREATE must not create the contract account"
        );
        for slot in [
            b"deploy:init_code_hash".as_slice(),
            b"deploy:runtime_code_hash".as_slice(),
            b"deploy:runtime_code".as_slice(),
            NovoVmAdapter::runtime_code_storage_key(),
        ] {
            assert_eq!(
                deploy_state.get_storage(&failed_contract, slot),
                None,
                "failed CREATE must not commit contract storage"
            );
        }
        assert_eq!(
            deploy_state.get_storage(&deploy_tx.from, b"deploy:last_failed_contract_address"),
            Some(&failed_contract)
        );
        let deploy_bal = deploy_outcome
            .observed_block_access_list
            .block_access_list
            .expect("failed create observed BAL");
        assert!(
            deploy_bal
                .0
                .iter()
                .all(|entry| entry.address.as_slice() != failed_contract.as_slice()),
            "failed CREATE must not emit a contract BAL account entry"
        );
    }

    #[test]
    fn execute_transaction_with_successful_deploy_artifact_rejects_eip7610_account_mainnet() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let tx = sample_tx(TxType::ContractDeploy);
        assert!(adapter.verify_transaction(&tx).expect("verify deploy"));
        let mut runtime_state = StateIR::new();
        let rejected_contract = sample_eip7610_rejected_mainnet_address_v1();
        let artifact = sample_aoem_artifact(&tx, true, [0x77; 32], Some(rejected_contract.clone()));

        let resolved = adapter
            .execute_transaction_with_artifact(&tx, &mut runtime_state, Some(&artifact))
            .expect("execute deploy with eip7610-rejected address");

        assert!(!resolved.status_ok);
        assert_eq!(resolved.contract_address, None);
        assert_eq!(
            runtime_state
                .get_account(&rejected_contract)
                .map(|it| it.code_hash.clone()),
            None
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"aoem:last_failure_class"),
            Some(&b"invalid".to_vec())
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"aoem:last_failure_recoverability"),
            Some(&b"non_recoverable".to_vec())
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"deploy:last_reject_reason"),
            Some(&b"eip7610_rejected_account".to_vec())
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"deploy:last_reject_contract"),
            Some(&EIP7610_REJECTION_CONTRACT_V1.as_bytes().to_vec())
        );
    }

    #[test]
    fn evm_execution_spec_create_existing_account_collision_invariants_v1() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "create-existing-account-collision".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let mut tx = sample_tx(TxType::ContractDeploy);
        tx.value = 17;
        tx.gas_limit = 120_000;
        tx.gas_price = 4;
        tx = resign_tx(tx);
        let collision_contract = vec![0x79u8; 20];
        let mut runtime_state = StateIR::new();
        let initial_sender_balance = 1_000_000u128;
        let existing_contract_balance = 123u128;
        let existing_code_hash = vec![0xaau8; 32];
        let existing_runtime_code = vec![0x60, 0xfe];
        fund_sender_for_test(&mut runtime_state, &tx, initial_sender_balance);
        runtime_state.set_account(
            collision_contract.clone(),
            AccountState {
                balance: existing_contract_balance,
                nonce: 1,
                code_hash: Some(existing_code_hash.clone()),
                storage_root: vec![0x11u8; 32],
            },
        );
        runtime_state.set_storage(
            collision_contract.clone(),
            b"deploy:runtime_code_hash".to_vec(),
            existing_code_hash.clone(),
        );
        runtime_state.set_storage(
            collision_contract.clone(),
            b"deploy:runtime_code".to_vec(),
            existing_runtime_code.clone(),
        );
        runtime_state.set_storage(
            collision_contract.clone(),
            NovoVmAdapter::runtime_code_storage_key().to_vec(),
            existing_runtime_code.clone(),
        );

        let mut artifact =
            sample_aoem_artifact(&tx, true, [0x79; 32], Some(collision_contract.clone()));
        artifact.gas_used = 43_000;
        artifact.cumulative_gas_used = 43_000;
        artifact.effective_gas_price = Some(tx.gas_price);
        artifact.runtime_code = Some(vec![0x60, 0x00, 0x60, 0x01]);
        artifact.runtime_code_hash = Some(vec![0xbbu8; 32]);
        let expected_fee = NovoVmAdapter::execution_fee_wei_v1(&tx, Some(&artifact), false);
        let expected_sender_balance = initial_sender_balance - expected_fee;

        let outcome = adapter
            .execute_transaction_with_observed_metadata_v1(&tx, &mut runtime_state, Some(&artifact))
            .expect("execute colliding create artifact");

        assert!(!outcome.artifact.status_ok);
        assert_eq!(outcome.artifact.contract_address, None);
        assert_eq!(outcome.artifact.runtime_code, None);
        assert_eq!(outcome.artifact.runtime_code_hash, None);
        assert_eq!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(expected_sender_balance)
        );
        let post_contract = runtime_state
            .get_account(&collision_contract)
            .expect("existing contract must remain");
        assert_eq!(post_contract.balance, existing_contract_balance);
        assert_eq!(post_contract.nonce, 1);
        assert_eq!(post_contract.code_hash, Some(existing_code_hash.clone()));
        assert_eq!(
            runtime_state.get_storage(&collision_contract, b"deploy:runtime_code_hash"),
            Some(&existing_code_hash)
        );
        assert_eq!(
            runtime_state.get_storage(&collision_contract, b"deploy:runtime_code"),
            Some(&existing_runtime_code)
        );
        assert_eq!(
            runtime_state.get_storage(
                &collision_contract,
                NovoVmAdapter::runtime_code_storage_key()
            ),
            Some(&existing_runtime_code)
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"deploy:last_failed_contract_address"),
            Some(&collision_contract)
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"deploy:last_reject_reason"),
            Some(&b"create_collision_existing_account".to_vec())
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"deploy:last_reject_contract"),
            Some(&collision_contract)
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"aoem:last_failure_class"),
            Some(&b"execution_failed".to_vec())
        );

        let block_access_list = outcome
            .observed_block_access_list
            .block_access_list
            .expect("create collision observed BAL");
        let sender_entry = block_access_list
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == tx.from.as_slice())
            .expect("sender BAL entry");
        assert_eq!(
            sender_entry.balance_changes[0].post_balance,
            NovoVmAdapter::u128_to_be32_v1(expected_sender_balance)
        );
        assert!(
            block_access_list
                .0
                .iter()
                .all(|entry| entry.address.as_slice() != collision_contract.as_slice()),
            "colliding CREATE must not emit a contract BAL account entry"
        );

        let adapter_contract = adapter
            .state
            .get_account(&collision_contract)
            .expect("adapter state keeps existing contract");
        assert_eq!(adapter_contract.balance, existing_contract_balance);
        assert_eq!(adapter_contract.code_hash, Some(existing_code_hash));
        assert_eq!(
            adapter
                .state
                .get_storage(&collision_contract, b"deploy:runtime_code"),
            Some(&existing_runtime_code)
        );
    }

    #[test]
    fn evm_execution_spec_create2_artifact_collision_invariants_v1() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "create2-artifact-collision".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let mut tx = sample_tx(TxType::ContractDeploy);
        tx.value = 29;
        tx.gas_limit = 120_000;
        tx.gas_price = 5;
        tx = resign_tx(tx);
        let empty_init_code_hash =
            decode_hex_bytes("c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470");
        let create2_contract = derive_create2_contract_address_m0(
            &tx.from,
            &decode_hex_bytes("cafebabe"),
            &empty_init_code_hash,
        );
        assert_eq!(create2_contract.len(), 20);

        let mut runtime_state = StateIR::new();
        let initial_sender_balance = 1_000_000u128;
        let existing_contract_balance = 456u128;
        let existing_code_hash = vec![0xccu8; 32];
        let existing_runtime_code = vec![0x60, 0x42, 0x00];
        fund_sender_for_test(&mut runtime_state, &tx, initial_sender_balance);
        runtime_state.set_account(
            create2_contract.clone(),
            AccountState {
                balance: existing_contract_balance,
                nonce: 1,
                code_hash: Some(existing_code_hash.clone()),
                storage_root: vec![0x22u8; 32],
            },
        );
        runtime_state.set_storage(
            create2_contract.clone(),
            b"deploy:runtime_code_hash".to_vec(),
            existing_code_hash.clone(),
        );
        runtime_state.set_storage(
            create2_contract.clone(),
            b"deploy:runtime_code".to_vec(),
            existing_runtime_code.clone(),
        );

        let mut artifact =
            sample_aoem_artifact(&tx, true, [0x7a; 32], Some(create2_contract.clone()));
        artifact.gas_used = 44_000;
        artifact.cumulative_gas_used = 44_000;
        artifact.effective_gas_price = Some(tx.gas_price);
        artifact.runtime_code = Some(vec![0x60, 0x00, 0x60, 0x01]);
        artifact.runtime_code_hash = Some(vec![0xddu8; 32]);
        let expected_fee = NovoVmAdapter::execution_fee_wei_v1(&tx, Some(&artifact), false);
        let expected_sender_balance = initial_sender_balance - expected_fee;

        let outcome = adapter
            .execute_transaction_with_observed_metadata_v1(&tx, &mut runtime_state, Some(&artifact))
            .expect("execute colliding CREATE2-address artifact");

        assert!(!outcome.artifact.status_ok);
        assert_eq!(outcome.artifact.contract_address, None);
        assert_eq!(
            runtime_state.get_account(&tx.from).map(|acc| acc.balance),
            Some(expected_sender_balance)
        );
        let post_contract = runtime_state
            .get_account(&create2_contract)
            .expect("existing CREATE2 target must remain");
        assert_eq!(post_contract.balance, existing_contract_balance);
        assert_eq!(post_contract.nonce, 1);
        assert_eq!(post_contract.code_hash, Some(existing_code_hash.clone()));
        assert_eq!(
            runtime_state.get_storage(&create2_contract, b"deploy:runtime_code_hash"),
            Some(&existing_code_hash)
        );
        assert_eq!(
            runtime_state.get_storage(&create2_contract, b"deploy:runtime_code"),
            Some(&existing_runtime_code)
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"deploy:last_failed_contract_address"),
            Some(&create2_contract)
        );
        assert_eq!(
            runtime_state.get_storage(&tx.from, b"deploy:last_reject_reason"),
            Some(&b"create_collision_existing_account".to_vec())
        );

        let block_access_list = outcome
            .observed_block_access_list
            .block_access_list
            .expect("CREATE2 collision observed BAL");
        let sender_entry = block_access_list
            .0
            .iter()
            .find(|entry| entry.address.as_slice() == tx.from.as_slice())
            .expect("sender BAL entry");
        assert_eq!(
            sender_entry.balance_changes[0].post_balance,
            NovoVmAdapter::u128_to_be32_v1(expected_sender_balance)
        );
        assert!(
            block_access_list
                .0
                .iter()
                .all(|entry| entry.address.as_slice() != create2_contract.as_slice()),
            "colliding CREATE2 target must not emit a contract BAL account entry"
        );
    }

    #[test]
    fn execute_transaction_with_successful_deploy_artifact_allows_eip7610_listed_address_on_non_mainnet(
    ) {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 11155111,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let mut tx = sample_tx(TxType::ContractDeploy);
        tx.chain_id = 11155111;
        tx = resign_tx(tx);
        assert!(adapter.verify_transaction(&tx).expect("verify deploy"));

        let mut runtime_state = StateIR::new();
        let listed_on_mainnet = sample_eip7610_rejected_mainnet_address_v1();
        let artifact = sample_aoem_artifact(&tx, true, [0x78; 32], Some(listed_on_mainnet.clone()));
        let resolved = adapter
            .execute_transaction_with_artifact(&tx, &mut runtime_state, Some(&artifact))
            .expect("execute deploy on non-mainnet");

        assert!(resolved.status_ok);
        assert_eq!(resolved.contract_address, Some(listed_on_mainnet.clone()));
        assert_eq!(
            runtime_state
                .get_account(&listed_on_mainnet)
                .and_then(|it| it.code_hash.clone()),
            Some(vec![0x77; 32])
        );
    }

    #[test]
    fn failed_revert_artifact_with_zero_gas_derives_host_fallback_gas_v1() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let mut tx = sample_tx(TxType::ContractCall);
        tx.gas_limit = 120_000;
        tx = resign_tx(tx);
        let mut runtime_state = StateIR::new();
        let mut artifact = sample_aoem_artifact(&tx, false, [0x55; 32], None);
        artifact.gas_used = 0;
        artifact.cumulative_gas_used = 0;
        artifact.revert_data = Some(vec![0xde, 0xad]);
        if let Some(anchor) = artifact.anchor.as_mut() {
            anchor.return_code = 3;
            anchor.return_code_name = "revert".to_string();
        }

        let resolved = adapter
            .execute_transaction_with_artifact(&tx, &mut runtime_state, Some(&artifact))
            .expect("execute failed call with derived gas");
        assert_eq!(resolved.gas_used, 35_032);
        assert_eq!(resolved.cumulative_gas_used, 35_032);
    }

    #[test]
    fn execute_transaction_rebuilds_logs_from_runtime_code_when_artifact_logs_absent() {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init");

        let mut runtime_state = StateIR::new();
        let deploy_tx = sample_tx(TxType::ContractDeploy);
        let contract_address = vec![0x44; 20];
        let topic0 = [0x99; 32];
        let data_word = [0x42; 32];
        let mut deploy_artifact =
            sample_aoem_artifact(&deploy_tx, true, [0x33; 32], Some(contract_address.clone()));
        deploy_artifact.event_logs.clear();
        deploy_artifact.log_bloom = vec![0u8; AOEM_LOG_BLOOM_BYTES_V1];
        deploy_artifact.runtime_code = Some(runtime_code_emit_single_log(topic0, data_word));
        deploy_artifact.runtime_code_hash = Some(vec![0x88; 32]);

        let deployed = adapter
            .execute_transaction_with_artifact(
                &deploy_tx,
                &mut runtime_state,
                Some(&deploy_artifact),
            )
            .expect("execute deploy with runtime code");
        assert_eq!(deployed.runtime_code_hash, Some(vec![0x88; 32]));

        let mut call_tx = sample_tx(TxType::ContractCall);
        call_tx.to = Some(contract_address.clone());
        call_tx.nonce = 1;
        call_tx.data = vec![0xaa, 0xbb, 0xcc];
        let call_tx = resign_tx(call_tx);
        let rebuilt = adapter
            .execute_transaction_with_artifact(&call_tx, &mut runtime_state, None)
            .expect("execute contract call with runtime log rebuild");

        assert_eq!(rebuilt.event_logs.len(), 1);
        assert_eq!(rebuilt.event_logs[0].emitter, contract_address);
        assert_eq!(rebuilt.event_logs[0].topics, vec![topic0]);
        assert_eq!(rebuilt.event_logs[0].data, data_word.to_vec());
        assert!(rebuilt.log_bloom.iter().any(|byte| *byte != 0));
    }

    #[test]
    fn evm_execution_spec_fork_rule_smoke_matrix_v1() {
        let profile = resolve_evm_profile(ChainType::EVM, 1).expect("eth profile");
        let precompiles = active_precompile_set_m0(&profile);
        for required in ["ecrecover", "sha256", "identity", "blake2f"] {
            assert!(
                precompiles.contains(&required),
                "ethereum precompile set must include {required}"
            );
        }

        let transfer_tx = sample_tx(TxType::Transfer);
        assert_eq!(estimate_intrinsic_gas_m0(&transfer_tx), 21_000);
        validate_tx_semantics_m0(&profile, &transfer_tx).expect("transfer gas gate");

        let mut deploy_tx = sample_tx(TxType::ContractDeploy);
        deploy_tx.gas_limit = 100_000;
        deploy_tx = resign_tx(deploy_tx);
        assert_eq!(estimate_intrinsic_gas_m0(&deploy_tx), 53_042);
        validate_tx_semantics_m0(&profile, &deploy_tx).expect("deploy gas gate");

        let mut gas_call_tx = sample_tx(TxType::ContractCall);
        gas_call_tx.data = vec![0x00, 0x11, 0x22];
        gas_call_tx.gas_limit = 30_000;
        gas_call_tx = resign_tx(gas_call_tx);
        assert_eq!(estimate_intrinsic_gas_m0(&gas_call_tx), 21_036);
        assert_eq!(
            estimate_intrinsic_gas_with_access_list_rules_m0(&gas_call_tx, 1, 2, false),
            21_036 + 2_400 + 2 * 1_900
        );
        assert!(
            estimate_intrinsic_gas_with_access_list_rules_m0(&gas_call_tx, 1, 2, true)
                > estimate_intrinsic_gas_with_access_list_rules_m0(&gas_call_tx, 1, 2, false)
        );
        assert!(
            estimate_calldata_floor_gas_m0(&gas_call_tx, 1, 2, true)
                > estimate_calldata_floor_gas_m0(&gas_call_tx, 1, 2, false)
        );
        validate_tx_semantics_m0(&profile, &gas_call_tx).expect("call gas gate");

        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id: 1,
            name: "execution-spec-smoke".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("init adapter");
        let mut runtime_state = StateIR::new();
        let contract_address = vec![0x44; 20];
        let topic0 = [0x99; 32];
        let data_word = [0x42; 32];
        let mut deploy_artifact =
            sample_aoem_artifact(&deploy_tx, true, [0x33; 32], Some(contract_address.clone()));
        deploy_artifact.event_logs.clear();
        deploy_artifact.log_bloom = vec![0u8; AOEM_LOG_BLOOM_BYTES_V1];
        deploy_artifact.runtime_code = Some(runtime_code_emit_single_log(topic0, data_word));
        deploy_artifact.runtime_code_hash = Some(vec![0x88; 32]);

        let deployed = adapter
            .execute_transaction_with_artifact(
                &deploy_tx,
                &mut runtime_state,
                Some(&deploy_artifact),
            )
            .expect("execute create sample");
        assert!(deployed.status_ok);
        assert_eq!(deployed.contract_address, Some(contract_address.clone()));
        assert_eq!(
            runtime_state
                .get_account(&contract_address)
                .and_then(|account| account.code_hash.clone()),
            Some(vec![0x88; 32])
        );
        assert_eq!(
            runtime_state.get_storage(&contract_address, b"deploy:runtime_code"),
            Some(&runtime_code_emit_single_log(topic0, data_word))
        );

        let mut call_tx = sample_tx(TxType::ContractCall);
        call_tx.to = Some(contract_address.clone());
        call_tx.nonce = 1;
        call_tx.gas_limit = 80_000;
        call_tx.data = vec![0xaa, 0xbb, 0xcc];
        let call_tx = resign_tx(call_tx);
        let call = adapter
            .execute_transaction_with_artifact(&call_tx, &mut runtime_state, None)
            .expect("execute call sample");
        assert!(call.status_ok);
        assert_eq!(call.event_logs.len(), 1);
        assert_eq!(call.event_logs[0].emitter, contract_address);
        assert_eq!(call.event_logs[0].topics, vec![topic0]);
        assert_eq!(call.event_logs[0].data, data_word.to_vec());
        assert!(call.log_bloom.iter().any(|byte| *byte != 0));
        assert_eq!(
            runtime_state.get_storage(&contract_address, &call_tx.nonce.to_le_bytes()),
            Some(&NovoVmAdapter::tx_hash_or_compute(&call_tx))
        );

        let mut revert_tx = sample_tx(TxType::ContractCall);
        revert_tx.to = Some(contract_address);
        revert_tx.nonce = 2;
        revert_tx.gas_limit = 120_000;
        revert_tx.data = vec![0xde, 0xad];
        let revert_tx = resign_tx(revert_tx);
        let revert_artifact = sample_aoem_artifact(&revert_tx, false, [0x55; 32], None);
        let reverted = adapter
            .execute_transaction_with_artifact(
                &revert_tx,
                &mut runtime_state,
                Some(&revert_artifact),
            )
            .expect("execute revert sample");
        assert!(!reverted.status_ok);
        assert_eq!(reverted.revert_data, Some(vec![0xde, 0xad]));
        assert!(reverted.event_logs.is_empty());
        assert!(reverted.log_bloom.iter().all(|byte| *byte == 0));
        assert_eq!(
            runtime_state.get_storage(&revert_tx.from, b"aoem:last_failure_class"),
            Some(&b"revert".to_vec())
        );
    }

    #[test]
    fn evm_equivalence_baseline_matrix_receipt_revert_gas_v1() {
        evm_execution_spec_create_address_derivation_matches_geth_v1();
        evm_execution_spec_create_call_failure_state_invariants_v1();
        evm_execution_spec_create2_artifact_collision_invariants_v1();
        evm_execution_spec_account_balance_value_fee_invariants_v1();
        evm_execution_spec_effective_gas_price_fee_debit_v1();
        evm_execution_spec_sstore_refund_cap_fee_debit_v1();
        evm_execution_spec_sload_warm_cold_fee_debit_v1();

        let type3_env_key = "NOVOVM_EVM_ENABLE_TYPE3_WRITE_CHAIN_1";
        let type3_env_captured = std::env::var(type3_env_key).ok();
        std::env::set_var(type3_env_key, "1");

        let mut call_tx = sample_tx(TxType::ContractCall);
        call_tx.gas_limit = 120_000;
        call_tx = resign_tx(call_tx);

        let deploy_tx = sample_tx(TxType::ContractDeploy);
        let deploy_artifact =
            sample_aoem_artifact(&deploy_tx, true, [0x33; 32], Some(vec![0x44; 20]));

        let mut reverting_tx = sample_tx(TxType::ContractCall);
        reverting_tx.gas_limit = 120_000;
        reverting_tx = resign_tx(reverting_tx);
        let reverting_artifact = sample_aoem_artifact(&reverting_tx, false, [0x55; 32], None);
        let out_of_gas_tx = sample_tx(TxType::ContractCall);
        let mut out_of_gas_artifact = sample_aoem_artifact(&out_of_gas_tx, false, [0x66; 32], None);
        out_of_gas_artifact.gas_used = out_of_gas_tx.gas_limit;
        out_of_gas_artifact.cumulative_gas_used = out_of_gas_tx.gas_limit;
        out_of_gas_artifact.revert_data = None;
        if let Some(anchor) = out_of_gas_artifact.anchor.as_mut() {
            anchor.return_code = 13;
            anchor.return_code_name = "out_of_gas".to_string();
        }
        let mut out_of_gas_zero_gas_tx = sample_tx(TxType::ContractCall);
        out_of_gas_zero_gas_tx.gas_limit = 120_000;
        out_of_gas_zero_gas_tx = resign_tx(out_of_gas_zero_gas_tx);
        let mut out_of_gas_zero_gas_artifact =
            sample_aoem_artifact(&out_of_gas_zero_gas_tx, false, [0x6a; 32], None);
        out_of_gas_zero_gas_artifact.gas_used = 0;
        out_of_gas_zero_gas_artifact.cumulative_gas_used = 0;
        out_of_gas_zero_gas_artifact.revert_data = None;
        if let Some(anchor) = out_of_gas_zero_gas_artifact.anchor.as_mut() {
            anchor.return_code = 13;
            anchor.return_code_name = "out_of_gas".to_string();
        }
        let invalid_opcode_tx = sample_tx(TxType::ContractCall);
        let mut invalid_opcode_artifact =
            sample_aoem_artifact(&invalid_opcode_tx, false, [0x67; 32], None);
        invalid_opcode_artifact.gas_used = 25_000;
        invalid_opcode_artifact.cumulative_gas_used = 25_000;
        invalid_opcode_artifact.revert_data = None;
        if let Some(anchor) = invalid_opcode_artifact.anchor.as_mut() {
            anchor.return_code = 14;
            anchor.return_code_name = "invalid_opcode".to_string();
        }
        let mut invalid_opcode_dirty_revert_artifact = invalid_opcode_artifact.clone();
        invalid_opcode_dirty_revert_artifact.revert_data = Some(vec![0xfa, 0x11, 0x00]);
        let mut invalid_opcode_zero_gas_tx = sample_tx(TxType::ContractCall);
        invalid_opcode_zero_gas_tx.gas_limit = 120_000;
        invalid_opcode_zero_gas_tx = resign_tx(invalid_opcode_zero_gas_tx);
        let mut invalid_opcode_zero_gas_artifact =
            sample_aoem_artifact(&invalid_opcode_zero_gas_tx, false, [0x6c; 32], None);
        invalid_opcode_zero_gas_artifact.gas_used = 0;
        invalid_opcode_zero_gas_artifact.cumulative_gas_used = 0;
        invalid_opcode_zero_gas_artifact.revert_data = None;
        if let Some(anchor) = invalid_opcode_zero_gas_artifact.anchor.as_mut() {
            anchor.return_code = 14;
            anchor.return_code_name = "invalid_opcode".to_string();
        }
        let mut execution_failed_tx = sample_tx(TxType::ContractCall);
        execution_failed_tx.gas_limit = 120_000;
        execution_failed_tx = resign_tx(execution_failed_tx);
        let mut execution_failed_artifact =
            sample_aoem_artifact(&execution_failed_tx, false, [0x68; 32], None);
        execution_failed_artifact.gas_used = 0;
        execution_failed_artifact.cumulative_gas_used = 0;
        execution_failed_artifact.revert_data = None;
        if let Some(anchor) = execution_failed_artifact.anchor.as_mut() {
            anchor.return_code = 2001;
            anchor.return_code_name = "engine_exec_failed".to_string();
        }
        let mut execution_failed_dirty_revert_artifact = execution_failed_artifact.clone();
        execution_failed_dirty_revert_artifact.revert_data = Some(vec![0xee, 0x77]);
        let mut deploy_failed_tx = sample_tx(TxType::ContractDeploy);
        deploy_failed_tx.gas_limit = 120_000;
        deploy_failed_tx = resign_tx(deploy_failed_tx);
        let mut deploy_failed_artifact =
            sample_aoem_artifact(&deploy_failed_tx, false, [0x69; 32], Some(vec![0x45; 20]));
        deploy_failed_artifact.gas_used = 39_000;
        deploy_failed_artifact.cumulative_gas_used = 39_000;
        deploy_failed_artifact.revert_data = None;
        if let Some(anchor) = deploy_failed_artifact.anchor.as_mut() {
            anchor.return_code = 13;
            anchor.return_code_name = "out_of_gas".to_string();
        }
        let mut deploy_out_of_gas_zero_gas_tx = sample_tx(TxType::ContractDeploy);
        deploy_out_of_gas_zero_gas_tx.gas_limit = 120_000;
        deploy_out_of_gas_zero_gas_tx = resign_tx(deploy_out_of_gas_zero_gas_tx);
        let mut deploy_out_of_gas_zero_gas_artifact = sample_aoem_artifact(
            &deploy_out_of_gas_zero_gas_tx,
            false,
            [0x6b; 32],
            Some(vec![0x46; 20]),
        );
        deploy_out_of_gas_zero_gas_artifact.gas_used = 0;
        deploy_out_of_gas_zero_gas_artifact.cumulative_gas_used = 0;
        deploy_out_of_gas_zero_gas_artifact.revert_data = None;
        if let Some(anchor) = deploy_out_of_gas_zero_gas_artifact.anchor.as_mut() {
            anchor.return_code = 13;
            anchor.return_code_name = "out_of_gas".to_string();
        }
        let mut deploy_execution_failed_zero_gas_tx = sample_tx(TxType::ContractDeploy);
        deploy_execution_failed_zero_gas_tx.gas_limit = 120_000;
        deploy_execution_failed_zero_gas_tx = resign_tx(deploy_execution_failed_zero_gas_tx);
        let mut deploy_execution_failed_zero_gas_artifact = sample_aoem_artifact(
            &deploy_execution_failed_zero_gas_tx,
            false,
            [0x6d; 32],
            Some(vec![0x47; 20]),
        );
        deploy_execution_failed_zero_gas_artifact.gas_used = 0;
        deploy_execution_failed_zero_gas_artifact.cumulative_gas_used = 0;
        deploy_execution_failed_zero_gas_artifact.revert_data = None;
        if let Some(anchor) = deploy_execution_failed_zero_gas_artifact.anchor.as_mut() {
            anchor.return_code = 2001;
            anchor.return_code_name = "engine_exec_failed".to_string();
        }
        let mut deploy_invalid_zero_gas_tx = sample_tx(TxType::ContractDeploy);
        deploy_invalid_zero_gas_tx.gas_limit = 120_000;
        deploy_invalid_zero_gas_tx = resign_tx(deploy_invalid_zero_gas_tx);
        let mut deploy_invalid_zero_gas_artifact = sample_aoem_artifact(
            &deploy_invalid_zero_gas_tx,
            false,
            [0x6e; 32],
            Some(vec![0x48; 20]),
        );
        deploy_invalid_zero_gas_artifact.gas_used = 0;
        deploy_invalid_zero_gas_artifact.cumulative_gas_used = 0;
        deploy_invalid_zero_gas_artifact.revert_data = None;
        if let Some(anchor) = deploy_invalid_zero_gas_artifact.anchor.as_mut() {
            anchor.return_code = 14;
            anchor.return_code_name = "invalid_opcode".to_string();
        }
        let typed_to = vec![0x4eu8; 20];
        let type2_payload = test_rlp_encode_list(&[
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(0),
            test_rlp_encode_u64(2),
            test_rlp_encode_u64(30),
            test_rlp_encode_u64(120_000),
            test_rlp_encode_bytes(&typed_to),
            test_rlp_encode_u128(0),
            test_rlp_encode_bytes(&[0xaa, 0xbb]),
            test_rlp_encode_list(&[]),
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(1),
        ]);
        let mut type2_raw = vec![0x02u8];
        type2_raw.extend_from_slice(&type2_payload);
        let type2_fields = translate_raw_evm_tx_fields_m0(&type2_raw).expect("decode type2 raw tx");
        let typed_type2_tx = tx_ir_from_raw_fields_m0(&type2_fields, &type2_raw, vec![0x91; 20], 1);
        let typed_type2_intrinsic =
            estimate_intrinsic_gas_with_envelope_extras_m0(&typed_type2_tx, 0, 0, 0);
        let typed_type2_invalid_target = typed_type2_intrinsic
            .saturating_add(4_000)
            .min(typed_type2_tx.gas_limit.max(typed_type2_intrinsic));
        let mut typed_type2_out_of_gas_artifact =
            sample_aoem_artifact(&typed_type2_tx, false, [0x7a; 32], None);
        typed_type2_out_of_gas_artifact.gas_used = 0;
        typed_type2_out_of_gas_artifact.cumulative_gas_used = 0;
        typed_type2_out_of_gas_artifact.revert_data = None;
        if let Some(anchor) = typed_type2_out_of_gas_artifact.anchor.as_mut() {
            anchor.return_code = 13;
            anchor.return_code_name = "out_of_gas".to_string();
        }
        let mut typed_type2_invalid_artifact =
            sample_aoem_artifact(&typed_type2_tx, false, [0x7b; 32], None);
        typed_type2_invalid_artifact.gas_used = 0;
        typed_type2_invalid_artifact.cumulative_gas_used = 0;
        typed_type2_invalid_artifact.revert_data = None;
        if let Some(anchor) = typed_type2_invalid_artifact.anchor.as_mut() {
            anchor.return_code = 14;
            anchor.return_code_name = "invalid_opcode".to_string();
        }

        let type3_payload = test_rlp_encode_list(&[
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(0),
            test_rlp_encode_u64(3),
            test_rlp_encode_u64(40),
            test_rlp_encode_u64(400_000),
            test_rlp_encode_bytes(&typed_to),
            test_rlp_encode_u128(0),
            test_rlp_encode_bytes(&[0xcc]),
            test_rlp_encode_list(&[]),
            test_rlp_encode_u64(7),
            test_rlp_encode_list(&[
                test_rlp_encode_bytes(&[0x11; 32]),
                test_rlp_encode_bytes(&[0x22; 32]),
            ]),
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(1),
            test_rlp_encode_u64(1),
        ]);
        let mut type3_raw = vec![0x03u8];
        type3_raw.extend_from_slice(&type3_payload);
        let typed_type3_case = match translate_raw_evm_tx_fields_m0(&type3_raw) {
            Ok(type3_fields) => {
                let typed_type3_tx =
                    tx_ir_from_raw_fields_m0(&type3_fields, &type3_raw, vec![0x92; 20], 1);
                let typed_type3_intrinsic =
                    estimate_intrinsic_gas_with_envelope_extras_m0(&typed_type3_tx, 0, 0, 2);
                let typed_type3_exec_failed_target = typed_type3_intrinsic
                    .saturating_add(14_000)
                    .min(typed_type3_tx.gas_limit.max(typed_type3_intrinsic));
                let mut typed_type3_execution_failed_artifact =
                    sample_aoem_artifact(&typed_type3_tx, false, [0x7c; 32], None);
                typed_type3_execution_failed_artifact.gas_used = 0;
                typed_type3_execution_failed_artifact.cumulative_gas_used = 0;
                typed_type3_execution_failed_artifact.revert_data = None;
                if let Some(anchor) = typed_type3_execution_failed_artifact.anchor.as_mut() {
                    anchor.return_code = 2001;
                    anchor.return_code_name = "engine_exec_failed".to_string();
                }
                Some(EquivalenceCaseV1 {
                    name: "typed_type3_execution_failed_zero_gas_artifact",
                    tx: typed_type3_tx,
                    artifact: Some(typed_type3_execution_failed_artifact),
                    expected_execution_ok: true,
                    expected_status_ok: Some(false),
                    expected_revert_data: Some(false),
                    expected_contract_address: Some(false),
                    expected_logs_present: Some(false),
                    expected_failure_class: Some("execution_failed"),
                    expected_failure_class_source: Some("anchor_return_code"),
                    expected_failure_recoverability: Some("recoverable"),
                    expected_failure_contract: Some(AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1),
                    target_gas_used: Some(typed_type3_exec_failed_target),
                    expected_cumulative_gas_used: Some(typed_type3_exec_failed_target),
                    expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
                })
            }
            Err(error) => {
                if error
                    .to_string()
                    .contains("blob (type 3) write path disabled in M0")
                {
                    None
                } else {
                    panic!("decode type3 raw tx: {error}");
                }
            }
        };

        let mut invalid_envelope_tx = sample_tx(TxType::Transfer);
        invalid_envelope_tx.signature = vec![0x01, 0x02, 0x03];

        let invalid_semantic_tx = sample_tx(TxType::CrossShard);

        let mut cases = vec![
            EquivalenceCaseV1 {
                name: "transfer_success_baseline",
                tx: sample_tx(TxType::Transfer),
                artifact: None,
                expected_execution_ok: true,
                expected_status_ok: Some(true),
                expected_revert_data: Some(false),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: None,
                expected_failure_class_source: None,
                expected_failure_recoverability: None,
                expected_failure_contract: None,
                target_gas_used: Some(21_000),
                expected_cumulative_gas_used: Some(21_000),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "contract_call_host_derived_gas_gap",
                tx: call_tx,
                artifact: None,
                expected_execution_ok: true,
                expected_status_ok: Some(true),
                expected_revert_data: Some(false),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: None,
                expected_failure_class_source: None,
                expected_failure_recoverability: None,
                expected_failure_contract: None,
                target_gas_used: Some(53_032),
                expected_cumulative_gas_used: Some(53_032),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "contract_deploy_artifact_receipt",
                tx: deploy_tx,
                artifact: Some(deploy_artifact),
                expected_execution_ok: true,
                expected_status_ok: Some(true),
                expected_revert_data: Some(false),
                expected_contract_address: Some(true),
                expected_logs_present: Some(true),
                expected_failure_class: None,
                expected_failure_class_source: None,
                expected_failure_recoverability: None,
                expected_failure_contract: None,
                target_gas_used: Some(21_000),
                expected_cumulative_gas_used: Some(21_000),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "contract_call_revert_artifact",
                tx: reverting_tx,
                artifact: Some(reverting_artifact),
                expected_execution_ok: true,
                expected_status_ok: Some(false),
                expected_revert_data: Some(true),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: Some("revert"),
                expected_failure_class_source: Some("heuristic_revert_data"),
                expected_failure_recoverability: Some("recoverable"),
                expected_failure_contract: Some(AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1),
                target_gas_used: Some(35_032),
                expected_cumulative_gas_used: Some(35_032),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "contract_call_out_of_gas_artifact",
                tx: out_of_gas_tx,
                artifact: Some(out_of_gas_artifact),
                expected_execution_ok: true,
                expected_status_ok: Some(false),
                expected_revert_data: Some(false),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: Some("out_of_gas"),
                expected_failure_class_source: Some("anchor_return_code"),
                expected_failure_recoverability: Some("recoverable"),
                expected_failure_contract: Some(AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1),
                target_gas_used: Some(21_000),
                expected_cumulative_gas_used: Some(21_000),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "contract_call_out_of_gas_zero_gas_artifact",
                tx: out_of_gas_zero_gas_tx,
                artifact: Some(out_of_gas_zero_gas_artifact),
                expected_execution_ok: true,
                expected_status_ok: Some(false),
                expected_revert_data: Some(false),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: Some("out_of_gas"),
                expected_failure_class_source: Some("anchor_return_code"),
                expected_failure_recoverability: Some("recoverable"),
                expected_failure_contract: Some(AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1),
                target_gas_used: Some(120_000),
                expected_cumulative_gas_used: Some(120_000),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "contract_call_invalid_opcode_artifact",
                tx: invalid_opcode_tx.clone(),
                artifact: Some(invalid_opcode_artifact),
                expected_execution_ok: true,
                expected_status_ok: Some(false),
                expected_revert_data: Some(false),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: Some("invalid"),
                expected_failure_class_source: Some("anchor_return_code"),
                expected_failure_recoverability: Some("non_recoverable"),
                expected_failure_contract: Some(AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1),
                target_gas_used: Some(25_000),
                expected_cumulative_gas_used: Some(25_000),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "contract_call_invalid_opcode_dirty_revert_artifact",
                tx: invalid_opcode_tx,
                artifact: Some(invalid_opcode_dirty_revert_artifact),
                expected_execution_ok: true,
                expected_status_ok: Some(false),
                expected_revert_data: Some(false),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: Some("invalid"),
                expected_failure_class_source: Some("anchor_return_code"),
                expected_failure_recoverability: Some("non_recoverable"),
                expected_failure_contract: Some(AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1),
                target_gas_used: Some(25_000),
                expected_cumulative_gas_used: Some(25_000),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "contract_call_invalid_opcode_zero_gas_artifact",
                tx: invalid_opcode_zero_gas_tx,
                artifact: Some(invalid_opcode_zero_gas_artifact),
                expected_execution_ok: true,
                expected_status_ok: Some(false),
                expected_revert_data: Some(false),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: Some("invalid"),
                expected_failure_class_source: Some("anchor_return_code"),
                expected_failure_recoverability: Some("non_recoverable"),
                expected_failure_contract: Some(AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1),
                target_gas_used: Some(25_032),
                expected_cumulative_gas_used: Some(25_032),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "contract_call_execution_failed_artifact",
                tx: execution_failed_tx.clone(),
                artifact: Some(execution_failed_artifact),
                expected_execution_ok: true,
                expected_status_ok: Some(false),
                expected_revert_data: Some(false),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: Some("execution_failed"),
                expected_failure_class_source: Some("anchor_return_code"),
                expected_failure_recoverability: Some("recoverable"),
                expected_failure_contract: Some(AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1),
                target_gas_used: Some(35_032),
                expected_cumulative_gas_used: Some(35_032),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "contract_call_execution_failed_dirty_revert_artifact",
                tx: execution_failed_tx,
                artifact: Some(execution_failed_dirty_revert_artifact),
                expected_execution_ok: true,
                expected_status_ok: Some(false),
                expected_revert_data: Some(false),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: Some("execution_failed"),
                expected_failure_class_source: Some("anchor_return_code"),
                expected_failure_recoverability: Some("recoverable"),
                expected_failure_contract: Some(AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1),
                target_gas_used: Some(35_032),
                expected_cumulative_gas_used: Some(35_032),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "contract_deploy_out_of_gas_artifact",
                tx: deploy_failed_tx,
                artifact: Some(deploy_failed_artifact),
                expected_execution_ok: true,
                expected_status_ok: Some(false),
                expected_revert_data: Some(false),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: Some("out_of_gas"),
                expected_failure_class_source: Some("anchor_return_code"),
                expected_failure_recoverability: Some("recoverable"),
                expected_failure_contract: Some(AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1),
                target_gas_used: Some(39_000),
                expected_cumulative_gas_used: Some(39_000),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "contract_deploy_out_of_gas_zero_gas_artifact",
                tx: deploy_out_of_gas_zero_gas_tx,
                artifact: Some(deploy_out_of_gas_zero_gas_artifact),
                expected_execution_ok: true,
                expected_status_ok: Some(false),
                expected_revert_data: Some(false),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: Some("out_of_gas"),
                expected_failure_class_source: Some("anchor_return_code"),
                expected_failure_recoverability: Some("recoverable"),
                expected_failure_contract: Some(AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1),
                target_gas_used: Some(120_000),
                expected_cumulative_gas_used: Some(120_000),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "contract_deploy_execution_failed_zero_gas_artifact",
                tx: deploy_execution_failed_zero_gas_tx,
                artifact: Some(deploy_execution_failed_zero_gas_artifact),
                expected_execution_ok: true,
                expected_status_ok: Some(false),
                expected_revert_data: Some(false),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: Some("execution_failed"),
                expected_failure_class_source: Some("anchor_return_code"),
                expected_failure_recoverability: Some("recoverable"),
                expected_failure_contract: Some(AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1),
                target_gas_used: Some(41_056),
                expected_cumulative_gas_used: Some(41_056),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "contract_deploy_invalid_zero_gas_artifact",
                tx: deploy_invalid_zero_gas_tx,
                artifact: Some(deploy_invalid_zero_gas_artifact),
                expected_execution_ok: true,
                expected_status_ok: Some(false),
                expected_revert_data: Some(false),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: Some("invalid"),
                expected_failure_class_source: Some("anchor_return_code"),
                expected_failure_recoverability: Some("non_recoverable"),
                expected_failure_contract: Some(AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1),
                target_gas_used: Some(29_048),
                expected_cumulative_gas_used: Some(29_048),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "typed_type2_out_of_gas_zero_gas_artifact",
                tx: typed_type2_tx.clone(),
                artifact: Some(typed_type2_out_of_gas_artifact),
                expected_execution_ok: true,
                expected_status_ok: Some(false),
                expected_revert_data: Some(false),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: Some("out_of_gas"),
                expected_failure_class_source: Some("anchor_return_code"),
                expected_failure_recoverability: Some("recoverable"),
                expected_failure_contract: Some(AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1),
                target_gas_used: Some(120_000),
                expected_cumulative_gas_used: Some(120_000),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "typed_type2_invalid_zero_gas_artifact",
                tx: typed_type2_tx,
                artifact: Some(typed_type2_invalid_artifact),
                expected_execution_ok: true,
                expected_status_ok: Some(false),
                expected_revert_data: Some(false),
                expected_contract_address: Some(false),
                expected_logs_present: Some(false),
                expected_failure_class: Some("invalid"),
                expected_failure_class_source: Some("anchor_return_code"),
                expected_failure_recoverability: Some("non_recoverable"),
                expected_failure_contract: Some(AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1),
                target_gas_used: Some(typed_type2_invalid_target),
                expected_cumulative_gas_used: Some(typed_type2_invalid_target),
                expected_gas_diff_class: Some(EquivalenceGasDiffClassV1::Exact),
            },
            EquivalenceCaseV1 {
                name: "invalid_envelope_signature",
                tx: invalid_envelope_tx,
                artifact: None,
                expected_execution_ok: false,
                expected_status_ok: None,
                expected_revert_data: None,
                expected_contract_address: None,
                expected_logs_present: None,
                expected_failure_class: None,
                expected_failure_class_source: None,
                expected_failure_recoverability: None,
                expected_failure_contract: None,
                target_gas_used: None,
                expected_cumulative_gas_used: None,
                expected_gas_diff_class: None,
            },
            EquivalenceCaseV1 {
                name: "invalid_semantic_tx_type",
                tx: invalid_semantic_tx,
                artifact: None,
                expected_execution_ok: false,
                expected_status_ok: None,
                expected_revert_data: None,
                expected_contract_address: None,
                expected_logs_present: None,
                expected_failure_class: None,
                expected_failure_class_source: None,
                expected_failure_recoverability: None,
                expected_failure_contract: None,
                target_gas_used: None,
                expected_cumulative_gas_used: None,
                expected_gas_diff_class: None,
            },
        ];
        if let Some(typed_type3_case) = typed_type3_case {
            cases.push(typed_type3_case);
        }

        for case in cases {
            let mut adapter = NovoVmAdapter::new(ChainConfig {
                chain_type: ChainType::EVM,
                chain_id: 1,
                name: "equivalence-matrix".to_string(),
                enabled: true,
                custom_config: None,
            });
            adapter.initialize().expect("init adapter");
            let mut runtime_state = StateIR::new();

            let verify_ok = adapter
                .verify_transaction(&case.tx)
                .expect("verify transaction");
            let can_force_artifact_execute = !verify_ok
                && case.expected_execution_ok
                && case.artifact.is_some()
                && case.tx.signature.len() != 96;
            if can_force_artifact_execute {
                // Typed raw fixtures in equivalence matrix focus semantic/gas alignment.
                // Their signatures are not always cryptographically valid test vectors.
                adapter.verified_tx_cache.insert(case.tx.hash.clone());
            }
            if !case.expected_execution_ok {
                assert!(
                    !verify_ok,
                    "{}: invalid case should fail verification",
                    case.name
                );
                let err = adapter
                    .execute_transaction_with_artifact(
                        &case.tx,
                        &mut runtime_state,
                        case.artifact.as_ref(),
                    )
                    .expect_err("invalid case must not execute");
                assert!(
                    err.to_string().contains("transaction verification failed"),
                    "{}: expected verification failure, got {err}",
                    case.name
                );
                continue;
            }

            if !can_force_artifact_execute {
                assert!(verify_ok, "{}: valid case failed verification", case.name);
            }
            runtime_state.accounts.insert(
                case.tx.from.clone(),
                AccountState {
                    balance: u128::MAX / 4,
                    nonce: case.tx.nonce,
                    code_hash: None,
                    storage_root: vec![0u8; 32],
                },
            );
            let resolved = adapter
                .execute_transaction_with_artifact(
                    &case.tx,
                    &mut runtime_state,
                    case.artifact.as_ref(),
                )
                .expect("execute with equivalence case");

            if let Some(expect_status_ok) = case.expected_status_ok {
                assert_eq!(
                    resolved.status_ok, expect_status_ok,
                    "{}: receipt status mismatch",
                    case.name
                );
            }
            let expected_receipt_type = case
                .artifact
                .as_ref()
                .and_then(|artifact| artifact.receipt_type);
            assert_eq!(
                resolved.receipt_type, expected_receipt_type,
                "{}: receipt type must follow artifact contract",
                case.name
            );
            let expected_effective_gas_price = case
                .artifact
                .as_ref()
                .and_then(|artifact| artifact.effective_gas_price)
                .or(Some(case.tx.gas_price));
            assert_eq!(
                resolved.effective_gas_price, expected_effective_gas_price,
                "{}: effective gas price mismatch",
                case.name
            );
            if let Some(expect_revert_data) = case.expected_revert_data {
                assert_eq!(
                    resolved.revert_data.is_some(),
                    expect_revert_data,
                    "{}: revert-data boundary mismatch",
                    case.name
                );
            }
            if let Some(expect_contract_address) = case.expected_contract_address {
                assert_eq!(
                    resolved.contract_address.is_some(),
                    expect_contract_address,
                    "{}: contract creation receipt boundary mismatch",
                    case.name
                );
            }
            if let Some(expect_logs_present) = case.expected_logs_present {
                assert_eq!(
                    !resolved.event_logs.is_empty(),
                    expect_logs_present,
                    "{}: receipt logs boundary mismatch",
                    case.name
                );
            }
            if resolved.status_ok {
                if resolved.event_logs.is_empty() {
                    assert!(
                        resolved.log_bloom.iter().all(|byte| *byte == 0),
                        "{}: successful tx without logs must keep zero bloom",
                        case.name
                    );
                } else {
                    assert!(
                        resolved.log_bloom.iter().any(|byte| *byte != 0),
                        "{}: successful tx with logs must expose non-zero bloom",
                        case.name
                    );
                }
            } else {
                assert!(
                    resolved.event_logs.is_empty(),
                    "{}: failed tx must not expose event logs",
                    case.name
                );
                assert!(
                    resolved.log_bloom.iter().all(|byte| *byte == 0),
                    "{}: failed tx must keep zero bloom",
                    case.name
                );
                assert!(
                    resolved.contract_address.is_none(),
                    "{}: failed tx must not keep contract address",
                    case.name
                );
                if case.expected_revert_data == Some(true) {
                    if let Some(artifact) = case.artifact.as_ref() {
                        assert_eq!(
                            resolved.revert_data, artifact.revert_data,
                            "{}: failed revert tx must preserve artifact revert-data",
                            case.name
                        );
                    }
                } else {
                    assert!(
                        resolved.revert_data.is_none(),
                        "{}: non-revert failed tx must not expose revert-data",
                        case.name
                    );
                }
            }
            if let Some(expect_failure_class) = case.expected_failure_class {
                assert_eq!(
                    runtime_state.get_storage(&case.tx.from, b"aoem:last_failure_class"),
                    Some(&expect_failure_class.as_bytes().to_vec()),
                    "{}: failure semantic class mismatch",
                    case.name
                );
            }
            if let Some(expect_failure_class_source) = case.expected_failure_class_source {
                assert_eq!(
                    runtime_state.get_storage(&case.tx.from, b"aoem:last_failure_class_source"),
                    Some(&expect_failure_class_source.as_bytes().to_vec()),
                    "{}: failure semantic class source mismatch",
                    case.name
                );
            }
            if let Some(expect_failure_recoverability) = case.expected_failure_recoverability {
                assert_eq!(
                    runtime_state.get_storage(&case.tx.from, b"aoem:last_failure_recoverability"),
                    Some(&expect_failure_recoverability.as_bytes().to_vec()),
                    "{}: failure recoverability mismatch",
                    case.name
                );
            }
            if let Some(expect_failure_contract) = case.expected_failure_contract {
                assert_eq!(
                    runtime_state
                        .get_storage(&case.tx.from, b"aoem:last_failure_classification_contract"),
                    Some(&expect_failure_contract.as_bytes().to_vec()),
                    "{}: failure contract source mismatch",
                    case.name
                );
            }

            let gas_diff_class = classify_gas_diff_v1(resolved.gas_used, case.target_gas_used);
            if let Some(expected_gas_diff_class) = case.expected_gas_diff_class {
                assert_eq!(
                    gas_diff_class, expected_gas_diff_class,
                    "{}: gas diff class mismatch (actual={}, target={:?})",
                    case.name, resolved.gas_used, case.target_gas_used
                );
            }
            if let Some(expected_cumulative_gas_used) = case.expected_cumulative_gas_used {
                assert_eq!(
                    resolved.cumulative_gas_used, expected_cumulative_gas_used,
                    "{}: cumulative gas mismatch",
                    case.name
                );
            }
            assert!(
                resolved.cumulative_gas_used >= resolved.gas_used,
                "{}: cumulative gas must be >= gas_used",
                case.name
            );
        }

        if let Some(value) = type3_env_captured {
            std::env::set_var(type3_env_key, value);
        } else {
            std::env::remove_var(type3_env_key);
        }
    }

    #[test]
    fn adapter_subject_normalization_prefers_explicit_account_id_v1() -> Result<()> {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::NovoVM,
            chain_id: 1,
            name: "test".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize()?;

        let mut tx = sample_tx(TxType::Transfer);
        tx.account_id = Some("uca:phase2:explicit".to_string());
        tx.fee_owner_account_id = Some("uca:phase2:fee".to_string());
        tx.nonce_owner_account_id = Some("uca:phase2:explicit".to_string());
        tx = resign_tx(tx);

        let mut runtime_state = StateIR::new();
        adapter.execute_transaction(&tx, &mut runtime_state)?;

        let persona = adapter.adapter_persona(&tx);
        assert_eq!(
            adapter
                .unified_account_router()
                .resolve_binding_owner(&persona),
            Some("uca:phase2:explicit")
        );
        assert!(adapter
            .unified_account_router()
            .get_account("uca:phase2:explicit")
            .is_ok());

        let legacy_fallback = NovoVmAdapter::fallback_adapter_account_id(&tx.from);
        assert!(
            adapter
                .unified_account_router()
                .get_account(&legacy_fallback)
                .is_err(),
            "legacy fallback subject should not be materialized when explicit account_id is supplied"
        );

        let nonce_owner_key = NovoVmAdapter::subject_state_key("uca:phase2:explicit");
        assert_eq!(
            adapter
                .state
                .get_account(&nonce_owner_key)
                .map(|acc| acc.nonce),
            Some(1),
            "subject nonce owner must advance under explicit account ownership"
        );
        assert_eq!(
            adapter
                .state
                .get_account(&tx.from)
                .map(|acc| acc.nonce)
                .unwrap_or(0),
            0,
            "legacy address account must not keep nonce ownership after subject normalization"
        );
        assert_eq!(
            adapter.get_nonce(&tx.from)?,
            1,
            "adapter nonce query should resolve through the bound account subject"
        );

        let mut legacy_nonce_key = tx.from.clone();
        legacy_nonce_key.extend_from_slice(&tx.nonce.to_le_bytes());
        assert!(
            !adapter.kv.contains_key(&legacy_nonce_key),
            "legacy address nonce key must not be written once subject nonce ownership is enabled"
        );
        assert!(
            adapter
                .kv
                .contains_key(&NovoVmAdapter::append_subject_nonce_key(
                    "uca:phase2:explicit",
                    0
                )),
            "subject nonce key must be written for the canonical nonce owner"
        );

        Ok(())
    }
}
