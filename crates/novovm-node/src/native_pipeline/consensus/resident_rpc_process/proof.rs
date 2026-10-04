//! Real optional proof of a published product block, never a synthetic receipt.
//! Explicit Linux/WSL assets only. Four original nodes keep the same execution,
//! quorum and storage path; only node 0 owns the separate blocking prover.
use super::*;

const PROOF_DEADLINE: Duration = Duration::from_secs(600);

fn explicit_file(variable: &str) -> Result<PathBuf> {
    let path = PathBuf::from(
        std::env::var_os(variable).with_context(|| format!("explicit {variable} required"))?,
    )
    .canonicalize()
    .with_context(|| format!("resolve {variable}"))?;
    ensure!(path.is_file(), "{variable} is not a file");
    Ok(path)
}

fn rejected(nodes: &ProductNodes, node: usize, method: &str, params: Value) -> Result<Value> {
    let reply = nodes.request(
        node,
        json!({"jsonrpc":"2.0","id":29,"method":method,"params":params}),
    )?;
    ensure!(
        reply.get("error").is_some() && reply.get("result").is_none(),
        "invalid proof request was accepted on node {node}: {reply}"
    );
    Ok(reply)
}

fn poll_proof(
    nodes: &mut ProductNodes,
    height: u64,
    deadline: Instant,
    wait_for_worker: bool,
) -> Result<Value> {
    loop {
        nodes.alive()?;
        let result = nodes.rpc(0, "nov_getBlockProof", json!([height]))?;
        let state = result["state"].as_str().context("proof state missing")?;
        ensure!(state != "failed", "actual proof failed: {result}");
        ensure!(
            Instant::now() < deadline,
            "actual proof deadline exceeded: {result}"
        );
        if state == "verified_durable" || (wait_for_worker && state == "proof_owner_pending") {
            return Ok(result);
        }
        ensure!(
            matches!(state, "reading_verified_archive" | "proof_owner_pending"),
            "unexpected proof state: {result}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn checked_proof(
    proof: &Value,
    image: &[u32; 8],
    first_receipt: &Value,
    recovered: bool,
) -> Result<()> {
    ensure!(
        proof["state"] == "verified_durable"
            && proof["verified"] == true
            && proof["persisted"] == true
            && proof["recovered"] == recovered
            && proof["image_id"] == json!(image)
            && proof["height"] == first_receipt["block_height"]
            && proof["transactions"] == 1
            && proof["changes_finality"] == false
            && proof["business_gpu_certified"] == false,
        "proof ownership/pins/completion differ: {proof}"
    );
    let block: Hash = serde_json::from_value(proof["block_hash"].clone())?;
    ensure!(
        Some(hex(&block).as_str()) == first_receipt["block_hash"].as_str(),
        "proof belongs to another finalized block"
    );
    for field in ["candidate_id", "document_digest"] {
        let hash: Hash = serde_json::from_value(proof[field].clone())?;
        ensure!(hash != [0; 32], "missing proof {field}");
    }
    let bytes = proof["receipt_bytes"]
        .as_u64()
        .context("receipt size missing")?;
    ensure!(
        (9..=16 * 1024 * 1024).contains(&bytes),
        "invalid real receipt size"
    );
    for field in ["receipt_sha256", "journal_sha256"] {
        let digest = proof[field].as_str().context("proof digest missing")?;
        ensure!(
            digest.len() == 64
                && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
                && digest.bytes().any(|byte| byte != b'0'),
            "invalid proof {field}"
        );
    }
    Ok(())
}

/// Both transactions come from payer 1. The parent's alternating-payer helper
/// must not charge the second transaction to fixture payer 3.
fn payer_one_balances(nodes: &mut ProductNodes, receipts: &[Value]) -> Result<Vec<Vec<Value>>> {
    let fees = receipts.iter().try_fold(0_u128, |total, receipt| {
        total
            .checked_add(
                receipt["charged_fee"]
                    .as_str()
                    .context("fee missing")?
                    .parse()?,
            )
            .context("proof fixture fee overflow")
    })?;
    let payer_after = 1_000_000_u128
        .checked_sub(17)
        .and_then(|balance| balance.checked_sub(fees))
        .context("proof fixture charged more than funded balance")?;
    let mut all = Vec::new();
    for node in 0..4 {
        let mut row = Vec::new();
        for (seed, expected) in [(1, payer_after), (2, 17), (3, 1_000_000)] {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                nodes.alive()?;
                let value = nodes.rpc(
                    node,
                    "nov_getAssetBalance",
                    json!({"account":hex(account(seed).as_bytes()),"asset":"NOV"}),
                )?;
                if value["query_complete"] == true {
                    ensure!(
                        value["finalized"] == true
                            && value["balance"].as_str() == Some(expected.to_string().as_str()),
                        "proof fixture amount/fee differs: {value}"
                    );
                    row.push(value);
                    break;
                }
                ensure!(
                    Instant::now() < deadline,
                    "proof fixture balance query timeout"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        all.push(row);
    }
    ensure!(
        all.iter().all(|row| row == &all[0]),
        "four-node balances differ"
    );
    Ok(all)
}

fn independently_verify_stored_proof(
    nodes: &ProductNodes,
    proof_library: &Path,
    image: &[u32; 8],
    rpc_proof: &Value,
    receipts: &[Value],
) -> Result<Value> {
    use crate::native_pipeline::consensus::chain::ChainRecord;
    use crate::native_pipeline::proof::{ExecutionJournalV1, JOURNAL_BYTES};
    use novovm_exec::resident::{ReceiptBackendUnavailable, ReceiptLimits, ReceiptSession};

    ensure!(
        nodes.processes.iter().all(Option::is_none),
        "independent proof read requires all original database owners stopped"
    );
    ensure!(
        receipts.len() == 2,
        "independent verifier needs both actual blocks"
    );
    let genesis = genesis()?;
    let store = CandidateStore::open(
        StoreConfig {
            library: library()?,
            database: nodes.directory.join("validator-0.rocksdb"),
            domain: StorageDomain {
                chain_id: genesis.chain_id,
                genesis_config_commitment: genesis.genesis_config_commitment,
                protocol_commitment: genesis.protocol_commitment,
            },
            storage: StorageConfig::default(),
            packet_budget: PacketBudget::default(),
        },
        OpenMode::Existing,
    )?;
    let read_block = |height: u64| -> Result<_> {
        let metadata = store.read_metadata(&[MetaKey::ChainBlock { height }])?;
        let bytes = metadata
            .values
            .into_iter()
            .next()
            .flatten()
            .context("independent verifier missing real chain block")?;
        let record = ChainRecord::decode(&bytes)?;
        let stored = store
            .recover(record.candidate_id())?
            .context("independent verifier missing actual stored candidate")?;
        ensure!(
            record.point().height == height && stored.context().height == height,
            "independent verifier archive height differs"
        );
        ensure!(
            stored.state_root() == record.point().state_root
                && stored.receipt_batch_commitment() == record.point().receipt_batch_commitment,
            "independent verifier archive/candidate roots differ"
        );
        Ok((record, stored))
    };
    let (first_record, first) = read_block(1)?;
    let (second_record, second) = read_block(2)?;
    ensure!(
        second_record.parent() == first_record.point(),
        "independent verifier parent chain differs"
    );
    for (record, receipt) in [
        (&first_record, &receipts[0]),
        (&second_record, &receipts[1]),
    ] {
        ensure!(
            Some(hex(&record.point().block_hash).as_str()) == receipt["block_hash"].as_str(),
            "independent verifier opened another RPC block"
        );
    }
    ensure!(
        rpc_proof["candidate_id"] == json!(first.candidate_id())
            && rpc_proof["document_digest"] == json!(first.document_digest()),
        "RPC proof is not attached to this real stored candidate"
    );
    let expected = ExecutionJournalV1::from_stored(&first)?.encode();
    let successor_journal = ExecutionJournalV1::from_stored(&second)?.encode();
    ensure!(
        expected != successor_journal,
        "wrong-block negative is not distinct"
    );
    let blob = store
        .read_proof(first.candidate_id(), *image)?
        .context("completed proof absent from original candidate database")?;
    // Test-local decoder only: no new production decoding API. Check every
    // independently derived binding BEFORE taking the native receipt slice.
    let header = 8 + 32 + 32 + 32 + JOURNAL_BYTES;
    ensure!(
        header == 256 && (header + 9..=header + 16 * 1024 * 1024).contains(&blob.len()),
        "stored NVPROOF1 record length invalid"
    );
    let image_bytes: Vec<u8> = image.iter().flat_map(|word| word.to_be_bytes()).collect();
    ensure!(
        &blob[..8] == b"NVPROOF1"
            && blob[8..40] == first.candidate_id()
            && blob[40..72] == first.document_digest()
            && blob[72..104] == image_bytes
            && blob[104..header] == expected,
        "stored NVPROOF1 candidate/document/image/journal binding differs"
    );
    let receipt = &blob[header..];
    ensure!(
        receipt.starts_with(b"AORCP002")
            && rpc_proof["receipt_bytes"] == receipt.len()
            && rpc_proof["receipt_sha256"] == format!("{:x}", Sha256::digest(receipt))
            && rpc_proof["journal_sha256"] == format!("{:x}", Sha256::digest(expected)),
        "stored real receipt differs from RPC evidence"
    );

    // This session belongs to the independent TEST process, not the stopped
    // producer. A successful real positive is required before all negatives;
    // missing backend or wrong asset cannot satisfy a rejection assertion.
    let mut verifier = ReceiptSession::open(proof_library, ReceiptLimits::default())?;
    let positive_started = Instant::now();
    verifier.verify(receipt, image, &expected)?;
    let positive_ms = positive_started.elapsed().as_millis();
    let mut wrong_image = *image;
    wrong_image[0] ^= 1;
    let mut tampered = receipt.to_vec();
    tampered[receipt.len() / 2] ^= 1;
    let negatives = [
        (
            "wrong_trusted_image",
            receipt,
            &wrong_image,
            expected.as_slice(),
        ),
        (
            "successor_block_journal",
            receipt,
            image,
            successor_journal.as_slice(),
        ),
        (
            "modified_receipt",
            tampered.as_slice(),
            image,
            expected.as_slice(),
        ),
        (
            "truncated_receipt",
            &receipt[..receipt.len() - 1],
            image,
            expected.as_slice(),
        ),
    ];
    let mut rejected = Vec::new();
    for (name, bytes, pin, journal) in negatives {
        let started = Instant::now();
        let error = verifier
            .verify(bytes, pin, journal)
            .err()
            .with_context(|| format!("real verifier accepted {name}"))?;
        ensure!(
            error.downcast_ref::<ReceiptBackendUnavailable>().is_none(),
            "missing native backend is not cryptographic rejection: {name}"
        );
        rejected.push(
            json!({"case":name,"rejected":true,"verify_ms":started.elapsed().as_millis(),
            "error":format!("{error:#}")}),
        );
    }
    let final_started = Instant::now();
    verifier.verify(receipt, image, &expected)?;
    Ok(
        json!({"verifier_process_id":std::process::id(),"native_positive_verified":true,
        "native_positive_ms":positive_ms,"negatives":rejected,"negative_count":4,
        "positive_recheck_verified":true,"positive_recheck_ms":final_started.elapsed().as_millis(),
        "source":"node0 original AOEM database; independent trusted image and stored candidate journal",
        "receipt_sha256":format!("{:x}",Sha256::digest(receipt)),
        "expected_journal_sha256":format!("{:x}",Sha256::digest(expected)),
        "wrong_block_journal_sha256":format!("{:x}",Sha256::digest(successor_journal))}),
    )
}

#[test]
#[ignore = "requires explicit real Linux/WSL node, AOEM AORCP002 backend and trusted NOV guest/image; may prove for 600 seconds"]
fn real_resident_rpc_candidate_proof_and_recovery() -> Result<()> {
    let binary = explicit_file("NOVOVM_TEST_RESIDENT_NODE")?;
    let proof_library = explicit_file("NOVOVM_TEST_PROOF_LIBRARY")?;
    let guest = explicit_file("NOVOVM_TEST_PROOF_GUEST")?;
    let image: [u32; 8] = serde_json::from_str(
        &std::env::var("NOVOVM_TEST_PROOF_IMAGE_JSON")
            .context("explicit NOVOVM_TEST_PROOF_IMAGE_JSON required")?,
    )?;
    ensure!(
        image != [0; 8],
        "trusted proof image cannot be empty/SKIP output"
    );
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/resident-rpc-proof-tests")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    eprintln!("actual product RPC proof artifacts={}", directory.display());
    let mut evidence = json!({"product_binary":binary,
        "product_binary_sha256":hex(&Sha256::digest(fs::read(&binary)?)),
        "execution_library_sha256":hex(&Sha256::digest(fs::read(library()?)?)),
        "proof_library":proof_library,"proof_library_sha256":hex(&Sha256::digest(fs::read(&proof_library)?)),
        "guest":guest,"guest_sha256":hex(&Sha256::digest(fs::read(&guest)?)),"trusted_image":image});
    let mut phase = "start four original nodes";
    let result = (|| -> Result<()> {
        let mut relay = Relay::start(&directory.join("relay"))?;
        let (mut nodes, _) = setup(&directory, &relay, binary)?;
        let mut config: Value = serde_json::from_slice(&fs::read(&nodes.configs[0])?)?;
        config["proof"] = json!({"library":proof_library,"guest":guest,"image_id":image});
        fs::write(&nodes.configs[0], serde_json::to_vec_pretty(&config)?)?;
        for node in 0..4 {
            nodes.start(node, "create")?;
        }
        let initial = nodes.wait_ready(&[0, 1, 2, 3])?;
        ensure!(
            initial.iter().all(|status| status["head"].is_null()),
            "not fresh genesis"
        );
        evidence["initial_status"] = json!(initial);

        phase = "single-ingress real signed transfer";
        let first = vec![signed(1, 0, 10)?];
        let mut hashes = transaction_hashes(&first)?;
        let admitted = nodes.submit_batch(0, &first)?;
        ensure!(
            admitted[0]["signature_verified"] == true,
            "input not authenticated"
        );
        let before = nodes.wait_receipts(&[0, 1, 2, 3], &hashes)?;
        ensure!(
            before[0][0]["success"] == true && before[0][0]["nonce_after"] == 1,
            "first transaction did not succeed"
        );
        let height = before[0][0]["block_height"]
            .as_u64()
            .context("source height missing")?;
        ensure!(height == 1, "fresh proof source was not the first block");
        evidence["source_receipts"] = json!(before);
        evidence["source_balances"] = json!(nodes.balances(&[0, 1, 2, 3], &before[0], [10, 0])?);
        evidence["invalid_requests"] = json!([
            rejected(&nodes, 0, "nov_proveBlock", json!([0]))?,
            rejected(&nodes, 0, "nov_proveBlock", json!([height + 100]))?,
            rejected(&nodes, 0, "nov_getBlockProof", json!(["1"]))?,
            rejected(&nodes, 1, "nov_proveBlock", json!([height]))?,
            rejected(&nodes, 1, "nov_getBlockProof", json!([height]))?
        ]);

        phase = "async proof while successor confirms";
        let started = Instant::now();
        let deadline = started + PROOF_DEADLINE;
        let requested = nodes.rpc(0, "nov_proveBlock", json!([height]))?;
        ensure!(
            requested["verified"] == false && requested["persisted"] == false,
            "fresh proof request fabricated immediate completion"
        );
        evidence["proof_admission_ms"] = json!(started.elapsed().as_millis());
        evidence["proof_admission"] = requested;
        let worker = poll_proof(&mut nodes, height, deadline, true)?;
        ensure!(
            worker["state"] == "proof_owner_pending",
            "proof finished before overlap observation"
        );
        let second = vec![signed(1, 1, 7)?];
        let successor_started = Instant::now();
        nodes.submit_batch(0, &second)?;
        hashes.extend(transaction_hashes(&second)?);
        let both = nodes.wait_receipts(&[0, 1, 2, 3], &hashes)?;
        ensure!(
            both[0][0] == before[0][0]
                && both[0][1]["success"] == true
                && both[0][1]["nonce_after"] == 2
                && both[0][1]["block_height"] == height + 1,
            "successor did not finalize while proof owner worked"
        );
        evidence["successor_finalized_ms"] = json!(successor_started.elapsed().as_millis());
        let overlap = nodes.rpc(0, "nov_getBlockProof", json!([height]))?;
        ensure!(
            overlap["state"] == "proof_owner_pending" && overlap["verified"] == false,
            "no observed proof/finality overlap; cannot claim control isolation: {overlap}"
        );
        evidence["pending_proof_after_successor_finality"] = overlap;
        evidence["busy_other_height"] = rejected(&nodes, 0, "nov_proveBlock", json!([height + 1]))?;
        let statuses = nodes.wait_ready(&[0, 1, 2, 3])?;
        let partition = receipt_partition(&both[0], &hashes, 2)?;
        verify_partition_heads(&statuses, &partition, 2)?;
        let balances = payer_one_balances(&mut nodes, &both[0])?;
        evidence["successor_status"] = json!(statuses);
        evidence["receipts"] = json!(both);
        evidence["balances"] = json!(balances);

        phase = "real proof verified and durable";
        let proved = poll_proof(&mut nodes, height, deadline, false)?;
        checked_proof(&proved, &image, &before[0][0], false)?;
        evidence["proof_wall_ms"] = json!(started.elapsed().as_millis());
        evidence["proof"] = proved.clone();
        ensure!(
            nodes.rpc(0, "nov_proveBlock", json!([height]))? == proved,
            "same completed request generated another result"
        );
        let live_pids: Vec<_> = nodes.processes.iter().flatten().map(Child::id).collect();

        phase = "four original databases cold restart and proof reverification";
        nodes.stop_all()?;
        for node in 0..4 {
            nodes.start(node, "existing")?;
        }
        let restarted = nodes.wait_ready(&[0, 1, 2, 3])?;
        let cold = nodes.wait_receipts(&[0, 1, 2, 3], &hashes)?;
        ensure!(cold == both, "cold user receipts differ");
        ensure!(
            payer_one_balances(&mut nodes, &cold[0])? == balances,
            "cold balances differ"
        );
        verify_partition_heads(&restarted, &partition, 2)?;
        let cold_pids: Vec<_> = nodes.processes.iter().flatten().map(Child::id).collect();
        ensure!(
            cold_pids.len() == 4 && cold_pids.iter().all(|pid| !live_pids.contains(pid)),
            "four original processes did not restart"
        );
        let recovered = poll_proof(
            &mut nodes,
            height,
            Instant::now() + Duration::from_secs(90),
            false,
        )?;
        checked_proof(&recovered, &image, &before[0][0], true)?;
        for field in [
            "height",
            "block_hash",
            "candidate_id",
            "document_digest",
            "image_id",
            "receipt_sha256",
            "journal_sha256",
            "receipt_bytes",
        ] {
            ensure!(
                recovered[field] == proved[field],
                "cold proof {field} changed"
            );
        }
        ensure!(
            recovered["prove_and_verify_ms"] == 0,
            "recovery regenerated proof instead of verifying stored receipt"
        );
        evidence["recovered_proof"] = recovered;
        evidence["live_pids"] = json!(live_pids);
        evidence["restarted_pids"] = json!(cold_pids);
        evidence["restarted_status"] = json!(restarted);
        nodes.stop_all()?;
        phase = "independent real backend positives and cryptographic rejection";
        evidence["independent_verification"] =
            independently_verify_stored_proof(&nodes, &proof_library, &image, &proved, &cold[0])?;
        relay.shutdown()?;
        Ok(())
    })();
    let report = json!({"schema":"novovm/resident-product-rpc-proof/v1","passed":result.is_ok(),
        "failure":result.as_ref().err().map(|error|format!("{error:#}")),"phase":phase,
        "evidence":evidence,"proof_deadline_seconds":600,
        "topology":"one host; four real original novovm-node processes; only node0 proof owner",
        "signed_single_ingress_transactions":2,"proof_scope":"one published direct-NOV candidate; optional attachment",
        "performance_measured":false,"four_machine_test":false,"gpu_proof_claimed":false,
        "production_acceptance":false});
    fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    result.with_context(|| format!("actual RPC proof artifacts={}", directory.display()))
}
