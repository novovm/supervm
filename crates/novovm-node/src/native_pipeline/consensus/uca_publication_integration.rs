//! Actual AOEM, ML-DSA spending and four original resident pipelines/journals.
//! In-process consensus message delivery, NOT native SDK/network activation.
//! Reopening after an unobserved ACK is not physical power-cut injection.
use super::*;
use crate::native_pipeline::business::direct_nov_fee::{quote_and_settle, TransferFeeRequest};
use crate::native_pipeline::business::nov_transfer_batch::{
    uca_effect_contract, uca_program_id, uca_receipt_codec,
};
use crate::native_pipeline::business::uca_spend_state::UcaSpendStateAccessV1;
use crate::native_pipeline::service::config::uca_genesis::{
    UcaGenesisAccountV1, UcaGenesisApprovalV1, UcaGenesisGrantV1, UcaGenesisManifestV1,
    INITIALIZATION_CONTEXT_V1,
};
use crate::native_pipeline::service::config::{
    self, GenesisAllocation, GenesisConfig, GenesisValidator,
};
use crate::native_pipeline::state::frontier::OwnedStateInput;
use crate::native_pipeline::state::tree::read_state_value;
use novovm_adapter_api::uca_spend_policy::SuppliedSpendGrantV1;
use novovm_exec::pq_signature::AoemMldsaParameterSet;
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions};
use novovm_protocol::uca_spend::{spend_key_ref_v1, UcaSpendIntentV1, MAX_SIGNED_SPEND_BYTES};

fn configuration(path: &Path, domain: Hash) -> Result<PipelineConfig> {
    let mut config = pipeline_config(path)?;
    config.store.domain.genesis_config_commitment = domain;
    config.store.domain.protocol_commitment = config::protocol_commitment();
    config.authentication.transaction_bytes = MAX_SIGNED_SPEND_BYTES;
    config.authentication.body_bytes = 64 * 1024;
    config.plan.transaction_bytes = MAX_SIGNED_SPEND_BYTES;
    config.plan.body_bytes = 64 * 1024;
    config.plan.access_keys = 512;
    config.capture.keys = 512;
    Ok(config)
}

fn intent(
    domain: Hash,
    key: &[u8],
    payer: &str,
    recipient: &str,
    amount: u128,
) -> Result<UcaSpendIntentV1> {
    Ok(UcaSpendIntentV1 {
        chain_id: CHAIN,
        genesis: domain,
        account_id: payer.into(),
        authorization_epoch: 1,
        spend_key_ref: spend_key_ref_v1(key)?,
        recipient_uca: recipient.into(),
        amount,
        max_fee: 50,
        nonce: 0,
        not_before_height: 1,
        expires_at_height: 30,
    })
}

fn execute(
    pipeline: &CandidatePipeline,
    context: BatchContext,
    raw: &[Vec<u8>],
    hints: &[(Account, Account)],
) -> Result<DurableCandidate> {
    let request = BatchRequest::from_uca(raw.to_vec(), hints.to_vec(), context, policy())?;
    let Submission::Accepted(mut ticket) = pipeline.try_submit(request)? else {
        bail!("empty UCA pipeline unexpectedly backpressured");
    };
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(Instant::now() < deadline, "UCA candidate deadline");
        if let Some(result) = ticket.try_take()? {
            return Ok(result.candidate().clone());
        }
        std::thread::yield_now();
    }
}

fn advance(journal: &mut ValidatorJournal, pipeline: &CandidatePipeline) -> Result<()> {
    journal.advance_height()?;
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(Instant::now() < deadline, "UCA journal advance deadline");
        if let Some(result) = journal.poll(pipeline)? {
            ensure!(result.is_none(), "advance emitted a vote");
            return Ok(());
        }
        std::thread::yield_now();
    }
}

#[test]
#[ignore = "real packaged AOEM; ML-DSA UCA pipeline/journal recovery, not native network or mainnet"]
fn actual_mldsa_uca_initialized_spend_respend_journal_recovery() -> Result<()> {
    let library = library()?;
    ensure!(
        aoem_bindings::default_host_dll_path().canonicalize()? == library,
        "fixture keygen and UCA pipelines must use the same AOEM library"
    );
    let facade = AoemExecFacade::open(&library, AoemExecOpenOptions::default())?;
    // Explicit same packaged runtime; no alternate verifier or fallback suite.
    let runtime = &facade;
    let management_signer = facade.mldsa_signer_v1(AoemMldsaParameterSet::MlDsa65)?;
    let spender = facade.uca_spend_signer_v1()?;
    let mut keys = Vec::new();
    for _ in 0..4 {
        keys.push(
            aoem_bindings::mldsa_keygen_v1_auto(65)?.context("ML-DSA-65 keygen unavailable")?,
        );
    }
    let ids = ["uca:publication-alice", "uca:publication-bob"];
    let addresses = [
        Account::try_from(vec![11; 20]).unwrap(),
        Account::try_from(vec![12; 20]).unwrap(),
    ];
    let mut manifest = UcaGenesisManifestV1 {
        genesis: GenesisConfig {
            chain_id: CHAIN,
            validator_epoch: 1,
            protocol_commitment: config::protocol_commitment(),
            genesis_config_commitment: [0; 32],
            timestamp_unix_ms: 172_800_500,
            validators: (0..4)
                .map(|i| GenesisValidator {
                    public_key: validator_key(i).verifying_key().to_bytes(),
                    weight: 1,
                })
                .collect(),
            allocations: vec![GenesisAllocation {
                account_hex: "0b".repeat(20),
                amount: "1000".into(),
            }],
            policy: policy(),
        },
        accounts: (0..2)
            .map(|i| {
                Ok(UcaGenesisAccountV1 {
                    uca_id: ids[i].into(),
                    asset_account: addresses[i].as_bytes().to_vec(),
                    management_public_key: keys[i].0.clone(),
                    grants: vec![UcaGenesisGrantV1 {
                        spend_public_key: keys[i + 2].0.clone(),
                        grant: SuppliedSpendGrantV1 {
                            account_id: ids[i].into(),
                            key_ref: spend_key_ref_v1(&keys[i + 2].0)?,
                            authorization_epoch: 1,
                            permits_nov_transfer: true,
                            revoked: false,
                            not_before_height: 1,
                            expires_at_height: 30,
                            max_debit_per_intent: 500,
                            remaining_debit_budget: 1000,
                        },
                    }],
                })
            })
            .collect::<Result<_>>()?,
    };
    let domain = manifest.signing_domain()?;
    let approvals = (0..2)
        .map(|i| {
            Ok(UcaGenesisApprovalV1 {
                uca_id: ids[i].into(),
                signature: management_signer.sign(
                    &keys[i].0,
                    &keys[i].1,
                    &manifest.canonical_bytes()?,
                    INITIALIZATION_CONTEXT_V1,
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        manifest.compile(runtime, domain, &approvals[..1]).is_err(),
        "missing owner approval admitted"
    );
    let initial = manifest.compile(runtime, domain, &approvals)?;
    // ML-DSA approvals may be randomized. Re-sign the EXACT manifest using
    // the same management keys: verified initial state must not include those
    // variable signature bytes. Preserve both approval bundles for comparison.
    let repeated_approvals = (0..2)
        .map(|i| {
            Ok(UcaGenesisApprovalV1 {
                uca_id: ids[i].into(),
                signature: management_signer.sign(
                    &keys[i].0,
                    &keys[i].1,
                    &manifest.canonical_bytes()?,
                    INITIALIZATION_CONTEXT_V1,
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let repeated_initial = manifest.compile(runtime, domain, &repeated_approvals)?;
    ensure!(
        repeated_initial.signing_domain() == initial.signing_domain()
            && repeated_initial.state().root() == initial.state().root()
            && repeated_initial.state().nodes() == initial.state().nodes()
            && repeated_initial.final_commitment() == initial.final_commitment(),
        "re-signing one pinned manifest changed its initial state"
    );
    println!(
        "independently regenerated management approvals differ={}, initial tree and final commitment identical",
        approvals.iter().zip(&repeated_approvals).any(|(a, b)| a.signature != b.signature)
    );
    ensure!(
        initial.signing_domain() == domain && initial.final_commitment() != domain,
        "domain and final state commitment conflated"
    );
    manifest.accounts[0].grants[0].grant.remaining_debit_budget += 1;
    ensure!(
        manifest.compile(runtime, domain, &approvals).is_err(),
        "changed grant admitted under old pin"
    );
    let changed_domain = manifest.signing_domain()?;
    ensure!(
        manifest
            .compile(runtime, changed_domain, &approvals)
            .is_err(),
        "old owner signatures approve changed grant"
    );
    manifest.accounts[0].grants[0].grant.remaining_debit_budget -= 1;

    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/consensus-tests")
        .join(format!(
            "uca-publication-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    let paths: Vec<_> = (0..4)
        .map(|i| directory.join(format!("validator-{i}")))
        .collect();
    fs::create_dir_all(&directory)?;
    // Public initialization evidence only; never persist expanded secret keys.
    fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    fs::write(
        directory.join("management-approvals.json"),
        serde_json::to_vec_pretty(&approvals)?,
    )?;
    let set = Arc::new(ValidatorSet::new(
        CHAIN,
        1,
        1,
        (0..4)
            .map(|i| Validator::new(validator_key(i).verifying_key().to_bytes(), 1))
            .collect::<Result<_>>()?,
    )?);
    let genesis = ConsensusContext {
        chain_id: CHAIN,
        genesis_config_commitment: domain,
        protocol_commitment: config::protocol_commitment(),
        epoch: 1,
        validator_set_hash: set.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    };
    let anchor = ParentPoint {
        height: 0,
        block_hash: [0; 32],
        state_root: initial.state().root(),
        receipt_batch_commitment: empty_root(),
        state_version: 0,
        decision_hash: [0; 32],
    };
    let mut services = Services::default();
    let mut journals = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        fs::create_dir_all(path)?;
        let cfg = configuration(path, domain)?;
        let store = CandidateStore::open(cfg.store.clone(), OpenMode::CreateNew)?;
        store.install_unpublished_state(initial.state())?;
        drop(store);
        services
            .0
            .push(CandidatePipeline::start(cfg, OpenMode::Existing)?);
        journals.push(Some(open_journal(
            ValidatorJournal::open(
                &services.0[i],
                genesis,
                anchor,
                set.clone(),
                validator_key(i),
            )?,
            &services.0[i],
        )?));
    }
    let mut expected_fee = 0u128;
    let mut expected_fee_state = FeeState::default();
    let mut expected_balances = [1000u128, 0u128];
    let mut budgets = [1000u128; 2];
    let mut final_head = anchor;
    let mut receipts = Vec::new();
    for height in 1..=2 {
        let payer = (height - 1) as usize;
        let recipient = 1 - payer;
        let amount = if height == 1 { 100 } else { 20 };
        let signed = spender.sign(
            intent(
                domain,
                &keys[payer + 2].0,
                ids[payer],
                ids[recipient],
                amount,
            )?,
            &keys[payer + 2].0,
            &keys[payer + 2].1,
        )?;
        let mut raw = vec![signed.encode()?];
        let mut hints = vec![(addresses[payer].clone(), addresses[recipient].clone())];
        if height == 1 {
            let mut replay = signed.intent.clone();
            replay.amount += 1; // Distinct signed body, same stable account nonce.
            raw.push(spender.sign(replay, &keys[2].0, &keys[2].1)?.encode()?);
            hints.push(hints[0].clone());
        }
        let current = journals[0].as_ref().unwrap().context();
        let parent = journals[0].as_ref().unwrap().parent();
        let mut execution = batch_context(parent.state_root);
        execution.genesis_config_commitment = domain;
        execution.protocol_commitment = current.protocol_commitment;
        execution.business_program = uca_program_id();
        execution.receipt_codec = uca_receipt_codec();
        execution.effect_contract = uca_effect_contract(&policy())?;
        execution.parent_height = parent.height;
        execution.parent_block_hash = parent.block_hash;
        execution.parent_receipt_root = parent.receipt_batch_commitment;
        execution.parent_state_version = parent.state_version;
        execution.height = height;
        execution.slot = height - 1;
        execution.timestamp_unix_ms += height;
        // Independent original quotation/settlement reference; do not infer
        // fees from the implementation's budget decrement or from balance sums.
        let expected_tx_hash: Hash = Sha256::digest(signed.intent.signing_bytes()?).into();
        let reference = quote_and_settle(
            &TransferFeeRequest {
                tx_hash: expected_tx_hash,
                payer: addresses[payer].clone(),
                recipient: addresses[recipient].clone(),
                asset: "NOV".into(),
                amount,
                pay_asset: "NOV".into(),
                max_pay_amount: 50,
                slippage_bps: 0,
            },
            &policy(),
            &expected_fee_state,
            expected_balances[payer],
            u128::from(execution.timestamp_unix_ms),
        )?;
        ensure!(
            reference.failure.is_none(),
            "reference fee rejected funded fixture"
        );
        let charged_fee = reference.charged_fee();
        expected_fee_state = reference.after_fee_state;
        expected_fee += charged_fee;
        expected_balances[payer] = expected_balances[payer]
            .checked_sub(amount + charged_fee)
            .context("reference spend has insufficient funds")?;
        expected_balances[recipient] += amount;
        budgets[payer] -= amount + charged_fee;
        let mut candidates = services
            .0
            .iter()
            .map(|p| execute(p, execution, &raw, &hints))
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            candidates
                .iter()
                .all(|c| c.packet().state_root() == candidates[0].packet().state_root()),
            "independent UCA execution diverged"
        );
        if height == 1 {
            ensure!(
                read_metadata(&services.0[0], vec![MetaKey::ChainHead])?.values[0].is_none(),
                "candidate persistence published a head"
            );
            // Reopen before any proposal/decision; same immutable candidate may
            // be recomputed, but must not duplicate economic effects or finalize.
            drop(journals[0].take());
            let service = services.0.remove(0);
            service.shutdown()?;
            services.0.insert(
                0,
                CandidatePipeline::start(configuration(&paths[0], domain)?, OpenMode::Existing)?,
            );
            journals[0] = Some(open_journal(
                ValidatorJournal::open(
                    &services.0[0],
                    genesis,
                    anchor,
                    set.clone(),
                    validator_key(0),
                )?,
                &services.0[0],
            )?);
            assert_eq!(journals[0].as_ref().unwrap().head(), None);
            candidates[0] = execute(&services.0[0], execution, &raw, &hints)?;
        }
        let value =
            BlockStatement::from_executed(candidates[0].packet(), current, &set, &parent)?.hash();
        let leader = (0..4)
            .find(|&i| {
                Validator::new(validator_key(i).verifying_key().to_bytes(), 1)
                    .unwrap()
                    .id()
                    == set.leader(height, 0).unwrap()
            })
            .unwrap();
        journals[leader]
            .as_mut()
            .unwrap()
            .propose(&candidates[leader], None)?;
        let DurableMessage::Proposal(proposal) =
            complete(journals[leader].as_mut().unwrap(), &services.0[leader])?
        else {
            bail!("missing proposal")
        };
        let proposal = proposal.verify(&set)?;
        let mut votes = Vec::new();
        for i in 0..4 {
            let journal = journals[i].as_mut().unwrap();
            journal.accept_proposal(&proposal, &candidates[i], None)?;
            votes.push(wire_vote(
                complete(journal, &services.0[i])?,
                &set,
                Phase::Prevote,
                value,
            )?);
        }
        let prevotes = Quorum::from_votes(&set, votes[..3].to_vec())?.verify(&set)?;
        votes.clear();
        for i in 0..4 {
            let journal = journals[i].as_mut().unwrap();
            journal.observe_prevotes(&prevotes, Some((&proposal, &candidates[i])))?;
            votes.push(wire_vote(
                complete(journal, &services.0[i])?,
                &set,
                Phase::Precommit,
                value,
            )?);
        }
        let decision = Quorum::from_votes(&set, votes[..3].to_vec())?.verify(&set)?;
        for i in 0..4 {
            let journal = journals[i].as_mut().unwrap();
            journal.observe_decision(&proposal, &candidates[i], &decision)?;
            ensure!(
                journal.decided().is_none(),
                "decision escaped before durable ACK"
            );
            if height == 1 && i == 0 {
                let deadline = Instant::now() + DEADLINE;
                while !journal.enqueue_pending(&services.0[i])? {
                    ensure!(Instant::now() < deadline, "decision enqueue deadline");
                    std::thread::yield_now();
                }
                ensure!(
                    journal.decided().is_none(),
                    "enqueue was treated as publication"
                );
                drop(journals[i].take());
                let service = services.0.remove(i);
                service.shutdown()?; // Drain storage, deliberately never consume ACK.
                services.0.insert(
                    i,
                    CandidatePipeline::start(
                        configuration(&paths[i], domain)?,
                        OpenMode::Existing,
                    )?,
                );
                journals[i] = Some(open_journal(
                    ValidatorJournal::open(
                        &services.0[i],
                        genesis,
                        anchor,
                        set.clone(),
                        validator_key(i),
                    )?,
                    &services.0[i],
                )?);
            } else {
                complete(journal, &services.0[i])?;
            }
            ensure!(
                journals[i].as_ref().unwrap().decided() == Some(value),
                "decision lost on recovery"
            );
        }
        final_head = journals[0]
            .as_ref()
            .unwrap()
            .head()
            .context("published head absent")?;
        ensure!(
            journals
                .iter()
                .flatten()
                .all(|j| j.head() == Some(final_head)),
            "published heads differ"
        );
        let mut expected = None;
        for pipeline in &services.0 {
            let mut read =
                super::super::ArchiveRead::new(height, final_head, current, set.clone())?;
            let deadline = Instant::now() + DEADLINE;
            let block = loop {
                ensure!(Instant::now() < deadline, "UCA archive deadline");
                if let Some(result) = read.poll(pipeline)? {
                    break result.context("published archive absent")?;
                }
                std::thread::yield_now();
            };
            ensure!(
                block.stored().raw_transactions() == raw,
                "archive changed signed NUAS"
            );
            let encoded = block.stored().receipt_bytes().to_vec();
            ensure!(
                expected.as_ref().is_none_or(|e| e == &encoded),
                "archive receipts differ"
            );
            expected = Some(encoded);
            let output = block
                .stored()
                .spend_outputs()
                .context("UCA budget evidence lost")?;
            let view = block.stored().receipt_view(0)?;
            ensure!(
                output[0].failure.is_none()
                    && output[0].remaining == budgets[payer]
                    && view.success
                    && view.nonce_after == 1
                    && view.charged_fee.parse::<u128>()? == charged_fee
                    && view.tx_hash == expected_tx_hash
                    && view.signer_identity == signed.intent.nonce_identity()?,
                "authorized spend receipt, fee, nonce or budget differs from reference"
            );
            if height == 1 {
                let replay_view = block.stored().receipt_view(1)?;
                ensure!(
                    output[1].failure
                        == Some(
                            novovm_adapter_api::uca_spend_policy::SpendPolicyError::NonceMismatch
                        )
                        && output[1].remaining == output[0].remaining
                        && !replay_view.success
                        && replay_view.nonce_after == 1
                        && replay_view.charged_fee == "0",
                    "replay consumed budget, fee or nonce"
                );
            }
        }
        receipts.push((raw, expected.unwrap(), current));
        if height < 2 {
            for (i, journal) in journals.iter_mut().enumerate() {
                advance(journal.as_mut().unwrap(), &services.0[i])?;
            }
        }
    }
    drop(journals);
    services.shutdown()?;
    // Query the same durable tree, not a parallel balance/permission database.
    for path in &paths {
        let store = CandidateStore::open(configuration(path, domain)?.store, OpenMode::Existing)?;
        let mut total = 0u128;
        for i in 0..2 {
            let access = UcaSpendStateAccessV1::new(
                intent(domain, &keys[i + 2].0, ids[i], ids[1 - i], 1)?,
                addresses[i].clone(),
                addresses[1 - i].clone(),
            )?;
            let captured = OwnedStateInput::capture(
                &store,
                final_head.state_root,
                access.declarations(),
                configuration(path, domain)?.capture,
            )?;
            let state = access.read(&captured, final_head.state_root)?;
            ensure!(
                state.next_nonce() == 1 && state.grant().remaining_debit_budget == budgets[i],
                "cold nonce/budget differs"
            );
            ensure!(
                state.balances() == (expected_balances[i], expected_balances[1 - i]),
                "cold exact payer/recipient balances differ"
            );
            total += state.balances().0;
        }
        ensure!(
            total + expected_fee == 1000,
            "funded balances/fees not conserved"
        );
        for change in fee_record_changes(&policy(), &expected_fee_state)? {
            let (key, expected) = match change {
                StateChange::Put { key, value } => (key, Some(value)),
                StateChange::Delete { key } => (key, None),
            };
            ensure!(
                read_state_value(&store, final_head.state_root, &key)? == expected,
                "cold fee buckets, window, diagnostics or policy differ from reference"
            );
        }
        drop(store);
    }
    for (i, path) in paths.iter().enumerate() {
        services.0.push(CandidatePipeline::start(
            configuration(path, domain)?,
            OpenMode::Existing,
        )?);
        let recovered = open_journal(
            ValidatorJournal::open(
                &services.0[i],
                genesis,
                anchor,
                set.clone(),
                validator_key(i),
            )?,
            &services.0[i],
        )?;
        ensure!(
            recovered.head() == Some(final_head),
            "cold journal head differs"
        );
        for (index, (raw, expected, context)) in receipts.iter().enumerate() {
            let mut read = super::super::ArchiveRead::new(
                index as u64 + 1,
                final_head,
                *context,
                set.clone(),
            )?;
            let deadline = Instant::now() + DEADLINE;
            let block = loop {
                ensure!(Instant::now() < deadline, "cold UCA archive deadline");
                if let Some(result) = read.poll(&services.0[i])? {
                    break result.context("cold UCA archive missing")?;
                }
                std::thread::yield_now();
            };
            ensure!(
                block.stored().raw_transactions() == raw.as_slice()
                    && block.stored().receipt_bytes() == expected.as_slice(),
                "cold UCA signed body or receipt bytes differ"
            );
        }
    }
    services.shutdown()?;
    for (_, secret) in &mut keys {
        secret.fill(0);
    }
    println!("UCA original pipeline/journal: 4 databases, 2 heights, recipient respend, candidate-only reopen, unobserved decision ACK recovery, cold balances/nonce/budget/receipts checked; native SDK and process kill NOT exercised");
    Ok(())
}
