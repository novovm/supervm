//! First candidates under a pinned fresh genesis; no selected head or signing.
use super::*;

fn validate_first(
    block: &NovNativeDurableBlockV1,
    config: &FreshGenesisConfigV1,
    initial_root: [u8; 32],
) -> Result<()> {
    validate_durable_block_v1(block)?;
    let root_profile = config.root_codec_profile()?;
    if block.header.post_state_root_codec != root_profile.state_root_codec()
        || block.header.cumulative_receipt_root_codec != root_profile.receipt_root_codec()
    {
        bail!("isolated first candidate root codecs differ from pinned fresh genesis profile");
    }
    if block.header.chain_id != config.chain_id
        || block.header.height != 1
        || block.header.parent_block_hash != [0; 32]
        || block.header.aoem_parent.is_some()
        || block.header.pre_state_root != initial_root
        || block.header.timestamp_unix_ms < config.timestamp_unix_ms
        // Genesis starts at sequence zero; execution advances once per tx,
        // not once per block. Bind the exact count rather than accepting any version.
        || block.header.state_version != u64::from(block.header.tx_count)
    {
        bail!("isolated first candidate does not extend the pinned fresh genesis");
    }
    Ok(())
}

/// Validate the complete graph and return an exact key allowlist. Reject orphan
/// artifacts/pins/index entries, selected state and capability downgrades rather
/// than accepting arbitrary keys merely because they have a candidate prefix.
pub(super) fn validated_keys(
    ledger: &NovNativeBlockLedgerV1,
    validation: &FreshGenesisValidationV1<'_>,
) -> Result<Vec<Vec<u8>>> {
    let config = validation.config();
    let chain = config.chain_id;
    let initial_root = validation.compiled().state_root();
    let height = ledger
        .load_candidate_height_index_inner_v1(chain, 1)?
        .context("fresh genesis candidate height index missing")?;
    let children = ledger
        .load_candidate_children_index_inner_v1(chain, [0; 32])?
        .context("fresh genesis candidate children index missing")?;
    if height.block_hashes.is_empty() || height.block_hashes != children.block_hashes {
        bail!("fresh genesis candidate indexes disagree or are empty");
    }
    let mut keys = vec![
        KEY_CANDIDATE_GRAPH_SCHEMA_V1.to_vec(),
        candidate_height_index_key_v1(chain, 1).into_bytes(),
        candidate_children_index_key_v1(chain, &[0; 32]).into_bytes(),
    ];
    for hash in height.block_hashes {
        let record = ledger
            .load_candidate_record_inner_v1(chain, hash)?
            .context("fresh genesis candidate record missing")?;
        if record.candidate_source != CANDIDATE_SOURCE_ISOLATED_V1
            || record.execution_selected_local
            || record.lifecycle_status != CANDIDATE_STATUS_ACTIVE_V1
        {
            bail!("fresh genesis graph contains an unsupported candidate state");
        }
        let block = ledger
            .load_candidate_block_for_record_inner_v1(&record)?
            .context("fresh genesis candidate artifact missing")?;
        validate_first(&block, config, initial_root)?;
        keys.push(candidate_record_key_v1(chain, &hash).into_bytes());
        keys.push(candidate_artifact_key_v1(chain, &hash).into_bytes());
        keys.push(isolated_candidate::pin_key(chain, &hash).into_bytes());
    }
    Ok(keys)
}

impl NovNativeBlockLedgerV1 {
    pub(crate) fn fresh_genesis_seal_config_v1(
        &self,
        chain: u64,
    ) -> Result<Option<(&FreshGenesisConfigV1, [u8; 32])>> {
        self.ensure_schema_v1()?;
        match &self.fresh_genesis_seal_scope {
            Some((config, namespace)) if config.chain_id == chain => Ok(Some((config, *namespace))),
            Some(_) => bail!("fresh genesis signing chain mismatch"),
            None => Ok(None),
        }
    }

    /// Coordinator holds workspace and authority locks; this method holds the
    /// ledger lock through the callback. The read-only view cannot escape it.
    pub(crate) fn with_fresh_genesis_seal_scope_v1<T>(
        path: &Path,
        expected: [u8; 32],
        namespace: [u8; 32],
        block: &NovNativeDurableBlockV1,
        binding: &NovNativeIsolatedExecutionBindingV1,
        action: impl FnOnce(&Self) -> Result<T>,
    ) -> Result<T> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("first signing candidate requires existing ledger")?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("genesis signing ledger lock poisoned"))?;
        let config = load_verified(&ledger, expected, namespace)?;
        if ledger.db.get(KEY_SCHEMA_V1)?.is_some_and(|schema| {
            schema == PROMOTION_SCHEMA.as_bytes() || is_published_schema(&schema)
        }) {
            bail!("pending fresh promotion requires recovery before signing");
        }
        let record = ledger
            .load_candidate_record_inner_v1(config.chain_id, block.header.block_hash)?
            .context("first signing candidate is not registered")?;
        if record.isolated_execution_binding.as_ref() != Some(binding)
            || ledger
                .load_candidate_block_for_record_inner_v1(&record)?
                .as_ref()
                != Some(block)
        {
            bail!("first signing candidate does not match verified live execution");
        }
        let view = Self {
            path: ledger.path.clone(),
            db: Arc::clone(&ledger.db),
            write_lock: Arc::clone(&ledger.write_lock),
            read_only: true,
            isolated_seal_scope: Some(record),
            fresh_successor_parent_target: None,
            fresh_parent_round_height: None,
            fresh_genesis_seal_scope: Some((config, namespace)),
        };
        action(&view)
    }

    /// Workspace coordinator only, under workspace then authority OS locks,
    /// after verifying live genesis and complete AOEM candidate output. Records
    /// historical execution; it never grants signing or selects an execution.
    pub(crate) fn register_fresh_genesis_candidate_v1(
        path: &Path,
        expected: [u8; 32],
        namespace: [u8; 32],
        block: NovNativeDurableBlockV1,
        binding: NovNativeIsolatedExecutionBindingV1,
    ) -> Result<NovNativeBlockCandidateRecordV1> {
        binding.validate()?;
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("fresh genesis candidate requires a reserved ledger")?;
        // Require an existing database before obtaining a writable handle;
        // validate its complete reservation under the write lock below.
        drop(ledger);
        let ledger = Self::open_inner_v1(path, true)?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("genesis candidate ledger lock poisoned"))?;
        let config = load_verified(&ledger, expected, namespace)?;
        validate_first(&block, &config, config.compile()?.state_root())?;
        if ledger.db.get(KEY_SCHEMA_V1)?.is_some_and(|schema| {
            schema == PROMOTION_SCHEMA.as_bytes() || is_published_schema(&schema)
        }) {
            bail!("pending fresh promotion requires recovery before registration");
        }
        let chain = config.chain_id;
        let hash = block.header.block_hash;
        if let Some(record) = ledger.load_candidate_record_inner_v1(chain, hash)? {
            if record.isolated_execution_binding.as_ref() != Some(&binding)
                || ledger
                    .load_candidate_block_for_record_inner_v1(&record)?
                    .as_ref()
                    != Some(&block)
            {
                bail!("fresh genesis candidate registration cannot replace pinned execution");
            }
            return Ok(record);
        }
        let mut record =
            candidate_record_from_block_v1(&block, CANDIDATE_SOURCE_OBSERVED_V1, false, false)?;
        record.candidate_source = CANDIDATE_SOURCE_ISOLATED_V1.to_string();
        record.local_aoem_readback_verified = true;
        record.isolated_execution_binding = Some(binding.clone());
        let mut batch = RocksDbWriteBatch::default();
        batch.put(KEY_SCHEMA_V1, CANDIDATES_SCHEMA.as_bytes());
        put_json_v1(
            &mut batch,
            candidate_artifact_key_v1(chain, &hash).as_bytes(),
            &block,
            "fresh genesis candidate artifact",
        )?;
        put_json_v1(
            &mut batch,
            isolated_candidate::pin_key(chain, &hash).as_bytes(),
            &binding,
            "fresh genesis candidate binding",
        )?;
        ledger.stage_candidate_graph_record_v1(&mut batch, &record)?;
        write_sync_v1(&ledger.db, batch)?;
        load_verified(&ledger, expected, namespace)?;
        let readback = ledger
            .load_candidate_record_inner_v1(chain, hash)?
            .context("fresh genesis candidate readback missing")?;
        if readback != record {
            bail!("fresh genesis candidate readback mismatch");
        }
        Ok(readback)
    }
}

#[cfg(test)]
mod root_codec_tests {
    use super::*;
    use crate::native_block_ledger::tests::TestLedgerV1;
    use crate::tx_ingress::fresh_genesis::{
        GenesisValidatorV1, GENESIS_SCHEMA_RECORD_V2, GENESIS_SCHEMA_V1,
    };

    #[test]
    fn root_codec_first_candidate_rejects_wrong_profile_before_persistence() {
        for schema in [GENESIS_SCHEMA_V1, GENESIS_SCHEMA_RECORD_V2] {
            let test = TestLedgerV1::new("first-root-profile");
            let key = ed25519_dalek::SigningKey::from_bytes(&[1; 32]);
            let config = FreshGenesisConfigV1 {
                schema: schema.into(),
                chain_id: 970042,
                timestamp_unix_ms: 1_900_000_000_000,
                protocol_config_commitment: [7; 32],
                allocations: vec![],
                total_initial_nov: "0".into(),
                validators: vec![GenesisValidatorV1 {
                    public_key: key.verifying_key().to_bytes(),
                    weight: 1,
                }],
            };
            let compiled = config.compile().unwrap();
            NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(
                &test.path,
                &config,
                compiled.config_commitment(),
                [3; 32],
            )
            .unwrap();
            let mut prepared = build_prepared_block_v1(NovNativeBlockCandidateInputV1 {
                context: novovm_protocol::NovBlockExecutionContextV1 {
                    chain_id: config.chain_id,
                    block_height: 1,
                    parent_block_hash: [0; 32],
                    slot: 1,
                    timestamp_unix_ms: config.timestamp_unix_ms,
                },
                tx_hashes: vec![[1; 32]],
                raw_txs: vec![vec![2]],
                pre_state_root: compiled.state_root(),
                aoem_parent: None,
            })
            .unwrap();
            let input = NovNativeBlockCommitInputV1 {
                post_state_root: [4; 32],
                cumulative_receipt_root: [5; 32],
                per_block_receipt_commitments: vec![[6; 32]],
                aoem_batch_id: "first-profile".into(),
                aoem_batch_result_id: "07".repeat(32),
                aoem_evidence_commitment: [8; 32],
                state_version: 1,
            };
            prepared.expected_aoem_batch_id = Some(input.aoem_batch_id.clone());
            prepared.expected_aoem_output_commitment = Some("09".repeat(32));
            let profile = compiled.root_codec_profile();
            let wrong_profile = if profile == NativeRootCodecProfileV1::LegacyWireV1 {
                NativeRootCodecProfileV1::RecordTreeV1
            } else {
                NativeRootCodecProfileV1::LegacyWireV1
            };
            let wrong =
                build_durable_block_with_root_codecs_v2(&prepared, input.clone(), wrong_profile)
                    .unwrap();
            let binding = NovNativeIsolatedExecutionBindingV1 {
                workspace_id: [10; 32],
                plan_commitment: [11; 32],
                output_digest: [12; 32],
            };
            let before = test
                .ledger()
                .db
                .iterator(rocksdb::IteratorMode::Start)
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert!(NovNativeBlockLedgerV1::register_fresh_genesis_candidate_v1(
                &test.path,
                compiled.config_commitment(),
                [3; 32],
                wrong,
                binding.clone(),
            )
            .unwrap_err()
            .to_string()
            .contains("root codecs differ"));
            assert_eq!(
                before,
                test.ledger()
                    .db
                    .iterator(rocksdb::IteratorMode::Start)
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap()
            );
            let correct =
                build_durable_block_with_root_codecs_v2(&prepared, input, profile).unwrap();
            NovNativeBlockLedgerV1::register_fresh_genesis_candidate_v1(
                &test.path,
                compiled.config_commitment(),
                [3; 32],
                correct.clone(),
                binding.clone(),
            )
            .unwrap();
            let seal = crate::native_block_seal::NovNativeBlockSealStoreV1::open(
                &test.path.join("seal-test"),
            )
            .unwrap();
            NovNativeBlockLedgerV1::with_fresh_genesis_seal_scope_v1(
                &test.path,
                compiled.config_commitment(),
                [3; 32],
                &correct,
                &binding,
                |ledger| {
                    seal.prepare_local_subject(
                        ledger,
                        config.chain_id,
                        correct.header.block_hash,
                        compiled.validator_set(),
                        0,
                        None,
                    )
                },
            )
            .unwrap();
        }
    }
}
