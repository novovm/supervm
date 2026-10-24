//! Explicit seeded-tree integration, not a genesis authorization, active ingress,
//! durable publication, mobile custody, or mainchain performance qualification.
use super::*;
use crate::native_pipeline::business::uca_spend_state::{
    account_record_changes, grant_record_changes, UcaSpendAccountStateV1, UcaSpendStateAccessV1,
};
use crate::native_pipeline::state::frontier::OwnedStateInput;
use crate::native_pipeline::state::tree::{empty_root, stage_state_update};
use novovm_adapter_api::uca_spend_policy::{SpendPolicyError, SuppliedSpendGrantV1};
use novovm_adapter_api::unified_account::{
    BindingState, PersonaBinding, PersonaType, UcaAccount, UcaStatus,
};
use novovm_exec::resident::ComputeSession;
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions};
use novovm_protocol::uca_spend::{spend_key_ref_v1, UcaSpendIntentV1};
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

fn account(id: &str, address: Vec<u8>) -> UcaSpendAccountStateV1 {
    UcaSpendAccountStateV1 {
        account: UcaAccount {
            uca_id: id.into(),
            primary_key_ref: vec![8; 32],
            primary_key_binding: None,
            status: UcaStatus::Active,
            created_at: 1,
            updated_at: 2,
        },
        binding: PersonaBinding {
            uca_id: id.into(),
            persona_type: PersonaType::Web30,
            chain_id: 7,
            external_address: address,
            binding_state: BindingState::Bound,
            bound_at: 1,
            revoked_at: None,
            cooldown_until: None,
        },
        current_spend_epoch: 3,
    }
}

fn fixture(
    i: &UcaSpendIntentV1,
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

fn grant(i: &UcaSpendIntentV1, budget: u128, case: &str) -> SuppliedSpendGrantV1 {
    SuppliedSpendGrantV1 {
        account_id: i.account_id.clone(),
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
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../aoem")
        .join(platform)
        .join("core/bin")
        .join(name);
    ensure!(
        aoem_bindings::default_host_dll_path().canonicalize()? == path.canonicalize()?,
        "fixture generator and execution must use the same packaged AOEM"
    );
    let facade = AoemExecFacade::open(&path, AoemExecOpenOptions::default())?;
    let signer = facade.uca_spend_signer_v1()?;
    let verifier = facade.uca_spend_verifier_v1()?;
    let (public, mut secret) = aoem_bindings::mldsa_keygen_v1_auto(65)?
        .context("packaged ML-DSA-65 key generation unavailable")?;
    let (other_public, mut other_secret) = aoem_bindings::mldsa_keygen_v1_auto(65)?
        .context("second packaged ML-DSA-65 fixture key unavailable")?;
    let result = (|| -> Result<()> {
        let base = UcaSpendIntentV1 {
            chain_id: 7,
            genesis: [2; 32],
            account_id: "uca:alice".into(),
            authorization_epoch: 3,
            spend_key_ref: spend_key_ref_v1(&public)?,
            recipient_uca: "uca:bob".into(),
            amount: 30,
            max_fee: 50,
            nonce: 0,
            not_before_height: 10,
            expires_at_height: 20,
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
                    i.recipient_uca = i.account_id.clone();
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
                    vec![([11; 20].into(), recipient.clone()); intents.len()],
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
                read_intent.recipient_uca = read_intent.account_id.clone();
            }
            let access = UcaSpendStateAccessV1::new(read_intent, [11; 20].into(), recipient)?;
            let root = cpu.effects().update().root();
            let captured =
                OwnedStateInput::capture(&after, root, access.declarations(), capture_budget())?;
            let state = access.read(&captured, root)?;
            assert_eq!(state.grant().remaining_debit_budget, expected_budget);
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
                let second_access = UcaSpendStateAccessV1::new(
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
                assert_eq!(second_state.grant().key_ref, intents[1].spend_key_ref);
                assert_eq!(
                    second_state.grant().remaining_debit_budget,
                    200 - 30 - receipts[1].delta.fee_funding_delta
                );
                assert_eq!(second_state.next_nonce(), 2);
                assert_eq!(second_state.next_nonce(), state.next_nonce());
                assert_eq!(second_state.balances(), state.balances());
            }
        }
        Ok(())
    })();
    secret.fill(0); // Ephemeral test key, including ordinary error returns.
    other_secret.fill(0);
    result
}
