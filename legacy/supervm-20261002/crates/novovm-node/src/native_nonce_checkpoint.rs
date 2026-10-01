#![forbid(unsafe_code)]

//! Pure, bounded verification of a legacy snapshot against an explicitly
//! supplied checkpoint and complete local durable-block history. Matching a
//! caller-supplied anchor is not independent proof of its provenance, AOEM
//! execution, a quorum certificate, or permission to activate new state.

use super::*;
use crate::native_block_ledger::{
    validate_durable_block_v1, NovNativeBlockLedgerHeadV1, NovNativeDurableBlockV1,
    NovNativePreparedAoemParentV1,
};
use native_nonce_migration::{
    plan_nonce_migration_v1, NonceMigrationPlanV1, MAX_HISTORY_BYTES_V1,
    MAX_HISTORY_TRANSACTIONS_V1, MAX_SNAPSHOT_BYTES_V1, MAX_TRANSACTION_BYTES_V1,
};

pub const MAX_CHECKPOINT_BLOCKS_V1: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NonceMigrationCheckpointV1 {
    pub chain_id: u64,
    pub namespace_digest: String,
    pub legacy_protocol_config_commitment: String,
    pub tip_block_hash: String,
    pub snapshot_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NonceMigrationCheckpointReportV1 {
    pub schema: &'static str,
    pub migration: NonceMigrationPlanV1,
    pub checkpoint: NonceMigrationCheckpointV1,
    pub block_count: usize,
    pub tx_count: usize,
    pub body_bytes: usize,
    pub checkpoint_anchors_matched: bool,
    pub local_block_commitments_verified: bool,
    pub ordered_history_verified: bool,
    pub receipt_commitments_verified: bool,
    pub snapshot_roots_match_tip: bool,
    pub independent_provenance_verified: bool,
    pub execution_replayed: bool,
    pub aoem_evidence_verified: bool,
    pub qc_verified: bool,
    pub chain_canonical: bool,
    pub activation_ready: bool,
    pub import_performed: bool,
}

fn canonical_checkpoint_hash_v1(value: &str, label: &str) -> Result<[u8; 32]> {
    let parsed = parse_fixed_hex_32_v1(value, label)?;
    if to_hex(&parsed) != value {
        bail!("nonce checkpoint {label} must be canonical lowercase 32-byte hex");
    }
    Ok(parsed)
}

/// Check commitments and legacy nonce reconstruction without reading any
/// environment, file, runtime, or live database. The supplied checkpoint must
/// be obtained independently by the caller; this API cannot authenticate it.
/// The input snapshot is the complete Host store JSON, not an AOEM envelope.
pub fn verify_nonce_checkpoint_v1(
    snapshot_json: &[u8],
    head: &NovNativeBlockLedgerHeadV1,
    blocks: &[NovNativeDurableBlockV1],
    checkpoint: &NonceMigrationCheckpointV1,
) -> Result<NonceMigrationCheckpointReportV1> {
    if snapshot_json.is_empty() || snapshot_json.len() > MAX_SNAPSHOT_BYTES_V1 {
        bail!("nonce checkpoint snapshot exceeds its nonempty 16 MiB bound");
    }
    if blocks.is_empty() || blocks.len() > MAX_CHECKPOINT_BLOCKS_V1 {
        bail!("nonce checkpoint requires between 1 and 512 complete blocks");
    }
    if checkpoint.chain_id == 0 {
        bail!("nonce checkpoint chain must be nonzero");
    }
    canonical_checkpoint_hash_v1(&checkpoint.namespace_digest, "namespace digest")?;
    canonical_checkpoint_hash_v1(
        &checkpoint.legacy_protocol_config_commitment,
        "legacy protocol commitment",
    )?;
    let expected_tip = canonical_checkpoint_hash_v1(&checkpoint.tip_block_hash, "tip block hash")?;
    let expected_snapshot =
        canonical_checkpoint_hash_v1(&checkpoint.snapshot_digest, "snapshot digest")?;
    let actual_snapshot = sha256_bytes_v1(&[
        b"novovm-native-nonce-migration-source-snapshot-v1\0",
        snapshot_json,
    ]);
    if actual_snapshot != expected_snapshot || head.block_hash != expected_tip {
        bail!("nonce checkpoint supplied snapshot or tip anchor does not match");
    }
    if head.schema != "novovm-native-block-ledger-head/v1"
        || head.chain_id != checkpoint.chain_id
        || head.height != blocks.len() as u64
        || head.block_count != blocks.len() as u64
        || !head.canonical_local
        || head.safe
        || head.finalized
        || head.proof_sealed
    {
        bail!("nonce checkpoint head is incomplete, mismatched, or falsely sealed");
    }

    // Apply aggregate and per-item bounds before the shared block validator
    // clones raw payloads, and before constructing a second ordered history.
    let mut tx_count = 0usize;
    let mut body_bytes = 0usize;
    for block in blocks {
        if block.body.raw_txs.is_empty()
            || block.body.raw_txs.len()
                > crate::native_block_ledger::NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1
            || block.body.tx_hashes.len() != block.body.raw_txs.len()
            || block.execution_evidence.per_block_receipt_commitments.len()
                != block.body.raw_txs.len()
        {
            bail!("nonce checkpoint block transaction or receipt count is invalid");
        }
        tx_count = tx_count
            .checked_add(block.body.raw_txs.len())
            .context("nonce checkpoint transaction count overflow")?;
        if tx_count > MAX_HISTORY_TRANSACTIONS_V1 {
            bail!("nonce checkpoint history exceeds its transaction count bound");
        }
        let mut block_bytes = 0usize;
        for raw in &block.body.raw_txs {
            if raw.is_empty() || raw.len() > MAX_TRANSACTION_BYTES_V1 {
                bail!("nonce checkpoint raw transaction exceeds its nonempty 1 MiB bound");
            }
            block_bytes = block_bytes
                .checked_add(raw.len())
                .context("nonce checkpoint block byte count overflow")?;
        }
        if block_bytes > crate::native_block_ledger::NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1 {
            bail!("nonce checkpoint block exceeds its body byte bound");
        }
        body_bytes = body_bytes
            .checked_add(block_bytes)
            .context("nonce checkpoint history byte count overflow")?;
        if body_bytes > MAX_HISTORY_BYTES_V1 {
            bail!("nonce checkpoint history exceeds its 64 MiB bound");
        }
    }
    if head.cumulative_tx_count != tx_count as u64
        || head.cumulative_body_bytes != body_bytes as u64
    {
        bail!("nonce checkpoint head cumulative counters do not match complete history");
    }

    let store: NovNativeExecutionStoreV1 =
        serde_json::from_slice(snapshot_json).context("decode nonce checkpoint Host snapshot")?;
    // The existing state-root codec internally expects JSON-value-compatible
    // numeric fields. Reject unsupported u128 ranges as an error before that
    // codec is invoked on an untrusted offline snapshot.
    serde_json::to_value(&store.module_state)
        .context("nonce checkpoint snapshot cannot encode under the V3 state codec")?;
    let mut raw_history = Vec::with_capacity(tx_count);
    let mut batch_ids = std::collections::BTreeSet::new();
    let mut batch_result_ids = std::collections::BTreeSet::new();
    for (index, block) in blocks.iter().enumerate() {
        validate_durable_block_v1(block).with_context(|| {
            format!("nonce checkpoint block {} commitment validation", index + 1)
        })?;
        let header = &block.header;
        if header.chain_id != checkpoint.chain_id || header.height != (index as u64) + 1 {
            bail!("nonce checkpoint requires complete ordered blocks from height 1");
        }
        if !batch_ids.insert(header.aoem_batch_id.as_str())
            || !batch_result_ids.insert(header.aoem_batch_result_id.as_str())
        {
            bail!("nonce checkpoint repeats an indexed AOEM batch or result identifier");
        }
        if index == 0 {
            // The semantic sequence advances once per committed transaction,
            // including rejected business execution, not once per block.
            if header.parent_block_hash != [0; 32]
                || header.aoem_parent.is_some()
                || header.state_version != u64::from(header.tx_count)
            {
                bail!("nonce checkpoint genesis contains unaccounted pre-chain AOEM state");
            }
        } else {
            let parent = &blocks[index - 1].header;
            let expected_parent = NovNativePreparedAoemParentV1 {
                batch_id: parent.aoem_batch_id.clone(),
                batch_result_id: parent.aoem_batch_result_id.clone(),
                state_root: parent.post_state_root,
                state_root_codec: parent.post_state_root_codec.clone(),
                cumulative_receipt_root: parent.cumulative_receipt_root,
                receipt_root_codec: parent.cumulative_receipt_root_codec.clone(),
                state_version: parent.state_version,
            };
            let next_state_version = parent
                .state_version
                .checked_add(u64::from(header.tx_count))
                .context("nonce checkpoint state version overflow")?;
            if header.parent_block_hash != parent.block_hash
                || header.pre_state_root != parent.post_state_root
                || header.aoem_parent.as_ref() != Some(&expected_parent)
                || header.slot <= parent.slot
                || header.timestamp_unix_ms < parent.timestamp_unix_ms
                || header.state_version != next_state_version
            {
                bail!("nonce checkpoint parent, AOEM state, time, or version continuity mismatch");
            }
        }
        for (tx_index, raw) in block.body.raw_txs.iter().enumerate() {
            let tx = decode_nov_native_tx_wire_v1(raw)
                .context("decode nonce checkpoint canonical raw transaction")?;
            let ir = nov_native_tx_to_adapter_tx_ir_v1(&tx)?;
            let hash = tx_hash_array_from_ir_v1(&ir);
            if hash != block.body.tx_hashes[tx_index] {
                bail!("nonce checkpoint raw transaction does not match committed TxIR hash");
            }
            let receipt = store
                .receipts
                .get(&to_hex(&hash))
                .context("nonce checkpoint committed transaction has no snapshot receipt")?;
            if full_native_receipt_commitment_v1(receipt)?
                != block.execution_evidence.per_block_receipt_commitments[tx_index]
            {
                bail!("nonce checkpoint full receipt commitment mismatch");
            }
            raw_history.push(raw.clone());
        }
    }
    let tip = &blocks.last().context("nonce checkpoint has no tip")?.header;
    if head.block_hash != tip.block_hash
        || head.post_state_root != tip.post_state_root
        || head.cumulative_receipt_root != tip.cumulative_receipt_root
        || head.state_version != tip.state_version
        || head.slot != tip.slot
        || head.timestamp_unix_ms != tip.timestamp_unix_ms
    {
        bail!("nonce checkpoint head does not exactly describe the last supplied block");
    }
    if native_semantic_ledger_state_digest_v1(&store.module_state) != to_hex(&tip.post_state_root)
        || native_execution_receipt_root_v2(&store)? != to_hex(&tip.cumulative_receipt_root)
        || store.module_state.aoem_semantic_ledger_sequence != tip.state_version
    {
        bail!("nonce checkpoint snapshot V3 state or V2 receipt roots do not match the tip");
    }
    let migration = plan_nonce_migration_v1(
        snapshot_json,
        &raw_history,
        checkpoint.chain_id,
        &checkpoint.namespace_digest,
        &checkpoint.legacy_protocol_config_commitment,
    )?;
    Ok(NonceMigrationCheckpointReportV1 {
        schema: "novovm-native-nonce-migration-checkpoint-report/v1",
        migration,
        checkpoint: checkpoint.clone(),
        block_count: blocks.len(),
        tx_count,
        body_bytes,
        checkpoint_anchors_matched: true,
        local_block_commitments_verified: true,
        ordered_history_verified: true,
        receipt_commitments_verified: true,
        snapshot_roots_match_tip: true,
        independent_provenance_verified: false,
        execution_replayed: false,
        aoem_evidence_verified: false,
        qc_verified: false,
        chain_canonical: false,
        activation_ready: false,
        import_performed: false,
    })
}

#[cfg(test)]
pub(crate) use tests::test_fixture_v1;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_block_ledger::{
        NovNativeBlockCandidateInputV1, NovNativeBlockCommitInputV1, NovNativeBlockLedgerV1,
    };
    use novovm_protocol::{
        NovBlockExecutionContextV1, NovExecutionModeV1, NovFeePolicyV1, NovVerificationModeV1,
    };

    const CHAIN: u64 = 98_917_902;

    fn signed_tx_v1(nonce: u64) -> NovNativeTxWireV1 {
        let seed = [0x69; 32];
        let caller = novovm_adapter_novovm::address_from_seed_v1(seed);
        let account = to_hex_prefixed_v1(&caller);
        let mut tx = NovNativeTxWireV1 {
            chain_id: CHAIN,
            kind: NovTxKindV1::Execute(novovm_protocol::NovExecuteTxV1 {
                caller,
                account_id: Some(account.clone()),
                fee_owner_account_id: Some(account.clone()),
                nonce_owner_account_id: Some(account),
                target: NovExecutionTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: br#"{"asset":"NOV","amount":1}"#.to_vec(),
                execution_mode: NovExecutionModeV1::Standard,
                execution_policy: NovExecutionPolicyV1::Standard,
                privacy_mode: NovPrivacyModeV1::Public,
                verification_mode: NovVerificationModeV1::Standard,
                fee_policy: NovFeePolicyV1 {
                    pay_asset: "NOV".to_string(),
                    max_pay_amount: 100,
                    slippage_bps: 0,
                },
                gas_like_limit: Some(90_000),
                nonce,
            }),
            signature: Vec::new(),
        };
        let ir = nov_native_tx_to_adapter_tx_ir_v1(&tx).unwrap();
        tx.signature = novovm_adapter_novovm::signature_payload_with_seed_v1(&ir, seed);
        tx
    }

    fn append_legacy_tx_v1(store: &mut NovNativeExecutionStoreV1, tx: &NovNativeTxWireV1) {
        let ir = nov_native_tx_to_adapter_tx_ir_v1(tx).unwrap();
        let hash = tx_hash_array_from_ir_v1(&ir);
        let NovTxKindV1::Execute(execute) = &tx.kind else {
            unreachable!()
        };
        let mut identity = b"account:".to_vec();
        identity.extend_from_slice(execute.nonce_owner_account_id.as_ref().unwrap().as_bytes());
        let identity_key = native_auth_nonce_identity_key_v1(CHAIN, &identity);
        let ledger_key = native_auth_nonce_ledger_key_v1(&(CHAIN, identity, execute.nonce));
        store
            .module_state
            .native_auth_next_nonces
            .insert(identity_key, execute.nonce + 1);
        store.module_state.native_auth_nonce_reservations.insert(
            ledger_key,
            to_hex(&native_auth_nonce_reservation_id_v1(hash, &tx.signature)),
        );
        let mut subject = subject_meta_from_execute_tx_v1(execute);
        subject.key_algo = "ed25519".to_string();
        subject.policy_enforced = true;
        let request = nov_native_tx_to_execution_request_v1(tx).unwrap().unwrap();
        let receipt = build_success_native_receipt_v1(
            &request,
            &unresolved_settled_fee_v1(&request),
            &subject,
            "treasury",
            &request.method,
            Vec::new(),
        );
        store.receipts.insert(to_hex(&hash), receipt);
    }

    /// Test data represents internally consistent, unsealed local claims only;
    /// it deliberately does not run AOEM or establish external provenance.
    pub(crate) fn test_fixture_v1() -> (
        Vec<u8>,
        NovNativeBlockLedgerHeadV1,
        Vec<NovNativeDurableBlockV1>,
        NonceMigrationCheckpointV1,
        std::path::PathBuf,
    ) {
        fixture_with_fault_v1(None)
    }

    #[derive(Clone, Copy)]
    enum FixtureFaultV1 {
        CanonicalTxHash,
        ReceiptCommitment,
        GenesisStateVersion,
        SkippedStateVersion,
        SnapshotStateRoot,
        SnapshotReceiptRoot,
        InvalidSignature,
    }

    fn fixture_with_fault_v1(
        fault: Option<FixtureFaultV1>,
    ) -> (
        Vec<u8>,
        NovNativeBlockLedgerHeadV1,
        Vec<NovNativeDurableBlockV1>,
        NonceMigrationCheckpointV1,
        std::path::PathBuf,
    ) {
        fixture_with_counts_v1(fault, &[1, 1])
    }

    fn fixture_with_counts_v1(
        fault: Option<FixtureFaultV1>,
        counts: &[usize],
    ) -> (
        Vec<u8>,
        NovNativeBlockLedgerHeadV1,
        Vec<NovNativeDurableBlockV1>,
        NonceMigrationCheckpointV1,
        std::path::PathBuf,
    ) {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "novovm-nonce-checkpoint-{}-{serial}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let ledger = NovNativeBlockLedgerV1::open(&path).unwrap();
        ledger
            .bind_aoem_ownership(CHAIN, &"ab".repeat(32), &"cd".repeat(32))
            .unwrap();
        let mut store = NovNativeExecutionStoreV1 {
            authority_chain_id: Some(CHAIN),
            authority_namespace_digest: "ab".repeat(32),
            ..Default::default()
        };
        store.module_state.native_auth_nonce_identity_scheme.clear();
        store.module_state.protocol_config_commitment = "cd".repeat(32);
        let mut blocks: Vec<NovNativeDurableBlockV1> = Vec::new();
        let mut next_nonce = 0u64;
        for (block_index, count) in counts.iter().copied().enumerate() {
            let height = block_index as u64 + 1;
            let mut transactions = Vec::new();
            let mut hashes = Vec::new();
            let mut raw_txs = Vec::new();
            for nonce in next_nonce..next_nonce + count as u64 {
                let mut tx = signed_tx_v1(nonce);
                if nonce == 0 && matches!(fault, Some(FixtureFaultV1::InvalidSignature)) {
                    tx.signature[95] ^= 1;
                }
                let ir = nov_native_tx_to_adapter_tx_ir_v1(&tx).unwrap();
                hashes.push(tx_hash_array_from_ir_v1(&ir));
                raw_txs.push(novovm_protocol::encode_nov_native_tx_wire_v1(&tx).unwrap());
                transactions.push(tx);
            }
            next_nonce += count as u64;
            let mut committed_hashes = hashes.clone();
            if block_index == 0 && matches!(fault, Some(FixtureFaultV1::CanonicalTxHash)) {
                committed_hashes[0][0] ^= 1;
            }
            let pre_state_root = parse_fixed_hex_32_v1(
                &native_semantic_ledger_state_digest_v1(&store.module_state),
                "fixture state",
            )
            .unwrap();
            let parent = blocks.last().map(|block| &block.header);
            let prepared = ledger
                .prepare(NovNativeBlockCandidateInputV1 {
                    context: NovBlockExecutionContextV1 {
                        chain_id: CHAIN,
                        block_height: height,
                        parent_block_hash: parent
                            .map(|header| header.block_hash)
                            .unwrap_or([0; 32]),
                        slot: height,
                        timestamp_unix_ms: 1_900_000_000_000 + height,
                    },
                    tx_hashes: committed_hashes,
                    raw_txs,
                    pre_state_root,
                    aoem_parent: parent.map(|header| NovNativePreparedAoemParentV1 {
                        batch_id: header.aoem_batch_id.clone(),
                        batch_result_id: header.aoem_batch_result_id.clone(),
                        state_root: header.post_state_root,
                        state_root_codec: header.post_state_root_codec.clone(),
                        cumulative_receipt_root: header.cumulative_receipt_root,
                        receipt_root_codec: header.cumulative_receipt_root_codec.clone(),
                        state_version: header.state_version,
                    }),
                })
                .unwrap();
            for tx in &transactions {
                append_legacy_tx_v1(&mut store, tx);
            }
            let state_version = next_nonce
                + u64::from(
                    matches!(fault, Some(FixtureFaultV1::GenesisStateVersion))
                        || (block_index == 1
                            && matches!(fault, Some(FixtureFaultV1::SkippedStateVersion))),
                );
            store.module_state.aoem_semantic_ledger_sequence = state_version;
            let batch_id = format!("nonce-checkpoint-fixture-{height}");
            let bound = ledger
                .bind_expected_aoem_batch_id(&prepared, &batch_id, &format!("{height:064x}"))
                .unwrap();
            let mut post_state_root = parse_fixed_hex_32_v1(
                &native_semantic_ledger_state_digest_v1(&store.module_state),
                "fixture state",
            )
            .unwrap();
            let mut receipt_root = parse_fixed_hex_32_v1(
                &native_execution_receipt_root_v2(&store).unwrap(),
                "fixture receipt root",
            )
            .unwrap();
            let mut receipt_commitments = hashes
                .iter()
                .map(|hash| {
                    full_native_receipt_commitment_v1(store.receipts.get(&to_hex(hash)).unwrap())
                        .unwrap()
                })
                .collect::<Vec<_>>();
            if block_index + 1 == counts.len()
                && matches!(fault, Some(FixtureFaultV1::SnapshotStateRoot))
            {
                post_state_root[0] ^= 1;
            }
            if block_index + 1 == counts.len()
                && matches!(fault, Some(FixtureFaultV1::SnapshotReceiptRoot))
            {
                receipt_root[0] ^= 1;
            }
            if block_index == 0 && matches!(fault, Some(FixtureFaultV1::ReceiptCommitment)) {
                receipt_commitments[0][0] ^= 1;
            }
            blocks.push(
                ledger
                    .commit(
                        &bound,
                        NovNativeBlockCommitInputV1 {
                            post_state_root,
                            cumulative_receipt_root: receipt_root,
                            per_block_receipt_commitments: receipt_commitments,
                            aoem_batch_id: batch_id,
                            aoem_batch_result_id: format!("{:064x}", height + 100),
                            aoem_evidence_commitment: [0x71; 32],
                            state_version,
                        },
                    )
                    .unwrap(),
            );
        }
        let head = ledger.load_head(CHAIN).unwrap().unwrap();
        drop(ledger);
        let snapshot = serde_json::to_vec(&store).unwrap();
        let checkpoint = NonceMigrationCheckpointV1 {
            chain_id: CHAIN,
            namespace_digest: store.authority_namespace_digest,
            legacy_protocol_config_commitment: store.module_state.protocol_config_commitment,
            tip_block_hash: to_hex(&head.block_hash),
            snapshot_digest: to_hex(&sha256_bytes_v1(&[
                b"novovm-native-nonce-migration-source-snapshot-v1\0",
                &snapshot,
            ])),
        };
        (snapshot, head, blocks, checkpoint, path)
    }

    #[test]
    fn native_nonce_checkpoint_verifies_local_claims_without_promoting_trust() {
        let (snapshot, head, blocks, checkpoint, _path) = test_fixture_v1();
        let report = verify_nonce_checkpoint_v1(&snapshot, &head, &blocks, &checkpoint).unwrap();
        assert_eq!(report.block_count, 2);
        assert_eq!(report.tx_count, 2);
        assert!(
            report.checkpoint_anchors_matched
                && report.local_block_commitments_verified
                && report.ordered_history_verified
                && report.receipt_commitments_verified
                && report.snapshot_roots_match_tip
        );
        assert!(
            !report.independent_provenance_verified
                && !report.execution_replayed
                && !report.aoem_evidence_verified
                && !report.qc_verified
                && !report.chain_canonical
                && !report.activation_ready
                && !report.import_performed
        );
        assert!(
            !report.migration.activation_ready
                && !report.migration.import_performed
                && !report.migration.snapshot_provenance_verified
                && !report.migration.qc_verified
        );
        assert_eq!(
            report,
            verify_nonce_checkpoint_v1(&snapshot, &head, &blocks, &checkpoint).unwrap()
        );
    }

    #[test]
    fn native_nonce_checkpoint_state_version_advances_per_transaction_not_block() {
        let (snapshot, head, blocks, checkpoint, _path) = fixture_with_counts_v1(None, &[2, 3]);
        assert_eq!(blocks[0].header.state_version, 2);
        assert_eq!(blocks[1].header.state_version, 5);
        let report = verify_nonce_checkpoint_v1(&snapshot, &head, &blocks, &checkpoint).unwrap();
        assert_eq!(report.tx_count, 5);
        assert_eq!(report.block_count, 2);
        let (snapshot, head, blocks, checkpoint, _path) =
            fixture_with_counts_v1(Some(FixtureFaultV1::SkippedStateVersion), &[2, 3]);
        assert_eq!(blocks[1].header.state_version, 6);
        assert!(verify_nonce_checkpoint_v1(&snapshot, &head, &blocks, &checkpoint).is_err());
    }

    #[test]
    fn native_nonce_checkpoint_rejects_anchor_and_domain_mismatch() {
        let (snapshot, head, blocks, checkpoint, _path) = test_fixture_v1();
        for field in 0..5 {
            let mut wrong = checkpoint.clone();
            match field {
                0 => wrong.snapshot_digest = "01".repeat(32),
                1 => wrong.tip_block_hash = "02".repeat(32),
                2 => wrong.chain_id += 1,
                3 => wrong.namespace_digest = "03".repeat(32),
                _ => wrong.legacy_protocol_config_commitment = "04".repeat(32),
            }
            assert!(verify_nonce_checkpoint_v1(&snapshot, &head, &blocks, &wrong).is_err());
        }
        let mut upper = checkpoint.clone();
        upper.namespace_digest.make_ascii_uppercase();
        assert!(verify_nonce_checkpoint_v1(&snapshot, &head, &blocks, &upper).is_err());
        let mut value = serde_json::to_value(&checkpoint).unwrap();
        value["activation_ready"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<NonceMigrationCheckpointV1>(value).is_err());
    }

    #[test]
    fn native_nonce_checkpoint_rejects_missing_reordered_and_corrupt_history() {
        let (snapshot, head, blocks, checkpoint, _path) = test_fixture_v1();
        assert!(verify_nonce_checkpoint_v1(&snapshot, &head, &[], &checkpoint).is_err());
        assert!(verify_nonce_checkpoint_v1(&snapshot, &head, &blocks[1..], &checkpoint).is_err());
        let mut reversed = blocks.clone();
        reversed.reverse();
        assert!(verify_nonce_checkpoint_v1(&snapshot, &head, &reversed, &checkpoint).is_err());
        let mut corrupt = blocks.clone();
        corrupt[0].body.raw_txs[0][0] ^= 1;
        assert!(verify_nonce_checkpoint_v1(&snapshot, &head, &corrupt, &checkpoint).is_err());
        let mut corrupt = blocks.clone();
        corrupt[0].execution_evidence.per_block_receipt_commitments[0][0] ^= 1;
        assert!(verify_nonce_checkpoint_v1(&snapshot, &head, &corrupt, &checkpoint).is_err());
    }

    #[test]
    fn native_nonce_checkpoint_rejects_forged_finality_and_head_counters() {
        let (snapshot, head, blocks, checkpoint, _path) = test_fixture_v1();
        for field in 0..8 {
            let mut changed = head.clone();
            match field {
                0 => changed.proof_sealed = true,
                1 => changed.finalized = true,
                2 => changed.safe = true,
                3 => changed.canonical_local = false,
                4 => changed.cumulative_body_bytes += 1,
                5 => changed.cumulative_tx_count += 1,
                6 => changed.state_version += 1,
                _ => changed.timestamp_unix_ms += 1,
            }
            assert!(verify_nonce_checkpoint_v1(&snapshot, &changed, &blocks, &checkpoint).is_err());
        }
        let mut sealed = blocks.clone();
        sealed[0].header.proof_sealed = true;
        assert!(verify_nonce_checkpoint_v1(&snapshot, &head, &sealed, &checkpoint).is_err());
    }

    #[test]
    fn native_nonce_checkpoint_rejects_reanchored_snapshot_changes() {
        let (snapshot, head, blocks, checkpoint, _path) = test_fixture_v1();
        for mutation in 0..4 {
            let mut store: NovNativeExecutionStoreV1 = serde_json::from_slice(&snapshot).unwrap();
            match mutation {
                0 => store.module_state.treasury_reserve_bucket_nov += 1,
                1 => store.module_state.aoem_semantic_ledger_sequence += 1,
                2 => {
                    store.receipts.values_mut().next().unwrap().failure_reason =
                        Some("forged".into())
                }
                _ => store.module_state.treasury_reserve_bucket_nov = u128::MAX,
            }
            let changed = serde_json::to_vec(&store).unwrap();
            let mut anchored = checkpoint.clone();
            anchored.snapshot_digest = to_hex(&sha256_bytes_v1(&[
                b"novovm-native-nonce-migration-source-snapshot-v1\0",
                &changed,
            ]));
            assert!(verify_nonce_checkpoint_v1(&changed, &head, &blocks, &anchored).is_err());
        }
    }

    #[test]
    fn native_nonce_checkpoint_checks_semantics_after_valid_hash_commitments() {
        for fault in [
            FixtureFaultV1::CanonicalTxHash,
            FixtureFaultV1::ReceiptCommitment,
            FixtureFaultV1::GenesisStateVersion,
            FixtureFaultV1::SkippedStateVersion,
            FixtureFaultV1::SnapshotStateRoot,
            FixtureFaultV1::SnapshotReceiptRoot,
            FixtureFaultV1::InvalidSignature,
        ] {
            let (snapshot, head, blocks, checkpoint, _path) = fixture_with_fault_v1(Some(fault));
            for block in &blocks {
                validate_durable_block_v1(block).expect("fault fixture retains valid block hashes");
            }
            assert!(verify_nonce_checkpoint_v1(&snapshot, &head, &blocks, &checkpoint).is_err());
        }
    }

    #[test]
    fn native_nonce_checkpoint_checks_bounds_before_payload_validation() {
        let (snapshot, head, blocks, checkpoint, _path) = test_fixture_v1();
        let oversized = vec![b' '; MAX_SNAPSHOT_BYTES_V1 + 1];
        assert!(
            verify_nonce_checkpoint_v1(&oversized, &head, &blocks, &checkpoint)
                .unwrap_err()
                .to_string()
                .contains("16 MiB bound")
        );
        let oversized_blocks = vec![blocks[0].clone(); MAX_CHECKPOINT_BLOCKS_V1 + 1];
        assert!(
            verify_nonce_checkpoint_v1(&snapshot, &head, &oversized_blocks, &checkpoint)
                .unwrap_err()
                .to_string()
                .contains("512 complete blocks")
        );
        let mut raw_oversized = blocks.clone();
        raw_oversized[0].body.raw_txs[0] = vec![0; MAX_TRANSACTION_BYTES_V1 + 1];
        assert!(
            verify_nonce_checkpoint_v1(&snapshot, &head, &raw_oversized, &checkpoint)
                .unwrap_err()
                .to_string()
                .contains("1 MiB bound")
        );
    }
}
