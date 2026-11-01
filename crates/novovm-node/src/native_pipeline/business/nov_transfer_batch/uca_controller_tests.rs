//! Real signed controller payments and optional delegation on the original plan.
use super::creation_tests::{read_state, SignedAssetRunner};
use super::*;
use novovm_protocol::uca_spend::{AssetAccountCreationV2, AssetAuthorityUpdateV2};

#[test]
#[ignore = "explicit qualified AOEM ML-DSA-65; real controller/delegated seeded-tree effects"]
fn actual_mldsa_controller_cpu_atomic_effects() -> Result<()> {
    let (path, facade) = actual_mldsa_facade()?;
    run_asset_controller_cases(&path, &facade, false)
}

#[test]
#[ignore = "explicit qualified AOEM ML-DSA-65 and Vulkan; controller/delegated full-plan equivalence"]
fn actual_mldsa_controller_cpu_integer_atomic_effect_equivalence() -> Result<()> {
    let (path, facade) = actual_mldsa_facade()?;
    run_asset_controller_cases(&path, &facade, true)
}

fn run_asset_controller_cases(
    path: &std::path::Path,
    facade: &AoemExecFacade,
    compare_integer: bool,
) -> Result<()> {
    let mut keys = (0..5)
        .map(|_| {
            aoem_bindings::mldsa_keygen_v1_auto(65)?
                .context("controller fixture ML-DSA key unavailable")
        })
        .collect::<Result<Vec<_>>>()?;
    let result = (|| -> Result<()> {
        let refs = keys
            .iter()
            .map(|(public, _)| spend_key_ref_v1(public))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let base = AssetSpendIntentV3 {
            authority: AssetSpendAuthorityV3::Controller,
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
            recipient_creation: None,
        };
        let mut delegated = base.clone();
        delegated.authority = AssetSpendAuthorityV3::DelegatedGrant;
        delegated.spend_key_ref = refs[2];
        let policy = super::super::body_tests::policy();
        let mut runner = SignedAssetRunner {
            facade,
            session: ComputeSession::open(path, 4)?,
            integer: if compare_integer {
                Some(ResidentTransferResources::new(path, 4, 8)?)
            } else {
                None
            },
            policy: policy.clone(),
        };
        for case in [
            "owner-no-grant",
            "owner-preserves-grant",
            "owner-and-delegated",
            "delegate-cannot-control",
            "recovery-cannot-control",
            "foreign-controller",
            "missing-delegation",
            "limited-delegation",
            "revoked-delegation",
            "rotation-success",
            "rotation-fee-failure",
            "rotation-rejected",
            "business-failure",
        ] {
            let has_grant = matches!(
                case,
                "owner-preserves-grant"
                    | "owner-and-delegated"
                    | "delegate-cannot-control"
                    | "limited-delegation"
                    | "revoked-delegation"
            );
            let recipient_balance = if case == "business-failure" {
                u128::MAX
            } else {
                7
            };
            let mut changes = fee_record_changes(&policy, &FeeState::default())?;
            for (address, control, recovery, balance) in [
                (base.payer.clone(), refs[0], Some(refs[3]), 1000u128),
                (base.recipient.clone(), refs[4], None, recipient_balance),
            ] {
                let authority = AssetAccountAuthorityV2 {
                    account: address.clone(),
                    asset: 1,
                    active: true,
                    current_spend_epoch: 3,
                    management_key_ref: control,
                    recovery_key_ref: recovery,
                };
                changes.extend(account_record_changes(7, &base.genesis, &authority)?);
                let mut location = base.clone();
                location.payer = address.clone();
                changes.push(StateChange::Put {
                    key: balance_key(&Account::try_from(address).map_err(anyhow::Error::msg)?),
                    value: balance.to_le_bytes().to_vec(),
                });
                changes.push(StateChange::Put {
                    key: nonce_key(&location.nonce_identity()?),
                    value: 0u64.to_le_bytes().to_vec(),
                });
            }
            let mut initial_grant = grant(&delegated, 1000, "controller-optional");
            if case == "limited-delegation" {
                initial_grant.remaining_debit_budget = 50;
            }
            initial_grant.revoked = case == "revoked-delegation";
            if has_grant {
                changes.extend(grant_record_changes(&delegated, &initial_grant)?);
            }
            let staged = stage_state_update(&Memory::default(), empty_root(), &changes)?;
            let mut memory = Memory(staged.nodes().clone());
            let mut root = staged.root();
            let mut second_owner = base.clone();
            second_owner.nonce = 1;
            let mut intents = vec![base.clone(), second_owner];
            match case {
                "owner-and-delegated" => {
                    intents[1] = delegated.clone();
                    intents[1].nonce = 1;
                }
                "delegate-cannot-control" | "recovery-cannot-control" | "foreign-controller" => {
                    intents[0].spend_key_ref = refs[match case {
                        "delegate-cannot-control" => 2,
                        "recovery-cannot-control" => 3,
                        _ => 4,
                    }];
                    intents[1].nonce = 0;
                }
                "missing-delegation" | "limited-delegation" | "revoked-delegation" => {
                    intents[0] = delegated.clone();
                    intents[1].nonce = 0;
                }
                "rotation-success" | "rotation-fee-failure" | "rotation-rejected" => {
                    let mut update = base.clone();
                    update.amount = 0;
                    update.recipient = update.payer.clone();
                    update.authority_update = Some(AssetAuthorityUpdateV2 {
                        use_recovery: false,
                        next_epoch: 4,
                        active: true,
                        management_key_ref: refs[1],
                        recovery_key_ref: Some(refs[3]),
                        grants: vec![],
                    });
                    let mut next_owner = base.clone();
                    next_owner.spend_key_ref = refs[1];
                    next_owner.authorization_epoch = 4;
                    if case == "rotation-success" {
                        let mut premature = base.clone();
                        premature.spend_key_ref = refs[1];
                        let mut obsolete = base.clone();
                        obsolete.authorization_epoch = 4;
                        obsolete.nonce = 1;
                        next_owner.nonce = 1;
                        intents = vec![premature, update, obsolete, next_owner];
                    } else {
                        let mut old_owner = base.clone();
                        if case == "rotation-fee-failure" {
                            update.max_fee = 0;
                            old_owner.nonce = 1;
                            next_owner.nonce = 2;
                        } else {
                            update.spend_key_ref = refs[1];
                            next_owner.nonce = 1;
                        }
                        intents = vec![update, old_owner, next_owner];
                    }
                }
                _ => {}
            }
            let executed = runner.run(&mut memory, &mut root, &intents, &keys)?;
            let receipts = executed.receipts();
            let outputs = executed.spend_outputs().unwrap();
            for (intent, output) in intents.iter().zip(outputs) {
                if !intent.uses_grant() {
                    assert_eq!(
                        output.remaining, 0,
                        "{case}: signed controller budget sentinel"
                    );
                }
            }
            match case {
                "delegate-cannot-control" | "recovery-cannot-control" | "foreign-controller" => {
                    assert_eq!(
                        outputs[0].failure,
                        Some(SpendPolicyError::WrongKey),
                        "{case}"
                    );
                    assert_eq!(receipts[0].delta.fee_funding_delta, 0);
                    assert_eq!(receipts[0].delta.nonce_after, 0);
                    assert!(receipts[1].failure.is_none(), "{case}: genuine controller");
                }
                "missing-delegation" | "limited-delegation" | "revoked-delegation" => {
                    let failure = match case {
                        "missing-delegation" => SpendPolicyError::StateUnavailable,
                        "limited-delegation" => SpendPolicyError::LimitExceeded,
                        _ => SpendPolicyError::Revoked,
                    };
                    assert_eq!(outputs[0].failure, Some(failure));
                    assert_eq!(receipts[0].delta.fee_funding_delta, 0);
                    assert_eq!(receipts[0].delta.nonce_after, 0);
                    assert!(
                        receipts[1].failure.is_none(),
                        "owner is not limited by delegation"
                    );
                }
                "rotation-success" => {
                    assert_eq!(outputs[0].failure, Some(SpendPolicyError::WrongKey));
                    assert!(receipts[1].failure.is_none());
                    assert_eq!(outputs[2].failure, Some(SpendPolicyError::WrongKey));
                    assert_eq!(receipts[2].delta.fee_funding_delta, 0);
                    assert_eq!(receipts[2].delta.nonce_after, 1);
                    assert!(receipts[3].failure.is_none());
                    assert_eq!(receipts[3].delta.nonce_after, 2);
                }
                "rotation-fee-failure" | "rotation-rejected" => {
                    assert!(receipts[0].failure.is_some());
                    assert!(
                        receipts[1].failure.is_none(),
                        "failed rotation preserves old controller"
                    );
                    assert_eq!(outputs[2].failure, Some(SpendPolicyError::WrongKey));
                    assert_eq!(receipts[2].delta.fee_funding_delta, 0);
                    if case == "rotation-rejected" {
                        assert_eq!(outputs[0].failure, Some(SpendPolicyError::WrongKey));
                        assert_eq!(receipts[0].delta.nonce_after, 0);
                    } else {
                        assert_eq!(
                            outputs[0].failure, None,
                            "authorized but fee-rejected rotation"
                        );
                        assert_eq!(receipts[0].delta.nonce_after, 1);
                    }
                }
                "business-failure" => {
                    assert!(receipts.iter().all(|receipt| receipt.failure.is_some()));
                    assert!(outputs.iter().all(|output| output.failure.is_none()));
                    assert!(receipts
                        .iter()
                        .all(|receipt| receipt.delta.fee_funding_delta > 0));
                    assert_eq!(receipts[1].delta.nonce_after, 2);
                }
                _ => assert!(
                    receipts.iter().all(|receipt| receipt.failure.is_none()),
                    "{case}"
                ),
            }
            let state = read_state(&memory, root, &base)?;
            assert!(
                state.grant().is_none(),
                "controller view does not load a grant"
            );
            let mut controller_grant_probe = base.clone();
            controller_grant_probe.authority = AssetSpendAuthorityV3::DelegatedGrant;
            assert!(
                read_state(&memory, root, &controller_grant_probe)?
                    .grant()
                    .is_none(),
                "controller payment must not create a synthetic grant record"
            );
            assert_eq!(
                state.payer().management_key_ref,
                refs[usize::from(case == "rotation-success")]
            );
            assert_eq!(
                state.payer().current_spend_epoch,
                if case == "rotation-success" { 4 } else { 3 }
            );
            assert_eq!(
                state.next_nonce(),
                receipts.last().unwrap().delta.nonce_after
            );
            let fees: u128 = receipts
                .iter()
                .map(|receipt| receipt.delta.fee_funding_delta)
                .sum();
            let paid: u128 = intents
                .iter()
                .zip(receipts)
                .filter(|(_, receipt)| receipt.failure.is_none())
                .map(|(intent, _)| intent.amount)
                .sum();
            assert_eq!(
                state.balances(),
                (1000 - paid - fees, recipient_balance + paid),
                "{case}"
            );
            let delegate_state = read_state(&memory, root, &delegated)?;
            if has_grant {
                let mut expected = initial_grant.clone();
                if case == "owner-and-delegated" {
                    expected.remaining_debit_budget -=
                        delegated.amount + receipts[1].delta.fee_funding_delta;
                    assert_eq!(outputs[1].remaining, expected.remaining_debit_budget);
                }
                assert_eq!(
                    delegate_state.grant(),
                    Some(&expected),
                    "{case}: only delegated debit mutates grant"
                );
            } else {
                assert!(delegate_state.grant().is_none());
            }
        }

        // A funded owner-only account can spend its income without creating a
        // delegation, using the same opening, balance, nonce and fee pipeline.
        let (mut memory, mut root) = fixture(&delegated, 1000, &policy, "owner-opening")?;
        let authority = AssetAccountAuthorityV2 {
            account: base.payer.clone(),
            asset: 1,
            active: true,
            current_spend_epoch: 3,
            management_key_ref: refs[0],
            recovery_key_ref: Some(refs[3]),
        };
        let staged = stage_state_update(
            &memory,
            root,
            &account_record_changes(7, &base.genesis, &authority)?,
        )?;
        root = staged.root();
        memory.0.extend(staged.nodes().clone());
        let descriptor = AssetAccountCreationV2 {
            management_key_ref: refs[1],
            recovery_key_ref: None,
            grants: vec![],
        };
        let mut opening = base.clone();
        opening.recipient = descriptor.account(7, &base.genesis)?.to_vec();
        opening.recipient_creation = Some(descriptor);
        opening.amount = 200;
        let created = runner.run(&mut memory, &mut root, &[opening.clone()], &keys)?;
        assert!(created.receipts()[0].failure.is_none());
        let mut spend = base.clone();
        spend.payer = opening.recipient;
        spend.authorization_epoch = 1;
        spend.spend_key_ref = refs[1];
        spend.amount = 20;
        let paid = runner.run(&mut memory, &mut root, &[spend.clone()], &keys)?;
        assert!(paid.receipts()[0].failure.is_none());
        assert_eq!(paid.spend_outputs().unwrap()[0].remaining, 0);
        let state = read_state(&memory, root, &spend)?;
        assert!(state.grant().is_none());
        let mut controller_grant_probe = spend.clone();
        controller_grant_probe.authority = AssetSpendAuthorityV3::DelegatedGrant;
        assert!(read_state(&memory, root, &controller_grant_probe)?
            .grant()
            .is_none());
        assert_eq!(state.next_nonce(), 1);
        assert_eq!(
            state.balances(),
            (200 - 20 - paid.receipts()[0].delta.fee_funding_delta, 27)
        );
        Ok(())
    })();
    for (_, secret) in &mut keys {
        secret.fill(0);
    }
    result
}
