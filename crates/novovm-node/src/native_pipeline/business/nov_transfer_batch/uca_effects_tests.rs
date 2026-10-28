//! Explicit seeded-tree integration, not a genesis authorization, active ingress,
//! durable publication, mobile custody, or mainchain performance qualification.
use super::*;
use crate::native_pipeline::business::uca_spend_state::{
    account_record_changes, grant_record_changes, AssetAccountAuthorityV2, AssetSpendStateAccessV2,
};
use crate::native_pipeline::state::frontier::OwnedStateInput;
use crate::native_pipeline::state::tree::{empty_root, stage_state_update};
use novovm_adapter_api::uca_spend_policy::{SpendPolicyError, SuppliedAssetSpendGrantV2};
use novovm_exec::resident::ComputeSession;
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions};
use novovm_protocol::uca_spend::{spend_key_ref_v1, AssetSpendIntentV2};
use std::time::Duration;

#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);
impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

fn capture_budget() -> CaptureBudget {
    CaptureBudget {
        keys: 4096,
        nodes: 8192,
        bytes: 4 * 1024 * 1024,
    }
}

fn account(_id: &str, address: Vec<u8>) -> AssetAccountAuthorityV2 {
    AssetAccountAuthorityV2 {
        account: address,
        asset: 1,
        active: true,
        current_spend_epoch: 3,
        management_key_ref: [8; 32],
        recovery_key_ref: Some([9; 32]),
    }
}

fn fixture(
    i: &AssetSpendIntentV2,
    budget: u128,
    policy: &DirectNovFeePolicy,
    case: &str,
) -> Result<(Memory, NodeHash)> {
    // Test-owned immutable state only. This does not infer authority from a key,
    // alter a real account, or provide a production initialization endpoint.
    let mut changes = fee_record_changes(policy, &FeeState::default())?;
    for (id, address, balance) in [
        ("uca:alice", vec![11; 20], 1000u128),
        (
            "uca:bob",
            vec![12; 32],
            if case == "business-failure-fee" {
                u128::MAX
            } else {
                7
            },
        ),
    ] {
        changes.extend(account_record_changes(
            7,
            &i.genesis,
            &account(id, address.clone()),
        )?);
        changes.push(StateChange::Put {
            key: balance_key(&Account::try_from(address).map_err(anyhow::Error::msg)?),
            value: balance.to_le_bytes().to_vec(),
        });
    }
    changes.extend(grant_record_changes(i, &grant(i, budget, case))?);
    changes.push(StateChange::Put {
        key: nonce_key(&i.nonce_identity()?),
        value: 0u64.to_le_bytes().to_vec(),
    });
    let staged = stage_state_update(&Memory::default(), empty_root(), &changes)?;
    Ok((Memory(staged.nodes().clone()), staged.root()))
}

fn grant(i: &AssetSpendIntentV2, budget: u128, case: &str) -> SuppliedAssetSpendGrantV2 {
    SuppliedAssetSpendGrantV2 {
        account: i.payer.clone(),
        asset: i.asset,
        key_ref: i.spend_key_ref,
        authorization_epoch: 3,
        permits_nov_transfer: true,
        revoked: case == "revoked",
        not_before_height: 1,
        expires_at_height: if case == "expired" { 10 } else { 30 },
        max_debit_per_intent: 100,
        remaining_debit_budget: budget,
    }
}

fn context(root: NodeHash, policy: &DirectNovFeePolicy) -> Result<BatchContext> {
    Ok(BatchContext {
        chain_id: 7,
        genesis_config_commitment: [2; 32],
        protocol_commitment: [9; 32],
        business_program: uca_program_id(),
        semantic_version: 1,
        effect_contract: uca_effect_contract(policy)?,
        parent_block_hash: [3; 32],
        parent_height: 9,
        parent_state_root: root,
        parent_receipt_root: empty_root(),
        parent_state_version: 9,
        receipt_codec: uca_receipt_codec(),
        height: 10,
        slot: 10,
        timestamp_unix_ms: 172_800_123,
    })
}

#[test]
fn asset_authority_metadata_is_bounded_before_prefix_allocation() -> Result<()> {
    use novovm_protocol::uca_spend::AssetAuthorityUpdateV2;
    let mut intent = AssetSpendIntentV2 {
        chain_id: 7,
        genesis: [2; 32],
        payer: vec![11; 20],
        recipient: vec![12; 32],
        asset: 1,
        authorization_epoch: 3,
        spend_key_ref: [4; 32],
        amount: 30,
        max_fee: 50,
        nonce: 0,
        not_before_height: 10,
        expires_at_height: 20,
        authority_update: None,
    };
    super::uca_effects::validate_authority_metadata(71, |_| &intent, 384 * 1024)?;
    assert!(super::uca_effects::validate_authority_metadata(1, |_| &intent, 1).is_err());
    intent.amount = 0;
    intent.recipient = intent.payer.clone();
    intent.authority_update = Some(AssetAuthorityUpdateV2 {
        use_recovery: false,
        next_epoch: 4,
        active: true,
        management_key_ref: [8; 32],
        recovery_key_ref: Some([9; 32]),
        grants: vec![],
    });
    assert!(
        super::uca_effects::validate_authority_metadata(1024, |_| &intent, 384 * 1024).is_err(),
        "bounded row count alone is not a bound on quadratic prefix material"
    );
    Ok(())
}

#[test]
#[ignore = "explicit packaged AOEM ML-DSA-65 and Vulkan; seeded UCA candidate, not active native spending"]
fn actual_mldsa_capture_uca_cpu_integer_atomic_effect_equivalence() -> Result<()> {
    run_actual_mldsa_effects(true)
}

#[test]
#[ignore = "explicit packaged AOEM ML-DSA-65; seeded UCA candidate, not active native spending"]
fn actual_mldsa_capture_uca_cpu_atomic_effects() -> Result<()> {
    run_actual_mldsa_effects(false)
}

fn run_actual_mldsa_effects(compare_integer: bool) -> Result<()> {
    let (platform, name) = if cfg!(target_os = "windows") {
        ("windows", "aoem_ffi.dll")
    } else if cfg!(target_os = "macos") {
        ("macos", "libaoem_ffi.dylib")
    } else {
        ("linux", "libaoem_ffi.so")
    };
    // Preserve packaged-library coverage by default. Isolated candidate gates
    // can name one explicit library, but fixture key generation and execution
    // must still load that SAME owner through normal manifest/capability checks.
    let path = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../aoem")
                .join(platform)
                .join("core/bin")
                .join(name)
        });
    ensure!(
        aoem_bindings::default_host_dll_path().canonicalize()? == path.canonicalize()?,
        "fixture generator and execution must use the same qualified AOEM"
    );
    let facade = AoemExecFacade::open(&path, AoemExecOpenOptions::default())?;
    let signer = facade.asset_spend_signer_v2()?;
    let verifier = facade.asset_spend_verifier_v2()?;
    let (public, mut secret) = aoem_bindings::mldsa_keygen_v1_auto(65)?
        .context("packaged ML-DSA-65 key generation unavailable")?;
    let (other_public, mut other_secret) = aoem_bindings::mldsa_keygen_v1_auto(65)?
        .context("second packaged ML-DSA-65 fixture key unavailable")?;
    let result = (|| -> Result<()> {
        let base = AssetSpendIntentV2 {
            chain_id: 7,
            genesis: [2; 32],
            payer: vec![11; 20],
            authorization_epoch: 3,
            spend_key_ref: spend_key_ref_v1(&public)?,
            recipient: vec![12; 32],
            asset: 1,
            amount: 30,
            max_fee: 50,
            nonce: 0,
            not_before_height: 10,
            expires_at_height: 20,
            authority_update: None,
        };
        let policy = super::body_tests::policy();
        let mut cpu_session = ComputeSession::open(&path, 4)?;
        let mut integer = if compare_integer {
            Some(ResidentTransferResources::new(&path, 4, 3)?)
        } else {
            None
        };
        for case in [
            "budget-prefix",
            "sufficient-budget",
            "stale-epoch",
            "zero-max-fee",
            "revoked",
            "expired",
            "nonce-replay",
            "business-failure-fee",
            "self-transfer-gross",
            "two-grants-shared-nonce",
        ] {
            let budget = if matches!(case, "budget-prefix" | "self-transfer-gross") {
                100
            } else {
                1000
            };
            let mut intents = vec![base.clone(), base.clone()];
            intents[1].nonce = 1;
            if case == "stale-epoch" {
                intents[0].authorization_epoch = 2;
                intents[1].nonce = 0; // Rejected authorization cannot consume it.
            }
            if case == "zero-max-fee" {
                intents.truncate(1);
                intents[0].max_fee = 0;
            }
            if case == "nonce-replay" {
                intents[1].nonce = 0;
                intents[1].amount = 31; // Distinct signed intent, not dedup rejection.
            }
            if case == "self-transfer-gross" {
                for i in &mut intents {
                    i.recipient = i.payer.clone();
                }
            }
            if case == "two-grants-shared-nonce" {
                intents[1].spend_key_ref = spend_key_ref_v1(&other_public)?;
                assert_ne!(intents[0].spend_key_ref, intents[1].spend_key_ref);
                assert_eq!(
                    intents[0].nonce_identity()?,
                    intents[1].nonce_identity()?,
                    "distinct spending keys must consume one stable UCA nonce"
                );
                let mut replay = intents[1].clone();
                replay.nonce = 0;
                replay.amount = 31; // Not an identical intent/dedup shortcut.
                intents.push(replay);
            }
            let recipient: Account = if case == "self-transfer-gross" {
                [11; 20].into()
            } else {
                [12; 32].into()
            };
            let (mut memory, mut root) = fixture(&base, budget, &policy, case)?;
            if case == "two-grants-shared-nonce" {
                // A second explicit seeded grant, not an authorization transaction.
                // Preserve the same UCA/binding/balance and the one existing nonce.
                let changes = grant_record_changes(&intents[1], &grant(&intents[1], 200, case))?;
                let staged = stage_state_update(&memory, root, &changes)?;
                root = staged.root();
                memory.0.extend(staged.nodes().clone());
            }
            let signed = intents
                .iter()
                .map(|i| {
                    if i.spend_key_ref == base.spend_key_ref {
                        signer.sign(i.clone(), &public, &secret)
                    } else {
                        signer.sign(i.clone(), &other_public, &other_secret)
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            let capture = || -> Result<NovTransferInput> {
                let checked = signed
                    .iter()
                    .cloned()
                    .map(|s| verifier.verify(s))
                    .collect::<Result<Vec<_>>>()?;
                NovTransferInput::capture_uca(
                    checked,
                    context(root, &policy)?,
                    policy.clone(),
                    &memory,
                    PlanBudget {
                        transactions: 3,
                        transaction_bytes: 8192,
                        body_bytes: 24576,
                        access_keys: 4096,
                    },
                    capture_budget(),
                )
            };
            let cpu = capture()?.execute(&mut cpu_session, Duration::from_secs(30))?;
            if let Some(integer) = &mut integer {
                let gpu = capture()?.execute_integer_plan(integer)?;
                assert_eq!(
                    cpu.spend_outputs(),
                    gpu.spend_outputs(),
                    "{case}: authorization/budget"
                );
                assert_eq!(
                    cpu.receipts(),
                    gpu.receipts(),
                    "{case}: ordered balances/nonce/fees/diagnostics"
                );
                assert_eq!(cpu.fees(), gpu.fees(), "{case}: complete fee state");
                assert_eq!(
                    cpu.effects().update().root(),
                    gpu.effects().update().root(),
                    "{case}: state root"
                );
                assert_eq!(
                    cpu.effects().update().nodes(),
                    gpu.effects().update().nodes(),
                    "{case}: staged bytes"
                );
                assert_eq!(
                    cpu.receipt_batch_commitment(),
                    gpu.receipt_batch_commitment(),
                    "{case}: receipts"
                );
                assert_eq!(
                    cpu.statement_commitment(),
                    gpu.statement_commitment(),
                    "{case}: candidate"
                );
                assert_eq!(
                    gpu.observation().gpu_waves,
                    1,
                    "one complete plan, not Host repair"
                );
                assert_eq!(gpu.observation().recomputed_transactions, 0);
            }

            let receipts = cpu.receipts();
            let outputs = cpu.spend_outputs().context("UCA outputs absent")?;
            let accepted: Vec<_> = receipts.iter().filter(|r| r.failure.is_none()).collect();
            match case {
                "budget-prefix" | "self-transfer-gross" => {
                    assert_eq!(accepted.len(), 1);
                    assert_eq!(outputs[1].failure, Some(SpendPolicyError::LimitExceeded));
                    assert_eq!(receipts[1].delta.nonce_before, 1);
                    assert_eq!(receipts[1].delta.nonce_after, 1);
                    assert_eq!(receipts[1].delta.fee_funding_delta, 0);
                    assert!(receipts[1].quote.is_none() && receipts[1].journal.is_none());
                }
                "sufficient-budget" => assert_eq!(accepted.len(), 2),
                "stale-epoch" => {
                    assert_eq!(outputs[0].failure, Some(SpendPolicyError::WrongEpoch));
                    assert_eq!(outputs[0].remaining, budget);
                    assert_eq!(receipts[0].delta.nonce_after, 0);
                    assert_eq!(receipts[0].delta.fee_funding_delta, 0);
                    assert_eq!(accepted.len(), 1);
                    assert!(receipts[1].failure.is_none());
                    assert_eq!(receipts[1].delta.nonce_after, 1);
                }
                "zero-max-fee" => {
                    assert!(accepted.is_empty(), "zero max fee is not unlimited");
                    assert!(receipts[0].fee_failure.is_some());
                    assert_eq!(receipts[0].delta.fee_funding_delta, 0);
                    assert_eq!(outputs[0].remaining, budget);
                }
                "revoked" | "expired" => {
                    let failure = if case == "revoked" {
                        SpendPolicyError::Revoked
                    } else {
                        SpendPolicyError::Expired
                    };
                    assert!(accepted.is_empty());
                    for (r, out) in receipts.iter().zip(outputs) {
                        assert_eq!(out.failure, Some(failure));
                        assert_eq!(out.remaining, budget);
                        assert_eq!(r.delta.nonce_before, 0);
                        assert_eq!(r.delta.nonce_after, 0);
                        assert_eq!(r.delta.fee_funding_delta, 0);
                        assert!(r.quote.is_none() && r.journal.is_none());
                    }
                    assert_eq!(
                        cpu.fees(),
                        &FeeState::default(),
                        "authorization rejection must not refresh fee windows or diagnostics"
                    );
                }
                "nonce-replay" => {
                    assert_eq!(accepted.len(), 1);
                    assert_eq!(outputs[1].failure, Some(SpendPolicyError::NonceMismatch));
                    assert_eq!(receipts[1].delta.nonce_before, 1);
                    assert_eq!(receipts[1].delta.nonce_after, 1);
                    assert_eq!(receipts[1].delta.fee_funding_delta, 0);
                    assert!(receipts[1].quote.is_none() && receipts[1].journal.is_none());
                }
                "business-failure-fee" => {
                    assert!(accepted.is_empty());
                    for (n, (r, out)) in receipts.iter().zip(outputs).enumerate() {
                        assert!(
                            out.failure.is_none(),
                            "authorized business failure is not authorization failure"
                        );
                        assert!(r.failure.is_some() && r.fee_failure.is_none());
                        assert!(r.delta.fee_funding_delta > 0);
                        assert_eq!(
                            r.delta.fee_funding_delta,
                            r.quote
                                .as_ref()
                                .context("business failure quote absent")?
                                .nov_amount
                        );
                        assert_eq!(r.delta.nonce_before, n as u64);
                        assert_eq!(r.delta.nonce_after, n as u64 + 1);
                        assert_eq!(r.delta.recipient.before, u128::MAX);
                        assert_eq!(r.delta.recipient.after, u128::MAX);
                    }
                }
                "two-grants-shared-nonce" => {
                    assert_eq!(accepted.len(), 2);
                    assert!(receipts[0].failure.is_none() && receipts[1].failure.is_none());
                    assert_eq!(receipts[0].delta.nonce_before, 0);
                    assert_eq!(receipts[0].delta.nonce_after, 1);
                    assert_eq!(receipts[1].delta.nonce_before, 1);
                    assert_eq!(receipts[1].delta.nonce_after, 2);
                    assert_eq!(outputs[0].failure, None);
                    assert_eq!(outputs[1].failure, None);
                    assert_eq!(
                        outputs[0].remaining,
                        budget - 30 - receipts[0].delta.fee_funding_delta
                    );
                    assert_eq!(
                        outputs[1].remaining,
                        200 - 30 - receipts[1].delta.fee_funding_delta
                    );
                    assert_eq!(outputs[2].failure, Some(SpendPolicyError::NonceMismatch));
                    assert_eq!(outputs[2].remaining, outputs[1].remaining);
                    assert_eq!(receipts[2].delta.nonce_before, 2);
                    assert_eq!(receipts[2].delta.nonce_after, 2);
                    assert_eq!(receipts[2].delta.fee_funding_delta, 0);
                    assert!(receipts[2].quote.is_none() && receipts[2].journal.is_none());
                }
                _ => unreachable!(),
            }
            let actual_fees: u128 = receipts.iter().map(|r| r.delta.fee_funding_delta).sum();
            for r in &accepted {
                assert_eq!(
                    r.delta.fee_funding_delta,
                    r.quote
                        .as_ref()
                        .context("successful quote absent")?
                        .nov_amount
                );
                assert!(r.delta.fee_funding_delta > 0 && r.delta.fee_funding_delta < 50);
            }
            let transferred: u128 = intents
                .iter()
                .zip(receipts)
                .filter(|(_, r)| r.failure.is_none())
                .map(|(i, _)| i.amount)
                .sum();
            let expected_budget = if case == "two-grants-shared-nonce" {
                budget - 30 - receipts[0].delta.fee_funding_delta
            } else {
                budget - transferred - actual_fees
            };
            if case != "two-grants-shared-nonce" {
                assert_eq!(outputs.last().unwrap().remaining, expected_budget);
            }
            // Read the staged tree, not only in-memory outputs: grant, nonce,
            // balances and fee effects must share the one resulting root.
            let mut after = Memory(memory.0.clone());
            after.0.extend(cpu.effects().update().nodes().clone());
            let mut read_intent = base.clone();
            if case == "self-transfer-gross" {
                read_intent.recipient = read_intent.payer.clone();
            }
            let access = AssetSpendStateAccessV2::new(read_intent, [11; 20].into(), recipient)?;
            let root = cpu.effects().update().root();
            let captured =
                OwnedStateInput::capture(&after, root, access.declarations(), capture_budget())?;
            let state = access.read(&captured, root)?;
            assert_eq!(
                state.grant().unwrap().remaining_debit_budget,
                expected_budget
            );
            let payer_before = 1000;
            let expected_balances = if case == "self-transfer-gross" {
                // Budget consumes gross transfer + fee, despite net money only paying the fee.
                assert_eq!(accepted.len(), 1);
                assert!(transferred > 0);
                (payer_before - actual_fees, payer_before - actual_fees)
            } else {
                let recipient_before = if case == "business-failure-fee" {
                    u128::MAX
                } else {
                    7
                };
                (
                    payer_before - transferred - actual_fees,
                    recipient_before + transferred,
                )
            };
            assert_eq!(state.balances(), expected_balances, "{case}: staged money");
            assert_eq!(
                state.next_nonce(),
                receipts.last().unwrap().delta.nonce_after
            );
            if case == "two-grants-shared-nonce" {
                let second_access = AssetSpendStateAccessV2::new(
                    intents[1].clone(),
                    [11; 20].into(),
                    [12; 32].into(),
                )?;
                let second_capture = OwnedStateInput::capture(
                    &after,
                    root,
                    second_access.declarations(),
                    capture_budget(),
                )?;
                let second_state = second_access.read(&second_capture, root)?;
                assert_eq!(
                    second_state.grant().unwrap().key_ref,
                    intents[1].spend_key_ref
                );
                assert_eq!(
                    second_state.grant().unwrap().remaining_debit_budget,
                    200 - 30 - receipts[1].delta.fee_funding_delta
                );
                assert_eq!(second_state.next_nonce(), 2);
                assert_eq!(second_state.next_nonce(), state.next_nonce());
                assert_eq!(second_state.balances(), state.balances());
            }
        }
        run_asset_management_cases(&path, &facade, compare_integer)?;
        Ok(())
    })();
    secret.fill(0); // Ephemeral test key, including ordinary error returns.
    other_secret.fill(0);
    result
}

fn run_asset_management_cases(
    path: &std::path::Path,
    facade: &AoemExecFacade,
    compare_integer: bool,
) -> Result<()> {
    use novovm_protocol::uca_spend::{AssetAuthorityUpdateV2, AssetGrantConfigV2};
    let signer = facade.asset_spend_signer_v2()?;
    let verifier = facade.asset_spend_verifier_v2()?;
    let mut keys = (0..4)
        .map(|_| {
            aoem_bindings::mldsa_keygen_v1_auto(65)?
                .context("asset lifecycle fixture key unavailable")
        })
        .collect::<Result<Vec<_>>>()?;
    let result = (|| -> Result<()> {
        let refs = keys
            .iter()
            .map(|(pk, _)| spend_key_ref_v1(pk))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let base = AssetSpendIntentV2 {
            chain_id: 7,
            genesis: [2; 32],
            payer: vec![11; 20],
            recipient: vec![12; 32],
            asset: 1,
            authorization_epoch: 3,
            spend_key_ref: refs[0],
            amount: 30,
            max_fee: 50,
            nonce: 0,
            not_before_height: 10,
            expires_at_height: 20,
            authority_update: None,
        };
        let policy = super::body_tests::policy();
        let mut session = ComputeSession::open(path, 4)?;
        let mut integer = if compare_integer {
            Some(ResidentTransferResources::new(path, 4, 8)?)
        } else {
            None
        };
        for case in [
            "rotate",
            "freeze",
            "recovery",
            "wrong-manager",
            "fee-failure",
            "missing-recovery",
            "manager-rotation",
            "same-key-reset",
            "same-key-reset-failure",
            "failed-create-then-omit",
        ] {
            let (mut memory, mut root) = fixture(&base, 1000, &policy, case)?;
            let mut authority = account("uca:alice", base.payer.clone());
            authority.management_key_ref = refs[2];
            authority.recovery_key_ref = if case == "missing-recovery" {
                None
            } else {
                Some(refs[3])
            };
            authority.active = case != "recovery";
            let update = stage_state_update(
                &memory,
                root,
                &account_record_changes(7, &base.genesis, &authority)?,
            )?;
            root = update.root();
            memory.0.extend(update.nodes().clone());
            let mut management = base.clone();
            management.recipient = management.payer.clone();
            management.amount = 0;
            management.spend_key_ref =
                if matches!(case, "wrong-manager" | "failed-create-then-omit") {
                    refs[0]
                } else if matches!(case, "recovery" | "missing-recovery") {
                    refs[3]
                } else {
                    refs[2]
                };
            if matches!(case, "fee-failure" | "same-key-reset-failure") {
                management.max_fee = 0;
            }
            management.authority_update = Some(AssetAuthorityUpdateV2 {
                use_recovery: matches!(case, "recovery" | "missing-recovery"),
                next_epoch: 4,
                active: case != "freeze",
                management_key_ref: if case == "manager-rotation" {
                    refs[0]
                } else {
                    refs[2]
                },
                recovery_key_ref: Some(refs[3]),
                grants: if case == "freeze" {
                    vec![]
                } else {
                    vec![AssetGrantConfigV2 {
                        key_ref: if case.starts_with("same-key-reset") {
                            refs[0]
                        } else {
                            refs[1]
                        },
                        permits_nov_transfer: true,
                        not_before_height: 1,
                        expires_at_height: 30,
                        max_debit_per_intent: 100,
                        remaining_debit_budget: 200,
                    }]
                },
            });
            let successful_update = !matches!(
                case,
                "wrong-manager"
                    | "fee-failure"
                    | "missing-recovery"
                    | "same-key-reset-failure"
                    | "failed-create-then-omit"
            );
            let mut old_spend = base.clone();
            old_spend.nonce = if matches!(case, "wrong-manager" | "missing-recovery") {
                0
            } else {
                1
            };
            let mut new_spend = base.clone();
            new_spend.spend_key_ref = if case.starts_with("same-key-reset") {
                refs[0]
            } else {
                refs[1]
            };
            new_spend.authorization_epoch = 4;
            new_spend.nonce = if successful_update {
                1
            } else {
                old_spend.nonce + 1
            };
            let mut intents = vec![management, old_spend, new_spend];
            if case == "failed-create-then-omit" {
                let mut omit = intents[0].clone();
                omit.spend_key_ref = refs[2];
                omit.authority_update.as_mut().unwrap().grants.clear();
                intents[1] = omit;
                intents[2].nonce = 1;
            }
            if case == "manager-rotation" {
                let mut obsolete_manager = intents[0].clone();
                obsolete_manager.authorization_epoch = 4;
                obsolete_manager.nonce = 2;
                obsolete_manager
                    .authority_update
                    .as_mut()
                    .unwrap()
                    .next_epoch = 5;
                intents.push(obsolete_manager.clone());
                obsolete_manager.spend_key_ref = refs[0];
                intents.push(obsolete_manager);
            }
            let signed = intents
                .iter()
                .map(|intent| {
                    let key = refs
                        .iter()
                        .position(|key| *key == intent.spend_key_ref)
                        .context("fixture signer missing")?;
                    signer.sign(intent.clone(), &keys[key].0, &keys[key].1)
                })
                .collect::<Result<Vec<_>>>()?;
            let capture = || -> Result<NovTransferInput> {
                NovTransferInput::capture_uca(
                    signed
                        .iter()
                        .cloned()
                        .map(|tx| verifier.verify(tx))
                        .collect::<Result<Vec<_>>>()?,
                    context(root, &policy)?,
                    policy.clone(),
                    &memory,
                    PlanBudget {
                        transactions: 8,
                        transaction_bytes: 8192,
                        body_bytes: 65536,
                        access_keys: 4096,
                    },
                    capture_budget(),
                )
            };
            let cpu = capture()?.execute(&mut session, Duration::from_secs(30))?;
            if let Some(integer) = &mut integer {
                let gpu = capture()?.execute_integer_plan(integer)?;
                assert_eq!(
                    cpu.receipts(),
                    gpu.receipts(),
                    "{case}: all ordered money/fee/nonce effects"
                );
                assert_eq!(
                    cpu.spend_outputs(),
                    gpu.spend_outputs(),
                    "{case}: authority effects"
                );
                assert_eq!(cpu.fees(), gpu.fees(), "{case}: fee state");
                assert_eq!(
                    cpu.effects().update().root(),
                    gpu.effects().update().root(),
                    "{case}: atomic authority+money root"
                );
                assert_eq!(
                    cpu.effects().update().nodes(),
                    gpu.effects().update().nodes(),
                    "{case}: atomic write set"
                );
                assert_eq!(
                    cpu.receipt_batch_commitment(),
                    gpu.receipt_batch_commitment()
                );
                assert_eq!(gpu.observation().gpu_waves, 1);
                assert_eq!(gpu.observation().recomputed_transactions, 0);
            }
            let receipts = cpu.receipts();
            let outputs = cpu.spend_outputs().unwrap();
            assert_eq!(
                outputs[0].remaining, 0,
                "management never consumes ordinary spend budget"
            );
            if case == "failed-create-then-omit" {
                assert_eq!(outputs[0].failure, Some(SpendPolicyError::WrongKey));
                assert!(receipts[1].failure.is_none());
                assert_eq!(
                    outputs[2].failure,
                    Some(SpendPolicyError::StateUnavailable),
                    "a failed create must not turn an absent key into a stale existing grant"
                );
                assert_eq!(receipts[2].delta.nonce_after, 1);
                let mut after = Memory(memory.0.clone());
                after.0.extend(cpu.effects().update().nodes().clone());
                let root = cpu.effects().update().root();
                let read = AssetSpendStateAccessV2::new(
                    intents[2].clone(),
                    [11; 20].into(),
                    [12; 32].into(),
                )?;
                let captured =
                    OwnedStateInput::capture(&after, root, read.declarations(), capture_budget())?;
                let state = read.read(&captured, root)?;
                assert!(state.grant().is_none());
                assert_eq!(state.static_failure(10), outputs[2].failure);
                continue;
            }
            if successful_update {
                assert!(receipts[0].failure.is_none(), "{case}");
                assert!(
                    receipts[1].failure.is_some(),
                    "old key cannot inherit replacement authority"
                );
                assert_eq!(receipts[1].delta.fee_funding_delta, 0);
                assert_eq!(receipts[1].delta.nonce_after, 1);
                if case == "freeze" {
                    assert!(receipts[2].failure.is_some());
                    assert_eq!(receipts[2].delta.nonce_after, 1);
                } else {
                    assert!(
                        receipts[2].failure.is_none(),
                        "{case}: newly authorized key after exact prefix"
                    );
                    assert_eq!(receipts[2].delta.nonce_after, 2);
                }
            } else {
                assert!(receipts[0].failure.is_some());
                assert!(
                    receipts[1].failure.is_none(),
                    "failed update cannot revoke old authority"
                );
                assert!(
                    receipts[2].failure.is_some(),
                    "failed update cannot grant new authority"
                );
                if !matches!(case, "fee-failure" | "same-key-reset-failure") {
                    assert_eq!(outputs[0].failure, Some(SpendPolicyError::WrongKey));
                    assert_eq!(receipts[0].delta.fee_funding_delta, 0);
                    assert_eq!(receipts[0].delta.nonce_after, 0);
                }
            }
            if case == "manager-rotation" {
                assert_eq!(outputs[3].failure, Some(SpendPolicyError::WrongKey));
                assert_eq!(receipts[3].delta.nonce_after, 2);
                assert!(receipts[4].failure.is_none());
                assert_eq!(receipts[4].delta.nonce_after, 3);
            }
            let mut after = Memory(memory.0.clone());
            after.0.extend(cpu.effects().update().nodes().clone());
            let read =
                AssetSpendStateAccessV2::new(base.clone(), [11; 20].into(), [12; 32].into())?;
            let root = cpu.effects().update().root();
            let captured =
                OwnedStateInput::capture(&after, root, read.declarations(), capture_budget())?;
            let state = read.read(&captured, root)?;
            assert_eq!(
                state.payer().current_spend_epoch,
                if case == "manager-rotation" {
                    5
                } else if successful_update {
                    4
                } else {
                    3
                }
            );
            assert_eq!(state.payer().active, case != "freeze");
            assert_eq!(
                state.next_nonce(),
                receipts.last().unwrap().delta.nonce_after
            );
            let fees: u128 = receipts.iter().map(|r| r.delta.fee_funding_delta).sum();
            let paid: u128 = intents
                .iter()
                .zip(receipts)
                .filter(|(_, r)| r.failure.is_none())
                .map(|(i, _)| i.amount)
                .sum();
            assert_eq!(state.balances(), (1000 - fees - paid, 7 + paid));
            let ordinary_debit = if successful_update {
                0
            } else {
                receipts[1].delta.fee_funding_delta + base.amount
            };
            let expected_budget = if case == "same-key-reset" {
                200 - base.amount - receipts[2].delta.fee_funding_delta
            } else {
                1000 - ordinary_debit
            };
            assert_eq!(
                state.grant().unwrap().remaining_debit_budget,
                expected_budget,
                "management resets only on success; management fees never consume spend budget"
            );
            if case == "same-key-reset" {
                assert_eq!(outputs[2].remaining, expected_budget);
                assert!(expected_budget < 200);
            } else if case == "same-key-reset-failure" {
                assert_eq!(outputs[1].remaining, expected_budget);
                assert_eq!(outputs[2].remaining, expected_budget);
                assert!(
                    expected_budget > 200,
                    "failed reset preserves previous budget"
                );
            }
            if successful_update {
                assert_eq!(
                    state.static_failure(10),
                    outputs[1].failure,
                    "same-batch stale-key refusal matches reading its durable poststate"
                );
            }
        }
        Ok(())
    })();
    for (_, secret) in &mut keys {
        secret.fill(0);
    }
    result
}
