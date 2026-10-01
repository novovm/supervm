// Included only in the existing lib-test candidate scope. The integration
// harness launches this worker after stopping the four real node processes.
// This is a serial scheduling reference using the same business implementation,
// plus independent fixture arithmetic; it is not a second protocol interpreter.

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferProcessParityInputV1 {
    tip_height: u64,
}

#[derive(serde::Serialize)]
struct TransferProcessParityEconomicV1 {
    balances: std::collections::BTreeMap<String, u128>,
    next_nonces: std::collections::BTreeMap<String, u64>,
    fees: u128,
    reserve_bucket: u128,
    fee_bucket: u128,
    risk_buffer: u128,
}

#[derive(serde::Serialize)]
struct TransferProcessParityReportV1<'a> {
    accepted: bool,
    scope: &'static str,
    state_root: String,
    receipt_root: String,
    receipts: &'a std::collections::BTreeMap<String, NovNativeExecutionReceiptV1>,
    economic: TransferProcessParityEconomicV1,
    block_count: u64,
    tx_count: usize,
}

fn transfer_process_parity_check_ingress_v1(
    block: &NovNativeDurableBlockV1,
    actual: &NovNativeExecutionStoreV1,
) -> anyhow::Result<()> {
    // The observed machine diagnostics may be reused, but ordered precommit
    // identities and successful admission are derived from the actual raw body.
    let size = NOV_NATIVE_AOEM_CONSENSUS_BATCH_CHUNK_SIZE_V1;
    for (chunk_index, raws) in block.body.raw_txs.chunks(size).enumerate() {
        let (wire, plan_id) = build_native_aoem_raw_tx_batch_ops_wire_v1(raws, size)?;
        let digest = to_hex(&sha256_bytes_v1(&[
            b"novovm-native-aoem-semantic-wire-digest-v1",
            &wire.bytes,
        ]));
        for (offset, raw) in raws.iter().enumerate() {
            let index = chunk_index * size + offset;
            let hash = canonical_nov_native_tx_hash_from_payload_v1(raw)?;
            anyhow::ensure!(
                block.body.tx_hashes[index] == hash,
                "raw transaction hash differs"
            );
            let receipt = actual
                .receipts
                .get(&to_hex(&hash))
                .context("receipt missing")?;
            let meta = receipt
                .aoem_semantic_ingress
                .as_ref()
                .context("ingress missing")?;
            anyhow::ensure!(
                receipt.tx_hash == to_hex(&hash)
                    && meta.execution_kernel == "AOEM"
                    && meta.semantic_entry == native_aoem_raw_tx_batch_precommit_entry_v1()
                    && meta.enabled
                    && meta.submitted
                    && meta.algebraic_semantic_entry
                    && meta.batch_mode
                    && meta.ingress_scope == "raw_tx_batch_precommit_item"
                    && meta.plan_id == plan_id
                    && meta.batch_plan_id == Some(plan_id)
                    && meta.wire_digest == digest
                    && meta.op_count == raws.len()
                    && meta.batch_size == raws.len()
                    && meta.batch_item_index == Some(index)
                    && meta.batch_item_count == Some(block.body.raw_txs.len())
                    && meta.processed_ops as usize == raws.len()
                    && meta.success_ops as usize == raws.len()
                    && meta.fallback_reason.is_none()
                    && receipt.aoem_semantic_commit
                        == build_native_receipt_aoem_semantic_commit_v1(receipt),
                "receipt precommit is not bound to the finalized ordered body"
            );
        }
    }
    Ok(())
}

fn transfer_process_serial_parity_run_v1() -> anyhow::Result<()> {
    use crate::native_root_codecs::NativeRootCodecProfileV1;
    use crate::tx_ingress::fresh_genesis::FreshGenesisConfigV1;

    let input_path = std::env::var_os("NOVOVM_TRANSFER_PARITY_INPUT")
        .context("NOVOVM_TRANSFER_PARITY_INPUT is required; this worker never skips")?;
    let output_path = std::env::var_os("NOVOVM_TRANSFER_PARITY_OUTPUT")
        .context("NOVOVM_TRANSFER_PARITY_OUTPUT is required")?;
    let input_bytes = std::fs::read(input_path)?;
    anyhow::ensure!(
        input_bytes.len() <= 1024,
        "parity input exceeds fixture bounds"
    );
    let input: TransferProcessParityInputV1 = serde_json::from_slice(&input_bytes)?;
    // Seven real transactions in this fixture imply at most seven nonempty
    // blocks. This is a test-input bound, not a chain or ledger capacity limit.
    anyhow::ensure!(
        (1..=7).contains(&input.tip_height),
        "invalid parity tip height"
    );
    let genesis_bytes = std::fs::read(std::env::current_dir()?.join("genesis.json"))?;
    anyhow::ensure!(
        genesis_bytes.len() <= 1024 * 1024,
        "genesis fixture exceeds bounds"
    );
    let config: FreshGenesisConfigV1 = serde_json::from_slice(&genesis_bytes)?;
    let compiled = config.compile()?;
    anyhow::ensure!(
        compiled.root_codec_profile() == NativeRootCodecProfileV1::RecordTreeV1,
        "parity fixture requires explicit record genesis"
    );
    let chain = config.chain_id;
    let params = serde_json::json!({
        "chain_id": chain,
        "aoem_owned_gate_config": {"production_candidate": true}
    });
    let native_path = resolve_native_execution_store_path_from_params_v1(&params)
        .context("worker requires the node's explicit native store path")?;
    let ledger_path = nov_native_block_ledger_rocksdb_path_v1(&native_path);
    let namespace = native_aoem_owned_state_namespace_digest_v1(&params, chain);
    let namespace_bytes = parse_fixed_hex_32_v1(&namespace, "parity namespace")?;
    let pin = compiled.config_commitment();

    let mut blocks = Vec::new();
    for height in 1..=input.tip_height {
        let (block, proof) = NovNativeBlockLedgerV1::load_fresh_finalized_block_by_height_v1(
            &ledger_path,
            pin,
            namespace_bytes,
            height,
        )?
        .context("finalized block missing")?;
        let crate::native_block_seal::round_message::NovNativeSealRoundMessageV1::DecisionCertificateV3 {
            decision, ..
        } = &proof.witness else {
            anyhow::bail!("parity requires real decision V3 finality");
        };
        decision.verify(&proof.authority.validator_set)?;
        anyhow::ensure!(
            proof.authority.validator_set.validators.len() == 4
                && proof
                    .authority
                    .validator_set
                    .validators
                    .iter()
                    .all(|validator| validator.weight == 1)
                && proof.authority.validator_set.total_weight == 4
                && proof.authority.validator_set.quorum_weight == 3
                && (3..=4).contains(&decision.votes.len())
                && decision.signed_weight >= 3,
            "parity requires four validators and a 3-of-4 decision"
        );
        anyhow::ensure!(
            block.header.chain_id == chain
                && block.header.height == height
                && block.header.post_state_root_codec
                    == compiled.root_codec_profile().state_root_codec()
                && block.header.cumulative_receipt_root_codec
                    == compiled.root_codec_profile().receipt_root_codec(),
            "finalized block domain/profile differs"
        );
        blocks.push(block);
    }
    let (binding, _, archived_tip) = NovNativeBlockLedgerV1::load_fresh_finalized_execution_v1(
        &ledger_path,
        pin,
        namespace_bytes,
        input.tip_height,
    )?;
    let (live_tip, _) = NovNativeBlockLedgerV1::load_fresh_finalized_tip_archive_v1(
        &ledger_path,
        pin,
        namespace_bytes,
        binding.workspace_id,
    )?;
    anyhow::ensure!(
        live_tip.block == archived_tip && blocks.last() == Some(&archived_tip),
        "requested parity height is not the complete live finalized tip"
    );
    let parent =
        workspace::load_finalized_genesis_parent_v1(chain, binding.workspace_id, pin, &params)?;
    anyhow::ensure!(
        parent.block() == &archived_tip && parent.output_digest() == binding.output_digest,
        "authority output differs from archive"
    );
    let actual = parent.state();

    // Fixture intent is fixed independently of what the node happened to write.
    // In particular the failed payment must still consume nonce 3 and its fee.
    let specifications = [
        (64, 0, 128, 1, true, 45),
        (64, 1, 201, 100, true, 45),
        (65, 0, 202, 11, true, 45),
        (64, 2, 201, 50, true, 45),
        (64, 3, 202, 2_000_000, false, 46),
        (64, 4, 64, 7, true, 45),
        (64, 5, 201, 9, true, 45),
    ];
    let expected: std::collections::BTreeMap<_, _> = specifications
        .iter()
        .map(|&(sender, nonce, recipient, amount, status, fee)| {
            let raw = transfer_candidate_raw(chain, nonce, [sender; 32], [recipient; 32], amount);
            let hash = canonical_nov_native_tx_hash_from_payload_v1(&raw).unwrap();
            (hash, (raw, status, fee))
        })
        .collect();
    anyhow::ensure!(expected.len() == 7, "fixture transaction collision");
    anyhow::ensure!(
        config.total_initial_nov == "2000000" && config.allocations.len() == 2,
        "mixed fixture genesis allocations differ"
    );
    for seed in [64, 65] {
        anyhow::ensure!(
            native_account_asset_balance_v1(
                compiled.initial_store(),
                &transfer_candidate_account([seed; 32]),
                "NOV"
            ) == 1_000_000,
            "fixture sender allocation differs"
        );
    }

    let mut serial = compiled.initial_store().clone();
    bind_native_execution_store_authority_domain_v1(&mut serial, chain, &namespace)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut shared_sender_block = false;
    for block in &blocks {
        anyhow::ensure!(
            native_record_commitment::consensus_state_root_v1(&serial.module_state)?
                == block.header.pre_state_root,
            "serial pre-state differs at height {}",
            block.header.height
        );
        transfer_process_parity_check_ingress_v1(block, actual)?;
        let mut sender_a_count = 0;
        let mut commitments = Vec::new();
        for raw in &block.body.raw_txs {
            let hash = canonical_nov_native_tx_hash_from_payload_v1(raw)?;
            let (expected_raw, status, fee) =
                expected.get(&hash).context("unexpected transaction")?;
            anyhow::ensure!(
                expected_raw == raw && seen.insert(hash),
                "changed or duplicate raw transaction"
            );
            let transaction = decode_nov_native_tx_wire_v1(raw)?;
            let ir = nov_native_tx_to_adapter_tx_ir_v1(&transaction)?;
            verify_nov_native_auth_v1(&params, &transaction, &ir, hash)?;
            let reservation = transfer_candidate_reservation(raw);
            anyhow::ensure!(
                check_nov_native_durable_auth_reservation_v1(&serial, &reservation)?.is_none(),
                "serial transaction was already committed"
            );
            let request = native_transfer_dispatch::fee_request_v1(&transaction, hash)?;
            let subject = fallback_execution_subject_meta_v1(&request);
            sender_a_count +=
                usize::from(subject.account_id == transfer_candidate_account([64; 32]));
            let ingress = actual.receipts[&reservation.tx_hash]
                .aoem_semantic_ingress
                .clone()
                .context("actual precommit ingress missing")?;
            native_transfer_record_execution::execute_segment_v1(
                &mut serial,
                &[native_transfer_dispatch::Item {
                    transaction: &transaction,
                    request: &request,
                    subject: &subject,
                    reservation: &reservation,
                    ingress,
                }],
                u128::from(block.header.timestamp_unix_ms),
            )?;
            let receipt = &serial.receipts[&reservation.tx_hash];
            anyhow::ensure!(
                receipt.status == *status
                    && receipt.settled_fee_nov == *fee
                    && receipt.paid_amount == *fee
                    && receipt.paid_asset == "NOV",
                "independent fee/status expectation differs"
            );
            if !status {
                anyhow::ensure!(
                    receipt.failure_reason.as_deref().is_some_and(
                        |reason| reason.starts_with("native.transfer.insufficient NOV")
                    ),
                    "failed transaction has the wrong business failure"
                );
            }
            anyhow::ensure!(
                receipt == &actual.receipts[&reservation.tx_hash],
                "serial full receipt differs for {}",
                reservation.tx_hash
            );
            commitments.push(full_native_receipt_commitment_v1(receipt)?);
        }
        shared_sender_block |= sender_a_count >= 2;
        anyhow::ensure!(
            native_record_commitment::consensus_state_root_v1(&serial.module_state)?
                == block.header.post_state_root,
            "serial post-state differs at height {}",
            block.header.height
        );
        anyhow::ensure!(
            native_record_commitment::cumulative_receipt_root_v1(&serial)?
                == block.header.cumulative_receipt_root,
            "serial receipt root differs at height {}",
            block.header.height
        );
        anyhow::ensure!(
            crate::native_block_ledger::nov_native_block_receipt_root_v1(
                &block.body.tx_hashes,
                &commitments
            )? == block.header.block_receipt_root,
            "serial block receipt root differs at height {}",
            block.header.height
        );
    }
    anyhow::ensure!(
        seen.len() == 7 && actual.receipts.len() == 7 && shared_sender_block,
        "fixture must finalize all seven transactions and a same-block signer conflict"
    );
    anyhow::ensure!(&serial == actual, "serial complete typed Store differs");
    anyhow::ensure!(
        serde_json::to_vec(&serial)? == serde_json::to_vec(actual)?,
        "serial complete Store bytes differ"
    );

    let mut balances = std::collections::BTreeMap::new();
    for (label, seed, expected_balance) in [
        ("A", 64, 999_569),
        ("C", 65, 999_944),
        ("B", 201, 159),
        ("D", 202, 11),
        ("bootstrap", 128, 1),
    ] {
        let balance =
            native_account_asset_balance_v1(actual, &transfer_candidate_account([seed; 32]), "NOV");
        anyhow::ensure!(
            balance == expected_balance,
            "independent {label} balance differs"
        );
        balances.insert(label.to_string(), balance);
    }
    let mut next_nonces = std::collections::BTreeMap::new();
    for (label, seed, expected_nonce) in [("A", 64, 6), ("C", 65, 1)] {
        let public_key = ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
            .verifying_key()
            .to_bytes();
        let identity = novovm_protocol::native_nonce::signer_nonce_identity_v2(&public_key);
        let identity_key = to_hex(&novovm_protocol::native_nonce::nonce_identity_digest_v1(
            chain, &identity,
        ));
        let nonce = actual
            .module_state
            .native_auth_next_nonces
            .get(&identity_key)
            .copied()
            .context("final nonce missing")?;
        anyhow::ensure!(nonce == expected_nonce, "independent {label} nonce differs");
        next_nonces.insert(label.to_string(), nonce);
    }
    let state = &actual.module_state;
    anyhow::ensure!(
        state.treasury_settled_nov_total == 316
            && state.treasury_settlements == 7
            && state.treasury_reserves.get("NOV") == Some(&316)
            && state.treasury_reserve_bucket_nov == 218
            && state.treasury_fee_bucket_nov == 63
            && state.treasury_risk_buffer_nov == 35,
        "independent treasury fee and bucket arithmetic differs"
    );
    anyhow::ensure!(
        balances.values().copied().sum::<u128>() + 316 == 2_000_000,
        "independent asset/fee conservation differs"
    );
    // Include every account, not only the five expected participants: an
    // accidental mint into another account must not pass a shared-code oracle.
    let mut all_nov = 0u128;
    for assets in state.account_asset_balances.values() {
        for (asset, balance) in assets {
            if asset == "NOV" {
                all_nov = all_nov
                    .checked_add(*balance)
                    .context("all-account NOV total overflow")?;
            } else {
                anyhow::ensure!(*balance == 0, "unexpected non-NOV asset balance");
            }
        }
    }
    anyhow::ensure!(
        all_nov.checked_add(316) == Some(2_000_000),
        "independent all-account asset/fee conservation differs"
    );
    // Re-read authority after the compute-only reference, before writing PASS.
    let after =
        workspace::load_finalized_genesis_parent_v1(chain, binding.workspace_id, pin, &params)?;
    anyhow::ensure!(
        after.output_digest() == parent.output_digest()
            && after.state() == actual
            && after.block() == parent.block(),
        "serial reference changed durable authority"
    );
    let report = TransferProcessParityReportV1 {
        accepted: true,
        scope: "same_host_four_process_mixed_transfer_serial_reference_v1",
        state_root: to_hex(&archived_tip.header.post_state_root),
        receipt_root: to_hex(&archived_tip.header.cumulative_receipt_root),
        receipts: &actual.receipts,
        economic: TransferProcessParityEconomicV1 {
            balances,
            next_nonces,
            fees: 316,
            reserve_bucket: 218,
            fee_bucket: 63,
            risk_buffer: 35,
        },
        block_count: input.tip_height,
        tx_count: seen.len(),
    };
    // Never overwrite a prior acceptance artifact, and never emit PASS before
    // all independent assertions and exact typed/byte comparisons succeeded.
    let output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_path)?;
    serde_json::to_writer(output, &report)?;
    Ok(())
}

#[test]
#[ignore = "requires stopped real four-node mixed-transfer fixture and explicit input/output paths"]
fn native_transfer_process_serial_parity_worker_v1() {
    transfer_candidate_on_runtime_stack(|| {
        transfer_process_serial_parity_run_v1().expect("real-process serial parity worker failed");
    });
}
