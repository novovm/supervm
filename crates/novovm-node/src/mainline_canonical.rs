#![forbid(unsafe_code)]

use anyhow::{Context, Result};
use novovm_exec::{SupervmEvmExecutionReceiptV1, SupervmEvmStateMirrorUpdateV1};
use novovm_network::{
    derive_eth_fullnode_chain_view_v1, EthFullnodeBlockContextV1, EthFullnodeBlockViewSource,
    EthFullnodeChainViewV1,
};
use novovm_protocol::EvmBlockAccessListV1;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainlineCanonicalBatchRecordV1 {
    pub seq: u64,
    pub source_detail: String,
    pub tx_count: usize,
    pub tap_requested: u64,
    pub tap_accepted: usize,
    pub tap_dropped: usize,
    pub apply_verified: bool,
    pub apply_applied: bool,
    pub apply_state_root: [u8; 32],
    #[serde(default)]
    pub block_access_list: Option<EvmBlockAccessListV1>,
    #[serde(default)]
    pub block_access_list_complete: bool,
    #[serde(default)]
    pub block_access_list_hash: Option<[u8; 32]>,
    pub exported_receipt_count: usize,
    pub mirrored_receipt_count: usize,
    pub state_version: u64,
    pub ingress_bypassed: bool,
    pub atomic_guard_enabled: bool,
    #[serde(default)]
    pub raw_tx_rlps: Vec<Vec<u8>>,
    pub receipts: Vec<SupervmEvmExecutionReceiptV1>,
    pub state_mirror_updates: Vec<SupervmEvmStateMirrorUpdateV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainlineCanonicalStoreV1 {
    pub schema: String,
    pub generated_unix_ms: u64,
    pub chain_type: String,
    pub chain_id: u64,
    pub batches: Vec<MainlineCanonicalBatchRecordV1>,
}

impl Default for MainlineCanonicalStoreV1 {
    fn default() -> Self {
        Self {
            schema: "supervm-mainline-canonical/v1".to_string(),
            generated_unix_ms: 0,
            chain_type: "evm".to_string(),
            chain_id: 0,
            batches: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MainlineEthBlockContextV1 {
    pub chain_id: u64,
    pub block_number: u64,
    pub canonical_batch_seq: u64,
    pub block_hash: [u8; 32],
    pub parent_block_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub block_access_list_hash: Option<[u8; 32]>,
    pub state_version: u64,
    pub tx_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MainlineEthBlockAccessListContextV1 {
    pub chain_id: u64,
    pub block_number: u64,
    pub canonical_batch_seq: u64,
    pub block_hash: [u8; 32],
    pub block_access_list: Option<EvmBlockAccessListV1>,
    pub block_access_list_complete: bool,
    pub block_access_list_hash: Option<[u8; 32]>,
}

pub fn derive_mainline_eth_block_hash_v1(
    chain_id: u64,
    batch: &MainlineCanonicalBatchRecordV1,
    parent_block_hash: [u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"supervm-mainline-eth-block-hash-v1");
    hasher.update(chain_id.to_le_bytes());
    hasher.update(batch.seq.to_le_bytes());
    hasher.update(parent_block_hash);
    hasher.update(batch.state_version.to_le_bytes());
    hasher.update(batch.apply_state_root);
    hasher.update((batch.tx_count as u64).to_le_bytes());
    for receipt in &batch.receipts {
        hasher.update(receipt.tx_hash.as_slice());
        hasher.update(receipt.state_root);
        hasher.update(receipt.tx_index.to_le_bytes());
    }
    hasher.finalize().into()
}

pub fn derive_mainline_eth_block_contexts_v1(
    store: &MainlineCanonicalStoreV1,
) -> Vec<MainlineEthBlockContextV1> {
    let mut out = Vec::with_capacity(store.batches.len());
    let mut parent_block_hash = [0u8; 32];
    for batch in &store.batches {
        let block_hash =
            derive_mainline_eth_block_hash_v1(store.chain_id, batch, parent_block_hash);
        let context = MainlineEthBlockContextV1 {
            chain_id: store.chain_id,
            block_number: batch.seq,
            canonical_batch_seq: batch.seq,
            block_hash,
            parent_block_hash,
            state_root: batch.apply_state_root,
            block_access_list_hash: batch.block_access_list_hash,
            state_version: batch.state_version,
            tx_count: batch.tx_count,
        };
        parent_block_hash = block_hash;
        out.push(context);
    }
    out
}

pub fn derive_mainline_eth_block_context_by_seq_v1(
    store: &MainlineCanonicalStoreV1,
    batch_seq: u64,
) -> Option<MainlineEthBlockContextV1> {
    derive_mainline_eth_block_contexts_v1(store)
        .into_iter()
        .find(|context| context.canonical_batch_seq == batch_seq)
}

pub fn derive_mainline_eth_block_access_list_by_hash_v1(
    store: &MainlineCanonicalStoreV1,
    block_hash: [u8; 32],
) -> Option<MainlineEthBlockAccessListContextV1> {
    let mut parent_block_hash = [0u8; 32];
    for batch in &store.batches {
        let candidate_hash =
            derive_mainline_eth_block_hash_v1(store.chain_id, batch, parent_block_hash);
        if candidate_hash == block_hash {
            return Some(MainlineEthBlockAccessListContextV1 {
                chain_id: store.chain_id,
                block_number: batch.seq,
                canonical_batch_seq: batch.seq,
                block_hash,
                block_access_list: batch.block_access_list.clone(),
                block_access_list_complete: batch.block_access_list_complete,
                block_access_list_hash: batch.block_access_list_hash,
            });
        }
        parent_block_hash = candidate_hash;
    }
    None
}

pub fn project_mainline_eth_block_context_to_fullnode_v1(
    context: &MainlineEthBlockContextV1,
) -> EthFullnodeBlockContextV1 {
    EthFullnodeBlockContextV1 {
        source: EthFullnodeBlockViewSource::CanonicalHostBatch,
        chain_id: context.chain_id,
        block_number: context.block_number,
        canonical_batch_seq: Some(context.canonical_batch_seq),
        block_hash: context.block_hash,
        parent_block_hash: context.parent_block_hash,
        state_root: context.state_root,
        block_access_list_hash: context.block_access_list_hash,
        state_version: context.state_version,
        tx_count: context.tx_count,
    }
}

pub fn derive_mainline_eth_fullnode_block_contexts_v1(
    store: &MainlineCanonicalStoreV1,
) -> Vec<EthFullnodeBlockContextV1> {
    derive_mainline_eth_block_contexts_v1(store)
        .iter()
        .map(project_mainline_eth_block_context_to_fullnode_v1)
        .collect()
}

pub fn derive_mainline_eth_fullnode_chain_view_v1(
    store: &MainlineCanonicalStoreV1,
) -> Option<EthFullnodeChainViewV1> {
    let blocks = derive_mainline_eth_fullnode_block_contexts_v1(store);
    derive_eth_fullnode_chain_view_v1(&blocks)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_millis() as u64)
        .unwrap_or(0)
}

pub fn load_mainline_canonical_store(path: &Path) -> Result<MainlineCanonicalStoreV1> {
    if !path.exists() {
        return Ok(MainlineCanonicalStoreV1::default());
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read canonical artifact store failed: {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(MainlineCanonicalStoreV1::default());
    }
    let parsed = serde_json::from_str::<MainlineCanonicalStoreV1>(&raw).with_context(|| {
        format!(
            "parse canonical artifact store json failed: {}",
            path.display()
        )
    })?;
    Ok(parsed)
}

pub fn save_mainline_canonical_store(path: &Path, store: &MainlineCanonicalStoreV1) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create canonical artifact dir failed: {}", parent.display())
            })?;
        }
    }
    let payload =
        serde_json::to_string_pretty(store).context("serialize canonical artifact store failed")?;
    fs::write(path, payload)
        .with_context(|| format!("write canonical artifact store failed: {}", path.display()))?;
    Ok(())
}

pub fn append_mainline_canonical_batch(
    path: &Path,
    chain_type: &str,
    chain_id: u64,
    batch: MainlineCanonicalBatchRecordV1,
) -> Result<MainlineCanonicalStoreV1> {
    let mut store = load_mainline_canonical_store(path)?;
    store.schema = "supervm-mainline-canonical/v1".to_string();
    store.generated_unix_ms = now_unix_ms();
    store.chain_type = chain_type.to_string();
    store.chain_id = chain_id;
    store.batches.push(batch);
    save_mainline_canonical_store(path, &store)?;
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_adapter_api::{ChainType, TxType};
    use novovm_protocol::{
        EvmBlockAccessAccountV1, EvmBlockAccessCodeChangeV1, EvmBlockAccessListV1,
        EvmBlockAccessSlotChangesV1, EvmBlockAccessStorageWriteV1,
    };
    use std::path::PathBuf;

    fn sample_block_access_list() -> EvmBlockAccessListV1 {
        EvmBlockAccessListV1(vec![EvmBlockAccessAccountV1 {
            address: [0x11; 20],
            storage_changes: vec![EvmBlockAccessSlotChangesV1 {
                slot: [0x22; 32],
                slot_changes: vec![EvmBlockAccessStorageWriteV1 {
                    block_access_index: 1,
                    post_value: [0x33; 32],
                }],
            }],
            storage_reads: Vec::new(),
            balance_changes: Vec::new(),
            nonce_changes: Vec::new(),
            code_changes: vec![EvmBlockAccessCodeChangeV1 {
                block_access_index: 0,
                new_code: vec![0xde, 0xad, 0xbe, 0xef],
            }],
        }])
    }

    #[test]
    fn append_mainline_canonical_batch_persists_store() {
        let temp_root =
            std::env::temp_dir().join(format!("novovm-node-canonical-{}", now_unix_ms()));
        let store_path = temp_root.join("canonical.json");
        let appended = append_mainline_canonical_batch(
            &store_path,
            "evm",
            1,
            MainlineCanonicalBatchRecordV1 {
                seq: 1,
                source_detail: "test".to_string(),
                tx_count: 1,
                tap_requested: 1,
                tap_accepted: 1,
                tap_dropped: 0,
                apply_verified: true,
                apply_applied: true,
                apply_state_root: [7u8; 32],
                block_access_list: Some(sample_block_access_list()),
                block_access_list_complete: false,
                block_access_list_hash: Some([0xabu8; 32]),
                exported_receipt_count: 0,
                mirrored_receipt_count: 0,
                state_version: 9,
                ingress_bypassed: true,
                atomic_guard_enabled: false,
                raw_tx_rlps: Vec::new(),
                receipts: Vec::new(),
                state_mirror_updates: Vec::new(),
            },
        )
        .expect("append canonical batch");
        assert_eq!(appended.chain_type, "evm");
        assert_eq!(appended.chain_id, 1);
        assert_eq!(appended.batches.len(), 1);

        let loaded = load_mainline_canonical_store(&store_path).expect("load canonical store");
        assert_eq!(loaded.batches.len(), 1);
        assert_eq!(
            loaded.batches[0].block_access_list,
            Some(sample_block_access_list())
        );
        assert_eq!(loaded.batches[0].block_access_list_hash, Some([0xabu8; 32]));

        let _ = fs::remove_file(PathBuf::from(&store_path));
        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn load_mainline_canonical_store_defaults_missing_block_access_list_hash_to_none() {
        let temp_root =
            std::env::temp_dir().join(format!("novovm-node-canonical-migrate-{}", now_unix_ms()));
        let store_path = temp_root.join("canonical.json");
        fs::create_dir_all(&temp_root).expect("create temp root");
        fs::write(
            &store_path,
            r#"{
  "schema": "supervm-mainline-canonical/v1",
  "generated_unix_ms": 1,
  "chain_type": "evm",
  "chain_id": 1,
  "batches": [
    {
      "seq": 1,
      "source_detail": "legacy",
      "tx_count": 0,
      "tap_requested": 0,
      "tap_accepted": 0,
      "tap_dropped": 0,
      "apply_verified": true,
      "apply_applied": true,
      "apply_state_root": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
      "exported_receipt_count": 0,
      "mirrored_receipt_count": 0,
      "state_version": 1,
      "ingress_bypassed": true,
      "atomic_guard_enabled": false,
      "receipts": [],
      "state_mirror_updates": []
    }
  ]
}"#,
        )
        .expect("write legacy canonical json");

        let loaded = load_mainline_canonical_store(&store_path).expect("load canonical store");
        assert_eq!(loaded.batches.len(), 1);
        assert_eq!(loaded.batches[0].block_access_list, None);
        assert_eq!(loaded.batches[0].block_access_list_hash, None);

        let _ = fs::remove_file(PathBuf::from(&store_path));
        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn derive_mainline_eth_block_contexts_chain_parent_hashes_and_numbers() {
        let store = MainlineCanonicalStoreV1 {
            schema: "supervm-mainline-canonical/v1".to_string(),
            generated_unix_ms: 1,
            chain_type: "evm".to_string(),
            chain_id: 1,
            batches: vec![
                MainlineCanonicalBatchRecordV1 {
                    seq: 5,
                    source_detail: "a".to_string(),
                    tx_count: 1,
                    tap_requested: 1,
                    tap_accepted: 1,
                    tap_dropped: 0,
                    apply_verified: true,
                    apply_applied: true,
                    apply_state_root: [0x11; 32],
                    block_access_list: Some(sample_block_access_list()),
                    block_access_list_complete: false,
                    block_access_list_hash: Some([0x77; 32]),
                    exported_receipt_count: 1,
                    mirrored_receipt_count: 1,
                    state_version: 7,
                    ingress_bypassed: true,
                    atomic_guard_enabled: false,
                    raw_tx_rlps: Vec::new(),
                    receipts: vec![SupervmEvmExecutionReceiptV1 {
                        chain_type: ChainType::EVM,
                        chain_id: 1,
                        tx_hash: vec![0x22; 32],
                        tx_index: 0,
                        tx_type: TxType::Transfer,
                        receipt_type: Some(2),
                        status_ok: true,
                        gas_used: 21_000,
                        cumulative_gas_used: 21_000,
                        effective_gas_price: Some(7),
                        log_bloom: vec![0u8; 256],
                        revert_data: None,
                        state_root: [0x33; 32],
                        state_version: 7,
                        contract_address: None,
                        logs: Vec::new(),
                    }],
                    state_mirror_updates: Vec::new(),
                },
                MainlineCanonicalBatchRecordV1 {
                    seq: 6,
                    source_detail: "b".to_string(),
                    tx_count: 1,
                    tap_requested: 1,
                    tap_accepted: 1,
                    tap_dropped: 0,
                    apply_verified: true,
                    apply_applied: true,
                    apply_state_root: [0x44; 32],
                    block_access_list: None,
                    block_access_list_complete: false,
                    block_access_list_hash: None,
                    exported_receipt_count: 1,
                    mirrored_receipt_count: 1,
                    state_version: 8,
                    ingress_bypassed: true,
                    atomic_guard_enabled: false,
                    raw_tx_rlps: Vec::new(),
                    receipts: vec![SupervmEvmExecutionReceiptV1 {
                        chain_type: ChainType::EVM,
                        chain_id: 1,
                        tx_hash: vec![0x55; 32],
                        tx_index: 0,
                        tx_type: TxType::Transfer,
                        receipt_type: Some(2),
                        status_ok: true,
                        gas_used: 21_000,
                        cumulative_gas_used: 21_000,
                        effective_gas_price: Some(8),
                        log_bloom: vec![0u8; 256],
                        revert_data: None,
                        state_root: [0x66; 32],
                        state_version: 8,
                        contract_address: None,
                        logs: Vec::new(),
                    }],
                    state_mirror_updates: Vec::new(),
                },
            ],
        };

        let contexts = derive_mainline_eth_block_contexts_v1(&store);
        assert_eq!(contexts.len(), 2);
        assert_eq!(contexts[0].block_number, 5);
        assert_eq!(contexts[0].canonical_batch_seq, 5);
        assert_eq!(contexts[0].parent_block_hash, [0u8; 32]);
        assert_eq!(contexts[0].block_access_list_hash, Some([0x77; 32]));
        assert_eq!(contexts[1].block_number, 6);
        assert_eq!(contexts[1].canonical_batch_seq, 6);
        assert_eq!(contexts[1].parent_block_hash, contexts[0].block_hash);
        assert_eq!(contexts[1].block_access_list_hash, None);
        assert_ne!(contexts[0].block_hash, contexts[1].block_hash);
        let by_seq = derive_mainline_eth_block_context_by_seq_v1(&store, 6).expect("context");
        assert_eq!(by_seq.block_hash, contexts[1].block_hash);
    }

    #[test]
    fn derive_mainline_eth_block_access_list_by_hash_returns_persisted_payload() {
        let store = MainlineCanonicalStoreV1 {
            schema: "supervm-mainline-canonical/v1".to_string(),
            generated_unix_ms: 1,
            chain_type: "evm".to_string(),
            chain_id: 1,
            batches: vec![
                MainlineCanonicalBatchRecordV1 {
                    seq: 5,
                    source_detail: "a".to_string(),
                    tx_count: 1,
                    tap_requested: 1,
                    tap_accepted: 1,
                    tap_dropped: 0,
                    apply_verified: true,
                    apply_applied: true,
                    apply_state_root: [0x11; 32],
                    block_access_list: Some(sample_block_access_list()),
                    block_access_list_complete: false,
                    block_access_list_hash: Some([0x77; 32]),
                    exported_receipt_count: 1,
                    mirrored_receipt_count: 1,
                    state_version: 7,
                    ingress_bypassed: true,
                    atomic_guard_enabled: false,
                    raw_tx_rlps: Vec::new(),
                    receipts: vec![SupervmEvmExecutionReceiptV1 {
                        chain_type: ChainType::EVM,
                        chain_id: 1,
                        tx_hash: vec![0x22; 32],
                        tx_index: 0,
                        tx_type: TxType::Transfer,
                        receipt_type: Some(2),
                        status_ok: true,
                        gas_used: 21_000,
                        cumulative_gas_used: 21_000,
                        effective_gas_price: Some(7),
                        log_bloom: vec![0u8; 256],
                        revert_data: None,
                        state_root: [0x33; 32],
                        state_version: 7,
                        contract_address: None,
                        logs: Vec::new(),
                    }],
                    state_mirror_updates: Vec::new(),
                },
                MainlineCanonicalBatchRecordV1 {
                    seq: 6,
                    source_detail: "b".to_string(),
                    tx_count: 1,
                    tap_requested: 1,
                    tap_accepted: 1,
                    tap_dropped: 0,
                    apply_verified: true,
                    apply_applied: true,
                    apply_state_root: [0x44; 32],
                    block_access_list: None,
                    block_access_list_complete: false,
                    block_access_list_hash: None,
                    exported_receipt_count: 1,
                    mirrored_receipt_count: 1,
                    state_version: 8,
                    ingress_bypassed: true,
                    atomic_guard_enabled: false,
                    raw_tx_rlps: Vec::new(),
                    receipts: vec![SupervmEvmExecutionReceiptV1 {
                        chain_type: ChainType::EVM,
                        chain_id: 1,
                        tx_hash: vec![0x55; 32],
                        tx_index: 0,
                        tx_type: TxType::Transfer,
                        receipt_type: Some(2),
                        status_ok: true,
                        gas_used: 21_000,
                        cumulative_gas_used: 21_000,
                        effective_gas_price: Some(8),
                        log_bloom: vec![0u8; 256],
                        revert_data: None,
                        state_root: [0x66; 32],
                        state_version: 8,
                        contract_address: None,
                        logs: Vec::new(),
                    }],
                    state_mirror_updates: Vec::new(),
                },
            ],
        };

        let contexts = derive_mainline_eth_block_contexts_v1(&store);
        let first =
            derive_mainline_eth_block_access_list_by_hash_v1(&store, contexts[0].block_hash)
                .expect("first access list context");
        assert_eq!(first.block_number, 5);
        assert_eq!(first.canonical_batch_seq, 5);
        assert_eq!(first.block_access_list, Some(sample_block_access_list()));
        assert!(!first.block_access_list_complete);
        assert_eq!(first.block_access_list_hash, Some([0x77; 32]));

        let second =
            derive_mainline_eth_block_access_list_by_hash_v1(&store, contexts[1].block_hash)
                .expect("second access list context");
        assert_eq!(second.block_number, 6);
        assert_eq!(second.block_access_list, None);
        assert_eq!(second.block_access_list_hash, None);

        assert!(derive_mainline_eth_block_access_list_by_hash_v1(&store, [0xff; 32]).is_none());
    }

    #[test]
    fn derive_mainline_eth_fullnode_chain_view_projects_canonical_block_context() {
        let store = MainlineCanonicalStoreV1 {
            schema: "supervm-mainline-canonical/v1".to_string(),
            generated_unix_ms: 1,
            chain_type: "evm".to_string(),
            chain_id: 1,
            batches: vec![MainlineCanonicalBatchRecordV1 {
                seq: 5,
                source_detail: "sample".to_string(),
                tx_count: 1,
                tap_requested: 1,
                tap_accepted: 1,
                tap_dropped: 0,
                apply_verified: true,
                apply_applied: true,
                apply_state_root: [0x41; 32],
                block_access_list: Some(sample_block_access_list()),
                block_access_list_complete: false,
                block_access_list_hash: Some([0x88; 32]),
                exported_receipt_count: 1,
                mirrored_receipt_count: 1,
                state_version: 9,
                ingress_bypassed: true,
                atomic_guard_enabled: false,
                raw_tx_rlps: Vec::new(),
                receipts: vec![SupervmEvmExecutionReceiptV1 {
                    chain_type: ChainType::EVM,
                    chain_id: 1,
                    tx_hash: vec![0x31; 32],
                    tx_index: 0,
                    tx_type: TxType::Transfer,
                    receipt_type: Some(2),
                    status_ok: true,
                    gas_used: 21_000,
                    cumulative_gas_used: 21_000,
                    effective_gas_price: Some(7),
                    log_bloom: vec![0u8; 256],
                    revert_data: None,
                    state_root: [0x51; 32],
                    state_version: 9,
                    contract_address: None,
                    logs: Vec::new(),
                }],
                state_mirror_updates: Vec::new(),
            }],
        };

        let blocks = derive_mainline_eth_fullnode_block_contexts_v1(&store);
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].source,
            novovm_network::EthFullnodeBlockViewSource::CanonicalHostBatch
        );
        assert_eq!(blocks[0].canonical_batch_seq, Some(5));
        assert_eq!(blocks[0].block_access_list_hash, Some([0x88; 32]));

        let view = derive_mainline_eth_fullnode_chain_view_v1(&store).expect("chain view");
        assert_eq!(
            view.source,
            novovm_network::EthFullnodeBlockViewSource::CanonicalHostBatch
        );
        assert_eq!(view.starting_block_number, 5);
        assert_eq!(view.current_block_number, 5);
        assert_eq!(view.highest_block_number, 5);
        assert_eq!(view.current_state_root, [0x41; 32]);
        assert_eq!(view.current_state_version, 9);
        assert_eq!(view.total_blocks, 1);
    }
}
