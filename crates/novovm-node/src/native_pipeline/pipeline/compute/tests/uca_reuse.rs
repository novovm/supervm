//! Real signature facts are reusable; parent-state authorization is not.
use super::*;
use crate::native_pipeline::business::nov_transfer_batch::{
    nonce_key, uca_effect_contract, uca_program_id, uca_receipt_codec,
};
use crate::native_pipeline::business::uca_spend_state::{
    account_record_changes, grant_record_changes, AssetAccountAuthorityV2,
};
use crate::native_pipeline::pipeline::{admitted_uca_for_test, tests as pipeline_tests};
use novovm_adapter_api::uca_spend_policy::{SpendPolicyError, SuppliedAssetSpendGrantV2};
use novovm_protocol::uca_spend::{spend_key_ref_v1, AssetSpendIntentV2, MAX_SIGNED_SPEND_BYTES};
use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV2, AssetSpendBatchV2};

fn account_record(address: Vec<u8>) -> AssetAccountAuthorityV2 {
    AssetAccountAuthorityV2 {
        account: address,
        asset: 1,
        active: true,
        management_key_ref: [8; 32],
        recovery_key_ref: None,
        current_spend_epoch: 1,
    }
}

#[test]
#[ignore = "actual packaged AOEM; signature reuse with new-root authority, not genesis or TPS acceptance"]
fn actual_mldsa_reused_signature_recaptures_revocation_budget_nonce_and_balance() -> Result<()> {
    let library: PathBuf = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .context("explicit trusted AOEM library required")?
        .into();
    let facade = novovm_exec::AoemExecFacade::open(&library, Default::default())?;
    let keypair =
        facade.mldsa_keygen_v1(novovm_exec::pq_signature::AoemMldsaParameterSet::MlDsa65)?;
    let intent = AssetSpendIntentV2 {
        asset: 1,
        authority_update: None,
        recipient_creation: None,
        chain_id: domain().chain_id,
        genesis: domain().genesis_config_commitment,
        payer: vec![1; 20],
        authorization_epoch: 1,
        spend_key_ref: spend_key_ref_v1(&keypair.public_key)?,
        recipient: vec![2; 20],
        amount: 30,
        max_fee: 50,
        nonce: 0,
        not_before_height: 1,
        expires_at_height: 50,
    };
    let signed = facade.asset_spend_signer_v2()?.sign(
        intent.clone(),
        &keypair.public_key,
        &keypair.expanded_secret_key,
    )?;
    let raw = vec![signed.encode()?];
    let limits = AssetSpendBatchLimitsV2 {
        transactions: 16,
        transaction_bytes: MAX_SIGNED_SPEND_BYTES,
        body_bytes: 65536,
    };
    let source = Arc::new(AssetSpendBatchV2::from_raw(&raw, limits)?);
    let mut pipeline_config = pipeline_tests::config("unused-uca-parent-reuse".into());
    pipeline_config.authentication.transaction_bytes = MAX_SIGNED_SPEND_BYTES;
    pipeline_config.plan.transaction_bytes = MAX_SIGNED_SPEND_BYTES;
    let config = ComputeConfig {
        library: library.clone(),
        workers: 2,
        execution_backend: Default::default(),
        signature_backend: Default::default(),
        prepare_asset_signatures: false,
        queue_capacity: 2,
        authentication: pipeline_config.authentication,
        plan: pipeline_config.plan,
        packet: PacketBudget::default(),
        capture: pipeline_config.capture,
        timeout: Duration::from_secs(30),
        domain: domain(),
        signatures: Arc::new(crate::native_pipeline::pipeline::SignatureWork::default()),
    };
    let (pipeline, _receiver) = pipeline_tests::inert_pipeline(pipeline_config);
    let token = admitted_uca_for_test(pipeline.identity.clone(), &facade, source.clone(), 0)?;
    let payer = Account::from([1; 20]);
    let recipient = Account::from([2; 20]);
    let grant = SuppliedAssetSpendGrantV2 {
        asset: 1,
        account: intent.payer.clone(),
        key_ref: intent.spend_key_ref,
        authorization_epoch: 1,
        permits_nov_transfer: true,
        revoked: false,
        not_before_height: 1,
        expires_at_height: 50,
        max_debit_per_intent: 100,
        remaining_debit_budget: 100,
    };
    let payer_record = account_record(vec![1; 20]);
    let mut changes = fee_record_changes(&policy(), &FeeState::default())?;
    changes.extend(account_record_changes(
        intent.chain_id,
        &intent.genesis,
        &payer_record,
    )?);
    changes.extend(account_record_changes(
        intent.chain_id,
        &intent.genesis,
        &account_record(vec![2; 20]),
    )?);
    changes.extend(grant_record_changes(&intent, &grant)?);
    for (account, balance) in [(&payer, 1000u128), (&recipient, 7u128)] {
        changes.push(StateChange::Put {
            key: balance_key(account),
            value: balance.to_le_bytes().to_vec(),
        });
    }
    changes.push(StateChange::Put {
        key: nonce_key(&intent.nonce_identity()?),
        value: 0u64.to_le_bytes().to_vec(),
    });
    let original = stage_state_update(&Memory::default(), empty_root(), &changes)?;
    let memory = Memory(original.nodes().clone());
    let mut session = ComputeSession::open(&library, 2)?;
    let mut verifier = None;
    for field in ["payer", "recipient", "domain", "signature"] {
        let mut wrong = signed.clone();
        match field {
            "payer" => wrong.intent.payer[0] ^= 1,
            "recipient" => wrong.intent.recipient[0] ^= 1,
            "domain" => wrong.intent.chain_id += 1,
            _ => wrong.signature[0] ^= 1,
        }
        let wrong = Arc::new(AssetSpendBatchV2::from_raw(&[wrong.encode()?], limits)?);
        let checked = token.checked_uca().unwrap().reattach(signed.clone())?;
        assert!(
            NovTransferBody::prepare_asset_spend_from_uca(
                wrong,
                vec![checked],
                policy(),
                config.plan,
            )
            .is_err(),
            "changed {field} attached to a real signature fact"
        );
    }
    for case in [
        "unchanged",
        "revoked",
        "budget",
        "nonce",
        "epoch",
        "balance",
    ] {
        let mut next_grant = grant.clone();
        let mutation = match case {
            "revoked" => {
                next_grant.revoked = true;
                grant_record_changes(&intent, &next_grant)?
            }
            "budget" => {
                next_grant.remaining_debit_budget = 0;
                grant_record_changes(&intent, &next_grant)?
            }
            "nonce" => vec![StateChange::Put {
                key: nonce_key(&intent.nonce_identity()?),
                value: 1u64.to_le_bytes().to_vec(),
            }],
            "epoch" => {
                let mut record = payer_record.clone();
                record.current_spend_epoch = 2;
                account_record_changes(intent.chain_id, &intent.genesis, &record)?
            }
            "balance" => vec![StateChange::Put {
                key: balance_key(&payer),
                value: 0u128.to_le_bytes().to_vec(),
            }],
            _ => vec![],
        };
        let changed = stage_state_update(&memory, original.root(), &mutation)?;
        let mut parent = Memory(memory.0.clone());
        parent.0.extend(changed.nodes().clone());
        let selected = Arc::new(AssetSpendBatchV2::from_raw(&raw, limits)?);
        let admitted = pipeline.rebind_admitted_uca(selected.clone(), &[&token])?;
        let body = authenticate(
            &mut session,
            &mut verifier,
            &config,
            AuthenticateRequest {
                raw_transactions: CandidateInput::Admitted(admitted),
                policy: policy(),
            },
        )?;
        assert!(
            verifier.is_none(),
            "reused immutable fact must not reopen/requalify crypto"
        );
        let mut context = context(changed.root());
        context.business_program = uca_program_id();
        context.effect_contract = uca_effect_contract(&policy())?;
        context.receipt_codec = uca_receipt_codec();
        let plan = body.bind(context)?;
        assert!(plan.plan().raw_transactions_if_materialized().is_none());
        assert_eq!(
            Arc::strong_count(&selected),
            2,
            "product rebuilt its APFL owner"
        );
        let reference = if case == "unchanged" {
            let checked = token.checked_uca().unwrap().reattach(signed.clone())?;
            let raw = NovTransferBody::prepare_asset_spend(vec![checked], policy(), config.plan)?
                .bind(context)?;
            assert!(raw.plan().raw_transactions_if_materialized().is_some());
            assert_eq!(raw.commitment(), plan.commitment());
            Some(raw)
        } else {
            None
        };
        let result = capture(plan, &parent)?
            .finalize_capture()?
            .execute(&mut session, Duration::from_secs(30))?;
        if let Some(reference) = reference {
            let expected = capture(reference, &parent)?
                .finalize_capture()?
                .execute(&mut session, Duration::from_secs(30))?;
            assert_eq!(
                result.effects().plan_commitment(),
                expected.effects().plan_commitment()
            );
            assert_eq!(
                result.effects().update().root(),
                expected.effects().update().root()
            );
            assert_eq!(
                result.effects().update().nodes(),
                expected.effects().update().nodes()
            );
            assert_eq!(result.receipts(), expected.receipts());
            assert_eq!(result.spend_outputs(), expected.spend_outputs());
            assert_eq!(result.fees(), expected.fees());
            assert_eq!(
                result.receipt_batch_commitment(),
                expected.receipt_batch_commitment()
            );
            assert_eq!(
                result.statement_commitment(),
                expected.statement_commitment()
            );
        }
        let receipt = &result.receipts()[0];
        let output = &result.spend_outputs().context("missing UCA outputs")?[0];
        if case == "unchanged" {
            assert!(receipt.failure.is_none());
            assert_eq!(receipt.delta.nonce_after, 1);
        } else if case == "balance" {
            assert!(
                receipt.failure.is_some(),
                "new balance cannot be bypassed by old signature fact"
            );
            assert_eq!(receipt.delta.fee_funding_delta, 0);
        } else {
            let error = match case {
                "revoked" => SpendPolicyError::Revoked,
                "budget" => SpendPolicyError::LimitExceeded,
                "nonce" => SpendPolicyError::NonceMismatch,
                _ => SpendPolicyError::WrongEpoch,
            };
            assert_eq!(output.failure, Some(error), "new parent {case}");
            assert_eq!(
                receipt.delta.nonce_after,
                if case == "nonce" { 1 } else { 0 }
            );
            assert_eq!(receipt.delta.fee_funding_delta, 0);
            assert!(receipt.quote.is_none() && receipt.journal.is_none());
            assert_eq!(result.fees(), &FeeState::default());
        }
    }
    assert_eq!(config.signatures.strict_verified.load(Ordering::Relaxed), 0);
    assert_eq!(config.signatures.reused.load(Ordering::Relaxed), 6);
    assert_eq!(config.signatures.uca_batch_calls.load(Ordering::Relaxed), 0);
    // Missing evidence still uses the original qualified batch verifier.
    let body = authenticate(
        &mut session,
        &mut verifier,
        &config,
        AuthenticateRequest {
            raw_transactions: CandidateInput::Uca {
                source: BatchSource::Uca(source.clone()),
            },
            policy: policy(),
        },
    )?;
    let mut fresh_context = context(original.root());
    fresh_context.business_program = uca_program_id();
    fresh_context.effect_contract = uca_effect_contract(&policy())?;
    fresh_context.receipt_codec = uca_receipt_codec();
    let fresh_plan = body.bind(fresh_context)?;
    assert!(fresh_plan
        .plan()
        .raw_transactions_if_materialized()
        .is_none());
    assert_eq!(
        Arc::strong_count(&source),
        3,
        "fresh verifier lost original APFL source"
    );
    assert!(verifier.is_some());
    assert_eq!(config.signatures.strict_verified.load(Ordering::Relaxed), 1);
    assert_eq!(config.signatures.uca_batch_calls.load(Ordering::Relaxed), 1);
    Ok(())
}
